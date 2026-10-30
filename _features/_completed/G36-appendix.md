# G36 付録: `tests/progfuzz.mjs` の参照実装と実測

本文は [G36-language-differential-fuzz.md](G36-language-differential-fuzz.md)。この付録は次の 6 節からなる。

- A: `tests/progfuzz.mjs` の全文。ブリーフが用意したビルド済みの `target/release/tsuzuri` で実行して確かめた参照実装で、実装者はこの文面を土台にする。
- B: 実測のコマンドと結果。
- C: `const def` の受理の実測（43 件）。
- D: 見つけた既知のバグ KB1 の最小の再現。
- E: 生成される `.tz` の例。
- F: 位相ごとのファイルを作る手順（`strip.mjs`）。

出荷した `tests/progfuzz.mjs`（2026-10-31）は、A の全文に、本文の「方針の最新化（2026-10-31。実装前の再検討）」の差を加えたものです: edition 2027 の正準形（範囲の `..=` と `..<`、所有権の補助関数の `own`、2027 の `Vec.push` と `Vec.push_owned`）、半開の範囲の 10 行を足した 126 行の表、被覆のタグ `loop_half_open`、自己検査の最小化を回数だけで止める `minimizeSeconds`。B〜E の数と文面は起票時（HEAD `a2313d19`）の実測で、出荷版の実測は本文の「実装と検証（2026-10-31）」にあります。

## A. `tests/progfuzz.mjs` の全文

`// <<< phase N` から `// >>> phase N` までの行は、位相 N（本文の「Phase 分割」）で足す行である。位相 1 のファイルは、位相 2 と 3 の行を除いたもの。入れ子の印（位相 2 の中の位相 3）もある。印の行そのものはファイルに残してよい（コメント）。位相 1 から 3 までを足したファイルは、付録 B の実測をこの全文で取った。

````js
// Differential test of the whole language (G36): random typed programs of integer arithmetic, aggregates and
// control flow, run by the compiler's constant evaluator (`const def`), by native -O0 and -O3 and by wasm32 -O0
// and -O3, against an independent BigInt evaluator of the same syntax tree. The expected values come from that
// evaluator, which follows the documented semantics (wrapping + - *, truncating / and %, shift counts taken
// modulo the bit width, wrapping casts, inclusive ranges that stop at the end of the type), never from the
// compiler. Programs that own a Vec or a string are also checked for leaks (TSUZURI_PROGFUZZ_KIND=owned).
//
//   node tests/progfuzz.mjs [compiler] [programs] [first seed] [depth]
//   node tests/progfuzz.mjs --self-test [compiler]
//
// Lines between `// <<< phase N` and `// >>> phase N` belong to phase N of the ticket G36 (see its steps).
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { availableParallelism } from "node:os";
import { join, resolve } from "node:path";

const selfTest = process.argv[2] === "--self-test";
const argv = process.argv.slice(selfTest ? 3 : 2);
const compiler = resolve(argv[0] ?? "target/release/tsuzuri");
const programs = Number(argv[1] ?? 20);
const firstSeed = Number(argv[2] ?? 1);
const depth = Number(argv[3] ?? 4);
assert.ok([programs, firstSeed, depth].every(Number.isSafeInteger) && programs > 0 && firstSeed >= 0 && depth >= 1 && depth <= 8,
  "usage: node tests/progfuzz.mjs [compiler] [programs] [first seed] [depth 1..8]");
const allPaths = ["const", "o0", "o3"];
// <<< phase 2
allPaths.push("wasm0", "wasm3");
// >>> phase 2
let kind = "values";
// <<< phase 3
kind = process.env.TSUZURI_PROGFUZZ_KIND ?? "values";
assert.ok(["values", "owned"].includes(kind), "TSUZURI_PROGFUZZ_KIND is values or owned");
// >>> phase 3
let paths = (process.env.TSUZURI_PROGFUZZ_PATHS ?? allPaths.join(",")).split(",");
// <<< phase 3
if (kind === "owned" && !process.env.TSUZURI_PROGFUZZ_PATHS) paths = allPaths.filter((path) => path !== "const");
// >>> phase 3
assert.ok(paths.every((path) => allPaths.includes(path)), `TSUZURI_PROGFUZZ_PATHS names ${allPaths.join(", ")}`);
const jobs = Number(process.env.TSUZURI_PROGFUZZ_JOBS ?? Math.min(4, availableParallelism()));
const mutantName = process.env.TSUZURI_PROGFUZZ_MUTANT ?? "";
let mutant = mutantName;
const outputRoot = resolve(process.env.TSUZURI_PROGFUZZ_DIR ?? "target/progfuzz", selfTest ? "self-test" : ".");
const STEP_LIMIT = 20000;
// <<< phase 2
let minimizeBudget = Number(process.env.TSUZURI_PROGFUZZ_BUDGET ?? 150);
// >>> phase 2
const exeSuffix = process.platform === "win32" ? ".exe" : "";
let generator = generate;
let wantedLines = (program, path, expected) => expected;
// <<< phase 3
if (kind === "owned") generator = generateOwned;
// The wasm paths of an ownership program also report the bytes still live after the call, and so do the native
// paths when the compiler has the Alloc module (F14).
let nativeLeaks = false;
wantedLines = (program, path, expected) => (program.kind === "owned" && (path.startsWith("wasm") || nativeLeaks) ? expected.map((v) => `${v} leaked 0`) : expected);
// >>> phase 3

// ---- random numbers: xorshift64* over BigInt, so that a seed means the same program everywhere ----

function xorshift(seed) {
  let s = BigInt.asUintN(64, BigInt(seed) + 0x9e3779b97f4a7c15n);
  s = BigInt.asUintN(64, (s ^ (s >> 30n)) * 0xbf58476d1ce4e5b9n);
  s = BigInt.asUintN(64, (s ^ (s >> 27n)) * 0x94d049bb133111ebn);
  s ^= s >> 31n;
  if (s === 0n) s = 0x2545f4914f6cdd1dn;
  const next = () => {
    s ^= s >> 12n;
    s ^= BigInt.asUintN(64, s << 25n);
    s ^= s >> 27n;
    return BigInt.asUintN(64, s * 0x2545f4914f6cdd1dn);
  };
  const below = (n) => Number((next() >> 11n) % BigInt(n));
  const pick = (list) => list[below(list.length)];
  return { next, below, pick, chance: (p) => below(1000) < p * 1000, word: () => BigInt.asIntN(64, next()) };
}

// ---- types ----

const INTS = [8, 16, 32, 64].flatMap((bits) => [true, false].map((signed) => ({ k: "int", bits, signed, name: `i${bits}${signed ? "" : "u"}` })));
const I64 = INTS.find((t) => t.name === "i64");
const I32 = INTS.find((t) => t.name === "i32");
const BOOL = { k: "bool", name: "bool" };
const low = (t) => (t.signed ? -(1n << BigInt(t.bits - 1)) : 0n);
const high = (t) => (t.signed ? (1n << BigInt(t.bits - 1)) - 1n : (1n << BigInt(t.bits)) - 1n);
const wrap = (t, v) => (t.signed ? BigInt.asIntN(t.bits, v) : BigInt.asUintN(t.bits, v));
const fits = (t, v) => v >= low(t) && v <= high(t);
const keyOf = (t) => t.k === "tuple" ? `(${t.items.map(keyOf).join("*")})` : t.k === "maybe" ? `Maybe<${keyOf(t.of)}>` : t.k === "arr" ? `[${keyOf(t.of)}]` : t.name;
const showTy = (t) => t.k === "tuple" ? `(${t.items.map(showTy).join(" * ")})` : t.k === "maybe" ? `Maybe<${showTy(t.of)}>` : t.k === "arr" ? `[${showTy(t.of)}]` : t.name;
const tupleTy = (items) => ({ k: "tuple", items });
const maybeTy = (of) => ({ k: "maybe", of });
const arrayTy = (of) => ({ k: "arr", of });
const poolCache = new Map();
// Values at the edges of a type, and the small values that programs compare with.
function pool(t) {
  if (!poolCache.has(t.name)) {
    const values = new Set([low(t), low(t) + 1n, high(t), high(t) - 1n, high(t) / 2n, high(t) / 2n + 1n]);
    for (const v of [0n, 1n, 2n, 3n, 4n, 5n, 7n, 8n, 9n, 15n, 16n, 31n, 32n, 63n, 64n, 100n, 127n, 128n, 255n, 256n, 65535n, 65536n, 0x7fffffffn, 0x80000000n, 0xffffffffn]) values.add(v);
    if (t.signed) for (const v of [-1n, -2n, -3n, -8n, -100n, -127n, -128n, -129n, -255n, -256n, -32768n, -2147483648n]) values.add(v);
    poolCache.set(t.name, [...values].filter((v) => fits(t, v)).sort((a, b) => (a < b ? -1 : a > b ? 1 : 0)));
  }
  return poolCache.get(t.name);
}

// ---- the generator ----

const lit = (ty, v) => ({ t: "lit", ty, v });
const variable = (e) => ({ t: "var", ty: e.ty, n: e.n });

