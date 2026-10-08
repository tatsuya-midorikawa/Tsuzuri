import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { createReadStream, createWriteStream } from 'node:fs';
import { access, chmod, copyFile, cp, lstat, mkdir, mkdtemp, readFile, readdir, realpath, rename, rm, writeFile } from 'node:fs/promises';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';
import { Readable } from 'node:stream';
import { pipeline } from 'node:stream/promises';
import { setTimeout as delay } from 'node:timers/promises';

export const repository = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
export const host = `${process.platform}-${process.arch}`;
export const suffix = process.platform === 'win32' ? '.exe' : '';
export const zigVersion = '0.16.0';
export const platforms = {
  'darwin-arm64': ['aarch64-macos', 'b23d70deaa879b5c2d486ed3316f7eaa53e84acf6fc9cc747de152450d401489'],
  'darwin-x64': ['x86_64-macos', '0387557ed1877bc6a2e1802c8391953baddba76081876301c522f52977b52ba7'],
  'linux-arm64': ['aarch64-linux', 'ea4b09bfb22ec6f6c6ceac57ab63efb6b46e17ab08d21f69f3a48b38e1534f17'],
  'linux-x64': ['x86_64-linux', '70e49664a74374b48b51e6f3fdfbf437f6395d42509050588bd49abe52ba3d00'],
  'win32-x64': ['x86_64-windows', '68659eb5f1e4eb1437a722f1dd889c5a322c9954607f5edcf337bc3684a75a7e'],
};
// ziglang.org is one server and asks automation to try these mirrors in random order first; the pinned
// SHA-256 authenticates every source. Snapshot of https://ziglang.org/download/community-mirrors.txt.
const zigMirrors = [
  'https://pkg.hexops.org/zig', 'https://zigmirror.hryx.net/zig', 'https://zig.linus.dev/zig', 'https://zig.squirl.dev',
  'https://zig.mirror.mschae23.de/zig', 'https://ziglang.freetls.fastly.net', 'https://zig.tilok.dev',
  'https://zig-mirror.tsimnet.eu/zig', 'https://zig.karearl.com/zig', 'https://pkg.earth/zig', 'https://fs.liujiacai.net/zigbuilds',
  'https://zigmirror.com', 'https://zig.chainsafe.dev', 'https://zig.savalione.com', 'https://zig.bcr.ist',
  'https://zig.vortan.dev/zig', 'https://pkg.alexrp.com/zig',
];
// Zig 0.16.0 crashes while linking on ARM64 Windows hosts; this release matches the bundled LLVM 21.1.8.
export const mingwVersion = '20251216';
export const mingwChecksums = {
  'win32-arm64': '60c06bd255feb2ef1eb6fce7ee6b307d8f78ee6639660f49861c7c10a8a86164',
};
export const llvmTools = ['clang', 'wasm-ld', ...(process.platform === 'darwin' ? ['dsymutil', 'llvm-link'] : []), ...(mingwChecksums[host] ? ['ld.lld'] : [])];

export function run(command, args, capture = false) {
  return execFileSync(command, args, { cwd: repository, encoding: 'utf8', stdio: capture ? ['ignore', 'pipe', 'pipe'] : 'inherit' });
}

export async function exists(file) {
  try { await access(file); return true; } catch { return false; }
}

export async function digest(file) {
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(file)) { hash.update(chunk); }
  return hash.digest('hex');
}

