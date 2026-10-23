import { spawnSync } from "node:child_process";
import { isAbsolute, relative, sep } from "node:path";

export const GREP_MAX_LINES = 100;
export const CHUNK_LINES = 500;
export const WINDOW_LINES = 40;
export const DIAG_V2_WINDOW = 5;
export const CHILD_TIMEOUT_MS = 60000;

export function countText(text) {
  let ascii = 0, other = 0;
  for (const ch of text) {
    if (ch.codePointAt(0) < 128) ascii++;
    else other++;
  }
  return { chars: ascii + other, ascii, other };
}
export const estimateTokens = (ascii, other) => Math.floor((10 * ascii + 52 * other + 39) / 40);
export function lowerMedian(values) {
  if (!values.length) throw new Error("lowerMedian needs at least one value");
  return [...values].sort((a, b) => a - b)[Math.floor((values.length - 1) / 2)];
}

export function invoke(compiler, args, cwd) {
  const result = spawnSync(compiler, args, {
    cwd, encoding: "utf8", input: "", timeout: CHILD_TIMEOUT_MS, maxBuffer: 16 * 1024 * 1024,
  });
  if (result.error?.code === "ETIMEDOUT") throw new Error(`tsuzuri ${args.join(" ")} timed out`);
  if (result.error || result.signal || result.status === null) {
    throw new Error(`tsuzuri ${args.join(" ")} failed: ${result.error?.code ?? result.signal}`);
  }
  return result;
}

export class Agent {
  constructor(tree, compiler = null) {
    this.tree = tree;
    this.compiler = compiler;
    this.calls = 0;
    this.ascii = 0;
    this.other = 0;
    this.seen = new Map();
  }
  emit(tool, text) {
    const count = countText(text);
    this.calls++;
    this.ascii += count.ascii;
    this.other += count.other;
    return text;
  }
  mark(path, from, to) {
    if (!this.seen.has(path)) this.seen.set(path, new Set());
    for (let line = from; line <= to; line++) this.seen.get(path).add(line);
  }
  grep(pattern, roots) {
    if (!pattern) throw new Error("grep pattern must not be empty");
    let text = "", matches = 0;
    for (const root of roots) for (const path of this.tree.files(root)) {
      const lines = this.tree.lines(path);
      for (let i = 0; i < lines.length; i++) if (lines[i].includes(pattern) && matches < GREP_MAX_LINES) {
        text += `${path}:${i + 1}:${lines[i]}\n`;
        this.mark(path, i + 1, i + 1);
        matches++;
      }
    }
    return this.emit("grep", text);
  }
  read(path, from, to) {
    const lines = this.tree.lines(path);
    from = Math.max(1, from);
    to = Math.min(lines.length, to);
    if (!Number.isInteger(from) || !Number.isInteger(to) || from > to) throw new Error(`empty read range: ${path}`);
    this.mark(path, from, to);
    return this.emit("read", lines.slice(from - 1, to).join("\n") + "\n");
  }
  chunk(path, k) {
    if (!Number.isInteger(k) || k < 1) throw new Error("chunk index must be positive");
    return this.read(path, (k - 1) * CHUNK_LINES + 1, k * CHUNK_LINES);
  }
  check(dir) {
    const result = invoke(this.compiler, ["check", ".", "--json"], dir);
    if (result.stdout || (result.status !== 0 && !result.stderr)) throw new Error("check did not return diagnostic JSON");
    const text = result.stderr.replace(/("path"\s*:\s*)("(?:[^"\\]|\\.)*")/g, (match, key, value) => {
      const path = JSON.parse(value);
      return isAbsolute(path) ? key + JSON.stringify("./" + relative(dir, path).split(sep).join("/")) : match;
    });
    return this.emit("check", text);
  }
  edit() { return this.emit("edit", "ok\n"); }
  fix(dir) {
    const result = invoke(this.compiler, ["fix", "."], dir);
    this.emit("fix", result.stdout + result.stderr);
    return result;
  }
  command(args, cwd) {
    const result = invoke(this.compiler, args, cwd);
    return this.emit("command", result.stdout + (result.status === 0 ? "" : result.stderr));
  }
  covers({ path, from, to }) {
    for (let i = from; i <= to; i++) if (!this.seen.get(path)?.has(i)) return false;
    return true;
  }
  get chars() { return this.ascii + this.other; }
  get tokens() { return estimateTokens(this.ascii, this.other); }
}
