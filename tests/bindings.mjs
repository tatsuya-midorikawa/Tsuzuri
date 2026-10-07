// End-to-end test of `tsuzuri build --target wasm32 --emit bindings-js` (E13): the generated glue
// loads the fixture's .wasm at -O0 and -O3, converts every ABI type, imports and callbacks, keeps
// the heap balanced, classifies traps and rebuilds the instance, and its .d.mts type-checks.
// usage: node tests/bindings.mjs target/release/tsuzuri
// TSUZURI_TSC may name TypeScript's bin/tsc; the default is vsc/node_modules/typescript/bin/tsc.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { copyFileSync, cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const repository = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const root = mkdtempSync(join(tmpdir(), "tsuzuri-bindings-"));
const fixture = join(root, "fixture");
cpSync(join(repository, "tests/fixtures/bindings"), fixture, { recursive: true });

function run(args, status = 0) {
  const result = spawnSync(compiler, args, { encoding: "utf8", timeout: 180_000, maxBuffer: 16 * 1024 * 1024 });
  assert.ifError(result.error);
  if (status !== null) assert.equal(result.status, status, `${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}
function rejected(args, code, status, message) {
  const result = run(args, status);
  assert.match(result.stderr, new RegExp(`error\\[${code}\\]`), `${args.join(" ")}\n${result.stderr}`);
  if (message) assert.ok(result.stderr.includes(message), `${args.join(" ")}\n${result.stderr}`);
}
function wasm(name, ...flags) {
  const output = join(root, `${name}.wasm`);
  run(["build", fixture, "--target", "wasm32", ...flags, "-o", output]);
  return output;
}

// The independent references: d(0) = 0, d(n) = 3 d(n - 1) + n wraps like i64.
const depth = (n) => { let value = 0n; for (let k = 1n; k <= n; k++) value = BigInt.asIntN(64, 3n * value + k); return value; };
const units = (text) => Array.from({ length: text.length }, (_, index) => text.charCodeAt(index));
const source = readFileSync(join(fixture, "Main.tz"), "utf8");
const divideLine = source.split("\n").findIndex((line) => line.startsWith("fn divide a b ="));
const divideColumn = source.split("\n")[divideLine].indexOf("a / b") + 1;
const expectedImports = [
  "env.e13_apply", "env.e13_visit", "tsuzuri.Main.host_fail", "tsuzuri.Main.host_flags", "tsuzuri.Main.host_greeting",
  "tsuzuri.Main.host_mix", "tsuzuri.Main.host_note", "tsuzuri.Main.host_scale", "tsuzuri.Main.now",
  "tsuzuri.e13_counter_add", "tsuzuri.e13_counter_free", "tsuzuri.e13_counter_new",
];

// Records the instances the glue creates, so the counting allocator of a build can be read.
const OriginalInstance = WebAssembly.Instance;
const originalInstantiate = WebAssembly.instantiate;
let latest;
WebAssembly.Instance = class extends OriginalInstance {
  constructor(module, imports) {
    super(module, imports);
    latest = this;
  }
};
WebAssembly.instantiate = async (...args) => {
  const result = await originalInstantiate(...args);
  latest = result.instance ?? result;
  return result;
};
// Requested bytes the module holds, without the 32 bytes of the statistics record itself.
function liveBytes() {
  const exports = latest.exports;
  const pointer = exports.tsuzuri_alloc(32n) >>> 0;
  exports.tsuzuri_alloc_stats(pointer);
  const live = new DataView(exports.memory.buffer).getBigUint64(pointer + 16, true);
  exports.tsuzuri_free(pointer);
  return live - 32n;
}

// Host state that the cases steer.
const handles = new Map();
let nextHandle = 1;
let applyMode = "twice";
let savedCallback;
let swallowed;
let scaleResult;
let scaleSeen;
let notes = [];
const marker = new Error("marker");
const hostImports = {
  "Main.now": () => 37n,
  "Main.host_scale": (values, factor) => {
    scaleSeen = values;
    const result = values.map((value) => value * factor);
    values.fill(-1);
    return scaleResult ?? result;
  },
  "Main.host_greeting": (name) => `hello ${name}`,
  "Main.host_fail": () => { throw marker; },
  "Main.host_note": (text) => { notes.push(text); },
  "Main.host_flags": (flags) => {
    assert.deepEqual(flags, { tiny: -5, wide: 7, flag: true });
    return { flags: { tiny: flags.tiny - 1, wide: 9, flag: false }, amount: 2.5 };
  },
  "Main.host_mix": (small, large, flag, narrow) => {
    assert.deepEqual([small, large, flag, narrow], [4294967295, 2n ** 64n - 2n, true, -2]);
    return large;
  },
  e13_counter_new: (start) => { const handle = nextHandle++; handles.set(handle, start); return handle; },
  e13_counter_add: (handle, amount) => { const value = handles.get(handle) + amount; handles.set(handle, value); return value; },
  e13_counter_free: (handle) => { const value = handles.get(handle); assert.ok(handles.delete(handle)); return value; },
  e13_apply: (callback, value) => {
    if (applyMode === "keep") savedCallback = callback;
    if (applyMode === "swallow") {
      try { callback(value); } catch (error) { swallowed = error; }
      return 0n;
    }
    return callback(callback(value));
  },
  e13_visit: (callback, handle) => (callback(handle, 10) ? 1n : 0n),
};

let TsuzuriTrap;
let loadBindings;
// Each case runs against fresh bindings of one build; `trap` cases discard the instance on purpose.
const cases = [
  ["1 scalars", (api) => assert.equal(api.exports.add(40n, 2n), 42n)],
  ["2 argument errors keep the instance", (api) => {
    api.withBorrowed("f64", 2, (borrowed) => {
      borrowed.view().set([1.5, 2.5]);
      assert.throws(() => api.exports.add(1, 2n), { name: "TypeError", message: "argument 0 of 'add' must be a bigint" });
      assert.throws(() => api.exports.add(2n ** 63n, 0n), { name: "RangeError", message: "argument 0 of 'add' is out of range for i64" });
      assert.throws(() => api.exports.add(1n), { name: "TypeError", message: "'add' expects 2 arguments" });
      assert.equal(api.exports.sum_float(borrowed), 4);
    });
  }],
  ["3 i64u", (api) => {
    assert.equal(api.exports.add_u64(2n ** 64n - 1n, 0n), 18446744073709551615n);
    assert.equal(api.exports.add_u64(2n ** 63n, 2n ** 63n - 1n), 2n ** 64n - 1n);
    assert.throws(() => api.exports.add_u64(-1n, 0n), RangeError);
  }],
  ["4 narrow integers", (api) => {
    assert.equal(api.exports.widen(-128, 65535, 4294967295), -128n + 65535n + 4294967295n);
    assert.throws(() => api.exports.widen(128, 0, 0), { name: "RangeError", message: "argument 0 of 'widen' is out of range for i8" });
    assert.throws(() => api.exports.widen(0, 65536, 0), RangeError);
    assert.throws(() => api.exports.widen(0, 0, -1), RangeError);
    assert.throws(() => api.exports.widen(0.5, 0, 0), TypeError);
  }],
  ["5 bool", (api) => {
    assert.equal(api.exports.negate(true), false);
    assert.equal(api.exports.negate(false), true);
    assert.throws(() => api.exports.negate(1), { name: "TypeError", message: "argument 0 of 'negate' must be a boolean" });
  }],
  ["6 f32", (api) => assert.equal(api.exports.twice32(0.1), Math.fround(Math.fround(0.1) * 2))],
  ["7 owned i64 buffers", (api) => {
    const input = BigInt64Array.of(1n, -2n, 2n ** 63n - 1n);
    const copy = api.exports.copy_values(input);
    assert.ok(copy instanceof BigInt64Array && copy !== input);
    assert.deepEqual([...copy], [1n, -2n, 2n ** 63n - 1n]);
    assert.equal(api.exports.copy_values(new BigInt64Array(0)).length, 0);
    assert.throws(() => api.exports.copy_values([1n]), TypeError);
  }],
  ["8 f64 slices", (api) => assert.equal(api.exports.sum_float(Float64Array.of(0.5, 0.25)), 0.75)],
  ["9 byte slices", (api) => {
    assert.equal(api.exports.checksum(Uint8Array.of(1, 2, 255)), 258n);
    assert.throws(() => api.exports.checksum(Int8Array.of(1)), TypeError);
  }],
  ["10 owned byte buffers", (api) => {
    assert.deepEqual([...api.exports.make_bytes(4n)], [0, 1, 2, 3]);
    assert.equal(api.exports.make_bytes(0n).length, 0);
  }],
  ["11 UTF-16 strings keep lone surrogates", (api) => {
    const text = "a\ud800b😀\udc00";
    assert.deepEqual(units(api.exports.copy_text(text)), units(text));
    assert.equal(api.exports.copy_text(""), "");
  }],
  ["12 UTF-8 strings", (api) => {
    assert.equal(api.exports.copy_utf8("é😀"), "é😀");
    assert.throws(() => api.exports.copy_utf8("\ud800"), { name: "TypeError", message: "argument 0 of 'copy_utf8' must be a well-formed string" });
  }],
  ["13 records", (api) => {
    assert.deepEqual(api.exports.update({ flags: { tiny: -1, wide: 65535, flag: true }, amount: 1.5 }), { flags: { tiny: -1, wide: 65535, flag: true }, amount: 2.5 });
    assert.throws(() => api.exports.update({ flags: { tiny: -1, wide: 65535, flag: true } }), { name: "TypeError", message: "argument 0 of 'update' field 'amount' must be a number" });
    assert.throws(() => api.exports.update({ flags: { tiny: -129, wide: 0, flag: true }, amount: 0 }), { name: "RangeError", message: "argument 0 of 'update' field 'flags.tiny' is out of range for i8" });
  }],
  ["14 records of another module", (api) => assert.equal(api.exports.area({ x: 2, y: 3.5 }), 7)],
  ["15 imports", (api) => assert.equal(api.exports.stamp(5n), 42n)],
  ["16 borrowed import arguments are copies", (api) => {
    const input = Float64Array.of(1, 2);
    assert.deepEqual([...api.exports.scaled(input, 3)], [3, 6]);
    assert.deepEqual([...input], [1, 2]);
    assert.deepEqual([...scaleSeen], [-1, -1]);
    assert.deepEqual([...api.exports.scaled(input, 0.5)], [0.5, 1]);
  }],
  ["17 UTF-8 import results", (api) => assert.equal(api.exports.greeting("世界"), "hello 世界")],
  ["18 host errors pass through", (api) => {
    assert.throws(() => api.exports.relay_fail(1n), (error) => error === marker);
    assert.equal(api.exports.add(1n, 1n), 2n);
  }, { trap: true }],
  ["19 traps", (api, build) => {
    let trap;
    assert.throws(() => api.exports.divide(1n, 0n), (error) => { trap = error; return error instanceof TsuzuriTrap; });
    assert.equal(trap.name, "TsuzuriTrap");
    assert.ok(trap.cause instanceof WebAssembly.RuntimeError);
    if (!build.sites) {
      assert.deepEqual(trap.trap, { reason: "trap", site: 0 });
      assert.equal(trap.message, "trap (site 0)");
    } else {
      assert.ok(trap.trap.site > 0);
      assert.equal(trap.trap.kind, "integer division by zero");
      assert.ok(trap.trap.path.endsWith("Main.tz"));
      assert.equal(trap.trap.line, divideLine + 1);
      assert.equal(trap.trap.column, divideColumn);
      assert.equal(trap.message, `trap at ${trap.trap.path}:${divideLine + 1}:${divideColumn} (integer division by zero)`);
    }
  }, { trap: true }],
  ["20 a new instance after a trap", (api) => {
    assert.throws(() => api.exports.divide(1n, 0n), TsuzuriTrap);
    assert.equal(api.exports.divide(-7n, 2n), -7n / 2n);
  }, { trap: true }],
  ["21 stack exhaustion", (api) => {
    assert.throws(() => api.exports.deep(100000000n), (error) => error instanceof TsuzuriTrap && error.message === "stack exhausted" && error.trap.reason === "stack" && error.trap.site === 0);
    assert.equal(api.exports.deep(10n), depth(10n));
    assert.equal(depth(10n), 44281n);
  }, { trap: true }],
  ["22 withBorrowed", (api) => {
    let kept;
    assert.equal(api.withBorrowed("f64", 3, (borrowed) => {
      kept = borrowed;
      assert.equal(borrowed.length, 3);
      borrowed.view().set([1, 2, 3]);
      return api.exports.sum_float(borrowed);
    }), 6);
    assert.throws(() => kept.view(), TypeError);
    assert.throws(() => api.exports.sum_float(kept), TypeError);
    assert.throws(() => api.withBorrowed("f64", 1, () => Promise.resolve(1)), TypeError);
    assert.throws(() => api.withBorrowed("i64", 1, (borrowed) => api.exports.sum_float(borrowed)), TypeError);
    assert.equal(api.withBorrowed("i64", 0, (borrowed) => api.exports.copy_values(borrowed).length), 0);
    assert.equal(api.withBorrowed("ubyte", 2, (borrowed) => { borrowed.view().set([7, 9]); return api.exports.checksum(borrowed); }), 16n);
    assert.throws(() => api.withBorrowed("i32", 1, () => 0), TypeError);
    assert.throws(() => api.withBorrowed("i64", -1, () => 0), RangeError);
  }],
  ["23 large strings do not leak", (api) => {
    const text = "x".repeat(1 << 20);
    for (let index = 0; index < 64; index++) assert.equal(api.exports.copy_text(text).length, 1 << 20);
  }],
  ["27 handles", (api) => {
    assert.equal(api.exports.counters(10n), 30n);
    assert.equal(handles.size, 0);
    assert.equal(api.exports.pass_through(4294967295), 4294967295);
    assert.throws(() => api.exports.pass_through(-1), RangeError);
    assert.throws(() => api.exports.pass_through(1.5), TypeError);
    const handle = hostImports.e13_counter_new(41n);
    assert.equal(api.exports.peek(handle), 41n);
    hostImports.e13_counter_free(handle);
  }],
  ["28 callbacks", (api) => {
    assert.equal(api.exports.callbacks(4n), 36n);
    assert.equal(api.exports.visit(11n), 12n);
    assert.equal(api.exports.visit(3n), 3n);
    applyMode = "keep";
    try {
      assert.equal(api.exports.callbacks(1n), 9n);
    } finally {
      applyMode = "twice";
    }
    assert.throws(() => savedCallback(1n), { name: "TypeError", message: "the callback of import 'e13_apply' is only valid during that import call" });
  }],
  ["29 a trap in a callback", (api) => {
    assert.throws(() => api.exports.callback_trap(1n), (error) => error instanceof TsuzuriTrap && error.trap.reason === "trap");
    assert.equal(api.exports.add(1n, 1n), 2n);
    applyMode = "swallow";
    try {
      let thrown;
      assert.throws(() => api.exports.callback_trap(1n), (error) => { thrown = error; return error instanceof TsuzuriTrap; });
      assert.equal(thrown, swallowed);
    } finally {
      applyMode = "twice";
    }
    assert.equal(api.exports.callbacks(2n), 18n);
  }, { trap: true }],
  ["30 string and record imports", (api) => {
    notes = [];
    assert.equal(api.exports.note("a\ud800"), 2n);
    assert.deepEqual(notes.map(units), [[97, 0xd800]]);
    assert.equal(api.exports.relay_flags(-5), 2.5 + 9 - 6);
    assert.equal(api.exports.mix(4294967295, 2n ** 64n - 2n), 2n ** 64n - 1n);
  }],
  ["31 fixed arrays in records", (api) => {
    assert.equal(api.exports.window_total({ values: [1, 2, 3], scale: 2, mark: 7 }), 19);
    assert.deepEqual(api.exports.make_window(5), { values: [5, 10, -5], scale: 0.5, mark: 255 });
    assert.throws(() => api.exports.window_total({ values: [1, 2], scale: 2, mark: 7 }), { name: "TypeError", message: "argument 0 of 'window_total' field 'values' must be an array of 3 elements" });
    assert.throws(() => api.exports.window_total({ values: [1, 2, 2 ** 31], scale: 2, mark: 7 }), { name: "RangeError", message: "argument 0 of 'window_total' field 'values[2]' is out of range for i32" });
  }],
  ["32 checked import results", (api) => {
    scaleResult = [1, 2];
    try {
      assert.throws(() => api.exports.scaled(Float64Array.of(1), 1), { name: "TypeError", message: "result of import 'Main.host_scale' must be a Float64Array" });
    } finally {
      scaleResult = undefined;
    }
    assert.deepEqual([...api.exports.scaled(Float64Array.of(2), 2)], [4]);
  }, { trap: true }],
  ["33 asynchronous recreation", async (api) => {
    await api.ready();
    assert.throws(() => api.exports.divide(1n, 0n), TsuzuriTrap);
    // A browser's main thread refuses to create instances of modules over 8 MB synchronously.
    const original = WebAssembly.Instance;
    let refused = 0;
    WebAssembly.Instance = function () {
      refused++;
      throw new RangeError("WebAssembly.Instance is disallowed on the main thread, if the buffer size is larger than 8MB. Use WebAssembly.instantiate.");
    };
    try {
      assert.throws(() => api.exports.divide(7n, 2n), (error) => error.message === "the WASM instance could not be recreated synchronously after a failure; await ready() to recreate it asynchronously, then call again" && error.cause instanceof RangeError);
      await Promise.all([api.ready(), api.ready()]);
      assert.equal(api.exports.divide(7n, 2n), 3n);
      assert.equal(api.exports.add(20n, 22n), 42n);
      assert.equal(refused, 1);
    } finally {
      WebAssembly.Instance = original;
    }
  }, { trap: true }],
  ["34 a Borrowed buffer belongs to its load", async (api, build) => {
    const other = await loadBindings(readFileSync(build.path), { imports: hostImports });
    api.withBorrowed("f64", 2, (borrowed) => {
      borrowed.view().set([1.5, 2]);
      // The other instance would read its own memory at this address.
      assert.throws(() => other.exports.sum_float(borrowed), { name: "TypeError", message: "argument 0 of 'sum_float' must be a Borrowed<Float64Array> from the same load()" });
      assert.equal(api.exports.sum_float(borrowed), 3.5);
    });
    other.withBorrowed("f64", 1, (borrowed) => {
      borrowed.view()[0] = 4;
      assert.throws(() => api.exports.sum_float(borrowed), { name: "TypeError", message: "argument 0 of 'sum_float' must be a Borrowed<Float64Array> from the same load()" });
      assert.equal(other.exports.sum_float(borrowed), 4);
    });
    assert.equal(other.exports.sum_float(Float64Array.of(1, 2)), 3);
  }],
  ["35 a leading U+FEFF stays", (api) => {
    // UTF-8 results keep a leading byte order mark, and so do UTF-8 arguments of imports.
    assert.equal(api.exports.copy_utf8("\uFEFFabc"), "\uFEFFabc");
    assert.equal(api.exports.copy_utf8("\uFEFF"), "\uFEFF");
    assert.equal(api.exports.greeting("\uFEFFworld"), "hello \uFEFFworld");
  }],
];

try {
  // The glue is generated once, deterministically, from the sources alone.
  const glue = join(root, "glue");
  mkdirSync(glue);
  run(["build", fixture, "--target", "wasm32", "--emit", "bindings-js", "-O3", "-o", join(glue, "bindings.mjs")]);
  run(["build", fixture, "--target", "wasm32", "--emit", "bindings-js", "-o", join(root, "again.mjs")]);
  assert.deepEqual(readFileSync(join(glue, "bindings.mjs")), readFileSync(join(root, "again.mjs")));
  assert.deepEqual(readFileSync(join(glue, "bindings.d.mts")), readFileSync(join(root, "again.d.mts")));
  const check = spawnSync(process.execPath, ["--check", join(glue, "bindings.mjs")], { encoding: "utf8" });
  assert.equal(check.status, 0, check.stderr);
  const { load, TsuzuriTrap: Trap } = await import(pathToFileURL(join(glue, "bindings.mjs")).href);
  TsuzuriTrap = Trap;
  loadBindings = load;

  for (const optimization of ["-O0", "-O3"]) {
    const builds = [
      { path: wasm(`plain${optimization}`, optimization) },
      { path: wasm(`sites${optimization}`, optimization, "--trap-info"), sites: true },
      { path: wasm(`counting${optimization}`, optimization, "--allocator", "counting"), counting: true },
    ];
    for (const build of builds) {
      const bytes = readFileSync(build.path);
      const module = new WebAssembly.Module(bytes);
      assert.deepEqual(WebAssembly.Module.imports(module).map(({ module, name, kind }) => { assert.equal(kind, "function"); return `${module}.${name}`; }).sort(), expectedImports);
      const options = { imports: hostImports };
      if (build.sites) options.sites = JSON.parse(readFileSync(`${build.path}.trap.json`, "utf8")).sites;
      for (const [name, body, flags = {}] of cases) {
        // Every case starts from bindings of its own, from bytes or a compiled module.
        const api = await load(name.startsWith("1") ? module : bytes, options);
        assert.ok(Object.isFrozen(api) && Object.isFrozen(api.exports));
        try {
          await body(api, build);
        } catch (error) {
          error.message = `${optimization} ${build.path}: case ${name}: ${error.message}`;
          throw error;
        }
        if (build.counting && !flags.trap) assert.equal(liveBytes(), 0n, `${optimization} case ${name} leaks`);
      }
    }
    // A precompiled module and the trap sites of its side table.
    const sited = builds[1];
    const api = await load(new WebAssembly.Module(readFileSync(sited.path)), { imports: hostImports, sites: JSON.parse(readFileSync(`${sited.path}.trap.json`, "utf8")).sites });
    assert.throws(() => api.exports.divide(1n, 0n), (error) => error.trap.kind === "integer division by zero");
    console.log(`bindings: ${optimization} passed (${cases.length} cases on ${builds.length} builds)`);
  }

  // 24-26 and the other load-time checks.
  const threads = join(root, "threads.wasm");
  run(["build", join(repository, "tests/fixtures/tasks"), "--target", "wasm32", "--wasm-feature", "threads", "-o", threads]);
  await assert.rejects(load(readFileSync(threads), { imports: hostImports }), { name: "Error", message: /^bindings do not support modules built with --wasm-feature threads/ });
  const plain = readFileSync(join(root, "plain-O3.wasm"));
  await assert.rejects(load(plain), { name: "TypeError", message: "missing import 'Main.host_fail'" });
  await assert.rejects(load(plain, { imports: { ...hostImports, "Main.now": 37n } }), { name: "TypeError", message: "missing import 'Main.now'" });
  const hostAbi = join(root, "host_abi.wasm");
  run(["build", join(repository, "tests/fixtures/host_abi"), "--target", "wasm32", "-o", hostAbi]);
  await assert.rejects(load(readFileSync(hostAbi), { imports: hostImports }), { name: "Error", message: "module does not match bindings: missing export 'tz_add'; regenerate the bindings from the same sources" });
  const imports = join(root, "imports.wasm");
  run(["build", join(repository, "tests/fixtures/host_imports"), "--target", "wasm32", "-o", imports]);
  await assert.rejects(load(readFileSync(imports), { imports: hostImports }), { message: /^module does not match bindings: unexpected import 'tsuzuri\.Main\./ });
  const debug = join(root, "debug");
  mkdirSync(debug);
  writeFileSync(join(debug, "Main.tz"), "export def add :: i64 -> i64 -> i64\nfn add a b = Debug.trace (a + b)\n");
  run(["build", debug, "--target", "wasm32", "--debug-output", "-o", join(debug, "debug.wasm")]);
  await assert.rejects(load(readFileSync(join(debug, "debug.wasm"))), { message: "bindings do not support imports from 'tsuzuri_debug'; build a library without IO main or --debug-output" });
  // Any explicit import module name, `__proto__` included, stays an ordinary property of the
  // import object, and a host import is only the caller's own property (PR #17 review).
  const names = join(root, "names");
  mkdirSync(names);
  writeFileSync(join(names, "Main.tz"), 'extern "__proto__" "answer" def answer :: unit -> i64\nextern "env" "toString" def text :: unit -> i64\n\nexport def ask :: i64\nfn ask = answer () + text ()\n');
  run(["build", names, "--target", "wasm32", "--emit", "bindings-js", "-o", join(names, "names.mjs")]);
  run(["build", names, "--target", "wasm32", "-o", join(names, "names.wasm")]);
  const namesBindings = await import(pathToFileURL(join(names, "names.mjs")).href);
  const namesModule = new WebAssembly.Module(readFileSync(join(names, "names.wasm")));
  await assert.rejects(namesBindings.load(namesModule, { imports: { answer: () => 40n } }), { name: "TypeError", message: "missing import 'toString'" });
  await assert.rejects(namesBindings.load(namesModule, { imports: Object.create({ answer: () => 40n, toString: () => 2n }) }), { name: "TypeError", message: "missing import 'answer'" });
  const namesApi = await namesBindings.load(namesModule, { imports: { answer: () => 40n, toString: () => 2n } });
  assert.equal(namesApi.exports.ask(), 42n);
  assert.ok(!Object.hasOwn(Object.prototype, "answer"));
  assert.equal(({}).answer, undefined);
  const wide = join(root, "wide.wasm");
  run(["build", fixture, "--target", "wasm64", "-o", wide]);
  await assert.rejects(load(readFileSync(wide), { imports: hostImports }), { message: "bindings support wasm32 modules only; build the .wasm with --target wasm32" });
  await assert.rejects(load("not a module"), TypeError);

  const point = join(root, "point");
  mkdirSync(point);
  run(["build", join(repository, "examples/point/Point.tz"), "--target", "wasm32", "--emit", "bindings-js", "-o", join(point, "point.mjs")]);
  run(["build", join(repository, "examples/point/Point.tz"), "--target", "wasm32", "-o", join(point, "point.wasm")]);
  const pointModule = new WebAssembly.Module(readFileSync(join(point, "point.wasm")));
  assert.deepEqual(WebAssembly.Module.imports(pointModule), []);
  // A scalar-only module exports no allocator, and the glue asks for none.
  assert.ok(!WebAssembly.Module.exports(pointModule).some((entry) => entry.name === "tsuzuri_alloc"));
  const pointBindings = await import(pathToFileURL(join(point, "point.mjs")).href);
  const pointApi = await pointBindings.load(pointModule);
  assert.equal(pointApi.exports.hypotenuse(3, 4), 5);
  assert.throws(() => pointApi.withBorrowed("f64", 1, () => 0), TypeError);
  assert.match(readFileSync(join(point, "point.d.mts"), "utf8"), /options\?: \{ sites\?: readonly TrapSite\[\] \}/);
  console.log("bindings: load checks passed");

  // TypeScript 6.0.3 type-checks the declarations with the consumer and rejects its mistakes.
  const tsc = process.env.TSUZURI_TSC ?? join(repository, "vsc/node_modules/typescript/bin/tsc");
  if (existsSync(tsc)) {
    copyFileSync(join(repository, "tests/bindings_consumer.mts"), join(glue, "consumer.mts"));
    const typed = spawnSync(process.execPath, [tsc, "--noEmit", "--strict", "--module", "nodenext", "--moduleResolution", "nodenext", "--target", "es2022", join(glue, "consumer.mts")], { encoding: "utf8", cwd: glue });
    assert.equal(typed.status, 0, `${typed.stdout}\n${typed.stderr}`);
    const version = spawnSync(process.execPath, [tsc, "--version"], { encoding: "utf8" }).stdout.trim();
    // Without the declarations next to the module, the same import fails: the .d.mts is what is checked.
    rmSync(join(glue, "bindings.d.mts"));
    const untyped = spawnSync(process.execPath, [tsc, "--noEmit", "--strict", "--module", "nodenext", "--moduleResolution", "nodenext", "--target", "es2022", join(glue, "consumer.mts")], { encoding: "utf8", cwd: glue });
    assert.notEqual(untyped.status, 0);
    console.log(`bindings: tsc passed (${version})`);
  } else {
    console.log("bindings: tsc skipped (run npm ci --prefix vsc or set TSUZURI_TSC)");
  }

  // The command line: invalid configurations exit with 2, build errors with 1, and nothing is written.
  const out = join(root, "rejected.mjs");
  rejected(["build", fixture, "--emit", "bindings-ts", "-o", out], "E2000", 2, "emit kind must be exe, object, llvm, header, wasm, wgsl, shared, bindings-js, bindings-cs, bindings-py, or bindings-cpp");
  rejected(["build", fixture, "--emit", "bindings-js", "-o", out], "E2000", 2, "'--emit bindings-js' requires '--target wasm32'");
  rejected(["build", fixture, "--target", "wasm64", "--emit", "bindings-js", "-o", out], "E2000", 2, "'--emit bindings-js' requires '--target wasm32'");
  for (const option of ["--trap-info", "--debug-info", "--debug-output"]) {
    rejected(["build", fixture, "--target", "wasm32", "--emit", "bindings-js", option, "-o", out], "E2000", 2, `${option} is not valid for bindings output; pass it when building the .wasm`);
  }
  rejected(["build", fixture, "--target", "wasm32", "--emit", "bindings-js", "--wasm-feature", "simd128", "-o", out], "E2000", 2, "--wasm-feature is not valid for bindings output; pass it when building the .wasm");
  rejected(["build", fixture, "--target", "wasm32", "--emit", "bindings-js", "-o", join(root, "rejected.js")], "E2000", 2, "bindings output must end with '.mjs'; declarations are written next to it as '<name>.d.mts'");
  rejected(["build", fixture, "--target", "wasm32", "--emit", "bindings-js", "--cpu", "native", "-o", out], "E2000", 2, "'--cpu native' requires native executable or object output");
  const empty = join(root, "empty");
  mkdirSync(empty);
  writeFileSync(join(empty, "Lib.tz"), "def one :: i64\nfn one = 1\n");
  rejected(["build", join(empty, "Lib.tz"), "--target", "wasm32", "--emit", "bindings-js", "-o", out], "E2004", 1, "bindings need at least one 'export def' entry point");
  if (process.platform !== "win32") {
    const before = readFileSync(join(fixture, "Main.tz"));
    symlinkSync(join(fixture, "Main.tz"), join(root, "linked.mjs"));
    rejected(["build", fixture, "--target", "wasm32", "--emit", "bindings-js", "-o", join(root, "linked.mjs")], "E2003", 1);
    symlinkSync(join(fixture, "Main.tz"), join(root, "declared.d.mts"));
    rejected(["build", fixture, "--target", "wasm32", "--emit", "bindings-js", "-o", join(root, "declared.mjs")], "E2003", 1);
    assert.ok(!existsSync(join(root, "declared.mjs")));
    assert.deepEqual(readFileSync(join(fixture, "Main.tz")), before);
  }
  assert.ok(!existsSync(out));
  console.log("bindings: command line checks passed");
} finally {
  WebAssembly.Instance = OriginalInstance;
  WebAssembly.instantiate = originalInstantiate;
  rmSync(root, { recursive: true, force: true });
}
