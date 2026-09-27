import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";

const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const quick = process.argv.includes("--quick");
const count = quick ? 10 : 200;
const repeats = quick ? 2 : 7;
const directory = mkdtempSync(join(tmpdir(), "tsuzuri-cache-bench-"));
const project = join(directory, "project");
const output = join(directory, process.platform === "win32" ? "program.exe" : "program");
const env = { ...process.env, TSUZURI_CACHE_DIR: join(directory, "cache") };
function execute(args) {
  const start = performance.now();
  const result = spawnSync(compiler, args, { encoding: "utf8", env, timeout: 180000 });
  assert.equal(result.status, 0, `${result.error ?? ""}\n${result.stderr}`);
  return performance.now() - start;
}
const median = values => values.sort((left, right) => left - right)[Math.floor(values.length / 2)];
try {
  mkdirSync(project);
  const main = ["let mut total = 0"];
  for (let index = 0; index < count; index++) {
    writeFileSync(join(project, `Module${index}.tz`), `def value :: i64\nfn value = ${index}\n`);
    main.push(`total = total + Module${index}.value()`);
  }
  main.push("total");
  writeFileSync(join(project, "Main.tz"), `${main.join("\n")}\n`);
  const args = ["build", project, "-o", output];
  execute(args);
  const expected = readFileSync(output);
  const uncached = [], warm = [];
  for (let index = 0; index < repeats; index++) {
    uncached.push(execute([...args, "--no-cache"]));
    warm.push(execute(args));
    assert.deepEqual(readFileSync(output), expected);
  }
  const result = spawnSync(output, [], { encoding: "utf8" });
  assert.equal(result.status, 0);
  assert.equal(result.stdout.trim(), String(count * (count - 1) / 2));
  console.log(JSON.stringify({ modules: count, repeats, uncached_median_ms: median(uncached), warm_median_ms: median(warm), quick, note: "End-to-end wall time includes parsing, checking, IR generation, fingerprints and publication; no speed threshold or general speed claim" }));
} finally {
  rmSync(directory, { recursive: true, force: true });
}
