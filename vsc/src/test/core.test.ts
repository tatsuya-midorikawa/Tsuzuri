import * as assert from 'node:assert/strict';
import { chmod, mkdtemp, mkdir, readFile, writeFile, rm, symlink } from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { affectsRoot, assertSupportedHost, commandArguments, defaultNamespace, DirectoryEntry, directlyIn, directoryExists, endsInPath, inRoot, isNamespace, jsonLines, libraryModules, libraryQualifier, lldbLaunch, plainPath, projectFile, ProjectRoot, projectRoot, ProjectRoots, reportedByServer, RootFileSystem, rootSpelling, reservedWords, runProcess, sourcePosition, sourceSelector, supportsDebug, supportsHost, testDebugArguments, testRunnerFiles, testRunnerPath } from '../core';

test('only the five supported hosts enable the IDE, with debugging disabled on Windows ARM64', () => {
	const supported = ['darwin-arm64', 'linux-arm64', 'linux-x64', 'win32-arm64', 'win32-x64'];
	for (const platform of ['darwin', 'linux', 'win32', 'freebsd', 'android', 'aix', 'openbsd', 'sunos', 'web']) {
		for (const architecture of ['x64', 'arm64', 'ia32', 'arm', 'riscv64', 'ppc64', 's390x']) {
			const host = `${platform}-${architecture}`;
			const expected = supported.includes(host);
			assert.equal(supportsHost(platform, architecture), expected, host);
			assert.equal(supportsDebug(platform, architecture), expected && host !== 'win32-arm64', host);
			if (expected) {
				assert.doesNotThrow(() => assertSupportedHost(platform, architecture), host);
			} else {
				assert.throws(() => assertSupportedHost(platform, architecture), {
					message: `Unsupported Tsuzuri extension host: ${host}. Supported hosts: ${supported.join(', ')}.`,
				});
			}
		}
	}
	assert.equal(supportsHost(), supported.includes(`${process.platform}-${process.arch}`));
	for (const action of ['check', 'build', 'run', 'test', 'wasm'] as const) {
		assert.ok(commandArguments(action, '/project').length);
	}
});

test('unsupported hosts fail activation and toolchain resolution before any IDE or compiler work', async () => {
	const sources = new Map(await Promise.all(['core', 'extension', 'toolchain'].map(async name =>
		[name, await readFile(path.join(__dirname, '..', `${name}.js`), 'utf8')] as const)));
	const modules = new Map<string, Record<string, unknown>>();
	const unexpected = () => { throw new Error('An unsupported host must not register IDE features or resolve a development compiler.'); };
	const vscode = {
		workspace: { isTrusted: true, getConfiguration: unexpected },
		window: { createOutputChannel: unexpected },
	};
	function load(name: string): Record<string, unknown> {
		const cached = modules.get(name);
		if (cached) { return cached; }
		const source = sources.get(name);
		assert.ok(source);
		const exports: Record<string, unknown> = {};
		modules.set(name, exports);
		runInNewContext(source, {
			exports,
			process: { platform: 'darwin', arch: 'x64' },
			require(id: string) {
				if (id === 'vscode') { return vscode; }
				if (id.startsWith('node:')) { return require(id); }
				if (sources.has(id.slice(2))) { return load(id.slice(2)); }
				return {};
			},
		});
		return exports;
	}
	for (const [module, name] of [['extension', 'activate'], ['toolchain', 'toolchain']]) {
		const invoke = load(module)[name];
		assert.ok(typeof invoke === 'function');
		await assert.rejects(async () => invoke({}), /Unsupported Tsuzuri extension host: darwin-x64\./);
	}
});

test('compiler arguments keep paths literal and debug builds unoptimized', () => {
	const root = path.join(os.tmpdir(), 'space & quote\' project');
	const args = commandArguments('debug', root, 3);
	assert.equal(args[1], root);
	assert.ok(args.includes('-O0'));
	assert.ok(args.includes('-g'));
	assert.ok(args.includes('--trap-info'));
	assert.ok(!args.includes('-O3'));
	assert.deepEqual(commandArguments('check', root, 0, true), ['check', root, '--deny-warnings']);
	assert.throws(() => commandArguments('build', root, 4));
});

test('debug launches load the LLDB formatters first and add macOS DWARF', () => {
	const launch = lldbLaunch('/p/.tsuzuri/debug/Main', '/ext/resources/lldb/tsuzuri_lldb.py', { initCommands: ['settings set a b'], preRunCommands: ['c'] }, 'darwin');
	assert.deepEqual(launch.initCommands, ['command script import "/ext/resources/lldb/tsuzuri_lldb.py"', 'settings set a b']);
	assert.deepEqual(launch.preRunCommands, ['c', 'target symbols add "/p/.tsuzuri/debug/Main.dwarf"']);
	assert.equal(launch.type, 'lldb');
	assert.equal(launch.terminal, 'integrated');
	const windows = lldbLaunch('C:\\p\\Main.exe', 'C:\\Program Files\\ext\\resources\\lldb\\tsuzuri_lldb.py', {}, 'win32');
	assert.deepEqual(windows.initCommands, ['command script import "C:/Program Files/ext/resources/lldb/tsuzuri_lldb.py"']);
	assert.deepEqual(windows.preRunCommands, []);
	assert.deepEqual(lldbLaunch('/p/Main', '/f.py', {}, 'linux').preRunCommands, []);
});

