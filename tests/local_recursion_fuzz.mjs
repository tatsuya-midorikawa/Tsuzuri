// Differential test of local `let rec` (B09): random programs with nested `let rec` groups, closures,
// mutated captures, and borrowed strings and arrays, compiled and run natively, against an independent
// evaluator of the same syntax tree. The expected values come from that evaluator, which follows the
// documented semantics (a closure takes the value that a local had when it was made), never from the
// compiler's output.
//
//   node tests/local_recursion_fuzz.mjs [compiler] [programs] [first seed] [depth]
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const programs = Number(process.argv[3] ?? 40);
const firstSeed = Number(process.argv[4] ?? 1);
const depth = Number(process.argv[5] ?? 4);
const inputs = [0, 1, 7, 12];

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

// Every value is a small non-negative number, so no operation overflows and no index is negative.
// Strings are only measured and borrowed, and a string is never captured by an ordinary lambda, which
// would move it; arrays are Copy.
function generate(seed) {
  const random = mulberry32(seed);
  const below = (n) => Math.floor(random() * n);
  const chance = (p) => random() < p;
  let counter = 0;
  const fresh = (prefix) => `${prefix}${counter++}`;
  const lit = (v) => ({ t: "lit", v });
  const variable = (n) => ({ t: "var", n });

  // `sc.vars` are the names in scope, `sc.level` counts the functions around the code, and `sc.calls`
  // are the functions that the code may call with a smaller fuel than its own.
  const bind = (sc, entry) => ({ ...sc, vars: [...sc.vars, { level: sc.level, ...entry }] });
  const of = (sc, kind) => sc.vars.filter((entry) => entry.kind === kind);

  function leaf(sc) {
    const numbers = [...of(sc, "num"), ...of(sc, "mut")];
    const strings = of(sc, "str");
    const arrays = of(sc, "arr");
    for (let attempt = 0; attempt < 4; attempt++) {
      switch (below(4)) {
        case 0: return lit(below(10));
        case 1: if (numbers.length) return variable(numbers[below(numbers.length)].name); break;
        case 2: if (strings.length) return { t: "slen", n: strings[below(strings.length)].name }; break;
        default:
          if (arrays.length) {
            const array = arrays[below(arrays.length)];
            return chance(0.3) ? { t: "alen", n: array.name } : { t: "aget", n: array.name, len: array.len, i: leaf(sc) };
          }
      }
    }
    return lit(below(10));
  }

  function call(sc, depth, budget) {
    const functions = [...of(sc, "fn"), ...of(sc, "fn1")];
    const target = functions[below(functions.length)];
    if (target.kind === "fn1") {
      // A function made by applying a function to its first argument only.
      if (target.busy && budget.calls <= 0) return null;
      if (target.busy) budget.calls--;
      return { t: "call1", f: target.name, a: expr(sc, depth - 1, budget) };
    }
    if (target.busy) {
      // A function whose body is being written calls itself or its group only with a smaller fuel.
      const way = sc.calls.find((entry) => entry.name === target.name);
      if (!way || budget.calls <= 0) return null;
      budget.calls--;
      return { t: "call", f: target.name, a: { t: "bin", op: "-", a: variable(way.fuel), b: lit(1) }, b: expr(sc, depth - 1, budget) };
    }
    if (target.rec) {
      const fuel = { t: "bin", op: "%", a: expr(sc, depth - 1, budget), b: lit(4) };
      return chance(0.3)
        ? { t: "apply", f: target.name, a: fuel, b: expr(sc, depth - 1, budget) }
        : { t: "call", f: target.name, a: fuel, b: expr(sc, depth - 1, budget) };
    }
    return { t: "call", f: target.name, a: expr(sc, depth - 1, budget), b: expr(sc, depth - 1, budget) };
  }

  // An operand does not assign: `m == { m = 1; m }` borrows `m` while the other operand assigns it.
  function expr(scope, depth, budget) {
    const sc = { ...scope, assignable: false };
    if (depth <= 0) return leaf(sc);
    switch (below(11)) {
      case 0: case 1: return { t: "bin", op: chance(0.5) ? "+" : "*", a: expr(sc, depth - 1, budget), b: expr(sc, depth - 1, budget) };
      case 2: return { t: "if", c: { op: chance(0.5) ? "<" : "==", a: expr(sc, depth - 1, budget), b: expr(sc, depth - 1, budget) }, a: expr(sc, depth - 1, budget), b: expr(sc, depth - 1, budget) };
      case 3: case 4:
        if (of(sc, "fn").length || of(sc, "fn1").length) {
          const made = call(sc, depth, budget);
          if (made) return made;
        }
        return leaf(sc);
      case 5: return depth > 1 ? block(sc, depth - 1, budget) : leaf(sc);
      case 6: return { t: "match", e: expr(sc, depth - 1, budget), a: expr(sc, depth - 1, budget), b: expr(sc, depth - 1, budget), c: expr(sc, depth - 1, budget) };
      case 7: {
        // Matching a string reads it; the strings are borrowed where a `let rec` function holds them.
        const strings = of(sc, "str");
        if (!strings.length) return leaf(sc);
        return { t: "smatch", n: strings[below(strings.length)].name, lit: String(below(12)), a: expr(sc, depth - 1, budget), b: expr(sc, depth - 1, budget) };
      }
      default: return leaf(sc);
    }
  }

  // The statements of a block, in order; the scope grows as they bind names.
  function statements(sc, depth, budget, count) {
    const stmts = [];
    for (let index = 0; index < count; index++) {
      const made = statement(sc, depth, budget);
      stmts.push(made.stmt);
      sc = made.sc;
    }
    return { stmts, sc };
  }

  function block(sc, depth, budget, count = 1 + below(3)) {
    const made = statements(sc, depth, budget, count);
    return { t: "block", stmts: made.stmts, r: expr(made.sc, depth - 1, budget) };
  }

  function statement(sc, depth, budget) {
    for (let attempt = 0; attempt < 8; attempt++) {
      switch (below(15)) {
        case 0: case 1: {
          const n = fresh("v");
          return { stmt: { t: "let", n, e: expr(sc, depth - 1, budget) }, sc: bind(sc, { name: n, kind: "num" }) };
        }
        case 2: {
          const n = fresh("m");
          return { stmt: { t: "mut", n, e: expr(sc, depth - 1, budget) }, sc: bind(sc, { name: n, kind: "mut" }) };
        }
        case 3: case 4: {
          const mutable = sc.assignable ? of(sc, "mut").filter((entry) => entry.level === sc.level) : [];
          if (!mutable.length) break;
          return { stmt: { t: "set", n: mutable[below(mutable.length)].name, e: expr(sc, depth - 1, budget) }, sc };
        }
        case 5: {
          const n = fresh("s");
          return { stmt: { t: "str", n, e: expr(sc, depth - 1, budget) }, sc: bind(sc, { name: n, kind: "str" }) };
        }
        case 6: {
          const n = fresh("a");
          const es = [0, 1, 2].map(() => expr(sc, depth - 2, budget));
          return { stmt: { t: "arr", n, es }, sc: bind(sc, { name: n, kind: "arr", len: 3 }) };
        }
        case 7: {
          if (depth < 2) break;
          const n = fresh("h");
          const k = fresh("k");
          const x = fresh("x");
          // An ordinary lambda moves what it captures, so it sees no strings.
          let inner = { ...sc, level: sc.level + 1, assignable: true, vars: sc.vars.filter((entry) => entry.kind !== "str") };
          inner = bind(bind(inner, { name: k, kind: "num" }), { name: x, kind: "num" });
          const body = chance(0.5) ? block(inner, depth - 1, { calls: 1 }) : expr(inner, depth - 1, { calls: 1 });
          return { stmt: { t: "lam", n, k, x, body }, sc: bind(sc, { name: n, kind: "fn", rec: false }) };
        }
        case 8: {
          // A `let rec` function applied to its fuel, kept as a function of one argument.
          const functions = of(sc, "fn").filter((entry) => entry.rec);
          if (!functions.length) break;
          const target = functions[below(functions.length)];
          let fuel;
          if (target.busy) {
            const way = sc.calls.find((entry) => entry.name === target.name);
            if (!way || budget.calls <= 0) break;
            budget.calls--;
            fuel = { t: "bin", op: "-", a: variable(way.fuel), b: lit(1) };
          } else {
            fuel = { t: "bin", op: "%", a: expr(sc, depth - 1, budget), b: lit(4) };
          }
          const n = fresh("p");
          return { stmt: { t: "part", n, f: target.name, a: fuel }, sc: bind(sc, { name: n, kind: "fn1", busy: Boolean(target.busy) }) };
        }
        case 9: {
          // A loop makes its closures again each time round, over the same mutable locals.
          if (depth < 2) break;
          const i = fresh("i");
          const made = statements(bind(sc, { name: i, kind: "num" }), depth - 1, budget, 1 + below(3));
          return { stmt: { t: "for", i, stmts: made.stmts }, sc };
        }
        default: {
          if (depth < 2) break;
          return recursive(sc, depth);
        }
      }
    }
    const n = fresh("v");
    return { stmt: { t: "let", n, e: lit(below(10)) }, sc: bind(sc, { name: n, kind: "num" }) };
  }

  // A `let rec` group: each member stops at a fuel of 0 and otherwise may call the members and the
  // enclosing functions with one less.
  function recursive(sc, depth) {
    const count = 1 + (chance(0.4) ? 1 : 0) + (chance(0.15) ? 1 : 0);
    const names = Array.from({ length: count }, () => fresh("f"));
    const members = [];
    let inside = { ...sc, level: sc.level + 1, assignable: true };
    for (const n of names) inside = bind(inside, { name: n, kind: "fn", rec: true, busy: true });
    for (const n of names) {
      const k = fresh("k");
      const x = fresh("x");
      const parameters = bind(bind(inside, { name: k, kind: "num" }), { name: x, kind: "num" });
      const base = expr({ ...parameters, calls: sc.calls }, depth - 1, { calls: 0 });
      const way = { ...parameters, calls: [...sc.calls, ...names.map((name) => ({ name, fuel: k }))] };
      const step = chance(0.5) ? block(way, depth - 1, { calls: 2 }) : expr(way, depth - 1, { calls: 2 });
      members.push({ n, k, x, base, step });
    }
    let after = sc;
    for (const n of names) after = bind(after, { name: n, kind: "fn", rec: true });
    return { stmt: { t: "rec", members }, sc: after };
  }

  const functions = [];
  for (let index = 0; index < 3; index++) {
    const n = `top${index}`;
    const parameter = fresh("n");
    const sc = bind({ vars: [], level: 0, calls: [], assignable: true }, { name: parameter, kind: "num" });
    functions.push({ n, parameter, body: block(sc, depth, { calls: 0 }, 2 + below(3)) });
  }
  return functions;
}

