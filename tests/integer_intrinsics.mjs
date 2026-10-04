import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const clang = process.env.TSUZURI_CLANG ?? "clang";
const temporary = mkdtempSync(join(tmpdir(), "tsuzuri-integer-intrinsics-"));
const operations = ["min", "max", "clamp", "abs", "unsigned_abs", "abs_diff", "count_ones",
  "leading_zeros", "trailing_zeros", "rotate_left", "rotate_right", "swap_bytes", "reverse_bits",
  "is_power_of_two", "checked_add", "checked_sub", "checked_mul", "checked_div", "checked_rem",
  "checked_neg", "saturating_add", "saturating_sub", "saturating_mul", "wrapping_pow", "checked_pow", "widening_mul"];
const definitions = [], functions = [], cases = [];
const bitsOf = (value, bits) => BigInt.asUintN(bits, value);
const limbs = (value, signed) => [bitsOf(value, 64), signed ? BigInt.asIntN(64, value >> 64n) : bitsOf(value >> 64n, 64)];
const execute = (program, args, options = {}) => {
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 240_000, maxBuffer: 16 * 1024 * 1024, ...options });
  if (result.error) {
    const index = result.stderr?.match(/failed case (\d+)/)?.[1];
    throw new Error(`${result.error.message}\n${result.stderr ?? ""}\n${index === undefined ? "" : cases[Number(index)].label}`, { cause: result.error });
  }
  assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
};
const cli = (args) => execute(compiler, args);

function reference(operation, left, right, amount, bits, signed) {
  const minimum = signed ? -(1n << BigInt(bits - 1)) : 0n;
  const maximum = signed ? (1n << BigInt(bits - 1)) - 1n : (1n << BigInt(bits)) - 1n;
  const wrap = (value) => signed ? BigInt.asIntN(bits, value) : bitsOf(value, bits);
  const saturate = (value) => value < minimum ? minimum : value > maximum ? maximum : value;
  const checked = (value) => value < minimum || value > maximum ? null : value;
  const unsigned = bitsOf(left, bits), width = BigInt(bits), shift = amount & (width - 1n);
  switch (operation) {
    case "min": return left < right ? left : right;
    case "max": return left > right ? left : right;
    case "clamp": {
      const bound = wrap(amount), lower = right < bound ? right : bound, upper = right > bound ? right : bound;
      return left < lower ? lower : left > upper ? upper : left;
    }
    case "abs": return wrap(left < 0n ? -left : left);
    case "unsigned_abs": return left < 0n ? -left : left;
    case "abs_diff": return left < right ? right - left : left - right;
    case "count_ones": return BigInt(unsigned.toString(2).replaceAll("0", "").length);
    case "leading_zeros": return unsigned === 0n ? width : width - BigInt(unsigned.toString(2).length);
    case "trailing_zeros": {
      if (unsigned === 0n) return width;
      let count = 0n;
      while (((unsigned >> count) & 1n) === 0n) count++;
      return count;
    }
    case "rotate_left": return wrap((unsigned << shift) | (unsigned >> ((width - shift) & (width - 1n))));
    case "rotate_right": return wrap((unsigned >> shift) | (unsigned << ((width - shift) & (width - 1n))));
    case "swap_bytes": {
      let result = 0n;
      for (let offset = 0n; offset < width; offset += 8n) result = (result << 8n) | ((unsigned >> offset) & 255n);
      return wrap(result);
    }
    case "reverse_bits": {
      let result = 0n;
      for (let offset = 0n; offset < width; offset++) result = (result << 1n) | ((unsigned >> offset) & 1n);
      return wrap(result);
    }
    case "is_power_of_two": return unsigned !== 0n && (unsigned & (unsigned - 1n)) === 0n ? 1n : 0n;
    case "checked_add": return checked(left + right);
    case "checked_sub": return checked(left - right);
    case "checked_mul": return checked(left * right);
    case "checked_div": return right === 0n || (signed && left === minimum && right === -1n) ? null : left / right;
    case "checked_rem": return right === 0n || (signed && left === minimum && right === -1n) ? null : left % right;
    case "checked_neg": return checked(-left);
    case "saturating_add": return saturate(left + right);
    case "saturating_sub": return saturate(left - right);
    case "saturating_mul": return saturate(left * right);
    case "wrapping_pow": return wrap(left ** amount);
    case "checked_pow": return amount < 0n ? null : checked(left ** amount);
    case "widening_mul": return left * right;
    default: throw new Error(operation);
  }
}

