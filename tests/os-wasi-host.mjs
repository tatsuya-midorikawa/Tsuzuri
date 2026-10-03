// Runs a WASI command module for tests/os.mjs: node os-wasi-host.mjs <module.wasm> <preopen name> <preopen directory> [arguments...]
// The environment is taken from TZ_WASI_ENV (a JSON object), and standard input, output, and error are the process's own.
import { readFileSync } from "node:fs";
import { WASI } from "node:wasi";

const [module, name, directory, ...args] = process.argv.slice(2);
const wasi = new WASI({
  version: "preview1",
  args: [module, ...args],
  env: JSON.parse(process.env.TZ_WASI_ENV ?? "{}"),
  preopens: { [name]: directory },
});
const compiled = await WebAssembly.compile(readFileSync(module));
const instance = await WebAssembly.instantiate(compiled, wasi.getImportObject());
process.exitCode = wasi.start(instance) ?? 0;
