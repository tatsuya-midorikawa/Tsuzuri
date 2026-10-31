# G35 付録 D・E・F

[G35 本文](G35-generated-docs-single-source.md)の付録。D は参照コード（`target/` の下の作業用ディレクトリで実際に動かして確かめたもの。D.2〜D.4・D.6〜D.11 は、付録 F の期待値・HEAD の文書・HEAD の `tests/benchmark_report.mjs` に対して実行した。D.1 の Rust は `Documentation` と `Span` を複製した単体のファイルを `rustc --edition 2024 --test` で動かして確かめ、実際のクレートは建てていない。実装はこれを写し、名前と挙動を変えない）、E は文書に書く文面、F はテストの期待値。

## D. 参照コード

### D.1 `Documentation`（`src/syntax.rs`）とそのテスト

`Documentation::deprecation` の本体を共通の `tag_notes` に寄せ、3 つの読み取り関数を足す（既存の doc コメント `/// The note of the `@deprecated` tag (G19)…` は残す）:

```rust
impl Documentation {
    pub fn deprecation(&self) -> Option<&str> {
        self.tag_notes("@deprecated").next()
    }

    /// The value of the first `@complexity` line (G35), which may be empty.
    pub fn complexity(&self) -> Option<&str> {
        self.tag_notes("@complexity").next()
    }

    /// The value of the first `@since` line (G35), which may be empty.
    pub fn since(&self) -> Option<&str> {
        self.tag_notes("@since").next()
    }

    /// The value of every `@see` line (G35), in order.
    pub fn see(&self) -> Vec<&str> {
        self.tag_notes("@see").collect()
    }

    /// The rest of every line whose first word is exactly `tag` (leading blanks ignored; the tag
    /// ends the line or is followed by a space or a tab, so `@deprecatedX` is not `@deprecated`).
    fn tag_notes<'a>(&'a self, tag: &'static str) -> impl Iterator<Item = &'a str> + 'a {
        let text = if self.text.contains(tag) { self.text.as_str() } else { "" };
        text.lines().filter_map(move |line| {
            let rest = line.trim_start().strip_prefix(tag)?;
            (rest.is_empty() || rest.starts_with([' ', '\t'])).then(|| rest.trim())
        })
    }
}
```

`tests/doc_tags.rs`（手順 2.1。`Documentation` と `Span` は公開）:

```rust
use tsuzuri::diagnostic::Span;
use tsuzuri::syntax::Documentation;

fn documented(text: &str) -> Documentation {
    Documentation { text: text.to_owned(), span: Span::default() }
}

/// The implementation before G35, kept as an independent reference.
fn old_deprecation(text: &str) -> Option<String> {
    if !text.contains("@deprecated") {
        return None;
    }
    text.lines().find_map(|line| {
        let rest = line.trim_start().strip_prefix("@deprecated")?;
        (rest.is_empty() || rest.starts_with([' ', '\t'])).then(|| rest.trim().to_owned())
    })
}
```

テスト 3 件の入力と期待値は F.3。

### D.2 `scripts/gen-docs.mjs`

```js
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, readdirSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

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

export function diagnosticKindsTable(lines) {     // DIAGNOSTIC_KINDS_HEADER は最初の 2 行。G23 の現物に合わせる
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

export function parseStdPage(text) {              // `tsuzuri doc std` の 1 ページ → [{ name, doc }]（def の節だけ）
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
    // Phase 2 (手順 2.9): { name: 'complexity:Array', file: '…/array.md' }, { name: 'complexity:Array', file: 'docs/language.md' }, … 16 件
    // Phase 3 (手順 3.6): { name: 'benchmark-summary', file: 'docs/benchmarks.md' }, { name: 'perf-metrics', file: 'docs/benchmarks.md' }
];

function generator(name) {
    if (name === 'diagnostic-kinds') return (context) => diagnosticKindsTable(context.catalog());
    const module = /^complexity:(.+)$/.exec(name)?.[1];
    if (module) {
        return (context) => {
            const entries = context.std()[module];
            if (!entries) throw new Error(`std has no module '${module}'`);
            return complexityTable(module, entries);
        };
    }
    throw new Error(`no generator for '${name}'`);                 // Phase 3 が benchmark-summary・perf-metrics を足す
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
        for (const item of [...items].sort((left, right) => right.block.start - left.block.start)) {   // 後ろから置き換える
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
    const run = (args) => spawnSync(compiler, args, { cwd: base, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, env: { ...process.env, TSUZURI_CACHE_DIR: '' } });
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
```

`main` は `scripts/gen-docs.mjs` を直接実行したときだけ動く。終了コードは「仕様 1」の表のとおり（`--list` と一致・書き込みは 0、`--check` のずれと例外は 1、未知の旗・`--only` の不一致・コンパイラ無しは 2）。**書き込みの `skip` の行**は、書き込みの後に `PLACEMENTS` のうち `requires: 'explain'` で `hasExplain` が偽のものを出す。`PLACEMENTS` のテスト用の差し替えは、`planBlocks`・`checkBlocks`・`writeBlocks` の `options.placements`（既定 `PLACEMENTS`）に渡す。`scripts/check-docs.mjs` は `import { checkBlocks, createContext } from './gen-docs.mjs'` し、`checkBlocks(createContext({ root, compiler }), { files: requested.length ? requested : undefined })` を呼ぶ。

### D.3 `benchmarks/render.mjs`

```js
const HTML = { '&': '&amp;', '<': '&lt;', '>': '&gt;' };
export const cell = (value) => String(value).replace(/[&<>]/g, (char) => HTML[char]).replaceAll('|', '\\|').replace(/\r?\n/g, ' ');
const text = (value) => String(value).replace(/[&<>]/g, (char) => HTML[char]);
export const serializeJson = (value) => `${JSON.stringify(value, null, 2)}\n`;

export function renderTable(headers, rows, { timingColumns = [] } = {}) {
    const tsuzuriColumn = timingColumns.find((index) => headers[index].startsWith('Tsuzuri'));
    const header = [...headers, ...(timingColumns.length ? ['観測最速'] : []), ...(tsuzuriColumn !== undefined ? ['Tsuzuri 相対性能'] : [])];
    const lines = [`| ${header.map(cell).join(' | ')} |`, `| ${header.map((name, index) => (index === 0 || name === '観測最速' ? '---' : '---:')).join(' | ')} |`];
    for (const row of rows) {
        if (row.length !== headers.length) throw new Error('Table column count mismatch');
        if (String(row[0]).includes('`')) throw new Error(`Row name contains a backtick: ${row[0]}`);
        const candidates = timingColumns.filter((index) => row[index] !== '-' && Number.isFinite(Number(row[index])));
        const minimum = Math.min(...candidates.map((index) => Number(row[index])));
        const winners = candidates.filter((index) => Number(row[index]) === minimum);
        const cells = row.map((value, index) => (index === 0 ? `\`${String(value).replaceAll('|', '\\|')}\`` : winners.includes(index) ? `**${cell(value)}**` : cell(value)));
        if (timingColumns.length) {
            const label = winners.length ? winners.map((index) => headers[index].replace(/ \([^)]*\)$/, '')).join(' / ') : '条件差あり';
            cells.push(cell(label + (winners.length > 1 ? '（同率）' : '')));
        }
        if (tsuzuriColumn !== undefined) {
            const comparable = candidates.includes(tsuzuriColumn) && minimum > 0 && Number.isFinite(minimum);
            const performance = comparable ? minimum / Number(row[tsuzuriColumn]) : null;
            cells.push(performance === null ? '-' : performance < 0.00001 ? '&lt;0.00001' : performance.toFixed(5));
        }
        lines.push(`| ${cells.join(' | ')} |`);
    }
    return lines;
}

export function renderSummaryBlock(summary) {     // 本体の行。文は E.5
    const environment = summary.environment;
    return [
        '**処理時間は小さいほど高速です。** 太字は各行の表示値で最速の実装です。同じ表示値は同率扱いです。僅差は一般的な優位性を示しません。',
        '',
        '**Tsuzuri 相対性能は大きいほど良好です。** 最速を `1.00000` とし、最速の処理時間を Tsuzuri の処理時間で割ります。`0.50000` は最速の半分の性能（処理時間は2倍）、`0.80000` は最速の80%の性能です。小数第5位に丸めるため、ごく僅かな差は `1.00000` になります。比較できない行は `-` です。',
        '',
        `- 最終測定: \`${summary.measured_at}\``,
        `- CPU: ${text(environment.cpu)} / ${text(environment.architecture)} / ${summary.cpu_mode}`,
        `- Tsuzuri: \`${environment.tsuzuri_sha256}\``,
        `- C#: ${text(environment.csharp)} / JavaScript: ${text(environment.javascript)}`,
        '',
        `**${summary.rows.length}種目 / ${summary.cpu_mode} / scale ${summary.scale} / ${summary.runs}回の中央値**`,
        '',
        ...renderTable(summary.headers, summary.rows, { timingColumns: summary.timing_columns }),
        '',
        '`-` は未測定です。すべて wall time（ms/呼び出し）で、起動時間は含みません。C#/JavaScript は別プロセス、管理メモリの回収条件は言語ごとに異なります。詳細な比較条件は後述します。',
    ];
}

