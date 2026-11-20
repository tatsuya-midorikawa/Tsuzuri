import { createHash } from 'node:crypto';
import { realpath, rm } from 'node:fs/promises';
import * as path from 'node:path';
import * as vscode from 'vscode';
import { directoryExists, excludedPattern, exists, jsonLines, lldbLaunch, plainPath, rootSpelling, supportsDebug, testDebugArguments, testRunnerFiles, testRunnerPath } from './core';
import { runCompiler } from './toolchain';
import { directoryRoot, documentHasProject, formatters, projectFor, projectRootFor, projectRoots, reportError, RunStart } from './workflow';

interface TestCase {
	index: number;
	name: string;
	module: string;
	path: string;
	range: { start: { line: number; character: number }; end: { line: number; character: number } };
}

/** Numbers the debug runs of this extension host, which name their runners (`testRunnerPath`). */
let debugRuns = 0;

/** A failed `tsuzuri test --list --json`, whose diagnostics an explicit run or debug reports. */
class DiscoveryError extends Error {
	constructor(readonly stderr: string) { super(stderr || 'Test discovery failed.'); }
}

export function registerTesting(context: vscode.ExtensionContext, output: vscode.OutputChannel, workflow: {
	begin: (roots: readonly string[]) => RunStart; track: (root: string) => void; publishDiagnostics: (root: string, text: string, start: RunStart) => Promise<void>; ensureDebugger: () => Promise<void>;
}) {
	const { publishDiagnostics } = workflow;
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
				// Discovery reads the files on disk and reports no diagnostics (G39 D1); the language server reports the
				// same errors as the user types, and an explicit run or debug reports this failure as its result.
				if (result.code !== 0) { throw new DiscoveryError(result.stderr); }
				const records = jsonLines(result.stdout);
				const real = await realpath(root).catch(() => root);
				const revision = createHash('sha256').update(result.stdout).digest('hex').slice(0, 16);
				const files = new Map<string, vscode.TestItem>();
				for (const [id, data] of cases) { if (data.root === root) { cases.delete(id); } }
				for (const record of records) {
					if (record.type !== 'test' || !Number.isInteger(record.index) || typeof record.name !== 'string' || typeof record.path !== 'string' || !record.range) {
						throw new Error('Invalid test discovery response.');
					}
					const testCase = record as unknown as TestCase;
					// The compiler names files by their canonical paths: verbatim on Windows, and with symbolic links resolved.
					const source = rootSpelling(root, real, path.resolve(root, plainPath(testCase.path)));
					const uri = vscode.Uri.file(source);
					let file = files.get(source);
					if (!file) {
						file = controller.createTestItem(`${root}:${source}`, testCase.module, uri);
						files.set(source, file);
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

	/**
	 * Discovers the tests of `root` for an explicit run or debug that started at `start` (taken right after its saveAll),
	 * which reports a failed discovery as its result. The same start stands for the compilation that follows: an edit
	 * after the saveAll (while the roots refresh, a discovery or an earlier root runs, or the debugger installs) makes the
	 * result stale, as the compiler reads the saved files.
	 */
	async function discoverForRun(root: string, signal: AbortSignal, start: RunStart) {
		workflow.track(root);
		// A discovery that started before the run's saveAll may have read older files.
		await refreshes.get(root)?.catch(() => undefined);
		// A root that stopped being one meanwhile (G40 D4), here or while an earlier root ran, has nothing to run;
		// discovering it would add it back.
		if (!roots.has(root)) { return; }
		try { await discover(root, signal); } catch (error) {
			if (error instanceof DiscoveryError) { await publishDiagnostics(root, error.stderr, start); }
			throw error;
		}
	}

	/**
	 * The roots of the workspace folders and of the open documents, except a document left open in a folder that was
	 * deleted or moved, whose root would be a directory that does not exist or one above the deleted folder (G40 D4).
	 */
	async function usedRoots(): Promise<Set<string>> {
		const used = new Set<string>();
		for (const folder of vscode.workspace.workspaceFolders ?? []) {
			if (await exists(path.join(folder.uri.fsPath, 'Main.tz')) || await exists(path.join(folder.uri.fsPath, 'Tsuzuri.toml'))
				|| vscode.workspace.getConfiguration('tsuzuri', folder.uri).get<string>('projectPath')) {
				used.add(await projectFor(folder.uri));
			} else {
				const sources = await vscode.workspace.findFiles(new vscode.RelativePattern(folder, '*.{tz,tc,tt}'), excludedPattern, 1);
				if (sources.length) { used.add(folder.uri.fsPath); }
			}
		}
		for (const document of vscode.workspace.textDocuments) {
			if (document.languageId === 'tsuzuri' && document.uri.scheme === 'file') {
				const found = await projectRootFor(document.uri);
				if (await documentHasProject(document.uri, found)) { used.add(found.root); }
			}
		}
		return used;
	}

	/**
	 * Adds the roots that are used. A lookup that a change cleared the cache during is read again, and the roots are added
	 * in the same task as the last check, so that a root read from older files does not come back after a re-rooting
	 * removed it (G40 D4).
	 */
	async function refreshRoots() {
		await projectRoots.settled(usedRoots, used => { for (const root of used) { addProject(root); } });
	}

	function removeProject(root: string) {
		controller.items.delete(root);
		roots.delete(root);
		clearTimeout(timers.get(root));
		timers.delete(root);
		for (const [id, data] of cases) { if (data.root === root) { cases.delete(id); } }
	}

	/**
	 * Rebuilds the roots after files below a workspace folder were created or deleted (G40 D4): a root that neither a
	 * workspace folder nor an open document gives, and that is no longer the root of the files directly in it or whose
	 * folder was deleted or moved, goes, and the new roots come, both at once. The extension calls it after it clears the
	 * cache of roots, so no root is found from old files.
	 */
	async function reroot() {
		const used = await usedRoots();
		const stale: string[] = [];
		for (const root of [...roots.keys()]) {
			if (!used.has(root) && (!(await directoryExists(root)) || (await directoryRoot(root)).root !== root)) { stale.push(root); }
		}
		stale.forEach(removeProject);
		for (const root of used) { addProject(root); }
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
					// A root removed while the earlier discovery ran (G40 D4) is not discovered, which would add it back.
					if (roots.has(root)) { await discover(root); }
				})().catch(error => output.appendLine(String(error)));
			}, 300));
		} catch (error) { output.appendLine(String(error)); }
	};
	const watcher = vscode.workspace.createFileSystemWatcher('**/*.{tz,tt,tc,toml}');
	context.subscriptions.push(watcher, watcher.onDidCreate(changed), watcher.onDidChange(changed), watcher.onDidDelete(changed),
		new vscode.Disposable(() => { for (const timer of timers.values()) { clearTimeout(timer); } }),
		vscode.workspace.onDidChangeWorkspaceFolders(event => {
			for (const [root] of roots) {
				if (event.removed.some(folder => root === folder.uri.fsPath || root.startsWith(folder.uri.fsPath + path.sep))) { removeProject(root); }
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

	/** The roots that `request` includes, or every root. */
	function requestedRoots(request: vscode.TestRunRequest): Set<string> {
		const result = new Set<string>();
		for (const item of request.include ?? [...roots.values()]) {
			let parent = item;
			while (parent.parent) { parent = parent.parent; }
			if (roots.has(parent.id)) { result.add(parent.id); }
		}
		return result;
	}

	/** The tests of the discovered `root` that `request` selects, without its exclusions. */
	function selection(request: vscode.TestRunRequest, root: string): vscode.TestItem[] {
		// A root that stopped being one while the run discovered it (G40 D4) has nothing to run.
		const project = roots.get(root);
		if (!project) { return []; }
		const excluded = new Set((request.exclude ?? []).flatMap(item => leaves(item).map(leaf => leaf.id)));
		const included = request.include ? request.include.flatMap(item => {
			if (item.id.startsWith(`${root}#`)) {
				if (!cases.has(item.id)) { throw new Error('Test declarations changed. Refresh Tests before selecting individual tests.'); }
				return [item];
			}
			if (item.id === root) { return leaves(project); }
			const current = project.children.get(item.id);
			return current ? leaves(current) : [];
		}) : leaves(project);
		return [...new Map(included.filter(item => !excluded.has(item.id) && cases.get(item.id)?.root === root).map(item => [item.id, item])).values()];
	}

	async function run(request: vscode.TestRunRequest, token: vscode.CancellationToken, optimization: number) {
		const execution = controller.createTestRun(request);
		const summary = { passed: 0, failed: 0, errored: 0, skipped: 0 };
		const abort = new AbortController();
		const subscription = token.onCancellationRequested(() => abort.abort());
		const pending = new Set<vscode.TestItem>();
		try {
			if (!await vscode.workspace.saveAll(false)) { return summary; }
			const start = workflow.begin([...roots.keys()]);
			await refreshRoots();
			for (const root of requestedRoots(request)) {
				if (token.isCancellationRequested) { break; }
				await discoverForRun(root, abort.signal, start);
				const selected = selection(request, root);
				if (!selected.length) { continue; }
				const byIndex = new Map(selected.map(item => [cases.get(item.id)!.index, item]));
				for (const item of selected) { execution.enqueued(item); execution.started(item); pending.add(item); }
				const args = ['test', root, '--json', `-O${optimization}`,
					...selected.flatMap(item => ['--index', String(cases.get(item.id)!.index)])];
				const result = await runCompiler(context, root, args, { signal: abort.signal });
				execution.appendOutput((result.stdout + result.stderr).replace(/\r?\n/g, '\r\n'));
				await publishDiagnostics(root, result.stderr, start);
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

	/**
	 * Debugs one test (G16 Phase 2): the compiler builds a runner of that test alone with debug information, and
	 * CodeLLDB starts it with the Tsuzuri formatters, as for Debug Project. The test passes when the runner exits 0.
	 */
	async function debug(request: vscode.TestRunRequest, token: vscode.CancellationToken) {
		const execution = controller.createTestRun(request);
		const summary = { passed: 0, failed: 0, errored: 0, skipped: 0 };
		const abort = new AbortController();
		const subscription = token.onCancellationRequested(() => abort.abort());
		let item: vscode.TestItem | undefined;
		let runner: string | undefined;
		try {
			if (!await vscode.workspace.saveAll(false)) { return summary; }
			const start = workflow.begin([...roots.keys()]);
			await refreshRoots();
			const selected: vscode.TestItem[] = [];
			for (const root of requestedRoots(request)) {
				await discoverForRun(root, abort.signal, start);
				selected.push(...selection(request, root));
			}
			if (selected.length !== 1) {
				for (const other of selected) { execution.skipped(other); summary.skipped++; }
				throw new Error(`Select one test to debug; ${selected.length} tests are selected.`);
			}
			item = selected[0];
			const { root, index } = cases.get(item.id)!;
			execution.enqueued(item);
			execution.started(item);
			await workflow.ensureDebugger();
			const run = `${process.pid}-${++debugRuns}`;
			runner = testRunnerPath(root, run);
			const result = await runCompiler(context, root, testDebugArguments(root, index, run), { signal: abort.signal });
			execution.appendOutput((result.stdout + result.stderr).replace(/\r?\n/g, '\r\n'));
			await publishDiagnostics(root, result.stderr, start);
			const built = jsonLines(result.stdout).find(record => record.type === 'debug');
			if (result.code !== 0 || !built || typeof built.program !== 'string' || !Array.isArray(built.arguments)) {
				throw new Error(result.stderr || 'The test debug build failed.');
			}
			const configuration = {
				...lldbLaunch(built.program, formatters(context)), name: `Debug Test: ${item.label}`,
				args: built.arguments.map(String), cwd: root,
			};
			const exitCode = await debugSession(vscode.workspace.getWorkspaceFolder(vscode.Uri.file(root)), configuration, execution, token);
			if (exitCode === 0) { execution.passed(item); summary.passed++; }
			else if (exitCode === undefined) { execution.skipped(item); summary.skipped++; }
			else {
				const message = new vscode.TestMessage(`The test exited with code ${exitCode}.`);
				if (item.uri && item.range) { message.location = new vscode.Location(item.uri, item.range); }
				execution.failed(item, message);
				summary.failed++;
			}
		} catch (error) {
			execution.appendOutput(`${String(error)}\r\n`);
			// Like a run, a debug session cancelled while it builds or starts skips its test.
			if (token.isCancellationRequested) {
				if (item) { execution.skipped(item); summary.skipped++; }
			} else {
				if (item) { execution.errored(item, new vscode.TestMessage(String(error))); }
				summary.errored++;
			}
		} finally {
			subscription.dispose();
			execution.end();
			// The session has ended, so nothing holds the runner; a file that cannot be removed only stays behind.
			if (runner) { await Promise.all(testRunnerFiles(runner).map(file => rm(file, { force: true }).catch(() => undefined))); }
		}
		return summary;
	}

	/** Starts `configuration` for `execution` and resolves with the runner's exit code, or undefined if it did not exit. */
	function debugSession(folder: vscode.WorkspaceFolder | undefined, configuration: vscode.DebugConfiguration,
		execution: vscode.TestRun, token: vscode.CancellationToken): Promise<number | undefined> {
		const marker = `${Date.now()}-${Math.random()}`;
		if (token.isCancellationRequested) { return Promise.resolve(undefined); }
		return new Promise((resolve, reject) => {
			let session: vscode.DebugSession | undefined;
			let exitCode: number | undefined;
			const ours = (candidate: vscode.DebugSession) => candidate.configuration.tsuzuriTest === marker;
			const subscriptions = [
				vscode.debug.registerDebugAdapterTrackerFactory('lldb', {
					createDebugAdapterTracker: candidate => ours(candidate) ? {
						onDidSendMessage(message) {
							if (message.type === 'event' && message.event === 'exited') { exitCode = message.body?.exitCode; }
						},
					} : undefined,
				}),
				vscode.debug.onDidStartDebugSession(candidate => {
					if (!ours(candidate)) { return; }
					session = candidate;
					// A cancellation before CodeLLDB reported the session found nothing to stop.
					if (token.isCancellationRequested) { void vscode.debug.stopDebugging(candidate); }
				}),
				vscode.debug.onDidTerminateDebugSession(candidate => {
					if (ours(candidate)) { finish(); resolve(exitCode); }
				}),
				token.onCancellationRequested(() => { if (session) { void vscode.debug.stopDebugging(session); } }),
			];
			const finish = () => { for (const subscription of subscriptions) { subscription.dispose(); } };
			vscode.debug.startDebugging(folder, { ...configuration, tsuzuriTest: marker }, { testRun: execution }).then(started => {
				if (!started) { finish(); reject(new Error('CodeLLDB could not start the test.')); }
			}, error => { finish(); reject(error); });
		});
	}

	controller.createRunProfile('Native (O0)', vscode.TestRunProfileKind.Run, async (request, token) => { await run(request, token, 0); }, true);
	controller.createRunProfile('Native (O3)', vscode.TestRunProfileKind.Run, async (request, token) => { await run(request, token, 3); });
	if (supportsDebug()) {
		controller.createRunProfile('Debug', vscode.TestRunProfileKind.Debug, async (request, token) => { await debug(request, token); }, true);
	}
	context.subscriptions.push(
		vscode.workspace.onDidOpenTextDocument(document => {
			if (document.languageId === 'tsuzuri' && document.uri.scheme === 'file') {
				void projectRoots.settled(() => projectFor(document.uri), addProject).catch(error => output.appendLine(String(error)));
			}
		}),
		vscode.commands.registerCommand('tsuzuri.test', async () => {
			const cancellation = new vscode.CancellationTokenSource();
			try {
				await vscode.commands.executeCommand('workbench.view.testing.focus');
				// The root of the active file is read again when a change cleared the cache meanwhile (G40 D4). Without one,
				// the root is a workspace folder's, which the cache does not decide, and reading it again would ask the user
				// to pick the folder again.
				const active = vscode.window.activeTextEditor?.document.uri;
				const project = active?.scheme === 'file' ? await projectRoots.settled(() => projectFor(active), addProject)
					: addProject(await projectFor());
				return await run(new vscode.TestRunRequest([project]), cancellation.token, 0);
			} catch (error) { reportError(error, output); return undefined; }
			finally { cancellation.dispose(); }
		}),
	);
	void refreshRoots().catch(error => output.appendLine(String(error)));
	return { controller, discover, roots, run, debug, reroot };
}
