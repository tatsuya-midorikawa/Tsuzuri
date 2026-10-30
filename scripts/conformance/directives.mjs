// The directives of a conformance test (G33 §4, §5): the `// key: value` lines at the end of its
// Main.tz, read without running anything. Errors are collected, never thrown.

/** The rule areas in the order of D-58. */
export const AREAS = ['LEX', 'SYN', 'TYP', 'OWN', 'EVL', 'NUM', 'STR', 'CTL', 'MOD', 'TSK', 'DIA', 'CLI', 'ABI', 'STD', 'EDN'];

/** `trap kind=K`: K and the description that `src/trap.rs` (`TrapKind::description`) prints. */
export const TRAP_KINDS = {
    assert: 'assertion failed',
    div_zero: 'integer division by zero',
    div_overflow: 'integer division overflow',
    bounds: 'index out of bounds',
    alloc_size: 'allocation size overflow',
    alloc_failure: 'allocation failed',
    range_step: 'range step is zero',
    match_failure: 'non-exhaustive match',
    pattern_mismatch: 'pattern mismatch',
    string_overflow: 'string length overflow',
    numeric_runtime: 'numeric runtime trap',
    wasm_runtime: 'wasm runtime trap',
    encoding: 'invalid Unicode encoding',
    debug_output: 'debug output failed',
    stack_overflow: 'stack overflow',
    overflow: 'unhandled OverflowException: arithmetic operation resulted in an overflow',
    contract_requires: 'contract requires failed',
    contract_ensures: 'contract ensures failed',
};

/** The compiler runs that `stdout`, `trap` and `tests-pass` may use. */
export const TARGETS = ['native-O0', 'native-O3', 'wasm-O0', 'wasm-O3'];

export const RULE_ID = new RegExp(`^(${AREAS.join('|')})-(?!000)[0-9]{3}$`);

const KEYS = ['rule', 'expect', 'edition', 'targets', 'cli', 'note'];
const SINGLE = ['edition', 'targets', 'cli'];
const DIRECTIVE = /^\/\/ ([a-z][a-z-]*): ?(.*)$/;
const FOOTER = /^\/\/ [a-z][a-z-]*: ?/;

/** The directives at the end of `text` and the number of body lines before them. */
export function splitFooter(text) {
    const lines = text.split('\n');
    if (lines.length && lines[lines.length - 1] === '') lines.pop();
    let start = lines.length;
    while (start > 0 && (lines[start - 1].trim() === '' || FOOTER.test(lines[start - 1]))) start -= 1;
    const directives = [];
    for (let index = start; index < lines.length; index++) {
        const match = DIRECTIVE.exec(lines[index]);
        if (match) directives.push({ key: match[1], value: match[2], line: index + 1 });
    }
    return { start, directives };
}

/** The words and JSON strings of a directive's value; throws an `Error` for a malformed string. */
export function tokens(value) {
    const out = [];
    let i = 0;
    while (i < value.length) {
        if (value[i] === ' ') {
            i += 1;
            continue;
        }
        if (value[i] === '"') {
            let j = i + 1;
            while (j < value.length && value[j] !== '"') j += value[j] === '\\' ? 2 : 1;
            if (j >= value.length) throw new Error('unterminated string');
            let text;
            try {
                text = JSON.parse(value.slice(i, j + 1));
            } catch {
                throw new Error('invalid JSON string');
            }
            if (j + 1 < value.length && value[j + 1] !== ' ') throw new Error('a string must be followed by a space');
            out.push({ string: true, text });
            i = j + 1;
        } else {
            let j = i;
            while (j < value.length && value[j] !== ' ') j += 1;
            out.push({ string: false, text: value.slice(i, j) });
            i = j;
        }
    }
    return out;
}

class BadArgument extends Error {}

function bad(message) {
    return new BadArgument(message);
}

function integer(text, what, minimum = 0) {
    if (!/^[0-9]+$/.test(text)) throw bad(`${what} must be an integer`);
    const value = Number(text);
    if (value < minimum) throw bad(`${what} must be >= ${minimum}`);
    return value;
}

