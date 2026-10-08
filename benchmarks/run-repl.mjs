// Latency of one REPL input and its stages (G13 Phase 3). It reports medians, minimums, and maximums
// in milliseconds as JSON lines; it asserts results but has no speed threshold.
//
//   node benchmarks/run-repl.mjs target/release/tsuzuri [--samples 9] [--out target/perf/G13-after]
//
// Stages of the REPL's program for an expression (`let it = (1 + N)` and `Display.display (ref it)`),
// measured with the CLI: process start (`--version`), analysis (`check`), IR (`build --emit llvm`),
// a whole build without and with a whole-build cache hit, Clang inside that build (timed by a wrapper
// passed as TSUZURI_CLANG), the IR compile and the link replayed separately from the IR the wrapper
// keeps, and the program's run. Then the REPL itself: the time from writing one input line to reading
// its result line, in one session, for `:type`, a new and a repeated expression, a `let`, and an action.
import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { cpus, loadavg, release, tmpdir } from "node:os";
import { delimiter, dirname, join, resolve } from "node:path";

const option = name => {
  const index = process.argv.indexOf(name);
  return index > 0 ? process.argv[index + 1] : undefined;
};
const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const samples = Number(option("--samples") ?? 9);
const out = option("--out");
const work = mkdtempSync(join(tmpdir(), "tsuzuri-repl-bench-"));
const cache = join(work, "cache");
const records = [];
const raw = [];

function executable(name) {
  if (name.includes("/")) return resolve(name);
  for (const directory of (process.env.PATH ?? "").split(delimiter)) {
    if (existsSync(join(directory, name))) return join(directory, name);
  }
  throw new Error(`${name} is not on PATH`);
}

const clang = executable(process.env.TSUZURI_CLANG ?? "clang");
const wrapper = join(work, "clang-timer");
const log = join(work, "clang.log");
const stash = join(work, "stash");
mkdirSync(stash);
writeFileSync(`${wrapper}.c`, String.raw`#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>
#include <sys/wait.h>
static void keep(const char *path, const char *directory) {
    char target[4096];
    const char *name = strrchr(path, '/');
    snprintf(target, sizeof target, "%s/%d-%s", directory, (int)getpid(), name ? name + 1 : path);
    FILE *in = fopen(path, "rb"), *out = in ? fopen(target, "wb") : NULL;
    char buffer[65536];
    size_t count;
    while (in && out && (count = fread(buffer, 1, sizeof buffer, in)) > 0) fwrite(buffer, 1, count, out);
    if (in) fclose(in);
    if (out) fclose(out);
}
int main(int argc, char **argv) {
    const char *real = getenv("TZ_BENCH_CLANG"), *log = getenv("TZ_BENCH_LOG"), *stash = getenv("TZ_BENCH_STASH");
    for (int i = 1; stash && i < argc; i++) {
        size_t n = strlen(argv[i]);
        if (n > 3 && strcmp(argv[i] + n - 3, ".ll") == 0) keep(argv[i], stash);
    }
    struct timespec start, end;
    clock_gettime(CLOCK_MONOTONIC, &start);
    pid_t child = fork();
    if (child == 0) {
        argv[0] = (char *)real;
        execv(real, argv);
        _exit(127);
    }
    int status = 0;
    waitpid(child, &status, 0);
    clock_gettime(CLOCK_MONOTONIC, &end);
    FILE *file = log ? fopen(log, "a") : NULL;
    if (file) {
        fprintf(file, "%.3f", (end.tv_sec - start.tv_sec) * 1e3 + (end.tv_nsec - start.tv_nsec) / 1e6);
        for (int i = 1; i < argc; i++) fprintf(file, "\t%s", argv[i]);
        fputc('\n', file);
        fclose(file);
    }
    return WIFEXITED(status) ? WEXITSTATUS(status) : 1;
}
`);
assert.equal(spawnSync(clang, ["-O2", `${wrapper}.c`, "-o", wrapper]).status, 0, "the clang wrapper builds");

// The REPL and its one-input processes use Clang directly; the stage builds go through the wrapper.
const plain = { ...process.env, TSUZURI_CACHE_DIR: cache };
const env = { ...plain, TSUZURI_CLANG: wrapper, TZ_BENCH_CLANG: clang, TZ_BENCH_LOG: log };
const median = values => [...values].sort((left, right) => left - right)[Math.floor(values.length / 2)];

