import { basename, dirname, join } from "node:path";
import { Agent, invoke } from "./measure.mjs";
import { copyProject } from "./corpus.mjs";

export const KB_COMMANDS = {
  definition: (task, id) => ["kb", "show", id(task.kb.id)],
  callers: (task, id) => ["kb", "callers", id(task.symbol)],
  "type-search": (task) => ["kb", "find", task.find],
  impact: (task, id) => ["kb", "tests-for", id(task.change)],
};
export function kbProject(task) {
  const root = task.corpus[0] === "std" ? "benchmarks/ai/corpus/shop" : task.corpus[0];
  return /\.(tz|tt|tc)$/.test(root) ? dirname(root) : root;
}
export function prepareKb(ctx, task) {
  ctx.kbProjects ??= new Map();
  const project = kbProject(task);
  if (!ctx.kbProjects.has(project)) {
    const cwd = copyProject(ctx.tree.physical(project), join(ctx.work, "kb", `${ctx.kbProjects.size}`, basename(project)));
    invoke(ctx.compiler, ["kb", "build"], cwd);
    ctx.kbProjects.set(project, cwd);
  }
  const cwd = ctx.kbProjects.get(project);
  if (task.corpus[0] === "std" && !ctx.stdKbBuilt) {
    invoke(ctx.compiler, ["kb", "build", "--std"], cwd);
    ctx.stdKbBuilt = true;
  }
  return cwd;
}
export function resolveKbId(ctx, task, short, cwd = prepareKb(ctx, task)) {
  if (task.corpus[0] === "std") return short;
  const result = invoke(ctx.compiler, ["kb", "export", "--symbols"], cwd);
  if (result.status !== 0) return short;
  let ids;
  try { ids = result.stdout.split("\n").filter((line) => line.trim()).map((line) => JSON.parse(line).id); }
  catch { throw new Error("kb export returned invalid symbols JSON"); }
  const matches = ids.filter((id) => typeof id === "string" && (id === short || id.endsWith(`::${short}`)));
  if (matches.length !== 1) throw new Error(`kb id '${short}': ${matches.length} matches`);
  return matches[0];
}
export function kbSucceeded(task, out) {
  const signatureOf = (answer) => answer.includes(" :: ") ? answer.slice(answer.indexOf(" :: ") + 4) : answer;
  const needles = ["callers", "impact"].includes(task.type) ? task.expect :
    task.kb?.contains ?? [signatureOf(task.answer[0])];
  return needles.every((needle) => out.includes(needle));
}
export function kb(ctx, task, oriented = false) {
  const cwd = prepareKb(ctx, task), agent = new Agent(ctx.tree, ctx.compiler);
  if (oriented) agent.command(["kb", "brief", "--project"], cwd);
  const out = agent.command(KB_COMMANDS[task.type](task, (short) => resolveKbId(ctx, task, short, cwd)), cwd);
  return { agent, complete: kbSucceeded(task, out) };
}
