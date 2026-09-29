import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { createReadStream, createWriteStream } from 'node:fs';
import { access, chmod, copyFile, cp, lstat, mkdir, mkdtemp, readFile, readdir, realpath, rename, rm, writeFile } from 'node:fs/promises';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';
import { Readable } from 'node:stream';
import { pipeline } from 'node:stream/promises';

const extension = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const repository = path.dirname(extension);
const destination = path.join(extension, 'toolchain');
const host = `${process.platform}-${process.arch}`;
const suffix = process.platform === 'win32' ? '.exe' : '';
const zigVersion = '0.16.0';
const platforms = {
  'darwin-arm64': ['aarch64-macos', 'b23d70deaa879b5c2d486ed3316f7eaa53e84acf6fc9cc747de152450d401489'],
  'darwin-x64': ['x86_64-macos', '0387557ed1877bc6a2e1802c8391953baddba76081876301c522f52977b52ba7'],
  'linux-arm64': ['aarch64-linux', 'ea4b09bfb22ec6f6c6ceac57ab63efb6b46e17ab08d21f69f3a48b38e1534f17'],
  'linux-x64': ['x86_64-linux', '70e49664a74374b48b51e6f3fdfbf437f6395d42509050588bd49abe52ba3d00'],
  'win32-arm64': ['aarch64-windows', 'aee38316ee4111717900f45dd3130145c39289e105541d737eb8c5ed653c78ef'],
  'win32-x64': ['x86_64-windows', '68659eb5f1e4eb1437a722f1dd889c5a322c9954607f5edcf337bc3684a75a7e'],
};
const debuggerVersion = '1.12.3';
const debuggerChecksums = {
  'darwin-arm64': '2f114a990e1b368dd1dbd33c80c0e719767af2d228391ec0df0571c957f9ac91',
  'darwin-x64': 'e25cc716b94c62c07fec268ff2785d2b797245b160502baef8b9c970a0c4d8e8',
  'linux-arm64': '0887f67d440554617894266f80706b700907c36b95e6e49d23b95a0e05318101',
  'linux-x64': '1cd7f386598022b51a5b93b9ffa23e812b23f519cfe1833384ec4bef4bfd1be1',
  'win32-x64': 'a916e509308dac817732f63ca604a8b93ed29cd16f38a2fa9f0b64ed58e8f51a',
};

function run(command, args, capture = false) {
  return execFileSync(command, args, { cwd: repository, encoding: 'utf8', stdio: capture ? ['ignore', 'pipe', 'pipe'] : 'inherit' });
}

async function exists(file) {
  try { await access(file); return true; } catch { return false; }
}

async function digest(file) {
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(file)) { hash.update(chunk); }
  return hash.digest('hex');
}

async function download(url, file, expected) {
  if (await exists(file) && await digest(file) === expected) { return; }
  console.log(`Downloading ${url}`);
  const response = await fetch(url, { signal: AbortSignal.timeout(300000) });
  if (!response.ok || !response.body) { throw new Error(`Download failed: ${response.status} ${url}`); }
  const temporary = `${file}.partial`;
  await pipeline(Readable.fromWeb(response.body), createWriteStream(temporary));
  if (await digest(temporary) !== expected) {
    await rm(temporary, { force: true });
    throw new Error(`SHA-256 mismatch: ${url}`);
  }
  await rename(temporary, file);
}

async function resources() {
  const root = path.join(extension, 'resources');
  await mkdir(root, { recursive: true });
  await copyFile(path.join(repository, 'LICENSE'), path.join(extension, 'LICENSE'));
  for (const name of ['_docs', 'docs', '_features', 'std']) {
    await cp(path.join(repository, name), path.join(root, 'handbook', name), { recursive: true });
  }
  await cp(path.join(repository, 'examples'), path.join(root, 'handbook', 'examples'), {
    recursive: true,
    filter: async file => {
      if (path.basename(file).startsWith('.') || ['node_modules', 'target'].includes(path.basename(file))) { return false; }
      const info = await lstat(file);
      return info.isDirectory() || (info.isFile() && /\.(tz|tt|tc|toml|md|json|html|css|js|mjs|c|h|cpp|rs|png|jpg|svg)$/.test(file));
    },
  });
  await mkdir(path.join(root, 'handbook', 'vsc'), { recursive: true });
  await copyFile(path.join(extension, 'README.md'), path.join(root, 'handbook', 'vsc', 'README.md'));
  await copyFile(path.join(repository, 'README.md'), path.join(root, 'handbook', 'README.md'));
  const completions = new Map();
  for (const name of (await readdir(path.join(repository, 'std'))).sort()) {
    if (!/\.(tz|tc)$/.test(name)) { continue; }
    const source = await readFile(path.join(repository, 'std', name), 'utf8');
    const module = path.basename(name, path.extname(name));
    for (const match of source.matchAll(/^def\s+(?:rec\s+)?([A-Za-z_][A-Za-z_0-9]*)\s*(?:\{[^}]*\}\s*)?::\s*([^\r\n]+)/gm)) {
      if (match[1].startsWith('__')) { continue; }
      completions.set(`${module}.${match[1]}`, { module, name: match[1], signature: match[2].split(/\s=\s/)[0].trim() });
    }
  }
  await writeFile(path.join(root, 'completions.json'), JSON.stringify([...completions.values()], null, 2) + '\n');
  const licenses = path.join(root, 'licenses');
  await mkdir(licenses, { recursive: true });
  const lock = JSON.parse(await readFile(path.join(extension, 'package-lock.json'), 'utf8'));
  for (const [name, metadata] of Object.entries(lock.packages)) {
    if (!name || metadata.dev) { continue; }
    const directory = path.join(extension, name);
    for (const file of await readdir(directory)) {
      if (/^(license|copying|notice)/i.test(file) && (await lstat(path.join(directory, file))).isFile()) {
        await copyFile(path.join(directory, file), path.join(licenses, `${name.replaceAll('/', '_')}-${file}`));
      }
    }
  }
  console.log(`Prepared offline handbook, licenses, and ${completions.size} standard-library completions.`);
}

