// The benchmark tables of `docs/benchmarks.md` (G35): Markdown rendered from
// `docs/benchmarks-data/summary.json` (`tz-bench-summary/1`) and `metrics.json` (`tz-bench-metrics/1`).
// `benchmarks/report.mjs` and `benchmarks/metrics.mjs` write the JSON; `scripts/gen-docs.mjs` renders the blocks.

const HTML = { '&': '&amp;', '<': '&lt;', '>': '&gt;' };
/** A cell of a Markdown table: HTML-escaped, `|` escaped, one line. */
export const cell = (value) => String(value).replace(/[&<>]/g, (char) => HTML[char]).replaceAll('|', '\\|').replace(/\r?\n/g, ' ');
const text = (value) => String(value).replace(/[&<>]/g, (char) => HTML[char]);
export const serializeJson = (value) => `${JSON.stringify(value, null, 2)}\n`;

/**
 * A Markdown table. With `timingColumns`, the fastest shown value of each row is bold and two derived columns follow:
 * the fastest implementation (`観測最速`) and, with a Tsuzuri column, Tsuzuri's relative performance.
 */
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

/** The body of the `benchmark-summary` block of `docs/benchmarks.md`. */
export function renderSummaryBlock(summary) {
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

/** The body of the `perf-metrics` block of `docs/benchmarks.md`. */
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
