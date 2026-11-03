// tzroutine { } end to end (TZ02): programs built and run by the compiler at -O0 and -O3 with 1 and 4
// Ps, the end-of-main wait, a trap inside a tzroutine, `tsuzuri test` and the rejected targets. The
// exports of tests/fixtures/tzroutine (with the allocation tracking) run in `features.mjs tzroutine`.
//
// usage: node tests/tzroutine.mjs [target/debug/tsuzuri]
import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(process.argv[2] ?? join(root, "target/debug/tsuzuri"));
const temporary = mkdtempSync(join(tmpdir(), "tsuzuri-tzroutine-"));

function execute(program, args, success = true, environment = {}) {
  const result = spawnSync(program, args, {
    cwd: root, encoding: "utf8", timeout: 300_000, maxBuffer: 16 * 1024 * 1024,
    env: { ...process.env, ...environment },
  });
  if (result.error) throw result.error;
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}
const cli = (args, success, environment) => execute(compiler, args, success, environment);
// Windows's console text mode writes CRLF.
const lines = (text) => text.replace(/\r\n/g, "\n");

// One project per directory: a project reads every source below its root.
let projects = 0;
function project(source) {
  const directory = join(temporary, `p${projects++}`);
  mkdirSync(directory);
  writeFileSync(join(directory, "Main.tz"), source);
  return directory;
}

