// M15: `tsuzuri build --emit app` on macOS. The expectations come from Apple's tools (codesign, plutil,
// dwarfdump, lldb, xattr, otool, open), not from the compiler.
//
//   node tests/macos_bundle.mjs [compiler] [--stage bundle|cache|open]
//
// `bundle` is E1 to E5, E7, E9 and E10 of the ticket, `cache` is E6, `open` is E8; without `--stage` all run.
// TSUZURI_MACOS_BUNDLE_NO_OPEN=1 skips E8 on a machine without LaunchServices (a session without a GUI).
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
  chmodSync, existsSync, lstatSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, readlinkSync,
  realpathSync, rmSync, statSync, symlinkSync, writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

if (process.platform !== "darwin") {
  console.log("macOS application bundles: skipped on a host that is not macOS");
  process.exit(0);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const positional = process.argv.slice(2).filter((argument, index, all) => !argument.startsWith("--") && all[index - 1] !== "--stage");
const compiler = resolve(positional[0] ?? join(root, "target", "debug", "tsuzuri"));
const stageAt = process.argv.indexOf("--stage");
const stage = stageAt < 0 ? null : process.argv[stageAt + 1];
assert.ok(stage === null || ["bundle", "cache", "open"].includes(stage), "--stage takes bundle, cache or open");
const runs = name => stage === null || stage === name;

const scratch = realpathSync(mkdtempSync(join(tmpdir(), "tsuzuri-m15-")));
const baseEnv = { ...process.env, TSUZURI_CACHE_DIR: join(scratch, "cache") };
delete baseEnv.MACOSX_DEPLOYMENT_TARGET;

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { encoding: "utf8", timeout: 300000, maxBuffer: 16 * 1024 * 1024, env: baseEnv, ...options });
  if (result.error) throw result.error;
  return result;
}

function must(command, args, options = {}) {
  const result = run(command, args, options);
  assert.equal(result.status, 0, `${command} ${args.join(" ")}\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`);
  return result;
}

/// A build that must fail with one diagnostic of `code` and `kind`; returns its message.
function rejected(args, code, kind, options = {}) {
  const result = run(compiler, [...args, "--json"], options);
  assert.notEqual(result.status, 0, `${args.join(" ")} must fail\n${result.stderr}`);
  const errors = result.stderr.split("\n").filter(line => line.startsWith("{")).map(line => JSON.parse(line)).filter(entry => entry.severity === "error");
  assert.equal(errors.length, 1, result.stderr);
  assert.equal(errors[0].code, code, result.stderr);
  assert.equal(errors[0].kind, kind, result.stderr);
  return errors[0].message;
}

const PROBE = `def main :: unit -> i32 = \\() ->
    let! directory = Env.current_dir ()
    let! executable = Env.executable_path ()
    do! IO.writeln (match directory with
        | Result.Ok text -> "cwd=" + text
        | Result.Error error -> Os.message (ref error))
    do! IO.writeln (match executable with
        | Result.Ok text -> "exe=" + text
        | Result.Error error -> Os.message (ref error))
    0
`;
const PACKAGE = '[package]\nname = "probe"\nversion = "0.1.0"\n';
// The `[macos]` of the example of the specification.
const PROBE_MACOS = '[macos]\nbundle_id = "dev.example.probe"\nname = "Probe"\nversion = "0.1.0"\nbuild = "1"\nicon = "assets/Probe.icns"\nui_element = true\n';

/// An icns file with no images: the magic and the length of the file.
function icns() {
  const bytes = Buffer.alloc(8);
  bytes.write("icns", 0, "latin1");
  bytes.writeUInt32BE(8, 4);
  return bytes;
}

let projects = 0;
/// A project directory with `manifest` after `[package]`, `Main.tz`, and the icon of the example.
function project(manifest = PROBE_MACOS, main = PROBE) {
  const directory = join(scratch, `project-${projects++}`);
  mkdirSync(join(directory, "assets"), { recursive: true });
  writeFileSync(join(directory, "Tsuzuri.toml"), PACKAGE + manifest);
  writeFileSync(join(directory, "Main.tz"), main);
  writeFileSync(join(directory, "assets", "Probe.icns"), icns());
  return directory;
}

function tree(directory, prefix = "") {
  return readdirSync(directory, { withFileTypes: true }).flatMap(entry => {
    const path = prefix + entry.name;
    return entry.isDirectory() && !entry.isSymbolicLink() ? [path + "/", ...tree(join(directory, entry.name), path + "/")] : [path];
  }).sort();
}

