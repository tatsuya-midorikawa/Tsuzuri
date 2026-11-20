// PB08: the options that `benchmarks/lsp/golden.mjs` refuses and the golden files that its `compare` refuses, so
// that a golden that ran nothing, or a file without entries, never compares equal to another (the first Copilot
// review of PR #122). No compiler runs: every refusal comes before the first `check` or language server.
//
// usage: node tests/benchmark_lsp_golden.mjs
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const golden = join(root, "benchmarks/lsp/golden.mjs");
const temporary = mkdtempSync(join(tmpdir(), "tsuzuri-lsp-golden-test-"));
const run = (...args) => spawnSync(process.execPath, [golden, ...args], { cwd: root, encoding: "utf8", timeout: 60_000 });
const test = (name, check) => {
  try {
    check();
    console.log(`golden: ${name} passed`);
  } catch (error) {
    process.exitCode = 1;
    console.log(`golden: ${name} FAILED\n${error.message}`);
  }
};
const refused = (child, pattern, what) => {
  assert.notEqual(child.status, 0, `${what} succeeded:\n${child.stdout}`);
  assert.match(child.stderr, pattern, `${what}:\n${child.stderr}`);
};

try {
  // No workload, so that `cli` has only the targets of the repository and `lsp` has no project; the compiler does
  // not exist, so a run that starts would fail with ENOENT instead of the refusal that the test expects.
  const workloads = join(temporary, "workloads");
  mkdirSync(workloads);
  writeFileSync(join(workloads, "workloads.json"), JSON.stringify({ projects: [] }) + "\n");
  const compiler = join(temporary, "no-compiler");
  let outputs = 0;

  test("cli refuses a --jobs that is not a positive safe integer", () => {
    for (const jobs of ["0", "-1", "abc", "1.5", "", "1e3", "9007199254740992"]) {
      const out = join(temporary, `jobs-${outputs++}.jsonl`);
      refused(run("cli", compiler, out, "--workloads", workloads, "--jobs", jobs), /--jobs/, `--jobs '${jobs}'`);
      assert.ok(!existsSync(out), `--jobs '${jobs}' wrote ${out}`);
    }
  });

  test("cli and lsp refuse a --threads that TSUZURI_THREADS does not take", () => {
    for (const threads of ["0", "-1", "abc", "1.5", "", "33"]) {
      const out = join(temporary, `threads-${outputs++}.jsonl`);
      refused(run("cli", compiler, out, "--workloads", workloads, "--threads", threads), /--threads/, `cli --threads '${threads}'`);
      assert.ok(!existsSync(out), `cli --threads '${threads}' wrote ${out}`);
      const directory = join(temporary, `threads-${outputs++}`);
      refused(run("lsp", compiler, directory, "--workloads", workloads, "--threads", threads), /--threads/, `lsp --threads '${threads}'`);
      assert.ok(!existsSync(directory), `lsp --threads '${threads}' wrote ${directory}`);
    }
  });

  test("lsp refuses a run without a project", () => {
    const directory = join(temporary, `projects-${outputs++}`);
    refused(run("lsp", compiler, directory, "--workloads", workloads), /no project/, "lsp without a project");
    assert.ok(!existsSync(directory), `lsp without a project wrote ${directory}`);
  });

  const entry = (target, stdout = "{}\n") => ({ target, status: 0, signal: null, stdout, stderr: "", kg: { "facts.json": "0".repeat(64) } });
  const lines = (...values) => values.map((value) => (typeof value === "string" ? value : JSON.stringify(value))).join("\n") + "\n";
  const pair = (name, left, right = left) => {
    const [a, b] = [join(temporary, `${name}-a.jsonl`), join(temporary, `${name}-b.jsonl`)];
    writeFileSync(a, left);
    writeFileSync(b, right);
    return run("compare", a, b);
  };

  test("compare matches equal CLI goldens and reports a difference", () => {
    const same = pair("same", lines(entry("examples/hello"), entry("tests/fixtures/json")));
    assert.equal(same.status, 0, same.stderr);
    assert.match(same.stdout, /"same":2,"differ":0/);
    const differ = pair("differ", lines(entry("examples/hello"), entry("tests/fixtures/json")),
      lines(entry("examples/hello"), entry("tests/fixtures/json", "{\"changed\":true}\n")));
    assert.equal(differ.status, 1, differ.stderr);
    assert.match(differ.stdout, /DIFF tests\/fixtures\/json stdout/);
    assert.match(differ.stdout, /"same":1,"differ":1/);
  });

  test("compare refuses CLI goldens with blank or missing entries", () => {
    const cases = [
      ["blank", "\n".repeat(442), /line 1 of .* is blank/],
      ["blank-inside", lines(entry("examples/hello"), "", entry("tests/fixtures/json")), /line 2 of .* is blank/],
      ["empty", "", /has no entries/],
      ["no-newline", JSON.stringify(entry("examples/hello")), /does not end with a newline/],
      ["fields", lines({ target: "examples/hello" }), /line 1 of .* is not an entry/],
      ["null", lines("null"), /line 1 of .* is not an entry/],
      ["kg", lines({ ...entry("examples/hello"), kg: null }), /line 1 of .* is not an entry/],
    ];
    for (const [name, text, pattern] of cases) refused(pair(name, text), pattern, `compare ${name}`);
  });

  const transcript = (log) => JSON.stringify({ project: "target/perf/PB08/projects/hello", log, publishes: [] }) + "\n";
  const directories = (name, files) => {
    const [a, b] = [join(temporary, `${name}-a`), join(temporary, `${name}-b`)];
    for (const directory of [a, b]) {
      mkdirSync(directory);
      for (const [file, text] of Object.entries(files)) writeFileSync(join(directory, file), text);
    }
    return run("compare", a, b);
  };

  test("compare matches equal transcripts and refuses directories without them", () => {
    const same = directories("transcripts", { "hello.json": transcript([["completion top", []]]), "hello.types.json": transcript([["signatures diagnostics", []]]) });
    assert.equal(same.status, 0, same.stderr);
    assert.match(same.stdout, /"same":2,"differ":0/);
    refused(directories("none", {}), /no transcripts/, "compare of empty directories");
    refused(directories("blank-transcript", { "hello.json": "" }), /hello\.json is not a transcript/, "compare of blank transcripts");
    refused(directories("empty-log", { "hello.json": transcript([]) }), /hello\.json is not a transcript/, "compare of transcripts without requests");
  });
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
