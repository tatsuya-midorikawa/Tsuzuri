// PR07: does a wasm32 module that a profile optimized run faster than the one without it? For a suite of benchmarks/ (control or
// apps) this builds the module plain, instrumented (`--profile-generate`) and with the profile (`--profile-use`), trains the
// instrumented module in this process through the writer that the build wrote beside it, and times the plain and the PGO module
// alternately. V8 compiles a module in two tiers, so the run starts again with --no-liftoff (TurboFan only) for stable timings.
//
//   node benchmarks/run-wasm-pgo.mjs [compiler] [--suite control|apps] [--rounds N] [--scale S]
//
// The workloads, their sizes and their checks are the ones of host.c of each suite; --scale shrinks the sizes of the timing run
// (default 0.5 for apps and 0.25 for control).
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

if (!process.execArgv.includes("--no-liftoff")) {
  const again = spawnSync(process.execPath, ["--no-liftoff", ...process.argv.slice(1)], { stdio: "inherit" });
  process.exit(again.status ?? 1);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const option = (name, fallback) => {
  const at = args.indexOf(`--${name}`);
  return at >= 0 ? args[at + 1] : fallback;
};
const compiler = resolve(args[0] && !args[0].startsWith("--") ? args[0] : join(root, "target/release/tsuzuri"));
const suiteName = option("suite", "apps");
const rounds = Number(option("rounds", "15"));
// The default module heap is 16 MiB, so the apps run at half of their native sizes.
const scale = Number(option("scale", suiteName === "apps" ? "0.5" : "0.25"));
const suites = {
  control: {
    sizes: { while_mix: 8e6, for_mix: 8e6, tail_mix: 8e6, tail_if_mix: 8e6, tail_builtin_mix: 8e6, match_dispatch: 8e6, array_sum: 4e6, array_index_sum: 4e6, array_copy: 2e6,
      list_sum: 1e5, closure_capture: 1e6, closure_churn: 1e6, record_pipeline: 8e6, integer128_mix: 2e6, float32_mix: 4e6, float64_mix: 4e6 },
    quick: () => 1024,
    checkSizes: [0n, 1n, 2n, 17n, 257n],
  },
  apps: {
    sizes: { tokenize: 200000, format_join: 100000, csv_aggregate: 100000, map_count: 200000, record_sort: 200000, tree_transform: 256, binary_trees: 2000, closure_pipeline: 1000000,
      graph_bfs: 100000, spectral_norm: 500 },
    quick: (name) => ({ tree_transform: 2, binary_trees: 4, spectral_norm: 16 })[name] ?? 1024,
    checkSizes: [0n, 1n, 2n, 3n],
  },
};
const suite = suites[suiteName];
if (!suite || !Number.isInteger(rounds) || rounds < 1 || !(scale > 0 && scale <= 10)) throw new Error("Usage: node benchmarks/run-wasm-pgo.mjs [compiler] [--suite control|apps] [--rounds N] [--scale S]");
const seeds = [0n, 42n, 0xffffffffffffffffn, 0x8000000000000000n, 0x7fffffffffffffffn];

const work = mkdtempSync(join(tmpdir(), "tsuzuri-wasm-pgo-"));
function build(output, extra) {
  const result = spawnSync(compiler, ["build", join(root, "benchmarks", suiteName), "--target", "wasm32", "-O3", ...extra, "-o", output], { encoding: "utf8", cwd: root });
  if (result.status !== 0) throw new Error(result.stderr);
}
try {
  const plain = join(work, "plain.wasm");
  const instrumented = join(work, "instrumented.wasm");
  const profiled = join(work, "pgo.wasm");
  build(plain, []);
  build(instrumented, ["--profile-generate", join(work, "profile")]);
  // Training: what the --quick run of the native host does, on the instrumented module.
  const { writeProfile } = await import(pathToFileURL(`${instrumented}.profile.mjs`));
  const trained = (await WebAssembly.instantiate(readFileSync(instrumented), {})).instance;
  for (const name of Object.keys(suite.sizes)) {
    const kernel = trained.exports[`tz_${name}`];
    for (const size of suite.checkSizes) for (const seed of seeds) kernel(size, seed);
    for (let call = 0; call < 8; ++call) kernel(BigInt(suite.quick(name)), 42n + BigInt(call));
  }
  await writeProfile(trained);
  build(profiled, ["--profile-use", join(work, "profile")]);

  const variants = await Promise.all([["plain", plain], ["pgo", profiled]].map(async ([label, file]) => ({ label, bytes: statSync(file).size, exports: (await WebAssembly.instantiate(readFileSync(file), {})).instance.exports })));
  const median = (values) => [...values].sort((a, b) => a - b)[Math.floor(values.length / 2)];
  console.log(`${suiteName}: wasm32 -O3, node ${process.version}, ${rounds} rounds, scale ${scale}; module bytes plain ${variants[0].bytes}, pgo ${variants[1].bytes}, instrumented ${statSync(instrumented).size}`);
  console.log("workload".padEnd(18), "plain ms".padStart(10), "pgo ms".padStart(10), "pgo/plain".padStart(10));
  for (const [name, base] of Object.entries(suite.sizes)) {
    const size = BigInt(Math.max(1, Math.round(base * scale)));
    const [first, second] = variants.map((variant) => variant.exports[`tz_${name}`]);
    if (first(size, 42n) !== second(size, 42n)) throw new Error(`${name}: the two modules disagree`);
    const repeats = variants.map((variant) => {
      const start = performance.now();
      variant.exports[`tz_${name}`](size, 42n);
      return Math.max(1, Math.ceil(20 / Math.max(0.01, performance.now() - start)));
    });
    const times = variants.map(() => []);
    for (let round = 0; round < rounds; ++round) {
      for (const index of round % 2 ? [1, 0] : [0, 1]) {
        const kernel = variants[index].exports[`tz_${name}`];
        const start = performance.now();
        for (let call = 0; call < repeats[index]; ++call) kernel(size, 42n + BigInt(round));
        times[index].push((performance.now() - start) / repeats[index]);
      }
    }
    const [a, b] = times.map(median);
    console.log(name.padEnd(18), a.toFixed(3).padStart(10), b.toFixed(3).padStart(10), (b / a).toFixed(3).padStart(10));
  }
} finally {
  rmSync(work, { recursive: true, force: true });
}
