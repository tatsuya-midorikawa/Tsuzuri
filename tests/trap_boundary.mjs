import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { createBoundary } from "../src/runtime/trap-boundary.mjs";

const repository = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const root = mkdtempSync(join(tmpdir(), "tsuzuri-trap-boundary-"));
const min = -(1n << 63n);

function build(fixture, name, optimization, ...flags) {
  const output = join(root, `${name}${optimization}.wasm`);
  const result = spawnSync(compiler, ["build", join(repository, "tests/fixtures", fixture), "--target", "wasm32", optimization, ...flags, "-o", output], { encoding: "utf8", timeout: 180_000 });
  assert.ifError(result.error);
  assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`);
  return output;
}

const compile = (path) => new WebAssembly.Module(readFileSync(path));
// d(0) = 0, d(n) = 3 d(n - 1) + n, the recursion in the fixture's `depth`.
const depth = (n) => { let value = 0n; for (let k = 1n; k <= n; k++) value = 3n * value + k; return value; };

try {
  for (const optimization of ["-O0", "-O3"]) {
    let cases = 0;
    const output = build("trap_boundary", "main", optimization, "--trap-info");
    const module = compile(output);
    const { sites } = JSON.parse(readFileSync(`${output}.trap.json`, "utf8"));
    const boundary = createBoundary(module, { sites });

    assert.deepEqual(WebAssembly.Module.imports(module), []); cases++;
    assert.deepEqual(boundary.call("tz_div", 7n, 2n), { ok: true, value: 3n }); cases++;
    assert.deepEqual(boundary.call("tz_div", -7n, 2n), { ok: true, value: -3n }); cases++;
    const memory = boundary.exports.memory;
    boundary.call("tz_div", 9n, 3n);
    assert.equal(boundary.exports.memory, memory); cases++;

    const zero = boundary.call("tz_div", 7n, 0n);
    assert.equal(zero.ok, false);
    assert.equal(zero.trap.reason, "trap");
    assert.ok(zero.trap.site > 0 && sites.some((site) => site.id === zero.trap.site));
    assert.equal(zero.trap.kind, "integer division by zero");
    assert.ok(zero.trap.path.endsWith("Main.tz"));
    // `a / b` starts at column 14 of line 2 in the fixture.
    assert.equal(zero.trap.line, 2);
    assert.equal(zero.trap.column, 14); cases++;
    assert.notEqual(boundary.exports.memory, memory); cases++;
    const overflow = boundary.call("tz_div", min, -1n);
    assert.equal(overflow.ok, false);
    assert.equal(overflow.trap.kind, "integer division overflow");
    assert.notEqual(overflow.trap.site, zero.trap.site); cases++;

    assert.deepEqual(boundary.call("tz_deep", 100000000n), { ok: false, trap: { reason: "stack", site: 0 } }); cases++;
    assert.deepEqual(boundary.call("tz_div", 9n, 3n), { ok: true, value: 3n }); cases++;
    assert.deepEqual(boundary.call("tz_deep", 10n), { ok: true, value: depth(10n) }); cases++;
    const stable = boundary.exports.memory;
    assert.throws(() => boundary.call("tz_nope"), { name: "TypeError", message: "unknown export tz_nope" });
    assert.equal(boundary.exports.memory, stable); cases++;

    const untraced = createBoundary(compile(build("trap_boundary", "untraced", optimization)));
    assert.deepEqual(untraced.call("tz_div", 7n, 0n), { ok: false, trap: { reason: "trap", site: 0 } }); cases++;

    const hostOutput = build("trap_boundary_host", "host", optimization, "--trap-info");
    const hostModule = compile(hostOutput);
    assert.deepEqual(WebAssembly.Module.imports(hostModule), [{ module: "tsuzuri", name: "Main.fail_host", kind: "function" }]); cases++;
    let host = (n) => n * 2n;
    const imported = createBoundary(hostModule, { imports: { tsuzuri: { "Main.fail_host": (n) => host(n) } } });
    assert.deepEqual(imported.call("tz_call_host", 20n), { ok: true, value: 41n }); cases++;
    const failure = new Error("host failed");
    host = () => { throw failure; };
    const before = imported.exports.memory;
    assert.throws(() => imported.call("tz_call_host", 5n), (error) => error === failure);
    assert.notEqual(imported.exports.memory, before); cases++;
    host = (n) => n * 2n;
    assert.deepEqual(imported.call("tz_call_host", 1n), { ok: true, value: 3n }); cases++;

    const threads = compile(build("trap_boundary", "threads", optimization, "--wasm-feature", "threads"));
    assert.ok(WebAssembly.Module.imports(threads).some(({ module: namespace }) => namespace === "tsuzuri_threads"));
    assert.throws(() => createBoundary(threads), { message: "createBoundary does not support modules built with --wasm-feature threads; use createThreadPool" }); cases++;

    assert.equal(cases, 17);
    console.log(`Trap boundary ${optimization}: ${cases} cases`);
  }
} finally { rmSync(root, { recursive: true, force: true }); }
