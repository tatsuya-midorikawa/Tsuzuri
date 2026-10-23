import { cp } from './text.mjs';

export function parseMarkdown(text) {
    const lines = text.replace(/\r\n/g, '\n').split('\n');
    const headings = [];
    const visible = [];
    const languages = [];
    let fence = null;
    let language = null;
    const stack = [];
    for (let i = 0; i < lines.length; i++) {
        const m = /^ {0,3}(`{3,}|~{3,})(.*)$/.exec(lines[i]);
        visible.push(!fence && !m);
        languages.push(fence ? language : null);
        if (m) {
            if (!fence) { fence = m[1]; language = m[2].trim().split(/\s+/)[0]; }
            else if (m[1][0] === fence[0] && m[1].length >= fence.length && !m[2].trim()) { fence = null; language = null; }
        }
        if (!visible[i]) continue;
        const h = /^(#{1,6}) (.+)$/.exec(lines[i]);
        if (!h) continue;
        const level = h[1].length;
        while (stack.length && headings[stack.at(-1)].level >= level) {
            headings[stack.pop()].end = i;
        }
        stack.push(headings.length);
        headings.push({ level, title: h[2], line: i + 1, end: lines.length, chars: 0 });
    }
    for (const h of headings) h.chars = cp(lines.slice(h.line - 1, h.end).join('\n'));
    return { lines, headings, visible, languages };
}

export function splitRow(line) {
    const cells = [];
    let value = '';
    let ticks = 0;
    let i = line.trim().startsWith('|') ? line.indexOf('|') + 1 : 0;
    for (; i < line.length; i++) {
        const c = line[i];
        if (c === '\\' && line[i + 1] === '\\') { value += '\\\\'; i++; }
        else if (c === '\\' && line[i + 1] === '|') { value += '|'; i++; }
        else if (c === '`') {
            let n = 1;
            while (line[i + n] === '`') n++;
            if (!ticks) ticks = n;
            else if (ticks === n) ticks = 0;
            value += '`'.repeat(n);
            i += n - 1;
        } else if (c === '|' && !ticks) { cells.push(value.trim()); value = ''; }
        else value += c;
    }
    if (value.trim() || !line.trim().endsWith('|')) cells.push(value.trim());
    return cells;
}

export function tables(md) {
    const result = [];
    for (let i = 0; i + 1 < md.lines.length; i++) {
        if (!md.visible[i] || !md.lines[i].trim().startsWith('|') || !/^\|\s*:?-{3,}/.test(md.lines[i + 1])) continue;
        const rows = [];
        const start = i;
        i += 2;
        while (i < md.lines.length && md.visible[i] && md.lines[i].trim().startsWith('|')) rows.push(splitRow(md.lines[i++]));
        result.push({ line: start + 1, header: splitRow(md.lines[start]), rows });
        i--;
    }
    return result;
}

export function sectionLines(md, heading) {
    return md.lines.slice(heading.line - 1, heading.end).join('\n').trimEnd();
}
