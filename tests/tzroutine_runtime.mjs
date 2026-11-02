// The tzroutine runtime on its own (TZ01): builds tests/tzroutine_runtime.c, which includes
// src/runtime/tzroutine.c, at -O0 and -O3 and runs every case with 1, 2 and 4 Ps. TSUZURI_TSAN=1
// adds a ThreadSanitizer build and TSUZURI_ASAN=1 an AddressSanitizer and UBSan build (each case
// once, with 4 Ps). On an Apple silicon Mac with Rosetta it also runs the x86-64 build. A build with
// src/runtime/stack.c and -DTZ_STACK_GUARD checks that a tzroutine's overflow is reported. On Windows
// (TZ01 Phase 2) TSUZURI_CLANG is the MSVC ABI clang and the builds link with lld.
//
// usage: node tests/tzroutine_runtime.mjs        (TSUZURI_CLANG names clang, default `clang`)
import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const clang = process.env.TSUZURI_CLANG ?? "clang";
const temporary = mkdtempSync(join(tmpdir(), "tsuzuri-tzroutine-"));
const source = "tests/tzroutine_runtime.c";
const cases = ["spawn", "tree", "pingpong", "mpmc", "select", "relay", "mutex", "waitgroup", "sleep",
  "external", "deep", "fp", "stack_size", "failures", "fp_mode", "wake_many", "count_after_join", "detector",
  "starvation", "select_same", "long_sleep"];
const windows = process.platform === "win32";
const platformFlags = windows
  ? [`--target=${process.arch === "arm64" ? "aarch64" : "x86_64"}-pc-windows-msvc`, "-fuse-ld=lld", "-D_CRT_SECURE_NO_WARNINGS"]
  : ["-pthread"];
// After the inputs, for linkers with --as-needed: glibc keeps fegetround and fesetround in libm.
const libraries = windows ? [] : ["-lm"];
// The cases whose children end by a signal, which a sanitizer reports in its own way.
const signalled = new Set(["stack_size", "failures"]);

function execute(program, args, environment = {}, success = true) {
  const result = spawnSync(program, args, {
    cwd: root, encoding: "utf8", timeout: 300_000, maxBuffer: 16 * 1024 * 1024,
    env: { ...process.env, ...environment },
  });
  if (result.error) throw result.error;
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")} ${JSON.stringify(environment)}\n${result.stdout}\n${result.stderr}`);
  return result;
}

function build(name, flags, extra = []) {
  const binary = join(temporary, windows ? `${name}.exe` : name);
  execute(clang, ["-std=c11", "-Wall", "-Wextra", "-Werror", ...flags, ...platformFlags, source, ...extra, ...libraries, "-o", binary]);
  return binary;
}

let runs = 0;
try {
  for (const optimization of ["-O0", "-O3"]) {
    const binary = build(`tzroutine${optimization}`, [optimization]);
    for (const procs of ["1", "2", "4"]) {
      for (const name of cases) {
        execute(binary, [name], { TSUZURI_MAXPROCS: procs });
        runs++;
      }
    }
  }
  if (!windows) {
    // With stack.c, a tzroutine that overflows its stack is reported like any other thread.
    const guarded = build("tzroutine-guarded", ["-O2", "-DTZ_STACK_GUARD"], ["-x", "c", "src/runtime/stack.c"]);
    const overflow = execute(guarded, ["child-overflow"], {}, false);
    assert.equal(overflow.signal, "SIGABRT", `${overflow.status} ${overflow.signal}\n${overflow.stderr}`);
    assert.match(overflow.stderr, /^trap: stack overflow$/m);
    for (const name of cases) {
      execute(guarded, [name], { TSUZURI_MAXPROCS: "4" });
      runs++;
    }
  }
  if (process.platform === "darwin" && process.arch === "arm64" && spawnSync("arch", ["-x86_64", "/usr/bin/true"]).status === 0) {
    const binary = build("tzroutine-x86_64", ["-arch", "x86_64", "-O2"]);
    for (const name of cases) {
      execute("arch", ["-x86_64", binary, name], { TSUZURI_MAXPROCS: "4" });
      runs++;
    }
  }
  const sanitizers = [];
  if (process.env.TSUZURI_TSAN === "1") sanitizers.push(["tsan", ["-fsanitize=thread"], { TSAN_OPTIONS: "halt_on_error=1" }]);
  if (process.env.TSUZURI_ASAN === "1") sanitizers.push(["asan", ["-fsanitize=address,undefined"], { ASAN_OPTIONS: "detect_leaks=0:detect_stack_use_after_return=1", UBSAN_OPTIONS: "halt_on_error=1" }]);
  for (const [name, flags, environment] of sanitizers) {
    const binary = build(`tzroutine-${name}`, ["-O1", "-g", ...flags]);
    for (const test of cases.filter(test => !signalled.has(test))) {
      execute(binary, [test], { TSUZURI_MAXPROCS: "4", ...environment });
      runs++;
    }
  }
  console.log(`tzroutine runtime: ${runs} runs passed`);
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
