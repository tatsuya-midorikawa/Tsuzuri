// Independent references for the `matrix_view` suite of tests/features.mjs (C11 Phase 2). A window is modelled as
// an index mapping (closures that call the parent window) plus the layout numbers the documentation promises, so
// the expected values do not depend on the stride arithmetic of std/MatrixView.tz. Integers wrap like i64.
import { integerProduct, productEntry } from "./matrix-cases.mjs";

const wrap = (value) => BigInt.asIntN(64, value);
const mix = (total, x) => wrap(total * 31n + BigInt(x));

const base = (rows, cols, at, offset = 0, rowStride = cols, colStride = 1) => ({ rows, cols, offset, rs: rowStride, cs: colStride, at });
const transpose = (g) => ({ rows: g.cols, cols: g.rows, offset: g.offset, rs: g.cs, cs: g.rs, at: (i, j) => g.at(j, i) });
const sub = (g, r, c, h, w) => ({ rows: h, cols: w, offset: g.offset + r * g.rs + c * g.cs, rs: g.rs, cs: g.cs, at: (i, j) => g.at(r + i, c + j) });
const row = (g, i) => sub(g, i, 0, 1, g.cols);
const col = (g, j) => sub(g, 0, j, g.rows, 1);

// One run of the buffer in row-major order: the positions of the elements are consecutive.
function contiguous(g) {
  let previous = null;
  for (let i = 0; i < g.rows; i++) {
    for (let j = 0; j < g.cols; j++) {
      const position = g.offset + i * g.rs + j * g.cs;
      if (previous !== null && position !== previous + 1) return false;
      previous = position;
    }
  }
  return true;
}

function foldRowMajor(g) {
  let total = 0n;
  for (let i = 0; i < g.rows; i++) for (let j = 0; j < g.cols; j++) total = mix(total, g.at(i, j));
  return total;
}

/** The digest of tests/fixtures/matrix_view/Main.tz: shape, layout, contiguity, the fold, and the fold of the copy. */
function digest(g) {
  let total = 7n;
  for (const value of [g.rows, g.cols, g.offset, g.rs, g.cs, contiguous(g) ? 1 : 0]) total = mix(total, value);
  const folded = foldRowMajor(g);
  total = mix(total, folded);
  total = mix(total, folded);
  return mix(total, g.rows * 1000 + g.cols);
}

function layouts(rows, cols) {
  const whole = base(rows, cols, (i, j) => BigInt(i * 100 + j));
  const flipped = transpose(whole);
  const inner = sub(whole, 1, 1, rows - 1, cols - 1);
  const twisted = transpose(inner);
  const nested = sub(twisted, 1, 0, cols - 2, rows - 1);
  let total = 0n;
  for (const g of [whole, flipped, inner, twisted, nested, row(whole, rows - 1), col(whole, cols - 1), col(twisted, 0), transpose(flipped)]) total = mix(total, digest(g));
  return total;
}

function degenerate(rows, cols) {
  const whole = base(rows, cols, (i, j) => BigInt(i * 100 + j + 1));
  return mix(digest(whole), digest(transpose(whole)));
}

function stridedLayout(offset, rows, cols, rowStride, colStride) {
  return digest(base(rows, cols, (i, j) => BigInt((offset + i * rowStride + j * colStride) * 7 + 3), offset, rowStride, colStride));
}

function access(rows, cols) {
  const view = transpose(base(rows, cols, (i, j) => BigInt(i * 100 + j)));
  let total = 3n;
  for (let i = -1; i <= view.rows; i++) {
    for (let j = -1; j <= view.cols; j++) {
      const inside = i >= 0 && i < view.rows && j >= 0 && j < view.cols;
      total = mix(total, inside ? view.at(i, j) : -1n);
      if (inside) total = mix(total, view.at(i, j));
    }
  }
  return total;
}

