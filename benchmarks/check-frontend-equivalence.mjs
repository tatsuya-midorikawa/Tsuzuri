import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { writeProject } from "./build/generate.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const out = resolve(args.shift() ?? "target/perf/PB02-equivalence");
assert.ok(args.every((argument) => ["--reuse", "--refresh-after", "--cli-matrix", "--quick"].includes(argument)
  || argument.startsWith("--baseline-ref=")));
const refresh = args.includes("--refresh-after");
const reuse = args.includes("--reuse") || refresh;
const cliMatrix = args.includes("--cli-matrix");
const quick = args.includes("--quick");
const reference = args.find((argument) => argument.startsWith("--baseline-ref="))?.slice("--baseline-ref=".length);
const baseline = join(out, "baseline.json");
const revision = reuse
  ? JSON.parse(readFileSync(existsSync(baseline) ? baseline : join(out, "comparison.json"), "utf8")).revision
  : execFileSync("git", ["rev-parse", "--verify", "--end-of-options", `${reference ?? "HEAD"}^{commit}`],
    { cwd: root, encoding: "utf8" }).trim();
if (reuse && reference) {
  assert.equal(execFileSync("git", ["rev-parse", "--verify", "--end-of-options", `${reference}^{commit}`],
    { cwd: root, encoding: "utf8" }).trim(), revision, "reuse must match the preserved baseline");
}
assert.ok(out.startsWith(join(root, "target/perf/")), "isolation output must be under target/perf");
const stage = join(out, "source");
const owned = [
  "check.rs", "polymorph.rs", "closures.rs", "closures_rec.rs", "constants.rs",
  "const_static.rs", "ownership.rs", "ownership_control.rs", "recursive.rs", "gpu_devices.rs", "intern.rs", "type_arena.rs",
];
let before, after;
function build(label) {
  execFileSync("cargo", ["build", "--locked", "--release", "--jobs", "1", "--manifest-path",
    join(stage, "Cargo.toml"), "--target-dir", join(out, "build"), "--bins"], {
    cwd: root, stdio: "inherit", timeout: 600_000,
  });
  const executable = join(out, `frontend-${label}`);
  copyFileSync(join(out, "build/release/frontend-equivalence"), executable);
  copyFileSync(join(out, "build/release/tsuzuri"), join(out, `tsuzuri-${label}`));
  return executable;
}

