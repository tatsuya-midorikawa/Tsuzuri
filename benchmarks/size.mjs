// PM08 size suite: the size of the artifacts a user gets, in bytes, recorded in the PX01 format.
//
//   node benchmarks/metrics.mjs size [compiler] --out <dir> [--project <dir>]... [--quick]
//
// `default` is the build a user runs: the whole-build cache and the runtime objects are on (each
// build gets its own empty cache directory, so the artifact never depends on a cache state).
// `nocache` is `--no-cache`: the runtime IR is embedded in the module. The PX01 `process` suite
// and the PX03 `build` suite record that embedded form, which is not what `tsuzuri build` ships.
// Sizes are deterministic, so a record has one sample, and every build is made twice and compared.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { artifactFingerprint, captureRun, makeRecord, writeRecords } from "./metrics.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
export const SIZE_PROJECTS = Object.freeze(["hello", "io", "point", "functional", "computations", "control", "currying",
  "polymorphism", "tasks"].map((name) => join(root, "examples", name)));

// The sections that carry the code and the read-only data of each object format. `llvm-objdump -h`
// prints the same table for all of them; the sizes are hexadecimal.
const LAYOUT = Object.freeze({
  macho: { code: ["__text"], data: ["__const", "__cstring"] },
  elf: { code: [".text"], data: [".rodata"] },
  coff: { code: [".text"], data: [".rdata"] },
  wasm: { code: ["CODE"], data: ["DATA"] },
});

/** The object format and the section sizes of the text that `llvm-objdump -h` prints. */
export function parseSections(text) {
  const format = /file format (\S+)/.exec(text)?.[1] ?? "";
  const sections = new Map();
  for (const line of text.split(/\r?\n/)) {
    const match = /^\s*\d+\s+(\S+)\s+([0-9a-f]+)\s/i.exec(line);
    if (match) sections.set(match[1], (sections.get(match[1]) ?? 0) + parseInt(match[2], 16));
  }
  return { format, sections };
}

/** `code` (`__text`, `.text`, `CODE`; required) and `data` (read-only data; 0 when absent) in bytes. */
export function sectionSizes(text, label = "artifact") {
  const { format, sections } = parseSections(text);
  const kind = /mach-o/i.test(format) ? "macho" : /^elf/i.test(format) ? "elf" : /^(coff|pe)/i.test(format) ? "coff"
    : /^wasm$/i.test(format) ? "wasm" : null;
  assert.ok(kind, `unrecognized object format '${format}' in ${label}`);
  const { code, data } = LAYOUT[kind];
  const present = (names) => names.filter((name) => sections.has(name));
  if (present(code).length === 0) throw new Error(`section ${code[0]} not found in ${label}`);
  const sum = (names) => present(names).reduce((total, name) => total + sections.get(name), 0);
  return { kind, code: sum(code), data: sum(data) };
}

