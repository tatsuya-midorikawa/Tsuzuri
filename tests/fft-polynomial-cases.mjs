// Independent JavaScript references for NC09: the suites `fft` and `polynomial` of tests/features.mjs.
// The transforms are computed from their definitions (an O(n^2) sum per term), the polynomials with BigInt or
// from closed forms. A value is folded after rounding to a fixed scale; `rounded` refuses a value that sits
// near a rounding boundary, so that the last bits of a correct implementation cannot change the expectation.
import assert from "node:assert/strict";

const mix = (total, x) => BigInt.asIntN(64, total * 31n + BigInt(x));

function rounded(scale, value, margin = 1e-6) {
  const scaled = value * scale;
  const fraction = scaled - Math.floor(scaled);
  assert.ok(Math.abs(fraction - 0.5) > margin, `${value} * ${scale} is too close to a rounding boundary`);
  return BigInt(Math.round(scaled));
}

const signal = (n) => Array.from({ length: n }, (_, j) => [((j * 7 + 3) % 11) - 5, ((j * 5 + 1) % 13) - 6]);
const samples = (n) => Array.from({ length: n }, (_, j) => ((j * 7 + 3) % 11) - 5);

// sum x[j] exp(sign 2 pi i j k / n), times `scale`.
function dft(values, sign, scale = 1) {
  const n = values.length;
  return values.map((_, k) => {
    let re = 0, im = 0;
    for (let j = 0; j < n; j++) {
      const angle = sign * 2 * Math.PI * ((j * k) % n) / n;
      const c = Math.cos(angle), s = Math.sin(angle);
      re += values[j][0] * c - values[j][1] * s;
      im += values[j][0] * s + values[j][1] * c;
    }
    return [re * scale, im * scale];
  });
}

// The transform of a rows x cols matrix in row-major order along one axis (0: down the columns, 1: along the rows).
function alongAxis(values, rows, cols, axis, sign, scale = 1) {
  const out = values.map((value) => [...value]);
  if (axis === 1) {
    for (let r = 0; r < rows; r++) dft(values.slice(r * cols, (r + 1) * cols), sign, scale).forEach((value, c) => { out[r * cols + c] = value; });
  } else {
    for (let c = 0; c < cols; c++) dft(Array.from({ length: rows }, (_, r) => values[r * cols + c]), sign, scale).forEach((value, r) => { out[r * cols + c] = value; });
  }
  return out;
}
const dft2 = (values, rows, cols, sign, scale = 1) => alongAxis(alongAxis(values, rows, cols, 1, sign), rows, cols, 0, sign, scale);

const digest = (scale, values, margin) => values.reduce((total, [re, im]) => mix(mix(total, rounded(scale, re, margin)), rounded(scale, im, margin)), BigInt(values.length));
const digestReal = (scale, values) => values.reduce((total, value) => mix(total, rounded(scale, value)), BigInt(values.length));

// The n real samples whose transform starts with the terms `half`, as NumPy's irfft defines them: the imaginary
// parts of the term 0 and, for an even n, of the term n / 2 do not count.
function halfInverse(half, n, conjugate, scale) {
  return Array.from({ length: n }, (_, j) => {
    let total = half[0][0];
    for (let k = 1; k <= Math.floor((n - 1) / 2); k++) {
      const angle = 2 * Math.PI * ((j * k) % n) / n;
      const im = conjugate ? -half[k][1] : half[k][1];
      total += 2 * (half[k][0] * Math.cos(angle) - im * Math.sin(angle));
    }
    if (n % 2 === 0) total += half[n / 2][0] * (j % 2 === 0 ? 1 : -1);
    return total * scale;
  });
}

const lengths = [1, 2, 3, 4, 5, 6, 7, 8, 9, 12, 15, 16, 17, 25, 30, 31, 32, 45, 64, 100, 127, 128, 243, 250];
const pair = (name, list, expected) => list.map((args) => [name, args.map(BigInt), expected(...args)]);