const programs = [
  // The example of the ticket: the result is read after the children are counted.
  [`let total = Arc.new (Atomic.create 0i64)
let done = Arc.new (Atomic.create 0i64)
for i in 0 ..< 100 do
    let mine = Arc.share (ref total)
    let finished = Arc.share (ref done)
    tzroutine {
        let _sum = Atomic.fetch_add (Arc.get (ref mine)) (i as i64)
        let _count = Atomic.fetch_add (Arc.get (ref finished)) 1i64
    }
while Atomic.load (Arc.get (ref done)) < 100i64 do
    Routine.yield ()
$"sum={Atomic.load (Arc.get (ref total))}"
`, "sum=4950\n"],
  // main waits for its tzroutines before it prints the value of the last expression.
  [`tzroutine {
    do! IO.write_line "child"
}
42
`, "child\n42\n"],
  // A grandchild is waited for too, and a def main waits before it returns.
  [`def main :: unit -> i32 = \\() ->
    tzroutine {
        tzroutine {
            do! IO.write_line "grandchild"
        }
    }
    0
`, "grandchild\n"],
  // Routine.go runs its owned function once.
  [`Routine.go (Owned.function (\\() -> {
    do! IO.write_line "go"
}))
7
`, "go\n7\n"],
  // A tzroutine spawned by a child of Task.scope on a pool worker is waited for by that child, so
  // Task.scope returns after it.
  [`let counter = Arc.new (Atomic.create 0i64)
let _seen = Task.scope (ref counter) 4 (\\shared index -> {
    let mine = Arc.share shared;
    tzroutine {
        for _i in 0 ..< 2000 do
            Routine.yield ()
        let _n = Atomic.fetch_add (Arc.get (ref mine)) 1i64
    };
    index
})
Atomic.load (Arc.get (ref counter))
`, "4\n"],
  // The IO entry waits in tsuzuri_main.
  [`do! IO.write_line "first"
tzroutine {
    do! IO.write_line "second"
}
`, "first\nsecond\n"],
  // A tzroutine that joins Task.parallel gives its P away: with one P, the children that the
  // items spawn (one item runs on the joining thread, one on a pool worker) still run.
  [`let done = Arc.new (Atomic.create 0i64)
let outer = Arc.share (ref done)
tzroutine {
    let shared = Arc.share (ref outer)
    let jobs = new [Task<i64>](2, \\index -> task {
        let mine = Arc.share (ref shared)
        tzroutine {
            let _added = Atomic.fetch_add (Arc.get (ref mine)) 1i64
        }
        return index
    })
    let _results = Task.run (Task.parallel jobs)
    let _last = Atomic.fetch_add (Arc.get (ref outer)) 10i64
}
while Atomic.load (Arc.get (ref done)) < 12i64 do
    Routine.yield ()
$"done={Atomic.load (Arc.get (ref done))}"
`, "done=12\n"],
  // A trap in a pool item of a failure domain skips the item's scope leave; the domain restores
  // the scope, so main still waits for the tzroutine spawned after it.
  [`def explode :: i64 -> i64
fn explode n =
    let jobs = new [Task<i64>](1, \\index -> task {
        let zero = n - n
        return index / zero
    })
    let results = Task.run (Task.parallel jobs)
    results[0]

let failed = match Domain.run (\\() -> explode 3) with
    | Result.Ok _ -> 0
    | Result.Error _ -> 1
tzroutine {
    let mut spin = 0i64
    while spin < 200000i64 do
        spin = spin + 1
    do! IO.write_line "child"
}
$"failed={failed}"
`, "child\nfailed=1\n"],
  // A tzroutine back from Task.parallel gets a P even when the only one keeps running a tzroutine
  // that yields until the returning one sets a flag.
  [`let flag = Arc.new (Atomic.create 0i64)
let done = Arc.new (Atomic.create 0i64)
let setter = Arc.share (ref flag)
let first = Arc.share (ref done)
tzroutine {
    let jobs = new [Task<i64>](2, \\index -> task { return index })
    let _results = Task.run (Task.parallel jobs)
    Atomic.store (Arc.get (ref setter)) 1i64
    let _n = Atomic.fetch_add (Arc.get (ref first)) 1i64
}
let watcher = Arc.share (ref flag)
let second = Arc.share (ref done)
tzroutine {
    while Atomic.load (Arc.get (ref watcher)) == 0i64 do
        Routine.yield ()
    let _n = Atomic.fetch_add (Arc.get (ref second)) 1i64
}
while Atomic.load (Arc.get (ref done)) < 2i64 do
    Routine.yield ()
$"done={Atomic.load (Arc.get (ref done))}"
`, "done=2\n"],
  // A failure domain inside a tzroutine whose pool items trap: the blocking call that the trap left
  // is left, so the tzroutine yields and spawns again.
  [`def explode :: i64 -> i64
fn explode n =
    let jobs = new [Task<i64>](2, \\index -> task {
        let zero = n - n
        return index / zero
    })
    let results = Task.run (Task.parallel jobs)
    results[0]

let done = Arc.new (Atomic.create 0i64)
let outer = Arc.share (ref done)
tzroutine {
    let failed = match Domain.run (\\() -> explode 3) with
        | Result.Ok _ -> 0i64
        | Result.Error _ -> 1i64
    for _i in 0 ..< 10 do
        Routine.yield ()
    let mine = Arc.share (ref outer)
    tzroutine {
        let _n = Atomic.fetch_add (Arc.get (ref mine)) 10i64
    }
    let _n = Atomic.fetch_add (Arc.get (ref outer)) failed
}
while Atomic.load (Arc.get (ref done)) < 11i64 do
    Routine.yield ()
$"done={Atomic.load (Arc.get (ref done))}"
`, "done=11\n"],
  // The lock mark is per thread and a tzroutine moves between threads at its yields: optimized
  // code must not keep the mark's address across them (no false trap, every lock counted).
  [`def work :: ref Arc<Mutex<i64>> -> ref Arc<Atomic<i64>> -> unit
fn work lock done =
    for _round in 0 ..< 200 do
        Routine.yield ()
        let _value = Mutex.with_lock (Arc.get lock) (\\value -> { deref value = deref value + 1; deref value })
        Routine.yield ()
    let _finished = Atomic.fetch_add (Arc.get done) 1i64

let lock = Arc.new (Mutex.create 0i64)
let done = Arc.new (Atomic.create 0i64)
for _i in 0 ..< 16 do
    let mine = Arc.share (ref lock)
    let finished = Arc.share (ref done)
    tzroutine {
        work (ref mine) (ref finished)
    }
while Atomic.load (Arc.get (ref done)) < 16i64 do
    Routine.yield ()
let total = Mutex.with_lock (Arc.get (ref lock)) (\\value -> deref value)
$"total={total}"
`, "total=3200\n"],
  // TZ05: the ticket's example of a timeout.
  [`let never: Channel<i64> = Channel.make 0
let deadline = Time.after (50 * Time.millisecond())
let result =
    select
    | recv _ from never -> "value"
    | recv _ from deadline -> "timeout"
do! IO.write_line result
`, "timeout\n"],
  // TZ07: the ticket's example: a worker stops when its context is cancelled, and main does not wait
  // for ever at its end.
  [`let root = Context.background ()
let ctx = Context.with_cancel root
let ticks: Channel<i64> = Channel.make 0
let worker_ctx = Context.share ctx
let out = Channel.share ticks
tzroutine {
    let done = Context.done worker_ctx
    let mut n = 0i64
    let mut running = true
    while running do
        select
        | recv _ from done -> running = false
        | send n to out -> n = n + 1
}
let mut seen = 0i64
for _ in 1 ..= 3 do
    match Channel.recv ticks with
    | Maybe.Some _ -> seen = seen + 1
    | Maybe.None -> ()
Context.cancel ctx
$"stopped after {seen}"
`, "stopped after 3\n"],
  // An ErrGroup whose functions watch the group's context: the first error stops the others.
  [`let group: ErrGroup<string> = ErrGroup.create ()
for i in 1 ..= 8 do
    let ctx = ErrGroup.context group
    ErrGroup.go group (Owned.function (\\() ->
        if i == 5 then Result.Error "five failed"
        else {
            let done = Context.done ctx;
            let _closed = Channel.recv done;
            Result.Ok ()
        }))
match ErrGroup.wait group with
| Result.Ok _ -> "ok"
| Result.Error message -> message
`, "five failed\n"],
  // A context whose deadline is an hour away does not keep main waiting: the end of main stops it.
  [`let root = Context.background ()
let ctx = Context.with_timeout root Time.hour()
let _after = Context.after_func ctx (Owned.function (\\() -> ()))
Context.is_done ctx
`, "false\n"],
  // A ticker and a quit channel in one select loop; main waits for the worker's first tick and quits it.
  [`def worker :: ref Channel<unit> -> ref Channel<i64> -> ref Channel<unit> -> unit
fn worker quit report first =
    let ticker = Ticker.create (2 * Time.millisecond())
    let ticks = Ticker.channel ticker
    let mut count = 0i64
    let mut running = true
    while running do
        select
        | recv _ from ticks -> {
            count = count + 1;
            if count == 1 then Channel.send first ()
        }
        | recv _ from quit -> running = false
    Channel.send report count

def run :: unit -> string
fn run _ =
    let quit: Channel<unit> = Channel.make 0
    let reports: Channel<i64> = Channel.make 1
    let firsts: Channel<unit> = Channel.make 1
    let worker_quit = Channel.share quit
    let worker_report = Channel.share reports
    let worker_first = Channel.share firsts
    tzroutine {
        worker (ref worker_quit) (ref worker_report) (ref worker_first)
    }
    let _ticked = Channel.recv firsts
    Channel.send quit ()
    match Channel.recv reports with
    | Maybe.Some count -> if count >= 1 then "stopped after ticks" else "no tick"
    | Maybe.None -> "closed"

run ()
`, "stopped after ticks\n"],
  // The end of main stops the timers an hour away without waiting for them.
  [`def leave :: unit -> i64
fn leave _ =
    let _after = Time.after Time.hour()
    let _ticks = Time.tick Time.hour()
    let _timer = Timer.create Time.hour()
    let _func = Time.after_func Time.hour() (Owned.function (\\() -> ()))
    tzroutine {
        let _inner = Time.after Time.hour()
    }
    7

leave ()
`, "7\n"],
];

