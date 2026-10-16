import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { arch, cpus, platform, release, tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { escape, renderTable, writeAtomically } from "./report.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
export const SCHEMA = 1;
export const METRICS = Object.freeze({
  wall_time: "ms", cpu_time: "ms", user_time: "ms", sys_time: "ms", phase_time: "ms",
  peak_rss: "bytes", alloc_calls: "count", alloc_bytes: "bytes",
  executable_bytes: "bytes", object_bytes: "bytes", wasm_bytes: "bytes",
  text_bytes: "bytes", rodata_bytes: "bytes", wasm_code_bytes: "bytes", wasm_data_bytes: "bytes",
});
export const LANGUAGES = Object.freeze(["tsuzuri", "c", "cpp", "rust", "zig", "go", "csharp", "javascript"]);
const labels = ["Tsuzuri", "C", "C++", "Rust", "Zig", "Go", "C#", "JavaScript"];
const fields = ["schema", "run_id", "commit", "dirty", "date", "mode", "host", "suite", "workload", "size",
  "language", "variant", "target", "opt", "cpu_mode", "metric", "phase", "unit", "samples", "median", "min", "max"];
const hostFields = ["os", "os_version", "arch", "cpu", "cores", "clang", "rustc", "node", "tsuzuri_sha256"];
const keyFields = ["suite", "workload", "size", "language", "variant", "target", "opt", "cpu_mode", "metric", "phase"];
const phasePattern = /^(load|parse|check|specialize|closures|ownership|emit|cache|total|tool\.[^\u0000]+)$/;
const key = (record) => JSON.stringify(keyFields.map((field) => record[field]));
const nonempty = (value) => typeof value === "string" && value.length > 0;
const hash = (file) => createHash("sha256").update(readFileSync(file)).digest("hex");
const execute = (command, args) => execFileSync(command, args, { cwd: root, encoding: "utf8", timeout: 600_000 });

export function artifactFingerprint(file) {
  const bytes = readFileSync(file);
  // Apple's linker randomizes the executable UUID and its derived signature, even before PX01.
  // All remaining bytes, including code, data, load commands and symbol tables, are compared.
  if (bytes.length >= 32 && bytes.readUInt32LE(0) === 0xfeedfacf && bytes.readUInt32LE(12) === 2) {
    const commands = bytes.readUInt32LE(16), end = 32 + bytes.readUInt32LE(20);
    assert.ok(commands <= 4096 && end <= bytes.length, "invalid Mach-O load commands");
    let offset = 32;
    for (let index = 0; index < commands; ++index) {
      assert.ok(offset + 8 <= end, "truncated Mach-O load command");
      const command = bytes.readUInt32LE(offset), size = bytes.readUInt32LE(offset + 4);
      assert.ok(size >= 8 && offset + size <= end, "invalid Mach-O load command size");
      if (command === 0x1b) {
        assert.equal(size, 24, "invalid LC_UUID");
        bytes.fill(0, offset + 8, offset + size);
      } else if (command === 0x1d) {
        assert.equal(size, 16, "invalid LC_CODE_SIGNATURE");
        const start = bytes.readUInt32LE(offset + 8), length = bytes.readUInt32LE(offset + 12);
        assert.ok(start >= end && start + length <= bytes.length, "invalid Mach-O code signature");
        bytes.fill(0, start, start + length);
      }
      offset += size;
    }
    assert.equal(offset, end, "inconsistent Mach-O load commands");
  } else if (bytes.length >= 20 && [0x14c, 0x8664, 0xaa64].includes(bytes.readUInt16LE(0))
    && bytes.readUInt16LE(16) === 0) {
    const sections = bytes.readUInt16LE(2);
    assert.ok(sections > 0 && 20 + sections * 40 <= bytes.length, "invalid COFF section headers");
    // Native Windows Clang records wall time in the standard COFF object's TimeDateStamp.
    bytes.fill(0, 4, 8);
  }
  return createHash("sha256").update(bytes).digest("hex");
}

export function median(values) {
  assert.ok(Array.isArray(values) && values.length > 0 && values.every((value) => Number.isFinite(value) && value >= 0),
    "samples must be nonempty, finite and nonnegative");
  const sorted = [...values].sort((left, right) => left - right);
  return (sorted[Math.floor((sorted.length - 1) / 2)] + sorted[Math.floor(sorted.length / 2)]) / 2;
}

export function summarize(samples) {
  const middle = median(samples);
  let min = samples[0], max = min;
  for (const value of samples) {
    min = Math.min(min, value);
    max = Math.max(max, value);
  }
  return { samples: [...samples], median: middle, min, max };
}

function version(command, args) {
  const child = spawnSync(command, args, { cwd: root, encoding: "utf8", timeout: 30_000 });
  if (child.error?.code === "ENOENT") {
    console.error(`metadata ${command}: not found`);
    return null;
  }
  if (child.error) throw child.error;
  assert.equal(child.status, 0, `${command}: ${child.stderr || child.stdout}`);
  assert.ok(nonempty(child.stdout.trim()), `${command}: empty version`);
  return child.stdout.trim().split(/\r?\n/)[0];
}

export function captureRun(directory, { compiler, mode, clang = process.env.TSUZURI_CLANG ?? "clang",
  rustc = process.env.TSUZURI_RUSTC ?? process.env.RUSTC ?? "rustc" }) {
  const run_id = basename(resolve(directory));
  assert.match(run_id, /^[A-Za-z0-9._-]{1,80}$/, "invalid run_id");
  assert.ok(["benchmark", "quick"].includes(mode));
  const processors = cpus();
  assert.ok(processors.length > 0, "CPU information unavailable");
  return {
    schema: SCHEMA, run_id, commit: execute("git", ["rev-parse", "HEAD"]).trim(),
    dirty: execute("git", ["status", "--porcelain"]).trim().length > 0,
    date: new Date().toISOString(), mode,
    host: {
      os: platform(), os_version: platform() === "darwin" ? execute("sw_vers", ["-productVersion"]).trim() : release(),
      arch: arch(), cpu: processors[0].model, cores: processors.length,
      clang: version(clang, ["--version"]), rustc: version(rustc, ["--version"]), node: process.version,
      tsuzuri_sha256: hash(resolve(compiler)),
    },
  };
}

export function makeRecord(context, { suite, workload, size = null, language, variant = "default",
  target = "native", opt, cpu_mode = "generic", metric, phase = null, samples }) {
  const record = {
    schema: context.schema, run_id: context.run_id, commit: context.commit, dirty: context.dirty,
    date: context.date, mode: context.mode, host: context.host, suite, workload, size, language, variant,
    target, opt, cpu_mode, metric, phase, unit: METRICS[metric], ...summarize(samples),
  };
  validateRecord(record);
  return record;
}

export function validateRecord(record) {
  assert.ok(record && typeof record === "object" && !Array.isArray(record), "record must be an object");
  assert.deepEqual(Object.keys(record).sort(), [...fields].sort(), "unexpected record fields");
  assert.equal(record.schema, SCHEMA);
  assert.match(record.run_id, /^[A-Za-z0-9._-]{1,80}$/);
  assert.match(record.commit, /^[0-9a-f]{40}$/);
  assert.equal(typeof record.dirty, "boolean");
  assert.ok(nonempty(record.date));
  assert.equal(new Date(record.date).toISOString(), record.date);
  assert.ok(["benchmark", "quick"].includes(record.mode));
  assert.ok(record.host && typeof record.host === "object" && !Array.isArray(record.host));
  assert.deepEqual(Object.keys(record.host).sort(), [...hostFields].sort(), "unexpected host fields");
  for (const field of ["os", "os_version", "arch", "cpu", "node"]) assert.ok(nonempty(record.host[field]), `invalid host.${field}`);
  assert.ok(Number.isSafeInteger(record.host.cores) && record.host.cores > 0);
  for (const field of ["clang", "rustc"]) assert.ok(record.host[field] === null || nonempty(record.host[field]));
  assert.match(record.host.tsuzuri_sha256, /^[0-9a-f]{64}$/);
  assert.match(record.suite, /^[a-z0-9-]+$/);
  assert.ok(nonempty(record.workload));
  assert.ok(record.size === null || Number.isSafeInteger(record.size) && record.size >= 0);
  assert.ok(LANGUAGES.includes(record.language));
  assert.match(record.variant, /^[a-z0-9-]+$/);
  assert.ok(["native", "wasm32"].includes(record.target));
  assert.ok(["O0", "O3", "Os", "Oz", "none"].includes(record.opt));
  assert.ok(["generic", "native"].includes(record.cpu_mode));
  assert.ok(Object.hasOwn(METRICS, record.metric));
  assert.equal(record.unit, METRICS[record.metric]);
  if (record.metric === "phase_time") assert.match(record.phase, phasePattern);
  else assert.equal(record.phase, null);
  const summary = summarize(record.samples);
  if (record.unit !== "ms") assert.ok(record.samples.every(Number.isSafeInteger), "bytes/count must be safe integers");
  const deterministic = record.metric.startsWith("alloc_") || record.metric.endsWith("_bytes");
  if (deterministic) assert.ok(record.samples.every((sample) => sample === record.samples[0]), "nondeterministic metric");
  else if (record.mode === "benchmark") assert.ok(record.samples.length >= 9, "benchmark requires at least 9 samples");
  for (const field of ["median", "min", "max"]) assert.equal(record[field], summary[field], `incorrect ${field}`);
  return record;
}

export function writeRecords(directory, suite, records) {
  assert.match(suite, /^[a-z0-9-]+$/);
  assert.ok(records.length > 0, "no records to write");
  const seen = new Set();
  for (const record of records) {
    validateRecord(record);
    assert.equal(record.suite, suite);
    assert.equal(record.run_id, basename(resolve(directory)));
    assert.ok(!seen.has(key(record)), "duplicate record key");
    seen.add(key(record));
  }
  mkdirSync(directory, { recursive: true });
  const file = join(directory, `${suite}.jsonl`);
  writeFileSync(file, records.map((record) => JSON.stringify(record)).join("\n") + "\n", { flag: "wx" });
  return file;
}

export function readRecords(directory) {
  const records = [], seen = new Set(), runs = new Set();
  for (const name of readdirSync(directory).filter((name) => name.endsWith(".jsonl")).sort()) {
    for (const line of readFileSync(join(directory, name), "utf8").split(/\r?\n/).filter((line) => line.trim())) {
      const record = validateRecord(JSON.parse(line));
      assert.equal(name, `${record.suite}.jsonl`, "suite does not match filename");
      assert.ok(!seen.has(key(record)), "duplicate record key");
      seen.add(key(record));
      runs.add(record.run_id);
      records.push(record);
    }
  }
  assert.ok(records.length > 0, "no metric records found");
  assert.equal(runs.size, 1, "mixed run_id values");
  return records;
}

export function parseTimeOutput(text, system) {
  let wall, user, sys, rss;
  if (system === "darwin") {
    const times = text.match(/^\s*(\d+(?:\.\d+)?) real\s+(\d+(?:\.\d+)?) user\s+(\d+(?:\.\d+)?) sys\s*$/m);
    if (times) [, wall, user, sys] = times;
    rss = text.match(/^\s*(\d+)\s+maximum resident set size\s*$/m)?.[1];
    wall = wall === undefined ? undefined : Number(wall) * 1000;
  } else if (system === "linux") {
    user = text.match(/User time \(seconds\): (\d+(?:\.\d+)?)/)?.[1];
    sys = text.match(/System time \(seconds\): (\d+(?:\.\d+)?)/)?.[1];
    const elapsed = text.match(/Elapsed \(wall clock\) time \(h:mm:ss or m:ss\): (\d+(?::\d+){1,2}(?:\.\d+)?)/)?.[1];
    if (elapsed) wall = elapsed.split(":").reduce((seconds, part) => seconds * 60 + Number(part), 0) * 1000;
    rss = text.match(/Maximum resident set size \(kbytes\): (\d+)/)?.[1];
    if (rss !== undefined) rss = Number(rss) * 1024;
  } else {
    throw new Error(`peak RSS measurement supports macOS and Linux, not ${system}`);
  }
  assert.ok([wall, user, sys, rss].every((value) => value !== undefined && Number.isFinite(Number(value)) && Number(value) >= 0),
    "unrecognized /usr/bin/time output");
  assert.ok(Number.isSafeInteger(Number(rss)), "invalid peak RSS");
  return { wall_ms: Math.round(wall), user_ms: Math.round(Number(user) * 1000),
    sys_ms: Math.round(Number(sys) * 1000), peak_rss_bytes: Number(rss) };
}

export function parseTimePasses(stderr) {
  const line = stderr.split(/\r?\n/).filter((line) => line.startsWith('{"time_passes":')).at(-1);
  assert.ok(line, "missing TSUZURI_TIME_PASSES report");
  const report = JSON.parse(line);
  assert.equal(report.time_passes, 1);
  assert.ok(Array.isArray(report.phases));
  assert.ok(Number.isFinite(report.total_ms) && report.total_ms >= 0);
  const seen = new Set();
  let sum = 0;
  for (const phase of report.phases) {
    assert.match(phase.name, phasePattern);
    assert.notEqual(phase.name, "total");
    assert.ok(!seen.has(phase.name), "duplicate timing phase");
    assert.ok(Number.isFinite(phase.ms) && phase.ms >= 0);
    seen.add(phase.name);
    sum += phase.ms;
  }
  assert.ok(sum <= report.total_ms + 0.0005 * (report.phases.length + 1), "overlapping timing phases");
  return { phases: report.phases, total_ms: report.total_ms };
}

export function validateGnuTimeVersion(text) {
  const match = text.match(/\bGNU time\)?[ \t]+(?:(\d+)\.(\d+)(?:\.\d+)?|UNKNOWN)(?=\s|$)/i);
  assert.ok(match && (match[1] === undefined || Number(match[1]) > 1 || Number(match[1]) === 1 && Number(match[2]) >= 8),
    `install GNU time 1.8+ at /usr/bin/time; unrecognized or unsupported version:\n${text}`);
}

