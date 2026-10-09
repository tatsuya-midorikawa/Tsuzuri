// Vulkan runtime tests (F09 Phase 3): src/runtime/gpu-vulkan.c through tests/gpu_vulkan_runtime.c, a synthetic Vulkan
// library (tests/gpu_vulkan_mock.c) for the paths a real machine does not reach, and the real device when there is one.
//
//   node tests/gpu_vulkan.mjs
//
// Everything is built with AddressSanitizer and UndefinedBehaviorSanitizer (a build without them is used when the compiler
// has none, and says so). A check that cannot run prints `SKIPPED` with the reason and is counted in the summary line;
// TSUZURI_REQUIRE_VULKAN=1 turns a skipped device check into a failure, TSUZURI_REQUIRE_SPIRV_TOOLS=1 a missing spirv-as.
import assert from "node:assert/strict";
import { existsSync, mkdtempSync, readFileSync as readRaw, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join, resolve } from "node:path";
import { spawnSync } from "node:child_process";

if (process.platform === "win32") {
  console.log("GPU Vulkan runtime: SKIPPED (the harness and the mock use dlopen and pthreads; the Windows loader path is verified by CI type checks only)");
  process.exit(0);
}

const root = resolve(import.meta.dirname, "..");
// A Buffer of its own: small reads come from a shared pool, and `.buffer` of those is not the file.
const readFileSync = path => { const data = readRaw(path); const copy = Buffer.alloc(data.length); data.copy(copy); return copy; };
const clang = process.env.TSUZURI_CLANG ?? "clang";
const directory = mkdtempSync(join(tmpdir(), "tsuzuri-gpu-vulkan-"));
const library = process.platform === "darwin" ? "dylib" : "so";
const skips = [];
let passed = 0;
let failed = 0;

function run(program, args, options = {}) {
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 300000, maxBuffer: 64 * 1024 * 1024, ...options });
  assert.ifError(result.error);
  return result;
}

function tool(name, candidates) {
  for (const candidate of candidates) if (candidate && existsSync(candidate)) return candidate;
  return run(name, ["--version"]).status === 0 ? name : undefined;
}

function skip(name, reason, requirement) {
  console.log(`SKIPPED ${name}: ${reason}`);
  skips.push(`${name}: ${reason}`);
  if (requirement && process.env[requirement]) {
    console.log(`FAILED ${name}: ${requirement} is set but the check was skipped`);
    failed++;
  }
}

function check(name, body) {
  try {
    body();
    passed++;
    console.log(`ok ${name}`);
  } catch (error) {
    failed++;
    console.log(`FAILED ${name}\n${error.stack ?? error}`);
  }
}

// ---- builds ----
const sanitize = ["-fsanitize=address,undefined", "-fno-sanitize-recover=undefined", "-g", "-O1"];
let flags = sanitize;
{
  const probe = join(directory, "probe.c");
  writeFileSync(probe, "int main(void) { return 0; }\n");
  const result = run(clang, [...sanitize, probe, "-o", join(directory, "probe")]);
  if (result.status !== 0) {
    flags = ["-g", "-O1"];
    skip("sanitizers", `${clang} cannot build with -fsanitize=address,undefined; the checks run without them`);
  }
}
const harnessPath = join(directory, "harness");
const mockPath = join(directory, `libvk_mock.${library}`);
const harnessBuild = run(clang, ["-std=c11", "-Wall", "-Wextra", "-Wpedantic", ...flags, join(root, "tests/gpu_vulkan_runtime.c"), "-o", harnessPath]);
assert.equal(harnessBuild.status, 0, harnessBuild.stderr);
assert.doesNotMatch(harnessBuild.stderr, /warning/, "the runtime must compile without warnings");
const mockBuild = run(clang, ["-std=gnu11", "-Wall", "-Wextra", "-Wno-unused-function", "-Wno-unused-variable", "-ffp-contract=off", ...flags, "-shared", "-fPIC", join(root, "tests/gpu_vulkan_mock.c"), "-o", mockPath]);
assert.equal(mockBuild.status, 0, mockBuild.stderr);
assert.doesNotMatch(mockBuild.stderr, /warning/, "the mock must compile without warnings");

// The Vulkan implementations the device checks run on: the default one of the machine, and every driver manifest named in
// TSUZURI_VULKAN_TEST_ICDS (a path list), each selected through VK_DRIVER_FILES (for example the SwiftShader of a browser).
const implementations = [{ label: "default", env: {} }, ...(process.env.TSUZURI_VULKAN_TEST_ICDS ?? "").split(delimiter).filter(Boolean).map((manifest, index) => ({ label: `icd${index}`, env: { VK_DRIVER_FILES: manifest, VK_ICD_FILENAMES: manifest } }))];
let current = implementations[0];
const environment = (extra = {}) => ({ ...process.env, ASAN_OPTIONS: "detect_stack_use_after_return=1:abort_on_error=0", UBSAN_OPTIONS: "print_stacktrace=1", ...extra });
const harness = (args, extra = {}) => run(harnessPath, args, { env: environment({ ...current.env, ...extra }) });
const mock = (config, args, extra = {}) => harness(args, { TSUZURI_VULKAN_LIBRARY: mockPath, TZ_VK_MOCK: config, ...extra });
const statusOf = result => Number(/status=(\d+)/.exec(result.stdout)?.[1] ?? NaN);

// A module with the entry points of a kernel and nothing else: enough for the mock, which parses only OpEntryPoint.
function fakeModule(capabilities = [1]) {
  const words = [0x07230203, 0x00010300, 0, 64, 0];
  const instruction = (opcode, operands) => words.push(((operands.length + 1) << 16) | opcode, ...operands);
  const text = value => {
    const bytes = [...Buffer.from(value + "\0")];
    while (bytes.length % 4) bytes.push(0);
    return Array.from({ length: bytes.length / 4 }, (_, index) => bytes.readUInt32LE?.(index * 4) ?? (bytes[index * 4] | (bytes[index * 4 + 1] << 8) | (bytes[index * 4 + 2] << 16) | (bytes[index * 4 + 3] << 24)) >>> 0);
  };
  for (const capability of capabilities) instruction(17, [capability]);
  instruction(14, [0, 1]);
  instruction(15, [5, 1, ...text("map_main"), 2]);
  instruction(15, [5, 3, ...text("init_main"), 2]);
  return Buffer.from(new Uint32Array(words).buffer);
}

