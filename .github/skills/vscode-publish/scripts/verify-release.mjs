import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';

const repository = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../../..');
const require = createRequire(path.join(repository, 'vsc', 'package.json'));
const { readVSIXPackage, readZip } = require('@vscode/vsce/out/zip');
const { ExtensionVersionFlags } = require('azure-devops-node-api/interfaces/GalleryInterfaces');
export const hosts = ['darwin-arm64', 'linux-arm64', 'linux-x64', 'win32-arm64', 'win32-x64'];

function validateVersion(version) {
  assert.match(version, /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/, 'Use an explicit stable version, such as 0.1.3');
}

export async function verifyVsix(version, host, file) {
  validateVersion(version);
  assert.ok(hosts.includes(host), `Unsupported release target: ${host}`);
  const filename = `tsuzuri-${version}-${host}.vsix`;
  assert.equal(path.basename(file), filename, 'Unexpected VSIX filename');
  const checksum = (await readFile(`${file}.sha256`, 'utf8')).trim().split('  ');
  assert.equal(checksum.length, 2, 'Invalid VSIX checksum file');
  assert.match(checksum[0], /^[0-9a-f]{64}$/, 'Invalid VSIX SHA-256');
  assert.equal(checksum[1], filename, 'Checksum filename mismatch');
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(file)) hash.update(chunk);
  assert.equal(hash.digest('hex'), checksum[0], 'VSIX SHA-256 mismatch');

  const entries = [];
  const contents = await readZip(file, name => {
    entries.push(name);
    return ['extension/toolchain/manifest.json', 'extension/changelog.md'].includes(name);
  });
  const files = new Set(entries);
  assert.equal(files.size, entries.length, 'Duplicate VSIX entries');
  const { manifest, xmlManifest } = await readVSIXPackage(file);
  assert.equal(manifest.publisher, 'tmidorikawa', 'VSIX publisher mismatch');
  assert.equal(manifest.name, 'tsuzuri', 'VSIX name mismatch');
  assert.equal(manifest.version, version, 'VSIX version mismatch');
  assert.equal(manifest.main, './dist/extension.js', 'VSIX entry point mismatch');
  const identity = xmlManifest.PackageManifest?.Metadata?.[0]?.Identity?.[0]?.$;
  assert.ok(identity, 'Missing VSIX identity');
  for (const [key, value] of Object.entries({ Publisher: 'tmidorikawa', Id: 'tsuzuri', Version: version, TargetPlatform: host })) {
    assert.equal(identity[key], value, `VSIX ${key} mismatch`);
  }
  const toolchain = contents.get('extension/toolchain/manifest.json');
  assert.ok(toolchain, 'Missing bundled toolchain manifest');
  const tools = JSON.parse(toolchain.toString('utf8'));
  assert.equal(`${tools.platform}-${tools.arch}`, host, 'Toolchain host mismatch');
  assert.match(tools.id, /^[0-9a-f]{64}$/, 'Invalid toolchain identity');
  const suffix = host.startsWith('win32-') ? '.exe' : '';
  for (const name of ['tsuzuri', 'tsuzuri-clang', 'clang', 'wasm-ld']) {
    assert.ok(files.has(`extension/toolchain/bin/${name}${suffix}`), `Missing bundled tool: ${name}`);
  }
  assert.ok(files.has('extension/dist/extension.js'), 'Missing extension bundle');
  assert.equal(files.has('extension/resources/codelldb.vsix'), host !== 'win32-arm64', 'Bundled debugger policy mismatch');
  const changelog = contents.get('extension/changelog.md');
  assert.ok(changelog, 'Missing packaged changelog');
  assert.match(changelog.toString('utf8'), new RegExp(`^## ${version.replaceAll('.', '\\.')}(?:[ \\t]|$)`, 'm'), 'Missing release notes for this version');
  return { publisher: manifest.publisher, name: manifest.name, version, host, sha256: checksum[0], toolchainId: tools.id, file: path.resolve(file) };
}

export function verifyMarketplace(version, metadata) {
  validateVersion(version);
  assert.equal(metadata.publisher?.publisherName, 'tmidorikawa', 'Marketplace publisher mismatch');
  assert.equal(metadata.extensionName, 'tsuzuri', 'Marketplace name mismatch');
  assert.ok(Array.isArray(metadata.versions), 'Missing Marketplace versions');
  const versions = metadata.versions.filter(item => item.version === version);
  assert.deepEqual(versions.map(item => item.targetPlatform).sort(), hosts,
    'Marketplace target set mismatch (missing, duplicate, or unexpected targets). If uploads were accepted, inspect propagation before retrying publication.');
  for (const item of versions) {
    assert.ok(Number.isInteger(item.flags), `Invalid Marketplace flags: ${item.targetPlatform}`);
    assert.equal(item.flags & ExtensionVersionFlags.Validated, ExtensionVersionFlags.Validated,
      `Marketplace validation is not complete: ${item.targetPlatform}${item.validationResultMessage ? `: ${item.validationResultMessage}` : ''}. Do not republish uploaded packages.`);
  }
  return { publisher: 'tmidorikawa', name: 'tsuzuri', version, hosts, validated: true };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const [command, version, ...args] = process.argv.slice(2);
    validateVersion(version);
    let result;
    if (command === 'vsix' && args.length === 2) {
      result = await verifyVsix(version, args[0], args[1]);
    } else if (command === 'marketplace' && args.length === 0) {
      const json = execFileSync(process.execPath, [require.resolve('@vscode/vsce/vsce'), 'show', 'tmidorikawa.tsuzuri', '--json'],
        { encoding: 'utf8', timeout: 120000, maxBuffer: 8 * 1024 * 1024 });
      result = verifyMarketplace(version, JSON.parse(json));
    } else {
      throw new Error('Usage: verify-release.mjs vsix VERSION HOST FILE | marketplace VERSION');
    }
    console.log(JSON.stringify(result, null, 2));
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
