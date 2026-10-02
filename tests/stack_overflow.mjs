// E14 Phase 3: native executables of programs that can recurse report a stack overflow themselves.
// Usage: node tests/stack_overflow.mjs [path/to/tsuzuri]
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

if (process.platform === "win32") {
  console.log("stack_overflow: skipped (Windows reports through G10)");
  process.exit(0);
}

const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const clang = process.env.TSUZURI_CLANG ?? "clang";
const root = mkdtempSync(join(tmpdir(), "tsuzuri-stack-overflow-"));
const report = "trap: stack overflow";

function execute(program, args, success = true, environment = {}) {
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 300_000, maxBuffer: 8 * 1024 * 1024, env: { ...process.env, ...environment } });
  assert.ifError(result.error);
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}

function project(name, source) {
  const directory = join(root, name);
  mkdirSync(directory);
  writeFileSync(join(directory, "Main.tz"), source);
  return directory;
}

// The multiplication keeps the recursion non-tail, so LLVM cannot turn it into a loop.
const depth = "def rec depth :: i64 -> i64\nfn rec depth n = if n == 0 then 0 else depth (n - 1) * 3 + n\n\n";
const projects = {
  deep: project("deep", `${depth}let result = depth 100000000\nresult\n`),
  worker: project("worker", `${depth}let results = Task.run (Task.parallel (new [Task<i64>](4, i -> task { depth (100000000 + i) })))\nresults[0]\n`),
  shallow: project("shallow", `${depth}let result = depth 20\nresult\n`),
  hello: project("hello", "let value = 6 * 7\nvalue\n"),
  foreign: project("foreign", `${depth}extern "e14_crash" def crash :: i64 -> i64\n\ncrash (depth 4)\n`),
  signals: project("signals", `${depth}extern "e14_signal" def signal :: i64 -> i64\n\nsignal (depth 4)\n`),
  library: project("library", `${depth}export def deep :: i64 -> i64\nfn deep n = depth n\n`),
};
const host = join(root, "host.c");
writeFileSync(host, `#include <signal.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
int64_t e14_crash(int64_t value) {
    *(volatile int *)0 = (int)value;
    return 0;
}
int64_t e14_signal(int64_t value) {
    const char *mode = getenv("TSUZURI_TEST_SIGNAL");
    raise(mode && strcmp(mode, "bus") == 0 ? SIGBUS : SIGSEGV);
    return value;
}
`);
const hostObject = join(root, "host.o");
execute(clang, ["-c", host, "-o", hostObject]);

const aborted = (result) => result.signal === "SIGABRT";
const invalidAccess = (result) => ["SIGSEGV", "SIGBUS"].includes(result.signal);
const symbols = (file) => execute("nm", ["-u", file]).stdout;
let cases = 0;

try {
  for (const optimization of ["-O0", "-O3"]) {
    // A deep recursion on the main thread and on a worker: the report, then abort.
    for (const name of ["deep", "worker"]) {
      const program = join(root, `${name}${optimization}`);
      execute(compiler, ["build", projects[name], optimization, "-o", program]);
      const result = execute(program, [], false);
      assert.ok(aborted(result), `${name} ${optimization}: ${result.signal} ${result.status}`);
      assert.match(result.stderr, new RegExp(`^${report}\\n`), `${name} ${optimization}`);
      // Every worker that overflows may write it before the first fault ends the process.
      if (name === "deep") assert.equal(result.stderr.split(report).length, 2, "the report is written once");
      cases++;
    }

    // The same program stays correct when it does not overflow, and `run` names the cause.
    const shallow = join(root, `shallow${optimization}`);
    execute(compiler, ["build", projects.shallow, optimization, "-o", shallow]);
    assert.equal(execute(shallow, []).status, 0);
    cases++;
    const run = execute(compiler, ["run", projects.deep, optimization], false);
    assert.equal(run.status, 1);
    assert.match(run.stderr, /E2005.*stack overflow: the stack was exhausted by deep recursion; reduce the recursion depth or use a loop/s);
    assert.doesNotMatch(run.stderr, /probably/);
    cases++;

    // A debug executable keeps its report (its own object, without debug information).
    const debug = join(root, `debug${optimization}`);
    execute(compiler, ["build", projects.deep, optimization, "-g", "-o", debug]);
    const crashed = execute(debug, [], false);
    assert.ok(aborted(crashed));
    assert.match(crashed.stderr, new RegExp(`^${report}\\n`));
    cases++;

    // Only programs that can recurse carry the handler; objects never do (a host keeps its own signals).
    const hello = join(root, `hello${optimization}`);
    execute(compiler, ["build", projects.hello, optimization, "-o", hello]);
    assert.doesNotMatch(symbols(hello), /sigaltstack|sigaction/);
    assert.match(symbols(join(root, `deep${optimization}`)), /sigaltstack/);
    assert.match(symbols(join(root, `deep${optimization}`)), /sigaction/);
    const object = join(root, `library${optimization}.o`);
    execute(compiler, ["build", projects.library, "--emit", "object", optimization, "-o", object]);
    assert.doesNotMatch(symbols(object), /sigaltstack|sigaction/);
    cases++;

    // A fault that is not a stack overflow is not reported as one, and `run` keeps the signal-based inference.
    const foreign = join(root, `foreign${optimization}`);
    execute(compiler, ["build", projects.foreign, optimization, "--link", hostObject, "-o", foreign]);
    assert.match(symbols(foreign), /sigaction/);
    const fault = execute(foreign, [], false);
    assert.ok(invalidAccess(fault));
    assert.doesNotMatch(fault.stderr, /stack overflow/);
    cases++;
    const inferred = execute(compiler, ["run", projects.foreign, optimization, "--link", hostObject], false);
    assert.equal(inferred.status, 1);
    assert.match(inferred.stderr, /E2005.*the stack was probably exhausted by deep recursion/s);
    cases++;

    const signals = join(root, `signals${optimization}`);
    execute(compiler, ["build", projects.signals, optimization, "--link", hostObject, "-o", signals]);
    assert.match(symbols(signals), /sigaction/);
    for (const [mode, expected] of [["segv", "SIGSEGV"], ["bus", "SIGBUS"]]) {
      const result = execute(signals, [], false, { TSUZURI_TEST_SIGNAL: mode });
      assert.equal(result.signal, expected, `${optimization} ${mode}: ${result.signal} ${result.status}`);
      assert.doesNotMatch(result.stderr, /stack overflow/);
      cases++;
    }
  }
  console.log(`stack_overflow: ${cases} cases`);
} finally {
  rmSync(root, { recursive: true, force: true });
}
