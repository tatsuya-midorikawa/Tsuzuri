import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, readFileSync, readdirSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(process.argv[2] ?? join(root, "target/debug/tsuzuri"));
const clang = process.env.TSUZURI_CLANG ?? "clang";
const temporary = mkdtempSync(join(tmpdir(), "tsuzuri-computations-"));
const fixture = join(root, "tests/fixtures/computations");
const sanitizer = process.env.TSUZURI_ASAN === "1" ? ["-fsanitize=address"] : [];

function execute(program, args, success = true) {
  const result = spawnSync(program, args, {
    cwd: root, encoding: "utf8", timeout: 180_000, maxBuffer: 4 * 1024 * 1024,
  });
  if (result.error) throw result.error;
  if (success) {
    assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  }
  return result;
}
const cli = (args, success) => execute(compiler, args, success);
const cases = [
  ["generic", [40n], 42n],
  ["generic", [(1n << 63n) - 1n], -(1n << 63n) + 1n],
  ["unit_bind", [], 42n],
  ["choices", [1], 42n],
  ["choices", [0], 20n],
  ["skipped_bind", [], 0n],
  ["yields", [], 42n],
  ["keyword_loops", [], 42n],
  ["flow_bind", [], 42n],
  ["branches", [1], 42n],
  ["branches", [0], 23n],
  ["zero_branch", [1], 42n],
  ["zero_branch", [0], 0n],
  ["loop_sum", [0n], 0n],
  ["loop_sum", [257n], 256n * 257n * 513n / 6n],
  ["nested_loops", [], 90n],
  ["owned_iterations", [], 36n],
  ["empty_loops", [], 42n],
  ["skip_delayed_tail", [], 42n],
  ["run_without_delay", [], 42n],
  ["lazy_snapshots", [], 12n],
  ["discarded_lazy", [], 42n],
  ["evaluation_order", [], 3n],
  ["borrowed", [], 42n],
  ["type_classes", [], 42n],
  ["array_bind", [], 3n],
  ["explicit_array_bind", [], 3n],
  ["nested_task", [], 42n],
  ["task_payload", [], 42n],
  ["while_delegate", [], 42n],
  ["numeric_boundary", [], -128n],
  ["decimal", [], 0.3],
  ["negative_zero", [], -0],
  ["nan_result", [], 1],
  ["heap_cycles", [8192n], 8192n * 275n],
  ["opt_scalar", [], 84n],
  ["opt_pipeline", [], 84n],
  ["opt_read_array", [], 8n],
  ["opt_read_string", [], 52n],
  ["opt_consumed_string", [], 12n],
  ["opt_copy_result", [], 104n],
  ["opt_mutable_bound_array", [], 42n],
  ["opt_mutable_bound_scalar", [], 43n],
  ["opt_snapshot_argument", [], 122n],
  ["opt_direct_snapshot", [], 122n],
  ["opt_callee_replaced", [], 1042n],
  ["opt_escaping", [], 65n],
  ["opt_return_borrow", [], 42n],
  ["opt_mutual", [], 42n],
  ["opt_two_callbacks", [], 45n],
  ["opt_initializer", [], 41n],
  ["opt_custom_operations", [], 53n],
  ["opt_empty_initializer", [], 1n],
  ["opt_list_initializer", [], 5n],
];
const traps = ["trap_bind", "trap_loop", "opt_trap_index"];
const cValue = (value) => typeof value === "bigint"
  ? value < 0n ? `(-INT64_C(${-value}))` : `INT64_C(${value})`
  : String(value);

