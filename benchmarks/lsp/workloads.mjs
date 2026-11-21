// The workloads of the language server's measurements and golden transcripts (PB08, "計測手順"), rebuilt
// from the repository: copies of three examples and five fixtures, `big` (six files, 215 KB, no Tsuzuri.toml)
// and `syn200` (`benchmarks/build/generate.mjs`, 201 files), and `<name>-allstd` variants for the CLI whose
// first line names every opt-in std module in a comment, so that `check` reads all of std as the language
// server does (`stdlib::sources_for` counts the identifiers of the text, comments included).
//
//   node benchmarks/lsp/workloads.mjs <directory> [--force]
import assert from "node:assert/strict";
import { cpSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { writeProject } from "../build/generate.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

// The Main.tz of `big`: six W1001 warnings that carry fixes (PB08; 412 bytes since E17 shortened its IO call, 415 in PB08).
export const BIG_MAIN = `def alpha :: own i64 -> i64 = \\n -> {
    let first = n + 1;
    let second = n + 2;
    n
}

def beta :: own i64 -> i64 = \\m -> {
    let third = m * 2;
    let fourth = m * 3;
    m
}

def gamma :: own i64 -> own i64 -> i64 = \\a b -> a

def delta :: own i64 -> i64 = \\k -> {
    let fifth = k - 1;
    k
}

def main :: own unit -> i32 = \\() ->
    do! IO.writeln (alpha 1 + beta 2 + gamma 3 4 + delta 5)
    0
`;

// The other files of `big`: these fixtures' Main.tz, byte for byte.
export const BIG_COPIES = Object.freeze([
  ["array_fsharp", "ArrayF.tz"], ["vec_dotnet", "VecD.tz"], ["in_place_writes", "InPlace.tz"],
  ["json", "JsonT.tz"], ["list_fsharp", "ListF.tz"],
]);

// name -> [kind, source]. `copy` copies a directory of the repository (without `target/`).
export const WORKLOADS = Object.freeze({
  hello: ["copy", "examples/hello"],
  point: ["copy", "examples/point"],
  polymorphism: ["copy", "examples/polymorphism"],
  conventions: ["copy", "tests/fixtures/conventions"],
  json: ["copy", "tests/fixtures/json"],
  anonymous_unions: ["copy", "tests/fixtures/anonymous_unions"],
  list_fsharp: ["copy", "tests/fixtures/list_fsharp"],
  in_place_writes: ["copy", "tests/fixtures/in_place_writes"],
  big: ["big"],
  syn200: ["synthetic", 200],
});

// The workloads whose all-std variant the CLI measures (the "CLI 全 std" column of the ticket).
export const ALL_STD = Object.freeze(["hello", "big"]);

// Every name that makes `check` read an opt-in std module, from `OPT_IN` in src/stdlib.rs.
export function optInNames() {
  const source = readFileSync(join(root, "src/stdlib.rs"), "utf8");
  const start = source.indexOf("pub(crate) const OPT_IN: &[OptIn] = &[");
  assert.ok(start >= 0, "OPT_IN not found in src/stdlib.rs");
  const block = source.slice(start, source.indexOf("\n];", start));
  const names = new Set();
  for (const [, list] of block.matchAll(/\bnames: &\[([^\]]*)\]/g)) {
    for (const [, name] of list.matchAll(/"([A-Za-z][A-Za-z0-9]*)"/g)) names.add(name);
  }
  assert.ok(names.size >= 30, `only ${names.size} opt-in names in src/stdlib.rs`);
  return [...names];
}

function writeWorkload(directory, name) {
  const [kind, source] = WORKLOADS[name];
  const project = join(directory, name);
  if (kind === "copy") {
    const from = join(root, source);
    cpSync(from, project, { recursive: true, filter: (path) => !relative(from, path).split(/[\\/]/).includes("target") });
  } else if (kind === "big") {
    mkdirSync(project);
    writeFileSync(join(project, "Main.tz"), BIG_MAIN);
    for (const [fixture, file] of BIG_COPIES) cpSync(join(root, "tests/fixtures", fixture, "Main.tz"), join(project, file));
  } else {
    writeProject(project, source);
  }
  return project;
}

// Writes every workload (and the all-std variants) under `directory`; returns name -> project directory.
export function writeWorkloads(directory, { force = false } = {}) {
  directory = resolve(directory);
  if (existsSync(directory)) {
    assert.ok(force, `${directory} exists; pass --force to rebuild it`);
    rmSync(directory, { recursive: true, force: true });
  }
  mkdirSync(directory, { recursive: true });
  const projects = {};
  for (const name of Object.keys(WORKLOADS)) projects[name] = writeWorkload(directory, name);
  const comment = `// ${optInNames().join(" ")}\n`;
  for (const name of ALL_STD) {
    const project = join(directory, `${name}-allstd`);
    cpSync(projects[name], project, { recursive: true });
    const main = join(project, "Main.tz");
    writeFileSync(main, comment + readFileSync(main, "utf8"));
    projects[`${name}-allstd`] = project;
  }
  writeFileSync(join(directory, "workloads.json"), JSON.stringify({ projects: Object.keys(projects), optIn: comment.trim() }, null, 2) + "\n");
  return projects;
}

// The project directories of a directory that `writeWorkloads` wrote.
export function readWorkloads(directory) {
  directory = resolve(directory);
  const manifest = join(directory, "workloads.json");
  assert.ok(existsSync(manifest), `${directory} has no workloads; run node benchmarks/lsp/workloads.mjs ${directory}`);
  return Object.fromEntries(JSON.parse(readFileSync(manifest, "utf8")).projects.map((name) => [name, join(directory, name)]));
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2);
  const directory = args.find((arg) => !arg.startsWith("--"));
  assert.ok(directory, "usage: node benchmarks/lsp/workloads.mjs <directory> [--force]");
  const projects = writeWorkloads(directory, { force: args.includes("--force") });
  console.log(JSON.stringify({ directory: resolve(directory), projects: Object.keys(projects) }));
}
