import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const clang = process.env.TSUZURI_CLANG ?? "clang";
const ar = process.env.TSUZURI_AR ?? "ar";
const hostSource = resolve("tests/ffi_extensions_host.c");
const root = mkdtempSync(join(tmpdir(), "tsuzuri-ffi-"));
const fixture = join(root, "fixture");
cpSync("tests/fixtures/ffi_extensions", fixture, { recursive: true });
function execute(program, args, success = true) {
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 180_000, maxBuffer: 16 * 1024 * 1024 });
  assert.ifError(result.error);
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}
function rejected(args, code) {
  const result = execute(compiler, args, false);
  assert.notEqual(result.status, 0, args.join(" "));
  assert.match(result.stderr, new RegExp(`error\\[${code}\\]`), `${args.join(" ")}\n${result.stderr}`);
}
// A copy of the fixture with an entry expression: counters 10 = 59 and callbacks 4 = 39.
function application(name) {
  const directory = join(root, name);
  mkdirSync(directory);
  writeFileSync(join(directory, "Main.tz"), `${readFileSync(join(fixture, "Main.tz"), "utf8")}\ncounters 10 + callbacks 4\n`);
  return directory;
}
try {
  const header = join(root, "tz-ffi.h");
  execute(compiler, ["build", fixture, "--emit", "header", "-o", header]);
  const headerText = readFileSync(header, "utf8");
  assert.equal(headerText.match(/typedef struct tz_handle_4Main_7Counter_s \*tz_handle_4Main_7Counter;/g).length, 1);
  assert.match(headerText, /tz_handle_4Main_7Counter tz_pass_through\(tz_handle_4Main_7Counter arg0\);/);
  // An explicit symbol is the host's own: the header declares no prototype for it.
  assert.doesNotMatch(headerText, /e12_|sqrt/);
  const irPath = join(root, "ffi.ll");
  execute(compiler, ["build", fixture, "--emit", "llvm", "-o", irPath]);
  const ir = readFileSync(irPath, "utf8").replaceAll("@malloc", "@tracked_alloc").replaceAll("@free", "@tracked_free").replaceAll("@realloc", "@tracked_realloc");
  writeFileSync(irPath, ir);
  for (const optimization of ["-O0", "-O3"]) {
    const executable = join(root, `ffi${optimization}`);
    execute(clang, [optimization, "-Wno-override-module", "-DNDEBUG", "-DE12_HOST_MAIN", irPath, hostSource, "-I", root, "-lm", "-pthread", "-o", executable]);
    execute(executable, []);
    // A trap inside a callback ends the process abnormally.
    assert.notEqual(execute(executable, ["trap"], false).status, 0);
    const object = join(root, `ffi${optimization}.o`);
    execute(compiler, ["build", fixture, "--emit", "object", optimization, "-o", object]);
    const linked = join(root, `linked${optimization}`);
    execute(clang, [optimization, "-DNDEBUG", "-DE12_HOST_MAIN", object, hostSource, "-I", root, "-lm", "-pthread", "-o", linked]);
    execute(linked, []);
    console.log(`ffi extensions: native ${optimization} passed`);
  }

  const app = application("app");
  const host = join(root, "host.o");
  execute(clang, ["-DNDEBUG", "-c", hostSource, "-o", host]);
  const run = (args, pthread = true) => { const result = execute(compiler, ["run", ...args, ...(pthread ? ["-l", "pthread"] : [])]); assert.equal(result.stdout.trim(), "98"); };
  run([app, "--link", host]);
  const library = join(root, "lib");
  mkdirSync(library);
  execute(ar, ["rcs", join(library, "libe12host.a"), host]);
  run([app, "-L", library, "-l", "e12host"]);
  const packaged = application("packaged");
  cpSync(host, join(packaged, "host.o"));
  writeFileSync(join(packaged, "Tsuzuri.toml"), '[package]\nname = "ffi-app"\nversion = "0.1.0"\n\n[native]\nlink = ["host.o"]\nlibraries = ["pthread"]\n');
  run([packaged], false);
  // build writes an executable that runs without the compiler too.
  const built = join(root, "built");
  execute(compiler, ["build", app, "-O0", "--link", host, "-l", "pthread", "-o", built]);
  assert.equal(execute(built, []).stdout.trim(), "98");
  // Debug-info executables take the same inputs (macOS links them in a second step).
  const debuggable = join(root, "debuggable");
  execute(compiler, ["build", app, "-g", "-O0", "--link", host, "-l", "pthread", "-o", debuggable]);
  assert.equal(execute(debuggable, []).stdout.trim(), "98");
  // So does tsuzuri test, which builds an executable of its own.
  const tested = join(root, "tested");
  mkdirSync(tested);
  writeFileSync(join(tested, "Main.tz"), `${readFileSync(join(fixture, "Main.tz"), "utf8")}\ntest "counters" =\n    assert (counters 10 == 59)\n\ntest "callbacks" =\n    assert (callbacks 4 == 39)\n`);
  assert.match(execute(compiler, ["test", tested, "--link", host, "-l", "pthread"]).stdout, /2 passed; 0 failed/);
  const routineHandles = join(root, "routine-handles");
  mkdirSync(routineHandles);
  writeFileSync(join(routineHandles, "Main.tz"), [
    "extern type copy IdHandle",
    'extern "w18_id_new" def id_new :: i64 -> IdHandle',
    'extern "w18_id_value" def id_value :: IdHandle -> i64',
    'extern "w18_id_borrow" def id_borrow :: ref IdHandle -> i64',
    "let handle = id_new 10",
    "match DataChannel.bounded 1 with",
    "| (sender, receiver) ->",
    "    tzroutine {",
    "        let _ = DataChannel.send (ref sender) (id_value handle)",
    "        ()",
    "    }",
    "    let scoped = Routine.scope (ref handle) 2 (\\w i -> id_borrow w + i)",
    "    let sent = match DataChannel.recv (ref receiver) with | Some v -> v | None -> 0",
    "    sent + scoped[0] + scoped[1] + id_value handle",
    "",
  ].join("\n"));
  assert.equal(execute(compiler, ["run", routineHandles, "--link", host, "-l", "pthread"]).stdout.trim(), "43");
  const multiExtern = join(root, "multi-extern");
  mkdirSync(multiExtern);
  writeFileSync(join(multiExtern, "Main.tz"), [
    "record Rect { left: i32, top: i32, right: i32, bottom: i32 }",
    'extern "w14_ffi_word" def word_int :: i64 -> i64 -> i64',
    'extern "w14_ffi_word" def word_rect :: i64 -> ref Rect -> i64',
    "let r = Rect { left: 7i32, top: 8i32, right: 9i32, bottom: 10i32 }",
    "word_int 0 42 + word_rect 1 (ref r)",
    "",
  ].join("\n"));
  for (const extra of [["-O0"], ["-O3"], ["--backend", "fast"]]) {
    const out = join(root, `multi-${extra.join("")}`);
    const built = execute(compiler, ["build", multiExtern, ...extra, "--link", host, "-l", "pthread", "-o", out], false);
    if (extra[0] === "--backend" && /built without the fast backend|not available on/.test(built.stderr)) continue;
    assert.equal(built.status, 0, built.stderr);
    assert.equal(execute(out, []).stdout.trim(), "50");
  }
  const nullableFixture = resolve("tests/fixtures/abi_nullable");
  execute(compiler, ["check", nullableFixture]);
  rejected(["build", nullableFixture, "--target", "wasm32", "-o", join(root, "nullable.wasm")], "E1008");
  rejected(["build", nullableFixture, "--target", "wasm64", "-o", join(root, "nullable64.wasm")], "E1008");
  rejected(["build", nullableFixture, "--target", "wasm32", "--emit", "bindings-js", "-o", join(root, "nullable.mjs")], "E1008");
  for (const optimization of ["-O0", "-O3"]) {
    const out = join(root, `nullable${optimization}`);
    execute(compiler, ["build", nullableFixture, optimization, "--link", host, "-l", "pthread", "-o", out]);
    assert.equal(execute(out, []).stdout.trim(), "45");
  }
  // W13: ref mut records/scalars (copy-in/copy-out), mutable slice buffers, and @count lengths.
  const w13Dir = join(root, "w13-ref-mut");
  mkdirSync(w13Dir);
  writeFileSync(join(w13Dir, "Main.tz"), [
    "record Rect { left: i32, top: i32, right: i32, bottom: i32 }",
    "record Narrow { a: i8, w: i16u, b: bool }",
    'extern "w13_inflate_rect" def inflate_rect :: ref mut Rect -> i32 -> i32 -> i32',
    'extern "w13_mut_narrow" def mut_narrow :: ref mut Narrow -> ref mut i16u -> ref mut bool -> i32',
    'extern "w13_noop_rect" def noop_rect :: ref mut Rect -> ref mut i32 -> unit',
    'extern "w13_fill_window" def fill_window :: ref mut [i16u..] -> i16u -> i32',
    "@count(1, i32)",
    "@count(2, i32u)",
    'extern "w13_sum_counts" def sum_counts :: ref [i32] -> ref [i32u] -> i64',
    "@count(2, none)",
    'extern "w13_write_counted_none" def write_none :: i32 -> ref mut [i32..] -> unit',
    "",
    "const BASE_ARR: [i16u] = [1i16u, 2i16u, 3i16u, 4i16u, 5i16u, 6i16u]",
    "",
    "def run_all :: unit -> i64 = \\() ->",
    "    let mut rc = Rect { left: 10i32, top: 20i32, right: 30i32, bottom: 40i32 }",
    "    let sum_in = inflate_rect (ref mut rc) 2i32 5i32",
    "    let rect_ok = sum_in == 100i32 && rc.left == 8i32 && rc.top == 15i32 && rc.right == 32i32 && rc.bottom == 45i32",
    "    let mut nar = Narrow { a: 5i8, w: 10i16u, b: false }",
    "    let mut w: i16u = 25i16u",
    "    let mut fl = false",
    "    let nar_in = mut_narrow (ref mut nar) (ref mut w) (ref mut fl)",
    "    let nar_ok = nar_in == 40i32 && nar.a == -7i8 && nar.w == 50000i16u && nar.b && w == 1025i16u && fl",
    "    let mut keep_rc = Rect { left: 1i32, top: 2i32, right: 3i32, bottom: 4i32 }",
    "    let mut keep_n = 77i32",
    "    noop_rect (ref mut keep_rc) (ref mut keep_n)",
    "    let untouched_ok = keep_rc.left == 1i32 && keep_rc.bottom == 4i32 && keep_n == 77i32",
    "    let mut win = BASE_ARR",
    "    let wrote = fill_window (ref mut win[2..<5]) 100i16u",
    "    let mut empty: [i16u] = []",
    "    let wrote0 = fill_window (ref mut empty) 1i16u",
    "    let win_ok = wrote == 3i32 && wrote0 == 0i32 && win[0] == 1i16u && win[1] == 2i16u && win[2] == 100i16u && win[3] == 110i16u && win[4] == 120i16u && win[5] == 6i16u",
    "    let sc = sum_counts (ref [10i32, 20i32, 30i32]) (ref [100i32u, 200i32u])",
    "    let mut raw: [i32] = [0i32, 0i32, 0i32, 99i32]",
    "    write_none 3i32 (ref mut raw[0..<3])",
    "    let cnt_ok = sc == 360 && raw[0] == 11i32 && raw[1] == 22i32 && raw[2] == 33i32 && raw[3] == 99i32",
    "    if rect_ok && nar_ok && untouched_ok && win_ok && cnt_ok then 42 else -1",
    "",
    "run_all ()",
    "",
  ].join("\n"));
  for (const extra of [["-O0"], ["-O3"], ["--backend", "fast"]]) {
    const out = join(root, `w13-${extra.join("")}`);
    const built = execute(compiler, ["build", w13Dir, ...extra, "--link", host, "-l", "pthread", "-o", out], false);
    if (extra[0] === "--backend" && /built without the fast backend|not available on/.test(built.stderr)) continue;
    assert.equal(built.status, 0, built.stderr);
    assert.equal(execute(out, []).stdout.trim(), "42");
  }
  // W20: ZString.literal, ZString.of (Latin-1 & non-BMP), ZString.to_string, and Maybe<ref ZString> under default and compact strings.
  const w20Dir = join(root, "w20-zstring");
  mkdirSync(w20Dir);
  writeFileSync(join(w20Dir, "Main.tz"), [
    'extern "w20_zlen" def zlen :: ref ZString -> i64',
    'extern "w20_zlen" def opt_zlen :: Maybe<ref ZString> -> i64',
    "",
    "def check_all :: unit -> i64 = \\() ->",
    '    let lit = ZString.literal "Tsuzuri"',
    '    let empty_lit = ZString.literal ""',
    '    let latin = "abc"',
    '    let astral = "Hi😀!"',
    "    let z_latin = match ZString.of (ref latin) with | Result.Ok z -> zlen (ref z) | Result.Error _ -> -99",
    "    let z_astral = match ZString.of (ref astral) with",
    "        | Result.Ok z ->",
    "            let back = ZString.to_string (ref z)",
    "            if back == astral then zlen (ref z) else -88",
    "        | Result.Error _ -> -77",
    '    let nul_err = match ZString.of "ab\\0cd" with',
    '        | Result.Error err -> if to_string err == "U+0000 at UTF-16 index 2 cannot be in a NUL-terminated string" then 1 else 0',
    "        | Result.Ok _ -> 0",
    "    let opt_some = opt_zlen (Maybe.Some lit)",
    "    let opt_none = opt_zlen Maybe.None",
    "    if zlen lit == 7 && zlen empty_lit == 0 && z_latin == 3 && z_astral == 5 && nul_err == 1 && opt_some == 7 && opt_none == -1 then 42 else -1",
    "",
    "check_all ()",
    "",
  ].join("\n"));
  for (const compact of [false, true]) {
    for (const optimization of ["-O0", "-O3"]) {
      const out = join(root, `w20-${compact ? "compact" : "utf16"}${optimization}`);
      const env = compact ? { ...process.env, TSUZURI_STRINGS: "compact" } : process.env;
      const res = spawnSync(compiler, ["build", w20Dir, "--no-cache", optimization, "--link", host, "-l", "pthread", "-o", out], { encoding: "utf8", env });
      assert.equal(res.status, 0, res.stderr);
      assert.equal(execute(out, []).stdout.trim(), "42");
    }
  }
  // W19: thread-bound `extern type local` handles stay on their creating OS thread across yields and MAXPROCS=1/4, and also lower cleanly on WASM.
  const threadBoundFixture = resolve("tests/fixtures/thread_bound");
  execute(compiler, ["check", threadBoundFixture]);
  execute(compiler, ["build", threadBoundFixture, "--target", "wasm32", "--wasm-feature", "jspi", "-o", join(root, "tb.wasm")]);
  for (const optimization of ["-O0", "-O3"]) {
    const out = join(root, `w19-thread-bound${optimization}`);
    execute(compiler, ["build", threadBoundFixture, "--no-cache", optimization, "--link", host, "-l", "pthread", "-o", out]);
    for (const procs of ["1", "4"]) {
      for (let round = 0; round < 5; round++) {
        const run = spawnSync(out, [], { encoding: "utf8", env: { ...process.env, TSUZURI_MAXPROCS: procs } });
        assert.equal(run.status, 0, run.stderr);
        assert.equal(run.stdout.trim(), "8704", `W19 thread_bound ${optimization} P=${procs} round=${round}`);
      }
    }
  }
  // NS01: non-Maybe extern handles trap with "null extern handle" on NULL across extern return, export param, and callback param.
  const nullTrap = join(root, "null-trap");
  mkdirSync(nullTrap);
  writeFileSync(join(nullTrap, "Main.tz"), [
    "extern type H",
    'extern "ns01_null_handle" def bad_return :: unit -> H',
    "export def bad_export :: own H -> i32",
    "fn bad_export _h = 1i32",
    "export def trigger_return :: i32 -> i32",
    "fn trigger_return _ = { let _ = bad_return (); 0i32 }",
    "",
  ].join("\n"));
  const nullHeader = join(root, "null_trap.h");
  execute(compiler, ["build", nullTrap, "--emit", "header", "--trap-mode", "return", "-o", nullHeader]);
  const nullObj = join(root, "null_trap.o");
  execute(compiler, ["build", nullTrap, "--emit", "object", "--trap-mode", "return", "-O3", "-o", nullObj]);
  const nullDriver = join(root, "null_driver.c");
  writeFileSync(nullDriver, [
    "#undef NDEBUG",
    "#include <assert.h>",
    '#include "null_trap.h"',
    "void *ns01_null_handle(void) { return 0; }",
    "int main(void) {",
    "  tsuzuri_trap_info trap = {0, 0}; int32_t out = 0;",
    "  assert(tsuzuri_try_bad_export(&trap, &out, 0) == 1 && trap.kind == 18);",
    "  trap = (tsuzuri_trap_info){0, 0};",
    "  assert(tsuzuri_try_trigger_return(&trap, &out, 0) == 1 && trap.kind == 18);",
    "  return 0;",
    "}",
    "",
  ].join("\n"));
  const nullRunner = join(root, "null_runner");
  execute(clang, ["-O2", "-pthread", "-I", root, nullDriver, nullObj, "-lm", "-o", nullRunner]);
  execute(nullRunner, []);
  const nullCbTrap = join(root, "null-cb-trap");
  mkdirSync(nullCbTrap);
  writeFileSync(join(nullCbTrap, "Main.tz"), [
    "extern type H",
    'extern "ns01_call_null" def bad_callback :: (H -> i32) -> i32',
    "private def take_cb :: own H -> i32",
    "fn take_cb _h = 1i32",
    "bad_callback take_cb",
    "",
  ].join("\n"));
  const nullCbRunner = join(root, "null_cb_runner");
  execute(compiler, ["build", nullCbTrap, "-O3", "--trap-info", "--link", host, "-l", "pthread", "-o", nullCbRunner]);
  const cbTrapRun = execute(nullCbRunner, [], false);
  assert.notEqual(cbTrapRun.status, 0);
  assert.match(cbTrapRun.stderr, /trap: null extern handle/);
  rejected(["run", app], "E2002");
  rejected(["run", app, "--link", join(root, "missing.o")], "E2001");
  rejected(["run", app, "-L", join(root, "missing-dir"), "-l", "e12host"], "E2001");
  rejected(["build", app, "--target", "wasm32", "--link", host, "-o", join(root, "x.wasm")], "E2000");
  rejected(["build", app, "-l", "lib-bad/name", "-o", join(root, "y")], "E2000");
  const same = join(root, "same.o");
  cpSync(host, same);
  rejected(["build", app, "--link", same, "-o", same], "E2003");
  assert.deepEqual(readFileSync(same), readFileSync(host));
  const dependent = join(root, "dependent");
  mkdirSync(join(dependent, "lib1"), { recursive: true });
  writeFileSync(join(dependent, "Tsuzuri.toml"), '[package]\nname = "root-app"\nversion = "0.1.0"\n\n[dependencies]\nlib1 = { path = "lib1" }\n');
  writeFileSync(join(dependent, "lib1", "Tsuzuri.toml"), '[package]\nname = "lib1"\nversion = "0.1.0"\n\n[native]\nlink = ["x.o"]\n');
  writeFileSync(join(dependent, "lib1", "Util.tz"), "export def one :: i64\nfn one = 1\n");
  writeFileSync(join(dependent, "Main.tz"), "1\n");
  rejected(["check", dependent], "E2000");
  assert.match(execute(compiler, ["check", dependent], false).stderr, /only the root package and dependencies it marks with native = true/);
  // The root package can let a dependency link host code: its inputs resolve against the dependency's own root.
  const trusted = join(root, "trusted");
  mkdirSync(join(trusted, "lib1"), { recursive: true });
  writeFileSync(join(trusted, "Tsuzuri.toml"), '[package]\nname = "root-app"\nversion = "0.1.0"\n\n[dependencies]\nlib1 = { path = "lib1", native = true }\n\n[native]\nlibraries = ["pthread"]\n');
  writeFileSync(join(trusted, "lib1", "Tsuzuri.toml"), '[package]\nname = "lib1"\nversion = "0.1.0"\n\n[native]\nlink = ["host.o"]\n');
  cpSync(host, join(trusted, "lib1", "host.o"));
  writeFileSync(join(trusted, "lib1", "Util.tz"), 'extern "e12_add_one" def add_one :: i64 -> i64\n\nexport def bump :: i64 -> i64\nfn bump value = add_one value\n');
  writeFileSync(join(trusted, "Main.tz"), "Lib1::Util.bump 41\n");
  assert.equal(execute(compiler, ["run", trusted]).stdout.trim(), "42");
  // Only the root's own opt-in counts: a dependency of a dependency cannot vouch for itself.
  const chained = join(root, "chained");
  mkdirSync(join(chained, "lib1", "lib2"), { recursive: true });
  writeFileSync(join(chained, "Tsuzuri.toml"), '[package]\nname = "root-app"\nversion = "0.1.0"\n\n[dependencies]\nlib1 = { path = "lib1", native = true }\n');
  writeFileSync(join(chained, "lib1", "Tsuzuri.toml"), '[package]\nname = "lib1"\nversion = "0.1.0"\n\n[dependencies]\nlib2 = { path = "lib2", native = true }\n');
  writeFileSync(join(chained, "lib1", "lib2", "Tsuzuri.toml"), '[package]\nname = "lib2"\nversion = "0.1.0"\n\n[native]\nlink = ["x.o"]\n');
  writeFileSync(join(chained, "lib1", "lib2", "Util.tz"), "export def one :: i64\nfn one = 1\n");
  writeFileSync(join(chained, "Main.tz"), "1\n");
  rejected(["check", chained], "E2000");

  // W17: `@link("w17")` links `libw17.a` with `-L` alone, including from a path dependency without `native = true`.
  const w17C = join(root, "w17.c");
  const w17Obj = join(root, "w17.o");
  const w17LibDir = join(root, "w17-lib");
  mkdirSync(w17LibDir);
  writeFileSync(w17C, "#include <stdint.h>\nint64_t w17_answer(void) { return 42; }\n");
  execute(clang, ["-O2", "-c", w17C, "-o", w17Obj]);
  execute(ar, ["rcs", join(w17LibDir, "libw17.a"), w17Obj]);
  const w17App = join(root, "w17-app");
  mkdirSync(join(w17App, "dep"), { recursive: true });
  writeFileSync(join(w17App, "Tsuzuri.toml"), '[package]\nname = "w17-app"\nversion = "0.1.0"\n\n[dependencies]\ndep = { path = "dep" }\n');
  writeFileSync(join(w17App, "dep", "Tsuzuri.toml"), '[package]\nname = "dep"\nversion = "0.1.0"\n');
  writeFileSync(join(w17App, "dep", "Api.tz"), '@trusted @link("w17")\nextern "w17_answer" def answer :: unit -> i64\ndef get :: unit -> i64 = \\() -> answer ()\n');
  writeFileSync(join(w17App, "Main.tz"), "export def run :: i64 -> i64 = \\x -> Dep::Api.get () + x\nDep::Api.get ()\n");
  const w17Bin = join(root, "w17-bin");
  execute(compiler, ["build", w17App, "-L", w17LibDir, "-o", w17Bin]);
  assert.equal(execute(w17Bin, []).stdout.trim(), "42");
  rejected(["build", w17App, "--target", "wasm32", "-o", join(root, "w17.wasm")], "E2000");
  // Without `@link("w17")`, linking fails with E2002.
  const w17NoLink = join(root, "w17-nolink");
  mkdirSync(w17NoLink);
  writeFileSync(join(w17NoLink, "Main.tz"), '@trusted\nextern "w17_answer" def answer :: unit -> i64\nanswer ()\n');
  rejected(["build", w17NoLink, "-L", w17LibDir, "-o", join(root, "w17-fail")], "E2002");

  // W16: `@last_error` atomically clears and reads POSIX `errno` around the call, including inside tzroutines.
  const w16Posix = join(root, "w16-posix");
  mkdirSync(w16Posix);
  writeFileSync(join(w16Posix, "Main.tz"), [
    "@trusted @last_error",
    'extern "close" def close_fd :: i32 -> i32 * i32u',
    "@trusted @last_error",
    'extern "getpid" def pid_checked :: unit -> i32 * i32u',
    "@trusted",
    'extern "close" def close_plain :: i32 -> i32',
    "def check_once :: unit -> bool = \\() ->",
    "    let (rc, err1) = close_fd (-1i32)",
    "    let _ = close_plain (-1i32)",
    "    let (p, err2) = pid_checked ()",
    "    rc == -1i32 && err1 == 9i32u && p > 0i32 && err2 == 0i32u",
    "def main :: unit -> i32 = \\() ->",
    "    let direct = check_once ()",
    "    let ch: Channel<i64> = Channel.make 1",
    "    let out = Channel.share (ref ch)",
    "    tzroutine { Channel.send (ref out) (if check_once () then 1 else 0) }",
    "    let spawned = match Channel.recv (ref ch) with | Maybe.Some x -> x | Maybe.None -> 0",
    "    if direct && spawned == 1 then 0i32 else 1i32",
    "",
  ].join("\n"));
  execute(compiler, ["check", w16Posix]);
  rejected(["build", w16Posix, "--target", "wasm32", "-o", join(root, "w16.wasm")], "E1008");
  rejected(["build", w16Posix, "--target", "wasm64", "-o", join(root, "w16-64.wasm")], "E1008");
  for (const optimization of ["-O0", "-O3"]) {
    const w16Bin = join(root, `w16${optimization}`);
    execute(compiler, ["build", w16Posix, "--no-cache", "--allocator", "counting", optimization, "-l", "pthread", "-o", w16Bin]);
    const res = execute(w16Bin, [], true, { TSUZURI_MAXPROCS: "2" });
    assert.equal(res.status, 0);
  }

  console.log("ffi extensions: run passed");

  const expectedImports = [
    "env.e12_host_now:function",
    "tsuzuri.e12_add_one:function",
    "tsuzuri.e12_apply_threaded:function",
    "tsuzuri.e12_apply_twice:function",
    "tsuzuri.e12_counter_add:function",
    "tsuzuri.e12_counter_free:function",
    "tsuzuri.e12_counter_new:function",
    "tsuzuri.e12_fold:function",
    "tsuzuri.e12_pair_mix:function",
    "tsuzuri.e12_run:function",
    "tsuzuri.e12_sum_with:function",
    "tsuzuri.e12_text_units:function",
    "tsuzuri.e12_visit:function",
    "tsuzuri.sqrt:function",
    "tsuzuri.w18_id_borrow:function",
    "tsuzuri.w18_id_new:function",
    "tsuzuri.w18_id_value:function",
  ];
  // Checks one linked module against the hand-computed table. `wide` is wasm64: pointers, handles and
  // table indices are BigInt there.
  function verify(module, wide) {
    assert.deepEqual(WebAssembly.Module.imports(module).map(({ module, name, kind }) => `${module}.${name}:${kind}`).sort(), expectedImports);
    // The function table is exported because callbacks are table indices.
    assert.ok(WebAssembly.Module.exports(module).some(({ name, kind }) => name === "__indirect_function_table" && kind === "table"));
    const handles = new Map();
    let next = wide ? 1n : 1;
    let instance;
    const callback = (index) => instance.exports.__indirect_function_table.get(index);
    const imports = {
      tsuzuri: {
        sqrt: Math.sqrt,
        e12_counter_new: (start) => { const handle = next++; handles.set(handle, start); return handle; },
        e12_counter_add: (handle, amount) => { const value = handles.get(handle) + amount; handles.set(handle, value); return value; },
        e12_counter_free: (handle) => { const value = handles.get(handle); assert.ok(handles.delete(handle)); return value; },
        e12_add_one: (value) => value + 1n,
        e12_apply_threaded: (index, value) => callback(index)(value),
        e12_apply_twice: (index, value) => callback(index)(callback(index)(value)),
        e12_fold: (index, count) => { let total = 0n; for (let item = 1n; item <= count; item++) total = callback(index)(total, item); return total; },
        e12_run: (index) => callback(index)(),
        e12_visit: (index, handle) => callback(index)(handle),
        e12_text_units: (index, _ptr, length, extra) => callback(index)(length) + extra,
        e12_pair_mix: (index, ptr) => {
          const view = new DataView(instance.exports.memory.buffer, Number(ptr), 16);
          return callback(index)(view.getBigInt64(0, true) * 10n + view.getBigInt64(8, true));
        },
        e12_sum_with: (index, ptr, length) => {
          const items = new BigInt64Array(instance.exports.memory.buffer, Number(ptr), Number(length));
          let total = 0n;
          for (const item of items) total += item;
          return callback(index)(total);
        },
        w18_id_new: (seed) => wide ? seed * 2n + 1n : Number(seed) * 2 + 1,
        w18_id_value: (handle) => (BigInt(handle) - 1n) / 2n,
        w18_id_borrow: (handle) => (BigInt(handle) - 1n) / 2n + 1n,
      },
      env: { e12_host_now: () => 40n },
    };
    instance = new WebAssembly.Instance(module, imports);
    assert.equal(instance.exports.tz_counters(10n), 59n);
    assert.equal(instance.exports.tz_callbacks(4n), 39n);
    assert.equal(instance.exports.tz_callback_shapes(10n), 82n);
    assert.equal(instance.exports.tz_threaded(3n), 9n);
    assert.equal(instance.exports.tz_temporaries(4n), 204n);
    assert.equal(instance.exports.tz_temporary_then_raise(3n), 27n);
    assert.equal(instance.exports.tz_temporary_then_raise(3037000500n), -1n);
    assert.equal(instance.exports.tz_copy_handles(10n), 66n);
    assert.equal(instance.exports.tz_misc(2.25), 40n);
    assert.equal(instance.exports.tz_misc(2), 0n);
    assert.equal(instance.exports.tz_pass_through(wide ? 7n : 7), wide ? 7n : 7);
    assert.equal(handles.size, 0);
    // A trap in a callback reaches the caller of the export; the instance is not used afterwards.
    assert.throws(() => instance.exports.tz_callback_trap(1n), WebAssembly.RuntimeError);
  }
  for (const optimization of ["-O0", "-O3"]) {
    const wasm = join(root, `ffi${optimization}.wasm`);
    execute(compiler, ["build", fixture, "--target", "wasm32", optimization, "-o", wasm]);
    verify(new WebAssembly.Module(readFileSync(wasm)), false);
    // Node.js 24 runs wasm64; older engines cannot load memory64, so the build is only linked there.
    const wide = join(root, `ffi${optimization}-64.wasm`);
    execute(compiler, ["build", fixture, "--target", "wasm64", optimization, "-o", wide]);
    if (Number(process.versions.node.split(".")[0]) >= 24) verify(new WebAssembly.Module(readFileSync(wide)), true);
    // Programs without callbacks keep a table-free export list.
    const plain = join(root, `plain${optimization}.wasm`);
    execute(compiler, ["build", "tests/fixtures/host_imports", "--target", "wasm32", optimization, "-o", plain]);
    assert.ok(!WebAssembly.Module.exports(new WebAssembly.Module(readFileSync(plain))).some(({ name }) => name === "__indirect_function_table"));
    console.log(`ffi extensions: WASM ${optimization} passed`);
  }
} finally { rmSync(root, { recursive: true, force: true }); }