test('debugging a test builds that test alone with debug information at a runner path of its own', () => {
	const root = path.join(os.tmpdir(), 'space & quote\' project');
	assert.deepEqual(testDebugArguments(root, 3, '42-1', 'linux'), ['test', root, '--index', '3', '-g', '-o', path.join(root, '.tsuzuri', 'test', 'runner-42-1'), '--json']);
	assert.equal(testDebugArguments(root, 0, '42-2', 'win32')[6], path.join(root, '.tsuzuri', 'test', 'runner-42-2.exe'));
	// Concurrent debug runs never share a runner, and each run removes its runner and debug information.
	assert.notEqual(testRunnerPath(root, '42-1', 'linux'), testRunnerPath(root, '42-2', 'linux'));
	const linux = testRunnerPath(root, '42-1', 'linux');
	assert.deepEqual(testRunnerFiles(linux), [linux, `${linux}.dwarf`]);
	const windows = testRunnerPath(root, '42-2', 'win32');
	assert.deepEqual(testRunnerFiles(windows), [windows, `${windows}.dwarf`, path.join(root, '.tsuzuri', 'test', 'runner-42-2.pdb')]);
});

test('new projects suggest a PascalCase namespace and accept identifiers joined by ::', () => {
	for (const [folder, namespace] of [['my-app', 'MyApp'], ['MyApp', 'MyApp'], ['hello_world', 'HelloWorld'], ['ex1', 'Ex1'], ['2024 app', 'App'], ['\u65e5\u672c', 'App']]) {
		assert.equal(defaultNamespace(folder), namespace, folder);
	}
	for (const text of ['Sample', 'Acme::Tools', 'lower::case_1', 'Acme::_internal', 'Acme::Maybe', 'Acme::std', 'Std', 'Tasks', 'Task', 'Acme::Task', 'task']) {
		assert.ok(isNamespace(text), text);
	}
	for (const text of ['', '1st', 'Acme.Tools', 'Acme::::Tools', 'Acme:Tools', 'Acme::', 'my-app', 'A::'.repeat(16) + 'A', 'Acme::_', 'Acme::match', 'Maybe', 'IO::Extra', 'std', 'std::Extra']) {
		assert.ok(!isNamespace(text), text);
	}
});

test('static completions offer library members after a module but leave namespace paths to the language server', () => {
	for (const [prefix, module] of [['let x = Maybe.ma', 'Maybe'], ['IO.', 'IO'], ['Holder { value:Maybe.ma', 'Maybe'], ['let x = std::Maybe.ma', 'Maybe'], ['(std::IO.', 'IO'], ['Sample::Maybe.ma', undefined], ['Sample::std::Maybe.ma', undefined], ['Sample::Shape.Maybe.', undefined], ['Shape.Maybe.ma', undefined], ['let x = maybe.', undefined], ['Sample::', undefined]]) {
		assert.equal(libraryQualifier(prefix!), module, prefix);
	}
	for (const prefix of ['Sample::', 'Sample::Fe', 'let ys = x::x', 'def f::Demo::', 'def f::Demo::Sh', 'rec go::Demo::Shapes::']) {
		assert.ok(endsInPath(prefix), prefix);
	}
	for (const prefix of ['def main :: ', 'Sample::Shape.ar', 'let x = Maybe.', 'def f::', 'def f::i', 'export def f::i6', 'rec go::', 'and step::De']) {
		assert.ok(!endsInPath(prefix), prefix);
	}
});

test('namespace rules use the lexer reserved words and reserved standard library modules', async () => {
	const repository = path.resolve(__dirname, '../../..');
	const lexer = await readFile(path.join(repository, 'src/lexer.rs'), 'utf8');
	const identifier = lexer.slice(lexer.indexOf('fn identifier'), lexer.indexOf('name => TokenKind::Ident'));
	assert.deepEqual([...identifier.matchAll(/"(\w+)" => TokenKind::/g)].map(match => match[1]).sort(), [...reservedWords].sort());
	const stdlib = await readFile(path.join(repository, 'src/stdlib.rs'), 'utf8');
	const start = stdlib.indexOf('pub const RESERVED_MODULES');
	const modules = stdlib.slice(start, stdlib.indexOf('];', start));
	assert.deepEqual([...modules.matchAll(/"(\w+)"/g)].map(match => match[1]).sort(), [...libraryModules].sort());
});

