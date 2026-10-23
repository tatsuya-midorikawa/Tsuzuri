import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, rmSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { writeAtomically } from "../report.mjs";
import { ROOT, Tree, isStablePath } from "./lib/corpus.mjs";
import { Agent, invoke } from "./lib/measure.mjs";
import { TYPES, loadTasks, resolveNeeds, isStableTask } from "./lib/tasks.mjs";
import { parseItems, deriveCallers, deriveImpact } from "./lib/derive.mjs";
import { STRATEGIES, taskInformation, copyProject, applyEdits, firstError, hasError } from "./lib/strategies.mjs";
import { makeRow, serializeRows, renderReport, updateReportDocument } from "./lib/report.mjs";
import { readExternal } from "./lib/external.mjs";
import { KB_COMMANDS } from "./lib/kb.mjs";
import { measureStatic } from "./lib/static.mjs";

const usage = "usage: node benchmarks/ai/run.mjs run|verify|report|task ID [options]";
function usageError(message) { const error = new Error(message); error.exitCode = 2; throw error; }
export function parseArguments(argv) {
  const command = argv[0];
  if (!["run", "verify", "report", "task"].includes(command)) usageError(`unknown command '${command ?? ""}'; use run, verify, report, or task`);
  const options = { command, compiler: join(ROOT, "target/release/tsuzuri"), out: join(ROOT, "target/ai-cost"),
    tasks: join(ROOT, "benchmarks/ai/tasks"), doc: join(ROOT, "docs/ai-cost.md"), strict: false, publish: false };
  let i = 1;
  if (command === "task") options.id = argv[i++];
  const allowed = {
    run: ["compiler", "out", "tasks", "types", "strategies"], verify: ["compiler", "tasks", "strict"],
    report: ["out", "doc", "publish"], task: ["tasks"],
  };
  const used = new Set();
  for (; i < argv.length; i++) {
    const flag = argv[i], key = flag.slice(2);
    if (!flag.startsWith("--") || !allowed[command].includes(key)) usageError(`unknown option '${flag}'`);
    if (used.has(key)) usageError(`duplicate option '${flag}'`);
    used.add(key);
    if (["strict", "publish"].includes(key)) { options[key] = true; continue; }
    const value = argv[++i];
    if (!value || value.startsWith("--")) usageError(`option '${flag}' needs a value`);
    options[key] = ["compiler", "out", "tasks", "doc"].includes(key) ? resolve(value) : value.split(",");
  }
  if (options.types) {
    options.types = options.types.map((letter) => letter.toUpperCase());
    if (options.types.some((letter) => !Object.hasOwn(TYPES, letter))) usageError("--types needs letters from A to F, comma separated");
  }
  for (const name of options.strategies ?? []) if (!STRATEGIES.some((strategy) => strategy.name === name)) {
    usageError(`unknown strategy '${name}'; known: ${STRATEGIES.map((strategy) => strategy.name).join(",")}`);
  }
  return options;
}
export function probe(compiler, args, cwd) {
  const result = invoke(compiler, args, cwd);
  return result.status === 0 || (result.status === 2 && result.stderr.includes("usage:"));
}
export function detectCapabilities(ctx) {
  const project = copyProject(ctx.tree.physical("benchmarks/ai/corpus/diag/c01"), join(ctx.work, "probe"));
  const first = firstError(new Agent(new Tree(project), ctx.compiler).check(project));
  const caps = {
    diagV2: first.related ? null : "check --json has no related spans",
    diagFix: probe(ctx.compiler, ["fix"], ctx.work) ? null : "tsuzuri fix is not available",
    kb: {},
    kbBrief: probe(ctx.compiler, ["kb", "brief"], ctx.work) ? null : "tsuzuri kb brief is not available",
  };
  const samples = { definition: { kb: { id: "" } }, callers: { symbol: "" }, "type-search": { find: "" }, impact: { change: "" } };
  for (const type of Object.keys(KB_COMMANDS)) {
    const args = KB_COMMANDS[type](samples[type], (id) => id).slice(0, 2);
    caps.kb[type] = probe(ctx.compiler, args, ctx.work) ? null : `tsuzuri ${args.join(" ")} is not available (${type})`;
  }
  return caps;
}
function makeContext(options, work) {
  mkdirSync(work, { recursive: true });
  return { compiler: options.compiler, tree: new Tree(), out: options.out, work, caps: {} };
}
export function verifyTask(task, ctx) {
  const tree = task.type === "diagnostic" ? new Tree(ctx.tree.physical(task.project)) : ctx.tree;
  const rows = task.needs ?? [];
  const files = task.type === "diagnostic" ? tree.files(".") : task.corpus.flatMap((root) => tree.files(root));
  for (const [path] of rows) if (!files.includes(path)) throw new Error(`task ${task.id}: need path ${path} is outside the corpus`);
  for (const [path] of task.docs?.needs ?? []) if (!path.startsWith("_tsuzuri/language-reference/")) {
    throw new Error(`task ${task.id}: need path ${path} is outside the corpus`);
  }
  const needs = resolveNeeds(rows, tree);
  if (task.docs) resolveNeeds(task.docs.needs, tree);
  const neededText = needs.map((need) => tree.lines(need.path).slice(need.from - 1, need.to).join("\n")).join("\n");
  for (const answer of task.answer ?? []) if (!neededText.includes(answer)) {
    throw new Error(`task ${task.id}: answer "${answer}" is not in the needed lines`);
  }
  for (const pattern of task.grep ?? []) if (!task.corpus.some((root) =>
    tree.files(root).some((path) => tree.lines(path).some((line) => line.includes(pattern))))) {
    throw new Error(`task ${task.id}: grep pattern "${pattern}" matches nothing`);
  }
  for (const [type, derive, key] of [["callers", deriveCallers, "callers"], ["impact", deriveImpact, "tests"]]) if (task.type === type) {
    const answer = derive(task, tree)[key];
    if (JSON.stringify(answer) !== JSON.stringify(task.expect)) {
      throw new Error(`task ${task.id}: derived ${key} differ from expect: ${JSON.stringify(answer)} vs ${JSON.stringify(task.expect)}`);
    }
  }
  if (task.type === "diagnostic") {
    const project = copyProject(tree.base, join(ctx.work, task.id));
    const d = firstError(new Agent(new Tree(project), ctx.compiler).check(project));
    if (d.code !== task.expect.code || d.span.line !== task.expect.line) {
      throw new Error(`task ${task.id}: expected first diagnostic ${task.expect.code} at line ${task.expect.line}, got ${d.code} at line ${d.span.line}`);
    }
    for (const key of ["fix", "first_try"]) if (task[key]) {
      const trial = copyProject(project, `${project}-${key}`);
      applyEdits(trial, task[key]);
      const error = hasError(new Agent(new Tree(trial), ctx.compiler).check(trial));
      if (key === "fix" && error) throw new Error(`task ${task.id}: fix leaves errors`);
      if (key === "first_try" && !error) throw new Error(`task ${task.id}: first_try does not fail`);
    }
  }
}
function verifyCorpus(root, ctx) {
  const result = invoke(ctx.compiler, ["check", ".", "--json"], ctx.tree.physical(root));
  if (result.status !== 0 || result.stderr || result.stdout) throw new Error(`corpus ${root}: check printed diagnostics`);
  const list = invoke(ctx.compiler, ["test", ".", "--list"], ctx.tree.physical(root));
  if (list.status !== 0) throw new Error(`corpus ${root}: test --list failed`);
  const actual = list.stdout.trim().split("\n").filter(Boolean).map((line) => line.replace(/^\d+\s+/, "")).sort();
  const expected = [...new Set(parseItems([root], ctx.tree).filter((item) => item.kind === "test").map((item) => item.id))].sort();
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw new Error(`corpus ${root}: test ids differ from tsuzuri test --list: ${JSON.stringify(actual)} vs ${JSON.stringify(expected)}`);
  }
}
function metadata(ctx, rows, skipped, skippedTasks) {
  const git = (args) => spawnSync("git", args, { cwd: ROOT, encoding: "utf8", timeout: 60000 });
  const commit = git(["rev-parse", "--short=12", "HEAD"]), status = git(["status", "--porcelain"]);
  return { schema: "tz-ai-meta/1", compiler: invoke(ctx.compiler, ["--version"], ROOT).stdout.split("\n")[0],
    commit: commit.status === 0 ? commit.stdout.trim() : "unknown", dirty: status.status !== 0 || !!status.stdout,
    rows: rows.length, skipped, skipped_tasks: skippedTasks };
}
async function run(options, tasks) {
  const work = join(options.out, "work"), marker = join(work, ".tz-ai-cost-work");
  if (existsSync(work) && !existsSync(marker)) throw new Error("refusing to replace an unmarked work directory");
  rmSync(work, { recursive: true, force: true });
  const ctx = makeContext(options, work), rows = [], skipped = [], skippedTasks = [];
  writeAtomically(marker, "tz-ai-cost/1\n");
  try {
    ctx.caps = detectCapabilities(ctx);
    const available = [];
    for (const strategy of STRATEGIES) {
      if (options.strategies && !options.strategies.includes(strategy.name)) continue;
      const reason = strategy.probe?.(ctx) ?? null;
      if (reason) {
        if (options.strategies) throw new Error(`strategy '${strategy.name}' is not available: ${reason}`);
        skipped.push({ strategy: strategy.name, reason });
      } else {
        available.push(strategy);
        if (strategy.name.startsWith("kb")) for (const reason of Object.values(ctx.caps.kb).filter(Boolean)) {
          skipped.push({ strategy: strategy.name, reason });
        }
      }
    }
    for (const task of tasks) {
      if (options.types && !options.types.includes(task.id[0])) continue;
      try {
        taskInformation(task, task.type === "diagnostic" ? new Tree(ctx.tree.physical(task.project)) : ctx.tree);
        if (task.docs) resolveNeeds(task.docs.needs, ctx.tree);
      } catch (error) {
        if (isStableTask(task)) throw error;
        skippedTasks.push({ task: task.id, reason: error.message });
        continue;
      }
      for (const strategy of available) if (strategy.types.includes(task.type) && (strategy.applies?.(task, ctx) ?? true)) {
        rows.push(makeRow(task, strategy, await strategy.run(ctx, task)));
      }
    }
    ctx.staticSkipped = [];
    const statics = await measureStatic(ctx);
    skipped.push(...ctx.staticSkipped);
    mkdirSync(options.out, { recursive: true });
    writeAtomically(join(options.out, "results.jsonl"), serializeRows(rows));
    writeAtomically(join(options.out, "static.jsonl"), serializeRows(statics));
    writeAtomically(join(options.out, "meta.json"), JSON.stringify(metadata(ctx, rows, skipped, skippedTasks)) + "\n");
    console.log(`wrote ${rows.length} rows to ${relative(process.cwd(), options.out) || "."}/results.jsonl`);
    for (const { strategy, reason } of skipped) console.log(`skipped strategy ${strategy}: ${reason}`);
    for (const { task, reason } of skippedTasks) console.log(`skipped task ${task}: ${reason}`);
  } finally { rmSync(work, { recursive: true, force: true }); }
}
async function verify(options, tasks) {
  const work = join(ROOT, `target/scratch/ai-cost-verify-${process.pid}`), ctx = makeContext(options, work);
  let failures = 0, drift = 0;
  try {
    for (const task of tasks) {
      try { verifyTask(task, ctx); }
      catch (error) {
        if (isStableTask(task)) failures++; else { drift++; if (options.strict) failures++; }
        console.log(`${isStableTask(task) ? "failure" : "drift"}: ${error.message}`);
      }
    }
    for (const root of [...new Set(tasks.flatMap((task) => task.corpus ?? []).filter(isStablePath))].sort()) {
      try { verifyCorpus(root, ctx); } catch (error) { failures++; console.log(`failure: ${error.message}`); }
    }
    console.log(`verified ${tasks.length} tasks: ${failures} failures, ${drift} drift`);
    return failures ? 1 : 0;
  } finally { rmSync(work, { recursive: true, force: true }); }
}
function report(options, tasks) {
  const path = join(options.out, "results.jsonl");
  if (!existsSync(path)) throw new Error(`${path} not found; run 'run' first`);
  const readRows = (file) => readFileSync(file, "utf8").split("\n").filter((line) => line.trim()).map((line) => JSON.parse(line));
  const generated = renderReport({
    rows: readRows(path), meta: JSON.parse(readFileSync(join(options.out, "meta.json"), "utf8")),
    statics: existsSync(join(options.out, "static.jsonl")) ? readRows(join(options.out, "static.jsonl")) : [],
    externals: readExternal(join(options.out, "external"), tasks.map((task) => task.id)), taskCount: tasks.length,
  });
  if (options.publish) {
    writeAtomically(options.doc, updateReportDocument(readFileSync(options.doc, "utf8"), generated));
    console.log(`updated ${relative(process.cwd(), options.doc)}`);
  } else process.stdout.write(generated);
}
export async function main(argv) {
  try {
    const options = parseArguments(argv), tasks = loadTasks(options.tasks);
    if (options.command === "task") {
      const task = tasks.find((task) => task.id === options.id);
      if (!task) usageError(`unknown task '${options.id ?? ""}'`);
      console.log(JSON.stringify({ id: task.id, type: task.type, prompt: task.prompt,
        ...(task.project ? { project: task.project } : { corpus: task.corpus }) }));
    } else if (options.command === "run") await run(options, tasks);
    else if (options.command === "verify") return await verify(options, tasks);
    else report(options, tasks);
    return 0;
  } catch (error) {
    console.error(`error: ${error.message.replaceAll("\n", " ")}`);
    if (error.exitCode === 2) console.error(usage);
    return error.exitCode ?? 1;
  }
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  process.exitCode = await main(process.argv.slice(2));
}