function generate(seed, maxDepth) {
  const random = xorshift(seed);
  const { below, chance, pick } = random;
  const tags = new Set();
  let counter = 0;
  const fresh = (prefix) => `${prefix}${counter++}`;
  const program = { seed, depth: maxDepth, records: [], unions: [], functions: [], inputs: [], tags };
  const weighted = (options) => {
    let roll = below(options.reduce((sum, [weight]) => sum + weight, 0));
    for (const [weight, make] of options) {
      if (roll < weight) return make();
      roll -= weight;
    }
    return null;
  };
  const scalar = () => (chance(0.15) ? BOOL : pick(INTS));
  const intOf = () => pick(INTS);
  const literal = (ty) => {
    if (ty === BOOL) return lit(BOOL, chance(0.5));
    if (chance(0.6)) return lit(ty, pick(pool(ty)));
    return lit(ty, low(ty) + BigInt.asUintN(64, random.next()) % (high(ty) - low(ty) + 1n));
  };
  const ctx = { loops: 0, self: null };

  // The declared types come first so that every function can use them.
  const fieldType = (limit) => weighted([
    [70, () => scalar()],
    [12, () => tupleTy([scalar(), scalar()])],
    [8, () => maybeTy(scalar())],
    [10, () => (limit > 0 ? program.records[below(limit)] : scalar())],
  ]);
  for (let i = 0, count = below(3); i < count; i++) {
    program.records.push({ k: "rec", id: i, name: `R${i}`, fields: Array.from({ length: 1 + below(4) }, (_, j) => ({ n: `f${j}`, ty: fieldType(i) })) });
  }
  for (let i = 0, count = below(3); i < count; i++) {
    const cases = Array.from({ length: 2 + below(3) }, (_, j) => ({
      n: `K${i}_${j}`,
      ty: weighted([
        [25, () => null], [40, () => scalar()], [20, () => tupleTy([scalar(), scalar()])],
        [15, () => (program.records.length ? pick(program.records) : scalar())],
      ]),
    }));
    program.unions.push({ k: "uni", id: i, name: `U${i}`, cases });
  }
  const anyType = (level) => weighted([
    [45, () => scalar()],
    [level > 0 ? 14 : 0, () => tupleTy(Array.from({ length: 2 + below(2) }, () => scalar()))],
    [program.records.length && level > 0 ? 12 : 0, () => pick(program.records)],
    [program.unions.length && level > 0 ? 10 : 0, () => pick(program.unions)],
    [level > 0 ? 8 : 0, () => maybeTy(chance(0.7) ? scalar() : tupleTy([scalar(), scalar()]))],
    [level > 0 ? 8 : 0, () => arrayTy(scalar())],
  ]) ?? scalar();

  const vars = (sc, ty) => sc.filter((e) => keyOf(e.ty) === keyOf(ty));
  const cast = (ty, a) => ({ t: "cast", ty, a });
  const bin = (ty, op, a, b) => ({ t: "bin", ty, op, a, b });
  const cmp = (op, a, b) => ({ t: "cmp", ty: BOOL, op, a, b });

  function gen(ty, sc, d) {
    switch (ty.k) {
      case "int": return genInt(ty, sc, d);
      case "bool": return genBool(sc, d);
      case "tuple": return genTuple(ty, sc, d);
      case "rec": return genRecord(ty, sc, d);
      case "uni": return genUnion(ty, sc, d);
      case "maybe": return genMaybe(ty, sc, d);
      case "arr": return genArray(ty, sc, d);
      default: throw new Error(ty.k);
    }
  }

  // ---- integers ----

  function leafInt(t, sc) {
    const same = vars(sc, t);
    if (same.length && chance(0.65)) return variable(pick(same));
    const others = sc.filter((e) => e.ty.k === "int" && e.ty !== t);
    if (others.length && chance(0.4)) return cast(t, variable(pick(others)));
    return literal(t);
  }
  function genInt(t, sc, d) {
    if (d <= 0) return leafInt(t, sc);
    return weighted([
      [22, () => bin(t, pick(["+", "-", "*"]), gen(t, sc, d - 1), chance(0.3) ? literal(t) : gen(t, sc, d - 1))],
      [8, () => bin(t, pick(["&", "|", "^"]), gen(t, sc, d - 1), chance(0.3) ? literal(t) : gen(t, sc, d - 1))],
      [7, () => division(t, sc, d)],
      [8, () => shift(t, sc, d)],
      [4, () => (t.signed && chance(0.5) ? { t: "neg", ty: t, a: gen(t, sc, d - 1) } : { t: "not", ty: t, a: gen(t, sc, d - 1) })],
      [8, () => {
        const from = pick(INTS.filter((other) => other !== t));
        tags.add(from.bits > t.bits ? "cast_narrow" : from.bits < t.bits ? "cast_widen" : "cast_sign");
        return cast(t, gen(from, sc, d - 1));
      }],
      [6, () => ifNode(t, sc, d)],
      [5, () => letNode(t, sc, d)],
      [5, () => matchInt(t, sc, d)],
      [6, () => loopNode(t, sc, d)],
      [6, () => callNode(t, sc, d)],
      [3, () => selfCall(t, sc, d)],
      [8, () => consume(t, sc, d)],
    ]) ?? leafInt(t, sc);
  }
  // `a / d` and `a % d` never trap: the divisor is bound once and replaced by 1 where it is 0 (or -1).
  function division(t, sc, d) {
    const a = gen(t, sc, d - 1);
    const op = pick(["/", "%"]);
    if (chance(0.35)) {
      const choices = pool(t).filter((v) => v !== 0n && v !== -1n);
      tags.add(t.signed ? "div_literal_signed" : "div_literal_unsigned");
      return { t: "div", ty: t, op, a, b: lit(t, pick(choices)) };
    }
    const g = fresh("g");
    const bad = t.signed
      ? { t: "logic", ty: BOOL, op: "||", a: cmp("==", variable({ n: g, ty: t }), lit(t, 0n)), b: cmp("==", variable({ n: g, ty: t }), lit(t, -1n)) }
      : cmp("==", variable({ n: g, ty: t }), lit(t, 0n));
    tags.add(t.signed ? "div_dynamic_signed" : "div_dynamic_unsigned");
    return {
      t: "let", ty: t, n: g, e: gen(t, sc, d - 1),
      body: { t: "div", ty: t, op, a, b: { t: "if", ty: t, c: bad, a: lit(t, 1n), b: variable({ n: g, ty: t }) } },
    };
  }
  function shift(t, sc, d) {
    const a = gen(t, sc, d - 1);
    const counts = [0n, 1n, BigInt(t.bits - 1), BigInt(t.bits), BigInt(t.bits + 1), BigInt(2 * t.bits + 1), 255n, ...(t.signed ? [-1n, -BigInt(t.bits)] : [])].filter((v) => fits(t, v));
    let c;
    if (chance(0.55)) {
      c = lit(t, pick(counts));
      if (c.v >= BigInt(t.bits)) tags.add("shift_count_over_width");
      if (c.v < 0n) tags.add("shift_count_negative");
    } else {
      c = gen(t, sc, d - 1);
      tags.add("shift_count_dynamic");
    }
    const op = pick(["<<<", ">>>"]);
    if (op === ">>>" && t.signed) tags.add("shift_right_signed");
    return { t: "shift", ty: t, op, a, c };
  }

  // ---- bool ----

  function genBool(sc, d) {
    const same = vars(sc, BOOL);
    if (d <= 0) return same.length && chance(0.5) ? variable(pick(same)) : weighted([[1, () => literal(BOOL)], [2, () => compare(sc, 0)]]);
    return weighted([
      [30, () => compare(sc, d)],
      [12, () => ({ t: "logic", ty: BOOL, op: pick(["&&", "||"]), a: genBool(sc, d - 1), b: genBool(sc, d - 1) })],
      [6, () => ({ t: "lnot", ty: BOOL, a: genBool(sc, d - 1) })],
      [5, () => cmp(pick(["==", "!="]), genBool(sc, d - 1), genBool(sc, d - 1))],
      [8, () => ifNode(BOOL, sc, d)],
      [4, () => letNode(BOOL, sc, d)],
      [5, () => callNode(BOOL, sc, d)],
      [3, () => selfCall(BOOL, sc, d)],
      [5, () => loopNode(BOOL, sc, d)],
      [6, () => consume(BOOL, sc, d)],
      [4, () => (same.length ? variable(pick(same)) : literal(BOOL))],
    ]) ?? literal(BOOL);
  }
  function compare(sc, d) {
    const t = intOf();
    return cmp(pick(["<", "<=", ">", ">=", "==", "!="]), gen(t, sc, d - 1), chance(0.35) ? literal(t) : gen(t, sc, d - 1));
  }

  // ---- aggregates ----

  const leafOrGen = (ty, sc, d) => gen(ty, sc, d - 1);
  function aggregate(ty, sc, d, build) {
    const same = vars(sc, ty);
    if (d <= 0) return same.length && chance(0.6) ? variable(pick(same)) : build(0);
    return weighted([
      [55, () => build(d)],
      [8, () => (same.length ? variable(pick(same)) : build(d))],
      [10, () => ifNode(ty, sc, d)],
      [6, () => letNode(ty, sc, d)],
      [9, () => callNode(ty, sc, d)],
      [4, () => selfCall(ty, sc, d)],
      [5, () => loopNode(ty, sc, d)],
    ]) ?? build(d);
  }
  const genTuple = (ty, sc, d) => aggregate(ty, sc, d, (level) => ({ t: "tuple", ty, items: ty.items.map((item) => leafOrGen(item, sc, level)) }));
  function genRecord(ty, sc, d) {
    return aggregate(ty, sc, d, (level) => {
      const same = vars(sc, ty);
      if (level > 0 && chance(0.3)) {
        const f = pick(ty.fields);
        tags.add("record_update");
        return { t: "update", ty, a: same.length && chance(0.5) ? variable(pick(same)) : genRecord(ty, sc, level - 1), f: f.n, e: leafOrGen(f.ty, sc, level) };
      }
      return { t: "record", ty, fields: ty.fields.map((f) => leafOrGen(f.ty, sc, level)) };
    });
  }
  function genUnion(ty, sc, d) {
    return aggregate(ty, sc, d, (level) => {
      const c = below(ty.cases.length);
      return { t: "ctor", ty, c, p: ty.cases[c].ty ? leafOrGen(ty.cases[c].ty, sc, level) : null };
    });
  }
  function genMaybe(ty, sc, d) {
    return aggregate(ty, sc, d, (level) => (chance(0.6) ? { t: "some", ty, a: leafOrGen(ty.of, sc, level) } : { t: "none", ty }));
  }
  function genArray(ty, sc, d) {
    return aggregate(ty, sc, d, (level) => {
      const length = 1 + below(4);
      if (level > 0 && chance(0.4)) {
        const i = fresh("i");
        tags.add("new_array");
        return { t: "newarray", ty, len: length, i, body: gen(ty.of, [...sc, { n: i, ty: I64 }], level - 1) };
      }
      return { t: "array", ty, items: Array.from({ length }, () => leafOrGen(ty.of, sc, level)) };
    });
  }

  // ---- control flow, shared by every type ----

  const ifNode = (ty, sc, d) => ({ t: "if", ty, c: genBool(sc, d - 1), a: gen(ty, sc, d - 1), b: gen(ty, sc, d - 1) });
  function letNode(ty, sc, d) {
    const bound = anyType(1);
    const n = fresh("v");
    return { t: "let", ty, n, e: gen(bound, sc, d - 1), body: gen(ty, [...sc, { n, ty: bound }], d - 1) };
  }
  function matchInt(ty, sc, d) {
    const st = intOf();
    const values = [...new Set(Array.from({ length: 1 + below(3) }, () => pick(pool(st))))];
    const arms = values.map((v) => ({ v, body: gen(ty, sc, d - 1) }));
    let guard = null;
    if (chance(0.4)) {
      const bind = fresh("m");
      const inner = [...sc, { n: bind, ty: st }];
      guard = { bind, cond: cmp(pick(["<", ">", "<=", ">="]), variable({ n: bind, ty: st }), literal(st)), body: gen(ty, inner, d - 1) };
      tags.add("match_guard");
    }
    tags.add("match_int");
    return { t: "matchint", ty, s: gen(st, sc, d - 1), st, arms, guard, dflt: gen(ty, sc, d - 1) };
  }
  // Takes apart a value of some other type to produce `ty`.
  function consume(ty, sc, d) {
    const options = [];
    for (const r of program.records) {
      const fields = r.fields.filter((f) => keyOf(f.ty) === keyOf(ty));
      if (fields.length) options.push([6, () => ({ t: "field", ty, a: gen(r, sc, d - 1), f: pick(fields).n })]);
    }
    options.push([5, () => {
      const items = [scalar(), scalar()];
      items[below(2)] = ty;
      const names = items.map(() => fresh("t"));
      const inner = [...sc, ...names.map((n, j) => ({ n, ty: items[j] }))];
      tags.add("match_tuple");
      return { t: "matchtuple", ty, s: gen(tupleTy(items), sc, d - 1), names, body: gen(ty, inner, d - 1) };
    }]);
    if (program.unions.length) {
      options.push([6, () => {
        const u = pick(program.unions);
        tags.add("match_union");
        const arms = u.cases.map((c) => {
          const names = !c.ty ? [] : c.ty.k === "tuple" ? c.ty.items.map(() => fresh("u")) : [fresh("u")];
          const types = !c.ty ? [] : c.ty.k === "tuple" ? c.ty.items : [c.ty];
          return { names, body: gen(ty, [...sc, ...names.map((n, j) => ({ n, ty: types[j] }))], d - 1) };
        });
        return { t: "matchuni", ty, s: gen(u, sc, d - 1), arms };
      }]);
    }
    options.push([5, () => {
      const of = scalar();
      const n = fresh("s");
      tags.add("match_maybe");
      return { t: "matchmaybe", ty, s: gen(maybeTy(of), sc, d - 1), n, some: gen(ty, [...sc, { n, ty: of }], d - 1), none: gen(ty, sc, d - 1) };
    }]);
    options.push([5, () => {
      tags.add("array_index");
      return { t: "index", ty, a: gen(arrayTy(ty), sc, d - 1), e: gen(intOf(), sc, d - 1) };
    }]);
    if (ty.k === "int") {
      options.push([2, () => {
        tags.add("array_length");
        const length = { t: "len", ty: I64, a: gen(arrayTy(scalar()), sc, d - 1) };
        return ty === I64 ? length : cast(ty, length);
      }]);
    }
    if (ty === BOOL) options.push([4, () => ({ t: "matchbool", ty, s: genBool(sc, d - 1), a: gen(ty, sc, d - 1), b: gen(ty, sc, d - 1) })]);
    return weighted(options);
  }
  function loopNode(ty, sc, d) {
    if (d <= 1 || ctx.loops >= 2) return null;
    ctx.loops++;
    try {
      const acc = fresh("a");
      const init = gen(ty, sc, d - 1);
      const inner = [...sc, { n: acc, ty }];
      if (chance(0.7)) {
        const it = intOf();
        const i = fresh("i");
        const bounds = loopBounds(it, sc, d);
        const scope = [...inner, { n: i, ty: it }];
        const exit = chance(0.3) ? { kind: pick(["break", "continue"]), cond: genBool(scope, d - 2) } : null;
        if (exit) tags.add(`loop_${exit.kind}`);
        tags.add("for_loop");
        return { t: "for", ty, acc, i, it, init, ...bounds, exit, body: gen(ty, scope, d - 1) };
      }
      const exit = chance(0.3) ? { kind: pick(["break", "continue"]), cond: genBool(inner, d - 2) } : null;
      if (exit) tags.add(`loop_${exit.kind}`);
      tags.add("while_loop");
      return { t: "while", ty, acc, c: fresh("c"), n: 1 + below(6), init, extra: chance(0.4) ? genBool(inner, d - 2) : null, exit, body: gen(ty, inner, d - 1) };
    } finally {
      ctx.loops--;
    }
  }
  // Ranges of at most eight iterations, some of them empty, some of them ending at the edge of the type.
  function loopBounds(it, sc, d) {
    const L = (v) => lit(it, v);
    const small = () => BigInt(below(4));
    return weighted([
      [28, () => { const lo = small(); return { lo: L(lo), step: null, hi: L(lo + BigInt(below(6))) }; }],
      [8, () => { tags.add("loop_empty"); const lo = small(); return { lo: L(lo + 3n), step: null, hi: L(lo) }; }],
      [14, () => { const lo = small(); return { lo: L(lo), step: L(BigInt(2 + below(2))), hi: L(lo + BigInt(3 + below(8))) }; }],
      [16, () => { tags.add("loop_to_type_edge"); const top = high(it); return { lo: L(top - BigInt(below(6))), step: chance(0.5) ? null : L(BigInt(1 + below(3))), hi: L(top) }; }],
      [it.signed ? 14 : 0, () => { tags.add("loop_descending"); return { lo: L(BigInt(2 + below(6))), step: L(-BigInt(1 + below(3))), hi: L(-BigInt(below(4))) }; }],
      [it.signed ? 8 : 0, () => { tags.add("loop_from_type_edge"); const bottom = low(it); return chance(0.5) ? { lo: L(bottom), step: null, hi: L(bottom + BigInt(below(6))) } : { lo: L(bottom + BigInt(below(6))), step: L(-1n), hi: L(bottom) }; }],
      [10, () => { tags.add("loop_dynamic_bound"); return { lo: L(0n), step: null, hi: bin(it, "+", L(BigInt(below(4))), bin(it, "&", gen(it, sc, d - 2), L(3n))) }; }],
    ]);
  }

  // ---- calls ----

  function callNode(ty, sc, d) {
    const candidates = program.functions.filter((f) => keyOf(f.result) === keyOf(ty));
    if (!candidates.length || d <= 0) return null;
    const f = pick(candidates);
    tags.add("call");
    return { t: "call", ty, f: f.name, args: f.params.map((p, j) => (f.rec && j === 0 ? lit(I64, BigInt(below(5))) : gen(p.ty, sc, d - 1))) };
  }
  // A recursive function calls itself with the fuel minus one, in the branch where the fuel is positive.
  function selfCall(ty, sc, d) {
    const me = ctx.self;
    if (!me || me.calls >= 2 || keyOf(me.result) !== keyOf(ty) || d <= 0) return null;
    me.calls++;
    tags.add("self_call");
    return { t: "call", ty, f: me.name, args: me.params.map((p, j) => (j === 0 ? bin(I64, "-", variable(p), lit(I64, 1n)) : gen(p.ty, sc, d - 1))) };
  }

  // ---- functions and the entry ----

  const helpers = 2 + below(4);
  for (let index = 0; index < helpers; index++) {
    const recursive = chance(0.25);
    const params = Array.from({ length: 1 + below(3) }, (_, j) => ({ n: `p${j}`, ty: anyType(1) }));
    if (recursive) params.unshift({ n: "fuel", ty: I64 });
    const result = anyType(1);
    const f = { name: `f${index}`, params, result, rec: recursive, body: null };
    ctx.loops = 0;
    if (recursive) {
      ctx.self = { name: f.name, params, result, calls: 0 };
      const fuel = variable(params[0]);
      const base = (() => { const saved = ctx.self; ctx.self = null; try { return gen(result, params, maxDepth - 1); } finally { ctx.self = saved; } })();
      let step = gen(result, params, maxDepth);
      if (!ctx.self.calls) {
        // The recursion must happen at least once, so that the fuel is what ends the run.
        const again = { t: "call", ty: result, f: f.name, args: params.map((p, j) => (j === 0 ? bin(I64, "-", fuel, lit(I64, 1n)) : gen(p.ty, params, maxDepth - 1))) };
        step = { t: "if", ty: result, c: genBool(params, 1), a: again, b: step };
        tags.add("self_call");
      }
      f.body = { t: "if", ty: result, c: cmp("<=", fuel, lit(I64, 0n)), a: base, b: step };
      ctx.self = null;
      tags.add("recursive_function");
    } else {
      f.body = gen(result, params, maxDepth);
    }
    program.functions.push(f);
  }

  // The entry: typed inputs cut from `k`, one call of every helper, and one more generated term.
  const toI64 = (e, sc) => {
    const ty = e.ty;
    switch (ty.k) {
      case "int": return ty === I64 ? e : cast(I64, e);
      case "bool": return { t: "if", ty: I64, c: e, a: lit(I64, 1n), b: lit(I64, 0n) };
      case "tuple": {
        const names = ty.items.map(() => fresh("t"));
        return { t: "matchtuple", ty: I64, s: e, names, body: mix(names.map((n, j) => toI64(variable({ n, ty: ty.items[j] })))) };
      }
      case "rec": {
        const n = fresh("r");
        return { t: "let", ty: I64, n, e, body: mix(ty.fields.map((f) => toI64({ t: "field", ty: f.ty, a: variable({ n, ty }), f: f.n }))) };
      }
      case "uni": {
        const arms = ty.cases.map((c, index) => {
          const names = !c.ty ? [] : c.ty.k === "tuple" ? c.ty.items.map(() => fresh("u")) : [fresh("u")];
          const types = !c.ty ? [] : c.ty.k === "tuple" ? c.ty.items : [c.ty];
          return { names, body: mix([lit(I64, BigInt(index + 1)), ...names.map((n, j) => toI64(variable({ n, ty: types[j] })))]) };
        });
        return { t: "matchuni", ty: I64, s: e, arms };
      }
      case "maybe": {
        const n = fresh("s");
        return { t: "matchmaybe", ty: I64, s: e, n, some: mix([lit(I64, 7n), toI64(variable({ n, ty: ty.of }))]), none: lit(I64, -5n) };
      }
      case "arr": {
        const n = fresh("r");
        const a = variable({ n, ty });
        return { t: "let", ty: I64, n, e, body: mix([{ t: "len", ty: I64, a }, toI64({ t: "index", ty: ty.of, a, e: lit(I64, 0n) }), toI64({ t: "index", ty: ty.of, a, e: lit(I64, 5n) })]) };
      }
      default: throw new Error(ty.k);
    }
  };
  // acc * 31 + item, wrapped in i64.
  const mix = (items) => items.reduce((acc, item) => bin(I64, "+", bin(I64, "*", acc, lit(I64, 31n)), item));
  const k = { n: "k", ty: I64 };
  const entryScope = [k];
  const derived = [];
  for (let j = 0, count = 2 + below(3); j < count; j++) {
    const ty = scalar();
    const n = `x${j}`;
    const shifted = { t: "shift", ty: I64, op: ">>>", a: variable(k), c: lit(I64, BigInt(below(64))) };
    derived.push({ n, ty, e: ty === BOOL ? cmp("<", { t: "cast", ty: I32, a: shifted }, lit(I32, 0n)) : cast(ty, shifted) });
    entryScope.push({ n, ty });
  }
  ctx.loops = 0;
  const terms = program.functions.map((f) => toI64({ t: "call", ty: f.result, f: f.name, args: f.params.map((p, j) => (f.rec && j === 0 ? lit(I64, BigInt(below(5))) : gen(p.ty, entryScope, maxDepth - 1))) }));
  terms.push(gen(I64, entryScope, maxDepth));
  let body = mix(terms);
  for (const e of derived.reverse()) body = { t: "let", ty: I64, n: e.n, e: e.e, body };
  program.functions.push({ name: "run", params: [k], result: I64, rec: false, body });
  program.inputs = [0n, 1n, -1n, (1n << 63n) - 1n, -(1n << 63n), random.word(), random.word(), BigInt(below(1000))];
  return program;
}

