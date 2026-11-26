// Independent BigInt reference for NC17 (edition 2028 range family) and NC18 (multi-axis bracket indexing).
// Models Specification §1, §2, §3, and §4 purely in arbitrary-precision BigInt without mirroring compiler lowering.
import assert from "node:assert/strict";

const wrap64 = (v) => BigInt.asIntN(64, v);
const foldValues = (vals) => vals.reduce((acc, v) => wrap64(acc * 31n + v), 0n);
const baseElements = (n) => Array.from({ length: Number(n) }, (_, i) => BigInt(i + 1) * 10n);

const posOf = (n, operand) => (operand.fromEnd ? n - 1n - operand.v : operand.v);

/**
 * Specification §2: converts a slice range form on length `n` into half-open `[lo, hi]`,
 * returning `[lo, hi]` when `0 <= lo <= hi <= n` and `null` when `BoundsCheck` traps.
 */
export function bounds(n, form) {
  let lo;
  let hi;
  switch (form.kind) {
    case "closed":
      lo = posOf(n, form.a);
      hi = posOf(n, form.b) + 1n;
      break;
    case "open":
      lo = posOf(n, form.a);
      hi = posOf(n, form.b);
      break;
    case "open_closed":
      lo = posOf(n, form.a) + 1n;
      hi = posOf(n, form.b) + 1n;
      break;
    case "open_open":
      lo = posOf(n, form.a) + 1n;
      hi = posOf(n, form.b);
      break;
    case "to":
      lo = 0n;
      hi = posOf(n, form.b) + 1n;
      break;
    case "to_open":
      lo = 0n;
      hi = posOf(n, form.b);
      break;
    case "from":
      lo = posOf(n, form.a);
      hi = n;
      break;
    case "from_open":
      lo = posOf(n, form.a) + 1n;
      hi = n;
      break;
    case "whole":
      lo = 0n;
      hi = n;
      break;
    default:
      throw new Error(`unknown range form: ${form.kind}`);
  }
  return 0n <= lo && lo <= hi && hi <= n ? [lo, hi] : null;
}

/**
 * Specification §3: element position (`^k` when `fromEnd` is true, plain `k` otherwise).
 * Returns position in `[0, n)` or `null` when `BoundsCheck` traps.
 */
export function item(n, k, fromEnd = false) {
  const pos = fromEnd ? n - 1n - k : k;
  return 0n <= pos && pos < n ? pos : null;
}

/**
 * Specification §4: stepped slice indices on length `n` (implemented for Tensor/Matrix in NC18).
 * Returns `bigint[]` of selected indices or `null` on trap.
 */
export function stepped(n, form) {
  const s = form.step;
  if (s === 0n) return null;
  if (s > 0n) {
    const lo = form.a === null ? 0n : posOf(n, form.a);
    const hi = form.b === null ? n : posOf(n, form.b) + (form.endClosed ? 1n : 0n);
    if (!(0n <= lo && lo <= hi && hi <= n)) return null;
    const out = [];
    for (let i = lo; i < hi; i += s) out.push(i);
    return out;
  }
  if (!form.endClosed && form.b !== null) return null;
  const last = n - 1n;
  const lo = form.a === null ? 0n : last - posOf(n, form.a);
  const hi = form.b === null ? n : last - posOf(n, form.b) + 1n;
  if (!(0n <= lo && lo <= hi && hi <= n)) return null;
  const stride = -s;
  const out = [];
  for (let i = lo; i < hi; i += stride) out.push(last - i);
  return out;
}

const F = (v) => ({ v: BigInt(v), fromEnd: false });
const E = (v) => ({ v: BigInt(v), fromEnd: true });

