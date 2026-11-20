// The language server's measurements of PB08 as PX01 records (suite `lsp`). It asserts what it reads but has no
// speed threshold.
//
//   node benchmarks/lsp/workloads.mjs target/perf/PB08/projects          (once: the workloads)
//   node benchmarks/run-lsp.mjs target/release/tsuzuri --out target/perf/PB08-after
//       [--workloads target/perf/PB08/projects] [--projects hello,big,syn200] [--threads unset,1,8]
//       [--samples 11] [--parts refresh,publish,codeaction,cli] [--quick]
//
// Parts (the ticket's 目標と指標):
// - refresh (M1): one server per project and thread setting, `rootUri` the project. After the didOpen of Main.tz
//   and its first publishDiagnostics, 2 warm-ups and then `--samples` times: the whole text replaced by the
//   original text plus "\n" (odd version) or "\n\n" (even version), which changes no diagnostic, then
//   `textDocument/semanticTokens/full`; the time from writing the didChange to the response (variant
//   `refresh-<threads>`). Then the same request without a change (`tokens-<threads>`), which refreshes nothing.
// - publish (M4): the time from writing a didChange to the publishDiagnostics of the edited URI with that version,
//   which includes the 200 ms debounce. The edits alternate between the original text plus "\n// edit <i>\n"
//   (`publish-comment-<threads>`, no new diagnostic) and plus "\ndef broken_<i> :: i64 = undefined_name_<i>\n"
//   (`publish-error-<threads>`, one more E1002); 2 warm-ups, then `--samples` of each.
// - codeaction (M4): `big` only. The five requests of the ticket on Main.tz, each once to record the titles of
//   the actions (projects.json, to compare before and after) and then `--samples` times: (1) line 20 (19 from 0),
//   column 0, no diagnostics, triggerKind 2; (2) the first diagnostic's range with it; (3) (2) with
//   only ["quickfix"]; (4) the whole document with every diagnostic, triggerKind 1; (5) 0:0, only
//   ["source.fixAll"], triggerKind 1.
// - cli (M3): `check <project>` with TSUZURI_CACHE_DIR= and TSUZURI_TIME_PASSES=1, at TSUZURI_THREADS=1 and unset
//   (`check-1`, `check-auto`), for each project and its `-allstd` variant when the workloads have one.
//
// `<threads>` is `auto` for an unset TSUZURI_THREADS. Each record's samples are milliseconds; projects.json keeps
// the raw per-sample values, the load average around each server and the code actions' titles.
import assert from "node:assert/strict";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { loadavg, platform } from "node:os";
import { join, resolve } from "node:path";
import { connect, endPosition, isDiagnostics, uriOf } from "./lsp/client.mjs";
import { readWorkloads } from "./lsp/workloads.mjs";
import { captureRun, makeRecord, measureProcess, processRecords, writeRecords } from "./metrics.mjs";

const args = process.argv.slice(2);
const compiler = resolve(args.shift() ?? "target/release/tsuzuri");
let out, workloads = resolve("target/perf/PB08/projects"), projects = ["hello", "big", "syn200"];
let threadSettings = ["unset", "1", "8"], samples = 11, parts = ["refresh", "publish", "codeaction", "cli"], quick = false;
while (args.length) {
  const name = args.shift();
  if (name === "--quick") { quick = true; continue; }
  const value = args.shift();
  assert.ok(value !== undefined, `${name} needs a value`);
  if (name === "--out") out = resolve(value);
  else if (name === "--workloads") workloads = resolve(value);
  else if (name === "--projects") projects = value.split(",");
  else if (name === "--threads") threadSettings = value.split(",");
  else if (name === "--samples") samples = Number(value);
  else if (name === "--parts") parts = value.split(",");
  else throw new Error(`unknown option ${name}`);
}
assert.ok(out, "--out is required");
assert.ok(!existsSync(join(out, "lsp.jsonl")), `EEXIST: ${out}`);
if (quick) samples = Math.min(samples, 3);
assert.ok(Number.isSafeInteger(samples) && samples > 0);
assert.ok(threadSettings.every((threads) => threads === "unset" || /^[1-9][0-9]*$/.test(threads)), "--threads takes unset or counts");
assert.ok(parts.every((part) => ["refresh", "publish", "codeaction", "cli"].includes(part)), `unknown part in ${parts}`);
const all = readWorkloads(workloads);
for (const project of projects) assert.ok(all[project], `unknown workload ${project}`);
mkdirSync(out, { recursive: true });
const context = captureRun(out, { compiler, mode: quick ? "quick" : "benchmark" });
const records = [];
const raw = { context, compiler, workloads, samples, parts, threads: threadSettings, measurements: [] };
const label = (threads) => (threads === "unset" ? "auto" : threads);
const WARMUPS = 2;