/** A relative path inside the test's directory. */
function relative(text, what) {
    if (text === '' || text.startsWith('/') || /^[A-Za-z]:/.test(text) || text.includes('\\') || text.split('/').includes('..')) {
        throw bad(`${what} must be a relative path without '..' or '\\'`);
    }
    return text;
}

function word(token, what) {
    if (!token || token.string) throw bad(`${what} must be a word`);
    return token.text;
}

function string(token, what) {
    if (!token || !token.string) throw bad(`${what} must be a JSON string`);
    return token.text;
}

/** `line=N file=NAME kind=K` after a check code, or after `trap kind=K`. */
function options(list, allowed) {
    const found = {};
    for (const token of list) {
        const text = word(token, 'an option');
        const separator = text.indexOf('=');
        const key = separator > 0 ? text.slice(0, separator) : text;
        const value = separator > 0 ? text.slice(separator + 1) : '';
        if (!allowed.includes(key) || separator < 0) throw bad(`unknown option '${text}'`);
        if (key in found) throw bad(`'${key}=' is given twice`);
        if (key === 'line') found.line = integer(value, 'line', 1);
        else if (key === 'file') found.file = relative(value, 'file');
        else if (key === 'kind') {
            if (!/^[a-z][a-z0-9_]*$/.test(value)) throw bad(`kind must be snake_case, got '${value}'`);
            found.kind = value;
        }
    }
    return found;
}

function normalExpectation(kind, args) {
    switch (kind) {
        case 'check-ok':
            if (args.length) throw bad('check-ok takes no argument');
            return { kind };
        case 'check-error':
        case 'check-warning': {
            const code = word(args[0], 'the code');
            const pattern = kind === 'check-error' ? /^E[0-9]{4}$/ : /^W[0-9]{4}$/;
            if (!pattern.test(code)) throw bad(`'${code}' is not a code for ${kind}`);
            const found = options(args.slice(1), ['line', 'file', 'kind']);
            return { kind, code, line: found.line, file: found.file, diagKind: found.kind };
        }
        case 'stdout':
            if (args.length !== 1) throw bad('stdout takes one JSON string');
            return { kind, stdout: string(args[0], 'the output') };
        case 'trap': {
            let rest = args;
            let stdout;
            const at = rest.findIndex(token => !token.string && token.text === 'stdout');
            if (at >= 0) {
                if (at !== rest.length - 2) throw bad("'stdout' must be the last pair of trap");
                stdout = string(rest[at + 1], 'the output');
                rest = rest.slice(0, at);
            }
            const found = options(rest, ['kind', 'line', 'file']);
            if (!found.kind) throw bad('trap needs kind=K');
            if (!(found.kind in TRAP_KINDS)) throw bad(`unknown trap kind '${found.kind}'`);
            return { kind, trapKind: found.kind, line: found.line, file: found.file, stdout };
        }
        case 'tests-pass':
            if (args.length !== 1) throw bad('tests-pass takes one count');
            return { kind, count: integer(word(args[0], 'the count'), 'the count') };
        default:
            return null;
    }
}

function cliExpectation(kind, args) {
    switch (kind) {
        case 'exit':
            if (args.length !== 1) throw bad('exit takes one status');
            return { kind, status: integer(word(args[0], 'the status'), 'the status') };
        case 'stdout':
        case 'stdout-has':
        case 'stderr-has':
        case 'stderr-lacks':
            if (args.length !== 1) throw bad(`${kind} takes one JSON string`);
            return { kind, text: string(args[0], 'the text') };
        case 'stdout-empty':
        case 'stderr-empty':
        case 'deterministic':
            if (args.length) throw bad(`${kind} takes no argument`);
            return { kind };
        case 'stderr-lines':
            if (args.length !== 1) throw bad('stderr-lines takes one count');
            return { kind, count: integer(word(args[0], 'the count'), 'the count') };
        case 'file-has':
        case 'file-lacks':
            if (args.length !== 2) throw bad(`${kind} takes a path and a JSON string`);
            return { kind, path: relative(word(args[0], 'the path'), 'the path'), text: string(args[1], 'the text') };
        default:
            return null;
    }
}

