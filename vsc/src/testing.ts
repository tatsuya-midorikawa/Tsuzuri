import { createHash } from 'node:crypto';
import * as path from 'node:path';
import * as vscode from 'vscode';
import { excludedPattern, exists, jsonLines } from './core';
import { runCompiler } from './toolchain';
import { projectFor, reportError } from './workflow';

interface TestCase {
	index: number;
	name: string;
	module: string;
	path: string;
	range: { start: { line: number; character: number }; end: { line: number; character: number } };
}

export function registerTesting(context: vscode.ExtensionContext, output: vscode.OutputChannel,
	publishDiagnostics: (root: string, text: string) => Promise<void>) {
	const controller = vscode.tests.createTestController('tsuzuri', 'Tsuzuri');
	context.subscriptions.push(controller);
	const roots = new Map<string, vscode.TestItem>();
	const cases = new Map<string, { root: string; index: number }>();
	const refreshes = new Map<string, Promise<void>>();
	const timers = new Map<string, NodeJS.Timeout>();

	function addProject(root: string) {
		if (!roots.has(root)) {
			const item = controller.createTestItem(root, path.basename(root), vscode.Uri.file(root));
			item.canResolveChildren = true;
			controller.items.add(item);
			roots.set(root, item);
		}
		return roots.get(root)!;
	}

	async function discover(root: string, signal?: AbortSignal) {
		if (refreshes.has(root)) { return refreshes.get(root); }
		const project = addProject(root);
		const refresh = (async () => {
			project.busy = true;
			try {
				const result = await runCompiler(context, root, ['test', root, '--list', '--json'], { signal, timeout: 60000 });
				if (result.code !== 0) {
					await publishDiagnostics(root, result.stderr);
					throw new Error(result.stderr || 'Test discovery failed.');
				}
				const records = jsonLines(result.stdout);
				const revision = createHash('sha256').update(result.stdout).digest('hex').slice(0, 16);
				const files = new Map<string, vscode.TestItem>();
				for (const [id, data] of cases) { if (data.root === root) { cases.delete(id); } }
				for (const record of records) {
					if (record.type !== 'test' || !Number.isInteger(record.index) || typeof record.name !== 'string' || typeof record.path !== 'string' || !record.range) {
						throw new Error('Invalid test discovery response.');
					}
					const testCase = record as unknown as TestCase;
					const uri = vscode.Uri.file(testCase.path);
					let file = files.get(testCase.path);
					if (!file) {
						file = controller.createTestItem(`${root}:${testCase.path}`, testCase.module, uri);
						files.set(testCase.path, file);
					}
					const id = `${root}#${revision}:${testCase.index}`;
					const item = controller.createTestItem(id, testCase.name || '(unnamed test)', uri);
					item.range = new vscode.Range(testCase.range.start.line, testCase.range.start.character, testCase.range.end.line, testCase.range.end.character);
					file.children.add(item);
					cases.set(id, { root, index: testCase.index });
				}
				project.children.replace([...files.values()]);
				project.error = undefined;
			} catch (error) {
				project.error = String(error);
				throw error;
			} finally { project.busy = false; }
		})();
		refreshes.set(root, refresh);
		try { await refresh; } finally { refreshes.delete(root); }
	}

	async function refreshRoots() {
		for (const folder of vscode.workspace.workspaceFolders ?? []) {
			if (await exists(path.join(folder.uri.fsPath, 'Main.tz')) || await exists(path.join(folder.uri.fsPath, 'Tsuzuri.toml'))
				|| vscode.workspace.getConfiguration('tsuzuri', folder.uri).get<string>('projectPath')) {
				addProject(await projectFor(folder.uri));
			} else {
				const sources = await vscode.workspace.findFiles(new vscode.RelativePattern(folder, '*.{tz,tc,tt}'), excludedPattern, 1);
				if (sources.length) { addProject(folder.uri.fsPath); }
			}
		}
		for (const document of vscode.workspace.textDocuments) {
			if (document.languageId === 'tsuzuri' && document.uri.scheme === 'file') { addProject(await projectFor(document.uri)); }
		}
	}

	const changed = async (uri: vscode.Uri) => {
		try {
			const root = await projectFor(uri);
			if (!roots.has(root)) { return; }
			clearTimeout(timers.get(root));
			timers.set(root, setTimeout(() => {
				timers.delete(root);
				void (async () => {
					await refreshes.get(root)?.catch(() => undefined);
					await discover(root);
				})().catch(error => output.appendLine(String(error)));
			}, 300));
		} catch (error) { output.appendLine(String(error)); }
	};
	const watcher = vscode.workspace.createFileSystemWatcher('**/*.{tz,tt,tc,toml}');
	context.subscriptions.push(watcher, watcher.onDidCreate(changed), watcher.onDidChange(changed), watcher.onDidDelete(changed),
		new vscode.Disposable(() => { for (const timer of timers.values()) { clearTimeout(timer); } }),
		vscode.workspace.onDidChangeWorkspaceFolders(event => {
			for (const [root] of roots) {
				if (event.removed.some(folder => root === folder.uri.fsPath || root.startsWith(folder.uri.fsPath + path.sep))) {
					controller.items.delete(root);
					roots.delete(root);
					clearTimeout(timers.get(root));
					timers.delete(root);
					for (const [id, data] of cases) { if (data.root === root) { cases.delete(id); } }
				}
			}
			void refreshRoots().catch(error => output.appendLine(String(error)));
		}),
	);

	controller.resolveHandler = async item => {
		try {
			if (item && roots.has(item.id)) { await discover(item.id); } else { await refreshRoots(); }
		} catch (error) { output.appendLine(String(error)); }
	};
	controller.refreshHandler = async token => {
		const abort = new AbortController();
		const subscription = token.onCancellationRequested(() => abort.abort());
		try {
			await refreshRoots();
			for (const root of roots.keys()) {
				if (token.isCancellationRequested) { break; }
				await discover(root, abort.signal);
			}
		} catch (error) { if (!token.isCancellationRequested) { reportError(error, output); } }
		finally { subscription.dispose(); }
	};

	function leaves(item: vscode.TestItem): vscode.TestItem[] {
		if (cases.has(item.id)) { return [item]; }
		const result: vscode.TestItem[] = [];
		item.children.forEach(child => result.push(...leaves(child)));
		return result;
	}

	async function run(request: vscode.TestRunRequest, token: vscode.CancellationToken, optimization: number) {
		const execution = controller.createTestRun(request);
		const summary = { passed: 0, failed: 0, errored: 0, skipped: 0 };
		const abort = new AbortController();
		const subscription = token.onCancellationRequested(() => abort.abort());
		const pending = new Set<vscode.TestItem>();
		try {
			if (!await vscode.workspace.saveAll(false)) { return summary; }
			await refreshRoots();
			const requestedRoots = new Set<string>();
			for (const item of request.include ?? [...roots.values()]) {
				let parent = item;
				while (parent.parent) { parent = parent.parent; }
				if (roots.has(parent.id)) { requestedRoots.add(parent.id); }
			}
			for (const root of requestedRoots) {
				if (token.isCancellationRequested) { break; }
				await discover(root, abort.signal);
				const excluded = new Set((request.exclude ?? []).flatMap(item => leaves(item).map(leaf => leaf.id)));
				const selection = request.include ? request.include.flatMap(item => {
					if (item.id.startsWith(`${root}#`)) {
						if (!cases.has(item.id)) { throw new Error('Test declarations changed. Refresh Tests before selecting individual tests.'); }
						return [item];
					}
					if (item.id === root) { return leaves(roots.get(root)!); }
					const current = roots.get(root)!.children.get(item.id);
					return current ? leaves(current) : [];
				}) : leaves(roots.get(root)!);
				const selected = [...new Map(selection.filter(item => !excluded.has(item.id) && cases.get(item.id)?.root === root).map(item => [item.id, item])).values()];
				if (!selected.length) { continue; }
				const byIndex = new Map(selected.map(item => [cases.get(item.id)!.index, item]));
				for (const item of selected) { execution.enqueued(item); execution.started(item); pending.add(item); }
				const args = ['test', root, '--json', `-O${optimization}`,
					...selected.flatMap(item => ['--index', String(cases.get(item.id)!.index)])];
				const result = await runCompiler(context, root, args, { signal: abort.signal });
				execution.appendOutput((result.stdout + result.stderr).replace(/\r?\n/g, '\r\n'));
				await publishDiagnostics(root, result.stderr);
				for (const record of jsonLines(result.stdout)) {
					if (record.type !== 'test' || typeof record.index !== 'number') { continue; }
					const item = byIndex.get(record.index);
					if (!item) { continue; }
					const duration = typeof record.duration_ms === 'number' ? record.duration_ms : undefined;
					if (record.status === 'passed') { execution.passed(item, duration); summary.passed++; }
					else {
						const message = new vscode.TestMessage(String(record.failure ?? 'Test failed.'));
						if (item.uri && item.range) { message.location = new vscode.Location(item.uri, item.range); }
						execution.failed(item, message, duration);
						summary.failed++;
					}
					pending.delete(item);
				}
				for (const item of selected) {
					if (pending.delete(item)) {
						execution.errored(item, new vscode.TestMessage(result.stderr || `Compiler returned ${result.code} without a test result.`));
						summary.errored++;
					}
				}
			}
		} catch (error) {
			execution.appendOutput(`${String(error)}\r\n`);
			if (!pending.size && !token.isCancellationRequested) { summary.errored++; }
			for (const item of pending) {
				if (token.isCancellationRequested) { execution.skipped(item); summary.skipped++; }
				else { execution.errored(item, new vscode.TestMessage(String(error))); summary.errored++; }
			}
		} finally { subscription.dispose(); execution.end(); }
		return summary;
	}

	controller.createRunProfile('Native (O0)', vscode.TestRunProfileKind.Run, async (request, token) => { await run(request, token, 0); }, true);
	controller.createRunProfile('Native (O3)', vscode.TestRunProfileKind.Run, async (request, token) => { await run(request, token, 3); });
	context.subscriptions.push(
		vscode.workspace.onDidOpenTextDocument(document => {
			if (document.languageId === 'tsuzuri' && document.uri.scheme === 'file') {
				void projectFor(document.uri).then(addProject).catch(error => output.appendLine(String(error)));
			}
		}),
		vscode.commands.registerCommand('tsuzuri.test', async () => {
			const cancellation = new vscode.CancellationTokenSource();
			try {
				await vscode.commands.executeCommand('workbench.view.testing.focus');
				const root = await projectFor();
				const project = addProject(root);
				return await run(new vscode.TestRunRequest([project]), cancellation.token, 0);
			} catch (error) { reportError(error, output); return undefined; }
			finally { cancellation.dispose(); }
		}),
	);
	void refreshRoots().catch(error => output.appendLine(String(error)));
	return { controller, discover, roots, run };
}
