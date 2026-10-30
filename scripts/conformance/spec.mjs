// The rule tables `spec/rules/<AREA>.md` (G33 §2), the headings that a Doc link may name, and the
// rule markers of the documents (§9).

import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { basename, join, posix, relative, resolve, sep } from 'node:path';

import { AREAS, RULE_ID } from './directives.mjs';

export const HEADER = '| ID | Rule | Doc | Tests | Status |';
export const MAX_TABLE_LINES = 1000;
export const MAX_RULE_LENGTH = 240;

function compare(a, b) {
    return a < b ? -1 : a > b ? 1 : 0;
}

/** The cells of a table row: split at `|` not preceded by `\`, trimmed, with `\|` read as `|`. */
export function splitRow(line) {
    return cellBounds(line).map(([start, end]) => line.slice(start, end).trim().replaceAll('\\|', '|'));
}

/** The `[start, end)` of each cell's text between the separators of a row. */
export function cellBounds(line) {
    const separators = [];
    for (let index = 0; index < line.length; index++) {
        if (line[index] === '|' && (index === 0 || line[index - 1] !== '\\')) separators.push(index);
    }
    const bounds = [];
    for (let index = 0; index + 1 < separators.length; index++) bounds.push([separators[index] + 1, separators[index + 1]]);
    return bounds;
}

/** `line` with the text of cell `index` replaced by `text`, its separators where they were. */
export function replaceCell(line, index, text) {
    const [start, end] = cellBounds(line)[index];
    return `${line.slice(0, start)} ${text.replaceAll('|', '\\|')} ${line.slice(end)}`;
}

/** The heading slugs of a Markdown text, as `scripts/check-docs.mjs` computes them. */
export function headingsOf(source) {
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
}

/** The lines of `text` with whether each is inside a code fence (the fence lines included). */
function fenced(text) {
    let inside = false;
    return text.split('\n').map(line => {
        const fence = /^```/.test(line);
        const result = { line, fenced: inside || fence };
        if (fence) inside = !inside;
        return result;
    });
}

const MARKER = /<!-- rule: (\S+) -->/g;
const TRAILING_MARKERS = /(?: <!-- rule: \S+ -->)+$/;

/** `line` with `replace` applied outside its inline code spans (`` `…` ``). */
function outsideCode(line, replace) {
    return line
        .split(/(`[^`]*`)/)
        .map((part, index) => (index % 2 ? part : replace(part)))
        .join('');
}

/** The rule markers outside code fences and inline code: `{ id, line }`. */
export function markersIn(text) {
    const found = [];
    fenced(text).forEach(({ line, fenced: inside }, index) => {
        if (inside) return;
        outsideCode(line, part => {
            for (const match of part.matchAll(MARKER)) found.push({ id: match[1], line: index + 1 });
            return part;
        });
    });
    return found;
}

/** `text` without its markers: a line of markers only goes, a marker inside a line goes alone. */
export function stripMarkers(text) {
    const out = [];
    for (const { line, fenced: inside } of fenced(text)) {
        if (inside) {
            out.push(line);
            continue;
        }
        // A marker at the end of a line (a list item's) goes with the blank before it.
        const stripped = outsideCode(line.replace(TRAILING_MARKERS, ''), part => part.replace(MARKER, ''));
        if (stripped !== line && stripped.trim() === '') continue;
        out.push(stripped);
    }
    return out.join('\n');
}

function walk(dir, prefix, found) {
    for (const name of readdirSync(dir).sort(compare)) {
        const path = join(dir, name);
        if (statSync(path).isDirectory()) walk(path, `${prefix}${name}/`, found);
        else if (name.endsWith('.md')) found.push(`${prefix}${name}`);
    }
}

/** The documents that markers may be in, relative to `root`: `docs/*.md`, `README.md`, `_tsuzuri/**`. */
export function docFiles(root) {
    const found = [];
    if (existsSync(join(root, 'docs'))) {
        for (const name of readdirSync(join(root, 'docs')).sort(compare)) {
            if (name.endsWith('.md') && statSync(join(root, 'docs', name)).isFile()) found.push(`docs/${name}`);
        }
    }
    if (existsSync(join(root, 'README.md'))) found.push('README.md');
    if (existsSync(join(root, '_tsuzuri'))) walk(join(root, '_tsuzuri'), '_tsuzuri/', found);
    return found.sort(compare);
}

/** Whether a Doc link may point at the root-relative `path`. */
export function allowedDoc(path) {
    return /^docs\/[^/]+\.md$/.test(path) || /^_tsuzuri\/language-reference\/.+\.md$/.test(path) || path === 'README.md';
}

/** The Doc cell for `page` (root-relative) and `heading`, or an error message. */
export function linkFor(root, page, heading) {
    const path = join(root, page);
    if (!existsSync(path)) return { error: `no page ${page}` };
    const target = posix.join('..', '..', page.split(sep).join('/'));
    if (heading === undefined) return { link: `[${basename(page)}](${target})` };
    const lines = readFileSync(path, 'utf8')
        .replace(/^```[^\n]*\n[\s\S]*?^```\s*$/gm, '')
        .split('\n')
        .filter(line => {
            const match = /^#{1,6} (.+)$/.exec(line);
            return match && match[1] === heading;
        });
    if (lines.length !== 1) return { error: `${lines.length ? 'more than one' : 'no'} heading '${heading}' in ${page}` };
    const slug = heading.toLowerCase().replace(/[^\p{L}\p{M}\p{N}\s_-]/gu, '').replace(/ /g, '-');
    return { link: `[${basename(page)} § ${heading}](${target}#${slug})` };
}

/** The root-relative path and the fragment of a Doc cell, or `null` when it is no single link. */
export function docTarget(cell) {
    const match = /^\[[^\]]+\]\(([^()\s]+)\)$/.exec(cell);
    if (!match) return null;
    const [path, fragment] = match[1].split('#');
    return { path: posix.normalize(posix.join('spec/rules', path)), fragment };
}

