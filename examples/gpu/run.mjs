import { readFile } from "node:fs/promises";
import { createWebGpu } from "../../src/runtime/webgpu.mjs";

if (!process.argv[2]) throw new Error("usage: node examples/gpu/run.mjs kernel.wgsl");
const binding = await import(process.env.TSUZURI_WEBGPU_MODULE ?? new URL("../../target/webgpu-runtime/node_modules/webgpu/index.js", import.meta.url).href);
Object.assign(globalThis, binding.globals);
let provider = binding.create(process.platform === "darwin" ? ["backend=metal"] : []);
let runtime = await createWebGpu(provider);
try {
  const program = await runtime.prepare(await readFile(process.argv[2], "utf8"));
  console.log(JSON.stringify(Array.from(await runtime.toArray(await runtime.init(program, 16)))));
} finally {
  await runtime.close();
  runtime = undefined;
  provider = undefined;
}
