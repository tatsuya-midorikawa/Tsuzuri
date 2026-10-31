import assert from "node:assert/strict";
import { mkdirSync, readFileSync, renameSync, rmSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { replaceNamedBlock } from "../scripts/gen-docs.mjs";
import { renderSummaryBlock, serializeJson, validateSummary } from "./render.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const languages = [["tsuzuri", "Tsuzuri"], ["c", "C"], ["cpp", "C++"], ["rust", "Rust"], ["csharp", "C#"], ["javascript", "JavaScript"]];
const median = (values) => {
  const sorted = [...values].sort((left, right) => left - right);
  return (sorted[Math.floor((sorted.length - 1) / 2)] + sorted[Math.floor(sorted.length / 2)]) / 2;
};

export function summarizeResults(results) {
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
  return validateSummary({
    schema: "tz-bench-summary/1", measured_at: recorded, cpu_mode: first.cpu, scale: first.scale, runs: results.length,
    environment: { cpu: native.cpu, architecture: native.architecture, tsuzuri_sha256: native.compiler_sha256,
      csharp: first.environment.csharp.runtime, javascript: first.environment.javascript.node },
    headers: ["種目", "仕事量", ...languages.map(([, label]) => `${label} (ms)`)],
    timing_columns: languages.map((_, index) => index + 2),
    rows,
  });
}

function formatMilliseconds(value) {
  return value < 0.0000005 ? value.toExponential(3) : value.toFixed(6);
}

export function publishSummary(results, { document = join(root, "docs/benchmarks.md"), data = join(root, "docs/benchmarks-data/summary.json") } = {}) {
  const summary = summarizeResults(results);
  const updated = replaceNamedBlock(readFileSync(document, "utf8"), "benchmark-summary", renderSummaryBlock(summary), document);
  mkdirSync(dirname(data), { recursive: true });
  writeAtomically(data, serializeJson(summary));
  writeAtomically(document, updated);
}

export function writeAtomically(path, text) {
  const temporary = `${path}.${process.pid}.tmp`;
  let created = false;
  try {
    writeFileSync(temporary, text, { flag: "wx" });
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
