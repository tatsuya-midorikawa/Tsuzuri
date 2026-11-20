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
const {
  ExtensionQueryFilterType,
  ExtensionQueryFlags,
  ExtensionVersionFlags,
  TypeInfo,
} = require('azure-devops-node-api/interfaces/GalleryInterfaces');
const { ContractSerializer } = require('azure-devops-node-api/Serialization');
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

function formatVersionTarget(item) {
  return `${item?.version ?? 'unknown'}@${item?.targetPlatform || 'universal'}`;
}

function assertValidatedTargets(items, label) {
  for (const item of items) {
    assert.ok(Number.isInteger(item.flags), `Invalid ${label} flags: ${item.targetPlatform}`);
    assert.equal(item.flags & ExtensionVersionFlags.Validated, ExtensionVersionFlags.Validated,
      `${label} validation is not complete: ${item.targetPlatform}${item.validationResultMessage ? `: ${item.validationResultMessage}` : ''}. Do not republish uploaded packages.`);
  }
}

export async function queryLatestMarketplace(fetchImpl = globalThis.fetch) {
  const marketplaceUrl = (process.env.VSCE_MARKETPLACE_URL || 'https://marketplace.visualstudio.com').replace(/\/+$/, '');
  const response = await fetchImpl(`${marketplaceUrl}/_apis/public/gallery/extensionquery`, {
    method: 'POST',
    headers: {
      Accept: 'application/json;api-version=3.0-preview.1',
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({
      filters: [{
        pageNumber: 1,
        pageSize: 1,
        criteria: [
          { filterType: ExtensionQueryFilterType.Name, value: 'tmidorikawa.tsuzuri' },
          { filterType: ExtensionQueryFilterType.InstallationTarget, value: 'Microsoft.VisualStudio.Code' },
        ],
      }],
      flags: ExtensionQueryFlags.IncludeLatestVersionOnly
        | ExtensionQueryFlags.IncludeVersionProperties
        | ExtensionQueryFlags.ExcludeNonValidated,
    }),
    signal: AbortSignal.timeout(120000),
  });
  assert.ok(response.ok, `Marketplace extensionquery failed: HTTP ${response.status} ${response.statusText}`);
  const payload = await response.json();
  if (payload?.errorCode !== undefined) {
    throw new Error(payload.message || `Marketplace extensionquery error: ${payload.errorCode}`);
  }
  const extensions = ContractSerializer.deserialize(
    payload?.results?.[0]?.extensions ?? [],
    TypeInfo.PublishedExtension,
    false,
    false,
  );
  const match = extensions?.find(
    item => `${item.publisher?.publisherName ?? ''}.${item.extensionName ?? ''}`.toLowerCase() === 'tmidorikawa.tsuzuri',
  );
  assert.ok(match, 'Extension tmidorikawa.tsuzuri not found in Marketplace IncludeLatestVersionOnly query');
  return match;
}

export function verifyMarketplace(version, metadata, latestMetadata) {
  validateVersion(version);
  assert.equal(metadata.publisher?.publisherName, 'tmidorikawa', 'Marketplace publisher mismatch');
  assert.equal(metadata.extensionName, 'tsuzuri', 'Marketplace name mismatch');
  assert.ok(Array.isArray(metadata.versions), 'Missing Marketplace versions');
  const versions = metadata.versions.filter(item => item.version === version);
  assert.deepEqual(versions.map(item => item.targetPlatform).sort(), hosts,
    'Marketplace target set mismatch (missing, duplicate, or unexpected targets). If uploads were accepted, inspect propagation before retrying publication.');
  assertValidatedTargets(versions, 'Marketplace');
  const orphaned = metadata.versions.filter(item => !hosts.includes(item.targetPlatform));
  assert.equal(orphaned.length, 0,
    `Marketplace has unsupported or orphaned targetPlatform entries: ${orphaned.map(formatVersionTarget).join(', ')}`);

  if (latestMetadata !== undefined) {
    assert.ok(latestMetadata && typeof latestMetadata === 'object', 'Missing Marketplace IncludeLatestVersionOnly metadata');
    assert.equal(latestMetadata.publisher?.publisherName, 'tmidorikawa', 'Marketplace IncludeLatestVersionOnly publisher mismatch');
    assert.equal(latestMetadata.extensionName, 'tsuzuri', 'Marketplace IncludeLatestVersionOnly name mismatch');
    assert.ok(Array.isArray(latestMetadata.versions), 'Missing Marketplace IncludeLatestVersionOnly versions');
    const orphanedLatest = latestMetadata.versions.filter(item => !hosts.includes(item.targetPlatform));
    assert.equal(orphanedLatest.length, 0,
      `Marketplace IncludeLatestVersionOnly has unsupported or orphaned targetPlatform entries: ${orphanedLatest.map(formatVersionTarget).join(', ')}`);
    const wrongVersionLatest = latestMetadata.versions.filter(item => item.version !== version);
    assert.equal(wrongVersionLatest.length, 0,
      `Marketplace IncludeLatestVersionOnly returned unexpected version entries: ${wrongVersionLatest.map(formatVersionTarget).join(', ')}`);
    assert.deepEqual(latestMetadata.versions.map(item => item.targetPlatform).sort(), hosts,
      'Marketplace IncludeLatestVersionOnly target set mismatch (missing, duplicate, or unexpected targets).');
    assertValidatedTargets(latestMetadata.versions, 'Marketplace IncludeLatestVersionOnly');
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
      const latestMetadata = await queryLatestMarketplace();
      result = verifyMarketplace(version, JSON.parse(json), latestMetadata);
    } else {
      throw new Error('Usage: verify-release.mjs vsix VERSION HOST FILE | marketplace VERSION');
    }
    console.log(JSON.stringify(result, null, 2));
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
