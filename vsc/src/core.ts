import { spawn } from 'node:child_process';
import { access, lstat, readdir, stat } from 'node:fs/promises';
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

/** The files that make their directory a project root: a manifest, or an entry module without one. */
const markers = new Set(['Tsuzuri.toml', 'Main.tz']);
/**
 * The markers by their names in lower case. A case-insensitive volume (macOS and Windows by default) finds a marker under
 * another case of its name, as the walk up from a file (`exists`) and the compiler do, so the scan asks the volume too.
 */
const foldedMarkers = new Map([...markers].map(marker => [marker.toLowerCase(), marker]));
/** The file that marks a knowledge graph's directory (`src/kg/write.rs`), which module discovery does not read. */
const graphMarker = '.tsuzuri-kg';
/** The limits of module discovery (`src/driver.rs`, `collect_sources`): directories with the root, the depth of a
 * directory below the root, and source files. */
const discovery = { directories: 1024, depth: 16, sources: 4096 };

/** An entry of a directory, as module discovery tells it apart. */
export interface DirectoryEntry {
	name: string;
	kind: 'file' | 'directory' | 'symlink' | 'other';
	/** What a symbolic link points to; undefined when the link is broken. */
	target?: 'file' | 'directory' | 'other';
}

/** The file system that finding a project root reads (`ProjectRoots`); the unit tests replace it. */
export interface RootFileSystem {
	/** The entries of `directory`; rejects with the code ENOENT or ENOTDIR when it is not a directory. */
	list(directory: string): Promise<DirectoryEntry[]>;
	exists(file: string): Promise<boolean>;
	isDirectory(file: string): Promise<boolean>;
	/** Whether `file` is itself a regular file, without following a symbolic link; false when it cannot be inspected. */
	isRegularFile(file: string): Promise<boolean>;
}

const missing = (error: unknown) => ['ENOENT', 'ENOTDIR'].includes((error as NodeJS.ErrnoException).code ?? '');

export const diskFileSystem: RootFileSystem = {
	async list(directory) {
		return Promise.all((await readdir(directory, { withFileTypes: true })).map(async (entry): Promise<DirectoryEntry> => {
			if (!entry.isSymbolicLink()) {
				return { name: entry.name, kind: entry.isDirectory() ? 'directory' : entry.isFile() ? 'file' : 'other' };
			}
			const target = await stat(path.join(directory, entry.name)).catch(() => undefined);
			return { name: entry.name, kind: 'symlink', target: target && (target.isDirectory() ? 'directory' : target.isFile() ? 'file' : 'other') };
		}));
	},
	exists,
	async isDirectory(file) {
		try { return (await stat(file)).isDirectory(); } catch (error) {
			if (missing(error)) { return false; }
			throw error;
		}
	},
	async isRegularFile(file) {
		try { return (await lstat(file)).isFile(); } catch { return false; }
	},
};

/**
 * Whether `directory` is a directory. One that cannot be checked for a reason other than its absence counts as one, so
 * that only a deleted or moved folder loses its project (G40 D4).
 */
export async function directoryExists(directory: string): Promise<boolean> {
	try { return (await stat(directory)).isDirectory(); } catch (error) { return !missing(error); }
}

/**
 * What a directory holds, as module discovery reads it from there: whether it contains a project marker or something
 * that makes the discovery fail, whether it is a knowledge graph's directory, and otherwise its counts.
 */
type Contents = { holds: true; graph: boolean } | { holds: false; graph: boolean; directories: number; sources: number; depth: number };

/**
 * What `ProjectRoots` keeps until `clear`: what each directory that a lookup asked about holds, and the entries of each
 * directory that a scan listed, so that a later scan of a directory below or beside it lists none of them again. Both
 * change only with a creation or deletion below the workspace folder, which clears them (`changed`), except for names
 * that a scan does not read or skips: those that start with `.`, and knowledge graphs under `target/tz/`.
 */
interface RootCache {
	contents: Map<string, Promise<Contents>>;
	listings: Map<string, Promise<DirectoryEntry[]>>;
}

const emptyCache = (): RootCache => ({ contents: new Map(), listings: new Map() });

/** The root of a file (G40 D1), and whether it is a remaining root: one that holds another project below it. */
export interface ProjectRoot {
	root: string;
	remaining: boolean;
}

/**
 * Finds the project roots of files (G40 D1) and keeps what each directory holds and each listed directory's entries
 * until `clear` (`RootCache`), so that each directory is read from the disk once. A change of the files below a
 * workspace folder clears them (`changed`).
 */
export class ProjectRoots {
	private cache = emptyCache();

