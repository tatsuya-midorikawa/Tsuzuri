import { spawn } from 'node:child_process';
import { access, stat } from 'node:fs/promises';
import * as path from 'node:path';

export const sourcePattern = '**/*.{tz,tt,tc}';
export const excludedPattern = '**/{node_modules,target,toolchain,.git,.tsuzuri,.vscode-test}/**';

const supportedHosts = ['darwin-arm64', 'linux-arm64', 'linux-x64', 'win32-arm64', 'win32-x64'];

export function supportsHost(platform: string = process.platform, architecture: string = process.arch): boolean {
	return supportedHosts.includes(`${platform}-${architecture}`);
}

export function assertSupportedHost(platform: string = process.platform, architecture: string = process.arch): void {
	if (!supportsHost(platform, architecture)) {
		throw new Error(`Unsupported Tsuzuri extension host: ${platform}-${architecture}. Supported hosts: ${supportedHosts.join(', ')}.`);
	}
}

export function supportsDebug(platform: string = process.platform, architecture: string = process.arch): boolean {
	return supportsHost(platform, architecture) && !(platform === 'win32' && architecture === 'arm64');
}

export async function exists(file: string): Promise<boolean> {
	try {
		await access(file);
		return true;
	} catch {
		return false;
	}
}

export async function projectRoot(file: string, workspaceRoot?: string): Promise<string> {
	let directory = path.dirname(file);
	try {
		if ((await stat(file)).isDirectory()) { directory = file; }
	} catch (error) {
		if ((error as NodeJS.ErrnoException).code !== 'ENOENT') { throw error; }
	}
	const fallback = workspaceRoot ?? directory;
	while (true) {
		if (await exists(path.join(directory, 'Tsuzuri.toml')) || await exists(path.join(directory, 'Main.tz'))) {
			return directory;
		}
		if (directory === workspaceRoot || path.dirname(directory) === directory) {
			return fallback;
		}
		directory = path.dirname(directory);
	}
}

/** Whether `file` is `root` or lies under it. Windows compares without case, as its file systems do. */
export function inRoot(root: string, file: string, platform: string = process.platform): boolean {
	const paths = platform === 'win32' ? path.win32 : path.posix;
	const relative = paths.relative(root, file);
	return !paths.isAbsolute(relative) && relative !== '..' && !relative.startsWith(`..${paths.sep}`);
}

/**
 * The path that a Windows verbatim path stands for: `\\?\C:\dir` is `C:\dir`, and `\\?\UNC\server\share\dir` is
 * `\\server\share\dir`. The compiler prints canonical paths, which `fs::canonicalize` makes verbatim on Windows; any other
 * path is returned as it is.
 */
export function plainPath(file: string): string {
	if (file.startsWith('\\\\?\\UNC\\')) { return `\\\\${file.slice(8)}`; }
	if (file.startsWith('\\\\?\\') && /^[A-Za-z]:\\/.test(file.slice(4))) { return file.slice(4); }
	return file;
}

/**
 * `file`, a path that the compiler printed for a project at `root`, spelled under `root` as VS Code names it. The compiler
 * canonicalizes the root (`real`, its real path: symbolic links resolved), so a path under `real` but not under `root` is
 * moved under `root`; any other path is returned as it is.
 */
export function rootSpelling(root: string, real: string, file: string, platform: string = process.platform): string {
	if (inRoot(root, file, platform) || !inRoot(real, file, platform)) { return file; }
	const paths = platform === 'win32' ? path.win32 : path.posix;
	return paths.join(root, paths.relative(real, file));
}

/**
 * Whether deleting `file` can change a project at `root` or in a package under it: a path under it, except names that
 * start with `.` and a knowledge graph under a `target/tz/`, which module discovery does not read and the compiler
 * rewrites on each run.
 */
export function affectsRoot(root: string, file: string, platform: string = process.platform): boolean {
	if (!inRoot(root, file, platform)) { return false; }
	const parts = (platform === 'win32' ? path.win32 : path.posix).relative(root, file).split(/[\\/]/);
	return !parts.some((part, index) => part.startsWith('.') || (part === 'target' && parts[index + 1] === 'tz' && index + 2 < parts.length));
}