// ---- source text ----

const patternLiteral = (ty, v) => (ty === BOOL ? String(v) : `${v}${ty.name}`);
const literalText = (ty, v) => (ty === BOOL ? String(v) : v < 0n ? `(${v}${ty.name})` : `${v}${ty.name}`);

function render(n, ctx) {
  const r = (x) => render(x, ctx);
  switch (n.t) {
    case "lit": return literalText(n.ty, n.v);
    case "var": return n.n;
    case "bin": case "div": case "cmp": case "logic": return `(${r(n.a)} ${n.op} ${r(n.b)})`;
    case "shift": return `(${r(n.a)} ${n.op} ${r(n.c)})`;
    case "neg": return `(-${r(n.a)})`;
    case "not": return `(~${r(n.a)})`;
    case "lnot": return `(!${r(n.a)})`;
    case "cast": return `(${r(n.a)} as ${n.ty.name})`;
    case "if": return `(if ${r(n.c)} then ${r(n.a)} else ${r(n.b)})`;
    case "let": return `({ let ${n.n}: ${showTy(n.e.ty)} = ${r(n.e)}; ${r(n.body)} })`;
    case "matchint": {
      const arms = n.arms.map((arm) => `| ${patternLiteral(n.st, arm.v)} -> ${r(arm.body)}`).join(" ");
      const guard = n.guard ? ` | ${n.guard.bind} when ${r(n.guard.cond)} -> ${r(n.guard.body)}` : "";
      return `(match ${r(n.s)} with ${arms}${guard} | _ -> ${r(n.dflt)})`;
    }
    case "matchbool": return `(match ${r(n.s)} with | true -> ${r(n.a)} | false -> ${r(n.b)})`;
    case "matchtuple": return `(match ${r(n.s)} with | (${n.names.join(", ")}) -> ${r(n.body)})`;
    case "matchuni": {
      const arms = n.s.ty.cases.map((c, index) => {
        const names = n.arms[index].names;
        const pattern = !c.ty ? c.n : c.ty.k === "tuple" ? `${c.n} (${names.join(", ")})` : `${c.n} ${names[0]}`;
        return `| ${pattern} -> ${r(n.arms[index].body)}`;
      });
      return `(match ${r(n.s)} with ${arms.join(" ")})`;
    }
    case "matchmaybe": return `(match ${r(n.s)} with | Maybe.Some ${n.n} -> ${r(n.some)} | Maybe.None -> ${r(n.none)})`;
    case "field": return `(${r(n.a)}).${n.f}`;
    case "tuple": return `(${n.items.map(r).join(", ")})`;
    case "record": return `(${n.ty.name} { ${n.ty.fields.map((f, j) => `${f.n}: ${r(n.fields[j])}`).join(", ")} })`;
    case "update": return `({ ${r(n.a)} with ${n.f} = ${r(n.e)} })`;
    case "ctor": return n.p ? `(${n.ty.cases[n.c].n} ${r(n.p)})` : n.ty.cases[n.c].n;
    case "some": return `(Maybe.Some ${r(n.a)})`;
    case "none": {
      const ix = `ix${ctx.n++}`;
      return `({ let ${ix}: ${showTy(n.ty)} = Maybe.None; ${ix} })`;
    }
    case "array": return `[${n.items.map(r).join(", ")}]`;
    case "newarray": return `(new [${showTy(n.ty.of)}](${n.len}, \\${n.i} -> ${r(n.body)}))`;
    case "index": {
      // The index is bound outside the brackets: a range `for` inside `xs[...]` is a parse error (known bug KB1).
      const ix = `ix${ctx.n++}`;
      return `({ let ${ix}: ${showTy(n.a.ty)} = ${r(n.a)}; let ${ix}j: i64u = ((${r(n.e)}) as i64u); ${ix}[(${ix}j % ((${ix}.length) as i64u)) as i64] })`;
    }
    case "len": {
      const ix = `ix${ctx.n++}`;
      return `({ let ${ix}: ${showTy(n.a.ty)} = ${r(n.a)}; ${ix}.length })`;
    }
    case "call": return `(${n.f} ${n.args.map(r).join(" ")})`;
    case "for": {
      const range = n.step ? `${r(n.lo)} .. ${r(n.step)} .. ${r(n.hi)}` : `${r(n.lo)} .. ${r(n.hi)}`;
      const exit = n.exit ? `if ${r(n.exit.cond)} then ${n.exit.kind} else (); ` : "";
      return `({ let mut ${n.acc}: ${showTy(n.ty)} = ${r(n.init)}; for ${n.i} in ${range} do { ${exit}${n.acc} = ${r(n.body)}; () }; ${n.acc} })`;
    }
    case "while": {
      const test = `(${n.c} < ${n.n}i32)`;
      const head = n.extra ? `(${test} && ${r(n.extra)})` : test;
      const exit = n.exit ? `if ${r(n.exit.cond)} then ${n.exit.kind} else (); ` : "";
      return `({ let mut ${n.acc}: ${showTy(n.ty)} = ${r(n.init)}; let mut ${n.c}: i32 = 0i32; while ${head} do { ${n.c} = (${n.c} + 1i32); ${exit}${n.acc} = ${r(n.body)}; () }; ${n.acc} })`;
    }
    default: throw new Error(`render ${n.t}`);
  }
}

const int64Text = (v) => (v < 0n ? `(${v}i64)` : `${v}i64`);

// `tail` is "run" (an executable that prints run(k) for every input), "const" (an executable that prints
// constants the compiler evaluates), or "lib" (the entry exported for a WebAssembly host).
function source(program, tail) {
  // <<< phase 3
  if (program.kind === "owned") return sourceOwned(program, tail);
  // >>> phase 3
  const lines = [];
  for (const r of program.records) lines.push(`record ${r.name} { ${r.fields.map((f) => `${f.n}: ${showTy(f.ty)}`).join(", ")} }`);
  for (const u of program.unions) lines.push(`union ${u.name} = ${u.cases.map((c) => (c.ty ? `${c.n} of ${showTy(c.ty)}` : c.n)).join(" | ")}`);
  const ctx = { n: 0 };
  for (const f of program.functions) {
    const rec = f.rec ? " rec" : "";
    const head = tail === "lib" && f.name === "run" ? "export const def" : "const def";
    lines.push("", `${head}${rec} ${f.name} :: ${[...f.params.map((p) => showTy(p.ty)), showTy(f.result)].join(" -> ")}`, `fn${rec} ${f.name} ${f.params.map((p) => p.n).join(" ")} = ${render(f.body, ctx)}`);
  }
  if (tail === "run") {
    lines.push("", "def main :: [string] -> i32 = \\args ->", "    let z = args.length");
    for (const k of program.inputs) lines.push(`    do! IO.write_line (to_string (run (${int64Text(k)} + z)))`);
    lines.push("    0");
  } else if (tail === "const") {
    lines.push("");
    program.inputs.forEach((k, j) => lines.push(`const C${j}: i64 = run ${int64Text(k)}`));
    lines.push("", "def main :: unit -> i32 = \\() ->");
    program.inputs.forEach((_, j) => lines.push(`    do! IO.write_line (to_string C${j})`));
    lines.push("    0");
  }
  return `${lines.join("\n")}\n`;
}

// ---- the evaluator ----

class Trap extends Error {}
class Limit extends Error {}
const check = (condition, message) => { if (!condition) throw new Trap(message); };

function evaluate(program, input) {
  // <<< phase 3
  if (program.kind === "owned") return evaluateOwned(program, input);
  // >>> phase 3
  let steps = 0;
  const functions = new Map(program.functions.map((f) => [f.name, f]));
  const call = (f, args) => {
    const env = new Map();
    f.params.forEach((p, j) => env.set(p.n, args[j]));
    return ev(f.body, env);
  };
  const shiftCount = (t, c) => {
    const masked = Number(BigInt.asUintN(t.bits, c) & BigInt(t.bits - 1));
    return mutant === "shift-saturating" ? Math.min(Number(BigInt.asUintN(t.bits, c)), 1024) : masked;
  };
  const ev = (n, env) => {
    if (++steps > STEP_LIMIT) throw new Limit();
    switch (n.t) {
      case "lit": return n.v;
      case "var": check(env.has(n.n), `unbound ${n.n}`); return env.get(n.n);
      case "bin": {
        const a = ev(n.a, env);
        const b = ev(n.b, env);
        const v = n.op === "+" ? a + b : n.op === "-" ? a - b : n.op === "*" ? a * b : n.op === "&" ? a & b : n.op === "|" ? a | b : a ^ b;
        return mutant === "mul-unwrapped" && n.op === "*" ? v : wrap(n.ty, v);
      }
      case "div": {
        const a = ev(n.a, env);
        const b = ev(n.b, env);
        check(b !== 0n, "division by zero");
        check(!(n.ty.signed && a === low(n.ty) && b === -1n), "division overflow");
        let q = a / b;
        let m = a % b;
        if (mutant === "div-floor" && m !== 0n && (m < 0n) !== (b < 0n)) { q -= 1n; m += b; }
        return n.op === "/" ? q : m;
      }
      case "shift": {
        const a = ev(n.a, env);
        const count = shiftCount(n.ty, ev(n.c, env));
        if (n.op === "<<<") return wrap(n.ty, a << BigInt(count));
        return mutant === "shr-logical" ? wrap(n.ty, BigInt.asUintN(n.ty.bits, a) >> BigInt(count)) : a >> BigInt(count);
      }
      case "neg": return wrap(n.ty, -ev(n.a, env));
      case "not": return wrap(n.ty, ~ev(n.a, env));
      case "cast": {
        const a = ev(n.a, env);
        if (mutant === "cast-saturate") return a < low(n.ty) ? low(n.ty) : a > high(n.ty) ? high(n.ty) : a;
        return wrap(n.ty, a);
      }
      case "cmp": {
        const a = ev(n.a, env);
        const b = ev(n.b, env);
        return { "<": a < b, "<=": a <= b, ">": a > b, ">=": a >= b, "==": a === b, "!=": a !== b }[n.op];
      }
      case "logic": return n.op === "&&" ? ev(n.a, env) && ev(n.b, env) : ev(n.a, env) || ev(n.b, env);
      case "lnot": return !ev(n.a, env);
      case "if": return ev(n.c, env) ? ev(n.a, env) : ev(n.b, env);
      case "let": return ev(n.body, new Map(env).set(n.n, ev(n.e, env)));
      case "matchint": {
        const s = ev(n.s, env);
        for (const arm of n.arms) if (arm.v === s) return ev(arm.body, env);
        if (n.guard) {
          const inner = new Map(env).set(n.guard.bind, s);
          if (ev(n.guard.cond, inner)) return ev(n.guard.body, inner);
        }
        return ev(n.dflt, env);
      }
      case "matchbool": return ev(n.s, env) ? ev(n.a, env) : ev(n.b, env);
      case "matchtuple": {
        const s = ev(n.s, env);
        const inner = new Map(env);
        n.names.forEach((name, j) => inner.set(name, s[j]));
        return ev(n.body, inner);
      }
      case "matchuni": {
        const s = ev(n.s, env);
        const arm = n.arms[s.c];
        const inner = new Map(env);
        const payloadTy = n.s.ty.cases[s.c].ty;
        if (payloadTy?.k === "tuple") arm.names.forEach((name, j) => inner.set(name, s.p[j]));
        else if (payloadTy) inner.set(arm.names[0], s.p);
        return ev(arm.body, inner);
      }
      case "matchmaybe": {
        const s = ev(n.s, env);
        return s.c === 1 ? ev(n.some, new Map(env).set(n.n, s.p)) : ev(n.none, env);
      }
      case "field": return ev(n.a, env)[n.a.ty.fields.findIndex((f) => f.n === n.f)];
      case "tuple": return n.items.map((item) => ev(item, env));
      case "record": return n.fields.map((field) => ev(field, env));
      case "update": {
        const r = [...ev(n.a, env)];
        r[n.ty.fields.findIndex((f) => f.n === n.f)] = ev(n.e, env);
        return r;
      }
      case "ctor": return { c: n.c, p: n.p ? ev(n.p, env) : null };
      case "some": return { c: 1, p: ev(n.a, env) };
      case "none": return { c: 0, p: null };
      case "array": return n.items.map((item) => ev(item, env));
      case "newarray": return Array.from({ length: n.len }, (_, j) => ev(n.body, new Map(env).set(n.i, BigInt(j))));
      case "index": {
        const a = ev(n.a, env);
        return a[Number(BigInt.asUintN(64, ev(n.e, env)) % BigInt(a.length))];
      }
      case "len": return BigInt(ev(n.a, env).length);
      case "call": return call(functions.get(n.f), n.args.map((arg) => ev(arg, env)));
      case "for": {
        let acc = ev(n.init, env);
        const first = ev(n.lo, env);
        const step = n.step ? ev(n.step, env) : 1n;
        const last = ev(n.hi, env);
        check(step !== 0n, "range step is zero");
        const up = step > 0n;
        const inclusive = mutant !== "loop-exclusive";
        for (let i = first; up ? (inclusive ? i <= last : i < last) : (inclusive ? i >= last : i > last);) {
          if (++steps > STEP_LIMIT) throw new Limit();
          const frame = new Map(env).set(n.acc, acc).set(n.i, i);
          const skip = n.exit && ev(n.exit.cond, frame);
          if (skip && n.exit.kind === "break") break;
          if (!skip) acc = ev(n.body, frame);
          const next = i + step;
          if (!fits(n.it, next)) break;
          i = next;
        }
        return acc;
      }
      case "while": {
        let acc = ev(n.init, env);
        for (let count = 0; ; ) {
          if (++steps > STEP_LIMIT) throw new Limit();
          const frame = new Map(env).set(n.acc, acc);
          if (!(count < n.n && (!n.extra || ev(n.extra, frame)))) break;
          count++;
          const skip = n.exit && ev(n.exit.cond, frame);
          if (skip && n.exit.kind === "break") break;
          if (!skip) acc = ev(n.body, frame);
        }
        return acc;
      }
      default: throw new Error(`evaluate ${n.t}`);
    }
  };
  return call(functions.get("run"), [input]);
}