const u32 = values => Buffer.from(new Uint32Array(values).buffer);
const u64 = values => Buffer.from(new BigUint64Array(values.map(BigInt)).buffer);
const mockModule = join(directory, "mock.spv");
writeFileSync(mockModule, fakeModule());
const lcg = seed => () => (seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0);
const sample = (count, seed = 1) => { const next = lcg(seed); return Array.from({ length: count }, next); };

// ---- the hand-declared ABI against the real headers ----
{
  const headers = [process.env.VULKAN_HEADERS, "/opt/homebrew/opt/vulkan-headers/include", "/usr/include", "/usr/local/include"].find(path => path && existsSync(join(path, "vulkan/vulkan.h")));
  if (!headers) {
    skip("vulkan abi", "the Vulkan headers were not found (set VULKAN_HEADERS); the hand-declared ABI was not compared");
  } else {
    check("vulkan abi: the declarations equal the real headers (layouts and constants)", () => {
      const real = join(directory, "abi_real");
      const mine = join(directory, "abi_mine");
      assert.equal(run(clang, ["-std=c11", "-DTZ_ABI_REAL", `-I${headers}`, join(root, "tests/gpu_vulkan_abi.c"), "-o", real]).status, 0);
      const built = run(clang, ["-std=c11", "-Wno-unused-function", join(root, "tests/gpu_vulkan_abi.c"), "-o", mine]);
      assert.equal(built.status, 0, built.stderr);
      const expected = run(real, []).stdout;
      const actual = run(mine, []).stdout;
      assert.ok(expected.split("\n").length > 90);
      assert.equal(actual, expected);
    });
  }
}

// ---- the loader and the capabilities, through the mock ----
const probeStatus = (config, features) => statusOf(mock(config, ["probe", "--features", String(features)]));
check("capabilities: statuses of open for devices with and without the features", () => {
  const strictControls = "api=1.1,fc_ext=1,szinp=1,denorm=1,rte=1,denorm_independence=32,rounding_independence=32";
  const matrix = [
    // [config, features 0, int64 (2), strict f32 (4), both (6)]
    ["", 0, 0, 2, 2],
    ["strict=1", 0, 0, 0, 0],
    ["int64=0", 0, 2, 2, 2],
    ["int64=0,strict=1", 0, 2, 0, 2],
    ["devices=0", 1, 1, 1, 1],
    ["no_version=1", 2, 2, 2, 2],
    ["instance=1.0", 2, 2, 2, 2],
    ["missing=vkCreateFence", 1, 1, 1, 1],
    ["missing=vkGetDeviceProcAddr", 1, 1, 1, 1],
    ["queue=0", 1, 1, 1, 1],
    ["invocations=64", 2, 2, 2, 2],
    ["group_size=128", 2, 2, 2, 2],
    ["type=discrete", 0, 0, 2, 2],
    ["type=cpu", 0, 0, 2, 2],
    ["portability=1", 0, 0, 2, 2],
    ["portability=1,strict=1", 0, 0, 0, 0],
    ["api=1.1", 0, 0, 2, 2],
    [strictControls, 0, 0, 0, 0],
    ["api=1.1,fc_ext=1,szinp=1,denorm=1,rte=1,denorm_independence=none,rounding_independence=none", 0, 0, 2, 2],
    ["strict=1,denorm=0", 0, 0, 2, 2],
    ["strict=1,rte=0", 0, 0, 2, 2],
    ["strict=1,szinp=0", 0, 0, 2, 2],
    ["strict=1,denorm_independence=none", 0, 0, 2, 2],
    ["strict=1,rounding_independence=none", 0, 0, 2, 2],
    ["strict=1,denorm_independence=32,rounding_independence=32", 0, 0, 0, 0],
  ];
  for (const [config, ...expected] of matrix) {
    [0, 2, 4, 6].forEach((features, index) => assert.equal(probeStatus(config, features), expected[index], `[${config}] features=${features}`));
  }
  // Bits of other backends (bit 0 = WebGPU shader-f16) never make the device unsupported.
  assert.equal(probeStatus("", 1), 0);
  assert.equal(probeStatus("", 9), 0);
});

check("capabilities: a device that satisfies the features is chosen among several, by type", () => {
  const result = mock("devices=3", ["probe", "--features", "0"]);
  assert.equal(statusOf(result), 0, result.stdout + result.stderr);
  assert.match(result.stdout, /Mock Vulkan device/);
});

// ---- the conformance probe of the strict float32 contract ----
// A device can report the float controls and still compute differently, so the runtime dispatches a probe module once, on
// the first request that needs strict float32 (tests/gpu_vulkan_probe.spvasm). The mock computes its eight operations in C
// float arithmetic; `probe=<lane>` flips a bit of one lane of its result.
const strictModule = join(directory, "probe-strict.spv");
writeFileSync(strictModule, fakeModule([1, 4464, 4466, 4467]));
const strictProbe = (config, features, { repeat = 1, kernel, env = {} } = {}) => {
  const args = ["probe", "--features", String(features), "--repeat", String(repeat)];
  if (kernel) {
    writeFileSync(join(directory, "probe-in.bin"), u32(sample(8)));
    args.push("--spirv", kernel, "--mode", "map", "--lanes", "1,1", "--count", "8", "--input", join(directory, "probe-in.bin"));
  }
  const result = mock(config, args, env);
  const field = name => new RegExp(`${name}=(\\S+)`).exec(result.stdout)?.[1];
  return { status: statusOf(result), strict: field("strict_f32"), probe: field("strict_probe"), run: field("run"), dispatches: Number(field("probe_dispatches")), stderr: result.stderr };
};
const verdict = answer => [answer.status, answer.strict, answer.probe, answer.dispatches];

check("strict probe: runs once, on the first request that needs strict float32, and only where the properties report the controls", () => {
  assert.deepEqual(verdict(strictProbe("strict=1", 0)), [0, "1", "not-run", 0], "nothing asks for strict float32");
  assert.deepEqual(verdict(strictProbe("strict=1", 2)), [0, "1", "not-run", 0], "a program of integers never runs it");
  assert.deepEqual(verdict(strictProbe("strict=1", 4)), [0, "1", "passed", 1]);
  assert.deepEqual(verdict(strictProbe("strict=1", 6, { repeat: 5 })), [0, "1", "passed", 1], "once, however many requests follow");
  assert.deepEqual(verdict(strictProbe("", 4)), [2, "0", "not-run", 0], "a device without the controls does not get the probe");
  assert.deepEqual(verdict(strictProbe("strict=1,denorm=0", 4)), [2, "0", "not-run", 0]);
  assert.match(strictProbe("strict=1", 4, { env: { TSUZURI_GPU_DEBUG: "1" } }).stderr, /the strict float32 probe passed \(24 lanes\)/);
});