export function renderMetricsBlock(metrics) {
    const lines = [];
    for (const run of metrics.runs) {
        const host = run.host;
        if (lines.length) lines.push('');
        lines.push(`**run \`${run.run_id}\`** / ${run.date} / commit \`${run.commit}\`${run.dirty ? '（未コミットの変更を含む）' : ''}`, '',
            `- CPU: ${text(host.cpu)} / ${text(host.arch)} / ${host.cores} cores / ${text(host.os)} ${text(host.os_version)}`,
            `- Clang: ${text(host.clang ?? '未記録')} / rustc: ${text(host.rustc ?? '未記録')} / Node: ${text(host.node)}`,
            `- Tsuzuri SHA-256: \`${host.tsuzuri_sha256}\``);
        for (const table of run.tables) lines.push('', `**${text(table.caption)}**`, '', ...renderTable(table.headers, table.rows));
    }
    return lines;
}
```

`validateSummary(summary)`・`validateMetrics(metrics)`（同じファイル。失敗は `Error('invalid summary: …')`・`Error('invalid metrics: …')`）:

```js
const isText = (value) => typeof value === 'string';
export function validateSummary(summary) {
    const fail = (what) => { throw new Error(`invalid summary: ${what}`); };
    if (summary?.schema !== 'tz-bench-summary/1') fail('schema');
    if (!isText(summary.measured_at)) fail('measured_at');
    if (!['generic', 'native'].includes(summary.cpu_mode)) fail('cpu_mode');
    if (!(Number.isFinite(summary.scale) && summary.scale > 0)) fail('scale');
    if (!(Number.isInteger(summary.runs) && summary.runs >= 1)) fail('runs');
    for (const key of ['cpu', 'architecture', 'tsuzuri_sha256', 'csharp', 'javascript']) if (!isText(summary.environment?.[key])) fail(`environment.${key}`);
    if (!Array.isArray(summary.headers) || summary.headers.length < 2 || !summary.headers.every(isText)) fail('headers');
    if (!Array.isArray(summary.timing_columns) || !summary.timing_columns.every((index) => Number.isInteger(index) && index >= 1 && index < summary.headers.length && / \(ms\)$/.test(summary.headers[index]))) fail('timing_columns');
    if (!Array.isArray(summary.rows) || summary.rows.length === 0) fail('rows');
    const names = new Set();
    for (const row of summary.rows) {
        if (!Array.isArray(row) || row.length !== summary.headers.length || !row.every(isText)) fail('row shape');
        if (names.has(row[0])) fail(`duplicate row ${row[0]}`);
        names.add(row[0]);
        for (const index of summary.timing_columns) if (row[index] !== '-' && !(Number.isFinite(Number(row[index])) && Number(row[index]) > 0)) fail(`timing ${row[0]}`);
    }
    return summary;
}
```
`validateMetrics(metrics)`（同じファイル。`isText` は上の定義）:

```js
export function validateMetrics(metrics) {
    const fail = (what) => { throw new Error(`invalid metrics: ${what}`); };
    if (metrics?.schema !== 'tz-bench-metrics/1') fail('schema');
    if (!Array.isArray(metrics.runs) || metrics.runs.length === 0) fail('runs');
    for (const run of metrics.runs) {
        for (const key of ['run_id', 'date']) if (!isText(run?.[key]) || run[key] === '') fail(key);
        if (!/^[0-9a-f]{12}$/.test(run.commit)) fail(`commit of ${run.run_id}`);
        if (typeof run.dirty !== 'boolean') fail(`dirty of ${run.run_id}`);
        const host = run.host;
        if (!Number.isInteger(host?.cores)) fail(`host.cores of ${run.run_id}`);
        for (const key of ['clang', 'rustc']) if (!(host[key] === null || isText(host[key]))) fail(`host.${key} of ${run.run_id}`);
        for (const key of ['cpu', 'arch', 'os', 'os_version', 'node', 'tsuzuri_sha256']) if (!isText(host[key])) fail(`host.${key} of ${run.run_id}`);
        if (!Array.isArray(run.tables) || run.tables.length === 0) fail(`tables of ${run.run_id}`);
        for (const table of run.tables) {
            if (!isText(table?.caption) || table.caption === '') fail(`caption of ${run.run_id}`);
            if (!Array.isArray(table.headers) || !table.headers.every(isText)) fail(`headers of ${table.caption}`);
            if (!Array.isArray(table.rows) || !table.rows.every((row) => Array.isArray(row) && row.length === table.headers.length && row.every(isText))) fail(`rows of ${table.caption}`);
        }
    }
    return metrics;
}
```

`summarizeResults`（`report.mjs`）・`summarizeRecords`（`metrics.mjs`）・`publishSummary`・`publishMetrics` の実装は D.8・D.9（旧 `renderSummary`・`renderMetricsSummary` の検査とグループ化をそのまま使い、HTML を作る代わりに summary・run のオブジェクトを返す）。

### D.4 移行のスクリプト（`benchmarks/migrate-html.mjs`。手順 3.4 で書き、3.7 で消す）

`docs/benchmarks.md` を読み、2 ブロックを解析し、旧 `renderTable` で作り直して元の文字列と比べ（往復検査）、すべて通ったときだけ JSON と文書を書く。旧版の複製 `benchmarks/legacy-report.mjs` と、D.3 の `benchmarks/render.mjs` が要る。HEAD の文書で実行して、出力は次のとおり（付録 A.3・A.4 の数と一致することを確かめた。`--check` を付けると何も書かず、4 行目の代わりに `--check: nothing written` を出して終了コード 0）:

```text
summary: headers 8 rows 38 round trip true
metrics: runs 2 tables 36 rows 314 cells 2109 round trip true
outside the blocks: unchanged
docs/benchmarks.md 952255 -> 489211 bytes; summary.json 6746; metrics.json 70695
```

```js
import assert from 'node:assert/strict';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { escape, renderTable as legacyTable } from './legacy-report.mjs';       // 旧版の複製（git show HEAD:benchmarks/report.mjs）
import { renderMetricsBlock, renderSummaryBlock, serializeJson, validateMetrics, validateSummary } from './render.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const documentPath = resolve(root, 'docs/benchmarks.md');
const dataDirectory = resolve(root, 'docs/benchmarks-data');
const document = readFileSync(documentPath, 'utf8');
assert.ok(!document.includes('<!-- generated:'), 'docs/benchmarks.md is already migrated');

const unescape = (value) => value.replaceAll('&quot;', '"').replaceAll('&gt;', '>').replaceAll('&lt;', '<').replaceAll('&amp;', '&');
const TABLE = /<div style="overflow-x:auto;max-width:100%">\n<table.*?<\/table>\n<\/div>/gs;

export function parseTable(html, derived) {      // derived: summary の表（導出の 2 列を捨てる）
    const caption = unescape(/<caption[^>]*>(.*?)<\/caption>/s.exec(html)[1]);
    let headers = [...(/<thead><tr>\n(.*?)<\/tr><\/thead>/s.exec(html)[1]).matchAll(/<th scope="col"[^>]*>(.*?)<\/th>/gs)].map((match) => unescape(match[1]));
    if (derived) headers = headers.slice(0, -2);
    const rows = [...(/<tbody>\n(.*?)<\/tbody>/s.exec(html)[1]).matchAll(/<tr>\n(.*?)<\/tr>/gs)].map((row) => {
        const cells = [...row[1].matchAll(/<(th|td)[^>]*>(.*?)<\/\1>/gs)].map((match) => {
            const inner = match[2].replace(/<\/?strong>/g, '');
            const code = /^<code[^>]*>(.*?)<\/code>$/s.exec(inner);
            return unescape(code ? code[1] : inner).trim();
        });
        return derived ? cells.slice(0, -2) : cells;
    });
    return { caption, headers, rows };
}

const between = (start, end) => {
    const from = document.indexOf(start);
    assert.ok(from >= 0, `missing ${start}`);
    return document.slice(from + start.length, document.indexOf(end, from));
};

// ---- summary: 解析 → 旧 renderTable で作り直して元の文字列と比べる -----------------------------
const summaryText = between('<!-- benchmark-summary:start -->', '<!-- benchmark-summary:end -->');
const paragraphs = [...summaryText.matchAll(/^<p>.*<\/p>$/gm)].map((match) => match[0]);
assert.equal(paragraphs.length, 4);
const environment = /^<p>最終測定: <code>(.*?)<\/code><br>CPU: (.*?) \/ (.*?) \/ (generic|native)<br>Tsuzuri: <code[^>]*>(.*?)<\/code><br>C#: (.*?) \/ JavaScript: (.*?)<\/p>$/.exec(paragraphs[2]);
assert.ok(environment, 'environment paragraph');
const parsed = parseTable(summaryText.match(TABLE)[0], true);
const caption = /^(\d+)種目 \/ (generic|native) \/ scale ([0-9.]+) \/ (\d+)回の中央値$/.exec(parsed.caption);
assert.ok(caption, 'summary caption');
const summary = {
    schema: 'tz-bench-summary/1', measured_at: unescape(environment[1]), cpu_mode: caption[2], scale: Number(caption[3]), runs: Number(caption[4]),
    environment: { cpu: unescape(environment[2]), architecture: unescape(environment[3]), tsuzuri_sha256: unescape(environment[5]), csharp: unescape(environment[6]), javascript: unescape(environment[7]) },
    headers: parsed.headers, timing_columns: parsed.headers.flatMap((header, index) => (/ \(ms\)$/.test(header) ? [index] : [])), rows: parsed.rows,
};
assert.equal(Number(caption[1]), summary.rows.length);
const legacySummary = ['\n' + paragraphs[0], paragraphs[1],
    `<p>最終測定: <code>${escape(summary.measured_at)}</code><br>CPU: ${escape(summary.environment.cpu)} / ${escape(summary.environment.architecture)} / ${escape(summary.cpu_mode)}<br>Tsuzuri: <code style="overflow-wrap:anywhere">${escape(summary.environment.tsuzuri_sha256)}</code><br>C#: ${escape(summary.environment.csharp)} / JavaScript: ${escape(summary.environment.javascript)}</p>`,
    legacyTable(summary.headers, summary.rows, { caption: parsed.caption, timingColumns: summary.timing_columns }), paragraphs[3] + '\n'].join('\n');
assert.equal(legacySummary, summaryText, 'summary block does not round-trip');
console.log(`summary: headers ${summary.headers.length} rows ${summary.rows.length} round trip true`);

// ---- metrics: run ごとの <p> と表 -----------------------------------------------------------------
const metricsText = between('<!-- perf-metrics:start -->', '<!-- perf-metrics:end -->');
const runParts = metricsText.split(/^(?=<p>run: )/m).filter((part) => part.startsWith('<p>run: '));
const paragraphOf = (part) => /^<p>run: .*?<\/p>/s.exec(part)[0];
const runs = runParts.map((part) => {
    const paragraph = paragraphOf(part);
    const match = /^<p>run: <code>(.*?)<\/code> \/ (.*?) \/ commit: <code>([0-9a-f]{12})<\/code>(（未コミットの変更を含む）)?<br>CPU: (.*?) \/ (.*?) \/ (\d+) cores \/ (\S+) (.*?)<br>Clang: (.*?) \/ rustc: (.*?) \/ Node: (.*?)<br>Tsuzuri SHA-256: <code>(.*?)<\/code><\/p>$/s.exec(paragraph);
    assert.ok(match, 'run paragraph');
    const optional = (value) => (value === '未記録' ? null : unescape(value));
    const run = {
        run_id: unescape(match[1]), date: unescape(match[2]), commit: match[3], dirty: Boolean(match[4]),
        host: { cpu: unescape(match[5]), arch: unescape(match[6]), cores: Number(match[7]), os: unescape(match[8]), os_version: unescape(match[9]), clang: optional(match[10]), rustc: optional(match[11]), node: unescape(match[12]), tsuzuri_sha256: unescape(match[13]) },
        tables: [...part.matchAll(TABLE)].map((table) => parseTable(table[0], false)),
    };
    const host = run.host;
    const legacyParagraph = `<p>run: <code>${escape(run.run_id)}</code> / ${escape(run.date)} / commit: <code>${escape(run.commit)}</code>${run.dirty ? '（未コミットの変更を含む）' : ''}<br>CPU: ${escape(host.cpu)} / ${escape(host.arch)} / ${host.cores} cores / ${escape(host.os)} ${escape(host.os_version)}<br>Clang: ${escape(host.clang ?? '未記録')} / rustc: ${escape(host.rustc ?? '未記録')} / Node: ${escape(host.node)}<br>Tsuzuri SHA-256: <code>${escape(host.tsuzuri_sha256)}</code></p>`;
    assert.equal(legacyParagraph, paragraph, 'run paragraph does not round-trip');
    return run;
});
const legacyMetrics = '\n' + runParts.map((part, index) => [paragraphOf(part), ...runs[index].tables.map((table) => legacyTable(table.headers, table.rows, { caption: table.caption }))].join('\n')).join('\n') + '\n';
assert.equal(legacyMetrics, metricsText, 'metrics block does not round-trip');
const metrics = { schema: 'tz-bench-metrics/1', runs };
const tables = runs.reduce((sum, run) => sum + run.tables.length, 0);
const bodyRows = runs.reduce((sum, run) => sum + run.tables.reduce((inner, table) => inner + table.rows.length, 0), 0);
const bodyCells = runs.reduce((sum, run) => sum + run.tables.reduce((inner, table) => inner + table.rows.length * table.headers.length, 0), 0);
console.log(`metrics: runs ${runs.length} tables ${tables} rows ${bodyRows} cells ${bodyCells} round trip true`);

