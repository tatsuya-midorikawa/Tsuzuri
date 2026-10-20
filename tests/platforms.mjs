import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { platforms, mingwChecksums } from '../scripts/toolchain/bundle.mjs';
import { debuggerChecksums } from '../vsc/scripts/toolchain.mjs';

const hosts = ['darwin-arm64', 'linux-arm64', 'linux-x64', 'win32-arm64', 'win32-x64'];
assert.deepEqual([...Object.keys(platforms), ...Object.keys(mingwChecksums)].sort(), hosts);
assert.deepEqual(Object.keys(debuggerChecksums).sort(), hosts.filter(host => host !== 'win32-arm64'));

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
console.log('platforms: exactly five toolchain/CI hosts; Windows Arm64 has no bundled debugger');
