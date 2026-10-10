// Vulkan and Gpu.Auto through the language (F09 Phase 3). Programs that name Gpu.Vulkan or Gpu.Auto are built by hand (the heap
// of the Tsuzuri code tracked, the runtime under AddressSanitizer and UndefinedBehaviorSanitizer when the compiler has them)
// and by the driver, run on the Vulkan implementation of this machine, and compared with the CPU reference. What the machine
// reports decides what must happen: the harness of tests/gpu_vulkan_runtime.c probes the device first, so the expectations are
// derived from it and never assumed.
//
//   node tests/gpu_vulkan_language.mjs [target/release/tsuzuri]
//
// TSUZURI_VULKAN_TEST_ICDS (a path list) repeats the device checks on other Vulkan drivers through VK_DRIVER_FILES. A check that
// cannot run prints SKIPPED with its reason and is counted in the summary line; TSUZURI_REQUIRE_VULKAN=1 turns the absence of a
// Vulkan device into a failure.
import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join, resolve } from "node:path";
import { spawnSync } from "node:child_process";

if (process.platform === "win32") {
  console.log("GPU Vulkan language: SKIPPED (the Windows loader path is verified by CI type checks only)");
  process.exit(0);
}

const root = resolve(import.meta.dirname, "..");
const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const clang = process.env.TSUZURI_CLANG ?? "clang";
const directory = mkdtempSync(join(tmpdir(), "tsuzuri-gpu-vulkan-language-"));
const skips = [];
// TSUZURI_VULKAN_ONLY=<regular expression> runs only the checks whose name matches (while one is being worked on).
const only = process.env.TSUZURI_VULKAN_ONLY ? new RegExp(process.env.TSUZURI_VULKAN_ONLY) : undefined;
let passed = 0;
let failed = 0;

function execute(program, args, { env = {}, success = true } = {}) {
  const merged = { ...process.env, ...env };
  for (const key of Object.keys(merged)) if (merged[key] === undefined) delete merged[key];
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 600000, maxBuffer: 16 * 1024 * 1024, env: merged });
  assert.ifError(result.error);
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}
const cli = (args, success) => execute(compiler, args, { success });
function check(name, body) {
  if (only && !only.test(name)) return;
  try {
    body();
    passed++;
    console.log(`ok ${name}`);
  } catch (error) {
    failed++;
    console.log(`FAILED ${name}\n${error.stack ?? error}`);
  }
}
function skip(name, reason, requirement) {
  console.log(`SKIPPED ${name}: ${reason}`);
  skips.push(`${name}: ${reason}`);
  if (requirement && process.env[requirement]) {
    console.log(`FAILED ${name}: ${requirement} is set but the check was skipped`);
    failed++;
  }
}

// ---- what the machine reports ----
const sanitizers = ["-fsanitize=address,undefined", "-fno-sanitize-recover=undefined", "-g"];
const sanitize = (() => {
  const source = join(directory, "probe.c");
  writeFileSync(source, "int main(void) { return 0; }\n");
  return execute(clang, [...sanitizers, source, "-o", join(directory, "probe")], { success: false }).status === 0 ? sanitizers : [];
})();
if (!sanitize.length) skip("sanitizers", `${clang} cannot build with -fsanitize=address,undefined; the hand-built runs have none`);
const harnessPath = join(directory, "harness");
execute(clang, ["-std=c11", "-O1", "-g", join(root, "tests/gpu_vulkan_runtime.c"), "-o", harnessPath]);
const implementations = [
  { label: "default", env: {} },
  ...(process.env.TSUZURI_VULKAN_TEST_ICDS ?? "").split(delimiter).filter(Boolean).map((manifest, index) => ({ label: `icd${index}`, env: { VK_DRIVER_FILES: manifest, VK_ICD_FILENAMES: manifest } })),
];
function probe(env) {
  const ask = features => execute(harnessPath, ["probe", "--features", String(features)], { env: { ...env, TSUZURI_GPU_DEBUG: "1" }, success: false });
  const result = ask(0);
  const status = Number(/status=(\d+)/.exec(result.stdout)?.[1] ?? NaN);
  const field = name => new RegExp(`${name}=(\\S+)`).exec(result.stdout)?.[1];
  // Strict f32 needs more than the properties: the device must also pass the built-in conformance probe, which runs on
  // the first request that asks for it. Only a request with the strict bit tells.
  const strict = status === 0 ? Number(/status=(\d+)/.exec(ask(4).stdout)?.[1] ?? NaN) === 0 : false;
  return {
    available: status === 0,
    int64: field("int64") === "1",
    strictF32: strict,
    // The kind of device that Gpu.Auto has a measured cost rule for: an integrated GPU that the host reaches without a copy.
    autoDevice: status === 0 && field("type") === "integrated" && field("transfer") === "direct",
    name: /device="([^"]*)"/.exec(result.stdout)?.[1] ?? "none",
  };
}
const devices = implementations.map(implementation => ({ ...implementation, caps: probe(implementation.env) })).filter(device => device.caps.available);
if (!devices.length) skip("device", "no Vulkan implementation was found on this machine; only the hermetic checks ran", "TSUZURI_REQUIRE_VULKAN");