// Canonical Specification §1 table (`n = 5`, `xs = [10, 20, 30, 40, 50]`).
export const rows = [
  { label: "xs[^0]", n: 5n, mode: "item", k: 0n, fromEnd: true, expected: 4n, containers: ["array", "fixed", "list", "vec", "string", "utf8string"], tensor: { pending: "NC18" } },
  { label: "xs[^1]", n: 5n, mode: "item", k: 1n, fromEnd: true, expected: 3n, containers: ["array", "fixed", "list", "vec", "string", "utf8string"], tensor: { pending: "NC18" } },
  { label: "ref xs[1..3]", n: 5n, mode: "slice", form: { kind: "closed", a: F(1), b: F(3) }, expected: [1n, 4n], containers: ["array", "fixed"], tensor: { pending: "NC18" } },
  { label: "ref xs[1..<3]", n: 5n, mode: "slice", form: { kind: "open", a: F(1), b: F(3) }, expected: [1n, 3n], containers: ["array", "fixed"], tensor: { pending: "NC18" } },
  { label: "ref xs[1<..3]", n: 5n, mode: "slice", form: { kind: "open_closed", a: F(1), b: F(3) }, expected: [2n, 4n], containers: ["array", "fixed"], tensor: { pending: "NC18" } },
  { label: "ref xs[1<..<3]", n: 5n, mode: "slice", form: { kind: "open_open", a: F(1), b: F(3) }, expected: [2n, 3n], containers: ["array", "fixed"], tensor: { pending: "NC18" } },
  { label: "ref xs[..2]", n: 5n, mode: "slice", form: { kind: "to", b: F(2) }, expected: [0n, 3n], containers: ["array", "fixed"], tensor: { pending: "NC18" } },
  { label: "ref xs[..<2]", n: 5n, mode: "slice", form: { kind: "to_open", b: F(2) }, expected: [0n, 2n], containers: ["array", "fixed"], tensor: { pending: "NC18" } },
  { label: "ref xs[2..]", n: 5n, mode: "slice", form: { kind: "from", a: F(2) }, expected: [2n, 5n], containers: ["array", "fixed"], tensor: { pending: "NC18" } },
  { label: "ref xs[2<..]", n: 5n, mode: "slice", form: { kind: "from_open", a: F(2) }, expected: [3n, 5n], containers: ["array", "fixed"], tensor: { pending: "NC18" } },
  { label: "ref xs[..^1]", n: 5n, mode: "slice", form: { kind: "to", b: E(1) }, expected: [0n, 4n], containers: ["array", "fixed"], tensor: { pending: "NC18" } },
  { label: "ref xs[1..^1]", n: 5n, mode: "slice", form: { kind: "closed", a: F(1), b: E(1) }, expected: [1n, 4n], containers: ["array", "fixed"], tensor: { pending: "NC18" } },
  { label: "ref xs[^2..]", n: 5n, mode: "slice", form: { kind: "from", a: E(2) }, expected: [2n, 5n], containers: ["array", "fixed"], tensor: { pending: "NC18" } },
  { label: "ref xs[^2<..]", n: 5n, mode: "slice", form: { kind: "from_open", a: E(2) }, expected: [3n, 5n], containers: ["array", "fixed"], tensor: { pending: "NC18" } },
  { label: "ref xs[0..^0]", n: 5n, mode: "slice", form: { kind: "closed", a: F(0), b: E(0) }, expected: [0n, 5n], containers: ["array", "fixed"], tensor: { pending: "NC18" } },
  { label: "ref xs[..<^0]", n: 5n, mode: "slice", form: { kind: "to_open", b: E(0) }, expected: [0n, 4n], containers: ["array", "fixed"], tensor: { pending: "NC18" } },
  { label: "ref xs[2..1]", n: 5n, mode: "slice", form: { kind: "closed", a: F(2), b: F(1) }, expected: [2n, 2n], containers: ["array", "fixed"], tensor: { pending: "NC18" } },
  { label: "ref xs[2..<2]", n: 5n, mode: "slice", form: { kind: "open", a: F(2), b: F(2) }, expected: [2n, 2n], containers: ["array", "fixed"], tensor: { pending: "NC18" } },
  { label: "ref xs[3..1]", n: 5n, mode: "slice", form: { kind: "closed", a: F(3), b: F(1) }, expected: null, containers: ["array", "fixed"], tensor: { pending: "NC18" } },
  { label: "ref xs[0..5]", n: 5n, mode: "slice", form: { kind: "closed", a: F(0), b: F(5) }, expected: null, containers: ["array", "fixed"], tensor: { pending: "NC18" } },
  { label: "ref xs[..^5]", n: 5n, mode: "slice", form: { kind: "to", b: E(5) }, expected: [0n, 0n], containers: ["array", "fixed"], tensor: { pending: "NC18" } },
  { label: "ref xs[^5..]", n: 5n, mode: "slice", form: { kind: "from", a: E(5) }, expected: null, containers: ["array", "fixed"], tensor: { pending: "NC18" } },
  { label: "ref t[0 .. 2 .. 4]", n: 5n, mode: "stepped", form: { a: F(0), step: 2n, b: F(4), endClosed: true }, expected: [0n, 2n, 4n], containers: [], pending: "NC18" },
  { label: "ref t[.. 2 ..]", n: 5n, mode: "stepped", form: { a: null, step: 2n, b: null, endClosed: true }, expected: [0n, 2n, 4n], containers: [], pending: "NC18" },
  { label: "ref t[.. -1 ..]", n: 5n, mode: "stepped", form: { a: null, step: -1n, b: null, endClosed: true }, expected: [4n, 3n, 2n, 1n, 0n], containers: [], pending: "NC18" },
  { label: "ref t[3 .. -1 .. 1]", n: 5n, mode: "stepped", form: { a: F(3), step: -1n, b: F(1), endClosed: true }, expected: [3n, 2n, 1n], containers: [], pending: "NC18" },
];

