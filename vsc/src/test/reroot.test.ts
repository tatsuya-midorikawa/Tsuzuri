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

/**
 * Waits until `condition` holds, letting the extension's callbacks, promises and file-system calls run. A fixed number
 * of turns is not enough when a step waits for the file system (`documentHasProject`) on a loaded machine.
 */
async function until(condition: () => boolean, what: string) {
	const end = Date.now() + 10_000;
	while (!condition()) {
		if (Date.now() > end) { assert.fail(`timed out: ${what}`); }
		await new Promise(resolve => setTimeout(resolve, 1));
	}
}

const passed = { code: 0, stdout: '', stderr: '' };
/** A discovery of top/a that lists the test `adds` of top/a/X.tz. */
const listed = {
	code: 0,
	stdout: `${JSON.stringify({ type: 'test', index: 0, name: 'adds', module: 'X', path: 'X.tz', range: { start: { line: 0, character: 0 }, end: { line: 0, character: 4 } } })}\n`,
	stderr: '',
};

/** The names of the tests below the root `project`. */
function testNames(project: Item) {
	const names: string[] = [];
	project.children.forEach(file => file.children.forEach(item => names.push((item as Item & { label: string }).label)));
	return names;
}

/**
 * The extension's modules with a stubbed vscode API, in a temporary folder `top` whose files decide the roots: while the
 * package `top/b` exists, each folder below `top` is a root (`top/a` for `top/a/X.tz`, which is open); once the package
 * is deleted (`deletePackage`), or the folder `top/a` (`deleteFolder`), `top` is the root of every file. A lookup of a
 * root reads the files when it starts and can be paused before it returns (`pauseLookup`), as a slow scan is.
 */
