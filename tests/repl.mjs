// End-to-end tests of `tsuzuri repl` (G13): scripted stdin, exact stdout, and diagnostic codes and
// positions on stderr. Expected values come from hand computation and from `tsuzuri run` of the same
// programs written as Main.tz, never from the REPL's own output.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const root = mkdtempSync(join(tmpdir(), "tsuzuri-repl-"));
const cache = join(root, "cache");

function repl(lines, { args = [], files = {} } = {}) {
  const cwd = mkdtempSync(join(root, "case-"));
  for (const [name, text] of Object.entries(files)) writeFileSync(join(cwd, name), text);
  const input = lines.map(line => `${line}\n`).join("");
  const result = spawnSync(compiler, ["repl", ...args], {
    input, cwd, encoding: "utf8", timeout: 600_000, maxBuffer: 64 * 1024 * 1024,
    env: { ...process.env, TSUZURI_CACHE_DIR: cache },
  });
  assert.ifError(result.error);
  return result;
}

function count(text, part) {
  return text.split(part).length - 1;
}

function check(id, lines, stdout, { status = 0, errors = [], quiet = false, ...options } = {}) {
  const result = repl(lines, options);
  const context = `${id}\n--- stdout\n${result.stdout}\n--- stderr\n${result.stderr}`;
  assert.equal(result.status, status, context);
  assert.equal(result.stdout, stdout.map(line => `${line}\n`).join(""), context);
  for (const part of errors) assert.ok(result.stderr.includes(part), `${context}\n--- missing: ${part}`);
  if (quiet) assert.equal(result.stderr, "", context);
  return result;
}

// The ticket's example session. `square base` with `square :: i64 -> i64` is 36, and 12 after the
// redefinition; an unsuffixed literal that nothing constrains defaults to i32 (docs/language.md).
const example = [
  "def square :: i64 -> i64 = \\x -> x * x",
  "let base = 6",
  "square base",
  ":type square",
  "def square :: i64 -> i64 = \\x -> x + x",
  "square base",
  "\"hi\"",
  "square",
  "let word = (",
  "    match base with",
  "    | 6 -> \"six\"",
  "    | _ -> \"other\"",
  ")",
  "",
  "word",
  "let broken = 10 / (base - 6)",
  "do! IO.write_line \"hello\"",
  ":list",
  ":quit",
  "1 + 1",
];
const exampleOutput = [
  "base: i32",
  "it: i64 = 36",
  "i64 -> i64",
  "it: i64 = 12",
  "it: string = hi",
  "it: i64 -> i64",
  "word: string",
  "it: string = six",
  "hello",
  "def square :: i64 -> i64 = \\x -> x + x",
  "let base = 6",
  "let word = (",
  "    match base with",
  "    | 6 -> \"six\"",
  "    | _ -> \"other\"",
  ")",
];
const r1 = check("R1", example, exampleOutput, { errors: ["input:1:14: error[E2005]"] });
assert.equal(count(r1.stderr, "error["), 1, r1.stderr);
// The trap's own line names the generated program, whose path does not depend on the run.
assert.ok(r1.stderr.includes("trap: integer division by zero at <repl>/Main.tz:"), r1.stderr);

check("R2", ["let = 3", "1 + 1"], ["it: i32 = 2"], { errors: ["input:1:5: error[E0002]"] });

// Redefining `square` with another type breaks `quad`, which stays in the session: the input is
// rejected at quad's line of `:list`, and `quad 2` = square (square 2) = 16 still works.
const r3 = check("R3", [
  "def square :: i64 -> i64 = \\x -> x * x",
  "def quad :: i64 -> i64 = \\x -> square (square x)",
  "quad 2",
  "def square :: bool -> bool = \\x -> x",
  "quad 2",
], ["it: i64 = 16", "it: i64 = 16"], { errors: ["session:2:", "error[E1"] });
assert.ok(!r3.stderr.includes("input:"), r3.stderr);

// A replaced binding keeps its place, so `b` is recomputed from the new `a`: 2 * 10, then 5 * 10.
check("R4", ["let a = 2", "let b = a * 10", "b", "let a = 5", "b"],
  ["a: i32", "b: i32", "it: i32 = 20", "a: i32", "it: i32 = 50"], { quiet: true });

check("R5", ["let c = 1; let c = c + 1", "c"], ["c: i32", "c: i32", "it: i32 = 2"], { quiet: true });

// `:type` does not run its expression (10 / 0 would trap), an IO value is not run, and an action
// runs once and is not kept.
check("R6", ["let z = 0", ":type 10 / z", "IO.write_line \"y\"", "do! IO.write_line \"x\"", "do! IO.write_line \"x\"", "1 + 1"],
  ["z: i32", "i32", "it: IO<unit>", "x", "x", "it: i32 = 2"], { quiet: true });