// ---- source ----

function render(node) {
  switch (node.t) {
    case "lit": return `${node.v}i64`;
    case "var": return node.n;
    case "bin": {
      const a = render(node.a);
      const b = render(node.b);
      if (node.op === "-") return `(${a} - ${b})`;
      if (node.op === "%") return `(${a} % ${b})`;
      return `((${a} ${node.op} ${b}) % 1000i64)`;
    }
    case "if": return `(if (${render(node.c.a)} ${node.c.op} ${render(node.c.b)}) then ${render(node.a)} else ${render(node.b)})`;
    case "call": return `(${node.f} ${render(node.a)} ${render(node.b)})`;
    case "call1": return `(${node.f} ${render(node.a)})`;
    case "apply": return `(apply1 (${node.f} ${render(node.a)}) ${render(node.b)})`;
    case "match": return `(match (${render(node.e)}) with | 0 -> (${render(node.a)}) | 1 -> (${render(node.b)}) | _ -> (${render(node.c)}))`;
    case "smatch": return `(match ${node.n} with | "${node.lit}" -> (${render(node.a)}) | _ -> (${render(node.b)}))`;
    case "slen": return `(String.length (ref ${node.n}))`;
    case "alen": return `${node.n}.length`;
    case "aget": return `${node.n}[(${render(node.i)}) % ${node.len}i64]`;
    case "block": return `({ ${[...node.stmts.map(renderStatement), render(node.r)].join(" ")} })`;
    default: throw new Error(node.t);
  }
}

