// M10: `ObjC.with_autorelease_pool`, the pool of the `@main` of an Objective-C image, and the pools
// that the scheduler runs around each run of a tzroutine, end to end on macOS. The fixture is
// linked with a host without ARC (tests/objc_pools_host.m) that counts its objects between init
// and dealloc; every expected line is derived from those counts, never from the compiler under
// test. The leak cases read the peak resident size from `/usr/bin/time -l`: the bound is far from
// both sides (about 6 MiB with the pools, more than 45 MiB without), and it is a check that the
// objects are released, not a speed threshold.
//
// Usage: node tests/objc_pools.mjs [path/to/tsuzuri]
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(process.argv[2] ?? join(root, "target", "debug", "tsuzuri"));
const clang = process.env.TSUZURI_CLANG ?? "clang";
// The programs are not under tests/fixtures/: the repository-wide tests build every fixture for
// their host, and these reach Objective-C, which only an Apple target has.
const fixture = join(root, "tests", "objc", "pools");
const plainPingpong = join(root, "tests", "objc", "pingpong");
const host = join(root, "tests", "objc_pools_host.m");
const bundledClang =
  process.env.TSUZURI_BUNDLED_CLANG ?? join(root, "target", "toolchain", "bin", "tsuzuri-clang");

if (process.platform !== "darwin") {
  console.log("objc_pools: skipped (the Objective-C runtime needs macOS)");
  process.exit(0);
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { encoding: "utf8", timeout: 300_000, ...options });
  if (result.error) throw result.error;
  return result;
}

function must(command, args, options = {}) {
  const result = run(command, args, options);
  assert.equal(
    result.status,
    0,
    `${command} ${args.join(" ")}\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`,
  );
  return result;
}

// A clang that is not Apple's (Homebrew's, the bundled toolchain's) needs the SDK named.
const sdk = run("xcrun", ["--show-sdk-path"]);
const sysroot = sdk.status === 0 ? ["-isysroot", sdk.stdout.trim()] : [];

// What each case prints, from the host's counts:
// - `explicit`: ten objects inside the pool, none after it; on the main thread and on a tzroutine.
// - `nested`: seven inside both pools, three after the inner one, none after the outer one; an
//   object that the pool's function returns is one live object with tag 21, and none once dropped.
// - `migrate`: no round of the 8 x 50 pools around a sleep popped on another thread than it
//   pushed on (a pop on another thread would have aborted), and nothing is left.
// - `parallel`: 1^2 + ... + 64^2 = 64 * 65 * 129 / 6 = 89440, and nothing is left of the
//   1 + ... + 64 objects that the elements autoreleased inside their pools.
// - `leak`: nothing is left of two million objects, a thousand for each pool.
// - `auto_yield`, `auto_sleep`: a thousand objects before the tzroutine gives its thread back,
//   none after: the scheduler popped the pool of that run.
// - `auto_exit`: seven objects inside the tzroutine, none once it has ended.
// - `locked`: five objects in a pool of the tzroutine's own over a sleep and a yield, none after
//   it; six in `Routine.with_locked_thread` over a yield and a sleep, still six after it (the run
//   has not ended), none after the next yield.
// - `owned`: an object that the program holds over a yield and a sleep still answers (tag 21) and
//   is the one live object; none after its drop.
// - `auto_leak`: nothing is left of two million objects without a pool in the program, with a
//   yield after each thousand.
// - `pingpong`: the 200000 round trips.
const expected = {
  explicit: "10 0",
  explicit_routine: "10 0",
  nested: "7 3 0 1 21 0",
  nested_routine: "7 3 0 1 21 0",
  migrate: "0 0",
  parallel: "89440 0",
  leak: "0",
  auto_yield: "1000 0",
  auto_sleep: "1000 0",
  auto_exit: "7 0",
  locked: "5 0 6 6 0",
  owned: "21 1 0",
  auto_leak: "0",
  pingpong: "200000",
};

// Hand-written declarations alone, as programs had them before M01: no Objective-C image, so no
// pool of `@main` and no pool of the scheduler. The tzroutine autoreleases ten objects, with a
// yield after each.
const LEGACY = `@trusted
@count(1, none)
extern "objc_getClass" def objc_get_class :: ref utf8string -> i64

@trusted
@count(1, none)
extern "sel_registerName" def sel :: ref utf8string -> i64

@trusted
extern "objc_msgSend" def msg0 :: i64 -> i64 -> i64

def main :: unit -> i32 = \\() ->
    tzroutine {
        let cls = objc_get_class (ref u8"NSObject\\0")
        let alloc = sel (ref u8"alloc\\0")
        let init = sel (ref u8"init\\0")
        let autorelease = sel (ref u8"autorelease\\0")
        for _ in 1 ..= 10 do
            let _ = msg0 (msg0 (msg0 cls alloc) init) autorelease
            Routine.yield ()
    }
    0
`;

