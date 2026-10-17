import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { copyFileSync, cpSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { arch, cpus, platform, release, tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { allocatorVariants, captureRun, inProcessRecords, instrumentAllocations, makeRecord, writeRecords } from "./metrics.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const usage = "Usage: node benchmarks/run-control.mjs [compiler] [--quick] [--scale number] [--cpu generic|native] [--allocator system|small] [--baseline compiler] [--pgo [--pgo-runs n]] [--artifacts directory] [--metrics directory]";
const take = (flag, fallback) => {
  const index = args.indexOf(flag);
  if (index < 0) return fallback;
  const value = args[index + 1];
  if (value === undefined || value.startsWith("--")) throw new Error(usage);
  args.splice(index, 2);
  return value;
};
const cpu = take("--cpu", "generic");
// PM05: the allocator of the timing object only; the tracked IR (allocation counts) always uses the system allocator.
const allocator = take("--allocator", "system");
const scale = Number(take("--scale", "1"));
const artifacts = take("--artifacts", null);
const metrics = take("--metrics", null);
const baselineArgument = take("--baseline", null);
const baseline = baselineArgument && resolve(baselineArgument);
const quick = args.includes("--quick");
// PR07: with --pgo the control object is also built with the profile of a --quick training run, and the host that links it runs
// alternately with the host of the plain object, --pgo-runs times (a --quick run once).
const pgo = args.includes("--pgo");
const pgoRunsArgument = take("--pgo-runs", null);
const pgoRuns = Number(pgoRunsArgument ?? (quick ? 1 : 5));
const positional = args.filter((arg) => arg !== "--quick" && arg !== "--pgo");
if (!["generic", "native"].includes(cpu) || !["system", "small"].includes(allocator) || !Number.isFinite(scale) || scale <= 0 || scale > 10
  || (quick && scale !== 1) || positional.length > 1
    || positional.some((arg) => arg.startsWith("-")) || args.filter((arg) => arg === "--quick").length > 1 || args.filter((arg) => arg === "--pgo").length > 1
    || !Number.isInteger(pgoRuns) || pgoRuns < 1 || pgoRuns > 21 || (!pgo && pgoRunsArgument !== null)) {
  throw new Error(usage);
}
const compiler = resolve(positional[0] ?? join(root, "target/release/tsuzuri"));
const { timing: tsuzuriVariant, allocation: allocationVariant } = allocatorVariants(allocator);
const clang = process.env.TSUZURI_CLANG ?? "clang";
const rustc = process.env.TSUZURI_RUSTC ?? "rustc";
const run = (program, arguments_, env = process.env) => execFileSync(program, arguments_, {
  cwd: root, encoding: "utf8", timeout: 180_000, stdio: ["ignore", "pipe", "pipe"],
  env: metrics ? { ...env, TSUZURI_CACHE_DIR: join(temporary, "cache") } : env,
});
const median = (values) => {
  const sorted = [...values].sort((a, b) => a - b);
  return (sorted[Math.floor((sorted.length - 1) / 2)] + sorted[Math.floor(sorted.length / 2)]) / 2;
};
// The llvm-profdata of the Clang that is measured: three directories above its resource directory, as the compiler finds it when
// nothing is beside Clang.
const findProfdata = () => process.env.TSUZURI_LLVM_PROFDATA
  ?? join(dirname(dirname(dirname(run(clang, ["-print-resource-dir"]).trim()))), "bin", "llvm-profdata");
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
  if (name === "closure_churn") {
    for (let index = 0n; index < size; index++) {
      const salt = (seed & 1n) === 0n ? seed : seed ^ 71n;
      for (let call = 0; call < 2; ++call) seed = wrap((seed ^ (seed >> 13n)) * 6364136223846793005n + salt + 1442695040888963407n);
    }
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
  const collection = ["array_sum", "array_index_sum", "array_copy", "list_sum"].includes(name);
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
const objectFlags = [...tsuzuriFlags, ...(allocator === "system" ? [] : ["--allocator", allocator])];
const context = metrics ? captureRun(metrics, { compiler, clang, rustc, mode: quick ? "quick" : "benchmark" }) : null;
const temporary = mkdtempSync(join(tmpdir(), "tsuzuri-control-benchmark-"));

try {
  const fixture = "benchmarks/control";
  const generated = [];
  for (const [emit, extension] of [["object", "o"], ["header", "h"], ["llvm", "ll"]]) {
    const path = join(temporary, `control.${extension}`);
    run(compiler, ["build", fixture, "--emit", emit, ...(emit === "object" ? objectFlags : []), "-o", path]);
    generated.push(`control.${extension}`);
  }
  run(clang, [...flags, "-Wno-override-module", "-S", "-emit-llvm", join(temporary, "control.ll"), "-o", join(temporary, "control.optimized.ll")]);
  run(clang, [...flags, "-Wno-override-module", "-S", join(temporary, "control.ll"), "-o", join(temporary, "control.s")]);
  generated.push("control.optimized.ll", "control.s");
  const references = [["c", "c", "c11"], ["cpp", "c++", "c++20"]];
  const referenceArguments = (name, mode, standard) => ["-x", mode, `-std=${standard}`, ...flags, "-Wall", "-Wextra", "-Werror", `-DPREFIX=${name}_`, `${fixture}/reference.c`];
  for (const [name, mode, standard] of references) {
    const common = referenceArguments(name, mode, standard);
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
  const hostFlags = ["-std=c11", ...flags, "-Wall", "-Wextra", "-Werror", ...(baseline ? ["-DBASELINE"] : [])];
  const hostObjects = (control, c, cpp) => [control, c, cpp, "control_reference.o", ...(baseline ? ["before.o"] : [])].map((name) => join(temporary, name));
  const linkHost = (output, control, c, cpp) => run(clang, [...hostFlags, `${fixture}/host.c`, "-I", temporary, ...hostObjects(control, c, cpp), "-lm", "-o", output]);
  const native = join(temporary, "benchmark");
  linkHost(native, "control.o", "c.o", "cpp.o");
  // PR07: the instrumented objects (Tsuzuri, C and C++ alike), linked into a host that a --quick run exercises, write the profile
  // that builds the PGO objects. Rust has no PGO build: its LLVM is not the one of Clang's profile runtime.
  const profile = join(temporary, "profile");
  const merged = join(temporary, "control.profdata");
  const hosts = [{ binary: native, runs: [] }];
  let profdata = null;
  let rawProfiles = [];
  if (pgo) {
    profdata = findProfdata();
    run(compiler, ["build", fixture, "--emit", "object", ...objectFlags, "--profile-generate", profile, "-o", join(temporary, "control.gen.o")]);
    for (const [name, mode, standard] of references) {
      run(clang, [...referenceArguments(name, mode, standard), `-fprofile-generate=${profile}`, "-c", "-o", join(temporary, `${name}.gen.o`)]);
    }
    run(clang, [...hostFlags, "-c", `${fixture}/host.c`, "-I", temporary, "-o", join(temporary, "host.o")]);
    const train = join(temporary, "benchmark-train");
    run(clang, [...flags, join(temporary, "host.o"), ...hostObjects("control.gen.o", "c.gen.o", "cpp.gen.o"), `-fprofile-generate=${profile}`, "-lm", "-o", train]);
    const environment = { ...process.env };
    delete environment.LLVM_PROFILE_FILE;
    run(train, ["--quick"], environment);
    rawProfiles = readdirSync(profile).filter((name) => name.endsWith(".profraw")).sort();
    assert.ok(rawProfiles.length > 0, "the training run wrote no profile");
    run(profdata, ["merge", "-o", merged, ...rawProfiles.map((name) => join(profile, name))]);
    run(compiler, ["build", fixture, "--emit", "object", ...objectFlags, "--profile-use", profile, "-o", join(temporary, "control.pgo.o")]);
    for (const [name, mode, standard] of references) {
      const used = [...referenceArguments(name, mode, standard), `-fprofile-use=${merged}`];
      run(clang, [...used, "-c", "-o", join(temporary, `${name}.pgo.o`)]);
      run(clang, [...used, "-S", "-o", join(temporary, `${name}.pgo.s`)]);
      run(clang, [...used, "-S", "-emit-llvm", "-o", join(temporary, `${name}.pgo.ll`)]);
      generated.push(`${name}.gen.o`, `${name}.pgo.o`, `${name}.pgo.s`, `${name}.pgo.ll`);
    }
    hosts.push({ binary: join(temporary, "benchmark-pgo"), runs: [] });
    linkHost(hosts[1].binary, "control.pgo.o", "c.pgo.o", "cpp.pgo.o");
    generated.push("control.gen.o", "control.pgo.o", "control.profdata");
  }
  // The plain and the PGO host run alternately, so that a change of the machine's load reaches both.
  const rounds = pgo ? pgoRuns : 1;
  for (let round = 0; round < rounds; ++round) {
    for (const host of round % 2 === 0 ? hosts : [...hosts].reverse()) host.runs.push(JSON.parse(run(host.binary, quick ? ["--quick"] : ["--scale", String(scale)])));
  }
  const combine = (runs) => {
    for (const other of runs.slice(1)) {
      assert.deepEqual(other.workloads.map(({ name, size, checks }) => [name, size, checks]), runs[0].workloads.map(({ name, size, checks }) => [name, size, checks]));
    }
    return { ...runs[0], samples: runs[0].samples * runs.length, workloads: runs[0].workloads.map((work, index) => ({ ...work, raw: runs.flatMap((one) => one.workloads[index].raw) })) };
  };
  const result = combine(hosts[0].runs);
  const pgoResult = pgo ? combine(hosts[1].runs) : null;
  const variants = ["c", "cpp", "rust", "tsuzuri", ...(baseline ? ["before"] : [])];
  const analyze = (data) => {
    assert.equal(data.samples, variants.length * (quick ? 1 : 3) * rounds);
    assert.deepEqual(data.workloads.map((work) => work.name), ["while_mix", "for_mix", "tail_mix", "tail_if_mix", "tail_builtin_mix", "match_dispatch", "array_sum",
      "array_index_sum", "array_copy", "list_sum", "closure_capture", "closure_churn", "record_pipeline", "integer128_mix", "float32_mix", "float64_mix"]);
    for (const work of data.workloads) {
      assert.equal(work.checks.length, 25);
      for (const check of work.checks) {
        assert.equal(check.checksum, hex(reference(work.name, BigInt(check.size), BigInt(check.seed))), work.name);
      }
      assert.equal(work.raw.length, data.samples);
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
  };
  analyze(result);
  if (pgo) analyze(pgoResult);
  const optimized = readFileSync(join(temporary, "control.optimized.ll"), "utf8");
  const inspection = {
    source_switch_count: (readFileSync(join(temporary, "control.ll"), "utf8").match(/\bswitch /g) ?? []).length,
    optimized_vector_instructions: (optimized.match(/(?:add|mul|load|store|xor) <\d+ x i\d+>/g) ?? []).length,
    array_allocator_calls: (optimized.match(/call .*@malloc\(/g) ?? []).length,
  };
  let pgoReport = null;
  if (pgo) {
    // The IR that Tsuzuri wrote, optimized by Clang with and without the profile, shows what the profile changes: `!prof` metadata
    // on branches and entries, and no fast-math flag that the plain build lacks. The C and C++ objects are read the same way.
    const count = (text, pattern) => (text.match(pattern) ?? []).length;
    const describe = (text) => ({
      functions_with_entry_count: count(text, /^define .*!prof /gm),
      weighted_instructions: count(text, /^\s.*!prof /gm),
      branch_weight_nodes: count(text, /branch_weights/g),
      fast_math_flags: count(text, / (?:fast|reassoc|nnan|ninf|nsz|arcp|contract|afn) /g),
    });
    const text = (name) => readFileSync(join(temporary, name), "utf8");
    const source = join(temporary, "control.ll");
    const irFlags = [...flags, "-Wno-override-module"];
    run(clang, [...irFlags, `-fprofile-use=${merged}`, "-S", "-emit-llvm", source, "-o", join(temporary, "control.pgo.ll")]);
    run(clang, [...irFlags, `-fprofile-use=${merged}`, "-S", source, "-o", join(temporary, "control.pgo.s")]);
    run(clang, [...irFlags, `-fprofile-generate=${profile}`, "-S", "-emit-llvm", source, "-o", join(temporary, "control.gen.ll")]);
    generated.push("control.pgo.ll", "control.pgo.s", "control.gen.ll");
    const size = (name) => statSync(join(temporary, name)).size;
    const entry = (plain, used, variant) => ({
      plain_median_ms: plain[`${variant}_median_ms`], pgo_median_ms: used[`${variant}_median_ms`],
      pgo_over_plain: plain[`${variant}_median_ms`] > 0 ? used[`${variant}_median_ms`] / plain[`${variant}_median_ms`] : null,
    });
    pgoReport = {
      runs: rounds,
      training: "one --quick run of the instrumented Tsuzuri, C and C++ objects, linked with -fprofile-generate",
      llvm_profdata: run(profdata, ["--version"]).trim().split("\n")[0],
      raw_profiles: rawProfiles.length,
      raw_profile_bytes: rawProfiles.reduce((sum, name) => sum + statSync(join(profile, name)).size, 0),
      merged_profile_bytes: statSync(merged).size,
      object_bytes: { plain: size("control.o"), instrumented: size("control.gen.o"), pgo: size("control.pgo.o") },
      inspection: {
        tsuzuri: { plain: describe(optimized), pgo: describe(text("control.pgo.ll")) },
        c: { plain: describe(text("c.ll")), pgo: describe(text("c.pgo.ll")) },
        cpp: { plain: describe(text("cpp.ll")), pgo: describe(text("cpp.pgo.ll")) },
        instrumented_tsuzuri_counters: count(text("control.gen.ll"), /@__profc_/g),
      },
      // Rust is the same object in both hosts, so its ratio says how far the machine moved between the two.
      workloads: result.workloads.map((plain, index) => {
        const used = pgoResult.workloads[index];
        assert.deepEqual([used.name, used.size], [plain.name, plain.size]);
        return {
          name: plain.name, size: plain.size,
          tsuzuri: entry(plain, used, "tsuzuri"), c: entry(plain, used, "c"), cpp: entry(plain, used, "cpp"), rust_unchanged: entry(plain, used, "rust"),
          tsuzuri_over_c: { plain: plain.tsuzuri_over_c, pgo: used.tsuzuri_over_c },
          tsuzuri_over_cpp: { plain: plain.tsuzuri_over_cpp, pgo: used.tsuzuri_over_cpp },
          tsuzuri_over_rust: { plain: plain.tsuzuri_over_rust, pgo: used.tsuzuri_over_rust },
        };
      }),
      host: pgoResult,
    };
  }
  if (metrics) {
    const trackedIr = join(temporary, "control.tracked.ll");
    writeFileSync(trackedIr, instrumentAllocations(optimized));
    run(clang, ["-O0", "-fPIC", "-Wno-override-module", "-c", trackedIr, "-o", join(temporary, "control.tracked.o")]);
    run(clang, ["-std=c11", "-O2", "-Wall", "-Wextra", "-Werror", "-c", "benchmarks/tracked_alloc.c", "-o", join(temporary, "tracked_alloc.o")]);
    const tracked = join(temporary, "tracked");
    run(clang, ["-std=c11", ...flags, "-Wall", "-Wextra", "-Werror", "-DTRACKING",
      `${fixture}/host.c`, "-I", temporary,
      ...["control.tracked.o", "tracked_alloc.o", "c.o", "cpp.o", "control_reference.o"].map((name) => join(temporary, name)),
      ...(optimized.includes("declare void @tsuzuri_task_parallel(") ? ["src/runtime/task.c", "-pthread"] : []), "-lm", "-o", tracked]);
    const tracking = JSON.parse(run(tracked, quick ? ["--quick"] : ["--scale", String(scale)]));
    assert.deepEqual(tracking.workloads.map((work) => [work.name, work.size]), result.workloads.map((work) => [work.name, work.size]));
    const records = inProcessRecords(context, "control", result.workloads,
      [...["c", "cpp", "rust", "tsuzuri"].map((name) => [name, name, name === "tsuzuri" ? tsuzuriVariant : "default"]),
        ...(baseline ? [["before", "tsuzuri", "before"]] : [])], { cpu });
    // A PGO object is another variant of the same workload, so that the records of a run without --pgo are never mistaken for it.
    if (pgo) {
      records.push(...inProcessRecords(context, "control", pgoResult.workloads,
        [["tsuzuri", "tsuzuri", tsuzuriVariant === "default" ? "pgo" : `${tsuzuriVariant}-pgo`], ["c", "c", "pgo"], ["cpp", "cpp", "pgo"]], { cpu }));
    }
    for (const work of tracking.workloads) for (const [metric, field] of [["alloc_calls", "calls"], ["alloc_bytes", "bytes"]]) {
      records.push(makeRecord(context, { suite: "control", workload: work.name, size: work.size, language: "tsuzuri",
        variant: allocationVariant, opt: "O3", cpu_mode: cpu, metric, samples: [work[field]] }));
    }
    writeRecords(metrics, "control", records);
    generated.push("control.tracked.ll", "control.tracked.o", "tracked_alloc.o");
  }
  if (artifacts) {
    mkdirSync(resolve(artifacts), { recursive: true });
    for (const name of generated) copyFileSync(join(temporary, name), join(resolve(artifacts), name));
    if (pgo) cpSync(profile, join(resolve(artifacts), "profile"), { recursive: true });
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
    cpu, scale, wall_clock: "CLOCK_MONOTONIC", c_cpp_flags: flags, rust_flags: rustFlags, tsuzuri_flags: tsuzuriFlags, tsuzuri_allocator: allocator,
    inspection, ...result,
    ...(pgoReport ? { pgo: pgoReport } : {}),
    note: "Native CPU time; two warmups per variant; rotating/reversed order; no LTO. Array workloads include allocation, initialization, traversal and free. C/C++ share a C-compatible baseline; Rust uses the same system allocation with safe slice traversal. Ratios below 1 favor Tsuzuri; microbenchmarks are not a general language ranking.",
  }, null, 2));
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
