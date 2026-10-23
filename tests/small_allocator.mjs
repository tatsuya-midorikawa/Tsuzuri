// PM05 Phase 2: --allocator small, end to end, on native 64-bit targets.
// Usage: node tests/small_allocator.mjs [compiler] [--unit]
// `--unit` only builds and runs tests/heap_small_test.c (the unit tests of src/runtime/heap-small.c) and needs no compiler.
// The unit tests also run with AddressSanitizer + UBSan and ThreadSanitizer (not on Windows); TSUZURI_NO_SANITIZERS=1 leaves
// those two out, and so does a clang whose sanitizer runtime does not start (see sanitizerProblem).
import assert from "node:assert/strict";
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const unitOnly = args.includes("--unit");
const compiler = resolve(args.find((argument) => !argument.startsWith("--")) ?? join(root, "target/debug/tsuzuri"));
const clang = process.env.TSUZURI_CLANG ?? "clang";
const windows = process.platform === "win32";
const suffix = windows ? ".exe" : "";
const threadFlags = windows ? [] : ["-pthread"];
const mathFlags = windows ? [] : ["-lm"];

function execute(program, programArguments, success = true, environment = {}) {
  const result = spawnSync(program, programArguments, { cwd: root, encoding: "utf8", timeout: 600_000, maxBuffer: 32 * 1024 * 1024, env: { ...process.env, ...environment } });
  if (result.error) throw result.error;
  if (success) assert.equal(result.status, 0, `${program} ${programArguments.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}
const cli = (arguments_, environment) => {
  const result = spawnSync(compiler, arguments_, { cwd: root, encoding: "utf8", timeout: 600_000, maxBuffer: 32 * 1024 * 1024, env: { ...process.env, ...environment } });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, `${compiler} ${arguments_.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
};
// Command-line errors end with status 2; errors found while building, with status 1.
function rejects(arguments_, message, status = 2) {
  const result = execute(compiler, arguments_, false);
  assert.equal(result.status, status, `${arguments_.join(" ")}\n${result.stderr}`);
  assert.match(result.stderr, /error\[E2000\]/, arguments_.join(" "));
  assert.ok(result.stderr.includes(message), `${arguments_.join(" ")}\n${result.stderr}`);
}

const work = mkdtempSync(join(tmpdir(), "tsuzuri-small-allocator-"));
// A sanitizer runtime that this clang cannot start must not fail the run: the Homebrew LLVM 21 of the macOS CI builds sanitizer
// programs that never start (an empty AddressSanitizer program hangs, an empty ThreadSanitizer program dies with SIGSEGV), so no
// unit test could say anything. TSUZURI_NO_SANITIZERS=1 leaves the sanitizer variants out without trying them (tests/net.mjs
// does the same); otherwise an empty program built with the same flags decides, so that a hang or a crash of the unit tests
// themselves under a working sanitizer still fails. The plain variants and -O3 always run and always decide.
const sanitizers = process.env.TSUZURI_NO_SANITIZERS !== "1";
function sanitizerProblem(name, flags) {
  const source = join(work, "empty.c"), binary = join(work, `empty-${name}${suffix}`);
  writeFileSync(source, "int main(void) { return 0; }\n");
  const built = spawnSync(clang, [...flags, ...threadFlags, source, "-o", binary], { cwd: root, encoding: "utf8", timeout: 120_000 });
  if (built.error || built.status !== 0) return null; // the build of the unit tests below reports it
  const result = spawnSync(binary, [], { cwd: root, encoding: "utf8", timeout: 30_000, killSignal: "SIGKILL", env: { ...process.env, ASAN_OPTIONS: "detect_leaks=0" } });
  const what = "an empty program built with the same flags";
  if (result.error?.code === "ETIMEDOUT") return `${what} did not exit within 30 s`;
  if (result.signal) return `${what} was killed by ${result.signal}`;
  if (result.status !== 0) return `${what} exited with status ${result.status}: ${result.stderr.split("\n").find((line) => line.trim()) ?? ""}`;
  return null;
}
function skipped(name, why) {
  console.log(`small allocator unit tests (${name}) skipped: ${why}`);
  // GitHub Actions shows this next to the job instead of leaving it in the log.
  if (process.env.GITHUB_ACTIONS) console.log(`::warning::small allocator unit tests (${name}) skipped: ${why}`);
}
try {
  // The unit tests: every class, the boundaries, chains and flushes, regions, resize, failing malloc, the address limit,
  // random operations against a model, and four threads that hand blocks to each other. Plain, then with sanitizers.
  const variants = [["O0", ["-O0"], false], ["O3", ["-O3"], false]];
  if (!windows) variants.push(["asan-ubsan", ["-O1", "-fsanitize=address,undefined", "-fno-sanitize-recover=all"], true], ["tsan", ["-O1", "-fsanitize=thread"], true]);
  for (const [name, flags, sanitized] of variants) {
    if (sanitized) {
      const why = sanitizers ? sanitizerProblem(name, flags) : "TSUZURI_NO_SANITIZERS=1";
      if (why) { skipped(name, why); continue; }
    }
    const binary = join(work, `heap-small-${name}${suffix}`);
    execute(clang, ["-std=c11", "-g", "-fno-strict-aliasing", "-Wall", "-Wextra", ...flags, ...threadFlags, join(root, "tests/heap_small_test.c"), "-o", binary]);
    // The allocator never returns its tables and the test zeroes the state that points at them, which LeakSanitizer (on by
    // default on Linux) would report; the program-level checks below look at the leaks that matter.
    const result = execute(binary, [], false, { ASAN_OPTIONS: "detect_leaks=0" });
    // A sanitizer that cannot start on this machine (a TSan that rejects its address-space layout) says so before a test runs.
    if (result.status !== 0 && /FATAL: \w*Sanitizer/.test(result.stderr) && !/WARNING: \w*Sanitizer|Assertion failed|runtime error/.test(result.stderr)) {
      skipped(name, result.stderr.split("\n").find((line) => /FATAL/.test(line)));
      continue;
    }
    assert.equal(result.status, 0, `${name}\n${result.stdout}\n${result.stderr}`);
    assert.match(result.stdout, /heap_small_test: ok/, name);
    console.log(`small allocator unit tests (${name}) passed`);
  }
  if (unitOnly) process.exit(0);

  const grow = join(work, "grow");
  mkdirSync(grow);
  writeFileSync(join(grow, "Grow.tz"), `export def grow :: i64 -> i64
fn grow count =
    let mut values: Vec<i64> = Vec.empty()
    for index in 1i64 .. count do values = Vec.push index values
    let result = Vec.to_array values
    Array.sum ref result
`);
  const library = join(grow, "Grow.tz");
  const build = (source, extra, output) => cli(["build", source, ...extra, "--no-cache", "-o", output]);

  // A. What it cannot be combined with.
  const native = "--allocator small requires a native target; WebAssembly modules keep their own heap";
  rejects(["build", library, "--target", "wasm32", "--allocator", "small", "-o", join(work, "x.wasm")], native);
  rejects(["build", library, "--target", "wasm64", "--allocator", "small", "-o", join(work, "x.wasm")], native);
  rejects(["build", library, "--emit", "wgsl", "--allocator", "small", "-o", join(work, "x.wgsl")], "--allocator small requires executable, object, shared library, LLVM IR, or header output");
  rejects(["build", library, "--emit", "object", "--trap-mode", "return", "--allocator", "small", "-o", join(work, "x.o")], "--trap-mode return keeps its own tracked allocator");
  rejects(["build", library, "--emit", "object", "--freestanding", "--allocator", "small", "-o", join(work, "x.o")], "--freestanding requires --allocator host");
  rejects(["run", library, "--allocator", "small"], "--allocator is only valid with build");

  // B. The IR: deterministic, the C library only for blocks over 512 bytes and for the regions, no mmap and no thread library,
  // one thread-local variable, and the default IR does not mention the size classes.
  const first = join(work, "small.ll"), second = join(work, "small-again.ll"), plain = join(work, "plain.ll"), system = join(work, "system.ll");
  build(library, ["--emit", "llvm", "--allocator", "small"], first);
  build(library, ["--emit", "llvm", "--allocator", "small"], second);
  build(library, ["--emit", "llvm"], plain);
  build(library, ["--emit", "llvm", "--allocator", "system"], system);
  const smallIr = readFileSync(first, "utf8");
  assert.equal(smallIr, readFileSync(second, "utf8"), "the IR is deterministic");
  assert.equal(readFileSync(system, "utf8"), readFileSync(plain, "utf8"), "--allocator system is the default");
  assert.ok(!readFileSync(plain, "utf8").includes("tz_small"), "the default IR has no size classes");
  const declared = (smallIr.match(/^declare .*$/gm) ?? []).map((line) => line.match(/@([^\s(]+)\(/)[1]);
  assert.equal(new Set(declared).size, declared.length, "declarations are deduplicated");
  assert.ok(declared.includes("malloc") && declared.includes("realloc") && declared.includes("free"));
  assert.equal((smallIr.match(/^@.* thread_local .*global /gm) ?? []).length, 1, "one thread-local variable");
  assert.ok(!/@(aligned_alloc|posix_memalign|mmap|munmap|calloc|pthread_\w+)\b/.test(smallIr), "no system call or thread library");
  assert.ok(/^define internal noalias ptr @tz\.alloc\(i64 %size\)/m.test(smallIr) && /^@tz_small_shared = weak hidden global/m.test(smallIr));

  // B2. LLVM treats the entry points like malloc and free: an allocation that is freed unread disappears, one that escapes
  // stays a call. The allocation and its free are in different helpers, as in a program: the inliner has already seen
  // each helper alone, so this holds only while the entry points are calls (noinline in the generated IR) with attributes.
  const probe = join(work, "probe.ll");
  writeFileSync(probe, `${smallIr}
@probe_sink = global ptr null
define internal ptr @probe_make(i64 %x) {
  %p = call ptr @tz.alloc(i64 24)
  store i64 %x, ptr %p
  ret ptr %p
}
define internal i64 @probe_take(ptr %p) {
  %v = load i64, ptr %p
  call void @tz.free(ptr %p)
  ret i64 %v
}
define i64 @probe_pair(i64 %x) {
  %p = call ptr @probe_make(i64 %x)
  %v = call i64 @probe_take(ptr %p)
  ret i64 %v
}
define void @probe_escape(i64 %x) {
  %p = call ptr @probe_make(i64 %x)
  store ptr %p, ptr @probe_sink
  ret void
}
`);
  for (const optimization of ["-O2", "-O3"]) {
    const optimized = execute(clang, [optimization, "-Wno-override-module", "-S", "-emit-llvm", "-x", "ir", probe, "-o", "-"]).stdout;
    const body = (name) => optimized.match(new RegExp(`^define [^\\n]*@${name}\\(.*?^}`, "ms"))?.[0] ?? assert.fail(`${name} is missing`);
    assert.ok(!/\bcall\b/.test(body("probe_pair")), `${optimization}: an allocation freed unread is removed\n${body("probe_pair")}`);
    assert.ok(/call [^\n]*@tz_small_alloc\(/.test(body("probe_escape")), `${optimization}: an escaping allocation stays\n${body("probe_escape")}`);
  }

  // C. Executables, with the cache: the allocator changes the artifact, both artifacts print the same, and a second build is a hit.
  const program = join(root, "examples/hello");
  const cache = join(work, "cache");
  const executables = {};
  for (const allocator of ["system", "small"]) {
    const output = join(work, `hello-${allocator}${suffix}`);
    cli(["build", program, "--allocator", allocator, "-o", output], { TSUZURI_CACHE_DIR: cache });
    executables[allocator] = output;
    if (!windows) assert.equal(/tz_small/.test(execute("nm", [output]).stdout), allocator === "small", `${allocator} symbols`);
  }
  assert.equal(execute(executables.small, []).stdout, execute(executables.system, []).stdout);
  const again = join(work, `hello-small-again${suffix}`);
  cli(["build", program, "--allocator", "small", "-o", again], { TSUZURI_CACHE_DIR: cache });
  assert.deepEqual(readFileSync(again), readFileSync(executables.small), "a second build of the same program is the same artifact");
  assert.notDeepEqual(readFileSync(executables.small), readFileSync(executables.system), "the allocator is part of the build");

  // D. The fixtures of the other suites through objects built with --allocator small: the answers are the ones of
  // tests/primitives.mjs and tests/tasks.mjs, and afterwards every block that was cut from a region is back in a cache or in
  // the central stack (tz_small_stats is for tests; it is hidden, so it is only reachable from the program it is linked into).
  const header = (source, name, extra) => {
    const path = join(work, `tz-${name}.h`);
    cli(["build", source, "--emit", "header", ...extra, "-o", path]);
    return path;
  };
  const host = (name, body) => {
    const path = join(work, `${name}-host.c`);
    writeFileSync(path, `#undef NDEBUG
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include "tz-${name}.h"
void tz_small_stats(uint64_t *out);
static uint64_t carved, regions;
static void settled(void) {
    uint64_t stats[5];
    tz_small_stats(stats);
    assert(stats[0] == stats[1]);
    assert(stats[0] >= carved);
    carved = stats[0];
    regions = stats[2];
}
int main(void) {
${body}
    printf("carved=%llu regions=%llu\\n", (unsigned long long)carved, (unsigned long long)regions);
    return 0;
}
`);
    return path;
  };
  // The program of tests/primitives.mjs without its generated numeric references.
  const primitives = join(work, "primitives");
  mkdirSync(primitives);
  for (const [from, to] of [["currying/Functions.tz", "Curried.tz"], ["currying/Classes.tt", "Classes.tt"], ["arrays/Arrays.tz", "Arrays.tz"], ["lists/Lists.tz", "Lists.tz"], ["primitives/Main.tz", "Main.tz"]]) {
    copyFileSync(join(root, "tests/fixtures", from), join(primitives, to));
  }
  const fixtures = {
    grow: { source: library, body: "    assert(tz_grow(100) == 5050); settled(); assert(tz_grow(0) == 0); settled(); assert(tz_grow(20000) == 200010000); settled();" },
    primitives: {
      source: primitives,
      body: `    assert(tz_ownership_stress(200000) == 512); settled();
    for (int i = 0; i < 500; ++i) { int n = i % 64; assert(tz_coalescing(n) == 16 * (3 * n + 3)); }
    settled();
    assert(tz_curried_churn(40000) == 2160054); settled();
    assert(tz_array_churn(100000) == 8); settled();
    assert(tz_list_churn(100000) == 8); settled();
    assert(tz_strings(1) == 18 && tz_strings(0) == 17); settled();`,
    },
    tasks: {
      source: join(root, "tests/fixtures/tasks"),
      // parallel_sum(c) = 0^2 + ... + (c-1)^2 = c(c-1)(2c-1)/6; heap_cycles(2048) is the sum of (n % 1024) + n over n = 1..2048,
      // 1047552 + 2098176; parallel_cycles(128) adds "even" (4) and "odd" (3) per round.
      body: `    assert(tz_parallel_sum(257) == 5625216); settled();
    assert(tz_heap_cycles(2048) == 3145728); settled();
    assert(tz_parallel_cycles(128) == 896); settled();
    assert(tz_owned_results() == 12 && tz_list_results() == 42); settled();`,
    },
  };
  // A Windows COFF object cannot embed the runtime that these programs need (E2002: the trap runtime; the thread pool of the tasks),
  // so Windows runs the unit tests (including their threads), the IR checks, and the executables of section C, not these objects.
  for (const [name, { source, body }] of windows ? [] : Object.entries(fixtures)) {
    const headerPath = header(source, name, ["--allocator", "small"]);
    assert.ok(headerPath);
    for (const optimization of ["-O0", "-O3"]) {
      const object = join(work, `${name}${optimization}.o`);
      build(source, ["--emit", "object", "--allocator", "small", optimization], object);
      const binary = join(work, `${name}${optimization}${suffix}`);
      execute(clang, ["-std=c11", "-O1", `-I${work}`, host(name, body), object, ...threadFlags, ...mathFlags, "-o", binary]);
      const output = execute(binary, []).stdout.trim();
      const match = /^carved=(\d+) regions=(\d+)$/.exec(output);
      // The size classes were used: something was cut from at least one region.
      assert.ok(match && Number(match[1]) > 0 && Number(match[2]) >= 1, `${name} ${optimization}: ${output}`);
      console.log(`small allocator: ${name} ${optimization} ${output}`);
    }
  }

  // E. The path of an allocation that finds a block in the thread's cache has no atomic instruction: the atomic operations
  // (the try-lock, the registry's publication) are all in functions that the fast path only calls when the cache is empty.
  const disassembler = windows ? null : process.env.TSUZURI_OBJDUMP ?? "objdump";
  const objdump = disassembler && spawnSync(disassembler, ["--version"], { encoding: "utf8" });
  if (objdump && !objdump.error && objdump.status === 0) {
    const object = join(work, "primitives-O3.o");
    const listing = execute(disassembler, ["-d", "--no-show-raw-insn", object]).stdout;
    const body = listing.split(/\n(?=[0-9a-f]+ <)/).find((part) => /^[0-9a-f]+ <_?tz_coalescing>/.test(part));
    assert.ok(body, "tz_coalescing is in the object");
    assert.ok(!/\b(ldaxr|stlxr|ldxr|stxr|swp\w*|cas\w*|ldadd\w*|ldset\w*|lock|xchg|cmpxchg|dmb)\b/.test(body), "no read-modify-write instruction in the fast path of tz_coalescing");
  } else {
    console.log("small allocator: no disassembler, the fast-path check is skipped");
  }

  // F. A program that links two objects has one allocator state: the stats, which come from one of the two copies of the code,
  // see the blocks that both objects cut. (A Windows linker refuses two weak definitions of one symbol without a COMDAT, so
  // two Tsuzuri objects do not link there at all.)
  if (!windows) {
    const objects = [];
    for (const [name, source] of [["a", `export def make_a :: i64 -> i64
fn make_a count =
    let mut values: Vec<i64> = Vec.empty()
    for index in 1i64 .. count do values = Vec.push index values
    let result = Vec.to_array values
    Array.sum ref result
`], ["b", `export def make_b :: i64 -> i64
fn make_b count =
    let values = new [i64](count, index -> index)
    Array.sum ref values
`]]) {
      const directory = join(work, `two-${name}`);
      mkdirSync(directory);
      writeFileSync(join(directory, "Main.tz"), source);
      const object = join(work, `two-${name}.o`);
      build(directory, ["--emit", "object", "--allocator", "small", "-O3"], object);
      objects.push(object);
    }
    const two = join(work, "two-host.c");
    writeFileSync(two, `#undef NDEBUG
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
int64_t tz_make_a(int64_t);
int64_t tz_make_b(int64_t);
void tz_small_stats(uint64_t *out);
int main(int argc, char **argv) {
    char which = argc > 1 ? argv[1][0] : 'c';
    if (which != 'b') assert(tz_make_a(20) == 210);
    if (which != 'a') assert(tz_make_b(5) == 10);
    uint64_t stats[5];
    tz_small_stats(stats);
    assert(stats[0] == stats[1]);
    printf("%llu\\n", (unsigned long long)stats[2]);
    return 0;
}
`);
    const twoBinary = join(work, "two-host");
    execute(clang, ["-std=c11", "-O1", two, ...objects, ...threadFlags, ...mathFlags, "-o", twoBinary]);
    const regionsOf = (which) => Number(execute(twoBinary, [which]).stdout);
    // The Vec grows by doubling through blocks of 32 bytes and up, the array of 5 integers is one block of 48 bytes: each
    // object cuts regions for a class the other never uses, so one shared state holds more regions than either alone.
    const [onlyA, onlyB, both] = [regionsOf("a"), regionsOf("b"), regionsOf("c")];
    assert.ok(onlyA >= 1 && onlyB >= 1 && both > onlyA && both > onlyB, `regions: a ${onlyA}, b ${onlyB}, both ${both}`);
    console.log(`small allocator: two objects share one state (regions ${onlyA} + ${onlyB} -> ${both})`);
  }

  // G. A shared library has its own allocator (its symbols are hidden): the host loads it and calls it.
  if (!windows) {
    const library_ = join(work, process.platform === "darwin" ? "libgrow.dylib" : "libgrow.so");
    build(library, ["--emit", "shared", "--allocator", "small"], library_);
    const loader = join(work, "loader.c");
    writeFileSync(loader, `#undef NDEBUG
#include <assert.h>
#include <dlfcn.h>
#include <stdint.h>
int main(void) {
    void *handle = dlopen("${library_.replaceAll("\\", "\\\\")}", RTLD_NOW);
    assert(handle);
    int64_t (*grow)(int64_t) = (int64_t (*)(int64_t))dlsym(handle, "tz_grow");
    assert(grow && grow(100) == 5050 && grow(20000) == 200010000);
    return dlclose(handle);
}
`);
    const loaded = join(work, "loader");
    execute(clang, ["-std=c11", loader, ...(process.platform === "darwin" ? [] : ["-ldl"]), "-o", loaded]);
    execute(loaded, []);
  }
  console.log("small allocator: ok");
} finally {
  rmSync(work, { recursive: true, force: true });
}
