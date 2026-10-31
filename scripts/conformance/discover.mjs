// Finding the conformance tests under `tests/conformance/<AREA>/` (G33 §3).

import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';

import { AREAS, parseTest } from './directives.mjs';

export const STEM = /^[A-Z]{3}-[0-9]{3}-[a-z0-9]+(-[a-z0-9]+)*$/;
export const MAX_FILE_BYTES = 64 * 1024;
export const MAX_DIRECTORY_FILES = 64;

function compare(a, b) {
    return a < b ? -1 : a > b ? 1 : 0;
}

/** The files below `dir`, relative and with `/`, skipping names that start with `.`. */
function filesBelow(dir, prefix = '') {
    const found = [];
    for (const name of readdirSync(dir).sort(compare)) {
        if (name.startsWith('.')) continue;
        const path = join(dir, name);
        if (statSync(path).isDirectory()) found.push(...filesBelow(path, `${prefix}${name}/`));
        else found.push(`${prefix}${name}`);
    }
    return found;
}

/**
 * The tests of the tree at `root`, sorted by stem, and the errors of their places, names and
 * directives (`{ path, line, name, message }`, `path` relative to `root`).
 */
export function discoverTests(root) {
    const base = join(root, 'tests', 'conformance');
    const tests = [];
    const errors = [];
    if (!existsSync(base)) return { tests, errors };
    const error = (path, line, name, message) => errors.push({ path, line, name, message });
    const stems = new Map();
    for (const area of readdirSync(base).sort(compare)) {
        if (area.startsWith('.')) continue;
        const areaPath = join(base, area);
        const relativeArea = `tests/conformance/${area}`;
        if (!statSync(areaPath).isDirectory() || !AREAS.includes(area)) {
            error(relativeArea, 1, 'unexpected_entry', `'${area}' is not a rule area directory`);
            continue;
        }
        for (const name of readdirSync(areaPath).sort(compare)) {
            if (name.startsWith('.')) continue;
            const path = join(areaPath, name);
            const relativePath = `${relativeArea}/${name}`;
            const directory = statSync(path).isDirectory();
            if (!directory && !name.endsWith('.tz')) {
                error(relativePath, 1, 'unexpected_entry', `'${name}' is neither a .tz test nor a test directory`);
                continue;
            }
            const stem = directory ? name : name.slice(0, -3);
            const mainPath = directory ? join(path, 'Main.tz') : path;
            const relativeMain = directory ? `${relativePath}/Main.tz` : relativePath;
            if (directory && !existsSync(mainPath)) {
                error(relativePath, 1, 'unexpected_entry', `the test directory '${name}' has no Main.tz`);
                continue;
            }
            let bad = false;
            if (!STEM.test(stem) || stem.length > 80) {
                error(relativeMain, 1, 'test_name', `'${stem}' is not a test name (AREA-NNN-words, at most 80 characters)`);
                bad = true;
            } else if (stem.slice(0, 3) !== area) {
                error(relativeMain, 1, 'test_name', `'${stem}' is not in its area directory '${stem.slice(0, 3)}'`);
                bad = true;
            } else if (stems.has(stem)) {
                error(relativeMain, 1, 'test_name', `'${stem}' is also ${stems.get(stem)}`);
                bad = true;
            } else {
                stems.set(stem, relativePath);
            }
            const files = directory ? filesBelow(path) : [name];
            if (directory && files.length > MAX_DIRECTORY_FILES) {
                error(relativeMain, 1, 'test_name', `'${stem}' has more than ${MAX_DIRECTORY_FILES} files`);
                bad = true;
            }
            const large = files.find(file => statSync(directory ? join(path, file) : path).size > MAX_FILE_BYTES);
            if (large) {
                error(relativeMain, 1, 'test_name', `'${large}' is larger than 64 KiB`);
                bad = true;
            }
            const text = readFileSync(mainPath, 'utf8');
            const parsed = parseTest(text);
            for (const failure of parsed.errors) error(relativeMain, failure.line, failure.name, failure.message);
            if (directory && parsed.edition && files.includes('Tsuzuri.toml')) {
                error(relativeMain, 1, 'edition_conflict', `'${stem}' has a Tsuzuri.toml and // edition:`);
                bad = true;
            }
            if (!bad) {
                tests.push({
                    stem,
                    area,
                    form: directory ? 'dir' : 'file',
                    path,
                    mainPath,
                    relativePath,
                    relativeMain,
                    text,
                    parsed,
                });
            }
        }
    }
    tests.sort((a, b) => compare(a.stem, b.stem));
    errors.sort((a, b) => compare(a.path, b.path) || a.line - b.line || compare(a.name, b.name));
    return { tests, errors };
}
