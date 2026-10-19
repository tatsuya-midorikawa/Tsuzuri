// PB07: a native -O0 executable is built from codegen units. The IR that the emitter makes is cut into units (one per
// module, and 16 buckets for the rest), the objects of the units are cached by content under <cache>/units, the units that
// the cache misses are compiled by a few parallel Clang processes, and the objects are linked once.
//
// A wrapper around TSUZURI_CLANG logs the unit files (u00012.ll) that reach Clang, so the checks count what was really
// compiled. The synthetic project of benchmarks/build/generate.mjs and its independent BigInt reference give the values.
import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { expectedOutput, moduleFiles, writeProject } from "../benchmarks/build/generate.mjs";
import { artifactFingerprint } from "../benchmarks/metrics.mjs";

const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const windows = process.platform === "win32";
if (windows) {
  // Windows builds are never cut into units: the units rely on hidden symbols and a POSIX link.
  console.log("codegen units: skipped on Windows (a Windows build is always one unit)");
  process.exit(0);
}
const root = mkdtempSync(join(tmpdir(), "tsuzuri-units-"));
const realClang = spawnSync("which", [process.env.TSUZURI_CLANG ?? "clang"], { encoding: "utf8" }).stdout.trim() || "clang";
const MODULES = 8;
const cases = [];
const test = (name, body) => cases.push([name, body]);
// The checks that need an LLVM tool skip where it is missing, unless a CI toolchain (LLVM_PREFIX) turns a skip into a failure.
const strict = process.env.TSUZURI_UNITS_STRICT === "1" || Boolean(process.env.LLVM_PREFIX);
const dwarfdump = process.env.TSUZURI_DWARFDUMP ?? "llvm-dwarfdump";
const available = (program) => spawnSync(program, ["--version"], { stdio: "ignore" }).status === 0;
function skip(what, program) {
  assert.ok(!strict, `${what}: ${program} is required`);
  console.log(`codegen units: skipping ${what} (no ${program})`);
}

function execute(program, args, { env = {}, input = "", success = true, cwd } = {}) {
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 300_000, env: { ...process.env, ...env }, input, cwd, maxBuffer: 64 * 1024 * 1024 });
  assert.ifError(result.error);
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}

let sequence = 0;
// A fresh directory with a project of MODULES modules (or the given files), a cache, and a Clang wrapper that logs the
// units it compiles.
function workspace(modules = MODULES, files = null) {
  const directory = join(root, `case-${++sequence}`);
  mkdirSync(directory);
  const project = join(directory, "project");
  if (files) {
    mkdirSync(project);
    for (const [name, text] of Object.entries(files)) writeFileSync(join(project, name), text);
  } else {
    writeProject(project, modules);
  }
  const log = join(directory, "units.log");
  const wrapper = join(directory, "clang-wrapper.mjs");
  writeFileSync(wrapper, `#!${process.execPath}
import { appendFileSync } from "node:fs";
import { basename } from "node:path";
import { spawnSync } from "node:child_process";
const args = process.argv.slice(2);
for (const argument of args) if (/\\.ll$/.test(argument)) appendFileSync(${JSON.stringify(log)}, basename(argument) + "\\n");
const result = spawnSync(${JSON.stringify(realClang)}, args, { stdio: "inherit" });
process.exitCode = result.status ?? 1;
`);
  chmodSync(wrapper, 0o755);
  const cache = join(directory, "cache");
  // The tests cut every build, however small; `auto` is tried by its own cases.
  const env = { TSUZURI_CACHE_DIR: cache, TSUZURI_CLANG: wrapper, TSUZURI_CODEGEN_UNITS: "split" };
  const space = {
    directory, project, cache, env, log,
    // The unit files that Clang has compiled since the last call, and everything else it was given an .ll for.
    compiled() {
      const lines = existsSync(log) ? readFileSync(log, "utf8").split("\n").filter(Boolean) : [];
      writeFileSync(log, "");
      return { units: lines.filter((line) => /^u\d{5}\.ll$/.test(line)), others: lines.filter((line) => !/^u\d{5}\.ll$/.test(line)) };
    },
    build(args = [], { env: extra = {}, output = "program", success = true } = {}) {
      const out = join(directory, output);
      const result = execute(compiler, ["build", project, "-O0", ...args, "-o", out], { env: { ...env, ...extra }, success });
      return { result, out };
    },
    unitEntries() {
      const units = join(cache, "units");
      return existsSync(units) ? readdirSync(units).filter((name) => /^[0-9a-f]{64}$/.test(name)).map((name) => join(units, name)) : [];
    },
  };
  space.compiled();
  return space;
}

