import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { cp, mkdtemp, mkdir, readdir, readFile, realpath, rm, symlink, writeFile } from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';

const [input, ...flags] = process.argv.slice(2);
if (!input || flags.some(flag => flag !== '--discover')) {
  throw new Error('usage: node scripts/toolchain/smoke.mjs <toolchain directory|archive> [--discover]');
}
const discover = flags.includes('--discover');
const source = path.resolve(input);
const archived = /\.(tar\.gz|zip)$/.test(source);
const directory = await mkdtemp(path.join(os.tmpdir(), 'tsuzuri IDE space-'));
let tools = path.join(directory, 'relocated toolchain');
const suffix = process.platform === 'win32' ? '.exe' : '';
const environment = { ...process.env, SDKROOT: path.join(directory, 'missing-sdk'), DEVELOPER_DIR: path.join(directory, 'missing-developer-tools') };
for (const key of Object.keys(environment)) {
  if (/^(PATH|TSUZURI_|ZIG_|DYLD_|LD_LIBRARY_PATH)/i.test(key)) { delete environment[key]; }
}
environment.ZIG_GLOBAL_CACHE_DIR = path.join(directory, 'zig-global');
environment.ZIG_LOCAL_CACHE_DIR = path.join(directory, 'zig-local');
environment.TSUZURI_CACHE_DIR = path.join(directory, 'cache');
const variables = { 'tsuzuri-clang': 'TSUZURI_CLANG', 'wasm-ld': 'TSUZURI_WASM_LD', 'llvm-link': 'TSUZURI_LLVM_LINK', dsymutil: 'TSUZURI_DSYMUTIL' };

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { cwd: directory, env: environment, encoding: 'utf8', timeout: 180000, ...options });
  assert.ifError(result.error);
  assert.equal(result.status, options.expectedCode ?? 0, `${command} ${args.join(' ')}\n${result.stdout}\n${result.stderr}`);
  return result;
}