// <<< phase 3
// ---- ownership programs (Phase 3) ----
//
// A program is a list of steps over owned variables: a Vec<i32> ("vec"), a string ("str") and a Vec<string>
// ("q"). The generator knows which variables are alive, so a variable is never used after it was moved; a
// variable that a branch may or may not have moved is dead for the rest of the program (and is dropped, or
// was dropped, by the compiler at the end of `run`). The wasm paths are built with --allocator counting and
// check that every block is freed when `run` returns.

const OWNED_HELPERS = [
  "def h_len :: ref Vec<i32> -> i64\nfn h_len v = (Vec.length v)",
  "def h_sum :: ref Vec<i32> -> i64\nfn h_sum v = ({ let mut acc: i64 = 0i64; for i in 0i64 .. ((Vec.length v) - 1i64) do { acc = (acc + ((v[i]) as i64)); () }; acc })",
  "def h_grow :: Vec<i32> -> i32 -> Vec<i32>\nfn h_grow v x = (Vec.push (x + 1i32) (Vec.push x v))",
  "def h_pop :: Vec<i32> -> Vec<i32>\nfn h_pop v = (match Vec.pop v with | (rest, _) -> rest)",
  "def h_drop :: Vec<i32> -> i64\nfn h_drop v = (Vec.length (ref v))",
  "def h_arr :: Vec<i32> -> i64\nfn h_arr v =\n    let a: [i32] = Vec.to_array v\n    ((Array.sum (ref a)) as i64)",
  "def h_cat :: string -> string -> string\nfn h_cat a b = (a + b)",
  "def h_slen :: ref string -> i64\nfn h_slen s = (String.length s)",
  "def h_take :: string -> i64\nfn h_take s = (String.length (ref s))",
  "def h_qlen :: ref Vec<string> -> i64\nfn h_qlen q = (Vec.length q)",
].join("\n\n");

function generateOwned(seed, maxDepth) {
  const random = xorshift(seed);
  const { below, chance, pick } = random;
  const tags = new Set();
  const steps = [];
  const live = [];
  const scalars = ["k"];
  let counter = 0;
  const fresh = (prefix) => `${prefix}${counter++}`;
  const e32 = (d) => (d <= 0 || chance(0.4) ? pick([{ k: "lit", v: pick(pool(I32)) }, { k: "sc", n: pick(scalars) }]) : { k: pick(["add", "mul"]), a: e32(d - 1), b: e32(d - 1) });
  const cond = () => ({ n: pick(scalars), op: pick(["<", ">", "==", "<=", ">=", "!="]), v: pick([-1n, 0n, 1n, 2n, 3n, 5n, 7n]) });
  const text = () => Array.from({ length: below(5) }, () => pick(["a", "b", "x", "y", "z", " "])).join("");
  const of = (kind) => live.filter((e) => e.kind === kind);
  const add = (step, tag) => { steps.push(step); tags.add(tag ?? step.op); };
  const bind = (op, step) => { const out = fresh("n"); scalars.push(out); add({ op, out, ...step }); };
  const kill = (name) => { live.splice(live.findIndex((e) => e.n === name), 1); };
  const born = (kind, op, step) => { const n = fresh(kind === "vec" ? "v" : kind === "str" ? "s" : "q"); live.push({ n, kind }); add({ op, n, ...step }); return n; };
  const count = 5 + below(4 + 3 * maxDepth);
  for (let j = 0; j < count; j++) {
    const vecs = of("vec");
    const strs = of("str");
    const qs = of("q");
    const options = [
      [live.length < 5 ? 6 : 0, () => {
        const init = pick(["empty", "capacity", "array"]);
        born("vec", "newvec", { init, items: init === "array" ? Array.from({ length: 1 + below(3) }, () => pick(pool(I32))) : [] });
      }],
      [live.length < 5 ? 3 : 0, () => born("str", "newstr", { text: text() })],
      [live.length < 5 ? 3 : 0, () => born("q", "newq", {})],
      [vecs.length ? 5 : 0, () => add({ op: "push", n: pick(vecs).n, e: e32(2) })],
      [vecs.length ? 3 : 0, () => add({ op: "grow", n: pick(vecs).n, e: e32(2) })],
      [vecs.length ? 3 : 0, () => add({ op: "pop", n: pick(vecs).n })],
      [vecs.length ? 3 : 0, () => add({ op: "loop", n: pick(vecs).n, i: fresh("i"), count: below(6) - 1 })],
      [vecs.length ? 2 : 0, () => add({ op: "whilepop", n: pick(vecs).n })],
      [vecs.length ? 3 : 0, () => bind("len", { n: pick(vecs).n })],
      [vecs.length ? 3 : 0, () => bind("sum", { n: pick(vecs).n })],
      [vecs.length && live.length < 6 ? 3 : 0, () => born("vec", "clone", { from: pick(vecs).n })],
      [vecs.length ? 3 : 0, () => { const v = pick(vecs).n; bind("drop", { n: v }); kill(v); }],
      [vecs.length ? 3 : 0, () => { const v = pick(vecs).n; bind("arr", { n: v }); kill(v); }],
      [vecs.length ? 3 : 0, () => add({ op: "branch", n: pick(vecs).n, cond: cond(), e: e32(1) })],
      [vecs.length ? 3 : 0, () => { const v = pick(vecs).n; bind("cdrop", { n: v, cond: cond(), other: pick([-1n, 0n, 9n]) }); kill(v); }],
      [vecs.length ? 2 : 0, () => { const v = pick(vecs).n; bind("mdrop", { n: v, on: pick(scalars), v: pick([0n, 1n, 2n]), other: pick([-1n, 0n, 9n]) }); kill(v); }],
      [strs.length ? 3 : 0, () => add({ op: "append", n: pick(strs).n, text: text() })],
      [strs.length ? 2 : 0, () => bind("slen", { n: pick(strs).n })],
      [strs.length ? 2 : 0, () => { const s = pick(strs).n; bind("take", { n: s }); kill(s); }],
      [strs.length >= 2 ? 3 : 0, () => {
        const first = pick(strs);
        const second = pick(strs.filter((e) => e !== first));
        kill(first.n); kill(second.n);
        born("str", "cat", { a: first.n, b: second.n });
      }],
      [qs.length ? 3 : 0, () => add({ op: "qpush", n: pick(qs).n, text: text() })],
      [qs.length && strs.length ? 3 : 0, () => { const s = pick(strs).n; add({ op: "qpushs", n: pick(qs).n, s }); kill(s); }],
      [qs.length ? 3 : 0, () => add({ op: "qpop", n: pick(qs).n })],
      [qs.length ? 2 : 0, () => add({ op: "qclear", n: pick(qs).n })],
      [qs.length ? 2 : 0, () => bind("qlen", { n: pick(qs).n })],
    ];
    let roll = below(options.reduce((sum, [weight]) => sum + weight, 0));
    for (const [weight, make] of options) {
      if (roll < weight) { make(); break; }
      roll -= weight;
    }
  }
  return { kind: "owned", seed, depth: maxDepth, steps, inputs: [-3n, -1n, 0n, 1n, 2n, 3n, 5n, 100n], tags };
}

// What the steps leave alive and which scalars they bind; throws Trap when a step uses a dead variable.
function replay(program) {
  const live = new Map();
  const scalars = ["k"];
  const use = (n, kind) => { check(live.get(n) === kind, `${n} is not a live ${kind}`); };
  for (const s of program.steps) {
    switch (s.op) {
      case "newvec": live.set(s.n, "vec"); break;
      case "newstr": live.set(s.n, "str"); break;
      case "newq": live.set(s.n, "q"); break;
      case "push": case "grow": case "pop": case "loop": case "whilepop": case "branch": use(s.n, "vec"); break;
      case "len": case "sum": use(s.n, "vec"); scalars.push(s.out); break;
      case "clone": use(s.from, "vec"); live.set(s.n, "vec"); break;
      case "drop": case "arr": case "cdrop": case "mdrop": use(s.n, "vec"); live.delete(s.n); scalars.push(s.out); break;
      case "append": use(s.n, "str"); break;
      case "slen": use(s.n, "str"); scalars.push(s.out); break;
      case "take": use(s.n, "str"); live.delete(s.n); scalars.push(s.out); break;
      case "cat": use(s.a, "str"); use(s.b, "str"); live.delete(s.a); live.delete(s.b); live.set(s.n, "str"); break;
      case "qpush": case "qpop": case "qclear": use(s.n, "q"); break;
      case "qpushs": use(s.n, "q"); use(s.s, "str"); live.delete(s.s); break;
      case "qlen": use(s.n, "q"); scalars.push(s.out); break;
      default: throw new Error(`replay ${s.op}`);
    }
    for (const name of [s.cond?.n, s.on, ...(s.e ? e32Names(s.e) : [])]) if (name) check(scalars.includes(name), `${name} is not bound`);
  }
  return { live, scalars };
}
const e32Names = (e) => (e.k === "sc" ? [e.n] : e.a ? [...e32Names(e.a), ...e32Names(e.b)] : []);

const e32Text = (e) => (e.k === "lit" ? literalText(I32, e.v) : e.k === "sc" ? `(${e.n} as i32)` : `(${e32Text(e.a)} ${e.k === "add" ? "+" : "*"} ${e32Text(e.b)})`);
const condText = (c) => `(${c.n} ${c.op} ${literalText(I64, c.v)})`;

function sourceOwned(program, tail) {
  const lines = [OWNED_HELPERS, "", `${tail === "lib" ? "export def" : "def"} run :: i64 -> i64`, "fn run k ="];
  for (const s of program.steps) {
    const t = s.text !== undefined ? JSON.stringify(s.text) : "";
    lines.push("    " + (() => {
      switch (s.op) {
        case "newvec": return `let mut ${s.n}: Vec<i32> = ${s.init === "empty" ? "Vec.empty()" : s.init === "capacity" ? "Vec.with_capacity 4" : `Vec.of_array [${s.items.map((v) => literalText(I32, v)).join(", ")}]`}`;
        case "newstr": return `let mut ${s.n}: string = ${t}`;
        case "newq": return `let mut ${s.n}: Vec<string> = Vec.empty()`;
        case "push": return `${s.n} = Vec.push ${e32Text(s.e)} ${s.n}`;
        case "grow": return `${s.n} = h_grow ${s.n} ${e32Text(s.e)}`;
        case "pop": return `${s.n} = h_pop ${s.n}`;
        case "loop": return `for ${s.i} in 0i64 .. ${literalText(I64, BigInt(s.count))} do { ${s.n} = Vec.push (${s.i} as i32) ${s.n}; () }`;
        case "whilepop": return `while ((Vec.length (ref ${s.n})) > 1i64) do { ${s.n} = h_pop ${s.n}; () }`;
        case "len": return `let ${s.out} = h_len (ref ${s.n})`;
        case "sum": return `let ${s.out} = h_sum (ref ${s.n})`;
        case "clone": return `let mut ${s.n}: Vec<i32> = Vec.clone (ref ${s.from})`;
        case "drop": return `let ${s.out} = h_drop ${s.n}`;
        case "arr": return `let ${s.out} = h_arr ${s.n}`;
        case "branch": return `${s.n} = (if ${condText(s.cond)} then h_grow ${s.n} ${e32Text(s.e)} else h_pop ${s.n})`;
        case "cdrop": return `let ${s.out} = (if ${condText(s.cond)} then h_drop ${s.n} else ${literalText(I64, s.other)})`;
        case "mdrop": return `let ${s.out} = (match ${s.on} with | ${s.v}i64 -> h_drop ${s.n} | _ -> ${literalText(I64, s.other)})`;
        case "append": return `${s.n} = ${s.n} + ${t}`;
        case "slen": return `let ${s.out} = h_slen (ref ${s.n})`;
        case "take": return `let ${s.out} = h_take ${s.n}`;
        case "cat": return `let mut ${s.n}: string = h_cat ${s.a} ${s.b}`;
        case "qpush": return `${s.n} = Vec.push ${t} ${s.n}`;
        case "qpushs": return `${s.n} = Vec.push ${s.s} ${s.n}`;
        case "qpop": return `${s.n} = (match Vec.pop ${s.n} with | (rest, _) -> rest)`;
        case "qclear": return `${s.n} = Vec.clear ${s.n}`;
        case "qlen": return `let ${s.out} = h_qlen (ref ${s.n})`;
        default: throw new Error(s.op);
      }
    })());
  }
  const { live, scalars } = replay(program);
  const items = [...scalars, ...[...live].map(([n, kind]) => `(${kind === "vec" ? "h_len" : kind === "str" ? "h_slen" : "h_qlen"} (ref ${n}))`)];
  lines.push(`    ${items.reduce((acc, item) => `((${acc} * 31i64) + ${item})`, "0i64")}`);
  if (tail === "run") {
    lines.push("", "def main :: [string] -> i32 = \\args ->", "    let z = args.length");
    program.inputs.forEach((k, j) => {
      if (nativeLeaks) lines.push(`    let b${j} = Alloc.snapshot ()`, `    let r${j} = run (${int64Text(k)} + z)`, `    let d${j} = Alloc.since b${j}`, `    do! IO.write_line (to_string r${j} + " leaked " + to_string (d${j}.allocs - d${j}.frees))`);
      else lines.push(`    do! IO.write_line (to_string (run (${int64Text(k)} + z)))`);
    });
    lines.push("    0");
  }
  return `${lines.join("\n")}\n`;
}

