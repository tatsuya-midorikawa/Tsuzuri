// F08: CPU kernels for the integer Array.sum, Array.min, and Array.max, end to end.
// Usage: node tests/cpu_kernels.mjs target/release/tsuzuri
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(process.argv[2] ?? join(root, "target/release/tsuzuri"));
const clang = process.env.TSUZURI_CLANG ?? "clang";
const fixture = join(root, "tests/fixtures/cpu_kernels");

function execute(program, args, env = process.env, success = true) {
  const result = spawnSync(program, args, { cwd: root, encoding: "utf8", env, timeout: 300_000, maxBuffer: 64 * 1024 * 1024 });
  assert.ifError(result.error);
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}

// The integer types as (name, bits, signed).
const types = [["i8", 8, true], ["i16", 16, true], ["i32", 32, true], ["i64", 64, true], ["i8u", 8, false], ["i16u", 16, false], ["i32u", 32, false], ["i64u", 64, false]];
const patterns = [0, 1, 2, 3, 4];
const lengths = [...Array.from({ length: 71 }, (_, index) => index), 127, 128, 129, 255, 256, 257, 4095, 4096, 4097];
const wrap = (value) => BigInt.asIntN(64, value);

// The same data as the fixture's `data` (4,100 elements).
function data(pattern, bits, signed) {
  let state = 1n;
  const values = [];
  for (let k = 0n; k < 4100n; k++) {
    state = wrap(state * 6364136223846793005n + 1442695040888963407n);
    const unsigned = BigInt.asUintN(64, state);
    const top = 1n << BigInt(bits - 1);
    const raw = [unsigned >> BigInt(64 - bits), signed ? top : 0n, signed ? top - 1n : (1n << BigInt(bits)) - 1n, unsigned >> 62n, k][pattern];
    values.push(signed ? BigInt.asIntN(bits, raw) : BigInt.asUintN(bits, raw));
  }
  return values;
}

// The checksum of `measure` over the slices of the fixture's `checksum`.
function checksum(values, measure) {
  let total = 0n;
  for (let offset = 0; offset < 4; offset++) {
    for (const length of lengths) total = wrap(total * 1000003n + measure(values.slice(offset, offset + length)));
  }
  return total;
}

// A result as the fixture returns it: the value converted to i64.
const asI64 = (value) => BigInt.asIntN(64, value);
function sum(values, bits, signed) {
  const total = values.reduce((left, right) => left + right, 0n);
  return asI64(signed ? BigInt.asIntN(bits, total) : BigInt.asUintN(bits, total));
}
function bestIndex(values, smaller) {
  let best = 0;
  for (let index = 1; index < values.length; index++) if (smaller ? values[index] < values[best] : values[index] > values[best]) best = index;
  return best;
}

const expected = [];
const reference = {};
for (const [name, bits, signed] of types) {
  for (const pattern of patterns) {
    const values = data(pattern, bits, signed);
    const sums = checksum(values, (slice) => sum(slice, bits, signed));
    const mins = checksum(values, (slice) => slice.length ? asI64(slice[bestIndex(slice, true)]) : 0n);
    const maxes = checksum(values, (slice) => slice.length ? asI64(slice[bestIndex(slice, false)]) : 0n);
    const minIndex = checksum(values, (slice) => BigInt(bestIndex(slice, true)));
    const maxIndex = checksum(values, (slice) => BigInt(bestIndex(slice, false)));
    reference[`sum_${name} ${pattern}`] = sums;
    reference[`min_${name} ${pattern}`] = mins;
    reference[`max_${name} ${pattern}`] = maxes;
    expected.push(`sum_${name} ${pattern} ${sums}`, `min_${name} ${pattern} ${mins}`, `max_${name} ${pattern} ${maxes}`, `index_min_${name} ${pattern} ${minIndex}`, `index_max_${name} ${pattern} ${maxIndex}`);
  }
}
// The example's four forms: three full sums and one of [3, 997), each wrapped to i32.
const lcg = Array.from({ length: 1000 }, (_, index) => BigInt.asIntN(32, wrap(BigInt(index + 1) * 6364136223846793005n + 1442695040888963407n)));
const i32Sum = (values) => BigInt.asIntN(32, values.reduce((left, right) => left + right, 0n));
const forms = 3n * i32Sum(lcg) + i32Sum(lcg.slice(3, 997));
expected.push(`forms ${forms}`);