function signature(path) {
  // codesign writes the description to the standard error.
  return must("codesign", ["-dv", "--verbose=4", path]).stderr;
}

function verified(bundle) {
  must("codesign", ["--verify", "--deep", "--strict", "--verbose=2", bundle]);
}

function infoPlist(bundle, json = true) {
  const path = join(bundle, "Contents", "Info.plist");
  must("plutil", ["-lint", path]);
  const bytes = readFileSync(path);
  // The bytes that the compiler wrote are the bytes that plutil writes for the same dictionary.
  const canonical = spawnSync("plutil", ["-convert", "xml1", "-o", "-", path], { encoding: "buffer" });
  assert.equal(canonical.status, 0);
  assert.equal(bytes.toString("utf8"), canonical.stdout.toString("utf8"));
  // JSON has no date and no data, so a property list with them is read key by key.
  if (!json) return key => must("plutil", ["-extract", key, "raw", "-o", "-", path]).stdout.replace(/\n$/, "");
  return JSON.parse(must("plutil", ["-convert", "json", "-o", "-", path]).stdout);
}

function uuids(...paths) {
  return [...must("dwarfdump", ["--uuid", ...paths]).stdout.matchAll(/UUID: ([0-9A-F-]+) /g)].map(match => match[1]);
}

function leftovers(directory) {
  return existsSync(directory) ? readdirSync(directory).filter(name => name.startsWith(".tsuzuri-")) : [];
}