// ---- the programs ----
// Backend codes of Gpu.last_backend as the programs report them: 0 CPU reference, 2 Vulkan, 5 Auto (only Gpu.backend says 5).
const common = `def mix :: i32 -> i32
fn mix value = (value * 1664525 + 1013904223) ^ (Bits.ushr value 13)

def scramble :: i32u -> i32u
fn scramble value = value * 1664525i32u + 1013904223i32u

def wide :: i64 -> i64
fn wide value = (value * 6364136223846793005l + 1442695040888963407l) ^ (value >>> 17)

def wide_unsigned :: i64u -> i64u
fn wide_unsigned value = (value * 6364136223846793005ul + 1442695040888963407ul) ^ (Bits.ushr value 29)

def poly :: f32 -> f32
fn poly value = ((value * 0.5 + 0.25) * value + 0.125) * value + 1.0

def strict_poly :: f32 -> f32
fn strict_poly value = value * value + value

def sample :: i32 -> f32
fn sample index = (index as f32) * 0.015625 + 0.5

def near :: f32 -> f32 -> f32 -> bool
fn near tolerance actual expected = Math.abs (actual - expected) <= tolerance * Math.abs expected

def code :: Gpu.Backend -> i32
fn code backend = match backend with
    | Gpu.Vulkan -> 2
    | Gpu.WebGpu -> 1
    | Gpu.Auto -> 5
    | _ -> 0

export def available_vulkan :: bool
fn available_vulkan = Result.is_ok (&(Gpu.request Gpu.Vulkan))

export def auto_device_backend :: i32
fn auto_device_backend = code (Gpu.backend (&(Result.get (Gpu.request Gpu.Auto))))

export def before_any_call :: i32
fn before_any_call = code (Gpu.last_backend ())
`;
// Each kind is one kernel and the way to compare it: strict ones bit for bit, relaxed f32 within 1e-5.
const kinds = {
  strict: { kernel: "mix", maker: "(\\index -> index * 3 - 7)", map: "Gpu.map", init: "Gpu.init", same: "actual[index] == expected[index]" },
  unsigned: { kernel: "scramble", maker: "(\\index -> (index as i32u) * 2654435761i32u)", map: "Gpu.map", init: "Gpu.init", same: "actual[index] == expected[index]" },
  relaxed: { kernel: "poly", maker: "sample", map: "Gpu.map_relaxed", init: "Gpu.init_relaxed", same: "near 0.00001 actual[index] expected[index]" },
  wide: { kernel: "wide", maker: "(\\index -> (index as i64) * 4294967311l - 7l)", map: "Gpu.map", init: "Gpu.init", same: "actual[index] == expected[index]" },
  unsigned_wide: { kernel: "wide_unsigned", maker: "(\\index -> (index as i64u) * 2654435761ul)", map: "Gpu.map", init: "Gpu.init", same: "actual[index] == expected[index]" },
  strict_float: { kernel: "strict_poly", maker: "sample", map: "Gpu.map", init: "Gpu.init", same: "actual[index] == expected[index]" },
};
// For a backend name (Vulkan or Gpu.Auto) and a kind: `wrong_<kind>_<backend> count` runs the reference and the device on the
// same lanes (init, then map) and counts the lanes that differ; `served_<kind>_<backend>` reads Gpu.last_backend right after
// the device's map.
function exportsFor(backend, kind) {
  const lower = backend.toLowerCase();
  const { kernel, maker, map, init, same } = kinds[kind];
  return `export def wrong_${kind}_${lower} :: i32 -> i32
fn wrong_${kind}_${lower} count = {
    let reference = Result.get (Gpu.request Gpu.CpuReference);
    let device = Result.get (Gpu.request Gpu.${backend});
    let expected = Gpu.to_array (${map} (&reference) ${kernel} (${init} (&reference) (count as i64) ${maker}));
    let actual = Gpu.to_array (${map} (&device) ${kernel} (${init} (&device) (count as i64) ${maker}));
    let mut wrong = 0;
    for index in 0i64 .. ((count as i64) - 1) do
        if ${same} then () else wrong = wrong + 1;
    wrong
}

export def served_${kind}_${lower} :: i32
fn served_${kind}_${lower} = {
    let device = Result.get (Gpu.request Gpu.${backend});
    let _values = Gpu.to_array (${map} (&device) ${kernel} (${init} (&device) 1000 ${maker}));
    code (Gpu.last_backend ())
}
`;
}
// What each project needs of the device, for an explicit Gpu.Vulkan request (the whole program's kernels at once) and for each
// kind under Gpu.Auto (the kernel's own needs).
const projects = {
  // Strict 32-bit kernels and relaxed f32 need no optional device feature.
  core: { kinds: ["strict", "unsigned", "relaxed"], vulkan: caps => caps.available, auto: () => caps => caps.autoDevice },
  // 64-bit kernels need shaderInt64.
  wide: { kinds: ["wide", "unsigned_wide"], vulkan: caps => caps.available && caps.int64, auto: () => caps => caps.autoDevice && caps.int64 },
  // A strict f32 kernel needs the float controls, and the request of a program with one asks for them for all its kernels,
  // while Gpu.Auto asks per kernel: the integer kernel beside it is still served by the device.
  strictf32: {
    kinds: ["strict", "strict_float"],
    vulkan: caps => caps.available && caps.strictF32,
    auto: kind => (kind === "strict_float" ? caps => caps.autoDevice && caps.strictF32 : caps => caps.autoDevice),
  },
};
for (const [name, project] of Object.entries(projects)) {
  project.source = common + ["Vulkan", "Auto"].flatMap(backend => project.kinds.map(kind => exportsFor(backend, kind))).join("\n");
  project.path = join(directory, name);
  mkdirSync(project.path, { recursive: true });
  writeFileSync(join(project.path, "Main.tz"), project.source);
}
const counts = [0, 1, 3, 255, 256, 257, 1000, 4097, 70000];

