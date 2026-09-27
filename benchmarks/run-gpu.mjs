import { spawnSync } from "node:child_process";
import { resolve } from "node:path";

const result = spawnSync(process.execPath, [resolve("tests/gpu.mjs"), resolve(process.argv[2] ?? "target/release/tsuzuri"), "--benchmark", ...process.argv.slice(3)], { stdio: "inherit", env: process.env });
if (result.error) throw result.error;
process.exitCode = result.status ?? 1;
