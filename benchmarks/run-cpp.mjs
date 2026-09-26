import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync, mkdtempSync, readdirSync, rmSync } from "node:fs";
import { arch, cpus, platform, release, tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
if (Number(process.versions.node.split(".")[0]) < 24) {
  throw new Error("This benchmark requires Node.js 24+; older V8 versions can crash while optimizing BigInt reference checks.");
}
const args = process.argv.slice(2);
const usage = "Usage: node benchmarks/run-cpp.mjs [compiler] [--quick] [--scale number] [--cpu generic|native] [--artifacts directory]";
const take = (flag, fallback) => {
  const index = args.indexOf(flag);
  if (index < 0) return fallback;
  const value = args[index + 1];
  if (!value || value.startsWith("--")) throw new Error(usage);
  args.splice(index, 2);
  return value;
};
const cpu = take("--cpu", "generic");
const scale = Number(take("--scale", "1"));
const artifacts = take("--artifacts", null);
if (!["generic", "native"].includes(cpu)) throw new Error(usage);
const quick = args.includes("--quick");
const positional = args.filter((arg) => arg !== "--quick");
if (!Number.isFinite(scale) || scale <= 0 || scale > 10 || (quick && scale !== 1)
  || positional.length > 1 || positional.some((arg) => arg.startsWith("-"))
    || args.filter((arg) => arg === "--quick").length > 1) {
  throw new Error(usage);
}
const compiler = resolve(positional[0] ?? join(root, "target/release/tsuzuri"));
const clang = process.env.TSUZURI_CLANG ?? "clang";
const rustc = process.env.TSUZURI_RUSTC ?? process.env.RUSTC ?? "rustc";
const run = (program, arguments_) => execFileSync(program, arguments_, {
  cwd: root, encoding: "utf8", timeout: 180_000, stdio: ["ignore", "pipe", "pipe"],
});
const median = (values) => {
  const sorted = [...values].sort((a, b) => a - b);
  return (sorted[Math.floor((sorted.length - 1) / 2)] + sorted[Math.floor(sorted.length / 2)]) / 2;
};
const hex = (value) => BigInt.asUintN(64, value).toString(16).padStart(16, "0");
const makeText = (size, seed) => {
  if (size === 0) return "";
  let text = (seed & 1n) === 0n ? "Az09-_ \n" : "A\0\u03A9\uD83D\uDE00\u4E2Dz\n";
  while (text.length < size) text += text;
  return text;
};
const textSum = (text) => {
  let total = 0n;
  for (let index = 0; index < text.length; ++index) total += BigInt(text.charCodeAt(index));
  return total;
};
const references = {
  integer_mix(size, seed) {
    let state = BigInt.asUintN(64, seed);
    for (let index = 0; index < size; ++index) {
      state = BigInt.asUintN(64, (state ^ (state >> 13n)) * 6364136223846793005n + 1442695040888963407n);
    }
    return state;
  },
  mandelbrot(size, seed) {
    if (size === 0) return 0n;
    const dx = 3 / size, dy = 2 / size, offset = Number(seed) * 0.000001;
    let total = 0;
    for (let y = 0; y < size; ++y) {
      for (let x = 0; x < size; ++x) {
        const cr = x * dx - 2 + offset, ci = y * dy - 1;
        let real = 0, imaginary = 0, count = 0;
        while (count < 256 && real * real + imaginary * imaginary <= 4) {
          const nextReal = real * real - imaginary * imaginary + cr;
          imaginary = 2 * real * imaginary + ci;
          real = nextReal;
          ++count;
        }
        total += count;
      }
    }
    return BigInt(total);
  },
  array_sum(size, seed) {
    let total = 0n;
    for (let index = 0; index < size; ++index) {
      total += (BigInt(index) ^ BigInt.asUintN(64, seed)) * 6364136223846793005n + 1442695040888963407n;
    }
    return total;
  },
  utf16_scan(size, seed) { return textSum(makeText(size, seed)); },
  utf16_compare(size, seed) {
    const text = makeText(size, seed);
    const left = text + ((seed & 2n) === 0n ? "a" : "b"), right = text + ((seed & 4n) === 0n ? "a" : "b");
    return left === right ? 1n : left < right ? 2n : 4n;
  },
  utf16_validate(size, seed) {
    const text = makeText(size, seed) + ((seed & 8n) === 0n ? "" : "\uD800");
    return textSum(text.toWellFormed()) + BigInt(text.isWellFormed());
  },
  utf8_roundtrip(size, seed) {
    const text = makeText(size, seed);
    return textSum(text) + BigInt(Buffer.byteLength(text, "utf8"));
  },
  format_parse(size, seed) {
    let state = BigInt.asUintN(64, seed);
    for (let index = 0; index < size; ++index) {
      state = BigInt.asUintN(64, (state ^ (state >> 13n)) * 6364136223846793005n + BigInt(index) + 1442695040888963407n);
      const text = state.toString();
      state = BigInt(text) ^ BigInt(text.length);
    }
    return state;
  },
  math_intrinsics(size, seed) {
    let state = Number((seed & 65535n) + 1n) / 16;
    for (let index = 0; index < size; ++index) {
      state = Math.sqrt(Math.abs(state) + (index & 255));
      state = Math.floor(state * 16) / 16 + Math.ceil(state) / 1024;
    }
    return BigInt(Math.trunc(state * 1048576));
  },
  task_sequence(size, seed) { return references.integer_mix(size, seed); },
  task_sequential(size, seed) {
    let total = 0n;
    for (let index = 0n; index < 16n; ++index) total += references.integer_mix(size, seed ^ index);
    return total;
  },
  task_parallel(size, seed) { return references.task_sequential(size, seed); },
};
const flags = ["-O3", "-std=c++20", "-fPIC", "-fno-fast-math", "-ffp-contract=off", "-fno-math-errno",
  "-Wall", "-Wextra", "-Werror", "-Wno-deprecated-declarations"];
if (cpu === "native") {
  if (["x64", "ia32"].includes(arch())) flags.push("-march=native");
  else if (["arm64", "arm"].includes(arch())) flags.push("-mcpu=native");
  else throw new Error(`Native CPU tuning is unsupported on ${arch()}`);
}
const tsuzuriFlags = ["-O3", "--cpu", cpu];
const rustFlags = ["--edition=2021", "--crate-type=staticlib", "-C", "opt-level=3",
  "-C", "panic=abort", "-C", "codegen-units=1", "-C", "lto=off"];
if (cpu === "native") rustFlags.push("-C", "target-cpu=native");
const temporary = mkdtempSync(join(tmpdir(), "tsuzuri-cpp-benchmark-"));

try {
  const objects = [];
  for (const [source, name] of [["benchmarks/Mix.tz", "mix"], ["benchmarks/cpp/Kernels.tz", "kernels"]]) {
    const object = join(temporary, `${name}.o`);
    run(compiler, ["build", source, "--emit", "object", ...tsuzuriFlags, "-o", object]);
    run(compiler, ["build", source, "--emit", "header", "-o", join(temporary, `${name}.h`)]);
    objects.push(object);
  }
  const native = join(temporary, platform() === "win32" ? "benchmark.exe" : "benchmark");
  const rustLibrary = join(temporary, "librustkernels.a");
  run(rustc, [...rustFlags, "benchmarks/cpp/native.rs", "-o", rustLibrary]);
  run(clang, ["--driver-mode=g++", ...flags, "benchmarks/cpp/native.cpp",
    "-I", temporary, ...objects, rustLibrary, "-o", native,
    ...(platform() === "win32" ? [] : ["-lm", "-pthread"]),
    ...(platform() === "linux" ? ["-ldl"] : [])]);
  const result = JSON.parse(run(native, quick ? ["--quick"] : ["--scale", String(scale)]));
  assert.equal(result.samples, 12);
  assert.deepEqual(result.workloads.map((workload) => workload.name), Object.keys(references));
  for (const workload of result.workloads) {
    assert.equal(workload.checks.length, 25);
    for (const check of workload.checks) {
      assert.equal(check.checksum, hex(references[workload.name](check.size, BigInt(check.seed))),
        `${workload.name}: reference mismatch at size=${check.size}, seed=${check.seed}`);
    }
    assert.equal(workload.raw.length, result.samples);
    for (const sample of workload.raw) {
      assert.ok(Number.isFinite(sample.cpp_ms) && sample.cpp_ms >= 0);
      assert.ok(Number.isFinite(sample.tsuzuri_ms) && sample.tsuzuri_ms >= 0);
      assert.ok(Number.isFinite(sample.rust_ms) && sample.rust_ms >= 0);
      for (const name of ["cpp", "rust", "tsuzuri"]) assert.ok(Number.isFinite(sample[`${name}_wall_ms`]) && sample[`${name}_wall_ms`] >= 0);
      if (!quick) assert.ok(sample.cpp_ms > 0 && sample.tsuzuri_ms > 0 && sample.rust_ms > 0,
        "Increase workload size: clock resolution is insufficient");
    }
    workload.cpp_median_ms = median(workload.raw.map((sample) => sample.cpp_ms));
    workload.tsuzuri_median_ms = median(workload.raw.map((sample) => sample.tsuzuri_ms));
    workload.rust_median_ms = median(workload.raw.map((sample) => sample.rust_ms));
    for (const name of ["cpp", "rust", "tsuzuri"]) workload[`${name}_wall_median_ms`] = median(workload.raw.map((sample) => sample[`${name}_wall_ms`]));
    workload.tsuzuri_over_cpp = workload.cpp_median_ms > 0
      ? workload.tsuzuri_median_ms / workload.cpp_median_ms : null;
    workload.tsuzuri_over_rust = workload.rust_median_ms > 0
      ? workload.tsuzuri_median_ms / workload.rust_median_ms : null;
    for (const name of ["cpp", "rust"]) workload[`tsuzuri_wall_over_${name}`] = workload[`${name}_wall_median_ms`] > 0
      ? workload.tsuzuri_wall_median_ms / workload[`${name}_wall_median_ms`] : null;
  }
  if (artifacts) {
    mkdirSync(resolve(artifacts), { recursive: true });
    for (const name of readdirSync(temporary)) copyFileSync(join(temporary, name), join(resolve(artifacts), name));
  }
  console.log(JSON.stringify({
    environment: {
      tsuzuri: run(compiler, ["--version"]).trim(),
      clang: run(clang, ["--version"]).split(/\r?\n/)[0],
      rustc: run(rustc, ["--version", "--verbose"]).trim(),
      node: process.version,
      platform: platform(),
      os_release: release(),
      architecture: arch(),
      cpu: cpus()[0]?.model,
      logical_cpus: cpus().length,
    },
    mode: quick ? "correctness-smoke" : "benchmark",
    scale, wall_clock: "std::chrono::steady_clock",
    cpp_flags: flags,
    tsuzuri_flags: tsuzuriFlags,
    rust_flags: rustFlags,
    ...result,
    note: "CPU and monotonic wall time, warmup, all six measurement orders, no LTO. Use wall time for task parallelism; CPU time sums worker time. Ratio < 1 favors Tsuzuri. Rust uses its bundled LLVM. UTF-16 workloads build content by doubling to the first power-of-two length >= size (minimum 8); language-specific allocation/encoding implementations differ. C++ codecvt is a C++20 standard baseline, not a best-available SIMD codec. Quick timings are not meaningful; microbenchmarks are not a general language ranking.",
  }, null, 2));
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
