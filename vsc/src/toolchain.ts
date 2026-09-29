import { readFile } from 'node:fs/promises';
import * as path from 'node:path';
import * as vscode from 'vscode';
import { exists, runProcess, ProcessOptions } from './core';

export interface Toolchain {
	compiler: string;
	root: string;
	env: NodeJS.ProcessEnv;
}

export async function toolchain(context: vscode.ExtensionContext, resource?: vscode.Uri): Promise<Toolchain> {
	if (!vscode.workspace.isTrusted) {
		throw new Error('Trust this workspace before running Tsuzuri.');
	}
	const settings = vscode.workspace.getConfiguration('tsuzuri', resource);
	const root = settings.get<string>('toolchainPath') || context.asAbsolutePath('toolchain');
	const suffix = process.platform === 'win32' ? '.exe' : '';
	const compiler = settings.get<string>('compilerPath') || path.join(root, 'bin', `tsuzuri${suffix}`);
	if (!path.isAbsolute(root) || !path.isAbsolute(compiler)) {
		throw new Error('Tsuzuri compilerPath and toolchainPath must be absolute paths.');
	}
	if (!await exists(compiler)) {
		throw new Error(`Tsuzuri compiler is missing: ${compiler}. Install the VSIX for ${process.platform}-${process.arch}, or configure a development compiler.`);
	}
	let id = 'development';
	const manifest = path.join(root, 'manifest.json');
	if (await exists(manifest)) {
		const metadata = JSON.parse(await readFile(manifest, 'utf8'));
		if (metadata.platform !== process.platform || metadata.arch !== process.arch) {
			throw new Error(`This toolchain is for ${metadata.platform}-${metadata.arch}; this extension host is ${process.platform}-${process.arch}.`);
		}
		if (typeof metadata.id !== 'string' || !/^[a-f0-9]{64}$/.test(metadata.id)) {
			throw new Error('Invalid Tsuzuri toolchain manifest.');
		}
		id = metadata.id;
	}
	const env = { ...process.env };
	for (const [variable, name] of Object.entries({
		TSUZURI_CLANG: 'tsuzuri-clang', TSUZURI_LLVM_CLANG: 'clang',
		TSUZURI_WASM_LD: 'wasm-ld', TSUZURI_LLVM_LINK: 'llvm-link', TSUZURI_DSYMUTIL: 'dsymutil',
	})) {
		const executable = path.join(root, 'bin', name + suffix);
		if (await exists(executable)) { env[variable] = executable; }
	}
	const zig = path.join(root, 'zig', `zig${suffix}`);
	if (await exists(zig)) { env.TSUZURI_ZIG = zig; }
	env.TSUZURI_CACHE_DIR = path.join(context.globalStorageUri.fsPath, 'cache', id);
	env.ZIG_GLOBAL_CACHE_DIR = path.join(context.globalStorageUri.fsPath, 'zig-cache');
	env.ZIG_LOCAL_CACHE_DIR = path.join(context.globalStorageUri.fsPath, 'zig-local-cache');
	const pathKey = Object.keys(env).find(key => key.toLowerCase() === 'path') ?? 'PATH';
	env[pathKey] = path.join(root, 'bin') + path.delimiter + (env[pathKey] ?? '');
	return { compiler, root, env };
}

export async function runCompiler(context: vscode.ExtensionContext, root: string, args: string[], options: ProcessOptions = {}) {
	const tools = await toolchain(context, vscode.Uri.file(root));
	return runProcess(tools.compiler, args, { ...options, cwd: root, env: tools.env });
}