// ---- 新しい文書。ブロックの外が 1 byte も変わらないことを確かめてから書く ---------------------------
const summaryJson = serializeJson(validateSummary(summary));
const metricsJson = serializeJson(validateMetrics(metrics));
const migrated = document
    .replace(/<!-- benchmark-summary:start -->[\s\S]*?<!-- benchmark-summary:end -->/, () => `<!-- generated:benchmark-summary -->\n${renderSummaryBlock(summary).join('\n')}\n<!-- /generated:benchmark-summary -->`)
    .replace(/<!-- perf-metrics:start -->[\s\S]*?<!-- perf-metrics:end -->/, () => `<!-- generated:perf-metrics -->\n${renderMetricsBlock(metrics).join('\n')}\n<!-- /generated:perf-metrics -->`);
const outside = (text, summaryPattern, metricsPattern) => text.replace(summaryPattern, 'SUMMARY').replace(metricsPattern, 'METRICS');
assert.equal(
    outside(migrated, /<!-- generated:benchmark-summary -->[\s\S]*?<!-- \/generated:benchmark-summary -->/, /<!-- generated:perf-metrics -->[\s\S]*?<!-- \/generated:perf-metrics -->/),
    outside(document, /<!-- benchmark-summary:start -->[\s\S]*?<!-- benchmark-summary:end -->/, /<!-- perf-metrics:start -->[\s\S]*?<!-- perf-metrics:end -->/),
    'the text outside the blocks changed');
console.log('outside the blocks: unchanged');

if (process.argv.includes('--check')) {          // 何も書かずに検査だけ
    console.log('--check: nothing written');
    process.exit(0);
}

mkdirSync(dataDirectory, { recursive: true });
writeFileSync(resolve(dataDirectory, 'summary.json'), summaryJson);
writeFileSync(resolve(dataDirectory, 'metrics.json'), metricsJson);
writeFileSync(documentPath, migrated);
console.log(`docs/benchmarks.md ${Buffer.byteLength(document)} -> ${Buffer.byteLength(migrated)} bytes; summary.json ${Buffer.byteLength(summaryJson)}; metrics.json ${Buffer.byteLength(metricsJson)}`);
```

往復検査が通らなければ `assert` が例外で止まり、何も書かない（停止条件 2）。実行済みの文書では `docs/benchmarks.md is already migrated` で止まる。

### D.5 `.github/workflows/docs.yml`（Phase 1。手順 1.5。YAML として検証済み）

```yaml
name: Tsuzuri docs

# G35: the generated documentation blocks, the drift checks and the link check. The compiler is built
# without LLVM: every command here only reads, checks or documents (`check`, `doc`, `--help`).
on:
  workflow_dispatch:
  pull_request:
    paths:
      - docs/**
      - _tsuzuri/**
      - _specs/**
      - README.md
      - std/**
      - src/**
      - benchmarks/**
      - scripts/gen-docs.mjs
      - scripts/check-cli-docs.mjs
      - scripts/check-links.mjs
      - scripts/check-docs.mjs
      - tests/gen_docs.mjs
      - tests/check_cli_docs.mjs
      - tests/check_links.mjs
      - tests/benchmark_render.mjs
      - tests/fixtures/gen-docs/**
      - Cargo.*
      - .github/workflows/docs.yml

permissions:
  contents: read

concurrency:
  group: docs-${{ github.ref }}
  cancel-in-progress: ${{ github.event_name == 'pull_request' }}

jobs:
  docs:
    runs-on: ubuntu-24.04
    timeout-minutes: 30
    steps:
      - uses: actions/checkout@v7
      - uses: actions/setup-node@v7
        with:
          node-version: 24
      - uses: dtolnay/rust-toolchain@stable
      - uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/registry/index
            ~/.cargo/registry/cache
            ~/.cargo/git/db
          key: docs-${{ runner.os }}-${{ hashFiles('Cargo.lock') }}
          restore-keys: docs-${{ runner.os }}-
      - name: Build the compiler
        run: cargo build --release --locked
      # Each step starts with `if: ${{ !cancelled() }}` so that a failure above does not hide it.
      - name: Generated blocks
        if: ${{ !cancelled() }}
        run: |
          node tests/gen_docs.mjs
          node scripts/gen-docs.mjs --check
```

Phase 3（手順 3.1・3.6）は、同じ `jobs.docs.steps` の最後に `- name: Benchmark renderer and links into docs/benchmarks.md`（`if: ${{ !cancelled() }}`、`run: |` で `node tests/benchmark_render.mjs`・`node tests/check_links.mjs`・`node scripts/check-links.mjs`）を、Phase 4（手順 4.6）は `- name: Drift checks`（`node tests/check_cli_docs.mjs` と `node scripts/check-cli-docs.mjs`）を足す。`paths` は変えない。

### D.6 `scripts/check-links.mjs`

```js
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, extname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');

export function headings(source) {                 // scripts/check-docs.mjs の同名の関数と同じ式
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

export function checkLinks({ root: base, files, into }) {      // files・into は base からの相対パス
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
```

`--list-headings PATH` の `PATH` は実行したディレクトリからの相対パス（リポジトリのルートで実行する）。`files` は `git ls-files '*.md'`（追跡されているファイルだけ）。壊れたものは stderr に 1 行ずつ出し、終了コードは 1。

### D.7 `scripts/check-cli-docs.mjs`（HEAD の基線を再現することを確かめた）

```js
import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createContext } from './gen-docs.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const REFERENCE = '_tsuzuri/language-reference/';
const unique = (values) => [...new Set(values)].sort();

// ---- cli ------------------------------------------------------------------------------------------
export function extractCli(help) {
    return {
        flags: unique([...help.matchAll(/(?<![\w-])(--[a-z][a-z0-9-]*)/g)].map((match) => match[1])),
        commands: unique([...help.matchAll(/^ {2,4}tsuzuri (?:\[(build)\]|([a-z][a-z-]*))/gm)].map((match) => match[1] ?? match[2])),
        variables: unique([...help.matchAll(/\bTSUZURI_[A-Z0-9_]+\b/g)].map((match) => match[0])),
    };
}
export const EXEMPT_FLAGS = { '--sysroot': 'a flag of the C compiler named in the TSUZURI_SYSROOT sentence of --help' };
export function checkCli(cli, pages) {           // pages: { 'README.md': text, 'option.md': text }
    const messages = [];
    for (const page of ['README.md', 'option.md']) {
        for (const flag of cli.flags) if (!(flag in EXEMPT_FLAGS) && !pages[page].includes(flag)) messages.push(`${page} lacks flag ${flag}`);
        for (const variable of cli.variables) if (!pages[page].includes(variable)) messages.push(`${page} lacks variable ${variable}`);
    }
    for (const command of cli.commands) {
        if (!new RegExp(`tsuzuri (?:\\[${command}\\]|${command})(?![A-Za-z0-9_-])`).test(pages['README.md'])) messages.push(`README.md lacks command ${command}`);
    }
    return messages;
}

// ---- codes ----------------------------------------------------------------------------------------
export function documentedCodes(text) {          // 表の各行の第 1 セル。範囲と結合セルを展開する
    const codes = [];
    for (const line of text.split('\n')) {
        const row = /^\| (.+?) \|/.exec(line);
        if (!row) continue;
        const range = /`([EW])([0-9]{4})`\s*[–-]\s*`([EW])([0-9]{4})`/.exec(row[1]);
        if (range) for (let n = Number(range[2]); n <= Number(range[4]); n++) codes.push(`${range[1]}${String(n).padStart(4, '0')}`);
        else codes.push(...[...row[1].matchAll(/`([EW][0-9]{4})`/g)].map((match) => match[1]));
    }
    return unique(codes);
}
export function scanEmittedCodes(sources) {      // sources: src/**/*.rs の本文の配列（G23 が無いとき）
    const codes = [];
    for (const text of sources) {
        codes.push(...[...text.matchAll(/\(\s*"([EW][0-9]{4})"\s*,/g)].map((match) => match[1]));
        codes.push(...[...text.matchAll(/\\"code\\":\\"([EW][0-9]{4})\\"/g)].map((match) => match[1]));
    }
    return unique(codes);
}
export function checkCodes(emitted, pages) {     // pages: { PATH: text }
    const messages = [];
    for (const [page, text] of Object.entries(pages)) {
        const documented = documentedCodes(text);
        for (const code of emitted) if (!documented.includes(code)) messages.push(`${code} is emitted but missing from ${page}`);
        for (const code of documented) if (!emitted.includes(code)) messages.push(`${code} is in ${page} but no source emits it`);
    }
    return messages;
}
const walk = (directory, extension) => readdirSync(directory, { withFileTypes: true })
    .flatMap((entry) => (entry.isDirectory() ? walk(join(directory, entry.name), extension) : entry.name.endsWith(extension) ? [join(directory, entry.name)] : []));
export function emittedCodes(context) {
    if (context.hasExplain()) return unique(context.catalog().map((line) => JSON.parse(line).code));
    return scanEmittedCodes(walk(resolve(context.root, 'src'), '.rs').map((file) => readFileSync(file, 'utf8')));
}

// ---- version --------------------------------------------------------------------------------------
export const versionMentions = (text) => [...text.matchAll(/\b[Tt]suzuri v?(\d+\.\d+\.\d+)\b/g)].map((match) => match[1]);
export function checkVersion(version, versionOutput, mentions) {      // mentions: [{ file, version }]
    const messages = mentions.filter((mention) => mention.version !== version).map((mention) => `${mention.file} says ${mention.version}, Cargo.toml says ${version}`);
    if (versionOutput !== `tsuzuri ${version}`) messages.push(`--version prints '${versionOutput}', expected 'tsuzuri ${version}'`);
    return messages;
}

// ---- std ------------------------------------------------------------------------------------------
export const STD_PAGES = { MatrixView: 'built-in-types-and-modules/matrix-view.md', Async: 'async-tasks-and-lazy/async.md' };
export const stdPage = (module) => STD_PAGES[module] ?? `built-in-types-and-modules/${module.toLowerCase()}.md`;
export function checkStd(modules, readPage) {     // modules: { Module: [{ name, doc }] }。readPage(PATH) は本文か、無ければ null
    const messages = [];
    for (const [module, entries] of Object.entries(modules)) {
        if (entries.length === 0) continue;
        const page = stdPage(module);
        const text = readPage(page);
        if (text === null) { messages.push(`${module} has ${entries.length} public functions but no page in STD_PAGES`); continue; }
        for (const { name } of entries) if (!text.includes(`\`${name}\``) && !text.includes(`${module}.${name}`)) messages.push(`${module}.${name} is not mentioned in ${page}`);
    }
    return messages;
}

// ---- running the checks ----------------------------------------------------------------------------
export const CHECKS = ['cli', 'codes', 'version', 'std'];
export function runChecks(context, compiler, names) {
    const read = (path) => readFileSync(resolve(context.root, path), 'utf8');
    const run = (args) => spawnSync(compiler, args, { cwd: context.root, encoding: 'utf8', maxBuffer: 16 * 1024 * 1024 });
    return CHECKS.filter((name) => names.includes(name)).map((name) => {
        if (name === 'cli') {
            const help = run(['--help']);
            if (help.status !== 0) throw new Error(`${compiler} --help failed: ${help.stderr}`);
            const cli = extractCli(help.stdout);
            return { name, summary: `${cli.flags.length} flags, ${cli.commands.length} commands, ${cli.variables.length} variables`,
                messages: checkCli(cli, { 'README.md': read('README.md'), 'option.md': read(`${REFERENCE}compiler/option.md`) }) };
        }
        if (name === 'codes') {
            const emitted = emittedCodes(context);
            return { name, summary: `${emitted.length} codes`,
                messages: checkCodes(emitted, { [`${REFERENCE}compiler/diagnostics.md`]: read(`${REFERENCE}compiler/diagnostics.md`), 'docs/language.md': read('docs/language.md') }) };
        }
        if (name === 'version') {
            const version = /^version = "(\d+\.\d+\.\d+)"/m.exec(read('Cargo.toml'))[1];
            const files = ['README.md', ...readdirSync(resolve(context.root, 'docs')).filter((file) => file.endsWith('.md') && file !== 'benchmarks.md').map((file) => `docs/${file}`),
                ...walk(resolve(context.root, '_tsuzuri'), '.md').map((file) => relative(context.root, file))];
            const mentions = files.flatMap((file) => versionMentions(read(file)).map((found) => ({ file, version: found })));
            return { name, summary: `${version} (${mentions.length} mentions)`, messages: checkVersion(version, run(['--version']).stdout.trim(), mentions) };
        }
        const modules = context.std();
        const readPage = (page) => (existsSync(resolve(context.root, REFERENCE, page)) ? read(`${REFERENCE}${page}`) : null);
        const functions = Object.values(modules).reduce((sum, entries) => sum + entries.length, 0);
        return { name, summary: `${functions} public functions in ${Object.values(modules).filter((entries) => entries.length > 0).length} modules`, messages: checkStd(modules, readPage) };
    });
}

