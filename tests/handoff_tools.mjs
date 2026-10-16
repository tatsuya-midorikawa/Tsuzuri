// PB03: the processes that the handoff to Clang starts. A Clang wrapper (a small Rust program, so that it works on
// every host) logs each invocation, can fail one on request, and runs the real Clang.
// Usage: node tests/handoff_tools.mjs [compiler]
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const windows = process.platform === "win32";
const suffix = windows ? ".exe" : "";
const scratch = resolve("target/handoff-tools-tests");
mkdirSync(scratch, { recursive: true });
const directory = mkdtempSync(join(scratch, "run-"));
const cache = join(directory, "cache"), log = join(directory, "clang.log");
const real = process.env.TSUZURI_CLANG ?? "clang";
const base = { ...process.env, TSUZURI_CACHE_DIR: cache, HANDOFF_LOG: log, HANDOFF_CLANG: /[\\/]/.test(real) ? resolve(real) : real,
  TMPDIR: directory, TMP: directory, TEMP: directory };
delete base.HANDOFF_FAIL;

function run(program, args, { env = base, success = true } = {}) {
  const result = spawnSync(program, args, { env, encoding: "utf8", timeout: 300_000, maxBuffer: 64 * 1024 * 1024 });
  assert.ifError(result.error);
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}

// One line per Clang invocation, the arguments separated by U+001F. The identity probes are not logged.
const wrapperSource = join(directory, "clang-wrapper.rs"), wrapper = join(directory, `clang-wrapper${suffix}`);
writeFileSync(wrapperSource, `
use std::{env, fs::OpenOptions, io::Write, process::{exit, Command}};
fn main() {
    let args: Vec<_> = env::args_os().skip(1).collect();
    let text: Vec<String> = args.iter().map(|a| a.to_string_lossy().into_owned()).collect();
    if !text.iter().any(|a| a == "--version" || a == "-dM") {
        let mut file = OpenOptions::new().create(true).append(true).open(env::var_os("HANDOFF_LOG").unwrap()).unwrap();
        writeln!(file, "{}", text.join("\\u{1f}")).unwrap();
        if let Ok(pattern) = env::var("HANDOFF_FAIL") {
            if !pattern.is_empty() && text.iter().any(|a| a.contains(&pattern)) {
                eprintln!("injected failure for {pattern}");
                exit(1);
            }
        }
    }
    let status = Command::new(env::var_os("HANDOFF_CLANG").unwrap()).args(&args).status().unwrap();
    exit(status.code().unwrap_or(1));
}`);
run(process.env.RUSTC ?? "rustc", ["--edition=2024", "-O", wrapperSource, "-o", wrapper]);
base.TSUZURI_CLANG = wrapper;

const calls = () => (existsSync(log) ? readFileSync(log, "utf8").split("\n").filter(Boolean).map((line) => line.split("\u001f")) : []);
// The invocations that compile the C file `name` (`threads.c`, `wasi.c`, `stack.c`).
const compilesOf = (since, name) => calls().slice(since).filter((args) => args.some((arg) => arg.split(/[\\/]/).pop() === name)).length;
const entries = (root) => (existsSync(root) ? readdirSync(root).filter((name) => /^[0-9a-f]{64}$/.test(name)) : []);
// Removes the whole-build entries and keeps the runtime objects, as prebuilt_runtime.mjs does.
function missBuilds() {
  for (const name of entries(cache)) rmSync(join(cache, name), { recursive: true, force: true });
}
const same = (a, b, what) => assert.equal(Buffer.compare(readFileSync(a), readFileSync(b)), 0, what);