async function entries(directory, prefix = '') {
  const result = [];
  for (const name of (await readdir(directory)).sort()) {
    if (!prefix && name === 'manifest.json') { continue; }
    const file = path.join(directory, name);
    const relative = prefix ? `${prefix}/${name}` : name;
    const information = await lstat(file);
    if (information.isSymbolicLink()) { throw new Error(`Toolchain must not contain symlinks: ${relative}`); }
    if (information.isDirectory()) { result.push(...await entries(file, relative)); }
    else if (information.isFile()) { result.push({ path: relative, size: information.size, sha256: await digest(file) }); }
    else { throw new Error(`Unsupported toolchain entry: ${relative}`); }
  }
  return result;
}

function extractArchive(archive, directory) {
  if (process.platform !== 'win32') {
    run('tar', ['-xf', archive, '-C', directory]);
    return;
  }
  // A PSModulePath inherited from PowerShell 7 makes Windows PowerShell load incompatible modules.
  const env = Object.fromEntries(Object.entries(process.env).filter(([name]) => name.toLowerCase() !== 'psmodulepath'));
  execFileSync('powershell.exe', [
    '-NoProfile', '-NonInteractive', '-Command',
    '$ErrorActionPreference = "Stop"; Expand-Archive -LiteralPath $env:TSUZURI_ARCHIVE -DestinationPath $env:TSUZURI_EXTRACTED -Force',
  ], { cwd: repository, stdio: 'inherit', env: { ...env, TSUZURI_ARCHIVE: archive, TSUZURI_EXTRACTED: directory } });
}

async function verify() {
  const manifest = JSON.parse(await readFile(path.join(destination, 'manifest.json'), 'utf8'));
  if (manifest.platform !== process.platform || manifest.arch !== process.arch) { throw new Error('Toolchain host mismatch.'); }
  const files = await entries(destination);
  if (JSON.stringify(files) !== JSON.stringify(manifest.files)) { throw new Error('Toolchain contents differ from the verified manifest. Rebuild the bundle.'); }
  if (createHash('sha256').update(JSON.stringify(files)).digest('hex') !== manifest.id) { throw new Error('Invalid toolchain identity.'); }
  for (const name of ['tsuzuri', 'tsuzuri-clang', 'clang', 'wasm-ld', ...(process.platform === 'darwin' ? ['dsymutil', 'llvm-link'] : [])]) {
    const file = path.join(destination, 'bin', name + suffix);
    if (!await exists(file)) { throw new Error(`Missing required tool: ${name}`); }
    if (process.platform !== 'win32' && !((await lstat(file)).mode & 0o111)) { throw new Error(`Tool is not executable: ${name}`); }
  }
  run(path.join(destination, 'bin', `tsuzuri${suffix}`), ['--version']);
  if (debuggerChecksums[host]) {
    if (await digest(path.join(destination, 'codelldb.vsix')) !== debuggerChecksums[host]) { throw new Error('Missing or invalid bundled debugger.'); }
  }
  console.log(`Verified ${host}: ${files.length} files, ${manifest.id}`);
}

