// Runs the existing node suites with TSUZURI_RANGE_FLAGS=audit and proven (PR02), and the integer
// range fuzzer. Audit mode writes every overflow flag that a range proof allows as a check that
// traps instead, so a suite that passes in audit mode shows that no proof failed on its programs;
// proven mode writes the flags themselves, so it shows that the optimized code still computes the
// same values. Run it again after merging anything that changes the typed IR or the emitter.
//
//   node tests/range_flags_audit.mjs [compiler] [quick|full] [mode ...]
//
// quick: the suites that compute with integers, about 20 minutes on a loaded machine. full: every node
// suite of the README's test list and the other differential fuzzers, an hour or more.
// TSUZURI_AUDIT_SUITES=control,e2e names the suites to run instead.
//
// The same check for the Rust tests is `TSUZURI_RANGE_FLAGS=audit cargo test --locked` (and
// `proven`), which gives every test that emits IR the mode; a test that asserts the absence of a
// flag chooses `RangeFlags::Off` itself.
import { spawnSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(process.argv[2] ?? join(root, "target/release/tsuzuri"));
const size = process.argv[3] ?? "quick";
const modes = process.argv.length > 4 ? process.argv.slice(4) : ["audit", "proven"];
if (!["quick", "full"].includes(size) || !modes.every((mode) => ["audit", "proven", "off"].includes(mode))) {
  console.error("usage: node tests/range_flags_audit.mjs [compiler] [quick|full] [audit|proven|off ...]");
  process.exit(2);
}

const quick = ["control", "numeric_casts", "integer_intrinsics", "primitives", "computations", "strings", "e2e"];
const full = [
  "e2e", "primitives", "prebuilt_runtime", "display_parse", "strings", "tasks", "async", "features", "json", "computations",
  "control", "lsp_sessions", "io", "repl", "script", "os", "targets", "cpu_kernels", "f16_hardware", "packages", "bindgen",
  "debug_info", "frontend_cache", "wasm_threads", "wasm_memory", "wasm_simd", "wasm64", "numeric_casts", "integer_intrinsics",
  "user_drop", "trap_boundary", "trap_return", "trap_return_sync", "stack_overflow", "allocator", "cache", "examples",
  "docgen", "host_imports", "ffi_extensions", "bindings", "bindings_threads", "host_bindings", "gpu", "strings_bridges",
  "simd", "cpu_dispatch", "benchmark_report", "benchmark_apps",
];
const named = process.env.TSUZURI_AUDIT_SUITES?.split(",").filter(Boolean);
const suites = (named ?? (size === "full" ? full : quick)).map((name) => [name, [`tests/${name}.mjs`, compiler]]);
if (!named && size === "full") {
  suites.push(["utf_simd", ["tests/utf_simd.mjs", "--quick"]],
    ["local_recursion_fuzz", ["tests/local_recursion_fuzz.mjs", compiler, "100"]],
    ["last_use_fuzz", ["tests/last_use_fuzz.mjs", compiler, "300", "1000", "4"]]);
}

let failed = 0;
for (const mode of modes) {
  for (const [name, args] of suites) {
    const start = Date.now();
    const result = spawnSync("node", args, {
      cwd: root, encoding: "utf8", maxBuffer: 256 * 1024 * 1024, timeout: 2 * 60 * 60 * 1000,
      env: { ...process.env, TSUZURI_RANGE_FLAGS: mode },
    });
    const seconds = Math.round((Date.now() - start) / 1000);
    const ok = result.status === 0;
    failed += ok ? 0 : 1;
    console.log(`${ok ? "ok  " : "FAIL"} ${mode.padEnd(6)} ${name} ${seconds}s`);
    if (!ok) console.log(`${result.stdout.split("\n").slice(-20).join("\n")}\n${result.stderr.split("\n").slice(-30).join("\n")}`);
  }
}
// The fuzzer builds each program in all three modes itself.
const size_args = size === "full" ? ["300", "1", "4"] : ["30", "1", "3"];
const start = Date.now();
const fuzz = spawnSync("node", ["tests/integer_ranges_fuzz.mjs", compiler, ...size_args], {
  cwd: root, encoding: "utf8", maxBuffer: 256 * 1024 * 1024, timeout: 2 * 60 * 60 * 1000,
});
console.log(`${fuzz.status === 0 ? "ok  " : "FAIL"} fuzz   integer_ranges_fuzz ${Math.round((Date.now() - start) / 1000)}s ${fuzz.stdout.trim()}`);
if (fuzz.status !== 0) {
  failed++;
  console.log(fuzz.stderr.split("\n").slice(-40).join("\n"));
}
if (failed) {
  console.error(`${failed} failed`);
  process.exit(1);
}
console.log("range flags audit: every suite passed");
