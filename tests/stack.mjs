// PM09: the calls between the functions of one recursion are guaranteed tail calls, so a mutual recursion of ten
// million calls runs in the stack of one call. The native half builds tests/fixtures/stack with each optimization level
// and runs it (with --backend fast too, where the compiler has it). The WebAssembly half builds it by default, where the
// IR has no `musttail` and nothing else changes, and with --wasm-feature tail-call, where the module runs in V8 in
// constant stack. Usage: node tests/stack.mjs [compiler]. TSUZURI_OBJDUMP names llvm-objdump (it counts the
// return_call instructions); TSUZURI_STACK_FAST=1 makes a compiler without --backend fast a failure instead of a skip.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const objdump = process.env.TSUZURI_OBJDUMP ?? "llvm-objdump";
const fixture = "tests/fixtures/stack";
const depth = 10_000_000n;
// 2 for even and odd, 3 for the cycle of ca, cb and cc.
const guaranteedCalls = 5;
const root = mkdtempSync(join(tmpdir(), "tsuzuri-stack-"));
const exe = process.platform === "win32" ? ".exe" : "";

function run(program, args, { env = {}, status = 0 } = {}) {
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 300_000, maxBuffer: 64 * 1024 * 1024, env: { ...process.env, ...env } });
  assert.ifError(result.error);
  if (status !== null) assert.equal(result.status, status, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}
// A command whose failure is what the test looks at.
const attempt = (program, args) => run(program, args, { status: null });
const build = (name, args, env) => {
  const output = join(root, name);
  run(compiler, ["build", fixture, ...args, "-o", output], { env });
  return output;
};

// The features that an object or a linked module names in its `target_features` section, as "+name" and "-name".
function wasmFeatures(bytes) {
  let position = 8;
  const leb = () => {
    let value = 0, shift = 0, byte;
    do { byte = bytes[position++]; value |= (byte & 0x7f) << shift; shift += 7; } while (byte & 0x80);
    return value;
  };
  const name = () => { const length = leb(); const text = bytes.subarray(position, position + length).toString("utf8"); position += length; return text; };
  while (position < bytes.length) {
    const id = bytes[position++], size = leb(), end = position + size;
    if (id === 0 && name() === "target_features") {
      const features = [];
      for (let count = leb(); count > 0; count--) { const prefix = String.fromCharCode(bytes[position++]); features.push(prefix + name()); }
      return features;
    }
    position = end;
  }
  return null;
}
const returnCalls = (path) => (run(objdump, ["-d", path]).stdout.match(/\breturn_call\b/g) ?? []).length;

// What the programs of the two exports return, which a C host or V8 reads.
function callExports(path) {
  const module = new WebAssembly.Module(readFileSync(path));
  const imports = {};
  for (const { module: space, name, kind } of WebAssembly.Module.imports(module)) {
    assert.equal(kind, "function");
    (imports[space] ??= {})[name] = () => 0;
  }
  const { exports } = new WebAssembly.Instance(module, imports);
  return { even: () => exports.tz_stack_even(depth), cycle: () => exports.tz_stack_cycle(depth) };
}