const run = (program, line) => execute(program, [], { input: `${line}\n` });
// Apple's linker writes a different UUID and signature into every executable, a single module included; every other byte is compared.
const sameBytes = (left, right) => assert.equal(artifactFingerprint(left), artifactFingerprint(right), `${left} and ${right} differ`);

// C1: the units and the single module give the same program, which gives the reference values.
test("C1 differential", () => {
  const space = workspace();
  const split = space.build(["--no-cache"], { output: "split" });
  const single = space.build(["--no-cache"], { output: "single", env: { TSUZURI_CODEGEN_UNITS: "single" } });
  const { units } = space.compiled();
  assert.ok(units.length > MODULES, `a build of ${MODULES} modules has more than ${MODULES} units: ${units.length}`);
  for (const line of ["12345", "", "綴り😀", "x".repeat(40)]) {
    const reference = expectedOutput(MODULES, line);
    for (const out of [split.out, single.out]) {
      const result = execute(out, [], { input: `${line}\n` });
      assert.equal(result.stdout, reference, `${out} ${JSON.stringify(line)}`);
    }
  }
});

// C2: two builds from units are the same bytes.
test("C2 determinism", () => {
  const space = workspace();
  const first = space.build(["--no-cache"], { output: "first" });
  const second = space.build(["--no-cache"], { output: "second" });
  sameBytes(first.out, second.out);
});

// C3: a build that takes most objects from the cache is the build that compiles everything.
test("C3 assembly from the cache", () => {
  const space = workspace();
  space.build([], { output: "cold" });
  const files = moduleFiles(3, { salt: 5 });
  for (const [name, text] of files) writeFileSync(join(space.project, name), text);
  const cached = space.build([], { output: "cached" });
  const fresh = space.build(["--no-cache"], { output: "fresh" });
  sameBytes(cached.out, fresh.out);
  assert.equal(run(cached.out, "12345").stdout, expectedOutput(MODULES, "12345", { index: 3, salt: 5 }));
});

// C4: only what an edit changes is compiled again.
test("C4 recompiled units", () => {
  const space = workspace();
  space.build([], { output: "cold" });
  const total = space.compiled().units.length;
  assert.ok(total > MODULES);
  // No change: the whole-build cache answers and no tool runs.
  space.build([], { output: "again" });
  assert.deepEqual(space.compiled(), { units: [], others: [] });
  // A different link and the same objects: the units come from the cache.
  space.build(["--strip"], { output: "stripped" });
  assert.deepEqual(space.compiled(), { units: [], others: [] });
  // The body of one function of one module: that module's unit alone.
  for (const [name, text] of moduleFiles(4, { salt: 1 })) writeFileSync(join(space.project, name), text);
  const edited = space.build([], { output: "edited" });
  assert.equal(space.compiled().units.length, 1);
  assert.equal(run(edited.out, "12345").stdout, expectedOutput(MODULES, "12345", { index: 4, salt: 1 }));
  // The body of a lambda: its module, and what the compiler names after it, but nowhere near every unit.
  const file = join(space.project, "Module1.tz");
  const text = readFileSync(file, "utf8");
  assert.ok(text.includes("let add = \\value -> value + offset\n"));
  writeFileSync(file, text.replaceAll("let add = \\value -> value + offset\n", "let add = \\value -> value + offset + 0\n"));
  const lambda = space.build([], { output: "lambda" });
  const { units } = space.compiled();
  assert.ok(units.length >= 1 && units.length * 2 < total, `${units.length} of ${total} units for a lambda`);
  assert.equal(run(lambda.out, "12345").stdout, expectedOutput(MODULES, "12345", { index: 4, salt: 1 }));
});

