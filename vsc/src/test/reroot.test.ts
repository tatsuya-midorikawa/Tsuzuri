import * as assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { mkdir, mkdtemp, rm } from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { test } from 'node:test';
import { runInThisContext } from 'node:vm';
import * as core from '../core';

/**
 * Loads the compiled module `name` of the extension with `modules` in place of its imports other than `node:` ones and
 * `timers` in place of setTimeout and clearTimeout, so that it runs without VS Code.
 */
function load(name: string, modules: Record<string, unknown>, timers: { setTimeout: unknown; clearTimeout: unknown }) {
	const file = path.join(__dirname, '..', `${name}.js`);
	const run = runInThisContext(`(function (exports, require, module, __filename, __dirname, setTimeout, clearTimeout) {${readFileSync(file, 'utf8')}\n})`, { filename: file });
	const module = { exports: {} as Record<string, any> };
	run(module.exports, (id: string) => {
		if (id.startsWith('node:')) { return require(id); }
		if (id in modules) { return modules[id]; }
		throw new Error(`${name} imports ${id}, which the test does not provide.`);
	}, module, file, path.dirname(file), timers.setTimeout, timers.clearTimeout);
	return module.exports;
}

interface Uri { fsPath: string; scheme: string }
interface Document { languageId: string; uri: Uri }
interface Item { id: string; children: Items }

class Items {
	readonly items = new Map<string, Item>();
	get size() { return this.items.size; }
	add(item: Item) { this.items.set(item.id, item); }
	delete(id: string) { this.items.delete(id); }
	get(id: string) { return this.items.get(id); }
	replace(items: Item[]) { this.items.clear(); items.forEach(item => this.add(item)); }
	forEach(callback: (item: Item) => void) { this.items.forEach(item => callback(item)); }
}

function deferred<T = void>() {
	let resolve!: (value: T) => void;
	return { promise: new Promise<T>(done => { resolve = done; }), resolve };
}

/** Lets the callbacks and promises that the extension has queued run. */
async function settle() {
	for (let turn = 0; turn < 20; turn++) { await new Promise(resolve => setImmediate(resolve)); }
}

const passed = { code: 0, stdout: '', stderr: '' };

/**
 * The extension's modules with a stubbed vscode API, in a temporary folder `top` whose files decide the roots: while the
 * package `top/b` exists, each folder below `top` is a root (`top/a` for `top/a/X.tz`, which is open); once the package
 * is deleted (`deletePackage`), `top` is the root of every file. A lookup of a root reads the files when it starts and
 * can be paused before it returns (`pauseLookup`), as a slow scan is.
 */
