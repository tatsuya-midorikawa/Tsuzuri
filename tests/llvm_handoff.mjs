// PB03 D1: a native executable defines only the functions that its entry and its exports reach.
// Libraries, objects, LLVM IR and WebAssembly keep their output. The IR that reaches Clang is copied by a
// wrapper around TSUZURI_CLANG, so the checks read what the executable was really built from.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { chmodSync, cpSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const fixtures = resolve("tests/fixtures/llvm_handoff");
const windows = process.platform === "win32";
const root = mkdtempSync(join(tmpdir(), "tsuzuri-handoff-"));
const executable = (name) => join(root, windows ? `${name}.exe` : name);

function execute(program, args, { env = process.env, input = "", success = true } = {}) {
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 300_000, env, input, maxBuffer: 64 * 1024 * 1024 });
  assert.ifError(result.error);
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}

function project(name) {
  const target = join(root, `project-${name}`);
  if (!existsSync(target)) cpSync(join(fixtures, name), target, { recursive: true });
  return target;
}

// A wrapper that copies the IR that reaches Clang (module.ll) and runs the real Clang. Unix only.
const spy = join(root, "clang-spy.mjs");
if (!windows) {
  writeFileSync(spy, `#!${process.execPath}
import { spawnSync } from "node:child_process";
import { copyFileSync } from "node:fs";
import { basename } from "node:path";
const args = process.argv.slice(2);
if (process.env.HANDOFF_IR_COPY) for (const argument of args) if (basename(argument) === "module.ll") copyFileSync(argument, process.env.HANDOFF_IR_COPY);
const result = spawnSync(process.env.HANDOFF_REAL_CLANG, args, { stdio: "inherit" });
process.exitCode = result.status ?? 1;
`);
  chmodSync(spy, 0o755);
}
let copies = 0;
function build(name, args, output, { capture = false } = {}) {
  const env = { ...process.env, TSUZURI_CACHE_DIR: join(root, "cache") };
  let ir = null;
  if (capture) {
    ir = join(root, `ir-${++copies}.ll`);
    Object.assign(env, { TSUZURI_CLANG: spy, HANDOFF_IR_COPY: ir, HANDOFF_REAL_CLANG: process.env.TSUZURI_CLANG ?? "clang" });
  }
  execute(compiler, ["build", project(name), ...args, "--no-cache", "-o", output], { env });
  return capture ? readFileSync(ir, "utf8") : null;
}

