import assert from "node:assert/strict";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir, totalmem } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { pathToFileURL } from "node:url";
import { createGpuImports } from "../src/runtime/webgpu.mjs";
import { fakeProvider } from "./gpu_runtime_provider.mjs";

// F09 Phase 2: `Gpu.request Gpu.WebGpu` in the language runtime. The loader paths (no library, a wrong version, a
// library that lacks functions, a library in the working directory), the failures of the native runtime (a C host,
// tests/gpu_runtime_host.c, drives it against a fake wgpu-native, tests/gpu_runtime_fake.c, that is dangerous in the way
// that the real one is: it hands out invalid objects and aborts when one is submitted), the pointers of the WASM host,
// and the opt-in WASM imports run everywhere; the dispatch checks need a real WebGPU implementation and run with
// TSUZURI_WEBGPU=1. TSUZURI_SANITIZE=1 builds the C side with AddressSanitizer and UBSan.
//   native: wgpu-native 29 (TSUZURI_WEBGPU_LIBRARY names it when it is not on the default search path)
//   WASM:   the `webgpu` binding of TSUZURI_WEBGPU_MODULE under Node.js 24 or newer (JavaScript Promise Integration)
const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const clang = process.env.TSUZURI_CLANG ?? "clang";
const directory = mkdtempSync(join(tmpdir(), "tsuzuri-gpu-runtime-"));
const useGpu = process.env.TSUZURI_WEBGPU === "1";
const windows = process.platform === "win32";
const executable = windows ? ".exe" : "";
const notes = [];

function execute(program, args, { env = {}, success = true, timeout = 300000, cwd } = {}) {
  const merged = { ...process.env, ...env };
  for (const key of Object.keys(merged)) if (merged[key] === undefined) delete merged[key];
  const result = spawnSync(program, args, { encoding: "utf8", timeout, cwd, maxBuffer: 8 * 1024 * 1024, env: merged });
  assert.ifError(result.error);
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}
const cli = (args, success) => execute(compiler, args, { success });
const project = (name, source) => {
  const path = join(directory, name);
  mkdirSync(path);
  writeFileSync(join(path, "Main.tz"), source);
  return path;
};
const trapped = result => result.status !== 0;
// With TSUZURI_GPU_DEBUG the runtime traces each dispatch as `map|init <lanes> lanes, kinds 0x<out><in>` (1 is a 32-bit
// integer lane, 2 f32, 3 f16): the count of dispatches per kind shows that the device ran the kernels.
const dispatched = text => {
  const tally = {};
  for (const [, , , kinds] of text.matchAll(/^tsuzuri gpu: webgpu: (map|init) (\d+) lanes, kinds 0x([0-9a-f]{4})$/gm)) tally[kinds] = (tally[kinds] ?? 0) + 1;
  return tally;
};
// The sweeps use 8 nonzero lane counts (a zero count dispatches nothing).
const sweepDispatches = { "0101": 32, "0102": 8, "0201": 16, "0202": 8 };
const halfDispatches = { "0201": 8, "0203": 8, "0301": 16, "0302": 8, "0303": 8 };

