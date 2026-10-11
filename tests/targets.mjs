// Native targets named with `--target TRIPLE` (G15): what each one produces on this machine, and
// whether the result was executed, linked only, or stopped at an object.
//
//   node tests/targets.mjs target/release/tsuzuri
//
// TSUZURI_CLANG              Clang for the host and object checks (as for the other scripts).
// TSUZURI_CROSS_LINK=1       Also link executables for every foreign target that a cross toolchain can link,
//                            and fail when one cannot be linked. TSUZURI_CROSS_CLANG names the cross
//                            toolchain: the bundled `tsuzuri-clang`, which brings Zig. Apple targets
//                            from macOS use the system Clang and the iPhoneOS and iPhoneSimulator SDKs
//                            that `xcrun` reports.
// TSUZURI_CROSS_EXECUTE=1    Also execute foreign executables through an emulator, and fail when there is
//                            none: qemu-user for the static musl targets on Linux (TSUZURI_RUNNER does
//                            the work, so the runner hook is exercised for real).
//
// Without the opt-in variables the script checks the host's own target, the objects of every target,
// the WebAssembly aliases, and the refusals. It never installs anything and runs no foreign executable
// without an emulator that is already there.
import assert from "node:assert/strict";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(process.argv[2] ?? join(root, "target/release/tsuzuri"));
const systemClang = process.env.TSUZURI_CLANG ?? "clang";
const crossLink = process.env.TSUZURI_CROSS_LINK === "1";
const crossExecute = process.env.TSUZURI_CROSS_EXECUTE === "1";
const crossClang = process.env.TSUZURI_CROSS_CLANG ?? process.env.TSUZURI_CLANG;
// A missing tool fails the run where a CI job or an opt-in says that the path must be exercised.
const strict = crossLink || crossExecute || process.env.CI !== undefined;
if (process.platform !== "darwin" && process.platform !== "linux") {
  throw new Error("this script runs on macOS and Linux; Windows has tests/windows.mjs");
}
if (crossLink && !crossClang) {
  throw new Error("TSUZURI_CROSS_LINK=1 needs TSUZURI_CROSS_CLANG (or TSUZURI_CLANG) to name the bundled tsuzuri-clang");
}

// The targets, spelled out independently of the compiler's table: the machine each one is for, the
// object format, and what the format records. `compiled` is whether Zig has a C library for it.
const triples = [
  ["aarch64-apple-darwin", { os: "macos", arch: "aarch64", format: "macho", platform: 1, minos: 12 }],
  ["x86_64-apple-darwin", { os: "macos", arch: "x86_64", format: "macho", platform: 1, minos: 12 }],
  ["aarch64-unknown-linux-gnu", { os: "linux", arch: "aarch64", format: "elf", libc: "gnu" }],
  ["x86_64-unknown-linux-gnu", { os: "linux", arch: "x86_64", format: "elf", libc: "gnu" }],
  ["aarch64-pc-windows-msvc", { os: "windows", arch: "aarch64", format: "coff", libc: "msvc" }],
  ["x86_64-pc-windows-msvc", { os: "windows", arch: "x86_64", format: "coff", libc: "msvc" }],
  ["aarch64-unknown-linux-musl", { os: "linux", arch: "aarch64", format: "elf", libc: "musl" }],
  ["x86_64-unknown-linux-musl", { os: "linux", arch: "x86_64", format: "elf", libc: "musl" }],
  ["riscv64gc-unknown-linux-gnu", { os: "linux", arch: "riscv64", format: "elf", libc: "gnu" }],
  ["riscv64gc-unknown-linux-musl", { os: "linux", arch: "riscv64", format: "elf", libc: "musl" }],
  ["aarch64-pc-windows-gnu", { os: "windows", arch: "aarch64", format: "coff", libc: "mingw" }],
  ["x86_64-pc-windows-gnu", { os: "windows", arch: "x86_64", format: "coff", libc: "mingw" }],
  ["aarch64-unknown-freebsd", { os: "freebsd", arch: "aarch64", format: "elf" }],
  ["x86_64-unknown-freebsd", { os: "freebsd", arch: "x86_64", format: "elf" }],
  ["aarch64-linux-android", { os: "android", arch: "aarch64", format: "elf" }],
  ["x86_64-linux-android", { os: "android", arch: "x86_64", format: "elf" }],
  ["aarch64-apple-ios", { os: "ios", arch: "aarch64", format: "macho", platform: 2, minos: 13, sdk: "iphoneos" }],
  ["aarch64-apple-ios-sim", { os: "ios", arch: "aarch64", format: "macho", platform: 7, minos: 14, sdk: "iphonesimulator" }],
  ["x86_64-apple-ios", { os: "ios", arch: "x86_64", format: "macho", platform: 7, minos: 13, sdk: "iphonesimulator" }],
  ["aarch64-unknown-none", { os: "none", arch: "aarch64", format: "elf", bare: true }],
  ["x86_64-unknown-none", { os: "none", arch: "x86_64", format: "elf", bare: true }],
  ["riscv64gc-unknown-none-elf", { os: "none", arch: "riscv64", format: "elf", bare: true }],
];
const byName = new Map(triples);

