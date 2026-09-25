import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(process.argv[2] ?? join(root, "target/debug/tsuzuri"));
const clang = process.env.TSUZURI_CLANG ?? "clang";
const temporary = mkdtempSync(join(tmpdir(), "tsuzuri-primitives-"));
function execute(program, args, success = true) {
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 180_000, maxBuffer: 4 * 1024 * 1024 });
  if (result.error) throw result.error;
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}
const cli = (args, success) => execute(compiler, args, success);

const referenceData = JSON.parse(execute("python3", ["-c", `
import decimal, fractions, json, random
random.seed(271828)
cases = []
for width, precision, emin, emax in [(32, 7, -95, 96), (64, 16, -383, 384), (128, 34, -6143, 6144)]:
    context = decimal.Context(prec=precision, Emin=emin, Emax=emax, rounding=decimal.ROUND_HALF_EVEN, clamp=1)
    for trap in context.traps:
        context.traps[trap] = False
    for i in range(32):
        scale = random.choice([0, 1, -1, emin // 2, emax // 2])
        a = context.create_decimal(str(random.randint(-10**precision + 1, 10**precision - 1)) + 'e' + str(scale))
        b = context.create_decimal(str(random.randint(1, 10**precision - 1)) + 'e' + str(-scale if i % 3 == 0 else scale))
        for op, method in [('+', context.add), ('-', context.subtract), ('*', context.multiply), ('/', context.divide)]:
            result = method(a, b)
            cases.append([width, str(a), str(b), op, str(result)])
def binary_literal(value):
    if isinstance(value, str):
        return value
    with decimal.localcontext() as context:
        context.prec = 36
        return str(decimal.Decimal(value.numerator) / decimal.Decimal(value.denominator))

def binary_round(value):
    if not value:
        return fractions.Fraction(0)
    negative = value < 0
    value = abs(value)
    n, d = value.numerator, value.denominator
    exponent = n.bit_length() - d.bit_length()
    if (n < d << exponent) if exponent >= 0 else (n << -exponent < d):
        exponent -= 1
    quantum = max(exponent - 112, -16494)
    if quantum > 16271:
        return '-Infinity' if negative else 'Infinity'
    if quantum >= 0:
        d <<= quantum
    else:
        n <<= -quantum
    coefficient, remainder = divmod(n, d)
    if 2 * remainder > d or (2 * remainder == d and coefficient % 2):
        coefficient += 1
    if coefficient >= 2**113:
        coefficient //= 2
        quantum += 1
    if quantum > 16271:
        return '-Infinity' if negative else 'Infinity'
    result = fractions.Fraction(coefficient)
    result = result * 2**quantum if quantum >= 0 else result / 2**-quantum
    return -result if negative else result

binary = []
for i in range(32):
    values = []
    for j in range(2):
        coefficient = random.randrange(2**112, 2**113) * random.choice([-1, 1])
        exponent = random.choice([-16494, -1000, -113, 0, 1000, 16271])
        value = fractions.Fraction(coefficient)
        values.append(value * 2**exponent if exponent >= 0 else value / 2**-exponent)
    a, b = values
    for op, result in [('+', a + b), ('-', a - b), ('*', a * b), ('/', a / b)]:
        binary.append([binary_literal(a), binary_literal(b), op, binary_literal(binary_round(result))])
print(json.dumps({'decimal': cases, 'binary': binary}))
`]).stdout);
const reference = referenceData.decimal;
const binaryReference = referenceData.binary;

const literal = (value, width) => {
  if (value === "Infinity") return `(1.0d${width} / 0.0d${width})`;
  if (value === "-Infinity") return `(-1.0d${width} / 0.0d${width})`;
  if (value === "NaN") return `(0.0d${width} / 0.0d${width})`;
  return `${value}d${width}`;
};
const generated = reference.map(([width, a, b, op, expected], index) =>
  `export fn decimal_${index}() -> bool {
     let result = ${literal(a, width)} ${op} ${literal(b, width)};
     ${expected === "NaN" ? "result != result" : `result == ${literal(expected, width)}`}
   }`).join("\n");
