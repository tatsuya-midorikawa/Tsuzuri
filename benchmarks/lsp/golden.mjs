// The golden outputs that PB08 must not change (the ticket's 手順 2 and "変えてはいけない意味"), made with one
// compiler and compared between compilers or thread counts:
//
//   node benchmarks/lsp/golden.mjs cli <compiler> <out.jsonl> [--threads 1|auto|N] [--workloads DIR] [--jobs N]
//   node benchmarks/lsp/golden.mjs lsp <compiler> <out-dir> [--threads unset|N] [--workloads DIR] [--projects a,b]
//   node benchmarks/lsp/golden.mjs compare <a> <b>        (two .jsonl files, or two directories of transcripts)
//
// `cli`: `check <target> --json` without the frontend cache (`TSUZURI_CACHE_DIR=`): the exit code, stdout and
// stderr, and the files of the knowledge graph that `check` writes (into a directory of its own, `TSUZURI_KG_DIR`;
// `.tsuzuri-kg` holds a stamp and is left out), for every example and fixture (a directory or a .tz file), every
// .tz file under tests/conformance/ and every workload of benchmarks/lsp/workloads.mjs.
// `lsp`: one transcript per workload project: the diagnostics of every published URI after each change, the
// semantic tokens, inlay hints and document symbols of every user file, hover and definition at fixed positions,
// completion after `Matrix.`, after `Json.` and at the start of a top-level line (the module list), prepareRename
// and rename, the code actions of the ticket's M4 (quickfix and source.fixAll.tsuzuri, and a quickfix for every
// diagnostic of Main.tz), and the same after an edit that adds an E1002, after one that misspells a module
// (`Matrx.init`: candidates and a replacement) and after the text is restored.
// The scenes of Phase 2 (O6), where the language server reads only the opt-in std modules that the sources name:
// a `def` that names `Matrix` and `Json` (hover of the module names and members), a `def` that misspells `Matrix`
// (its candidates, replacement fix and quickfix where top-level code would clash with `def main`), a `def` with
// unknown values that only members of opt-in modules match (`parse`, `Null`), completion
// after `Json.Value.` and `std::`, and, after the restored text has been analyzed, an edit that starts
// `Json.parse (` and does not parse: signature help, `Json.` completion and hover there.
// The scene of the review of Phase 2, in a session of its own (`<workload>.types.json`), so that the transcripts
// above stay as they were: misspelt types that only the type of an opt-in std module named after the module
// matches (`Tensr`, `WaitGrop`, `Tickr`, `Rgex`, `Semaphor`, `Contxt`), in signatures, a type annotation, a record
// literal and a record pattern: the diagnostics, the code actions of the document, source.fixAll.tsuzuri and a
// quickfix for every diagnostic of Main.tz. Then the same for misspelt types in an unsaved buffer of the namespace
// `Sample` beside an unsaved `Sample::Tensor` that hides the std `Tensor` type there (the first Copilot review of
// PR #122); neither file is written.
// `--threads` sets TSUZURI_THREADS (`auto` and `unset` leave it unset; a count from 1 to 32), `--jobs` is a positive
// integer. `compare` refuses a CLI golden with a blank line, a line that is not an entry or no entries, and a directory
// without transcripts or with a transcript that holds no request (tests/benchmark_lsp_golden.mjs). No timing is
// recorded.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { connect, endPosition, isDiagnostics, position, uriOf } from "./client.mjs";
import { readWorkloads } from "./workloads.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const sha256 = (data) => createHash("sha256").update(data).digest("hex");

function option(args, name, fallback) {
  const index = args.indexOf(name);
  if (index < 0) return fallback;
  const [, value] = args.splice(index, 2);
  assert.ok(value !== undefined, `${name} needs a value`);
  return value;
}

// `--jobs`: a positive safe integer. With none, no worker would start: the golden would hold blank lines and still
// report every target.
function jobCount(value) {
  const jobs = /^[1-9][0-9]*$/.test(value) ? Number(value) : NaN;
  assert.ok(Number.isSafeInteger(jobs), `--jobs takes a positive integer, not '${value}'`);
  return jobs;
}

