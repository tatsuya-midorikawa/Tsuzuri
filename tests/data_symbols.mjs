import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(
  process.argv[2] ??
    join(root, "target", "debug", process.platform === "win32" ? "tsuzuri.exe" : "tsuzuri"),
);
const clang = process.env.TSUZURI_CLANG ?? "clang";
const hostC = join(root, "tests", "data_symbols_host.c");

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { encoding: "utf8", ...options });
  if (result.error) throw result.error;
  return result;
}

function must(command, args, options = {}) {
  const result = run(command, args, options);
  assert.equal(
    result.status,
    0,
    `${command} ${args.join(" ")}\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`,
  );
  return result;
}

const scratch = mkdtempSync(join(tmpdir(), "tsuzuri-m06-"));
const baseEnv = { ...process.env, TSUZURI_CACHE_DIR: "" };

try {
  // Phase 2 E2E (all platforms: Windows, macOS, Linux) at -O0 and -O3: ZUtf8String + Maybe<ZUtf8String>.
  const zHostObj = join(scratch, process.platform === "win32" ? "zhost.obj" : "zhost.o");
  must(clang, ["-c", "-O2", "-DM06_ZUTF8_ONLY", hostC, "-o", zHostObj]);

  const zDir = join(scratch, "zapp");
  mkdirSync(zDir, { recursive: true });
  const zMain = join(zDir, "Main.tz");
  writeFileSync(
    zMain,
    [
      "@trusted",
      'extern "m06_c_strlen" def c_strlen :: ref ZUtf8String -> i64',
      "@trusted",
      'extern "m06_c_strlen" def c_opt_strlen :: Maybe<ref ZUtf8String> -> i64',
      "@trusted",
      'extern "m06_c_lookup" def c_lookup :: i32 -> Maybe<ZUtf8String>',
      "",
      "def main :: Array<string> -> i32 = \\args ->",
      '    if args.length > 0 && args[0] == "bad" then',
      "        match c_lookup 3i32 with",
      "        | Maybe.Some _ -> 1i32",
      "        | Maybe.None -> 2i32",
      "    else",
      '        let a = c_strlen (ZUtf8String.literal "Tsuzuri")',
      "        let b = c_opt_strlen Maybe.None",
      '        let dyn_in = u8"😀!"',
      "        let c = match ZUtf8String.of (ref dyn_in) with",
      "            | Result.Ok z -> c_strlen (ref z)",
      "            | Result.Error _ -> -99",
      "        let d = match c_lookup 0i32 with | Maybe.None -> 1 | Maybe.Some _ -> 0",
      "        let e = match c_lookup 1i32 with",
      "            | Maybe.Some z ->",
      "                let u = ZUtf8String.to_utf8string (ref z)",
      '                if u == u8"こんにちは" then u.length else -1',
      "            | Maybe.None -> -2",
      "        let f = match c_lookup 2i32 with",
      "            | Maybe.Some z -> (ZUtf8String.to_utf8string (ref z)).length",
      "            | Maybe.None -> -3",
      "        if a == 7 && b == -1 && c == 5 && d == 1 && e == 15 && f == 0 then 0i32 else 9i32",
      "",
    ].join("\n"),
  );

  for (const opt of ["-O0", "-O3"]) {
    const exe = join(
      scratch,
      `zmain${opt}${process.platform === "win32" ? ".exe" : ""}`,
    );
    must(
      compiler,
      ["build", zMain, opt, "--trap-info", "--link", zHostObj, "-o", exe],
      { env: baseEnv },
    );
    must(exe, [], { env: baseEnv });
    const bad = run(exe, ["bad"], { env: baseEnv });
    assert.notEqual(bad.status, 0, `expected Encoding trap at ${opt}`);
    assert.match(bad.stderr, /invalid UTF-8|encoding/i);
  }

  if (process.platform === "win32") {
    // On Windows, reached data symbols fail with E1008 data_symbol_target.
    const winDir = join(scratch, "winapp");
    mkdirSync(winDir, { recursive: true });
    const winData = join(winDir, "Main.tz");
    writeFileSync(
      winData,
      '@trusted\nextern "m06_mutable" view def read_mut :: unit -> i32\ndef main :: unit -> i32 = \\() -> read_mut ()\n',
    );
    const rejected = run(
      compiler,
      ["build", "--json", winData, "-o", join(scratch, "win.exe")],
      { env: baseEnv },
    );
    assert.notEqual(rejected.status, 0);
    assert.match(rejected.stderr + rejected.stdout, /data_symbol_target/);
    console.log("M06: ZUtf8String native -O0/-O3 and Windows data_symbol_target passed");
  } else {
    // Phase 1 E2E on macOS and Linux: both .o and shared library (.so / .dylib) at -O0 and -O3.
    const hostObj = join(scratch, "host.o");
    const shlibExt = process.platform === "darwin" ? "dylib" : "so";
    const hostShlib = join(scratch, `libm06host.${shlibExt}`);
    must(clang, ["-c", "-O2", "-fPIC", hostC, "-o", hostObj]);
    must(
      clang,
      process.platform === "darwin"
        ? ["-dynamiclib", "-O2", hostObj, "-o", hostShlib]
        : ["-shared", "-O2", hostObj, "-o", hostShlib],
    );

    const dataDir = join(scratch, "dataapp");
    mkdirSync(dataDir, { recursive: true });
    const dataMain = join(dataDir, "Main.tz");
    writeFileSync(
      dataMain,
      [
        "extern type copy BoxPtr",
        "extern type copy Queue",
        "record Pair { x: i32, y: i32 }",
        "",
        '@trusted extern "m06_min_i8" view def min_i8 :: unit -> i8',
        '@trusted extern "m06_max_u16" view def max_u16 :: unit -> i16u',
        '@trusted extern "m06_min_i64" view def min_i64 :: unit -> i64',
        '@trusted extern "m06_neg_zero" view def neg_zero :: unit -> f32',
        '@trusted extern "m06_nan_f64" view def nan_f64 :: unit -> f64',
        '@trusted extern "m06_null_ptr" view def null_as_maybe :: unit -> Maybe<BoxPtr>',
        '@trusted extern "m06_null_ptr" view def null_as_handle :: unit -> BoxPtr',
        '@trusted extern "m06_nonnull_ptr" view def nonnull_ptr :: unit -> BoxPtr',
        '@trusted extern "m06_point" view def read_point :: unit -> Pair',
        '@trusted extern "m06_main_q" address def main_q :: unit -> Queue',
        '@trusted extern "m06_mutable" view def read_mut :: unit -> i32',
        '@trusted extern "m06_init_nan" def init_nan :: unit -> unit',
        '@trusted extern "m06_mutate" def mutate :: i32 -> unit',
        '@trusted extern "m06_check_queue_addr" def check_queue_addr :: Queue -> i32',
        '@trusted extern "m06_read_box" def read_box :: BoxPtr -> i64',
        "",
        "def main :: Array<string> -> i32 = \\args ->",
        '    if args.length > 0 && args[0] == "nulltrap" then',
        "        let _bad = null_as_handle ()",
        "        1i32",
        "    else",
        "        init_nan ()",
        "        let ok_i8 = min_i8 () == -128i8",
        "        let ok_u16 = max_u16 () == 65535i16u",
        "        let ok_i64 = min_i64 () + 1 == -9223372036854775807",
        "        let nz = neg_zero () as f64",
        "        let ok_nz = nz == 0.0 && Math.signbit nz",
        "        let ok_nan = Math.is_nan (nan_f64 ())",
        "        let ok_none = match null_as_maybe () with | Maybe.None -> true | Maybe.Some _ -> false",
        "        let ok_box = read_box (nonnull_ptr ()) == 777",
        "        let p = read_point ()",
        "        let ok_pair = p.x == -17i32 && p.y == 29i32",
        "        let ok_addr = check_queue_addr (main_q ()) == 1i32",
        "        let before = read_mut ()",
        "        mutate 22i32",
        "        let after = read_mut ()",
        "        mutate 20i32",
        "        if ok_i8 && ok_u16 && ok_i64 && ok_nz && ok_nan && ok_none && ok_box && ok_pair && ok_addr && before + after == 42i32 then 0i32 else 5i32",
        "",
      ].join("\n"),
    );

    for (const [label, linkArgs] of [
      ["object", ["--link", hostObj]],
      [
        "shlib",
        [
          "-L",
          scratch,
          "-l",
          "m06host",
          ...(process.platform === "linux" ? [`-Wl,-rpath,${scratch}`] : []),
        ],
      ],
    ]) {
      for (const opt of ["-O0", "-O3"]) {
        const exe = join(scratch, `data-${label}${opt}`);
        must(compiler, ["build", dataMain, opt, "--trap-info", ...linkArgs, "-o", exe], {
          env: baseEnv,
        });
        const runEnv =
          process.platform === "darwin"
            ? { ...baseEnv, DYLD_LIBRARY_PATH: scratch }
            : baseEnv;
        must(exe, [], { env: runEnv });
        const trapped = run(exe, ["nulltrap"], { env: runEnv });
        assert.notEqual(trapped.status, 0);
        assert.match(trapped.stderr, /null extern handle/i);
      }
    }

    console.log("M06: data symbols (.o + shared lib) and ZUtf8String at -O0/-O3 passed");
  }
} finally {
  rmSync(scratch, { recursive: true, force: true });
}