let gnuTimeChecked = false;
export function measureProcess(command, args, { cwd = root, env = process.env, runs = 9, warmups = 1,
  timeoutMs = 600_000, system = platform() } = {}) {
  const flag = { darwin: "-l", linux: "-v" }[system];
  if (!flag) throw new Error(`peak RSS measurement supports macOS and Linux, not ${system}`);
  assert.ok(Number.isSafeInteger(runs) && runs > 0 && Number.isSafeInteger(warmups) && warmups >= 0);
  assert.ok(Number.isSafeInteger(timeoutMs) && timeoutMs > 0);
  if (system === "linux" && !gnuTimeChecked) {
    const version = spawnSync("/usr/bin/time", ["--version"], { env: { ...env, LC_ALL: "C" }, encoding: "utf8", timeout: 10_000 });
    if (version.error) throw new Error(`cannot execute /usr/bin/time; install GNU time 1.8+: ${version.error.message}`);
    assert.equal(version.status, 0, `cannot query /usr/bin/time:\n${version.stdout}${version.stderr}`);
    validateGnuTimeVersion(`${version.stdout}\n${version.stderr}`);
    gnuTimeChecked = true;
  }
  const temporary = mkdtempSync(join(tmpdir(), "tsuzuri-metrics-"));
  const result = { wall_ms: [], user_ms: [], sys_ms: [], peak_rss_bytes: [], stdout: [], stderr: [] };
  try {
    for (let index = 0; index < warmups + runs; ++index) {
      const report = join(temporary, `time-${index}.txt`);
      const start = process.hrtime.bigint();
      const child = spawnSync("/usr/bin/time", [flag, "-o", report, command, ...args], {
        cwd, env: { ...env, LC_ALL: "C" }, encoding: "utf8", timeout: timeoutMs, killSignal: "SIGKILL",
        maxBuffer: 256 * 1024 * 1024, detached: true,
      });
      const wall = Number(process.hrtime.bigint() - start) / 1e6;
      if (child.error) {
        // time waits for a child, so killing only time on timeout would orphan the compiler/tools.
        if (child.pid) {
          try {
            process.kill(-child.pid, "SIGKILL");
          } catch (error) {
            if (error.code !== "ESRCH") throw new AggregateError([child.error, error], `cannot terminate measured process group ${child.pid}`);
          }
        }
        throw new Error(`cannot measure ${command} ${args.join(" ")}: ${child.error.message}`, { cause: child.error });
      }
      if (child.status !== 0) {
        throw Object.assign(new Error(`${command} exited with ${child.status ?? child.signal}:\n${child.stdout}${child.stderr}`),
          { status: child.status, signal: child.signal, stdout: child.stdout, stderr: child.stderr });
      }
      if (index < warmups) continue;
      const parsed = parseTimeOutput(readFileSync(report, "utf8"), system);
      result.wall_ms.push(wall);
      result.user_ms.push(parsed.user_ms);
      result.sys_ms.push(parsed.sys_ms);
      result.peak_rss_bytes.push(parsed.peak_rss_bytes);
      result.stdout.push(child.stdout);
      result.stderr.push(child.stderr);
    }
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
  return result;
}

export function instrumentAllocations(ir) {
  assert.doesNotMatch(ir, /@(aligned_alloc|posix_memalign|strdup|strndup|valloc|memalign)\b/, "unsupported allocator in tracked IR");
  return ir.replace(/@(malloc|calloc|realloc|free)\b/g, "@tracked_$1")
    .replace(/^attributes #\d+ = .*\n/gm, "").replace(/ #\d+\b/g, "");
}

export function unsupportedReason(stderr) {
  const line = stderr.split(/\r?\n/).find((line) => /error\[E2004\]/.test(line));
  assert.ok(line, "missing expected E2004 diagnostic");
  return line;
}

export function parseCompilerDiagnostics(stderr) {
  const diagnostics = [];
  for (const line of stderr.split(/\r?\n/).filter((line) => line.trim())) {
    const value = JSON.parse(line);
    if (Object.hasOwn(value, "time_passes")) {
      parseTimePasses(line);
    } else {
      assert.ok(typeof value.code === "string" && typeof value.message === "string", "unrecognized compiler JSON");
      diagnostics.push(value);
    }
  }
  return diagnostics;
}

export function inProcessRecords(context, suite, workloads, variants, { cpu = "generic", opt = "O3" } = {}) {
  return workloads.flatMap((work) => variants.flatMap(([label, language, variant = "default"]) =>
    [["wall_time", `${label}_wall_ms`], ["cpu_time", `${label}_ms`]].map(([metric, column]) =>
      makeRecord(context, { suite, workload: work.name, size: work.size, language, variant, opt, cpu_mode: cpu,
        metric, samples: work.raw.map((sample) => sample[column]) }))));
}

export function processRecords(context, fields, measured, { phases = true } = {}) {
  const samples = {
    wall_time: measured.wall_ms, cpu_time: measured.user_ms.map((value, index) => value + measured.sys_ms[index]),
    user_time: measured.user_ms, sys_time: measured.sys_ms, peak_rss: measured.peak_rss_bytes,
  };
  const records = Object.entries(samples).map(([metric, samples]) => makeRecord(context, { ...fields, metric, samples }));
  if (phases) {
    const reports = measured.stderr.map(parseTimePasses);
    const names = reports[0].phases.map((phase) => phase.name);
    for (const report of reports) assert.deepEqual(report.phases.map((phase) => phase.name), names, "timing phases changed between samples");
    for (const [index, name] of names.entries()) records.push(makeRecord(context, {
      ...fields, metric: "phase_time", phase: name, samples: reports.map((report) => report.phases[index].ms),
    }));
    records.push(makeRecord(context, { ...fields, metric: "phase_time", phase: "total", samples: reports.map((report) => report.total_ms) }));
  }
  return records;
}

export function compareRuns(baseline, current) {
  for (const records of [baseline, current]) {
    records.forEach(validateRecord);
    assert.equal(new Set(records.map(key)).size, records.length, "duplicate record key");
  }
  const before = new Map(baseline.map((record) => [key(record), record]));
  const after = new Map(current.map((record) => [key(record), record]));
  const stats = (record) => record ? { median: record.median, min: record.min, max: record.max } : null;
  return [...new Set([...before.keys(), ...after.keys()])].sort().map((id) => {
    const left = before.get(id), right = after.get(id), record = right ?? left;
    const spread = left && right ? Math.max(left.max - left.min, right.max - right.min) : null;
    const difference = left && right ? right.median - left.median : null;
    const conditions = left && right && (left.mode !== right.mode ||
      hostFields.filter((field) => field !== "tsuzuri_sha256").some((field) => left.host[field] !== right.host[field]));
    return {
      ...Object.fromEntries(keyFields.map((field) => [field, record[field]])), unit: record.unit,
      before: stats(left), after: stats(right), ratio: left && right && left.median > 0 ? right.median / left.median : null,
      spread, verdict: !left || !right ? "片側のみ" : conditions ? "条件差" : Math.abs(difference) <= spread ? "差なし" : difference < 0 ? "改善" : "悪化",
      dirty: Boolean(left?.dirty || right?.dirty), quick: record.mode === "quick",
    };
  });
}

const markdown = (value) => String(value).replaceAll("|", "\\|").replace(/[\r\n]/g, " ");
export function renderComparison(rows) {
  const headings = ["種目", "言語", "variant", "target", "opt", "指標", "before 中央値", "after 中央値", "after/before", "広がり", "判定"];
  const lines = [];
  if (rows.some((row) => row.dirty)) lines.push("注意: 未コミットの変更を含む run です。", "");
  if (rows.some((row) => row.quick)) lines.push("注意: quick の値は性能の証拠ではありません。", "");
  lines.push(`| ${headings.join(" | ")} |`, `| ${headings.map(() => "---").join(" | ")} |`);
  const value = (number, unit) => number === null || number === undefined ? "-" : unit === "ms" ? number.toFixed(3) : String(number);
  for (const row of rows) {
    lines.push(`| ${[`${row.suite}/${row.workload}${row.size === null ? "" : ` (${row.size})`}`, row.language, row.variant,
      row.target, row.opt, `${row.metric}${row.phase ? ` [${row.phase}]` : ""} (${row.unit})`,
      value(row.before?.median, row.unit), value(row.after?.median, row.unit), row.ratio === null ? "-" : row.ratio.toFixed(3),
      value(row.spread, row.unit), row.verdict].map(markdown).join(" | ")} |`);
  }
  return lines.join("\n");
}

export function renderMetricsSummary(records) {
  assert.ok(records.length > 0);
  records.forEach(validateRecord);
  const first = records[0], host = first.host;
  for (const record of records) {
    assert.equal(record.mode, "benchmark", "Quick/smoke timings must not be published");
    for (const field of ["run_id", "commit", "dirty"]) assert.equal(record[field], first[field], `mixed ${field}`);
    assert.deepEqual(record.host, host, "mixed host conditions");
  }
  assert.equal(new Set(records.map(key)).size, records.length, "duplicate record key");
  const date = records.map((record) => record.date).sort()[0];
  const lines = [`<p>run: <code>${escape(first.run_id)}</code> / ${escape(date)} / commit: <code>${escape(first.commit.slice(0, 12))}</code>${first.dirty ? "（未コミットの変更を含む）" : ""}<br>CPU: ${escape(host.cpu)} / ${escape(host.arch)} / ${host.cores} cores / ${escape(host.os)} ${escape(host.os_version)}<br>Clang: ${escape(host.clang ?? "未記録")} / rustc: ${escape(host.rustc ?? "未記録")} / Node: ${escape(host.node)}<br>Tsuzuri SHA-256: <code>${escape(host.tsuzuri_sha256)}</code></p>`];
  const groups = new Map();
  for (const record of records) {
    const id = JSON.stringify([record.metric, record.target, record.opt, record.cpu_mode]);
    if (!groups.has(id)) groups.set(id, []);
    groups.get(id).push(record);
  }
  for (const group of groups.values()) {
    const columns = [...new Set(group.map((record) => JSON.stringify([record.language, record.variant])))].map(JSON.parse)
      .sort(([left], [right]) => LANGUAGES.indexOf(left) - LANGUAGES.indexOf(right));
    const rows = new Map();
    for (const record of group) {
      const id = JSON.stringify([record.suite, record.workload, record.size, record.phase]);
      if (!rows.has(id)) rows.set(id, [
        `${record.suite}/${record.workload}${record.phase ? ` [${record.phase}]` : ""}`,
        record.size ?? "-", ...columns.map(() => "-"),
      ]);
      const column = columns.findIndex(([language, variant]) => record.language === language && record.variant === variant);
      rows.get(id)[column + 2] = record.unit === "ms" ? record.median.toFixed(3) : String(record.median);
    }
    const { metric, unit, target, opt, cpu_mode } = group[0];
    lines.push(renderTable(["種目", "仕事量", ...columns.map(([language, variant]) =>
      `${labels[LANGUAGES.indexOf(language)]}${variant === "default" ? "" : ` ${variant}`}`)], [...rows.values()],
    { caption: `${metric} (${unit}) / ${target} / ${opt} / ${cpu_mode} / 中央値` }));
  }
  return lines.join("\n");
}

export function updateMetricsSummary(document, runs) {
  const start = "<!-- perf-metrics:start -->", end = "<!-- perf-metrics:end -->";
  assert.equal(document.split(start).length, 2, "add the perf-metrics markers to docs/benchmarks.md first");
  assert.equal(document.split(end).length, 2, "add the perf-metrics markers to docs/benchmarks.md first");
  assert.ok(document.indexOf(start) < document.indexOf(end), "reversed perf-metrics markers");
  assert.ok(runs.length > 0);
  return document.slice(0, document.indexOf(start) + start.length) + "\n" +
    runs.map(renderMetricsSummary).join("\n") + "\n" + document.slice(document.indexOf(end));
}

function processSuite(compiler, out, projects, quick) {
  const context = captureRun(out, { compiler, mode: quick ? "quick" : "benchmark" });
  const temporary = mkdtempSync(join(tmpdir(), "tsuzuri-process-bench-"));
  const records = [];
  try {
    for (const project of projects) {
      const variants = [
        ["check", "native", "none", null, ["check", project]],
        ...["O0", "O3"].map((opt) => ["build", "native", opt, "executable_bytes", ["build", project, `-${opt}`, "--cpu", "generic", "--no-cache"]]),
        ["build-object", "native", "O3", "object_bytes", ["build", project, "--emit", "object", "-O3", "--cpu", "generic", "--no-cache"]],
        ["build-wasm", "wasm32", "O3", "wasm_bytes", ["build", project, "--target", "wasm32", "-O3", "--no-cache"]],
      ];
      const executables = new Map();
      for (const [variant, target, opt, sizeMetric, args] of variants) {
        const output = join(temporary, `app-${variant}-${opt}${target === "wasm32" ? ".wasm" : ""}`);
        const commandArgs = sizeMetric ? [...args, "-o", output] : args;
        const options = { env: { ...process.env, TSUZURI_TIME_PASSES: "1", TSUZURI_CACHE_DIR: "" }, runs: quick ? 1 : 9, warmups: quick ? 0 : 1 };
        const sizes = [], fingerprints = [];
        const measured = { wall_ms: [], user_ms: [], sys_ms: [], peak_rss_bytes: [], stdout: [], stderr: [] };
        let skipped = false;
        for (let sample = -options.warmups; sample < options.runs; sample++) {
          let result;
          try {
            result = measureProcess(compiler, commandArgs, { ...options, runs: 1, warmups: 0 });
          } catch (error) {
            if (sample === -options.warmups && ["build-object", "build-wasm"].includes(variant) && error.status === 1 &&
              /\bE2004\b/.test(error.stderr)) {
              console.error(`skip ${project} ${variant}: ${unsupportedReason(error.stderr)}`);
              skipped = true;
              break;
            }
            throw error;
          }
          if (sample < 0) continue;
          for (const field of Object.keys(measured)) measured[field].push(...result[field]);
          if (sizeMetric) {
            sizes.push(statSync(output).size);
            fingerprints.push(artifactFingerprint(output));
          }
        }
        if (skipped) continue;
        const fields = { suite: "process", workload: project, language: "tsuzuri", variant, target, opt };
        records.push(...processRecords(context, fields, measured));
        if (sizeMetric) {
          assert.ok(sizes.every((size) => size === sizes[0]), "artifact size changed between samples");
          assert.ok(fingerprints.every((value) => value === fingerprints[0]), "artifact code changed between samples");
          records.push(makeRecord(context, { ...fields, metric: sizeMetric, samples: [sizes[0]] }));
        }
        if (variant === "build") executables.set(opt, output);
      }
      for (const [opt, executable] of executables) {
        const measured = measureProcess(executable, [], { env: { ...process.env, TSUZURI_TIME_PASSES: "0" },
          runs: quick ? 1 : 9, warmups: quick ? 0 : 1 });
        assert.ok(measured.stdout.every((stdout) => stdout === measured.stdout[0]), "execution output changed between samples");
        records.push(...processRecords(context, { suite: "process", workload: project, language: "tsuzuri", variant: "execute",
          target: "native", opt }, measured, { phases: false }));
      }
    }
    writeRecords(out, "process", records);
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const usage = "Usage: node benchmarks/metrics.mjs process|size [compiler] --out dir [--project dir]... [--quick] | report dir... [--baseline dir | --publish]";
  const [action, ...args] = process.argv.slice(2);
  const options = {}, positional = [], projects = [];
  let quick = false, publish = false;
  const measuring = action === "process" || action === "size";
  for (let index = 0; index < args.length; ++index) {
    const arg = args[index];
    if (arg === "--quick" && !quick && measuring) quick = true;
    else if (arg === "--publish" && !publish && action === "report") publish = true;
    else if (["--out", "--baseline", "--project"].includes(arg)) {
      const value = args[++index];
      assert.ok(value && !value.startsWith("-"), usage);
      if (arg === "--project" && measuring) projects.push(value);
      else {
        assert.ok(options[arg] === undefined && (arg === "--out" && measuring || arg === "--baseline" && action === "report"), usage);
        options[arg] = value;
      }
    } else {
      assert.ok(!arg.startsWith("-"), usage);
      positional.push(arg);
    }
  }
  if (action === "process") {
    assert.ok(options["--out"] && positional.length <= 1, usage);
    const out = resolve(options["--out"]);
    assert.ok(!existsSync(join(out, "process.jsonl")), `EEXIST: ${out}/process.jsonl`);
    processSuite(resolve(positional[0] ?? join(root, "target/release/tsuzuri")), out, projects.length ? projects : ["examples/hello"], quick);
  } else if (action === "size") {
    assert.ok(options["--out"] && positional.length <= 1, usage);
    const out = resolve(options["--out"]);
    assert.ok(!existsSync(join(out, "size.jsonl")), `EEXIST: ${out}/size.jsonl`);
    // A dynamic import without `await`: size.mjs imports this module, which must finish evaluating first.
    import("./size.mjs").then(({ SIZE_PROJECTS, sizeSuite }) => sizeSuite(resolve(positional[0] ?? join(root, "target/release/tsuzuri")),
      out, { projects: projects.length ? projects.map((project) => resolve(project)) : SIZE_PROJECTS, quick }));
  } else if (action === "report") {
    assert.ok(positional.length > 0 && !(publish && options["--baseline"]), usage);
    const runs = positional.map(readRecords);
    if (publish) {
      const path = join(root, "docs/benchmarks.md");
      writeAtomically(path, updateMetricsSummary(readFileSync(path, "utf8"), runs));
      console.log("Metrics summary updated");
    } else if (options["--baseline"]) {
      assert.equal(runs.length, 1, usage);
      console.log(renderComparison(compareRuns(readRecords(options["--baseline"]), runs[0])));
    } else {
      for (const record of runs.flat()) console.log(`${record.suite}/${record.workload} ${record.variant} ${record.language} ${record.target} ${record.opt} ${record.metric}${record.phase ? `[${record.phase}]` : ""} ${record.median} ${record.unit} (${record.min} ~ ${record.max}, ${record.samples.length})`);
    }
  } else {
    throw new Error(usage);
  }
}