for (const row of rows) {
  if (row.mode === "item") {
    assert.deepEqual(item(row.n, row.k, row.fromEnd), row.expected, row.label);
  } else if (row.mode === "slice") {
    assert.deepEqual(bounds(row.n, row.form), row.expected, row.label);
  } else if (row.mode === "stepped") {
    assert.deepEqual(stepped(row.n, row.form), row.expected, row.label);
  }
  assert.ok(row.containers.length > 0 || row.pending === "NC18", `untested row without pending marker: ${row.label}`);
}

const INT_TYPES = [
  { name: "i32", min: -(1n << 31n), max: (1n << 31n) - 1n, encode: (v) => [wrap64(v)] },
  { name: "i64", min: -(1n << 63n), max: (1n << 63n) - 1n, encode: (v) => [wrap64(v)] },
  { name: "i64u", min: 0n, max: (1n << 64n) - 1n, encode: (v) => [wrap64(v)] },
  {
    name: "i128",
    min: -(1n << 127n),
    max: (1n << 127n) - 1n,
    encode: (v) => {
      const u = BigInt.asUintN(128, v);
      return [wrap64(u >> 64n), wrap64(u & ((1n << 64n) - 1n))];
    },
  },
];

function sampleValues(ty, n) {
  const candidates = [0n, 1n, n - 1n, n, n + 1n, -1n, ty.max, ty.min];
  if (ty.name === "i64u") candidates.push(1n << 63n, (1n << 64n) - 1n);
  if (ty.name === "i128") candidates.push(1n << 63n, -(1n << 63n) - 1n);
  const uniq = [];
  const seen = new Set();
  for (const v of candidates) {
    if (v < ty.min || v > ty.max) continue;
    const key = v.toString();
    if (!seen.has(key)) {
      seen.add(key);
      uniq.push(v);
    }
  }
  return uniq;
}