for (const bits of [8, 16, 32, 64, 128]) for (const signed of [true, false]) {
  const type = `i${bits}${signed ? "" : "u"}`, wide = signed ? "i128" : "i128u", high = signed ? "i64" : "i64u";
  const minimum = signed ? -(1n << BigInt(bits - 1)) : 0n;
  const maximum = signed ? (1n << BigInt(bits - 1)) - 1n : (1n << BigInt(bits)) - 1n;
  const boundary = [...new Set([minimum, minimum + 1n, 0n, 1n, 2n, maximum - 1n, maximum, ...(signed ? [-2n, -1n] : [])])];
  const inputs = boundary.flatMap((left) => boundary.map((right) => [left, right]));
  if (bits === 8) for (let value = minimum; value <= maximum; value++) inputs.push([value, 1n]);
  let seed = 0x123456789abcdefn;
  for (let index = 0; index < 1000; index++) {
    seed = bitsOf(seed * 6364136223846793005n + 1442695040888963407n, 128);
    const left = signed ? BigInt.asIntN(bits, seed) : bitsOf(seed, bits);
    seed = bitsOf(seed * 2862933555777941757n + 3037000493n, 128);
    inputs.push([left, signed ? BigInt.asIntN(bits, seed) : bitsOf(seed, bits)]);
  }
  for (const operation of operations) {
    if ((!signed && ["abs", "unsigned_abs", "checked_neg"].includes(operation)) || (signed && operation === "is_power_of_two") || (bits === 128 && operation === "widening_mul")) continue;
    const id = functions.length, name = `integer_${id}`;
    const count = ["count_ones", "leading_zeros", "trailing_zeros"].includes(operation);
    const outputSigned = count || (signed && !["unsigned_abs", "abs_diff"].includes(operation));
    const outputWide = outputSigned ? "i128" : "i128u", outputHigh = outputSigned ? "i64" : "i64u";
    const checked = operation.startsWith("checked_");
    const unary = ["abs", "unsigned_abs", "count_ones", "leading_zeros", "trailing_zeros", "swap_bytes", "reverse_bits", "is_power_of_two", "checked_neg"].includes(operation);
    const arguments_ = operation === "clamp" ? `left (Int.min right (amount as ${type})) (Int.max right (amount as ${type}))`
      : operation.endsWith("_pow") || operation.startsWith("rotate_") ? "left amount" : unary ? "left" : "left right";
    const compare = operation === "is_power_of_two" ? "(if actual then 1i128 else 0i128) == expected as i128" : `(actual as ${outputWide}) == expected`;
    definitions.push(`export def ${name} :: i64u -> ${high} -> i64u -> ${high} -> i64 -> i64u -> ${outputHigh} -> bool -> bool
fn ${name} left_low left_high right_low right_high amount expected_low expected_high present =
    let left = (((left_high as ${wide}) <<< 64) | (left_low as ${wide})) as ${type}
    let right = (((right_high as ${wide}) <<< 64) | (right_low as ${wide})) as ${type}
    let expected = ((expected_high as ${outputWide}) <<< 64) | (expected_low as ${outputWide})
    ${checked ? `match Int.${operation} ${arguments_} with | Some actual -> present && ${compare} | None -> !present` : `let actual = Int.${operation} ${arguments_}\n    ${compare}`}
`);
    functions.push({ name, signed, outputSigned });
    for (const [index, [left, right]] of inputs.entries()) {
      const amount = operation.endsWith("_pow") ? BigInt(index % 18 - (operation === "checked_pow" ? 1 : 0))
        : [-257n, -129n, -1n, 0n, 1n, 63n, 64n, 127n, 128n, 255n][index % 10];
      const expected = reference(operation, left, right, amount, bits, signed);
      cases.push({ id, label: `${type} ${operation}(${left}, ${right}, ${amount})`,
        args: [...limbs(left, signed), ...limbs(right, signed), amount, ...limbs(expected ?? 0n, outputSigned), expected === null ? 0 : 1] });
    }
  }
}