	constructor(private readonly files: RootFileSystem = diskFileSystem) {}

	clear() { this.cache = emptyCache(); }

	/** Clears the cache when a file or directory created or deleted at `file` below `folder` can change a root. */
	changed(folder: string, file: string, platform: string = process.platform): boolean {
		if (!affectsRoot(folder, file, platform)) { return false; }
		this.clear();
		return true;
	}

	/**
	 * Calls `lookup` again until no change cleared the cache while it ran, and then `use` with its result in the same
	 * task. A re-rooting does not wait for a lookup that has not started a client yet, so a client started from a root
	 * that was read before a change would stay beside the clients of the new roots (G40 D4).
	 */
	async settled<T, R>(lookup: () => Promise<T>, use: (value: T) => R): Promise<Awaited<R>> {
		for (;;) {
			const cache = this.cache;
			const value = await lookup();
			if (cache === this.cache) { return use(value) as Awaited<R>; }
		}
	}

	/**
	 * The root of `file`, or of the directory `file`, in the workspace folder `workspaceRoot`. The workspace folder is a
	 * directory also when it was deleted, so that its root is not the folder above it.
	 */
	async find(file: string, workspaceRoot?: string): Promise<ProjectRoot> {
		return this.findForDirectory(file === workspaceRoot || await this.files.isDirectory(file) ? file : path.dirname(file), workspaceRoot);
	}

	/**
	 * The root of a file in `directory`: the nearest directory with a marker up to the workspace folder; without one,
	 * the highest directory up to the workspace folder that holds no other project, or `directory` itself when it holds
	 * one (a remaining root). Outside every workspace folder, the nearest directory with a marker up to the file system's
	 * root, or else `directory`.
	 */
	async findForDirectory(directory: string, workspaceRoot?: string): Promise<ProjectRoot> {
		const cache = this.cache;
		for (let current = directory; ; current = path.dirname(current)) {
			if (await this.files.exists(path.join(current, 'Tsuzuri.toml')) || await this.files.exists(path.join(current, 'Main.tz'))) {
				return { root: current, remaining: false };
			}
			if (current === workspaceRoot || path.dirname(current) === current) { break; }
		}
		if ((await this.holds(cache, directory)).holds) { return { root: directory, remaining: true }; }
		let root = directory;
		while (workspaceRoot !== undefined && root !== workspaceRoot) {
			const parent = path.dirname(root);
			if (parent === root || !inRoot(workspaceRoot, parent) || (await this.holds(cache, parent)).holds) { break; }
			root = parent;
		}
		return { root, remaining: false };
	}

	private holds(cache: RootCache, directory: string): Promise<Contents> {
		let contents = cache.contents.get(directory);
		if (!contents) {
			contents = this.scan(cache, directory);
			cache.contents.set(directory, contents);
		}
		return contents;
	}

	private list(cache: RootCache, directory: string): Promise<DirectoryEntry[]> {
		let listing = cache.listings.get(directory);
		if (!listing) {
			listing = this.files.list(directory);
			cache.listings.set(directory, listing);
		}
		return listing;
	}

