// Independent references for the `matrix` suite of tests/features.mjs (C11). Expected values come from
// JavaScript numbers (every binary32 operation rounded with Math.fround) and BigInt, never from the
// compiler. A JavaScript multiply and add never fuse, so these are the separately rounded sums that
// `Matrix.mul` promises: total = +0, then total = total + (left(i, k) * right(k, j)) for k = 0, 1, ...
const wrap = (value) => BigInt.asIntN(64, value);

/**
 * Element (i, j) of the product of the fixture's inputs (inner elements per sum), rounded to `bits` (32 or 64)
 * after every operation. `fused` replaces each separately rounded multiply and add by one correctly rounded
 * fused multiply-add, the order of `Matrix.mul_fma`.
 */
export function productEntry(inner, seed, bits, fused, i, j) {
  const round = bits === 32 ? Math.fround : (value) => value;
  const pool = [1e16, 1, -1e16, 3, -3, 0.5, -0.25, 1e-38, bits === 32 ? 1e-45 : 5e-324, -0].map(round);
  let total = 0;
  for (let k = 0; k < inner; k++) {
    const a = pool[(i * 37 + k * 11 + seed) % 10];
    const b = pool[(k * 13 + j * 29 + seed + 3) % 10];
    total = fused ? fusedMultiplyAdd(a, b, total, bits) : round(total + round(a * b));
  }
  return total;
}

/** The rows x cols product, flat and row-major. */
export function shapedProduct(rows, inner, cols, seed, bits, fused = false) {
  const out = [];
  for (let i = 0; i < rows; i++) for (let j = 0; j < cols; j++) out.push(productEntry(inner, seed, bits, fused, i, j));
  return out;
}

/** The n x (n + 2) product of the fixture's inputs for an n x (n + 1) left operand. */
export function matrixProduct(n, seed, bits) {
  return shapedProduct(n, n + 1, n + 2, seed, bits);
}

// A finite, nonzero binary64 value as [signed integer mantissa, exponent] with value = mantissa * 2^exponent.
function split(value) {
  const view = new DataView(new ArrayBuffer(8));
  view.setFloat64(0, value);
  const bits = view.getBigUint64(0);
  const field = Number((bits >> 52n) & 0x7ffn);
  const fraction = bits & ((1n << 52n) - 1n);
  const mantissa = field === 0 ? fraction : fraction | (1n << 52n);
  return [bits >> 63n ? -mantissa : mantissa, field === 0 ? -1074 : field - 1075];
}

/**
 * a * b + c rounded once to nearest-even in binary64 or binary32 (`bits`), computed exactly with BigInt.
 * The inputs are finite. The sign of an exact zero follows IEEE 754: negative only when the product and the
 * addend are both negative zeros.
 */
export function fusedMultiplyAdd(a, b, c, bits) {
  const precision = bits === 32 ? 24 : 53;
  const minimum = bits === 32 ? -126 : -1022;
  const terms = [];
  if (a !== 0 && b !== 0) {
    const [left, leftExponent] = split(a);
    const [right, rightExponent] = split(b);
    terms.push([left * right, leftExponent + rightExponent]);
  }
  if (c !== 0) terms.push(split(c));
  if (terms.length === 0) return Object.is(a * b, -0) && Object.is(c, -0) ? -0 : 0;
  const lowest = Math.min(...terms.map(([, exponent]) => exponent));
  const exact = terms.reduce((sum, [mantissa, exponent]) => sum + (mantissa << BigInt(exponent - lowest)), 0n);
  if (exact === 0n) return 0;
  const magnitude = exact < 0n ? -exact : exact;
  const leading = lowest + magnitude.toString(2).length - 1;
  const quantum = Math.max(leading - (precision - 1), minimum - (precision - 1));
  const shift = quantum - lowest;
  let mantissa;
  if (shift > 0) {
    mantissa = magnitude >> BigInt(shift);
    const remainder = magnitude & ((1n << BigInt(shift)) - 1n);
    const half = 1n << BigInt(shift - 1);
    if (remainder > half || (remainder === half && (mantissa & 1n))) mantissa += 1n;
  } else {
    mantissa = magnitude << BigInt(-shift);
  }
  const value = Number(mantissa) * 2 ** quantum;
  return (exact < 0n ? -1 : 1) * (bits === 32 ? Math.fround(value) : value);
}

