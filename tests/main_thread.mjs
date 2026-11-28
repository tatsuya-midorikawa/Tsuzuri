// M09: `MainThread.post`, `after` and `is_current`, end to end on macOS. Each scene of
// tests/main_thread/app runs on a CFRunLoop of the main thread; the expected lines follow from
// the documented rules (a posted function runs on the main thread, posts run in their order,
// deadlines in theirs, what is still posted at the end of `main` is dropped once without being
// called), never from the compiler under test.
//
// Usage: node tests/main_thread.mjs [path/to/tsuzuri]
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(process.argv[2] ?? join(root, "target", "debug", "tsuzuri"));
const fixture = join(root, "tests", "main_thread", "app");
const handles = join(root, "tests", "main_thread", "handles");
const host = join(root, "tests", "main_thread_host.c");
const clang = process.env.TSUZURI_CLANG ?? "clang";
const bundledClang =
  process.env.TSUZURI_BUNDLED_CLANG ?? join(root, "target", "toolchain", "bin", "tsuzuri-clang");

if (process.platform !== "darwin") {
  console.log("main_thread: skipped (the main dispatch queue needs macOS)");
  process.exit(0);
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { encoding: "utf8", timeout: 120_000, ...options });
  if (result.error && result.error.code !== "ETIMEDOUT") throw result.error;
  return result;
}

function must(command, args, options = {}) {
  const result = run(command, args, options);
  assert.equal(
    result.status,
    0,
    `${command} ${args.join(" ")}\nstatus ${result.status} signal ${result.signal}\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`,
  );
  return result;
}

// CoreFoundation (`--framework CoreFoundation`, M20) only runs and stops the run loop of the
// scenes; `MainThread` itself needs nothing beyond libSystem.

// What each scene prints.
// - `runloop`: a tzroutine posts and schedules; both functions find themselves on the main thread.
// - `from_main`: the posts of the main thread are not called in place (`posted three` is first)
//   and run in the order of the posts.
// - `ran_then_dropped`: the function is dropped right after its call, before `main` goes on.
// - `drop_at_end`: `main` never serves the queue: no function runs, and each capture is dropped
//   once, in the order of the posts, after the body of `main`. The third deadline saturates.
// - `spawn_in_post`: the tzroutine that the posted function starts sleeps past the return of
//   `main`, whose boundary waits for it.
// - `gcd_worker`: a callback on a worker thread of libdispatch is not on the main thread; what it
//   posts is.
// - `after_order`: the deadlines 0, 20 ms and 80 ms run in that order, whatever the order of the
//   calls.
const expected = {
  runloop: "posted on main: true\nafter on main: true\n",
  from_main: "posted three\nfirst\nsecond\nthird\n",
  ran_then_dropped: "ran posted\ndropped posted\nmain returns\n",
  drop_at_end: "main returns\ndropped posted\ndropped after\ndropped forever\n",
  spawn_in_post: "posted ran\nmain returns\nspawned finished\n",
  gcd_worker: "worker on main: false\nposted on main: true\n",
  after_order: "after 0\nafter 20\nafter 80\n",
};

const scratch = mkdtempSync(join(tmpdir(), "tsuzuri-m09-"));
const env = { ...process.env, TSUZURI_CACHE_DIR: "" };
let ran = 0;

// Runs one scene under a time limit: a posted function that is never called would leave the run
// loop running, and a process that outlives the limit is killed and fails the scene.
function scene(executable, name, procs, label) {
  const result = run(executable, [name], {
    env: { ...env, TSUZURI_MAXPROCS: String(procs) },
    timeout: 15_000,
    killSignal: "SIGKILL",
  });
  assert.equal(
    result.signal,
    null,
    `${name} ${label} P${procs}: killed by ${result.signal} (the run loop never stopped?)\n${result.stdout}\n${result.stderr}`,
  );
  assert.equal(result.status, 0, `${name} ${label} P${procs}\n${result.stdout}\n${result.stderr}`);
  assert.equal(result.stdout, expected[name], `${name} ${label} P${procs}`);
  assert.equal(result.stderr, "", `${name} ${label} P${procs}`);
  ran += 1;
}

