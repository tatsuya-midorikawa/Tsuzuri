// M19: an executable for macOS whose standard error is closed or /dev/null (one started from Finder, the Dock,
// `open` or launchd) sends its trap and runtime reports to unified logging; a terminal, a pipe and a file get the
// bytes they got before.
// Usage: node tests/macos_report.mjs [path/to/tsuzuri]
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { closeSync, copyFileSync, existsSync, mkdirSync, mkdtempSync, openSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

if (process.platform !== "darwin") {
  console.log("macos_report: skipped (unified logging is the report destination of macOS executables only)");
  process.exit(0);
}

const repository = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(process.argv[2] ?? join(repository, "target", "release", "tsuzuri"));
const bundledClang = process.env.TSUZURI_BUNDLED_CLANG ?? join(repository, "target", "toolchain", "bin", "tsuzuri-clang");
const root = mkdtempSync(join(tmpdir(), "tsuzuri-macos-report-"));
const subsystem = "dev.tsuzuri.runtime";
// The children decide by their own standard error; the variables that would change what they write are unset.
const childEnvironment = { ...process.env };
delete childEnvironment.OS_ACTIVITY_MODE;
delete childEnvironment.TSUZURI_TRAP_REPORT;
delete childEnvironment.TSUZURI_CPU_FORCE;

function execute(program, args, options = {}) {
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 300_000, maxBuffer: 64 * 1024 * 1024, cwd: root, ...options });
  assert.ifError(result.error);
  assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}

function project(name, source) {
  mkdirSync(join(root, name));
  writeFileSync(join(root, name, "Main.tz"), source);
  return name;
}

// The compiler runs in `root` with the project's relative name, so the path in a trap's message is `<name>/Main.tz`.
function build(name, output, flags = [], environment = process.env) {
  execute(compiler, ["build", name, ...flags, "-o", output], { env: environment });
  return join(root, output);
}

// Runs `program` with its standard error as a pipe, as /dev/null, closed, or as a file, and gives back what it wrote
// there with how it ended. The shell of "closed" replaces itself with the program, so the process ID is the program's.
function launch(program, stderr, { args = [], environment = childEnvironment, timeout = 60_000 } = {}) {
  const options = { encoding: "utf8", timeout, maxBuffer: 64 * 1024 * 1024, env: environment, cwd: root };
  let result;
  let written = null;
  if (stderr === "pipe") {
    result = spawnSync(program, args, { ...options, stdio: ["ignore", "pipe", "pipe"] });
    written = result.stderr;
  } else if (stderr === "null") {
    result = spawnSync(program, args, { ...options, stdio: ["ignore", "pipe", "ignore"] });
  } else if (stderr === "closed") {
    result = spawnSync("/bin/sh", ["-c", 'exec "$0" "$@" 2>&-', program, ...args], { ...options, stdio: ["ignore", "pipe", "ignore"] });
  } else {
    const file = join(root, `stderr-${stderr}.txt`);
    const descriptor = openSync(file, "w");
    try {
      result = spawnSync(program, args, { ...options, stdio: ["ignore", "pipe", descriptor] });
    } finally {
      closeSync(descriptor);
    }
    written = readFileSync(file, "utf8");
  }
  assert.ifError(result.error);
  return { pid: result.pid, status: result.status, signal: result.signal, stdout: result.stdout, stderr: written };
}

// ---------------------------------------------------------------- unified logging

const pad = (value) => String(value).padStart(2, "0");
const started = new Date(Date.now() - 5_000);
const since = `${started.getFullYear()}-${pad(started.getMonth() + 1)}-${pad(started.getDate())} ${pad(started.getHours())}:${pad(started.getMinutes())}:${pad(started.getSeconds())}`;

// The entries of the subsystem since the test began, by process ID; `null` when `log show` fails. zsh has a builtin
// named `log`, so the tool is named by its path. An entry is looked up by process ID and never by process name: the
// log names a process by the UUID of its image, which two builds of one source share.
function logged() {
  const result = spawnSync("/usr/bin/log", ["show", "--predicate", `subsystem == "${subsystem}"`, "--start", since, "--style", "ndjson"],
    { encoding: "utf8", timeout: 120_000, maxBuffer: 256 * 1024 * 1024 });
  if (result.error || result.status !== 0) return null;
  const entries = new Map();
  for (const line of result.stdout.split("\n")) {
    if (!line.startsWith("{")) continue;
    const entry = JSON.parse(line);
    if (entry.eventMessage === undefined || entry.subsystem !== subsystem) continue;
    if (!entries.has(entry.processID)) entries.set(entry.processID, []);
    entries.get(entry.processID).push(entry);
  }
  return entries;
}

