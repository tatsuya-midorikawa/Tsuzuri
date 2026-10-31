// B10 Phase 2: failure domains end to end. `Domain.run` of the ticket's example prints `average 9` and
// `failed (kind 1)` (1 is integer division by zero: 27 / 3 = 9, and an empty array divides by its length 0),
// through `run` and through a built executable; `tsuzuri test` catches an out-of-range index (kind 3); and a C
// host of `--emit llvm --trap-mode return` alternates 10,000 successes and traps with no block left alive,
// with a domain inside its own boundary. Every case runs at -O0 and -O3.
// Usage: node tests/domain.mjs [path/to/tsuzuri]
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

if (process.platform === "win32") {
  console.log("domain: skipped (failure domains are native POSIX only; Windows builds report E1038)");
  process.exit(0);
}

const repository = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const clang = process.env.TSUZURI_CLANG ?? "clang";
const fixture = join(repository, "tests/fixtures/domain");
const host = join(repository, "tests/fixtures/domain_host");
const root = mkdtempSync(join(tmpdir(), "tsuzuri-domain-"));

function execute(program, args, success = true) {
  const result = spawnSync(program, args, { cwd: repository, encoding: "utf8", timeout: 300_000, maxBuffer: 8 * 1024 * 1024 });
  assert.ifError(result.error);
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}

const expected = "average 9\nfailed (kind 1)\n";
let cases = 0;
try {
  for (const optimization of ["-O0", "-O3"]) {
    assert.equal(execute(compiler, ["run", fixture, optimization, "--no-cache"]).stdout, expected); cases++;
    const executable = join(root, `domain${optimization}`);
    execute(compiler, ["build", fixture, optimization, "--no-cache", "-o", executable]);
    const ran = execute(executable, []);
    assert.equal(ran.stdout, expected);
    assert.equal(ran.stderr, ""); cases++;
    const tested = execute(compiler, ["test", fixture, optimization]);
    assert.match(tested.stdout + tested.stderr, /2 passed/); cases++;

    const ir = join(root, `host${optimization}.ll`);
    execute(compiler, ["build", host, "--emit", "llvm", "--trap-mode", "return", optimization, "--no-cache", "-o", ir]);
    for (const [name, flags] of [["plain", [optimization]], ["asan", ["-g", "-O1", "-fsanitize=address,undefined", "-fno-sanitize-recover=undefined"]]]) {
      const program = join(root, `host-${name}${optimization}`);
      execute(clang, ["-std=c11", ...flags, "-Wno-override-module", "-pthread", "tests/domain_host.c", ir, "-o", program]);
      assert.equal(execute(program, []).stdout, "domain host: ok, live 0\n"); cases++;
    }
  }
  console.log(`domain: ${cases} cases`);
} finally {
  rmSync(root, { recursive: true, force: true });
}
