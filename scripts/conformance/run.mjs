// Running one conformance test against a compiler (G33 §5, §6).

import { spawn } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

import { TRAP_KINDS } from './directives.mjs';
import { checkCli, matchDiagnostic, namesFile, parseDiagnostics, summaryOf, trapLine } from './expect.mjs';

export const MAX_OUTPUT_BYTES = 4 * 1024 * 1024;

/** Runs the compiler once: its status, standard output and error, and whether it timed out. */
export function runCompiler(compiler, args, { cwd, timeout = 120, env = process.env } = {}) {
    return new Promise(resolve => {
        const child = spawn(compiler, args, { cwd, env, stdio: ['ignore', 'pipe', 'pipe'] });
        const chunks = { stdout: [], stderr: [] };
        let size = 0;
        let tooLarge = false;
        let timedOut = false;
        const collect = name => chunk => {
            size += chunk.length;
            if (size > MAX_OUTPUT_BYTES) {
                tooLarge = true;
                child.kill('SIGKILL');
                return;
            }
            chunks[name].push(chunk);
        };
        child.stdout.on('data', collect('stdout'));
        child.stderr.on('data', collect('stderr'));
        const timer = setTimeout(() => {
            timedOut = true;
            child.kill('SIGKILL');
        }, timeout * 1000);
        const finish = status => {
            clearTimeout(timer);
            resolve({
                status,
                stdout: Buffer.concat(chunks.stdout).toString('utf8'),
                stderr: Buffer.concat(chunks.stderr).toString('utf8'),
                timedOut,
                tooLarge,
            });
        };
        child.on('error', failure => {
            chunks.stderr.push(Buffer.from(String(failure.message)));
            finish(null);
        });
        child.on('close', status => finish(status));
    });
}

/** Writes the test into the empty directory `dir`: its files, and a manifest for `// edition:`. */
export function materialize(test, dir) {
    mkdirSync(dir, { recursive: true });
    if (test.form === 'dir') cpSync(test.path, dir, { recursive: true });
    else writeFileSync(join(dir, 'Main.tz'), test.text);
    if (test.parsed.edition) {
        writeFileSync(
            join(dir, 'Tsuzuri.toml'),
            `[package]\nname = "conformance"\nversion = "0.1.0"\nedition = "${test.parsed.edition}"\n`,
        );
    }
}

const NATIVE_RUNS = ['native-O0', 'native-O3'];
const ALL_RUNS = ['native-O0', 'native-O3', 'wasm-O0', 'wasm-O3'];

/** The runs that an expectation of kind `kind` uses, after `--targets native`. */
export function runsOf(test, kind, nativeOnly) {
    const chosen = test.parsed.targets ?? (kind === 'tests-pass' ? ALL_RUNS : NATIVE_RUNS);
    return nativeOnly ? chosen.filter(run => run.startsWith('native-')) : chosen;
}

function describeDiagnostics(diagnostics) {
    if (!diagnostics.length) return 'no diagnostics';
    return diagnostics
        .slice(0, 5)
        .map(d => `${d.code}@${String(d.path ?? '').replaceAll('\\', '/').split('/').pop()}:${d.span?.line ?? '?'}`)
        .join(', ');
}

function failureOf(result, timeout) {
    if (result.timedOut) return `timed out after ${timeout} s`;
    if (result.tooLarge) return 'the output exceeds 4 MiB';
    return null;
}

