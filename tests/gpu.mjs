import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { pathToFileURL } from "node:url";
import { createWebGpu } from "../src/runtime/webgpu.mjs";

const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const clang = process.env.TSUZURI_CLANG ?? "clang";
const directory = mkdtempSync(join(tmpdir(), "tsuzuri-gpu-"));
const quick = process.argv.includes("--quick");
const benchmark = process.argv.includes("--benchmark");
const useGpu = process.env.TSUZURI_WEBGPU === "1" && !quick;
const rows = [];
let runtime;
let provider;
function execute(program, args, success = true) {
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 120000, maxBuffer: 8 * 1024 * 1024 });
  assert.ifError(result.error);
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}
const cli = (args, success) => execute(compiler, args, success);
const bits = value => Number(BigInt.asUintN(32, BigInt(value)));
const signed = value => BigInt.asIntN(32, BigInt(value));
const kernels = [
  { name: "mix", body: "(value * 1664525 + 1013904223) ^ (value >>> 13)", reference: value => bits((signed(value) * 1664525n + 1013904223n) ^ (BigInt(bits(value)) >> 13n)) },
  { name: "cast", body: "((value as i32u) + 4294967295i32u) as i32", reference: value => bits(BigInt(value) - 1n) },
  { name: "locals", body: "{ let mut current = value; let before = current; current = current + 1; if before < 0 then -current else current ^ before }", reference: value => bits(value < 0 ? -signed(signed(value) + 1n) : signed(signed(value) + 1n) ^ signed(value)) },
];
const inputs = [-2147483648, 2147483647, -1, 0, 1];
let seed = 0x12345678;
for (let index = 0; index < 256; index++) { seed = (Math.imul(seed, 1664525) + 1013904223) | 0; inputs.push(seed); }
try {
  await assert.rejects(createWebGpu(null), /unavailable/);
  let startupMs;
  if (useGpu) {
    const binding = await import(process.env.TSUZURI_WEBGPU_MODULE ?? pathToFileURL(resolve("target/webgpu-runtime/node_modules/webgpu/index.js")).href);
    Object.assign(globalThis, binding.globals);
    provider = binding.create(process.platform === "darwin" ? ["backend=metal"] : []);
    const start = performance.now();
    runtime = await createWebGpu(provider);
    startupMs = performance.now() - start;
  }
  for (const kernel of kernels) {
    const project = join(directory, kernel.name);
    mkdirSync(project);
    const source = join(project, "Main.tz");
    writeFileSync(source, `export def kernel :: i32 -> i32\nfn kernel value = ${kernel.body}\n`);
    const wgsl = join(project, "kernel.wgsl");
    const repeated = join(project, "repeated.wgsl");
    cli(["build", source, "--emit", "wgsl", "-o", wgsl]);
    cli(["build", source, "--emit", "wgsl", "-o", repeated]);
    assert.deepEqual(readFileSync(wgsl), readFileSync(repeated));
    const expected = inputs.map(kernel.reference);
    const host = join(project, "host.c");
    writeFileSync(host, `#include <stdint.h>\n#include <stdio.h>\nextern int32_t tz_kernel(int32_t);\nint main(void) { int32_t inputs[] = {${inputs.join(",")}}; uint32_t expected[] = {${expected.map(value => `${value}u`).join(",")}}; for (unsigned index = 0; index < ${inputs.length}; index++) if ((uint32_t)tz_kernel(inputs[index]) != expected[index]) { fprintf(stderr, "case %u\\n", index); return 1; } return 0; }\n`);
    let instance;
    for (const optimization of [0, 3]) {
      const object = join(project, `kernel-${optimization}.o`);
      const wasm = join(project, `kernel-${optimization}.wasm`);
      const native = join(project, `host-${optimization}${process.platform === "win32" ? ".exe" : ""}`);
      cli(["build", source, "--emit", "object", `-O${optimization}`, "-o", object]);
      execute(clang, [host, object, "-o", native]);
      execute(native, []);
      cli(["build", source, "--target", "wasm32", `-O${optimization}`, "-o", wasm]);
      const module = new WebAssembly.Module(readFileSync(wasm));
      assert.deepEqual(WebAssembly.Module.imports(module), []);
      instance = new WebAssembly.Instance(module);
      inputs.forEach((value, index) => assert.equal(instance.exports.tz_kernel(value) >>> 0, expected[index]));
    }
    const start = performance.now();
    for (let repeat = 0; repeat < (quick ? 1 : 30); repeat++) for (const value of inputs) instance.exports.tz_kernel(value);
    const row = { kernel: kernel.name, count: inputs.length, cpu_wasm_scalar_calls_ms: performance.now() - start, gpu: "not requested" };
    if (runtime) {
      const compileStart = performance.now();
      const program = await runtime.prepare(readFileSync(wgsl, "utf8"));
      row.pipeline_creation_ms = performance.now() - compileStart;
      const transferStart = performance.now();
      const uploaded = await runtime.fromArray(new Int32Array(inputs));
      const mapped = await runtime.map(program, uploaded);
      assert.throws(() => runtime.map(program, uploaded), /consumed/);
      assert.deepEqual(Array.from(await runtime.toArray(mapped)), expected);
      row.gpu_transfer_dispatch_sync_readback_ms = performance.now() - transferStart;
      for (const count of [0, 1, 255, 256, 257, inputs.length]) {
        const initialized = await runtime.init(program, count);
        assert.deepEqual(Array.from(await runtime.toArray(initialized)), Array.from({ length: count }, (_, index) => kernel.reference(index)));
      }
      assert.throws(() => runtime.init(program, -1), RangeError);
      assert.throws(() => runtime.fromArray(new Float32Array(1)), TypeError);
      if (kernel.name === "mix") {
        const example = execute(process.execPath, [resolve("examples/gpu/run.mjs"), wgsl]);
        assert.deepEqual(JSON.parse(example.stdout), Array.from({ length: 16 }, (_, index) => kernel.reference(index)));
      }
      let resident = await runtime.fromArray(new Int32Array(inputs));
      let chained = inputs;
      const residentStart = performance.now();
      for (let step = 0; step < 3; step++) {
        resident = await runtime.map(program, resident);
        chained = chained.map(value => Number(signed(kernel.reference(value))));
      }
      row.gpu_resident_dispatch_sync_ms = performance.now() - residentStart;
      assert.deepEqual(Array.from(await runtime.toArray(resident)), chained.map(bits));
      row.gpu = "validated";
      row.adapter = runtime.info;
      row.device_startup_ms = startupMs;
    }
    rows.push(row);
  }
  const api = join(directory, "api");
  mkdirSync(api);
  const source = join(api, "Main.tz");
  writeFileSync(source, `export def pipeline :: i64
fn pipeline = {
    let device = Result.get (Gpu.request Gpu.CpuReference);
    let buffer = Gpu.init (&device) 8 (\\index -> index * index);
    let mapped = Gpu.map (&device) (\\value -> value + 1) buffer;
    let values = Gpu.to_array mapped;
    Array.sum (&values) as i64
}
export def float_reference :: f32 -> f32
fn float_reference value = {
    let device = Result.get (Gpu.request Gpu.CpuReference);
    let values = [value];
    let buffer = Gpu.from_array (&device) (&values);
    let mapped = Gpu.map (&device) (\\item -> item * item + item) buffer;
    let result = Gpu.to_array mapped;
    result[0]
}
export def unavailable :: bool
fn unavailable = {
  let explicit = Gpu.request Gpu.WebGpu;
  let automatic = Gpu.request Gpu.Auto;
  Result.is_error (&explicit) && Result.is_error (&automatic)
}
`);
  const ir = join(api, "api.ll");
  cli(["build", source, "--emit", "llvm", "-o", ir]);
  writeFileSync(ir, readFileSync(ir, "utf8").replaceAll("@malloc(", "@tracked_alloc(").replaceAll("@free(", "@tracked_free("));
  const host = join(api, "host.c");
  writeFileSync(host, `#include <stdint.h>\n#include <stdlib.h>\n#include <assert.h>\n#include <math.h>\n#include <string.h>\nstatic uint64_t live;\nvoid *tracked_alloc(uint64_t size) { uint64_t *header = malloc(size + 16); assert(header); header[0] = size; live += size; return header + 2; }\nvoid tracked_free(void *pointer) { if (pointer) { uint64_t *header = (uint64_t *)pointer - 2; live -= header[0]; free(header); } }\nextern int64_t tz_pipeline(void);\nextern float tz_float_reference(float);\nextern uint8_t tz_unavailable(void);\nint main(void) { uint32_t patterns[] = {0, 0x80000000u, 1, 0x80000001u, 0x00800000u, 0x3f800000u, 0xbf800000u, 0x7f800000u, 0xff800000u, 0x7fc00000u}; for (unsigned repeat = 0; repeat < 500; repeat++) { assert(tz_pipeline() == 148); assert(tz_unavailable()); for (unsigned index = 0; index < 10; index++) { float value; memcpy(&value, &patterns[index], 4); volatile float product = value * value; float expected = product + value; float actual = tz_float_reference(value); if (isnan(expected)) assert(isnan(actual)); else assert(memcmp(&actual, &expected, 4) == 0); } assert(live == 0); } return 0; }\n`);
  for (const optimization of [0, 3]) {
    const native = join(api, `host-${optimization}${process.platform === "win32" ? ".exe" : ""}`);
    execute(clang, [ir, host, `-O${optimization}`, "-ffp-contract=off", "-Wno-override-module", "-o", native, ...(process.platform === "win32" ? [] : ["-lm"])]);
    execute(native, []);
    const wasm = join(api, `api-${optimization}.wasm`);
    cli(["build", source, "--target", "wasm32", `-O${optimization}`, "-o", wasm]);
    const module = new WebAssembly.Module(readFileSync(wasm));
    assert.deepEqual(WebAssembly.Module.imports(module), []);
    const instance = new WebAssembly.Instance(module);
    for (let repeat = 0; repeat < 500; repeat++) {
      assert.equal(instance.exports.tz_pipeline(), 148n);
      assert.equal(instance.exports.tz_unavailable(), 1);
      for (const value of [0, -0, 2 ** -149, -(2 ** -149), 2 ** -126, 1, -1, Infinity, -Infinity, NaN]) {
        assert.deepEqual(instance.exports.tz_float_reference(value), Math.fround(Math.fround(value * value) + value));
      }
    }
    assert.ok(instance.exports.memory.buffer.byteLength <= 16 * 1024 * 1024);
  }
  const invalid = join(directory, "invalid");
  mkdirSync(invalid);
  writeFileSync(join(invalid, "Main.tz"), "export def kernel :: f32 -> f32\nfn kernel value = value + 1.0\n");
  const protectedOutput = join(invalid, "old.wgsl");
  writeFileSync(protectedOutput, "preserved");
  const failure = cli(["build", invalid, "--emit", "wgsl", "-o", protectedOutput, "--json"], false);
  assert.equal(JSON.parse(failure.stderr).code, "E1018");
  assert.equal(readFileSync(protectedOutput, "utf8"), "preserved");
  if (benchmark) console.log(JSON.stringify({ note: "CPU scalar host calls are not a bulk speed comparison; resident time excludes upload/readback; no speed thresholds", rows }));
  else console.log(`GPU phase 1: ${kernels.length * inputs.length} integer references, native/WASM O0/O3, owned heap and strict float CPU reference; WebGPU ${useGpu ? "executed and matched" : "not requested (CPU reference validation only)"}`);
} finally {
  if (runtime) await runtime.close();
  runtime = undefined;
  provider = undefined;
  rmSync(directory, { recursive: true, force: true });
}