function renderStatement(stmt) {
  switch (stmt.t) {
    case "let": return `let ${stmt.n} = ${render(stmt.e)};`;
    case "mut": return `let mut ${stmt.n} = ${render(stmt.e)};`;
    case "set": return `${stmt.n} = ${render(stmt.e)};`;
    case "str": return `let ${stmt.n} = to_string (${render(stmt.e)});`;
    case "arr": return `let ${stmt.n} = [${stmt.es.map(render).join(", ")}];`;
    case "lam": return `let ${stmt.n} = \\(${stmt.k}: i64) (${stmt.x}: i64) -> ${render(stmt.body)};`;
    case "part": return `let ${stmt.n} = (${stmt.f} ${render(stmt.a)});`;
    case "for": return `for ${stmt.i} in 0i64 .. 2i64 do { ${[...stmt.stmts.map(renderStatement), "()"].join(" ")} };`;
    case "rec":
      return stmt.members
        .map((m, index) => `${index === 0 ? "let rec" : "and"} ${m.n} = \\(${m.k}: i64) (${m.x}: i64) -> if (${m.k} == 0i64) then ${render(m.base)} else ${render(m.step)};`)
        .join(" ");
    default: throw new Error(stmt.t);
  }
}

function source(functions) {
  const lines = ["def apply1 :: (i64 -> i64) -> i64 -> i64 = \\f x -> f x", ""];
  for (const { n, parameter, body } of functions) lines.push(`def ${n} :: i64 -> i64 = \\${parameter} -> ${render(body)}`, "");
  lines.push("def main :: unit -> i32 = \\() ->");
  for (const { n } of functions) for (const input of inputs) lines.push(`    do! IO.write_line (to_string (${n} ${input}i64))`);
  lines.push("    0", "");
  return lines.join("\n");
}

