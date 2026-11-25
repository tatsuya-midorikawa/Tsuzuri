import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { pathToFileURL } from "node:url";

const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const clang = process.env.TSUZURI_CLANG ?? "clang";
const windows = process.platform === "win32";
const isBundledClang = /tsuzuri-clang(?:\.exe)?$/i.test(clang);
const root = mkdtempSync(join(tmpdir(), "tsuzuri-c-layout-"));

function run(program, args, cwd = root) {
  const result = spawnSync(program, args, { cwd, encoding: "utf8", timeout: 180_000, maxBuffer: 16 * 1024 * 1024 });
  assert.ifError(result.error);
  assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}

try {
  // 1. Native C host (Phase 1 + Phase 2): header static asserts (C and C++), padding bytes, packed nested records, handles, FnPtr, ZString.
  const fixture = resolve("tests/fixtures/c_layout");
  const header = join(root, "c_layout.h");
  run(compiler, ["build", fixture, "--emit", "header", "-o", header]);
  const cFlags = windows && !isBundledClang
    ? [`--target=${process.arch === "arm64" ? "aarch64" : "x86_64"}-pc-windows-msvc`, "-fuse-ld=lld", "-D_CRT_SECURE_NO_WARNINGS"]
    : [];
  const hostC = resolve("tests/c_layout_host.c");
  // Check C++ compatibility of generated header static asserts.
  const cppProbe = join(root, "header_cpp.cpp");
  writeFileSync(cppProbe, '#include "c_layout.h"\nint probe(void) { return (int)sizeof(tz_record_4Main_9SmallFont); }\n');
  run(clang, [...cFlags, "-x", "c++", "-fsyntax-only", "-I", root, cppProbe]);

  for (const opt of ["-O0", "-O3"]) {
    const obj = join(root, `c_layout${opt}.o`);
    const exe = join(root, `c_layout${opt}${windows ? ".exe" : ""}`);
    run(compiler, ["build", fixture, "--no-cache", opt, "--emit", "object", "-o", obj]);
    run(clang, [...cFlags, opt, "-I", root, hostC, obj, ...(windows ? [] : ["-lm"]), "-o", exe]);
    run(exe, []);
  }

  // 2. WASM + Phase 3 JS bindings for numeric-only @c_layout records (including pack = 2 and fixed arrays).
  const wasmSrc = join(root, "Main.tz");
  writeFileSync(wasmSrc, `
@c_layout(pack = 2)
record PackedWasm {
    a: i8,
    b: i16u,
    c: i32,
    arr: [i16; 3]
}
export def round_trip :: ref PackedWasm -> PackedWasm = \\p ->
    PackedWasm { a: p.a + 1i8, b: p.b + 2i16u, c: p.c + 3i32, arr: [p.arr[2], p.arr[1], p.arr[0]] }
`);
  for (const opt of ["-O0", "-O3"]) {
    const wasm = join(root, `wasm${opt}.wasm`);
    const glue = join(root, `wasm${opt}.mjs`);
    run(compiler, ["build", wasmSrc, "--target", "wasm32", opt, "-o", wasm]);
    run(compiler, ["build", wasmSrc, "--target", "wasm32", "--emit", "bindings-js", "-o", glue]);
    const mod = await import(pathToFileURL(glue).href);
    const inst = await mod.load(readFileSync(wasm));
    const res = inst.exports.round_trip({ a: -120, b: 60000, c: 123456, arr: [-10, 20, -30] });
    assert.deepEqual(res, { a: -119, b: 60002, c: 123459, arr: [-30, 20, -10] });
  }
  console.log("C layout: native Phase 1+2 (padding, pack, handles, FnPtr, ZString, C++ header) and WASM Phase 3 JS bindings at -O0 and -O3 passed");
} finally {
  rmSync(root, { recursive: true, force: true });
}
