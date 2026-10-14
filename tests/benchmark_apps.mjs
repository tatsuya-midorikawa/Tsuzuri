import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { hex, reference, checkReference, workloads } from "../benchmarks/apps/reference.mjs";
import { assertStrictFloatIR, optionalTool, parseArguments, zigReferenceBuild } from "../benchmarks/run-apps.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
for (const cpu of ["generic", "native"]) for (const system of ["linux", "darwin", "win32"]) {
  const build = zigReferenceBuild("reference.zig", "artifacts", cpu, system);
  const archive = system === "linux";
  assert.deepEqual(build.args.slice(0, 2), [archive ? "build-lib" : "build-obj", "reference.zig"]);
  assert.equal(build.binary, archive ? "libapps_zig.a" : "zig.o");
  assert.equal(build.flags.includes("-static"), archive);
  assert.ok(build.flags.includes("-fcompiler-rt"));
  assert.ok(build.flags.includes("ReleaseSafe") && build.flags.includes("-fPIC") && build.flags.includes("-fllvm"));
  assert.ok(build.flags.includes(`-mcpu=${cpu === "native" ? "native" : "baseline"}`));
  assert.ok(!build.flags.some((flag) => flag === "-fno-stack-check" || flag === "-fno-stack-protector"));
  assert.deepEqual(build.args.slice(-3), [
    `-femit-bin=${join("artifacts", build.binary)}`,
    `-femit-asm=${join("artifacts", "zig.s")}`,
    `-femit-llvm-ir=${join("artifacts", "zig.optimized.ll")}`,
  ]);
}
const coefficientIR = (operation, conversion = "%den = uitofp i64 %n to double") =>
  `define double @coefficient(i64 %n, double %input) {\n  ${conversion}\n  %value = ${operation}\n  ret double %value\n}\n`;
assertStrictFloatIR(coefficientIR("fdiv double 1.000000e+00, %den"), "strict division");
assertStrictFloatIR(coefficientIR("fdiv nnan double 1.000000e+00, %den"), "LLVM23 inferred nnan");
assertStrictFloatIR(coefficientIR("fdiv nnan double 1.000000e+00, %den, !dbg !7"), "inferred nnan with metadata");
assertStrictFloatIR(coefficientIR("fdiv nnan double 1.000000e+00, %den", "%den = uitofp i64 0 to double"), "infinity is not NaN");
for (const flag of ["fast", "reassoc", "ninf", "nsz", "arcp", "contract", "afn"]) {
  for (const flags of [flag, `nnan ${flag}`, `${flag} nnan`]) {
    assert.throws(() => assertStrictFloatIR(coefficientIR(`fdiv ${flags} double 1.000000e+00, %den`), "unsafe flags"), /strict f64/);
  }
}
for (const operation of ["fadd", "fsub", "fmul"]) {
  assert.throws(() => assertStrictFloatIR(coefficientIR(`${operation} nnan double 1.000000e+00, %den`), "reduction flags"), /strict f64/);
}
assert.throws(() => assertStrictFloatIR(coefficientIR("fdiv nnan double 0.000000e+00, %den"), "zero over zero"), /strict f64/);
assert.throws(() => assertStrictFloatIR(coefficientIR("fdiv nnan double 1.000000e+00, %den", "%den = fadd double %input, 0.000000e+00"), "NaN-capable divisor"), /strict f64/);
assert.throws(() => assertStrictFloatIR(coefficientIR("fdiv nnan double 1.000000e+00, %den", "%den = sitofp i64 %n to double"), "unproved conversion"), /strict f64/);
assert.throws(() => assertStrictFloatIR(
  coefficientIR("fdiv nnan double 1.000000e+00, %den")
    + "define double @unproved(double %den) {\n  %value = fdiv nnan double 1.000000e+00, %den\n  ret double %value\n}\n",
  "function-local SSA proof"), /strict f64/);
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
console.log("benchmark apps strict FP, reference, CLI and optional tool tests passed");
