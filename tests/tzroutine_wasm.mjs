import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

// E2E for the tzroutines of WebAssembly (TZ09): a wasm32 module of --wasm-feature jspi that spawns imports `tsuzuri_tz`,
// and the glue of --emit bindings-js runs its tzroutines on one thread, suspending a G's stack with JSPI while it waits.
// The cases of tests/fixtures/tzroutine_wasm and the native suites of the scheduler's fixtures (tzroutine,
// tzroutine_values, channel, select, timers, sync_types, context, whose expected values are those of tests/features.mjs)
// run through the glue at -O0 and -O3; tests/fixtures/tzroutine_net_wasm adds the sockets of --wasm-feature net. It needs
// JSPI (Node.js 24 or newer) and skips the runs without it, after the refusals. No result depends on how long something
// took, except that a sleep and a timeout are not early.
//   node tests/tzroutine_wasm.mjs target/debug/tsuzuri
const repository = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(process.argv[2] ?? join(repository, "target/release/tsuzuri"));
for (const name of ["TSUZURI_CLANG", "TSUZURI_WASM_LD"]) {
  if (/[\\/]/.test(process.env[name] ?? "")) process.env[name] = resolve(process.env[name]);
}
process.chdir(repository);
const root = mkdtempSync(join(tmpdir(), "tsuzuri-tzroutine-wasm-"));
const DEADLOCK = "all tzroutines are asleep - deadlock!";