function shifts(rows, cols) {
  const at = (r, c) => BigInt(r * cols + c);
  let total = 0n;
  // fftshift moves element i to (i + n / 2) % n: the element at position p came from (p - n / 2) mod n.
  const back = (p, n) => (((p - Math.floor(n / 2)) % n) + n) % n;
  const forth = (p, n) => (p + Math.floor(n / 2)) % n;
  for (let r = 0; r < rows; r++) for (let c = 0; c < cols; c++) total = mix(total, at(back(r, rows), back(c, cols)));
  for (let r = 0; r < rows; r++) for (let c = 0; c < cols; c++) total = mix(total, at(forth(r, rows), forth(c, cols)));
  for (let r = 0; r < rows; r++) for (let c = 0; c < cols; c++) total = mix(total, at(r, back(c, cols)));
  return total;
}

function frequencies(n) {
  let total = 0n;
  for (let i = 0; i < n; i++) total = mix(total, BigInt(4 * (i < Math.floor((n - 1) / 2) + 1 ? i : i - n)));
  for (let i = 0; i <= Math.floor(n / 2); i++) total = mix(total, BigInt(4 * i));
  return total;
}

export const fftCases = [
  ...pair("fft_digest", lengths.map((n) => [n]), (n) => digest(1024, dft(signal(n), -1))),
  ...pair("ifft_digest", lengths.map((n) => [n]), (n) => digest(65536, dft(signal(n), 1, 1 / n))),
  ...pair("ortho_digest", [1, 2, 3, 4, 7, 8, 12, 17, 30, 64, 100].map((n) => [n]), (n) =>
    mix(digest(4096, dft(signal(n), -1, 1 / Math.sqrt(n))), digest(4096, dft(signal(n), -1, 1 / n)))),
  ...pair("padded_digest", [[5, 8], [5, 3], [8, 13], [12, 12], [7, 1], [3, 30]], (n, length) =>
    digest(1024, dft(Array.from({ length }, (_, j) => (j < n ? signal(n)[j] : [0, 0])), -1))),
  ...pair("rfft_digest", lengths.map((n) => [n]), (n) =>
    digest(1024, dft(samples(n).map((value) => [value, 0]), -1).slice(0, Math.floor(n / 2) + 1))),
  ...pair("irfft_digest", lengths.map((n) => [n]), (n) => digestReal(65536, halfInverse(signal(Math.floor(n / 2) + 1), n, false, 1 / n))),
  ...pair("hfft_digest", lengths.map((n) => [n]), (n) => digestReal(1024, halfInverse(signal(Math.floor(n / 2) + 1), n, true, 1))),
  ...pair("fft2_digest", [[1, 1], [2, 2], [3, 4], [4, 3], [5, 7], [8, 8], [6, 10], [17, 4], [1, 9], [9, 1]], (rows, cols) =>
    mix(digest(1024, dft2(signal(rows * cols), rows, cols, -1)), digest(65536, dft2(signal(rows * cols), rows, cols, 1, 1 / (rows * cols))))),
  ...pair("axis_digest", [[2, 2], [3, 4], [7, 3], [8, 5], [1, 6]], (rows, cols) => digest(1024, alongAxis(signal(rows * cols), rows, cols, 0, -1))),
  ...pair("rfft2_digest", [[2, 2], [3, 4], [4, 3], [5, 7], [8, 8], [6, 10], [1, 9], [9, 1]], (rows, cols) => {
    const full = dft2(samples(rows * cols).map((value) => [value, 0]), rows, cols, -1);
    const kept = Math.floor(cols / 2) + 1;
    return digest(1024, Array.from({ length: rows * kept }, (_, index) => full[Math.floor(index / kept) * cols + (index % kept)]));
  }),
  ...pair("real_round_trip", [[1, 1], [2, 2], [3, 4], [4, 3], [5, 7], [8, 8], [6, 10], [7, 9]], () => 0n),
  // Single precision: a coarse scale and a wide margin, on the lengths whose terms stay away from the boundaries
  // (tests/fft.rs checks f32 against f64 for long transforms).
  ...pair("f32_digest", [1, 2, 3, 4, 6, 7, 8, 9, 12, 16].map((n) => [n]), (n) => digest(16, dft(signal(n), -1), 0.05)),
  ...pair("shift_digest", [[1, 1], [2, 2], [3, 5], [4, 6], [5, 4], [1, 7]], shifts),
  ...pair("freq_digest", [1, 2, 3, 4, 7, 8, 16].map((n) => [n]), frequencies),
  ...pair("plan_agrees", [1, 2, 8, 12, 17, 30, 100, 127].map((n) => [n]), () => 1n),
  // More than one parallel chunk of 16384 values, and lengths of every kind of kernel.
  ...pair("parallel_agrees", [[1, 1, 1], [2, 3, 5], [2, 64, 300], [3, 17, 512], [1, 130, 131]], () => 1n),
  ...pair("error_codes", [[0], [1], [2], [3], [4], [5], [6], [7]], (which) => [101n, 0n, 227n, 303n, 443n, 521n, 101n, 0n][which]),
];