try {
  if (runs("bundle")) {
    for (const level of ["-O0", "-O3"]) {
      // E1: the bundle of the example.
      const source = project();
      const out = join(scratch, `e1${level}`);
      const bundle = join(out, "Probe.app");
      must(compiler, ["build", source, "--emit", "app", level, "-o", bundle]);
      assert.deepEqual(tree(bundle), [
        "Contents/",
        "Contents/Info.plist",
        "Contents/MacOS/",
        "Contents/MacOS/Probe",
        "Contents/PkgInfo",
        "Contents/Resources/",
        "Contents/Resources/Probe.icns",
        "Contents/_CodeSignature/",
        "Contents/_CodeSignature/CodeResources",
      ]);
      assert.deepEqual(readdirSync(out), ["Probe.app"], "no sidecar and no temporary directory stays");
      assert.equal(readFileSync(join(bundle, "Contents", "PkgInfo"), "latin1"), "APPL????");
      assert.deepEqual(infoPlist(bundle), {
        CFBundleExecutable: "Probe",
        CFBundleIconFile: "Probe.icns",
        CFBundleIdentifier: "dev.example.probe",
        CFBundleInfoDictionaryVersion: "6.0",
        CFBundleName: "Probe",
        CFBundlePackageType: "APPL",
        CFBundleShortVersionString: "0.1.0",
        CFBundleVersion: "1",
        LSMinimumSystemVersion: "12.0",
        LSUIElement: true,
        NSHighResolutionCapable: true,
      });
      verified(bundle);
      const described = signature(bundle);
      assert.match(described, /^Identifier=dev\.example\.probe$/m);
      const flags = /flags=0x[0-9a-f]+\(([^)]*)\)/.exec(described)[1].split(",");
      assert.ok(flags.includes("adhoc") && !flags.includes("linker-signed"), described);
      assert.doesNotMatch(described, /Info\.plist=not bound/);
      // The executable inside is the executable of `--emit exe`: the same bytes before codesign signs it
      // again, so the same code, and it runs from the bundle.
      const program = join(bundle, "Contents", "MacOS", "Probe");
      assert.equal(statSync(program).mode & 0o777, 0o755);
      const ran = must(program, [], { cwd: scratch });
      assert.equal(ran.stdout, `cwd=${scratch}\nexe=${program}\n`);
      // The minimum version is the one that the linker recorded.
      assert.match(must("otool", ["-l", program]).stdout, /minos 12\.0/);

      // E2: `-g` gives the dSYM beside the bundle, and no flat `.dwarf`.
      const debug = join(scratch, `e2${level}`);
      const debugBundle = join(debug, "Probe.app");
      const dsym = `${debugBundle}.dSYM`;
      must(compiler, ["build", source, "--emit", "app", level, "-g", "-o", debugBundle]);
      assert.deepEqual(readdirSync(debug).sort(), ["Probe.app", "Probe.app.dSYM"]);
      assert.deepEqual(tree(dsym), [
        "Contents/",
        "Contents/Info.plist",
        "Contents/Resources/",
        "Contents/Resources/DWARF/",
        "Contents/Resources/DWARF/Probe",
      ]);
      assert.ok(![...tree(debug)].some(path => path.endsWith(".dwarf")));
      verified(debugBundle);
      const debugProgram = join(debugBundle, "Contents", "MacOS", "Probe");
      const pair = uuids(dsym, debugProgram);
      assert.equal(pair.length, 2);
      assert.equal(pair[0], pair[1], "the dSYM belongs to the executable");
      const dsymInfo = JSON.parse(must("plutil", ["-convert", "json", "-o", "-", join(dsym, "Contents", "Info.plist")]).stdout);
      assert.deepEqual(dsymInfo, {
        CFBundleDevelopmentRegion: "English",
        CFBundleIdentifier: "com.apple.xcode.dsym.dev.example.probe",
        CFBundleInfoDictionaryVersion: "6.0",
        CFBundlePackageType: "dSYM",
        CFBundleShortVersionString: "0.1.0",
        CFBundleSignature: "????",
        CFBundleVersion: "1",
      });
      // lldb finds the dSYM by the UUID without `target symbols add`: the breakpoint has a source line.
      const lldb = run("lldb", ["--batch", "-o", `target create ${JSON.stringify(debugBundle)}`, "-o", "breakpoint set -n main"]);
      assert.equal(lldb.status, 0, lldb.stderr);
      assert.match(lldb.stdout, /(Main|Env|IO)\.tz:\d+/, lldb.stdout + lldb.stderr);
      // A build without `-g` replaces the bundle and leaves the dSYM alone.
      const before = statSync(join(dsym, "Contents", "Info.plist")).mtimeMs;
      must(compiler, ["build", source, "--emit", "app", level, "-o", debugBundle]);
      assert.equal(statSync(join(dsym, "Contents", "Info.plist")).mtimeMs, before);
      verified(debugBundle);
      // A second `-g` build replaces both, and they still belong together.
      must(compiler, ["build", source, "--emit", "app", level, "-g", "-o", debugBundle]);
      const again = uuids(dsym, debugProgram);
      assert.equal(again[0], again[1]);
      assert.deepEqual(leftovers(debug), []);
    }

    // The bundle keys do not change `--emit exe`: the executable is the same with and without them.
    {
      // Two links of one program differ in LC_UUID and in the signature over it, so the object is compared
      // byte for byte, and the executable by its code and data.
      const withKeys = project("");
      const sections = path => must("otool", ["-t", "-d", path]).stdout.replace(/^.*:\n/, "");
      const build = name => {
        must(compiler, ["build", withKeys, "-O0", "--no-cache", "--emit", "object", "-o", join(scratch, `${name}.o`)]);
        must(compiler, ["build", withKeys, "-O0", "--no-cache", "-o", join(scratch, name)]);
        return [readFileSync(join(scratch, `${name}.o`)), sections(join(scratch, name))];
      };
      const [plainObject, plainSections] = build("exe-without");
      writeFileSync(join(withKeys, "Tsuzuri.toml"), PACKAGE + PROBE_MACOS);
      const [keyedObject, keyedSections] = build("exe-with");
      assert.ok(plainObject.equals(keyedObject));
      assert.equal(plainSections, keyedSections);
      assert.ok(plainSections.length > 1000);
      // Without `-o` the bundle is named after the entry file.
      must(compiler, ["build", withKeys, "--emit", "app", "-O0"]);
      assert.ok(existsSync(join(withKeys, "Main.app", "Contents", "MacOS", "Main")));
      verified(join(withKeys, "Main.app"));
    }

    // E3: resources are copied by content and mode only; extended attributes stay behind.
    {
      const source = project(PROBE_MACOS + 'resources = ["assets/help", "assets/tagged.txt", "assets/tool.sh"]\n');
      const help = join(source, "assets", "help");
      mkdirSync(join(help, "en.lproj"), { recursive: true });
      writeFileSync(join(help, "index.html"), "<p>help</p>\n");
      writeFileSync(join(help, "en.lproj", "Strings.strings"), '"k" = "v";\n');
      symlinkSync("en.lproj/Strings.strings", join(help, "Current.strings"));
      const tagged = join(source, "assets", "tagged.txt");
      writeFileSync(tagged, "tagged\n");
      must("xattr", ["-wx", "com.apple.FinderInfo", "00".repeat(8) + "01" + "00".repeat(23), tagged]);
      must("xattr", ["-w", "com.apple.quarantine", "0081;00000000;Tsuzuri;", tagged]);
      must("xattr", ["-wx", "com.apple.FinderInfo", "00".repeat(8) + "01" + "00".repeat(23), join(help, "index.html")]);
      const tool = join(source, "assets", "tool.sh");
      writeFileSync(tool, "#!/bin/sh\nexit 0\n");
      chmodSync(tool, 0o750);
      const bundle = join(scratch, "e3", "Probe.app");
      must(compiler, ["build", source, "--emit", "app", "-O0", "-o", bundle]);
      verified(bundle);
      const resources = join(bundle, "Contents", "Resources");
      assert.deepEqual(tree(resources), [
        "Probe.icns", "help/", "help/Current.strings", "help/en.lproj/", "help/en.lproj/Strings.strings", "help/index.html", "tagged.txt", "tool.sh",
      ]);
      assert.equal(readFileSync(join(resources, "tagged.txt"), "utf8"), "tagged\n");
      for (const copy of [join(resources, "tagged.txt"), join(resources, "help", "index.html")]) {
        for (const attribute of ["com.apple.FinderInfo", "com.apple.quarantine"]) {
          assert.notEqual(run("xattr", ["-p", attribute, copy]).status, 0, `${attribute} on ${copy}`);
        }
      }
      // The source keeps its attributes: the copy did not move them.
      must("xattr", ["-p", "com.apple.FinderInfo", tagged]);
      assert.equal(statSync(join(resources, "tool.sh")).mode & 0o777, 0o755);
      assert.equal(statSync(join(resources, "tagged.txt")).mode & 0o777, 0o644);
      assert.ok(lstatSync(join(resources, "help", "Current.strings")).isSymbolicLink());
      assert.equal(readlinkSync(join(resources, "help", "Current.strings")), "en.lproj/Strings.strings");
      // A link that leaves the copied directory is not copied.
      symlinkSync("../tagged.txt", join(help, "Outside.txt"));
      const message = rejected(["build", source, "--emit", "app", "-O0", "-o", bundle], "E2001", "macos_bundle_input_invalid");
      assert.match(message, /symbolic link 'assets\/help\/Outside\.txt' points outside 'assets\/help'/);
      // The failed build left the bundle of the build before.
      verified(bundle);
      assert.ok(!existsSync(join(resources, "help", "Outside.txt")));
    }

    // E4 and E7: embedded code is signed from the inside, and `--rpath` lets the executable load it.
    {
      const library = join(scratch, "native");
      mkdirSync(library);
      writeFileSync(join(library, "lib.c"), "int m15_value(void) { return 7; }\n");
      const dylib = join(library, "libm15.dylib");
      must("clang", ["-dynamiclib", "-install_name", "@rpath/libm15.dylib", join(library, "lib.c"), "-o", dylib]);
      const main = '@trusted\nextern "m15_value" def m15_value :: unit -> i32\n\ndef main :: unit -> i32 = \\() -> m15_value ()\n';
      // The framework lives beside the project, not in it: module discovery walks every directory of a project and
      // refuses the directory links that a framework has (E1011), so `embed` names it with `..`.
      const source = project('[macos]\nbundle_id = "dev.example.embed"\nembed = ["../vendor/libm15.dylib", "../vendor/M15.framework"]\n', main);
      const vendor = join(scratch, "vendor");
      const framework = join(vendor, "M15.framework");
      mkdirSync(join(framework, "Versions", "A", "Resources"), { recursive: true });
      must("clang", ["-dynamiclib", "-install_name", "@rpath/M15.framework/Versions/A/M15", join(library, "lib.c"), "-o", join(framework, "Versions", "A", "M15")]);
      writeFileSync(join(framework, "Versions", "A", "Resources", "Info.plist"), `<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>M15</string>
<key>CFBundleIdentifier</key><string>dev.example.m15framework</string>
<key>CFBundlePackageType</key><string>FMWK</string>
<key>CFBundleVersion</key><string>1</string>
</dict></plist>
`);
      symlinkSync("A", join(framework, "Versions", "Current"));
      symlinkSync("Versions/Current/M15", join(framework, "M15"));
      symlinkSync("Versions/Current/Resources", join(framework, "Resources"));
      must("clang", ["-dynamiclib", "-install_name", "@rpath/libm15.dylib", join(library, "lib.c"), "-o", join(vendor, "libm15.dylib")]);
      // Neither is signed: the linker's signature is taken away.
      for (const code of [join(vendor, "libm15.dylib"), join(framework, "Versions", "A", "M15")]) {
        must("codesign", ["--remove-signature", code]);
        assert.notEqual(run("codesign", ["-dv", code]).status, 0);
      }
      for (const level of ["-O0", "-O3"]) {
        const bundle = join(scratch, `e4${level}`, "Embed.app");
        must(compiler, ["build", source, "--emit", "app", level, "--link", dylib, "--rpath", "@executable_path/../Frameworks", "-o", bundle]);
        verified(bundle);
        const frameworks = join(bundle, "Contents", "Frameworks");
        for (const nested of [join(frameworks, "libm15.dylib"), join(frameworks, "M15.framework")]) {
          const described = signature(nested);
          assert.match(described, /flags=0x[0-9a-f]+\(adhoc\)/, described);
        }
        assert.ok(lstatSync(join(frameworks, "M15.framework", "M15")).isSymbolicLink());
        assert.equal(readlinkSync(join(frameworks, "M15.framework", "Versions", "Current")), "A");
        // E7: dyld finds the library of `@rpath` in Contents/Frameworks.
        const ran = run(join(bundle, "Contents", "MacOS", "Embed"), []);
        assert.equal(ran.status, 7, ran.stderr);
      }
      // Something that is not code does not go to Contents/Frameworks.
      writeFileSync(join(vendor, "notes.txt"), "notes\n");
      writeFileSync(join(source, "Tsuzuri.toml"), PACKAGE + '[macos]\nbundle_id = "dev.example.embed"\nembed = ["../vendor/notes.txt"]\n');
      const message = rejected(["build", source, "--emit", "app", "-O0", "--link", dylib, "-o", join(scratch, "e4-notes", "Embed.app")], "E2001", "macos_bundle_input_invalid");
      assert.equal(message, "'../vendor/notes.txt' in embed is neither a Mach-O file nor a .framework directory; put it in resources");
      mkdirSync(join(vendor, "Data"));
      writeFileSync(join(source, "Tsuzuri.toml"), PACKAGE + '[macos]\nbundle_id = "dev.example.embed"\nembed = ["../vendor/Data"]\n');
      rejected(["build", source, "--emit", "app", "-O0", "--link", dylib, "-o", join(scratch, "e4-notes", "Embed.app")], "E2001", "macos_bundle_input_invalid");
      assert.ok(!existsSync(join(scratch, "e4-notes", "Embed.app")), "a failed build leaves no bundle");
      assert.deepEqual(leftovers(join(scratch, "e4-notes")), []);
    }

    // E5: what may be replaced, and what a failure leaves.
    {
      const source = project();
      const out = join(scratch, "e5");
      const bundle = join(out, "Probe.app");
      must(compiler, ["build", source, "--emit", "app", "-O0", "-o", bundle]);
      const stale = join(bundle, "Contents", "Resources", "stale.txt");
      writeFileSync(stale, "stale\n");
      must(compiler, ["build", source, "--emit", "app", "-O0", "-o", bundle]);
      assert.ok(!existsSync(stale), "the second build replaces the whole bundle");
      verified(bundle);

      // A bundle of another identifier, and of another executable name, is not ours.
      const other = project(PROBE_MACOS.replace("dev.example.probe", "dev.example.other"));
      const kept = () => readFileSync(join(bundle, "Contents", "Info.plist"), "utf8");
      const info = kept();
      let message = rejected(["build", other, "--emit", "app", "-O0", "-o", bundle], "E2003", "app_output_not_owned");
      assert.equal(message, "'Probe.app' exists and is not a bundle that tsuzuri built for 'dev.example.other'; remove it or choose another output");
      assert.equal(kept(), info);
      verified(bundle);
      // A directory without an Info.plist, a file, and a symbolic link are not ours either.
      const plain = join(out, "Plain.app");
      mkdirSync(join(plain, "Contents"), { recursive: true });
      writeFileSync(join(plain, "Contents", "precious.txt"), "precious\n");
      rejected(["build", source, "--emit", "app", "-O0", "-o", plain], "E2003", "app_output_not_owned");
      assert.equal(readFileSync(join(plain, "Contents", "precious.txt"), "utf8"), "precious\n");
      const file = join(out, "File.app");
      writeFileSync(file, "a file\n");
      rejected(["build", source, "--emit", "app", "-O0", "-o", file], "E2003", "app_output_not_owned");
      assert.equal(readFileSync(file, "utf8"), "a file\n");
      const link = join(out, "Link.app");
      symlinkSync("Probe.app", link);
      rejected(["build", source, "--emit", "app", "-O0", "-o", link], "E2003", "app_output_not_owned");
      assert.ok(lstatSync(link).isSymbolicLink());
      // The dSYM of another bundle is not replaced, and the bundle stays as it was.
      const foreign = `${bundle}.dSYM`;
      mkdirSync(join(foreign, "Contents"), { recursive: true });
      writeFileSync(join(foreign, "Contents", "Info.plist"), readFileSync(join(root, "tests", "fixtures", "macos_bundle", "Dsym.plist"), "utf8").replace("dev.example.probe", "dev.example.elsewhere"));
      message = rejected(["build", source, "--emit", "app", "-O0", "-g", "-o", bundle], "E2003", "app_output_not_owned");
      assert.match(message, /^'Probe\.app\.dSYM' exists and is not the dSYM of 'dev\.example\.probe'/);
      assert.equal(kept(), info);
      rmSync(foreign, { recursive: true });

      // A codesign that fails: the bundle of the build before stays, and nothing else is left.
      const fake = join(scratch, "fake-bin");
      mkdirSync(fake);
      writeFileSync(join(fake, "codesign"), "#!/bin/sh\necho 'codesign: refused by the test' >&2\nexit 1\n");
      chmodSync(join(fake, "codesign"), 0o755);
      const program = readFileSync(join(bundle, "Contents", "MacOS", "Probe"));
      writeFileSync(join(source, "Main.tz"), PROBE.replace('"cwd="', '"directory="'));
      message = rejected(["build", source, "--emit", "app", "-O0", "-o", bundle], "E2002", "external_tool_failed", { env: { ...baseEnv, PATH: `${fake}:${baseEnv.PATH}` } });
      assert.match(message, /refused by the test/);
      assert.match(message, /signed with codesign from \/usr\/bin or the Xcode Command Line Tools/);
      assert.ok(readFileSync(join(bundle, "Contents", "MacOS", "Probe")).equals(program));
      assert.equal(kept(), info);
      verified(bundle);
      assert.deepEqual(leftovers(out), []);
      // The same build with the real codesign replaces it.
      must(compiler, ["build", source, "--emit", "app", "-O0", "-o", bundle]);
      assert.match(must(join(bundle, "Contents", "MacOS", "Probe"), [], { cwd: scratch }).stdout, /^directory=/);

      // `--trap-info` writes its table beside the bundle, not into it.
      must(compiler, ["build", source, "--emit", "app", "-O0", "--trap-info", "-o", bundle]);
      assert.ok(existsSync(`${bundle}.trap.json`));
      JSON.parse(readFileSync(`${bundle}.trap.json`, "utf8"));
      verified(bundle);
    }

    // E9: `[macos] min_version` reaches Info.plist through the executable.
    {
      const source = project(PROBE_MACOS + 'min_version = "13.0"\n');
      const bundle = join(scratch, "e9", "Probe.app");
      must(compiler, ["build", source, "--emit", "app", "-O0", "-o", bundle]);
      assert.equal(infoPlist(bundle).LSMinimumSystemVersion, "13.0");
      assert.match(must("otool", ["-l", join(bundle, "Contents", "MacOS", "Probe")]).stdout, /minos 13\.0/);
      // `info_plist` adds its keys, and the whole is still plutil's form.
      writeFileSync(join(source, "Extra.plist"), readFileSync(join(root, "tests", "fixtures", "macos_bundle", "Extra.source.plist")));
      writeFileSync(join(source, "Tsuzuri.toml"), PACKAGE + PROBE_MACOS + 'info_plist = "Extra.plist"\n');
      must(compiler, ["build", source, "--emit", "app", "-O0", "-o", bundle]);
      const info = infoPlist(bundle, false);
      assert.equal(info("NSHumanReadableCopyright"), "Copyright © 2026 Example & Sons <dev@example.invalid>");
      assert.equal(info("NSAppTransportSecurity.NSAllowsLocalNetworking"), "true");
      assert.equal(info("CFBundleIdentifier"), "dev.example.probe");
      assert.equal(info("LSMinimumSystemVersion"), "12.0");
      assert.equal(info("1.deep.one"), "AQ==");
      assert.equal(info("Z"), "2024-02-29T23:59:59Z");
      verified(bundle);
    }

    // E10: the errors.
    {
      const source = project();
      const out = join(scratch, "e10");
      const bundle = join(out, "Probe.app");
      const app = ["build", source, "--emit", "app", "-O0"];
      assert.equal(
        rejected([...app, "-o", join(out, "Probe")], "E2000", "output_extension_invalid"),
        "app output must end with '.app'; its file name, without the extension, names the executable",
      );
      rejected([...app, "-o", join(out, ".app")], "E2000", "output_extension_invalid");
      assert.equal(
        rejected([...app, "--target", "wasm32", "-o", bundle], "E2000", "option_unsupported_platform"),
        "'--emit app' builds a macOS application; build for aarch64-apple-darwin or remove it",
      );
      rejected([...app, "--target", "x86_64-unknown-linux-gnu", "-o", bundle], "E2000", "option_unsupported_platform");
      // The rules of the executable inside are the rules of `--emit exe`.
      assert.match(rejected([...app, "--trap-mode", "return", "-o", bundle], "E2000", "option_requires_build"), /trap-mode/);
      rejected(["run", source, "--emit", "app"], "E2000", "option_invalid_for_command");
      // No bundle identifier: in the manifest, and without a manifest.
      const anonymous = project("");
      assert.equal(
        rejected(["build", anonymous, "--emit", "app", "-O0", "-o", bundle], "E2000", "macos_bundle_id_missing"),
        "'--emit app' needs [macos] bundle_id in the root Tsuzuri.toml",
      );
      const single = join(scratch, "single");
      mkdirSync(single);
      writeFileSync(join(single, "Main.tz"), "def main :: unit -> i32 = \\() -> 0i32\n");
      rejected(["build", join(single, "Main.tz"), "--emit", "app", "-O0", "-o", bundle], "E2000", "macos_bundle_id_missing");
      // The inputs.
      const withExtra = text => {
        const directory = project(PROBE_MACOS + 'info_plist = "Extra.plist"\n');
        writeFileSync(join(directory, "Extra.plist"), text);
        return directory;
      };
      const plist = body => `<?xml version="1.0" encoding="UTF-8"?>\n<plist version="1.0">\n${body}\n</plist>\n`;
      assert.equal(
        rejected(["build", withExtra(plist("<dict><key>CFBundleIdentifier</key><string>x.y</string></dict>")), "--emit", "app", "-O0", "-o", bundle], "E2000", "macos_bundle_conflict"),
        "Info.plist key 'CFBundleIdentifier' comes from both [macos] bundle_id and 'Extra.plist'; remove one",
      );
      assert.match(
        rejected(["build", withExtra(plist("<array/>")), "--emit", "app", "-O0", "-o", bundle], "E2001", "macos_bundle_input_invalid"),
        /^'Extra\.plist' is not a property list that info_plist can add: its top-level value must be a <dict>$/,
      );
      const binary = withExtra(plist("<dict><key>k</key><string>v</string></dict>"));
      must("plutil", ["-convert", "binary1", join(binary, "Extra.plist")]);
      assert.match(
        rejected(["build", binary, "--emit", "app", "-O0", "-o", bundle], "E2001", "macos_bundle_input_invalid"),
        /plutil -convert xml1/,
      );
      const noIcon = project();
      rmSync(join(noIcon, "assets", "Probe.icns"));
      assert.match(
        rejected(["build", noIcon, "--emit", "app", "-O0", "-o", bundle], "E2001", "io_failure"),
        /^cannot read macOS bundle input 'assets\/Probe\.icns': /,
      );
      const badIcon = project();
      writeFileSync(join(badIcon, "assets", "Probe.icns"), "\x89PNG\r\n\x1a\n");
      assert.equal(
        rejected(["build", badIcon, "--emit", "app", "-O0", "-o", bundle], "E2001", "macos_bundle_input_invalid"),
        "'assets/Probe.icns' is not an icns file: expected the 'icns' magic and its length",
      );
      assert.ok(!existsSync(out) || readdirSync(out).length === 0, "no failed build wrote anything");
      // A manifest value of the wrong form is an error of every build, as `[windows] subsystem` is.
      const malformed = project('[macos]\nbundle_id = "not a bundle id"\n');
      rejected(["build", malformed, "-O0", "-o", join(scratch, "malformed")], "E0002", "invalid_macos_setting");
      // A bundle that holds the sources of the build is not replaced.
      const inside = join(scratch, "Inside.app");
      mkdirSync(join(inside, "Contents"), { recursive: true });
      writeFileSync(join(inside, "Tsuzuri.toml"), PACKAGE + '[macos]\nbundle_id = "dev.example.inside"\n');
      writeFileSync(join(inside, "Main.tz"), "def main :: unit -> i32 = \\() -> 0i32\n");
      writeFileSync(join(inside, "Contents", "Info.plist"), plist("<dict><key>CFBundleIdentifier</key><string>dev.example.inside</string><key>CFBundleExecutable</key><string>Inside</string></dict>"));
      rejected(["build", inside, "--emit", "app", "-O0", "-o", inside], "E2003", "output_overwrites_input");
      assert.ok(existsSync(join(inside, "Main.tz")));
    }
    console.log("macOS application bundles: bundle stage passed");
  }

  if (runs("cache")) {
    // E6: the executable is cached like `--emit exe`; the bundle is assembled every time.
    const cache = join(scratch, "cache-e6");
    const env = { ...baseEnv, TSUZURI_CACHE_DIR: cache };
    const entries = () => (existsSync(cache) ? readdirSync(cache).filter(name => /^[0-9a-f]{64}$/.test(name)) : []);
    const source = project(PROBE_MACOS + 'resources = ["assets/data.txt"]\n');
    writeFileSync(join(source, "assets", "data.txt"), "one\n");
    const out = join(scratch, "e6");
    const bundle = join(out, "Probe.app");
    must(compiler, ["build", source, "-O0", "-o", join(out, "probe")], { env });
    assert.equal(entries().length, 1);
    // `--emit app` after `--emit exe` uses the same entry.
    must(compiler, ["build", source, "--emit", "app", "-O0", "-o", bundle], { env });
    assert.equal(entries().length, 1);
    const info = join(bundle, "Contents", "Info.plist");
    const first = lstatSync(info, { bigint: true });
    must(compiler, ["build", source, "--emit", "app", "-O0", "-o", bundle], { env });
    assert.equal(entries().length, 1);
    const second = lstatSync(info, { bigint: true });
    assert.ok(second.ino !== first.ino || second.mtimeNs > first.mtimeNs, "the bundle is built again on a cache hit");
    verified(bundle);
    // A resource is not part of the key: the entry stays, and the bundle has the new bytes.
    writeFileSync(join(source, "assets", "data.txt"), "two\n");
    must(compiler, ["build", source, "--emit", "app", "-O0", "-o", bundle], { env });
    assert.equal(entries().length, 1);
    assert.equal(readFileSync(join(bundle, "Contents", "Resources", "data.txt"), "utf8"), "two\n");
    verified(bundle);
    // `-g`: the key holds the final path, so the second build hits, and the pair stays one build.
    const debug = join(out, "Debug.app");
    must(compiler, ["build", source, "--emit", "app", "-O0", "-g", "-o", debug], { env });
    assert.equal(entries().length, 2);
    const program = join(debug, "Contents", "MacOS", "Debug");
    const built = uuids(`${debug}.dSYM`, program);
    must(compiler, ["build", source, "--emit", "app", "-O0", "-g", "-o", debug], { env });
    assert.equal(entries().length, 2, "the -g build of the same output is a cache hit");
    assert.deepEqual(uuids(`${debug}.dSYM`, program), built);
    assert.equal(built[0], built[1]);
    verified(debug);
    console.log("macOS application bundles: cache stage passed");
  }

  if (runs("open")) {
    if (process.env.TSUZURI_MACOS_BUNDLE_NO_OPEN) {
      console.log("macOS application bundles: open stage skipped (TSUZURI_MACOS_BUNDLE_NO_OPEN)");
    } else {
      // E8: LaunchServices starts the bundle with the working directory `/`.
      for (const level of ["-O0", "-O3"]) {
        const source = project();
        const out = join(scratch, `e8${level}`);
        const bundle = join(out, "Probe.app");
        must(compiler, ["build", source, "--emit", "app", level, "-o", bundle]);
        const stdout = join(out, "stdout.txt");
        const stderr = join(out, "stderr.txt");
        // `open -W` cannot wait for an application that ends at once, so the output file is what is waited for.
        const opened = run("perl", ["-e", "alarm 15; exec @ARGV", "open", "-n", "--stdout", stdout, "--stderr", stderr, bundle]);
        assert.equal(opened.status, 0, opened.stderr);
        const program = join(bundle, "Contents", "MacOS", "Probe");
        const expected = `cwd=/\nexe=${program}\n`;
        const deadline = Date.now() + 10000;
        let text = "";
        while (Date.now() < deadline) {
          text = existsSync(stdout) ? readFileSync(stdout, "utf8") : "";
          if (text === expected) break;
          spawnSync("sleep", ["0.1"]);
        }
        assert.equal(text, expected);
        // The application has ended: nothing of it runs any more.
        let running = "";
        for (let attempt = 0; attempt < 50; attempt++) {
          running = run("pgrep", ["-f", program]).stdout.trim();
          if (running === "") break;
          spawnSync("sleep", ["0.1"]);
        }
        assert.equal(running, "", "the application left a process");
      }
      console.log("macOS application bundles: open stage passed");
    }
  }
} finally {
  rmSync(scratch, { recursive: true, force: true });
}
console.log("macOS application bundles: passed");
