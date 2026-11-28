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
    assert.deepEqual(readFileSync(debugGuiOds, "utf8").trimEnd().split("\n"), ["note", "", "tail"]);

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

    // W22: FnPtr<F> stored in a record and passed through a partially applied extern to RegisterClassExW.
    const w22FnPtrExe = buildGui("w22_fnptr", [optimization], { compiler, clang, directory });
    assert.equal(launchWithoutHandles(w22FnPtrExe, [], { launcher }), 42, `w22_fnptr ${optimization}`);

    // W23: Anchor<AppState> in GWLP_USERDATA: WNDPROC updates state, self-SendMessageW hits Busy, and RAII Drop DestroyWindow hits Stale.
    const w23AnchorExe = buildGui("w23_anchor", [optimization], { compiler, clang, directory });
    assert.equal(launchWithoutHandles(w23AnchorExe, [], { launcher }), 42, `w23_anchor ${optimization}`);

    // W13: GetClientRect (ref mut Rect), SetWindowTextW / GetWindowTextW (@count + ref mut [i16u..]), and PeekMessageW (ref mut Msg).
    const w13Win32Exe = buildGui("w13_win32", [optimization], { compiler, clang, directory });
    assert.equal(launchWithoutHandles(w13Win32Exe, [], { launcher }), 42, `w13_win32 ${optimization}`);

    // W20: ZString.literal, ZString.of, Maybe<ref ZString>, lstrlenW, SetWindowTextW, and GetWindowTextW round-trip.
    const w20ZStringExe = buildGui("w20_zstring", [optimization], { compiler, clang, directory });
    assert.equal(launchWithoutHandles(w20ZStringExe, [], { launcher }), 42, `w20_zstring ${optimization}`);

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

  // W19: Win32 GDI `extern type local Hdc` (`GetDC` / `GetDeviceCaps` / `ReleaseDC`) stays confined
  // to the pinned OS thread across yields on 32 concurrent tzroutines with `TSUZURI_MAXPROCS=4`.
  const w19Dir = join(directory, "w19-thread-bound");
  mkdirSync(w19Dir, { recursive: true });
  writeFileSync(join(w19Dir, "Main.tz"), [
    "extern type copy Hwnd",
    "extern type local Hdc",
    "@trusted",
    'extern "GetDesktopWindow" def desktop :: unit -> Hwnd',
    "@trusted",
    'extern "GetDC" def get_dc :: Hwnd -> Maybe<Hdc>',
    "@trusted",
    'extern "GetDeviceCaps" def device_caps :: ref Hdc -> i32 -> i32',
    "@trusted",
    'extern "ReleaseDC" def release_dc :: Hwnd -> own Hdc -> i32',
    "@trusted",
    'extern "GetCurrentThreadId" def current_tid :: unit -> i32u',
    "",
    "def probe_dc :: Hwnd -> i64 = \\hwnd ->",
    "    let tid0 = current_tid ()",
    "    match get_dc hwnd with",
    "    | Maybe.None -> -10",
    "    | Maybe.Some dc ->",
    "        let mut same_tid = true",
    "        for _step = 1 to 8 do",
    "            Routine.yield ()",
    "            if current_tid () != tid0 then same_tid = false else ()",
    "        let bpp = device_caps (ref dc) 12i32",
    "        let rel = release_dc hwnd dc",
    "        if same_tid && bpp >= 0i32 && rel == 1i32 then 1 else -1",
    "",
    "def main :: unit -> i32 = \\() ->",
    "    let hwnd = desktop ()",
    "    let ch: Channel<i64> = Channel.make 32",
    "    for _w = 1 to 32 do",
    "        let out = Channel.share (ref ch)",
    "        tzroutine {",
    "            let ok = Routine.with_locked_thread (\\() -> probe_dc hwnd)",
    "            Channel.send (ref out) ok",
    "        }",
    "    let mut sum = 0",
    "    for _w = 1 to 32 do",
    "        let v = match Channel.recv (ref ch) with | Maybe.Some x -> x | Maybe.None -> 0",
    "        sum = sum + v",
    "    if sum == 32 then 42i32 else 91i32",
    "",
  ].join("\n"));
  for (const optimization of ["-O0", "-O3"]) {
    const w19Exe = join(directory, `w19-thread-bound${optimization}.exe`);
    execute(compiler, ["build", w19Dir, "--no-cache", optimization, "--subsystem", "windows", "-l", "user32", "-l", "gdi32", "-o", w19Exe]);
    for (const procs of ["1", "4"]) {
      const res = execute(launcher, ["--no-handles", "--", w19Exe], true, { TSUZURI_MAXPROCS: procs });
      assert.match(res.stdout.trim().split(/\r?\n/).pop(), /^exit=0x0000002A$/i, `w19 ${optimization} P=${procs}: ${res.stdout}`);
    }
  }
  console.log("Windows GUI: W19 extern type local Hdc (GetDC / GetDeviceCaps / ReleaseDC) passed across P=1 and P=4");

  // W16: `@last_error` captures Win32 GetLastError() and clears stale SetLastError(123) before calls (direct and tzroutine).
  const w16Dir = join(directory, "w16-last-error");
  mkdirSync(w16Dir, { recursive: true });
  writeFileSync(join(w16Dir, "Main.tz"), [
    "@trusted @last_error",
    'extern "CloseHandle" def close_raw :: i64 -> bool * i32u',
    "@trusted @last_error",
    'extern "GetCurrentProcessId" def current_pid :: unit -> i32u * i32u',
    "@trusted",
    'extern "SetLastError" def set_last_error :: i32u -> unit',
    "def probe_once :: unit -> i32u = \\() ->",
    "    let (ok, err_bad) = close_raw 0",
    "    set_last_error 123i32u",
    "    let (pid, err_cleared) = current_pid ()",
    "    if !ok && pid > 0i32u && err_cleared == 0i32u then err_bad else 99i32u",
    "def main :: unit -> i32 = \\() ->",
    "    let direct = probe_once ()",
    "    let ch: Channel<i64> = Channel.make 1",
    "    let out = Channel.share (ref ch)",
    "    tzroutine { Channel.send (ref out) (probe_once () as i64) }",
    "    let spawned = (match Channel.recv (ref ch) with | Maybe.Some x -> x | Maybe.None -> 0) as i32u",
    "    if direct == 6i32u && spawned == 6i32u then 6i32 else 91i32",
    "",
  ].join("\n"));
  execute(compiler, ["check", w16Dir]);
  for (const wasmTarget of ["wasm32", "wasm64"]) {
    const wasmRes = execute(compiler, ["build", w16Dir, "--no-cache", "--target", wasmTarget, "-o", join(directory, `w16-${wasmTarget}.wasm`)], false);
    assert.notEqual(wasmRes.status, 0);
    assert.match(wasmRes.stderr, /error\[E1008\]/, `W16 ${wasmTarget} must fail with E1008:\n${wasmRes.stderr}`);
  }
  for (const optimization of ["-O0", "-O3"]) {
    const w16Exe = join(directory, `w16-last-error${optimization}.exe`);
    execute(compiler, ["build", w16Dir, "--no-cache", optimization, "-o", w16Exe]);
    for (const procs of ["1", "4"]) {
      assert.equal(execute(w16Exe, [], false, { TSUZURI_MAXPROCS: procs }).status, 6, `W16 ${toolchainLabel} ${optimization} P=${procs}`);
    }
  }
  console.log(`Windows (${toolchainLabel}): W16 @last_error (CloseHandle=6, SetLastError(0) clear, tzroutines, WASM E1008) passed`);

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
  const osObjDir = join(directory, "os-obj");
  mkdirSync(osObjDir);
  writeFileSync(join(osObjDir, "Main.tz"), 'def main :: unit -> i32 = \\() ->\n    let! _ = File.read_text "missing.txt"\n    0\n');
  const osObjRejected = cli(["build", osObjDir, "--emit", "object", "--no-cache", "-o", join(directory, "os-obj.obj"), "--json"], false);
  assert.equal(diagnostic(osObjRejected).code, "E2002");
  assert.match(diagnostic(osObjRejected).message, /operating-system runtime/);
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

  // W39: multiple runtime-free Tsuzuri objects share weak definitions via `comdat any` across lld-link and link.exe at -O0 and -O3.
  const msvcLinkFlags = nativeFlags.map((flag) => (flag === "-fuse-ld=lld" ? "-fuse-ld=link" : flag));
  const llvmNm = join(dirname(clang), process.platform === "win32" ? "llvm-nm.exe" : "llvm-nm");
  const nmTool = existsSync(llvmNm) ? llvmNm : "llvm-nm";
  assert.doesNotMatch(execute(nmTool, [object]).stdout, /\.weak\./, "COFF object must not emit .weak. default symbols");
  const multiTwice = join(directory, "multi-twice");
  const multiTag = join(directory, "multi-tag");
  const multiPad = join(directory, "multi-pad");
  for (const dir of [multiTwice, multiTag, multiPad]) mkdirSync(dir);
  writeFileSync(
    join(multiTwice, "Main.tz"),
    [
      "export def twice :: i64 -> i64",
      "fn twice x = x * 2",
      "export def wide_div :: i64 -> i64 -> i64",
      "fn wide_div a b = (((a as i128) * 1000000000000i128) / (b as i128)) as i64",
      "",
    ].join("\n"),
  );
  writeFileSync(
    join(multiTag, "Main.tz"),
    "export def make_tag :: i64 -> [ubyte]\nfn make_tag n = Array.init n (\\i -> (65 + i) as ubyte)\n",
  );
  writeFileSync(
    join(multiPad, "Main.tz"),
    "export def make_pad :: i64 -> [ubyte]\nfn make_pad n = Array.init n (\\i -> (97 + i) as ubyte)\n",
  );
  const tagHeader = join(directory, "multi_tag.h");
  const padHeader = join(directory, "multi_pad.h");
  cli(["build", multiTag, "--emit", "header", "-o", tagHeader]);
  cli(["build", multiPad, "--emit", "header", "-o", padHeader]);
  const multiScalarHost = join(directory, "multi_scalar_host.c");
  writeFileSync(
    multiScalarHost,
    [
      "#undef NDEBUG",
      "#include <assert.h>",
      "#include <stdint.h>",
      "extern int64_t tz_answer(void), tz_twice(int64_t), tz_wide_div(int64_t, int64_t);",
      "int main(void) {",
      "  assert(tz_twice(tz_answer()) == 84);",
      "  assert(tz_wide_div(1000000000000LL, 300000000000LL) == 3333333333333LL);",
      "  return 0;",
      "}",
      "",
    ].join("\n"),
  );
  const multiOwnedHost = join(directory, "multi_owned_host.c");
  writeFileSync(
    multiOwnedHost,
    [
      "#undef NDEBUG",
      "#include <assert.h>",
      "#include <stdio.h>",
      "#include <string.h>",
      '#include "multi_tag.h"',
      '#include "multi_pad.h"',
      "int main(void) {",
      "  tsuzuri_ubyte_buffer tag = {0, 0}, pad = {0, 0};",
      "  tz_make_tag(&tag, 3);",
      "  tz_make_pad(&pad, 2);",
      "  assert(tag.len == 3 && memcmp(tag.ptr, \"ABC\", 3) == 0);",
      "  assert(pad.len == 2 && memcmp(pad.ptr, \"ab\", 2) == 0);",
      "  fwrite(tag.ptr, 1, (size_t)tag.len, stdout);",
      "  fwrite(pad.ptr, 1, (size_t)pad.len, stdout);",
      "  tsuzuri_free(tag.ptr);",
      "  tsuzuri_free(pad.ptr);",
      "  return 0;",
      "}",
      "",
    ].join("\n"),
  );
  for (const optimization of ["-O0", "-O3"]) {
    const objA = join(directory, `multi-a${optimization}.obj`);
    const objB = join(directory, `multi-b${optimization}.obj`);
    cli(["build", library, "--no-cache", optimization, "--emit", "object", "-o", objA]);
    cli(["build", multiTwice, "--no-cache", optimization, "--emit", "object", "-o", objB]);
    assert.doesNotMatch(execute(nmTool, [objB]).stdout, /\.weak\./);
    for (const [label, linkFlags] of [["lld", nativeFlags], ["link", msvcLinkFlags]]) {
      const abExe = join(directory, `multi-ab-${label}${optimization}.exe`);
      execute(clang, [...linkFlags, optimization, multiScalarHost, objA, objB, "-o", abExe]);
      execute(abExe, []);
    }
    for (const [allocLabel, allocFlags] of [["system", []], ["small", ["--allocator", "small"]]]) {
      const objC = join(directory, `multi-c-${allocLabel}${optimization}.obj`);
      const objD = join(directory, `multi-d-${allocLabel}${optimization}.obj`);
      cli(["build", multiTag, "--no-cache", optimization, ...allocFlags, "--emit", "object", "-o", objC]);
      cli(["build", multiPad, "--no-cache", optimization, ...allocFlags, "--emit", "object", "-o", objD]);
      assert.doesNotMatch(execute(nmTool, [objC]).stdout, /\.weak\./);
      assert.doesNotMatch(execute(nmTool, [objD]).stdout, /\.weak\./);
      for (const [label, linkFlags] of [["lld", nativeFlags], ["link", msvcLinkFlags]]) {
        const cdExe = join(directory, `multi-cd-${allocLabel}-${label}${optimization}.exe`);
        execute(clang, [...linkFlags, optimization, "-I", directory, multiOwnedHost, objC, objD, "-o", cdExe]);
        assert.equal(execute(cdExe, []).stdout, "ABCab");
      }
    }
  }
  console.log("Windows MSVC: multiple Tsuzuri objects linked with lld-link and link.exe at -O0 and -O3 (system and small allocators)");
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

  // W21: @c_layout structs match real Win32 headers (LOGFONTW, DLGTEMPLATE, GUID, MSG, WNDCLASSEXW)
  // and CreateFontIndirectW succeeds with ref LogFont.
  const winCLayoutDir = join(directory, "win-c-layout");
  mkdirSync(winCLayoutDir);
  writeFileSync(join(winCLayoutDir, "Main.tz"), [
    "extern type copy Hwnd",
    "extern type copy Hinstance",
    "extern type copy Hicon",
    "extern type copy Hcursor",
    "extern type copy Hbrush",
    "extern type Hfont",
    "@c_layout",
    "record Point { x: i32, y: i32 }",
    "@c_layout",
    "record LogFont {",
    "    height: i32, width: i32, escapement: i32, orientation: i32, weight: i32,",
    "    italic: i8u, underline: i8u, strike_out: i8u, char_set: i8u,",
    "    out_precision: i8u, clip_precision: i8u, quality: i8u, pitch_and_family: i8u,",
    "    face_name: [i16u; 32]",
    "}",
    "@c_layout(pack = 2)",
    "record DlgTemplate { style: i32u, extended_style: i32u, cdit: i16u, x: i16, y: i16, cx: i16, cy: i16 }",
    "@c_layout",
    "record Guid { data1: i32u, data2: i16u, data3: i16u, data4: [i8u; 8] }",
    "@c_layout",
    "record Msg { hwnd: Maybe<Hwnd>, message: i32u, w_param: i64u, l_param: i64, time: i32u, pt: Point }",
    "@c_layout",
    "record WndClassEx {",
    "    cb_size: i32u, style: i32u, lpfn_wnd_proc: FnPtr<(Hwnd -> i32u -> i64u -> i64 -> i64)>,",
    "    cb_cls_extra: i32, cb_wnd_extra: i32, h_instance: Maybe<Hinstance>,",
    "    h_icon: Maybe<Hicon>, h_cursor: Maybe<Hcursor>, hbr_background: Maybe<Hbrush>,",
    "    lpsz_menu_name: Maybe<ref ZString>, lpsz_class_name: ref ZString, h_icon_sm: Maybe<Hicon>",
    "}",
    "@trusted",
    'extern "CreateFontIndirectW" def create_font :: ref LogFont -> Maybe<Hfont>',
    "@trusted",
    'extern "DeleteObject" def delete_object :: Hfont -> i32',
    "@trusted",
    'extern "RegisterClassExW" def register_class :: ref WndClassEx -> i16u',
    "export def dlg_items :: ref DlgTemplate -> i16u = \\d -> d.cdit",
    "export def guid_d1 :: ref Guid -> i32u = \\g -> g.data1",
    "export def msg_code :: ref Msg -> i32u = \\m -> m.message",
    "def main :: unit -> i32 = \\() ->",
    "    let lf = LogFont {",
    "        height: -12i32, width: 0i32, escapement: 0i32, orientation: 0i32, weight: 400i32,",
    "        italic: 0i8u, underline: 0i8u, strike_out: 0i8u, char_set: 1i8u,",
    "        out_precision: 0i8u, clip_precision: 0i8u, quality: 0i8u, pitch_and_family: 0i8u,",
    "        face_name: FixedArray.init (\\_ -> 0i16u)",
    "    }",
    "    match create_font (ref lf) with",
    "    | Maybe.Some f -> if delete_object f != 0i32 then 7i32 else 1i32",
    "    | Maybe.None -> 2i32",
    "",
  ].join("\n"));
  const winCLayoutH = join(directory, "win_c_layout.h");
  cli(["build", winCLayoutDir, "--emit", "header", "-o", winCLayoutH]);
  const winCLayoutCheck = join(directory, "win_c_layout_check.c");
  writeFileSync(winCLayoutCheck, [
    "#define WIN32_LEAN_AND_MEAN",
    "#include <windows.h>",
    "#include <stddef.h>",
    '#include "win_c_layout.h"',
    '_Static_assert(sizeof(tz_record_4Main_7LogFont) == sizeof(LOGFONTW), "LOGFONTW size");',
    '_Static_assert(offsetof(tz_record_4Main_7LogFont, face_name) == offsetof(LOGFONTW, lfFaceName), "LOGFONTW face");',
    '_Static_assert(sizeof(tz_record_4Main_11DlgTemplate) == sizeof(DLGTEMPLATE), "DLGTEMPLATE size");',
    '_Static_assert(offsetof(tz_record_4Main_11DlgTemplate, cdit) == offsetof(DLGTEMPLATE, cdit), "DLGTEMPLATE cdit");',
    '_Static_assert(sizeof(tz_record_4Main_4Guid) == sizeof(GUID), "GUID size");',
    '_Static_assert(sizeof(tz_record_4Main_3Msg) == sizeof(MSG), "MSG size");',
    '_Static_assert(sizeof(tz_record_4Main_10WndClassEx) == sizeof(WNDCLASSEXW), "WNDCLASSEXW size");',
    '_Static_assert(offsetof(tz_record_4Main_10WndClassEx, lpsz_class_name) == offsetof(WNDCLASSEXW, lpszClassName), "WNDCLASSEXW class");',
    "int ok(void) { return 0; }",
    "",
  ].join("\n"));
  execute(clang, [...nativeFlags, "-c", "-I", directory, winCLayoutCheck, "-o", join(directory, "win_c_layout_check.obj")]);
  const fontExe = join(directory, "win-c-layout-font.exe");
  cli(["build", winCLayoutDir, "--no-cache", "-O3", "-l", "gdi32", "-l", "user32", "-o", fontExe]);
  assert.equal(execute(fontExe, [], false).status, 7, "CreateFontIndirectW with @c_layout LogFont");
  console.log("Windows MSVC: W21 @c_layout Win32 header parity & CreateFontIndirectW passed");

  // W36: a @c_layout record without ref is the C struct by value. PtInRect(const RECT *, POINT) takes
  // the POINT in one integer register on x64 and Arm64; a pointer there reads another point.
  const byValueWin = join(directory, "by-value-win");
  mkdirSync(byValueWin);
  writeFileSync(join(byValueWin, "Main.tz"), [
    "@c_layout",
    "record Rect { left: i32, top: i32, right: i32, bottom: i32 }",
    "@c_layout",
    "record Point { x: i32, y: i32 }",
    "@trusted",
    'extern "PtInRect" def pt_in_rect :: ref Rect -> Point -> i32',
    "def main :: unit -> i32 = \\() ->",
    "    let r = Rect { left: 0i32, top: 0i32, right: 10i32, bottom: 10i32 }",
    "    let inside = pt_in_rect (ref r) (Point { x: 5i32, y: 5i32 })",
    "    let outside = pt_in_rect (ref r) (Point { x: 5i32, y: 50i32 })",
    "    inside + outside * 10i32",
    "",
  ].join("\n"));
  const byValueIr = join(directory, "by-value-win.ll");
  cli(["build", byValueWin, "--no-cache", "--emit", "llvm", "-o", byValueIr]);
  assert.match(readFileSync(byValueIr, "utf8"), /^declare (?:dllimport )?i32 @PtInRect\(ptr, i64\)$/m, "PtInRect takes the POINT by value");
  for (const optimization of ["-O0", "-O3"]) {
    const byValueExe = join(directory, `by-value-win${optimization}.exe`);
    cli(["build", byValueWin, "--no-cache", optimization, "-l", "user32", "-o", byValueExe]);
    assert.equal(execute(byValueExe, [], false).status, 1, `PtInRect with a by-value POINT ${optimization}`);
  }
  console.log("Windows MSVC: W36 @c_layout by value (PtInRect) passed");

  // W24 + NS07: extern view with ExternAddr live HeapAlloc / HeapFree round-trip, zero rejection, and misaligned trap site.
  const winViewDir = join(directory, "win-extern-view");
  mkdirSync(winViewDir);
  writeFileSync(join(winViewDir, "Main.tz"), [
    "record Point { x: i32, y: i32 }",
    "@trusted",
    'extern "GetProcessHeap" def process_heap :: unit -> i64',
    "@trusted",
    'extern "HeapAlloc" def heap_alloc :: i64 -> i32u -> i64 -> i64',
    "@trusted",
    'extern "HeapFree" def heap_free :: i64 -> i32u -> i64 -> i32',
    "@trusted",
    "extern view def read_point :: ExternAddr -> Point",
    "@trusted",
    "extern view def write_point :: ExternAddr -> ref Point -> unit",
    "def main :: Array<string> -> i32 = \\args ->",
    "    if args.length > 0 then",
    "        let bad = Maybe.get (ExternAddr.of_i64 2)",
    "        (read_point bad).x",
    "    else",
    "        let zero_none = match ExternAddr.of_i64 0 with | Maybe.None -> 0i32 | Maybe.Some _ -> 99i32",
    "        let heap = process_heap ()",
    "        let raw = heap_alloc heap 8i32u 8",
    "        let address = Maybe.get (ExternAddr.of_i64 raw)",
    "        let point = Point { x: 40i32, y: 2i32 }",
    "        write_point address (ref point)",
    "        let back = read_point address",
    "        let _ = heap_free heap 0i32u raw",
    "        back.x + back.y + zero_none",
    "",
  ].join("\n"));
  for (const optimization of ["-O0", "-O3"]) {
    const viewExe = join(directory, `win-view${optimization}.exe`);
    cli(["build", winViewDir, "--no-cache", optimization, "--trap-info", "-o", viewExe]);
    assert.equal(execute(viewExe, [], false).status, 42, `extern view HeapAlloc ${optimization}`);
    const trapped = execute(viewExe, ["misaligned"], false);
    assert.equal(trapped.status >>> 0, 0xc000001d, `extern view misaligned trap ${optimization}`);
    assert.match(trapped.stderr, /index out of bounds at .*Main\.tz:15:/, `extern view trap site ${optimization}: ${trapped.stderr}`);
  }
  console.log("Windows MSVC: W24/NS07 extern view ExternAddr HeapAlloc round-trip & trap passed");

  // W17: `@link("gdi32")` links GdiFlush without `-l gdi32` and emits `declare dllimport`.
  const w17WinDir = join(directory, "win-w17-link");
  mkdirSync(w17WinDir);
  writeFileSync(join(w17WinDir, "Main.tz"), [
    '@trusted @link("gdi32")',
    'extern "GdiFlush" def gdi_flush :: unit -> bool',
    "def main :: unit -> i32 = \\() ->",
    "    if gdi_flush () then 7i32 else 8i32",
    "",
  ].join("\n"));
  const w17Ir = join(directory, "win-w17.ll");
  cli(["build", w17WinDir, "--no-cache", "--emit", "llvm", "-o", w17Ir]);
  assert.match(readFileSync(w17Ir, "utf8"), /declare dllimport i32 @GdiFlush\(\)/);
  for (const optimization of ["-O0", "-O3"]) {
    const w17Exe = join(directory, `win-w17${optimization}.exe`);
    cli(["build", w17WinDir, "--no-cache", optimization, "-o", w17Exe]);
    assert.equal(execute(w17Exe, [], false).status, 7, `W17 @link("gdi32") ${optimization}`);
  }
  console.log("Windows MSVC: W17 @link(\"gdi32\") and dllimport passed");

  // W05: `--emit shared` writes `<name>.dll` and `<name>.lib`, exports only `shared_exports`, caches
  // DLL + import library + PDB, and runs from C hosts (including tzroutine + host threads and OS APIs).
  const llvmReadobjCandidate = join(dirname(clang), "llvm-readobj.exe");
  const readobjTool = existsSync(llvmReadobjCandidate) ? llvmReadobjCandidate : "llvm-readobj";
  const coffExports = (dllPath) =>
    execute(readobjTool, ["--coff-exports", dllPath])
      .stdout.split(/\r?\n/)
      .map((line) => /^\s*Name:\s*(\S+)\s*$/.exec(line)?.[1])
      .filter(Boolean)
      .sort();
  const sharedDir = join(directory, "w05-shared");
  const sharedCache = join(directory, "w05-cache");
  mkdirSync(sharedDir);
  const tagSrc = join(sharedDir, "tag-src");
  mkdirSync(tagSrc);
  writeFileSync(
    join(tagSrc, "Main.tz"),
    [
      "export def add :: i64 -> i64 -> i64",
      "fn add a b = a + b",
      "export def make_tag :: i64 -> [ubyte]",
      "fn make_tag n = Array.init n (\\i -> (65 + i) as ubyte)",
      "",
    ].join("\n"),
  );
  const tagDllHeader = join(sharedDir, "tag.h");
  cli(["build", tagSrc, "--emit", "header", "-o", tagDllHeader]);
  const tagDllHost = join(sharedDir, "tag_host.c");
  writeFileSync(
    tagDllHost,
    [
      "#undef NDEBUG",
      "#include <assert.h>",
      "#include <stdio.h>",
      "#include <string.h>",
      '#include "tag.h"',
      "int main(void) {",
      "  assert(tz_add(40, 2) == 42);",
      "  tsuzuri_ubyte_buffer tag = {0, 0};",
      "  tz_make_tag(&tag, 3);",
      "  assert(tag.len == 3 && memcmp(tag.ptr, \"ABC\", 3) == 0);",
      "  fwrite(tag.ptr, 1, 3, stdout);",
      "  tsuzuri_free(tag.ptr);",
      "  return 0;",
      "}",
      "",
    ].join("\n"),
  );
  for (const optimization of ["-O0", "-O3"]) {
    const outSub = join(sharedDir, `out${optimization}`);
    mkdirSync(outSub);
    const dllPath = join(outSub, "my tag.dll");
    const libPath = join(outSub, "my tag.lib");
    cli(["build", tagSrc, "--no-cache", optimization, "--emit", "shared", "-o", dllPath]);
    assert.deepEqual(readdirSync(outSub).sort(), ["my tag.dll", "my tag.lib"]);
    assert.deepEqual(coffExports(dllPath), ["tsuzuri_alloc", "tsuzuri_free", "tz_add", "tz_make_tag"]);
    const nmOut = execute(nmTool, [libPath]).stdout;
    assert.doesNotMatch(nmOut, /\.weak\./);
    for (const sym of ["__imp_tsuzuri_alloc", "__imp_tsuzuri_free", "__imp_tz_add", "__imp_tz_make_tag"]) {
      assert.match(nmOut, new RegExp(`\\b${sym}\\b`));
    }
    const readobjLib = execute(readobjTool, [libPath]).stdout;
    assert.match(readobjLib, /File:\s*my tag\.dll\r?\nFormat:\s*COFF-import-file-/);

    const countingDll = join(outSub, "counting.dll");
    cli(["build", tagSrc, "--no-cache", optimization, "--allocator", "counting", "--emit", "shared", "-o", countingDll]);
    assert.deepEqual(coffExports(countingDll), ["tsuzuri_alloc", "tsuzuri_alloc_stats", "tsuzuri_free", "tz_add", "tz_make_tag"]);

    for (const [label, linkFlags] of [["lld", nativeFlags], ["link", msvcLinkFlags]]) {
      const hostExe = join(outSub, `host-${label}.exe`);
      execute(clang, [...linkFlags, optimization, "-I", sharedDir, tagDllHost, libPath, "-o", hostExe]);
      assert.equal(execute(hostExe, []).stdout, "ABC");
    }

    // Runtime-embedded DLL: tzroutine scheduler + 8 host OS threads (verifies TLS detach cleanup).
    const tzDll = join(outSub, "tz_rt.dll");
    const tzLib = join(outSub, "tz_rt.lib");
    cli(["build", "tests/fixtures/tzroutine/Main.tz", "--no-cache", optimization, "--emit", "shared", "-o", tzDll]);
    const tzDllHost = join(outSub, "tz_dll_host.c");
    writeFileSync(
      tzDllHost,
      [
        "#undef NDEBUG",
        "#include <assert.h>",
        "#include <process.h>",
        "#include <stdint.h>",
        "#include <windows.h>",
        "extern int64_t tz_sum_atomic(int64_t), tz_nested(int64_t), tz_moves_owned(void), tz_go_owned(void), tz_counts(void);",
        "static unsigned __stdcall worker(void *arg) {",
        "  (void)arg;",
        "  assert(tz_sum_atomic(100) == 4950);",
        "  return 0;",
        "}",
        "int main(void) {",
        "  assert(tz_sum_atomic(100) == 4950);",
        "  assert(tz_nested(10) == 2047);",
        "  assert(tz_moves_owned() == 13);",
        "  assert(tz_go_owned() == 11);",
        "  assert(tz_counts() == 1);",
        "  HANDLE threads[8];",
        "  for (int i = 0; i < 8; i++) {",
        "    uintptr_t h = _beginthreadex(NULL, 0, worker, NULL, 0, NULL);",
        "    assert(h != 0);",
        "    threads[i] = (HANDLE)h;",
        "  }",
        "  assert(WaitForMultipleObjects(8, threads, TRUE, 30000) == WAIT_OBJECT_0);",
        "  for (int i = 0; i < 8; i++) CloseHandle(threads[i]);",
        "  return 0;",
        "}",
        "",
      ].join("\n"),
    );
    const tzHostExe = join(outSub, "tz_dll_host.exe");
    execute(clang, [...nativeFlags, optimization, tzDllHost, tzLib, "-o", tzHostExe]);
    execute(tzHostExe, []);
  }

  // Debug PDB + build cache round-trip for DLL + import library + PDB.
  const debugSub = join(sharedDir, "debug");
  mkdirSync(debugSub);
  const debugDll = join(debugSub, "tag_dbg.dll");
  const debugLib = join(debugSub, "tag_dbg.lib");
  const debugPdb = join(debugSub, "tag_dbg.pdb");
  cli(["build", tagSrc, "-g", "--emit", "shared", "-o", debugDll], true, { TSUZURI_CACHE_DIR: sharedCache });
  assert.ok(existsSync(debugPdb), "MSVC debug DLL must write adjacent .pdb");
  assert.match(execute(readobjTool, ["--coff-debug-directory", debugDll]).stdout, /PDBFileName:\s*tag_dbg\.pdb/);
  const firstDllBytes = readFileSync(debugDll);
  const firstLibBytes = readFileSync(debugLib);
  const firstPdbBytes = readFileSync(debugPdb);
  rmSync(debugSub, { recursive: true, force: true });
  mkdirSync(debugSub);
  cli(["build", tagSrc, "-g", "--emit", "shared", "-o", debugDll], true, { TSUZURI_CACHE_DIR: sharedCache });
  assert.deepEqual(readFileSync(debugDll), firstDllBytes);
  assert.deepEqual(readFileSync(debugLib), firstLibBytes);
  assert.deepEqual(readFileSync(debugPdb), firstPdbBytes);

  // Refusals: --trap-mode return, non-.dll extension, --allocator host, and unresolved extern.
  const badTrap = diagnostic(cli(["build", tagSrc, "--emit", "shared", "--trap-mode", "return", "-o", join(sharedDir, "bad.dll"), "--json"], false));
  assert.equal(badTrap.code, "E2000");
  assert.match(badTrap.message, /--trap-mode return is not supported for Windows shared libraries yet/);
  const badExt = diagnostic(cli(["build", tagSrc, "--emit", "shared", "-o", join(sharedDir, "bad.so"), "--json"], false));
  assert.equal(badExt.code, "E2000");
  assert.match(badExt.message, /must end with '\.dll'/);
  const badHost = diagnostic(cli(["build", tagSrc, "--emit", "shared", "--allocator", "host", "-o", join(sharedDir, "bad_host.dll"), "--json"], false));
  assert.equal(badHost.code, "E2000");
  const unresolvedLib = join(sharedDir, "unresolved");
  mkdirSync(unresolvedLib);
  writeFileSync(join(unresolvedLib, "Main.tz"), "extern def missing_host :: unit -> i64\nexport def call_missing :: i64\nfn call_missing = missing_host ()\n");
  const unresolvedErr = diagnostic(cli(["build", unresolvedLib, "--no-cache", "--emit", "shared", "-o", join(sharedDir, "unresolved.dll"), "--json"], false));
  assert.equal(unresolvedErr.code, "E2002");
  assert.match(unresolvedErr.message, /unresolved symbols: tsuzuri_host_Main_missing_host/);
  assert.doesNotMatch(unresolvedErr.message, /shared libraries require the Clang linker/);
  const unresolvedExeDir = join(sharedDir, "unresolved-exe");
  mkdirSync(unresolvedExeDir);
  writeFileSync(join(unresolvedExeDir, "Main.tz"), "extern def host_value :: unit -> i64\ndef main :: unit -> i32 = \\() -> (host_value ()) as i32\n");
  const unresolvedExeErr = diagnostic(cli(["build", unresolvedExeDir, "--no-cache", "-o", join(sharedDir, "unresolved.exe"), "--json"], false));
  assert.equal(unresolvedExeErr.code, "E2002");
  assert.match(unresolvedExeErr.message, /unresolved symbols: tsuzuri_host_Main_host_value/);
  assert.doesNotMatch(unresolvedExeErr.message, /install LLVM\/Clang/);
  console.log("Windows MSVC: W05 --emit shared (.dll + .lib + .pdb + cache + tzroutine threads) passed");

  runGuiStage(compiler, clang, directory);
  console.log("Windows MSVC: native O0/O3, tzroutines and Parallel, linked IR, UTF-8, hardlinks, atomic replacement, object ABI and WASM passed");
    }
  } finally {
    rmSync(directory, { recursive: true, force: true, maxRetries: 10, retryDelay: 50 });
  }
}
