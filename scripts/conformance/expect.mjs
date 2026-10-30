// Matching a compiler's output against an expectation (G33 §5). Pure functions: nothing here runs
// the compiler.

/** The diagnostics of `check --json`: one JSON object per stderr line; other lines are skipped. */
export function parseDiagnostics(stderr) {
    const found = [];
    for (const line of stderr.split('\n')) {
        if (!line.startsWith('{')) continue;
        try {
            const value = JSON.parse(line);
            if (value && typeof value.code === 'string' && typeof value.severity === 'string') found.push(value);
        } catch {
            // Not a diagnostic.
        }
    }
    return found;
}

function normalized(path) {
    return String(path ?? '').replaceAll('\\', '/');
}

/** Whether `path` names `file`: the same path, or one that ends with `/file`. */
export function namesFile(path, file) {
    const text = normalized(path);
    return text === file || text.endsWith(`/${file}`);
}

/**
 * Whether diagnostic `d` satisfies `e` (`{ severity, code, line, file, diagKind }`): the same
 * severity and code, and the line (in `file`, `Main.tz` by default), file and kind when given.
 */
export function matchDiagnostic(d, e) {
    if (d.severity !== e.severity || d.code !== e.code) return false;
    if (e.line !== undefined) {
        if (d.span?.line !== e.line || !namesFile(d.path, e.file ?? 'Main.tz')) return false;
    } else if (e.file !== undefined && !namesFile(d.path, e.file)) {
        return false;
    }
    if (e.diagKind !== undefined && d.kind !== e.diagKind) return false;
    return true;
}

const TRAP = /^trap: (.*) at (.+):([0-9]+):([0-9]+)$/;

/** The first `trap: <description> at <path>:<line>:<column>` line of `stderr`, or `null`. */
export function trapLine(stderr) {
    for (const line of stderr.split('\n')) {
        const match = TRAP.exec(line.replace(/\r$/, ''));
        if (match) return { description: match[1], path: match[2], line: Number(match[3]), column: Number(match[4]) };
    }
    return null;
}

/** The `summary` line of `test --json`, or `null`. */
export function summaryOf(stdout) {
    for (const line of stdout.split('\n')) {
        if (!line.startsWith('{')) continue;
        try {
            const value = JSON.parse(line);
            if (value?.type === 'summary') return { passed: value.passed, failed: value.failed, ignored: value.ignored };
        } catch {
            // A broken line is no summary.
        }
    }
    return null;
}

function shown(text) {
    return JSON.stringify(text.length > 200 ? `${text.slice(0, 200)}…` : text);
}

/**
 * The reason why the cli result `r` (`{ status, stdout, stderr }`) fails expectation `e`, or
 * `null` when it holds. `read(path)` returns the text of a file of the test's directory or `null`.
 */
export function checkCli(e, r, read) {
    switch (e.kind) {
        case 'exit':
            return r.status === e.status ? null : `exit ${e.status}: got ${r.status}`;
        case 'stdout':
            return r.stdout === e.text ? null : `stdout: got ${shown(r.stdout)}`;
        case 'stdout-has':
            return r.stdout.includes(e.text) ? null : `stdout-has ${shown(e.text)}: not in stdout ${shown(r.stdout)}`;
        case 'stderr-has':
            return r.stderr.includes(e.text) ? null : `stderr-has ${shown(e.text)}: not in stderr ${shown(r.stderr)}`;
        case 'stderr-lacks':
            return r.stderr.includes(e.text) ? `stderr-lacks ${shown(e.text)}: found in stderr` : null;
        case 'stdout-empty':
            return r.stdout === '' ? null : `stdout-empty: got ${shown(r.stdout)}`;
        case 'stderr-empty':
            return r.stderr === '' ? null : `stderr-empty: got ${shown(r.stderr)}`;
        case 'stderr-lines': {
            const count = r.stderr.split('\n').filter(line => line.trim() !== '').length;
            return count === e.count ? null : `stderr-lines ${e.count}: got ${count}`;
        }
        case 'file-has': {
            const text = read(e.path);
            if (text === null) return `file-has ${e.path}: no such file`;
            return text.includes(e.text) ? null : `file-has ${e.path} ${shown(e.text)}: not in the file`;
        }
        case 'file-lacks': {
            const text = read(e.path);
            return text !== null && text.includes(e.text) ? `file-lacks ${e.path} ${shown(e.text)}: found in the file` : null;
        }
        default:
            return `unknown expectation ${e.kind}`;
    }
}
