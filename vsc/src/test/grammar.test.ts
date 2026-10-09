import * as assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import * as path from 'node:path';
import { test } from 'node:test';
import { Registry, parseRawGrammar, INITIAL } from 'vscode-textmate';
import { loadWASM, OnigScanner, OnigString } from 'vscode-oniguruma';

test('TextMate grammar tokenizes nested comments, type variables, literals, and lambdas', async () => {
	const { grammar, registry } = await loadGrammar();
	try {
		const first = grammar.tokenizeLine('/* outer /* nested */', INITIAL);
		const second = grammar.tokenizeLine('still outer */ def identity :: \'a -> \'a = \\value -> value', first.ruleStack);
		assert.ok(second.tokens[0].scopes.includes('comment.block.tsuzuri'));
		assert.ok(second.tokens.some(token => token.scopes.includes('storage.type.generic.tsuzuri')));
		assert.ok(second.tokens.some(token => token.scopes.includes('keyword.operator.tsuzuri')));
		const literals = grammar.tokenizeLine('let text = u8"a\\n"; let ch = \'A\'; let number = 42i64', INITIAL);
		for (const scope of ['string.quoted.double.tsuzuri', 'string.quoted.single.tsuzuri', 'constant.numeric.tsuzuri', 'constant.character.escape.tsuzuri']) {
			assert.ok(literals.tokens.some(token => token.scopes.includes(scope)), scope);
		}
		const line = 'let a = u8$"é{s}{{x}}{count + 1:>8.2}{f (1, [2])} {$"[{n}]"}"; let b = 7';
		const interpolated = grammar.tokenizeLine(line, INITIAL);
		const at = (text: string, from = 0) => {
			const start = line.indexOf(text, from);
			const token = interpolated.tokens.find(candidate => candidate.startIndex <= start && start < candidate.endIndex);
			assert.ok(token, text);
			return token.scopes;
		};
		assert.ok(at('u8$"').includes('string.interpolated.tsuzuri'));
		assert.ok(at('{{').includes('constant.character.escape.tsuzuri'));
		assert.ok(at('count').includes('meta.embedded.interpolation.tsuzuri'));
		assert.ok(at('1:').includes('constant.numeric.tsuzuri'));
		assert.ok(at('>8.2').includes('constant.other.format-spec.tsuzuri'));
		assert.ok(at('2])').includes('meta.embedded.interpolation.tsuzuri'));
		assert.ok(!at('2])').includes('constant.other.format-spec.tsuzuri'));
		assert.ok(at('n}').filter(scope => scope === 'string.interpolated.tsuzuri').length === 2);
		assert.ok(!at('7').includes('string.interpolated.tsuzuri'));
		assert.ok(at('7').includes('constant.numeric.tsuzuri'));
	} finally { registry.dispose(); }
});

test('TextMate grammar tokenizes namespaces, using, error handling, attributes, and literal suffixes', async () => {
	const { grammar, registry } = await loadGrammar();
	try {
		const scopes = (line: string, text: string) => {
			const start = line.indexOf(text);
			const token = grammar.tokenizeLine(line, INITIAL).tokens.find(candidate => candidate.startIndex <= start && start < candidate.endIndex);
			assert.ok(token, text);
			return token.scopes;
		};
		for (const line of ['namespace Sample::Features', 'using Sample::Features // shared shapes']) {
			assert.ok(scopes(line, line.split(' ')[0]).includes('keyword.other.namespace.tsuzuri'), line);
			for (const segment of ['Sample', 'Features']) {
				assert.ok(scopes(line, segment).includes('entity.name.namespace.tsuzuri'), `${line}: ${segment}`);
			}
			assert.ok(scopes(line, '::').includes('keyword.operator.tsuzuri'), line);
		}
		const qualified = 'let area = Sample::Features::Shape.area (lower::Shape.Rect (3.0, 4.0))';
		for (const segment of ['Sample', 'Features', 'Shape.area', 'lower']) {
			assert.ok(scopes(qualified, segment).includes('entity.name.namespace.tsuzuri'), segment);
		}
		assert.ok(scopes(qualified, '::').includes('keyword.operator.tsuzuri'));
		assert.ok(!scopes('let ys = x::xs', 'x::').includes('entity.name.namespace.tsuzuri'));
		assert.ok(scopes('def area::Sample::Shape -> f64', 'area').includes('entity.name.function.tsuzuri'));
		assert.ok(scopes('using Sample // shared', 'shared').includes('comment.line.double-slash.tsuzuri'));
		assert.ok(!scopes('let using = namespace + 1', 'using').includes('keyword.other.namespace.tsuzuri'));
		assert.ok(!scopes('    using resource', 'using').includes('keyword.other.namespace.tsuzuri'));
		for (const word of ['try', 'with', 'is', 'finally']) {
			assert.ok(scopes('try x with | e is OverflowException -> e finally done', word).includes('keyword.control.tsuzuri'), word);
		}
		for (const attribute of ['@checked', '@literal']) {
			assert.ok(scopes(`${attribute} def x :: i32 = 1`, attribute).includes('storage.modifier.attribute.tsuzuri'), attribute);
		}
		const literals = 'let a = [86y; 86uy; 86s; 86us; 86u; 86l; 86ul; 86L; 86UL; 99I; 4.14hf; 4.14f; 4.14F; 0.5hm; 0.5m; 0.5M]';
		for (const literal of literals.slice(9, -1).split('; ')) {
			const token = grammar.tokenizeLine(literals, INITIAL).tokens.find(candidate => literals.slice(candidate.startIndex, candidate.endIndex) === literal);
			assert.ok(token?.scopes.includes('constant.numeric.tsuzuri'), literal);
		}
		assert.ok(scopes('let b = \'a\'B', '\'a\'B').includes('string.quoted.single.tsuzuri'));
		assert.ok(scopes('let s = "test"B', '"test"B').includes('string.quoted.double.tsuzuri'));
		for (const operator of ['&&&', '|||', '^^^', '~~~', '<<<', '>>>', '**']) {
			assert.ok(scopes(`let v = a ${operator} b`, operator).includes('keyword.operator.tsuzuri'), operator);
		}
	} finally { registry.dispose(); }
});

