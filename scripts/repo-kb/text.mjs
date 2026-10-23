import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';

export const readText = path => readFileSync(path, 'utf8').replace(/\r\n/g, '\n');
export const cp = s => [...s].length;
export const take = (s, n) => [...s].slice(0, n).join('');
export const sha256 = data => createHash('sha256').update(data).digest('hex');
export const cmp = (a, b) => a < b ? -1 : a > b ? 1 : 0;
export const jsonl = rows => rows.map(row => `${JSON.stringify(row)}\n`).join('');
export function slug(title, seen = new Map()) {
    const base = title.toLowerCase().replace(/[^\p{L}\p{M}\p{N}\s_-]/gu, '').replace(/ /g, '-');
    const n = seen.get(base) ?? 0;
    seen.set(base, n + 1);
    return n ? `${base}-${n}` : base;
}
