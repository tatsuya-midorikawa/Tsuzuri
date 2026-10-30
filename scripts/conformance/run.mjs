// Running one conformance test against a compiler (G33 §5, §6).

import { spawn, spawnSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

import { TRAP_KINDS } from './directives.mjs';
import { checkCli, matchDiagnostic, namesFile, parseDiagnostics, summaryOf, trapLine } from './expect.mjs';

export const MAX_OUTPUT_BYTES = 4 * 1024 * 1024;

/** Seconds that the runner waits for the pipes after the compiler exits or is stopped. */
export const PIPE_GRACE_SECONDS = 2;

const live = new Set();

/**
 * Stops `child` and everything that it started: `tsuzuri run` runs the program as its own child, which inherits
 * the pipes. On POSIX the compiler leads its own process group (`detached`), so the group is killed; on Windows
 * `taskkill /T` stops the tree.
 */
export function killTree(child) {
    if (child.exitCode !== null || child.signalCode !== null) {
        if (process.platform !== 'win32') {
            try {
                process.kill(-child.pid, 'SIGKILL');
            } catch {
                // The group is gone.
            }
        }
        return;
    }
    if (process.platform === 'win32') {
        spawnSync('taskkill', ['/pid', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
        child.kill('SIGKILL');
        return;
    }
    try {
        process.kill(-child.pid, 'SIGKILL');
    } catch {
        child.kill('SIGKILL');
    }
}

process.on('exit', () => {
    for (const child of live) killTree(child);
});
for (const signal of ['SIGINT', 'SIGTERM']) {
    process.once(signal, () => {
        for (const child of live) killTree(child);
        process.kill(process.pid, signal);
    });
}

/** Runs the compiler once: its status, standard output and error, and whether it timed out. */
export function runCompiler(compiler, args, { cwd, timeout = 120, env = process.env } = {}) {
    return new Promise(resolve => {
        const child = spawn(compiler, args, {
            cwd,
            env,
            stdio: ['ignore', 'pipe', 'pipe'],
            detached: process.platform !== 'win32',
        });
        live.add(child);
        const chunks = { stdout: [], stderr: [] };
        let size = 0;
        let tooLarge = false;
        let timedOut = false;
        let done = false;
        let grace = null;
        const stop = () => {
            killTree(child);
            // A descendant that survives keeps the pipes open, and `close` would never come: stop waiting for it.
            grace ??= setTimeout(() => finish(null), PIPE_GRACE_SECONDS * 1000);
        };
        const collect = name => chunk => {
            if (tooLarge) return;
            size += chunk.length;
            if (size > MAX_OUTPUT_BYTES) {
                tooLarge = true;
                stop();
                return;
            }
            chunks[name].push(chunk);
        };
        child.stdout.on('data', collect('stdout'));
        child.stderr.on('data', collect('stderr'));
        const timer = setTimeout(() => {
            timedOut = true;
            stop();
        }, timeout * 1000);
        const finish = status => {
            if (done) return;
            done = true;
            clearTimeout(timer);
            clearTimeout(grace);
            live.delete(child);
            // What the program left running is stopped too, and its pipes are closed.
            killTree(child);
            child.stdout.destroy();
            child.stderr.destroy();
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
        child.on('exit', status => {
            grace ??= setTimeout(() => finish(timedOut || tooLarge ? null : status), PIPE_GRACE_SECONDS * 1000);
        });
        child.on('close', status => finish(timedOut || tooLarge ? null : status));
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
        .map(d => `${d.code}@${String(d.path ?? '').replaceAll('\\', '/').split('/').pop()}:${d.span?.line ?? '?'}:${d.span?.column ?? '?'}`)
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
            const wanted = {
                severity,
                code: expect.code,
                line: expect.line,
                column: expect.column,
                file: expect.file,
                diagKind: expect.diagKind,
                message: expect.message,
            };
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