// Every call below is a kernel of its own call site: strict i32 and i32u lanes (bit for bit with the CPU reference)
// and relaxed f32 lanes (compared with the reference within 1e-5, or exactly for an integer result).
const common = `def mix :: i32 -> i32
fn mix value = (value * 1664525 + 1013904223) ^ (Bits.ushr value 13)

def scramble :: i32u -> i32u
fn scramble value = value * 1664525i32u + 1013904223i32u

def poly :: f32 -> f32
fn poly value = ((value * 0.5 + 0.25) * value + 0.125) * value + 1.0

def above :: f32 -> i32
fn above value = if value > 1.0 then 1 else 0

def sample :: i32 -> f32
fn sample index = (index as f32) * 0.015625 + 0.5

def near :: f32 -> f32 -> f32 -> bool
fn near tolerance actual expected = Math.abs (actual - expected) <= tolerance * Math.abs expected

def available :: bool
fn available = Result.is_ok (&(Gpu.request Gpu.WebGpu))

def strict_mismatches :: i32 -> i32
fn strict_mismatches count = {
    let reference = Result.get (Gpu.request Gpu.CpuReference);
    let device = Result.get (Gpu.request Gpu.WebGpu);
    let expected = Gpu.to_array (Gpu.map (&reference) mix (Gpu.init (&reference) (count as i64) (\\index -> index * 3 - 7)));
    let actual = Gpu.to_array (Gpu.map (&device) mix (Gpu.init (&device) (count as i64) (\\index -> index * 3 - 7)));
    let mut wrong = 0;
    for index in 0i64 .. ((count as i64) - 1) do
        if actual[index] != expected[index] then wrong = wrong + 1 else ();
    wrong
}

def unsigned_mismatches :: i32 -> i32
fn unsigned_mismatches count = {
    let reference = Result.get (Gpu.request Gpu.CpuReference);
    let device = Result.get (Gpu.request Gpu.WebGpu);
    let expected = Gpu.to_array (Gpu.map (&reference) scramble (Gpu.init (&reference) (count as i64) (\\index -> (index as i32u) * 2654435761i32u)));
    let actual = Gpu.to_array (Gpu.map (&device) scramble (Gpu.init (&device) (count as i64) (\\index -> (index as i32u) * 2654435761i32u)));
    let mut wrong = 0;
    for index in 0i64 .. ((count as i64) - 1) do
        if actual[index] != expected[index] then wrong = wrong + 1 else ();
    wrong
}

def relaxed_mismatches :: i32 -> i32
fn relaxed_mismatches count = {
    let reference = Result.get (Gpu.request Gpu.CpuReference);
    let device = Result.get (Gpu.request Gpu.WebGpu);
    let expected = Gpu.to_array (Gpu.map_relaxed (&reference) poly (Gpu.init_relaxed (&reference) (count as i64) sample));
    let actual = Gpu.to_array (Gpu.map_relaxed (&device) poly (Gpu.init_relaxed (&device) (count as i64) sample));
    let mut wrong = 0;
    for index in 0i64 .. ((count as i64) - 1) do
        if near 0.00001 actual[index] expected[index] then () else wrong = wrong + 1;
    wrong
}

def threshold_mismatches :: i32 -> i32
fn threshold_mismatches count = {
    let reference = Result.get (Gpu.request Gpu.CpuReference);
    let device = Result.get (Gpu.request Gpu.WebGpu);
    let expected = Gpu.to_array (Gpu.map_relaxed (&reference) above (Gpu.init_relaxed (&reference) (count as i64) sample));
    let actual = Gpu.to_array (Gpu.map_relaxed (&device) above (Gpu.init_relaxed (&device) (count as i64) sample));
    let mut wrong = 0;
    for index in 0i64 .. ((count as i64) - 1) do
        if actual[index] != expected[index] then wrong = wrong + 1 else ();
    wrong
}

def sweep :: i32
fn sweep = {
    let counts = [0, 1, 3, 255, 256, 257, 1000, 4097, 70000];
    let mut wrong = 0;
    for position in 0i64 .. 8i64 do
        wrong = wrong + strict_mismatches counts[position] + unsigned_mismatches counts[position] + relaxed_mismatches counts[position] + threshold_mismatches counts[position];
    wrong
}

def strict_float :: f32 -> f32
fn strict_float value = {
    let device = Result.get (Gpu.request Gpu.WebGpu);
    let values = [value];
    let mapped = Gpu.map (&device) (\\item -> item * item + item) (Gpu.from_array (&device) (&values));
    let result = Gpu.to_array mapped;
    result[0]
}
`;
// f16 lanes need the shader-f16 device feature, so the program that uses them is separate: its request is Unavailable
// on an adapter without the feature, which says nothing about the programs above.
const half = `def near :: f32 -> f32 -> f32 -> bool
fn near tolerance actual expected = Math.abs (actual - expected) <= tolerance * Math.abs expected

def sample :: i32 -> f32
fn sample index = (index as f32) * 0.015625 + 0.5

def half_sample :: i32 -> f16
fn half_sample index = ((index & 255) as f16) * 0.25f16 + 0.5f16

def half_square :: f16 -> f16
fn half_square value = value * value + value

def half_widen :: f16 -> f32
fn half_widen value = (value as f32) * 3.0 + 0.25

def half_narrow :: f32 -> f16
fn half_narrow value = (value * 0.5 + 0.25) as f16

def half_available :: bool
fn half_available = Result.is_ok (&(Gpu.request Gpu.WebGpu))

def half_mismatches :: i32 -> i32
fn half_mismatches count = {
    let reference = Result.get (Gpu.request Gpu.CpuReference);
    let device = Result.get (Gpu.request Gpu.WebGpu);
    let expected = Gpu.to_array (Gpu.map_relaxed (&reference) half_square (Gpu.init_relaxed (&reference) (count as i64) half_sample));
    let actual = Gpu.to_array (Gpu.map_relaxed (&device) half_square (Gpu.init_relaxed (&device) (count as i64) half_sample));
    let wide_expected = Gpu.to_array (Gpu.map_relaxed (&reference) half_widen (Gpu.init_relaxed (&reference) (count as i64) half_sample));
    let wide_actual = Gpu.to_array (Gpu.map_relaxed (&device) half_widen (Gpu.init_relaxed (&device) (count as i64) half_sample));
    let narrow_expected = Gpu.to_array (Gpu.map_relaxed (&reference) half_narrow (Gpu.init_relaxed (&reference) (count as i64) sample));
    let narrow_actual = Gpu.to_array (Gpu.map_relaxed (&device) half_narrow (Gpu.init_relaxed (&device) (count as i64) sample));
    let mut wrong = 0;
    for index in 0i64 .. ((count as i64) - 1) do {
        if near 0.004 (actual[index] as f32) (expected[index] as f32) then () else wrong = wrong + 1;
        if near 0.000001 wide_actual[index] wide_expected[index] then () else wrong = wrong + 1;
        if near 0.002 (narrow_actual[index] as f32) (narrow_expected[index] as f32) then () else wrong = wrong + 1
    };
    wrong
}

def half_sweep :: i32
fn half_sweep = {
    let counts = [0, 1, 3, 255, 256, 257, 1000, 4097, 70000];
    let mut wrong = 0;
    for position in 0i64 .. 8i64 do
        wrong = wrong + half_mismatches counts[position];
    wrong
}
`;
const nativeSources = {
  sweep: `${common}\nif available() then sweep() else -1\n`,
  strictFloat: `${common}\nstrict_float 3.0\n`,
  half: `${half}\nif half_available() then half_sweep() else -1\n`,
};
const wasmSource = `${common}
export def wasm_available :: bool
fn wasm_available = available()

export def wasm_sweep :: i32
fn wasm_sweep = if available() then sweep() else -1

export def wasm_strict_float :: f32 -> f32
fn wasm_strict_float value = strict_float value
`;
const wasmHalfSource = `${half}
export def wasm_half_sweep :: i32
fn wasm_half_sweep = if half_available() then half_sweep() else -1
`;
// Kernels that once produced WGSL that Tint and Naga reject: a mutable parameter (WGSL parameters are immutable) and
// an else-if chain at the deepest nesting that Tint accepts (62 arms; a 63rd arm is E1017 in a relaxed call, and a strict
// call has no kernel then). Their results are exact, so the device is compared bit for bit.
const arms = (count, literal) => Array.from({ length: count }, (_, arm) => `if value == ${literal(arm)} then ${literal(100 + arm)} else `).join("");
const shapes = `def bump :: i32 -> i32
fn bump mut x = { x = x + 1; x * 2 }

def bump_float :: f32 -> f32
fn bump_float mut x = { x = x * 2.0 + 1.0; x * 0.5 }

def chain62 :: i32 -> i32
fn chain62 value = ${arms(62, String)}0

def chain63 :: i32 -> i32
fn chain63 value = ${arms(63, String)}0

def sample :: i32 -> f32
fn sample index = (index as f32) * 0.015625 + 0.5

def shapes_available :: bool
fn shapes_available = Result.is_ok (&(Gpu.request Gpu.WebGpu))

def shape_mismatches :: i32 -> i32
fn shape_mismatches count = {
    let reference = Result.get (Gpu.request Gpu.CpuReference);
    let device = Result.get (Gpu.request Gpu.WebGpu);
    let expected = Gpu.to_array (Gpu.map (&reference) bump (Gpu.init (&reference) (count as i64) (\\index -> index * 3 - 7)));
    let actual = Gpu.to_array (Gpu.map (&device) bump (Gpu.init (&device) (count as i64) (\\index -> index * 3 - 7)));
    let chained = Gpu.to_array (Gpu.init (&reference) (count as i64) chain62);
    let chain_actual = Gpu.to_array (Gpu.init (&device) (count as i64) chain62);
    let floats = Gpu.to_array (Gpu.map_relaxed (&reference) bump_float (Gpu.init_relaxed (&reference) (count as i64) sample));
    let float_actual = Gpu.to_array (Gpu.map_relaxed (&device) bump_float (Gpu.init_relaxed (&device) (count as i64) sample));
    let mut wrong = 0;
    for index in 0i64 .. ((count as i64) - 1) do {
        if actual[index] != expected[index] then wrong = wrong + 1 else ();
        if chain_actual[index] != chained[index] then wrong = wrong + 1 else ();
        if float_actual[index] != floats[index] then wrong = wrong + 1 else ()
    };
    wrong
}

def shapes_sweep :: i32
fn shapes_sweep = {
    let counts = [0, 1, 3, 255, 256, 257, 1000, 4097, 70000];
    let mut wrong = 0;
    for position in 0i64 .. 8i64 do
        wrong = wrong + shape_mismatches counts[position];
    wrong
}

def deep63 :: i32
fn deep63 = {
    let device = Result.get (Gpu.request Gpu.WebGpu);
    let values = [5];
    let mapped = Gpu.map (&device) chain63 (Gpu.from_array (&device) (&values));
    (Gpu.to_array mapped)[0]
}
`;
const shapeDispatches = { "0101": 24, "0201": 8, "0202": 8 };
const wasmShapesSource = `${shapes}
export def wasm_shapes_sweep :: i32
fn wasm_shapes_sweep = if shapes_available() then shapes_sweep() else -1

export def wasm_deep63 :: i32
fn wasm_deep63 = deep63()
`;
nativeSources.shapes = `${shapes}\nif shapes_available() then shapes_sweep() else -1\n`;
nativeSources.deep = `${shapes}\ndeep63()\n`;
// The largest call that a device with the default limits takes (65,535 workgroups of 256 invocations), and one lane more,
// which the runtime refuses with status 3 before it makes a buffer.
nativeSources.edge = "let device = Result.get (Gpu.request Gpu.WebGpu)\nlet values = Gpu.to_array (Gpu.init (&device) 16776960 (\\index -> index * 2))\nvalues[16776959]\n";
nativeSources.over = "let device = Result.get (Gpu.request Gpu.WebGpu)\nlet values = Gpu.to_array (Gpu.init (&device) 16776961 (\\index -> index * 2))\nvalues[16776960]\n";

