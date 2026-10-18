// Runs a command once one of N slots is free, so that sessions in several worktrees do not start heavy jobs
// (cargo nextest, release builds, node suites) at the same time: `node scripts/slot.mjs -- cargo nextest run --locked`.
// The slots are directories under the temporary directory, shared by every worktree of the machine.
// TSUZURI_SLOTS sets N (default: a third of the cores, at least 2). A slot whose owner has died is taken over.
import { spawn } from 'node:child_process';
import { mkdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { availableParallelism, constants, tmpdir } from 'node:os';
import { join } from 'node:path';

const command = process.argv.slice(2);
if (command[0] === '--') command.shift();
if (!command.length) {
    console.error('usage: node scripts/slot.mjs -- <command> [arguments]');
    process.exit(2);
}
const slots = Number(process.env.TSUZURI_SLOTS) || Math.max(2, Math.floor(availableParallelism() / 3));
const root = process.env.TSUZURI_SLOT_DIR ?? join(tmpdir(), 'tsuzuri-slots');
mkdirSync(root, { recursive: true });

function alive(pid) {
    try {
        process.kill(pid, 0);
        return true;
    } catch (error) {
        return error.code === 'EPERM';
    }
}

function stale(slot) {
    try {
        return !alive(Number(readFileSync(join(slot, 'pid'), 'utf8')));
    } catch {
        // No pid yet: the owner is between mkdir and write. Only a slot that stays so for a minute is a crash's leftover.
        try {
            return Date.now() - statSync(slot).mtimeMs > 60_000;
        } catch {
            return false;
        }
    }
}

function take(slot) {
    try {
        mkdirSync(slot);
        writeFileSync(join(slot, 'pid'), String(process.pid));
        return true;
    } catch (error) {
        if (error.code === 'EEXIST') return false;
        throw error;
    }
}

// Removing a dead owner's slot and taking it over must not interleave with another process doing the same,
// or two of them end up owning one slot; `reap` is the guard directory that serializes the two.
function reap(slot) {
    const guard = join(root, 'reap');
    try {
        mkdirSync(guard);
    } catch (error) {
        if (error.code !== 'EEXIST') throw error;
        try {
            if (Date.now() - statSync(guard).mtimeMs > 10_000) rmSync(guard, { recursive: true, force: true });
        } catch {}
        return false;
    }
    try {
        if (!stale(slot)) return false;
        rmSync(slot, { recursive: true, force: true });
        return take(slot);
    } finally {
        rmSync(guard, { recursive: true, force: true });
    }
}

function acquire() {
    for (let index = 0; index < slots; index++) {
        const slot = join(root, `slot-${index}`);
        if (take(slot) || reap(slot)) return slot;
    }
    return undefined;
}

let slot;
for (let waited = 0; !(slot = acquire()); waited += 2) {
    if (waited % 30 === 0) console.error(`slot: all ${slots} slots are busy, waiting (TSUZURI_SLOTS=${slots})`);
    await new Promise(resolve => setTimeout(resolve, 2000));
}
const release = () => rmSync(slot, { recursive: true, force: true });
const child = spawn(command[0], command.slice(1), { stdio: 'inherit' });
for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, () => child.kill(signal));
child.on('error', error => {
    release();
    console.error(`slot: ${error.message}`);
    process.exit(127);
});
child.on('exit', (code, signal) => {
    release();
    process.exit(code ?? 128 + (constants.signals[signal] ?? 1));
});
