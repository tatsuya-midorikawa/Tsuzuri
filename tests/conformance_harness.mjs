// G33: the self-tests of the conformance harness (`scripts/conformance.mjs`) and of the rule checker
// (`scripts/check-spec.mjs`). The inputs are strings and the small tree
// `tests/fixtures/conformance_mini/good/`; every expectation is written by hand.
//
// usage: node tests/conformance_harness.mjs [compiler] [part of a test name]

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { parseTest, splitFooter, tokens, TRAP_KINDS } from '../scripts/conformance/directives.mjs';
import { discoverTests } from '../scripts/conformance/discover.mjs';
import { checkCli, matchDiagnostic, summaryOf, trapLine } from '../scripts/conformance/expect.mjs';
import { headingsOf, readRules } from '../scripts/conformance/spec.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const compiler = resolve(process.argv[2] ?? process.env.TSUZURI_BIN ?? join(root, 'target/release/tsuzuri'));
const only = process.argv[3];
const good = join(root, 'tests/fixtures/conformance_mini/good');
const scratch = mkdtempSync(join(tmpdir(), 'tsuzuri-conformance-harness-'));
const tests = [];

function test(name, body) {
    tests.push({ name, body });
}

let counter = 0;
function fresh() {
    counter += 1;
    const dir = join(scratch, `t${counter}`);
    mkdirSync(dir, { recursive: true });
    return dir;
}

/** A copy of `good/` with `from` replaced by `to` once in `file` (`null` deletes the file). */
function mutate(file, from, to) {
    const dir = fresh();
    cpSync(good, dir, { recursive: true });
    if (file === null) return dir;
    const path = join(dir, file);
    if (to === null && from === null) {
        rmSync(path, { recursive: true, force: true });
        return dir;
    }
    const text = readFileSync(path, 'utf8');
    assert.ok(text.includes(from), `${file} has no ${JSON.stringify(from)}`);
    writeFileSync(path, text.replace(from, to));
    return dir;
}

/** A tree with the given test files (path under tests/conformance → text). */
function tree(files) {
    const dir = fresh();
    for (const [path, text] of Object.entries(files)) {
        const full = join(dir, 'tests/conformance', path);
        mkdirSync(dirname(full), { recursive: true });
        writeFileSync(full, text);
    }
    return dir;
}

function node(script, args, env = {}) {
    const result = spawnSync(process.execPath, [join(root, 'scripts', script), ...args], {
        encoding: 'utf8',
        env: { ...process.env, ...env },
    });
    return { status: result.status, stdout: result.stdout, stderr: result.stderr };
}

function conformance(args, env) {
    return node('conformance.mjs', [compiler, ...args], env);
}

function checkSpec(args) {
    return node('check-spec.mjs', args);
}

function names(errors) {
    return errors.map(error => error.name);
}

/** The errors of `check-spec --json`; `coverage_block` only when `block` (a test that changes the
 * tests or the rules also changes the coverage). */
function specErrors(dir, extra = [], block = false) {
    const result = checkSpec(['--root', dir, '--json', ...extra]);
    return result.stdout
        .split('\n')
        .filter(Boolean)
        .map(line => JSON.parse(line))
        .filter(row => row.name && (block || row.name !== 'coverage_block'));
}

// Directives.

test('footer_splits_directives_from_body', () => {
    const { start, directives } = splitFooter('let x = 1\nx\n\n// rule: OWN-001\n// expect: check-ok\n');
    assert.equal(start, 2);
    assert.deepEqual(directives, [
        { key: 'rule', value: 'OWN-001', line: 4 },
        { key: 'expect', value: 'check-ok', line: 5 },
    ]);
});

test('footer_stops_at_plain_comment_or_code', () => {
    let footer = splitFooter('x\n// rule: OWN-001\n// done\n');
    assert.equal(footer.start, 3);
    assert.deepEqual(footer.directives, []);
    footer = splitFooter('x\n// expect: check-ok\ny\n');
    assert.deepEqual(footer.directives, []);
    footer = splitFooter('x\n// foo: 1\n');
    assert.deepEqual(footer.directives, [{ key: 'foo', value: '1', line: 2 }]);
});

test('tokens_words_and_json_strings', () => {
    assert.deepEqual(tokens('stdout "a b\\n" x'), [
        { string: false, text: 'stdout' },
        { string: true, text: 'a b\n' },
        { string: false, text: 'x' },
    ]);
});

test('tokens_reject_bad_strings', () => {
    assert.throws(() => tokens('"abc'), /unterminated/);
    assert.throws(() => tokens('"a"b'), /followed by a space/);
    assert.throws(() => tokens('"\\x"'), /invalid JSON/);
});