function defined(ir) {
  return [...ir.matchAll(/^define .*?@("[^"]+"|[^\s(]+)\(/gm)].map((match) => match[1].replace(/^"|"$/g, ""));
}
const slots = (names) => names.filter((name) => /^tz\.dyn\.slot\.Main\.\$instance\.\d+\.size$/.test(name)).length;

try {
  for (const optimization of ["-O0", "-O3"]) {
    // 1. The executable of `prune` runs.
    const exe = executable(`prune${optimization}`);
    const ir = build("prune", [optimization], exe, { capture: !windows });
    assert.equal(execute(exe, []).stdout.replaceAll("\r\n", "\n"), "42\n");
    if (!windows) {
      // 2. It defines what main and the export reach, and not the public function that nothing reaches.
      const names = defined(ir);
      for (const name of ["tz.fn.Lib.used", "tz.fn.Lib.exported", "tz_exported", "main"]) assert.ok(names.includes(name), `${optimization}: ${name}`);
      for (const name of ["tz.fn.Lib.unused", "tz.apply.Lib.unused.0"]) assert.ok(!names.includes(name), `${optimization}: ${name}`);
      // 3. The IR of a library and of an object keeps every public function.
      const object = build("prune", [optimization, "--emit", "object"], join(root, `prune${optimization}.o`), { capture: true });
      assert.ok(defined(object).includes("tz.fn.Lib.unused"), `${optimization}: object`);
      build("prune", [optimization, "--emit", "llvm"], join(root, `prune${optimization}.ll`));
      assert.ok(defined(readFileSync(join(root, `prune${optimization}.ll`), "utf8")).includes("tz.fn.Lib.unused"));
      // 5. The same input gives the same IR.
      assert.equal(build("prune", [optimization], executable(`again${optimization}`), { capture: true }), ir);
    }
    // 4. WebAssembly keeps its imports empty and its export callable.
    const wasm = join(root, `prune${optimization}.wasm`);
    build("prune", [optimization, "--target", "wasm32"], wasm);
    const module = new WebAssembly.Module(readFileSync(wasm));
    assert.deepEqual(WebAssembly.Module.imports(module), []);
    assert.equal(new WebAssembly.Instance(module).exports.tz_exported(5n), 4n);

    // 6. Dyn vtables, function values, helpers of an export and a second module are kept when reached.
    const reach = executable(`reach${optimization}`);
    const reachIr = build("reach", [optimization], reach, { capture: !windows });
    assert.equal(execute(reach, []).stdout.replaceAll("\r\n", "\n"), "43,16,10\n");
    if (!windows) {
      const names = defined(reachIr);
      for (const name of ["tz.fn.Main.only_for_export", "tz.fn.Main.apply_twice", "tz.fn.Main.add_three", "tz.fn.Extra.used_helper", "tz_exported_seven"]) {
        assert.ok(names.includes(name), `${optimization}: ${name}`);
      }
      for (const name of ["tz.fn.Main.unused_total", "tz.fn.Main.unused_closure", "tz.fn.Main.cubes", "tz.fn.Extra.unused_helper"]) {
        assert.ok(!names.includes(name), `${optimization}: ${name}`);
      }
      // The Square and Bar vtables stay; the Cube vtable, built only by the unreachable function, goes.
      assert.equal(slots(names), 2, `${optimization}: slots of ${names.filter((name) => name.includes("dyn.slot"))}`);
      build("reach", [optimization, "--emit", "llvm"], join(root, `reach${optimization}.ll`));
      const library = defined(readFileSync(join(root, `reach${optimization}.ll`), "utf8"));
      assert.equal(slots(library), 3);
      for (const name of ["tz.fn.Main.unused_total", "tz.fn.Extra.unused_helper"]) assert.ok(library.includes(name), name);
    }
    const reachWasm = join(root, `reach${optimization}.wasm`);
    build("reach", [optimization, "--target", "wasm32"], reachWasm);
    const instance = new WebAssembly.Instance(new WebAssembly.Module(readFileSync(reachWasm)));
    assert.equal(instance.exports.tz_exported_seven(6n), 42n);

    // 7. The drops of the ends of a channel stay roots, and their builtins load the element type, so its definition must not
    // go with an unused function that is the only one to name it (std `Maybe<i64>`, a user generic record).
    for (const name of ["channel_std", "channel_user"]) {
      const channel = executable(`${name}${optimization}`);
      build(name, [optimization], channel);
      assert.equal(execute(channel, []).stdout.replaceAll("\r\n", "\n"), "ok\n", `${name} ${optimization}`);
    }

    // 8. Dead code that used to fail the build builds now: an extern symbol that no library provides, named only by an
    // unused public function. The entry that calls it still fails at the link.
    const dead = executable(`dead-extern${optimization}`);
    build("dead_extern", [optimization], dead);
    assert.equal(execute(dead, []).stdout.replaceAll("\r\n", "\n"), "ok\n");
    const reached = execute(compiler, ["build", project("dead_extern_reached"), optimization, "--no-cache", "-o", executable(`reached${optimization}`)], { success: false });
    assert.notEqual(reached.status, 0);
    assert.match(reached.stderr, /pb03_missing_symbol/);

    // 10. `tsuzuri run` instruments traps. Two runtime parts that share a line (`}define ...`) are two functions, and pruning an unused
    // function that locks a `Mutex` removes the line break between them (`tree_lock` built before, `tree` did not build at all).
    if (!windows) {
      for (const name of ["tree", "tree_lock"]) {
        const result = execute(compiler, ["run", project(name), optimization, "--no-cache"]);
        assert.equal(result.stdout, "[1, 2, 3] 1\n", `${name} ${optimization}`);
      }
    }
  }

  // 9. The same for an operating-system API on a target that has none (the driver refuses before any tool runs): only a program
  // that reaches `File.read_text` is refused.
  const windows_gnu = (name) => execute(compiler, ["build", project(name), "--target", "x86_64-pc-windows-gnu", "-O0", "--no-cache", "-o", join(root, `${name}.exe`)], { success: false });
  const refusal = /operating-system APIs are not supported on Windows/;
  assert.match(windows_gnu("dead_os_reached").stderr, refusal);
  assert.doesNotMatch(windows_gnu("dead_os").stderr, refusal);
  console.log("llvm handoff: roots ok");
} finally {
  rmSync(root, { recursive: true, force: true });
}
