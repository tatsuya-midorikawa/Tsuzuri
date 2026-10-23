import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { parseMarkdown, sectionLines } from './markdown.mjs';
import { filterPhase, roleHeading } from './tickets.mjs';
import { depends, followers, relatedCommands, symbolText } from './query.mjs';
import { cmp, cp, readText, take } from './text.mjs';

export const STAGES = [
    { interfaces: 6000, decisionCount: 8, decisionChars: Infinity, symbols: 4000, commands: 1200, lite: Infinity },
    { interfaces: 2500, decisionCount: 4, decisionChars: 1500, symbols: 0 },
    { spec: 9000, design: 9000, tests: 3000, interfaces: 1200, decisionCount: 2, decisionChars: 1200 },
    { spec: 5000, design: 4000, steps: 6000, tests: 1500, accept: 2000, interfaces: 0, decisionCount: 0, commands: 600 },
    { start: 3500, spec: 2500, design: 2000, steps: 4500, tests: 800, accept: 1200 },
    { quick: 2000, start: 1500, spec: 800, design: 600, steps: 1800, tests: 400, accept: 600, commands: 300 },
    { lite: 3000 },
];
export function cut(text, cap, pointer) {
    const total = cp(text);
    const command = /^(?:cat|sed|node) /.test(pointer) ? pointer : `node scripts/repo-kb.mjs ${pointer}`;
    if (total <= cap) return { text, shown: total, total, state: 'full' };
    if (!cap) return { text: `[omitted: rest: ${command}]`, shown: 0, total, state: 'omitted' };
    const lines = [];
    let count = 0;
    for (const line of text.split('\n')) {
        const length = cp(line) + (lines.length ? 1 : 0);
        if (count + length > cap) { if (!lines.length) lines.push(take(line, cap)); break; }
        lines.push(line);
        count += length;
    }
    const shown = lines.join('\n');
    return { text: `${shown}\n[shortened: ${cp(shown)} of ${total} chars; rest: ${command}]`,
        shown: cp(shown), total, state: 'shortened' };
}
export function brief(ctx, kb, id, opts) {
    const ticket = kb.tickets.find(t => t.id === id);
    if (!ticket) return null;
    const read = path => ctx.texts?.get(path) ?? readText(join(ctx.root, path));
    const source = read(ticket.path);
    const md = parseMarkdown(source);
    const roles = ['quick', 'start', 'spec', 'design', 'steps', 'tests', 'accept', ...opts.with];
    const parts = roles.map(role => {
        const heading = roleHeading(md, role);
        if (!heading) return { key: role, title: role, text: null, pointer: `section ${id} ${role}` };
        let text = sectionLines(md, heading);
        if (['spec', 'design', 'steps', 'tests', 'docs'].includes(role)) text = filterPhase(text, opts.phase, role === 'steps');
        return { key: role, title: `TICKET ${id} / ${role}: ${heading.title} (${ticket.path}:${heading.line}-${heading.end})`,
            text, pointer: `section ${id} ${role} --phase ${opts.phase}` };
    });
    const interfaces = [];
    const own = roleHeading(md, 'needs');
    if (own) interfaces.push({ id, role: 'needs', text: sectionLines(md, own) });
    for (const dep of ticket.deps.filter(d => !d.weak)) {
        const t = kb.tickets.find(t => t.id === dep.id);
        if (!t) continue;
        const parsed = parseMarkdown(read(t.path));
        const gives = roleHeading(parsed, 'gives');
        if (gives) interfaces.push({ id: t.id, role: 'gives', text: `=== INTERFACE PROVIDED BY ${t.id} ===\n${sectionLines(parsed, gives)}` });
    }
    const cited = new Set();
    const referenceText = md.lines.slice(0, (md.headings.find(h => h.level === 2)?.line ?? md.lines.length + 1) - 1).join('\n')
        + parts.map(p => p.text ?? '').join('\n');
    for (const m of referenceText.matchAll(/D-([0-9]{2,3})(?:[〜–~]D-([0-9]{2,3}))?(?![0-9])/g)) {
        for (let n = Number(m[1]); n <= Math.min(Number(m[2] ?? m[1]), Number(m[1]) + 59); n++) cited.add(`D-${String(n).padStart(2, '0')}`);
    }
    const decisions = [...cited].map(id => kb.decisions.find(d => d.id === id)).filter(Boolean);
    const symbols = symbolText(ctx, ticket, source);
    const commands = relatedCommands(kb, ticket, source);
    const litePath = '_features/GUIDE-LITE.md';
    const lite = opts.lite && (ctx.texts?.has(litePath) || existsSync(join(ctx.root, litePath))) ? read(litePath) : null;
    function render(caps) {
        const content = [];
        const bodies = [];
        function add(key, title, text, cap, pointer) {
            if (text === null) { content.push(`${key} ${title} 0/0 omitted (absent)`); return; }
            const result = cut(text, cap ?? Infinity, pointer);
            content.push(`${key} ${title.split(' (')[0]} ${result.shown}/${result.total} ${result.state}`);
            bodies.push(`=== ${title} ===\n${result.text}`);
        }
        for (const p of parts) add(p.key, p.title, p.text, caps[p.key], p.pointer);
        add('interfaces', 'INTERFACES', interfaces.map(part => part.text).join('\n\n'), caps.interfaces,
            interfaces.map(part => `node scripts/repo-kb.mjs section ${part.id} ${part.role}`).join('; '));
        for (const d of decisions.slice(0, caps.decisionCount)) {
            add(d.id, `DECISION ${d.id} ${d.title} (_features/GUIDE.md:${d.line})`, d.text,
                caps.decisionChars, `decision ${d.id}`);
        }
        const omitted = decisions.slice(caps.decisionCount);
        if (omitted.length) {
            content.push(`decisions DECISIONS 0/${omitted.reduce((n, d) => n + cp(d.text), 0)} omitted`);
            bodies.push(`=== DECISIONS ===\n[omitted: ${omitted.map(d => d.id).join(', ')}; rest: node scripts/repo-kb.mjs decision ${omitted[0].id}; repeat for each ID]`);
        }
        if (symbols) {
            const result = cut(symbols, caps.symbols, `section ${id} symbols`);
            content.push(`symbols PUBLIC SYMBOLS ${result.shown}/${result.total} ${result.state}`);
            bodies.push(result.text);
        }
        const commandText = commands.slice(0, 12).join('\n')
            + (commands.length > 12 ? `\n[omitted: ${commands.length - 12} more commands; rest: node scripts/repo-kb.mjs section ${id} commands]` : '');
        add('commands', 'TESTS AND COMMANDS', commandText, caps.commands, `section ${id} commands`);
        if (lite !== null) add('lite', 'GUIDE-LITE (_features/GUIDE-LITE.md)', lite, caps.lite, 'cat _features/GUIDE-LITE.md');
        const affected = ticket.files.slice(0, 20).map(f => f.path + (f.exists ? '' : ' (new)')).join(', ') || '(none)';
        return [
            `=== REPO-KB BRIEF v1: ${id} phase=${opts.phase} max-chars=${opts.maxChars} ===`,
            ticket.title, `state: ${ticket.state} | priority: ${ticket.priority} | size: ${ticket.size} | file: ${ticket.path}`,
            `depends: ${depends(kb, ticket, ', ')}`, `followers: ${followers(kb, id).join(', ') || '(none)'}`,
            `affects: ${affected}${ticket.files.length > 20 ? ` … ${ticket.files.length - 20} more` : ''}`,
            '=== HOW TO USE THIS PACK ===',
            '1. Read the ticket parts in order; GUIDE-LITE at the end holds the rules every ticket assumes.',
            '2. A line starting with "[shortened" or "[omitted" names the command that prints the missing text; do not open whole tickets or GUIDE.md first.',
            '3. Report anything that contradicts the repository instead of improvising (GUIDE-LITE section 9).',
            '=== CONTENTS ===', ...content.slice(0, 24), ...bodies, '=== END ===', '',
        ].join('\n');
    }
    const caps = {};
    for (let stage = 0; stage < STAGES.length; stage++) {
        Object.assign(caps, STAGES[stage]);
        if (stage === 6 && lite !== null) {
            const minimum = render(caps);
            caps.lite = Math.max(3000, 3000 + opts.maxChars - cp(minimum) - 100);
        }
        const result = render(caps);
        if (cp(result) <= opts.maxChars) return result;
    }
    const error = new Error('cannot fit in --max-chars');
    error.status = 1;
    throw error;
}
