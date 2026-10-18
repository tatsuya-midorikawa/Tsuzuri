// Compares the two code generators of `tsuzuri build` (PB05): LLVM (Clang at -O0) and fast (the Cranelift
// translator of --backend fast) on the synthetic projects of benchmarks/build/generate.mjs.
//
//   node benchmarks/run-backend.mjs [COMPILER] --out DIR [--sizes 200,1000] [--samples 9]
//        [--scenarios build,build-edit-body,build-edit-signature] [--target native|wasm32]
//
// COMPILER is a tsuzuri built with `cargo build --release --features fast-backend`
// (default target/release/tsuzuri). Each sample builds with one backend and then with the other (the
// order alternates), after the same edit of the same module, so a slow moment of the machine hits both.
// `build` is a cold `--no-cache` build; the two edit scenarios keep a cache of their own per backend and
// change one module in the middle of the project: its function bodies (`build-edit-body`) or its list of
// exports (`build-edit-signature`). The last build of every scenario is run and its output is compared
// with the reference of the generator (a wasm32 module is run in this process, with a host that reads one
// line and writes to a buffer). Wall time, CPU time (user + system, children included), peak
// memory and the compiler's own phases are written to DIR/backend.jsonl and summarized on stdout.
import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { mkdirSync, readFileSync, rmSync, statSync, writeFileSync, appendFileSync, existsSync } from "node:fs";
import { cpus, loadavg, platform, release } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { measureProcess, parseTimePasses, summarize } from "./metrics.mjs";
import { expectedOutput, moduleFiles, writeProject } from "./build/generate.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const SCENARIOS = ["build", "build-edit-body", "build-edit-signature"];
const BACKENDS = ["llvm", "fast"];

function editModule(project, modules, scenario, sample) {
  const index = Math.floor(modules / 2);
  const options = scenario === "build-edit-body" ? { salt: sample } : { extra: sample };
  for (const [name, text] of moduleFiles(index, options)) {
    const file = join(project, name);
    if (readFileSync(file, "utf8") !== text) writeFileSync(file, text);
  }
  return index;
}

// The compiler's phases: the frontend and the rest of the compiler, the translator (fast), and the
// tools (Clang compiles and links for llvm; for fast it links only).
function breakdown(report) {
  const sum = (predicate) => report.phases.filter((phase) => predicate(phase.name)).reduce((total, phase) => total + phase.ms, 0);
  const tools = sum((name) => name.startsWith("tool."));
  const translator = sum((name) => name.startsWith("backend."));
  const cache = sum((name) => name === "cache");
  return { total: report.total_ms, tools, translator, cache, frontend: report.total_ms - tools - translator - cache };
}

function verify(compiler, app, modules, edited, label) {
  for (const line of ["", "12345", "綴り😀"]) {
    const output = execFileSync(app, [], { input: `${line}\n`, encoding: "utf8", timeout: 60_000 });
    assert.equal(output, expectedOutput(modules, line, edited), `${label}: the program prints another result for ${JSON.stringify(line)}`);
  }
}

// The same check for a wasm32 module: the host of tests/io.mjs, reading one line and then the end of the input.
function verifyWasm(file, modules, edited, label) {
  for (const line of ["", "12345", "綴り😀"]) {
    let instance;
    let reads = 0;
    let output = "";
    instance = new WebAssembly.Instance(new WebAssembly.Module(readFileSync(file)), { tsuzuri_io: {
      read_line(out) {
        const bytes = reads++ === 0 ? Buffer.from(line) : null;
        const length = bytes?.length ?? 0;
        const pointer = length ? instance.exports.tsuzuri_alloc(BigInt(length)) : 0;
        if (length) new Uint8Array(instance.exports.memory.buffer, pointer, length).set(bytes);
        const view = new DataView(instance.exports.memory.buffer);
        view.setUint32(out, pointer, true);
        view.setBigInt64(out + 8, BigInt(length), true);
        return bytes ? 0 : 1;
      },
      write(destination, pointer, length) {
        assert.ok(destination === 1 || destination === 2);
        if (destination === 1) output += Buffer.from(new Uint8Array(instance.exports.memory.buffer, pointer, Number(length))).toString();
        return 0;
      },
    } });
    instance.exports.tsuzuri_main();
    assert.equal(output, expectedOutput(modules, line, edited), `${label}: the module prints another result for ${JSON.stringify(line)}`);
  }
}