// ---- the evaluator ----

class Limit extends Error {}

function evaluate(functions) {
  let steps = 0;
  // A scope is a chain of cells, so that an assignment reaches the scope of its variable.
  const lookup = (scope, name) => {
    for (let at = scope; at; at = at.parent) if (at.vars.has(name)) return at;
    throw new Error(`unbound ${name}`);
  };
  const get = (scope, name) => lookup(scope, name).vars.get(name);
  // A closure holds the values that the locals had when it was made.
  const snapshot = (scope) => {
    const values = new Map();
    const chain = [];
    for (let at = scope; at; at = at.parent) chain.unshift(at);
    for (const at of chain) for (const [name, value] of at.vars) values.set(name, value);
    return { vars: values, parent: null };
  };
  const invoke = (closure, a, b) => {
    const scope = { vars: new Map(closure.env.vars), parent: null };
    scope.vars.set(closure.k, a);
    scope.vars.set(closure.x, b);
    return run(closure.body, scope);
  };
  function run(node, scope) {
    if (++steps > 400000) throw new Limit();
    switch (node.t) {
      case "lit": return node.v;
      case "var": return get(scope, node.n);
      case "bin": {
        const a = run(node.a, scope);
        const b = run(node.b, scope);
        if (node.op === "-") return a - b;
        if (node.op === "%") return a % b;
        return (node.op === "+" ? a + b : a * b) % 1000;
      }
      case "if": {
        const a = run(node.c.a, scope);
        const b = run(node.c.b, scope);
        return (node.c.op === "<" ? a < b : a === b) ? run(node.a, scope) : run(node.b, scope);
      }
      case "call": {
        const f = get(scope, node.f);
        const a = run(node.a, scope);
        return invoke(f, a, run(node.b, scope));
      }
      case "apply": {
        const f = get(scope, node.f);
        const a = run(node.a, scope);
        return invoke(f, a, run(node.b, scope));
      }
      case "call1": {
        // A function applied to its fuel only: the fuel was worked out when it was made.
        const p = get(scope, node.f);
        return invoke(p.f, p.fuel, run(node.a, scope));
      }
      case "match": {
        const value = run(node.e, scope);
        return run(value === 0 ? node.a : value === 1 ? node.b : node.c, scope);
      }
      case "smatch": return run(get(scope, node.n) === node.lit ? node.a : node.b, scope);
      case "slen": return get(scope, node.n).length;
      case "alen": return get(scope, node.n).length;
      case "aget": return get(scope, node.n)[run(node.i, scope) % node.len];
      case "block": {
        const inner = { vars: new Map(), parent: scope };
        for (const stmt of node.stmts) exec(stmt, inner);
        return run(node.r, inner);
      }
      default: throw new Error(node.t);
    }
  }
  function exec(stmt, scope) {
    switch (stmt.t) {
      case "let": case "mut": scope.vars.set(stmt.n, run(stmt.e, scope)); break;
      case "set": lookup(scope, stmt.n).vars.set(stmt.n, run(stmt.e, scope)); break;
      case "str": scope.vars.set(stmt.n, String(run(stmt.e, scope))); break;
      case "arr": scope.vars.set(stmt.n, stmt.es.map((e) => run(e, scope))); break;
      case "lam": scope.vars.set(stmt.n, { k: stmt.k, x: stmt.x, body: stmt.body, env: snapshot(scope) }); break;
      case "part": scope.vars.set(stmt.n, { f: get(scope, stmt.f), fuel: run(stmt.a, scope) }); break;
      case "for":
        for (let i = 0; i <= 2; i++) {
          const inner = { vars: new Map([[stmt.i, i]]), parent: scope };
          for (const each of stmt.stmts) exec(each, inner);
        }
        break;
      case "rec": {
        const env = snapshot(scope);
        const made = stmt.members.map((m) => ({ k: m.k, x: m.x, body: { t: "if", c: { op: "==", a: { t: "var", n: m.k }, b: { t: "lit", v: 0 } }, a: m.base, b: m.step }, env: { vars: new Map(env.vars), parent: null } }));
        stmt.members.forEach((m, index) => {
          scope.vars.set(m.n, made[index]);
          for (const closure of made) closure.env.vars.set(m.n, made[index]);
        });
        break;
      }
      default: throw new Error(stmt.t);
    }
  }
  const lines = [];
  for (const { parameter, body } of functions) {
    for (const input of inputs) lines.push(String(run(body, { vars: new Map([[parameter, input]]), parent: null })));
  }
  return lines;
}

