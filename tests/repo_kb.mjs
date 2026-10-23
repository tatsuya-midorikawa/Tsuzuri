import assert from 'node:assert/strict';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseMarkdown, splitRow } from '../scripts/repo-kb/markdown.mjs';
import { cp, slug } from '../scripts/repo-kb/text.mjs';
import { listFiles } from '../scripts/repo-kb/repo.mjs';
import { scanRust, diagSites, publicItems } from '../scripts/repo-kb/rust.mjs';
import { classify, parseDeps, parseFilesCell, parseTicket, roleHeading } from '../scripts/repo-kb/tickets.mjs';
import { mentions, parseDecisions } from '../scripts/repo-kb/decisions.mjs';
import { parseModuleTables } from '../scripts/repo-kb/modules.mjs';
import { indexTests } from '../scripts/repo-kb/tests-index.mjs';
import { indexDocs } from '../scripts/repo-kb/docs-index.mjs';
import { build, collect, isFresh } from '../scripts/repo-kb/build.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const mini = resolve(root, 'tests/fixtures/repo_kb/mini');
const tests = [];
function test(name, run) { tests.push({ name, run }); }

test('markdown_headings_ignore_fenced_lines', () => {
    const md = parseMarkdown('# A\n```\n## not\n```\n~~~\n### also\n~~~\n## B');
    assert.deepEqual(md.headings.map(h => [h.title, h.level, h.line, h.end]), [['A', 1, 1, 8], ['B', 2, 8, 8]]);
    assert.equal(cp('a😀'), 2);
});
test('markdown_table_splits_on_unescaped_pipes_outside_code', () => {
    assert.deepEqual(splitRow('| a \\| b | `x|y` | c |'), ['a | b', '`x|y`', 'c']);
});
test('slug_matches_check_docs', () => {
    const seen = new Map();
    assert.deepEqual(['ムーブ後の使用エラー（E1012）', 'Phase 1: 差分', 'X', 'X'].map(s => slug(s, seen)),
        ['ムーブ後の使用エラーe1012', 'phase-1-差分', 'x', 'x-1']);
    for (const file of ['index.md', 'ownership/ownership.md', 'compiler/diagnostics.md']) {
        const path = resolve(root, '_tsuzuri/language-reference', file);
        let source;
        try { source = readFileSync(path, 'utf8'); } catch (e) { if (e.code === 'ENOENT') continue; throw e; }
        const duplicates = new Map();
        const expected = [];
        for (const line of source.replace(/^```[^\n]*\n[\s\S]*?^```\s*$/gm, '').split('\n')) {
            const m = /^#{1,6} (.+)$/.exec(line);
            if (!m) continue;
            const base = m[1].toLowerCase().replace(/[^\p{L}\p{M}\p{N}\s_-]/gu, '').replace(/ /g, '-');
            const n = duplicates.get(base) ?? 0;
            expected.push(n ? `${base}-${n}` : base);
            duplicates.set(base, n + 1);
        }
        const actualSeen = new Map();
        assert.deepEqual(parseMarkdown(source).headings.map(h => slug(h.title, actualSeen)), expected);
    }
});
test('list_files_falls_back_to_walk', () => {
    const files = listFiles(mini);
    assert.ok(files.includes('README.md'));
    assert.ok(!files.some(p => p.startsWith('target/') || p.startsWith('.git/')));
    assert.deepEqual(files, [...files].sort());
});

