import assert from "node:assert/strict";
import { linkSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";

if (process.platform !== "win32") throw new Error("Windows E2E requires a Windows host; use TSUZURI_WINDOWS_SDK cargo test --test windows for cross-link validation");
const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri.exe");
const clang = process.env.TSUZURI_CLANG ?? "clang";
assert.ok(["x64", "arm64"].includes(process.arch), `unsupported Windows architecture: ${process.arch}`);
const named = `${process.arch === "arm64" ? "aarch64" : "x86_64"}-pc-windows-msvc`;
const directory = mkdtempSync(join(tmpdir(), "tsuzuri-windows-"));
const nativeFlags = [`--target=${named}`, "-fuse-ld=lld", "-Wno-override-module", "-D_CRT_SECURE_NO_WARNINGS"];
function execute(program, args, success = true, env = {}) {
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 180000, env: { ...process.env, ...env }, maxBuffer: 8 * 1024 * 1024 });
  assert.ifError(result.error);
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}
const cli = (args, success, env) => execute(compiler, args, success, env);
// Warnings come first on stderr as JSON lines; the error that --json reports is the last one.
const diagnostic = (result) => JSON.parse(result.stderr.trim().split("\n").filter((line) => line.startsWith("{")).pop());
try {
  const project = join(directory, "console");
  mkdirSync(project);
  const input = join(project, "Main.tz");
  writeFileSync(input, 'let text = "\u65e5\u672c\u8a9e"\nDebug.print (ref text)\n42\n');
  for (const optimization of [0, 3]) {
    const flags = [`-O${optimization}`];
    const run = cli(["run", input, ...flags]);
    assert.equal(run.stdout, "42\n");
    assert.ok(run.stderr.includes("\u65e5\u672c\u8a9e\n"));
    const executable = join(directory, `console-${optimization}.exe`);
    cli(["build", input, "-o", executable, ...flags]);
    cli(["build", input, "-o", executable, ...flags]);
    const previous = readFileSync(executable);
    const failed = cli(["build", input, "-o", executable, ...flags, "--json"], false, { TSUZURI_CLANG: join(directory, "missing-clang.exe") });
    assert.equal(diagnostic(failed).code, "E2002");
    assert.deepEqual(readFileSync(executable), previous);
    // Routine.scope and Parallel on the tzroutine scheduler (TZ12); the scheduler's own C tests on fibers are
    // tests/tzroutine_runtime.mjs, which the Windows job runs too.
    assert.equal(cli(["run", "examples/tzroutines", ...flags]).stdout, "21325334000\n");
    assert.equal(cli(["run", "examples/control", ...flags]).stdout, "42\n");
    // The LLVM output of a program that spawns, linked by a host with src/runtime/tzroutine.c.
    const ir = join(directory, `tzroutine-${optimization}.ll`);
    cli(["build", "tests/fixtures/tzroutine/Main.tz", "--emit", "llvm", "--trap-info", "-o", ir, ...flags]);
    assert.match(readFileSync(ir, "utf8"), /define dllexport i64 @tz_sum_atomic/);
    const host = join(directory, "host.c");
    writeFileSync(host, '#include <stdint.h>\n#include <assert.h>\nextern int64_t tz_sum_atomic(int64_t), tz_nested(int64_t), tz_moves_owned(void), tz_go_owned(void), tz_counts(void);\nint main(void) { assert(tz_sum_atomic(100) == 4950); assert(tz_nested(10) == 2047); assert(tz_moves_owned() == 13); assert(tz_go_owned() == 11); assert(tz_counts() == 1); return 0; }\n');
    const spawning = join(directory, `tzroutine-${optimization}.exe`);
    execute(clang, [...nativeFlags, ...flags, host, ir, "src/runtime/tzroutine.c", "-o", spawning]);
    execute(spawning, []);
    const rejected = cli(["build", "tests/fixtures/tzroutine/Main.tz", "--emit", "object", "-o", join(directory, "rejected.obj"), "--json"], false);
    assert.equal(diagnostic(rejected).code, "E2002");
  }
  const alias = join(directory, "source-alias.ll");
  linkSync(input, alias);
  const protectedResult = cli(["build", input, "--emit", "llvm", "-o", alias, "--json"], false);
  assert.equal(diagnostic(protectedResult).code, "E2003");
  const library = join(directory, "library");
  mkdirSync(library);
  writeFileSync(join(library, "Main.tz"), "export def answer :: i64\nfn answer = 42\n");
  const object = join(directory, "answer.obj");
  cli(["build", library, "--emit", "object", "-o", object]);
  const host = join(directory, "answer.c");
  writeFileSync(host, "#include <stdint.h>\nextern int64_t tz_answer(void);\nint main(void) { return tz_answer() == 42 ? 0 : 1; }\n");
  const executable = join(directory, "answer.exe");
  execute(clang, [...nativeFlags, host, object, "-o", executable]);
  execute(executable, []);
  const wasm = join(directory, "answer.wasm");
  cli(["build", library, "--target", "wasm32", "-o", wasm]);
  const { module, instance } = await WebAssembly.instantiate(readFileSync(wasm));
  assert.deepEqual(WebAssembly.Module.imports(module), []);
  assert.equal(instance.exports.tz_answer(), 42n);
  // The machine's own target, named, builds, runs and tests like `--target native`.
  const suite = join(directory, "suite");
  mkdirSync(suite);
  writeFileSync(join(suite, "Main.tz"), 'test "passes" = assert true\ntest "adds" = assert (40 + 2 == 42)\n');
  const unused = { TSUZURI_RUNNER: "tsuzuri-runner-that-does-not-exist" };
  for (const optimization of ["-O0", "-O3"]) {
    for (const [example, output] of [["tzroutines", "21325334000\n"], ["control", "42\n"]]) {
      const executable = join(directory, `${example}-named${optimization}.exe`);
      cli(["build", `examples/${example}`, "--no-cache", optimization, "--target", named, "-o", executable]);
      assert.equal(execute(executable, []).stdout, output, `${example} ${optimization}`);
    }
    assert.equal(cli(["run", "examples/control", optimization, "--no-cache", "--target", named], true, unused).stdout, "42\n");
    assert.match(cli(["test", suite, optimization, "--target", named, "--json"], true, unused).stdout, /"passed":2,"failed":0/);
  }
  console.log(`Windows MSVC: the named target ${named} built, ran and tested at -O0 and -O3`);
  // A trap reports where it happened and ends the process with STATUS_ILLEGAL_INSTRUCTION (`llvm.trap`).
  const trapping = join(directory, "trap");
  mkdirSync(trapping);
  writeFileSync(join(trapping, "Main.tz"), "def divide :: i64 -> i64\nfn divide x = 10 / x\ndivide 0\n");
  for (const optimization of ["-O0", "-O3"]) {
    const trapped = diagnostic(cli(["run", trapping, optimization, "--json"], false));
    assert.equal(trapped.code, "E2005");
    assert.match(trapped.message, /exit code: 0xc000001d/);
    assert.match(trapped.message, /trap: integer division by zero at /);
    assert.equal(trapped.span.line, 2);
  }
  console.log("Windows MSVC: a trap reports its position and ends the process with 0xC000001D at -O0 and -O3");
  console.log("Windows MSVC: native O0/O3, tzroutines and Parallel, linked IR, UTF-8, hardlinks, atomic replacement, object ABI and WASM passed");
} finally {
  rmSync(directory, { recursive: true, force: true });
}
