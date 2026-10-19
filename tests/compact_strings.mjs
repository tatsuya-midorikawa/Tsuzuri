// PM03: the form matrix of the strings. tests/fixtures/compact_strings makes every kind of string
// (literals, concatenations, slices, joins, interpolations, parses, code units, Maybe and Result payloads, map
// keys) from ten texts in four ways each, and its exports fold the strings into one number. The numbers must
// be the same in `TSUZURI_STRINGS=utf16` (every string is UTF-16) and `compact` (a string is Latin-1 when its
// producer can tell), for wasm32 and native, at -O0 and -O3, and no allocation may be left behind (the counting
// allocator of each build says so after every call).
//
//   node tests/compact_strings.mjs [compiler] [--quick]
//
// TSUZURI_BASELINE=<compiler> also builds the fixture with that compiler (the one before PM03) in the default
// form for wasm32 and native -O0, and its numbers must be the same as well.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(process.argv.find((argument, index) => index > 1 && !argument.startsWith("--")) ?? join(root, "target/debug/tsuzuri"));
const baseline = process.env.TSUZURI_BASELINE ? resolve(process.env.TSUZURI_BASELINE) : null;
const clang = process.env.TSUZURI_CLANG ?? "clang";
const quick = process.argv.includes("--quick");
const fixture = join(root, "tests/fixtures/compact_strings");
const scratch = join(root, "target", `compact-strings-${process.pid}`);
mkdirSync(scratch, { recursive: true });
const environment = { ...process.env, TMPDIR: scratch, TSUZURI_CACHE_DIR: join(scratch, "cache") };
delete environment.TSUZURI_STRINGS;

function execute(program, args, { env = {}, allowFailure = false } = {}) {
  const result = spawnSync(program, args, {
    cwd: root, env: { ...environment, ...env }, encoding: "utf8", timeout: 900_000, maxBuffer: 256 * 1024 * 1024,
  });
  if (result.error) throw result.error;
  if (!allowFailure) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}

const source = readFileSync(join(fixture, "Main.tz"), "utf8");
const probes = [...source.matchAll(/^export def (probe_\w+) ::/gm)].map((match) => match[1]);
assert.ok(probes.length >= 12, `the fixture exports its probes: ${probes}`);
const span = quick ? 10 : 30;
const pairs = [];
for (let a = 0; a < span; ++a) for (let b = 0; b < span; ++b) pairs.push([a, b]);

// results.get(label) is a Map from `name a b` to the value (a decimal string, or "trap").
const results = new Map();

async function wasmResults(label, build) {
  const wasm = join(scratch, `${label}.wasm`);
  execute(build.compiler, ["build", fixture, "--target", "wasm32", `-O${build.optimization}`, ...(build.simd ? ["--wasm-feature", "simd128"] : []), "--allocator", "counting", "--no-cache", "-o", wasm], { env: build.env });
  const module = new WebAssembly.Module(readFileSync(wasm));
  assert.deepEqual(WebAssembly.Module.imports(module), [], `${label} has no imports`);
  const fresh = () => {
    const api = new WebAssembly.Instance(module).exports;
    return { api, scratch: api.tsuzuri_alloc(32n) };
  };
  let { api, scratch: record } = fresh();
  const values = new Map();
  for (const name of probes) {
    for (const [a, b] of pairs) {
      let value;
      try {
        value = String(api[`tz_${name}`](BigInt(a), BigInt(b)));
      } catch (error) {
        assert.ok(error instanceof WebAssembly.RuntimeError, `${label} ${name} ${a} ${b}: ${error}`);
        value = "trap";
        ({ api, scratch: record } = fresh());
      }
      values.set(`${name} ${a} ${b}`, value);
      api.tsuzuri_alloc_stats(record);
      const [allocations, frees, live] = new BigUint64Array(api.memory.buffer, record, 4);
      assert.equal(live, 32n, `${label}: ${name} ${a} ${b} leaves ${live - 32n} bytes`);
      assert.equal(allocations, frees + 1n, `${label}: ${name} ${a} ${b} allocations ${allocations} frees ${frees}`);
    }
  }
  results.set(label, values);
}