const rust = () => readFileSync(resolve(mini, 'src/check.rs'), 'utf8');
test('rust_scan_masks_comments_strings_and_chars', () => {
    const { code } = scanRust(rust());
    assert.equal(code.length, rust().length);
    assert.ok(!/E1001|E1002|fn fake/.test(code));
});
test('rust_scan_handles_raw_strings_and_lifetimes', () => {
    const { code, literals } = scanRust('let a = r##"x"#y"##; let b: &\'a str = "";');
    assert.deepEqual(literals.map(l => l.value), ['x"#y', '']);
    assert.ok(code.includes("'a"));
});
test('rust_diag_sites_skip_cfg_test_modules', () => {
    const sites = diagSites(rust());
    assert.equal(sites.find(s => s.code === 'E1999').test, true);
    assert.equal(sites.filter(s => !s.test).length, 6);
});
test('rust_diag_sites_name_impl_and_fn', () => {
    assert.deepEqual(diagSites(rust()).filter(s => !s.test).map(s => [s.code, s.line, s.function]),
        [['E1004', 4, null], ['E1005', 15, 'Checker::value_expression'], ['E1012', 16, 'Checker::value_expression'],
            ['W1006', 26, 'Rule::check'], ['W1006', 31, 'warn_copy'], ['E1013', 45, 'after_tests']]);
    assert.equal(diagSites('fn error(x: impl Into<String>) { diag("E0002"); }')[0].function, 'error');
});
test('rust_public_items_list_signature_lines', () => {
    assert.deepEqual(publicItems(rust()).map(s => s.line), [8, 13, 24, 30, 44]);
});

const context = () => {
    const files = listFiles(mini);
    return { root: mini, files, fileSet: new Set(files), ticketIds: new Set(['A01', 'B02', 'C03', 'PB04', 'PX05']),
        decisionIds: new Set(['D-01', 'D-02', 'D-03']), warnings: [] };
};
const ticket = path => parseTicket(context(), `_features/${path}`, path.includes('_completed/'));
test('ticket_classification_by_file_name', () => {
    assert.deepEqual(['README.md', 'GUIDE.md', 'GUIDE-LITE.md', 'A01-alpha.md', 'A01-appendix.md', 'A01-appendix-more.md', 'notes.md'].map(classify),
        [null, null, null, { kind: 'ticket', id: 'A01' }, { kind: 'appendix', id: 'A01' }, { kind: 'appendix', id: 'A01' }, { kind: 'skip' }]);
});
test('ticket_meta_table_both_separator_forms', () => {
    assert.equal(ticket('A01-alpha.md').priority, 'P1');
    assert.equal(ticket('_completed/B02-beta.md').priority, 'P2');
    assert.match(readFileSync(resolve(root, '_features/G22-diagnostic-v2.md'), 'utf8'), /\| 優先度 \| P0（/);
});
test('ticket_meta_table_without_blank_line_after_title', () => {
    assert.equal(ticket('_completed/B02-beta.md').files[0].path, 'src/check.rs');
    assert.equal(ticket('_completed/C03-gamma.md').id, 'C03');
});
test('ticket_size_and_state_with_notes', () => {
    assert.deepEqual([ticket('A01-alpha.md').size, ticket('_completed/B02-beta.md').size, ticket('_completed/C03-gamma.md').size], ['M', 'XL', 'L']);
    assert.equal(ticket('_completed/C03-gamma.md').state_cell, 'done');
    assert.ok(ticket('A01-alpha.md').size_note.startsWith('全体 M'));
});
test('ticket_state_conflict_is_reported_and_location_wins', () => {
    const ctx = context();
    const t = parseTicket(ctx, '_features/_completed/B02-beta.md', true);
    assert.equal(t.state, 'done');
    assert.equal(t.state_cell, 'todo');
    assert.match(ctx.warnings[0], /^state_conflict B02:/);
});
test('deps_table_forms', () => {
    const known = new Set(['A01', 'A03', 'E01', 'E02', 'B01', 'G02', 'A15', 'G19', 'A17', 'G22', 'G23', 'F02', 'E05']);
    for (const [s, expected] of [
        ['–', []], ['なし', []], ['E02, B01', [['E02', false], ['B01', false]]],
        ['(A01)', [['A01', true]]],
        ['G02, 弱依存 A03, 弱依存 E01（`W1002` の非公開判定）', [['G02', false], ['A03', true], ['E01', true]]],
        ['F02, E05, B01（`Gpu.request` が `Result` を返すため）', [['F02', false], ['E05', false], ['B01', false]]],
        ['A15（done）、G19（done。edition）、(A17)。A19 は別件', [['A15', false], ['G19', false], ['A17', true]]],
        ['G22 の Phase 1（…）。… 弱い依存: [G23](G23.md)（…）', [['G22', false], ['G23', true]]],
    ]) assert.deepEqual(parseDeps(s, known).map(d => [d.id, d.weak]), expected, s);
});
test('files_cell_forms', () => {
    const ctx = context();
    assert.deepEqual(parseFilesCell('`src/check.rs` `src/new.rs` `src/*.rs` `tests/fixtures/` `Checker::name`', ctx),
        [{ path: 'src/check.rs', exists: true }, { path: 'src/new.rs', exists: false }, { path: 'src/control.rs', exists: true },
            { path: 'src/lib.rs', exists: true }, { path: 'src/recursion.rs', exists: true }, { path: 'tests/fixtures/', exists: true }]);
    assert.deepEqual(parseFilesCell('`src/{check,lib}.rs`', ctx).map(f => f.path), ['src/check.rs', 'src/lib.rs']);
});
test('ticket_sections_roles_and_aliases', () => {
    assert.equal(roleHeading(parseMarkdown('## 確定した仕様\nbody'), 'spec').title, '確定した仕様');
    assert.equal(roleHeading(parseMarkdown('## 確定した仕様\nbody'), 'quick'), undefined);
});
test('ticket_headings_index_has_levels_lines_chars', () => {
    const t = ticket('A01-alpha.md');
    assert.deepEqual(t.headings[0], { level: 2, title: '実装者向け早見', line: 13, end: 14, chars: 24 });
    assert.ok(t.headings.every(h => h.level === 2 || h.level === 3));
});
test('mentions_filter_unknown_ids', () => {
    assert.deepEqual(mentions('A01 A64 D-01〜D-03 E1012 `src/check.rs` `src/missing.rs`', context()),
        { tickets: ['A01'], decisions: ['D-01', 'D-02', 'D-03'], diags: ['E1012'], files: ['src/check.rs'] });
});

