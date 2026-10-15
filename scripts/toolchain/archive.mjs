import { execFileSync } from 'node:child_process';
import { cp, mkdir, mkdtemp, readdir, rm, stat, writeFile } from 'node:fs/promises';
import * as path from 'node:path';
import { digest, extractArchive, verify } from './bundle.mjs';

const [toolchain, output] = process.argv.slice(2).map(argument => path.resolve(argument));
if (!toolchain || !output) { throw new Error('usage: node scripts/toolchain/archive.mjs <toolchain> <output directory>'); }

const manifest = await verify(toolchain);
const version = manifest.compiler.replace(/^tsuzuri /, '');
if (!/^\d+\.\d+\.\d+/.test(version)) { throw new Error(`Unexpected compiler version: ${manifest.compiler}`); }
const name = `tsuzuri-${version}-${manifest.platform}-${manifest.arch}`;
await mkdir(output, { recursive: true });
const archive = path.join(output, `${name}${manifest.platform === 'win32' ? '.zip' : '.tar.gz'}`);
const staging = await mkdtemp(path.join(output, '.archive-'));
const check = await mkdtemp(path.join(output, '.check-'));
try {
  await cp(toolchain, path.join(staging, name), { recursive: true });
  await rm(archive, { force: true });
  if (manifest.platform === 'win32') {
    // Git for Windows puts a GNU tar first on PATH, which cannot write zip archives.
    const tar = path.join(process.env.SystemRoot ?? 'C:\\Windows', 'System32', 'tar.exe');
    execFileSync(tar, ['-a', '-cf', archive, '-C', staging, name], { stdio: 'inherit' });
  } else {
    // Keep macOS AppleDouble files and extended attributes out of the archive.
    execFileSync('tar', ['--no-xattrs', '-czf', archive, '-C', staging, name], {
      stdio: 'inherit', env: { ...process.env, COPYFILE_DISABLE: '1' },
    });
  }
  await writeFile(`${archive}.sha256`, `${await digest(archive)}  ${path.basename(archive)}\n`);
  extractArchive(archive, check);
  const roots = await readdir(check);
  if (roots.length !== 1 || roots[0] !== name) { throw new Error(`Archive must contain only ${name}/, found ${roots.join(', ')}`); }
  const extracted = await verify(path.join(check, name));
  if (extracted.id !== manifest.id) { throw new Error('Archive contents differ from the toolchain manifest.'); }
} finally {
  await rm(staging, { recursive: true, force: true });
  await rm(check, { recursive: true, force: true });
}
console.log(`Archived ${archive} (${(await stat(archive)).size} bytes, ${manifest.id})`);
