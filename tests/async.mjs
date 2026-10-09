// B08 Phase 2 and 3: async computations that a host drives.
//
//   node tests/async.mjs target/release/tsuzuri
//
// The host-driven executor (`Async.start`, `Async.host`, `tsuzuri_async_poll`,
// `tsuzuri_async_complete`) runs under a C host, natively with every allocation tracked, and
// under a JavaScript host for wasm32, at -O0 and -O3. The pure executor `Async.run` is the
// `async` suite of tests/features.mjs.
import assert from "node:assert/strict";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { spawnSync } from "node:child_process";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(process.argv[2] ?? join(root, "target/debug/tsuzuri"));
const clang = process.env.TSUZURI_CLANG ?? "clang";
const sanitizer = process.env.TSUZURI_ASAN === "1" ? ["-fsanitize=address"] : [];
const MAX = (1n << 63n) - 1n;

function execute(program, args, success = true) {
  const result = spawnSync(program, args, { cwd: root, encoding: "utf8", timeout: 180_000, maxBuffer: 16 * 1024 * 1024 });
  if (result.error) throw result.error;
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}
const cli = (args) => execute(compiler, args);
const tracked = (ir) => {
  const instrumented = ir.replaceAll("@malloc", "@tracked_alloc").replaceAll("@free", "@tracked_free").replaceAll("@realloc", "@tracked_realloc");
  return sanitizer.length ? instrumented.replace(/ nounwind(?=[^{}\n]* \{)/g, " nounwind sanitize_address") : instrumented;
};

// The C host of tests/fixtures/async_host: it records the operations that start and the
// cancellations and reports, and can call back into the executor from a report.
const TRACKING = `
#include <assert.h>
#include <stdint.h>
#include <stdatomic.h>
#include <stdlib.h>
static _Atomic uint64_t live;
void *tracked_alloc(uint64_t size) {
    uint64_t *p = malloc((size_t)size + 16);
    assert(p);
    p[0] = size;
    p[1] = UINT64_C(0x51a110ca7e);
    live += size;
    return p + 2;
}
void tracked_free(void *value) {
    if (!value) return;
    uint64_t *p = (uint64_t *)value - 2;
    assert(p[1] == UINT64_C(0x51a110ca7e));
    p[1] = 0;
    assert(live >= p[0]);
    live -= p[0];
    free(p);
}
void *tracked_realloc(void *value, uint64_t size) {
    if (!value) return tracked_alloc(size);
    if (!size) { tracked_free(value); return NULL; }
    uint64_t *previous = (uint64_t *)value - 2;
    assert(previous[1] == UINT64_C(0x51a110ca7e));
    uint64_t old_size = previous[0];
    uint64_t *next = realloc(previous, (size_t)size + 16);
    assert(next);
    next[0] = size;
    next[1] = UINT64_C(0x51a110ca7e);
    if (size >= old_size) live += size - old_size; else live -= old_size - size;
    return next + 2;
}
`;

// Threads and a monotonic clock for the C hosts: POSIX threads, or Win32 on Windows.
const PORTABLE = `
#if defined(_WIN32)
#include <windows.h>
typedef HANDLE tz_thread;
struct tz_launch { void *(*run)(void *); void *argument; };
static DWORD WINAPI tz_launch_main(LPVOID raw) {
    struct tz_launch launch = *(struct tz_launch *)raw;
    free(raw);
    launch.run(launch.argument);
    return 0;
}
static int tz_thread_create(tz_thread *thread, void *(*run)(void *), void *argument) {
    struct tz_launch *launch = malloc(sizeof *launch);
    if (!launch) return 1;
    launch->run = run;
    launch->argument = argument;
    *thread = CreateThread(NULL, 0, tz_launch_main, launch, 0, NULL);
    if (!*thread) { free(launch); return 1; }
    return 0;
}
static int tz_thread_join(tz_thread thread) { return WaitForSingleObject(thread, INFINITE) == WAIT_OBJECT_0 && CloseHandle(thread) ? 0 : 1; }
static int tz_thread_detach(tz_thread thread) { return CloseHandle(thread) ? 0 : 1; }
static void tz_sleep_milliseconds(int64_t milliseconds) { Sleep((DWORD)milliseconds); }
static int64_t tz_monotonic_milliseconds(void) {
    LARGE_INTEGER frequency, counter;
    QueryPerformanceFrequency(&frequency);
    QueryPerformanceCounter(&counter);
    return counter.QuadPart / frequency.QuadPart * 1000 + counter.QuadPart % frequency.QuadPart * 1000 / frequency.QuadPart;
}
#else
#include <pthread.h>
#include <time.h>
typedef pthread_t tz_thread;
static int tz_thread_create(tz_thread *thread, void *(*run)(void *), void *argument) { return pthread_create(thread, NULL, run, argument); }
static int tz_thread_join(tz_thread thread) { return pthread_join(thread, NULL); }
static int tz_thread_detach(tz_thread thread) { return pthread_detach(thread); }
static void tz_sleep_milliseconds(int64_t milliseconds) {
    struct timespec delay = {(time_t)(milliseconds / 1000), (long)(milliseconds % 1000) * 1000000L};
    nanosleep(&delay, NULL);
}
static int64_t tz_monotonic_milliseconds(void) {
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return (int64_t)now.tv_sec * 1000 + now.tv_nsec / 1000000;
}
#endif
`;

// Native executables take the platform's suffix, and POSIX programs that start threads link them.
const windows = process.platform === "win32";
const exe = (path) => (windows ? `${path}.exe` : path);
const threads = windows ? [] : ["-pthread"];

const HOST = `${TRACKING}${PORTABLE}
#include "tz-async_host.h"
static int64_t started_operation[16], started_request[16], cancelled[16], report_tag[16], report_value[16];
static int started, cancels, reports, reenter, complete_inside;
void tsuzuri_host_Main_start_operation(int64_t operation, int64_t request) {
    started_operation[started] = operation;
    started_request[started++] = request;
    // An operation may complete before its start returns.
    if (complete_inside) tsuzuri_async_complete(operation, request * 2);
}
void tsuzuri_host_Main_cancel_operation(int64_t operation) { cancelled[cancels++] = operation; }
void tsuzuri_host_Main_report(int64_t tag, int64_t value) {
    report_tag[reports] = tag;
    report_value[reports++] = value;
    if (reenter) tsuzuri_async_poll(0);
}
static void reset(void) { started = cancels = reports = reenter = complete_inside = 0; }
// Another thread has an executor of its own: it runs only the computations started on it.
static void *elsewhere(void *unused) {
    (void)unused;
    tz_sleeping(1);
    assert(tsuzuri_async_poll(0) == 1);
    assert(tsuzuri_async_poll(1) == -1 && reports == 1 && report_value[0] == 1);
    return NULL;
}
int main(int argc, char **argv) {
    if (argc == 2) {
        switch (atoi(argv[1])) {
        case 0: tsuzuri_async_complete(99, 0); break;
        case 1: tz_sequential(1); tsuzuri_async_poll(0); tsuzuri_async_complete(started_operation[0], 1); tsuzuri_async_complete(started_operation[0], 1); break;
        case 2: reenter = 1; tz_yielding(1); tsuzuri_async_poll(0); break;
        case 3: (void)tz_run_host(1); break;
        case 4: tz_cancelling(4); tsuzuri_async_poll(0); tsuzuri_async_poll(5); tsuzuri_async_complete(started_operation[0], 1); break;
        }
        return 0;
    }
    reset();
    tz_sequential(10);
    assert(started == 0 && live > 0);
    assert(tsuzuri_async_poll(0) == INT64_MAX && started == 1 && started_request[0] == 10);
    tsuzuri_async_complete(started_operation[0], 100);
    assert(tsuzuri_async_poll(0) == INT64_MAX && started == 2 && started_request[1] == 11);
    assert(started_operation[1] > started_operation[0]);
    tsuzuri_async_complete(started_operation[1], 23);
    assert(tsuzuri_async_poll(0) == -1 && reports == 1 && report_tag[0] == 1 && report_value[0] == 123);
    assert(live == 0);

    reset();
    tz_concurrent(5);
    assert(tsuzuri_async_poll(0) == INT64_MAX && started == 2);
    tsuzuri_async_complete(started_operation[1], 7);
    tsuzuri_async_complete(started_operation[0], 3);
    assert(tsuzuri_async_poll(0) == -1 && reports == 1 && report_value[0] == 3007);
    assert(live == 0);

    reset();
    tz_cancelling(4);
    assert(tsuzuri_async_poll(0) == 5 && started == 1);
    assert(tsuzuri_async_poll(3) == 5 && cancels == 0);
    assert(tsuzuri_async_poll(5) == -1 && cancels == 1 && cancelled[0] == started_operation[0]);
    assert(reports == 1 && report_tag[0] == 3 && report_value[0] == -4);
    assert(live == 0);

    reset();
    tz_yielding(3);
    for (int64_t step = 0; step < 3; step++) assert(tsuzuri_async_poll(step) == step && reports == step + 1 && report_value[step] == step);
    assert(tsuzuri_async_poll(3) == -1 && reports == 4 && report_value[3] == 3);
    assert(live == 0);

    reset();
    tz_sleeping(100);
    assert(tsuzuri_async_poll(10) == 110 && tsuzuri_async_poll(50) == 110 && reports == 0);
    assert(tsuzuri_async_poll(110) == -1 && reports == 1 && report_value[0] == 110);
    assert(live == 0);

    // The clock never goes back: an earlier time keeps the latest one.
    reset();
    tz_sleeping(5);
    assert(tsuzuri_async_poll(200) == 205 && tsuzuri_async_poll(100) == 205 && tsuzuri_async_poll(205) == -1);
    assert(report_value[0] == 205 && live == 0);

    // Completions inside the start callback reach the computation at the next poll.
    reset();
    complete_inside = 1;
    tz_sequential(10);
    assert(tsuzuri_async_poll(0) == 0 && started == 1);
    assert(tsuzuri_async_poll(0) == 0 && started == 2);
    assert(tsuzuri_async_poll(0) == -1 && report_value[0] == 20 + 22);
    assert(live == 0);

    // Computations started together share the executor; none runs until a poll.
    reset();
    tz_sleeping(3);
    tz_concurrent(1);
    tz_yielding(1);
    assert(tsuzuri_async_poll(0) == 0 && started == 2 && reports == 1);
    tsuzuri_async_complete(started_operation[0], 1);
    tsuzuri_async_complete(started_operation[1], 2);
    // Both completions reach the concurrent computation, and the yielding one finishes.
    assert(tsuzuri_async_poll(1) == 3 && reports == 3 && report_value[1] == 1002 && report_value[2] == 1);
    assert(tsuzuri_async_poll(2) == 3 && reports == 3);
    assert(tsuzuri_async_poll(3) == -1 && reports == 4 && report_tag[3] == 5 && report_value[3] == 3);
    assert(live == 0);
    // Without computations a poll does nothing.
    assert(tsuzuri_async_poll(0) == -1 && live == 0);

    // A computation started on this thread waits for this thread's poll, whatever other threads do.
    reset();
    tz_sleeping(2);
    tz_thread thread;
    assert(tz_thread_create(&thread, elsewhere, NULL) == 0);
    assert(tz_thread_join(thread) == 0);
    assert(tsuzuri_async_poll(0) == 2 && reports == 1);
    assert(tsuzuri_async_poll(2) == -1 && reports == 2 && report_value[1] == 2);
    assert(live == 0);
    return 0;
}
`;
const TRAPS = ["unknown operation", "second completion", "poll inside a poll", "Async.run of a host operation", "completion after cancellation"];

function hostDriven(directory) {
  const fixture = join(root, "tests/fixtures/async_host");
  cli(["check", fixture]);
  const header = join(directory, "tz-async_host.h");
  const ir = join(directory, "async_host.ll");
  const again = join(directory, "again.ll");
  cli(["build", fixture, "--emit", "header", "-o", header]);
  const prototypes = readFileSync(header, "utf8");
  assert.match(prototypes, /^int64_t tsuzuri_async_poll\(int64_t now\);$/m);
  assert.match(prototypes, /^void tsuzuri_async_complete\(int64_t operation, int64_t value\);$/m);
  cli(["build", fixture, "--emit", "llvm", "-o", ir]);
  cli(["build", fixture, "--emit", "llvm", "-o", again]);
  const source = readFileSync(ir, "utf8");
  assert.equal(source, readFileSync(again, "utf8"), "async_host: IR is deterministic");
  writeFileSync(ir, tracked(source));
  const host = join(directory, "host.c");
  writeFileSync(host, HOST);
  for (const optimization of ["0", "3"]) {
    const native = exe(join(directory, `async_host-O${optimization}`));
    execute(clang, [`-O${optimization}`, "-Wno-override-module", ...sanitizer, `-I${directory}`, ir, host, ...threads, "-o", native]);
    execute(native, []);
    for (let index = 0; index < TRAPS.length; index++) {
      const result = execute(native, [String(index)], false);
      assert.ok(result.status !== 0 || result.signal, `native O${optimization}: ${TRAPS[index]} must trap`);
    }
    const wasm = join(directory, `async_host-O${optimization}.wasm`);
    cli(["build", fixture, "--target", "wasm32", `-O${optimization}`, "-o", wasm]);
    const module = new WebAssembly.Module(readFileSync(wasm));
    assert.deepEqual(WebAssembly.Module.imports(module).map(({ module, name }) => `${module}.${name}`),
      ["tsuzuri.Main.start_operation", "tsuzuri.Main.cancel_operation", "tsuzuri.Main.report"], "only the program's externs are imported");
    wasmHost(module, optimization);
  }
  // A program that does not start computations has no entry points.
  const plain = join(directory, "Plain.tz");
  writeFileSync(plain, "export def pure :: i64 -> i64\nfn pure n = Async.run (Async { do! Async.sleep 2; return n })");
  const plainIr = join(directory, "plain.ll");
  cli(["build", plain, "--emit", "llvm", "-o", plainIr]);
  assert.doesNotMatch(readFileSync(plainIr, "utf8"), /tsuzuri_async_/);
  console.log("async: host-driven executor, native (live == 0) and WASM at O0/O3 passed");
}

function wasmHost(module, optimization) {
  const make = () => {
    const log = { started: [], cancelled: [], reports: [], reenter: false, completeInside: false };
    let exports;
    const instance = new WebAssembly.Instance(module, { tsuzuri: {
      "Main.start_operation": (operation, request) => {
        log.started.push([operation, request]);
        if (log.completeInside) exports.tsuzuri_async_complete(operation, request * 2n);
      },
      "Main.cancel_operation": (operation) => log.cancelled.push(operation),
      "Main.report": (tag, value) => {
        log.reports.push([tag, value]);
        if (log.reenter) exports.tsuzuri_async_poll(0n);
      },
    } });
    exports = instance.exports;
    return { exports, log };
  };
  const where = `WASM O${optimization}`;
  {
    const { exports, log } = make();
    exports.tz_sequential(10n);
    assert.equal(exports.tsuzuri_async_poll(0n), MAX, where);
    exports.tsuzuri_async_complete(log.started[0][0], 100n);
    assert.equal(exports.tsuzuri_async_poll(0n), MAX, where);
    assert.deepEqual(log.started.map(([, request]) => request), [10n, 11n], where);
    exports.tsuzuri_async_complete(log.started[1][0], 23n);
    assert.equal(exports.tsuzuri_async_poll(0n), -1n, where);
    assert.deepEqual(log.reports, [[1n, 123n]], where);
  }
  {
    const { exports, log } = make();
    exports.tz_concurrent(5n);
    exports.tsuzuri_async_poll(0n);
    exports.tsuzuri_async_complete(log.started[1][0], 7n);
    exports.tsuzuri_async_complete(log.started[0][0], 3n);
    assert.equal(exports.tsuzuri_async_poll(0n), -1n, where);
    assert.deepEqual(log.reports, [[2n, 3007n]], where);
  }
  {
    const { exports, log } = make();
    exports.tz_cancelling(4n);
    assert.equal(exports.tsuzuri_async_poll(0n), 5n, where);
    assert.equal(exports.tsuzuri_async_poll(5n), -1n, where);
    assert.deepEqual(log.cancelled, [log.started[0][0]], where);
    assert.deepEqual(log.reports, [[3n, -4n]], where);
  }
  {
    const { exports, log } = make();
    exports.tz_yielding(3n);
    assert.deepEqual([0n, 1n, 2n, 3n].map((now) => exports.tsuzuri_async_poll(now)), [0n, 1n, 2n, -1n], where);
    assert.deepEqual(log.reports.map(([, value]) => value), [0n, 1n, 2n, 3n], where);
  }
  {
    const { exports, log } = make();
    exports.tz_sleeping(100n);
    assert.deepEqual([10n, 50n, 110n].map((now) => exports.tsuzuri_async_poll(now)), [110n, 110n, -1n], where);
    assert.deepEqual(log.reports, [[5n, 110n]], where);
  }
  {
    const { exports, log } = make();
    log.completeInside = true;
    exports.tz_sequential(10n);
    assert.deepEqual([0n, 0n, 0n].map((now) => exports.tsuzuri_async_poll(now)), [0n, 0n, -1n], where);
    assert.deepEqual(log.reports, [[1n, 42n]], where);
  }
  const traps = [
    ({ exports }) => exports.tsuzuri_async_complete(99n, 0n),
    ({ exports, log }) => { exports.tz_sequential(1n); exports.tsuzuri_async_poll(0n); exports.tsuzuri_async_complete(log.started[0][0], 1n); exports.tsuzuri_async_complete(log.started[0][0], 1n); },
    ({ exports, log }) => { log.reenter = true; exports.tz_yielding(1n); exports.tsuzuri_async_poll(0n); },
    ({ exports }) => exports.tz_run_host(1n),
    ({ exports, log }) => { exports.tz_cancelling(4n); exports.tsuzuri_async_poll(0n); exports.tsuzuri_async_poll(5n); exports.tsuzuri_async_complete(log.started[0][0], 1n); },
  ];
  traps.forEach((trap, index) => assert.throws(() => trap(make()), WebAssembly.RuntimeError, `${where}: ${TRAPS[index]}`));
}

// The C host of tests/fixtures/async_reactor: operations complete 20 ms later on threads of
// their own through tsuzuri_async_post, except those with a negative request. Operations start on
// the workers of Task.parallel too, so the host counts them atomically.
const REACTOR = `${TRACKING}${PORTABLE}
#include "tz-async_reactor.h"
static int64_t started_operation[16], cancelled[16], report_tag[16], report_value[16];
static _Atomic int started;
static int cancels, reports;
struct job { int64_t operation, value; };
static void *complete_later(void *argument) {
    struct job job = *(struct job *)argument;
    free(argument);
    tz_sleep_milliseconds(20);
    assert(tsuzuri_async_post(job.operation, job.value) == 1);
    return NULL;
}
void tsuzuri_host_Main_start_operation(int64_t operation, int64_t request) {
    started_operation[atomic_fetch_add(&started, 1)] = operation;
    if (request < 0) return;
    struct job *job = malloc(sizeof *job);
    assert(job);
    job->operation = operation;
    job->value = request * 10;
    tz_thread thread;
    assert(tz_thread_create(&thread, complete_later, job) == 0);
    assert(tz_thread_detach(thread) == 0);
}
void tsuzuri_host_Main_cancel_operation(int64_t operation) {
    cancelled[cancels++] = operation;
    // A completion that races with cancellation is rejected without retaining a mailbox.
    assert(tsuzuri_async_post(operation, 0) == 0);
}
void tsuzuri_host_Main_report(int64_t tag, int64_t value) {
    report_tag[reports] = tag;
    report_value[reports++] = value;
}
int main(void) {
    assert(tsuzuri_async_post(5, 0) == 0);
    assert(tsuzuri_async_post((INT64_C(999) << 39) | 1, 0) == 0 && live == 0);
    int64_t start = tz_monotonic_milliseconds();
    assert(tz_timers(30) == 1);
    assert(tz_monotonic_milliseconds() - start >= 60);
    assert(live == 0);
    assert(tz_completed(4) == 40 * 1000 + 50 && started == 2 && live == 0);
    assert(tsuzuri_async_post(started_operation[0], 1) == 0 && live == 0);
    started = 0;
    assert(tz_cancelled(-3) == 3 && started == 1 && cancels == 1 && cancelled[0] == started_operation[0]);
    assert(live == 0);
    assert(tz_background(21) == 42 && reports == 1 && report_tag[0] == 1 && report_value[0] == 21);
    assert(live == 0);
    // Each worker waits for its own operation; the posts reach the threads that began them.
    started = 0;
    assert(tz_parallel_completed(1) == 10 * (1 + 2 + 3 + 4) && started == 4);
    // A bounded pool may reuse a worker for several jobs, but never reuses an operation id.
    for (int index = 0; index < 4; index++) {
        assert((started_operation[index] >> 39) > 0);
        for (int other = 0; other < index; other++) {
            assert(started_operation[index] != started_operation[other]);
        }
    }
    assert(live == 0);
    return 0;
}
`;

function reactor(directory) {
  const fixture = join(root, "tests/fixtures/async_reactor");
  const header = join(directory, "tz-async_reactor.h");
  const ir = join(directory, "async_reactor.ll");
  cli(["build", fixture, "--emit", "header", "-o", header]);
  assert.match(readFileSync(header, "utf8"), /^int32_t tsuzuri_async_post\(int64_t operation, int64_t value\);$/m);
  cli(["build", fixture, "--emit", "llvm", "-o", ir]);
  const source = readFileSync(ir, "utf8");
  assert.match(source, /^declare void @tsuzuri_async_wait\(i64, i64\)$/m);
  writeFileSync(ir, tracked(source));
  const host = join(directory, "reactor.c");
  writeFileSync(host, REACTOR);
  for (const optimization of ["0", "3"]) {
    for (const processors of [undefined, 1, 2]) {
      const task = join(root, "src/runtime/task.c");
      const runtime = processors === undefined ? task : join(directory, `task-${processors}.c`);
      // A header name is not a string literal: forward slashes keep a Windows path as it is.
      if (processors !== undefined) writeFileSync(runtime, `#define TZ_TASK_SYSCONF(name) ${processors}\n#include "${task.replaceAll("\\", "/")}"\n`);
      const native = exe(join(directory, `async_reactor-O${optimization}-${processors ?? "host"}`));
      execute(clang, [`-O${optimization}`, "-Wno-override-module", ...sanitizer, `-I${directory}`, ir, host,
        join(root, "src/runtime/async.c"), runtime, ...threads, "-o", native]);
      execute(native, []);
    }
  }
  // The driver links the reactor into executables, objects, and shared libraries by itself.
  const program = join(directory, "Main.tz");
  writeFileSync(program, "def main :: unit -> i32 = \\() ->\n    let! value = Async.block_on (Async { do! Async.sleep 5; let! later = Async.now (); return later })\n    if value > 0 then 0 else 1\n");
  const reactorProgram = exe(join(directory, "reactor-program"));
  cli(["build", program, "-o", reactorProgram]);
  execute(reactorProgram, []);
  const wasm = join(directory, "plain.wasm");
  const refused = execute(compiler, ["build", fixture, "--target", "wasm32", "-o", wasm], false);
  assert.notEqual(refused.status, 0);
  assert.match(refused.stderr, /E2000.*--wasm-feature jspi/);
  console.log("async: block_on reactor natively, posted completions and cancellation (live == 0) at O0/O3 passed");
}

// JSPI suspends the WebAssembly stack in `tsuzuri_async.wait` (Node.js 24 or newer).
async function jspi(directory) {
  if (typeof WebAssembly.Suspending !== "function") {
    console.log("async: JSPI checks skipped (WebAssembly.Suspending needs Node.js 24 or newer)");
    return;
  }
  const fixture = join(root, "tests/fixtures/async_reactor");
  for (const target of ["wasm32", "wasm64"]) for (const optimization of ["0", "3"]) {
    const wasm = join(directory, `async_reactor-jspi-${target}-O${optimization}.wasm`);
    cli(["build", fixture, "--target", target, "--wasm-feature", "jspi", `-O${optimization}`, "-o", wasm]);
    const module = new WebAssembly.Module(readFileSync(wasm));
    assert.deepEqual(WebAssembly.Module.imports(module).map(({ module, name }) => `${module}.${name}`),
      ["tsuzuri.Main.start_operation", "tsuzuri.Main.cancel_operation", "tsuzuri.Main.report", "tsuzuri_async.clock", "tsuzuri_async.wait"]);
    const log = { started: [], cancelled: [], reports: [] };
    let exports;
    let wake = null;
    const imports = {
      tsuzuri: {
        "Main.start_operation": (operation, request) => {
          log.started.push(operation);
          if (request >= 0n) setTimeout(() => { exports.tsuzuri_async_complete(operation, request * 10n); wake?.(); }, 20);
        },
        "Main.cancel_operation": (operation) => log.cancelled.push(operation),
        "Main.report": (tag, value) => log.reports.push([tag, value]),
      },
      tsuzuri_async: {
        clock: () => BigInt(Math.floor(performance.now())),
        wait: new WebAssembly.Suspending((deadline) => new Promise((resolve) => {
          const timer = deadline >= MAX ? undefined : setTimeout(resolve, Math.max(0, Number(deadline) - performance.now()));
          wake = () => { clearTimeout(timer); wake = null; resolve(); };
        })),
      },
    };
    const instance = new WebAssembly.Instance(module, imports);
    exports = instance.exports;
    for (const epoch of [-1n, 0n, 1n << 24n]) {
      assert.throws(() => new WebAssembly.Instance(module, imports).exports.tsuzuri_async_set_epoch(epoch), WebAssembly.RuntimeError);
    }
    exports.tsuzuri_async_set_epoch(9n);
    const run = (name) => WebAssembly.promising(exports[`tz_${name}`]);
    const where = `JSPI ${target} O${optimization}`;
    const start = performance.now();
    assert.equal(await run("timers")(30n), 1n, where);
    assert.ok(performance.now() - start >= 59, where);
    assert.equal(await run("completed")(4n), 40050n, where);
    assert.equal(log.started[0] >> 39n, 9n, where);
    log.started.length = 0;
    assert.equal(await run("cancelled")(-3n), 3n, where);
    assert.deepEqual(log.cancelled, [log.started[0]], where);
    assert.equal(await run("background")(21n), 42n, where);
    assert.deepEqual(log.reports, [[1n, 21n]], where);
    assert.throws(() => exports.tsuzuri_async_set_epoch(10n), WebAssembly.RuntimeError, where);
  }
  console.log("async: block_on under JSPI on wasm32/wasm64 at O0/O3 passed");
}

// The generated JavaScript bindings drive the executor of Async.start on the event loop and, under
// JSPI, return promises from the exports (E13 glue).
async function glue(directory) {
  const hostFixture = join(root, "tests/fixtures/async_host");
  const hostGlue = join(directory, "host.mjs");
  const hostWasm = join(directory, "host.wasm");
  cli(["build", hostFixture, "--target", "wasm32", "--emit", "bindings-js", "-o", hostGlue]);
  cli(["build", hostFixture, "--target", "wasm32", "-o", hostWasm]);
  const declarations = readFileSync(join(directory, "host.d.mts"), "utf8");
  assert.match(declarations, /^export interface AsyncHost \{$/m);
  assert.match(declarations, /^export interface Bindings \{ readonly async: AsyncHost \}$/m);
  const { load } = await import(pathToFileURL(hostGlue).href);
  const reports = [];
  const cancelled = [];
  const started = [];
  let notifyStart;
  let bindings;
  bindings = await load(readFileSync(hostWasm), { imports: {
    "Main.start_operation": (operation, request) => {
      started.push(operation);
      notifyStart?.(operation);
      if (request < 0n) return;
      new Promise((resolve) => setTimeout(() => resolve(request * 10n), 5)).then((value) => bindings.async.complete(operation, value));
    },
    "Main.cancel_operation": (operation) => cancelled.push(operation),
    "Main.report": (tag, value) => reports.push([tag, value]),
  } });
  bindings.exports.sequential(1n);
  bindings.exports.concurrent(2n);
  bindings.exports.sleeping(30n);
  const start = performance.now();
  await bindings.async.settled();
  assert.ok(performance.now() - start >= 29);
  const byTag = new Map(reports);
  assert.equal(reports.length, 3);
  assert.equal(byTag.get(1n), 10n + 20n);
  assert.equal(byTag.get(2n), 20n * 1000n + 30n);
  assert.ok(byTag.get(5n) >= 30n);
  bindings.exports.cancelling(-4n);
  await bindings.async.settled();
  assert.deepEqual(reports.at(-1), [3n, 4n]);
  assert.equal(cancelled.length, 1);
  const otherStarted = [];
  let other;
  other = await load(readFileSync(hostWasm), { imports: {
    "Main.start_operation": (operation) => {
      otherStarted.push(operation);
      other.async.complete(operation, 21n);
    },
    "Main.cancel_operation": () => {},
    "Main.report": () => {},
  } });
  const scheduled = new Map();
  const setTimer = globalThis.setTimeout;
  const clearTimer = globalThis.clearTimeout;
  let done;
  try {
    globalThis.setTimeout = (callback) => {
      const handle = Symbol("timer");
      scheduled.set(handle, callback);
      return handle;
    };
    globalThis.clearTimeout = (handle) => scheduled.delete(handle);
    other.exports.sequential(-1n);
    done = other.async.settled();
    let polls = 0;
    while (scheduled.size) {
      assert.equal(scheduled.size, 1, "synchronous completions must leave only one poll timer");
      const [handle, callback] = scheduled.entries().next().value;
      scheduled.delete(handle);
      callback();
      assert.ok(++polls <= 3, "two synchronous host operations need three polls");
    }
    assert.equal(polls, 3);
  } finally {
    globalThis.setTimeout = setTimer;
    globalThis.clearTimeout = clearTimer;
  }
  await done;
  assert.notEqual(otherStarted[0], started[0]);
  assert.throws(() => bindings.async.complete(otherStarted[0], 0n), /different or discarded Async instance/);
  assert.throws(() => bindings.async.complete(1, 2n), TypeError);
  assert.throws(() => bindings.async.complete(1n << 64n, 0n), RangeError);
  assert.throws(() => bindings.async.complete(1n, 1n << 63n), RangeError);
  bindings.exports.failing(0n);
  await assert.rejects(bindings.async.settled(), (error) => error.name === "TsuzuriTrap");
  await assert.rejects(bindings.async.settled(), (error) => error.name === "TsuzuriTrap");
  await bindings.ready();
  const stale = started[0];
  const freshStart = new Promise((resolve) => { notifyStart = resolve; });
  bindings.exports.sequential(-10n);
  const fresh = await freshStart;
  assert.notEqual(fresh, stale);
  assert.throws(() => bindings.async.complete(stale, 99n), /discarded Async instance/);
  const secondStart = new Promise((resolve) => { notifyStart = resolve; });
  bindings.async.complete(fresh, 20n);
  bindings.async.complete(await secondStart, 22n);
  await bindings.async.settled();
  assert.deepEqual(reports.at(-1), [1n, 42n]);
  notifyStart = undefined;
  // A completion that nothing waits for traps, which discards the instance.
  assert.throws(() => bindings.async.complete(99n, 0n), (error) => error.name === "TsuzuriTrap");
  await assert.rejects(bindings.async.settled(), (error) => error.name === "TsuzuriTrap");
  await bindings.ready();
  const tsc = process.env.TSUZURI_TSC ?? join(root, "vsc/node_modules/typescript/bin/tsc");
  if (existsSync(tsc)) {
    const consumer = join(directory, "consumer.mts");
    writeFileSync(consumer, `import { load, type AsyncHost } from "./host.mjs";
const bindings = await load(new Uint8Array(), { imports: { "Main.start_operation": (_operation: bigint, _request: bigint) => {}, "Main.cancel_operation": (_operation: bigint) => {}, "Main.report": (_tag: bigint, _value: bigint) => {} } });
const host: AsyncHost = bindings.async;
host.complete(1n, 2n);
const done: Promise<void> = host.settled();
bindings.exports.sequential(1n);
void done;
`);
    const typed = spawnSync(process.execPath, [tsc, "--noEmit", "--strict", "--module", "nodenext", "--moduleResolution", "nodenext", "--target", "es2022", consumer], { encoding: "utf8", cwd: directory });
    assert.equal(typed.status, 0, typed.stdout + typed.stderr);
  }
  if (typeof WebAssembly.Suspending === "function") {
    const reactorFixture = join(root, "tests/fixtures/async_reactor");
    const reactorGlue = join(directory, "reactor.mjs");
    const reactorWasm = join(directory, "reactor.wasm");
    cli(["build", reactorFixture, "--target", "wasm32", "--wasm-feature", "jspi", "--emit", "bindings-js", "-o", reactorGlue]);
    cli(["build", reactorFixture, "--target", "wasm32", "--wasm-feature", "jspi", "-o", reactorWasm]);
    assert.match(readFileSync(join(directory, "reactor.d.mts"), "utf8"), /^  completed\(arg0: bigint\): Promise<bigint>;$/m);
    const { load: loadReactor } = await import(pathToFileURL(reactorGlue).href);
    const log = [];
    let reactorBindings;
    reactorBindings = await loadReactor(readFileSync(reactorWasm), { imports: {
      "Main.start_operation": (operation, request) => {
        if (request >= 0n) setTimeout(() => reactorBindings.async.complete(operation, request * 10n), 10);
      },
      "Main.cancel_operation": (operation) => log.push(["cancel", operation]),
      "Main.report": (tag, value) => log.push([tag, value]),
    } });
    const pending = reactorBindings.exports.completed(4n);
    assert.ok(pending instanceof Promise);
    assert.equal(await pending, 40050n);
    assert.deepEqual(await Promise.all([
      reactorBindings.exports.completed(6n),
      reactorBindings.exports.completed(8n),
    ]), [60070n, 80090n]);
    const input = BigInt64Array.of(1n, 2n);
    const buffers = [reactorBindings.exports.buffered(input, 20n), reactorBindings.exports.buffered(input, 0n)];
    input[0] = 99n;
    assert.deepEqual(await Promise.all(buffers), [BigInt64Array.of(1n, 2n), BigInt64Array.of(1n, 2n)]);
    assert.equal(reactorBindings.withBorrowed, undefined);
    assert.equal(await reactorBindings.exports.timers(20n), 1n);
    assert.equal(await reactorBindings.exports.cancelled(-3n), 3n);
    assert.equal(await reactorBindings.exports.background(21n), 42n);
    assert.deepEqual(log.map(([kind]) => kind), ["cancel", 1n]);
  }
  // The executor is thread-local, which WebAssembly threads do not set up: their bindings and
  // modules refuse it.
  for (const emit of [["--emit", "bindings-js", "-o", join(directory, "threads.mjs")], ["-o", join(directory, "threads.wasm")]]) {
    const refused = execute(compiler, ["build", hostFixture, "--target", "wasm32", "--wasm-feature", "threads", ...emit], false);
    assert.notEqual(refused.status, 0);
    assert.match(refused.stderr, /E2000.*--wasm-feature threads cannot be combined with Async\.start or Async\.block_on/);
  }
  console.log(`async: JavaScript bindings drive the executor${typeof WebAssembly.Suspending === "function" ? " and return promises under JSPI" : ""} passed`);
}

function runnerTests(directory) {
  const project = join(directory, "runners");
  mkdirSync(project);
  writeFileSync(join(project, "Main.tz"), `test "real timer" =
    let! time = Async.block_on (Async { do! Async.sleep 1; let! now = Async.now (); return now })
    assert (time > 0)

test "background timer" =
    do! Async.start (Async { do! Async.sleep 1 })
    do! Async.block_on (Async { do! Async.sleep 2 })

bench "async return" = \\n ->
    let! value = Async.block_on (Async.Return n)
    value
`);
  for (const optimization of [0, 3]) {
    const tested = cli(["test", project, `-O${optimization}`]);
    assert.match(tested.stdout, /2 passed/);
    cli(["bench", project, `-O${optimization}`, "--samples", "1"]);
  }
  const debug = exe(join(directory, "debug-test"));
  cli(["test", project, "--index", "0", "-g", "-o", debug]);
  execute(debug, ["0"]);
  const refused = execute(compiler, ["test", project, "--target", "wasm32"], false);
  assert.notEqual(refused.status, 0);
  assert.match(refused.stderr, /E2000.*WebAssembly test runner cannot drive Async/);
  console.log("async: native tests, debug tests, and benches at O0/O3 passed");
}

const directory = mkdtempSync(join(tmpdir(), "tsuzuri-async-"));
try {
  hostDriven(directory);
  reactor(directory);
  await jspi(directory);
  await glue(directory);
  runnerTests(directory);
} finally {
  // Windows can keep a just-run executable locked for a moment.
  try {
    rmSync(directory, { recursive: true, force: true, maxRetries: 20, retryDelay: 250 });
  } catch (error) {
    console.warn(`async: could not remove ${directory}: ${error.message}`);
  }
}
