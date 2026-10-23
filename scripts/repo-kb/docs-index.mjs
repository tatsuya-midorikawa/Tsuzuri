import { join } from 'node:path';
import { parseMarkdown } from './markdown.mjs';
import { cp, readText, slug } from './text.mjs';

export function indexDocs(ctx) {
    return ctx.files.filter(p => /^_tsuzuri\/language-reference\/.*\.md$/.test(p)
        || /^docs\/[^/]+\.md$/.test(p) || p === 'README.md').map(path => {
        const text = ctx.texts?.get(path) ?? readText(join(ctx.root, path));
        const md = parseMarkdown(text);
        const seen = new Map();
        const headings = md.headings.map(h => ({ ...h, anchor: slug(h.title, seen) }));
        return { path, kind: path.startsWith('_tsuzuri/') ? 'reference' : path === 'README.md' ? 'readme' : 'docs',
            title: md.headings.find(h => h.level === 1)?.title ?? '', chars: cp(text),
            headings: headings.filter(h => h.level === 2 || h.level === 3).slice(0, 200).map(({ level, title, anchor }) => ({ level, title, anchor })) };
    });
}
