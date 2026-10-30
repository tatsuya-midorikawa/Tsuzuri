// Generated blocks of the documents (G35): `<!-- generated:NAME -->` … `<!-- /generated:NAME -->`.
// `PLACEMENTS` lists every block and the file that holds it; the body of each is made from the
// compiler's own output (`explain --catalog --json`, `doc std`), never by hand.
//
// usage: node scripts/gen-docs.mjs [--check] [--list] [--only NAME]... [--bin PATH]

import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, readdirSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { renderMetricsBlock, renderSummaryBlock, validateMetrics, validateSummary } from '../benchmarks/render.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');

// ---- blocks (Phase 1) ---------------------------------------------------------
const START = /^<!-- generated:([a-z][a-z-]*(?::[A-Za-z0-9]+)?) -->$/;
const END = /^<!-- \/generated:([^ ]+) -->$/;
export function scanBlocks(text, file = 'text') {
    const blocks = [];
    let open = null;
    let fenced = false;
    text.split('\n').forEach((line, index) => {
        if (/^```/.test(line)) fenced = !fenced;
        if (fenced) return;
        const start = START.exec(line);
        const end = END.exec(line);
        if (start) {
            if (open) throw new Error(`${file}:${index + 1}: nested block '${start[1]}' inside '${open.name}'`);
            if (blocks.some((block) => block.name === start[1])) throw new Error(`${file}:${index + 1}: duplicate block '${start[1]}'`);
            open = { name: start[1], start: index };
        } else if (end) {
            if (!open || open.name !== end[1]) throw new Error(`${file}:${index + 1}: end marker without start '${end[1]}'`);
            blocks.push({ name: open.name, start: open.start, end: index });
            open = null;
        }
    });
    if (open) throw new Error(`${file}:${open.start + 1}: missing end marker for '${open.name}'`);
    return blocks;
}
export const bodyOf = (text, block) => text.split('\n').slice(block.start + 1, block.end);
export function replaceBlock(text, block, body) {
    const lines = text.split('\n');
    lines.splice(block.start + 1, block.end - block.start - 1, ...body);
    return lines.join('\n');
}
export function replaceNamedBlock(text, name, body, file) {
    const block = scanBlocks(text, file).find((candidate) => candidate.name === name);
    if (!block) throw new Error(`add the generated:${name} markers to ${file} first`);
    return replaceBlock(text, block, body);
}

/** The body of the `diagnostic-kinds` block: one row per code, its kinds in id order (G23's `kind_table`). */
export function diagnosticKindsTable(lines) {
    const byCode = new Map();
    for (const line of lines) {
        const { code, kind } = JSON.parse(line);
        if (!byCode.has(code)) byCode.set(code, []);
        byCode.get(code).push(kind);
    }
    return ['| コード | kind |', '| --- | --- |',
        ...[...byCode.keys()].sort().map((code) => `| \`${code}\` | ${byCode.get(code).sort().map((kind) => `\`${kind}\``).join(', ')} |`)];
}

// ---- tags and the complexity table (Phase 2) -------------------------------------
export function tagNotes(text, tag) {
    if (!text.includes(tag)) return [];
    const notes = [];
    for (const line of text.split(/\r?\n/)) {
        const rest = line.trimStart();
        if (!rest.startsWith(tag)) continue;
        const after = rest.slice(tag.length);
        if (after === '' || after.startsWith(' ') || after.startsWith('\t')) notes.push(after.trim());
    }
    return notes;
}
const FACTOR = '(?:1|[nmk]|(?:[nmk] )?log(?:\\^2)? [nmk])';
const PRODUCT = `${FACTOR}(?: \\* ${FACTOR})*`;
const EXPR = `${PRODUCT}(?: \\+ ${PRODUCT})*`;
const ONE = new RegExp(`^(?:(amortized|expected) )?O\\((${EXPR})\\)$`);
const TWO = new RegExp(`^(amortized|expected) O\\((${EXPR})\\), worst O\\((${EXPR})\\)$`);
export function parseComplexity(text) {
    const two = TWO.exec(text);
    if (two) return [{ qualifier: two[1], expr: two[2] }, { qualifier: 'worst', expr: two[3] }];
    const one = ONE.exec(text);
    return one ? [{ qualifier: one[1] ?? null, expr: one[2] }] : null;
}
const WORDS = { amortized: '償却', expected: '平均', worst: '最悪' };
export const renderComplexity = (clauses) => clauses
    .map(({ qualifier, expr }) => `${qualifier ? `${WORDS[qualifier]} ` : ''}$O(${expr.replace(/log(\^2)?/g, '\\log$1').replaceAll(' * ', ' \\cdot ')})$`)
    .join('、');

/** The `def` sections of one page of `tsuzuri doc std`: `[{ name, doc }]`, `doc` being the text after the signature. */
export function parseStdPage(text) {
    const entries = [];
    for (const section of text.split('\n## `').slice(1)) {
        const name = section.slice(0, section.indexOf('`'));
        const fence = /```tsuzuri\n([\s\S]*?)\n```/.exec(section);
        if (!fence || !/^(?:export )?(?:const )?def\b/.test(fence[1])) continue;
        entries.push({ name, doc: section.slice(fence.index + fence[0].length).trim() });
    }
    return entries;
}