// ---- command line ----------------------------------------------------------------------------------
const USAGE = 'usage: node scripts/check-cli-docs.mjs [--only cli|codes|version|std]... [--bin PATH]';

export function main(argv) {
    const only = [];
    let compiler = process.env.TSUZURI_BIN ?? join(root, 'target/release/tsuzuri');
    for (let index = 0; index < argv.length; index += 1) {
        if (argv[index] === '--only' && index + 1 < argv.length) only.push(argv[++index]);
        else if (argv[index] === '--bin' && index + 1 < argv.length) compiler = resolve(argv[++index]);
        else {
            console.error(`check-cli-docs: unknown option '${argv[index]}'\n${USAGE}`);
            return 2;
        }
    }
    const unknown = only.find((name) => !CHECKS.includes(name));
    if (unknown !== undefined) {
        console.error(`check-cli-docs: unknown check '${unknown}'\n${USAGE}`);
        return 2;
    }
    if (!existsSync(compiler)) {
        console.error(`check-cli-docs: compiler not found: ${compiler}`);
        return 2;
    }
    let failed = 0;
    const results = runChecks(createContext({ root, compiler }), compiler, only.length > 0 ? only : CHECKS);
    for (const result of results) {
        if (result.messages.length === 0) console.log(`ok ${result.name}: ${result.summary}`);
        else {
            failed++;
            for (const message of result.messages) console.error(`FAIL ${result.name}: ${message}`);
        }
    }
    console.log(`check-cli-docs: ${results.length} checks, ${failed} failed`);
    return failed > 0 ? 1 : 0;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) process.exitCode = main(process.argv.slice(2));
