import * as assert from 'node:assert/strict';
import { mkdtemp, mkdir, readFile, writeFile, rm } from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { affectsRoot, assertSupportedHost, commandArguments, defaultNamespace, endsInPath, inRoot, isNamespace, jsonLines, libraryModules, libraryQualifier, lldbLaunch, notifiesServer, plainPath, projectFile, projectRoot, reportedByServer, rootSpelling, reservedWords, runProcess, sourcePosition, supportsDebug, supportsHost, testDebugArguments, testRunnerFiles, testRunnerPath } from '../core';

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
	} finally { await rm(root, { recursive: true, force: true }); }
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

test('the language server hears of every source and manifest event, and of the other deletions that can change its root', () => {
	// The source watcher reported these before G39 added the delete watcher of '**'; module discovery reads sources under
	// target/tz/ outside a knowledge graph, so they are not filtered.
	for (const file of ['/w/app/Main.tz', '/w/app/target/tz/gen/Gen.tz', '/w/app/target/tz/kg/Show.tc', '/w/app/.hidden/Point.tt', '/w/app/lib/Tsuzuri.toml', '/w/app/Geometry', '/w/app/target']) {
		assert.ok(notifiesServer('/w/app', file, 'linux'), file);
	}
	for (const file of ['/w/app/.tsuzuri', '/w/app/.tsuzuri/test/runner-1', '/w/app/target/tz/kg', '/w/app/target/tz/kg/nodes.jsonl', '/w/app/.git']) {
		assert.ok(!notifiesServer('/w/app', file, 'linux'), file);
	}
	assert.ok(notifiesServer('c:\\w\\app', 'C:\\w\\app\\target\\tz\\gen\\Gen.tz', 'win32'));
	assert.ok(!notifiesServer('c:\\w\\app', 'C:\\w\\app\\target\\tz\\kg', 'win32'));
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
