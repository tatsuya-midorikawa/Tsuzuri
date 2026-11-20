import * as assert from 'node:assert/strict';
import { mkdir, readFile, rm, writeFile } from 'node:fs/promises';
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

	// G40: a folder that is not a project, with a package below it. Each file has one client: a file outside the package
	// takes the highest directory that holds no other project, and a file beside the package takes a remaining root,
	// whose client serves only the files directly in the folder.
	const tsuzuri = (target: vscode.Uri, code?: string) => vscode.languages.getDiagnostics(target).filter(item => item.source === 'tsuzuri' && (!code || item.code === code));
	const opened = async (target: vscode.Uri) => {
		await vscode.commands.executeCommand('vscode.open', target);
		return vscode.workspace.openTextDocument(target);
	};
	const hovers = async (target: vscode.TextDocument, needle: string) =>
		await vscode.commands.executeCommand<vscode.Hover[]>('vscode.executeHoverProvider', target.uri, target.positionAt(target.getText().indexOf(needle))) ?? [];
	const outline = async (target: vscode.TextDocument) =>
		(await vscode.commands.executeCommand<vscode.DocumentSymbol[]>('vscode.executeDocumentSymbolProvider', target.uri) ?? []).map(symbol => symbol.name);
	const hovered = (description: string, target: vscode.TextDocument, needle: string) =>
		waitFor(description, async () => (await hovers(target, needle)).length ? true : undefined);
	const selector = (root: string) => {
		const filter = api.clients.get(root)?.clientOptions.documentSelector?.[0];
		return typeof filter === 'object' && 'pattern' in filter ? String(filter.pattern) : undefined;
	};
	// VS Code sends a document to every client whose selector matches it, by its own matching of the glob.
	const matches = (root: string, target: vscode.TextDocument) =>
		vscode.languages.match(api.clients.get(root)!.clientOptions.documentSelector as unknown as vscode.DocumentSelector, target);
	const loose = vscode.Uri.file(process.env.TSUZURI_TEST_LOOSE!).fsPath;
	const scratch = path.join(loose, 'scratch');
	const looseApp = path.join(loose, 'App');
	const packageMain = vscode.Uri.file(path.join(looseApp, 'Main.tz'));
	const scratchDocument = await opened(vscode.Uri.file(path.join(scratch, 'Try.tz')));
	await hovered('hover of a file outside the package', scratchDocument, 't()');
	assert.ok(api.clients.has(scratch));
	assert.ok(!api.clients.has(loose));
	assert.deepEqual(tsuzuri(packageMain, 'E1002'), []);
	const appDocument = await opened(packageMain);
	await hovered('hover of the package', appDocument, 'main');
	assert.ok(api.clients.has(looseApp));
	assert.deepEqual(tsuzuri(packageMain), []);
	assert.equal((await hovers(appDocument, 'main')).length, 1);
	assert.deepEqual(await outline(appDocument), ['main']);
	const looseNotes = vscode.Uri.file(path.join(loose, 'Notes.tz'));
	const looseNotesDocument = await opened(looseNotes);
	await waitFor('the error of the file beside the package', () => tsuzuri(looseNotes, 'E1003')[0]);
	assert.ok(selector(loose)?.endsWith('/loose/*.{tz,tt,tc}'), selector(loose));
	assert.ok(matches(loose, looseNotesDocument) > 0);
	assert.equal(matches(loose, appDocument), 0);
	// The remaining root's server reads App without its manifest; its E1002 in App/Main.tz, sent before Notes.tz, is dropped.
	assert.deepEqual(tsuzuri(packageMain), []);
	assert.equal((await hovers(appDocument, 'main')).length, 1);
	assert.deepEqual(await outline(appDocument), ['main']);
	// With namespace headers both servers read App alike: one hover and one symbol show that only App's client serves it.
	const loose2 = vscode.Uri.file(process.env.TSUZURI_TEST_LOOSE2!).fsPath;
	const notes2Document = await opened(vscode.Uri.file(path.join(loose2, 'Notes.tz')));
	await hovered('hover beside a package with headers', notes2Document, 'n()');
	assert.ok(selector(loose2)?.endsWith('/loose2/*.{tz,tt,tc}'), selector(loose2));
	const app2Document = await opened(vscode.Uri.file(path.join(loose2, 'App', 'Main.tz')));
	await hovered('hover of the package with headers', app2Document, 'main');
	// The remaining root's client is never sent App/Main.tz, which only App's client serves. The matching decides this
	// without timing; the counts below agree with it, but alone they would show a second client only once its server
	// answers.
	assert.ok(matches(loose2, notes2Document) > 0);
	assert.equal(matches(loose2, app2Document), 0);
	assert.ok(matches(path.join(loose2, 'App'), app2Document) > 0);
	assert.equal((await hovers(app2Document, 'main')).length, 1);
	assert.deepEqual(await outline(app2Document), ['main']);
	console.log('VS Code: a folder with a package gives each file one client, and a remaining root serves only its own files.');
	// G40 D4: a created or deleted marker, or a deleted package folder, decides the roots again.
	const remaining = api.clients.get(loose);
	await writeFile(path.join(loose, 'Main.tz'), 'def main :: unit -> i32 = \\() -> 0\n');
	await waitFor('the roots after a marker was created', () => !api.clients.has(scratch) && !api.testing.roots.has(scratch)
		&& api.clients.get(loose) !== remaining && selector(loose)?.endsWith('/loose/**/*.{tz,tt,tc}') ? true : undefined);
	await rm(path.join(loose, 'Main.tz'));
	await waitFor('the roots after the marker was deleted', () => api.clients.has(scratch) && api.testing.roots.has(scratch)
		&& selector(loose)?.endsWith('/loose/*.{tz,tt,tc}') ? true : undefined);
	assert.ok(api.clients.has(looseApp));
	// A deleted folder with open files in it leaves no client or test root in a directory that does not exist: not for an
	// open file of a project in it (its root would now be the deleted folder, as the folder above holds App), and not for
	// a root that is the deleted folder itself.
	const extra = path.join(loose, 'extra');
	const demo = path.join(extra, 'demo');
	const gone = path.join(loose, 'gone');
	await mkdir(demo, { recursive: true });
	await mkdir(gone);
	await writeFile(path.join(demo, 'Main.tz'), 'def main :: unit -> i32 = \\() -> 0\n');
	await writeFile(path.join(gone, 'Other.tz'), 'fn o() -> i64 { 1 }\n');
	await hovered('hover of a project in a folder to delete', await opened(vscode.Uri.file(path.join(demo, 'Main.tz'))), 'main');
	await hovered('hover of a root to delete', await opened(vscode.Uri.file(path.join(gone, 'Other.tz'))), 'o()');
	assert.ok(api.clients.has(demo) && api.clients.has(gone));
	await api.testing.discover(demo);
	await api.testing.discover(gone);
	// A short-lived compiler run in the folder (a test discovery) can hold it on Windows for a moment; a language server
	// would hold it for good, which the retries do not outlast.
	await rm(extra, { recursive: true, maxRetries: 10, retryDelay: 100 });
	await rm(gone, { recursive: true, maxRetries: 10, retryDelay: 100 });
	await waitFor('the roots after folders with open files were deleted', () => !api.clients.has(demo) && !api.clients.has(gone)
		&& !api.testing.roots.has(demo) && !api.testing.roots.has(gone) ? true : undefined);
	assert.ok(!api.clients.has(extra), 'no client in the deleted folder');
	assert.ok(!api.testing.roots.has(extra), 'no test root in the deleted folder');
	const restored = api.clients.get(loose);
	await rm(looseApp, { recursive: true, maxRetries: 10, retryDelay: 100 });
	await waitFor('the roots after the package folder was deleted', () => !api.clients.has(scratch) && !api.clients.has(looseApp)
		&& !api.testing.roots.has(scratch) && !api.testing.roots.has(looseApp)
		&& api.clients.get(loose) !== restored && selector(loose)?.endsWith('/loose/**/*.{tz,tt,tc}') ? true : undefined);
	console.log('VS Code: creating or deleting a marker or a package folder decides the roots of the language servers and tests again.');
	await vscode.window.showTextDocument(document);
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

	await replace('def reuse :: own [i64] -> i64\nfn reuse a =\n    let b = a\n    Array.length (ref a) + Array.length (ref b)\n');
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
	const runTask = async (action: string) => {
		let exitCode: number | undefined;
		const taskSubscription = vscode.tasks.onDidEndTaskProcess(event => {
			if (event.execution.task.definition.action === action && event.execution.task.definition.project === root) { exitCode = event.exitCode; }
		});
		try {
			await vscode.commands.executeCommand(`tsuzuri.${action}`, uri);
			assert.equal(await waitFor(`VS Code ${action} task`, () => exitCode, 180000), 0);
		} finally { taskSubscription.dispose(); }
	};
	for (const action of ['build', 'run', 'check']) { await runTask(action); }
	// G39: the tasks have no problem matcher, whose markers VS Code keeps until the next task. getDiagnostics can miss the
	// removal of such markers, so the editor's marker navigation tells whether Main.tz has any.
	const hasMarkers = async () => {
		const editor = await vscode.window.showTextDocument(document);
		const last = document.lineAt(document.lineCount - 1).range.end;
		editor.selection = new vscode.Selection(last, last);
		await vscode.commands.executeCommand('editor.action.marker.next');
		const moved = await waitFor('marker navigation', () => editor.selection.active.isEqual(last) ? undefined : true, 2000).catch(() => false);
		await vscode.commands.executeCommand('closeMarkersNavigation');
		return moved;
	};
	await replace(original.replace('    let result', '    let unused = 7\n    let result'));
	for (const action of ['check', 'run']) { await runTask(action); }
	await waitFor('language server warning', () => vscode.languages.getDiagnostics(uri).find(item => item.code === 'W1001' && item.source === 'tsuzuri'));
	assert.equal(await hasMarkers(), true);
	await replace(original);
	assert.ok(await document.save());
	await waitFor('language server warning cleared', () => vscode.languages.getDiagnostics(uri).some(item => item.source === 'tsuzuri') ? undefined : true);
	assert.equal(await hasMarkers(), false);
	console.log('VS Code: Check and Run tasks with a warning leave no marker after the fix.');
	// G39: the test discovery that a save starts reports no diagnostics, so no copy of a fixed error stays behind.
	await replace(original.replace('calculate 2', 'calculate missing'));
	assert.ok(await document.save());
	await waitFor('failed test discovery after saving an error', () => api.testing.roots.get(root)!.error !== undefined ? true : undefined);
	await replace(original);
	assert.ok(await document.save());
	await waitFor('test discovery after saving the fix', () => api.testing.roots.get(root)!.error === undefined ? true : undefined);
	assert.deepEqual(vscode.languages.getDiagnostics(uri).filter(item => item.source === 'Tsuzuri').map(item => item.code), []);
	console.log('VS Code: test discovery after saving an error and its fix leaves no copy of the error.');
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
	// G39: a test run leaves out the warnings that the language server reports at the same place.
	await replace(original.replace('    let result', '    let unused = 7\n    let result'));
	await waitFor('language server warning', () => vscode.languages.getDiagnostics(uri).find(item => item.code === 'W1001' && item.source === 'tsuzuri'));
	const warned = new vscode.CancellationTokenSource();
	try {
		assert.deepEqual(await api.testing.run(new vscode.TestRunRequest([project]), warned.token, 0), { passed: 1, failed: 1, errored: 0, skipped: 0 });
	} finally { warned.dispose(); }
	assert.deepEqual(vscode.languages.getDiagnostics(uri).filter(item => item.code === 'W1001').map(item => item.source).sort(), ['tsuzuri']);
	assert.deepEqual(vscode.languages.getDiagnostics(uri).filter(item => item.source === 'Tsuzuri').map(item => item.code), ['E2006']);
	console.log('VS Code: a test run shows no copy of a language server warning.');
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

	// G39: an explicit test run reports its failed discovery, such as E1036 that the language server does not check,
	// until the project changes: an edit, a deleted folder, or a changed manifest. The run's own saveAll keeps it.
	const packages = vscode.Uri.file(process.env.TSUZURI_TEST_PACKAGES!).fsPath;
	const copies = (target: vscode.Uri, code?: string) => vscode.languages.getDiagnostics(target).filter(item => item.source === 'Tsuzuri' && (!code || item.code === code));
	const cleared = (description: string, target: vscode.Uri) => waitFor(description, () => copies(target).length ? undefined : true);
	const runProject = async (project: string) => {
		const token = new vscode.CancellationTokenSource();
		try { return await api.testing.run(new vscode.TestRunRequest([api.testing.roots.get(project)!]), token.token, 0); }
		finally { token.dispose(); }
	};
	const failed = { passed: 0, failed: 0, errored: 1, skipped: 0 };
	const limited = path.join(packages, 'capabilities');
	const limitedUri = vscode.Uri.file(path.join(limited, 'Main.tz'));
	const limitedDocument = await vscode.workspace.openTextDocument(limitedUri);
	const limitedText = limitedDocument.getText();
	const editLimited = async (text: string) => {
		const edit = new vscode.WorkspaceEdit();
		edit.replace(limitedUri, new vscode.Range(limitedDocument.positionAt(0), limitedDocument.positionAt(limitedDocument.getText().length)), text);
		assert.ok(await vscode.workspace.applyEdit(edit));
	};
	await waitFor('capabilities project', () => api.testing.roots.get(limited));
	const saves = vscode.workspace.createFileSystemWatcher(new vscode.RelativePattern(limited, 'Main.tz'), false, false, true);
	try {
		const saved = new Promise(resolve => { saves.onDidCreate(resolve); saves.onDidChange(resolve); });
		await editLimited(`${limitedText}\n`);
		assert.deepEqual(await runProject(limited), failed);
		assert.equal(copies(limitedUri, 'E1036').length, 1);
		// The watcher reports the saveAll of the run late, as a change or as a creation, or (on macOS, for the first
		// saves of a file in a session) not at all.
		await Promise.race([saved, new Promise(resolve => setTimeout(resolve, 3000))]);
		await new Promise(resolve => setTimeout(resolve, 500));
		assert.equal(copies(limitedUri, 'E1036').length, 1);
	} finally { saves.dispose(); }
	await editLimited(limitedText);
	await cleared('run result cleared by an edit', limitedUri);
	assert.deepEqual(await runProject(limited), failed);
	assert.equal(copies(limitedUri, 'E1036').length, 1);
	await rm(path.join(limited, 'Extra'), { recursive: true });
	await cleared('run result cleared by a deleted folder', limitedUri);
	assert.deepEqual(await runProject(limited), failed);
	assert.equal(copies(limitedUri, 'E1036').length, 1);
	await writeFile(path.join(limited, 'Fresh.tz'), 'def fresh :: i64 = 2\n');
	await cleared('run result cleared by a created source', limitedUri);
	assert.deepEqual(await runProject(limited), failed);
	assert.equal(copies(limitedUri, 'E1036').length, 1);
	const manifest = path.join(limited, 'Tsuzuri.toml');
	const denied = await readFile(manifest, 'utf8');
	await writeFile(manifest, denied.replace('allow = []', 'allow = ["stdio"]'));
	await cleared('run result cleared by a manifest change', limitedUri);
	console.log('VS Code: a test run reports E1036 of its discovery until an edit, a deleted folder, a created source, or a manifest change.');

	// G39: a root that a run first follows after its start (found by refreshRoots) keeps an edit made before that.
	const lateRoot = async (name: string) => {
		const root = path.join(packages, name);
		await mkdir(root, { recursive: true });
		await writeFile(path.join(root, 'Main.tz'), 'def late :: i64 = 1\n');
		const uri = vscode.Uri.file(path.join(root, 'Main.tz'));
		return { root, uri, text: JSON.stringify({ severity: 'error', code: 'E2005', message: 'late', path: uri.fsPath, span: { start: 0, end: 3 } }) };
	};
	const edited = await lateRoot('late-edited');
	let lateStart = api.workflow.begin([]);
	const lateDocument = await vscode.workspace.openTextDocument(edited.uri);
	const lateEdit = new vscode.WorkspaceEdit();
	lateEdit.insert(edited.uri, lateDocument.positionAt(0), '\n');
	assert.ok(await vscode.workspace.applyEdit(lateEdit));
	api.workflow.track(edited.root);
	await api.workflow.publishDiagnostics(edited.root, edited.text, lateStart);
	assert.equal(copies(edited.uri).length, 0);
	assert.ok(await lateDocument.save());
	const untouched = await lateRoot('late-untouched');
	lateStart = api.workflow.begin([]);
	api.workflow.track(untouched.root);
	await api.workflow.publishDiagnostics(untouched.root, untouched.text, lateStart);
	assert.equal(copies(untouched.uri, 'E2005').length, 1);
	console.log('VS Code: a root that a run follows after its start keeps the edits made before.');

	// G39: a result in a path dependency outside the root (../lib) depends on every source outside the root: a change
	// there clears it, also one that is not in the result's files, and a change during the run drops it.
	const app = path.join(packages, 'app');
	const calc = vscode.Uri.file(path.join(packages, 'lib', 'Calc.tz'));
	const shapes = path.join(packages, 'lib', 'Shapes.tz');
	await api.testing.discover(app).catch(() => undefined);
	assert.deepEqual(await runProject(app), failed);
	assert.equal(copies(calc, 'E1003').length, 1);
	await writeFile(shapes, 'export def base :: i64 = 0\n');
	await cleared('dependency result cleared by a change of another file outside the root', calc);
	await writeFile(calc.fsPath, 'export def twice :: i64 -> i64 = \\value ->\n    let unused = 7\n    value * 2 + Shapes.base()\n');
	const passed = { passed: 1, failed: 0, errored: 0, skipped: 0 };
	// The language server also reports this W1001, and a run leaves out what it reports (step 3). No client covers lib
	// until Calc.tz opens below, so the run's copy shows; the checks of a dropped run below rely on that.
	const server = () => vscode.languages.getDiagnostics(calc).filter(item => item.source === 'tsuzuri').map(item => item.code);
	assert.deepEqual(await runProject(app), passed);
	assert.deepEqual(server(), []);
	assert.equal(copies(calc, 'W1001').length, 1);
	const latest = () => { let item: vscode.TestItem | undefined; api.testing.roots.get(app)!.children.forEach(child => { item = child; }); return item; };
	/**
	 * Runs app and makes `change` once the run's discovery has replaced the items. The run takes the change numbers and
	 * starts the compiler in the same task as that replacement, so the change comes during the compilation. An edit in
	 * the editor counts at once, without a watcher or a file time.
	 */
	const changedDuringRun = async (change: () => Promise<void>) => {
		const discovered = latest();
		const inFlight = runProject(app);
		await waitFor('the discovery of the run', () => latest() !== discovered ? true : undefined);
		await change();
		return inFlight;
	};
	const replaceText = async (target: vscode.TextDocument, text: string) => {
		const edit = new vscode.WorkspaceEdit();
		edit.replace(target.uri, new vscode.Range(target.positionAt(0), target.positionAt(target.getText().length)), text);
		assert.ok(await vscode.workspace.applyEdit(edit));
	};
	// An edit of a Tsuzuri document outside the root (other/Main.tz, open since the start) during the run drops a result
	// that points outside the root.
	const otherText = other.getText();
	assert.deepEqual(await changedDuringRun(() => replaceText(other, `${otherText}\n`)), passed);
	assert.deepEqual(copies(calc), []);
	await replaceText(other, otherText);
	assert.ok(await other.save());
	assert.deepEqual(await runProject(app), passed);
	assert.deepEqual(server(), []);
	assert.equal(copies(calc, 'W1001').length, 1);
	// An edit of any document under the root during the run drops its result: the root's own change number.
	const notes = await vscode.workspace.openTextDocument(vscode.Uri.file(path.join(app, 'notes.txt')));
	const notesText = notes.getText();
	assert.deepEqual(await changedDuringRun(() => replaceText(notes, `${notesText}More.\n`)), passed);
	assert.deepEqual(copies(calc), []);
	await replaceText(notes, notesText);
	assert.ok(await notes.save());
	// A change on disk clears a result made before it, also when a later run of the root started after the change but
	// stopped after its discovery without a result (here, a run that excludes every test).
	const appMain = vscode.Uri.file(path.join(app, 'Main.tz'));
	const appText = await readFile(appMain.fsPath, 'utf8');
	await writeFile(appMain.fsPath, appText.replace('== 4', '== 5'));
	assert.deepEqual(await runProject(app), { passed: 0, failed: 1, errored: 0, skipped: 0 });
	assert.equal(copies(appMain, 'E2006').length, 1);
	await writeFile(appMain.fsPath, appText);
	const appItem = api.testing.roots.get(app)!;
	const none = new vscode.CancellationTokenSource();
	try {
		assert.deepEqual(await api.testing.run(new vscode.TestRunRequest([appItem], [appItem]), none.token, 0), { passed: 0, failed: 0, errored: 0, skipped: 0 });
	} finally { none.dispose(); }
	await cleared('run result cleared by a change on disk before a run that stopped after its discovery', appMain);
	await writeFile(calc.fsPath, 'export def twice :: i64 -> i64 = \\value -> value * 2 + true\n');
	assert.deepEqual(await runProject(app), failed);
	assert.equal(copies(calc, 'E1003').length, 1);
	// VS Code reports a deleted folder as one event of the folder, also outside the root.
	await rm(path.join(packages, 'lib', 'Extra'), { recursive: true });
	await cleared('dependency result cleared by a deleted folder outside the root', calc);
	assert.deepEqual(await runProject(app), failed);
	assert.equal(copies(calc, 'E1003').length, 1);
	const library = await vscode.workspace.openTextDocument(calc);
	await replaceText(library, 'export def twice :: i64 -> i64 = \\value -> value * 2\n');
	assert.ok(await library.save());
	await cleared('dependency result cleared by an edit outside the root', calc);
	console.log('VS Code: a result in a path dependency clears on changes outside the root, and a run changed while it runs shows no result.');

	// G39: removing a workspace folder clears the results of its projects. This is the last check, as the language
	// servers restart. The packages folder is not the first, whose removal would restart the extension host.
	const removeFolder = async () => {
		await writeFile(manifest, denied);
		assert.deepEqual(await runProject(limited), failed);
		assert.equal(copies(limitedUri, 'E1036').length, 1);
		const index = vscode.workspace.workspaceFolders!.findIndex(folder => folder.uri.fsPath === packages);
		assert.equal(index, 2);
		assert.ok(vscode.workspace.updateWorkspaceFolders(index, 1));
		await cleared('run result cleared by removing its workspace folder', limitedUri);
		console.log('VS Code: removing a workspace folder clears the run results of its projects.');
	};

	if (!supportsDebug()) {
		assert.equal(process.platform, 'win32');
		assert.equal(process.arch, 'arm64');
		assert.equal(vscode.extensions.getExtension('vadimcn.vscode-lldb'), undefined);
		assert.equal(await vscode.commands.executeCommand('tsuzuri.debug', uri), false);
		await removeFolder();
		console.log('Windows ARM64: editing, diagnostics, build, tests passed; only debugging is unavailable.');
		return;
	}
	let stopped: { session: vscode.DebugSession; threadId: number } | undefined;
	const tracker = vscode.debug.registerDebugAdapterTrackerFactory('lldb', {
		createDebugAdapterTracker: session => ({ onDidSendMessage(message) {
			if (message.type === 'event' && message.event === 'stopped') { stopped = { session, threadId: message.body.threadId }; }
		} }),
	});
	const breakpoint = new vscode.SourceBreakpoint(new vscode.Location(uri, new vscode.Position(3, 4)));
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
		// The Tsuzuri LLDB formatters, which the launch loads, show a string as its text.
		assert.ok(variables.some((variable: { name: string; value: string }) => variable.name === 'label' && variable.value === '"ok"'), JSON.stringify(variables));
		stopped = undefined;
		await pause.session.customRequest('next', { threadId: pause.threadId });
		await waitFor('source step', () => stopped, 30000);
		await vscode.debug.stopDebugging(pause.session);
		console.log('VS Code: debug build, CodeLLDB launch, source breakpoint, stack, formatted local variables, stepping passed.');
	} finally {
		vscode.debug.removeBreakpoints([breakpoint]);
		tracker.dispose();
		if (vscode.debug.activeDebugSession) { await vscode.debug.stopDebugging(); }
	}

	// The Debug test profile builds one test with debug information and stops in its body.
	await api.testing.discover(root);
	const current: vscode.TestItem[] = [];
	api.testing.roots.get(root)!.children.forEach(file => file.children.forEach(item => current.push(item)));
	const passing = current.find(item => item.range?.start.line === 9);
	assert.ok(passing, JSON.stringify(current.map(item => [item.label, item.range?.start.line])));
	stopped = undefined;
	const testTracker = vscode.debug.registerDebugAdapterTrackerFactory('lldb', {
		createDebugAdapterTracker: session => ({ onDidSendMessage(message) {
			if (message.type === 'event' && message.event === 'stopped') { stopped = { session, threadId: message.body.threadId }; }
		} }),
	});
	const testBreakpoint = new vscode.SourceBreakpoint(new vscode.Location(uri, new vscode.Position(9, 14)));
	vscode.debug.addBreakpoints([testBreakpoint]);
	const debugging = new vscode.CancellationTokenSource();
	try {
		const result = api.testing.debug(new vscode.TestRunRequest([passing]), debugging.token);
		const pause = await waitFor('test body breakpoint', () => stopped, 180000);
		const stack = await pause.session.customRequest('stackTrace', { threadId: pause.threadId });
		assert.equal(stack.stackFrames[0].source.path, uri.fsPath);
		assert.match(stack.stackFrames[0].name, /test@10:/);
		await pause.session.customRequest('continue', { threadId: pause.threadId });
		assert.deepEqual(await result, { passed: 1, failed: 0, errored: 0, skipped: 0 });
		console.log('VS Code: Debug test profile, test body breakpoint, and result passed.');
	} finally {
		vscode.debug.removeBreakpoints([testBreakpoint]);
		testTracker.dispose();
		debugging.dispose();
		if (vscode.debug.activeDebugSession) { await vscode.debug.stopDebugging(); }
	}

	// G39: Debug Project reports E1036 of its build until the project changes.
	await writeFile(manifest, denied);
	assert.equal(await vscode.commands.executeCommand('tsuzuri.debug', limitedUri), false);
	assert.equal(copies(limitedUri, 'E1036').length, 1);
	await editLimited(`${limitedText}\n`);
	await cleared('Debug Project result cleared by an edit', limitedUri);
	console.log('VS Code: Debug Project reports E1036 of its build until an edit.');
	await removeFolder();
	console.log('All Tsuzuri VS Code integration checks passed.');
}
