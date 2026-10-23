import { basename, extname } from "node:path";

const KEYWORD = /^(?:def|fn|and|private|export|const|record|union|class|instance|type|test|bench|extern|namespace|using)\b/;
const DECLARATION = /^(?:(?:private|export|const)\s+)*(def|fn|and)\s+(?:rec\s+)?([A-Za-z_][A-Za-z0-9_]*)/;
const TEST_HEAD = /^test\s+"((?:[^"\\]|\\.)*)"/;

export function stripComment(line) {
  let inString = false;
  for (let i = 0; i < line.length; i++) {
    if (inString && line[i] === "\\") { i++; continue; }
    if (line[i] === '"') inString = !inString;
    else if (!inString && line[i] === "/" && line[i + 1] === "/") return line.slice(0, i);
  }
  return line;
}
export function parseItems(roots, tree) {
  const items = [];
  for (const root of roots) for (const path of tree.files(root)) {
    if (![".tz", ".tt", ".tc"].includes(extname(path))) continue;
    const module = basename(path, extname(path)), lines = tree.lines(path).map(stripComment);
    const starts = [];
    for (let i = 0; i < lines.length; i++) if (KEYWORD.test(lines[i])) starts.push(i + 1);
    for (let k = 0; k < starts.length; k++) {
      const from = starts[k];
      let to = (starts[k + 1] ?? lines.length + 1) - 1;
      while (to > from && !lines[to - 1].trim()) to--;
      const decl = DECLARATION.exec(lines[from - 1]), test = TEST_HEAD.exec(lines[from - 1]);
      if (!decl && !test) continue;
      const name = decl ? decl[2] : test[1];
      items.push({ path, module, name, id: `${module}.${name}`, from, to,
        lines: lines.slice(from - 1, to), kind: decl ? "function" : "test" });
    }
  }
  return items;
}
export function referenceLines(item, symbol) {
  const [module, name] = symbol.split(".");
  const qualified = new RegExp(`(?<![A-Za-z0-9_.])${module}\\.${name}(?![A-Za-z0-9_])`);
  const bare = new RegExp(`(?<![A-Za-z0-9_.])${name}(?![A-Za-z0-9_])`);
  const hits = [];
  item.lines.forEach((line, i) => {
    if (qualified.test(line) || (item.module === module && bare.test(line))) hits.push(item.from + i);
  });
  return hits;
}
export function callersOf(items, symbol) {
  const found = new Map();
  for (const item of items) if (item.kind === "function" && item.id !== symbol) {
    const refs = referenceLines(item, symbol);
    if (!refs.length) continue;
    if (!found.has(item.id)) found.set(item.id, []);
    found.get(item.id).push({ item, last: refs.at(-1) });
  }
  return new Map([...found.keys()].sort().map((id) => [id, found.get(id)]));
}
const needOf = ({ item, last }) => ({ path: item.path, from: item.from, to: last });
export function deriveCallers(task, tree) {
  const callers = callersOf(parseItems(task.corpus, tree), task.symbol);
  return { callers: [...callers.keys()], needs: [...callers.values()].flat().map(needOf),
    grep: [task.symbol.split(".")[1]] };
}
export function deriveImpact(task, tree) {
  const items = parseItems(task.corpus, tree);
  const parent = new Map([[task.change, null]]), order = [task.change], tests = [];
  for (let head = 0; head < order.length; head++) {
    const target = order[head];
    for (const [id, entries] of callersOf(items, target)) if (!parent.has(id)) {
      parent.set(id, { callee: target, entries });
      order.push(id);
    }
    for (const item of items) if (item.kind === "test" && !parent.has(item.id)) {
      const refs = referenceLines(item, target);
      if (!refs.length) continue;
      parent.set(item.id, { callee: target, entries: [{ item, last: refs.at(-1) }] });
      tests.push(item.id);
    }
  }
  tests.sort();
  const needs = new Map();
  for (let cur of tests) while (parent.get(cur)) {
    const { callee, entries } = parent.get(cur);
    for (const entry of entries) {
      const need = needOf(entry), key = JSON.stringify([need.path, need.from]);
      if (!needs.has(key) || needs.get(key).to < need.to) needs.set(key, need);
    }
    cur = callee;
  }
  return { tests, needs: [...needs.values()], grep: order.filter((id) =>
    items.some((item) => item.kind === "function" && item.id === id)).map((id) => id.split(".")[1]) };
}
export function itemRange(lines, line) {
  let from = line, to = line;
  while (from > 1 && !KEYWORD.test(lines[from - 1])) from--;
  if (!KEYWORD.test(lines[from - 1])) return [line, line];
  while (to < lines.length && !KEYWORD.test(lines[to])) to++;
  while (to > from && !lines[to - 1].trim()) to--;
  return [from, to];
}
