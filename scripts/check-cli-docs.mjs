// Drift between the compiler and its documents (G35): the flags, commands and environment variables of
// `--help` against README.md and option.md (`cli`), the diagnostic codes against the code tables (`codes`),
// the version against every `Tsuzuri X.Y.Z` mention (`version`), and the public std functions against
// their module pages (`std`).
//
// usage: node scripts/check-cli-docs.mjs [--only cli|codes|version|std]... [--bin PATH]

import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createContext } from './gen-docs.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const REFERENCE = '_tsuzuri/language-reference/';
const unique = (values) => [...new Set(values)].sort();

// ---- cli ------------------------------------------------------------------------------------------
export function extractCli(help) {
    return {
        flags: unique([...help.matchAll(/(?<![\w-])(--[a-z][a-z0-9-]*)/g)].map((match) => match[1])),
        commands: unique([...help.matchAll(/^ {2,4}tsuzuri (?:\[(build)\]|([a-z][a-z-]*))/gm)].map((match) => match[1] ?? match[2])),
        variables: unique([...help.matchAll(/\bTSUZURI_[A-Z0-9_]+\b/g)].map((match) => match[0])),
    };
}
export const EXEMPT_FLAGS = { '--sysroot': 'a flag of the C compiler named in the TSUZURI_SYSROOT sentence of --help' };
/** The flags and variables that README.md or option.md lacks, and the commands that README.md lacks. */
export function checkCli(cli, pages) {
    const messages = [];
    // A whole token: `--rev` is not documented by `--reverse`, nor `TSUZURI_KG` by `TSUZURI_KG_DIR`.
    const names = (text, name) => new RegExp(`(?<![\\w-])${name}(?![\\w-])`).test(text);
    for (const page of ['README.md', 'option.md']) {
        for (const flag of cli.flags) if (!(flag in EXEMPT_FLAGS) && !names(pages[page], flag)) messages.push(`${page} lacks flag ${flag}`);
        for (const variable of cli.variables) if (!names(pages[page], variable)) messages.push(`${page} lacks variable ${variable}`);
    }
    for (const command of cli.commands) {
        if (!new RegExp(`tsuzuri (?:\\[${command}\\]|${command})(?![A-Za-z0-9_-])`).test(pages['README.md'])) messages.push(`README.md lacks command ${command}`);
    }
    return messages;
}

