import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const clang = process.env.TSUZURI_CLANG ?? "clang";
const fixture = "tests/fixtures/host_imports";
const root = mkdtempSync(join(tmpdir(), "tsuzuri-imports-"));
function execute(program, args, success = true) {
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 180_000, maxBuffer: 16 * 1024 * 1024 });
  assert.ifError(result.error);
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}
try {
  execute(compiler, ["build", fixture, "--emit", "header", "-o", join(root, "tz-imports.h")]);
  const irPath = join(root, "imports.ll");
  execute(compiler, ["build", fixture, "--emit", "llvm", "-o", irPath]);
  const ir = readFileSync(irPath, "utf8").replaceAll("@malloc", "@tracked_alloc").replaceAll("@free", "@tracked_free").replaceAll("@realloc", "@tracked_realloc");
  writeFileSync(irPath, ir);
  for (const optimization of ["-O0", "-O3"]) {
    const executable = join(root, `imports${optimization}`);
    execute(clang, [optimization, "-Wno-override-module", irPath, "tests/host_imports_runtime.c", "src/runtime/task.c", "-I", root, "-pthread", "-lm", "-o", executable]);
    execute(executable, []);
    for (const mode of ["0", "1", "2"]) assert.notEqual(execute(executable, [mode], false).status, 0);
    const object = join(root, `imports${optimization}.o`);
    execute(compiler, ["build", fixture, "--emit", "object", optimization, "-o", object]);
    const linked = join(root, `linked${optimization}`);
    execute(clang, [optimization, object, "tests/host_imports_runtime.c", "-I", root, "-pthread", "-lm", "-o", linked]);
    execute(linked, []);
    const wasm = join(root, `imports${optimization}.wasm`);
    execute(compiler, ["build", fixture, "--target", "wasm32", optimization, "--trap-info", "-o", wasm]);
    const module = new WebAssembly.Module(readFileSync(wasm));
    const names = ["now", "mark", "combine", "small", "flag", "host_read", "host_buffer", "host_text", "host_point", "invalid_buffer"].map((name) => `Main.${name}`).sort();
    assert.deepEqual(WebAssembly.Module.imports(module).map(({ module, name, kind }) => { assert.equal(module, "tsuzuri"); assert.equal(kind, "function"); return name; }).sort(), names);
    assert.throws(() => new WebAssembly.Instance(module));
    let instance;
    let order = 0n;
    let calls = 0;
    const data = () => new DataView(instance.exports.memory.buffer);
    const descriptor = (out, pointer, length) => { data().setUint32(out, pointer, true); data().setBigInt64(out + 8, length, true); };
    instance = new WebAssembly.Instance(module, { tsuzuri: {
      "Main.now": () => { calls++; return 40n; },
      "Main.mark": (value) => { order = order * 10n + value; return value; },
      "Main.combine": (left, right) => left * 10n + right,
      "Main.small": (value) => value + 1,
      "Main.flag": (value) => value ? 0 : 7,
      "Main.host_read": (pointer, length, text, units, utf8, bytes, point) => {
        assert.equal(length, 2n); assert.equal(data().getBigInt64(pointer, true), 20n); assert.equal(data().getBigInt64(pointer + 8, true), 22n);
        assert.equal(units, 3n); assert.equal(data().getUint16(text, true), 97);
        assert.equal(bytes, 2n); assert.equal(data().getUint8(utf8), 120);
        assert.equal(data().getFloat64(point, true), 3); assert.equal(data().getInt32(point + 8, true), 1);
        return 42n;
      },
      "Main.host_buffer": (out) => { const pointer = instance.exports.tsuzuri_alloc(16n); data().setBigInt64(pointer, 20n, true); data().setBigInt64(pointer + 8, 22n, true); descriptor(out, pointer, 2n); },
      "Main.host_text": (out) => { const pointer = instance.exports.tsuzuri_alloc(2n); data().setUint8(pointer, 111); data().setUint8(pointer + 1, 107); descriptor(out, pointer, 2n); },
      "Main.host_point": (out, point) => { data().setFloat64(out, data().getFloat64(point, true) + 0.5, true); data().setInt32(out + 8, 7, true); },
      "Main.invalid_buffer": (out, mode) => descriptor(out, 0, mode === 0n ? -1n : mode === 1n ? 1n : (1n << 63n) - 1n),
    } });
    assert.equal(instance.exports.tz_effects(0), 57n); assert.equal(order, 1235n); assert.equal(calls, 1);
    order = 0n; assert.equal(instance.exports.tz_effects(1), 56n); assert.equal(order, 1234n);
    assert.equal(instance.exports.tz_function_value(), 82n);
    for (let value = -260n; value <= 260n; value++) assert.equal(instance.exports.tz_normalized(value), 1);
    for (let index = 0; index < 10000; index++) assert.equal(instance.exports.tz_buffers(), 86n);
    const before = calls; assert.equal(instance.exports.tz_tasks(), 640n); assert.equal(calls, before + 16);
    for (const mode of [0n, 1n, 2n]) assert.throws(() => instance.exports.tz_bad_buffer(mode), WebAssembly.RuntimeError);
    assert.ok(instance.exports.memory.buffer.byteLength <= 16 * 1024 * 1024);
    console.log(`host imports: native/WASM ${optimization} passed`);
  }
} finally { rmSync(root, { recursive: true, force: true }); }