/**
 * Whether the language server of `root` hears of a watched-file event of `file` (G39): every event of a source or manifest,
 * which the source watcher reports as it did before, and of the other paths, which only the delete watcher of `**` reports,
 * those that `affectsRoot` keeps. Module discovery reads the sources under a `target/tz/` outside a knowledge graph.
 */
export function notifiesServer(root: string, file: string, platform: string = process.platform): boolean {
	return /\.(?:tz|tt|tc|toml)$/.test(file) || affectsRoot(root, file, platform);
}

/**
 * The zero-based line and UTF-16 character of the compiler's byte `offset` into the UTF-8 file `bytes`, in the text that
 * VS Code shows: without the byte order mark, which the offsets count, and with CRLF, CR, and LF each ending a line.
 */
export function sourcePosition(bytes: Uint8Array, offset: number): { line: number; character: number } {
	const start = bytes[0] === 0xef && bytes[1] === 0xbb && bytes[2] === 0xbf ? 3 : 0;
	let end = Math.min(Math.max(offset, start), bytes.length);
	while (end > start && end < bytes.length && (bytes[end] & 0xc0) === 0x80) { end--; }
	let text = Buffer.from(bytes.buffer, bytes.byteOffset + start, end - start).toString('utf8');
	// An offset at the LF of a CRLF is the end of the line, as VS Code has no position between the two.
	if (text.endsWith('\r') && bytes[end] === 0x0a) { text = text.slice(0, -1); }
	const lines = text.split(/\r\n|\r|\n/);
	return { line: lines.length - 1, character: lines[lines.length - 1].length };
}

/** Whether the language server reports `diagnostic`: one of `server` has its code and starts where it starts (G39 D2). */
export function reportedByServer(diagnostic: { code?: string; line: number; character: number }, server: { code?: string; line: number; character: number }[]): boolean {
	return server.some(item => item.code === diagnostic.code && item.line === diagnostic.line && item.character === diagnostic.character);
}

/** The sources, manifests, and lock file of a Tsuzuri project, as a glob of one path segment. */
export const sourceFiles = '{*.tz,*.tt,*.tc,*.toml,Tsuzuri.lock}';

/** Whether `file` is a Tsuzuri source, manifest, or lock file (`sourceFiles`), which the extension's watchers report. */
export function projectFile(file: string): boolean {
	return /(?:\.(?:tz|tt|tc|toml)|(?:^|[\\/])Tsuzuri\.lock)$/.test(file);
}

export type Action = 'check' | 'build' | 'run' | 'test' | 'debug' | 'wasm';

export function commandArguments(action: Action, root: string, optimization = 3, denyWarnings = false): string[] {
	if (!Number.isInteger(optimization) || optimization < 0 || optimization > 3) {
		throw new Error('Optimization must be between 0 and 3.');
	}
	const args = [action === 'debug' || action === 'wasm' ? 'build' : action, root];
	if (action !== 'check') {
		args.push(`-O${action === 'debug' || action === 'test' ? 0 : optimization}`);
	}
	if (denyWarnings) {
		args.push('--deny-warnings');
	}
	if (action === 'debug') {
		args.push('-g', '--trap-info');
	}
	if (action === 'wasm') {
		args.push('--target', 'wasm32');
	}
	if (action === 'build' || action === 'debug' || action === 'wasm') {
		args.push('-o', outputPath(root, action));
	}
	return args;
}

/** Where the Debug test profile builds the runner of one test (G16 Phase 2). Each debug run has its own
 * `run` name, so concurrent debug runs of a project never overwrite or lock each other's runner. */
export function testRunnerPath(root: string, run: string, platform: string = process.platform): string {
	return path.join(root, '.tsuzuri', 'test', `runner-${run}${platform === 'win32' ? '.exe' : ''}`);
}