// ---- hand-built runs: the heap of the Tsuzuri code is tracked, and the runtime is the one the driver links ----
const runtimeSource = join(directory, "runtime.c");
writeFileSync(runtimeSource, `#define TZ_GPU_VULKAN 1\n${readFileSync(join(root, "src/runtime/gpu-vulkan.c"), "utf8")}\n${readFileSync(join(root, "src/runtime/gpu.c"), "utf8")}`);
function host(project) {
  const declarations = [];
  const body = [];
  for (const kind of project.kinds) {
    for (const backend of ["vulkan", "auto"]) {
      declarations.push(`extern int32_t tz_wrong_${kind}_${backend}(int32_t);`, `extern int32_t tz_served_${kind}_${backend}(void);`);
      const guard = backend === "vulkan" ? "if (available) " : "";
      body.push(`  ${guard}{ for (unsigned index = 0; index < sizeof counts / sizeof counts[0]; index++) { int32_t wrong = tz_wrong_${kind}_${backend}(counts[index]); if (wrong != 0) { printf("MISMATCH ${kind} ${backend} %d lanes: %d\\n", counts[index], wrong); return 1; } if (live != 0) { printf("LEAK ${kind} ${backend} %d lanes: %lld bytes\\n", counts[index], (long long)live); return 1; } } printf("served ${kind} ${backend} %d\\n", tz_served_${kind}_${backend}()); }`);
    }
  }
  return `#undef NDEBUG
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
static int64_t live;
void *tracked_alloc(uint64_t size) { uint64_t *header = malloc(size + 16); if (!header) abort(); header[0] = size; live += (int64_t)size; return header + 2; }
void tracked_free(void *pointer) { if (pointer) { uint64_t *header = (uint64_t *)pointer - 2; live -= (int64_t)header[0]; free(header); } }
extern uint8_t tz_available_vulkan(void);
extern int32_t tz_auto_device_backend(void);
extern int32_t tz_before_any_call(void);
${declarations.join("\n")}
int main(void) {
  static const int32_t counts[] = {${counts.join(", ")}};
  int available = tz_available_vulkan();
  printf("available %d\\n", available);
  printf("auto_device_backend %d\\n", tz_auto_device_backend());
  printf("before_any_call %d\\n", tz_before_any_call());
${body.join("\n")}
  if (live != 0) { printf("LEAK at exit: %lld bytes\\n", (long long)live); return 1; }
  return 0;
}
`;
}
const builds = {};
function build(name, optimization) {
  const key = `${name}-${optimization}`;
  if (builds[key]) return builds[key];
  const project = projects[name];
  const ir = join(project.path, "module.ll");
  cli(["build", project.path, "--emit", "llvm", "-o", ir]);
  const tracked = join(project.path, "tracked.ll");
  writeFileSync(tracked, readFileSync(ir, "utf8").replaceAll("@malloc(", "@tracked_alloc(").replaceAll("@free(", "@tracked_free("));
  const hostPath = join(project.path, "host.c");
  writeFileSync(hostPath, host(project));
  const program = join(project.path, `host-${optimization}`);
  execute(clang, [tracked, hostPath, runtimeSource, `-O${optimization}`, "-ffp-contract=off", "-Wno-override-module", ...sanitize, "-o", program, "-lm", "-pthread", ...(process.platform === "linux" ? ["-ldl"] : [])]);
  return (builds[key] = program);
}
function served(output) {
  const found = {};
  for (const [, kind, backend, value] of output.matchAll(/^served (\w+) (\w+) (\d+)$/gm)) found[`${kind}_${backend}`] = Number(value);
  return found;
}
const env = (device, extra = {}) => ({ ...device.env, ASAN_OPTIONS: "detect_stack_use_after_return=1", TSUZURI_GPU_AUTO_MIN_WORK: undefined, ...extra });