check("strict probe: a device that reports the controls but computes any lane differently refuses strict float32, and only that", () => {
  for (let lane = 0; lane < 24; lane++) {
    const answer = strictProbe(`strict=1,probe=${lane}`, 4, { env: { TSUZURI_GPU_DEBUG: "1" } });
    assert.deepEqual(verdict(answer), [2, "0", "failed", 1], `lane ${lane}`);
    assert.match(answer.stderr, new RegExp(`lane ${lane}: the device computed`));
    assert.match(answer.stderr, /but 1 of 24 probe lanes differ from the reference; strict float32 kernels are refused/);
  }
  assert.deepEqual(verdict(strictProbe("strict=1,probe=3", 6, { repeat: 3 })), [2, "0", "failed", 1], "a failed probe is not run again");
  // The kernels that do not need strict float32 still run on the same device, in the same process.
  const integers = strictProbe("strict=1,probe=3", 4, { kernel: mockModule });
  assert.deepEqual([integers.status, integers.run, integers.probe], [2, "0", "failed"]);
  const plain = strictProbe("strict=1,probe=3", 0, { kernel: mockModule });
  assert.deepEqual([plain.status, plain.run, plain.probe, plain.dispatches], [0, "0", "not-run", 0]);
});

check("strict probe: a kernel with the strict modes triggers it, and the verdict decides whether the kernel runs", () => {
  const lazy = strictProbe("strict=1", 0, { kernel: strictModule });
  assert.deepEqual([lazy.status, lazy.run, lazy.probe, lazy.dispatches], [0, "0", "passed", 1]);
  const refused = strictProbe("strict=1,probe=3", 0, { kernel: strictModule });
  assert.deepEqual([refused.status, refused.run, refused.probe, refused.dispatches], [0, "2", "failed", 1]);
  const without = strictProbe("", 0, { kernel: strictModule });
  assert.deepEqual([without.run, without.probe, without.dispatches], ["2", "not-run", 0]);
});

check("strict probe: the harness mode conform prints every lane against the reference, on any device", () => {
  assert.match(mock("strict=1", ["conform"]).stdout, /conform lanes=24 different=0/);
  assert.match(mock("", ["conform"]).stdout, /conform lanes=24 different=0/, "it runs whatever the properties say");
  const bad = mock("strict=1,probe=5", ["conform"]);
  assert.match(bad.stdout, /conform lanes=24 different=1/);
  assert.match(bad.stdout, /lane  5 -\(a \* b\) .*DIFFERENT/);
});

check("loader: a library that cannot be loaded makes the backend unavailable, and no other library is tried", () => {
  for (const missing of ["/nonexistent/libvulkan.so.1", join(directory, "no-such-library")]) {
    const result = harness(["probe", "--features", "0"], { TSUZURI_VULKAN_LIBRARY: missing, TSUZURI_GPU_DEBUG: "1" });
    assert.equal(statusOf(result), 1, result.stdout + result.stderr);
    assert.match(result.stderr, /cannot be loaded/);
  }
});

check("loader: a library without the Vulkan entry points is unavailable (the wrong library)", () => {
  const other = process.platform === "darwin" ? "/usr/lib/libSystem.B.dylib" : "libc.so.6";
  const result = harness(["probe", "--features", "0"], { TSUZURI_VULKAN_LIBRARY: other, TSUZURI_GPU_DEBUG: "1" });
  assert.equal(statusOf(result), 1, result.stdout + result.stderr);
});

// ---- running through the mock ----
const expectedMock = (inputs, laneBytes) => inputs.map(value => laneBytes === 8 ? (BigInt(value) * 3n + 7n) & 0xffffffffffffffffn : (Math.imul(value, 3) + 7) >>> 0);
const runMock = (config, { mode = "map", lanes = "1,1", count, features = 0, extra = [], input }) => {
  const args = ["run", "--spirv", mockModule, "--mode", mode, "--lanes", lanes, "--count", String(count), "--features", String(features), "--output", join(directory, "out.bin"), ...extra];
  if (input) { writeFileSync(join(directory, "in.bin"), input); args.push("--input", join(directory, "in.bin")); }
  return mock(config, args);
};

check("run: map, init, 32- and 64-bit lanes, every count around the group size, direct and staged transfers", () => {
  for (const config of ["", "unified=0", "type=discrete", "max_groups=1", "max_groups=3,type=discrete"]) {
    for (const count of [1, 2, 255, 256, 257, 511, 512, 513, 1000, 4097]) {
      const inputs = sample(count, count);
      let result = runMock(config, { count, input: u32(inputs) });
      assert.equal(statusOf(result), 0, `[${config}] ${count}: ${result.stdout}${result.stderr}`);
      assert.deepEqual([...new Uint32Array(readFileSync(join(directory, "out.bin")).buffer.slice(0))], expectedMock(inputs, 4), `[${config}] map ${count}`);
      result = runMock(config, { mode: "init", lanes: "1,1", count });
      assert.equal(statusOf(result), 0, `[${config}] init ${count}: ${result.stdout}${result.stderr}`);
      assert.deepEqual([...new Uint32Array(readFileSync(join(directory, "out.bin")).buffer.slice(0))], expectedMock(Array.from({ length: count }, (_, index) => index), 4), `[${config}] init ${count}`);
      const wide = Array.from({ length: count }, (_, index) => BigInt(inputs[index]) << 17n | BigInt(index));
      result = runMock(config, { lanes: "4,4", count, input: u64(wide) });
      assert.equal(statusOf(result), 0, `[${config}] wide ${count}: ${result.stdout}${result.stderr}`);
      assert.deepEqual([...new BigUint64Array(readFileSync(join(directory, "out.bin")).buffer.slice(0))], expectedMock(wide, 8), `[${config}] wide ${count}`);
      result = runMock(config, { lanes: "1,4", count, input: u32(inputs) });
      assert.equal(statusOf(result), 0, `[${config}] 32 to 64 ${count}: ${result.stdout}${result.stderr}`);
      assert.deepEqual([...new BigUint64Array(readFileSync(join(directory, "out.bin")).buffer.slice(0))], expectedMock(inputs, 8), `[${config}] 32 to 64 ${count}`);
    }
  }
});

check("run: a count above the group-count limit is dispatched in chunks, each lane written once", () => {
  const count = 100003;
  const inputs = sample(count, 7);
  const result = runMock("max_groups=5", { count, input: u32(inputs) });
  assert.equal(statusOf(result), 0, result.stdout + result.stderr);
  assert.deepEqual([...new Uint32Array(readFileSync(join(directory, "out.bin")).buffer.slice(0))], expectedMock(inputs, 4));
});

