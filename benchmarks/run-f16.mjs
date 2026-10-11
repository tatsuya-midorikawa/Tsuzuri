// Compares the f16 paths of the compiler on three kernels over an f64 array, and against C `_Float16` (D10).
//
//   node benchmarks/run-f16.mjs BASELINE-COMPILER [COMPILER] [--quick] [--samples N] [--out DIR]
//
// BASELINE-COMPILER is the compiler before D10, so its f16 is the bundled software runtime ("soft"); COMPILER is the one
// under test ("hardware" where it picks `half` instructions). On Apple Silicon a third Tsuzuri build, "promoted", is the
// IR of COMPILER compiled with `-mcpu=generic`, which has no FP16 arithmetic: each `half` operation becomes conversions to
// binary32 around one binary32 operation, as on the AArch64 CPUs of Linux and Windows without FEAT_FP16.
// Every implementation must return the same bits (the runner fails otherwise); there is no speed threshold.
// Raw data: target/perf/D10/<run id>/results.jsonl (JSON Lines, a `host` record first).
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { cpus, loadavg, release, tmpdir } from "node:os";
import { join, resolve } from "node:path";

const args = process.argv.slice(2);
const flag = (name, fallback) => { const index = args.indexOf(name); return index < 0 ? fallback : args.splice(index, 2)[1]; };
const quick = args.includes("--quick");
const samples = Number(flag("--samples", quick ? 3 : 9));
const runId = `${new Date().toISOString().replace(/[:.]/g, "-")}${quick ? "-quick" : ""}`;
const out = resolve(flag("--out", join("target", "perf", "D10", runId)));
const [baseline, current = baseline] = args.filter((argument) => !argument.startsWith("--")).map((path) => resolve(path));
assert.ok(baseline, "usage: node benchmarks/run-f16.mjs BASELINE-COMPILER [COMPILER] [--quick] [--samples N] [--out DIR]");
const clang = process.env.TSUZURI_CLANG ?? "clang";
const root = mkdtempSync(join(tmpdir(), "tsuzuri-f16-benchmark-"));
const execute = (program, programArgs) => {
  const result = spawnSync(program, programArgs, { encoding: "utf8", timeout: 600_000, maxBuffer: 64 * 1024 * 1024 });
  assert.ifError(result.error);
  assert.equal(result.status, 0, `${program} ${programArgs.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result.stdout;
};
const first = (text) => text.split("\n")[0];
const workloads = ["half_chain", "half_roundtrip", "half_scale"];
try {
  const builds = [{ name: "soft", compiler: baseline }, { name: "hardware", compiler: current }];
  if (process.platform === "darwin" && process.arch === "arm64") builds.push({ name: "promoted", compiler: current, flags: ["-mcpu=generic"] });
  const records = [];
  const names = new Set();
  for (const build of builds) {
    const object = join(root, `${build.name}.o`);
    if (build.flags) {
      const ir = join(root, `${build.name}.ll`);
      execute(build.compiler, ["build", "benchmarks/f16", "--emit", "llvm", "-O3", "-o", ir]);
      execute(clang, ["-O3", "-Wno-override-module", ...build.flags, "-c", ir, "-o", object]);
    } else {
      execute(build.compiler, ["build", "benchmarks/f16", "--emit", "object", "-O3", "--no-cache", "-o", object]);
    }
    // The C kernels use `_Float16`, which a compiler that rounds binary16 operations through binary32 would compute differently.
    const excess = process.arch === "arm64" ? [] : ["-ffloat16-excess-precision=none"];
    const executable = join(root, `${build.name}${process.platform === "win32" ? ".exe" : ""}`);
    const link = [...excess, "-O3", "-std=c11", "-ffp-contract=off", "-fno-lto", "benchmarks/f16/f16.c", object, "-lm", "-o", executable];
    const linked = spawnSync(clang, link, { encoding: "utf8" });
    if (linked.status !== 0 && excess.length) execute(clang, link.slice(excess.length));
    else assert.equal(linked.status, 0, linked.stderr);
    const lines = execute(executable, [quick ? "4097" : "1000000", String(samples)]).trim().split("\n").map((line) => JSON.parse(line));
    for (const line of lines) {
      records.push({ build: build.name, ...line });
      names.add(`${line.workload}/${line.implementation}`);
    }
  }
  // The same bits from every implementation of a workload: soft, hardware, promoted, and C.
  for (const workload of workloads) {
    const checksums = new Set(records.filter((record) => record.workload === workload).map((record) => record.checksum));
    assert.equal(checksums.size, 1, `${workload}: the implementations disagree: ${[...checksums]}`);
  }
  const host = {
    date: new Date().toISOString(), baseline, compiler: current,
    version: first(execute(current, ["--version"])), clang: first(execute(clang, ["--version"])), node: process.version,
    cpu: cpus()[0]?.model, cores: cpus().length, os: `${process.platform} ${release()}`, arch: process.arch, loadavg_end: loadavg().map((value) => +value.toFixed(1)),
    commit: first(spawnSync("git", ["rev-parse", "--short", "HEAD"], { encoding: "utf8" }).stdout ?? ""),
    dirty: Boolean(spawnSync("git", ["status", "--porcelain"], { encoding: "utf8" }).stdout?.trim()),
    quick, samples, note: "Medians of rotating runs on a possibly loaded machine; no speed threshold",
  };
  mkdirSync(out, { recursive: true });
  writeFileSync(join(out, "results.jsonl"), [{ host }, ...records].map((entry) => JSON.stringify(entry)).join("\n") + "\n");
  for (const workload of workloads) {
    for (const build of builds) {
      const record = records.find((entry) => entry.workload === workload && entry.build === build.name && entry.implementation === "tsuzuri");
      console.log(JSON.stringify({ workload, implementation: `tsuzuri ${build.name}`, milliseconds: record.milliseconds, min: record.min, max: record.max, count: record.count, repeats: record.repeats, checksum: record.checksum, quick }));
    }
    // C ran next to each Tsuzuri build; the one beside the hardware build is the reference.
    const record = records.find((entry) => entry.workload === workload && entry.build === "hardware" && entry.implementation === "c");
    console.log(JSON.stringify({ workload, implementation: "C _Float16", milliseconds: record.milliseconds, min: record.min, max: record.max, count: record.count, repeats: record.repeats, checksum: record.checksum, quick }));
  }
  console.log(JSON.stringify({ rawData: join(out, "results.jsonl") }));
} finally { rmSync(root, { recursive: true, force: true }); }
