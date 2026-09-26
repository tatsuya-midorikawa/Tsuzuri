import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(process.argv[2] ?? join(root, "target/debug/tsuzuri"));
const only = process.argv[3];
const clang = process.env.TSUZURI_CLANG ?? "clang";
const sanitizer = process.env.TSUZURI_ASAN === "1" ? ["-fsanitize=address"] : [];
const min = -(1n << 63n);
const max = (1n << 63n) - 1n;

function execute(program, args, success = true) {
  const result = spawnSync(program, args, {
    cwd: root, encoding: "utf8", timeout: 180_000, maxBuffer: 16 * 1024 * 1024,
  });
  if (result.error) throw result.error;
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}
const cli = (args) => execute(compiler, args);
const cValue = (n) => typeof n !== "bigint" ? String(n)
  : n === min ? "INT64_MIN" : n < 0n ? `(-INT64_C(${-n}))`
    : n > max ? `UINT64_C(${n})` : `INT64_C(${n})`;

// Each suite is a fixture directory whose exports are called with the listed
// arguments. Native hosts track every allocation, so each call must leave no
// live heap bytes; WASM modules must stay import-free.
const suites = {
  visibility: {
    cases: [
      ["answer", [], 42n],
      ["reveal", [2n], 8n],
      ["reveal", [12n], 15n],
      ["reveal", [41n], 41n],
      ["reveal", [100n], 100n],
      ["owned_total", [1n], 3n],
      ["owned_total", [64n], 66n],
    ],
    inspect(ir, header) {
      assert.match(header, /tz_answer/);
      assert.doesNotMatch(header, /owned\b|sum|weight|Token|Pair/);
      assert.match(ir, /@tz\.fn\.Secret\.weight/);
    },
  },
  generic_records: {
    cases: [
      ["numbers", [], 4203n],
      ["strings", [], 5n],
      ["nested", [39n], 42n],
      ["nested", [-3n], 0n],
      ["closures", [20n], 41n],
      ["closures", [0n], 1n],
      ["matched", [2n], 402n],
      ["collections", [5n], 3022n],
      ["nested_syntax", [39n], 42n],
      ["nested_syntax", [-3n], 0n],
    ],
    inspect(ir, header) {
      assert.doesNotMatch(header, /Pair|Box|Wrap/);
      for (const definition of [
        '%"tz.record.Main.Pair[i64,string]" = type { i64, %tz.string }',
        '%"tz.record.Main.Pair[i64,i64]" = type { i64, i64 }',
        '%"tz.record.Main.Box[fn[i64->i64]]" = type { %tz.closure }',
        '%"tz.record.Main.Wrap[Main.Pair[i64,string]]" = type { %"tz.record.Main.Pair[i64,string]" }',
      ]) {
        assert.equal(ir.split(`${definition}\n`).length, 2, `${definition} is defined once`);
      }
      assert.doesNotMatch(ir, /tz\.record\.Main\.(Pair|Box|Wrap) = type/);
    },
  },
  unions: {
    cases: [
      ["shape_area_scaled", [], 24566n],
      ["maybe_number", [], 42n],
      ["maybe_string_length", [], 5n],
      ["color_sum", [], 123n],
      ["constructors", [5n], 3655n],
      ["constructors", [-2n], 2878n],
      ["either", [4n], 8n],
      ["either", [-3n], 6n],
      ["either", [0n], 7n],
      ["guarded", [3n], 7n],
      ["guarded", [10n], 107n],
      ["nested", [5n], 5n],
      ["nested", [0n], 100n],
      ["nested", [-1n], 200n],
      ["containers", [5n], 461513n],
      ["containers", [0n], 460003n],
      ["owned_collections", [4n], 443206n],
      ["owned_collections", [0n], 93206n],
      ["copy_reuse", [1n], 6066n],
      ["copy_reuse", [10n], 33336n],
      ["captured", [3n], 21n],
      ["captured", [5n], 29n],
      ["qualified", [4n], 45n],
      ["qualified", [7n], 75n],
      ["churn", [0n], 0n],
      ["churn", [4n], 34n],
      ["churn", [40000n], 1600226662n],
    ],
    inspect(ir, header) {
      assert.doesNotMatch(header, /Shape|Maybe|Color|Pick|Tagged/);
      for (const definition of [
        '%"tz.union.Main.Shape" = type { i32, [1 x i128] }',
        '%"tz.union.Main.Color" = type i32',
        '%"tz.union.Main.Pick" = type { i32, i64 }',
        '%"tz.union.Main.Maybe[i64]" = type { i32, i64 }',
        '%"tz.union.Main.Maybe[string]" = type { i32, %tz.string }',
        '%"tz.union.Shapes.Shape" = type { i32, double }',
      ]) {
        assert.equal(ir.split(`${definition}\n`).length, 2, `${definition} is defined once`);
      }
      assert.doesNotMatch(ir, /tz\.union\.Main\.Maybe = type/);
      assert.match(ir, /switch i32/);
      // First-class constructors become one hidden function per concrete instance.
      const constructors = ir.match(/^define internal \S+ @tz\.fn\.\$case\.Main\.Maybe\.Some\.\$mono\.\d+\(/gm) ?? [];
      assert.equal(constructors.length, 2, "Maybe.Some has an i64 and a string constructor function");
    },
  },
  stdlib: {
    cases: [
      ["answer", [], 42n],
      ["checked", [5n], 5n],
      ["checked", [0n], 0n],
    ],
    traps: [["checked", [-1n]]],
    inspect(ir, header) {
      assert.match(header, /tz_answer/);
      assert.doesNotMatch(header, /Math|zero|identity/);
      for (const definition of [
        "define internal double @tz.fn.Math.zero(",
        "define internal double @tz.fn.Math.identity_f64(",
        "define internal i64 @tz.builtin.unreachable.i64(i8 %unit) noreturn nounwind {",
      ]) {
        assert.equal(ir.split(definition).length, 2, `${definition} is defined once`);
      }
    },
  },
  option_result: {
    cases: [
      ["option_some", [], 42n],
      ["option_none_short_circuit", [], 42n],
      ["result_ok", [], 42n],
      ["result_error_short_circuit", [], 42n],
      ["option_loop", [0n], 0n],
      ["option_loop", [257n], 257n],
      ["result_loop", [0n], 0n],
      ["result_loop", [257n], 257n],
      ["option_loop", [40000n], 40000n],
      ["result_loop", [40000n], 40000n],
      ["stopped_loops", [], 42n],
      ["while_and_zero", [], 42n],
      ["while_failure", [], 42n],
      ["owned_loop_failures", [], 12n],
      ["remaining_functions", [], 42n],
      ["defaults", [], 42n],
      ["conversions", [], 42n],
      ["owned", [0n], 0n],
      ["owned", [1n], 20n],
      ["owned", [40000n], 800000n],
      ["copy_snapshot", [], 42n],
      ["borrowed_patterns", [], 24n],
    ],
    traps: [
      ["trap_option_get", []],
      ["trap_result_get", []],
      ["trap_result_get_error", []],
      ["trap_string", []],
      ["trap_eager_default", []],
    ],
    inspect(ir, header) {
      assert.doesNotMatch(header, /Option|Result/);
      assert.match(ir, /@tz\.specialized\./);
      assert.match(ir, /tz\.union\.Option\.Option\[string\]/);
      assert.match(ir, /tz\.union\.Result\.Result\[string,i64\]/);
    },
  },
  display_parse: {
    cases: [
      ["displays", [], 1],
      ["text_copy", [], 20n],
      ["parse_numbers", [], 42n],
      ["parse_bool", [], 1],
      ["malformed", [], 1],
      ["round_f64", [0.1], 0.1],
      ["round_f32", [Math.fround(0.1)], Math.fround(0.1)],
      ["custom", [0n], 0n],
      ["custom", [1n], 10n],
      ["custom", [40000n], 400000n],
    ],
    inspect(ir) {
      assert.match(ir, /@tz_soft_format/);
      assert.match(ir, /@tz_soft_parse/);
      assert.doesNotMatch(ir, /@printf|@strtod|@strtof|@snprintf|@memcmp/);
    },
  },
};

function run(name, suite) {
  const fixture = join(root, "tests/fixtures", name);
  const temporary = mkdtempSync(join(tmpdir(), `tsuzuri-${name}-`));
  try {
    cli(["check", fixture]);
    const ir = join(temporary, `${name}.ll`);
    const again = join(temporary, "again.ll");
    // A `tz-` prefix keeps fixture headers from shadowing system headers.
    const headerPath = join(temporary, `tz-${name}.h`);
    cli(["build", fixture, "--emit", "header", "-o", headerPath]);
    cli(["build", fixture, "--emit", "llvm", "-o", ir]);
    cli(["build", fixture, "--emit", "llvm", "-o", again]);
    const sourceIr = readFileSync(ir, "utf8");
    assert.equal(sourceIr, readFileSync(again, "utf8"), `${name}: IR is deterministic`);
    const declarations = sourceIr.match(/^declare .*$/gm) ?? [];
    assert.equal(new Set(declarations).size, declarations.length, `${name}: declarations are deduplicated`);
    suite.inspect?.(sourceIr, readFileSync(headerPath, "utf8"));
    let trackedIr = sourceIr.replaceAll("@malloc", "@tracked_alloc").replaceAll("@free", "@tracked_free");
    if (sanitizer.length) trackedIr = trackedIr.replaceAll(" nounwind {", " nounwind sanitize_address {");
    writeFileSync(ir, trackedIr);
    const traps = suite.traps ?? [];
    const host = join(temporary, "host.c");
    writeFileSync(host, `
#include <assert.h>
#include <math.h>
#include <stdint.h>
#include <stdlib.h>
#include "tz-${name}.h"
static uint64_t live;
void *tracked_alloc(uint64_t size) {
    uint64_t *p = malloc((size_t)size + 16);
    assert(p);
    p[0] = size;
    p[1] = UINT64_C(0x51a110ca7e);
    live += size;
    return p + 2;
}
void tracked_free(void *value) {
    if (!value) return;
    uint64_t *p = (uint64_t *)value - 2;
    assert(p[1] == UINT64_C(0x51a110ca7e));
    p[1] = 0;
    assert(live >= p[0]);
    live -= p[0];
    free(p);
}
int main(int argc, char **argv) {
    if (argc == 2) {
        switch (atoi(argv[1])) {
            ${traps.map(([trap, args], index) => `case ${index}: (void)tz_${trap}(${args.map(cValue).join(", ")}); break;`).join("\n")}
        }
        return 0;
    }
    ${suite.cases.map(([exported, args, expected]) =>
    `assert(tz_${exported}(${args.map(cValue).join(", ")}) == ${cValue(expected)}); assert(live == 0);`).join("\n    ")}
    return 0;
}
`);
    for (const optimization of ["0", "3"]) {
      const native = join(temporary, `${name}-O${optimization}`);
      execute(clang, [`-O${optimization}`, "-Wno-override-module", "-ffp-contract=off", ...sanitizer,
        `-I${temporary}`, ir, host, "-lm", "-o", native]);
      execute(native, []);
      for (let index = 0; index < traps.length; index++) {
        const result = execute(native, [String(index)], false);
        assert.ok(result.status !== 0 || result.signal, `${name} native O${optimization}: ${traps[index][0]} must trap`);
      }
      const wasm = join(temporary, `${name}-O${optimization}.wasm`);
      cli(["build", fixture, "--target", "wasm32", `-O${optimization}`, "-o", wasm]);
      const module = new WebAssembly.Module(readFileSync(wasm));
      assert.deepEqual(WebAssembly.Module.imports(module), [], `${name}: WASM has no imports`);
      const exports = new WebAssembly.Instance(module).exports;
      for (const [exported, args, expected] of suite.cases) {
        assert.equal(exports[`tz_${exported}`](...args), expected, `${name} WASM O${optimization}: ${exported}(${args})`);
        assert.ok(exports.memory.buffer.byteLength <= 16 * 1024 * 1024, `${name}: WASM stays within 16 MiB`);
      }
      for (const [trap, args] of traps) {
        const fresh = new WebAssembly.Instance(module).exports;
        assert.throws(() => fresh[`tz_${trap}`](...args), WebAssembly.RuntimeError, trap);
      }
    }
    return suite.cases.length;
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
}

let total = 0;
for (const [name, suite] of Object.entries(suites)) {
  if (only && only !== name) continue;
  total += run(name, suite);
  console.log(`${name}: native/WASM at O0/O3 passed`);
}
assert.ok(total > 0, "no feature suite ran");
console.log(`features: ${total} cases passed`);
