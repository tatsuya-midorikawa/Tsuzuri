// Differential test of the integer range proofs (PR02): random programs of integer arithmetic and
// loops, compiled in the three TSUZURI_RANGE_FLAGS modes, against an independent BigInt evaluator of
// the same syntax tree. The expected values come from that evaluator, which follows the documented
// semantics (wrapping + - *, truncating / and %, masked shift counts, wrapping casts, loops that stop
// at the end of the type), never from the compiler's output.
//
//   node tests/integer_ranges_fuzz.mjs [compiler] [programs] [first seed] [depth]
//
// Every program is built for wasm32 as audit at -O0 (a wrong proof is a trap), and as proven and off
// at -O3, and run on boundary-biased inputs (all inputs of an 8-bit type). With
// TSUZURI_FUZZ_NATIVE=1 the same builds also run natively through clang (TSUZURI_CLANG).
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(process.argv[2] ?? join(root, "target/release/tsuzuri"));
const programs = Number(process.argv[3] ?? 20);
const firstSeed = Number(process.argv[4] ?? 1);
const depth = Number(process.argv[5] ?? 3);
const native = process.env.TSUZURI_FUZZ_NATIVE === "1";
const clang = process.env.TSUZURI_CLANG ?? "clang";
assert.ok([programs, firstSeed, depth].every(Number.isSafeInteger) && programs > 0 && depth > 0);
const work = join(root, "target", `integer-ranges-fuzz-${process.pid}`);

