import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { arch, cpus, platform, release, tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const usage = "Usage: node benchmarks/run-control.mjs [compiler] [--quick] [--scale number] [--cpu generic|native] [--baseline compiler] [--artifacts directory]";
const take = (flag, fallback) => {
  const index = args.indexOf(flag);
  if (index < 0) return fallback;
  const value = args[index + 1];
  if (value === undefined || value.startsWith("--")) throw new Error(usage);
  args.splice(index, 2);
  return value;
};
const cpu = take("--cpu", "generic");
const scale = Number(take("--scale", "1"));
const artifacts = take("--artifacts", null);
const baselineArgument = take("--baseline", null);
const baseline = baselineArgument && resolve(baselineArgument);
const quick = args.includes("--quick");
const positional = args.filter((arg) => arg !== "--quick");
if (!["generic", "native"].includes(cpu) || !Number.isFinite(scale) || scale <= 0 || scale > 10
  || (quick && scale !== 1) || positional.length > 1
    || positional.some((arg) => arg.startsWith("-")) || args.filter((arg) => arg === "--quick").length > 1) {
  throw new Error(usage);
}
const compiler = resolve(positional[0] ?? join(root, "target/release/tsuzuri"));
const clang = process.env.TSUZURI_CLANG ?? "clang";
const rustc = process.env.TSUZURI_RUSTC ?? "rustc";
const run = (program, arguments_) => execFileSync(program, arguments_, {
  cwd: root, encoding: "utf8", timeout: 180_000, stdio: ["ignore", "pipe", "pipe"],
});
const median = (values) => {
  const sorted = [...values].sort((a, b) => a - b);
  return (sorted[Math.floor((sorted.length - 1) / 2)] + sorted[Math.floor(sorted.length / 2)]) / 2;
};
const wrap = (n) => BigInt.asUintN(64, n);
const hex = (n) => wrap(n).toString(16).padStart(16, "0");
const factors = [17n, 3n, 29n, 7n, 61n, 11n, 83n, 5n, 47n, 19n, 101n, 31n, 53n, 23n, 97n, 13n];
function reference(name, size, seed) {
  if (["while_mix", "for_mix", "tail_mix", "tail_if_mix", "tail_builtin_mix", "record_pipeline"].includes(name)) {
    for (let n = BigInt(size); n > 0n; n--) seed = wrap((seed ^ (seed >> 13n)) * 6364136223846793005n + n + 1442695040888963407n);
    return seed;
  }
  if (name === "closure_capture") {
    const salt = (seed & 1n) === 0n ? seed : seed ^ 71n;
    for (let index = 0n; index < size; index++) seed = wrap((seed ^ (seed >> 13n)) * 6364136223846793005n + salt + 1442695040888963407n);
    return seed;
  }
  if (name === "integer128_mix") {
    let state = (seed << 64n) | 1442695040888963407n;
    for (let remaining = size; remaining > 0n; remaining--) {
      state = BigInt.asUintN(128, (state ^ (state >> 43n)) * 6364136223846793005n + remaining);
    }
    return wrap(state ^ (state >> 64n));
  }
  if (name.startsWith("float")) {
    const round = name === "float32_mix" ? Math.fround : (value) => value;
    let state = round(Number(seed & 65535n) / 16 + 1);
    const factor = round(name === "float32_mix" ? 1.000001 : 1.0000001);
    for (let index = 0; index < Number(size); index++) state = round(round(state * factor) + (index & 7) / 16);
    return state >= 2 ** 64 ? (1n << 64n) - 1n : BigInt(Math.trunc(state));
  }
  const collection = ["array_sum", "array_copy", "list_sum"].includes(name);
  let total = collection ? 0n : seed;
  for (let i = 0n; i < size; i++) total += collection
    ? (i ^ seed) * 6364136223846793005n + 1442695040888963407n
    : factors[Number(wrap(seed + i) & 15n)] * (i + 1n);
  return wrap(name === "array_copy" ? total * 2n : total);
}
const flags = ["-O3", "-fPIC", "-fno-fast-math", "-ffp-contract=off"];
const rustFlags = ["--edition=2021", "--crate-type=lib", "--crate-name=control_reference",
  "-C", "opt-level=3", "-C", "panic=abort", "-C", "force-unwind-tables=no",
  "-C", "relocation-model=pic", "-C", "codegen-units=1"];
if (cpu === "native") {
  if (["x64", "ia32"].includes(arch())) flags.push("-march=native");
  else if (["arm64", "arm"].includes(arch())) flags.push("-mcpu=native");
  else throw new Error(`Native tuning unsupported on ${arch()}`);
  rustFlags.push("-C", "target-cpu=native");
}
const tsuzuriFlags = ["-O3", "--cpu", cpu];
const temporary = mkdtempSync(join(tmpdir(), "tsuzuri-control-benchmark-"));

try {
  const fixture = "benchmarks/control";
  const generated = [];
  for (const [emit, extension] of [["object", "o"], ["header", "h"], ["llvm", "ll"]]) {
    const path = join(temporary, `control.${extension}`);
    run(compiler, ["build", fixture, "--emit", emit, ...(emit === "object" ? tsuzuriFlags : []), "-o", path]);
    generated.push(`control.${extension}`);
  }
  run(clang, [...flags, "-Wno-override-module", "-S", "-emit-llvm", join(temporary, "control.ll"), "-o", join(temporary, "control.optimized.ll")]);
  run(clang, [...flags, "-Wno-override-module", "-S", join(temporary, "control.ll"), "-o", join(temporary, "control.s")]);
  generated.push("control.optimized.ll", "control.s");
  for (const [name, mode, standard] of [["c", "c", "c11"], ["cpp", "c++", "c++20"]]) {
    const common = ["-x", mode, `-std=${standard}`, ...flags, "-Wall", "-Wextra", "-Werror", `-DPREFIX=${name}_`, `${fixture}/reference.c`];
    run(clang, [...common, "-c", "-o", join(temporary, `${name}.o`)]);
    run(clang, [...common, "-S", "-o", join(temporary, `${name}.s`)]);
    run(clang, [...common, "-S", "-emit-llvm", "-o", join(temporary, `${name}.ll`)]);
    generated.push(`${name}.o`, `${name}.s`, `${name}.ll`);
  }
  run(rustc, [...rustFlags, "--emit=obj,asm,llvm-ir", `${fixture}/reference.rs`, "--out-dir", temporary]);
  generated.push("control_reference.o", "control_reference.s", "control_reference.ll");
  if (baseline) {
    const ir = join(temporary, "before.ll");
    const header = join(temporary, "before.h");
    run(baseline, ["build", fixture, "--emit", "llvm", "-o", ir]);
    run(baseline, ["build", fixture, "--emit", "header", "-o", header]);
    writeFileSync(ir, readFileSync(ir, "utf8").replaceAll("@tz_", "@before_tz_"));
    writeFileSync(header, readFileSync(header, "utf8").replaceAll("tz_", "before_tz_"));
    run(clang, [...flags, "-Wno-override-module", "-c", ir, "-o", join(temporary, "before.o")]);
    run(clang, [...flags, "-Wno-override-module", "-S", ir, "-o", join(temporary, "before.s")]);
    run(clang, [...flags, "-Wno-override-module", "-S", "-emit-llvm", ir, "-o", join(temporary, "before.optimized.ll")]);
    generated.push("before.ll", "before.h", "before.o", "before.s", "before.optimized.ll");
  }
  const native = join(temporary, "benchmark");
  run(clang, ["-std=c11", ...flags, "-Wall", "-Wextra", "-Werror", ...(baseline ? ["-DBASELINE"] : []),
    `${fixture}/host.c`, "-I", temporary,
    ...["control.o", "c.o", "cpp.o", "control_reference.o", ...(baseline ? ["before.o"] : [])].map((name) => join(temporary, name)), "-lm", "-o", native]);
  const result = JSON.parse(run(native, quick ? ["--quick"] : ["--scale", String(scale)]));
  const variants = ["c", "cpp", "rust", "tsuzuri", ...(baseline ? ["before"] : [])];
  assert.equal(result.samples, variants.length * (quick ? 1 : 3));
  assert.deepEqual(result.workloads.map((work) => work.name), ["while_mix", "for_mix", "tail_mix", "tail_if_mix", "tail_builtin_mix", "match_dispatch", "array_sum",
    "array_copy", "list_sum", "closure_capture", "record_pipeline", "integer128_mix", "float32_mix", "float64_mix"]);
  for (const work of result.workloads) {
    assert.equal(work.checks.length, 25);
    for (const check of work.checks) {
      assert.equal(check.checksum, hex(reference(work.name, BigInt(check.size), BigInt(check.seed))), work.name);
    }
    assert.equal(work.raw.length, result.samples);
    for (const variant of variants) {
      const times = work.raw.map((sample) => sample[`${variant}_ms`]);
      assert.ok(times.every((time) => Number.isFinite(time) && (quick ? time >= 0 : time > 0)));
      work[`${variant}_median_ms`] = median(times);
      const wallTimes = work.raw.map((sample) => sample[`${variant}_wall_ms`]);
      assert.ok(wallTimes.every((time) => Number.isFinite(time) && time >= 0));
      work[`${variant}_wall_median_ms`] = median(wallTimes);
    }
    for (const variant of ["c", "cpp", "rust"]) {
      work[`tsuzuri_over_${variant}`] = work[`${variant}_median_ms`] > 0
        ? work.tsuzuri_median_ms / work[`${variant}_median_ms`] : null;
    }
    if (baseline) {
      work.speedup = work.tsuzuri_median_ms > 0 ? work.before_median_ms / work.tsuzuri_median_ms : null;
    }
  }
  const optimized = readFileSync(join(temporary, "control.optimized.ll"), "utf8");
  const inspection = {
    source_switch_count: (readFileSync(join(temporary, "control.ll"), "utf8").match(/\bswitch /g) ?? []).length,
    optimized_vector_instructions: (optimized.match(/(?:add|mul|load|store|xor) <\d+ x i\d+>/g) ?? []).length,
    array_allocator_calls: (optimized.match(/call .*@malloc\(/g) ?? []).length,
  };
  if (artifacts) {
    mkdirSync(resolve(artifacts), { recursive: true });
    for (const name of generated) copyFileSync(join(temporary, name), join(resolve(artifacts), name));
  }
  console.log(JSON.stringify({
    environment: {
      tsuzuri: run(compiler, ["--version"]).trim(),
      compiler_sha256: createHash("sha256").update(readFileSync(compiler)).digest("hex"),
      baseline: baseline ?? null,
      baseline_sha256: baseline ? createHash("sha256").update(readFileSync(baseline)).digest("hex") : null,
      clang: run(clang, ["--version"]).trim(),
      rustc: run(rustc, ["-vV"]).trim(),
      node: process.version, platform: platform(), os_release: release(),
      architecture: arch(), cpu: cpus()[0]?.model, logical_cpus: cpus().length,
    },
    mode: quick ? "correctness-smoke" : "benchmark",
    cpu, scale, wall_clock: "CLOCK_MONOTONIC", c_cpp_flags: flags, rust_flags: rustFlags, tsuzuri_flags: tsuzuriFlags,
    inspection, ...result,
    note: "Native CPU time; two warmups per variant; rotating/reversed order; no LTO. Array workloads include allocation, initialization, traversal and free. C/C++ share a C-compatible baseline; Rust uses the same system allocation with safe slice traversal. Ratios below 1 favor Tsuzuri; microbenchmarks are not a general language ranking.",
  }, null, 2));
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
