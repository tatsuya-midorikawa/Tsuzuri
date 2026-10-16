import assert from "node:assert/strict";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { writeProject } from "../benchmarks/build/generate.mjs";

// PB04: the frontend (parse, body check, finish, ownership) runs on worker threads, up to
// TSUZURI_THREADS. Nothing the compiler prints or writes may depend on the number of threads:
// diagnostics (order, dedup, the 1,000 cap and the 50 shown), IR, the exit code, and the bytes of
// the frontend cache. TSUZURI_BASELINE=<compiler built before PB04> also compares with that
// compiler. The Rust tests (src/lib.rs, src/parallel.rs) compare the typed modules.
const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const baseline = process.env.TSUZURI_BASELINE ? resolve(process.env.TSUZURI_BASELINE) : null;
const repository = resolve(new URL("..", import.meta.url).pathname);
const directory = mkdtempSync(join(tmpdir(), "tsuzuri-parallel-frontend-e2e-"));
// Unset means the compiler's own choice: the available cores, at most 8.
const THREADS = ["1", "2", "8", undefined];
const name = threads => (threads === undefined ? "unset" : threads);

function run(executable, args, { threads, cache = "", extra = {} } = {}) {
  const env = { ...process.env, TSUZURI_CACHE_DIR: cache, TSUZURI_TIME_PASSES: "", ...extra };
  delete env.TSUZURI_THREADS;
  if (threads !== undefined) env.TSUZURI_THREADS = threads;
  const result = spawnSync(executable, args, { encoding: "utf8", timeout: 600000, env, maxBuffer: 256 * 1024 * 1024 });
  assert.ifError(result.error);
  return { status: result.status, stdout: result.stdout, stderr: result.stderr };
}
const cli = (args, options) => run(compiler, args, options);

// The same output at every thread count (and as the baseline compiler's, when there is one).
function equal(label, args, { counts = THREADS, status } = {}) {
  const reference = cli(args, { threads: counts[0] });
  if (status !== undefined) assert.equal(reference.status, status, `${label}: ${reference.stderr.slice(0, 600)}`);
  for (const threads of counts.slice(1)) {
    assert.deepEqual(cli(args, { threads }), reference, `${label}: ${name(threads)} threads differ from ${name(counts[0])}`);
  }
  if (baseline) assert.deepEqual(run(baseline, args), reference, `${label}: the baseline compiler differs`);
  return reference;
}

function emit(label, project, { target = "native", level = "-O0", counts = THREADS } = {}) {
  const outputs = [];
  const args = output => ["build", project, "--emit", "llvm", level, "--cpu", "generic", "--no-cache", "-o", output,
    ...(target === "native" ? [] : ["--target", target])];
  for (const threads of counts) {
    const output = join(directory, `${label}-${target}${level}-${name(threads)}.ll`);
    const result = cli(args(output), { threads });
    const text = existsSync(output) ? readFileSync(output) : null;
    outputs.push({ result: { ...result, stderr: result.stderr.replaceAll(output, "<out>"), stdout: result.stdout.replaceAll(output, "<out>") }, text });
  }
  outputs.forEach((output, index) => {
    assert.deepEqual(output.result, outputs[0].result, `${label} ${target}${level}: ${name(counts[index])} threads`);
    assert.ok(output.text === null && outputs[0].text === null || output.text?.equals(outputs[0].text), `${label} ${target}${level}: IR differs at ${name(counts[index])} threads`);
  });
  if (baseline) {
    const output = join(directory, `${label}-${target}${level}-baseline.ll`);
    const reference = run(baseline, args(output));
    assert.equal(reference.status, outputs[0].result.status);
    if (outputs[0].text) assert.ok(readFileSync(output).equals(outputs[0].text), `${label}: the baseline compiler's IR differs`);
  }
  return outputs[0];
}