for (const device of devices) {
  const { caps } = device;
  for (const [name, project] of Object.entries(projects)) {
    for (const optimization of [0, 3]) {
      const label = `${name} -O${optimization} on ${device.label} (${caps.name})`;
      check(`vulkan: ${label}: an explicit device matches the CPU reference or is Unavailable by what the device reports`, () => {
        const result = execute(build(name, optimization), [], { env: env(device) });
        assert.match(result.stdout, /^available (\d)$/m);
        const available = /^available (\d)$/m.exec(result.stdout)[1] === "1";
        assert.equal(available, project.vulkan(caps), `the request is ${available ? "Ok" : "Unavailable"}; the device reports ${JSON.stringify(caps)}`);
        const found = served(result.stdout);
        for (const kind of project.kinds) {
          if (available) assert.equal(found[`${kind}_vulkan`], 2, `${kind}: the explicit device serves its calls`);
          else assert.equal(found[`${kind}_vulkan`], undefined, `${kind}: no call runs on an Unavailable device`);
        }
        assert.match(result.stdout, /^before_any_call 0$/m);
        assert.match(result.stdout, /^auto_device_backend 5$/m, "Gpu.backend of an Auto device says Auto");
      });
      check(`auto: ${label}: the measured rule keeps small calls on the CPU reference, and the results are the reference's`, () => {
        const result = execute(build(name, optimization), [], { env: env(device) });
        const found = served(result.stdout);
        for (const kind of project.kinds) assert.equal(found[`${kind}_auto`], 0, `${kind}`);
      });
      check(`auto: ${label}: with the cost rule replaced by a threshold of 0 the device serves what it can run and the CPU the rest`, () => {
        const result = execute(build(name, optimization), [], { env: env(device, { TSUZURI_GPU_AUTO_MIN_WORK: "0" }) });
        const found = served(result.stdout);
        for (const kind of project.kinds) {
          assert.equal(found[`${kind}_auto`], project.auto(kind)(caps) ? 2 : 0, `${kind}: ${JSON.stringify(caps)}`);
        }
      });
    }
  }
}

