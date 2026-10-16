import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { arch, cpus, loadavg, platform, release, tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

// PM05 Phase 2: --allocator system against --allocator small on the PX02 apps, Tsuzuri only. The same compiler builds
// benchmarks/apps as an object with each allocator; benchmarks/apps/host.c, built once per object, runs every workload in
// process (CPU time of calibrated repeats, as run-apps.mjs does), and a separate process per workload gives the peak RSS.
// Rounds alternate the order of the two builds. The checksums of both builds must agree. No threshold: it prints the
// medians and ranges, the ratio of the two, and the load average at the start and the end.
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const usage = "Usage: node benchmarks/run-small.mjs [compiler] [--rounds n] [--rss-runs n] [--scale x] [--only name,...] [--cpu generic|native] [--json file]";

export function parseArguments(args) {
  const options = { compiler: join(root, "target/release/tsuzuri"), rounds: 5, rssRuns: 9, scale: 1, only: [], cpu: "generic", json: null };
  let positional = false;
  const value = (index, flag) => {
    if (index + 1 >= args.length || args[index + 1].startsWith("--")) throw new Error(`missing value for ${flag}\n${usage}`);
    return args[index + 1];
  };
  const count = (text, flag) => {
    if (!/^[1-9][0-9]*$/.test(text)) throw new Error(`invalid ${flag}: ${text}\n${usage}`);
    return Number(text);
  };
  for (let index = 0; index < args.length; ++index) {
    const flag = args[index];
    if (!flag.startsWith("--")) {
      if (positional) throw new Error(`unexpected positional argument: ${flag}\n${usage}`);
      options.compiler = resolve(flag);
      positional = true;
    } else if (flag === "--rounds") options.rounds = count(value(index++, flag), flag);
    else if (flag === "--rss-runs") options.rssRuns = count(value(index++, flag), flag);
    else if (flag === "--scale") {
      options.scale = Number(value(index++, flag));
      if (!Number.isFinite(options.scale) || options.scale <= 0 || options.scale > 10) throw new Error(`invalid --scale: ${args[index]}\n${usage}`);
    } else if (flag === "--only") options.only = value(index++, flag).split(",").filter(Boolean);
    else if (flag === "--cpu") {
      options.cpu = value(index++, flag);
      if (!["generic", "native"].includes(options.cpu)) throw new Error(`invalid --cpu: ${options.cpu}\n${usage}`);
    } else if (flag === "--json") options.json = resolve(value(index++, flag));
    else throw new Error(`unknown flag: ${flag}\n${usage}`);
  }
  return options;
}

const median = (values) => {
  const sorted = [...values].sort((a, b) => a - b);
  return (sorted[Math.floor((sorted.length - 1) / 2)] + sorted[Math.floor(sorted.length / 2)]) / 2;
};

function run(program, programArguments) {
  const result = spawnSync(program, programArguments, { cwd: root, encoding: "utf8", maxBuffer: 256 * 1024 * 1024, timeout: 900_000 });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${program} ${programArguments.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}

// The peak resident set size of one process, in bytes, from /usr/bin/time (BSD -l on macOS, GNU -f %M on Linux).
function peakRss(host, name, scale) {
  const command = platform() === "darwin" ? ["-l"] : platform() === "linux" ? ["-f", "%M"] : null;
  if (!command) throw new Error("peak RSS is measured with /usr/bin/time on macOS and Linux");
  const result = run("/usr/bin/time", [...command, host, "--only", name, "tsuzuri", "--scale", String(scale)]);
  const bytes = platform() === "darwin" ? Number(/(\d+)\s+maximum resident set size/.exec(result.stderr)?.[1]) : Number(result.stderr.trim().split("\n").at(-1)) * 1024;
  assert.ok(bytes > 0, `no peak RSS for ${name}: ${result.stderr}`);
  return { bytes, checksum: result.stdout.trim() };
}

export function runSmall(options) {
  const { compiler, rounds, rssRuns, scale, only, cpu, json } = options;
  const clang = process.env.TSUZURI_CLANG ?? "clang";
  const work = mkdtempSync(join(tmpdir(), "tsuzuri-run-small-"));
  try {
    const allocators = ["system", "small"];
    run(compiler, ["build", join(root, "benchmarks/apps"), "--emit", "header", "-O3", "--cpu", cpu, "-o", join(work, "apps.h")]);
    const hosts = {};
    for (const allocator of allocators) {
      const object = join(work, `apps-${allocator}.o`);
      run(compiler, ["build", join(root, "benchmarks/apps"), "--emit", "object", "-O3", "--cpu", cpu, ...(allocator === "small" ? ["--allocator", "small"] : []), "--no-cache", "-o", object]);
      const hostObject = join(work, `host-${allocator}.o`);
      run(clang, ["-std=c11", "-O3", "-fPIC", "-fno-fast-math", "-ffp-contract=off", "-DPROCESS_PREFIX=tz_", '-DPROCESS_LABEL="tsuzuri"', join(root, "benchmarks/apps/host.c"), "-I", work, "-c", "-o", hostObject]);
      hosts[allocator] = join(work, `host-${allocator}`);
      run(clang, ["-O3", hostObject, object, "-lm", "-o", hosts[allocator]]);
    }
    const loadStart = loadavg();
    const timings = {}; // workload -> allocator -> CPU ms of every sample
    const sizes = {}, checks = {};
    for (let round = 0; round < rounds; round++) {
      for (const allocator of round % 2 ? [...allocators].reverse() : allocators) {
        const result = JSON.parse(run(hosts[allocator], ["--scale", String(scale)]).stdout);
        for (const workload of result.workloads) {
          if (only.length && !only.includes(workload.name)) continue;
          sizes[workload.name] = workload.size;
          const key = JSON.stringify(workload.checks);
          assert.equal((checks[workload.name] ??= key), key, `${workload.name}: the checksums of the two allocators differ`);
          ((timings[workload.name] ??= {})[allocator] ??= []).push(...workload.raw.map((sample) => sample.tsuzuri_ms));
        }
      }
    }
    const names = Object.keys(timings);
    assert.ok(names.length > 0, `no workload selected: ${only.join(",")}`);
    const rss = {};
    for (const name of names) {
      for (let runIndex = 0; runIndex < rssRuns; runIndex++) {
        const checksums = [];
        for (const allocator of runIndex % 2 ? [...allocators].reverse() : allocators) {
          const measured = peakRss(hosts[allocator], name, scale);
          checksums.push(measured.checksum);
          ((rss[name] ??= {})[allocator] ??= []).push(measured.bytes);
        }
        assert.equal(checksums[0], checksums[1], `${name}: the results of the two allocators differ`);
      }
    }
    const loadEnd = loadavg();
    const rows = names.map((name) => {
      const system = timings[name].system, small = timings[name].small;
      return {
        workload: name, size: sizes[name], samples: system.length,
        system_ms: [Math.min(...system), median(system), Math.max(...system)], small_ms: [Math.min(...small), median(small), Math.max(...small)],
        ratio_median: median(small) / median(system), ratio_min: Math.min(...small) / Math.min(...system),
        system_rss_bytes: median(rss[name].system), small_rss_bytes: median(rss[name].small),
      };
    });
    const number = (value) => (value < 10 ? value.toFixed(3) : value.toFixed(2));
    const mebibytes = (bytes) => (bytes / 1048576).toFixed(2);
    console.log("| workload | size | system ms (min / median / max) | small ms (min / median / max) | small / system (median, min) | system RSS MiB | small RSS MiB |");
    console.log("|---|---|---|---|---|---|---|");
    for (const row of rows) {
      console.log(`| ${row.workload} | ${row.size} | ${row.system_ms.map(number).join(" / ")} | ${row.small_ms.map(number).join(" / ")} | ${row.ratio_median.toFixed(3)}, ${row.ratio_min.toFixed(3)} | ${mebibytes(row.system_rss_bytes)} | ${mebibytes(row.small_rss_bytes)} |`);
    }
    const environment = { cpu: cpus()[0]?.model, arch: arch(), os: `${platform()} ${release()}`, load_start: loadStart, load_end: loadEnd, rounds, rss_runs: rssRuns, scale, cpu_mode: cpu };
    console.log(`\nrounds ${rounds} (${rows[0].samples} samples per build), rss runs ${rssRuns}, scale ${scale}, cpu ${cpu}; ${environment.cpu}, ${environment.arch}, ${environment.os}; load average ${loadStart.map((x) => x.toFixed(2)).join(" ")} -> ${loadEnd.map((x) => x.toFixed(2)).join(" ")}`);
    if (json) writeFileSync(json, `${JSON.stringify({ environment, rows }, null, 2)}\n`);
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    runSmall(parseArguments(process.argv.slice(2)));
  } catch (error) {
    console.error(error.message);
    process.exit(2);
  }
}