// Every file of a corpus, written as Module<i>.tz next to a Main.tz.
function corpus(label, count, module, main = "1\n") {
  const project = join(directory, label);
  mkdirSync(project);
  writeFileSync(join(project, "Main.tz"), main);
  for (let index = 0; index < count; index++) {
    const [file, text] = module(index);
    const path = join(project, file ?? `Module${index}.tz`);
    mkdirSync(join(path, ".."), { recursive: true });
    writeFileSync(path, text);
  }
  return project;
}
const padding = `// ${"padding ".repeat(30)}\n`;
const modules = {
  moved: index => [null, `record Owned${index} { text: string, count: i64 }\n\ndef broken0 :: i64 -> i64 = \\x ->\n    let value = Owned${index} { text: "text", count: x + 0 }\n    let other = { value with count = x + 1 }\n    value.text.length\n`],
  typed: index => [null, Array.from({ length: 4 }, (_, f) => `def bad${f} :: i64 -> i64 = \\x -> x + "${index}"\n`).join("\n")],
  syntax: index => [null, `${padding}def broken${index} :: = \n`],
  incomplete: index => [null, `union Color${index} = Red | Green | Blue\n\ndef pick :: Color${index} -> i64 = \\color ->\n    match color with\n    | Red -> 1\n    | Green -> 2\n`],
  unused: index => [null, Array.from({ length: 24 }, (_, f) => `def f${f} :: i64 -> i64 = \\x ->\n    let unused${f} = x + ${index}\n    x * ${f}\n`).join("\n")],
  mixed: index => [null, index % 3 === 0 ? `${padding}def broken${index} :: = \n` : `${padding}def value${index} :: i64 = ${index}\n`],
};

