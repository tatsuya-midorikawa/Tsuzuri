import { spawn } from 'node:child_process';
import { access, stat } from 'node:fs/promises';
import * as path from 'node:path';

export const sourcePattern = '**/*.{tz,tt,tc}';
export const excludedPattern = '**/{node_modules,target,toolchain,.git,.tsuzuri,.vscode-test}/**';

export function supportsDebug(platform: string = process.platform, architecture: string = process.arch): boolean {
	return ['darwin-x64', 'darwin-arm64', 'linux-x64', 'linux-arm64', 'win32-x64'].includes(`${platform}-${architecture}`);
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
export const reservedWords = new Set('fn def rec and export extern private record union type const test bench class instance deriving dyn let task do return yield for in to downto while break continue mut ref deref new as if then elif else match with when true false'.split(' '));
export const libraryModules = new Set('Maybe Result Array List Vec String Utf8String Char Utf8Char Math Int Debug Parallel Simd Map Set HashMap HashSet Seq Test Gpu IO Owned File Dir Path Env Time Random Os Process Format Exception BigInt FixedArray Dyn Arena Rc Arc Regex Unicode Json Cbor Bench Gen Async Atomic Mutex Channel'.split(' '));

/**
 * Whether `tsuzuri new` accepts `text` as a namespace such as `Acme::Tools`: at most 16 identifiers joined by `::`
 * and 255 bytes, none a reserved word, `_`, or `Task`, and the first neither the `std` namespace nor a standard
 * library module.
 */
export function isNamespace(text: string): boolean {
	const segments = text.split('::');
	return text.length <= 255 && segments.length <= 16 && segments[0] !== 'std' && !libraryModules.has(segments[0])
		&& segments.every(segment => /^[A-Za-z_][A-Za-z0-9_]*$/.test(segment)
			&& segment !== '_' && segment !== 'Task' && !reservedWords.has(segment));
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