// ---- running ----

function execute(directory, optimization) {
  return new Promise((done) => {
    const child = spawn(compiler, ["run", directory, optimization, "--no-cache"], { stdio: ["ignore", "pipe", "pipe"] });
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (chunk) => { stdout += chunk; });
    child.stderr.on("data", (chunk) => { stderr += chunk; });
    child.on("close", (status) => done({ status, stdout, stderr }));
  });
}

const root = mkdtempSync(join(tmpdir(), "tsuzuri-let-rec-fuzz-"));
let discarded = 0;
let checked = 0;
const failures = [];
const queue = [];
for (let seed = firstSeed; seed < firstSeed + programs; seed++) {
  const functions = generate(seed);
  let expected;
  try {
    expected = evaluate(functions);
  } catch (error) {
    if (!(error instanceof Limit)) throw error;
    discarded++;
    continue;
  }
  queue.push({ seed, text: source(functions), expected });
}

async function worker() {
  while (queue.length) {
    const { seed, text, expected } = queue.shift();
    const directory = join(root, `seed-${seed}`);
    mkdirSync(directory);
    writeFileSync(join(directory, "Main.tz"), text);
    for (const optimization of seed % 3 === 0 ? ["-O0", "-O3"] : ["-O0"]) {
      const result = await execute(directory, optimization);
      const actual = result.stdout.trim().split("\n");
      if (result.status !== 0 || actual.join("|") !== expected.join("|")) {
        failures.push({ seed, optimization, directory, expected: expected.join("|"), actual: actual.join("|"), stderr: result.stderr.split("\n").filter((line) => /error|panic/.test(line)).slice(0, 3).join("\n") });
        break;
      }
    }
    checked++;
  }
}

await Promise.all(Array.from({ length: 4 }, worker));
if (failures.length) {
  for (const failure of failures.sort((a, b) => a.seed - b.seed)) {
    console.error(`seed ${failure.seed} ${failure.optimization}: ${failure.directory}/Main.tz\n  expected ${failure.expected}\n  actual   ${failure.actual}\n  ${failure.stderr}`);
  }
  console.error(`${failures.length} of ${checked} programs differ (directories kept)`);
  process.exitCode = 1;
} else {
  rmSync(root, { recursive: true, force: true });
  assert.ok(checked > 0, "no program was checked");
  console.log(`local let rec fuzz: ${checked} programs (${discarded} discarded for their size) match the evaluator on native -O0 (every third also -O3)`);
}
