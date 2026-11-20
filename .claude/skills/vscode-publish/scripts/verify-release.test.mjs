import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { createWriteStream } from 'node:fs';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import * as path from 'node:path';
import { pipeline } from 'node:stream/promises';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { hosts, queryLatestMarketplace, verifyMarketplace, verifyVsix } from './verify-release.mjs';
import { platforms, mingwChecksums } from '../../../../scripts/toolchain/bundle.mjs';

const repository = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../../..');
const require = createRequire(path.join(repository, 'vsc', 'package.json'));
const { ZipFile } = require('yazl');
const version = '1.2.3';

async function fixture(directory, host, toolsArch, identityHost = host) {
  const [platform, arch] = host.split('-');
  const file = path.join(directory, `tsuzuri-${version}-${host}.vsix`);
  const zip = new ZipFile();
  const add = (name, contents) => zip.addBuffer(Buffer.from(contents), name);
  add('extension/package.json', JSON.stringify({
    name: 'tsuzuri', publisher: 'tmidorikawa', version, main: './dist/extension.js',
    activationEvents: ['onLanguage:tsuzuri'],
    engines: { vscode: '^1.103.0' }, license: 'Apache-2.0',
    repository: { type: 'git', url: 'https://github.com/tatsuya-midorikawa/Tsuzuri.git' },
  }));
  add('extension.vsixmanifest', `<PackageManifest><Metadata><Identity Publisher="tmidorikawa" Id="tsuzuri" Version="${version}" TargetPlatform="${identityHost}" /></Metadata></PackageManifest>`);
  add('extension/toolchain/manifest.json', JSON.stringify({ platform, arch: toolsArch ?? arch, id: 'a'.repeat(64) }));
  add('extension/changelog.md', `# Changelog\n\n## ${version} - 2026-10-21\n\n- Release.\n`);
  add('extension/dist/extension.js', '');
  for (const name of ['tsuzuri', 'tsuzuri-clang', 'clang', 'wasm-ld']) {
    add(`extension/toolchain/bin/${name}${platform === 'win32' ? '.exe' : ''}`, 'test tool');
  }
  if (host !== 'win32-arm64') add('extension/resources/codelldb.vsix', 'test debugger');
  zip.end();
  await pipeline(zip.outputStream, createWriteStream(file));
  const digest = createHash('sha256').update(await readFile(file)).digest('hex');
  await writeFile(`${file}.sha256`, `${digest}  ${path.basename(file)}\n`);
  return file;
}

test('release targets match the supported distribution hosts', () => {
  assert.deepEqual([...Object.keys(platforms), ...Object.keys(mingwChecksums)].sort(), hosts);
});

test('the verifier has no publish command', () => {
  const result = spawnSync(process.execPath, [fileURLToPath(new URL('./verify-release.mjs', import.meta.url)), 'publish', version],
    { encoding: 'utf8', timeout: 30000 });
  assert.ifError(result.error);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /Usage: verify-release\.mjs/);
});

