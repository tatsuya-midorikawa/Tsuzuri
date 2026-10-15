import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { lstat, mkdir, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';

const repository = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const environmentNames = ['SDKROOT', 'DEVELOPER_DIR', 'MACOSX_DEPLOYMENT_TARGET', 'INCLUDE', 'LIB', 'LIBPATH',
  'CPATH', 'C_INCLUDE_PATH', 'CPLUS_INCLUDE_PATH', 'LIBRARY_PATH', 'SOURCE_DATE_EPOCH',
  'TSUZURI_SYSROOT', 'TSUZURI_ZIG', 'TSUZURI_LLVM_CLANG'];
const inlineMath = name => /^tz_math_(sqrt|fabs)_f(32|64)$/.test(name);
async function resourceFingerprint(directory, compiler) {
  const resources = [];
  async function collect(current, prefix = '') {
    for (const name of (await readdir(current)).sort()) {
      const relative = prefix ? `${prefix}/${name}` : name;
      if ((!prefix && name === 'manifest.json') || relative === compiler
        || relative === 'share/runtime' || relative.startsWith('share/runtime/')) continue;
      const file = path.join(current, name), info = await lstat(file);
      if (info.isSymbolicLink()) throw new Error(`Toolchain must not contain symlinks: ${relative}`);
      if (info.isDirectory()) await collect(file, relative);
      else if (info.isFile()) resources.push([relative, hash(await readFile(file))]);
      else throw new Error(`Unsupported toolchain entry: ${relative}`);
    }
  }
  await collect(directory);
  return hash(JSON.stringify(resources));
}
export const mathObjectSource = source => source.replace(/^define internal (.*?@(\w+)\()/gm,
  (line, signature, name) => /^tz_math_.*_f(32|64)$/.test(name) && !inlineMath(name)
    ? `define hidden ${signature}` : line);

/** Build the exact generic objects selected by driver::compile_runtime_object. */
export async function bundleRuntime(destination, { clang = null, native = true } = {}) {
  const suffix = process.platform === 'win32' ? '.exe' : '';
  const compiler = clang ?? path.join(destination, 'bin', `tsuzuri-clang${suffix}`);
  const directory = path.join(destination, 'share/runtime');
  const work = path.join(directory, 'build');
  await mkdir(work, { recursive: true });
  const runtime = name => readFile(path.join(repository, 'src/runtime', name), 'utf8');
  const architecture = process.arch === 'arm64' ? 'aarch64' : 'x86_64';
  const nativeArgs = process.platform === 'win32' ? [`--target=${architecture}-pc-windows-msvc`] : ['-fPIC'];
  const sections = ['-ffunction-sections', '-fdata-sections'];
  const objects = [];
  async function compile(unit, source, args, id, extension) {
    const input = `${unit}.${extension}`, file = `${id}.o`;
    await writeFile(path.join(work, input), source);
    execFileSync(compiler, [...args, '-c', input, '-o', path.join(directory, file)],
      { cwd: work, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
    const bytes = await readFile(path.join(directory, file));
    objects.push({ unit, source: hash(source), args, file, size: bytes.length, sha256: hash(bytes) });
  }
  try {
    const numeric = await runtime('numeric.ll'), math = mathObjectSource(await runtime('math.ll'));
    for (const optimization of [0, 1, 2, 3]) {
      const ir = ['-x', 'ir', '-Wno-override-module', `-O${optimization}`];
      for (const [unit, source] of [['numeric', numeric], ['math', math]]) {
        if (native) await compile(unit, source, [...ir, ...nativeArgs, ...sections], `${unit}-native-O${optimization}`, 'll');
        for (const simd of [false, true]) {
          await compile(unit, source, [...ir, '--target=wasm32-unknown-unknown', '-mbulk-memory',
            simd ? '-msimd128' : '-mno-simd128', ...sections],
          `${unit}-wasm32-${simd ? 'simd' : 'scalar'}-O${optimization}`, 'll');
        }
      }
      if (native) {
        let task = await runtime('task.c');
        if (process.platform === 'win32') task = task.replace('#include "task-windows.h"', await runtime('task-windows.h'));
        const cpu = await runtime('cpu.c'), io = await runtime('io.c');
        for (let mask = 1; mask < 8; mask++) {
          const source = ['', '', '', '', '', mask & 1 ? task : '', mask & 2 ? cpu : '', mask & 4 ? io : '', ''].join('\n');
          const args = ['-std=c11', '-c', ...nativeArgs, `-O${optimization}`,
            ...(mask & 1 && process.platform !== 'win32' ? ['-pthread'] : []), ...sections];
          await compile('native-c', source, args, `native-c-${mask}-O${optimization}`, 'c');
        }
      }
    }
    const tools = {};
    for (const name of [`bin/tsuzuri-clang${suffix}`, `bin/clang${suffix}`,
      ...(native && !(process.platform === 'win32' && process.arch === 'arm64') ? [`zig/zig${suffix}`] : [])]) {
      try { tools[name] = hash(await readFile(path.join(destination, name))); }
      catch (error) { if (!clang || error.code !== 'ENOENT') throw error; }
    }
    const manifest = { format: 2, os: { darwin: 'macos', win32: 'windows', linux: 'linux' }[process.platform],
      arch: process.arch === 'arm64' ? 'aarch64' : 'x86_64',
      resource_fingerprint: await resourceFingerprint(destination, `bin/tsuzuri${suffix}`),
      environment: Object.fromEntries(environmentNames.map(name => [name, process.env[name] ?? null])),
      tools, objects };
    await writeFile(path.join(directory, 'manifest.json'), JSON.stringify(manifest, null, 2) + '\n');
    return manifest;
  } finally {
    await rm(work, { recursive: true, force: true });
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [destination, clang] = process.argv.slice(2);
  if (!destination) throw new Error('usage: node scripts/toolchain/runtime.mjs <distribution> [clang]');
  const manifest = await bundleRuntime(path.resolve(destination), { clang });
  console.log(`Bundled ${manifest.objects.length} runtime objects.`);
}