// Known results guard the reference itself. 0.1 * 10 - 1 keeps the rounding error of 0.1 * 10, where a
// separate multiply gives exactly 1 and so 0; the other cases are exact by construction.
{
  const check = (actual, expected) => { if (!Object.is(actual, expected)) throw new Error(`fused multiply-add reference: ${actual} != ${expected}`); };
  check(fusedMultiplyAdd(0.1, 10, -1, 64), 5.551115123125783e-17);
  check(fusedMultiplyAdd(1 + 2 ** -29, 1, 0, 64), 1 + 2 ** -29);
  check(fusedMultiplyAdd(-(1 + 2 ** -30), 1 + 2 ** -30, 1 + 2 ** -29, 64), -(2 ** -60));
  check(fusedMultiplyAdd(-(1 + 2 ** -13), 1 + 2 ** -13, 1 + 2 ** -12, 32), -(2 ** -26));
  check(fusedMultiplyAdd(-0, 1, -0, 64), -0);
  check(fusedMultiplyAdd(-0, 1, 0, 64), 0);
  check(fusedMultiplyAdd(5e-324, 0.5, 0, 64), 0);
  check(fusedMultiplyAdd(5e-324, 0.75, 0, 64), 5e-324);
  check(fusedMultiplyAdd(1e308, 10, -1e308, 64), Infinity);
  check(fusedMultiplyAdd(3, 5, 0.5, 32), 15.5);
}

/** The wrapping i64 checksum `total * 31 + x` over the product of the integer inputs. */
export function integerProduct(n, seed) {
  const multiplier = 6364136223846793005n;
  const left = (i, k) => wrap((BigInt(i) * 31n + BigInt(k) * 17n + BigInt(seed)) * multiplier);
  const right = (k, j) => wrap((BigInt(k) * 31n + BigInt(j) * 17n + BigInt(seed) + 1n) * multiplier);
  let checksum = 0n;
  for (let i = 0; i < n; i++) {
    for (let j = 0; j < n + 2; j++) {
      let total = 0n;
      for (let k = 0; k < n + 1; k++) total = wrap(total + wrap(left(i, k) * right(k, j)));
      checksum = wrap(checksum * 31n + total);
    }
  }
  return checksum;
}

/** transpose, row, get (inside and outside), map, fold, as_array, to_array and of_array folded into one checksum. */
export function shapes(n) {
  const rows = n;
  const cols = n + 2;
  const element = (i, j) => BigInt(i * 100 + j);
  let total = 7n;
  for (let j = 0; j < cols; j++) for (let i = 0; i < rows; i++) total = wrap(total * 31n + element(i, j));
  for (let i = 0; i < rows; i++) {
    let sum = 0n;
    for (let j = 0; j < cols; j++) sum += element(i, j);
    total = wrap(total * 17n + sum);
  }
  let doubled = 0n;
  for (let i = 0; i < rows; i++) for (let j = 0; j < cols; j++) doubled += element(i, j) * 2n;
  total = wrap(total * 13n + doubled);
  const get = (i, j) => (i >= 0 && i < rows && j >= 0 && j < cols ? element(i, j) : -1n);
  total = wrap(total * 11n + get(n - 1, n + 1) + get(n, 0) * 3n + get(0, n + 2) * 5n + get(-1, 0) * 7n);
  return wrap(total * 7n + BigInt(rows) * 3n + BigInt(cols) + BigInt(rows * cols));
}

/** Matrices whose elements own memory: arrays of i64 and strings. */
export function ownedElements(n) {
  let total = 0n;
  for (let i = 0; i < n; i++) {
    for (let j = 0; j < n; j++) {
      const length = i + j;
      total = wrap(total * 3n + BigInt(length) + (length > 0 ? BigInt((length - 1) * 3 + i) : 0n));
    }
  }
  if (n > 0) total = wrap(total * 5n + BigInt(n));
  for (let i = 0; i < n; i++) total = wrap(total * 11n + BigInt(`${i}:1`.length));
  for (let row = 0; row < n; row++) for (let col = 0; col < n; col++) total = wrap(total * 7n + BigInt(row + col));
  return wrap(total * 13n + BigInt(n * 2));
}

// The contraction probes multiply [1 + 2^-29, -(1 + 2^-30)] by [1, 1 + 2^-30] (binary64; 2^-12 and 2^-13 for
// binary32). The first product is exact and the second rounds to its negation, so separately rounded
// steps give +0 while a fused multiply-add of the second step would leave -2^-60 (the cases expect 0).
const negativeZero = (value) => (Object.is(value, -0) ? 1n : 0n);

const entryIndexes = (n) => {
  const count = n * (n + 2);
  return n <= 3 ? Array.from({ length: count }, (_, index) => index) : [0, Math.floor(count / 2), count - 1];
};