async function host() {
	const top = await mkdtemp(path.join(os.tmpdir(), 'tsuzuri-reroot-'));
	const app = path.join(top, 'a');
	const file = path.join(app, 'X.tz');
	await mkdir(app);
	const uri = (fsPath: string): Uri => ({ fsPath, scheme: 'file' });
	const document = (fsPath: string): Document => ({ languageId: 'tsuzuri', uri: uri(fsPath) });
	const files = { package: true };
	const documents = [document(file)];
	const projectRoots = new core.ProjectRoots();
	let pause: { reached: () => void; resume: Promise<void> } | undefined;
	async function projectRootFor(resource: Uri = documents[0].uri, directory = false) {
		const folder = directory ? resource.fsPath : path.dirname(resource.fsPath);
		const root = files.package ? path.join(top, path.relative(top, folder).split(path.sep)[0]) : top;
		const paused = pause;
		if (paused) {
			pause = undefined;
			paused.reached();
			await paused.resume;
		}
		return { root, remaining: false, configured: false };
	}
	const output: string[] = [];
	const disposable = { dispose() {} };
	const opened: ((document: Document) => unknown)[] = [];
	const watchers: { pattern: unknown; changed: ((uri: Uri) => unknown)[] }[] = [];
	const commands = new Map<string, (...args: unknown[]) => any>();
	const token = () => ({ isCancellationRequested: false, onCancellationRequested: () => disposable });
	const controller = {
		items: new Items(),
		createTestItem: (id: string, label: string, itemUri: Uri) => ({ id, label, uri: itemUri, children: new Items(), busy: false }),
		createRunProfile() {},
		createTestRun: () => ({ appendOutput() {}, end() {} }),
		dispose() {},
		resolveHandler: undefined as unknown as (item?: Item) => Promise<void>,
	};
	const vscode = {
		Uri: { file: uri },
		Disposable: class { constructor(readonly dispose: () => void) {} },
		TestRunRequest: class { constructor(readonly include?: Item[], readonly exclude?: Item[]) {} },
		CancellationTokenSource: class { token = token(); dispose() {} },
		TestRunProfileKind: { Run: 1, Debug: 2 },
		RelativePattern: class { constructor(readonly base: unknown, readonly pattern: string) {} },
		StatusBarAlignment: { Left: 1 },
		tests: { createTestController: () => controller },
		workspace: {
			isTrusted: true,
			textDocuments: documents,
			getConfiguration: () => ({ get: () => undefined }),
			findFiles: async () => [],
			saveAll: async () => true,
			getWorkspaceFolder: () => undefined,
			createFileSystemWatcher(pattern: unknown) {
				const watcher = { pattern, changed: [] as ((uri: Uri) => unknown)[] };
				watchers.push(watcher);
				const listen = (listener: (uri: Uri) => unknown) => { watcher.changed.push(listener); return disposable; };
				return { onDidCreate: () => disposable, onDidChange: listen, onDidDelete: () => disposable, dispose() {} };
			},
			onDidOpenTextDocument: (listener: (document: Document) => unknown) => { opened.push(listener); return disposable; },
			onDidChangeWorkspaceFolders: () => disposable,
			onDidChangeConfiguration: () => disposable,
		},
		window: {
			activeTextEditor: { document: documents[0] },
			createOutputChannel: () => ({ appendLine: (line: string) => output.push(line), dispose() {} }),
			createStatusBarItem: () => ({ show() {}, hide() {}, dispose() {} }),
			onDidChangeActiveTextEditor: () => disposable,
		},
		commands: {
			registerCommand: (name: string, callback: (...args: unknown[]) => unknown) => { commands.set(name, callback); return disposable; },
			executeCommand: async () => undefined,
		},
	};
	/** The arguments of each compiler run; the first run after `holdCompiler` returns only when it is resolved. */
	const compiled: string[][] = [];
	let held: ReturnType<typeof deferred<typeof passed>> | undefined;
	const timers: (() => void)[] = [];
	/** The language clients that the extension made; the first `stop` after `holdStop` returns only when it is resolved. */
	const languageClients: { root: string; stopped: boolean }[] = [];
	let stopHeld: ReturnType<typeof deferred<void>> | undefined;
	class LanguageClient {
		readonly root: string;
		stopped = false;
		constructor(_id: string, _name: string, _server: unknown, options: { workspaceFolder: { uri: Uri } }) {
			this.root = options.workspaceFolder.uri.fsPath;
			languageClients.push(this);
		}
		async start() {}
		async stop() {
			this.stopped = true;
			const held = stopHeld;
			stopHeld = undefined;
			await held?.promise;
		}
	}
	const modules = {
		vscode,
		'vscode-languageclient/node': { LanguageClient },
		'./core': core,
		'./editor': { registerEditor() {} },
		'./toolchain': {
			async toolchain() { return { compiler: 'tsuzuri', env: {} }; },
			runCompiler(_context: unknown, _root: string, args: string[]) {
				compiled.push(args);
				const result = held;
				held = undefined;
				return result ? result.promise : Promise.resolve(passed);
			},
		},
		'./workflow': {
			projectRoots, projectRootFor,
			async projectFor(resource?: Uri) { return (await projectRootFor(resource)).root; },
			directoryRoot: (directory: string) => projectRootFor(uri(directory), true),
			async documentHasProject() { return true; },
			formatters: () => '',
			reportError: (error: unknown) => { output.push(String(error)); },
		},
	};
	const fakeTimers = {
		setTimeout: (callback: () => void) => timers.push(callback),
		clearTimeout: (handle?: number) => { if (handle) { timers[handle - 1] = () => undefined; } },
	};
	const context = { subscriptions: [] as unknown[] };
	const workflow = { begin: () => ({ epoch: 0, started: 0 }), track() {}, async publishDiagnostics() {}, async ensureDebugger() {} };
	return {
		top, app, file, files, documents, document, projectRoots, controller, commands, compiled, timers, languageClients, output, vscode, token,
		/** Registers the Test Explorer support and waits for its first roots. */
		async testing() {
			const testing = load('testing', modules, fakeTimers).registerTesting(context, vscode.window.createOutputChannel(), workflow);
			await controller.resolveHandler();
			return testing;
		},
		/** Activates the extension with `modules['./testing']` in place of the Test Explorer support. */
		async activate(testing: unknown) {
			return load('extension', { ...modules, './testing': { registerTesting: () => testing }, './workflow': { ...modules['./workflow'], registerWorkflow: () => ({}) } }, fakeTimers).activate(context);
		},
		/** Makes the next language client's `stop` wait until the returned deferred is resolved. */
		holdStop() { return stopHeld = deferred(); },
		/** Makes the next compiler run wait until the returned deferred is resolved. */
		holdCompiler() { return held = deferred<typeof passed>(); },
		/** Pauses the next lookup of a root after it read the files; it continues at `resume`. */
		pauseLookup() {
			const reached = deferred(), resume = deferred();
			pause = { reached: reached.resolve, resume: resume.promise };
			return { reached: reached.promise, resume: resume.resolve };
		},
		/** Saves `changed` in an editor: the Test Explorer support's watcher reports it. */
		save(changed: string) {
			for (const watcher of watchers) { if (watcher.pattern === '**/*.{tz,tt,tc,toml}') { watcher.changed.forEach(listener => listener(uri(changed))); } }
		},
		open(added: Document) {
			documents.push(added);
			opened.forEach(listener => listener(added));
		},
		/**
		 * Deletes the package and re-roots the Test Explorer support as the extension does (`reroot` in `extension.ts`):
		 * the creation or deletion clears the cache of roots, and then the test roots are rebuilt.
		 */
		async deletePackage(testing: { reroot: () => Promise<void> }) {
			files.package = false;
			projectRoots.clear();
			await testing.reroot();
		},
		dispose: () => rm(top, { recursive: true, force: true }),
	};
}

