import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(process.argv[2] ?? join(root, "target/debug/tsuzuri"));
const clang = process.env.TSUZURI_CLANG ?? "clang";
const fixture = join(root, "tests/fixtures/control");
const temporary = mkdtempSync(join(tmpdir(), "tsuzuri-control-"));
const sanitizer = process.env.TSUZURI_ASAN === "1" ? ["-fsanitize=address"] : [];
const min = -(1n << 63n);
const max = (1n << 63n) - 1n;
const umax = (1n << 64n) - 1n;
const wrap = (n) => BigInt.asIntN(64, n);

function execute(program, args, success = true) {
  const result = spawnSync(program, args, {
    cwd: root, encoding: "utf8", timeout: 180_000, maxBuffer: 4 * 1024 * 1024,
  });
  if (result.error) throw result.error;
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}
const cli = (args) => execute(compiler, args);

function rangeReference(first, step, last, unsigned = false) {
  let total = 0n;
  for (let n = first; step > 0n ? n <= last : n >= last; n += step) {
    if (n < (unsigned ? 0n : min) || n > (unsigned ? umax : max)) break;
    total = wrap(total + n);
  }
  return total;
}

const cases = [
  ["inclusive", [1, 4, 0], 10n],
  ["inclusive", [4, 1, 1], 10n],
  ["inclusive", [4, 1, 0], 0n],
  ["inclusive", [1, 4, 1], 0n],
  ["inclusive", [2147483646, 2147483647, 0], 4294967293n],
  ["inclusive", [-2147483647, -2147483648, 1], -4294967295n],
  ["inclusive", [2147483647, 2147483647, 0], 2147483647n],
  ["inclusive", [-2147483648, -2147483648, 1], -2147483648n],
  ["integer_boundaries", [], 2n],
  ["bounds_once", [], 13123n],
  ["while_checks", [0n], 100n],
  ["while_checks", [3n], 403n],
  ["while_checks", [-2n], 100n],
  ["destructuring", [], 583n],
  ["utf8_iteration", [], 4593n],
  ["structural_patterns", [], 55n],
  ["guard_once", [], 42n],
  ["function_forms", [], 46n],
  ["partial_moves", [], 47n],
  ["owned_guard", [], 4n],
  ["owned_replacements", [0n], 3n],
  ["owned_replacements", [8192n], 3n],
  ["tail_match", [1000000n], 1000000n],
  ["mutual", [10n], 1],
  ["mutual", [11n], 0],
  ["nested_match", [0n], 42n],
  ["nested_match", [1n], 10n],
  ["active_patterns", [], 94n],
  ["active_order", [], 134042n],
  ["active_owned", [0n], 0n],
  ["active_owned", [4096n], 20480n],
  ["active_short_circuit", [], 42n],
  ["software_patterns", [], 42n],
  ["floating_order", [], 1],
  ["scrutinee_once", [], 142n],
  ["empty_loops", [], 42n],
  ["recursion_wide", [0], -3n],
  ["recursion_wide", [1], -6n],
  ["recursion_wide", [2], 0n],
  ["recursion_order", [0], 130012n],
  ["recursion_order", [1], 130012n],
  ["recursion_order", [2], 130012n],
  ["recursion_floating", [], 1],
  ["recursion_zero", [], 1],
  ["recursion_nan", [], 1],
  ["recursion_owned", [0n], 5n],
  ["recursion_owned", [8192n], 32773n],
  ["recursion_temporaries", [100000n], 100000n],
];
for (const n of [0, 1, 2, 126, 127, -1, -2, -127, -128]) {
  cases.push(["recursion_down8", [n], BigInt.asUintN(8, BigInt(n))]);
  cases.push(["recursion_up8", [n], BigInt.asUintN(8, -BigInt(n))]);
}
for (const n of [0, 1, 2, 127, 128, 254, 255]) cases.push(["recursion_unsigned8", [n], BigInt(n)]);
for (const n of [min, max, -2n, -1n, 0n, 1n, 2n, 7n]) {
  cases.push(["recursion_snapshot", [n], wrap(n - 1n)]);
  for (const fuel of [0n, 1n, 3n]) for (const seed of [-3n, max]) {
    let count = n, state = seed, remaining = fuel;
    while (remaining > 0n && count !== 0n) {
      const unsigned = BigInt.asUintN(64, state);
      state = wrap((unsigned ^ (unsigned >> 13n)) * 6364136223846793005n + count + 1442695040888963407n);
      count = wrap(count - 1n);
      remaining--;
    }
    cases.push(["recursion_bounded", [n, fuel, seed], state]);
  }
}
for (const n of [-128, -127, -126, -125, -124, -1, 0, 127]) {
  cases.push(["dense_i8", [n], n <= -125 ? BigInt(n + 129) * 10n : 42n]);
}
for (const n of [min, -1n, 0n, 1n, 2n, 3n, 4n, 5n, 6n, 8n, 9n, max]) {
  cases.push(["dense_holes", [n], [1n, 2n, 4n, 5n].includes(n) ? n * 10n : 42n]);
}
for (const n of [0, -0, 1.5, 2, NaN, Infinity, -Infinity]) {
  cases.push(["numeric_patterns", [n], n === 0 ? 1n : n === 1.5 ? 2n : 3n]);
}
for (const args of [
  [1n, 2n, 10n], [10n, -3n, 1n], [3n, 2n, 1n], [1n, -1n, 3n],
  [max - 1n, 2n, max], [max - 1n, 1n, max], [min + 1n, -1n, min],
  [min + 1n, -2n, min], [min, min, min], [max, max, max],
  [1n, min, min], [min, max, max],
]) cases.push(["stepped", args, rangeReference(...args)]);
for (const args of [[1n, 3n, 12n], [umax - 1n, 2n, umax], [umax, 1n, umax], [3n, 1n, 1n]]) {
  cases.push(["unsigned_steps", args, rangeReference(...args, true)]);
}
for (const n of [0n, 1n, 127n, 4096n]) {
  let total = 0n;
  for (let i = 0n; i < n; i++) total = wrap(total + ((i * 17n) ^ -123n));
  cases.push(["collection_sum", [n, -123n], wrap(total + n)]);
  cases.push(["list_sum", [n], n * (n - 1n) / 2n * 3n]);
  cases.push(["loop_copies", [n], n * 62n + n * (n - 1n) / 2n]);
}
const patternValues = new Map([[-3n, 17n], [-1n, 17n], [0n, 3n], [1n, 29n], [2n, 7n], [3n, 61n],
  [4n, 11n], [5n, 83n], [6n, 5n], [7n, 47n], [8n, 19n], [9n, 101n]]);
