import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, extname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const compiler = resolve(process.env.TSUZURI_BIN ?? join(root, 'target/release/tsuzuri'));
const temporary = mkdtempSync(join(tmpdir(), 'tsuzuri-docs-'));
const requested = process.argv.slice(2).map(path => resolve(path));
const pages = [];
let examples = 0;
let executions = 0;
let tests = 0;
let links = 0;

function collect(directory) {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
        const path = join(directory, entry.name);
        if (entry.isDirectory()) collect(path);
        else if (extname(path) === '.md') pages.push(path);
    }
}

function headings(source) {
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

try {
    collect(join(root, '_tsuzuri'));
    pages.sort();
    for (const page of pages) {
        if (requested.length && !requested.includes(page)) continue;
        const source = readFileSync(page, 'utf8');
        const label = relative(root, page);
        assert.ok(source.endsWith('\n'), `${label}: missing final newline`);
        assert.ok(!/^[ \t]+```tsuzuri/m.test(source), `${label}: an indented tsuzuri code fence is not checked; start it at column 1`);
        const prose = source.replace(/^```[^\n]*\n[\s\S]*?^```\s*$/gm, '');
        for (const match of prose.matchAll(/\[[^\]\n]+\]\(([^\s)]+)\)/g)) {
            const target = match[1];
            if (/^[a-z][a-z0-9+.-]*:/i.test(target)) continue;
            const [path, fragment] = target.split('#');
            const destination = path ? resolve(dirname(page), decodeURIComponent(path)) : page;
            assert.ok(existsSync(destination), `${label}: missing link ${target}`);
            if (fragment && extname(destination) === '.md') {
                assert.ok(headings(readFileSync(destination, 'utf8')).has(decodeURIComponent(fragment)), `${label}: missing heading ${target}`);
            }
            links++;
        }
        if (existsSync(join(dirname(page), '.tsuzuri-docs'))) continue;
        const projects = new Map();
        let block = 0;
        for (const match of source.matchAll(/^```tsuzuri(?: ([^\n]+))?\n([\s\S]*?)^```\s*$/gm)) {
            block++;
            const options = Object.fromEntries((match[1] ?? '').split(/\s+/).filter(Boolean).map(option => {
                const separator = option.indexOf('=');
                assert.ok(separator > 0, `${label}: invalid example option ${option}`);
                return [option.slice(0, separator), option.slice(separator + 1)];
            }));
            assert.ok(Object.keys(options).every(key => ['project', 'file', 'run'].includes(key)), `${label}: unknown example option`);
            const key = options.project ?? `block-${block}`;
            const project = projects.get(key) ?? { files: new Map(), expected: undefined };
            const file = options.file ?? 'Main.tz';
            assert.ok(!file.includes('..') && !file.startsWith(sep) && ['.tz', '.tt', '.tc'].includes(extname(file)), `${label}: invalid example file`);
            assert.ok(!project.files.has(file), `${label}: duplicate example file ${file}`);
            project.files.set(file, match[2]);
            if (options.run !== undefined) project.expected = decodeURIComponent(options.run);
            projects.set(key, project);
        }
        for (const [key, project] of projects) {
            const directory = mkdtempSync(join(temporary, 'example-'));
            for (const [file, content] of project.files) {
                const path = join(directory, file);
                mkdirSync(dirname(path), { recursive: true });
                writeFileSync(path, content);
            }
            const context = `${label} (${key})`;
            const invoke = args => {
                try {
                    return execFileSync(compiler, args, { encoding: 'utf8', maxBuffer: 4 * 1024 * 1024 });
                } catch (error) {
                    throw new Error(`${context}: ${error.stderr ?? error.message}`, { cause: error });
                }
            };
            const input = join(directory, project.files.has('Main.tz') ? 'Main.tz' : project.files.keys().next().value);
            invoke(['check', input]);
            examples++;
            if (project.expected !== undefined) {
                for (const optimization of ['-O0', '-O3']) {
                    assert.equal(invoke(['run', directory, optimization]).trim(), project.expected, `${context} ${optimization}`);
                    executions++;
                }
            }
            if ([...project.files.values()].some(content => /^test\s+"/m.test(content))) {
                invoke(['test', directory]);
                tests++;
            }
        }
    }
    assert.ok(!requested.length || requested.every(path => pages.includes(path)), 'Requested page not found');
    console.log(`Docs: ${requested.length || pages.length} pages, ${links} links, ${examples} checked examples, ${executions} native runs (O0/O3), ${tests} test projects.`);
} finally {
    rmSync(temporary, { recursive: true, force: true });
}