	/**
	 * Reads `root` breadth first by the rules of module discovery (G40 D3): names that start with `.` and knowledge
	 * graphs' directories below the root are not read. It stops at a marker (under any case of its name that the volume
	 * finds as the marker), and at what makes the discovery fail (too many directories or sources, a directory too deep, a
	 * source or directory symbolic link, a source that is not a regular file, a directory that cannot be listed), which it
	 * counts as another project. A subdirectory whose contents are known is not read again, and a directory that an
	 * earlier scan listed is not listed again.
	 */
	private async scan(cache: RootCache, root: string): Promise<Contents> {
		const queue: [string, number][] = [[root, 0]];
		let directories = 0, sources = 0, depth = 0, graph = false;
		for (let index = 0; index < queue.length; index++) {
			const [directory, level] = queue[index];
			let entries: DirectoryEntry[] | undefined;
			try { entries = await this.list(cache, directory); } catch (error) {
				if (missing(error)) { entries = []; }
			}
			// Module discovery skips a directory below the root whose `.tsuzuri-kg` is a regular file, which it inspects
			// without following a link before it lists the directory (`symlink_metadata`): a graph that cannot be listed is
			// skipped too, and a marker in a directory that cannot be searched does not count. A directory whose listing
			// names no marker under any case has none, so the marker is inspected only when the listing names one or failed,
			// and the result is the same as inspecting it first.
			const generated = (!entries || entries.some(entry => entry.name.toLowerCase() === graphMarker))
				&& await this.files.isRegularFile(path.join(directory, graphMarker));
			if (directory === root) { graph = generated; } else if (generated) { continue; }
			if (!entries) { return { holds: true, graph }; }
			directories++;
			depth = Math.max(depth, level);
			if (directories > discovery.directories || level >= discovery.depth) { return { holds: true, graph }; }
			for (const entry of entries) {
				if (entry.name.startsWith('.')) { continue; }
				if (markers.has(entry.name)) { return { holds: true, graph }; }
				const folded = foldedMarkers.get(entry.name.toLowerCase());
				if (folded && await this.files.exists(path.join(directory, folded))) { return { holds: true, graph }; }
				const source = /\.(?:tz|tt|tc)$/.test(entry.name);
				if (entry.kind === 'symlink') {
					if (source || entry.target === undefined || entry.target === 'directory') { return { holds: true, graph }; }
				} else if (entry.kind === 'directory') {
					const child = path.join(directory, entry.name);
					const known = cache.contents.get(child);
					if (!known) {
						queue.push([child, level + 1]);
						continue;
					}
					const contents = await known;
					if (contents.graph) { continue; }
					if (contents.holds) { return { holds: true, graph }; }
					directories += contents.directories;
					sources += contents.sources;
					depth = Math.max(depth, level + 1 + contents.depth);
					if (directories > discovery.directories || depth >= discovery.depth || sources > discovery.sources) { return { holds: true, graph }; }
				} else if (source && (entry.kind !== 'file' || ++sources > discovery.sources)) {
					return { holds: true, graph };
				}
			}
		}
		return { holds: false, graph, directories, sources, depth };
	}
}

/** The root of `file` (`ProjectRoots.find`), read with `roots`, a new cache unless one is given. */
export async function projectRoot(file: string, workspaceRoot?: string, roots: ProjectRoots = new ProjectRoots()): Promise<string> {
	return (await roots.find(file, workspaceRoot)).root;
}

/** Whether `file` lies directly in the directory `root`, the files that a remaining root's client serves (G40 D2). */
export function directlyIn(root: string, file: string, platform: string = process.platform): boolean {
	const paths = platform === 'win32' ? path.win32 : path.posix;
	const relative = paths.relative(root, file);
	return relative !== '' && relative !== '..' && !paths.isAbsolute(relative) && !relative.includes(paths.sep);
}

/**
 * The glob of the sources that the client of `root` serves: every source below it, or for a remaining root only those
 * directly in it, so that no file has two clients (G40 D2). The glob's special characters in `root` match themselves.
 */
export function sourceSelector(root: string, remaining: boolean): string {
	return `${root.replaceAll('\\', '/').replace(/[\[\]{}*?]/g, '[$&]')}/${remaining ? '*.{tz,tt,tc}' : sourcePattern}`;
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
 * start with `.` and the knowledge graph `target/tz/kg/` of the root or of a package under it, which module discovery
 * does not read (it skips a directory with the graph's marker) and the compiler replaces on each check, build, run, and
 * test, also on each test discovery. Module discovery reads every other directory under `target/tz/`, whose deletion is
 * kept (Copilot review of PR #119).
 */
export function affectsRoot(root: string, file: string, platform: string = process.platform): boolean {
	if (!inRoot(root, file, platform)) { return false; }
	const parts = (platform === 'win32' ? path.win32 : path.posix).relative(root, file).split(/[\\/]/);
	return !parts.some((part, index) => part.startsWith('.') || (part === 'target' && parts[index + 1] === 'tz' && parts[index + 2] === 'kg'));
}

/**
 * Whether the language server of `root` hears of a watched-file event of `file` (G39): every event of a source or manifest,
 * which the source watcher reports as it did before, and of the other paths, which only the delete watcher of `**` reports,
 * those that `affectsRoot` keeps (a deleted folder of sources under `target/tz/` outside the knowledge graph among them).
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
export const libraryModules = new Set('Maybe Result Array List Vec String Utf8String Char Utf8Char Math Int Debug Parallel Simd Map Set HashMap HashSet Seq Test Gpu IO Owned File Dir Path Env Time Random Os Process Net Format Exception BigInt FixedArray Dyn Arena Rc Arc Regex Unicode Json Cbor Bench Gen Matrix Atomic Mutex MatrixView Tensor Channel Alloc Text RegexText Domain Monoid Monoids Routine DataChannel SendOnly ReceiveOnly Timer Ticker WaitGroup Once OnceValue RwMutex Cond Semaphore SyncMap Context ErrGroup FnPtr Anchor ZString ZUtf8String ExternAddr Complex Slice Linalg ObjC MainThread'.split(' '));

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
