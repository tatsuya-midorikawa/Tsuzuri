import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { hex, reference, checkReference, workloads } from "../benchmarks/apps/reference.mjs";
import { optionalTool, parseArguments } from "../benchmarks/run-apps.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
checkReference();
assert.equal(workloads.length, 10);
assert.equal(hex(-1n), "ffffffffffffffff");
assert.throws(() => reference("missing", 1, 0n), /unknown app workload/);
for (const work of workloads) {
  for (const size of [-1, 0.5, work.maximum + 1]) assert.throws(() => reference(work.name, size, 0n), /invalid .* size/);
  for (const seed of [0n, 42n, (1n << 64n) - 1n, 1n << 63n, (1n << 63n) - 1n]) {
    assert.equal(reference(work.name, 0, seed), 0n);
    assert.equal(reference(work.name, 2, seed + (1n << 64n)), reference(work.name, 2, seed));
  }
}
assert.equal(reference("graph_bfs", 1, 42n), 0n);
assert.equal(reference("record_sort", 1, 42n), 0n);
assert.equal(reference("spectral_norm", 17, 42n) ^ 42n, reference("spectral_norm", 17, 0n));

const options = parseArguments(["--quick", "target/release/tsuzuri", "--cpu", "native", "--metrics", "target/perf/apps-test", "--artifacts", "target/apps-artifacts"]);
assert.equal(options.quick, true);
assert.equal(options.cpu, "native");
assert.equal(options.scale, 1);
assert.equal(parseArguments(["--scale", "1e-3"]).scale, 0.001);
for (const args of [
  ["--unknown"], ["--baseline", "x"], ["-O3"], ["--quick", "--quick"],
  ["--scale", "1", "--scale", "1"], ["--cpu", "generic", "--cpu", "generic"],
  ["--metrics", "x", "--metrics", "y"], ["--artifacts", "x", "--artifacts", "y"],
  ["--scale"], ["--cpu"], ["--metrics"], ["--artifacts"],
  ["--metrics", "--quick"], ["--metrics", "-x"], ["--artifacts", "-q"], ["--quick", "--scale", "1"],
  ["--scale", "0"], ["--scale", "-1"], ["--scale", "Infinity"], ["--scale", "NaN"],
  ["--scale", "0x1"], ["--scale", "10.1"], ["--cpu", "wrong"],
  ["one", "two"],
]) assert.throws(() => parseArguments(args), /unknown|duplicate|missing|mutually exclusive|invalid|unexpected/, JSON.stringify(args));

mkdirSync(join(root, "target"), { recursive: true });
const temporary = mkdtempSync(join(root, "target", "px02-apps-tests-"));
const oldError = console.error;
try {
  const env = { ...process.env, PATH: temporary, TMPDIR: temporary, TMP: temporary, TEMP: temporary };
  delete env.TSUZURI_ZIG;
  delete env.TSUZURI_GO;
  const messages = [];
  console.error = (message) => messages.push(message);
  assert.equal(optionalTool("zig", env).skipped, "not found");
  assert.equal(optionalTool("go", env).skipped, "not found");
  assert.deepEqual(messages, [
    "zig not found (set TSUZURI_ZIG to its executable); skipping Zig references",
    "go not found (set TSUZURI_GO to its executable); skipping Go references",
  ]);
  console.error = oldError;
  for (const [language, version] of [["zig", "0.16.0"], ["go", "go version go1.27.2 test/arch"]]) {
    writeFileSync(join(temporary, language), `#!/bin/sh\nprintf '${version}\\n'\n`, { mode: 0o755, flag: "wx" });
    assert.equal(optionalTool(language, env).version, version);
    const variable = `TSUZURI_${language.toUpperCase()}`;
    assert.throws(() => optionalTool(language, { ...env, [variable]: "" }), /must name an executable/);
    assert.throws(() => optionalTool(language, { ...env, [variable]: join(temporary, "absent") }), /cannot execute/);
    const child = spawnSync(process.execPath, [join(root, "benchmarks/run-apps.mjs"), "--quick"], {
      cwd: root, env: { ...env, [variable]: join(temporary, "absent") }, encoding: "utf8", timeout: 30_000,
    });
    assert.equal(child.status, 1);
    assert.match(child.stderr, new RegExp(`${variable}.*cannot execute`));
    assert.equal(child.stdout, "");
  }
  if (process.env.TSUZURI_APPS_HOST) {
    const host = resolve(process.env.TSUZURI_APPS_HOST);
    const runHost = (args) => spawnSync(host, args, {
      cwd: root, env: { ...process.env, TMPDIR: temporary, TMP: temporary, TEMP: temporary },
      encoding: "utf8", timeout: 30_000,
    });
    for (const work of workloads) for (const language of ["c", "tsuzuri"]) {
      const child = runHost(["--only", work.name, language, "--quick"]);
      assert.equal(child.status, 0, child.stderr);
      assert.equal(child.stdout, `${hex(reference(work.name, work.quick, 42n))}\n`);
    }
    for (const args of [
      ["--unknown"], ["--baseline", "x"], ["--quick", "--quick"], ["--scale"],
      ["--scale", "1", "--scale", "1"], ["--scale", "0"], ["--scale", "NaN"],
      ["--only"], ["--only", "tokenize"], ["--only", "missing", "c", "--quick"],
      ["--only", "tokenize", "missing", "--quick"],
      ["--only", "tokenize", "c", "--only", "tokenize", "c", "--quick"],
      ["--only", "tokenize", "c", "--checks-only", "--quick"],
      ["--quick", "--scale", "1"], ["--checks-only", "--checks-only"],
    ]) {
      const child = runHost(args);
      assert.equal(child.status, 2, JSON.stringify(args));
      assert.equal(child.stdout, "");
      assert.match(child.stderr, /unknown|duplicate|missing|invalid|mutually exclusive|not compiled/);
    }
  }
} finally {
  console.error = oldError;
  rmSync(temporary, { recursive: true, force: true });
}
console.log("benchmark apps reference, strict CLI and optional tool tests passed");
