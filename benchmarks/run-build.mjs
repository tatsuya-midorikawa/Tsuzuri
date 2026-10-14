import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
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
]);
const languages = ["tsuzuri", "c", "cpp", "rust", "rust-workspace", "go", "zig"];
const execute = (program, args, options = {}) => execFileSync(program, args,
  { cwd: root, encoding: "utf8", timeout: 600_000, maxBuffer: 256 * 1024 * 1024, stdio: ["pipe", "pipe", "pipe"], ...options });

export function breakdown(report) {
  let frontend = 0, cache = 0, tools = 0;
  for (const phase of report.phases) {
    if (phase.name === "cache") cache += phase.ms;
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

function runBuild({ compiler, out, quick, sizes, selected, largeWasm }) {
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
  const temporary = mkdtempSync(join(tmpdir(), "tsuzuri-build-bench-"));
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
      const variants = language === "tsuzuri" ? VARIANTS : VARIANTS.filter(([name, target]) => target === "native" && name.startsWith("build"));
      for (const [variant, target, opt] of variants) {
        if (modules === null && variant.startsWith("build-edit")) continue;
        if (modules > 200 && variant === "build-wasm" && opt === "O3" && !largeWasm) {
          const reason = "LLVM 21 exceeds the 600000ms process budget for large synthetic WASM -O3; use --large-wasm to require it";
          console.error(`skip synthetic ${modules} build-wasm O3: ${reason}`);
          skippedVariants.push({ workload: "synthetic", size: modules, language: "tsuzuri", variant, target, opt, reason });
          continue;
        }
        const command = language === "tsuzuri" ? tsuzuriCommand(compiler, project, work, variant, target, opt)
          : referenceCommand(language, project, opt, tools, jobs);
        const fields = { suite: "build", workload: workload.workload, size: modules, language: options.language,
          variant: language.startsWith("rust") ? `${options.rustLayout}-${variant}` : variant, target, opt };
        const cached = variant === "build-warm" || variant.startsWith("build-edit");
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
            cwd: project, env: language === "tsuzuri" && !cached ? { ...env, TSUZURI_CACHE_DIR: "" } : env, quick,
            compilerPhases: language === "tsuzuri", allowSkip: modules === null,
            sizeMetric: variant === "build" || variant === "build-wasm" ? target === "wasm32" ? "wasm_bytes" : "executable_bytes" : null,
            deterministic: !variant.startsWith("build-edit"),
            knownLinkVariation: platform() === "darwin" && language === "tsuzuri" && modules === 1000
              && variant === "build" && target === "native" && opt === "O0",
            before: (sample) => {
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
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const usage = "Usage: node benchmarks/run-build.mjs [compiler] --out dir [--quick] [--large-wasm] [--sizes N[,N...]] [--languages tsuzuri,c,cpp,rust,rust-workspace,go,zig]";
  const args = process.argv.slice(2), options = {}, positional = [];
  let quick = false, largeWasm = false;
  for (let index = 0; index < args.length; index++) {
    const flag = args[index];
    if (flag === "--quick" && !quick) quick = true;
    else if (flag === "--large-wasm" && !largeWasm) largeWasm = true;
    else if (["--out", "--sizes", "--languages"].includes(flag) && options[flag] === undefined) {
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
  runBuild({ compiler: resolve(positional[0] ?? join(root, "target/release/tsuzuri")), out: resolve(options["--out"]), quick, sizes, selected, largeWasm });
}
