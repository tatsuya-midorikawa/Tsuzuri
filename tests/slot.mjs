// scripts/slot.mjs: never more commands at once than there are slots, a slot of a dead owner is taken over, exit codes pass through.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { appendFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = mkdtempSync(join(tmpdir(), 'tsuzuri-slot-test-'));
const script = fileURLToPath(new URL('../scripts/slot.mjs', import.meta.url));
const env = { ...process.env, TSUZURI_SLOTS: '2', TSUZURI_SLOT_DIR: join(root, 'slots') };
const log = join(root, 'log');
const slow = `const fs = require('node:fs'); fs.appendFileSync(${JSON.stringify(log)}, '+'); setTimeout(() => fs.appendFileSync(${JSON.stringify(log)}, '-'), 500)`;
const run = code => new Promise(resolve => spawn(process.execPath, [script, '--', process.execPath, '-e', code], { env, stdio: 'inherit' }).on('exit', resolve));

try {
    appendFileSync(log, '');
    // Slot 0 belongs to a process that does not exist; the four jobs must still share two slots.
    mkdirSync(join(env.TSUZURI_SLOT_DIR, 'slot-0'), { recursive: true });
    writeFileSync(join(env.TSUZURI_SLOT_DIR, 'slot-0', 'pid'), '2147483647');
    const started = Date.now();
    assert.deepEqual(await Promise.all([run(slow), run(slow), run(slow), run(slow)]), [0, 0, 0, 0]);
    let running = 0;
    let peak = 0;
    for (const mark of readFileSync(log, 'utf8')) {
        running += mark === '+' ? 1 : -1;
        peak = Math.max(peak, running);
    }
    assert.equal(peak, 2, 'two jobs should run at once');
    assert.ok(Date.now() - started >= 1000, 'four jobs of 0.5 s on two slots take at least 1 s');
    assert.equal(await run('process.exit(7)'), 7, 'the exit code of the command is passed through');
    console.log('slot: at most 2 of 4 jobs ran at once, a dead owner was replaced, exit codes pass through');
} finally {
    rmSync(root, { recursive: true, force: true });
}
