import { spawnSync } from 'node:child_process';
import { mkdir, mkdtemp, readFile, writeFile, rm } from 'node:fs/promises';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';
import { downloadAndUnzipVSCode, resolveCliArgsFromVSCodeExecutablePath, runTests } from '@vscode/test-electron';

const extension = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const cache = path.join(extension, '.vscode-test');
await mkdir(cache, { recursive: true });
const directory = await mkdtemp(path.join(cache, 'integration-'));
const project = path.join(directory, 'project # [one]');
const other = path.join(directory, 'other');
const extensions = path.join(directory, 'extensions');
const userData = path.join(directory, 'user-data');
const executable = process.env.VSCODE_EXECUTABLE_PATH ?? await downloadAndUnzipVSCode('1.103.2');
try {
  await mkdir(path.join(project, 'Geometry'), { recursive: true });
  await mkdir(path.join(project, 'Shapes'), { recursive: true });
  await mkdir(other);
  await writeFile(path.join(project, 'Main.tz'),
    'def calculate :: i64 -> i64 = \\value ->\n    let result = value + Geometry::Point.offset()\n    result\n\ndef main :: i64 = calculate 2\n\ntest "same" = assert true\ntest "same" = assert false\n');
  await writeFile(path.join(project, 'Geometry', 'Point.tz'), 'def offset :: i64 = 40\n');
  await writeFile(path.join(project, 'Shapes', 'Circle.tz'), 'namespace Demo::Shapes\n\ndef radius :: i64 = 0\n');
  await writeFile(path.join(project, 'Report.tz'), 'namespace Demo\n\nusing Demo::Shapes\n\ndef total :: i64 = Circle.radius() + Shapes::Circle.radius()\n');
  await writeFile(path.join(other, 'Main.tz'), 'def main :: i64 = 7\n');
  const workspace = path.join(directory, 'integration.code-workspace');
  await writeFile(workspace, JSON.stringify({ folders: [{ path: project }, { path: other }], settings: { 'telemetry.telemetryLevel': 'off', 'chat.disableAIFeatures': true, 'update.mode': 'none', 'extensions.autoUpdate': false, 'security.workspace.trust.enabled': false } }));
  let developmentPath = extension;
  if (process.argv.includes('--installed')) {
    const [cli, ...args] = resolveCliArgsFromVSCodeExecutablePath(executable, { reuseMachineInstall: true });
    const manifest = JSON.parse(await readFile(path.join(extension, 'package.json'), 'utf8'));
    const artifact = path.join(extension, 'dist', `tsuzuri-${manifest.version}-${process.platform}-${process.arch}.vsix`);
    const result = spawnSync(cli, [...args, '--extensions-dir', extensions, '--user-data-dir', userData, '--install-extension', artifact, '--force'],
      { stdio: 'inherit', shell: process.platform === 'win32' });
    if (result.error || result.status !== 0) { throw result.error ?? new Error(`VSIX installation failed: ${result.status}`); }
    developmentPath = path.join(directory, 'test-harness');
    await mkdir(developmentPath);
    await writeFile(path.join(developmentPath, 'package.json'), JSON.stringify({ name: 'tsuzuri-integration-tests', publisher: 'tsuzuri', version: '0.0.1', engines: { vscode: '^1.103.0' } }));
  }
  await runTests({
    vscodeExecutablePath: executable,
    extensionDevelopmentPath: developmentPath,
    extensionTestsPath: path.join(extension, 'out', 'test', 'extension.test.js'),
    extensionTestsEnv: { TSUZURI_TEST_PROJECT: project, TSUZURI_TEST_OTHER: other, TSUZURI_TEST_INSTALLED: process.argv.includes('--installed') ? '1' : '0' },
    launchArgs: [workspace, '--extensions-dir', extensions, '--user-data-dir', userData, '--skip-welcome', '--skip-release-notes', '--disable-workspace-trust', '--disable-gpu', '--no-sandbox'],
  });
} finally {
  await rm(directory, { recursive: true, force: true });
}
