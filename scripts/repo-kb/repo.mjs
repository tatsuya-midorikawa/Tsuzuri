import { execFileSync } from 'node:child_process';
import { lstatSync, readdirSync, statSync } from 'node:fs';
import { join, posix } from 'node:path';
import { cmp, readText, sha256 } from './text.mjs';

export function listFiles(root) {
    if (!statSync(root).isDirectory()) throw new Error('root is not a directory');
    try {
        const files = execFileSync('git', ['ls-files', '--cached', '--others', '--exclude-standard', '-z'],
            { cwd: root, encoding: 'utf8', maxBuffer: 32 * 1024 * 1024, stdio: ['ignore', 'pipe', 'pipe'] });
        // A fixture inside another repository must not inherit that repository's index.
        const prefix = execFileSync('git', ['rev-parse', '--show-prefix'], { cwd: root, encoding: 'utf8' }).trim();
        if (!prefix) return files.split('\0').filter(Boolean).filter(p => {
            try { return lstatSync(join(root, p)).isFile(); } catch (e) { if (e.code === 'ENOENT') return false; throw e; }
        }).sort(cmp);
    } catch (e) {
        if (!e.code && !e.status) throw e;
    }
    const result = [];
    const dirs = [''];
    while (dirs.length) {
        const dir = dirs.pop();
        for (const entry of readdirSync(join(root, dir), { withFileTypes: true })) {
            if (['.git', 'target', 'node_modules'].includes(entry.name)) continue;
            const path = posix.join(dir, entry.name);
            if (entry.isDirectory()) dirs.push(path);
            else if (entry.isFile()) result.push(path);
        }
    }
    return result.sort(cmp);
}

export function inputFiles(files) {
    return files.filter(p => /^(?:_features|_perfs)\/.*\.md$/.test(p)
        || /^src\/.*\.rs$/.test(p) || /^tests\/[^/]+\.(?:rs|mjs)$/.test(p)
        || /^_tsuzuri\/language-reference\/.*\.md$/.test(p) || /^docs\/[^/]+\.md$/.test(p)
        || p === 'README.md' || /^scripts\/repo-kb(?:\/[^/]+)?\.mjs$/.test(p));
}
export function inputHash(root, files) {
    const rows = files.map(p => `F ${p}`);
    for (const p of inputFiles(files)) {
        if (statSync(join(root, p)).size > 8 * 1024 * 1024) continue;
        rows.push(`H ${sha256(readText(join(root, p)))} ${p}`);
    }
    return sha256(rows.join('\n'));
}
export function globToRegExp(glob) {
    return new RegExp(`^${glob.split('*').map(s => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).join('[^/]*')}$`);
}
export function expandCell(token, ctx) {
    const file = /^[A-Za-z0-9_./-]+\.(?:rs|mjs|md|toml|json|jsonl|yml|yaml|tz|tt|tc|ll|c|h|sh|py|ts|mts|cs|cpp|txt|lock|spvasm|js)$/;
    if (token.includes('/') && !token.includes('::') && /\{[^{}]+\}/.test(token)) {
        const m = /^(.*?)\{([^{}]+)\}(.*)$/.exec(token);
        return m[2].split(',').flatMap(s => expandCell(m[1] + s + m[3], ctx));
    }
    if (token.includes('*') && token.includes('/') && !/\s/.test(token)) {
        const re = globToRegExp(token);
        const found = ctx.files.filter(p => re.test(p));
        return found.length ? found.map(path => ({ path, exists: true })) : [{ path: token, exists: false }];
    }
    if (/^[A-Za-z0-9_./-]+\/$/.test(token)) return [{ path: token, exists: ctx.files.some(p => p.startsWith(token)) }];
    if ((file.test(token) && token.includes('/')) || ['Cargo.toml', 'Cargo.lock', 'README.md', 'AGENTS.md', 'LICENSE'].includes(token)) {
        return [{ path: token, exists: ctx.fileSet.has(token) }];
    }
    return [];
}