/** Saves the first of `urls` whose bytes match `expected`; a source silent for 30 seconds is skipped. */
export async function download(urls, file, expected) {
  if (await exists(file) && await digest(file) === expected) { return; }
  const temporary = `${file}.partial`;
  const failures = [];
  // GitHub releases and mirrors fail transiently (HTTP 5xx), so retry every source after a pause.
  for (let pass = 1; pass <= 3; pass++) {
    if (pass > 1) {
      console.warn(`Retrying every source in ${pass * 10} seconds`);
      await delay(pass * 10000);
    }
    for (const url of urls) {
      console.log(`Downloading ${url}`);
      const controller = new AbortController();
      let timer;
      const alive = () => {
        clearTimeout(timer);
        timer = setTimeout(() => controller.abort(new Error('no data for 30 seconds')), 30000);
      };
      try {
        alive();
        const response = await fetch(url, { signal: controller.signal });
        if (!response.ok || !response.body) { throw new Error(`HTTP ${response.status}`); }
        await pipeline(Readable.fromWeb(response.body), async function* (chunks) {
          for await (const chunk of chunks) { alive(); yield chunk; }
        }, createWriteStream(temporary));
        if (await digest(temporary) !== expected) { throw new Error('SHA-256 mismatch'); }
        await rename(temporary, file);
        return;
      } catch (error) {
        failures.push(`${url}: ${error.message}`);
        console.warn(`Download failed: ${error.message}`);
      } finally {
        clearTimeout(timer);
      }
    }
  }
  await rm(temporary, { force: true });
  throw new Error(`Download failed from every source:\n${failures.join('\n')}`);
}