function evaluateOwned(program, input) {
  const values = new Map([["k", input]]);
  const owned = new Map();
  const get = (n, kind) => { check(owned.has(n) && owned.get(n).kind === kind, `${n} is not a live ${kind}`); return owned.get(n).value; };
  const e32 = (e) => (e.k === "lit" ? e.v : e.k === "sc" ? wrap(I32, values.get(e.n)) : wrap(I32, e.k === "add" ? e32(e.a) + e32(e.b) : e32(e.a) * e32(e.b)));
  const cond = (c) => { const a = values.get(c.n); return { "<": a < c.v, ">": a > c.v, "==": a === c.v, "<=": a <= c.v, ">=": a >= c.v, "!=": a !== c.v }[c.op]; };
  const sum = (list) => list.reduce((a, b) => a + b, 0n);
  const set = (n, kind, value) => owned.set(n, { kind, value });
  for (const s of program.steps) {
    switch (s.op) {
      case "newvec": set(s.n, "vec", s.init === "array" ? [...s.items] : []); break;
      case "newstr": set(s.n, "str", s.text); break;
      case "newq": set(s.n, "q", []); break;
      case "push": get(s.n, "vec").push(e32(s.e)); break;
      case "grow": { const v = get(s.n, "vec"); const x = e32(s.e); v.push(x, wrap(I32, x + 1n)); break; }
      case "pop": if (mutant !== "owned-pop-ignored") get(s.n, "vec").pop(); break;
      case "loop": for (let i = 0; i <= s.count; i++) get(s.n, "vec").push(BigInt(i)); break;
      case "whilepop": { const v = get(s.n, "vec"); while (v.length > 1) v.pop(); break; }
      case "len": values.set(s.out, BigInt(get(s.n, "vec").length)); break;
      case "sum": values.set(s.out, sum(get(s.n, "vec"))); break;
      case "clone": set(s.n, "vec", mutant === "owned-clone-aliased" ? get(s.from, "vec") : [...get(s.from, "vec")]); break;
      case "drop": values.set(s.out, BigInt(get(s.n, "vec").length)); owned.delete(s.n); break;
      case "arr": values.set(s.out, wrap(I32, sum(get(s.n, "vec")))); owned.delete(s.n); break;
      case "branch": { const v = get(s.n, "vec"); if (cond(s.cond)) { const x = e32(s.e); v.push(x, wrap(I32, x + 1n)); } else v.pop(); break; }
      case "cdrop": values.set(s.out, cond(s.cond) ? BigInt(get(s.n, "vec").length) : s.other); owned.delete(s.n); break;
      case "mdrop": values.set(s.out, values.get(s.on) === s.v ? BigInt(get(s.n, "vec").length) : s.other); owned.delete(s.n); break;
      case "append": set(s.n, "str", get(s.n, "str") + s.text); break;
      case "slen": values.set(s.out, BigInt(get(s.n, "str").length)); break;
      case "take": values.set(s.out, BigInt(get(s.n, "str").length)); owned.delete(s.n); break;
      case "cat": { const a = get(s.a, "str"); const b = get(s.b, "str"); owned.delete(s.a); owned.delete(s.b); set(s.n, "str", a + b); break; }
      case "qpush": get(s.n, "q").push(s.text); break;
      case "qpushs": { const t = get(s.s, "str"); owned.delete(s.s); get(s.n, "q").push(t); break; }
      case "qpop": get(s.n, "q").pop(); break;
      case "qclear": owned.set(s.n, { kind: "q", value: [] }); break;
      case "qlen": values.set(s.out, BigInt(get(s.n, "q").length)); break;
      default: throw new Error(s.op);
    }
  }
  let acc = 0n;
  const { live, scalars } = replay(program);
  for (const name of scalars) acc = wrap(I64, acc * 31n + values.get(name));
  for (const [n, kind] of live) acc = wrap(I64, acc * 31n + BigInt(get(n, kind).length));
  return acc;
}

// >>> phase 3

// ---- running the compiler ----

function exec(command, args, { cwd, timeout = 120000, env } = {}) {
  return new Promise((done) => {
    const child = spawn(command, args, { cwd, env: { ...process.env, ...env }, stdio: ["ignore", "pipe", "pipe"] });
    let stdout = "";
    let stderr = "";
    let timedOut = false;
    const timer = setTimeout(() => { timedOut = true; child.kill("SIGKILL"); }, timeout);
    child.stdout.on("data", (chunk) => { stdout += chunk; });
    child.stderr.on("data", (chunk) => { stderr += chunk; });
    child.on("error", (error) => { clearTimeout(timer); done({ status: -1, signal: null, stdout, stderr: String(error), timedOut }); });
    child.on("close", (status, signal) => { clearTimeout(timer); done({ status, signal, stdout: stdout.replaceAll("\r\n", "\n"), stderr, timedOut }); });
  });
}

// <<< phase 2
const wasmCall = `const { readFileSync } = require("node:fs");
const [file, ...inputs] = process.argv.slice(1);
const exports = new WebAssembly.Instance(new WebAssembly.Module(readFileSync(file))).exports;
for (const input of inputs) {
  try { console.log(String(exports.tz_run(BigInt(input)))); } catch (error) { console.log("trap " + error.constructor.name); }
}`;
// >>> phase 2

// <<< phase 3
// For ownership programs: the module is built with --allocator counting, and after every call the number of live
// bytes must be what it was before it (the 32 bytes of the scratch block for the statistics).
const wasmOwnedCall = `const { readFileSync } = require("node:fs");
const [file, ...inputs] = process.argv.slice(1);
const api = new WebAssembly.Instance(new WebAssembly.Module(readFileSync(file))).exports;
const scratch = api.tsuzuri_alloc(32n);
const live = () => { api.tsuzuri_alloc_stats(scratch); return new BigUint64Array(api.memory.buffer, scratch, 4)[2]; };
const base = live();
for (const input of inputs) {
  try {
    const value = api.tz_run(BigInt(input));
    if (process.env.TSUZURI_PROGFUZZ_INJECT_LEAK) api.tsuzuri_alloc(16n);
    console.log(String(value) + " leaked " + String(live() - base));
  } catch (error) { console.log("trap " + error.constructor.name); }
}`;
// >>> phase 3

// What one path produced for the program: the printed values, or why there are none.
async function runPath(path, directory, program) {
  const env = { TMPDIR: directory, TMP: directory, TEMP: directory };
  const common = ["--no-cache"];
  const fail = (kind, detail) => ({ ok: false, kind, detail });
  // <<< phase 2
  if (path === "wasm0" || path === "wasm3") {
    const flags = [];
    let script = wasmCall;
    // <<< phase 3
    if (program.kind === "owned") { flags.push("--allocator", "counting"); script = wasmOwnedCall; }
    // >>> phase 3
    const wasm = join(directory, "lib", `lib-${path}.wasm`);
    const build = await exec(compiler, ["build", join(directory, "lib"), "--target", "wasm32", `-O${path === "wasm0" ? 0 : 3}`, ...flags, ...common, "-o", wasm], { env });
    if (build.status !== 0) return fail("compile", build.stderr);
    const run = await exec(process.execPath, ["-e", script, wasm, ...program.inputs.map(String)], { timeout: 60000 });
    if (run.timedOut) return fail("timeout", "wasm run");
    if (run.status !== 0) return fail("run", run.stderr);
    return { ok: true, lines: run.stdout.trim().split("\n") };
  }
  // >>> phase 2
  const folder = path === "const" ? "const" : "exe";
  const level = path === "o3" ? "-O3" : "-O0";
  const exe = join(directory, folder, `${path}${exeSuffix}`);
  const flags = [];
  // <<< phase 3
  if (nativeLeaks && program.kind === "owned") flags.push("--allocator", "counting");
  // >>> phase 3
  const build = await exec(compiler, ["build", join(directory, folder), level, ...flags, ...common, "-o", exe], { env });
  // The constant evaluator has one budget for the whole program; running out of it says nothing about the value.
  if (path === "const" && /error\[E1026\].*(compiler limit|nested const def calls)/.test(build.stderr)) return { ok: false, kind: "limit", detail: build.stderr };
  if (build.status !== 0) return fail("compile", build.stderr);
  const run = await exec(exe, [], { timeout: 60000, env });
  if (run.timedOut) return fail("timeout", "native run");
  if (run.status !== 0 || run.signal) return fail("run", `status ${run.status} signal ${run.signal}\n${run.stderr}`);
  return { ok: true, lines: run.stdout.trim().split("\n") };
}

function compare(result, expected) {
  if (result.kind === "limit") return null;
  if (!result.ok) return { kind: result.kind, detail: result.detail };
  const actual = result.lines;
  if (actual.length !== expected.length) return { kind: "value", detail: `expected ${expected.length} lines, got ${actual.length}`, actual, expected };
  const at = actual.findIndex((line, j) => line !== String(expected[j]));
  return at < 0 ? null : { kind: "value", detail: `input ${at}`, at, actual: actual[at], expected: String(expected[at]) };
}

// <<< phase 2
// ---- minimizing a failing program ----

const SKIP = new Set(["ty", "st", "it", "result", "records", "unions", "inputs", "tags"]);

// Every node of a tree with its path; a path is the list of keys and indexes that leads to it.
function* positions(value, path = []) {
  if (Array.isArray(value)) {
    for (let j = 0; j < value.length; j++) yield* positions(value[j], [...path, j]);
  } else if (value && typeof value === "object") {
    if (typeof value.t === "string") yield [path, value];
    for (const [key, child] of Object.entries(value)) if (!SKIP.has(key) && child && typeof child === "object") yield* positions(child, [...path, key]);
  }
}
function directChildren(node) {
  const found = [];
  const visit = (value) => {
    if (Array.isArray(value)) value.forEach(visit);
    else if (value && typeof value === "object") {
      if (typeof value.t === "string") found.push(value);
      else for (const [key, child] of Object.entries(value)) if (!SKIP.has(key)) visit(child);
    }
  };
  for (const [key, child] of Object.entries(node)) if (!SKIP.has(key)) visit(child);
  return found;
}
function replaceAt(root, path, value) {
  if (!path.length) return value;
  const [key, ...rest] = path;
  const copy = Array.isArray(root) ? [...root] : { ...root };
  copy[key] = replaceAt(root[key], rest, value);
  return copy;
}
function mapNodes(value, change) {
  if (Array.isArray(value)) return value.map((item) => mapNodes(item, change));
  if (value && typeof value === "object") {
    if (typeof value.t === "string") {
      const replaced = change(value);
      if (replaced) return replaced;
    }
    return Object.fromEntries(Object.entries(value).map(([key, child]) => [key, SKIP.has(key) ? child : mapNodes(child, change)]));
  }
  return value;
}
const substitute = (node, name, replacement) => mapNodes(node, (n) => (n.t === "var" && n.n === name ? replacement : null));
const abs = (v) => (v < 0n ? -v : v);

// Fewer nodes first, then smaller literals: every accepted step lowers this pair, so the loop ends.
function weightOf(program) {
  // <<< phase 3
  if (program.kind === "owned") return [program.steps.length, 0n];
  // >>> phase 3
  let nodes = 0;
  let magnitude = 0n;
  for (const f of program.functions) for (const [, n] of positions(f.body)) { nodes++; if (n.t === "lit" && n.ty !== BOOL) magnitude += abs(n.v); }
  return [nodes, magnitude];
}
const lighter = (a, b) => a[0] < b[0] || (a[0] === b[0] && a[1] < b[1]);

function prune(program) {
  // <<< phase 3
  if (program.kind === "owned") return program;
  // >>> phase 3
  const byName = new Map(program.functions.map((f) => [f.name, f]));
  const used = new Set(["run"]);
  const queue = ["run"];
  while (queue.length) {
    for (const [, n] of positions(byName.get(queue.pop()).body)) if (n.t === "call" && !used.has(n.f)) { used.add(n.f); queue.push(n.f); }
  }
  return { ...program, functions: program.functions.filter((f) => used.has(f.name)) };
}

function valid(program) {
  try {
    for (const k of program.inputs) evaluate(program, k);
    return true;
  } catch (error) {
    if (error instanceof Trap || error instanceof Limit) return false;
    throw error;
  }
}

// Smaller versions of the program, most promising first: the entry function before the helpers, a node before its children.
function* candidates(program) {
  // <<< phase 3
  if (program.kind === "owned") {
    for (let j = program.steps.length - 1; j >= 0; j--) yield { ...program, steps: program.steps.filter((_, k) => k !== j) };
    return;
  }
  // >>> phase 3
  for (let fi = program.functions.length - 1; fi >= 0; fi--) {
    for (const [relative, node] of positions(program.functions[fi].body)) {
      const path = ["functions", fi, "body", ...relative];
      const put = (replacement) => replaceAt(program, path, replacement);
      const ty = node.ty;
      if (ty.k === "int" && !(node.t === "lit" && node.v === 0n)) yield put(lit(ty, 0n));
      if (ty === BOOL && node.t !== "lit") { yield put(lit(BOOL, false)); yield put(lit(BOOL, true)); }
      for (const child of directChildren(node)) if (keyOf(child.ty) === keyOf(ty)) yield put(child);
      if (node.t === "let") yield put(substitute(node.body, node.n, node.e));
      if (node.t === "matchint") {
        for (let j = 0; j < node.arms.length; j++) yield put({ ...node, arms: node.arms.filter((_, k) => k !== j) });
        if (node.guard) yield put({ ...node, guard: null });
      }
      if ((node.t === "for" || node.t === "while") && node.exit) yield put({ ...node, exit: null });
      if (node.t === "while" && node.extra) yield put({ ...node, extra: null });
      if (node.t === "for" && node.step) yield put({ ...node, step: null });
      if (node.t === "lit" && ty.k === "int") for (const v of [node.v / 2n, node.v < 0n ? -1n : 1n]) if (abs(v) < abs(node.v)) yield put(lit(ty, v));
    }
  }
}

// `stillFails(candidate)` decides whether a candidate shows the same difference; at most `budget` calls.
async function minimize(program, stillFails, { budget = 150, seconds = 600 } = {}) {
  const deadline = Date.now() + seconds * 1000;
  let current = prune(program);
  let attempts = 0;
  for (let progress = true; progress && attempts < budget && Date.now() < deadline;) {
    progress = false;
    for (const next of candidates(current)) {
      const smaller = prune(next);
      if (!lighter(weightOf(smaller), weightOf(current)) || !valid(smaller)) continue;
      if (attempts >= budget || Date.now() >= deadline) break;
      attempts++;
      if (await stillFails(smaller)) { current = smaller; progress = true; break; }
    }
  }
  return { program: current, attempts, nodes: weightOf(current)[0] };
}

const errorCode = (text) => /error\[(E\d+)\]/.exec(text ?? "")?.[1] ?? "";
// >>> phase 2

function writeSources(program, directory) {
  const tails = [["exe", "run"], ["const", "const"], ["lib", "lib"]];
  // <<< phase 3
  if (program.kind === "owned") tails.splice(1, 1);
  // >>> phase 3
  for (const [folder, tail] of tails) {
    mkdirSync(join(directory, folder), { recursive: true });
    writeFileSync(join(directory, folder, "Main.tz"), source(program, tail));
  }
}

// <<< phase 2
const tailOf = (path) => (path === "const" ? "const" : path.startsWith("wasm") ? "lib" : "run");

