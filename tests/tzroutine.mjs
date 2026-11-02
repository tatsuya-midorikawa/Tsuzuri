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
  // The IO entry waits in tsuzuri_main.
  [`do! IO.write_line "first"
tzroutine {
    do! IO.write_line "second"
}
`, "first\nsecond\n"],
];

let runs = 0;
try {
  for (const [source, expected] of programs) {
    const directory = project(source);
    for (const optimization of ["-O0", "-O3"]) {
      for (const procs of ["1", "4"]) {
        const run = cli(["run", directory, optimization], true, { TSUZURI_MAXPROCS: procs });
        assert.equal(run.stdout, expected, `${source}\n${optimization} P${procs}`);
        runs++;
      }
    }
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

  // A frame larger than the 128 KiB guard is probed, so the overflow is reported (POSIX).
  if (process.platform !== "win32") {
    const frames = project(`def rec deep :: i64 -> i64
fn rec deep n =
    let a: [[i64; 1024]; 8] = FixedArray.init (\\i -> FixedArray.init (\\k -> k + i + n))
    let b: [[i64; 1024]; 8] = FixedArray.init (\\i -> FixedArray.init (\\k -> k * i + n))
    let c: [[i64; 1024]; 8] = FixedArray.init (\\i -> FixedArray.init (\\k -> k - i + n))
    if n == 0 then a[1][2] + b[3][4] + c[5][6] else deep (n - 1) + a[7][n % 1024] + b[0][1] + c[2][3]

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
