import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { arch, cpus, loadavg, platform, release, tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { captureRun, instrumentAllocations, makeRecord, measureProcess, median, writeRecords } from "./metrics.mjs";
import { reference, repeats, sizes, workloads } from "./list-memory/reference.mjs";

// PM04: what a list costs, measured from outside. benchmarks/list-memory is built with the compiler as a native library at -O3 in two
// ways: its IR with the allocator calls replaced by benchmarks/tracked_alloc.c, which counts calls and requested bytes (deterministic, one
// call per size), and the object that Tsuzuri produces, which times the workloads and gives the peak RSS of a process that runs one of them.
// With --baseline the same is done for an older compiler (variant `before`) and the samples alternate between the two. --wasm adds the heap
// high-water mark of each workload as wasm32 (the size of the module's memory after the run). No threshold: it records what was measured.
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const usage = "Usage: node benchmarks/run-list-memory.mjs [compiler] [--baseline compiler] [--quick] [--samples n] [--only name,...] [--wasm] [--metrics directory] [--json file]";

export function parseArguments(args) {
  const options = { compiler: join(root, "target/release/tsuzuri"), baseline: null, quick: false, samples: 9, only: [], wasm: false, metrics: null, json: null };
  let positional = false, samplesGiven = false;
  const value = (index, flag) => {
    if (index + 1 >= args.length || args[index + 1].startsWith("--")) throw new Error(`missing value for ${flag}\n${usage}`);
    return args[index + 1];
  };
  for (let index = 0; index < args.length; ++index) {
    const flag = args[index];
    if (!flag.startsWith("--")) {
      if (positional) throw new Error(`unexpected positional argument: ${flag}\n${usage}`);
      options.compiler = resolve(flag);
      positional = true;
    } else if (flag === "--quick") options.quick = true;
    else if (flag === "--wasm") options.wasm = true;
    else if (flag === "--baseline") options.baseline = resolve(value(index++, flag));
    else if (flag === "--metrics") options.metrics = resolve(value(index++, flag));
    else if (flag === "--json") options.json = resolve(value(index++, flag));
    else if (flag === "--samples") {
      const text = value(index++, flag);
      if (!/^[1-9]\d{0,2}$/.test(text)) throw new Error(`invalid --samples: ${text}\n${usage}`);
      options.samples = Number(text);
      samplesGiven = true;
    } else if (flag === "--only") {
      options.only = value(index++, flag).split(",").filter(Boolean);
      for (const name of options.only) if (!workloads.includes(name)) throw new Error(`unknown workload: ${name}\n${usage}`);
    } else throw new Error(`unknown flag: ${flag}\n${usage}`);
  }
  if (options.quick && !samplesGiven) options.samples = 1;
  if (!options.quick && options.samples < 9 && options.metrics) throw new Error(`--metrics records need at least 9 samples unless --quick\n${usage}`);
  return options;
}

function run(program, programArguments) {
  const result = spawnSync(program, programArguments, { cwd: root, encoding: "utf8", maxBuffer: 256 * 1024 * 1024, timeout: 600_000 });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${program} ${programArguments.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result.stdout;
}

const sha256 = (file) => createHash("sha256").update(readFileSync(file)).digest("hex");

// The native library of the fixture at -O3: the object that runs, and the tracked build whose IR was optimized first and instrumented after.
function buildNative(compiler, label, directory, clang, tracker) {
  const work = join(directory, label);
  mkdirSync(work);
  const fixture = join(root, "benchmarks/list-memory");
  const flags = ["-O3", "-fPIC", "-fno-fast-math", "-ffp-contract=off"];
  const ir = join(work, "list-memory.ll"), object = join(work, "list-memory.o");
  run(compiler, ["build", fixture, "--emit", "llvm", "-O3", "-o", ir]);
  run(compiler, ["build", fixture, "--emit", "object", "-O3", "--cpu", "generic", "-o", object]);
  const optimized = join(work, "list-memory.optimized.ll"), trackedIr = join(work, "list-memory.tracked.ll"), trackedObject = join(work, "list-memory.tracked.o");
  run(clang, [...flags, "-Wno-override-module", "-S", "-emit-llvm", ir, "-o", optimized]);
  writeFileSync(trackedIr, instrumentAllocations(readFileSync(optimized, "utf8")));
  run(clang, ["-O0", "-fPIC", "-Wno-override-module", "-c", trackedIr, "-o", trackedObject]);
  const host = join(fixture, "host.c"), timed = join(work, "timed"), tracked = join(work, "tracked");
  const checks = ["-std=c11", ...flags, "-Wall", "-Wextra", "-Werror"];
  run(clang, [...checks, host, object, "-lm", "-o", timed]);
  run(clang, [...checks, "-DTRACKING", host, trackedObject, tracker, "-lm", "-o", tracked]);
  return { timed, tracked };
}

function allocations(tracked, name, size) {
  const result = JSON.parse(run(tracked, [name, String(size)]));
  assert.equal(BigInt(result.result), reference(name, size), `${name}(${size}): checksum`);
  assert.equal(result.live, 0, `${name}(${size}): every allocation is freed`);
  return { calls: result.alloc_calls, bytes: result.alloc_bytes };
}

function sample(timed, name, size, quick) {
  const measured = measureProcess(timed, [name, String(size), String(repeats(name, size, quick))], { runs: 1, warmups: 0 });
  const result = JSON.parse(measured.stdout[0]);
  assert.equal(BigInt(result.result), reference(name, size), `${name}(${size}): checksum`);
  return { wall_ms: result.wall_ms, cpu_ms: result.cpu_ms, peak_rss: measured.peak_rss_bytes[0] };
}

// wasm32: the memory of the instance after one call is the high-water mark of the heap, which is never given back; a trap is an out-of-memory.
function wasmHeap(compiler, label, directory, grid, quick) {
  const output = join(directory, `${label}.wasm`);
  run(compiler, ["build", join(root, "benchmarks/list-memory"), "--target", "wasm32", "-O3", "-o", output]);
  const module = new WebAssembly.Module(readFileSync(output));
  assert.deepEqual(WebAssembly.Module.imports(module), [], "standalone WASM");
  const initial = new WebAssembly.Instance(module, {}).exports.memory.buffer.byteLength;
  const rows = [];
  for (const name of grid.names) for (const size of quick ? [1000] : name === "index_walk" ? [1000, 10000] : grid.sizes) {
    const instance = new WebAssembly.Instance(module, {});
    let trapped = false;
    try {
      assert.equal(BigInt(instance.exports[`tz_${name}`](BigInt(size))), reference(name, size), `${name}(${size}): checksum`);
    } catch (error) {
      if (!(error instanceof WebAssembly.RuntimeError)) throw error;
      trapped = true;
    }
    rows.push({ workload: name, size, trapped, memory_bytes: instance.exports.memory.buffer.byteLength, heap_bytes: instance.exports.memory.buffer.byteLength - initial });
  }
  return { initial_bytes: initial, rows };
}

const fixed = (value) => (value >= 100 ? value.toFixed(1) : value >= 1 ? value.toFixed(3) : value >= 0.001 ? value.toFixed(5) : value.toExponential(2));
const mebibytes = (bytes) => (bytes / 1048576).toFixed(2);

export function runListMemory(options) {
  const { compiler, baseline, quick, samples, only, metrics } = options;
  const clang = process.env.TSUZURI_CLANG ?? "clang";
  const names = only.length ? only : workloads;
  const variants = [["default", compiler], ...(baseline ? [["before", baseline]] : [])];
  const directory = mkdtempSync(join(tmpdir(), "tsuzuri-list-memory-"));
  try {
    const tracker = join(directory, "tracked_alloc.o");
    run(clang, ["-std=c11", "-O2", "-Wall", "-Wextra", "-Werror", "-c", join(root, "benchmarks/tracked_alloc.c"), "-o", tracker]);
    const builds = Object.fromEntries(variants.map(([variant, path]) => [variant, buildNative(path, variant, directory, clang, tracker)]));
    const grids = (name) => ({
      counted: quick ? sizes.quick : name === "index_walk" ? sizes.indexAllocations : sizes.allocations,
      timed: quick ? sizes.quick : name === "index_walk" ? sizes.indexMeasured : sizes.measured,
    });
    const startLoad = loadavg();
    const results = [];
    for (const name of names) {
      const { counted, timed } = grids(name);
      for (const size of new Set([...counted, ...timed])) {
        const entry = { workload: name, size, variants: {} };
        for (const [variant] of variants) entry.variants[variant] = { allocations: null, wall_ms: [], cpu_ms: [], peak_rss: [] };
        if (counted.includes(size)) for (const [variant] of variants) entry.variants[variant].allocations = allocations(builds[variant].tracked, name, size);
        if (timed.includes(size)) {
          for (let round = quick ? 0 : -1; round < samples; ++round) {
            // The order alternates from sample to sample so that drift favours neither build; the first round is a warm-up.
            const order = round % 2 === 0 ? variants : [...variants].reverse();
            for (const [variant] of order) {
              const measured = sample(builds[variant].timed, name, size, quick);
              if (round < 0) continue;
              entry.variants[variant].wall_ms.push(measured.wall_ms);
              entry.variants[variant].cpu_ms.push(measured.cpu_ms);
              entry.variants[variant].peak_rss.push(measured.peak_rss);
            }
          }
        }
        results.push(entry);
      }
    }
    const wasm = options.wasm ? Object.fromEntries(variants.map(([variant, path]) => [variant, wasmHeap(path, variant, directory, { names, sizes: sizes.wasm }, quick)])) : null;
    const endLoad = loadavg();

    if (metrics) {
      const context = captureRun(metrics, { compiler, mode: quick ? "quick" : "benchmark" });
      const records = [];
      for (const { workload, size, variants: measured } of results) {
        for (const [variant] of variants) {
          const data = measured[variant];
          const fields = { suite: "list-memory", workload, size, language: "tsuzuri", variant, opt: "O3" };
          if (data.allocations) {
            records.push(makeRecord(context, { ...fields, metric: "alloc_calls", samples: [data.allocations.calls] }));
            records.push(makeRecord(context, { ...fields, metric: "alloc_bytes", samples: [data.allocations.bytes] }));
          }
          if (data.wall_ms.length) {
            records.push(makeRecord(context, { ...fields, metric: "wall_time", samples: data.wall_ms }));
            records.push(makeRecord(context, { ...fields, metric: "cpu_time", samples: data.cpu_ms }));
            records.push(makeRecord(context, { ...fields, metric: "peak_rss", samples: data.peak_rss }));
          }
        }
      }
      writeRecords(metrics, "list-memory", records);
    }

    const lines = [["workload", "n", ...variants.flatMap(([variant]) => [`${variant} calls / bytes`, `${variant} wall ms (median)`, `${variant} RSS MB`])]];
    for (const { workload, size, variants: measured } of results) {
      lines.push([workload, String(size), ...variants.flatMap(([variant]) => {
        const data = measured[variant];
        return [data.allocations ? `${data.allocations.calls} / ${data.allocations.bytes}` : "-",
          data.wall_ms.length ? fixed(median(data.wall_ms)) : "-", data.peak_rss.length ? mebibytes(median(data.peak_rss)) : "-"];
      })]);
    }
    console.log(lines.map((line) => line.join("\t")).join("\n"));
    if (wasm) {
      console.log("\nwasm32 heap high-water (bytes above the empty module; trap = out of memory)");
      for (const [variant, { initial_bytes, rows }] of Object.entries(wasm)) {
        console.log(`${variant} (empty module ${initial_bytes} bytes):`);
        for (const row of rows) console.log(`  ${row.workload}(${row.size}) ${row.trapped ? "trap" : row.heap_bytes}`);
      }
    }
    console.log(`\nsamples: ${quick ? "quick" : samples} per size (${quick ? "no warm-up" : "after one warm-up"}), alternating builds; load average ${startLoad.map((x) => x.toFixed(2)).join(" ")} -> ${endLoad.map((x) => x.toFixed(2)).join(" ")}`);

    if (options.json) {
      writeFileSync(options.json, `${JSON.stringify({
        schema: 1, date: new Date().toISOString(), quick, samples, load_average: [startLoad, endLoad],
        host: { os: `${platform()} ${release()}`, arch: arch(), cpu: cpus()[0]?.model, cores: cpus().length, node: process.version },
        compilers: Object.fromEntries(variants.map(([variant, path]) => [variant, sha256(path)])),
        results, wasm,
      }, null, 2)}\n`);
    }
    return { results, wasm };
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) runListMemory(parseArguments(process.argv.slice(2)));
