import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import { createVSIX, listFiles } from '@vscode/vsce';
import { debuggerChecksums } from './toolchain.mjs';

const require = createRequire(import.meta.url);
const yauzl = require('yauzl');
const { lintFiles } = require('@vscode/vsce/out/secretLint');
const extension = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const manifest = JSON.parse(await readFile(path.join(extension, 'package.json'), 'utf8'));
const target = `${process.platform}-${process.arch}`;
const output = path.join(extension, 'dist', `tsuzuri-${manifest.version}-${target}.vsix`);
await mkdir(path.dirname(output), { recursive: true });

async function verifyArchive(file) {
  const tools = JSON.parse(await readFile(path.join(extension, 'toolchain', 'manifest.json'), 'utf8'));
  const expected = new Map(tools.files.map(item => [`extension/toolchain/${item.path}`, item]));
  // CodeLLDB is outside the toolchain manifest; check it against the pinned release digest.
  if (debuggerChecksums[target]) {
    expected.set('extension/resources/codelldb.vsix', { sha256: debuggerChecksums[target] });
  }
  const required = new Set(['extension/dist/extension.js', 'extension/package.json', 'extension/language-configuration.json',
    `extension/${manifest.icon}`,
    'extension/syntaxes/tsuzuri.tmLanguage.json', 'extension/snippets/tsuzuri.json', 'extension/resources/completions.json',
    'extension/resources/handbook/_tsuzuri/language-reference/index.md', 'extension/LICENSE.txt', 'extension/toolchain/manifest.json']);
  await new Promise((resolve, reject) => {
    yauzl.open(file, { lazyEntries: true }, (error, zip) => {
      if (error) { reject(error); return; }
      zip.on('error', reject);
      zip.on('entry', entry => {
        required.delete(entry.fileName);
        const record = expected.get(entry.fileName);
        if (!record) { zip.readEntry(); return; }
        if (record.size !== undefined && entry.uncompressedSize !== record.size) { zip.close(); reject(new Error(`Wrong archive size: ${entry.fileName}`)); return; }
        if (process.platform !== 'win32' && /extension\/toolchain\/(?:bin\/|zig\/zig$)/.test(entry.fileName)
          && !((entry.externalFileAttributes >>> 16) & 0o111)) {
          zip.close(); reject(new Error(`Archive lost executable permissions: ${entry.fileName}`)); return;
        }
        zip.openReadStream(entry, (streamError, stream) => {
          if (streamError) { zip.close(); reject(streamError); return; }
          const hash = createHash('sha256');
          stream.on('error', reject);
          stream.on('data', chunk => hash.update(chunk));
          stream.on('end', () => {
            if (hash.digest('hex') !== record.sha256) { zip.close(); reject(new Error(`Archive checksum mismatch: ${entry.fileName}`)); return; }
            expected.delete(entry.fileName);
            zip.readEntry();
          });
        });
      });
      zip.on('end', () => {
        if (expected.size || required.size) { reject(new Error(`Incomplete VSIX: ${[...expected.keys(), ...required].slice(0, 12).join(', ')}`)); }
        else { resolve(); }
      });
      zip.readEntry();
    });
  });
}

if (process.argv.includes('--scan')) {
  const files = (await listFiles({ cwd: extension, dependencies: false }))
    .filter(file => !file.startsWith('toolchain/') && !/\.(jpg|jpeg|png|gif|svg|vsix)$/i.test(file))
    .map(file => path.join(extension, file));
  const scanned = await lintFiles(files, true, true);
  if (!scanned.ok) {
    throw new Error(`Secret scan failed: ${scanned.results.map(result => `${result.filePath} (${result.ruleId})`).join(', ')}`);
  }
  console.log(`Secret scan passed: ${files.length} extension and documentation files. Toolchain integrity is verified separately.`);
} else {
  if (!process.argv.includes('--verify')) {
    // vsce ignores repository.directory, so point README-relative links at vsc/ explicitly.
    const repository = manifest.repository.url.replace(/\.git$/, '');
    await createVSIX({ cwd: extension, packagePath: output, target, dependencies: false,
      allowPackageAllSecrets: true, allowPackageEnvFile: true,
      baseContentUrl: `${repository}/blob/HEAD/${manifest.repository.directory}`,
      baseImagesUrl: `${repository}/raw/HEAD/${manifest.repository.directory}` });
  }
  await verifyArchive(output);
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(output)) { hash.update(chunk); }
  await writeFile(`${output}.sha256`, `${hash.digest('hex')}  ${path.basename(output)}\n`);
  console.log(`Verified installable VSIX: ${output}`);
}
