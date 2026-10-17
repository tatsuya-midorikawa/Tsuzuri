// PR07: profile-guided optimization. `--profile-generate DIR` builds a program that writes raw profiles under DIR when it
// exits normally, and `--profile-use PATH` builds with an indexed profile or with the merge of a directory of raw
// profiles. Clang does the work, so the sections that need its profile runtime and an llvm-profdata skip where there is
// none, unless TSUZURI_PGO_STRICT=1 or LLVM_PREFIX (a toolchain that CI installs on purpose) turns a skip into a failure.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { chmodSync, cpSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { artifactFingerprint } from "../benchmarks/metrics.mjs";

const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const fixtures = resolve("tests/fixtures/pgo");
const windows = process.platform === "win32";
const strict = process.env.TSUZURI_PGO_STRICT === "1" || Boolean(process.env.LLVM_PREFIX);
const realClang = process.env.TSUZURI_CLANG ?? "clang";
const root = realpathSync(mkdtempSync(join(tmpdir(), "tsuzuri-pgo-")));
const total = "3699964\n";

const baseEnv = { ...process.env, TSUZURI_CACHE_DIR: join(root, "cache") };
delete baseEnv.LLVM_PROFILE_FILE;

function execute(program, args, { env = baseEnv, status = 0, cwd = root } = {}) {
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 300_000, env, cwd, maxBuffer: 64 * 1024 * 1024 });
  assert.ifError(result.error);
  if (status !== null) assert.equal(result.status, status, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}
const tz = (args, options) => execute(compiler, args, options);

const copies = new Map();
function project(name) {
  if (!copies.has(name)) {
    const target = join(root, `project-${name}`);
    cpSync(join(fixtures, name), target, { recursive: true });
    copies.set(name, target);
  }
  return copies.get(name);
}
const executable = (name) => join(root, name);
const build = (name, args, output, options) => tz(["build", project(name), ...args, "-o", output], options);
const profraws = (directory) => (existsSync(directory) ? readdirSync(directory).filter((file) => file.endsWith(".profraw")).sort() : []);
const files = (directory) => profraws(directory).map((file) => join(directory, file));

function wrapper(path, name, real) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, `#!${process.execPath}
const { appendFileSync } = require("node:fs");
const { spawnSync } = require("node:child_process");
const args = process.argv.slice(2);
if (process.env.PGO_LOG) appendFileSync(process.env.PGO_LOG, JSON.stringify([${JSON.stringify(name)}, ...args]) + "\\n");
if (${JSON.stringify(name)} === "clang" && process.env.PGO_RESOURCE && args[0] === "-print-resource-dir") {
  console.log(process.env.PGO_RESOURCE);
  process.exit(0);
}
const result = spawnSync(${JSON.stringify(real)}, args, { stdio: "inherit" });
process.exitCode = result.status ?? 1;
`);
  chmodSync(path, 0o755);
  return path;
}
const logged = (log) => (existsSync(log) ? readFileSync(log, "utf8").split("\n").filter(Boolean).map((line) => JSON.parse(line)) : []);
const compiles = (log) => logged(log).filter(([name, ...args]) => name === "clang" && args.some((argument) => basename(argument) === "module.ll"));
const merges = (log) => logged(log).filter(([name]) => name === "llvm-profdata");

// The llvm-profdata that the compiler is expected to find, found the same way: the variable, beside Clang, or in Clang's toolchain.
function realProfdata() {
  if (process.env.TSUZURI_LLVM_PROFDATA) return process.env.TSUZURI_LLVM_PROFDATA;
  const located = realClang.includes("/") ? realClang : spawnSync("which", [realClang], { encoding: "utf8" }).stdout.trim();
  const candidates = [];
  if (located) candidates.push(join(dirname(realpathSync(located)), "llvm-profdata"));
  const resource = spawnSync(realClang, ["-print-resource-dir"], { encoding: "utf8" }).stdout?.trim();
  if (resource) candidates.push(join(dirname(dirname(dirname(resource))), "bin", "llvm-profdata"));
  return candidates.find((candidate) => existsSync(candidate)) ?? null;
}

