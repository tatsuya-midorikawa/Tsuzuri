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

// The size that clang gives each alloca type that a lifetime marker brackets, from a probe function per type that
// LLVM folds to a constant, against the size in the marker.
function checkMarkerSizes(ir, targetArguments) {
  const sizes = new Map();
  let allocas = new Map();
  for (const line of ir.split("\n")) {
    if (line.startsWith("define ")) allocas = new Map();
    const alloca = line.match(/^\s*(%[\w.$]+) = alloca (.+), align 16$/);
    if (alloca) allocas.set(alloca[1], alloca[2]);
    const marker = line.match(/^\s*call void @llvm\.lifetime\.(?:start|end)\.p0\(i64 (\d+), ptr (%[\w.$]+)\)$/);
    if (!marker) continue;
    const type = allocas.get(marker[2]);
    assert.ok(type, `a lifetime marker points at an alloca of its function: ${line}`);
    if (!sizes.has(type)) sizes.set(type, new Set());
    sizes.get(type).add(BigInt(marker[1]));
  }
  assert.ok(sizes.size > 0, "the IR has lifetime markers");
  // A string buffer and a closure environment have an alloca and no markers (llvm_frame.rs, Frame::Buffer).
  assert.ok(!sizes.has("[256 x i8]"), "a string buffer has lifetime markers");
  const types = [...sizes.keys()];
  const probes = types.map((type, index) =>
    `define i64 @tz_probe_${index}() {\nentry:\n  ret i64 ptrtoint (ptr getelementptr (${type}, ptr null, i32 1) to i64)\n}\n`);
  const probe = join(temporary, "probe.ll");
  writeFileSync(probe, `${ir}\n${probes.join("")}`);
  const folded = execute(clang, ["-x", "ir", "-Wno-override-module", ...targetArguments, "-O1", "-S", "-emit-llvm", probe, "-o", "-"]).stdout;
  types.forEach((type, index) => {
    const body = folded.match(new RegExp(`define [^\\n]*@tz_probe_${index}\\(\\)[^\\n]*\\{\\n([\\s\\S]*?)\\n\\}`));
    const size = BigInt(body?.[1].match(/ret i64 (\d+)/)?.[1] ?? -1);
    assert.deepEqual([...sizes.get(type)], [size], `marker sizes of the alloca ${type}`);
  });
}

function rangeReference(first, step, last, unsigned = false) {
  let total = 0n;
  for (let n = first; step > 0n ? n <= last : n >= last; n += step) {
    if (n < (unsigned ? 0n : min) || n > (unsigned ? umax : max)) break;
    total = wrap(total + n);
  }
  return total;
}