try {
  cli(["check", fixture]);
  cli(["build", fixture, "--emit", "header", "-o", join(temporary, "computations.h")]);
  const ir = join(temporary, "computations.ll");
  const repeatedIr = join(temporary, "repeated.ll");
  cli(["build", fixture, "--emit", "llvm", "-o", ir]);
  cli(["build", join(fixture, "Flow.tc"), "--emit", "llvm", "-o", repeatedIr]);
  const sourceIr = readFileSync(ir, "utf8");
  assert.equal(readFileSync(repeatedIr, "utf8"), sourceIr);
  for (const method of ["Bind", "Return", "Yield", "Combine", "Delay", "Run", "For", "While"]) {
    assert.match(sourceIr, new RegExp(`@tz\\.fn\\.Flow\\.${method}(?:[.(])`));
  }
  let trackedIr = sourceIr.replaceAll("@malloc", "@tracked_alloc").replaceAll("@free", "@tracked_free");
  if (sanitizer.length) trackedIr = trackedIr.replaceAll(" nounwind {", " nounwind sanitize_address {");
  writeFileSync(ir, trackedIr);
  const host = join(temporary, "host.c");
  writeFileSync(host, `
#include <assert.h>
#include <math.h>
#include <stdint.h>
#include <stdlib.h>
#include "computations.h"
static uint64_t live, allocated;
void *tracked_alloc(uint64_t size) {
    uint64_t *pointer = malloc((size_t)size + 16);
    assert(pointer);
    pointer[0] = size;
    pointer[1] = UINT64_C(0x51a110ca7e);
    live += size;
    allocated += size;
    return pointer + 2;
}
void tracked_free(void *value) {
    if (!value) return;
    uint64_t *pointer = (uint64_t *)value - 2;
    assert(pointer[1] == UINT64_C(0x51a110ca7e));
    pointer[1] = 0;
    assert(live >= pointer[0]);
    live -= pointer[0];
    free(pointer);
}
int main(int argc, char **argv) {
    if (argc == 2) {
        switch (atoi(argv[1])) {
            ${traps.map((name, index) => `case ${index}: (void)tz_${name}(); break;`).join("\n")}
            default: return 2;
        }
        return 0;
    }
    ${cases.map(([name, args, expected]) => {
      const call = `tz_${name}(${args.map(cValue).join(", ")})`;
      const condition = Object.is(expected, -0)
        ? `${call} == 0.0 && signbit(${call})`
        : `${call} == ${cValue(expected)}`;
      return `assert(${condition}); assert(live == 0);`;
    }).join("\n")}
#ifdef TRACKING
    assert(allocated > 16 * 1024 * 1024);
    uint64_t before = allocated;
    assert(tz_array_bind() == 3);
    uint64_t implicit_cost = allocated - before;
    before = allocated;
    assert(tz_explicit_array_bind() == 3);
    assert(allocated - before == implicit_cost);
    assert(live == 0);
#endif
    return 0;
}
`);
  for (const optimization of [0, 3]) {
    const native = join(temporary, `native-${optimization}`);
    execute(clang, ["-std=c11", `-O${optimization}`, "-Wno-override-module", "-DTRACKING", ...sanitizer,
      host, ir, "-lm", "-o", native]);
    execute(native, []);
    for (const [index, name] of traps.entries()) {
      assert.notEqual(execute(native, [String(index)], false).status, 0, `native ${name}`);
    }

    const object = join(temporary, `computations-${optimization}.o`);
    const linked = join(temporary, `object-host-${optimization}`);
    cli(["build", fixture, "--emit", "object", `-O${optimization}`, "-o", object]);
    execute(clang, ["-std=c11", ...sanitizer, host, object, "-lm", "-o", linked]);
    execute(linked, []);

    const wasm = join(temporary, `computations-${optimization}.wasm`);
    cli(["build", fixture, "--target", "wasm32", `-O${optimization}`, "-o", wasm]);
    const module = await WebAssembly.compile(readFileSync(wasm));
    assert.deepEqual(WebAssembly.Module.imports(module), []);
    const instance = await WebAssembly.instantiate(module);
    for (const [name, args, expected] of cases) {
      assert.equal(instance.exports[`tz_${name}`](...args), expected, `WASM -O${optimization} ${name}`);
    }
    for (const name of traps) {
      const isolated = await WebAssembly.instantiate(module);
      assert.throws(() => isolated.exports[`tz_${name}`](), WebAssembly.RuntimeError, name);
    }
    assert.ok(instance.exports.memory.buffer.byteLength <= 16 * 1024 * 1024);
    const repeated = join(temporary, `repeated-${optimization}.wasm`);
    cli(["build", fixture, "--target", "wasm32", `-O${optimization}`, "-o", repeated]);
    assert.deepEqual(readFileSync(repeated), readFileSync(wasm));
    assert.equal(cli(["run", "examples/computations", `-O${optimization}`]).stdout, "42\n");
    console.log(`Computations -O${optimization}: ${cases.length} native/WASM results, ${traps.length} traps, deterministic output, owned memory reclaimed`);
  }

  const invalid = join(temporary, "invalid");
  mkdirSync(invalid);
  const main = join(invalid, "Main.tz");
  const builder = join(invalid, "Builder.tc");
  const classes = join(invalid, "Classes.tt");
  writeFileSync(main, "Builder { return 42 }");
  writeFileSync(builder, "def Return :: 'a -> 'a\nfn Return value = value");
  writeFileSync(classes, "class A<'a> { def f :: 'a -> i64 }\nclass B<'a> { def g :: 'a -> i64 }");
  cli(["check", classes]);
  for (const [path, text, code, original] of [
    [main, "Builder { yield 42 }", "E1018", readFileSync(main, "utf8")],
    [builder, "def Return :: i64 -> i64\nfn Return value = false", "E1003", readFileSync(builder, "utf8")],
    [classes, "class Bad<'a> { def f :: 'b -> 'b }", "E1016", readFileSync(classes, "utf8")],
    [classes, "def helper :: i64\nfn helper = 42", "E1018", readFileSync(classes, "utf8")],
  ]) {
    writeFileSync(path, text);
    const result = cli(["check", main, "--json"], false);
    assert.equal(result.status, 1);
    const error = JSON.parse(result.stderr);
    assert.equal(error.code, code);
    assert.equal(error.path, path);
    assert.ok(error.span.start < Buffer.byteLength(text));
    writeFileSync(path, original);
  }
  for (const path of [main, classes, builder]) {
    const before = readFileSync(path);
    const result = cli(["build", main, "--emit", "llvm", "-o", path, "--json"], false);
    assert.equal(result.status, 1);
    assert.equal(JSON.parse(result.stderr).code, "E2003");
    assert.deepEqual(readFileSync(path), before);
  }
  const old = join(invalid, "Old.tzr");
  writeFileSync(old, "42");
  const result = cli(["check", old, "--json"], false);
  assert.equal(result.status, 1);
  assert.equal(JSON.parse(result.stderr).code, "E2000");
  assert.match(result.stderr, /rename/);

  const depthProject = join(temporary, "depth");
  mkdirSync(depthProject);
  writeFileSync(join(depthProject, "Flow.tc"), readFileSync(join(fixture, "Flow.tc")));
  const depthMain = join(depthProject, "Main.tz");
  const nested = (count) => "Flow { return ".repeat(count) + "42" + " }".repeat(count);
  writeFileSync(depthMain, `export def depth :: i64\nfn depth = ${nested(25)}\ndepth()`);
  for (const optimization of [0, 3]) {
    assert.equal(cli(["run", depthProject, `-O${optimization}`]).stdout, "42\n");
    const wasm = join(temporary, `depth-${optimization}.wasm`);
    cli(["build", depthProject, "--target", "wasm32", `-O${optimization}`, "-o", wasm]);
    const { instance } = await WebAssembly.instantiate(readFileSync(wasm));
    assert.equal(instance.exports.tz_depth(), 42n);
  }
  for (const count of [26, 32]) {
    writeFileSync(depthMain, nested(count));
    const result = cli(["check", depthProject, "--json"], false);
    assert.equal(result.status, 1);
    assert.equal(JSON.parse(result.stderr).code, "E0002");
    assert.equal(JSON.parse(result.stderr).path, depthMain);
  }

  const budgetProject = join(temporary, "budget");
  mkdirSync(budgetProject);
  const budgetSource = `
def apply :: (i64 -> i64) -> i64 -> i64
fn apply body value = body value
export def budget :: i64 -> i64
fn budget offset = {
    let values = [offset];
    let mut total = 0;
    ${Array.from({ length: 520 }, (_, index) => `total = total + apply (value -> value + values[0]) ${index};`).join("\n")}
    total
}
budget 1
`;
  writeFileSync(join(budgetProject, "Main.tz"), budgetSource);
  const expectedBudget = 520n * 521n / 2n;
  for (const optimization of [0, 3]) {
    assert.equal(cli(["run", budgetProject, `-O${optimization}`]).stdout, `${expectedBudget}\n`);
    const wasm = join(temporary, `budget-${optimization}.wasm`);
    cli(["build", budgetProject, "--target", "wasm32", `-O${optimization}`, "-o", wasm]);
    const { instance } = await WebAssembly.instantiate(readFileSync(wasm));
    assert.equal(instance.exports.tz_budget(1n), expectedBudget);
  }

  const largeProject = join(temporary, "large");
  mkdirSync(largeProject);
  const benchmark = join(root, "benchmarks/computations");
  for (const name of readdirSync(benchmark).filter((name) => /\.(?:tz|tc)$/.test(name))) {
    writeFileSync(join(largeProject, name), readFileSync(join(benchmark, name)));
  }
  writeFileSync(join(largeProject, "Main.tz"), readFileSync(join(largeProject, "Main.tz"), "utf8") + `
ce_array_for 262144 42 == direct_array_for 262144 42 &&
ce_owned_capture 262144 42 == direct_owned_capture 262144 42
`);
  assert.equal(cli(["run", largeProject, "-O3"]).stdout, "true\n");
  const largeWasm = join(temporary, "large.wasm");
  cli(["build", largeProject, "--target", "wasm32", "-O3", "-o", largeWasm]);
  const { instance } = await WebAssembly.instantiate(readFileSync(largeWasm));
  for (const name of ["array_for", "owned_capture"]) {
    assert.equal(instance.exports[`tz_ce_${name}`](262144n, 42n), instance.exports[`tz_direct_${name}`](262144n, 42n));
  }
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