try {
  if (archived) {
    const extracted = path.join(directory, 'extracted archive');
    await mkdir(extracted);
    if (process.platform === 'win32') {
      execFileSync(path.join(process.env.SystemRoot ?? 'C:\\Windows', 'System32', 'tar.exe'), ['-xf', source, '-C', extracted], { stdio: 'inherit' });
    } else {
      execFileSync('tar', ['-xzf', source, '-C', extracted], { stdio: 'inherit' });
    }
    const roots = await readdir(extracted);
    assert.equal(roots.length, 1, `an archive has one top-level directory: ${roots}`);
    tools = path.join(extracted, roots[0]);
  } else {
    await cp(source, tools, { recursive: true });
  }
  let compiler = path.join(tools, 'bin', `tsuzuri${suffix}`);
  if (discover) {
    // Only the compiler is on PATH; it must find the bundled tools itself.
    const only = path.join(directory, 'path-only');
    await mkdir(only);
    if (process.platform !== 'win32') {
      await symlink(compiler, path.join(only, 'tsuzuri'));
      compiler = path.join(only, 'tsuzuri');
    }
    environment.PATH = only;
    const real = await realpath(tools);
    const info = run(compiler, ['toolchain', 'info']).stdout;
    for (const [name, variable] of Object.entries(variables).slice(0, 2)) {
      assert.match(info, new RegExp(`^${variable}: bundled ${(path.join(real, 'bin', name + suffix)).replace(/[.*+?^${}()|[\]\\]/g, '\\$&')} `, 'm'), info);
    }
    console.log(info.trim());
  } else {
    environment.PATH = path.join(tools, 'bin');
    for (const [name, variable] of Object.entries(variables)) {
      environment[variable] = path.join(tools, 'bin', name + suffix);
    }
  }
  const project = path.join(directory, 'project # [one]');
  await mkdir(project);
  await writeFile(path.join(project, 'Main.tz'),
    'def main :: IO<unit> =\n    let! _line = IO.read_line ()\n    do! IO.write_line "42"\n\n'
    + 'test "same" = assert false\ntest "same" = assert true\n'
    + 'test "tasks" = { let tasks = new [task { return 1 }, task { return 2 }]; let values = Task.run (Task.parallel tasks); assert (values.length == 2) }\n');
  run(compiler, ['check', project, '--json']);
  const discovery = run(compiler, ['test', project, '--list', '--json'], {
    env: { ...environment, TSUZURI_CLANG: path.join(directory, 'not-a-compiler') },
  });
  const cases = discovery.stdout.trim().split('\n').map(line => JSON.parse(line));
  assert.equal(cases.length, 3);
  assert.equal(cases[0].name, cases[1].name);
  assert.deepEqual(cases.map(item => item.index), [0, 1, 2]);
  for (const optimization of ['-O0', '-O3']) {
    const artifact = path.join(directory, `application${suffix}`);
    run(compiler, ['build', project, optimization, '-g', '--no-cache', '-o', artifact]);
    assert.equal(run(artifact, [], { input: 'input\n' }).stdout, '42\n');
    const passed = run(compiler, ['test', project, optimization, '--json', '--index', '1', '--index', '2']);
    assert.match(passed.stdout, /"passed":2,"failed":0,"ignored":1/);
    const failed = run(compiler, ['test', project, optimization, '--json', '--index', '0'], { expectedCode: 1 });
    assert.match(failed.stdout, /"status":"failed"/);
    console.log(`Bundled native ${optimization}: IO, DWARF, duplicate test selection, parallel tasks, failures passed.`);
  }
  const kernel = path.join(directory, 'kernel');
  await mkdir(kernel);
  await writeFile(path.join(kernel, 'Main.tz'), 'export def answer :: i64 = 42\n');
  for (const optimization of ['-O0', '-O3']) {
    const artifact = path.join(directory, 'kernel.wasm');
    run(compiler, ['build', kernel, '--target', 'wasm32', optimization, '--no-cache', '-o', artifact]);
    const wasm = await WebAssembly.instantiate(await readFile(artifact));
    assert.equal(wasm.instance.exports.tz_answer(), 42n);
    console.log(`Bundled WASM ${optimization}: linked and executed.`);
  }
  run(compiler, ['fmt', path.join(kernel, 'Main.tz')]);
  const created = path.join(directory, 'new app');
  run(compiler, ['new', created, '--namespace', 'Acme::Smoke']);
  assert.match(await readFile(path.join(created, 'Tsuzuri.toml'), 'utf8'), /^namespace = "Acme::Smoke"$/m);
  await mkdir(path.join(created, 'Shapes'));
  await writeFile(path.join(created, 'Shapes', 'Square.tz'), 'namespace Acme::Smoke::Shapes\n\ndef side :: i64 -> i64 = \\x -> x * 2\n');
  await writeFile(path.join(created, 'Main.tz'), 'namespace Acme::Smoke\n\nusing Acme::Smoke::Shapes\n\ndef main :: i64 = \\() -> Square.side 20 + Shapes::Square.side 1\n');
  const application = path.join(directory, `namespaces${suffix}`);
  run(compiler, ['build', created, '--no-cache', '-o', application]);
  assert.equal(run(application, []).stdout, '42\n');
  console.log('New project: tsuzuri new, namespace, and using passed.');
  if (discover) {
    const rejected = run(compiler, ['build', kernel, '--target', 'wasm32', '--no-cache', '-o', path.join(directory, 'rejected.wasm')], {
      env: { ...environment, TSUZURI_WASM_LD: path.join(directory, 'not-a-linker') }, expectedCode: 1,
    });
    assert.match(rejected.stderr, /E2002/);
    console.log('Discovery: bundled tools found from PATH; an explicit variable still wins.');
  }
  console.log('Relocated toolchain passed without system compiler, SDK, or PATH tools.');
} finally {
  // Windows can keep a just-run executable locked for a moment (EBUSY).
  await rm(directory, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 });
}