let oniguruma: Promise<void> | undefined;

test('TextMate grammar treats a first-line shebang as a comment', async () => {
	const { grammar, registry } = await loadGrammar();
	try {
		const shebang = '#!/usr/bin/env tsuzuri script';
		const first = grammar.tokenizeLine(shebang, INITIAL);
		assert.ok(first.tokens[0].scopes.includes('comment.line.shebang.tsuzuri'));
		assert.equal(first.tokens[0].endIndex, shebang.length);
		const later = grammar.tokenizeLine(shebang, grammar.tokenizeLine('let a = 1', INITIAL).ruleStack);
		assert.ok(!later.tokens.some(token => token.scopes.includes('comment.line.shebang.tsuzuri')));
	} finally { registry.dispose(); }
});

test('member-position keywords stay function names', async () => {
	const { grammar, registry } = await loadGrammar();
	try {
		for (const member of ['Async.yield', 'Set.union', 'Rc.new']) {
			const line = `do! ${member} ()`;
			const start = line.indexOf('.') + 1;
			const token = grammar.tokenizeLine(line, INITIAL).tokens.find(candidate => candidate.startIndex === start);
			assert.ok(token, member);
			assert.ok(token.scopes.includes('entity.name.function.tsuzuri'), member);
			assert.ok(!token.scopes.includes('keyword.control.tsuzuri'), member);
		}
		assert.ok(grammar.tokenizeLine('yield 1', INITIAL).tokens[0].scopes.includes('keyword.control.tsuzuri'));
	} finally { registry.dispose(); }
});

test('a builder alias is an attribute and a module-like name', async () => {
	const { grammar, registry } = await loadGrammar();
	try {
		// The formatter keeps one space, but unformatted declarations are valid too.
		for (const line of ['@alias async', '@alias    async', '@alias\tasync', '@alias \t async // note']) {
			const tokens = grammar.tokenizeLine(line, INITIAL).tokens;
			const at = (offset: number) => {
				const token = tokens.find(candidate => candidate.startIndex <= offset && offset < candidate.endIndex);
				assert.ok(token, `${JSON.stringify(line)} offset ${offset}`);
				return token.scopes;
			};
			assert.ok(at(0).includes('storage.modifier.attribute.tsuzuri'), JSON.stringify(line));
			assert.ok(at(line.indexOf('async')).includes('entity.name.namespace.tsuzuri'), JSON.stringify(line));
		}
		// Without a name yet, the attribute alone is still highlighted.
		const bare = grammar.tokenizeLine('@alias', INITIAL).tokens;
		assert.ok(bare[0].scopes.includes('storage.modifier.attribute.tsuzuri'));
	} finally { registry.dispose(); }
});

async function loadGrammar() {
	const root = path.resolve(__dirname, '../..');
	oniguruma ??= readFile(require.resolve('vscode-oniguruma/release/onig.wasm')).then(data => loadWASM(data));
	await oniguruma;
	const registry = new Registry({
		onigLib: Promise.resolve({ createOnigScanner: patterns => new OnigScanner(patterns), createOnigString: text => new OnigString(text) }),
		loadGrammar: async () => parseRawGrammar(await readFile(path.join(root, 'syntaxes/tsuzuri.tmLanguage.json'), 'utf8'), 'tsuzuri.json'),
	});
	const grammar = await registry.loadGrammar('source.tsuzuri');
	assert.ok(grammar);
	return { grammar, registry };
}
