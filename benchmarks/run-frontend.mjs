import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { platform } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { captureRun, parseTimeOutput, processRecords, writeRecords } from "./metrics.mjs";
import { writeProject } from "./build/generate.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const compiler = resolve(args.shift() ?? "target/release/tsuzuri");
let out, sizes = [200, 1000], quick = false;
while (args.length) {
  const option = args.shift();
  if (option === "--out") out = resolve(args.shift());
  else if (option === "--sizes") sizes = args.shift().split(",").map(Number);
  else if (option === "--quick") quick = true;
  else throw new Error(`unknown option ${option}`);
}
assert.ok(out, "--out is required");
assert.ok(sizes.every((size) => Number.isSafeInteger(size) && size > 0 && size <= 1000));
assert.ok(["darwin", "linux"].includes(platform()), "RSS measurement requires macOS or Linux");
assert.ok(!existsSync(join(out, "frontend.jsonl")), `EEXIST: ${out}`);
mkdirSync(out, { recursive: true });
const context = captureRun(out, { compiler, mode: quick ? "quick" : "benchmark" });
const projects = [["hello", null, join(root, "examples/hello")]];
mkdirSync(join(out, "projects"), { recursive: true });
for (const size of quick ? [10] : sizes) {
  const project = join(out, "projects", `s${size}`);
  writeProject(project, size);
  projects.push([`synthetic-${size}`, size, project]);
}
const records = [];
const manifests = [];
for (const [workload, size, project] of projects) {
  for (const variant of ["check", "emit-llvm"]) {
    const output = join(out, `${workload}.ll`);
    const command = variant === "check" ? ["check", project]
      : ["build", project, "--emit", "llvm", "-O0", "--cpu", "generic", "--no-cache", "-o", output];
    const samples = { wall_ms: [], user_ms: [], sys_ms: [], peak_rss_bytes: [], stderr: [] };
    let expectedDiagnostics, expectedIR;
    for (let sample = 0; sample <= (quick ? 1 : 9); ++sample) {
      const report = join(out, `${workload}-${variant}-${sample}.time`);
      const start = process.hrtime.bigint();
      const child = spawnSync("/usr/bin/time", [platform() === "darwin" ? "-l" : "-v", "-o", report, compiler, ...command], {
        cwd: root, encoding: "utf8", timeout: 600_000, maxBuffer: 256 * 1024 * 1024,
        detached: true,
        env: { ...process.env, LC_ALL: "C", TSUZURI_CACHE_DIR: "", TSUZURI_TIME_PASSES: "1" },
      });
      const wall = Number(process.hrtime.bigint() - start) / 1e6;
      if (child.error && child.pid) {
        try { process.kill(-child.pid, "SIGKILL"); }
        catch (error) { if (error.code !== "ESRCH") throw error; }
      }
      assert.ifError(child.error);
      assert.equal(child.status, 0, `${workload}/${variant}: ${child.stdout}${child.stderr}`);
      const diagnostics = child.stdout + child.stderr.split(/\r?\n/)
        .filter((line) => !line.startsWith('{"time_passes":')).join("\n");
      expectedDiagnostics ??= diagnostics;
      assert.equal(diagnostics, expectedDiagnostics, "diagnostics changed between samples");
      if (variant === "emit-llvm") {
        const ir = readFileSync(output);
        expectedIR ??= ir;
        assert.ok(ir.equals(expectedIR), "IR changed between samples");
      }
      if (sample === 0) continue;
      const timing = parseTimeOutput(readFileSync(report, "utf8"), platform());
      samples.wall_ms.push(wall);
      samples.user_ms.push(timing.user_ms);
      samples.sys_ms.push(timing.sys_ms);
      samples.peak_rss_bytes.push(timing.peak_rss_bytes);
      samples.stderr.push(child.stderr);
    }
    records.push(...processRecords(context, {
      suite: "frontend", workload, size, language: "tsuzuri", variant,
      target: "native", opt: variant === "check" ? "none" : "O0",
    }, samples));
    manifests.push({ workload, variant, command: [compiler, ...command], cache: "disabled" });
    console.log(`${workload}/${variant}: ${samples.wall_ms.length} samples`);
  }
}
writeFileSync(join(out, "projects.json"), JSON.stringify({ context, manifests }, null, 2) + "\n");
console.log(writeRecords(out, "frontend", records));
