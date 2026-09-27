import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const clang = process.env.TSUZURI_CLANG ?? "clang";
const root = mkdtempSync(join(tmpdir(), "tsuzuri-dispatch-"));
function execute(program, args, env = process.env, success = true) {
  const result = spawnSync(program, args, { encoding: "utf8", env, timeout: 180_000, maxBuffer: 16 * 1024 * 1024 });
  assert.ifError(result.error);
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}
try {
  const host = join(root, "host.c");
  writeFileSync(host, `#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <pthread.h>
extern int64_t tz_sum(const int64_t *, int64_t);
extern int64_t tz_task_sum(void);
extern uint64_t tsuzuri_cpu_features(void);
extern int tsuzuri_cpu_variant(void);
static void *run(void *unused) {
  (void)unused;
  int64_t values[4099];
  for (int index = 0; index < 4099; ++index) { uint64_t bits = UINT64_MAX - (uint64_t)index * 37; memcpy(&values[index], &bits, 8); }
  for (int count = 0; count <= 4097; count += count < 20 ? 1 : 37) {
    uint64_t total = 0; for (int index = 0; index < count; ++index) total += (uint64_t)values[index + 1];
    int64_t expected; memcpy(&expected, &total, 8);
    assert(tz_sum(values + 1, count) == expected);
  }
  assert(tz_task_sum() == 42);
  return NULL;
}
int main(void) { pthread_t threads[8]; for (int index = 0; index < 8; ++index) assert(pthread_create(&threads[index], NULL, run, NULL) == 0); for (int index = 0; index < 8; ++index) assert(pthread_join(threads[index], NULL) == 0); printf("%llu %d\\n", (unsigned long long)tsuzuri_cpu_features(), tsuzuri_cpu_variant()); return 0; }
`);
  for (const optimization of ["-O0", "-O3"]) for (const cpu of ["generic", "native"]) {
    const object = join(root, `${cpu}${optimization}.o`);
    execute(compiler, ["build", "tests/fixtures/cpu_dispatch", "--emit", "object", optimization, "--cpu", cpu, "-o", object]);
    const executable = join(root, `${cpu}${optimization}`);
    execute(clang, [host, object, optimization, "-pthread", "-lm", "-o", executable]);
    const [features, selected] = execute(executable, []).stdout.trim().split(" ").map(Number);
    assert.equal(selected, features & 2 ? 2 : features & 1 ? 1 : 0);
    assert.equal(execute(executable, [], { ...process.env, TSUZURI_CPU_FORCE: "baseline" }).stdout.trim().split(" ")[1], "0");
    for (const [variant, feature] of [["sse4.2", 1], ["avx2", 2], ["sve", 65536], ["unknown", 0]]) {
      const supported = feature !== 0 && (features & feature) !== 0;
      const result = execute(executable, [], { ...process.env, TSUZURI_CPU_FORCE: variant }, supported);
      if (supported) assert.equal(Number(result.stdout.trim().split(" ")[1]), feature);
      else { assert.notEqual(result.status, 0); assert.match(result.stderr, /requested variant is unavailable or unknown/); }
    }
    const program = join(root, `console-${cpu}${optimization}`);
    execute(compiler, ["build", "tests/fixtures/cpu_dispatch", optimization, "--cpu", cpu, "-o", program]);
    assert.equal(execute(program, []).stdout.trim(), "42");
    console.log(`dispatch: ${cpu} ${optimization} features=${features} selected=${selected}`);
  }
  const plain = join(root, "plain.ll");
  execute(compiler, ["build", "tests/fixtures/cpu_dispatch", "--target", "wasm32", "--emit", "llvm", "-o", plain]);
  assert.doesNotMatch(readFileSync(plain, "utf8"), /tsuzuri_cpu/);
  for (const optimization of ["-O0", "-O3"]) {
    const wasm = join(root, `sum${optimization}.wasm`);
    execute(compiler, ["build", "tests/fixtures/cpu_dispatch", "--target", "wasm32", optimization, "-o", wasm]);
    const module = new WebAssembly.Module(readFileSync(wasm));
    assert.deepEqual(WebAssembly.Module.imports(module), []);
    assert.equal(new WebAssembly.Instance(module).exports.tz_task_sum(), 42n);
  }
  const cpuIr = join(root, "cpu.ll");
  execute(clang, ["-std=c11", "-O3", "-S", "-emit-llvm", "src/runtime/cpu.c", "-o", cpuIr]);
  const text = readFileSync(cpuIr, "utf8");
  assert.match(text, /cmpxchg/);
  assert.match(text, /load atomic/);
  assert.doesNotMatch(text, /ifunc|__cpu_model/);
  if (process.platform === "darwin" && process.arch === "arm64") {
    const x86 = join(root, "cpu-x86.ll");
    execute(clang, ["-target", "x86_64-apple-macos11", "-std=c11", "-O3", "-S", "-emit-llvm", "src/runtime/cpu.c", "-o", x86]);
    const x86Text = readFileSync(x86, "utf8");
    assert.match(x86Text, /\+avx2/);
    assert.match(x86Text, /xgetbv/);
    assert.doesNotMatch(x86Text, /ifunc|__cpu_model/);
    console.log("dispatch: x86 variants cross-compiled; execution requires an x86 host");
  }
} finally { rmSync(root, { recursive: true, force: true }); }
