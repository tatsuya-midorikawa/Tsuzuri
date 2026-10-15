import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

const compiler = resolve(process.argv[2] ?? 'target/release/tsuzuri');
const scratch = resolve('target/prebuilt-runtime-tests');
mkdirSync(scratch, { recursive: true });
const directory = mkdtempSync(join(scratch, 'run-'));
const suffix = process.platform === 'win32' ? '.exe' : '';
const cache = join(directory, 'cache'), log = join(directory, 'clang.log');
const clang = process.env.TSUZURI_CLANG ?? 'clang';
const env = { ...process.env, TSUZURI_CACHE_DIR: cache, PB01_LOG: log,
  PB01_CLANG: /[\\/]/.test(clang) ? resolve(clang) : clang, TMPDIR: directory, TMP: directory, TEMP: directory };
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
function run(program, args, environment = env) {
  const result = spawnSync(program, args, { env: environment, encoding: 'utf8', timeout: 180000 });
  assert.ifError(result.error);
  assert.equal(result.status, 0, `${program} ${args.join(' ')}\n${result.stdout}\n${result.stderr}`);
  return result;
}
const entries = root => existsSync(root) ? readdirSync(root).filter(name => /^[0-9a-f]{64}$/.test(name)) : [];
function miss() {
  for (const name of entries(cache)) rmSync(join(cache, name), { recursive: true, force: true });
}
function calls() {
  return existsSync(log) ? readFileSync(log, 'utf8').trim().split('\n').filter(Boolean) : [];
}
function snapshot(root) {
  const files = {};
  function walk(directory, prefix = '') {
    for (const name of readdirSync(directory, { withFileTypes: true })) {
      const path = join(directory, name.name), label = `${prefix}/${name.name}`;
      if (name.isDirectory()) walk(path, label);
      else files[label] = hash(readFileSync(path));
    }
  }
  if (existsSync(root)) walk(root);
  return files;
}
try {
  // A native Rust shim works with MSVC too; no shell or POSIX executable-script assumption.
  const wrapperSource = join(directory, 'clang.rs'), wrapper = join(directory, `clang-spy${suffix}`);
  writeFileSync(wrapperSource, `
use std::{env,fs::OpenOptions,io::Write,path::Path,process::{Command,exit}};
fn main() {
 let mut args:Vec<_>=env::args_os().skip(1).collect();
 let compiler=env::var_os("PB01_CLANG").unwrap();
 // The spy hides the launcher name: preserve its MinGW executable-link convention.
 if cfg!(windows) && Path::new(&compiler).file_stem().is_some_and(|n| n=="tsuzuri-clang") {
  for arg in &mut args { if arg=="-Wl,/OPT:REF" { *arg="-Wl,--gc-sections".into(); } }
 }
 if !args.iter().any(|a| a=="--version" || a=="-dM") {
  let mut f=OpenOptions::new().create(true).append(true).open(env::var_os("PB01_LOG").unwrap()).unwrap();
  writeln!(f,"{:?}",args).unwrap();
 }
 let status=Command::new(compiler).args(args).status().unwrap();
 exit(status.code().unwrap_or(1));
}`);
  run(process.env.RUSTC ?? 'rustc', ['--edition=2024', '-O', wrapperSource, '-o', wrapper]);
  env.TSUZURI_CLANG = wrapper;
  const project = join(directory, 'project');
  mkdirSync(project);
  const source = join(project, 'Main.tz');
  writeFileSync(source, 'export def value :: i64 = (to_string 42).length\n'
    + 'export def sine :: f64 -> f64 = \\x -> Math.sin x + Math.sqrt x\n'
    + 'export def math_checks :: bool =\n'
    + '    Math.cos 0.0 == 1.0 && Math.pow 2.0 3.0 == 8.0 && Math.log 1.0 == 0.0\n'
    + '    && Math.exp 0.0 == 1.0 && Math.sin 0.0 == 0.0\n'
    + '    && Math.is_nan (Math.sqrt (-1.0)) && to_string (Math.sqrt (-0.0)) == "-0"\n'
    + 'assert (math_checks())\n42\n');
  let victim;
  for (const optimization of ['-O0', '-O3']) {
    for (const target of ['native', 'wasm32', 'wasm32-simd']) {
      const output = join(directory, target === 'native' ? `app${suffix}` : 'app.wasm');
      const args = ['build', project, optimization, '--target', target === 'native' ? 'native' : 'wasm32',
        ...(target === 'wasm32-simd' ? ['--wasm-feature', 'simd128'] : []), '-o', output];
      const start = calls().length;
      run(compiler, args);
      if (target === 'native' && optimization === '-O0') victim = entries(join(cache, 'runtime'))[0];
      const cold = calls().slice(start);
      assert.equal(cold.filter(line => line.includes('"numeric.ll"')).length, 1);
      assert.equal(cold.filter(line => line.includes('"math.ll"')).length, 1);
      miss();
      const warmStart = calls().length;
      run(compiler, args);
      assert.equal(calls().length - warmStart, 1, 'warm runtime only compiles/links user IR');
      if (target === 'native') {
        assert.equal(run(output, []).stdout.replaceAll('\r\n', '\n'), '42\n');
        if (process.platform !== 'win32') {
          const symbols = run('nm', ['-g', output]).stdout;
          for (const name of ['value', 'sine', 'math_checks']) {
            assert.match(symbols, new RegExp(`\\b_?tz_${name}\\b`), 'dead stripping must retain unused public exports');
          }
        }
      }
      else {
        const module = new WebAssembly.Module(readFileSync(output));
        assert.deepEqual(WebAssembly.Module.imports(module), []);
        const instance = new WebAssembly.Instance(module);
        assert.equal(instance.exports.tz_value(), 2n);
        assert.equal(instance.exports.tz_math_checks(), 1);
        assert.ok(Math.abs(instance.exports.tz_sine(1) - (Math.sin(1) + 1)) < 1e-15);
      }
      const oldCache = snapshot(cache);
      const independent = join(directory, `independent-${target}${target === 'native' ? suffix : '.wasm'}`);
      run(compiler, [...args.slice(0, -1), independent, '--no-cache']);
      assert.deepEqual(snapshot(cache), oldCache, '--no-cache must not read/update any cache');
      if (target === 'native') assert.equal(run(independent, []).stdout.replaceAll('\r\n', '\n'), '42\n');
      else {
        const module = new WebAssembly.Module(readFileSync(independent));
        assert.deepEqual(WebAssembly.Module.exports(module),
          WebAssembly.Module.exports(new WebAssembly.Module(readFileSync(output))));
        const instance = new WebAssembly.Instance(module);
        assert.equal(instance.exports.tz_value(), 2n);
        assert.equal(instance.exports.tz_math_checks(), 1);
        assert.equal(instance.exports.tz_sine(1), new WebAssembly.Instance(new WebAssembly.Module(readFileSync(output))).exports.tz_sine(1));
      }
    }
  }
  const abi = join(directory, 'abi');
  mkdirSync(abi);
  writeFileSync(join(abi, 'Main.tz'), 'export def value :: f64 -> f64 = \\x -> Math.sin x + Math.sqrt x\n');
  for (const emit of ['object', 'header']) {
    const output = join(directory, `cached.${emit}`), independent = join(directory, `independent.${emit}`);
    run(compiler, ['build', abi, '-O0', '--emit', emit, '-o', output]);
    run(compiler, ['build', abi, '-O0', '--emit', emit, '-o', independent, '--no-cache']);
    assert.deepEqual(readFileSync(output), readFileSync(independent),
      `${emit} output must stay independent of runtime object selection`);
  }
  for (const optimization of ['-O0', '-O3']) {
    const output = join(directory, `tasks${suffix}`);
    const args = ['build', resolve('examples/tasks'), optimization, '-o', output];
    const start = calls().length;
    run(compiler, args);
    assert.equal(calls().slice(start).filter(line => line.includes('"task.c"')).length, 1);
    miss();
    const warm = calls().length;
    run(compiler, args);
    assert.equal(calls().length - warm, 1);
    assert.equal(run(output, []).stdout.replaceAll('\r\n', '\n'), '21325334000\n');
  }
  miss();
  const invalidated = join(directory, `invalidation${suffix}`);
  const priorCalls = calls().length;
  run(compiler, ['build', project, '-O0', '-o', invalidated],
    { ...env, SOURCE_DATE_EPOCH: process.env.SOURCE_DATE_EPOCH === '1' ? '2' : '1' });
  assert.equal(calls().slice(priorCalls).filter(line => line.includes('"numeric.ll"')).length, 1,
    'SDK/build environment changes must invalidate runtime objects');
  const runtimeRoot = join(cache, 'runtime');
  writeFileSync(join(runtimeRoot, victim, 'artifact'), 'corrupt');
  miss();
  const native = join(directory, `corruption${suffix}`);
  run(compiler, ['build', project, '-O0', '-o', native]);
  assert.equal(run(native, []).stdout.replaceAll('\r\n', '\n'), '42\n');
  // Every published object must have a complete matching metadata record.
  for (const key of entries(runtimeRoot)) {
    const manifest = JSON.parse(readFileSync(join(runtimeRoot, key, 'metadata.json')));
    assert.equal(manifest.files.artifact.sha256, hash(readFileSync(join(runtimeRoot, key, 'artifact'))));
  }
  const concurrentRoot = join(directory, 'concurrent');
  await Promise.all(Array.from({ length: 4 }, (_, index) => new Promise((yes, no) => {
    const child = spawn(compiler, ['build', project, '-O0', '-o', join(directory, `race-${index}${suffix}`)],
      { env: { ...env, TSUZURI_CACHE_DIR: concurrentRoot }, stdio: ['ignore', 'pipe', 'pipe'] });
    let error = ''; child.stderr.on('data', bytes => error += bytes);
    child.on('error', no); child.on('exit', status => status === 0 ? yes() : no(new Error(error)));
  })));
  for (const key of entries(join(concurrentRoot, 'runtime'))) {
    const root = join(concurrentRoot, 'runtime', key), metadata = JSON.parse(readFileSync(join(root, 'metadata.json')));
    assert.equal(metadata.files.artifact.sha256, hash(readFileSync(join(root, 'artifact'))));
  }
  assert.equal(readdirSync(join(concurrentRoot, 'runtime')).filter(name => /^\.lock-|^\.tsuzuri-/.test(name) && name !== '.tsuzuri-cache').length, 0);
  const disabled = join(directory, 'disabled-cache');
  run(compiler, ['build', project, '-O0', '--no-cache', '-o', native], { ...env, TSUZURI_CACHE_DIR: disabled });
  assert.equal(existsSync(disabled), false);
  const brokenRoot = join(directory, 'file-not-directory');
  writeFileSync(brokenRoot, 'not a cache');
  run(compiler, ['build', project, '-O0', '-o', native], { ...env, TSUZURI_CACHE_DIR: brokenRoot });
  assert.equal(run(native, []).stdout.replaceAll('\r\n', '\n'), '42\n');
  console.log('PB01: native/WASM O0/O3 cold/warm objects, no-cache independence, corruption, atomic races and unavailable-cache fallback passed.');
} finally {
  rmSync(directory, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 });
}