// `--threads`: `auto` or `unset`, or a count that TSUZURI_THREADS takes (1 to `MAX_THREADS`, 32, in src/parallel.rs).
// Any other value would make every `check` report the variable's error, and two such goldens would compare equal.
function threadSetting(value) {
  assert.ok(value === "auto" || value === "unset" || (/^[1-9][0-9]*$/.test(value) && Number(value) <= 32),
    `--threads takes auto, unset or a count from 1 to 32, not '${value}'`);
  return value;
}

function environment(threads) {
  const env = { ...process.env, TSUZURI_CACHE_DIR: "" };
  delete env.TSUZURI_THREADS;
  delete env.TSUZURI_TIME_PASSES;
  delete env.TSUZURI_KG;
  delete env.TSUZURI_KG_DIR;
  if (threads !== "auto" && threads !== "unset") env.TSUZURI_THREADS = threads;
  return env;
}

// Every target of `cli`, in a fixed order, as paths relative to the repository when they are inside it.
export function cliTargets(workloads) {
  const targets = [];
  for (const directory of ["examples", "tests/fixtures"]) {
    for (const entry of readdirSync(join(root, directory)).sort()) {
      const path = join(root, directory, entry);
      if (statSync(path).isDirectory() || entry.endsWith(".tz")) targets.push(path);
    }
  }
  const walk = (directory) => readdirSync(directory, { withFileTypes: true })
    .sort((left, right) => (left.name < right.name ? -1 : left.name > right.name ? 1 : 0))
    .flatMap((entry) => entry.isDirectory() ? walk(join(directory, entry.name))
      : entry.name.endsWith(".tz") ? [join(directory, entry.name)] : []);
  targets.push(...walk(join(root, "tests/conformance")));
  if (workloads) targets.push(...Object.values(readWorkloads(workloads)));
  return targets;
}