test('project root respects nested projects and source subdirectories', async () => {
	const root = await mkdtemp(path.join(os.tmpdir(), 'tsuzuri-project-'));
	const loose = await mkdtemp(path.join(os.tmpdir(), 'tsuzuri-loose-'));
	try {
		const app = path.join(root, 'app');
		await mkdir(path.join(app, 'Geometry'), { recursive: true });
		await writeFile(path.join(app, 'Main.tz'), '42');
		const file = path.join(app, 'Geometry', 'Point.tz');
		await writeFile(file, '');
		assert.equal(await projectRoot(file, root), app);
		assert.equal(await projectRoot(path.join(app, 'Geometry', 'Unsaved.tz'), root), app);
		await writeFile(path.join(root, 'Library.tz'), '');
		assert.equal(await projectRoot(path.join(root, 'Library.tz'), root), root);
		// G40 D1: a file without a marker takes the highest directory that contains no other project.
		await mkdir(path.join(root, 'scratch'));
		await writeFile(path.join(root, 'scratch', 'Try.tz'), '');
		assert.equal(await projectRoot(path.join(root, 'scratch', 'Try.tz'), root), path.join(root, 'scratch'));
		// A file beside the package stays at the folder, which is a remaining root (G40 spec 1.4); a marked root never is,
		// also with another package below it.
		assert.deepEqual(await new ProjectRoots().find(path.join(root, 'Library.tz'), root), { root, remaining: true });
		assert.deepEqual(await new ProjectRoots().find(path.join(root, 'scratch', 'Try.tz'), root), { root: path.join(root, 'scratch'), remaining: false });
		await mkdir(path.join(app, 'lib'));
		await writeFile(path.join(app, 'lib', 'Tsuzuri.toml'), '');
		assert.deepEqual(await new ProjectRoots().find(path.join(app, 'Main.tz'), root), { root: app, remaining: false });
		assert.deepEqual(await new ProjectRoots().find(path.join(app, 'Geometry', 'Point.tz'), root), { root: app, remaining: false });
		// Outside every workspace folder, the file's directory is the root, and a remaining one when it holds a project.
		assert.deepEqual(await new ProjectRoots().find(path.join(root, 'Notes.tz')), { root, remaining: true });
		assert.deepEqual(await new ProjectRoots().find(path.join(root, 'scratch', 'Try.tz')), { root: path.join(root, 'scratch'), remaining: false });
		// An unsaved file in a directory that does not exist. Such a root has no client or test root (G40 D4).
		assert.deepEqual(await new ProjectRoots().find(path.join(root, 'missing', 'deeper', 'New.tz'), root), { root: path.join(root, 'missing'), remaining: false });
		assert.equal(await directoryExists(path.join(root, 'missing')), false);
		assert.equal(await directoryExists(path.join(root, 'Library.tz')), false);
		assert.equal(await directoryExists(path.join(root, 'Library.tz', 'below')), false);
		assert.equal(await directoryExists(root), true);
		// A folder without a package keeps one root for every file, as before.
		await mkdir(path.join(loose, 'a', 'b'), { recursive: true });
		await writeFile(path.join(loose, 'a', 'X.tz'), '');
		await writeFile(path.join(loose, 'Top.tz'), '');
		assert.equal(await projectRoot(path.join(loose, 'a', 'X.tz'), loose), loose);
		assert.equal(await projectRoot(path.join(loose, 'a', 'b', 'Y.tz'), loose), loose);
		assert.deepEqual(await new ProjectRoots().find(path.join(loose, 'Top.tz'), loose), { root: loose, remaining: false });
		if (process.platform !== 'win32') {
			// Module discovery stops at a directory or source symbolic link, so a folder with one holds another project.
			await mkdir(path.join(loose, 'real'));
			await symlink(path.join(loose, 'real'), path.join(loose, 'linked'));
			assert.equal(await projectRoot(path.join(loose, 'a', 'X.tz'), loose), path.join(loose, 'a'));
			await rm(path.join(loose, 'linked'));
			await writeFile(path.join(loose, 'real', 'data.txt'), '');
			await symlink(path.join(loose, 'real', 'data.txt'), path.join(loose, 'data.txt'));
			assert.equal(await projectRoot(path.join(loose, 'a', 'X.tz'), loose), loose);
			await symlink(path.join(loose, 'real', 'Gone.tz'), path.join(loose, 'Broken.tz'));
			assert.equal(await projectRoot(path.join(loose, 'a', 'X.tz'), loose), path.join(loose, 'a'));
		}
	} finally {
		await rm(root, { recursive: true, force: true });
		await rm(loose, { recursive: true, force: true });
	}
});

/**
 * A file system of directories and their entries, which counts the directories that it lists. A case-insensitive one
 * (macOS and Windows by default) finds a file under any case of its name, as `access` does there. A directory can be
 * unreadable (it cannot be listed) or unsearchable (nothing in it can be inspected, and no directory below it listed).
 */
class VirtualFileSystem implements RootFileSystem {
	readonly directories = new Map<string, DirectoryEntry[]>();
	readonly unreadable = new Set<string>();
	readonly unsearchable = new Set<string>();
	lists = 0;
	/** A directory whose listing, once taken, is returned only after `resume`: a change can come meanwhile. */
	pause?: { directory: string; listed: () => void; resume: Promise<void> };
	constructor(readonly top: string, readonly caseInsensitive = false) { this.directories.set(top, []); }
	add(file: string, kind: DirectoryEntry['kind'] = 'file', target?: DirectoryEntry['target']) {
		const parent = path.dirname(file);
		if (!this.directories.has(parent)) { this.add(parent, 'directory'); }
		this.directories.get(parent)!.push({ name: path.basename(file), kind, target });
		if (kind === 'directory') { this.directories.set(file, []); }
	}
	remove(file: string) {
		const entries = this.directories.get(path.dirname(file))!;
		entries.splice(entries.findIndex(entry => entry.name === path.basename(file)), 1);
	}
	async list(directory: string) {
		this.lists++;
		if (this.unreadable.has(directory) || this.unsearchable.has(path.dirname(directory))) {
			throw Object.assign(new Error(`EACCES: ${directory}`), { code: 'EACCES' });
		}
		const entries = this.directories.get(directory);
		if (!entries) { throw Object.assign(new Error(`ENOENT: ${directory}`), { code: 'ENOENT' }); }
		const listing = [...entries];
		const pause = this.pause;
		if (pause?.directory === directory) {
			this.pause = undefined;
			pause.listed();
			await pause.resume;
		}
		return listing;
	}
	/** The entry of `file`, under the volume's case rule; none in a directory that cannot be searched. */
	private entry(file: string) {
		if (this.unsearchable.has(path.dirname(file))) { return undefined; }
		const fold = (name: string) => this.caseInsensitive ? name.toLowerCase() : name;
		return this.directories.get(path.dirname(file))?.find(entry => fold(entry.name) === fold(path.basename(file)));
	}
	async exists(file: string) { return this.entry(file) !== undefined; }
	async isDirectory(file: string) { return this.directories.has(file); }
	async isRegularFile(file: string) { return this.entry(file)?.kind === 'file'; }
}