function parsed(lines) {
    return parseTest(`0\n\n${lines.map(line => `// ${line}`).join('\n')}\n`);
}

function errorNames(lines) {
    return names(parsed(lines).errors);
}

test('parse_check_options', () => {
    const { expects, errors } = parsed(['rule: OWN-001', 'expect: check-error E1012 line=3 file=Lib.tz kind=use_after_move']);
    assert.deepEqual(errors, []);
    assert.equal(expects[0].code, 'E1012');
    assert.equal(expects[0].line, 3);
    assert.equal(expects[0].file, 'Lib.tz');
    assert.equal(expects[0].diagKind, 'use_after_move');
    const withColumn = parsed(['rule: DIA-004', 'expect: check-warning W1001 line=2 column=9 message "prefix it"']);
    assert.deepEqual(withColumn.errors, []);
    assert.equal(withColumn.expects[0].column, 9);
    assert.equal(withColumn.expects[0].message, 'prefix it');
    for (const bad of ['check-error E1012 line=1 line=2', 'check-error E1012 line=0', 'check-error E1012 file=../x', 'check-error E1012 file=/x', 'check-error W1001', 'check-warning E1012', 'check-error E12', 'check-error E1012 column=2', 'check-error E1012 line=1 column=0', 'check-error E1012 message "x" line=1', 'check-error E1012 message x']) {
        assert.deepEqual(errorNames(['rule: OWN-001', `expect: ${bad}`]), ['bad_argument'], bad);
    }
});

test('parse_trap_with_stdout_tail', () => {
    const { expects, errors } = parsed(['rule: EVL-001', 'expect: trap kind=div_zero stdout "1\\n"']);
    assert.deepEqual(errors, []);
    assert.equal(expects[0].trapKind, 'div_zero');
    assert.equal(expects[0].stdout, '1\n');
    for (const bad of ['trap kind=nope', 'trap', 'trap kind=bounds stdout', 'trap stdout "x" kind=bounds']) {
        assert.deepEqual(errorNames(['rule: EVL-001', `expect: ${bad}`]), ['bad_argument'], bad);
    }
});

test('parse_cli_mode', () => {
    const { expects, errors, mode, cli } = parsed(['rule: CLI-001', 'cli: check $DIR --json', 'expect: exit 1', 'expect: stderr-has "x"', 'expect: file-has out.h "y"', 'expect: stderr-lines 1']);
    assert.deepEqual(errors, []);
    assert.equal(mode, 'cli');
    assert.deepEqual(cli, ['check', '$DIR', '--json']);
    assert.equal(expects.length, 4);
    assert.deepEqual(errorNames(['rule: CLI-001', 'cli: check $DIR', 'expect: check-ok']), ['unknown_expectation']);
    assert.deepEqual(errorNames(['rule: CLI-001', 'cli: build $DIR', 'expect: deterministic']), ['bad_argument']);
    assert.deepEqual(errorNames(['rule: CLI-001', 'cli: a "b"', 'expect: exit 0']), ['bad_argument']);
    // A `cli` line that cannot be read is reported once, and `deterministic` does not read its words.
    assert.deepEqual(errorNames(['rule: CLI-001', 'cli: a "b"', 'expect: deterministic']), ['bad_argument']);
});

test('parse_unknown_directive_and_expectation', () => {
    assert.deepEqual(errorNames(['rule: OWN-001', 'expect: check-ok', 'foo: x']), ['unknown_directive']);
    assert.deepEqual(errorNames(['rule: OWN-001', 'expect: frobnicate']), ['unknown_expectation']);
});

test('parse_mixed_modes_and_trap_stdout', () => {
    assert.deepEqual(errorNames(['rule: EVL-001', 'expect: trap kind=assert', 'expect: stdout "x"']), ['mixed_expectations']);
});

test('parse_targets_rules', () => {
    assert.deepEqual(errorNames(['rule: NUM-001', 'expect: stdout "x"', 'targets: native-O0 wasm-O0']), ['targets_not_allowed']);
    assert.deepEqual(errorNames(['rule: NUM-001', 'expect: tests-pass 1', 'targets: native-O0 wasm-O0']), []);
    assert.deepEqual(errorNames(['rule: NUM-001', 'expect: check-ok', 'targets: native-O0']), ['targets_not_allowed']);
    assert.deepEqual(errorNames(['rule: CLI-001', 'cli: check $DIR', 'expect: exit 0', 'targets: native-O0']), ['targets_not_allowed']);
    assert.deepEqual(errorNames(['rule: NUM-001', 'expect: tests-pass 1', 'targets: native-O0 native-O0']), ['bad_argument']);
    assert.deepEqual(errorNames(['rule: NUM-001', 'expect: tests-pass 1', 'targets: x86']), ['bad_argument']);
});

test('parse_requires_rule_and_expect', () => {
    assert.deepEqual(errorNames(['note: x']).sort(), ['missing_expect', 'missing_rule']);
});

test('parse_rule_id_format', () => {
    for (const id of ['OWN-000', 'own-001', 'ABC-001', 'OWN-1']) {
        assert.deepEqual(errorNames([`rule: ${id}`, 'expect: check-ok']), ['rule_format'], id);
    }
    assert.deepEqual(parsed(['rule: OWN-001', 'rule: NUM-002', 'expect: check-ok']).rules, ['OWN-001', 'NUM-002']);
});

// Matching.

test('match_diagnostic_fields', () => {
    const d = { severity: 'error', code: 'E1012', message: 'm', path: '/t/Main.tz', span: { line: 5 } };
    const e = extra => ({ severity: 'error', code: 'E1012', ...extra });
    assert.equal(matchDiagnostic(d, e({})), true);
    assert.equal(matchDiagnostic(d, e({ line: 5 })), true);
    assert.equal(matchDiagnostic(d, e({ line: 4 })), false);
    assert.equal(matchDiagnostic(d, e({ file: 'Main.tz' })), true);
    assert.equal(matchDiagnostic(d, e({ file: 'Lib.tz' })), false);
    assert.equal(matchDiagnostic(d, e({ diagKind: 'x' })), false);
    assert.equal(matchDiagnostic({ ...d, kind: 'x' }, e({ diagKind: 'x' })), true);
});

test('match_diagnostic_default_file', () => {
    const d = { severity: 'error', code: 'E1012', path: '/t/Lib.tz', span: { line: 2 } };
    const e = extra => ({ severity: 'error', code: 'E1012', ...extra });
    assert.equal(matchDiagnostic(d, e({ line: 2 })), false);
    assert.equal(matchDiagnostic(d, e({ line: 2, file: 'Lib.tz' })), true);
    assert.equal(matchDiagnostic(d, e({})), true);
    assert.equal(matchDiagnostic({ ...d, path: 'C:\\t\\Lib.tz' }, e({ file: 'Lib.tz' })), true);
});

test('trap_line_parsing', () => {
    const line = 'trap: index out of bounds at /t/Main.tz:3:1';
    const expected = { description: 'index out of bounds', path: '/t/Main.tz', line: 3, column: 1 };
    assert.deepEqual(trapLine(line), expected);
    assert.deepEqual(trapLine(`something else\n${line}\nmore`), expected);
    assert.equal(trapLine('error[E2005]: program terminated'), null);
    assert.equal(trapLine('trap: stack overflow at C:\\t\\Main.tz:3:1').path, 'C:\\t\\Main.tz');
});

test('trap_table_matches_src_trap_rs', () => {
    const source = readFileSync(join(root, 'src/trap.rs'), 'utf8');
    const body = source.slice(source.indexOf('pub fn description'), source.indexOf('pub fn diagnostic_kind'));
    const pairs = [...body.matchAll(/Self::([A-Za-z]+) =>\s*\{?\s*"([^"]+)"/g)].map(match => match[2]);
    assert.equal(pairs.length, Object.keys(TRAP_KINDS).length);
    assert.deepEqual([...pairs].sort(), Object.values(TRAP_KINDS).sort());
});

test('test_summary_parsing', () => {
    const stdout = [
        '{"type":"test","index":0,"name":"a","status":"passed","duration_ms":3}',
        'not json',
        '{"type":"test","index":1,"name":"b","status":"passed","duration_ms":4}',
        '{"type":"summary","passed":2,"failed":0,"ignored":0,"duration_ms":9}',
    ].join('\n');
    assert.deepEqual(summaryOf(stdout), { passed: 2, failed: 0, ignored: 0 });
    assert.equal(summaryOf('{"type":"test"}\n{broken'), null);
    assert.equal(checkCli({ kind: 'stderr-lines', count: 1 }, { status: 0, stdout: '', stderr: 'a\n\n' }, () => null), null);
});

// Discovery and runs.

test('discover_file_and_directory_forms', () => {
    const { tests: found, errors } = discoverTests(good);
    assert.deepEqual(errors, []);
    assert.deepEqual(
        found.map(test => [test.stem, test.form]),
        [
            ['NUM-001-dir', 'dir'],
            ['NUM-001-i8-wraps', 'file'],
            ['OWN-001-use-after-move', 'file'],
        ],
    );
});

test('discover_rejects_bad_entries', () => {
    const check = (path, text, name) => {
        const dir = mutate(null);
        writeFileSync(join(dir, 'tests/conformance', path), text);
        const { errors } = discoverTests(dir);
        if (name === null) assert.deepEqual(errors, [], path);
        else assert.deepEqual(names(errors), [name], path);
    };
    const body = readFileSync(join(good, 'tests/conformance/OWN/OWN-001-use-after-move.tz'), 'utf8');
    check('OWN/readme.txt', 'x', 'unexpected_entry');
    const foo = mutate(null);
    mkdirSync(join(foo, 'tests/conformance/FOO'));
    writeFileSync(join(foo, 'tests/conformance/FOO/x.tz'), body);
    assert.deepEqual(names(discoverTests(foo).errors), ['unexpected_entry']);
    check('OWN/OWN-001-Bad_Name.tz', body, 'test_name');
    check('OWN/.DS_Store', 'x', null);
    check('OWN/NUM-001-i8-wraps.tz', readFileSync(join(good, 'tests/conformance/NUM/NUM-001-i8-wraps.tz'), 'utf8'), 'test_name');
    check('OWN/NUM-009-x.tz', body.replace('OWN-001', 'NUM-009'), 'test_name');
});

function runTree(files, args = []) {
    return conformance(['--root', tree(files), '--targets', 'native', ...args]);
}

const OWN_BODY = 'let a = "x"\nlet b = a\na\n\n// rule: OWN-001\n';

test('run_check_expectations', () => {
    const result = runTree({
        'OWN/OWN-001-check-ok.tz': 'let x = 1\nx\n\n// rule: OWN-001\n// expect: check-ok\n',
        'OWN/OWN-001-check-error.tz': `${OWN_BODY}// expect: check-error E1012 line=3\n`,
        'OWN/OWN-001-check-warning.tz': 'def f :: unit -> i64 = \\() -> {\n    let unused = 1;\n    2\n}\nf ()\n\n// rule: OWN-001\n// expect: check-warning W1001 line=2\n',
        'OWN/OWN-001-check-wrong.tz': `${OWN_BODY}// expect: check-error E1012 line=2\n`,
    });
    assert.equal(result.status, 1, result.stderr);
    assert.match(result.stdout, /^ok OWN-001-check-error /m);
    assert.match(result.stdout, /^ok OWN-001-check-ok /m);
    assert.match(result.stdout, /^ok OWN-001-check-warning /m);
    assert.match(result.stdout, /^FAIL OWN-001-check-wrong /m);
});

test('run_failure_reports_detail', () => {
    let result = runTree({ 'OWN/OWN-001-wrong-code.tz': `${OWN_BODY}// expect: check-error E1003\n` });
    assert.equal(result.status, 1);
    assert.match(result.stdout, /check-error E1003: the first error does not match; got E1012@Main.tz:3:1/);
    result = conformance(['--root', good, '--targets', 'native', '--area', 'OWN', '--timeout', '0.01']);
    assert.equal(result.status, 1);
    assert.match(result.stdout, /timed out after 0\.01 s/);
});

test('run_timeout_stops_the_programs_that_the_compiler_started', () => {
    // `tsuzuri run` starts the program as its child, which inherits the pipes; a program that never ends must
    // not keep the runner waiting after the timeout. The stand-in compiler starts such a child and waits.
    if (process.platform === 'win32') return;
    const dir = fresh();
    const pidFile = join(dir, 'child.pid');
    const fake = join(dir, 'fake-tsuzuri');
    writeFileSync(fake, `#!/bin/sh\nsleep 300 &\necho $! > '${pidFile}'\nwait\n`, { mode: 0o755 });
    const started = Date.now();
    const result = spawnSync(
        process.execPath,
        [join(root, 'scripts', 'conformance.mjs'), fake, '--root', good, '--targets', 'native', '--filter', 'i8-wraps', '--timeout', '0.5'],
        { encoding: 'utf8', timeout: 60000 },
    );
    const seconds = (Date.now() - started) / 1000;
    assert.equal(result.error, undefined, 'the runner hung');
    assert.equal(result.status, 1, result.stdout + result.stderr);
    assert.match(result.stdout, /timed out after 0\.5 s/);
    assert.ok(seconds < 30, `the runner took ${seconds} s`);
    const pid = Number(readFileSync(pidFile, 'utf8'));
    assert.throws(() => process.kill(pid, 0), /ESRCH/, `the program ${pid} still runs`);
});

test('run_stdout_and_trap', () => {
    const result = runTree({
        'EVL/EVL-001-stdout.tz': '1 + 2\n\n// rule: EVL-001\n// expect: stdout "3\\n"\n',
        'EVL/EVL-001-trap.tz': 'let zero = 0\n1 / zero\n\n// rule: EVL-001\n// expect: trap kind=div_zero line=2\n',
        'EVL/EVL-001-trap-stdout.tz': 'def show :: i64 -> i64 = \\n -> {\n    do! IO.write_line (to_string n);\n    n\n}\nlet zero = 0\nshow 1 + 10 / zero\n\n// rule: EVL-001\n// expect: trap kind=div_zero stdout "1\\n"\n',
    });
    assert.equal(result.status, 0, result.stdout + result.stderr);
});

test('run_tests_pass_native', () => {
    assert.equal(conformance(['--root', good, '--targets', 'native', '--filter', 'i8-wraps']).status, 0);
    const dir = mutate('tests/conformance/NUM/NUM-001-i8-wraps.tz', 'tests-pass 1', 'tests-pass 2');
    const result = conformance(['--root', dir, '--targets', 'native', '--filter', 'i8-wraps']);
    assert.equal(result.status, 1);
    assert.match(result.stdout, /passed=1 != 2/);
});

test('run_cli_expectations', () => {
    const result = runTree({
        'DIA/DIA-001-cli-check.tz': `${OWN_BODY.replace('OWN-001', 'DIA-001')}// cli: check $DIR --json\n// expect: exit 1\n// expect: stderr-has "\\"code\\":\\"E1012\\""\n// expect: stdout-empty\n// expect: stderr-lines 1\n`,
        'ABI/ABI-001-header.tz': 'export def add :: i64 -> i64 -> i64 = \\a b -> a + b\ndef hidden :: i64 -> i64 = \\x -> x\n\n// rule: ABI-001\n// cli: build $DIR --emit header -o $DIR/out.h\n// expect: exit 0\n// expect: file-has out.h "#pragma once"\n// expect: file-lacks out.h "hidden"\n',
    });
    assert.equal(result.status, 0, result.stdout + result.stderr);
});

test('run_deterministic', () => {
    const result = runTree({
        'CLI/CLI-001-deterministic.tz': 'export def add :: i64 -> i64 -> i64 = \\a b -> a + b\n\n// rule: CLI-001\n// cli: build $DIR --emit llvm -o $OUT\n// expect: deterministic\n',
    });
    assert.equal(result.status, 0, result.stdout + result.stderr);
});

test('run_edition_manifest', () => {
    let result = runTree({
        'EDN/EDN-001-edition-2026.tz': 'let x = 1\nx\n\n// rule: EDN-001\n// edition: 2026\n// expect: check-ok\n',
        'EDN/EDN-001-edition-2030.tz': 'let x = 1\nx\n\n// rule: EDN-001\n// edition: 2030\n// expect: check-error E0002\n',
    });
    assert.equal(result.status, 0, result.stdout + result.stderr);
    const dir = tree({ 'EDN/EDN-001-conflict/Main.tz': 'let x = 1\nx\n\n// rule: EDN-001\n// edition: 2026\n// expect: check-ok\n' });
    writeFileSync(join(dir, 'tests/conformance/EDN/EDN-001-conflict/Tsuzuri.toml'), '[package]\nname = "x"\nversion = "0.1.0"\n');
    result = conformance(['--root', dir, '--targets', 'native']);
    assert.equal(result.status, 2);
    assert.match(result.stderr, /edition_conflict/);
});

test('cli_selection_and_exit_codes', () => {
    const count = args => conformance(['--root', good, '--targets', 'native', '--list', ...args]).stdout.split('\n').filter(Boolean).length;
    assert.equal(count(['--area', 'OWN']), 1);
    assert.equal(count(['--rule', 'OWN-001']), 2);
    assert.equal(count(['--rule', 'NUM-001']), 2);
    assert.equal(count(['--filter', 'dir']), 1);
    const list = conformance(['--root', good, '--targets', 'native', '--list']).stdout;
    assert.match(list, /^NUM-001-i8-wraps\tNUM-001,OWN-001\ttests-pass$/m);
    assert.equal(conformance(['--root', good, '--area', 'NUM', '--filter', 'zzz']).status, 2);
    assert.equal(conformance(['--root', good, '--bogus']).status, 2);
    assert.equal(runTree({ 'OWN/OWN-001-wrong.tz': `${OWN_BODY}// expect: check-ok\n` }).status, 1);
    assert.equal(conformance(['--root', good, '--targets', 'native']).status, 0);
});

test('cli_json_output', () => {
    const run = () => conformance(['--root', good, '--targets', 'native', '--json']).stdout;
    const first = run();
    const rows = first.trim().split('\n').map(line => JSON.parse(line));
    assert.deepEqual(rows.slice(0, 3).map(row => row.stem), ['NUM-001-dir', 'NUM-001-i8-wraps', 'OWN-001-use-after-move']);
    assert.deepEqual(rows[3], { type: 'summary', tests: 3, ok: 3, failed: 0 });
    assert.equal(run(), first);
});

test('wasm_probe_failure_message', () => {
    const dir = tree({ 'NUM/NUM-001-wasm.tz': 'test "one" = Test.equal (ref 1) (ref 1)\n\n// rule: NUM-001\n// expect: tests-pass 1\n' });
    const env = { TSUZURI_WASM_LD: '/nonexistent/wasm-ld' };
    const all = conformance(['--root', dir, '--targets', 'all'], env);
    assert.equal(all.status, 2, all.stdout + all.stderr);
    assert.match(all.stderr, /--targets native/);
    assert.equal(conformance(['--root', dir, '--targets', 'native'], env).status, 0);
});

test('output_has_no_absolute_paths', () => {
    const dir = tree({ 'OWN/OWN-001-wrong.tz': `${OWN_BODY}// expect: check-ok\n`, 'OWN/OWN-001-stdout.tz': 'let x = "a"\nx\n\n// rule: OWN-001\n// expect: stdout "b\\n"\n' });
    const text = conformance(['--root', dir, '--targets', 'native']).stdout;
    const json = conformance(['--root', dir, '--targets', 'native', '--json']).stdout;
    for (const output of [text, json]) {
        assert.ok(!output.includes('tsuzuri-conformance-'), output);
        assert.ok(!output.includes(tmpdir()), output);
    }
    assert.equal(conformance(['--root', dir, '--targets', 'native']).stdout, text);
});

// The rule tables and check-spec.

test('spec_reads_tables_and_unescapes_pipes', () => {
    const { rules } = readRules(good);
    const num = rules.find(rule => rule.id === 'NUM-001');
    assert.equal(num.rule, 'An `i8` addition MUST wrap, and `a | b` MUST be a bitwise or.');
    assert.equal(num.line, 5);
    const dir = mutate(null);
    rmSync(join(dir, 'spec/rules'), { recursive: true });
    const result = checkSpec(['--root', dir]);
    assert.equal(result.status, 2);
    assert.equal(result.stderr, 'check-spec: spec/rules/ not found\n');
});

test('spec_id_checks', () => {
    assert.deepEqual(names(specErrors(mutate('spec/rules/OWN.md', 'OWN-001 |', 'OWN-1 |'))).filter(name => name.startsWith('rule_')), ['rule_id']);
    assert.deepEqual(names(specErrors(mutate('spec/rules/OWN.md', 'OWN-001 |', 'NUM-001 |'))).filter(name => name.startsWith('rule_')).sort(), ['rule_duplicate', 'rule_id']);
    assert.deepEqual(names(specErrors(mutate('spec/rules/OWN.md', 'OWN-002 |', 'OWN-001 |'))).filter(name => name.startsWith('rule_')), ['rule_duplicate']);
    const dir = mutate(null);
    const path = join(dir, 'spec/rules/OWN.md');
    const lines = readFileSync(path, 'utf8').split('\n');
    [lines[4], lines[5]] = [lines[5], lines[4]];
    writeFileSync(path, lines.join('\n'));
    assert.deepEqual(names(specErrors(dir)).filter(name => name.startsWith('rule_')), ['rule_order']);
});

test('spec_sentence_checks', () => {
    const sentence = 'A moved `string` MUST NOT be used again (E1012).';
    const check = replacement => names(specErrors(mutate('spec/rules/OWN.md', sentence, replacement)));
    assert.deepEqual(check(sentence.slice(0, -1)), ['rule_sentence']);
    assert.deepEqual(check(sentence.replace('MUST NOT', 'SHOULD NOT')), ['rule_sentence']);
    assert.deepEqual(check(`A ${'x'.repeat(228)} MUST hold.`), ['rule_sentence']);
    assert.deepEqual(check(sentence.replace('MUST NOT', 'MAY')), []);
});

test('spec_doc_link_checks', () => {
    const link = '[page.md § Moves](../../docs/page.md#moves)';
    const check = replacement => names(specErrors(mutate('spec/rules/OWN.md', link, replacement)));
    for (const bad of ['-', `${link} ${link}`, '[x](../../docs/missing.md)', '[x](../../docs/page.md#nope)', '[x](../../src/lib.rs)']) {
        assert.deepEqual(check(bad), ['doc_link'], bad);
    }
    assert.deepEqual(check('[page.md](../../docs/page.md)'), []);
    assert.deepEqual(names(specErrors(good)), []);
});

test('spec_tests_cell_and_write', () => {
    const cell = 'NUM-001-i8-wraps, OWN-001-use-after-move';
    const dir = mutate('spec/rules/OWN.md', cell, '-');
    const errors = specErrors(dir);
    assert.deepEqual(names(errors), ['tests_cell']);
    assert.match(errors[0].message, new RegExp(cell));
    assert.equal(checkSpec(['--root', dir, '--write']).status, 0);
    const original = readFileSync(join(good, 'spec/rules/OWN.md'), 'utf8');
    assert.equal(readFileSync(join(dir, 'spec/rules/OWN.md'), 'utf8'), original);
    assert.equal(checkSpec(['--root', dir, '--write']).status, 0);
    assert.equal(readFileSync(join(dir, 'spec/rules/OWN.md'), 'utf8'), original);
});

test('spec_untested_and_unknown_rule', () => {
    let dir = mutate('tests/conformance/OWN/OWN-001-use-after-move.tz', null, null);
    assert.deepEqual(names(specErrors(dir)), ['tests_cell']);
    dir = mutate('tests/conformance/NUM/NUM-001-dir/Main.tz', '// rule: NUM-001', '// rule: NUM-777');
    assert.deepEqual(names(specErrors(dir)).sort(), ['test_rule', 'test_rule', 'tests_cell']);
    dir = mutate('tests/conformance/NUM/NUM-001-dir', null, null);
    rmSync(join(dir, 'tests/conformance/NUM/NUM-001-i8-wraps.tz'));
    assert.deepEqual(names(specErrors(dir)).sort(), ['rule_untested', 'tests_cell', 'tests_cell']);
});

test('spec_retired_rules', () => {
    let dir = mutate('tests/conformance/OWN/OWN-001-use-after-move.tz', '// rule: OWN-001\n', '// rule: OWN-001\n// rule: OWN-002\n');
    assert.deepEqual(names(specErrors(dir)).sort(), ['rule_retired_tested', 'test_rule', 'tests_cell']);
    dir = mutate('spec/rules/OWN.md', '| NUM-001-i8-wraps, OWN-001-use-after-move | active |', '| NUM-001-i8-wraps, OWN-001-use-after-move | retired |');
    const found = names(specErrors(dir)).filter(name => name !== 'marker_rule');
    assert.deepEqual(found.sort(), ['rule_retired_tested', 'test_rule', 'test_rule']);
});

test('spec_test_name_and_primary_rule', () => {
    const dir = mutate('tests/conformance/NUM/NUM-001-i8-wraps.tz', '// rule: NUM-001\n// rule: OWN-001\n', '// rule: OWN-001\n// rule: NUM-001\n');
    assert.deepEqual(names(specErrors(dir)), ['test_rule']);
    const large = mutate(null);
    writeFileSync(join(large, 'tests/conformance/OWN/OWN-001-large.tz'), `${'// filler\n'.repeat(6700)}${OWN_BODY}// expect: check-ok\n`);
    assert.ok(names(specErrors(large)).includes('test_name'));
});

test('spec_markers', () => {
    let dir = mutate('docs/page.md', '<!-- rule: OWN-001 -->', '<!-- rule: OWN-777 -->');
    assert.deepEqual(names(specErrors(dir)), ['marker_rule']);
    dir = mutate('docs/page.md', '<!-- rule: NUM-001 -->', '');
    assert.deepEqual(names(specErrors(dir)), []);
    assert.deepEqual(names(specErrors(dir, ['--require-markers'])), ['marker_missing']);
    dir = mutate('spec/rules/OWN.md', '| NUM-001-i8-wraps, OWN-001-use-after-move | active |', '| NUM-001-i8-wraps, OWN-001-use-after-move | retired |');
    assert.deepEqual(names(specErrors(dir)).sort(), ['marker_rule', 'rule_retired_tested', 'test_rule', 'test_rule']);
});

test('spec_coverage_formats', () => {
    const text = checkSpec(['--root', good, '--coverage']).stdout;
    const lines = text.trim().split('\n');
    assert.equal(lines[0], 'check-spec: ok (2 active, 1 retired, 3 tests)');
    assert.equal(lines[1], 'Area   Active  Retired  Tested  Tests  Marked');
    assert.equal(lines[5], 'OWN         1        1       1      1       1');
    assert.equal(lines[17], 'Total       2        1       2      3       2');
    const markdown = checkSpec(['--root', good, '--coverage-markdown']).stdout;
    const page = readFileSync(join(good, '_tsuzuri/language-reference/compiler/conformance.md'), 'utf8');
    assert.equal(markdown, page.slice(page.indexOf('| Area |'), page.indexOf('<!-- /generated')));
    const json = JSON.parse(checkSpec(['--root', good, '--coverage', '--json']).stdout);
    assert.equal(json.schema, 'tz-spec-coverage/1');
    assert.equal(json.areas.length, 15);
    assert.deepEqual(json.total, { active: 2, retired: 1, tested: 2, tests: 3, marked: 2 });
    assert.equal(checkSpec(['--root', good, '--coverage']).stdout, text);
});

test('spec_link_command', () => {
    assert.equal(checkSpec(['--root', good, '--link', 'docs/page.md', 'Moves']).stdout, '[page.md § Moves](../../docs/page.md#moves)\n');
    assert.equal(checkSpec(['--root', good, '--link', 'docs/page.md']).stdout, '[page.md](../../docs/page.md)\n');
    assert.equal(checkSpec(['--root', good, '--link', 'docs/page.md', 'Nope']).status, 1);
    assert.equal(checkSpec(['--root', good, '--link', 'docs/none.md']).status, 1);
});

test('spec_generated_block', () => {
    const dir = mutate('_tsuzuri/language-reference/compiler/conformance.md', '| NUM | 1 | 0 | 1 | 2 | 1 |', '| NUM | 9 | 0 | 1 | 2 | 1 |');
    assert.deepEqual(names(specErrors(dir, [], true)), ['coverage_block']);
    assert.equal(checkSpec(['--root', dir, '--write']).status, 0);
    assert.equal(
        readFileSync(join(dir, '_tsuzuri/language-reference/compiler/conformance.md'), 'utf8'),
        readFileSync(join(good, '_tsuzuri/language-reference/compiler/conformance.md'), 'utf8'),
    );
    const without = mutate('_tsuzuri/language-reference/compiler/conformance.md', null, null);
    assert.deepEqual(names(specErrors(without, [], true)), []);
});

test('slug_matches_check_docs', () => {
    // The expression of `headings` in scripts/check-docs.mjs, copied here to compare.
    const reference = source => {
        const found = new Set();
        const duplicates = new Map();
        for (const line of source.replace(/^```[^\n]*\n[\s\S]*?^```\s*$/gm, '').split('\n')) {
            const heading = /^#{1,6} (.+)$/.exec(line);
            if (!heading) continue;
            const base = heading[1].toLowerCase().replace(/[^\p{L}\p{M}\p{N}\s_-]/gu, '').replace(/ /g, '-');
            const count = duplicates.get(base) ?? 0;
            found.add(count ? `${base}-${count}` : base);
            duplicates.set(base, count + 1);
        }
        return found;
    };
    const checkDocs = readFileSync(join(root, 'scripts/check-docs.mjs'), 'utf8');
    assert.ok(checkDocs.includes(".replace(/[^\\p{L}\\p{M}\\p{N}\\s_-]/gu, '').replace(/ /g, '-')"));
    for (const page of ['_tsuzuri/language-reference/ownership-and-memory/ownership.md', 'docs/language.md', 'README.md']) {
        const source = readFileSync(join(root, page), 'utf8');
        assert.deepEqual([...headingsOf(source)].sort(), [...reference(source)].sort(), page);
    }
});

test('spec_strip_markers', () => {
    const result = checkSpec(['--root', good, '--strip-markers', 'docs/page.md']);
    assert.equal(result.status, 0);
    assert.ok(!result.stdout.includes('<!-- rule: OWN-001 -->'));
    assert.ok(result.stdout.includes('- Integer addition wraps.\n'));
    assert.ok(result.stdout.includes('<!-- rule: NUM-999 -->'));
    assert.ok(result.stdout.includes('## Moves\n\nA moved value'));
});

test('real_tree_is_clean', () => {
    const result = checkSpec([]);
    assert.equal(result.status, 0, result.stderr);
});

let passed = 0;
let failed = 0;
try {
    for (const { name, body } of tests) {
        if (only && !name.includes(only)) continue;
        try {
            body();
            passed += 1;
            console.log(`ok ${name}`);
        } catch (error) {
            failed += 1;
            console.log(`FAIL ${name}\n${error.stack ?? error}`);
        }
    }
} finally {
    rmSync(scratch, { recursive: true, force: true });
}
console.log(failed ? `${failed} of ${passed + failed} tests failed` : `${passed} tests passed`);
process.exitCode = failed ? 1 : 0;
assert.ok(existsSync(good));
