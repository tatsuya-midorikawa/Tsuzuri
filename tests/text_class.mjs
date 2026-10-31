// D14: a call of `Text.f` is a direct call of an instance method, and that method only calls `String.f` or
// `Utf8String.f` (the Text calls of tests/fixtures/text_class_calls against the direct calls of text_class_direct).
// Always checked, on the LLVM IR that `--emit llvm` writes: every instance method has exactly one call, whose callee is
// the function of the same name; the callers contain no indirect call; the direct program has no instance method.
// With `--objects` it also builds both programs at the optimization levels and prints whether the object code (native)
// and the WebAssembly module are byte-identical. That table is a measurement, not an assertion: LLVM's inlining
// decisions can move a few instructions in a large program.
// It also runs tests/fixtures/text_class_stages at -O0 and -O3 and checks the order of its steps.
// Usage: node tests/text_class.mjs [compiler] [--objects]
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync, rmSync } from 'node:fs';
import { join, resolve } from 'node:path';

const compiler = resolve(process.argv.slice(2).find((argument) => !argument.startsWith('--')) ?? 'target/release/tsuzuri');
const scratch = resolve('target/text-class-tests');
rmSync(scratch, { recursive: true, force: true });
mkdirSync(scratch, { recursive: true });

function build(fixture, output, options) {
  const result = spawnSync(compiler, ['build', resolve('tests/fixtures', fixture), '-o', output, ...options], { encoding: 'utf8', timeout: 180000 });
  assert.ifError(result.error);
  assert.equal(result.status, 0, `${fixture} ${options.join(' ')}\n${result.stdout}\n${result.stderr}`);
  return readFileSync(output);
}

const methods = ['length', 'compare', 'clone', 'find', 'rfind', 'contains', 'starts_with', 'ends_with', 'slice', 'sub', 'join', 'concat', 'split', 'trim', 'trim_start', 'trim_end', 'to_ascii_lower', 'to_ascii_upper', 'repeat', 'replace'];
const ir = build('text_class_calls', join(scratch, 'calls.ll'), ['--emit', 'llvm', '-O0']).toString('utf8');
const bodies = [...ir.matchAll(/^define internal [^\n]*@tz\.fn\.(String|Utf8String)\.\$instance\.\d+\.(\w+)\([^\n]*\{\n([\s\S]*?)^\}/gm)];
assert.equal(bodies.length, 2 * methods.length, 'the instance methods of both types that the program calls');
for (const [, type, method, body] of bodies) {
  const calls = body.split('\n').filter((line) => /\bcall\b/.test(line));
  assert.equal(calls.length, 1, `${type}.${method} makes one call`);
  const callee = method === 'clone' ? (type === 'String' ? '@tz.fn.$builtin.clone_string' : '@tz.fn.$builtin.Utf8String.clone')
    : method === 'compare' ? `@tz.fn.$builtin.${type}.compare` : `@tz.fn.${type}.${method}`;
  assert.ok(calls[0].includes(`${callee}(`), `${type}.${method} calls ${callee}: ${calls[0]}`);
}
for (const type of ['String', 'Utf8String']) {
  assert.deepEqual([...new Set(bodies.filter((body) => body[1] === type).map((body) => body[2]))].sort(), [...methods].sort(), `${type} instance methods`);
}
for (const name of ['score', 'score8']) {
  const body = ir.match(new RegExp(`^define internal [^\\n]*@tz\\.fn\\.Main\\.${name}\\([^\\n]*\\{\\n([\\s\\S]*?)^\\}`, 'm'))[1];
  assert.doesNotMatch(body, /\bcall [^\n]*%[A-Za-z0-9_.]+\(/, `${name} has no indirect call`);
  assert.match(body, /\bcall [^\n]*@tz\.fn\.String\.\$instance\.\d+\.length\(|\bcall [^\n]*@tz\.fn\.Utf8String\.\$instance\.\d+\.length\(/, `${name} calls an instance method directly`);
}
assert.doesNotMatch(ir, /@tz\.dyn\.|%tz\.hkt/, 'Text calls are resolved at compile time');
const directIr = build('text_class_direct', join(scratch, 'direct.ll'), ['--emit', 'llvm', '-O0']).toString('utf8');
assert.doesNotMatch(directIr, /\$instance\.\d+\.length/, 'the direct program calls no instance method');
console.log(`text class: ${bodies.length} instance methods, each one call; no indirect call`);

// An instance may take a method's arguments in stages (tests/fixtures/text_class_stages): its first stage runs before
// the later arguments are evaluated, as for a function (docs/language.md, "関数の宣言と適用"); an implementation that
// names every argument evaluates them all before its body. Debug.trace writes each step to standard error.
for (const level of ['-O0', '-O3']) {
  const result = spawnSync(compiler, ['run', resolve('tests/fixtures/text_class_stages'), level], { encoding: 'utf8', timeout: 180000 });
  assert.ifError(result.error);
  assert.equal(result.status, 0, `text_class_stages ${level}\n${result.stdout}\n${result.stderr}`);
  assert.equal(result.stdout.trim(), '221 343', `text_class_stages ${level}`);
  const steps = result.stderr.split('\n').filter((line) => /^\d+$/.test(line.trim())).map((line) => line.trim());
  assert.deepEqual(steps, ['1', '2', '4', '3'], `text_class_stages ${level}: the order of the steps`);
}
console.log('text class: a staged instance method runs its first stage before the later arguments at -O0 and -O3');

if (process.argv.includes('--objects')) {
  for (const [kind, extension, extra, levels] of [
    ['native object', 'o', ['--emit', 'object'], ['-O1', '-O2', '-O3', '-Os', '-Oz']],
    ['wasm32', 'wasm', ['--target', 'wasm32'], ['-O1', '-O2', '-O3']],
  ]) {
    for (const level of levels) {
      const direct = build('text_class_direct', join(scratch, `direct${level}.${extension}`), [...extra, level]);
      const calls = build('text_class_calls', join(scratch, `calls${level}.${extension}`), [...extra, level]);
      console.log(`${kind} ${level}: ${direct.equals(calls) ? 'identical' : 'differs'} (${direct.length} and ${calls.length} bytes)`);
    }
  }
}
rmSync(scratch, { recursive: true, force: true });
