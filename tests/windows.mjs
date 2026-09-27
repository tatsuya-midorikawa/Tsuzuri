import assert from "node:assert/strict";
import { linkSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";

if (process.platform !== "win32") throw new Error("Windows E2E requires a Windows host; use TSUZURI_WINDOWS_SDK cargo test --test windows for cross-link validation");
const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri.exe");
const clang = process.env.TSUZURI_CLANG ?? "clang";
const directory = mkdtempSync(join(tmpdir(), "tsuzuri-windows-"));
const nativeFlags = ["--target=x86_64-pc-windows-msvc", "-fuse-ld=lld", "-Wno-override-module", "-D_CRT_SECURE_NO_WARNINGS"];
function execute(program, args, success = true, env = {}) {
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 180000, env: { ...process.env, ...env }, maxBuffer: 8 * 1024 * 1024 });
  assert.ifError(result.error);
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}
const cli = (args, success, env) => execute(compiler, args, success, env);
try {
  const project = join(directory, "console");
  mkdirSync(project);
  const input = join(project, "Main.tz");
  writeFileSync(input, 'let text = "\u65e5\u672c\u8a9e"\nDebug.print (&text)\n42\n');
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
    assert.equal(JSON.parse(failed.stderr).code, "E2002");
    assert.deepEqual(readFileSync(executable), previous);
    assert.equal(cli(["run", "examples/tasks", ...flags]).stdout, "21325334000\n");
    assert.equal(cli(["run", "examples/control", ...flags]).stdout, "42\n");
    const scheduler = join(directory, `scheduler-${optimization}.exe`);
    execute(clang, [...nativeFlags, "-std=c11", "-Wall", "-Wextra", "-Werror", ...flags, "tests/task_runtime.c", "-o", scheduler]);
    for (const topology of ["-1", "1", "4", "1000"]) execute(scheduler, [topology]);
    const failures = join(directory, `failures-${optimization}.exe`);
    execute(clang, [...nativeFlags, "-std=c11", ...flags, "tests/windows_runtime.c", "-o", failures]);
    execute(failures, []);
    for (const [mode, name] of [["create", "CreateThread"], ["wait", "WaitForSingleObject"], ["close", "CloseHandle"]]) {
      const result = execute(failures, [mode], false);
      assert.notEqual(result.status, 0);
      assert.ok(result.stderr.includes(`${name} failed`));
    }
    const ir = join(directory, `tasks-${optimization}.ll`);
    cli(["build", "tests/fixtures/tasks/Main.tz", "--emit", "llvm", "--trap-info", "-o", ir, ...flags]);
    assert.match(readFileSync(ir, "utf8"), /define dllexport i64 @tz_parallel_results_lowest/);
    const host = join(directory, "host.c");
    writeFileSync(host, '#include <stdint.h>\n#include <assert.h>\nextern int64_t tz_parallel_results_lowest(void), tz_parallel_results_owned(int64_t), tz_parallel_results_nested(void), tz_wide_numbers(void);\nint main(void) { assert(tz_parallel_results_lowest() == 2); assert(tz_parallel_results_owned(2048) == 6); assert(tz_parallel_results_nested() == 2016); assert(tz_wide_numbers()); return 0; }\n');
    const tasks = join(directory, `tasks-${optimization}.exe`);
    execute(clang, [...nativeFlags, ...flags, host, ir, "src/runtime/task.c", "-o", tasks]);
    execute(tasks, []);
    const rejected = cli(["build", "tests/fixtures/tasks/Main.tz", "--emit", "object", "-o", join(directory, "rejected.obj"), "--json"], false);
    assert.equal(JSON.parse(rejected.stderr).code, "E2002");
  }
  const alias = join(directory, "source-alias.ll");
  linkSync(input, alias);
  const protectedResult = cli(["build", input, "--emit", "llvm", "-o", alias, "--json"], false);
  assert.equal(JSON.parse(protectedResult.stderr).code, "E2003");
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
  console.log("Windows MSVC: native O0/O3, Win32 tasks/results/failure injection, UTF-8, hardlinks, atomic replacement, object ABI and WASM passed");
} finally {
  rmSync(directory, { recursive: true, force: true });
}
