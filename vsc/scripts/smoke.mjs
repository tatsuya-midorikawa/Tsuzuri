import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { cp, mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';

const extension = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const source = path.resolve(process.argv[2] ?? path.join(extension, 'toolchain'));
const directory = await mkdtemp(path.join(os.tmpdir(), 'tsuzuri IDE space-'));
const tools = path.join(directory, 'relocated toolchain');
const suffix = process.platform === 'win32' ? '.exe' : '';
const environment = { ...process.env, SDKROOT: path.join(directory, 'missing-sdk'), DEVELOPER_DIR: path.join(directory, 'missing-developer-tools') };
for (const key of Object.keys(environment)) {
  if (/^(PATH|TSUZURI_|ZIG_|DYLD_|LD_LIBRARY_PATH)/i.test(key)) { delete environment[key]; }
}
environment.PATH = path.join(tools, 'bin');
environment.ZIG_GLOBAL_CACHE_DIR = path.join(directory, 'zig-global');
environment.ZIG_LOCAL_CACHE_DIR = path.join(directory, 'zig-local');
environment.TSUZURI_CACHE_DIR = path.join(directory, 'cache');
for (const [name, variable] of Object.entries({ 'tsuzuri-clang': 'TSUZURI_CLANG', 'wasm-ld': 'TSUZURI_WASM_LD', 'llvm-link': 'TSUZURI_LLVM_LINK', dsymutil: 'TSUZURI_DSYMUTIL' })) {
  environment[variable] = path.join(tools, 'bin', name + suffix);
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { cwd: directory, env: environment, encoding: 'utf8', timeout: 180000, ...options });
  assert.ifError(result.error);
  assert.equal(result.status, options.expectedCode ?? 0, `${command} ${args.join(' ')}\n${result.stdout}\n${result.stderr}`);
  return result;
}

try {
  await cp(source, tools, { recursive: true });
  const compiler = path.join(tools, 'bin', `tsuzuri${suffix}`);
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
  console.log('Relocated toolchain passed without system compiler, SDK, or PATH tools.');
} finally {
  await rm(directory, { recursive: true, force: true });
}
