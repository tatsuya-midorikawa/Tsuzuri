// Checks of the fast backend (`--backend fast`, PB05) and the wrappers that run the existing suites on it.
//
//   node tests/fast_backend.mjs                     the checks of the option table, hello, traps and the cache
//   node tests/fast_backend.mjs wrapper DIR         writes DIR/clang, a stand-in for TSUZURI_CLANG that
//                                                   translates the -O0 LLVM IR it is asked to compile (route B)
//                                                   and DIR/tsuzuri, a stand-in for the compiler that adds
//                                                   --backend fast to -O0 builds (route A)
//   node tests/fast_backend.mjs parity SUITE...     runs tests/SUITE.mjs with both wrappers and reports how many
//                                                   compiles were translated, which fell back, and why
//
// The binaries come from `cargo build --release --features fast-backend`: target/release/tsuzuri and
// target/release/tz-fast-cc (override with TSUZURI_BIN and TSUZURI_FAST_CC).
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
  appendFileSync, chmodSync, copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, realpathSync,
  rmSync, symlinkSync, writeFileSync,
} from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const self = fileURLToPath(import.meta.url);
const executable = process.platform === "win32" ? ".exe" : "";

function binary(name, variable) {
  if (process.env[variable]) return process.env[variable];
  for (const profile of ["release", "debug"]) {
    const candidate = join(root, "target", profile, `${name}${executable}`);
    if (existsSync(candidate)) return candidate;
  }
  return join(root, "target", "release", `${name}${executable}`);
}

// ---- the clang stand-in (route B) ----

// A compile that the translator may take: only -O0 (or no level) of the host's own target, without
// the options that ask Clang for something the IR text does not carry.
const REFUSED = [/^-O[123sz]$/, /^-Ofast$/, /^-g/, /^--target/, /^-target$/, /^-fsanitize/, /^-mcpu/, /^-march/,
  /^-mavx/, /^-msse/, /^-flto/, /^-emit-llvm$/, /^-S$/, /^-E$/, /^-fprofile/, /^-mf16c/, /^-mfma/, /^-mavx512fp16/];

function log(event) {
  if (process.env.FAST_LOG) appendFileSync(process.env.FAST_LOG, `${JSON.stringify(event)}\n`);
}

function clangWrapper(args) {
  const real = process.env.FAST_REAL_CLANG ?? "clang";
  const passthrough = (reason) => {
    log({ event: "passthrough", reason });
    const result = spawnSync(real, args, { stdio: "inherit" });
    process.exit(result.status ?? 1);
  };
  const inputs = args.filter((arg, index) => arg.endsWith(".ll") && args[index - 1] !== "-o" && existsSync(arg));
  if (inputs.length === 0) return passthrough("no IR input");
  const refused = args.find((arg) => REFUSED.some((pattern) => pattern.test(arg)));
  if (refused) return passthrough(`option ${refused}`);
  if (args.some((arg) => /^-O[0-9]$/.test(arg) && arg !== "-O0")) return passthrough("optimized");
  const work = join(process.env.FAST_WORK ?? dirname(self), `fast-wrapper-${process.pid}`);
  mkdirSync(work, { recursive: true });
  const rewritten = [];
  const translated = new Map();
  for (const input of inputs) {
    const object = join(work, `${translated.size}-${basename(input, ".ll")}.o`);
    const started = process.hrtime.bigint();
    const result = spawnSync(process.env.FAST_CC, [input, "-o", object], { encoding: "utf8" });
    const ms = Number((process.hrtime.bigint() - started) / 1_000_000n);
    if (result.status !== 0) {
      const reason = (result.stderr || String(result.error)).trim().split("\n")[0];
      log({ event: "fallback", input, reason, ms });
      rmSync(work, { recursive: true, force: true });
      const direct = spawnSync(real, args, { stdio: "inherit" });
      process.exit(direct.status ?? 1);
    }
    log({ event: "translated", input, ms });
    translated.set(input, object);
  }
  for (let index = 0; index < args.length; index++) {
    const arg = args[index];
    if (arg === "-x" && args[index + 1] === "ir") {
      index++;
      continue;
    }
    rewritten.push(translated.get(arg) ?? arg);
  }
  const compileOnly = rewritten.includes("-c");
  const status = (() => {
    const sources = rewritten.filter((arg, index) => /\.(c|cc|cpp|o|a|ll)$/.test(arg) && rewritten[index - 1] !== "-o");
    if (compileOnly && sources.length === 1 && translated.size === 1) {
      copyFileSync(sources[0], rewritten[rewritten.indexOf("-o") + 1]);
      return 0;
    }
    return spawnSync(real, rewritten, { stdio: "inherit" }).status ?? 1;
  })();
  rmSync(work, { recursive: true, force: true });
  process.exit(status);
}

// ---- the compiler stand-in (route A) ----

