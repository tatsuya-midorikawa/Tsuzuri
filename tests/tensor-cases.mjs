// Independent references for the `tensor` suite of tests/features.mjs (C11 Phase 2). A window is modelled as an
// index mapping (closures that call the parent window) plus the layout numbers the documentation promises, so the
// expected values do not depend on the stride arithmetic of std/Tensor.tz. Integers wrap like i64.
const wrap = (value) => BigInt.asIntN(64, value);
const mix = (total, x) => wrap(total * 31n + BigInt(x));

const volume = (shape) => (shape.includes(0) ? 0 : shape.reduce((product, extent) => product * extent, 1));
const dense = (shape) => shape.map((_, axis) => shape.slice(axis + 1).reduce((product, extent) => product * extent, 1));
const flatIndex = (shape, index) => index.reduce((flat, position, axis) => flat * shape[axis] + position, 0);
function unflatten(shape, flat) {
  const index = new Array(shape.length);
  for (let axis = shape.length - 1; axis >= 0; axis--) {
    index[axis] = flat % shape[axis];
    flat = Math.floor(flat / shape[axis]);
  }
  return index;
}

// Every index of the shape in row-major order; a shape with no axes has the one empty index.
function* indices(shape) {
  if (volume(shape) === 0) return;
  const index = shape.map(() => 0);
  for (;;) {
    yield [...index];
    let axis = shape.length - 1;
    while (axis >= 0) {
      index[axis]++;
      if (index[axis] < shape[axis]) break;
      index[axis] = 0;
      axis--;
    }
    if (axis < 0) return;
  }
}

const window = (shape, strides, offset, at) => ({ shape, strides, offset, at });
const base = (shape, element = (flat) => BigInt(flat * 7 + 1)) => window(shape, dense(shape), 0, (index) => element(flatIndex(shape, index)));
const permute = (v, axes) => window(
  axes.map((axis) => v.shape[axis]),
  axes.map((axis) => v.strides[axis]),
  v.offset,
  (index) => {
    const original = [];
    axes.forEach((axis, position) => { original[axis] = index[position]; });
    return v.at(original);
  });
const indexAxis = (v, axis, position) => window(
  v.shape.filter((_, d) => d !== axis),
  v.strides.filter((_, d) => d !== axis),
  v.offset + position * v.strides[axis],
  (index) => v.at([...index.slice(0, axis), position, ...index.slice(axis)]));
const narrow = (v, axis, start, length) => window(
  v.shape.map((extent, d) => (d === axis ? length : extent)),
  v.strides,
  v.offset + start * v.strides[axis],
  (index) => v.at(index.map((position, d) => (d === axis ? position + start : position))));
const reshapeView = (v, shape) => window(shape, dense(shape), v.offset, (index) => v.at(unflatten(v.shape, flatIndex(shape, index))));

// One run of the buffer in row-major order: the positions of the elements are consecutive.
function contiguous(v) {
  let previous = null;
  for (const index of indices(v.shape)) {
    const position = v.offset + index.reduce((sum, p, axis) => sum + p * v.strides[axis], 0);
    if (previous !== null && position !== previous + 1) return false;
    previous = position;
  }
  return true;
}

function fold(v) {
  let total = 0n;
  for (const index of indices(v.shape)) total = mix(total, v.at(index));
  return total;
}

/** The digest of tests/fixtures/tensor/Main.tz: rank, count, layout, contiguity, the fold, and the fold of the copy. */
function digest(v) {
  const count = volume(v.shape);
  let total = 7n;
  for (const value of [v.shape.length, count, v.offset, contiguous(v) ? 1 : 0]) total = mix(total, value);
  v.shape.forEach((extent, axis) => {
    total = mix(total, extent);
    total = mix(total, v.strides[axis]);
  });
  const folded = fold(v);
  total = mix(total, folded);
  total = mix(total, folded);
  return mix(total, v.shape.length * 1000 + count);
}

const digests = (views) => views.reduce((total, v) => mix(total, digest(v)), 0n);

function ops(a, b, c) {
  const v = base([a, b, c]);
  const rotated = permute(v, [2, 0, 1]);
  const reversed = permute(v, [2, 1, 0]);
  const first = narrow(v, 0, 1, a - 1);
  return digests([
    v, rotated, permute(v, [1, 2, 0]), reversed, permute(v, [0, 2, 1]), permute(rotated, [1, 2, 0]),
    indexAxis(v, 0, a - 1), indexAxis(v, 1, b - 1), indexAxis(v, 2, c - 1), indexAxis(rotated, 1, a - 1),
    narrow(v, 1, 1, b - 1), narrow(v, 2, 1, c - 1), narrow(reversed, 0, 1, c - 1), first,
    reshapeView(v, [a * b, c]), reshapeView(v, [a, b * c]), reshapeView(v, [a * b * c]), reshapeView(v, [1, a * b * c, 1]),
    reshapeView(first, [(a - 1) * b, c]), indexAxis(narrow(v, 2, 0, 1), 2, 0),
  ]);
}

// map and to_tensor give contiguous tensors: the values of the window in its own row-major order.
const packed = (v, transform = (x) => x) => base(v.shape, (flat) => transform(v.at(unflatten(v.shape, flat))));

function maps(a, b, c) {
  const v = base([a, b, c]);
  const copy = packed(permute(v, [2, 0, 1]));
  let total = digests([packed(v, (x) => x * 2n + 1n), copy, packed(narrow(v, 2, 1, c - 1), (x) => x + 5n)]);
  const flat = [...indices(copy.shape)].map((index) => copy.at(index));
  total = mix(total, flat.reduce((sum, x) => wrap(sum + x), 0n));
  for (const x of flat) total = mix(total, x);
  return total;
}

