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
		for (const line of ['namespace Sample.Features', 'using Sample.Features // shared shapes']) {
			assert.ok(scopes(line, line.split(' ')[0]).includes('keyword.other.namespace.tsuzuri'), line);
			assert.ok(scopes(line, 'Sample.Features').includes('entity.name.namespace.tsuzuri'), line);
		}
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
