// The run time of the code of the two code generators of `tsuzuri build` (PB05): the 16 kernels of benchmarks/control built with LLVM
// (Clang at -O0), with `--backend fast` (-O0), and with LLVM at -O3 for reference. The checksum of every kernel must be the same in
// the three builds. Native builds are objects linked with benchmarks/control/kernels.c; wasm32 builds are modules called from this
// process (V8 compiles each function with Liftoff first and tiers hot ones up, so the first calls of a round can be slower).
//
//   node benchmarks/run-backend-kernels.mjs [COMPILER] --out DIR [--target native|wasm32] [--repeats 7] [--rounds 3]
//
// COMPILER is a tsuzuri built with `cargo build --release --features fast-backend` (default target/release/tsuzuri). Each round runs
// every build once, in a rotated order, and a kernel's time is the median of the medians of its rounds. DIR/kernels.json keeps every sample.
import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { cpus, loadavg } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const control = join(root, "benchmarks/control");
const usage = "Usage: node benchmarks/run-backend-kernels.mjs [compiler] --out DIR [--target native|wasm32] [--repeats N] [--rounds N]";
const args = process.argv.slice(2);
const take = (flag, fallback) => {
  const index = args.indexOf(flag);
  if (index < 0) return fallback;
  const value = args[index + 1];
  if (value === undefined || value.startsWith("--")) throw new Error(usage);
  args.splice(index, 2);
  return value;
};
const out = take("--out", null);
const target = take("--target", "native");
const repeats = Number(take("--repeats", "7"));
const rounds = Number(take("--rounds", "3"));
if (!out || !["native", "wasm32"].includes(target) || !Number.isInteger(repeats) || repeats < 1 || !Number.isInteger(rounds) || rounds < 1 || args.length > 1) {
  throw new Error(usage);
}
const compiler = resolve(args[0] ?? join(root, "target/release/tsuzuri"));
const directory = resolve(out);
mkdirSync(directory, { recursive: true });

const VARIANTS = [
  { name: "llvm-O0", args: ["-O0"] },
  { name: "fast-O0", args: ["-O0", "--backend", "fast"] },
  { name: "llvm-O3", args: ["-O3"] },
];
const driver = readFileSync(join(control, "kernels.c"), "utf8");
const sizes = new Map([...driver.matchAll(/\{"(\w+)", tz_\w+, (\d+)\}/g)].map((match) => [match[1], Number(match[2])]));
if (sizes.size !== 16) throw new Error(`benchmarks/control/kernels.c lists ${sizes.size} kernels, not 16`);

const median = (values) => [...values].sort((a, b) => a - b)[Math.floor(values.length / 2)];
const build = (variant) => {
  const artifact = join(directory, `control-${variant.name}.${target === "wasm32" ? "wasm" : "o"}`);
  // The arrays of the kernels need more than the 16 MiB wasm32 default.
  const emit = target === "wasm32" ? ["--target", "wasm32", "--wasm-max-memory", "268435456"] : ["--emit", "object"];
  execFileSync(compiler, ["build", control, ...emit, ...variant.args, "-o", artifact], { stdio: ["ignore", "ignore", "pipe"] });
  if (target === "wasm32") return { artifact, bytes: statSync(artifact).size };
  const executable = join(directory, `kernels-${variant.name}`);
  execFileSync(process.env.CC ?? "cc", ["-O2", join(control, "kernels.c"), artifact, "-o", executable, "-lm", "-pthread"], { stdio: ["ignore", "ignore", "pipe"] });
  return { artifact: executable, bytes: statSync(artifact).size };
};

// One round of one build: kernel name -> { checksum, median of the repeats in milliseconds }.
const runNative = (executable) =>
  Object.fromEntries(execFileSync(executable, [String(repeats)], { encoding: "utf8" }).trim().split("\n").map((line) => {
    const [name, checksum, , medianText] = line.split(" ");
    return [name, { checksum, ms: Number(medianText) }];
  }));
const runWasm = (module) => {
  const exports = new WebAssembly.Instance(module, {}).exports;
  return Object.fromEntries([...sizes].map(([name, size]) => {
    const samples = [];
    let result;
    for (let repeat = 0; repeat < repeats; repeat++) {
      const start = process.hrtime.bigint();
      result = exports[`tz_${name}`](BigInt(size), 42n);
      samples.push(Number(process.hrtime.bigint() - start) / 1e6);
    }
    return [name, { checksum: BigInt.asUintN(64, result).toString(16).padStart(16, "0"), ms: median(samples) }];
  }));
};

const loadBefore = loadavg();
const builds = VARIANTS.map((variant) => ({ ...variant, ...build(variant) }));
for (const built of builds) built.module = target === "wasm32" ? new WebAssembly.Module(readFileSync(built.artifact)) : null;
const times = new Map(builds.map((built) => [built.name, new Map([...sizes.keys()].map((name) => [name, []]))]));
const checksums = new Map([...sizes.keys()].map((name) => [name, new Set()]));
for (let round = 0; round < rounds; round++) {
  for (let index = 0; index < builds.length; index++) {
    const built = builds[(index + round) % builds.length];
    const result = target === "wasm32" ? runWasm(built.module) : runNative(built.artifact);
    for (const [name, { checksum, ms }] of Object.entries(result)) {
      times.get(built.name).get(name).push(ms);
      checksums.get(name).add(checksum);
    }
  }
}
const different = [...checksums].filter(([, set]) => set.size !== 1).map(([name]) => name);
if (different.length > 0) throw new Error(`the builds disagree on the checksum of ${different.join(", ")}`);

const row = (name) => builds.map((built) => median(times.get(built.name).get(name)));
const lines = [`kernel`.padEnd(18) + builds.map((built) => built.name.padStart(10)).join("") + "  fast/llvm-O0"];
const totals = builds.map(() => 0);
for (const name of sizes.keys()) {
  const medians = row(name);
  medians.forEach((value, index) => { totals[index] += value; });
  lines.push(name.padEnd(18) + medians.map((value) => value.toFixed(2).padStart(10)).join("") + `  ${(medians[1] / medians[0]).toFixed(2)}`);
}
lines.push("total".padEnd(18) + totals.map((value) => value.toFixed(1).padStart(10)).join("") + `  ${(totals[1] / totals[0]).toFixed(2)}`);
lines.push(`${target === "wasm32" ? "module" : "object"} bytes: ${builds.map((built) => `${built.name} ${built.bytes}`).join(", ")}`);
lines.push(`checksums: the same in all ${builds.length} builds for all ${sizes.size} kernels`);
console.log(lines.join("\n"));
writeFileSync(join(directory, "kernels.json"), `${JSON.stringify({
  compiler, target, repeats, rounds, cpus: cpus().length, model: cpus()[0].model, load_before: loadBefore, load_after: loadavg(), date: new Date().toISOString(),
  builds: builds.map(({ name, bytes }) => ({ name, bytes })),
  kernels: Object.fromEntries([...sizes].map(([name, size]) => [name, { size, ms: Object.fromEntries(builds.map((built) => [built.name, times.get(built.name).get(name)])) }])),
}, null, 2)}\n`);