const shown = result => result.stderr.split("\n").filter(line => /error\[|warning\[/.test(line)).length;
try {
  // 1. The projects of the repository: every example and fixture, checked at every thread count,
  // and the examples emitted.
  const examples = readdirSync(join(repository, "examples"), { withFileTypes: true }).filter(entry => entry.isDirectory()).map(entry => join(repository, "examples", entry.name));
  const fixtures = readdirSync(join(repository, "tests/fixtures"), { withFileTypes: true }).filter(entry => entry.isDirectory()).map(entry => join(repository, "tests/fixtures", entry.name));
  assert.ok(examples.length >= 10 && fixtures.length >= 90);
  for (const project of [...examples, ...fixtures]) equal(project, ["check", project]);
  for (const project of examples) emit(project, project);
  const hello = join(repository, "examples/hello");
  assert.equal(equal("hello", ["check", hello], { status: 0 }).stderr, "");

  // 2. Synthetic projects (benchmarks/build): enough functions for every stage to use workers.
  const s200 = join(directory, "s200");
  writeProject(s200, 200);
  equal("s200 check", ["check", s200], { status: 0 });
  for (const target of ["native", "wasm32"]) for (const level of ["-O0", "-O3"]) emit("s200", s200, { target, level });
  const s1000 = join(directory, "s1000");
  writeProject(s1000, 1000);
  equal("s1000 check", ["check", s1000], { counts: ["1", "8"], status: 0 });
  emit("s1000", s1000, { counts: ["1", "8"] });

  // 3. Diagnostics: the cap (1,000 unique, 50 shown, "at least N more"), the order of the sources,
  // and the errors that come from each stage. Text and --json.
  const cases = [
    ["ownership errors over the cap", corpus("moved", 1200, modules.moved), 50, /at least 950 more errors not shown/],
    ["type errors over the cap", corpus("typed", 400, modules.typed), 50, /at least 950 more errors not shown/],
    ["syntax errors over the cap", corpus("syntax", 1100, modules.syntax), 50, /at least 950 more errors not shown/],
    ["match errors over the cap", corpus("incomplete", 1100, modules.incomplete), 50, /at least 950 more errors not shown/],
    ["syntax errors among valid modules", corpus("mixed", 600, modules.mixed), 50, /150 more errors not shown/],
    ["a few errors", corpus("few", 30, modules.typed), 50, /70 more errors not shown/],
  ];
  for (const [label, project, count, omitted] of cases) {
    const text = equal(label, ["check", project], { status: 1 });
    assert.equal(shown(text), count, label);
    assert.match(text.stderr, omitted, label);
    equal(`${label} (json)`, ["check", project, "--json"], { status: 1 });
  }
  // Warnings come out in source order, whatever the order in which the workers finish; 1,440
  // reach their own cap of 1,000.
  const unused = corpus("unused", 60, modules.unused);
  const warnings = equal("warnings", ["check", unused], { status: 0 });
  assert.equal(shown(warnings), 50);
  assert.match(warnings.stderr, /at least 950 more warnings not shown/);
  equal("warnings deny", ["check", unused, "--deny-warnings"], { status: 1 });
  equal("warnings (json)", ["check", unused, "--json"], { status: 0 });

  // 4. Limits: unbounded specialization, nesting, recursive types, each among many modules.
  const around = project => { writeProject(project, 100); return project; };
  const limits = [
    ["specialization E1017", "def rec f :: 'a -> unit\nfn rec f x = f [x]\nf 1\n", "E1017"],
    ["growing specialization E1017", "def rec f :: 'a -> unit\nfn rec f x = { let y = x + x; f [y] }\n", "E1017"],
    ["recursive types E1010", "record A { b: B }\nrecord B { a: A }\n", "E1010"],
    ["call nesting E0002", `def f :: i32\nfn f = ${"f (".repeat(200)}0${")".repeat(200)}\n`, "E0002"],
    ["type nesting E0002", `def f :: ${"(".repeat(200)}i32${")".repeat(200)}\nfn f = 0\n`, "E0002"],
  ];
  for (const [label, main, code] of limits) {
    const project = around(join(directory, label.replaceAll(" ", "-")));
    writeFileSync(join(project, "Main.tz"), main);
    assert.match(equal(label, ["check", project], { status: 1 }).stderr, new RegExp(`error\\[${code}\\]`), label);
  }
  // The deepest nesting that the parser accepts, and one more level, in each shape.
  const depth = 125;
  const shapes = {
    parentheses: n => `${"(".repeat(n)}1${")".repeat(n)}`,
    sum: n => `${"1 + ".repeat(n)}1`,
    list: n => `${"[".repeat(n)}0${"]".repeat(n)}`,
    negation: n => `${"- ".repeat(n)}0`,
    conditional: n => `${"if true then 0 else ".repeat(n)}1`,
  };
  for (const [shape, text] of Object.entries(shapes)) {
    for (const [level, accepted] of [[depth, true], [depth + 1, false]]) {
      const project = around(join(directory, `deep-${shape}-${level}`));
      writeFileSync(join(project, "Main.tz"), `def deep :: i64 -> i64 = \\x ->\n    let _value = ${text(level)}\n    x\n\ndeep 1\n`);
      const result = equal(`${shape} ${level}`, ["check", project], { status: accepted ? 0 : 1 });
      if (!accepted) assert.match(result.stderr, /error\[E0002\]: (syntax nesting|expression depth) exceeds 128/);
      if (accepted) emit(`deep-${shape}`, project, { counts: ["1", "8"] });
    }
  }

  // 5. The knob. A value that is not an integer from 1 to 32 is E2000 and exit code 2 for the
  // commands that analyze sources, before they read anything; white space around a number and
  // an empty value are fine.
  for (const value of ["0", "33", "abc", "-1", "+4", "1.5", "0x8", "1e1", "٣", "4 4", " ", "99999999999999999999"]) {
    for (const command of [["check", hello], ["build", hello, "--emit", "llvm", "-o", join(directory, "never.ll")], ["test", hello], ["repl"]]) {
      const result = cli(command, { threads: value });
      assert.equal(result.status, 2, `${command[0]} ${JSON.stringify(value)}`);
      assert.match(result.stderr, /error\[E2000\]: TSUZURI_THREADS must be an integer from 1 to 32/);
      assert.equal(result.stdout, "");
    }
  }
  assert.ok(!existsSync(join(directory, "never.ll")));
  for (const value of ["", " 4 ", "\t8\n", "1", "32", "007"]) {
    const result = cli(["check", hello], { threads: value });
    assert.equal(result.status, 0, `${JSON.stringify(value)}: ${result.stderr}`);
    assert.equal(result.stderr, "");
  }
  // Commands that do not analyze sources do not read it.
  for (const command of [["--help"], ["toolchain", "info"], ["fmt", "--check", hello]]) {
    assert.equal(cli(command, { threads: "abc" }).status, 0, command.join(" "));
  }
  const help = cli(["--help"]);
  assert.match(help.stdout, /TSUZURI_THREADS/);

  // 6. The caches. The thread count is in no key: the frontend cache has the same bytes (cold and
  // warm) at every count, a warm run at another count rewrites nothing, and the build cache hits.
  const cached = ["1", "2", "8", "32"].map(threads => {
    const cache = join(directory, `cache-${threads}`);
    const cold = cli(["check", s200], { threads, cache });
    assert.equal(cold.status, 0, cold.stderr);
    const files = Object.fromEntries(readdirSync(join(cache, "frontend")).sort().map(file => [file, readFileSync(join(cache, "frontend", file))]));
    assert.equal(Object.keys(files).length, 2, "a pack and a manifest");
    assert.deepEqual(cli(["check", s200], { threads, cache }), cold);
    for (const [file, bytes] of Object.entries(files)) assert.deepEqual(readFileSync(join(cache, "frontend", file)), bytes, file);
    return { cache, files };
  });
  for (const { files } of cached.slice(1)) {
    assert.deepEqual(Object.keys(files), Object.keys(cached[0].files));
    for (const file of Object.keys(files)) assert.ok(files[file].equals(cached[0].files[file]), `frontend cache file ${file} differs`);
  }
  {
    const { cache } = cached[0];
    const stamps = readdirSync(join(cache, "frontend")).map(file => statSync(join(cache, "frontend", file)).mtimeMs);
    const warm = cli(["check", s200], { threads: "8", cache });
    assert.equal(warm.status, 0);
    assert.deepEqual(readdirSync(join(cache, "frontend")).map(file => statSync(join(cache, "frontend", file)).mtimeMs), stamps, "a warm run at another count rewrote the cache");
    // The cached run's output equals the uncached one's.
    assert.deepEqual(warm, cli(["check", s200], { threads: "8" }));
  }
  {
    const cache = join(directory, "build-cache");
    const project = join(directory, "s20");
    writeProject(project, 20);
    const build = threads => {
      const output = join(directory, `s20-${threads}`);
      const result = cli(["build", project, "-o", output], { threads, cache, extra: { TSUZURI_TIME_PASSES: "1" } });
      assert.equal(result.status, 0, result.stderr);
      const passes = JSON.parse(result.stderr.trim().split("\n").at(-1));
      return { phases: passes.phases.map(phase => phase.name), bytes: readFileSync(output) };
    };
    const first = build("1");
    assert.ok(first.phases.some(phase => phase.startsWith("tool.")), `the first build links: ${first.phases}`);
    for (const threads of ["8", "2", "32"]) {
      const again = build(threads);
      assert.ok(!again.phases.some(phase => phase.startsWith("tool.")), `a build at ${threads} threads hits the cache: ${again.phases}`);
      assert.ok(again.bytes.equals(first.bytes));
    }
  }

  // 7. The phase times stay wall-clock, measured on the main thread: they add up to at most the total.
  for (const threads of ["1", "8"]) {
    const result = cli(["check", s200], { threads, extra: { TSUZURI_TIME_PASSES: "1" } });
    assert.equal(result.status, 0);
    const passes = JSON.parse(result.stderr.trim().split("\n").at(-1));
    const known = new Set(["load", "parse", "check", "specialize", "closures", "ownership", "emit", "cache"]);
    assert.ok(passes.phases.every(phase => known.has(phase.name) || phase.name.startsWith("tool.")), JSON.stringify(passes.phases));
    const sum = passes.phases.reduce((total, phase) => total + phase.ms, 0);
    assert.ok(sum <= passes.total_ms + 0.5, `${threads} threads: phases ${sum} ms, total ${passes.total_ms} ms`);
  }

  // 8. Repeats: no run differs from the first, at more threads than cores.
  for (const [label, project, args] of [["s200", s200, ["check"]], ["moved", join(directory, "moved"), ["check", "--json"]], ["syntax", join(directory, "syntax"), ["check"]]]) {
    const first = cli([args[0], project, ...args.slice(1)], { threads: "16" });
    for (let repeat = 0; repeat < 20; repeat++) {
      assert.deepEqual(cli([args[0], project, ...args.slice(1)], { threads: "16" }), first, `${label}: repeat ${repeat}`);
    }
  }
  console.log("parallel frontend e2e: ok");
} finally {
  rmSync(directory, { recursive: true, force: true });
}
