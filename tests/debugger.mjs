import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

// G16: LLDB shows Tsuzuri values with scripts/lldb/tsuzuri_lldb.py, frames carry source names,
// and stepping neither goes back to an earlier line nor stops in the runtime or generated glue.
// Expected values come from the fixtures' expressions, not from the compiler's output.
const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const fixture = resolve("tests/fixtures/debug_view");
const formatter = resolve("scripts/lldb/tsuzuri_lldb.py");
const lldb = process.env.TSUZURI_LLDB ?? "lldb";
const root = mkdtempSync(join(tmpdir(), "tsuzuri-debugger-"));
let sessions = 0;

function execute(program, args) {
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 120_000, maxBuffer: 16 * 1024 * 1024 });
  if (program === lldb && result.error?.code === "ENOENT") throw new Error("lldb not found; install LLDB or set TSUZURI_LLDB");
  assert.ifError(result.error);
  assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result.stdout + result.stderr;
}

/** Builds `project` with `-g -O0` and runs one LLDB batch of `commands` on it. */
function session(project, commands) {
  const app = join(root, `app${++sessions}`);
  execute(compiler, ["build", project, "-g", "-O0", "-o", app]);
  return debug(app, [], commands);
}

/** Runs one LLDB batch of `commands` on the `-g` executable `app`, started with `args`. */
function debug(app, args, commands) {
  // macOS keeps the DWARF in the `.dwarf` file beside the executable; Linux keeps it inside.
  const options = ["-b", "-o", `command script import ${JSON.stringify(formatter)}`];
  if (process.platform === "darwin") options.push("-o", `target symbols add ${JSON.stringify(`${app}.dwarf`)}`);
  for (const command of commands) options.push("-o", command);
  const output = execute(lldb, [...options, "--", app, ...args]);
  assert.doesNotMatch(output, /Traceback|error:/, output);
  return output;
}

/** The text that LLDB prints for each command, by command. */
function sections(output) {
  const result = [];
  for (const part of output.split(/^\(lldb\) /m).slice(1)) {
    const newline = part.indexOf("\n");
    result.push({ command: part.slice(0, newline), text: part.slice(newline + 1) });
  }
  return result;
}

