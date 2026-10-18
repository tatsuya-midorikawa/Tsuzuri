// `tsuzuri watch` end to end: each run prints what the same command prints on its own, a save
// that changes the text starts a run (also when the length and the modification time stay the
// same), and a touch does not.
// usage: node tests/watch.mjs [target/release/tsuzuri]
import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { chmodSync, copyFileSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, renameSync, rmSync, unlinkSync, utimesSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const root = realpathSync(mkdtempSync(join(tmpdir(), "tsuzuri-watch-")));
const env = { ...process.env, TSUZURI_CACHE_DIR: join(root, "cache"), TSUZURI_THREADS: "2" };
const children = new Set();
// A modification time that two writes can both be set to: the length and the time then stay the same.
const fixed = new Date(1_700_000_000_000);

const pause = (milliseconds) => new Promise((done) => setTimeout(done, milliseconds));

function cold(args) {
  const result = spawnSync(compiler, args, { cwd: root, env, encoding: "utf8", timeout: 180_000, maxBuffer: 16 << 20 });
  assert.ifError(result.error);
  return result;
}

const statusLine = /^watch: run (\d+) finished with exit code (\d+)\n|^\{"type":"watch","run":(\d+),"exit":(\d+)\}\n/m;

// Starts `tsuzuri watch ARGS` in the test root and collects what each run writes before its status line.
function watch(args, program = compiler, extra = {}) {
  const child = spawn(program, ["watch", ...args], { cwd: root, env: { ...env, ...extra }, stdio: ["ignore", "pipe", "pipe"] });
  children.add(child);
  const state = { runs: [], stderr: "", stdout: "", consumed: 0, waiters: [], exit: null };
  child.stdout.setEncoding("utf8").on("data", (chunk) => { state.stdout += chunk; });
  child.stderr.setEncoding("utf8").on("data", (chunk) => {
    state.stderr += chunk;
    for (;;) {
      const rest = state.stderr.slice(state.consumed);
      const match = statusLine.exec(rest);
      if (!match) break;
      state.runs.push({ run: Number(match[1] ?? match[3]), exit: Number(match[2] ?? match[4]), stderr: rest.slice(0, match.index), stdout: state.stdout, line: match[0] });
      state.consumed += match.index + match[0].length;
    }
    state.waiters = state.waiters.filter((waiter) => !waiter());
  });
  child.on("exit", (code) => { state.exit = code; state.waiters = state.waiters.filter((waiter) => !waiter()); });
  state.child = child;
  // Resolves when run `number` has finished (a hang is a failure after a minute: no time is asserted).
  state.run = (number) => new Promise((done, fail) => {
    const timer = setTimeout(() => fail(new Error(`no run ${number} after 60 s\nruns: ${JSON.stringify(state.runs)}\nstderr so far: ${state.stderr}`)), 60_000);
    const check = () => {
      if (state.runs.length >= number) { clearTimeout(timer); done(state.runs[number - 1]); return true; }
      if (state.exit !== null) { clearTimeout(timer); fail(new Error(`watch exited with ${state.exit} before run ${number}\n${state.stderr}`)); return true; }
      return false;
    };
    if (!check()) state.waiters.push(check);
  });
  state.stop = () => { children.delete(child); child.kill("SIGTERM"); };
  // Resolves with the exit code when the process ends by itself.
  state.exited = () => new Promise((done, fail) => {
    const timer = setTimeout(() => fail(new Error(`the watch did not stop after 60 s\nstderr so far: ${state.stderr}`)), 60_000);
    const check = () => {
      if (state.exit === null) return false;
      clearTimeout(timer);
      done(state.exit);
      return true;
    };
    if (!check()) state.waiters.push(check);
  });
  return state;
}

// Writes a file and sets its modification time to the fixed one.
function save(path, text) {
  writeFileSync(path, text);
  utimesSync(path, fixed, fixed);
}

try {
  const app = join(root, "app");
  mkdirSync(app);
  const main = join(app, "Main.tz");
  save(main, "42\n");

  // 1 to 6: check, with the output of every run equal to that of a command of its own.
  const checking = watch(["check", "app"]);
  let first = await checking.run(1);
  assert.equal(first.exit, 0);
  assert.equal(first.line, "watch: run 1 finished with exit code 0\n");
  assert.equal(first.stderr, cold(["check", "app"]).stderr);

  save(main, "missing\n");
  let second = await checking.run(2);
  assert.equal(second.exit, 1);
  assert.match(second.stderr, /error\[E1002\]: unknown value 'missing'/);
  assert.equal(second.stderr, cold(["check", "app"]).stderr);

  // The same length and the same modification time: only the status-change time (or, where there is
  // none, the contents that are read once a second) can tell.
  save(main, "mixxing\n");
  let third = await checking.run(3);
  assert.equal(third.exit, 1);
  assert.match(third.stderr, /unknown value 'mixxing'/);
  assert.equal(third.stderr, cold(["check", "app"]).stderr);

  // A touch: the modification time moves and the text does not. No run follows, not even one second later.
  const touched = new Date(fixed.getTime() + 1_000_000);
  utimesSync(main, touched, touched);
  await pause(2_500);
  assert.equal(checking.runs.length, 3, "a touch must not start a run");
  save(main, "43\n");
  let fourth = await checking.run(4);
  assert.equal(fourth.exit, 0, "the next run is the edit, and no run was made for the touch");
  assert.equal(fourth.stderr, cold(["check", "app"]).stderr);

  // A source that appears, and one that goes.
  const extra = join(app, "Extra.tz");
  save(extra, "def broken :: i64 = \n");
  let fifth = await checking.run(5);
  assert.equal(fifth.exit, 1);
  assert.equal(fifth.stderr, cold(["check", "app"]).stderr);
  unlinkSync(extra);
  let sixth = await checking.run(6);
  assert.equal(sixth.exit, 0);
  assert.equal(sixth.stderr, cold(["check", "app"]).stderr);

  // A source in a directory that is new, a manifest, and a hidden file (which is not read).
  mkdirSync(join(app, "Deep"));
  save(join(app, "Deep", "Inner.tz"), "private def helper :: i64 = 1\n");
  await checking.run(7);
  save(join(app, ".Hidden.tz"), "this is not read\n");
  save(join(app, "notes.txt"), "nor is this\n");
  await pause(2_500);
  assert.equal(checking.runs.length, 7, "files that no run reads must not start a run");
  checking.stop();

  // 7: build; the output of every run is that of the same build alone.
  const out = join(root, "out");
  mkdirSync(out);
  save(main, "42\n");
  rmSync(join(app, "Deep"), { recursive: true });
  rmSync(join(app, ".Hidden.tz"));
  const building = watch(["build", "app", "--emit", "llvm", "-O0", "-o", "out/watched.ll"]);
  let built = await building.run(1);
  assert.equal(built.exit, 0);
  assert.equal(cold(["build", "app", "--emit", "llvm", "-O0", "-o", "out/cold.ll"]).status, 0);
  assert.deepEqual(readFileSync(join(out, "watched.ll")), readFileSync(join(out, "cold.ll")));
  save(main, "44\n");
  built = await building.run(2);
  assert.equal(built.exit, 0);
  cold(["build", "app", "--emit", "llvm", "-O0", "-o", "out/cold.ll"]);
  assert.deepEqual(readFileSync(join(out, "watched.ll")), readFileSync(join(out, "cold.ll")));
  assert.match(readFileSync(join(out, "watched.ll"), "utf8"), /44/);
  building.stop();

  // 7b: an executable, where the platform runs one.
  if (process.platform !== "win32") {
    const exe = join(out, "watched-app");
    const executable = watch(["build", "app", "-O0", "--cpu", "generic", "-o", "out/watched-app"]);
    assert.equal((await executable.run(1)).exit, 0);
    assert.equal(spawnSync(exe, [], { encoding: "utf8" }).stdout, "44\n");
    save(main, "45\n");
    assert.equal((await executable.run(2)).exit, 0);
    assert.equal(spawnSync(exe, [], { encoding: "utf8" }).stdout, "45\n");
    executable.stop();
  }

  // 8: --json reports each run as one JSON object, and the run's own diagnostics as JSON too.
  save(main, "42\n");
  const json = watch(["check", "app", "--json"]);
  let jsonRun = await json.run(1);
  assert.equal(jsonRun.line, "{\"type\":\"watch\",\"run\":1,\"exit\":0}\n");
  save(main, "missing\n");
  jsonRun = await json.run(2);
  assert.equal(jsonRun.line, "{\"type\":\"watch\",\"run\":2,\"exit\":1}\n");
  assert.equal(jsonRun.stderr, cold(["check", "app", "--json"]).stderr);
  assert.equal(JSON.parse(jsonRun.stderr.trim()).code, "E1002");
  json.stop();

  // 9: a project that cannot be read yet, and the directory that appears.
  const later = watch(["check", "later"]);
  const missing = await later.run(1);
  assert.equal(missing.exit, 1);
  assert.equal(missing.stderr, cold(["check", "later"]).stderr);
  mkdirSync(join(root, "later"));
  save(join(root, "later", "Main.tz"), "1 + 1\n");
  const appeared = await later.run(2);
  assert.equal(appeared.exit, 0);
  later.stop();

  // 10: tests: a failing test, and the fix.
  const suite = join(root, "suite");
  mkdirSync(suite);
  save(join(suite, "Main.tz"), "def classify :: i64 -> i64 = \\x -> if x > 0 then 1 else 0\n\ntest \"zero\" = Test.is_true (classify 1 == 0)\n");
  const testing = watch(["test", "suite"]);
  const failing = await testing.run(1);
  assert.equal(failing.exit, 1);
  assert.match(failing.stdout, /not ok 1 - Main zero/);
  assert.match(failing.stderr, /E2006/);
  save(join(suite, "Main.tz"), "def classify :: i64 -> i64 = \\x -> if x > 0 then 1 else 0\n\ntest \"zero\" = Test.is_true (classify 0 == 0)\n");
  const passing = await testing.run(2);
  assert.equal(passing.exit, 0);
  assert.match(passing.stdout, /ok 1 - Main zero/);
  testing.stop();

  // 11: only check, build and test are watched; the command is named.
  for (const [args, json] of [[[], false], [["run", "app"], false], [["fmt", "app"], false], [["lsp"], false], [["app"], false], [["watch", "check", "app"], false], [["--json"], true]]) {
    const result = spawnSync(compiler, ["watch", ...args], { cwd: root, env, encoding: "utf8", timeout: 60_000 });
    assert.equal(result.status, 2, `watch ${args.join(" ")}`);
    assert.match(result.stderr, /watch runs check, build, or test; use tsuzuri watch check <path>/);
    if (json) assert.equal(JSON.parse(result.stderr.trim()).code, "E2000");
  }
  const noCache = spawnSync(compiler, ["watch", "check", "app", "--no-cache"], { cwd: root, env, encoding: "utf8", timeout: 60_000 });
  assert.equal(noCache.status, 2);
  assert.match(noCache.stderr, /--no-cache is only valid with build, run, script, or repl/);
  const threads = spawnSync(compiler, ["watch", "check", "app"], { cwd: root, env: { ...env, TSUZURI_THREADS: "0" }, encoding: "utf8", timeout: 60_000 });
  assert.equal(threads.status, 2);
  assert.match(threads.stderr, /TSUZURI_THREADS must be an integer from 1 to 32/);

  // 12: a compiler that is replaced while it watches (a rebuild) ends the watch before another run starts with it,
  // whether the project is edited or not: a run of the old image would show its result to somebody who waits for the
  // new one's, and its build cache would be written under the key of the new file. Windows cannot replace an image
  // that is running, so there is nothing to test there.
  if (process.platform === "win32") {
    console.log("Watch: a replaced compiler is not tested on Windows, which cannot replace a running executable");
  } else {
    save(main, "42\n");
    for (const [name, extra, last] of [["text", [], "watch: the compiler binary changed; stopping (start tsuzuri watch again)\n"], ["json", ["--json"], "{\"type\":\"watch\",\"stopped\":\"compiler-changed\"}\n"]]) {
      for (const [when, edits] of [["while it is idle", false], ["with an edit that follows", true]]) {
        const label = `${name}, ${when}`;
        const copy = join(root, `compiler-${name}`);
        copyFileSync(compiler, copy);
        chmodSync(copy, 0o755);
        const replaced = watch(["check", "app", ...extra], copy);
        assert.equal((await replaced.run(1)).exit, 0, label);
        copyFileSync(compiler, `${copy}.next`);
        chmodSync(`${copy}.next`, 0o755);
        renameSync(`${copy}.next`, copy);
        if (edits) save(main, `${46 + extra.length}\n`);
        assert.equal(await replaced.exited(), 1, label);
        assert.equal(replaced.runs.length, 1, `no run starts with the replaced compiler (${label}): ${replaced.stderr}`);
        assert.ok(replaced.stderr.endsWith(last), `${label}: ${replaced.stderr}`);
      }
    }
  }

  // 13: TSUZURI_TIME_PASSES reports each run alone: the phases of an earlier run are not summed in.
  save(main, "42\n");
  const timed = watch(["check", "app"], compiler, { TSUZURI_TIME_PASSES: "1" });
  const reports = (run) => run.stderr.split("\n").filter((line) => line.startsWith("{\"time_passes\":1,"));
  const timedFirst = await timed.run(1);
  assert.equal(reports(timedFirst).length, 1);
  assert.ok(JSON.parse(reports(timedFirst)[0]).phases.some((phase) => phase.name === "specialize"));
  save(main, "missing\n");
  const timedSecond = await timed.run(2);
  assert.equal(reports(timedSecond).length, 1);
  assert.ok(!JSON.parse(reports(timedSecond)[0]).phases.some((phase) => phase.name === "specialize"), "a failed run has no specialize phase");
  timed.stop();

  console.log(`Watch: runs equal the commands alone, same-length same-time edits, additions, removals, new directories, touches, JSON status lines, build, test, usage errors${process.platform === "win32" ? "" : " and a replaced compiler"} passed`);
} finally {
  for (const child of children) child.kill("SIGTERM");
  rmSync(root, { recursive: true, force: true });
}
