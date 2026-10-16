// Differential check between two Tsuzuri compilers for PB03 (the handoff to LLVM): every example, fixture
// and benchmark project is built by both compilers in several modes.
//   - `--emit header|llvm|object|wasm|shared` outputs must be byte identical (diagnostics too, for failures), and so
//     must the WebAssembly of the modes with the cache on (`wasmt`: threads, `wasi`: the WASI host), which compile
//     their C runtimes through the runtime cache;
//   - native executables (`-O0`, `-O3`, `-g`) must behave identically (exit status, stdout, stderr), and the IR
//     handed to Clang may only lose functions (a pruned executable defines a subset of the before's functions).
// Usage: node benchmarks/check-handoff-equivalence.mjs --before <compiler> --after <compiler> --out <dir>
//          [--quick] [--only a,b] [--modes header,llvm,...] [--jobs N] [--timeout ms]
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { chmodSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { platform } from "node:os";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const usage = "Usage: node benchmarks/check-handoff-equivalence.mjs --before <compiler> --after <compiler> --out <dir> "
  + "[--quick] [--only a,b] [--modes header,llvm,object,wasm,shared,wasmt,wasi,exe0,exe3,exeg,exegc] [--jobs N] [--timeout ms]";
const ALL_MODES = ["header", "llvm", "object", "wasm", "shared", "wasmt", "wasi", "exe0", "exe3", "exeg", "exegc"];
// The modes that run with the build cache on, each in a cache directory of its own.
const CACHED_MODES = new Set(["wasmt", "wasi", "exegc"]);
const QUICK = ["examples/hello", "examples/tasks", "tests/fixtures/dyn_dispatch", "tests/fixtures/cpu_versions",
  "tests/fixtures/debug_info", "llvm_handoff/prune", "llvm_handoff/reach"];
const sharedExtension = platform() === "darwin" ? "dylib" : "so";

export function parseArguments(argv) {
  const options = { jobs: 3, timeout: 120_000, modes: ALL_MODES, quick: false, only: null };
  for (let index = 0; index < argv.length; index++) {
    const flag = argv[index];
    if (flag === "--quick") options.quick = true;
    else if (["--before", "--after", "--out", "--only", "--modes", "--jobs", "--timeout"].includes(flag)) {
      const value = argv[++index];
      assert.ok(value && !value.startsWith("--"), usage);
      options[flag.slice(2)] = value;
    } else assert.fail(usage);
  }
  assert.ok(options.before && options.after && options.out, usage);
  options.jobs = Number(options.jobs);
  options.timeout = Number(options.timeout);
  assert.ok(Number.isInteger(options.jobs) && options.jobs >= 1 && Number.isInteger(options.timeout) && options.timeout > 0, usage);
  if (typeof options.modes === "string") options.modes = options.modes.split(",");
  assert.ok(options.modes.every((mode) => ALL_MODES.includes(mode)), usage);
  if (typeof options.only === "string") options.only = options.only.split(",");
  return options;
}

// Directories that hold Tsuzuri sources directly, one or two levels below `examples`, `tests/fixtures` and `benchmarks`.
export function projects(only, quick) {
  const found = [];
  const visit = (directory, name, depth) => {
    const entries = readdirSync(directory, { withFileTypes: true }).sort((a, b) => (a.name < b.name ? -1 : 1));
    if (entries.some((entry) => entry.isFile() && /\.(tz|tt|tc)$/.test(entry.name))) found.push(name);
    else if (depth > 0) {
      for (const entry of entries) if (entry.isDirectory()) visit(join(directory, entry.name), `${name}/${entry.name}`, depth - 1);
    }
  };
  for (const parent of ["examples", "tests/fixtures", "benchmarks"]) {
    const directory = join(root, parent);
    if (!existsSync(directory)) continue;
    for (const entry of readdirSync(directory, { withFileTypes: true }).sort((a, b) => (a.name < b.name ? -1 : 1))) {
      if (entry.isDirectory()) visit(join(directory, entry.name), `${parent}/${entry.name}`, 1);
    }
  }
  const wanted = only ?? (quick ? QUICK : null);
  return wanted ? found.filter((project) => wanted.some((item) => project === item || project.endsWith(`/${item}`))) : found;
}

// The same bytes except what the platform's linker randomizes: the UUID and the code signature of a Mach-O file.
export function fingerprint(file) {
  const bytes = readFileSync(file);
  if (bytes.length >= 32 && bytes.readUInt32LE(0) === 0xfeedfacf && [2, 6].includes(bytes.readUInt32LE(12))) {
    const commands = bytes.readUInt32LE(16), end = 32 + bytes.readUInt32LE(20);
    let offset = 32;
    for (let index = 0; index < commands && offset + 8 <= end; index++) {
      const command = bytes.readUInt32LE(offset), size = bytes.readUInt32LE(offset + 4);
      if (command === 0x1b) bytes.fill(0, offset + 8, offset + size);
      else if (command === 0x1d) {
        const start = bytes.readUInt32LE(offset + 8), length = bytes.readUInt32LE(offset + 12);
        bytes.fill(0, start, start + length);
      }
      offset += size;
    }
  }
  return createHash("sha256").update(bytes).digest("hex");
}

export function definedFunctions(ir) {
  const names = new Set();
  for (const line of ir.split("\n")) {
    if (!line.startsWith("define ")) continue;
    const match = /@("[^"]*"|[^\s(]+)\(/.exec(line);
    if (match) names.add(match[1]);
  }
  return names;
}

function run(command, args, { cwd = root, env = process.env, timeout, input = "" } = {}) {
  return new Promise((resolvePromise) => {
    const child = spawn(command, args, { cwd, env, stdio: ["pipe", "pipe", "pipe"] });
    const out = [], err = [];
    let size = 0, timedOut = false;
    const collect = (chunks) => (chunk) => {
      size += chunk.length;
      if (size <= 8 * 1024 * 1024) chunks.push(chunk);
    };
    child.stdout.on("data", collect(out));
    child.stderr.on("data", collect(err));
    child.stdin.on("error", () => {});
    child.stdin.end(input);
    const timer = setTimeout(() => { timedOut = true; child.kill("SIGKILL"); }, timeout);
    child.on("error", (error) => {
      clearTimeout(timer);
      resolvePromise({ status: null, signal: null, stdout: "", stderr: String(error), timedOut });
    });
    child.on("close", (status, signal) => {
      clearTimeout(timer);
      resolvePromise({ status, signal, stdout: Buffer.concat(out).toString("utf8"),
        stderr: Buffer.concat(err).toString("utf8"), timedOut });
    });
  });
}

function modeArguments(mode, project, output) {
  const base = ["build", join(root, project)];
  switch (mode) {
    case "header": return [...base, "--emit", "header", "-o", output];
    case "llvm": return [...base, "--emit", "llvm", "-O0", "--no-cache", "-o", output];
    case "object": return [...base, "--emit", "object", "-O0", "--cpu", "generic", "--no-cache", "-o", output];
    case "wasm": return [...base, "--target", "wasm32", "-O0", "--no-cache", "-o", output];
    case "shared": return [...base, "--emit", "shared", "-O0", "--cpu", "generic", "--no-cache", "-o", output];
    case "exe0": return [...base, "-O0", "--cpu", "generic", "--no-cache", "-o", output];
    case "exe3": return [...base, "-O3", "--cpu", "generic", "--no-cache", "-o", output];
    case "exeg": return [...base, "-O0", "-g", "--cpu", "generic", "--no-cache", "-o", output];
    case "wasmt": return [...base, "--target", "wasm32", "--wasm-feature", "threads", "-O0", "-o", output];
    case "wasi": return [...base, "--target", "wasm32", "--wasm-host", "wasi", "-O0", "-o", output];
    case "exegc": return [...base, "-O0", "-g", "--cpu", "generic", "-o", output];
    default: throw new Error(mode);
  }
}

const extension = (mode) => ({ header: "h", llvm: "ll", object: "o", wasm: "wasm", wasmt: "wasm", wasi: "wasm", shared: sharedExtension }[mode] ?? "exe");

// A wrapper that keeps the IR that reaches Clang (module.ll) of a build, and runs the real Clang.
function writeSpy(directory) {
  const spy = join(directory, "clang-spy.mjs");
  writeFileSync(spy, `#!${process.execPath}
import { spawnSync } from "node:child_process";
import { copyFileSync } from "node:fs";
import { basename } from "node:path";
const args = process.argv.slice(2);
const save = process.env.HANDOFF_IR_COPY;
if (save) for (const argument of args) if (basename(argument) === "module.ll") copyFileSync(argument, save);
const result = spawnSync(process.env.HANDOFF_REAL_CLANG, args, { stdio: "inherit" });
process.exitCode = result.status ?? 1;
`);
  chmodSync(spy, 0o755);
  return spy;
}

export function normalize(text, replacements) {
  let result = text;
  for (const [from, to] of replacements) result = result.split(from).join(to);
  // A Homebrew `wasm-ld` (a link to the multi-call `lld`) made the compilers before PB03 WP-F disable the cache of
  // a WebAssembly build and say so; the fix removes the message and is the only intended change of a diagnostic.
  return result.replace(/\.tsuzuri-\d+-\d+/g, ".tsuzuri-N").replace(/tsuzuri-clang-\d+-\d+/g, "tsuzuri-clang-N")
    .replace(/^build cache disabled: tool version query failed\n/gm, "");
}

async function execute(label, compiler, project, mode, context) {
  const directory = join(context.out, label, project.replaceAll("/", "__"), mode);
  rmSync(directory, { recursive: true, force: true });
  mkdirSync(directory, { recursive: true });
  const output = join(directory, `artifact.${extension(mode)}`);
  const ir = join(directory, "module.ll.copy");
  const env = { ...process.env, TMPDIR: directory, HANDOFF_IR_COPY: ir,
    HANDOFF_REAL_CLANG: process.env.TSUZURI_CLANG ?? "clang" };
  if (CACHED_MODES.has(mode)) env.TSUZURI_CACHE_DIR = join(directory, "cache");
  const spying = mode.startsWith("exe") || mode === "object";
  if (spying && platform() !== "win32") env.TSUZURI_CLANG = context.spy;
  const replacements = [[directory, "<WORK>"], [join(root, project), "<PROJECT>"], [root, "<ROOT>"]];
  const build = await run(compiler, modeArguments(mode, project, output), { env, timeout: context.timeout });
  const record = { status: build.status, signal: build.signal, timedOut: build.timedOut,
    stderr: normalize(build.stderr, replacements), stdout: normalize(build.stdout, replacements) };
  if (build.status === 0 && existsSync(output)) {
    record.artifact = fingerprint(output);
    record.bytes = statSync(output).size;
    if (existsSync(ir)) record.ir = readFileSync(ir, "utf8");
    if (mode.startsWith("exe")) {
      const attempt = async () => {
        const result = await run(output, [], { cwd: join(root, project), timeout: context.runTimeout, env: process.env });
        return { status: result.status, signal: result.signal, timedOut: result.timedOut,
          stdout: normalize(result.stdout, replacements), stderr: normalize(result.stderr, replacements) };
      };
      record.run = await attempt();
      // Nondeterministic programs (time, randomness, process ids) are compared by exit status only.
      if (label === "before") record.stable = JSON.stringify(record.run) === JSON.stringify(await attempt());
    }
  }
  return record;
}

function compare(mode, before, after) {
  const problems = [];
  const same = (name, left, right) => { if (left !== right) problems.push(`${name} differs`); };
  same("exit status", before.status, after.status);
  same("signal", before.signal, after.signal);
  same("diagnostics", before.stderr, after.stderr);
  same("stdout", before.stdout, after.stdout);
  let detail = before.status === 0 ? "built" : "same failure";
  if (before.status === 0 && after.status === 0) {
    if (!mode.startsWith("exe")) {
      same("artifact bytes", before.artifact, after.artifact);
      if (before.artifact === after.artifact) detail = "identical";
    } else {
      const left = before.run, right = after.run;
      if (before.stable === false) {
        same("exit status of the program", left.status, right.status);
        detail = "nondeterministic program: exit status only";
      } else {
        same("program exit status", left.status, right.status);
        same("program signal", left.signal, right.signal);
        same("program stdout", left.stdout, right.stdout);
        same("program stderr", left.stderr, right.stderr);
        detail = "same behavior";
      }
      if (before.ir !== undefined && after.ir !== undefined) {
        if (before.ir === after.ir) detail += ", identical IR";
        else {
          const kept = definedFunctions(after.ir), had = definedFunctions(before.ir);
          const added = [...kept].filter((name) => !had.has(name));
          const removed = [...had].filter((name) => !kept.has(name));
          if (added.length) problems.push(`the IR gains ${added.length} functions, such as ${added[0]}`);
          if (!kept.has("@main") && had.has("@main")) problems.push("the IR lost @main");
          detail += `, IR pruned (${removed.length} of ${had.size} functions removed)`;
        }
      }
    }
  }
  return { problems, detail };
}

async function main() {
  const options = parseArguments(process.argv.slice(2));
  const out = resolve(options.out);
  assert.ok(!existsSync(join(out, "summary.json")), `EEXIST: ${out}/summary.json`);
  mkdirSync(out, { recursive: true });
  const before = resolve(options.before), after = resolve(options.after);
  const list = projects(options.only, options.quick);
  assert.ok(list.length > 0, "no project selected");
  const context = { out, timeout: options.timeout, runTimeout: 20_000, spy: writeSpy(out) };
  const jobs = list.flatMap((project) => options.modes.map((mode) => ({ project, mode })));
  const results = [];
  let next = 0, failures = 0;
  async function worker() {
    while (next < jobs.length) {
      const { project, mode } = jobs[next++];
      const left = await execute("before", before, project, mode, context);
      const right = await execute("after", after, project, mode, context);
      const { problems, detail } = compare(mode, left, right);
      const record = { project, mode, detail, problems, before: left.status, after: right.status };
      results.push(record);
      if (problems.length) {
        failures++;
        console.error(`FAIL ${project} ${mode}: ${problems.join("; ")}`);
      } else console.log(`ok   ${project} ${mode}: ${detail}`);
    }
  }
  await Promise.all(Array.from({ length: options.jobs }, worker));
  results.sort((a, b) => (a.project + a.mode < b.project + b.mode ? -1 : 1));
  const counts = {};
  for (const result of results) {
    const key = result.problems.length ? "FAIL" : result.detail.replace(/\(\d+ of \d+/, "(N of M");
    counts[key] = (counts[key] ?? 0) + 1;
  }
  writeFileSync(join(out, "summary.json"), JSON.stringify({ before, after, projects: list, modes: options.modes,
    counts, results }, null, 2) + "\n");
  console.log(`\n${results.length} comparisons over ${list.length} projects (${relative(root, out)}/summary.json):`);
  for (const [key, count] of Object.entries(counts).sort()) console.log(`  ${String(count).padStart(5)}  ${key}`);
  if (failures) {
    console.error(`${failures} comparisons failed`);
    process.exitCode = 1;
  } else console.log(`handoff equivalence: ${results.length} comparisons passed`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await main();
