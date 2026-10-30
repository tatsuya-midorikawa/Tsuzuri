// G35: the generated blocks of the documents (`scripts/gen-docs.mjs`). The inputs are strings, a
// fixture page of `tsuzuri doc std` and documents in a temporary directory with a fake context, so
// no compiler is needed; every expectation is written by hand.
//
// usage: node tests/gen_docs.mjs

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import {
    PLACEMENTS,
    bodyOf,
    checkBlocks,
    complexityTable,
    diagnosticKindsTable,
    parseComplexity,
    parseStdPage,
    renderComplexity,
    replaceBlock,
    scanBlocks,
    tagNotes,
    writeBlocks,
} from '../scripts/gen-docs.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const scratch = mkdtempSync(join(tmpdir(), 'tsuzuri-gen-docs-'));
const test = (name, check) => {
    check();
    console.log(`gen-docs: ${name} passed`);
};

try {
    // ---- F.1: scanning blocks ----------------------------------------------------------------
    const sample = ['intro', '<!-- generated:complexity:Map -->', 'old', '<!-- /generated:complexity:Map -->', 'tail', '```', '<!-- generated:x -->', '```'].join('\n');
    test('a block is found and a fenced marker is not', () => {
        assert.deepEqual(scanBlocks(sample), [{ name: 'complexity:Map', start: 1, end: 3 }]);
    });
    test('replaceBlock swaps the body only', () => {
        const replaced = replaceBlock(sample, scanBlocks(sample)[0], ['new 1', 'new 2']).split('\n');
        assert.deepEqual(replaced.slice(0, 5), ['intro', '<!-- generated:complexity:Map -->', 'new 1', 'new 2', '<!-- /generated:complexity:Map -->']);
        assert.deepEqual(replaced.slice(5), ['tail', '```', '<!-- generated:x -->', '```']);
    });
    test('a nested block is an error', () => {
        assert.throws(() => scanBlocks('<!-- generated:a -->\n<!-- generated:b -->'), /nested block 'b' inside 'a'/);
    });
    test('a start without an end is an error', () => {
        assert.throws(() => scanBlocks('<!-- generated:a -->'), /missing end marker for 'a'/);
    });
    test('an end without a start is an error', () => {
        assert.throws(() => scanBlocks('<!-- /generated:a -->'), /end marker without start 'a'/);
    });
    test('a name used twice is an error', () => {
        const twice = '<!-- generated:a -->\n<!-- /generated:a -->\n<!-- generated:a -->\n<!-- /generated:a -->';
        assert.throws(() => scanBlocks(twice), /duplicate block 'a'/);
    });
    test('an indented marker is not a marker', () => {
        assert.deepEqual(scanBlocks(' <!-- generated:a -->\n <!-- /generated:a -->'), []);
    });
    test('an empty body is read and filled', () => {
        const empty = 'x\n<!-- generated:a -->\n<!-- /generated:a -->\ny';
        const [block] = scanBlocks(empty);
        assert.deepEqual(bodyOf(empty, block), []);
        assert.equal(replaceBlock(empty, block, ['body']), 'x\n<!-- generated:a -->\nbody\n<!-- /generated:a -->\ny');
    });

    // ---- F.2: generating and checking blocks with a fake context ---------------------------
    const catalog = ['{"kind":"b","code":"E1012"}', '{"kind":"a","code":"E1012"}', '{"kind":"x","code":"E0001"}'];
    const std = {
        Sample: [
            { name: 'a', doc: '@complexity O(1)' },
            { name: 'b', doc: 'Text.\n\n@complexity amortized O(1), worst O(n)\n@since 0.2.0\n@see Sample.a' },
        ],
    };
    const fake = (overrides = {}) => ({ root: scratch, hasExplain: () => true, catalog: () => catalog, std: () => std, ...overrides });
    const placements = [
        { name: 'diagnostic-kinds', file: 'docs/diagnostics.md', requires: 'explain' },
        { name: 'complexity:Sample', file: 'docs/sample.md' },
    ];
    const write = (file, text) => {
        mkdirSync(dirname(join(scratch, file)), { recursive: true });
        writeFileSync(join(scratch, file), text);
    };
    write('docs/diagnostics.md', '# D\n<!-- generated:diagnostic-kinds -->\n<!-- /generated:diagnostic-kinds -->\n');
    write('docs/sample.md', '# S\n<!-- generated:complexity:Sample -->\n<!-- /generated:complexity:Sample -->\n');
    test('diagnosticKindsTable sorts codes and kinds', () => {
        assert.deepEqual(diagnosticKindsTable(catalog), ['| コード | kind |', '| --- | --- |', '| `E0001` | `x` |', '| `E1012` | `a`, `b` |']);
    });
    test('checkBlocks reports empty bodies as stale', () => {
        const { checked, stale } = checkBlocks(fake(), { placements });
        assert.equal(checked, 2);
        assert.equal(stale.length, 2);
        assert.equal(stale[0], 'stale docs/diagnostics.md (diagnostic-kinds)\n  line 1 of the block\n    expected: | コード | kind |\n    found:    (none)');
    });
    test('writeBlocks writes once, then nothing', () => {
        assert.deepEqual(writeBlocks(fake(), { placements }), { checked: 2, updated: 2 });
        const written = statSync(join(scratch, 'docs/sample.md')).mtimeMs;
        assert.deepEqual(writeBlocks(fake(), { placements }), { checked: 2, updated: 0 });
        assert.equal(statSync(join(scratch, 'docs/sample.md')).mtimeMs, written);
        assert.deepEqual(checkBlocks(fake(), { placements }).stale, []);
        assert.equal(
            readFileSync(join(scratch, 'docs/sample.md'), 'utf8'),
            '# S\n<!-- generated:complexity:Sample -->\n| 関数 | 計算量 | 版 | 関連 |\n| --- | --- | --- | --- |\n| `Sample.a` | $O(1)$ | - | - |\n| `Sample.b` | 償却 $O(1)$、最悪 $O(n)$ | 0.2.0 | `Sample.a` |\n<!-- /generated:complexity:Sample -->\n',
        );
    });
    test('a block that needs explain is skipped without it', () => {
        assert.deepEqual(checkBlocks(fake({ hasExplain: () => false }), { placements }), { checked: 1, stale: [], skipped: ['diagnostic-kinds'] });
    });
    test('an unknown block is an error', () => {
        write('docs/sample.md', '# S\n<!-- generated:complexity:Sample -->\n<!-- /generated:complexity:Sample -->\n<!-- generated:typo -->\n<!-- /generated:typo -->\n');
        assert.throws(() => checkBlocks(fake(), { placements }), /docs\/sample\.md: unknown block 'typo'/);
    });
    test('a missing block is an error', () => {
        write('docs/sample.md', '# S\n');
        assert.throws(() => checkBlocks(fake(), { placements }), /docs\/sample\.md: missing block 'complexity:Sample'$/);
        write('docs/diagnostics.md', '# D\n');
        assert.throws(() => checkBlocks(fake(), { placements, only: ['diagnostic-kinds'] }), /docs\/diagnostics\.md: missing block 'diagnostic-kinds' \(G23 step 4\.2 adds the markers\)/);
    });
    test('a document with CR is an error', () => {
        write('docs/sample.md', '# S\r\n<!-- generated:complexity:Sample -->\r\n<!-- /generated:complexity:Sample -->\r\n');
        assert.throws(() => checkBlocks(fake(), { placements, only: ['complexity'] }), /CRLF is not supported/);
    });
    test('--list prints the placements', () => {
        const result = spawnSync(process.execPath, [join(root, 'scripts/gen-docs.mjs'), '--list'], { encoding: 'utf8' });
        assert.equal(result.status, 0, result.stderr);
        assert.equal(result.stdout, PLACEMENTS.map(({ name, file }) => `${name}\t${file}\n`).join(''));
        const unknown = spawnSync(process.execPath, [join(root, 'scripts/gen-docs.mjs'), '--x'], { encoding: 'utf8' });
        assert.equal(unknown.status, 2);
        assert.match(unknown.stderr, /^gen-docs: unknown option '--x'\nusage: node scripts\/gen-docs\.mjs/);
        const only = spawnSync(process.execPath, [join(root, 'scripts/gen-docs.mjs'), '--check', '--only', 'nothing'], { encoding: 'utf8' });
        assert.equal(only.status, 2);
        assert.match(only.stderr, /gen-docs: no block named 'nothing'/);
    });

    // ---- F.3 (rule), F.4 and F.5: tags and the complexity table ------------------------------
    test('tagNotes follows the rule of @deprecated', () => {
        const cases = [
            ['Old.\n@deprecated use new', ['use new']],
            ['@deprecated', ['']],
            ['  @deprecated  spaced  ', ['spaced']],
            ['@deprecated\ttabbed', ['tabbed']],
            ['@deprecated a\n@deprecated b', ['a', 'b']],
            ['@deprecatedX', []],
            ['@deprecatedX\n@deprecated real', ['real']],
            ['text', []],
            ['see @deprecated here', []],
            ['', []],
            ['@deprecated x\r\n', ['x']],
        ];
        for (const [text, notes] of cases) assert.deepEqual(tagNotes(text, '@deprecated'), notes, JSON.stringify(text));
    });
    test('parseComplexity accepts the closed grammar only', () => {
        const valid = ['O(1)', 'O(log n)', 'O(n)', 'O(n log n)', 'O(n log^2 n)', 'O(n + m)', 'O(n * m)', 'O(k)', 'O(n * m + k)', 'O(n + k)', 'amortized O(1)', 'amortized O(1), worst O(n)', 'expected O(1), worst O(n)'];
        for (const text of valid) assert.notEqual(parseComplexity(text), null, text);
        const invalid = ['', 'O()', 'O(2)', 'O(n^2)', 'O(n  m)', 'O(n*m)', 'o(n)', 'O(n) ', 'worst O(n)', 'amortized O(1), worst O(n), worst O(n)', 'expected O(1), O(n)', 'O(log)', 'O(n log)', 'O(x)', 'fast', 'O(n), worst O(n)'];
        for (const text of invalid) assert.equal(parseComplexity(text), null, JSON.stringify(text));
    });
    test('renderComplexity writes math', () => {
        const cases = [
            ['O(n log^2 n)', '$O(n \\log^2 n)$'],
            ['O(n * m + k)', '$O(n \\cdot m + k)$'],
            ['amortized O(1), worst O(n)', '償却 $O(1)$、最悪 $O(n)$'],
            ['expected O(1), worst O(n)', '平均 $O(1)$、最悪 $O(n)$'],
            ['O(log n)', '$O(\\log n)$'],
        ];
        for (const [text, rendered] of cases) assert.equal(renderComplexity(parseComplexity(text)), rendered, text);
    });
    test('parseStdPage reads the def sections', () => {
        const page = readFileSync(join(root, 'tests/fixtures/gen-docs/std-doc/Sample.md'), 'utf8');
        assert.deepEqual(parseStdPage(page), std.Sample);
        // `pure`, `export`, `const` and effect attributes come before `def` in the signature.
        const prefixed = '# P\n\n## `p`\n\n```tsuzuri\npure def p :: i64\n```\n\nP.\n\n## `q`\n\n```tsuzuri\n@total export def q :: i64\n```\n\nQ.\n';
        assert.deepEqual(parseStdPage(prefixed), [{ name: 'p', doc: 'P.' }, { name: 'q', doc: 'Q.' }]);
    });
    test('complexityTable adds columns only when used', () => {
        assert.deepEqual(complexityTable('Sample', std.Sample.slice(0, 1)), ['| 関数 | 計算量 |', '| --- | --- |', '| `Sample.a` | $O(1)$ |']);
        assert.deepEqual(complexityTable('Sample', std.Sample), [
            '| 関数 | 計算量 | 版 | 関連 |',
            '| --- | --- | --- | --- |',
            '| `Sample.a` | $O(1)$ | - | - |',
            '| `Sample.b` | 償却 $O(1)$、最悪 $O(n)$ | 0.2.0 | `Sample.a` |',
        ]);
    });
    test('complexityTable names the function with a bad tag', () => {
        const table = doc => () => complexityTable('Sample', [{ name: 'c', doc }]);
        assert.throws(table('No tag.'), /^Error: Sample\.c: missing @complexity$/);
        assert.throws(table('@complexity fast'), /Sample\.c: invalid @complexity "fast"/);
        assert.throws(table('@complexity O(1)\n@since 1.0'), /invalid @since "1\.0"/);
        assert.throws(table('@complexity O(1)\n@see a b'), /invalid @see "a b"/);
    });
} finally {
    rmSync(scratch, { recursive: true, force: true });
}
