// Checks the rule tables of `spec/rules/`, the conformance tests that name the rules, and the rule
// markers of the documents (G33 §7). No compiler is needed.
//
// usage: node scripts/check-spec.mjs [--root DIR] [--write] [--json] [--coverage | --coverage-markdown]
//            [--require-markers] [--link PAGE [HEADING]] [--strip-markers PATH]
// Exit status: 0 without errors, 1 with errors (or a --link that names no page or heading), 2 for a
// usage error or an unreadable input.

import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { AREAS, RULE_ID } from './conformance/directives.mjs';
import { discoverTests } from './conformance/discover.mjs';
import {
    allowedDoc,
    docFiles,
    docTarget,
    headingsOf,
    linkFor,
    markersIn,
    MAX_RULE_LENGTH,
    readRules,
    replaceCell,
    stripMarkers,
} from './conformance/spec.mjs';

const repository = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const COVERAGE_PAGE = '_tsuzuri/language-reference/compiler/conformance.md';
const BLOCK_START = '<!-- generated:spec-coverage -->';
const BLOCK_END = '<!-- /generated:spec-coverage -->';

function compare(a, b) {
    return a < b ? -1 : a > b ? 1 : 0;
}

/** Whether `rule` is a sentence of §2. */
function sentenceProblem(rule) {
    if (rule.includes('\n')) return 'the rule spans lines';
    if ([...rule].length > MAX_RULE_LENGTH) return `the rule is longer than ${MAX_RULE_LENGTH} characters`;
    if (!rule.endsWith('.')) return 'the rule does not end with a period';
    if (/\bSHOULD\b/.test(rule)) return 'the rule says SHOULD; use MUST, MUST NOT or MAY';
    if (!/\b(MUST|MAY)\b/.test(rule)) return 'the rule says none of MUST, MUST NOT and MAY';
    return null;
}

/** The coverage table of `rules`, `tests` and the marked IDs. */
function coverageOf(rules, tests, marked) {
    const areas = AREAS.map(area => {
        const own = rules.filter(rule => rule.area === area);
        const active = own.filter(rule => rule.status === 'active');
        return {
            area,
            active: active.length,
            retired: own.filter(rule => rule.status === 'retired').length,
            tested: active.filter(rule => rule.testStems.length).length,
            tests: tests.filter(test => test.parsed.rules[0]?.slice(0, 3) === area).length,
            marked: active.filter(rule => marked.has(rule.id)).length,
        };
    });
    const total = { active: 0, retired: 0, tested: 0, tests: 0, marked: 0 };
    for (const row of areas) for (const key of Object.keys(total)) total[key] += row[key];
    return { schema: 'tz-spec-coverage/1', areas, total };
}

const COLUMNS = ['Active', 'Retired', 'Tested', 'Tests', 'Marked'];

function coverageRows(coverage) {
    return [
        ...coverage.areas.map(row => [row.area, row.active, row.retired, row.tested, row.tests, row.marked]),
        ['Total', coverage.total.active, coverage.total.retired, coverage.total.tested, coverage.total.tests, coverage.total.marked],
    ];
}

export function coverageMarkdown(coverage) {
    const lines = ['| Area | Active | Retired | Tested | Tests | Marked |', '| --- | ---: | ---: | ---: | ---: | ---: |'];
    for (const row of coverageRows(coverage)) lines.push(`| ${row.join(' | ')} |`);
    return `${lines.join('\n')}\n`;
}

export function coverageText(coverage) {
    const rows = [['Area', ...COLUMNS], ...coverageRows(coverage).map(row => row.map(String))];
    const widths = rows[0].map((_, column) => Math.max(...rows.map(row => row[column].length)));
    return `${rows
        .map(row => row.map((cell, column) => (column === 0 ? cell.padEnd(widths[0]) : cell.padStart(widths[column]))).join('  '))
        .join('\n')}\n`;
}

/** The text of the coverage page with its generated block replaced, or `null` without markers. */
function withBlock(text, block) {
    const start = text.indexOf(BLOCK_START);
    const end = text.indexOf(BLOCK_END);
    if (start < 0 || end < start) return null;
    return `${text.slice(0, start + BLOCK_START.length)}\n${block}${text.slice(end)}`;
}

