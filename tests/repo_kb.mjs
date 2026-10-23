import assert from 'node:assert/strict';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { spawn, spawnSync } from 'node:child_process';
import { setTimeout as delay } from 'node:timers/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parseMarkdown, splitRow } from '../scripts/repo-kb/markdown.mjs';
import { cp, inputText, MAX_INPUT_BYTES, slug } from '../scripts/repo-kb/text.mjs';
import { inputFiles, listFiles } from '../scripts/repo-kb/repo.mjs';
import { scanRust, diagSites, publicItems } from '../scripts/repo-kb/rust.mjs';
import { classify, filterPhase, parseDeps, parseFilesCell, parseTicket, roleHeading } from '../scripts/repo-kb/tickets.mjs';
import { mentions, parseDecisions } from '../scripts/repo-kb/decisions.mjs';
import { parseModuleTables } from '../scripts/repo-kb/modules.mjs';
import { indexTests } from '../scripts/repo-kb/tests-index.mjs';
import { indexDocs } from '../scripts/repo-kb/docs-index.mjs';
import { build, collect, isFresh, loadKb } from '../scripts/repo-kb/build.mjs';
import { ticketText, decisionText, filesText, diagText, searchText, sectionText } from '../scripts/repo-kb/query.mjs';
import { brief, cut, STAGES } from '../scripts/repo-kb/brief.mjs';
import { checkGuideLite } from '../scripts/check-guide-lite.mjs';
import { checkNodeVersion } from '../scripts/repo-kb.mjs';

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
    for (const file of ['index.md', 'compiler/option.md', 'compiler/diagnostics.md']) {
        const path = resolve(root, '_tsuzuri/language-reference', file);
        for (const newline of ['\n', '\r\n']) {
            const source = readFileSync(path, 'utf8').replace(/\r\n/g, '\n').replace(/\n/g, newline);
            const duplicates = new Map();
            const expected = [];
            for (const line of source.replace(/\r\n/g, '\n').replace(/^```[^\n]*\n[\s\S]*?^```\s*$/gm, '').split('\n')) {
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
test('public_symbol_names_skip_declaration_modifiers', () => {
    const text = 'pub const fn unix() {}\npub static mut SHARED: i32 = 0;\npub unsafe extern "C" fn external() {}\npub async unsafe fn promised() {}\npub union Payload { value: i32 }\n';
    assert.deepEqual(publicItems(text).map(s => s.name), ['unix', 'SHARED', 'external', 'promised', 'Payload']);
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

async function scratch(run) {
    const base = resolve(root, 'target/scratch');
    mkdirSync(base, { recursive: true });
    const dir = mkdtempSync(resolve(base, 'repo-kb-test-'));
    try { return await run(dir); } finally { rmSync(dir, { recursive: true, force: true }); }
}
const cli = (...args) => spawnSync(process.execPath, [resolve(root, 'scripts/repo-kb.mjs'), ...args], { encoding: 'utf8' });
test('build_writes_files_in_fixed_key_order', () => scratch(async dir => {
    await build(mini, dir);
    assert.equal(readdirSync(dir).length, 8);
    const row = JSON.parse(readFileSync(resolve(dir, 'tickets.jsonl'), 'utf8').split('\n')[0]);
    assert.deepEqual(Object.keys(row), ['id', 'title', 'path', 'completed', 'priority', 'size', 'size_note', 'state', 'state_cell', 'state_raw',
        'deps', 'deps_raw', 'followers_raw', 'files', 'refs', 'appendices', 'chars', 'headings']);
}));
test('build_is_byte_identical_twice', () => scratch(async dir => {
    await build(mini, dir);
    const files = readdirSync(dir);
    const before = files.map(f => readFileSync(resolve(dir, f), 'utf8'));
    await build(mini, dir);
    assert.deepEqual(files.map(f => readFileSync(resolve(dir, f), 'utf8')), before);
}));
test('build_refuses_foreign_directory', () => scratch(async dir => {
    writeFileSync(resolve(dir, 'x.txt'), 'precious');
    await assert.rejects(build(mini, dir), /not a repo-kb directory/);
    assert.equal(cli('build', '--root', mini, '--out', dir).status, 3);
    assert.equal(readFileSync(resolve(dir, 'x.txt'), 'utf8'), 'precious');
}));
test('verify_detects_stale_input', () => scratch(async dir => {
    cpSync(mini, resolve(dir, 'repo'), { recursive: true });
    const input = resolve(dir, 'repo');
    const out = resolve(dir, 'out');
    await build(input, out);
    assert.ok(await isFresh(input, out));
    writeFileSync(resolve(input, 'README.md'), '# Changed\n');
    assert.equal(await isFresh(input, out), false);
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
    const suites = kb.tests.filter(t => t.kind === 'suite').map(t => t.name);
    assert.deepEqual(suites, [...suites].sort());
    assert.ok(kb.modules.every(m => !m.missing.length));
});

const fixtureKb = () => collect(mini);
test('ticket_lookup_text_format', () => {
    const text = ticketText(fixtureKb(), 'A01');
    assert.match(text, /^A01 A01: Alpha feature\npath: _features\/A01-alpha.md\nstate: todo/);
    assert.match(text, /depends: B02 done; C03 done \(weak\)/);
    assert.match(text, /headings:\n  2 実装者向け早見 L13-L14 \(24 chars\)/);
});
test('decision_lookup_by_number_forms', () => {
    const kb = fixtureKb();
    for (const id of ['D-02', 'd-02', '2', '02']) assert.equal(decisionText(kb, id), decisionText(kb, 'D-02'));
    assert.equal(decisionText(kb, 'D-99'), null);
});
test('files_lookup_matches_cell_and_mentions', () => {
    const kb = fixtureKb();
    assert.match(filesText(kb, 'src/check.rs'), /A01 \(_features\/A01-alpha.md\) cell/);
    assert.match(filesText(kb, 'src/'), /D-02 mention/);
});
test('diag_lookup_lists_sites_and_mentions', () => {
    assert.match(diagText(fixtureKb(), 'E1012'), /src\/check.rs:Checker::value_expression L16/);
    assert.equal(diagText(fixtureKb(), 'E9999'), null);
});
test('search_matches_all_words_case_insensitive', () => {
    assert.match(searchText(fixtureKb(), ['ALPHA', 'feature']), /^ticket\tA01\t/);
    assert.equal(searchText(fixtureKb(), ['zzz']), '(no matches)');
});
test('section_command_prints_exact_section', () => {
    const kb = fixtureKb();
    const ctx = context();
    assert.match(sectionText(ctx, kb, 'A01', 'spec'), /^=== SECTION A01 仕様 /);
    assert.match(sectionText(ctx, kb, 'A01', '設計'), /設計の本文。$/);
    assert.equal(sectionText(ctx, kb, 'A01', 'nope'), null);
});
test('phase_filter_sticky_in_steps', () => {
    const text = sectionText(context(), fixtureKb(), 'A01', 'steps', 1);
    assert.match(text, /前文。/);
    assert.match(text, /Phase 1 に属す。/);
    assert.ok(!text.includes('**手順 2: 仕上げ。**'));
    assert.match(text, /\[Phase 2 omitted:/);
});
test('phase_filter_non_sticky_in_spec', () => {
    const text = sectionText(context(), fixtureKb(), 'A01', 'spec', 2);
    assert.ok(!text.includes('Phase 1 の仕様。'));
    assert.match(text, /Phase 2 の仕様。/);
    assert.match(text, /どの Phase にも要る。/);
    assert.equal(filterPhase('## Spec\nplain', 1), '## Spec\nplain');
});

const pack = (ctx, kb, id, opts = {}) => brief(ctx, kb, id, { phase: 1, maxChars: 24000, lite: true, with: [], ...opts });
test('brief_contains_required_parts_in_order', () => {
    const text = pack(context(), fixtureKb(), 'A01');
    let previous = -1;
    for (const title of ['REPO-KB BRIEF', 'HOW TO USE THIS PACK', 'CONTENTS', 'TICKET A01 / quick', 'TICKET A01 / start',
        'TICKET A01 / spec', 'TICKET A01 / design', 'TICKET A01 / steps', 'TICKET A01 / tests', 'TICKET A01 / accept',
        'INTERFACES', 'TESTS AND COMMANDS', 'GUIDE-LITE', 'END']) {
        const index = text.indexOf(`=== ${title}`);
        assert.ok(index > previous, title);
        previous = index;
    }
});
test('brief_respects_max_chars_for_every_fixture_ticket', () => {
    const kb = fixtureKb();
    for (const t of kb.tickets) for (const maxChars of [16000, 24000, 60000]) {
        assert.ok(cp(pack(context(), kb, t.id, { maxChars })) <= maxChars);
    }
});
test('brief_truncation_stages_follow_the_table', () => {
    const ctx = context();
    ctx.texts = new Map([['_features/A01-alpha.md', '# A01: Alpha\n## 仕様\n' + 'long\n'.repeat(4000)]]);
    const kb = fixtureKb();
    assert.match(pack(ctx, kb, 'A01', { maxChars: 14000 }), /\[shortened:/);
    assert.ok(!pack(ctx, kb, 'A01', { maxChars: 60000 }).includes('[shortened:'));
    assert.equal(STAGES[2].spec, 9000);
    assert.equal(STAGES[5].quick, 2000);
    assert.equal(cut('a😀\nb', 2, 'section A01 spec').text.startsWith('a😀'), true);
});
test('brief_no_lite_option', () => {
    const text = pack(context(), fixtureKb(), 'A01', { lite: false });
    assert.ok(!text.includes('=== GUIDE-LITE'));
    assert.ok(cp(text) < cp(pack(context(), fixtureKb(), 'A01')));
});
test('brief_is_deterministic', () => {
    const kb = fixtureKb();
    assert.equal(pack(context(), kb, 'A01'), pack(context(), kb, 'A01'));
});
test('brief_rejects_too_small_max_chars', () => {
    for (const n of ['15999', 'x']) assert.equal(cli('brief', 'A01', '--max-chars', n).status, 2);
});
test('brief_real_repo_all_tickets_default_and_16000', () => {
    const kb = real();
    const ctx = { root };
    for (const t of kb.tickets) for (const maxChars of [24000, 16000]) {
        const first = pack(ctx, kb, t.id, { maxChars });
        assert.ok(cp(first) <= maxChars, `${t.id} ${maxChars}`);
        assert.equal(first, pack(ctx, kb, t.id, { maxChars }));
    }
});

test('guide_lite_check_accepts_fixture', () => {
    const result = checkGuideLite(context(), fixtureKb(), '_features/GUIDE-LITE.md');
    assert.deepEqual(result.failures, []);
    assert.ok(result.checked >= 6);
});
test('guide_lite_check_reports_each_class', () => scratch(dir => {
    cpSync(mini, dir, { recursive: true });
    const content = [
        '`src/missing.rs` `missing.mjs` `missing/` `node scripts/missing.mjs` `E9999` D-99 §3.1',
        '[bad](missing.md) [bad anchor](GUIDE.md#missing)', '`Options::validate` `Checker::missing` `MISSING_CONST` `missing_test_name` X99',
    ].join('\n');
    writeFileSync(resolve(dir, '_features/GUIDE-LITE.md'), content);
    const ctx = { ...context(), root: dir };
    const result = checkGuideLite(ctx, fixtureKb(), '_features/GUIDE-LITE.md');
    for (const token of ['src/missing.rs', 'missing.mjs', 'missing/', 'scripts/missing.mjs', 'E9999', 'D-99', '3.1', 'missing.md', 'GUIDE.md#missing',
        'Options::validate', 'Checker::missing', 'MISSING_CONST', 'missing_test_name', 'X99']) {
        assert.ok(result.failures.some(f => f.includes(token)), `${token}: ${result.failures.join('\n')}`);
    }
    assert.ok(result.failures.every(f => /^FAIL .+ \(line [123]\)$/.test(f)), result.failures.join('\n'));
}));
test('guide_lite_planned_entry_fails_when_it_exists', () => {
    const ctx = context();
    ctx.files.push('guide/README.md');
    assert.ok(checkGuideLite(ctx, fixtureKb(), '_features/GUIDE-LITE.md').failures.some(f => f.includes('now exists; remove it from PLANNED')));
});

test('cli_exit_codes', async () => {
    for (const args of [[], ['bad'], ['build', '--bad'], ['ticket'], ['section', 'A01'], ['build', 'extra']]) {
        assert.equal(cli(...args).status, 2, args.join(' '));
    }
    await scratch(dir => {
        assert.equal(cli('brief', 'NOPE', '--root', mini, '--out', dir).status, 1);
        assert.equal(cli('build', '--root', resolve(dir, 'missing')).status, 3);
    });
});
test('outputs_have_no_absolute_paths_or_timestamps', () => scratch(async dir => {
    const kb = await build(mini, dir);
    const text = readdirSync(dir).map(f => readFileSync(resolve(dir, f), 'utf8')).join('\n') + pack(context(), kb, 'A01');
    assert.ok(!text.includes(root));
    assert.ok(!text.includes(dir));
    assert.ok(!/20[0-9]{2}-[0-9]{2}-[0-9]{2}T/.test(text));
}));
test('node_version_gate', () => {
    assert.throws(() => checkNodeVersion('18.20.0'), /Node.js 20 or later is required/);
    checkNodeVersion('20.19.6');
    checkNodeVersion('24.0.0');
});

for (const missing of [false, true]) test(missing ? 'concurrent_queries_initialize_a_missing_cache' : 'concurrent_stale_queries_use_independent_temporary_paths', () => scratch(async dir => {
    cpSync(mini, resolve(dir, 'repo'), { recursive: true });
    const repo = resolve(dir, 'repo');
    const out = resolve(dir, 'out');
    if (!missing) {
        await build(repo, out);
        writeFileSync(resolve(repo, 'README.md'), '# Changed concurrently\n');
    }
    const query = () => new Promise((done, reject) => {
        const child = spawn(process.execPath, [resolve(root, 'scripts/repo-kb.mjs'), 'ticket', 'A01', '--root', repo, '--out', out], { stdio: ['ignore', 'pipe', 'pipe'] });
        let errors = '';
        child.stderr.on('data', chunk => { errors += chunk; });
        child.on('error', reject);
        child.on('exit', code => code === 0 ? done() : reject(new Error(errors)));
    });
    await Promise.all(Array.from({ length: 8 }, query));
    assert.equal(readdirSync(out).length, 8);
    assert.ok(await isFresh(repo, out));
}));
test('overlapping_builders_publish_complete_source_generations', () => scratch(async dir => {
    cpSync(mini, dir, { recursive: true });
    const out = resolve(dir, 'target/cache');
    await build(dir, out);
    const barriers = resolve(dir, 'target/barriers');
    mkdirSync(barriers, { recursive: true });
    const flag = name => resolve(barriers, name);
    const workers = [];
    const worker = role => {
        // A pauses before tickets, B after tickets: without locking their generations are spliced.
        const code = `
import fs from 'node:fs';
import { basename, join } from 'node:path';
import { syncBuiltinESMExports } from 'node:module';
const flag = name => join(${JSON.stringify(barriers)}, name);
const wait = name => {
    const deadline = Date.now() + 120000;
    while (!fs.existsSync(flag(name))) {
        if (Date.now() > deadline) throw new Error('barrier timed out: ' + name);
        Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 10);
    }
};
const rename = fs.renameSync;
fs.renameSync = (from, to) => {
    if (basename(to) !== 'tickets.jsonl') return rename(from, to);
    if (${JSON.stringify(role)} === 'a') {
        fs.writeFileSync(flag('a-ready'), '');
        wait('release-a');
        return rename(from, to);
    }
    const result = rename(from, to);
    fs.writeFileSync(flag('b-ready'), '');
    wait('release-b');
    return result;
};
const mkdir = fs.mkdirSync;
fs.mkdirSync = (path, ...args) => {
    if (${JSON.stringify(role)} === 'b' && String(path).endsWith('.repo-kb-lock')) fs.writeFileSync(flag('b-lock'), '');
    return mkdir(path, ...args);
};
syncBuiltinESMExports();
const { build } = await import(${JSON.stringify(pathToFileURL(resolve(root, 'scripts/repo-kb/build.mjs')).href)});
await build(${JSON.stringify(dir)}, ${JSON.stringify(out)});
`;
        const child = spawn(process.execPath, ['--input-type=module', '-e', code], { stdio: ['ignore', 'ignore', 'pipe'] });
        let errors = '';
        child.stderr.on('data', chunk => { errors += chunk; });
        const done = new Promise((accept, reject) => {
            child.on('error', reject);
            child.on('exit', status => status === 0 ? accept() : reject(new Error(errors || `worker exited ${status}`)));
        });
        done.catch(() => {});
        const item = { child, done };
        workers.push(item);
        return item;
    };
    const wait = async (predicate, active) => {
        const deadline = Date.now() + 120000;
        while (!predicate()) {
            assert.ok(Date.now() < deadline, 'worker barrier timed out');
            assert.ok(active.every(w => w.child.exitCode === null), 'worker exited before its barrier');
            await delay(10);
        }
    };
    try {
        const a = worker('a');
        await wait(() => existsSync(flag('a-ready')), [a]);
        assert.equal(existsSync(resolve(out, 'manifest.json')), false, 'interrupted publication must not leave a valid old manifest');
        const ticket = resolve(dir, '_features/A01-alpha.md');
        writeFileSync(ticket, readFileSync(ticket, 'utf8').replace('Alpha', 'Changed Alpha'));
        const b = worker('b');
        await wait(() => existsSync(flag('b-lock')) || existsSync(flag('b-ready')), [a, b]);
        writeFileSync(flag('release-a'), '');
        await a.done;
        await wait(() => existsSync(flag('b-ready')), [b]);
        writeFileSync(flag('release-b'), '');
        await b.done;
        assert.ok(await isFresh(dir, out));
        const cached = await loadKb(out);
        assert.equal(cached.tickets.find(t => t.id === 'A01').title, collect(dir).tickets.find(t => t.id === 'A01').title);
    } finally {
        for (const { child } of workers) if (child.exitCode === null) child.kill();
        await Promise.allSettled(workers.map(w => w.done));
    }
}));
test('cache_readers_wait_for_generation_publication', () => scratch(async dir => {
    const out = resolve(dir, 'cache');
    await build(mini, out);
    const lock = `${out}.repo-kb-lock`;
    mkdirSync(lock);
    let completed = false;
    const read = Promise.resolve(loadKb(out)).then(kb => { completed = true; return kb; });
    try {
        await Promise.resolve();
        assert.equal(completed, false);
    } finally {
        rmSync(lock, { recursive: true, force: true });
    }
    assert.ok((await read).tickets.some(t => t.id === 'A01'));
}));
test('in_tree_output_is_not_its_own_input', () => scratch(async dir => {
    cpSync(mini, dir, { recursive: true });
    const out = resolve(dir, 'kb-output');
    await build(dir, out);
    assert.ok(await isFresh(dir, out));
    assert.equal(cli('ticket', 'A01', '--root', dir, '--out', out, '--no-refresh').status, 0);
    assert.equal(spawnSync('git', ['init', '-q'], { cwd: dir }).status, 0);
    assert.ok(await isFresh(dir, out));
    await build(dir, out);
    assert.ok(await isFresh(dir, out));
}));
test('indexed_filter_runner_command_is_runnable', () => {
    const row = real().tests.find(t => t.path === 'tests/repo_kb.mjs');
    assert.equal(row.compiler_arg, false);
    assert.equal(row.command, 'node tests/repo_kb.mjs');
});
test('compiler_paths_are_detected_in_find_and_slice_runners', () => {
    for (const path of ['tests/compact_strings.mjs', 'tests/compact_strings_raw.mjs', 'tests/small_allocator.mjs']) {
        const row = real().tests.find(t => t.path === path);
        assert.equal(row.compiler_arg, true, path);
        assert.equal(row.command, `node ${path} target/release/tsuzuri`);
    }
});
test('indexed_target_selector_runner_uses_valid_default', () => {
    const row = real().tests.find(t => t.path === 'tests/wasm_heap.mjs');
    assert.equal(row.compiler_arg, false);
    assert.equal(row.command, 'node tests/wasm_heap.mjs');
});
test('indexed_child_only_harness_is_not_a_standalone_command', () => {
    const row = real().tests.find(t => t.path === 'tests/os-wasi-host.mjs');
    assert.equal(row.helper, true);
    assert.equal(row.command, '');
});
test('manual_comparison_runner_does_not_advertise_missing_inputs', () => {
    const row = real().tests.find(t => t.path === 'tests/display_parse_exhaustive.mjs');
    assert.equal(row.helper, false);
    assert.equal(row.command, '');
});
test('oversized_suite_source_is_not_read_or_indexed', () => scratch(async dir => {
    cpSync(mini, dir, { recursive: true });
    const path = resolve(dir, 'tests/features.mjs');
    const source = readFileSync(path, 'utf8') + '\n//' + 'x'.repeat(8 * 1024 * 1024);
    writeFileSync(path, source);
    const out = resolve(dir, 'kb-output');
    const kb = await build(dir, out);
    assert.ok(kb.manifest.warnings.includes('file_too_large tests/features.mjs'));
    assert.ok(!kb.tests.some(t => t.path === 'tests/features.mjs'));
    writeFileSync(path, source.replace('storage', 'renamed'));
    assert.ok(await isFresh(dir, out));
}));
test('source_size_transitions_preserve_the_captured_content_hash', () => scratch(async dir => {
    cpSync(mini, dir, { recursive: true });
    const out = resolve(dir, 'target/cache');
    const readme = resolve(dir, 'README.md');
    const small = '# Captured README\n';
    const oversized = 'x'.repeat(MAX_INPUT_BYTES + 1);
    for (const [before, after] of [[small, oversized], [oversized, small]]) {
        writeFileSync(readme, before);
        const snapshot = await build(dir, out);
        const files = listFiles(dir);
        const texts = new Map(inputFiles(files).flatMap(path => {
            const text = inputText({ root: dir }, path);
            return text === null ? [] : [[path, text]];
        }));
        writeFileSync(readme, after);
        const indexed = collect(dir, { files, texts });
        writeFileSync(resolve(out, 'manifest.json'), JSON.stringify(indexed.manifest));
        assert.equal(await isFresh(dir, out), false, 'a later size transition must make the captured snapshot stale');
        assert.equal(indexed.manifest.input_sha256, snapshot.manifest.input_sha256);
        assert.deepEqual(indexed.docs, snapshot.docs);
    }
}));
test('required_guide_size_failure_is_not_silently_hidden', () => {
    const ctx = context();
    ctx.texts = new Map([['_features/GUIDE-LITE.md', 'x'.repeat(MAX_INPUT_BYTES + 1)]]);
    assert.throws(() => pack(ctx, fixtureKb(), 'A01'), /GUIDE-LITE.*8 MiB/);
    assert.throws(() => checkGuideLite(ctx, fixtureKb()), /GUIDE input.*8 MiB/);
    assert.ok(pack(ctx, fixtureKb(), 'A01', { lite: false }));
});
test('ticket_entry_points_skip_sources_that_exceed_admission', () => scratch(dir => {
    cpSync(mini, dir, { recursive: true });
    const kb = collect(dir);
    writeFileSync(resolve(dir, '_features/A01-alpha.md'), 'x'.repeat(MAX_INPUT_BYTES + 1));
    const ctx = { root: dir };
    for (const role of ['spec', 'commands', 'symbols']) assert.equal(sectionText(ctx, kb, 'A01', role), null);
    assert.equal(pack(ctx, kb, 'A01'), null);
}));
test('source_admission_enforces_utf8_byte_boundary_on_disk_and_cache', () => scratch(dir => {
    const boundary = '\u{10000}'.repeat(MAX_INPUT_BYTES / 4);
    const ctx = { root: dir, texts: new Map([['at', boundary], ['over', boundary + 'x']]) };
    assert.ok(inputText(ctx, 'at') === boundary);
    assert.equal(inputText(ctx, 'over'), null);
    const path = resolve(dir, 'source.rs');
    writeFileSync(path, boundary);
    assert.ok(inputText({ root: dir }, 'source.rs') === boundary);
    writeFileSync(path, boundary + 'x');
    assert.equal(inputText({ root: dir }, 'source.rs'), null);
}));
test('wrapped_suite_and_returned_metadata_are_indexed', () => {
    const suite = real().tests.find(t => t.kind === 'suite' && t.name === 'list_chunks');
    assert.ok(suite);
    assert.equal(suite.cases, true);
    assert.equal(suite.traps, true);
    assert.equal(suite.inspect, true);
    assert.equal(suite.fixture, 'tests/fixtures/list_chunks');
    assert.equal(suite.command, 'node tests/features.mjs target/release/tsuzuri list_chunks');
});
test('checker_skips_oversized_rust_and_javascript', () => scratch(async dir => {
    cpSync(mini, dir, { recursive: true });
    const padding = ' '.repeat(8 * 1024 * 1024);
    writeFileSync(resolve(dir, 'src/oversized.rs'), padding + '\npub const ONLY_OVERSIZED_CONSTANT: i32 = 1;\n');
    writeFileSync(resolve(dir, 'tests/oversized.mjs'), padding + '\nconst oversized_js_symbol_name = 1;\n');
    writeFileSync(resolve(dir, '_features/GUIDE-LITE.md'), '# Guide\n`ONLY_OVERSIZED_CONSTANT`\n`oversized_js_symbol_name`\n');
    const result = checkGuideLite({ root: dir }, collect(dir));
    assert.equal(result.failures.length, 2);
    assert.ok(result.failures.some(f => f.includes('ONLY_OVERSIZED_CONSTANT')));
    assert.ok(result.failures.some(f => f.includes('oversized_js_symbol_name')));
}));
test('symbol_queries_and_briefs_skip_oversized_sources', () => scratch(async dir => {
    cpSync(mini, dir, { recursive: true });
    writeFileSync(resolve(dir, 'src/check.rs'), ' '.repeat(8 * 1024 * 1024) + '\npub fn oversized_declaration() {}\n');
    const ctx = { root: dir };
    const kb = collect(dir);
    const recovered = sectionText(ctx, kb, 'A01', 'symbols');
    assert.ok(!recovered.includes('oversized_declaration'));
    assert.match(recovered, /PUBLIC SYMBOLS src\/lib\.rs/);
    const text = pack(ctx, kb, 'A01');
    assert.ok(!text.includes('oversized_declaration'));
    assert.match(text, /PUBLIC SYMBOLS src\/lib\.rs/);
}));
test('literal_h3_section_is_retrievable', () => {
    const text = sectionText(context(), fixtureKb(), 'A01', '停止条件');
    assert.match(text, /=== SECTION A01 停止条件/);
    assert.match(text, /E1012/);
});
test('literal_section_names_use_the_same_phase_filter_as_roles', () => {
    const kb = fixtureKb();
    for (const [role, title] of [['steps', '実装手順'], ['spec', '仕様']]) {
        assert.equal(sectionText(context(), kb, 'A01', title, 1), sectionText(context(), kb, 'A01', role, 1));
    }
});
test('interface_recovery_names_each_contributing_section', () => {
    const ctx = context();
    ctx.texts = new Map([
        ['_features/A01-alpha.md', '# A01: Alpha\n## 前提とする他チケットのインターフェース\n' + 'own\n'.repeat(5000)],
        ['_features/_completed/B02-beta.md', '# B02: Beta\n## 他チケットへの提供インターフェース\n' + 'provided\n'.repeat(5000)],
    ]);
    const text = pack(ctx, fixtureKb(), 'A01', { maxChars: 16000 });
    assert.match(text, /section A01 needs/);
    assert.match(text, /section B02 gives/);
});
test('cited_const_function_is_prioritized_beyond_the_first_ten_symbols', () => {
    const ctx = context();
    const original = readFileSync(resolve(mini, '_features/A01-alpha.md'), 'utf8');
    const code = Array.from({ length: 12 }, (_, i) => `pub fn early_${i}() {}`).join('\n') + '\npub const fn unix() {}\n';
    ctx.texts = new Map([['_features/A01-alpha.md', original + '\nPlatform::unix\n'], ['src/check.rs', code]]);
    const recovered = sectionText(ctx, fixtureKb(), 'A01', 'symbols');
    assert.match(recovered, /pub const fn unix/);
    assert.equal((recovered.match(/pub fn early_/g) ?? []).length, 9);
    assert.match(pack(ctx, fixtureKb(), 'A01'), /pub const fn unix/);
});
test('symbol_recovery_reads_every_contributing_source', () => {
    const ctx = context();
    const original = readFileSync(resolve(mini, '_features/A01-alpha.md'), 'utf8');
    ctx.texts = new Map([['_features/A01-alpha.md', original.replace('共通。D-01〜D-03。', 'long\n'.repeat(5000))]]);
    const text = pack(ctx, fixtureKb(), 'A01', { maxChars: 16000 });
    assert.match(text, /rest: node scripts\/repo-kb\.mjs section A01 symbols/);
    const recovered = sectionText(ctx, fixtureKb(), 'A01', 'symbols');
    assert.match(recovered, /PUBLIC SYMBOLS src\/check\.rs/);
    assert.match(recovered, /PUBLIC SYMBOLS src\/lib\.rs/);
    assert.match(recovered, /pub (?:fn|struct|enum)/);
});
test('command_recovery_returns_complete_generated_list', () => {
    const ctx = context();
    const kb = fixtureKb();
    const text = sectionText(ctx, kb, 'A01', 'commands');
    assert.match(text, /cargo test --locked --test alpha/);
    assert.match(text, /node tests\/features\.mjs target\/release\/tsuzuri storage/);
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
