import { join } from 'node:path';
import { parseMarkdown } from './markdown.mjs';
import { expandCell } from './repo.mjs';
import { cmp, cp } from './text.mjs';

export function mentions(text, ctx, self) {
    const unique = (values, n) => [...new Set(values)].sort(cmp).slice(0, n);
    const tickets = [...text.matchAll(/(?<![A-Za-z0-9-])([A-Z]{1,2}[0-9]{2})(?![A-Za-z0-9])/g)]
        .map(m => m[1]).filter(id => id !== self && ctx.ticketIds.has(id));
    const decisions = [];
    const decisionIds = ctx.decisionIds ?? new Set();
    for (const m of text.matchAll(/D-([0-9]{2,3})(?:[〜–~]D-([0-9]{2,3}))?(?![0-9])/g)) {
        const start = Number(m[1]);
        const end = Math.min(Number(m[2] ?? m[1]), start + 59);
        for (let n = start; n <= end; n++) {
            const id = `D-${String(n).padStart(2, '0')}`;
            if (id !== self && decisionIds.has(id)) decisions.push(id);
        }
    }
    const diags = [...text.matchAll(/\b([EW][0-9]{4})\b/g)].map(m => m[1]);
    const files = [];
    for (const m of text.matchAll(/`([^`]+)`/g)) {
        if (/[*{}]/.test(m[1])) continue;
        for (const file of expandCell(m[1], ctx)) if (file.exists) files.push(file.path);
    }
    return { tickets: unique(tickets, 100), decisions: unique(decisions, 60), diags: unique(diags, 100), files: unique(files, 300) };
}
export function parseDecisions(text) {
    const md = parseMarkdown(text);
    const result = [];
    for (const h of md.headings) {
        const m = /^(D-[0-9]{2,3}) (.+)$/.exec(h.title);
        if (h.level !== 3 || !m) continue;
        const body = md.lines.slice(h.line, h.end).join('\n').trim();
        result.push({ id: m[1], title: m[2], line: h.line, end: h.end, chars: cp(md.lines.slice(h.line - 1, h.end).join('\n')),
            text: body, tickets: [], diags: [], decisions: [], files: [] });
    }
    return result.sort((a, b) => cmp(a.id, b.id));
}