check("vulkan: an empty or wrong TSUZURI_VULKAN_LIBRARY makes Gpu.Vulkan Unavailable and keeps Gpu.Auto on the CPU reference", () => {
  const program = build("core", 3);
  for (const library of ["", "/nonexistent/libvulkan.dylib"]) {
    const result = execute(program, [], { env: { TSUZURI_VULKAN_LIBRARY: library, TSUZURI_GPU_AUTO_MIN_WORK: "0" } });
    assert.match(result.stdout, /^available 0$/m, library);
    const found = served(result.stdout);
    assert.deepEqual(found, { strict_auto: 0, unsigned_auto: 0, relaxed_auto: 0 }, `Auto never selects a backend that cannot open [${library}]`);
  }
});

// ---- the conformance probe through the language, on a synthetic device that reports the strict float controls ----
// The device properties are a claim of the driver, so a program with strict f32 kernels gets a device only when the built-in
// probe, run on the first request, also finds the strict results. The mock (tests/gpu_vulkan_mock.c) computes the probe the way
// a conforming device does and corrupts one lane on request (`probe=<lane>`).
{
  const mockLibrary = join(directory, process.platform === "darwin" ? "libvk_mock.dylib" : "libvk_mock.so");
  const mockBuilt = execute(clang, ["-std=gnu11", "-O1", "-ffp-contract=off", "-Wno-unused-function", "-Wno-unused-variable", "-shared", "-fPIC", join(root, "tests/gpu_vulkan_mock.c"), "-o", mockLibrary], { success: false });
  if (mockBuilt.status !== 0) {
    skip("probe through the language", `${clang} cannot build the mock Vulkan library: ${mockBuilt.stderr.split("\n")[0]}`);
  } else {
    const availability = name => {
      const project = projects[name];
      const ir = join(project.path, "availability.ll");
      cli(["build", project.path, "--emit", "llvm", "-o", ir]);
      const hostPath = join(project.path, "availability.c");
      writeFileSync(hostPath, `#undef NDEBUG\n#include <stdint.h>\n#include <stdio.h>\nextern uint8_t tz_available_vulkan(void);\nint main(void) { printf("available %d\\n", tz_available_vulkan()); return 0; }\n`);
      const executable = join(project.path, "availability");
      execute(clang, [ir, hostPath, runtimeSource, "-O1", "-ffp-contract=off", "-Wno-override-module", ...sanitize, "-o", executable, "-lm", "-pthread", ...(process.platform === "linux" ? ["-ldl"] : [])]);
      return executable;
    };
    const request = (executable, config) => execute(executable, [], { env: { TSUZURI_VULKAN_LIBRARY: mockLibrary, TZ_VK_MOCK: config, TSUZURI_GPU_DEBUG: "1" } });
    check("probe: Gpu.request of a program with strict f32 kernels is Ok only where the controls are reported and the conformance probe passes", () => {
      const strict = availability("strictf32");
      assert.match(request(strict, "strict=1").stdout, /^available 1$/m, "the controls are reported and the probe passes");
      assert.match(request(strict, "strict=1").stderr, /the strict float32 probe passed \(27 lanes\)/);
      const refused = request(strict, "strict=1,probe=3");
      assert.match(refused.stdout, /^available 0$/m, "the controls are reported but a lane of the probe differs");
      assert.match(refused.stderr, /the strict float32 probe: -\(a \* b\), lane 3: the device computed/);
      assert.match(refused.stderr, /strict float32 kernels are refused/);
      assert.match(request(strict, "").stdout, /^available 0$/m, "no controls are reported");
    });
    check("probe: a program without strict f32 kernels never runs it, and a device that fails it still serves that program", () => {
      const core = availability("core");
      for (const config of ["strict=1", "strict=1,probe=3", ""]) {
        const answer = request(core, config);
        assert.match(answer.stdout, /^available 1$/m, config);
        assert.doesNotMatch(answer.stderr, /strict float32 probe/, `${config}: nothing asks for strict float32`);
      }
    });
  }
}

