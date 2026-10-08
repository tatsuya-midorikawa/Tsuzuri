import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const fixture = resolve("tests/fixtures/debug_info");
const view = resolve("tests/fixtures/debug_view");
const clang = process.env.TSUZURI_CLANG ?? "clang";
const dwarf = process.env.TSUZURI_DWARFDUMP ?? "llvm-dwarfdump";
const root = mkdtempSync(join(tmpdir(), "tsuzuri-debug-"));

function execute(program, args) {
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 180_000, maxBuffer: 16 * 1024 * 1024 });
  assert.ifError(result.error);
  assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  assert.doesNotMatch(result.stderr, /ignoring invalid debug info|invalid !dbg|inlinable function call in a function with debug info/);
  return result.stdout;
}

try {
  execute(dwarf, ["--version"]);
  for (const optimization of ["-O0", "-O3"]) {
    const ir = join(root, `debug${optimization}.ll`);
    const repeated = join(root, `again${optimization}.ll`);
    for (const output of [ir, repeated]) execute(compiler, ["build", fixture, "--emit", "llvm", optimization, "-g", "--trap-info", "-o", output]);
    const text = readFileSync(ir, "utf8");
    assert.equal(text, readFileSync(repeated, "utf8"));
    for (const item of ["!DICompileUnit", "!DILocation", "!DILocalVariable", "!DICompositeType", "@llvm.dbg.declare"]) assert.ok(text.includes(item), item);
    const object = join(root, `debug${optimization}.o`);
    execute(compiler, ["build", fixture, "--emit", "object", optimization, "-g", "--trap-info", "-o", object]);
    execute(dwarf, ["--verify", object]);
    const info = execute(dwarf, ["--debug-info", object]);
    for (const item of ["DW_TAG_compile_unit", "DW_TAG_subprogram", '"Main.answer"']) assert.ok(info.includes(item), item);
    if (optimization === "-O0") for (const name of ['"input"', '"point"', '"label"', '"values"', '"capacity"', '"pair"']) assert.ok(info.includes(name), name);
    const host = join(root, "host.c");
    writeFileSync(host, "#include <stdint.h>\nextern int64_t tz_answer(int64_t);\nextern int64_t tz_parallel(void);\nint main(void) { return tz_answer(40) != 42 || tz_parallel() != 42; }\n");
    const executable = join(root, `host${optimization}`);
    execute(clang, [host, object, "-pthread", "-lm", "-o", executable]);
    execute(executable, []);
    const consolePath = join(root, `console${optimization}`);
    execute(compiler, ["build", fixture, "-g", optimization, "-o", consolePath]);
    assert.equal(execute(consolePath, []).trim(), "42");
    const consoleSymbols = process.platform === "darwin" ? `${consolePath}.dwarf` : consolePath;
    execute(dwarf, ["--verify", consoleSymbols]);
    assert.match(execute(dwarf, ["--debug-info", consoleSymbols]), /DW_TAG_compile_unit/);
    const wasm = join(root, `debug${optimization}.wasm`);
    execute(compiler, ["build", fixture, "--target", "wasm32", "-g", optimization, "--trap-info", "-o", wasm]);
    execute(dwarf, ["--verify", wasm]);
    const module = new WebAssembly.Module(readFileSync(wasm));
    assert.deepEqual(WebAssembly.Module.imports(module), []);
    assert.ok(WebAssembly.Module.customSections(module, ".debug_info").length > 0);
    assert.ok(WebAssembly.Module.customSections(module, ".debug_line").length > 0);
    const instance = new WebAssembly.Instance(module);
    assert.equal(instance.exports.tz_answer(40n), 42n);
    assert.equal(instance.exports.tz_parallel(), 42n);
    const plain = join(root, `plain${optimization}.wasm`);
    execute(compiler, ["build", fixture, "--target", "wasm32", optimization, "-o", plain]);
    const plainModule = new WebAssembly.Module(readFileSync(plain));
    assert.equal(WebAssembly.Module.customSections(plainModule, ".debug_info").length, 0);
    assert.equal(new WebAssembly.Instance(plainModule).exports.tz_answer(40n), 42n);
    // G16: the Tsuzuri shapes of the debugger fixture in a native object and a wasm32 module.
    const viewObject = join(root, `view${optimization}.o`);
    execute(compiler, ["build", view, "--emit", "object", optimization, "-g", "-o", viewObject]);
    const viewWasm = join(root, `view${optimization}.wasm`);
    execute(compiler, ["build", view, "--target", "wasm32", optimization, "-g", "-o", viewWasm]);
    assert.deepEqual(WebAssembly.Module.imports(new WebAssembly.Module(readFileSync(viewWasm))), []);
    const shapes = ["DW_TAG_typedef", "DW_TAG_enumeration_type", "DW_TAG_union_type", '"$tag"', '"$payload"', '"Main.Tree.node"', '"[|i32|].node"', '"Main.show.lambda@22:15"'];
    for (const file of [viewObject, viewWasm]) {
      execute(dwarf, ["--verify", file]);
      const info = execute(dwarf, ["--debug-info", file]);
      assert.doesNotMatch(info, /DW_AT_linkage_name/, file);
      // Optimization may drop the types of removed variables, so -O3 checks them in the IR.
      for (const item of optimization === "-O0" ? shapes : shapes.slice(0, 1)) assert.ok(info.includes(item), `${file}: ${item}`);
    }
    const viewIr = join(root, `view${optimization}.ll`);
    execute(compiler, ["build", view, "--emit", "llvm", optimization, "-g", "-o", viewIr]);
    const viewText = readFileSync(viewIr, "utf8");
    for (const item of ["DW_TAG_enumeration_type, name: \"Main.Color\"", "DW_TAG_union_type, name: \"Main.Shape.$payload\"", "name: \"Main.Tree.node\"", "DW_TAG_typedef, name: \"i64\""]) assert.ok(viewText.includes(item), item);
    assert.ok(!viewText.includes("linkageName:"));
    console.log(`debug info: native/WASM ${optimization} verified`);
  }
} finally { rmSync(root, { recursive: true, force: true }); }