async function host() {
	const top = await mkdtemp(path.join(os.tmpdir(), 'tsuzuri-reroot-'));
	const app = path.join(top, 'a');
	const file = path.join(app, 'X.tz');
	const pkg = path.join(top, 'b');
	await mkdir(app);
	await mkdir(pkg);
	const uri = (fsPath: string): Uri => ({ fsPath, scheme: 'file' });
	const document = (fsPath: string): Document => ({ languageId: 'tsuzuri', uri: uri(fsPath) });
	const files: { package: boolean; configured?: string } = { package: true };
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
		return files.configured ? { root: files.configured, remaining: false, configured: true } : { root, remaining: false, configured: false };
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
		Range: class { constructor(readonly startLine: number, readonly startCharacter: number, readonly endLine: number, readonly endCharacter: number) {} },
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
	const fakeTimers = {
		setTimeout: (callback: () => void) => timers.push(callback),
		clearTimeout: (handle?: number) => { if (handle) { timers[handle - 1] = () => undefined; } },
	};
	const toolchain = {
		async toolchain() { return { compiler: 'tsuzuri', env: {} }; },
		runCompiler(_context: unknown, _root: string, args: string[]) {
			compiled.push(args);
			const result = held;
			held = undefined;
			return result ? result.promise : Promise.resolve(passed);
		},
	};
	// Whether a document has a project, and the error that names its missing folder, read the disk as the extension does.
	const { documentHasProject, missingProject } = load('workflow', { vscode, './core': core, './toolchain': toolchain }, fakeTimers);
	const modules = {
		vscode,
		'vscode-languageclient/node': { LanguageClient },
		'./core': core,
		'./editor': { registerEditor() {} },
		'./toolchain': toolchain,
		'./workflow': {
			projectRoots, projectRootFor, documentHasProject, missingProject,
			async projectFor(resource?: Uri) { return (await projectRootFor(resource)).root; },
			directoryRoot: (directory: string) => projectRootFor(uri(directory), true),
			formatters: () => '',
			reportError: (error: unknown) => { output.push(String(error)); },
		},
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
			await rm(pkg, { recursive: true, force: true });
			files.package = false;
			projectRoots.clear();
			await testing.reroot();
		},
		/** Creates the package top/b again and re-roots as `deletePackage` does: each folder below top is a root again. */
		async createPackage(testing: { reroot: () => Promise<void> }) {
			await mkdir(pkg);
			files.package = true;
			projectRoots.clear();
			await testing.reroot();
		},
		/**
		 * Deletes the folder top/a with the open file X.tz in it, as a package's folder is deleted: the walk up from a file
		 * left open there ends at top, which remains. The Test Explorer support is re-rooted as for `deletePackage`.
		 */
		async deleteFolder(testing: { reroot: () => Promise<void> }) {
			await rm(app, { recursive: true, force: true });
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

test('a discovery that a re-rooting outlasts gives its tests to the root that was added back meanwhile', async () => {
	const stubbed = await host();
	const { app, top, compiled } = stubbed;
	try {
		const testing = await stubbed.testing();
		const removed = testing.roots.get(app)!;
		const earlier = stubbed.holdCompiler();
		const discovery = testing.discover(app);
		await settle();
		// The package is deleted, which removes top/a, and created again, which adds it back, while the compiler runs.
		await stubbed.deletePackage(testing);
		assert.deepEqual([...testing.roots.keys()], [top]);
		await stubbed.createPackage(testing);
		const current = testing.roots.get(app)!;
		assert.notEqual(current, removed);
		// A discovery of the root that was added back waits for the one that runs.
		const again = testing.discover(app);
		earlier.resolve(listed);
		await Promise.all([discovery, again]);
		assert.deepEqual(testNames(current), ['adds']);
		assert.equal(current.error, undefined);
		assert.equal(current.busy, false);
		assert.deepEqual(testNames(removed), []);
		assert.equal(removed.busy, false);
		assert.deepEqual([...testing.cases.values()], [{ root: app, index: 0 }]);
		assert.deepEqual(compiled.map(args => args[1]), [app]);
		assert.deepEqual(stubbed.output, []);
	} finally { await stubbed.dispose(); }
});

test('a discovery whose root a re-rooting removed meanwhile records no tests for it', async () => {
	const stubbed = await host();
	const { app, top } = stubbed;
	try {
		const testing = await stubbed.testing();
		const removed = testing.roots.get(app)!;
		const earlier = stubbed.holdCompiler();
		const discovery = testing.discover(app);
		await settle();
		await stubbed.deletePackage(testing);
		assert.deepEqual([...testing.roots.keys()], [top]);
		earlier.resolve(listed);
		await discovery;
		assert.deepEqual([...testing.cases.keys()], []);
		assert.deepEqual(testNames(removed), []);
		assert.equal(removed.busy, false);
		assert.deepEqual([...testing.roots.keys()], [top]);
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

test('Test Project reports that the folder of the active file does not exist, and adds no root above it', async () => {
	const stubbed = await host();
	const { app, compiled, commands } = stubbed;
	try {
		const testing = await stubbed.testing();
		assert.deepEqual([...testing.roots.keys()], [app]);
		// top/a is deleted while X.tz stays open and active: the walk up from it ends at top, which holds other files.
		await stubbed.deleteFolder(testing);
		assert.deepEqual([...testing.roots.keys()], []);
		const summary = await commands.get('tsuzuri.test')!();
		assert.deepEqual([...testing.roots.keys()], []);
		assert.deepEqual(compiled, []);
		assert.deepEqual(stubbed.output, [`Error: The folder of X.tz does not exist: ${app}`]);
		assert.equal(summary, undefined);
	} finally { await stubbed.dispose(); }
});

test('Test Project reports that the folder of tsuzuri.projectPath does not exist, as opening a document does', async () => {
	const stubbed = await host();
	const { top, compiled, commands, files } = stubbed;
	try {
		const testing = await stubbed.testing();
		const configured = path.join(top, 'gone');
		files.configured = configured;
		const summary = await commands.get('tsuzuri.test')!();
		assert.equal(testing.roots.has(configured), false);
		assert.deepEqual(compiled, []);
		assert.deepEqual(stubbed.output, [`Error: The project folder of tsuzuri.projectPath does not exist: ${configured}`]);
		assert.equal(summary, undefined);
	} finally { await stubbed.dispose(); }
});

test('a document whose folder is deleted while its root is read adds no root above it when the lookup ends', async () => {
	const stubbed = await host();
	const { app } = stubbed;
	try {
		const testing = await stubbed.testing();
		// The lookup for top/a/Y.tz reads the root top/a; top/a is deleted, and the re-rooting removes it, before the lookup
		// returns. The lookup is read again, and the walk up from Y.tz now ends at top.
		const lookup = stubbed.pauseLookup();
		stubbed.open(stubbed.document(path.join(app, 'Y.tz')));
		await lookup.reached;
		await stubbed.deleteFolder(testing);
		assert.deepEqual([...testing.roots.keys()], []);
		lookup.resume();
		await settle();
		assert.deepEqual([...testing.roots.keys()], []);
		assert.deepEqual(stubbed.output, []);
	} finally { await stubbed.dispose(); }
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
		await until(() => languageClients[0].stopped, 'the restart stops the server of top/a');
		const other = path.join(top, 'b');
		stubbed.open(stubbed.document(path.join(other, 'Main.tz')));
		await until(() => languageClients.length === 2, 'the document of top/b starts a client');
		assert.deepEqual(languageClients.map(client => client.root), [app, other]);
		stopping.resolve();
		await restart;
		// The restart reopens the documents without waiting for them: wait until each running server has its client in
		// the map (never true with the old deactivate(), whose untracked server keeps running), then let anything late run.
		const tracked = () => languageClients.filter(client => !client.stopped);
		await until(() => tracked().length >= 2 && tracked().every(client => api.clients.get(client.root) === client), 'each running server is tracked');
		await settle();
		// Each server that runs has its client in the map, one for each root.
		const running = languageClients.filter(client => !client.stopped);
		assert.deepEqual(running.map(client => client.root).sort(), [app, other].sort());
		for (const client of running) { assert.equal(api.clients.get(client.root), client); }
		assert.deepEqual(stubbed.output, []);
	} finally { await stubbed.dispose(); }
});
