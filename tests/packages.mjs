// E2E for git and registry dependencies (E10): `tsuzuri fetch` downloads commits
// of local bare repositories through file:/// urls (no network), writes
// Tsuzuri.lock, and every later build reads only the lockfile and the package
// store. `tsuzuri publish` prints the index entries of a local registry index.
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
function gitEnvironment() {
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
  return env;
}

function git(repository, args, input) {
  const result = spawnSync("git", ["-c", "init.defaultBranch=main", `--git-dir=${repository}`, ...args], { input, env: gitEnvironment() });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, `git ${args.join(" ")}\n${result.stderr}`);
  return result.stdout.toString().trim();
}

// Commits files (one directory level) on top of main of a bare repository without a
// working tree; returns the commit id.
function commit(repository, files) {
  mkdirSync(repository, { recursive: true });
  git(repository, ["init", "--bare", "--quiet"]);
  const tree = (entries) => git(repository, ["mktree", "-z"], entries.join(""));
  const top = [];
  const directories = {};
  for (const [path, text] of Object.entries(files)) {
    const line = (name) => `100644 blob ${git(repository, ["hash-object", "-w", "--stdin"], text)}\t${name}\0`;
    const [head, ...rest] = path.split("/");
    if (rest.length === 0) top.push(line(head));
    else (directories[head] ??= []).push(line(rest.join("/")));
  }
  for (const [name, entries] of Object.entries(directories)) top.push(`040000 tree ${tree(entries)}\t${name}\0`);
  const parent = spawnSync("git", [`--git-dir=${repository}`, "rev-parse", "--verify", "--quiet", "refs/heads/main"], { env: gitEnvironment() });
  const rev = git(repository, ["commit-tree", tree(top), "-m", "fixture", ...(parent.status === 0 ? ["-p", parent.stdout.toString().trim()] : [])]);
  git(repository, ["update-ref", "refs/heads/main", rev]);
  return rev;
}

function writeFiles(root, files) {
  for (const [path, text] of Object.entries(files)) {
    mkdirSync(dirname(join(root, path)), { recursive: true });
    writeFileSync(join(root, path), text);
  }
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

  // A registry: publish prints each version's index entry from a working copy that
  // matches its commit, the index repository lists the entries, and fetch selects
  // the minimal versions that satisfy every requirement.
  const index = join(directory, "repos", "index.git");
  const versions = {};
  const commitIndex = () => commit(index, Object.fromEntries(Object.entries(versions).map(([name, entries]) =>
    [`index/${name}.json`, `${JSON.stringify({ name, versions: entries }, null, 2)}\n`])));
  const registry = `\n[registry]\nindex = "${pathToFileURL(index).href}"\n`;
  function publish(name, version, files) {
    const repository = join(directory, "repos", `${name}.git`);
    const rev = commit(repository, files);
    const copy = join(directory, "copies", `${name}-${version}`);
    writeFiles(copy, files);
    // A package with registry dependencies fetches them before publish checks it.
    if (files["Tsuzuri.toml"].includes("[registry]")) cli(["fetch", copy]);
    const printed = cli(["publish", copy, "--git", pathToFileURL(repository).href, "--rev", rev]);
    const entry = JSON.parse(printed.stdout);
    assert.deepEqual([entry.version, entry.git, entry.rev], [version, pathToFileURL(repository).href, rev]);
    (versions[name] ??= []).push(entry);
  }
  const shapes = (version, unit) => ({
    "Tsuzuri.toml": `[package]\nname = "shapes"\nversion = "${version}"\n`,
    "Area.tz": `def unit :: i64\nfn unit = ${unit}\n`,
  });
  publish("shapes", "1.0.0", shapes("1.0.0", 7));
  publish("shapes", "1.1.0", shapes("1.1.0", 8));
  publish("shapes", "1.2.0", shapes("1.2.0", 9));
  commitIndex();
  publish("tiles", "1.0.0", {
    "Tsuzuri.toml": `[package]\nname = "tiles"\nversion = "1.0.0"\n[dependencies]\nshapes = { version = "1.1.0" }\n${registry}`,
    "Grid/Count.tz": "def count :: i64\nfn count = Shapes::Area.unit() * 3\n",
  });
  commitIndex();
  const store = join(directory, "registry-app");
  writeFiles(store, {
    "Tsuzuri.toml": `[package]\nname = "store"\nversion = "0.1.0"\n[dependencies]\nshapes = { version = "1.0.0" }\ntiles = { version = "1.0.0" }\n${registry}`,
    "Main.tz": "export def answer :: i64\nfn answer = Shapes::Area.unit() + Tiles::Grid::Count.count()\n\nanswer()\n",
  });
  cli(["fetch", store]);
  const registryLock = JSON.parse(readFileSync(join(store, "Tsuzuri.lock"), "utf8"));
  assert.equal(registryLock.format, 2);
  // shapes 1.0.0 (store) and 1.1.0 (tiles) select 1.1.0, not the newest 1.2.0.
  assert.deepEqual(registryLock.packages.map(entry => [entry.name, entry.version]), [["shapes", "1.1.0"], ["tiles", "1.0.0"]]);
  const registryExpected = 8n + 8n * 3n;
  for (const optimization of ["-O0", "-O3"]) {
    assert.equal(cli(["run", store, optimization]).stdout, `${registryExpected}\n`);
    const wasm = join(directory, `store${optimization}.wasm`);
    cli(["build", store, "--target", "wasm32", optimization, "-o", wasm]);
    const { module, instance } = await WebAssembly.instantiate(readFileSync(wasm));
    assert.deepEqual(WebAssembly.Module.imports(module), []);
    assert.equal(instance.exports.tz_answer(), registryExpected);
  }
  console.log("Packages: git and registry dependencies through file:/// repositories, publish, minimal version selection, Tsuzuri.lock, offline native/WASM O0/O3 builds, deterministic IR");
} finally {
  rmSync(directory, { recursive: true, force: true });
}
