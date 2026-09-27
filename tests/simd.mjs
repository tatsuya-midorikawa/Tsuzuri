import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
function execute(program, args, env = process.env) {
  const result = spawnSync(program, args, { encoding: "utf8", env, timeout: 300_000, maxBuffer: 16 * 1024 * 1024 });
  assert.ifError(result.error);
  assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result.stdout;
}
for (const enabled of [false, true]) {
  console.log(execute(process.execPath, ["tests/features.mjs", compiler, "simd"], { ...process.env, TSUZURI_TEST_CPU: enabled ? "native" : "generic", TSUZURI_TEST_WASM_SIMD: enabled ? "1" : "0" }).trim());
}
const root = mkdtempSync(join(tmpdir(), "tsuzuri-simd-inspect-"));
try {
  for (const enabled of [false, true]) {
    const path = join(root, `${enabled}.wasm`);
    execute(compiler, ["build", "tests/fixtures/simd", "--target", "wasm32", "-O3", ...(enabled ? ["--wasm-feature", "simd128"] : []), "-o", path]);
    const disassembly = execute(process.env.TSUZURI_OBJDUMP ?? "llvm-objdump", ["-d", path]);
    assert.equal(/\b(?:v128|i8x16|i16x8|i32x4|i64x2|f32x4|f64x2)\./.test(disassembly), enabled);
    assert.ok(WebAssembly.validate(readFileSync(path)));
  }
} finally { rmSync(root, { recursive: true, force: true }); }
