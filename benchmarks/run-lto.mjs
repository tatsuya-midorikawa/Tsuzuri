import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, statSync, writeFileSync } from "node:fs";
import { loadavg, tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { captureRun, makeRecord, measureProcess, summarize, writeRecords } from "./metrics.mjs";

// PR08: a C loop calls the small exported Tsuzuri function `tz_add` (benchmarks/lto/small_call.c), linked three ways:
//   object   `--emit object`, an ordinary link: the call stays a call
//   bitcode  `--emit bitcode`, a ThinLTO link by the same Clang: the body of `tz_add` is inlined into the loop
//   c-only   `add` written in C, in the same file, built with -O3 -flto=thin
// One Clang builds and links all three, because bitcode can only be read by the LLVM that is at least as new as the one
// that wrote it. Every variant must print the sum that the loop has, which is computed here again, before it is timed.
// The variants run in a rotating order, one sample each per round, after one untimed run each. There is no threshold: the
// medians and ranges are printed, and a median that lies inside the range of another is not a difference.
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const usage = "Usage: node benchmarks/run-lto.mjs [compiler] [--quick] [--size n] [--artifacts directory] [--metrics directory]";
export const VARIANTS = ["object", "bitcode", "c-only"];
const LIBRARY = "export def add :: i64 -> i64 -> i64\nfn add x y = x + y\n";

export function parseArguments(args) {
  const options = { compiler: join(root, "target/release/tsuzuri"), quick: false, size: null, artifacts: null, metrics: null };
  let positional = false;
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
    else if (flag === "--size") {
      const text = value(index++, flag);
      if (!/^[1-9][0-9]{0,15}$/.test(text)) throw new Error(`invalid --size: ${text}\n${usage}`);
      options.size = Number(text);
    } else if (flag === "--artifacts") options.artifacts = resolve(value(index++, flag));
    else if (flag === "--metrics") options.metrics = resolve(value(index++, flag));
    else throw new Error(`unknown flag: ${flag}\n${usage}`);
  }
  options.size ??= options.quick ? 2_000_000 : 1_000_000_000;
  return options;
}

// The sum that small_call.c prints for `n`: `values` repeats every 1024 elements, as `i * 2654435761u % 1000u` in 32 bits.
export function expectedTotal(n) {
  const values = Array.from({ length: 1024 }, (_, index) => Number((BigInt(index) * 2654435761n) & 0xffffffffn) % 1000);
  const sum = (items) => items.reduce((total, item) => total + BigInt(item), 0n);
  return BigInt(Math.floor(n / 1024)) * sum(values) + sum(values.slice(0, n % 1024));
}
// Independent references: Python, 2026-09-30 (the ticket PR08).
assert.equal(expectedTotal(1000), 498932n);
assert.equal(expectedTotal(1_000_000_000), 499351563280n);