function nativeResults(label, build) {
  const header = join(scratch, `${label}.h`);
  const object = join(scratch, `${label}.o`);
  execute(build.compiler, ["build", fixture, "--emit", "header", "--allocator", "counting", "-o", header], { env: build.env });
  execute(build.compiler, ["build", fixture, "--emit", "object", "--allocator", "counting", `-O${build.optimization}`, "--no-cache", "-o", object], { env: build.env });
  const host = join(scratch, `${label}.c`);
  const calls = probes.map((name) => `    for (size_t i = 0; i < sizeof pairs / sizeof pairs[0]; ++i) {
        int64_t value = tz_${name}(pairs[i][0], pairs[i][1]);
        tsuzuri_alloc_stats(&stats);
        if (stats.live_bytes != 0 || stats.allocations != stats.frees) { printf("LEAK ${name} %lld %lld live=%llu\\n", (long long)pairs[i][0], (long long)pairs[i][1], (unsigned long long)stats.live_bytes); return 1; }
        printf("${name} %lld %lld %lld\\n", (long long)pairs[i][0], (long long)pairs[i][1], (long long)value);
    }`).join("\n");
  writeFileSync(host, `#include <stdint.h>
#include <stdio.h>
#include "${label}.h"
static const int64_t pairs[][2] = {${pairs.map(([a, b]) => `{${a},${b}}`).join(",")}};
int main(void) {
    tsuzuri_allocation_stats stats;
${calls}
    return 0;
}
`);
  const binary = join(scratch, label);
  execute(clang, ["-std=c11", "-O1", `-I${scratch}`, host, object, "-lm", "-o", binary]);
  const output = execute(binary, []).stdout;
  const values = new Map();
  for (const line of output.trim().split("\n")) {
    const [name, a, b, value] = line.split(" ");
    values.set(`${name} ${a} ${b}`, value);
  }
  results.set(label, values);
}

// The longest string of a WebAssembly module (tests/fixtures/compact_strings_limit). The heap of a wasm32 module refuses a
// block above its limit, 16,777,184 bytes by default, and a Latin-1 string is refused at the length that the UTF-16 string
// is, so both forms trap at the same calls and return the same values below them. --wasm-max-memory moves both limits.
function limits() {
  const project = join(root, "tests/fixtures/compact_strings_limit");
  const exportsOf = (form, ...options) => {
    const wasm = join(scratch, `limit-${form}-${options.join("")}.wasm`);
    execute(compiler, ["build", project, "--target", "wasm32", "-O0", "--no-cache", ...options, "-o", wasm], { env: { TSUZURI_STRINGS: form } });
    const module = new WebAssembly.Module(readFileSync(wasm));
    return () => new WebAssembly.Instance(module).exports;
  };
  const outcome = (fresh, name, n) => {
    try {
      return String(fresh()[`tz_${name}`](BigInt(n)));
    } catch (error) {
      assert.ok(error instanceof WebAssembly.RuntimeError, `${name}(${n}): ${error}`);
      return "trap";
    }
  };
  let compared = 0;
  // doubled and joined need twice their length in memory, so they are refused at 4,500,000; grown at 9,000,000 units.
  const sizes = { doubled: [0, 1000, 1_000_000, 4_500_000], joined: [0, 1000, 1_000_000, 4_500_000], grown: [0, 1000, 1_000_000, 9_000_000] };
  for (const [options, refused] of [[[], true], [["--wasm-max-memory", "64MiB"], false]]) {
    const fresh = { utf16: exportsOf("utf16", ...options), compact: exportsOf("compact", ...options) };
    for (const [name, lengths] of Object.entries(sizes)) {
      for (const n of lengths) {
        const utf16 = outcome(fresh.utf16, name, n);
        const compact = outcome(fresh.compact, name, n);
        assert.equal(compact, utf16, `${name}(${n}) ${options.join(" ")}: both forms give the same`);
        const expected = n === lengths.at(-1) && refused ? "trap" : String(name === "grown" ? n : 2 * n);
        assert.equal(utf16, expected, `${name}(${n}) ${options.join(" ")}`);
        compared += 1;
      }
    }
  }
  console.log(`compact strings limits: ${compared} calls trap or return the same in both forms at the default and a 64 MiB heap`);
}