/** Runs `test` in `ctx.work/<stem>`: `{ status: 'ok' | 'fail', details }`. */
export async function runTest(test, ctx) {
    const dir = join(ctx.work, test.stem);
    materialize(test, dir);
    const details = [];
    const scrub = text => text.split(dir).join('$DIR');
    const run = args => runCompiler(ctx.compiler, args, { cwd: dir, timeout: ctx.timeout, env: ctx.env });
    const parsed = test.parsed;
    if (parsed.mode === 'cli') {
        const words = parsed.cli.map(word => word.replaceAll('$DIR', dir));
        const expects = parsed.expects.filter(expect => expect.kind !== 'deterministic');
        if (parsed.expects.some(expect => expect.kind === 'deterministic')) {
            const outputs = [];
            for (const name of ['a.out', 'b.out']) {
                const result = await run(words.map(word => word.replaceAll('$OUT', join(dir, name))));
                const failure = failureOf(result, ctx.timeout);
                if (failure) details.push(`deterministic: ${failure}`);
                else if (result.status !== 0) details.push(`deterministic: exit ${result.status}: ${result.stderr.split('\n')[0]}`);
                outputs.push(existsSync(join(dir, name)) ? readFileSync(join(dir, name)) : null);
            }
            if (!details.length && (!outputs[0] || !outputs[1] || !outputs[0].equals(outputs[1]))) {
                details.push('deterministic: the two outputs differ');
            }
        }
        if (expects.length) {
            const result = await run(words.map(word => word.replaceAll('$OUT', join(dir, 'out'))));
            const failure = failureOf(result, ctx.timeout);
            if (failure) details.push(`cli: ${failure}`);
            else {
                const read = path => (existsSync(join(dir, path)) ? readFileSync(join(dir, path), 'utf8') : null);
                for (const expect of expects) {
                    const reason = checkCli(expect, result, read);
                    if (reason) details.push(reason);
                }
            }
        }
        return { status: details.length ? 'fail' : 'ok', details: details.map(scrub) };
    }
    let checked = null;
    const check = async () => {
        if (!checked) checked = await run(['check', dir, '--json']);
        return checked;
    };
    for (const expect of parsed.expects) {
        if (expect.kind === 'check-ok' || expect.kind === 'check-error' || expect.kind === 'check-warning') {
            const result = await check();
            const failure = failureOf(result, ctx.timeout);
            if (failure) {
                details.push(`${expect.raw}: ${failure}`);
                continue;
            }
            const diagnostics = parseDiagnostics(result.stderr);
            const errors = diagnostics.filter(d => d.severity === 'error');
            if (expect.kind === 'check-ok') {
                if (result.status !== 0 || errors.length) {
                    details.push(`check-ok: exit ${result.status}; got ${describeDiagnostics(errors)}`);
                }
                continue;
            }
            const severity = expect.kind === 'check-error' ? 'error' : 'warning';
            const status = severity === 'error' ? 1 : 0;
            const wanted = { severity, code: expect.code, line: expect.line, file: expect.file, diagKind: expect.diagKind };
            if (result.status !== status) {
                details.push(`${expect.raw}: exit ${result.status}, expected ${status}; got ${describeDiagnostics(diagnostics)}`);
            } else if (!diagnostics.some(d => matchDiagnostic(d, wanted))) {
                details.push(`${expect.raw}: no ${severity} diagnostic matches; got ${describeDiagnostics(diagnostics)}`);
            }
            continue;
        }
        if (expect.kind === 'stdout' || expect.kind === 'trap') {
            for (const target of runsOf(test, expect.kind, ctx.nativeOnly)) {
                const optimization = target.endsWith('O0') ? '-O0' : '-O3';
                const result = await run(['run', dir, optimization]);
                const failure = failureOf(result, ctx.timeout);
                const label = `${expect.raw} (${target})`;
                if (failure) {
                    details.push(`${label}: ${failure}`);
                    continue;
                }
                if (expect.kind === 'stdout') {
                    if (result.status !== 0) details.push(`${label}: exit ${result.status}: ${result.stderr.split('\n')[0]}`);
                    else if (result.stdout !== expect.stdout) details.push(`${label}: got ${JSON.stringify(result.stdout)}`);
                    continue;
                }
                const trap = trapLine(result.stderr);
                const description = TRAP_KINDS[expect.trapKind];
                if (result.status !== 1) details.push(`${label}: exit ${result.status}, expected 1`);
                else if (!trap || trap.description !== description) {
                    details.push(`${label}: no 'trap: ${description}' line; got ${JSON.stringify(result.stderr.split('\n')[0])}`);
                } else if (expect.line !== undefined && (trap.line !== expect.line || !namesFile(trap.path, expect.file ?? 'Main.tz'))) {
                    details.push(`${label}: the trap is at ${trap.path}:${trap.line}`);
                } else if (expect.file !== undefined && !namesFile(trap.path, expect.file)) {
                    details.push(`${label}: the trap is in ${trap.path}`);
                } else if (expect.stdout !== undefined && result.stdout !== expect.stdout) {
                    details.push(`${label}: got stdout ${JSON.stringify(result.stdout)}`);
                }
            }
            continue;
        }
        if (expect.kind === 'tests-pass') {
            for (const target of runsOf(test, expect.kind, ctx.nativeOnly)) {
                const optimization = target.endsWith('O0') ? '-O0' : '-O3';
                const platform = target.startsWith('wasm') ? 'wasm32' : 'native';
                // The suite checks the values that the language defines; whether `tsuzuri test` fails a
                // test whose arithmetic wraps (A23) is a policy of the runner, not a rule.
                const result = await run(['test', dir, '--json', '--target', platform, optimization, '--allow-overflow']);
                const failure = failureOf(result, ctx.timeout);
                const label = `${expect.raw} (${target})`;
                const summary = summaryOf(result.stdout);
                if (failure) details.push(`${label}: ${failure}`);
                else if (result.status !== 0 || !summary) {
                    details.push(`${label}: exit ${result.status}: ${result.stderr.split('\n')[0]}`);
                } else if (summary.failed !== 0 || summary.passed !== expect.count) {
                    details.push(`${label}: passed=${summary.passed} != ${expect.count} or failed=${summary.failed}`);
                }
            }
        }
    }
    return { status: details.length ? 'fail' : 'ok', details: details.map(scrub) };
}

/** Whether `test --target wasm32` works here: `null`, or the first line of its error. */
export async function probeWasm(ctx) {
    const dir = join(ctx.work, '.probe-wasm');
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, 'Main.tz'), 'test "probe" = Test.equal (ref 1) (ref 1)\n');
    const result = await runCompiler(ctx.compiler, ['test', dir, '--json', '--target', 'wasm32', '-O0'], {
        cwd: dir,
        timeout: ctx.timeout,
        env: ctx.env,
    });
    if (result.status === 0) return null;
    const line = result.stderr.split('\n').find(text => text.trim()) ?? `exit ${result.status}`;
    return line.split(dir).join('$DIR');
}
