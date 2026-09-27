import { readFileSync } from "node:fs";

if (process.argv.length !== 4 || !/^[0-9]+$/.test(process.argv[3])) process.exit(2);
const index = Number(process.argv[3]);
const module = new WebAssembly.Module(readFileSync(process.argv[2]));
if (WebAssembly.Module.imports(module).length !== 0) throw new Error("test runner must not import host functions");
const instance = new WebAssembly.Instance(module, {});
if (!Number.isSafeInteger(index) || index >= instance.exports.tsuzuri_test_count()) process.exit(2);
try {
  process.exitCode = instance.exports.tsuzuri_test_run(index);
} catch (error) {
  if (!(error instanceof WebAssembly.RuntimeError)) throw error;
  process.exitCode = 1;
}