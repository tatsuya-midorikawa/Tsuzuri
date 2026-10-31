// G35: the link check into a document (`scripts/check-links.mjs`), on a fake tree of two documents.
//
// usage: node tests/check_links.mjs

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { checkLinks } from '../scripts/check-links.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const scratch = mkdtempSync(join(tmpdir(), 'tsuzuri-check-links-'));
const test = (name, check) => {
    check();
    console.log(`check-links: ${name} passed`);
};

try {
    writeFileSync(join(scratch, 'a.md'), '# A\n\n[x](b.md#h1) [y](b.md#nope) [z](missing.md)\n\n```text\n[w](b.md#fenced)\n```\n');
    writeFileSync(join(scratch, 'b.md'), '# H1\n');
    test('links into a file are counted and broken fragments reported', () => {
        assert.deepEqual(checkLinks({ root: scratch, files: ['a.md', 'b.md'], into: ['b.md'] }), {
            links: 2,
            withFragment: 2,
            distinct: 2,
            broken: ['a.md: missing heading b.md#nope'],
        });
    });
    test('a missing file is reported', () => {
        assert.deepEqual(checkLinks({ root: scratch, files: ['a.md'], into: ['missing.md'] }).broken, ['a.md: missing file missing.md']);
    });
    test('--list-headings prints the slugs and an unknown flag is an error', () => {
        const script = join(root, 'scripts/check-links.mjs');
        const listed = spawnSync(process.execPath, [script, '--list-headings', 'b.md'], { cwd: scratch, encoding: 'utf8' });
        assert.equal(listed.status, 0, listed.stderr);
        assert.equal(listed.stdout, 'h1\n');
        const unknown = spawnSync(process.execPath, [script, '--x'], { cwd: scratch, encoding: 'utf8' });
        assert.equal(unknown.status, 2);
        assert.match(unknown.stderr, /^check-links: unknown option '--x'/);
    });
} finally {
    rmSync(scratch, { recursive: true, force: true });
}
