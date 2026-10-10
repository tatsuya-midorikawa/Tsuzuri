import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync, chmodSync } from "node:fs";
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
  ["parallel_results_recursive", [0], 2n], ["parallel_results_recursive", [1], 5n],
  ...[0n, 1n, 257n].map(count => ["parallel_results_ok", [count], count * (count - 1n) * (2n * count - 1n) / 6n]),
  ["parallel_results_lowest", [], 2n],
  ...[0n, 1n, 257n, 2048n].map(count => ["parallel_results_owned", [count], count === 0n ? 0n : 6n]),
  ["parallel_results_values", [], 10n], ["parallel_results_functions", [], 42n],
  ["parallel_results_nested", [], 2016n], ["parallel_results_first_class", [], 42n], ["parallel_results_cold", [], 42n],
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
const traps = ["trap_task", "trap_parallel", "trap_allocation", "trap_parallel_results"];

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
    for (const topology of ["-1", "0", "1", "4", "1000"]) execute(scheduler, [topology]);
    for (const operation of ["create", "join", "mutex_lock", "mutex_unlock", "cond_wait", "cond_broadcast", "once", "atexit"]) {
      const failed = execute(scheduler, [operation], false);
      assert.notEqual(failed.status, 0, `pthread_${operation} failures must be reported`);
      assert.match(failed.stderr, new RegExp(`Tsuzuri task runtime: ${operation === "atexit" ? "atexit" : `pthread_${operation}`} failed`));
    }

    // F10: the lock of Mutex.with_lock (exclusion, parking, refusals and the abandonment that a trap
    // boundary makes), also under the sanitizer that the environment asks for.
    const locks = join(temporary, `sync-${optimization}`);
    execute(clang, ["-std=c11", "-Wall", "-Wextra", "-Werror", `-O${optimization}`, "-pthread", "tests/sync_runtime.c", "-o", locks]);
    assert.match(execute(locks, []).stdout, /^sync_runtime: .* passed/);
    for (const [variable, sanitizer] of [["TSUZURI_TSAN", "thread"], ["TSUZURI_ASAN", "address"]]) {
      if (process.env[variable] !== "1") continue;
      const checked = join(temporary, `sync-${sanitizer}-${optimization}`);
      execute(clang, ["-std=c11", `-O${optimization}`, "-g", `-fsanitize=${sanitizer}`, "-pthread", "tests/sync_runtime.c", "-o", checked]);
      assert.match(execute(checked, []).stdout, /^sync_runtime: .* passed/);
    }

    // F10 Phase 2: the channel runtime (waiting, waking, the deadlock verdict, helping) on a pool of
    // one to four threads, which the test sets because the pool reads the CPU count once.
    const channels = join(temporary, `channel-${optimization}`);
    execute(clang, ["-std=c11", "-Wall", "-Wextra", "-Werror", `-O${optimization}`, "-pthread", "tests/channel_runtime.c", "-o", channels]);
    for (const threads of ["1", "2", "3", "4"]) {
      assert.match(execute(channels, [threads]).stdout, /^channel_runtime: \d+ threads passed/);
    }
    for (const [variable, sanitizer] of [["TSUZURI_TSAN", "thread"], ["TSUZURI_ASAN", "address"]]) {
      if (process.env[variable] !== "1") continue;
      const checked = join(temporary, `channel-${sanitizer}-${optimization}`);
      execute(clang, ["-std=c11", `-O${optimization}`, "-g", `-fsanitize=${sanitizer}`, "-pthread", "tests/channel_runtime.c", "-o", checked]);
      for (const threads of ["1", "2", "4"]) {
        assert.match(execute(checked, [threads]).stdout, /^channel_runtime: \d+ threads passed/);
      }
    }

    // F10 Phase 2: Tsuzuri pipelines between tasks that run at the same time, on pools of one to four
    // threads. Each size is its own build because the pool reads the CPU count once. A result never
    // depends on the schedule; an export that needs more threads than there are traps with the
    // deadlock message instead of hanging, which the one-thread pool shows for every export.
    const channelIr = join(temporary, `channel-threads-${optimization}.ll`);
    cli(["build", join(root, "tests/fixtures/channel_threads"), "--emit", "llvm", `-O${optimization}`, "-o", channelIr]);
    const channelCases = [
      ["pipeline", [3, 1000], "500500", 2], ["pipeline", [1, 257], String(257 * 258 / 2), 2],
      ["ping_pong", [2000], "2000002000", 2], ["tokens_across", [200], "200200001", 2],
      ["three_stages", [500], "250500", 3], ["fan_in", [2, 50, 3], "5050100", 3], ["job_queue", [100, 4], "338350100", 3],
    ];
    const channelTraps = ["leaked_sender", "deadlock_pair"];
    const channelHost = join(temporary, `channel-host-${optimization}.c`);
    const prototype = (name, count) => `extern int64_t tz_${name}(${Array(count).fill("int64_t").join(", ") || "void"});`;
    writeFileSync(channelHost, `
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
${[...new Set(channelCases.map(([name, args]) => prototype(name, args.length))), ...channelTraps.map((name) => prototype(name, 0))].join("\n")}
int main(int argc, char **argv) {
    if (argc != 2) return 2;
    int64_t result = -1;
    switch (atoi(argv[1])) {
${channelCases.map(([name, args], index) => `        case ${index}: result = tz_${name}(${args.map((value) => `${value}LL`).join(", ")}); break;`).join("\n")}
${channelTraps.map((name, index) => `        case ${channelCases.length + index}: result = tz_${name}(); break;`).join("\n")}
        default: return 2;
    }
    printf("%lld\\n", (long long)result);
    return 0;
}
`);
    const deadlock = /Tsuzuri runtime: deadlock: every task is waiting on a channel/;
    const exercise = (program, threads) => {
      for (const [index, [name, , expected, needed]] of channelCases.entries()) {
        if (threads >= needed) {
          for (let repeat = 0; repeat < 3; repeat++) {
            assert.equal(execute(program, [String(index)]).stdout.trim(), expected, `${name} on ${threads} threads`);
          }
        } else if (threads === 1 || (name === "three_stages" && threads === 2)) {
          const trapped = execute(program, [String(index)], false);
          assert.notEqual(trapped.status, 0, `${name} on ${threads} threads traps`);
          assert.match(trapped.stderr, deadlock, `${name} on ${threads} threads`);
        }
      }
      for (const [offset, name] of channelTraps.entries()) {
        const trapped = execute(program, [String(channelCases.length + offset)], false);
        assert.notEqual(trapped.status, 0, `${name} on ${threads} threads traps`);
        assert.match(trapped.stderr, deadlock, `${name} on ${threads} threads`);
      }
    };
    for (const threads of [1, 2, 3, 4]) {
      const program = join(temporary, `channel-threads-${optimization}-${threads}`);
      execute(clang, ["-std=c11", `-O${optimization}`, "-Wno-override-module", `-DTZ_TASK_SYSCONF(name)=${threads}L`,
        channelHost, channelIr, "src/runtime/task.c", "-pthread", "-lm", "-o", program]);
      exercise(program, threads);
    }
    // The runtime of the pool is the part that the sanitizers instrument here; the generated code only
    // calls it, so a race in the waiting protocol of a real Tsuzuri pipeline would be reported.
    for (const [variable, sanitizer] of [["TSUZURI_TSAN", "thread"], ["TSUZURI_ASAN", "address"]]) {
      if (process.env[variable] !== "1") continue;
      for (const threads of [2, 4]) {
        const checked = join(temporary, `channel-threads-${sanitizer}-${optimization}-${threads}`);
        execute(clang, ["-std=c11", `-O${optimization}`, "-g", `-fsanitize=${sanitizer}`, "-Wno-override-module", `-DTZ_TASK_SYSCONF(name)=${threads}L`,
          channelHost, channelIr, "src/runtime/task.c", "-pthread", "-lm", "-o", checked]);
        exercise(checked, threads);
      }
    }
    console.log(`Tasks -O${optimization}: channel pipelines on 1 to 4 threads complete or trap with the deadlock message`);

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
    const instrumented = join(temporary, `results-traps-${optimization}.wasm`);
    cli(["build", fixture, "--target", "wasm32", "--trap-info", `-O${optimization}`, "-o", instrumented]);
    const trapped = (await WebAssembly.instantiate(readFileSync(instrumented))).instance;
    assert.equal(trapped.exports.tz_parallel_results_lowest(), 2n);
    assert.equal(trapped.exports.tz_parallel_results_recursive(1), 5n);
    assert.throws(() => trapped.exports.tz_trap_parallel_results(), WebAssembly.RuntimeError);
    assert.notEqual(trapped.exports.tsuzuri_trap_site(), 0);
  }

  assert.equal(cli(["run", "examples/tasks", "-O0"]).stdout, "21325334000\n");
  assert.equal(cli(["run", "examples/tasks", "-O3", "--cpu", "native"]).stdout, "21325334000\n");

  // Each separately built native object carries a coalescible task runtime.
  const other = join(temporary, "other");
  mkdirSync(other);
  writeFileSync(join(other, "Other.tz"), `
export def other :: i64
fn other = { let values = Task.run (Task.parallel [task { 42 }]); values[0] }
test "other result" = assert (other() == 42)
`);
  const otherObject = join(other, "other.o");
  cli(["build", join(other, "Other.tz"), "--emit", "object", "-o", otherObject]);
  execute(clang, ["-std=c11", "-DOTHER", host, join(temporary, "tasks-3.o"), otherObject, "-pthread", "-lm",
    "-o", join(temporary, "combined")]);
  execute(join(temporary, "combined"), []);
  assert.equal(execute("nm", [join(temporary, "combined")]).stdout.split("\n").filter((line) => /\b[TtWw]\s+_?tsuzuri_task_parallel$/.test(line)).length, 1);
  const hooks = join(temporary, "pool-hooks.h"), wrapper = join(temporary, "clang-pool.mjs");
  writeFileSync(hooks, `#include <pthread.h>\nextern int count_pool_create(pthread_t *, const pthread_attr_t *, void *(*)(void *), void *);\n#define TZ_TASK_PTHREAD_CREATE count_pool_create\n#define TZ_TASK_SYSCONF(name) 4\n`);
  writeFileSync(wrapper, `#!${process.execPath}\nimport {spawnSync} from "node:child_process";\nconst args=process.argv.slice(2); if(args.includes("-std=c11")) args.push("-include", ${JSON.stringify(hooks)}); const result=spawnSync(${JSON.stringify(clang)},args,{stdio:"inherit"}); if(result.error) throw result.error; process.exit(result.status ?? 1);\n`);
  chmodSync(wrapper, 0o755);
  const countedFirst = join(temporary, "counted-first.o"), countedSecond = join(temporary, "counted-second.o");
  cli(["build", fixture, "--emit", "object", "-O3", "-o", countedFirst], true, { TSUZURI_CLANG: wrapper });
  writeFileSync(join(other, "Other.tz"), "export def other :: i64\nfn other = { let values = Task.run (Task.parallel [task { 20 }, task { 22 }]); values[0] + values[1] }");
  cli(["build", join(other, "Other.tz"), "--emit", "object", "-O3", "-o", countedSecond], true, { TSUZURI_CLANG: wrapper });
  const concurrentHost = join(temporary, "concurrent.c"), concurrent = join(temporary, "concurrent");
  writeFileSync(concurrentHost, `#include <assert.h>\n#include <pthread.h>\n#include <stdatomic.h>\n#include <stdint.h>\nstatic atomic_uint created;\nint count_pool_create(pthread_t *thread,const pthread_attr_t *attributes,void *(*run)(void *),void *context) { atomic_fetch_add(&created,1); return pthread_create(thread,attributes,run,context); }\nextern int64_t tz_parallel_sum(int64_t), tz_other(void);\nstatic pthread_mutex_t mutex=PTHREAD_MUTEX_INITIALIZER; static pthread_cond_t ready=PTHREAD_COND_INITIALIZER; static unsigned arrivals;\nstatic void *invoke(void *pointer) { uintptr_t which=(uintptr_t)pointer; assert(pthread_mutex_lock(&mutex)==0); if(++arrivals==8) assert(pthread_cond_broadcast(&ready)==0); while(arrivals<8) assert(pthread_cond_wait(&ready,&mutex)==0); assert(pthread_mutex_unlock(&mutex)==0); for(unsigned index=0;index<64;++index) { if(which&1) assert(tz_other()==42); else assert(tz_parallel_sum(257)==5625216); } return 0; }\nint main(void) { pthread_t callers[8]; for(uintptr_t index=0;index<8;++index) assert(pthread_create(&callers[index],0,invoke,(void *)index)==0); for(unsigned index=0;index<8;++index) assert(pthread_join(callers[index],0)==0); assert(atomic_load(&created)==3); return 0; }\n`);
  execute(clang, ["-std=c11", "-O3", "-Wall", "-Wextra", "-Werror", concurrentHost, countedFirst, countedSecond, "-pthread", "-lm", "-o", concurrent]);
  execute(concurrent, []);
  assert.equal(execute("nm", [concurrent]).stdout.split("\n").filter((line) => /\b[TtWw]\s+_?tsuzuri_task_parallel$/.test(line)).length, 1);
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
