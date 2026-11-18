import { execFileSync } from 'node:child_process';
import { copyFile, link, mkdir, mkdtemp, readdir, readlink, rm, stat, symlink, writeFile } from 'node:fs/promises';
import * as path from 'node:path';
import { digest, extractArchive, systemTar, verify } from './bundle.mjs';

const [toolchain, output] = process.argv.slice(2).map(argument => path.resolve(argument));
if (!toolchain || !output) { throw new Error('usage: node scripts/toolchain/archive.mjs <toolchain> <output directory>'); }

const manifest = await verify(toolchain);
const version = manifest.compiler.replace(/^tsuzuri /, '');
if (!/^\d+\.\d+\.\d+/.test(version)) { throw new Error(`Unexpected compiler version: ${manifest.compiler}`); }
const name = `tsuzuri-${version}-${manifest.platform}-${manifest.arch}`;
await mkdir(output, { recursive: true });
const archive = path.join(output, `${name}${manifest.platform === 'win32' ? '.zip' : '.tar.gz'}`);

/** The tree of `source` at `target`, with each file hard-linked rather than copied: the archive needs the toolchain under
 * the name `name`, and copying its ~20,000 files took minutes on windows-2022. A file that cannot be linked (another
 * volume, a file system without links) is copied; symbolic links stay links. */
async function linkTree(source, target) {
  await mkdir(target, { recursive: true });
  for (const entry of await readdir(source, { withFileTypes: true })) {
    const from = path.join(source, entry.name);
    const to = path.join(target, entry.name);
    if (entry.isDirectory()) {
      await linkTree(from, to);
    } else if (entry.isSymbolicLink()) {
      await symlink(await readlink(from), to);
    } else {
      await link(from, to).catch(() => copyFile(from, to));
    }
  }
}
const staging = await mkdtemp(path.join(output, '.archive-'));
const check = await mkdtemp(path.join(output, '.check-'));
try {
  await linkTree(toolchain, path.join(staging, name));
  await rm(archive, { force: true });
  if (manifest.platform === 'win32') {
    // Git for Windows puts a GNU tar first on PATH, which cannot write zip archives.
    execFileSync(systemTar(), ['-a', '-cf', archive, '-C', staging, name], { stdio: 'inherit' });
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
