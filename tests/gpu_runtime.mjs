import assert from "node:assert/strict";
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { pathToFileURL } from "node:url";
import { createGpuImports } from "../src/runtime/webgpu.mjs";

// F09 Phase 2: `Gpu.request Gpu.WebGpu` in the language runtime. The loader paths (no library, a wrong version, a
// library that lacks functions) and the opt-in WASM imports run everywhere; the dispatch checks need a real WebGPU
// implementation and run with TSUZURI_WEBGPU=1:
//   native: wgpu-native 29 (TSUZURI_WEBGPU_LIBRARY names it when it is not on the default search path)
//   WASM:   the `webgpu` binding of TSUZURI_WEBGPU_MODULE under Node.js 24 or newer (JavaScript Promise Integration)
const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const clang = process.env.TSUZURI_CLANG ?? "clang";
const directory = mkdtempSync(join(tmpdir(), "tsuzuri-gpu-runtime-"));
const useGpu = process.env.TSUZURI_WEBGPU === "1";
const windows = process.platform === "win32";
const executable = windows ? ".exe" : "";
const notes = [];

function execute(program, args, { env = {}, success = true } = {}) {
  const merged = { ...process.env, ...env };
  for (const key of Object.keys(merged)) if (merged[key] === undefined) delete merged[key];
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 300000, maxBuffer: 8 * 1024 * 1024, env: merged });
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

try {
  // ---- Native: the runtime of src/runtime/gpu.c loads wgpu-native at run time --------------------------------------
  const builds = {};
  for (const optimization of [0, 3]) {
    for (const [name, source] of Object.entries(nativeSources)) {
      if (optimization === 3 && name !== "sweep" && name !== "half") continue;
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
    // A build from source (Homebrew's wgpu-native) reports version 0: the user vouches for it by naming it, and a
    // library found on the search path is not trusted that way.
    const unversioned = mock("unversioned", ["-DMOCK_VERSION=0u"]);
    refused(unversioned, /the library lacks wgpu/);
    const search = join(directory, "search");
    mkdirSync(search);
    copyFileSync(unversioned, join(search, `libwgpu_native.${extension}`));
    const found = execute(sweep, [], { env: { TSUZURI_WEBGPU_LIBRARY: undefined, TSUZURI_GPU_DEBUG: "1", [process.platform === "darwin" ? "DYLD_LIBRARY_PATH" : "LD_LIBRARY_PATH"]: search } });
    assert.equal(found.stdout.trim(), "-1");
    assert.match(found.stderr, /the library reports no version; set TSUZURI_WEBGPU_LIBRARY to use a build of wgpu-native 29/);
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
  } else {
    notes.push("native WebGPU dispatch not requested");
  }

  // ---- WASM: the default module has no imports; --wasm-feature webgpu adds exactly tsuzuri_gpu.open and run ---------
  const wasmProject = project("wasm", wasmSource);
  const wasmHalfProject = project("wasm-half", wasmHalfSource);
  const modules = {};
  for (const optimization of [0, 3]) {
    const plain = join(wasmProject, `plain-${optimization}.wasm`);
    const opted = join(wasmProject, `webgpu-${optimization}.wasm`);
    const halved = join(wasmHalfProject, `webgpu-${optimization}.wasm`);
    cli(["build", wasmProject, "--target", "wasm32", `-O${optimization}`, "-o", plain]);
    cli(["build", wasmProject, "--target", "wasm32", "--wasm-feature", "webgpu", `-O${optimization}`, "-o", opted]);
    cli(["build", wasmHalfProject, "--target", "wasm32", "--wasm-feature", "webgpu", `-O${optimization}`, "-o", halved]);
    const unused = join(wasmProject, `unused-${optimization}.wasm`);
    const pure = project(`pure-${optimization}`, "export def double :: i32 -> i32\nfn double value = value * 2\n");
    cli(["build", pure, "--target", "wasm32", "--wasm-feature", "webgpu", `-O${optimization}`, "-o", unused]);
    assert.deepEqual(WebAssembly.Module.imports(new WebAssembly.Module(readFileSync(plain))), [], "the default WASM output of a program with a WebGPU request has no imports");
    assert.deepEqual(WebAssembly.Module.imports(new WebAssembly.Module(readFileSync(opted))).map(({ module, name, kind }) => `${module}.${name}:${kind}`).sort(), ["tsuzuri_gpu.open:function", "tsuzuri_gpu.run:function"]);
    assert.deepEqual(WebAssembly.Module.imports(new WebAssembly.Module(readFileSync(unused))), [], "a program with no GPU device adds no imports under the flag");
    modules[optimization] = { plain, opted, halved };
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
      }
      notes.push("WASM WebGPU dispatch under JSPI matched the CPU reference at -O0 and -O3");
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
