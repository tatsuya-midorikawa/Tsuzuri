import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const clang = process.env.TSUZURI_CLANG ?? "clang";
const ar = process.env.TSUZURI_AR ?? "ar";
const hostSource = resolve("tests/ffi_extensions_host.c");
const root = mkdtempSync(join(tmpdir(), "tsuzuri-ffi-"));
const fixture = join(root, "fixture");
cpSync("tests/fixtures/ffi_extensions", fixture, { recursive: true });
function execute(program, args, success = true) {
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 180_000, maxBuffer: 16 * 1024 * 1024 });
  assert.ifError(result.error);
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}
function rejected(args, code) {
  const result = execute(compiler, args, false);
  assert.notEqual(result.status, 0, args.join(" "));
  assert.match(result.stderr, new RegExp(`error\\[${code}\\]`), `${args.join(" ")}\n${result.stderr}`);
}
// A copy of the fixture with an entry expression: counters 10 = 59 and callbacks 4 = 39.
function application(name) {
  const directory = join(root, name);
  mkdirSync(directory);
  writeFileSync(join(directory, "Main.tz"), `${readFileSync(join(fixture, "Main.tz"), "utf8")}\ncounters 10 + callbacks 4\n`);
  return directory;
}
try {
  const header = join(root, "tz-ffi.h");
  execute(compiler, ["build", fixture, "--emit", "header", "-o", header]);
  const headerText = readFileSync(header, "utf8");
  assert.equal(headerText.match(/typedef struct tz_handle_4Main_7Counter_s \*tz_handle_4Main_7Counter;/g).length, 1);
  assert.match(headerText, /tz_handle_4Main_7Counter tz_pass_through\(tz_handle_4Main_7Counter arg0\);/);
  // An explicit symbol is the host's own: the header declares no prototype for it.
  assert.doesNotMatch(headerText, /e12_|sqrt/);
  const irPath = join(root, "ffi.ll");
  execute(compiler, ["build", fixture, "--emit", "llvm", "-o", irPath]);
  const ir = readFileSync(irPath, "utf8").replaceAll("@malloc", "@tracked_alloc").replaceAll("@free", "@tracked_free").replaceAll("@realloc", "@tracked_realloc");
  writeFileSync(irPath, ir);
  for (const optimization of ["-O0", "-O3"]) {
    const executable = join(root, `ffi${optimization}`);
    execute(clang, [optimization, "-Wno-override-module", "-DE12_HOST_MAIN", irPath, hostSource, "-I", root, "-lm", "-o", executable]);
    execute(executable, []);
    // A trap inside a callback ends the process abnormally.
    assert.notEqual(execute(executable, ["trap"], false).status, 0);
    const object = join(root, `ffi${optimization}.o`);
    execute(compiler, ["build", fixture, "--emit", "object", optimization, "-o", object]);
    const linked = join(root, `linked${optimization}`);
    execute(clang, [optimization, "-DE12_HOST_MAIN", object, hostSource, "-I", root, "-lm", "-o", linked]);
    execute(linked, []);
    console.log(`ffi extensions: native ${optimization} passed`);
  }

  const app = application("app");
  const host = join(root, "host.o");
  execute(clang, ["-c", hostSource, "-o", host]);
  const run = (args) => { const result = execute(compiler, ["run", ...args]); assert.equal(result.stdout.trim(), "98"); };
  run([app, "--link", host]);
  const library = join(root, "lib");
  mkdirSync(library);
  execute(ar, ["rcs", join(library, "libe12host.a"), host]);
  run([app, "-L", library, "-l", "e12host"]);
  const packaged = application("packaged");
  cpSync(host, join(packaged, "host.o"));
  writeFileSync(join(packaged, "Tsuzuri.toml"), '[package]\nname = "ffi-app"\nversion = "0.1.0"\n\n[native]\nlink = ["host.o"]\n');
  run([packaged]);
  // build writes an executable that runs without the compiler too.
  const built = join(root, "built");
  execute(compiler, ["build", app, "-O0", "--link", host, "-o", built]);
  assert.equal(execute(built, []).stdout.trim(), "98");
  // Debug-info executables take the same inputs (macOS links them in a second step).
  const debuggable = join(root, "debuggable");
  execute(compiler, ["build", app, "-g", "-O0", "--link", host, "-o", debuggable]);
  assert.equal(execute(debuggable, []).stdout.trim(), "98");
  // So does tsuzuri test, which builds an executable of its own.
  const tested = join(root, "tested");
  mkdirSync(tested);
  writeFileSync(join(tested, "Main.tz"), `${readFileSync(join(fixture, "Main.tz"), "utf8")}\ntest "counters" =\n    assert (counters 10 == 59)\n\ntest "callbacks" =\n    assert (callbacks 4 == 39)\n`);
  assert.match(execute(compiler, ["test", tested, "--link", host]).stdout, /2 passed; 0 failed/);
  rejected(["run", app], "E2002");
  rejected(["run", app, "--link", join(root, "missing.o")], "E2001");
  rejected(["run", app, "-L", join(root, "missing-dir"), "-l", "e12host"], "E2001");
  rejected(["build", app, "--target", "wasm32", "--link", host, "-o", join(root, "x.wasm")], "E2000");
  rejected(["build", app, "-l", "lib-bad/name", "-o", join(root, "y")], "E2000");
  const same = join(root, "same.o");
  cpSync(host, same);
  rejected(["build", app, "--link", same, "-o", same], "E2003");
  assert.deepEqual(readFileSync(same), readFileSync(host));
  const dependent = join(root, "dependent");
  mkdirSync(join(dependent, "lib1"), { recursive: true });
  writeFileSync(join(dependent, "Tsuzuri.toml"), '[package]\nname = "root-app"\nversion = "0.1.0"\n\n[dependencies]\nlib1 = { path = "lib1" }\n');
  writeFileSync(join(dependent, "lib1", "Tsuzuri.toml"), '[package]\nname = "lib1"\nversion = "0.1.0"\n\n[native]\nlink = ["x.o"]\n');
  writeFileSync(join(dependent, "lib1", "Util.tz"), "export def one :: i64\nfn one = 1\n");
  writeFileSync(join(dependent, "Main.tz"), "1\n");
  rejected(["check", dependent], "E2000");
  console.log("ffi extensions: run passed");

  const expectedImports = [
    "env.e12_host_now:function",
    "tsuzuri.e12_add_one:function",
    "tsuzuri.e12_apply_twice:function",
    "tsuzuri.e12_counter_add:function",
    "tsuzuri.e12_counter_free:function",
    "tsuzuri.e12_counter_new:function",
    "tsuzuri.e12_fold:function",
    "tsuzuri.e12_run:function",
    "tsuzuri.e12_visit:function",
    "tsuzuri.sqrt:function",
  ];
  // Checks one linked module against the hand-computed table. `wide` is wasm64: pointers, handles and
  // table indices are BigInt there.
  function verify(module, wide) {
    assert.deepEqual(WebAssembly.Module.imports(module).map(({ module, name, kind }) => `${module}.${name}:${kind}`).sort(), expectedImports);
    // The function table is exported because callbacks are table indices.
    assert.ok(WebAssembly.Module.exports(module).some(({ name, kind }) => name === "__indirect_function_table" && kind === "table"));
    const handles = new Map();
    let next = wide ? 1n : 1;
    let instance;
    const callback = (index) => instance.exports.__indirect_function_table.get(index);
    const imports = {
      tsuzuri: {
        sqrt: Math.sqrt,
        e12_counter_new: (start) => { const handle = next++; handles.set(handle, start); return handle; },
        e12_counter_add: (handle, amount) => { const value = handles.get(handle) + amount; handles.set(handle, value); return value; },
        e12_counter_free: (handle) => { const value = handles.get(handle); assert.ok(handles.delete(handle)); return value; },
        e12_add_one: (value) => value + 1n,
        e12_apply_twice: (index, value) => callback(index)(callback(index)(value)),
        e12_fold: (index, count) => { let total = 0n; for (let item = 1n; item <= count; item++) total = callback(index)(total, item); return total; },
        e12_run: (index) => callback(index)(),
        e12_visit: (index, handle) => callback(index)(handle),
      },
      env: { e12_host_now: () => 40n },
    };
    instance = new WebAssembly.Instance(module, imports);
    assert.equal(instance.exports.tz_counters(10n), 59n);
    assert.equal(instance.exports.tz_callbacks(4n), 39n);
    assert.equal(instance.exports.tz_callback_shapes(10n), 82n);
    assert.equal(instance.exports.tz_misc(2.25), 40n);
    assert.equal(instance.exports.tz_misc(2), 0n);
    assert.equal(instance.exports.tz_pass_through(wide ? 7n : 7), wide ? 7n : 7);
    assert.equal(handles.size, 0);
    // A trap in a callback reaches the caller of the export; the instance is not used afterwards.
    assert.throws(() => instance.exports.tz_callback_trap(1n), WebAssembly.RuntimeError);
  }
  for (const optimization of ["-O0", "-O3"]) {
    const wasm = join(root, `ffi${optimization}.wasm`);
    execute(compiler, ["build", fixture, "--target", "wasm32", optimization, "-o", wasm]);
    verify(new WebAssembly.Module(readFileSync(wasm)), false);
    // Node.js 24 runs wasm64; older engines cannot load memory64, so the build is only linked there.
    const wide = join(root, `ffi${optimization}-64.wasm`);
    execute(compiler, ["build", fixture, "--target", "wasm64", optimization, "-o", wide]);
    if (Number(process.versions.node.split(".")[0]) >= 24) verify(new WebAssembly.Module(readFileSync(wide)), true);
    // Programs without callbacks keep a table-free export list.
    const plain = join(root, `plain${optimization}.wasm`);
    execute(compiler, ["build", "tests/fixtures/host_imports", "--target", "wasm32", optimization, "-o", plain]);
    assert.ok(!WebAssembly.Module.exports(new WebAssembly.Module(readFileSync(plain))).some(({ name }) => name === "__indirect_function_table"));
    console.log(`ffi extensions: WASM ${optimization} passed`);
  }
} finally { rmSync(root, { recursive: true, force: true }); }
