// PM03: runs the existing node suites with TSUZURI_STRINGS=compact (and utf16), the way
// tests/range_flags_audit.mjs runs them with TSUZURI_RANGE_FLAGS. A suite that passes in both forms shows
// that no program of the suite reads or writes the layout of a `string` behind the compiler's back, and that
// the Latin-1 strings that `compact` makes behave as the UTF-16 strings of `utf16` do. Run it again after
// merging anything that touches strings, the runtime's string files, or the emitter's reading of a length.
//
//   node tests/compact_strings_modes.mjs [compiler] [quick|full] [utf16|compact ...]
//
// quick: the suites that read or write strings, about 15 minutes on a loaded machine. full: every node suite
// of the README's test list that a string can reach, an hour or more. TSUZURI_MODES_SUITES=strings,json names
// the suites to run instead. The suites that read LLVM's tools (debug_info, wasm_simd) need them on PATH; the
// ones with BigInt-heavy checks are quicker on Node 24. The same check for the Rust tests is
// `TSUZURI_STRINGS=compact cargo test --locked`: every test that emits IR gets the form, and a test that
// asserts the IR of the UTF-16 form emits that form itself.
import { spawnSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(process.argv[2] ?? join(root, "target/release/tsuzuri"));
const size = process.argv[3] ?? "quick";
const modes = process.argv.length > 4 ? process.argv.slice(4) : ["compact", "utf16"];
if (!["quick", "full"].includes(size) || !modes.every((mode) => ["utf16", "compact"].includes(mode))) {
  console.error("usage: node tests/compact_strings_modes.mjs [compiler] [quick|full] [utf16|compact ...]");
  process.exit(2);
}

const quick = [
  "compact_strings_raw", "compact_strings", "strings", "display_parse", "integer_format", "primitives", "size", "json",
  "host_imports", "host_bindings", "io",
];
const full = [
  ...quick, "e2e", "examples", "bindings", "features", "os", "script", "repl", "targets", "packages", "bindgen", "wasm_memory",
  "wasm64", "ffi_extensions", "bindings_threads", "docgen", "user_drop", "trap_boundary", "trap_return", "trap_return_sync",
  "tasks", "async", "computations", "control", "debug_info", "wasm_simd", "debugger",
];
const named = process.env.TSUZURI_MODES_SUITES?.split(",").filter(Boolean);
const suites = (named ?? (size === "full" ? full : quick)).map((name) => [name, [`tests/${name}.mjs`, compiler, ...(name === "compact_strings" && size === "quick" ? ["--quick"] : [])]]);

let failed = 0;
for (const mode of modes) {
  for (const [name, args] of suites) {
    const start = Date.now();
    const result = spawnSync(process.execPath, args, {
      cwd: root, encoding: "utf8", maxBuffer: 256 * 1024 * 1024, timeout: 2 * 60 * 60 * 1000,
      env: { ...process.env, TSUZURI_STRINGS: mode },
    });
    const seconds = Math.round((Date.now() - start) / 1000);
    const ok = result.status === 0;
    failed += ok ? 0 : 1;
    console.log(`${ok ? "ok  " : "FAIL"} ${mode.padEnd(7)} ${name} ${seconds}s`);
    if (!ok) console.log(`${result.stdout.split("\n").slice(-20).join("\n")}\n${result.stderr.split("\n").slice(-30).join("\n")}`);
  }
}
if (failed) {
  console.error(`${failed} failed`);
  process.exit(1);
}
console.log("compact strings modes: every suite passed");
