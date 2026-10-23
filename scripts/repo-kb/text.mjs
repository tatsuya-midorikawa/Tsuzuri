import { createHash } from 'node:crypto';
import { readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';

export const MAX_INPUT_BYTES = 8 * 1024 * 1024;
export const readText = path => readFileSync(path, 'utf8').replace(/\r\n/g, '\n');
export const admittedText = text => Buffer.byteLength(text, 'utf8') <= MAX_INPUT_BYTES ? text.replace(/\r\n/g, '\n') : null;
export function inputText(ctx, path) {
    const cached = ctx.texts?.get(path);
    if (cached !== undefined) return admittedText(cached);
    const file = join(ctx.root, path);
    return statSync(file).size <= MAX_INPUT_BYTES ? admittedText(readFileSync(file, 'utf8')) : null;
}
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