/** Every check of §7 over the tree at `root`: `{ errors, coverage, rules, tests, missing }`. */
export function checkAll(root, options = {}) {
    const errors = [];
    const error = (path, line, name, message) => errors.push({ path, line, name, message });
    const { rules, errors: ruleErrors, missing } = readRules(root);
    if (missing) return { errors, coverage: null, rules, tests: [], missing: true };
    errors.push(...ruleErrors);
    const byId = new Map();
    const lastInFile = new Map();
    for (const rule of rules) {
        rule.testStems = [];
        if (!RULE_ID.test(rule.id)) {
            // A malformed ID is reported once; no test can name it.
            error(rule.file, rule.line, 'rule_id', `'${rule.id}' is not a rule ID`);
            rule.invalid = true;
            continue;
        }
        if (rule.id.slice(0, 3) !== rule.area) {
            error(rule.file, rule.line, 'rule_id', `'${rule.id}' is not in the area ${rule.area}`);
            rule.invalid = true;
        }
        if (byId.has(rule.id)) {
            error(rule.file, rule.line, 'rule_duplicate', `'${rule.id}' is also ${byId.get(rule.id).file}:${byId.get(rule.id).line}`);
            rule.invalid = true;
        } else if (!rule.invalid) {
            byId.set(rule.id, rule);
        }
        if (rule.invalid) continue;
        const last = lastInFile.get(rule.file);
        if (last !== undefined && compare(rule.id, last) < 0) error(rule.file, rule.line, 'rule_order', `'${rule.id}' comes after '${last}'`);
        lastInFile.set(rule.file, rule.id);
        if (!['active', 'retired'].includes(rule.status)) {
            error(rule.file, rule.line, 'rule_status', `the status '${rule.status}' is neither active nor retired`);
            continue;
        }
        if (rule.status === 'active') {
            const problem = sentenceProblem(rule.rule);
            if (problem) error(rule.file, rule.line, 'rule_sentence', `${rule.id}: ${problem}`);
        }
        if (rule.status === 'retired' && rule.doc === '-') continue;
        const target = docTarget(rule.doc);
        if (!target) {
            error(rule.file, rule.line, 'doc_link', `${rule.id}: the Doc cell is not one link`);
            continue;
        }
        if (!allowedDoc(target.path)) {
            error(rule.file, rule.line, 'doc_link', `${rule.id}: '${target.path}' is not a document a rule may cite`);
            continue;
        }
        const page = join(root, target.path);
        if (!existsSync(page)) {
            error(rule.file, rule.line, 'doc_link', `${rule.id}: no file '${target.path}'`);
            continue;
        }
        rule.docPath = target.path;
        if (target.fragment !== undefined && !headingsOf(readFileSync(page, 'utf8')).has(decodeURIComponent(target.fragment))) {
            error(rule.file, rule.line, 'doc_link', `${rule.id}: no heading '#${target.fragment}' in '${target.path}'`);
        }
    }
    const { tests, errors: testErrors } = discoverTests(root);
    for (const failure of testErrors) {
        const name = failure.name === 'test_name' || failure.name === 'unexpected_entry' ? 'test_name' : 'test_directive';
        error(failure.path, failure.line, name, `${failure.name}: ${failure.message}`);
    }
    for (const test of tests) {
        const stemId = test.stem.slice(0, 7);
        test.parsed.rules.forEach((id, index) => {
            const line = test.parsed.ruleLines[index];
            const rule = byId.get(id);
            if (!rule) error(test.relativeMain, line, 'test_rule', `'${id}' is not in the rule tables`);
            else {
                if (rule.status === 'retired') error(test.relativeMain, line, 'test_rule', `'${id}' is retired`);
                if (!rule.testStems.includes(test.stem)) rule.testStems.push(test.stem);
            }
            if (index === 0 && id !== stemId) error(test.relativeMain, line, 'test_rule', `the first rule '${id}' is not the test's '${stemId}'`);
        });
    }
    const rewrites = new Map();
    for (const rule of rules) {
        if (rule.invalid) continue;
        rule.testStems.sort(compare);
        const expected = rule.testStems.length ? rule.testStems.join(', ') : '-';
        if (rule.status === 'active' && !rule.testStems.length) error(rule.file, rule.line, 'rule_untested', `${rule.id} has no test`);
        if (rule.status === 'retired' && rule.testStems.length) {
            error(rule.file, rule.line, 'rule_retired_tested', `${rule.id} is retired but tested by ${expected}`);
        }
        if (rule.tests !== expected) {
            error(rule.file, rule.line, 'tests_cell', `${rule.id}: the Tests cell must be '${expected}'`);
            const list = rewrites.get(rule.file) ?? [];
            list.push({ line: rule.line, text: replaceCell(rule.text, 3, expected) });
            rewrites.set(rule.file, list);
        }
    }
    const marked = new Set();
    for (const file of docFiles(root)) {
        for (const marker of markersIn(readFileSync(join(root, file), 'utf8'))) {
            const rule = byId.get(marker.id);
            if (!rule || rule.status !== 'active') {
                error(file, marker.line, 'marker_rule', `'${marker.id}' is ${rule ? 'retired' : 'not in the rule tables'}`);
            } else if (rule.docPath === file) {
                marked.add(marker.id);
            }
        }
    }
    if (options.requireMarkers) {
        for (const rule of rules) {
            if (rule.status === 'active' && rule.docPath && !marked.has(rule.id)) {
                error(rule.file, rule.line, 'marker_missing', `${rule.id} has no marker in ${rule.docPath}`);
            }
        }
    }
    const coverage = coverageOf(rules, tests, marked);
    let block = null;
    if (existsSync(join(root, COVERAGE_PAGE))) {
        const text = readFileSync(join(root, COVERAGE_PAGE), 'utf8');
        const wanted = withBlock(text, coverageMarkdown(coverage));
        if (wanted !== null && wanted !== text) {
            error(COVERAGE_PAGE, text.slice(0, text.indexOf(BLOCK_START)).split('\n').length, 'coverage_block', 'the coverage table is out of date; run check-spec --write');
            block = wanted;
        }
    }
    errors.sort((a, b) => compare(a.path, b.path) || a.line - b.line || compare(a.name, b.name));
    return { errors, coverage, rules, tests, missing: false, rewrites, block };
}

