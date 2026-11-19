import { readdir, readFile, stat } from 'node:fs/promises';
import * as path from 'node:path';
import * as vscode from 'vscode';
import { Action, affectsRoot, commandArguments, defaultNamespace, exists, inRoot, isNamespace, jsonLines, lldbLaunch, outputPath, projectFile, projectRoot, reportedByServer, sourcePosition, supportsDebug } from './core';
import { runCompiler, toolchain } from './toolchain';

/** The change numbers of a project root just before a run of the compiler started (`begin`), which its result must match. */
export interface RunStart {
	changed: number;
	outside: number;
}

/**
 * The result of the last test run, test debug, or Debug Project of a project root (G39 D2): its URIs, whether one is
 * outside the root (a path dependency), the numbers of the root's last change and of its last change outside the
 * root, the start of its last run, and the watcher of deletions under the root.
 */
interface RootResult {
	uris: vscode.Uri[];
	external: boolean;
	changed: number;
	outside: number;
	started: number;
	deletions: vscode.FileSystemWatcher;
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
	/** Numbers every change, so a root that is removed and added again never matches a run that started before. */
	let changes = 0;
	function rootState(root: string): RootResult {
		let state = roots.get(root);
		if (!state) {
			const created: RootResult = {
				uris: [], external: false, changed: ++changes, outside: changes, started: -Infinity,
				// A folder that is deleted or moved arrives as one event of the folder, which the source watcher does not match.
				deletions: vscode.workspace.createFileSystemWatcher(new vscode.RelativePattern(root, '**'), true, true, false),
			};
			created.deletions.onDidDelete(uri => { if (affectsRoot(root, uri.fsPath)) { created.changed = ++changes; clear(created); } });
			roots.set(root, state = created);
		}
		return state;
	}
	function clear(state: RootResult) {
		for (const uri of state.uris) { diagnostics.delete(uri); }
		state.uris = [];
		state.external = false;
	}
	/**
	 * A change of `file` (G39 spec 3) changes the roots that contain it, and when it is a Tsuzuri source, also the
	 * results of the other roots that point outside their root. A watcher's event of a written file gives the file's
	 * `modified` time, which counts only after the root's last run started: the saveAll before a run can reach the
	 * watcher late.
	 */
	function changed(file: string, source: boolean, modified?: number) {
		for (const [root, state] of roots) {
			if (modified !== undefined && modified <= state.started) { continue; }
			if (inRoot(root, file)) {
				state.changed = ++changes;
				clear(state);
			} else if (source) {
				state.outside = ++changes;
				if (state.external) { clear(state); }
			}
		}
	}
	/** Records the start of a run of the compiler in `root`, just before it starts; `publishDiagnostics` takes the result. */
	function begin(root: string): RunStart {
		const state = rootState(root);
		state.started = Date.now();
		return { changed: state.changed, outside: state.outside };
	}
	// VS Code 1.103 on macOS can report the saveAll before a run as a creation, so a creation counts by the file's times
	// like a change. The status change time is new after a move, which keeps the modification time.
	const written = (uri: vscode.Uri) => {
		void stat(uri.fsPath).then(info => Math.trunc(Math.max(info.mtimeMs, info.ctimeMs)), () => Date.now())
			.then(modified => changed(uri.fsPath, true, modified));
	};
	const sources = vscode.workspace.createFileSystemWatcher('**/*.{tz,tt,tc,toml}');
	context.subscriptions.push(sources, sources.onDidCreate(written), sources.onDidChange(written),
		sources.onDidDelete(uri => changed(uri.fsPath, true)),
		vscode.workspace.onDidChangeTextDocument(event => {
			const document = event.document.uri;
			if (event.contentChanges.length && document.scheme === 'file') { changed(document.fsPath, projectFile(document.fsPath)); }
		}),
		vscode.workspace.onDidChangeWorkspaceFolders(event => {
			for (const [root, state] of roots) {
				if (event.removed.some(folder => inRoot(folder.uri.fsPath, root))) {
					clear(state);
					state.deletions.dispose();
					roots.delete(root);
				}
			}
		}),
		new vscode.Disposable(() => { for (const state of roots.values()) { state.deletions.dispose(); } }),
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
		for (const record of jsonLines(text)) {
			if (typeof record.path !== 'string' || typeof record.message !== 'string') { continue; }
			const file = path.resolve(root, record.path);
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
		if (!state || state.changed !== start.changed || external && state.outside !== start.outside) { return; }
		clear(state);
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
		const item = new vscode.Task(definition ?? { type: 'tsuzuri', action, project: root },
			folder ?? vscode.TaskScope.Workspace, `${action}: ${path.basename(root)}`, 'Tsuzuri',
			new vscode.ProcessExecution(tools.compiler, args, { cwd: root, env }), '$tsuzuri');
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
			if (!await vscode.workspace.saveAll(false)) { return undefined; }
			const root = configuration.project ? path.resolve(folder?.uri.fsPath ?? '', configuration.project)
				: await projectFor(folder?.uri);
			const controller = new AbortController();
			const cancellation = token.onCancellationRequested(() => controller.abort());
			try {
				if (token.isCancellationRequested) { return undefined; }
				output.show(true);
				const start = begin(root);
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
	return { begin, publishDiagnostics, ensureDebugger };
}