test('a folder that module discovery cannot read within its limits counts as one that holds another project', async () => {
	const top = path.resolve('/w');
	const notes = path.join(top, 'notes', 'X.tz');
	const rootOf = (files: VirtualFileSystem) => new ProjectRoots(files).find(notes, top).then(found => found.root);
	const tree = () => { const files = new VirtualFileSystem(top); files.add(notes); return files; };
	// 1024 directories with the root are read, the 1025th is not.
	const wide = tree();
	for (let index = 0; index < 1022; index++) { wide.add(path.join(top, 'many', `d${index}`), 'directory'); }
	assert.equal(wide.directories.size, 1025);
	assert.equal(await rootOf(wide), path.join(top, 'notes'));
	wide.remove(path.join(top, 'many', 'd0'));
	assert.equal(await rootOf(wide), top);
	// A directory 15 segments below the root is read, one 16 segments below is not.
	const deep = tree();
	const chain = Array.from({ length: 15 }, (_, index) => `s${index}`);
	deep.add(path.join(top, ...chain), 'directory');
	assert.equal(await rootOf(deep), top);
	deep.add(path.join(top, ...chain, 's15'), 'directory');
	assert.equal(await rootOf(deep), path.join(top, 'notes'));
	// Source and directory symbolic links, broken ones included, and sources that are not regular files stop module
	// discovery; a link to another file does not.
	for (const [name, kind, target, holds] of [
		['Link.tz', 'symlink', 'file', true], ['linked', 'symlink', 'directory', true], ['broken', 'symlink', undefined, true],
		['data.txt', 'symlink', 'file', false], ['fifo', 'symlink', 'other', false], ['Pipe.tz', 'other', undefined, true], ['socket', 'other', undefined, false],
	] as const) {
		const files = tree();
		files.add(path.join(top, 'sub', name), kind, target);
		assert.equal(await rootOf(files), holds ? path.join(top, 'notes') : top, name);
	}
	// Module discovery reads no name that starts with '.', and no knowledge graph's directory.
	const hidden = tree();
	hidden.add(path.join(top, '.git', 'Main.tz'));
	hidden.add(path.join(top, 'target', 'tz', 'kg', '.tsuzuri-kg'));
	hidden.add(path.join(top, 'target', 'tz', 'kg', 'Main.tz'));
	assert.equal(await rootOf(hidden), top);
	hidden.add(path.join(top, 'target', 'Main.tz'));
	assert.equal(await rootOf(hidden), path.join(top, 'notes'));
	// An unsaved file in a directory that does not exist: the directory is empty.
	const unlisted = new VirtualFileSystem(top);
	assert.deepEqual(await new ProjectRoots(unlisted).find(path.join(top, 'gone', 'New.tz'), top), { root: top, remaining: false });
});