export function artifactSections(file, objdump) {
  const child = spawnSync(objdump, ["-h", file], { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
  if (child.error || child.status !== 0) {
    throw new Error(`cannot read the sections of ${file} with ${objdump}: ${child.error?.message ?? child.stderr}`);
  }
  return sectionSizes(child.stdout, file);
}

const executableSuffix = process.platform === "win32" ? ".exe" : "";
const optionOf = (opt) => `-${opt}`;

// [variant, target, opts, extra arguments, compiler feature the row needs]
const ROWS = Object.freeze([
  ["default", "native", ["O0", "O3", "Os", "Oz"], [], null],
  ["nocache", "native", ["O0", "O3", "Os", "Oz"], ["--no-cache"], null],
  ["strip", "native", ["O3", "Oz"], ["--strip"], "strip"],
  ["object", "native", ["O3", "Oz"], ["--emit", "object"], null],
  ["default", "wasm32", ["O0", "O3", "Os", "Oz"], [], null],
  ["nocache", "wasm32", ["O0", "O3", "Os", "Oz"], ["--no-cache"], null],
  ["wasm-opt", "wasm32", ["O3", "Oz"], ["--wasm-opt"], "wasm-opt"],
]);

function compilerSupports(compiler) {
  const help = spawnSync(compiler, ["--help"], { encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
  const text = `${help.stdout}${help.stderr}`;
  return { size: text.includes("-Oz"), strip: text.includes("--strip"), "wasm-opt": text.includes("--wasm-opt") };
}

function unsupportedBuild(stderr) {
  return stderr.split(/\r?\n/).find((line) => /error\[E200[14]\]/.test(line)) ?? null;
}

/** Builds the same artifact twice, each with a cold cache directory of its own, and checks that they match. */
function buildTwice(compiler, args, extension, work, label) {
  const outputs = [];
  for (const copy of ["a", "b"]) {
    const output = join(work, `${label}-${copy}${extension}`);
    const cache = mkdtempSync(join(work, `cache-${copy}-`));
    const child = spawnSync(compiler, [...args, "-o", output], {
      cwd: root, env: { ...process.env, TSUZURI_CACHE_DIR: cache }, encoding: "utf8", maxBuffer: 64 * 1024 * 1024,
    });
    rmSync(cache, { recursive: true, force: true });
    if (child.error) throw child.error;
    if (child.status !== 0) {
      const reason = copy === "a" ? unsupportedBuild(child.stderr) : null;
      if (reason) return { skipped: reason };
      throw new Error(`${compiler} ${args.join(" ")} exited with ${child.status}:\n${child.stderr}`);
    }
    outputs.push(output);
  }
  assert.equal(artifactFingerprint(outputs[0]), artifactFingerprint(outputs[1]), `nondeterministic build: ${outputs[0]}`);
  return { output: outputs[0] };
}

function wasmOptUsable(environment) {
  const tool = environment.TSUZURI_WASM_OPT ?? "wasm-opt";
  const child = spawnSync(tool, ["--version"], { encoding: "utf8" });
  return { usable: !child.error && child.status === 0, explicit: environment.TSUZURI_WASM_OPT !== undefined, tool };
}

export function sizeSuite(compiler, out, { projects = SIZE_PROJECTS, quick = false, log = console.error,
  objdump = process.env.TSUZURI_OBJDUMP ?? "llvm-objdump", clang = process.env.TSUZURI_CLANG ?? "clang",
  rustc = process.env.TSUZURI_RUSTC ?? process.env.RUSTC ?? "rustc" } = {}) {
  const probe = spawnSync(objdump, ["--version"], { encoding: "utf8" });
  if (probe.error || probe.status !== 0) {
    throw new Error(`the size suite reads sections with llvm-objdump; install LLVM or set TSUZURI_OBJDUMP (${objdump})`);
  }
  const context = captureRun(out, { compiler, mode: quick ? "quick" : "benchmark" });
  const supports = compilerSupports(compiler);
  const records = [];
  const work = mkdtempSync(join(tmpdir(), "tsuzuri-size-"));
  const add = (fields, metric, value) => records.push(makeRecord(context, { suite: "size", cpu_mode: "generic", ...fields,
    metric, samples: [value] }));
  const nativeMetrics = (fields, file) => {
    add(fields, "executable_bytes", statSync(file).size);
    const { code, data } = artifactSections(file, objdump);
    add(fields, "text_bytes", code);
    add(fields, "rodata_bytes", data);
  };
  try {
    let counter = 0;
    for (const project of quick ? projects.slice(0, 1) : projects) {
      const workload = basename(project);
      for (const [variant, target, opts, extra, feature] of ROWS) {
        if (quick && (variant !== "default" || !opts.includes("O3"))) continue;
        let tool = null;
        if (feature === "wasm-opt") {
          tool = wasmOptUsable(process.env);
          if (!supports[feature] || !tool.usable) {
            const why = !supports[feature] ? "the compiler has no --wasm-opt" : `${tool.tool} is not usable`;
            if (tool.explicit && supports[feature]) throw new Error(`TSUZURI_WASM_OPT is set but ${why}`);
            log(`skip ${workload} ${variant} ${target}: ${why}`);
            continue;
          }
        } else if (feature && !supports[feature]) {
          log(`skip ${workload} ${variant} ${target}: the compiler has no --${feature}`);
          continue;
        }
        for (const opt of opts) {
          if (quick && opt !== "O3") continue;
          if (["Os", "Oz"].includes(opt) && !supports.size) {
            log(`skip ${workload} ${variant} ${opt} ${target}: the compiler has no -Os/-Oz`);
            continue;
          }
          const extension = target === "wasm32" ? ".wasm" : extra.includes("object") ? ".o" : executableSuffix;
          const args = ["build", project, optionOf(opt), ...(target === "wasm32" ? ["--target", "wasm32"] : ["--cpu", "generic"]), ...extra];
          const built = buildTwice(compiler, args, extension, work, `p${counter++}`);
          if (built.skipped) {
            log(`skip ${workload} ${variant} ${opt} ${target}: ${built.skipped}`);
            continue;
          }
          const fields = { workload, language: "tsuzuri", variant, target, opt };
          if (target === "wasm32") {
            add(fields, "wasm_bytes", statSync(built.output).size);
            const { code, data } = artifactSections(built.output, objdump);
            add(fields, "wasm_code_bytes", code);
            add(fields, "wasm_data_bytes", data);
          } else if (variant === "object") {
            add(fields, "object_bytes", statSync(built.output).size);
          } else {
            nativeMetrics(fields, built.output);
          }
        }
      }
    }
    // The references: a C and a Rust program that print the same number.
    for (const [language, command, source, optimizations] of [
      ["c", clang, "hello.c", [["O3", ["-O3"]], ["Oz", ["-Oz"]]]],
      ["rust", rustc, "hello.rs", [["O3", ["-C", "opt-level=3"]], ["Oz", ["-C", "opt-level=z"]]]],
    ]) {
      if (spawnSync(command, ["--version"], { encoding: "utf8" }).error) {
        log(`skip ${language} hello: ${command} not found`);
        continue;
      }
      for (const [opt, flags] of optimizations) {
        const output = join(work, `${language}-${opt}${executableSuffix}`);
        const built = spawnSync(command, [...flags, join(root, "benchmarks/size", source), "-o", output], { encoding: "utf8" });
        if (built.error || built.status !== 0) throw new Error(`${command} failed on ${source}:\n${built.error ?? built.stderr}`);
        nativeMetrics({ workload: "hello", language, variant: "default", target: "native", opt }, output);
      }
    }
    writeRecords(out, "size", records);
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
  return records;
}