const MIB = 1024 * 1024;

// The peak resident size of one run, in bytes, and what the run printed.
function measured(executable, name, env) {
  const result = must("/usr/bin/time", ["-l", executable, name], { env });
  const peak = /^\s*(\d+)\s+maximum resident set size/m.exec(result.stderr);
  assert.ok(peak, `no peak resident size in:\n${result.stderr}`);
  return { bytes: Number(peak[1]), stdout: result.stdout };
}

function missingPools(text) {
  return text.split("\n").filter(line => line.includes("MISSING POOLS")).length;
}

function median(values) {
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.floor(sorted.length / 2)];
}

const scratch = mkdtempSync(join(tmpdir(), "tsuzuri-m10-"));
const env = { ...process.env, TSUZURI_CACHE_DIR: "", TSUZURI_MAXPROCS: "4" };
const debugPools = { ...env, OBJC_DEBUG_MISSING_POOLS: "YES" };
let ran = 0;
try {
  const hostObject = join(scratch, "host.o");
  must(clang, [
    "-fno-objc-arc",
    "-O2",
    "-mmacosx-version-min=12.0",
    ...sysroot,
    "-c",
    host,
    "-o",
    hostObject,
  ]);
  const link = ["--link", hostObject, "--framework", "Foundation"];
  for (const optimization of ["-O0", "-O3"]) {
    const executable = join(scratch, `pools${optimization}`);
    // No `-l objc`: the driver adds it for an Objective-C image.
    must(compiler, ["build", fixture, optimization, "--trap-info", ...link, "-o", executable], { env });
    for (const procs of ["1", "4"]) {
      for (const [name, line] of Object.entries(expected)) {
        // The larger cases run once, with four Ps.
        if (procs === "1" && ["leak", "auto_leak", "pingpong"].includes(name)) continue;
        const result = must(executable, [name], { env: { ...env, TSUZURI_MAXPROCS: procs } });
        assert.equal(result.stdout, `${line}\n`, `${name} at ${optimization} with ${procs} P`);
        assert.equal(result.stderr, "", `${name} at ${optimization} with ${procs} P`);
        ran += 1;
      }
    }

    // The pool of `@main`: five objects that `main` autoreleases without a pool of its own are
    // alive while it runs and gone when the process exits, and libobjc finds a pool for each.
    const mainPool = must(executable, ["main_pool"], { env: debugPools });
    assert.equal(mainPool.stdout, "5\nexit 0\n", `main_pool at ${optimization}`);
    assert.equal(missingPools(mainPool.stderr), 0, mainPool.stderr);
    // The pop is after the wait for the tzroutines: the tzroutine still sees what `main`
    // autoreleased, 20 ms after `main` returned.
    const mainWait = must(executable, ["main_wait"], { env: debugPools });
    assert.equal(mainWait.stdout, "1\nexit 0\n", `main_wait at ${optimization}`);
    assert.equal(missingPools(mainWait.stderr), 0, mainWait.stderr);
    ran += 2;

    // No object is autoreleased without a pool: in the explicit pools on tzroutines, in the
    // chunks of `Parallel.map`, in Foundation's own work, and on tzroutines without a pool in the
    // program (the scheduler's).
    for (const name of ["leak", "parallel", "stack", "migrate", "auto_yield", "auto_leak"]) {
      const result = must(executable, [name], { env: debugPools });
      assert.equal(missingPools(result.stderr), 0, `${name} at ${optimization}:\n${result.stderr.slice(0, 2000)}`);
      ran += 1;
    }

    // Foundation's formatting ran on a tzroutine of the default stack size (1 MiB). The depth
    // below the tzroutine's body where Foundation called back is printed, and is no threshold.
    const stack = must(executable, ["stack"], { env });
    const [worked, depth, left] = stack.stdout.trim().split(" ");
    assert.equal(worked, "true", stack.stdout);
    assert.equal(left, "0", stack.stdout);
    assert.ok(Number(depth) > 0 && Number(depth) < MIB, stack.stdout);
    console.log(`objc_pools: ${optimization} stack: Foundation called back ${depth} bytes below the tzroutine's body`);
    ran += 1;

    // The peak resident size. Two million objects held by one pool (the pool of `@main`, on the
    // main thread) show that the measure sees them; with a pool for each thousand, and with the
    // scheduler's pools and a yield after each thousand, the peak stays near the empty program's.
    const held = measured(executable, "held_by_main", env);
    assert.equal(held.stdout, "2000000\n", `held_by_main at ${optimization}`);
    assert.ok(held.bytes > 30 * MIB, `held_by_main at ${optimization}: ${held.bytes} bytes`);
    for (const name of ["leak", "auto_leak"]) {
      const bounded = measured(executable, name, env);
      assert.equal(bounded.stdout, `${expected[name]}\n`, `${name} at ${optimization}`);
      assert.ok(bounded.bytes < 20 * MIB, `${name} at ${optimization}: ${bounded.bytes} bytes`);
      console.log(
        `objc_pools: ${optimization} ${name}: peak ${(bounded.bytes / MIB).toFixed(1)} MiB (one pool for all: ${(held.bytes / MIB).toFixed(1)} MiB)`,
      );
      ran += 1;
    }
    ran += 1;

    // The runtime of the image calls the two pool functions.
    const undefinedSymbols = must("nm", ["-u", executable]).stdout;
    assert.match(undefinedSymbols, /_objc_autoreleasePoolPush/, `symbols at ${optimization}`);
    assert.match(undefinedSymbols, /_objc_autoreleasePoolPop/, `symbols at ${optimization}`);
    ran += 1;
  }

  // A program without Objective-C keeps the runtime that it had: nothing of libobjc is referenced.
  const plain = join(scratch, "pingpong");
  must(compiler, ["build", plainPingpong, "-O3", "-o", plain], { env });
  assert.equal(must(plain, [], { env }).stdout, "200000\n", "the exchange without Objective-C");
  assert.doesNotMatch(must("nm", ["-u", plain]).stdout, /objc_/, "a program without Objective-C");
  ran += 2;

  // Hand-written declarations make no Objective-C image: the program has no pool of `@main` and
  // its scheduler runs no pool, so its objects are reported, as before. This also shows that
  // OBJC_DEBUG_MISSING_POOLS reports on this system, which the zero counts above rely on.
  const legacySource = join(scratch, "legacy");
  mkdirSync(legacySource);
  writeFileSync(join(legacySource, "Main.tz"), LEGACY);
  const legacy = join(scratch, "legacy-program");
  must(compiler, ["build", legacySource, "-O3", "-l", "objc", "-o", legacy], { env });
  const reported = must(legacy, [], { env: debugPools });
  // libobjc reports each object but the first of a thread, which only sets its empty pool up.
  assert.ok(missingPools(reported.stderr) >= 5, reported.stderr.slice(0, 2000));
  assert.doesNotMatch(must("nm", ["-u", legacy]).stdout, /autoreleasePool/, "hand-written declarations");
  ran += 2;

  // The cost of the scheduler's pools on a switch: the same exchange over unbuffered channels in
  // the image (a push and a pop for each run of a tzroutine) and in the program without
  // Objective-C. The times are printed and are no pass or fail.
  const image = join(scratch, "pools-O3");
  const times = { image: [], plain: [] };
  for (let round = 0; round < 5; round++) {
    for (const [key, command, args] of [["image", image, ["pingpong"]], ["plain", plain, []]]) {
      const start = process.hrtime.bigint();
      must(command, args, { env });
      times[key].push(Number(process.hrtime.bigint() - start) / 1e6);
    }
  }
  console.log(
    `objc_pools: 200000 round trips: ${median(times.image).toFixed(0)} ms with the scheduler's pools, ${median(times.plain).toFixed(0)} ms without (medians of 5, whole process)`,
  );

  // The bundled toolchain (zig as the linker driver, with lld) links the same program.
  if (existsSync(bundledClang)) {
    const executable = join(scratch, "pools-bundled");
    must(compiler, ["build", fixture, "-O3", ...link, "-o", executable], {
      env: { ...env, TSUZURI_CLANG: bundledClang },
    });
    for (const name of ["explicit_routine", "nested", "migrate", "auto_yield", "locked"]) {
      assert.equal(must(executable, [name], { env }).stdout, `${expected[name]}\n`, `${name} bundled`);
      ran += 1;
    }
    const mainPool = must(executable, ["main_pool"], { env: debugPools });
    assert.equal(mainPool.stdout, "5\nexit 0\n", "main_pool bundled");
    assert.equal(missingPools(mainPool.stderr), 0, mainPool.stderr);
    ran += 1;
  } else {
    console.log(`objc_pools: the bundled toolchain link is skipped (${bundledClang} is missing)`);
  }
} finally {
  rmSync(scratch, { recursive: true, force: true });
}
console.log(`objc_pools: ok (${ran} checks)`);
