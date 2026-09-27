import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const objdump = process.env.TSUZURI_OBJDUMP ?? "llvm-objdump";
const root = mkdtempSync(join(tmpdir(), "tsuzuri-wasm-simd-"));
const vectorInstruction = /\b(?:v128|i8x16|i16x8|i32x4|i64x2|f32x4|f64x2)\./;
function execute(program, args) {
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 180_000, maxBuffer: 16 * 1024 * 1024 });
  assert.ifError(result.error);
  assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result.stdout;
}
try {
  execute(objdump, ["--version"]);
  for (const optimization of ["-O0", "-O3"]) {
    const artifacts = [];
    for (const simd of [false, true]) {
      const path = join(root, `${optimization}-${simd}.wasm`);
      const options = simd ? ["--wasm-feature", "simd128"] : [];
      execute(compiler, ["build", "tests/fixtures/wasm_simd", "--target", "wasm32", optimization, ...options, "-o", path]);
      const bytes = readFileSync(path);
      assert.ok(WebAssembly.validate(bytes));
      const module = new WebAssembly.Module(bytes);
      assert.deepEqual(WebAssembly.Module.imports(module), []);
      const disassembly = execute(objdump, ["-d", path]);
      if (!simd) assert.doesNotMatch(disassembly, vectorInstruction);
      const instance = new WebAssembly.Instance(module);
      for (const count of [0n, 1n, 3n, 4n, 17n, 1024n, 10000n]) for (const seed of [0, -1, 2147483647, -2147483648]) {
        let expected = 0n;
        for (let index = 0n; index < count; index++) expected += BigInt.asIntN(32, (index + BigInt(seed)) * 3n + 7n);
        assert.equal(instance.exports.tz_transform(count, seed), expected);
      }
      artifacts.push(bytes);
      console.log(`wasm SIMD: ${optimization} enabled=${simd}, vector instructions=${vectorInstruction.test(disassembly)}`);
    }
    if (optimization === "-O3") assert.notDeepEqual(artifacts[0], artifacts[1]);
  }
} finally { rmSync(root, { recursive: true, force: true }); }
