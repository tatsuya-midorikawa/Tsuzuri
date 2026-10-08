import * as path from 'node:path';
import * as vscode from 'vscode';
import { LanguageClient } from 'vscode-languageclient/node';
import { sourcePattern, supportsDebug } from './core';
import { registerEditor } from './editor';
import { registerTesting } from './testing';
import { toolchain } from './toolchain';
import { projectFor, registerWorkflow, reportError } from './workflow';

const clients = new Map<string, LanguageClient>();
const watchers = new Map<string, vscode.FileSystemWatcher>();

export async function activate(context: vscode.ExtensionContext) {
	if (!vscode.workspace.isTrusted) {
		return;
	}
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
		const root = await projectFor(document.uri);
		if (pending.has(root)) { return pending.get(root); }
		if (clients.has(root)) {
			return;
		}
		const start = (async () => {
			const tools = await toolchain(context, document.uri);
			const watcher = vscode.workspace.createFileSystemWatcher(new vscode.RelativePattern(root, '**/*.{tz,tt,tc,toml}'));
			watchers.set(root, watcher);
			const client = new LanguageClient('tsuzuri', 'Tsuzuri', {
				command: tools.compiler, args: ['lsp'], options: { cwd: root, env: tools.env },
			}, {
				documentSelector: [{ language: 'tsuzuri', scheme: 'file', pattern: `${root.replaceAll('\\', '/').replace(/[\[\]{}*?]/g, '[$&]')}/${sourcePattern}` }],
				workspaceFolder: { uri: vscode.Uri.file(root), name: path.basename(root), index: 0 },
				synchronize: { fileEvents: watcher }, outputChannel: output,
			});
			clients.set(root, client);
			try { await client.start(); }
			catch (error) { clients.delete(root); watchers.delete(root); watcher.dispose(); throw error; }
		})();
		pending.set(root, start);
		try { await start; } finally { pending.delete(root); }
	}

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
		vscode.window.onDidChangeActiveTextEditor(updateStatus),
		vscode.workspace.onDidOpenTextDocument(safelyOpen),
		vscode.workspace.onDidChangeWorkspaceFolders(() => { void vscode.commands.executeCommand('tsuzuri.restartLanguageServer'); }),
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
		vscode.commands.registerCommand('tsuzuri.restartLanguageServer', async () => {
			await Promise.allSettled(pending.values());
			await deactivate();
			vscode.workspace.textDocuments.forEach(safelyOpen);
		}),
	);
	updateStatus();
	await Promise.all(vscode.workspace.textDocuments.map(document => open(document).catch(error => reportError(error, output))));
	return { clients, testing };
}

export async function deactivate() {
	for (const watcher of watchers.values()) { watcher.dispose(); }
	watchers.clear();
	await Promise.allSettled([...clients.values()].map(client => client.stop()));
	clients.clear();
}