function execute(args, success = true) {
  const result = spawnSync(compiler, args, { encoding: "utf8", timeout: 300_000, maxBuffer: 64 * 1024 * 1024 });
  assert.ifError(result.error);
  if (success) assert.equal(result.status, 0, `${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}
// A copy of a fixture, so that a build leaves nothing in the repository.
function fixture(name) {
  const directory = join(root, name);
  mkdirSync(directory, { recursive: true });
  cpSync(join("tests/fixtures", name, "Main.tz"), join(directory, "Main.tz"));
  return directory;
}
let projects = 0;
function project(source) {
  const directory = join(root, `p${projects++}`);
  mkdirSync(directory);
  writeFileSync(join(directory, "Main.tz"), source);
  return directory;
}
// A call that never settles fails the case instead of hanging the run.
function within(promise, what, milliseconds = 120_000) {
  let timer;
  const late = new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(`timed out waiting for ${what}`)), milliseconds); });
  return Promise.race([promise, late]).finally(() => clearTimeout(timer));
}
const jspi = ["--target", "wasm32", "--wasm-feature", "jspi"];
// Builds the module and its glue; `features` go to both, `options` to the module's build alone.
async function load(project, optimization, name, { imports = {}, features = [], options: build = [], sites = false } = {}) {
  const wasm = join(root, `${name}${optimization}.wasm`);
  const glue = join(root, `${name}${optimization}.mjs`);
  execute(["build", project, ...jspi, ...features, ...build, optimization, "-o", wasm]);
  execute(["build", project, ...jspi, ...features, "--emit", "bindings-js", "-o", glue]);
  const module = await import(pathToFileURL(glue).href);
  const options = { imports };
  if (sites) options.sites = JSON.parse(readFileSync(`${wasm}.trap.json`, "utf8")).sites;
  const bindings = await module.load(readFileSync(wasm), options);
  return { bindings, wasm, TsuzuriTrap: module.TsuzuriTrap };
}
// A result as features.mjs compares it: a bool is 0 or 1.
const plain = (value) => (typeof value === "boolean" ? Number(value) : value);
async function expectDeadlock(promise, what) {
  await assert.rejects(within(promise, what), (error) => {
    assert.ok(error instanceof WebAssembly.RuntimeError, `${what}: ${error?.stack ?? error}`);
    assert.equal(error.message, DEADLOCK, what);
    return true;
  }, what);
}
async function expectTrap(promise, TsuzuriTrap, what) {
  let caught;
  await assert.rejects(within(promise, what), (error) => {
    assert.ok(error instanceof TsuzuriTrap, `${what}: ${error?.stack ?? error}`);
    assert.equal(error.trap.reason, "trap", what);
    caught = error;
    return true;
  }, what);
  return caught;
}

// The native suites of the scheduler's fixtures (tests/features.mjs, where they are native only), with the same
// expected values: each is computed from the case, not taken from a run. `deadlocks` are the traps that are a deadlock
// natively, which the glue reports as one.
// A tzroutine that has started and not finished holds a stack of 64 KiB (D3): `yields` keeps 1000 of them at once, more
// than the default 16 MiB has room for. At -O0 the soft arithmetic of a 128-bit decimal has frames of about 60 KiB, more
// than a tzroutine's stack, so `wide_numbers` traps there as a stack overflow (`o0Traps`) instead of writing below it.
const suites = {
  tzroutine: {
    options: ["--wasm-max-memory", "128MiB"],
    cases: [
      ...[0n, 1n, 100n, 10000n].map((n) => ["sum_atomic", [n], n * (n - 1n) / 2n]),
      ...[0n, 10n].map((depth) => ["nested", [depth], 2n ** (depth + 1n) - 1n]),
      ["moves_owned", [], BigInt("tzroutine".length + 4)],
      ["drops_unused_capture", [], 1n],
      ...[1n, 64n, 1000n].map((count) => ["yields", [count], 3n * count]),
      ["go_owned", [], 11n],
      ["counts", [], 1n],
    ],
  },
  tzroutine_values: {
    cases: [
      ...[0n, 1n, 257n].map((count) => ["ordered_children", [count], 1]),
      ["nested_scopes", [7n], 64n * 7n + 63n * 64n / 2n],
      ["record_children", [], 17n],
      ["function_children", [], 42n],
      ["unit_children", [], 16n],
      ["integer_boundary", [], 1],
      ["negative_zero", [], 1],
      ["nan_result", [], 1],
    ],
    optimized: [["wide_numbers", [], 1]],
    o0Traps: [["wide_numbers", []]],
  },
  channel: {
    cases: [
      ...[0n, 1n, 10000n].map((n) => ["ping_pong", [n], n]),
      ["pipeline", [1000n], 1000n * 1001n],
      ["fan_in", [], 8n * 1000n * 1001n / 2n],
      ["fan_out", [], 1000n * 1000000n + 1000n * 1001n / 2n],
      ["owned_items", [], 53n],
      ["for_break", [], 1610n],
      ["unit_channel", [], 100n],
      ["both_kinds", [], 3n * 10n + 5n * 26n],
      ["scope_children", [], 5050n],
      ["routine_scope", [], 1275n],
      ...[0n, 1n, 1000n].map((n) => ["directions", [n], n * (n + 1n) / 2n]),
    ],
    traps: [["send_closed", []], ["close_twice", []]],
    deadlocks: [["recv_deadlock", []]],
  },
  select: {
    cases: [
      ["three_channels", [], 321n],
      ["fairness", [], 1n],
      ...[1n, 10000n].map((n) => ["send_and_recv", [n], n * (n + 1n) / 2n]),
      ["guards", [], 50n],
      ["closed_recv", [], 1n],
      ["drops_unchosen", [], 32n],
      ["same_channel_twice", [], 90n],
      ["in_loop", [], 49999n * 50000n],
      ["jump_drops", [], 3n],
      ["generic_items", [], 7n],
    ],
    traps: [["send_closed", []]],
    deadlocks: [["no_arm", []]],
  },
  timers: {
    cases: [
      ["after_is_a_timeout", [30n], 1n],
      ["sleeps_are_not_early", [], 6n],
      ["sleep_does_not_block_others", [], 1n],
      ["timer_stop_reset", [], 10010n],
      ["ticker_five", [], 5n],
      ["tick_dropped", [], 2n],
      ["after_func_runs", [], 1n],
      ["after_func_stopped", [], 1n],
      ["pending_at_exit", [], 1n],
    ],
  },
  sync_types: {
    cases: [
      ...[0n, 1n, 100n, 10000n].map((n) => ["wait_group_sum", [n], n * (n - 1n) / 2n]),
      ["wait_group_go", [1000n], 1000n * 1001n / 2n],
      ["mutex_hold_across_wait", [], 55n],
      ["mutex_different_nested", [], 8n * 100n * 3n],
      ["once_once", [], 100n + 32n],
      ["once_value", [], 1000n + 16n * 7n],
      ["rw_readers", [], 2000n * 1000n],
      ["cond_queue", [], 500n * 10n],
      ["semaphore_limit", [], 11n],
      ["sync_map", [], 2n * (799n * 800n / 2n) * 1000n + 800n],
      ["sync_arrays", [], 6n * 1000n + 2n * 100n + 6n * 10n + 1n],
    ],
    traps: [["negative_counter", []], ["mutex_same_nested", []], ["rw_nested", []], ["semaphore_over", []],
      ["semaphore_weight", []]],
    deadlocks: [["mutex_order_deadlock", []]],
  },
  context: {
    cases: [
      ["stop_a_worker", [], 3n],
      ["cancel_tree", [], 10n],
      ["timeout", [], 1n],
      ["parent_deadline_wins", [], 1n],
      ["deadline_passed", [], 11n],
      ["cancel_idempotent", [], 111n],
      ["deep_cancel", [], 10000n],
      ["after_func_on_cancel", [], 1n],
      ["errgroup_ok", [], 5050n],
      ["errgroup_first_error", [], 309n],
      ["errgroup_limit", [], 1n],
      ["errgroup_try_go", [], 7601n],
      ["release_order", [], 1n],
      ["errgroup_limit_raised", [], 9n],
      ["deadline_max", [], 1n],
    ],
    traps: [["errgroup_limit_while_running", []]],
  },
};

// deep n of the fixture: n + deep (n - 1) % 512, deep 0 = 0.
const deep = (n) => (n === 0n ? 0n : n + deep(n - 1n) % 512n);
const failures = [];
async function check(name, body) {
  const started = performance.now();
  try {
    await body();
    console.log(`tzroutine wasm: ${name} (${Math.round(performance.now() - started)} ms)`);
  } catch (error) {
    failures.push(name);
    console.error(`tzroutine wasm: FAIL ${name}\n${error?.stack ?? error}`);
  }
}

try {
  // The refusals, which need no JSPI: E2000 without jspi (build and bindings), with wasm64, threads or webgpu, and in the
  // test runner of WebAssembly, which does not drive JSPI.
  const spawner = project("export def f :: i64 -> i64\nfn f n =\n    tzroutine { () }\n    n\n");
  for (const [args, message] of [
    [["--target", "wasm32"], /tzroutine on WebAssembly needs --wasm-feature jspi/],
    [["--target", "wasm32", "--emit", "bindings-js"], /tzroutine on WebAssembly needs --wasm-feature jspi/],
    [["--target", "wasm64", "--wasm-feature", "jspi"], /tzroutine with --wasm-feature jspi requires --target wasm32/],
    [["--target", "wasm32", "--wasm-feature", "threads"], /tzroutine cannot be combined with --wasm-feature threads/],
    [["--target", "wasm32", "--wasm-feature", "threads", "--emit", "bindings-js"], /tzroutine cannot be combined with --wasm-feature threads/],
    [["--target", "wasm32", "--wasm-feature", "jspi", "--wasm-feature", "webgpu"], /tzroutine cannot be combined with --wasm-feature webgpu/],
  ]) {
    const output = join(root, args.includes("bindings-js") ? "refused.mjs" : "refused.wasm");
    const refused = execute(["build", spawner, ...args, "-o", output, "--json"], false);
    assert.notEqual(refused.status, 0, args.join(" "));
    assert.match(refused.stderr, /"code":"E2000"/, args.join(" "));
    assert.match(refused.stderr, message, args.join(" "));
  }
  const tested = project("test \"spawns\" =\n    tzroutine { () }\n    assert true\n");
  const runner = execute(["test", tested, "--target", "wasm32", "--json"], false);
  assert.notEqual(runner.status, 0);
  assert.match(runner.stderr, /"code":"E2000"/);
  assert.match(runner.stderr, /the WebAssembly test runner cannot run tzroutines/);
  // A module that spawns nothing is the default one: no import (D-18), with or without jspi.
  const single = fixture("channel_single");
  for (const extra of [[], ["--wasm-feature", "jspi"]]) {
    const wasm = join(root, "single.wasm");
    execute(["build", single, "--target", "wasm32", ...extra, "-o", wasm]);
    assert.deepEqual(WebAssembly.Module.imports(new WebAssembly.Module(readFileSync(wasm))), [], extra.join(" "));
  }
  // A module that spawns but makes no timer imports neither `clock` nor `timer`, and keeps its tsuzuri_tz_fire.
  const timerless = project("export def f :: i64 -> i64\nfn f n =\n    tzroutine { () }\n    n\n");
  for (const optimization of ["-O0", "-O3"]) {
    const wasm = join(root, `timerless${optimization}.wasm`);
    execute(["build", timerless, ...jspi, optimization, "-o", wasm]);
    const module = new WebAssembly.Module(readFileSync(wasm));
    const names = WebAssembly.Module.imports(module).map((entry) => `${entry.module}.${entry.name}`).sort();
    assert.deepEqual(names, ["tsuzuri_tz.ready", "tsuzuri_tz.spawn", "tsuzuri_tz.suspend"], optimization);
    assert.ok(WebAssembly.Module.exports(module).some((entry) => entry.name === "tsuzuri_tz_fire"), optimization);
  }
  console.log("tzroutine wasm: the refusals, the import-less default module and a timer-free module passed");

  if (typeof WebAssembly.Suspending !== "function" || typeof WebAssembly.promising !== "function") {
    console.log("tzroutine wasm: JSPI runs skipped (WebAssembly.Suspending needs Node.js 24 or newer)");
  } else {
    const own = fixture("tzroutine_wasm");
    const net = fixture("tzroutine_net_wasm");
    const others = Object.fromEntries(Object.keys(suites).map((name) => [name, fixture(name)]));
    for (const optimization of ["-O0", "-O3"]) {
      const reports = [];
      const { bindings, wasm, TsuzuriTrap } = await load(own, optimization, "own", { imports: { "Main.report": (value) => reports.push(value) } });
      const e = bindings.exports;
      await check(`imports ${optimization}`, () => {
        const module = new WebAssembly.Module(readFileSync(wasm));
        const imports = WebAssembly.Module.imports(module);
        for (const entry of imports) {
          if (entry.module === "tsuzuri_tz") assert.ok(["suspend", "spawn", "ready", "clock", "timer"].includes(entry.name), entry.name);
          else assert.deepEqual(entry, { module: "tsuzuri", name: "Main.report", kind: "function" });
        }
        for (const name of ["suspend", "spawn", "ready", "clock", "timer"]) {
          assert.ok(imports.some((entry) => entry.module === "tsuzuri_tz" && entry.name === name), name);
        }
        const exported = WebAssembly.Module.exports(module).map((entry) => entry.name);
        for (const name of ["tsuzuri_tz_entry", "tsuzuri_tz_fire", "tz_sum"]) assert.ok(exported.includes(name), name);
      });
      // The example of TZ03's quick guide, through the glue.
      await check(`sum ${optimization}`, async () => assert.equal(await within(e.sum(100n), "sum"), "sum=5050"));
      await check(`ping_pong ${optimization}`, async () => {
        for (const n of [0n, 1n, 1000n]) assert.equal(await within(e.ping_pong(n), "ping_pong"), n);
      });
      await check(`pipeline ${optimization}`, async () => {
        for (const n of [0n, 1n, 500n]) assert.equal(await within(e.pipeline(n), "pipeline"), n * (n + 1n) * (2n * n + 1n) / 6n);
      });
      await check(`select ${optimization}`, async () => {
        for (const n of [1n, 300n]) assert.equal(await within(e.select_two(n), "select_two"), 1001n * n * (n + 1n) / 2n);
      });
      await check(`wait_group_sum ${optimization}`, async () => {
        for (const n of [0n, 1n, 100n, 5000n]) assert.equal(await within(e.wait_group_sum(n), "wait_group_sum"), n * (n - 1n) / 2n);
      });
      await check(`timeout ${optimization}`, async () => {
        const started = performance.now();
        assert.equal(await within(e.timeout(30n), "timeout"), 1n);
        assert.ok(performance.now() - started >= 29, "the call took the timeout");
        assert.equal(await within(e.sleep_lets_others_run(), "sleep"), 11n);
        // A deadline that is past again as soon as it fires does not keep the glue firing (Copilot on #90).
        assert.equal(await within(e.short_ticker(100n), "short_ticker"), 100n);
      });
      await check(`deadlock ${optimization}`, async () => {
        await expectDeadlock(e.deadlock(5n), "deadlock");
        // The instance is discarded and the next call gets a new one.
        assert.equal(await within(e.ping_pong(3n), "after the deadlock"), 3n);
      });
      await check(`stack ${optimization}`, async () => {
        for (const depth of [0n, 1n, 4n]) {
          assert.equal(await within(e.neighbors(depth, 3n), "neighbors"), deep(depth) + 3n * (255n * 256n / 2n));
        }
        await expectTrap(e.neighbors(40n, 3n), TsuzuriTrap, "stack_overflow");
        assert.equal(await within(e.interleaved(40n), "interleaved"), 40n);
      });
      await check(`serialized ${optimization}`, async () => {
        reports.length = 0;
        const calls = [e.serialized(1n), e.serialized(2n), e.serialized(3n)];
        assert.deepEqual(await within(Promise.all(calls), "serialized"), [10n, 20n, 30n]);
        assert.deepEqual(reports, [1n, -1n, 2n, -2n, 3n, -3n]);
      });
      await check(`data_channel ${optimization}`, async () => {
        for (const n of [0n, 1n, 300n]) assert.equal(await within(e.data_channel_sum(n), "data_channel_sum"), n * (n + 1n) / 2n);
      });
      await check(`parallel and mutex ${optimization}`, async () => {
        assert.equal(await within(e.parallel_in_routines(), "parallel"), 4n * 500500n);
        assert.equal(await within(e.mutex_across_wait(), "mutex"), 12n);
      });
      await check(`trap ${optimization}`, async () => {
        await expectTrap(e.trap_in_routine(7n), TsuzuriTrap, "trap_in_routine");
        assert.equal(await within(e.counts(), "counts"), 31n);
      });
      // A deep recursion traps as a stack overflow, at the site that --trap-info names.
      await check(`stack_overflow site ${optimization}`, async () => {
        const traced = await load(own, optimization, "own-traps", { imports: { "Main.report": () => {} }, options: ["--trap-info"], sites: true });
        const error = await expectTrap(traced.bindings.exports.neighbors(40n, 3n), traced.TsuzuriTrap, "stack_overflow site");
        assert.equal(error.trap.kind, "stack overflow");
        assert.equal(await within(traced.bindings.exports.neighbors(4n, 1n), "after the overflow"), deep(4n) + 255n * 256n / 2n);
      });

      // The sockets of --wasm-feature net on tzroutines (the handover of TZ13): a server and its clients in one module, a
      // read that waits for a later write, a write that races the peer's close, a read across a close and a read's
      // timeout while the others run.
      await check(`net ${optimization}`, async () => {
        const loaded = await load(net, optimization, "net", { features: ["--wasm-feature", "net"] });
        const exports = loaded.bindings.exports;
        assert.equal(await within(exports.echo_server(1n), "echo_server 1"), 1n);
        assert.equal(await within(exports.echo_server(50n), "echo_server 50"), 50n);
        assert.equal(await within(exports.read_waits_for_write(), "read_waits_for_write"), 1000n + 1n + 2n + 3n + 4n);
        assert.equal(await within(exports.write_races_close(), "write_races_close"), 1n);
        assert.equal(await within(exports.close_and_timeout(), "close_and_timeout"), 11n);
        // Two calls of a module whose waits are sockets are queued too.
        assert.deepEqual(await within(Promise.all([exports.echo_server(5n), exports.echo_server(6n)]), "queued echo servers"), [5n, 6n]);
      });

      for (const [name, suite] of Object.entries(suites)) {
        await check(`${name} ${optimization}`, async () => {
          const loaded = await load(others[name], optimization, name, { options: suite.options ?? [] });
          const exports = loaded.bindings.exports;
          for (const [function_, args, expected] of [...suite.cases, ...(optimization === "-O0" ? [] : suite.optimized ?? [])]) {
            assert.deepEqual(plain(await within(exports[function_](...args), `${name}.${function_}`)), expected, `${name}.${function_}(${args.join(", ")})`);
          }
          for (const [function_, args] of suite.traps ?? []) await expectTrap(exports[function_](...args), loaded.TsuzuriTrap, `${name}.${function_}`);
          for (const [function_, args] of suite.deadlocks ?? []) await expectDeadlock(exports[function_](...args), `${name}.${function_}`);
          if (optimization === "-O0") {
            for (const [function_, args] of suite.o0Traps ?? []) await expectTrap(exports[function_](...args), loaded.TsuzuriTrap, `${name}.${function_}`);
          }
        });
      }
    }
    assert.deepEqual(failures, [], `tzroutine wasm cases that failed: ${failures.join(", ")}`);
    console.log("tzroutine wasm: tzroutines through JSPI passed at -O0 and -O3");
  }
} finally {
  rmSync(root, { recursive: true, force: true });
}
