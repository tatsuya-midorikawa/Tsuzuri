import { readFileSync, writeSync } from "node:fs";

if (process.argv.length !== 4 || !/^[0-9]+$/.test(process.argv[3])) process.exit(2);
const index = Number(process.argv[3]);
const module = new WebAssembly.Module(readFileSync(process.argv[2]));
const imports = WebAssembly.Module.imports(module);
// Only a program that uses property tests imports the Debug output, for its reports.
if (imports.some(entry => entry.module !== "tsuzuri_debug" || entry.name !== "write")) throw new Error("test runner must not import host functions other than tsuzuri_debug.write");
let memory;
const host = imports.length === 0 ? {} : {
  tsuzuri_debug: {
    write(pointer, length) {
      writeSync(2, new Uint8Array(memory.buffer, Number(pointer), Number(length)));
      writeSync(2, "\n");
    },
  },
};
const instance = new WebAssembly.Instance(module, host);
memory = instance.exports.memory;
if (!Number.isSafeInteger(index) || index >= instance.exports.tsuzuri_test_count()) process.exit(2);
try {
  process.exitCode = instance.exports.tsuzuri_test_run(index);
} catch (error) {
  if (!(error instanceof WebAssembly.RuntimeError)) throw error;
  process.exitCode = 1;
}