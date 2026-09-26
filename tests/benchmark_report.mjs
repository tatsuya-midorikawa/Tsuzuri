import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { publishSummary, renderSummary, renderTable, updateSummary } from "../benchmarks/report.mjs";

const result = {
  mode: "benchmark", cpu: "generic", scale: 0.1, measured_at: "2026-09-26T00:00:00.000Z",
  environment: { native: { control: { cpu: "Test <CPU>", architecture: "arm64", compiler_sha256: "abc" } }, csharp: { runtime: ".NET 10" }, javascript: { node: "v24" } },
  workloads: [{ family: "control", name: "example", size: 10, checks: 25, variants: Object.fromEntries([
    ["tsuzuri", 2], ["cpp", 1], ["rust", 1], ["csharp", 3], ["javascript", 4],
  ].map(([name, median_wall_ms]) => [name, { median_wall_ms }])) }],
};
const original = "# 性能測定\n\n## 表の読み方\n\n説明\n\n## 記録\n\n過去の結果\n";
const updated = updateSummary(original, [result]);
assert.ok(updated.indexOf("## ベンチマーク サマリー") > updated.indexOf("## 表の読み方"));
assert.ok(updated.indexOf("## ベンチマーク サマリー") < updated.indexOf("## 記録"));
assert.ok(updated.endsWith("## 記録\n\n過去の結果\n"));
assert.equal((updated.match(/data-fastest="true"/g) ?? []).length, 2);
assert.match(updated, /C\+\+ \/ Rust（同率）/);
assert.match(updated, /Tsuzuri 相対性能/);
assert.match(updated, /最速 = 1\.00000/);
assert.match(updated, /data-tsuzuri-performance="0\.5"/);
assert.match(updated, />\s+0\.50000<\/code>/);
assert.match(updated, /Test &lt;CPU&gt;/);
assert.match(updated, /font-variant-numeric:tabular-nums/);
assert.match(updated, /<strong><code style=/);
assert.ok(updated.indexOf("Tsuzuri (ms)") < updated.indexOf("C++ (ms)"));
assert.equal(updateSummary(updated, [result]).trim(), updated.trim());
assert.throws(() => renderSummary([{ ...result, mode: "correctness-smoke" }]));
assert.throws(() => renderSummary([result, { ...result, cpu: "native" }]));
const invalid = structuredClone(result);
invalid.workloads[0].variants.tsuzuri.median_wall_ms = NaN;
assert.throws(() => renderSummary([invalid]));
const owned = structuredClone(result);
owned.workloads[0].name = "std_option_owned";
assert.doesNotMatch(renderSummary([owned]), /data-fastest/);
assert.doesNotMatch(renderSummary([owned]), /data-tsuzuri-performance=/);
const fastest = structuredClone(result);
fastest.workloads[0].variants.tsuzuri.median_wall_ms = 0.5;
assert.match(renderSummary([fastest]), /data-tsuzuri-performance="1"/);
assert.match(renderSummary([fastest]), />\s+1\.00000<\/code>/);
fastest.workloads[0].variants.tsuzuri.median_wall_ms = 1;
assert.match(renderSummary([fastest]), /data-tsuzuri-performance="1"/);
fastest.workloads[0].variants.tsuzuri.median_wall_ms = 1000000;
assert.match(renderSummary([fastest]), /&lt;0\.00001/);
assert.doesNotMatch(renderTable(["種目", "Tsuzuri (ms)", "C++ (ms)"], [["zero", "1.000000", "0.000000"]], { timingColumns: [1, 2] }), /data-tsuzuri-performance=/);
assert.doesNotMatch(renderTable(["種目", "Tsuzuri (ms)", "C++ (ms)"], [["missing", "-", "1.000000"]], { timingColumns: [1, 2] }), /data-tsuzuri-performance=/);
const tiny = structuredClone(result);
tiny.workloads[0].variants.tsuzuri.median_wall_ms = 0.000000001;
assert.match(renderSummary([tiny]), /1\.000e-9/);
assert.match(renderTable(["名前", "時間 (ms)"], [["<script>", "0.100000"]], { timingColumns: [1] }), /&lt;script&gt;/);
const second = structuredClone(result);
second.measured_at = "2026-09-26T01:00:00.000Z";
second.workloads[0].variants.tsuzuri.median_wall_ms = 6;
assert.match(renderSummary([result, second]), /4\.000000/);
assert.match(renderSummary([result, second]), /2026-09-26T01:00:00.000Z/);
assert.throws(() => renderSummary([result, { ...result, scale: 1 }]));
assert.throws(() => updateSummary(updated.replace("<!-- benchmark-summary:end -->", ""), [result]));
const temporary = mkdtempSync(join(tmpdir(), "tsuzuri-report-test-"));
try {
  const path = join(temporary, "report.md");
  writeFileSync(path, original);
  assert.throws(() => publishSummary([invalid], path));
  assert.equal(readFileSync(path, "utf8"), original);
  publishSummary([result], path);
  assert.equal(readFileSync(path, "utf8"), updated);
  publishSummary([result], path);
  assert.equal(readFileSync(path, "utf8"), updated);
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
console.log("Benchmark report: placement, ranking/ties, units, escaping, idempotence and invalid-result guards passed");