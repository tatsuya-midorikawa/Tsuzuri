// Differential check between two Tsuzuri compilers for PB03 (the handoff to LLVM): every example, fixture
// and benchmark project is built by both compilers in several modes.
//   - `--emit header|llvm|object|wasm|shared` outputs must be byte identical (diagnostics too, for failures), and so
//     must the WebAssembly of the modes with the cache on (`wasmt`: threads, `wasi`: the WASI host), which compile
//     their C runtimes through the runtime cache;
//   - native executables (`-O0`, `-O3`, `-g`) must behave identically (exit status, stdout, stderr), and the IR
//     handed to Clang may only lose functions (a pruned executable defines a subset of the before's functions, the
//     same type definitions and no new declaration). A build that failed before may only succeed now if the link
//     failed on a symbol that the pruned code no longer names (dead code that used to fail the build), or if the trap
//     instrumentation of the baseline took a runtime part for a part of the function before it (fixed with PB03);
//     a link that still fails may only name fewer undefined symbols;
//   - `lib0`, `lib3`, `libg`, `libt`, `libc`: a native executable is the only output that prunes, and only a fifth of the
//     projects are executables. These modes give every library-style project an entry in four ways, and every project
//     that already has an entry a copy whose exports are plain public functions (see `convertProject`), and compare
//     those programs like the executables above (`-O0`, `-O3`, `-O0 -g`, `-O3 --trap-info`, and `-O3` with the cache on).
// Usage: node benchmarks/check-handoff-equivalence.mjs --before <compiler> --after <compiler> --out <dir>
//          [--quick] [--only a,b] [--modes header,llvm,...] [--jobs N] [--timeout ms]
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { chmodSync, cpSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { platform } from "node:os";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const usage = "Usage: node benchmarks/check-handoff-equivalence.mjs --before <compiler> --after <compiler> --out <dir> "
  + "[--quick] [--only a,b] [--modes header,llvm,object,wasm,shared,wasmt,wasi,exe0,exe3,exeg,exegc,lib0,lib3,libg,libt,libc] [--jobs N] [--timeout ms]";
// The modes of the library-style programs (`convertProject`) and the arguments of each: `lib0`, `lib3` and `libg` build like
// `exe0`, `exe3` and `exeg`, `libt` is what `tsuzuri run` builds (`--trap-info`), and `libc` is what `tsuzuri build` does by
// default (`-O3`, the cache on, so the numeric and C runtimes are objects of their own).
const LIBRARY_MODES = {
  lib0: ["-O0", "--no-cache"],
  lib3: ["-O3", "--no-cache"],
  libg: ["-O0", "-g", "--no-cache"],
  libt: ["-O3", "--trap-info", "--no-cache"],
  libc: ["-O3"],
};
const ALL_MODES = ["header", "llvm", "object", "wasm", "shared", "wasmt", "wasi", "exe0", "exe3", "exeg", "exegc", ...Object.keys(LIBRARY_MODES)];
// The modes that run with the build cache on, each in a cache directory of its own.
const CACHED_MODES = new Set(["wasmt", "wasi", "exegc", "libc"]);
const isExecutable = (mode) => mode.startsWith("exe") || mode in LIBRARY_MODES;
const QUICK = ["examples/hello", "examples/tasks", "tests/fixtures/dyn_dispatch", "tests/fixtures/cpu_versions",
  "tests/fixtures/debug_info", "llvm_handoff/prune", "llvm_handoff/reach", "llvm_handoff/channel_std", "llvm_handoff/tree_lock"];
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

function functionNames(ir, keyword) {
  const names = new Set();
  for (const line of ir.split("\n")) {
    // A runtime part is appended right behind the closing brace of the function before it.
    const text = line.startsWith("}") ? line.slice(1) : line;
    if (!text.startsWith(keyword)) continue;
    const match = /@("[^"]*"|[^\s(]+)\(/.exec(text);
    if (match) names.add(match[1]);
  }
  return names;
}

export const definedFunctions = (ir) => functionNames(ir, "define ");
export const declaredFunctions = (ir) => functionNames(ir, "declare ");

// The lines that define a record or union type of the program, such as `%"tz.union.Maybe.Maybe[i64]" = type { i32, i64 }`
// (the types of the runtime parts that a program includes only when it uses them, such as `%tz.rec.header` and
// `%struct.tzrt_format`, are not the program's).
export const typeDefinitions = (ir) => new Set(ir.split("\n").filter((line) => /^%"?tz\.(record|union)\./.test(line) && line.includes(" = type ")));

// ---- Library-style projects as executables (modes lib0, lib3, libg, libt, libc) ---------------------------------------------
// Only a native executable prunes, and only about a fifth of the projects have an entry, so nothing else would ever run the
// pruned emission over the code of the rest. `convertProject` gives a project an entry, and the variants differ in what
// the executable keeps:
//   keep    a constant entry; the exports stay exports, which are roots of an executable;
//   none    a constant entry and the exports become plain public functions, so only the drops of std types are reachable;
//   even    the exports become plain public functions, and the entry calls the even-numbered functions that it can write
//   odd     the arguments of (scalars and strings), or the odd-numbered ones; the rest are public and unreachable;
//   unexport  for a project that has an entry already: its exports become plain public functions.
// The entry prints what the calls returned, so the behavior of the reachable code is compared too. Many fixtures keep a
// function that traps under a name that does not say so, and the program ends there: both compilers must end the same way.
const SOURCE_FILE = /\.(tz|tt|tc)$/;
const CALL_LIMIT = 48;
const CALLABLE_TYPE = /^(?:i8u?|i16u?|i32u?|i64u?|byte|ubyte|sbyte|f32|f64|bool|string|unit)$/;
const ARGUMENT = { bool: "true", f32: "0.5", f64: "0.5", string: "\"ab\"", unit: "()" };
// Functions that say that they exist to trap (a call would end the program before the other calls ran).
const TRAPPING = /trap|deadlock|panic|abort|boom|unreachable/;

function sourceFiles(directory) {
  return readdirSync(directory, { withFileTypes: true }).sort((a, b) => (a.name < b.name ? -1 : 1))
    .flatMap((entry) => (entry.isDirectory() ? sourceFiles(join(directory, entry.name))
      : SOURCE_FILE.test(entry.name) ? [join(directory, entry.name)] : []));
}

// The top-level functions of `text` whose parameters and result are types that a call can be written for.
export function callableFunctions(text, qualifier = "") {
  const found = [];
  for (const [, name, rest] of text.matchAll(/^(?:export )?def (?:rec )?([a-z_][A-Za-z0-9_]*) :: (.*)$/gm)) {
    const types = rest.replace(/ =( .*)?$/, "").trim().split(" -> ");
    const result = types.pop();
    if (TRAPPING.test(name) || ![...types, result].every((type) => CALLABLE_TYPE.test(type))) continue;
    const call = types.length === 0 ? `${qualifier}${name}()` : `(${qualifier}${name} ${types.map((type) => ARGUMENT[type] ?? "3").join(" ")})`;
    found.push({ call, shown: result !== "unit" });
  }
  return found;
}

// The callable functions of `Main.tz` and of the other modules next to it, in file order.
function projectCalls(directory) {
  return readdirSync(directory, { withFileTypes: true }).sort((a, b) => (a.name < b.name ? -1 : 1))
    .filter((entry) => entry.isFile() && entry.name.endsWith(".tz"))
    .flatMap((entry) => callableFunctions(readFileSync(join(directory, entry.name), "utf8"), entry.name === "Main.tz" ? "" : `${entry.name.slice(0, -3)}.`));
}

function entryCalls(directory, variant) {
  if (variant !== "even" && variant !== "odd") return [];
  const half = projectCalls(directory).filter((_, index) => index % 2 === (variant === "even" ? 0 : 1));
  const stride = Math.max(1, Math.ceil(half.length / CALL_LIMIT));
  return half.filter((_, index) => index % stride === 0);
}

// Top-level code: each call in a binding, then the results that can be shown as one string (or "ok").
function entryCode(calls) {
  const shown = calls.flatMap((call, index) => (call.shown ? [`{r${index}}`] : []));
  const bindings = calls.map((call, index) => `let ${call.shown ? "" : "_"}r${index} = ${call.call}\n`).join("");
  return `${bindings}${shown.length ? `$"${shown.join(" ")}"` : "\"ok\""}\n`;
}

const hasExports = (directory) => sourceFiles(directory).some((file) => /^export def /m.test(readFileSync(file, "utf8")));

// The variants that a project gets: `kind` is "library" (no entry) or "executable".
export function variantsOf(directory, kind) {
  if (kind === "executable") return hasExports(directory) ? ["unexport"] : [];
  return [...(hasExports(directory) ? ["keep"] : []), "none", ...["even", "odd"].filter((variant) => entryCalls(directory, variant).length > 0)];
}

// Writes the project at `source` with `variant` applied to `target`; returns the number of calls of the entry.
export function convertProject(source, target, variant) {
  rmSync(target, { recursive: true, force: true });
  cpSync(source, target, { recursive: true });
  if (variant !== "keep") {
    for (const file of sourceFiles(target)) {
      const text = readFileSync(file, "utf8");
      if (/^export def /m.test(text)) writeFileSync(file, text.replace(/^export def /gm, "def "));
    }
  }
  if (variant === "unexport") return 0;
  const calls = entryCalls(target, variant);
  const main = join(target, "Main.tz");
  const text = existsSync(main) ? `${readFileSync(main, "utf8").trimEnd()}\n\n` : "";
  writeFileSync(main, text + entryCode(calls));
  return calls.length;
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

function modeArguments(mode, directory, output) {
  const base = ["build", directory];
  if (mode in LIBRARY_MODES) return [...base, ...LIBRARY_MODES[mode], "--cpu", "generic", "-o", output];
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
  // The linker names the temporary object of `module.ll` after a random suffix (`module-25824c.o`).
  return result.replace(/\.tsuzuri-\d+-\d+/g, ".tsuzuri-N").replace(/tsuzuri-clang-\d+-\d+/g, "tsuzuri-clang-N")
    .replace(/\bmodule-[0-9a-f]{6}\.o\b/g, "module-N.o")
    .replace(/^build cache disabled: tool version query failed\n/gm, "");
}

async function execute(label, compiler, { project, mode, variant }, context) {
  const directory = join(context.out, label, project.replaceAll("/", "__"), variant ? `${mode}-${variant}` : mode);
  rmSync(directory, { recursive: true, force: true });
  mkdirSync(directory, { recursive: true });
  // A converted project is built and run in a copy of its own (the program may write files next to its sources).
  let source = join(root, project);
  if (variant) {
    const original = source;
    source = join(directory, "src");
    convertProject(original, source, variant);
  }
  const output = join(directory, `artifact.${extension(mode)}`);
  const ir = join(directory, "module.ll.copy");
  const env = { ...process.env, TMPDIR: directory, HANDOFF_IR_COPY: ir,
    HANDOFF_REAL_CLANG: process.env.TSUZURI_CLANG ?? "clang" };
  if (CACHED_MODES.has(mode)) env.TSUZURI_CACHE_DIR = join(directory, "cache");
  const spying = isExecutable(mode) || mode === "object";
  if (spying && platform() !== "win32") env.TSUZURI_CLANG = context.spy;
  const replacements = [[source, "<PROJECT>"], [directory, "<WORK>"], [root, "<ROOT>"]];
  const build = await run(compiler, modeArguments(mode, source, output), { env, timeout: context.timeout });
  const record = { status: build.status, signal: build.signal, timedOut: build.timedOut,
    stderr: normalize(build.stderr, replacements), stdout: normalize(build.stdout, replacements) };
  if (build.status === 0 && existsSync(output)) {
    record.artifact = fingerprint(output);
    record.bytes = statSync(output).size;
    if (existsSync(ir)) record.ir = normalize(readFileSync(ir, "utf8"), replacements);
    if (isExecutable(mode)) {
      const attempt = async () => {
        const result = await run(output, [], { cwd: source, timeout: context.runTimeout, env: process.env });
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

// Whether the before compiler takes a project for a library (E2004: no entry; a project without `Main.tz` is one too) or an executable.
async function projectKind(compiler, project, context) {
  if (!existsSync(join(root, project, "Main.tz"))) return "library";
  const directory = join(context.out, "probe", project.replaceAll("/", "__"));
  rmSync(directory, { recursive: true, force: true });
  mkdirSync(directory, { recursive: true });
  const build = await run(compiler, ["build", join(root, project), "-O0", "--cpu", "generic", "--no-cache", "-o", join(directory, "artifact.exe")],
    { env: { ...process.env, TMPDIR: directory }, timeout: context.timeout });
  return /error\[E2004\]: an (?:executable requires|application must start from)/.test(build.stderr) ? "library" : "executable";
}

// A link that failed on a symbol: what a pruned executable no longer needs when only dead code names the symbol.
const LINK_FAILURE = /Undefined symbols|undefined reference|undefined symbol/i;
// What the trap instrumentation of the baseline did to a runtime part that shares the line of the closing brace before it
// (`}define ...`): the function took no `%tz.context`, and its body used one. It is fixed with PB03, and a build that failed
// on it may succeed now.
const CONTEXT_FAILURE = /use of undefined value '%tz\.context'/;
const undefinedSymbols = (stderr) => [...stderr.matchAll(/^\s+"([^"]+)", referenced from/gm), ...stderr.matchAll(/undefined reference to [`']([^'`]+)'/g)]
  .map((match) => match[1]);
const outcome = (run) => (run.timedOut ? "timeout" : run.signal ? `signal ${run.signal}` : `exit ${run.status}`);

function compare(mode, before, after, variant) {
  const problems = [];
  const same = (name, left, right) => { if (left !== right) problems.push(`${name} differs`); };
  const linkFailed = isExecutable(mode) && before.status !== 0 && LINK_FAILURE.test(before.stderr);
  const contextFailed = isExecutable(mode) && before.status !== 0 && CONTEXT_FAILURE.test(before.stderr);
  if ((linkFailed || contextFailed) && after.status === 0) {
    // A constant entry shows what the program prints.
    if (["keep", "none"].includes(variant) && (after.run?.status !== 0 || after.run.stdout.replaceAll("\r\n", "\n") !== "ok\n")) {
      problems.push("the constant entry did not print ok");
    }
    return { problems, detail: linkFailed ? "now builds: the link needed a symbol that only unreachable code names"
      : "now builds: the trap instrumentation of the baseline failed on a runtime part (fixed)" };
  }
  if ((linkFailed || contextFailed) && after.status !== 0 && before.stderr !== after.stderr) {
    // Still no build, but with the failures that the kept code has: the symbols that it names, not the ones of removed code.
    const had = new Set(undefinedSymbols(before.stderr));
    const gained = undefinedSymbols(after.stderr).filter((symbol) => !had.has(symbol));
    same("exit status", before.status, after.status);
    if (!LINK_FAILURE.test(after.stderr)) problems.push("diagnostics differs");
    if (gained.length) problems.push(`the link fails on ${gained.length} symbols that did not fail before, such as ${gained[0]}`);
    return { problems, detail: "same failure: the link names fewer undefined symbols" };
  }
  same("exit status", before.status, after.status);
  same("signal", before.signal, after.signal);
  same("diagnostics", before.stderr, after.stderr);
  same("stdout", before.stdout, after.stdout);
  let detail = before.status === 0 ? "built" : "same failure";
  if (before.status === 0 && after.status === 0) {
    if (!isExecutable(mode)) {
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
        detail = `same behavior (${outcome(left)})`;
      }
      if (before.ir !== undefined && after.ir !== undefined) {
        if (before.ir === after.ir) detail += ", identical IR";
        else {
          const kept = definedFunctions(after.ir), had = definedFunctions(before.ir);
          const added = [...kept].filter((name) => !had.has(name));
          const removed = [...had].filter((name) => !kept.has(name));
          if (added.length) problems.push(`the IR gains ${added.length} functions, such as ${added[0]}`);
          if (!kept.has("@main") && had.has("@main")) problems.push("the IR lost @main");
          const declared = declaredFunctions(before.ir), gained = [...declaredFunctions(after.ir)].filter((name) => !declared.has(name));
          if (gained.length) problems.push(`the IR gains ${gained.length} declarations, such as ${gained[0]}`);
          // A pruned executable still defines every type of the program (a drop or a builtin may load one that no kept body names).
          const types = typeDefinitions(before.ir), now = typeDefinitions(after.ir);
          const lost = [...types].filter((line) => !now.has(line)), added_types = [...now].filter((line) => !types.has(line));
          if (lost.length || added_types.length) problems.push(`the type definitions differ (${lost.length} lost, ${added_types.length} new), such as ${(lost[0] ?? added_types[0]).split(" = ")[0]}`);
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
  const kinds = {};
  if (options.modes.some((mode) => mode in LIBRARY_MODES)) {
    let probe = 0;
    await Promise.all(Array.from({ length: options.jobs }, async () => {
      while (probe < list.length) {
        const project = list[probe++];
        kinds[project] = await projectKind(before, project, context);
      }
    }));
  }
  const jobs = list.flatMap((project) => options.modes.flatMap((mode) => (mode in LIBRARY_MODES
    ? variantsOf(join(root, project), kinds[project]).map((variant) => ({ project, mode, variant }))
    : [{ project, mode }])));
  const results = [];
  let next = 0, failures = 0;
  async function worker() {
    while (next < jobs.length) {
      const job = jobs[next++];
      const label = job.variant ? `${job.mode}:${job.variant}` : job.mode;
      const left = await execute("before", before, job, context);
      const right = await execute("after", after, job, context);
      const { problems, detail } = compare(job.mode, left, right, job.variant);
      const record = { project: job.project, mode: label, detail, problems, before: left.status, after: right.status };
      // What a build that failed in both ended with, to see what the converted programs do not get through.
      if (left.status !== 0 && right.status !== 0) record.failure = /error\[(E\d+)\]: ([^\n]{0,100})/.exec(right.stderr)?.slice(1).join(": ") ?? right.stderr.slice(0, 100);
      results.push(record);
      if (problems.length) {
        failures++;
        console.error(`FAIL ${job.project} ${label}: ${problems.join("; ")}`);
      } else console.log(`ok   ${job.project} ${label}: ${detail}`);
    }
  }
  await Promise.all(Array.from({ length: options.jobs }, worker));
  results.sort((a, b) => (a.project + a.mode < b.project + b.mode ? -1 : 1));
  const counts = {}, failed = {};
  for (const result of results) {
    const key = result.problems.length ? "FAIL" : result.detail.replace(/\(\d+ of \d+/, "(N of M");
    counts[key] = (counts[key] ?? 0) + 1;
    if (result.failure) failed[result.failure.split(":")[0]] = (failed[result.failure.split(":")[0]] ?? 0) + 1;
  }
  writeFileSync(join(out, "summary.json"), JSON.stringify({ before, after, projects: list, modes: options.modes,
    kinds, counts, failed, results }, null, 2) + "\n");
  console.log(`\n${results.length} comparisons over ${list.length} projects (${relative(root, out)}/summary.json):`);
  for (const [key, count] of Object.entries(counts).sort()) console.log(`  ${String(count).padStart(5)}  ${key}`);
  if (Object.keys(failed).length) console.log(`builds that failed in both, by error code: ${JSON.stringify(failed)}`);
  if (failures) {
    console.error(`${failures} comparisons failed`);
    process.exitCode = 1;
  } else console.log(`handoff equivalence: ${results.length} comparisons passed`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await main();