// The loaded program reads a line; the REPL's own stdin ("1 + 1") must not reach it, so it reads
// the end of its input and prints nothing. Actions do not appear in `:list`.
const echo = "let! line = IO.read_line ()\nlet! value = line\ndo! IO.write_line value\n";
check("R7", [":load Echo.tz", "1 + 1", ":list"], ["it: i32 = 2"], { files: { "Echo.tz": echo }, quiet: true });

const r8 = check("R8", [":foo", ":type", ":load missing.tz", ":load notes.txt", ":list extra", "1"], ["it: i32 = 1"], {
  errors: [
    "unknown command ':foo'; commands are :type, :load, :list, :reset, and :quit",
    ":type requires an expression",
    ":load requires one .tz file path",
    ":list, :reset, and :quit take no arguments",
  ],
});
assert.equal(count(r8.stderr, "input:1:1: error[E2000]"), 4, r8.stderr);
assert.equal(count(r8.stderr, "input:1:1: error[E2001]"), 1, r8.stderr);

const r9 = check("R9", [
  "extern def drop_log :: i64 -> unit",
  "test \"adds numbers\" = assert (1 + 2 == 3)",
  "def main :: unit -> i32 = \\() -> 1",
  "1",
], ["it: i32 = 1"], {
  errors: [
    "input:1:12: error[E2000]: extern declarations are not supported in the REPL",
    "input:1:6: error[E2000]: test declarations are not supported in the REPL",
    "input:1:5: error[E2000]: 'main' cannot be defined in the REPL",
  ],
});
assert.equal(count(r9.stderr, "error["), 3, r9.stderr);

check("R10", ["while true do ()", "1 + 1"], ["it: i32 = 2"], {
  args: ["--timeout", "1"],
  errors: ["input:1:1: error[E2005]: evaluation exceeded the 1-second limit and was stopped; use --timeout to change it"],
});

// 17 MiB + 1 byte, as `tsuzuri run` of the same Main.tz writes.
check("R11", ["do! IO.write_line (String.repeat \"x\" (17l * 1048576l))", "1 + 1"], ["it: i32 = 2"], {
  errors: ["input:1:1: error[E2005]: program output exceeded 16 MiB and the program was stopped"],
});

check("R12", [`"${"a".repeat(1_100_000)}"`, "1 + 1"], ["it: i32 = 2"], { errors: ["input:1:1: error[E0003]"] });

// The optimization level changes only the time.
const r13 = check("R13", example, exampleOutput, { args: ["-O3"], errors: ["input:1:14: error[E2005]"] });
assert.equal(r13.stdout, r1.stdout);

for (const [args, message] of [
  [["Main.tz"], "repl takes no paths; supported options are -O0 to -O3, --cpu, --no-cache, and --timeout"],
  [["--target", "wasm32"], "repl supports only the native target; build WebAssembly with 'tsuzuri build --target wasm32'"],
  [["--timeout", "3601"], "--timeout requires whole seconds from 0 to 3600"],
]) {
  check(`R14 ${args.join(" ")}`, ["1 + 1"], [], { args, status: 2, errors: [`<command line>:1:1: error[E2000]: ${message}`] });
}

// A binding runs again for every evaluation, so its Debug output repeats on stderr (D5); `t + 1` = 6.
const traced = check("R15", ["let t = Debug.trace 5", "t + 1", "t + 2"], ["t: i32", "it: i32 = 6", "it: i32 = 7"]);
assert.equal(count(traced.stderr, "5\n"), 3, traced.stderr);

// A top-level IO value is not run when it is bound or shown, only by an action.
check("R16", ["let action = IO.write_line \"once\"", "action", "do! action"],
  ["action: IO<unit>", "it: IO<unit>", "once"], { quiet: true });

// A statement that ends with ';' stays in the session, like a binding, so `c` is 1 + 1.
check("R17", ["let mut c = 1", "c = c + 1;", "c"], ["c: i32", "it: i32 = 2"], { quiet: true });

// A loaded file keeps a separate signature with its definition, and a value without a Display
// instance shows only its type, spelled as in the checker's messages (`found Main.Point`).
const shapes = "record Point { x: i64, y: i64 }\ndef shift :: Point -> Point\n/// Moves right.\ndef width :: i64 = 3\nfn shift p = Point { x: p.x + width(), y: p.y }\n";
check("R18", [":load Shapes.tz", "(shift (Point { x: 1, y: 2 })).x", "Point { x: 1, y: 2 }", ":list"], [
  "it: i64 = 4",
  "it: Main.Point",
  "record Point { x: i64, y: i64 }",
  "def shift :: Point -> Point\nfn shift p = Point { x: p.x + width(), y: p.y }",
  "/// Moves right.\ndef width :: i64 = 3",
], { files: { "Shapes.tz": shapes }, quiet: true });

rmSync(root, { recursive: true, force: true });
console.log("REPL: scripted sessions, redefinition, session state, commands, limits, diagnostics, and -O0/-O3 passed");