// ---- the driver builds the same programs: it links the Vulkan runtime when a program names Vulkan or Auto ----
const program = `def mix :: i32 -> i32
fn mix value = (value * 1664525 + 1013904223) ^ (Bits.ushr value 13)

def wide :: i64 -> i64
fn wide value = (value * 6364136223846793005l + 1442695040888963407l) ^ (value >>> 17)

def name :: Gpu.Backend -> string
fn name backend = match backend with
    | Gpu.CpuReference -> "cpu"
    | Gpu.WebGpu -> "webgpu"
    | Gpu.Vulkan -> "vulkan"
    | Gpu.Cuda -> "cuda"
    | Gpu.Metal -> "metal"
    | Gpu.Auto -> "auto"

def main :: unit -> i32 = \\() ->
    let! a = IO.write_line ("before " + name (Gpu.last_backend ()))
    let auto = Result.get (Gpu.request Gpu.Auto)
    let small = Gpu.to_array (Gpu.map (&auto) mix (Gpu.init (&auto) 1000 (\\index -> index * 3 - 7)))
    let! b = IO.write_line ("auto small " + name (Gpu.last_backend ()) + " " + to_string small[1])
    let big = Gpu.to_array (Gpu.map (&auto) wide (Gpu.init (&auto) 100000 (\\index -> (index as i64) * 3l)))
    let! c = IO.write_line ("auto wide " + name (Gpu.last_backend ()) + " " + to_string big[1])
    match Gpu.request Gpu.Vulkan with
    | Result.Error _ ->
        let! e = IO.write_line "vulkan unavailable"
        0
    | Result.Ok device ->
        let explicit = Gpu.to_array (Gpu.map (&device) mix (Gpu.init (&device) 1000 (\\index -> index * 3 - 7)))
        let! d = IO.write_line ("explicit " + name (Gpu.last_backend ()) + " " + to_string explicit[1])
        0
`;
const programPath = join(directory, "program");
mkdirSync(programPath);
writeFileSync(join(programPath, "Main.tz"), program);
for (const optimization of [0, 3]) {
  const executable = join(programPath, `run-${optimization}`);
  let built = false;
  const ensureBuilt = () => {
    if (!built) cli(["build", programPath, `-O${optimization}`, "-o", executable]);
    built = true;
    return executable;
  };
  for (const device of devices) {
    const { caps } = device;
    check(`driver -O${optimization} on ${device.label}: the program links the Vulkan runtime, Auto serves by the rule, and an explicit device runs the kernel`, () => {
      // mix(-4) and wide(3), the results of element 1 of the two calls, computed independently of the compiler.
      const cpuLine = /auto small cpu 1007592660/;
      // The program has a 64-bit kernel, and a request names the features of every kernel of the program, the Gpu.Auto ones
      // too: without shaderInt64 the explicit device is Unavailable although the 32-bit kernel alone would run.
      const explicit = caps.available && caps.int64;
      const plain = execute(ensureBuilt(), [], { env: env(device) });
      assert.match(plain.stdout, /^before cpu$/m);
      assert.match(plain.stdout, cpuLine, "a small call stays on the CPU reference");
      assert.match(plain.stdout, /auto wide cpu 2088359638719790806/, "so does a 100,000-lane 64-bit call of a few operations");
      assert.match(plain.stdout, explicit ? /explicit vulkan 1007592660/ : /vulkan unavailable/);
      const forced = execute(ensureBuilt(), [], { env: env(device, { TSUZURI_GPU_AUTO_MIN_WORK: "0", TSUZURI_GPU_DEBUG: "1" }) });
      const served = caps.autoDevice ? "vulkan" : "cpu";
      assert.match(forced.stdout, new RegExp(`auto small ${served} 1007592660`));
      assert.match(forced.stdout, new RegExp(`auto wide ${caps.autoDevice && caps.int64 ? "vulkan" : "cpu"} 2088359638719790806`), "the 64-bit kernel needs shaderInt64 as well");
      // The runtime traces each dispatch: kinds 0x<out><in>, 1 is a 32-bit lane and 4 a 64-bit one. init and map of the
      // 1000-lane call and of the 100,000-lane call (64-bit output, 32-bit index input) run on the device.
      const dispatches = [...forced.stderr.matchAll(/^tsuzuri: Vulkan: (map|init) (\d+) lanes, kinds 0x([0-9a-f]{4})$/gm)].map(match => `${match[1]} ${match[2]} ${match[3]}`);
      const expected = [];
      if (caps.autoDevice) expected.push("init 1000 0101", "map 1000 0101");
      if (caps.autoDevice && caps.int64) expected.push("init 100000 0401", "map 100000 0404");
      if (explicit) expected.push("init 1000 0101", "map 1000 0101");
      assert.deepEqual(dispatches, expected);
      const missing = execute(ensureBuilt(), [], { env: env(device, { TSUZURI_GPU_AUTO_MIN_WORK: "0", TSUZURI_VULKAN_LIBRARY: "" }) });
      assert.match(missing.stdout, /auto small cpu 1007592660/);
      assert.match(missing.stdout, /vulkan unavailable/);
    });
  }
}

