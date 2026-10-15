import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, rmSync } from "node:fs";
import { join, resolve } from "node:path";
import { captureRun, makeRecord, writeRecords } from "./metrics.mjs";

const args = process.argv.slice(2);
const index = args.indexOf("--metrics");
let metrics;
if (index >= 0) {
  metrics = args[index+1];
  assert.ok(metrics && !metrics.startsWith("-"));
  args.splice(index,2);
}
const quick = args.includes("--quick");
const positional = args.filter((x) => x !== "--quick");
assert.ok(positional.length <= 1 && positional.every((x) => !x.startsWith("-")),
  "Usage: node benchmarks/run-utf.mjs [compiler] [--quick] [--metrics directory]");
const compiler = resolve(positional[0] ?? "target/release/tsuzuri");
const scratch = resolve("target", `utf-benchmark-${process.pid}`);
mkdirSync(scratch, { recursive: true });
const context = metrics ? captureRun(metrics, { compiler, mode: quick ? "quick" : "benchmark" }) : null;
function run(program,args) {
  const r = spawnSync(program,args,{ encoding: "utf8", env: { ...process.env, TMPDIR: scratch }, timeout: 600_000 });
  assert.ifError(r.error);
  assert.equal(r.status,0,`${program} ${args.join(" ")}\n${r.stdout}\n${r.stderr}`);
  return r.stdout;
}
try {
  const object = join(scratch,"utf.o"), executable = join(scratch,"benchmark");
  run(compiler,["build","benchmarks/utf/Kernels.tz","--emit","object","-O3","-o",object]);
  run(process.env.TSUZURI_CLANG ?? "clang",["-O3","-std=c11","-fno-lto","benchmarks/utf/host.c",object,"-o",executable]);
  const records = run(executable,[quick ? "1" : "0"]).trim().split("\n").map(JSON.parse);
  if (metrics) writeRecords(metrics,"utf",records.map((r) => makeRecord(context,{
    suite:"utf", workload:`${r.op}/${r.kind}`, size:r.size, language:"tsuzuri", variant:"default", opt:"O3",
    metric:"wall_time", samples:r.samples.map((x) => x*r.size/1e6),
  })));
  for (const { samples, ...r } of records) console.log(JSON.stringify({ ...r, quick }));
} finally {
  rmSync(scratch,{ recursive:true, force:true });
}
