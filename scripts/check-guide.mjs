// Checks the guide for AI agents (G21): every ```tsuzuri example of guide/*.md and guide/*.tmpl
// type-checks, runs with the `run=` output at -O0 and -O3, and passes its tests, in every edition
// it is shown in (an example inside `<!--if edition=YYYY-->` only in that edition, any other in
// every supported edition); the runtime claims of the card hold; and every chapter is within the
// limits of D-56 in every edition.
//
// usage: node scripts/check-guide.mjs
// TSUZURI_BIN is the compiler (default target/release/tsuzuri); TSUZURI_GUIDE_EXAMPLES is the guide
// directory to check (default guide/), which only this script reads.
import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, extname, join, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const compiler = resolve(process.env.TSUZURI_BIN ?? join(root, 'target/release/tsuzuri'));
const guide = resolve(process.env.TSUZURI_GUIDE_EXAMPLES ?? join(root, 'guide'));
const temporary = mkdtempSync(join(tmpdir(), 'tsuzuri-guide-'));
const EDITIONS = ['2026', '2027'];
const LIMITS = { card: 6000, language: 8500, ownership: 4500, pitfalls: 4000, workflow: 2990 };
let examples = 0;
let executions = 0;
let tests = 0;

/// The parts of `source` with the edition each is limited to (`null` for every edition).
function editionParts(source, label) {
  const parts = [];
  let edition = null;
  let text = '';
  for (const line of source.split(/(?<=\n)/)) {
    const open = line.match(/^<!--if edition=(\d+)-->\n?$/);
    if (open || /^<!--endif-->\n?$/.test(line)) {
      assert.ok(open ? edition === null : edition !== null, `${label}: unbalanced edition block`);
      parts.push({ edition, text });
      edition = open ? open[1] : null;
      text = '';
    } else text += line;
  }
  assert.equal(edition, null, `${label}: an edition block is not closed`);
  parts.push({ edition, text });
  return parts;
}

function compile(args, context) {
  try {
    return execFileSync(compiler, args, { encoding: 'utf8', maxBuffer: 4 * 1024 * 1024 });
  } catch (error) {
    throw new Error(`${context}: ${error.stderr ?? error.message}`, { cause: error });
  }
}

/// Writes `files` to a fresh project of `edition`.
function project(files, edition) {
  const directory = mkdtempSync(join(temporary, 'example-'));
  writeFileSync(join(directory, 'Tsuzuri.toml'), `[package]\nname = "guide-example"\nversion = "0.1.0"\nedition = "${edition}"\n`);
  for (const [file, content] of files) {
    const path = join(directory, file);
    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(path, content);
  }
  return directory;
}

try {
  const files = readdirSync(guide).filter(name => name.endsWith('.md') || name.endsWith('.tmpl')).sort();
  assert.ok(files.length > 0, `no guide files in ${guide}`);
  for (const name of files) {
    const label = `guide/${name}`;
    // A Windows checkout with core.autocrlf has CRLF; the fences and edition markers are matched per line.
    const source = readFileSync(join(guide, name), 'utf8').replaceAll('\r\n', '\n');
    assert.ok(source.endsWith('\n'), `${label}: missing final newline`);
    assert.ok(/^[\x00-\x7f]*$/.test(source), `${label}: the guide is ASCII only`);
    assert.ok(!/^[ \t]+```tsuzuri/m.test(source), `${label}: an indented tsuzuri code fence is not checked; start it at column 1`);
    const projects = new Map();
    let block = 0;
    for (const part of editionParts(source, label)) {
      // The fence rules below are copied from scripts/check-docs.mjs, which exports nothing.
      for (const match of part.text.matchAll(/^```tsuzuri(?: ([^\n]+))?\n([\s\S]*?)^```\s*$/gm)) {
        block++;
        const options = Object.fromEntries((match[1] ?? '').split(/\s+/).filter(Boolean).map(option => {
          const separator = option.indexOf('=');
          assert.ok(separator > 0, `${label}: invalid example option ${option}`);
          return [option.slice(0, separator), option.slice(separator + 1)];
        }));
        assert.ok(Object.keys(options).every(key => ['project', 'file', 'run'].includes(key)), `${label}: unknown example option`);
        // A project is per edition block, as in tests/guide.rs: the same name in two blocks is two projects.
        const name = options.project ?? `block-${block}`;
        const key = `${name} ${part.edition ?? 'every edition'}`;
        const entry = projects.get(key) ?? { name, files: new Map(), expected: undefined, edition: part.edition };
        const file = options.file ?? 'Main.tz';
        assert.ok(!file.includes('..') && !file.startsWith(sep) && ['.tz', '.tt', '.tc'].includes(extname(file)), `${label}: invalid example file`);
        assert.ok(!entry.files.has(file), `${label}: duplicate example file ${file}`);
        entry.files.set(file, match[2]);
        if (options.run !== undefined) entry.expected = decodeURIComponent(options.run);
        projects.set(key, entry);
      }
    }
    for (const entry of projects.values()) {
      for (const edition of entry.edition ? [entry.edition] : EDITIONS) {
        const context = `${label} (${entry.name}, edition ${edition})`;
        const directory = project(entry.files, edition);
        compile(['check', directory], context);
        examples++;
        if (entry.expected !== undefined) {
          for (const optimization of ['-O0', '-O3']) {
            assert.equal(compile(['run', directory, optimization], context).trim(), entry.expected, `${context} ${optimization}`);
            executions++;
          }
        }
        if ([...entry.files.values()].some(content => /^test\s+"/m.test(content))) {
          compile(['test', directory], context);
          tests++;
        }
      }
    }
  }
  // The card says that `for i in 0 .. xs.length` compiles, then traps: `..` includes the end.
  for (const edition of EDITIONS) {
    const directory = project(new Map([['Main.tz', 'def main :: unit -> i32 = \\() ->\n    let xs = [1, 2, 3]\n    let mut sum = 0\n    for i in 0 .. xs.length do\n        sum = sum + xs[i]\n    sum\n']]), edition);
    compile(['check', directory], `the trap claim (edition ${edition})`);
    const run = spawnSync(compiler, ['run', directory], { encoding: 'utf8' });
    assert.equal(run.status, 1, `the trap claim (edition ${edition}): ${run.stderr}`);
    assert.match(run.stderr, /trap/, `the trap claim (edition ${edition})`);
    executions++;
  }
  // The limits of D-56, for the guide that the compiler prints.
  for (const edition of EDITIONS) {
    const size = args => [...compile(['guide', ...args, '--edition', edition], 'guide')].length;
    for (const [topic, limit] of Object.entries(LIMITS)) {
      const chars = size(['--topic', topic]);
      assert.ok(chars <= limit, `guide/${topic}.md has ${chars} characters in edition ${edition}; the limit is ${limit}`);
    }
    const level1 = size(['--level', '1']);
    assert.ok(level1 <= 20000, `level 1 has ${level1} characters in edition ${edition}`);
  }
  console.log(`Guide: ${examples} checked examples, ${executions} native runs (O0/O3), ${tests} test projects.`);
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