const cases = [
  ["break_while", [0n], 0n],
  ["break_while", [100n], 42n],
  ["continue_endpoints", [], 16n],
  ["break_nested", [], 309n],
  ["break_sources", [1], 2n],
  ["break_sources", [0], 5n],
  ["break_match_owned", [], 44n],
  ["break_skips_trap", [1], 42n],
  ["continue_temporaries", [4096n], 4096n],
  ["nested_jump_temporaries", [], 10n],
  ["branch_jump_temporaries", [0], 4n],
  ["branch_jump_temporaries", [1], 0n],
  ["match_jump_temporaries", [0], 4n],
  ["match_jump_temporaries", [1], 0n],
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
  ["lambda_nested", [], 9n],
  ["inline_forms", [], 42n],
  ["lambda_guard_where_order", [], 6n],
  ["lambda_guard_where_owned", [], 14n],
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
for (const diff of [-11, -10, -6, -5, 0, 5, 6, 10, 11]) {
  cases.push(["lambda_guard_score", [20 + diff, 20], Math.abs(diff) > 10 ? 2n : Math.abs(diff) > 5 ? 1n : 0n]);
}
for (const value of [min, -3n, -2n, -1n, 0n, 1n, 2n, 3n, 42n, max]) {
  cases.push(["active_lambda_guards", [value], value % 2n === 0n ? 0n : 1n]);
}
for (const count of [0n, 1n, 2n, 3n, 10n, 4096n]) {
  let total = 0n;
  for (let index = 0n; index < count; index++) if (index % 3n !== 0n) total += index;
  cases.push(["continue_sum", [count], total]);
  cases.push(["break_owned", [count], (count < 4n ? count : 4n) * 4n]);
  cases.push(["continue_owned", [count], count / 2n * 4n]);
}
for (let kind = 0; kind <= 10; kind++) cases.push(["break_temporaries", [kind], 1n]);
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
// PR02: loops with the literal step 1 or -1 that end at the extremes of their type. The values, not only
// the iteration count, are compared with a BigInt reference, so a loop that skips or repeats an end fails.
function edgeReference(first, last, down, step) {
  let total = 0n;
  for (let n = first; down ? n >= last : n <= last; n += down ? -1n : 1n) total = step(total, n);
  return total;
}
for (const [first, last, down] of [
  [max - 3n, max, 0], [min + 3n, min, 1], [max, max, 0], [min, min, 1], [min, min, 0], [max, max, 1],
  [3n, 1n, 0], [1n, 3n, 1], [-2n, 3n, 0], [3n, -2n, 1], [max - 1n, max, 1], [min + 1n, min, 0],
]) {
  cases.push(["range_edges", [first, last, down], edgeReference(first, last, down === 1,
    (total, n) => n % 2n === 0n ? total : wrap(total + n * 3n))]);
}
for (const [first, last] of [[umax - 3n, umax], [umax, umax], [(1n << 63n) - 2n, (1n << 63n) + 1n], [5n, 3n], [0n, 3n], [umax - 1n, umax]]) {
  cases.push(["range_edges_u64", [first, last], BigInt.asIntN(64, edgeReference(first, last, false,
    (total, n) => BigInt.asUintN(64, total + BigInt.asUintN(64, (n ^ 5n) * 7n))))]);
}
for (const [first, last] of [[4294967292, 4294967295], [0, 3], [3, 1], [4294967295, 4294967295], [2147483646, 2147483649]]) {
  cases.push(["range_edges_u32", [first, last], edgeReference(BigInt(first), BigInt(last), false,
    (total, n) => total + n * 3n)]);
}
for (const [first, last, down] of [
  [124, 127, 0], [125, 127, 0], [127, 127, 0], [-125, -128, 1], [-128, -128, 0], [-128, -128, 1], [127, 127, 1],
  [3, 1, 0], [1, 3, 1], [-3, 4, 0], [4, -3, 1], [126, 127, 1], [-127, -128, 0],
]) {
  cases.push(["range_edges_i8", [first, last, down], edgeReference(BigInt(first), BigInt(last), down === 1,
    (total, n) => wrap(total * 31n + n))]);
}
{
  const i128max = (1n << 127n) - 1n, i128min = -(1n << 127n);
  let count = 0n, low = 0n;
  for (let n = i128max - 3n; n <= i128max; n++) { count++; low += BigInt.asIntN(64, n); }
  for (let n = i128min + 3n; n >= i128min; n--) { count++; low += BigInt.asIntN(64, n); }
  cases.push(["range_edges_i128", [], wrap(count * 1000n + low)]);
}
// PR02 D10: the builtin forms `Add.add`, `Sub.sub` and `Mul.mul` wrap like the operators and keep the order of evaluation.
for (const seed of [0n, 1n, -1n, max, min, 123456789n, -987654321n, 65535n, 4294967295n]) {
  const a = wrap(seed + max), b = wrap(a - seed), c = wrap(seed * seed);
  const d = BigInt.asIntN(8, BigInt.asIntN(8, seed) + 127n);
  const e = BigInt.asUintN(16, BigInt.asUintN(16, seed) * 65535n);
  const f = BigInt.asUintN(32, BigInt.asUintN(32, seed) - 1n);
  cases.push(["builtin_wrap", [seed], wrap(a + b * 3n + c + d + e + f)]);
}
cases.push(["builtin_order", [], 123007n]);
// PR02: arithmetic that a range proof covers, and at the edge of the range, where a flag would be wrong.
for (const n of [0n, 1n, 255n, 256n, 511n, 65535n, 0x7fffn << 8n | 100n, -1n, min, max]) {
  const weight = (n >> 8n) & 255n;
  let total = 0n;
  for (let i = 0n; i <= (n & 255n); i++) total = wrap(total + wrap(wrap(i * 3n + 1n) * weight));
  cases.push(["affine_sum", [n], total]);
}
for (const x of [0n, 1n, max - 1n, max, -1n, min + 1n, min]) {
  for (const y of [0n, 1n, 2n, max, -1n, min]) {
    const a = x >= 0n ? wrap(x + 1n) : wrap(x - 1n), b = y > 0n ? wrap(y - 1n) : wrap(y + 1n);
    cases.push(["wrap_edges", [x, y], wrap(a * 3n + b)]);
  }
}
for (const x of [-128, -1, 0, 1, 63, 64, 127]) {
  for (const y of [-128, -1, 0, 1, 126, 127]) {
    const s = BigInt.asIntN(8, BigInt.asIntN(8, BigInt(x & 63) * 2n) + 1n), t = BigInt.asIntN(8, BigInt(y & 127) + 1n);
    cases.push(["narrow_wrap", [x, y], BigInt.asIntN(8, s + t) * 1000n + t]);
  }
}
for (const x of [0, 1, 2, 127, 128, 255]) {
  for (const y of [0, 1, 2, 127, 128, 255]) {
    const a = x !== 0 ? (x - 1) & 255 : 255, b = (y - 1) & 255;
    cases.push(["unsigned_sub", [x, y], BigInt((a + b) & 255)]);
  }
}
for (const x of [-32768, -32767, -256, -1, 0, 1, 127, 128, 255, 256, 32767]) {
  const wide = BigInt(x) * BigInt(x), narrow = BigInt.asIntN(8, BigInt.asIntN(8, BigInt(x & 255)) * 2n);
  cases.push(["mixed_width", [x], wide * 1000n + narrow]);
}
for (const n of [0n, 1n, 2n, 100n, 101n, 4096n]) {
  let breaking = 0n, skipping = 0n;
  for (let i = 0n; i < n && i !== 100n; i++) breaking += i;
  for (let i = 0n; i < n; i++) if ((i * 7n - 3n) % 3n !== 0n) skipping += i * 7n - 3n;
  cases.push(["each_break", [n], breaking], ["each_continue", [n], skipping]);
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
// PM09: stack frames of literals, string buffers and closure environments (Frames.tz). Each expected value is
// computed from the definition in the fixture with BigInt, not from the compiler's output; `live == 0` after
// each case checks that no frame is freed and no copy leaks, and the ASan run checks the lifetime markers.
const framesFold = (seed, values, factor) => values.reduce((total, v) => wrap(total * factor + v), seed);
for (const n of [-5n, -1n, 0n, 1n, 7n, 1n << 62n]) cases.push(["frames_disjoint", [n], n > 0n ? 4n + n : 4n - n]);
for (const n of [0n, 1n, 1000n, 100000n]) cases.push(["frames_loop", [n], 3n * n]);
for (const n of [-3n, 0n, 1n, 10n, max - 2n, min]) {
  cases.push(["frames_moved", [n], wrap(framesFold(1n, [n, wrap(n + 1n), wrap(n + 2n)], 7n) + 2n)]);
  cases.push(["frames_merge", [n], framesFold(0n, n > 0n ? [n, wrap(n + 1n)] : [wrap(n - 1n), wrap(n - 2n)], 10n)]);
  const rows = [[n, wrap(n + 1n)], [wrap(n + 2n)], [wrap(n * 2n), wrap(n * 3n), wrap(n * 4n)]];
  cases.push(["frames_nested", [n], wrap(framesFold(0n, rows.flat(), 3n) + 3n)]);
}
for (const n of [0n, 1n, 2n, 3n, 10n, 100n, 4096n]) {
  let jumps = 0n, buffers = 0n, closures = 0n;
  for (let i = 0n; i < n; i++) {
    if (i % 3n !== 0n) {
      if (jumps > 1000n) break;
      jumps += 3n + i;
    }
  }
  for (let i = 0n; i < n; i++) buffers += (i % 2n === 0n ? 3n : 4n) + 97n;
  for (let i = 0n; i < n; i++) {
    const a = i + 1n, b = i * 2n, add = (k) => k + a + b;
    closures += add(1n) + add(2n);
  }
  cases.push(["frames_jumps", [n], jumps], ["frames_buffers", [n], buffers], ["frames_closures", [n], closures]);
}
function framesSpread(n) {
  if (n <= 0n) return 0n;
  let total = 0n;
  if (n % 2n === 0n) for (let k = 0n; k < 64n; k++) total = wrap(total * 31n + n + k);
  else for (let k = 2n; k < 64n; k++) total = wrap(total * 17n + n * k);
  return wrap(total + framesSpread(n - 1n));
}
for (const n of [-1n, 0n, 1n, 2n, 3n, 40n, 101n]) cases.push(["frames_recursive", [n], framesSpread(n)]);
// The frames of the arms of a match or an if hold one value: the test of one frame must not pass for another.
const armValues = new Map([[0n, [1n]], [1n, [2n]], [2n, [4n]], [3n, [8n, 16n]], [4n, [32n, 64n]]]);
for (const n of [-1n, 0n, 1n, 2n, 3n, 4n, 5n, 9n, max]) {
  cases.push(["frames_arms", [n], framesFold(0n, armValues.get(n) ?? [3n, 5n], 7n)]);
  cases.push(["frames_branches", [n], framesFold(0n, n > 0n ? [n, 100n] : [wrap(n + 1n), 200n], 7n)]);
  cases.push(["frames_reassigned", [n], framesFold(0n, n > 0n ? [wrap(n * 2n), wrap(n * 3n)] : [wrap(n + 10n), wrap(n + 11n)], 7n)]);
}
// What `Utf8String.from_bytes` makes of the bytes of each arm of `frames_validate`, which V8 decides: the length of
// valid UTF-8, -1 for the rest.
const validateArms = [[], [0], [128], [192, 128], [237, 160, 128], [244, 143, 191, 191], [244, 144, 128, 128],
  [239, 187, 191], [240, 159, 152], [65, 0, 66]];
const utf8Length = (bytes) => {
  try { new TextDecoder("utf-8", { fatal: true }).decode(Uint8Array.from(bytes)); return BigInt(bytes.length); } catch { return -1n; }
};
for (const [which, bytes] of validateArms.entries()) cases.push(["frames_validate", [which], utf8Length(bytes)]);
// Four locals of the same length that an `if` chooses between keep their markers (all four start before the
// `if`, so LLVM gives them four slots); a miss would show as the bytes of another local.
const localBytes = [[0], [128], [65], [200]];
for (let which = 0; which < 6; which++) cases.push(["frames_locals", [which], utf8Length(localBytes[Math.min(which, 3)])]);
// PM09: guaranteed tail calls between the functions of a recursion (TailCalls.tz), at depths that every build
// can run: tests/stack.mjs runs them at depth 10,000,000. The expected values are simulated with BigInt.
function tailLists(n) {
  let xs = [1n, 2n], acc = 0n, ping = true;
  for (let k = n; ; k--, ping = !ping) {
    if (k === 0n) return wrap(acc + BigInt(xs.length));
    const fold = (factor) => xs.reduce((s, v) => wrap(s * factor + v), 0n);
    [acc, xs] = ping ? [wrap(acc + fold(3n)), [k, k + 1n]] : [wrap(acc + fold(5n)), [k * 2n, k, k + 3n]];
  }
}
function tailTexts(n) {
  let acc = 0n, even = true;
  for (let k = n; ; k--, even = !even) {
    const length = even ? (k % 2n === 0n ? 3n : 4n) : (k % 2n === 0n ? 5n : 2n);
    if (k === 0n) return acc + length;
    acc += length;
  }
}
function tailSelfAndGroup(n) {
  let acc = 0n, a = true;
  for (let k = n; k !== 0n; k--) {
    if (a) { if (k % 2n === 0n) acc += 1n; else { acc += 10n; a = false; } }
    else { acc += 100n; a = true; }
  }
  return acc;
}
for (const n of [0n, 1n, 2n, 3n, 4n, 5n, 7n, 100n, 999n, 1000n]) {
  let cycle = 0n;
  for (let i = 0n; i < n; i++) cycle += [1n, 2n, 3n][Number(i % 3n)];
  cases.push(["tail_cycle", [n], cycle], ["tail_lists", [n], tailLists(n)], ["tail_texts", [n], tailTexts(n)]);
  cases.push(["tail_words", [n], n % 2n === 0n ? 4n : 3n]);
  cases.push(["tail_self_and_group", [n], tailSelfAndGroup(n)]);
  cases.push(["tail_pipe", [n], n % 2n === 0n ? 1 : 0]);
  for (const value of [0n, 21n, -7n]) cases.push(["tail_lend", [value, n], n % 2n === 0n ? value : value * 2n]);
}
const traps = ["trap_step", "trap_pattern", "trap_lambda", "trap_for_active", "trap_fx_active", "recursion_trap_checked", "break_traps_without_jump"];
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
  const declaredNames = declarations.map((line) => line.match(/@([^\s(]+)\(/)[1]);
  assert.equal(new Set(declaredNames).size, declaredNames.length, "intrinsic declarations are deduplicated");
  // PM09: every stack frame of an array or list literal is bracketed by lifetime markers whose size is
  // the size of its alloca on the target, which a sanitizer reads as the extent of the marker.
  assert.match(sourceIr, /call void @llvm\.lifetime\.start\.p0\(i64 \d+, ptr %v\d+\)/);
  assert.match(sourceIr, /call void @llvm\.lifetime\.end\.p0\(i64 \d+, ptr %v\d+\)/);
  checkMarkerSizes(sourceIr, []);
  const wasmIr = join(temporary, "control-wasm.ll");
  cli(["build", fixture, "--target", "wasm32", "--emit", "llvm", "-o", wasmIr]);
  checkMarkerSizes(readFileSync(wasmIr, "utf8"), ["--target=wasm32-unknown-unknown"]);
  // The vectors of the simd fixture are laid out by each target: wasm32 aligns a 16-byte vector to 16, which the bounds of
  // layout_bound do not say, so a frame that holds a vector has no markers, and every other marker is as large as its alloca.
  for (const [name, buildArguments, targetArguments] of [["native", [], []], ["wasm32", ["--target", "wasm32"], ["--target=wasm32-unknown-unknown"]]]) {
    const simdIr = join(temporary, `simd-${name}.ll`);
    cli(["build", join(root, "tests/fixtures/simd"), "--emit", "llvm", ...buildArguments, "-o", simdIr]);
    checkMarkerSizes(readFileSync(simdIr, "utf8"), targetArguments);
  }
  let trackedIr = sourceIr.replaceAll("@malloc", "@tracked_alloc").replaceAll("@free", "@tracked_free").replaceAll("@realloc", "@tracked_realloc");
  if (sanitizer.length) trackedIr = trackedIr.replace(/ nounwind(?=[^{}\n]* \{)/g, " nounwind sanitize_address");
  writeFileSync(ir, trackedIr);
  const host = join(temporary, "host.c");
  writeFileSync(host, `
#undef NDEBUG
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
void *tracked_realloc(void *value, uint64_t size) {
    if (!value) return size ? tracked_alloc(size) : NULL;
    if (!size) { tracked_free(value); return NULL; }
    uint64_t *p = (uint64_t *)value - 2;
    assert(p[1] == UINT64_C(0x51a110ca7e));
    uint64_t old_size = p[0];
    p = realloc(p, (size_t)size + 16);
    assert(p);
    p[0] = size;
    live = live - old_size + size;
    return p + 2;
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
