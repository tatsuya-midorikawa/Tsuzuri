import * as assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import * as path from 'node:path';
import { test } from 'node:test';
import { Registry, parseRawGrammar, INITIAL } from 'vscode-textmate';
import { loadWASM, OnigScanner, OnigString } from 'vscode-oniguruma';

test('TextMate grammar tokenizes nested comments, type variables, literals, and lambdas', async () => {
	const root = path.resolve(__dirname, '../..');
	await loadWASM(await readFile(require.resolve('vscode-oniguruma/release/onig.wasm')));
	const registry = new Registry({
		onigLib: Promise.resolve({ createOnigScanner: patterns => new OnigScanner(patterns), createOnigString: text => new OnigString(text) }),
		loadGrammar: async () => parseRawGrammar(await readFile(path.join(root, 'syntaxes/tsuzuri.tmLanguage.json'), 'utf8'), 'tsuzuri.json'),
	});
	try {
		const grammar = await registry.loadGrammar('source.tsuzuri');
		assert.ok(grammar);
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