test('catalog visibility is not completion until every exact target is validated and no orphaned targets remain', async () => {
  const metadata = {
    publisher: { publisherName: 'tmidorikawa' }, extensionName: 'tsuzuri',
    versions: [
      ...hosts.map(targetPlatform => ({ version, targetPlatform, flags: 1 })),
      ...hosts.map(targetPlatform => ({ version: '0.1.6', targetPlatform, flags: 1 })),
    ],
  };
  const latestMetadata = {
    publisher: { publisherName: 'tmidorikawa' }, extensionName: 'tsuzuri',
    versions: hosts.map(targetPlatform => ({ version, targetPlatform, flags: 1 })),
  };
  assert.equal(verifyMarketplace(version, metadata).validated, true);
  assert.equal(verifyMarketplace(version, metadata, latestMetadata).validated, true);

  const copy = () => structuredClone(metadata);
  const copyLatest = () => structuredClone(latestMetadata);

  const pending = copy();
  pending.versions[1].flags = 0;
  assert.throws(() => verifyMarketplace(version, pending, latestMetadata), /validation is not complete: linux-arm64/);

  const missing = copy();
  missing.versions.splice(0, 1);
  assert.throws(() => verifyMarketplace(version, missing, latestMetadata), /target set mismatch/);

  const unsupported = copy();
  unsupported.versions.push({ version, targetPlatform: 'darwin-x64', flags: 1 });
  assert.throws(() => verifyMarketplace(version, unsupported, latestMetadata), /target set mismatch/);

  const orphanedDarwinX64 = copy();
  orphanedDarwinX64.versions.push({ version: '0.1.1', targetPlatform: 'darwin-x64', flags: 1 });
  assert.throws(() => verifyMarketplace(version, orphanedDarwinX64, latestMetadata),
    /unsupported or orphaned targetPlatform entries: 0\.1\.1@darwin-x64/);

  const orphanedUniversal = copy();
  orphanedUniversal.versions.push({ version: '0.1.0', targetPlatform: undefined, flags: 1 });
  assert.throws(() => verifyMarketplace(version, orphanedUniversal, latestMetadata),
    /unsupported or orphaned targetPlatform entries: 0\.1\.0@universal/);

  const latestOrphaned = copyLatest();
  latestOrphaned.versions.push({ version: '0.1.1', targetPlatform: 'darwin-x64', flags: 1 });
  assert.throws(() => verifyMarketplace(version, metadata, latestOrphaned),
    /IncludeLatestVersionOnly has unsupported or orphaned targetPlatform entries: 0\.1\.1@darwin-x64/);

  const latestStaleHost = copyLatest();
  latestStaleHost.versions[0].version = '0.1.6';
  assert.throws(() => verifyMarketplace(version, metadata, latestStaleHost),
    /IncludeLatestVersionOnly returned unexpected version entries: 0\.1\.6@darwin-arm64/);

  const latestMissingHost = copyLatest();
  latestMissingHost.versions.splice(0, 1);
  assert.throws(() => verifyMarketplace(version, metadata, latestMissingHost),
    /IncludeLatestVersionOnly target set mismatch/);

  const latestUnvalidated = copyLatest();
  latestUnvalidated.versions[0].flags = 0;
  assert.throws(() => verifyMarketplace(version, metadata, latestUnvalidated),
    /IncludeLatestVersionOnly validation is not complete: darwin-arm64/);

  let capturedRequest;
  const deserializedLatest = await queryLatestMarketplace(async (url, options) => {
    capturedRequest = { url, options, body: JSON.parse(options.body) };
    return {
      ok: true,
      status: 200,
      statusText: 'OK',
      json: async () => ({
        results: [{
          extensions: [{
            publisher: { publisherName: 'tmidorikawa' },
            extensionName: 'tsuzuri',
            versions: hosts.map(targetPlatform => ({ version, targetPlatform, flags: 'validated' })),
          }],
        }],
      }),
    };
  });
  assert.match(capturedRequest.url, /\/_apis\/public\/gallery\/extensionquery$/);
  assert.equal(capturedRequest.options.method, 'POST');
  assert.deepEqual(capturedRequest.body.filters[0].criteria, [
    { filterType: 7, value: 'tmidorikawa.tsuzuri' },
    { filterType: 8, value: 'Microsoft.VisualStudio.Code' },
  ]);
  assert.equal(capturedRequest.body.flags, 0x200 | 0x10 | 0x20);
  assert.equal(verifyMarketplace(version, metadata, deserializedLatest).validated, true);

  const wrongPublisher = copy();
  wrongPublisher.publisher.publisherName = 'other';
  assert.throws(() => verifyMarketplace(version, wrongPublisher), /publisher mismatch/);
  assert.throws(() => verifyMarketplace('1.2.3-preview', metadata), /explicit stable version/);
});

test('VSIX checks cover all five hosts and reject mismatched or tampered packages', async () => {
  const scratch = path.join(repository, 'target', 'scratch');
  await mkdir(scratch, { recursive: true });
  const directory = await mkdtemp(path.join(scratch, 'vscode-publish-skill-'));
  try {
    for (const host of hosts) {
      const file = await fixture(directory, host);
      assert.equal((await verifyVsix(version, host, file)).host, host);
    }
    const file = path.join(directory, `tsuzuri-${version}-linux-x64.vsix`);
    await writeFile(`${file}.sha256`, `${'0'.repeat(64)}  ${path.basename(file)}\n`);
    await assert.rejects(verifyVsix(version, 'linux-x64', file), /SHA-256 mismatch/);
    const mismatch = path.join(directory, 'mismatch');
    await mkdir(mismatch);
    const bad = await fixture(mismatch, 'darwin-arm64', 'x64');
    await assert.rejects(verifyVsix(version, 'darwin-arm64', bad), /Toolchain host mismatch/);
    await assert.rejects(verifyVsix(version, 'darwin-x64', bad), /Unsupported release target/);
    const identity = path.join(directory, 'identity');
    await mkdir(identity);
    const wrongTarget = await fixture(identity, 'darwin-arm64', undefined, 'linux-x64');
    await assert.rejects(verifyVsix(version, 'darwin-arm64', wrongTarget), /VSIX TargetPlatform mismatch/);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
