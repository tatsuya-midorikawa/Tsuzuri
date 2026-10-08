// End-to-end tests of `tsuzuri script` and the shebang line (G13 Phase 2). Expected outputs come from
// hand computation and from `tsuzuri run` of the same source as a project's Main.tz, never from
// `tsuzuri script` itself.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { chmodSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, dirname, join, resolve } from "node:path";

const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const root = mkdtempSync(join(tmpdir(), "tsuzuri-script-"));
const cache = join(root, "cache");
const env = { ...process.env, TSUZURI_CACHE_DIR: cache };
const entries = () => readdirSync(cache).filter(name => /^[0-9a-f]{64}$/.test(name));

function spawn(program, args, { cwd = root, input = "", extraEnv = {} } = {}) {
  const result = spawnSync(program, args, {
    cwd, input, encoding: "utf8", timeout: 600_000, maxBuffer: 16 * 1024 * 1024, env: { ...env, ...extraEnv },
  });
  assert.ifError(result.error);
  return result;
}

const tsuzuri = (args, options) => spawn(compiler, args, options);

function expect(result, status, stdout, errors = []) {
  const context = `--- stdout\n${result.stdout}\n--- stderr\n${result.stderr}`;
  assert.equal(result.status, status, context);
  if (stdout !== undefined) assert.equal(result.stdout, stdout, context);
  for (const part of errors) assert.ok(result.stderr.includes(part), `${context}\n--- missing: ${part}`);
}

const shebang = "#!/usr/bin/env tsuzuri script\n";
const files = {
  "hello.tz": `${shebang}let name = "script"\n"hello, " + name\n`,
  "args.tz": `${shebang}def main :: Array<string> -> i32 = \\args ->\n    do! IO.write_line (String.join (ref "|") (ref args))\n    0\n`,
  "exit.tz": `${shebang}def main :: unit -> i32 = \\() -> 3\n`,
  "trap.tz": `${shebang}let x = 10 / (5 - 5)\nx\n`,
  "echo.tz": `${shebang}let! line = IO.read_line ()\nlet! value = line\ndo! IO.write_line ("got " + value)\n`,
  "unused.tz": `${shebang}let unused = 1\n2\n`,
  // A sibling with a syntax error: `tsuzuri run` of the folder would read it, a script does not.
  "Broken.tz": "def broken :: i64 = (\n",
  "Traits.tt": "class Show<'a> {\n    def show :: 'a -> string\n}\n",
};
for (const [name, text] of Object.entries(files)) writeFileSync(join(root, name), text);

// The program's result prints like `tsuzuri run`; the shebang line is a comment.
expect(tsuzuri(["script", "hello.tz"]), 0, "hello, script\n");

// Everything after the file goes to the program unchanged, including options and `--`.
expect(tsuzuri(["script", "-O3", "--no-cache", "args.tz", "a", "b c", "--", "--help", "-O0", ""]), 0, "a|b c|--|--help|-O0|\n");
expect(tsuzuri(["script", "args.tz"]), 0, "\n");
expect(tsuzuri(["script", "--", "args.tz", "x"]), 0, "x\n");

// The exit code, the trap report, and their positions match `tsuzuri run` of the same Main.tz.
const project = join(root, "project");
mkdirSync(project);
for (const [name, status] of [["exit.tz", 1], ["trap.tz", 1]]) {
  writeFileSync(join(project, "Main.tz"), files[name]);
  const viaRun = tsuzuri(["run", "project"]);
  const viaScript = tsuzuri(["script", name]);
  assert.equal(viaRun.status, status, viaRun.stderr);
  assert.equal(viaScript.status, status, viaScript.stderr);
  assert.equal(viaScript.stdout, viaRun.stdout);
  assert.equal(viaScript.stderr, viaRun.stderr.replaceAll("project/Main.tz", name), viaScript.stderr);
}
expect(tsuzuri(["script", "exit.tz"]), 1, "", ["exit.tz:1:1: error[E2005]: program exited with code 3"]);
// Line 2, column 9 is `10 / (5 - 5)`; the shebang line counts as line 1.
expect(tsuzuri(["script", "trap.tz"]), 1, "", ["trap: integer division by zero at trap.tz:2:9", "trap.tz:2:9: error[E2005]"]);
const json = tsuzuri(["script", "--json", "trap.tz"]);
expect(json, 1, "", ['"code":"E2005"', '"path":"trap.tz"', '"line":2,"column":9']);

