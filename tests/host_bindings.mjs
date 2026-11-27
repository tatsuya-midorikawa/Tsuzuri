// End-to-end test of `--emit shared` and the C#, Python and C++ bindings of E13 Phase 2 on the native
// target: the library exports only the public C ABI and links its extern imports from --link, each
// binding passes the same calls (scalars, buffers, strings, records, fixed arrays, handles), owned
// results leave no allocation behind (--allocator counting), a trap ends the process, and with
// --trap-mode return a trap becomes an exception of the host language. The headers of two libraries
// are read in one C or C++ translation unit and both libraries are called from it.
// usage: node tests/host_bindings.mjs target/release/tsuzuri
// Python 3 (PYTHON or python3), clang++ (TSUZURI_CLANGXX or clang++) and dotnet 8+ run when present.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { copyFileSync, cpSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repository = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const clang = process.env.TSUZURI_CLANG ?? "clang";
const clangxx = process.env.TSUZURI_CLANGXX ?? "clang++";
const python = process.env.PYTHON ?? "python3";
const root = mkdtempSync(join(tmpdir(), "tsuzuri-host-bindings-"));
const fixture = join(root, "fixture");
cpSync(join(repository, "tests/fixtures/bindings_native"), fixture, { recursive: true });
const extension = process.platform === "darwin" ? "dylib" : "so";

function execute(program, args, options = {}) {
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 600_000, maxBuffer: 16 * 1024 * 1024, ...options });
  if (options.allowFailure) return result;
  assert.ifError(result.error);
  assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}
function available(program, args) {
  const result = spawnSync(program, args, { encoding: "utf8" });
  return result.status === 0 ? result.stdout.trim() : null;
}
function rejected(args, code, status, message) {
  const result = execute(compiler, args, { allowFailure: true });
  assert.equal(result.status, status, `${args.join(" ")}\n${result.stderr}`);
  assert.match(result.stderr, new RegExp(`error\\[${code}\\]`), result.stderr);
  if (message) assert.ok(result.stderr.includes(message), `${args.join(" ")}\n${result.stderr}`);
}
// The defined external symbols of a shared library.
function exported(library) {
  const listing = process.platform === "darwin" ? execute("nm", ["-gU", library]).stdout : execute("nm", ["-D", "--defined-only", library]).stdout;
  return listing.split("\n").map((line) => line.trim().split(/\s+/).at(-1)).filter(Boolean)
    .map((name) => (process.platform === "darwin" ? name.replace(/^_/, "") : name)).sort();
}

const exportNames = ["add", "add_u64", "area", "check", "checksum", "copy_text", "copy_utf8", "copy_values", "counters", "divide", "make_bytes", "make_window", "narrow", "negate", "pass_through", "peek", "scaled", "sum_float", "twice32", "update", "widen", "window_total"];

