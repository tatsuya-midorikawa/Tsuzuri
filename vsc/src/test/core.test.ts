import * as assert from 'node:assert/strict';
import { mkdtemp, mkdir, readFile, writeFile, rm } from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { test } from 'node:test';
import { commandArguments, defaultNamespace, endsInPath, isNamespace, jsonLines, libraryModules, libraryQualifier, lldbLaunch, projectRoot, reservedWords, runProcess, supportsDebug } from '../core';

test('Windows ARM64 disables only debugging and x86 is not an IDE target', () => {
	assert.equal(supportsDebug('win32', 'arm64'), false);
	assert.equal(supportsDebug('win32', 'ia32'), false);
	for (const [platform, architecture] of [['win32', 'x64'], ['darwin', 'x64'], ['darwin', 'arm64'], ['linux', 'x64'], ['linux', 'arm64']]) {
		assert.equal(supportsDebug(platform, architecture), true);
	}
	for (const action of ['check', 'build', 'run', 'test', 'wasm'] as const) {
		assert.ok(commandArguments(action, '/project').length);
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

test('new projects suggest a PascalCase namespace and accept identifiers joined by ::', () => {
	for (const [folder, namespace] of [['my-app', 'MyApp'], ['MyApp', 'MyApp'], ['hello_world', 'HelloWorld'], ['ex1', 'Ex1'], ['2024 app', 'App'], ['\u65e5\u672c', 'App']]) {
		assert.equal(defaultNamespace(folder), namespace, folder);
	}
	for (const text of ['Sample', 'Acme::Tools', 'lower::case_1', 'Acme::_internal', 'Acme::Maybe', 'Acme::std', 'Std', 'Tasks']) {
		assert.ok(isNamespace(text), text);
	}
	for (const text of ['', '1st', 'Acme.Tools', 'Acme::::Tools', 'Acme:Tools', 'Acme::', 'my-app', 'A::'.repeat(16) + 'A', 'Task', 'Acme::Task', 'Acme::_', 'Acme::match', 'Maybe', 'IO::Extra', 'std', 'std::Extra']) {
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
