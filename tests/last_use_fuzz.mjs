// Differential Copy semantics: the interpreter clones values, never reasons about last uses.
// node tests/last_use_fuzz.mjs [compiler] [programs] [first seed] [depth]
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { resolve, join } from "node:path";

const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const count = Number(process.argv[3] ?? 40);
const first = Number(process.argv[4] ?? 1);
const depth = Number(process.argv[5] ?? 4);
assert.ok([count, first, depth].every(Number.isSafeInteger) && count > 0 && depth > 0);
const directory = resolve(`target/last-use-fuzz-${process.pid}`);
const inputs = [0, 1, 2, 5, 9];
const coverage = Object.fromEntries(["update", "derived", "branch", "for", "while", "closure", "borrow", "match_temp", "match_last", "match_kept", "match_tuple", "match_guard", "match_overlap"].map(k => [k, 0]));
function random(seed) {
  let x = seed >>> 0;
  return n => {
    x = (Math.imul(x, 1664525) + 1013904223) >>> 0;
    return x % n;
  };
}
const lit = v => ({ tag: "lit", v });
const variable = name => ({ tag: "var", name });
const bin = (op, a, b) => ({ tag: "bin", op, a, b });
const get = (name, i) => ({ tag: "get", name, i });
const set = (name, i, v) => ({ tag: "set", name, i, v });
function generate(seed) {
  const below = random(seed);
  const statements = [], terms = [];
  for (let k = 0; k < 4; k++) statements.push({ tag: "array", name: `a${k}`, offset: below(8) + 1 });
  statements.push({ tag: "let", name: "t", value: lit(0), mutable: true });
  const index = () => lit(below(4));
  const scalar = level => level === 0
    ? (below(2) ? variable("n") : lit(below(30)))
    : (below(2) ? bin("+", scalar(level - 1), scalar(level - 1)) : {
      tag: "if",
      condition: bin("==", bin("%", variable("n"), lit(2)), lit(0)),
      yes: scalar(level - 1), no: scalar(level - 1),
    });
  const value = () => scalar(Math.min(depth, 5));
  for (let k = 0; k < depth; k++) {
    const a = `a${below(4)}`;
    statements.push({ tag: "assign", name: a, value: set(a, index(), value()), shape: "update" });
    statements.push({ tag: "let", name: `d${k}`, value: set(a, index(), value()), shape: "derived" });
    terms.push(get(a, index()), get(`d${k}`, index()));
    statements.push({ tag: "let", name: `b${k}`, value: { tag: "if", condition: bin("==", bin("%", variable("n"), lit(2)), lit(0)), yes: set(a, index(), value()), no: variable(a) }, shape: "branch" });
    terms.push(get(`b${k}`, index()));
  }
  statements.push({ tag: "for", name: "i", body: [
    { tag: "assign", name: "a0", value: set("a0", bin("%", variable("i"), lit(4)), bin("*", variable("i"), lit(below(5) + 1))) },
    { tag: "assign", name: "t", value: bin("+", variable("t"), get("a0", bin("%", variable("i"), lit(4)))) },
  ], shape: "for" });
  statements.push({ tag: "closure", name: "f", array: "a1", extra: below(12), shape: "closure" });
  statements.push({ tag: "assign", name: "a1", value: set("a1", lit(1), lit(below(40) + 40)), shape: "update" });
  terms.push({ tag: "call", name: "f", argument: lit(1) });
  statements.push({ tag: "borrow", name: "r", array: "a2", shape: "borrow" });
  statements.push({ tag: "let", name: "e", value: set("a2", lit(2), lit(77)) });
  terms.push(get("r", lit(2)), get("e", lit(2)));
  statements.push({ tag: "let", name: "j", value: lit(0), mutable: true });
  statements.push({ tag: "while", condition: bin("<", variable("j"), variable("n")), body: [
    { tag: "assign", name: "j", value: bin("+", variable("j"), lit(1)) },
    { tag: "jump", kind: "continue", condition: bin("==", bin("%", variable("j"), lit(3)), lit(0)) },
    { tag: "jump", kind: "break", condition: bin(">", variable("j"), lit(7)) },
    { tag: "assign", name: "a3", value: set("a3", bin("%", variable("j"), lit(4)), bin("*", variable("j"), lit(below(4) + 1))) },
    { tag: "assign", name: "t", value: bin("+", variable("t"), get("a3", bin("%", variable("j"), lit(4)))) },
  ], shape: "while" });
  for (const kind of ["temp", "last", "kept", "tuple", "guard", "overlap"]) {
    statements.push({ tag: "match", name: `m_${kind}`, kind, offset: below(10) + 1, index: below(4), shape: `match_${kind}` });
    terms.push(variable(`m_${kind}`));
  }
  terms.push(variable("t"), variable("n"), get("a1", lit(1)));
  const result = bin("%", terms.reduce((a, b) => bin("+", a, b), lit(0)), lit(1000003));
  return { statements, result };
}
function expression(e) {
  switch (e.tag) {
    case "lit": return String(e.v);
    case "var": return e.name;
    case "bin": return `(${expression(e.a)} ${e.op} ${expression(e.b)})`;
    case "get": return `${e.name}[${expression(e.i)}]`;
    case "set": return `(Array.set ${e.name} ${expression(e.i)} ${expression(e.v)})`;
    case "if": return `(if ${expression(e.condition)} then ${expression(e.yes)} else ${expression(e.no)})`;
    case "call": return `(${e.name} ${expression(e.argument)})`;
    default: throw Error(e.tag);
  }
}
function render(statements, indent = "    ") {
  return statements.map(s => {
    const line = x => indent + x;
    switch (s.tag) {
      case "array": return line(`let mut ${s.name} = new [i64](4, \\i -> i + ${s.offset})`);
      case "let": return line(`let ${s.mutable ? "mut " : ""}${s.name} = ${expression(s.value)}`);
      case "assign": return line(`${s.name} = ${expression(s.value)}`);
      case "for": return line(`for ${s.name} in 0 .. 3 do\n`) + render(s.body, indent + "    ");
      case "while": return line(`while ${expression(s.condition)} do\n`) + render(s.body, indent + "    ");
      case "jump": return line(`if ${expression(s.condition)} then ${s.kind} else ()`);
      case "closure": return line(`let ${s.name} = \\k -> ${s.array}[k % 4] + ${s.extra}`);
      case "borrow": return line(`let ${s.name} = ref ${s.array}`);
      case "match": {
        const payload = `new [i64](4, \\i -> i + ${s.offset})`;
        const match = value => `match ${value} with\n${indent}    | Maybe.Some xs -> xs[${s.index}]\n${indent}    | Maybe.None -> 0`;
        if (s.kind === "temp") return line(`let ${s.name} = ${match(`maybe ${s.offset}`)}`);
        if (s.kind === "last") return line(`let v_last = Maybe.Some (${payload})\n`) + line(`let ${s.name} = ${match("v_last")}`);
        if (s.kind === "kept") return line(`let v_kept = Maybe.Some (${payload})\n`) + line(`let first_kept = ${match("v_kept")}\n`) + line(`let second_kept = ${match("v_kept")}\n`) + line(`let ${s.name} = first_kept + second_kept`);
        if (s.kind === "tuple") return line(`let ${s.name} = match (maybe ${s.offset}, maybe ${s.offset + 1}) with\n${indent}    | (Maybe.Some a, Maybe.Some b) -> a[${s.index}] + b[${s.index}]\n${indent}    | _ -> 0`);
        if (s.kind === "guard") return line(`let ${s.name} = match maybe ${s.offset} with\n${indent}    | Maybe.Some xs when xs[0] > n -> xs[${s.index}] + 100\n${indent}    | Maybe.Some xs -> xs[${s.index}]\n${indent}    | Maybe.None -> 0`);
        return line(`let ${s.name} = match maybe ${s.offset} with\n${indent}    | Maybe.Some xs as whole -> xs[${s.index}] + (match whole with | Maybe.Some ys -> ys[${s.index}] | Maybe.None -> 0)\n${indent}    | Maybe.None -> 0`);
      }
      default: throw Error(s.tag);
    }
  }).join("\n");
}
const clone = v => Array.isArray(v) ? v.map(clone) : v;
function evaluate(e, env) {
  switch (e.tag) {
    case "lit": return e.v;
    case "var": return clone(env[e.name]);
    case "get": return env[e.name][evaluate(e.i, env)];
    case "set": {
      const out = clone(env[e.name]);
      out[evaluate(e.i, env)] = evaluate(e.v, env);
      return out;
    }
    case "if": return evaluate(e.condition, env) ? evaluate(e.yes, env) : evaluate(e.no, env);
    case "call": return env[e.name](evaluate(e.argument, env));
    case "bin": {
      const a = evaluate(e.a, env), b = evaluate(e.b, env);
      switch (e.op) {
        case "+": return a + b; case "*": return a * b; case "%": return a % b;
        case "==": return a === b; case "<": return a < b; case ">": return a > b;
        default: throw Error(e.op);
      }
    }
    default: throw Error(e.tag);
  }
}
function execute(statements, env) {
  for (const s of statements) {
    switch (s.tag) {
      case "array": env[s.name] = Array.from({ length: 4 }, (_, i) => i + s.offset); break;
      case "let": case "assign": env[s.name] = evaluate(s.value, env); break;
      case "borrow": env[s.name] = env[s.array]; break;
      case "closure": {
        const snapshot = clone(env[s.array]);
        env[s.name] = k => snapshot[k % 4] + s.extra;
        break;
      }
      case "for":
        for (let i = 0; i <= 3; i++) { env[s.name] = i; if (execute(s.body, env) === "break") break; }
        break;
      case "while":
        while (evaluate(s.condition, env)) { if (execute(s.body, env) === "break") break; }
        break;
      case "jump": if (evaluate(s.condition, env)) return s.kind; break;
      case "match": {
        // Maybe.Some constructs a value; bindings copy its payload. No move implementation here.
        const payload = Array.from({ length: 4 }, (_, i) => i + s.offset);
        const binding = clone(payload);
        let answer = binding[s.index];
        if (["kept", "overlap"].includes(s.kind)) answer += clone(payload)[s.index];
        if (s.kind === "tuple") answer += clone(payload.map(x => x + 1))[s.index];
        if (s.kind === "guard" && binding[0] > env.n) answer += 100;
        env[s.name] = answer;
        break;
      }
      default: throw Error(s.tag);
    }
  }
}
function source(tree) {
  return `def maybe :: i64 -> Maybe<[i64]> = \\v -> Maybe.Some (new [i64](4, \\i -> i + v))\n\ndef run :: i64 -> i64 = \\n ->\n${render(tree.statements)}\n    ${expression(tree.result)}\n\ndef main :: unit -> i32 = \\() ->\n${inputs.map(n => `    do! IO.write_line (to_string (run ${n}i64))`).join("\n")}\n    0\n`;
}
mkdirSync(directory, { recursive: true });
try {
  for (let seed = first; seed < first + count; seed++) {
    const tree = generate(seed), text = source(tree);
    for (const s of tree.statements) if (s.shape) coverage[s.shape]++;
    const expected = inputs.map(n => { const env = { n }; execute(tree.statements, env); return evaluate(tree.result, env); }).join("\n") + "\n";
    writeFileSync(join(directory, "Main.tz"), text);
    for (const optimization of [0, 3]) {
      const result = spawnSync(compiler, ["run", directory, `-O${optimization}`, "--no-cache"], {
        encoding: "utf8", timeout: 120000,
        env: { ...process.env, TMPDIR: directory, TMP: directory, TEMP: directory },
      });
      try {
        assert.equal(result.status, 0, result.stderr);
        assert.equal(result.stdout.replaceAll("\r\n", "\n"), expected);
      } catch (error) {
        console.error(`seed=${seed} depth=${depth} -O${optimization}\n${text}`);
        throw error;
      }
    }
  }
  for (const [shape, visits] of Object.entries(coverage)) assert.ok(visits > 0, shape);
  console.log(`last-use fuzz: ${count} programs, ${count * 2} native O0/O3 executions, seed ${first}, depth ${depth}; zero differences`);
  console.log(JSON.stringify(coverage));
} finally {
  rmSync(directory, { recursive: true, force: true });
}