try {
  const host = join(root, "host.o");
  execute(clang, ["-c", "-fPIC", "-O2", join(repository, "tests/host_bindings_host.c"), "-o", host]);
  // The bindings come from the sources alone and are byte-identical every time.
  const directories = {};
  for (const kind of ["py", "cpp", "cs"]) mkdirSync((directories[kind] = join(root, kind)));
  mkdirSync(join(root, "again"));
  const bindings = (kind, output, ...flags) => {
    // The file name names the library, so the second copy keeps it in another directory.
    const again = join(root, "again", basename(output));
    for (const path of [output, again]) execute(compiler, ["build", fixture, "--emit", `bindings-${kind}`, ...flags, "-o", path]);
    assert.deepEqual(readFileSync(output), readFileSync(again), `${kind} bindings are deterministic`);
  };
  bindings("py", join(directories.py, "native.py"));
  bindings("py", join(directories.py, "native_trap.py"), "--trap-mode", "return");
  bindings("cpp", join(directories.cpp, "native.hpp"));
  bindings("cpp", join(directories.cpp, "native_trap.hpp"), "--trap-mode", "return");
  bindings("cs", join(directories.cs, "native.cs"));
  bindings("cs", join(directories.cs, "native_trap.cs"), "--trap-mode", "return");
  execute(compiler, ["build", fixture, "--emit", "header", "--allocator", "counting", "-o", join(directories.cpp, "native.h")]);
  execute(compiler, ["build", fixture, "--emit", "header", "--trap-mode", "return", "-o", join(directories.cpp, "native_trap.h")]);

  // The library of --trap-mode return, and its trap side table.
  const trapLibrary = join(root, `libnative_trap.${extension}`);
  execute(compiler, ["build", fixture, "--emit", "shared", "--trap-mode", "return", "--link", host, "-o", trapLibrary]);
  assert.ok(existsSync(`${trapLibrary}.trap.json`));
  assert.deepEqual(exported(trapLibrary), [...exportNames.map((name) => `tsuzuri_try_${name}`), "tsuzuri_alloc", "tsuzuri_free", ...exportNames.map((name) => `tz_${name}`)].sort());

  const pythonVersion = available(python, ["--version"]);
  const clangxxVersion = available(clangxx, ["--version"])?.split("\n")[0];
  for (const optimization of ["-O0", "-O3"]) {
    // The file name is the library's install name (soname), which hosts record when they link it.
    mkdirSync(join(root, optimization));
    const library = join(root, optimization, `libnative.${extension}`);
    execute(compiler, ["build", fixture, "--emit", "shared", "--allocator", "counting", "--link", host, optimization, "-o", library]);
    // Only the public C ABI leaves the library; the runtime and the host's own functions stay inside.
    assert.deepEqual(exported(library), ["tsuzuri_alloc", "tsuzuri_alloc_stats", "tsuzuri_free", ...exportNames.map((name) => `tz_${name}`)].sort());
    if (pythonVersion) {
      // The bindings find lib<name> next to themselves when load() has no path.
      copyFileSync(library, join(directories.py, `libnative.${extension}`));
      copyFileSync(trapLibrary, join(directories.py, `libnative_trap.${extension}`));
      execute(python, [join(repository, "tests/host_bindings_test.py"), join(directories.py, "native.py"), join(directories.py, `libnative.${extension}`), join(directories.py, "native_trap.py"), join(directories.py, `libnative_trap.${extension}`), `${trapLibrary}.trap.json`]);
      execute(python, ["-c", `import sys; sys.path.insert(0, ${JSON.stringify(directories.py)}); import native; assert native.load().add(40, 2) == 42`]);
    }
    if (clangxxVersion) {
      const program = join(root, `cpp${optimization}`);
      copyFileSync(library, join(directories.cpp, `libnative.${extension}`));
      execute(clangxx, ["-std=c++20", "-Wall", "-Wextra", "-Werror", optimization, join(repository, "tests/host_bindings_test.cpp"), "-I", directories.cpp, join(directories.cpp, `libnative.${extension}`), `-Wl,-rpath,${directories.cpp}`, "-o", program]);
      execute(program, []);
      // A trap ends the process when the library was not built with --trap-mode return.
      const trapped = execute(program, ["trap"], { allowFailure: true });
      assert.ok(trapped.status !== 0, `the trap ended the process: ${trapped.status} ${trapped.signal}`);
    }
    console.log(`host bindings: ${optimization} passed (${[pythonVersion && "Python", clangxxVersion && "C++"].filter(Boolean).join(", ") || "no host language found"})`);
  }
  if (clangxxVersion) {
    const program = join(root, "cpp-trap");
    copyFileSync(trapLibrary, join(directories.cpp, `libnative_trap.${extension}`));
    execute(clangxx, ["-std=c++20", "-Wall", "-Wextra", "-Werror", "-O2", "-DTRAP_MODE", join(repository, "tests/host_bindings_test.cpp"), "-I", directories.cpp, join(directories.cpp, `libnative_trap.${extension}`), `-Wl,-rpath,${directories.cpp}`, "-o", program]);
    execute(program, []);
    console.log(`host bindings: C++ --trap-mode return passed (${clangxxVersion})`);
  } else {
    console.log("host bindings: C++ skipped (no clang++)");
  }
  if (!pythonVersion) console.log("host bindings: Python skipped (no python3)");

  // Two libraries in one translation unit: every header carries the shared declarations behind one
  // guard, and liba and libb both export the record of the package they depend on, which stands behind
  // a guard of its own. C11, C++17 and the C++20 bindings of both read the headers together and call
  // both libraries; libb is built with --trap-mode return.
  const two = join(root, "two");
  const twoOut = join(two, "out");
  for (const directory of ["common", "liba", "libb", "out"]) mkdirSync(join(two, directory), { recursive: true });
  writeFileSync(join(two, "common", "Tsuzuri.toml"), '[package]\nname = "common"\nversion = "0.1.0"\n');
  writeFileSync(join(two, "common", "Geo.tz"), "record Point { x: f64, y: f64 }\n");
  for (const name of ["liba", "libb"]) {
    writeFileSync(join(two, name, "Tsuzuri.toml"), `[package]\nname = "${name}"\nversion = "0.1.0"\n\n[dependencies]\ncommon = { path = "../common" }\n`);
  }
  writeFileSync(join(two, "liba", "Lib.tz"), "export def coordinate_sum :: ref Common::Geo.Point -> f64\nfn coordinate_sum point = point.x + point.y\n\nexport def greet :: ref string -> string\nfn greet name = clone_string name\n");
  writeFileSync(join(two, "libb", "Lib.tz"), "export def origin :: own i64 -> Common::Geo.Point\nfn origin x = Common::Geo.Point { x: x as f64, y: 0.5 }\n\nexport def total :: ref [i64] -> i64\nfn total values = Array.sum values\n\nexport def echo :: ref utf8string -> utf8string\nfn echo text = Utf8String.clone text\n\nexport def divide :: own i64 -> own i64 -> i64\nfn divide a b = a / b\n");
  const libraries = { liba: [join(two, "liba", "Lib.tz")], libb: [join(two, "libb", "Lib.tz"), "--trap-mode", "return"] };
  for (const [name, [source, ...flags]] of Object.entries(libraries)) {
    execute(compiler, ["build", source, "--emit", "header", ...flags, "-o", join(twoOut, `${name}.h`)]);
    execute(compiler, ["build", source, "--emit", "shared", ...flags, "-o", join(twoOut, `lib${name}.${extension}`)]);
    if (clangxxVersion) execute(compiler, ["build", source, "--emit", "bindings-cpp", ...flags, "-o", join(twoOut, `${name}.hpp`)]);
  }
  execute(compiler, ["build", libraries.libb[0], "--emit", "header", "--trap-mode", "return", "-o", join(two, "again.h")]);
  assert.deepEqual(readFileSync(join(two, "again.h")), readFileSync(join(twoOut, "libb.h")), "the header is deterministic");
  const twoHost = join(repository, "tests/host_bindings_two_libraries.c");
  const twoFlags = ["-Wall", "-Wextra", "-pedantic", "-Werror", "-I", twoOut];
  const twoLink = [join(twoOut, `libliba.${extension}`), join(twoOut, `liblibb.${extension}`), `-Wl,-rpath,${twoOut}`];
  const twoRuns = [[clang, "C11", ["-std=c11", ...twoFlags, twoHost]]];
  if (clangxxVersion) {
    twoRuns.push(
      [clangxx, "C++17", ["-std=c++17", ...twoFlags, "-x", "c++", twoHost, "-x", "none"]],
      [clangxx, "C++20 bindings", ["-std=c++20", "-DTSUZURI_CPP_BINDINGS", ...twoFlags, "-x", "c++", twoHost, "-x", "none"]],
    );
  }
  for (const [tool, label, args] of twoRuns) {
    const program = join(two, label.replace(/\W+/g, "_"));
    execute(tool, [...args, ...twoLink, "-o", program]);
    assert.match(execute(program, []).stdout, /^two libraries passed$/m, label);
  }
  console.log(`host bindings: two libraries in one translation unit passed (${twoRuns.map(([, label]) => label).join(", ")})`);

  // C#: one console project with both bindings, against both libraries.
  const dotnetVersion = available("dotnet", ["--version"]);
  const major = Number(dotnetVersion?.split(".")[0]);
  if (major >= 8) {
    const project = directories.cs;
    copyFileSync(join(repository, "tests/host_bindings_test.cs"), join(project, "Program.cs"));
    writeFileSync(join(project, "bindings.csproj"), `<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <TargetFramework>net${major}.0</TargetFramework>
    <AllowUnsafeBlocks>true</AllowUnsafeBlocks>
    <Nullable>enable</Nullable>
    <ImplicitUsings>disable</ImplicitUsings>
    <TreatWarningsAsErrors>true</TreatWarningsAsErrors>
  </PropertyGroup>
</Project>
`);
    const environment = { ...process.env, DOTNET_CLI_TELEMETRY_OPTOUT: "1", DOTNET_NOLOGO: "1", DOTNET_SKIP_FIRST_TIME_EXPERIENCE: "1" };
    // Freed memory is overwritten, so a buffer freed during a copy shows (macOS, glibc).
    const scribbled = { ...environment, MallocScribble: "1", MALLOC_PERTURB_: "85" };
    execute("dotnet", ["build", "-c", "Release", "-nologo", "-v", "q"], { cwd: project, env: environment });
    for (const optimization of ["-O0", "-O3"]) {
      const output = execute("dotnet", [join(project, "bin", "Release", `net${major}.0`, "bindings.dll"), join(root, optimization, `libnative.${extension}`), trapLibrary], { env: scribbled });
      assert.match(output.stdout, /^csharp bindings passed \(trap site \d+\)/);
    }
    console.log(`host bindings: C# passed (.NET SDK ${dotnetVersion})`);
  } else {
    console.log("host bindings: C# skipped (no .NET 8 or later SDK)");
  }

  // The build cache: a library records its file name as its install name (soname), so a build to
  // another name does not restore the cached library; the same name in another directory does.
  const cacheable = join(root, "cacheable");
  mkdirSync(cacheable);
  writeFileSync(join(cacheable, "Main.tz"), "export def add :: i64 -> i64 -> i64 = \\left right ->\n    left + right\n");
  const cache = join(root, "cache");
  const installName = (library) => (process.platform === "darwin"
    ? execute("otool", ["-D", library]).stdout.trim().split("\n").at(-1)
    : /\(SONAME\)\s+Library soname: \[(.+)\]/.exec(execute("readelf", ["-d", library]).stdout)?.[1]);
  for (const [directory, file, entries] of [["first", `libfoo.${extension}`, 1], ["second", `libbar.${extension}`, 2], ["third", `libfoo.${extension}`, 2]]) {
    const library = join(root, directory, file);
    execute(compiler, ["build", cacheable, "--emit", "shared", "-o", library], { env: { ...process.env, TSUZURI_CACHE_DIR: cache } });
    assert.equal(installName(library), process.platform === "darwin" ? `@rpath/${file}` : file, `${directory}/${file}`);
    assert.equal(readdirSync(cache).filter((name) => /^[0-9a-f]{64}$/.test(name)).length, entries, `cache entries after ${directory}/${file}`);
  }
  console.log("host bindings: shared library cache passed");

  // The command line.
  const out = join(root, "rejected");
  rejected(["build", fixture, "--emit", "shared", "--target", "wasm32", "-o", `${out}.so`], "E2000", 2, "'--emit shared' requires '--target native'");
  rejected(["build", fixture, "--emit", "bindings-cs", "--target", "wasm32", "-o", `${out}.cs`], "E2000", 2, "'--emit bindings-cs' requires '--target native'");
  rejected(["build", fixture, "--emit", "bindings-py", "-o", `${out}.txt`], "E2000", 2, "bindings output must end with '.py'; its file name, without the extension, names the shared library");
  rejected(["build", fixture, "--emit", "bindings-cpp", "-o", `${out}.h`], "E2000", 2, "bindings output must end with '.hpp'");
  rejected(["build", fixture, "--emit", "bindings-cpp", "--trap-info", "-o", `${out}.hpp`], "E2000", 2, "--trap-info is not valid for bindings output; pass it when building the shared library");
  rejected(["build", fixture, "--emit", "bindings-cs", "--allocator", "counting", "-o", `${out}.cs`], "E2000", 2, "--allocator is not valid for bindings output; pass it when building the shared library");
  rejected(["build", fixture, "--emit", "shared", "--allocator", "host", "-o", `${out}.${extension}`], "E2000", 2, "--allocator host cannot be combined with --emit shared");
  rejected(["build", fixture, "--emit", "bindings-py", "--cpu", "native", "-o", `${out}.py`], "E2000", 2, "'--cpu native' requires native executable or object output");
  // The extern imports of a shared library resolve when it is linked, as for an executable.
  rejected(["build", fixture, "--emit", "shared", "-o", `${out}.${extension}`], "E2002", 1);
  const empty = join(root, "empty");
  mkdirSync(empty);
  writeFileSync(join(empty, "Lib.tz"), "def one :: i64\nfn one = 1\n");
  rejected(["build", join(empty, "Lib.tz"), "--emit", "shared", "-o", `${out}.${extension}`], "E2004", 1, "a shared library needs at least one 'export def' entry point");
  rejected(["build", join(empty, "Lib.tz"), "--emit", "bindings-cs", "-o", `${out}.cs`], "E2004", 1, "bindings need at least one 'export def' entry point");
  for (const suffix of [`.${extension}`, ".cs", ".py", ".hpp", ".txt", ".h"]) assert.ok(!existsSync(`${out}${suffix}`));
  console.log("host bindings: command line checks passed");

  // NS02: nullable `Maybe<H>`, `Maybe<ref H>`, and `Maybe<ref R>` in `export def` across C, C++, Python, and C# (normal and --trap-mode return).
  const ns02Dir = join(root, "ns02");
  const ns02Out = join(ns02Dir, "out");
  mkdirSync(ns02Out, { recursive: true });
  const ns02HostC = join(ns02Dir, "ns02_host.c");
  const ns02HostObj = join(ns02Dir, "ns02_host.o");
  writeFileSync(ns02HostC, [
    "#include <stdint.h>",
    "#include <stdlib.h>",
    "typedef struct { int64_t id; } ns02_item;",
    "void *ns02_item_new(int64_t id) { ns02_item *p = malloc(sizeof(ns02_item)); p->id = id; return p; }",
    "int64_t ns02_item_id(const void *p) { return ((const ns02_item *)p)->id; }",
    "void ns02_item_free(void *p) { free(p); }",
    "",
  ].join("\n"));
  execute(clang, ["-c", "-fPIC", "-O2", ns02HostC, "-o", ns02HostObj]);
  writeFileSync(join(ns02Dir, "Main.tz"), [
    "extern type Item",
    "record Filter { min_id: i64, divisor: i64 }",
    'extern "ns02_item_new" def item_new :: i64 -> Item',
    'extern "ns02_item_id" def item_id :: ref Item -> i64',
    'extern "ns02_item_free" def item_free :: Item -> unit',
    "export def make_if_positive :: i64 -> Maybe<Item>",
    "fn make_if_positive n = if n > 0 then Some (item_new n) else None",
    "export def peek_or :: Maybe<ref Item> -> i64 -> i64",
    "fn peek_or m fallback = match m with | Some h -> item_id h | None -> fallback",
    "export def consume_filtered :: Maybe<Item> -> Maybe<ref Filter> -> i64",
    "fn consume_filtered m mf =",
    "    let bonus = match mf with | Some f -> f.min_id / f.divisor | None -> 0",
    "    match m with",
    "    | Some h ->",
    "        let v = item_id (ref h)",
    "        item_free h",
    "        v + bonus",
    "    | None -> -1 + bonus",
    "",
  ].join("\n"));
  const ns02Lib = join(ns02Out, `libns02.${extension}`);
  const ns02TrapLib = join(ns02Out, `libns02_trap.${extension}`);
  execute(compiler, ["build", ns02Dir, "--emit", "shared", "--allocator", "counting", "--link", ns02HostObj, "-O2", "-o", ns02Lib]);
  execute(compiler, ["build", ns02Dir, "--emit", "shared", "--trap-mode", "return", "--link", ns02HostObj, "-O2", "-o", ns02TrapLib]);
  execute(compiler, ["build", ns02Dir, "--emit", "header", "--allocator", "counting", "-o", join(ns02Out, "ns02.h")]);
  execute(compiler, ["build", ns02Dir, "--emit", "header", "--trap-mode", "return", "-o", join(ns02Out, "ns02_trap.h")]);
  for (const kind of ["py", "cpp", "cs"]) {
    const ext = kind === "py" ? "py" : kind === "cpp" ? "hpp" : "cs";
    execute(compiler, ["build", ns02Dir, "--emit", `bindings-${kind}`, "-o", join(ns02Out, `ns02.${ext}`)]);
    execute(compiler, ["build", ns02Dir, "--emit", `bindings-${kind}`, "--trap-mode", "return", "-o", join(ns02Out, `ns02_trap.${ext}`)]);
  }
  rejected(["build", ns02Dir, "--target", "wasm32", "-o", join(ns02Out, "ns02.wasm")], "E1008", 1);
  rejected(["build", ns02Dir, "--target", "wasm32", "--emit", "bindings-js", "-o", join(ns02Out, "ns02.mjs")], "E1008", 1);

  // Misaligned non-NULL Maybe<ref Filter> pointer from C traps cleanly (status == 1 in --trap-mode return).
  const ns02AlignC = join(ns02Dir, "ns02_align.c");
  writeFileSync(ns02AlignC, [
    "#undef NDEBUG",
    "#include <assert.h>",
    "#include <stdint.h>",
    '#include "ns02_trap.h"',
    "int main(void) {",
    "  uint64_t raw[4] = {0};",
    "  const tz_record_4Main_6Filter *misaligned = (const tz_record_4Main_6Filter *)((const char *)raw + 1);",
    "  tsuzuri_trap_info trap = {0, 0}; int64_t out = 0;",
    "  assert(tsuzuri_try_consume_filtered(&trap, &out, 0, misaligned) == 1);",
    "  assert(tsuzuri_try_consume_filtered(&trap, &out, 0, 0) == 0 && out == -1);",
    "  return 0;",
    "}",
    "",
  ].join("\n"));
  const ns02AlignBin = join(ns02Dir, "ns02_align");
  execute(clang, ["-std=c11", "-O2", "-I", ns02Out, ns02AlignC, ns02TrapLib, `-Wl,-rpath,${ns02Out}`, "-o", ns02AlignBin]);
  execute(ns02AlignBin, []);

  if (pythonVersion) {
    const ns02PyTest = join(ns02Dir, "test_ns02.py");
    writeFileSync(ns02PyTest, [
      "import sys",
      `sys.path.insert(0, ${JSON.stringify(ns02Out)})`,
      "import ns02, ns02_trap",
      "for mod, is_trap in ((ns02, False), (ns02_trap, True)):",
      "    lib = mod.load()",
      "    assert lib.make_if_positive(0) is None",
      "    h = lib.make_if_positive(40)",
      "    assert h is not None",
      "    assert lib.peek_or(None, 99) == 99",
      "    assert lib.peek_or(h, 99) == 40",
      "    assert lib.consume_filtered(None, None) == -1",
      "    assert lib.consume_filtered(None, mod.tz_record_4Main_6Filter(10, 5)) == 1",
      "    assert lib.consume_filtered(h, mod.tz_record_4Main_6Filter(10, 5)) == 42",
      "    try:",
      "        lib.peek_or(0, 1)",
      "        assert False",
      "    except TypeError:",
      "        pass",
      "    if is_trap:",
      "        try:",
      "            lib.consume_filtered(None, mod.tz_record_4Main_6Filter(10, 0))",
      "            assert False",
      "        except mod.TsuzuriTrap:",
      "            pass",
      "",
    ].join("\n"));
    execute(python, [ns02PyTest]);
  }

  if (clangxxVersion) {
    const ns02CppTest = join(ns02Dir, "test_ns02.cpp");
    writeFileSync(ns02CppTest, [
      "#undef NDEBUG",
      "#include <cassert>",
      "#include <stdexcept>",
      "#ifdef TRAP_MODE",
      '#include "ns02_trap.hpp"',
      "namespace api = tsuzuri::ns02_trap;",
      "#else",
      '#include "ns02.hpp"',
      "namespace api = tsuzuri::ns02;",
      "#endif",
      "int main() {",
      "  assert(!api::make_if_positive(0).has_value());",
      "  auto h = api::make_if_positive(40);",
      "  assert(h.has_value());",
      "  assert(api::peek_or(std::nullopt, 99) == 99);",
      "  assert(api::peek_or(h, 99) == 40);",
      "  assert(api::consume_filtered(std::nullopt, std::nullopt) == -1);",
      "  tz_record_4Main_6Filter f{10, 5};",
      "  assert(api::consume_filtered(std::nullopt, f) == 1);",
      "  assert(api::consume_filtered(h, f) == 42);",
      "  bool rejected_null_some = false;",
      "  try { (void)api::peek_or(std::optional<tz_handle_4Main_4Item>(nullptr), 0); } catch (const std::invalid_argument &) { rejected_null_some = true; }",
      "  assert(rejected_null_some);",
      "#ifdef TRAP_MODE",
      "  bool caught_trap = false;",
      "  tz_record_4Main_6Filter bad{10, 0};",
      "  try { (void)api::consume_filtered(std::nullopt, bad); } catch (const api::trap_error &) { caught_trap = true; }",
      "  assert(caught_trap);",
      "#else",
      "  tsuzuri_allocation_stats stats{};",
      "  tsuzuri_alloc_stats(&stats);",
      "  assert(stats.live_bytes == 0);",
      "#endif",
      "  return 0;",
      "}",
      "",
    ].join("\n"));
    const ns02CppPlain = join(ns02Dir, "ns02_cpp_plain");
    execute(clangxx, ["-std=c++20", "-Wall", "-Wextra", "-Werror", "-O2", ns02CppTest, "-I", ns02Out, ns02Lib, `-Wl,-rpath,${ns02Out}`, "-o", ns02CppPlain]);
    execute(ns02CppPlain, []);
    const ns02CppTrap = join(ns02Dir, "ns02_cpp_trap");
    execute(clangxx, ["-std=c++20", "-Wall", "-Wextra", "-Werror", "-O2", "-DTRAP_MODE", ns02CppTest, "-I", ns02Out, ns02TrapLib, `-Wl,-rpath,${ns02Out}`, "-o", ns02CppTrap]);
    execute(ns02CppTrap, []);
  }

  if (major >= 8) {
    const ns02CsProj = join(ns02Dir, "cs");
    mkdirSync(ns02CsProj, { recursive: true });
    copyFileSync(join(ns02Out, "ns02.cs"), join(ns02CsProj, "ns02.cs"));
    copyFileSync(join(ns02Out, "ns02_trap.cs"), join(ns02CsProj, "ns02_trap.cs"));
    writeFileSync(join(ns02CsProj, "ns02.csproj"), `<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <TargetFramework>net${major}.0</TargetFramework>
    <AllowUnsafeBlocks>true</AllowUnsafeBlocks>
    <Nullable>enable</Nullable>
    <ImplicitUsings>disable</ImplicitUsings>
    <TreatWarningsAsErrors>true</TreatWarningsAsErrors>
  </PropertyGroup>
</Project>
`);
    writeFileSync(join(ns02CsProj, "Program.cs"), [
      "using System;",
      "using System.Runtime.InteropServices;",
      "using Tsuzuri.Bindings;",
      "internal static class Program {",
      "  private static void Check(bool ok, string msg) { if (!ok) throw new InvalidOperationException(msg); }",
      "  public static void Main(string[] args) {",
      "    NativeLibrary.SetDllImportResolver(typeof(Ns02).Assembly, (name, _, _) => name switch {",
      "      Ns02.LibraryName => NativeLibrary.Load(args[0]),",
      "      Ns02Trap.LibraryName => NativeLibrary.Load(args[1]),",
      "      _ => IntPtr.Zero,",
      "    });",
      "    Check(Ns02.make_if_positive(0) is null, \"None return\");",
      "    var h = Ns02.make_if_positive(40);",
      "    Check(h is not null, \"Some return\");",
      "    Check(Ns02.peek_or(null, 99) == 99, \"None borrow\");",
      "    Check(Ns02.peek_or(h, 99) == 40, \"Some borrow\");",
      "    Check(Ns02.consume_filtered(null, null) == -1, \"None owned + None record\");",
      "    Ns02.tz_record_4Main_6Filter? f = new Ns02.tz_record_4Main_6Filter { min_id = 10, divisor = 5 };",
      "    Check(Ns02.consume_filtered(null, in f) == 1, \"Some record\");",
      "    Check(Ns02.consume_filtered(h, in f) == 42, \"Some owned + Some record\");",
      "    bool badSome = false;",
      "    try { Ns02.peek_or(new Ns02.tz_handle_4Main_4Item(0), 1); } catch (ArgumentException) { badSome = true; }",
      "    Check(badSome, \"Some(0) rejected\");",
      "    var ht = Ns02Trap.make_if_positive(40);",
      "    Check(ht is not null, \"trap mode Some return\");",
      "    Ns02Trap.tz_record_4Main_6Filter? ft = new Ns02Trap.tz_record_4Main_6Filter { min_id = 10, divisor = 5 };",
      "    Check(Ns02Trap.consume_filtered(ht, in ft) == 42, \"trap mode Some owned + record\");",
      "    bool trapped = false;",
      "    Ns02Trap.tz_record_4Main_6Filter? bad = new Ns02Trap.tz_record_4Main_6Filter { min_id = 10, divisor = 0 };",
      "    try { Ns02Trap.consume_filtered(null, in bad); } catch (Ns02Trap.TsuzuriTrapException) { trapped = true; }",
      "    Check(trapped, \"trap caught\");",
      "  }",
      "}",
      "",
    ].join("\n"));
    const environment = { ...process.env, DOTNET_CLI_TELEMETRY_OPTOUT: "1", DOTNET_NOLOGO: "1", DOTNET_SKIP_FIRST_TIME_EXPERIENCE: "1" };
    execute("dotnet", ["build", "-c", "Release", "-nologo", "-v", "q"], { cwd: ns02CsProj, env: environment });
    execute("dotnet", [join(ns02CsProj, "bin", "Release", `net${major}.0`, "ns02.dll"), ns02Lib, ns02TrapLib], { env: environment });
  }
  console.log("host bindings: NS02 nullable exports and host bindings passed");
} finally {
  rmSync(root, { recursive: true, force: true });
}