async function bundle() {
  if (!platforms[host]) { throw new Error(`Unsupported IDE host: ${host}. Current VS Code does not support x86 (32-bit).`); }
  const rustHost = /^host: (.+)$/m.exec(run('rustc', ['-vV'], true))?.[1];
  const architecture = process.arch === 'arm64' ? 'aarch64' : 'x86_64';
  if (!rustHost?.startsWith(`${architecture}-`)) { throw new Error(`Rust host ${rustHost} does not match the native extension host ${host}.`); }
  const llvm = process.env.LLVM_PREFIX;
  if (!llvm || !path.isAbsolute(llvm)) { throw new Error('Set LLVM_PREFIX to an LLVM 21 installation with Clang and LLVM utilities.'); }
  const clang = path.join(llvm, 'bin', `clang${suffix}`);
  if (!/clang version 21\./.test(run(clang, ['--version'], true))) { throw new Error('Use LLVM 21 to match the embedded runtime IR.'); }
  const [zigPlatform, checksum] = platforms[host];
  const archiveName = `zig-${zigPlatform}-${zigVersion}.${process.platform === 'win32' ? 'zip' : 'tar.xz'}`;
  const cache = path.join(repository, 'target', 'vsc-downloads');
  await mkdir(cache, { recursive: true });
  const archive = path.join(cache, archiveName);
  await download(`https://ziglang.org/download/${zigVersion}/${archiveName}`, archive, checksum);
  const extracted = await mkdtemp(path.join(cache, 'extract-'));
  const stage = await mkdtemp(path.join(extension, '.toolchain-stage-'));
  try {
    extractArchive(archive, extracted);
    const zig = path.join(extracted, `zig-${zigPlatform}-${zigVersion}`);
    await mkdir(path.join(stage, 'bin'), { recursive: true });
    await mkdir(path.join(stage, 'lib'), { recursive: true });
    await mkdir(path.join(stage, 'licenses'), { recursive: true });
    if (debuggerChecksums[host]) {
      const debuggerArchive = path.join(cache, `codelldb-${debuggerVersion}-${host}.vsix`);
      await download(`https://github.com/vadimcn/codelldb/releases/download/v${debuggerVersion}/codelldb-${host}.vsix`, debuggerArchive, debuggerChecksums[host]);
      await copyFile(debuggerArchive, path.join(stage, 'codelldb.vsix'));
    }
    await mkdir(path.join(stage, 'zig'), { recursive: true });
    await copyFile(path.join(zig, `zig${suffix}`), path.join(stage, 'zig', `zig${suffix}`));
    await cp(path.join(zig, 'lib'), path.join(stage, 'zig', 'lib'), { recursive: true, dereference: true });
    await copyFile(path.join(zig, 'LICENSE'), path.join(stage, 'licenses', 'Zig.txt'));
    if (process.platform !== 'win32') { await chmod(path.join(stage, 'zig', 'zig'), 0o755); }

    const copied = new Map();
    const destinations = new Map();
    const licensed = new Set();
    async function licenses(source) {
      let directory = path.dirname(source);
      for (let depth = 0; depth < 4 && path.dirname(directory) !== directory; depth++, directory = path.dirname(directory)) {
        if (licensed.has(directory)) { continue; }
        licensed.add(directory);
        for (const name of await readdir(directory)) {
          if (/^(license|copying|notice)(\.|$)/i.test(name) && (await lstat(path.join(directory, name))).isFile()) {
            const label = createHash('sha256').update(directory).digest('hex').slice(0, 12);
            await copyFile(path.join(directory, name), path.join(stage, 'licenses', `${path.basename(directory)}-${label}-${name}`));
          }
        }
      }
      if (process.platform === 'linux') {
        const owner = run('dpkg-query', ['-S', source], true).split(': ')[0].split(':')[0];
        const notice = path.join('/usr/share/doc', owner, 'copyright');
        if (!await exists(notice)) { throw new Error(`Missing redistribution license for ${source}`); }
        await copyFile(notice, path.join(stage, 'licenses', `${owner}.txt`));
      }
    }
    async function native(source, target) {
      source = await realpath(source);
      if (copied.has(source)) { return copied.get(source); }
      if (destinations.has(target) && destinations.get(target) !== source) { throw new Error(`Conflicting shared library: ${target}`); }
      destinations.set(target, source);
      copied.set(source, target);
      await copyFile(source, target);
      await chmod(target, 0o755);
      await licenses(source);
      if (process.platform === 'darwin') {
        const dependencies = run('otool', ['-L', source], true).split('\n').slice(1).map(line => line.trim().split(' (')[0]).filter(Boolean);
        const rpaths = [...run('otool', ['-l', source], true).matchAll(/cmd LC_RPATH\s+cmdsize \d+\s+path (.+?) \(offset/g)]
          .map(match => match[1].replace('@loader_path', path.dirname(source)));
        rpaths.push(path.resolve(path.dirname(source), '../lib'), path.dirname(source), path.join(llvm, 'lib'));
        for (const dependency of dependencies) {
          if (dependency.startsWith('/usr/lib/') || dependency.startsWith('/System/') || dependency === source) { continue; }
          let original = dependency.replace('@loader_path', path.dirname(source));
          if (dependency.startsWith('@rpath/')) {
            original = '';
            for (const directory of rpaths) {
              const candidate = path.join(directory, dependency.slice('@rpath/'.length));
              if (await exists(candidate)) { original = candidate; break; }
            }
            if (!original) { throw new Error(`Cannot resolve ${dependency} from ${source}`); }
          }
          const resolved = await realpath(original);
          if (resolved === source) { continue; }
          const label = createHash('sha256').update(resolved).digest('hex').slice(0, 12);
          const bundled = await native(resolved, path.join(stage, 'lib', `${label}-${path.basename(resolved)}`));
          run('install_name_tool', ['-change', dependency, `@loader_path/${path.relative(path.dirname(target), bundled)}`, target]);
        }
        if (target.endsWith('.dylib')) { run('install_name_tool', ['-id', `@rpath/${path.basename(target)}`, target]); }
        run('codesign', ['--force', '--sign', '-', target]);
      } else if (process.platform === 'linux') {
        const dependencies = run('ldd', [source], true);
        if (dependencies.includes('not found')) { throw new Error(`Unresolved shared library: ${dependencies}`); }
        for (const match of dependencies.matchAll(/=>\s+(\/\S+)\s+\(/g)) {
          if (/^(?:libc|libm|libpthread|libdl|librt|libresolv|libutil)\.so/.test(path.basename(match[1]))) { continue; }
          await native(match[1], path.join(stage, 'lib', path.basename(match[1])));
        }
        run('patchelf', ['--set-rpath', target.includes(`${path.sep}bin${path.sep}`) ? '$ORIGIN/../lib' : '$ORIGIN', target]);
      }
      return target;
    }
    for (const name of ['clang', 'wasm-ld', ...(process.platform === 'darwin' ? ['dsymutil', 'llvm-link'] : [])]) {
      let source = path.join(llvm, 'bin', name + suffix);
      if (name === 'wasm-ld' && process.env.TSUZURI_WASM_LD) { source = process.env.TSUZURI_WASM_LD; }
      await native(source, path.join(stage, 'bin', name + suffix));
    }
    if (process.platform === 'win32') {
      for (const name of await readdir(path.join(llvm, 'bin'))) {
        if (name.endsWith('.dll')) { await native(path.join(llvm, 'bin', name), path.join(stage, 'bin', name)); }
      }
    }
    if (process.platform === 'win32') { process.env.RUSTFLAGS = `${process.env.RUSTFLAGS ?? ''} -C target-feature=+crt-static`; }
    run('cargo', ['build', '--release', '--locked']);
    await copyFile(path.join(repository, 'target', 'release', `tsuzuri${suffix}`), path.join(stage, 'bin', `tsuzuri${suffix}`));
    run('rustc', ['--edition=2024', '-O', ...(process.platform === 'win32' ? ['-C', 'target-feature=+crt-static'] : []),
      path.join(extension, 'scripts', 'clang.rs'), '-o', path.join(stage, 'bin', `tsuzuri-clang${suffix}`)]);
    if (process.platform !== 'win32') { await chmod(path.join(stage, 'bin', 'tsuzuri'), 0o755); }
    await copyFile(path.join(repository, 'LICENSE'), path.join(stage, 'licenses', 'Tsuzuri.txt'));
    const files = await entries(stage);
    const manifest = {
      version: 1, platform: process.platform, arch: process.arch,
      compiler: run(path.join(stage, 'bin', `tsuzuri${suffix}`), ['--version'], true).trim(),
      zig: zigVersion, llvm: 21, debugger: debuggerChecksums[host] ? debuggerVersion : null,
      id: createHash('sha256').update(JSON.stringify(files)).digest('hex'), files,
    };
    await writeFile(path.join(stage, 'manifest.json'), JSON.stringify(manifest, null, 2) + '\n');
    const backup = path.join(extension, `.toolchain-backup-${process.pid}`);
    if (await exists(destination)) {
      if (!await exists(path.join(destination, 'manifest.json'))) { throw new Error('Refusing to replace an unmanaged toolchain directory.'); }
      await rename(destination, backup);
    }
    try { await rename(stage, destination); }
    catch (error) { if (await exists(backup)) { await rename(backup, destination); } throw error; }
    await rm(backup, { recursive: true, force: true });
  } finally {
    await rm(extracted, { recursive: true, force: true });
    await rm(stage, { recursive: true, force: true });
  }
  await verify();
}

try {
  if (process.argv.includes('--verify')) { await verify(); }
  else if (process.argv.includes('--resources')) { await resources(); }
  else { await resources(); await bundle(); }
} catch (error) {
  console.error(error);
  process.exitCode = 1;
}