try {
  for (const optimization of ["-O0", "-O3"]) {
    const executable = join(scratch, `app${optimization}`);
    must(
      compiler,
      ["build", fixture, optimization, "--trap-info", "--framework", "CoreFoundation", "-o", executable],
      { env },
    );
    for (const name of Object.keys(expected)) {
      for (const procs of [1, 4]) scene(executable, name, procs, optimization);
    }
    // An unknown scene returns 2: the dispatch of the fixture itself.
    assert.equal(run(executable, ["nothing"], { env }).status, 2);
  }

  // The runners are boundaries too: a test and a bench sample drop what they posted, and the
  // runner's own thread is the main thread.
  const project = join(scratch, "runners");
  mkdirSync(project);
  writeFileSync(
    join(project, "Main.tz"),
    [
      "def work :: unit -> unit = \\() -> ()",
      "",
      'test "a test runs on the main thread and its posts are dropped" =',
      "    MainThread.post (Owned.function (\\() -> work ()))",
      "    MainThread.after (Time.hour()) (Owned.function (\\() -> work ()))",
      "    assert (MainThread.is_current ())",
      "",
      'test "a tzroutine is not the main thread" =',
      "    let answer: Channel<bool> = Channel.make 1",
      "    let reply = Channel.share answer",
      "    tzroutine {",
      "        MainThread.post (Owned.function (\\() -> work ()))",
      "        Channel.send reply (MainThread.is_current ())",
      "    }",
      "    let on_main = match Channel.recv answer with",
      "        | Maybe.Some current -> current",
      "        | Maybe.None -> true",
      "    assert (on_main == false)",
      "",
      'bench "posts" = Bench.of (\\_ -> MainThread.post (Owned.function (\\() -> work ())))',
      "",
      "0",
      "",
    ].join("\n"),
  );
  for (const optimization of ["-O0", "-O3"]) {
    const tested = must(compiler, ["test", project, optimization], { env, timeout: 120_000 });
    assert.match(tested.stdout, /2 passed/, tested.stdout);
    ran += 1;
  }
  must(compiler, ["bench", project, "--samples", "3"], { env, timeout: 120_000 });
  ran += 1;

  // Phase 2: a callback whose body uses an `extern type main` handle checks its thread on entry.
  // The C host calls the callbacks on the main thread and on a thread of its own.
  // - `callback_on_main`: the body runs (one window while it does, none after).
  // - `plain_callback_on_thread`: a callback without the effect has no check and runs on the
  //   host's thread, which is not the main thread.
  // - `callback_on_thread`: the check traps before the body runs: `calling` is the only line, and
  //   the standard error names the reason.
  const sdk = run("xcrun", ["--show-sdk-path"]);
  const sysroot = sdk.status === 0 ? ["-isysroot", sdk.stdout.trim()] : [];
  const hostObject = join(scratch, "host.o");
  must(clang, ["-c", "-O2", "-pthread", "-mmacosx-version-min=12.0", ...sysroot, host, "-o", hostObject]);
  for (const optimization of ["-O0", "-O3"]) {
    const executable = join(scratch, `handles${optimization}`);
    must(compiler, ["build", handles, optimization, "--link", hostObject, "-o", executable], { env });
    const onMain = must(executable, ["callback_on_main"], { env, timeout: 15_000 });
    assert.equal(onMain.stdout, "event 3: 1 window on main true\nleft 0\n", optimization);
    assert.equal(onMain.stderr, "", optimization);
    const plain = must(executable, ["plain_callback_on_thread"], { env, timeout: 15_000 });
    assert.equal(plain.stdout, "tick 5 on main false\n", optimization);
    const wrong = run(executable, ["callback_on_thread"], { env, timeout: 15_000 });
    // The trap ends the process with SIGTRAP, which a shell reports as exit code 133.
    assert.ok(
      wrong.signal === "SIGTRAP" || wrong.status === 133,
      `callback_on_thread at ${optimization}: status ${wrong.status}, signal ${wrong.signal}\n${wrong.stderr}`,
    );
    assert.equal(wrong.stdout, "calling\n", optimization);
    assert.match(
      wrong.stderr,
      /Tsuzuri runtime: a callback that uses main-thread handles was called on another thread/,
    );
    // Phase 3: a thread of the host that has not joined the runtime reads a shared anchor and
    // sends 41 + 1 to a tzroutine through the channel in it.
    for (const procs of [1, 4]) {
      const shared = must(executable, ["shared_from_thread"], {
        env: { ...env, TSUZURI_MAXPROCS: String(procs) },
        timeout: 15_000,
      });
      assert.equal(shared.stdout, "tzroutine received 42\n", `${optimization} P${procs}`);
      assert.equal(shared.stderr, "", `${optimization} P${procs}`);
    }
    ran += 5;
  }

  // The counting allocator: the boxes of the functions that ran and of those that were dropped are
  // all freed, so the program's heap is balanced at the end of `main`.
  const counted = join(scratch, "counted");
  must(
    compiler,
    ["build", fixture, "-O3", "--allocator", "counting", "--framework", "CoreFoundation", "-o", counted],
    { env },
  );
  for (const name of ["from_main", "drop_at_end", "after_order"]) scene(counted, name, 4, "counting");

  // The bundled toolchain (zig as the linker driver, with lld) compiles the runtime part against
  // its own libSystem headers and links the same program. Without one the case is skipped.
  if (existsSync(bundledClang)) {
    const executable = join(scratch, "app-bundled");
    must(compiler, ["build", fixture, "-O3", "--framework", "CoreFoundation", "-o", executable], {
      env: { ...env, TSUZURI_CLANG: bundledClang },
    });
    for (const name of Object.keys(expected)) scene(executable, name, 4, "bundled");
  } else {
    console.log(`main_thread: the bundled toolchain is not at ${bundledClang}; its case is skipped`);
  }
} finally {
  rmSync(scratch, { recursive: true, force: true });
}
console.log(`main_thread: ${ran} checks passed`);
