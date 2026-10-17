import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { resolve } from "node:path";
import { parseArguments, runListMemory } from "../benchmarks/run-list-memory.mjs";
import { consChunks, reference, repeats, sizes, workloads } from "../benchmarks/list-memory/reference.mjs";

// PM04: the checksums of benchmarks/list-memory come from closed forms; they must equal the sum that the kernels compute, term by term.
for (const name of workloads) {
  for (const n of [0, 1, 2, 3, 10, 1000]) {
    let total = 0n;
    for (let i = 0n; i < BigInt(n); ++i) total += name === "clone_list" ? 2n * i : name === "walk_list" || name === "fold_list" ? 16n * i : name === "string_list" ? 3n : i;
    // The comparison counts the 16 times that two equal lists are equal, however long they are.
    if (name === "compare_list") total = 16n;
    assert.equal(reference(name, n), total, `${name}(${n})`);
  }
}
assert.throws(() => reference("missing", 1), /unknown list-memory workload/);
assert.equal(workloads.length, 9);
for (const grid of Object.values(sizes)) assert.ok(grid.every((size, index) => Number.isSafeInteger(size) && size > 0 && (index === 0 || size > grid[index - 1])));
assert.ok(repeats("bulk_new", 1, false) > repeats("bulk_new", 1000000, false), "small sizes repeat more");
assert.ok(repeats("index_walk", 10000, false) <= 3, "the quadratic size runs few times");

const options = parseArguments(["--quick", "--only", "bulk_new,cons_build", "--wasm"]);
assert.equal(options.quick, true);
assert.equal(options.samples, 1);
assert.deepEqual(options.only, ["bulk_new", "cons_build"]);
assert.equal(parseArguments(["--samples", "9", "/some/compiler", "--baseline", "/older"]).baseline, resolve("/older"));
for (const bad of [["--samples", "0"], ["--only", "missing"], ["--unknown"], ["a", "b"], ["--baseline"], ["--samples", "3", "--metrics", "somewhere"]]) {
  assert.throws(() => parseArguments(bad), /Usage:|unknown|invalid|missing|unexpected/, bad.join(" "));
}

// With a compiler, the quick run builds the fixture, checks every checksum, counts allocations (all freed) and reads the wasm32 heap.
const compiler = process.argv[2] ? resolve(process.argv[2]) : null;
if (compiler && existsSync(compiler)) {
  const { results, wasm } = runListMemory(parseArguments([compiler, "--quick", "--wasm"]));
  for (const { workload, size, variants } of results) {
    const { allocations, wall_ms } = variants.default;
    assert.ok(allocations.calls >= 1 || size === 0, `${workload}(${size}) allocates`);
    // A list that is built at once is one chunk; `List.cons` adds one per step of the growth rule.
    if (workload === "cons_build") assert.equal(allocations.calls, consChunks(size), `${workload}(${size}) chunks`);
    if (workload === "compare_list") assert.equal(allocations.calls, consChunks(size) + (size === 0 ? 0 : 1), `${workload}(${size}) chunks`);
    if (["bulk_new", "tail_consume", "index_walk", "walk_list", "fold_list"].includes(workload)) assert.equal(allocations.calls, 1, `${workload}(${size}) is one chunk`);
    assert.ok(wall_ms.length === 1 && wall_ms[0] > 0, `${workload}(${size}) is timed`);
  }
  assert.equal(wasm.default.rows.length, workloads.length);
  console.log("list-memory benchmark: references, arguments and a quick run verified");
} else {
  console.log("list-memory benchmark: references and arguments verified (no compiler given for the quick run)");
}