for (let n = -5n; n <= 12n; n++) cases.push(["patterns", [n], patternValues.get(n) ?? 42n]);
cases.push(["patterns", [142n], 42n], ["patterns", [max], max - 100n]);
const traps = ["trap_step", "trap_pattern", "trap_lambda", "trap_for_active", "trap_fx_active", "recursion_trap_checked"];
const cValue = (n) => typeof n !== "bigint" ? Number.isNaN(n) ? "NAN"
  : n === Infinity ? "INFINITY" : n === -Infinity ? "-INFINITY" : Object.is(n, -0) ? "-0.0" : String(n)
  : n === min ? "INT64_MIN" : n < 0n ? `(-INT64_C(${-n}))`
    : n > max ? `UINT64_C(${n})` : `INT64_C(${n})`;

try {
  cli(["check", fixture]);
  const ir = join(temporary, "control.ll");
  const again = join(temporary, "again.ll");
  cli(["build", fixture, "--emit", "header", "-o", join(temporary, "control.h")]);
  cli(["build", fixture, "--emit", "llvm", "-o", ir]);
  cli(["build", fixture, "--emit", "llvm", "-o", again]);
  const sourceIr = readFileSync(ir, "utf8");
  assert.equal(sourceIr, readFileSync(again, "utf8"));
  assert.match(sourceIr, /switch i64/);
  assert.match(sourceIr, /llvm\.sadd\.with\.overflow\.i64/);
  const declarations = sourceIr.match(/^declare .*$/gm);
  assert.equal(new Set(declarations).size, declarations.length, "intrinsic declarations are deduplicated");
  let trackedIr = sourceIr.replaceAll("@malloc", "@tracked_alloc").replaceAll("@free", "@tracked_free");
  if (sanitizer.length) trackedIr = trackedIr.replaceAll(" nounwind {", " nounwind sanitize_address {");
  writeFileSync(ir, trackedIr);
  const host = join(temporary, "host.c");
  writeFileSync(host, `
#include <assert.h>
#include <math.h>
#include <stdint.h>
#include <stdlib.h>
#include "control.h"
static uint64_t live;
void *tracked_alloc(uint64_t size) {
    uint64_t *p = malloc((size_t)size + 16);
    assert(p);
    p[0] = size;
    p[1] = UINT64_C(0x51a110ca7e);
    live += size;
    return p + 2;
}
void tracked_free(void *value) {
    if (!value) return;
    uint64_t *p = (uint64_t *)value - 2;
    assert(p[1] == UINT64_C(0x51a110ca7e));
    p[1] = 0;
    assert(live >= p[0]);
    live -= p[0];
    free(p);
}
int main(int argc, char **argv) {
    if (argc == 2) {
        switch (atoi(argv[1])) {
            ${traps.map((name, index) => `case ${index}: (void)tz_${name}(); break;`).join("\n")}
        }
        return 0;
    }
    ${cases.map(([name, args, expected]) =>
    `assert(tz_${name}(${args.map(cValue).join(", ")}) == ${cValue(expected)}); assert(live == 0);`).join("\n")}
    for (int i = 0; i < 32; ++i) {
        assert(tz_loop_copies(8192) == INT64_C(34058240));
        assert(live == 0);
    }
    return 0;
}
`);
  for (const optimization of ["0", "3"]) {
    const native = join(temporary, `control-O${optimization}`);
    execute(clang, [`-O${optimization}`, "-Wno-override-module", "-ffp-contract=off", ...sanitizer,
      ir, host, "-lm", "-o", native]);
    execute(native, []);
    for (let index = 0; index < traps.length; index++) {
      const result = execute(native, [String(index)], false);
      assert.ok(result.status !== 0 || result.signal, `native O${optimization}: ${traps[index]} must trap`);
    }
    const wasm = join(temporary, `control-O${optimization}.wasm`);
    cli(["build", fixture, "--target", "wasm32", `-O${optimization}`, "-o", wasm]);
    const module = new WebAssembly.Module(readFileSync(wasm));
    assert.deepEqual(WebAssembly.Module.imports(module), []);
    const exports = new WebAssembly.Instance(module).exports;
    for (const [name, args, expected] of cases) {
      assert.equal(exports[`tz_${name}`](...args), expected, `WASM O${optimization}: ${name}(${args})`);
    }
    for (const name of traps) {
      const fresh = new WebAssembly.Instance(module).exports;
      assert.throws(() => fresh[`tz_${name}`](), WebAssembly.RuntimeError, name);
    }
    for (let i = 0; i < 32; i++) assert.equal(exports.tz_loop_copies(8192n), 34058240n);
    assert.ok(exports.memory.buffer.byteLength <= 16 * 1024 * 1024);
  }
  console.log(`control: ${cases.length} cases, ownership cleanup, traps and loop boundaries passed on native/WASM at O0/O3`);
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
