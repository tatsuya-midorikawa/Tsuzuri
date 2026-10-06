import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync, rmSync, mkdirSync, copyFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

// Sums the same shapes through dyn values, a union match, and monomorphized generic code (A14).
const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const quick = process.argv.includes("--quick");
const objdump = process.env.TSUZURI_OBJDUMP ?? "/opt/homebrew/opt/llvm@21/bin/llvm-objdump";
const root = mkdtempSync(join(tmpdir(), "tsuzuri-dyn-benchmark-"));
const names = ["dyn", "union", "generic"];
function execute(program, args) {
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 300_000, maxBuffer: 1 << 28 });
  assert.ifError(result.error);
  assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result.stdout;
}
try {
  const object = join(root, "all.o");
  execute(compiler, ["build", "benchmarks/dyn", "--emit", "object", "-O3", "-o", object]);
  // Code size: one object per version, each exporting only its own function.
  const source = readFileSync("benchmarks/dyn/Main.tz", "utf8");
  const bytes = {};
  for (const name of names) {
    const project = join(root, name);
    mkdirSync(project);
    copyFileSync("benchmarks/dyn/Shapes.tt", join(project, "Shapes.tt"));
    const only = names.filter((other) => other !== name)
      .reduce((text, other) => text.replace(`export def ${other}_total`, `def ${other}_total`), source);
    writeFileSync(join(project, "Main.tz"), only);
    const single = join(root, `${name}.o`);
    execute(compiler, ["build", project, "--emit", "object", "-O3", "-o", single]);
    bytes[name] = execute(objdump, ["-d", single]).length;
  }
  const host = join(root, "host.c");
  writeFileSync(host, `#define _POSIX_C_SOURCE 200809L
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <time.h>
extern int64_t tz_dyn_total(int64_t, int64_t);
extern int64_t tz_union_total(int64_t, int64_t);
extern int64_t tz_generic_total(int64_t, int64_t);
static double now(void) { struct timespec time; assert(clock_gettime(CLOCK_MONOTONIC, &time) == 0); return (double)time.tv_sec + (double)time.tv_nsec * 1e-9; }
int main(void) {
  const int64_t count = ${quick ? 300 : 3000}; const int64_t rounds = ${quick ? 10 : 2000};
  int64_t (*functions[])(int64_t, int64_t) = {tz_dyn_total, tz_union_total, tz_generic_total};
  const char *names[] = {"dyn", "union", "generic"};
  int64_t expected = functions[0](count, rounds); double samples[3][9];
  for (int index = 1; index < 3; ++index) assert(functions[index](count, rounds) == expected);
  for (int sample = 0; sample < 9; ++sample) for (int offset = 0; offset < 3; ++offset) {
    int implementation = (sample + offset) % 3; double start = now();
    assert(functions[implementation](count, rounds) == expected);
    samples[implementation][sample] = (now() - start) * 1000.0;
  }
  for (int implementation = 0; implementation < 3; ++implementation) {
    for (int outer = 1; outer < 9; ++outer) for (int inner = outer; inner > 0 && samples[implementation][inner] < samples[implementation][inner - 1]; --inner) { double value = samples[implementation][inner]; samples[implementation][inner] = samples[implementation][inner - 1]; samples[implementation][inner - 1] = value; }
    printf("{\\"implementation\\":\\"%s\\",\\"milliseconds\\":%.6f,\\"min\\":%.6f,\\"max\\":%.6f,\\"count\\":%lld,\\"rounds\\":%lld,\\"checksum\\":%lld}\\n", names[implementation], samples[implementation][4], samples[implementation][0], samples[implementation][8], (long long)count, (long long)rounds, (long long)expected);
  }
  return 0;
}
`);
  const executable = join(root, "benchmark");
  execute(process.env.TSUZURI_CLANG ?? "clang", ["-O3", "-std=c11", "-fno-lto", host, object, "-lm", "-o", executable]);
  for (const line of execute(executable, []).trim().split("\n")) {
    const row = JSON.parse(line);
    console.log(JSON.stringify({ ...row, disassemblyBytes: bytes[row.implementation], quick }));
  }
} finally { rmSync(root, { recursive: true, force: true }); }
