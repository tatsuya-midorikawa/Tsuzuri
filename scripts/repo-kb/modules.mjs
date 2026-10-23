import { posix } from 'node:path';
import { parseMarkdown, tables } from './markdown.mjs';
import { expandCell } from './repo.mjs';
import { take } from './text.mjs';

export function parseModuleTables(source, text, ctx) {
    const result = [];
    let table = 0;
    for (const t of tables(parseMarkdown(text))) {
        if (t.header[0] !== 'ファイル') continue;
        table++;
        for (const [i, cells] of t.rows.entries()) {
            const files = [];
            const dirs = [];
            const missing = [];
            let base = '';
            for (const part of cells[0].split(' / ')) {
                let token = /`([^`]+)`/.exec(part)?.[1];
                if (!token) continue;
                if (token.includes('/')) base = token.slice(0, token.lastIndexOf('/') + 1);
                else token = posix.join(base, token);
                for (const file of expandCell(token, ctx)) {
                    if (file.path.endsWith('/')) dirs.push(file.path);
                    else files.push(file);
                    if (!file.exists) {
                        missing.push(file.path);
                        ctx.warnings.push(`table_path_missing ${file.path} (${source} table ${table})`);
                    }
                }
            }
            result.push({ source, table, row: i + 1, files, dirs, missing, text: take(cells.slice(1).join(' / ').replace(/\s+/g, ' '), 600) });
        }
    }
    return result;
}