function record(name, optimization, values, extra = {}) {
  const entry = { name, optimization, samples: values.length, median_ms: +median(values).toFixed(1), min_ms: +Math.min(...values).toFixed(1), max_ms: +Math.max(...values).toFixed(1), ...extra };
  records.push(entry);
  values.forEach((ms, index) => raw.push({ name, optimization, index, ms: +ms.toFixed(3) }));
  console.log(JSON.stringify(entry));
}

function timed(program, args, options = {}) {
  const start = performance.now();
  const result = spawnSync(program, args, { encoding: "utf8", env, timeout: 600_000, ...options });
  const elapsed = performance.now() - start;
  assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.error ?? ""}${result.stderr}`);
  return { elapsed, result };
}

// The program the REPL checks and runs for the expression input `1 + N` in an empty session.
const project = join(work, "project");
mkdirSync(project);
const program = value => `let it = (\n1 + ${value}\n)\nDisplay.display (ref it)\n`;
let unique = 1000;

function clangLog() {
  const lines = existsSync(log) ? readFileSync(log, "utf8").trim().split("\n").filter(Boolean) : [];
  rmSync(log, { force: true });
  return lines.map(line => {
    const [ms, ...args] = line.split("\t");
    return { ms: Number(ms), args };
  });
}

for (const level of [0, 3]) {
  const O = `-O${level}`;
  const stages = { start: [], check: [], llvm: [], miss: [], clang: [], cachedMiss: [], probe: [], hit: [], compile: [], link: [], run: [], rerun: [], lli: [] };
  // Research only (G13 D13): LLVM's own JIT runner on the same IR, when Clang's directory has one.
  const lli = join(dirname(clang), "lli");
  const exe = join(work, "program");
  for (let index = 0; index <= samples; index++) {
    const kept = unique++;
    writeFileSync(join(project, "Main.tz"), program(kept));
    const start = timed(compiler, ["--version"]).elapsed;
    const check = timed(compiler, ["check", project]).elapsed;
    const llvm = timed(compiler, ["build", project, "--emit", "llvm", "--no-cache", O, "-o", join(work, "module.ll")]).elapsed;
    clangLog();
    rmSync(stash, { recursive: true, force: true });
    mkdirSync(stash);
    const miss = timed(compiler, ["build", project, "--no-cache", O, "-o", exe], { env: { ...env, TZ_BENCH_STASH: stash } }).elapsed;
    const invocations = clangLog();
    const builds = invocations.filter(call => !call.args.includes("--version"));
    assert.equal(builds.length, 1, JSON.stringify(invocations));
    writeFileSync(join(project, "Main.tz"), program(unique++));
    const cachedMiss = timed(compiler, ["build", project, O, "-o", exe]).elapsed;
    clangLog();
    const hit = timed(compiler, ["build", project, O, "-o", exe]).elapsed;
    const hitCalls = clangLog();
    assert.ok(hitCalls.every(call => call.args.includes("--version")), "a cache hit runs no Clang build");
    const probe = hitCalls.reduce((sum, call) => sum + call.ms, 0);
    // The kept IR, compiled and linked in two steps with the build's flags.
    const ir = join(stash, readdirSync(stash).find(name => name.endsWith(".ll")));
    const object = join(work, "module.o");
    const flags = builds[0].args.filter(arg => !arg.endsWith(".ll") && arg !== "-o" && !arg.includes("artifact"));
    const compileFlags = flags.filter(arg => arg !== "-lm");
    assert.ok(compileFlags.every(arg => arg.startsWith("-") || arg === "ir"), `only the IR is compiled: ${flags.join(" ")}`);
    const compile = timed(clang, [...compileFlags, "-c", ir, "-o", object]).elapsed;
    const link = timed(clang, [object, "-lm", "-o", join(work, "relinked")]).elapsed;
    // The first run of a new executable includes the operating system's checks of a new file.
    const run = timed(exe, []);
    assert.equal(run.result.stdout, `${1 + unique - 1}\n`);
    const rerun = timed(exe, []).elapsed;
    if (existsSync(lli)) {
      const jit = timed(lli, [O, ir]);
      assert.equal(jit.result.stdout, `${1 + kept}\n`);
      if (index > 0) stages.lli.push(jit.elapsed);
    }
    if (index === 0) continue; // warm-up
    Object.entries({ start, check, llvm, miss, cachedMiss, hit, compile, link, run: run.elapsed, rerun }).forEach(([key, value]) => stages[key].push(value));
    stages.clang.push(builds[0].ms);
    stages.probe.push(probe);
  }
  record("process start (tsuzuri --version)", level, stages.start);
  record("check (start + load + analysis)", level, stages.check);
  record("build --emit llvm --no-cache (check + IR)", level, stages.llvm);
  record("build exe --no-cache (whole build, cache miss)", level, stages.miss);
  record("clang inside that build (IR compile + link)", level, stages.clang);
  record("clang --version for the cache key, inside a cache-hit build", level, stages.probe);
  record("replay: clang -c of the IR", level, stages.compile);
  record("replay: link", level, stages.link);
  record("build exe, cache enabled, cache miss (adds the key and the store)", level, stages.cachedMiss);
  record("build exe, whole-build cache hit", level, stages.hit);
  record("program run, first run of the new file", level, stages.run);
  record("program run, second run", level, stages.rerun);
  if (stages.lli.length) record("research: lli of the same IR (JIT compile and run, no link, no new file)", level, stages.lli);
}

// The REPL: the time from writing one input to reading its one result line, in one session.
async function session(level) {
  const child = spawn(compiler, ["repl", `-O${level}`], { env: plain, stdio: ["pipe", "pipe", "pipe"] });
  let buffer = "";
  let stderr = "";
  let waiting;
  child.stdout.setEncoding("utf8");
  child.stdout.on("data", chunk => {
    buffer += chunk;
    const newline = buffer.indexOf("\n");
    if (newline >= 0 && waiting) {
      const line = buffer.slice(0, newline);
      buffer = buffer.slice(newline + 1);
      const resolveLine = waiting;
      waiting = undefined;
      resolveLine(line);
    }
  });
  child.stderr.on("data", chunk => { stderr += chunk; });
  const ask = input => new Promise(resolveLine => {
    const start = performance.now();
    waiting = line => resolveLine({ line, ms: performance.now() - start });
    child.stdin.write(`${input}\n`);
  });
  const kinds = {
    ":type (analysis only)": index => [`:type 1 + ${unique + index}`, "i32"],
    "new expression (cache miss)": index => [`${unique + index} + 1`, `it: i32 = ${unique + index + 1}`],
    "repeated expression (cache hit)": () => ["1 + 1", "it: i32 = 2"],
    "new let (cache miss)": index => [`let v = ${unique + index}`, "v: i32"],
    "new action (cache miss)": index => [`do! IO.write_line "${unique + index}"`, `${unique + index}`],
  };
  for (const [name, input] of Object.entries(kinds)) {
    const values = [];
    for (let index = 0; index <= samples; index++) {
      const [text, expected] = input(index);
      const { line, ms } = await ask(text);
      assert.equal(line, expected, `${name}: ${text}\n${stderr}`);
      if (index > 0) values.push(ms);
    }
    unique += samples + 1;
    record(`repl: ${name}`, level, values);
  }
  child.stdin.end(":quit\n");
  await new Promise(resolveExit => child.once("close", resolveExit));
}
for (const level of [0, 3]) await session(level);

// The whole process for one expression, as `printf '1 + 1\n' | tsuzuri repl --no-cache` measures it.
for (const level of [0, 3]) {
  const values = [];
  for (let index = 0; index <= samples; index++) {
    const { elapsed, result } = timed(compiler, ["repl", `-O${level}`, "--no-cache"], { input: "1 + 1\n", env: plain });
    assert.equal(result.stdout, "it: i32 = 2\n");
    if (index > 0) values.push(elapsed);
  }
  record("printf '1 + 1\\n' | tsuzuri repl --no-cache", level, values);
}

const host = {
  date: new Date().toISOString(), compiler, version: spawnSync(compiler, ["--version"], { encoding: "utf8" }).stdout.trim(),
  clang: spawnSync(clang, ["--version"], { encoding: "utf8" }).stdout.split("\n")[0], node: process.version,
  cpu: cpus()[0]?.model, cores: cpus().length, os: `${process.platform} ${release()}`, loadavg_end: loadavg().map(value => +value.toFixed(1)), samples,
};
console.log(JSON.stringify({ host, note: "Medians of separate processes on a possibly loaded machine; no speed threshold" }));
if (out) {
  mkdirSync(out, { recursive: true });
  writeFileSync(join(out, "summary.jsonl"), [JSON.stringify({ host }), ...records.map(entry => JSON.stringify(entry))].join("\n") + "\n");
  writeFileSync(join(out, "samples.jsonl"), raw.map(entry => JSON.stringify(entry)).join("\n") + "\n");
}
rmSync(work, { recursive: true, force: true });
