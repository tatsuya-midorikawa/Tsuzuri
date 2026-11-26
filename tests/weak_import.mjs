import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

if (process.platform !== "darwin") {
  console.log("macOS weak import & min_version: skipped on non-macOS host");
  process.exit(0);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(process.argv[2] ?? join(root, "target", "debug", "tsuzuri"));
const systemClang = process.env.TSUZURI_CLANG ?? "clang";
const bundledClang =
  process.env.TSUZURI_BUNDLED_CLANG ?? join(root, "target", "toolchain", "bin", "tsuzuri-clang");

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

const scratch = mkdtempSync(join(tmpdir(), "tsuzuri-m17-"));
const clangs = [["clang", systemClang]];
if (existsSync(bundledClang)) {
  clangs.push(["bundled", bundledClang]);
}

try {
  // 1. `[macos] min_version = "13.0"` and default `12.0` in `LC_BUILD_VERSION` (`minos`).
  const verProj = join(scratch, "verproj");
  mkdirSync(verProj, { recursive: true });
  writeFileSync(
    join(verProj, "Tsuzuri.toml"),
    '[package]\nname = "verproj"\nversion = "0.1.0"\nedition = "2028"\n[macos]\nmin_version = "13.0"\n',
  );
  writeFileSync(
    join(verProj, "Main.tz"),
    "export def answer :: i32 = 42i32\ndef main :: unit -> i32 = \\() -> 0i32\n",
  );

  for (const [label, clangPath] of clangs) {
    const env = { ...process.env, TSUZURI_CLANG: clangPath, TSUZURI_CACHE_DIR: "" };
    delete env.MACOSX_DEPLOYMENT_TARGET;
    const exe13 = join(scratch, `ver13-${label}`);
    must(compiler, ["build", verProj, "-o", exe13], { env });
    const otool13 = must("otool", ["-l", exe13]).stdout;
    assert.match(otool13, /minos 13\.0/);
  }

  // 2. `@weak` function & data symbol dylib swap test across `-O0` and `-O3`.
  const dylibPath = join(scratch, "libm17w.dylib");
  const newC = join(scratch, "new.c");
  const oldC = join(scratch, "old.c");
  writeFileSync(
    newC,
    [
      "int m17_old_api(void) { return 3; }",
      "int m17_new_api(void) { return 7; }",
      "double m17_new_data = 12.5;",
      "",
    ].join("\n"),
  );
  writeFileSync(oldC, "int m17_old_api(void) { return 3; }\n");
  const newDylib = join(scratch, "new.dylib");
  const oldDylib = join(scratch, "old.dylib");
  must("clang", ["-dynamiclib", newC, "-install_name", dylibPath, "-o", newDylib]);
  must("clang", ["-dynamiclib", oldC, "-install_name", dylibPath, "-o", oldDylib]);

  const weakProj = join(scratch, "weakproj");
  mkdirSync(weakProj, { recursive: true });
  writeFileSync(
    join(weakProj, "Main.tz"),
    [
      "@trusted",
      'extern "m17_old_api" def old_api :: unit -> i32',
      "@trusted @weak",
      'extern "m17_new_api" def new_api :: unit -> i32',
      "@trusted @weak",
      'extern "m17_new_data" view def new_data :: unit -> Maybe<f64>',
      "",
      "def main :: Array<string> -> i32 = \\args ->",
      '    if args.length > 0 && args[0] == "call" then',
      "        new_api ()",
      '    elif args.length > 0 && args[0] == "data" then',
      "        match new_data () with",
      "        | Maybe.Some v -> if v == 12.5 then 11i32 else 12i32",
      "        | Maybe.None -> 10i32",
      "    else",
      "        old_api ()",
      "",
    ].join("\n"),
  );

  for (const [label, clangPath] of clangs) {
    const env = { ...process.env, TSUZURI_CLANG: clangPath, TSUZURI_CACHE_DIR: "" };
    for (const opt of ["-O0", "-O3"]) {
      rmSync(dylibPath, { force: true });
      copyFileSync(newDylib, dylibPath);
      const bin = join(scratch, `weak-${label}${opt}`);
      must(compiler, ["build", weakProj, opt, "--trap-info", "-L", scratch, "-l", "m17w", "-o", bin], {
        env,
      });
      const callNew = run(bin, ["call"]);
      assert.equal(callNew.status, 7, `new dylib call (${label} ${opt}): ${callNew.signal} ${callNew.stderr}`);
      const dataNew = run(bin, ["data"]);
      assert.equal(dataNew.status, 11, `new dylib data (${label} ${opt}): ${dataNew.signal} ${dataNew.stderr}`);

      // Swap in `old.dylib` without `m17_new_api` / `m17_new_data` (unlink first so Apple Silicon
      // XNU creates a fresh vnode rather than reusing the previous dylib's cached code signature):
      // uncalled weak symbol lets process start and return 3; weak data symbol returns `None` (10);
      // calling missing weak function prints the runtime diagnostic and traps!
      rmSync(dylibPath, { force: true });
      copyFileSync(oldDylib, dylibPath);
      const baseOld = run(bin, []);
      assert.equal(baseOld.status, 3, `old dylib base (${label} ${opt}): ${baseOld.signal} ${baseOld.stderr}`);
      const dataOld = run(bin, ["data"]);
      assert.equal(dataOld.status, 10, `old dylib data (${label} ${opt}): ${dataOld.signal} ${dataOld.stderr}`);
      const trapped = run(bin, ["call"]);
      assert.notEqual(trapped.status, 0);
      assert.match(
        trapped.stderr,
        /Tsuzuri runtime: the weak symbol 'm17_new_api' is not available on this system/,
      );
      assert.match(trapped.stderr, /trap: assertion failed/);
    }
  }

  console.log(
    `macOS M17: min_version & @weak passed across ${clangs.map(([k]) => k).join(", ")}`,
  );
} finally {
  rmSync(scratch, { recursive: true, force: true });
}