const binaryLiteral = (value) => {
  if (value === "Infinity") return "(1.0f128 / 0.0f128)";
  if (value === "-Infinity") return "(-1.0f128 / 0.0f128)";
  return `${value}f128`;
};
const generatedBinary = binaryReference.map(([a, b, op, expected], index) =>
  `export fn binary_${index}() -> bool {
     ${binaryLiteral(a)} ${op} ${binaryLiteral(b)} == ${binaryLiteral(expected)}
   }`).join("\n");
// 8,192 i64 elements fill the 64 KiB stack-literal budget exactly; one more falls back to the heap.
const largeLiteral = (name, count) => `
export def ${name} :: i64
fn ${name} =
    let values = [${Array.from({ length: count }, (_, index) => index).join(", ")}]
    values[${count - 1}] + values.length
`;
const storageLiterals = largeLiteral("large_stack", 8192) + largeLiteral("large_heap", 8193);
// [call, native result, heap allocations, WASM call, WASM result]
const storage = [
  ["stack_array()", "10450", 0, (api) => api.tz_stack_array(), 10450n],
  ["stack_list()", "388", 0, (api) => api.tz_stack_list(), 388n],
  ["stack_string()", "2015", 0, (api) => api.tz_stack_string(), 2015n],
  ["stack_nested()", "435032", 0, (api) => api.tz_stack_nested(), 435032n],
  ["temporary_stack()", "5281210", 0, (api) => api.tz_temporary_stack(), 5281210n],
  ["empty_stack()", "0", 0, (api) => api.tz_empty_stack(), 0n],
  ["wide_stack()", "1", 0, (api) => api.tz_wide_stack(), 1],
  ["branch_stack(1)", "71112", 0, (api) => api.tz_branch_stack(1), 71112n],
  ["branch_stack(0)", "11233", 1, (api) => api.tz_branch_stack(0), 11233n],
  ["large_stack()", "16383", 0, (api) => api.tz_large_stack(), 16383n],
  ["large_heap()", "16385", 1, (api) => api.tz_large_heap(), 16385n],
  ["heap_literals()", "3053", 4, (api) => api.tz_heap_literals(), 3053n],
  ["escape_return()", "1210", 1, (api) => api.tz_escape_return(), 1210n],
  ["escape_argument()", "35", 1, (api) => api.tz_escape_argument(), 35n],
  ["escape_nested()", "3043", 3, (api) => api.tz_escape_nested(), 3043n],
  ["escape_record()", "642", 2, (api) => api.tz_escape_record(), 642n],
  ["escape_strings()", "3032", 3, (api) => api.tz_escape_strings(), 3032n],
  ["escape_list()", "397", 3, (api) => api.tz_escape_list(), 397n],
  ["mutable_borrow()", "63", 7, (api) => api.tz_mutable_borrow(), 63n],
  ["assign_stack()", "2352", 4, (api) => api.tz_assign_stack(), 2352n],
  ["partial_move()", "905078", 5, (api) => api.tz_partial_move(), 905078n],
  ["match_stack(1)", "6321", 1, (api) => api.tz_match_stack(1), 6321n],
  ["match_stack(0)", "7321", 2, (api) => api.tz_match_stack(0), 7321n],
  ["copy_stack()", "21", 1, (api) => api.tz_copy_stack(), 21n],
  ["concat_stack()", "1206", 2, (api) => api.tz_concat_stack(), 1206n],
  ["capture_stack()", "76", 4, (api) => api.tz_capture_stack(), 76n],
  ["function_stack()", "4015", 2, (api) => api.tz_function_stack(), 4015n],
  ["loop_copies(10)", "210110", 10, (api) => api.tz_loop_copies(10n), 210110n],
  ["loop_moves(10)", "22", 30, (api) => api.tz_loop_moves(10n), 22n],
  ["tail_frames(0)", "4", 0, (api) => api.tz_tail_frames(0n), 4n],
  ["tail_frames(100000)", "5", 200000, (api) => api.tz_tail_frames(100000n), 5n],
  ["reborrow_stack()", "33", 11, (api) => api.tz_reborrow_stack(), 33n],
  ["or_moves(1)", "3", 1, (api) => api.tz_or_moves(1), 3n],
  ["or_moves(0)", "2", 1, (api) => api.tz_or_moves(0), 2n],
  ["delivered_match(0)", "14", 0, (api) => api.tz_delivered_match(0n), 14n],
  ["delivered_match(1)", "23", 0, (api) => api.tz_delivered_match(1n), 23n],
  ["delivered_match(2)", "0", 0, (api) => api.tz_delivered_match(2n), 0n],
  ["scrutinee_moves()", "602", 6, (api) => api.tz_scrutinee_moves(), 602n],
  ["lent_capture()", "616", 1, (api) => api.tz_lent_capture(), 616n],
];