test('decisions_parse_and_ranges', () => {
    const ds = parseDecisions(readFileSync(resolve(mini, '_features/GUIDE.md'), 'utf8'));
    assert.deepEqual(ds.map(d => d.id), ['D-01', 'D-02', 'D-03']);
    assert.ok(ds[1].text.includes('| rule | one |'));
});
test('decisions_skip_fenced_headings', () => {
    assert.ok(!parseDecisions(readFileSync(resolve(mini, '_features/GUIDE.md'), 'utf8')).some(d => d.id === 'D-99'));
});
test('module_tables_resolve_relative_names_and_globs', () => {
    const rows = parseModuleTables('docs/architecture.md', readFileSync(resolve(mini, 'docs/architecture.md'), 'utf8'), context());
    assert.deepEqual(rows[1].files.map(f => f.path), ['src/control.rs', 'src/recursion.rs']);
    assert.deepEqual(rows[2].files.map(f => f.path), ['src/runtime/string.ll', 'src/runtime/heap-native.ll', 'src/runtime/heap-wasm.ll', 'src/runtime/utf8string.ll']);
    assert.deepEqual(rows[3].dirs, ['std/']);
    assert.deepEqual(rows[3].missing, ['src/missing.rs']);
});
test('tests_index_rust_mjs_and_suites', () => {
    const rows = indexTests(context());
    assert.equal(rows.find(t => t.name === 'alpha').count, 2);
    assert.ok(rows.find(t => t.name === 'run').compiler_arg);
    assert.equal(rows.find(t => t.name === 'helper').command, '');
    assert.equal(rows.find(t => t.name === 'storage').fixture, 'tests/fixtures/storage/Storage.tz');
});
test('docs_index_headings_and_anchors', () => {
    const row = indexDocs(context()).find(d => d.path.endsWith('ownership/rules.md'));
    assert.deepEqual(row.headings.map(h => h.anchor), ['x', 'x-1']);
});
test('diagnostics_index_merges_sites_docs_and_mentions', () => {
    const row = collect(mini).diagnostics.find(d => d.code === 'E1012');
    assert.equal(row.site_count, 1);
    assert.equal(row.meaning, 'ムーブ後の使用');
    assert.ok(row.tickets.includes('A01'));
    assert.ok(row.decisions.includes('D-02'));
});