try {
  const input = join(temporary, "Main.tz"), ir = join(temporary, "integers.ll"), again = join(temporary, "again.ll");
  definitions.push("export def trap_clamp :: i64\nfn trap_clamp = Int.clamp 0i64 2 1\nexport def trap_pow :: i64\nfn trap_pow = Int.wrapping_pow 2i64 (-1)\n");
  writeFileSync(input, definitions.join("\n"));
  cli(["build", input, "--emit", "llvm", "-o", ir]);
  cli(["build", input, "--emit", "llvm", "-o", again]);
  const text = readFileSync(ir, "utf8");
  assert.equal(text, readFileSync(again, "utf8"));
  assert.doesNotMatch(text, /mul\.with\.overflow\.i128|__muloti4/);
  const declarations = text.match(/^declare .*$/gm) ?? [];
  assert.equal(new Set(declarations).size, declarations.length);
  cli(["build", input, "--emit", "header", "-o", join(temporary, "integers.h")]);
  const host = join(temporary, "host.c");
  writeFileSync(host, `#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "integers.h"
static int64_t signed_value(uint64_t value) { int64_t result; memcpy(&result, &value, sizeof result); return result; }
int main(int argc, char **argv) {
  if (argc > 1) { if (atoi(argv[1]) == 0) (void)tz_trap_clamp(); else (void)tz_trap_pow(); return 0; }
  unsigned operation, present; unsigned long long al, ah, bl, bh, amount, el, eh; size_t index = 0;
  while (scanf("%u %llu %llu %llu %llu %llu %llu %llu %u", &operation, &al, &ah, &bl, &bh, &amount, &el, &eh, &present) == 9) {
    int result = 0;
    switch (operation) {
      ${functions.map(({ name, signed, outputSigned }, id) => `case ${id}: result = tz_${name}(al, ${signed ? "signed_value(ah)" : "ah"}, bl, ${signed ? "signed_value(bh)" : "bh"}, signed_value(amount), el, ${outputSigned ? "signed_value(eh)" : "eh"}, present); break;`).join("\n")}
    }
    if (!result) { fprintf(stderr, "failed case %zu\\n", index); return 1; }
    index++;
  }
  printf("%zu\\n", index); return 0;
}
`);
  const data = cases.map(({ id, args }) => `${id} ${args.map((value) => typeof value === "bigint" ? bitsOf(value, 64) : value).join(" ")}`).join("\n");
  for (const optimization of [0, 3]) {
    const native = join(temporary, `integers-${optimization}`), wasm = join(temporary, `integers-${optimization}.wasm`);
    execute(clang, [`-O${optimization}`, "-Wno-override-module", host, ir, "-o", native]);
    const result = execute(native, [], { input: data });
    assert.equal(Number(result.stdout.trim()), cases.length);
    cli(["build", input, "--target", "wasm32", `-O${optimization}`, "-o", wasm]);
    const module = new WebAssembly.Module(readFileSync(wasm));
    assert.deepEqual(WebAssembly.Module.imports(module), []);
    const api = new WebAssembly.Instance(module).exports;
    for (const test of cases) assert.equal(api[`tz_${functions[test.id].name}`](...test.args), 1, test.label);
    for (const [index, name] of ["trap_clamp", "trap_pow"].entries()) {
      assert.throws(() => api[`tz_${name}`](), WebAssembly.RuntimeError);
      const trapped = spawnSync(native, [String(index)], { timeout: 10000 });
      assert.ok(trapped.status !== 0 || trapped.signal, `${name} must trap`);
    }
    console.log(`integer intrinsics O${optimization}: ${cases.length} BigInt cases, all widths/signs, native/WASM, no imports`);
  }
} finally {
  rmSync(temporary, { recursive: true, force: true });
}