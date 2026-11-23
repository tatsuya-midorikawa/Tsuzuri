import assert from "node:assert/strict";
import { linkSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";

if (process.platform !== "win32") throw new Error("Windows E2E requires a Windows host; use TSUZURI_WINDOWS_SDK cargo test --test windows for cross-link validation");
const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri.exe");
const clang = process.env.TSUZURI_CLANG ?? "clang";
assert.ok(["x64", "arm64"].includes(process.arch), `unsupported Windows architecture: ${process.arch}`);
const named = `${process.arch === "arm64" ? "aarch64" : "x86_64"}-pc-windows-msvc`;
const directory = mkdtempSync(join(tmpdir(), "tsuzuri-windows-"));
const nativeFlags = [`--target=${named}`, "-fuse-ld=lld", "-Wno-override-module", "-D_CRT_SECURE_NO_WARNINGS"];
function execute(program, args, success = true, env = {}, input = undefined) {
  const result = spawnSync(program, args, { input, encoding: "utf8", timeout: 180000, env: { ...process.env, ...env }, maxBuffer: 8 * 1024 * 1024 });
  assert.ifError(result.error);
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}
const cli = (args, success, env, input) => execute(compiler, args, success, env, input);
// Warnings come first on stderr as JSON lines; the error that --json reports is the last one.
const diagnostic = (result) => JSON.parse(result.stderr.trim().split("\n").filter((line) => line.startsWith("{")).pop());
try {
  const project = join(directory, "console");
  mkdirSync(project);
  const input = join(project, "Main.tz");
  writeFileSync(input, 'let text = "\u65e5\u672c\u8a9e"\nDebug.print (ref text)\n42\n');
  for (const optimization of [0, 3]) {
    const flags = [`-O${optimization}`];
    const run = cli(["run", input, ...flags]);
    assert.equal(run.stdout, "42\n");
    assert.ok(run.stderr.includes("\u65e5\u672c\u8a9e\n"));
    const executable = join(directory, `console-${optimization}.exe`);
    cli(["build", input, "-o", executable, ...flags]);
    cli(["build", input, "-o", executable, ...flags]);
    const previous = readFileSync(executable);
    const failed = cli(["build", input, "-o", executable, ...flags, "--json"], false, { TSUZURI_CLANG: join(directory, "missing-clang.exe") });
    assert.equal(diagnostic(failed).code, "E2002");
    assert.deepEqual(readFileSync(executable), previous);
    // Routine.scope and Parallel on the tzroutine scheduler (TZ12); the scheduler's own C tests on fibers are
    // tests/tzroutine_runtime.mjs, which the Windows job runs too.
    assert.equal(cli(["run", "examples/tzroutines", ...flags]).stdout, "21325334000\n");
    assert.equal(cli(["run", "examples/control", ...flags]).stdout, "42\n");
    // The LLVM output of a program that spawns, linked by a host with src/runtime/tzroutine.c.
    const ir = join(directory, `tzroutine-${optimization}.ll`);
    cli(["build", "tests/fixtures/tzroutine/Main.tz", "--emit", "llvm", "--trap-info", "-o", ir, ...flags]);
    assert.match(readFileSync(ir, "utf8"), /define dllexport i64 @tz_sum_atomic/);
    const host = join(directory, "host.c");
    writeFileSync(host, '#include <stdint.h>\n#include <assert.h>\nextern int64_t tz_sum_atomic(int64_t), tz_nested(int64_t), tz_moves_owned(void), tz_go_owned(void), tz_counts(void);\nint main(void) { assert(tz_sum_atomic(100) == 4950); assert(tz_nested(10) == 2047); assert(tz_moves_owned() == 13); assert(tz_go_owned() == 11); assert(tz_counts() == 1); return 0; }\n');
    const spawning = join(directory, `tzroutine-${optimization}.exe`);
    execute(clang, [...nativeFlags, ...flags, host, ir, "src/runtime/tzroutine.c", "-o", spawning]);
    execute(spawning, []);
    const rejected = cli(["build", "tests/fixtures/tzroutine/Main.tz", "--emit", "object", "-o", join(directory, "rejected.obj"), "--json"], false);
    assert.equal(diagnostic(rejected).code, "E2002");
  }
  const alias = join(directory, "source-alias.ll");
  linkSync(input, alias);
  const protectedResult = cli(["build", input, "--emit", "llvm", "-o", alias, "--json"], false);
  assert.equal(diagnostic(protectedResult).code, "E2003");
  const library = join(directory, "library");
  mkdirSync(library);
  writeFileSync(join(library, "Main.tz"), "export def answer :: i64\nfn answer = 42\n");
  const object = join(directory, "answer.obj");
  cli(["build", library, "--emit", "object", "-o", object]);
  const host = join(directory, "answer.c");
  writeFileSync(host, "#include <stdint.h>\nextern int64_t tz_answer(void);\nint main(void) { return tz_answer() == 42 ? 0 : 1; }\n");
  const executable = join(directory, "answer.exe");
  execute(clang, [...nativeFlags, host, object, "-o", executable]);
  execute(executable, []);
  const wasm = join(directory, "answer.wasm");
  cli(["build", library, "--target", "wasm32", "-o", wasm]);
  const { module, instance } = await WebAssembly.instantiate(readFileSync(wasm));
  assert.deepEqual(WebAssembly.Module.imports(module), []);
  assert.equal(instance.exports.tz_answer(), 42n);
  // The machine's own target, named, builds, runs and tests like `--target native`.
  const suite = join(directory, "suite");
  mkdirSync(suite);
  writeFileSync(join(suite, "Main.tz"), 'test "passes" = assert true\ntest "adds" = assert (40 + 2 == 42)\n');
  const unused = { TSUZURI_RUNNER: "tsuzuri-runner-that-does-not-exist" };
  for (const optimization of ["-O0", "-O3"]) {
    for (const [example, output] of [["tzroutines", "21325334000\n"], ["control", "42\n"]]) {
      const executable = join(directory, `${example}-named${optimization}.exe`);
      cli(["build", `examples/${example}`, "--no-cache", optimization, "--target", named, "-o", executable]);
      assert.equal(execute(executable, []).stdout, output, `${example} ${optimization}`);
    }
    assert.equal(cli(["run", "examples/control", optimization, "--no-cache", "--target", named], true, unused).stdout, "42\n");
    assert.match(cli(["test", suite, optimization, "--target", named, "--json"], true, unused).stdout, /"passed":2,"failed":0/);
  }
  console.log(`Windows MSVC: the named target ${named} built, ran and tested at -O0 and -O3`);
  // A trap reports where it happened and ends the process with STATUS_ILLEGAL_INSTRUCTION (`llvm.trap`).
  const trapping = join(directory, "trap");
  mkdirSync(trapping);
  writeFileSync(join(trapping, "Main.tz"), "def divide :: i64 -> i64\nfn divide x = 10 / x\ndivide 0\n");
  for (const optimization of ["-O0", "-O3"]) {
    const trapped = diagnostic(cli(["run", trapping, optimization, "--json"], false));
    assert.equal(trapped.code, "E2005");
    assert.match(trapped.message, /exit code: 0xc000001d/);
    assert.match(trapped.message, /trap: integer division by zero at /);
    assert.equal(trapped.span.line, 2);
  }
  console.log("Windows MSVC: a trap reports its position and ends the process with 0xC000001D at -O0 and -O3");

  // W04: when standard error is missing, `--trap-info` and tzroutine runtime reports fall back to
  // OutputDebugStringW; when standard error is present, nothing is sent to the debugger.
  const launcher = join(directory, "windows_launch.exe");
  execute(clang, [...nativeFlags, "-O1", "tests/windows_launch.c", "-o", launcher]);
  const probeOut = join(directory, "probe.txt");
  const probeRun = execute(launcher, ["--no-handles", "--", launcher, "--probe", probeOut]);
  assert.match(probeRun.stdout.trim(), /^exit=0x00000000$/);
  const probeLines = readFileSync(probeOut, "utf8").trim().split(/\r?\n/);
  assert.equal(probeLines.length, 3);
  for (let fd = 0; fd < 3; fd++) {
    assert.match(probeLines[fd], new RegExp(`^fd${fd}=-2 handle${fd}=(?:0+|0x0+|\\(null\\))$`, "i"));
  }
  const probeDebug = join(directory, "probe-debug.txt");
  assert.match(execute(launcher, ["--no-handles", "--debug-strings", probeDebug, "--", launcher, "--probe-debug"]).stdout.trim(), /^exit=0x00000000$/);
  assert.deepEqual(readFileSync(probeDebug, "utf8").trimEnd().split("\n"), ["probe 1", "probe 2"]);

  const wgProject = join(directory, "wg-negative");
  mkdirSync(wgProject);
  writeFileSync(join(wgProject, "Main.tz"), "def main :: unit -> i32 = \\() ->\n    let group = WaitGroup.create ()\n    WaitGroup.done (ref group)\n    0\n");
  const jpDir = join(directory, "\u65e5\u672c\u8a9e");
  mkdirSync(jpDir);
  writeFileSync(join(jpDir, "Main.tz"), "def divide :: i64 -> i64\nfn divide x = 10 / x\ndivide 0\n");

  for (const optimization of ["-O0", "-O3"]) {
    const trapExe = join(directory, `trap-ods${optimization}.exe`);
    cli(["build", trapping, "--no-cache", optimization, "--trap-info", "-o", trapExe]);
    const odsTrap = join(directory, `ods-trap${optimization}.txt`);
    execute(launcher, ["--no-handles", "--debug-strings", odsTrap, "--", trapExe]);
    const trapLines = readFileSync(odsTrap, "utf8").trimEnd().split("\n");
    assert.equal(trapLines.length, 1, trapLines.join("\n"));
    assert.match(trapLines[0], /^trap: integer division by zero at .*Main\.tz:2:15$/);
    const trapExit = execute(launcher, ["--no-handles", "--", trapExe]);
    assert.match(trapExit.stdout.trim().split(/\r?\n/).pop(), /^exit=0xC000001D$/i);

    // With inherited standard error, the trap report goes to stderr and not to OutputDebugStringW.
    const odsInherited = join(directory, `ods-inherited${optimization}.txt`);
    const inherited = execute(launcher, ["--debug-strings", odsInherited, "--", trapExe]);
    assert.equal(readFileSync(odsInherited, "utf8"), "");
    assert.match(inherited.stderr, /trap: integer division by zero at /);

    const wgExe = join(directory, `wg-ods${optimization}.exe`);
    cli(["build", wgProject, "--no-cache", optimization, "-o", wgExe]);
    const odsWg = join(directory, `ods-wg${optimization}.txt`);
    execute(launcher, ["--no-handles", "--debug-strings", odsWg, "--", wgExe]);
    const wgLines = readFileSync(odsWg, "utf8").trimEnd().split("\n");
    assert.deepEqual(wgLines, ["Tsuzuri runtime: negative WaitGroup counter"]);

    const jpExe = join(directory, `jp-ods${optimization}.exe`);
    cli(["build", jpDir, "--no-cache", optimization, "--trap-info", "-o", jpExe]);
    const odsJp = join(directory, `ods-jp${optimization}.txt`);
    execute(launcher, ["--no-handles", "--debug-strings", odsJp, "--", jpExe]);
    const jpLines = readFileSync(odsJp, "utf8").trimEnd().split("\n");
    assert.equal(jpLines.length, 1);
    assert.ok(jpLines[0].includes("\u65e5\u672c\u8a9e"), jpLines[0]);
  }
  console.log("Windows MSVC: trap and runtime reports fall back to OutputDebugStringW without standard error at -O0 and -O3");

  // W42: 128-bit division, remainder, multiplication and shifts follow the Windows ABI at -O0 and -O3,
  // both inside Tsuzuri programs and in C hosts linked with a Tsuzuri object.
  const wideRun = join(directory, "i128-run");
  mkdirSync(wideRun);
  writeFileSync(join(wideRun, "Main.tz"), [
    "def main :: unit -> i32 = \\() ->",
    "    let! first_line = IO.readln ()",
    "    let! second_line = IO.readln ()",
    "    let first = Maybe.get first_line",
    "    let second = Maybe.get second_line",
    "    let a: i128 = Maybe.get (Parse.parse (ref first))",
    "    let b: i128 = Maybe.get (Parse.parse (ref second))",
    "    let ua: i128u = a as i128u",
    "    let ub: i128u = b as i128u",
    "    do! IO.writeln $\"{a / b} {a % b} {ua / ub} {ua % ub}\"",
    "    0",
    "",
  ].join("\n"));
  const negUnsigned = BigInt.asUintN(128, -100n);
  const expectedNeg = `-14 -2 ${negUnsigned / 7n} ${negUnsigned % 7n}\n`;
  for (const optimization of ["-O0", "-O3"]) {
    assert.equal(cli(["run", wideRun, "--no-cache", optimization], true, {}, "100\n3\n").stdout, "33 1 33 1\n");
    assert.equal(cli(["run", wideRun, "--no-cache", optimization], true, {}, "-100\n7\n").stdout, expectedNeg);
  }

  const wideLib = join(directory, "i128-lib");
  mkdirSync(wideLib);
  writeFileSync(join(wideLib, "Main.tz"), [
    "export def sdiv :: i64 -> i64 -> i64 -> i64",
    "fn sdiv x scale y = (((x as i128) * (scale as i128)) / (y as i128)) as i64",
    "export def smod :: i64 -> i64 -> i64 -> i64",
    "fn smod x scale y = (((x as i128) * (scale as i128)) % (y as i128)) as i64",
    "export def udiv :: i64 -> i64 -> i64 -> i64",
    "fn udiv x scale y = ((((x as i64u) as i128u) * ((scale as i64u) as i128u)) / ((y as i64u) as i128u)) as i64",
    "export def umod :: i64 -> i64 -> i64 -> i64",
    "fn umod x scale y = ((((x as i64u) as i128u) * ((scale as i64u) as i128u)) % ((y as i64u) as i128u)) as i64",
    "",
  ].join("\n"));
  const wideHost = join(directory, "i128_host.c");
  writeFileSync(wideHost, [
    "#undef NDEBUG",
    "#include <assert.h>",
    "#include <stdint.h>",
    "#include <stdio.h>",
    "typedef unsigned __int128 u128;",
    "typedef __int128 s128;",
    "extern u128 __udivmodti4(u128 a, u128 b, u128 *rem);",
    "extern s128 __ashrti3(s128 a, int count);",
    "extern int64_t tz_sdiv(int64_t, int64_t, int64_t);",
    "extern int64_t tz_smod(int64_t, int64_t, int64_t);",
    "extern int64_t tz_udiv(int64_t, int64_t, int64_t);",
    "extern int64_t tz_umod(int64_t, int64_t, int64_t);",
    "static void put128(u128 v) { printf(\"%016llx%016llx \", (unsigned long long)(v >> 64), (unsigned long long)v); }",
    "static int check_exports(void) {",
    "  volatile s128 hundred = 100, three = 3;",
    "  assert(hundred / three == 33);",
    "  assert(tz_sdiv(100, 1, 3) == 33 && tz_smod(100, 1, 3) == 1);",
    "  assert(tz_sdiv(-100, 1, 7) == -14 && tz_smod(-100, 1, 7) == -2);",
    "  assert(tz_sdiv(1000000000000LL, 1000000000000LL, 1000000000000LL) == 1000000000000LL);",
    "  assert(tz_sdiv(-1000000000000LL, 1000000000000LL, 300000000000LL) == -3333333333333LL);",
    "  assert(tz_smod(-1000000000000LL, 1000000000000LL, 300000000000LL) == -100000000000LL);",
    "  assert(tz_udiv(1000000000000LL, 1000000000000LL, 700000LL) == 1428571428571428571LL);",
    "  assert(tz_umod(1000000000000LL, 1000000000000LL, 700000LL) == 300000LL);",
    "  return 0;",
    "}",
    "int main(void) {",
    "  check_exports();",
    "  unsigned long long al, ah, bl, bh; int shift;",
    "  while (scanf(\"%llx %llx %llx %llx %d\", &al, &ah, &bl, &bh, &shift) == 5) {",
    "    u128 a = ((u128)ah << 64) | (u128)al;",
    "    u128 b = ((u128)bh << 64) | (u128)bl;",
    "    s128 sa = (s128)a, sb = (s128)b;",
    "    u128 rem = 0;",
    "    u128 q = __udivmodti4(a, b, &rem);",
    "    u128 q_null = __udivmodti4(a, b, NULL);",
    "    put128(a * b); put128(a / b); put128(a % b);",
    "    put128((u128)(sa / sb)); put128((u128)(sa % sb));",
    "    put128(q); put128(rem); put128(q_null);",
    "    put128(a << shift); put128(a >> shift); put128((u128)__ashrti3(sa, shift));",
    "    putchar('\\n');",
    "  }",
    "  return 0;",
    "}",
    "",
  ].join("\n"));
  const mask128 = (1n << 128n) - 1n;
  const mask64 = (1n << 64n) - 1n;
  const hex128 = (v) => (v & mask128).toString(16).padStart(32, "0");
  const minSigned128 = -(1n << 127n);
  const pairs = [
    [0n, 1n, 0], [1n, 1n, 1], [mask128, 1n, 63],
    [mask128, mask128, 64], [1n << 127n, mask128, 65],
    [1n << 127n, 1n << 127n, 127], [mask64, mask64, 31],
    [BigInt.asUintN(128, -100n), 7n, 5],
    [BigInt.asUintN(128, 100n), BigInt.asUintN(128, -7n), 70],
    [BigInt.asUintN(128, -100n), BigInt.asUintN(128, -7n), 95],
  ];
  let seed = 42n;
  for (let index = 0; index < 100; index++) {
    seed = BigInt.asUintN(128, seed * 0x2360ed051fc65da44385df649fccf645n + 1n);
    const a = seed;
    seed = BigInt.asUintN(128, seed * 0x2360ed051fc65da44385df649fccf645n + 1n);
    let b = seed || 1n;
    if (BigInt.asIntN(128, a) === minSigned128 && BigInt.asIntN(128, b) === -1n) b = 1n;
    pairs.push([a, b, (index * 13) & 127]);
  }
  const stdinLines = pairs.map(([a, b, s]) =>
    `${(a & mask64).toString(16)} ${(a >> 64n).toString(16)} ${(b & mask64).toString(16)} ${(b >> 64n).toString(16)} ${s}`
  ).join("\n") + "\n";
  const expectedWide = pairs.map(([a, b, s]) => {
    const sa = BigInt.asIntN(128, a);
    const sb = BigInt.asIntN(128, b);
    const shift = BigInt(s);
    return [
      a * b, a / b, a % b,
      BigInt.asUintN(128, sa / sb), BigInt.asUintN(128, sa % sb),
      a / b, a % b, a / b,
      (a << shift) & mask128, a >> shift, BigInt.asUintN(128, sa >> shift),
    ].map(hex128).join(" ");
  });
  for (const optimization of ["-O0", "-O3"]) {
    const wideObj = join(directory, `i128${optimization}.obj`);
    cli(["build", wideLib, "--no-cache", optimization, "--emit", "object", "-o", wideObj]);
    const wideExe = join(directory, `i128${optimization}.exe`);
    execute(clang, [...nativeFlags, optimization, wideHost, wideObj, "-o", wideExe]);
    const lines = execute(wideExe, [], true, {}, stdinLines).stdout.replaceAll("\r\n", "\n").trimEnd().split("\n").map((line) => line.trim());
    assert.deepEqual(lines, expectedWide, `i128 reference mismatch at ${optimization}`);
  }
  console.log("Windows MSVC: 128-bit integer helpers match BigInt across -O0 and -O3");

  const copyHwnd = join(directory, "copy-hwnd");
  mkdirSync(copyHwnd);
  writeFileSync(join(copyHwnd, "Main.tz"), [
    "extern type copy Hwnd",
    "@trusted",
    'extern "GetDesktopWindow" def desktop :: unit -> Hwnd',
    "@trusted",
    'extern "IsWindow" def is_window :: Hwnd -> i32',
    "",
    "def main :: unit -> i32 = \\() ->",
    "    let h = desktop ()",
    "    let probe = \\() -> is_window h",
    "    let copied = h",
    "    probe () + is_window h + is_window copied - 2",
    "",
  ].join("\n"));
  for (const optimization of ["-O0", "-O3"]) {
    const hwndExe = join(directory, `copy-hwnd${optimization}.exe`);
    cli(["build", copyHwnd, "--no-cache", optimization, "-l", "user32", "-o", hwndExe]);
    assert.equal(execute(hwndExe, [], false).status, 1, `copy-hwnd ${optimization}`);
  }
  console.log("Windows MSVC: extern type copy Hwnd passed");
  console.log("Windows MSVC: native O0/O3, tzroutines and Parallel, linked IR, UTF-8, hardlinks, atomic replacement, object ABI and WASM passed");
} finally {
  rmSync(directory, { recursive: true, force: true });
}
