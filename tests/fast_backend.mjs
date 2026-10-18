// Checks of the fast backend (`--backend fast`, PB05) and the wrappers that run the existing suites on it.
//
//   node tests/fast_backend.mjs [COMPILER]          the checks of the option table, hello, traps, determinism,
//                                                   the symbols of an object and the cache
//   node tests/fast_backend.mjs programs [COMPILER] builds every project of examples/ and tests/fixtures/ with both
//                                                   backends at -O0, runs them, and compares output and status
//   node tests/fast_backend.mjs inventory [COMPILER] translates the -O0 IR of the same projects and groups the
//                                                   constructs the translator rejects (E2002) by reason
//   node tests/fast_backend.mjs debug [COMPILER]    builds the same projects with -g on both backends, has
//                                                   llvm-dwarfdump verify the fast one and compares the source
//                                                   lines of each function with the LLVM backend's
//   (FAST_DEBUG=1 makes the programs and inventory modes build with -g too.)
//   node tests/fast_backend.mjs wrapper DIR         writes DIR/clang, a stand-in for TSUZURI_CLANG that
//                                                   translates the -O0 LLVM IR it is asked to compile (route B)
//                                                   and DIR/tsuzuri, a stand-in for the compiler that adds
//                                                   --backend fast to -O0 builds (route A)
//   node tests/fast_backend.mjs parity SUITE[=ARG,..]... runs tests/SUITE.mjs with both wrappers and reports how many
//                                                   compiles were translated, which fell back, and why
//
// The binaries come from `cargo build --release --features fast-backend`: target/release/tsuzuri and
// target/release/tz-fast-cc (override with TSUZURI_BIN and TSUZURI_FAST_CC).
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
  appendFileSync, chmodSync, copyFileSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, realpathSync,
  rmSync, statSync, symlinkSync, writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { Worker } from "node:worker_threads";

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
const REFUSED = [/^-O[123sz]$/, /^-Ofast$/, /^--target/, /^-target$/, /^-fsanitize/, /^-mcpu/, /^-march/,
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
  // A compile for wasm32 (D13) is the translator's too, unless it asks for threads, SIMD or debug information.
  const wasm = args.includes("--target=wasm32-unknown-unknown");
  const refused = args.find((arg) => REFUSED.some((pattern) => pattern.test(arg)) && !(wasm && arg === "--target=wasm32-unknown-unknown"))
    ?? (wasm ? args.find((arg) => ["-matomics", "-msimd128", "-g"].includes(arg)) : undefined);
  if (refused) return passthrough(`option ${refused}`);
  if (args.some((arg) => /^-O[0-9]$/.test(arg) && arg !== "-O0")) return passthrough("optimized");
  const work = join(process.env.FAST_WORK ?? dirname(self), `fast-wrapper-${process.pid}`);
  mkdirSync(work, { recursive: true });
  const rewritten = [];
  const translated = new Map();
  for (const input of inputs) {
    const object = join(work, `${translated.size}-${basename(input, ".ll")}.o`);
    const started = process.hrtime.bigint();
    const result = spawnSync(process.env.FAST_CC, [input, "-o", object, ...(wasm ? ["--target", "wasm32"] : [])], { encoding: "utf8" });
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
  // The build is one that the fast backend takes, whatever its optimization level: a native executable or object,
  // or a wasm32 module or object without SIMD, threads, WASI or debug information.
  const targets = (list) => list.flatMap((arg, index) => (arg === "--target" ? [list[index + 1] === "wasm32-unknown-unknown" ? "wasm32" : list[index + 1]] : []));
  const takes = (list) => action !== undefined && !list.includes("--backend")
    && !list.some((arg) => REFUSED.some((pattern) => pattern.test(arg) && arg !== "--target"))
    && !list.includes("--version") && !list.includes("--help") && !list.includes("-h")
    && !list.some((arg) => ["--cpu", "--wasm-feature", "--wasm-host", "--wasm-opt", "--profile"].includes(arg))
    && targets(list).every((target) => target === "native" || target === "wasm32")
    && !(targets(list).includes("wasm32") && list.some((arg) => ["-g", "--debug-info"].includes(arg)))
    && !list.some((arg, index) => arg === "--emit" && !(targets(list).includes("wasm32") ? ["wasm", "object"] : ["exe", "object"]).includes(list[index + 1]));
  const optimized = args.some((arg) => /^-O[123sz]$/.test(arg)) || !args.includes("-O0");
  const eligible = !optimized && takes(args);
  // FAST_FORCE=1 also runs the optimized builds of a suite on the fast backend, at -O0, to see that its
  // programs still do what the suite expects. A suite that asserts on the optimized code then fails.
  const without = args.filter((arg) => !/^-O([0-3sz]|fast)$/.test(arg));
  const forced = !eligible && Boolean(process.env.FAST_FORCE) && takes(without);
  // The option goes before a `--` that ends the options.
  const withBackend = (list, added) => {
    const separator = list.indexOf("--");
    const options = [...added, "--backend", "fast"];
    return separator < 0 ? [...list, ...options] : [...list.slice(0, separator), ...options, ...list.slice(separator)];
  };
  const forwarded = eligible ? withBackend(args, []) : forced ? withBackend(without, ["-O0"]) : args;
  log({ event: eligible || forced ? "fast-build" : "llvm-build", args: args.join(" ") });
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
  for (const entry of suites) {
    // `suite=arg,arg` passes those arguments instead of the compiler, to a suite that builds with Clang alone.
    const [suite, given] = entry.split("=");
    const logFile = join(directory, "wrapper.log");
    rmSync(logFile, { force: true });
    const started = Date.now();
    const result = spawnSync("node", [join(root, "tests", `${suite}.mjs`), ...(given === undefined ? [join(directory, "tsuzuri")] : given.split(","))], {
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

// ---- the checks of the default mode ----

function session(compilerArgument) {
  const compiler = resolve(compilerArgument ?? binary("tsuzuri", "TSUZURI_BIN"));
  const work = mkdtempSync(join(tmpdir(), "tsuzuri-fast-backend-"));
  const cache = join(work, "cache");
  const execute = (program, args, { cwd = root, env = {}, timeout = 300_000 } = {}) => {
    const result = spawnSync(program, args, {
      cwd, encoding: "utf8", timeout, maxBuffer: 1 << 26, stdio: ["ignore", "pipe", "pipe"],
      env: { ...process.env, TSUZURI_CACHE_DIR: cache, ...env },
    });
    if (result.error && result.error.code !== "ETIMEDOUT") throw result.error;
    return result;
  };
  const cli = (args, options) => {
    const result = execute(compiler, args, options);
    assert.equal(result.status, 0, `${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
    return result;
  };
  const entries = () => (existsSync(cache) ? readdirSync(cache) : []).filter((name) => /^[0-9a-f]{64}$/.test(name));
  return { compiler, work, cache, execute, cli, entries };
}

// What a run of a program shows to its user.
const observe = ({ status, signal, stdout, stderr }) => ({ status, signal, stdout, stderr });

function checks(compilerArgument) {
  const { compiler, work, execute, cli, entries } = session(compilerArgument);
  const hello = join(root, "examples", "hello");
  const functional = join(root, "examples", "functional");
  const rejects = (args, message) => {
    const result = execute(compiler, args);
    assert.equal(result.status, 2, `${args.join(" ")}\n${result.stderr}`);
    assert.match(result.stderr, /error\[E2000\]/, args.join(" "));
    assert.ok(result.stderr.includes(message), `${args.join(" ")}\n${result.stderr}`);
  };
  const step = (name, body) => {
    body();
    console.log(`ok ${name}`);
  };
  try {
    step("the option table", () => {
      const fast = ["build", hello, "--backend", "fast", "-o", join(work, "x")];
      for (const flag of ["-O1", "-O2", "-O3", "-Os", "-Oz"]) {
        rejects([...fast, flag], `--backend fast supports only -O0; remove ${flag} or use --backend llvm`);
      }
      const native = "--backend fast supports only the native target and wasm32; use --backend llvm for";
      rejects([...fast, "--target", "wasm64"], `${native} wasm64`);
      rejects([...fast, "--target", "x86_64-pc-windows-msvc"], `${native} --target x86_64-pc-windows-msvc`);
      for (const emit of ["llvm", "header", "wgsl", "wgsl-relaxed", "spirv", "spirv-relaxed", "shared", "bitcode", "bindings-js"]) {
        rejects([...fast, "--emit", emit], `--backend fast produces only exe and object outputs; use --backend llvm for --emit ${emit}`);
      }
      // wasm32 modules and objects are translated too (D13); what changes the code or the link beyond that is refused.
      const wasm = [...fast, "--target", "wasm32"];
      const wasmOption = (option) => `--backend fast does not support ${option} for wasm32; remove it or use --backend llvm`;
      rejects([...wasm, "--emit", "llvm"], "--backend fast produces only wasm and object outputs for wasm32; use --backend llvm for --emit llvm");
      rejects([...wasm, "--wasm-feature", "simd128"], wasmOption("--wasm-feature simd128"));
      rejects([...wasm, "--wasm-feature", "threads"], wasmOption("--wasm-feature threads"));
      rejects([...wasm, "--wasm-host", "wasi"], wasmOption("--wasm-host"));
      rejects([...wasm, "--wasm-opt"], wasmOption("--wasm-opt"));
      rejects([...wasm, "-g"], wasmOption("-g"));
      const unsupported = (option) => `--backend fast does not support ${option} yet; remove it or use --backend llvm`;
      rejects([...fast, "--cpu", "native"], unsupported("--cpu native"));
      const profile = "--backend fast cannot instrument or optimize with a profile; use --backend llvm with --profile-generate and --profile-use";
      rejects([...fast, "--profile-generate", join(work, "profiles")], profile);
      rejects([...fast, "--profile-use", join(work, "none.profdata")], profile);
      rejects(["build", hello, "--backend", "gcc"], "backend must be 'llvm' or 'fast'");
      rejects(["build", hello, "--backend"], "--backend needs a value");
      rejects(["build", hello, "--backend", "fast", "--backend", "llvm"], "backend specified more than once");
      for (const action of ["check", "doc", "fmt"]) {
        rejects([action, hello, "--backend", "fast"], "--backend applies only to build, run, and test");
      }
    });

    const probe = execute(compiler, ["build", hello, "--backend", "fast", "-o", join(work, "probe")]);
    if (probe.status !== 0) {
      // A compiler built without the feature, or a host without a translator, says so and nothing else.
      assert.equal(probe.status, 2, probe.stderr);
      assert.match(probe.stderr, /error\[E2000\]: (this tsuzuri was built without the fast backend|--backend fast is not available on)/, probe.stderr);
      console.log("skipped the rest: this compiler has no fast backend for this host");
      return;
    }
    assert.equal(probe.stderr, "", "a fast build prints nothing (no linker warnings)");

    step("hello builds and runs the same", () => {
      const llvm = join(work, "hello.llvm");
      const fast = join(work, "hello.fast");
      cli(["build", hello, "-O0", "-o", llvm]);
      const built = cli(["build", hello, "-O0", "--backend", "fast", "-o", fast]);
      assert.equal(built.stderr, "");
      const expected = observe(execute(llvm, []));
      assert.equal(expected.status, 0, expected.stderr);
      assert.deepEqual(observe(execute(fast, [])), expected);
      assert.ok(statSync(fast).mode & 0o100, "the output is executable");
      const viaRun = cli(["run", hello, "--backend", "fast"]);
      assert.equal(viaRun.stdout, expected.stdout);
    });

    step("a trap reports the same site and ends the same way", () => {
      const project = join(work, "trap");
      mkdirSync(project);
      writeFileSync(join(project, "Main.tz"), `def pick :: [i64] -> i64 -> i64 = \\xs i -> xs[i]

def main :: unit -> i32 = \\() ->
    let xs = [1, 2, 3]
    do! IO.write_line (pick xs 1)
    do! IO.write_line (pick xs 7)
    0
`);
      const programs = {};
      for (const backend of ["llvm", "fast"]) {
        const output = join(work, `trap.${backend}`);
        cli(["build", project, "-O0", "--backend", backend, "--trap-info", "-o", output]);
        programs[backend] = output;
      }
      const expected = observe(execute(programs.llvm, [], { cwd: project }));
      assert.ok(expected.signal !== null || expected.status !== 0, "the program traps");
      assert.match(expected.stderr, /index out of bounds at .*Main\.tz:1:44/);
      assert.equal(expected.stdout, "2\n");
      assert.deepEqual(observe(execute(programs.fast, [], { cwd: project })), expected);
      assert.deepEqual(readFileSync(`${programs.fast}.trap.json`), readFileSync(`${programs.llvm}.trap.json`));
      // `run` reports the same E2005 with the signal and the source location.
      const viaRun = (backend) => observe(execute(compiler, ["run", project, "-O0", "--backend", backend]));
      const reported = viaRun("llvm");
      assert.equal(reported.status, 1);
      assert.match(reported.stderr, /error\[E2005\]: program terminated with signal: 5 \(SIGTRAP\)/);
      assert.deepEqual(viaRun("fast"), reported);
    });

    step("the object does not depend on the number of threads", () => {
      for (const [name, project] of [["hello", hello], ["functional", functional]]) {
        const build = (label, threads) => {
          const object = join(work, `${name}-${label}.o`);
          cli(["build", project, "-O0", "--backend", "fast", "--emit", "object", "--no-cache", "-o", object], { env: { TSUZURI_THREADS: threads } });
          return readFileSync(object);
        };
        const single = build("one", "1");
        assert.ok(single.length > 1000, name);
        assert.ok(single.equals(build("eight", "8")), `${name}: 1 and 8 threads`);
        assert.ok(single.equals(build("again", "1")), `${name}: the same threads twice`);
      }
      // The linked program is the same too.
      const link = (label, threads) => {
        const output = join(work, `hello-${label}`);
        cli(["build", hello, "-O0", "--backend", "fast", "--no-cache", "-o", output], { env: { TSUZURI_THREADS: threads } });
        return readFileSync(output);
      };
      assert.ok(link("exe-1", "1").equals(link("exe-8", "8")), "the executable depends on the threads");
    });

    step("the object exports the symbols of the LLVM object", () => {
      const names = (backend) => {
        const object = join(work, `symbols-${backend}.o`);
        cli(["build", functional, "-O0", "--backend", backend, "--emit", "object", "-o", object]);
        const listed = execute("nm", ["-g", "-U", object]);
        assert.equal(listed.status, 0, listed.stderr);
        return listed.stdout.split("\n").map((line) => line.trim().split(/\s+/)).filter((fields) => fields.length >= 2)
          .map((fields) => fields.at(-1)).sort();
      };
      const expected = names("llvm");
      assert.ok(expected.length >= 5, expected.join(" "));
      assert.deepEqual(names("fast"), expected);
    });

    step("the allocators, freestanding objects and trap-mode return build what the LLVM backend builds", () => {
      const exports = (args) => {
        const object = join(work, `options-${args.join("")}.o`);
        cli(["build", join(root, "tests", "fixtures", "host_abi"), "-O0", ...args, "--emit", "object", "-o", object]);
        const listed = execute("nm", ["-g", "-U", object]);
        assert.equal(listed.status, 0, listed.stderr);
        return listed.stdout.split("\n").map((line) => line.trim().split(/\s+/)).filter((fields) => fields.length >= 2)
          .map((fields) => fields.at(-1)).sort();
      };
      for (const options of [["--allocator", "small"], ["--allocator", "counting"], ["--freestanding", "--allocator", "host"], ["--trap-mode", "return"]]) {
        const expected = exports(options);
        assert.ok(expected.length >= 5, options.join(" "));
        assert.deepEqual(exports([...options, "--backend", "fast"]), expected, options.join(" "));
      }
      const llvm = join(work, "small.llvm");
      const fast = join(work, "small.fast");
      cli(["build", hello, "-O0", "--allocator", "small", "-o", llvm]);
      cli(["build", hello, "-O0", "--allocator", "small", "--backend", "fast", "-o", fast]);
      assert.deepEqual(observe(execute(fast, [], { cwd: hello })), observe(execute(llvm, [], { cwd: hello })));
    });

    step("the cache keeps the backends apart", () => {
      // A program of its own, so that no earlier step has put its key in the cache.
      const project = join(work, "cache-project");
      mkdirSync(project);
      writeFileSync(join(project, "Main.tz"), `def rec sum :: i64 -> i64 -> i64 = \\n total ->
    if n <= 0 {
        total
    } else {
        sum (n - 1) (total + n)
    }

def main :: unit -> i32 = \\() ->
    do! IO.write_line (sum 99 0)
    0
`);
      const build = (backend, label) => {
        const output = join(work, `cache-${label}`);
        cli(["build", project, "-O0", ...(backend ? ["--backend", backend] : []), "-o", output]);
        return readFileSync(output);
      };
      const before = entries().length;
      const llvm = build(undefined, "llvm-1");
      assert.equal(entries().length, before + 1);
      const fast = build("fast", "fast-1");
      assert.equal(entries().length, before + 2, "fast is a different key");
      assert.ok(!fast.equals(llvm), "the fast build is not the LLVM artifact");
      assert.ok(build("llvm", "llvm-2").equals(llvm), "--backend llvm is the default key");
      assert.ok(build("fast", "fast-2").equals(fast), "a second fast build hits the cache");
      assert.equal(entries().length, before + 2);
      assert.equal(execute(join(work, "cache-fast-2"), []).stdout, "4950\n");
    });

    step("wasm32 modules and objects: the same behavior, imports and exports, whatever the threads", () => {
      const output = (backend, label, extra = [], env = {}) => {
        const file = join(work, `wasm-${label}.wasm`);
        cli(["build", hello, "--target", "wasm32", "-O0", ...(backend ? ["--backend", backend] : []), ...extra, "-o", file], { env });
        return file;
      };
      const behavior = (file) => {
        const result = spawnSync(process.execPath, [self, "__wasmrun", file], { encoding: "utf8", timeout: 60_000 });
        assert.equal(result.status, 0, result.stderr);
        return result.stdout;
      };
      const llvm = output(undefined, "llvm");
      const fast = output("fast", "fast");
      assert.equal(behavior(fast), behavior(llvm));
      assert.ok(behavior(fast).includes("write 1 5050"), behavior(fast));
      // The same module whatever the number of threads, with the runtime in the IR or as objects.
      for (const extra of [[], ["--no-cache"], ["--emit", "object"]]) {
        const one = output("fast", `one${extra.join("")}`, extra, { TSUZURI_THREADS: "1" });
        const eight = output("fast", `eight${extra.join("")}`, extra, { TSUZURI_THREADS: "8" });
        assert.ok(readFileSync(one).equals(readFileSync(eight)), `${extra.join(" ")}: 1 and 8 threads`);
      }
      // The cache keeps the backends apart for wasm32 too.
      assert.ok(!readFileSync(fast).equals(readFileSync(llvm)), "the fast module is not the LLVM artifact");
      assert.ok(readFileSync(output("fast", "fast-again")).equals(readFileSync(fast)), "a second fast build");
      assert.ok(readFileSync(output("llvm", "llvm-again")).equals(readFileSync(llvm)), "--backend llvm is the default");
    });

    step("tsuzuri test reports the same on both backends", () => {
      for (const fixture of ["property", "coverage"]) {
        const directory = join(root, "tests", "fixtures", fixture);
        const run = (backend) => observe(execute(compiler, ["test", directory, "-O0", "--backend", backend]));
        const expected = run("llvm");
        assert.match(expected.stdout, /^(not )?ok 1 - /m, fixture);
        assert.deepEqual(run("fast"), expected, fixture);
      }
      const directory = join(root, "tests", "fixtures", "coverage");
      const lcov = (backend) => {
        const file = join(work, `coverage-${backend}.lcov`);
        execute(compiler, ["test", directory, "-O0", "--backend", backend, "--coverage", file]);
        return readFileSync(file);
      };
      assert.ok(lcov("llvm").equals(lcov("fast")), "the coverage report depends on the backend");
    });

    step("-g gives the functions of the fast backend the lines of the LLVM backend", () => {
      const tool = dwarfdumpTool();
      for (const project of [hello, functional]) {
        const llvm = join(work, "debug-llvm");
        const fast = join(work, "debug-fast");
        cli(["build", project, "-O0", "-g", "-o", llvm]);
        cli(["build", project, "-O0", "-g", "--backend", "fast", "-o", fast]);
        assert.deepEqual(observe(execute(fast, [], { cwd: project })), observe(execute(llvm, [], { cwd: project })));
        if (!tool) continue;
        const verified = execute(tool, ["--verify", debugFile(fast)]);
        assert.match(verified.stdout, /No errors\./, verified.stdout + verified.stderr);
        const expected = linesByFunction(tool, debugFile(llvm));
        const actual = linesByFunction(tool, debugFile(fast));
        assert.ok(actual.has("Main.main"), "the fast build has no Main.main");
        for (const [name, lines] of expected) {
          // Only what an entry or an export reaches is translated.
          if (!actual.has(name)) continue;
          for (const line of lines) assert.ok(actual.get(name).has(line), `${name} lacks ${line}`);
        }
      }
      // A build without -g has no debug sections at all.
      const plain = join(work, "plain.o");
      cli(["build", hello, "-O0", "--backend", "fast", "--emit", "object", "-o", plain]);
      assert.ok(!readFileSync(plain).includes("debug_line"), "an object without -g has a line table");
    });

    step("without the flag the LLVM output is unchanged", () => {
      for (const level of ["-O0", "-O3"]) {
        const plain = join(work, `plain${level}.ll`);
        const explicit = join(work, `explicit${level}.ll`);
        cli(["build", hello, level, "--emit", "llvm", "--no-cache", "-o", plain]);
        cli(["build", hello, level, "--emit", "llvm", "--backend", "llvm", "--no-cache", "-o", explicit]);
        assert.ok(readFileSync(plain).equals(readFileSync(explicit)), level);
        if (process.env.TSUZURI_BASELINE) {
          const baseline = join(work, `baseline${level}.ll`);
          const result = spawnSync(process.env.TSUZURI_BASELINE, ["build", hello, level, "--emit", "llvm", "--no-cache", "-o", baseline], { encoding: "utf8" });
          assert.equal(result.status, 0, result.stderr);
          assert.ok(readFileSync(plain).equals(readFileSync(baseline)), `${level} differs from TSUZURI_BASELINE`);
        }
      }
    });
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
}

// ---- every project of examples/ and tests/fixtures/ ----

function projects() {
  const found = [];
  for (const [kind, parent] of [["examples", "examples"], ["fixtures", join("tests", "fixtures")]]) {
    for (const name of readdirSync(join(root, parent)).sort()) {
      const directory = join(root, parent, name);
      if (statSync(directory).isDirectory()) found.push({ name: `${kind}/${name}`, directory });
    }
  }
  return found;
}

function programs(compilerArgument) {
  const { compiler, work, execute } = session(compilerArgument);
  // FAST_DEBUG=1 builds every project with -g, so that the line tables are made and the programs still agree.
  const debug = process.env.FAST_DEBUG ? ["-g"] : [];
  // The software half-precision helpers are exported by an object that uses `f16` under the soft
  // path, which the fast backend always takes; the LLVM backend takes it only where the host has no
  // half-precision hardware.
  const exported = (object) => execute("nm", ["-g", "-U", object]).stdout.split("\n")
    .map((line) => line.trim().split(/\s+/).at(-1)).filter((name) => name && !/^_?tz_soft_/.test(name)).sort();
  let same = 0;
  let libraries = 0;
  const differ = [];
  const skipped = [];
  try {
    for (const { name, directory } of projects()) {
      const label = name.replace("/", "-");
      const llvm = join(work, `${label}.llvm`);
      const fast = join(work, `${label}.fast`);
      const reference = execute(compiler, ["build", directory, "-O0", ...debug, "-o", llvm]);
      if (reference.status !== 0) {
        // A library has no entry point: what its object exports must be the same.
        const object = join(work, `${label}.llvm.o`);
        if (!/E2004/.test(reference.stderr) || execute(compiler, ["build", directory, "-O0", ...debug, "--emit", "object", "-o", object]).status !== 0) {
          skipped.push(`${name}: the LLVM build fails`);
          continue;
        }
        const built = execute(compiler, ["build", directory, "-O0", ...debug, "--backend", "fast", "--emit", "object", "-o", join(work, `${label}.fast.o`)]);
        if (built.status !== 0) {
          differ.push(`${name}: BUILD ${built.stderr.trim().split("\n")[0].slice(0, 300)}`);
        } else if (exported(object).join(" ") !== exported(join(work, `${label}.fast.o`)).join(" ")) {
          differ.push(`${name}: the exported symbols differ`);
        } else {
          libraries++;
        }
        continue;
      }
      const built = execute(compiler, ["build", directory, "-O0", ...debug, "--backend", "fast", "-o", fast]);
      if (built.status !== 0) {
        differ.push(`${name}: BUILD ${built.stderr.trim().split("\n")[0].slice(0, 300)}`);
        continue;
      }
      const expected = execute(llvm, [], { cwd: directory, timeout: 20_000 });
      if (expected.error) {
        skipped.push(`${name}: the program does not end within 20 s`);
        continue;
      }
      const actual = execute(fast, [], { cwd: directory, timeout: 20_000 });
      try {
        assert.deepEqual(observe(actual), observe(expected));
        same++;
      } catch {
        differ.push(`${name}: llvm status ${expected.status ?? expected.signal}, fast status ${actual.status ?? actual.signal}`);
      }
    }
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
  console.log(`same-output=${same} same-exports=${libraries} differ=${differ.length} skipped=${skipped.length}`);
  for (const line of differ) console.log(`DIFFER ${line}`);
  for (const line of skipped) console.log(`skipped ${line}`);
  process.exit(differ.length === 0 ? 0 : 1);
}

// ---- what the translator rejects ----

function inventory(compilerArgument) {
  const { compiler, work, execute } = session(compilerArgument);
  const debug = process.env.FAST_DEBUG ? ["-g"] : [];
  const translator = binary("tz-fast-cc", "TSUZURI_FAST_CC");
  let translated = 0;
  const reasons = new Map();
  const noIr = [];
  try {
    for (const { name, directory } of projects()) {
      const label = name.replace("/", "-");
      const ir = join(work, `${label}.ll`);
      if (execute(compiler, ["build", directory, "--emit", "llvm", "-O0", ...debug, "-o", ir]).status !== 0) {
        noIr.push(name);
        continue;
      }
      const result = execute(translator, [ir, "-o", join(work, `${label}.o`)]);
      if (result.status === 0) {
        translated++;
        continue;
      }
      const reason = result.stderr.trim().split("\n")[0].slice(0, 200);
      reasons.set(reason, [...(reasons.get(reason) ?? []), name]);
    }
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
  const rejected = [...reasons.values()].reduce((sum, names) => sum + names.length, 0);
  console.log(`translated=${translated} rejected=${rejected} without-ir=${noIr.length}`);
  for (const [reason, names] of reasons) console.log(`  x${names.length} ${reason}\n    ${names.join(" ")}`);
  // The LLVM IR of a host with the hardware half-precision path has `half`, which the driver
  // replaces by the software path whenever it translates; every other rejection is a gap.
  const gaps = [...reasons].filter(([reason]) => !/\bhalf\b/.test(reason));
  process.exit(gaps.length === 0 ? 0 : 1);
}

// ---- line tables (-g) ----

// llvm-dwarfdump: TSUZURI_DWARFDUMP, the PATH, the Xcode tools or Homebrew's LLVM.
function dwarfdumpTool() {
  const candidates = [process.env.TSUZURI_DWARFDUMP, "llvm-dwarfdump", "/Library/Developer/CommandLineTools/usr/bin/llvm-dwarfdump",
    "/opt/homebrew/opt/llvm/bin/llvm-dwarfdump", "/opt/homebrew/opt/llvm@21/bin/llvm-dwarfdump", "/usr/local/opt/llvm/bin/llvm-dwarfdump"];
  return candidates.find((tool) => tool && spawnSync(tool, ["--version"], { stdio: "ignore" }).status === 0);
}

// The file that holds the DWARF of a `-g` build: macOS keeps it beside the executable, in a flat dSYM.
const debugFile = (output) => (process.platform === "darwin" ? `${output}.dwarf` : output);

// The source lines of every function in a debug file, as { function name -> Set of "file:line" }, with the
// rows and the subprograms read from llvm-dwarfdump's text. Line 0 is "no line" and an end of sequence is no code.
function linesByFunction(tool, file) {
  const run = (flag) => {
    const result = spawnSync(tool, [flag, file], { encoding: "utf8", maxBuffer: 1 << 28 });
    assert.equal(result.status, 0, `${tool} ${flag} ${file}\n${result.stderr}`);
    return result.stdout;
  };
  const functions = [];
  let current;
  for (const line of run("--debug-info").split("\n")) {
    if (/DW_TAG_/.test(line)) {
      if (current?.name !== undefined && current.low !== undefined) functions.push(current);
      current = /DW_TAG_subprogram/.test(line) ? {} : undefined;
      continue;
    }
    if (!current) continue;
    let found;
    if ((found = /DW_AT_name\s+\("(.*)"\)/.exec(line))) current.name = found[1];
    else if ((found = /DW_AT_low_pc\s+\(0x([0-9a-f]+)\)/.exec(line))) current.low = BigInt(`0x${found[1]}`);
    else if ((found = /DW_AT_high_pc\s+\(0x([0-9a-f]+)\)/.exec(line))) current.high = BigInt(`0x${found[1]}`);
  }
  if (current?.name !== undefined && current.low !== undefined) functions.push(current);
  const directories = new Map();
  const files = new Map();
  const rows = [];
  let lastFile;
  for (const line of run("--debug-line").split("\n")) {
    let found;
    if (/^debug_line\[/.test(line)) {
      directories.clear();
      files.clear();
    } else if ((found = /include_directories\[\s*(\d+)\]\s*=\s*"(.*)"/.exec(line))) directories.set(Number(found[1]), found[2]);
    else if ((found = /file_names\[\s*(\d+)\]:/.exec(line))) lastFile = Number(found[1]);
    else if ((found = /^\s+name:\s+"(.*)"/.exec(line))) files.set(lastFile, { name: found[1] });
    else if ((found = /^\s+dir_index:\s+(\d+)/.exec(line))) files.get(lastFile).directory = Number(found[1]);
    else if ((found = /^0x([0-9a-f]+)\s+(\d+)\s+\d+\s+(\d+)\s+\d+\s+\d+\s+\d+\s*(.*)$/.exec(line))) {
      const file = files.get(Number(found[3]));
      const directory = file && file.directory ? `${directories.get(file.directory)}/` : "";
      rows.push({ address: BigInt(`0x${found[1]}`), line: Number(found[2]), file: file ? directory + file.name : "?", end: /end_sequence/.test(found[4]) });
    }
  }
  const table = new Map();
  for (const { name, low, high } of functions) {
    const lines = table.get(name) ?? new Set();
    for (const row of rows) {
      if (row.end || row.line === 0 || row.address < low || row.address >= (high < low ? low + high : high)) continue;
      lines.add(`${row.file}:${row.line}`);
    }
    table.set(name, lines);
  }
  return table;
}

// `-g` builds of every project with both backends: llvm-dwarfdump accepts the debug file of the fast one, and its
// functions have the lines that the LLVM backend's have.
function debugLines(compilerArgument) {
  const { compiler, work, execute } = session(compilerArgument);
  const tool = dwarfdumpTool();
  if (!tool) {
    console.log("skipped: llvm-dwarfdump is not available (set TSUZURI_DWARFDUMP)");
    return;
  }
  let projectsChecked = 0;
  let functionsChecked = 0;
  let identical = 0;
  let extras = 0;
  let unreached = 0;
  const problems = [];
  const different = [];
  try {
    for (const { name, directory } of projects()) {
      const label = name.replace("/", "-");
      const llvm = join(work, `${label}.llvm`);
      const fast = join(work, `${label}.fast`);
      const reference = execute(compiler, ["build", directory, "-O0", "-g", "-o", llvm]);
      if (reference.status !== 0) continue;
      const built = execute(compiler, ["build", directory, "-O0", "-g", "--backend", "fast", "-o", fast]);
      if (built.status !== 0) {
        problems.push(`${name}: BUILD ${built.stderr.trim().split("\n")[0].slice(0, 300)}`);
        continue;
      }
      const verified = execute(tool, ["--verify", debugFile(fast)]);
      if (verified.status !== 0 || !/No errors\./.test(verified.stdout)) {
        problems.push(`${name}: llvm-dwarfdump --verify: ${(verified.stdout + verified.stderr).trim().split("\n").slice(-3).join(" | ").slice(0, 300)}`);
        continue;
      }
      projectsChecked++;
      const expected = linesByFunction(tool, debugFile(llvm));
      const actual = linesByFunction(tool, debugFile(fast));
      for (const [functionName, lines] of expected) {
        // The fast backend translates only what an entry or an export reaches; the LLVM backend's -O0 object keeps
        // the other functions too.
        if (!actual.has(functionName)) {
          unreached++;
          continue;
        }
        functionsChecked++;
        const got = actual.get(functionName);
        const missing = [...lines].filter((line) => !got.has(line));
        const extra = [...got].filter((line) => !lines.has(line));
        // A line that only the fast backend has comes from code that Clang's FastISel folds into its user, such as the
        // address of a constant.
        if (extra.length > 0) extras++;
        if (missing.length === 0) identical++;
        else different.push(`${name}: ${functionName} missing ${missing.slice(0, 4).join(",")}`);
      }
    }
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
  console.log(`projects=${projectsChecked} functions=${functionsChecked} with-every-line=${identical} missing-lines=${different.length} with-extra-lines=${extras} unreached=${unreached} problems=${problems.length}`);
  for (const line of problems) console.log(`PROBLEM ${line}`);
  for (const line of different.slice(0, 40)) console.log(`MISSING ${line}`);
  process.exit(problems.length === 0 && different.length === 0 ? 0 : 1);
}

// ---- WebAssembly (wasm32) ----

// The body of a worker: a fresh instance of the module with a host that records what the program writes, then
// every attempt at calling one export. A hung export is cut off by the parent, not here.
function wasmWorker() {
  const { workerData, parentPort } = require("node:worker_threads");
  const { readFileSync } = require("node:fs");
  const module = new WebAssembly.Module(readFileSync(workerData.file));
  const log = [];
  let memory;
  let stub = false;
  const imports = {};
  for (const { module: from, name, kind } of WebAssembly.Module.imports(module)) {
    imports[from] ??= {};
    if (kind !== "function") continue;
    imports[from][name] = from === "tsuzuri_io" && name === "write"
      ? (fd, pointer, length) => {
        log.push(`write ${fd} ${Buffer.from(memory.buffer, pointer, Number(length)).toString("latin1")}`);
        return 0;
      }
      : () => {
        stub = true;
        return 0;
      };
  }
  const outcome = (call) => {
    try {
      const value = call();
      return typeof value === "bigint" ? `${value}n` : String(value);
    } catch (error) {
      const text = `${error.constructor.name}: ${String(error.message).split("\n")[0]}`;
      // A runaway recursion ends in whichever limit comes first: V8's stack, whose frames are larger in the fast
      // backend's code, or the shadow stack, which a wasm-ld layout puts at the bottom of the memory.
      return /Maximum call stack size exceeded|memory access out of bounds/.test(text) ? "stack exhausted" : text;
    }
  };
  let instance;
  try {
    instance = new WebAssembly.Instance(module, imports);
  } catch (error) {
    parentPort.postMessage({ instantiate: String(error.message).split("\n")[0] });
    return;
  }
  memory = instance.exports.memory;
  const value = instance.exports[workerData.name];
  // The parameter types are not visible from JavaScript: try BigInts, then numbers, with the same inputs for both modules.
  const attempts = [];
  for (const make of [(n) => BigInt(n), (n) => n]) {
    for (const seed of [0, 1, 7, -3]) {
      attempts.push(outcome(() => value(...Array.from({ length: value.length }, (_, index) => make(seed + index)))));
    }
  }
  parentPort.postMessage({ attempts, log, stub });
}

// `node tests/fast_backend.mjs __wasmrun FILE.wasm`: calls every exported function (the entry first, the rest by
// name) in a fresh instance with a few inputs, and prints what happened as JSON. The two backends' modules must
// print the same.
async function wasmRun(file) {
  const module = new WebAssembly.Module(readFileSync(file));
  const names = WebAssembly.Module.exports(module)
    .filter(({ kind, name }) => kind === "function" && !/^(tsuzuri_(alloc|free|thread|async)|__)/.test(name))
    .map(({ name }) => name).sort()
    .sort((a, b) => Number(b === "tsuzuri_main") - Number(a === "tsuzuri_main"));
  const results = {};
  let stub = false;
  for (const name of names) {
    const worker = new Worker(`(${wasmWorker})()`, { eval: true, workerData: { file, name } });
    results[name] = await new Promise((done) => {
      const timer = setTimeout(() => done({ timeout: true }), 5_000);
      worker.once("message", (message) => {
        clearTimeout(timer);
        done(message);
      });
      worker.once("error", (error) => {
        clearTimeout(timer);
        done({ error: String(error.message).split("\n")[0] });
      });
    });
    await worker.terminate();
    stub ||= Boolean(results[name].stub);
    delete results[name].stub;
  }
  console.log(JSON.stringify({ results, stub }));
}

function wasmPrograms(compilerArgument) {
  const { compiler, work, execute } = session(compilerArgument);
  // What differs first between the two runs' JSON: the output, or the results of an export.
  const firstDifference = (expected, actual) => {
    try {
      const [a, b] = [JSON.parse(expected), JSON.parse(actual)];
      for (const key of new Set([...Object.keys(a.results ?? {}), ...Object.keys(b.results ?? {})])) {
        if (JSON.stringify(a.results?.[key]) !== JSON.stringify(b.results?.[key])) {
          return `${key}: llvm ${JSON.stringify(a.results?.[key]).slice(0, 200)} fast ${JSON.stringify(b.results?.[key]).slice(0, 200)}`;
        }
      }
      return `llvm ${expected.slice(0, 100)} fast ${actual.slice(0, 100)}`;
    } catch {
      return `llvm ${expected.slice(0, 120)} | fast ${actual.slice(0, 120)}`;
    }
  };
  const describe = (file) => {
    const result = spawnSync(process.execPath, [self, "__wasmrun", file], { encoding: "utf8", timeout: 120_000, maxBuffer: 1 << 26 });
    return result.status === 0 && result.stdout ? result.stdout.trim() : `run failed: ${result.error?.code ?? result.status} ${result.stderr.trim().split("\n")[0]}`;
  };
  const shape = (file) => spawnSync(process.execPath, ["-e", `
    const m = new WebAssembly.Module(require("fs").readFileSync(process.argv[1]));
    const key = (entry) => JSON.stringify(entry);
    console.log(JSON.stringify([WebAssembly.Module.imports(m).map(key).sort(), WebAssembly.Module.exports(m).map(key).sort()]));`, file], { encoding: "utf8" }).stdout.trim();
  const validate = (file) => spawnSync(process.execPath, ["-e", `process.exit(WebAssembly.validate(require("fs").readFileSync(process.argv[1])) ? 0 : 1)`, file]).status === 0;
  let same = 0;
  let stubbed = 0;
  const differ = [];
  const skipped = [];
  const extra = process.env.FAST_WASM_ARGS ? process.env.FAST_WASM_ARGS.split(" ") : [];
  try {
    for (const { name, directory } of projects()) {
      const label = name.replace("/", "-");
      const llvm = join(work, `${label}.llvm.wasm`);
      const fast = join(work, `${label}.fast.wasm`);
      const reference = execute(compiler, ["build", directory, "--target", "wasm32", "-O0", ...extra, "-o", llvm]);
      if (reference.status !== 0) {
        skipped.push(`${name}: the LLVM build fails`);
        continue;
      }
      const built = execute(compiler, ["build", directory, "--target", "wasm32", "-O0", ...extra, "--backend", "fast", "-o", fast]);
      if (built.status !== 0) {
        differ.push(`${name}: BUILD ${built.stderr.trim().split("\n")[0].slice(0, 300)}`);
        continue;
      }
      if (!validate(fast)) {
        differ.push(`${name}: the module does not validate`);
        continue;
      }
      if (shape(llvm) !== shape(fast)) {
        differ.push(`${name}: the imports or exports differ`);
        continue;
      }
      const expected = describe(llvm);
      const actual = describe(fast);
      if (expected !== actual) {
        differ.push(`${name}: ${firstDifference(expected, actual)}`);
        continue;
      }
      same++;
      if (expected.includes('"stub":true')) stubbed++;
    }
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
  console.log(`same-behavior=${same} (with host imports stubbed: ${stubbed}) differ=${differ.length} skipped=${skipped.length}`);
  for (const line of differ) console.log(`DIFFER ${line}`);
  for (const line of skipped) console.log(`skipped ${line}`);
  process.exit(differ.length === 0 ? 0 : 1);
}

const [mode, ...rest] = process.argv.slice(2);
switch (mode) {
  case "__clang": clangWrapper(rest); break;
  case "__tsuzuri": compilerWrapper(rest); break;
  case "__wasmrun": await wasmRun(rest[0]); break;
  case "wrapper": console.log(writeWrappers(resolve(rest[0] ?? join(root, "target", "fast-wrapper")))); break;
  case "parity": parity(rest); break;
  case "programs": programs(rest[0]); break;
  case "wasm": wasmPrograms(rest[0]); break;
  case "inventory": inventory(rest[0]); break;
  case "debug": debugLines(rest[0]); break;
  case undefined: checks(); break;
  default: checks(mode);
}