function run({ compiler, out, sizes, samples, scenarios, target }) {
  assert.ok(["darwin", "linux"].includes(platform()), "the measurement needs macOS or Linux");
  const jsonl = join(out, "backend.jsonl");
  assert.ok(!existsSync(jsonl), `${jsonl} exists`);
  mkdirSync(out, { recursive: true });
  const version = spawnSync(compiler, ["--version"], { encoding: "utf8" }).stdout.trim();
  const environment = { compiler, version, target, platform: `${platform()} ${release()}`, cpus: cpus().length, model: cpus()[0]?.model,
    load_before: loadavg(), date: new Date().toISOString(), samples };
  writeFileSync(join(out, "environment.json"), `${JSON.stringify(environment, null, 2)}\n`);
  const summaries = [];
  for (const modules of sizes) {
    const work = join(out, `size-${modules}`);
    rmSync(work, { recursive: true, force: true });
    mkdirSync(work, { recursive: true });
    const project = join(work, "project");
    writeProject(project, modules);
    const suffix = target === "wasm32" ? ".wasm" : "";
    const apps = { llvm: join(work, `app-llvm${suffix}`), fast: join(work, `app-fast${suffix}`) };
    const caches = { llvm: join(work, "cache-llvm"), fast: join(work, "cache-fast") };
    const command = (backend, scenario) => ({
      program: compiler,
      args: ["build", project, "-O0", ...(target === "wasm32" ? ["--target", "wasm32"] : ["--cpu", "generic"]), ...(scenario === "build" ? ["--no-cache"] : []), "--backend", backend, "-o", apps[backend]],
      env: { ...process.env, TSUZURI_TIME_PASSES: "1", TSUZURI_CACHE_DIR: caches[backend] },
    });
    const measure = (backend, scenario) => {
      const { program, args, env } = command(backend, scenario);
      const result = measureProcess(program, args, { cwd: root, env, runs: 1, warmups: 0 });
      return { wall_ms: result.wall_ms[0], cpu_ms: result.user_ms[0] + result.sys_ms[0], peak_rss_bytes: result.peak_rss_bytes[0],
        phases: breakdown(parseTimePasses(result.stderr[0])), app_bytes: statSync(apps[backend]).size };
    };
    for (const scenario of scenarios) {
      const rows = { llvm: [], fast: [] };
      // The first build of a scenario fills the caches and is not counted.
      for (const backend of BACKENDS) measure(backend, scenario);
      let edited = {};
      for (let sample = 1; sample <= samples; sample++) {
        if (scenario !== "build") {
          const index = editModule(project, modules, scenario, sample);
          edited = scenario === "build-edit-body" ? { index, salt: sample } : {};
        }
        for (const backend of sample % 2 === 1 ? BACKENDS : [...BACKENDS].reverse()) {
          const row = { modules, scenario, backend, sample, ...measure(backend, scenario) };
          rows[backend].push(row);
          appendFileSync(jsonl, `${JSON.stringify(row)}\n`);
        }
      }
      for (const backend of BACKENDS) {
        const label = `${modules} modules, ${scenario}, ${backend}`;
        if (target === "wasm32") verifyWasm(apps[backend], modules, edited, label);
        else verify(compiler, apps[backend], modules, edited, label);
      }
      const median = (backend, pick) => summarize(rows[backend].map(pick)).median;
      const summary = { modules, scenario };
      for (const backend of BACKENDS) {
        summary[backend] = {
          wall_ms: summarize(rows[backend].map((row) => row.wall_ms)),
          cpu_ms: summarize(rows[backend].map((row) => row.cpu_ms)),
          total_ms: median(backend, (row) => row.phases.total),
          tools_ms: median(backend, (row) => row.phases.tools),
          translator_ms: median(backend, (row) => row.phases.translator),
          frontend_ms: median(backend, (row) => row.phases.frontend),
          peak_rss_mb: median(backend, (row) => row.peak_rss_bytes) / 1048576,
          app_kb: rows[backend][0].app_bytes / 1024,
        };
      }
      summaries.push(summary);
      const fast = summary.fast, llvm = summary.llvm;
      console.log(`${modules} modules ${scenario}: wall llvm ${llvm.wall_ms.median.toFixed(0)} ms [${llvm.wall_ms.min.toFixed(0)}-${llvm.wall_ms.max.toFixed(0)}]`
        + ` fast ${fast.wall_ms.median.toFixed(0)} ms [${fast.wall_ms.min.toFixed(0)}-${fast.wall_ms.max.toFixed(0)}] (${(fast.wall_ms.median / llvm.wall_ms.median).toFixed(2)}x);`
        + ` cpu llvm ${llvm.cpu_ms.median.toFixed(0)} fast ${fast.cpu_ms.median.toFixed(0)} ms (${(fast.cpu_ms.median / llvm.cpu_ms.median).toFixed(2)}x);`
        + ` tools llvm ${llvm.tools_ms.toFixed(0)} fast ${fast.tools_ms.toFixed(0)} ms, translator fast ${fast.translator_ms.toFixed(0)} ms;`
        + ` rss llvm ${llvm.peak_rss_mb.toFixed(0)} fast ${fast.peak_rss_mb.toFixed(0)} MB; app llvm ${llvm.app_kb.toFixed(0)} fast ${fast.app_kb.toFixed(0)} KB`);
    }
  }
  environment.load_after = loadavg();
  writeFileSync(join(out, "environment.json"), `${JSON.stringify(environment, null, 2)}\n`);
  writeFileSync(join(out, "summary.json"), `${JSON.stringify(summaries, null, 2)}\n`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const usage = "usage: node benchmarks/run-backend.mjs [COMPILER] --out DIR [--sizes 200,1000] [--samples 9] [--scenarios build,build-edit-body,build-edit-signature] [--target native|wasm32]";
  const args = process.argv.slice(2), options = {}, positional = [];
  for (let index = 0; index < args.length; index++) {
    if (["--out", "--sizes", "--samples", "--scenarios", "--target"].includes(args[index]) && options[args[index]] === undefined) options[args[index]] = args[++index];
    else if (args[index].startsWith("--")) assert.fail(usage);
    else positional.push(args[index]);
  }
  assert.ok(options["--out"] && positional.length <= 1, usage);
  const sizes = (options["--sizes"] ?? "200,1000").split(",").map(Number);
  assert.ok(sizes.every((size) => Number.isSafeInteger(size) && size > 0), usage);
  const samples = Number(options["--samples"] ?? 9);
  assert.ok(Number.isSafeInteger(samples) && samples > 0, usage);
  const scenarios = (options["--scenarios"] ?? SCENARIOS.join(",")).split(",");
  assert.ok(scenarios.every((scenario) => SCENARIOS.includes(scenario)), usage);
  const target = options["--target"] ?? "native";
  assert.ok(["native", "wasm32"].includes(target), usage);
  run({ compiler: resolve(positional[0] ?? join(root, "target/release/tsuzuri")), out: resolve(options["--out"]), sizes, samples, scenarios, target });
}