export async function entries(directory, prefix = '') {
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

export function extractArchive(archive, directory) {
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

export async function verify(destination) {
  const manifest = JSON.parse(await readFile(path.join(destination, 'manifest.json'), 'utf8'));
  if (manifest.platform !== process.platform || manifest.arch !== process.arch) { throw new Error('Toolchain host mismatch.'); }
  const files = await entries(destination);
  if (JSON.stringify(files) !== JSON.stringify(manifest.files)) { throw new Error('Toolchain contents differ from the verified manifest. Rebuild the bundle.'); }
  if (createHash('sha256').update(JSON.stringify(files)).digest('hex') !== manifest.id) { throw new Error('Invalid toolchain identity.'); }
  for (const name of ['tsuzuri', 'tsuzuri-clang', ...llvmTools]) {
    const file = path.join(destination, 'bin', name + suffix);
    if (!await exists(file)) { throw new Error(`Missing required tool: ${name}`); }
    if (process.platform !== 'win32' && !((await lstat(file)).mode & 0o111)) { throw new Error(`Tool is not executable: ${name}`); }
  }
  for (const file of ['share/lldb/tsuzuri_lldb.py', 'share/natvis/tsuzuri.natvis']) {
    if (!await exists(path.join(destination, ...file.split('/')))) { throw new Error(`Missing debugger support file: ${file}`); }
  }
  run(path.join(destination, 'bin', `tsuzuri${suffix}`), ['--version']);
  console.log(`Verified ${host}: ${files.length} files, ${manifest.id}`);
  return manifest;
}

/** The first output line naming a version, or else the first nonempty line. */
function versionLine(file) {
  const lines = run(file, ['--version'], true).split(/\r?\n/).map(line => line.trim()).filter(Boolean);
  return lines.find(line => /version/i.test(line)) ?? lines[0] ?? '';
}

/** Copies the license texts of the normal Rust dependencies compiled into the compiler. */
async function crateLicenses(rustHost, licenses) {
  const metadata = JSON.parse(run('cargo', ['metadata', '--format-version', '1', '--locked', '--filter-platform', rustHost], true));
  const packages = new Map(metadata.packages.map(item => [item.id, item]));
  const nodes = new Map(metadata.resolve.nodes.map(node => [node.id, node]));
  const reached = new Set();
  const pending = [metadata.resolve.root];
  while (pending.length) {
    for (const dependency of nodes.get(pending.pop()).deps) {
      if (dependency.dep_kinds.some(kind => kind.kind === null) && !reached.has(dependency.pkg)) {
        reached.add(dependency.pkg);
        pending.push(dependency.pkg);
      }
    }
  }
  const crates = [...reached].map(id => packages.get(id))
    .sort((left, right) => left.name.localeCompare(right.name) || left.version.localeCompare(right.version));
  const lines = ['Rust standard library: MIT OR Apache-2.0 (Apache-2.0 text: Tsuzuri.txt)'];
  for (const crate of crates) {
    lines.push(`${crate.name} ${crate.version} ${crate.license}`);
    const directory = path.dirname(crate.manifest_path);
    const texts = (await readdir(directory)).filter(name => /^(license|copying|notice)/i.test(name)).sort();
    if (!texts.length) { throw new Error(`Missing license file for crate ${crate.name}`); }
    for (const name of texts) {
      await copyFile(path.join(directory, name), path.join(licenses, `crate-${crate.name}-${crate.version}-${name}`));
    }
  }
  await writeFile(path.join(licenses, 'rust-crates.txt'), lines.join('\n') + '\n');
}

export async function bundle(destination) {
  if (!platforms[host] && !mingwChecksums[host]) { throw new Error(`Unsupported toolchain host: ${host}. x86 (32-bit) hosts are not supported.`); }
  const rustHost = /^host: (.+)$/m.exec(run('rustc', ['-vV'], true))?.[1];
  const architecture = process.arch === 'arm64' ? 'aarch64' : 'x86_64';
  if (!rustHost?.startsWith(`${architecture}-`)) { throw new Error(`Rust host ${rustHost} does not match the native toolchain host ${host}.`); }
  const llvm = process.env.LLVM_PREFIX;
  if (!llvm || !path.isAbsolute(llvm)) { throw new Error('Set LLVM_PREFIX to an LLVM 21 installation with Clang and LLVM utilities.'); }
  const clang = path.join(llvm, 'bin', `clang${suffix}`);
  if (!/clang version 21\./.test(run(clang, ['--version'], true))) { throw new Error('Use LLVM 21 to match the embedded runtime IR.'); }
  const cache = path.join(repository, 'target', 'vsc-downloads');
  await mkdir(cache, { recursive: true });
  await mkdir(path.dirname(destination), { recursive: true });
  const extracted = await mkdtemp(path.join(cache, 'extract-'));
  const stage = await mkdtemp(path.join(path.dirname(destination), '.toolchain-stage-'));
  try {
    await mkdir(path.join(stage, 'bin'), { recursive: true });
    await mkdir(path.join(stage, 'lib'), { recursive: true });
    await mkdir(path.join(stage, 'licenses'), { recursive: true });
    if (mingwChecksums[host]) {
      const name = `llvm-mingw-${mingwVersion}-ucrt-${architecture}`;
      const archive = path.join(cache, `${name}.zip`);
      await download([`https://github.com/mstorsjo/llvm-mingw/releases/download/${mingwVersion}/${name}.zip`], archive, mingwChecksums[host]);
      extractArchive(archive, extracted);
      const source = path.join(extracted, name);
      const sysroot = path.join(stage, 'mingw');
      const triple = `${architecture}-w64-mingw32`;
      const resources = path.join('lib', 'clang', '21');
      const builtins = path.join(resources, 'lib', 'windows', `libclang_rt.builtins-${architecture}.a`);
      await cp(path.join(source, triple, 'lib'), path.join(sysroot, triple, 'lib'), { recursive: true, dereference: true });
      await cp(path.join(source, 'include'), path.join(sysroot, 'include'), {
        recursive: true, dereference: true, filter: file => file !== path.join(source, 'include', 'c++'),
      });
      await cp(path.join(source, resources, 'include'), path.join(sysroot, resources, 'include'), { recursive: true, dereference: true });
      await mkdir(path.dirname(path.join(sysroot, builtins)), { recursive: true });
      await copyFile(path.join(source, builtins), path.join(sysroot, builtins));
      await copyFile(path.join(source, 'LICENSE.TXT'), path.join(stage, 'licenses', 'llvm-mingw.txt'));
      const notices = path.join(source, triple, 'share', 'mingw32');
      for (const file of await readdir(notices)) {
        await copyFile(path.join(notices, file), path.join(stage, 'licenses', `mingw-w64-${file}`));
      }
    } else {
      const [zigPlatform, checksum] = platforms[host];
      const archiveName = `zig-${zigPlatform}-${zigVersion}.${process.platform === 'win32' ? 'zip' : 'tar.xz'}`;
      const archive = path.join(cache, archiveName);
      const mirrors = zigMirrors.map(mirror => [Math.random(), `${mirror}/${archiveName}?source=tsuzuri-toolchain`])
        .sort(([left], [right]) => left - right).map(([, url]) => url);
      await download([...mirrors, `https://ziglang.org/download/${zigVersion}/${archiveName}`], archive, checksum);
      extractArchive(archive, extracted);
      const zig = path.join(extracted, `zig-${zigPlatform}-${zigVersion}`);
      await mkdir(path.join(stage, 'zig'), { recursive: true });
      await copyFile(path.join(zig, `zig${suffix}`), path.join(stage, 'zig', `zig${suffix}`));
      await cp(path.join(zig, 'lib'), path.join(stage, 'zig', 'lib'), { recursive: true, dereference: true });
      await copyFile(path.join(zig, 'LICENSE'), path.join(stage, 'licenses', 'Zig.txt'));
      if (process.platform !== 'win32') { await chmod(path.join(stage, 'zig', 'zig'), 0o755); }
    }
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
    for (const name of llvmTools) {
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
      path.join(repository, 'scripts', 'toolchain', 'clang.rs'), '-o', path.join(stage, 'bin', `tsuzuri-clang${suffix}`)]);
    if (process.platform !== 'win32') { await chmod(path.join(stage, 'bin', 'tsuzuri'), 0o755); }
    await copyFile(path.join(repository, 'LICENSE'), path.join(stage, 'licenses', 'Tsuzuri.txt'));
    // The LLDB formatters for `-g` builds, which `command script import` loads, and the natvis views
    // that MSVC links of `-g` objects embed with /NATVIS (G16).
    await mkdir(path.join(stage, 'share', 'lldb'), { recursive: true });
    await copyFile(path.join(repository, 'scripts', 'lldb', 'tsuzuri_lldb.py'), path.join(stage, 'share', 'lldb', 'tsuzuri_lldb.py'));
    await mkdir(path.join(stage, 'share', 'natvis'), { recursive: true });
    await copyFile(path.join(repository, 'src', 'runtime', 'tsuzuri.natvis'), path.join(stage, 'share', 'natvis', 'tsuzuri.natvis'));
    await copyFile(path.join(repository, 'src', 'runtime', 'musl', 'COPYRIGHT'), path.join(stage, 'licenses', 'musl-COPYRIGHT.txt'));
    await crateLicenses(rustHost, path.join(stage, 'licenses'));
    const toolVersions = Object.fromEntries(llvmTools.map(name => [name, versionLine(path.join(stage, 'bin', name + suffix))]));
    const files = await entries(stage);
    const manifest = {
      version: 1, platform: process.platform, arch: process.arch,
      compiler: run(path.join(stage, 'bin', `tsuzuri${suffix}`), ['--version'], true).trim(),
      zig: platforms[host] ? zigVersion : null, mingw: mingwChecksums[host] ? mingwVersion : null,
      llvm: 21, toolVersions,
      id: createHash('sha256').update(JSON.stringify(files)).digest('hex'), files,
    };
    await writeFile(path.join(stage, 'manifest.json'), JSON.stringify(manifest, null, 2) + '\n');
    const backup = path.join(path.dirname(destination), `.toolchain-backup-${process.pid}`);
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
  return verify(destination);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const destination = process.argv.slice(2).find(argument => !argument.startsWith('--'));
    if (!destination) { throw new Error('usage: node scripts/toolchain/bundle.mjs <destination> [--verify]'); }
    if (process.argv.includes('--verify')) { await verify(path.resolve(destination)); }
    else { await bundle(path.resolve(destination)); }
  } catch (error) {
    console.error(error);
    process.exitCode = 1;
  }
}
