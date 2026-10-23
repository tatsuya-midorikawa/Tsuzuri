import { dirname, posix, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { collect } from './repo-kb/build.mjs';
import { parseMarkdown } from './repo-kb/markdown.mjs';
import { scanRust } from './repo-kb/rust.mjs';
import { listFiles } from './repo-kb/repo.mjs';
import { inputText, slug } from './repo-kb/text.mjs';

export const SECTION_KEYWORDS = {
    '1': '絶対に守る規則', '3.1': 'テスト選択', '6.7': '診断コード', '6.9': '走査箇所',
    '8': 'ドキュメント更新規約', '11.1': '落とし穴', '12': '構文の早見表', '13.1': '止めて報告する条件',
};
export const PLANNED = ['guide/'];
const escaped = s => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
export function checkGuideLite(ctx, kb, file = '_features/GUIDE-LITE.md') {
    const files = ctx.files ?? listFiles(ctx.root);
    const set = new Set(files);
    const read = path => inputText(ctx, path);
    const liteText = read(file);
    const guideText = read('_features/GUIDE.md');
    if (liteText === null || guideText === null) throw new Error('GUIDE input exceeds the 8 MiB file limit');
    const md = parseMarkdown(liteText);
    const guide = parseMarkdown(guideText);
    const seenSlugs = new Map();
    const anchors = new Set(guide.headings.map(h => slug(h.title, seenSlugs)));
    const rust = files.filter(p => /^(?:src\/.*|tests\/[^/]+)\.rs$/.test(p)).map(read).filter(text => text !== null).map(text => scanRust(text).code).join('\n');
    const js = files.filter(p => /^tests\/[^/]+\.mjs$/.test(p)).map(read).filter(text => text !== null).join('\n');
    const failures = [];
    const checked = new Set();
    function check(kind, token, line, reason) {
        const key = `${kind}:${token}:${line}`;
        if (checked.has(key)) return;
        checked.add(key);
        if (reason) failures.push(`FAIL ${kind} ${token} — ${reason} (line ${line})`);
    }
    for (const planned of PLANNED) {
        if (files.some(p => p.startsWith(planned))) check('directory', planned, 1, 'now exists; remove it from PLANNED');
    }
    function pathToken(token, line, kind = 'file') {
        if (/^(?:target\/|\/tmp\/)/.test(token) || /[<>$=*{}]/.test(token)) return;
        if (/^[A-Za-z0-9_.][A-Za-z0-9_./-]*\/$/.test(token)) {
            check('directory', token, line, !files.some(p => p.startsWith(token)) && !PLANNED.includes(token) ? 'directory does not exist' : null);
        } else if (/^[A-Za-z0-9_.][A-Za-z0-9_./-]*\.(?:rs|mjs|md|sh|toml|yml|yaml|py|ll|c|json|jsonl|tz|tt|tc|lock)$/.test(token)) {
            if (token.includes('/')) check(kind, token, line, set.has(token) ? null : 'file does not exist');
            else if (/\.(?:rs|mjs|sh|ll)$/.test(token)) check('file', token, line, files.some(p => posix.basename(p) === token) ? null : 'no file with this name');
        }
    }
    function command(text, line) {
        for (const token of text.split(' #')[0].split(/\s+/)) {
            const value = token.replace(/^["']|["';]$/g, '');
            if (value.includes('/') && !value.startsWith('-')) pathToken(value, line, 'command');
        }
    }
    for (const [i, text] of md.lines.entries()) {
        const line = i + 1;
        if (!md.visible[i]) { if (md.languages[i] === 'sh') command(text, line); continue; }
        for (const m of text.matchAll(/`([^`]+)`/g)) {
            const token = m[1];
            pathToken(token.includes('<') && token.endsWith('/') ? token.split('<')[0] : token, line);
            if (/^(?:cargo|node|npx|sh|python3|git) /.test(token)) command(token, line);
            if (/^[EW][0-9]{4}$/.test(token)) {
                const d = kb.diagnostics.find(d => d.code === token);
                check('diagnostic', token, line, d && (d.site_count || d.meaning) ? null : 'not emitted in src and not in the language-reference table');
                continue;
            }
            const symbol = /^([A-Z][A-Za-z0-9_]*)::([A-Za-z_][A-Za-z0-9_]*)$/.exec(token);
            if (symbol) {
                const type = new RegExp(`\\b(?:struct|enum|trait|type|union)\\s+${escaped(symbol[1])}\\b|\\bimpl\\b[^{};]*\\b${escaped(symbol[1])}\\b`).test(rust);
                const item = new RegExp(`\\b${escaped(symbol[2])}\\b`).test(rust);
                check('symbol', token, line, !type ? `type ${symbol[1]} not found` : !item ? `item ${symbol[2]} not found` : null);
            } else if (/^[A-Z][A-Z0-9_]{3,}$/.test(token)) {
                check('symbol', token, line, new RegExp(`\\b(?:const|static)\\s+${escaped(token)}\\b`).test(rust) ? null : 'no const/static with this name');
            } else if (/^[a-z][a-z0-9]*(?:_[a-z0-9]+){2,}$/.test(token)) {
                check('symbol', token, line, new RegExp(`\\bfn\\s+${escaped(token)}\\b`).test(rust) || js.includes(token) ? null : 'no fn or mjs mention');
            }
        }
        for (const m of text.matchAll(/D-[0-9]{2,3}(?![0-9])/g)) check('decision', m[0], line, kb.decisions.some(d => d.id === m[0]) ? null : 'no such decision in GUIDE.md');
        for (const m of text.matchAll(/§\s*(\d+(?:\.\d+)?)/g)) {
            const n = m[1];
            const h = guide.headings.find(h => h.title.startsWith(`${n}${n.includes('.') ? ' ' : '. '}`));
            const keyword = SECTION_KEYWORDS[n];
            check('section', n, line, !keyword ? 'no keyword registered in SECTION_KEYWORDS'
                : !h ? 'no such heading in GUIDE.md' : !h.title.includes(keyword) ? `heading '${h.title}' does not contain '${keyword}'` : null);
        }
        for (const m of text.matchAll(/\[[^\]\n]+\]\(([^\s)]+)\)/g)) {
            const url = m[1];
            if (/^[a-z][a-z0-9+.-]*:/i.test(url)) continue;
            const [path, fragment] = url.split('#');
            const target = path ? posix.normalize(posix.join(posix.dirname(file), decodeURIComponent(path))) : file;
            const reason = !set.has(target) ? 'target file does not exist'
                : target === '_features/GUIDE.md' && fragment && !anchors.has(decodeURIComponent(fragment)) ? 'no heading with this slug in GUIDE.md' : null;
            check('link', url, line, reason);
        }
        for (const m of text.matchAll(/(?<![A-Za-z0-9-])([A-Z]{1,2}[0-9]{2})(?![A-Za-z0-9])/g)) {
            check('ticket', m[1], line, kb.tickets.some(t => t.id === m[1]) ? null : 'no such ticket');
        }
    }
    return { checked: checked.size, failures };
}
export function main(argv) {
    let root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
    let file = '_features/GUIDE-LITE.md';
    try {
        for (let i = 0; i < argv.length; i++) {
            const a = argv[i];
            const value = argv[++i];
            if (!['--root', '--file'].includes(a) || !value || value.startsWith('--')) {
                console.error('check-guide-lite: expected --root DIR or --file PATH');
                return 2;
            }
            if (a === '--root') root = resolve(value);
            else file = value;
        }
        const kb = collect(root);
        const result = checkGuideLite({ root }, kb, file);
        for (const failure of result.failures) console.log(failure);
        console.log(`check-guide-lite: ${result.checked} references checked, ${result.failures.length} failed`);
        return result.failures.length ? 1 : 0;
    } catch (error) {
        console.error(`check-guide-lite: ${error.code ? `cannot read repository (${error.code})` : error.message}`);
        return 1;
    }
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) process.exitCode = main(process.argv.slice(2));