const work = mkdtempSync(join(tmpdir(), "tsuzuri-cpu-kernels-"));
try {
  // The C host calls every export, and the min and max kernels directly for their indices.
  const cType = { i8: "int8_t", i16: "int16_t", i32: "int32_t", i64: "int64_t", i8u: "uint8_t", i16u: "uint16_t", i32u: "uint32_t", i64u: "uint64_t" };
  const host = join(work, "host.c");
  writeFileSync(host, `#include <stdint.h>
#include <stdio.h>
${types.map(([name]) => `int64_t tz_sum_${name}(int64_t);\nint64_t tz_min_${name}(int64_t);\nint64_t tz_max_${name}(int64_t);\nint64_t tsuzuri_cpu_min_${name}(const ${cType[name]} *, int64_t);\nint64_t tsuzuri_cpu_max_${name}(const ${cType[name]} *, int64_t);`).join("\n")}
int64_t tz_forms(void);
static const int64_t lengths[] = { ${lengths.join(", ")} };
static uint64_t raw(int pattern, int bits, int is_signed, uint64_t k, uint64_t state) {
    uint64_t top = UINT64_C(1) << (bits - 1);
    switch (pattern) {
    case 0: return state >> (64 - bits);
    case 1: return is_signed ? top : 0;
    case 2: return is_signed ? top - 1 : (bits == 64 ? UINT64_MAX : (UINT64_C(1) << bits) - 1);
    case 3: return state >> 62;
    default: return k;
    }
}
#define INDICES(NAME, T, BITS, SIGNED) \\
    for (int pattern = 0; pattern < 5; ++pattern) { \\
        static T values[4100]; \\
        uint64_t state = 1; \\
        for (uint64_t k = 0; k < 4100; ++k) { \\
            state = state * UINT64_C(6364136223846793005) + UINT64_C(1442695040888963407); \\
            values[k] = (T)raw(pattern, BITS, SIGNED, k, state); \\
        } \\
        uint64_t minimum = 0, maximum = 0; \\
        for (int offset = 0; offset < 4; ++offset) \\
            for (size_t index = 0; index < sizeof(lengths) / sizeof(lengths[0]); ++index) { \\
                minimum = minimum * 1000003 + (uint64_t)tsuzuri_cpu_min_##NAME(values + offset, lengths[index]); \\
                maximum = maximum * 1000003 + (uint64_t)tsuzuri_cpu_max_##NAME(values + offset, lengths[index]); \\
            } \\
        printf("index_min_" #NAME " %d %lld\\n", pattern, (long long)(int64_t)minimum); \\
        printf("index_max_" #NAME " %d %lld\\n", pattern, (long long)(int64_t)maximum); \\
    }
int main(void) {
${types.map(([name, bits, signed]) => `    for (int pattern = 0; pattern < 5; ++pattern) {\n        printf("sum_${name} %d %lld\\n", pattern, (long long)tz_sum_${name}(pattern));\n        printf("min_${name} %d %lld\\n", pattern, (long long)tz_min_${name}(pattern));\n        printf("max_${name} %d %lld\\n", pattern, (long long)tz_max_${name}(pattern));\n    }\n    INDICES(${name}, ${cType[name]}, ${bits}, ${signed ? 1 : 0})`).join("\n")}
    printf("forms %lld\\n", (long long)tz_forms());
    return 0;
}
`);
  const sorted = [...expected].sort().join("\n");
  const variants = (features) => [["", true], ["baseline", true], ["sse4.2", (features & 1) !== 0], ["avx2", (features & 2) !== 0], ["avx512", (features & 4) !== 0], ["sve", (features & 65536) !== 0], ["unknown", false]];
  for (const cpu of ["generic", "native"]) for (const optimization of ["-O0", "-O3"]) {
    const object = join(work, `${cpu}${optimization}.o`);
    execute(compiler, ["build", fixture, "--emit", "object", "--cpu", cpu, optimization, "--no-cache", "-o", object]);
    const binary = join(work, `${cpu}${optimization}`);
    const probe = join(work, "probe.c");
    writeFileSync(probe, "#include <stdint.h>\n#include <stdio.h>\nuint64_t tsuzuri_cpu_features(void);\nint main(void) { printf(\"%llu\\n\", (unsigned long long)tsuzuri_cpu_features()); return 0; }\n");
    execute(clang, ["-std=c11", "-O1", host, object, "-lm", "-o", binary]);
    const featureProbe = join(work, `features${cpu}${optimization}`);
    execute(clang, ["-std=c11", probe, object, "-lm", "-o", featureProbe]);
    const features = Number(execute(featureProbe, []).stdout.trim());
    for (const [variant, supported] of variants(features)) {
      const env = { ...process.env };
      if (variant) env.TSUZURI_CPU_FORCE = variant; else delete env.TSUZURI_CPU_FORCE;
      const result = execute(binary, [], env, supported);
      if (supported) assert.equal(result.stdout.trim().split("\n").sort().join("\n"), sorted, `${cpu} ${optimization} ${variant || "default"}`);
      else {
        assert.notEqual(result.status, 0, `${variant} must fail`);
        assert.match(result.stderr, /requested variant is unavailable or unknown/);
      }
    }
    console.log(`cpu_kernels: ${cpu} ${optimization} features=${features} matches the BigInt reference`);
  }

  // WASM keeps the std bodies: the same checksums at -O0 and -O3, with and without simd128.
  for (const optimization of ["-O0", "-O3"]) for (const simd of [false, true]) {
    const wasm = join(work, `kernels${optimization}${simd}.wasm`);
    execute(compiler, ["build", fixture, "--target", "wasm32", optimization, ...(simd ? ["--wasm-feature", "simd128"] : []), "--no-cache", "-o", wasm]);
    const module = new WebAssembly.Module(readFileSync(wasm));
    assert.deepEqual(WebAssembly.Module.imports(module), []);
    const api = new WebAssembly.Instance(module).exports;
    for (const key of Object.keys(reference)) {
      const [name, pattern] = key.split(" ");
      assert.equal(api[`tz_${name}`](BigInt(pattern)), reference[key], `wasm ${optimization} simd=${simd} ${key}`);
    }
    assert.equal(api.tz_forms(), forms);
  }

  // The runtime's own checks of every kernel and level against scalar references.
  for (const optimization of ["-O0", "-O3"]) {
    const binary = join(work, `cpu_runtime${optimization}`);
    execute(clang, ["-std=c11", optimization, "-pthread", "tests/cpu_runtime.c", "-o", binary]);
    assert.match(execute(binary, []).stdout, /^features=\d+ variant=\d+\n$/);
  }

  // Phase 3: whichever version of a `@cpu` function runs, the program prints the same line.
  let dot = 0, scaled = 0, total = 0;
  for (let index = 0; index < 1003; index++) {
    dot += ((index * 7) % 13 - 6) * ((index * 5) % 11 - 5);
    scaled += (index % 17) * 3;
    total += index % 29;
  }
  const line = `${dot} ${scaled} ${total}\n`;
  for (const optimization of ["-O0", "-O3"]) {
    const binary = join(work, `versions${optimization}`);
    execute(compiler, ["build", "tests/fixtures/cpu_versions", optimization, "--no-cache", "-o", binary]);
    assert.equal(execute(binary, []).stdout, line);
    for (const level of ["baseline", "sse4.2", "avx2", "avx512", "sve", "sve2"]) {
      const result = execute(binary, [], { ...process.env, TSUZURI_CPU_FORCE: level }, false);
      if (result.status === 0) assert.equal(result.stdout, line, `${optimization} ${level}`);
      else assert.match(result.stderr, /requested variant is unavailable/, `${optimization} ${level}`);
    }
  }

  // The x86 versions are compiled here, not run: Apple silicon has no x86 or SVE hardware.
  const x86 = join(work, "cpu-x86.s");
  execute(clang, ["-target", "x86_64-apple-macos11", "-std=c11", "-O3", "-S", "src/runtime/cpu.c", "-o", x86]);
  const assembly = readFileSync(x86, "utf8");
  const body = (name) => assembly.slice(assembly.indexOf(`_${name}:`), assembly.indexOf("\n\t.cfi_endproc", assembly.indexOf(`_${name}:`)));
  assert.match(body("tz_cpu_min_i32_avx2"), /ymm/);
  assert.match(body("tz_cpu_sum_i64_avx2"), /ymm/);
  assert.match(body("tz_cpu_max_i8u_avx512"), /zmm/);
  assert.doesNotMatch(assembly, /ifunc|__cpu_model/);
  execute(clang, ["-target", "x86_64-apple-macos11", "-std=c11", "-O0", "-c", "src/runtime/cpu.c", "-o", join(work, "cpu-x86-O0.o")]);
  // AArch64 Linux reads SVE from the auxiliary vector; minimal C headers stand in for its sysroot.
  const sysroot = join(work, "sysroot");
  mkdirSync(sysroot);
  writeFileSync(join(sysroot, "stdio.h"), "typedef struct FILE FILE;\nextern FILE *stderr;\nint fputs(const char *, FILE *);\n");
  writeFileSync(join(sysroot, "stdlib.h"), "_Noreturn void abort(void);\nchar *getenv(const char *);\n");
  writeFileSync(join(sysroot, "string.h"), "typedef unsigned long size_t;\nvoid *memcpy(void *, const void *, size_t);\nint strcmp(const char *, const char *);\n");
  const arm = join(work, "cpu-arm-linux.s");
  execute(clang, ["-target", "aarch64-linux-gnu", "-nostdlibinc", "-isystem", sysroot, "-std=c11", "-O3", "-S", "src/runtime/cpu.c", "-o", arm]);
  assert.match(readFileSync(arm, "utf8"), /getauxval/);
  console.log("cpu_kernels: x86 variants cross-compiled; execution requires an x86 host");
} finally {
  rmSync(work, { recursive: true, force: true });
}