function mulberry32(seed) {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

// ---- types ----

const types = [8, 16, 32, 64].flatMap((bits) =>
  [true, false].map((signed) => ({ bits, signed, name: `i${bits}${signed ? "" : "u"}` })));
const typeOf = (name) => types.find((type) => type.name === name);
const I64 = typeOf("i64");
const low = (t) => (t.signed ? -(1n << BigInt(t.bits - 1)) : 0n);
const high = (t) => (t.signed ? (1n << BigInt(t.bits - 1)) - 1n : (1n << BigInt(t.bits)) - 1n);
const wrap = (t, value) => (t.signed ? BigInt.asIntN(t.bits, value) : BigInt.asUintN(t.bits, value));
const fits = (t, value) => value >= low(t) && value <= high(t);

// Values near the edges of a type and of the thresholds that programs compare with.
function pool(t) {
  const values = new Set([low(t), low(t) + 1n, high(t), high(t) - 1n, high(t) / 2n, high(t) / 2n + 1n]);
  for (const v of [0n, 1n, 2n, 3n, 4n, 5n, 7n, 8n, 9n, 10n, 15n, 16n, 31n, 32n, 63n, 64n, 99n, 100n, 127n, 128n,
    255n, 256n, 1000n, 65535n, 65536n, 0x7fffffffn, 0x80000000n, 0xffffffffn, 0x100000000n]) values.add(v);
  if (t.signed) for (const v of [-1n, -2n, -3n, -8n, -100n, -127n, -128n, -129n, -255n, -256n, -32768n]) values.add(v);
  return [...values].filter((v) => fits(t, v)).sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
}

// ---- the generator ----

// The programs are built to put the proofs on the edge: the constants of an operation are chosen at
// the largest value that does not overflow, and one past it, given the range that the generator
// knows its operand has (a loop counter, a variable that an enclosing condition narrows, a mask).
// A proof that is off by one then produces a wrong flag, which audit mode turns into a trap.
function generate(seed) {
  const random = mulberry32(seed);
  const below = (n) => Math.floor(random() * n);
  const chance = (p) => random() < p;
  const pick = (list) => list[below(list.length)];
  // 64-bit programs get a third of the seeds: their loops are the ones whose counters carry flags
  // that a loop shape alone proves.
  const T = chance(0.33) ? pick([I64, typeOf("i64u")]) : pick(types);
  let counter = 0;
  const fresh = (prefix) => `${prefix}${counter++}`;
  const lit = (t, v) => ({ t: "lit", ty: t, v });
  const anyLit = (t) => lit(t, pick(pool(t)));
  // A name in scope with the interval that the generator knows it has.
  const entry = (n, lo = low(T), hi = high(T)) => ({ n, lo, hi });
  const variable = (e) => ({ t: "var", ty: T, n: e.n, lo: e.lo, hi: e.hi });
  const unique = (list) => [...new Set(list)];

  const masks = (t) => [1n, 3n, 7n, 15n, 31n, 63n, 127n, 255n, 0xffffn, 0xffffffffn, 6n, 12n, 0xf0n, -2n, -16n]
    .filter((v) => fits(t, v));

  // The values that an expression can have, as far as the generator knows.
  function rangeOf(node) {
    const t = node.ty;
    switch (node.t) {
      case "lit": return [node.v, node.v];
      case "var": return [node.lo, node.hi];
      case "and": return node.c.v >= 0n ? [0n, node.c.v] : [low(t), high(t)];
      case "rem": return t.signed ? [-(node.c.v - 1n), node.c.v - 1n] : [0n, node.c.v - 1n];
      case "shr": return t.signed ? [low(t) >> node.k, high(t) >> node.k] : [0n, high(t) >> node.k];
      default: return [low(t), high(t)];
    }
  }

  // Constants for `a op k` where `a` lies in [lo, hi], at and just beyond the edge of the type.
  function tuned(op, t, [lo, hi]) {
    const near = [];
    const around = (v) => near.push(v - 1n, v, v + 1n);
    if (op === "+") { around(high(t) - hi); around(low(t) - lo); }
    if (op === "-") { around(lo - low(t)); around(hi - high(t)); }
    if (op === "*") {
      for (const bound of [lo, hi]) {
        if (bound > 0n) { around(high(t) / bound); around(((1n << BigInt(t.bits)) - 1n) / bound); }
        if (bound < 0n) { around(low(t) / bound); around(high(t) / bound); }
      }
    }
    return unique(near).filter((v) => fits(t, v));
  }

  // What `variable op literal` narrows the variable to where it holds.
  function narrowing(e, op, c) {
    let { lo, hi } = e;
    if (op === "<") hi = hi < c - 1n ? hi : c - 1n;
    else if (op === "<=") hi = hi < c ? hi : c;
    else if (op === ">") lo = lo > c + 1n ? lo : c + 1n;
    else if (op === ">=") lo = lo > c ? lo : c;
    else if (op === "==") { lo = lo > c ? lo : c; hi = hi < c ? hi : c; }
    else if (c === lo) lo += 1n;
    else if (c === hi) hi -= 1n;
    return lo <= hi ? { ...e, lo, hi } : e;
  }
  const narrowScope = (scope, narrowed) => scope.map((e) => narrowed.findLast((n) => n.n === e.n) ?? e);

  // A comparison, and the variables that it narrows where it holds.
  function comparison(scope, d) {
    const op = pick(["<", "<=", ">", ">=", "==", "!="]);
    const e = pick(scope);
    if (chance(0.7)) {
      const c = anyLit(T);
      // `k op x` says the same as `x flipped-op k`, and a proof must read it that way round too.
      const flipped = { "<": ">", "<=": ">=", ">": "<", ">=": "<=", "==": "==", "!=": "!=" };
      const node = chance(0.35)
        ? { t: "cmp", op: flipped[op], a: c, b: variable(e) }
        : { t: "cmp", op, a: variable(e), b: c };
      return { node, narrowed: [narrowing(e, op, c.v)] };
    }
    const a = chance(0.7) ? variable(e) : expr(T, scope, d - 1);
    const b = chance(0.5) ? variable(pick(scope)) : expr(T, scope, d - 1);
    return { node: { t: "cmp", op, a, b }, narrowed: [] };
  }

  function condition(scope, d) {
    if (!chance(0.3)) return comparison(scope, d);
    const first = comparison(scope, d);
    const second = comparison(narrowScope(scope, first.narrowed), d);
    return { node: { t: "land", a: first.node, b: second.node }, narrowed: [...first.narrowed, ...second.narrowed] };
  }

  function leaf(t, scope) {
    if (t === T) return chance(0.7) ? variable(pick(scope)) : anyLit(t);
    return chance(0.6) ? { t: "cast", ty: t, a: variable(pick(scope)) } : anyLit(t);
  }

  // `a op k` or `k op a` with the constant at the edge of what `a` allows.
  function arithmetic(t, scope, d) {
    const op = pick(["+", "-", "*"]);
    const a = expr(t, scope, d - 1);
    const constants = tuned(op, t, rangeOf(a));
    if (constants.length && chance(0.55)) {
      const k = lit(t, pick(constants));
      return op !== "-" && chance(0.3) ? { t: "bin", ty: t, op, a: k, b: a } : { t: "bin", ty: t, op, a, b: k };
    }
    if (op === "-" && chance(0.25)) {
      // `k - a` overflows below the type when k is small and a is large.
      const [lo, hi] = rangeOf(a);
      const edges = unique([low(t) + hi - 1n, low(t) + hi, low(t) + hi + 1n, high(t) + lo - 1n, high(t) + lo, high(t) + lo + 1n]).filter((v) => fits(t, v));
      if (edges.length) return { t: "bin", ty: t, op, a: lit(t, pick(edges)), b: a };
    }
    return { t: "bin", ty: t, op, a, b: expr(t, scope, d - 1) };
  }

  function loop(scope, d) {
    const t = T;
    const i = fresh("i");
    const small = (n) => lit(t, BigInt(n));
    const near = (v) => lit(t, v);
    // `count` iterations that cover a large part of the type, so that arithmetic on the counter
    // crosses the edge of the type inside the loop.
    const scaled = (count, start, down) => {
      const room = high(t) - start;
      const step = room / BigInt(count) > 0n ? room / BigInt(count) : 1n;
      const stop = start + step * BigInt(count - 1);
      return down
        ? { lo: near(stop), hi: near(start), step: near(-step), range: [start, stop] }
        : { lo: near(start), hi: near(stop), step: step === 1n && chance(0.5) ? null : near(step), range: [start, stop] };
    };
    const window = (a, b, step) => ({ ...step, range: [a, b] });
    // A descending loop whose counter takes its largest values at the start, which a proof must not forget.
    const descending = (first, last) => ({ lo: small(first), hi: small(last), step: lit(t, -1n), range: [BigInt(last), BigInt(first)] });
    const schemes = [
      () => ({ lo: small(below(6)), hi: small(5 + below(9)), step: null, range: [0n, 13n] }),
      () => ({ lo: small(below(10)), hi: small(below(10)), step: small(1 + below(3)), range: [0n, 9n] }),
      () => {
        const first = high(t) - BigInt(below(8));
        return window(first, high(t), { lo: near(first), hi: near(high(t)), step: pick([null, small(2), small(3)]) });
      },
      () => ({ lo: { t: "sub", ty: t, a: near(high(t)), b: { t: "and", ty: t, a: variable(pick(scope)), c: lit(t, 7n) } }, hi: near(high(t)), step: pick([null, small(2)]), range: [high(t) - 7n, high(t)] }),
      () => ({ lo: { t: "and", ty: t, a: variable(pick(scope)), c: lit(t, 3n) }, hi: { t: "bin", ty: t, op: "+", a: { t: "and", ty: t, a: variable(pick(scope)), c: lit(t, 3n) }, b: small(below(9)) }, step: null, range: [0n, 11n] }),
      () => ({ lo: small(0), hi: small(below(31)), step: { t: "bin", ty: t, op: "+", a: { t: "and", ty: t, a: variable(pick(scope)), c: lit(t, 3n) }, b: small(1) }, range: [0n, 30n] }),
      () => scaled(2 + below(40), 0n, false),
      () => scaled(2 + below(40), 0n, false),
      ...(t.signed ? [
        // A counter of step 1 or -1 that crosses zero: it is -1 and then 0, which is an unsigned overflow.
        () => { const k = 1 + below(5); return { lo: small(-k), hi: small(k), step: null, range: [BigInt(-k), BigInt(k)] }; },
        () => { const k = 1 + below(5); return { lo: small(k), hi: small(-k), step: lit(t, -1n), range: [BigInt(-k), BigInt(k)] }; },
        () => descending(5 + below(8), below(6)),
        () => descending(5 + below(8), below(6)),
        () => { const last = low(t) + BigInt(below(8)); return window(low(t), last, { lo: near(low(t)), hi: near(last), step: null }); },
        () => { const first = low(t) + BigInt(below(8)); return window(low(t), first, { lo: near(first), hi: near(low(t)), step: lit(t, -1n) }); },
        () => { const last = high(t) - BigInt(below(8)); return window(last, high(t), { lo: near(high(t)), hi: near(last), step: lit(t, -2n) }); },
        () => scaled(2 + below(40), low(t) / 2n, false),
        () => scaled(2 + below(40), 0n, true),
        () => scaled(2 + below(40), 0n, true),
      ] : []),
    ];
    const { lo, hi, step, range } = pick(schemes)();
    const acc = fresh("acc");
    const inner = [...scope, entry(i, range[0], range[1])];
    const body = expr(t, inner, d - 1);
    const update = chance(0.6) ? { t: "bin", ty: t, op: pick(["+", "-", "*"]), a: { t: "var", ty: t, n: acc, lo: low(t), hi: high(t) }, b: body } : body;
    return { t: "loop", ty: t, i, acc, init: chance(0.7) ? small(below(4)) : expr(t, scope, d - 1), lo, hi, step, update };
  }

  function expr(t, scope, d) {
    if (d <= 0) return leaf(t, scope);
    const roll = below(100);
    if (roll < 34) return arithmetic(t, scope, d);
    if (roll < 42) return { t: "and", ty: t, a: expr(t, scope, d - 1), c: lit(t, pick(masks(t))) };
    if (roll < 47) {
      const c = pick([2n, 3n, 4n, 7n, 10n, 16n, 100n]);
      return { t: pick(["rem", "div"]), ty: t, a: expr(t, scope, d - 1), c: lit(t, fits(t, c) ? c : 2n) };
    }
    if (roll < 52) {
      return { t: pick(["shr", "shl"]), ty: t, a: expr(t, scope, d - 1), k: BigInt(below(t.bits)) };
    }
    if (roll < 55) return { t: t.signed && chance(0.5) ? "neg" : "not", ty: t, a: expr(t, scope, d - 1) };
    if (roll < 57) {
      const via = pick(types.filter((other) => other !== t));
      return { t: "cast", ty: t, a: { t: "cast", ty: via, a: expr(t, scope, d - 1) } };
    }
    if (roll < 65) {
      // The arithmetic is done in another type, and the result is cut back.
      const via = pick(types.filter((other) => other.bits > t.bits));
      if (via) return { t: "wide", ty: t, via, op: pick(["+", "-", "*"]), a: expr(t, scope, d - 1), b: expr(t, scope, d - 1) };
    }
    if (roll < 79) {
      const cond = condition(scope, d);
      return { t: "if", ty: t, c: cond.node, a: expr(t, narrowScope(scope, cond.narrowed), d - 1), b: expr(t, scope, d - 1) };
    }
    if (roll < 88) {
      const n = fresh("v");
      const e = expr(t, scope, d - 1);
      const [lo, hi] = rangeOf(e);
      return { t: "let", ty: t, n, e, body: expr(t, [...scope, entry(n, lo, hi)], d - 1) };
    }
    if (t === T && d >= 2 && roll < 97) return loop(scope, d);
    return leaf(t, scope);
  }

  // Half of the programs are a loop at the top, with the rest of the generator inside it.
  const scope = [entry("x"), entry("y")];
  return { T, body: depth >= 2 && chance(0.5) ? loop(scope, depth) : expr(T, scope, depth) };
}

// ---- source ----

const suffix = (t) => t.name;
function renderLiteral(t, v) {
  return `${v}${suffix(t)}`;
}

function render(node) {
  switch (node.t) {
    case "lit": return renderLiteral(node.ty, node.v);
    case "var": return node.n;
    case "bin": return `(${render(node.a)} ${node.op} ${render(node.b)})`;
    case "sub": return `(${render(node.a)} - ${render(node.b)})`;
    case "and": return `(${render(node.a)} & ${render(node.c)})`;
    case "rem": return `(${render(node.a)} % ${render(node.c)})`;
    case "div": return `(${render(node.a)} / ${render(node.c)})`;
    case "shr": return `(${render(node.a)} >>> ${node.k}${node.ty.name})`;
    case "shl": return `(${render(node.a)} <<< ${node.k}${node.ty.name})`;
    case "neg": return `(-${render(node.a)})`;
    case "not": return `(~${render(node.a)})`;
    case "cast": return `(${render(node.a)} as ${node.ty.name})`;
    case "wide": return `(((${render(node.a)} as ${node.via.name}) ${node.op} (${render(node.b)} as ${node.via.name})) as ${node.ty.name})`;
    case "cmp": return `(${render(node.a)} ${node.op} ${render(node.b)})`;
    case "if": return `(if ${renderCondition(node.c)} then ${render(node.a)} else ${render(node.b)})`;
    case "let": return `({ let ${node.n} = ${render(node.e)}; ${render(node.body)} })`;
    case "loop": {
      const range = node.step
        ? `${render(node.lo)} .. ${render(node.step)} ..= ${render(node.hi)}`
        : `${render(node.lo)} ..= ${render(node.hi)}`;
      return `({ let mut ${node.acc} = ${render(node.init)}; for ${node.i} in ${range} do { ${node.acc} = ${render(node.update)}; () }; ${node.acc} })`;
    }
    default: throw new Error(node.t);
  }
}

function renderCondition(node) {
  return node.t === "land" ? `(${renderCondition(node.a)} && ${renderCondition(node.b)})` : render(node);
}

function source(program) {
  const { T, body } = program;
  return `export def f :: ${T.name} -> ${T.name} -> i64\nfn f x y = (${render(body)} as i64)\n`;
}

// ---- the evaluator ----

function evaluate(program, x, y) {
  const run = (node, env) => {
    switch (node.t) {
      case "lit": return node.v;
      case "var": return env.get(node.n);
      case "bin": {
        const a = run(node.a, env);
        const b = run(node.b, env);
        return wrap(node.ty, node.op === "+" ? a + b : node.op === "-" ? a - b : a * b);
      }
      case "sub": return wrap(node.ty, run(node.a, env) - run(node.b, env));
      case "and": return wrap(node.ty, run(node.a, env) & run(node.c, env));
      case "rem": return run(node.a, env) % run(node.c, env);
      case "div": return run(node.a, env) / run(node.c, env);
      case "shr": return run(node.a, env) >> node.k;
      case "shl": return wrap(node.ty, run(node.a, env) << node.k);
      case "neg": return wrap(node.ty, -run(node.a, env));
      case "not": return wrap(node.ty, ~run(node.a, env));
      case "cast": return wrap(node.ty, run(node.a, env));
      case "wide": {
        const a = wrap(node.via, run(node.a, env));
        const b = wrap(node.via, run(node.b, env));
        return wrap(node.ty, wrap(node.via, node.op === "+" ? a + b : node.op === "-" ? a - b : a * b));
      }
      case "cmp": {
        const a = run(node.a, env);
        const b = run(node.b, env);
        return { "<": a < b, "<=": a <= b, ">": a > b, ">=": a >= b, "==": a === b, "!=": a !== b }[node.op];
      }
      case "if": return condition(node.c, env) ? run(node.a, env) : run(node.b, env);
      case "let": return run(node.body, new Map(env).set(node.n, run(node.e, env)));
      case "loop": {
        const t = node.ty;
        let acc = run(node.init, env);
        const first = run(node.lo, env);
        const last = run(node.hi, env);
        const step = node.step ? run(node.step, env) : 1n;
        assert.notEqual(step, 0n, "a zero step traps");
        const up = !t.signed || step > 0n;
        for (let i = first; up ? i <= last : i >= last;) {
          const inner = new Map(env).set(node.i, i).set(node.acc, acc);
          acc = run(node.update, inner);
          const next = i + step;
          if (!fits(t, next)) break;
          i = next;
        }
        return acc;
      }
      default: throw new Error(node.t);
    }
  };
  const condition = (node, env) => (node.t === "land" ? condition(node.a, env) && condition(node.b, env) : run(node, env));
  return wrap(I64, run(program.body, new Map([["x", x], ["y", y]])));
}

// ---- inputs ----

function inputsOf(program, seed) {
  const t = program.T;
  const values = pool(t);
  const random = mulberry32(seed ^ 0x9e3779b9);
  const pairs = [];
  if (t.bits === 8) {
    for (let a = low(t); a <= high(t); a++) for (let b = low(t); b <= high(t); b++) pairs.push([a, b]);
    return pairs;
  }
  for (const a of values) for (const b of values) pairs.push([a, b]);
  const span = high(t) - low(t) + 1n;
  for (let n = 0; n < 150; n++) {
    const draw = () => low(t) + (BigInt(Math.floor(random() * 2 ** 32)) * 4294967296n + BigInt(Math.floor(random() * 2 ** 32))) % span;
    pairs.push([draw(), draw()]);
  }
  return pairs;
}

// ---- running ----

const argument = (t, value) => (t.bits === 64 ? value : Number(value));

function build(directory, mode, level) {
  const output = join(directory, `${mode}-O${level}.wasm`);
  const result = spawnSync(compiler, ["build", directory, "--target", "wasm32", `-O${level}`, "-o", output], {
    cwd: root, encoding: "utf8", env: { ...process.env, TSUZURI_RANGE_FLAGS: mode, TSUZURI_CACHE_DIR: "" },
  });
  assert.equal(result.status, 0, `${mode} -O${level}: ${result.stderr}`);
  return new WebAssembly.Instance(new WebAssembly.Module(readFileSync(output))).exports;
}

function runWasm(exports, T, inputs, expected, label, text) {
  for (let index = 0; index < inputs.length; index++) {
    const [x, y] = inputs[index];
    let actual;
    try {
      actual = BigInt.asIntN(64, BigInt(exports.tz_f(argument(T, x), argument(T, y))));
    } catch (error) {
      assert.fail(`${label}: f(${x}, ${y}) ${error instanceof WebAssembly.RuntimeError ? "trapped (a proof is wrong, or a trap the evaluator missed)" : error}\n${text}`);
    }
    assert.equal(actual, expected[index], `${label}: f(${x}, ${y})\n${text}`);
  }
}

function runNative(directory, mode, level, T, inputs, expected, label, text) {
  const ir = join(directory, `${mode}.ll`);
  const env = { ...process.env, TSUZURI_RANGE_FLAGS: mode, TSUZURI_CACHE_DIR: "" };
  let result = spawnSync(compiler, ["build", directory, "--emit", "llvm", "-o", ir], { cwd: root, encoding: "utf8", env });
  assert.equal(result.status, 0, `${label}: ${result.stderr}`);
  const ctype = `${T.signed ? "" : "u"}int${T.bits}_t`;
  const list = (index) => inputs.map((pair) => `${pair[index]}${T.bits === 64 ? (T.signed ? "LL" : "ULL") : ""}`).join(", ");
  const driver = join(directory, "driver.c");
  writeFileSync(driver, `#include <stdint.h>\n#include <stdio.h>\nextern int64_t tz_f(${ctype}, ${ctype});\n` +
    `static const ${ctype} xs[] = {${list(0)}};\nstatic const ${ctype} ys[] = {${list(1)}};\n` +
    `int main(void) { for (unsigned i = 0; i < ${inputs.length}; i++) printf("%lld\\n", (long long)tz_f(xs[i], ys[i])); return 0; }\n`);
  const executable = join(directory, `native-${mode}-O${level}`);
  result = spawnSync(clang, [`-O${level}`, "-Wno-override-module", "-ffp-contract=off", ir, driver, "-lm", "-o", executable], { encoding: "utf8" });
  assert.equal(result.status, 0, `${label}: ${result.stderr}`);
  result = spawnSync(executable, [], { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
  assert.equal(result.status, 0, `${label}: native run failed (${result.signal ?? result.status})\n${text}`);
  const lines = result.stdout.trim().split("\n");
  assert.equal(lines.length, inputs.length, label);
  for (let index = 0; index < inputs.length; index++) {
    assert.equal(BigInt(lines[index]), expected[index], `${label}: f(${inputs[index][0]}, ${inputs[index][1]})\n${text}`);
  }
}

// How many instructions of the proven IR carry a flag: the proofs that the run puts to the test.
function flagged(directory) {
  const ir = join(directory, "proven.ll");
  const result = spawnSync(compiler, ["build", directory, "--emit", "llvm", "-o", ir], {
    cwd: root, encoding: "utf8", env: { ...process.env, TSUZURI_RANGE_FLAGS: "proven", TSUZURI_CACHE_DIR: "" },
  });
  assert.equal(result.status, 0, result.stderr);
  const text = readFileSync(ir, "utf8");
  const body = text.slice(text.indexOf("@tz.fn.Main.f("));
  const own = body.slice(0, body.indexOf("\n}"));
  return { all: (own.match(/ = (?:add|sub|mul) (?:nuw |nsw )+i\d+/g) ?? []).length, arithmetic: (own.match(/ = (?:add|sub|mul) /g) ?? []).length };
}

mkdirSync(work, { recursive: true });
let calls = 0;
let proofs = 0;
let instructions = 0;
const usage = { audit: 0 };
try {
  for (let seed = firstSeed; seed < firstSeed + programs; seed++) {
    const program = generate(seed);
    const text = `seed ${seed}\n${source(program)}`;
    const directory = join(work, `p${seed}`);
    mkdirSync(directory, { recursive: true });
    writeFileSync(join(directory, "Main.tz"), source(program));
    const inputs = inputsOf(program, seed);
    let expected;
    try {
      expected = inputs.map(([x, y]) => evaluate(program, x, y));
    } catch (error) {
      if (error.code === "ERR_ASSERTION") continue;
      throw error;
    }
    for (const [mode, level] of [["audit", 0], ["proven", 3], ["off", 3]]) {
      const label = `${mode} -O${level} wasm32`;
      runWasm(build(directory, mode, level), program.T, inputs, expected, label, text);
      calls += inputs.length;
    }
    if (native) {
      for (const [mode, level] of [["audit", 0], ["proven", 3]]) {
        runNative(directory, mode, level, program.T, inputs, expected, `${mode} -O${level} native`, text);
        calls += inputs.length;
      }
    }
    const stats = flagged(directory);
    proofs += stats.all;
    instructions += stats.arithmetic;
    usage.audit++;
    rmSync(directory, { recursive: true, force: true });
  }
  console.log(`integer ranges fuzz: ${usage.audit} programs, ${calls} calls matched the evaluator in audit, proven and off modes${native ? " (native too)" : ""}; ${proofs} of ${instructions} integer + - * and loop counters carry a flag`);
} finally {
  rmSync(work, { recursive: true, force: true });
}
