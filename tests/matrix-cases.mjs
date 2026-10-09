// Independent references for the `matrix` suite of tests/features.mjs (C11). Expected values come from
// JavaScript numbers (every binary32 operation rounded with Math.fround) and BigInt, never from the
// compiler. A JavaScript multiply and add never fuse, so these are the separately rounded sums that
// `Matrix.mul` promises: total = +0, then total = total + (left(i, k) * right(k, j)) for k = 0, 1, ...
const wrap = (value) => BigInt.asIntN(64, value);

/** The n x (n + 2) product of the fixture's inputs, flat and row-major, rounded to `bits` (32 or 64) after every operation. */
export function matrixProduct(n, seed, bits) {
  const round = bits === 32 ? Math.fround : (value) => value;
  const pool = [1e16, 1, -1e16, 3, -3, 0.5, -0.25, 1e-38, bits === 32 ? 1e-45 : 5e-324, -0].map(round);
  const out = [];
  for (let i = 0; i < n; i++) {
    for (let j = 0; j < n + 2; j++) {
      let total = 0;
      for (let k = 0; k < n + 1; k++) {
        total = round(total + round(pool[(i * 37 + k * 11 + seed) % 10] * pool[(k * 13 + j * 29 + seed + 3) % 10]));
      }
      out.push(total);
    }
  }
  return out;
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
];

export const traps = [
  ["of_array_length", []], ["add_shape", []], ["mul_shape", []],
  ["at_col", [3n]], ["at_col", [-1n]], ["at_row", [2n]], ["at_row", [-1n]],
  ["row_index", [2n]], ["row_index", [-1n]],
  ["negative_dims", []], ["count_overflow", []], ["mul_count_overflow", []], ["alloc_size", []],
];
