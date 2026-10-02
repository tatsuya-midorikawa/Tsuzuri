// E14 Phase 2: `--trap-mode return` for native objects. A C host calls the tsuzuri_try_<name> exports,
// a trap comes back as status 1 with a site of the .trap.json table, every block a call allocated is
// freed, nested boundaries answer status 2, and a Task.parallel worker trap reaches the call that
// submitted the group. Usage: node tests/trap_return.mjs [path/to/tsuzuri]
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

if (process.platform === "win32") {
  console.log("trap_return: skipped (a Windows object with an embedded runtime is E2002)");
  process.exit(0);
}

const repository = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const clang = process.env.TSUZURI_CLANG ?? "clang";
const fixture = join(repository, "tests/fixtures/trap_return");
const source = readFileSync(join(fixture, "Main.tz"), "utf8");
const root = mkdtempSync(join(tmpdir(), "tsuzuri-trap-return-"));
const rounds = 3000;

function execute(program, args, success = true) {
  const result = spawnSync(program, args, { cwd: repository, encoding: "utf8", timeout: 300_000, maxBuffer: 8 * 1024 * 1024 });
  assert.ifError(result.error);
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}
const build = (target, flags, output, success = true) => execute(compiler, ["build", target, ...flags, "-o", output], success);

const lineOf = (needle) => {
  const index = source.split("\n").findIndex((line) => line.includes(needle));
  assert.ok(index >= 0, needle);
  return index + 1;
};
const expected = {
  div_zero: ["integer division by zero", lineOf("fn div a b")],
  div_overflow: ["integer division overflow", lineOf("fn div a b")],
  fill_div: ["integer division by zero", lineOf("values[count - 1] / divisor")],
  imported_div: ["integer division by zero", lineOf("numbers[count - 1] / divisor")],
  make_negative: [/^allocation/, 0],
  label_div: ["integer division by zero", lineOf("fn label_div")],
  parallel_div: ["integer division by zero", lineOf("return (scratch[1] + i) /")],
  mixed_trap_first: ["integer division by zero", lineOf("Result.Ok (1 / (index - trap_at))")],
  nested_div: ["integer division by zero", lineOf("100 / (i * 4 + j - trap_at)")],
};

function checkRun(result, table) {
  const lines = result.stdout.trim().split("\n");
  assert.equal(lines.at(-1), `ok ${rounds}`);
  const { sites } = JSON.parse(readFileSync(table, "utf8"));
  const traps = new Map(lines.filter((line) => line.startsWith("trap ")).map((line) => {
    const [, name, site, kind] = line.split(" ");
    return [name, { site: Number(site), kind: Number(kind) }];
  }));
  assert.deepEqual([...traps.keys()], Object.keys(expected));
  for (const [name, [kind, line]] of Object.entries(expected)) {
    const { site, kind: number } = traps.get(name);
    // The host-visible kind is the kind encoded in the site id.
    assert.equal((site - 1) % 15, number, name);
    const entry = sites.find((candidate) => candidate.id === site);
    assert.ok(entry, `${name}: site ${site} is not in the table`);
    if (typeof kind === "string") assert.equal(entry.kind, kind, name);
    else assert.match(entry.kind, kind, name);
    if (line) {
      assert.ok(entry.path.endsWith("Main.tz"), name);
      assert.equal(entry.span.line, line, name);
    }
  }
}

