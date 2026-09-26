import assert from "node:assert/strict";
import { readFileSync, renameSync, rmSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const startMarker = "<!-- benchmark-summary:start -->";
const endMarker = "<!-- benchmark-summary:end -->";
const languages = [["tsuzuri", "Tsuzuri"], ["c", "C"], ["cpp", "C++"], ["rust", "Rust"], ["csharp", "C#"], ["javascript", "JavaScript"]];
const escape = (value) => String(value).replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;").replaceAll('"', "&quot;");
const median = (values) => {
  const sorted = [...values].sort((left, right) => left - right);
  return (sorted[Math.floor((sorted.length - 1) / 2)] + sorted[Math.floor(sorted.length / 2)]) / 2;
};
const numberStyle = "font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums;white-space:pre;letter-spacing:0";
const cellStyle = "padding:8px 12px;border:1px solid #94a3b8";

export function renderTable(headers, rows, { caption, timingColumns = [], unrankedRows = new Set() } = {}) {
  const width = Math.max(1, ...rows.flatMap((row) => timingColumns.map((index) => String(row[index]).length)));
  const tsuzuriColumn = timingColumns.find((index) => headers[index].startsWith("Tsuzuri"));
  const table = [
    '<div style="overflow-x:auto;max-width:100%">',
    '<table style="border-collapse:collapse;width:100%;font-size:13px">',
    `<caption style="text-align:left;padding:8px 0;font-weight:600">${escape(caption ?? "処理時間: 小さいほど高速")}</caption>`,
    "<thead><tr>",
    ...headers.map((header) => `<th scope="col" style="${cellStyle};white-space:nowrap">${escape(header)}</th>`),
    timingColumns.length ? `<th scope="col" style="${cellStyle};white-space:nowrap">観測最速</th>` : "",
    tsuzuriColumn !== undefined ? `<th scope="col" style="${cellStyle};white-space:nowrap" title="この行の最速時間 / Tsuzuriの処理時間。大きいほど高速です">Tsuzuri 相対性能<br><small>最速 = 1.00000</small></th>` : "",
    "</tr></thead>",
    "<tbody>",
  ];
  for (const [rowIndex, row] of rows.entries()) {
    assert.equal(row.length, headers.length, "Table column count mismatch");
    const candidates = unrankedRows.has(rowIndex) ? [] : timingColumns.filter((index) => row[index] !== "-" && Number.isFinite(Number(row[index])));
    const minimum = Math.min(...candidates.map((index) => Number(row[index])));
    const winners = candidates.filter((index) => Number(row[index]) === minimum);
    table.push("<tr>");
    for (const [index, value] of row.entries()) {
      const numeric = timingColumns.includes(index) || /^\d+(?:\.\d+)?$/.test(String(value));
      const winner = winners.includes(index);
      const tag = index === 0 ? "th" : "td";
      const emphasis = winner ? ";background-color:#dcfce7;color:#14532d;font-weight:700" : "";
      const formatted = numeric ? `<code style="${numberStyle};background:transparent;color:inherit">${escape(String(value).padStart(timingColumns.includes(index) ? width : 0))}</code>` : escape(value);
      const contents = winner ? `<strong>${formatted}</strong>` : formatted;
      table.push(`<${tag}${index === 0 ? ' scope="row"' : ""} style="${cellStyle};text-align:${numeric ? "right" : "left"}${emphasis}"${winner ? ' title="この行の観測最小値" data-fastest="true"' : ""}>${contents}</${tag}>`);
    }
    if (timingColumns.length) {
      const label = winners.length ? winners.map((index) => headers[index].replace(/ \([^)]*\)$/, "")).join(" / ") : "条件差あり";
      table.push(`<td style="${cellStyle};white-space:nowrap">${escape(label)}${winners.length > 1 ? "（同率）" : ""}</td>`);
    }
    if (tsuzuriColumn !== undefined) {
      const comparable = candidates.includes(tsuzuriColumn) && minimum > 0 && Number.isFinite(minimum);
      const performance = comparable ? minimum / Number(row[tsuzuriColumn]) : null;
      const label = performance === null ? "-" : performance < 0.00001 ? "<0.00001" : performance.toFixed(5);
      table.push(`<td style="${cellStyle};text-align:right"${comparable ? ` data-tsuzuri-performance="${performance}" title="最速時間 / Tsuzuri時間 = ${performance}"` : ' title="比較条件または測定値のため計算対象外"'}><code style="${numberStyle};background:transparent;color:inherit">${escape(label.padStart(8))}</code></td>`);
    }
    table.push("</tr>");
  }
  table.push("</tbody></table>", "</div>");
  return table.filter(Boolean).join("\n");
}