// ---- the weight of a kernel prices the path a lane takes (no timing: the descriptors' weights and the rule's decisions) ----
// A kernel `if v < 0 then <1,280 operations> else v + 1` costs a lane two operations on the lanes that matter, and was priced
// at 1,282: Gpu.Auto then served 29 of 30 calls of 1,000,000 lanes on the device, which was slower than the CPU reference. The
// program calls Gpu.Auto with that kernel and with a branch-free kernel of 1,280 operations (the control: the same rule must
// move that one to the device). The device is the mock, an integrated GPU, so the answer does not depend on the machine.
{
  const mockLibrary = join(directory, process.platform === "darwin" ? "libvk_mock_weight.dylib" : "libvk_mock_weight.so");
  const mockBuilt = execute(clang, ["-std=gnu11", "-O1", "-ffp-contract=off", "-Wno-unused-function", "-Wno-unused-variable", "-shared", "-fPIC", join(root, "tests/gpu_vulkan_mock.c"), "-o", mockLibrary], { success: false });
  if (mockBuilt.status !== 0) {
    skip("weights through the language", `${clang} cannot build the mock Vulkan library: ${mockBuilt.stderr.split("\n")[0]}`);
  } else {
    const weightProject = join(directory, "weights");
    mkdirSync(weightProject);
    writeFileSync(join(weightProject, "Main.tz"), `def step :: i32 -> i32
fn step value = ((value * 3 + 1) ^ 5) * 7 + 11

def r4 :: i32 -> i32
fn r4 value = step (step (step (step value)))

def r16 :: i32 -> i32
fn r16 value = r4 (r4 (r4 (r4 value)))

def r64 :: i32 -> i32
fn r64 value = r16 (r16 (r16 (r16 value)))

def straight :: i32 -> i32
fn straight value = r64 (r64 (r64 (r64 value)))

def branchy :: i32 -> i32
fn branchy value = if value < 0 then r64 (r64 (r64 (r64 value))) else value + 1

def on_vulkan :: unit -> i32
fn on_vulkan _unit = match Gpu.last_backend () with
    | Gpu.Vulkan -> 1
    | _ -> 0

def straight_calls :: unit -> i32
fn straight_calls _unit = {
    let device = Result.get (Gpu.request Gpu.Auto);
    let mut served = 0;
    for call in 0 .. 29 do {
        let _values = Gpu.to_array (Gpu.map (&device) straight (Gpu.init (&device) 1000000 (\\index -> index - 500000)));
        served = served + on_vulkan ()
    };
    served
}

def branchy_calls :: unit -> i32
fn branchy_calls _unit = {
    let device = Result.get (Gpu.request Gpu.Auto);
    let mut served = 0;
    for call in 0 .. 29 do {
        let _values = Gpu.to_array (Gpu.map (&device) branchy (Gpu.init (&device) 1000000 (\\index -> index - 500000)));
        served = served + on_vulkan ()
    };
    served
}

def main :: unit -> i32 = \\() ->
    let! a = IO.write_line ("branchy " + to_string (branchy_calls ()))
    let! b = IO.write_line ("straight " + to_string (straight_calls ()))
    0
`);
    check("weights: the compiler prices a branchy kernel by its cheaper path, and Gpu.Auto keeps 1,000,000 lanes of it on the CPU reference", () => {
      const executable = join(weightProject, "weights");
      cli(["build", weightProject, "-o", executable]);
      const run = execute(executable, [], { env: { TSUZURI_VULKAN_LIBRARY: mockLibrary, TZ_VK_MOCK: "", TSUZURI_GPU_AUTO_MIN_WORK: undefined, TSUZURI_GPU_DEBUG: "1" } });
      assert.match(run.stdout, /^branchy 0$/m, `no call of the branchy kernel pays for the transfer: all 30 stay on the CPU reference\n${run.stdout}`);
      const straight = Number(/^straight (\d+)$/m.exec(run.stdout)?.[1]);
      assert.ok(straight >= 25, `the same rule moves the kernel of 1,280 operations to the device (served ${straight} of 30)`);
      const dispatches = [...run.stderr.matchAll(/^tsuzuri: Vulkan: map 1000000 lanes/gm)].length;
      assert.equal(dispatches, straight, "every call of the straight kernel that the rule sent to the device was dispatched, and none of the branchy kernel");
      const ir = join(weightProject, "weights.ll");
      cli(["build", weightProject, "--emit", "llvm", "-o", ir]);
      const weights = [...readFileSync(ir, "utf8").matchAll(/ptr @tz\.gpu\.kernel\.\d+\.spirv, i32 \d+, i32 (\d+) \}/g)].map(match => Number(match[1]));
      assert.ok(weights.includes(2), `the branchy kernel weighs 2 (a comparison and the cheaper arm): ${weights}`);
      assert.ok(weights.includes(1280), `the straight kernel weighs 1,280: ${weights}`);
      assert.ok(!weights.some(weight => weight > 1280), `no weight counts both arms of the branchy kernel: ${weights}`);
    });
  }
}

