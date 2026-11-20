import { spawnSync } from 'node:child_process';
import { mkdir, mkdtemp, readFile, writeFile, rm } from 'node:fs/promises';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';
import { assertSupportedHost } from '../../scripts/toolchain/bundle.mjs';

assertSupportedHost();
const { downloadAndUnzipVSCode, resolveCliArgsFromVSCodeExecutablePath, runTests } = await import('@vscode/test-electron');
const extension = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const cache = path.join(extension, '.vscode-test');
await mkdir(cache, { recursive: true });
const directory = await mkdtemp(path.join(cache, 'integration-'));
const project = path.join(directory, 'project # [one]');
const other = path.join(directory, 'other');
// Packages for the run results of G39: one that may not use IO (E1036), and one with a path dependency outside its root.
const packages = path.join(directory, 'packages');
// Folders that are not projects, with a package below them (G40): one whose package reads only with its manifest, and
// one whose files all have namespace headers.
const loose = path.join(directory, 'loose');
const loose2 = path.join(directory, 'loose2');
const extensions = path.join(directory, 'extensions');
const userData = path.join(directory, 'user-data');
const executable = process.env.VSCODE_EXECUTABLE_PATH ?? await downloadAndUnzipVSCode('1.103.2');
try {
  await mkdir(path.join(project, 'Geometry'), { recursive: true });
  await mkdir(path.join(project, 'Shapes'), { recursive: true });
  await mkdir(other);
  await writeFile(path.join(project, 'Main.tz'),
    'def calculate :: i64 -> i64 = \\value ->\n    let label = "ok"\n    let result = value + Geometry::Point.offset() + label.length - 2\n    result\n\ndef main :: unit -> i32 = \\() ->\n    do! IO.write_line (calculate 2)\n    0\n\ntest "same" = assert true\ntest "same" = assert false\n');
  await writeFile(path.join(project, 'Geometry', 'Point.tz'), 'def offset :: i64 = 40\n');
  await writeFile(path.join(project, 'Shapes', 'Circle.tz'), 'namespace Demo::Shapes\n\ndef radius :: i64 = 0\n');
  await writeFile(path.join(project, 'Report.tz'), 'namespace Demo\n\nusing Demo::Shapes\n\ndef total :: i64 = Circle.radius() + Shapes::Circle.radius()\n');
  await writeFile(path.join(other, 'Main.tz'), 'def main :: unit -> i32 = \\() -> 7\n');
  for (const folder of ['capabilities/Extra', 'app', 'lib/Extra']) { await mkdir(path.join(packages, folder), { recursive: true }); }
  await writeFile(path.join(packages, 'capabilities', 'Tsuzuri.toml'), '[package]\nname = "capabilities"\nversion = "0.1.0"\n\n[capabilities]\nallow = []\n');
  await writeFile(path.join(packages, 'capabilities', 'Main.tz'), 'def main :: unit -> i32 = \\() ->\n    do! IO.write_line "denied"\n    0\n\ntest "runs" = assert true\n');
  await writeFile(path.join(packages, 'capabilities', 'Extra', 'Note.tz'), 'def note :: i64 = 1\n');
  await writeFile(path.join(packages, 'app', 'Tsuzuri.toml'), '[package]\nname = "app"\nversion = "0.1.0"\n\n[dependencies]\nlib = { path = "../lib" }\n');
  await writeFile(path.join(packages, 'app', 'Main.tz'), 'def main :: unit -> i32 = \\() -> 0\n\ntest "uses lib" = assert (Lib::Calc.twice 2 == 4)\n');
  await writeFile(path.join(packages, 'app', 'notes.txt'), 'Not a source.\n');
  await writeFile(path.join(packages, 'lib', 'Tsuzuri.toml'), '[package]\nname = "lib"\nversion = "0.1.0"\n');
  await writeFile(path.join(packages, 'lib', 'Calc.tz'), 'export def twice :: i64 -> i64 = \\value -> value * 2 + Shapes.base()\n');
  await writeFile(path.join(packages, 'lib', 'Shapes.tz'), 'export def base :: bool = true\n');
  await writeFile(path.join(packages, 'lib', 'Extra', 'Note.tz'), 'export def note :: i64 = 1\n');
  for (const [folder, util, notes] of [[loose, 'def value :: i32 = 0\n', 'fn n() -> i64 { true }\n'], [loose2, 'namespace App\n\ndef value :: i32 = 0\n', 'fn n() -> i64 { 1 }\n']]) {
    await mkdir(path.join(folder, 'scratch'), { recursive: true });
    await mkdir(path.join(folder, 'App'));
    await writeFile(path.join(folder, 'scratch', 'Try.tz'), 'fn t() -> i64 { 1 }\n');
    await writeFile(path.join(folder, 'App', 'Tsuzuri.toml'), '[package]\nname = "app"\nversion = "0.1.0"\nnamespace = "App"\n');
    await writeFile(path.join(folder, 'App', 'Main.tz'), 'namespace App\n\ndef main :: unit -> i32 = \\() -> App::Util.value()\n');
    await writeFile(path.join(folder, 'App', 'Util.tz'), util);
    await writeFile(path.join(folder, 'Notes.tz'), notes);
  }
  const workspace = path.join(directory, 'integration.code-workspace');
  await writeFile(workspace, JSON.stringify({ folders: [{ path: project }, { path: other }, { path: packages }, { path: loose }, { path: loose2 }], settings: { 'telemetry.telemetryLevel': 'off', 'chat.disableAIFeatures': true, 'update.mode': 'none', 'extensions.autoUpdate': false, 'security.workspace.trust.enabled': false } }));
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
    extensionTestsEnv: { TSUZURI_TEST_PROJECT: project, TSUZURI_TEST_OTHER: other, TSUZURI_TEST_PACKAGES: packages, TSUZURI_TEST_LOOSE: loose, TSUZURI_TEST_LOOSE2: loose2, TSUZURI_TEST_INSTALLED: process.argv.includes('--installed') ? '1' : '0' },
    launchArgs: [workspace, '--extensions-dir', extensions, '--user-data-dir', userData, '--skip-welcome', '--skip-release-notes', '--disable-workspace-trust', '--disable-gpu', '--no-sandbox'],
  });
} finally {
  // Windows can hold the project directory briefly after VS Code exits (EBUSY), as in smoke.mjs.
  await rm(directory, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 });
}