export function renderSummary(results) {
  assert.ok(results.length > 0, "At least one benchmark result is required");
  const first = results[0];
  const signature = (result) => JSON.stringify({ cpu: result.cpu, scale: result.scale, environment: result.environment, native_options: result.native_options,
    workloads: result.workloads.map((work) => [work.family, work.name, work.size, Object.keys(work.variants).sort()]) });
  for (const result of results) {
    assert.equal(result.mode, "benchmark", "Quick/smoke timings must not be published");
    assert.ok(["generic", "native"].includes(result.cpu));
    assert.ok(Number.isFinite(result.scale) && result.scale > 0);
    assert.ok(result.workloads.length > 0);
    assert.equal(signature(result), signature(first), "Do not combine different compilers, environments, CPU modes, sizes or workloads");
    const keys = result.workloads.map((work) => `${work.family}/${work.name}`);
    assert.equal(new Set(keys).size, keys.length, "Duplicate workload");
    for (const work of result.workloads) {
      assert.ok(Number.isSafeInteger(work.size) && work.size >= 0);
      assert.ok(Number.isInteger(work.checks) && work.checks > 0);
      for (const language of ["tsuzuri", "cpp", "rust", "csharp", "javascript"]) assert.ok(work.variants[language], `Missing ${language} result`);
      for (const [language] of languages) if (work.variants[language]) {
        assert.ok(Number.isFinite(work.variants[language].median_wall_ms) && work.variants[language].median_wall_ms > 0, "Timings must be finite and positive");
      }
    }
  }
  const rows = first.workloads.map((work, index) => [
    `${work.family}/${work.name}`, work.family === "cpp" && work.name === "mandelbrot" ? `${work.size} x ${work.size}` : String(work.size),
    ...languages.map(([language]) => work.variants[language]
      ? formatMilliseconds(median(results.map((result) => result.workloads[index].variants[language].median_wall_ms))) : "-"),
  ]);
  const native = first.environment.native.control;
  const recorded = results.map((result) => result.measured_at).filter(Boolean).sort().at(-1) ?? "未記録";
  const table = renderTable(["種目", "仕事量", ...languages.map(([, label]) => `${label} (ms)`)], rows, {
    caption: `${first.workloads.length}種目 / ${first.cpu} / scale ${first.scale} / ${results.length}回の中央値`,
    timingColumns: languages.map((_, index) => index + 2),
    unrankedRows: new Set(first.workloads.flatMap((work, index) => work.name === "std_option_owned" ? [index] : [])),
  });
  return [
    "## ベンチマーク サマリー", "", startMarker,
    `<p><strong>処理時間は小さいほど高速です。</strong> 緑色・太字は各行の表示値で最速の実装です。同じ表示値は同率扱いです。僅差は一般的な優位性を示しません。</p>`,
    "<p><strong>Tsuzuri 相対性能は大きいほど良好です。</strong> 最速を <code>1.00000</code> とし、最速の処理時間を Tsuzuri の処理時間で割ります。<code>0.50000</code> は最速の半分の性能（処理時間は2倍）、<code>0.80000</code> は最速の80%の性能です。小数第5位に丸めるため、ごく僅かな差は <code>1.00000</code> になります。比較できない行は <code>-</code> です。</p>",
    `<p>最終測定: <code>${escape(recorded)}</code><br>CPU: ${escape(native.cpu)} / ${escape(native.architecture)} / ${escape(first.cpu)}<br>Tsuzuri: <code style="overflow-wrap:anywhere">${escape(native.compiler_sha256)}</code><br>C#: ${escape(first.environment.csharp.runtime)} / JavaScript: ${escape(first.environment.javascript.node)}</p>`,
    table,
    "<p><code>-</code> は未測定です。すべて wall time（ms/呼び出し）で、起動時間は含みません。C#/JavaScript は別プロセス、管理メモリの回収条件は言語ごとに異なります。<code>std_option_owned</code> は確保条件が異なるため順位を付けません。詳細な比較条件は後述します。</p>",
    endMarker, "",
  ].join("\n");
}

function formatMilliseconds(value) {
  return value < 0.0000005 ? value.toExponential(3) : value.toFixed(6);
}

export function updateSummary(document, results) {
  const summary = renderSummary(results);
  const heading = "## ベンチマーク サマリー";
  if (document.includes(startMarker) || document.includes(endMarker)) {
    assert.equal(document.split(startMarker).length, 2, "Ambiguous summary start marker");
    assert.equal(document.split(endMarker).length, 2, "Ambiguous summary end marker");
    const start = document.indexOf(heading);
    const end = document.indexOf(endMarker);
    assert.ok(start >= 0 && start < document.indexOf(startMarker) && end > document.indexOf(startMarker));
    return document.slice(0, start) + summary.trimEnd() + document.slice(end + endMarker.length);
  }
  assert.ok(!document.includes(heading), "Add generation markers before replacing an existing summary");
  const reading = document.indexOf("## 表の読み方\n");
  const following = document.indexOf("\n## ", reading + 1);
  assert.ok(reading >= 0 && following > reading, "Cannot locate summary insertion point");
  return document.slice(0, following) + "\n" + summary + document.slice(following);
}

export function publishSummary(results, path = join(root, "docs/benchmarks.md")) {
  const updated = updateSummary(readFileSync(path, "utf8"), results);
  const temporary = `${path}.${process.pid}.tmp`;
  let created = false;
  try {
    writeFileSync(temporary, updated, { flag: "wx" });
    created = true;
    renameSync(temporary, path);
  } finally {
    if (created) rmSync(temporary, { force: true });
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const paths = process.argv.slice(2);
  assert.ok(paths.length > 0, "Usage: node benchmarks/report.mjs result.json [same-condition-result.json ...]");
  publishSummary(paths.map((path) => {
    const result = JSON.parse(readFileSync(path, "utf8"));
    return { ...result, measured_at: result.measured_at ?? statSync(path).mtime.toISOString() };
  }));
  console.log("Benchmark summary updated");
}