const sleep = (milliseconds) => Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, milliseconds);

// Reads the log until every process of `expected` (process ID -> least number of entries) has its entries, or 60
// seconds pass: an entry is stored before the call that sent it returns, but `log show` may read a moment behind.
function waitForEntries(expected) {
  const deadline = Date.now() + 60_000;
  for (;;) {
    const entries = logged();
    if (entries === null) return null;
    if ([...expected].every(([pid, least]) => (entries.get(pid)?.length ?? 0) >= least) || Date.now() > deadline) return entries;
    sleep(500);
  }
}

// What the test expects of the log: `{ pid, name, count, least, category, message, prefix }` per process.
const expectations = [];
const silent = (run, name) => expectations.push({ pid: run.pid, name, count: 0 });
const reported = (run, name, category, message, { least = false, prefix = false } = {}) =>
  expectations.push({ pid: run.pid, name, count: 1, least, category, message, prefix });

let cases = 0;
try {
  const clang = "clang";
  const host = join(repository, "tests", "macos_report_host.c");

  // ---------------------------------------------------------------- where a report goes, without the log
  // src/runtime/oslog-darwin.c built with TZ_REPORT_TEST_LOG writes its entries to a file. The first number is
  // the answer for the standard error that the process started with, the second the answer after standard error
  // was pointed somewhere else: the first one is remembered.
  {
    const entriesFile = join(root, "entries.txt");
    const hook = join(root, "report-hook");
    execute(clang, ["-std=c11", "-O1", "-Wall", "-Wextra", "-Werror", `-DTZ_REPORT_TEST_LOG="${entriesFile}"`, host, "-o", hook]);
    const wide = "x".repeat(512);
    const entries = [
      "trap\ttrap: integer division by zero at Main.tz:1:34",
      "trap\tTsuzuri runtime: the weak symbol 'tz_missing' is not available on this system",
      "runtime\tTsuzuri runtime: negative WaitGroup counter",
      `runtime\t${wide}`,
      "",
    ].join("\n");
    const zero = openSync("/dev/zero", "w");
    try {
      for (const [stderr, answer] of [["pipe", "0 0"], ["hook-file", "0 0"], ["null", "1 1"], ["closed", "1 1"], [zero, "0 0"]]) {
        rmSync(entriesFile, { force: true });
        let run;
        if (stderr === zero) {
          // A character device that is not /dev/null, as a terminal is.
          const result = spawnSync(hook, [], { encoding: "utf8", stdio: ["ignore", "pipe", zero], timeout: 60_000 });
          assert.ifError(result.error);
          run = { status: result.status, stdout: result.stdout };
        } else {
          run = launch(hook, stderr);
        }
        assert.equal(run.status, 0, `report hook, standard error ${stderr}`);
        assert.equal(run.stdout, `${answer}\n`, `report hook, standard error ${stderr}`);
        assert.equal(readFileSync(entriesFile, "utf8"), entries, `report hook, standard error ${stderr}`);
        cases++;
      }
    } finally {
      closeSync(zero);
    }
  }

  // ---------------------------------------------------------------- can this machine read the log?
  const probe = join(root, "report-probe");
  execute(clang, ["-std=c11", "-O1", "-DTZ_REPORT_PROBE", host, "-o", probe]);
  const probed = launch(probe, "null");
  assert.equal(probed.status, 0);
  const readable = (waitForEntries(new Map([[probed.pid, 1]]))?.get(probed.pid)?.length ?? 0) >= 1;
  if (!readable) {
    console.log("macos_report: `log show` cannot read the entries of this machine's processes; the entries are not checked");
  }

  // ---------------------------------------------------------------- programs
  const division = project("divz", "def divide :: i64 -> i64 = \\x -> 10 / x\n\ndef main :: Array<string> -> i32 = \\args ->\n    (divide args.length) as i32\n");
  // `10 / x` starts at column 34 of line 1.
  const divisionReport = "trap: integer division by zero at divz/Main.tz:1:34";
  const waitGroup = project("waitgroup", "let group = WaitGroup.create ()\nWaitGroup.done (ref group)\n0\n");
  const waitGroupReport = "Tsuzuri runtime: negative WaitGroup counter";
  // The multiplication keeps the recursion non-tail, so LLVM cannot turn it into a loop (tests/stack_overflow.mjs).
  const depth = "def rec depth :: i64 -> i64\nfn rec depth n = if n == 0 then 0 else depth (n - 1) * 3 + n\n\n";
  const deep = project("deep", `${depth}let result = depth 100000000\nresult\n`);
  const routine = project("routine", `${depth}for i in 0 ..< 4 do\n    tzroutine {\n        let _r = depth (100000000 + (i as i64))\n    }\n0\n`);
  const overflowReport = "trap: stack overflow";
  const afterMain = project("aftermain", "@trusted\nextern \"atexit\" def c_atexit :: (unit -> unit) -> i32\n\ndef bye :: unit -> unit = \\() -> ()\n\ndef main :: unit -> i32 = \\() ->\n    let _r = c_atexit bye\n    0\n");
  const afterMainReport = "Tsuzuri runtime: a callback was called after main returned; stop the timers, threads and windows that call it before main returns";
  const hello = project("hello", "let value = 6 * 7\nvalue\n");
  // W12: a host thread that never calls back does not hide the deadlock of `main`.
  mkdirSync(join(root, "lifetime"));
  copyFileSync(join(repository, "tests/fixtures/callback_lifetime/Main.tz"), join(root, "lifetime/Main.tz"));
  const lifetimeHost = join(root, "lifetime-host.o");
  execute(clang, ["-c", "-O2", "-pthread", join(repository, "tests/fixtures/callback_lifetime/host.c"), "-o", lifetimeHost]);
  const deadlockReport = "Tsuzuri runtime: all tzroutines are asleep - deadlock!";
  // M17: a weak function that the library of the running system does not have. The report is written in three
  // pieces (a prefix, the name, a suffix) and is one entry.
  const library = join(root, "libm19w.dylib");
  writeFileSync(join(root, "new.c"), "int m19_new_api(void) { return 7; }\n");
  writeFileSync(join(root, "old.c"), "int m19_old_api(void) { return 3; }\n");
  execute(clang, ["-dynamiclib", "new.c", "-install_name", library, "-o", "new.dylib"]);
  execute(clang, ["-dynamiclib", "old.c", "-install_name", library, "-o", "old.dylib"]);
  const weak = project("weak", "@trusted @weak\nextern \"m19_new_api\" def new_api :: unit -> i32\n\ndef main :: unit -> i32 = \\() -> new_api ()\n");
  const weakReport = "Tsuzuri runtime: the weak symbol 'm19_new_api' is not available on this system";
  const cpuReport = "Tsuzuri CPU runtime: requested variant is unavailable or unknown";

  const trapped = (run, name) => assert.equal(run.signal, "SIGTRAP", `${name}: ${run.signal} ${run.status}`);
  const aborted = (run, name) => assert.equal(run.signal, "SIGABRT", `${name}: ${run.signal} ${run.status}`);

  for (const optimization of ["-O0", "-O3"]) {
    // A trap's message (--trap-info): the pipe and the file get the same line as before and the log gets nothing;
    // /dev/null and a closed standard error leave one entry with the line.
    {
      const program = build(division, `divz${optimization}`, [optimization, "--trap-info"]);
      const piped = launch(program, "pipe");
      trapped(piped, `divz ${optimization} pipe`);
      assert.equal(piped.stderr, `${divisionReport}\n`);
      silent(piped, `divz ${optimization} pipe`);
      const filed = launch(program, `divz${optimization}`);
      trapped(filed, `divz ${optimization} file`);
      assert.equal(filed.stderr, piped.stderr);
      silent(filed, `divz ${optimization} file`);
      for (const stderr of ["null", "closed"]) {
        const run = launch(program, stderr);
        trapped(run, `divz ${optimization} ${stderr}`);
        reported(run, `divz ${optimization} ${stderr}`, "trap", divisionReport);
      }
      cases += 4;
    }
    // A report of the tzroutine runtime, in a default build.
    {
      const program = build(waitGroup, `waitgroup${optimization}`, [optimization]);
      const piped = launch(program, "pipe");
      trapped(piped, `waitgroup ${optimization} pipe`);
      assert.equal(piped.stderr, `${waitGroupReport}\n`);
      silent(piped, `waitgroup ${optimization} pipe`);
      const run = launch(program, "null");
      trapped(run, `waitgroup ${optimization} null`);
      reported(run, `waitgroup ${optimization} null`, "runtime", waitGroupReport);
      cases += 2;
    }
    // A stack overflow, reported inside the signal handler on the alternate stack: on the main thread once, and
    // by every tzroutine that overflows before the first one ends the process.
    {
      const program = build(deep, `deep${optimization}`, [optimization]);
      const piped = launch(program, "pipe");
      aborted(piped, `deep ${optimization} pipe`);
      assert.equal(piped.stderr, `${overflowReport}\n`);
      silent(piped, `deep ${optimization} pipe`);
      const run = launch(program, "null", { timeout: 30_000 });
      aborted(run, `deep ${optimization} null`);
      reported(run, `deep ${optimization} null`, "trap", overflowReport);
      const routines = build(routine, `routine${optimization}`, [optimization]);
      const spawned = launch(routines, "null", { timeout: 30_000 });
      aborted(spawned, `routine ${optimization} null`);
      reported(spawned, `routine ${optimization} null`, "trap", overflowReport, { least: true });
      cases += 3;
    }
    // W12: `atexit` calls the callback after `main` returned, in a default build.
    {
      const program = build(afterMain, `aftermain${optimization}`, [optimization]);
      const piped = launch(program, "pipe");
      trapped(piped, `aftermain ${optimization} pipe`);
      assert.equal(piped.stderr, `${afterMainReport}\n`);
      silent(piped, `aftermain ${optimization} pipe`);
      const run = launch(program, "null");
      trapped(run, `aftermain ${optimization} null`);
      reported(run, `aftermain ${optimization} null`, "trap", afterMainReport);
      cases += 2;
    }
    // The deadlock report, after which the parked `main` is woken to trap.
    {
      const program = build("lifetime", `lifetime${optimization}`, [optimization, "--link", lifetimeHost, "-l", "pthread"]);
      const piped = launch(program, "pipe", { args: ["deadlock"], timeout: 30_000 });
      assert.notEqual(piped.status, 0);
      assert.ok(piped.stderr.startsWith(deadlockReport), piped.stderr);
      silent(piped, `deadlock ${optimization} pipe`);
      const run = launch(program, "null", { args: ["deadlock"], timeout: 30_000 });
      assert.notEqual(run.status, 0);
      reported(run, `deadlock ${optimization} null`, "runtime", deadlockReport, { prefix: true });
      cases += 2;
    }
    // M17: the weak symbol's report is one entry, though the generated code writes it in three pieces.
    {
      rmSync(library, { force: true });
      copyFileSync(join(root, "new.dylib"), library);
      const program = build(weak, `weak${optimization}`, [optimization, "-L", root, "-l", "m19w"]);
      assert.equal(launch(program, "null").status, 7);
      // Unlinked first, so that the system reads the code signature of the new file.
      rmSync(library, { force: true });
      copyFileSync(join(root, "old.dylib"), library);
      const piped = launch(program, "pipe");
      trapped(piped, `weak ${optimization} pipe`);
      assert.equal(piped.stderr, `${weakReport}\n`);
      silent(piped, `weak ${optimization} pipe`);
      const run = launch(program, "null");
      trapped(run, `weak ${optimization} null`);
      reported(run, `weak ${optimization} null`, "trap", weakReport);
      cases += 2;
    }
    // F08: a CPU variant that this machine does not have.
    {
      const program = join(root, `cpu${optimization}`);
      execute(compiler, ["build", "tests/fixtures/cpu_dispatch", optimization, "-o", program], { cwd: repository });
      const forced = { ...childEnvironment, TSUZURI_CPU_FORCE: "unknown" };
      const piped = launch(program, "pipe", { environment: forced });
      aborted(piped, `cpu ${optimization} pipe`);
      assert.equal(piped.stderr, `${cpuReport}\n`);
      silent(piped, `cpu ${optimization} pipe`);
      const run = launch(program, "null", { environment: forced });
      aborted(run, `cpu ${optimization} null`);
      reported(run, `cpu ${optimization} null`, "runtime", cpuReport);
      cases += 2;
    }
  }

  // A debug build links the runtime's objects in a step of its own, --no-cache compiles them where a cached build
  // restores them, codegen units (PB07) split the IR that holds the writer, and the fast backend (PB05) translates
  // the same IR without Clang.
  const variants = [
    ["debug", ["-g"], {}],
    ["nocache", ["--no-cache"], {}],
    ["units", [], { TSUZURI_CODEGEN_UNITS: "split" }],
    ["fast", ["--backend", "fast"], {}],
  ];
  for (const [label, flags, variables] of variants) {
    const environment = { ...process.env, ...variables };
    if (label === "fast") {
      // A compiler built without the fast backend says so.
      const probe = spawnSync(compiler, ["build", division, "-O0", "--trap-info", ...flags, "-o", "divz-probe"], { encoding: "utf8", timeout: 300_000, cwd: root });
      assert.ifError(probe.error);
      if (probe.status !== 0 && /built without the fast backend|--backend fast is not available on/.test(probe.stderr)) continue;
    }
    const program = build(division, `divz-${label}`, ["-O0", "--trap-info", ...flags], environment);
    const piped = launch(program, "pipe");
    trapped(piped, `divz ${label} pipe`);
    assert.equal(piped.stderr, `${divisionReport}\n`);
    silent(piped, `divz ${label} pipe`);
    const run = launch(program, "null");
    trapped(run, `divz ${label} null`);
    reported(run, `divz ${label} null`, "trap", divisionReport);
    const overflowing = build(deep, `deep-${label}`, ["-O0", ...flags], environment);
    const overflow = launch(overflowing, "null", { timeout: 30_000 });
    aborted(overflow, `deep ${label} null`);
    reported(overflow, `deep ${label} null`, "trap", overflowReport);
    cases += 3;
  }

  // The toolchain of a distribution, when the tree has one: its C library headers have no <os/log.h>, and its
  // linker reads the system library's symbols from its own list.
  if (existsSync(bundledClang)) {
    const environment = { ...process.env, TSUZURI_CLANG: bundledClang, TSUZURI_CACHE_DIR: "" };
    for (const optimization of ["-O0", "-O3"]) {
      const program = build(division, `divz-bundled${optimization}`, [optimization, "--trap-info"], environment);
      const run = launch(program, "null");
      trapped(run, `divz bundled ${optimization} null`);
      reported(run, `divz bundled ${optimization} null`, "trap", divisionReport);
      const overflowing = build(routine, `routine-bundled${optimization}`, [optimization], environment);
      const overflow = launch(overflowing, "null", { timeout: 30_000 });
      aborted(overflow, `routine bundled ${optimization} null`);
      reported(overflow, `routine bundled ${optimization} null`, "trap", overflowReport, { least: true });
      cases += 2;
    }
  }

  // OS_ACTIVITY_MODE=disable turns the process's logging off: nothing is left, as with /dev/null before, and the
  // process ends the same way.
  {
    const run = launch(join(root, "divz-O0"), "null", { environment: { ...childEnvironment, OS_ACTIVITY_MODE: "disable" } });
    trapped(run, "divz with logging off");
    silent(run, "divz with logging off");
    cases++;
  }

  // `tsuzuri run` reads the program's standard error through a pipe, so its report is what it was.
  {
    const run = spawnSync(compiler, ["run", division, "-O0"], { encoding: "utf8", timeout: 300_000, cwd: root, env: childEnvironment });
    assert.ifError(run.error);
    assert.equal(run.status, 1, run.stderr);
    assert.ok(run.stderr.startsWith(`${divisionReport}\n`), run.stderr);
    assert.match(run.stderr, /error\[E2005\]: program terminated with signal: 5 \(SIGTRAP\)/);
    cases++;
  }

  // Only the executables that can report carry the destination, and objects never do: their host owns its
  // standard error.
  {
    const undefinedSymbols = (file) => execute("nm", ["-u", file]).stdout;
    assert.match(undefinedSymbols(join(root, "divz-O0")), /_os_log_create/);
    const plain = build(hello, "hello-O0", ["-O0"]);
    assert.doesNotMatch(undefinedSymbols(plain), /os_log|tsuzuri_report/);
    for (const [name, flags] of [[division, ["--trap-info"]], [waitGroup, []], [deep, []]]) {
      const object = build(name, `${name}.o`, ["-O0", "--emit", "object", ...flags]);
      assert.doesNotMatch(undefinedSymbols(object), /os_log|tsuzuri_report/, name);
      cases++;
    }
    cases += 2;
  }

  // ---------------------------------------------------------------- an application that LaunchServices starts
  // M15 and M19 together: `open` gives the executable of a bundle /dev/null as its standard error, which is the
  // case that the log is for. The process ID of the application is not known here, so its entry is found by its
  // message (the project has a name of its own) and by the path of the image inside the bundle.
  // TSUZURI_MACOS_BUNDLE_NO_OPEN=1 skips the stage on a machine without LaunchServices, as in macos_bundle.mjs.
  if (readable && !process.env.TSUZURI_MACOS_BUNDLE_NO_OPEN) {
    const name = project("appdivz", "def divide :: i64 -> i64 = \\x -> 10 / x\n\ndef main :: Array<string> -> i32 = \\args ->\n    (divide args.length) as i32\n");
    writeFileSync(join(root, name, "tsuzuri.toml"), '[package]\nname = "appdivz"\nversion = "0.1.0"\n\n[macos]\nbundle_id = "dev.tsuzuri.test.appdivz"\nui_element = true\n');
    execute(compiler, ["build", name, "-O0", "--trap-info", "--emit", "app", "-o", "AppDivz.app"]);
    // `open -W` waits for the application, which traps at once; perl's alarm ends an `open` that never returns.
    const opened = spawnSync("perl", ["-e", "alarm 60; exec @ARGV", "open", "-W", "-n", join(root, "AppDivz.app")], { encoding: "utf8", timeout: 90_000 });
    assert.ifError(opened.error);
    assert.equal(opened.status, 0, `open AppDivz.app\n${opened.stdout}\n${opened.stderr}`);
    const message = "trap: integer division by zero at appdivz/Main.tz:1:34";
    const image = "/AppDivz.app/Contents/MacOS/AppDivz";
    const deadline = Date.now() + 60_000;
    let found = [];
    for (;;) {
      const entries = logged();
      assert.ok(entries, "`log show` failed after it had worked");
      found = [...entries.values()].flat().filter((entry) => entry.eventMessage === message);
      if (found.length > 0 || Date.now() > deadline) break;
      sleep(500);
    }
    assert.equal(found.length, 1, `the trap of an application started by open: one entry\n${JSON.stringify(found)}`);
    assert.equal(found[0].category, "trap");
    assert.equal(found[0].messageType, "Error");
    assert.ok(String(found[0].processImagePath).endsWith(image), `the entry's image: ${found[0].processImagePath}`);
    cases++;
  }

  // ---------------------------------------------------------------- the entries
  if (readable) {
    const expected = new Map(expectations.filter((entry) => entry.count > 0).map((entry) => [entry.pid, entry.count]));
    const entries = waitForEntries(expected);
    assert.ok(entries, "`log show` failed after it had worked");
    for (const { pid, name, count, least, category, message, prefix } of expectations) {
      const found = entries.get(pid) ?? [];
      const text = found.map((entry) => `[${entry.category}] ${entry.eventMessage}`).join("\n");
      if (count === 0) {
        assert.equal(found.length, 0, `${name}: nothing goes to the log\n${text}`);
        continue;
      }
      if (least) assert.ok(found.length >= 1, `${name}: at least one entry\n${text}`);
      else assert.equal(found.length, 1, `${name}: one entry\n${text}`);
      for (const entry of found) {
        assert.equal(entry.category, category, `${name}\n${text}`);
        assert.equal(entry.messageType, "Error", `${name}\n${text}`);
        if (prefix) assert.ok(entry.eventMessage.startsWith(message), `${name}\n${text}`);
        else assert.equal(entry.eventMessage, message, `${name}\n${text}`);
      }
    }
  }

  // No process of the test is left behind.
  const left = spawnSync("pgrep", ["-f", root], { encoding: "utf8" });
  assert.equal(left.stdout.trim(), "", `processes left: ${left.stdout}`);

  console.log(`macos_report: ${cases} cases, ${readable ? `${expectations.length} processes checked against unified logging` : "unified logging not readable here"}`);
} finally {
  rmSync(root, { recursive: true, force: true });
}
