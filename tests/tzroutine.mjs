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

  // A tzroutine cannot switch away inside Mutex.with_lock, whose lock belongs to the thread.
  const locked = project(`let lock = Mutex.create 0i64
tzroutine { () }
Mutex.with_lock (ref lock) (\\value -> {
    Routine.yield ();
    deref value
})
`);
  for (const optimization of ["-O0", "-O3"]) {
    const run = cli(["run", locked, optimization], false);
    assert.notEqual(run.status, 0);
    assert.match(run.stderr, /a tzroutine cannot yield or wait inside Mutex.with_lock/);
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
