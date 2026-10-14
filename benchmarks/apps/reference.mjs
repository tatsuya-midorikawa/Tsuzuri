import assert from "node:assert/strict";

export const workloads = Object.freeze([
  ["tokenize", 200_000, 1_024, 10_000_000],
  ["format_join", 100_000, 1_024, 10_000_000],
  ["csv_aggregate", 100_000, 1_024, 10_000_000],
  ["map_count", 200_000, 1_024, 10_000_000],
  ["record_sort", 200_000, 1_024, 10_000_000],
  ["tree_transform", 256, 2, 100_000],
  ["binary_trees", 2_000, 4, 1_000_000],
  ["closure_pipeline", 1_000_000, 1_024, 10_000_000],
  ["graph_bfs", 100_000, 1_024, 1_000_000],
  ["spectral_norm", 500, 16, 5_000],
].map(([name, normal, quick, maximum]) => Object.freeze({ name, normal, quick, maximum })));

export const wrap = (value) => BigInt.asUintN(64, value);
export const hex = (value) => wrap(value).toString(16).padStart(16, "0");
const multiplier = 6364136223846793005n;
const increment = 1442695040888963407n;
const hash = (key) => {
  const value = wrap(wrap(key) * multiplier + increment);
  return value ^ (value >> 29n);
};
const fold = (total, value) => wrap(total * 31n + value);
const words = ["alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta"];

function build(depth, key) {
  const value = hash(key);
  if (depth === 0) return { kind: "lit", value: value >> 40n };
  const left = build(depth - 1, wrap(key * 2n + 1n));
  if ((value & 3n) === 0n) return { kind: "neg", left };
  const right = build(depth - 1, wrap(key * 2n + 2n));
  return { kind: (value & 3n) === 1n ? "mul" : "add", left, right };
}

function simplify(expression) {
  if (expression.kind === "lit") return { kind: "lit", value: expression.value };
  const left = simplify(expression.left);
  if (expression.kind === "neg") {
    if (left.kind === "neg") return left.left;
    if (left.kind === "lit") return { kind: "lit", value: wrap(-left.value) };
    return { kind: "neg", left };
  }
  const right = simplify(expression.right);
  if (expression.kind === "add" && left.kind === "lit" && right.kind === "lit") {
    return { kind: "lit", value: wrap(left.value + right.value) };
  }
  return { kind: expression.kind, left, right };
}

function evaluate(expression) {
  if (expression.kind === "lit") return expression.value;
  const left = evaluate(expression.left);
  if (expression.kind === "neg") return wrap(-left);
  const right = evaluate(expression.right);
  return wrap(expression.kind === "add" ? left + right : left * right);
}

const spectralResults = new Map();
function spectralNorm(n) {
  if (spectralResults.has(n)) return spectralResults.get(n);
  const coefficient = (row, column) => {
    const sum = row + column;
    return 1 / (sum * (sum + 1) / 2 + row + 1);
  };
  const multiply = (values, transpose) => Array.from({ length: n }, (_, row) => {
    let total = 0;
    for (let column = 0; column < n; ++column) {
      const entry = transpose ? coefficient(column, row) : coefficient(row, column);
      total = total + entry * values[column];
    }
    return total;
  });
  const ata = (values) => multiply(multiply(values, false), true);
  let u = Array(n).fill(1), v = Array(n).fill(0);
  for (let iteration = 0; iteration < 10; ++iteration) {
    v = ata(u);
    u = ata(v);
  }
  let uv = 0, vv = 0;
  for (let index = 0; index < n; ++index) {
    uv = uv + u[index] * v[index];
    vv = vv + v[index] * v[index];
  }
  const value = BigInt(Math.trunc(Math.sqrt(uv / vv) * 1e15));
  spectralResults.set(n, value);
  return value;
}