try {
  // The header gains the trap record and the try_ prototypes only in trap mode.
  const plainHeader = join(root, "plain.h");
  build(fixture, ["--emit", "header"], plainHeader);
  assert.doesNotMatch(readFileSync(plainHeader, "utf8"), /tsuzuri_try_|tsuzuri_trap_info/);
  const headers = join(root, "include");
  mkdirSync(headers);
  const header = join(headers, "trap_return.h");
  build(fixture, ["--emit", "header", "--trap-mode", "return"], header);
  const text = readFileSync(header, "utf8");
  assert.match(text, /typedef struct \{\n {4}uint32_t site;\n {4}uint32_t kind;\n\} tsuzuri_trap_info;/);
  assert.match(text, /int32_t tsuzuri_try_div\(tsuzuri_trap_info \*trap, int64_t \*result, int64_t arg0, int64_t arg1\);/);
  assert.match(text, /int32_t tsuzuri_try_make\(tsuzuri_trap_info \*trap, tsuzuri_i64_buffer \*out, int64_t arg0\);/);
  assert.match(text, /int32_t tsuzuri_try_noop\(tsuzuri_trap_info \*trap, int64_t \*result\);/);
  assert.match(text, /int64_t tz_div\(int64_t arg0, int64_t arg1\);/);

  // Option errors.
  const refused = (flags, pattern, action = "build") => {
    const output = action === "build" ? ["-o", join(root, "refused.out")] : [];
    const result = execute(compiler, [action, fixture, ...flags, ...output], false);
    assert.equal(result.status, 2, flags.join(" "));
    assert.match(result.stderr, pattern, flags.join(" "));
  };
  refused(["--trap-mode", "return", "--target", "wasm32", "--emit", "object"], /E2000.*--trap-mode return is only valid for native object, llvm or header output/s);
  refused(["--trap-mode", "return", "--emit", "exe"], /E2000.*--trap-mode return is only valid/s);
  refused(["--trap-mode", "other", "--emit", "object"], /trap mode must be 'return'/);
  refused(["--trap-mode", "return", "--trap-mode", "return", "--emit", "object"], /trap mode specified more than once/);
  refused(["--trap-mode", "return"], /--trap-mode is only valid with build/, "run");
  const callbacks = execute(compiler, ["build", join(repository, "tests/fixtures/ffi_extensions"), "--emit", "llvm", "--trap-mode", "return", "-o", join(root, "callbacks.ll")], false);
  assert.equal(callbacks.status, 1);
  assert.match(callbacks.stderr, /E2000.*--trap-mode return cannot be combined with extern callbacks/s);

  const ordinary = join(root, "ordinary");
  mkdirSync(ordinary);
  writeFileSync(join(ordinary, "Main.tz"), `
export def ordinary_make :: i64 -> [i64]
fn ordinary_make count = new [i64](count, \\index -> index)
export def ordinary_sum :: ref [i64] -> i64
fn ordinary_sum values = Array.sum values
export def ordinary_parallel :: i64 -> i64
fn ordinary_parallel count = Task.run task {
    let! results = Task.parallel (new [Task<i64>](count, index -> task { index }))
    return Array.sum (&results)
}
`);

  for (const optimization of ["-O0", "-O3"]) {
    // LLVM IR: the tracked heap and the raise call, deterministic, and no direct libc heap calls.
    const ir = join(root, `trap_return${optimization}.ll`);
    build(fixture, ["--emit", "llvm", "--trap-mode", "return", optimization], ir);
    const text = readFileSync(ir, "utf8");
    assert.match(text, /call void @tsuzuri_trap_raise\(i32 %site, i32 %kind\)/);
    assert.match(text, /define i32 @tsuzuri_try_div\(ptr %trap, ptr %result, i64 %arg0, i64 %arg1\)/);
    assert.match(text, /@tsuzuri_tracked_malloc/);
    assert.doesNotMatch(text, /@malloc\b|@realloc\b|@free\b/);
    const again = join(root, `again${optimization}.ll`);
    build(fixture, ["--emit", "llvm", "--trap-mode", "return", optimization], again);
    assert.equal(readFileSync(again, "utf8"), text);
    assert.ok(readFileSync(`${ir}.trap.json`, "utf8").includes('"sites"'));

    // The shipped object: its embedded runtime, then the same host over a counting allocator.
    const object = join(root, `trap_return${optimization}.o`);
    build(fixture, ["--emit", "object", "--trap-mode", "return", optimization], object);
    const plain = join(root, `plain${optimization}`);
    execute(clang, ["-std=c11", "-Wall", "-Wextra", "-Werror", "-O1", "-pthread", `-I${headers}`, "tests/trap_return_host.c", object, "-o", plain]);
    checkRun(execute(plain, [String(rounds)]), `${object}.trap.json`);
    const counting = join(root, `counting${optimization}`);
    execute(clang, ["-std=c11", "-Wall", "-Wextra", "-Werror", "-O1", "-pthread", "-DCOUNTING", `-I${headers}`, "tests/trap_return_host.c", object, "-o", counting]);
    checkRun(execute(counting, [String(rounds)]), `${object}.trap.json`);

    const ordinaryObject = join(root, `ordinary${optimization}.o`);
    build(ordinary, ["--emit", "object", optimization], ordinaryObject);
    for (const [order, objects] of [["ordinary-first", [ordinaryObject, object]], ["boundary-first", [object, ordinaryObject]]]) {
      for (const counted of [false, true]) {
        const mixed = join(root, `mixed-${order}-${counted}${optimization}`);
        execute(clang, ["-std=c11", "-Wall", "-Wextra", "-Werror", "-O1", "-pthread", "-DMIXED_OBJECTS", ...(counted ? ["-DCOUNTING"] : []),
          `-I${headers}`, "tests/trap_return_host.c", ...objects, "-o", mixed]);
        checkRun(execute(mixed, [String(rounds)]), `${object}.trap.json`);
      }
    }

    // A host that links the LLVM output supplies the trap runtime and the scheduler itself.
    const linked = join(root, `linked${optimization}`);
    execute(clang, ["-std=c11", optimization, "-Wno-override-module", "-pthread", "-DCOUNTING", `-I${headers}`, "tests/trap_return_host.c", ir,
      "-x", "c", "-DTZ_TRAP_BOUNDARY", "src/runtime/task.c", "-o", linked]);
    checkRun(execute(linked, [String(rounds)]), `${ir}.trap.json`);
  }

  // The runtime on its own: counted allocation, groups, nesting and concurrent hosts.
  const unit = (name, flags, topologies) => {
    const program = join(root, name);
    execute(clang, ["-std=c11", "-Wall", "-Wextra", "-Werror", ...flags, "-pthread", "tests/trap_boundary_runtime.c", "-o", program]);
    for (const topology of topologies) assert.match(execute(program, [topology]).stdout, /^trap boundary runtime: \d+ checks passed\n$/);
  };
  unit("unit-O0", ["-O0"], ["0", "1", "2", "4", "32"]);
  unit("unit-O3", ["-O3"], ["0", "1", "2", "4", "32"]);
  unit("unit-asan", ["-g", "-O1", "-fsanitize=address,undefined", "-fno-sanitize-recover=undefined"], ["0", "1", "4"]);
  unit("unit-tsan", ["-g", "-O1", "-fsanitize=thread"], ["0", "2"]);

  console.log("trap_return: 2 optimization levels x (object, counted object, linked IR, both mixed link orders with/without counting), 4 runtime builds, option errors, header");
} finally {
  rmSync(root, { recursive: true, force: true });
}
