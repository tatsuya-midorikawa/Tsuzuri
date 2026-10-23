import { existsSync, mkdirSync, readdirSync, renameSync, statSync, writeFileSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { listFiles, inputFiles, inputHash } from './repo.mjs';
import { classify, parseTicket } from './tickets.mjs';
import { mentions, parseDecisions } from './decisions.mjs';
import { parseModuleTables } from './modules.mjs';
import { diagSites } from './rust.mjs';
import { indexTests } from './tests-index.mjs';
import { indexDocs } from './docs-index.mjs';
import { cmp, jsonl, readText } from './text.mjs';

const names = ['tickets', 'decisions', 'modules', 'diagnostics', 'tests', 'docs'];
async function prepare(root) {
    const files = listFiles(root);
    const texts = new Map();
    const paths = inputFiles(files);
    for (let i = 0; i < paths.length; i += 32) {
        await Promise.all(paths.slice(i, i + 32).map(async path => {
            if (statSync(join(root, path)).size <= 8 * 1024 * 1024) {
                texts.set(path, (await readFile(join(root, path), 'utf8')).replace(/\r\n/g, '\n'));
            }
        }));
    }
    return { files, texts };
}
export function collect(root, prepared) {
    const files = prepared?.files ?? listFiles(root);
    const warnings = [];
    const inputs = inputFiles(files);
    const texts = prepared?.texts ?? new Map();
    for (const path of inputs) {
        if (statSync(join(root, path)).size > 8 * 1024 * 1024) warnings.push(`file_too_large ${path}`);
        else if (!texts.has(path)) texts.set(path, readText(join(root, path)));
    }
    const ctx = { root, files, fileSet: new Set(files), ticketIds: new Set(), decisionIds: new Set(), warnings, texts };
    const ticketPaths = [];
    const appendices = [];
    for (const path of files.filter(p => /^(?:_features|_perfs)\/.*\.md$/.test(p) && texts.has(p))) {
        const kind = classify(path);
        if (!kind) continue;
        if (kind.kind === 'skip') { warnings.push(`file_skipped ${path}`); continue; }
        if (kind.kind === 'appendix') { appendices.push({ path, id: kind.id }); continue; }
        if (ctx.ticketIds.has(kind.id)) throw new Error(`dup_ticket_id ${kind.id}`);
        ctx.ticketIds.add(kind.id);
        ticketPaths.push(path);
    }
    if (ticketPaths.length > 2000) throw new Error('ticket limit exceeded (2000)');
    const guide = texts.get('_features/GUIDE.md') ?? '';
    const decisions = parseDecisions(guide);
    const decisionNumbers = new Set();
    for (const d of decisions) {
        const n = Number(d.id.slice(2));
        if (decisionNumbers.has(n)) warnings.push(`decision_gap ${n}`);
        decisionNumbers.add(n);
        ctx.decisionIds.add(d.id);
        Object.assign(d, mentions(`${d.id} ${d.title}\n${d.text}`, ctx, d.id));
        delete d.refs;
    }
    for (let n = 1; n <= Math.max(0, ...decisionNumbers); n++) if (!decisionNumbers.has(n)) warnings.push(`decision_gap ${n}`);
    // All decision IDs must be registered before resolving forward references.
    for (const d of decisions) {
        const refs = mentions(`${d.id} ${d.title}\n${d.text}`, ctx, d.id);
        d.tickets = refs.tickets; d.diags = refs.diags; d.decisions = refs.decisions; d.files = refs.files;
    }
    const tickets = ticketPaths.map(path => {
        const t = parseTicket(ctx, path, path.includes('/_completed/'));
        t.appendices = appendices.filter(a => a.id === t.id).map(a => a.path);
        return t;
    }).sort((a, b) => cmp(a.id, b.id));
    for (const m of (texts.get('_features/README.md') ?? '').matchAll(/^\s*([A-Z]{1,2}\d{2})\s*(-->|-\.->)\s*([A-Z]{1,2}\d{2})/gm)) {
        const ticket = tickets.find(t => t.id === m[3]);
        if (!ticket || !ctx.ticketIds.has(m[1])) continue;
        const existing = ticket.deps.find(d => d.id === m[1]);
        if (existing) existing.weak = m[2] === '-.->';
        else ticket.deps.push({ id: m[1], weak: m[2] === '-.->' });
    }
    const modules = ['docs/architecture.md', '_features/GUIDE.md'].flatMap(path => texts.has(path) ? parseModuleTables(path, texts.get(path), ctx) : [])
        .sort((a, b) => cmp(a.source, b.source) || a.table - b.table || a.row - b.row);
    const sites = files.filter(p => /^src\/.*\.rs$/.test(p) && texts.has(p)).flatMap(path =>
        diagSites(texts.get(path)).filter(s => !s.test).map(s => ({ code: s.code, path, function: s.function, line: s.line })));
    const tests = indexTests({ ...ctx, files: files.filter(p => !inputs.includes(p) || texts.has(p)) });
    const docs = indexDocs({ ...ctx, files: files.filter(p => texts.has(p)) });
    const meanings = new Map();
    for (const m of (texts.get('_tsuzuri/language-reference/compiler/diagnostics.md') ?? '').matchAll(/^\| `([EW][0-9]{4})` \| (.+) \|$/gm)) {
        meanings.set(m[1], m[2].split(' | ')[0]);
    }
    const codes = new Set([...sites.map(s => s.code), ...meanings.keys()]);
    for (const t of tickets) for (const code of t.refs.diags) codes.add(code);
    for (const d of decisions) for (const code of d.diags) codes.add(code);
    for (const d of docs) for (const m of texts.get(d.path).matchAll(/\b[EW][0-9]{4}\b/g)) codes.add(m[0]);
    const diagnostics = [...codes].sort(cmp).map(code => {
        const allSites = sites.filter(s => s.code === code).sort((a, b) => cmp(a.path, b.path) || a.line - b.line);
        return { code, meaning: meanings.get(code) ?? '', site_count: allSites.length,
            sites: allSites.slice(0, 100).map(({ path, function: fn, line }) => ({ path, function: fn, line })),
            pages: docs.filter(d => new RegExp(`\\b${code}\\b`).test(texts.get(d.path))).map(d => d.path).slice(0, 30),
            tickets: tickets.filter(t => t.refs.diags.includes(code)).map(t => t.id),
            decisions: decisions.filter(d => d.diags.includes(code)).map(d => d.id) };
    });
    const manifest = { schema: 'tz-repo-kb/1', input_sha256: inputHash(root, files, texts), inputs: texts.size,
        counts: { tickets: tickets.length, appendices: appendices.length, decisions: decisions.length, modules: modules.length,
            diagnostics: diagnostics.length, tests: tests.length, docs: docs.length }, warnings: warnings.sort(cmp) };
    return { manifest, tickets, decisions, modules, diagnostics, tests, docs };
}
export async function build(root, out) {
    if (existsSync(out) && readdirSync(out).length && (!existsSync(join(out, '.repo-kb')) || readText(join(out, '.repo-kb')) !== 'tz-repo-kb/1\n')) {
        throw new Error('output directory is not a repo-kb directory');
    }
    const kb = collect(root, await prepare(root));
    mkdirSync(out, { recursive: true });
    const write = (name, text) => {
        writeFileSync(join(out, `${name}.tmp`), text);
        renameSync(join(out, `${name}.tmp`), join(out, name));
    };
    write('.repo-kb', 'tz-repo-kb/1\n');
    for (const name of names) write(`${name}.jsonl`, jsonl(kb[name]));
    write('manifest.json', `${JSON.stringify(kb.manifest)}\n`);
    return kb;
}
export function loadKb(out) {
    const kb = { manifest: JSON.parse(readText(join(out, 'manifest.json'))) };
    for (const name of names) kb[name] = readText(join(out, `${name}.jsonl`)).trim().split('\n').filter(Boolean).map(line => JSON.parse(line));
    return kb;
}
export async function isFresh(root, out) {
    if (!existsSync(join(out, 'manifest.json'))) return false;
    const manifest = JSON.parse(readText(join(out, 'manifest.json')));
    const { files, texts } = await prepare(root);
    return manifest.schema === 'tz-repo-kb/1' && manifest.input_sha256 === inputHash(root, files, texts);
}
export async function ensureFresh(root, out, { refresh = true } = {}) {
    if (!await isFresh(root, out)) {
        if (!refresh) return null;
        console.error('repo-kb: rebuilding (stale)');
        return build(root, out);
    }
    return loadKb(out);
}
