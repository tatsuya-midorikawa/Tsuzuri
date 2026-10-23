import { extractRole } from './tickets.mjs';
import { cmp, cp, take } from './text.mjs';

const joinOrNone = (values, separator = '; ') => values.length ? values.join(separator) : '(none)';
export const decisionId = value => /^(?:d-)?[0-9]{1,3}$/i.test(value) ? `D-${String(Number(value.replace(/^d-/i, ''))).padStart(2, '0')}` : null;
export function depends(kb, ticket, separator = '; ') {
    return joinOrNone(ticket.deps.map(d => `${d.id} ${kb.tickets.find(t => t.id === d.id)?.state ?? 'unknown'}${d.weak ? ' (weak)' : ''}`), separator);
}
export const followers = (kb, id) => kb.tickets.filter(t => t.deps.some(d => d.id === id)).map(t => t.id);
export function ticketText(kb, id) {
    const t = kb.tickets.find(t => t.id === id);
    if (!t) return null;
    return [
        `${t.id} ${t.title}`, `path: ${t.path}`,
        `state: ${t.state} | priority: ${t.priority} | size: ${t.size}${t.size_note ? ` (${t.size_note})` : ''} | completed: ${t.completed ? 'yes' : 'no'}`,
        `depends: ${depends(kb, t)}`, `followers: ${joinOrNone(followers(kb, id))}`,
        `files: ${joinOrNone(t.files.slice(0, 30).map(f => f.path + (f.exists ? '' : ' (new)')))}${t.files.length > 30 ? ` … ${t.files.length - 30} more` : ''}`,
        `appendices: ${joinOrNone(t.appendices)}`, 'headings:',
        ...t.headings.map(h => `  ${h.level} ${h.title} L${h.line}-L${h.end} (${h.chars} chars)`),
    ].join('\n');
}
export function decisionText(kb, number) {
    const d = kb.decisions.find(d => d.id === decisionId(number));
    if (!d) return null;
    return [`${d.id} ${d.title}`, `source: _features/GUIDE.md L${d.line}-L${d.end} (${d.chars} chars)`,
        `mentions: tickets ${joinOrNone(d.tickets, ' ')}; diags ${joinOrNone(d.diags, ' ')}; decisions ${joinOrNone(d.decisions, ' ')}`,
        '---', d.text].join('\n');
}
export function fileMatches(kb, path) {
    const match = value => path.endsWith('/') ? value.startsWith(path) : value === path;
    return {
        tickets: kb.tickets.filter(t => t.files.some(f => match(f.path)) || t.refs.files.some(match)),
        decisions: kb.decisions.filter(d => d.files.some(match)),
        modules: kb.modules.filter(m => m.files.some(f => match(f.path)) || m.dirs.some(match)),
    };
}
export function filesText(kb, path) {
    const found = fileMatches(kb, path);
    const match = value => path.endsWith('/') ? value.startsWith(path) : value === path;
    const lines = ['tickets:'];
    lines.push(...(found.tickets.length ? found.tickets.map(t => `  ${t.id} (${t.path}) ${t.files.some(f => match(f.path)) ? 'cell' : 'mention'}`) : ['  (none)']));
    lines.push('decisions:', ...(found.decisions.length ? found.decisions.map(d => `  ${d.id} mention`) : ['  (none)']));
    lines.push('modules:', ...(found.modules.length ? found.modules.map(m => `  ${m.source} table ${m.table} row ${m.row}`) : ['  (none)']));
    return lines.join('\n');
}
export function diagText(kb, code) {
    const d = kb.diagnostics.find(d => d.code === code);
    if (!d) return null;
    return [`${d.code} ${d.meaning || '(no row in compiler/diagnostics.md)'}`, `sites (${d.site_count}):`,
        ...d.sites.map(s => `  ${s.path}:${s.function ?? '(module)'} L${s.line}`),
        `pages: ${joinOrNone(d.pages)}`, `tickets: ${joinOrNone(d.tickets, ' ')}`, `decisions: ${joinOrNone(d.decisions, ' ')}`].join('\n');
}
export function searchText(kb, words) {
    const entries = [
        ...kb.tickets.map(t => ({ kind: 'ticket', key: t.id, text: `${t.title} ${t.headings.map(h => h.title).join(' ')}` })),
        ...kb.decisions.map(d => ({ kind: 'decision', key: d.id, text: d.title })),
        ...kb.diagnostics.map(d => ({ kind: 'diagnostic', key: d.code, text: d.meaning })),
        ...kb.modules.map(m => ({ kind: 'module', key: `${m.source}:${m.table}:${m.row}`, text: m.text })),
        ...kb.tests.map(t => ({ kind: 'test', key: `${t.path}:${t.name}`, text: t.name })),
        ...kb.docs.map(d => ({ kind: 'doc', key: d.path, text: `${d.title} ${d.headings.map(h => h.title).join(' ')}` })),
    ];
    const terms = words.flatMap(w => w.toLowerCase().split(/\s+/)).filter(Boolean);
    const matched = entries.filter(e => terms.every(w => `${e.key} ${e.text}`.toLowerCase().includes(w)));
    const exact = e => terms.length === 1 && e.key.toLowerCase() === terms[0] ? 0 : 1;
    matched.sort((a, b) => exact(a) - exact(b) || cmp(a.kind, b.kind) || cmp(a.key, b.key));
    return matched.length ? matched.slice(0, 40).map(e => `${e.kind}\t${e.key}\t${take(e.text.replace(/\s+/g, ' '), 100)}`).join('\n') : '(no matches)';
}
export function sectionText(ctx, kb, id, role, phase) {
    const t = kb.tickets.find(t => t.id === id);
    if (!t) return null;
    const part = extractRole(ctx, t, role, phase);
    if (!part) return null;
    return `=== SECTION ${id} ${part.heading.title} (${t.path}:${part.heading.line}-${part.heading.end}, ${cp(part.text)} chars) ===\n${part.text}`;
}
