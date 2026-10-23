import { basename, join } from 'node:path';
import { parseMarkdown, sectionLines, tables } from './markdown.mjs';
import { expandCell } from './repo.mjs';
import { mentions } from './decisions.mjs';
import { cp, readText, take } from './text.mjs';

export const ROLES = {
    quick: ['実装者向け早見'], purpose: ['目的', '目的と実装範囲'], start: ['着手条件と停止条件'],
    current: ['現状', '現状と計測'], frozen: ['変えてはいけない意味'],
    spec: ['仕様', '確定した仕様', '確定した API と意味', '実装仕様', '現在の構文'],
    design: ['設計'], steps: ['実装手順'], tests: ['テスト計画'], docs: ['ドキュメント'],
    accept: ['受け入れ条件'], pitfalls: ['落とし穴'], nongoals: ['対象外'],
    decisions: ['決定事項', '未決事項', '決定事項の結果'], needs: ['前提とする他チケットのインターフェース'],
    gives: ['他チケットへの提供インターフェース', '他チケットへ提供するインターフェース', '後続に提供するインターフェース'],
};
export const normalized = title => title.split('（')[0].trim();
export function roleHeading(md, role) {
    for (const name of ROLES[role] ?? [role]) {
        const found = md.headings.find(h => normalized(h.title) === name
            && (h.level === 2 || (h.level === 3 && (!ROLES[role] || ['needs', 'gives'].includes(role)))));
        if (found) return found;
    }
}
export function classify(path) {
    const name = basename(path);
    if (['README.md', 'GUIDE.md', 'GUIDE-LITE.md'].includes(name)) return null;
    const appendix = /^([A-Z]{1,2}[0-9]{2})-appendix(?:-[a-z0-9][a-z0-9-]*)?\.md$/.exec(name);
    if (appendix) return { kind: 'appendix', id: appendix[1] };
    const ticket = /^([A-Z]{1,2}[0-9]{2})-[a-z0-9][a-z0-9-]*\.md$/.exec(name);
    return ticket ? { kind: 'ticket', id: ticket[1] } : { kind: 'skip' };
}
const unlink = text => text.replace(/\[([^\]]+)\]\([^)]+\)/g, '$1');
export function parseDeps(cell, known, warn = () => {}) {
    const text = unlink(cell);
    let depth = 0;
    let cut = text.length;
    for (let i = 0; i < text.length; i++) {
        if ('(（'.includes(text[i])) depth++;
        else if (')）'.includes(text[i])) depth = Math.max(0, depth - 1);
        else if (text[i] === '。' && !depth) { cut = i; break; }
    }
    const list = text.slice(0, cut).replace(/（[^（）]*）/g, s => ' '.repeat(s.length));
    const deps = new Map();
    for (const m of list.matchAll(/(?<![A-Za-z0-9])([A-Z]{1,2}[0-9]{2})(?![A-Za-z0-9])/g)) {
        const before = list.slice(Math.max(0, m.index - 8), m.index);
        if (!deps.has(m[1])) deps.set(m[1], /\($|弱依存\s*$|弱い依存[:：]\s*$/.test(before));
    }
    for (const m of text.slice(cut + 1).matchAll(/弱い依存[:：]\s*\[?([A-Z]{1,2}[0-9]{2})(?![A-Za-z0-9])/g)) {
        if (!deps.has(m[1])) deps.set(m[1], true);
    }
    return [...deps].filter(([id]) => { if (known.has(id)) return true; warn(id); return false; }).map(([id, weak]) => ({ id, weak }));
}
export function parseFilesCell(cell, ctx) {
    const found = new Map();
    for (const m of unlink(cell).matchAll(/`([^`]+)`/g)) {
        for (const file of expandCell(m[1], ctx)) if (!found.has(file.path)) found.set(file.path, file);
    }
    return [...found.values()].slice(0, 200);
}
export function parseTicket(ctx, path, completed) {
    const id = classify(path).id;
    const text = ctx.texts?.get(path) ?? readText(join(ctx.root, path));
    const md = parseMarkdown(text);
    const heading = /^# ([A-Z]{1,2}[0-9]{2}): (.+)$/.exec(md.lines[0]);
    const warn = message => ctx.warnings.push(message);
    if (!heading || heading[1] !== id) warn(`ticket_id_mismatch ${path}`);
    const table = tables(md).find(t => t.header.join('|') === '項目|内容');
    const meta = new Map();
    for (const row of table?.rows ?? []) {
        const name = row[0].replace(/（実装時）$/, '');
        if (!meta.has(name)) meta.set(name, row.slice(1).join('|'));
    }
    if (!table) warn(`ticket_table_missing ${path}`);
    if (table) for (const field of ['ID', '優先度', '規模', '依存', '状態']) {
        if (!meta.has(field)) warn(`ticket_field_missing ${id}: ${field}`);
    }
    const priority = /^(P[0-3])(?:（[^]*）)?$/.exec(meta.get('優先度') ?? '')?.[1] ?? null;
    const sizeRaw = meta.get('規模') ?? '';
    const size = /(?<![A-Za-z])(XL|S|M|L)(?![A-Za-z])/.exec(sizeRaw)?.[1] ?? null;
    const stateRaw = meta.get('状態') ?? '';
    const stateCell = /^(todo|done)/.exec(stateRaw)?.[1] ?? 'unknown';
    if (table) {
        if (!priority) warn(`ticket_field_invalid ${id}: priority`);
        if (!size) warn(`ticket_field_invalid ${id}: size`);
        if (stateCell === 'unknown') warn(`ticket_field_invalid ${id}: state`);
        if ((completed && stateCell !== 'done') || (!completed && stateCell === 'done')) {
            warn(`state_conflict ${id}: cell '${stateCell}', but the file is ${completed ? 'under _completed/' : 'not under _completed/'}`);
        }
    }
    const depsRaw = meta.get('依存') ?? '';
    return {
        id, title: md.lines[0].replace(/^# /, ''), path, completed, priority, size,
        size_note: sizeRaw === size ? '' : take(sizeRaw, 200), state: completed ? 'done' : stateCell,
        state_cell: stateCell, state_raw: take(stateRaw, 160),
        deps: parseDeps(depsRaw, ctx.ticketIds, dep => warn(`dep_unknown ${id}: ${dep}`)),
        deps_raw: take(depsRaw, 600), followers_raw: take(meta.get('後続') ?? '', 600),
        files: parseFilesCell(meta.get('主な影響ファイル') ?? '', ctx),
        refs: mentions(text, ctx, id), appendices: [], chars: cp(text),
        headings: md.headings.filter(h => h.level === 2 || h.level === 3).slice(0, 300),
    };
}
export function filterPhase(lines, phase, sticky = false) {
    const text = Array.isArray(lines) ? lines.join('\n') : lines;
    const md = parseMarkdown(text);
    const starts = md.headings.filter(h => h.level === 3 || h.level === 4);
    if (!starts.some(h => /^Phase\s*[0-9]+/.test(h.title))) return text;
    let current = 0;
    const parts = [md.lines.slice(0, (starts[0]?.line ?? 1) - 1).join('\n')];
    for (let i = 0; i < starts.length; i++) {
        const h = starts[i];
        const m = /^Phase\s*([0-9]+)/.exec(h.title);
        if (m) current = Number(m[1]);
        else if (!sticky) current = 0;
        const body = md.lines.slice(h.line - 1, (starts[i + 1]?.line ?? md.lines.length + 1) - 1).join('\n');
        parts.push(!current || current === phase ? body : `[Phase ${current} omitted: ${h.title} (${cp(body)} chars)]`);
    }
    return parts.join('\n');
}
export function extractRole(ctx, ticket, role, phase) {
    const md = parseMarkdown(ctx.texts?.get(ticket.path) ?? readText(join(ctx.root, ticket.path)));
    const heading = roleHeading(md, role);
    if (!heading) return null;
    let text = sectionLines(md, heading);
    const canonical = Object.entries(ROLES).find(([, names]) => names.includes(normalized(heading.title)))?.[0];
    if (phase !== undefined && ['steps', 'spec', 'design', 'tests', 'docs'].includes(canonical)) text = filterPhase(text, phase, canonical === 'steps');
    return { heading, text };
}