try {
  const input = join(temporary, "Main.tz");
  writeFileSync(join(temporary, "Curried.tz"), readFileSync(join(root, "tests/fixtures/currying/Functions.tz"), "utf8"));
  writeFileSync(join(temporary, "Classes.tt"), readFileSync(join(root, "tests/fixtures/currying/Classes.tt"), "utf8"));
  writeFileSync(join(temporary, "Arrays.tz"), readFileSync(join(root, "tests/fixtures/arrays/Arrays.tz"), "utf8"));
  writeFileSync(join(temporary, "Lists.tz"), readFileSync(join(root, "tests/fixtures/lists/Lists.tz"), "utf8"));
  writeFileSync(join(temporary, "Storage.tz"), readFileSync(join(root, "tests/fixtures/storage/Storage.tz"), "utf8") + storageLiterals);
  writeFileSync(input, readFileSync(join(root, "tests/fixtures/primitives/Main.tz"), "utf8") + "\n" + generated + "\n" + generatedBinary);
  cli(["check", input]);
  const header = join(temporary, "primitives.h");
  cli(["build", input, "--emit", "header", "-o", header]);
  const host = join(temporary, "host.c");
  writeFileSync(host, `
#include <assert.h>
#include <math.h>
#include <stdint.h>
#include <stdlib.h>
#include <stdio.h>
#include "primitives.h"
static uint64_t live, peak, allocations;
void *tracked_alloc(uint64_t n) {
    uint64_t *p = malloc((size_t)n + 16);
    assert(p);
    p[0] = n; p[1] = UINT64_C(0x51a110ca7e);
    live += n; if (live > peak) peak = live;
    ++allocations;
    return p + 2;
}
void tracked_free(void *pointer) {
    if (!pointer) return;
    uint64_t *p = (uint64_t *)pointer - 2;
    assert(p[1] == UINT64_C(0x51a110ca7e));
    p[1] = 0; assert(live >= p[0]); live -= p[0]; free(p);
}
int main(int argc, char **argv) {
    if (argc > 1) {
        switch (atoi(argv[1])) {
            case 0: tz_array_bounds(0, 0); break;
            case 1: tz_array_bounds(3, -1); break;
            case 2: tz_array_bounds(3, 3); break;
            case 3: tz_array_length(-1); break;
            case 4: tz_array_length(INT64_MIN); break;
            case 5: tz_array_length(INT64_MAX); break;
            case 6: tz_array_length(INT64_C(1) << 61); break;
            case 7: tz_array_init_trap(2); break;
            case 8: tz_list_bounds(0, 0); break;
            case 9: tz_list_bounds(3, -1); break;
            case 10: tz_list_bounds(3, 3); break;
            case 11: tz_list_length(-1); break;
            case 12: tz_list_length(INT64_MIN); break;
            case 13: tz_list_length(INT64_MAX); break;
            case 14: tz_list_length(INT64_C(1) << 61); break;
            case 15: tz_list_init_trap(2); break;
        }
        return 0;
    }
    assert(tz_curried_integer() == 42 && live == 0);
    assert(tz_curried_float() == 3.75 && live == 0);
    assert(tz_curried_decimal() == 0.3 && live == 0);
    assert(tz_curried_capture() == 42 && live == 0);
    assert(tz_curried_choose(1, 41) == 42 && live == 0);
    assert(tz_curried_choose(0, 43) == 42 && live == 0);
    assert(tz_curried_aggregate() == 63 && live == 0);
    assert(tz_curried_string() == 13 && live == 0);
    assert(tz_curried_nested() == 15 && live == 0);
    assert(tz_curried_borrow() == 19 && live == 0);
    assert(tz_curried_order() == 12 && live == 0);
    assert(tz_curried_pipeline() == 21 && live == 0);
    assert(tz_curried_exclusive() == 7 && live == 0);
    assert(tz_curried_class() == 45 && live == 0);
    assert(tz_curried_higher() == 42 && live == 0);
    assert(tz_curried_churn(40000) == 2160054 && live == 0);
    assert(tz_strings(1) == 24 && live == 0);
    assert(tz_strings(0) == 23 && live == 0);
    assert(tz_mutable_local() == 42);
    assert(tz_mutable_local_keywords() == 42);
    assert(tz_narrow_signed(127, 1) == -128);
    assert(tz_narrow_unsigned(255, 1) == 0);
    assert(tz_unsigned32(UINT32_MAX, 1) == 0);
    assert(tz_unsigned64(UINT64_MAX, 2) == UINT64_C(9223372036854775807));
    assert(tz_float32(0.1f, 0.2f) == (0.1f + 0.2f) * 0.5f);
    assert(tz_half_add(1.0f, 0.00048828125f) == 1.0f);
    assert(tz_half_add(65504.0f, 65504.0f) == INFINITY);
    assert(signbit(tz_half_divide(-0.0f, 1.0f)));
    assert(isnan(tz_half_divide(0.0f, 0.0f)));
    assert(tz_integer128());
    assert(tz_binary_precision());
    assert(tz_decimal_precision());
    for (int mode = 0; mode < 3; ++mode) {
        for (int count = -1; count <= 255; ++count) {
            unsigned __int128 value = ((unsigned __int128)UINT64_C(0xfedcba9876543210) << 64) | UINT64_C(0x8123456789abcdef);
            int shift = count & 127;
            unsigned __int128 expected = mode == 0 ? value << shift : value >> shift;
            if (mode == 2 && shift) expected |= ~(unsigned __int128)0 << (128 - shift);
            assert(tz_wide_shift((int64_t)value, (int64_t)(value >> 64), count, mode, 0) == (int64_t)expected);
            assert(tz_wide_shift((int64_t)value, (int64_t)(value >> 64), count, mode, 1) == (int64_t)(expected >> 64));
        }
    }
    ${reference.map((_, index) => `if (!tz_decimal_${index}()) { fprintf(stderr, "decimal case ${index} failed\\n"); return 1; }`).join("\n    ")}
    ${binaryReference.map((_, index) => `if (!tz_binary_${index}()) { fprintf(stderr, "binary128 case ${index} failed\\n"); return 1; }`).join("\n    ")}
    assert(tz_ownership_stress(200000) == 512 && live == 0);
    for (int i = 0; i < 500; ++i) {
        int n = i % 64;
        assert(tz_coalescing(n) == 16 * (3 * n + 3) && live == 0);
    }
    assert(peak < 16384);
    peak = 0;
    for (int n = 0; n <= 32; ++n) {
        assert(tz_array_values(n) == (n == 0 ? 0 : 3 * n + 19) && live == 0);
        assert(tz_array_owned(n) == (n == 0 ? 0 : n + 6) && live == 0);
        assert(tz_array_capture(n) == 4 * n && live == 0);
        assert(tz_array_functions(n) == (n == 0 ? 0 : n + 29) && live == 0);
        assert(tz_array_zero_sized(n) == 2 * n && live == 0);
    }
    assert(tz_array_values(2048) == 6163 && live == 0);
    assert(tz_array_shapes(1) == 15 && live == 0);
    assert(tz_array_shapes(0) == 27 && live == 0);
    assert(tz_array_empty() == 0 && live == 0);
    assert(tz_array_temporaries() == 15 && live == 0);
    assert(tz_array_wide() && live == 0);
    assert(tz_array_borrows() == 15 && live == 0);
    assert(tz_array_replace() == 17 && live == 0);
    assert(tz_array_order() == 44 && live == 0);
    assert(tz_array_bounds(3, 2) == 2 && live == 0);
    assert(tz_array_init_trap(0) == 0 && live == 0);
    assert(tz_array_init_trap(1) == 1 && live == 0);
    assert(tz_array_churn(100000) == 8 && live == 0);
    for (int n = 0; n <= 32; ++n) {
        assert(tz_list_values(n) == (n == 0 ? 0 : 3 * n + 19) && live == 0);
        assert(tz_list_owned(n) == (n == 0 ? 0 : n + 6) && live == 0);
        assert(tz_list_capture(n) == 4 * n && live == 0);
        assert(tz_list_functions(n) == (n == 0 ? 0 : n + 29) && live == 0);
        assert(tz_list_zero_sized(n) == 2 * n && live == 0);
    }
    assert(tz_list_values(2048) == 6163 && live == 0);
    assert(tz_list_shapes(1) == 15 && live == 0);
    assert(tz_list_shapes(0) == 27 && live == 0);
    assert(tz_list_empty() == 0 && live == 0);
    assert(tz_list_temporaries() == 15 && live == 0);
    assert(tz_list_mixed() == 7 && live == 0);
    assert(tz_list_wide() && live == 0);
    assert(tz_list_borrows() == 15 && live == 0);
    assert(tz_list_replace() == 17 && live == 0);
    assert(tz_list_order() == 44 && live == 0);
    assert(tz_list_bounds(3, 2) == 2 && live == 0);
    assert(tz_list_init_trap(0) == 0 && live == 0);
    assert(tz_list_init_trap(1) == 1 && live == 0);
    assert(tz_list_churn(100000) == 8 && live == 0);
    assert(peak < 131072);
    uint64_t before = allocations;
    assert(tz_list_length(100000) == 100000 && live == 0);
    assert(allocations - before == 100000);
    before = allocations;
    assert(tz_list_length(0) == 0 && live == 0 && allocations == before);
    // Literals without 'new' live in the stack frame; heap work is only 'new', escapes, and copies.
    ${storage.map(([call, expected, count]) => `before = allocations;
    assert(tz_${call} == ${expected} && live == 0);
    if (allocations - before != ${count}) { fprintf(stderr, "${call}: %llu heap allocations\\n", (unsigned long long)(allocations - before)); return 1; }`).join("\n    ")}
    return 0;
}
`);
  for (const optimization of [0, 3]) {
    const ir = join(temporary, `primitives-${optimization}.ll`);
    const native = join(temporary, `primitives-${optimization}`);
    const wasm = join(temporary, `primitives-${optimization}.wasm`);
    cli(["build", input, "--emit", "llvm", "-o", ir]);
    writeFileSync(ir, readFileSync(ir, "utf8").replaceAll("@malloc", "@tracked_alloc").replaceAll("@free", "@tracked_free"));
    execute(clang, [`-O${optimization}`, "-Wno-override-module", host, ir, "-lm", "-o", native]);
    execute(native, []);
    for (let trap = 0; trap < 16; ++trap) {
      const result = execute(native, [String(trap)], false);
      assert.notEqual(result.status, 0, `native collection trap ${trap}`);
    }
    cli(["build", input, "--target", "wasm32", `-O${optimization}`, "-o", wasm]);
    const { module, instance } = await WebAssembly.instantiate(readFileSync(wasm));
    assert.deepEqual(WebAssembly.Module.imports(module), []);
    const api = instance.exports;
    assert.equal(api.tz_curried_integer(), 42);
    assert.equal(api.tz_curried_float(), 3.75);
    assert.equal(api.tz_curried_decimal(), 0.3);
    assert.equal(api.tz_curried_capture(), 42);
    assert.equal(api.tz_curried_choose(1, 41), 42);
    assert.equal(api.tz_curried_choose(0, 43), 42);
    assert.equal(api.tz_curried_aggregate(), 63);
    assert.equal(api.tz_curried_string(), 13n);
    assert.equal(api.tz_curried_nested(), 15n);
    assert.equal(api.tz_curried_borrow(), 19n);
    assert.equal(api.tz_curried_order(), 12);
    assert.equal(api.tz_curried_pipeline(), 21);
    assert.equal(api.tz_curried_exclusive(), 7n);
    assert.equal(api.tz_curried_class(), 45);
    assert.equal(api.tz_curried_higher(), 42);
    assert.equal(api.tz_curried_churn(40000n), 2160054n);
    for (let n = 0n; n <= 32n; ++n) {
      assert.equal(api.tz_array_values(n), n === 0n ? 0n : 3n * n + 19n);
      assert.equal(api.tz_array_owned(n), n === 0n ? 0n : n + 6n);
      assert.equal(api.tz_array_capture(n), 4n * n);
      assert.equal(api.tz_array_functions(n), n === 0n ? 0n : n + 29n);
      assert.equal(api.tz_array_zero_sized(n), 2n * n);
    }
    assert.equal(api.tz_array_values(2048n), 6163n);
    assert.equal(api.tz_array_shapes(1), 15n);
    assert.equal(api.tz_array_shapes(0), 27n);
    assert.equal(api.tz_array_empty(), 0n);
    assert.equal(api.tz_array_temporaries(), 15n);
    assert.equal(api.tz_array_wide(), 1);
    assert.equal(api.tz_array_borrows(), 15n);
    assert.equal(api.tz_array_replace(), 17n);
    assert.equal(api.tz_array_order(), 44n);
    assert.equal(api.tz_array_bounds(3n, 2n), 2n);
    assert.equal(api.tz_array_init_trap(0n), 0n);
    assert.equal(api.tz_array_init_trap(1n), 1n);
    for (let n = 0n; n <= 32n; ++n) {
      assert.equal(api.tz_list_values(n), n === 0n ? 0n : 3n * n + 19n);
      assert.equal(api.tz_list_owned(n), n === 0n ? 0n : n + 6n);
      assert.equal(api.tz_list_capture(n), 4n * n);
      assert.equal(api.tz_list_functions(n), n === 0n ? 0n : n + 29n);
      assert.equal(api.tz_list_zero_sized(n), 2n * n);
    }
    assert.equal(api.tz_list_values(2048n), 6163n);
    assert.equal(api.tz_list_shapes(1), 15n);
    assert.equal(api.tz_list_shapes(0), 27n);
    assert.equal(api.tz_list_empty(), 0n);
    assert.equal(api.tz_list_temporaries(), 15n);
    assert.equal(api.tz_list_mixed(), 7n);
    assert.equal(api.tz_list_wide(), 1);
    assert.equal(api.tz_list_borrows(), 15n);
    assert.equal(api.tz_list_replace(), 17n);
    assert.equal(api.tz_list_order(), 44n);
    assert.equal(api.tz_list_bounds(3n, 2n), 2n);
    assert.equal(api.tz_list_init_trap(0n), 0n);
    assert.equal(api.tz_list_init_trap(1n), 1n);
    assert.equal(api.tz_list_length(100000n), 100000n);
    for (const [call, , , run, expected] of storage) assert.equal(run(api), expected, call);
    assert.equal(api.tz_strings(1), 24n);
    assert.equal(api.tz_strings(0), 23n);
    assert.equal(api.tz_mutable_local(), 42);
    assert.equal(api.tz_mutable_local_keywords(), 42);
    assert.equal(api.tz_narrow_signed(127, 1), -128);
    assert.equal(api.tz_narrow_unsigned(255, 1), 0);
    assert.equal(api.tz_unsigned32(-1, 1), 0);
    assert.equal(api.tz_unsigned64(-1n, 2n), 9223372036854775807n);
    assert.equal(api.tz_float32(0.1, 0.2), Math.fround(Math.fround(Math.fround(0.1) + Math.fround(0.2)) * 0.5));
    assert.equal(api.tz_half_add(1, 0.00048828125), 1);
    assert.equal(api.tz_half_add(65504, 65504), Infinity);
    assert.equal(api.tz_half_divide(-0, 1), -0);
    assert.ok(Number.isNaN(api.tz_half_divide(0, 0)));
    assert.equal(api.tz_integer128(), 1);
    assert.equal(api.tz_binary_precision(), 1);
    assert.equal(api.tz_decimal_precision(), 1);
    for (let mode = 0; mode < 3; mode++) {
      for (let count = -1n; count <= 255n; count++) {
        const value = 0xfedcba98765432108123456789abcdefn;
        const shift = count & 127n;
        const expected = mode === 0 ? BigInt.asUintN(128, value << shift)
          : mode === 1 ? value >> shift : BigInt.asUintN(128, BigInt.asIntN(128, value) >> shift);
        const low = BigInt.asIntN(64, value);
        const high = BigInt.asIntN(64, value >> 64n);
        assert.equal(api.tz_wide_shift(low, high, count, mode, 0), BigInt.asIntN(64, expected));
        assert.equal(api.tz_wide_shift(low, high, count, mode, 1), BigInt.asIntN(64, expected >> 64n));
      }
    }
    reference.forEach((test, index) => assert.equal(api[`tz_decimal_${index}`](), 1, `decimal reference: ${test.join(" ")}`));
    binaryReference.forEach((test, index) => assert.equal(api[`tz_binary_${index}`](), 1, `binary128 reference: ${test.join(" ")}`));
    for (const run of [
      () => api.tz_signed8_divide(1, 0),
      () => api.tz_signed8_divide(-128, -1),
      () => api.tz_unsigned128_zero(0n),
      () => api.tz_signed128_overflow(),
      () => api.tz_string_bounds(-1n),
      () => api.tz_string_bounds(3n),
      () => api.tz_array_bounds(0n, 0n),
      () => api.tz_array_bounds(3n, -1n),
      () => api.tz_array_bounds(3n, 3n),
      () => api.tz_array_length(-1n),
      () => api.tz_array_length(-(1n << 63n)),
      () => api.tz_array_length((1n << 63n) - 1n),
      () => api.tz_array_length(1n << 61n),
      () => api.tz_array_length(3n * 1024n * 1024n),
      () => api.tz_array_init_trap(2n),
      () => api.tz_list_bounds(0n, 0n),
      () => api.tz_list_bounds(3n, -1n),
      () => api.tz_list_bounds(3n, 3n),
      () => api.tz_list_length(-1n),
      () => api.tz_list_length(-(1n << 63n)),
      () => api.tz_list_length((1n << 63n) - 1n),
      () => api.tz_list_length(1n << 61n),
      () => api.tz_list_init_trap(2n),
    ]) assert.throws(run, WebAssembly.RuntimeError);
    const before = api.memory.buffer.byteLength;
    assert.equal(api.tz_ownership_stress(200000n), 512n);
    assert.equal(api.tz_array_churn(100000n), 8n);
    assert.equal(api.tz_list_churn(100000n), 8n);
    assert.equal(api.tz_list_length(100000n), 100000n);
    assert.equal(api.tz_loop_moves(100000n), 22n);
    assert.equal(api.tz_tail_frames(200000n), 5n);
    for (let i = 0; i < 500; ++i) {
      const n = BigInt(i % 64);
      assert.equal(api.tz_coalescing(n), 16n * (3n * n + 3n));
    }
    assert.ok(api.memory.buffer.byteLength <= before + 65536, "heap must reuse and coalesce freed blocks");
    const exhausted = (await WebAssembly.instantiate(readFileSync(wasm))).instance.exports;
    assert.throws(() => exhausted.tz_allocation_limit(), WebAssembly.RuntimeError);
    assert.ok(exhausted.memory.buffer.byteLength <= 16 * 1024 * 1024);
    const exhaustedList = (await WebAssembly.instantiate(readFileSync(wasm))).instance.exports;
    assert.throws(() => exhaustedList.tz_list_length(1024n * 1024n), WebAssembly.RuntimeError);
    assert.ok(exhaustedList.memory.buffer.byteLength <= 16 * 1024 * 1024);
    assert.equal(cli(["run", input, `-O${optimization}`]).stdout, "UTF-8: 日本語 😀\n");
    console.log(`-O${optimization}: primitive widths, ${reference.length} decimal / ${binaryReference.length} binary128 reference cases, curried closures, runtime arrays and linked lists, ${storage.length} stack/heap storage cases, UTF-8, borrows, bounded heap, no WASM imports`);
  }
  for (const [value, expected] of [
    ["340282366920938463463374607431768211455i128u", "340282366920938463463374607431768211455"],
    ["-170141183460469231731687303715884105728i128", "-170141183460469231731687303715884105728"],
    ["1.0f16 / 2.0f16", "0.5"],
    ["1.0000000000000000000000000000000002f128", "1.0000000000000000000000000000000002"],
    ["0.1d128 + 0.2d128", "0.3"],
    ['"a\\0b"', "a\0b"],
  ]) {
    writeFileSync(input, value);
    assert.equal(cli(["run", input]).stdout, `${expected}\n`, value);
  }
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