```

HEAD での出力（G23 が無い。順序は `cli`・`codes`・`version`・`std` で、`ok` は stdout、`FAIL` は stderr）:

```text
FAIL cli: README.md lacks flag --debug-output
FAIL cli: README.md lacks flag --version
FAIL cli: README.md lacks flag --warn
FAIL cli: README.md lacks variable TSUZURI_DSYMUTIL
FAIL cli: README.md lacks variable TSUZURI_LLVM_LINK
FAIL cli: option.md lacks flag --git
FAIL cli: option.md lacks flag --rev
FAIL cli: README.md lacks command toolchain
FAIL codes: W2001 is emitted but missing from docs/language.md
ok version: 0.1.0 (22 mentions)
ok std: 559 public functions in 42 modules
check-cli-docs: 4 checks, 2 failed
```

### D.8 `benchmarks/report.mjs`（手順 3.3。書き換え後の全文。`renderTable`・`escape`・`renderSummary`・`updateSummary` と HTML の定数を消し、`summarizeResults`・`publishSummary` にする。検証済み）

```js
import assert from "node:assert/strict";
import { mkdirSync, readFileSync, renameSync, rmSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { replaceNamedBlock } from "../scripts/gen-docs.mjs";
import { renderSummaryBlock, serializeJson, validateSummary } from "./render.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const languages = [["tsuzuri", "Tsuzuri"], ["c", "C"], ["cpp", "C++"], ["rust", "Rust"], ["csharp", "C#"], ["javascript", "JavaScript"]];
const median = (values) => {
  const sorted = [...values].sort((left, right) => left - right);
  return (sorted[Math.floor((sorted.length - 1) / 2)] + sorted[Math.floor(sorted.length / 2)]) / 2;
};

export function summarizeResults(results) {
  assert.ok(results.length > 0, "At least one benchmark result is required");
  const first = results[0];
  const signature = (result) => JSON.stringify({ cpu: result.cpu, scale: result.scale, environment: result.environment, native_options: result.native_options,
    workloads: result.workloads.map((work) => [work.family, work.name, work.size, Object.keys(work.variants).sort()]) });
  for (const result of results) {
    assert.equal(result.mode, "benchmark", "Quick/smoke timings must not be published");
    assert.ok(["generic", "native"].includes(result.cpu));
    assert.ok(Number.isFinite(result.scale) && result.scale > 0);
    assert.ok(result.workloads.length > 0);
    assert.equal(signature(result), signature(first), "Do not combine different compilers, environments, CPU modes, sizes or workloads");
    const keys = result.workloads.map((work) => `${work.family}/${work.name}`);
    assert.equal(new Set(keys).size, keys.length, "Duplicate workload");
    for (const work of result.workloads) {
      assert.ok(Number.isSafeInteger(work.size) && work.size >= 0);
      assert.ok(Number.isInteger(work.checks) && work.checks > 0);
      for (const language of ["tsuzuri", "cpp", "rust", "csharp", "javascript"]) assert.ok(work.variants[language], `Missing ${language} result`);
      for (const [language] of languages) if (work.variants[language]) {
        assert.ok(Number.isFinite(work.variants[language].median_wall_ms) && work.variants[language].median_wall_ms > 0, "Timings must be finite and positive");
      }
    }
  }
  const rows = first.workloads.map((work, index) => [
    `${work.family}/${work.name}`, work.family === "cpp" && work.name === "mandelbrot" ? `${work.size} x ${work.size}` : String(work.size),
    ...languages.map(([language]) => work.variants[language]
      ? formatMilliseconds(median(results.map((result) => result.workloads[index].variants[language].median_wall_ms))) : "-"),
  ]);
  const native = first.environment.native.control;
  const recorded = results.map((result) => result.measured_at).filter(Boolean).sort().at(-1) ?? "未記録";
  return validateSummary({
    schema: "tz-bench-summary/1", measured_at: recorded, cpu_mode: first.cpu, scale: first.scale, runs: results.length,
    environment: { cpu: native.cpu, architecture: native.architecture, tsuzuri_sha256: native.compiler_sha256,
      csharp: first.environment.csharp.runtime, javascript: first.environment.javascript.node },
    headers: ["種目", "仕事量", ...languages.map(([, label]) => `${label} (ms)`)],
    timing_columns: languages.map((_, index) => index + 2),
    rows,
  });
}

function formatMilliseconds(value) {
  return value < 0.0000005 ? value.toExponential(3) : value.toFixed(6);
}

export function publishSummary(results, { document = join(root, "docs/benchmarks.md"), data = join(root, "docs/benchmarks-data/summary.json") } = {}) {
  const summary = summarizeResults(results);
  const updated = replaceNamedBlock(readFileSync(document, "utf8"), "benchmark-summary", renderSummaryBlock(summary), document);
  mkdirSync(dirname(data), { recursive: true });
  writeAtomically(data, serializeJson(summary));
  writeAtomically(document, updated);
}

export function writeAtomically(path, text) {
  const temporary = `${path}.${process.pid}.tmp`;
  let created = false;
  try {
    writeFileSync(temporary, text, { flag: "wx" });
    created = true;
    renameSync(temporary, path);
  } finally {
    if (created) rmSync(temporary, { force: true });
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const paths = process.argv.slice(2);
  assert.ok(paths.length > 0, "Usage: node benchmarks/report.mjs result.json [same-condition-result.json ...]");
  publishSummary(paths.map((path) => {
    const result = JSON.parse(readFileSync(path, "utf8"));
    return { ...result, measured_at: result.measured_at ?? statSync(path).mtime.toISOString() };
  }));
  console.log("Benchmark summary updated");
}
```

`benchmarks/run-managed.mjs` の `publishSummary([result])` は、そのまま動く（2 つ目の引数の既定値が `docs/benchmarks.md` と `docs/benchmarks-data/summary.json`）。

### D.9 `benchmarks/metrics.mjs` の変更（手順 3.3。4 か所。検証済み）

1. 8 行目の `import { escape, renderTable, writeAtomically } from "./report.mjs";` を次の 3 行に:

```js
import { replaceNamedBlock } from "../scripts/gen-docs.mjs";
import { renderMetricsBlock, serializeJson, validateMetrics } from "./render.mjs";
import { writeAtomically } from "./report.mjs";
```

2. `export function renderMetricsSummary(records)` を `export function summarizeRecords(records)` に改名し、検査（先頭から `const date = …` まで）は変えない。`const lines = [`<p>run: …</p>`];`（`<p>` の 1 行）は `const tables = [];` に置き換える。行の最初の 2 つのセルのうち、2 つ目を文字列にする（`record.size ?? "-"` → `String(record.size ?? "-")`。JSON のセルはすべて string）。表を作る最後の部分（`const { metric, unit, target, opt, cpu_mode } = group[0];` から関数の終わりまで）を次に置き換える:

```js
    const { metric, unit, target, opt, cpu_mode } = group[0];
    tables.push({ caption: `${metric} (${unit}) / ${target} / ${opt} / ${cpu_mode} / 中央値`,
      headers: ["種目", "仕事量", ...columns.map(([language, variant]) => `${labels[LANGUAGES.indexOf(language)]}${variant === "default" ? "" : ` ${variant}`}`)],
      rows: [...rows.values()] });
  }
  return { run_id: first.run_id, date, commit: first.commit.slice(0, 12), dirty: Boolean(first.dirty),
    host: { cpu: host.cpu, arch: host.arch, cores: host.cores, os: host.os, os_version: host.os_version, clang: host.clang ?? null, rustc: host.rustc ?? null, node: host.node, tsuzuri_sha256: host.tsuzuri_sha256 },
    tables };
}
```

3. `export function updateMetricsSummary(document, runs) { … }`（関数全体）を次に置き換える:

```js
export function publishMetrics(runs, { document = join(root, "docs/benchmarks.md"), data = join(root, "docs/benchmarks-data/metrics.json") } = {}) {
  assert.ok(runs.length > 0);
  const metrics = validateMetrics({ schema: "tz-bench-metrics/1", runs: runs.map(summarizeRecords) });
  const updated = replaceNamedBlock(readFileSync(document, "utf8"), "perf-metrics", renderMetricsBlock(metrics), document);
  mkdirSync(dirname(data), { recursive: true });
  writeAtomically(data, serializeJson(metrics));
  writeAtomically(document, updated);
}
```

4. 末尾の CLI の `if (publish) {` の中の 3 行（`const path = …`・`writeAtomically(path, updateMetricsSummary(…))`・`console.log("Metrics summary updated");`）を次の 2 行に:

```js
      publishMetrics(runs);
      console.log("Metrics summary updated");
```

`renderComparison`・`compareRuns`・`--baseline` の分岐は変えない。`writeAtomically` は `report.mjs` が `export` したまま。

### D.10 `tests/benchmark_report.mjs` の書き換え（手順 3.3。付録 S.5 の表のとおり。検証済み: `node tests/benchmark_report.mjs` が 34 行（`Benchmark report: …` 1 行と `Metrics: … passed` 33 行）・終了コード 0）

1. 7〜10 行目の import を次に:

```js
import { replaceNamedBlock } from "../scripts/gen-docs.mjs";
import { publishSummary, summarizeResults } from "../benchmarks/report.mjs";
import { renderMetricsBlock, renderSummaryBlock, renderTable } from "../benchmarks/render.mjs";
import { allocatorVariants, artifactFingerprint, captureRun, compareRuns, instrumentAllocations, makeRecord, measureProcess, median, parseCompilerDiagnostics, parseTimeOutput,
  parseTimePasses, publishMetrics, readRecords, renderComparison, summarize, summarizeRecords,
  unsupportedReason, validateGnuTimeVersion, validateRecord, writeRecords } from "../benchmarks/metrics.mjs";
```

2. `const result = {…};` の次の `const original = …` から、`console.log("Benchmark report: …")` の直前の `} finally { … }` までを次に置き換える（`result` の定義と `console.log` の行は変えない）:

```js
const original = "# 性能測定\n\n## 表の読み方\n\n説明\n\n## ベンチマーク サマリー\n\n<!-- generated:benchmark-summary -->\n<!-- /generated:benchmark-summary -->\n\n## 記録\n\n過去の結果\n";
const renderSummary = (results) => renderSummaryBlock(summarizeResults(results)).join("\n");
const updateSummary = (document, results) => replaceNamedBlock(document, "benchmark-summary", renderSummary(results).split("\n"), "report.md");
const updated = updateSummary(original, [result]);
assert.ok(updated.indexOf("## ベンチマーク サマリー") > updated.indexOf("## 表の読み方"));
assert.ok(updated.indexOf("## ベンチマーク サマリー") < updated.indexOf("## 記録"));
assert.ok(updated.endsWith("## 記録\n\n過去の結果\n"));
assert.equal((updated.match(/\*\*[0-9.]+\*\*/g) ?? []).length, 2);
assert.match(updated, /C\+\+ \/ Rust（同率）/);
assert.match(updated, /Tsuzuri 相対性能/);
assert.match(updated, /最速を `1\.00000` とし/);
assert.match(updated, /\| 0\.50000 \|/);
assert.match(updated, /Test &lt;CPU&gt;/);
assert.doesNotMatch(updated, /<table|style=|<strong>/);
assert.ok(updated.indexOf("Tsuzuri (ms)") < updated.indexOf("C++ (ms)"));
assert.equal(updateSummary(updated, [result]).trim(), updated.trim());
assert.throws(() => renderSummary([{ ...result, mode: "correctness-smoke" }]));
assert.throws(() => renderSummary([result, { ...result, cpu: "native" }]));
const invalid = structuredClone(result);
invalid.workloads[0].variants.tsuzuri.median_wall_ms = NaN;
assert.throws(() => renderSummary([invalid]));
const owned = structuredClone(result);
owned.workloads[0].name = "std_option_owned";
assert.match(renderSummary([owned]), /\*\*1\.000000\*\*/);
assert.match(renderSummary([owned]), /\| 0\.50000 \|/);
const fastest = structuredClone(result);
fastest.workloads[0].variants.tsuzuri.median_wall_ms = 0.5;
assert.match(renderSummary([fastest]), /\| 1\.00000 \|/);
assert.match(renderSummary([fastest]), /\*\*0\.500000\*\*/);
fastest.workloads[0].variants.tsuzuri.median_wall_ms = 1;
assert.match(renderSummary([fastest]), /\| 1\.00000 \|/);
fastest.workloads[0].variants.tsuzuri.median_wall_ms = 1000000;
assert.match(renderSummary([fastest]), /&lt;0\.00001/);
assert.match(renderTable(["種目", "Tsuzuri (ms)", "C++ (ms)"], [["zero", "1.000000", "0.000000"]], { timingColumns: [1, 2] }).join("\n"), /\| - \|$/);
assert.match(renderTable(["種目", "Tsuzuri (ms)", "C++ (ms)"], [["missing", "-", "1.000000"]], { timingColumns: [1, 2] }).join("\n"), /\| - \|$/);
const tiny = structuredClone(result);
tiny.workloads[0].variants.tsuzuri.median_wall_ms = 0.000000001;
assert.match(renderSummary([tiny]), /1\.000e-9/);
assert.match(renderTable(["<名前>", "時間 (ms)"], [["<script>", "0.100000"]], { timingColumns: [1] }).join("\n"), /&lt;名前&gt; \|[^\n]*\n[^\n]*\n\| `<script>` \|/);
const second = structuredClone(result);
second.measured_at = "2026-09-26T01:00:00.000Z";
second.workloads[0].variants.tsuzuri.median_wall_ms = 6;
assert.match(renderSummary([result, second]), /4\.000000/);
assert.match(renderSummary([result, second]), /2026-09-26T01:00:00.000Z/);
assert.throws(() => renderSummary([result, { ...result, scale: 1 }]));
assert.throws(() => updateSummary(updated.replace("<!-- /generated:benchmark-summary -->", ""), [result]), /missing end marker/);
assert.throws(() => updateSummary("# Test", [result]), /add the generated:benchmark-summary markers/);
const temporary = mkdtempSync(join(tmpdir(), "tsuzuri-report-test-"));
try {
  const path = join(temporary, "report.md");
  const data = join(temporary, "data", "summary.json");
  writeFileSync(path, original);
  assert.throws(() => publishSummary([invalid], { document: path, data }));
  assert.equal(readFileSync(path, "utf8"), original);
  assert.ok(!existsSync(data));
  publishSummary([result], { document: path, data });
  assert.equal(readFileSync(path, "utf8"), updated);
  assert.deepEqual(JSON.parse(readFileSync(data, "utf8")), summarizeResults([result]));
  publishSummary([result], { document: path, data });
  assert.equal(readFileSync(path, "utf8"), updated);
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
```

3. `test("median and summary", …)` の直前の行に次を足す（metrics のテストの補助）:

```js
const renderRecords = (records) => renderMetricsBlock({ schema: "tz-bench-metrics/1", runs: [summarizeRecords(records)] }).join("\n");
```

4. `renderMetricsSummary([…])` の 3 か所（`allocator variants` のテストの `const summary = …`、`summary metadata, units and escaping` の 2 か所）を `renderRecords([…])` に置き換える。`summary metadata, units and escaping` と、その次の `summary markers, publication guards and idempotence` のテストは次に置き換える:

```js
  test("summary metadata, units and escaping", () => {
    const summary = renderRecords([record({ workload: "<example>" }),
      record({ workload: "<example>", language: "c" })]);
    assert.match(summary, /peak_rss \(bytes\) \/ native \/ O0 \/ generic \/ 中央値/);
    assert.match(summary, /`process\/<example>`/);
    assert.ok(summary.indexOf("Tsuzuri execute") < summary.indexOf("C execute"));
    assert.doesNotMatch(summary, /<table|style=|data-fastest|data-tsuzuri-performance/);
    assert.match(renderRecords([record({ metric: "wall_time", samples: Array(9).fill(1.25) })]), /1\.250/);
  });
  test("summary markers, publication guards and idempotence", () => {
    const document = "# Test\n<!-- generated:perf-metrics -->\n<!-- /generated:perf-metrics -->\nlast\n";
    const directory = mkdtempSync(join(tmpdir(), "tsuzuri-metrics-test-"));
    try {
      const path = join(directory, "report.md");
      const data = join(directory, "data", "metrics.json");
      writeFileSync(path, document);
      publishMetrics([[record()]], { document: path, data });
      const updated = readFileSync(path, "utf8");
      assert.ok(updated.endsWith("last\n"));
      assert.deepEqual(JSON.parse(readFileSync(data, "utf8")), { schema: "tz-bench-metrics/1", runs: [summarizeRecords([record()])] });
      publishMetrics([[record()]], { document: path, data });
      assert.equal(readFileSync(path, "utf8"), updated);
      writeFileSync(path, "# Test");
      assert.throws(() => publishMetrics([[record()]], { document: path, data }), /add the generated:perf-metrics markers/);
      writeFileSync(path, document);
      assert.throws(() => publishMetrics([[record({}, { ...context, mode: "quick" })]], { document: path, data }), /must not be published/);
      assert.throws(() => publishMetrics([[record(), record({}, { ...context, host: { ...context.host, cpu: "Different" } })]], { document: path, data }));
      assert.equal(readFileSync(path, "utf8"), document);
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  });
```

### D.11 `scripts/gen-docs.mjs` への Phase 3 の追加（手順 3.3 で生成関数、手順 3.6 で置き場。検証済み: 移行した文書と JSON に対して `--check` が `2 blocks checked, 0 stale`、書き込みが `0 updated`）

1. 手順 3.3: D.2 の import の最後（`import { fileURLToPath } from 'node:url';` の次）に 1 行足す。

```js
import { renderMetricsBlock, renderSummaryBlock, validateMetrics, validateSummary } from '../benchmarks/render.mjs';
```

2. 手順 3.3: `function generator(name)` の前に `readJson` を足し、`generator` の `diagnostic-kinds` の行の次に 2 行、最後の `throw` の行のコメント（`// Phase 3 が …`）を消す。

```js
const readJson = (base, file) => JSON.parse(readFileSync(resolve(base, file), 'utf8'));

function generator(name) {
    if (name === 'diagnostic-kinds') return (context) => diagnosticKindsTable(context.catalog());
    if (name === 'benchmark-summary') return (context) => renderSummaryBlock(validateSummary(readJson(context.root, 'docs/benchmarks-data/summary.json')));
    if (name === 'perf-metrics') return (context) => renderMetricsBlock(validateMetrics(readJson(context.root, 'docs/benchmarks-data/metrics.json')));
    // …（complexity:* の分岐は D.2 のまま）
    throw new Error(`no generator for '${name}'`);
}
```

3. 手順 3.6: `PLACEMENTS` の `// Phase 3 (手順 3.6): …` のコメントの行を、次の 2 件に置き換える。

```js
    { name: 'benchmark-summary', file: 'docs/benchmarks.md' },
    { name: 'perf-metrics', file: 'docs/benchmarks.md' },
```

## E. 文書の文面

### E.1 タグの仕様（手順 2.8）

**`docs/language.md`**（「ドキュメントコメント」の節の、`@deprecated` の 2 段落のあと・`tsuzuri doc <file|directory> -o <outdir> [--json]` の段落の前に 1 段落）:

> 説明の行を `@complexity`・`@since`・`@see` で始めると、宣言に計算量・版・関連する宣言を付けられます（新しい構文ではありません。`@deprecated` と同じ「行頭のタグ」の規則です）。各行の先頭の空白を除いた最初の語がタグそのもので、直後は行末か空白（スペースかタブ）です。`@complexityX` はタグではありません。値は、タグの後ろから行末までの前後の空白を除いたもの（空でもよい）です。`@complexity` と `@since` は最初の 1 行だけが有効で、`@see` は何行でも書けます。`tsuzuri doc` と LSP のホバーは、これらの行も説明として表示します。コンパイラは値を検査せず、診断も出さず、生成される IR も変わりません。標準ライブラリの値は `node scripts/gen-docs.mjs` と `cargo test --test std_tags` が検査し、標準コレクションの計算量の表（[標準コレクションの計算量](#標準コレクションの計算量)）は `@complexity` から作られます。書き方と規則は [構造化タグ](../../_tsuzuri/language-reference/organizing-tsuzuri/documentation-comment.md#構造化タグ) です。

**`documentation-comment.md`**（`### 警告 W1005` の節のあと・`## tsuzuri doc によるドキュメント生成` の前に、新しい節）:

````markdown
## 構造化タグ

説明の行を `@complexity`・`@since`・`@see` で始めると、宣言に計算量・版・関連する宣言を付けられます。新しい構文ではなく、`@deprecated`（[非推奨の印](#非推奨の印)）と同じ、説明の本文の約束です。

```text
/// Returns a new array with a copy of every element.
///
/// @complexity O(n)
/// @see Array.append
def copy :: Copy<'a> => ref ['a] -> ['a]
```

- **タグの行**: 各行の先頭の空白を除いた最初の語がタグそのもので、直後が行末か空白（スペースかタブ）の行です。`@complexityX` はタグではありません。値は、タグの後ろから行末までの前後の空白を除いたものです。説明の本文との間に `///` だけの行を 1 行置くと、`tsuzuri doc` で別の段落になります。順序は「説明、`@complexity`、`@since`、`@see`」です。
- **`@complexity`**: 最初の 1 行だけが有効です。値は `O(式)`、または `amortized O(式)`・`expected O(式)` に `, worst O(式)` を続けたものです。式は `1`、変数 `n`・`m`・`k`、`log`・`log^2`、`+`・`*` で書きます（`n log n`、`n + m`、`n * m`）。`n` は主な入力の要素数、`m` はもう 1 つの入力の要素数、`k` は新しいコレクションを作る関数の結果の要素数です。1 回の呼び出しの最悪の時間で、コールバック・`Eq.eq`・`Ord.lt`・`Hash.hash` は `O(1)` と数えます。`Seq` を返す関数は `Seq` を返すまでの時間です。`expected` と `amortized` は、キーのハッシュが一様に分散するときの値です。
- **`@since`**: 最初の 1 行だけが有効です。値は `0.2.0` のような版で、その宣言が最初に入ったコンパイラの版です。新しい公開関数から付けます。
- **`@see`**: 何行でも書けます。値は `Array.sum_kahan` のような `Module.name`（または `Module`）で、1 行に 1 つです。
- **検査**: コンパイラは値を検査せず、診断も出しません。標準ライブラリの Array・Vec・Map・Set・HashMap・HashSet・List・String の全公開関数に有効な `@complexity` が要り、各ページの「計算量」の表は `node scripts/gen-docs.mjs` がこのタグから作ります。
- **互換性**: タグの追加と変更は説明の変更で、`tsuzuri api-diff` は `non-breaking` と報告します。IR は変わりません。
````

### E.2 README・`option.md`・`docs/language.md` の補完（手順 4.3・4.4）

`README.md`（失敗 6 件）:

1. 「コマンド書式」のコードブロックの `tsuzuri bindgen …` の行の次に `tsuzuri toolchain info` の 1 行。
2. 「主要オプション一覧」の `--deny-warnings` の行の次に 3 行:
   - `` | `--warn implicit-copy` | 配列とリストの暗黙のコピー（`W1006`）を警告として報告します。生成コードは変わりません。 | ``
   - `` | `--debug-output` | WASM の `Debug` 出力の import を有効にします（native は常に出力を書きます）。 | ``
   - `` | `--version` | コンパイラの版（`tsuzuri 0.1.0` の形）を出して終了します。 | ``
3. 「入力とエラー報告の仕様」の箇条の最後に 1 つ: `` - **デバッグ用のツール**: macOS で、タスクを含むデバッグ用オブジェクトを作る `llvm-link` は環境変数 `TSUZURI_LLVM_LINK`（既定 `llvm-link`）、デバッグ実行ファイルを作る `dsymutil` は `TSUZURI_DSYMUTIL`（既定 `dsymutil`）で指定できます。空の値も「設定済み」で、配布物や `PATH` を探しません（[option.md](./_tsuzuri/language-reference/compiler/option.md#環境変数)）。 ``

`_tsuzuri/language-reference/compiler/option.md`（失敗 2 件）: 「どのサブコマンドが受け付けるか」の最初の段落で、`bindgen` の文（`…`--json` だけを受け付けます。`）と `` `repl` はパスを取らず `` の間に 1 文: `` `fetch` はディレクトリと `--json` だけ、`publish` はディレクトリと `--git URL`・`--rev COMMIT`・`--json` だけを受け付けます。 ``

`docs/language.md`（失敗 1 件）: 診断コードの表の `W2002` の行の前に 1 行: `` | `W2001` | キャッシュを使えなかったときのツール警告（ソースの警告ではない。`--json` のときだけ、このコードの JSON オブジェクトになる） | ``

### E.3 計算量のページ（手順 2.9）

**凡例 L1**（`## 計算量` の最初の段落。HashMap・HashSet は L2、String は専用の文）:

> 計算量は 1 回の呼び出しの最悪の時間です。`n` は主な入力の要素数、`m` はもう 1 つの入力の要素数、`k` は結果の要素数です。`Seq` を返す関数は `Seq` を返すまでの時間で、取り出しは 1 要素ごとに $O(1)$ です。規則は [構造化タグ](../organizing-tsuzuri/documentation-comment.md#構造化タグ) にあります。表は `std/` の `@complexity` から `node scripts/gen-docs.mjs` が作ります。

**L2**: L1 の後ろに「平均・償却の値は、キーのハッシュが一様に分散するときのものです。`Hash.hash` や `Eq.eq` のコスト（例えば長い文字列キーの比較）は含みません。」

**マーカーの対**: 本体が空のまま `<!-- generated:complexity:Module -->`・`<!-- /generated:complexity:Module -->` の 2 行を書き、`node scripts/gen-docs.mjs` で本体を作る。

| ページ | 置き換える範囲 | 新しい内容（`## 計算量` の中） |
| --- | --- | --- |
| `array.md` | `## 計算量` の見出しの次から、`## 所有権と借用` の前まで | L1・マーカー（`Array`）・見出し `### 構文と組み込みの操作`・「次の操作は `std/Array.tz` の `def` ではないので、表は手書きです。」・旧の 11 行から std の `def` の行を除いた 7 行（`xs[i]`・`xs.length`・`ref xs[a..b]`・`Array.write` / `swap_in`・`Array.set`・`new [T](k, f)`（`$O(k)$`）・`Array.sort_by`（`$O(n \log n)$`・安定マージソート））。値と備考は旧の行のまま |
| `vec.md` | 同（`## 反復の仕方` の前まで） | L1・マーカー（`Vec`）・見出し `### 組み込みの操作`・「次の操作は `std/Vec.tz` の `def` ではないので、表は手書きです。」・旧の 8 行をそのまま |
| `map.md` | 同（`## 所有権と借用` の前まで） | L1・マーカー（`Map`）・「`insert` と `remove` が $O(n)$ なのは、要素を整列済みの連続バッファに持ち、位置より後ろの要素をずらすためです。既存のキーの値の置換は $O(\log n)$ です。」 |
| `set.md` | 同（`## まとめ` の前まで） | L1・マーカー（`Set`）・「`insert` と `remove` が $O(n)$ なのは、整列済みの連続バッファで位置より後ろの要素をずらすためです。`union`・`intersect`・`difference` は、整列済みの 2 つの列を 1 回ずつ走査します。」 |
| `hashmap.md` | 同（旧の表と、その次の「ハッシュ値が一様なときの計算量です。…」の 1 行。`消費したマップの再利用は…` の段落は残す） | L2・マーカー（`HashMap`） |
| `hashset.md` | 同（旧の表と、その次の「※ ハッシュ値が一様に分散している場合の計算量です。…」の 1 行） | L2・マーカー（`HashSet`） |
| `list.md` | 同（`## 所有権と借用` の前まで） | L1・マーカー（`List`）・見出し `### 構文と組み込みの操作`・「次の操作は `std/List.tz` の `def` ではないので、表は手書きです。」・旧の 10 行から std の `def` の行（`is_empty`・`length`・`fold`）を除き、`xs.length` を足した 9 行（`List.cons`・`match xs with head :: _`・`List.tail`・`xs[i]` の `$O(index + 1)$`・`xs.length`・`List.map` / `map_ref`・`List.fold_ref`・`List.reverse`・`List.to_array`）。値と備考は旧の行のまま |
| `string.md` | 新しい節を `## 注意点` の直前に | 見出し `## 計算量`・「計算量は 1 回の呼び出しの最悪の時間です。`n` は `text`（対象の文字列）のコード単位数、`m` は `needle`・`separator`・`prefix`・`suffix` のコード単位数、`k` は結果のコード単位数です（`join`・`concat` の `n` は部品の数）。規則は [構造化タグ](../organizing-tsuzuri/documentation-comment.md#構造化タグ) にあります。表は `std/String.tz` の `@complexity` から `node scripts/gen-docs.mjs` が作ります。組み込み関数のうち `compare`・`from_utf8`・`is_well_formed`・`to_well_formed` の計算量は、各節の表の `計算量` の列にあります。」・マーカー（`String`） |
| `docs/language.md` | 新しい節 `### 標準コレクションの計算量` を `### 配列・リスト API` の後・`### 演算子` の前に。`### HashMap / HashSet` の旧の表（5 行）は削除 | 凡例（E.3 の L2 に、組み込み操作は各ページの手書きの表にあること、[構造化タグ](../../_tsuzuri/language-reference/organizing-tsuzuri/documentation-comment.md#構造化タグ) へのリンクを足したもの）、続けて 8 モジュールを `**Array**`・`**Vec**`・`**Map**`・`**Set**`・`**HashMap**`・`**HashSet**`・`**List**`・`**String**` の太字の行とマーカーの対で。`### HashMap / HashSet` は、削除した表の位置に「計算量は [標準コレクションの計算量](#標準コレクションの計算量) の表のとおりです。」と書き、次の `※ 上記は…` の行を「※ 平均・償却の値は、ハッシュ値が一様に分散している場合のものです。`Hash.hash` や `Eq.eq` の実行コスト（例えば長い文字列キーの比較に要する計算量）は除外しています。」に直す |

### E.4 `_specs/`（手順 4.1）

`_specs/README.md`（全文）:

````markdown
# `_specs/`（歴史的資料）

**歴史的資料。現行の仕様ではない。** Tsuzuri の初期設計のメモで、2026-10-04〜05 に言語の実装をこれに合わせるときに使った。その後の変更（`try` の戻り値型、シフト量の型、既定の名前空間、`utf8string` の文字コードの説明、配列型の表記 `byte[]`、ラムダの `when` ガード）は反映していない。`_archive/` のファイルは書き換えずにそのまま残す。

現行の仕様は [言語リファレンス](../../_tsuzuri/language-reference/index.md) にある。

| 元のファイル | 現行のページ |
| --- | --- |
| `_archive/error-handling.md` | [try ... with ... finally](../../_tsuzuri/language-reference/exception-handling/try-with-finally.md) |
| `_archive/functions.md` | [関数](../../_tsuzuri/language-reference/values-and-functions/functions.md)・[ラムダ式](../../_tsuzuri/language-reference/values-and-functions/lambda-expressions.md)・[ジェネリック関数](../../_tsuzuri/language-reference/values-and-functions/generics-functions.md) |
| `_archive/literals.md` | [リテラル](../../_tsuzuri/language-reference/literals-and-strings/literals.md) |
| `_archive/namespace-and-module.md` | [名前空間](../../_tsuzuri/language-reference/organizing-tsuzuri/namespaces.md)・[モジュール](../../_tsuzuri/language-reference/organizing-tsuzuri/modules.md)・[using 宣言](../../_tsuzuri/language-reference/organizing-tsuzuri/using-declarations.md) |
| `_archive/operators.md` | [演算子と式](../../_tsuzuri/language-reference/values-and-functions/op-and-expressions.md) |
````

`try-with-finally.md` の「注意点」の 3 つ目の箇条（`- 設計メモの `_specs/error-handling.md` には、…` の 1 行）を次に置き換える:

> - 歴史的資料の `_specs/_archive/error-handling.md` には、関数の戻り値型が自動で `Result` になるという古い説明が残っています。動く形は、このページの例のとおりです。

### E.5 benchmarks の文（手順 3.6。`docs/benchmarks.md` の散文 3 文）

「サマリー表は HTML テーブル形式で記述されています。…」と「表示環境の設定で HTML の背景色や文字色が反映されない場合でも、…」の 2 文（連続する 2 行）を、次の 1 段落（2 文）に置き換える:

> サマリー表は `docs/benchmarks-data/summary.json` から `node scripts/gen-docs.mjs` が作る Markdown の表です。数値は右揃えで、各行の最速の表示値は太字、「観測最速」の列にもその言語名が入ります。表を手で直さず、JSON を直して作り直してください。

「`node benchmarks/metrics.mjs report <dir>... --publish` は、下のマーカーの間だけに本計測の要約を公開します。」を次に置き換える:

> `node benchmarks/metrics.mjs report <dir>... --publish` は、`docs/benchmarks-data/metrics.json` を書き、下のマーカーの間の表をそこから作り直します。

ブロックの本体の文（サマリーの 2 段落と脚注）は D.3 の `renderSummaryBlock` のとおり。HTML の版との差は、1 つ目の段落の「緑色・太字は」が「太字は」になったことだけ。

## F. テストの期待値

### F.1 `scanBlocks`・`replaceBlock`（`tests/gen_docs.mjs`、手順 1.1）

| 入力（行を `\n` でつなぐ） | 期待 |
| --- | --- |
| `intro` / `<!-- generated:complexity:Map -->` / `old` / `<!-- /generated:complexity:Map -->` / `tail` / ```` ``` ```` / `<!-- generated:x -->` / ```` ``` ```` | `[{ name: 'complexity:Map', start: 1, end: 3 }]`（フェンスの中は無視） |
| 上の入力に `replaceBlock(text, blocks[0], ['new 1', 'new 2'])` | 最初の 5 行が `intro` / 開始 / `new 1` / `new 2` / 終了 |
| `<!-- generated:a -->` / `<!-- generated:b -->` | 例外 `nested block 'b' inside 'a'` |
| `<!-- generated:a -->` | `missing end marker for 'a'` |
| `<!-- /generated:a -->` | `end marker without start 'a'` |
| 同じ名前のブロックを 2 つ | `duplicate block 'a'` |
| ` <!-- generated:a -->` / ` <!-- /generated:a -->`（先頭に空白） | `[]` |
| 本文が空のブロック（開始と終了が隣り合う） | `bodyOf` が `[]`、`replaceBlock` が 2 行の間に本体を入れる |

### F.2 ブロックの生成と検査（`tests/gen_docs.mjs`、手順 1.2。偽の context。一時ディレクトリに 2 つの文書を作る）

偽の context: `{ root: <一時ディレクトリ>, hasExplain: () => true, catalog: () => [3 行], std: () => ({ Sample: [2 件] }) }`。カタログ: `{"kind":"b","code":"E1012"}`・`{"kind":"a","code":"E1012"}`・`{"kind":"x","code":"E0001"}`。`std` の 2 件: `{ name: 'a', doc: '@complexity O(1)' }`・`{ name: 'b', doc: 'Text.\n\n@complexity amortized O(1), worst O(n)\n@since 0.2.0\n@see Sample.a' }`。置き場は `PLACEMENTS` を書き換えず、`checkBlocks(context, { placements })`・`writeBlocks(context, { placements })` の `placements` に `[{ name: 'diagnostic-kinds', file: 'docs/diagnostics.md', requires: 'explain' }, { name: 'complexity:Sample', file: 'docs/sample.md' }]` を渡す。一時ディレクトリの 2 つの文書は、最初は本体が空のブロック 1 組（`# D`・`# S` の見出しの次の行から）だけ。表の行の順に同じ一時ディレクトリで続けて検査する（`writeBlocks` の後の状態が、続く行の入力）。

| 操作 | 期待 |
| --- | --- |
| `diagnosticKindsTable(カタログ)` | 4 行の配列（表の期待は下） |
| 本体が空の 2 ブロックに `checkBlocks` | `checked` 2、`stale` 2 件。最初は `stale docs/diagnostics.md (diagnostic-kinds)\n  line 1 of the block\n    expected: \| コード \| kind \|\n    found:    (none)` |
| `writeBlocks` を 2 回 | 1 回目 `{ checked: 2, updated: 2 }`、2 回目 `{ checked: 2, updated: 0 }`。続く `checkBlocks` の `stale` が空 |
| `hasExplain: () => false` で `checkBlocks` | `{ checked: 1, stale: [], skipped: ['diagnostic-kinds'] }` |
| 文書に置き場に無いブロック `typo` | `docs/sample.md: unknown block 'typo'` |
| 文書にブロックが無い | `docs/sample.md: missing block 'complexity:Sample'`。`diagnostic-kinds` なら末尾に ` (G23 step 4.2 adds the markers)` |
| `\r` を含む文書 | `CRLF is not supported` |
| `--list` | `process.execPath` で `scripts/gen-docs.mjs --list` を実行した標準出力が、`PLACEMENTS` の各行 `name<TAB>file`（HEAD は 1 行 `diagnostic-kinds<TAB>_tsuzuri/language-reference/compiler/diagnostics.md`）。終了コード 0 |

`diagnosticKindsTable(カタログ)` の期待（配列の 4 要素が次の 4 行）:

```text
| コード | kind |
| --- | --- |
| `E0001` | `x` |
| `E1012` | `a`, `b` |
```

### F.3 `tests/doc_tags.rs`（手順 2.1）

`tag_notes_follow_the_deprecated_rule`: `deprecation()` を F.3.1 の 10 件（`src/warnings.rs` の `deprecation_note_reads_the_first_tag_line` と同じ）と、`@complexity` の次の表で確かめる。

| text | `deprecation()`（10 件） | `complexity()` |
| --- | --- | --- |
| `"Old.\n@deprecated use new"` | `Some("use new")` | |
| `"@deprecated"` | `Some("")` | |
| `"  @deprecated  spaced  "` | `Some("spaced")` | |
| `"@deprecated\ttabbed"` | `Some("tabbed")` | |
| `"@deprecated a\n@deprecated b"` | `Some("a")` | |
| `"@deprecatedX"` | `None` | |
| `"@deprecatedX\n@deprecated real"` | `Some("real")` | |
| `"text"`・`"see @deprecated here"`・`""` | `None` | |
| `"@complexity O(n)"` | | `Some("O(n)")` |
| `"Text.\n\n@complexity amortized O(1), worst O(n)"` | | `Some("amortized O(1), worst O(n)")` |
| `"@complexity"` | | `Some("")` |
| `"  @complexity\tO(1)  "` | | `Some("O(1)")` |
| `"@complexity O(1)\n@complexity O(n)"` | | `Some("O(1)")` |
| `"@complexityX O(n)"`・`"see @complexity O(n)"` | | `None` |

`complexity_since_and_see_read_their_lines`: `"Text.\n\n@complexity O(n)\n@since 0.2.0\n@see Array.sum\n@see Map"` で `complexity() == Some("O(n)")`、`since() == Some("0.2.0")`、`see() == ["Array.sum", "Map"]`。`"@since 0.2.0\n@since 0.3.0"` の `since()` は `Some("0.2.0")`。`"@see"` の `see()` は `[""]`。`deprecation_is_unchanged`: 上の 10 件と `"@deprecated x\r\n"`・`"a\n  @deprecated\tz"` で `old_deprecation(text)`（D.1）と `documented(text).deprecation().map(str::to_owned)` が一致する。

### F.4 フィクスチャ `tests/fixtures/gen-docs/std-doc/Sample.md`（手順 2.2。`tsuzuri doc` のページの形）

````markdown
# Sample

Namespace: `std`

## `Sample`

```tsuzuri
record Sample {
  x: i64
}
```

## `a`

```tsuzuri
def a :: i64
```

@complexity O(1)

## `b`

```tsuzuri
def b :: i64 -> i64
```

Text.

@complexity amortized O(1), worst O(n)
@since 0.2.0
@see Sample.a
````

`parseStdPage` の期待: `[{ name: 'a', doc: '@complexity O(1)' }, { name: 'b', doc: 'Text.\n\n@complexity amortized O(1), worst O(n)\n@since 0.2.0\n@see Sample.a' }]`（`record` の節は含まれない）。`complexityTable('Sample', [最初の 1 件])` の期待: `['| 関数 | 計算量 |', '| --- | --- |', '| `Sample.a` | $O(1)$ |']`。2 件の期待: `['| 関数 | 計算量 | 版 | 関連 |', '| --- | --- | --- | --- |', '| `Sample.a` | $O(1)$ | - | - |', '| `Sample.b` | 償却 $O(1)$、最悪 $O(n)$ | 0.2.0 | `Sample.a` |']`。

### F.5 タグの文法（`tests/gen_docs.mjs`、手順 2.2）

- `parseComplexity` が非 `null`（13 件）: `O(1)`・`O(log n)`・`O(n)`・`O(n log n)`・`O(n log^2 n)`・`O(n + m)`・`O(n * m)`・`O(k)`・`O(n * m + k)`・`O(n + k)`・`amortized O(1)`・`amortized O(1), worst O(n)`・`expected O(1), worst O(n)`。
- `null`（16 件）: 空文字・`O()`・`O(2)`・`O(n^2)`・`O(n  m)`（空白 2 つ）・`O(n*m)`・`o(n)`・`O(n) `（末尾に空白）・`worst O(n)`・`amortized O(1), worst O(n), worst O(n)`・`expected O(1), O(n)`・`O(log)`・`O(n log)`・`O(x)`・`fast`・`O(n), worst O(n)`。
- `renderComplexity`: `O(n log^2 n)` → `$O(n \log^2 n)$`、`O(n * m + k)` → `$O(n \cdot m + k)$`、`amortized O(1), worst O(n)` → `償却 $O(1)$、最悪 $O(n)$`、`expected O(1), worst O(n)` → `平均 $O(1)$、最悪 $O(n)$`、`O(log n)` → `$O(\log n)$`。
- `complexityTable` の失敗: `{ name: 'c', doc: 'No tag.' }` → `Sample.c: missing @complexity`、`'@complexity fast'` → `Sample.c: invalid @complexity "fast"`、`'@complexity O(1)\n@since 1.0'` → `invalid @since "1.0"`、`'@complexity O(1)\n@see a b'` → `invalid @see "a b"`。
- `tagNotes(text, '@deprecated')` が F.3 の 10 件で、Rust と同じ値（`@deprecated x\r\n` は `['x']`）。

### F.6 リンク検査（`tests/check_links.mjs`、手順 3.1）

一時ディレクトリに `a.md`（`[x](b.md#h1)`・`[y](b.md#nope)`・`[z](missing.md)`・フェンスの中の `[w](b.md#fenced)`）と `b.md`（`# H1`）を作り、`checkLinks({ root, files: ['a.md', 'b.md'], into: ['b.md'] })`。期待: `links` 2（`#h1`・`#nope`。`missing.md` は `into` でないので数えない）、`withFragment` 2、`distinct` 2、`broken` は `['a.md: missing heading b.md#nope']` の 1 件。`into: ['missing.md']` なら `broken` に `a.md: missing file missing.md`。CLI（`process.execPath` で `scripts/check-links.mjs` を、`cwd` をその一時ディレクトリにして実行する）: `--list-headings b.md` の標準出力が `h1` の 1 行で終了コード 0、未知の旗 `--x` が終了コード 2 で stderr に `check-links: unknown option '--x'`。

### F.7 benchmarks の描画（`tests/benchmark_render.mjs`（新規）、手順 3.2。`renderTable(headers, rows, { timingColumns })` の出力。見出し `['種目', 'Tsuzuri (ms)', 'C++ (ms)', 'Rust (ms)']`、`timingColumns: [1, 2, 3]`）

見出し行は `| 種目 | Tsuzuri (ms) | C++ (ms) | Rust (ms) | 観測最速 | Tsuzuri 相対性能 |`、区切り行は `| --- | ---: | ---: | ---: | --- | ---: |`。入力の行と期待の行:

```text
['a','2.000000','1.000000','1.000000']        同率
| `a` | 2.000000 | **1.000000** | **1.000000** | C++ / Rust（同率） | 0.50000 |
['b','1.000000','2.000000','3.000000']        Tsuzuri が最速
| `b` | **1.000000** | 2.000000 | 3.000000 | Tsuzuri | 1.00000 |
['c','-','-','-']                             比べられない
| `c` | - | - | - | 条件差あり | - |
['d','1000000.000000','0.500000','-']         0.00001 未満
| `d` | 1000000.000000 | **0.500000** | - | C++ | &lt;0.00001 |
```

追加の検査: `renderTable(['種目', '仕事量', 'Tsuzuri'], [['<script>', '-', '1.250']])` は、導出列がなく、区切りが `| --- | ---: | ---: |`、行が `` | `<script>` | - | 1.250 | ``。`cell('a|b<c>&')` は `a\|b&lt;c&gt;&amp;`。列数が違うと `Table column count mismatch`、1 列目のバッククォートは `Row name contains a backtick`。`renderSummaryBlock` は、`measured_at: '2026-09-26T00:00:00.000Z'`・`cpu_mode: 'generic'`・`scale: 0.1`・`runs: 1`・環境 `{ cpu: 'Test <CPU>', architecture: 'arm64', tsuzuri_sha256: 'abc', csharp: '.NET 10', javascript: 'v24' }`・行 `['control/example','10','2.000000','1.000000','1.000000']`（見出し `種目`・`仕事量`・`Tsuzuri (ms)`・`C++ (ms)`・`Rust (ms)`、`timing_columns: [2,3,4]`）で、`- CPU: Test &lt;CPU&gt; / arm64 / generic`・`**1種目 / generic / scale 0.1 / 1回の中央値**`・`` | `control/example` | 10 | 2.000000 | **1.000000** | **1.000000** | C++ / Rust（同率） | 0.50000 | `` の各行を含み、`<table`・`style=` を含まない。`renderMetricsBlock` は、run `px01-test`（`dirty: true`、`clang: null`、host の cpu `Test <CPU>`）で、`**run \`px01-test\`** / 2026-10-13T00:00:00.000Z / commit \`aaaaaaaaaaaa\`（未コミットの変更を含む）`・`- Clang: 未記録 / rustc: Rust 1.98 / Node: v24`・`**peak_rss (bytes) / native / O0 / generic / 中央値**`・`` | `process/<example>` | - | 1687552 | 1687552 | `` の各行を含む。`serializeJson({ a: [1, 'x'] })` は `"{\n  \"a\": [\n    1,\n    \"x\"\n  ]\n}\n"`。`validateSummary` は、`rows` の行の長さ違い・時間のセルが `0`・同じ名前の行・`cpu_mode` が `fast` で `invalid summary: …` を投げる。

上の期待値を実行する完全なテスト（検証済み: `node tests/benchmark_render.mjs` が `Render: … passed` を 8 行出して終了コード 0）:

```js
import assert from 'node:assert/strict';
import { cell, renderMetricsBlock, renderSummaryBlock, renderTable, serializeJson, validateMetrics, validateSummary } from '../benchmarks/render.mjs';

const test = (name, check) => { check(); console.log(`Render: ${name} passed`); };
const headers = ['種目', 'Tsuzuri (ms)', 'C++ (ms)', 'Rust (ms)'];
const table = (rows) => renderTable(headers, rows, { timingColumns: [1, 2, 3] });

test('header and separator', () => {
    const lines = table([]);
    assert.equal(lines[0], '| 種目 | Tsuzuri (ms) | C++ (ms) | Rust (ms) | 観測最速 | Tsuzuri 相対性能 |');
    assert.equal(lines[1], '| --- | ---: | ---: | ---: | --- | ---: |');
});
test('tie, winner, no winner, tiny', () => {
    assert.equal(table([['a', '2.000000', '1.000000', '1.000000']])[2], '| `a` | 2.000000 | **1.000000** | **1.000000** | C++ / Rust（同率） | 0.50000 |');
    assert.equal(table([['b', '1.000000', '2.000000', '3.000000']])[2], '| `b` | **1.000000** | 2.000000 | 3.000000 | Tsuzuri | 1.00000 |');
    assert.equal(table([['c', '-', '-', '-']])[2], '| `c` | - | - | - | 条件差あり | - |');
    assert.equal(table([['d', '1000000.000000', '0.500000', '-']])[2], '| `d` | 1000000.000000 | **0.500000** | - | C++ | &lt;0.00001 |');
});
test('plain table and escaping', () => {
    assert.deepEqual(renderTable(['種目', '仕事量', 'Tsuzuri'], [['<script>', '-', '1.250']]), ['| 種目 | 仕事量 | Tsuzuri |', '| --- | ---: | ---: |', '| `<script>` | - | 1.250 |']);
    assert.equal(cell('a|b<c>&'), 'a\\|b&lt;c&gt;&amp;');
    assert.throws(() => renderTable(['x', 'y'], [['a']]), /Table column count mismatch/);
    assert.throws(() => renderTable(['x'], [['a`b']]), /Row name contains a backtick/);
});

const summary = {
    schema: 'tz-bench-summary/1', measured_at: '2026-09-26T00:00:00.000Z', cpu_mode: 'generic', scale: 0.1, runs: 1,
    environment: { cpu: 'Test <CPU>', architecture: 'arm64', tsuzuri_sha256: 'abc', csharp: '.NET 10', javascript: 'v24' },
    headers: ['種目', '仕事量', 'Tsuzuri (ms)', 'C++ (ms)', 'Rust (ms)'], timing_columns: [2, 3, 4], rows: [['control/example', '10', '2.000000', '1.000000', '1.000000']],
};
test('renderSummaryBlock', () => {
    const block = renderSummaryBlock(summary).join('\n');
    for (const expected of ['- CPU: Test &lt;CPU&gt; / arm64 / generic', '**1種目 / generic / scale 0.1 / 1回の中央値**', '| `control/example` | 10 | 2.000000 | **1.000000** | **1.000000** | C++ / Rust（同率） | 0.50000 |']) {
        assert.ok(block.includes(expected), expected);
    }
    assert.doesNotMatch(block, /<table|style=/);
});
const metrics = {
    schema: 'tz-bench-metrics/1',
    runs: [{
        run_id: 'px01-test', date: '2026-10-13T00:00:00.000Z', commit: 'aaaaaaaaaaaa', dirty: true,
        host: { cpu: 'Test <CPU>', arch: 'arm64', cores: 10, os: 'darwin', os_version: '27.0', clang: null, rustc: 'Rust 1.98', node: 'v24', tsuzuri_sha256: 'b'.repeat(64) },
        tables: [{ caption: 'peak_rss (bytes) / native / O0 / generic / 中央値', headers: ['種目', '仕事量', 'Tsuzuri execute', 'C execute'], rows: [['process/<example>', '-', '1687552', '1687552']] }],
    }],
};
test('renderMetricsBlock', () => {
    const block = renderMetricsBlock(metrics).join('\n');
    for (const expected of ['**run `px01-test`** / 2026-10-13T00:00:00.000Z / commit `aaaaaaaaaaaa`（未コミットの変更を含む）', '- Clang: 未記録 / rustc: Rust 1.98 / Node: v24', '**peak_rss (bytes) / native / O0 / generic / 中央値**', '| `process/<example>` | - | 1687552 | 1687552 |']) {
        assert.ok(block.includes(expected), expected);
    }
});
test('serializeJson', () => {
    assert.equal(serializeJson({ a: [1, 'x'] }), '{\n  "a": [\n    1,\n    "x"\n  ]\n}\n');
});
test('validateSummary rejects', () => {
    assert.equal(validateSummary(structuredClone(summary)).rows.length, 1);
    const mutate = (change) => { const copy = structuredClone(summary); change(copy); return () => validateSummary(copy); };
    assert.throws(mutate((value) => { value.rows[0].pop(); }), /invalid summary/);
    assert.throws(mutate((value) => { value.rows[0][2] = '0'; }), /invalid summary/);
    assert.throws(mutate((value) => { value.rows.push([...value.rows[0]]); }), /invalid summary/);
    assert.throws(mutate((value) => { value.cpu_mode = 'fast'; }), /invalid summary/);
});
test('validateMetrics rejects', () => {
    assert.equal(validateMetrics(structuredClone(metrics)).runs.length, 1);
    const mutate = (change) => { const copy = structuredClone(metrics); change(copy); return () => validateMetrics(copy); };
    assert.throws(mutate((value) => { value.runs[0].commit = 'xyz'; }), /invalid metrics/);
    assert.throws(mutate((value) => { value.runs[0].tables[0].rows[0].pop(); }), /invalid metrics/);
    assert.throws(mutate((value) => { value.runs = []; }), /invalid metrics/);
    assert.throws(mutate((value) => { value.runs[0].host.cores = 1.5; }), /invalid metrics/);
});
```

### F.8 ドリフト検査（`tests/check_cli_docs.mjs`、手順 4.2〜4.5）

- `extractCli`: `--help` の断片 `"  tsuzuri check x [--json]\n  tsuzuri [build] y [options]\n    --wasm-feature <name>  …TSUZURI_SYSROOT …\n  -O0, -O1\n"` から `flags ['--json', '--wasm-feature']`・`commands ['build', 'check']`・`variables ['TSUZURI_SYSROOT']`。`--sysroot` は `EXEMPT_FLAGS` で免除される。
- `checkCli`: README の本文に `--json` が無ければ `README.md lacks flag --json`。`tsuzuri build` が `tsuzuri buildx` だけなら `README.md lacks command build`（直後が識別子の文字）。
- `documentedCodes`: 行 `| \`E1001\`–\`E1003\` | … |` → `E1001 E1002 E1003`、`| \`E2001\` / \`E2002\` | … |` → 2 つ、`| 項目 | 説明 |` → なし。
- `checkCodes`: 発行 `['E1001','W2001']`、文書の表が `E1001` と `E9999` なら `W2001 is emitted but missing from PATH` と `E9999 is in PATH but no source emits it`。
- `scanEmittedCodes`: `Diagnostic::new("E1001", …)`・`"{{\"severity\":\"warning\",\"code\":\"W2001\"}}"`・`Edition::parse("E2026")` から `['E1001', 'W2001']`（`E2026` は `(` の直後でも `,` が続かないので拾わない）。
- `versionMentions`: `"Tsuzuri 0.1.0 と tsuzuri v0.2.1 と Tsuzuri 1.0"` → `['0.1.0', '0.2.1']`。
- 終了コード: `--only nope` は `check-cli-docs: unknown check 'nope'` で 2、コンパイラが無ければ 2、1 つでも `FAIL` があれば 1。