// Shrinks the program while the same path keeps failing in the same way, and saves it under failures/.
async function minimizeFailure(program, failure) {
  const reduced = failure.at === undefined ? program : { ...program, inputs: [program.inputs[failure.at]] };
  const same = (difference) => difference && difference.kind === failure.kind && (failure.kind !== "compile" || errorCode(difference.detail) === errorCode(failure.detail));
  const work = join(outputRoot, `minimize-${program.seed}-${failure.path}`);
  const stillFails = async (candidate) => {
    writeSources(candidate, work);
    return same(compare(await runPath(failure.path, work, candidate), wantedLines(candidate, failure.path, candidate.inputs.map((k) => evaluate(candidate, k)))));
  };
  const before = weightOf(reduced)[0];
  const result = await minimize(reduced, stillFails, { budget: minimizeBudget });
  rmSync(work, { recursive: true, force: true });
  const saved = join(outputRoot, "failures", `seed-${program.seed}-${failure.path}`);
  rmSync(saved, { recursive: true, force: true });
  mkdirSync(saved, { recursive: true });
  const min = result.program;
  writeFileSync(join(saved, "Main.tz"), source(min, tailOf(failure.path)));
  writeFileSync(join(saved, "original.txt"), source(program, tailOf(failure.path)));
  const expected = min.inputs.map((k) => evaluate(min, k));
  writeFileSync(join(saved, "info.txt"), [
    `seed ${program.seed}  depth ${program.depth}  path ${failure.path}  kind ${failure.kind}`,
    `inputs ${min.inputs.join(" ")}`,
    `expected ${expected.join(" ")}`,
    `actual ${failure.actual ?? "-"} (first difference in the original), ${failure.detail.split("\n")[0]}`,
    `nodes ${before} -> ${result.nodes} after ${result.attempts} compiler runs`,
    `reproduce: node tests/progfuzz.mjs ${compiler} 1 ${program.seed} ${program.depth}`,
    "",
  ].join("\n"));
  return { directory: saved, nodes: result.nodes, before, attempts: result.attempts };
}
// >>> phase 2

// ---- the semantics table ----
// One line per case: kind, type(s), operator, operands, result. Every result was worked out by hand from
// docs/language.md and the language reference (wrapping + - *, truncating / and %, shift counts taken
// modulo the bit width, arithmetic >>> for signed types), and the self test runs each line on the compiler too.
const TABLE = `
bin i8 + 127 1 -128
bin i8u + 255 1 0
bin i16 + 32767 1 -32768
bin i32u + 4294967295 1 0
bin i64 + 9223372036854775807 1 -9223372036854775808
bin i64u + 18446744073709551615 1 0
bin i8 - -128 1 127
bin i8u - 0 1 255
bin i64 - -9223372036854775808 1 9223372036854775807
bin i64u - 0 1 18446744073709551615
bin i8 * 100 3 44
bin i8 * -128 -1 -128
bin i8u * 16 16 0
bin i16 * 300 300 24464
bin i32 * 65536 65536 0
bin i32 * 46341 46341 -2147479015
bin i64 * -9223372036854775808 -1 -9223372036854775808
bin i64 * 3037000500 3037000500 -9223372036709301616
bin i64u * 4294967296 4294967296 0
div i32 / -7 2 -3
div i32 / 7 -2 -3
div i32 / -7 -2 3
div i32 % -7 2 -1
div i32 % 7 -2 1
div i32 % -7 -2 -1
div i8 / -128 2 -64
div i8 % -128 3 -2
div i8 / 127 -1 -127
div i8u / 255 2 127
div i8u % 255 7 3
div i16 / -32768 -3 10922
div i64 / -9223372036854775808 2 -4611686018427387904
div i64 % -9223372036854775808 10 -8
div i64u / 18446744073709551615 10 1844674407370955161
div i64u % 18446744073709551615 10 5
bin i8 & -1 15 15
bin i8 | -128 1 -127
bin i8 ^ 5 -1 -6
bin i8u & 200 15 8
bin i8u | 128 1 129
bin i8u ^ 200 255 55
bin i64 & -1 9223372036854775807 9223372036854775807
bin i64u ^ 18446744073709551615 1 18446744073709551614
shift i8 <<< 1 7 -128
shift i8 <<< 1 8 1
shift i8 <<< 1 9 2
shift i8 <<< 1 -1 -128
shift i8u <<< 255 1 254
shift i8u <<< 1 255 128
shift i16 <<< 1 15 -32768
shift i16 <<< 1 16 1
shift i16 <<< 3 -1 -32768
shift i32 <<< 1 31 -2147483648
shift i32 <<< 1 32 1
shift i32 <<< 1 33 2
shift i32 <<< 5 -31 10
shift i32u <<< 1 31 2147483648
shift i64 <<< 1 63 -9223372036854775808
shift i64 <<< 1 64 1
shift i64 <<< 1 65 2
shift i64 <<< 1 -1 -9223372036854775808
shift i64u <<< 1 63 9223372036854775808
shift i8 >>> -128 1 -64
shift i8 >>> -1 7 -1
shift i8 >>> -1 8 -1
shift i8 >>> 64 -1 0
shift i8u >>> 128 7 1
shift i8u >>> 255 8 255
shift i8u >>> 255 9 127
shift i32 >>> -8 1 -4
shift i32 >>> -2147483648 31 -1
shift i32 >>> -2147483648 32 -2147483648
shift i32u >>> 4294967295 31 1
shift i32u >>> 4294967295 33 2147483647
shift i64 >>> -9223372036854775808 63 -1
shift i64 >>> -1 64 -1
shift i64u >>> 18446744073709551615 63 1
un i8 neg -128 -128
un i32 neg 5 -5
un i64 neg -9223372036854775808 -9223372036854775808
un i8 not 0 -1
un i8u not 0 255
un i64 not -9223372036854775808 9223372036854775807
un i32u not 1 4294967294
cast i32 i8 300 44
cast i8 i8u -1 255
cast i8u i8 255 -1
cast i64 i32 -9223372036854775808 0
cast i8 i64u -1 18446744073709551615
cast i8u i64 255 255
cast i16 i8 -32768 0
cast i64u i64 18446744073709551615 -1
cast i32 i64u -1 18446744073709551615
cast i32u i64 4294967295 4294967295
cast i64 i8u -1 255
cast i16u i16 40000 -25536
cast i64 i16 65537 1
cast i64 i32u -1 4294967295
cmp i8u < 200 100 0
cmp i8u > 200 100 1
cmp i8 < -1 0 1
cmp i64u > 18446744073709551615 0 1
cmp i8u >= 255 255 1
cmp i64 < -9223372036854775808 9223372036854775807 1
cmp i32u < 4294967295 0 0
cmp i16 != -32768 32767 1
cmp i8 == -128 -128 1
for i8 125 2 127 252
for i8u 250 - 255 1515
for i32 5 - 0 0
for i32 5 -2 0 9
for i8 -128 - -126 -381
for i8 127 -1 125 378
for i64 9223372036854775805 - 9223372036854775807 9223372036854775802
for i64u 18446744073709551613 - 18446744073709551615 -6
for i16 -32768 3 -32760 -98295
`;

// The table as a program: one function per line, and `run k` returns line k as an i64.
function tableProgram() {
  const functions = [];
  const arms = [];
  const expected = [];
  const names = [];
  const ty = (name) => INTS.find((t) => t.name === name);
  const v = (n, t) => ({ t: "var", ty: t, n });
  TABLE.trim().split("\n").forEach((line, index) => {
    const w = line.split(" ");
    const name = `v${index}`;
    let params;
    let result;
    let body;
    let args;
    let want;
    if (["bin", "div", "shift", "cmp"].includes(w[0])) {
      const t = ty(w[1]);
      params = [{ n: "a", ty: t }, { n: "b", ty: t }];
      args = [lit(t, BigInt(w[3])), lit(t, BigInt(w[4]))];
      want = BigInt(w[5]);
      if (w[0] === "cmp") { result = BOOL; body = { t: "cmp", ty: BOOL, op: w[2], a: v("a", t), b: v("b", t) }; }
      else { result = t; body = w[0] === "shift" ? { t: "shift", ty: t, op: w[2], a: v("a", t), c: v("b", t) } : { t: w[0], ty: t, op: w[2], a: v("a", t), b: v("b", t) }; }
    } else if (w[0] === "un") {
      const t = ty(w[1]);
      params = [{ n: "a", ty: t }];
      args = [lit(t, BigInt(w[3]))];
      want = BigInt(w[4]);
      result = t;
      body = { t: w[2], ty: t, a: v("a", t) };
    } else if (w[0] === "cast") {
      const from = ty(w[1]);
      const to = ty(w[2]);
      params = [{ n: "a", ty: from }];
      args = [lit(from, BigInt(w[3]))];
      want = BigInt(w[4]);
      result = to;
      body = { t: "cast", ty: to, a: v("a", from) };
    } else {
      const it = ty(w[1]);
      params = [{ n: "p", ty: I64 }];
      args = [lit(I64, 0n)];
      want = BigInt(w[5]);
      result = I64;
      const acc = v("acc", I64);
      body = {
        t: "for", ty: I64, acc: "acc", i: "i", it, init: lit(I64, 0n), lo: lit(it, BigInt(w[2])), step: w[3] === "-" ? null : lit(it, BigInt(w[3])), hi: lit(it, BigInt(w[4])), exit: null,
        body: { t: "bin", ty: I64, op: "+", a: acc, b: { t: "cast", ty: I64, a: v("i", it) } },
      };
    }
    functions.push({ name, params, result, rec: false, body });
    const call = { t: "call", ty: result, f: name, args };
    arms.push({ v: BigInt(index), body: result === BOOL ? { t: "if", ty: I64, c: call, a: lit(I64, 1n), b: lit(I64, 0n) } : result === I64 ? call : { t: "cast", ty: I64, a: call } });
    expected.push(wrap(I64, want));
    names.push(line);
  });
  const k = { n: "k", ty: I64 };
  functions.push({ name: "run", params: [k], result: I64, rec: false, body: { t: "matchint", ty: I64, s: variable(k), st: I64, arms, guard: null, dflt: lit(I64, -1n) } });
  return { program: { seed: 0, depth: 0, records: [], unions: [], functions, inputs: expected.map((_, j) => BigInt(j)), tags: new Set() }, expected, names };
}

// ---- main ----

const coverage = Object.create(null);

async function checkProgram(seed) {
  const program = generator(seed, depth);
  let expected;
  try {
    expected = program.inputs.map((k) => evaluate(program, k));
  } catch (error) {
    if (error instanceof Limit) return { seed, discarded: true };
    if (error instanceof Trap) throw new Error(`seed ${seed}: the generator made a program that traps (${error.message})`);
    throw error;
  }
  for (const tag of program.tags) coverage[tag] = (coverage[tag] ?? 0) + 1;
  const directory = join(outputRoot, `seed-${seed}`);
  rmSync(directory, { recursive: true, force: true });
  writeSources(program, directory);
  const failures = [];
  let limited = 0;
  for (const path of paths) {
    const result = await runPath(path, directory, program);
    if (result.kind === "limit") limited++;
    const difference = compare(result, wantedLines(program, path, expected));
    if (difference) failures.push({ seed, path, ...difference });
  }
  if (process.env.TSUZURI_PROGFUZZ_KEEP !== "1") rmSync(directory, { recursive: true, force: true });
  // <<< phase 2
  if (failures.length && process.env.TSUZURI_PROGFUZZ_MINIMIZE !== "0") failures[0].saved = await minimizeFailure(program, failures[0]);
  // >>> phase 2
  return { seed, failures, limited, executions: paths.length * program.inputs.length };
}

// <<< phase 3
// Whether `tsuzuri run --allocator counting` can read the allocation counts from a program (F14's Alloc module).
async function supportsAlloc() {
  const folder = join(outputRoot, "alloc-probe");
  mkdirSync(folder, { recursive: true });
  writeFileSync(join(folder, "Main.tz"), "def main :: unit -> i32 = \\() ->\n    let before = Alloc.snapshot ()\n    let spent = Alloc.since before\n    do! IO.write_line (to_string (spent.allocs - spent.frees))\n    0\n");
  const result = await exec(compiler, ["run", folder, "--allocator", "counting", "--no-cache"], { env: { TMPDIR: folder } });
  rmSync(folder, { recursive: true, force: true });
  return result.status === 0 && result.stdout.trim() === "0";
}
// >>> phase 3

async function fuzz() {
  mkdirSync(outputRoot, { recursive: true });
  // <<< phase 3
  if (kind === "owned") {
    nativeLeaks = await supportsAlloc();
    if (!nativeLeaks) console.log("progfuzz: the compiler has no Alloc module, so only the wasm paths check for leaks");
  }
  // >>> phase 3
  const queue = Array.from({ length: programs }, (_, j) => firstSeed + j);
  const results = [];
  await Promise.all(Array.from({ length: jobs }, async () => {
    while (queue.length) results.push(await checkProgram(queue.shift()));
  }));
  return results.sort((a, b) => a.seed - b.seed);
}

const REQUIRED_TAGS = {
  values: ["array_index", "array_length", "call", "cast_narrow", "cast_sign", "cast_widen", "div_dynamic_signed", "div_dynamic_unsigned", "div_literal_signed", "div_literal_unsigned",
    "for_loop", "while_loop", "loop_break", "loop_continue", "loop_descending", "loop_empty", "loop_to_type_edge", "loop_from_type_edge", "loop_dynamic_bound",
    "match_guard", "match_int", "match_maybe", "match_tuple", "match_union", "new_array", "record_update", "recursive_function", "self_call",
    "shift_count_dynamic", "shift_count_negative", "shift_count_over_width", "shift_right_signed"],
};
// <<< phase 3
REQUIRED_TAGS.owned = ["newvec", "newstr", "newq", "push", "grow", "pop", "loop", "whilepop", "len", "sum", "clone", "drop", "arr", "branch", "cdrop", "mdrop", "append", "slen", "take", "cat", "qpush", "qpushs", "qpop", "qclear", "qlen"];
// >>> phase 3

function report(results) {
  const failed = results.filter((r) => r.failures?.length);
  const discarded = results.filter((r) => r.discarded).length;
  const limited = results.reduce((sum, r) => sum + (r.limited ?? 0), 0);
  const executions = results.reduce((sum, r) => sum + (r.executions ?? 0), 0);
  if (failed.length) {
    for (const result of failed) for (const f of result.failures) {
      console.error(`seed ${f.seed} ${f.path}: ${f.kind} ${f.detail.split("\n")[0]}${f.actual !== undefined ? ` (actual ${f.actual}, expected ${f.expected})` : ""}`);
      if (f.saved) console.error(`  minimized from ${f.saved.before} to ${f.saved.nodes} nodes in ${f.saved.attempts} compiler runs: ${f.saved.directory}`);
    }
    console.error(`${failed.length} of ${results.length - discarded} programs differ`);
    process.exitCode = 1;
    return;
  }
  // A generator that stopped producing a construct, or that is thrown away by the size limit, tests nothing.
  assert.ok(discarded * 4 <= results.length, `${discarded} of ${results.length} programs were discarded for their size`);
  if (results.length >= 100) for (const tag of REQUIRED_TAGS[kind]) assert.ok(coverage[tag] > 0, `the generator never produced "${tag}" in ${results.length} programs`);
  console.log(`progfuzz ${kind}: ${results.length - discarded} programs (${discarded} discarded for their size, ${limited} constant evaluations over budget), ${executions} executions on ${paths.join(", ")}, seed ${firstSeed}, depth ${depth}; zero differences`);
  console.log(JSON.stringify(Object.fromEntries(Object.entries(coverage).sort())));
}

