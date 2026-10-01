import { readdir, readFile, writeFile } from 'node:fs/promises';
import * as path from 'node:path';
import * as vscode from 'vscode';
import { Action, commandArguments, exists, jsonLines, outputPath, projectRoot, supportsDebug } from './core';
import { runCompiler, toolchain } from './toolchain';

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

export function reportError(error: unknown, output: vscode.OutputChannel) {
	output.appendLine(String(error));
	void vscode.window.showErrorMessage(String(error));
}

export function registerWorkflow(context: vscode.ExtensionContext, output: vscode.OutputChannel) {
	const diagnostics = vscode.languages.createDiagnosticCollection('tsuzuri-build');
	context.subscriptions.push(diagnostics);
	const diagnosticsByRoot = new Map<string, vscode.Uri[]>();
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

	async function publishDiagnostics(root: string, text: string) {
		for (const uri of diagnosticsByRoot.get(root) ?? []) { diagnostics.delete(uri); }
		const grouped = new Map<string, vscode.Diagnostic[]>();
		for (const record of jsonLines(text)) {
			if (typeof record.path !== 'string' || typeof record.message !== 'string') { continue; }
			const file = path.resolve(root, record.path);
			let start = new vscode.Position(0, 0);
			let end = start;
			try {
				const source = await readFile(file);
				const span = record.span as { start?: number; end?: number } | undefined;
				const position = (offset: number) => {
					const prefix = source.subarray(0, Math.max(0, offset)).toString('utf8').split('\n');
					return new vscode.Position(prefix.length - 1, prefix.at(-1)!.length);
				};
				start = position(span?.start ?? 0);
				end = position(span?.end ?? span?.start ?? 0);
			} catch { end = start; }
			const diagnostic = new vscode.Diagnostic(new vscode.Range(start, end), record.message,
				record.severity === 'warning' ? vscode.DiagnosticSeverity.Warning : vscode.DiagnosticSeverity.Error);
			diagnostic.source = 'Tsuzuri';
			diagnostic.code = typeof record.code === 'string' ? record.code : undefined;
			const entries = grouped.get(file) ?? [];
			entries.push(diagnostic);
			grouped.set(file, entries);
		}
		const uris: vscode.Uri[] = [];
		for (const [file, entries] of grouped) {
			const uri = vscode.Uri.file(file);
			diagnostics.set(uri, entries);
			uris.push(uri);
		}
		diagnosticsByRoot.set(root, uris);
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
				const result = await vscode.window.withProgress({ location: vscode.ProgressLocation.Notification, title: 'Tsuzuri: Building for Debug', cancellable: true },
					async (_progress, progressToken) => {
						const cancel = progressToken.onCancellationRequested(() => controller.abort());
						try {
							return await runCompiler(context, root, [...commandArguments('debug', root), '--json'], {
								signal: controller.signal, onOutput: text => output.append(text),
							});
						} finally { cancel.dispose(); }
					});
				await publishDiagnostics(root, result.stderr);
				if (result.code !== 0) { throw new Error('Tsuzuri debug build failed. See Problems and Tsuzuri Output.'); }
				if (controller.signal.aborted) { return undefined; }
			} finally { cancellation.dispose(); }
			const program = outputPath(root, 'debug');
			const resolved: vscode.DebugConfiguration = {
				...configuration, type: 'lldb', request: 'launch', name: configuration.name || 'Debug Tsuzuri',
				program, cwd: root, sourceLanguages: ['c'],
				terminal: configuration.terminal ?? 'integrated',
				preRunCommands: [...(configuration.preRunCommands ?? []),
					...(process.platform === 'darwin' ? [`target symbols add ${JSON.stringify(`${program}.dwarf`)}`] : [])],
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
			vscode.commands.executeCommand('markdown.showPreview', vscode.Uri.joinPath(context.extensionUri, 'resources', 'handbook', '_docs', 'README.md'))),
		vscode.commands.registerCommand('tsuzuri.newProject', async () => {
			const selected = await vscode.window.showOpenDialog({ canSelectFiles: false, canSelectFolders: true, canSelectMany: false, openLabel: 'Create Tsuzuri Project in Empty Folder' });
			if (!selected?.[0]) { return; }
			try {
				const directory = selected[0].fsPath;
				if ((await readdir(directory)).length) { throw new Error('Choose an empty folder. Existing files will not be overwritten.'); }
				await writeFile(path.join(directory, 'Main.tz'), 'def main :: IO<unit> =\n    do! IO.write_line "Hello, Tsuzuri!"\n\ntest "adds numbers" = assert (1 + 2 == 3)\n', { flag: 'wx' });
				await writeFile(path.join(directory, 'Tsuzuri.toml'), '[package]\nname = "app"\nversion = "0.1.0"\n', { flag: 'wx' });
				await writeFile(path.join(directory, '.gitignore'), '.tsuzuri/\n', { flag: 'wx' });
				await vscode.commands.executeCommand('vscode.openFolder', selected[0], true);
			} catch (error) { reportError(error, output); }
		}),
	);
	return { publishDiagnostics };
}