check("run: limits and refusals have their own statuses", () => {
  const counts = (config, count, extra = {}) => statusOf(runMock(config, { count, input: count > 0 && count < 100000 ? u32(sample(count)) : undefined, ...extra }));
  assert.equal(counts("", 0), 0, "an empty call does nothing");
  assert.equal(counts("max_range=1024", 257, { input: u32(sample(257)) }), 3, "maxStorageBufferRange");
  assert.equal(counts("max_range=1024", 256, { input: u32(sample(256)) }), 0);
  assert.equal(counts("max_allocation=2048", 513, { input: u32(sample(513)) }), 3, "maxMemoryAllocationSize");
  assert.equal(counts("", 2147483648), 3, "more than 2147483647 lanes");
  assert.equal(counts("", -1), 3, "a negative count");
  assert.equal(statusOf(runMock("", { count: 8, lanes: "3,3", input: u32(sample(8)) })), 2, "f16 lanes have no Vulkan kernel");
  assert.equal(statusOf(runMock("", { count: 8, lanes: "9,1", input: u32(sample(8)) })), 2, "unknown lane kind");
  assert.equal(statusOf(runMock("", { count: 8, features: 4, input: u32(sample(8)) })), 2, "strict float on a device without the controls");
  assert.equal(statusOf(runMock("strict=1", { count: 8, features: 4, input: u32(sample(8)) })), 0, "strict float on a device with the controls");
  assert.equal(statusOf(runMock("strict=1,probe=3", { count: 8, features: 4, input: u32(sample(8)) })), 2, "strict float on a device that fails the conformance probe");
});

check("run: a module that needs what the device lacks is refused even when the features do not say so", () => {
  const wide = join(directory, "int64.spv");
  writeFileSync(wide, fakeModule([1, 11]));
  const run64 = config => mock(config, ["run", "--spirv", wide, "--mode", "map", "--lanes", "1,1", "--count", "8", "--features", "0", "--input", (writeFileSync(join(directory, "in.bin"), u32(sample(8))), join(directory, "in.bin"))]);
  assert.equal(statusOf(run64("int64=0")), 2);
  assert.equal(statusOf(run64("int64=1")), 0);
  const strict = join(directory, "strict.spv");
  writeFileSync(strict, fakeModule([1, 4464, 4466, 4467]));
  const runStrict = config => mock(config, ["run", "--spirv", strict, "--mode", "map", "--lanes", "1,1", "--count", "8", "--features", "0", "--input", join(directory, "in.bin")]);
  assert.equal(statusOf(runStrict("")), 2);
  assert.equal(statusOf(runStrict("strict=1")), 0);
  assert.equal(statusOf(runStrict("strict=1,probe=3")), 2, "the controls are reported but the conformance probe fails");
  const odd = join(directory, "odd.spv");
  writeFileSync(odd, fakeModule([1, 9999]));
  assert.equal(statusOf(mock("", ["run", "--spirv", odd, "--mode", "map", "--lanes", "1,1", "--count", "8", "--input", join(directory, "in.bin")])), 2, "an unknown capability");
  const garbage = join(directory, "garbage.spv");
  writeFileSync(garbage, Buffer.alloc(64, 0xAB));
  assert.equal(statusOf(mock("", ["run", "--spirv", garbage, "--mode", "map", "--lanes", "1,1", "--count", "8", "--input", join(directory, "in.bin")])), 2, "not SPIR-V");
  const truncated = fakeModule().subarray(0, 30);
  writeFileSync(join(directory, "truncated.spv"), truncated);
  assert.equal(statusOf(mock("", ["run", "--spirv", join(directory, "truncated.spv"), "--mode", "map", "--lanes", "1,1", "--count", "8", "--input", join(directory, "in.bin")])), 2, "a truncated module");
});

// ---- Gpu.Auto: is a call worth running on this backend? (the mock is an integrated GPU with unified memory by default) ----
const measured = { TSUZURI_GPU_AUTO_MIN_WORK: undefined };
const always = { TSUZURI_GPU_AUTO_MIN_WORK: "0" };
const ask = (config, { mode = "map", lanes = "1,1", count = 1, features = 0, weight = 1, repeat = 1, module = mockModule, env = measured } = {}) => {
  const result = mock(config, ["auto", "--spirv", module, "--mode", mode, "--lanes", lanes, "--count", String(count), "--features", String(features), "--weight", String(weight), "--repeat", String(repeat)], env);
  const parsed = /status=0 chosen=([01](?:,[01])*) opened=([01])/.exec(result.stdout);
  assert.ok(parsed, `${result.stdout}${result.stderr}`);
  assert.equal(result.status, 0, result.stderr);
  return { chosen: parsed[1].split(",").map(Number), opened: parsed[2] === "1", stderr: result.stderr };
};

check("auto: the measured cost rule keeps cheap work on the CPU reference and never opens the device for it", () => {
  for (const [count, weight] of [[1, 1], [1000, 5], [1000000, 5], [100000, 20]]) {
    const answer = ask("", { count, weight });
    assert.deepEqual(answer.chosen, [0], `${count} lanes of weight ${weight}`);
    assert.equal(answer.opened, false, "no device is opened for work that the CPU does faster");
  }
});

check("auto: work that outweighs the transfer, the open, and the compile opens the device and is chosen, again and again", () => {
  for (const [lanes, count] of [["1,1", 4000000], ["4,4", 2000000], ["1,4", 3000000]]) {
    const answer = ask("", { count, weight: 1280, lanes, repeat: 3 });
    assert.deepEqual(answer.chosen, [1, 1, 1], `${count} lanes ${lanes}`);
    assert.equal(answer.opened, true);
  }
  assert.deepEqual(ask("", { mode: "init", count: 4000000, weight: 1280 }).chosen, [1], "init has no input to move");
});

check("auto: calls that each save less than the first use costs pay it together, once, and then stay on the device", () => {
  // 100,000 lanes of weight 320: far cheaper on a warm device, but not by as much as opening it and building the pipeline.
  const answer = ask("", { count: 100000, weight: 320, repeat: 100 });
  const switched = answer.chosen.indexOf(1);
  assert.ok(switched > 0, `the first calls stay on the CPU reference: ${answer.chosen.join("")}`);
  assert.ok(answer.chosen.slice(switched).every(chosen => chosen === 1), "after the first use is paid every call is the device's");
  assert.equal(answer.opened, true);
  // Calls that the warm device would not do faster build no credit and never open it.
  const never = ask("", { count: 100000, weight: 20, repeat: 100 });
  assert.deepEqual(never.chosen, Array(100).fill(0));
  assert.equal(never.opened, false);
  // A kernel that the device cannot run is declined however much credit there is, and so is every call after it.
  assert.deepEqual(ask("int64=0", { count: 100000, weight: 320, features: 2, repeat: 100 }).chosen, Array(100).fill(0));
  assert.deepEqual(ask("", { count: 100000, weight: 320, features: 4, repeat: 100 }).chosen, Array(100).fill(0));
});