// C5: a damaged entry of the unit cache is compiled again.
test("C5 damaged entry", () => {
  const space = workspace();
  space.build([], { output: "cold" });
  const total = space.compiled().units.length;
  const entries = space.unitEntries();
  assert.equal(entries.length, total, "one entry per unit");
  writeFileSync(join(entries[0], "artifact"), "corrupt");
  const rebuilt = space.build(["--strip"], { output: "rebuilt" });
  assert.equal(space.compiled().units.length, 1);
  assert.equal(run(rebuilt.out, "12345").stdout, expectedOutput(MODULES, "12345"));
  // The entry is whole again: with the whole-build entries gone, every unit comes from the cache.
  for (const name of readdirSync(space.cache)) if (/^[0-9a-f]{64}$/.test(name)) rmSync(join(space.cache, name), { recursive: true });
  const more = space.build([], { output: "again" });
  assert.deepEqual(space.compiled(), { units: [], others: [] });
  assert.equal(run(more.out, "12345").stdout, expectedOutput(MODULES, "12345"));
});

// C6: which builds are cut into units, and which options keep working.
test("C6 eligibility", () => {
  const space = workspace(3);
  const units = (args, env = {}) => {
    const { out } = space.build(["--no-cache", ...args], { env, output: "program" });
    const count = space.compiled().units.length;
    assert.equal(run(out, "12345").stdout, expectedOutput(3, "12345"), args.join(" "));
    return count;
  };
  assert.ok(units([]) > 3, "-O0 is split");
  assert.ok(units(["--strip"]) > 3, "--strip is split");
  assert.ok(units(["--cpu", "native"]) > 3, "--cpu native is split");
  assert.ok(units(["--allocator", "small"]) > 3, "--allocator small is split");
  assert.ok(units(["--trap-info"]) > 3, "--trap-info is split");
  assert.ok(units(["-g"]) > 3, "-g is split");
  assert.equal(units([], { TSUZURI_CODEGEN_UNITS: "single" }), 0, "TSUZURI_CODEGEN_UNITS=single");
  assert.equal(units([], { TSUZURI_CODEGEN_UNITS: "auto" }), 0, "a small program stays one unit under auto");
  assert.equal(units([], { TSUZURI_CODEGEN_UNITS: "" }), 0, "an empty TSUZURI_CODEGEN_UNITS is auto");
  assert.ok(units([], { TSUZURI_CODEGEN_UNITS: "split" }) > 3, "TSUZURI_CODEGEN_UNITS=split");
  for (const optimization of ["-O1", "-O2", "-O3"]) {
    const out = join(space.directory, "optimized");
    execute(compiler, ["build", space.project, optimization, "--no-cache", "-o", out], { env: space.env });
    assert.equal(space.compiled().units.length, 0, optimization);
    assert.equal(run(out, "12345").stdout, expectedOutput(3, "12345"));
  }
  for (const emit of ["object", "llvm"]) {
    execute(compiler, ["build", space.project, "-O0", "--emit", emit, "--no-cache", "-o", join(space.directory, `output.${emit}`)], { env: space.env });
    assert.equal(space.compiled().units.length, 0, `--emit ${emit}`);
  }
});

test("C6 invalid setting", () => {
  const space = workspace(2);
  for (const value of ["x", "Single", "1"]) {
    const result = space.build([], { env: { TSUZURI_CODEGEN_UNITS: value }, success: false }).result;
    assert.equal(result.status, 1, value);
    assert.match(result.stderr, /E2000/);
    assert.match(result.stderr, new RegExp(`invalid TSUZURI_CODEGEN_UNITS value '${value}'; use 'auto', 'single' or 'split'`));
  }
  assert.equal(space.build([], { env: { TSUZURI_CODEGEN_UNITS: "" } }).result.status, 0);
});

// `auto` cuts a build only when its IR is large enough for the units to pay: below 4 MiB one unit is faster (a Clang job
// costs about 8 ms of CPU time per unit), above it the parallel compile wins.
test("C6 auto threshold", () => {
  const small = workspace(3);
  small.build(["--no-cache"], { env: { TSUZURI_CODEGEN_UNITS: "auto" } });
  assert.equal(small.compiled().units.length, 0);
  const large = workspace(110);
  const { out } = large.build(["--no-cache"], { env: { TSUZURI_CODEGEN_UNITS: "auto" } });
  assert.ok(large.compiled().units.length > 110, "a large program is cut under auto");
  assert.equal(run(out, "12345").stdout, expectedOutput(110, "12345"));
});

