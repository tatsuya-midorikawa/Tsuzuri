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
function execute(program, args, success = true, env = process.env) {
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 120000, maxBuffer: 8 * 1024 * 1024, env });
  assert.ifError(result.error);
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}
const cli = (args, success) => execute(compiler, args, success);
const bits = value => Number(BigInt.asUintN(32, BigInt(value)));
const signed = value => BigInt.asIntN(32, BigInt(value));
const kernels = [
  { name: "mix", body: "(value * 1664525 + 1013904223) ^ (Bits.ushr value 13)", reference: value => bits((signed(value) * 1664525n + 1013904223n) ^ (BigInt(bits(value)) >> 13n)) },
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
      assert.throws(() => runtime.fromArray(new Float64Array(1)), TypeError);
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
  // Relaxed f32 kernels (F09): the strict CPU reference is exact, the real adapter is compared within the D6 tolerances.
  const fr = Math.fround;
  const f32View = new Float32Array(1);
  const u32View = new Uint32Array(f32View.buffer);
  const ulp = value => { f32View[0] = Math.abs(value); const low = f32View[0]; u32View[0] += 1; return f32View[0] - low; };
  const relaxedKernels = [
    { name: "poly", from: "f32", to: "f32", body: "value * value + value", tolerance: 4, reference: value => fr(fr(value * value) + value) },
    { name: "horner", from: "f32", to: "f32", body: "((value * 0.5 + 0.25) * value + 0.125) * value + 1.0", tolerance: 12, reference: value => {
      const scaled = fr(fr(value * 0.5) + 0.25);
      return fr(fr(fr(fr(scaled * value) + 0.125) * value) + 1.0);
    } },
    { name: "ratio", from: "f32", to: "f32", body: "(value + 1.0) / (value * value + 2.0)", tolerance: 9, reference: value => fr(fr(value + 1.0) / fr(fr(value * value) + 2.0)) },
    { name: "index", from: "i32", to: "f32", body: "(value as f32) * 0.5 + 0.25", tolerance: 5, reference: value => fr(fr(fr(value) * 0.5) + 0.25) },
    { name: "threshold", from: "f32", to: "i32", body: "if value * value > 2.0 then 1 else 0", tolerance: 0, reference: value => (fr(value * value) > 2 ? 1 : 0) },
  ];
  const floatInputs = [2 ** -8, 0.5, 1, 1.5, 2, 3, 255, 256];
  let floatSeed = 0x2468ace1;
  for (let index = 0; index < 248; index++) {
    floatSeed = (Math.imul(floatSeed, 1664525) + 1013904223) | 0;
    floatInputs.push(fr(2 ** ((floatSeed >>> 8) / 2 ** 24 * 16 - 8)));
  }
  const finiteCount = floatInputs.length;
  floatInputs.push(0, -0, 2 ** -149, NaN, Infinity, -Infinity, 3.4e38);
  const indexInputs = Array.from({ length: floatInputs.length }, (_, index) => index);
  const sameFloat = (actual, expected) => (Number.isNaN(expected) ? Number.isNaN(actual) : Object.is(actual, expected));
  const relaxedErrors = {};
  const relaxedCpu = {};
  for (const kernel of relaxedKernels) {
    const project = join(directory, `relaxed-${kernel.name}`);
    mkdirSync(project);
    const source = join(project, "Main.tz");
    writeFileSync(source, `export def kernel :: ${kernel.from} -> ${kernel.to}\nfn kernel value = ${kernel.body}\n`);
    const wgsl = join(project, "kernel.wgsl");
    const repeated = join(project, "repeated.wgsl");
    cli(["build", source, "--emit", "wgsl-relaxed", "-o", wgsl]);
    cli(["build", source, "--emit", "wgsl-relaxed", "-o", repeated]);
    assert.deepEqual(readFileSync(wgsl), readFileSync(repeated));
    assert.ok(readFileSync(wgsl, "utf8").startsWith(`// tsuzuri-gpu float=relaxed input=${kernel.from === "f32" ? "f32" : "i32"} output=${kernel.to === "f32" ? "f32" : "i32"}\n`));
    assert.equal(JSON.parse(cli(["build", source, "--emit", "wgsl", "-o", join(project, "strict.wgsl"), "--json"], false).stderr).code, "E1018");
    assert.notEqual(cli(["build", source, "--emit", "wgsl-relaxed", "-O3", "-o", join(project, "never.wgsl")], false).status, 0);
    const lanes = kernel.from === "i32" ? indexInputs : floatInputs;
    for (const optimization of [0, 3]) {
      const wasm = join(project, `kernel-${optimization}.wasm`);
      cli(["build", source, "--target", "wasm32", `-O${optimization}`, "-o", wasm]);
      const module = new WebAssembly.Module(readFileSync(wasm));
      assert.deepEqual(WebAssembly.Module.imports(module), []);
      const { exports } = new WebAssembly.Instance(module);
      for (const value of lanes) {
        const expected = kernel.reference(value);
        const actual = exports.tz_kernel(value);
        assert.ok(kernel.to === "i32" ? actual === expected : sameFloat(actual, expected), `${kernel.name}(${value}) -O${optimization}: ${actual} != ${expected}`);
      }
      if (optimization === 3) {
        const cpuStart = performance.now();
        for (let repeat = 0; repeat < (quick ? 1 : 30); repeat++) for (const value of lanes) exports.tz_kernel(value);
        relaxedCpu[kernel.name] = performance.now() - cpuStart;
      }
    }
  }
  const wide = join(directory, "relaxed-f64");
  mkdirSync(wide);
  writeFileSync(join(wide, "Main.tz"), "export def kernel :: f64 -> f64\nfn kernel value = value + 1.0\n");
  const wideOutput = join(wide, "old.wgsl");
  writeFileSync(wideOutput, "preserved");
  assert.equal(JSON.parse(cli(["build", wide, "--emit", "wgsl-relaxed", "-o", wideOutput, "--json"], false).stderr).code, "E1018");
  assert.equal(readFileSync(wideOutput, "utf8"), "preserved");
  if (runtime) {
    for (const kernel of relaxedKernels) {
      const wgsl = readFileSync(join(directory, `relaxed-${kernel.name}`, "kernel.wgsl"), "utf8");
      await assert.rejects(runtime.prepare(wgsl), /requires prepare/);
      await assert.rejects(runtime.prepare(wgsl, { float: "other" }), /requires prepare/);
      const preparing = performance.now();
      const program = await runtime.prepare(wgsl, { float: "relaxed" });
      const row = { kernel: `relaxed_${kernel.name}`, count: floatInputs.length, float: "relaxed", tolerance_ulps: kernel.tolerance, cpu_wasm_scalar_calls_ms: relaxedCpu[kernel.name], gpu: "validated", adapter: runtime.info, device_startup_ms: startupMs, pipeline_creation_ms: performance.now() - preparing };
      const check = (actuals, label) => {
        let worst = 0;
        actuals.forEach((actual, index) => {
          if (index >= finiteCount) return;
          const value = kernel.from === "i32" ? indexInputs[index] : floatInputs[index];
          const expected = kernel.reference(value);
          if (kernel.to === "i32") {
            assert.equal(actual >> 0, expected, `${label} ${kernel.name}(${value})`);
          } else {
            assert.ok(Number.isFinite(actual), `${label} ${kernel.name}(${value}) is not finite: ${actual}`);
            const error = Math.abs(actual - expected) / ulp(expected);
            worst = Math.max(worst, error);
            assert.ok(error <= kernel.tolerance, `${label} ${kernel.name}(${value}): ${actual} vs ${expected} is ${error} ulp, over ${kernel.tolerance}`);
          }
        });
        return worst;
      };
      let worst = 0;
      if (kernel.from === "i32") {
        for (const count of [0, 1, 255, 256, 257, floatInputs.length]) {
          const initialized = await runtime.init(program, count);
          const values = await runtime.toArray(initialized);
          assert.ok(values instanceof Float32Array && values.length === count);
          values.forEach((actual, index) => {
            if (index < finiteCount) worst = Math.max(worst, Math.abs(actual - kernel.reference(index)) / ulp(kernel.reference(index)));
            assert.ok(Math.abs(actual - kernel.reference(index)) <= kernel.tolerance * ulp(kernel.reference(index)), `init ${kernel.name}(${index}): ${actual}`);
          });
        }
        const transferStart = performance.now();
        const mapped = await runtime.init(program, floatInputs.length);
        worst = Math.max(worst, check(await runtime.toArray(mapped), "init"));
        row.gpu_transfer_dispatch_sync_readback_ms = performance.now() - transferStart;
      } else {
        const transferStart = performance.now();
        const uploaded = await runtime.fromArray(new Float32Array(floatInputs));
        const mapped = await runtime.map(program, uploaded);
        assert.throws(() => runtime.map(program, uploaded), /consumed/);
        const actuals = await runtime.toArray(mapped);
        row.gpu_transfer_dispatch_sync_readback_ms = performance.now() - transferStart;
        assert.ok(kernel.to === "i32" ? actuals instanceof Uint32Array : actuals instanceof Float32Array);
        assert.equal(actuals.length, floatInputs.length);
        worst = check(actuals, "map");
        const mismatched = await runtime.fromArray(new Int32Array(1));
        assert.throws(() => runtime.map(program, mismatched), /does not match the kernel input/);
        assert.deepEqual(Array.from(await runtime.toArray(mismatched)), [0]);
      }
      if (kernel.name === "ratio") {
        let resident = await runtime.fromArray(new Float32Array(floatInputs.slice(0, finiteCount)));
        const residentStart = performance.now();
        for (let step = 0; step < 3; step++) resident = await runtime.map(program, resident);
        row.gpu_resident_dispatch_sync_ms = performance.now() - residentStart;
        const chained = await runtime.toArray(resident);
        chained.forEach((actual, index) => {
          let expected = floatInputs[index];
          for (let step = 0; step < 3; step++) expected = kernel.reference(expected);
          assert.ok(Math.abs(actual - expected) <= 54 * ulp(expected), `resident ratio(${floatInputs[index]}): ${actual} vs ${expected}`);
        });
      }
      assert.throws(() => runtime.fromArray(new Float64Array(1)), TypeError);
      row.max_ulp_error = worst;
      relaxedErrors[kernel.name] = worst;
      rows.push(row);
    }
  }
  // f16 lanes (F09 Phase 2). f16 cannot be exported, so these kernels have no --emit form: the WGSL is read from the
  // kernel table that a device-aware program embeds. The values are small integers, which f16 holds exactly.
  const halfProject = join(directory, "halves");
  mkdirSync(halfProject);
  writeFileSync(join(halfProject, "Main.tz"), "def square :: f16 -> f16\nfn square value = value * value + value\ndef index :: i32 -> f16\nfn index value = value as f16\nlet device = Result.get (Gpu.request Gpu.WebGpu)\nlet values: [f16] = [1.0f16]\nlet _a = Gpu.map_relaxed (&device) square (Gpu.from_array (&device) (&values))\nlet _b = Gpu.init_relaxed (&device) 4 index\n0\n");
  const halfIr = join(halfProject, "halves.ll");
  cli(["build", halfProject, "--emit", "llvm", "-o", halfIr]);
  const embedded = number => {
    const constant = new RegExp(`@tz\\.gpu\\.kernel\\.${number}\\.wgsl = [^\\n]*? c"((?:[^"\\\\]|\\\\[0-9A-F]{2})*)"`).exec(readFileSync(halfIr, "utf8"));
    return constant[1].replace(/\\([0-9A-F]{2})/g, (_, hex) => String.fromCharCode(parseInt(hex, 16))).replace(/\0$/, "");
  };
  const squareWgsl = embedded(0);
  const indexWgsl = embedded(1);
  assert.ok(squareWgsl.startsWith("// tsuzuri-gpu float=relaxed input=f16 output=f16\nenable f16;\n"));
  assert.ok(indexWgsl.startsWith("// tsuzuri-gpu float=relaxed input=i32 output=f16\nenable f16;\n"));
  let halfNote = "f16 resident lanes not requested";
  if (runtime) {
    const half16 = integer => (integer === 0 ? 0 : ((Math.floor(Math.log2(integer)) + 15) << 10) | Math.round((integer / 2 ** Math.floor(Math.log2(integer)) - 1) * 1024));
    await assert.rejects(runtime.prepare(squareWgsl, { float: "relaxed" }), /shader-f16/);
    const adapter = await provider.requestAdapter();
    if (adapter.features.has("shader-f16")) {
      const halves = await createWebGpu(provider, { features: ["shader-f16"] });
      try {
        const square = await halves.prepare(squareWgsl, { float: "relaxed" });
        const index = await halves.prepare(indexWgsl, { float: "relaxed" });
        // Odd counts leave a 2-byte tail, which the buffers pad to 4 bytes.
        for (const count of [0, 1, 3, 40, 41]) {
          const initialized = await halves.toArray(await halves.init(index, count));
          assert.ok(initialized instanceof Uint16Array);
          assert.deepEqual(Array.from(initialized), Array.from({ length: count }, (_, k) => half16(k)));
          const squared = await halves.toArray(await halves.map(square, await halves.fromArray(Uint16Array.from({ length: count }, (_, k) => half16(k)))));
          assert.deepEqual(Array.from(squared), Array.from({ length: count }, (_, k) => half16(k * k + k)));
        }
        assert.throws(() => halves.fromArray(new Float64Array(1)), TypeError);
      } finally {
        await halves.close();
      }
      halfNote = "f16 resident lanes matched exactly";
    } else {
      halfNote = "the adapter lacks shader-f16, so f16 resident lanes were not run";
    }
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
export def relaxed_reference :: f32 -> f32
fn relaxed_reference value = {
    let device = Result.get (Gpu.request Gpu.CpuReference);
    let values = [value];
    let buffer = Gpu.from_array (&device) (&values);
    let mapped = Gpu.map_relaxed (&device) (\\item -> item * item + item) buffer;
    let result = Gpu.to_array mapped;
    result[0]
}
export def relaxed_init_sum :: f32
fn relaxed_init_sum = {
    let device = Result.get (Gpu.request Gpu.CpuReference);
    let buffer = Gpu.init_relaxed (&device) 8 (\\index -> (index as f32) * 0.5 + 0.25);
    let values = Gpu.to_array buffer;
    Array.sum (&values)
}
export def unavailable :: bool
fn unavailable = {
  let explicit = Gpu.request Gpu.WebGpu;
  let automatic = Gpu.request Gpu.Auto;
  Result.is_error (&explicit) && Result.is_ok (&automatic)
}
`);
  const ir = join(api, "api.ll");
  cli(["build", source, "--emit", "llvm", "-o", ir]);
  writeFileSync(ir, readFileSync(ir, "utf8").replaceAll("@malloc(", "@tracked_alloc(").replaceAll("@free(", "@tracked_free("));
  const host = join(api, "host.c");
  writeFileSync(host, `#include <stdint.h>\n#include <stdlib.h>\n#include <assert.h>\n#include <math.h>\n#include <string.h>\nstatic uint64_t live;\nvoid *tracked_alloc(uint64_t size) { uint64_t *header = malloc(size + 16); assert(header); header[0] = size; live += size; return header + 2; }\nvoid tracked_free(void *pointer) { if (pointer) { uint64_t *header = (uint64_t *)pointer - 2; live -= header[0]; free(header); } }\nextern int64_t tz_pipeline(void);\nextern float tz_float_reference(float);\nextern float tz_relaxed_reference(float);\nextern float tz_relaxed_init_sum(void);\nextern uint8_t tz_unavailable(void);\nint main(void) { uint32_t patterns[] = {0, 0x80000000u, 1, 0x80000001u, 0x00800000u, 0x3f800000u, 0xbf800000u, 0x7f800000u, 0xff800000u, 0x7fc00000u}; for (unsigned repeat = 0; repeat < 500; repeat++) { assert(tz_pipeline() == 148); assert(tz_unavailable()); assert(tz_relaxed_init_sum() == 16.0f); for (unsigned index = 0; index < 10; index++) { float value; memcpy(&value, &patterns[index], 4); volatile float product = value * value; float expected = product + value; float actual = tz_float_reference(value); float relaxed = tz_relaxed_reference(value); if (isnan(expected)) { assert(isnan(actual)); assert(isnan(relaxed)); } else { assert(memcmp(&actual, &expected, 4) == 0); assert(memcmp(&relaxed, &expected, 4) == 0); } } assert(live == 0); } return 0; }\n`);
  for (const optimization of [0, 3]) {
    const native = join(api, `host-${optimization}${process.platform === "win32" ? ".exe" : ""}`);
    // `unavailable` names Gpu.WebGpu, so the program calls the GPU runtime of src/runtime/gpu.c. The run has
    // no WebGPU library (an empty TSUZURI_WEBGPU_LIBRARY disables the backend), so the result does not depend on
    // the machine: Gpu.request Gpu.WebGpu is Unavailable and no CPU run is substituted, while Gpu.request Gpu.Auto
    // always succeeds (F09 Phase 3: it needs no device, and the CPU reference serves its calls). This clang line
    // has no Vulkan backend, which only a program that names Gpu.Vulkan or Gpu.Auto gets from the driver.
    execute(clang, [ir, host, resolve("src/runtime/gpu.c"), `-O${optimization}`, "-ffp-contract=off", "-Wno-override-module", "-o", native, ...(process.platform === "win32" ? [] : ["-lm", "-pthread"]), ...(process.platform === "linux" ? ["-ldl"] : [])]);
    execute(native, [], true, { ...process.env, TSUZURI_WEBGPU_LIBRARY: "" });
    const wasm = join(api, `api-${optimization}.wasm`);
    cli(["build", source, "--target", "wasm32", `-O${optimization}`, "-o", wasm]);
    const module = new WebAssembly.Module(readFileSync(wasm));
    assert.deepEqual(WebAssembly.Module.imports(module), []);
    const instance = new WebAssembly.Instance(module);
    for (let repeat = 0; repeat < 500; repeat++) {
      assert.equal(instance.exports.tz_pipeline(), 148n);
      assert.equal(instance.exports.tz_unavailable(), 1);
      assert.equal(instance.exports.tz_relaxed_init_sum(), 16);
      for (const value of [0, -0, 2 ** -149, -(2 ** -149), 2 ** -126, 1, -1, Infinity, -Infinity, NaN]) {
        assert.deepEqual(instance.exports.tz_float_reference(value), Math.fround(Math.fround(value * value) + value));
        assert.deepEqual(instance.exports.tz_relaxed_reference(value), Math.fround(Math.fround(value * value) + value));
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
  if (benchmark) console.log(JSON.stringify({ note: "CPU scalar host calls are not a bulk speed comparison; GPU times include upload, dispatch, completion wait, and readback unless the column says resident; no speed thresholds", rows }));
  else console.log(`GPU phase 1: ${kernels.length * inputs.length} integer references and ${relaxedKernels.length * floatInputs.length} relaxed f32 references, native/WASM O0/O3, owned heap and strict float CPU reference; WebGPU ${useGpu ? `executed and matched (relaxed within the D6 tolerances; max ulp error ${Object.entries(relaxedErrors).map(([name, worst]) => `${name} ${worst.toFixed(2)}`).join(", ")}; ${halfNote})` : "not requested (CPU reference validation only)"}`);
} finally {
  if (runtime) await runtime.close();
  runtime = undefined;
  provider = undefined;
  rmSync(directory, { recursive: true, force: true });
}
