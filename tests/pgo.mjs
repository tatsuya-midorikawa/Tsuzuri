// PR07: profile-guided optimization. `--profile-generate DIR` builds a program that writes raw profiles under DIR when it
// exits normally, and `--profile-use PATH` builds with an indexed profile or with the merge of a directory of raw
// profiles. Clang does the work, so the sections that need its profile runtime and an llvm-profdata skip where there is
// none, unless TSUZURI_PGO_STRICT=1 or LLVM_PREFIX (a toolchain that CI installs on purpose) turns a skip into a failure.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { chmodSync, copyFileSync, cpSync, existsSync, linkSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
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
const { appendFileSync, readFileSync } = require("node:fs");
const { spawnSync } = require("node:child_process");
const args = process.argv.slice(2);
if (process.env.PGO_LOG) appendFileSync(process.env.PGO_LOG, JSON.stringify([${JSON.stringify(name)}, ...args]) + "\\n");
// The raw profiles that llvm-profdata merges are in a file that the build deletes, so the log keeps its lines.
for (const argument of args) {
  if (process.env.PGO_LOG && argument.startsWith("--input-files=")) {
    const lines = readFileSync(argument.slice("--input-files=".length), "utf8").split("\\n").filter(Boolean);
    appendFileSync(process.env.PGO_LOG, JSON.stringify(["input-files", ...lines]) + "\\n");
  }
}
if (${JSON.stringify(name)} === "clang" && process.env.PGO_RESOURCE && args[0] === "-print-resource-dir") {
  console.log(process.env.PGO_RESOURCE);
  process.exit(0);
}
const result = spawnSync(${JSON.stringify(real)}, args, { stdio: "inherit" });
// PGO_NOISE is what a linker of another machine prints for the link of an instrumented program, which this one does not.
const linking = args.some((argument) => argument.startsWith("-fprofile-generate")) && !args.includes("-c") && !args.includes("-S");
if (process.env.PGO_NOISE && linking) process.stderr.write(JSON.parse(process.env.PGO_NOISE).join("\\n") + "\\n");
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
// The counts of `tz_total`, the function of the library fixture that the tests of the wasm32 writer read back, in a raw or an indexed profile.
const functionCounts = (profdata, file) =>
  execute(profdata, ["show", "--all-functions", "--counts", file]).stdout.match(/tz_total:\n\s+Hash: \S+\n\s+Counters: \d+\n\s+Block counts: (\[[^\]]*\])/)?.[1];

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
    [["--profile-use", "a", "--target", "wasm64"], "profile-guided optimization supports --target wasm32 only; remove --target wasm64 or the profile option"],
    [["--profile-generate", "a", "--target", "wasm32", "--emit", "object"], "profile-guided optimization needs --emit wasm for WebAssembly"],
    [["--profile-use", "a", "--target", "wasm32", "--wasm-feature", "threads"], "--profile-generate and --profile-use cannot be combined with --wasm-feature threads: the counters are not atomic"],
    [["--profile-generate", "a", "--target", "wasm32", "--wasm-opt"], "--profile-generate cannot be combined with --wasm-opt"],
    [["--profile-generate", "a", "--emit", "llvm"], "profile-guided optimization needs --emit exe or object"],
    [["--profile-use", "a", "--emit", "shared"], "profile-guided optimization needs --emit exe or object"],
    [["--profile-generate", "a", "--emit", "bitcode"], "profile-guided optimization needs --emit exe or object"],
    [["--profile-use", "a", "--emit", "bitcode"], "profile-guided optimization needs --emit exe or object"],
    [["--profile-generate", "a", "--target", "wasm32", "--emit", "bitcode"], "profile-guided optimization needs --emit wasm for WebAssembly"],
    [["--profile-use", "a", "--target", "wasm32", "--emit", "bitcode"], "profile-guided optimization needs --emit wasm for WebAssembly"],
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
  for (const action of ["check", "test", "fmt"]) {
    for (const option of ["--profile-generate", "--profile-use"]) {
      const result = tz([action, project("branch"), option, "prof"], { env, status: 2 });
      assert.ok(result.stderr.includes("error[E2000]: --profile-generate and --profile-use are only valid with build or run"), `${action} ${option}\n${result.stderr}`);
    }
  }
  // `run` takes them, and refuses what a build refuses.
  const refusedRun = tz(["run", project("branch"), "--profile-generate", "a", "--profile-use", "b"], { env, status: 2 });
  assert.ok(refusedRun.stderr.includes("--profile-generate and --profile-use cannot be combined; build once with each"), refusedRun.stderr);
  const crossRun = tz(["run", project("branch"), "--profile-use", "a", "--target", cross], { env, status: 2 });
  assert.ok(crossRun.stderr.includes(`apply to the machine that builds; remove --target ${cross}`), crossRun.stderr);

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
  for (const action of ["build", "run"]) {
    for (const args of [["--profile-generate", join(directory, "prof")], ["--profile-use", join(directory, "prof")]]) {
      const result = tz([action, project("branch"), ...args], { env: { ...baseEnv, TSUZURI_CLANG: launcher }, status: 1 });
      assert.ok(result.stderr.includes("error[E2002]: profile options need an LLVM Clang: the bundled Clang launcher (tsuzuri-clang) passes no profile options to LLVM Clang"), result.stderr);
      assert.ok(result.stderr.includes("set TSUZURI_CLANG to an LLVM 21 Clang"), result.stderr);
    }
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
  const named = (flags) => flags.join("").replaceAll("-", "");
  for (const flags of variants) {
    const profile = join(root, `matrix-profile-${named(flags)}`);
    const exe = executable(`matrix-${named(flags)}`);
    build("branch", ["--profile-generate", profile, ...flags], exe);
    assert.equal(execute(exe, []).stdout, total, flags.join(" "));
    assert.equal(profraws(profile).length, 1, flags.join(" "));
  }
  // The profile of the default build is a profile of this program too; a flag that changes the IR may leave functions
  // unmatched. Its directory of raw profiles is merged, which the half above does without llvm-profdata.
  needProfdata();
  for (const flags of variants) {
    const used = executable(`matrix-use-${named(flags)}`);
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

section("a program that traps reports the same trap with and without a profile", () => {
  trained();
  const common = ["-O3", "--trap-info", "--no-cache"];
  const profile = join(root, "trap-compare-profile");
  const trap = (name, extra) => {
    const exe = executable(`trap-${name}`);
    const built = build("trap", [...common, ...extra], exe);
    const result = execute(exe, [], { status: null });
    assert.ok(result.signal, `${name} ends with a signal`);
    return { exe, built, result, sidecar: readFileSync(`${exe}.trap.json`) };
  };
  const plain = trap("plain", []);
  assert.ok(plain.result.stderr.includes("trap: integer division by zero at"), plain.result.stderr);
  assert.ok(plain.result.stderr.includes("Main.tz:1:43"), plain.result.stderr);
  const generated = trap("generate", ["--profile-generate", profile]);
  assert.equal(generated.result.signal, plain.result.signal);
  assert.equal(generated.result.stderr, plain.result.stderr, "the same message at the same place");
  assert.deepEqual(generated.sidecar, plain.sidecar, "the trap table is byte-identical");
  // With an argument the program divides by one and ends normally, which writes the profile of this program.
  execute(generated.exe, ["argument"]);
  assert.equal(profraws(profile).length, 1);
  // `run` reports the trap with its location and the source line, whatever the profile option.
  const reported = (extra) => tz(["run", project("trap"), "-O3", ...extra], { status: 1 }).stderr;
  const expected = reported([]);
  assert.ok(expected.includes("error[E2005]: program terminated with signal"), expected);
  assert.ok(expected.includes("Main.tz:1:43"), expected);
  assert.equal(reported(["--profile-generate", join(root, "trap-run-profile")]), expected);
  // Using the profile merges it, which needs llvm-profdata.
  needProfdata();
  const used = trap("use", ["--profile-use", profile]);
  assert.equal(used.built.stderr, "", "the profile matches the program");
  assert.equal(used.result.signal, plain.result.signal);
  assert.equal(used.result.stderr, plain.result.stderr, "the same message at the same place");
  assert.deepEqual(used.sidecar, plain.sidecar, "the trap table is byte-identical");
  assert.equal(reported(["--profile-use", profile]), expected);
});

section("run builds with the profile options and runs the program", () => {
  trained();
  const profile = join(root, "run-profile");
  // The instrumented program runs, prints what the program prints, and leaves its profile where the option says.
  const generated = tz(["run", project("branch"), "--profile-generate", profile]);
  assert.equal(generated.stdout, total);
  assert.equal(generated.stderr, "");
  assert.equal(profraws(profile).length, 1);
  // A trap ends the run before the profile is written, and the trap is reported as usual.
  const trapProfile = join(root, "run-trap-profile");
  const trapped = tz(["run", project("trap"), "--profile-generate", trapProfile], { status: 1 });
  assert.ok(trapped.stderr.includes("error[E2005]: program terminated with signal"), trapped.stderr);
  assert.deepEqual(profraws(trapProfile), []);
  // A directory that cannot be written is reported once the program has ended normally.
  const unwritable = join(root, "not-a-directory");
  writeFileSync(unwritable, "a file\n");
  const sub = join(unwritable, "profiles");
  const warned = tz(["run", project("branch"), "--profile-generate", sub]);
  assert.equal(warned.stdout, total);
  assert.ok(warned.stderr.includes(`the program wrote no profile under '${sub}'; check that the directory can be written`), warned.stderr);
  const json = tz(["run", project("branch"), "--profile-generate", sub, "--json"]);
  const warnings = json.stderr.split("\n").filter((line) => line.startsWith("{")).map((line) => JSON.parse(line));
  assert.equal(warnings.length, 1, json.stderr);
  assert.equal(warnings[0].code, "W2001");
  assert.ok(warnings[0].message.startsWith("the program wrote no profile under"), json.stderr);
  // LLVM_PROFILE_FILE chooses the file, so the warning stays out of the way.
  const chosen = join(root, "run-chosen", "run.profraw");
  const redirected = tz(["run", project("branch"), "--profile-generate", sub], { env: { ...baseEnv, LLVM_PROFILE_FILE: chosen } });
  assert.equal(redirected.stderr, "");
  assert.ok(existsSync(chosen));
  // A profile of a run is a profile of a run: the same build applies it without a warning. Merging it needs llvm-profdata.
  needProfdata();
  const used = tz(["run", project("branch"), "--profile-use", profile]);
  assert.equal(used.stdout, total);
  assert.equal(used.stderr, "");
});

section("the linker's note on the alignment of the profile sections is not shown, and no other linker warning is hidden", () => {
  trained();
  // Clang 21 links the profile runtime with 16 KiB sections, which a link for x86_64 macOS may not give: Apple's linker
  // says so on three lines, and the Intel runner printed exactly these for every instrumented link.
  const note = (section, from = "0x4000") => `ld: warning: reducing alignment of section ${section} from ${from} to 0x1000 because it exceeds segment maximum alignment`;
  const [counters, data, bits] = ["cnts", "data", "bits"].map((kind) => note(`__DATA,__llvm_prf_${kind}`));
  // What stays: another warning of the link, and the same words for a section that is not one of the profile's.
  const others = ["ld: warning: some other warning of the link", note("__DATA,__other_section"), note("__DATA,__llvm_prf_cnts", "0x8000")];
  const spy = wrapper(join(root, "noisy-tools", "clang"), "clang", realClang);
  const env = { ...baseEnv, TSUZURI_CACHE_DIR: join(root, "cache-noisy"), TSUZURI_CLANG: spy, PGO_NOISE: JSON.stringify([counters, data, others[0], bits, others[1], others[2]]) };
  const expected = `${others.join("\n")}\n`;
  const profiles = join(root, "noisy-profiles");
  const exe = executable("noisy");
  const built = tz(["build", project("branch"), "--profile-generate", profiles, "-o", exe], { env });
  assert.equal(built.stderr, expected);
  assert.equal(execute(exe, []).stdout, total);
  // The cache keeps what was shown, so a hit shows the same.
  assert.equal(tz(["build", project("branch"), "--profile-generate", profiles, "-o", exe], { env }).stderr, expected);
  assert.equal(tz(["run", project("branch"), "--profile-generate", profiles], { env }).stderr, expected);
  if (process.platform === "darwin") {
    // A debug build links in a second Clang run, which is where the note comes from.
    assert.equal(tz(["build", project("branch"), "--profile-generate", profiles, "-g", "-o", executable("noisy-g")], { env }).stderr, expected);
  }
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

section("a wasm32 module is instrumented, and the writer beside it saves the profile", () => {
  trained();
  const profdata = needProfdata();
  const plain = join(root, "library-plain.wasm");
  const probe = build("library", ["--target", "wasm32"], plain, { status: null });
  if (probe.status !== 0) skip(`no WebAssembly toolchain: ${probe.stderr.split("\n")[0]}`);
  const directory = join(root, "wasm-profile");
  const generated = join(root, "library-generate.wasm");
  const instrument = ["--target", "wasm32", "--profile-generate", directory];
  const built = build("library", instrument, generated);
  assert.ok(!built.stderr.includes("error"), built.stderr);
  const writer = `${generated}.profile.mjs`;
  assert.ok(existsSync(writer), "the writer is published beside the module");
  // The host runs the module, and the writer saves what the module counted. The writer of another module refuses a module that was not instrumented.
  const run = `import { readFileSync } from "node:fs";
import { defaultDirectory, profileBytes, writeProfile } from ${JSON.stringify(pathToFileURL(writer).href)};
const open = async (file) => (await WebAssembly.instantiate(readFileSync(file), {})).instance;
const instance = await open(${JSON.stringify(generated)});
console.log(JSON.stringify({ result: String(instance.exports.tz_total(100000n)), directory: defaultDirectory, written: await writeProfile(instance) }));
try { profileBytes(await open(${JSON.stringify(plain)})); } catch (error) { console.log(JSON.stringify({ refused: error.message })); }
`;
  const [trained_, refused] = execute(process.execPath, ["--input-type=module", "-e", run]).stdout.trim().split("\n").map((line) => JSON.parse(line));
  assert.equal(trained_.result, "3699964");
  assert.equal(trained_.directory, directory);
  assert.equal(dirname(trained_.written), directory);
  assert.equal(profraws(directory).length, 1);
  assert.match(refused.refused, /does not export __llvm_profile_raw_version/);

  // Several writers that save into one directory keep every profile: each call makes a file of its own and replaces none.
  // The writer imported under two query strings is two module instances, as the writers of two modules and of two worker
  // threads are: each counts its files from 0, and all of them have the id of this process. A call starts the counters of its
  // instance again, so that each call has an instance of its own, which counted the same workload.
  const href = pathToFileURL(writer).href;
  const several = join(root, "wasm-several");
  const [sequential, concurrent] = JSON.parse(execute(process.execPath, ["--input-type=module", "-e", `import { readFileSync } from "node:fs";
const writers = [await import(${JSON.stringify(`${href}?a`)}), await import(${JSON.stringify(`${href}?b`)})];
const open = async () => (await WebAssembly.instantiate(readFileSync(${JSON.stringify(generated)}), {})).instance;
const instances = [];
for (let index = 0; index < 6; index++) {
  instances.push(await open());
  instances[index].exports.tz_total(100000n);
}
const save = (index) => writers[index % 2].writeProfile(instances[index], ${JSON.stringify(several)});
const sequential = [await save(0), await save(1)];
const concurrent = await Promise.all([2, 3, 4, 5].map(save));
console.log(JSON.stringify([sequential, concurrent]));
`]).stdout.trim());
  const saved = [...sequential, ...concurrent];
  assert.equal(new Set(saved).size, 6, `a file for every call: ${saved.join(" ")}`);
  assert.deepEqual(profraws(several), saved.map((path) => basename(path)).sort());
  for (const path of saved) {
    assert.match(basename(path), /^default_wasm_\d+_\d+\.profraw$/);
    assert.ok(readFileSync(path).length > 0);
    assert.deepEqual(readFileSync(path), readFileSync(saved[0]), "the same workload counted the same");
  }
  // Worker threads load a copy of the writer each, and write at the same time.
  const workers = join(root, "wasm-workers");
  const workerFile = join(root, "train-worker.mjs");
  writeFileSync(workerFile, `import { readFileSync } from "node:fs";
import { parentPort, workerData } from "node:worker_threads";
const { writeProfile } = await import(workerData.writer);
const { instance } = await WebAssembly.instantiate(readFileSync(workerData.module), {});
instance.exports.tz_total(100000n);
parentPort.postMessage(await writeProfile(instance, workerData.directory));
`);
  const threads = JSON.parse(execute(process.execPath, ["--input-type=module", "-e", `import { Worker } from "node:worker_threads";
const start = () => new Promise((resolve, reject) => {
  const worker = new Worker(new URL(${JSON.stringify(pathToFileURL(workerFile).href)}), { execArgv: [], workerData: { writer: ${JSON.stringify(href)}, module: ${JSON.stringify(generated)}, directory: ${JSON.stringify(workers)} } });
  worker.once("message", resolve);
  worker.once("error", reject);
});
console.log(JSON.stringify(await Promise.all([start(), start(), start(), start()])));
`]).stdout.trim());
  assert.equal(new Set(threads).size, 4, `a file for every thread: ${threads.join(" ")}`);
  assert.equal(profraws(workers).length, 4);
  for (const path of threads) assert.deepEqual(readFileSync(path), readFileSync(threads[0]));

  // The counts of the module are the counts of the native program of the same source.
  const counts = (file) => functionCounts(profdata, file);
  const nativeDirectory = join(root, "wasm-oracle-profile");
  const object = join(root, "wasm-oracle.o");
  build("library", ["--emit", "object", "--profile-generate", nativeDirectory], object);
  const oracle = executable("wasm-oracle");
  execute(realClang, [join(fixtures, "host.c"), object, "-o", oracle, `-fprofile-generate=${nativeDirectory}`, "-lm"]);
  execute(oracle, []);
  assert.equal(counts(trained_.written), "[100000, 1, 33333, 33334]");
  assert.equal(counts(trained_.written), counts(files(nativeDirectory)[0]));

  // A call starts the counters of its instance again, so the files of one instance hold what it counted between the calls, and
  // the merge of a directory is what it counted in all: no iteration is counted twice, and none is lost.
  const merged = (directory) => {
    const output = `${directory}.profdata`;
    execute(profdata, ["merge", "-o", output, ...files(directory)]);
    return counts(output);
  };
  const deltas = join(root, "wasm-deltas");
  const scenes = JSON.parse(execute(process.execPath, ["--input-type=module", "-e", `import { readFileSync } from "node:fs";
import { open } from "node:fs/promises";
import { join } from "node:path";
import { profileBytes, writeProfile } from ${JSON.stringify(href)};
const into = (name) => join(${JSON.stringify(deltas)}, name);
const instance = (await WebAssembly.instantiate(readFileSync(${JSON.stringify(generated)}), {})).instance;
const work = () => instance.exports.tz_total(1000n);
const bytes = () => Buffer.from(profileBytes(instance)).toString("base64");
// The writer puts the bytes in the file with this method, which the scenes that follow replace to make a write fail or take time.
const handle = await open(process.execPath);
const prototype = Object.getPrototypeOf(handle);
await handle.close();
const original = prototype.writeFile;
const replacing = async (replacement, body) => {
  prototype.writeFile = replacement;
  try {
    return await body();
  } finally {
    prototype.writeFile = original;
  }
};
const scenes = {};

// One instance, the workload twice, and a call after each. Reading the profile does not start the counters again.
work();
scenes.reads = bytes() === bytes();
const first = await writeProfile(instance, into("twice"));
work();
scenes.twice = [first, await writeProfile(instance, into("twice"))];

// Two calls that overlap on one instance: what it counted is in one of the two files.
work();
scenes.overlap = await Promise.all([writeProfile(instance, into("overlap")), writeProfile(instance, into("overlap"))]);

// A write that fails leaves no file, and the counters hold what they held.
work();
const before = bytes();
scenes.failure = await replacing(async () => { throw Object.assign(new Error("disk full"), { code: "ENOSPC" }); }, () => writeProfile(instance, into("failure")).then(() => "written", (error) => error.code));
scenes.kept = bytes() === before;
scenes.retried = await writeProfile(instance, into("failure"));

// The module counts while a file is written: that is in the next file, and neither lost nor in both.
work();
let counting = true;
scenes.window = [await replacing(function (...args) {
  if (counting) {
    counting = false;
    work();
  }
  return original.apply(this, args);
}, () => writeProfile(instance, into("window")))];
scenes.window.push(await writeProfile(instance, into("window")));
console.log(JSON.stringify(scenes));
`]).stdout.trim());
  const thousand = "[1000, 1, 333, 334]";
  assert.equal(scenes.reads, true, "profileBytes only reads");
  assert.equal(new Set(scenes.twice).size, 2);
  assert.deepEqual(scenes.twice.map(counts), [thousand, thousand], "each file holds what was counted since the call before");
  assert.equal(merged(join(deltas, "twice")), "[2000, 2, 666, 668]");
  assert.equal(new Set(scenes.overlap).size, 2);
  assert.equal(merged(join(deltas, "overlap")), thousand, "calls that overlap count the workload once");
  assert.deepEqual([scenes.failure, scenes.kept], ["ENOSPC", true], "a write that failed gives the counts back");
  assert.equal(profraws(join(deltas, "failure")).length, 1, "a write that failed leaves no file");
  assert.equal(counts(scenes.retried), thousand);
  assert.deepEqual(scenes.window.map(counts), [thousand, thousand], "what is counted while a file is written is in the next one");
  assert.equal(merged(join(deltas, "window")), "[2000, 2, 666, 668]");

  // The cache returns the module and its writer.
  const again = join(root, "library-generate-again.wasm");
  build("library", instrument, again);
  assert.deepEqual(readFileSync(again), readFileSync(generated));
  assert.equal(readFileSync(`${again}.profile.mjs`, "utf8"), readFileSync(writer, "utf8"));

  // The profile applies to the same program without a word, and the module still computes the same.
  const used = join(root, "library-use.wasm");
  const applied = build("library", ["--target", "wasm32", "--profile-use", directory], used);
  assert.equal(applied.stderr, "");
  const evaluate = (file) => execute(process.execPath, ["-e", `const module = new WebAssembly.Module(require("node:fs").readFileSync(${JSON.stringify(file)}));
console.log(String(new WebAssembly.Instance(module, {}).exports.tz_total(100000n)));`]).stdout.trim();
  assert.equal(evaluate(used), "3699964");
  // A program that changed is told so in one line.
  const changed = join(root, "project-changed-library");
  mkdirSync(changed, { recursive: true });
  writeFileSync(join(changed, "Main.tz"), readFileSync(join(fixtures, "library", "Main.tz"), "utf8").replace("    | _ -> 100", "    | 2 -> 100\n    | _ -> 1000"));
  const stale = tz(["build", changed, "--target", "wasm32", "--profile-use", directory, "-o", join(root, "library-stale.wasm")]);
  assert.match(stale.stderr.trim(), /^profile data does not match \d+ function\(s\) in this build; they are optimized without profile data\./);
  assert.equal(evaluate(join(root, "library-stale.wasm")), "3699964");
});

// A module whose stack is 2 GiB or more has its profile sections at or above 2^31, where the i32 global that holds an address
// reads as a negative number. The writer is tried on an instance made by hand in a memory of more than 2.5 GiB, and on real modules.
section("a profile that lies above 2 GiB of memory is written as one below it", () => {
  trained();
  const profdata = needProfdata();
  const counts = (file) => functionCounts(profdata, file);
  const probe = build("library", ["--target", "wasm32"], join(root, "library-plain-again.wasm"), { status: null });
  if (probe.status !== 0) skip(`no WebAssembly toolchain: ${probe.stderr.split("\n")[0]}`);
  // The real modules have a stack of 2 GiB, which puts every section above 2^31, and a little less, which puts the counters across
  // it. A memory limit of almost 4 GiB, which they need, changes the code of `tz_total`, so the oracle has the same limit and the
  // default stack: the same program, with its sections far below 2^31.
  const make = (name, extra) => {
    const module = join(root, `library-${name}.wasm`);
    build("library", ["--target", "wasm32", "--wasm-max-memory", "4294901760", ...extra, "--profile-generate", join(root, `wasm-${name}`)], module);
    return module;
  };
  const oracle = make("limit", []);
  const stacks = [2147483648, 2147483520];
  const modules = stacks.map((stack) => make(`stack-${stack}`, ["--wasm-stack-size", String(stack)]));
  const href = pathToFileURL(`${modules[0]}.profile.mjs`).href;

  // The same profile at four addresses: below 2^31; with the counters across it; with every section from it on; above it.
  const places = [0x1000, 0x7ffffff0, 0x7ffffff8, 0xa0000000];
  const top = places.at(-1);
  const version = (1n << 56n) | 10n;
  const data = Buffer.from(Array.from({ length: 96 }, (_, index) => 1 + (index % 250)));
  const names = Buffer.from("ABCDEFGHIJKLM");
  // The raw profile of that layout: a header of 16 words, the data records, the counters, and the names padded to 8 bytes.
  // The counters lie 24 bytes before the data records, so their difference is a 32-bit number; the names start at their address.
  const expected = (base, counters = [7n, 8n, 9n]) => {
    const bytes = Buffer.alloc(128 + 96 + 24 + 16);
    [0xff6c70726f665281n, version, 0n, 2n, 0n, 3n, 0n, 0n, 0n, 13n, 2n ** 32n - 24n, 0n, BigInt(base + 128), 0n, 0n, 2n].forEach((word, index) => bytes.writeBigUInt64LE(word, index * 8));
    data.copy(bytes, 128);
    counters.forEach((count, index) => bytes.writeBigUInt64LE(count, 224 + index * 8));
    names.copy(bytes, 248);
    return bytes;
  };
  const directory = join(root, "wasm-above");
  const given = JSON.stringify({ places, version: String(version), data: data.toString("base64"), names: names.toString("base64"), directory });
  const made = JSON.parse(execute(process.execPath, ["--input-type=module", "-e", `import { profileBytes, writeProfile } from ${JSON.stringify(href)};
const { places, version, data, names, directory } = ${given};
const result = await (async () => {
  let memory;
  try {
    memory = new WebAssembly.Memory({ initial: 40976 });
  } catch (error) {
    if (error instanceof RangeError) return { unsupported: error.message };
    throw error;
  }
  const bytes = new Uint8Array(memory.buffer);
  const view = new DataView(memory.buffer);
  const address = (value) => new WebAssembly.Global({ value: "i32" }, value | 0);
  const place = (base) => {
    view.setBigUint64(base, BigInt(version), true);
    [7n, 8n, 9n].forEach((count, index) => view.setBigUint64(base + 8 + index * 8, count, true));
    bytes.set(Buffer.from(data, "base64"), base + 32);
    bytes.set(Buffer.from(names, "base64"), base + 128);
    return { exports: { memory, __llvm_profile_raw_version: address(base),
      __start___llvm_prf_cnts: address(base + 8), __stop___llvm_prf_cnts: address(base + 32),
      __start___llvm_prf_data: address(base + 32), __stop___llvm_prf_data: address(base + 128),
      __start___llvm_prf_names: address(base + 128), __stop___llvm_prf_names: address(base + 141) } };
  };
  const attempt = async (body) => {
    try {
      return await body();
    } catch (error) {
      return { error: error.name + ": " + error.message };
    }
  };
  const read = {};
  for (const base of places) read[base] = await attempt(() => Buffer.from(profileBytes(place(base))).toString("base64"));
  // Sections that are not inside the memory are refused, whatever the number reads as.
  const size = memory.buffer.byteLength;
  const beyond = place(0x1000);
  beyond.exports.__stop___llvm_prf_names = address(size + 64);
  const lost = place(0x1000);
  lost.exports.__llvm_profile_raw_version = address(size);
  const refused = [await attempt(() => profileBytes(beyond)), await attempt(() => profileBytes(lost))];
  const top = places.at(-1);
  const instance = place(top);
  const saved = await attempt(async () => [await writeProfile(instance, directory), await writeProfile(instance, directory)]);
  const after = (from, to) => Buffer.from(bytes.subarray(top + from, top + to)).toString("base64");
  return { read, refused, saved, counters: after(8, 32), kept: after(32, 141) };
})();
console.log(JSON.stringify(result));
`]).stdout.trim());
  if (made.unsupported) skip(`this host cannot make a WebAssembly memory of 2.5 GiB: ${made.unsupported}`);
  for (const base of places) {
    const where = `a profile at 0x${base.toString(16)}`;
    assert.equal(typeof made.read[base], "string", `profileBytes of ${where}: ${made.read[base]?.error}`);
    assert.deepEqual(Buffer.from(made.read[base], "base64"), expected(base), `profileBytes of ${where}`);
  }
  assert.ok(Array.isArray(made.saved), `writeProfile of a profile at 0x${top.toString(16)}: ${made.saved?.error}`);
  assert.deepEqual(made.saved.map((path) => readFileSync(path)), [expected(top), expected(top, [0n, 0n, 0n])]);
  assert.equal(made.counters, Buffer.alloc(24).toString("base64"), "the counters start from zero again");
  assert.equal(made.kept, Buffer.concat([data, names]).toString("base64"), "the data records and the names stay");
  for (const entry of made.refused) assert.match(entry.error ?? "profile bytes", /not inside its memory/);

  // The module counts `tz_total(1000n)` and its writer saves the profile, as it does for any module.
  const train = (module) => JSON.parse(execute(process.execPath, ["--input-type=module", "-e", `import { readFileSync } from "node:fs";
import { writeProfile } from ${JSON.stringify(pathToFileURL(`${module}.profile.mjs`).href)};
const result = await (async () => {
  let instance;
  try {
    instance = (await WebAssembly.instantiate(readFileSync(${JSON.stringify(module)}), {})).instance;
  } catch (error) {
    if (error instanceof RangeError) return { unsupported: error.message };
    throw error;
  }
  instance.exports.tz_total(1000n);
  return { path: await writeProfile(instance) };
})();
console.log(JSON.stringify(result));
`]).stdout.trim());
  const reference = train(oracle);
  assert.ok(reference.path, `the oracle runs: ${reference.unsupported}`);
  const counted = counts(reference.path);
  assert.match(counted, /^\[1000, 1, /, "the oracle counts the loop");
  for (const [index, stack] of stacks.entries()) {
    const saved = train(modules[index]);
    if (saved.unsupported) skip(`this host cannot run a WebAssembly module with a stack of ${stack} bytes: ${saved.unsupported}`);
    assert.equal(counts(saved.path), counted, `a module with a stack of ${stack} bytes counts what the oracle counts`);
  }
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

section("a directory of thousands of raw profiles is merged through a list", () => {
  const { directory } = trained();
  needProfdata();
  // Together the paths are longer than the arguments of a process (1 MiB on macOS, 2 MiB on Linux). The name of the
  // directory has a comma and a space, which a plain list of paths would misread.
  const many = join(root, "many, raw profiles");
  mkdirSync(many, { recursive: true });
  const [raw] = files(directory);
  let place = linkSync;
  for (let index = 0; index < 10_000; index += 1) {
    const target = join(many, `${String(index).padStart(5, "0")}-${"x".repeat(230)}.profraw`);
    try {
      place(raw, target);
    } catch {
      place = copyFileSync;
      place(raw, target);
    }
  }
  const paths = files(many);
  assert.equal(paths.length, 10_000);
  assert.ok(paths.reduce((size, path) => size + path.length + 1, 0) > 2 ** 21, "the paths are longer than any limit of arguments");
  const exe = executable("many");
  const built = build("branch", ["--profile-use", many, "--no-cache"], exe);
  assert.equal(built.stderr, "");
  assert.equal(execute(exe, []).stdout, total);
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
  const [, command, flag, output, ...rest] = merges(log)[0];
  assert.deepEqual([command, flag, basename(output)], ["merge", "-o", "profile.profdata"]);
  assert.equal(rest.length, 1, "the raw profiles are not arguments of the merge");
  assert.ok(rest[0].startsWith("--input-files="), rest[0]);
  const [, ...listed] = logged(log).find(([name]) => name === "input-files");
  assert.deepEqual(listed, files(directory).map((file) => `1,${file}`), "the raw profiles are merged in the order of their names");
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
