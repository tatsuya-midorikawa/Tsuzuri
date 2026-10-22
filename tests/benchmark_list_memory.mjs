import assert from "node:assert/strict";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArguments, runListMemory, stageFixture } from "../benchmarks/run-list-memory.mjs";
import { consChunks, reference, repeats, sizes, workloads } from "../benchmarks/list-memory/reference.mjs";

// The quick run below needs the compiler. A missing argument or a path that does not exist fails, so that a CI step that lost its path
// cannot pass by skipping the run.
assert.ok(process.argv[2], "usage: node tests/benchmark_list_memory.mjs <compiler>");
const compiler = resolve(process.argv[2]);
assert.ok(existsSync(compiler), `the compiler ${compiler} does not exist`);

// PM04: the checksums of benchmarks/list-memory come from closed forms; they must equal the sum that the kernels compute, term by term.
for (const name of workloads) {
  for (const n of [0, 1, 2, 3, 10, 1000]) {
    let total = 0n;
    for (let i = 0n; i < BigInt(n); ++i) {
      total += name === "clone_list" ? 2n * i : name === "walk_list" || name === "fold_list" ? 16n * i
        : name === "escape_list" ? 32n * i + 496n : name === "string_list" ? 3n : i;
    }
    // The comparison counts the 16 times that two equal lists are equal, however long they are.
    if (name === "compare_list") total = 16n;
    assert.equal(reference(name, n), total, `${name}(${n})`);
  }
}
assert.throws(() => reference("missing", 1), /unknown list-memory workload/);
assert.equal(workloads.length, 12);
for (const grid of Object.values(sizes)) assert.ok(grid.every((size, index) => Number.isSafeInteger(size) && size > 0 && (index === 0 || size > grid[index - 1])));
assert.ok(repeats("bulk_new", 1, false) > repeats("bulk_new", 1000000, false), "small sizes repeat more");
assert.ok(repeats("index_walk", 10000, false) <= 3, "the quadratic size runs few times");

const options = parseArguments(["--quick", "--only", "bulk_new,cons_build", "--wasm", "--opt", "O0"]);
assert.equal(options.quick, true);
assert.equal(options.samples, 1);
assert.equal(options.opt, "O0");
assert.equal(parseArguments([]).opt, "O3");
assert.deepEqual(options.only, ["bulk_new", "cons_build"]);
assert.equal(parseArguments(["--samples", "9", "/some/compiler", "--baseline", "/older"]).baseline, resolve("/older"));
for (const bad of [["--samples", "0"], ["--only", "missing"], ["--unknown"], ["a", "b"], ["--baseline"], ["--opt", "O2"], ["--samples", "3", "--metrics", "somewhere"]]) {
  assert.throws(() => parseArguments(bad), /Usage:|unknown|invalid|missing|unexpected/, bad.join(" "));
}

// A baseline from before List.fold_ref became List.foldr rejects the fixture, so it builds a copy that differs only in that name; a compiler
// that accepts the fixture builds it as it is. The stub rejects what such an older compiler rejects.
const staging = mkdtempSync(join(tmpdir(), "tsuzuri-list-memory-test-"));
try {
  const fixture = fileURLToPath(new URL("../benchmarks/list-memory", import.meta.url));
  assert.equal(stageFixture(compiler, "default", staging), fixture, "the compiler that knows List.foldr builds the fixture itself");
  const older = join(staging, "older-tsuzuri");
  writeFileSync(older, "#!/bin/sh\n! grep -q 'List.foldr' \"$2/Main.tz\"\n", { mode: 0o755 });
  const staged = stageFixture(older, "before", staging);
  assert.notEqual(staged, fixture);
  const earlier = readFileSync(join(staged, "Main.tz"), "utf8");
  assert.ok(earlier.includes("List.fold_ref") && !/\bList\.foldr\b/.test(earlier), "the old spelling");
  assert.equal(earlier.replace(/List\.fold_ref/g, "List.foldr"), readFileSync(join(fixture, "Main.tz"), "utf8"), "only the name differs");
  assert.equal(readFileSync(join(staged, "host.c"), "utf8"), readFileSync(join(fixture, "host.c"), "utf8"), "the host is copied");
} finally {
  rmSync(staging, { recursive: true, force: true });
}

// The quick run builds the fixture, checks every checksum, counts allocations (all freed) and reads the wasm32 heap.
const { results, wasm } = runListMemory(parseArguments([compiler, "--quick", "--wasm"]));
for (const { workload, size, variants } of results) {
  const { allocations, wall_ms } = variants.default;
  // `escape_list` builds and drops its lists in one function, and LLVM may remove the allocation of each (it does with Clang 21).
  assert.ok(allocations.calls >= 1 || size === 0 || workload === "escape_list", `${workload}(${size}) allocates`);
  // A list that is built at once is one chunk; `List.cons` adds one per step of the growth rule.
  if (workload === "cons_build") assert.equal(allocations.calls, consChunks(size), `${workload}(${size}) chunks`);
  if (workload === "compare_list") assert.equal(allocations.calls, consChunks(size) + (size === 0 ? 0 : 1), `${workload}(${size}) chunks`);
  if (["list_to_array", "array_to_list"].includes(workload)) assert.equal(allocations.calls, 2, `${workload}(${size}) is a chunk and an array`);
  if (workload === "escape_list") assert.ok(allocations.calls <= size, `${workload}(${size}) moves at most one chunk per call to the heap`);
  if (["bulk_new", "tail_consume", "index_walk", "walk_list", "fold_list"].includes(workload)) assert.equal(allocations.calls, 1, `${workload}(${size}) is one chunk`);
  // A run that LLVM folds into a constant (`escape_list` of 1) can read 0 on a coarse clock, so a time is a number that is not negative.
  assert.ok(wall_ms.length === 1 && Number.isFinite(wall_ms[0]) && wall_ms[0] >= 0, `${workload}(${size}) is timed`);
}
assert.equal(wasm.default.rows.length, workloads.length);
console.log("list-memory benchmark: references, arguments and a quick run verified");