// ---- codes ----------------------------------------------------------------------------------------
/** The codes in the first cell of each table row of `text`; a range `E1001`–`E1003` is expanded. */
export function documentedCodes(text) {
    const codes = [];
    for (const line of text.split('\n')) {
        const row = /^\| (.+?) \|/.exec(line);
        if (!row) continue;
        const range = /`([EW])([0-9]{4})`\s*[–-]\s*`([EW])([0-9]{4})`/.exec(row[1]);
        if (range) for (let n = Number(range[2]); n <= Number(range[4]); n++) codes.push(`${range[1]}${String(n).padStart(4, '0')}`);
        else codes.push(...[...row[1].matchAll(/`([EW][0-9]{4})`/g)].map((match) => match[1]));
    }
    return unique(codes);
}
/** The codes that Rust sources emit, read from the text (for a compiler without `explain --catalog`). */
export function scanEmittedCodes(sources) {
    const codes = [];
    for (const text of sources) {
        codes.push(...[...text.matchAll(/\(\s*"([EW][0-9]{4})"\s*,/g)].map((match) => match[1]));
        codes.push(...[...text.matchAll(/\\"code\\":\\"([EW][0-9]{4})\\"/g)].map((match) => match[1]));
    }
    return unique(codes);
}
export function checkCodes(emitted, pages) {
    const messages = [];
    for (const [page, text] of Object.entries(pages)) {
        const documented = documentedCodes(text);
        for (const code of emitted) if (!documented.includes(code)) messages.push(`${code} is emitted but missing from ${page}`);
        for (const code of documented) if (!emitted.includes(code)) messages.push(`${code} is in ${page} but no source emits it`);
    }
    return messages;
}
const walk = (directory, extension) => readdirSync(directory, { withFileTypes: true })
    .flatMap((entry) => (entry.isDirectory() ? walk(join(directory, entry.name), extension) : entry.name.endsWith(extension) ? [join(directory, entry.name)] : []));
export function emittedCodes(context) {
    if (context.hasExplain()) return unique(context.catalog().map((line) => JSON.parse(line).code));
    return scanEmittedCodes(walk(resolve(context.root, 'src'), '.rs').map((file) => readFileSync(file, 'utf8')));
}

// ---- version --------------------------------------------------------------------------------------
export const versionMentions = (text) => [...text.matchAll(/\b[Tt]suzuri v?(\d+\.\d+\.\d+)\b/g)].map((match) => match[1]);
export function checkVersion(version, versionOutput, mentions) {
    const messages = mentions.filter((mention) => mention.version !== version).map((mention) => `${mention.file} says ${mention.version}, Cargo.toml says ${version}`);
    if (versionOutput !== `tsuzuri ${version}`) messages.push(`--version prints '${versionOutput}', expected 'tsuzuri ${version}'`);
    return messages;
}

// ---- std ------------------------------------------------------------------------------------------
export const STD_PAGES = { MatrixView: 'built-in-types-and-modules/matrix-view.md', Async: 'async-tasks-and-lazy/async.md' };
export const stdPage = (module) => STD_PAGES[module] ?? `built-in-types-and-modules/${module.toLowerCase()}.md`;
/** The public std functions that their module's reference page does not mention; `readPage` returns null for no page. */
export function checkStd(modules, readPage) {
    const messages = [];
    for (const [module, entries] of Object.entries(modules)) {
        if (entries.length === 0) continue;
        const page = stdPage(module);
        const text = readPage(page);
        if (text === null) { messages.push(`${module} has ${entries.length} public functions but no page in STD_PAGES`); continue; }
        // `List.find_index` does not mention `List.find`: the qualified name must end there.
        for (const { name } of entries) {
            if (!text.includes(`\`${name}\``) && !new RegExp(`${module}\\.${name}(?![A-Za-z0-9_])`).test(text)) messages.push(`${module}.${name} is not mentioned in ${page}`);
        }
    }
    return messages;
}

// ---- running the checks ----------------------------------------------------------------------------
export const CHECKS = ['cli', 'codes', 'version', 'std'];
export function runChecks(context, compiler, names) {
    const read = (path) => readFileSync(resolve(context.root, path), 'utf8');
    const run = (args) => spawnSync(compiler, args, { cwd: context.root, encoding: 'utf8', maxBuffer: 16 * 1024 * 1024 });
    return CHECKS.filter((name) => names.includes(name)).map((name) => {
        if (name === 'cli') {
            const help = run(['--help']);
            if (help.status !== 0) throw new Error(`${compiler} --help failed: ${help.stderr}`);
            const cli = extractCli(help.stdout);
            return { name, summary: `${cli.flags.length} flags, ${cli.commands.length} commands, ${cli.variables.length} variables`,
                messages: checkCli(cli, { 'README.md': read('README.md'), 'option.md': read(`${REFERENCE}compiler/option.md`) }) };
        }
        if (name === 'codes') {
            const emitted = emittedCodes(context);
            return { name, summary: `${emitted.length} codes`,
                messages: checkCodes(emitted, { [`${REFERENCE}compiler/diagnostics.md`]: read(`${REFERENCE}compiler/diagnostics.md`), 'docs/language.md': read('docs/language.md') }) };
        }
        if (name === 'version') {
            const version = /^version = "(\d+\.\d+\.\d+)"/m.exec(read('Cargo.toml'))[1];
            const files = ['README.md', ...readdirSync(resolve(context.root, 'docs')).filter((file) => file.endsWith('.md') && file !== 'benchmarks.md').map((file) => `docs/${file}`),
                ...walk(resolve(context.root, '_tsuzuri'), '.md').map((file) => relative(context.root, file))];
            const mentions = files.flatMap((file) => versionMentions(read(file)).map((found) => ({ file, version: found })));
            return { name, summary: `${version} (${mentions.length} mentions)`, messages: checkVersion(version, run(['--version']).stdout.trim(), mentions) };
        }
        const modules = context.std();
        const readPage = (page) => (existsSync(resolve(context.root, REFERENCE, page)) ? read(`${REFERENCE}${page}`) : null);
        const functions = Object.values(modules).reduce((sum, entries) => sum + entries.length, 0);
        return { name, summary: `${functions} public functions in ${Object.values(modules).filter((entries) => entries.length > 0).length} modules`, messages: checkStd(modules, readPage) };
    });
}

// ---- command line ----------------------------------------------------------------------------------
const USAGE = 'usage: node scripts/check-cli-docs.mjs [--only cli|codes|version|std]... [--bin PATH]';

export function main(argv) {
    const only = [];
    let compiler = process.env.TSUZURI_BIN ?? join(root, 'target/release/tsuzuri');
    for (let index = 0; index < argv.length; index += 1) {
        if (argv[index] === '--only' && index + 1 < argv.length) only.push(argv[++index]);
        else if (argv[index] === '--bin' && index + 1 < argv.length) compiler = resolve(argv[++index]);
        else {
            console.error(`check-cli-docs: unknown option '${argv[index]}'\n${USAGE}`);
            return 2;
        }
    }
    const unknown = only.find((name) => !CHECKS.includes(name));
    if (unknown !== undefined) {
        console.error(`check-cli-docs: unknown check '${unknown}'\n${USAGE}`);
        return 2;
    }
    if (!existsSync(compiler)) {
        console.error(`check-cli-docs: compiler not found: ${compiler}`);
        return 2;
    }
    let failed = 0;
    const results = runChecks(createContext({ root, compiler }), compiler, only.length > 0 ? only : CHECKS);
    for (const result of results) {
        if (result.messages.length === 0) console.log(`ok ${result.name}: ${result.summary}`);
        else {
            failed++;
            for (const message of result.messages) console.error(`FAIL ${result.name}: ${message}`);
        }
    }
    console.log(`check-cli-docs: ${results.length} checks, ${failed} failed`);
    return failed > 0 ? 1 : 0;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) process.exitCode = main(process.argv.slice(2));