/**
 * The rows of every table of `root/spec/rules/` and the errors of their files and rows. A row is
 * `{ id, rule, doc, tests, status, file, line, area, text }`; `file` is root-relative.
 */
export function readRules(root) {
    const dir = join(root, 'spec', 'rules');
    const rules = [];
    const errors = [];
    if (!existsSync(dir)) return { rules, errors, missing: true };
    for (const name of readdirSync(dir).sort(compare)) {
        const file = `spec/rules/${name}`;
        const area = name.slice(0, -3);
        if (!name.endsWith('.md') || !AREAS.includes(area)) {
            errors.push({ path: file, line: 1, name: 'rule_file', message: `'${name}' is not <AREA>.md` });
            continue;
        }
        const lines = readFileSync(join(dir, name), 'utf8').split('\n');
        if (lines.length > MAX_TABLE_LINES + 1) {
            errors.push({ path: file, line: 1, name: 'rule_file', message: `more than ${MAX_TABLE_LINES} lines` });
            continue;
        }
        const header = lines.indexOf(HEADER);
        if (header < 0) {
            errors.push({ path: file, line: 1, name: 'rule_file', message: `no table with the header '${HEADER}'` });
            continue;
        }
        for (let index = header + 2; index < lines.length && lines[index].startsWith('|'); index++) {
            const cells = splitRow(lines[index]);
            if (cells.length !== 5) {
                errors.push({ path: file, line: index + 1, name: 'rule_row', message: `a row has ${cells.length} cells, not 5` });
                continue;
            }
            const [id, rule, doc, tests, status] = cells;
            rules.push({ id, rule, doc, tests, status, file, line: index + 1, area, text: lines[index] });
        }
    }
    return { rules, errors, missing: false };
}

/** The root-relative form of an absolute `path` under `root`, with `/`. */
export function rootRelative(root, path) {
    return relative(resolve(root), resolve(path)).split(sep).join('/');
}

export { RULE_ID };
