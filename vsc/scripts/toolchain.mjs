import { copyFile, cp, lstat, mkdir, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';
import { bundle, digest, download, host, repository, verify } from '../../scripts/toolchain/bundle.mjs';

const extension = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const destination = path.join(extension, 'toolchain');
const codelldb = path.join(extension, 'resources', 'codelldb.vsix');
const debuggerVersion = '1.12.3';
export const debuggerChecksums = {
  'darwin-arm64': '2f114a990e1b368dd1dbd33c80c0e719767af2d228391ec0df0571c957f9ac91',
  'darwin-x64': 'e25cc716b94c62c07fec268ff2785d2b797245b160502baef8b9c970a0c4d8e8',
  'linux-arm64': '0887f67d440554617894266f80706b700907c36b95e6e49d23b95a0e05318101',
  'linux-x64': '1cd7f386598022b51a5b93b9ffa23e812b23f519cfe1833384ec4bef4bfd1be1',
  'win32-x64': 'a916e509308dac817732f63ca604a8b93ed29cd16f38a2fa9f0b64ed58e8f51a',
};

/** CodeLLDB is VSIX-only, so it lives beside the shared toolchain tree. */
async function debuggerPackage() {
  if (!debuggerChecksums[host]) { return; }
  const cache = path.join(repository, 'target', 'vsc-downloads');
  await mkdir(cache, { recursive: true });
  const archive = path.join(cache, `codelldb-${debuggerVersion}-${host}.vsix`);
  await download([`https://github.com/vadimcn/codelldb/releases/download/v${debuggerVersion}/codelldb-${host}.vsix`], archive, debuggerChecksums[host]);
  await mkdir(path.dirname(codelldb), { recursive: true });
  await copyFile(archive, codelldb);
}

async function verifyDebugger() {
  if (debuggerChecksums[host] && await digest(codelldb).catch(() => '') !== debuggerChecksums[host]) {
    throw new Error('Missing or invalid bundled debugger.');
  }
}

async function resources() {
  const root = path.join(extension, 'resources');
  await mkdir(root, { recursive: true });
  await copyFile(path.join(repository, 'LICENSE'), path.join(extension, 'LICENSE'));
  // Rebuild the handbook from scratch so documents removed from the repository do not linger in the VSIX.
  await rm(path.join(root, 'handbook'), { recursive: true, force: true });
  for (const name of ['_tsuzuri', 'docs', '_features', 'std']) {
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
  await cp(path.join(extension, 'images'), path.join(root, 'handbook', 'vsc', 'images'), { recursive: true });
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
  // The LLDB formatters that debug launches load (G16).
  await mkdir(path.join(root, 'lldb'), { recursive: true });
  await copyFile(path.join(repository, 'scripts', 'lldb', 'tsuzuri_lldb.py'), path.join(root, 'lldb', 'tsuzuri_lldb.py'));
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

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    if (process.argv.includes('--verify')) { await verify(destination); await verifyDebugger(); }
    else if (process.argv.includes('--resources')) { await resources(); }
    else { await resources(); await bundle(destination); await debuggerPackage(); await verifyDebugger(); }
  } catch (error) {
    console.error(error);
    process.exitCode = 1;
  }
}