function files(directory) {
  if (!existsSync(directory)) return {};
  const found = {};
  const walk = (current) => {
    for (const entry of readdirSync(current, { withFileTypes: true }).sort((a, b) => (a.name < b.name ? -1 : 1))) {
      const path = join(current, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (entry.name !== ".tsuzuri-kg") found[relative(directory, path)] = sha256(readFileSync(path));
    }
  };
  walk(directory);
  return found;
}

function check(compiler, target, env, kg) {
  rmSync(kg, { recursive: true, force: true });
  return new Promise((resolvePromise, reject) => {
    const child = spawn(compiler, ["check", target, "--json"], { env: { ...env, TSUZURI_KG_DIR: kg }, cwd: root });
    const stdout = [], stderr = [];
    child.stdout.on("data", (chunk) => stdout.push(chunk));
    child.stderr.on("data", (chunk) => stderr.push(chunk));
    const timer = setTimeout(() => child.kill("SIGKILL"), 600_000);
    child.on("error", reject);
    child.on("close", (status, signal) => {
      clearTimeout(timer);
      // A compiler that was killed (the timeout above) or died of a signal did not finish its check: two such entries
      // would compare equal without comparing any output, so the golden is not written.
      if (status === null) {
        rmSync(kg, { recursive: true, force: true });
        reject(new Error(`check of ${relative(root, target)} did not finish: ${signal ?? "no exit status"}`));
        return;
      }
      const entry = {
        target: relative(root, target), status, signal,
        stdout: Buffer.concat(stdout).toString(), stderr: Buffer.concat(stderr).toString(), kg: files(kg),
      };
      rmSync(kg, { recursive: true, force: true });
      resolvePromise(entry);
    });
  });
}

async function cli(compiler, out, { threads, workloads, jobs }) {
  const targets = cliTargets(workloads);
  const env = environment(threads);
  const entries = new Array(targets.length);
  let next = 0;
  const scratch = `${out}.kg`;
  const worker = async (slot) => {
    while (next < targets.length) {
      const index = next++;
      entries[index] = await check(compiler, targets[index], env, join(scratch, String(slot)));
    }
  };
  await Promise.all(Array.from({ length: jobs }, (_, slot) => worker(slot)));
  rmSync(scratch, { recursive: true, force: true });
  const missing = entries.findIndex((entry) => entry === undefined);
  assert.equal(missing, -1, `no entry for ${targets[missing]}`);
  mkdirSync(dirname(out), { recursive: true });
  writeFileSync(out, entries.map((entry) => JSON.stringify(entry)).join("\n") + "\n");
  const summary = {
    threads, targets: entries.length, nonzero: entries.filter((entry) => entry.status !== 0).length,
    kg: entries.filter((entry) => Object.keys(entry.kg).length).length, sha256: sha256(readFileSync(out)),
  };
  console.log(JSON.stringify(summary));
}

// The user .tz files of a project, Main.tz first, then in path order.
function userFiles(project) {
  const found = [];
  const walk = (directory) => {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      if (entry.name.startsWith(".") || entry.name === "target") continue;
      const path = join(directory, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (entry.name.endsWith(".tz")) found.push(path);
    }
  };
  walk(project);
  const main = join(project, "Main.tz");
  assert.ok(found.includes(main), `${project} has no Main.tz`);
  return [main, ...found.filter((path) => path !== main).sort()];
}

// At most `limit` positions, spread evenly over the identifiers of `text`: one character into each.
function positions(text, limit) {
  const offsets = [...text.matchAll(/[A-Za-z_][A-Za-z0-9_]*/g)].map((match) => match.index + Math.min(1, match[0].length - 1));
  const step = Math.max(1, Math.ceil(offsets.length / limit));
  return offsets.filter((_, index) => index % step === 0).slice(0, limit);
}

async function transcript(compiler, project, env) {
  const lsp = connect(compiler, { env });
  const log = [];
  const result = async (label, method, params) => {
    const response = await lsp.request(method, params).response;
    log.push([label, response.error ? { error: response.error } : response.result]);
    return response.result;
  };
  await lsp.initialize(project);
  const paths = userFiles(project);
  const texts = Object.fromEntries(paths.map((path) => [path, readFileSync(path, "utf8")]));
  const main = paths[0], mainUri = uriOf(main), original = texts[main];
  const name = (path) => relative(project, path);
  const document = (path) => ({ textDocument: { uri: uriOf(path) } });
  const full = (text) => ({ start: { line: 0, character: 0 }, end: endPosition(text) });
  let version = 1;
  lsp.notify("textDocument/didOpen", { textDocument: { uri: mainUri, languageId: "tsuzuri", version, text: original } });
  await lsp.wait(isDiagnostics(mainUri, version));
  for (const path of paths.slice(1)) {
    lsp.notify("textDocument/didOpen", { textDocument: { uri: uriOf(path), languageId: "tsuzuri", version: 1, text: texts[path] } });
  }
  // Every user file: its tokens (the first refreshes the project with all buffers open), hints and symbols.
  for (const path of paths) {
    await result(`semanticTokens ${name(path)}`, "textDocument/semanticTokens/full", document(path));
    await result(`inlayHint ${name(path)}`, "textDocument/inlayHint", { ...document(path), range: full(texts[path]) });
    await result(`documentSymbol ${name(path)}`, "textDocument/documentSymbol", document(path));
  }
  for (const path of paths) {
    for (const offset of positions(texts[path], path === main ? 60 : 20)) {
      const at = { ...document(path), position: position(texts[path], offset) };
      await result(`hover ${name(path)}:${offset}`, "textDocument/hover", at);
      await result(`definition ${name(path)}:${offset}`, "textDocument/definition", at);
    }
  }
  let renames = 0;
  for (const offset of positions(original, 60)) {
    if (renames === 3) break;
    const at = { ...document(main), position: position(original, offset) };
    const prepared = await result(`prepareRename Main.tz:${offset}`, "textDocument/prepareRename", at);
    if (prepared && !prepared.error) {
      await result(`rename Main.tz:${offset}`, "textDocument/rename", { ...at, newName: "renamed_probe" });
      renames++;
    }
  }
  const diagnosticsOf = (uri) => lsp.messages.filter(isDiagnostics(uri)).at(-1)?.params.diagnostics ?? [];
  const actions = async (label, text) => {
    const diagnostics = diagnosticsOf(mainUri);
    const lines = text.split("\n").length;
    const quiet = { line: Math.min(19, lines - 1), character: 0 };
    await result(`${label} codeAction cursor`, "textDocument/codeAction", { ...document(main), range: { start: quiet, end: quiet }, context: { diagnostics: [], triggerKind: 2 } });
    if (diagnostics.length) {
      const first = diagnostics[0];
      await result(`${label} codeAction first`, "textDocument/codeAction", { ...document(main), range: first.range, context: { diagnostics: [first], triggerKind: 2 } });
      await result(`${label} codeAction first quickfix`, "textDocument/codeAction", { ...document(main), range: first.range, context: { diagnostics: [first], only: ["quickfix"], triggerKind: 2 } });
    }
    await result(`${label} codeAction document`, "textDocument/codeAction", { ...document(main), range: full(text), context: { diagnostics, triggerKind: 1 } });
    const origin = { line: 0, character: 0 };
    await result(`${label} codeAction fixAll`, "textDocument/codeAction", { ...document(main), range: { start: origin, end: origin }, context: { diagnostics: [], only: ["source.fixAll"], triggerKind: 1 } });
    for (const [index, diagnostic] of diagnostics.slice(0, 30).entries()) {
      await result(`${label} codeAction quickfix ${index}`, "textDocument/codeAction", { ...document(main), range: diagnostic.range, context: { diagnostics: [diagnostic], only: ["quickfix"], triggerKind: 2 } });
    }
  };
  await actions("open", original);
  // A change of Main.tz; the request after it refreshes the project (or the debounce did already).
  const change = async (label, text, requests) => {
    version++;
    lsp.notify("textDocument/didChange", { textDocument: { uri: mainUri, version }, contentChanges: [{ text }] });
    await requests(text);
    await lsp.wait(isDiagnostics(mainUri, version));
    log.push([`${label} diagnostics`, Object.fromEntries(paths.map((path) => [name(path), diagnosticsOf(uriOf(path))]))]);
  };
  log.push(["open diagnostics", Object.fromEntries(paths.map((path) => [name(path), diagnosticsOf(uriOf(path))]))]);
  await change("broken", `${original}\ndef broken_1 :: i64 = undefined_name_1\n`, async (text) => {
    await result("broken semanticTokens", "textDocument/semanticTokens/full", document(main));
    await result("broken inlayHint", "textDocument/inlayHint", { ...document(main), range: full(text) });
    for (const offset of positions(original, 10)) {
      await result(`broken hover Main.tz:${offset}`, "textDocument/hover", { ...document(main), position: position(text, offset) });
    }
    const end = text.lastIndexOf("undefined_name_1") + 3;
    await result("broken hover undefined", "textDocument/hover", { ...document(main), position: position(text, end) });
  });
  await actions("broken", `${original}\ndef broken_1 :: i64 = undefined_name_1\n`);
  // A misspelt module: the diagnostic's candidates and its replacement quickfix.
  const misspelt = `${original}\nlet misspelt_probe = Matrx.init\n`;
  await change("misspelt", misspelt, async () => {
    await result("misspelt semanticTokens", "textDocument/semanticTokens/full", document(main));
  });
  await actions("misspelt", misspelt);
  // Phase 2: a definition that names two opt-in modules and checks; hover of their names and members.
  const named = `${original}\ndef opt_in_probe :: own unit -> i64 = \\() -> {\n    let m = Matrix.init 2 2 (\\i j -> i);\n    let parsed = Json.parse (ref u8"1");\n    Matrix.rows (ref m) + (if Result.is_ok (ref parsed) then 1 else 0)\n}\n`;
  await change("named", named, async (text) => {
    await result("named semanticTokens", "textDocument/semanticTokens/full", document(main));
    const at = (needle, into) => ({ ...document(main), position: position(text, text.lastIndexOf(needle) + into) });
    for (const [label, needle, into] of [["Matrix", "Matrix.init", 1], ["init", "Matrix.init", 8], ["rows", "Matrix.rows", 8], ["Json", "Json.parse", 1], ["parse", "Json.parse", 6]]) {
      await result(`named hover ${label}`, "textDocument/hover", at(needle, into));
    }
  });
  // Phase 2: a misspelt module inside a definition, which a project with `def main` accepts.
  const misspeltDef = `${original}\ndef misspelt_def_probe :: own unit -> i64 = \\() -> {\n    let m = Matrx.init 2 2 (\\i j -> i);\n    0\n}\n`;
  await change("misspelt-def", misspeltDef, async () => {
    await result("misspelt-def semanticTokens", "textDocument/semanticTokens/full", document(main));
  });
  await actions("misspelt-def", misspeltDef);
  // Phase 2: unknown values that members of opt-in modules match by their last part: `parse` (`Format.parse` and
  // `Json.parse`, so no single best name and no fix) and `Null` (`Json.Null`).
  const unknown = `${original}\ndef unknown_probe :: own unit -> i64 = \\() -> {\n    let a = parse;\n    let b = Null;\n    0\n}\n`;
  await change("unknown", unknown, async () => {
    await result("unknown semanticTokens", "textDocument/semanticTokens/full", document(main));
  });
  await actions("unknown", unknown);
  for (const [label, suffix] of [["matrix", "\nlet completion_probe = Matrix."], ["json", "\nlet completion_probe = Json."], ["top", "\n"],
    ["json-value", "\nlet completion_probe = Json.Value."], ["std", "\nlet completion_probe = std::"]]) {
    await change(`completion ${label}`, original + suffix, async (text) => {
      await result(`completion ${label}`, "textDocument/completion", { ...document(main), position: endPosition(text) });
    });
  }
  await change("restored", original, async (text) => {
    await result("restored semanticTokens", "textDocument/semanticTokens/full", document(main));
    await result("restored inlayHint", "textDocument/inlayHint", { ...document(main), range: full(text) });
  });
  // Phase 2: after that analysis, the first edit that names `Json` does not parse; the requests fall back to it.
  const signature = `${original}\nlet signature_probe = Json.parse (`;
  await change("signature", signature, async (text) => {
    await result("signature signatureHelp", "textDocument/signatureHelp", { ...document(main), position: endPosition(text) });
    const dot = text.lastIndexOf("Json.") + 5;
    await result("signature completion", "textDocument/completion", { ...document(main), position: position(text, dot) });
    await result("signature hover Json", "textDocument/hover", { ...document(main), position: position(text, dot - 4) });
    await result("signature hover parse", "textDocument/hover", { ...document(main), position: position(text, dot + 1) });
  });
  const publishes = lsp.messages.filter((message) => message.method === "textDocument/publishDiagnostics")
    .map((message) => [relative(project, fileURLToPath(message.params.uri)), message.params.version ?? null, message.params.diagnostics]);
  const exit = await lsp.shutdown();
  assert.equal(exit.code, 0, `the server of ${project} exited with ${exit.code ?? exit.signal}:\n${lsp.stderr}`);
  return { project: relative(root, project), files: paths.map(name), log, publishes };
}

// The review scene of Phase 2: misspelt types in the places that look up a type name (`type_pool`).
const TYPE_SCENES = [
  ["signatures", "\ndef type_probe_1 :: ref Tensr -> i64 = \\_x -> 0\ndef type_probe_2 :: ref WaitGrop -> i64 = \\_x -> 0\n" +
    "def type_probe_3 :: ref Tickr -> i64 = \\_x -> 0\ndef type_probe_4 :: own Maybe<Rgex> -> i64 = \\_x -> 0\n" +
    "def type_probe_5 :: ref Valu -> i64 = \\_x -> 0\n"],
  ["bodies", "\ndef type_probe_6 :: own unit -> i64 = \\() -> {\n    let _s = Semaphor { count: 1 };\n    let _t: Tickr = 0;\n    0\n}\n" +
    "def type_probe_7 :: own i64 -> i64 = \\v -> match v with\n    | Contxt { done = d } -> d\n"],
];

async function typeTranscript(compiler, project, env) {
  const lsp = connect(compiler, { env });
  const log = [];
  const result = async (label, method, params) => {
    const response = await lsp.request(method, params).response;
    log.push([label, response.error ? { error: response.error } : response.result]);
    return response.result;
  };
  await lsp.initialize(project);
  const paths = userFiles(project);
  const main = paths[0], mainUri = uriOf(main), original = readFileSync(main, "utf8");
  const document = { textDocument: { uri: mainUri } };
  const diagnosticsOf = (uri) => lsp.messages.filter(isDiagnostics(uri)).at(-1)?.params.diagnostics ?? [];
  let version = 1;
  lsp.notify("textDocument/didOpen", { textDocument: { uri: mainUri, languageId: "tsuzuri", version, text: original } });
  await lsp.wait(isDiagnostics(mainUri, version));
  for (const [label, suffix] of TYPE_SCENES) {
    const text = original + suffix;
    version++;
    lsp.notify("textDocument/didChange", { textDocument: { uri: mainUri, version }, contentChanges: [{ text }] });
    await lsp.wait(isDiagnostics(mainUri, version));
    const diagnostics = diagnosticsOf(mainUri);
    log.push([`${label} diagnostics`, diagnostics]);
    const origin = { line: 0, character: 0 };
    await result(`${label} codeAction document`, "textDocument/codeAction", { ...document, range: { start: origin, end: endPosition(text) }, context: { diagnostics, triggerKind: 1 } });
    await result(`${label} codeAction fixAll`, "textDocument/codeAction", { ...document, range: { start: origin, end: origin }, context: { diagnostics: [], only: ["source.fixAll"], triggerKind: 1 } });
    for (const [index, diagnostic] of diagnostics.slice(0, 30).entries()) {
      await result(`${label} codeAction quickfix ${index}`, "textDocument/codeAction", { ...document, range: diagnostic.range, context: { diagnostics: [diagnostic], only: ["quickfix"], triggerKind: 2 } });
    }
  }
  // The first Copilot review of PR #122, after the original text is restored: two unsaved buffers in the namespace
  // `Sample`, a module named like the opt-in `Tensor` that declares a value only, whose module path hides the std
  // `Tensor` type from that namespace, and a module with the misspelt types `Tensr` (no candidate `Tensor`) and
  // `Tickr` (`Ticker`, which no module hides).
  version++;
  lsp.notify("textDocument/didChange", { textDocument: { uri: mainUri, version }, contentChanges: [{ text: original }] });
  await lsp.wait(isDiagnostics(mainUri, version));
  const shadowing = [
    [join(project, "Tensor.tz"), "namespace Sample\ndef shadow_value :: i64 = 1\n"],
    [join(project, "ShadowProbe.tz"), "namespace Sample\ndef shadow_probe_1 :: ref Tensr -> i64 = \\_x -> 0\n" +
      "def shadow_probe_2 :: ref Tickr -> i64 = \\_x -> 0\n"],
  ];
  for (const [path] of shadowing) assert.ok(!existsSync(path), `${path} exists`);
  for (const [path, text] of shadowing) {
    lsp.notify("textDocument/didOpen", { textDocument: { uri: uriOf(path), languageId: "tsuzuri", version: 1, text } });
  }
  const [probe, probeText] = shadowing[1];
  const probeDocument = { textDocument: { uri: uriOf(probe) } };
  await lsp.wait(isDiagnostics(uriOf(probe), 1));
  const diagnostics = diagnosticsOf(uriOf(probe));
  log.push(["shadowed diagnostics", diagnostics]);
  const origin = { line: 0, character: 0 };
  await result("shadowed codeAction document", "textDocument/codeAction", { ...probeDocument, range: { start: origin, end: endPosition(probeText) }, context: { diagnostics, triggerKind: 1 } });
  await result("shadowed codeAction fixAll", "textDocument/codeAction", { ...probeDocument, range: { start: origin, end: origin }, context: { diagnostics: [], only: ["source.fixAll"], triggerKind: 1 } });
  for (const [index, diagnostic] of diagnostics.entries()) {
    await result(`shadowed codeAction quickfix ${index}`, "textDocument/codeAction", { ...probeDocument, range: diagnostic.range, context: { diagnostics: [diagnostic], only: ["quickfix"], triggerKind: 2 } });
  }
  const publishes = lsp.messages.filter((message) => message.method === "textDocument/publishDiagnostics")
    .map((message) => [relative(project, fileURLToPath(message.params.uri)), message.params.version ?? null, message.params.diagnostics]);
  const exit = await lsp.shutdown();
  assert.equal(exit.code, 0, `the server of ${project} exited with ${exit.code ?? exit.signal}:\n${lsp.stderr}`);
  return { project: relative(root, project), log, publishes };
}

async function lspTranscripts(compiler, out, { threads, workloads, projects }) {
  const all = readWorkloads(workloads);
  const names = projects ?? Object.keys(all).filter((name) => !name.endsWith("-allstd"));
  assert.ok(names.length > 0, `no project to record in ${workloads}`);
  for (const name of names) assert.ok(all[name], `unknown workload ${name}`);
  mkdirSync(out, { recursive: true });
  for (const name of names) {
    const value = await transcript(compiler, all[name], environment(threads));
    const text = JSON.stringify(value);
    writeFileSync(join(out, `${name}.json`), text + "\n");
    const hints = value.log.filter(([label]) => label.startsWith("inlayHint ")).reduce((sum, [, hints]) => sum + (Array.isArray(hints) ? hints.length : 0), 0);
    console.log(JSON.stringify({ threads, project: name, requests: value.log.length, publishes: value.publishes.length, hints, sha256: sha256(text) }));
    const types = await typeTranscript(compiler, all[name], environment(threads));
    const typesText = JSON.stringify(types);
    writeFileSync(join(out, `${name}.types.json`), typesText + "\n");
    console.log(JSON.stringify({ threads, project: `${name}.types`, requests: types.log.length, publishes: types.publishes.length, sha256: sha256(typesText) }));
  }
}

// The entries of a CLI golden, one per line. A blank line, a line that is not a whole entry, an entry of a check that did
// not finish (killed by the timeout or a signal: no exit status) or a file without entries is refused: it would compare
// equal to another such file without comparing any output of the compiler. A nonzero exit status is a finished check.
function cliEntries(file) {
  const text = readFileSync(file, "utf8");
  assert.ok(text.length > 0 && text !== "\n", `${file} has no entries`);
  assert.ok(text.endsWith("\n"), `${file} does not end with a newline`);
  return text.slice(0, -1).split("\n").map((line, index) => {
    assert.ok(line.trim().length > 0, `line ${index + 1} of ${file} is blank`);
    const entry = JSON.parse(line);
    const object = (value) => value !== null && typeof value === "object" && !Array.isArray(value);
    assert.ok(object(entry) && typeof entry.target === "string" && entry.target.length > 0
      && Number.isSafeInteger(entry.status) && entry.signal === null
      && typeof entry.stdout === "string" && typeof entry.stderr === "string" && object(entry.kg),
    `line ${index + 1} of ${file} is not an entry of a finished check (target, an exit status and no signal, stdout, stderr and kg)`);
    return entry;
  });
}

// A transcript of `lsp`: its requests (at least one) and publishes. A blank file or one without requests is refused.
function transcriptText(file) {
  const text = readFileSync(file, "utf8");
  let value = null;
  try {
    value = JSON.parse(text);
  } catch {}
  assert.ok(value !== null && Array.isArray(value.log) && value.log.length > 0 && Array.isArray(value.publishes),
    `${file} is not a transcript (log with at least one request, publishes)`);
  return text;
}

function compare(left, right) {
  const pairs = [];
  if (statSync(left).isDirectory()) {
    const names = new Set([...readdirSync(left), ...readdirSync(right)].filter((name) => name.endsWith(".json")));
    assert.ok(names.size > 0, `${left} and ${right} hold no transcripts`);
    for (const name of [...names].sort()) pairs.push([join(left, name), join(right, name)]);
  } else {
    pairs.push([left, right]);
  }
  let same = 0, differ = 0;
  for (const [a, b] of pairs) {
    if (!existsSync(a) || !existsSync(b)) {
      differ++;
      console.log(`MISSING ${existsSync(a) ? b : a}`);
      continue;
    }
    if (a.endsWith(".jsonl")) {
      const [x, y] = [cliEntries(a), cliEntries(b)];
      assert.deepEqual(x.map((entry) => entry.target), y.map((entry) => entry.target), `${a} and ${b} have different targets`);
      for (const [index, entry] of x.entries()) {
        if (JSON.stringify(entry) === JSON.stringify(y[index])) same++;
        else {
          differ++;
          for (const key of ["status", "signal", "stdout", "stderr", "kg"]) {
            if (JSON.stringify(entry[key]) !== JSON.stringify(y[index][key])) {
              console.log(`DIFF ${entry.target} ${key}: ${JSON.stringify(entry[key]).slice(0, 300)} ||| ${JSON.stringify(y[index][key]).slice(0, 300)}`);
            }
          }
        }
      }
    } else {
      const [x, y] = [transcriptText(a), transcriptText(b)];
      if (x === y) same++;
      else {
        differ++;
        const [p, q] = [JSON.parse(x), JSON.parse(y)];
        const at = p.log.findIndex((entry, index) => JSON.stringify(entry) !== JSON.stringify(q.log[index]));
        const publish = p.publishes.findIndex((entry, index) => JSON.stringify(entry) !== JSON.stringify(q.publishes[index]));
        console.log(`DIFF ${a}: first differing request ${at >= 0 ? p.log[at][0] : "(none)"}, publish ${publish}, lengths ${p.log.length}/${q.log.length} ${p.publishes.length}/${q.publishes.length}`);
      }
    }
  }
  console.log(JSON.stringify({ left, right, same, differ }));
  return differ;
}

const args = process.argv.slice(2);
const command = args.shift();
const threads = threadSetting(option(args, "--threads", command === "cli" ? "auto" : "unset"));
const workloads = option(args, "--workloads", join(root, "target/perf/PB08/projects"));
const jobs = jobCount(option(args, "--jobs", "1"));
const projects = option(args, "--projects", null)?.split(",");
if (command === "cli") {
  assert.equal(args.length, 2, "usage: golden.mjs cli <compiler> <out.jsonl> [--threads 1|auto|N] [--workloads DIR] [--jobs N]");
  await cli(resolve(args[0]), resolve(args[1]), { threads, workloads, jobs });
} else if (command === "lsp") {
  assert.equal(args.length, 2, "usage: golden.mjs lsp <compiler> <out-dir> [--threads unset|N] [--workloads DIR] [--projects a,b]");
  await lspTranscripts(resolve(args[0]), resolve(args[1]), { threads, workloads, projects });
} else if (command === "compare") {
  assert.equal(args.length, 2, "usage: golden.mjs compare <a> <b>");
  process.exitCode = compare(resolve(args[0]), resolve(args[1])) ? 1 : 0;
} else {
  throw new Error("usage: golden.mjs cli|lsp|compare ...");
}
