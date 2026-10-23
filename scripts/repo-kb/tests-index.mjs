import { join, posix } from 'node:path';
import { scanRust } from './rust.mjs';
import { cmp, readText } from './text.mjs';

export function parseSuites(text) {
    const body = /(?:^|\n)const suites = \{([\s\S]*?)^};$/m.exec(text)?.[1] ?? '';
    const matches = [...body.matchAll(/^  ([A-Za-z_][A-Za-z0-9_]*): (?:\{|\(\(\) => \{)/gm)];
    return matches.map((m, i) => {
        const source = body.slice(m.index, matches[i + 1]?.index ?? body.length);
        const wrapped = m[0].includes('(() =>');
        const data = wrapped ? /^    return \{([\s\S]*?)^    \};/m.exec(source)?.[1] ?? '' : source;
        const indent = wrapped ? '      ' : '    ';
        return { name: m[1], input: new RegExp(`^${indent}input: "([^"]+)"`, 'm').exec(data)?.[1] ?? '',
            traps: new RegExp(`^${indent}traps:`, 'm').test(data),
            inspect: new RegExp(`^${indent}inspect\\(`, 'm').test(data),
            cases: new RegExp(`^${indent}cases:`, 'm').test(data) };
    });
}
export function indexTests(ctx) {
    const read = path => ctx.texts?.get(path) ?? readText(join(ctx.root, path));
    const rs = ctx.files.filter(p => /^tests\/[^/]+\.rs$/.test(p));
    const mjs = ctx.files.filter(p => /^tests\/[^/]+\.mjs$/.test(p));
    const contents = new Map(mjs.map(path => [path, read(path)]));
    const result = rs.map(path => {
        const code = scanRust(read(path)).code;
        const tests = [...code.matchAll(/^[ \t]*#\[test\][\s\S]*?\bfn\s+([A-Za-z_][A-Za-z0-9_]*)/gm)];
        const name = posix.basename(path, '.rs');
        return { kind: 'rust_test', path, name, count: tests.length, names: tests.map(m => m[1]).slice(0, 300), command: `cargo test --locked --test ${name}` };
    });
    for (const path of mjs) {
        const name = posix.basename(path, '.mjs');
        const compiler_arg = !['tests/repo_kb.mjs', 'tests/wasm_heap.mjs'].includes(path)
            && (/argv\[2\]/.test(contents.get(path)) || /\bconst compiler\s*=\s*resolve\((?:process\.argv|args)\.find\(/.test(contents.get(path)));
        const helper = path === 'tests/os-wasi-host.mjs' || [...contents].some(([other, text]) => other !== path
            && [...text.matchAll(/\bfrom\s+["']\.\/([^"']+\.mjs)["']/g)].some(m => m[1] === `${name}.mjs`));
        result.push({ kind: 'mjs_test', path, name, compiler_arg, helper,
            command: helper || path === 'tests/display_parse_exhaustive.mjs' ? '' : `node ${path}${compiler_arg ? ' target/release/tsuzuri' : ''}` });
    }
    const path = 'tests/features.mjs';
    if (contents.has(path)) {
        for (const suite of parseSuites(contents.get(path))) {
            const fixture = `tests/fixtures/${suite.input || suite.name}`;
            const fixture_exists = ctx.fileSet.has(fixture) || ctx.files.some(p => p.startsWith(`${fixture}/`));
            if (!fixture_exists) ctx.warnings.push(`suite_fixture_missing ${suite.name}: ${fixture}`);
            result.push({ kind: 'suite', path, name: suite.name, input: suite.input, fixture, fixture_exists,
                traps: suite.traps, inspect: suite.inspect, cases: suite.cases,
                command: `node tests/features.mjs target/release/tsuzuri ${suite.name}` });
        }
    }
    const order = { rust_test: 0, mjs_test: 1, suite: 2 };
    return result.sort((a, b) => order[a.kind] - order[b.kind] || cmp(a.path, b.path) || cmp(a.name, b.name));
}