test('a knowledge graph is told by its marker as module discovery tells it, also when it cannot be listed or searched', async () => {
	const top = path.resolve('/w');
	const notes = path.join(top, 'notes', 'X.tz');
	const graph = path.join(top, 'target', 'tz', 'kg');
	const marker = path.join(graph, '.tsuzuri-kg');
	// `collect_sources` checks a directory's marker with `symlink_metadata` before it lists the directory: it skips one
	// whose marker is a regular file, also when the directory cannot be listed, and lists any other, which fails when the
	// directory cannot be listed or a directory below it cannot be read. A marker that is a link or a directory does not
	// count, and neither does one in a directory that cannot be searched.
	for (const [label, build, holds, caseInsensitive] of [
		['a graph', files => files.add(marker), false],
		['a graph that cannot be listed', files => { files.add(marker); files.unreadable.add(graph); }, false],
		['a directory without a marker that cannot be listed', files => { files.add(graph, 'directory'); files.unreadable.add(graph); }, true],
		['a linked marker in a directory that cannot be listed', files => { files.add(marker, 'symlink', 'file'); files.unreadable.add(graph); }, true],
		['a directory marker in a directory that cannot be listed', files => { files.add(marker, 'directory'); files.unreadable.add(graph); }, true],
		['a marker in a directory that cannot be searched, with a directory in it', files => {
			files.add(marker); files.add(path.join(graph, 'nodes'), 'directory'); files.unsearchable.add(graph);
		}, true],
		['a marker in a directory that cannot be searched, with a file in it', files => {
			files.add(marker); files.add(path.join(graph, 'manifest.json')); files.unsearchable.add(graph);
		}, false],
		['a marker under another case, on a case-insensitive volume', files => { files.add(path.join(graph, '.TSUZURI-KG')); files.add(path.join(graph, 'Main.tz')); }, false, true],
		['a marker under another case, on a case-sensitive volume', files => { files.add(path.join(graph, '.TSUZURI-KG')); files.add(path.join(graph, 'Main.tz')); }, true, false],
	] as [string, (files: VirtualFileSystem) => void, boolean, boolean?][]) {
		const files = new VirtualFileSystem(top, caseInsensitive);
		files.add(notes);
		build(files);
		assert.deepEqual(await new ProjectRoots(files).find(notes, top), { root: holds ? path.join(top, 'notes') : top, remaining: false }, label);
	}
	// A graph that was the folder of a lookup first is skipped as well by the scan of a folder above it, which takes what
	// the lookup found.
	const files = new VirtualFileSystem(top);
	files.add(notes);
	files.add(marker);
	files.unreadable.add(graph);
	const roots = new ProjectRoots(files);
	assert.deepEqual(await roots.find(path.join(graph, 'Notes.tz'), top), { root: graph, remaining: true });
	assert.deepEqual(await roots.find(notes, top), { root: top, remaining: false });
	// The same on the disk of this host, where permissions apply (not on Windows, and not to the superuser).
	if (process.platform === 'win32' || process.getuid?.() === 0) { return; }
	const disk = await mkdtemp(path.join(os.tmpdir(), 'tsuzuri-graph-'));
	const locked: string[] = [];
	const lock = async (directory: string, mode: number) => { await chmod(directory, mode); locked.push(directory); };
	try {
		const tree = async (name: string) => {
			const folder = path.join(disk, name);
			await mkdir(path.join(folder, 'notes'), { recursive: true });
			await mkdir(path.join(folder, 'target', 'tz', 'kg'), { recursive: true });
			await writeFile(path.join(folder, 'notes', 'X.tz'), '');
			return { folder, kg: path.join(folder, 'target', 'tz', 'kg') };
		};
		const rootOn = async (folder: string) => path.relative(folder, await projectRoot(path.join(folder, 'notes', 'X.tz'), folder)) || '.';
		const unlisted = await tree('unlisted');
		await writeFile(path.join(unlisted.kg, '.tsuzuri-kg'), '');
		await lock(unlisted.kg, 0o311);
		assert.equal(await rootOn(unlisted.folder), '.', 'a graph that cannot be listed');
		const plain = await tree('plain');
		await lock(plain.kg, 0o311);
		assert.equal(await rootOn(plain.folder), 'notes', 'a directory without a marker that cannot be listed');
		const linked = await tree('linked');
		await writeFile(path.join(linked.folder, 'target', 'tz', 'real'), '');
		await symlink(path.join('..', 'real'), path.join(linked.kg, '.tsuzuri-kg'));
		await lock(linked.kg, 0o311);
		assert.equal(await rootOn(linked.folder), 'notes', 'a linked marker in a directory that cannot be listed');
		const unsearched = await tree('unsearched');
		await writeFile(path.join(unsearched.kg, '.tsuzuri-kg'), '');
		await mkdir(path.join(unsearched.kg, 'nodes'));
		await lock(unsearched.kg, 0o644);
		assert.equal(await rootOn(unsearched.folder), 'notes', 'a marker in a directory that cannot be searched, with a directory in it');
	} finally {
		for (const directory of locked) { await chmod(directory, 0o755); }
		await rm(disk, { recursive: true, force: true });
	}
});

test('project roots read each directory once until a change that module discovery can see', async () => {
	const top = path.resolve('/w');
	const notes = path.join(top, 'notes', 'X.tz');
	const files = new VirtualFileSystem(top);
	files.add(notes);
	for (let index = 0; index < 4095; index++) { files.add(path.join(top, 'src', `S${index}.tz`)); }
	const roots = new ProjectRoots(files);
	assert.deepEqual(await roots.find(notes, top), { root: top, remaining: false });
	const lists = files.lists;
	assert.ok(lists > 0);
	assert.deepEqual(await roots.find(notes, top), { root: top, remaining: false });
	assert.deepEqual(await roots.find(path.join(top, 'notes', 'Other.tz'), top), { root: top, remaining: false });
	assert.deepEqual(await roots.find(path.join(top, 'Top.tz'), top), { root: top, remaining: false });
	assert.equal(files.lists, lists);
	// A marker below the folder shows after the change clears the cache.
	files.add(path.join(top, 'app', 'Main.tz'));
	assert.equal((await roots.find(notes, top)).root, top);
	assert.equal(roots.changed(top, path.join(top, 'app')), true);
	assert.deepEqual(await roots.find(notes, top), { root: path.join(top, 'notes'), remaining: false });
	assert.deepEqual(await roots.find(path.join(top, 'Top.tz'), top), { root: top, remaining: true });
	files.remove(path.join(top, 'app'));
	assert.equal(roots.changed(top, path.join(top, 'app')), true);
	assert.equal((await roots.find(notes, top)).root, top);
	// 4096 sources are read, the 4097th is not: a created source splits the root, and its deletion joins it again.
	const extra = path.join(top, 'src', 'Extra.tz');
	files.add(extra);
	assert.equal((await roots.find(notes, top)).root, top);
	assert.equal(roots.changed(top, extra), true);
	assert.equal((await roots.find(notes, top)).root, path.join(top, 'notes'));
	files.remove(extra);
	assert.equal(roots.changed(top, extra), true);
	assert.equal((await roots.find(notes, top)).root, top);
	// Changes that module discovery does not read keep the cache: names that start with '.' and knowledge graphs.
	const kept = files.lists;
	for (const file of [path.join(top, '.git', 'index.lock'), path.join(top, 'src', '.cache'), path.join(top, 'target', 'tz', 'kg'), path.join(top, 'app', '.tsuzuri', 'bin')]) {
		assert.equal(roots.changed(top, file), false, file);
	}
	assert.equal((await roots.find(notes, top)).root, top);
	assert.equal(files.lists, kept);
});

