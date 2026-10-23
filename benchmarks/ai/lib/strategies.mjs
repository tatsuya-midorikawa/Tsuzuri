import { readFileSync } from "node:fs";
import { Tree, copyProject } from "./corpus.mjs";
import { Agent, WINDOW_LINES, DIAG_V2_WINDOW } from "./measure.mjs";
import { resolveNeeds, groupByPath } from "./tasks.mjs";
import { deriveCallers, deriveImpact, itemRange } from "./derive.mjs";
import { writeAtomically } from "../../report.mjs";
import { kb } from "./kb.mjs";

export { copyProject } from "./corpus.mjs";

export function taskInformation(task, tree) {
  if (task.type === "callers") return deriveCallers(task, tree);
  if (task.type === "impact") return deriveImpact(task, tree);
  return { grep: task.grep, needs: resolveNeeds(task.needs, tree) };
}
export function mergeWindows(windows) {
  const out = [];
  for (const [from, to] of [...windows].sort((a, b) => a[0] - b[0])) {
    if (out.length && from <= out.at(-1)[1] + 1) out.at(-1)[1] = Math.max(out.at(-1)[1], to);
    else out.push([from, to]);
  }
  return out;
}
function chunkUntilCovered(agent, needs) {
  for (const [path, list] of groupByPath(needs)) {
    const open = list.filter((need) => !agent.covers(need));
    for (let k = 1; !open.every((need) => agent.covers(need)); k++) agent.chunk(path, k);
  }
}
function baseline(ctx, task, mode) {
  const agent = new Agent(ctx.tree), { grep, needs } = taskInformation(task, ctx.tree);
  for (const pattern of grep) agent.grep(pattern, task.corpus);
  if (mode === "full") {
    for (const path of [...agent.seen.keys()].sort()) agent.read(path, 1, ctx.tree.lines(path).length);
    chunkUntilCovered(agent, needs);
  } else if (mode === "chunk") chunkUntilCovered(agent, needs);
  else for (const [path, list] of groupByPath(needs)) {
    const open = list.filter((need) => !agent.covers(need));
    const n = ctx.tree.lines(path).length;
    for (const [from, to] of mergeWindows(open.map((need) => [
      Math.max(1, need.from - WINDOW_LINES), Math.min(n, need.to + WINDOW_LINES),
    ]))) agent.read(path, from, to);
  }
  return { agent, complete: needs.every((need) => agent.covers(need)) };
}
function baselineDocs(ctx, task) {
  const agent = new Agent(ctx.tree), index = "_tsuzuri/language-reference/index.md";
  agent.read(index, 1, ctx.tree.lines(index).length);
  const needs = resolveNeeds(task.docs.needs, ctx.tree);
  chunkUntilCovered(agent, needs);
  return { agent, complete: needs.every((need) => agent.covers(need)) };
}
function diagnosticLines(text) {
  return text.split("\n").filter((line) => line.trim()).map((line) => {
    let value;
    try { value = JSON.parse(line); } catch { throw new Error("check returned a non-JSON diagnostic"); }
    if (!value || !["error", "warning"].includes(value.severity)) throw new Error("check returned an invalid diagnostic");
    return value;
  });
}
export function firstError(text) {
  const error = diagnosticLines(text).find((line) => line.severity === "error");
  if (!error) throw new Error("check returned no error diagnostic");
  return error;
}
export const hasError = (text) => diagnosticLines(text).some((line) => line.severity === "error");
export function applyEdits(dir, edits) {
  const tree = new Tree(dir);
  for (const edit of edits) {
    const path = tree.physical(edit.path), text = readFileSync(path, "utf8"), at = text.indexOf(edit.find);
    if (!edit.find || at < 0 || text.indexOf(edit.find, at + 1) >= 0) {
      throw new Error(`edit ${edit.path}: "${edit.find.slice(0, 40)}" must occur exactly once`);
    }
    writeAtomically(path, text.slice(0, at) + edit.replace + text.slice(at + edit.find.length));
  }
}
export function tryAttempts(agent, task, project) {
  const attempts = task.first_try ? [task.first_try, task.fix] : [task.fix];
  let rounds = 0;
  for (const edits of attempts) {
    const trial = copyProject(project, `${project}-trial`);
    applyEdits(trial, edits);
    agent.edit();
    const out = agent.check(trial);
    rounds++;
    if (!hasError(out)) return { agent, rounds, complete: true };
  }
  return { agent, rounds, complete: false };
}
function diagV2From(agent, first, task, project, tree) {
  const d = firstError(first);
  const spans = [d, ...(d.related ?? [])].map((span) => ({
    path: span.path.replace(/^\.\//, ""), from: span.span.line, to: span.span.end_line,
  }));
  for (const [path, list] of groupByPath(spans)) {
    for (const [from, to] of mergeWindows(list.map((span) => [
      Math.max(1, span.from - DIAG_V2_WINDOW), Math.min(tree.lines(path).length, span.to + DIAG_V2_WINDOW),
    ]))) agent.read(path, from, to);
  }
  for (const need of resolveNeeds(task.needs, tree)) if (!agent.covers(need)) {
    agent.read(need.path, need.from - DIAG_V2_WINDOW, need.to + DIAG_V2_WINDOW);
  }
  return { ...tryAttempts(agent, task, project), code: d.code };
}
export function diagV1(ctx, task) {
  const project = copyProject(ctx.tree.physical(task.project), `${ctx.work}/${task.id}`);
  const tree = new Tree(project), agent = new Agent(tree, ctx.compiler), first = agent.check(project);
  const d = firstError(first), path = d.path.replace(/^\.\//, ""), line = d.span.line;
  const [start, end] = itemRange(tree.lines(path), line);
  agent.read(path, Math.min(line - WINDOW_LINES, start), Math.max(line + WINDOW_LINES, end));
  for (const need of resolveNeeds(task.needs, tree)) if (!agent.covers(need)) {
    agent.read(need.path, need.from - WINDOW_LINES, need.to + WINDOW_LINES);
  }
  return { ...tryAttempts(agent, task, project), code: d.code };
}
export function diagV2(ctx, task) {
  const project = copyProject(ctx.tree.physical(task.project), `${ctx.work}/${task.id}`);
  const tree = new Tree(project), agent = new Agent(tree, ctx.compiler);
  return diagV2From(agent, agent.check(project), task, project, tree);
}
export function diagFix(ctx, task) {
  const project = copyProject(ctx.tree.physical(task.project), `${ctx.work}/${task.id}`);
  const tree = new Tree(project), agent = new Agent(tree, ctx.compiler), first = agent.check(project);
  const result = agent.fix(copyProject(project, `${project}-fix`));
  if (result.status === 0 && !result.stderr.includes("error[E")) {
    return { agent, rounds: 0, code: firstError(first).code, complete: true };
  }
  return diagV2From(agent, first, task, project, tree);
}
const baseTypes = ["definition", "callers", "type-search", "spec", "impact"];
const kbTypes = ["definition", "callers", "type-search", "impact"];
export function kbUnavailable(ctx, oriented) {
  const reasons = kbTypes.map((type) => ctx.caps.kb[type]).filter(Boolean);
  if (oriented && ctx.caps.kbBrief) return ctx.caps.kbBrief;
  return reasons.length === kbTypes.length ? reasons.join("; ") : null;
}
export const STRATEGIES = [
  { name: "baseline-grep", types: baseTypes, run: (ctx, task) => baseline(ctx, task, "full") },
  { name: "baseline-grep-chunk", types: baseTypes, run: (ctx, task) => baseline(ctx, task, "chunk") },
  { name: "baseline-grep-window", types: baseTypes, run: (ctx, task) => baseline(ctx, task, "window") },
  { name: "baseline-docs", types: ["definition", "spec"], applies: (task) => !!task.docs, run: baselineDocs },
  { name: "diag-v1", types: ["diagnostic"], run: diagV1 },
  { name: "diag-v2", types: ["diagnostic"], probe: (ctx) => ctx.caps.diagV2, run: diagV2 },
  { name: "diag-fix", types: ["diagnostic"], probe: (ctx) => ctx.caps.diagFix, run: diagFix },
  { name: "kb", types: kbTypes, probe: (ctx) => kbUnavailable(ctx, false),
    applies: (task, ctx) => !ctx.caps.kb[task.type] && (task.type !== "definition" || !!task.kb) &&
      (task.type !== "type-search" || !!task.find), run: (ctx, task) => kb(ctx, task) },
  { name: "kb-oriented", types: kbTypes, probe: (ctx) => kbUnavailable(ctx, true),
    applies: (task, ctx) => !ctx.caps.kb[task.type] && (task.type !== "definition" || !!task.kb) &&
      (task.type !== "type-search" || !!task.find), run: (ctx, task) => kb(ctx, task, true) },
];
