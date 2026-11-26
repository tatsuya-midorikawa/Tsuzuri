// Independent JavaScript BigInt reference for NC19 (43 Tensor operator extension instances).

const foldI64 = (vals) => vals.reduce((acc, v) => BigInt.asIntN(64, acc * 37n + BigInt(v)), 0n);
const foldF64Scaled = (vals) =>
  vals.reduce((acc, v) => BigInt.asIntN(64, acc * 37n + BigInt(Math.trunc(v * 100))), 0n);

const truncDiv = (a, b) => BigInt(Math.trunc(Number(a) / Number(b)));
const truncRem = (a, b) => a - truncDiv(a, b) * b;

export const cases = [
  [
    "spec_examples_f64",
    [],
    foldF64Scaled([10.0, 10.0, 10.0, 10.0]) +
      foldF64Scaled([2.0, 4.0, 6.0]) +
      foldF64Scaled([-0.5, 0.5, 1.5]) +
      foldF64Scaled([0.25, 2.25, 6.25]),
  ],
  [
    "spec_broadcast_and_neg_i64",
    [],
    foldI64([10, 21, 32, 13, 24, 35]) +
      foldI64([-10, -20, -30]) +
      foldI64([505, 510, 515, 1005, 1010, 1015]),
  ],
  [
    "all_43_forms_i64",
    [],
    (() => {
      const t = [12n, 20n, 30n];
      const u = [3n, 4n, 5n];
      const pb = [2n, 3n, 4n];
      const pe = [3n, 2n, 1n];
      return (
        foldI64(t.map((x, i) => x + u[i])) +
        foldI64(t.map((x) => 2n + x)) +
        foldI64(t.map((x, i) => x - u[i])) +
        foldI64(t.map((x) => 100n - x)) +
        foldI64(t.map((x) => x - 100n)) +
        foldI64(t.map((x, i) => x * u[i])) +
        foldI64(t.map((x) => 3n * x)) +
        foldI64(t.map((x, i) => truncDiv(x, u[i]))) +
        foldI64(t.map((x) => truncDiv(120n, x))) +
        foldI64(t.map((x) => truncDiv(x, 2n))) +
        foldI64(t.map((x, i) => truncRem(x, u[i]))) +
        foldI64(t.map((x) => truncRem(100n, x))) +
        foldI64(t.map((x) => truncRem(x, 7n))) +
        foldI64(pb.map((x, i) => x ** pe[i])) +
        foldI64(pe.map((e) => 2n ** e)) +
        foldI64(pb.map((x) => x ** 2n)) +
        foldI64(t.map((x) => -x))
      );
    })(),
  ],
  [
    "negative_stride_empty_and_signed_div_rem",
    [],
    foldI64([30n, 10n, -10n, -30n]) +
      foldI64([-3n, 3n, -3n, 3n]) +
      foldI64([-1n, -1n, 1n, 1n]) +
      foldI64([-3n, -3n, 3n, 3n]) +
      foldI64([-1n, -1n, 1n, 1n]),
  ],
];

export const traps = [
  ["trap_shape_mismatch", [2n], "assertion failed"],
  ["trap_scalar_div_zero", [0n], "integer division by zero"],
];
