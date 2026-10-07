// End-to-end test of `--emit shared` and the C#, Python and C++ bindings of E13 Phase 2 on the native
// target: the library exports only the public C ABI and links its extern imports from --link, each
// binding passes the same calls (scalars, buffers, strings, records, fixed arrays, handles), owned
// results leave no allocation behind (--allocator counting), a trap ends the process, and with
// --trap-mode return a trap becomes an exception of the host language.
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
} finally {
  rmSync(root, { recursive: true, force: true });
}