/** The runner of a debug run and the debug information that the compiler writes beside it. */
export function testRunnerFiles(runner: string): string[] {
	const files = [runner, `${runner}.dwarf`];
	if (runner.endsWith('.exe')) { files.push(`${runner.slice(0, -'.exe'.length)}.pdb`); }
	return files;
}

/** The compiler arguments that build test `index` of `root` for a debugger without running it. */
export function testDebugArguments(root: string, index: number, run: string, platform: string = process.platform): string[] {
	return ['test', root, '--index', String(index), '-g', '-o', testRunnerPath(root, run, platform), '--json'];
}

export function outputPath(root: string, action: Action): string {
	return path.join(root, '.tsuzuri', action === 'debug' ? 'debug' : 'bin',
		`Main${action === 'wasm' ? '.wasm' : process.platform === 'win32' ? '.exe' : ''}`);
}

export interface LaunchOptions {
	terminal?: string;
	initCommands?: string[];
	preRunCommands?: string[];
}

/**
 * The CodeLLDB launch of the `-g` build `program`. The Tsuzuri LLDB formatters at `formatters` load before
 * the user's init commands, and on macOS the DWARF that the build keeps beside the executable is added.
 */
export function lldbLaunch(program: string, formatters: string, configuration: LaunchOptions & Record<string, unknown> = {}, platform: string = process.platform) {
	// LLDB reads backslashes in a quoted argument as escapes; Windows accepts forward slashes.
	return {
		type: 'lldb', request: 'launch', program, sourceLanguages: ['c'],
		terminal: configuration.terminal ?? 'integrated',
		initCommands: [`command script import ${JSON.stringify(formatters.replaceAll('\\', '/'))}`, ...(configuration.initCommands ?? [])],
		preRunCommands: [...(configuration.preRunCommands ?? []),
			...(platform === 'darwin' ? [`target symbols add ${JSON.stringify(`${program}.dwarf`)}`] : [])],
	};
}

/** A PascalCase namespace for a folder name such as `my-app`, or `App` when it has no usable words. */
export function defaultNamespace(folder: string): string {
	const words = folder.match(/[A-Z]+(?![a-z])|[A-Z]?[a-z0-9]+|[A-Z]/g) ?? [];
	const name = words
		.filter(word => /^[A-Za-z]/.test(word))
		.map(word => word[0].toUpperCase() + word.slice(1).toLowerCase())
		.join('');
	return name || 'App';
}

/** The lexer's reserved words and the reserved standard library module names, as `tsuzuri new` checks them. */
export const reservedWords = new Set('fn def rec and export extern private record union type const test bench class instance deriving dyn let do return yield for in to downto while break continue mut ref deref new as if then elif else match with when true false'.split(' '));
export const libraryModules = new Set('Maybe Result Array List Vec String Utf8String Char Utf8Char Math Int Debug Parallel Simd Map Set HashMap HashSet Seq Test Gpu IO Owned File Dir Path Env Time Random Os Process Net Format Exception BigInt FixedArray Dyn Arena Rc Arc Regex Unicode Json Cbor Bench Gen Matrix Atomic Mutex MatrixView Tensor Channel Alloc Text RegexText Domain Monoid Monoids Routine DataChannel SendOnly ReceiveOnly Timer Ticker WaitGroup Once OnceValue RwMutex Cond Semaphore SyncMap Context ErrGroup'.split(' '));

/**
 * Whether `tsuzuri new` accepts `text` as a namespace such as `Acme::Tools`: at most 16 identifiers joined by `::`
 * and 255 bytes, none a reserved word or `_`, and the first neither the `std` namespace nor a standard library
 * module.
 */
export function isNamespace(text: string): boolean {
	const segments = text.split('::');
	return text.length <= 255 && segments.length <= 16 && segments[0] !== 'std' && !libraryModules.has(segments[0])
		&& segments.every(segment => /^[A-Za-z_][A-Za-z0-9_]*$/.test(segment)
			&& segment !== '_' && !reservedWords.has(segment));
}

