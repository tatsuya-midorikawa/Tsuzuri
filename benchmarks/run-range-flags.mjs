// Gate A of PR02: does writing the overflow flags that the range proofs allow change the code that
// LLVM generates? Every corpus is built to IR in TSUZURI_RANGE_FLAGS=off and =proven, the IR is
// compiled at -O1, -O2 and -O3 for arm64 and x86_64 (assembly only, nothing runs), and the
// wasm32 -O3 module is built in both modes. The assembly of the two modes is compared line by line, in
// order, with only comments and blank lines ignored; the module is compared byte by byte. The report lists,
// for each corpus, how many integer instructions carry a flag and where the assembly or the module differs.
//
//   node benchmarks/run-range-flags.mjs [compiler] [--levels 1,2,3] [--targets arm64,x86_64,wasm32]
//                                       [--corpora control,matrix,...] [--keep dir]
//
// The runs are deterministic and need no quiet machine; timings of kernels whose assembly differs
// are measured separately (docs/benchmarks.md).
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { assemblyLines, difference } from "./assembly.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const option = (name, fallback) => {
  const at = args.indexOf(`--${name}`);
  return at >= 0 ? args[at + 1] : fallback;
};
const compiler = resolve(args[0] && !args[0].startsWith("--") ? args[0] : join(root, "target/release/tsuzuri"));
const levels = option("levels", "1,2,3").split(",").map(Number);
const targets = option("targets", "arm64,x86_64,wasm32").split(",");
const clang = process.env.TSUZURI_CLANG ?? "clang";
const keep = option("keep", null);
const work = resolve(keep ?? join(root, "target", `range-flags-${process.pid}`));

// Directories of Main.tz, and the library files that have no Main.tz.
const corpora = [
  ["control", "benchmarks/control"],
  ["computations", "benchmarks/computations"],
  ["bounds_shapes", "benchmarks/bounds_shapes"],
  ["matrix", "benchmarks/matrix"],
  ["simd", "benchmarks/simd"],
  ["utf", "benchmarks/utf/Kernels.tz"],
  ["dispatch", "benchmarks/dispatch"],
  ["apps", "benchmarks/apps"],
  ["cpp", "benchmarks/cpp/Kernels.tz"],
  ["allocation", "benchmarks/allocation"],
  ["range_flags", "benchmarks/range_flags"],
  ["fixtures", "tests/fixtures/control"],
];
const chosen = option("corpora", null)?.split(",");
const selected = corpora.filter(([name]) => !chosen || chosen.includes(name));

const triples = { arm64: "arm64-apple-macos14", x86_64: "x86_64-unknown-linux-gnu" };

function run(program, programArgs, env = {}) {
  const result = spawnSync(program, programArgs, {
    cwd: root, encoding: "utf8", maxBuffer: 1024 * 1024 * 1024, env: { ...process.env, TSUZURI_CACHE_DIR: "", ...env },
  });
  if (result.status !== 0) throw new Error(`${program} ${programArgs.join(" ")}\n${result.stderr}`);
  return result;
}

mkdirSync(work, { recursive: true });
const rows = [];
try {
  for (const [name, path] of selected) {
    if (!existsSync(join(root, path))) continue;
    const ir = {};
    for (const mode of ["off", "proven"]) {
      const file = join(work, `${name}-${mode}.ll`);
      run(compiler, ["build", path, "--emit", "llvm", "-o", file], { TSUZURI_RANGE_FLAGS: mode });
      ir[mode] = file;
    }
    // The flags of `off` are the ones that hand-written runtime IR carries; the proofs add the rest.
    const count = (mode, pattern) => (readFileSync(ir[mode], "utf8").match(pattern) ?? []).length;
    const flaggedOff = count("off", / = (?:add|sub|mul) (?:nuw |nsw )+i\d+/g);
    const flagged = count("proven", / = (?:add|sub|mul) (?:nuw |nsw )+i\d+/g);
    const arithmetic = count("proven", / = (?:add|sub|mul) (?:nuw |nsw )*i\d+/g);
    const row = { corpus: name, flaggedOff, flagged, arithmetic, results: {} };
    for (const target of targets.filter((t) => t !== "wasm32")) {
      for (const level of levels) {
        const text = {};
        for (const mode of ["off", "proven"]) {
          const out = join(work, `${name}-${mode}-${target}-O${level}.s`);
          run(clang, [`-O${level}`, "-S", `--target=${triples[target]}`, "-fPIC", "-fno-fast-math", "-ffp-contract=off", "-Wno-override-module", ir[mode], "-o", out]);
          text[mode] = assemblyLines(readFileSync(out, "utf8"), target);
        }
        row.results[`${target} -O${level}`] = { unit: "lines", off: text.off.length, proven: text.proven.length, differing: difference(text.off, text.proven) };
      }
    }
    if (targets.includes("wasm32")) {
      const bytes = {};
      try {
        for (const mode of ["off", "proven"]) {
          const out = join(work, `${name}-${mode}.wasm`);
          run(compiler, ["build", path, "--target", "wasm32", "-O3", "-o", out], { TSUZURI_RANGE_FLAGS: mode });
          bytes[mode] = readFileSync(out);
        }
        row.results["wasm32 -O3"] = { unit: "bytes", off: bytes.off.length, proven: bytes.proven.length, differing: Buffer.compare(bytes.off, bytes.proven) === 0 ? 0 : 1 };
      } catch (error) {
        // A corpus without an entry point or an export is not a WebAssembly module.
        if (!/E2004/.test(String(error))) throw error;
      }
    }
    rows.push(row);
    const describe = ([key, value]) => `${key}: ${value.differing === 0 ? "same" : value.unit === "bytes" ? `module differs (${value.off} -> ${value.proven} bytes)` : `${value.differing} lines differ (${value.off} -> ${value.proven} lines)`}`;
    const summary = Object.entries(row.results).map(describe).join("; ");
    console.log(`${name}: ${flaggedOff} (off) and ${flagged} (proven) of ${arithmetic} integer + - * carry a flag; ${summary}`);
  }
  writeFileSync(join(work, "report.json"), JSON.stringify(rows, null, 2));
  const changed = rows.flatMap((row) => Object.entries(row.results).filter(([, value]) => value.differing !== 0).map(([key]) => `${row.corpus} ${key}`));
  console.log(changed.length ? `changed: ${changed.join(", ")}` : "no corpus differs in any target or level");
} finally {
  if (!keep) rmSync(work, { recursive: true, force: true });
}