// Standard input reaches the program, and warnings behave as for `tsuzuri run`.
expect(tsuzuri(["script", "echo.tz"], { input: "abc\n" }), 0, "got abc\n");
expect(tsuzuri(["script", "unused.tz"]), 0, "2\n", ["unused.tz:2:5: warning[W1001]"]);
expect(tsuzuri(["script", "--deny-warnings", "unused.tz"]), 1, "", ["unused.tz:2:5: warning[W1001]"]);

// A second run of the same script reuses the cached executable.
rmSync(cache, { recursive: true, force: true });
expect(tsuzuri(["script", "hello.tz"]), 0, "hello, script\n");
assert.equal(entries().length, 1);
expect(tsuzuri(["script", "hello.tz"]), 0, "hello, script\n");
assert.equal(entries().length, 1);

// Command-line errors exit with 2 and source errors with 1, as for the other subcommands.
for (const [args, message] of [
  [["script"], "script needs a source file: tsuzuri script [options] FILE [arguments...]"],
  [["script", "--target", "wasm32", "hello.tz"], "script accepts -O0 to -O3, --cpu, --no-cache, --json, --deny-warnings, --warn, --link, -l, and -L before the file"],
  [["script", "-o", "out", "hello.tz"], "script accepts -O0 to -O3, --cpu, --no-cache, --json, --deny-warnings, --warn, --link, -l, and -L before the file"],
  [["script", "-O0", "-O3", "hello.tz"], "optimization specified more than once"],
]) {
  expect(tsuzuri(args), 2, "", [`<command line>:1:1: error[E2000]: ${message}`]);
}
expect(tsuzuri(["script", "missing.tz"]), 1, "", ["missing.tz:1:1: error[E2001]"]);
expect(tsuzuri(["script", "Traits.tt"]), 1, "", ["Traits.tt:1:1: error[E2000]: a script is a .tz program"]);

// `check` and `fmt` read the shebang line as a comment, and `fmt` keeps it.
const checked = join(root, "checked");
mkdirSync(checked);
writeFileSync(join(checked, "Main.tz"), files["hello.tz"]);
expect(tsuzuri(["check", checked]), 0, "");
const messy = join(root, "Messy.tz");
writeFileSync(messy, `${shebang}let   x =  1\nx\n`);
expect(tsuzuri(["fmt", messy]), 0, "");
assert.equal(readFileSync(messy, "utf8"), `${shebang}let x = 1\nx\n`);
expect(tsuzuri(["fmt", "--check", messy]), 0, "");

// Executed through its shebang line, with the compiler's directory first on PATH. macOS splits the
// shebang's arguments itself; Linux needs `env -S`.
if (process.platform !== "win32") {
  const path = `${dirname(compiler)}${delimiter}${process.env.PATH}`;
  const forms = process.platform === "darwin" ? ["#!/usr/bin/env tsuzuri script", "#!/usr/bin/env -S tsuzuri script"] : ["#!/usr/bin/env -S tsuzuri script"];
  for (const [index, line] of forms.entries()) {
    const script = join(root, `greet${index}`);
    writeFileSync(script, `${line}\ndef main :: Array<string> -> i32 = \\args ->\n    do! IO.write_line ("hi " + String.join (ref " ") (ref args))\n    0\n`);
    chmodSync(script, 0o755);
    expect(spawn(script, ["x", "y z"], { extraEnv: { PATH: path } }), 0, "hi x y z\n");
    // Through a symbolic link, as a script installed on PATH often is.
    const link = join(root, `linked${index}`);
    symlinkSync(script, link);
    expect(spawn(link, ["w"], { extraEnv: { PATH: path } }), 0, "hi w\n");
  }
}

rmSync(root, { recursive: true, force: true });
console.log("Script: arguments, exit codes, traps, stdin, warnings, cache, CLI errors, fmt, and shebang execution passed");
