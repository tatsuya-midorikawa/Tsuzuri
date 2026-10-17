// Checks of the fast backend (`--backend fast`, PB05) and the wrappers that run the existing suites on it.
//
//   node tests/fast_backend.mjs [COMPILER]          the checks of the option table, hello, traps, determinism,
//                                                   the symbols of an object and the cache
//   node tests/fast_backend.mjs programs [COMPILER] builds every project of examples/ and tests/fixtures/ with both
//                                                   backends at -O0, runs them, and compares output and status
//   node tests/fast_backend.mjs inventory [COMPILER] translates the -O0 IR of the same projects and groups the
//                                                   constructs the translator rejects (E2002) by reason
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
  appendFileSync, chmodSync, copyFileSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, realpathSync,
  rmSync, statSync, symlinkSync, writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
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
      const native = "--backend fast supports only the native target; use --backend llvm for";
      rejects([...fast, "--target", "wasm32"], `${native} wasm32`);
      rejects([...fast, "--target", "wasm64"], `${native} wasm64`);
      rejects([...fast, "--target", "x86_64-pc-windows-msvc"], `${native} --target x86_64-pc-windows-msvc`);
      for (const emit of ["llvm", "header", "wgsl", "wgsl-relaxed", "spirv", "spirv-relaxed", "shared", "bindings-js"]) {
        rejects([...fast, "--emit", emit], `--backend fast produces only exe and object outputs; use --backend llvm for --emit ${emit}`);
      }
      const unsupported = (option) => `--backend fast does not support ${option} yet; remove it or use --backend llvm`;
      rejects([...fast, "-g"], unsupported("-g"));
      rejects([...fast, "--cpu", "native"], unsupported("--cpu native"));
      rejects([...fast, "--allocator", "small"], unsupported("--allocator small"));
      rejects([...fast, "--emit", "object", "--allocator", "counting"], unsupported("--allocator counting"));
      rejects([...fast, "--emit", "object", "--allocator", "host", "--freestanding"], unsupported("--freestanding"));
      rejects([...fast, "--emit", "object", "--trap-mode", "return"], unsupported("--trap-mode return"));
      rejects(["build", hello, "--backend", "gcc"], "backend must be 'llvm' or 'fast'");
      rejects(["build", hello, "--backend"], "--backend needs a value");
      rejects(["build", hello, "--backend", "fast", "--backend", "llvm"], "backend specified more than once");
      for (const action of ["check", "doc", "fmt"]) {
        rejects([action, hello, "--backend", "fast"], "--backend applies only to build and run");
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
      const reference = execute(compiler, ["build", directory, "-O0", "-o", llvm]);
      if (reference.status !== 0) {
        // A library has no entry point: what its object exports must be the same.
        const object = join(work, `${label}.llvm.o`);
        if (!/E2004/.test(reference.stderr) || execute(compiler, ["build", directory, "-O0", "--emit", "object", "-o", object]).status !== 0) {
          skipped.push(`${name}: the LLVM build fails`);
          continue;
        }
        const built = execute(compiler, ["build", directory, "-O0", "--backend", "fast", "--emit", "object", "-o", join(work, `${label}.fast.o`)]);
        if (built.status !== 0) {
          differ.push(`${name}: BUILD ${built.stderr.trim().split("\n")[0].slice(0, 300)}`);
        } else if (exported(object).join(" ") !== exported(join(work, `${label}.fast.o`)).join(" ")) {
          differ.push(`${name}: the exported symbols differ`);
        } else {
          libraries++;
        }
        continue;
      }
      const built = execute(compiler, ["build", directory, "-O0", "--backend", "fast", "-o", fast]);
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
  const translator = binary("tz-fast-cc", "TSUZURI_FAST_CC");
  let translated = 0;
  const reasons = new Map();
  const noIr = [];
  try {
    for (const { name, directory } of projects()) {
      const label = name.replace("/", "-");
      const ir = join(work, `${label}.ll`);
      if (execute(compiler, ["build", directory, "--emit", "llvm", "-O0", "-o", ir]).status !== 0) {
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

const [mode, ...rest] = process.argv.slice(2);
switch (mode) {
  case "__clang": clangWrapper(rest); break;
  case "__tsuzuri": compilerWrapper(rest); break;
  case "wrapper": console.log(writeWrappers(resolve(rest[0] ?? join(root, "target", "fast-wrapper")))); break;
  case "parity": parity(rest); break;
  case "programs": programs(rest[0]); break;
  case "inventory": inventory(rest[0]); break;
  case undefined: checks(); break;
  default: checks(mode);
}