export const fftTraps = [["trap_fftfreq_without_samples", [0n]], ["trap_fftfreq_without_samples", [-3n]]];

// ---------------------------------------------------------------------------------------------------------
// Polynomial: exact references with BigInt. p(x) = 3 - 2 x + 5 x^3 + x^4 and q(x) = -1 + 4 x + 2 x^2.
// ---------------------------------------------------------------------------------------------------------

const p = [3n, -2n, 0n, 5n, 1n];
const q = [-1n, 4n, 2n];
const horner = (coefficients, x) => coefficients.reduceRight((value, c) => value * x + c, 0n);
// Horner's rule at the Gaussian integer a + b i.
const hornerComplex = (coefficients, a, b) => coefficients.reduceRight(([re, im], c) => [re * a - im * b + c, re * b + im * a], [0n, 0n]);
const at = (coefficients, index) => (index < coefficients.length ? coefficients[index] : 0n);
const longer = (left, right) => Array.from({ length: Math.max(left.length, right.length) }, (_, index) => index);
const addP = (left, right) => longer(left, right).map((index) => at(left, index) + at(right, index));
const subP = (left, right) => longer(left, right).map((index) => at(left, index) - at(right, index));
function mulP(left, right) {
  const out = Array.from({ length: left.length + right.length - 1 }, () => 0n);
  left.forEach((a, i) => right.forEach((b, j) => { out[i + j] += a * b; }));
  return out;
}
const derivP = (coefficients) => coefficients.slice(1).map((c, index) => BigInt(index + 1) * c);
const fromRoots = (roots) => roots.reduce((product, root) => mulP(product, [-root, 1n]), [1n]);
const foldInts = (values) => values.reduce((total, value) => mix(total, value), BigInt(values.length));

// Exact fractions [numerator, denominator] for the recurrences of T_n and P_n at k / 8.
const times = ([a, b], [c, d]) => [a * c, b * d];
const minus = ([a, b], [c, d]) => [a * d - c * b, b * d];
function seriesValue(kind, degree, k) {
  const x = [BigInt(k), 8n];
  let before = [1n, 1n], current = x;
  if (degree === 0) return 1n << 20n;
  for (let n = 1; n < degree; n++) {
    // T_(n+1) = 2 x T_n - T_(n-1);  (n + 1) P_(n+1) = (2 n + 1) x P_n - n P_(n-1)
    const next = kind === 0
      ? minus(times([2n, 1n], times(x, current)), before)
      : times([1n, BigInt(n + 1)], minus(times([BigInt(2 * n + 1), 1n], times(x, current)), times([BigInt(n), 1n], before)));
    before = current;
    current = next;
  }
  const scaled = current[0] * (1n << 20n);
  assert.equal(scaled % current[1], 0n, `the value of degree ${degree} at ${k}/8 times 2^20 is an integer`);
  return scaled / current[1];
}

