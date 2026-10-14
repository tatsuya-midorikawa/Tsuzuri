import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { constants, copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { arch, cpus, platform, release } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { captureRun, inProcessRecords, instrumentAllocations, makeRecord, measureProcess, median, readRecords, writeRecords } from "./metrics.mjs";
import { checkReference, hex, reference, workloads as definitions, wrap } from "./apps/reference.mjs";

const filename = fileURLToPath(import.meta.url);
const root = resolve(dirname(filename), "..");
const fixture = join(root, "benchmarks/apps");
const usage = "Usage: node benchmarks/run-apps.mjs [compiler] [--quick | --scale number] [--cpu generic|native] [--artifacts directory] [--metrics directory]";
const checkSizes = [0, 1, 2, 17, 257];
const checkSeeds = [0n, 42n, (1n << 64n) - 1n, 1n << 63n, (1n << 63n) - 1n];

export function parseArguments(args) {
  const options = { compiler: join(root, "target/release/tsuzuri"), quick: false, scale: 1, cpu: "generic", artifacts: null, metrics: null };
  const seen = new Set();
  let compiler = false;
  for (let index = 0; index < args.length; ++index) {
    const flag = args[index];
    if (!flag.startsWith("-")) {
      if (compiler || !flag) throw new Error(`unexpected positional argument: ${flag}\n${usage}`);
      options.compiler = resolve(flag);
      compiler = true;
      continue;
    }
    if (!["--quick", "--scale", "--cpu", "--artifacts", "--metrics"].includes(flag)) throw new Error(`unknown flag: ${flag}\n${usage}`);
    if (seen.has(flag)) throw new Error(`duplicate flag: ${flag}\n${usage}`);
    seen.add(flag);
    if (flag === "--quick") {
      options.quick = true;
      continue;
    }
    const value = args[++index];
    if (!value || value.startsWith("-")) throw new Error(`missing value for ${flag}${value ? ` (got ${value})` : ""}\n${usage}`);
    if (flag === "--scale") {
      if (!/^(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/.test(value)) throw new Error(`invalid --scale: ${value}\n${usage}`);
      options.scale = Number(value);
      if (!Number.isFinite(options.scale) || options.scale <= 0 || options.scale > 10) throw new Error(`invalid --scale: ${value}\n${usage}`);
    } else if (flag === "--cpu") {
      if (!["generic", "native"].includes(value)) throw new Error(`invalid --cpu: ${value}\n${usage}`);
      options.cpu = value;
    } else options[flag.slice(2)] = resolve(value);
  }
  if (seen.has("--quick") && seen.has("--scale")) throw new Error(`--quick and --scale are mutually exclusive\n${usage}`);
  return options;
}

export function optionalTool(language, env = process.env) {
  assert.ok(["zig", "go"].includes(language));
  const variable = `TSUZURI_${language.toUpperCase()}`, explicit = Object.hasOwn(env, variable);
  const program = explicit ? env[variable] : language;
  if (!program) throw new Error(`${variable} must name an executable`);
  const child = spawnSync(program, ["version"], { cwd: root, env, encoding: "utf8", timeout: 30_000 });
  if (child.error?.code === "ENOENT" && !explicit) {
    console.error(`${language} not found (set ${variable} to its executable); skipping ${language === "zig" ? "Zig" : "Go"} references`);
    return { program: null, version: null, skipped: "not found" };
  }
  if (child.error) throw new Error(`${explicit ? variable : language} (${program}) cannot execute: ${child.error.message}`);
  if (child.status !== 0) throw new Error(`${explicit ? variable : language} (${program}) version failed: ${child.stderr || child.stdout}`);
  const version = child.stdout.trim();
  if (language === "zig" && version !== "0.16.0") throw new Error(`Zig 0.16.0 is required; ${program} reports ${version}`);
  if (language === "go" && !/^go version go1\.27\.2 \S+$/.test(version)) throw new Error(`Go 1.27.2 is required; ${program} reports ${version}`);
  return { program, version, skipped: null };
}

export function assertStrictFloatIR(ir, name) {
  const flagged = /\b(?:fadd|fsub|fmul|fdiv)\s+(?:fast|reassoc|nnan|ninf|nsz|arcp|contract|afn)\b/;
  const unsignedDoubles = new Set(), instructions = [];
  let scope = 0;
  for (const line of ir.split(/\r?\n/)) {
    if (/^define\b/.test(line)) ++scope;
    const conversion = line.match(/^\s*(%[^\s=]+)\s*=\s*uitofp\s+i64\s+(?:%[^\s,]+|\d+)\s+to\s+double(?=\s*(?:,|;|$))/);
    if (conversion) unsignedDoubles.add(`${scope}/${conversion[1]}`);
    if (flagged.test(line)) instructions.push([scope, line]);
  }
  for (const [scope, line] of instructions) {
    // LLVM 23 infers nnan here: u64 -> f64 is finite/nonnegative, and 1/+0 is infinity, not NaN.
    // This does not permit reassociation, contraction, or ignoring infinities/signed zero.
    const reciprocal = line.match(/^\s*%[^\s=]+\s*=\s*fdiv\s+nnan\s+double\s+1\.000000e\+00,\s+(%[^\s,]+)(?:\s*,\s*!.*)?\s*(?:;.*)?$/);
    assert.ok(reciprocal && unsignedDoubles.has(`${scope}/${reciprocal[1]}`), `${name}/strict f64: ${line.trim()}`);
  }
}

export function zigReferenceBuild(source, directory, cpu = "generic", system = platform()) {
  const archive = system === "linux";
  const binary = archive ? "libapps_zig.a" : "zig.o";
  const flags = [...(archive ? ["-static"] : []), "-O", "ReleaseSafe", "-fPIC", "-fllvm", "-fcompiler-rt", "-lc",
    `-mcpu=${cpu === "native" ? "native" : "baseline"}`];
  return {
    binary, flags,
    args: [archive ? "build-lib" : "build-obj", source, ...flags, `-femit-bin=${join(directory, binary)}`,
      `-femit-asm=${join(directory, "zig.s")}`, `-femit-llvm-ir=${join(directory, "zig.optimized.ll")}`],
  };
}

export async function runApps(options) {
  if (Number(process.versions.node.split(".")[0]) < 24) throw new Error("Apps benchmarks require Node.js 24+; use npx --yes --package=node@24 node benchmarks/run-apps.mjs");
  checkReference();
  const { compiler, quick, scale, cpu, artifacts, metrics } = options;
  const clang = process.env.TSUZURI_CLANG ?? "clang";
  const rustc = process.env.TSUZURI_RUSTC ?? process.env.RUSTC ?? "rustc";
  if (metrics && existsSync(join(metrics, "apps.jsonl"))) throw new Error(`apps.jsonl already exists in ${metrics}; choose a new run directory`);
  mkdirSync(join(root, "target"), { recursive: true });
  const temporary = mkdtempSync(join(root, "target", "px02-apps-"));
  const savedTemp = new Map(["TMPDIR", "TMP", "TEMP"].map((key) => [key, process.env[key]]));
  const scratch = join(temporary, "tmp");
  mkdirSync(scratch);
  for (const key of savedTemp.keys()) process.env[key] = scratch;
  const env = {
    ...process.env, TSUZURI_CACHE_DIR: join(temporary, "tsuzuri-cache"),
    ZIG_GLOBAL_CACHE_DIR: join(temporary, "zig-global"), ZIG_LOCAL_CACHE_DIR: join(temporary, "zig-local"),
    GOCACHE: join(temporary, "go-cache"), GOPATH: join(temporary, "go-path"),
    GOTMPDIR: scratch, GOENV: "off", GOTOOLCHAIN: "local", CGO_ENABLED: "1", CC: clang,
  };
  const goDefaults = ["GOGC", "GOMAXPROCS", "GOMEMLIMIT", "GODEBUG", "GOFLAGS", "GOEXPERIMENT", "GOOS", "GOARCH", "GOAMD64", "GOARM64",
    "CGO_CFLAGS", "CGO_CPPFLAGS", "CGO_CXXFLAGS", "CGO_LDFLAGS"];
  for (const key of goDefaults) delete env[key];
  const generated = [];
  const path = (name) => join(temporary, name);
  const run = (program, args, timeout = 180_000) => {
    const child = spawnSync(program, args, { cwd: root, env, encoding: "utf8", timeout, maxBuffer: 256 * 1024 * 1024 });
    if (child.error) throw new Error(`${program} ${args.join(" ")}: ${child.error.message}`);
    if (child.status !== 0) throw new Error(`${program} ${args.join(" ")} exited with ${child.status ?? child.signal}:\n${child.stdout}${child.stderr}`);
    return { stdout: child.stdout, stderr: child.stderr };
  };
  const expectedCache = new Map();
  const expected = (name, size, seed) => {
    const key = `${name}/${size}/${seed}`;
    if (!expectedCache.has(key)) expectedCache.set(key, hex(reference(name, size, BigInt(seed))));
    return expectedCache.get(key);
  };
  const hostArgs = quick ? ["--quick"] : ["--scale", String(scale)];
  const flags = ["-O3", "-fPIC", "-fno-fast-math", "-ffp-contract=off"];
  const rustFlags = ["--edition=2021", "--crate-type=staticlib", "--crate-name=apps_reference",
    "-C", "opt-level=3", "-C", "panic=abort", "-C", "force-unwind-tables=no",
    "-C", "relocation-model=pic", "-C", "codegen-units=1", "-D", "warnings"];
  const zigBuild = zigReferenceBuild(join(fixture, "reference.zig"), temporary, cpu);
  const goFlags = ["build", "-buildmode=c-archive", "-gcflags=-S"];
  const tsuzuriFlags = ["-O3", "--cpu", cpu];
  try {
    if (cpu === "native") {
      if (["x64", "ia32"].includes(arch())) flags.push("-march=native");
      else if (["arm64", "arm"].includes(arch())) flags.push("-mcpu=native");
      else throw new Error(`Native tuning unsupported on ${arch()}`);
      rustFlags.push("-C", "target-cpu=native");
    }
    const zig = optionalTool("zig", env), go = optionalTool("go", env);
    const skipped = Object.fromEntries([["zig", zig], ["go", go]].filter(([, tool]) => tool.skipped).map(([name, tool]) => [name, tool.skipped]));
    const languages = ["c", "cpp", "rust", "tsuzuri", ...(zig.program ? ["zig"] : []), ...(go.program ? ["go"] : [])];
    const context = metrics ? captureRun(metrics, { compiler, clang, rustc, mode: quick ? "quick" : "benchmark" }) : null;
    const compilerHash = createHash("sha256").update(readFileSync(compiler)).digest("hex");
    if (context) assert.equal(context.host.tsuzuri_sha256, compilerHash, "compiler changed while capturing metadata");
    for (const [emit, extension] of [["object", "o"], ["header", "h"], ["llvm", "ll"]]) {
      run(compiler, ["build", fixture, "--emit", emit, ...tsuzuriFlags, "-o", path(`apps.${extension}`)]);
      generated.push(`apps.${extension}`);
    }
    run(clang, [...flags, "-Wno-override-module", "-S", "-emit-llvm", path("apps.ll"), "-o", path("apps.optimized.ll")]);
    run(clang, [...flags, "-Wno-override-module", "-S", path("apps.ll"), "-o", path("apps.s")]);
    generated.push("apps.optimized.ll", "apps.s");
    for (const language of ["c", "cpp"]) {
      const args = [language === "c" ? "-std=c11" : "--driver-mode=g++", ...(language === "cpp" ? ["-std=c++20"] : []),
        ...flags, "-Wall", "-Wextra", "-Werror", join(fixture, `reference.${language === "c" ? "c" : "cpp"}`)];
      run(clang, [...args, "-c", "-o", path(`${language}.o`)]);
      run(clang, [...args, "-S", "-o", path(`${language}.s`)]);
      run(clang, [...args, "-S", "-emit-llvm", "-o", path(`${language}.optimized.ll`)]);
      generated.push(`${language}.o`, `${language}.s`, `${language}.optimized.ll`);
    }
    const rust = run(rustc, [...rustFlags, "--print", "native-static-libs", "--emit=link,obj,asm,llvm-ir",
      join(fixture, "reference.rs"), "--out-dir", temporary]);
    const nativeLibs = rust.stderr.match(/native-static-libs:\s*([^\r\n]+)/)?.[1]?.trim().split(/\s+/);
    assert.ok(nativeLibs?.length, "rustc did not report native-static-libs");
    assert.ok(nativeLibs.every((argument) => /^[-A-Za-z0-9_./+:=]+$/.test(argument)), "unsupported rustc native-static-libs argument");
    generated.push("libapps_reference.a", "apps_reference.o", "apps_reference.s", "apps_reference.ll");
    if (zig.program) {
      // ELF's combined runtime object can crash; archive its members without changing codegen.
      run(zig.program, zigBuild.args);
      generated.push(zigBuild.binary, "zig.s", "zig.optimized.ll");
    }
    if (go.program) {
      const build = run(go.program, [...goFlags, "-o", path("libapps_go.a"), join(fixture, "reference.go")]);
      writeFileSync(path("go.s"), build.stderr, { flag: "wx" });
      generated.push("libapps_go.a", "libapps_go.h", "go.s");
    }
    const objects = {
      c: "c.o", cpp: "cpp.o", rust: "libapps_reference.a", tsuzuri: "apps.o",
      ...(zig.program ? { zig: zigBuild.binary } : {}), ...(go.program ? { go: "libapps_go.a" } : {}),
    };
    const hostDefines = [...(zig.program ? ["-DHAVE_ZIG"] : []), ...(go.program ? ["-DHAVE_GO"] : [])];
    const compileHost = (name, defines, selected, libraries = []) => {
      run(clang, ["-std=c11", ...flags, "-Wall", "-Wextra", "-Werror", ...defines,
        join(fixture, "host.c"), "-I", temporary, "-c", "-o", path(`${name}.o`)]);
      const cpp = selected.includes(objects.cpp);
      run(clang, [...(cpp ? ["--driver-mode=g++"] : []), ...flags, path(`${name}.o`),
        ...selected.map(path), ...libraries, "-lm", "-o", path(name)]);
      generated.push(name, `${name}.o`);
      return path(name);
    };
    const native = compileHost("apps-native", hostDefines, languages.map((language) => objects[language]), nativeLibs);
    const result = JSON.parse(run(native, hostArgs, quick ? 180_000 : 600_000).stdout);
    assert.deepEqual(result.variants, languages);
    assert.equal(result.samples, languages.length * (quick ? 1 : 3));
    assert.deepEqual(result.workloads.map((work) => work.name), definitions.map((work) => work.name));
    const cases = checkSizes.flatMap((size) => checkSeeds.map((seed) => [size, seed.toString()]));
    const verifyChecks = (work) => {
      assert.deepEqual(work.checks.map((check) => [check.size, check.seed]), cases, `${work.name}/25 boundary cases`);
      for (const check of work.checks) assert.equal(check.checksum, expected(work.name, check.size, check.seed), `${work.name}/${check.size}/${check.seed}`);
    };
    for (const work of result.workloads) {
      const definition = definitions.find((entry) => entry.name === work.name);
      assert.equal(work.size, quick ? definition.quick : Math.max(1, Math.ceil(definition.normal * scale)));
      verifyChecks(work);
      assert.equal(work.warmups.length, 2);
      for (const [index, warmup] of work.warmups.entries()) {
        assert.equal(warmup.seed, String(42 + index));
        assert.equal(warmup.checksum, expected(work.name, work.size, warmup.seed), `${work.name}/warmup`);
      }
      assert.equal(work.raw.length, result.samples);
      for (const [index, sample] of work.raw.entries()) {
        assert.equal(sample.seed, wrap(18446744073709551557n + BigInt(index) * 71n).toString());
        assert.equal(sample.checksum, expected(work.name, work.size, sample.seed), `${work.name}/timed/${sample.seed}`);
        for (const language of languages) {
          assert.ok(Number.isSafeInteger(sample[`${language}_repeats`]) && sample[`${language}_repeats`] >= 1 && sample[`${language}_repeats`] <= 10_000);
          if (quick) assert.equal(sample[`${language}_repeats`], 1);
          for (const field of [`${language}_ms`, `${language}_wall_ms`]) assert.ok(Number.isFinite(sample[field]) && sample[field] >= 0, `${work.name}/${field}`);
        }
      }
      for (const language of languages) {
        work[`${language}_median_ms`] = median(work.raw.map((sample) => sample[`${language}_ms`]));
        work[`${language}_wall_median_ms`] = median(work.raw.map((sample) => sample[`${language}_wall_ms`]));
      }
      for (const language of languages.filter((language) => language !== "tsuzuri")) {
        work[`tsuzuri_over_${language}`] = work[`${language}_median_ms`] > 0
          ? work.tsuzuri_median_ms / work[`${language}_median_ms`] : null;
      }
    }
    run(compiler, ["build", fixture, "--emit", "object", "-O0", "--cpu", cpu, "-o", path("apps-O0.o")]);
    generated.push("apps-O0.o");
    const unoptimized = compileHost("apps-native-O0", ["-DPROCESS_PREFIX=tz_", '-DPROCESS_LABEL="tsuzuri"'], ["apps-O0.o"]);
    const nativeO0 = JSON.parse(run(unoptimized, ["--checks-only", ...hostArgs]).stdout);
    assert.equal(nativeO0.samples, 0);
    assert.deepEqual(nativeO0.variants, ["tsuzuri"]);
    assert.deepEqual(nativeO0.workloads.map((work) => work.name), definitions.map((work) => work.name));
    for (const [index, work] of nativeO0.workloads.entries()) {
      verifyChecks(work);
      assert.deepEqual(work.checks, result.workloads[index].checks, `${work.name}/native O0 vs O3`);
      assert.deepEqual(work.raw, []);
    }
    const wasmChecks = {};
    for (const opt of ["O0", "O3"]) {
      const wasm = `apps-${opt}.wasm`;
      run(compiler, ["build", fixture, "--target", "wasm32", `-${opt}`, "-o", path(wasm)]);
      const module = new WebAssembly.Module(readFileSync(path(wasm)));
      assert.deepEqual(WebAssembly.Module.imports(module), [], `${opt}/standalone WASM`);
      const instance = new WebAssembly.Instance(module, {});
      let count = 0;
      for (const work of result.workloads) {
        const kernel = instance.exports[`tz_${work.name}`];
        assert.equal(typeof kernel, "function", `${work.name}/${opt}/export`);
        for (const check of work.checks) {
          assert.equal(hex(kernel(BigInt(check.size), BigInt(check.seed))), check.checksum, `${work.name}/${opt}/${check.size}/${check.seed}`);
          ++count;
        }
      }
      wasmChecks[opt] = { checks: count, imports: [] };
      generated.push(wasm);
    }
    const optimized = readFileSync(path("apps.optimized.ll"), "utf8");
    writeFileSync(path("apps.tracked.ll"), instrumentAllocations(optimized), { flag: "wx" });
    run(clang, ["-O0", "-fPIC", "-Wno-override-module", "-c", path("apps.tracked.ll"), "-o", path("apps.tracked.o")]);
    run(clang, ["-std=c11", "-O2", "-Wall", "-Wextra", "-Werror", "-c", join(root, "benchmarks/tracked_alloc.c"), "-o", path("tracked_alloc.o")]);
    generated.push("apps.tracked.ll", "apps.tracked.o", "tracked_alloc.o");
    const tracked = compileHost("apps-tracking", ["-DTRACKING"], ["apps.tracked.o", "tracked_alloc.o", "c.o"]);
    const tracking = JSON.parse(run(tracked, hostArgs, quick ? 180_000 : 600_000).stdout);
    assert.equal(tracking.tracking, 1);
    assert.deepEqual(tracking.workloads.map((work) => [work.name, work.size]), result.workloads.map((work) => [work.name, work.size]));
    for (const work of tracking.workloads) {
      assert.equal(work.seed, "42");
      assert.equal(work.checksum, expected(work.name, work.size, work.seed), `${work.name}/tracked`);
      assert.equal(work.live, 0, `${work.name}/retained bytes`);
      for (const field of ["calls", "bytes"]) assert.ok(Number.isSafeInteger(work[field]) && work[field] >= 0);
    }
    assert.doesNotMatch(readFileSync(join(fixture, "reference.rs"), "utf8"), /\b(?:unsafe|get_unchecked)\b/);
    for (const name of ["apps.optimized.ll", "c.optimized.ll", "cpp.optimized.ll", "apps_reference.ll", ...(zig.program ? ["zig.optimized.ll"] : [])]) {
      assertStrictFloatIR(readFileSync(path(name), "utf8"), name);
    }
    if (go.program) assert.doesNotMatch(readFileSync(path("go.s"), "utf8"), /\b(?:V?FMADD|V?FMSUB)[A-Z0-9_.]*\b/i, "Go's explicit rounding prevents fused multiply-add");
    let metricCount = 0;
    const records = [];
    if (metrics) {
      records.push(...inProcessRecords(context, "apps", result.workloads, languages.map((language) => [language, language, "default"]), { cpu }));
      for (const language of languages) {
        const prefix = language === "tsuzuri" ? "tz_" : `${language}_`;
        const processHost = compileHost(`apps-rss-${language}`, [`-DPROCESS_PREFIX=${prefix}`, `-DPROCESS_LABEL="${language}"`],
          [objects[language]], language === "rust" ? nativeLibs : []);
        for (const work of result.workloads) {
          const measured = measureProcess(processHost, ["--only", work.name, language, ...hostArgs], {
            cwd: root, env, runs: quick ? 1 : 9, warmups: quick ? 0 : 1, timeoutMs: quick ? 180_000 : 600_000,
          });
          const checksum = expected(work.name, work.size, "42");
          for (const output of measured.stdout) assert.equal(output.trim(), checksum, `${work.name}/${language}/RSS checksum`);
          assert.ok(measured.peak_rss_bytes.every((value) => Number.isSafeInteger(value) && value > 0));
          records.push(makeRecord(context, { suite: "apps", workload: work.name, size: work.size, language, opt: "O3",
            cpu_mode: cpu, metric: "peak_rss", samples: measured.peak_rss_bytes }));
          work[`${language}_peak_rss_bytes`] = measured.peak_rss_bytes;
        }
      }
      for (const work of tracking.workloads) for (const [metric, field] of [["alloc_calls", "calls"], ["alloc_bytes", "bytes"]]) {
        records.push(makeRecord(context, { suite: "apps", workload: work.name, size: work.size, language: "tsuzuri",
          opt: "O3", cpu_mode: cpu, metric, samples: [work[field]] }));
      }
      metricCount = definitions.length * languages.length * 3 + definitions.length * 2;
      assert.equal(records.length, metricCount);
    }
    assert.equal(createHash("sha256").update(readFileSync(compiler)).digest("hex"), compilerHash, "compiler changed during the benchmark");
    if (metrics) {
      writeRecords(metrics, "apps", records);
      assert.equal(readRecords(metrics).filter((record) => record.suite === "apps").length, metricCount);
    }
    if (artifacts) {
      mkdirSync(artifacts, { recursive: true });
      for (const name of generated) copyFileSync(path(name), join(artifacts, name), constants.COPYFILE_EXCL);
    }
    return {
      environment: {
        tsuzuri: run(compiler, ["--version"]).stdout.trim(), compiler_sha256: compilerHash,
        clang: run(clang, ["--version"]).stdout.trim(), rustc: run(rustc, ["-vV"]).stdout.trim(),
        zig: zig.version, go: go.version,
        go_gc: go.program ? { gogc: 100, gomemlimit: "unlimited default", gomaxprocs: "automatic default",
          gomaxprocs_effective: result.go_runtime.gomaxprocs, num_cpu: result.go_runtime.num_cpu,
          ignored_environment_overrides: goDefaults.filter((key) => Object.hasOwn(process.env, key)) } : null,
        node: process.version, platform: platform(), os_release: release(), architecture: arch(),
        cpu: cpus()[0]?.model, logical_cpus: cpus().length,
      },
      mode: quick ? "correctness-smoke" : "benchmark", cpu, scale, skipped,
      wall_clock: "CLOCK_MONOTONIC", c_cpp_flags: flags, rust_flags: rustFlags,
      rust_native_static_libs: nativeLibs, zig_flags: zig.program ? zigBuild.flags : null,
      go_flags: go.program ? goFlags : null, tsuzuri_flags: tsuzuriFlags,
      correctness: { native_O0: { checks: 250, language: "tsuzuri" }, native_O3: { checks_per_language: 250, languages },
        wasm: wasmChecks, independent_reference: "BigInt wrapping u64 and strictly ordered Number f64" },
      allocations: tracking.workloads, metrics: metrics ? { directory: metrics, records: metricCount } : null,
      inspection: {
        optimized_allocator_call_sites: (optimized.match(/\bcall\b[^\n]*@(malloc|calloc|realloc)\(/g) ?? []).length,
        strict_f64: true, rust_safe: true, artifacts: artifacts ? generated : [],
        go_assembly: go.program ? "Go compiler's optimized -S listing; Go does not produce LLVM IR" : null,
      },
      ...result,
      note: "Independent native implementations; sorted-vector Map/Set, stable bottom-up merge and CSR in every language. Owned tokens, rows, fields and decimal strings; UTF-16 in Tsuzuri and bytes elsewhere. C/C++ omit index checks; Rust, Zig ReleaseSafe, Go and Tsuzuri retain their normal safety checks. Tsuzuri's payload-free Tree.Leaf has a null representation; the other native references allocate leaf nodes. Go uses native GC and default automatic GOMAXPROCS; its build uses the Go compiler's default CPU target. Two warmups and rotating/reversed in-process order, no LTO. RSS uses a separate single-language host and exactly one seed-42 call (quick: one sample; benchmark: nine plus one warmup); it includes process/runtime startup. Tracked Tsuzuri calls use actual workload sizes and retain zero bytes. Quick results verify correctness, not performance.",
    };
  } finally {
    for (const [key, value] of savedTemp) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
    rmSync(temporary, { recursive: true, force: true });
  }
}

if (process.argv[1] && resolve(process.argv[1]) === filename) {
  try {
    console.log(JSON.stringify(await runApps(parseArguments(process.argv.slice(2))), null, 2));
  } catch (error) {
    console.error(error.stack ?? error.message);
    process.exitCode = 1;
  }
}