// C7: the public symbols of the executable are the same.
test("C7 public symbols", () => {
  const space = workspace();
  const split = space.build(["--no-cache"], { output: "split" });
  const single = space.build(["--no-cache"], { output: "single", env: { TSUZURI_CODEGEN_UNITS: "single" } });
  const symbols = (program) => {
    const args = process.platform === "darwin" ? ["-gU", program] : ["-g", "--defined-only", program];
    const result = execute("nm", args);
    return result.stdout.split("\n").filter(Boolean).map((line) => line.trim().split(/\s+/).at(-1)).sort();
  };
  const expected = symbols(single.out);
  assert.ok(expected.length > 0);
  assert.deepEqual(symbols(split.out), expected);
  assert.ok(!symbols(split.out).some((name) => /tz\.(fn|unit|apply)/.test(name)), "no unit symbol is public");
});

// The objects of the units: a function is private to the program, a constant is local to its unit.
test("C7 object symbols", () => {
  const space = workspace(3);
  space.build([], { output: "program" });
  const entries = space.unitEntries();
  assert.ok(entries.length > 3);
  let functions = 0;
  for (const entry of entries) {
    const object = join(entry, "artifact");
    if (process.platform === "darwin") {
      const lines = execute("nm", ["-m", object]).stdout.split("\n").filter(Boolean);
      for (const line of lines) {
        const match = /(\S.*?) (_tz\.\S+)$/.exec(line);
        // An undefined symbol carries no visibility in a Mach-O object; the definition says whether it is hidden.
        if (!match || /\(undefined\)/.test(line)) continue;
        assert.ok(/private external|non-external/.test(match[1]), `${line} in ${object}`);
        if (/_tz\.fn\.Module/.test(line)) { functions++; assert.match(line, /private external/); }
      }
      assert.ok(!lines.some((line) => /\bweak external\b.*_tz\./.test(line)), `${object}`);
    } else {
      const lines = execute("readelf", ["-sW", object]).stdout.split("\n");
      for (const line of lines) {
        const fields = line.trim().split(/\s+/);
        if (!/tz\./.test(fields.at(-1) ?? "") || fields.at(-1)?.startsWith("tz.unit")) continue;
        if (fields[6] === "UND") continue;
        assert.equal(fields[5], "HIDDEN", `${line} in ${object}`);
        functions++;
      }
    }
  }
  assert.ok(functions > 0);
});

// C8: builds that run at the same time on one cache all succeed and make the same program.
test("C8 concurrent builds", async () => {
  const space = workspace();
  const builds = Array.from({ length: 4 }, (_, index) => new Promise((done, fail) => {
    const out = join(space.directory, `race-${index}`);
    const child = spawn(compiler, ["build", space.project, "-O0", "-o", out], { env: { ...process.env, ...space.env }, stdio: ["ignore", "pipe", "pipe"] });
    let stderr = "";
    child.stderr.on("data", (chunk) => { stderr += chunk; });
    child.on("error", fail);
    child.on("exit", (code) => (code === 0 ? done(out) : fail(new Error(stderr))));
  }));
  const outputs = await Promise.all(builds);
  for (const out of outputs.slice(1)) sameBytes(outputs[0], out);
  assert.equal(run(outputs[0], "12345").stdout, expectedOutput(MODULES, "12345"));
});

// A program that traps in the module that its input names: a line of N characters makes `A.divide` divide by N,
// `B.divide` by N - 1 and `C.divide` by N - 2.
const TRAP_FILES = {
  "Main.tz": `def main :: unit -> i32 = \\() ->
    let! line = IO.read_line ()
    let text = Maybe.default_value "" line
    let n = String.length text as i64
    do! IO.write_line (to_string (A.divide 100 n))
    do! IO.write_line (to_string (B.divide 100 (n - 1)))
    do! IO.write_line (to_string (C.divide 100 (n - 2)))
    0
`,
  "A.tz": "def divide :: i64 -> i64 -> i64 = \\a b -> a / b\n",
  "B.tz": "def divide :: i64 -> i64 -> i64 = \\a b -> a / b\n",
  "C.tz": "def divide :: i64 -> i64 -> i64 = \\a b -> a / b\n",
};
const outcome = (program, line) => {
  const result = spawnSync(program, [], { encoding: "utf8", input: `${line}\n` });
  assert.ifError(result.error);
  return { stdout: result.stdout, stderr: result.stderr, status: result.status, signal: result.signal };
};
const trapMessage = (module, line = 1, column = 43) => new RegExp(`^trap: integer division by zero at .*${module}\\.tz:${line}:${column}$`, "m");