let runs = 0;
try {
  for (const [source, expected] of programs) {
    // Domain.run needs a POSIX build: Windows refuses it (E1038).
    if (process.platform === "win32" && source.includes("Domain.run")) continue;
    const directory = project(source);
    for (const optimization of ["-O0", "-O3"]) {
      for (const procs of ["1", "4"]) {
        const run = cli(["run", directory, optimization], true, { TSUZURI_MAXPROCS: procs });
        assert.equal(lines(run.stdout), expected, `${source}\n${optimization} P${procs}`);
        runs++;
      }
    }
  }

  // With trap information, the start functions of tzroutine { } and Routine.go keep the ABI that
  // the runtime calls them with.
  const informed = project(`let counter = Arc.new (Atomic.create 0i64)
let mine = Arc.share (ref counter)
tzroutine {
    let _n = Atomic.fetch_add (Arc.get (ref mine)) 1i64
}
let other = Arc.share (ref counter)
Routine.go (Owned.function (\\() -> {
    let _n = Atomic.fetch_add (Arc.get (ref other)) 1i64;
    ()
}))
while Atomic.load (Arc.get (ref counter)) < 2i64 do
    Routine.yield ()
Atomic.load (Arc.get (ref counter))
`);
  for (const optimization of ["-O0", "-O3"]) {
    const run = cli(["run", informed, "--trap-info", optimization]);
    assert.equal(lines(run.stdout), "2\n", `--trap-info ${optimization}`);
    runs++;
  }
  // TZ05: the runtime calls an after_func's function, or drops it, as `void (*)(void *)` too.
  const delayed = project(`let done: Channel<i64> = Channel.make 1
let signal = Channel.share done
let _fired = Time.after_func Time.millisecond() (Owned.function (\\() -> Channel.send signal 1))
let _dropped = Time.after_func Time.hour() (Owned.function (\\() -> ()))
match Channel.recv done with
| Maybe.Some value -> value
| Maybe.None -> 0
`);
  for (const optimization of ["-O0", "-O3"]) {
    const run = cli(["run", delayed, "--trap-info", optimization]);
    assert.equal(lines(run.stdout), "1\n", `after_func --trap-info ${optimization}`);
    runs++;
  }

  // A trap in a tzroutine ends the process with the trap's report.
  const trapping = project("tzroutine {\n    assert (1 == 2)\n}\n42\n");
  for (const optimization of ["-O0", "-O3"]) {
    const run = cli(["run", trapping, optimization], false);
    assert.notEqual(run.status, 0);
    assert.match(run.stderr, /trap: assertion failed/);
    assert.doesNotMatch(run.stdout, /42/);
    runs++;
  }

  // A frame larger than the 128 KiB guard is probed, so the overflow is reported (POSIX). Twenty
  // arrays of 8 KiB make a frame of about 330 KiB at -O3 too; three of 64 KiB took AArch64's
  // instruction selection over a minute at -O3 (DAG combining of the aggregate stores), more than
  // a CI runner gives a command.
  if (process.platform !== "win32") {
    const arrays = Array.from({ length: 20 }, (_, j) => j);
    const frames = project(`def rec deep :: i64 -> i64
fn rec deep n =
${arrays.map((j) => `    let a${j}: [i64; 1024] = FixedArray.init (\\k -> k * ${j + 1} + n)\n`).join("")}\
    if n == 0 then ${arrays.map((j) => `a${j}[${j}]`).join(" + ")} else deep (n - 1) + ${arrays.map((j) => `a${j}[(n + ${j}) % 1024]`).join(" + ")}

let done = Arc.new (Atomic.create 0i64)
let mine = Arc.share (ref done)
tzroutine {
    let _r = Atomic.fetch_add (Arc.get (ref mine)) (deep 100)
}
0
`);
    for (const optimization of ["-O0", "-O3"]) {
      const run = cli(["run", frames, optimization], false);
      assert.notEqual(run.status, 0);
      assert.match(run.stderr, /^trap: stack overflow$/m, `${optimization}\n${run.stderr}`);
      runs++;
    }
  }

  // TZ06: a tzroutine may switch away inside Mutex.with_lock: the lock is the G's (TZ02 trapped).
  const locked = project(`let lock = Mutex.create 0i64
tzroutine { () }
Mutex.with_lock (ref lock) (\\value -> {
    Routine.yield ();
    deref value
})
`);
  for (const optimization of ["-O0", "-O3"]) {
    const run = cli(["run", locked, optimization]);
    assert.equal(lines(run.stdout), "0\n");
    runs++;
  }

  // TZ06: the messages of the sync types' traps, and the deadlock of two locks taken in opposite
  // orders (the detector names the waits).
  const failing = [
    [`let group = WaitGroup.create ()\nWaitGroup.done (ref group)\n0\n`, /negative WaitGroup counter/],
    [`let _once = Once.create ()\nlet lock = Mutex.create 1i64\nlet borrowed = ref lock\nMutex.with_lock borrowed (\\_outer -> Mutex.with_lock borrowed (\\inner -> deref inner))\n`,
      /Mutex.with_lock cannot be nested/],
    [`let lock = RwMutex.create 1i64\nlet borrowed = ref lock\nRwMutex.with_read borrowed (\\_outer -> RwMutex.with_write borrowed (\\inner -> deref inner))\n`,
      /RwMutex cannot be locked again by the tzroutine that holds it/],
    [`let slots = Semaphore.create 1\nSemaphore.release (ref slots) 1\n0\n`, /Semaphore released more than it acquired/],
    [`let slots = Semaphore.create 1\nSemaphore.acquire (ref slots) 2\n0\n`, /Semaphore weight is negative or larger than its capacity/],
    [`let first = Arc.new (Mutex.create 0i64)
let second = Arc.new (Mutex.create 0i64)
let ready: Channel<unit> = Channel.make 0
let a1 = Arc.share (ref first)
let b1 = Arc.share (ref second)
let signal = Channel.share ready
tzroutine {
    let told = ref signal
    let inner = Arc.get (ref b1)
    let _sum = Mutex.with_lock (Arc.get (ref a1)) (\\_x -> {
        Channel.send told ();
        Mutex.with_lock inner (\\y -> deref y)
    })
}
let waiting = ref ready
let inner = Arc.get (ref first)
Mutex.with_lock (Arc.get (ref second)) (\\_y -> {
    let _ready = Channel.recv waiting;
    Mutex.with_lock inner (\\x -> deref x)
})
`, /all tzroutines are asleep - deadlock! .*Mutex lock/],
  ];
  for (const [source, message] of failing) {
    const directory = project(source);
    const run = cli(["run", directory, "-O0"], false);
    assert.notEqual(run.status, 0, source);
    assert.match(run.stderr, message, `${source}\n${run.stderr}`);
    runs++;
  }

  // A test waits for the tzroutines it spawned.
  const tested = project(`def bump :: unit -> i64 = \\() ->
    let shared = Arc.new (Atomic.create 0i64)
    let mine = Arc.share (ref shared)
    tzroutine {
        let _old = Atomic.fetch_add (Arc.get (ref mine)) 1i64
    }
    while Atomic.load (Arc.get (ref shared)) < 1i64 do
        Routine.yield ()
    Atomic.load (Arc.get (ref shared))

test "a tzroutine counts" =
    assert (bump () == 1i64)

bump ()
`);
  for (const optimization of ["-O0", "-O3"]) {
    const test = cli(["test", tested, optimization]);
    assert.match(test.stdout, /1 passed; 0 failed/);
    runs++;
  }

  // TZ05: Time.sleep_ms in a tzroutine waits without its thread (the hook of os.c): a watcher sees
  // the sleeper asleep, which needs the thread while it sleeps (with one P too). The sleeper starts
  // when the watcher runs, and the watcher yields until it has woken. Time.after's value is on
  // Time.monotonic_ns's clock, and not early. os.c is POSIX only.
  if (process.platform !== "win32") {
    const sleeping = project(`def run :: unit -> string
fn run _ =
    let state = Arc.new (Atomic.create 0i64)
    let ready: Channel<unit> = Channel.make 0
    let reports: Channel<string> = Channel.make 1
    let sleeper_state = Arc.share (ref state)
    let start = Channel.share ready
    tzroutine {
        let _go = Channel.recv start
        Atomic.store (Arc.get (ref sleeper_state)) 1
        let! _slept = Time.sleep_ms 100
        Atomic.store (Arc.get (ref sleeper_state)) 2
    }
    let watcher_state = Arc.share (ref state)
    let go = Channel.share ready
    let report = Channel.share reports
    tzroutine {
        Channel.send go ()
        let mut seen = false
        let mut current = Atomic.load (Arc.get (ref watcher_state))
        while current != 2 do
            if current == 1 then seen = true
            Routine.yield ()
            current = Atomic.load (Arc.get (ref watcher_state))
        Channel.send report (if seen then "worker ran" else "blocked")
    }
    match Channel.recv reports with
    | Maybe.Some who -> who
    | Maybe.None -> "none"

run ()
`);
    const valued = project(`def run :: unit -> string
fn run _ =
    let! before = Time.monotonic_ns ()
    let deadline = Time.after (10 * Time.millisecond())
    let sent = match Channel.recv deadline with
        | Maybe.Some time -> time
        | Maybe.None -> 0
    let! after = Time.monotonic_ns ()
    match (before, after) with
    | (Result.Ok first, Result.Ok last) -> if first + 10 * Time.millisecond() <= sent && sent <= last then "in order" else $"{first} {sent} {last}"
    | _ -> "no clock"

run ()
`);
    for (const optimization of ["-O0", "-O3"]) {
      for (const procs of ["1", "4"]) {
        const run = cli(["run", sleeping, optimization], true, { TSUZURI_MAXPROCS: procs });
        assert.equal(lines(run.stdout), "worker ran\n", `Time.sleep_ms ${optimization} P${procs}`);
        runs++;
      }
      const run = cli(["run", valued, optimization]);
      assert.equal(lines(run.stdout), "in order\n", `Time.after's value ${optimization}`);
      runs++;
    }
  }

  // A timer is the scheduler's: WebAssembly waits for JSPI (TZ09), and --trap-mode return refuses it
  // as it refuses a tzroutine. Time.sleep alone is Time.sleep_ms's wait there, an OS API.
  const timed = project("export def f :: i64 -> i64\nfn f n =\n    let _deadline = Time.after Time.second()\n    n\n");
  const timedWasm = cli(["build", timed, "--target", "wasm32", "-o", join(temporary, "timed.wasm"), "--json"], false);
  assert.match(timedWasm.stderr, /"code":"E2000"/);
  assert.match(timedWasm.stderr, /tzroutine on WebAssembly needs --wasm-feature jspi/);
  const timedReturn = cli(["build", timed, "--emit", "shared", "--trap-mode", "return", "-o", join(temporary, "timed.so"), "--json"], false);
  assert.match(timedReturn.stderr, /"code":"E2000"/);
  const slept = project("export def f :: i64 -> i64\nfn f n =\n    Time.sleep Time.millisecond()\n    n\n");
  const sleptWasm = cli(["build", slept, "--target", "wasm32", "-o", join(temporary, "slept.wasm"), "--json"], false);
  assert.match(sleptWasm.stderr, /operating-system APIs|--wasm-host/);
  runs += 3;

  // TZ07: a Context is the scheduler's too (its deadline is a timer).
  const contexted = project("export def f :: i64 -> i64\nfn f n =\n    let root = Context.background ()\n    if Context.is_done root then 0 else n\n");
  const contextWasm = cli(["build", contexted, "--target", "wasm32", "-o", join(temporary, "context.wasm"), "--json"], false);
  assert.match(contextWasm.stderr, /"code":"E2000"/);
  const contextReturn = cli(["build", contexted, "--emit", "shared", "--trap-mode", "return", "-o", join(temporary, "context.so"), "--json"], false);
  assert.match(contextReturn.stderr, /"code":"E2000"/);
  runs += 2;

  // TZ11: inside Domain.run, Parallel stays on the pool, which runs each chunk under the boundary:
  // the sum is the same, and a trap in a chunk is the domain's Result.Error.
  const domained = project(`tzroutine { () }
let sum = match Domain.run (\\() -> Parallel.sum (ref (Parallel.init 10000 (\\index -> index)))) with
    | Result.Ok total -> total
    | Result.Error _ -> -1
let trapped = match Domain.run (\\() -> (Parallel.init 8192 (\\index -> { assert (index != 4096); index })).length) with
    | Result.Ok _ -> "no trap"
    | Result.Error _ -> "trapped"
$"{sum} {trapped}"
`);
  for (const optimization of ["-O0", "-O3"]) {
    for (const procs of ["1", "4"]) {
      const run = cli(["run", domained, optimization], true, { TSUZURI_MAXPROCS: procs });
      assert.equal(lines(run.stdout), "49995000 trapped\n", `Parallel in Domain.run ${optimization} P${procs}`);
      runs++;
    }
  }

  // TZ11: Routine.scope spawns a tzroutine for each child: E2000 on WebAssembly too.
  const scoped = project("export def f :: i64 -> i64\nfn f n =\n    let values = [n, n]\n    let doubled = Routine.scope (ref values) 2 (\\shared index -> shared[index] * 2)\n    doubled[0]\n");
  const scopedWasm = cli(["build", scoped, "--target", "wasm32", "-o", join(temporary, "scoped.wasm"), "--json"], false);
  assert.match(scopedWasm.stderr, /"code":"E2000"/);
  runs++;

  // WebAssembly waits for JSPI (TZ09); the error is E2000 before any build.
  const exported = project("export def f :: i64 -> i64\nfn f n =\n    tzroutine { () }\n    n\n");
  const wasm = cli(["build", exported, "--target", "wasm32", "-o", join(temporary, "out.wasm"), "--json"], false);
  assert.match(wasm.stderr, /"code":"E2000"/);
  assert.match(wasm.stderr, /tzroutine on WebAssembly needs --wasm-feature jspi/);
  runs++;

  console.log(`tzroutine: ${runs} runs passed`);
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
