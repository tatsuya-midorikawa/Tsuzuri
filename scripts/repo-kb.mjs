import { dirname, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { build, ensureFresh, isFresh } from './repo-kb/build.mjs';
import { decisionId, decisionText, diagText, fileMatches, filesText, searchText, sectionText, ticketText } from './repo-kb/query.mjs';
import { brief } from './repo-kb/brief.mjs';

const defaultRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
export function checkNodeVersion(version) {
    if (Number(version.split('.')[0]) < 20) throw new Error('Node.js 20 or later is required');
}
function usage(message) {
    const error = new Error(message);
    error.status = 2;
    throw error;
}
export function options(argv) {
    const command = argv[0];
    if (!['build', 'verify', 'brief', 'ticket', 'section', 'decision', 'files', 'diag', 'search'].includes(command)) usage('unknown or missing command');
    const o = { root: defaultRoot, out: null, refresh: true, json: false, phase: command === 'brief' ? 1 : undefined, maxChars: 24000, lite: true, with: [], args: [] };
    const values = new Set(['--root', '--out', '--phase', '--max-chars', '--with']);
    for (let i = 1; i < argv.length; i++) {
        const a = argv[i];
        if (!a.startsWith('--')) { o.args.push(a); continue; }
        if (values.has(a)) {
            const value = argv[++i];
            if (!value || value.startsWith('--')) usage(`missing value for ${a}`);
            if (['--phase', '--max-chars'].includes(a) && !/^[0-9]+$/.test(value)) usage(`${a} requires an integer`);
            if (a === '--root') o.root = resolve(value);
            else if (a === '--out') o.out = resolve(value);
            else if (a === '--phase') o.phase = Number(value);
            else if (a === '--max-chars') o.maxChars = Number(value);
            else o.with = value.split(',');
        } else if (a === '--no-refresh') o.refresh = false;
        else if (a === '--json') o.json = true;
        else if (a === '--no-lite') o.lite = false;
        else usage(`unknown option: ${a}`);
        if (['--max-chars', '--no-lite', '--with'].includes(a) && command !== 'brief') usage(`${a} is only valid with brief`);
        if (a === '--phase' && !['brief', 'section'].includes(command)) usage('--phase is only valid with brief or section');
    }
    if (o.json && !['ticket', 'decision', 'files', 'diag'].includes(command)) usage('--json is not valid with this command');
    if (!Number.isSafeInteger(o.maxChars) || o.maxChars < 16000 || o.maxChars > 400000) usage('--max-chars must be between 16000 and 400000');
    if (o.phase !== undefined && (!Number.isSafeInteger(o.phase) || o.phase < 1)) usage('--phase must be a positive integer');
    if (o.with.some(role => !['frozen', 'pitfalls', 'decisions', 'docs', 'current', 'nongoals'].includes(role))) usage('invalid role for --with');
    const arity = { build: 0, verify: 0, brief: 1, ticket: 1, section: 2, decision: 1, files: 1, diag: 1 };
    if (command === 'search' ? !o.args.length : o.args.length !== arity[command]) usage('incorrect number of arguments');
    o.out ??= resolve(o.root, 'target/repo-kb');
    return { command, ...o };
}
export async function main(argv) {
    try {
        checkNodeVersion(process.versions.node);
        const o = options(argv);
        if (o.command === 'verify') {
            const fresh = await isFresh(o.root, o.out);
            console.log(fresh ? 'repo-kb: fresh' : 'repo-kb: stale (run: node scripts/repo-kb.mjs build)');
            return fresh ? 0 : 1;
        }
        if (o.command === 'build') {
            const kb = await build(o.root, o.out);
            for (const warning of kb.manifest.warnings) console.error(`repo-kb: warning: ${warning}`);
            const counts = Object.entries(kb.manifest.counts).filter(([key]) => key !== 'appendices').map(([key, value]) => `${key}=${value}`).join(' ');
            const out = relative(o.root, o.out).replaceAll('\\', '/') || '.';
            console.log(`repo-kb: ${counts} warnings=${kb.manifest.warnings.length} -> ${out}`);
            return 0;
        }
        const kb = await ensureFresh(o.root, o.out, { refresh: o.refresh });
        if (!kb) { console.error('repo-kb: stale (run: node scripts/repo-kb.mjs build)'); return 1; }
        const [id, name] = o.args;
        const ctx = { root: o.root };
        const query = {
            ticket: () => o.json ? kb.tickets.find(t => t.id === id) : ticketText(kb, id),
            decision: () => o.json ? kb.decisions.find(d => d.id === decisionId(id)) : decisionText(kb, id),
            files: () => o.json ? fileMatches(kb, id) : filesText(kb, id),
            diag: () => o.json ? kb.diagnostics.find(d => d.code === id) : diagText(kb, id),
            search: () => searchText(kb, o.args),
            section: () => sectionText(ctx, kb, id, name, o.phase),
            brief: () => brief(ctx, kb, id, o),
        };
        const result = query[o.command]();
        if (result === null || result === undefined) {
            const ticket = kb.tickets.find(t => t.id === id);
            console.error(o.command === 'section' && ticket
                ? `repo-kb: no such section: ${name} (have: ${ticket.headings.map(h => h.title).join(', ')})`
                : `repo-kb: not found: ${id}`);
            return 1;
        }
        console.log(o.json ? JSON.stringify(result) : result);
        return 0;
    } catch (error) {
        console.error(`repo-kb: ${error.code ? `cannot read requested repository or output (${error.code})` : error.message}`);
        return error.status ?? 3;
    }
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) process.exitCode = await main(process.argv.slice(2));