// ---- WebAssembly: no Vulkan host, no new import ----
for (const optimization of [0, 3]) {
  check(`wasm32 -O${optimization}: Gpu.Vulkan is Unavailable, Gpu.Auto runs the CPU reference, and the default module has no import`, () => {
    const wasm = join(projects.core.path, `core-${optimization}.wasm`);
    cli(["build", projects.core.path, "--target", "wasm32", `-O${optimization}`, "-o", wasm]);
    const module = new WebAssembly.Module(readFileSync(wasm));
    assert.deepEqual(WebAssembly.Module.imports(module), []);
    const { exports } = new WebAssembly.Instance(module);
    assert.equal(exports.tz_available_vulkan(), 0);
    assert.equal(exports.tz_before_any_call(), 0);
    assert.equal(exports.tz_auto_device_backend(), 5);
    for (const count of counts) {
      assert.equal(exports.tz_wrong_strict_auto(count), 0, `${count}`);
      assert.equal(exports.tz_wrong_unsigned_auto(count), 0, `${count}`);
      assert.equal(exports.tz_wrong_relaxed_auto(count), 0, `${count}`);
    }
    assert.equal(exports.tz_served_strict_auto(), 0, "no WASM host is measured, so Auto never leaves the CPU reference");
    assert.ok(exports.memory.buffer.byteLength <= 16 * 1024 * 1024);
  });
}
check("wasm32 --wasm-feature webgpu: the imports are tsuzuri_gpu.open and tsuzuri_gpu.run only, and Gpu.Auto adds none", () => {
  const wasm = join(projects.core.path, "core-webgpu.wasm");
  cli(["build", projects.core.path, "--target", "wasm32", "--wasm-feature", "webgpu", "-O3", "-o", wasm]);
  const imports = WebAssembly.Module.imports(new WebAssembly.Module(readFileSync(wasm))).map(entry => `${entry.module}.${entry.name} ${entry.kind}`).sort();
  assert.deepEqual(imports, ["tsuzuri_gpu.open function", "tsuzuri_gpu.run function"]);
});

console.log(`GPU Vulkan language: ${passed} of ${passed + failed} checks passed, ${skips.length} skipped${skips.length ? ` (${skips.join("; ")})` : ""}; devices: ${devices.map(device => `${device.label}: ${device.caps.name}`).join(", ") || "none"}; sanitizers: ${sanitize.length ? "on" : "off"}${only ? `; ONLY ${only} (a filtered run, not the suite)` : ""}`);
process.exit(failed === 0 ? 0 : 1);
