import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";

const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const clang = process.env.TSUZURI_CLANG ?? "clang";
const windows = process.platform === "win32";
const isBundledClang = /tsuzuri-clang(?:\.exe)?$/i.test(clang);
const root = mkdtempSync(join(tmpdir(), "tsuzuri-extern-view-"));

function run(program, args, success = true) {
  const result = spawnSync(program, args, { cwd: root, encoding: "utf8", timeout: 180_000, maxBuffer: 16 * 1024 * 1024 });
  assert.ifError(result.error);
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}

try {
  const fixture = resolve("tests/fixtures/extern_view");
  const header = join(root, "extern_view.h");
  run(compiler, ["build", fixture, "--emit", "header", "-o", header]);
  const cFlags = windows && !isBundledClang
    ? [`--target=${process.arch === "arm64" ? "aarch64" : "x86_64"}-pc-windows-msvc`, "-fuse-ld=lld", "-D_CRT_SECURE_NO_WARNINGS"]
    : [];
  const hostC = resolve("tests/extern_view_host.c");

  for (const opt of ["-O0", "-O3"]) {
    const obj = join(root, `extern_view${opt}.o`);
    const exe = join(root, `extern_view${opt}${windows ? ".exe" : ""}`);
    run(compiler, ["build", fixture, "--no-cache", opt, "--emit", "object", "-o", obj]);
    run(clang, [...cFlags, opt, "-I", root, hostC, obj, ...(windows ? [] : ["-lm"]), "-o", exe]);
    run(exe, []);
  }

  // Trap on null (0) and misaligned (+2 on 4-byte align, +4 on 8-byte align) with call-site location.
  const trapSrc = join(root, "Main.tz");
  writeFileSync(trapSrc, [
    "record Point { x: i32, y: i32 }",
    "record Wide { a: i64, b: i32 }",
    "@trusted",
    "extern view def read_pt :: i64 -> Point",
    "@trusted",
    "extern view def read_wide :: i64 -> Wide",
    "def main :: Array<string> -> i32 = \\args ->",
    '    if args[0] == "null" then',
    "        (read_pt 0).x",
    '    elif args[0] == "align4" then',
    "        (read_pt 2).x",
    "    else",
    "        (read_wide 4).b",
    "",
  ].join("\n"));
  for (const opt of ["-O0", "-O3"]) {
    const trapExe = join(root, `trap${opt}${windows ? ".exe" : ""}`);
    run(compiler, ["build", trapSrc, "--no-cache", opt, "--trap-info", "-o", trapExe]);
    for (const [mode, line] of [["null", 9], ["align4", 11], ["align8", 13]]) {
      const res = run(trapExe, [mode], false);
      assert.notEqual(res.status, 0, `${mode} ${opt} must trap`);
      assert.match(res.stderr, new RegExp(`index out of bounds at .*Main\\.tz:${line}:`), `${mode} ${opt}: ${res.stderr}`);
    }
  }

  // M21: temporary record values passed to `extern view` writes at -O0 and -O3.
  const tempDir = join(root, "temp_write");
  mkdirSync(tempDir);
  writeFileSync(join(tempDir, "Main.tz"), [
    "@c_layout",
    "record P { x: i32, y: i32 }",
    "@trusted",
    'extern "calloc" def c_calloc :: i64u -> i64u -> i64',
    "@trusted",
    "extern view def read_p :: i64 -> P",
    "@trusted",
    "extern view def write_p :: i64 -> ref P -> unit",
    "def shifted :: i32 -> P = \\n -> P { x: n, y: n + 1i32 }",
    "def main :: unit -> i32 = \\() ->",
    "    let base = c_calloc 4 8",
    "    let c = true",
    "    write_p base (ref (P { x: 1i32, y: 2i32 }))",
    "    write_p (base + 8) (shifted 10i32)",
    "    write_p (base + 16) (ref (if c then P { x: 30i32, y: 40i32 } else P { x: 0i32, y: 0i32 }))",
    "    let a = read_p base",
    "    let b = read_p (base + 8)",
    "    let d = read_p (base + 16)",
    "    a.x + a.y + b.x + b.y + d.x + d.y",
    "",
  ].join("\n"));
  for (const opt of ["-O0", "-O3"]) {
    const tempExe = join(root, `temp_write${opt}${windows ? ".exe" : ""}`);
    run(compiler, ["build", tempDir, "--no-cache", opt, "-o", tempExe]);
    const res = run(tempExe, [], false);
    assert.equal(res.status, 94, `${opt} temporary write expected exit 94, got ${res.status}\n${res.stdout}\n${res.stderr}`);
  }
  console.log("Extern view: Phase 1+2 C host round-trip, temporary record writes, padding preservation, and null/alignment call-site traps at -O0 and -O3 passed");
} finally {
  rmSync(root, { recursive: true, force: true });
}