try {
  // WP2. The C runtimes that a build compiled on every run (threads.c, wasi.c, the guard of a -g executable) are runtime
  // objects in the cache now: compiled once, restored afterwards, and the artifact does not change.
  const cases = [
    { name: "threads.c", unit: "WASM threads", hint: "WASM threads require Clang atomics and bulk-memory support",
      args: [resolve("tests/fixtures/tasks"), "--target", "wasm32", "--wasm-feature", "threads"], extension: ".wasm" },
    { name: "wasi.c", unit: "WASI", hint: "the WASI host needs Clang with WebAssembly support",
      args: [resolve("examples/hello"), "--target", "wasm32", "--wasm-host", "wasi"], extension: ".wasm" },
  ];
  for (const { name, unit, hint, args, extension } of cases) {
    for (const optimization of ["-O0", "-O3"]) {
      const label = `${unit} ${optimization}`;
      const output = (tag) => join(directory, `${name}-${optimization}-${tag}${extension}`);
      let start = calls().length;
      run(compiler, ["build", ...args, optimization, "-o", output("cold")]);
      assert.equal(compilesOf(start, name), 1, `${label}: the first build compiles ${name}`);
      missBuilds();
      start = calls().length;
      run(compiler, ["build", ...args, optimization, "-o", output("warm")]);
      assert.equal(compilesOf(start, name), 0, `${label}: the runtime cache serves ${name}`);
      same(output("cold"), output("warm"), `${label}: a restored object links to the same module`);
      // --no-cache neither reads nor writes any cache and compiles the runtime again.
      const before = readdirSync(join(cache, "runtime")).sort();
      start = calls().length;
      run(compiler, ["build", ...args, optimization, "--no-cache", "-o", output("fresh")]);
      assert.equal(compilesOf(start, name), 1, `${label}: --no-cache compiles ${name}`);
      assert.deepEqual(readdirSync(join(cache, "runtime")).sort(), before, `${label}: --no-cache leaves the cache alone`);
      // The numeric runtime is a separate object only with the cache, so only the modules that embed it either way are
      // byte-identical; the others export the same.
      if (name === "threads.c") same(output("cold"), output("fresh"), `${label}: cached and fresh objects give the same module`);
      else assert.deepEqual(WebAssembly.Module.exports(new WebAssembly.Module(readFileSync(output("cold")))),
        WebAssembly.Module.exports(new WebAssembly.Module(readFileSync(output("fresh")))), `${label}: the same exports`);
    }
    assert.ok(entries(join(cache, "runtime")).length >= 2, `${unit}: one runtime object per optimization level`);
    // A failing compile names what the build needs, cached or not, and stores nothing: the next build compiles it again.
    for (const flags of [[], ["--no-cache"]]) {
      const failing = join(directory, `failing-${name}-${flags.length}${extension}`);
      rmSync(join(cache, "runtime"), { recursive: true, force: true });
      missBuilds();
      const result = run(compiler, ["build", ...args, "-O1", ...flags, "-o", failing], { env: { ...base, HANDOFF_FAIL: name }, success: false });
      assert.notEqual(result.status, 0, `${unit}: the failure is reported`);
      assert.match(result.stderr, /E2002/);
      assert.match(result.stderr, /injected failure/);
      assert.ok(result.stderr.includes(hint), `${unit} ${flags}: ${result.stderr}`);
      assert.ok(!existsSync(failing), `${unit}: no artifact after a failure`);
      const start = calls().length;
      run(compiler, ["build", ...args, "-O1", ...flags, "-o", failing]);
      assert.equal(compilesOf(start, name), 1, `${unit} ${flags}: the failed object was not stored`);
    }
  }

  if (!windows) {
    // The guard of a -g executable that can recurse is its own object, compiled without debug information.
    const depth = "def rec depth :: i64 -> i64\nfn rec depth n = if n == 0 then 0 else depth (n - 1) * 3 + n\n\n";
    const project = join(directory, "deep");
    mkdirSync(project);
    writeFileSync(join(project, "Main.tz"), `${depth}let result = depth 100000000\nresult\n`);
    const shallow = join(directory, "shallow");
    mkdirSync(shallow);
    writeFileSync(join(shallow, "Main.tz"), `${depth}let result = depth 20\nresult\n`);
    const guarded = [];
    for (const [flags, tag] of [[[], "cached"], [["--no-cache"], "fresh"]]) {
      const output = join(directory, `deep-${tag}`);
      rmSync(join(cache, "runtime"), { recursive: true, force: true });
      missBuilds();
      let start = calls().length;
      run(compiler, ["build", project, "-g", "-O0", ...flags, "-o", output]);
      assert.equal(compilesOf(start, "stack.c"), 1, `${tag}: the guard is compiled once`);
      if (tag === "cached") {
        missBuilds();
        start = calls().length;
        run(compiler, ["build", project, "-g", "-O0", "-o", output]);
        assert.equal(compilesOf(start, "stack.c"), 0, "the runtime cache serves the guard");
      }
      const result = run(output, [], { success: false });
      assert.notEqual(result.status, 0);
      assert.match(result.stderr, /trap: stack overflow/, `${tag}: the guard still reports an overflow`);
      guarded.push(result.stderr);
    }
    assert.equal(guarded[0], guarded[1]);
    const out = join(directory, "shallow-run");
    run(compiler, ["build", shallow, "-g", "-O0", "-o", out]);
    assert.ok(run(out, []).stdout.length > 0);
    rmSync(join(cache, "runtime"), { recursive: true, force: true });
    missBuilds();
    const failing = run(compiler, ["build", project, "-g", "-O1", "-o", join(directory, "deep-failing")], { env: { ...base, HANDOFF_FAIL: "stack.c" }, success: false });
    assert.match(failing.stderr, /injected failure/);
    assert.match(failing.stderr, /native runtime requires Clang and the platform C\/OS SDK headers/);
  }
  console.log("handoff tools: C runtime objects ok");

  // WP5 (D6). TSUZURI_LINKER=lld adds -fuse-ld=lld to the final link of a native executable or shared library, never to a
  // compile, a relocatable link or a WebAssembly build. Unset and `default` change nothing, and a bad value fails first.
  const lldFlag = "-fuse-ld=lld";
  const withFlag = (since) => calls().slice(since).filter((args) => args.includes(lldFlag));
  const hello = resolve("examples/hello");
  // Whether the Clang that the build runs finds an LLD, by the rules of that Clang (beside it or on PATH).
  const probe = join(directory, "lld-probe.c");
  writeFileSync(probe, "int main(void) { return 0; }\n");
  const haveLld = !windows && spawnSync(base.HANDOFF_CLANG, [probe, lldFlag, "-o", join(directory, "lld-probe")], { encoding: "utf8" }).status === 0;
  const link = (flags, env, output) => run(compiler, ["build", hello, "-O0", "--no-cache", ...flags, "-o", output], { env, success: false });
  const exe = join(directory, `linker${suffix}`);

  // The wrapper hides the name of the bundled launcher, whose Windows links differ, so a Windows host links nothing here.
  for (const value of windows ? [] : [undefined, "", "default"]) {
    const start = calls().length;
    const result = link([], value === undefined ? base : { ...base, TSUZURI_LINKER: value }, exe);
    assert.equal(result.status, 0, `TSUZURI_LINKER=${value}: ${result.stderr}`);
    assert.ok(calls().length > start);
    assert.equal(withFlag(start).length, 0, `TSUZURI_LINKER=${value} leaves the link to Clang`);
  }
  for (const value of ["gold", "LLD", "lld "]) {
    const start = calls().length;
    const result = link([], { ...base, TSUZURI_LINKER: value }, join(directory, "bad-linker"));
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /E2000/);
    assert.match(result.stderr, /TSUZURI_LINKER must be 'default' or 'lld'/);
    assert.equal(calls().length, start, `TSUZURI_LINKER=${value} fails before any tool runs`);
  }
  if (windows) {
    const start = calls().length;
    const result = link([], { ...base, TSUZURI_LINKER: "lld" }, exe);
    assert.match(result.stderr, /E2000[^]*not supported for Windows/);
    assert.equal(calls().length, start);
  } else {
    // An LLD that fails is reported as Clang's failure with a hint, and nothing is written.
    const failing = join(directory, "linker-failing");
    const result = link([], { ...base, TSUZURI_LINKER: "lld", HANDOFF_FAIL: lldFlag }, failing);
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /E2002/);
    assert.match(result.stderr, /TSUZURI_LINKER=lld needs LLD/);
    assert.ok(!existsSync(failing));
    // One Clang run links, with the flag once; the same run is the compile of an executable.
    const start = calls().length;
    const built = link([], { ...base, TSUZURI_LINKER: "lld" }, exe);
    const flagged = withFlag(start);
    assert.equal(flagged.length, 1, "exactly one invocation carries the flag");
    assert.equal(flagged[0].filter((argument) => argument === lldFlag).length, 1);
    if (haveLld) {
      assert.equal(built.status, 0, built.stderr);
      assert.equal(run(exe, []).stdout.trim(), "5050");
    } else {
      assert.notEqual(built.status, 0, "without LLD Clang cannot link");
      assert.match(built.stderr, /TSUZURI_LINKER=lld needs LLD/);
    }
    // A cached executable of the default linker is not returned for LLD, and the other way round.
    const cached = join(directory, "linker-cached");
    run(compiler, ["build", hello, "-O0", "-o", cached]);
    const before = calls().length;
    const other = run(compiler, ["build", hello, "-O0", "-o", join(directory, "linker-cached-lld")], { env: { ...base, TSUZURI_LINKER: "lld" }, success: false });
    assert.ok(calls().length > before, `a build for LLD is not a hit of the default one: ${other.stderr}`);
    // Objects and relocatable links are never given the flag.
    for (const flags of [["--emit", "object"], ["--emit", "object", "--cpu", "generic"]]) {
      const start = calls().length;
      run(compiler, ["build", resolve("tests/fixtures/tasks"), "-O0", "--no-cache", ...flags, "-o", join(directory, "tasks.o")], { env: { ...base, TSUZURI_LINKER: "lld" } });
      assert.ok(calls().length > start);
      assert.equal(withFlag(start).length, 0, `${flags}: an object is not linked`);
    }
    // WebAssembly is linked by wasm-ld.
    const start2 = calls().length;
    run(compiler, ["build", hello, "--target", "wasm32", "-O0", "--no-cache", "-o", join(directory, "linker.wasm")], { env: { ...base, TSUZURI_LINKER: "lld" } });
    assert.equal(withFlag(start2).length, 0, "WebAssembly");
    if (haveLld && process.platform === "darwin") {
      // The debug executable has its own link, with the sidecar that dsymutil reads.
      const debug = join(directory, "linker-debug");
      const start3 = calls().length;
      run(compiler, ["build", hello, "-O0", "-g", "--no-cache", "-o", debug], { env: { ...base, TSUZURI_LINKER: "lld" } });
      assert.equal(withFlag(start3).length, 1, "-g");
      assert.equal(run(debug, []).stdout.trim(), "5050");
    }
  }
  console.log("handoff tools: linker choice ok");
} finally {
  rmSync(directory, { recursive: true, force: true });
}