// C9: `--trap-info` is built from units too, with the same behavior and the same sites, and the numbers of the sites of
// one file do not change when another file is edited.
test("C9 trap information", () => {
  const space = workspace(0, TRAP_FILES);
  const split = space.build(["--trap-info", "--no-cache"], { output: "split" });
  assert.ok(space.compiled().units.length > 3, "--trap-info is split");
  const single = space.build(["--trap-info", "--no-cache"], { output: "single", env: { TSUZURI_CODEGEN_UNITS: "single" } });
  assert.equal(space.compiled().units.length, 0);
  for (const [line, module] of [["", "A"], ["x", "B"], ["xy", "C"], ["xyz", null]]) {
    const left = outcome(split.out, line);
    assert.deepEqual(left, outcome(single.out, line), JSON.stringify(line));
    if (module) {
      assert.match(left.stderr, trapMessage(module));
      assert.equal(left.signal === null, false, "a trap ends the program with a signal");
    } else {
      assert.equal(left.status, 0);
    }
  }
  // The side tables name the same sites; only their numbers differ.
  const table = (out) => JSON.parse(readFileSync(`${out}.trap.json`, "utf8")).sites;
  const key = (site) => `${site.path}|${site.span.start}|${site.span.end}|${site.kind}`;
  assert.deepEqual(table(split.out).map(key).sort(), table(single.out).map(key).sort());
  assert.equal(new Set(table(split.out).map((site) => site.id)).size, table(split.out).length);
  const repeated = space.build(["--trap-info", "--no-cache"], { output: "repeated" });
  sameBytes(split.out, repeated.out);
  assert.deepEqual(readFileSync(`${repeated.out}.trap.json`), readFileSync(`${split.out}.trap.json`));
});

test("C9 trap sites keep their numbers across edits", () => {
  const space = workspace(0, TRAP_FILES);
  space.build(["--trap-info"], { output: "cold" });
  assert.ok(space.compiled().units.length > 3);
  // A function above `divide` moves every site of B: B's unit alone is compiled again.
  writeFileSync(join(space.project, "B.tz"), `def twice :: i64 -> i64 = \\x -> x + x\n\n${TRAP_FILES["B.tz"]}`);
  const edited = space.build(["--trap-info"], { output: "edited" });
  assert.equal(space.compiled().units.length, 1);
  assert.match(outcome(edited.out, "x").stderr, trapMessage("B", 3));
  assert.match(outcome(edited.out, "").stderr, trapMessage("A"));
  // The same for the file that calls the others.
  writeFileSync(join(space.project, "Main.tz"), `\n${TRAP_FILES["Main.tz"]}`);
  const moved = space.build(["--trap-info"], { output: "moved" });
  assert.equal(space.compiled().units.length, 1);
  assert.match(outcome(moved.out, "xy").stderr, trapMessage("C"));
  // What the cache gave is what a build of everything gives.
  const fresh = space.build(["--trap-info", "--no-cache"], { output: "fresh" });
  sameBytes(moved.out, fresh.out);
});

test("C9 run -O0", () => {
  const space = workspace(0, TRAP_FILES);
  const run = (line, env = {}) => execute(compiler, ["run", space.project, "-O0"], { env: { ...space.env, ...env }, input: `${line}\n`, success: false });
  const split = run("xyz");
  assert.equal(split.status, 0, split.stderr);
  assert.equal(split.stdout, "33\n50\n100\n");
  assert.ok(space.compiled().units.length > 3, "run -O0 is split");
  const trapped = run("xy");
  assert.equal(trapped.status, 1);
  assert.match(trapped.stderr, trapMessage("C"));
  const reference = run("xy", { TSUZURI_CODEGEN_UNITS: "single" });
  assert.deepEqual({ status: trapped.status, stdout: trapped.stdout, stderr: trapped.stderr }, { status: reference.status, stdout: reference.stdout, stderr: reference.stderr });
});

