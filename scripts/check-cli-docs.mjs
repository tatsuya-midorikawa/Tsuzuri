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
/** The flags and variables that the given pages lack, and the commands that README.md lacks. */
export function checkCli(cli, pages) {
    const messages = [];
    // A whole token: `--rev` is not documented by `--reverse`, nor `TSUZURI_KG` by `TSUZURI_KG_DIR`.
    const names = (text, name) => new RegExp(`(?<![\\w-])${name}(?![\\w-])`).test(text);
    for (const [page, text] of Object.entries(pages)) {
        for (const flag of cli.flags) if (!(flag in EXEMPT_FLAGS) && !names(text, flag)) messages.push(`${page} lacks flag ${flag}`);
        for (const variable of cli.variables) if (!names(text, variable)) messages.push(`${page} lacks variable ${variable}`);
    }
    if ('README.md' in pages) {
        for (const command of cli.commands) {
            if (!new RegExp(`tsuzuri (?:\\[${command}\\]|${command})(?![A-Za-z0-9_-])`).test(pages['README.md'])) messages.push(`README.md lacks command ${command}`);
        }
    }
    return messages;
}

// ---- codes ----------------------------------------------------------------------------------------
/** The codes in the first cell of each non-generated table row of `text`; a range `E1001`–`E1003` is expanded. */
export function documentedCodes(text) {
    const codes = [];
    let generated = false;
    for (const line of text.split('\n')) {
        if (/^<!-- generated:[^ ]+ -->$/.test(line.trim())) { generated = true; continue; }
        if (/^<!-- \/generated:[^ ]+ -->$/.test(line.trim())) { generated = false; continue; }
        if (generated) continue;
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
export const STD_PAGES = {
    Alloc: 'stdlib/testing/alloc.md',
    Anchor: 'stdlib/memory/anchor.md',
    Arc: 'stdlib/memory/rc.md',
    Arena: 'stdlib/memory/arena.md',
    Array: 'stdlib/collections/array.md',
    Atomic: 'stdlib/sync/atomic.md',
    Bench: 'stdlib/testing/bench.md',
    BigInt: 'stdlib/numeric/bigint.md',
    Cbor: 'stdlib/encoding/cbor.md',
    Channel: 'stdlib/sync/channel.md',
    Char: 'stdlib/text/char.md',
    Complex: 'stdlib/numeric/complex.md',
    Cond: 'stdlib/sync/cond.md',
    Context: 'stdlib/sync/context.md',
    DataChannel: 'stdlib/sync/datachannel.md',
    Debug: 'stdlib/core/debug.md',
    Dir: 'stdlib/sys/dir.md',
    Domain: 'stdlib/core/domain.md',
    Env: 'stdlib/sys/env.md',
    ErrGroup: 'stdlib/sync/errgroup.md',
    Exception: 'stdlib/core/exception.md',
    File: 'stdlib/sys/file.md',
    FnPtr: 'stdlib/memory/fnptr.md',
    Format: 'stdlib/text/format.md',
    Gen: 'stdlib/testing/gen.md',
    Gpu: 'stdlib/numeric/gpu.md',
    HashMap: 'stdlib/collections/hashmap.md',
    HashSet: 'stdlib/collections/hashset.md',
    IO: 'stdlib/sys/io.md',
    Int: 'stdlib/numeric/int.md',
    Json: 'stdlib/encoding/json.md',
    Linalg: 'stdlib/numeric/linalg.md',
    List: 'stdlib/collections/list.md',
    Map: 'stdlib/collections/map.md',
    Math: 'stdlib/numeric/math.md',
    Matrix: 'stdlib/numeric/matrix.md',
    MatrixView: 'stdlib/numeric/matrix-view.md',
    Maybe: 'stdlib/core/maybe.md',
    Monoid: 'stdlib/core/monoid.md',
    Monoids: 'stdlib/core/monoid.md',
    Mutex: 'stdlib/sync/mutex.md',
    Net: 'stdlib/sys/net.md',
    Once: 'stdlib/sync/once.md',
    OnceValue: 'stdlib/sync/once.md',
    Os: 'stdlib/sys/os.md',
    Owned: 'stdlib/memory/owned.md',
    Parallel: 'stdlib/sync/parallel.md',
    Path: 'stdlib/sys/path.md',
    Process: 'stdlib/sys/process.md',
    Random: 'stdlib/sys/random.md',
    RandomTensor: 'stdlib/numeric/random-tensor.md',
    Rc: 'stdlib/memory/rc.md',
    ReceiveOnly: 'stdlib/sync/channel.md',
    Regex: 'stdlib/text/regex.md',
    RegexText: 'stdlib/text/regex.md',
    Result: 'stdlib/core/result.md',
    Routine: 'stdlib/sync/routine.md',
    RwMutex: 'stdlib/sync/rwmutex.md',
    Semaphore: 'stdlib/sync/semaphore.md',
    SendOnly: 'stdlib/sync/channel.md',
    Seq: 'stdlib/collections/seq.md',
    Set: 'stdlib/collections/set.md',
    Simd: 'stdlib/numeric/simd.md',
    Slice: 'stdlib/numeric/slice.md',
    String: 'stdlib/text/string.md',
    SyncMap: 'stdlib/sync/syncmap.md',
    Tensor: 'stdlib/numeric/tensor.md',
    Test: 'stdlib/testing/test.md',
    Text: 'stdlib/text/text.md',
    Ticker: 'stdlib/sync/timer.md',
    Time: 'stdlib/sys/time.md',
    Timer: 'stdlib/sync/timer.md',
    Unicode: 'stdlib/text/unicode.md',
    Utf8Char: 'stdlib/text/char.md',
    Utf8String: 'stdlib/text/utf8string.md',
    Vec: 'stdlib/collections/vec.md',
    WaitGroup: 'stdlib/sync/waitgroup.md',
    ZString: 'stdlib/text/zstring.md',
    ZUtf8String: 'stdlib/text/zutf8string.md',
};
export const stdPage = (module) => STD_PAGES[module] ?? `stdlib/${module.toLowerCase()}.md`;
/** The public std functions that their module's reference page does not mention; `readPage` returns null for no page. */
export function checkStd(modules, readPage) {
    const messages = [];
    for (const [module, entries] of Object.entries(modules)) {
        if (entries.length === 0) continue;
        const page = stdPage(module);
        const text = readPage(page);
        if (text === null) { messages.push(`${module} has ${entries.length} public functions but no page in STD_PAGES`); continue; }
        // `List.find_index` does not mention `List.find`, nor `HashMap.get` `Map.get`: the qualified name stands alone.
        for (const { name } of entries) {
            if (!text.includes(`\`${name}\``) && !new RegExp(`(?<![A-Za-z0-9_])${module}\\.${name}(?![A-Za-z0-9_])`).test(text)) messages.push(`${module}.${name} is not mentioned in ${page}`);
        }
    }
    return messages;
}

// ---- running the checks ----------------------------------------------------------------------------
export const CHECKS = ['cli', 'codes', 'version', 'std'];
export function runChecks(context, compiler, names, referenceRoot = null) {
    const read = (path) => readFileSync(resolve(context.root, path), 'utf8');
    const run = (args) => spawnSync(compiler, args, { cwd: context.root, encoding: 'utf8', maxBuffer: 16 * 1024 * 1024 });
    const refDocs = referenceRoot ? resolve(referenceRoot, 'docs') : null;
    return CHECKS.filter((name) => names.includes(name)).map((name) => {
        if (name === 'cli') {
            const help = run(['--help']);
            if (help.status !== 0) throw new Error(`${compiler} --help failed: ${help.stderr}`);
            const cli = extractCli(help.stdout);
            const pages = { 'README.md': read('README.md') };
            if (refDocs && existsSync(join(refDocs, 'compiler/options.md'))) {
                pages['options.md'] = readFileSync(join(refDocs, 'compiler/options.md'), 'utf8');
            }
            return { name, summary: `${cli.flags.length} flags, ${cli.commands.length} commands, ${cli.variables.length} variables`,
                messages: checkCli(cli, pages) };
        }
        if (name === 'codes') {
            const emitted = emittedCodes(context);
            const pages = { 'docs/language.md': read('docs/language.md') };
            if (refDocs && existsSync(join(refDocs, 'compiler/diagnostics.md'))) {
                pages['Tsuzuri-Reference/docs/compiler/diagnostics.md'] = readFileSync(join(refDocs, 'compiler/diagnostics.md'), 'utf8');
            }
            return { name, summary: `${emitted.length} codes`,
                messages: checkCodes(emitted, pages) };
        }
        if (name === 'version') {
            const version = /^version = "(\d+\.\d+\.\d+)"/m.exec(read('Cargo.toml'))[1];
            const files = ['README.md', ...readdirSync(resolve(context.root, 'docs')).filter((file) => file.endsWith('.md') && file !== 'benchmarks.md').map((file) => `docs/${file}`)];
            const mentions = files.flatMap((file) => versionMentions(read(file)).map((found) => ({ file, version: found })));
            if (refDocs && existsSync(refDocs)) {
                for (const file of walk(refDocs, '.md')) {
                    for (const found of versionMentions(readFileSync(file, 'utf8'))) {
                        mentions.push({ file: `Tsuzuri-Reference/${relative(referenceRoot, file)}`, version: found });
                    }
                }
            }
            return { name, summary: `${version} (${mentions.length} mentions)`, messages: checkVersion(version, run(['--version']).stdout.trim(), mentions) };
        }
        const modules = context.std();
        const functions = Object.values(modules).reduce((sum, entries) => sum + entries.length, 0);
        const moduleCount = Object.values(modules).filter((entries) => entries.length > 0).length;
        if (!refDocs || !existsSync(refDocs)) {
            return { name, summary: `${functions} public functions in ${moduleCount} modules (reference pages live in Tsuzuri-Reference)`, messages: [] };
        }
        const readPage = (page) => (existsSync(join(refDocs, page)) ? readFileSync(join(refDocs, page), 'utf8') : null);
        return { name, summary: `${functions} public functions in ${moduleCount} modules`, messages: checkStd(modules, readPage) };
    });
}

// ---- command line ----------------------------------------------------------------------------------
const USAGE = 'usage: node scripts/check-cli-docs.mjs [--only cli|codes|version|std]... [--bin PATH] [--reference-dir PATH]';

export function main(argv) {
    const only = [];
    let compiler = process.env.TSUZURI_BIN ?? join(root, 'target/release/tsuzuri');
    let referenceRoot = process.env.TSUZURI_REFERENCE_DIR ? resolve(process.env.TSUZURI_REFERENCE_DIR) : null;
    for (let index = 0; index < argv.length; index += 1) {
        if (argv[index] === '--only' && index + 1 < argv.length) only.push(argv[++index]);
        else if (argv[index] === '--bin' && index + 1 < argv.length) compiler = resolve(argv[++index]);
        else if (argv[index] === '--reference-dir' && index + 1 < argv.length) referenceRoot = resolve(argv[++index]);
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
    const results = runChecks(createContext({ root, compiler }), compiler, only.length > 0 ? only : CHECKS, referenceRoot);
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