test('a directory that a scan listed is not listed again for a file opened in it later, and the roots stay the same', async () => {
	const top = path.resolve('/w');
	const files = new VirtualFileSystem(top);
	for (const file of ['a/X.tz', 'b/Y.tz', 'b/c/Z.tz', 'b/c/d/W.tz']) { files.add(path.join(top, file)); }
	const roots = new ProjectRoots(files);
	assert.deepEqual(await roots.find(path.join(top, 'a', 'X.tz'), top), { root: top, remaining: false });
	// The scan of the folder for a/X.tz listed b/ and the directories below it; a file opened there lists none again.
	const lists = files.lists;
	for (const file of ['b/Y.tz', 'b/c/Z.tz', 'b/c/d/W.tz']) {
		assert.deepEqual(await roots.find(path.join(top, file), top), { root: top, remaining: false }, file);
	}
	assert.equal(files.lists, lists);
	// Each file has the root that a lookup of its own finds, in either order of the lookups: with a marker beside the
	// files, at the limits of the directories and the depth, which the folder reaches and a directory below it does not.
	const tree = (sources: string[]) => {
		const files = new VirtualFileSystem(top);
		for (const file of sources) { files.add(path.join(top, file)); }
		return files;
	};
	const wide = tree(['notes/X.tz', 'many/M.tz', 'many/d0/D.tz', 'b/c/Y.tz']);
	for (let index = 1; index < 1021; index++) { wide.add(path.join(top, 'many', `d${index}`), 'directory'); }
	const chain = Array.from({ length: 16 }, (_, index) => `s${index}`);
	for (const files of [
		tree(['a/X.tz', 'b/c/Y.tz', 'b/pkg/Main.tz', 'b/pkg/Util.tz', 'b/Z.tz', 'd/e/f/W.tz', 'Top.tz']),
		wide,
		tree(['notes/X.tz', 's0/F.tz', `${chain.join('/')}/G.tz`, `${chain.slice(0, 8).join('/')}/H.tz`]),
	]) {
		const sources = [...files.directories].flatMap(([directory, entries]) =>
			entries.filter(entry => entry.kind === 'file').map(entry => path.join(directory, entry.name)));
		const alone = await Promise.all(sources.map(file => new ProjectRoots(files).find(file, top)));
		for (const order of [sources, [...sources].reverse()]) {
			const shared = new ProjectRoots(files);
			for (const file of order) { assert.deepEqual(await shared.find(file, top), alone[sources.indexOf(file)], file); }
		}
	}
});

test('a root that was read while a change cleared the cache is read again before it is used', async () => {
	const top = path.resolve('/w');
	const notes = path.join(top, 'notes', 'X.tz');
	const app = path.join(top, 'app');
	const files = new VirtualFileSystem(top);
	files.add(notes);
	files.add(path.join(app, 'Util.tz'));
	const roots = new ProjectRoots(files);
	let listed!: () => void, resume!: () => void;
	const reached = new Promise<void>(resolve => { listed = resolve; });
	files.pause = { directory: app, listed, resume: new Promise<void>(resolve => { resume = resolve; }) };
	const used: ProjectRoot[] = [];
	const settled = roots.settled(() => roots.find(notes, top), found => { used.push(found); return found.root; });
	// The scan of the folder has listed app/ without a marker. A marker is created there, and its event clears the cache
	// before the lookup ends: a client started from that lookup would serve the folder beside the clients of the new roots.
	await reached;
	files.add(path.join(app, 'Main.tz'));
	assert.equal(roots.changed(top, path.join(app, 'Main.tz')), true);
	resume();
	assert.equal(await settled, path.join(top, 'notes'));
	assert.deepEqual(used, [{ root: path.join(top, 'notes'), remaining: false }]);
	// Without a change meanwhile, the lookup runs once, from the cache.
	const lists = files.lists;
	assert.equal(await roots.settled(() => roots.find(notes, top), found => found.root), path.join(top, 'notes'));
	assert.equal(files.lists, lists);
});

test('a marker that the volume finds under another case of its name holds another project', async () => {
	const top = path.resolve('/w');
	const app = path.join(top, 'app');
	const scratch = path.join(top, 'scratch');
	for (const caseInsensitive of [true, false]) {
		for (const marker of ['tsuzuri.toml', 'main.tz', 'MAIN.TZ']) {
			const files = new VirtualFileSystem(top, caseInsensitive);
			files.add(path.join(scratch, 'Try.tz'));
			files.add(path.join(app, marker));
			const roots = new ProjectRoots(files);
			const label = `${marker}, case-insensitive: ${caseInsensitive}`;
			// The walk up finds the marker under the volume's case rule, as the compiler does, so the scan
			// counts it too: otherwise the folder's client would serve the package's files beside the package's own.
			assert.deepEqual(await roots.find(path.join(app, 'Util.tz'), top), { root: caseInsensitive ? app : top, remaining: false }, label);
			assert.deepEqual(await roots.find(path.join(scratch, 'Try.tz'), top), { root: caseInsensitive ? scratch : top, remaining: false }, label);
			assert.deepEqual(await roots.find(path.join(top, 'Notes.tz'), top), { root: top, remaining: caseInsensitive }, label);
		}
	}
	// The same on the disk of this host, whichever case rule its temporary directory has.
	const disk = await mkdtemp(path.join(os.tmpdir(), 'tsuzuri-case-'));
	try {
		await mkdir(path.join(disk, 'app'));
		await mkdir(path.join(disk, 'scratch'));
		await writeFile(path.join(disk, 'app', 'tsuzuri.toml'), '');
		const insensitive = await projectRoot(path.join(disk, 'app', 'Util.tz'), disk) === path.join(disk, 'app');
		assert.equal(await projectRoot(path.join(disk, 'scratch', 'Try.tz'), disk), insensitive ? path.join(disk, 'scratch') : disk);
	} finally { await rm(disk, { recursive: true, force: true }); }
});