try {
  execute(lldb, ["--version"]);
  const output = session(fixture, [
    "type category list tsuzuri",
    "breakpoint set -n Main.show",
    "breakpoint set -f Main.tz -l 8",
    "breakpoint set -f Main.tz -l 14",
    "breakpoint set -f Main.tz -l 24",
    "run",
    "frame variable",
    "continue",
    "thread step-in",
    "frame info",
    "continue",
    "frame variable",
    "frame variable point.x point.label",
    "image lookup -r -n '^Main\\.show\\.lambda@22:'",
    "kill",
  ]);
  const parts = sections(output);
  const text = command => parts.filter(part => part.command === command).map(part => part.text);
  assert.match(text("type category list tsuzuri")[0], /Category: tsuzuri \(enabled\)/);
  // No linkage name: frames and breakpoints use source names, and a breakpoint on the function
  // stops at its first line, after the parameters are bound.
  assert.match(text("breakpoint set -n Main.show")[0], /^Breakpoint 1: where = app\d+`Main\.show \+ \d+ at Main\.tz:8:/);
  assert.match(text("run")[0], /`Main\.show\(input=40\) at Main\.tz:8/);
  // Locals are not yet initialized at line 8; the formatters report them without raising.
  const [early, late] = text("frame variable");
  assert.match(early, /^\(i64\) input = 40$/m);
  // A binding's store is located at its declaration, so stepping goes forward to line 15.
  assert.match(text("frame info")[0], /`Main\.show\(input=40\) at Main\.tz:15/);
  for (const line of [
    "(i64) input = 40",
    '(string) text = "h\u00e9llo"',
    '(utf8string) bytes = u8"abc"',
    "(char) letter = 'A'",
    "(Main.Color) color = Green",
    "(Main.Shape) shape = Rect(1.5, 2)",
    "(Main.Tree) tree = Node(Leaf, 40, Leaf)",
    "(Maybe<i64>) maybe = Some(40)",
    "(Maybe<i64>) nothing = None",
    '(Result<i64, string>) outcome = Error("bad")',
    "([i64]) values = length=3 {\n  [0] = 1\n  [1] = 2\n  [2] = 3\n}",
    "([|i32|]) chain = length=2 {\n  [0] = 4\n  [1] = 5\n}",
    "(Vec<i32>) growing = length=1 capacity=4 {\n  [0] = 7\n}",
    "(Map<i32, i32>) table = size=1 {",
    "(i64 -> i64) add = <fn Main.show.lambda@22:15>",
    "(i64) total = 6",
  ]) assert.ok(late.includes(line), `${line}\n${late}`);
  assert.match(late, /table = size=1 \{\n {2}\[0\] = \(key = 1, value = 10\)/);
  assert.match(text("frame variable point.x point.label")[0], /^\(i64\) point\.x = 40\n\(string\) point\.label = "p"$/m);
  // The lambda and its specialization for the known call share the lambda's source name.
  const lookup = text("image lookup -r -n '^Main\\.show\\.lambda@22:'")[0];
  assert.match(lookup, /\d+ match(es)? found/, lookup);
  assert.equal(lookup.match(/Summary: app\d+`Main\.show\.lambda@22:15 at Main\.tz/g)?.length, lookup.match(/Summary:/g).length, lookup);

  // Stepping from line 18 enters std functions and the lambda, which have source, but not the
  // runtime (`Array.sum` calls a C kernel) or the builtin wrappers of `Vec.empty`, `Vec.push`.
  const steps = sections(session(fixture, ["breakpoint set -f Main.tz -l 18", "run", ...Array(16).fill("thread step-in"), "kill"]))
    .filter(part => part.command === "thread step-in")
    .map(part => /frame #0: 0x[0-9a-f]+ app\d+`(.+?)(?:\(.*\))? at ([^:\s]+):(\d+)/.exec(part.text));
  assert.ok(steps.every(Boolean), "every step stops in a frame with a source line");
  const frames = steps.map(([, name, file, line]) => ({ name, file, line: Number(line) }));
  for (const frame of frames) {
    assert.match(frame.file, /\.tz$/, JSON.stringify(frame));
    assert.doesNotMatch(frame.name, /^(tz\.|tsuzuri_|\$)/, JSON.stringify(frame));
  }
  const lines = frames.filter(frame => frame.name === "Main.show").map(frame => frame.line);
  assert.deepEqual(lines, [...lines].sort((left, right) => left - right), `stepping went back: ${lines}`);
  for (const name of ["Map.singleton", "Array.sum", "Main.show.lambda@22:15"]) {
    assert.ok(frames.some(frame => frame.name === name), `${name}: ${JSON.stringify(frames)}`);
  }

  // Edge cases: nesting limit, truncation, escapes, slices, sets, tasks, and references.
  const extras = join(root, "extras");
  mkdirSync(extras);
  writeFileSync(join(extras, "Main.tz"), [
    "union Nest = Stop | Wrap of Nest",
    "record Holder { name: string, tags: [string] }",
    "",
    "def inspect :: i64 -> i64",
    "fn inspect seed =",
    "    let nest = Wrap (Wrap (Wrap (Wrap (Wrap Stop))))",
    '    let unit_text = "ab"',
    "    let long = String.repeat (ref unit_text) 600",
    '    let escaped = "tab\\there \\"q\\" \\u{d800}!"',
    '    let names = ["x", "y"]',
    "    let window = ref names",
    "    let wide = u8'\\u{e9}'",
    "    let empty = ()",
    "    let keys = Set.insert (Set.insert (Set.singleton 3) 1) 2",
    "    let job = task { return seed }",
    '    let holder = Holder { name: "h", tags: ["t"] }',
    "    let label = ref holder.name",
    "    let total = Array.length window + long.length + escaped.length + Task.run job",
    "    total + (if wide == u8'\\u{e9}' then 1 else 0) + label.length - seed",
    "",
    "inspect 5",
    "",
  ].join("\n"));
  const edge = sections(session(extras, ["breakpoint set -f Main.tz -l 19", "run", "frame variable", "kill"]))
    .find(part => part.command === "frame variable").text;
  for (const line of [
    "(Main.Nest) nest = Wrap(Wrap(Wrap(Wrap(...))))",
    `(string) long = "${"ab".repeat(512)}..."`,
    '(string) escaped = "tab\\there \\"q\\" \\u{d800}!"',
    '(ref [string]) window = length=2 {\n  [0] = "x"\n  [1] = "y"\n}',
    "(utf8char) wide = u8'\u00e9'",
    "(unit) empty = ()",
    "(Set<i32>) keys = size=3 {\n  [0] = 1\n  [1] = 2\n  [2] = 3\n}",
    "(Task<i64>) job = <task>",
    '  tags = length=1 {\n    [0] = "t"\n  }',
    "(i64) total = 1222",
  ]) assert.ok(edge.includes(line), `${line}\n${edge}`);
  assert.match(edge, /^\(ref string\) label = 0x[0-9a-f]+ "h"$/m);

  // Phase 2: `test --index N -g -o PATH` builds a runner of test N alone, without running it; a
  // debugger starts it with the printed arguments and stops in the test's body.
  const specs = join(root, "specs");
  mkdirSync(specs);
  writeFileSync(join(specs, "Main.tz"), [
    "def calculate :: i64 -> i64 = \\value ->",
    '    let label = "ok"',
    "    value + label.length",
    "",
    "test \"same\" = assert true",
    "test \"body\" =",
    "    let total = calculate 40",
    '    let words = ["a", "b"]',
    "    assert (total == 42 && words.length == 2)",
    "",
  ].join("\n"));
  const runner = join(root, "runner");
  const built = spawnSync(compiler, ["test", specs, "--index", "1", "-g", "-o", runner, "--json"], { encoding: "utf8" });
  assert.equal(built.status, 0, built.stderr);
  const launch = JSON.parse(built.stdout);
  assert.deepEqual([launch.type, launch.index, launch.name, launch.arguments], ["debug", 1, "body", ["0"]]);
  const test = sections(debug(launch.program, launch.arguments, [
    "breakpoint set -f Main.tz -l 9",
    "breakpoint set -f Main.tz -l 3",
    "run",
    "frame variable",
    "continue",
    "frame variable",
    "bt",
    "continue",
  ]));
  const at = command => test.filter(part => part.command === command).map(part => part.text);
  assert.match(at("breakpoint set -f Main.tz -l 9")[0], /`Main\.test@6:6 \+ \d+ at Main\.tz:9/);
  assert.match(at("run")[0], /`Main\.calculate\(value=40\) at Main\.tz:3/);
  const [callee, body] = at("frame variable");
  assert.match(callee, /^\(string\) label = "ok"$/m);
  assert.match(at("continue")[0], /`Main\.test@6:6 at Main\.tz:9/);
  assert.match(body, /^\(i64\) total = 42$/m);
  assert.ok(body.includes('([string]) words = length=2 {\n  [0] = "a"\n  [1] = "b"\n}'), body);
  // The runner's entry has no debug information, like the runtime.
  assert.match(at("bt")[0], /frame #1: 0x[0-9a-f]+ runner`tsuzuri_test_run \+ \d+\n/);
  assert.match(at("continue")[1], /exited with status = 0/);
  console.log("debugger: LLDB formatters, names and stepping verified");
} finally { rmSync(root, { recursive: true, force: true }); }