test('a discovery that waits for an earlier one does not bring back a root that a re-rooting removed meanwhile', async () => {
	const stubbed = await host();
	const { app, top, controller, compiled, timers } = stubbed;
	try {
		const testing = await stubbed.testing();
		assert.deepEqual([...testing.roots.keys()], [app]);
		// A discovery of top/a runs, and a save in top/a schedules another one, which waits for the first after 300 ms.
		const earlier = stubbed.holdCompiler();
		const discovery = testing.discover(app);
		stubbed.save(stubbed.file);
		await settle();
		assert.equal(timers.length, 1);
		timers[0]();
		await settle();
		// The package is deleted meanwhile: the files of top/a now belong to top, and the re-rooting removes top/a.
		await stubbed.deletePackage(testing);
		assert.deepEqual([...testing.roots.keys()], [top]);
		earlier.resolve(passed);
		await discovery;
		await settle();
		assert.deepEqual([...testing.roots.keys()], [top]);
		assert.equal(controller.items.get(app), undefined);
		assert.deepEqual(compiled.map(args => args[1]), [app]);
		assert.deepEqual(stubbed.output, []);
	} finally { await stubbed.dispose(); }
});

test('an explicit run that waits for an earlier discovery does not bring back a root that a re-rooting removed meanwhile', async () => {
	const stubbed = await host();
	const { app, top, compiled } = stubbed;
	try {
		const testing = await stubbed.testing();
		const earlier = stubbed.holdCompiler();
		const discovery = testing.discover(app);
		const run = testing.run(new stubbed.vscode.TestRunRequest([testing.roots.get(app)]), stubbed.token(), 0);
		await settle();
		await stubbed.deletePackage(testing);
		assert.deepEqual([...testing.roots.keys()], [top]);
		earlier.resolve(passed);
		await discovery;
		// The run has nothing to run in a root that stopped being one.
		assert.deepEqual(await run, { passed: 0, failed: 0, errored: 0, skipped: 0 });
		assert.deepEqual([...testing.roots.keys()], [top]);
		assert.deepEqual(compiled.map(args => args[1]), [app]);
		assert.deepEqual(stubbed.output, []);
	} finally { await stubbed.dispose(); }
});

test('a root that was read before a change cleared the cache is not added to the Test Explorer after the re-rooting', async t => {
	// The three places that add the root of a lookup: a refresh of the roots, an opened document, and Test Project.
	for (const entry of ['refresh', 'open', 'command'] as const) {
		await t.test(entry, async () => {
			const stubbed = await host();
			const { app, top, controller, commands } = stubbed;
			try {
				const testing = await stubbed.testing();
				assert.deepEqual([...testing.roots.keys()], [app]);
				// The lookup reads the root top/a of top/a/X.tz while the package exists; the package is deleted, and the
				// re-rooting removes top/a, before the lookup returns.
				const lookup = stubbed.pauseLookup();
				const done = entry === 'refresh' ? controller.resolveHandler()
					: entry === 'open' ? stubbed.open(stubbed.document(path.join(app, 'Y.tz')))
					: commands.get('tsuzuri.test')!();
				await lookup.reached;
				await stubbed.deletePackage(testing);
				assert.deepEqual([...testing.roots.keys()], [top]);
				lookup.resume();
				await done;
				await settle();
				assert.deepEqual([...testing.roots.keys()], [top]);
				assert.deepEqual(stubbed.output, []);
			} finally { await stubbed.dispose(); }
		});
	}
});

test('a client that starts while Restart Language Server stops the others stays tracked, so no server runs without one', async () => {
	const stubbed = await host();
	const { app, top, commands, languageClients } = stubbed;
	try {
		const api = await stubbed.activate({ async reroot() {} });
		assert.deepEqual([...api.clients.keys()], [app]);
		// The restart waits while the server of top/a stops, and a document of the package top/b is opened meanwhile.
		const stopping = stubbed.holdStop();
		const restart = commands.get('tsuzuri.restartLanguageServer')!();
		await settle();
		assert.ok(languageClients[0].stopped);
		const other = path.join(top, 'b');
		stubbed.open(stubbed.document(path.join(other, 'Main.tz')));
		await settle();
		assert.deepEqual(languageClients.map(client => client.root), [app, other]);
		stopping.resolve();
		await restart;
		await settle();
		// Each server that runs has its client in the map, one for each root.
		const running = languageClients.filter(client => !client.stopped);
		assert.deepEqual(running.map(client => client.root).sort(), [app, other].sort());
		for (const client of running) { assert.equal(api.clients.get(client.root), client); }
		assert.deepEqual(stubbed.output, []);
	} finally { await stubbed.dispose(); }
});
