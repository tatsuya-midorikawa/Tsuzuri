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
// What makes the symbols of the units local in an object: nmedit of Xcode, or an objcopy (the compiler builds an object from
// one module on a machine that has none).
const canLocalize = process.platform === "darwin" || [process.env.TSUZURI_OBJCOPY, "llvm-objcopy", "objcopy"].some((program) => program && available(program));

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
  // The body of a lambda: the lambda is defined in the unit of the module that uses it, so that unit alone.
  const file = join(space.project, "Module1.tz");
  const text = readFileSync(file, "utf8");
  assert.ok(text.includes("let add = \\value -> value + offset\n"));
  writeFileSync(file, text.replaceAll("let add = \\value -> value + offset\n", "let add = \\value -> value + offset + 0\n"));
  const lambda = space.build([], { output: "lambda" });
  assert.equal(space.compiled().units.length, 1, `of ${total} units for a lambda`);
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
  // An object is cut where the machine can make symbols local again; the other outputs are one module.
  const object = join(space.directory, "output.o");
  execute(compiler, ["build", space.project, "-O0", "--emit", "object", "--no-cache", "-o", object], { env: space.env });
  assert.equal(space.compiled().units.length > 3, canLocalize, "--emit object is split where symbols can be made local");
  execute(compiler, ["build", space.project, "-O3", "--emit", "object", "--no-cache", "-o", object], { env: space.env });
  assert.equal(space.compiled().units.length, 0, "--emit object -O3");
  for (const emit of ["llvm", "shared", "bitcode"]) {
    // A shared library and bitcode need an export, which this project has not.
    const exported = emit === "llvm" ? space : workspace(0, OBJECT_PROGRAMS.p1);
    execute(compiler, ["build", exported.project, "-O0", "--emit", emit, "--no-cache", "-o", join(exported.directory, `output.${emit}`)], { env: exported.env });
    assert.equal(exported.compiled().units.length, 0, `--emit ${emit}`);
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

// C7: the public symbols of the executable are the same. A symbol is public when a link or a loader can see it: the global
// symbols of a Mach-O executable (ld64 makes the hidden ones local), and the global symbols with default visibility of an ELF
// executable (GNU ld keeps the units' hidden definitions in the symbol table as GLOBAL HIDDEN, and `strip` removes them).
test("C7 public symbols", () => {
  const space = workspace();
  const split = space.build(["--no-cache"], { output: "split" });
  const single = space.build(["--no-cache"], { output: "single", env: { TSUZURI_CODEGEN_UNITS: "single" } });
  const symbols = (program, visibility = "DEFAULT") => {
    if (process.platform === "darwin") {
      const result = execute("nm", ["-gU", program]);
      return result.stdout.split("\n").filter(Boolean).map((line) => line.trim().split(/\s+/).at(-1)).sort();
    }
    return execute("readelf", ["-sW", program]).stdout.split("\n").map((line) => line.trim().split(/\s+/))
      .filter((fields) => /^\d+:$/.test(fields[0]) && fields[4] !== "LOCAL" && fields[6] !== "UND" && fields[5] === visibility && fields[7])
      .map((fields) => fields[7]).sort();
  };
  const expected = symbols(single.out);
  assert.ok(expected.length > 0);
  assert.deepEqual(symbols(split.out), expected);
  assert.ok(!symbols(split.out).some((name) => /tz\.(fn|unit|apply)/.test(name)), "no unit symbol is public");
  if (process.platform !== "darwin") {
    // What else the symbol table shows is hidden, and only what the units made global to the link.
    const before = symbols(single.out, "HIDDEN");
    const extra = symbols(split.out, "HIDDEN").filter((name) => !before.includes(name));
    assert.ok(extra.every((name) => /^tz\./.test(name)), extra.join(" "));
    assert.deepEqual(before.filter((name) => !symbols(split.out, "HIDDEN").includes(name)), []);
  }
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
        // A symbol with a name of the program: a global one (not a section or a local of the unit) that the unit defines.
        if (!/^\d+:$/.test(fields[0]) || fields[4] === "LOCAL" || fields[3] === "SECTION" || fields[6] === "UND") continue;
        if (!/^tz\./.test(fields.at(-1) ?? "") || fields.at(-1).startsWith("tz.unit")) continue;
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

// A wasm32 module with exports in three files: `tz_run` divides in `A` and in `B`, `tz_lam` uses a lambda.
const WASM_FILES = {
  "Main.tz": `export def run :: i64 -> i64 -> i64
fn run a b = A.add_div a b + B.mul_div a b

export def lam :: i64 -> i64
fn lam n =
    let offset = n % 7
    let add = \\v -> v + offset
    add (add n)
`,
  "A.tz": "def add_div :: i64 -> i64 -> i64\nfn add_div a b = (a + 1) / b\n",
  "B.tz": "def mul_div :: i64 -> i64 -> i64\nfn mul_div a b = (a * 2) / b\n",
};
const wasmLd = process.env.TSUZURI_WASM_LD ?? "wasm-ld";
const instantiate = (path) => {
  const module = new WebAssembly.Module(readFileSync(path));
  assert.deepEqual(WebAssembly.Module.imports(module), []);
  return new WebAssembly.Instance(module).exports;
};

// C11: a wasm32 module at -O0 is built from units, linked by wasm-ld with the runtime objects, and behaves like the module
// of a single unit, down to the site that a trap reports.
test("C11 wasm32", () => {
  if (!available(wasmLd)) return skip("wasm32 builds", wasmLd);
  const space = workspace(0, WASM_FILES);
  const build = (name, args = [], env = {}) => space.build(["--target", "wasm32", ...args], { output: `${name}.wasm`, env });
  const check = (path, traps) => {
    const exports = instantiate(path);
    assert.equal(exports.tz_run(7n, 2n), 11n);
    assert.equal(exports.tz_lam(10n), 16n);
    assert.throws(() => exports.tz_run(7n, 0n), WebAssembly.RuntimeError);
    if (!traps) return;
    const sites = JSON.parse(readFileSync(`${path}.trap.json`, "utf8")).sites;
    const site = sites.find((candidate) => candidate.id === exports.tsuzuri_trap_site());
    assert.equal(site?.kind, "integer division by zero");
    assert.match(site.path, /A\.tz$/);
  };
  for (const args of [[], ["--no-cache"], ["--trap-info"], ["--trap-info", "--no-cache"], ["--wasm-feature", "simd128"], ["-g"]]) {
    const split = build("split", args);
    assert.ok(space.compiled().units.length > 3, `${args.join(" ")} is split`);
    const single = build("single", args, { TSUZURI_CODEGEN_UNITS: "single" });
    assert.equal(space.compiled().units.length, 0);
    for (const built of [split, single]) check(built.out, args.includes("--trap-info"));
    if (args.includes("--trap-info")) {
      const table = (out) => JSON.parse(readFileSync(`${out}.trap.json`, "utf8")).sites;
      const key = (site) => `${site.path}|${site.span.start}|${site.span.end}|${site.kind}`;
      assert.deepEqual(table(split.out).map(key).sort(), table(single.out).map(key).sort());
    }
    // The same text gives the same module.
    const again = build("again", args);
    assert.deepEqual(readFileSync(again.out), readFileSync(split.out), args.join(" "));
  }
  // An edit of one file: that file's unit alone is compiled again, and wasm-ld links the module.
  const cached = workspace(0, WASM_FILES);
  const cold = cached.build(["--target", "wasm32", "--trap-info"], { output: "cold.wasm" });
  assert.ok(cached.compiled().units.length > 3);
  writeFileSync(join(cached.project, "B.tz"), `def twice :: i64 -> i64\nfn twice x = x + x\n\n${WASM_FILES["B.tz"]}`);
  const edited = cached.build(["--target", "wasm32", "--trap-info"], { output: "edited.wasm" });
  assert.equal(cached.compiled().units.length, 1);
  assert.equal(instantiate(edited.out).tz_run(7n, 2n), 11n);
  assert.ok(readFileSync(cold.out).length > 0);
});

// Two programs that each have a `Main.helper` and a `Mod.bump` that no host may see, and one export.
const OBJECT_PROGRAMS = {
  p1: { "Main.tz": "def helper :: i64 -> i64\nfn helper x = x + Mod.bump x\n\nexport def f1 :: i64 -> i64\nfn f1 x = helper x * 2\n", "Mod.tz": "def bump :: i64 -> i64\nfn bump x = x + 1\n" },
  p2: { "Main.tz": "def helper :: i64 -> i64\nfn helper x = x + Mod.bump x\n\nexport def f2 :: i64 -> i64\nfn f2 x = helper x * 3\n", "Mod.tz": "def bump :: i64 -> i64\nfn bump x = x + 2\n" },
};
// What an object makes visible to a link: its global symbols with their binding and visibility, without addresses.
function linkSymbols(object) {
  if (process.platform === "darwin") {
    return execute("nm", ["-m", object]).stdout.split("\n").filter((line) => line && !/ non-external /.test(line))
      .map((line) => line.replace(/^[0-9a-f]+ /, "")).sort();
  }
  return execute("readelf", ["-sW", object]).stdout.split("\n").map((line) => line.trim().split(/\s+/))
    .filter((fields) => /^\d+:$/.test(fields[0]) && fields[4] !== "LOCAL" && fields[7])
    .map((fields) => `${fields[4]} ${fields[5]} ${fields[6] === "UND" ? "UND" : "DEF"} ${fields[7]}`).sort();
}

// C12: a native object is made of units that a relocatable link joins; what the host sees is what the single module shows,
// and the names that were internal do not clash when two objects of two programs are linked into one program.
test("C12 native object", () => {
  if (!canLocalize) return skip("object builds", "objcopy");
  const programs = {};
  for (const [name, files] of Object.entries(OBJECT_PROGRAMS)) {
    const space = workspace(0, files);
    const build = (output, args = [], env = {}) => space.build(["--emit", "object", ...args], { output, env });
    const split = build(`${name}-split.o`, ["--no-cache"]);
    assert.ok(space.compiled().units.length > 2, `${name}: an object is split`);
    const single = build(`${name}-single.o`, ["--no-cache"], { TSUZURI_CODEGEN_UNITS: "single" });
    assert.equal(space.compiled().units.length, 0);
    assert.deepEqual(linkSymbols(split.out), linkSymbols(single.out), `${name}: the symbols the host sees`);
    for (const symbol of linkSymbols(split.out)) assert.ok(!/tz\.(fn|unit|apply)/.test(symbol), symbol);
    sameBytes(split.out, build(`${name}-again.o`, ["--no-cache"]).out);
    programs[name] = { space, split, single };
  }
  // Both objects define `tz.fn.Main.helper`: the units made it global to the link, and the object makes it local again.
  const host = join(root, "objects-host.c");
  writeFileSync(host, "#include <stdint.h>\nextern int64_t tz_f1(int64_t);\nextern int64_t tz_f2(int64_t);\nint main(void) { return tz_f1(1) == 6 && tz_f2(1) == 12 ? 0 : 1; }\n");
  for (const mode of ["split", "single"]) {
    const program = join(root, `objects-host-${mode}`);
    execute(realClang, [host, programs.p1[mode].out, programs.p2[mode].out, "-o", program]);
    execute(program, []);
  }
  // An edit of one file changes one unit, and the object still holds the same symbols.
  const { space } = programs.p1;
  writeFileSync(join(space.project, "Mod.tz"), "def bump :: i64 -> i64\nfn bump x = x + 3\n");
  space.build(["--emit", "object"], { output: "p1-edit.o" });
  space.compiled();
  writeFileSync(join(space.project, "Mod.tz"), "def bump :: i64 -> i64\nfn bump x = x + 4\n");
  const edited = space.build(["--emit", "object"], { output: "p1-edited.o" });
  assert.equal(space.compiled().units.length, 1);
  assert.deepEqual(linkSymbols(edited.out), linkSymbols(programs.p1.split.out));
});

// With the C runtime (tasks) merged in, and with trap information: the host sees the same, and runs the program.
test("C12 native object with the runtime", () => {
  if (!canLocalize) return skip("object builds", "objcopy");
  const fixture = resolve("tests/fixtures/debug_info");
  const files = { "Main.tz": readFileSync(join(fixture, "Main.tz"), "utf8") };
  const space = workspace(0, files);
  for (const args of [[], ["--trap-info"], ["--no-cache"]]) {
    const split = space.build(["--emit", "object", ...args], { output: "tasks-split.o" });
    assert.ok(space.compiled().units.length > 2, `${args.join(" ")} is split`);
    const single = space.build(["--emit", "object", ...args], { output: "tasks-single.o", env: { TSUZURI_CODEGEN_UNITS: "single" } });
    assert.deepEqual(linkSymbols(split.out), linkSymbols(single.out), args.join(" "));
    const host = join(root, "tasks-host.c");
    writeFileSync(host, "#include <stdint.h>\nextern int64_t tz_answer(int64_t);\nextern int64_t tz_parallel(void);\nint main(void) { return tz_answer(40) != 42 || tz_parallel() != 42; }\n");
    for (const built of [split, single]) {
      const program = join(root, "tasks-host");
      execute(realClang, [host, built.out, "-pthread", "-lm", "-o", program]);
      execute(program, []);
    }
  }
});

// C13: the programs of examples/ behave the same from units and from one module: output, error output and exit status.
test("C13 examples", () => {
  const examples = resolve("examples");
  const out = join(root, "examples");
  mkdirSync(out);
  const space = workspace(0, { "Main.tz": "def main :: unit -> i32 = \\() -> 0\n" });
  let compared = 0;
  for (const name of readdirSync(examples).sort()) {
    const project = join(examples, name);
    if (!existsSync(join(project, "Main.tz"))) continue;
    const results = ["split", "single"].map((mode) => {
      const program = join(out, `${name}-${mode}`);
      const built = spawnSync(compiler, ["build", project, "-O0", "--no-cache", "-o", program], { encoding: "utf8", timeout: 300_000, env: { ...process.env, ...space.env, TSUZURI_CODEGEN_UNITS: mode } });
      assert.ifError(built.error);
      if (built.status !== 0) return { built: built.status, error: /error\[(E\d+)\]/.exec(built.stderr)?.[1] };
      const ran = spawnSync(program, [], { encoding: "utf8", timeout: 60_000, input: "" });
      return { built: 0, stdout: ran.stdout, stderr: ran.stderr, status: ran.status, signal: ran.signal };
    });
    assert.deepEqual(results[0], results[1], `examples/${name}`);
    if (results[0].built === 0) compared++;
  }
  assert.ok(compared >= 6, `${compared} examples built and ran`);
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