function run(program, programArguments, environment = process.env) {
  const result = spawnSync(program, programArguments, { cwd: root, env: environment, encoding: "utf8", maxBuffer: 256 * 1024 * 1024, timeout: 600_000 });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${program} ${programArguments.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result.stdout;
}

// llvm-objdump of the LLVM that Clang comes from: TSUZURI_OBJDUMP, the one beside Clang, or the one on PATH.
function objdumpTool(clang) {
  if (process.env.TSUZURI_OBJDUMP) return process.env.TSUZURI_OBJDUMP;
  const suffix = process.platform === "win32" ? ".exe" : "";
  return /[\\/]/.test(clang) ? join(dirname(resolve(clang)), `llvm-objdump${suffix}`) : "llvm-objdump";
}

// The instructions of `main` that name tz_add: a call is one, an inlined loop has none.
export function callsToAdd(executable, clang) {
  const result = spawnSync(objdumpTool(clang), ["-d", "--no-show-raw-insn", executable], { encoding: "utf8", maxBuffer: 256 * 1024 * 1024 });
  if (result.error || result.status !== 0) {
    console.error(`calls to tz_add are not counted: ${result.error?.message ?? result.stderr.trim()}`);
    return null;
  }
  const lines = result.stdout.split("\n");
  const start = lines.findIndex((line) => /^[0-9a-f]+ <_?main>:/.test(line));
  assert.ok(start >= 0, "no main in the disassembly");
  let count = 0;
  for (let index = start + 1; index < lines.length && lines[index].trim() !== ""; ++index) {
    if (lines[index].includes("tz_add")) ++count;
  }
  return count;
}

function clangVersion(clang) {
  return run(clang, ["--version"]).split("\n")[0];
}

export function main(args) {
  const { compiler, quick, size, artifacts, metrics } = parseArguments(args);
  const clang = process.env.TSUZURI_CLANG ?? "clang";
  const rounds = quick ? 3 : 9;
  const temporary = mkdtempSync(join(tmpdir(), "tsuzuri-lto-benchmark-"));
  const environment = { ...process.env, TSUZURI_CACHE_DIR: join(temporary, "tsuzuri-cache") };
  const kept = artifacts ?? temporary;
  mkdirSync(kept, { recursive: true });
  const load = [loadavg()[0]];
  try {
    const project = join(temporary, "lib");
    mkdirSync(project);
    writeFileSync(join(project, "Main.tz"), LIBRARY);
    const source = join(root, "benchmarks", "lto", "small_call.c");
    const lto = ["-flto=thin", ...(process.platform === "linux" ? ["-fuse-ld=lld"] : [])];
    const object = join(kept, "add.o"), bitcode = join(kept, "add.bc");
    run(compiler, ["build", project, "--emit", "object", "-O3", "-o", object], environment);
    run(compiler, ["build", project, "--emit", "bitcode", "-O3", "-o", bitcode], environment);
    const executables = Object.fromEntries(VARIANTS.map((variant) => [variant, join(kept, variant)]));
    run(clang, ["-O3", "-c", source, "-o", join(kept, "host.o")]);
    run(clang, ["-O3", join(kept, "host.o"), object, "-o", executables.object]);
    run(clang, ["-O3", ...lto, "-c", source, "-o", join(kept, "host-lto.o")]);
    run(clang, ["-O3", ...lto, join(kept, "host-lto.o"), bitcode, "-o", executables.bitcode]);
    run(clang, ["-O3", ...lto, "-DADD=c_add", source, "-o", executables["c-only"]]);
    // Correctness comes first: a variant that prints another sum is not timed.
    for (const variant of VARIANTS) {
      for (const n of [1000, size]) {
        assert.equal(run(executables[variant], [String(n)]).trim(), String(expectedTotal(n)), `${variant} n=${n}`);
      }
    }
    const calls = Object.fromEntries(VARIANTS.map((variant) => [variant, callsToAdd(executables[variant], clang)]));
    const bytes = Object.fromEntries(VARIANTS.map((variant) => [variant, statSync(executables[variant]).size]));
    const expected = `${expectedTotal(size)}\n`;
    const sample = (variant) => {
      const measured = measureProcess(executables[variant], [String(size)], { env: environment, runs: 1, warmups: 0 });
      assert.equal(measured.stdout[0], expected, variant);
      return measured;
    };
    for (const variant of VARIANTS) sample(variant);
    const wall = Object.fromEntries(VARIANTS.map((variant) => [variant, []]));
    const user = Object.fromEntries(VARIANTS.map((variant) => [variant, []]));
    for (let round = 0; round < rounds; ++round) {
      for (let offset = 0; offset < VARIANTS.length; ++offset) {
        const variant = VARIANTS[(round + offset) % VARIANTS.length];
        const measured = sample(variant);
        wall[variant].push(measured.wall_ms[0]);
        user[variant].push(measured.user_ms[0]);
      }
    }
    load.push(loadavg()[0]);
    const rows = VARIANTS.map((variant) => ({ variant, ...summarize(wall[variant]), bytes: bytes[variant], calls: calls[variant] }));
    const reference = rows.find((row) => row.variant === "c-only").median;
    console.log(`lto/small_call n=${size}, ${rounds} samples per variant, ${clangVersion(clang)}, load average ${load.map((value) => value.toFixed(2)).join(" -> ")}`);
    console.log("variant   median ms     min ms     max ms   vs c-only   executable bytes   tz_add in main");
    for (const row of rows) {
      console.log(`${row.variant.padEnd(8)} ${row.median.toFixed(1).padStart(10)} ${row.min.toFixed(1).padStart(10)} ${row.max.toFixed(1).padStart(10)} ${(row.median / reference).toFixed(2).padStart(10)}x ${String(row.bytes).padStart(18)} ${String(row.calls ?? "n/a").padStart(15)}`);
    }
    writeFileSync(join(kept, "samples.json"), JSON.stringify({ size, rounds, clang: clangVersion(clang), load, wall, user, bytes, calls }, null, 2) + "\n");
    if (metrics) {
      const context = captureRun(metrics, { compiler, clang, mode: quick ? "quick" : "benchmark" });
      const records = [];
      for (const variant of VARIANTS) {
        const fields = { suite: "lto", workload: "small_call", size, language: variant === "c-only" ? "c" : "tsuzuri", variant, opt: "O3" };
        records.push(makeRecord(context, { ...fields, metric: "wall_time", samples: wall[variant] }));
        records.push(makeRecord(context, { ...fields, metric: "user_time", samples: user[variant] }));
        records.push(makeRecord(context, { ...fields, metric: "executable_bytes", samples: [bytes[variant]] }));
      }
      console.log(`records: ${writeRecords(metrics, "lto", records)}`);
    }
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main(process.argv.slice(2));