test('a remaining root serves only the files directly in it', () => {
	assert.ok(directlyIn('/w/loose', '/w/loose/Notes.tz', 'linux'));
	assert.ok(directlyIn('/w/loose', '/w/loose/..Notes.tz', 'linux'));
	for (const file of ['/w/loose', '/w/loose/App/Main.tz', '/w/loose/scratch/Try.tz', '/w/other/Notes.tz', '/w/Notes.tz', '/w/loose2/Notes.tz']) {
		assert.ok(!directlyIn('/w/loose', file, 'linux'), file);
	}
	assert.ok(directlyIn('c:\\w\\loose', 'C:\\W\\Loose\\Notes.tz', 'win32'));
	assert.ok(!directlyIn('c:\\w\\loose', 'C:\\w\\loose\\App\\Main.tz', 'win32'));
	assert.equal(sourceSelector('/w/a [b] {c}', false), '/w/a [[]b[]] [{]c[}]/**/*.{tz,tt,tc}');
	assert.equal(sourceSelector('/w/loose', true), '/w/loose/*.{tz,tt,tc}');
	assert.equal(sourceSelector('C:\\w\\loose', true), 'C:/w/loose/*.{tz,tt,tc}');
});

test('run results belong to the roots that contain their files, and deletions that module discovery reads change a root', () => {
	assert.ok(inRoot('/w/app', '/w/app', 'linux'));
	assert.ok(inRoot('/w/app', '/w/app/Main.tz', 'linux'));
	assert.ok(inRoot('/w/app', '/w/app/lib/Calc.tz', 'darwin'));
	assert.ok(inRoot('/w/app', '/w/app/..data/Calc.tz', 'linux'));
	for (const file of ['/w/lib/Calc.tz', '/w/application/Main.tz', '/w', '/other/app/Main.tz']) {
		assert.ok(!inRoot('/w/app', file, 'linux'), file);
	}
	// Windows compares paths without case, as its file systems do, and another drive is never inside.
	assert.ok(inRoot('c:\\w\\app', 'C:\\W\\App\\Main.tz', 'win32'));
	assert.ok(!inRoot('c:\\w\\app', 'd:\\w\\app\\Main.tz', 'win32'));
	assert.ok(!inRoot('c:\\w\\app', 'c:\\w\\lib\\Calc.tz', 'win32'));
	for (const file of ['/w/app/Main.tz', '/w/app/Geometry', '/w/app/target', '/w/app/target/Main.tz', '/w/app/target/tzx/a', '/w/app']) {
		assert.ok(affectsRoot('/w/app', file, 'linux'), file);
	}
	// Module discovery reads no name that starts with '.', and each build rewrites the knowledge graph in target/tz/, also
	// of a package under the root or, for a deletion outside every root, under its workspace folder.
	for (const file of ['/w/app/.tsuzuri', '/w/app/.tsuzuri/test/runner-1', '/w/app/Geometry/.hidden/Point.tz', '/w/app/target/tz/kg', '/w/app/target/tz/kg/nodes.jsonl', '/w/app/lib/target/tz/kg', '/w/lib/Calc.tz']) {
		assert.ok(!affectsRoot('/w/app', file, 'linux'), file);
	}
	for (const file of ['/w/app/lib/target/tz', '/w/app/lib/target/Calc.tz', '/w/app/lib/tz/kg']) { assert.ok(affectsRoot('/w/app', file, 'linux'), file); }
	assert.ok(!affectsRoot('c:\\w\\app', 'C:\\w\\app\\target\\tz\\kg', 'win32'));
	assert.ok(affectsRoot('c:\\w\\app', 'C:\\w\\app\\Shapes', 'win32'));
	for (const file of ['Main.tz', 'lib/Calc.tt', 'Show.tc', 'Tsuzuri.toml', 'C:\\w\\Main.tz', '/w/app/Tsuzuri.lock', 'C:\\w\\Tsuzuri.lock', 'Tsuzuri.lock']) { assert.ok(projectFile(file), file); }
	for (const file of ['README.md', 'Main.tzx', 'main.c', 'Tsuzuri.toml.bak', 'tz', '/w/app/Cargo.lock', '/w/app/MyTsuzuri.lock', 'Tsuzuri.lock.bak']) { assert.ok(!projectFile(file), file); }
});

