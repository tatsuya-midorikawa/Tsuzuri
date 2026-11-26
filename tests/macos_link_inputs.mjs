import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

if (process.platform !== "darwin") {
  console.log("macOS link inputs: skipped on non-macOS host");
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

const scratch = mkdtempSync(join(tmpdir(), "tsuzuri-m20-"));
const clangs = [["clang", systemClang]];
if (existsSync(bundledClang)) {
  clangs.push(["bundled", bundledClang]);
}

try {
  // Build custom `M20Probe.framework` once with system clang.
  const fwSearch = join(scratch, "fwdir");
  const fwBundle = join(fwSearch, "M20Probe.framework");
  mkdirSync(fwBundle, { recursive: true });
  const probeC = join(scratch, "probe.c");
  writeFileSync(probeC, "long m20_probe_answer(void) { return 42; }\n");
  must("clang", [
    "-dynamiclib",
    probeC,
    "-install_name",
    "@rpath/M20Probe.framework/M20Probe",
    "-o",
    join(fwBundle, "M20Probe"),
  ]);

  // 1. `--framework AppKit` vs `-l objc` alone, plus `[native] frameworks`.
  const fwProj = join(scratch, "fw");
  mkdirSync(fwProj, { recursive: true });
  writeFileSync(
    join(fwProj, "Main.tz"),
    [
      "@trusted",
      'extern "objc_getClass" def objc_get_class :: ref ZUtf8String -> i64',
      "",
      "def main :: unit -> i32 = \\() ->",
      '    let cls = objc_get_class (ZUtf8String.literal "NSApplication")',
      "    if cls != 0 then 0i32 else 1i32",
      "",
    ].join("\n"),
  );

  const manifestProj = join(scratch, "fwmanifest");
  mkdirSync(manifestProj, { recursive: true });
  writeFileSync(
    join(manifestProj, "Tsuzuri.toml"),
    '[package]\nname = "fwmanifest"\nversion = "0.1.0"\nedition = "2028"\n[native]\nframeworks = ["AppKit"]\n',
  );
  writeFileSync(
    join(manifestProj, "Main.tz"),
    [
      "@trusted",
      'extern "objc_getClass" def objc_get_class :: ref ZUtf8String -> i64',
      "def main :: unit -> i32 = \\() ->",
      '    if objc_get_class (ZUtf8String.literal "NSApplication") != 0 then 0i32 else 1i32',
      "",
    ].join("\n"),
  );

  // 2. Custom framework with `-F` and `--rpath`, and `--emit shared`.
  const customProj = join(scratch, "custom");
  mkdirSync(customProj, { recursive: true });
  writeFileSync(
    join(customProj, "Main.tz"),
    [
      "@trusted",
      'extern "m20_probe_answer" def probe_answer :: unit -> i64',
      "export def exported_probe :: i64 = probe_answer ()",
      "def main :: unit -> i32 = \\() -> if probe_answer () == 42 then 0i32 else 1i32",
      "",
    ].join("\n"),
  );

  // 3. Phase 2: `@link(framework = "AppKit")` on `NSApplicationLoad` and `@link(framework = "Foundation")` on M06 data symbol.
  const attrProj = join(scratch, "attr");
  mkdirSync(attrProj, { recursive: true });
  writeFileSync(
    join(attrProj, "Main.tz"),
    [
      "extern type copy CFString",
      '@trusted @link(framework = "AppKit")',
      'extern "NSApplicationLoad" def application_load :: unit -> unit',
      '@trusted @link(framework = "Foundation")',
      'extern "NSDefaultRunLoopMode" view def default_mode :: unit -> CFString',
      '@trusted @link(framework = "CoreFoundation")',
      'extern "CFStringGetLength" def cf_len :: CFString -> i64',
      "",
      "def main :: unit -> i32 = \\() ->",
      "    application_load ()",
      "    if cf_len (default_mode ()) == 21 then 0i32 else 1i32",
      "",
    ].join("\n"),
  );

  // Without `--framework AppKit`, `-l objc` alone returns 1 (AppKit not loaded).
  const noFwBin = join(scratch, "nofw");
  must(compiler, ["build", fwProj, "-l", "objc", "-o", noFwBin], {
    env: { ...process.env, TSUZURI_CACHE_DIR: "" },
  });
  assert.equal(run(noFwBin, []).status, 1);

  for (const [label, clangPath] of clangs) {
    const env = { ...process.env, TSUZURI_CLANG: clangPath, TSUZURI_CACHE_DIR: "" };
    for (const opt of ["-O0", "-O3"]) {
      const appBin = join(scratch, `app-${label}${opt}`);
      must(compiler, ["build", fwProj, opt, "--framework", "AppKit", "-o", appBin], { env });
      must(appBin, []);
      const otoolL = must("otool", ["-L", appBin]).stdout;
      assert.match(otoolL, /AppKit\.framework/);

      const manBin = join(scratch, `man-${label}${opt}`);
      must(compiler, ["build", manifestProj, opt, "-o", manBin], { env });
      must(manBin, []);

      const customBin = join(scratch, `custom-${label}${opt}`);
      must(
        compiler,
        [
          "build",
          customProj,
          opt,
          "-F",
          fwSearch,
          "--framework",
          "M20Probe",
          "--rpath",
          "@executable_path/fwdir",
          "-o",
          customBin,
        ],
        { env },
      );
      must(customBin, []);
      const otoolLoad = must("otool", ["-l", customBin]).stdout;
      assert.match(otoolLoad, /LC_RPATH[\s\S]*?path @executable_path\/fwdir/);

      // Without `--rpath`, dyld refuses to launch `customBin`.
      const noRpathBin = join(scratch, `norpath-${label}${opt}`);
      must(
        compiler,
        ["build", customProj, opt, "-F", fwSearch, "--framework", "M20Probe", "-o", noRpathBin],
        { env },
      );
      assert.notEqual(run(noRpathBin, []).status, 0);

      // `--emit shared` embeds `-framework` and `LC_RPATH` too.
      const sharedLib = join(scratch, `libcustom-${label}${opt}.dylib`);
      must(
        compiler,
        [
          "build",
          customProj,
          opt,
          "--emit",
          "shared",
          "-F",
          fwSearch,
          "--framework",
          "M20Probe",
          "--rpath",
          fwSearch,
          "-o",
          sharedLib,
        ],
        { env },
      );
      assert.match(must("otool", ["-L", sharedLib]).stdout, /M20Probe\.framework/);
      assert.match(must("otool", ["-l", sharedLib]).stdout, /LC_RPATH/);

      // Phase 2 `@link(framework = ...)` without `--framework`.
      const attrBin = join(scratch, `attr-${label}${opt}`);
      must(compiler, ["build", attrProj, opt, "-o", attrBin], { env });
      must(attrBin, []);
    }
  }

  console.log(`macOS link inputs: Phase 1 & Phase 2 passed across ${clangs.map(([k]) => k).join(", ")}`);
} finally {
  rmSync(scratch, { recursive: true, force: true });
}
