// E2E for `tsuzuri bindgen` (E11): golden outputs of the four fixtures, determinism,
// `check`/`fmt --check` of the outputs, a native round trip that links the generated
// externs to a C library at -O0 and -O3, and the CLI's error codes.
// Usage: node tests/bindgen.mjs target/release/tsuzuri
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const clang = process.env.TSUZURI_CLANG ?? "clang";
mkdirSync(resolve("target"), { recursive: true });
const root = mkdtempSync(join(resolve("target"), "tsuzuri-bindgen-"));
const fixtures = join(root, "fixtures");
cpSync("tests/fixtures/bindgen", fixtures, { recursive: true });

function execute(program, args, { success = true, env = process.env } = {}) {
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 180_000, maxBuffer: 64 * 1024 * 1024, env });
  assert.ifError(result.error);
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}
function rejected(args, code, status, env) {
  const result = execute(compiler, args, { success: false, env });
  assert.equal(result.status, status, `${args.join(" ")}\n${result.stderr}`);
  assert.match(result.stderr, new RegExp(`error\\[${code}\\]`), `${args.join(" ")}\n${result.stderr}`);
  assert.equal(result.stdout, "");
  return result.stderr;
}
const sanitize = (text) => text.replace(/[^\x20-\x7e]/g, "?");
const clangVersion = sanitize(execute(clang, ["--version"]).stdout.split("\n")[0].trim());
const target = sanitize(execute(clang, ["-dumpmachine"]).stdout.trim());
const sha256 = (path) => createHash("sha256").update(readFileSync(path)).digest("hex");

// Runs bindgen into a directory of its own (a file input loads its whole directory) and
// compares the output with expected.tz after replacing the machine-specific lines.
function golden(name, header, module, extra = []) {
  const fixture = join(fixtures, name);
  const directory = join(root, `out-${name}`);
  mkdirSync(directory);
  const output = join(directory, `${module}.tz`);
  const args = ["bindgen", join(fixture, header), "-o", output, ...extra];
  const first = execute(compiler, args);
  assert.equal(first.stdout, "");
  const text = readFileSync(output, "utf8");
  const lines = text.split("\n");
  assert.equal(lines[1], `// header: ${header} sha256=${sha256(join(fixture, header))}`);
  assert.equal(lines[2], `// clang: ${clangVersion}`);
  assert.equal(lines[3], `// target: ${target}`);
  lines[1] = `// header: ${header} sha256=<sha256>`;
  lines[2] = "// clang: <clang>";
  lines[3] = "// target: <target>";
  assert.equal(lines.join("\n"), readFileSync(join(fixture, "expected.tz"), "utf8"), name);
  // Deterministic: a second run replaces the generated file with the same bytes.
  execute(compiler, args);
  assert.equal(readFileSync(output, "utf8"), text, `${name} is not deterministic`);
  execute(compiler, ["check", output]);
  execute(compiler, ["fmt", "--check", output]);
  const skipped = text.split("\n").filter((line) => line.startsWith("// skipped "));
  assert.equal((first.stderr.match(/warning\[W2002\]/g) ?? []).length, skipped.length, `${name}\n${first.stderr}`);
  return { output, text, skipped };
}

