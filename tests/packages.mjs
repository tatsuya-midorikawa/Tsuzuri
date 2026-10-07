// E2E for git dependencies (E10): `tsuzuri fetch` downloads commits of local
// bare repositories through file:/// urls (no network), writes Tsuzuri.lock, and
// every later build reads only the lockfile and the package store.
import assert from "node:assert/strict";
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(process.argv[2] ?? join(root, "target/release/tsuzuri"));
const directory = join(root, "target", "tmp", `packages-e2e-${process.pid}`);
const cache = join(directory, "cache");
const gitConfig = join(directory, "gitconfig");
const exe = process.platform === "win32" ? ".exe" : "";

function run(program, args, { input, env = {}, success = true } = {}) {
  const result = spawnSync(program, args, {
    input,
    env: { ...process.env, ...env },
    encoding: "utf8",
    timeout: 180_000,
    maxBuffer: 16 * 1024 * 1024,
  });
  if (result.error) throw result.error;
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}

const cli = (args, success = true) => run(compiler, args, { env: { TSUZURI_CACHE_DIR: cache }, success });

// Fixture git commands ignore the user's configuration and always name their repository.
function git(repository, args, input) {
  const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !key.toUpperCase().startsWith("GIT_")));
  Object.assign(env, {
    GIT_CONFIG_NOSYSTEM: "1",
    GIT_CONFIG_GLOBAL: gitConfig,
    GIT_AUTHOR_NAME: "Tsuzuri Test",
    GIT_AUTHOR_EMAIL: "test@example.org",
    GIT_AUTHOR_DATE: "2026-01-01T00:00:00Z",
    GIT_COMMITTER_NAME: "Tsuzuri Test",
    GIT_COMMITTER_EMAIL: "test@example.org",
    GIT_COMMITTER_DATE: "2026-01-01T00:00:00Z",
  });
  const result = spawnSync("git", ["-c", "init.defaultBranch=main", `--git-dir=${repository}`, ...args], { input, env });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, `git ${args.join(" ")}\n${result.stderr}`);
  return result.stdout.toString().trim();
}

// Commits flat files to a bare repository without a working tree; returns the commit id.
function commit(repository, files) {
  mkdirSync(repository, { recursive: true });
  git(repository, ["init", "--bare", "--quiet"]);
  const entries = Object.entries(files)
    .map(([path, text]) => `100644 blob ${git(repository, ["hash-object", "-w", "--stdin"], text)}\t${path}\0`)
    .join("");
  const rev = git(repository, ["commit-tree", git(repository, ["mktree", "-z"], entries), "-m", "fixture"]);
  git(repository, ["update-ref", "refs/heads/main", rev]);
  return rev;
}

const gitDependency = (name, url, rev) => `${name} = { git = "${url}", rev = "${rev}" }\n`;

rmSync(directory, { recursive: true, force: true });
mkdirSync(directory, { recursive: true });
writeFileSync(gitConfig, "");
try {
  const geometry = join(directory, "repos", "geometry-core.git");
  const geometryRev = commit(geometry, {
    "Tsuzuri.toml": '[package]\nname = "geometry-core"\nversion = "0.1.0"\n',
    "Point.tz": "def value :: i64\nfn value = Maybe.get (Maybe.Some 42)\n",
    "Main.tz": "def main :: i64\nfn main = 99\n",
  });
  const other = join(directory, "repos", "other.git");
  const otherRev = commit(other, {
    "Tsuzuri.toml": `[package]\nname = "other"\nversion = "0.1.0"\n[dependencies]\n${gitDependency("geometry-core", pathToFileURL(geometry).href, geometryRev)}`,
    "Library.tz": "def value :: i64\nfn value = GeometryCore::Main.main() - 99\n",
  });
  const app = join(directory, "app");
  mkdirSync(app);
  writeFileSync(join(app, "Tsuzuri.toml"), `[package]\nname = "app"\nversion = "0.1.0"\n[dependencies]\n${gitDependency("geometry-core", pathToFileURL(geometry).href, geometryRev)}${gitDependency("other", pathToFileURL(other).href, otherRev)}`);
  writeFileSync(join(app, "Main.tz"), "export def answer :: i64\nfn answer = GeometryCore::Point.value() + Other::Library.value()\n\nanswer()\n");
  // The value of Point.value() plus Library.value(), computed here independently.
  const expected = 42n + (99n - 99n);

  const fetched = cli(["fetch", app]);
  assert.equal(fetched.stdout + fetched.stderr, "");
  const lock = JSON.parse(readFileSync(join(app, "Tsuzuri.lock"), "utf8"));
  assert.equal(lock.format, 1);
  assert.deepEqual(lock.packages.map(entry => [entry.name, entry.git, entry.rev]), [
    ["geometry-core", pathToFileURL(geometry).href, geometryRev],
    ["other", pathToFileURL(other).href, otherRev],
  ]);

  const first = join(directory, "first.ll");
  const second = join(directory, "second.ll");
  cli(["build", app, "--emit", "llvm", "-o", first]);
  cli(["build", app, "--emit", "llvm", "--no-cache", "-o", second]);
  assert.deepEqual(readFileSync(first), readFileSync(second));
  for (const optimization of ["-O0", "-O3"]) {
    assert.equal(cli(["run", app, optimization]).stdout, `${expected}\n`);
    assert.equal(cli(["run", app, optimization, "--no-cache"]).stdout, `${expected}\n`);
    const native = join(directory, `app${optimization}${exe}`);
    cli(["build", app, optimization, "-o", native]);
    assert.equal(run(native, []).stdout, `${expected}\n`);
    const wasm = join(directory, `app${optimization}.wasm`);
    cli(["build", app, "--target", "wasm32", optimization, "-o", wasm]);
    const { module, instance } = await WebAssembly.instantiate(readFileSync(wasm));
    assert.deepEqual(WebAssembly.Module.imports(module), []);
    assert.equal(instance.exports.tz_answer(), expected);
  }

  rmSync(join(app, "Tsuzuri.lock"));
  const missing = cli(["build", app, "--emit", "llvm", "-o", first], false);
  assert.equal(missing.status, 1);
  assert.match(missing.stderr, /error\[E2007\]: Tsuzuri\.lock is missing; run tsuzuri fetch/);
  cli(["fetch", app]);
  assert.equal(cli(["run", app]).stdout, `${expected}\n`);
  console.log("Packages: git dependencies through file:/// repositories, Tsuzuri.lock, offline native/WASM O0/O3 builds, deterministic IR");
} finally {
  rmSync(directory, { recursive: true, force: true });
}
