// PM05 Phase 2: --allocator small.
// Usage: node tests/small_allocator.mjs --unit
// Builds and runs tests/heap_small_test.c, the unit tests of src/runtime/heap-small.c, plain and with sanitizers.
import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const clang = process.env.TSUZURI_CLANG ?? "clang";
const windows = process.platform === "win32";
const suffix = windows ? ".exe" : "";
const threadFlags = windows ? [] : ["-pthread"];

function execute(program, programArguments) {
  const result = spawnSync(program, programArguments, { cwd: root, encoding: "utf8", timeout: 600_000, maxBuffer: 32 * 1024 * 1024 });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, `${program} ${programArguments.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}

const work = mkdtempSync(join(tmpdir(), "tsuzuri-small-allocator-"));
try {
  // The unit tests: every class, the boundaries, chains and flushes, a held lock, regions, resize, failing malloc, the
  // address limit, random operations against a model, and four threads that hand blocks to each other.
  const variants = [["O0", ["-O0"]], ["O3", ["-O3"]]];
  if (!windows) variants.push(["asan-ubsan", ["-O1", "-fsanitize=address,undefined", "-fno-sanitize-recover=all"]], ["tsan", ["-O1", "-fsanitize=thread"]]);
  for (const [name, flags] of variants) {
    const binary = join(work, `heap-small-${name}${suffix}`);
    execute(clang, ["-std=c11", "-g", "-fno-strict-aliasing", "-Wall", "-Wextra", ...flags, ...threadFlags, join(root, "tests/heap_small_test.c"), "-o", binary]);
    assert.match(execute(binary, []).stdout, /heap_small_test: ok/, name);
    console.log(`small allocator unit tests (${name}) passed`);
  }
} finally {
  rmSync(work, { recursive: true, force: true });
}
