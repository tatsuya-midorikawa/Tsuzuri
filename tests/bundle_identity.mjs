// The identity of a bundled toolchain is read once and remembered (PB03 WP-F): a fake distribution whose launcher
// `bin/tsuzuri-clang` is a spy around the real Clang. Usage: node tests/bundle_identity.mjs [compiler].
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';

if (process.platform === 'win32') {
  // The spy would be taken for the MinGW launcher; the bundled launcher itself is covered by the smoke test.
  console.log('bundle identity: skipped on Windows');
  process.exit(0);
}
const compiler = resolve(process.argv[2] ?? 'target/release/tsuzuri');
const scratch = resolve('target/bundle-identity-tests');
mkdirSync(scratch, { recursive: true });
const directory = mkdtempSync(join(scratch, 'run-'));
const cache = join(directory, 'cache'), log = join(directory, 'clang.log'), dist = join(directory, 'dist');
const clang = process.env.TSUZURI_CLANG ?? 'clang';
// The checks count the compiler calls of the one-module pipeline; codegen units are tested by codegen_units.mjs.
const env = { ...process.env, TSUZURI_CACHE_DIR: cache, PB03_LOG: log, TMPDIR: directory, TSUZURI_CODEGEN_UNITS: 'single',
  PB03_CLANG: /[\\/]/.test(clang) ? resolve(clang) : clang };
delete env.TSUZURI_ZIG;
delete env.TSUZURI_LLVM_CLANG;
const sleep = milliseconds => new Promise(done => setTimeout(done, milliseconds));
// A memo is not trusted while the newest time of the distribution is within two seconds of the moment it is written.
const settle = () => sleep(2400);
function run(program, args, environment = env) {
  const result = spawnSync(program, args, { env: environment, encoding: 'utf8', timeout: 180000 });
  assert.ifError(result.error);
  assert.equal(result.status, 0, `${program} ${args.join(' ')}\n${result.stdout}\n${result.stderr}`);
  return result;
}
const write = (file, text) => {
  mkdirSync(dirname(file), { recursive: true });
  writeFileSync(file, text);
};
const calls = () => existsSync(log) ? readFileSync(log, 'utf8').split('\n').filter(Boolean) : [];
const entries = root => existsSync(root) ? readdirSync(root).filter(name => /^[0-9a-f]{64}$/.test(name)) : [];
// Removes the whole-build entries and keeps the runtime objects.
const missBuilds = () => { for (const name of entries(cache)) rmSync(join(cache, name), { recursive: true, force: true }); };
const memos = () => existsSync(join(cache, 'bundle')) ? readdirSync(join(cache, 'bundle')).filter(name => name.endsWith('.json')) : [];
try {
  const wrapperSource = join(directory, 'launcher.rs'), launcher = join(dist, 'bin', 'tsuzuri-clang');
  mkdirSync(join(dist, 'bin'), { recursive: true });
  writeFileSync(wrapperSource, `
use std::{env, fs::OpenOptions, io::Write, process::{exit, Command}};
fn main() {
    let args: Vec<_> = env::args_os().skip(1).collect();
    let kind = if args.iter().any(|a| a == "--version") { "version" } else if args.iter().any(|a| a == "-dM") { "probe" } else { "compile" };
    let mut file = OpenOptions::new().create(true).append(true).open(env::var_os("PB03_LOG").unwrap()).unwrap();
    writeln!(file, "{kind} {:?}", args).unwrap();
    let status = Command::new(env::var_os("PB03_CLANG").unwrap()).args(args).status().unwrap();
    exit(status.code().unwrap_or(1));
}`);
  run(process.env.RUSTC ?? 'rustc', ['--edition=2024', '-O', wrapperSource, '-o', launcher]);
  for (const [file, text] of Object.entries({
    'bin/clang': 'clang one', 'zig/zig': 'zig one', 'lib/libfake.a': 'library one',
    'include/fake.h': '// header one', 'share/doc/deep/readme.txt': 'deep one', 'manifest.json': '{"id":"one"}',
  })) write(join(dist, file), text);
  env.TSUZURI_CLANG = launcher;
  const project = join(directory, 'project');
  // A float's text comes from the numeric runtime object; an integer's is written by the module itself.
  write(join(project, 'Main.tz'), '42.5\n');
  const output = join(directory, 'app');
  const build = (environment = env) => {
    const start = calls().length;
    run(compiler, ['build', project, '-O0', '-o', output], environment);
    assert.equal(run(output, []).stdout.trim(), '42.5');
    const lines = calls().slice(start);
    return {
      version: lines.filter(line => line.startsWith('version ')).length,
      numeric: lines.filter(line => line.includes('"numeric.ll"')).length,
      compiles: lines.filter(line => line.startsWith('compile ')).length,
    };
  };
  await settle();

  // The first build reads the distribution, compiles the runtime objects and remembers the identity.
  assert.deepEqual(build(), { version: 1, numeric: 1, compiles: 2 }, 'cold');
  assert.equal(memos().length, 1);
  // The runtime objects hit and the identity comes from the memo: nothing but the user's module is compiled.
  missBuilds();
  assert.deepEqual(build(), { version: 0, numeric: 0, compiles: 1 }, 'warm runtime, remembered identity');
  assert.deepEqual(build(), { version: 0, numeric: 0, compiles: 0 }, 'whole-build hit runs no tool');

  // A file added to a checked directory invalidates the identity, the runtime objects and the build.
  write(join(dist, 'include', 'added.h'), '// header two');
  assert.deepEqual(build(), { version: 1, numeric: 1, compiles: 2 }, 'a file was added');
  await settle();
  assert.deepEqual(build(), { version: 1, numeric: 0, compiles: 0 }, 'remembered again');
  assert.deepEqual(build(), { version: 0, numeric: 0, compiles: 0 });

  // A tool that is replaced does too, and so does a rewritten manifest.
  write(join(dist, 'bin', 'clang'), 'clang two, longer');
  assert.deepEqual(build(), { version: 1, numeric: 1, compiles: 2 }, 'Clang was replaced');
  await settle();
  build();
  write(join(dist, 'manifest.json'), '{"id":"two, longer"}');
  assert.equal(build().version, 1, 'a new manifest');
  await settle();
  build();
  assert.deepEqual(build(), { version: 0, numeric: 0, compiles: 0 });

  // An override sends the launcher elsewhere, so the identity is read in full on every build.
  const overridden = { ...env, TSUZURI_ZIG: join(dist, 'zig', 'zig') };
  assert.equal(build(overridden).version, 1, 'TSUZURI_ZIG');
  assert.equal(build(overridden).version, 1, 'TSUZURI_ZIG is probed on every build');
  assert.deepEqual(build(), { version: 0, numeric: 0, compiles: 0 }, 'the remembered identity of the plain layout is still valid');

  // A directory that is not a cache is not written to.
  const foreign = join(directory, 'foreign');
  write(join(foreign, 'keep.txt'), 'mine');
  const result = run(compiler, ['build', project, '-O0', '-o', output], { ...env, TSUZURI_CACHE_DIR: foreign });
  assert.match(result.stderr, /cache disabled/);
  assert.deepEqual(readdirSync(foreign), ['keep.txt']);
  console.log('bundle identity: ok');
} finally {
  rmSync(directory, { recursive: true, force: true });
}