/**
 * The standard library module that qualifies the name typed at the end of `prefix`, such as `Maybe` in `Maybe.ma`
 * or `std::Maybe.ma`. After any other namespace or module path the name is the user's: `Sample::Maybe.` and
 * `Shape.Maybe.` name no library module.
 */
export function libraryQualifier(prefix: string): string | undefined {
	return /(?<![\w.]|::)(?:std::)?([A-Z][A-Za-z_0-9]*)\.[A-Za-z_0-9]*$/.exec(prefix)?.[1];
}

/**
 * Whether `prefix` ends in a `::` path such as `Sample::Fe`, which only the language server completes.
 * The `::` that annotates a declaration's type, as in `def f::i`, starts no path.
 */
export function endsInPath(prefix: string): boolean {
	return /\w::\w*$/.test(prefix) && !/(?<!\w)(?:def|rec|and)\s+\w+::\w*$/.test(prefix);
}

export interface ProcessResult {
	code: number;
	stdout: string;
	stderr: string;
}

export interface ProcessOptions {
	cwd?: string;
	env?: NodeJS.ProcessEnv;
	signal?: AbortSignal;
	onOutput?: (text: string) => void;
	timeout?: number;
}

export function runProcess(command: string, args: string[], options: ProcessOptions = {}): Promise<ProcessResult> {
	return new Promise((resolve, reject) => {
		if (options.signal?.aborted) {
			reject(new Error('Operation cancelled.'));
			return;
		}
		const child = spawn(command, args, {
			cwd: options.cwd, env: options.env, windowsHide: true,
			detached: process.platform !== 'win32', stdio: ['ignore', 'pipe', 'pipe'],
		});
		let stdout = '';
		let stderr = '';
		let size = 0;
		let failure: Error | undefined;
		let killTimer: NodeJS.Timeout | undefined;
		const stop = (error: Error) => {
			if (failure) {
				return;
			}
			failure = error;
			if (!child.pid) {
				return;
			}
			if (process.platform === 'win32') {
				const taskkill = path.join(process.env.SystemRoot ?? 'C:\\Windows', 'System32', 'taskkill.exe');
				const killer = spawn(taskkill, ['/pid', String(child.pid), '/T', '/F'], { windowsHide: true, stdio: 'ignore' });
				killer.on('error', () => child.kill());
			} else {
				const kill = (signal: NodeJS.Signals) => {
					try { process.kill(-child.pid!, signal); } catch { return; }
				};
				kill('SIGTERM');
				killTimer = setTimeout(() => kill('SIGKILL'), 1000);
				killTimer.unref();
			}
		};
		const cancel = () => stop(new Error('Operation cancelled.'));
		options.signal?.addEventListener('abort', cancel, { once: true });
		const timer = options.timeout ? setTimeout(() => stop(new Error('Compiler operation timed out.')), options.timeout) : undefined;
		for (const [stream, name] of [[child.stdout, 'stdout'], [child.stderr, 'stderr']] as const) {
			stream.setEncoding('utf8');
			stream.on('data', (text: string) => {
				size += Buffer.byteLength(text);
				if (size > 32 * 1024 * 1024) {
					stop(new Error('Compiler output exceeded 32 MiB.'));
					return;
				}
				if (name === 'stdout') { stdout += text; } else { stderr += text; }
				options.onOutput?.(text);
			});
		}
		child.on('error', error => { failure = error; });
		child.on('close', code => {
			clearTimeout(timer);
			clearTimeout(killTimer);
			options.signal?.removeEventListener('abort', cancel);
			if (failure) { reject(failure); } else { resolve({ code: code ?? 1, stdout, stderr }); }
		});
		if (options.signal?.aborted) { cancel(); }
	});
}

export function jsonLines(text: string): Record<string, unknown>[] {
	return text.split(/\r?\n/).filter(line => line.trim()).map(line => {
		const value: unknown = JSON.parse(line);
		if (typeof value !== 'object' || value === null || Array.isArray(value)) {
			throw new Error('Invalid compiler JSON response.');
		}
		return value as Record<string, unknown>;
	});
}
