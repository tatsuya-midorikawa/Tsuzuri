import { cpSync, existsSync, lstatSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, extname, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

export const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const extensions = new Set([".tz", ".tt", ".tc", ".md", ".toml", ".json"]);
const excluded = (name) => name.startsWith(".") || name === "target" || name === "node_modules";

export function logicalPath(path) {
  if (typeof path !== "string" || !path || path.includes("\\") ||
      path.includes("\0") || path.startsWith("/") || /^[A-Za-z]:/.test(path) ||
      path.split("/").some((part) => part === ".." || part === "")) {
    throw new Error(`invalid logical path '${path}'`);
  }
  return path.replace(/^\.\//, "");
}

export class Tree {
  constructor(base = ROOT) {
    this.base = resolve(base);
    this.cache = new Map();
  }
  physical(path) {
    path = logicalPath(path);
    let physical = this.base;
    for (const part of path.split("/")) {
      physical = resolve(physical, part);
      if (existsSync(physical) && lstatSync(physical).isSymbolicLink()) {
        throw new Error(`symbolic link is not a corpus file: ${path}`);
      }
    }
    return physical;
  }
  files(root) {
    root = logicalPath(root);
    const physical = this.physical(root);
    if (!existsSync(physical)) throw new Error(`corpus root '${root}' does not exist`);
    const files = [];
    const walk = (path) => {
      const stat = lstatSync(this.physical(path));
      if (stat.isDirectory()) {
        for (const name of readdirSync(this.physical(path)).sort()) {
          if (!excluded(name)) walk(path === "." ? name : `${path}/${name}`);
        }
      } else if (stat.isFile() && extensions.has(extname(path))) files.push(path);
    };
    if (root === "." || !root.split("/").some(excluded)) walk(root);
    return files.sort();
  }
  lines(path) {
    if (!this.cache.has(path)) {
      let lines;
      try { lines = readFileSync(this.physical(path), "utf8").split("\n"); }
      catch (error) {
        if (!error.code) throw error;
        throw new Error(`corpus file '${path}' cannot be read (${error.code})`);
      }
      if (lines.at(-1) === "") lines.pop();
      this.cache.set(path, lines);
    }
    return this.cache.get(path);
  }
}

export const isStablePath = (path) => path.startsWith("benchmarks/ai/corpus/");

export function copyProject(source, target) {
  rmSync(target, { recursive: true, force: true });
  cpSync(source, target, { recursive: true });
  if (resolve(source).startsWith(resolve(ROOT, "benchmarks/ai/corpus") + sep)) {
    writeFileSync(resolve(target, "Tsuzuri.toml"),
      '[package]\nname = "ai-corpus"\nversion = "0.1.0"\nedition = "2026"\n');
  }
  return target;
}
