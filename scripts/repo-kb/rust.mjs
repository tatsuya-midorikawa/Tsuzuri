import { cp, take } from './text.mjs';

export function scanRust(text) {
    const masked = text.split('');
    const literals = [];
    let i = 0;
    let line = 1;
    function mask(end) {
        for (; i < end; i++) {
            if (text[i] === '\n') line++;
            else masked[i] = ' ';
        }
    }
    while (i < text.length) {
        if (text.startsWith('//', i)) {
            const end = text.indexOf('\n', i);
            mask(end < 0 ? text.length : end);
        } else if (text.startsWith('/*', i)) {
            let end = i + 2;
            let depth = 1;
            while (end < text.length && depth) {
                if (text.startsWith('/*', end)) { depth++; end += 2; }
                else if (text.startsWith('*/', end)) { depth--; end += 2; }
                else end++;
            }
            mask(end);
        } else {
            const boundary = i === 0 || !/[A-Za-z0-9_]/.test(text[i - 1]);
            const raw = boundary && /^(?:br|r)(#{0,255})"/.exec(text.slice(i, i + 258));
            const quoted = text[i] === '"' || (boundary && text.startsWith('b"', i));
            if (raw) {
                const start = i + raw[0].length;
                const closer = `"${raw[1]}`;
                const closing = text.indexOf(closer, start);
                const end = closing < 0 ? text.length : closing + closer.length;
                literals.push({ value: text.slice(start, closing < 0 ? text.length : closing), offset: i, line });
                mask(end);
            } else if (quoted) {
                const start = i + (text[i] === 'b' ? 2 : 1);
                let end = start;
                while (end < text.length) {
                    if (text[end] === '\\') { end += 2; continue; }
                    if (text[end++] === '"') break;
                }
                literals.push({ value: text.slice(start, end - 1).replace(/\\(["\\])/g, '$1'), offset: i, line });
                mask(Math.min(end, text.length));
            } else if (text[i] === "'") {
                const char = /^'(?:\\(?:x[0-9a-fA-F]{2}|u\{[0-9a-fA-F_]+\}|[^\n])|[^\\'\n])'/u.exec(text.slice(i, i + 32));
                if (char) mask(i + char[0].length);
                else { i++; }
            } else {
                if (text[i] === '\n') line++;
                i++;
            }
        }
    }
    return { code: masked.join(''), literals };
}

function implName(header) {
    let s = header.trim();
    if (s.startsWith('<')) {
        let depth = 0;
        let i = 0;
        for (; i < s.length; i++) {
            if (s[i] === '<') depth++;
            if (s[i] === '>' && --depth === 0) { i++; break; }
            if (s[i] === '>' && depth < 0) break;
        }
        s = s.slice(i).trim();
    }
    const afterFor = /\bfor\s+([\s\S]*)/.exec(s);
    if (afterFor) s = afterFor[1];
    return /^(?:[A-Za-z_][A-Za-z0-9_]*::)*([A-Za-z_][A-Za-z0-9_]*)/.exec(s)?.[1] ?? null;
}

export function annotate(text) {
    const scanned = scanRust(text);
    const scopes = [];
    const ranges = [];
    let pending = null;
    let testAttribute = false;
    let round = 0;
    let square = 0;
    const context = start => {
        const fn = [...scopes].reverse().find(s => s.kind === 'fn');
        const owner = fn?.owner;
        ranges.push({ start, function: fn ? (owner ? `${owner}::${fn.name}` : fn.name) : null,
            test: scopes.some(s => s.test) });
    };
    context(0);
    const tokens = /#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]|\b(?:fn|impl|trait|mod)\b|[{}()[\];]/g;
    for (const m of scanned.code.matchAll(tokens)) {
        const t = m[0];
        if (t.startsWith('#')) { testAttribute = true; continue; }
        if (['fn', 'impl', 'trait', 'mod'].includes(t)) {
            if (pending) continue;
            const rest = scanned.code.slice(m.index + t.length);
            pending = { kind: t, start: m.index + t.length, round, square,
                name: /^\s+([A-Za-z_][A-Za-z0-9_]*)/.exec(rest)?.[1] ?? null,
                test: t === 'mod' && testAttribute };
            testAttribute = false;
        } else if (t === '(') round++;
        else if (t === ')') round--;
        else if (t === '[') square++;
        else if (t === ']') square--;
        else if (t === ';' && pending && round === pending.round && square === pending.square) pending = null;
        else if (t === '{') {
            if (pending && round === pending.round && square === pending.square) {
                if (pending.kind === 'impl') pending.name = implName(scanned.code.slice(pending.start, m.index));
                if (pending.kind === 'fn') pending.owner = [...scopes].reverse().find(s => ['impl', 'trait'].includes(s.kind))?.name;
                scopes.push(pending);
                pending = null;
            } else scopes.push({ kind: 'block' });
            context(m.index + 1);
        } else if (t === '}') {
            scopes.pop();
            context(m.index + 1);
        }
    }
    const at = offset => {
        let lo = 0;
        let hi = ranges.length;
        while (lo + 1 < hi) {
            const mid = (lo + hi) >> 1;
            if (ranges[mid].start <= offset) lo = mid;
            else hi = mid;
        }
        return ranges[lo];
    };
    return { ...scanned, at };
}
// The registry of diagnostic kinds (G23, D-51): its `(Variant, "id", "CODE")` rows declare the kinds, and a
// `DiagnosticKind::Variant` path elsewhere is where the compiler reports one.
export const KIND_REGISTRY = 'src/diagnostic_kinds.rs';
export function kindRegistry(text) {
    const scanned = annotate(text);
    const kinds = [];
    for (const m of text.matchAll(/\(\s*([A-Z][A-Za-z0-9]*)\s*,\s*"([a-z][a-z0-9_]*)"\s*,\s*"([EW][0-9]{4})"\s*\)/g)) {
        if (scanned.code[m.index] === '(' && !scanned.at(m.index).test) kinds.push({ variant: m[1], id: m[2], code: m[3] });
    }
    return kinds;
}
export function diagSites(text, variants = new Map()) {
    const scanned = annotate(text);
    const found = scanned.literals.filter(l => /^[EW][0-9]{4}$/.test(l.value)).map(l => ({ code: l.value, kind: '', offset: l.offset }));
    if (variants.size) {
        for (const m of scanned.code.matchAll(/\bDiagnosticKind\s*::\s*([A-Z][A-Za-z0-9]*)\b/g)) {
            const kind = variants.get(m[1]);
            if (kind) found.push({ code: kind.code, kind: kind.id, offset: m.index });
        }
    }
    const newlines = [...scanned.code.matchAll(/\n/g)].map(m => m.index);
    const lineOf = offset => {
        let lo = 0;
        let hi = newlines.length;
        while (lo < hi) {
            const mid = (lo + hi) >> 1;
            if (newlines[mid] < offset) lo = mid + 1;
            else hi = mid;
        }
        return lo + 1;
    };
    return found.sort((a, b) => a.offset - b.offset).map(s => ({
        code: s.code, kind: s.kind, function: scanned.at(s.offset).function, line: lineOf(s.offset), test: scanned.at(s.offset).test,
    }));
}
export function publicItems(text) {
    const scanned = annotate(text);
    const original = text.split('\n');
    const result = [];
    let offset = 0;
    for (const [i, line] of scanned.code.split('\n').entries()) {
        const declaration = /^[ \t]*pub(?:\([^)]*\))?\s+(?:(?:const|async|unsafe|extern)\s+)*(?:fn|struct|enum|trait|type|const|static|mod|union)\s+(?:mut\s+)?((?:r#)?[A-Za-z_][A-Za-z0-9_]*)/.exec(line);
        if (declaration && !scanned.at(offset).test) {
            const signature = original[i].trim();
            result.push({ line: i + 1, name: declaration[1], signature: cp(signature) > 140 ? `${take(signature, 137)}...` : signature });
        }
        offset += line.length + 1;
    }
    return result;
}