/** The directives of a test file: rules, expectations, mode and errors (`{ line, name, message }`). */
export function parseTest(text) {
    const { start, directives } = splitFooter(text);
    const errors = [];
    const error = (line, name, message) => errors.push({ line, name, message });
    const rules = [];
    const ruleLines = [];
    const expectLines = [];
    const seen = new Set();
    let cli = null;
    let edition = null;
    let targets = null;
    let targetsLine = 0;
    for (const directive of directives) {
        const { key, value, line } = directive;
        if (!KEYS.includes(key)) {
            error(line, 'unknown_directive', `unknown directive '${key}'`);
            continue;
        }
        if (SINGLE.includes(key)) {
            if (seen.has(key)) {
                error(line, 'duplicate_directive', `'${key}' is given more than once`);
                continue;
            }
            seen.add(key);
        }
        if (key === 'rule') {
            if (RULE_ID.test(value)) {
                rules.push(value);
                ruleLines.push(line);
            }
            else error(line, 'rule_format', `'${value}' is not a rule ID`);
        } else if (key === 'expect') {
            expectLines.push(directive);
        } else if (key === 'edition') {
            if (/^20[0-9]{2}$/.test(value)) edition = value;
            else error(line, 'bad_argument', `edition must be 20NN, got '${value}'`);
        } else if (key === 'targets') {
            const list = value.split(' ').filter(Boolean);
            if (!list.length || list.some(target => !TARGETS.includes(target)) || new Set(list).size !== list.length) {
                error(line, 'bad_argument', `targets must be distinct names of ${TARGETS.join(', ')}`);
            } else {
                targets = list;
                targetsLine = line;
            }
        } else if (key === 'cli') {
            if (/["']/.test(value) || !value.trim()) error(line, 'bad_argument', 'cli takes words without quotes');
            else cli = value.split(/\s+/).filter(Boolean);
        }
    }
    if (!rules.length && !errors.some(e => e.name === 'rule_format')) error(start + 1, 'missing_rule', 'no // rule: directive');
    if (!expectLines.length) error(start + 1, 'missing_expect', 'no // expect: directive');
    const mode = seen.has('cli') ? 'cli' : 'normal';
    const expects = [];
    for (const { value, line } of expectLines) {
        let list;
        try {
            list = tokens(value);
        } catch (failure) {
            error(line, 'bad_argument', failure.message);
            continue;
        }
        if (!list.length || list[0].string) {
            error(line, 'unknown_expectation', `'${value}' is not an expectation`);
            continue;
        }
        const kind = list[0].text;
        try {
            const parsed = mode === 'cli' ? cliExpectation(kind, list.slice(1)) : normalExpectation(kind, list.slice(1));
            if (!parsed) error(line, 'unknown_expectation', `'${kind}' is not an expectation of the ${mode} mode`);
            else expects.push({ ...parsed, raw: value, at: line });
        } catch (failure) {
            if (!(failure instanceof BadArgument)) throw failure;
            error(line, 'bad_argument', failure.message);
        }
    }
    const kinds = new Set(expects.map(expect => expect.kind));
    if (mode === 'cli' && kinds.has('deterministic') && !cli.includes('$OUT') && !cli.some(word => word.includes('$OUT'))) {
        error(expects.find(expect => expect.kind === 'deterministic').at, 'bad_argument', 'deterministic needs $OUT in cli');
    }
    if (mode === 'normal' && kinds.has('trap') && kinds.has('stdout')) {
        error(expects.find(expect => expect.kind === 'stdout').at, 'mixed_expectations', 'write the output of a trap as trap … stdout "…"');
    }
    if (targets) {
        const runs = ['stdout', 'trap', 'tests-pass'].some(kind => kinds.has(kind));
        if (mode === 'cli') error(targetsLine, 'targets_not_allowed', 'targets does not apply to cli');
        else if (!runs) error(targetsLine, 'targets_not_allowed', 'targets needs stdout, trap or tests-pass');
        else if ((kinds.has('stdout') || kinds.has('trap')) && targets.some(target => target.startsWith('wasm-'))) {
            error(targetsLine, 'targets_not_allowed', 'stdout and trap run natively only');
        }
    }
    errors.sort((a, b) => a.line - b.line || (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
    return { rules, ruleLines, expects, cli, edition, targets, mode, bodyLines: start, errors };
}
