import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { arch, cpus, loadavg, platform, release, tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { performance } from "node:perf_hooks";
import { fileURLToPath } from "node:url";
import { hex, reference, workloads as appDefinitions } from "./apps/reference.mjs";
import * as kernels from "./heap/reference.mjs";

// PM05: the WASM heap, measured from outside. Each workload is built for wasm32 at -O3 with the compiler (and with
// --baseline, an older compiler, interleaved sample by sample) and run in Node.js; the footprint is the size of the
// module's memory after the run, which is the high-water mark of the heap in pages because memory is never given back.
// No threshold: it prints what was measured, with the load average of the machine at the start and the end.
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const usage = "Usage: node benchmarks/run-heap.mjs [compiler] [--baseline compiler] [--samples n | --quick] [--capacity] [--json file]";
const seed = 42n;

export function parseArguments(args) {
  const options = { compiler: join(root, "target/release/tsuzuri"), baseline: null, samples: 9, quick: false, capacity: false, json: null };
  let positional = false;
  for (let index = 0; index < args.length; ++index) {
    const flag = args[index];
    if (!flag.startsWith("-")) {
      if (positional) throw new Error(`unexpected positional argument: ${flag}\n${usage}`);
      options.compiler = resolve(flag);
      positional = true;
    } else if (flag === "--quick") {
      options.quick = true;
      options.samples = 3;
    } else if (flag === "--capacity") {
      options.capacity = true;
    } else if (["--baseline", "--samples", "--json"].includes(flag)) {
      const value = args[++index];
      if (!value || value.startsWith("-")) throw new Error(`missing value for ${flag}\n${usage}`);
      if (flag === "--samples") {
        if (!/^[1-9]\d{0,2}$/.test(value)) throw new Error(`invalid --samples: ${value}\n${usage}`);
        options.samples = Number(value);
      } else options[flag.slice(2)] = resolve(value);
    } else throw new Error(`unknown flag: ${flag}\n${usage}`);
  }
  return options;
}

// Sizes at which the default heap of 16 MiB suffices for every heap measured (--capacity finds the largest ones).
function workloadList(quick) {
  const shrink = (value) => (quick ? Math.max(1, Math.floor(value / 10)) : value);
  const churn = (name, slots, rounds, spread) => {
    const count = shrink(rounds);
    return { suite: "heap", fixture: "heap", name, export: "steady_churn", args: [BigInt(slots), BigInt(count), BigInt(spread), seed], expected: () => kernels.steadyChurn(slots, count, spread, seed) };
  };
  const growth = (name, count, rounds) => {
    const repeats = shrink(rounds);
    return { suite: "heap", fixture: "heap", name, export: "vec_growth", args: [BigInt(count), BigInt(repeats), seed], expected: () => kernels.vecGrowth(count, repeats, seed) };
  };
  const app = (name, size) => {
    const n = shrink(size);
    return { suite: "apps", fixture: "apps", name, export: name, args: [BigInt(n), seed], expected: () => reference(name, n, seed) };
  };
  return [
    churn("small_churn", 4000, 400000, 6), churn("mixed_churn", 2000, 200000, 200), growth("vec_growth", 200000, 20),
    app("tokenize", 60000), app("format_join", 30000), app("csv_aggregate", 60000), app("record_sort", 60000),
    app("tree_transform", 128), app("binary_trees", 1000), app("graph_bfs", 50000),
  ];
}

function build(compiler, fixture, directory, label) {
  const output = join(directory, `${label}-${fixture}.wasm`);
  const source = fixture === "heap" ? join(root, "benchmarks/heap/Main.tz") : join(root, "benchmarks/apps");
  const child = spawnSync(compiler, ["build", source, "--target", "wasm32", "-O3", "--no-cache", "-o", output], { cwd: root, encoding: "utf8", timeout: 600_000 });
  assert.equal(child.status, 0, `${compiler} build ${fixture}\n${child.stdout}\n${child.stderr}`);
  const module = new WebAssembly.Module(readFileSync(output));
  assert.deepEqual(WebAssembly.Module.imports(module), [], `${fixture}: standalone WASM`);
  return module;
}

function execute(module, workload, args = workload.args) {
  const instance = new WebAssembly.Instance(module, {});
  const start = performance.now();
  let result = null, trapped = false;
  try {
    result = instance.exports[`tz_${workload.export}`](...args);
  } catch (error) {
    if (!(error instanceof WebAssembly.RuntimeError)) throw error;
    trapped = true;
  }
  return { ms: performance.now() - start, memory: instance.exports.memory.buffer.byteLength, result, trapped };
}

const sorted = (values) => [...values].sort((a, b) => a - b);
const median = (values) => sorted(values)[Math.floor(values.length / 2)];
const fixed = (value) => (value >= 100 ? value.toFixed(0) : value >= 10 ? value.toFixed(1) : value.toFixed(2));
const mebibytes = (bytes) => `${(bytes / 1048576).toFixed(2)} MiB`;
const sha256 = (file) => createHash("sha256").update(readFileSync(file)).digest("hex");

function measure(workloads, compilers, modules, samples) {
  const results = [];
  for (const workload of workloads) {
    const expected = hex(workload.expected());
    const record = { suite: workload.suite, workload: workload.name, args: workload.args.map(String), runs: {} };
    for (const { label } of compilers) record.runs[label] = { samples_ms: [], memory_bytes: null, trapped: false };
    for (let sample = -1; sample < samples; ++sample) {
      // Alternating the order of the compilers in each sample keeps drift from favouring either one.
      const order = sample % 2 === 0 ? compilers : [...compilers].reverse();
      for (const { label } of order) {
        const run = execute(modules[label][workload.fixture], workload);
        const entry = record.runs[label];
        if (run.trapped) {
          entry.trapped = true;
          continue;
        }
        assert.equal(hex(run.result), expected, `${label}/${workload.name}: checksum`);
        if (sample < 0) continue;
        entry.samples_ms.push(run.ms);
        if (entry.memory_bytes === null) entry.memory_bytes = run.memory;
        else assert.equal(run.memory, entry.memory_bytes, `${label}/${workload.name}: the footprint is the same every run`);
      }
    }
    for (const entry of Object.values(record.runs)) {
      const times = entry.samples_ms;
      Object.assign(entry, times.length ? { min_ms: Math.min(...times), median_ms: median(times), max_ms: Math.max(...times) } : {});
    }
    results.push(record);
  }
  return results;
}

// The largest size of each app that still runs, from the quick size in steps of 1.5, with its footprint.
function capacities(compilers, modules) {
  const found = [];
  for (const definition of appDefinitions) {
    const record = { workload: definition.name, runs: {} };
    for (const { label } of compilers) {
      let size = definition.quick, best = null;
      for (let step = 0; step < 14 && size <= definition.maximum; ++step) {
        const run = execute(modules[label].apps, { export: definition.name }, [BigInt(size), seed]);
        if (run.trapped) break;
        best = { size, memory_bytes: run.memory };
        size = Math.ceil(size * 1.5);
      }
      record.runs[label] = best;
    }
    found.push(record);
  }
  return found;
}

function table(results, compilers) {
  const [first, second] = compilers;
  const head = second
    ? `| workload | ${first.label} ms (min / median / max) | ${second.label} ms (min / median / max) | ${second.label} / ${first.label} | ${first.label} memory | ${second.label} memory |\n|---|---|---|---|---|---|`
    : `| workload | ${first.label} ms (min / median / max) | memory |\n|---|---|---|`;
  const cell = (entry) => (entry.trapped ? "trap" : `${fixed(entry.min_ms)} / ${fixed(entry.median_ms)} / ${fixed(entry.max_ms)}`);
  const memory = (entry) => (entry.trapped ? "trap" : mebibytes(entry.memory_bytes));
  const rows = results.map(({ workload, runs }) => {
    if (!second) return `| ${workload} | ${cell(runs[first.label])} | ${memory(runs[first.label])} |`;
    const a = runs[first.label], b = runs[second.label];
    const ratio = a.trapped || b.trapped ? "-" : (b.median_ms / a.median_ms).toFixed(2);
    return `| ${workload} | ${cell(a)} | ${cell(b)} | ${ratio} | ${memory(a)} | ${memory(b)} |`;
  });
  return [head, ...rows].join("\n");
}

function main(args) {
  const options = parseArguments(args);
  kernels.checkReference();
  const compilers = [...(options.baseline ? [{ label: "baseline", path: options.baseline }] : []), { label: "current", path: options.compiler }];
  const directory = mkdtempSync(join(tmpdir(), "tsuzuri-heap-bench-"));
  try {
    const modules = {};
    for (const { label, path } of compilers) modules[label] = { heap: build(path, "heap", directory, label), apps: build(path, "apps", directory, label) };
    const load = [loadavg()];
    const startedAt = new Date().toISOString();
    const results = measure(workloadList(options.quick), compilers, modules, options.samples);
    const capacity = options.capacity ? capacities(compilers, modules) : null;
    load.push(loadavg());
    console.log(table(results, compilers));
    if (capacity) {
      console.log("\n| app | " + compilers.map(({ label }) => `${label} largest size (memory)`).join(" | ") + " |\n|---|" + compilers.map(() => "---|").join(""));
      for (const { workload, runs } of capacity) console.log(`| ${workload} | ${compilers.map(({ label }) => (runs[label] ? `${runs[label].size} (${mebibytes(runs[label].memory_bytes)})` : "none")).join(" | ")} |`);
    }
    console.log(`\nsamples: ${options.samples} after one warm-up run each; load average at the start ${load[0].map((value) => value.toFixed(2)).join(" ")}, at the end ${load[1].map((value) => value.toFixed(2)).join(" ")}`);
    if (options.json) {
      const record = {
        schema: 1, started_at: startedAt, samples: options.samples,
        host: { os: `${platform()} ${release()}`, arch: arch(), cpu: cpus()[0]?.model, cores: cpus().length, node: process.version, load_average: load },
        compilers: Object.fromEntries(compilers.map(({ label, path }) => [label, sha256(path)])),
        results, capacity,
      };
      writeFileSync(options.json, `${JSON.stringify(record, null, 2)}\n`);
    }
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main(process.argv.slice(2));