export const cases = [
  ...[0, 1, 2, 7, 16].flatMap((n) => [0, 5].map((seed) => ["int_mul", [BigInt(n), BigInt(seed)], integerProduct(n, seed)])),
  ...[[64, "f64_entry"], [32, "f32_entry"]].flatMap(([bits, name]) => [1, 2, 3, 7, 16].flatMap((n) => [0, 1, 7].flatMap((seed) => {
    const product = matrixProduct(n, seed, bits);
    return entryIndexes(n).map((index) => [name, [BigInt(n), BigInt(seed), BigInt(index)], product[index]]);
  }))),
  ["contraction_probe64", [1], 0],
  ["contraction_probe32", [1], 0],
  // Signs of zero (1 for -0): an empty inner dimension, (-0) * 1, -0 + -0, and a transposed -0.
  ["zero_signs", [], negativeZero(0) + 2n * negativeZero(0 + (-0 * 1)) + 4n * negativeZero(-0 + -0) + 8n * negativeZero(-0)],
  // NaN from a NaN input, from infinity * 0, and from infinity + -infinity.
  ["nan_count", [], BigInt([0 + NaN * 1, 0 + Infinity * 0, Infinity + -Infinity].filter(Number.isNaN).length)],
  ...[0, 1, 3, 16].map((n) => ["shapes", [BigInt(n)], shapes(n)]),
  ...[0, 1, 5].map((n) => ["owned_elements", [BigInt(n)], ownedElements(n)]),
  ...kernelCases(),
];

// `Matrix.set` replaces elements in place; the checksum folds the whole matrix afterwards.
function setChecksum(n) {
  const cells = Array.from({ length: n }, (_, i) => Array.from({ length: n + 1 }, (_, j) => BigInt(i * 100 + j)));
  for (let step = 0; step < n * 3; step++) cells[step % n][(step * 7) % (n + 1)] = BigInt(step * 1000 + 1);
  let total = 7n;
  for (const row of cells) for (const value of row) total = wrap(total * 31n + value);
  return total;
}

// The kernels: textbook and fused sums against the loop oracles in the fixture, the parallel products against
// the sequential ones, single elements against the references above, and the contraction probes.
function kernelCases() {
  const cases = [];
  for (const [name, seeds] of [["oracle_mismatches64", [0, 1, 7]], ["oracle_mismatches32", [0, 1, 7]]]) {
    for (const seed of seeds) for (const span of [10, 13]) cases.push([name, [BigInt(seed), BigInt(span)], 0n]);
  }
  // Shapes around the one-chunk limit of 2^20 multiply-adds: 129 x 128 x 64 and 3 x 500 x 1500 have several
  // chunks, 64 x 64 x 64 and 2000 x 5 x 7 have one, and empty dimensions have none.
  const shapeList = [[0, 5, 5], [5, 0, 5], [5, 5, 0], [1, 1, 1], [7, 9, 5], [64, 64, 64], [129, 128, 64], [3, 500, 1500], [2000, 5, 7], [129, 1, 1000]];
  for (const name of ["parallel_mismatches64", "parallel_mismatches32"]) {
    for (const [rows, inner, cols] of shapeList) for (const span of [10, 13]) cases.push([name, [rows, inner, cols, 1, span].map(BigInt), 0n]);
  }
  for (const [bits, name] of [[64, "kernel_entry64"], [32, "kernel_entry32"]]) {
    for (const seed of [0, 1, 7]) {
      for (const mode of [0, 1, 2, 3]) {
        const [rows, inner, cols] = [2, 3, 4];
        for (let index = 0; index < rows * cols; index++) {
          cases.push([name, [rows, inner, cols, seed, mode, index].map(BigInt), productEntry(inner, seed, bits, mode >= 2, Math.floor(index / cols), index % cols)]);
        }
      }
    }
    for (const [rows, inner, cols, entries] of [[129, 128, 64, [0, 127 * 64 + 5, 128 * 64, 129 * 64 - 1]], [3, 500, 1500, [0, 1500 + 7, 2 * 1500 + 1499]]]) {
      for (const mode of [0, 1, 2, 3]) {
        for (const index of entries) {
          cases.push([name, [rows, inner, cols, 0, mode, index].map(BigInt), productEntry(inner, 0, bits, mode >= 2, Math.floor(index / cols), index % cols)]);
        }
      }
    }
  }
  // Probe inputs: separate roundings give +0, fused multiply-adds leave the rounding error of the second product.
  for (const mode of [0, 1, 2, 3]) {
    cases.push(["kernel_probe64", [1, BigInt(mode)], mode >= 2 ? -(2 ** -60) : 0]);
    cases.push(["kernel_probe32", [1, BigInt(mode)], mode >= 2 ? -(2 ** -26) : 0]);
  }
  for (const [rows, inner, cols] of [[0, 3, 4], [7, 3, 11], [129, 128, 64], [3, 500, 1500]]) cases.push(["integer_parallel", [rows, inner, cols, 5].map(BigInt), 0n]);
  for (const n of [1, 2, 5, 9]) cases.push(["set_checksum", [BigInt(n)], setChecksum(n)]);
  return cases;
}

export const traps = [
  ["of_array_length", []], ["add_shape", []], ["mul_shape", []],
  ["at_col", [3n]], ["at_col", [-1n]], ["at_row", [2n]], ["at_row", [-1n]],
  ["row_index", [2n]], ["row_index", [-1n]],
  ["negative_dims", []], ["count_overflow", []], ["mul_count_overflow", []], ["alloc_size", []],
  ["set_row", [2n]], ["set_row", [-1n]], ["set_col", [3n]], ["set_col", [-1n]],
];
