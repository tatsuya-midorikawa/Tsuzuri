import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { platform, tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { publishSummary, renderSummary, renderTable, updateSummary } from "../benchmarks/report.mjs";
import { allocatorVariants, artifactFingerprint, captureRun, compareRuns, instrumentAllocations, makeRecord, measureProcess, median, parseCompilerDiagnostics, parseTimeOutput,
  parseTimePasses, readRecords, renderComparison, renderMetricsSummary, summarize, updateMetricsSummary,
  unsupportedReason, validateGnuTimeVersion, validateRecord, writeRecords } from "../benchmarks/metrics.mjs";
import { BUILD_LANGUAGES, FUNCTIONS_PER_MODULE, expectedOutput, generateProject, mainSource, moduleSource,
  writeProject } from "../benchmarks/build/generate.mjs";
import { breakdown, skipLargeWasm, VARIANTS } from "../benchmarks/run-build.mjs";
import { parseSections, sectionSizes } from "../benchmarks/size.mjs";
import { assemblyLines, difference } from "../benchmarks/assembly.mjs";

const result = {
  mode: "benchmark", cpu: "generic", scale: 0.1, measured_at: "2026-09-26T00:00:00.000Z",
  environment: { native: { control: { cpu: "Test <CPU>", architecture: "arm64", compiler_sha256: "abc" } }, csharp: { runtime: ".NET 10" }, javascript: { node: "v24" } },
  workloads: [{ family: "control", name: "example", size: 10, checks: 25, variants: Object.fromEntries([
    ["tsuzuri", 2], ["cpp", 1], ["rust", 1], ["csharp", 3], ["javascript", 4],
  ].map(([name, median_wall_ms]) => [name, { median_wall_ms }])) }],
};
const original = "# 性能測定\n\n## 表の読み方\n\n説明\n\n## 記録\n\n過去の結果\n";
const updated = updateSummary(original, [result]);
assert.ok(updated.indexOf("## ベンチマーク サマリー") > updated.indexOf("## 表の読み方"));
assert.ok(updated.indexOf("## ベンチマーク サマリー") < updated.indexOf("## 記録"));
assert.ok(updated.endsWith("## 記録\n\n過去の結果\n"));
assert.equal((updated.match(/data-fastest="true"/g) ?? []).length, 2);
assert.match(updated, /C\+\+ \/ Rust（同率）/);
assert.match(updated, /Tsuzuri 相対性能/);
assert.match(updated, /最速 = 1\.00000/);
assert.match(updated, /data-tsuzuri-performance="0\.5"/);
assert.match(updated, />\s+0\.50000<\/code>/);
assert.match(updated, /Test &lt;CPU&gt;/);
assert.match(updated, /font-variant-numeric:tabular-nums/);
assert.match(updated, /<strong><code style=/);
assert.ok(updated.indexOf("Tsuzuri (ms)") < updated.indexOf("C++ (ms)"));
assert.equal(updateSummary(updated, [result]).trim(), updated.trim());
assert.throws(() => renderSummary([{ ...result, mode: "correctness-smoke" }]));
assert.throws(() => renderSummary([result, { ...result, cpu: "native" }]));
const invalid = structuredClone(result);
invalid.workloads[0].variants.tsuzuri.median_wall_ms = NaN;
assert.throws(() => renderSummary([invalid]));
const owned = structuredClone(result);
owned.workloads[0].name = "std_option_owned";
assert.match(renderSummary([owned]), /data-fastest/);
assert.match(renderSummary([owned]), /data-tsuzuri-performance=/);
const fastest = structuredClone(result);
fastest.workloads[0].variants.tsuzuri.median_wall_ms = 0.5;
assert.match(renderSummary([fastest]), /data-tsuzuri-performance="1"/);
assert.match(renderSummary([fastest]), />\s+1\.00000<\/code>/);
fastest.workloads[0].variants.tsuzuri.median_wall_ms = 1;
assert.match(renderSummary([fastest]), /data-tsuzuri-performance="1"/);
fastest.workloads[0].variants.tsuzuri.median_wall_ms = 1000000;
assert.match(renderSummary([fastest]), /&lt;0\.00001/);
assert.doesNotMatch(renderTable(["種目", "Tsuzuri (ms)", "C++ (ms)"], [["zero", "1.000000", "0.000000"]], { timingColumns: [1, 2] }), /data-tsuzuri-performance=/);
assert.doesNotMatch(renderTable(["種目", "Tsuzuri (ms)", "C++ (ms)"], [["missing", "-", "1.000000"]], { timingColumns: [1, 2] }), /data-tsuzuri-performance=/);
const tiny = structuredClone(result);
tiny.workloads[0].variants.tsuzuri.median_wall_ms = 0.000000001;
assert.match(renderSummary([tiny]), /1\.000e-9/);
assert.match(renderTable(["名前", "時間 (ms)"], [["<script>", "0.100000"]], { timingColumns: [1] }), /&lt;script&gt;/);
const second = structuredClone(result);
second.measured_at = "2026-09-26T01:00:00.000Z";
second.workloads[0].variants.tsuzuri.median_wall_ms = 6;
assert.match(renderSummary([result, second]), /4\.000000/);
assert.match(renderSummary([result, second]), /2026-09-26T01:00:00.000Z/);
assert.throws(() => renderSummary([result, { ...result, scale: 1 }]));
assert.throws(() => updateSummary(updated.replace("<!-- benchmark-summary:end -->", ""), [result]));
const temporary = mkdtempSync(join(tmpdir(), "tsuzuri-report-test-"));
try {
  const path = join(temporary, "report.md");
  writeFileSync(path, original);
  assert.throws(() => publishSummary([invalid], path));
  assert.equal(readFileSync(path, "utf8"), original);
  publishSummary([result], path);
  assert.equal(readFileSync(path, "utf8"), updated);
  publishSummary([result], path);
  assert.equal(readFileSync(path, "utf8"), updated);
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
console.log("Benchmark report: placement, ranking/ties, units, escaping, idempotence and invalid-result guards passed");

const test = (name, check) => { check(); console.log(`Metrics: ${name} passed`); };
const context = {
  schema: 1, run_id: "px01-test", commit: "a".repeat(40), dirty: false, date: "2026-10-13T00:00:00.000Z", mode: "benchmark",
  host: { os: "darwin", os_version: "27.0", arch: "arm64", cpu: "Test <CPU>", cores: 10,
    clang: "Clang 21", rustc: "Rust 1.98", node: "v24", tsuzuri_sha256: "b".repeat(64) },
};
const record = (fields = {}, run = context) => makeRecord(run, {
  suite: "process", workload: "example", language: "tsuzuri", variant: "execute", opt: "O0",
  metric: "peak_rss", samples: Array(9).fill(1687552), ...fields,
});
test("median and summary", () => {
  assert.equal(median([3, 1, 2]), 2);
  assert.equal(median([4, 1, 3, 2]), 2.5);
  assert.deepEqual(summarize([5, 1, 3]), { samples: [5, 1, 3], median: 3, min: 1, max: 5 });
  for (const values of [[], [NaN], [-1], [Infinity]]) assert.throws(() => summarize(values));
});
test("macOS time units", () => {
  const text = "  0.16 real 0.11 user 0.05 sys\n 43253760 maximum resident set size\n 12878280 peak memory footprint\n";
  assert.deepEqual(parseTimeOutput(text, "darwin"), { wall_ms: 160, user_ms: 110, sys_ms: 50, peak_rss_bytes: 43253760 });
  assert.throws(() => parseTimeOutput("0.16 real 0.11 user 0.05 sys", "darwin"), /unrecognized/);
  assert.throws(() => parseTimeOutput(text, "win32"));
});
test("Linux time units", () => {
  const text = "User time (seconds): 0.11\nSystem time (seconds): 0.05\nElapsed (wall clock) time (h:mm:ss or m:ss): 1:02:03.45\nMaximum resident set size (kbytes): 42240\n";
  assert.deepEqual(parseTimeOutput(text, "linux"), { wall_ms: 3723450, user_ms: 110, sys_ms: 50, peak_rss_bytes: 43253760 });
  assert.equal(parseTimeOutput(text.replace("1:02:03.45", "0:00.16"), "linux").wall_ms, 160);
  assert.throws(() => parseTimeOutput(text.replace("Maximum resident set size (kbytes): 42240\n", ""), "linux"), /unrecognized/);
});
test("GNU time versions and distro UNKNOWN banner", () => {
  for (const text of ["GNU time 1.8\n", "time (GNU Time) 1.9\n", "GNU time 2.0.1\n",
    "time (GNU Time) UNKNOWN\nCopyright (C) 2018 Free Software Foundation, Inc.\n"]) validateGnuTimeVersion(text);
  for (const text of ["GNU time 1.7\n", "GNU time 0.9\n", "BusyBox time 1.9\n",
    "time (GNU Time)\nCopyright (C) 2018 Free Software Foundation, Inc.\n"]) {
    assert.throws(() => validateGnuTimeVersion(text), /unrecognized or unsupported version/);
  }
});
test("phase reports and non-overlap", () => {
  const report = '{"time_passes":1,"phases":[{"name":"parse","ms":1}],"total_ms":2}';
  assert.equal(parseTimePasses(`${report}\nother\n${report.replace('"ms":1', '"ms":0.5')}\n`).phases[0].ms, 0.5);
  for (const name of ["split", "units", "kg-capture", "kg-model", "kg-write", "kg-facts", "kg-brief", "kg-views", "kg-publish"]) {
    assert.equal(parseTimePasses(report.replace("parse", name)).phases[0].name, name);
  }
  assert.throws(() => parseTimePasses(report.replace("parse", "kg-other")));
  assert.throws(() => parseTimePasses("nothing"));
  assert.throws(() => parseTimePasses(report.replace('"time_passes":1', '"time_passes":2')));
  assert.throws(() => parseTimePasses(report.replace('"ms":1', '"ms":3')), /overlapping/);
  assert.throws(() => parseTimePasses(report.replace('"ms":1', '"ms":-1')));
});
test("tool phases preserve valid external executable names", () => {
  const names = ["tool.Compiler wrapper", "tool.コンパイラ", "tool.clang@21", "tool.compiler\nwrapper"];
  const report = (name) => JSON.stringify({ time_passes: 1, phases: [{ name, ms: 1 }], total_ms: 1 });
  for (const name of names) {
    assert.equal(parseTimePasses(report(name)).phases[0].name, name);
    assert.equal(record({ metric: "phase_time", phase: name, samples: Array(9).fill(1) }).phase, name);
  }
  for (const name of ["tool.", "tool.\u0000compiler"]) {
    assert.throws(() => parseTimePasses(report(name)));
    assert.throws(() => record({ metric: "phase_time", phase: name, samples: Array(9).fill(1) }));
  }
});
test("unsupported target reason survives preceding warnings", () => {
  assert.equal(unsupportedReason("Main.tz:1:1: warning[W1001]: unused\nMain.tz:2:1: error[E2004]: missing main\n"),
    "Main.tz:2:1: error[E2004]: missing main");
  assert.throws(() => unsupportedReason("warning[W1001]: unused"), /missing expected/);
});
test("JSON diagnostics coexist with opt-in timing metadata", () => {
  const diagnostic = { severity: "error", code: "E0002", message: "expected expression", path: "Main.tz", span: { start: 1, end: 2 } };
  const timing = '{"time_passes":1,"phases":[],"total_ms":1}';
  assert.deepEqual(parseCompilerDiagnostics(`${JSON.stringify(diagnostic)}\n${timing}\n`), [diagnostic]);
  assert.deepEqual(parseCompilerDiagnostics(JSON.stringify(diagnostic)), [diagnostic]);
  assert.throws(() => parseCompilerDiagnostics(timing.replace('"time_passes":1', '"time_passes":2')));
  assert.throws(() => parseCompilerDiagnostics('{"unexpected":1}'));
  assert.throws(() => parseCompilerDiagnostics("not JSON"));
});
test("fixed schema and record validation", () => {
  const valid = record();
  assert.deepEqual(Object.keys(valid), ["schema", "run_id", "commit", "dirty", "date", "mode", "host", "suite", "workload",
    "size", "language", "variant", "target", "opt", "cpu_mode", "metric", "phase", "unit", "samples", "median", "min", "max"]);
  for (const change of [
    { extra: true }, { host: { ...context.host, node: undefined } }, { commit: "a".repeat(39) },
    { date: "2026-10-13T00:00:00Z" }, { unit: "ms" }, { phase: "parse", metric: "wall_time", unit: "ms" },
    { phase: "link", metric: "phase_time", unit: "ms" }, { metric: "alloc_calls", unit: "count", samples: [1.5] },
    { metric: "wall_time", unit: "ms", samples: Array(8).fill(1) }, { metric: "alloc_bytes", samples: [1, 2] }, { median: 42 },
  ]) assert.throws(() => validateRecord({ ...valid, ...change }));
  assert.equal(record({ suite: "apps" }).suite, "apps");
  assert.equal(record({ suite: "build", language: "go" }).language, "go");
});
test("allocator variants: a run with another allocator never matches the default run", () => {
  assert.deepEqual(allocatorVariants("system"), { timing: "default", allocation: "default" });
  assert.deepEqual(allocatorVariants("small"), { timing: "small", allocation: "system" });
  assert.throws(() => allocatorVariants("pool"), /unknown allocator/);
  const timing = (variant, metric = "wall_time") => record({ suite: "control", variant, opt: "O3", metric, samples: Array(9).fill(1) });
  const verdicts = (before, after) => compareRuns([before], [after]).map((row) => row.verdict);
  assert.deepEqual(verdicts(timing("default"), timing(allocatorVariants("system").timing)), ["差なし"]);
  assert.deepEqual(verdicts(timing("default"), timing(allocatorVariants("small").timing)), ["片側のみ", "片側のみ"]);
  const peak = (variant) => record({ suite: "apps", variant, opt: "O3", samples: Array(9).fill(1687552) });
  assert.deepEqual(verdicts(peak("default"), peak(allocatorVariants("small").timing)), ["片側のみ", "片側のみ"]);
  const counts = (variant) => record({ suite: "apps", variant, opt: "O3", metric: "alloc_calls", samples: [7] });
  assert.deepEqual(verdicts(counts("default"), counts(allocatorVariants("small").allocation)), ["片側のみ", "片側のみ"]);
  assert.equal(counts(allocatorVariants("small").allocation).variant, "system");
  // The summary gives the small allocator a column of its own.
  const summary = renderMetricsSummary([timing("default"), record({ suite: "control", language: "c", variant: "default", opt: "O3", metric: "wall_time", samples: Array(9).fill(2) }),
    record({ suite: "control", variant: "small", opt: "O3", metric: "wall_time", samples: Array(9).fill(3) })]);
  assert.match(summary, /Tsuzuri small/);
});
test("assembly comparison keeps immediates, operands and order", () => {
  // `#` starts an immediate on AArch64 and a comment on x86; `;` and `//` are the AArch64 comments.
  assert.notDeepEqual(assemblyLines("mov w0, #1", "arm64"), assemblyLines("mov w0, #2", "arm64"));
  assert.deepEqual(assemblyLines("\tmov\tw0, #1 ; constant\n\n\tadd w1, w0, #4 // sum\n; %bb.1:\n", "arm64"), ["mov\tw0, #1", "add w1, w0, #4"]);
  assert.deepEqual(assemblyLines("\tcallq\tfoo@PLT # call\n\t.type\tfoo,@function\n# %bb.0:\n\tmovl\t$1, %eax\n", "x86_64"),
    ["callq\tfoo@PLT", ".type\tfoo,@function", "movl\t$1, %eax"]);
  assert.deepEqual(assemblyLines('\t.asciz\t"a#b;c" # comment\n', "x86_64"), ['.asciz\t"a#b;c"']);
  assert.deepEqual(assemblyLines('\t.asciz\t"say \\" # inside" ; outside\n', "arm64"), ['.asciz\t"say \\" # inside"']);
  assert.throws(() => assemblyLines("nop", "riscv"), /unknown assembly target/);
  // The same instructions in another order are a difference; the distance is that of an edit script.
  const swapped = [assemblyLines("mov x0, x1\nadd x1, x1, x2\n", "arm64"), assemblyLines("add x1, x1, x2\nmov x0, x1\n", "arm64")];
  assert.equal(difference(...swapped), 2);
  assert.equal(difference(["a", "b", "c"], ["a", "b", "c"]), 0);
  assert.equal(difference(["a", "b", "c"], ["a", "c"]), 1);
  assert.equal(difference([], ["a", "b"]), 2);
  const lcs = (a, b) => {
    const table = Array.from({ length: a.length + 1 }, () => new Int32Array(b.length + 1));
    for (let i = a.length - 1; i >= 0; --i) for (let j = b.length - 1; j >= 0; --j) {
      table[i][j] = a[i] === b[j] ? table[i + 1][j + 1] + 1 : Math.max(table[i + 1][j], table[i][j + 1]);
    }
    return table[0][0];
  };
  let state = 12345;
  const random = (bound) => (state = (state * 1103515245 + 12345) & 0x7fffffff) % bound;
  for (let round = 0; round < 2000; ++round) {
    const alphabet = 1 + random(5);
    const a = Array.from({ length: random(14) }, () => String(random(alphabet)));
    const b = Array.from({ length: random(14) }, () => String(random(alphabet)));
    assert.equal(difference(a, b), a.length + b.length - 2 * lcs(a, b), JSON.stringify([a, b]));
  }
});
test("size metrics and the size optimization levels", () => {
  for (const metric of ["executable_bytes", "object_bytes", "wasm_bytes", "text_bytes", "rodata_bytes", "wasm_code_bytes", "wasm_data_bytes"]) {
    for (const opt of ["O0", "O3", "Os", "Oz"]) assert.equal(record({ suite: "size", variant: "default", metric, opt, samples: [33768] }).median, 33768);
    // Sizes are deterministic: two different samples mean a nondeterministic build.
    assert.throws(() => record({ suite: "size", metric, samples: [1, 2] }), /nondeterministic/);
  }
  assert.throws(() => record({ suite: "size", metric: "text_bytes", opt: "O2", samples: [1] }));
  assert.equal(record({ suite: "size", language: "c", variant: "wasm-opt", target: "wasm32", opt: "Oz", metric: "wasm_bytes", samples: [2353] }).variant, "wasm-opt");
});
test("size suite reads the sections of Mach-O, ELF and WASM tables (llvm-objdump -h, hexadecimal)", () => {
  const machO = `hello:\tfile format mach-o arm64

Sections:
Idx Name          Size     VMA              Type
  0 __text        000033f8 0000000100000460 TEXT
  1 __stubs       00000054 0000000100003858 TEXT
  2 __const       000027b0 00000001000038b0 DATA
  3 __cstring     00000008 0000000100006060 DATA
  4 __unwind_info 00000080 0000000100006068 DATA
`;
  // 0x33f8 = 13,304 bytes of code; 0x27b0 + 0x08 = 10,168 bytes of constants and strings.
  assert.deepEqual(sectionSizes(machO, "hello"), { kind: "macho", code: 13304, data: 10168 });
  assert.equal(parseSections(machO).sections.get("__stubs"), 0x54);
  const wasm = `hello.wasm:\tfile format wasm

Sections:
Idx Name          Size     VMA      Type
  0 TYPE          00000063 0000000a
  8 CODE          000063dc 00000109 TEXT
  9 DATA          0000269a 000064e8 DATA
`;
  assert.deepEqual(sectionSizes(wasm, "hello.wasm"), { kind: "wasm", code: 25564, data: 9882 });
  // A module without a DATA section has no read-only data; one without CODE is not a module of ours.
  assert.deepEqual(sectionSizes(wasm.replace(/.*DATA.*\n/, ""), "point.wasm"), { kind: "wasm", code: 25564, data: 0 });
  assert.throws(() => sectionSizes(wasm.replace(/.*CODE.*\n/, ""), "empty.wasm"), /section CODE not found in empty\.wasm/);
  const elf = `a.out:\tfile format elf64-x86-64

Sections:
Idx Name            Size     VMA              Type
  0                 00000000 0000000000000000
  1 .rodata         00002a50 0000000000200000 DATA
  2 .text           00004080 0000000000202a50 TEXT
`;
  assert.deepEqual(sectionSizes(elf, "a.out"), { kind: "elf", code: 0x4080, data: 0x2a50 });
  assert.throws(() => sectionSizes(elf.replace(/.*\.text.*\n/, ""), "a.out"), /section \.text not found in a\.out/);
  assert.throws(() => sectionSizes("x:\tfile format unknown\n", "x"), /unrecognized object format/);
});
const metricTemporary = mkdtempSync(join(tmpdir(), "tsuzuri-metrics-test-"));
try {
  test("capture and compiler SHA-256", () => {
    const compiler = join(metricTemporary, "compiler");
    writeFileSync(compiler, "abc");
    const run = captureRun(join(metricTemporary, "px01-test"), { compiler, mode: "quick" });
    assert.equal(run.run_id, "px01-test");
    assert.match(run.commit, /^[0-9a-f]{40}$/);
    assert.equal(new Date(run.date).toISOString(), run.date);
    assert.equal(Object.keys(run.host).length, 9);
    assert.equal(run.host.tsuzuri_sha256, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    assert.throws(() => captureRun(join(metricTemporary, "bad name"), { compiler, mode: "quick" }));
  });
  test("JSONL roundtrip and exclusive creation", () => {
    const directory = join(metricTemporary, "px01-test");
    const first = record({ suite: "z-suite" }), second = record({ suite: "a-suite" });
    writeRecords(directory, "z-suite", [first]);
    writeRecords(directory, "a-suite", [second]);
    assert.deepEqual(readRecords(directory), [second, first]);
    assert.throws(() => writeRecords(directory, "z-suite", [first]), { code: "EEXIST" });
    assert.throws(() => writeRecords(directory, "wrong", [first]));
    assert.throws(() => writeRecords(directory, "z-suite", [first, first]), /duplicate/);
    writeFileSync(join(directory, "z-suite.jsonl"), JSON.stringify(first) + "\n" + JSON.stringify(first) + "\n");
    assert.throws(() => readRecords(directory), /duplicate/);
    writeFileSync(join(directory, "z-suite.jsonl"), JSON.stringify({ ...first, run_id: "another" }) + "\n");
    assert.throws(() => readRecords(directory), /mixed run_id/);
  });
  test("the size action runs through the CLI and needs llvm-objdump", () => {
    const script = fileURLToPath(new URL("../benchmarks/metrics.mjs", import.meta.url));
    const missing = spawnSync(process.execPath, [script, "size", join(metricTemporary, "compiler"), "--out", join(metricTemporary, "size-cli")], {
      encoding: "utf8", env: { ...process.env, TSUZURI_OBJDUMP: join(metricTemporary, "no-such-objdump") },
    });
    // Exit code 13 would be an unsettled top-level await: size.mjs imports this module.
    assert.equal(missing.status, 1, missing.stderr);
    assert.match(missing.stderr, /llvm-objdump/);
    const usage = spawnSync(process.execPath, [script, "size"], { encoding: "utf8" });
    assert.notEqual(usage.status, 0);
    assert.match(usage.stderr, /Usage: node benchmarks\/metrics\.mjs process\|size/);
  });
  test("allocator IR instrumentation", () => {
    const original = "declare ptr @malloc(i64) #1\ncall ptr @calloc(i64 4, i64 8)\ncall ptr @realloc(ptr null, i64 1)\ncall void @free(ptr null)\n@malloc_usable_size\nattributes #1 = { nounwind }\n";
    assert.equal(instrumentAllocations(original), "declare ptr @tracked_malloc(i64)\ncall ptr @tracked_calloc(i64 4, i64 8)\ncall ptr @tracked_realloc(ptr null, i64 1)\ncall void @tracked_free(ptr null)\n@malloc_usable_size\n");
    for (const allocator of ["strdup", "posix_memalign", "aligned_alloc"]) assert.throws(() => instrumentAllocations(`@${allocator}(ptr null)`), /unsupported allocator/);
  });
  test("artifact fingerprints preserve code and exclude only volatile Mach-O metadata", () => {
    const file = join(metricTemporary, "artifact");
    writeFileSync(file, "abc");
    assert.equal(artifactFingerprint(file), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    const macho = Buffer.alloc(80, 0);
    macho.writeUInt32LE(0xfeedfacf, 0);
    macho.writeUInt32LE(2, 12);
    macho.writeUInt32LE(2, 16);
    macho.writeUInt32LE(40, 20);
    macho.writeUInt32LE(0x1b, 32);
    macho.writeUInt32LE(24, 36);
    macho.writeUInt32LE(0x1d, 56);
    macho.writeUInt32LE(16, 60);
    macho.writeUInt32LE(76, 64);
    macho.writeUInt32LE(4, 68);
    writeFileSync(file, macho);
    const original = artifactFingerprint(file);
    macho.fill(42, 40, 56);
    macho.fill(24, 76, 80);
    writeFileSync(file, macho);
    assert.equal(artifactFingerprint(file), original);
    macho[72] = 1;
    writeFileSync(file, macho);
    assert.notEqual(artifactFingerprint(file), original);
    macho.writeUInt32LE(100, 68);
    writeFileSync(file, macho);
    assert.throws(() => artifactFingerprint(file), /code signature/);
  });
  test("COFF fingerprints exclude only the native compiler timestamp", () => {
    const file = join(metricTemporary, "coff-artifact");
    const coff = Buffer.alloc(80, 0);
    coff.writeUInt16LE(0x8664, 0);
    coff.writeUInt16LE(1, 2);
    coff.writeUInt32LE(0x6aceeee0, 4);
    coff.write(".text", 20, "ascii");
    coff.writeUInt32LE(20, 36);
    coff.writeUInt32LE(60, 40);
    writeFileSync(file, coff);
    const original = artifactFingerprint(file);
    coff.writeUInt32LE(0x6aceeee2, 4);
    writeFileSync(file, coff);
    assert.equal(artifactFingerprint(file), original);
    coff[60] = 1;
    writeFileSync(file, coff);
    assert.notEqual(artifactFingerprint(file), original);
    coff[60] = 0;
    coff.writeUInt16LE(0xaa64, 0);
    writeFileSync(file, coff);
    assert.notEqual(artifactFingerprint(file), original);
    coff.writeUInt16LE(2, 2);
    writeFileSync(file, coff);
    assert.throws(() => artifactFingerprint(file), /COFF section headers/);
  });
  test("the IR diff runner counts a diagnostic as a failure of the project and a missing compiler as a failure of the run", () => {
    if (platform() === "win32") return;
    const script = fileURLToPath(new URL("../benchmarks/layout/ir-diff.mjs", import.meta.url));
    const project = join(metricTemporary, "ir-diff-project");
    mkdirSync(project);
    writeFileSync(join(project, "Main.tz"), "1\n");
    const fake = (name, body) => {
      const file = join(metricTemporary, name);
      writeFileSync(file, `#!/bin/sh\n${body}\n`);
      chmodSync(file, 0o755);
      return file;
    };
    const diff = (before, after) => spawnSync(process.execPath, [script, before, after, "--out", join(metricTemporary, "ir-diff-out"), project], { encoding: "utf8" });
    const rejecting = fake("rejecting", 'echo "Main.tz:1:1: error[E2002]: no" >&2; exit 1');
    const alike = diff(rejecting, rejecting);
    assert.equal(alike.status, 0, alike.stderr);
    assert.match(alike.stdout, /both failed alike 1/);
    const other = diff(rejecting, fake("rejecting-other", 'echo "Main.tz:1:1: error[E2003]: no" >&2; exit 1'));
    assert.equal(other.status, 1);
    assert.match(other.stdout, /DIFF/);
    for (const [before, after] of [["/no/such/compiler", "/no/such/compiler"], [fake("crashing", 'echo "boom" >&2; exit 1'), fake("crashing-again", 'echo "boom" >&2; exit 1')]]) {
      const broken = diff(before, after);
      assert.notEqual(broken.status, 0, `${before}: ${broken.stdout}`);
      assert.doesNotMatch(broken.stdout, /both failed alike/);
    }
  });
  test("comparison verdicts and conditions", () => {
    const quick = { ...context, mode: "quick" };
    const time = (samples) => record({ metric: "wall_time", samples }, quick);
    const same = compareRuns([time([10, 11, 12])], [time([8, 9, 9])])[0];
    assert.equal(same.verdict, "差なし");
    assert.equal(same.spread, 2);
    assert.equal(same.ratio, 9 / 11);
    assert.equal(compareRuns([time([10, 10, 11])], [time([13, 13, 14])])[0].verdict, "悪化");
    const alloc = (samples) => record({ metric: "alloc_calls", samples }, quick);
    assert.equal(compareRuns([alloc([5])], [alloc([4])])[0].verdict, "改善");
    assert.equal(compareRuns([time([1])], [])[0].verdict, "片側のみ");
    const changed = { ...time([1]), host: { ...context.host, cpu: "Different" } };
    assert.equal(compareRuns([time([1])], [changed])[0].verdict, "条件差");
    assert.equal(compareRuns([time([1])], [{ ...time([1]), commit: "c".repeat(40), host: { ...context.host, tsuzuri_sha256: "d".repeat(64) } }])[0].verdict, "差なし");
    assert.equal(compareRuns([time([0])], [time([0])])[0].ratio, null);
  });
  test("comparison table", () => {
    const table = renderComparison(compareRuns([record()], []));
    assert.equal(table.split("\n")[0], "| 種目 | 言語 | variant | target | opt | 指標 | before 中央値 | after 中央値 | after/before | 広がり | 判定 |");
    assert.equal((table.split("\n")[1].match(/---/g) ?? []).length, 11);
    assert.match(table, /片側のみ/);
    assert.match(table, /1687552 \| - \| - \| -/);
  });
  test("summary metadata, units and escaping", () => {
    const summary = renderMetricsSummary([record({ workload: "<example>" }),
      record({ workload: "<example>", language: "c" })]);
    assert.match(summary, /peak_rss \(bytes\) \/ native \/ O0 \/ generic \/ 中央値/);
    assert.match(summary, /process\/&lt;example&gt;/);
    assert.ok(summary.indexOf("Tsuzuri execute") < summary.indexOf(">C execute<"));
    assert.doesNotMatch(summary, /data-fastest|data-tsuzuri-performance/);
    assert.match(renderMetricsSummary([record({ metric: "wall_time", samples: Array(9).fill(1.25) })]), /1\.250/);
  });
  test("summary markers, publication guards and idempotence", () => {
    const document = "# Test\n<!-- perf-metrics:start -->\n<!-- perf-metrics:end -->\nlast\n";
    const updated = updateMetricsSummary(document, [[record()]]);
    assert.equal(updateMetricsSummary(updated, [[record()]]), updated);
    assert.ok(updated.endsWith("last\n"));
    assert.throws(() => updateMetricsSummary("# Test", [[record()]]), /add the perf-metrics markers/);
    assert.throws(() => updateMetricsSummary(document, [[record({}, { ...context, mode: "quick" })]]), /must not be published/);
    assert.throws(() => updateMetricsSummary(document, [[record(), record({}, { ...context, host: { ...context.host, cpu: "Different" } })]]));
  });
  if (["darwin", "linux"].includes(platform()) && existsSync("/usr/bin/time")) test("process measurement, RSS and failures", () => {
    const measured = measureProcess(process.execPath, ["-e", "const b = Buffer.alloc(64 * 1024 * 1024, 1); console.log(b[12345])"], { runs: 2, warmups: 1 });
    for (const values of Object.values(measured)) assert.equal(values.length, 2);
    assert.deepEqual(measured.stdout, ["1\n", "1\n"]);
    assert.ok(measured.peak_rss_bytes.every((rss) => rss >= 67108864));
    assert.throws(() => measureProcess("/bin/sh", ["-c", "echo err >&2; exit 3"], { runs: 1, warmups: 0 }), /exited with 3:\nerr/);
    assert.throws(() => measureProcess(process.execPath, [], { system: "win32" }));
    const pidFile = join(metricTemporary, "child.pid");
    const code = `const{spawn}=require("node:child_process");const{writeFileSync}=require("node:fs");const child=spawn(process.execPath,["-e","process.on('SIGTERM',()=>{});setInterval(()=>{},1000)"],{stdio:"ignore"});writeFileSync(${JSON.stringify(pidFile)},String(child.pid));process.on('SIGTERM',()=>{});setInterval(()=>{},1000);`;
    assert.throws(() => measureProcess(process.execPath, ["-e", code], { runs: 1, warmups: 0, timeoutMs: 1500 }), /ETIMEDOUT/);
    const pid = Number(readFileSync(pidFile, "utf8"));
    let state;
    for (let attempt = 0; attempt < 20; attempt++) {
      state = spawnSync("ps", ["-p", String(pid), "-o", "stat="], { encoding: "utf8" });
      if (state.status === 1 || /^Z/.test(state.stdout.trim())) break;
      Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 50);
    }
    assert.ok(state.status === 1 || /^Z/.test(state.stdout.trim()), `timed-out child ${pid} is still running: ${state.stdout}`);
  }); else console.log("skip measureProcess: macOS/Linux /usr/bin/time not found");
  test("tracked malloc/calloc/realloc/free and overflow", () => {
    const source = join(metricTemporary, "host.c"), binary = join(metricTemporary, "host");
    writeFileSync(source, `#undef NDEBUG
#include <assert.h>
#include <stdint.h>
#include <stddef.h>
#include <string.h>
void *tracked_malloc(uint64_t);
void *tracked_calloc(uint64_t,uint64_t);
void *tracked_realloc(void *,uint64_t);
void tracked_free(void *);
void tracked_reset(void);
void tracked_read(uint64_t *,uint64_t *,uint64_t *);
int main(int argc,char **argv) {
  if(argc==2 && strcmp(argv[1],"overflow")==0) { tracked_calloc(UINT64_MAX,2); return 1; }
  if(argc==2 && strcmp(argv[1],"malloc-overflow")==0) { tracked_malloc(UINT64_MAX); return 1; }
  void *p=tracked_malloc(10); unsigned char *q=tracked_calloc(4,8);
  for(int i=0;i<32;i++) assert(q[i]==0);
  p=tracked_realloc(p,100); void *r=tracked_realloc(NULL,16);
  assert(tracked_realloc(r,0)==NULL);
  tracked_free(p); tracked_free(q); tracked_free(NULL);
  uint64_t calls,bytes,live; tracked_read(&calls,&bytes,&live);
  assert(calls==4 && bytes==158 && live==0);
  tracked_reset(); tracked_read(&calls,&bytes,&live);
  assert(calls==0 && bytes==0 && live==0);
  return 0;
}\n`);
    execFileSync(process.env.TSUZURI_CLANG ?? "clang", ["-std=c11", "-O2", "-Wall", "-Wextra", "-Werror",
      source, new URL("../benchmarks/tracked_alloc.c", import.meta.url).pathname, "-o", binary]);
    execFileSync(binary);
    for (const argument of ["overflow", "malloc-overflow"]) {
      const failed = spawnSync(binary, [argument], { encoding: "utf8" });
      assert.ifError(failed.error);
      assert.ok(failed.signal || failed.status !== 0);
      assert.match(failed.stderr, /size overflow/);
    }
  });
  test("synthetic generator determinism and structure", () => {
    assert.equal(FUNCTIONS_PER_MODULE, 20);
    assert.deepEqual(generateProject(3), generateProject(3));
    assert.deepEqual(generateProject(3).map(([name]) => name), ["Main.tz", "Module0.tz", "Module1.tz", "Module2.tz"]);
    const lines = [];
    for (let index = 0; index < 3; index++) {
      const source = moduleSource(index);
      for (let fn = 0; fn < 20; fn++) assert.equal(source.split(`private def f${fn} :: i64u -> i64u`).length, 2);
      assert.equal(source.split("def run :: i64u -> i64u").length, 2);
      assert.doesNotMatch(source, /\t| +\n/);
      assert.ok(source.endsWith("\n"));
      lines.push(source.split("\n").length);
    }
    assert.equal(new Set(lines).size, 1);
    assert.match(moduleSource(1), /"m1-"/);
    for (const language of BUILD_LANGUAGES) assert.deepEqual(generateProject(2, { language }), generateProject(2, { language }));
  });
  test("synthetic one-module body and interface edits", () => {
    const before = moduleSource(1).split("\n"), after = moduleSource(1, { salt: 5 }).split("\n");
    assert.equal(before.length, after.length);
    assert.equal(before.filter((line, index) => line !== after[index]).length, 1);
    assert.equal(moduleSource(1, { extra: 7 }), moduleSource(1) + "def extra_7 :: i64u -> i64u = \\x -> x + 7i64u\n");
    assert.notEqual(expectedOutput(3, "12345", { index: 1, salt: 5 }), expectedOutput(3, "12345"));
  });
  test("synthetic main input and module order", () => {
    const source = mainSource(3);
    assert.match(source, /let! line = IO\.read_line/);
    for (let index = 0; index < 3; index++) assert.equal(source.split(`Module${index}.run seed`).length, 2);
    assert.ok(source.indexOf("Module0.run seed") < source.indexOf("Module1.run seed"));
    assert.ok(source.indexOf("Module1.run seed") < source.indexOf("Module2.run seed"));
  });
  test("synthetic generator bounds and exclusive directory creation", () => {
    for (const size of [0, 5001, 1.5, NaN]) assert.throws(() => generateProject(size));
    assert.throws(() => generateProject(2, { language: "unknown" }));
    const directory = join(metricTemporary, "generated");
    const manifest = writeProject(directory, 3);
    const sources = generateProject(3);
    assert.equal(manifest.lines, sources.reduce((sum, [, text]) => sum + text.split("\n").length - 1, 0));
    assert.equal(manifest.bytes, sources.reduce((sum, [, text]) => sum + Buffer.byteLength(text), 0));
    assert.equal(manifest.files, 4);
    assert.throws(() => writeProject(directory, 3), { code: "EEXIST" });
    const full = generateProject(1000);
    const lines = full.reduce((sum, [, text]) => sum + text.split("\n").length - 1, 0);
    assert.ok(lines >= 130000 && lines <= 140000, `expected 130K..140K lines, got ${lines}`);
  });
  test("synthetic independent reference values", () => {
    for (const line of ["", "12345", "綴り😀"]) {
      const output = expectedOutput(2, line);
      assert.match(output, /^\d+\n$/);
      assert.ok(BigInt(output.trim()) < 1n << 64n);
    }
    assert.notEqual(expectedOutput(2, ""), expectedOutput(2, "12345"));
    assert.equal(expectedOutput(2, "abcd"), expectedOutput(2, "綴り😀"));
  });
  test("compiler/build phase breakdown", () => {
    assert.deepEqual(breakdown({ phases: [{ name: "load", ms: 1 }, { name: "parse", ms: 2 }, { name: "cache", ms: 3 },
      { name: "tool.clang", ms: 4 }], total_ms: 12 }), { frontend: 3, cache: 3, tools: 4, other: 2 });
    // The codegen units (PB07): the cut counts with the frontend, the unit cache with the cache.
    assert.deepEqual(breakdown({ phases: [{ name: "emit", ms: 2 }, { name: "split", ms: 1 }, { name: "units", ms: 3 },
      { name: "tool.clang", ms: 4 }], total_ms: 11 }), { frontend: 3, cache: 3, tools: 4, other: 1 });
    assert.throws(() => breakdown({ phases: [{ name: "check", ms: 5 }], total_ms: 1 }), /overlapping/);
    assert.deepEqual(breakdown({ phases: [{ name: "parse", ms: 0.001 }], total_ms: 0 }), { frontend: 0.001, cache: 0, tools: 0, other: 0 });
  });
  test("large WASM guard includes runtime cache variants", () => {
    for (const [, target, opt] of VARIANTS.filter(([, target, opt]) => target === "wasm32" && opt === "O3")) {
      for (const modules of [201, 1000]) assert.equal(skipLargeWasm(modules, target, opt, false), true);
      for (const modules of [null, 10, 200]) assert.equal(skipLargeWasm(modules, target, opt, false), false);
      assert.equal(skipLargeWasm(1000, target, opt, true), false);
    }
    assert.equal(skipLargeWasm(1000, "native", "O3", false), false);
    assert.equal(skipLargeWasm(1000, "wasm32", "O0", false), false);
  });
} finally {
  rmSync(metricTemporary, { recursive: true, force: true });
}