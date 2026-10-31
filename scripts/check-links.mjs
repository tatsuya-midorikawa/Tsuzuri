// Relative links into a document (G35): every `[text](path#fragment)` of the tracked Markdown files
// (outside code fences) that points into `--into` (default `docs/benchmarks.md`) must name an existing
// file and heading. `--list-headings PATH` prints the heading slugs of a document, one per line.
//
// usage: node scripts/check-links.mjs [--into PATH]... [--list-headings PATH]

import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, extname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');

/** The heading slugs of a Markdown text, in order: the rule of `headings` in `scripts/check-docs.mjs`. */
export function headings(source) {
    const found = [];
    const duplicates = new Map();
    for (const line of source.replace(/^```[^\n]*\n[\s\S]*?^```\s*$/gm, '').split('\n')) {
        const heading = /^#{1,6} (.+)$/.exec(line);
        if (!heading) continue;
        const base = heading[1].toLowerCase().replace(/[^\p{L}\p{M}\p{N}\s_-]/gu, '').replace(/ /g, '-');
        const count = duplicates.get(base) ?? 0;
        found.push(count ? `${base}-${count}` : base);
        duplicates.set(base, count + 1);
    }
    return found;
}

/** The relative links of `files` that point into `into` (both relative to `base`), and the broken ones. */
export function checkLinks({ root: base, files, into }) {
    const targets = new Set(into.map((path) => resolve(base, path)));
    const cache = new Map();
    const anchors = (path) => cache.get(path) ?? cache.set(path, new Set(headings(readFileSync(path, 'utf8')))).get(path);
    let links = 0;
    let withFragment = 0;
    const distinct = new Set();
    const broken = [];
    for (const file of files) {
        const page = resolve(base, file);
        const prose = readFileSync(page, 'utf8').replace(/^```[^\n]*\n[\s\S]*?^```\s*$/gm, '');
        for (const match of prose.matchAll(/\[[^\]\n]+\]\(([^\s)]+)\)/g)) {
            const target = match[1];
            if (/^[a-z][a-z0-9+.-]*:/i.test(target)) continue;
            const [path, fragment] = target.split('#');
            const destination = path ? resolve(dirname(page), decodeURIComponent(path)) : page;
            if (!targets.has(destination)) continue;
            links++;
            if (!existsSync(destination)) { broken.push(`${file}: missing file ${target}`); continue; }
            if (!fragment) continue;
            withFragment++;
            distinct.add(fragment);
            if (extname(destination) === '.md' && !anchors(destination).has(decodeURIComponent(fragment))) broken.push(`${file}: missing heading ${target}`);
        }
    }
    return { links, withFragment, distinct: distinct.size, broken };
}

const USAGE = 'usage: node scripts/check-links.mjs [--into PATH]... [--list-headings PATH]';

export function main(argv) {
    const into = [];
    let listed;
    for (let index = 0; index < argv.length; index += 1) {
        if (argv[index] === '--into' && index + 1 < argv.length) into.push(argv[++index]);
        else if (argv[index] === '--list-headings' && index + 1 < argv.length) listed = argv[++index];
        else {
            console.error(`check-links: unknown option '${argv[index]}'\n${USAGE}`);
            return 2;
        }
    }
    if (listed !== undefined) {
        for (const heading of headings(readFileSync(resolve(listed), 'utf8'))) console.log(heading);
        return 0;
    }
    if (into.length === 0) into.push('docs/benchmarks.md');
    const files = execFileSync('git', ['ls-files', '*.md'], { cwd: root, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 }).split('\n').filter(Boolean);
    const { links, withFragment, distinct, broken } = checkLinks({ root, files, into });
    console.log(`check-links: ${links} links into ${into.join(', ')} (${withFragment} with a fragment, ${distinct} distinct), ${broken.length} broken`);
    for (const message of broken) console.error(message);
    return broken.length > 0 ? 1 : 0;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) process.exitCode = main(process.argv.slice(2));
