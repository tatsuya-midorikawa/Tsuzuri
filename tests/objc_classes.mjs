// M01: `extern class`, the built-in module `ObjC`, and the ownership of Objective-C objects, end
// to end on macOS. The fixture is linked with an ARC host (tests/objc_classes_host.m) that counts
// its objects between init and dealloc; every expected line is derived from those counts and from
// the host's own answers, never from the compiler under test.
//
// Usage: node tests/objc_classes.mjs [path/to/tsuzuri]
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(process.argv[2] ?? join(root, "target", "debug", "tsuzuri"));
const clang = process.env.TSUZURI_CLANG ?? "clang";
// The program is not under tests/fixtures/: the repository-wide tests build every fixture for
// their host, and this one reaches Objective-C, which only an Apple target has.
const fixture = join(root, "tests", "objc", "classes");
const host = join(root, "tests", "objc_classes_host.m");
const bundledClang =
  process.env.TSUZURI_BUNDLED_CLANG ?? join(root, "target", "toolchain", "bin", "tsuzuri-clang");

if (process.platform !== "darwin") {
  console.log("objc_classes: skipped (the Objective-C runtime needs macOS)");
  process.exit(0);
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { encoding: "utf8", timeout: 120_000, ...options });
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

// A clang that is not Apple's (Homebrew's, the bundled toolchain's) needs the SDK named.
const sdk = run("xcrun", ["--show-sdk-path"]);
const sysroot = sdk.status === 0 ? ["-isysroot", sdk.stdout.trim()] : [];

// What each case prints, from the host's counts:
// - `balance`: two tags of 21, and no object left after the pool is popped.
// - `retained`: one object while the +1 result is held, none after its drop.
// - `forgotten_attribute`: the same +1 function declared without its attribute is retained once
//   more, so one reference leaks (the safe side of a missing attribute).
// - `retain_and_drop`: one object under two references, still one after the first drop, none
//   after the second.
// - `handshake`: the +0 result is dropped before the pool is popped and is gone already.
// - `maybe_nil`: nil is `None`; an object is `Some` (tag 21).
// - `downcast`: a parent is no child; a child is a parent, an NSObject and an object, and no
//   TzShared. Two objects live while the casts are held, none after.
// - `moves`: a record with two, an array with three and a Vec with two objects: seven.
// - `sync_thread`: one object before it moves into the tzroutine, none once that has dropped it.
// - `selector`: the selrefs name the selectors that the strings name (the host compares names).
// - `data_symbol`: the host's global is nil, then holds an object that the read retains and the
//   end of the match releases: one object while the global holds it, none after the host resets
//   the global.
const expected = {
  balance: "42 0",
  retained: "1 0",
  forgotten_attribute: "1 1",
  retain_and_drop: "1 1 0 0",
  handshake: "0 0",
  maybe_nil: "None Some21 0",
  downcast: "None Some21 Some Some None 2 0",
  moves: "7 0",
  sync_thread: "1 0",
  selector: "1 1 1 0",
  data_symbol: "None Some21 1 0",
};

const scratch = mkdtempSync(join(tmpdir(), "tsuzuri-m01-"));
const env = { ...process.env, TSUZURI_CACHE_DIR: "" };
let ran = 0;
try {
  const hostObject = join(scratch, "host.o");
  must(clang, [
    "-fobjc-arc",
    "-O2",
    "-mmacosx-version-min=12.0",
    ...sysroot,
    "-c",
    host,
    "-o",
    hostObject,
  ]);
  for (const optimization of ["-O0", "-O3"]) {
    const executable = join(scratch, `probe${optimization}`);
    // No `-l objc`: the driver adds it for an Objective-C image.
    must(
      compiler,
      ["build", fixture, optimization, "--trap-info", "--link", hostObject, "-o", executable],
      { env },
    );
    for (const [name, line] of Object.entries(expected)) {
      const result = must(executable, [name], { env });
      assert.equal(result.stdout, `${line}\n`, `${name} at ${optimization}`);
      assert.equal(result.stderr, "", `${name} at ${optimization}`);
      ran += 1;
    }
    // A nil result where the type has no `Maybe` traps at the boundary.
    const trapped = run(executable, ["nil_trap"], { env });
    // The trap ends the process with SIGTRAP, which a shell reports as exit code 133.
    assert.ok(
      trapped.signal === "SIGTRAP" || trapped.status === 133,
      `nil_trap at ${optimization}: status ${trapped.status}, signal ${trapped.signal}\n${trapped.stderr}`,
    );
    assert.match(trapped.stderr, /trap: null extern handle at .*Main\.tz:\d+:\d+/);
    assert.equal(trapped.stdout, "");
    ran += 1;

    // The object file holds the class reference in `__objc_classrefs` and the image info that
    // makes the linker treat it as an Objective-C image (Apple's ld drops the references to
    // external classes from the linked output, so the sections are checked in the object).
    const object = join(scratch, `probe${optimization}.o`);
    must(compiler, ["build", fixture, optimization, "--emit", "object", "-o", object], { env });
    const sections = must("otool", ["-l", object]).stdout;
    for (const section of ["__objc_classrefs", "__objc_selrefs", "__objc_imageinfo"]) {
      assert.match(sections, new RegExp(`sectname ${section}`), `${section} at ${optimization}`);
    }
    ran += 1;
  }

  // The counting allocator: the program's own heap is balanced too.
  const counted = join(scratch, "counted");
  must(
    compiler,
    ["build", fixture, "-O3", "--allocator", "counting", "--link", hostObject, "-o", counted],
    { env },
  );
  for (const name of ["balance", "moves", "downcast"]) {
    assert.equal(must(counted, [name], { env }).stdout, `${expected[name]}\n`, `${name} counted`);
    ran += 1;
  }

  // The bundled toolchain (zig as the linker driver, with lld) links the same program: M20 finds
  // the SDK for it. Without a bundled toolchain the case is skipped with its reason.
  if (existsSync(bundledClang)) {
    const executable = join(scratch, "probe-bundled");
    must(compiler, ["build", fixture, "-O3", "--link", hostObject, "-o", executable], {
      env: { ...env, TSUZURI_CLANG: bundledClang },
    });
    for (const name of ["balance", "handshake", "selector"]) {
      assert.equal(must(executable, [name], { env }).stdout, `${expected[name]}\n`, `${name} bundled`);
      ran += 1;
    }
    // `-l objc` on the command line takes libobjc out of the libraries that the driver adds: the
    // link still needs the SDK to find it.
    const explicit = join(scratch, "probe-bundled-explicit");
    must(
      compiler,
      ["build", fixture, "-O3", "--link", hostObject, "-l", "objc", "-o", explicit],
      { env: { ...env, TSUZURI_CLANG: bundledClang } },
    );
    assert.equal(must(explicit, ["balance"], { env }).stdout, `${expected.balance}\n`, "explicit -l objc");
    ran += 1;
  } else {
    console.log(`objc_classes: the bundled toolchain link is skipped (${bundledClang} is missing)`);
  }
} finally {
  rmSync(scratch, { recursive: true, force: true });
}
console.log(`objc_classes: ok (${ran} checks)`);
