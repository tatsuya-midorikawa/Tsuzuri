import assert from "node:assert/strict";
import { execFileSync, spawn, spawnSync } from "node:child_process";
import { closeSync, existsSync, mkdirSync, mkdtempSync, openSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { cpus, platform, tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { artifactFingerprint, captureRun, makeRecord, measureProcess, parseTimePasses, processRecords, summarize, unsupportedReason, writeRecords } from "./metrics.mjs";
import { expectedOutput, moduleFiles, writeProject } from "./build/generate.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
export const REAL_PROJECTS = Object.freeze(["examples/computations", "examples/control", "examples/currying", "examples/functional",
  "examples/hello", "examples/io", "examples/point", "examples/polymorphism", "examples/tasks", "benchmarks/control", "benchmarks/computations"]);
export const VARIANTS = Object.freeze([
  ["check", "native", "none"], ["emit-llvm", "native", "O0"], ["build", "native", "O0"], ["build", "native", "O3"],
  ["build-wasm", "wasm32", "O0"], ["build-wasm", "wasm32", "O3"], ["build-warm", "native", "O0"],
  ["build-edit-body", "native", "O0"], ["build-edit-signature", "native", "O0"],
  // PB06: the command after an edit of a module's body, by a process of its own and by a resident one
  // (`check-edit-body-server` and `build-edit-body-server` measure both, one sample of each in turn;
  // `watch-edit-body` is the time from the save to the end of the run of `tsuzuri watch check`).
  ["check-edit-body-server", "native", "none"], ["build-edit-body-server", "native", "O0"], ["watch-edit-body", "native", "none"],
  ...["build-runtime-cold", "build-runtime-warm"].flatMap(variant =>
    ["native", "wasm32"].flatMap(target => ["O0", "O3"].map(opt => [variant, target, opt]))),
]);
const languages = ["tsuzuri", "c", "cpp", "rust", "rust-workspace", "go", "zig"];
const execute = (program, args, options = {}) => execFileSync(program, args,
  { cwd: root, encoding: "utf8", timeout: 600_000, maxBuffer: 256 * 1024 * 1024, stdio: ["pipe", "pipe", "pipe"], ...options });

export function skipLargeWasm(modules, target, opt, largeWasm) {
  return modules > 200 && target === "wasm32" && opt === "O3" && !largeWasm;
}

export function breakdown(report) {
  let frontend = 0, cache = 0, tools = 0;
  for (const phase of report.phases) {
    // PB07: `units` is the reads and writes of the unit cache (and of the files of the units), `split` the cut of the IR.
    // G27: the stages of the knowledge graph (`kg-*`) count with the frontend.
    if (phase.name === "cache" || phase.name === "units") cache += phase.ms;
    else if (phase.name.startsWith("tool.")) tools += phase.ms;
    else frontend += phase.ms;
  }
  const tolerance = 0.0005 * (report.phases.length + 1);
  const other = report.total_ms - frontend - cache - tools;
  assert.ok(other >= -tolerance, "overlapping compiler timing phases");
  return { frontend, cache, tools, other: Math.max(0, other) };
}

function tool(name, variable, args, skipped) {
  const explicit = process.env[variable] ?? (name === "rustc" ? process.env.RUSTC : undefined);
  const executable = explicit ?? name;
  const child = spawnSync(executable, args, { cwd: root, encoding: "utf8", timeout: 30_000 });
  if (child.error?.code === "ENOENT" && explicit === undefined) {
    skipped[name] = "not found";
    console.error(`${name} not found (set ${variable} to its executable); skipping ${name} references`);
    return null;
  }
  if (child.error) throw new Error(`cannot execute ${variable}=${executable}: ${child.error.message}`);
  assert.equal(child.status, 0, `${executable} version failed:\n${child.stdout}${child.stderr}`);
  assert.ok(child.stdout.trim(), `${executable}: empty version`);
  return { executable, version: child.stdout.trim().split(/\r?\n/)[0] };
}

function sourceStats(project) {
  let files = 0, lines = 0, bytes = 0;
  const walk = (directory) => {
    for (const entry of readdirSync(directory, { withFileTypes: true }).sort((left, right) => left.name < right.name ? -1 : 1)) {
      if (entry.isDirectory()) walk(join(directory, entry.name));
      else if (entry.isFile() && /\.(tz|tt|tc)$/.test(entry.name)) {
        const text = readFileSync(join(directory, entry.name));
        files++;
        bytes += text.length;
        lines += text.toString("utf8").split("\n").length - 1;
      }
    }
  };
  walk(project);
  return { files, lines, bytes };
}

function tsuzuriCommand(compiler, project, temporary, variant, target, opt) {
  const output = join(temporary, `app-${variant}-${opt}${target === "wasm32" ? ".wasm" : ""}`);
  if (variant === "check") return { program: compiler, args: ["check", project], output: null };
  const args = ["build", project, `-${opt}`];
  if (target === "wasm32") args.push("--target", "wasm32");
  else args.push("--cpu", "generic");
  if (variant === "emit-llvm") args.push("--emit", "llvm");
  if (["emit-llvm", "build", "build-wasm"].includes(variant)) args.push("--no-cache");
  args.push("-o", output);
  return { program: compiler, args, output };
}

function referenceCommand(language, project, opt, tools, jobs) {
  if (language === "c" || language === "cpp") return {
    program: tools.make.executable, args: ["-j", String(jobs), `CC=${tools.clang.executable}`, `OPT=-${opt}`],
    output: join(project, "app"),
  };
  if (language.startsWith("rust")) return {
    program: tools.cargo.executable, args: ["build", "--offline", "--manifest-path", join(project, "Cargo.toml"),
      "--jobs", String(jobs), ...(opt === "O3" ? ["--release"] : [])],
    output: join(project, "target", opt === "O3" ? "release" : "debug", "tsuzuri-build-bench"),
  };
  if (language === "go") return {
    program: tools.go.executable, args: ["build", ...(opt === "O0" ? ["-gcflags=all=-N -l"] : []), "-o", join(project, "app"), "."],
    output: join(project, "app"),
  };
  return { program: tools.zig.executable, args: ["build-exe", "Main.zig", "-lc", "-mcpu=baseline",
    "-O", opt === "O0" ? "Debug" : "ReleaseSafe", `-femit-bin=${join(project, "app")}`], output: join(project, "app") };
}

function resetReference(language, project, env) {
  if (language === "c" || language === "cpp") {
    for (const name of readdirSync(project).filter((name) => /\.(o|d)$/.test(name) || name === "app")) rmSync(join(project, name));
  } else if (language.startsWith("rust")) {
    rmSync(join(project, "target"), { recursive: true, force: true });
  } else if (language === "go") {
    rmSync(env.GOCACHE, { recursive: true, force: true });
    rmSync(join(project, "app"), { force: true });
  } else {
    for (const path of [env.ZIG_GLOBAL_CACHE_DIR, env.ZIG_LOCAL_CACHE_DIR]) rmSync(path, { recursive: true, force: true });
    rmSync(join(project, "app"), { force: true });
  }
}

function editModule(project, modules, language, variant, sample) {
  const index = Math.floor(modules / 2);
  const changed = [];
  for (const [name, text] of moduleFiles(index, { language: language === "rust-workspace" ? "rust" : language,
    rustLayout: language === "rust-workspace" ? "workspace" : "single",
    ...(variant === "build-edit-body" ? { salt: sample } : variant === "build-edit-signature" ? { extra: sample } : {}) })) {
    const file = join(project, name);
    if (readFileSync(file, "utf8") === text) continue;
    writeFileSync(file, text);
    changed.push(name);
  }
  return changed;
}

function verifyOutput(command, env, project, modules, edited = {}) {
  for (const line of ["", "12345", "綴り😀"]) {
    assert.equal(execute(command.output, [], { cwd: project, env, input: line + "\n" }), expectedOutput(modules, line, edited),
      `synthetic output mismatch: ${command.output} ${JSON.stringify(line)}; ${command.program} ${command.args.join(" ")}${command.diagnostic ?? ""}`);
  }
}

export function measureVariant(context, fields, command, { cwd, env, quick, before = () => {}, after = () => {},
  compilerPhases = true, sizeMetric = null, deterministic = true, allowSkip = false, knownLinkVariation = false }) {
  const measured = { wall_ms: [], user_ms: [], sys_ms: [], peak_rss_bytes: [], stdout: [], stderr: [] };
  const sizes = [], fingerprints = [], phases = [];
  const runs = quick ? 1 : 9, warmups = quick ? 0 : 1;
  for (let sample = -warmups; sample < runs; sample++) {
    before(sample);
    let result;
    try {
      result = measureProcess(command.program, command.args, { cwd, env, runs: 1, warmups: 0 });
    } catch (error) {
      if (allowSkip && sample === -warmups && error.status === 1 && /\bE2004\b/.test(error.stderr)) {
        console.error(`skip ${fields.workload} ${fields.variant} ${fields.opt}: ${unsupportedReason(error.stderr)}`);
        return null;
      }
      throw error;
    }
    const report = compilerPhases ? parseTimePasses(result.stderr[0]) : null;
    after(sample, report, result);
    if (sample < 0) continue;
    for (const field of Object.keys(measured)) measured[field].push(...result[field]);
    if (report) phases.push(breakdown(report));
    if (command.output) {
      sizes.push(statSync(command.output).size);
      fingerprints.push(artifactFingerprint(command.output));
    }
  }
  if (deterministic && sizes.length) {
    const label = `${fields.workload} size=${fields.size} ${fields.variant} ${fields.target} ${fields.opt}`;
    assert.ok(sizes.every((size) => size === sizes[0]), `${label}: artifact size changed between identical builds`);
    // Reference compilers' debug metadata need not be reproducible; Tsuzuri's emitted code must be.
    if (compilerPhases && !fingerprints.every((fingerprint) => fingerprint === fingerprints[0])) {
      assert.ok(knownLinkVariation, `${label}: artifact code changed between identical builds: ${fingerprints.join(",")}`);
      console.error(`warning ${label}: pre-existing Mach-O import-stub ordering changed; checksums/IR/size verified; fingerprints recorded in projects.json`);
    }
  }
  const records = processRecords(context, fields, measured, { phases: compilerPhases });
  if (sizeMetric) records.push(makeRecord(context, { ...fields, metric: sizeMetric, samples: [sizes[0]] }));
  const summary = {
    ...fields, wall: summarize(measured.wall_ms).median, peak_rss: summarize(measured.peak_rss_bytes).median,
    ...Object.fromEntries(["frontend", "cache", "tools", "other"].map((name) => [name, phases.length ? summarize(phases.map((phase) => phase[name])).median : null])),
  };
  return { records, summary, ir_bytes: fields.variant === "emit-llvm" ? sizes[0] : null,
    linker_variation: deterministic && knownLinkVariation && new Set(fingerprints).size > 1 ? fingerprints : null };
}

const sleep = (milliseconds) => Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, milliseconds);

// The CPU time and the resident size of a running process, as `ps` (macOS) or /proc (Linux) say: the
// resolution is 10 ms.
function processUsage(pid) {
  if (platform() === "linux") {
    const stat = readFileSync(`/proc/${pid}/stat`, "utf8");
    const fields = stat.slice(stat.lastIndexOf(")") + 2).split(" ");
    return { user_ms: Number(fields[11]) * 10, sys_ms: Number(fields[12]) * 10, rss_bytes: Number(fields[21]) * 4096 };
  }
  const [user, sys, rss] = execFileSync("ps", ["-o", "utime=,stime=,rss=", "-p", String(pid)], { encoding: "utf8" }).trim().split(/\s+/);
  const clock = (text) => text.split(":").reduce((total, part) => total * 60 + Number(part), 0) * 1000;
  return { user_ms: clock(user), sys_ms: clock(sys), rss_bytes: Number(rss) * 1024 };
}

// PB06: a `tsuzuri serve` of the project, with the environment of the clients that are measured (and a
// directory for its socket that is short enough for a socket path).
function startServer(compiler, project, env) {
  const directory = mkdtempSync(join(tmpdir(), "tzb-"));
  // `measureProcess` runs the clients with LC_ALL=C, which a server compares with its own environment.
  const serverEnv = { ...env, LC_ALL: "C", TSUZURI_SERVER_DIR: join(directory, "s") };
  const log = join(directory, "server.log");
  const descriptor = openSync(log, "w");
  const child = spawn(compiler, ["serve", project, "--idle-timeout", "3600"], { cwd: project, env: serverEnv, stdio: ["ignore", "ignore", descriptor] });
  closeSync(descriptor);
  const ask = (operation) => spawnSync(compiler, ["serve", operation, project], { cwd: project, env: serverEnv, encoding: "utf8" });
  for (const started = Date.now(); ask("--status").status !== 0; sleep(20)) {
    assert.ok(Date.now() - started < 60_000, `the build server did not start:\n${readFileSync(log, "utf8")}`);
  }
  return {
    env: serverEnv, pid: child.pid, status: () => JSON.parse(ask("--status").stdout),
    stop() {
      ask("--stop");
      rmSync(directory, { recursive: true, force: true });
    },
  };
}

// PB06: the same command after the same kind of edit (the body of the module in the middle), run by a
// process of its own (--no-server) and by a build server, one sample of each in turn, the one that goes
// first alternating. Each sample is a different text, so every request is a miss of the artifact cache.
function measureServerPair(context, fields, { compiler, project, env, modules, kind, quick, work }) {
  const output = join(work, `app-${kind}-server`);
  const args = kind === "check" ? ["check", project] : ["build", project, "-O0", "--cpu", "generic", "-o", output];
  const arms = [
    { variant: `${kind}-edit-body-no-server`, served: false, args: [...args, "--no-server"], measured: { wall_ms: [], user_ms: [], sys_ms: [], peak_rss_bytes: [], stdout: [], stderr: [] } },
    { variant: `${kind}-edit-body-server`, served: true, args, measured: { wall_ms: [], user_ms: [], sys_ms: [], peak_rss_bytes: [], stdout: [], stderr: [] } },
  ];
  rmSync(env.TSUZURI_CACHE_DIR, { recursive: true, force: true });
  const server = startServer(compiler, project, env);
  const runs = quick ? 1 : 9, warmups = quick ? 0 : 1;
  const serverCpu = [];
  let salt = 2;
  try {
    // The caches (the runtime objects, the parsed sources) are warm, and so is the server.
    execute(compiler, [...args, "--no-server"], { cwd: project, env: server.env });
    for (let sample = -warmups; sample < runs; sample++) {
      for (const arm of (sample % 2 === 0 ? arms : [...arms].reverse())) {
        editModule(project, modules, "tsuzuri", "build-edit-body", salt++);
        const served = arm.served;
        const before = served ? server.status() : null;
        const cpuBefore = served ? processUsage(server.pid) : null;
        const result = measureProcess(compiler, arm.args, { cwd: project, env: server.env, runs: 1, warmups: 0 });
        if (served) {
          const cpuAfter = processUsage(server.pid);
          assert.equal(server.status().served, before.served + 1, `${arm.variant}: the server did not run the request`);
          if (sample >= 0) serverCpu.push(cpuAfter.user_ms + cpuAfter.sys_ms - cpuBefore.user_ms - cpuBefore.sys_ms);
        }
        if (kind === "build") {
          verifyOutput({ output, program: compiler, args: arm.args }, server.env, project, modules, { index: Math.floor(modules / 2), salt: salt - 1 });
        }
        if (sample < 0) continue;
        for (const field of Object.keys(arm.measured)) arm.measured[field].push(...result[field]);
      }
    }
    const final = server.status();
    const usage = processUsage(server.pid);
    return {
      arms: arms.map((arm) => {
        const phases = arm.measured.stderr.map((text) => breakdown(parseTimePasses(text)));
        return {
          records: processRecords(context, { ...fields, variant: arm.variant }, arm.measured),
          summary: { ...fields, variant: arm.variant, wall: summarize(arm.measured.wall_ms).median, peak_rss: summarize(arm.measured.peak_rss_bytes).median,
            ...Object.fromEntries(["frontend", "cache", "tools", "other"].map((name) => [name, summarize(phases.map((phase) => phase[name])).median])) },
        };
      }),
      server: { rss_bytes: usage.rss_bytes, resident_bytes: final.resident_bytes, served: final.served, source_hits: final.source_hits,
        source_reads: final.source_reads, digest_hits: final.digest_hits, digest_reads: final.digest_reads, refusals: final.refusals,
        cpu_ms_per_request: serverCpu.length ? summarize(serverCpu).median : null },
    };
  } finally {
    server.stop();
  }
}

// PB06: the time from the save of an edited module to the end of the run of `tsuzuri watch check`, and
// what the run reports (TSUZURI_TIME_PASSES). The 100 ms poll is in it.
function measureWatch(context, fields, { compiler, project, env, modules, quick, work }) {
  const log = join(work, "watch.log");
  const descriptor = openSync(log, "w");
  const child = spawn(compiler, ["watch", "check", project], { cwd: project, env, stdio: ["ignore", "ignore", descriptor] });
  closeSync(descriptor);
  const runsOf = () => [...readFileSync(log, "utf8").matchAll(/^watch: run (\d+) finished with exit code (\d+)$/gm)];
  const waitFor = (count) => {
    for (const started = Date.now(); runsOf().length < count; sleep(1)) assert.ok(Date.now() - started < 120_000, `no watch run ${count}:\n${readFileSync(log, "utf8")}`);
  };
  const measured = { wall_ms: [], user_ms: [], sys_ms: [], peak_rss_bytes: [], stdout: [], stderr: [] };
  const runs = quick ? 1 : 9, warmups = quick ? 0 : 1;
  try {
    waitFor(1);
    for (let sample = -warmups; sample < runs; sample++) {
      // The text of the run that is waited for is what the log has after the status line of the one before.
      const finished = runsOf().length;
      const before = readFileSync(log, "utf8");
      const usage = processUsage(child.pid);
      editModule(project, modules, "tsuzuri", "build-edit-body", sample + 3);
      const started = process.hrtime.bigint();
      waitFor(finished + 1);
      const wall = Number(process.hrtime.bigint() - started) / 1e6;
      const after = processUsage(child.pid);
      if (sample < 0) continue;
      const text = readFileSync(log, "utf8").slice(before.length);
      assert.match(text, /watch: run \d+ finished with exit code 0/, `a watch run failed:\n${text}`);
      measured.wall_ms.push(wall);
      measured.user_ms.push(after.user_ms - usage.user_ms);
      measured.sys_ms.push(after.sys_ms - usage.sys_ms);
      measured.peak_rss_bytes.push(after.rss_bytes);
      measured.stdout.push("");
      measured.stderr.push(text);
    }
    const phases = measured.stderr.map((text) => breakdown(parseTimePasses(text)));
    return {
      records: processRecords(context, fields, measured),
      summary: { ...fields, wall: summarize(measured.wall_ms).median, peak_rss: summarize(measured.peak_rss_bytes).median,
        ...Object.fromEntries(["frontend", "cache", "tools", "other"].map((name) => [name, summarize(phases.map((phase) => phase[name])).median])) },
    };
  } finally {
    child.kill("SIGTERM");
  }
}

function runBuild({ compiler, out, quick, sizes, selected, largeWasm, only = null }) {
  assert.ok(["darwin", "linux"].includes(platform()), "build metrics require macOS or Linux");
  assert.ok(!existsSync(join(out, "build.jsonl")) && !existsSync(join(out, "projects.json")), `EEXIST: ${out}`);
  const context = captureRun(out, { compiler, mode: quick ? "quick" : "benchmark" });
  const skipped = {}, skippedVariants = [], tools = {};
  const needs = new Set();
  for (const language of selected) {
    if (["c", "cpp"].includes(language)) { needs.add("clang"); needs.add("make"); }
    if (language.startsWith("rust")) { needs.add("rustc"); needs.add("cargo"); }
    if (["go", "zig"].includes(language)) needs.add(language);
  }
  const definitions = { clang: ["TSUZURI_CLANG", ["--version"]], make: ["TSUZURI_MAKE", ["--version"]],
    rustc: ["TSUZURI_RUSTC", ["--version"]], cargo: ["TSUZURI_CARGO", ["--version"]], go: ["TSUZURI_GO", ["version"]], zig: ["TSUZURI_ZIG", ["version"]] };
  for (const name of needs) tools[name] = tool(name, ...definitions[name], skipped);
  const available = selected.filter((language) => language === "tsuzuri" || ["c", "cpp"].includes(language) && tools.clang && tools.make
    || language.startsWith("rust") && tools.rustc && tools.cargo || language === "go" && tools.go || language === "zig" && tools.zig);
  assert.ok(available.length > 0, "no selected build compiler is available");
  const scratch = join(root, "target", "build-benchmark");
  mkdirSync(scratch, { recursive: true });
  const temporary = mkdtempSync(join(scratch, "run-"));
  const records = [], projects = [], summaries = [], jobs = cpus().length;
  try {
    const workloads = available.flatMap((language) => sizes.map((modules) => ({ language, modules, workload: "synthetic" })));
    if (available.includes("tsuzuri")) workloads.push(...(quick ? ["examples/hello"] : REAL_PROJECTS).map((workload) => ({ language: "tsuzuri", modules: null, workload })));
    for (const [number, workload] of workloads.entries()) {
      const { language, modules } = workload;
      const work = join(temporary, `work-${number}`);
      mkdirSync(work);
      const project = modules === null ? join(root, workload.workload) : join(work, "source");
      const options = { language: language === "rust-workspace" ? "rust" : language, rustLayout: language === "rust-workspace" ? "workspace" : "single" };
      const manifest = modules === null ? sourceStats(project) : writeProject(project, modules, options);
      const env = { ...process.env, TSUZURI_TIME_PASSES: "1", TSUZURI_CACHE_DIR: join(work, "cache"),
        GOENV: "off", GOTOOLCHAIN: "local", GOPROXY: "off", GOCACHE: join(work, "go-cache"), GOPATH: join(work, "go-path"),
        ZIG_GLOBAL_CACHE_DIR: join(work, "zig-global"), ZIG_LOCAL_CACHE_DIR: join(work, "zig-local"),
        CARGO_NET_OFFLINE: "true", ...(tools.rustc ? { RUSTC: tools.rustc.executable } : {}) };
      const commands = [];
      const descriptor = { workload: workload.workload, size: modules, language: options.language,
        ...(options.language === "rust" ? { layout: options.rustLayout } : {}), ...manifest, ir_bytes: null, commands };
      projects.push(descriptor);
      if (language.startsWith("rust")) execute(tools.cargo.executable, ["generate-lockfile", "--offline", "--manifest-path", join(project, "Cargo.toml")], { cwd: project, env });
      if (modules !== null) {
        for (const opt of ["O0", "O3"]) {
          const command = language === "tsuzuri" ? tsuzuriCommand(compiler, project, work, "build", "native", opt)
            : referenceCommand(language, project, opt, tools, jobs);
          execute(command.program, command.args, { cwd: project, env });
          verifyOutput(command, env, project, modules);
        }
      }
      const variants = language === "tsuzuri" ? VARIANTS : VARIANTS.filter(([name, target]) =>
        target === "native" && name.startsWith("build") && !name.startsWith("build-runtime") && !name.endsWith("-server"));
      for (const [variant, target, opt] of variants) {
        if (only !== null && !only.includes(variant)) continue;
        if (modules === null && variant.startsWith("build-edit")) continue;
        if (variant === "check-edit-body-server" || variant === "build-edit-body-server" || variant === "watch-edit-body") {
          // PB06: a resident process, on the synthetic projects (an edit of the module in the middle).
          if (modules === null) continue;
          const fieldsOf = (name) => ({ suite: "build", workload: workload.workload, size: modules, language: options.language, variant: name, target, opt });
          try {
            if (variant === "watch-edit-body") {
              const measured = measureWatch(context, fieldsOf(variant), { compiler, project, env, modules, quick, work });
              records.push(...measured.records);
              summaries.push({ ...measured.summary, lines: descriptor.lines });
            } else {
              const kind = variant.startsWith("check") ? "check" : "build";
              const measured = measureServerPair(context, fieldsOf(variant), { compiler, project, env, modules, kind, quick, work });
              for (const arm of measured.arms) {
                records.push(...arm.records);
                summaries.push({ ...arm.summary, lines: descriptor.lines });
              }
              (descriptor.servers ??= []).push({ kind, ...measured.server });
            }
          } finally {
            editModule(project, modules, "tsuzuri", "original", 0);
          }
          continue;
        }
        if (skipLargeWasm(modules, target, opt, largeWasm)) {
          const reason = "LLVM 21 exceeds the 600000ms process budget for large synthetic WASM -O3; use --large-wasm to require it";
          console.error(`skip synthetic ${modules} ${variant} O3: ${reason}`);
          skippedVariants.push({ workload: "synthetic", size: modules, language: "tsuzuri", variant, target, opt, reason });
          continue;
        }
        const command = language === "tsuzuri" ? tsuzuriCommand(compiler, project, work, variant, target, opt)
          : referenceCommand(language, project, opt, tools, jobs);
        const fields = { suite: "build", workload: workload.workload, size: modules, language: options.language,
          variant: language.startsWith("rust") ? `${options.rustLayout}-${variant}` : variant, target, opt };
        const cached = variant === "build-warm" || variant === "build-runtime-warm" || variant.startsWith("build-edit");
        if (cached) {
          if (language === "tsuzuri") rmSync(join(work, "cache"), { recursive: true, force: true });
          else resetReference(language, project, env);
          try {
            if (language === "tsuzuri") {
              const prime = measureProcess(command.program, command.args, { cwd: project, env, runs: 1, warmups: 0 });
              assert.ok(parseTimePasses(prime.stderr[0]).phases.some((phase) => phase.name.startsWith("tool.")), "cache priming unexpectedly hit artifact cache");
            } else {
              execute(command.program, command.args, { cwd: project, env });
            }
          } catch (error) {
            if (modules === null && error.status === 1 && /\bE2004\b/.test(error.stderr?.toString())) {
              console.error(`skip ${workload.workload} ${variant} ${opt}: ${unsupportedReason(error.stderr.toString())}`);
              continue;
            }
            throw error;
          }
        }
        const recordedCommand = { variant: fields.variant, target, opt, program: command.program, args: [...command.args] };
        commands.push(recordedCommand);
        try {
          const measured = measureVariant(context, fields, command, {
            cwd: project, env: language === "tsuzuri" && !cached && !variant.startsWith("build-runtime")
              ? { ...env, TSUZURI_CACHE_DIR: "" } : env, quick,
            compilerPhases: language === "tsuzuri", allowSkip: modules === null,
            sizeMetric: variant === "build" || variant === "build-wasm" ? target === "wasm32" ? "wasm_bytes" : "executable_bytes" : null,
            deterministic: !variant.startsWith("build-edit"),
            knownLinkVariation: platform() === "darwin" && language === "tsuzuri" && modules === 1000
              && variant === "build" && target === "native" && opt === "O0",
            before: (sample) => {
              if (variant === "build-runtime-cold") rmSync(env.TSUZURI_CACHE_DIR, { recursive: true, force: true });
              if (variant === "build-runtime-warm") {
                for (const name of readdirSync(env.TSUZURI_CACHE_DIR).filter(name => /^[a-f0-9]{64}$/.test(name))) {
                  rmSync(join(env.TSUZURI_CACHE_DIR, name), { recursive: true, force: true });
                }
              }
              if (variant === "build" && language !== "tsuzuri") resetReference(language, project, env);
              if (variant.startsWith("build-edit")) {
                const changed = editModule(project, modules, language, variant, sample + 2);
                if (["c", "cpp"].includes(language)) {
                  command.args = command.args.filter((argument) => !argument.startsWith("--what-if="));
                  command.args.push(...changed.map((file) => `--what-if=${file}`));
                  recordedCommand.args = [...command.args];
                  rmSync(command.output, { force: true });
                }
              }
            },
            after: (sample, report, result) => {
              if (report) {
                const toolPhases = report.phases.filter((phase) => phase.name.startsWith("tool."));
                if (variant === "emit-llvm" || variant === "check" || variant === "build-warm") assert.equal(toolPhases.length, 0, `${variant} unexpectedly ran a tool`);
                if (variant.startsWith("build-edit")) assert.ok(toolPhases.length > 0, `${variant} unexpectedly hit artifact cache`);
                if (variant.startsWith("build-runtime")) assert.ok(toolPhases.length > 0, `${variant} unexpectedly hit artifact cache`);
              }
              if (modules !== null && target === "native" && variant !== "check" && variant !== "emit-llvm") {
                verifyOutput({ ...command, diagnostic: `; variant=${variant}, build stdout:\n${result.stdout[0]}` },
                  env, project, modules, variant === "build-edit-body" ? { index: Math.floor(modules / 2), salt: sample + 2 } : {});
              }
            },
          });
          if (measured) {
            records.push(...measured.records);
            descriptor.ir_bytes ??= measured.ir_bytes;
            summaries.push({ ...measured.summary, lines: descriptor.lines });
            if (measured.linker_variation) {
              descriptor.linker_variation ??= [];
              descriptor.linker_variation.push({ variant, target, opt, fingerprints: measured.linker_variation });
            }
          }
        } finally {
          if (variant.startsWith("build-edit")) editModule(project, modules, language, "original", 0);
        }
      }
    }
    mkdirSync(out, { recursive: true });
    writeRecords(out, "build", records);
    writeFileSync(join(out, "projects.json"), JSON.stringify({
      schema: 1, jobs, toolchains: Object.fromEntries(Object.entries(tools).filter(([, value]) => value).map(([name, value]) => [name, value.version])),
      go_environment: { GOGC: process.env.GOGC ?? "default", GOMAXPROCS: process.env.GOMAXPROCS ?? "default" }, skipped, skipped_variants: skippedVariants, projects,
    }, null, 2) + "\n", { flag: "wx" });
    console.log("| project | modules | language / variant | target | opt | wall ms | frontend ms | cache ms | tools ms | other ms | peak RSS MiB | check lines/s |");
    console.log("| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |");
    const ms = (value) => value === null ? "-" : value.toFixed(3);
    for (const row of summaries) console.log(`| ${row.workload} | ${row.size ?? "-"} | ${row.language} / ${row.variant} | ${row.target} | ${row.opt} | ${ms(row.wall)} | ${ms(row.frontend)} | ${ms(row.cache)} | ${ms(row.tools)} | ${ms(row.other)} | ${(row.peak_rss / 1048576).toFixed(3)} | ${row.variant === "check" && row.wall > 0 ? Math.round(row.lines * 1000 / row.wall) : "-"} |`);
    // PB06: what the build servers held, and what they cost (the peak RSS of the table is the client's).
    for (const project of projects) for (const server of project.servers ?? []) {
      console.log(`server (${project.workload} ${project.size} ${server.kind}): RSS ${(server.rss_bytes / 1048576).toFixed(1)} MiB, resident ${(server.resident_bytes / 1048576).toFixed(2)} MiB, `
        + `${server.served} requests, CPU ${server.cpu_ms_per_request === null ? "-" : server.cpu_ms_per_request.toFixed(0)} ms per request, source hits/reads ${server.source_hits}/${server.source_reads}, digest hits/reads ${server.digest_hits}/${server.digest_reads}`);
    }
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const usage = "Usage: node benchmarks/run-build.mjs [compiler] --out dir [--quick] [--large-wasm] [--sizes N[,N...]] [--languages tsuzuri,c,cpp,rust,rust-workspace,go,zig] [--variants NAME[,NAME...]]";
  const args = process.argv.slice(2), options = {}, positional = [];
  let quick = false, largeWasm = false;
  for (let index = 0; index < args.length; index++) {
    const flag = args[index];
    if (flag === "--quick" && !quick) quick = true;
    else if (flag === "--large-wasm" && !largeWasm) largeWasm = true;
    else if (["--out", "--sizes", "--languages", "--variants"].includes(flag) && options[flag] === undefined) {
      const value = args[++index];
      assert.ok(value && !value.startsWith("-"), usage);
      options[flag] = value;
    } else {
      assert.ok(!flag.startsWith("-"), usage);
      positional.push(flag);
    }
  }
  assert.ok(options["--out"] && positional.length <= 1, usage);
  const sizeText = options["--sizes"] ?? (quick ? "10" : "200,1000");
  assert.match(sizeText, /^\d+(,\d+)*$/, usage);
  const sizes = sizeText.split(",").map(Number);
  assert.ok(sizes.every((size) => Number.isSafeInteger(size) && size >= 1 && size <= 5000) && new Set(sizes).size === sizes.length, usage);
  const selected = (options["--languages"] ?? languages.join(",")).split(",");
  assert.ok(selected.length > 0 && selected.every((language) => languages.includes(language)) && new Set(selected).size === selected.length, usage);
  // Only these variants (by name), for a measurement of one thing; all of them when there is no option.
  const only = options["--variants"]?.split(",") ?? null;
  assert.ok(only === null || only.every((name) => VARIANTS.some(([variant]) => variant === name)), `unknown variant in --variants; ${VARIANTS.map(([variant]) => variant).filter((name, at, names) => names.indexOf(name) === at).join(", ")}`);
  runBuild({ compiler: resolve(positional[0] ?? join(root, "target/release/tsuzuri")), out: resolve(options["--out"]), quick, sizes, selected, largeWasm, only });
}