function empties() {
  const [empty, wide, flat, scalar, nothing] = [[0, 3], [3, 0], [0], [], [2, 0, 4]].map((shape) => base(shape));
  let total = digests([empty, wide, flat, scalar, nothing, permute(nothing, [2, 0, 1]), indexAxis(nothing, 0, 1), indexAxis(nothing, 2, 3),
    narrow(empty, 1, 1, 2), reshapeView(nothing, [8, 0]), reshapeView(scalar, [1, 1, 1]), reshapeView(flat, [0, 5])]);
  total = mix(total, 0);
  return mix(total, 0);
}

const rankLimit = (n) => digest(base(new Array(n).fill(1)));

function matrices(rows, cols) {
  const v = base([rows, cols], (flat) => BigInt(Math.floor(flat / cols) * 10 + (flat % cols)));
  const grid = (g) => {
    let total = 0n;
    for (let i = 0; i < g.rows; i++) for (let j = 0; j < g.cols; j++) total = mix(total, g.at(i, j));
    return total;
  };
  const windowed = { rows, cols, at: (i, j) => v.at([i, j]) };
  const flipped = { rows: cols, cols: rows, at: (i, j) => v.at([j, i]) };
  const back = window([rows - 1, cols - 1], [cols, 1], cols + 1, ([i, j]) => v.at([i + 1, j + 1]));
  let total = mix(0n, digest(v));
  for (const value of [rows * 1000 + cols, 0, cols * 1000 + 1, grid(windowed), cols * 1000 + rows, 1000 + cols, grid(flipped), digest(back)]) total = mix(total, value);
  // reshape [cols, rows] and to_matrix keep the flat row-major order.
  total = mix(total, cols * 1000 + rows);
  let folded = 0n;
  for (const index of indices([rows, cols])) folded = mix(folded, v.at(index));
  return mix(total, folded);
}

function owners(n) {
  let sum = 0n;
  for (let k = 0; k < 2 * n; k++) sum = wrap(sum + BigInt(k * 3));
  const total = mix(sum, digest(base([2, n], (flat) => BigInt(flat + 100))));
  return mix(total, 2 * n);
}

function ownedTensor(n) {
  let total = 0n;
  for (let i = 0; i < n; i++) for (let j = 0; j < 2; j++) total = mix(total, `s${j * n + i}`.length);
  return mix(total, n * 100 + 1);
}

function lookups() {
  let total = 0n;
  for (let i = -1; i <= 2; i++) {
    for (let j = -1; j <= 3; j++) total = mix(total, i >= 0 && i < 2 && j >= 0 && j < 3 ? BigInt((i * 3 + j) * 7 + 1) : -1n);
  }
  for (let wrong = 0; wrong < 3; wrong++) total = mix(total, -1n);
  return mix(total, 1n);
}

export const cases = [
  ...[[2, 3, 4], [1, 1, 1], [3, 1, 2], [2, 2, 2], [1, 5, 1], [4, 3, 2], [2, 1, 6]].map((shape) => ["ops", shape.map(BigInt), ops(...shape)]),
  ...[[2, 3, 4], [1, 1, 1], [3, 1, 2], [4, 3, 2], [1, 5, 2]].map((shape) => ["maps", shape.map(BigInt), maps(...shape)]),
  ["empties", [], empties()],
  ...[0, 1, 2, 15, 16].map((n) => ["rank_limit", [BigInt(n)], rankLimit(n)]),
  ...[[2, 2], [2, 3], [3, 2], [4, 5], [5, 4]].map(([rows, cols]) => ["matrices", [BigInt(rows), BigInt(cols)], matrices(rows, cols)]),
  ...[0, 1, 2, 5].map((n) => ["owners", [BigInt(n)], owners(n)]),
  ...[0, 1, 2, 5, 12].map((n) => ["owned_tensor", [BigInt(n)], ownedTensor(n)]),
  ["lookups", [], lookups()],
];

export const traps = [
  ...[[2, -1], [-1, 2], [4294967296, 4294967296], [9223372036854775807n, 2]].map((shape) => ["init_trap", shape.map(BigInt)]),
  ...[0n, 5n].map((tail) => ["overflow_trap", [tail]]),
  ["of_array_trap", []], ["reshape_trap", []], ["borrow_trap", []],
  ...[17n, 40n].map((n) => ["rank_limit", [n]]),
  ...[[2, 0, 0], [0, 3, 0], [0, 0, 4], [-1, 0, 0], [0, -1, 0], [0, 0, -1]].map((index) => ["at_trap", index.map(BigInt)]),
  ["at_rank_trap", []],
  ...[[0, 0, 1], [0, 1, 3], [0, 1, -1], [3, 1, 2], [1, 1, 1], [0, 2, 2]].map((axes) => ["permute_trap", axes.map(BigInt)]),
  ["permute_rank_trap", []],
  ...[[3, 0], [-1, 0], [0, 2], [0, -1], [1, 3], [2, 4]].map((pair) => ["index_axis_trap", pair.map(BigInt)]),
  ["index_axis_scalar_trap", []],
  ...[[3, 0, 1], [-1, 0, 1], [0, -1, 1], [0, 0, -1], [0, 3, 0], [0, 1, 2], [2, 0, 5], [1, 2, 2]].map((triple) => ["narrow_trap", triple.map(BigInt)]),
  ...[[5, 5], [2, 13], [-1, -24]].map((shape) => ["reshape_view_trap", shape.map(BigInt)]),
  ["reshape_view_strided_trap", []], ["to_matrix_trap", []], ["as_matrix_view_trap", []],
];
