import { realpath } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import * as path from 'node:path';
import * as vscode from 'vscode';
import { LanguageClient } from 'vscode-languageclient/node';
import { assertSupportedHost, directlyIn, directoryExists, sourceSelector, supportsDebug } from './core';
import { registerEditor } from './editor';
import { registerTesting } from './testing';
import { toolchain } from './toolchain';
import { directoryRoot, documentHasProject, projectRootFor, projectRoots, registerWorkflow, reportError } from './workflow';

const clients = new Map<string, LanguageClient>();
const watchers = new Map<string, vscode.FileSystemWatcher>();
/**
 * How the root of each client was found: a remaining root's client serves only the files directly in it (G40 D2), and
 * a root of `tsuzuri.projectPath` does not move when files change.
 */
const kinds = new Map<string, { remaining: boolean; configured: boolean }>();

export async function activate(context: vscode.ExtensionContext) {
	if (!vscode.workspace.isTrusted) {
		return;
	}
	assertSupportedHost();
	const output = vscode.window.createOutputChannel('Tsuzuri');
	context.subscriptions.push(output);
	await vscode.commands.executeCommand('setContext', 'tsuzuri.debugAvailable', supportsDebug());
	registerEditor(context);
	const workflow = registerWorkflow(context, output);
	const testing = registerTesting(context, output, workflow);
	const pending = new Map<string, Promise<void>>();

	async function open(document: vscode.TextDocument) {
		if (document.languageId !== 'tsuzuri' || document.uri.scheme !== 'file') {
			return;
		}
		// A lookup that a change cleared the cache during is read again, and the client starts in the same task as the last
		// check: a re-rooting waits only for clients that have started (`pending`), not for this lookup (G40 D4).
		return projectRoots.settled(async () => {
			const found = await projectRootFor(document.uri);
			// A document left open in a folder that was deleted or moved has no project: its root would be a directory that
			// does not exist or one above the deleted folder, whose server could not read the document either (G40 D4).
			return { ...found, readable: pending.has(found.root) || clients.has(found.root) || await documentHasProject(document.uri, found) };
		}, ({ root, remaining, configured, readable }) => {
			if (!readable) {
				if (configured) { throw new Error(`The project folder of tsuzuri.projectPath does not exist: ${root}`); }
				return;
			}
			if (pending.has(root)) { return pending.get(root); }
			if (clients.has(root)) {
				return;
			}
			return startClient(document, root, remaining, configured);
		});
	}

	/** Starts the client of `root` for `document`; it is in `pending` before this returns. */
	function startClient(document: vscode.TextDocument, root: string, remaining: boolean, configured: boolean) {
		const start = (async () => {
			const tools = await toolchain(context, document.uri);
			// The server of a remaining root reads the packages below it too, which have clients of their own (G40 D2). Its
			// paths can be canonical: with symbolic links resolved.
			const real = remaining ? await realpath(root).catch(() => root) : root;
			const own = (uri: vscode.Uri) => directlyIn(root, uri.fsPath) || directlyIn(real, uri.fsPath);
			const watcher = vscode.workspace.createFileSystemWatcher(new vscode.RelativePattern(root, '**/*.{tz,tt,tc,toml}'));
			watchers.set(root, watcher);
			// The server takes its root from `initialize`, not from its working directory. Windows does not let a folder that
			// is a running process's working directory be deleted or moved, so a working directory in the root would keep
			// the user from deleting or moving a package folder while its server runs (G40 D4).
			const client = new LanguageClient('tsuzuri', 'Tsuzuri', {
				command: tools.compiler, args: ['lsp'], options: { cwd: tmpdir(), env: tools.env },
			}, {
				documentSelector: [{ language: 'tsuzuri', scheme: 'file', pattern: sourceSelector(root, remaining) }],
				workspaceFolder: { uri: vscode.Uri.file(root), name: path.basename(root), index: 0 },
				synchronize: { fileEvents: watcher }, outputChannel: output,
				middleware: remaining ? {
					handleDiagnostics: (uri, diagnostics, next) => next(uri, own(uri) ? diagnostics : []),
					provideWorkspaceSymbols: async (query, token, next) => (await next(query, token))?.filter(symbol => own(symbol.location.uri)),
				} : {},
			});
			clients.set(root, client);
			kinds.set(root, { remaining, configured });
			try { await client.start(); }
			catch (error) { clients.delete(root); kinds.delete(root); watchers.delete(root); watcher.dispose(); throw error; }
		})();
		pending.set(root, start);
		return start.finally(() => { pending.delete(root); });
	}

	async function stop(root: string) {
		const client = clients.get(root);
		clients.delete(root);
		kinds.delete(root);
		watchers.get(root)?.dispose();
		watchers.delete(root);
		await client?.stop().catch(() => undefined);
	}

	/** Runs the changes of the set of clients one after another. */
	let queue: Promise<unknown> = Promise.resolve();
	const serial = (task: () => Promise<void>) => {
		const next = queue.then(task);
		queue = next.catch(() => undefined);
		return next;
	};

	/**
	 * Decides the roots again after files below a workspace folder were created or deleted (G40 D4). A client stops when
	 * its root is no longer the root of the files directly in it, when it becomes a remaining root or stops being one
	 * (its selector changes; otherwise it would keep serving files of other clients), or when its folder was deleted or
	 * moved. Then the open documents start the clients that they lack, and the test roots are rebuilt, in this order and
	 * with a cleared cache. This runs in the background, so its errors go to the output, not to a notification on each
	 * change of the files.
	 */
	async function reroot() {
		projectRoots.clear();
		await Promise.allSettled(pending.values());
		const stale: string[] = [];
		for (const [root, kind] of kinds) {
			if (kind.configured) { continue; }
			const found = await directoryRoot(root);
			if (found.root !== root || found.remaining !== kind.remaining || !(await directoryExists(root))) { stale.push(root); }
		}
		await Promise.all(stale.map(stop));
		await Promise.all(vscode.workspace.textDocuments.map(document => open(document).catch(error => output.appendLine(String(error)))));
		await testing.reroot();
	}
	let rerootTimer: NodeJS.Timeout | undefined;
	// Changes come in bursts (a git checkout), so the roots are decided again once they pause, as test discovery does.
	const scheduleReroot = () => {
		clearTimeout(rerootTimer);
		rerootTimer = setTimeout(() => {
			rerootTimer = undefined;
			void serial(reroot).catch(error => output.appendLine(String(error)));
		}, 300);
	};
	// Every creation and deletion below a workspace folder, whatever its name: a marker, a source (the limit of 4096
	// sources), or a directory (the limit of 1024 directories). A folder that is deleted or moved arrives as one event of
	// the folder. A changed file's contents do not change a root.
	const rootChanges = vscode.workspace.createFileSystemWatcher('**', false, true, false);
	const rootChanged = (uri: vscode.Uri) => {
		const folder = vscode.workspace.getWorkspaceFolder(uri);
		if (folder && projectRoots.changed(folder.uri.fsPath, uri.fsPath)) { scheduleReroot(); }
	};

	const safelyOpen = (document: vscode.TextDocument) => { void open(document).catch(error => reportError(error, output)); };
	const status = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 15);
	status.text = '$(code) Tsuzuri';
	status.tooltip = 'Tsuzuri project actions';
	status.command = 'tsuzuri.actions';
	const updateStatus = () => {
		if (vscode.window.activeTextEditor?.document.languageId === 'tsuzuri') { status.show(); } else { status.hide(); }
	};
	context.subscriptions.push(
		status,
		rootChanges, rootChanges.onDidCreate(rootChanged), rootChanges.onDidDelete(rootChanged),
		new vscode.Disposable(() => clearTimeout(rerootTimer)),
		vscode.window.onDidChangeActiveTextEditor(updateStatus),
		vscode.workspace.onDidOpenTextDocument(safelyOpen),
		vscode.workspace.onDidChangeWorkspaceFolders(() => {
			void vscode.commands.executeCommand('tsuzuri.restartLanguageServer');
			scheduleReroot();
		}),
		vscode.workspace.onDidChangeConfiguration(event => {
			if (['tsuzuri.compilerPath', 'tsuzuri.toolchainPath', 'tsuzuri.projectPath'].some(key => event.affectsConfiguration(key))) {
				void vscode.commands.executeCommand('tsuzuri.restartLanguageServer');
			}
		}),
		vscode.commands.registerCommand('tsuzuri.actions', async () => {
			const actions = ['Run', ...(supportsDebug() ? ['Debug'] : []), 'Build', 'Check', 'Test', 'Wasm', 'New Project', 'Open Documentation', 'Show Toolchain'];
			const selected = await vscode.window.showQuickPick(actions, { title: 'Tsuzuri' });
			if (selected) {
				const command = selected[0].toLowerCase() + selected.slice(1).replaceAll(' ', '');
				await vscode.commands.executeCommand(`tsuzuri.${command}`);
			}
		}),
		vscode.commands.registerCommand('tsuzuri.restartLanguageServer', () => serial(async () => {
			projectRoots.clear();
			await Promise.allSettled(pending.values());
			await deactivate();
			vscode.workspace.textDocuments.forEach(safelyOpen);
		})),
	);
	updateStatus();
	await Promise.all(vscode.workspace.textDocuments.map(document => open(document).catch(error => reportError(error, output))));
	return { clients, testing, workflow };
}

export async function deactivate() {
	for (const watcher of watchers.values()) { watcher.dispose(); }
	watchers.clear();
	kinds.clear();
	await Promise.allSettled([...clients.values()].map(client => client.stop()));
	clients.clear();
}