const machine = {
  macho: { aarch64: 0x0100000c, x86_64: 0x01000007 },
  elf: { aarch64: 183, x86_64: 62, riscv64: 243 },
  coff: { aarch64: 0xaa64, x86_64: 0x8664 },
};
const expected = { tasks: "21325334000\n", control: "42\n" };

const work = mkdtempSync(join(tmpdir(), "tsuzuri-targets-"));
const baseEnvironment = { ...process.env, TSUZURI_CACHE_DIR: join(work, "cache") };
delete baseEnvironment.TSUZURI_RUNNER;
delete baseEnvironment.TSUZURI_SYSROOT;
// Zig keeps its caches in the working directory unless it is told otherwise.
baseEnvironment.ZIG_GLOBAL_CACHE_DIR ??= join(work, "zig-cache");
baseEnvironment.ZIG_LOCAL_CACHE_DIR ??= join(work, "zig-local");

function execute(program, args, { success = true, env = {}, cwd = root, timeout = 600_000 } = {}) {
  const result = spawnSync(program, args, {
    encoding: "utf8",
    timeout,
    maxBuffer: 16 * 1024 * 1024,
    cwd,
    env: { ...baseEnvironment, ...env },
  });
  if (result.error) throw result.error;
  if (success) {
    assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}\n${result.signal ?? ""}`);
  }
  return result;
}

const cli = (args, options) => execute(compiler, args, options);

/** The error that `--json` reports on stderr: the last JSON line. */
function failure(args, env = {}) {
  const result = cli([...args, "--json"], { success: false, env });
  const line = result.stderr.trim().split("\n").filter((text) => text.startsWith("{")).pop();
  assert.ok(line, `${args.join(" ")}\n${result.stderr}`);
  return { status: result.status, ...JSON.parse(line) };
}

function tool(command) {
  return spawnSync(command, ["--version"], { encoding: "utf8" }).status === 0;
}

/** The format, machine and kind of an object or executable, from its header alone. */
function inspect(file) {
  const bytes = readFileSync(file);
  if (bytes.readUInt32LE(0) === 0xfeedfacf) {
    const found = { format: "macho", machine: bytes.readUInt32LE(4), type: bytes.readUInt32LE(12) };
    let offset = 32;
    for (let command = 0; command < bytes.readUInt32LE(16); command++) {
      if (bytes.readUInt32LE(offset) === 0x32) {
        found.platform = bytes.readUInt32LE(offset + 8);
        found.minos = bytes.readUInt32LE(offset + 12) >>> 16;
      }
      offset += bytes.readUInt32LE(offset + 4);
    }
    return found;
  }
  if (bytes.subarray(0, 4).equals(Buffer.from("\x7fELF", "latin1"))) {
    assert.deepEqual([bytes[4], bytes[5]], [2, 1], `${file} is not a 64-bit little-endian ELF`);
    return { format: "elf", machine: bytes.readUInt16LE(18), type: bytes.readUInt16LE(16), osabi: bytes[7], flags: bytes.readUInt32LE(48), bytes };
  }
  if (bytes.readUInt16LE(0) === 0x5a4d) {
    const header = bytes.readUInt32LE(0x3c);
    assert.equal(bytes.readUInt32LE(header), 0x4550, `${file} has no PE signature`);
    return { format: "coff", machine: bytes.readUInt16LE(header + 4), type: "executable" };
  }
  return { format: "coff", machine: bytes.readUInt16LE(0), type: "object" };
}

/** What the header of `file` must say for target `name`; `kind` is `object` or `executable`. */
function check(name, file, kind) {
  const target = byName.get(name);
  const found = inspect(file);
  assert.equal(found.format, target.format, `${name}: ${file}`);
  assert.equal(found.machine, machine[target.format][target.arch], `${name}: ${file}`);
  if (target.format === "macho") {
    assert.equal(found.type, kind === "object" ? 1 : 2, name);
    assert.equal(found.platform, target.platform, name);
    assert.equal(found.minos, target.minos, name);
  } else if (target.format === "elf") {
    assert.ok(kind === "object" ? found.type === 1 : [2, 3].includes(found.type), `${name}: e_type ${found.type}`);
    // The linker may leave a FreeBSD executable at ELFOSABI_NONE, which FreeBSD runs by its interpreter.
    if (kind === "object") assert.equal(found.osabi, target.os === "freebsd" ? 9 : 0, name);
    if (target.arch === "riscv64") {
      // EF_RISCV_RVC, and the float ABI field EF_RISCV_FLOAT_ABI_DOUBLE.
      assert.equal(found.flags & 0x1, 1, name);
      assert.equal(found.flags & 0x6, 0x4, `${name}: ${found.flags.toString(16)}`);
    }
    if (target.os === "freebsd" && kind === "executable") {
      assert.ok(found.bytes.includes("/libexec/ld-elf.so.1"), name);
    }
    if (target.libc === "gnu" && kind === "executable") {
      assert.ok(found.bytes.includes(target.arch === "x86_64" ? "/lib64/ld-linux-x86-64.so.2" : target.arch === "aarch64" ? "/lib/ld-linux-aarch64.so.1" : "/lib/ld-linux-riscv64-lp64d.so.1"), name);
    }
  } else if (kind === "executable") {
    assert.equal(found.type, "executable", name);
  }
}

const level = { object: 0, linked: 1, executed: 2 };
const outcome = new Map();
function record(name, kind, note) {
  const previous = outcome.get(name);
  if (!previous || level[kind] > level[previous.kind]) outcome.set(name, { kind, notes: [note] });
  else if (level[kind] === level[previous.kind]) previous.notes.push(note);
}

function hostTriple() {
  const arch = process.arch === "arm64" ? "aarch64" : process.arch === "x64" ? "x86_64" : undefined;
  assert.ok(arch, `unsupported host architecture ${process.arch}`);
  if (process.platform === "darwin") return `${arch}-apple-darwin`;
  const glibc = process.report.getReport().header.glibcVersionRuntime;
  return `${arch}-unknown-linux-${glibc ? "gnu" : "musl"}`;
}
const host = hostTriple();
const hostArch = byName.get(host).arch;
assert.ok(byName.has(host), host);

const answer = join(work, "answer");
mkdirSync(answer);
writeFileSync(join(answer, "Main.tz"), "export def answer :: i64\nfn answer = 42\n");
const suite = join(work, "suite");
mkdirSync(suite);
writeFileSync(join(suite, "Main.tz"), 'test "passes" = assert true\ntest "adds" = assert (40 + 2 == 42)\n');

try {
  // The help lists exactly the targets of this table: a target added to the compiler needs a row here.
  const help = cli(["--help"]).stdout;
  const listed = help.slice(help.indexOf("TRIPLE (64-bit native targets only")).split("\n").slice(1).map((line) => line.trim()).filter(Boolean);
  assert.deepEqual(listed, triples.map(([name]) => name));
  console.log(`help: ${listed.length} targets listed`);

  // The machine's own target, named explicitly: the same programs as `--target native`, at -O0 and -O3.
  for (const optimization of ["-O0", "-O3"]) {
    for (const example of ["tasks", "control"]) {
      const named = join(work, `${example}-${host}${optimization}`);
      const native = join(work, `${example}-native${optimization}`);
      cli(["build", `examples/${example}`, "--no-cache", optimization, "--target", host, "-o", named], { env: { TSUZURI_CLANG: systemClang } });
      cli(["build", `examples/${example}`, "--no-cache", optimization, "-o", native], { env: { TSUZURI_CLANG: systemClang } });
      check(host, named, "executable");
      assert.equal(execute(named, []).stdout, expected[example], `${example} ${optimization}`);
      assert.equal(execute(native, []).stdout, expected[example]);
    }
    // `run` and `test` need no runner for the machine's own target, and a runner is not asked.
    const unused = { TSUZURI_CLANG: systemClang, TSUZURI_RUNNER: "tsuzuri-runner-that-does-not-exist" };
    assert.equal(cli(["run", "examples/control", optimization, "--no-cache", "--target", host], { env: unused }).stdout, expected.control);
    assert.match(cli(["test", suite, optimization, "--target", host, "--json"], { env: unused }).stdout, /"passed":2,"failed":0/);
  }
  const tuned = join(work, "tuned.o");
  cli(["build", answer, "--emit", "object", "--target", host, "--cpu", "native", "--no-cache", "-o", tuned], { env: { TSUZURI_CLANG: systemClang } });
  check(host, tuned, "object");
  record(host, "executed", "-O0 and -O3: build, run, test; --target native gives the same output");

  // Every target as an object of a program that needs no C runtime: the only output a plain Clang
  // can make for a target whose headers and linker it does not have.
  const backends = (clang) => spawnSync(clang, ["--print-targets"], { encoding: "utf8" }).stdout.split("\n").map((line) => line.trim().split(/\s+/)[0]);
  const available = new Set(backends(systemClang));
  const backend = { aarch64: "aarch64", x86_64: "x86-64", riscv64: "riscv64" };
  for (const [name, target] of triples) {
    if (!available.has(backend[target.arch])) {
      assert.ok(!strict, `${systemClang} has no ${backend[target.arch]} code generator, which ${name} needs`);
      console.log(`${name}: skipped, ${systemClang} has no ${backend[target.arch]} code generator`);
      continue;
    }
    const object = join(work, `${name}.o`);
    const flags = target.bare ? ["--freestanding", "--allocator", "host"] : [];
    cli(["build", answer, "--emit", "object", "--no-cache", "--target", name, ...flags, "-o", object], { env: { TSUZURI_CLANG: systemClang } });
    check(name, object, "object");
    record(name, "object", `${target.format} ${target.arch} object`);
    if (target.libc === "msvc") {
      // Clang stamps a COFF object with the time unless told not to, which would make a second build differ.
      await new Promise((done) => setTimeout(done, 1100));
      const again = join(work, `${name}-again.o`);
      cli(["build", answer, "--emit", "object", "--no-cache", "--target", name, "-o", again], { env: { TSUZURI_CLANG: systemClang } });
      assert.ok(readFileSync(object).equals(readFileSync(again)), `${name}: the object differs between two builds`);
    }
  }

  // The other Apple architecture links with the Apple tools that are already here.
  if (process.platform === "darwin") {
    const other = host === "aarch64-apple-darwin" ? "x86_64-apple-darwin" : "aarch64-apple-darwin";
    const executable = join(work, `tasks-${other}`);
    cli(["build", "examples/tasks", "--no-cache", "--target", other, "-o", executable], { env: { TSUZURI_CLANG: systemClang } });
    check(other, executable, "executable");
    record(other, "linked", "Mach-O linked with the system Clang");
    // Rosetta, where it is installed, runs the other architecture through the runner hook.
    const rosetta = host === "aarch64-apple-darwin" && spawnSync("arch", ["-x86_64", "/usr/bin/true"]).status === 0;
    if (rosetta) {
      const run = cli(["run", "examples/tasks", "--no-cache", "--target", other], { env: { TSUZURI_CLANG: systemClang, TSUZURI_RUNNER: "arch -x86_64" } });
      assert.equal(run.stdout, expected.tasks);
      record(other, "executed", "through Rosetta (TSUZURI_RUNNER='arch -x86_64')");
    }
    // The iOS targets link against an SDK, and they are never run here.
    const sdks = new Map();
    const developer = process.env.DEVELOPER_DIR ?? "/Applications/Xcode.app/Contents/Developer";
    const find = (sdk) => {
      const reported = spawnSync("xcrun", ["--sdk", sdk, "--show-sdk-path"], { encoding: "utf8" });
      if (reported.status === 0 && reported.stdout.trim()) return reported.stdout.trim();
      // `xcrun` refuses until the Xcode license is accepted, though the SDK is there.
      const platform = sdk === "iphoneos" ? "iPhoneOS" : "iPhoneSimulator";
      const path = join(developer, "Platforms", `${platform}.platform`, "Developer", "SDKs", `${platform}.sdk`);
      return existsSync(path) ? path : undefined;
    };
    for (const [name, target] of triples.filter(([, target]) => target.os === "ios")) {
      if (!sdks.has(target.sdk)) sdks.set(target.sdk, find(target.sdk));
      const sdk = sdks.get(target.sdk);
      if (!sdk) {
        assert.ok(!crossLink, `${name} needs the ${target.sdk} SDK, which xcrun does not report`);
        console.log(`${name}: no ${target.sdk} SDK here, so it stays an object`);
        continue;
      }
      const exe = join(work, `tasks-${name}`);
      cli(["build", "examples/tasks", "--no-cache", "--target", name, "-o", exe], { env: { TSUZURI_CLANG: "/usr/bin/clang", TSUZURI_SYSROOT: sdk } });
      check(name, exe, "executable");
      record(name, "linked", `Mach-O linked against ${sdk.split("/").pop()}`);
      // Without the SDK the build says so instead of linking the macOS libraries.
      const missing = failure(["build", "examples/tasks", "--no-cache", "--target", name, "-o", join(work, "no-sdk")], { TSUZURI_CLANG: "/usr/bin/clang" });
      assert.equal(missing.code, "E2002", name);
      assert.match(missing.message, /incompatible-sysroot/);
      assert.match(missing.message, /set TSUZURI_SYSROOT to it/);
    }
  }

  if (crossLink) {
    // The bundled toolchain brings the C library of every target below, so no sysroot is needed.
    const cross = { TSUZURI_CLANG: crossClang };
    for (const [name, target] of triples) {
      if (name === host || target.bare || target.os === "android" || target.os === "ios" || target.libc === "msvc") continue;
      const exe = join(work, `tasks-cross-${name}${target.os === "windows" ? ".exe" : ""}`);
      cli(["build", "examples/tasks", "--no-cache", "--target", name, "-o", exe], { env: cross });
      check(name, exe, "executable");
      record(name, "linked", `${target.format} linked with ${crossClang.split("/").pop()}`);
      // A program that needs no runtime also builds as an object and as a shared library, where the target has them.
      if (target.os !== "windows") {
        const library = join(work, `answer-${name}.lib`);
        cli(["build", answer, "--emit", "shared", "--no-cache", "--target", name, "-o", library], { env: cross });
        const found = inspect(library);
        assert.equal(found.machine, machine[target.format][target.arch], name);
        assert.equal(found.type, target.format === "macho" ? 6 : 3, `${name} shared library`);
      }
      // A static musl executable of the host's architecture runs on any Linux.
      if (process.platform === "linux" && target.libc === "musl" && target.arch === hostArch) {
        assert.equal(execute(exe, []).stdout, expected.tasks, name);
        record(name, "executed", "static musl executable, run directly");
      }
    }
    // The Windows MSVC targets need the MSVC SDK, which no bundle has, and the bundle says so.
    for (const name of ["x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc"]) {
      const refused = failure(["build", "examples/tasks", "--no-cache", "--target", name, "-o", join(work, "msvc")], cross);
      assert.equal(refused.code, "E2002", name);
      assert.match(refused.message, /MinGW, not the MSVC runtime/);
      assert.match(refused.message, /pc-windows-gnu/);
    }
    // A sysroot and the bundled toolchain do not mix: the bundle brings its own C library.
    const sysroot = failure(["build", "examples/tasks", "--no-cache", "--target", "x86_64-unknown-linux-gnu", "-o", join(work, "sysroot")], { ...cross, TSUZURI_SYSROOT: work });
    assert.equal(sysroot.code, "E2002");
    assert.match(sysroot.message, /cannot use a sysroot/);
    // Android, iOS and bare metal have no C library in the bundle, but their objects come from it too.
    for (const [name, target] of triples) {
      if (!(target.bare || target.os === "android" || target.os === "ios")) continue;
      const object = join(work, `bundle-${name}.o`);
      const flags = target.bare ? ["--freestanding", "--allocator", "host"] : [];
      cli(["build", answer, "--emit", "object", "--no-cache", "--target", name, ...flags, "-o", object], { env: cross });
      check(name, object, "object");
    }
    console.log("cross link: every target the bundled toolchain can link was linked and inspected");
  }

  if (crossExecute) {
    // qemu-user runs the static musl executables of the other architectures; the runner hook starts it.
    const qemu = { aarch64: "qemu-aarch64", x86_64: "qemu-x86_64", riscv64: "qemu-riscv64" };
    const cross = { TSUZURI_CLANG: crossClang };
    assert.equal(process.platform, "linux", "TSUZURI_CROSS_EXECUTE=1 runs qemu-user, which is Linux only");
    for (const [name, target] of triples) {
      if (target.libc !== "musl" || target.arch === hostArch) continue;
      const program = [qemu[target.arch], `${qemu[target.arch]}-static`].find(tool);
      assert.ok(program, `TSUZURI_CROSS_EXECUTE=1 needs ${qemu[target.arch]} (apt install qemu-user) to run ${name}`);
      const env = { ...cross, TSUZURI_RUNNER: program };
      for (const optimization of ["-O0", "-O3"]) {
        assert.equal(cli(["run", "examples/control", "--no-cache", optimization, "--target", name], { env }).stdout, expected.control, `${name} ${optimization}`);
        assert.equal(cli(["run", "examples/tasks", "--no-cache", optimization, "--target", name], { env }).stdout, expected.tasks, `${name} ${optimization}`);
        assert.match(cli(["test", suite, optimization, "--target", name, "--json"], { env }).stdout, /"passed":2,"failed":0/, `${name} ${optimization}`);
      }
      record(name, "executed", `under ${program} (TSUZURI_RUNNER)`);
    }
  }

  // The aliases of WebAssembly select the same code generators, byte for byte.
  for (const [alias, plain] of [["wasm32-unknown-unknown", "wasm32"], ["wasm64-unknown-unknown", "wasm64"]]) {
    const first = join(work, `${plain}.wasm`);
    const second = join(work, `${alias}.wasm`);
    cli(["build", answer, "--target", plain, "--no-cache", "-o", first]);
    cli(["build", answer, "--target", alias, "--no-cache", "-o", second]);
    assert.ok(readFileSync(first).equals(readFileSync(second)), alias);
  }

  // The refusals, as `--json` reports them.
  const list = triples.map(([name]) => name).join(", ");
  let error = failure(["build", "examples/control", "--target", "x86_64-unknown-haiku"]);
  assert.deepEqual([error.status, error.code, error.path], [2, "E2000", "<command line>"]);
  assert.equal(error.message, `unknown target 'x86_64-unknown-haiku'; use native, wasm32, wasm64, or one of ${list}`);
  error = failure(["build", "examples/control", "--target", "i686-pc-windows-msvc"]);
  assert.equal(error.message, `unknown target 'i686-pc-windows-msvc'; 32-bit native targets are not supported because native code generation assumes 64-bit pointers; use native, wasm32, wasm64, or one of ${list}`);
  const foreign = triples.find(([name, target]) => name !== host && target.os === "linux" && target.libc === "gnu")[0];
  error = failure(["test", suite, "--target", foreign]);
  assert.deepEqual([error.status, error.code], [1, "E2000"]);
  assert.equal(error.message, `cannot run ${foreign} tests on this machine; run 'tsuzuri test' on a ${foreign} machine, set TSUZURI_RUNNER to a command that runs ${foreign} executables (such as qemu-user, Wine, or an emulator wrapper), or use 'tsuzuri build --target ${foreign}'`);
  error = failure(["run", "examples/control", "--target", foreign]);
  assert.match(error.message, new RegExp(`^cannot run ${foreign} programs on this machine; run the program on a ${foreign} machine, set TSUZURI_RUNNER`));
  error = failure(["build", "examples/control", "--target", foreign, "--cpu", "native"]);
  assert.deepEqual([error.status, error.code], [2, "E2000"]);
  assert.equal(error.message, `'--cpu native' requires the host target; use '--cpu generic' when cross-compiling to ${foreign}`);
  error = failure(["build", "examples/control", "--target", "x86_64-unknown-none"]);
  assert.equal(error.message, "x86_64-unknown-none has no operating system: build it with --freestanding --allocator host and --emit object, llvm, or header");
  error = failure(["run", "examples/control", "--target", "wasm32"]);
  assert.equal(error.code, "E2000");
  // The runtimes a target lacks are refused before any tool runs, whatever the host.
  const os = join(work, "os");
  mkdirSync(os);
  writeFileSync(join(os, "Main.tz"), 'def main :: unit -> i32 = \\() ->\n    let! home = Env.var "HOME"\n    0\n');
  error = failure(["build", os, "--target", "aarch64-apple-ios", "--no-cache", "-o", join(work, "os-ios")]);
  assert.deepEqual([error.status, error.code], [1, "E2002"]);
  assert.match(error.message, /not available for aarch64-apple-ios: src\/runtime\/os\.c is written for the headers of macOS, Linux, and FreeBSD/);
  error = failure(["build", os, "--target", "aarch64-linux-android", "--no-cache", "-o", join(work, "os-android")]);
  assert.deepEqual([error.status, error.code], [1, "E2002"]);
  assert.match(error.message, /not available for aarch64-linux-android/);
  error = failure(["build", os, "--target", "x86_64-pc-windows-gnu", "--no-cache", "-o", join(work, "os-windows")]);
  assert.match(error.message, /not supported on Windows yet \(G10\)/);
  // A freestanding object of a bare-metal target links no C library.
  const bare = join(work, "bare-metal.ll");
  cli(["build", answer, "--emit", "llvm", "--target", "x86_64-unknown-none", "--freestanding", "--allocator", "host", "-o", bare]);
  assert.match(readFileSync(bare, "utf8"), /^define [^\n]* noredzone \{$/m);

  const lines = triples.map(([name]) => {
    const found = outcome.get(name);
    return found ? `${name}: ${found.kind === "object" ? "object only" : found.kind === "linked" ? "linked only" : "executed"} (${found.notes.join("; ")})` : `${name}: not checked`;
  });
  console.log(lines.join("\n"));
  const count = (kind) => [...outcome.values()].filter((found) => found.kind === kind).length;
  console.log(`targets: ${count("executed")} executed, ${count("linked")} linked only, ${count("object")} object only, ${triples.length - outcome.size} not checked`);
  if (strict) assert.equal(outcome.size, triples.length, "every target is checked where a tool is required");
} finally {
  rmSync(work, { recursive: true, force: true });
}
