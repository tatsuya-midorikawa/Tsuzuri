// A23: `--overflow-report` end to end. The fixture's exports run with an input that wraps and one
// that does not; the values are checked against JavaScript's BigInt, natively (through a C host,
// whose standard error is the report) and on wasm32 (through `tsuzuri_overflow_hit` and the side
// table), at -O0 and -O3. A build without the report has the same values and reports nothing.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve, sep } from "node:path";

const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const clang = process.env.TSUZURI_CLANG ?? "clang";
const fixture = "tests/fixtures/overflow_report";
// The source path as the build spells it: the directory it received, then the file.
const path = `${fixture}${sep}Main.tz`;
const pattern = (text) => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
// A C host writes CRLF on Windows, where its standard streams are in text mode.
const lines = (text) => text.replaceAll("\r\n", "\n");
const temporary = mkdtempSync(join(tmpdir(), "tsuzuri-overflow-report-"));

const run = (program, args) => {
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 240_000 });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
};

const i32 = (value) => BigInt.asIntN(32, value), u32 = (value) => BigInt.asUintN(32, value);
const i8 = (value) => BigInt.asIntN(8, value), i16 = (value) => BigInt.asIntN(16, value);
const i64 = (value) => BigInt.asIntN(64, value), i128 = (value) => BigInt.asIntN(128, value);
const tail = (n) => { let total = 0n; for (; n > 0n; n -= 1n) total = i32(total + 1000000000n); return total; };
// Each export with the input that wraps and the one that does not (appendix E.2), the start of
// its site's span (`line:column (type operation)`, as the side table has it) and the operator
// that the report line names.
const cases = [
  ["add32", [[2147483647n, 1n], [1n, 2n]], (a, b) => i32(a + b), "1:49 (i32 add)", "1:51"],
  ["sub32u", [[0n, 1n], [5n, 3n]], (a, b) => u32(a - b), "2:53 (i32u sub)", "2:55"],
  ["mul8", [[100n, 100n], [3n, 4n]], (a, b) => i8(a * b), "3:45 (i8 mul)", "3:47"],
  ["neg64", [[-9223372036854775808n], [5n]], (a) => i64(-a), "4:40 (i64 neg)", "4:40"],
  ["pow16", [[200n, 2n], [3n, 2n]], (a, b) => i16(a ** b), "5:49 (i16 pow)", "5:51"],
  ["wide64", [[4611686018427387904n], [1n]], (a) => i64(i128((a << 64n) + (a << 64n))), "6:36 (i128 add)", "6:47"],
  ["tail32", [[3n], [2n]], tail, "8:100 (i32 add)", "8:107"],
];
const expectedStdout = [
  ...cases.map(([, inputs, reference]) => inputs.map((input) => reference(...input)).join(" ")),
  `233 2147483647`,
].join("\n") + "\n";
const expectedReport = cases.map(([, , , site, operator]) => `warning: integer overflow wrapped at ${path}:${operator} ${site.slice(site.indexOf(" ") + 1)}\n`).join("");