// A writable window is a mapping from (row, column) to a position of the buffer.
function mutScript(rows, cols) {
  const buffer = Array.from({ length: rows * cols }, (_, k) => BigInt(k));
  const source = (i, j) => BigInt(1000 + i * 10 + j);
  const grid = (r, c, index) => ({ rows: r, cols: c, index });
  const whole = grid(rows, cols, (i, j) => i * cols + j);
  const write = (g, i, j, value) => { buffer[g.index(i, j)] = value; };
  const window = (g, r, c, h, w) => grid(h, w, (i, j) => g.index(r + i, c + j));
  const flip = (g) => grid(g.cols, g.rows, (i, j) => g.index(j, i));
  write(whole, 0, 0, 99n);
  write(whole, 1, 0, 77n);
  write(whole, 1, cols - 1, 78n);
  const inner = window(whole, 1, 1, rows - 1, cols - 1);
  for (let i = 0; i < inner.rows; i++) for (let j = 0; j < inner.cols; j++) write(inner, i, j, 5n);
  write(inner, 0, 0, 6n);
  let seen = 0n;
  for (let i = 0; i < rows; i++) for (let j = 0; j < cols; j++) seen = mix(seen, buffer[whole.index(i, j)]);
  const flipped = flip(whole);
  write(flipped, cols - 1, rows - 1, 55n);
  const block = window(flipped, 1, 1, cols - 1, rows - 1);
  for (let i = 0; i < block.rows; i++) for (let j = 0; j < block.cols; j++) write(block, i, j, source(1 + j, 1 + i));
  const head = window(flipped, 0, 0, 1, rows);
  for (let j = 0; j < head.cols; j++) write(head, 0, j, wrap(buffer[head.index(0, j)] * 3n + 1n));
  let total = mix(0n, seen);
  for (const value of buffer) total = mix(total, value);
  return total;
}

function rowwise(rows, cols) {
  let total = 0n;
  for (let i = 0; i < rows; i++) for (let j = 0; j < cols; j++) total = mix(total, i * 1000 + j);
  return total;
}

function ownedMatrix(n) {
  let total = 0n;
  for (let i = 0; i < n; i++) for (let j = 0; j <= n; j++) total = mix(total, i * 10 + j + 1000);
  return total;
}

// The words `${j}:${i + 1}` of the transposed matrix of words, without its first row.
function ownedElements(n) {
  let total = 0n;
  for (let i = 0; i < n; i++) for (let j = 0; j < n; j++) total = mix(total, `${j}:${i + 1}`.length);
  return mix(total, n * 100 + n);
}

const mismatchSeeds = [[0, 10], [1, 10], [7, 10], [0, 13], [1, 13], [7, 13]];
const strided = [
  [0, 5, 8, 8, 1], [0, 8, 5, 1, 8], [3, 4, 6, 8, 1], [39, 1, 1, 0, 0], [0, 3, 4, 0, 1], [5, 4, 3, 1, 0], [7, 3, 3, 0, 0],
  [0, 0, 5, 8, 1], [0, 5, 0, 1, 8], [100, 0, 3, 1, 1], [0, 2, 3, 20, 3], [2, 3, 2, 12, 13], [0, 1, 40, 0, 1], [0, 40, 1, 1, 0],
  [10, 1, 5, 99, 2], [10, 5, 1, 2, 77], [10, 5, 1, 1, 77], [0, 1, 1, 0, 0], [1, 4, 5, 5, 1],
];
const productShapes = [[2, 3, 4], [1, 1, 1], [3, 5, 2], [4, 1, 6], [0, 3, 2], [3, 0, 2], [5, 5, 5]];
const maxIndex = 9223372036854775807n;

// Windows with no elements and an axis of 2^62 rows: every operation finishes at once instead of walking the empty axis.
function hugeEmpty() {
  const rows = 2n ** 62n;
  return [7n, rows, rows, rows, rows].reduce(mix, 0n);
}

