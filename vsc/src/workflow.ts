import { readdir, readFile, realpath, stat } from 'node:fs/promises';
import * as path from 'node:path';
import * as vscode from 'vscode';
import { Action, affectsRoot, commandArguments, defaultNamespace, exists, inRoot, isNamespace, jsonLines, lldbLaunch, outputPath, plainPath, projectFile, projectRoot, reportedByServer, rootSpelling, sourceFiles, sourcePosition, supportsDebug } from './core';
import { runCompiler, toolchain } from './toolchain';

/**
 * A run of the compiler in a project root (`begin`): the root's change numbers just before it started, which its result
 * must match, and the time it started, which no file that a watcher reports written may follow.
 */
export interface RunStart {
	epoch: number;
	started: number;
}

/**
 * The result of the last test run, test debug, or Debug Project of a project root (G39 D2): its URIs, whether one is
 * outside the root (a path dependency), and the start of the run that produced it. The numbers of the root's last
 * change and of its last change outside the root count the changes that have no time (an edit in the editor, a
 * deletion); the newest times of a file that a watcher reported written under the root and outside it count the others.
 * Each run compares them with its own start, not with the root's latest run, which may stop before it has a result.
 */
interface RootResult {
	uris: vscode.Uri[];
	external: boolean;
	shown: number;
	changed: number;
	outside: number;
	written: number;
	writtenOutside: number;
	watcher: vscode.FileSystemWatcher;
}

export async function projectFor(resource?: vscode.Uri): Promise<string> {
	resource ??= vscode.window.activeTextEditor?.document.uri;
	let folder = resource ? vscode.workspace.getWorkspaceFolder(resource) : undefined;
	if (!resource || resource.scheme !== 'file') {
		folder = vscode.workspace.workspaceFolders?.length === 1 ? vscode.workspace.workspaceFolders[0]
			: await vscode.window.showWorkspaceFolderPick({ placeHolder: 'Select a Tsuzuri project' });
		resource = folder?.uri;
	}
	if (!resource) { throw new Error('Open a Tsuzuri folder or source file first.'); }
	const configured = vscode.workspace.getConfiguration('tsuzuri', resource).get<string>('projectPath');
	if (configured) {
		return path.resolve(folder?.uri.fsPath ?? path.dirname(resource.fsPath), configured);
	}
	return projectRoot(resource.fsPath, folder?.uri.fsPath);
}

/** The Tsuzuri LLDB formatters that the extension ships. */
export function formatters(context: vscode.ExtensionContext): string {
	return context.asAbsolutePath(path.join('resources', 'lldb', 'tsuzuri_lldb.py'));
}

export function reportError(error: unknown, output: vscode.OutputChannel) {
	output.appendLine(String(error));
	void vscode.window.showErrorMessage(String(error));
}

