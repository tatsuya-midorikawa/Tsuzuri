// G35: the drift checks between the compiler and its documents (`scripts/check-cli-docs.mjs`), on fake
// `--help` text, fake pages and fake sources. No compiler is needed.
//
// usage: node tests/check_cli_docs.mjs

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import {
    EXEMPT_FLAGS,
    checkCli,
    checkCodes,
    checkStd,
    checkVersion,
    documentedCodes,
    extractCli,
    scanEmittedCodes,
    stdPage,
    versionMentions,
} from '../scripts/check-cli-docs.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const test = (name, check) => {
    check();
    console.log(`check-cli-docs: ${name} passed`);
};

test('extractCli reads flags, commands and variables', () => {
    const help = '  tsuzuri check x [--json]\n  tsuzuri [build] y [options]\n    --wasm-feature <name>  set TSUZURI_SYSROOT for --sysroot\n  -O0, -O1\n';
    assert.deepEqual(extractCli(help), {
        flags: ['--json', '--sysroot', '--wasm-feature'],
        commands: ['build', 'check'],
        variables: ['TSUZURI_SYSROOT'],
    });
    assert.ok('--sysroot' in EXEMPT_FLAGS);
});
test('checkCli names what a page lacks', () => {
    const cli = { flags: ['--json', '--sysroot'], commands: ['build'], variables: ['TSUZURI_SYSROOT'] };
    const full = 'tsuzuri build --json TSUZURI_SYSROOT';
    assert.deepEqual(checkCli(cli, { 'README.md': full, 'option.md': full }), []);
    assert.deepEqual(checkCli(cli, { 'README.md': 'tsuzuri buildx TSUZURI_SYSROOT', 'option.md': full }), [
        'README.md lacks flag --json',
        'README.md lacks command build',
    ]);
    // A longer token does not document a shorter one.
    assert.deepEqual(checkCli({ flags: ['--rev'], commands: [], variables: ['TSUZURI_KG'] }, { 'README.md': '--reverse TSUZURI_KG_DIR', 'option.md': '`--rev` and TSUZURI_KG.' }), [
        'README.md lacks flag --rev',
        'README.md lacks variable TSUZURI_KG',
    ]);
    assert.deepEqual(checkCli(cli, { 'README.md': 'tsuzuri [build] --json', 'option.md': '--json' }), [
        'README.md lacks variable TSUZURI_SYSROOT',
        'option.md lacks variable TSUZURI_SYSROOT',
    ]);
});
test('documentedCodes expands ranges and joined cells', () => {
    assert.deepEqual(documentedCodes('| `E1001`–`E1003` | x |\n| `E2001` / `E2002` | y |\n| 項目 | 説明 |\n'), ['E1001', 'E1002', 'E1003', 'E2001', 'E2002']);
});
test('checkCodes reports both directions', () => {
    assert.deepEqual(checkCodes(['E1001', 'W2001'], { 'p.md': '| `E1001` | a |\n| `E9999` | b |\n' }), [
        'W2001 is emitted but missing from p.md',
        'E9999 is in p.md but no source emits it',
    ]);
});
test('scanEmittedCodes reads the code literals of sources', () => {
    const source = 'Diagnostic::new("E1001", message)\nlet json = "{{\\"severity\\":\\"warning\\",\\"code\\":\\"W2001\\"}}";\nEdition::parse("E2026")\n';
    assert.deepEqual(scanEmittedCodes([source]), ['E1001', 'W2001']);
});
test('versionMentions and checkVersion', () => {
    assert.deepEqual(versionMentions('Tsuzuri 0.1.0 と tsuzuri v0.2.1 と Tsuzuri 1.0'), ['0.1.0', '0.2.1']);
    assert.deepEqual(checkVersion('0.1.0', 'tsuzuri 0.1.0', [{ file: 'a.md', version: '0.1.0' }]), []);
    assert.deepEqual(checkVersion('0.1.0', 'tsuzuri 0.2.0', [{ file: 'a.md', version: '0.0.9' }]), [
        'a.md says 0.0.9, Cargo.toml says 0.1.0',
        "--version prints 'tsuzuri 0.2.0', expected 'tsuzuri 0.1.0'",
    ]);
});
test('checkStd looks for each function on its module page', () => {
    assert.equal(stdPage('MatrixView'), 'built-in-types-and-modules/matrix-view.md');
    assert.equal(stdPage('Array'), 'built-in-types-and-modules/array.md');
    const pages = { 'built-in-types-and-modules/sample.md': 'Use `a` or `Sample.b`.' };
    const read = page => pages[page] ?? null;
    assert.deepEqual(checkStd({ Sample: [{ name: 'a' }, { name: 'b' }, { name: 'c' }], Empty: [] }, read), ['Sample.c is not mentioned in built-in-types-and-modules/sample.md']);
    assert.deepEqual(checkStd({ Other: [{ name: 'a' }] }, read), ['Other has 1 public functions but no page in STD_PAGES']);
    // `Sample.bc` does not mention `Sample.b`.
    assert.deepEqual(checkStd({ Sample: [{ name: 'b' }] }, () => 'Use `Sample.bc`.'), ['Sample.b is not mentioned in built-in-types-and-modules/sample.md']);
    assert.deepEqual(checkStd({ Map: [{ name: 'get' }] }, () => 'Use HashMap.get.'), ['Map.get is not mentioned in built-in-types-and-modules/map.md']);
});
test('the command line reports usage errors with 2', () => {
    const script = join(root, 'scripts/check-cli-docs.mjs');
    const unknown = spawnSync(process.execPath, [script, '--only', 'nope'], { encoding: 'utf8' });
    assert.equal(unknown.status, 2);
    assert.match(unknown.stderr, /^check-cli-docs: unknown check 'nope'/);
    const missing = spawnSync(process.execPath, [script, '--bin', join(root, 'target/no-such-compiler')], { encoding: 'utf8' });
    assert.equal(missing.status, 2);
    assert.match(missing.stderr, /^check-cli-docs: compiler not found: /);
    const flag = spawnSync(process.execPath, [script, '--x'], { encoding: 'utf8' });
    assert.equal(flag.status, 2);
    assert.match(flag.stderr, /^check-cli-docs: unknown option '--x'/);
});
