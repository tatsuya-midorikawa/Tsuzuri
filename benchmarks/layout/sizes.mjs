// Sizes and alignments of the named record and union types of a Tsuzuri project (PM01).
//
// usage: node benchmarks/layout/sizes.mjs <compiler> <project> [--out dir] [--target native|wasm32|<clang triple>]... [--all]
//
// The IR comes from `<compiler> build <project> --emit llvm -O0` (with `--target wasm32` for wasm32). Each named
// struct type of that IR is measured by clang alone, so the layout is the real one of the target triple: no `opt`,
// no hand-copied data layout. Any other `--target` value is a clang triple measured on the native IR, for example
// `x86_64-unknown-linux-gnu`.
// Every row is `size align type definition`. Without `--all` only user and standard records and unions are printed.
import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const usage = "Usage: node benchmarks/layout/sizes.mjs <compiler> <project> [--out dir] [--target native|wasm32|<clang triple>]... [--all]";
const args = process.argv.slice(2);
const positional = [];
const targets = [];
let out = join(root, "target/perf/PM01-sizes");
let all = false;
for (let index = 0; index < args.length; ++index) {
  const arg = args[index];
  if (arg === "--all") all = true;
  else if (arg === "--out" || arg === "--target") {
    const value = args[++index];
    if (value === undefined || value.startsWith("--")) throw new Error(usage);
    if (arg === "--out") out = resolve(value);
    else targets.push(value);
  } else if (!arg.startsWith("-")) positional.push(arg);
  else throw new Error(usage);
}
if (positional.length !== 2) throw new Error(usage);
const [compiler, project] = positional.map((path) => resolve(path));
if (targets.length === 0) targets.push("native", "wasm32", "x86_64-unknown-linux-gnu");
const clang = process.env.TSUZURI_CLANG ?? "clang";
mkdirSync(out, { recursive: true });

function emit(wasm) {
  const file = join(out, wasm ? "layout-wasm32.ll" : "layout-native.ll");
  const flags = ["build", project, "--emit", "llvm", "-O0", "-o", file];
  if (wasm) flags.push("--target", "wasm32");
  try {
    execFileSync(compiler, flags, { env: { ...process.env, TSUZURI_CACHE_DIR: "" }, stdio: ["ignore", "ignore", "pipe"] });
  } catch (error) {
    throw new Error(`${compiler} ${flags.join(" ")} failed:\n${error.stderr}`);
  }
  return readFileSync(file, "utf8");
}

function measure(ir, triple) {
  const types = ir.split("\n").filter((line) => /^%[^ ]+ = type/.test(line) && !line.includes("opaque"));
  let text = `${types.join("\n")}\n`;
  types.forEach((line, index) => {
    const name = line.split(" = type")[0];
    text += `define i64 @size.${index}() {\n  ret i64 ptrtoint (ptr getelementptr (${name}, ptr null, i32 1) to i64)\n}\n`;
    text += `define i64 @align.${index}() {\n  ret i64 ptrtoint (ptr getelementptr ({ i1, ${name} }, ptr null, i32 0, i32 1) to i64)\n}\n`;
  });
  const arguments_ = ["-O1", "-S", "-emit-llvm", "-x", "ir", "-", "-o", "-", "-Wno-override-module"];
  if (triple !== "native") arguments_.unshift("-target", triple);
  const folded = execFileSync(clang, arguments_, { input: text, encoding: "utf8", stdio: ["pipe", "pipe", "ignore"] });
  const values = new Map();
  for (const block of folded.split(/^define /m).slice(1)) {
    const name = /@(size|align)\.(\d+)\(/.exec(block);
    const value = /ret i64 (\d+)/.exec(block);
    if (name && value) values.set(`${name[1]}.${name[2]}`, value[1]);
  }
  return types.map((line, index) => `${values.get(`size.${index}`)} ${values.get(`align.${index}`)} ${line}`);
}

let native;
let wasm32;
for (const target of targets) {
  const wasm = target === "wasm32";
  const ir = wasm ? (wasm32 ??= emit(true)) : (native ??= emit(false));
  const rows = measure(ir, wasm ? "wasm32-unknown-unknown" : target);
  writeFileSync(join(out, `sizes-${target}.txt`), `${rows.join("\n")}\n`);
  console.log(`# ${target}`);
  for (const row of rows) if (all || /^\d+ \d+ %"?tz\.(union|record)\./.test(row)) console.log(row);
}