export function registerWorkflow(context: vscode.ExtensionContext, output: vscode.OutputChannel) {
	const diagnostics = vscode.languages.createDiagnosticCollection('tsuzuri-build');
	context.subscriptions.push(diagnostics);
	/** The result of each project root, until the project changes. */
	const roots = new Map<string, RootResult>();
	/**
	 * Numbers every change. A run keeps the number reached at its saveAll (`epoch`), and a change numbered after it in
	 * its root (or outside it, for a result there) makes its result stale.
	 */
	let changes = 0;
	/** The number of the last removal of each root's workspace folder, so a run that started before it shows nothing. */
	const removed = new Map<string, number>();
	function rootState(root: string): RootResult {
		let state = roots.get(root);
		if (!state) {
			const created: RootResult = {
				uris: [], external: false, shown: -Infinity, changed: changes, outside: changes, written: -Infinity, writtenOutside: -Infinity,
				// Every deletion under the root (a folder that is deleted or moved arrives as one event of the folder), and the
				// sources created or written under it, which the workspace's watcher misses for a root outside the workspace
				// folders (`tsuzuri.projectPath`, an absolute `project` of a debug configuration).
				watcher: vscode.workspace.createFileSystemWatcher(new vscode.RelativePattern(root, '**')),
			};
			created.watcher.onDidDelete(uri => { if (affectsRoot(root, uri.fsPath)) { created.changed = ++changes; clear(created); } });
			const write = (uri: vscode.Uri) => { if (projectFile(uri.fsPath) && affectsRoot(root, uri.fsPath)) { stamped(uri); } };
			created.watcher.onDidCreate(write);
			created.watcher.onDidChange(write);
			roots.set(root, state = created);
		}
		return state;
	}
	function clear(state: RootResult) {
		for (const uri of state.uris) { diagnostics.delete(uri); }
		state.uris = [];
		state.external = false;
	}
	/** A change outside the root, which every source outside the root can cause in a result there (G39 spec 3). */
	function changedOutside(state: RootResult) {
		state.outside = ++changes;
		if (state.external) { clear(state); }
	}
	/**
	 * An edit of `file` in the editor (G39 spec 3) changes the roots that contain it, and when it is a Tsuzuri source, also
	 * the results of the other roots that point outside their root.
	 */
	function edited(file: string, source: boolean) {
		for (const [root, state] of roots) {
			if (inRoot(root, file)) {
				state.changed = ++changes;
				clear(state);
			} else if (source) { changedOutside(state); }
		}
	}
	/**
	 * A source that a watcher reports created or written at `time` makes stale the results of the runs that started
	 * before it, whether shown or still running; it is not a change for a run that started after it, which read it. The
	 * saveAll before a run can reach the watcher after the run has started, or after its result is shown.
	 */
	function written(file: string, time: number) {
		for (const [root, state] of roots) {
			if (inRoot(root, file)) {
				state.written = Math.max(state.written, time);
				if (time > state.shown) { clear(state); }
			} else {
				state.writtenOutside = Math.max(state.writtenOutside, time);
				if (state.external && time > state.shown) { clear(state); }
			}
		}
	}
	/**
	 * Records the start of runs of the compiler in `roots`, right after the saveAll that precedes them (G39 spec 3): an edit
	 * after it leaves the saved files that the compiler reads behind the editor. `publishDiagnostics` takes the result; a
	 * root that the run finds later (`track`) shares the start.
	 */
	function begin(roots: readonly string[]): RunStart {
		for (const root of roots) { rootState(root); }
		return { epoch: changes, started: Date.now() };
	}
	/**
	 * Starts following the changes of `root`, a root of a run that found it after its start. The edits and deletions
	 * before a root is followed are not recorded for it, so a new root starts at the number of the last change anywhere:
	 * a run that started before that change shows no result in it (an unrelated change only hides a result early).
	 */
	function track(root: string) { rootState(root); }
	// VS Code 1.103 on macOS can report the saveAll before a run as a creation, so a creation counts by the file's times
	// like a change. The status change time is new after a move, which keeps the modification time.
	const stamped = (uri: vscode.Uri) => {
		void stat(uri.fsPath).then(info => Math.trunc(Math.max(info.mtimeMs, info.ctimeMs)), () => Date.now())
			.then(time => written(uri.fsPath, time));
	};
	const sources = vscode.workspace.createFileSystemWatcher(`**/${sourceFiles}`);
	// Each root's own watcher sees the deletions under it; this one sees those outside it, where a path dependency's
	// deleted folder also arrives as one event. Names that module discovery does not read are left out, as under a root.
	const removals = vscode.workspace.createFileSystemWatcher('**', true, true, false);
	context.subscriptions.push(sources, sources.onDidCreate(stamped), sources.onDidChange(stamped), removals,
		removals.onDidDelete(uri => {
			const folder = vscode.workspace.getWorkspaceFolder(uri)?.uri.fsPath;
			if (folder && !affectsRoot(folder, uri.fsPath)) { return; }
			for (const [root, state] of roots) {
				if (!inRoot(root, uri.fsPath)) { changedOutside(state); }
			}
		}),
		vscode.workspace.onDidChangeTextDocument(event => {
			const document = event.document.uri;
			if (event.contentChanges.length && document.scheme === 'file') { edited(document.fsPath, projectFile(document.fsPath)); }
		}),
		vscode.workspace.onDidChangeWorkspaceFolders(event => {
			for (const [root, state] of roots) {
				if (event.removed.some(folder => inRoot(folder.uri.fsPath, root))) {
					removed.set(root, ++changes);
					clear(state);
					state.watcher.dispose();
					roots.delete(root);
				}
			}
		}),
		new vscode.Disposable(() => { for (const state of roots.values()) { state.watcher.dispose(); } }),
	);
	let debuggerInstallation: PromiseLike<void> | undefined;
	async function ensureDebugger() {
		if (!supportsDebug()) {
			throw new Error('Source debugging is not yet available on Windows ARM64. Editing, checking, building, running, and testing are supported.');
		}
		if (!vscode.extensions.getExtension('vadimcn.vscode-lldb')) {
			const archive = vscode.Uri.joinPath(context.extensionUri, 'resources', 'codelldb.vsix').fsPath;
			const source = await exists(archive) ? vscode.Uri.file(archive) : 'vadimcn.vscode-lldb';
			debuggerInstallation ??= vscode.window.withProgress({ location: vscode.ProgressLocation.Notification, title: 'Tsuzuri: Installing CodeLLDB' },
				async () => { await vscode.commands.executeCommand('workbench.extensions.installExtension', source); }).then(() => undefined);
			try { await debuggerInstallation; } finally { debuggerInstallation = undefined; }
		}
		let debuggerExtension = vscode.extensions.getExtension('vadimcn.vscode-lldb');
		if (!debuggerExtension) {
			await new Promise<void>(resolve => {
				let subscription: vscode.Disposable | undefined;
				let timeout: ReturnType<typeof setTimeout> | undefined;
				let settled = false;
				const finish = () => {
					if (settled) { return; }
					settled = true;
					if (timeout) { clearTimeout(timeout); }
					subscription?.dispose();
					resolve();
				};
				subscription = vscode.extensions.onDidChange(() => {
					if (vscode.extensions.getExtension('vadimcn.vscode-lldb')) { finish(); }
				});
				timeout = setTimeout(finish, 10000);
				debuggerExtension = vscode.extensions.getExtension('vadimcn.vscode-lldb');
				if (debuggerExtension) { finish(); }
			});
			debuggerExtension = vscode.extensions.getExtension('vadimcn.vscode-lldb');
		}
		if (!debuggerExtension) { throw new Error('CodeLLDB could not be installed. See Tsuzuri Output and retry Debug.'); }
		await debuggerExtension.activate();
	}

	/** Shows the diagnostics of the run of `root` that started at `start` in place of its previous result. */
	async function publishDiagnostics(root: string, text: string, start: RunStart) {
		const grouped = new Map<string, vscode.Diagnostic[]>();
		// The compiler names files by their canonical paths: verbatim on Windows, and with symbolic links resolved.
		const real = await realpath(root).catch(() => root);
		for (const record of jsonLines(text)) {
			if (typeof record.path !== 'string' || typeof record.message !== 'string') { continue; }
			const file = rootSpelling(root, real, path.resolve(root, plainPath(record.path)));
			let range = new vscode.Range(0, 0, 0, 0);
			try {
				const source = await readFile(file);
				const span = record.span as { start?: number; end?: number } | undefined;
				const from = sourcePosition(source, span?.start ?? 0);
				const to = sourcePosition(source, span?.end ?? span?.start ?? 0);
				range = new vscode.Range(from.line, from.character, to.line, to.character);
			} catch { /* A file that cannot be read keeps its diagnostic at the start. */ }
			const diagnostic = new vscode.Diagnostic(range, record.message,
				record.severity === 'warning' ? vscode.DiagnosticSeverity.Warning : vscode.DiagnosticSeverity.Error);
			diagnostic.source = 'Tsuzuri';
			diagnostic.code = typeof record.code === 'string' ? record.code : undefined;
			const entries = grouped.get(file) ?? [];
			entries.push(diagnostic);
			grouped.set(file, entries);
		}
		// A change since the run started makes its result stale (G39 spec 3). A result in a file outside the root depends
		// on every source outside the root, since any of them can cause it.
		const state = roots.get(root);
		const external = [...grouped.keys()].some(file => !inRoot(root, file));
		if (!state || state.changed > start.epoch || (removed.get(root) ?? 0) > start.epoch || state.written > start.started
			|| external && (state.outside > start.epoch || state.writtenOutside > start.started)) { return; }
		clear(state);
		state.shown = start.started;
		const position = (item: vscode.Diagnostic) => ({
			code: typeof item.code === 'object' ? String(item.code.value) : item.code === undefined ? undefined : String(item.code),
			line: item.range.start.line, character: item.range.start.character,
		});
		for (const [file, entries] of grouped) {
			const uri = vscode.Uri.file(file);
			// The language server reports the front-end diagnostics as the user types, so the run adds only the others.
			const server = vscode.languages.getDiagnostics(uri).filter(item => item.source === 'tsuzuri').map(position);
			const added = entries.filter(entry => !reportedByServer(position(entry), server));
			if (!added.length) { continue; }
			diagnostics.set(uri, added);
			state.uris.push(uri);
		}
		state.external = external;
	}

	async function task(action: Action, root: string, definition?: vscode.TaskDefinition): Promise<vscode.Task> {
		const uri = vscode.Uri.file(root);
		const folder = vscode.workspace.getWorkspaceFolder(uri);
		const tools = await toolchain(context, uri);
		const settings = vscode.workspace.getConfiguration('tsuzuri', uri);
		const args = commandArguments(action, root, settings.get<number>('optimization', 3), settings.get<boolean>('denyWarnings', false));
		const env = Object.fromEntries(Object.entries(tools.env).filter((entry): entry is [string, string] => entry[1] !== undefined));
		// No problem matcher (G39 D3): VS Code keeps a matcher's markers until the next task, and the extension cannot clear
		// them. The terminal links the `path:line:column` of each diagnostic; `$tsuzuri` stays for users' tasks.json.
		const item = new vscode.Task(definition ?? { type: 'tsuzuri', action, project: root },
			folder ?? vscode.TaskScope.Workspace, `${action}: ${path.basename(root)}`, 'Tsuzuri',
			new vscode.ProcessExecution(tools.compiler, args, { cwd: root, env }), []);
		item.group = action === 'test' ? vscode.TaskGroup.Test : action === 'build' ? vscode.TaskGroup.Build : undefined;
		item.presentationOptions = { reveal: vscode.TaskRevealKind.Always, panel: vscode.TaskPanelKind.Dedicated, clear: true };
		return item;
	}

	context.subscriptions.push(vscode.tasks.registerTaskProvider('tsuzuri', {
		async provideTasks() {
			const tasks: vscode.Task[] = [];
			for (const folder of vscode.workspace.workspaceFolders ?? []) {
				const root = await projectFor(folder.uri);
				for (const action of ['build', 'check', 'run', 'test', 'debug', 'wasm'] as const) {
					tasks.push(await task(action, root));
				}
			}
			return tasks;
		},
		async resolveTask(unresolved) {
			const action = unresolved.definition.action;
			if (!['build', 'check', 'run', 'test', 'debug', 'wasm'].includes(action)) { return undefined; }
			const folder = typeof unresolved.scope === 'object' ? unresolved.scope : undefined;
			const root = unresolved.definition.project
				? path.resolve(folder?.uri.fsPath ?? '', unresolved.definition.project)
				: await projectFor(folder?.uri);
			return task(action, root, unresolved.definition);
		},
	}));

	for (const action of ['build', 'check', 'run', 'wasm'] as const) {
		context.subscriptions.push(vscode.commands.registerCommand(`tsuzuri.${action}`, async (resource?: vscode.Uri) => {
			try {
				if (!await vscode.workspace.saveAll(false)) { return; }
				return await vscode.tasks.executeTask(await task(action, await projectFor(resource)));
			} catch (error) { reportError(error, output); }
		}));
	}

	async function debugConfiguration(folder: vscode.WorkspaceFolder | undefined, configuration: vscode.DebugConfiguration,
		token: vscode.CancellationToken): Promise<vscode.DebugConfiguration | undefined> {
		try {
			await ensureDebugger();
			// The root is found before the saveAll, so that nothing waits between the saveAll and the start of the build.
			const root = configuration.project ? path.resolve(folder?.uri.fsPath ?? '', configuration.project)
				: await projectFor(folder?.uri);
			if (!await vscode.workspace.saveAll(false)) { return undefined; }
			const start = begin([root]);
			const controller = new AbortController();
			const cancellation = token.onCancellationRequested(() => controller.abort());
			try {
				if (token.isCancellationRequested) { return undefined; }
				output.show(true);
				const result = await vscode.window.withProgress({ location: vscode.ProgressLocation.Notification, title: 'Tsuzuri: Building for Debug', cancellable: true },
					async (_progress, progressToken) => {
						const cancel = progressToken.onCancellationRequested(() => controller.abort());
						try {
							return await runCompiler(context, root, [...commandArguments('debug', root), '--json'], {
								signal: controller.signal, onOutput: text => output.append(text),
							});
						} finally { cancel.dispose(); }
					});
				await publishDiagnostics(root, result.stderr, start);
				if (result.code !== 0) { throw new Error('Tsuzuri debug build failed. See Problems and Tsuzuri Output.'); }
				if (controller.signal.aborted) { return undefined; }
			} finally { cancellation.dispose(); }
			const resolved: vscode.DebugConfiguration = {
				...configuration, ...lldbLaunch(outputPath(root, 'debug'), formatters(context), configuration), cwd: root,
				name: configuration.name || 'Debug Tsuzuri',
			};
			delete resolved.project;
			return resolved;
		} catch (error) { reportError(error, output); return undefined; }
	}
	const provider: vscode.DebugConfigurationProvider = {
		provideDebugConfigurations: () => supportsDebug() ? [{ type: 'tsuzuri', request: 'launch', name: 'Debug Tsuzuri' }] : [],
		resolveDebugConfiguration: debugConfiguration,
	};
	context.subscriptions.push(
		vscode.debug.registerDebugConfigurationProvider('tsuzuri', provider),
		vscode.debug.registerDebugConfigurationProvider('tsuzuri', provider, vscode.DebugConfigurationProviderTriggerKind.Dynamic),
		vscode.commands.registerCommand('tsuzuri.debug', async (resource?: vscode.Uri) => {
			try {
				const root = await projectFor(resource);
				return await vscode.debug.startDebugging(vscode.workspace.getWorkspaceFolder(vscode.Uri.file(root)),
					{ type: 'tsuzuri', request: 'launch', name: 'Debug Tsuzuri', project: root });
			} catch (error) { reportError(error, output); return false; }
		}),
		vscode.commands.registerCommand('tsuzuri.showToolchain', async () => {
			try {
				const tools = await toolchain(context, vscode.window.activeTextEditor?.document.uri);
				output.show();
				output.appendLine(`Host: ${process.platform}-${process.arch}\nCompiler: ${tools.compiler}\nToolchain: ${tools.root}`);
				const result = await runCompiler(context, path.dirname(tools.compiler), ['--version']);
				output.append(result.stdout + result.stderr);
			} catch (error) { reportError(error, output); }
		}),
		vscode.commands.registerCommand('tsuzuri.openDocumentation', () =>
			vscode.commands.executeCommand('markdown.showPreview', vscode.Uri.joinPath(context.extensionUri, 'resources', 'handbook', '_tsuzuri', 'language-reference', 'index.md'))),
		vscode.commands.registerCommand('tsuzuri.newProject', async () => {
			const selected = await vscode.window.showOpenDialog({ canSelectFiles: false, canSelectFolders: true, canSelectMany: false, openLabel: 'Create Tsuzuri Project in Empty Folder' });
			if (!selected?.[0]) { return; }
			try {
				const directory = selected[0].fsPath;
				if ((await readdir(directory)).length) { throw new Error('Choose an empty folder. Existing files will not be overwritten.'); }
				const namespace = await vscode.window.showInputBox({
					title: 'Tsuzuri Project Namespace',
					prompt: 'The default namespace that Tsuzuri.toml and Main.tz declare, such as Acme::Tools',
					value: defaultNamespace(path.basename(directory)),
					validateInput: text => isNamespace(text) ? undefined
						: 'Use identifiers joined by :: such as Acme::Tools, without reserved words or _, that do not start with std or a standard library module name.',
				});
				if (namespace === undefined) { return; }
				const result = await runCompiler(context, directory, ['new', directory, '--namespace', namespace]);
				if (result.code !== 0) { throw new Error((result.stderr || result.stdout).trim() || 'tsuzuri new failed.'); }
				await vscode.commands.executeCommand('vscode.openFolder', selected[0], true);
			} catch (error) { reportError(error, output); }
		}),
	);
	return { begin, track, publishDiagnostics, ensureDebugger };
}
