import assert from "node:assert/strict";
import { lstatSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawn, spawnSync } from "node:child_process";

// G17: the frontend cache under <cache root>/frontend/ must not change any output.
const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const repository = resolve(new URL("..", import.meta.url).pathname);
const directory = mkdtempSync(join(tmpdir(), "tsuzuri-frontend-cache-e2e-"));
const cache = join(directory, "cache");
const frontend = join(cache, "frontend");
const env = { ...process.env, TSUZURI_CACHE_DIR: cache };
const disabled = { TSUZURI_CACHE_DIR: "" };

function cli(args, { success = true, overrides = {} } = {}) {
  const result = spawnSync(compiler, args, { encoding: "utf8", timeout: 300000, env: { ...env, ...overrides }, maxBuffer: 64 * 1024 * 1024 });
  assert.ifError(result.error);
  if (success) assert.equal(result.status, 0, `${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}
function concurrent(args) {
  return new Promise((resolveChild, reject) => {
    const child = spawn(compiler, args, { env, stdio: ["ignore", "pipe", "pipe"] });
    let stderr = "";
    child.stderr.on("data", chunk => { stderr += chunk; });
    child.on("error", reject);
    child.on("exit", code => code === 0 ? resolveChild() : reject(new Error(stderr)));
  });
}
const files = prefix => readdirSync(frontend).filter(name => name.startsWith(prefix)).sort();
const packs = () => files("p-");
const manifests = () => files("m-");
const only = list => { assert.equal(list.length, 1, list.join(", ")); return list[0]; };
const manifest = () => JSON.parse(readFileSync(join(frontend, only(manifests()))));
const moduleOf = name => manifest().modules.find(module => module.name === name);
const same = (left, right) => assert.deepEqual(readFileSync(left), readFileSync(right));
const outcome = result => ({ status: result.status, stdout: result.stdout, stderr: result.stderr });

// The std modules that a program loads without naming an opt-in module (D-40).
const optIn = new Set([...readFileSync(join(repository, "src/stdlib.rs"), "utf8").matchAll(/module: "(\w+)",/g)].map(match => match[1]));
const defaultStd = readdirSync(join(repository, "std"))
  .map(name => name.replace(/\.(tz|tt|tc)$/, ""))
  .filter(name => !optIn.has(name))
  .sort();
assert.ok(optIn.size > 0 && defaultStd.length > 10, `${[...optIn]} ${defaultStd}`);

// The project of benchmarks/run-cache.mjs: Module<i>.value() is i, Main.tz sums them.
const count = 20;
const project = join(directory, "project");
mkdirSync(project);
const modulePath = index => join(project, `Module${index}.tz`);
const main = ["let mut total = 0"];
for (let index = 0; index < count; index++) {
  writeFileSync(modulePath(index), `def value :: i64\nfn value = ${index}\n`);
  main.push(`total = total + Module${index}.value()`);
}
main.push("total");
writeFileSync(join(project, "Main.tz"), `${main.join("\n")}\n`);
const sum = count * (count - 1) / 2;

const builds = [];
for (const target of ["native", "wasm32"]) {
  for (const level of ["-O0", "-O3"]) {
    builds.push([target, level, ["--emit", "llvm", level, ...(target === "wasm32" ? ["--target", "wasm32"] : [])]]);
  }
}
function emit(label, extra, overrides) {
  const outputs = [];
  for (const [target, level, args] of builds) {
    const output = join(directory, `${label}-${target}${level}.ll`);
    cli(["build", project, ...args, "-o", output, ...extra], { overrides });
    outputs.push(output);
  }
  return outputs;
}

try {
  // 1. The IR is byte-identical without the cache, cold and warm.
  const reference = emit("reference", ["--no-cache"]);
  assert.throws(() => statSync(cache), /ENOENT/);
  const cold = emit("cold", []);
  const warm = emit("warm", []);
  reference.forEach((path, index) => { same(path, cold[index]); same(path, warm[index]); });

  // 2. Running gives the independently computed sum, cold and warm.
  rmSync(frontend, { recursive: true, force: true });
  assert.equal(cli(["run", project]).stdout, `${sum}\n`);
  assert.equal(cli(["run", project]).stdout, `${sum}\n`);

  // 3. One pack and one manifest per project and kind; a warm run rewrites neither.
  const pack = join(frontend, only(packs()));
  const manifestPath = join(frontend, only(manifests()));
  const modules = manifest().modules;
  const user = modules.filter(module => module.origin === "user").map(module => module.name);
  const std = modules.filter(module => module.origin === "std").map(module => module.name);
  assert.deepEqual(user, ["Main", ...Array.from({ length: count }, (_, index) => `Module${index}`)].sort());
  assert.deepEqual(std.sort(), defaultStd);
  assert.deepEqual(modules.map(module => module.name), modules.map(module => module.name).sort());
  assert.equal(manifest().kind, "program");
  const coldPack = readFileSync(pack);
  const stamps = [statSync(pack).mtimeMs, statSync(manifestPath).mtimeMs];
  cli(["build", project, "--emit", "llvm", "-O0", "-o", join(directory, "again.ll")]);
  assert.deepEqual([statSync(pack).mtimeMs, statSync(manifestPath).mtimeMs], stamps);

  // 4. A body-only edit changes the module's source key but not its interface.
  const before = moduleOf("Module3");
  writeFileSync(modulePath(3), "def value :: i64\nfn value = 30\n");
  const edited = join(directory, "edited.ll");
  const editedReference = join(directory, "edited-reference.ll");
  cli(["build", project, "--emit", "llvm", "-o", edited]);
  cli(["build", project, "--emit", "llvm", "--no-cache", "-o", editedReference]);
  same(editedReference, edited);
  const after = moduleOf("Module3");
  assert.notEqual(after.source, before.source);
  assert.equal(after.interface, before.interface);
  assert.equal(moduleOf("Module4").source, modules.find(module => module.name === "Module4").source);
  assert.equal(cli(["run", project]).stdout, `${sum - 3 + 30}\n`);

  // 5. A header edit changes the interface; the diagnostics match the uncached ones.
  writeFileSync(modulePath(3), "def value :: i32\nfn value = 30\n");
  const failed = cli(["build", project, "--emit", "llvm", "--json", "-o", edited], { success: false });
  const failedReference = cli(["build", project, "--emit", "llvm", "--json", "--no-cache", "-o", edited], { success: false });
  assert.deepEqual(outcome(failed), outcome(failedReference));
  assert.notEqual(failed.status, 0);
  assert.match(failed.stderr, /"code":"E1003"/);
  assert.notEqual(moduleOf("Module3").interface, before.interface);
  writeFileSync(modulePath(3), "def value :: i64\nfn value = 3\n");
  cli(["build", project, "--emit", "llvm", "-O0", "-o", edited]);
  same(reference[0], edited);
  assert.deepEqual(readFileSync(pack), coldPack);

  // 6. A corrupt, empty or symlinked pack is a miss that the next build replaces.
  const corruptions = [
    () => { const bytes = readFileSync(pack); bytes[bytes.length - 1] ^= 1; writeFileSync(pack, bytes); },
    () => writeFileSync(pack, ""),
    () => { const elsewhere = join(directory, "elsewhere.tzp"); writeFileSync(elsewhere, coldPack); rmSync(pack); symlinkSync(elsewhere, pack); },
  ];
  for (const corrupt of corruptions) {
    corrupt();
    const output = join(directory, "corrupt.ll");
    cli(["build", project, "--emit", "llvm", "-O0", "-o", output]);
    same(reference[0], output);
    assert.ok(lstatSync(pack).isFile());
    assert.deepEqual(readFileSync(pack), coldPack);
  }

  // 7. Concurrent cold builds all succeed with the same IR and leave no temporary files.
  rmSync(frontend, { recursive: true, force: true });
  await Promise.all(Array.from({ length: 6 }, (_, index) => concurrent(["build", project, "--emit", "llvm", "-O0", "-o", join(directory, `race-${index}.ll`)])));
  for (let index = 0; index < 6; index++) same(reference[0], join(directory, `race-${index}.ll`));
  assert.deepEqual(files(".tmp-"), []);
  assert.deepEqual(readFileSync(pack), coldPack);

  // 8. Diagnostics keep their codes, messages, positions and order.
  const syntax = join(directory, "syntax");
  mkdirSync(syntax);
  writeFileSync(join(syntax, "Main.tz"), "Alpha.one () + Beta.two ()\n");
  writeFileSync(join(syntax, "Alpha.tz"), "def one :: unit -> i64 = \\_ -> (1 +\n");
  writeFileSync(join(syntax, "Beta.tz"), "def two :: unit -> = 2\n");
  const typed = join(directory, "typed");
  mkdirSync(typed);
  writeFileSync(join(typed, "Main.tz"), "let unused = 1\nAlpha.one () + Beta.two () + Gamma.three ()\n");
  writeFileSync(join(typed, "Alpha.tz"), "def one :: unit -> i64 = \\_ ->\n    let spare = 2\n    true\n");
  writeFileSync(join(typed, "Beta.tz"), "def two :: unit -> i64 = \\_ -> \"two\"\n");
  writeFileSync(join(typed, "Gamma.tz"), "def three :: unit -> i64 = \\_ ->\n    let idle = 3\n    3\n");
  for (const target of [syntax, typed]) {
    rmSync(frontend, { recursive: true, force: true });
    const uncached = cli(["check", target, "--json"], { success: false, overrides: disabled });
    const coldCheck = cli(["check", target, "--json"], { success: false });
    const warmCheck = cli(["check", target, "--json"], { success: false });
    assert.notEqual(uncached.status, 0);
    assert.deepEqual(outcome(coldCheck), outcome(uncached));
    assert.deepEqual(outcome(warmCheck), outcome(uncached));
    const build = ["build", target, "--emit", "llvm", "--json", "-o", join(directory, "failed.ll")];
    assert.deepEqual(outcome(cli(build, { success: false })), outcome(cli([...build, "--no-cache"], { success: false })));
  }
  const reported = cli(["check", syntax, "--json"], { success: false }).stderr.trim().split("\n").map(line => JSON.parse(line));
  assert.ok(reported.filter(diagnostic => diagnostic.code === "E0002").length >= 2, JSON.stringify(reported));
  writeFileSync(join(typed, "Alpha.tz"), "def one :: unit -> i64 = \\_ ->\n    let spare = 2\n    1\n");
  writeFileSync(join(typed, "Beta.tz"), "def two :: unit -> i64 = \\_ -> 2\n");
  const warned = cli(["check", typed, "--json"]);
  assert.deepEqual(outcome(warned), outcome(cli(["check", typed, "--json"], { overrides: disabled })));
  assert.equal((warned.stderr.match(/"code":"W1001"/g) ?? []).length, 3, warned.stderr);

  // 9. TSUZURI_CACHE_DIR= disables both caches; an unrelated directory is left alone.
  const empty = cli(["build", project, "--emit", "llvm", "-O0", "-o", join(directory, "empty.ll")], { overrides: disabled });
  assert.match(empty.stderr, /build cache disabled/);
  same(reference[0], join(directory, "empty.ll"));
  const unrelated = join(directory, "unrelated");
  mkdirSync(unrelated);
  writeFileSync(join(unrelated, "keep"), "preserved");
  const foreign = cli(["build", project, "--emit", "llvm", "-O0", "-o", join(directory, "foreign.ll")], { overrides: { TSUZURI_CACHE_DIR: unrelated } });
  assert.match(foreign.stderr, /cache disabled/);
  assert.deepEqual(readdirSync(unrelated), ["keep"]);
  same(reference[0], join(directory, "foreign.ll"));

  // 10. check, test and doc use the cache too (D5) with unchanged output; --no-cache stays build/run only.
  rmSync(cache, { recursive: true, force: true });
  const checked = cli(["check", project]);
  only(packs());
  assert.equal(manifest().kind, "program");
  assert.deepEqual(outcome(cli(["check", project])), outcome(checked));
  assert.deepEqual(outcome(checked), outcome(cli(["check", project], { overrides: disabled })));
  assert.equal(JSON.parse(cli(["check", project, "--no-cache", "--json"], { success: false }).stderr).code, "E2000");
  assert.equal(JSON.parse(cli(["test", project, "--no-cache", "--json"], { success: false }).stderr).code, "E2000");
  const tested = join(directory, "tested");
  mkdirSync(tested);
  writeFileSync(join(tested, "Math2.tz"), "def double :: i64 -> i64 = \\x -> x * 2\n\ntest \"doubles\" = assert (double 21 == 42)\n\ntest \"fails\" = assert (double 2 == 5)\n");
  const testUncached = cli(["test", tested], { success: false, overrides: disabled });
  assert.notEqual(testUncached.status, 0);
  for (let run = 0; run < 2; run++) assert.deepEqual(outcome(cli(["test", tested], { success: false })), outcome(testUncached));
  const documented = [join(directory, "docs-uncached"), join(directory, "docs-cold"), join(directory, "docs-warm")];
  cli(["doc", project, "-o", documented[0]], { overrides: disabled });
  cli(["doc", project, "-o", documented[1]]);
  cli(["doc", project, "-o", documented[2]]);
  for (const name of readdirSync(documented[0])) {
    same(join(documented[0], name), join(documented[1], name));
    same(join(documented[0], name), join(documented[2], name));
  }
  const kinds = manifests().map(name => JSON.parse(readFileSync(join(frontend, name))).kind).sort();
  assert.deepEqual(kinds, ["docs", "program", "tests"]);
  console.log("frontend cache e2e: ok");
} finally {
  rmSync(directory, { recursive: true, force: true });
}