function environment(threads) {
  const env = { ...process.env };
  for (const name of ["TSUZURI_THREADS", "TSUZURI_TIME_PASSES", "TSUZURI_CACHE_DIR"]) delete env[name];
  if (threads !== "unset") env.TSUZURI_THREADS = threads;
  return env;
}

function record(workload, variant, values, extra = {}) {
  records.push(makeRecord(context, {
    suite: "lsp", workload, language: "tsuzuri", variant, opt: "none", metric: "wall_time", samples: values,
  }));
  raw.measurements.push({ workload, variant, samples: values, ...extra });
  const sorted = [...values].sort((left, right) => left - right);
  console.log(JSON.stringify({ workload, variant, median: +sorted[(sorted.length - 1) >> 1].toFixed(1), min: +sorted[0].toFixed(1), max: +sorted.at(-1).toFixed(1), ...extra }));
}

// A server on `project` with Main.tz open and its first diagnostics published.
async function open(project, threads) {
  const lsp = connect(compiler, { env: environment(threads) });
  await lsp.initialize(project);
  const main = join(project, "Main.tz");
  const text = readFileSync(main, "utf8");
  const uri = uriOf(main);
  lsp.notify("textDocument/didOpen", { textDocument: { uri, languageId: "tsuzuri", version: 1, text } });
  const first = await lsp.wait(isDiagnostics(uri, 1));
  return { lsp, uri, text, first, load: [loadavg()[0]] };
}

async function close(server) {
  const exit = await server.lsp.shutdown();
  assert.equal(exit.code, 0, `the server exited with ${exit.code ?? exit.signal}:\n${server.lsp.stderr}`);
  server.load.push(loadavg()[0]);
  return server.load.map((value) => +value.toFixed(2));
}

async function refresh(name, threads) {
  const server = await open(all[name], threads);
  const { lsp, uri, text } = server;
  const times = [];
  let version = 1;
  for (let index = 0; index < WARMUPS + samples; index++) {
    version++;
    const sent = lsp.now();
    lsp.notify("textDocument/didChange", { textDocument: { uri, version }, contentChanges: [{ text: text + (version % 2 ? "\n" : "\n\n") }] });
    const response = await lsp.request("textDocument/semanticTokens/full", { textDocument: { uri } }).response;
    assert.ok(Array.isArray(response.result?.data), `semantic tokens of ${name}: ${JSON.stringify(response.error)}`);
    if (index >= WARMUPS) times.push(response.t - sent);
  }
  const handler = [];
  for (let index = 0; index < samples; index++) {
    const request = lsp.request("textDocument/semanticTokens/full", { textDocument: { uri } });
    const response = await request.response;
    handler.push(response.t - request.sent);
  }
  const diagnostics = lsp.messages.filter(isDiagnostics(uri)).at(-1).params.diagnostics.length;
  const load = await close(server);
  record(name, `refresh-${label(threads)}`, times, { diagnostics, load });
  record(name, `tokens-${label(threads)}`, handler, { load });
}

async function publish(name, threads) {
  const server = await open(all[name], threads);
  const { lsp, uri, text } = server;
  const comment = [], error = [];
  let version = 1;
  for (let index = 0; index < 2 * (WARMUPS + samples); index++) {
    version++;
    const broken = index % 2 === 1;
    const edited = broken ? `${text}\ndef broken_${index} :: i64 = undefined_name_${index}\n` : `${text}\n// edit ${index}\n`;
    const sent = lsp.now();
    lsp.notify("textDocument/didChange", { textDocument: { uri, version }, contentChanges: [{ text: edited }] });
    const published = await lsp.wait(isDiagnostics(uri, version));
    assert.ok(!broken || published.params.diagnostics.some((diagnostic) => diagnostic.code === "E1002"), `${name}: the broken edit publishes E1002`);
    if (index >= 2 * WARMUPS) (broken ? error : comment).push(published.t - sent);
  }
  const load = await close(server);
  record(name, `publish-comment-${label(threads)}`, comment, { load });
  record(name, `publish-error-${label(threads)}`, error, { load });
}