try {
  const basic = golden("basic", "sample.h", "Sample");
  assert.deepEqual(basic.skipped, ["// skipped sample_log: variadic function", "// skipped sample_twice: function has internal linkage (static)"]);
  const included = golden("include", "main.h", "Included", ["--include-dir", join(fixtures, "include", "inc")]);
  assert.doesNotMatch(included.text, /"b"|DECLARE_F|IN_OTHER|DROPPED/);
  golden("names", "names.h", "Names");
  const skipped = golden("skipped", "skipped.h", "Skipped");
  const annotations = [
    "--buffer", "ext_sum:values:count", "--buffer", "ext_mean:values:count", "--buffer", "ext_checksum:1:2",
    "--buffer", "ext_text_length:text:length", "--buffer", "ext_fill:values:count", "--buffer", "ext_split:values:count",
    "--buffer", "ext_sum32:values:count", "--consume", "ext_counter_free:counter",
  ];
  const extended = golden("extended", "ext.h", "Ext", annotations);
  assert.equal(extended.skipped.length, 9);

  // --json: one W2002 object per skipped declaration, in header order.
  const json = execute(compiler, ["bindgen", join(fixtures, "skipped", "skipped.h"), "-o", join(root, "out-skipped", "Skipped.tz"), "--json"]);
  const warnings = json.stderr.trim().split("\n").map((line) => JSON.parse(line));
  assert.equal(warnings.length, skipped.skipped.length);
  assert.equal(warnings.length, 27);
  warnings.forEach((warning, index) => {
    const [, name, reason] = /^\/\/ skipped ([^:]+): (.*)$/.exec(skipped.skipped[index]);
    assert.equal(warning.severity, "warning");
    assert.equal(warning.code, "W2002");
    assert.equal(warning.message, `skipped C declaration '${name}': ${reason}; declare it by hand or wrap it in a C function with a supported signature`);
    assert.ok(warning.span.line > 2 && warning.span.end > warning.span.start, JSON.stringify(warning));
    const header = readFileSync(join(fixtures, "skipped", "skipped.h"), "utf8");
    assert.equal(header.slice(warning.span.start, warning.span.end), name, JSON.stringify(warning));
  });
  console.log("bindgen: golden, determinism, check, fmt and W2002 passed");

  // Round trip: Main.tz calls every generated extern; host.c compares each result with
  // the direct C call and with the value worked out by hand.
  const project = join(root, "project");
  mkdirSync(project);
  cpSync(join(fixtures, "basic", "Main.tz"), join(project, "Main.tz"));
  cpSync(basic.output, join(project, "Sample.tz"));
  execute(compiler, ["build", project, "--emit", "header", "-o", join(root, "tz-basic.h")]);
  for (const optimization of ["-O0", "-O3"]) {
    const object = join(root, `basic${optimization}.o`);
    execute(compiler, ["build", project, "--emit", "object", optimization, "-o", object]);
    const executable = join(root, `basic${optimization}`);
    execute(clang, [optimization, object, join(fixtures, "basic", "sample.c"), join(fixtures, "basic", "host.c"), "-I", join(fixtures, "basic"), "-I", root, "-lm", "-o", executable]);
    assert.equal(execute(executable, []).stdout, "bindgen round trip ok\n");
    console.log(`bindgen: native round trip ${optimization} passed`);
  }

  // Phase 2 round trip: macros, an opaque handle (--consume), callbacks, and buffers.
  // The IR build tracks Tsuzuri's allocations, so every lent buffer must be freed.
  const extendedProject = join(root, "extended-project");
  mkdirSync(extendedProject);
  cpSync(join(fixtures, "extended", "Main.tz"), join(extendedProject, "Main.tz"));
  cpSync(extended.output, join(extendedProject, "Ext.tz"));
  execute(compiler, ["build", extendedProject, "--emit", "header", "-o", join(root, "tz-extended.h")]);
  const irPath = join(root, "extended.ll");
  execute(compiler, ["build", extendedProject, "--emit", "llvm", "-o", irPath]);
  const ir = readFileSync(irPath, "utf8");
  assert.equal(ir, (execute(compiler, ["build", extendedProject, "--emit", "llvm", "-o", irPath]), readFileSync(irPath, "utf8")));
  writeFileSync(irPath, ir.replaceAll("@malloc", "@tracked_alloc").replaceAll("@free", "@tracked_free").replaceAll("@realloc", "@tracked_realloc"));
  const extendedSources = [join(fixtures, "extended", "ext.c"), join(fixtures, "extended", "host.c"), "-I", join(fixtures, "extended"), "-I", root, "-lm"];
  for (const optimization of ["-O0", "-O3"]) {
    const tracked = join(root, `extended-ir${optimization}`);
    execute(clang, [optimization, "-Wno-override-module", irPath, "src/runtime/task.c", ...extendedSources, "-pthread", "-o", tracked]);
    assert.equal(execute(tracked, []).stdout, "bindgen extended round trip ok\n");
    const object = join(root, `extended${optimization}.o`);
    execute(compiler, ["build", extendedProject, "--emit", "object", optimization, "-o", object]);
    const linked = join(root, `extended${optimization}`);
    execute(clang, [optimization, object, ...extendedSources, "-o", linked]);
    assert.equal(execute(linked, []).stdout, "bindgen extended round trip ok\n");
    console.log(`bindgen: native round trip with macros, handles, callbacks, and buffers ${optimization} passed`);
  }

  // The command line and the tools.
  const header = join(fixtures, "basic", "sample.h");
  const fresh = join(root, "fresh");
  mkdirSync(fresh);
  rejected(["bindgen", header], "E2000", 2);
  rejected(["bindgen", header, "-o", join(fresh, "x.txt")], "E2000", 2);
  rejected(["bindgen", header, "-o", join(fresh, "X.tz"), "--target", "wasm32"], "E2000", 2);
  rejected(["bindgen", header, header, "-o", join(fresh, "X.tz")], "E2000", 2);
  rejected(["bindgen", join(fixtures, "basic"), "-o", join(fresh, "X.tz")], "E2001", 1);
  const missing = rejected(["bindgen", header, "-o", join(fresh, "X.tz")], "E2002", 1, { ...process.env, TSUZURI_CLANG: join(root, "no-such-clang") });
  assert.match(missing, /TSUZURI_CLANG/);
  const broken = join(fresh, "broken.h");
  writeFileSync(broken, "int broken(;\n");
  assert.match(rejected(["bindgen", broken, "-o", join(fresh, "X.tz")], "E2002", 1), /broken\.h:1/);
  assert.match(rejected(["bindgen", join(fixtures, "include", "main.h"), "-o", join(fresh, "X.tz")], "E2002", 1), /--include-dir/);
  assert.ok(!existsSync(join(fresh, "X.tz")));
  // Annotations: the shape is checked with the arguments, the names after Clang.
  const extendedHeader = join(fixtures, "extended", "ext.h");
  rejected(["bindgen", extendedHeader, "-o", join(fresh, "X.tz"), "--buffer", "ext_sum:values"], "E2000", 2);
  rejected(["bindgen", extendedHeader, "-o", join(fresh, "X.tz"), "--consume", "ext_counter_free:0"], "E2000", 2);
  rejected(["bindgen", extendedHeader, "-o", join(fresh, "X.tz"), "--buffer", "a:b:c", "--buffer", "a:b:d"], "E2000", 2);
  assert.match(rejected(["bindgen", extendedHeader, "-o", join(fresh, "X.tz"), "--buffer", "nothing:1:2"], "E2000", 1), /'nothing', which the header does not declare/);
  assert.match(rejected(["bindgen", extendedHeader, "-o", join(fresh, "X.tz"), "--buffer", "ext_sum:values:size"], "E2000", 1), /parameter 'size', which 'ext_sum' does not have/);
  assert.match(rejected(["bindgen", extendedHeader, "-o", join(fresh, "X.tz"), "--consume", "ext_counter_free:3"], "E2000", 1), /parameter 3 of 'ext_counter_free', which has 1 parameter$/m);
  assert.match(rejected(["bindgen", extendedHeader, "-o", join(fresh, "X.tz"), "--buffer", "ext_sum:1:2", "--consume", "ext_sum:values"], "E2000", 1), /another annotation already names/);
  assert.ok(!existsSync(join(fresh, "X.tz")));
  // Output protection: nothing that bindgen did not write is replaced.
  const handWritten = join(fresh, "Hand.tz");
  writeFileSync(handWritten, "// my code\nconst X: i32 = 1\n");
  assert.match(rejected(["bindgen", header, "-o", handWritten], "E2003", 1), /does not start with the tsuzuri bindgen marker/);
  assert.equal(readFileSync(handWritten, "utf8"), "// my code\nconst X: i32 = 1\n");
  const link = join(fresh, "Link.tz");
  symlinkSync(basic.output, link);
  rejected(["bindgen", header, "-o", link], "E2003", 1);
  assert.equal(readFileSync(basic.output, "utf8"), basic.text);
  rejected(["bindgen", header, "-o", fresh + "/Dir.tz/.."], "E2000", 2);
  mkdirSync(join(fresh, "Dir.tz"));
  rejected(["bindgen", header, "-o", join(fresh, "Dir.tz")], "E2003", 1);
  const self = join(fresh, "Self.tz");
  cpSync(header, self);
  assert.match(rejected(["bindgen", self, "-o", self], "E2003", 1), /must not overwrite the header/);
  assert.equal(readFileSync(self, "utf8"), readFileSync(header, "utf8"));
  // A new directory for the output is created.
  execute(compiler, ["bindgen", header, "-o", join(fresh, "new", "Sample.tz")]);
  assert.equal(readFileSync(join(fresh, "new", "Sample.tz"), "utf8"), basic.text);
  console.log("bindgen: command line and output protection passed");
} finally {
  rmSync(root, { recursive: true, force: true });
}
