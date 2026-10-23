import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { ROOT, isStablePath, logicalPath } from "./corpus.mjs";

export const TYPES = {
  A: { name: "definition", file: "a-definition.json" },
  B: { name: "callers", file: "b-callers.json" },
  C: { name: "diagnostic", file: "c-diagnostic.json" },
  D: { name: "type-search", file: "d-type-search.json" },
  E: { name: "spec", file: "e-spec.json" },
  F: { name: "impact", file: "f-impact.json" },
};
const required = {
  definition: ["corpus", "grep", "needs", "answer"],
  callers: ["corpus", "symbol", "expect"],
  diagnostic: ["project", "expect", "needs", "fix"],
  "type-search": ["corpus", "grep", "needs", "answer"],
  spec: ["corpus", "grep", "needs", "answer"],
  impact: ["corpus", "change", "expect"],
};
const optional = {
  definition: ["docs", "kb"], callers: [], diagnostic: ["first_try"],
  "type-search": ["find", "kb"], spec: ["docs"], impact: [],
};
const object = (x) => x !== null && typeof x === "object" && !Array.isArray(x);
const text = (x) => typeof x === "string" && x.length > 0;
const strings = (x) => Array.isArray(x) && x.every(text);
const natural = (x) => Number.isSafeInteger(x) && x >= 0;
const symbol = (x) => typeof x === "string" && /^[A-Za-z][A-Za-z0-9_]*\.[A-Za-z_][A-Za-z0-9_]*$/.test(x);

function validateTask(task, type, ids) {
  const fail = (message) => { throw new Error(`task ${task?.id ?? "?"}: ${message}`); };
  if (!object(task)) fail("must be an object");
  const keys = ["id", "type", "prompt", ...required[type], ...optional[type]];
  for (const key of Object.keys(task)) if (!keys.includes(key)) fail(`unknown key '${key}'`);
  for (const key of ["id", "type", "prompt", ...required[type]]) if (!(key in task)) fail(`missing key '${key}'`);
  const letter = Object.keys(TYPES).find((key) => TYPES[key].name === type);
  if (!new RegExp(`^${letter}-[0-9]{2}$`).test(task.id) || task.type !== type) fail("id and type must match the task file");
  if (ids.has(task.id)) fail("duplicate id");
  ids.add(task.id);
  if (!text(task.prompt)) fail("prompt must be a non-empty string");
  for (const key of ["corpus", "grep", "answer"]) if (key in task &&
      (!strings(task[key]) || !task[key].length)) fail(`${key} must be a non-empty string array`);
  for (const path of task.corpus ?? []) logicalPath(path);
  if (task.project !== undefined) {
    logicalPath(task.project);
    if (!/^benchmarks\/ai\/corpus\/diag\/c[0-9]{2}$/.test(task.project)) fail("project must be a diagnostic corpus");
  }
  const needs = (rows) => {
    if (!Array.isArray(rows) || !rows.length) fail("needs must be a non-empty array");
    for (const row of rows) {
      if (!Array.isArray(row) || row.length !== 4 || !text(row[0]) || !text(row[1]) ||
          !natural(row[2]) || !natural(row[3])) fail("invalid need");
      logicalPath(row[0]);
    }
  };
  if (task.needs) needs(task.needs);
  if (task.docs !== undefined) {
    if (!object(task.docs) || Object.keys(task.docs).join() !== "needs") fail("invalid docs");
    needs(task.docs.needs);
  }
  if (task.kb !== undefined) {
    if (!object(task.kb) || !symbol(task.kb.id) ||
        Object.keys(task.kb).some((key) => !["id", "contains"].includes(key)) ||
        ("contains" in task.kb && (!strings(task.kb.contains) || !task.kb.contains.length))) fail("invalid kb");
  }
  if (task.find !== undefined && !text(task.find)) fail("find must be a non-empty string");
  for (const key of ["symbol", "change"]) if (key in task && !symbol(task[key])) fail(`invalid ${key}`);
  if (["callers", "impact"].includes(type)) {
    if (!strings(task.expect) || JSON.stringify(task.expect) !== JSON.stringify([...new Set(task.expect)].sort())) fail("expect must be sorted and unique");
  }
  if (type === "diagnostic") {
    if (!object(task.expect) || !/^E[0-9]{4}$/.test(task.expect.code) ||
        !natural(task.expect.line) || task.expect.line < 1 ||
        Object.keys(task.expect).some((key) => !["code", "line"].includes(key))) fail("invalid diagnostic expect");
    for (const key of ["fix", "first_try"]) if (key in task) {
      if (!Array.isArray(task[key]) || !task[key].length) fail(`invalid ${key}`);
      for (const edit of task[key]) {
        if (!object(edit) || !text(edit.path) || !text(edit.find) || typeof edit.replace !== "string" ||
            Object.keys(edit).some((k) => !["path", "find", "replace"].includes(k))) fail("invalid edit");
        logicalPath(edit.path);
      }
    }
  }
}

export function loadTasks(dir = join(ROOT, "benchmarks/ai/tasks")) {
  const tasks = [], ids = new Set();
  for (const file of readdirSync(dir).filter((name) => name.endsWith(".json")).sort()) {
    let data;
    try { data = JSON.parse(readFileSync(join(dir, file), "utf8")); }
    catch { throw new Error(`${file}: not valid JSON`); }
    if (!object(data) || data.schema !== "tz-ai-task/1" || !Object.hasOwn(required, data.type) ||
        !Array.isArray(data.tasks) || Object.keys(data).some((key) => !["schema", "type", "tasks"].includes(key))) {
      throw new Error(`${file}: invalid tz-ai-task/1 file`);
    }
    for (const task of data.tasks) {
      validateTask(task, data.type, ids);
      tasks.push(task);
    }
  }
  return tasks.sort((a, b) => a.id < b.id ? -1 : a.id > b.id ? 1 : 0);
}

export function resolveNeeds(rows, tree) {
  return rows.map(([path, match, before, after]) => {
    const lines = tree.lines(path), hits = [];
    for (let i = 0; i < lines.length; i++) if (lines[i].includes(match)) hits.push(i + 1);
    if (hits.length !== 1) throw new Error(`need ${path} "${match}": ${hits.length} matching lines`);
    return { path, from: Math.max(1, hits[0] - before), to: Math.min(lines.length, hits[0] + after) };
  });
}
export function groupByPath(needs) {
  const groups = new Map();
  for (const need of needs) {
    if (!groups.has(need.path)) groups.set(need.path, []);
    groups.get(need.path).push(need);
  }
  return [...groups.keys()].sort().map((path) => [path, groups.get(path)]);
}
export const isStableTask = (task) => task.type === "diagnostic" ||
  (task.corpus.length > 0 && task.corpus.every(isStablePath));