export function reference(name, size, seed) {
  const definition = workloads.find((work) => work.name === name);
  assert.ok(definition, `unknown app workload: ${name}`);
  const n = Number(size);
  assert.ok(Number.isSafeInteger(n) && n >= 0 && n <= definition.maximum, `invalid ${name} size: ${size}`);
  assert.equal(typeof seed, "bigint", "seed must be a BigInt");
  seed = wrap(seed);
  if (n === 0) return 0n;
  if (name === "tokenize") {
    const text = Array.from({ length: n }, (_, index) => words[Number(hash(seed + BigInt(index)) >> 61n)]).join(" ");
    const tokens = text.split(" ");
    return tokens.reduce((total, token) => fold(total, BigInt(token.length)), BigInt(tokens.length));
  }
  if (name === "format_join") {
    const text = Array.from({ length: n }, (_, index) => hash(seed + BigInt(index)).toString()).join(",");
    const sum = text.split(",").reduce((total, field) => wrap(total + BigInt(field)), 0n);
    return fold(BigInt(text.length), sum);
  }
  if (name === "csv_aggregate") {
    const text = Array.from({ length: n }, (_, index) => {
      const value = hash(seed + BigInt(index));
      return `${value >> 60n},${(value >> 20n) & 65535n}`;
    }).join("\n");
    const lines = text.split("\n"), sums = Array(16).fill(0n), counts = Array(16).fill(0n);
    for (const line of lines) {
      const [category, amount] = line.split(",").map(BigInt);
      sums[Number(category)] = wrap(sums[Number(category)] + amount);
      counts[Number(category)] = wrap(counts[Number(category)] + 1n);
    }
    let total = fold(0n, BigInt(lines.length));
    for (let category = 0; category < 16; ++category) total = fold(fold(total, sums[category]), counts[category]);
    return total;
  }
  if (name === "map_count") {
    const counts = new Map(), keys = new Set();
    let state = seed;
    for (let index = 0; index < n; ++index) {
      state = wrap(state * multiplier + increment);
      const key = Number((state >> 33n) & 1023n);
      counts.set(key, wrap((counts.get(key) ?? 0n) + 1n));
      keys.add(key & 255);
    }
    let total = BigInt(counts.size) * 1_000_003n + BigInt(keys.size);
    for (let key = 0; key < 1024; ++key) total = fold(total, counts.get(key) ?? 0n);
    return total;
  }
  if (name === "record_sort") {
    // A different sorting implementation independently checks the specified stable order.
    const records = Array.from({ length: n }, (_, id) => ({ key: hash(seed + BigInt(id)) >> 44n, id }));
    records.sort((left, right) => left.key === right.key ? left.id - right.id : left.key < right.key ? -1 : 1);
    return records.reduce((total, record) => fold(total, BigInt(record.id)), 0n);
  }
  if (name === "tree_transform") {
    let total = 0n;
    for (let index = 0; index < n; ++index) total = fold(total, evaluate(simplify(build(12, wrap(seed + BigInt(index))))));
    return total;
  }
  if (name === "binary_trees") {
    let total = 0n;
    for (let index = 0; index < n; ++index) {
      const depth = 4n + (wrap(seed + BigInt(index)) & 7n);
      total = fold(total, (1n << (depth + 1n)) - 1n);
    }
    return total;
  }
  if (name === "closure_pipeline") {
    const functions = [0n, 1n, 2n, 3n].map((key) => (value) => wrap(value * multiplier + (seed ^ key)));
    const values = Array.from({ length: n }, (_, index) => wrap(seed + BigInt(index)));
    const mapped = values.map((value) => functions[Number(value & 3n)](value));
    return mapped.reduce((total, value) => (value & 1n) === 0n ? fold(total, value) : total, 0n);
  }
  if (name === "graph_bfs") {
    // Adjacency lists, rather than CSR, provide an independent graph reference.
    const adjacent = Array.from({ length: n }, () => []);
    for (let index = 0; index < 4 * n; ++index) {
      const value = hash(seed + BigInt(index));
      adjacent[Number((value >> 32n) % BigInt(n))].push(Number((value & 0xffffffffn) % BigInt(n)));
    }
    const distances = new Int32Array(n).fill(-1), queue = new Uint32Array(n);
    distances[0] = 0;
    let head = 0, tail = 1;
    while (head < tail) {
      const source = queue[head++];
      for (const destination of adjacent[source]) if (distances[destination] < 0) {
        distances[destination] = distances[source] + 1;
        queue[tail++] = destination;
      }
    }
    return distances.reduce((total, distance) => fold(total, wrap(BigInt(distance))), 0n);
  }
  return wrap(spectralNorm(n) ^ seed);
}

export function checkReference() {
  for (const seed of [0n, 42n, (1n << 64n) - 1n]) {
    for (const work of workloads) assert.equal(reference(work.name, 0, seed), 0n, `${work.name}/empty`);
    const state = wrap(seed * multiplier + increment), key = Number((state >> 33n) & 1023n);
    let expected = 1_000_004n;
    for (let index = 0; index < 1024; ++index) expected = fold(expected, index === key ? 1n : 0n);
    assert.equal(reference("map_count", 1, seed), expected, "one-entry sorted map/set");
  }
  assert.equal(reference("tokenize", 1, 0n), 36n);
  assert.equal(reference("binary_trees", 1, 42n), 127n);
  assert.equal(reference("tree_transform", 1, 42n), 0x84cfa56ce891d040n);
  assert.equal(reference("spectral_norm", 1, 0n), 1_000_000_000_000_000n);
}