check("auto: TSUZURI_GPU_AUTO_MIN_WORK replaces the cost rule by a threshold on lanes times weight, and nothing else", () => {
  assert.deepEqual(ask("", { count: 1, weight: 1, env: always }).chosen, [1]);
  assert.deepEqual(ask("", { count: 100, weight: 50, env: { TSUZURI_GPU_AUTO_MIN_WORK: "5000" } }).chosen, [1], "5000 reaches 5000");
  assert.deepEqual(ask("", { count: 100, weight: 49, env: { TSUZURI_GPU_AUTO_MIN_WORK: "5000" } }).chosen, [0], "4900 does not");
  assert.deepEqual(ask("", { count: 4000000, weight: 1280, env: { TSUZURI_GPU_AUTO_MIN_WORK: "9999999999999" } }).chosen, [0], "a threshold above the work wins over the rule");
  for (const bad of ["abc", "-1", "5x"]) {
    const answer = ask("", { count: 1000, weight: 5, env: { TSUZURI_GPU_AUTO_MIN_WORK: bad, TSUZURI_GPU_DEBUG: "1" } });
    assert.deepEqual(answer.chosen, [0], `${bad}: the measured rule applies`);
    assert.match(answer.stderr, /TSUZURI_GPU_AUTO_MIN_WORK=.* is not a count/);
  }
  // The threshold does not relax what the device must be able to do.
  assert.deepEqual(ask("int64=0", { count: 1000, features: 2, env: always }).chosen, [0], "64-bit integers without shaderInt64");
  assert.deepEqual(ask("", { count: 1000, features: 4, env: always }).chosen, [0], "strict float without the controls");
  assert.deepEqual(ask("strict=1", { count: 1000, features: 4, env: always }).chosen, [1], "strict float with the controls");
  assert.deepEqual(ask("strict=1,probe=3", { count: 1000, features: 4, env: always }).chosen, [0], "strict float where the conformance probe fails");
  assert.deepEqual(ask("", { count: 1000, features: 1, env: always }).chosen, [1], "a feature bit of another backend is not this backend's");
  assert.deepEqual(ask("devices=0", { count: 1000, repeat: 3, env: always }).chosen, [0, 0, 0], "no device");
  assert.deepEqual(ask("no_version=1", { count: 1000, env: always }).chosen, [0], "no usable loader");
});

check("auto: only the measured kind of device is chosen, and only when the buffers fit and the pipeline builds", () => {
  for (const config of ["type=discrete", "type=cpu", "type=virtual", "type=other", "unified=0"]) {
    assert.deepEqual(ask(config, { count: 4000000, weight: 1280, env: always }).chosen, [0], config);
  }
  assert.deepEqual(ask("type=integrated", { count: 1000, env: always }).chosen, [1]);
  assert.deepEqual(ask("max_range=1024", { count: 1000, env: always }).chosen, [0], "4000 bytes exceed maxStorageBufferRange");
  assert.deepEqual(ask("max_range=1024", { count: 256, env: always }).chosen, [1]);
  assert.deepEqual(ask("max_allocation=2048", { count: 513, env: always }).chosen, [0], "maxMemoryAllocationSize");
  assert.deepEqual(ask("", { count: 2147483648, env: always }).chosen, [0], "more lanes than a call can have");
  assert.deepEqual(ask("", { count: 0, env: always }).chosen, [0], "an empty call has nothing to run");
  assert.deepEqual(ask("", { count: 8, lanes: "3,3", env: always }).chosen, [0], "f16 lanes have no Vulkan kernel");
  assert.deepEqual(ask("", { count: 8, lanes: "9,1", env: always }).chosen, [0], "an unknown lane kind");
  assert.deepEqual(ask("", { count: 8, lanes: "9,2", mode: "init", env: always }).chosen, [1], "init has no input lanes to check");
  const garbage = join(directory, "auto-garbage.spv");
  writeFileSync(garbage, Buffer.alloc(64, 0xAB));
  const wide = join(directory, "auto-int64.spv");
  writeFileSync(wide, fakeModule([1, 11]));
  const odd = join(directory, "auto-odd.spv");
  writeFileSync(odd, fakeModule([1, 9999]));
  const tiny = join(directory, "auto-tiny.spv");
  writeFileSync(tiny, Buffer.alloc(8));
  for (const [config, module, why] of [["", garbage, "not SPIR-V"], ["int64=0", wide, "a module that needs shaderInt64 whatever the features say"], ["", odd, "an unknown capability"], ["", tiny, "too short to be a module"]]) {
    assert.deepEqual(ask(config, { count: 1000, module, env: always }).chosen, [0], why);
  }
  assert.deepEqual(ask("int64=1", { count: 1000, module: wide, env: always }).chosen, [1]);
});

check("auto: a kernel that cannot be prepared is not chosen, and says why only under TSUZURI_GPU_DEBUG", () => {
  const garbage = join(directory, "auto-garbage.spv");
  const quiet = ask("", { count: 1000, module: garbage, env: always });
  assert.deepEqual(quiet.chosen, [0]);
  assert.equal(quiet.stderr, "", "the CPU reference serves the call without a message");
  const verbose = ask("", { count: 1000, module: garbage, env: { ...always, TSUZURI_GPU_DEBUG: "1" } });
  assert.match(verbose.stderr, /not a SPIR-V module/);
  const strict = ask("", { count: 1000, features: 4, env: { ...always, TSUZURI_GPU_DEBUG: "1" } });
  assert.deepEqual(strict.chosen, [0]);
  assert.match(strict.stderr, /strict float32 controls/);
});

check("auto: the Nth Vulkan call fails for every N: the answer is never Vulkan, and no object or allocation is left", () => {
  let passes = 0;
  for (const config of ["", "type=integrated,max_groups=1", "int64=0", "strict=1"]) {
    const result = mock(config, ["autosweep", "--spirv", mockModule, "--mode", "map", "--lanes", "1,1", "--count", "1000", "--features", config === "strict=1" ? "4" : "0", "--weight", "1"], always);
    const summary = /autosweep passes=(\d+) chosen=(\d+) failures=(\d+)/.exec(result.stdout);
    assert.ok(summary, `[${config}] ${result.stdout}${result.stderr}`);
    assert.equal(result.status, 0, `[${config}] ${result.stderr}`);
    assert.equal(Number(summary[2]), 1, `[${config}] the fault-free pass chooses Vulkan`);
    assert.equal(Number(summary[3]), 0);
    passes += Number(summary[1]);
  }
  assert.ok(passes > 40, `${passes} faulted passes`);
});

