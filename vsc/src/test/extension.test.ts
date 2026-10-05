import * as assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import * as path from 'node:path';
import * as vscode from 'vscode';
import type { activate } from '../extension';
import { supportsDebug } from '../core';

async function waitFor<Value>(description: string, value: () => Value | undefined | Promise<Value | undefined>, timeout = 30000): Promise<Value> {
	const deadline = Date.now() + timeout;
	while (Date.now() < deadline) {
		const result = await value();
		if (result !== undefined) { return result; }
		await new Promise(resolve => setTimeout(resolve, 50));
	}
	throw new Error(`Timed out: ${description}`);
}

export async function run(): Promise<void> {
	// Match VS Code's fsPath form, which lowercases Windows drive letters.
	const root = vscode.Uri.file(process.env.TSUZURI_TEST_PROJECT!).fsPath;
	const uri = vscode.Uri.file(path.join(root, 'Main.tz'));
	const document = await vscode.workspace.openTextDocument(uri);
	await vscode.window.showTextDocument(document);
	assert.equal(document.languageId, 'tsuzuri');
	const extension = vscode.extensions.getExtension<NonNullable<Awaited<ReturnType<typeof activate>>>>('tmidorikawa.tsuzuri');
	assert.ok(extension);
	if (process.env.TSUZURI_TEST_INSTALLED === '1') {
		assert.ok(extension.extensionPath.includes(`${path.sep}extensions${path.sep}`), extension.extensionPath);
	}
	const api = await extension.activate();
	const original = document.getText();
	const replace = async (text: string) => {
		const edit = new vscode.WorkspaceEdit();
		edit.replace(uri, new vscode.Range(document.positionAt(0), document.positionAt(document.getText().length)), text);
		assert.ok(await vscode.workspace.applyEdit(edit));
	};
	const hover = await waitFor('language server hover', async () => {
		const values = await vscode.commands.executeCommand<vscode.Hover[]>('vscode.executeHoverProvider', uri, document.positionAt(original.indexOf('calculate 2')));
		return values?.length ? values : undefined;
	});
	assert.match(hover.flatMap(item => item.contents.map(content => typeof content === 'string' ? content : content.value)).join('\n'), /i64/);
	const symbols = await vscode.commands.executeCommand<vscode.DocumentSymbol[]>('vscode.executeDocumentSymbolProvider', uri);
	assert.ok(symbols.some(symbol => symbol.name === 'calculate'));
	const definitions = await vscode.commands.executeCommand<(vscode.Location | vscode.LocationLink)[]>('vscode.executeDefinitionProvider', uri, document.positionAt(original.indexOf('Geometry::Point.offset')));
	assert.ok(definitions.length);
	const target = 'targetUri' in definitions[0] ? definitions[0].targetUri : definitions[0].uri;
	assert.equal(target.fsPath, path.join(root, 'Geometry', 'Point.tz'));
	const report = await vscode.workspace.openTextDocument(vscode.Uri.file(path.join(root, 'Report.tz')));
	for (const needle of ['Circle.radius', 'Shapes::Circle.radius']) {
		const found = await waitFor(`namespace definition of ${needle}`, async () => {
			const values = await vscode.commands.executeCommand<(vscode.Location | vscode.LocationLink)[]>('vscode.executeDefinitionProvider', report.uri, report.positionAt(report.getText().indexOf(needle)));
			return values?.length ? values[0] : undefined;
		});
		assert.equal(('targetUri' in found ? found.targetUri : found.uri).fsPath, path.join(root, 'Shapes', 'Circle.tz'), needle);
	}
	console.log('VS Code: language registration, hover, outline, cross-file and namespace definitions passed.');

	const other = await vscode.workspace.openTextDocument(vscode.Uri.file(path.join(process.env.TSUZURI_TEST_OTHER!, 'Main.tz')));
	await waitFor('independent workspace language servers', () => api.clients.size === 2 ? true : undefined);
	const invalid = 'def bad :: i64 = { let _text = "\u65e5\u{1f600}"; true }\n';
	await replace(invalid);
	const error = await waitFor('unsaved UTF-16 diagnostic', () => vscode.languages.getDiagnostics(uri).find(item => item.code === 'E1003'));
	assert.equal(error.range.start.character, invalid.indexOf('true'));
	assert.equal(vscode.languages.getDiagnostics(other.uri).filter(item => item.severity === vscode.DiagnosticSeverity.Error).length, 0);
	assert.equal(await readFile(uri.fsPath, 'utf8'), original);
	await replace(original.replace('    let result', '    let unused = 7\n    let result'));
	await waitFor('unused binding warning', () => vscode.languages.getDiagnostics(uri).find(item => item.severity === vscode.DiagnosticSeverity.Warning));
	await replace(original);
	await waitFor('diagnostics cleared', () => !vscode.languages.getDiagnostics(uri).length ? true : undefined);
	console.log('VS Code: unsaved Unicode errors, warnings, clearing, multi-root isolation passed.');

	await replace('def reuse :: [i64] -> i64\nfn reuse a =\n    let b = a\n    Array.length (ref a) + Array.length (ref b)\n');
	const hints = await waitFor('implicit copy inlay hint', async () => {
		const values = await vscode.commands.executeCommand<vscode.InlayHint[]>('vscode.executeInlayHintProvider', uri, new vscode.Range(0, 0, 4, 0));
		return values?.length ? values : undefined;
	});
	assert.deepEqual(hints.map(hint => [hint.label, hint.position.line, hint.position.character]), [['copy (local)', 2, 13]]);
	await replace('def identity::i64->i64=\\value->value+1\n');
	const edits = await vscode.commands.executeCommand<vscode.TextEdit[]>('vscode.executeFormatDocumentProvider', uri, { tabSize: 4, insertSpaces: true });
	assert.ok(edits.length);
	const formatting = new vscode.WorkspaceEdit();
	formatting.set(uri, edits);
	assert.ok(await vscode.workspace.applyEdit(formatting));
	assert.match(document.getText(), /def identity :: i64 -> i64/);
	assert.equal(await readFile(uri.fsPath, 'utf8'), original);
	await replace('def main :: unit -> i32 = \\() ->\n    do! IO.\n');
	const completions = await vscode.commands.executeCommand<vscode.CompletionList>('vscode.executeCompletionItemProvider', uri, new vscode.Position(1, 11), '.');
	assert.ok(completions.items.some(item => item.label === 'write_line'));
	await replace('');
	const snippets = JSON.parse(await readFile(path.join(extension.extensionPath, 'snippets', 'tsuzuri.json'), 'utf8'));
	assert.ok(await vscode.window.activeTextEditor!.insertSnippet(new vscode.SnippetString(snippets.Function.body.join('\n'))));
	assert.match(document.getText(), /= \\value ->/);
	await vscode.commands.executeCommand('leaveSnippet');
	await replace(original);
	assert.ok(await document.save());
	console.log('VS Code: unsaved formatting, standard-library completion, and copy inlay hints passed.');

	const tasks = await vscode.tasks.fetchTasks({ type: 'tsuzuri' });
	assert.ok(tasks.some(task => task.definition.action === 'build'));
	assert.ok(tasks.some(task => task.definition.action === 'test'));
	for (const action of ['build', 'run', 'check']) {
		let exitCode: number | undefined;
		const taskSubscription = vscode.tasks.onDidEndTaskProcess(event => {
			if (event.execution.task.definition.action === action && event.execution.task.definition.project === root) { exitCode = event.exitCode; }
		});
		try {
			await vscode.commands.executeCommand(`tsuzuri.${action}`, uri);
			assert.equal(await waitFor(`VS Code ${action} task`, () => exitCode, 180000), 0);
		} finally { taskSubscription.dispose(); }
	}
	await api.testing.discover(root);
	const project = api.testing.roots.get(root)!;
	const tests: vscode.TestItem[] = [];
	project.children.forEach(file => file.children.forEach(item => tests.push(item)));
	assert.equal(tests.length, 2);
	assert.equal(tests[0].label, tests[1].label);
	assert.notEqual(tests[0].id, tests[1].id);
	const cancellation = new vscode.CancellationTokenSource();
	try {
		assert.deepEqual(await api.testing.run(new vscode.TestRunRequest([tests[0]]), cancellation.token, 0), { passed: 1, failed: 0, errored: 0, skipped: 0 });
		assert.deepEqual(await api.testing.run(new vscode.TestRunRequest([tests[1]]), cancellation.token, 3), { passed: 0, failed: 1, errored: 0, skipped: 0 });
	} finally { cancellation.dispose(); }
	await replace(`${original}\ntest "new" = assert true\n`);
	assert.ok(await document.save());
	await waitFor('automatic test discovery after save', () => {
		const names: string[] = [];
		api.testing.roots.get(root)!.children.forEach(file => file.children.forEach(item => names.push(item.label)));
		return names.includes('new') ? true : undefined;
	});
	await replace(original);
	assert.ok(await document.save());
	await vscode.commands.executeCommand('tsuzuri.restartLanguageServer');
	await waitFor('language server restart', async () => {
		const values = await vscode.commands.executeCommand<vscode.Hover[]>('vscode.executeHoverProvider', uri, document.positionAt(original.indexOf('calculate 2')));
		return values?.length ? true : undefined;
	});
	console.log('VS Code: build task, test discovery, duplicate-name selection, pass/fail reporting passed.');

	if (!supportsDebug()) {
		assert.equal(process.platform, 'win32');
		assert.equal(process.arch, 'arm64');
		assert.equal(vscode.extensions.getExtension('vadimcn.vscode-lldb'), undefined);
		assert.equal(await vscode.commands.executeCommand('tsuzuri.debug', uri), false);
		console.log('Windows ARM64: editing, diagnostics, build, tests passed; only debugging is unavailable.');
		return;
	}
	let stopped: { session: vscode.DebugSession; threadId: number } | undefined;
	const tracker = vscode.debug.registerDebugAdapterTrackerFactory('lldb', {
		createDebugAdapterTracker: session => ({ onDidSendMessage(message) {
			if (message.type === 'event' && message.event === 'stopped') { stopped = { session, threadId: message.body.threadId }; }
		} }),
	});
	const breakpoint = new vscode.SourceBreakpoint(new vscode.Location(uri, new vscode.Position(2, 4)));
	vscode.debug.addBreakpoints([breakpoint]);
	try {
		assert.ok(await vscode.commands.executeCommand('tsuzuri.debug', uri));
		const pause = await waitFor('source breakpoint', () => stopped, 180000);
		const stack = await pause.session.customRequest('stackTrace', { threadId: pause.threadId });
		assert.equal(stack.stackFrames[0].source.path, uri.fsPath);
		assert.match(stack.stackFrames[0].name, /calculate/);
		const scopes = await pause.session.customRequest('scopes', { frameId: stack.stackFrames[0].id });
		const variables = (await Promise.all(scopes.scopes.map((scope: { variablesReference: number }) => pause.session.customRequest('variables', { variablesReference: scope.variablesReference })))).flatMap(result => result.variables);
		assert.ok(variables.some((variable: { name: string; value: string }) => variable.name === 'result' && variable.value === '42'), JSON.stringify(variables));
		stopped = undefined;
		await pause.session.customRequest('next', { threadId: pause.threadId });
		await waitFor('source step', () => stopped, 30000);
		await vscode.debug.stopDebugging(pause.session);
		console.log('VS Code: debug build, CodeLLDB launch, source breakpoint, stack, local variable, stepping passed.');
	} finally {
		vscode.debug.removeBreakpoints([breakpoint]);
		tracker.dispose();
		if (vscode.debug.activeDebugSession) { await vscode.debug.stopDebugging(); }
	}
	console.log('All Tsuzuri VS Code integration checks passed.');
}
