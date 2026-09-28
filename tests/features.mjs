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
const sanitizerKind = process.env.TSUZURI_TSAN === "1" ? "thread" : process.env.TSUZURI_ASAN === "1" ? "address" : null;
const sanitizer = sanitizerKind ? [`-fsanitize=${sanitizerKind}`] : [];
const nativeOptions = process.env.TSUZURI_TEST_CPU === "native" ? [process.arch === "arm64" ? "-mcpu=native" : "-march=native"] : [];
const wasmOptions = process.env.TSUZURI_TEST_WASM_SIMD === "1" ? ["--wasm-feature", "simd128"] : [];
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

function orderedReferences(count, seed, bits) {
  const rounded = bits === 32 ? Math.fround : (value) => value;
  const input = (index, salt) => rounded([1e16, 1, -1e16, 3, -3, 0.5, -0.25, 1e-38][(index * 37 + salt) % 8]);
  const values = Array.from({ length: count }, (_, index) => input(index, seed));
  let level = values;
  while (level.length > 1) level = Array.from({ length: Math.ceil(level.length / 2) }, (_, index) => index * 2 + 1 < level.length ? rounded(level[index * 2] + level[index * 2 + 1]) : level[index * 2]);
  let total = 0, correction = 0, dot = 0;
  for (const [index, value] of values.entries()) {
    const next = rounded(total + value);
    correction = rounded(correction + (Math.abs(total) >= Math.abs(value) ? rounded(rounded(total - next) + value) : rounded(rounded(value - next) + total)));
    total = next;
    dot = rounded(dot + rounded(value * input(index, seed + 3)));
  }
  return [total, level[0] ?? 0, rounded(total + correction), dot];
}