let runs = 0;
for (const optimization of ["0", "3"]) {
  // Native: the C host prints the values; the report goes to its standard error.
  for (const report of [false, true]) {
    const ir = join(temporary, `native-${optimization}-${report}.ll`);
    const executable = join(temporary, `host-${optimization}-${report}${process.platform === "win32" ? ".exe" : ""}`);
    run(compiler, ["build", fixture, "--emit", "llvm", `-O${optimization}`, "-o", ir, ...(report ? ["--overflow-report"] : [])]);
    const text = readFileSync(ir, "utf8");
    assert.equal(text.includes("tsuzuri_overflow_hits"), report);
    run(clang, [`-O${optimization}`, "-Wno-override-module", `${fixture}/host.c`, ir, "-o", executable]);
    const result = run(executable, []);
    assert.equal(lines(result.stdout), expectedStdout, `native -O${optimization} report=${report}`);
    assert.equal(lines(result.stderr), report ? expectedReport : "", `native -O${optimization} report=${report}`);
    runs += 1;
  }
  // wasm32: the hits are read through the exports, and the side table names the sites.
  const plainPath = join(temporary, `plain-${optimization}.wasm`);
  const reportPath = join(temporary, `report-${optimization}.wasm`);
  run(compiler, ["build", fixture, "--target", "wasm32", `-O${optimization}`, "-o", plainPath]);
  run(compiler, ["build", fixture, "--target", "wasm32", `-O${optimization}`, "--overflow-report", "-o", reportPath]);
  const plain = new WebAssembly.Module(readFileSync(plainPath));
  const reported = new WebAssembly.Module(readFileSync(reportPath));
  assert.deepEqual(WebAssembly.Module.imports(reported), []);
  assert.deepEqual(WebAssembly.Module.imports(plain), []);
  const plainExports = new WebAssembly.Instance(plain, {}).exports;
  assert.equal(plainExports.tsuzuri_overflow_count, undefined);
  const table = JSON.parse(readFileSync(`${reportPath}.overflow.json`, "utf8"));
  assert.equal(table.version, 1);
  assert.equal(table.sites.length, 8);
  const at = (site) => table.sites.findIndex((entry) => `${entry.span.line}:${entry.span.column} (${entry.type} ${entry.operation})` === site);
  for (const [index, entry] of table.sites.entries()) {
    assert.equal(entry.id, index);
    assert.equal(entry.path, path);
    assert.match(entry.message, new RegExp(`^integer overflow wrapped at ${pattern(path)}:${entry.span.line}:\\d+ \\(${entry.type} ${entry.operation}\\)$`));
  }
  for (const [, , , site, operator] of cases) {
    const entry = table.sites[at(site)];
    assert.equal(entry.message, `integer overflow wrapped at ${path}:${operator} ${site.slice(site.indexOf(" ") + 1)}`);
  }
  const call = (exports, name, input) => {
    const argument = (value) => (["neg64", "wide64"].includes(name) ? value : Number(value));
    const value = exports[`tz_${name}`](...input.map(argument));
    return name === "sub32u" ? BigInt(value >>> 0) : BigInt(value);
  };
  const fresh = () => new WebAssembly.Instance(reported, {}).exports;
  const wrapping = fresh(), quiet = fresh();
  assert.equal(wrapping.tsuzuri_overflow_count(), 8);
  for (const [name, [wraps, stays], reference, site] of cases) {
    assert.equal(call(wrapping, name, wraps), reference(...wraps), `${name} -O${optimization}`);
    assert.equal(call(plainExports, name, wraps), reference(...wraps), `${name} -O${optimization}`);
    assert.equal(call(quiet, name, stays), reference(...stays), `${name} -O${optimization}`);
    assert.equal(call(plainExports, name, stays), reference(...stays), `${name} -O${optimization}`);
    assert.notEqual(at(site), -1, site);
  }
  assert.equal(wrapping.tz_masked(1000), 233);
  assert.equal(wrapping.tz_saturated(2147483647, 1), 2147483647);
  const hits = (exports) => Array.from({ length: 8 }, (_, index) => exports.tsuzuri_overflow_hit(index));
  const expectedHits = Array(8).fill(1);
  expectedHits[at("8:92 (i32 sub)")] = 0;
  assert.deepEqual(hits(wrapping), expectedHits, `-O${optimization}`);
  assert.deepEqual(hits(quiet), Array(8).fill(0), `-O${optimization}`);
  assert.equal(wrapping.tsuzuri_overflow_hit(8), 0);
  assert.equal(wrapping.tsuzuri_overflow_hit(-1), 0);
  runs += 1;
}

rmSync(temporary, { recursive: true, force: true });
console.log(`overflow report: native and wasm32 at -O0 and -O3, ${runs} runs, ${cases.length} wrapping exports, 8 sites`);
