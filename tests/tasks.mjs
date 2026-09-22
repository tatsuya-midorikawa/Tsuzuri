import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(process.argv[2] ?? join(root, "target/debug/tsuzuri"));
const clang = process.env.TSUZURI_CLANG ?? "clang";
const temporary = mkdtempSync(join(tmpdir(), "tsuzuri-tasks-"));
const fixture = join(root, "tests/fixtures/tasks");
function execute(program, args, success = true, environment = {}) {
  const result = spawnSync(program, args, {
    cwd: root, encoding: "utf8", timeout: 180_000, maxBuffer: 4 * 1024 * 1024,
    env: { ...process.env, ...environment },
  });
  if (result.error) throw result.error;
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}
const cli = (args, success, environment) => execute(compiler, args, success, environment);
const cases = [
  ["task_parallel", [], 42n],
  ["sequence", [20n], 41n],
  ["parallel_sum", [0n], 0n],
  ["parallel_sum", [1n], 0n],
  ["parallel_sum", [257n], 256n * 257n * 513n / 6n],
  ["ordered", [257n], 1],
  ["nested", [7n], 2464n],
  ["owned_results", [], 12n],
  ["record_results", [], 17n],
  ["list_results", [], 42n],
  ["function_results", [], 42n],
  ["function_captures", [], 42n],
  ["partial_move", [], 5n],
  ["snapshot", [], 42n],
  ["curried", [], 42n],
  ["nested_task", [], 42n],
  ["nested_group", [], 42n],
  ["cold", [], 42n],
  ["units", [], 16n],
  ["integer_boundary", [], 1],
  ["wide_numbers", [], 1],
  ["negative_zero", [], -0],
  ["nan_result", [], 1],
  ["heap_cycles", [2048n], 3145728n],
  ["parallel_cycles", [128n], 896n],
  ["monad_laws", [20n], 1],
];
const traps = ["trap_task", "trap_parallel", "trap_allocation"];