const arithmetic = [
  foldInts(addP(p, q)),
  foldInts(subP(q, p)),
  foldInts(mulP(p, q)),
  foldInts(mulP(mulP(q, q), q)),
  foldInts(derivP(p)),
  foldInts(derivP(derivP(derivP(p)))),
  // 60 times the antiderivative [0, 3, -1, 0, 5/4, 1/5], and the one that is 7 at 0.
  foldInts([0n, 180n, -60n, 0n, 75n, 12n]),
  foldInts([420n, 180n, -60n, 0n, 75n, 12n]),
  foldInts(fromRoots([-2n, 1n, 3n, 3n])),
  foldInts([4n, 0n, -2n]),
  // (x^2 - 2 x + 5) (x + 3) = 15 - x + x^2 + x^3, with zero imaginary parts.
  [15n, -1n, 1n, 1n].reduce((total, re) => mix(mix(total, re), 0n), 4n),
];

export const polynomialCases = [
  ...[-20n, -3n, -1n, 0n, 1n, 2n, 7n, 20n].map((x) => ["eval_at", [x],
    mix(mix(mix(horner(p, x), horner(p, x)), 0n), foldInts([horner(q, x), horner(q, x + 1n)]))]),
  ...[[0n, 1n], [1n, 2n], [-2n, 3n], [5n, -4n], [0n, 0n], [3n, 0n]].map(([a, b]) => ["eval_complex_at", [a, b],
    (([re, im]) => mix(re, im))(hornerComplex(p, a, b))]),
  ...arithmetic.map((expected, which) => ["arithmetic", [BigInt(which)], expected]),
  // 1024 times the quotient p and the remainder 5 - 3 x.
  ["division", [0n], foldInts(p.map((c) => c * 1024n))],
  ["division", [1n], foldInts([5120n, -3072n])],
  // 1024 times the cubic 2 - x + 3 x^2 + 4 x^3 in the power basis (with and without weights), as the Chebyshev
  // series 3.5 T0 + 2 T1 + 1.5 T2 + T3, and as the Legendre series 3 P0 + 1.4 P1 + 2 P2 + 1.6 P3.
  ["fitting", [0n], foldInts([2048n, -1024n, 3072n, 4096n])],
  ["fitting", [1n], foldInts([2048n, -1024n, 3072n, 4096n])],
  ["fitting", [2n], foldInts([3584n, 2048n, 1536n, 1024n])],
  ["fitting", [3n], foldInts([3072n, rounded(1024, 1.4), 2048n, rounded(1024, 1.6)])],
  // The roots -2, 1, 2 - 3i, 2 + 3i and 3, sorted by real part and then by imaginary part.
  ...[1n, 1024n].map((scale) => ["root_digest", [scale],
    [[-2n, 0n], [1n, 0n], [2n, -3n], [2n, 3n], [3n, 0n]].reduce((total, [re, im]) => mix(mix(total, re * scale), im * scale), 5n)]),
  ...[0, 1].flatMap((kind) => [0, 1, 2, 3, 4, 5].flatMap((degree) => [-8, -5, -1, 0, 3, 8, 12].map((k) =>
    ["series", [BigInt(kind), BigInt(degree), BigInt(k)], seriesValue(kind, degree, k)]))),
  ...[434n, 16n, 32n, 5n, 434n, 1n, 0n].map((expected, which) => ["error_codes", [BigInt(which)], expected]),
];

export const polynomialTraps = [["trap_negative_power", [-1n]], ["trap_negative_order", [-2n]], ["trap_negative_tolerance", [-1n]]];