/** Rewrites the Tests cells and the coverage block that `result` found out of date. */
function write(root, result) {
    for (const [file, list] of result.rewrites) {
        const lines = readFileSync(join(root, file), 'utf8').split('\n');
        for (const { line, text } of list) lines[line - 1] = text;
        writeFileSync(join(root, file), lines.join('\n'));
    }
    if (result.block !== null) writeFileSync(join(root, COVERAGE_PAGE), result.block);
}

function usage(message) {
    process.stderr.write(`check-spec: ${message}\n`);
    process.exitCode = 2;
}

export function main(argv) {
    const options = { root: repository };
    for (let index = 0; index < argv.length; index++) {
        const argument = argv[index];
        if (argument === '--root' || argument === '--strip-markers') {
            if (index + 1 >= argv.length) return usage(`${argument} needs a value`);
            options[argument === '--root' ? 'root' : 'strip'] = argv[++index];
        } else if (argument === '--link') {
            const rest = argv.slice(index + 1).filter(value => !value.startsWith('--'));
            if (!rest.length) return usage('--link needs a page');
            options.link = rest.slice(0, 2);
            index += options.link.length;
        } else if (['--write', '--json', '--coverage', '--coverage-markdown', '--require-markers'].includes(argument)) {
            options[argument.slice(2)] = true;
        } else {
            return usage(`unknown argument '${argument}'`);
        }
    }
    const root = resolve(options.root);
    if (options.link) {
        const { link, error } = linkFor(root, options.link[0], options.link[1]);
        if (error) {
            process.stderr.write(`check-spec: ${error}\n`);
            process.exitCode = 1;
        } else {
            process.stdout.write(`${link}\n`);
        }
        return;
    }
    if (options.strip) {
        const path = resolve(root, options.strip);
        if (!existsSync(path)) return usage(`no file ${options.strip}`);
        process.stdout.write(stripMarkers(readFileSync(path, 'utf8')));
        return;
    }
    let result = checkAll(root, { requireMarkers: options['require-markers'] });
    if (result.missing) return usage('spec/rules/ not found');
    if (options.write && (result.rewrites.size || result.block !== null)) {
        write(root, result);
        result = checkAll(root, { requireMarkers: options['require-markers'] });
    }
    if (options['coverage-markdown']) {
        process.stdout.write(coverageMarkdown(result.coverage));
        process.exitCode = result.errors.length ? 1 : 0;
        return;
    }
    if (options.json) {
        const out = result.errors.map(({ path, line, name, message }) => JSON.stringify({ path, line, name, message }));
        if (options.coverage) out.push(JSON.stringify(result.coverage));
        if (out.length) process.stdout.write(`${out.join('\n')}\n`);
    } else {
        for (const { path, line, name, message } of result.errors) process.stderr.write(`${path}:${line}: ${name}: ${message}\n`);
        const active = result.rules.filter(rule => rule.status === 'active').length;
        const retired = result.rules.filter(rule => rule.status === 'retired').length;
        process.stdout.write(
            result.errors.length
                ? `check-spec: ${result.errors.length} errors\n`
                : `check-spec: ok (${active} active, ${retired} retired, ${result.tests.length} tests)\n`,
        );
        if (options.coverage) process.stdout.write(coverageText(result.coverage));
    }
    process.exitCode = result.errors.length ? 1 : 0;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
    main(process.argv.slice(2));
}
