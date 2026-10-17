// PR07 D9: where does LLVM put a trap block? A trap block that the likely path jumps over costs a taken branch on that path, and a
// branch weight (!prof) could move it. Every corpus is built to IR, compiled at -O3 for arm64 and x86_64 (assembly only, nothing
// runs), and each trap instruction (`brk #0x1`, `ud2`) is classified by what comes before and after it in its function:
//
//   out_of_line    follows a jump, a return or another trap (or starts the function): only a branch reaches it;
//   chain          follows another instruction, the end of a path that fails after some cleanup (a free, a store);
//   at_end         follows a conditional branch and ends the function: the trap block is already last;
//   loop_exit      follows a conditional branch back to an earlier label (a loop's back edge) and code follows it: the loop
//                  falls through into the trap when it runs out, and the likely path takes the back edge;
//   jump_over      follows a conditional branch forward over it and code follows it: the likely path takes a branch that
//                  a different layout would not need. Only this kind is a layout that a branch weight could improve.
//
//   node benchmarks/run-trap-layout.mjs [compiler] [--targets arm64,x86_64] [--level 3] [--corpora control,apps,...]
//
// The runs are deterministic and need no quiet machine.
import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { assemblyLines } from "./assembly.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const option = (name, fallback) => {
  const at = args.indexOf(`--${name}`);
  return at >= 0 ? args[at + 1] : fallback;
};
const compiler = resolve(args[0] && !args[0].startsWith("--") ? args[0] : join(root, "target/release/tsuzuri"));
const targets = option("targets", "arm64,x86_64").split(",");
const level = Number(option("level", "3"));
const clang = process.env.TSUZURI_CLANG ?? "clang";
const triples = { arm64: "arm64-apple-macos14", x86_64: "x86_64-unknown-linux-gnu" };
const trap = { arm64: /^brk\s+#0x1$/, x86_64: /^ud2$/ };
const leaves = { arm64: /^(?:b|br|ret|retab|retaa|brk)(?:\s|$)/, x86_64: /^(?:jmp|jmpq|ret|retq|ud2)(?:\s|$)/ };
const conditional = { arm64: /^(?:b\.\w+|cbz|cbnz|tbz|tbnz)\s/, x86_64: /^j(?!mp)\w+\s/ };
const isLabel = (line) => /^[^\s]+:$/.test(line);
const local = /^(?:LBB|Ltmp|Lloh|Lfunc|ltmp|\.L)/;
const kinds = ["out_of_line", "chain", "at_end", "loop_exit", "jump_over"];

const benchmarks = ["control", "computations", "bounds_shapes", "matrix", "simd", "dispatch", "apps", "allocation", "range_flags"].map((name) => [name, `benchmarks/${name}`]);
const corpora = [...benchmarks, ["utf", "benchmarks/utf/Kernels.tz"], ["cpp", "benchmarks/cpp/Kernels.tz"], ["fixtures-control", "tests/fixtures/control"],
  ...["computations", "control", "currying", "functional", "hello", "io", "native", "point", "polymorphism", "tasks"].map((name) => [`examples-${name}`, `examples/${name}`])];
const chosen = option("corpora", null)?.split(",");

function run(program, programArgs) {
  const result = spawnSync(program, programArgs, { cwd: root, encoding: "utf8", maxBuffer: 1024 * 1024 * 1024, env: { ...process.env, TSUZURI_CACHE_DIR: "" } });
  if (result.status !== 0) throw new Error(`${program} ${programArgs.join(" ")}\n${result.stderr}`);
}

function classify(assembly, target) {
  const lines = assemblyLines(assembly, target);
  const labels = new Map(lines.flatMap((line, index) => (isLabel(line) ? [[line.slice(0, -1), index]] : [])));
  const counts = { traps: 0, ...Object.fromEntries(kinds.map((kind) => [kind, 0])) };
  let previous = null;
  // A trap after a conditional branch is classified by what follows it, which is known at the next instruction or label of a function.
  let pending = null;
  const settle = (followed) => {
    if (pending !== null) ++counts[followed ? pending : "at_end"];
    pending = null;
  };
  lines.forEach((line, index) => {
    if (line.startsWith(".")) return;
    if (isLabel(line)) {
      if (!local.test(line)) {
        settle(false);
        previous = null;
      }
      return;
    }
    settle(true);
    if (trap[target].test(line)) {
      ++counts.traps;
      if (previous === null || leaves[target].test(previous.text)) ++counts.out_of_line;
      else if (!conditional[target].test(previous.text)) ++counts.chain;
      else {
        const destination = labels.get(previous.text.split(/[\s,]+/).at(-1));
        pending = destination !== undefined && destination < index ? "loop_exit" : "jump_over";
      }
    }
    previous = { text: line, index };
  });
  settle(false);
  return counts;
}

const work = mkdtempSync(join(tmpdir(), "tsuzuri-trap-layout-"));
const total = Object.fromEntries(targets.map((target) => [target, { traps: 0, ...Object.fromEntries(kinds.map((kind) => [kind, 0])) }]));
const describe = (counts) => `${counts.traps} traps: ${kinds.map((kind) => `${counts[kind]} ${kind.replaceAll("_", " ")}`).join(", ")}`;
let built = 0;
try {
  for (const [name, path] of corpora.filter(([name]) => !chosen || chosen.includes(name))) {
    if (!existsSync(join(root, path))) continue;
    const ir = join(work, `${name}.ll`);
    try {
      run(compiler, ["build", path, "--emit", "llvm", "-o", ir]);
    } catch {
      console.log(`${name}: skipped (no module to build)`);
      continue;
    }
    ++built;
    const cells = [];
    for (const target of targets) {
      const out = join(work, `${name}-${target}.s`);
      run(clang, [`-O${level}`, "-S", `--target=${triples[target]}`, "-fPIC", "-fno-fast-math", "-ffp-contract=off", "-Wno-override-module", ir, "-o", out]);
      const counts = classify(readFileSync(out, "utf8"), target);
      for (const key of Object.keys(counts)) total[target][key] += counts[key];
      cells.push(`${target}: ${describe(counts)}`);
    }
    console.log(`${name}: ${cells.join("; ")}`);
  }
  console.log(`${built} corpora at -O${level}: ${targets.map((target) => `${target} ${describe(total[target])}`).join("; ")}`);
} finally {
  rmSync(work, { recursive: true, force: true });
}