const sweep = (config, { mode = "map", lanes = "1,1", count = 1000, features = 0, input = u32(sample(count)) } = {}) => {
  const args = ["sweep", "--spirv", mockModule, "--mode", mode, "--lanes", lanes, "--count", String(count), "--features", String(features), "--output", join(directory, "sweep.bin")];
  if (mode === "map") { writeFileSync(join(directory, "sweep-in.bin"), input); args.push("--input", join(directory, "sweep-in.bin")); }
  const result = mock(config, args);
  const summary = /sweep passes=(\d+) ok=(\d+) unavailable=(\d+) unsupported=(\d+) limit=(\d+) failed=(\d+) failures=(\d+)/.exec(result.stdout);
  assert.ok(summary, result.stdout + result.stderr);
  assert.equal(result.status, 0, `[${config}] ${result.stderr.split("\n").filter(line => /^fault|misuse|live/.test(line)).slice(0, 12).join("\n")}`);
  assert.equal(Number(summary[7]), 0);
  return { passes: Number(summary[1]), ok: Number(summary[2]) };
};

check("faults: the Nth Vulkan call fails for every N: a documented status, no object or allocation left, recovery afterwards", () => {
  let passes = 0;
  for (const [config, options] of [
    ["", {}],
    ["", { mode: "init", count: 777 }],
    ["unified=0,type=discrete", {}],
    ["max_groups=1", { count: 1300 }],
    ["max_groups=3,unified=0", { lanes: "4,4", input: u64(Array.from({ length: 1000 }, (_, index) => index * 7)) }],
    ["strict=1", { features: 6 }],
    ["code=-2", {}],
    ["code=-4", {}],
    ["code=-3,unified=0,type=discrete", {}],
    ["portability=1,fc_ext=1,api=1.1,strict=1,denorm_independence=32,rounding_independence=32", { features: 6 }],
  ]) {
    const result = sweep(config, options);
    assert.equal(result.ok, 1, `[${config}] exactly the fault-free pass succeeds`);
    passes += result.passes;
  }
  assert.ok(passes > 250, `${passes} faulted passes`);
});

check("faults: the output of the fault-free pass is the output a plain run gives", () => {
  const inputs = sample(1000);
  sweep("", { input: u32(inputs) });
  assert.deepEqual([...new Uint32Array(readFileSync(join(directory, "sweep.bin")).buffer.slice(0))], expectedMock(inputs, 4));
});

check("threads: concurrent runs and the lazy initialization are serialized (mock, sanitizers)", () => {
  const inputs = sample(4000, 3);
  writeFileSync(join(directory, "threads-in.bin"), u32(inputs));
  const result = mock("max_groups=4", ["run", "--spirv", mockModule, "--mode", "map", "--lanes", "1,1", "--count", "4000", "--input", join(directory, "threads-in.bin"), "--output", join(directory, "threads-out.bin"), "--threads", "8", "--repeat", "20"]);
  assert.equal(statusOf(result), 0, result.stdout + result.stderr);
  assert.deepEqual([...new Uint32Array(readFileSync(join(directory, "threads-out.bin")).buffer.slice(0))], expectedMock(inputs, 4));
});

{
  // ThreadSanitizer cannot be combined with AddressSanitizer, so it has its own build of the harness and the mock.
  const tsanHarness = join(directory, "harness_tsan");
  const tsanMock = join(directory, `libvk_mock_tsan.${library}`);
  const built = run(clang, ["-std=c11", "-g", "-O1", "-fsanitize=thread", join(root, "tests/gpu_vulkan_runtime.c"), "-o", tsanHarness]);
  const builtMock = built.status === 0 ? run(clang, ["-std=gnu11", "-g", "-O1", "-ffp-contract=off", "-fsanitize=thread", "-Wno-unused-function", "-Wno-unused-variable", "-shared", "-fPIC", join(root, "tests/gpu_vulkan_mock.c"), "-o", tsanMock]) : built;
  if (builtMock.status !== 0) {
    skip("threads under ThreadSanitizer", `${clang} cannot build with -fsanitize=thread`);
  } else {
    check("threads: no data race in the runtime (ThreadSanitizer, mock)", () => {
      const result = run(tsanHarness, ["run", "--spirv", mockModule, "--mode", "map", "--lanes", "1,1", "--count", "4000", "--input", join(directory, "threads-in.bin"), "--output", join(directory, "threads-tsan.bin"), "--threads", "8", "--repeat", "20"], { env: { ...process.env, TSUZURI_VULKAN_LIBRARY: tsanMock, TZ_VK_MOCK: "max_groups=4", TSAN_OPTIONS: "halt_on_error=1" } });
      assert.equal(statusOf(result), 0, result.stdout + result.stderr);
      assert.doesNotMatch(result.stderr, /ThreadSanitizer/);
    });
  }
}

// ---- the real devices ----
const spirvAs = tool("spirv-as", ["/opt/homebrew/opt/spirv-tools/bin/spirv-as", "/usr/local/bin/spirv-as", "/usr/bin/spirv-as"]);
const spirvVal = tool("spirv-val", ["/opt/homebrew/opt/spirv-tools/bin/spirv-val", "/usr/local/bin/spirv-val", "/usr/bin/spirv-val"]);
const usedImplementations = new Set();