function overlay() {
  for (const file of owned) copyFileSync(join(root, "src", file), join(stage, "src", file));
  // PM07's proof-root extension belongs to an independent backend optimization.
  const ownershipPath = join(stage, "src/ownership.rs");
  const ownership = readFileSync(ownershipPath, "utf8");
  assert.match(ownership, /let local = whole[\s\S]*?\.map\(\|\(id, _\)\| id\);/);
  writeFileSync(ownershipPath, ownership.replace(/let local = whole[\s\S]*?\.map\(\|\(id, _\)\| id\);/,
    `let local = match expression.kind {
            E::Local(id) if whole && !self.state.aliases.contains_key(&id) => Some(id),
            _ => None,
        };`));
}
if (!reuse) {
assert.ok(!existsSync(stage), `EEXIST: ${stage}`);
mkdirSync(stage, { recursive: true });
const archive = execFileSync("git", ["archive", revision, "Cargo.toml", "Cargo.lock", "src", "std"], {
  cwd: root, maxBuffer: 256 * 1024 * 1024,
});
execFileSync("tar", ["-x", "-C", stage], { input: archive });
const bin = join(stage, "src/bin");
mkdirSync(bin, { recursive: true });
writeFileSync(join(bin, "frontend-equivalence.rs"), `
use std::path::Path;
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let result = tsuzuri::driver::Project::load(Path::new(&args[1]));
    let module = match result {
        Ok(project) => match project.analyze_all() {
            Ok(module) => module,
            Err(errors) => {
                std::fs::write(format!("{}.typed", args[2]), format!("{errors:?}")).unwrap();
                return;
            }
        },
        Err(error) => {
            std::fs::write(format!("{}.typed", args[2]), format!("{error:?}")).unwrap();
            return;
        }
    };
    std::fs::write(format!("{}.typed", args[2]), format!("{module:?}")).unwrap();
    for (name, wasm) in [("native", false), ("wasm32", true)] {
        match tsuzuri::llvm::emit_target(&module, tsuzuri::llvm::Entry::Library, wasm) {
            Ok(ir) => std::fs::write(format!("{}.{name}.ll", args[2]), ir).unwrap(),
            Err(error) => std::fs::write(format!("{}.{name}.error", args[2]), format!("{error:?}")).unwrap(),
        }
    }
}
`);
before = build("before");
overlay();
after = build("after");
writeFileSync(baseline, JSON.stringify({ revision }) + "\n");
} else {
  before = join(out, "frontend-before");
  after = join(out, "frontend-after");
  for (const binary of [before, after, join(out, "tsuzuri-before"), join(out, "tsuzuri-after")])
    assert.ok(existsSync(binary), `missing isolated binary ${binary}`);
  if (refresh) {
    overlay();
    after = build("after");
  }
}
const projects = [];
const quickFixtures = new Set([
  "control", "computations", "exceptions", "string_interpolation",
  "local_recursion", "const_functions", "dyn_dispatch", "fixed_arrays",
]);
for (const directory of ["examples", "tests/fixtures"]) {
  for (const entry of readdirSync(join(root, directory), { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
    if (entry.isDirectory() && (!quick || (directory === "examples"
      ? entry.name === "hello" : quickFixtures.has(entry.name))))
      projects.push([`${directory.replace("/", "-")}-${entry.name}`, join(root, directory, entry.name)]);
  }
}
const diagnostics = [
  ["move", 'record Owned { text: string, count: i64 }\nlet value = Owned { text: "text", count: 1 }\nlet other = { value with count = 2 }\nvalue.text.length'],
  ["borrow-conflict", 'record Owned { text: string, count: i64 }\nlet value = Owned { text: "text", count: 1 }\nlet borrowed = ref value.text\nlet other = { value with count = 2 }\nborrowed.length'],
  ["borrowed-update", 'record Owned { text: string, count: i64 }\ndef update :: ref Owned -> Owned\nfn update borrowed = { deref borrowed with count = 2 }'],
  ["update-order", 'record Owned { text: string, count: i64 }\nlet value = Owned { text: "text", count: 1 }\n{ value with count = value.text.length }'],
  ["escape", 'fn bad() -> ref i64 { let value = 1; ref value }'],
  ["mutable-alias", 'let mut value = 1\nlet first = ref mut value\nlet second = ref mut value\n*first + *second'],
  ["generic-copy", 'def duplicate :: \'a -> (\'a * \'a) = \\value -> (value, value)\nduplicate "owned"'],
  ["lookup-order", 'fn first() -> i64 { absent_one }\nfn second() -> i64 { absent_two }'],
  ["multi-error", 'fn first() -> i64 { absent_one }\nfn second() -> bool { 1 }\nfn third() -> i64 { absent_three }'],
  ["field-order", 'record Point { x: i64 }\nlet point = Point { x: 1 }\n{ point with missing = 1; x = true }.x'],
  ["recursive-type", 'record A { b: B }\nrecord B { a: A }'],
  ["unbounded-specialization", "def rec f :: 'a -> unit\nfn rec f x = f [x]\nf 1"],
];
for (const [name, source] of diagnostics) {
  const project = join(out, "diagnostics", name);
  mkdirSync(project, { recursive: true });
  writeFileSync(join(project, "Main.tz"), source);
  projects.push([`diagnostic-${name}`, project]);
}
for (const size of quick ? [10] : [200, 1000]) {
  const project = join(out, `s${size}`);
  if (!existsSync(project)) writeProject(project, size);
  projects.push([`s${size}`, project]);
}
const pairs = [];
for (const [name, project] of projects) {
  for (const [label, executable] of [["before", before], ["after", after]]) {
    mkdirSync(join(out, label), { recursive: true });
    execFileSync(executable, [project, join(out, label, name)], {
      cwd: root, timeout: 600_000, maxBuffer: 256 * 1024 * 1024,
      env: { ...process.env, TSUZURI_CACHE_DIR: "", TSUZURI_TIME_PASSES: "" },
    });
    if (cliMatrix) {
      for (const target of ["native", "wasm32"]) {
        for (const opt of ["O0", "O3"]) {
          const prefix = join(out, label, `${name}.cli-${target}-${opt}`);
          const output = `${prefix}.ll`;
          const command = ["build", project, "--emit", "llvm", `-${opt}`,
            "--cpu", "generic", "--no-cache", "-o", output];
          if (target === "wasm32") command.push("--target", "wasm32");
          const child = spawnSync(join(out, `tsuzuri-${label}`), command, {
            cwd: root, encoding: "utf8", timeout: 600_000, maxBuffer: 256 * 1024 * 1024,
            env: { ...process.env, TSUZURI_CACHE_DIR: "", TSUZURI_TIME_PASSES: "" },
          });
          assert.ifError(child.error);
          writeFileSync(`${prefix}.diagnostics`, JSON.stringify({
            status: child.status,
            signal: child.signal,
            stdout: child.stdout.replaceAll(output, "<output>"),
            stderr: child.stderr.replaceAll(output, "<output>"),
          }) + "\n");
        }
      }
    }
  }
  if (name.startsWith("diagnostic-")) {
    assert.ok(!readFileSync(join(out, "before", `${name}.typed`), "utf8").startsWith("CheckedModule"),
      `${name} must exercise a diagnostic`);
  }
  const outputs = readdirSync(join(out, "before")).filter((file) => file.startsWith(`${name}.`));
  for (const file of outputs) {
    assert.ok(existsSync(join(out, "after", file)), `missing after output ${file}`);
    assert.ok(readFileSync(join(out, "before", file)).equals(readFileSync(join(out, "after", file))), `PB02 changed ${file}`);
  }
  assert.deepEqual(readdirSync(join(out, "after")).filter((file) => file.startsWith(`${name}.`)).sort(), outputs.sort());
  pairs.push({ project: name, outputs, exact: true });
  console.log(`${name}: exact frontend, diagnostics, baseline LLVM`);
}
writeFileSync(join(out, quick ? "comparison-quick.json" : "comparison.json"), JSON.stringify({
  revision,
  owned, excluded: "PM07 note_copy proof-root rule; all other backend/runtime streams use baseline sources",
  cliMatrix,
  quick,
  sourceSha256: Object.fromEntries(owned.map((file) => [
    file, createHash("sha256").update(readFileSync(join(stage, "src", file))).digest("hex"),
  ])),
  pairs,
}, null, 2) + "\n");
console.log(`${pairs.length} projects matched exactly`);