test('compiler paths are spelled as VS Code names the project: no verbatim prefix, under the root as opened', () => {
	// fs::canonicalize makes the compiler's paths verbatim on Windows (G39 Phase 1 found it in the Windows CI).
	assert.equal(plainPath('\\\\?\\D:\\a\\Tsuzuri\\Main.tz'), 'D:\\a\\Tsuzuri\\Main.tz');
	assert.equal(plainPath('\\\\?\\UNC\\server\\share\\Main.tz'), '\\\\server\\share\\Main.tz');
	assert.equal(plainPath('\\\\?\\GLOBALROOT\\Device\\X'), '\\\\?\\GLOBALROOT\\Device\\X');
	assert.equal(plainPath('/private/tmp/p/Main.tz'), '/private/tmp/p/Main.tz');
	assert.equal(plainPath('Main.tz'), 'Main.tz');
	// A root opened through a symbolic link: the compiler names the real path.
	assert.equal(rootSpelling('/tmp/p', '/private/tmp/p', '/private/tmp/p/A/B.tz', 'darwin'), '/tmp/p/A/B.tz');
	assert.equal(rootSpelling('/tmp/p', '/private/tmp/p', '/tmp/p/B.tz', 'darwin'), '/tmp/p/B.tz');
	assert.equal(rootSpelling('/tmp/p', '/private/tmp/p', '/private/tmp/lib/C.tz', 'darwin'), '/private/tmp/lib/C.tz');
	// Windows compares paths without case, as VS Code's lower-case drive letters need.
	assert.equal(rootSpelling('d:\\a\\p', 'D:\\a\\p', 'D:\\a\\p\\Main.tz', 'win32'), 'D:\\a\\p\\Main.tz');
	assert.equal(inRoot('d:\\a\\p', plainPath('\\\\?\\D:\\a\\p\\Main.tz'), 'win32'), true);
});

test('compiler byte offsets become the positions of the text that VS Code shows', () => {
	const at = (text: string | number[], offset: number) => {
		const position = sourcePosition(typeof text === 'string' ? Buffer.from(text, 'utf8') : Uint8Array.from(text), offset);
		return [position.line, position.character];
	};
	assert.deepEqual(at('ab\ncd', 4), [1, 1]);
	assert.deepEqual(at('ab\ncd', 0), [0, 0]);
	// The byte order mark is not part of the editor's text, though the compiler's offsets count it.
	const bom = [0xef, 0xbb, 0xbf, ...Buffer.from('def x :: i64 = true\r\ndef y :: i64 = "\u{1f600}" + 1\r\n')];
	assert.deepEqual([18, 22, 39, 45].map(offset => at(bom, offset)), [[0, 15], [0, 19], [1, 15], [1, 19]]);
	for (const offset of [0, 1, 2, 3]) { assert.deepEqual(at(bom, offset), [0, 0], String(offset)); }
	assert.deepEqual(at('\ufeffx', 4), [0, 1]);
	// CRLF and a lone CR end a line, as in VS Code; the LF of a CRLF is still the end of its line.
	assert.deepEqual([1, 2, 3, 4].map(offset => at('a\r\nbc', offset)), [[0, 1], [0, 1], [1, 0], [1, 1]]);
	assert.deepEqual([1, 2, 3].map(offset => at('a\rb', offset)), [[0, 1], [1, 0], [1, 1]]);
	assert.deepEqual(at('a\r\n\r\nb', 5), [2, 0]);
	// Columns count UTF-16 code units: a supplementary-plane character is two, a CJK character one.
	assert.deepEqual(at('x = "\u{1f600}" + \u65e5 1', 9), [0, 7]);
	assert.deepEqual(at('x = "\u{1f600}" + \u65e5 1', 16), [0, 12]);
	// An offset inside a character starts at the character; one past the end is the end.
	assert.deepEqual(at('a\u00e9b', 2), [0, 1]);
	assert.deepEqual(at('ab\nc', 99), [1, 1]);
	assert.deepEqual(at('ab', -5), [0, 0]);
});

test('a run result duplicates a language server diagnostic of the same code that starts at the same position', () => {
	const server = [{ code: 'W1001', line: 2, character: 8 }, { code: 'E1003', line: 4, character: 0 }];
	assert.ok(reportedByServer({ code: 'W1001', line: 2, character: 8 }, server));
	assert.ok(reportedByServer({ code: 'E1003', line: 4, character: 0 }, server));
	assert.ok(!reportedByServer({ code: 'W1001', line: 2, character: 9 }, server));
	assert.ok(!reportedByServer({ code: 'W1001', line: 3, character: 8 }, server));
	assert.ok(!reportedByServer({ code: 'E1036', line: 2, character: 8 }, server));
	assert.ok(!reportedByServer({ code: 'E2006', line: 0, character: 0 }, []));
});

test('process execution captures Unicode without a shell and can be cancelled', async () => {
	const value = 'space & $() \' " \u{1f600}';
	const result = await runProcess(process.execPath, ['-e', 'process.stdout.write(process.argv[1]); process.stderr.write("err")', value]);
	assert.equal(result.stdout, value);
	assert.equal(result.stderr, 'err');
	assert.equal(result.code, 0);
	const controller = new AbortController();
	const running = runProcess(process.execPath, ['-e', 'setInterval(() => {}, 1000)'], { signal: controller.signal });
	controller.abort();
	await assert.rejects(running, /cancelled/);
	assert.deepEqual(jsonLines('{"index":0}\r\n{"index":1}\n'), [{ index: 0 }, { index: 1 }]);
	assert.throws(() => jsonLines('null'));
});
