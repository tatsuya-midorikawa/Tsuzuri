import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import * as vscode from 'vscode';
import { toolchain } from './toolchain';
import { endsInPath, libraryQualifier, runProcess } from './core';

const keywords = 'def fn rec and export extern private record union type const test class instance deriving dyn let task do return yield for in to downto while break continue mut ref deref new as if then elif else match with when true false where';
const types = 'bool unit string utf8string char utf8char byte ubyte i8 i16 i32 i64 i128 i8u i16u i32u i64u i128u f16 f32 f64 f128 d32 d64 d128 Task Maybe Result Vec Map Set Seq IO';

export function registerEditor(context: vscode.ExtensionContext) {
	context.subscriptions.push(vscode.languages.registerDocumentFormattingEditProvider('tsuzuri', {
		async provideDocumentFormattingEdits(document, _options, token) {
			if (token.isCancellationRequested) { return []; }
			const version = document.version;
			const tools = await toolchain(context, document.uri);
			const directory = await mkdtemp(path.join(os.tmpdir(), 'tsuzuri-format-'));
			const controller = new AbortController();
			const cancellation = token.onCancellationRequested(() => controller.abort());
			try {
				const file = path.join(directory, /\.(tz|tt|tc)$/.test(document.fileName) ? path.basename(document.fileName) : 'Main.tz');
				await writeFile(file, document.getText(), 'utf8');
				const result = await runProcess(tools.compiler, ['fmt', file, '--json'], {
					env: tools.env, signal: controller.signal, timeout: 30000,
				});
				if (token.isCancellationRequested || document.version !== version) { return []; }
				if (result.code !== 0) { throw new Error(result.stderr || 'Tsuzuri formatting failed.'); }
				const formatted = await readFile(file, 'utf8');
				if (token.isCancellationRequested || document.version !== version) { return []; }
				return [vscode.TextEdit.replace(new vscode.Range(document.positionAt(0), document.positionAt(document.getText().length)), formatted)];
			} finally {
				cancellation.dispose();
				await rm(directory, { recursive: true, force: true });
			}
		},
	}));
	context.subscriptions.push(vscode.languages.registerCompletionItemProvider('tsuzuri', {
		async provideCompletionItems(document, position) {
			const prefix = document.lineAt(position).text.slice(0, position.character);
			if (endsInPath(prefix)) { return []; }
			const module = libraryQualifier(prefix);
			if (module) {
				const file = context.asAbsolutePath('resources/completions.json');
				try {
					const library: { module: string; name: string; signature: string }[] = JSON.parse(await readFile(file, 'utf8'));
					return library.filter(item => item.module === module).map(item => {
						const completion = new vscode.CompletionItem(item.name, vscode.CompletionItemKind.Function);
						completion.detail = item.signature;
						return completion;
					});
				} catch { return []; }
			}
			return [
				...keywords.split(' ').map(word => new vscode.CompletionItem(word, vscode.CompletionItemKind.Keyword)),
				...types.split(' ').map(word => new vscode.CompletionItem(word, vscode.CompletionItemKind.TypeParameter)),
			];
		},
	}, '.'));
}
