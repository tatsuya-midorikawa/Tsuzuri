import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { assertSupportedHost, platforms, mingwChecksums } from '../scripts/toolchain/bundle.mjs';
import { debuggerChecksums } from '../vsc/scripts/toolchain.mjs';

const hosts = ['darwin-arm64', 'linux-arm64', 'linux-x64', 'win32-arm64', 'win32-x64'];
assert.deepEqual([...Object.keys(platforms), ...Object.keys(mingwChecksums)].sort(), hosts);
assert.deepEqual(Object.keys(debuggerChecksums).sort(), hosts.filter(host => host !== 'win32-arm64'));

for (const host of hosts) assert.doesNotThrow(() => assertSupportedHost(host));
for (const host of ['darwin-x64', 'win32-ia32', 'linux-riscv64', 'freebsd-x64', 'android-arm64', 'toString', '__proto__']) {
  assert.throws(() => assertSupportedHost(host), { message: `Unsupported toolchain host: ${host}. Supported hosts: ${hosts.join(', ')}.` });
}
for (const [script, flags] of [
  ['package', []], ['package', ['--verify']], ['package', ['--scan']],
  ['toolchain', []], ['toolchain', ['--verify']], ['toolchain', ['--resources']],
  ['test-extension', []], ['test-extension', ['--installed']],
]) {
  const url = new URL(`../vsc/scripts/${script}.mjs`, import.meta.url);
  const result = spawnSync(process.execPath, ['--input-type=module', '-e', `
    Object.defineProperty(process, 'platform', { value: 'darwin' });
    Object.defineProperty(process, 'arch', { value: 'x64' });
    process.argv = [process.execPath, ${JSON.stringify(fileURLToPath(url))}, ...${JSON.stringify(flags)}];
    await import(${JSON.stringify(url.href)});
  `], { encoding: 'utf8', timeout: 30_000 });
  assert.ifError(result.error);
  assert.equal(result.status, 1, `${script} ${flags.join(' ')}: ${result.stdout}\n${result.stderr}`);
  assert.ok(result.stderr.includes(`Unsupported toolchain host: darwin-x64. Supported hosts: ${hosts.join(', ')}.`), result.stderr);
}

const bundleUrl = new URL('../scripts/toolchain/bundle.mjs', import.meta.url);
const mismatchedClang = spawnSync(process.execPath, ['--input-type=module', '-e', `
  import assert from 'node:assert/strict';
  import childProcess from 'node:child_process';
  import { syncBuiltinESMExports } from 'node:module';
  import path from 'node:path';
  Object.defineProperty(process, 'platform', { value: 'darwin' });
  Object.defineProperty(process, 'arch', { value: 'arm64' });
  process.env.LLVM_PREFIX = path.resolve('must-not-be-used');
  childProcess.execFileSync = (command, args) => {
    if (command === 'rustc') return 'host: aarch64-apple-darwin\\n';
    if (args[0] === '--version') return 'clang version 21.1.8\\n';
    if (args[0] === '-dumpmachine') return 'x86_64-apple-darwin27.0.0\\n';
    throw new Error('Unexpected native tool execution');
  };
  syncBuiltinESMExports();
  const { bundle } = await import(${JSON.stringify(bundleUrl.href)});
  await assert.rejects(bundle(path.resolve('must-not-be-created')), {
    message: 'The macOS toolchain requires an Apple Silicon (ARM64) Clang; got x86_64-apple-darwin27.0.0.',
  });
`], { encoding: 'utf8', timeout: 30_000 });
assert.ifError(mismatchedClang.error);
assert.equal(mismatchedClang.status, 0, mismatchedClang.stderr);

const runners = {
  'darwin-arm64': 'macos-15',
  'linux-arm64': 'ubuntu-24.04-arm',
  'linux-x64': 'ubuntu-24.04',
  'win32-arm64': 'windows-11-arm',
  'win32-x64': 'windows-2022',
};
for (const [workflow, expected] of [
  ['targets', hosts.filter(host => !host.startsWith('win32-'))],
  ['windows', hosts.filter(host => host.startsWith('win32-'))],
  ['vscode', hosts],
]) {
  const source = readFileSync(new URL(`../.github/workflows/${workflow}.yml`, import.meta.url), 'utf8');
  const matrix = [...source.matchAll(/^\s+- os: (\S+)\r?\n\s+(?:host|target): (\S+)/gm)]
    .map(([, os, host]) => [host, os]).sort(([left], [right]) => left.localeCompare(right));
  assert.deepEqual(matrix, expected.map(host => [host, runners[host]]), workflow);
}
for (const name of ['Development', 'Publishing']) {
  const source = readFileSync(new URL(`../vsc/${name}.md`, import.meta.url), 'utf8');
  const documented = [...source.matchAll(/^\| `?((?:darwin|linux|win32)-[a-z0-9]+)`? \|/gm)].map(([, host]) => host).sort();
  assert.deepEqual(documented, hosts, name);
}
console.log('platforms: exactly five toolchain/CI hosts; macOS requires Apple Silicon Clang; unsupported VSIX build/verify and IDE test hosts are refused; Windows Arm64 has no bundled debugger');
