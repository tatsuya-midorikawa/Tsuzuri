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
  const options = ["--no-lldbinit", "-b", "-o", "settings set target.disable-aslr false",
    "-o", `command script import ${JSON.stringify(formatter)}`];
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
  // PM03: the reading of a string of either form, without a session (a session needs a debugserver that can launch programs).
  execute("python3", ["tests/lldb_strings.py"]);
  // PM04: the walk over the chunks of a list, in the same way.
  execute("python3", ["tests/lldb_lists.py"]);
  execute("python3", ["tests/lldb_collections.py"]);
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
    // Garbage whose element address passes 2^64: the providers give no child, not an OverflowError.
    "script print('probe', *(getattr(tsuzuri_lldb, provider)(lldb.target.CreateValueFromData(name, " +
      "lldb.SBData.CreateDataFromUInt64Array(lldb.eByteOrderLittle, 8, words), lldb.frame.FindVariable(name).GetType()), " +
      "{}).get_child_at_index(index) for provider, name, words, index in [('SequenceProvider', 'values', [2**64 - 16, 3], 2), " +
      "('ListProvider', 'chain', [2**64 - 4, 2], 0)]))",
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
    // Built by List.cons, whose chunks hold 2 and then 3 of the elements.
    "([|i32|]) grown = length=5 {\n  [0] = 1\n  [1] = 2\n  [2] = 3\n  [3] = 4\n  [4] = 5\n}",
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
  assert.match(parts.find(part => part.command.startsWith("script print('probe'")).text, /^probe None None$/m);

  // Stepping from line 18 enters std functions and the lambda, which have source, but not the
  // runtime (`Array.sum` calls a C kernel) or the builtin wrappers of `Vec.empty`, `Vec.push`.
  // Stop once the three intended calls have been visited: a fixed number of steps can
  // overshoot into process teardown on another architecture. Read source data, not UI formatting.
  const stepScript = [
    "import json",
    "lldb.debugger.SetAsync(False)",
    "step_frames = []",
    "required = {'Map.singleton', 'Array.sum', 'Main.show.lambda@22:15'}",
    "for _ in range(16):",
    "    lldb.process.GetSelectedThread().StepInto()",
    "    frame = lldb.process.GetSelectedThread().GetFrameAtIndex(0)",
    "    entry = frame.GetLineEntry()",
    "    step_frames.append({'name': frame.GetFunctionName(), 'file': entry.GetFileSpec().GetFilename(), 'line': entry.GetLine()})",
    "    if required <= {item['name'] for item in step_frames}: break",
    "print('steps=' + json.dumps(step_frames))",
  ].join("\n");
  const stepped = session(fixture, ["breakpoint set -f Main.tz -l 18", "run",
    `script exec(${JSON.stringify(stepScript)})`, "kill"]);
  const frames = JSON.parse(/^steps=(.+)$/m.exec(stepped)?.[1] ?? "null");
  assert.ok(frames?.length && frames.every(frame => frame.name && frame.file && frame.line > 0),
    `every step stops in a frame with a source line\n${JSON.stringify(frames)}\n${stepped}`);
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
    "record Lookalike { code: i64, environment: i64, clone: i64, drop: i64 }",
    "record Descriptive { descriptor: i64, environment: i64 }",
    "",
    "def inspect :: i64 -> i64",
    "fn inspect seed =",
    "    let nest = Wrap (Wrap (Wrap (Wrap (Wrap Stop))))",
    '    let unit_text = "ab"',
    "    let long = String.repeat 600 (ref unit_text)",
    '    let escaped = "tab\\there \\"q\\" \\u{d800}!"',
    '    let names = ["x", "y"]',
    "    let window = ref names",
    "    let wide = u8'\\u{e9}'",
    "    let empty = ()",
    "    let keys = Set.insert 2 (Set.insert 1 (Set.singleton 3))",
    "    let job = task { return seed }",
    '    let holder = Holder { name: "h", tags: ["t"] }',
    "    let label = ref holder.name",
    "    let lookalike = Lookalike { code: 1, environment: 2, clone: 3, drop: 4 }",
    "    let descriptive = Descriptive { descriptor: 5, environment: 6 }",
    "    let total = Array.length window + long.length + escaped.length + Task.run job",
    "    total + (if wide == u8'\\u{e9}' then 1 else 0) + label.length - seed + lookalike.code - 1 + descriptive.descriptor - 5",
    "",
    "inspect 5",
    "",
  ].join("\n"));
  const edge = sections(session(extras, ["breakpoint set -f Main.tz -l 23", "run", "frame variable", "kill"]))
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
  // A record with the field names of a function value keeps the default record display: the old
  // names (code, environment, clone, drop) and the current ones (descriptor, environment) alike.
  assert.match(edge, /^\(Main\.Lookalike\) lookalike = (?:\(code = 1, environment = 2, clone = 3, drop = 4\)|\{\n  code = 1\n  environment = 2\n  clone = 3\n  drop = 4\n\})$/m);
  assert.match(edge, /^\(Main\.Descriptive\) descriptive = (?:\(descriptor = 5, environment = 6\)|\{\n  descriptor = 5\n  environment = 6\n\})$/m);

  // C12: cross from the 512-entry head to the next chunk, without eagerly reading every chunk.
  const chunked = join(root, "chunked");
  mkdirSync(chunked);
  writeFileSync(join(chunked, "Main.tz"), [
    "def build :: i64 -> Set<i64>",
    "fn build count =",
    "    let mut set: Set<i64> = Set.empty()",
    "    for key in 0i64 .. count - 1 do set = Set.insert key set",
    "    set",
    "",
    "def inspect :: i64 -> i64",
    "fn inspect count =",
    "    let many = build count",
    "    Set.length (ref many)",
    "",
    "inspect 513",
    "",
  ].join("\n"));
  const crossing = sections(session(chunked, ["breakpoint set -f Main.tz -l 10", "run", "frame variable many", "frame variable many[0] many[511] many[512]", "kill"]));
  const summary = crossing.find(part => part.command === "frame variable many").text;
  assert.match(summary, /^\(Set<i64>\) many = size=513 \{$/m, summary);
  assert.match(summary, /^ {2}\[0\] = 0$/m, summary);
  const picked = crossing.find(part => part.command === "frame variable many[0] many[511] many[512]").text;
  for (const index of [0, 511, 512]) assert.match(picked, new RegExp(`^\\(i64\\) many\\[${index}\\] = ${index}$`, "m"), picked);

  // PM01: a union of a nullary case and a string or array payload stores no tag. `$niche` holds
  // the length -1 for the nullary case, and the formatter reads the case from it, whichever case
  // comes first. A native `Maybe<ref T>` is the pointer itself: the null pointer is `None`.
  const niches = join(root, "niches");
  mkdirSync(niches);
  writeFileSync(join(niches, "Main.tz"), [
    "union Alias = NoAlias | Spelled of string",
    "union Label = Named of string | Anonymous",
    "",
    "def inspect :: i64 -> i64",
    "fn inspect seed =",
    '    let present = Maybe.Some "x"',
    "    let absent: Maybe<string> = Maybe.None",
    '    let alias = Spelled "abc"',
    "    let nobody = NoAlias",
    "    let label = Anonymous",
    '    let named = Named "n"',
    "    let numbers: Maybe<[i64]> = Maybe.Some [1, 2]",
    "    let missing: Maybe<[i64]> = Maybe.None",
    "    let values = [10, 20]",
    "    let located = Array.try_findr (\\x -> deref x == 20) (ref values)",
    "    let nowhere = Array.try_findr (\\x -> deref x == 99) (ref values)",
    "    let total = seed + (match ref present with | Some text -> text.length | None -> 0)",
    "    total",
    "",
    "inspect 5",
    "",
  ].join("\n"));
  const niche = sections(session(niches, ["breakpoint set -f Main.tz -l 18", "run", "frame variable", "kill"]))
    .find(part => part.command === "frame variable").text;
  for (const line of [
    '(Maybe<string>) present = Some("x")',
    "(Maybe<string>) absent = None",
    '(Main.Alias) alias = Spelled("abc")',
    "(Main.Alias) nobody = NoAlias",
    "(Main.Label) label = Anonymous",
    '(Main.Label) named = Named("n")',
    "(Maybe<[i64]>) numbers = Some(length=2)",
    "(Maybe<[i64]>) missing = None",
    "(Maybe<ref i32>) nowhere = None",
    "(i64) total = 6",
  ]) assert.ok(niche.includes(line), `${line}\n${niche}`);
  assert.match(niche, /^\(Maybe<ref i32>\) located = Some\(0x[0-9a-f]+\)$/m);

  // PM01: a record that is smaller with its fields sorted by alignment stores them in that order.
  // The debug info lists the members as declared and gives each the offset where it lives, so
  // LLDB reads the values from the right bytes.
  const stored = join(root, "stored");
  mkdirSync(stored);
  writeFileSync(join(stored, "Main.tz"), [
    "record Spread { a: i8, b: i64, c: i8, d: i32, e: bool }",
    "record Frame { first: bool, inner: Spread, last: i16 }",
    "",
    "def inspect :: i64 -> i64",
    "fn inspect seed =",
    "    let spread = Spread { a: 1, b: seed + 1000, c: 3, d: 400, e: true }",
    "    let frame = Frame { first: true, inner: spread, last: 7 }",
    "    spread.b + frame.last as i64",
    "",
    "inspect 5",
    "",
  ].join("\n"));
  const placed = sections(session(stored, ["breakpoint set -f Main.tz -l 8", "run", "frame variable", "kill"]))
    .find(part => part.command === "frame variable").text;
  assert.match(placed, /\(Main\.Spread\) spread = [\s\S]*?a = (?:1|'\\x01')[\s\S]*?b = 1005[\s\S]*?c = (?:3|'\\x03')[\s\S]*?d = 400[\s\S]*?e = true/);
  assert.match(placed, /\(Main\.Frame\) frame = [\s\S]*?first = true[\s\S]*?inner = [\s\S]*?b = 1005[\s\S]*?d = 400[\s\S]*?last = 7/);

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
    "def rec fib :: i64 -> i64 = \\n -> if n < 2 then n else fib (n - 1) + fib (n - 2)",
    "test \"optimized\" =",
    "    let value = fib 20",
    "    assert (value == 6765)",
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
  // The dispatch is an artificial frame at the test's declaration; the C entry has no debug
  // information, like the runtime.
  assert.match(at("bt")[0], /frame #1: 0x[0-9a-f]+ runner`tsuzuri_test_run at Main\.tz:6:6\n\s*frame #2: 0x[0-9a-f]+ runner`main \+ \d+\n/);
  assert.match(at("continue")[1], /exited with status = 0/);

  // With optimization the dispatch inlines the test, which keeps its lines through the dispatch's
  // own debug information: breakpoints in the body bind and stop there.
  const optimized = join(root, "optimized");
  execute(compiler, ["test", specs, "--index", "2", "-g", "-O2", "-o", optimized]);
  const inlined = sections(debug(optimized, ["0"], [
    "breakpoint set -f Main.tz -l 12",
    "breakpoint set -f Main.tz -l 13",
    "run",
    "continue",
    "continue",
  ]));
  const stops = command => inlined.filter(part => part.command === command).map(part => part.text);
  for (const line of [12, 13]) {
    assert.match(stops(`breakpoint set -f Main.tz -l ${line}`)[0], new RegExp(`Main\\.test@11:6(?: \\+ \\d+)? at Main\\.tz:${line}:`));
  }
  // Apple LLDB prints [opt]; LLVM LLDB prints [inlined] for the same optimized inline frame.
  assert.match(stops("run")[0], /stop reason = breakpoint 1\.1\n.*`Main\.test@11:6 at Main\.tz:12:\d+ \[(?:opt|inlined)\]/);
  assert.match(stops("continue")[0], /stop reason = breakpoint 2\.1\n.*`Main\.test@11:6 at Main\.tz:13:\d+ \[(?:opt|inlined)\]/);
  assert.match(stops("continue")[1], /exited with status = 0/);
  console.log("debugger: LLDB formatters, names and stepping verified");
} finally { rmSync(root, { recursive: true, force: true }); }