function assembly(wide) {
  const stride = wide ? 8 : 4;
  const text = `OpCapability Shader
${wide ? "OpCapability Int64" : ""}
OpMemoryModel Logical GLSL450
OpEntryPoint GLCompute %map_main "map_main" %gid
OpEntryPoint GLCompute %init_main "init_main" %gid
OpExecutionMode %map_main LocalSize 256 1 1
OpExecutionMode %init_main LocalSize 256 1 1
OpDecorate %gid BuiltIn GlobalInvocationId
OpDecorate %rt ArrayStride ${stride}
OpMemberDecorate %in_block 0 Offset 0
OpMemberDecorate %in_block 0 NonWritable
OpDecorate %in_block Block
OpMemberDecorate %out_block 0 Offset 0
OpDecorate %out_block Block
OpDecorate %in_var DescriptorSet 0
OpDecorate %in_var Binding 0
OpDecorate %out_var DescriptorSet 0
OpDecorate %out_var Binding 1
OpMemberDecorate %push 0 Offset 0
OpMemberDecorate %push 1 Offset 4
OpDecorate %push Block
%void = OpTypeVoid
%fn = OpTypeFunction %void
%u32 = OpTypeInt 32 0
%elem = OpTypeInt 64 0
%bool = OpTypeBool
%v3 = OpTypeVector %u32 3
%ptr_in = OpTypePointer Input %v3
%gid = OpVariable %ptr_in Input
%rt = OpTypeRuntimeArray %elem
%in_block = OpTypeStruct %rt
%out_block = OpTypeStruct %rt
%ptr_in_block = OpTypePointer StorageBuffer %in_block
%ptr_out_block = OpTypePointer StorageBuffer %out_block
%in_var = OpVariable %ptr_in_block StorageBuffer
%out_var = OpVariable %ptr_out_block StorageBuffer
%push = OpTypeStruct %u32 %u32
%ptr_push = OpTypePointer PushConstant %push
%pc = OpVariable %ptr_push PushConstant
%ptr_pc_u32 = OpTypePointer PushConstant %u32
%ptr_sb = OpTypePointer StorageBuffer %elem
%c0 = OpConstant %u32 0
%c1 = OpConstant %u32 1
%mul = OpConstant %elem 1664525
%add = OpConstant %elem 1013904223
%map_main = OpFunction %void None %fn
%e1 = OpLabel
%g1 = OpLoad %v3 %gid
%x1 = OpCompositeExtract %u32 %g1 0
%pb1 = OpAccessChain %ptr_pc_u32 %pc %c1
%b1 = OpLoad %u32 %pb1
%pl1 = OpAccessChain %ptr_pc_u32 %pc %c0
%l1 = OpLoad %u32 %pl1
%i1 = OpIAdd %u32 %x1 %b1
%in1 = OpULessThan %bool %i1 %l1
OpSelectionMerge %end1 None
OpBranchConditional %in1 %body1 %end1
%body1 = OpLabel
%pi1 = OpAccessChain %ptr_sb %in_var %c0 %i1
%v1 = OpLoad %elem %pi1
%m1 = OpIMul %elem %v1 %mul
%r1 = OpIAdd %elem %m1 %add
%po1 = OpAccessChain %ptr_sb %out_var %c0 %i1
OpStore %po1 %r1
OpBranch %end1
%end1 = OpLabel
OpReturn
OpFunctionEnd
%init_main = OpFunction %void None %fn
%e2 = OpLabel
%g2 = OpLoad %v3 %gid
%x2 = OpCompositeExtract %u32 %g2 0
%pb2 = OpAccessChain %ptr_pc_u32 %pc %c1
%b2 = OpLoad %u32 %pb2
%pl2 = OpAccessChain %ptr_pc_u32 %pc %c0
%l2 = OpLoad %u32 %pl2
%i2 = OpIAdd %u32 %x2 %b2
%in2 = OpULessThan %bool %i2 %l2
OpSelectionMerge %end2 None
OpBranchConditional %in2 %body2 %end2
%body2 = OpLabel
${wide ? "%w2 = OpUConvert %elem %i2" : "%w2 = OpCopyObject %elem %i2"}
%m2 = OpIMul %elem %w2 %mul
%r2 = OpIAdd %elem %m2 %add
%po2 = OpAccessChain %ptr_sb %out_var %c0 %i2
OpStore %po2 %r2
OpBranch %end2
%end2 = OpLabel
OpReturn
OpFunctionEnd
`;
  // The 32-bit module uses %u32 for the lane: a type is declared once.
  return wide ? text : text.replace("%elem = OpTypeInt 64 0\n", "").replaceAll("%elem", "%u32");
}

const modules = {};
function assemble(wide) {
  const name = wide ? "wide" : "narrow";
  if (!modules[name]) {
    const source = join(directory, `${name}.spvasm`);
    writeFileSync(source, assembly(wide));
    const output = join(directory, `${name}.spv`);
    const result = run(spirvAs, ["--target-env", "spv1.3", source, "-o", output]);
    assert.equal(result.status, 0, result.stderr);
    if (spirvVal) assert.equal(run(spirvVal, ["--target-env", "vulkan1.1", output]).status, 0);
    modules[name] = output;
  }
  return modules[name];
}

const hostMul = (value, wide) => wide ? (BigInt(value) * 1664525n + 1013904223n) & 0xffffffffffffffffn : (Math.imul(value, 1664525) + 1013904223) >>> 0;
const device = (name, body) => {
  if (!spirvAs) return skip(name, "spirv-as was not found", "TSUZURI_REQUIRE_SPIRV_TOOLS");
  for (const implementation of implementations) {
    current = implementation;
    const probe = harness(["probe", "--features", "0"], { TSUZURI_GPU_DEBUG: "1" });
    if (statusOf(probe) !== 0) {
      skip(`${name} [${implementation.label}]`, `no usable Vulkan device (status ${statusOf(probe)}): ${(probe.stderr + probe.stdout).trim().split("\n").pop()}`, "TSUZURI_REQUIRE_VULKAN");
      continue;
    }
    usedImplementations.add(`${implementation.label}: ${/device="([^"]*)"/.exec(probe.stdout)?.[1] ?? "?"}`);
    check(`${name} [${implementation.label}]`, body);
  }
  current = implementations[0];
};
const realRun = (module, args) => harness(["run", "--spirv", module, ...args, "--output", join(directory, "real-out.bin")]);

device("device: the runtime computes a 32-bit kernel (map, init, direct and staged), every count around the group size", () => {
  const module = assemble(false);
  for (const staged of [[], ["--staged"]]) {
    for (const count of [1, 255, 256, 257, 1000, 100003]) {
      const inputs = sample(count, count + 11);
      writeFileSync(join(directory, "real-in.bin"), u32(inputs));
      let result = realRun(module, ["--mode", "map", "--lanes", "1,1", "--count", String(count), "--input", join(directory, "real-in.bin"), ...staged]);
      assert.equal(statusOf(result), 0, result.stdout + result.stderr);
      assert.deepEqual([...new Uint32Array(readFileSync(join(directory, "real-out.bin")).buffer.slice(0))], inputs.map(value => hostMul(value, false)));
      result = realRun(module, ["--mode", "init", "--lanes", "1,1", "--count", String(count), ...staged]);
      assert.equal(statusOf(result), 0, result.stdout + result.stderr);
      assert.deepEqual([...new Uint32Array(readFileSync(join(directory, "real-out.bin")).buffer.slice(0))], Array.from({ length: count }, (_, index) => hostMul(index, false)));
    }
  }
});