const outcome = { passed: 0, skipped: 0, failed: 0 };
class Skipped extends Error {}
const skip = (reason) => {
  throw new Skipped(reason);
};
function section(name, body) {
  try {
    body();
    outcome.passed += 1;
    console.log(`ok   ${name}`);
  } catch (error) {
    if (error instanceof Skipped && !strict) {
      outcome.skipped += 1;
      console.log(`skip ${name}: ${error.message}`);
    } else {
      outcome.failed += 1;
      console.log(`FAIL ${name}`);
      console.log(error instanceof Skipped ? `  skipped, which strict mode refuses: ${error.message}` : (error.stack ?? String(error)));
    }
  }
}

// The training run that most sections build on: the `branch` project, instrumented and run once.
let training;
function trained() {
  if (!training) {
    const directory = join(root, "training");
    const exe = executable("training-exe");
    const built = build("branch", ["--profile-generate", directory], exe, { status: null });
    if (built.status !== 0) {
      const lines = built.stderr.split("\n").filter((line) => line && !line.startsWith(" "));
      training = { error: lines.slice(0, 2).map((line) => line.replace(/^.*?(error\[)/, "$1")).join(" | ") };
    } else {
      assert.equal(execute(exe, []).stdout, total);
      training = { directory, exe };
    }
  }
  if (training.error) skip(`no instrumented build with this Clang: ${training.error}`);
  return training;
}
function needProfdata() {
  const tool = realProfdata();
  if (!tool) skip("no llvm-profdata beside this Clang or in its toolchain");
  return tool;
}

if (windows) {
  section("Windows rejects the profile options", () => {
    for (const args of [["--profile-generate", join(root, "prof")], ["--profile-use", join(root, "prof")]]) {
      const result = tz(["build", project("branch"), ...args], { status: 2 });
      assert.ok(result.stderr.includes("error[E2000]: --profile-generate and --profile-use are not supported on Windows yet"), result.stderr);
    }
  });
  finish();
}

section("option errors need no tool", () => {
  const env = { ...baseEnv, TSUZURI_CLANG: join(root, "no-such-clang") };
  const cross = process.platform === "linux" && process.arch === "x64" ? "aarch64-unknown-linux-gnu" : "x86_64-unknown-linux-gnu";
  const cases = [
    [["--profile-generate"], "--profile-generate needs a value"],
    [["--profile-generate", ""], "--profile-generate needs a value"],
    [["--profile-use"], "--profile-use needs a value"],
    [["--profile-use", ""], "--profile-use needs a value"],
    [["--profile-generate", "a", "--profile-generate", "b"], "profile generation specified more than once"],
    [["--profile-use", "a", "--profile-use", "b"], "profile use specified more than once"],
    [["--profile-generate", "a", "--profile-use", "b"], "--profile-generate and --profile-use cannot be combined; build once with each"],
    [["--profile-generate", "a", "--target", "wasm32"], "profile-guided optimization is native-only; remove --target wasm32 or the profile option"],
    [["--profile-use", "a", "--target", "wasm64"], "profile-guided optimization is native-only; remove --target wasm64 or the profile option"],
    [["--profile-generate", "a", "--emit", "llvm"], "profile-guided optimization needs --emit exe or object"],
    [["--profile-use", "a", "--emit", "shared"], "profile-guided optimization needs --emit exe or object"],
    [["--profile-generate", "a", "--emit", "object", "--freestanding", "--allocator", "host"], "--profile-generate cannot be combined with --freestanding"],
    [["--profile-generate", join(root, "100%")], "--profile-generate directory must not contain '%'"],
    [["--profile-use", "a", "--target", "x86_64-pc-windows-msvc"], "--profile-generate and --profile-use are not supported on Windows yet"],
    [["--profile-generate", "a", "--target", cross], `--profile-generate and --profile-use apply to the machine that builds; remove --target ${cross}`],
    [["--profile-use", "a", "--target", cross], `--profile-generate and --profile-use apply to the machine that builds; remove --target ${cross}`],
  ];
  for (const [args, message] of cases) {
    const result = tz(["build", project("branch"), ...args], { env, status: 2 });
    assert.ok(result.stderr.includes(`error[E2000]: ${message}`), `${args.join(" ")}\n${result.stderr}`);
  }
  for (const action of ["check", "test", "fmt", "run"]) {
    for (const option of ["--profile-generate", "--profile-use"]) {
      const result = tz([action, project("branch"), option, "prof"], { env, status: 2 });
      assert.ok(result.stderr.includes("error[E2000]: --profile-generate and --profile-use are only valid with build"), `${action} ${option}\n${result.stderr}`);
    }
  }

  // A profile that cannot be read is refused before any tool runs.
  const raw = join(root, "raw.profraw");
  writeFileSync(raw, Buffer.concat([Buffer.from("8172666f72706cff", "hex"), Buffer.alloc(120)]));
  const notes = join(root, "notes.txt");
  writeFileSync(notes, "not a profile\n");
  const empty = join(root, "empty-profiles");
  mkdirSync(empty, { recursive: true });
  const missing = join(root, "missing.profdata");
  const refused = [
    [raw, `error[E2000]: '${raw}' is a raw profile; pass its directory to --profile-use or merge it with llvm-profdata`],
    [notes, `error[E2000]: --profile-use expects an indexed .profdata file or a directory of .profraw files; '${notes}' is neither`],
    [empty, `error[E2000]: --profile-use directory '${empty}' has no .profraw files; run the program built with --profile-generate first`],
    [missing, `error[E2001]: cannot read profile '${missing}'`],
  ];
  for (const [path, message] of refused) {
    const result = tz(["build", project("branch"), "--profile-use", path], { env, status: 1 });
    assert.ok(result.stderr.includes(message), `${path}\n${result.stderr}`);
  }
});

section("the bundled launcher is refused", () => {
  const directory = join(root, "launcher");
  mkdirSync(directory, { recursive: true });
  const marker = join(directory, "ran");
  const launcher = join(directory, "tsuzuri-clang");
  writeFileSync(launcher, `#!/bin/sh\necho ran > "${marker}"\nexit 99\n`);
  chmodSync(launcher, 0o755);
  for (const args of [["--profile-generate", join(directory, "prof")], ["--profile-use", join(directory, "prof")]]) {
    const result = tz(["build", project("branch"), ...args], { env: { ...baseEnv, TSUZURI_CLANG: launcher }, status: 1 });
    assert.ok(result.stderr.includes("error[E2002]: profile options need an LLVM Clang: the bundled Clang launcher (tsuzuri-clang) passes no profile options to LLVM Clang"), result.stderr);
    assert.ok(result.stderr.includes("set TSUZURI_CLANG to an LLVM 21 Clang"), result.stderr);
  }
  assert.ok(!existsSync(marker), "the launcher must not run");
});

section("an instrumented program writes a raw profile when it exits", () => {
  const { directory, exe } = trained();
  const [file] = files(directory);
  assert.equal(files(directory).length, 1);
  assert.ok(["8172666f72706cff", "8152666f72706cff"].includes(readFileSync(file).subarray(0, 8).toString("hex")), "raw profile magic");
  // The same program merges its next run into the same file.
  const before = readFileSync(file).length;
  assert.equal(execute(exe, []).stdout, total);
  assert.deepEqual(files(directory), [file]);
  assert.equal(readFileSync(file).length, before);
  // LLVM_PROFILE_FILE chooses the file at run time, over the directory that the build named.
  const elsewhere = join(root, "elsewhere", "run-%p.profraw");
  assert.equal(execute(exe, [], { env: { ...baseEnv, LLVM_PROFILE_FILE: elsewhere } }).stdout, total);
  assert.equal(profraws(join(root, "elsewhere")).length, 1);
  assert.deepEqual(files(directory), [file]);
  // It prints what the program without instrumentation prints.
  const plain = executable("plain");
  build("branch", [], plain);
  assert.equal(execute(plain, []).stdout, total);
});

section("the options that change a build instrument and use it", () => {
  const { directory } = trained();
  const variants = [["-O0"], ["-O1"], ["-O2"], ["-O3"], ["-Os"], ["-Oz"], ["--cpu", "native"], ["--allocator", "small"], ["--strip"], ["--trap-info"], ["-g"], ["--no-cache"]];
  for (const flags of variants) {
    const name = flags.join("").replaceAll("-", "");
    const profile = join(root, `matrix-profile-${name}`);
    const exe = executable(`matrix-${name}`);
    build("branch", ["--profile-generate", profile, ...flags], exe);
    assert.equal(execute(exe, []).stdout, total, flags.join(" "));
    assert.equal(profraws(profile).length, 1, flags.join(" "));
    // The profile of the default build is a profile of this program too; a flag that changes the IR may leave functions unmatched.
    const used = executable(`matrix-use-${name}`);
    build("branch", ["--profile-use", directory, ...flags], used);
    assert.equal(execute(used, []).stdout, total, flags.join(" "));
  }
});

section("a trapped run writes no profile", () => {
  trained();
  const profile = join(root, "trap-profile");
  const exe = executable("trap");
  build("trap", ["--profile-generate", profile], exe);
  const trapped = execute(exe, [], { status: null });
  assert.ok(trapped.status !== 0, "the run traps");
  assert.deepEqual(profraws(profile), []);
  execute(exe, ["argument"]);
  assert.equal(profraws(profile).length, 1);
});

section("an object takes the profile runtime from the final link", () => {
  trained();
  const profile = join(root, "object-profile");
  const object = join(root, "library-generate.o");
  build("library", ["--emit", "object", "--profile-generate", profile], object);
  const host = join(fixtures, "host.c");
  const exe = executable("library-generate");
  execute(realClang, [host, object, "-o", exe, `-fprofile-generate=${profile}`, "-lm"]);
  assert.equal(execute(exe, []).stdout, total);
  assert.equal(profraws(profile).length, 1);
  if (process.platform === "darwin") {
    const unlinked = execute(realClang, [host, object, "-o", executable("library-unlinked"), "-lm"], { status: null });
    assert.notEqual(unlinked.status, 0);
    assert.ok(unlinked.stderr.includes("__llvm_profile_runtime"), unlinked.stderr);
  }
  needProfdata();
  const used = join(root, "library-use.o");
  build("library", ["--emit", "object", "--profile-use", profile], used);
  const plain = join(root, "library-plain.o");
  build("library", ["--emit", "object"], plain);
  assert.notEqual(artifactFingerprint(used), artifactFingerprint(plain));
  const optimized = executable("library-use");
  execute(realClang, [host, used, "-o", optimized, "-lm"]);
  assert.equal(execute(optimized, []).stdout, total);
});

section("a directory of raw profiles and its merge build the same program", () => {
  const { directory } = trained();
  const profdata = needProfdata();
  const merged = join(root, "merged.profdata");
  execute(profdata, ["merge", "-o", merged, ...files(directory)]);
  const fromDirectory = executable("use-directory");
  const fromFile = executable("use-file");
  build("branch", ["--profile-use", directory, "--no-cache"], fromDirectory);
  build("branch", ["--profile-use", merged, "--no-cache"], fromFile);
  assert.equal(execute(fromDirectory, []).stdout, total);
  assert.equal(execute(fromFile, []).stdout, total);
  assert.equal(artifactFingerprint(fromDirectory), artifactFingerprint(fromFile));
  // The profile is applied: the program is not the one that the same build makes without it.
  const plain = executable("plain-no-cache");
  build("branch", ["--no-cache"], plain);
  assert.notEqual(artifactFingerprint(fromDirectory), artifactFingerprint(plain));
  // The same input gives the same program.
  const again = executable("use-directory-again");
  build("branch", ["--profile-use", directory, "--no-cache"], again);
  assert.equal(artifactFingerprint(again), artifactFingerprint(fromDirectory));
});

section("the cache keys follow the profile", () => {
  const { directory } = trained();
  const profdata = needProfdata();
  const spy = wrapper(join(root, "spy", "clang"), "clang", realClang);
  const log = join(root, "spy.log");
  const env = { ...baseEnv, TSUZURI_CACHE_DIR: join(root, "cache-keys"), TSUZURI_CLANG: spy, PGO_LOG: log };
  const count = () => compiles(log).length;
  const flagOf = (compile, prefix) => compile.find((argument) => argument.startsWith(prefix));

  build("branch", [], executable("keys-plain"), { env });
  assert.equal(count(), 1);
  assert.ok(!compiles(log)[0].some((argument) => argument.startsWith("-fprofile")), "a build without profile options passes none to Clang");

  // A relative directory is made absolute: the program runs wherever it is started.
  const first = executable("keys-generate-1");
  build("branch", ["--profile-generate", "relative-profiles"], first, { env });
  assert.equal(count(), 2);
  const generate = compiles(log)[1];
  assert.equal(flagOf(generate, "-fprofile-generate="), `-fprofile-generate=${join(root, "relative-profiles")}`);
  assert.equal(generate[generate.indexOf("-O3") + 1], flagOf(generate, "-fprofile-generate="), "the flag follows the optimization level");
  build("branch", ["--profile-generate", "relative-profiles"], executable("keys-generate-2"), { env });
  assert.equal(count(), 2, "the same instrumented build is served from the cache");
  assert.equal(artifactFingerprint(executable("keys-generate-2")), artifactFingerprint(first));
  build("branch", ["--profile-generate", join(root, "other-profiles")], executable("keys-generate-3"), { env });
  assert.equal(count(), 3, "another directory is another program");
  assert.equal(flagOf(compiles(log)[2], "-fprofile-generate="), `-fprofile-generate=${join(root, "other-profiles")}`);

  // The key of a profile is its content: the directory and its merge are one profile, and a profile with other counts is another.
  build("branch", ["--profile-use", directory], executable("keys-use-1"), { env });
  assert.equal(count(), 4);
  const use = flagOf(compiles(log)[3], "-fprofile-use=");
  assert.ok(use?.endsWith("profile.profdata"), use);
  assert.ok(!use.includes(directory), "Clang reads the staged copy, which the build owns");
  build("branch", ["--profile-use", directory], executable("keys-use-2"), { env });
  assert.equal(count(), 4);
  const merged = join(root, "keys-merged.profdata");
  execute(profdata, ["merge", "-o", merged, ...files(directory)]);
  build("branch", ["--profile-use", merged], executable("keys-use-3"), { env });
  assert.equal(count(), 4, "a merged copy of the same profile hits the same entry");
  assert.equal(artifactFingerprint(executable("keys-use-3")), artifactFingerprint(executable("keys-use-1")));
  // More runs make other counts, which is another profile and another cache entry.
  const more = join(root, "more-profiles");
  const exe = executable("keys-more-exe");
  build("branch", ["--profile-generate", more], exe);
  for (const name of ["a", "b", "c", "d"]) execute(exe, [], { env: { ...baseEnv, LLVM_PROFILE_FILE: join(more, `${name}.profraw`) } });
  const moreMerged = join(root, "keys-more.profdata");
  execute(profdata, ["merge", "-o", moreMerged, ...files(more)]);
  assert.notDeepEqual(readFileSync(moreMerged), readFileSync(merged));
  build("branch", ["--profile-use", more], executable("keys-use-4"), { env });
  assert.equal(count(), 5, "a profile with other counts is another program");
});

section("a profile of another program is summarized in one warning", () => {
  const { directory } = trained();
  needProfdata();
  // `branch` with one more arm in `classify`: the control flow of that function no longer matches.
  const changed = join(root, "project-changed");
  mkdirSync(changed, { recursive: true });
  writeFileSync(join(changed, "Main.tz"), readFileSync(join(fixtures, "branch", "Main.tz"), "utf8").replace("    | _ -> 100", "    | 2 -> 100\n    | _ -> 1000"));
  const message = /^profile data does not match \d+ function\(s\) in this build; they are optimized without profile data\. Rebuild with --profile-generate and rerun the training workload$/;
  const exe = executable("changed");
  const first = tz(["build", changed, "--profile-use", directory, "-o", exe]);
  const lines = first.stderr.split("\n").filter(Boolean);
  assert.equal(lines.length, 1, first.stderr);
  assert.match(lines[0], message);
  assert.ok(!first.stderr.includes("hash mismatch"), "the per-function lines are summarized");
  assert.equal(execute(exe, []).stdout, total);
  // A cached build replays the warning, as text and as JSON.
  assert.equal(tz(["build", changed, "--profile-use", directory, "-o", exe]).stderr, first.stderr);
  const json = tz(["build", changed, "--profile-use", directory, "-o", exe, "--json"]);
  const warnings = json.stderr.split("\n").filter(Boolean).map((line) => JSON.parse(line));
  assert.equal(warnings.length, 1, json.stderr);
  assert.equal(warnings[0].code, "W2001");
  assert.equal(warnings[0].severity, "warning");
  assert.match(warnings[0].message, message);
  // A profile of the same program is silent.
  assert.equal(tz(["build", project("branch"), "--profile-use", directory, "-o", executable("matched")]).stderr, "");
});

section("a profile that Clang cannot read names the file that the user gave", () => {
  if (basename(realClang).replace(/\.exe$/i, "") === "tsuzuri-clang") skip("TSUZURI_CLANG is the bundled launcher");
  const future = join(root, "future.profdata");
  const bytes = Buffer.alloc(128);
  Buffer.from("ff6c70726f666981", "hex").copy(bytes, 0);
  bytes.writeBigUInt64LE(99n, 8);
  writeFileSync(future, bytes);
  const result = tz(["build", project("branch"), "--profile-use", future, "-o", executable("future")], { status: 1 });
  assert.ok(result.stderr.includes("error[E2002]"), result.stderr);
  assert.ok(result.stderr.includes(`Error in reading profile ${future}: `), result.stderr);
  assert.ok(!result.stderr.includes("profile.profdata"), "the staged copy is not shown");
  assert.ok(result.stderr.includes("the profile must be merged by the llvm-profdata that matches this Clang; regenerate it with --profile-generate"), result.stderr);
});

section("llvm-profdata is found beside Clang or in its toolchain, and never on PATH", () => {
  const { directory } = trained();
  const real = needProfdata();
  const log = join(root, "discovery.log");
  const env = { ...baseEnv, PGO_LOG: log };
  delete env.TSUZURI_LLVM_PROFDATA;
  const use = (extra, options) => tz(["build", project("branch"), "--profile-use", directory, "--no-cache", "-o", executable("discovered"), ...extra], options);

  // The variable is used as it is.
  const named = wrapper(join(root, "named", "llvm-profdata"), "llvm-profdata", real);
  use([], { env: { ...env, TSUZURI_LLVM_PROFDATA: named } });
  assert.equal(merges(log).length, 1);
  const [, command, flag, output, ...inputs] = merges(log)[0];
  assert.deepEqual([command, flag, basename(output)], ["merge", "-o", "profile.profdata"]);
  assert.deepEqual(inputs, files(directory), "the raw profiles are merged in the order of their names");
  const broken = use([], { env: { ...env, TSUZURI_LLVM_PROFDATA: join(root, "no-such-profdata") }, status: 1 });
  assert.ok(broken.stderr.includes("error[E2002]: cannot execute"), broken.stderr);
  assert.ok(broken.stderr.includes("llvm-profdata must come from the same LLVM as the Clang that built the instrumented program; set TSUZURI_LLVM_PROFDATA"), broken.stderr);

  // A decoy on PATH never runs: another LLVM's tool reads this Clang's raw profiles only by chance.
  const decoy = join(root, "decoy");
  mkdirSync(decoy, { recursive: true });
  const ran = join(root, "decoy-ran");
  writeFileSync(join(decoy, "llvm-profdata"), `#!/bin/sh\necho ran > "${ran}"\nexit 1\n`);
  chmodSync(join(decoy, "llvm-profdata"), 0o755);
  use([], { env: { ...env, PATH: `${decoy}${process.platform === "win32" ? ";" : ":"}${process.env.PATH}` } });
  assert.ok(!existsSync(ran), "PATH is not searched");

  // Beside Clang's executable, in front of the toolchain that Clang reports.
  rmSync(log, { force: true });
  const beside = join(root, "beside");
  wrapper(join(beside, "clang"), "clang", realClang);
  wrapper(join(beside, "llvm-profdata"), "llvm-profdata", real);
  use([], { env: { ...env, TSUZURI_CLANG: join(beside, "clang") } });
  assert.equal(merges(log).length, 1, "the llvm-profdata beside Clang merges");

  // Nothing beside it: the toolchain that Clang reports for itself.
  rmSync(log, { force: true });
  const alone = wrapper(join(root, "alone", "clang"), "clang", realClang);
  use([], { env: { ...env, TSUZURI_CLANG: alone } });
  assert.equal(merges(log).length, 0);
  assert.ok(logged(log).some(([name, argument]) => name === "clang" && argument === "-print-resource-dir"), "Clang is asked for its resource directory");

  // Nowhere: a clear error, and a merged profile still needs no tool.
  const missing = { ...env, TSUZURI_CLANG: alone, PGO_RESOURCE: join(root, "no-toolchain", "lib", "clang", "21") };
  const absent = use([], { env: missing, status: 1 });
  assert.ok(absent.stderr.includes("error[E2002]: cannot find llvm-profdata; set TSUZURI_LLVM_PROFDATA to the llvm-profdata that belongs to the Clang in TSUZURI_CLANG"), absent.stderr);
  const merged = join(root, "discovery-merged.profdata");
  execute(real, ["merge", "-o", merged, ...files(directory)]);
  tz(["build", project("branch"), "--profile-use", merged, "--no-cache", "-o", executable("discovered-file")], { env: missing });
  assert.equal(execute(executable("discovered-file"), []).stdout, total);
});

section("the merge is timed as a tool", () => {
  const { directory } = trained();
  const real = needProfdata();
  const env = { ...baseEnv, TSUZURI_TIME_PASSES: "1" };
  const phases = (result) => JSON.parse(result.stderr.split("\n").filter((line) => line.startsWith("{\"time_passes\"")).at(-1)).phases.map(({ name }) => name);
  assert.ok(phases(tz(["build", project("branch"), "--profile-use", directory, "-o", executable("timed")], { env })).includes("tool.llvm-profdata"));
  const merged = join(root, "timed.profdata");
  execute(real, ["merge", "-o", merged, ...files(directory)]);
  assert.ok(!phases(tz(["build", project("branch"), "--profile-use", merged, "-o", executable("timed")], { env })).includes("tool.llvm-profdata"));
});

finish();

function finish() {
  rmSync(root, { recursive: true, force: true });
  console.log(`PGO: ${outcome.passed} sections passed, ${outcome.skipped} skipped, ${outcome.failed} failed`);
  process.exit(outcome.failed === 0 ? 0 : 1);
}