try {
  run(objdump, ["--version"]);

  // Native: both levels, and the instrumented builds that add an argument to the calls (--trap-info) or a line to each one (-g).
  for (const optimization of ["-O0", "-O3"]) {
    for (const extra of [[], ["--trap-info"], ["-g"]]) {
      const program = build(`native${optimization}${extra.join("")}${exe}`, [optimization, ...extra]);
      assert.equal(run(program, []).stdout.replaceAll("\r\n", "\n"), `1\n${depth}\n`, `native ${optimization} ${extra}`);
    }
    console.log(`stack: native ${optimization} ran a depth of ${depth} in the default, --trap-info and -g builds`);
  }
  // The IR: each guaranteed call is a `musttail call` whose next line is the `ret` of its value.
  const native = readFileSync(build("native.ll", ["--emit", "llvm"]), "utf8").split("\n");
  const calls = native.flatMap((line, index) => (line.includes("musttail call") ? [[line, native[index + 1]]] : []));
  assert.equal(calls.length, guaranteedCalls);
  for (const [call, next] of calls) assert.match(next, new RegExp(`^\\s*ret (i1|i64) ${call.trim().split(" ")[0]}$`), call);

  // The fast backend translates `musttail` to a jump for the functions that it can give the tail convention to.
  const fast = attempt(compiler, ["build", fixture, "--backend", "fast", "-O0", "-o", join(root, `fast${exe}`)]);
  if (fast.status === 0) {
    for (const extra of [[], ["--trap-info"], ["-g"]]) {
      const program = build(`fast${extra.join("")}${exe}`, ["--backend", "fast", "-O0", ...extra]);
      assert.equal(run(program, []).stdout.replaceAll("\r\n", "\n"), `1\n${depth}\n`, `fast ${extra}`);
    }
    console.log("stack: native --backend fast -O0 ran the same depth in the default, --trap-info and -g builds");
  } else {
    assert.match(fast.stderr, /built without the fast backend|--backend fast is not available/, fast.stderr);
    assert.notEqual(process.env.TSUZURI_STACK_FAST, "1", `--backend fast is not available: ${fast.stderr}`);
    console.log("stack: --backend fast is not in this compiler, skipped");
  }

  // WebAssembly by default: no `musttail`, no tail-call instruction, no tail-call feature in the object.
  const defaultIr = readFileSync(build("default.ll", ["--target", "wasm32", "--emit", "llvm"]), "utf8");
  assert.ok(!defaultIr.includes("musttail") && !defaultIr.includes("wasm-feature"));
  for (const optimization of ["-O0", "-O3"]) {
    const module = build(`default${optimization}.wasm`, ["--target", "wasm32", optimization]);
    assert.ok(WebAssembly.validate(readFileSync(module)));
    assert.equal(returnCalls(module), 0, `default ${optimization}`);
    const object = build(`default${optimization}.o`, ["--target", "wasm32", "--emit", "object", optimization]);
    assert.ok(!wasmFeatures(readFileSync(object)).includes("+tail-call"), `default ${optimization}`);
    // Whether V8 holds ten million frames is not what is checked: -O0 overflows its stack, -O3 turns the recursion into a loop.
    let outcome;
    try { outcome = `${callExports(module).cycle()}`; } catch (error) { outcome = error.constructor.name; }
    console.log(`stack: default wasm32 ${optimization}: no musttail, no return_call, no tail-call feature; depth ${depth} gives ${outcome}`);
  }

  // WebAssembly with the feature: the IR has the note and the `musttail` calls, the code has the return_call
  // instructions (at -O0, where nothing inlines the recursion), and the module runs at the full depth.
  const tailIr = readFileSync(build("tail.ll", ["--target", "wasm32", "--emit", "llvm", "--wasm-feature", "tail-call"]), "utf8");
  assert.equal(tailIr.split("\n")[0], "; wasm-feature: tail-call; compile this IR with -mtail-call");
  assert.equal(tailIr.match(/musttail call /g).length, guaranteedCalls);
  for (const optimization of ["-O0", "-O3"]) {
    for (const extra of [[], ["--trap-info"], ["-g"]]) {
      const label = `tail-call ${optimization} ${extra}`;
      const module = build(`tail${optimization}${extra.join("")}.wasm`, ["--target", "wasm32", optimization, "--wasm-feature", "tail-call", ...extra]);
      const bytes = readFileSync(module);
      assert.ok(WebAssembly.validate(bytes), label);
      if (optimization === "-O0") assert.ok(returnCalls(module) >= guaranteedCalls, `${label}: ${returnCalls(module)} return_call`);
      const { even, cycle } = callExports(module);
      assert.equal(even(), 1n, label);
      assert.equal(cycle(), depth, label);
    }
    const object = build(`tail${optimization}.o`, ["--target", "wasm32", "--emit", "object", optimization, "--wasm-feature", "tail-call"]);
    assert.ok(wasmFeatures(readFileSync(object)).includes("+tail-call"), `tail-call ${optimization}`);
    console.log(`stack: wasm32 --wasm-feature tail-call ${optimization} ran a depth of ${depth} (default, --trap-info and -g builds)`);
  }

  // The option is part of what the build cache keys: with one cache directory, alternating builds give each its own module.
  const env = { TSUZURI_CACHE_DIR: join(root, "cache") };
  const withReturnCalls = [];
  for (const feature of [[], ["--wasm-feature", "tail-call"], [], ["--wasm-feature", "tail-call"]]) {
    withReturnCalls.push(returnCalls(build("cached.wasm", ["--target", "wasm32", "-O0", ...feature], env)) > 0);
  }
  assert.deepEqual(withReturnCalls, [false, true, false, true]);

  // The options that cannot have it say so.
  for (const [args, message] of [
    [["--wasm-feature", "tail-call"], "--wasm-feature tail-call requires wasm32 or wasm64 object, LLVM IR, or WASM output"],
    [["--target", "wasm32", "--wasm-feature", "tail-call", "--backend", "fast", "-O0"], "--backend fast does not support --wasm-feature tail-call for wasm32; remove it or use --backend llvm"],
  ]) {
    const refused = attempt(compiler, ["build", fixture, ...args, "-o", join(root, "refused")]);
    assert.notEqual(refused.status, 0, args.join(" "));
    assert.ok(refused.stderr.includes(message), `${args.join(" ")}: ${refused.stderr}`);
  }
  console.log("stack: the default WebAssembly build is unchanged, the option is cached apart and refused where it has no meaning");
} finally { rmSync(root, { recursive: true, force: true }); }