const MUTANTS = ["shift-saturating", "mul-unwrapped", "div-floor", "shr-logical", "cast-saturate", "loop-exclusive"];
// <<< phase 3
const OWNED_MUTANTS = ["owned-pop-ignored", "owned-clone-aliased"];
// >>> phase 3

// Known compiler bugs the generator works around. Each probe must still fail; when one is fixed this test
// says so, and the workaround and the entry are to be deleted.
const KNOWN_BUGS = [
  {
    id: "KB1",
    source: "def f :: [i64] -> i64\nfn f xs = xs[({ let mut a: i64 = 0i64; for i in 0i64 .. 1i64 do { a = (a + i); () }; a })]\n\ndef main :: unit -> i32 = \\() ->\n    do! IO.write_line (to_string (f [10i64, 20i64, 30i64]))\n    0\n",
    error: "error[E0002]: expected 'do' after the enumerable expression",
  },
];

// Three seeds of the random numbers, worked out with a second implementation (Python integers masked to 64 bits).
const KNOWN_RANDOM = [
  [1, [0x4b46a55df3611b9bn, 0xd7e1f1410e763ef4n, 0x5f14ec66975f9b06n]],
  [2, [0x8a6f1a28befa4ca9n, 0x159e8dc3455a5efen, 0xe617cb304c58a414n]],
  [123456789, [0xc924fb4cdcf409c7n, 0x28e47c224000b4fan, 0x114b0233ea2aea7fn]],
];

async function selfTesting() {
  mkdirSync(outputRoot, { recursive: true });
  const savedPaths = paths;
  // 0. The random numbers.
  for (const [seed, words] of KNOWN_RANDOM) {
    const random = xorshift(seed);
    for (const word of words) assert.equal(random.next(), word, `xorshift64* of seed ${seed}`);
  }
  console.log("self test: xorshift64* gives the known words for 3 seeds");
  // 1. The evaluator against the semantics table.
  const table = tableProgram();
  table.program.inputs.forEach((k, j) => assert.equal(evaluate(table.program, k), table.expected[j], `evaluator, table line "${table.names[j]}"`));
  console.log(`self test: the evaluator matches all ${table.expected.length} lines of the semantics table`);
  // 2. A seed is a program: the same seed twice gives the same text, different seeds give different texts.
  const texts = new Set();
  for (let seed = 1; seed <= 40; seed++) {
    const text = source(generate(seed, 4), "run");
    assert.equal(text, source(generate(seed, 4), "run"), `seed ${seed}`);
    texts.add(text);
  }
  assert.equal(texts.size, 40);
  console.log("self test: 40 seeds give 40 distinct programs, each the same on a second generation");
  // 3. Each deliberately wrong evaluator disagrees with the right one on some generated program.
  const answers = (program, wrongly) => {
    mutant = wrongly;
    try { return program.inputs.map((k) => evaluate(program, k)); } catch (error) { if (error instanceof Limit) return null; throw error; } finally { mutant = mutantName; }
  };
  const mutantSets = [[generate, MUTANTS]];
  // <<< phase 3
  mutantSets.push([generateOwned, OWNED_MUTANTS]);
  // >>> phase 3
  for (const [make, names] of mutantSets) {
    for (const name of names) {
      let changed = 0;
      for (let seed = 1; seed <= 200; seed++) {
        const program = make(seed, 4);
        const right = answers(program, "");
        const wrong = answers(program, name);
        if (right && wrong && right.some((v, j) => v !== wrong[j])) changed++;
      }
      assert.ok(changed > 0, `the mutant ${name} changed no program of 200`);
      console.log(`self test: mutant ${name} changes the answer of ${changed} of 200 programs`);
    }
  }
  // <<< phase 2
  // 4. The minimizer on a made-up property that needs no compiler: a shift by at least the bit width.
  const wide = (program) => program.functions.some((f) => [...positions(f.body)].some(([, n]) => n.t === "shift" && n.c.t === "lit" && n.c.v >= BigInt(n.ty.bits)));
  let big = null;
  for (let seed = 1; seed < 300 && !big; seed++) { const program = generate(seed, 5); if (wide(program) && valid(program)) big = program; }
  assert.ok(big, "no program with a wide shift in 300 seeds");
  const small = await minimize(big, async (program) => wide(program), { budget: 5000, seconds: 120 });
  assert.ok(wide(small.program) && small.nodes <= 30 && small.nodes * 10 <= weightOf(big)[0], `the minimizer left ${small.nodes} of ${weightOf(big)[0]} nodes\n${source(small.program, "lib")}`);
  console.log(`self test: the minimizer cut ${weightOf(big)[0]} nodes to ${small.nodes} in ${small.attempts} tries`);
  // >>> phase 2
  // 5. The compiler against the semantics table, on every configured path.
  const directory = join(outputRoot, "table");
  writeSources(table.program, directory);
  for (const path of paths) {
    const difference = compare(await runPath(path, directory, table.program), table.expected);
    assert.equal(difference, null, `the table on ${path}: ${JSON.stringify(difference)}`);
  }
  console.log(`self test: the compiler matches the semantics table on ${paths.join(", ")}`);
  // <<< phase 2
  // 6. The whole machine on a wrong evaluator: it must find, minimize and save a difference.
  const savedBudget = minimizeBudget;
  paths = ["o0"];
  minimizeBudget = 60;
  try {
    for (const name of ["div-floor", "shift-saturating"]) {
      mutant = name;
      let found = null;
      for (let seed = 1; seed <= 60 && !found; seed++) {
        const result = await checkProgram(seed);
        if (result.failures?.length) found = result.failures[0];
      }
      assert.ok(found?.kind === "value" && found.saved, `the mutant ${name} was not caught`);
      assert.ok(found.saved.nodes <= 60, `the minimized program has ${found.saved.nodes} nodes`);
      assert.ok(readFileSync(join(found.saved.directory, "Main.tz"), "utf8").includes("def main"));
      // `tsuzuri run <directory>` reads every .tz file of the directory, so the copy of the original program is a .txt.
      assert.deepEqual(readdirSync(found.saved.directory).filter((name) => name.endsWith(".tz")), ["Main.tz"]);
      console.log(`self test: mutant ${name} caught by seed ${found.seed}, minimized to ${found.saved.nodes} nodes`);
    }
  } finally {
    mutant = mutantName;
    paths = savedPaths;
    minimizeBudget = savedBudget;
  }
  // >>> phase 2
  // 7. The constants really are computed by the compiler: `main` of the const text calls no user function,
  //    while `main` of the run text does.
  const probe = generate(7, 3);
  writeSources(probe, join(outputRoot, "probe"));
  const mainBody = async (folder) => {
    const ll = join(outputRoot, "probe", `${folder}.ll`);
    const built = await exec(compiler, ["build", join(outputRoot, "probe", folder), "--emit", "llvm", "-O0", "--no-cache", "-o", ll], { env: { TMPDIR: outputRoot } });
    assert.equal(built.status, 0, built.stderr);
    const text = readFileSync(ll, "utf8");
    const start = text.indexOf("define internal i32 @tz.fn.Main.main(");
    assert.ok(start >= 0, "no @tz.fn.Main.main in the IR");
    return text.slice(start, text.indexOf("\n}", start));
  };
  assert.ok(!/call [^\n]*@tz\.fn\.Main\./.test(await mainBody("const")), "the const text calls a user function from main");
  assert.ok(/call [^\n]*@tz\.fn\.Main\.run/.test(await mainBody("exe")), "the run text must call run from main");
  console.log("self test: main of the const text calls no user function; main of the run text calls run");
  // 8. The known compiler bugs still reproduce.
  for (const bug of KNOWN_BUGS) {
    const folder = join(outputRoot, bug.id);
    mkdirSync(folder, { recursive: true });
    writeFileSync(join(folder, "Main.tz"), bug.source);
    const result = await exec(compiler, ["build", folder, "-O0", "--no-cache", "-o", join(folder, `x${exeSuffix}`)], { env: { TMPDIR: folder } });
    assert.ok(result.status !== 0 && result.stderr.includes(bug.error), `${bug.id} no longer reproduces: it is fixed, so delete its workaround and its entry\n${result.stderr}`);
    console.log(`self test: known bug ${bug.id} still reproduces`);
  }
  // <<< phase 3
  // 9. Ownership programs: a seed is a program, a wrong evaluator is caught, and the leak measurement sees a leak.
  generator = generateOwned;
  nativeLeaks = await supportsAlloc();
  try {
    const ownedTexts = new Set();
    for (let seed = 1; seed <= 40; seed++) {
      const text = source(generateOwned(seed, 4), "lib");
      assert.equal(text, source(generateOwned(seed, 4), "lib"), `owned seed ${seed}`);
      ownedTexts.add(text);
    }
    assert.equal(ownedTexts.size, 40);
    paths = ["o0"];
    minimizeBudget = 40;
    mutant = "owned-pop-ignored";
    let caught = null;
    for (let seed = 1; seed <= 60 && !caught; seed++) {
      const result = await checkProgram(seed);
      if (result.failures?.length) caught = result.failures[0];
    }
    assert.ok(caught?.kind === "value" && caught.saved && caught.saved.nodes <= 12, `the mutant owned-pop-ignored was not caught and minimized: ${JSON.stringify(caught?.saved)}`);
    console.log(`self test: mutant owned-pop-ignored caught by seed ${caught.seed}, minimized to ${caught.saved.nodes} steps`);
    mutant = mutantName;
    const probeOwned = generateOwned(1, 4);
    const folder = join(outputRoot, "leak");
    writeSources(probeOwned, folder);
    const expectedLines = probeOwned.inputs.map((k) => `${evaluate(probeOwned, k)} leaked`);
    for (const [inject, leaked] of [[undefined, () => "0"], ["1", (j) => String(16 * (j + 1))]]) {
      if (inject) process.env.TSUZURI_PROGFUZZ_INJECT_LEAK = inject; else delete process.env.TSUZURI_PROGFUZZ_INJECT_LEAK;
      const result = await runPath("wasm0", folder, probeOwned);
      assert.ok(result.ok && result.lines.every((line, j) => line === `${expectedLines[j]} ${leaked(j)}`), `leak measurement with injection ${inject}: ${JSON.stringify(result)}`);
    }
    console.log("self test: the leak measurement reads 0 for a clean run and counts every injected 16-byte block");
  } finally {
    mutant = mutantName;
    generator = generate;
    if (kind === "owned") generator = generateOwned;
    paths = savedPaths;
    minimizeBudget = savedBudget;
    nativeLeaks = false;
    delete process.env.TSUZURI_PROGFUZZ_INJECT_LEAK;
  }
  // >>> phase 3
  rmSync(outputRoot, { recursive: true, force: true });
  console.log("progfuzz self test: ok");
}

if (selfTest) await selfTesting();
else report(await fuzz());
````

## B. 実測

測った機械: Apple M1 Max（10 コア）、macOS（Darwin arm64）、Node v20.19.6、ビルド済みの `target/release/tsuzuri`（ファイルの日時は 10 月 17 日 22:00）、ジョブ数は既定の 4。すべて付録 A の全文（位相 1〜3）で取った。コマンドは、リポジトリの直下で `tests/progfuzz.mjs` として実行する形に直して書く。

| # | コマンド | 結果 |
| --- | --- | --- |
| B1 | `node tests/progfuzz.mjs target/release/tsuzuri 30 1 3` | 30 本・1,200 回（5 経路）・29 秒・差 0。1 本あたり、native `-O0` 約 0.5 秒、`-O3` 約 0.7 秒、wasm32 `-O0` 約 0.1 秒、`-O3` 約 0.35 秒（`--no-cache`） |
| B2 | `node tests/progfuzz.mjs target/release/tsuzuri 100 1 4` | 100 本・4,000 回・2 分 3 秒・差 0 |
| B3 | `node tests/progfuzz.mjs target/release/tsuzuri 300 1000 5`（索引の回避の前） | 18 本が全経路でコンパイルできない。90 行（18 本 × 5 経路）がすべて `error[E0002]: expected 'do' after the enumerable expression`。残り 282 本は差 0。原因は付録 D の KB1 |
| B4 | 同じ（索引の回避の後） | 300 本・12,000 回・差 0・約 7 分（B3 と同時に別の実行が走っていた） |
| B5 | `node tests/progfuzz.mjs target/release/tsuzuri 200 1 4` | 200 本・8,000 回・2 分 24 秒・差 0・打ち切り 0・定数評価の予算超過 0。CI の smoke の見積もり |
| B6 | `TSUZURI_PROGFUZZ_KIND=owned node tests/progfuzz.mjs target/release/tsuzuri 100 1 4` | 100 本・3,200 回（o0・o3・wasm0・wasm3）・1 分 8 秒・差 0。`Alloc` がないので native の漏れの検査は無効（「F14 がありません」の 1 行が出る） |
| B7 | `TSUZURI_PROGFUZZ_KIND=owned node tests/progfuzz.mjs target/release/tsuzuri 300 1 5` | 300 本・9,600 回・差 0 |
| B8 | B6 と同じで、F14 の試作のコンパイラ（`Alloc.snapshot`／`Alloc.since` がある）を指定、30 本・depth 4 | 960 回・差 0。native の漏れの検査（1 呼び出しごとの `allocs - frees` が 0）も有効だった |
| B9 | `node tests/progfuzz.mjs --self-test target/release/tsuzuri` | 全部が通る。81 秒（位相 1 だけの版は 3.7 秒、位相 1 と 2 の版は 72 秒） |
| B10 | B3 の seed 1034 を `TSUZURI_PROGFUZZ_PATHS=o0` で最小化 | 1,500 節点が 72 節点になった（150 回のコンパイラの実行、約 2 分）。残りの形は「配列の添字の中の範囲 `for`」 |

B9 の内訳（位相 1〜3 の全文）:

```text
self test: xorshift64* gives the known words for 3 seeds
self test: the evaluator matches all 116 lines of the semantics table
self test: 40 seeds give 40 distinct programs, each the same on a second generation
self test: mutant shift-saturating changes the answer of 122 of 200 programs
self test: mutant mul-unwrapped changes the answer of 65 of 200 programs
self test: mutant div-floor changes the answer of 67 of 200 programs
self test: mutant shr-logical changes the answer of 99 of 200 programs
self test: mutant cast-saturate changes the answer of 164 of 200 programs
self test: mutant loop-exclusive changes the answer of 37 of 200 programs
self test: mutant owned-pop-ignored changes the answer of 27 of 200 programs
self test: mutant owned-clone-aliased changes the answer of 16 of 200 programs
self test: the minimizer cut 750 nodes to 20 in 109 tries
self test: the compiler matches the semantics table on const, o0, o3, wasm0, wasm3
self test: mutant div-floor caught by seed 2, minimized to 3 nodes
self test: mutant shift-saturating caught by seed 1, minimized to 56 nodes
self test: main of the const text calls no user function; main of the run text calls run
self test: known bug KB1 still reproduces
self test: mutant owned-pop-ignored caught by seed 2, minimized to 2 steps
self test: the leak measurement reads 0 for a clean run and counts every injected 16-byte block
progfuzz self test: ok
```