function scratch(run) {
    const base = resolve(root, 'target/scratch');
    mkdirSync(base, { recursive: true });
    const dir = mkdtempSync(resolve(base, 'repo-kb-test-'));
    try { return run(dir); } finally { rmSync(dir, { recursive: true, force: true }); }
}
const cli = (...args) => spawnSync(process.execPath, [resolve(root, 'scripts/repo-kb.mjs'), ...args], { encoding: 'utf8' });
test('build_writes_files_in_fixed_key_order', () => scratch(dir => {
    build(mini, dir);
    assert.equal(readdirSync(dir).length, 8);
    const row = JSON.parse(readFileSync(resolve(dir, 'tickets.jsonl'), 'utf8').split('\n')[0]);
    assert.deepEqual(Object.keys(row), ['id', 'title', 'path', 'completed', 'priority', 'size', 'size_note', 'state', 'state_cell', 'state_raw',
        'deps', 'deps_raw', 'followers_raw', 'files', 'refs', 'appendices', 'chars', 'headings']);
}));
test('build_is_byte_identical_twice', () => scratch(dir => {
    build(mini, dir);
    const files = readdirSync(dir);
    const before = files.map(f => readFileSync(resolve(dir, f), 'utf8'));
    build(mini, dir);
    assert.deepEqual(files.map(f => readFileSync(resolve(dir, f), 'utf8')), before);
}));
test('build_refuses_foreign_directory', () => scratch(dir => {
    writeFileSync(resolve(dir, 'x.txt'), 'precious');
    assert.throws(() => build(mini, dir), /not a repo-kb directory/);
    assert.equal(cli('build', '--root', mini, '--out', dir).status, 3);
    assert.equal(readFileSync(resolve(dir, 'x.txt'), 'utf8'), 'precious');
}));
test('verify_detects_stale_input', () => scratch(dir => {
    cpSync(mini, resolve(dir, 'repo'), { recursive: true });
    const input = resolve(dir, 'repo');
    const out = resolve(dir, 'out');
    build(input, out);
    assert.ok(isFresh(input, out));
    writeFileSync(resolve(input, 'README.md'), '# Changed\n');
    assert.equal(isFresh(input, out), false);
    const result = cli('verify', '--root', input, '--out', out);
    assert.equal(result.status, 1);
    assert.match(result.stdout, /stale/);
}));
let realKb;
function real() { return realKb ??= collect(root); }
test('build_real_repo_invariants', () => {
    const kb = real();
    assert.ok(kb.tickets.length >= 125);
    assert.ok(kb.decisions.length >= 58);
    assert.equal(new Set(kb.tickets.map(t => t.id)).size, kb.tickets.length);
    assert.ok(kb.manifest.warnings.every(w => w.startsWith('state_conflict ')), kb.manifest.warnings.join('\n'));
    for (const t of kb.tickets) for (const d of t.deps) assert.ok(kb.tickets.some(other => other.id === d.id));
    const readme = readFileSync(resolve(root, '_features/README.md'), 'utf8');
    for (const m of readme.matchAll(/^\s*([A-Z]{1,2}\d{2})\s*(-->|-\.->)\s*([A-Z]{1,2}\d{2})/gm)) {
        assert.ok(kb.tickets.find(t => t.id === m[3])?.deps.some(d => d.id === m[1] && d.weak === (m[2] === '-.->')), m[0]);
    }
    assert.ok(kb.tests.filter(t => t.kind === 'suite').every(t => t.fixture_exists));
    assert.ok(kb.modules.every(m => !m.missing.length));
});

let passed = 0;
for (const { name, run } of tests) {
    if (process.argv[2] && !name.includes(process.argv[2])) continue;
    await run();
    console.log(`ok ${name}`);
    passed++;
}
assert.ok(passed, 'no tests selected');
console.log(`${passed} tests passed`);
