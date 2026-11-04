// F10, TZ12: Mutex.with_lock and channels under `--trap-mode return`. A trap inside the lock is caught by the boundary
// of the call, which releases the locks that the thread holds (src/runtime/trap.c calls the hook that
// src/runtime/tzroutine.c installs), so the next call locks again, and a Parallel whose item traps inside
// the lock (inside the boundary Parallel runs every item on the calling thread) leaves no lock held. A wait on a
// channel that nothing can end is a deadlock, which the verdict turns into a trap of the call, and the thread is
// clean afterwards. Native objects embed the runtime; hosts that link the LLVM output provide trap.c themselves and
// link the scheduler, here with one, two and eight Ps. Usage: node tests/trap_return_sync.mjs [path/to/tsuzuri]
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

if (process.platform === "win32") {
  console.log("trap_return_sync: skipped (a Windows object with an embedded runtime is E2002)");
  process.exit(0);
}

const repository = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const clang = process.env.TSUZURI_CLANG ?? "clang";
const fixture = join(repository, "tests/fixtures/trap_return_sync");
const root = mkdtempSync(join(tmpdir(), "tsuzuri-trap-return-sync-"));
const sanitizer = process.env.TSUZURI_TSAN === "1" ? ["-fsanitize=thread"] : process.env.TSUZURI_ASAN === "1" ? ["-fsanitize=address"] : [];
// TSUZURI_SYNC_ROUNDS raises the number of calls for a stress run.
const rounds = Number(process.env.TSUZURI_SYNC_ROUNDS ?? 300);

function execute(program, args, success = true, environment = {}) {
  const result = spawnSync(program, args, {
    cwd: repository, encoding: "utf8", timeout: 300_000, maxBuffer: 8 * 1024 * 1024, env: { ...process.env, ...environment },
  });
  assert.ifError(result.error);
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}
const build = (flags, output) => execute(compiler, ["build", fixture, ...flags, "-o", output]);

try {
  const headers = join(root, "include");
  mkdirSync(headers);
  build(["--emit", "header", "--trap-mode", "return"], join(headers, "trap_return_sync.h"));
  for (const optimization of ["-O0", "-O3"]) {
    const object = join(root, `sync${optimization}.o`);
    build(["--emit", "object", "--trap-mode", "return", optimization], object);
    const host = join(root, `host${optimization}`);
    execute(clang, ["-std=c11", "-Wall", "-Wextra", "-Werror", "-O1", "-pthread", ...sanitizer, `-I${headers}`, "tests/trap_return_sync_host.c", object, "-o", host]);
    assert.match(execute(host, [String(rounds)]).stdout, new RegExp(`^ok ${rounds}\\n$`), `object ${optimization}`);
    // The same host over a counting allocator: no block outlives a call, trapped or not. The embedded
    // runtime of the object is not instrumented, so a sanitizer would miss its atomics and report races
    // that its synchronization excludes; the linked builds below run the sanitizer over every line of C.
    const counted = join(root, `counted${optimization}`);
    execute(clang, ["-std=c11", "-Wall", "-Wextra", "-Werror", "-O1", "-pthread", "-DCOUNTING", `-I${headers}`, "tests/trap_return_sync_host.c", object, "-o", counted]);
    assert.match(execute(counted, [String(rounds)]).stdout, new RegExp(`^ok ${rounds}\\n$`), `counted object ${optimization}`);

    // The host of LLVM IR: the trap runtime in the host, and the tzroutine scheduler, which Mutex, Parallel and
    // DataChannel use natively (TZ12), at one, two and eight Ps.
    const ir = join(root, `sync${optimization}.ll`);
    build(["--emit", "llvm", "--trap-mode", "return", optimization], ir);
    const text = readFileSync(ir, "utf8");
    for (const declaration of ["declare i32 @tsuzuri_tz_mutex32_lock(ptr)", "declare i32 @tsuzuri_tz_parallel(ptr, ptr, i64)", "declare i32 @tsuzuri_channel_send(ptr, ptr)"]) {
      assert.ok(text.includes(`${declaration}\n`), `${optimization}: ${declaration}`);
    }
    const linked = join(root, `linked${optimization}`);
    execute(clang, ["-std=c11", optimization, "-Wno-override-module", "-pthread", ...sanitizer, "-DLINKED", `-I${headers}`,
      "tests/trap_return_sync_host.c", ir, "-x", "c", "src/runtime/tzroutine.c", "-o", linked]);
    for (const processors of [1, 2, 8]) {
      const run = execute(linked, [String(rounds)], true, { TSUZURI_MAXPROCS: String(processors) });
      assert.match(run.stdout, new RegExp(`^ok ${rounds}\\n$`), `linked ${optimization} with ${processors} Ps`);
    }
  }
  console.log("trap_return_sync: a trap inside Mutex.with_lock releases the lock, and a deadlock or a trap in a channel program is a trap of the call (object and linked IR, 1, 2 and 8 Ps, -O0 and -O3)");
} finally {
  rmSync(root, { recursive: true, force: true });
}
