// Generates the 43 operator extension instances (NC19 Specification §6) in `std/Tensor.tz`
// between `// generated:tensor-operators` and `// /generated:tensor-operators`.
//
// Usage: node scripts/gen-tensor-operators.mjs [--check]

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const target = join(root, "std", "Tensor.tz");
const checkOnly = process.argv.includes("--check");

const START_MARKER = "// generated:tensor-operators";
const END_MARKER = "// /generated:tensor-operators";

const BINARY_OPS = [
  { cls: "Add", method: "add", pair: "add_or_trap", rhsScalar: "add_scalar", lhsScalar: "radd_scalar" },
  { cls: "Sub", method: "sub", pair: "sub_or_trap", rhsScalar: "sub_scalar", lhsScalar: "rsub_scalar" },
  { cls: "Mul", method: "mul", pair: "mul_or_trap", rhsScalar: "mul_scalar", lhsScalar: "scale" },
  { cls: "Div", method: "div", pair: "div_or_trap", rhsScalar: "div_scalar", lhsScalar: "rdiv_scalar" },
  { cls: "Rem", method: "rem", pair: "rem_or_trap", rhsScalar: "rem_scalar", lhsScalar: "rrem_scalar" },
  { cls: "Pow", method: "pow", pair: "pow_or_trap", rhsScalar: "pow_scalar", lhsScalar: "rpow_scalar" },
];

function generateBody() {
  const lines = [];
  for (const op of BINARY_OPS) {
    const ctx = `(${op.cls}<'a>, Copy<'a>)`;
    const shapes = [
      [`View<'a>, View<'a>, Tensor<'a>`, `${op.pair} left right`],
      [`Tensor<'a>, View<'a>, Tensor<'a>`, `${op.pair} (ref (view left)) right`],
      [`View<'a>, Tensor<'a>, Tensor<'a>`, `${op.pair} left (ref (view right))`],
      [`'a, Tensor<'a>, Tensor<'a>`, `${op.lhsScalar} (deref left) (ref (view right))`],
      [`Tensor<'a>, 'a, Tensor<'a>`, `${op.rhsScalar} (deref right) (ref (view left))`],
      [`'a, View<'a>, Tensor<'a>`, `${op.lhsScalar} (deref left) right`],
      [`View<'a>, 'a, Tensor<'a>`, `${op.rhsScalar} (deref right) left`],
    ];
    for (const [args, call] of shapes) {
      lines.push(`instance ${ctx} => ${op.cls}<${args}> {`);
      lines.push(`    fn ${op.method} left right = ${call}`);
      lines.push(`}`);
      lines.push(``);
    }
  }
  lines.push(`instance (Neg<'a>, Copy<'a>) => Neg<View<'a>, Tensor<'a>> {`);
  lines.push(`    fn neg value = neg value`);
  lines.push(`}`);
  return lines.join("\n");
}

const text = readFileSync(target, "utf8");
const nl = text.includes("\r\n") ? "\r\n" : "\n";
const lines = text.split(/\r?\n/);
const startIdx = lines.indexOf(START_MARKER);
const endIdx = lines.indexOf(END_MARKER);
if (startIdx === -1 || endIdx === -1 || endIdx <= startIdx) {
  console.error(`gen-tensor-operators: markers not found in ${target}`);
  process.exit(1);
}

const generatedLines = generateBody().split("\n");
const nextLines = [...lines.slice(0, startIdx + 1), ...generatedLines, ...lines.slice(endIdx)];
const nextText = nextLines.join(nl);

if (checkOnly) {
  if (nextText !== text) {
    console.error("gen-tensor-operators: std/Tensor.tz generated block is stale; run node scripts/gen-tensor-operators.mjs");
    process.exit(1);
  }
  console.log("gen-tensor-operators: 43 instances up to date");
} else {
  if (nextText !== text) {
    writeFileSync(target, nextText, "utf8");
    console.log("gen-tensor-operators: updated 43 instances in std/Tensor.tz");
  } else {
    console.log("gen-tensor-operators: 43 instances already up to date");
  }
}