try {
  cli(["check", fixture]);
  cli(["build", fixture, "--emit", "header", "-o", join(temporary, "tasks.h")]);
  const ir = join(temporary, "tasks.ll");
  const missingClang = { TSUZURI_CLANG: join(temporary, "missing-clang") };
  cli(["build", fixture, "--emit", "llvm", "-o", ir], true, missingClang);
  const protectedOutput = join(temporary, "preserved.o");
  writeFileSync(protectedOutput, "previous successful artifact");
  const failedBuild = cli(["build", fixture, "--emit", "object", "-o", protectedOutput], false, missingClang);
  assert.equal(failedBuild.status, 1);
  assert.match(failedBuild.stderr, /E2002/);
  assert.equal(readFileSync(protectedOutput, "utf8"), "previous successful artifact");
  const sourceIr = readFileSync(ir, "utf8");
  assert.equal(sourceIr.match(/declare void @tsuzuri_task_parallel\(/g)?.length, 1);
  assert.doesNotMatch(sourceIr, /@tz\.env\.clone\.\$task\./);
  writeFileSync(ir, sourceIr.replaceAll("@malloc", "@tracked_alloc").replaceAll("@free", "@tracked_free"));
  const host = join(temporary, "host.c");
  writeFileSync(host, `
#include <assert.h>
#include <math.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdlib.h>
#include "tasks.h"
extern int64_t tz_other(void);
static _Atomic uint64_t live, allocated;
void *tracked_alloc(uint64_t size) {
    uint64_t *pointer = malloc((size_t)size + 16);
    assert(pointer);
    pointer[0] = size;
    pointer[1] = UINT64_C(0x51a110ca7e);
    atomic_fetch_add(&live, size);
    atomic_fetch_add(&allocated, size);
    return pointer + 2;
}
void tracked_free(void *value) {
    if (!value) return;
    uint64_t *pointer = (uint64_t *)value - 2;
    assert(pointer[1] == UINT64_C(0x51a110ca7e));
    pointer[1] = 0;
    uint64_t before = atomic_fetch_sub(&live, pointer[0]);
    assert(before >= pointer[0]);
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
      const call = `tz_${name}(${args.map((value) => `${value}LL`).join(", ")})`;
      const condition = Object.is(expected, -0)
        ? `${call} == 0.0 && signbit(${call})`
        : `${call} == ${expected}${typeof expected === "bigint" ? "LL" : ""}`;
      return `assert(${condition}); assert(atomic_load(&live) == 0);`;
    }).join("\n")}
#ifdef TRACKING
    assert(atomic_load(&allocated) > 16 * 1024 * 1024);
#endif
#ifdef OTHER
    assert(tz_other() == 42);
#endif
    return 0;
}
`);
  for (const optimization of [0, 3]) {
    const scheduler = join(temporary, `scheduler-${optimization}`);
    execute(clang, ["-std=c11", "-Wall", "-Wextra", "-Werror", `-O${optimization}`,
      "-pthread", "tests/task_runtime.c", "-o", scheduler]);
    execute(scheduler, []);
    for (const operation of ["create", "join"]) {
      const failed = execute(scheduler, [operation], false);
      assert.notEqual(failed.status, 0, `pthread_${operation} failures must be reported`);
      assert.match(failed.stderr, new RegExp(`Tsuzuri task runtime: pthread_${operation} failed`));
    }

    const native = join(temporary, `native-${optimization}`);
    execute(clang, ["-std=c11", `-O${optimization}`, "-Wno-override-module", "-DTRACKING",
      host, ir, "src/runtime/task.c", "-pthread", "-lm", "-o", native]);
    execute(native, []);
    for (const [index, name] of traps.entries()) {
      assert.notEqual(execute(native, [String(index)], false).status, 0, `native ${name}`);
    }

    const object = join(temporary, `tasks-${optimization}.o`);
    const linked = join(temporary, `object-host-${optimization}`);
    cli(["build", fixture, "--emit", "object", `-O${optimization}`, "-o", object]);
    execute(clang, ["-std=c11", host, object, "-pthread", "-lm", "-o", linked]);
    execute(linked, []);

    const wasm = join(temporary, `tasks-${optimization}.wasm`);
    cli(["build", fixture, "--target", "wasm32", `-O${optimization}`, "-o", wasm]);
    const module = await WebAssembly.compile(readFileSync(wasm));
    assert.deepEqual(WebAssembly.Module.imports(module), []);
    const instance = await WebAssembly.instantiate(module);
    for (const [name, args, expected] of cases) {
      assert.equal(instance.exports[`tz_${name}`](...args), expected, `WASM -O${optimization} ${name}`);
    }
    for (const name of traps) {
      const isolated = await WebAssembly.instantiate(module);
      assert.throws(() => isolated.exports[`tz_${name}`](), WebAssembly.RuntimeError, `WASM ${name}`);
    }
    if (instance.exports.memory) {
      assert.ok(instance.exports.memory.buffer.byteLength <= 16 * 1024 * 1024);
    }
    console.log(`Tasks -O${optimization}: ${cases.length} native/WASM results, ${traps.length} traps, bounded/joined threads, owned memory reclaimed`);
  }

  assert.equal(cli(["run", "examples/tasks", "-O0"]).stdout, "21325334000\n");
  assert.equal(cli(["run", "examples/tasks", "-O3", "--cpu", "native"]).stdout, "21325334000\n");

  // Each separately built native object carries a coalescible task runtime.
  const other = join(temporary, "other");
  mkdirSync(other);
  writeFileSync(join(other, "Other.tz"), `
export def other :: i64
fn other = { let values = Task.run (Task.parallel [task { 42 }]); values[0] }
`);
  const otherObject = join(other, "other.o");
  cli(["build", join(other, "Other.tz"), "--emit", "object", "-o", otherObject]);
  execute(clang, ["-std=c11", "-DOTHER", host, join(temporary, "tasks-3.o"), otherObject, "-pthread", "-lm",
    "-o", join(temporary, "combined")]);
  execute(join(temporary, "combined"), []);
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