function compilerWrapper(args) {
  const real = process.env.FAST_REAL_TSUZURI;
  const others = ["check", "doc", "fmt", "test", "bench", "lsp", "repl", "script", "new", "fetch", "publish", "api-diff", "bindgen", "toolchain"];
  const action = ["build", "run"].includes(args[0]) ? args[0]
    : args[0] && !args[0].startsWith("-") && !others.includes(args[0]) ? "build" : undefined;
  const optimized = args.some((arg) => /^-O[123sz]$/.test(arg)) || !args.includes("-O0");
  const eligible = action !== undefined && !optimized && !args.includes("--backend") && !args.some((arg) => REFUSED.some((pattern) => pattern.test(arg)))
    && !args.includes("--version") && !args.includes("--help") && !args.includes("-h")
    && !args.some((arg, index) => arg === "--emit" && !["exe", "object"].includes(args[index + 1]))
    && !args.some((arg) => ["--cpu", "--allocator", "--freestanding", "--trap-mode", "--wasm-feature", "--wasm-host", "--profile"].includes(arg))
    && !args.some((arg, index) => arg === "--target" && args[index + 1] !== "native");
  const forwarded = eligible ? [...args, "--backend", "fast"] : args;
  log({ event: eligible ? "fast-build" : "llvm-build", args: args.join(" ") });
  const result = spawnSync(real, forwarded, { stdio: "inherit" });
  process.exit(result.status ?? 1);
}

function writeWrappers(directory) {
  mkdirSync(directory, { recursive: true });
  const realClang = process.env.TSUZURI_CLANG ?? "clang";
  const environment = [
    `FAST_REAL_CLANG=${JSON.stringify(realClang)}`,
    `FAST_CC=${JSON.stringify(binary("tz-fast-cc", "TSUZURI_FAST_CC"))}`,
    `FAST_REAL_TSUZURI=${JSON.stringify(binary("tsuzuri", "TSUZURI_BIN"))}`,
    `FAST_WORK=${JSON.stringify(directory)}`,
    `FAST_LOG=${JSON.stringify(join(directory, "wrapper.log"))}`,
  ].join(" ");
  writeFileSync(join(directory, "clang"), `#!/bin/sh\nexec env ${environment} node ${JSON.stringify(self)} __clang "$@"\n`);
  writeFileSync(join(directory, "tsuzuri"), `#!/bin/sh\nexec env ${environment} node ${JSON.stringify(self)} __tsuzuri "$@"\n`);
  chmodSync(join(directory, "clang"), 0o755);
  chmodSync(join(directory, "tsuzuri"), 0o755);
  // A suite that looks for the LLVM tools next to TSUZURI_CLANG finds them here.
  const bins = [dirname(realpathSync(spawnSync("which", [realClang], { encoding: "utf8" }).stdout.trim() || realClang)),
    "/opt/homebrew/opt/llvm@21/bin", "/opt/homebrew/opt/llvm/bin", "/usr/local/opt/llvm/bin"];
  for (const bin of bins) {
    if (!existsSync(bin)) continue;
    for (const name of readdirSync(bin)) {
      if (!/^(llvm-|opt$|llc$|lld|ld\.lld|wasm-ld|clang-)/.test(name) && name !== "clang++") continue;
      const link = join(directory, name);
      if (!existsSync(link)) symlinkSync(join(bin, name), link);
    }
  }
  return directory;
}

function summarize(logFile) {
  const counts = { translated: 0, fallback: 0, passthrough: 0, "fast-build": 0, "llvm-build": 0 };
  const reasons = new Map();
  if (!existsSync(logFile)) return { counts, reasons };
  for (const line of readFileSync(logFile, "utf8").split("\n")) {
    if (!line) continue;
    const event = JSON.parse(line);
    counts[event.event] = (counts[event.event] ?? 0) + 1;
    if (event.event === "fallback") reasons.set(event.reason, (reasons.get(event.reason) ?? 0) + 1);
  }
  return { counts, reasons };
}

function parity(suites) {
  const directory = join(root, "target", "fast-parity");
  rmSync(directory, { recursive: true, force: true });
  writeWrappers(directory);
  let failed = 0;
  for (const suite of suites) {
    const logFile = join(directory, "wrapper.log");
    rmSync(logFile, { force: true });
    const started = Date.now();
    const result = spawnSync("node", [join(root, "tests", `${suite}.mjs`), join(directory, "tsuzuri")], {
      cwd: root, stdio: ["ignore", "pipe", "pipe"], encoding: "utf8", maxBuffer: 1 << 28,
      env: { ...process.env, TSUZURI_CLANG: join(directory, "clang") },
    });
    const { counts, reasons } = summarize(logFile);
    console.log(`${suite}: ${result.status === 0 ? "pass" : "FAIL"} ${((Date.now() - started) / 1000).toFixed(1)}s translated=${counts.translated} fallback=${counts.fallback} fast-build=${counts["fast-build"]} llvm-build=${counts["llvm-build"]}`);
    for (const [reason, count] of reasons) console.log(`  fallback x${count}: ${reason}`);
    if (result.status !== 0) {
      failed++;
      console.log(`${result.stdout.split("\n").slice(-30).join("\n")}\n${result.stderr.split("\n").slice(-30).join("\n")}`);
    }
  }
  process.exit(failed === 0 ? 0 : 1);
}

const [mode, ...rest] = process.argv.slice(2);
switch (mode) {
  case "__clang": clangWrapper(rest); break;
  case "__tsuzuri": compilerWrapper(rest); break;
  case "wrapper": console.log(writeWrappers(resolve(rest[0] ?? join(root, "target", "fast-wrapper")))); break;
  case "parity": parity(rest); break;
  default: assert.fail(`unknown mode ${mode}`);
}