device("device: the runtime computes a 64-bit kernel when the device has shaderInt64", () => {
  const int64 = harness(["probe", "--features", "2"]);
  if (statusOf(int64) !== 0) { assert.equal(statusOf(realRun(assemble(true), ["--mode", "map", "--lanes", "4,4", "--count", "4", "--features", "2"])), 2); return; }
  const module = assemble(true);
  for (const staged of [[], ["--staged"]]) {
    for (const count of [1, 257, 5000]) {
      const inputs = Array.from({ length: count }, (_, index) => (BigInt(sample(count, 5)[index]) << 32n) | BigInt(sample(count, 6)[index]));
      writeFileSync(join(directory, "real-in.bin"), u64(inputs));
      let result = realRun(module, ["--mode", "map", "--lanes", "4,4", "--count", String(count), "--features", "2", "--input", join(directory, "real-in.bin"), ...staged]);
      assert.equal(statusOf(result), 0, result.stdout + result.stderr);
      assert.deepEqual([...new BigUint64Array(readFileSync(join(directory, "real-out.bin")).buffer.slice(0))], inputs.map(value => hostMul(value, true)));
      result = realRun(module, ["--mode", "init", "--lanes", "1,4", "--count", String(count), "--features", "2", ...staged]);
      assert.equal(statusOf(result), 0, result.stdout + result.stderr);
      assert.deepEqual([...new BigUint64Array(readFileSync(join(directory, "real-out.bin")).buffer.slice(0))], Array.from({ length: count }, (_, index) => hostMul(index, true)));
    }
  }
});

device("device: repeated and concurrent runs leave nothing behind (sanitizers, allocation counts)", () => {
  const module = assemble(false);
  const inputs = sample(20000, 99);
  writeFileSync(join(directory, "real-in.bin"), u32(inputs));
  let result = realRun(module, ["--mode", "map", "--lanes", "1,1", "--count", "20000", "--input", join(directory, "real-in.bin"), "--repeat", "40"]);
  assert.equal(statusOf(result), 0, result.stdout + result.stderr);
  assert.match(result.stdout, /live_allocations=1\b/, "only the cached pipeline is live during the process");
  result = realRun(module, ["--mode", "map", "--lanes", "1,1", "--count", "20000", "--input", join(directory, "real-in.bin"), "--threads", "4", "--repeat", "10", "--staged"]);
  assert.equal(statusOf(result), 0, result.stdout + result.stderr);
  assert.deepEqual([...new Uint32Array(readFileSync(join(directory, "real-out.bin")).buffer.slice(0))], inputs.map(value => hostMul(value, false)));
});

const strictNotes = [];
device("device: the conformance probe agrees with what the device reports about the strict float32 controls", () => {
  const reported = harness(["probe", "--features", "0"]).stdout;
  const name = /device="([^"]*)"/.exec(reported)?.[1] ?? "?";
  const propertiesSay = /strict_f32=1/.test(reported);
  const conform = harness(["conform", "--features", "0"]);
  const lanes = /conform lanes=24 different=(\d+)/.exec(conform.stdout);
  if (/float_controls=1/.test(reported)) assert.ok(lanes, `a device that reports float controls runs the probe module: ${conform.stdout}${conform.stderr}`);
  const differing = lanes ? [...conform.stdout.matchAll(/^lane +(\d+) .*DIFFERENT$/gm)].map(match => Number(match[1])) : undefined;
  assert.equal(differing?.length ?? 0, lanes ? Number(lanes[1]) : 0);
  const strict = harness(["probe", "--features", "4"]);
  const probe = /strict_probe=(\S+)/.exec(strict.stdout)?.[1];
  if (propertiesSay) {
    assert.equal(statusOf(strict), probe === "passed" ? 0 : 2, strict.stdout + strict.stderr);
    if (probe === "passed") assert.deepEqual(differing, [], "the verdict of the runtime and the lanes of the probe agree");
    if (probe === "failed" && lanes) assert.ok(differing.length > 0, "the runtime refused the device and the lanes differ");
  } else {
    assert.equal(statusOf(strict), 2);
    assert.equal(probe, "not-run", "a device that does not report the controls is never probed by the runtime");
  }
  strictNotes.push(`${name}: ${propertiesSay ? "reports the strict float32 controls" : "does not report them"}, probe ${probe}${differing?.length ? ` (lanes that differ: ${differing.join(", ")})` : ""}`);
});

device("device: an unloadable library, an unknown ICD, and a bad module are refused with their statuses", () => {
  const module = assemble(false);
  writeFileSync(join(directory, "real-in.bin"), u32(sample(8)));
  const args = ["run", "--spirv", module, "--mode", "map", "--lanes", "1,1", "--count", "8", "--input", join(directory, "real-in.bin"), "--output", join(directory, "real-out.bin")];
  assert.equal(statusOf(harness(args, { TSUZURI_VULKAN_LIBRARY: "/nonexistent/libvulkan" })), 1);
  const disabled = harness(args, { TSUZURI_VULKAN_LIBRARY: "", TSUZURI_GPU_DEBUG: "1" });
  assert.equal(statusOf(disabled), 1, "an empty TSUZURI_VULKAN_LIBRARY disables the backend, like the WebGPU one");
  assert.match(disabled.stderr, /TSUZURI_VULKAN_LIBRARY is empty/);
  const icd = harness(args, { VK_DRIVER_FILES: join(directory, "no-such-icd.json"), VK_ICD_FILENAMES: join(directory, "no-such-icd.json"), TSUZURI_GPU_DEBUG: "1" });
  assert.equal(statusOf(icd), 1, icd.stdout + icd.stderr);
  writeFileSync(join(directory, "bad.spv"), Buffer.alloc(40, 7));
  assert.equal(statusOf(harness(["run", "--spirv", join(directory, "bad.spv"), "--mode", "map", "--lanes", "1,1", "--count", "8", "--input", join(directory, "real-in.bin")])), 2);
});

rmSync(directory, { recursive: true, force: true });
if (strictNotes.length) console.log(`strict float32 on the real devices (the drivers' own verdicts, informational): ${strictNotes.join("; ")}`);
const total = passed + failed;
console.log(`GPU Vulkan runtime: ${passed} of ${total} checks passed${failed ? `, ${failed} FAILED` : ""}, ${skips.length} skipped${skips.length ? ` (${skips.map(entry => entry.split(":")[0]).join("; ")})` : ""}; real devices: ${usedImplementations.size ? [...usedImplementations].join(", ") : "none available"}; sanitizers: ${flags === sanitize ? "on" : "off"}`);
process.exit(failed ? 1 : 0);