const SINCE = /^[0-9]+\.[0-9]+\.[0-9]+$/;
const SEE = /^[A-Z][A-Za-z0-9]*(?:\.[A-Za-z_][A-Za-z0-9_]*)?$/;
export function complexityTable(module, entries) {
    const rows = entries.map(({ name, doc }) => {
        const id = `${module}.${name}`;
        const [value] = tagNotes(doc, '@complexity');
        if (value === undefined) throw new Error(`${id}: missing @complexity`);
        const clauses = parseComplexity(value);
        if (!clauses) throw new Error(`${id}: invalid @complexity "${value}"`);
        const [since] = tagNotes(doc, '@since');
        if (since !== undefined && !SINCE.test(since)) throw new Error(`${id}: invalid @since "${since}"`);
        const see = tagNotes(doc, '@see');
        for (const target of see) if (!SEE.test(target)) throw new Error(`${id}: invalid @see "${target}"`);
        return { id, cost: renderComplexity(clauses), since, see };
    });
    const withSince = rows.some((row) => row.since !== undefined);
    const withSee = rows.some((row) => row.see.length > 0);
    const header = ['関数', '計算量', ...(withSince ? ['版'] : []), ...(withSee ? ['関連'] : [])];
    return [`| ${header.join(' | ')} |`, `| ${header.map(() => '---').join(' | ')} |`,
        ...rows.map((row) => `| ${[`\`${row.id}\``, row.cost, ...(withSince ? [row.since ?? '-'] : []),
            ...(withSee ? [row.see.map((target) => `\`${target}\``).join(', ') || '-'] : [])].join(' | ')} |`)];
}

// ---- placements, plan, check, write ---------------------------------------------
export const PLACEMENTS = [
    { name: 'diagnostic-kinds', file: '_tsuzuri/language-reference/compiler/diagnostics.md', requires: 'explain' },
    { name: 'benchmark-summary', file: 'docs/benchmarks.md' },
    { name: 'perf-metrics', file: 'docs/benchmarks.md' },
    { name: 'complexity:Array', file: '_tsuzuri/language-reference/built-in-types-and-modules/array.md' },
    { name: 'complexity:Array', file: 'docs/language.md' },
    { name: 'complexity:Vec', file: '_tsuzuri/language-reference/built-in-types-and-modules/vec.md' },
    { name: 'complexity:Vec', file: 'docs/language.md' },
    { name: 'complexity:Map', file: '_tsuzuri/language-reference/built-in-types-and-modules/map.md' },
    { name: 'complexity:Map', file: 'docs/language.md' },
    { name: 'complexity:Set', file: '_tsuzuri/language-reference/built-in-types-and-modules/set.md' },
    { name: 'complexity:Set', file: 'docs/language.md' },
    { name: 'complexity:HashMap', file: '_tsuzuri/language-reference/built-in-types-and-modules/hashmap.md' },
    { name: 'complexity:HashMap', file: 'docs/language.md' },
    { name: 'complexity:HashSet', file: '_tsuzuri/language-reference/built-in-types-and-modules/hashset.md' },
    { name: 'complexity:HashSet', file: 'docs/language.md' },
    { name: 'complexity:List', file: '_tsuzuri/language-reference/built-in-types-and-modules/list.md' },
    { name: 'complexity:List', file: 'docs/language.md' },
    { name: 'complexity:String', file: '_tsuzuri/language-reference/built-in-types-and-modules/string.md' },
    { name: 'complexity:String', file: 'docs/language.md' },
];

const readJson = (base, file) => JSON.parse(readFileSync(resolve(base, file), 'utf8'));

function generator(name) {
    if (name === 'diagnostic-kinds') return (context) => diagnosticKindsTable(context.catalog());
    if (name === 'benchmark-summary') return (context) => renderSummaryBlock(validateSummary(readJson(context.root, 'docs/benchmarks-data/summary.json')));
    if (name === 'perf-metrics') return (context) => renderMetricsBlock(validateMetrics(readJson(context.root, 'docs/benchmarks-data/metrics.json')));
    const module = /^complexity:(.+)$/.exec(name)?.[1];
    if (module) {
        return (context) => {
            const entries = context.std()[module];
            if (!entries) throw new Error(`std has no module '${module}'`);
            return complexityTable(module, entries);
        };
    }
    throw new Error(`no generator for '${name}'`);
}

const selects = (placement, only) => only.length === 0 || only.some((name) => placement.name === name || placement.name.startsWith(`${name}:`));

export function planBlocks(context, { only = [], files, placements = PLACEMENTS } = {}) {
    const selected = placements.filter((placement) => selects(placement, only) && (!files || files.includes(resolve(context.root, placement.file))));
    const loaded = new Map();
    const read = (file) => {
        if (!loaded.has(file)) {
            const text = readFileSync(resolve(context.root, file), 'utf8');
            if (text.includes('\r')) throw new Error(`${file}: CRLF is not supported`);
            const blocks = scanBlocks(text, file);
            const known = new Set(placements.filter((placement) => placement.file === file).map((placement) => placement.name));
            for (const block of blocks) if (!known.has(block.name)) throw new Error(`${file}: unknown block '${block.name}'`);
            loaded.set(file, { text, blocks });
        }
        return loaded.get(file);
    };
    return selected.map((placement) => {
        if (placement.requires === 'explain' && !context.hasExplain()) return { placement, skipped: true };
        const { text, blocks } = read(placement.file);
        const block = blocks.find((candidate) => candidate.name === placement.name);
        if (!block) throw new Error(`${placement.file}: missing block '${placement.name}'${placement.requires === 'explain' ? ' (G23 step 4.2 adds the markers)' : ''}`);
        return { placement, text, block, expected: generator(placement.name)(context), actual: bodyOf(text, block) };
    });
}

export function checkBlocks(context, options = {}) {
    const plan = planBlocks(context, options);
    const stale = [];
    for (const { expected, actual, placement } of plan.filter((entry) => !entry.skipped)) {
        const index = expected.findIndex((line, at) => line !== actual[at]);
        if (index >= 0 || expected.length !== actual.length) {
            const at = index >= 0 ? index : Math.min(expected.length, actual.length);
            stale.push(`stale ${placement.file} (${placement.name})\n  line ${at + 1} of the block\n    expected: ${expected[at] ?? '(none)'}\n    found:    ${actual[at] ?? '(none)'}`);
        }
    }
    return { checked: plan.filter((entry) => !entry.skipped).length, stale, skipped: plan.filter((entry) => entry.skipped).map((entry) => entry.placement.name) };
}

export function writeBlocks(context, options = {}) {
    const plan = planBlocks(context, options).filter((entry) => !entry.skipped);
    const byFile = new Map();
    for (const item of plan) byFile.set(item.placement.file, [...(byFile.get(item.placement.file) ?? []), item]);
    let updated = 0;
    for (const [file, items] of byFile) {
        let text = items[0].text;
        let changed = 0;
        for (const item of [...items].sort((left, right) => right.block.start - left.block.start)) {  // from the end, so lines do not move
            if (item.expected.length === item.actual.length && item.expected.every((line, at) => line === item.actual[at])) continue;
            text = replaceBlock(text, item.block, item.expected);
            changed++;
        }
        if (changed) {
            const path = resolve(context.root, file);
            const temporary = `${path}.${process.pid}.tmp`;
            writeFileSync(temporary, text, { flag: 'wx' });
            renameSync(temporary, path);
            updated += changed;
        }
    }
    return { checked: plan.length, updated };
}

// ---- the real context ----------------------------------------------------------
export function createContext({ root: base, compiler }) {
    const run = (args) => spawnSync(compiler, args, { cwd: base, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, env: { ...process.env, TSUZURI_CACHE_DIR: '', TSUZURI_KG: '0' } });
    let explain;
    let catalog;
    let std;
    return {
        root: base,
        hasExplain() { return (explain ??= run(['explain', '--catalog', '--json']).status === 0); },
        catalog() {
            if (!catalog) {
                const result = run(['explain', '--catalog', '--json']);
                if (result.status !== 0) throw new Error(`explain --catalog --json failed: ${result.stderr}`);
                catalog = result.stdout.split('\n').filter(Boolean);
            }
            return catalog;
        },
        std() { return (std ??= readStd(base, run)); },
    };
}
export function readStd(base, run) {
    const out = join(base, 'target/gen-docs/std-doc');
    rmSync(out, { recursive: true, force: true });
    mkdirSync(dirname(out), { recursive: true });
    const result = run(['doc', join(base, 'std'), '-o', out]);
    if (result.status !== 0) throw new Error(`tsuzuri doc std failed: ${result.stderr}`);
    const modules = {};
    for (const file of readdirSync(out)) if (file.endsWith('.md') && file !== 'index.md') modules[file.slice(0, -3)] = parseStdPage(readFileSync(join(out, file), 'utf8'));
    return modules;
}

// ---- command line ----------------------------------------------------------------------
const USAGE = 'usage: node scripts/gen-docs.mjs [--check] [--list] [--only NAME]... [--bin PATH]';
const skipLine = (name) => `skip ${name}: this compiler has no 'explain --catalog --json' (G23); the block is not checked`;

export function main(argv) {
    const options = { check: false, list: false, only: [], bin: process.env.TSUZURI_BIN ?? join(root, 'target/release/tsuzuri') };
    for (let index = 0; index < argv.length; index += 1) {
        const argument = argv[index];
        if (argument === '--check') options.check = true;
        else if (argument === '--list') options.list = true;
        else if (argument === '--only' && index + 1 < argv.length) options.only.push(argv[++index]);
        else if (argument === '--bin' && index + 1 < argv.length) options.bin = resolve(argv[++index]);
        else {
            console.error(`gen-docs: unknown option '${argument}'\n${USAGE}`);
            return 2;
        }
    }
    if (options.list) {
        for (const { name, file } of PLACEMENTS) console.log(`${name}\t${file}`);
        return 0;
    }
    const unknown = options.only.find((name) => !PLACEMENTS.some((placement) => selects(placement, [name])));
    if (unknown !== undefined) {
        console.error(`gen-docs: no block named '${unknown}'`);
        return 2;
    }
    if (!existsSync(options.bin)) {
        console.error(`gen-docs: compiler not found: ${options.bin}`);
        return 2;
    }
    const context = createContext({ root, compiler: options.bin });
    try {
        if (options.check) {
            const { checked, stale, skipped } = checkBlocks(context, { only: options.only });
            for (const name of skipped) console.log(skipLine(name));
            if (stale.length > 0) {
                for (const report of stale) console.error(report);
                console.error("run 'node scripts/gen-docs.mjs' to update");
                return 1;
            }
            console.log(`gen-docs: ${checked} blocks checked, 0 stale`);
            return 0;
        }
        const { checked, updated } = writeBlocks(context, { only: options.only });
        for (const placement of PLACEMENTS) {
            if (placement.requires === 'explain' && selects(placement, options.only) && !context.hasExplain()) console.log(skipLine(placement.name));
        }
        console.log(`gen-docs: ${checked} blocks checked, ${updated} updated`);
        return 0;
    } catch (error) {
        console.error(error.message);
        return 1;
    }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) process.exitCode = main(process.argv.slice(2));