// A host import that takes a callback is lowered at its call site (tests/fixtures/compact_strings_import), and the
// compact form widens its `ref string` arguments for the host there. A copy must be gone when an argument that follows it
// raises inside a `try`, as when the call ends: the host sees the same units in both forms, and no allocation is left.
function imports() {
  const project = join(root, "tests/fixtures/compact_strings_import");
  // 21 = triple (5 + 2), 664 and 52497 = the units of "héllo" and "日本"; the overflow of n + 1 is caught as -1.
  const calls = [["applied", 5n, 53187n], ["caught", 5n, 53188n], ["caught", 9223372036854775807n, -1n], ["applied", 0n, 53182n], ["caught", -3n, 53180n]];
  const units = (memory, pointer, length) => new Uint16Array(memory.buffer, pointer, Number(length)).reduce((sum, unit) => sum + BigInt(unit), 0n);
  const header = join(scratch, "import.h");
  execute(compiler, ["build", project, "--emit", "header", "--allocator", "counting", "-o", header]);
  const host = join(scratch, "import.c");
  writeFileSync(host, `#include <stdint.h>
#include <stdio.h>
#include "import.h"
int64_t pm03_apply(int64_t (*callback)(int64_t), const uint16_t *first, int64_t first_length, const uint16_t *second, int64_t second_length, int64_t extra) {
    int64_t total = callback(first_length + second_length) + extra;
    for (int64_t i = 0; i < first_length; ++i) total += first[i];
    for (int64_t i = 0; i < second_length; ++i) total += second[i];
    return total;
}
static const struct { const char *name; int64_t (*run)(int64_t); int64_t argument; } calls[] = {${calls.map(([name, argument]) => `{"${name}", tz_${name}, ${argument}LL}`).join(", ")}};
int main(void) {
    tsuzuri_allocation_stats stats;
    for (size_t i = 0; i < sizeof calls / sizeof calls[0]; ++i) {
        int64_t value = calls[i].run(calls[i].argument);
        tsuzuri_alloc_stats(&stats);
        if (stats.live_bytes != 0 || stats.allocations != stats.frees) { printf("LEAK %s %lld live=%llu\\n", calls[i].name, (long long)calls[i].argument, (unsigned long long)stats.live_bytes); return 1; }
        printf("%s %lld %lld\\n", calls[i].name, (long long)calls[i].argument, (long long)value);
    }
    return 0;
}
`);
  const expected = `${calls.map(([name, argument, value]) => `${name} ${argument} ${value}`).join("\n")}\n`;
  let builds = 0;
  for (const form of ["utf16", "compact"]) {
    for (const optimization of [0, 3]) {
      const label = `${form}-O${optimization}`;
      const env = { TSUZURI_STRINGS: form };
      const object = join(scratch, `import-${label}.o`);
      execute(compiler, ["build", project, "--emit", "object", "--allocator", "counting", `-O${optimization}`, "--no-cache", "-o", object], { env });
      const binary = join(scratch, `import-${label}`);
      execute(clang, ["-std=c11", "-O1", `-I${scratch}`, host, object, "-lm", "-o", binary]);
      assert.equal(execute(binary, []).stdout, expected, `native ${label}`);

      const wasm = join(scratch, `import-${label}.wasm`);
      execute(compiler, ["build", project, "--target", "wasm32", `-O${optimization}`, "--allocator", "counting", "--no-cache", "-o", wasm], { env });
      const module = new WebAssembly.Module(readFileSync(wasm));
      assert.deepEqual(WebAssembly.Module.imports(module).map(({ module, name }) => `${module}.${name}`), ["tsuzuri.pm03_apply"], `wasm ${label}`);
      let api;
      const imported = { tsuzuri: { pm03_apply: (callback, first, firstLength, second, secondLength, extra) =>
        api.__indirect_function_table.get(callback)(firstLength + secondLength) + units(api.memory, first, firstLength) + units(api.memory, second, secondLength) + extra } };
      api = new WebAssembly.Instance(module, imported).exports;
      const record = api.tsuzuri_alloc(32n);
      for (const [name, argument, value] of calls) {
        assert.equal(api[`tz_${name}`](argument), value, `wasm ${label}: ${name} ${argument}`);
        api.tsuzuri_alloc_stats(record);
        const [allocations, frees, live] = new BigUint64Array(api.memory.buffer, record, 4);
        assert.equal(live, 32n, `wasm ${label}: ${name} ${argument} leaves ${live - 32n} bytes`);
        assert.equal(allocations, frees + 1n, `wasm ${label}: ${name} ${argument} allocations ${allocations} frees ${frees}`);
      }
      builds += 2;
    }
  }
  console.log(`compact strings imports: ${builds} builds give the host the same units, and a caught overflow leaves no widened copy`);
}

try {
  // The sections that need no matrix come first: they are short and fail fast.
  limits();
  imports();
  const builds = [];
  for (const form of ["utf16", "compact"]) {
    for (const optimization of [0, 3]) {
      builds.push({ label: `${form}-O${optimization}`, compiler, optimization, env: { TSUZURI_STRINGS: form } });
    }
  }
  if (baseline) builds.push({ label: "baseline-O0", compiler: baseline, optimization: 0, env: {} });
  for (const build of builds) {
    await wasmResults(`wasm-${build.label}`, build);
    nativeResults(`native-${build.label}`, build);
  }
  // WebAssembly with simd128 selects the vector kernels (native always has them).
  for (const form of ["utf16", "compact"]) {
    for (const optimization of [0, 3]) {
      await wasmResults(`wasm-simd128-${form}-O${optimization}`, { compiler, optimization, simd: true, env: { TSUZURI_STRINGS: form } });
    }
  }
  const reference = results.get("wasm-utf16-O0");
  assert.equal(reference.size, probes.length * pairs.length);
  for (const [label, values] of results) {
    assert.equal(values.size, reference.size, label);
    for (const [key, expected] of reference) {
      assert.equal(values.get(key), expected, `${label}: ${key}`);
    }
  }
  const traps = [...reference.values()].filter((value) => value === "trap").length;
  const distinct = new Set(reference.values()).size;
  assert.ok(distinct > reference.size / 4, `the matrix must tell its cases apart: ${distinct} values in ${reference.size}`);
  console.log(`compact strings matrix: ${probes.length} probes x ${pairs.length} pairs, ${results.size} builds (${[...results.keys()].join(", ")}) agree; ${traps} traps; ${distinct} distinct values; no allocation left`);
} finally {
  rmSync(scratch, { recursive: true, force: true });
}
