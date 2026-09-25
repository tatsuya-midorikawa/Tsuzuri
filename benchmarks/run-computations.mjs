import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { arch, cpus, platform, release, tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const usage = "Usage: node benchmarks/run-computations.mjs [compiler] [--quick] [--cpu generic|native] [--baseline compiler] [--artifacts directory]";
const options = {};
const positional = [];
let quick = false;
for (let index = 0; index < args.length; ++index) {
  const arg = args[index];
  if (arg === "--quick" && !quick) quick = true;
  else if (["--cpu", "--baseline", "--artifacts"].includes(arg) && options[arg] === undefined) {
    const value = args[++index];
    if (value === undefined || value.startsWith("--")) throw new Error(usage);
    options[arg] = value;
  } else if (!arg.startsWith("-")) positional.push(arg);
  else throw new Error(usage);
}
if (positional.length > 1) throw new Error(usage);
const compiler = resolve(positional[0] ?? join(root, "target/release/tsuzuri"));
const baseline = options["--baseline"] && resolve(options["--baseline"]);
const cpu = options["--cpu"] ?? "generic";
if (!["generic", "native"].includes(cpu)) throw new Error(usage);
const clang = process.env.TSUZURI_CLANG ?? "clang";
const run = (program, arguments_) => execFileSync(program, arguments_, {
  cwd: root, encoding: "utf8", timeout: 300_000, stdio: ["ignore", "pipe", "pipe"],
});
const wrap = (value) => BigInt.asUintN(64, value);
const hex = (value) => wrap(value).toString(16).padStart(16, "0");
const mix = (value, salt) => wrap((value ^ (value >> 13n)) * 6364136223846793005n + salt);
function reference(name, count, seed) {
  let state = wrap(seed);
  if (name.startsWith("array_")) {
    let total = 0n;
    for (let i = 0; i < count; ++i) {
      const value = mix(BigInt(i), state);
      total += (value ^ (value >> 17n)) * (state | 1n);
    }
    return wrap(total);
  }
  const values = name === "owned_capture"
    ? Array.from({ length: 256 }, (_, i) => mix(BigInt(i), state)) : [];
  for (let remaining = count; remaining > 0; --remaining) {
    const salt = 1442695040888963407n + BigInt(remaining);
    if (name === "bind") state = mix(state, salt);
    else if (name === "delayed") {
      state = wrap(mix(state, salt) + mix(state ^ 71n, salt) + mix(state ^ 113n, salt));
    } else if (name === "checked" || name === "std_result") {
      const first = state ^ salt;
      state = (state & 7n) === 0n ? first
        : (first & 3n) === 0n ? mix(first, salt) : mix(first, salt) ^ salt;
    } else if (name === "std_option") {
      state = (state & 7n) === 0n ? state ^ salt : mix(state ^ salt, salt);
    } else if (name === "std_option_owned") {
      state = mix(state, salt) ^ ((state & 7n) === 0n ? 0n : 12n);
    } else if (name === "owned_capture") state = mix(values[Number(state & 255n)], state);
    else throw new Error(`Unknown workload ${name}`);
  }
  return wrap(state);
}
const median = (values) => {
  const sorted = [...values].sort((a, b) => a - b);
  return (sorted[Math.floor((sorted.length - 1) / 2)] + sorted[Math.floor(sorted.length / 2)]) / 2;
};
const stats = (values) => ({ median_ms: median(values), min_ms: Math.min(...values), max_ms: Math.max(...values) });
const flags = ["-O3", "-fPIC", "-fno-fast-math", "-ffp-contract=off", "-Wno-override-module"];
if (cpu === "native") {
  if (["x64", "ia32"].includes(arch())) flags.push("-march=native");
  else if (["arm64", "arm"].includes(arch())) flags.push("-mcpu=native");
  else throw new Error(`Unsupported architecture for --cpu native: ${arch()}`);
}
const temporary = mkdtempSync(join(tmpdir(), "tsuzuri-computation-benchmark-"));
const source = join(root, "benchmarks/computations/Main.tz");

try {
  let baselineSource = source;
  let baselineRecSyntax = baseline ? "explicit-rec" : null;
  if (baseline) {
    const probeDirectory = join(temporary, "rec-probe");
    mkdirSync(probeDirectory);
    const probe = join(probeDirectory, "Main.tz");
    writeFileSync(probe, "def rec identity :: i64 -> i64\nfn rec identity n = n\n");
    const supported = spawnSync(baseline, ["check", probe, "--json"], {
      cwd: root, encoding: "utf8", timeout: 30_000,
    });
    if (supported.error) throw supported.error;
    if (supported.status !== 0) {
      if (supported.status !== 1 || JSON.parse(supported.stderr).code !== "E0002") {
        throw new Error(`Baseline capability check failed:\n${supported.stderr}`);
      }
      const legacy = join(temporary, "before-source");
      mkdirSync(legacy);
      for (const name of readdirSync(dirname(source)).filter((name) => /\.(tz|tt|tc)$/.test(name))) {
        const original = readFileSync(join(dirname(source), name), "utf8");
        writeFileSync(join(legacy, name), original.replace(/^(\s*(?:export\s+)?(?:def|fn))\s+rec\s+/gm, "$1 "));
      }
      baselineSource = join(legacy, "Main.tz");
      baselineRecSyntax = "legacy-rec-erased";
    }
  }
  const objects = [], trackedObjects = [], wasmModules = [];
  for (const [label, binary] of [["current", compiler], ...(baseline ? [["before", baseline]] : [])]) {
    const input = label === "before" ? baselineSource : source;
    const ir = join(temporary, `${label}.ll`);
    run(binary, ["build", input, "--emit", "llvm", "-o", ir]);
    const header = join(temporary, label === "current" ? "kernels.h" : "before.h");
    run(binary, ["build", input, "--emit", "header", "-o", header]);
    if (label === "before") {
      writeFileSync(ir, readFileSync(ir, "utf8").replaceAll("@tz_ce_", "@before_tz_ce_").replaceAll("@tz_direct_", "@before_tz_direct_"));
      writeFileSync(header, readFileSync(header, "utf8").replaceAll("tz_ce_", "before_tz_ce_").replaceAll("tz_direct_", "before_tz_direct_"));
    }
    const object = join(temporary, `${label}.o`);
    run(clang, [...flags, "-c", ir, "-o", object]);
    objects.push(object);
    run(clang, [...flags, "-S", "-emit-llvm", ir, "-o", join(temporary, `${label}.opt.ll`)]);
    run(clang, [...flags, "-S", ir, "-o", join(temporary, `${label}.s`)]);
    const tracked = join(temporary, `${label}.tracked.ll`);
    // Count surviving allocations, not allocations LLVM already removed. Drop inferred effects
    // before adding instrumentation, and do not optimize the instrumented IR a second time.
    writeFileSync(tracked, readFileSync(join(temporary, `${label}.opt.ll`), "utf8")
      .replaceAll("@malloc", "@tracked_alloc").replaceAll("@free", "@tracked_free")
      .replace(/^attributes #\d+ = .*\n/gm, "").replace(/ #\d+\b/g, ""));
    const trackedObject = join(temporary, `${label}.tracked.o`);
    run(clang, ["-O0", "-fPIC", "-Wno-override-module", "-c", tracked, "-o", trackedObject]);
    trackedObjects.push(trackedObject);
    const wasm = join(temporary, `${label}.wasm`);
    run(binary, ["build", input, "--target", "wasm32", "-O3", "-o", wasm]);
    const module = await WebAssembly.compile(readFileSync(wasm));
    assert.deepEqual(WebAssembly.Module.imports(module), []);
    wasmModules.push([label, module, readFileSync(wasm).byteLength]);
  }
  const host = join(root, "benchmarks/computations/native.cpp");
  const hostFlags = ["--driver-mode=g++", ...flags, "-std=c++20", "-Wall", "-Wextra", "-Werror", "-I", temporary,
    ...(baseline ? ["-DBASELINE"] : [])];
  const native = join(temporary, "native");
  run(clang, [...hostFlags, host, ...objects, "-lm", "-o", native]);
  run(clang, [...hostFlags, "-S", host, "-o", join(temporary, "cpp.s")]);
  run(clang, [...hostFlags, "-S", "-emit-llvm", host, "-o", join(temporary, "cpp.opt.ll")]);
  const result = JSON.parse(run(native, quick ? ["--quick"] : []));
  const tracked = join(temporary, "tracked");
  run(clang, [...hostFlags, "-DTRACKING", host, ...trackedObjects, "-lm", "-o", tracked]);
  const allocations = JSON.parse(run(tracked, ["--quick"]));
  for (const [index, workload] of result.workloads.entries()) {
    assert.equal(workload.checks.length, 25);
    for (const check of workload.checks) {
      assert.equal(check.checksum, hex(reference(workload.name, check.size, BigInt(check.seed))));
    }
    workload.allocations_at_size_64 = allocations.workloads[index].allocations;
    workload.native = {};
    for (const label of ["cpp", "direct", "computation", ...(baseline ? ["before"] : [])]) {
      const times = workload.raw.map((sample) => sample[`${label}_ms`]);
      assert.ok(times.every((time) => Number.isFinite(time) && time >= 0));
      if (!quick) assert.ok(times.every((time) => time > 0), "Clock resolution is insufficient");
      workload.native[label] = stats(times);
    }
    const native = workload.native;
    native.computation_over_cpp = native.cpp.median_ms > 0 ? native.computation.median_ms / native.cpp.median_ms : null;
    native.computation_over_direct = native.direct.median_ms > 0 ? native.computation.median_ms / native.direct.median_ms : null;
    if (baseline) native.speedup = native.before.median_ms / native.computation.median_ms;
  }

  const wasmApis = [];
  for (const [label, module, bytes] of wasmModules) {
    const instance = await WebAssembly.instantiate(module);
    wasmApis.push([label, instance.exports]);
    for (const workload of result.workloads) {
      for (const check of workload.checks) {
        for (const prefix of ["ce", "direct"]) {
          const value = instance.exports[`tz_${prefix}_${workload.name}`](BigInt(check.size), BigInt(check.seed));
          assert.equal(hex(value), check.checksum, `${label} WASM ${workload.name}`);
        }
      }
      workload.wasm ??= { size: quick ? 128 : Math.min(workload.size, 1024), variants: {} };
      workload.wasm.variants[label] = { bytes };
    }
  }
  for (const work of result.workloads) {
    const count = BigInt(work.wasm.size);
    const variants = [["direct", wasmApis[0][1][`tz_direct_${work.name}`]],
      ...wasmApis.map(([label, api]) => [label, api[`tz_ce_${work.name}`]])];
    const repeats = new Map();
    const raw = Object.fromEntries(variants.map(([label]) => [label, []]));
    for (const [label, kernel] of variants) {
      const expected = hex(reference(work.name, work.wasm.size, 40n));
      for (let warm = 0; warm < 3; ++warm) assert.equal(hex(kernel(count, 40n)), expected);
      const start = performance.now();
      const result = kernel(count, 40n);
      const duration = performance.now() - start;
      assert.equal(hex(result), expected);
      repeats.set(label, quick ? 1 : Math.min(10000, Math.ceil(20 / Math.max(0.01, duration))));
    }
    for (let sample = 0; sample < result.samples; ++sample) {
      const seed = BigInt(42 + sample);
      const expected = BigInt.asIntN(64, reference(work.name, work.wasm.size, seed));
      for (let index = 0; index < variants.length; ++index) {
        const [label, kernel] = variants[(index + sample) % variants.length];
        const n = repeats.get(label);
        const start = performance.now();
        for (let repeat = 0; repeat < n; ++repeat) {
          if (kernel(count, seed) !== expected) throw new Error(`${label} WASM ${work.name}: checksum mismatch`);
        }
        const duration = (performance.now() - start) / n;
        raw[label].push(duration);
      }
    }
    work.wasm.raw_ms = raw;
    work.wasm.repeats = Object.fromEntries(repeats);
    work.wasm.times = Object.fromEntries(Object.entries(raw).map(([label, times]) => [label, stats(times)]));
    work.wasm.computation_over_direct = work.wasm.times.current.median_ms / work.wasm.times.direct.median_ms;
    if (baseline) work.wasm.speedup = work.wasm.times.before.median_ms / work.wasm.times.current.median_ms;
  }
  if (options["--artifacts"]) {
    const destination = resolve(options["--artifacts"]);
    mkdirSync(destination, { recursive: true });
    for (const name of readdirSync(temporary)) cpSync(join(temporary, name), join(destination, name), { recursive: true });
  }
  console.log(JSON.stringify({
    environment: {
      tsuzuri: run(compiler, ["--version"]).trim(),
      compiler,
      compiler_sha256: createHash("sha256").update(readFileSync(compiler)).digest("hex"),
      baseline: baseline ?? null,
      baseline_rec_syntax: baselineRecSyntax,
      baseline_sha256: baseline ? createHash("sha256").update(readFileSync(baseline)).digest("hex") : null,
      clang: run(clang, ["--version"]).split(/\r?\n/)[0],
      node: process.version, platform: platform(), os_release: release(), architecture: arch(),
      cpu: cpus()[0]?.model, logical_cpus: cpus().length,
    },
    mode: quick ? "correctness-smoke" : "benchmark",
    native_flags: flags,
    wasm_flags: ["-O3"],
    ...result,
    note: "Matched outputs, wrapping integers, no LTO/fast-math; rotating measurement order with per-variant repeat calibration. Native CPU time and warmed WASM wall time are separate. Allocations are measured in a separate instrumented executable, not in timed runs. Reference implementations are optimized candidates, not a proof of global optimality. Quick-mode timings are not meaningful.",
  }, null, 2));
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