async function codeActions(threads) {
  const server = await open(all.big, threads);
  const { lsp, uri, text, first } = server;
  const diagnostics = first.params.diagnostics;
  assert.ok(diagnostics.length > 0, "big's Main.tz has diagnostics");
  const origin = { line: 0, character: 0 }, line20 = { line: 19, character: 0 };
  const requests = [
    ["cursor", { range: { start: line20, end: line20 }, context: { diagnostics: [], triggerKind: 2 } }],
    ["diagnostic", { range: diagnostics[0].range, context: { diagnostics: [diagnostics[0]], triggerKind: 2 } }],
    ["quickfix", { range: diagnostics[0].range, context: { diagnostics: [diagnostics[0]], only: ["quickfix"], triggerKind: 2 } }],
    ["document", { range: { start: origin, end: endPosition(text) }, context: { diagnostics, triggerKind: 1 } }],
    ["fixall", { range: { start: origin, end: origin }, context: { diagnostics: [], only: ["source.fixAll"], triggerKind: 1 } }],
  ];
  const results = [];
  for (const [variant, params] of requests) {
    const times = [];
    let titles;
    for (let index = 0; index <= samples; index++) {
      const request = lsp.request("textDocument/codeAction", { textDocument: { uri }, ...params });
      const response = await request.response;
      assert.ok(!response.error, `codeAction ${variant}: ${JSON.stringify(response.error)}`);
      const current = (response.result ?? []).map((action) => action.title);
      titles ??= current;
      assert.deepEqual(current, titles, `codeAction ${variant} changed its actions`);
      if (index > 0) times.push(response.t - request.sent);
    }
    results.push([variant, times, titles]);
  }
  const load = await close(server);
  for (const [variant, times, titles] of results) record("big", `codeaction-${variant}-${label(threads)}`, times, { titles, load });
}

function cli(name) {
  for (const [workload, project] of [[name, all[name]], [`${name}-allstd`, all[`${name}-allstd`]]]) {
    if (!project) continue;
    for (const threads of ["1", "unset"]) {
      const env = { ...environment(threads), TSUZURI_CACHE_DIR: "", TSUZURI_TIME_PASSES: "1" };
      const before = loadavg()[0];
      const measured = measureProcess(compiler, ["check", project], { env, runs: samples, warmups: 1 });
      const fields = { suite: "lsp", workload, language: "tsuzuri", variant: `check-${label(threads)}`, target: "native", opt: "none" };
      let phases = true, made;
      try {
        made = processRecords(context, fields, measured);
      } catch (error) {
        // A phase that PX01 does not name (for example `effects`) leaves only the process's values.
        phases = false;
        made = processRecords(context, fields, measured, { phases: false });
        console.error(`${workload}: phases left out: ${error.message.split("\n")[0]}`);
      }
      records.push(...made);
      const load = [before, loadavg()[0]].map((value) => +value.toFixed(2));
      raw.measurements.push({ workload, variant: fields.variant, samples: measured.wall_ms, phases, load });
      const sorted = [...measured.wall_ms].sort((left, right) => left - right);
      console.log(JSON.stringify({ workload, variant: fields.variant, median: +sorted[(sorted.length - 1) >> 1].toFixed(1), load }));
    }
  }
}

for (const name of projects) {
  for (const threads of threadSettings) {
    if (parts.includes("refresh")) await refresh(name, threads);
    if (parts.includes("publish")) await publish(name, threads);
  }
}
if (parts.includes("codeaction")) for (const threads of threadSettings) await codeActions(threads);
if (parts.includes("cli")) {
  assert.ok(["darwin", "linux"].includes(platform()), "the cli part measures with /usr/bin/time (macOS or Linux)");
  for (const name of projects) cli(name);
}
writeFileSync(join(out, "projects.json"), JSON.stringify(raw, null, 2) + "\n");
console.log(writeRecords(out, "lsp", records));