export const cases = [
  ...[[2, 2], [2, 3], [3, 2], [3, 5], [5, 3], [6, 4], [4, 4]].map(([rows, cols]) => ["layouts", [BigInt(rows), BigInt(cols)], layouts(rows, cols)]),
  ...[[0, 0], [0, 5], [5, 0], [1, 1], [1, 4], [4, 1], [3, 1], [1, 3], [2, 2]].map(([rows, cols]) => ["degenerate", [BigInt(rows), BigInt(cols)], degenerate(rows, cols)]),
  ...[[0, 0], [1, 1], [2, 3], [3, 2], [0, 4], [4, 0], [5, 5]].map(([rows, cols]) => ["array_view", [BigInt(rows), BigInt(cols)], digest(base(rows, cols, (i, j) => BigInt((i * cols + j) * 3 + 1)))]),
  ...strided.map((layout) => ["strided_layout", layout.map(BigInt), stridedLayout(...layout)]),
  ...[[2, 3], [3, 2], [1, 1], [0, 3], [3, 0], [4, 5]].map(([rows, cols]) => ["access", [BigInt(rows), BigInt(cols)], access(rows, cols)]),
  ...mismatchSeeds.flatMap(([seed, span]) => ["view_product_mismatches64", "view_product_mismatches32"].map((name) => [name, [BigInt(seed), BigInt(span)], 0n])),
  ...productShapes.flatMap(([rows, inner, cols]) => [0, 1, 7].flatMap((seed) => Array.from({ length: rows * cols }, (_, index) => [
    "view_product_entry64", [rows, inner, cols, seed, index].map(BigInt), productEntry(inner, seed, 64, false, Math.floor(index / cols), index % cols)]))),
  ...[0, 1, 2, 7, 16].flatMap((n) => [0, 5].map((seed) => ["view_int_product", [BigInt(n), BigInt(seed)], integerProduct(n, seed)])),
  ...[[2, 2], [2, 3], [3, 2], [3, 5], [5, 3], [6, 4]].map(([rows, cols]) => ["mut_script", [BigInt(rows), BigInt(cols)], mutScript(rows, cols)]),
  ...[[0, 0], [0, 3], [3, 0], [1, 1], [2, 3], [4, 4], [7, 2]].map(([rows, cols]) => ["rowwise", [BigInt(rows), BigInt(cols)], rowwise(rows, cols)]),
  ...[0, 1, 2, 5].map((n) => ["owned_matrix", [BigInt(n)], ownedMatrix(n)]),
  ...[0, 1, 2, 5, 12].map((n) => ["owned_elements", [BigInt(n)], ownedElements(n)]),
];

// Run one per process with a kill timer: before the fix they never returned.
export const bounded = [["huge_empty", [], hugeEmpty()]];

export const traps = [
  // A window past the 10 elements, with a negative argument, with a huge count or with an overflowing index.
  // The last element's index may itself be i64::MAX (`[MAX, 1, 1, 0, 0]`, `[0, 1, 2, 0, MAX]`): it is outside the data too.
  ...[[0, 3, 4, 4, 1], [-1, 1, 1, 1, 1], [0, -1, 1, 1, 1], [0, 1, -1, 1, 1], [0, 2, 2, -1, 1], [0, 2, 2, 1, -1], [10, 1, 1, 0, 0],
    [9, 1, 2, 0, 1], [0, 1, 11, 0, 1], [9223372036854775807n, 2, 1, 1, 0], [0, 2, 2, 9223372036854775807n, 1],
    [maxIndex, 1, 1, 0, 0], [0, 1, 2, 0, maxIndex], [maxIndex - 1n, 1, 2, 0, 1], [0, 2, 1, maxIndex, 0]].map((layout) => ["strided_trap", layout.map(BigInt)]),
  ["of_array_trap", []], ["count_overflow", []], ["extent_overflow", []], ["mul_trap", []],
  // The shapes are compared before either operand is copied: a huge left window is a mismatch, not an allocation failure.
  ["mul_shape_before_copy_trap", []],
  ...[[5, 0, 0, 0], [0, 6, 0, 0], [-1, 0, 1, 1], [0, -1, 1, 1], [0, 0, -1, 1], [0, 0, 1, -1], [0, 0, 5, 1], [0, 0, 1, 6], [1, 1, 4, 1], [1, 1, 1, 5]].map((window) => ["sub_trap", window.map(BigInt)]),
  ...[-1n, 4n].map((index) => ["row_trap", [index]]),
  ...[-1n, 5n].map((index) => ["col_trap", [index]]),
  ...[[5, 0], [0, 4], [-1, 0], [0, -1]].map(([i, j]) => ["at_trap", [BigInt(i), BigInt(j)]]),
  ["mut_of_array_trap", []], ["mut_row_transposed_trap", []], ["mut_copy_trap", []],
  ...[[2, 0], [0, 3], [-1, 0], [0, -1]].map(([i, j]) => ["mut_write_trap", [BigInt(i), BigInt(j)]]),
  ...[-1n, 2n].map((index) => ["mut_row_trap", [index]]),
  ...[[3, 0, 0, 0], [0, 4, 0, 0], [1, 1, 2, 1], [1, 1, 1, 3], [-1, 0, 1, 1]].map((window) => ["mut_sub_trap", window.map(BigInt)]),
];
