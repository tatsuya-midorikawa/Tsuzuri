import assert from "node:assert/strict";
import { existsSync, linkSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { peHeaders, peResources, unwindFunctions, writeTestRes } from "./windows-pe.mjs";

const selfPath = fileURLToPath(import.meta.url);
const repository = resolve(dirname(selfPath), "..");
const isMain = Boolean(process.argv[1]) && resolve(process.argv[1]) === selfPath;

function currentClang() {
  return process.env.TSUZURI_CLANG ?? "clang";
}

function isBundledClang(clangBin = currentClang()) {
  return /tsuzuri-clang(?:\.exe)?$/i.test(clangBin);
}

function msvcNativeFlags() {
  const named = `${process.arch === "arm64" ? "aarch64" : "x86_64"}-pc-windows-msvc`;
  return [`--target=${named}`, "-fuse-ld=lld", "-Wno-override-module", "-D_CRT_SECURE_NO_WARNINGS"];
}

function cCompileFlags(clangBin = currentClang()) {
  return isBundledClang(clangBin) ? ["-D_CRT_SECURE_NO_WARNINGS"] : msvcNativeFlags();
}

function execute(program, args, success = true, env = {}, input = undefined) {
  const result = spawnSync(program, args, { input, encoding: "utf8", timeout: 180000, env: { ...process.env, ...env }, maxBuffer: 8 * 1024 * 1024 });
  assert.ifError(result.error);
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}

let sharedWorkDir = null;
let sharedLauncher = null;
let sharedShimObj = null;
let buildCounter = 0;

function ensureSharedWorkDir() {
  if (!sharedWorkDir) {
    sharedWorkDir = mkdtempSync(join(tmpdir(), "tsuzuri-win-gui-"));
    process.on("exit", () => {
      try { rmSync(sharedWorkDir, { recursive: true, force: true }); } catch {}
    });
  }
  return sharedWorkDir;
}

function ensureLauncher(workDir = ensureSharedWorkDir(), clangBin = currentClang()) {
  if (sharedLauncher && existsSync(sharedLauncher)) return sharedLauncher;
  const out = join(workDir, "windows_launch.exe");
  execute(clangBin, [...cCompileFlags(clangBin), "-O1", join(repository, "tests/windows_launch.c"), "-o", out]);
  sharedLauncher = out;
  return out;
}

export function launchWithoutHandles(exe, args = [], options = {}) {
  if (process.platform !== "win32") throw new Error("launchWithoutHandles requires a Windows host");
  const launcher = options.launcher ?? ensureLauncher(options.directory, options.clang);
  const launchArgs = ["--no-handles"];
  if (options.firstException) launchArgs.push("--first-exception");
  if (options.debugStrings) launchArgs.push("--debug-strings", options.debugStrings);
  if (options.timeoutMs !== undefined) launchArgs.push("--timeout-ms", String(options.timeoutMs));
  launchArgs.push("--", exe, ...args);
  const result = execute(launcher, launchArgs, true, options.env ?? {});
  const lastLine = result.stdout.trim().split(/\r?\n/).pop() ?? "";
  assert.notEqual(lastLine, "timeout", `child timed out: ${exe} ${args.join(" ")}`);
  const match = lastLine.match(/^exit=0x([0-9a-fA-F]{8})$/);
  assert.ok(match, `unexpected windows_launch output for ${exe}:\n${result.stdout}\n${result.stderr}`);
  return Number.parseInt(match[1], 16) >>> 0;
}

export function buildGui(fixture, flags = [], options = {}) {
  if (process.platform !== "win32") throw new Error("buildGui requires a Windows host");
  const workDir = options.directory ?? ensureSharedWorkDir();
  const compilerBin = options.compiler ?? resolve(process.argv[2] ?? join(repository, "target/release/tsuzuri.exe"));
  const clangBin = options.clang ?? currentClang();
  if (!sharedShimObj || !existsSync(sharedShimObj)) {
    const obj = join(workDir, "windows_gui_shim.obj");
    execute(clangBin, [...cCompileFlags(clangBin), "-c", "-O1", join(repository, "tests/windows_gui_shim.c"), "-o", obj]);
    sharedShimObj = obj;
  }
  const fixturePath = existsSync(fixture) ? fixture : join(repository, "tests/fixtures/windows_gui", fixture);
  const out = options.output ?? join(workDir, `gui-${++buildCounter}.exe`);
  execute(compilerBin, ["build", fixturePath, "--no-cache", "--link", sharedShimObj, "-l", "user32", ...flags, "-o", out], true, options.env ?? {});
  return out;
}

function runGuiStage(compiler, clang, directory) {
  sharedLauncher = null;
  sharedShimObj = null;
  const toolchainLabel = isBundledClang(clang) ? "bundled MinGW" : "LLVM MSVC";
  const cFlags = cCompileFlags(clang);
  const launcher = ensureLauncher(directory, clang);

  // 1. launch_without_handles
  const probeOut = join(directory, "gui-probe.txt");
  assert.equal(launchWithoutHandles(launcher, ["--probe", probeOut], { launcher }), 0);
  const probeLines = readFileSync(probeOut, "utf8").trim().split(/\r?\n/);
  assert.equal(probeLines.length, 3);
  for (let fd = 0; fd < 3; fd++) {
    assert.match(probeLines[fd], new RegExp(`^fd${fd}=-2 handle${fd}=(?:0+|0x0+|\\(null\\))$`, "i"));
  }

  // 2. debug_strings
  const probeDebug = join(directory, "gui-probe-debug.txt");
  assert.equal(launchWithoutHandles(launcher, ["--probe-debug"], { launcher, debugStrings: probeDebug }), 0);
  assert.deepEqual(readFileSync(probeDebug, "utf8").trimEnd().split("\n"), ["probe 1", "probe 2"]);

  // 3. launch_exit_codes
  for (const code of [0, 7, 255]) {
    const exitDir = join(directory, `gui-exit-${code}`);
    mkdirSync(exitDir, { recursive: true });
    writeFileSync(join(exitDir, "Main.tz"), `def main :: unit -> i32 = \\() -> ${code}\n`);
    const exitExe = join(directory, `gui-exit-${code}.exe`);
    execute(compiler, ["build", exitDir, "--no-cache", "-o", exitExe]);
    assert.equal(launchWithoutHandles(exitExe, [], { launcher }), code);
  }

  const allowedTrapCodes = [0xc000001d];
  const dispatchedTrapCodes = [0xc000001d];
  const kernelTrapCodes = [0xc000001d];

  for (const optimization of ["-O0", "-O3"]) {
    // 4. io_without_handles (W08: try_writeln -> Unavailable (13), try_readln -> Unavailable (24), Debug.print -> OutputDebugStringW)
    const ioExe = join(directory, `gui-io${optimization}.exe`);
    execute(compiler, ["build", join(repository, "tests/fixtures/windows_gui/io"), "--no-cache", optimization, "-o", ioExe]);
    assert.equal(execute(ioExe, [], false).status, 10, `io with handles ${optimization}`);
    assert.equal(launchWithoutHandles(ioExe, [], { launcher }), 13, `io without handles ${optimization}`);

    const ioReadExe = join(directory, `gui-io-read${optimization}.exe`);
    execute(compiler, ["build", join(repository, "tests/fixtures/windows_gui/io_read"), "--no-cache", optimization, "-o", ioReadExe]);
    assert.equal(launchWithoutHandles(ioReadExe, [], { launcher }), 24, `io_read without handles ${optimization}`);

    const debugGuiExe = join(directory, `gui-debug${optimization}.exe`);
    execute(compiler, ["build", join(repository, "tests/fixtures/windows_gui/debug_out"), "--no-cache", optimization, "-o", debugGuiExe]);
    const debugGuiOds = join(directory, `gui-debug-ods${optimization}.txt`);
    assert.equal(launchWithoutHandles(debugGuiExe, [], { launcher, debugStrings: debugGuiOds }), 5, `debug without handles ${optimization}`);
    assert.deepEqual(readFileSync(debugGuiOds, "utf8").trimEnd().split("\n"), ["note"]);

    const ioTrapExe = join(directory, `gui-io-trap${optimization}.exe`);
    execute(compiler, ["build", join(repository, "tests/fixtures/windows_gui/io_trap"), "--no-cache", optimization, "-o", ioTrapExe]);
    assert.equal(execute(ioTrapExe, [], false).status, 0, `io_trap with handles ${optimization}`);
    const ioTrapCode = launchWithoutHandles(ioTrapExe, [], { launcher });
    assert.ok(allowedTrapCodes.includes(ioTrapCode), `io_trap without handles ${optimization}: 0x${ioTrapCode.toString(16)}`);

    // 5. message_window
    const pumpExe = buildGui("pump", [optimization], { compiler, clang, directory });
    assert.equal(execute(pumpExe, [], false).status, 3, `pump with handles ${optimization}`);
    assert.equal(launchWithoutHandles(pumpExe, [], { launcher }), 3, `pump without handles ${optimization}`);

    // 6. message_window_reentry
    const reentryExe = buildGui("reentry", [optimization], { compiler, clang, directory });
    assert.equal(execute(reentryExe, [], false).status, 2, `reentry with handles ${optimization}`);
    assert.equal(launchWithoutHandles(reentryExe, [], { launcher }), 2, `reentry without handles ${optimization}`);

    // 7. tzroutine_wakes_pump
    const wakeExe = buildGui("wake", [optimization], { compiler, clang, directory });
    for (const procs of ["1", "4"]) {
      const env = { TSUZURI_MAXPROCS: procs };
      assert.equal(execute(wakeExe, [], false, env).status, 1, `wake with handles ${optimization} P=${procs}`);
      assert.equal(launchWithoutHandles(wakeExe, [], { launcher, env }), 1, `wake without handles ${optimization} P=${procs}`);
    }

    // 8. trap_in_dispatched_callback
    const trapTimerExe = buildGui("trap_timer", [optimization], { compiler, clang, directory });
    const timerMarker = join(directory, `marker-timer${optimization}.txt`);
    const timerCode = launchWithoutHandles(trapTimerExe, [timerMarker], { launcher });
    assert.ok(dispatchedTrapCodes.includes(timerCode), `trap_timer ${optimization}: 0x${timerCode.toString(16)}`);
    assert.ok(!existsSync(timerMarker), `trap_timer marker should not exist (${optimization})`);

    // 9. trap_in_kernel_callback
    const trapCreateExe = buildGui("trap_create", [optimization], { compiler, clang, directory });
    const createMarker = join(directory, `marker-create${optimization}.txt`);
    const createCode = launchWithoutHandles(trapCreateExe, [createMarker], { launcher });
    assert.ok(kernelTrapCodes.includes(createCode), `trap_create ${optimization}: 0x${createCode.toString(16)}`);
    assert.ok(!existsSync(createMarker), `trap_create marker should not exist (${optimization})`);

    // W07: SetTimer TIMERPROC callback trap in a Tsuzuri executable is caught by VEH and terminates with 0xC000001D.
    const trapTimerProcExe = buildGui("trap_timer_proc", [optimization], { compiler, clang, directory });
    const timerProcMarker = join(directory, `marker-timerproc${optimization}.txt`);
    assert.equal(launchWithoutHandles(trapTimerProcExe, [timerProcMarker], { launcher }), 0xc000001d, `trap_timer_proc ${optimization}`);
    assert.ok(!existsSync(timerProcMarker), `trap_timer_proc marker should not exist (${optimization})`);

    // W12: RegisterClassExW static WNDPROC (WM_CREATE, SendMessageW, PostMessageW, WM_CLOSE -> DestroyWindow -> WM_DESTROY reentry),
    // SetTimer TIMERPROC, TrySubmitThreadpoolCallback boundary wait, outside pseudo-G deadlock, and post-main callback trap.
    const w12WndExe = buildGui("w12_wndclass", [optimization], { compiler, clang, directory });
    assert.equal(launchWithoutHandles(w12WndExe, [], { launcher }), 42, `w12_wndclass ${optimization}`);

    const w12DeadExe = buildGui("w12_deadlock", [optimization], { compiler, clang, directory });
    const w12DeadOds = join(directory, `w12-dead${optimization}.txt`);
    assert.equal(launchWithoutHandles(w12DeadExe, [], { launcher, debugStrings: w12DeadOds }), 0xc000001d, `w12_deadlock ${optimization}`);
    assert.match(readFileSync(w12DeadOds, "utf8"), /all tzroutines are asleep - deadlock!/);

    const w12AfterExe = buildGui("w12_after_main", [optimization], { compiler, clang, directory });
    const w12AfterOds = join(directory, `w12-after${optimization}.txt`);
    assert.equal(launchWithoutHandles(w12AfterExe, [], { launcher, debugStrings: w12AfterOds }), 0xc000001d, `w12_after_main ${optimization}`);
    assert.match(readFileSync(w12AfterOds, "utf8"), /a callback was called after main returned/);

    // W07: C host linking `--emit object` (no VEH) terminates with 0xC000001D in both TIMERPROC and WM_CREATE.
    const tickDir = join(directory, `tick-obj${optimization}`);
    mkdirSync(tickDir, { recursive: true });
    writeFileSync(join(tickDir, "Main.tz"), "export def on_tick :: i64 -> i64\nfn on_tick divisor = 10 / divisor\n");
    for (const trapInfo of [false, true]) {
      const tag = `${optimization}${trapInfo ? "-info" : ""}`;
      const tickObj = join(directory, `tick${tag}.obj`);
      execute(compiler, ["build", tickDir, "--no-cache", optimization, ...(trapInfo ? ["--trap-info"] : []), "--emit", "object", "-o", tickObj]);
      const hostExe = join(directory, `trap-host${tag}.exe`);
      execute(clang, [...cFlags, optimization, join(repository, "tests/windows_trap_host.c"), tickObj, "-luser32", "-o", hostExe]);
      if (trapInfo) {
        const ods = join(directory, `ods-host${tag}.txt`);
        assert.equal(launchWithoutHandles(hostExe, ["timer"], { launcher, debugStrings: ods }), 0xc000001d);
        assert.match(readFileSync(ods, "utf8"), /trap: integer division by zero/);
      } else {
        assert.equal(launchWithoutHandles(hostExe, ["timer"], { launcher }), 0xc000001d);
      }
      assert.equal(launchWithoutHandles(hostExe, ["create"], { launcher }), 0xc000001d);
    }
  }

  // 10. pe_headers & W09 (--subsystem windows / [windows] subsystem)
  const consoleCheckExe = join(directory, "pe-console.exe");
  const peConsoleDir = join(directory, "pe-console");
  mkdirSync(peConsoleDir, { recursive: true });
  writeFileSync(join(peConsoleDir, "Main.tz"), "def main :: unit -> i32 = \\() -> 7\n");
  execute(compiler, ["build", peConsoleDir, "--no-cache", "-o", consoleCheckExe]);
  const consolePe = peHeaders(consoleCheckExe);
  assert.equal(consolePe.subsystem, 3);
  assert.equal(consolePe.machine, process.arch);

  const explicitConsoleExe = join(directory, "pe-explicit-console.exe");
  execute(compiler, ["build", peConsoleDir, "--no-cache", "--subsystem", "console", "-o", explicitConsoleExe]);
  assert.equal(peHeaders(explicitConsoleExe).subsystem, 3);

  const cliGuiExe = join(directory, "pe-cli-gui.exe");
  execute(compiler, ["build", peConsoleDir, "--no-cache", "--subsystem", "windows", "-o", cliGuiExe]);
  const cliGuiPe = peHeaders(cliGuiExe);
  assert.equal(cliGuiPe.subsystem, 2);
  assert.equal(cliGuiPe.machine, process.arch);
  assert.equal(launchWithoutHandles(cliGuiExe, [], { launcher }), 7);
  assert.equal(execute(cliGuiExe, [], false).status, 7);

  // Manifest `[windows] subsystem = "windows"`, CLI override `--subsystem console`, `run`, `test`, and build cache.
  const manifestGuiDir = join(directory, "manifest-gui");
  mkdirSync(manifestGuiDir, { recursive: true });
  writeFileSync(join(manifestGuiDir, "Tsuzuri.toml"), '[package]\nname = "gui-app"\nversion = "0.1.0"\nedition = "2027"\n\n[windows]\nsubsystem = "windows"\n');
  writeFileSync(join(manifestGuiDir, "Main.tz"), 'def main :: unit -> i32 = \\() ->\n    let! res = IO.try_writeln "hi"\n    match res with\n    | Result.Ok () -> 0\n    | Result.Error IO.Unavailable -> 13\n    | Result.Error _ -> 11\n\ntest "runner stays console" = assert (1 + 1 == 2)\n');
  const subsysCacheDir = join(directory, "subsys-cache");
  mkdirSync(subsysCacheDir, { recursive: true });
  const cacheEnv = { TSUZURI_CACHE_DIR: subsysCacheDir };
  const manifestGuiExe = join(directory, "manifest-gui.exe");
  execute(compiler, ["build", manifestGuiDir, "-o", manifestGuiExe], true, cacheEnv);
  assert.equal(peHeaders(manifestGuiExe).subsystem, 2);
  assert.equal(launchWithoutHandles(manifestGuiExe, [], { launcher }), 13);
  execute(compiler, ["build", manifestGuiDir, "-o", manifestGuiExe], true, cacheEnv);
  const countCacheEntries = () => readdirSync(subsysCacheDir).filter((name) => /^[0-9a-f]{64}$/.test(name)).length;
  assert.equal(countCacheEntries(), 1);
  const overrideConsoleExe = join(directory, "manifest-override.exe");
  execute(compiler, ["build", manifestGuiDir, "--subsystem", "console", "-o", overrideConsoleExe], true, cacheEnv);
  assert.equal(peHeaders(overrideConsoleExe).subsystem, 3);
  assert.equal(countCacheEntries(), 2);
  assert.equal(execute(compiler, ["run", manifestGuiDir, "--subsystem", "windows"]).stdout, "hi\n");
  execute(compiler, ["test", manifestGuiDir]);

  const guiProbeC = join(directory, "pe_gui_probe.c");
  writeFileSync(guiProbeC, "int main(void) { return 0; }\n");
  const guiProbeExe = join(directory, "pe-gui.exe");
  const guiSubsysFlags = isBundledClang(clang)
    ? ["-Wl,--subsystem,windows"]
    : ["-Xlinker", "/subsystem:windows", "-Xlinker", "/entry:mainCRTStartup"];
  execute(clang, [...cFlags, ...guiSubsysFlags, guiProbeC, "-o", guiProbeExe]);
  const guiPe = peHeaders(guiProbeExe);
  assert.equal(guiPe.subsystem, 2);
  assert.equal(guiPe.machine, process.arch);

  // 11. pe_resources
  const resFile = join(directory, "test.res");
  writeTestRes(resFile, [
    { type: 24, name: 1, language: 1033, data: "<assembly xmlns=\"urn:schemas-microsoft-com:asm.v1\" manifestVersion=\"1.0\"/>" },
    { type: 16, name: 1, language: 1033, data: "version-payload" },
  ]);
  const resExe = join(directory, "pe-res.exe");
  execute(clang, [...cFlags, guiProbeC, resFile, "-o", resExe]);
  const resources = peResources(resExe);
  assert.ok(resources.some((r) => r.type === 24 && r.name === 1 && r.language === 1033), JSON.stringify(resources));
  assert.ok(resources.some((r) => r.type === 16 && r.name === 1 && r.language === 1033), JSON.stringify(resources));

  // 12. unwind_functions
  const unwindC = join(directory, "unwind_probe.c");
  writeFileSync(unwindC, "extern int tz_ext_callee(int);\nint tz_probe_nonleaf(int x) { return tz_ext_callee(x) + 1; }\n");
  const unwindCObj = join(directory, "unwind_probe.obj");
  execute(clang, [...cFlags, "-c", "-O1", unwindC, "-o", unwindCObj]);
  const cUnwind = unwindFunctions(unwindCObj);
  assert.ok(cUnwind.has("tz_probe_nonleaf"), `expected tz_probe_nonleaf in ${[...cUnwind].join(", ")}`);

  const unwindTzDir = join(directory, "unwind-tz");
  mkdirSync(unwindTzDir, { recursive: true });
  writeFileSync(join(unwindTzDir, "Main.tz"), "export def rec fib :: i64 -> i64\nfn rec fib n = if n <= 1 then n else fib (n - 1) + fib (n - 2)\n");
  const unwindTzObj = join(directory, "unwind_tz.obj");
  execute(compiler, ["build", unwindTzDir, "--no-cache", "-O3", "--emit", "object", "-o", unwindTzObj]);
  const tzUnwind = unwindFunctions(unwindTzObj);
  assert.ok(tzUnwind.has("tz.fn.Main.fib") && tzUnwind.has("tz.trap.fail"), `expected Tsuzuri unwind entries in ${[...tzUnwind].join(", ")}`);

  // W07: stack overflow without standard handles sends `trap: stack overflow` to OutputDebugStringW and exits 0xC00000FD.
  const deepDir = join(directory, "deep-overflow");
  mkdirSync(deepDir, { recursive: true });
  writeFileSync(join(deepDir, "Main.tz"), "def rec depth :: i64 -> i64\nfn rec depth n = if n == 0 then 0 else depth (n - 1) * 3 + n\ndef main :: unit -> i32 = \\() -> (depth 100000000) as i32\n");
  const deepExe = join(directory, "deep-overflow.exe");
  execute(compiler, ["build", deepDir, "--no-cache", "-O0", "-o", deepExe]);
  const deepOds = join(directory, "deep-ods.txt");
  assert.equal(launchWithoutHandles(deepExe, [], { launcher, debugStrings: deepOds }), 0xc00000fd);
  assert.deepEqual(readFileSync(deepOds, "utf8").trimEnd().split("\n"), ["trap: stack overflow"]);

  // W07: VEH ignores non-ud2 illegal instructions (such as ud1 on x86_64) caught by C SEH `__try`/`__except`.
  if (!isBundledClang(clang) && process.arch === "x64") {
    const ud1Dir = join(directory, "ud1-seh");
    mkdirSync(ud1Dir, { recursive: true });
    const ud1C = join(directory, "ud1_seh.c");
    writeFileSync(ud1C, [
      "#include <windows.h>",
      "#include <stdint.h>",
      "__attribute__((noinline)) static void raise_ud1(void) {",
      "  __asm__ volatile(\"ud1 %%eax, %%eax\" : : : \"eax\");",
      "}",
      "int64_t w07_catch_ud1(void) {",
      "  __try {",
      "    raise_ud1();",
      "    return -1;",
      "  } __except (GetExceptionCode() == EXCEPTION_ILLEGAL_INSTRUCTION ? EXCEPTION_EXECUTE_HANDLER : EXCEPTION_CONTINUE_SEARCH) {",
      "    return 42;",
      "  }",
      "}",
      "",
    ].join("\n"));
    const ud1Obj = join(directory, "ud1_seh.obj");
    execute(clang, [...cFlags, "-c", "-O1", ud1C, "-o", ud1Obj]);
    writeFileSync(join(ud1Dir, "Main.tz"), "@trusted\nextern \"w07_catch_ud1\" def catch_ud1 :: unit -> i64\ndef main :: unit -> i32 = \\() -> (catch_ud1 ()) as i32\n");
    const ud1Exe = join(directory, "ud1-seh.exe");
    execute(compiler, ["build", ud1Dir, "--no-cache", "--link", ud1Obj, "-o", ud1Exe]);
    assert.equal(launchWithoutHandles(ud1Exe, [], { launcher }), 42);
  }

  console.log(`Windows GUI: Tsuzuri object unwind entries (${toolchainLabel}): ${tzUnwind.size}`);
  console.log(`Windows GUI: launcher, message-only windows, callbacks, tzroutine wake and PE checks passed with ${toolchainLabel}`);
}

if (isMain) {
  if (process.platform !== "win32") throw new Error("Windows E2E requires a Windows host; use TSUZURI_WINDOWS_SDK cargo test --test windows for cross-link validation");
  const args = process.argv.slice(2);
  let stage = null;
  const positional = [];
  for (let i = 0; i < args.length; i++) {
    if (args[i] === "--stage") {
      stage = args[++i];
    } else {
      positional.push(args[i]);
    }
  }
  const compiler = resolve(positional[0] ?? join(repository, "target/release/tsuzuri.exe"));
  for (const name of ["TSUZURI_CLANG", "TSUZURI_WASM_LD"]) {
    if (/[\\/]/.test(process.env[name] ?? "")) process.env[name] = resolve(process.env[name]);
  }
  process.chdir(repository);
  const clang = currentClang();
  assert.ok(["x64", "arm64"].includes(process.arch), `unsupported Windows architecture: ${process.arch}`);
  const named = `${process.arch === "arm64" ? "aarch64" : "x86_64"}-pc-windows-msvc`;
  const directory = mkdtempSync(join(tmpdir(), "tsuzuri-windows-"));
  const nativeFlags = msvcNativeFlags();
  const cli = (cliArgs, success, env, input) => execute(compiler, cliArgs, success, env, input);
  // Warnings come first on stderr as JSON lines; the error that --json reports is the last one.
  const diagnostic = (result) => JSON.parse(result.stderr.trim().split("\n").filter((line) => line.startsWith("{")).pop());
  try {
    if (stage === "gui") {
      runGuiStage(compiler, clang, directory);
    } else if (stage !== null) {
      throw new Error(`unknown --stage '${stage}'; expected gui`);
    } else {
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

  const nullableWin = join(directory, "nullable-win");
  mkdirSync(nullableWin);
  writeFileSync(join(nullableWin, "Main.tz"), [
    "extern type Module",
    "extern type ModuleName",
    "extern type Slot",
    "@trusted",
    'extern "GetModuleHandleW" def module_handle :: Maybe<ModuleName> -> Maybe<Module>',
    "@trusted",
    'extern "TlsAlloc" def tls_alloc :: unit -> i32u',
    "@trusted",
    'extern "TlsGetValue" def tls_get :: i32u -> Maybe<Slot>',
    "def loaded :: Maybe<Module> -> i32 = \\m -> match m with | Some _ -> 1 | None -> 0",
    "def empty :: Maybe<Slot> -> i32 = \\s -> match s with | Some _ -> 0 | None -> 2",
    "def main :: unit -> i32 = \\() -> loaded (module_handle None) + empty (tls_get (tls_alloc ()))",
    "",
  ].join("\n"));
  for (const optimization of ["-O0", "-O3"]) {
    const nullExe = join(directory, `nullable-win${optimization}.exe`);
    cli(["build", nullableWin, "--no-cache", optimization, "-o", nullExe]);
    assert.equal(execute(nullExe, [], false).status, 3, `nullable-win ${optimization}`);
  }
  console.log("Windows MSVC: Maybe<H> NULL ABI passed");
  runGuiStage(compiler, clang, directory);
  console.log("Windows MSVC: native O0/O3, tzroutines and Parallel, linked IR, UTF-8, hardlinks, atomic replacement, object ABI and WASM passed");
    }
  } finally {
    rmSync(directory, { recursive: true, force: true, maxRetries: 10, retryDelay: 50 });
  }
}