const FORMS = [
  { fn: "closed", arity: 2, build: (a, b) => ({ kind: "closed", a: F(a), b: F(b) }) },
  { fn: "open", arity: 2, build: (a, b) => ({ kind: "open", a: F(a), b: F(b) }) },
  { fn: "open_closed", arity: 2, build: (a, b) => ({ kind: "open_closed", a: F(a), b: F(b) }) },
  { fn: "open_open", arity: 2, build: (a, b) => ({ kind: "open_open", a: F(a), b: F(b) }) },
  { fn: "to", arity: 1, build: (b) => ({ kind: "to", b: F(b) }) },
  { fn: "to_open", arity: 1, build: (b) => ({ kind: "to_open", b: F(b) }) },
  { fn: "from", arity: 1, build: (a) => ({ kind: "from", a: F(a) }) },
  { fn: "from_open", arity: 1, build: (a) => ({ kind: "from_open", a: F(a) }) },
  { fn: "drop_end", arity: 1, build: (b) => ({ kind: "to", b: E(b) }) },
  { fn: "from_end_start", arity: 1, build: (a) => ({ kind: "from", a: E(a) }) },
  { fn: "from_end_start_open", arity: 1, build: (a) => ({ kind: "from_open", a: E(a) }) },
  { fn: "between_from_end", arity: 2, build: (a, b) => ({ kind: "closed", a: F(a), b: E(b) }) },
  { fn: "until_from_end", arity: 1, build: (b) => ({ kind: "to_open", b: E(b) }) },
];

export const cases = [];
export const traps = [];

const seenCalls = new Set();
function recordCall(isTrap, fnName, args, expected) {
  const key = `${isTrap ? "T" : "C"}:${fnName}:${args.map(String).join(",")}`;
  if (seenCalls.has(key)) return;
  seenCalls.add(key);
  if (isTrap) {
    traps.push([fnName, args, "index out of bounds"]);
  } else {
    cases.push([fnName, args, expected]);
  }
}

// Keep trap process count bounded on Windows while covering every form, integer type, container,
// length n in {0, 1, 5}, and out-of-bounds value class (-1, n, n+1, MIN, MAX, 2^64-1, >i64).
for (const ty of INT_TYPES) {
  for (const n of [0n, 1n, 5n]) {
    const xs = baseElements(n);
    const vals = sampleValues(ty, n);

    for (const v of vals) {
      const pos = item(n, v, true);
      const isTrap = pos === null;
      const expected = isTrap ? 0n : xs[Number(pos)];
      const itemContainers = n === 5n
        ? ["array", "fixed", "list", "vec", "string", "utf8string"]
        : ["array", "list", "vec", "string", "utf8string"];
      for (const container of itemContainers) {
        if (isTrap && container !== "array" && !(container === "fixed" && (v === 5n || v === ty.max)) && v !== n && v !== -1n) {
          continue;
        }
        const fnName = container === "fixed"
          ? `fixed_item_from_end_${ty.name}`
          : `${container}_item_from_end_${ty.name}`;
        const args = container === "fixed" ? ty.encode(v) : [n, ...ty.encode(v)];
        recordCall(isTrap, fnName, args, expected);
      }
    }

    for (const form of FORMS) {
      const argTuples = [];
      if (form.arity === 1) {
        for (const v of vals) argTuples.push([v]);
      } else {
        const anchor = 0n >= ty.min && 0n <= ty.max ? 0n : vals[0];
        for (const v of vals) {
          argTuples.push([anchor, v]);
          argTuples.push([v, anchor]);
        }
        if (n === 5n) {
          for (const [a, b] of [[1n, 3n], [2n, 1n], [2n, 2n], [3n, 1n], [0n, 5n]]) {
            if (a >= ty.min && a <= ty.max && b >= ty.min && b <= ty.max) argTuples.push([a, b]);
          }
        }
      }

      for (const tuple of argTuples) {
        const range = bounds(n, form.build(...tuple));
        const isTrap = range === null;
        const expected = isTrap ? 0n : foldValues(xs.slice(Number(range[0]), Number(range[1])));
        const encoded = tuple.flatMap((v) => ty.encode(v));

        if (!isTrap || ty.name === "i64" || (n === 5n && tuple.some((v) => v === ty.max || v === ty.min || v === -1n || v === 5n))) {
          recordCall(isTrap, `array_${form.fn}_${ty.name}`, [n, ...encoded], expected);
        }
        if (n === 5n && (!isTrap || ty.name === "i64" || tuple.some((v) => v === ty.max || v === -1n))) {
          recordCall(isTrap, `fixed_${form.fn}_${ty.name}`, encoded, expected);
        }
      }
    }
  }
}
