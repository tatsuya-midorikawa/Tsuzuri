// Compares the LLVM IR that two compilers emit for the same projects (PM01 identity steps).
//
// usage: node benchmarks/layout/ir-diff.mjs <before-compiler> <after-compiler> [--out dir] [--opt -O0[,-O3]] [--wasm] [--lines N] [project...]
//
// Without projects it takes every directory of `tests/fixtures`, `examples`, and `benchmarks/layout|computations`.
// A project that neither compiler builds counts as the same when both fail with the same first diagnostic line.
// Exit status 1 when some IR differs. The IR of both compilers is kept under `<out>/before` and `<out>/after`.
import { execFile } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, readdirSync, statSync } from "node:fs";
import { basename, dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

const execute = promisify(execFile);
const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const usage = "Usage: node benchmarks/layout/ir-diff.mjs <before-compiler> <after-compiler> [--out dir] [--opt -O0[,-O3]] [--wasm] [--lines N] [project...]";
const args = process.argv.slice(2);
const positional = [];
let out = join(root, "target/perf/PM01-ir");
let opts = ["-O0"];
let wasm = false;
let lines = 6;
for (let index = 0; index < args.length; ++index) {
  const arg = args[index];
  if (arg === "--wasm") wasm = true;
  else if (["--out", "--opt", "--lines"].includes(arg)) {
    const value = args[++index];
    if (value === undefined || value.startsWith("--")) throw new Error(usage);
    if (arg === "--out") out = resolve(value);
    else if (arg === "--opt") opts = value.split(",");
    else lines = Number(value);
  } else if (arg.startsWith("--")) throw new Error(usage);
  else positional.push(arg);
}
if (positional.length < 2) throw new Error(usage);
const [before, after] = positional.slice(0, 2).map((path) => resolve(path));
const directories = (parent) => (existsSync(parent) ? readdirSync(parent).sort().map((name) => join(parent, name)).filter((path) => statSync(path).isDirectory()) : []);
const projects = positional.length > 2
  ? positional.slice(2).map((path) => resolve(path))
  : [...directories(join(root, "tests/fixtures")), ...directories(join(root, "examples")),
      join(root, "benchmarks/layout"), join(root, "benchmarks/computations")];

async function build(compiler, side, project, target, opt) {
  const name = `${relative(root, project).replaceAll(/[\\/]/g, "_")}.${target}${opt}.ll`;
  const directory = join(out, side);
  mkdirSync(directory, { recursive: true });
  const file = join(directory, name);
  const flags = ["build", project, "--emit", "llvm", opt, "-o", file];
  if (target === "wasm32") flags.push("--target", "wasm32");
  try {
    await execute(compiler, flags, { env: { ...process.env, TSUZURI_CACHE_DIR: "" }, maxBuffer: 1 << 28 });
    return { text: readFileSync(file, "utf8") };
  } catch (error) {
    return { failure: String(error.stderr ?? error.message).split("\n").find((line) => /error/.test(line))?.replaceAll(project, "<project>") ?? "failed" };
  }
}

const jobs = [];
for (const project of projects) {
  for (const target of wasm ? ["native", "wasm32"] : ["native"]) {
    for (const opt of opts) jobs.push({ project, target, opt });
  }
}
const results = [];
let next = 0;
async function worker() {
  while (next < jobs.length) {
    const job = jobs[next++];
    const [left, right] = await Promise.all([
      build(before, "before", job.project, job.target, job.opt),
      build(after, "after", job.project, job.target, job.opt),
    ]);
    results.push({ ...job, left, right });
  }
}
await Promise.all(Array.from({ length: 4 }, worker));
results.sort((a, b) => `${a.project}${a.target}${a.opt}`.localeCompare(`${b.project}${b.target}${b.opt}`));

let same = 0;
let failed = 0;
let different = 0;
for (const { project, target, opt, left, right } of results) {
  const label = `${basename(project)} ${target} ${opt}`;
  if (left.failure !== undefined || right.failure !== undefined) {
    if (left.failure === right.failure) failed += 1;
    else {
      different += 1;
      console.log(`DIFF ${label}: before ${left.failure ?? "built"}; after ${right.failure ?? "built"}`);
    }
  } else if (left.text === right.text) same += 1;
  else {
    different += 1;
    const a = left.text.split("\n");
    const b = right.text.split("\n");
    const shown = [];
    for (let index = 0; index < Math.max(a.length, b.length) && shown.length < lines; ++index) {
      if (a[index] !== b[index]) shown.push(`  line ${index + 1}\n    - ${a[index]}\n    + ${b[index]}`);
    }
    console.log(`DIFF ${label}: ${a.length} / ${b.length} lines\n${shown.join("\n")}`);
  }
}
console.log(`identical ${same}, both failed alike ${failed}, different ${different} (of ${results.length})`);
process.exit(different === 0 ? 0 : 1);
