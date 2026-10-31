// The language conformance suite (G33 §6): runs the tests of `tests/conformance/<AREA>/` against a
// compiler and checks each against the expectations at the end of its Main.tz.
//
// usage: node scripts/conformance.mjs [compiler] [--area A[,B]] [--rule ID[,ID]] [--filter WORD]
//            [--jobs N] [--targets all|native] [--timeout SECONDS] [--list] [--json] [--keep]
//            [--timing] [--root DIR]
// The compiler is the first argument, else TSUZURI_BIN, else target/release/tsuzuri. Exit status:
// 0 when every chosen test passes, 1 when one fails, 2 for a usage error, a malformed test, a
// missing compiler, an unusable wasm32 toolchain, or an empty selection.

import { existsSync, mkdtempSync, rmSync } from 'node:fs';
import { availableParallelism, tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { AREAS } from './conformance/directives.mjs';
import { discoverTests } from './conformance/discover.mjs';
import { probeWasm, runsOf, runTest } from './conformance/run.mjs';

const repository = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const VALUED = ['--area', '--rule', '--filter', '--jobs', '--targets', '--timeout', '--root'];
const FLAGS = ['--list', '--json', '--keep', '--timing'];

function usage(message) {
    process.stderr.write(`conformance: ${message}\n`);
    process.exitCode = 2;
}

function parseArguments(argv) {
    const options = { positional: [] };
    for (let index = 0; index < argv.length; index++) {
        const argument = argv[index];
        if (VALUED.includes(argument)) {
            if (index + 1 >= argv.length) throw new Error(`${argument} needs a value`);
            if (options[argument] !== undefined) throw new Error(`${argument} is given more than once`);
            options[argument] = argv[++index];
        } else if (FLAGS.includes(argument)) {
            options[argument] = true;
        } else if (argument.startsWith('--')) {
            throw new Error(`unknown option '${argument}'`);
        } else {
            options.positional.push(argument);
        }
    }
    if (options.positional.length > 1) throw new Error('only one compiler may be given');
    return options;
}

function list(value) {
    return value === undefined ? null : value.split(',').filter(Boolean);
}

export async function main(argv) {
    let options;
    try {
        options = parseArguments(argv);
    } catch (failure) {
        return usage(failure.message);
    }
    const root = resolve(options['--root'] ?? repository);
    const compiler = resolve(options.positional[0] ?? process.env.TSUZURI_BIN ?? join(repository, 'target/release/tsuzuri'));
    const targets = options['--targets'] ?? 'all';
    if (!['all', 'native'].includes(targets)) return usage('--targets must be all or native');
    const jobs = Number(options['--jobs'] ?? Math.min(4, availableParallelism()));
    if (!Number.isInteger(jobs) || jobs < 1 || jobs > 64) return usage('--jobs must be an integer from 1 to 64');
    const timeout = Number(options['--timeout'] ?? 120);
    if (!(timeout > 0)) return usage('--timeout must be a positive number of seconds');
    const areas = list(options['--area']);
    if (areas?.some(area => !AREAS.includes(area))) return usage(`--area takes the areas ${AREAS.join(', ')}`);
    const rules = list(options['--rule']);
    const filter = options['--filter'];

    const { tests, errors } = discoverTests(root);
    if (errors.length) {
        for (const error of errors) process.stderr.write(`${error.path}:${error.line}: ${error.name}: ${error.message}\n`);
        process.stderr.write(`conformance: ${errors.length} malformed tests\n`);
        process.exitCode = 2;
        return;
    }
    const nativeOnly = targets === 'native';
    let chosen = tests.filter(
        test =>
            (!areas || areas.includes(test.area)) &&
            (!rules || test.parsed.rules.some(rule => rules.includes(rule))) &&
            (filter === undefined || test.stem.includes(filter)),
    );
    if (nativeOnly) {
        const runnable = test =>
            test.parsed.mode === 'cli' ||
            test.parsed.expects.every(expect => !['stdout', 'trap', 'tests-pass'].includes(expect.kind) || runsOf(test, expect.kind, true).length);
        const skipped = chosen.filter(test => !runnable(test)).length;
        chosen = chosen.filter(runnable);
        process.stderr.write(`note: wasm targets skipped${skipped ? ` (${skipped} wasm-only tests not run)` : ''}\n`);
    }
    if (!chosen.length) return usage('no test is selected');
    if (options['--list']) {
        for (const test of chosen) {
            process.stdout.write(`${test.stem}\t${test.parsed.rules.join(',')}\t${test.parsed.expects.map(expect => expect.kind).join(',')}\n`);
        }
        return;
    }
    if (!existsSync(compiler)) return usage(`no compiler at ${compiler}; build it or pass its path`);
    const work = mkdtempSync(join(tmpdir(), 'tsuzuri-conformance-'));
    const ctx = { compiler, work, timeout, nativeOnly, env: process.env };
    try {
        const usesWasm = !nativeOnly && chosen.some(test => test.parsed.expects.some(expect => expect.kind === 'tests-pass' && runsOf(test, 'tests-pass', false).some(run => run.startsWith('wasm-'))));
        if (usesWasm) {
            const failure = await probeWasm(ctx);
            if (failure !== null) {
                return usage(`wasm32 tests are unavailable (${failure}); install wasm-ld (TSUZURI_WASM_LD) or pass --targets native`);
            }
        }
        const results = new Array(chosen.length);
        let next = 0;
        const worker = async () => {
            while (next < chosen.length) {
                const index = next++;
                const started = Date.now();
                results[index] = await runTest(chosen[index], ctx);
                results[index].milliseconds = Date.now() - started;
            }
        };
        await Promise.all(Array.from({ length: Math.min(jobs, chosen.length) }, worker));
        let failed = 0;
        const out = [];
        for (const [index, test] of chosen.entries()) {
            const result = results[index];
            if (result.status !== 'ok') failed += 1;
            if (options['--json']) {
                const row = { type: 'test', stem: test.stem, rules: test.parsed.rules, status: result.status === 'ok' ? 'ok' : 'fail', details: result.details };
                if (options['--timing']) row.ms = result.milliseconds;
                out.push(JSON.stringify(row));
            } else {
                const timing = options['--timing'] ? ` ${result.milliseconds} ms` : '';
                out.push(`${result.status === 'ok' ? 'ok' : 'FAIL'} ${test.stem} (${test.parsed.rules.join(',')})${timing}`);
                for (const detail of result.details) out.push(`  ${detail}`);
            }
        }
        if (options['--json']) {
            out.push(JSON.stringify({ type: 'summary', tests: chosen.length, ok: chosen.length - failed, failed }));
        } else {
            out.push(`conformance: ${chosen.length} tests, ${chosen.length - failed} ok, ${failed} failed`);
        }
        process.stdout.write(`${out.join('\n')}\n`);
        if (options['--keep']) process.stderr.write(`conformance: kept the work directory ${work}\n`);
        process.exitCode = failed ? 1 : 0;
    } finally {
        if (!options['--keep']) rmSync(work, { recursive: true, force: true });
    }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
    await main(process.argv.slice(2));
}
