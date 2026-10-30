// G35: the Markdown rendering of the benchmark tables (`benchmarks/render.mjs`).
//
// usage: node tests/benchmark_render.mjs

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