// Each suite is a fixture directory whose exports are called with the listed
// arguments. Native hosts track every allocation, so each call must leave no
// live heap bytes; WASM modules must stay import-free.
const suites = {
  higher_kinds: {
    cases: [
      ...[min, -1n, 0n, 41n, max].flatMap(seed => [["option_map", [seed], BigInt.asIntN(64, seed + 1n)], ["result_map", [seed], BigInt.asIntN(64, seed * 3n)]]),
      ["result_error", [], 6n], ["all_constructors", [], 210n], ["binary_constructor", [], 42n], ["first_class", [], 42n],
      ...[0n, 1n, 10000n].map(count => ["owned_option", [count], count * 6n]),
    ],
    inspect(ir) { assert.doesNotMatch(ir, /%tz\.hkt|dictionary/); },
  },
  fma_reductions: {
    cases: [
      ...[32, 64].flatMap((bits) => [...Array.from({ length: 18 }, (_, index) => index), 31, 32, 33, 63, 64, 65, 127, 128, 129, 255, 256, 257, 511, 512, 513, 1023, 1024, 1025].flatMap((count) => [0, 1, 7].flatMap((seed) => orderedReferences(count, seed, bits).map((expected, operation) => [`ordered${bits}`, [BigInt(count), BigInt(seed), BigInt(operation)], expected])))),
      ...[0n, 1n, 2n, 3n, 17n, 1025n].map((count) => ["fused_order", [count], 1]),
      ["all_formats", [], 1], ["evaluation_order", [], 123n],
    ],
    traps: [["mismatch", [0n]], ["mismatch", [1n]]],
  },
  computation_extensions: {
    cases: [
      ["match_some", [], 42n], ["match_none", [], 1], ["result_error", [], 4n],
      ["bind_return", [], 1042n], ["bind_two", [], 2042n], ["merge_fallback", [], 42n],
      ["mutable_bind_two", [], 2042n],
      ["source_order", [], 6123n], ["strict_none", [], 2n], ["mutable_group", [], 42n],
      ...[0n, 1n, 10000n].map((count) => ["owned_match", [count], count * 10n]),
    ],
    traps: [["strict_trap", []]],
  },
  simd: {
    cases: [
      ...[8, 16, 32, 64].flatMap((bits) => [false, true].flatMap((unsigned) => [0n, 1n, -1n, 127n, -129n, 2147483647n, -(1n << 63n)].flatMap((seed) => [0n, 1n, BigInt(bits), BigInt(bits + 1)].map((shift) => {
        const cast = (value) => unsigned ? BigInt.asUintN(bits, value) : BigInt.asIntN(bits, value);
        let expected = 0n;
        for (let lane = 0; lane < 128 / bits; lane++) {
          const value = cast(seed + (lane === 0 ? 7n : 0n));
          expected = cast(expected + cast((value * value + value) << (shift & BigInt(bits - 1))));
        }
        return [`vector_i${bits}${unsigned ? "u" : ""}`, [seed, shift], BigInt.asIntN(64, expected)];
      })))),
      ["float_semantics", [], 1], ["float_order", [], 1], ["simd_owned", [], 42n], ["mask_storage", [], 1],
      ...[0n, 1n, 4n].map((index) => ["simd_load", [index], 10n + index * 4n]),
      ["operator_method", [41n], 42n],
    ],
    traps: [["simd_extract_trap", [-1n]], ["simd_extract_trap", [4n]], ["simd_load", [-1n]], ["simd_load", [5n]], ["simd_load", [9223372036854775807n]]],
    inspect(ir) {
      for (const type of ["<16 x i8>", "<8 x i16>", "<4 x i32>", "<2 x i64>", "<4 x float>", "<2 x double>", "<4 x i1>"]) assert.ok(ir.includes(type), type);
      assert.doesNotMatch(ir, /(?:fadd|fmul) fast|add nsw <|add nuw </);
    },
  },
  borrowed_records: {
    cases: [
      ...[0n, 1n, 42n, 10000n].map((count) => ["view_sum", [count], count * (count + 1n)]),
      ["text_view", [], 16n], ["partial_view", [], 13n],
      ["view_iteration", [10000n], 60000n], ["pair_view", [], 3n],
      ["named_view", [], 10n], ["named_reference", [], 42n],
    ],
  },
  iteration_protocol: {
    cases: [
      ["seq_once", [], 42n], ["seq_empty", [], 0n], ["seq_cold", [], 42n],
      ...[0n, 1n, 10n, 1000n, 10000n].flatMap((count) => {
        let filtered = 0n;
        let textLength = 0n;
        for (let value = 0n; value < count; value++) {
          if (value % 3n === 0n) filtered += value * 2n;
          if (String(value).length % 2 === 0) textLength += BigInt(String(value).length);
        }
        return [["seq_range_sum", [count], count * (count - 1n) / 2n], ["seq_list", [count], count * (count - 1n) / 2n], ["seq_map_filter", [count], filtered], ["seq_owned", [count], textLength]];
      }),
      ["seq_borrowed", [], 25n], ["seq_early_exit", [], 100n], ["seq_task", [], 42n],
    ],
    inspect(ir) {
      const next = [...ir.matchAll(/^define internal [^\n]*@tz\.builtin\.Seq\.next[^\n]*\{([\s\S]*?)^\}/gm)];
      assert.ok(next.length > 0);
      for (const [, body] of next) {
        assert.doesNotMatch(body, /@tz\.closure\.clone/);
        assert.match(body, /i1 (?:false|0)\)/);
      }
    },
  },
  map_set: {
    cases: [
      ...[0n, 1n, 33n, 257n, 1024n].flatMap((count) => {
        const map = new Map();
        const text = new Map();
        const left = new Set();
        const right = new Set();
        for (let index = 0n; index < count; index++) {
          map.set(index * 37n % 101n, index * 3n);
          text.set(String(index % 9n), String(index));
          if (index % 2n === 0n) left.add(index);
          if (index % 3n === 0n) right.add(index);
        }
        const checksum = [...map].sort(([left], [right]) => Number(left - right)).reduce((total, [key, value]) => BigInt.asIntN(64, total * 31n + key * 7n + value), 0n);
        const sum = (values) => [...values].reduce((total, value) => total + value, 0n);
        const union = new Set([...left, ...right]);
        const common = [...left].filter((value) => right.has(value));
        const different = [...left].filter((value) => !right.has(value));
        const oddCount = count / 2n;
        return [
          ["map_insert_lookup", [count], checksum],
          ["map_replace_drop", [count], BigInt([...text].reduce((total, [key, value]) => total + key.length + value.length, 0))],
          ["map_remove", [count], 3n * oddCount * oddCount],
          ["set_algebra", [count], sum(union) * 1000000n + sum(common) * 1000n + sum(different)],
        ];
      }),
      ["set_owned_union", [], 2n], ["map_capture", [], 84n], ["map_borrowed_values", [], 8n],
      ["first_representative", [], 1], ["map_task_drop", [], 0n], ["owned_record_keys", [], 42n],
    ],
    traps: [["map_at_missing", []], ["nan_query", []], ["nan_stored", []]],
    inspect(ir) {
      const lookups = [...ir.matchAll(/^define internal [^\n]*@tz\.fn\.Map\.(?:lower_bound|found|get|at|contains_key)[^\n]*\{([\s\S]*?)^\}/gm)];
      assert.ok(lookups.length > 0);
      for (const [, body] of lookups) assert.doesNotMatch(body, /@tz\.(?:alloc|realloc)\(/);
    },
  },
  hierarchical: {
    cases: [
      ["distance_sum", [3n, 4n, 5n, 12n], 18n],
      ["distance_sum", [0n, 0n, 8n, 15n], 17n],
      ["distance_sum", [-3n, -4n, 20n, 21n], 34n],
    ],
  },
  constants: {
    cases: [
      ["integer_values", [], 42n], ["owned_values", [], 42n],
      ["borrowed_values", [], 8n], ["float_values", [], 1],
      ["repeat_values", [100000n], 600000n], ["captured_values", [], 42n],
      ["wide_values", [], 1], ["float_reference", [1.0009765625, 0.00048828125], 1],
    ],
  },
  deriving: {
    cases: [
      ["record_flags", [0n, 1n], 14n], ["record_flags", [1n, 0n], 50n], ["record_flags", [1n, 1n], 50n],
      ["defaults", [], 42n], ["case_order", [], 141414n],
      ["floating", [0n], 2n], ["floating", [1n], 41n], ["floating", [2n], 2n],
      ["owned_words", [], 42n], ["recursive_compare", [], 42n],
    ],
  },
  typeclasses: {
    cases: [
      ...["array_mask", "list_mask", "tuple_mask"].flatMap((name) => [-1n, 0n, 1n].flatMap((left) => [-1n, 0n, 1n].map((right) => [name, [left, right], left === right ? 41n : left < right ? 14n : 50n]))),
      ["prefixes", [], 501414n], ["defaults", [], 42n], ["noncopy_elements", [], 39n],
      ["nested_collections", [], 1414n], ["floating_masks", [0n], 2n], ["floating_masks", [1n], 41n], ["floating_masks", [2n], 2n],
      ["long_lists", [0n], 0n], ["long_lists", [100000n], 100000n], ["staged_comparison", [], 42n],
      ["optional_values", [], 42n],
      ["short_circuits", [], 42n],
    ],
    inspect(ir) {
      const helpers = [...ir.matchAll(/^define [^\n]*@tz\.fn\.\$intrinsic\.(?:Eq|Ord)\.[^\n]*\{([\s\S]*?)^\}/gm)];
      const lists = helpers.map((match) => match[1]).filter((body) => /phi ptr \[[^\n]+\], \[/.test(body));
      assert.ok(lists.length >= 6, "list comparison helpers are present");
      for (const body of lists) {
        assert.equal((body.match(/phi ptr \[[^\n]+\], \[/g) ?? []).length, 2, "both list cursors advance in lockstep");
        assert.ok(!body.includes("@tz.alloc") && !body.includes("@tz.list.index"));
      }
    },
  },
  recursive_types: {
    cases: [
      ["deep_drop", [0n], 0n], ["deep_drop", [100000n], 100000n],
      ["deep_clone", [0n], 3n], ["deep_clone", [50000n], 100003n],
      ["subtree_move", [], 41n], ["consuming_walk", [10000n], 50005000n],
      ["rose_clone", [0n], 3n], ["rose_clone", [25000n], 5n],
      ["list_drop", [25000n], 1n], ["record_clone", [10000n], 13n],
      ["generic_container", [], 42n], ["second_empty", [], 42n],
      ["constructor_value", [], 42n], ["task_payload", [], 42n],
      ["vec_clone", [25000n], 5n], ["mutual_clone", [20000n], 43n],
      ["guard_cleanup", [], 42n], ["task_tree", [10000n], 10000n],
      ["constructor_order", [0n], 42123n], ["constructor_order", [1n], 42123n],
      ["list_clone", [25000n], 5n], ["tail_walk", [100000n], 5000050000n],
      ["alternative_cleanup", [], 42n],
    ],
    nativeCases: [["deep_drop", [1000000n], 1000000n], ["deep_clone", [250000n], 500003n]],
    wasmTraps: [["deep_drop", [1000000n]]],
    traps: [],
  },
  parallel: {
    cases: [
      ...[0n, 1n, 4095n, 4096n, 4097n, 8192n].map((count) => ["parallel_init_sum", [count], count * (count + 1n) / 2n]),
      ...[4194303n, 4194304n, 4194305n].map((count) => ["parallel_large_length", [count], count]),
      ["parallel_slice", [], 19n], ["parallel_strings", [4097n], 20485n],
      ["parallel_copy_arrays", [4097n], 4097n * 4098n / 2n], ["parallel_copy_functions", [], 22n],
      ["parallel_callback_snapshots", [4097n], 8194n], ["parallel_float_special", [], 1n], ["parallel_order", [], 1203n],
      ["parallel_capture_order", [], 53n],
      ["parallel_wrapping", [], -16n],
    ],
    traps: [["parallel_large_length", [-1n]], ["parallel_trap", []]],
  },
  debug_output: {
    cases: [["debug_answer", [], 42n], ["debug_owned", [0n], 0n], ["debug_owned", [1000n], 5000n]],
    traps: [["debug_display_trap", []]],
  },
  active_patterns: {
    cases: [
      ["active_option", [0], 43n], ["active_option", [1], -1n],
      ["active_owned", [0n], 0n], ["active_owned", [64n], 320n], ["active_owned", [10000n], 50000n],
      ["active_parity", [-1n], 1n], ["active_parity", [0n], 0n], ["active_parity", [1n], 1n], ["active_parity", [14n], 0n],
      ["active_repeated_case", [0], 0n], ["active_repeated_case", [1], 5n],
      ["active_multi_owned", [0n], 0n], ["active_multi_owned", [64n], 320n], ["active_multi_owned", [10000n], 50000n],
      ["active_order", [], 3231n], ["active_conservative", [], 2n], ["active_borrowed", [], 8n],
    ],
    traps: [["active_trap", []]],
  },
  string_library: {
    cases: [
      ["character_units", [], 55357n + 4n + 128512n + 13n + 5n + 4n],
      ["all_unit_transfer", [], 65536n * 65535n / 2n],
      ...[0n, 1n, -1n, -1n, -1n, 4n, -1n, 3n, -1n, 3n].map((expected, which) => ["validate_bytes", [which], expected]),
    ],
    traps: [["trap_decode_end", []], ["trap_decode_continuation", []], ["trap_repeat_negative", []], ["trap_repeat_overflow", []]],
  },
  chars: {
    cases: [
      ["all_code_units", [], 65536n], ["character_patterns", [], 42n], ["character_order", [], 1],
      ["parse_characters", [], 1], ["utf8_ascii", [], 1],
      ...[0, 0x7f, 0x80, 0x7ff, 0x800, 0xd7ff, 0xd800, 0xdfff, 0xe000, 0xffff, 0x10000, 0x10ffff, 0x110000, 0xffffffff].map((value) => ["scalar_roundtrip", [value], 1]),
    ],
    traps: [["trap_surrogate", []], ["trap_outside", []]],
    inspect(ir) { assert.match(ir, /switch i16/); assert.match(ir, /switch i32/); },
  },
  vec: {
    cases: [
      ["vec_push_sum", [0n], 0n], ["vec_push_sum", [1n], 0n], ["vec_push_sum", [4096n], 4096n * 4095n / 2n],
      ["vec_pop_order", [0n], 0n], ["vec_pop_order", [4n], 3210n],
      ["vec_growth", [], 408n], ["vec_set_swap", [], 4312n],
      ["vec_strings", [1024n], 5n], ["vec_clone", [], 16n],
      ["vec_get", [-1n], -1n], ["vec_get", [0n], 42n], ["vec_get", [1n], -1n],
      ["vec_capture", [], 44n], ["vec_empty_transfer", [], 0n], ["vec_nested", [], 42n],
    ],
    traps: [["vec_bounds", []], ["vec_negative_capacity", []], ["vec_negative_reserve", []], ["vec_overflow_reserve", []], ["vec_overflow_capacity", []], ["vec_negative_truncate", []]],
  },
  array_bulk: {
    cases: [
      ["array_basics", [], 44n],
      ["borrowed_search", [], 16n],
      ["bulk_callbacks", [], 495n],
      ["bulk_empty", [], 1n],
      ["bulk_short_circuit", [], 1],
      ["bulk_list", [], 448n],
      ["sort_stable_owned", [], [0n, 2n, 4n, 1n, 3n, 5n].reduce((total, ordinal) => total * 10n + ordinal + 6n, 0n)],
      ["sort_float", [], 1],
      ["reductions", [], 1],
      ["first_duplicate", [], 1n],
      ["filtered", [], 135n],
      ...[0, 1, 2, 3, 17, 128, 1024].map((count) => ["sort_numbers", [BigInt(count)],
        Array.from({ length: count }, (_, index) => BigInt((index * 13 + 7) % 19)).sort((left, right) => left < right ? -1 : left > right ? 1 : 0)
          .reduce((total, value, index) => total + BigInt(index + 1) * value, 0n)]),
    ],
    traps: [["trap_sub", []], ["trap_zip", []]],
  },
  consuming_update: {
    cases: [
      ["update_numbers", [4n, 2n, 42n], 46n],
      ["update_numbers", [1n, 0n, -1n], -1n],
      ["update_snapshot", [], 1131n],
      ["update_callback", [], 13n],
      ["update_owned", [0n], 11n],
      ["update_owned", [4096n], 10n],
      ["update_partial", [], 45n],
      ["update_list", [], 13n],
      ["update_list_snapshot", [], 123n],
      ["swap_same", [], 13n],
    ],
    traps: [["trap_update_index", []], ["trap_swap_index", []], ["trap_empty_tail", []]],
  },
  slices: {
    cases: [
      ["slice_sum", [0n, 0n, 0n], 0n],
      ["slice_sum", [10n, 0n, 10n], 45n],
      ["slice_sum", [10n, 3n, 7n], 18n],
      ["slice_sum", [10n, 10n, 10n], 0n],
      ["slice_nested", [], 230n],
      ["slice_string_refs", [], 13n],
      ["slice_copy_owned", [], 23n],
      ["slice_closures", [], 35n],
      ["slice_aggregates", [], 6n],
      ["slice_reborrow", [], 4n],
      ["slice_borrowed_match", [], 8n],
    ],
    traps: [["trap_slice_start", []], ["trap_slice_end", []], ["trap_slice_order", []], ["trap_slice_index", []]],
    inspect(ir) {
      assert.match(ir, /@tz\.fn\.Main\.sum\(%tz\.array/);
      assert.match(ir, /icmp ule i64/);
    },
  },
  borrowed_comparisons: {
    cases: [
      ["compare_loop", [0n], 0n],
      ["compare_loop", [1n], 5n],
      ["compare_loop", [20000n], 100000n],
      ["method_scalar", [42n, 42n], 1],
      ["method_scalar", [42n, 43n], 0],
      ["method_float", [0, -0], 1],
      ["method_float", [1, 2], 1],
      ["method_record", [], 4n],
      ["comparison_order", [], 112n],
      ["comparison_jump", [], 1n],
    ],
    inspect(ir) {
      assert.match(ir, /icmp eq i64/);
      assert.match(ir, /fcmp olt double/);
      assert.match(ir, /@tz\.string\.equal/);
      assert.doesNotMatch(ir, /fcmp fast/);
    },
  },
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
      ["propagation_first_owned_error", [], 11n],
      ["propagation_explicit_conversion", [], 42n],
      ["propagation_selected_branch", [1], 42n],
      ["propagation_eager_let", [1n], 42n],
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
      ["trap_propagation_eager_let", []],
      ["trap_propagation_statement", []],
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

const stringSamples = ["", "hello hello", "l", "a\0b", "\u{1f600}", "\ue000", " \tAbC\r\n", "\ud800", "\ude00", "aa"];
for (const count of [0, 1, 4095, 4096, 4097, 8192]) {
  const chunks = Math.min(1024, Math.ceil(count / 4096));
  const fold = (identity, initial, value, combine) => {
    if (!chunks) return identity;
    const partials = [];
    for (let chunk = 0; chunk < chunks; chunk++) {
      const first = Math.floor(count / chunks) * chunk + Math.floor((count % chunks) * chunk / chunks);
      const last = Math.floor(count / chunks) * (chunk + 1) + Math.floor((count % chunks) * (chunk + 1) / chunks);
      let total = initial;
      for (let index = first; index < last; index++) total = combine(total, value(index));
      partials.push(total);
    }
    return partials.reduce(combine, identity);
  };
  suites.parallel.cases.push(["parallel_subtract", [BigInt(count)], fold(10n, 10n, (index) => BigInt(index % 7), (left, right) => BigInt.asIntN(64, left - right))]);
  suites.parallel.cases.push(["parallel_copy_reduce", [BigInt(count)], fold(1n, 1n, BigInt, (left, right) => left + right)]);
  suites.parallel.cases.push(["parallel_float", [BigInt(count)], BigInt(fold(0, 0, (index) => index % 3 === 0 ? 1e16 : index % 3 === 1 ? 1 : -1e16, (left, right) => left + right))]);
}
const wrapHash = (value) => BigInt.asIntN(64, value);
function textHash(text, utf8) {
  const units = utf8 ? [...Buffer.from(text, "utf8")] : text.split("").map((character) => character.charCodeAt(0));
  return units.reduce((result, value) => wrapHash(result * 31n + BigInt(value)), BigInt(units.length));
}
for (const utf8 of [false, true]) {
  const indices = stringSamples.map((_, index) => index).filter((index) => !utf8 || ![7, 8].includes(index));
  for (const first of indices) for (const second of indices) {
    const text = stringSamples[first], needle = stringSamples[second];
    const textBytes = Buffer.from(text, "utf8"), needleBytes = Buffer.from(needle, "utf8");
    const parts = needle === "" && utf8 ? Array.from(text) : text.split(needle);
    const emptyReplaced = utf8 ? (text === "" ? "-" : `-${Array.from(text).join("-")}-`) : text.replaceAll("", "-");
    const values = [
      utf8 ? textBytes.indexOf(needleBytes) : text.indexOf(needle),
      utf8 ? textBytes.lastIndexOf(needleBytes) : text.lastIndexOf(needle),
      Number(text.includes(needle)), Number(text.startsWith(needle)), Number(text.endsWith(needle)),
      textHash(needle === "" ? emptyReplaced : text.replaceAll(needle, "-"), utf8),
      parts.reduce((total, part) => wrapHash(total * 31n + textHash(part, utf8)), BigInt(parts.length)),
      textHash(text + needle, utf8), textHash(`${text}-${needle}`, utf8),
      textHash(text.replace(/^[\t\n\v\f\r ]+|[\t\n\v\f\r ]+$/g, ""), utf8),
      textHash(text.replace(/[A-Z]/g, (letter) => letter.toLowerCase()), utf8),
      textHash(text.replace(/[a-z]/g, (letter) => letter.toUpperCase()), utf8),
      utf8 ? Math.sign(Buffer.compare(textBytes, needleBytes)) : text < needle ? -1 : text > needle ? 1 : 0,
      textHash(text.repeat(2), utf8),
    ];
    for (const [operation, expected] of values.entries()) suites.string_library.cases.push([utf8 ? "utf8_operation" : "text_operation", [operation, BigInt(first), BigInt(second)], BigInt(expected)]);
  }
  for (const which of indices) {
    const text = stringSamples[which], bytes = Buffer.from(text, "utf8"), length = utf8 ? bytes.length : text.length;
    for (let first = -1; first <= length + 1; first++) for (let last = -1; last <= length + 1; last++) {
      const boundary = (offset) => offset === 0 || offset === length || (bytes[offset] & 0xc0) !== 0x80;
      const valid = first >= 0 && first <= last && last <= length && (!utf8 || (boundary(first) && boundary(last)));
      const expected = valid ? textHash(utf8 ? bytes.subarray(first, last).toString("utf8") : text.slice(first, last), utf8) : -1n;
      suites.string_library.cases.push([utf8 ? "utf8_slice" : "text_slice", [BigInt(which), BigInt(first), BigInt(last)], expected]);
    }
  }
}

function hashBytes(bytes) {
  return bytes.reduce((hash, byte) => BigInt.asUintN(64, (hash ^ BigInt(byte)) * 1099511628211n), 14695981039346656037n);
}
function littleEndian(value, bytes = 8) {
  return Array.from({ length: bytes }, (_, index) => Number((BigInt(value) >> BigInt(index * 8)) & 255n));
}
function scalarHash(tag, value, bytes) {
  return BigInt.asIntN(64, hashBytes([...littleEndian(tag), ...littleEndian(value, bytes)]));
}
for (let kind = 0; kind < 10; kind++) {
  const bytes = 1 << (kind % 5);
  for (const seed of [min, -1n, 0n, 1n, 127n, 256n, max]) {
    suites.deriving.cases.push(["hash_integer", [BigInt(kind), seed], scalarHash(0x10 + kind % 5, bytes === 16 ? seed << 72n : seed, bytes)]);
  }
}
for (const [kind, bits, fraction, bias] of [[0, 16, 10, 15], [1, 32, 23, 127], [2, 64, 52, 1023], [3, 128, 112, 16383]]) {
  const sign = 1n << BigInt(bits - 1);
  const infinity = BigInt(bias * 2 + 1) << BigInt(fraction);
  const values = [0n, 0n, BigInt(bias) << BigInt(fraction) | 1n << BigInt(fraction - 1), sign | BigInt(bias + 1) << BigInt(fraction) | 1n << BigInt(fraction - 2), infinity, sign | infinity, infinity | 1n << BigInt(fraction - 1)];
  values.forEach((value, which) => suites.deriving.cases.push(["hash_real", [BigInt(kind), BigInt(which)], scalarHash(0x21 + kind, value, bits / 8)]));
}
for (const [kind, bits, fraction, bias] of [[0, 32, 23, 101], [1, 64, 53, 398], [2, 128, 113, 6176]]) {
  const sign = 1n << BigInt(bits - 1);
  const values = [BigInt(bias - 1) << BigInt(fraction) | 12n, BigInt(bias - 1) << BigInt(fraction) | 12n, BigInt(bias) << BigInt(fraction), BigInt(bias) << BigInt(fraction), sign | BigInt(bias) << BigInt(fraction) | 12n, 31n << BigInt(bits - 6), 30n << BigInt(bits - 6), sign | 30n << BigInt(bits - 6)];
  values.forEach((value, which) => suites.deriving.cases.push(["hash_decimal", [BigInt(kind), BigInt(which)], scalarHash(0x32 + kind, value, bits / 8)]));
}
for (const [which, text] of ["hello", "A\n\"\\\0", "\uD800", "\u{1F600}"].entries()) {
  const units = Array.from({ length: text.length }, (_, index) => text.charCodeAt(index));
  suites.deriving.cases.push(["hash_text", [BigInt(which)], BigInt.asIntN(64, hashBytes([...littleEndian(0x53), ...littleEndian(units.length), ...units.flatMap((unit) => littleEndian(unit, 2))]))]);
}
for (const [which, text] of ["hello", "\u{1F600}"].entries()) {
  const bytes = [...Buffer.from(text)];
  suites.deriving.cases.push(["hash_utf8", [BigInt(which)], BigInt.asIntN(64, hashBytes([...littleEndian(0x73), ...littleEndian(bytes.length), ...bytes]))]);
}
for (const [which, value] of [0, 39, 0xD800, 0x3042, 0x1F600].entries()) {
  suites.deriving.cases.push(["hash_character", [BigInt(which)], scalarHash(which < 3 ? 0x43 : 0x63, value, which < 3 ? 2 : 4)]);
}
for (const value of [0, 1]) suites.deriving.cases.push(["hash_boolean", [value], scalarHash(1, value, 8)]);
suites.deriving.cases.push(["hash_unit", [], BigInt.asIntN(64, hashBytes(littleEndian(2)))]);
function compositeHash(tag, components) {
  return hashBytes([...littleEndian(tag), ...littleEndian(components.length), ...components.flatMap((hash, index) => [...littleEndian(index), ...littleEndian(hash)])]);
}
function unionHash(index, payload = -1n) {
  return hashBytes([...littleEndian(0x55), ...littleEndian(index), ...littleEndian(payload)]);
}
const integerHashes = [1n, 2n, 3n].map((value) => scalarHash(0x13, value, 8));
const textHashAbc = hashBytes([...littleEndian(0x53), ...littleEndian(3), ...[97, 98, 99].flatMap((unit) => littleEndian(unit, 2))]);
const structuralHashes = [
  compositeHash(0x52, integerHashes.slice(0, 2)), unionHash(0), unionHash(1, scalarHash(0x13, 42n, 8)),
  unionHash(2, compositeHash(0x54, [scalarHash(0x23, 0x3ff8000000000000n, 8), scalarHash(0x23, 0xc004000000000000n, 8)])),
  unionHash(1, compositeHash(0x54, [unionHash(0), scalarHash(0x13, 42n, 8), unionHash(0)])),
  compositeHash(0x41, integerHashes), compositeHash(0x4c, integerHashes),
  compositeHash(0x54, [scalarHash(0x13, 42n, 8), textHashAbc]), compositeHash(0x52, []),
];
structuralHashes.forEach((hash, which) => suites.deriving.cases.push(["hash_structure", [BigInt(which)], BigInt.asIntN(64, hash)]));
suites.deriving.cases.push(["hash_default_decimal", [], scalarHash(0x33, 398n << 53n, 8)]);
function canonicalTextHash(text) {
  return BigInt.asIntN(64, hashBytes([...littleEndian(0x53), ...littleEndian(text.length), ...Array.from({ length: text.length }, (_, index) => text.charCodeAt(index)).flatMap((unit) => littleEndian(unit, 2))]));
}
function quotedLiteral(text, kind) {
  const delimiter = kind >= 2 ? "'" : '"';
  const prefix = kind % 2 ? "u8" : "";
  const escapes = new Map([[0, "0"], [9, "t"], [10, "n"], [13, "r"]]);
  const content = [...text].map((character) => {
    const scalar = character.codePointAt(0);
    if (character === delimiter || character === "\\") return `\\${character}`;
    if (escapes.has(scalar)) return `\\${escapes.get(scalar)}`;
    if (scalar < 32 || scalar === 127 || (scalar >= 0xd800 && scalar <= 0xdfff)) {
      const hex = scalar.toString(16).toUpperCase().padStart(4, "0");
      return prefix ? `\\u{${hex}}` : `\\u${hex}`;
    }
    return character;
  }).join("");
  return `${prefix}${delimiter}${content}${delimiter}`;
}
for (const value of [0, 1, 9, 10, 13, 31, 34, 39, 92, 127, 128, 0x3042, 0xd7ff, 0xd800, 0xdbff, 0xdc00, 0xdfff, 0xe000, 0xffff]) {
  suites.deriving.cases.push(["display_character", [BigInt(value)], canonicalTextHash(`Quote { value: ${quotedLiteral(String.fromCharCode(value), 2)} }`)]);
}
for (const value of [0, 1, 9, 10, 13, 39, 92, 127, 128, 0x3042, 0xffff, 0x1f600, 0x10ffff]) {
  suites.deriving.cases.push(["display_utf8_character", [BigInt(value)], canonicalTextHash(`Quote { value: ${quotedLiteral(String.fromCodePoint(value), 3)} }`)]);
}
for (const [which, text] of ["", "A\n\r\t\"\\\0\u0001\u001F\u007F", "\uD800x\uDC00", "\uD800\uD801\uDC01\uDC02x", "\u{1F600}", "'plain'"].entries()) {
  suites.deriving.cases.push(["display_text", [BigInt(which)], canonicalTextHash(`Quote { value: ${quotedLiteral(text, 0)} }`)]);
}
for (const [which, text] of ["A\n\r\t\"\\\0\u0001\u001F\u007F", "\u{1F600}"].entries()) {
  suites.deriving.cases.push(["display_utf8_text", [BigInt(which)], canonicalTextHash(`Quote { value: ${quotedLiteral(text, 1)} }`)]);
}
suites.deriving.cases.push(["display_point", [], canonicalTextHash("Point { x: 1, y: 2 }")]);
for (const [which, text] of ["First", "Number 42", "Pair (1.5, -2.5)", "Node (Leaf, 42, Leaf)", "Empty { }", '[("a\\n", 1), ("b", 2)]', '[|"a", "b"|]', "[||]", "Quote { value: Point { x: 1, y: 2 } }"].entries()) {
  suites.deriving.cases.push(["display_structure", [BigInt(which)], canonicalTextHash(text)]);
}
for (const count of [0, 1, 10000]) {
  suites.deriving.cases.push(["display_large", [BigInt(count)], canonicalTextHash(`[${Array.from({ length: count }, (_, index) => index).join(", ")}]`)]);
}

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
    let trackedIr = sourceIr.replaceAll("@malloc", "@tracked_alloc").replaceAll("@free", "@tracked_free").replaceAll("@realloc", "@tracked_realloc");
    if (sanitizerKind) trackedIr = trackedIr.replaceAll(" nounwind {", ` nounwind sanitize_${sanitizerKind} {`);
    writeFileSync(ir, trackedIr);
    const traps = suite.traps ?? [];
    const host = join(temporary, "host.c");
    writeFileSync(host, `
#include <assert.h>
#include <math.h>
#include <stdint.h>
#include <stdatomic.h>
#include <stdlib.h>
#include "tz-${name}.h"
static _Atomic uint64_t live;
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
  void *tracked_realloc(void *value, uint64_t size) {
    if (!value) return tracked_alloc(size);
    if (!size) { tracked_free(value); return NULL; }
    uint64_t *previous = (uint64_t *)value - 2;
    assert(previous[1] == UINT64_C(0x51a110ca7e));
    uint64_t old_size = previous[0];
    uint64_t *next = realloc(previous, (size_t)size + 16);
    assert(next);
    next[0] = size;
    next[1] = UINT64_C(0x51a110ca7e);
    if (size >= old_size) live += size - old_size; else live -= old_size - size;
    return next + 2;
  }
int main(int argc, char **argv) {
    if (argc == 2) {
        switch (atoi(argv[1])) {
            ${traps.map(([trap, args], index) => `case ${index}: (void)tz_${trap}(${args.map(cValue).join(", ")}); break;`).join("\n")}
        }
        return 0;
    }
    ${[...suite.cases, ...suite.nativeCases ?? []].map(([exported, args, expected]) =>
    `assert(tz_${exported}(${args.map(cValue).join(", ")}) == ${cValue(expected)}); assert(live == 0);`).join("\n    ")}
    return 0;
}
`);
    for (const optimization of ["0", "3"]) {
      const native = join(temporary, `${name}-O${optimization}`);
      execute(clang, [`-O${optimization}`, "-Wno-override-module", "-ffp-contract=off", ...sanitizer, ...nativeOptions,
        `-I${temporary}`, ir, host, ...(sourceIr.includes("declare void @tsuzuri_task_parallel(") ? [join(root, "src/runtime/task.c"), "-pthread"] : []), "-lm", "-o", native]);
      execute(native, []);
      if (name === "parallel" && optimization === "3") {
        for (const processors of [1, 4]) {
          const runtime = join(temporary, `runtime-${processors}.c`);
          const binary = join(temporary, `parallel-cpus-${processors}`);
          writeFileSync(runtime, `#define TZ_TASK_SYSCONF(name) ${processors}\n#include ${JSON.stringify(join(root, "src/runtime/task.c"))}\n`);
          execute(clang, ["-O3", "-Wno-override-module", "-ffp-contract=off", ...sanitizer,
            `-I${temporary}`, ir, host, runtime, "-pthread", "-lm", "-o", binary]);
          execute(binary, []);
        }
      }
      for (let index = 0; index < traps.length; index++) {
        const result = execute(native, [String(index)], false);
        assert.ok(result.status !== 0 || result.signal, `${name} native O${optimization}: ${traps[index][0]} must trap`);
      }
      const wasm = join(temporary, `${name}-O${optimization}.wasm`);
      cli(["build", fixture, "--target", "wasm32", ...wasmOptions, `-O${optimization}`, "-o", wasm]);
      const module = new WebAssembly.Module(readFileSync(wasm));
      assert.deepEqual(WebAssembly.Module.imports(module), [], `${name}: WASM has no imports`);
      const exports = new WebAssembly.Instance(module).exports;
      for (const [exported, args, expected] of suite.cases) {
        assert.equal(exports[`tz_${exported}`](...args), expected, `${name} WASM O${optimization}: ${exported}(${args})`);
        assert.ok(exports.memory.buffer.byteLength <= 16 * 1024 * 1024, `${name}: WASM stays within 16 MiB`);
      }
      for (const [trap, args] of [...traps, ...suite.wasmTraps ?? []]) {
        const fresh = new WebAssembly.Instance(module).exports;
        assert.throws(() => fresh[`tz_${trap}`](...args), WebAssembly.RuntimeError, trap);
      }
      if (name === "recursive_types") {
        const measured = join(temporary, `recursive-traps-${optimization}.wasm`);
        cli(["build", fixture, "--target", "wasm32", ...wasmOptions, "--trap-info", `-O${optimization}`, "-o", measured]);
        const checked = new WebAssembly.Instance(new WebAssembly.Module(readFileSync(measured))).exports;
        assert.equal(checked.tz_deep_clone(50000n), 100003n);
        assert.equal(checked.tz_mutual_clone(20000n), 43n);
        assert.throws(() => checked.tz_deep_drop(1000000n), WebAssembly.RuntimeError);
        assert.ok(checked.tsuzuri_trap_site() > 0);
      }
    }
    return suite.cases.length;
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
}

function debugOutputChecks() {
  const directory = mkdtempSync(join(tmpdir(), "tsuzuri-debug-output-"));
  const fixture = join(root, "tests/fixtures/debug_output");
  try {
    for (const optimization of [0, 3]) {
      const native = join(directory, `debug-${optimization}`);
      cli(["build", fixture, `-O${optimization}`, "-o", native]);
      const result = execute(native, []);
      assert.equal(result.stdout, "42\n");
      assert.equal(result.stderr, "hello\0\u{1f600}\n42\n");
      const wasm = join(directory, `debug-${optimization}.wasm`);
      cli(["build", fixture, "--target", "wasm32", "--debug-output", `-O${optimization}`, "-o", wasm]);
      const module = new WebAssembly.Module(readFileSync(wasm));
      assert.deepEqual(WebAssembly.Module.imports(module), [{ module: "tsuzuri_debug", name: "write", kind: "function" }]);
      const lines = [];
      let instance;
      instance = new WebAssembly.Instance(module, { tsuzuri_debug: { write(pointer, length) {
        lines.push(Buffer.from(new Uint8Array(instance.exports.memory.buffer, pointer, Number(length))).toString("utf8"));
      } } });
      assert.equal(instance.exports.tz_debug_answer(), 42n);
      assert.deepEqual(lines, ["hello\0\u{1f600}", "42"]);
      assert.throws(() => instance.exports.tz_debug_display_trap(), WebAssembly.RuntimeError);
    }
    const unused = join(directory, "Unused.tz"), base = join(directory, "unused.wasm"), opted = join(directory, "unused-opted.wasm");
    writeFileSync(unused, "export def value :: i64\nfn value = 1");
    cli(["build", unused, "--target", "wasm32", "-o", base]);
    cli(["build", unused, "--target", "wasm32", "--debug-output", "-o", opted]);
    assert.deepEqual(readFileSync(base), readFileSync(opted));
    const parallel = mkdtempSync(join(directory, "parallel-"));
    const parallelSource = join(parallel, "Main.tz");
    writeFileSync(parallelSource, "let tasks = new [Task<i64>](16, index -> task { return Debug.trace index })\nlet results = Task.run (Task.parallel tasks)\nArray.sum (ref results)");
    for (const optimization of [0, 3]) {
      const native = join(directory, `parallel-${optimization}`);
      cli(["build", parallelSource, `-O${optimization}`, "-o", native]);
      const result = execute(native, []);
      assert.equal(result.stdout, "120\n");
      assert.equal(result.stderr.split("\n").length - 1, 16);
      assert.equal(result.stderr.replaceAll("\n", "").split("").sort().join(""), Array.from({ length: 16 }, (_, index) => String(index)).join("").split("").sort().join(""));
    }
    const ir = join(directory, "short-write.ll"), host = join(directory, "short-write.c");
    const runtime = readFileSync(join(root, "src/runtime/debug.ll"), "utf8").replaceAll("@write(", "@mock_write(").replaceAll("@tz.free(", "@mock_free(");
    writeFileSync(ir, `%tz.utf8string = type { ptr, i64 }\ndeclare void @llvm.trap()\ndeclare void @mock_free(ptr)\n@input = private constant [6 x i8] c"ab\\00cde"\n${runtime}\ndefine void @probe() { call void @tz.debug.write(%tz.utf8string { ptr @input, i64 6 }) ret void }\n`);
    writeFileSync(host, `#include <assert.h>\n#include <stdint.h>\n#include <string.h>\nstatic char output[7]; static uint64_t length; static int mode, freed;\nextern void probe(void);\nint64_t mock_write(int fd, const char *text, uint64_t size) { assert(fd == 2); if(mode == 1) return 0; if(mode == 2) return -1; uint64_t count = size > 2 ? 2 : size; assert(length + count <= 7); memcpy(output + length, text, count); length += count; return count; }\nvoid mock_free(void *pointer) { assert(pointer); ++freed; }\nint main(int argc, char **argv) { mode = argc > 1 ? argv[1][0] - '0' : 0; probe(); assert(length == 7 && memcmp(output, "ab\\0cde\\n", 7) == 0 && freed == 1); return 0; }\n`);
    for (const optimization of [0, 3]) {
      const native = join(directory, `short-${optimization}`);
      execute(clang, [`-O${optimization}`, "-Wno-override-module", ir, host, "-o", native]);
      execute(native, []);
      for (const mode of ["1", "2"]) {
        const result = execute(native, [mode], false);
        assert.notEqual(result.status, 0);
        assert.notEqual(result.error?.code, "ETIMEDOUT");
      }
    }
  } finally { rmSync(directory, { recursive: true, force: true }); }
}

function characterConsoleChecks() {
  const directory = mkdtempSync(join(tmpdir(), "tsuzuri-character-console-"));
  const source = join(directory, "Main.tz");
  try {
    for (const optimization of [0, 3]) {
      for (const [literal, expected] of [["'\\0'", "\0"], ["'\\u3042'", "\u3042"], ["u8'\\u{3042}'", "\u3042"], ["u8'\\u{1F600}'", "\u{1f600}"], ["u8'\\u{10FFFF}'", "\u{10ffff}"]]) {
        writeFileSync(source, literal);
        assert.equal(cli(["run", source, `-O${optimization}`]).stdout, `${expected}\n`);
      }
      for (const literal of ["'\\uD800'", "'\\uDFFF'"]) {
        writeFileSync(source, literal);
        const result = execute(compiler, ["run", source, `-O${optimization}`], false);
        assert.notEqual(result.status, 0);
        assert.equal(result.stdout, "");
      }
    }
  } finally { rmSync(directory, { recursive: true, force: true }); }
}

function wasmReallocationChecks() {
  const directory = mkdtempSync(join(tmpdir(), "tsuzuri-realloc-"));
  try {
    const input = join(directory, "realloc.ll");
    writeFileSync(input, `declare void @llvm.trap()\n${readFileSync(join(root, "src/runtime/heap-wasm.ll"), "utf8")}
define ptr @allocate(i64 %size) { %value = call ptr @tz.alloc(i64 %size) ret ptr %value }
define ptr @resize(ptr %old, i64 %previous, i64 %size) { %value = call ptr @tz.realloc(ptr %old, i64 %previous, i64 %size) ret ptr %value }
define void @release(ptr %value) { call void @tz.free(ptr %value) ret void }
`);
    for (const optimization of [0, 3]) {
      const object = join(directory, `realloc-${optimization}.o`), output = join(directory, `realloc-${optimization}.wasm`);
      execute(clang, ["--target=wasm32", `-O${optimization}`, "-Wno-override-module", "-c", input, "-o", object]);
      execute(process.env.TSUZURI_WASM_LD ?? "wasm-ld", [object, "--no-entry", "--export=allocate", "--export=resize", "--export=release", "--export-memory", "--max-memory=16777216", "-o", output]);
      const module = new WebAssembly.Module(readFileSync(output));
      assert.deepEqual(WebAssembly.Module.imports(module), []);
      const fresh = () => new WebAssembly.Instance(module).exports;
      for (const neighborSize of [32n, 128n]) {
        const api = fresh();
        const original = api.allocate(32n), neighbor = api.allocate(neighborSize), guard = api.allocate(16n);
        new Uint8Array(api.memory.buffer, original, 32).fill(0x5a);
        api.release(neighbor);
        const grown = api.resize(original, 32n, neighborSize === 32n ? 70n : 80n);
        assert.equal(grown, original);
        assert.ok(new Uint8Array(api.memory.buffer, grown, 32).every((value) => value === 0x5a));
        if (neighborSize === 128n) {
          const remainder = api.allocate(64n);
          assert.equal(remainder, original + 96);
          api.release(remainder);
        }
        api.release(grown);
        api.release(guard);
        const combined = api.allocate(neighborSize === 32n ? 100n : 200n);
        assert.equal(combined, original);
        api.release(combined);
      }
      {
        const api = fresh();
        const original = api.allocate(32n), blocker = api.allocate(32n);
        new Uint8Array(api.memory.buffer, original, 32).fill(0x6b);
        const moved = api.resize(original, 32n, 96n);
        assert.notEqual(moved, original);
        assert.ok(new Uint8Array(api.memory.buffer, moved, 32).every((value) => value === 0x6b));
        api.release(blocker);
        api.release(moved);
      }
      {
        const api = fresh();
        assert.equal(api.resize(0, 0n, 0n), 0);
        const allocated = api.resize(0, 0n, 32n);
        assert.notEqual(allocated, 0);
        assert.equal(api.resize(allocated, 32n, 16n), allocated);
        assert.equal(api.resize(allocated, 16n, 0n), 0);
        assert.equal(api.allocate(32n), allocated);
        assert.throws(() => api.resize(0, 0n, 16777185n), WebAssembly.RuntimeError);
      }
    }
  } finally { rmSync(directory, { recursive: true, force: true }); }
  console.log("WASM realloc: split, absorb, fallback, null, zero and coalescing passed at O0/O3");
}

let total = 0;
for (const [name, suite] of Object.entries(suites)) {
  if (only && only !== name) continue;
  total += run(name, suite);
  if (name === "vec") wasmReallocationChecks();
  if (name === "chars") characterConsoleChecks();
  if (name === "debug_output") debugOutputChecks();
  console.log(`${name}: native/WASM at O0/O3 passed`);
}
assert.ok(total > 0, "no feature suite ran");
console.log(`features: ${total} cases passed`);