既存の差分ファズ（HEAD）: `tests/last_use_fuzz.mjs`（200 行）・`tests/integer_ranges_fuzz.mjs`（491 行）・`tests/local_recursion_fuzz.mjs`（456 行）は、どれも `node tests/<名前>.mjs [コンパイラ] [件数] [最初の seed] [深さ]` の位置引数で、乱数は `mulberry32`（2 本）または線形合同（1 本）、失敗は例外で終了コード 1、成功は 1 行（`last_use_fuzz` は続けて JSON の被覆）を出す。`.github/workflows/targets.yml` の「Last-use differential fuzz smoke」（`40 1 4`）と「Integer range proofs …」（`20 1 3`）の段が手本。

## C. `const def` の受理の実測

`src/const_rules.rs` の `validate_function` が受理する範囲を、`tsuzuri check` で 43 件の本体に当てた結果（`const def probe` の本体だけを変えた。`check` は定数から呼ばれなくても本体を検査する）。生成器が使う構文は、すべて「受理」の側にある。

| 本体 | 結果 | 診断（先頭） |
| --- | --- | --- |
| 整数の `+ - * / % & \| ^ <<< >>>`・単項 `-`・`~` | 受理 | |
| 整数・整数の `as` | 受理 | |
| `&&`・`\|\|`・`!`・比較 | 受理 | |
| `if`・`let`・`let mut` と再代入 | 受理 | |
| 整数の `match`（ガードつき） | 受理 | |
| union・record・`Maybe`・タプルの `match` | 受理 | |
| 範囲の `for`（刻みつき） | 受理 | |
| 配列の `for` | 受理 | |
| `while`・`break`・`continue` | 受理 | |
| タプル・record の構築と `{ r with f = v }` | 受理 | |
| 配列リテラル・`new [T](n, \i -> …)`・`xs[i]`・`xs.length` | 受理 | |
| `const def` の呼び出し | 受理 | |
| 文字列の `.length`・`s[0]`・`==` | 受理 | |
| `f64` の算術・`f64` から整数への `as` | 受理 | |
| `**`・`char`・`unit`・`i128`・固定長配列 `[T; N]`・`Maybe.Some` | 受理 | |
| `x / 0`（定義の検査） | 受理 | 呼び出して評価したときに `E1026`（`const division trapped on zero or signed overflow`） |
| `const` でない `def` の呼び出し | 拒否 | `E1026` `'plain' is not a const def; mark it 'const def' or compute the value at runtime` |
| std の関数（`Array.get`） | 拒否 | `E1026` `'get' is not a const def; …` |
| `Vec.empty()` | 拒否 | `E1026` `'Vec.empty' is not a const def; …` |
| タプル・配列の `==` | 拒否 | `E1026` `an operator on a record, tuple, array, or union is not allowed in a const def; apply it to their fields` |
| 文字列補間 `$"{x}"` | 拒否 | `E1026` `this operation is not allowed in a const def; move it to a runtime def` |
| パイプ `\|>` | 拒否 | `E1026` `the pipe operator is not allowed in a const def; write the call` |
| `@checked` | 拒否 | `E1026` `an '@checked' operation raises an exception that has no compile-time form; …` |
| ラムダ（`new [T](n, f)` の外） | 拒否 | `E1026` `a function value is not allowed in a const def; only new [T](n, f) takes one` |
| `ref` の借用 | 拒否 | `E1026` `a borrow is not allowed in a const def; read the value directly` |
| decimal の算術 | 拒否 | `E1026` `this const arithmetic is unsupported; keep decimal arithmetic and overloaded operations in runtime code` |
| 型変数つきの署名 | 拒否 | `E1026` `a const def must have a concrete signature; remove type variables and constraints` |
| スライス `xs[0..1]` | 拒否 | `E0002` `array slices must be borrows; …`（構文の規則） |
| record の `==`（`Eq` の実装なし） | 拒否 | `E1005` `no instance for Eq<Main.R>`（言語の規則。`const` の規則ではない） |
| フィールド・要素への代入 | 拒否 | `E1012` `assignment replaces a mutable binding; …`（言語の規則） |
| 同じ文字列引数を 2 回使う `s + s` | 拒否 | `E1012` `use of moved or partially moved value 's'`（言語の規則） |

評価の上限（実測）: 呼び出しの深さ 128 を超えると `E1026` `const evaluation exceeds the compiler limit of 128 nested const def calls; reduce the recursion depth or compute the value at runtime`（`fib` 型の再帰 `f 200`）。300 万回の `for` は `E1026` `const evaluation exceeds the compiler limit; split or simplify the constants`。生成した 300 本（depth 5）で、予算の超過は 0 件だった。

## D. 見つけた既知のバグ KB1

`xs[...]` の角括弧の中に、範囲の `for` を含む式を書くと、`E0002` になる。最小の再現（HEAD の `target/release/tsuzuri build` で確かめた）:

```tsuzuri
def f :: [i64] -> i64
fn f xs = xs[({ let mut a: i64 = 0i64; for i in 0i64 .. 1i64 do { a = (a + i); () }; a })]

def main :: unit -> i32 = \() ->
    do! IO.write_line (to_string (f [10i64, 20i64, 30i64]))
    0
```

```text
Main.tz:2:54: error[E0002]: expected 'do' after the enumerable expression
  2 | ({ let mut a: i64 = 0i64; for i in 0i64 .. 1i64 do { a = (a + i); () }; a })]
    |                                         ^
```

原因の場所（`src/parser.rs`）: `index_or_slice` は、`xs[a..b]` の `..` を範囲の式として読まないために `stop_at_slice_dotdot` を `true` にして添字の式を読む。この旗は、角括弧の中の `{ … }` や `( … )` に入っても `false` に戻らない。二項演算子を読む `expression` の中の `minimum == 0 && !self.stop_at_slice_dotdot && self.eat(&TokenKind::DotDot)` が `..` を拒み、`for` の見出しが `do` の前で止まる。直す場所の候補は、括弧とブロックに入るときにこの旗を保存して `false` にし、出るときに戻すこと（`stop_at_arm`・`stop_at_arrow` と同じ扱い）。この直しは G36 の対象外（本文の「対象外」）。

付録 A の `index` は、添字の式を括弧の外の `let` に束縛して回避している（`let ix0j: i64u = ((e) as i64u); ix0[(ix0j % …) as i64]`）。`--self-test` が、この再現が今も失敗することを確かめる。直ったら、`--self-test` が「KB1 は直った」と言って止まるので、回避と `KNOWN_BUGS` の項目を消す。

## E. 生成される `.tz` の例

`node tests/progfuzz.mjs target/release/tsuzuri 1 5 2`（seed 5・depth 2）が書く `exe/Main.tz`（`TSUZURI_PROGFUZZ_KEEP=1`）:

```tsuzuri
union U0 = K0_0 of i16 | K0_1 of i64u

const def f0 :: bool -> i8 -> bool
fn f0 p0 p1 = (((p1 as i8u) as i64u) == ({ let v0: [i32u] = [2134666360i32u, 256i32u]; 13452433348025490985i64u }))

const def f1 :: i16u -> Maybe<i16u> -> i32u
fn f1 p0 p1 = 16i32u

const def run :: i64 -> i64
fn run k = ({ let x0: i8u = ((k >>> 14i64) as i8u); ({ let x1: i64u = ((k >>> 3i64) as i64u); (((((if (f0 ({ let v1: i64 = k; ((x1 as i8) < (-3i8)) }) ({ let g2: i8 = (x0 as i8); ((-118i8) / (if ((g2 == 0i8) || (g2 == (-1i8))) then 1i8 else g2)) })) then 1i64 else 0i64) * 31i64) + ((f1 (match 65536i32u with | 4i32u -> 127i16u | _ -> 25628i16u) ({ let ix0: Maybe<i16u> = Maybe.None; ix0 })) as i64)) * 31i64) + k) }) })

def main :: [string] -> i32 = \args ->
    let z = args.length
    do! IO.write_line (to_string (run (0i64 + z)))
    do! IO.write_line (to_string (run (1i64 + z)))
    do! IO.write_line (to_string (run ((-1i64) + z)))
    do! IO.write_line (to_string (run (9223372036854775807i64 + z)))
    do! IO.write_line (to_string (run ((-9223372036854775808i64) + z)))
    do! IO.write_line (to_string (run ((-702017493991926302i64) + z)))
    do! IO.write_line (to_string (run (8938910063984227991i64 + z)))
    do! IO.write_line (to_string (run (338i64 + z)))
    0
```

`const/Main.tz` は同じ本体で、`main` の代わりに次を持つ（`run` の呼び出しは定数の初期化式にだけある）。`lib/Main.tz` は同じ本体で、`run` の署名が `export const def run :: i64 -> i64` になり、`main` がない（wasm32 の `tz_run`）。

```tsuzuri
const C0: i64 = run 0i64
const C1: i64 = run 1i64
const C2: i64 = run (-1i64)
…
def main :: unit -> i32 = \() ->
    do! IO.write_line (to_string C0)
    …
    0
```

`TSUZURI_PROGFUZZ_MUTANT=div-floor TSUZURI_PROGFUZZ_PATHS=o0 node tests/progfuzz.mjs target/release/tsuzuri 5 1 4` が保存する `failures/seed-2-o0/info.txt` と `Main.tz`（実物。パスは省略。評価器を故意に誤らせたので、`expected` は誤った評価器の値）:

```text
seed 2  depth 4  path o0  kind value
inputs -1
expected 4294967294
actual -55522 (first difference in the original), input 2
nodes 320 -> 3 after 150 compiler runs
reproduce: node tests/progfuzz.mjs /…/target/release/tsuzuri 1 2 4
```

```tsuzuri
record R0 { f0: (i32u * i64), f1: i16, f2: i16, f3: (i64 * i32) }
union U0 = K0_0 of bool | K0_1 of (i8u * i32)
union U1 = K1_0 | K1_1 | K1_2 of (i8u * i64u)

const def run :: i64 -> i64
fn run k = (k % 4294967295i64)

def main :: [string] -> i32 = \args ->
    let z = args.length
    do! IO.write_line (to_string (run ((-1i64) + z)))
    0
```

最小化は節点の置換だけなので、使われない型の宣言は残る。実行ファイルの `-1 % 4294967295` は `-1`、床の剰余（誤った評価器）は `4294967294`。

## F. 位相ごとのファイルを作る手順

位相 N までのファイルは、付録 A の全文から、位相 N より大きい位相の印の間の行を落とせば作れる。次の短い script は確認用で、リポジトリに置かない（`target/` の下で使う）。`node strip.mjs tests/progfuzz.mjs 1 target/progfuzz-phase1.mjs` で位相 1 だけの版ができ、`node --check` が通り、`--self-test` が通る（実測: 位相 1 の版 1,170 行・3.7 秒、位相 1 と 2 の版 1,371 行・72 秒、位相 1〜3 の版 1,675 行・81 秒。印の行を含む付録 A の全文は 1,731 行）。

```js
import { readFileSync, writeFileSync } from "node:fs";

const [input, phaseText, output] = process.argv.slice(2);
const phase = Number(phaseText);
const stack = [];
const kept = [];
for (const line of readFileSync(input, "utf8").split("\n")) {
  const open = /^\s*\/\/ <<< phase (\d)$/.exec(line);
  const close = /^\s*\/\/ >>> phase (\d)$/.exec(line);
  if (open) { stack.push(Number(open[1])); continue; }
  if (close) {
    if (stack.pop() !== Number(close[1])) throw new Error(`unbalanced marker: ${line}`);
    continue;
  }
  if (stack.every((p) => p <= phase)) kept.push(line);
}
if (stack.length) throw new Error("unclosed marker");
writeFileSync(output, kept.join("\n"));
```

### 位相の印の領域と、足す手順

位相 2 の印（`// <<< phase 2`）は 9 つ、位相 3 の印は 19 つある。ファイルの上から数えた順に名前を付ける。手順 2.x・3.x が足すのは、次の表の領域。手順 1.2〜1.7 は位相 1 の行だけ（印の外）を、手順の「内容」にある関数の名前で足す。

| 領域 | 場所 | 内容 | 手順 |
| --- | --- | --- | --- |
| 2a | 設定の `allPaths` の直後 | `allPaths.push("wasm0", "wasm3")` | 2.1 |
| 2b | 設定の `STEP_LIMIT` の直後 | `minimizeBudget` | 2.2 |
| 2c | `exec` の後 | `wasmCall` | 2.1 |
| 2d | `runPath` の先頭 | wasm の経路の分岐 | 2.1 |
| 2e | `compare` の後 | 最小化の節（`SKIP` から `errorCode` まで） | 2.2 |
| 2f | `writeSources` の後 | `tailOf`・`minimizeFailure` | 2.3 |
| 2g | `checkProgram` の中 | `minimizeFailure` を呼ぶ 1 行 | 2.3 |
| 2h | `selfTesting` の step 4 | 最小化の検査 | 2.2 |
| 2i | `selfTesting` の step 6 | 変異の検出・最小化・保存 | 2.3 |
| 3a | 設定 | `kind` の読み込み | 3.1 |
| 3b | 設定 | owned のときの `paths` の絞り込み | 3.1 |
| 3c | 設定 | `generator`・`nativeLeaks`・`wantedLines` | 3.1 |
| 3d | `source` の先頭 | owned なら `sourceOwned` | 3.1 |
| 3e | `evaluate` の先頭 | owned なら `evaluateOwned` | 3.1 |
| 3f | 所有権の節 | `OWNED_HELPERS` から `evaluateOwned` まで | 3.1 |
| 3g | `wasmCall` の後 | `wasmOwnedCall` | 3.2 |
| 3h | 領域 2d の中 | `--allocator counting` と `wasmOwnedCall` の選択 | 3.2 |
| 3i | `runPath` の native の部分 | `nativeLeaks` のとき `--allocator counting` | 3.3 |
| 3j | `weightOf` の中 | owned の重み | 3.1 |
| 3k | `prune` の中 | owned は関数を捨てない | 3.1 |
| 3l | `candidates` の中 | owned のステップの削除の候補 | 3.1 |
| 3m | `writeSources` の中 | owned は `const` の文面を作らない | 3.1 |
| 3n | `checkProgram` の後 | `supportsAlloc` | 3.2 |
| 3o | `fuzz` の中 | owned の native の漏れの検査の有効化 | 3.3 |
| 3p | `REQUIRED_TAGS` の後 | `REQUIRED_TAGS.owned` | 3.1 |
| 3q | `MUTANTS` の後 | `OWNED_MUTANTS` | 3.1 |
| 3r | `selfTesting` の step 3 | `mutantSets.push(...)` | 3.1 |
| 3s | `selfTesting` の step 9 | 所有権のプログラムの検査 | 3.2 |

この表と手順の「内容」のとおりに作った段階版（手順 1.2〜1.7・2.1〜2.3・3.1〜3.2 の各終わり）で、各手順の「確認」の出力が出ることを実測した（手順 1.5 は変異の名前の配列 `MUTANTS` を足さないと `ReferenceError` になる。手順 3.2 は `supportsAlloc` を足さないと step 9 が `ReferenceError` になる）。手順 3.3 の終わりは付録 A の全文と同じ。