try {
  // ---- Native: the runtime of src/runtime/gpu.c loads wgpu-native at run time --------------------------------------
  const builds = {};
  for (const optimization of [0, 3]) {
    for (const [name, source] of Object.entries(nativeSources)) {
      if (optimization === 3 && name !== "sweep" && name !== "half" && name !== "shapes") continue;
      const path = project(`${name}-${optimization}`, source);
      const output = join(path, `program${executable}`);
      cli(["build", path, `-O${optimization}`, "-o", output]);
      builds[`${name}-${optimization}`] = output;
    }
  }
  const sweep = builds["sweep-0"];
  // The same program twice, the same IR: the kernels of the call sites are embedded deterministically.
  const first = join(directory, "first.ll");
  const second = join(directory, "second.ll");
  cli(["build", join(directory, "sweep-0"), "--emit", "llvm", "-o", first]);
  cli(["build", join(directory, "sweep-0"), "--emit", "llvm", "-o", second]);
  assert.deepEqual(readFileSync(first), readFileSync(second));
  assert.match(readFileSync(first, "utf8"), /tz\.gpu\.kernels/);

  // No library: the backend is disabled, so the program is told Unavailable and runs no kernel (no CPU substitute).
  const disabled = execute(sweep, [], { env: { TSUZURI_WEBGPU_LIBRARY: "", TSUZURI_GPU_DEBUG: "1" } });
  assert.equal(disabled.stdout.trim(), "-1");
  assert.match(disabled.stderr, /TSUZURI_WEBGPU_LIBRARY is empty; the backend is disabled/);
  const missing = execute(sweep, [], { env: { TSUZURI_WEBGPU_LIBRARY: join(directory, "no-such-library"), TSUZURI_GPU_DEBUG: "1" } });
  assert.equal(missing.stdout.trim(), "-1");
  assert.match(missing.stderr, /no WebGPU library \(wgpu-native\) could be loaded/);
  assert.equal(execute(sweep, [], { env: { TSUZURI_WEBGPU_LIBRARY: join(directory, "no-such-library") } }).stderr, "", "a failed open is silent without TSUZURI_GPU_DEBUG");
  assert.equal(execute(builds["half-0"], [], { env: { TSUZURI_WEBGPU_LIBRARY: "" } }).stdout.trim(), "-1");
  assert.equal(execute(builds["shapes-0"], [], { env: { TSUZURI_WEBGPU_LIBRARY: "" } }).stdout.trim(), "-1");
  // The embedded kernels of the shapes: a mutable parameter is a variable of the body, the 62-arm chain is embedded, and
  // the strict 63-arm call has no kernel (its last arm, 162, is in no embedded text).
  {
    const ir = join(directory, "shapes.ll");
    cli(["build", join(directory, "shapes-0"), "--emit", "llvm", "-o", ir]);
    const text = readFileSync(ir, "utf8");
    assert.match(text, /var local_\d+: i32 = param_\d+;/);
    assert.match(text, /var local_\d+: f32 = param_\d+;/);
    assert.ok(text.includes("bitcast<i32>(161u)") && !text.includes("bitcast<i32>(162u)"));
  }
  // A relaxed call with 63 arms is refused at compile time (E1017), like `--emit wgsl-relaxed`.
  const refused63 = project("deep-relaxed", `def chain :: f32 -> f32\nfn chain value = ${arms(63, number => `${number}.0`)}0.0\nlet device = Result.get (Gpu.request Gpu.WebGpu)\nlet values: [f32] = [1.0]\nGpu.to_array (Gpu.map_relaxed (&device) chain (Gpu.from_array (&device) (&values)))\n`);
  assert.equal(JSON.parse(cli(["build", refused63, "--json", "-o", join(refused63, "never")], false).stderr).code, "E1017");

  // Libraries that are not wgpu-native 29 fail closed with a reason.
  if (windows) {
    notes.push("mock libraries skipped on Windows");
  } else {
    const extension = process.platform === "darwin" ? "dylib" : "so";
    const mock = (name, defines) => {
      const output = join(directory, `${name}.${extension}`);
      execute(clang, ["-shared", "-fPIC", ...defines, resolve("tests/gpu_runtime_mock.c"), "-o", output]);
      return output;
    };
    const environment = library => ({ TSUZURI_WEBGPU_LIBRARY: library, TSUZURI_GPU_DEBUG: "1" });
    const refused = (library, pattern) => {
      const result = execute(sweep, [], { env: environment(library) });
      assert.equal(result.stdout.trim(), "-1");
      assert.match(result.stderr, pattern);
    };
    refused(mock("older", ["-DMOCK_VERSION=0x1C000000u"]), /wgpu-native 28\.0\.0\.0 is not supported; the runtime needs major version 29/);
    refused(mock("newer", ["-DMOCK_VERSION=0x1E000000u"]), /wgpu-native 30\.0\.0\.0 is not supported/);
    refused(mock("other", ["-DMOCK_NO_VERSION"]), /the library has no wgpuGetVersion; only wgpu-native 29 is supported/);
    refused(mock("incomplete", ["-DMOCK_VERSION=0x1D000101u"]), /the library lacks wgpu/);
    // A build from source (Homebrew's wgpu-native) reports version 0: the user vouches for it by naming it. The
    // libraries that the runtime finds by itself are not trusted that way (see the host builds below).
    const unversioned = mock("unversioned", ["-DMOCK_VERSION=0u"]);
    refused(unversioned, /the library lacks wgpu/);

    // ---- The runtime driven directly: a C host (tests/gpu_runtime_host.c) built with src/runtime/gpu.c ---------------
    // The failures that a language program only sees as a trap show up here as statuses, and a process that wgpu-native
    // aborts shows as a failed run. TSUZURI_SANITIZE=1 builds the runtime, the host, and the fake with AddressSanitizer
    // and UBSan. NDEBUG is defined on every command line, as the bundled zig cc does from -O1 up: the C files undefine
    // it for their own assertions.
    const runtimeSource = resolve("src/runtime/gpu.c");
    const hostSource = resolve("tests/gpu_runtime_host.c");
    const sanitize = process.env.TSUZURI_SANITIZE === "1";
    const sanitizerEnv = sanitize ? { ASAN_OPTIONS: "detect_stack_use_after_return=1", UBSAN_OPTIONS: "halt_on_error=1:print_stacktrace=1" } : {};
    const compile = (output, sources, flags = []) => {
      execute(clang, [...sources, "-o", output, "-std=gnu11", "-Wall", "-DNDEBUG", ...(sanitize ? ["-fsanitize=address,undefined", "-fno-omit-frame-pointer", "-g"] : []), ...flags, "-lm", "-pthread", ...(process.platform === "linux" ? ["-ldl"] : [])]);
      return output;
    };
    const fake = compile(join(directory, `fake.${extension}`), [resolve("tests/gpu_runtime_fake.c")], ["-shared", "-fPIC", "-Wno-unused-function", "-Wno-unused-variable"]);
    // The runtime gives up on a device after a minute, which a test cannot wait for: this host builds it with 400 ms.
    const host = compile(join(directory, "host"), [hostSource, runtimeSource], ["-DTZ_WGPU_TIMEOUT_MS=400"]);
    const quiet = { TSUZURI_GPU_DEBUG: "", ...sanitizerEnv };
    const withFake = (injection = "") => ({ ...quiet, TSUZURI_WEBGPU_LIBRARY: fake, FAKE_WGPU: injection, FAKE_WGPU_REPORT: "1" });
    const alive = text => /^fake wgpu: alive (.*)$/m.exec(text)?.[1];
    const released = "instances=0 adapters=0 devices=0 queues=0 buffers=0 modules=0 pipelines=0 layouts=0 groups=0 encoders=0 passes=0 commands=0";
    for (const scenario of ["basic", "half", "failures", "limits"]) {
      const result = execute(host, [scenario], { env: withFake(), timeout: 60000 });
      assert.match(result.stdout, new RegExp(`^${scenario}: ok$`, "m"));
      assert.equal(alive(result.stderr), released, `${scenario}: everything that the runtime made was released`);
    }
    // Faults, each in a process of its own. A call that fails ends with a status (4, or 3 over a limit), never with a
    // submit of an object that the library marked invalid (which aborts the process), and the next call fails or works
    // as the device allows. `recover` fails once and then works; `refuse` fails every time.
    const recover = ["run", "map", "300", "4", "map", "300", "0", "init", "300", "0"];
    const refuse = ["run", "map", "300", "4", "init", "300", "4", "map", "300", "4"];
    for (const [injection, args, reason] of [
      ["fail-buffer=1", recover, /the buffer could not be created/],
      ["fail-buffer=2", recover, /the buffer could not be created/],
      ["fail-buffer=3", recover, /the buffer could not be created/],
      ["fail-buffer=4", recover, /the buffer could not be created/],
      ["fail-bindgroup", refuse, /the bind group is invalid/],
      ["fail-pipeline", refuse, /the pipeline failed/],
      ["fail-finish", refuse, /the command buffer is invalid/],
      // The device is lost during the first call; the second never reaches the queue (a submit on a lost device panics).
      ["lose-device", ["run", "map", "300", "4", "map", "300", "4", "init", "10", "4"], /the device was lost/],
      // The device's limits, which are small here, and not the adapter's, which are large: the largest call is accepted.
      ["max-groups=40", ["run", "map", "10240", "0", "init", "10240", "0", "map", "10241", "3", "init", "10241", "3", "map", "300", "0"], /exceeds the device limit/],
      ["max-buffer=4096", ["run", "map", "1024", "0", "init", "1025", "3", "map", "1025", "3", "map", "300", "0"], /exceeds the device limit/],
    ]) {
      const result = execute(host, args, { env: withFake(injection), timeout: 60000 });
      assert.match(result.stdout, /^sequence: ok$/m, injection);
      assert.match(result.stderr, reason, injection);
      assert.equal(alive(result.stderr), released, `${injection}: everything that the runtime made was released`);
    }
    // A device that does not answer. A blocking poll cannot be given up on, so the runtime never makes one: the call
    // ends with status 4 after the timeout, the device is not asked again, and the process ends (the release of a stuck
    // device would wait for it).
    const stalled = execute(host, ["stall"], { env: withFake("stall-map"), timeout: 20000 });
    assert.match(stalled.stdout, /^stall: ok$/m);
    assert.match(stalled.stderr, /the device did not finish the kernel in time/);
    assert.match(stalled.stderr, /the device did not answer in time; it is not used again/);
    const unanswered = execute(host, ["open"], { env: withFake("stall-adapter"), success: false, timeout: 20000 });
    assert.equal(unanswered.status, 1, "an adapter that never answers is Unavailable");
    // The instance is released, which completes the request that was given up on: its callback finds no wait that it belongs to.
    assert.equal(alive(unanswered.stderr), released);

    // A library in the working directory is never loaded by default: dlopen of a bare name searches it on macOS, and the
    // initializers of a planted library would run in every program that asks for a device. Loading is seen by a file
    // that the library creates when it is loaded.
    const planted = join(directory, "planted");
    mkdirSync(planted);
    const marker = join(planted, "loaded");
    const bare = `libwgpu_native.${extension}`;
    execute(clang, ["-shared", "-fPIC", "-DMOCK_VERSION=0x1D000101u", "-DMOCK_MARKER", resolve("tests/gpu_runtime_mock.c"), "-o", join(planted, bare)]);
    const production = compile(join(directory, "host-production"), [hostSource, runtimeSource]);
    const searching = { ...quiet, TSUZURI_WEBGPU_LIBRARY: undefined, TSUZURI_GPU_DEBUG: "1", MOCK_LOAD_MARKER: marker };
    const ignored = execute(production, ["open"], { env: searching, cwd: planted, success: false });
    assert.ok(!existsSync(marker), `a library in the working directory was loaded: ${ignored.stderr}`);
    assert.ok([1, 2].includes(ignored.status), "the open ends as Unavailable or unsupported, whatever the machine has installed");
    // The same library, named explicitly, is loaded (the marker works), and an incomplete one is refused with a reason.
    const named = execute(production, ["open"], { env: { ...searching, TSUZURI_WEBGPU_LIBRARY: join(planted, bare) }, cwd: planted, success: false });
    assert.ok(existsSync(marker) && named.status === 2 && /the library lacks wgpu/.test(named.stderr), named.stderr);
    rmSync(marker);
    // A default candidate is an absolute path: a name without a directory is never handed to dlopen, also when a build
    // lists one (TZ_GPU_WGPU_DEFAULTS replaces the list).
    const relative = compile(join(directory, "host-relative"), [hostSource, runtimeSource], [`-DTZ_GPU_WGPU_DEFAULTS="${bare}"`]);
    const refusedName = execute(relative, ["open"], { env: searching, cwd: planted, success: false });
    assert.ok(!existsSync(marker), "a candidate without a directory was loaded");
    assert.equal(refusedName.status, 1);
    assert.match(refusedName.stderr, /no WebGPU library \(wgpu-native\) could be loaded/);
    // A library found by the search is not trusted to be wgpu-native 29 when it reports no version.
    const searched = compile(join(directory, "host-searched"), [hostSource, runtimeSource], [`-DTZ_GPU_WGPU_DEFAULTS="${unversioned}"`]);
    const found = execute(searched, ["open"], { env: { ...quiet, TSUZURI_WEBGPU_LIBRARY: undefined, TSUZURI_GPU_DEBUG: "1" }, success: false });
    assert.equal(found.status, 2);
    assert.match(found.stderr, /the library reports no version; set TSUZURI_WEBGPU_LIBRARY to use a build of wgpu-native 29/);
    notes.push("a C host drove the runtime against a fake wgpu-native: 10 injected faults end in statuses (no submit of an invalid object), a stalled device is given up on and not asked again, the limits come from the device, no library is loaded from the working directory");

    // A user call of the device-aware std functions would name a kernel of its own: they are private to the Gpu module.
    const forged = project("forged", "def negate :: i32 -> i32\nfn negate value = 0 - value\nlet device = Result.get (Gpu.request Gpu.WebGpu)\nlet values = [10, 20, 30]\nlet mapped = Gpu.map_on (&device) negate (Gpu.from_array (&device) (&values)) 0\n(Gpu.to_array mapped)[0]\n");
    const refusal = JSON.parse(cli(["build", forged, "--json", "-o", join(forged, "never")], false).stderr);
    assert.equal(refusal.code, "E1022");
    assert.match(refusal.message, /private name 'Gpu\.map_on' is only visible inside module 'Gpu'/);

    // The lowering of Gpu.__run compares the lane kinds of the kernel that it was numbered with against the element types of
    // the call, and a kernel with other lanes is a call without a kernel: the host sizes its copies of the arrays by the
    // kinds, so a mismatch would read and write past them. The compiler numbers every call itself, so the descriptor is
    // patched here: the f16 output of the kernel becomes a 32-bit output, which is twice the size of what the call allocates.
    const lanes = project("lanes", "def half_index :: i32 -> f16\nfn half_index value = value as f16\nexport def half_length :: i32\nfn half_length = {\n    let device = Result.get (Gpu.request Gpu.WebGpu);\n    let values = Gpu.to_array (Gpu.init_relaxed (&device) 4 half_index);\n    values.length as i32\n}\n");
    cli(["build", lanes, "--emit", "llvm", "-o", join(lanes, "intact.ll")]);
    const text = readFileSync(join(lanes, "intact.ll"), "utf8");
    // The descriptor is `{ flags, lanes, features, wgsl, wgsl_length, spirv, spirv_length, weight }` (F09 Phase 3 added the last
    // three); a program that names WebGPU only has no SPIR-V and no weight (null, 0, 0).
    const descriptor = /(\{ i32 \d+, i32 )769(, i32 \d+, ptr @tz\.gpu\.kernel\.0\.wgsl, i32 \d+, ptr null, i32 0, i32 0 \})/;
    assert.match(text, descriptor);
    writeFileSync(join(lanes, "patched.ll"), text.replace(descriptor, "$1257$2"));
    writeFileSync(join(lanes, "host.c"), "#include <stdint.h>\n#include <stdio.h>\nextern int32_t tz_half_length(void);\nint main(void) { printf(\"%d\\n\", (int)tz_half_length()); return 0; }\n");
    const runLanes = name => execute(compile(join(lanes, name), [join(lanes, `${name}.ll`), join(lanes, "host.c"), runtimeSource], ["-O0", "-Wno-override-module"]), [], { env: withFake(), success: false });
    const intact = runLanes("intact");
    assert.equal(intact.status, 0, intact.stderr);
    assert.equal(intact.stdout.trim(), "4");
    const patched = runLanes("patched");
    assert.notEqual(patched.status, 0, "the call with a kernel of other lanes trapped");
    assert.equal(patched.stdout, "");
    assert.match(patched.stderr, /this call has no WGSL kernel/);
    notes.push("a kernel whose lanes do not match the call's element types is never run, and Gpu.map_on and Gpu.init_on cannot be written by a user (E1022)");

    if (useGpu) {
      // The real wgpu-native (TSUZURI_WEBGPU_LIBRARY names it): the same scenarios, with a broken shader, a missing entry
      // point and a bind group that does not fit its layout, which wgpu-native only reports as errors, and the largest
      // call that a device with the default limits takes (65,535 workgroups of 256 invocations).
      const real = compile(join(directory, "host-real"), [hostSource, runtimeSource]);
      for (const scenario of ["basic", "half", "failures", "limits", "maximum"]) {
        const result = execute(real, [scenario], { env: quiet, timeout: 120000 });
        assert.match(result.stdout, new RegExp(`^${scenario}: (ok|skipped.*)$`, "m"), result.stderr);
      }
      notes.push("the C host ran against the real wgpu-native: a broken shader, a missing entry point and a bad bind group end in status 4 (twice each), 16,776,961 lanes end in status 3, and 16,776,960 lanes run");
    }
  }

  if (useGpu) {
    for (const optimization of [0, 3]) {
      const real = execute(builds[`sweep-${optimization}`], [], { env: { TSUZURI_GPU_DEBUG: "1" } });
      assert.equal(real.stdout.trim(), "0", `native -O${optimization}: ${real.stderr}`);
      assert.deepEqual(dispatched(real.stderr), sweepDispatches);
    }
    // A strict float kernel on a WebGPU device is never run (there is no silent CPU or relaxed run): it traps.
    const strict = execute(builds["strictFloat-0"], [], { success: false });
    assert.ok(trapped(strict));
    assert.equal(strict.stdout, "");
    assert.match(strict.stderr, /this call has no WGSL kernel: a strict Gpu\.map or Gpu\.init runs on a WebGPU device only with i32 or i32u lanes; use Gpu\.map_relaxed or Gpu\.init_relaxed for f32 or f16/);
    notes.push("native WebGPU dispatch matched the CPU reference (strict i32 and i32u bit for bit, relaxed f32 within 1e-5) at -O0 and -O3");
    // f16 lanes: the device is opened with shader-f16 or the request is Unavailable, with the reason.
    let halfRan = false;
    for (const optimization of [0, 3]) {
      const result = execute(builds[`half-${optimization}`], [], { env: { TSUZURI_GPU_DEBUG: "1" } });
      if (result.stdout.trim() === "-1") {
        assert.match(result.stderr, /the adapter lacks shader-f16/);
      } else {
        assert.equal(result.stdout.trim(), "0", `native f16 -O${optimization}: ${result.stderr}`);
        assert.deepEqual(dispatched(result.stderr), halfDispatches);
        halfRan = true;
      }
    }
    notes.push(halfRan ? "native f16 lanes (shader-f16) matched the CPU reference within 4 ulp" : "the native adapter lacks shader-f16, so the f16 request was Unavailable");
    // Kernels with a mutable parameter and 62 else-if arms (Tint's nesting limit) run on the device; a strict call with 63 arms has no kernel.
    for (const optimization of [0, 3]) {
      const result = execute(builds[`shapes-${optimization}`], [], { env: { TSUZURI_GPU_DEBUG: "1" } });
      assert.equal(result.stdout.trim(), "0", `native shapes -O${optimization}: ${result.stderr}`);
      assert.deepEqual(dispatched(result.stderr), shapeDispatches);
    }
    const deep = execute(builds["deep-0"], [], { success: false });
    assert.ok(trapped(deep));
    assert.equal(deep.stdout, "");
    assert.match(deep.stderr, /this call has no WGSL kernel/);
    notes.push("native kernels with a mutable parameter and 62 else-if arms matched the CPU reference bit for bit, and a strict call with 63 arms trapped");
    // The limits are the device's: the largest call runs, and one lane more is refused before any buffer is made (status 3,
    // which the program sees as a trap that says why).
    const edge = execute(builds["edge-0"], []);
    assert.equal(edge.stdout.trim(), String(16776959 * 2));
    const over = execute(builds["over-0"], [], { success: false });
    assert.ok(trapped(over));
    assert.equal(over.stdout, "");
    assert.match(over.stderr, /the number of lanes exceeds the device limit/);
    notes.push("native Gpu.init of 16,776,960 lanes ran on the device and 16,776,961 lanes trapped on the device limit");
  } else {
    notes.push("native WebGPU dispatch not requested");
  }

  // ---- WASM: the default module has no imports; --wasm-feature webgpu adds exactly tsuzuri_gpu.open and run ---------
  const wasmProject = project("wasm", wasmSource);
  const wasmHalfProject = project("wasm-half", wasmHalfSource);
  const wasmShapesProject = project("wasm-shapes", wasmShapesSource);
  const modules = {};
  for (const optimization of [0, 3]) {
    const plain = join(wasmProject, `plain-${optimization}.wasm`);
    const opted = join(wasmProject, `webgpu-${optimization}.wasm`);
    const halved = join(wasmHalfProject, `webgpu-${optimization}.wasm`);
    const shaped = join(wasmShapesProject, `webgpu-${optimization}.wasm`);
    cli(["build", wasmProject, "--target", "wasm32", `-O${optimization}`, "-o", plain]);
    cli(["build", wasmProject, "--target", "wasm32", "--wasm-feature", "webgpu", `-O${optimization}`, "-o", opted]);
    cli(["build", wasmHalfProject, "--target", "wasm32", "--wasm-feature", "webgpu", `-O${optimization}`, "-o", halved]);
    cli(["build", wasmShapesProject, "--target", "wasm32", "--wasm-feature", "webgpu", `-O${optimization}`, "-o", shaped]);
    const unused = join(wasmProject, `unused-${optimization}.wasm`);
    const pure = project(`pure-${optimization}`, "export def double :: i32 -> i32\nfn double value = value * 2\n");
    cli(["build", pure, "--target", "wasm32", "--wasm-feature", "webgpu", `-O${optimization}`, "-o", unused]);
    assert.deepEqual(WebAssembly.Module.imports(new WebAssembly.Module(readFileSync(plain))), [], "the default WASM output of a program with a WebGPU request has no imports");
    assert.deepEqual(WebAssembly.Module.imports(new WebAssembly.Module(readFileSync(opted))).map(({ module, name, kind }) => `${module}.${name}:${kind}`).sort(), ["tsuzuri_gpu.open:function", "tsuzuri_gpu.run:function"]);
    assert.deepEqual(WebAssembly.Module.imports(new WebAssembly.Module(readFileSync(unused))), [], "a program with no GPU device adds no imports under the flag");
    modules[optimization] = { plain, opted, halved, shaped };
  }
  // Without the feature the request is Unavailable and the module runs on its own.
  for (const optimization of [0, 3]) {
    const { exports } = new WebAssembly.Instance(new WebAssembly.Module(readFileSync(modules[optimization].plain)));
    assert.equal(exports.tz_wasm_available(), 0);
    assert.equal(exports.tz_wasm_sweep(), -1);
    assert.throws(() => exports.tz_wasm_strict_float(3), WebAssembly.RuntimeError, "Result.get of the Unavailable request traps; no kernel runs elsewhere");
  }

  if (typeof WebAssembly.Suspending !== "function") {
    notes.push("JSPI checks skipped (WebAssembly.Suspending needs Node.js 24 or newer)");
  } else {
    // The imports suspend the module while the host awaits the adapter: call the exports through WebAssembly.promising.
    const instantiate = (path, provider, debug = false) => {
      const host = createGpuImports(provider, () => instance.exports.memory, { debug });
      const module = new WebAssembly.Module(readFileSync(path));
      const instance = new WebAssembly.Instance(module, { tsuzuri_gpu: host.imports });
      return { host, call: name => WebAssembly.promising(instance.exports[name]) };
    };
    const capture = async action => {
      const original = console.error;
      const lines = [];
      console.error = (...parts) => lines.push(parts.join(" "));
      try { return { value: await action(), lines }; } catch (error) { return { error, lines }; } finally { console.error = original; }
    };
    const none = { requestAdapter: async () => null };
    for (const optimization of [0, 3]) {
      const { host, call } = instantiate(modules[optimization].opted, none);
      assert.equal(await call("tz_wasm_available")(), 0);
      assert.equal(await call("tz_wasm_sweep")(), -1);
      await assert.rejects(call("tz_wasm_strict_float")(3), WebAssembly.RuntimeError);
      await host.close();
      const halved = instantiate(modules[optimization].halved, none);
      assert.equal(await halved.call("tz_wasm_half_sweep")(), -1);
      await halved.host.close();
      const shaped = instantiate(modules[optimization].shaped, none);
      assert.equal(await shaped.call("tz_wasm_shapes_sweep")(), -1);
      await shaped.host.close();
    }
    // ---- Pointers above 2 GiB: a module with a large --wasm-max-memory passes them as negative i32 values ------------------
    {
      let memory;
      try { memory = new WebAssembly.Memory({ initial: 36000 }); } catch { /* the engine has no room for 2.2 GiB */ }
      if (!memory) {
        notes.push("the high-pointer checks were skipped (no room for 2.2 GiB of WebAssembly memory)");
      } else {
        const signed = address => address | 0;
        const text = new TextEncoder().encode("fn map_main() {}\nfn init_main() {}\n");
        const wgsl = 2 ** 31 + 4096;
        const output = 2 ** 31 + 65536;
        new Uint8Array(memory.buffer, wgsl, text.length).set(text);
        const host = createGpuImports(fakeProvider(), () => memory);
        assert.equal(await host.functions.open(1, 0), 0);
        const run = (count, address = output) => capture(() => host.functions.run(1, 1, 0, 0x0101, signed(wgsl), text.length, 0, 0, 0, BigInt(count), signed(address)));
        // The kernel text and the result lie above the signed range: the lanes arrive in the output.
        const high = await run(1000);
        assert.equal(high.value, 0, high.lines.join("\n"));
        assert.deepEqual(Array.from(new Int32Array(memory.buffer, output, 1000)), Array.from({ length: 1000 }, (_, index) => index));
        // A buffer that does not fit in the memory of the module is a failure (4), and a genuine limit is 3.
        const outside = await run(1000, memory.buffer.byteLength - 100);
        assert.equal(outside.value, 4);
        assert.match(outside.lines.join("\n"), /lies outside the memory of the module/);
        const limit = await run(100000000, 65536);
        assert.equal(limit.value, 3);
        assert.match(limit.lines.join("\n"), /exceeds the kernel or device limit/);
        await host.close();
        // A RangeError of another kind (here a failed allocation) is a failure, not a limit.
        const failing = createGpuImports(fakeProvider({ failOnBuffer: true }), () => memory);
        assert.equal(await failing.functions.open(1, 0), 0);
        const allocation = await capture(() => failing.functions.run(1, 1, 0, 0x0101, signed(wgsl), text.length, 0, 0, 0, 1000n, signed(output)));
        assert.equal(allocation.value, 4);
        assert.match(allocation.lines.join("\n"), /Array buffer allocation failed/);
        await failing.close();
        notes.push("the WASM host reads pointers above 2 GiB as unsigned, reports a buffer outside the memory as a failure (4), and only a real limit as status 3");
      }
      // The same in a real module: a 2.4 GB array moves the heap above 2 GiB, so the result of Gpu.init lies there.
      if (totalmem() >= 12 * 2 ** 30) {
        const highProject = project("wasm-high", "export def wasm_high :: i32\nfn wasm_high = {\n    let device = Result.get (Gpu.request Gpu.WebGpu);\n    let filler = Array.init 600000000 (\\index -> (index as i32) * 7);\n    let values = Gpu.to_array (Gpu.init (&device) 1000 (\\index -> index));\n    values[999] + filler[(values[1] as i64)]\n}\n");
        const highModule = join(highProject, "high.wasm");
        cli(["build", highProject, "--target", "wasm32", "--wasm-feature", "webgpu", "--wasm-max-memory", "3500MiB", "-O3", "-o", highModule]);
        const { host, call } = instantiate(highModule, fakeProvider());
        const result = await capture(() => call("tz_wasm_high")());
        assert.equal(result.value, 999 + 7, `${result.error ?? ""}\n${result.lines.join("\n")}`);
        await host.close();
        notes.push("a WASM module with a 2.4 GB array ran Gpu.init above 2 GiB");
      } else {
        notes.push("the 2.4 GB array test was skipped (less than 12 GiB of memory)");
      }
    }
    if (useGpu) {
      const binding = await import(process.env.TSUZURI_WEBGPU_MODULE ?? pathToFileURL(resolve("target/webgpu-runtime/node_modules/webgpu/index.js")).href);
      Object.assign(globalThis, binding.globals);
      let halfRan = false;
      for (const optimization of [0, 3]) {
        const provider = binding.create(process.platform === "darwin" ? ["backend=metal"] : []);
        const { host, call } = instantiate(modules[optimization].opted, provider, true);
        assert.equal(await call("tz_wasm_available")(), 1);
        const swept = await capture(() => call("tz_wasm_sweep")());
        assert.equal(swept.value, 0, `WASM -O${optimization}: ${swept.error ?? ""}`);
        assert.deepEqual(dispatched(swept.lines.map(line => `${line}\n`).join("")), sweepDispatches);
        const strict = await capture(() => call("tz_wasm_strict_float")(3));
        assert.ok(strict.error instanceof WebAssembly.RuntimeError);
        assert.match(strict.lines.join("\n"), /this call has no WGSL kernel: a strict Gpu\.map or Gpu\.init runs on a WebGPU device only with i32 or i32u lanes/);
        await host.close();
        const halved = instantiate(modules[optimization].halved, binding.create(process.platform === "darwin" ? ["backend=metal"] : []), true);
        const result = await capture(() => halved.call("tz_wasm_half_sweep")());
        if (result.value === -1) {
          assert.match(result.lines.join("\n"), /lacks the required feature shader-f16/);
        } else {
          assert.equal(result.value, 0, `WASM f16 -O${optimization}: ${result.error ?? ""}`);
          assert.deepEqual(dispatched(result.lines.map(line => `${line}\n`).join("")), halfDispatches);
          halfRan = true;
        }
        await halved.host.close();
        // The shapes: a mutable parameter and 62 else-if arms run on Dawn bit for bit, and the strict call with 63 arms traps.
        const shaped = instantiate(modules[optimization].shaped, binding.create(process.platform === "darwin" ? ["backend=metal"] : []), true);
        const sweptShapes = await capture(() => shaped.call("tz_wasm_shapes_sweep")());
        assert.equal(sweptShapes.value, 0, `WASM shapes -O${optimization}: ${sweptShapes.error ?? ""}\n${sweptShapes.lines.join("\n")}`);
        assert.deepEqual(dispatched(sweptShapes.lines.map(line => `${line}\n`).join("")), shapeDispatches);
        const deep = await capture(() => shaped.call("tz_wasm_deep63")());
        assert.ok(deep.error instanceof WebAssembly.RuntimeError);
        assert.match(deep.lines.join("\n"), /this call has no WGSL kernel/);
        await shaped.host.close();
      }
      notes.push("WASM WebGPU dispatch under JSPI matched the CPU reference at -O0 and -O3");
      notes.push("WASM kernels with a mutable parameter and 62 else-if arms matched the CPU reference bit for bit");
      notes.push(halfRan ? "WASM f16 lanes (shader-f16) matched the CPU reference within 4 ulp" : "the WASM adapter lacks shader-f16, so the f16 request was Unavailable");
    } else {
      notes.push("WASM WebGPU dispatch not requested");
    }
    notes.push("WASM imports ran under JSPI");
  }
  console.log(`GPU phase 2: device-aware kernels embedded deterministically, Unavailable without a library or with a wrong one, no imports without --wasm-feature webgpu; ${notes.join("; ")}`);
} finally {
  rmSync(directory, { recursive: true, force: true });
}
