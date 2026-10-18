import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

// PM02: LLDB shows a function value or a `Task` by its shape: a `descriptor` that points to a
// structure whose first member `code` is a function pointer, and an `environment`. This asks the
// formatter about the types of a `-g` executable without starting it, so it needs no debugger session
// that can run a process. A record that names its fields alike is not a function value.
const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const formatter = resolve("scripts/lldb/tsuzuri_lldb.py");
const lldb = process.env.TSUZURI_LLDB ?? "lldb";
const root = mkdtempSync(join(tmpdir(), "tsuzuri-debugger-static-"));

function execute(program, args) {
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 120_000, maxBuffer: 16 * 1024 * 1024 });
  if (program === lldb && result.error?.code === "ENOENT") throw new Error("lldb not found; install LLDB or set TSUZURI_LLDB");
  assert.ifError(result.error);
  assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result.stdout + result.stderr;
}

try {
  const project = join(root, "project");
  mkdirSync(project);
  writeFileSync(join(project, "Main.tz"), [
    "record Entries { code: i64, clone: i64, drop: i64 }",
    "record Shaped {r} { descriptor: ref {r} Entries, environment: i64 }",
    "record Plain { descriptor: i64, environment: i64 }",
    "record Old { code: i64, environment: i64, clone: i64, drop: i64 }",
    "",
    "def inspect :: i64 -> i64",
    "fn inspect seed =",
    "    let entries = Entries { code: 7, clone: 8, drop: 9 }",
    "    let shaped = Shaped { descriptor: ref entries, environment: 2 }",
    "    let plain = Plain { descriptor: 5, environment: 6 }",
    "    let old = Old { code: 1, environment: 2, clone: 3, drop: 4 }",
    "    let add = \\offset -> seed + offset",
    "    let job = task { return seed }",
    "    shaped.environment + plain.descriptor + old.code + add 1 + Task.run job - seed - 14",
    "",
    "inspect 5",
    "",
  ].join("\n"));
  const app = join(root, "app");
  execute(compiler, ["build", project, "-g", "-O0", "-o", app]);
  const probe = join(root, "probe.py");
  writeFileSync(probe, [
    "import lldb",
    "import tsuzuri_lldb",
    "def find(name):",
    "    return lldb.target.FindTypes(name).GetTypeAtIndex(0)",
    "for name in ['i64 -> i64', 'Task<i64>', 'Main.Plain', 'Main.Old', 'Main.Shaped', 'Main.Entries']:",
    "    print('shape', name, tsuzuri_lldb.is_function(find(name), None), tsuzuri_lldb.field_names(find(name)))",
    // A function value whose descriptor is null, and a task: no memory is read.
    "def blank(name):",
    "    data = lldb.SBData.CreateDataFromUInt64Array(lldb.eByteOrderLittle, 8, [0, 0])",
    "    return lldb.target.CreateValueFromData('probe', data, find(name))",
    "print('summary', tsuzuri_lldb.function_summary(blank('i64 -> i64'), None))",
    "print('summary', tsuzuri_lldb.function_summary(blank('Task<i64>'), None))",
    "descriptor = find('i64 -> i64').GetFieldAtIndex(0).GetType().GetPointeeType()",
    "print('descriptor', descriptor.GetName(), tsuzuri_lldb.field_names(descriptor), descriptor.GetByteSize())",
    "print('code', descriptor.GetFieldAtIndex(0).GetType().GetPointeeType().IsFunctionType())",
  ].join("\n"));
  // macOS keeps the DWARF in the `.dwarf` file beside the executable; Linux keeps it inside.
  const options = ["-b", "-o", `command script import ${JSON.stringify(formatter)}`];
  if (process.platform === "darwin") options.push("-o", `target symbols add ${JSON.stringify(`${app}.dwarf`)}`);
  options.push("-o", `script exec(open(${JSON.stringify(probe)}).read())`);
  const output = execute(lldb, [...options, "--", app]);
  assert.doesNotMatch(output, /Traceback|error:/, output);
  for (const line of [
    "shape i64 -> i64 True ['descriptor', 'environment']",
    "shape Task<i64> True ['descriptor', 'environment']",
    "shape Main.Plain False ['descriptor', 'environment']",
    "shape Main.Old False ['code', 'environment', 'clone', 'drop']",
    "shape Main.Shaped False ['descriptor', 'environment']",
    "shape Main.Entries False ['code', 'clone', 'drop']",
    "summary <fn 0x0>",
    "summary <task>",
    "descriptor closure.descriptor ['code', 'clone', 'drop'] 24",
    "code True",
  ]) assert.ok(output.includes(line + "\n"), `${line}\n${output}`);
  console.log("debugger_static: function values and Task are told from records that name their fields alike");
} finally {
  rmSync(root, { recursive: true, force: true });
}
