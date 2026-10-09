// F10: Mutex.with_lock under `--trap-mode return`. A trap inside the lock is caught by the boundary of
// the call, which releases the lock and clears the thread's state (src/runtime/trap.c calls the
// hook that src/runtime/task.c installs), so the next call locks again and the children of a group that
// wait for the lock are not left waiting. Native objects embed the runtime; hosts that link the LLVM
// output provide trap.c themselves, here at several pool sizes. Usage: node tests/trap_return_sync.mjs [path/to/tsuzuri]
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
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

function execute(program, args, success = true) {
  const result = spawnSync(program, args, { cwd: repository, encoding: "utf8", timeout: 300_000, maxBuffer: 8 * 1024 * 1024 });
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

    // The host of LLVM IR: the trap runtime in the host, the scheduler at one, two and many threads.
    const ir = join(root, `sync${optimization}.ll`);
    build(["--emit", "llvm", "--trap-mode", "return", optimization], ir);
    for (const processors of [1, 2, 8]) {
      const scheduler = join(root, `scheduler-${processors}.c`);
      writeFileSync(scheduler, `#define TZ_TASK_SYSCONF(name) ${processors}\n#define TZ_TRAP_BOUNDARY\n#include ${JSON.stringify(join(repository, "src/runtime/task.c"))}\n`);
      const linked = join(root, `linked-${processors}${optimization}`);
      execute(clang, ["-std=c11", optimization, "-Wno-override-module", "-pthread", ...sanitizer, "-DLINKED", `-I${headers}`,
        "tests/trap_return_sync_host.c", ir, scheduler, "-o", linked]);
      assert.match(execute(linked, [String(rounds)]).stdout, new RegExp(`^ok ${rounds}\\n$`), `linked ${optimization} with ${processors} processors`);
    }
  }
  console.log("trap_return_sync: a trap inside Mutex.with_lock releases the lock (object and linked IR, 1, 2 and 8 threads, -O0 and -O3)");
} finally {
  rmSync(root, { recursive: true, force: true });
}
