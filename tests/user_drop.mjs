import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const clang = process.env.TSUZURI_CLANG ?? "clang";
const sanitizerKind = process.env.TSUZURI_TSAN === "1" ? "thread" : process.env.TSUZURI_ASAN === "1" ? "address" : null;
const sanitizer = sanitizerKind ? [`-fsanitize=${sanitizerKind}`] : [];
const fixture = "tests/fixtures/user_drop";
const root = mkdtempSync(join(tmpdir(), "tsuzuri-user-drop-"));
function execute(program, args, success = true) {
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 180_000, maxBuffer: 16 * 1024 * 1024 });
  assert.ifError(result.error);
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}
// The trapping export must stop before any user drop runs.
function trapsWithoutDrops(executable) {
  const result = execute(executable, ["trap"], false);
  assert.notEqual(result.status, 0);
  assert.ok(!result.stdout.includes("dropped"), result.stdout);
}
function instantiate(wasm) {
  const module = new WebAssembly.Module(readFileSync(wasm));
  assert.deepEqual(WebAssembly.Module.imports(module).map(({ module, name, kind }) => { assert.equal(module, "tsuzuri"); assert.equal(kind, "function"); return name; }), ["Main.drop_log"]);
  const log = [];
  const instance = new WebAssembly.Instance(module, { tsuzuri: { "Main.drop_log": (value) => { log.push(value); } } });
  return { instance, log };
}
try {
  execute(compiler, ["build", fixture, "--emit", "header", "-o", join(root, "tz-user-drop.h")]);
  const irPath = join(root, "user_drop.ll");
  execute(compiler, ["build", fixture, "--emit", "llvm", "-o", irPath]);
  const ir = readFileSync(irPath, "utf8").replaceAll("@malloc", "@tracked_alloc").replaceAll("@free", "@tracked_free").replaceAll("@realloc", "@tracked_realloc");
  writeFileSync(irPath, ir);
  for (const optimization of ["-O0", "-O3"]) {
    const executable = join(root, `user-drop${optimization}`);
    execute(clang, [optimization, ...sanitizer, "-Wno-override-module", irPath, "tests/user_drop_runtime.c", "src/runtime/task.c", "-I", root, "-pthread", "-lm", "-o", executable]);
    execute(executable, []);
    trapsWithoutDrops(executable);
    const object = join(root, `user-drop${optimization}.o`);
    execute(compiler, ["build", fixture, "--emit", "object", optimization, "-o", object]);
    const linked = join(root, `linked${optimization}`);
    execute(clang, [optimization, object, "tests/user_drop_runtime.c", "-I", root, "-pthread", "-lm", "-o", linked]);
    execute(linked, []);
    trapsWithoutDrops(linked);

    const wasm = join(root, `user-drop${optimization}.wasm`);
    execute(compiler, ["build", fixture, "--target", "wasm32", optimization, "-o", wasm]);
    const { instance, log } = instantiate(wasm);
    const call = (name, ...args) => { log.length = 0; return instance.exports[`tz_${name}`](...args); };
    const values = (expected) => assert.deepEqual(log, expected.map(BigInt));
    assert.equal(call("scopes"), 3n); values([2, 1]);
    assert.equal(call("fields"), 1n); values([1, 10, 11]);
    assert.equal(call("reassign"), 2n); values([1, 2]);
    assert.equal(call("consume"), 7n); values([7]);
    assert.equal(call("loop_break"), 4n); values([0, 1, 2, 3]);
    assert.equal(call("array"), 3n); values([1, 2, 3]);
    assert.equal(call("map_remove"), 2n);
    assert.deepEqual(log.slice(0, 2), [2n, 100n]);
    assert.deepEqual(log.slice(2).sort((left, right) => Number(left - right)), [1n, 3n]);
    assert.equal(call("generic"), 9n); values([5, 4]);
    assert.equal(call("chain"), 3n); values([1, 2, 3]);
    // 1,000,000 nodes exceed the default 16 MiB memory; the larger build below covers them.
    assert.equal(call("long_chain", 100000n), 100000n);
    assert.equal(log.length, 100001); assert.equal(log.reduce((total, value) => total + value, 0n), 100001n);
    assert.equal(call("task_owned"), 7n); values([7]);
    assert.equal(call("task_unstarted"), 0n); values([8]);
    assert.equal(call("tasks_parallel"), 120n);
    assert.equal(log.length, 16); assert.equal(log.reduce((total, value) => total + value, 0n), 120n);
    assert.equal(call("drop_allocates"), 5n); values([1]);
    assert.equal(call("moved_field"), 1n); values([1, 2]);
    assert.equal(call("temporary_field"), 6n); values([6]);
    assert.equal(call("unions"), 123n); values([31, 20, 21]);
    assert.equal(call("use_scopes"), 1n); values([2, 1]);
    assert.equal(call("use_computation"), 15n); values([3, 4]);
    assert.equal(call("use_task"), 6n); values([5]);
    assert.equal(call("early_drop"), 2n); values([1, 100, 2]);
    assert.equal(call("owned_function"), 44n); values([100, 7]);
    assert.equal(call("owned_task"), 9n); values([8]);
    assert.equal(call("owned_unused"), 0n); values([9]);
    assert.equal(call("owned_array"), 31n); values([1, 2]);
    assert.equal(call("owned_nested"), 8n); values([3]);
    assert.equal(call("owned_generic"), 2n); values([4]);
    for (let index = 0; index < 10000; index++) { assert.equal(call("scopes"), 3n); values([2, 1]); }
    assert.ok(instance.exports.memory.buffer.byteLength <= 16 * 1024 * 1024);
    const trapped = instantiate(wasm);
    assert.throws(() => trapped.instance.exports.tz_trap_after_create(), WebAssembly.RuntimeError);
    assert.deepEqual(trapped.log, []);

    const large = join(root, `user-drop-large${optimization}.wasm`);
    execute(compiler, ["build", fixture, "--target", "wasm32", optimization, "--wasm-max-memory", "128MiB", "-o", large]);
    const deep = instantiate(large);
    assert.equal(deep.instance.exports.tz_long_chain(1000000n), 1000000n);
    assert.equal(deep.log.length, 1000001); assert.equal(deep.log.reduce((total, value) => total + value, 0n), 1000001n);
    console.log(`user drop: native/WASM ${optimization} passed`);
  }
} finally { rmSync(root, { recursive: true, force: true }); }