// C10: `-g` is built from units too: every unit has the debug information of its functions, which together are the
// functions of the single module, and an edit changes one unit.
test("C10 debug information", () => {
  const space = workspace();
  const split = space.build(["-g", "--no-cache"], { output: "split" });
  assert.ok(space.compiled().units.length > MODULES, "-g is split");
  const single = space.build(["-g", "--no-cache"], { output: "single", env: { TSUZURI_CODEGEN_UNITS: "single" } });
  for (const out of [split.out, single.out]) assert.equal(run(out, "12345").stdout, expectedOutput(MODULES, "12345"));
  // macOS keeps the DWARF in the file beside the executable, Linux in the executable.
  const symbols = (out) => (process.platform === "darwin" ? `${out}.dwarf` : out);
  if (available(dwarfdump)) {
    const functions = (out) => {
      const info = execute(dwarfdump, ["--debug-info", symbols(out)]).stdout;
      return [...new Set(info.split("DW_TAG_subprogram").slice(1).map((part) => /DW_AT_name\s+\("([^"]*)"\)/.exec(part)?.[1]).filter(Boolean))].sort();
    };
    for (const out of [split.out, single.out]) execute(dwarfdump, ["--verify", symbols(out)]);
    const names = functions(single.out);
    assert.ok(names.some((name) => name.startsWith("Module0.")) && names.includes("Main.main"), names.join(" "));
    assert.deepEqual(functions(split.out), names);
  } else {
    skip("DWARF verification", dwarfdump);
  }
  // The same debug information from the same text. (The UUID that Apple's linker gives an executable follows the objects'
  // paths and times, so the bytes of a macOS debug build differ from build to build, a single module included.)
  const again = space.build(["-g", "--no-cache"], { output: "again" });
  if (available(dwarfdump)) {
    // The first line of a dump names the file.
    const dump = (out) => execute(dwarfdump, ["--debug-info", "--debug-line", symbols(out)]).stdout.split("\n").slice(1).join("\n");
    assert.equal(dump(again.out), dump(split.out));
  } else if (process.platform !== "darwin") {
    sameBytes(split.out, again.out);
  }
  // An edit of one module changes the unit of that module, with its debug information, and nothing else.
  const edited = workspace();
  edited.build(["-g"], { output: "cold" });
  edited.compiled();
  for (const [name, text] of moduleFiles(2, { salt: 1 })) writeFileSync(join(edited.project, name), text);
  const rebuilt = edited.build(["-g"], { output: "edited" });
  assert.equal(edited.compiled().units.length, 1);
  assert.equal(run(rebuilt.out, "12345").stdout, expectedOutput(MODULES, "12345", { index: 2, salt: 1 }));
});

// The phases of TSUZURI_TIME_PASSES: the cut, the units, and the wait for Clang, which do not overlap.
test("time passes", () => {
  const space = workspace(3);
  const report = (env) => {
    const { result } = space.build(["--no-cache"], { env: { TSUZURI_TIME_PASSES: "1", ...env } });
    const line = result.stderr.trim().split("\n").at(-1);
    return JSON.parse(line);
  };
  const split = report({});
  const names = split.phases.map((phase) => phase.name);
  assert.ok(names.includes("split") && names.includes("units"), names.join(","));
  assert.ok(names.some((name) => name.startsWith("tool.")));
  const sum = split.phases.reduce((total, phase) => total + phase.ms, 0);
  assert.ok(sum <= split.total_ms + 1, `phases ${sum} ms exceed the total ${split.total_ms} ms`);
  const single = report({ TSUZURI_CODEGEN_UNITS: "single" });
  assert.ok(!single.phases.some((phase) => phase.name === "split" || phase.name === "units"));
});

// The cache: units are in their own directory, which leaves the whole-build entries where they were.
test("cache layout", () => {
  const space = workspace(3);
  space.build([], { output: "program" });
  const root = readdirSync(space.cache);
  const whole = root.filter((name) => /^[0-9a-f]{64}$/.test(name));
  assert.equal(whole.length, 1, "one whole-build entry");
  assert.ok(root.includes("units"));
  const entry = readdirSync(join(space.cache, "units", space.unitEntries()[0].split("/").at(-1))).sort();
  assert.deepEqual(entry, ["artifact", "metadata.json"]);
});

let passed = 0;
try {
  for (const [name, body] of cases) {
    try {
      await body();
      passed++;
    } catch (error) {
      console.error(`FAILED ${name}`);
      throw error;
    }
  }
  console.log(`codegen units: ${passed} cases passed`);
} finally {
  rmSync(root, { recursive: true, force: true });
}
