# C12 付録 2: テスト・デバッガ表示・文書の文面

[C12 本体](C12-ordered-map-set-chunked.md)の付録 2。`std` の全文とベンチマークは [C12-appendix.md](C12-appendix.md)。
2026-10-23: A・B・C・D と、E・G を含む実 LLDB suite を検証済み。
E は lazy に作る Vec ヘッダの合成ビューを外す必要があり、H にもその回帰を反映した。
F は XML 整形式と PDB 埋め込みを確認済み（Visual Studio の実 UI 表示は未確認）。
永続的なモック試験は `tests/lldb_collections.py`、実測と検証数は本体の実装・検証記録を参照。

## A. `tests/fixtures/map_set_chunks/Main.tz`（新規。全文。423 行・20,241 バイト）

E2E suite `map_set_chunks`（`export def`）と、`tsuzuri test` が走らせる性質テスト（末尾の 5 つの `test`）を 1 つのファイルに持つ。
インデントは空白 4 つ。コメント中の「ticket」は本チケットを指す。

<!-- file: tests/fixtures/map_set_chunks/Main.tz -->
```text
// C12: the chunks of Map and Set. Build with `tsuzuri build` for the E2E suite `map_set_chunks`; run `tsuzuri test` for the property tests at the end.

// ---------------------------------------------------------------------------------------------------------------------
// Where the chunks split, merge and disappear. Every expected value is worked out by hand in the ticket (C12 "テスト計画").

def sequence :: i64 -> i64 -> i64 -> Map<i64, i64>
fn sequence first step count =
    let mut map: Map<i64, i64> = Map.empty()
    for index in 0i64 .. count - 1 do map = Map.insert (first + index * step) index map
    map

// The number of chunks, or -1 when the chunks break one of their rules.
def chunks :: ref Map<i64, i64> -> i64
fn chunks map = if Map.invariants_hold map then Map.chunk_count map else -1

// Ascending keys fill a chunk to 512 and then start a new one.
export def ascending :: i64 -> i64
fn ascending count =
    let map = sequence 0 1 count
    chunks (ref map)

// Descending keys always go to the front of the first chunk, which splits in two halves when it is full.
export def descending :: i64 -> i64
fn descending count =
    let map = sequence (count - 1) (-1) count
    chunks (ref map)

// 512 even keys fill one chunk; the odd key 1 goes to the middle of the full chunk and splits it: 1 chunk, then 2.
export def middle_split :: i64
fn middle_split =
    let map = sequence 0 2 512
    let before = chunks (ref map)
    let after = Map.insert 1 99 map
    before * 1000 + chunks (ref after)

// 513 keys are 2 chunks of 257 and 256. Removing the 256 keys that are 2 mod 4 leaves 257, and one more removal leaves 256, which merges the two:
// the chunks before the removals, after 256 of them, after the last one, and the keys that are left.
export def merge_after_removals :: i64
fn merge_after_removals =
    let mut map = sequence 0 2 512
    map = Map.insert 1 99 map
    let start = chunks (ref map)
    for index in 0i64 .. 255 do map = Map.remove (2 + index * 4) map
    let before = chunks (ref map)
    map = Map.remove 0 map
    let after = chunks (ref map)
    start * 10000000 + before * 1000000 + after * 1000 + Map.length (ref map)

// 768 ascending keys are chunks of 512 and 256. Removing the first 512 keys leaves the first chunk with 1 key, and removing that one deletes the
// first chunk: the chunks before and after, and the keys that are left.
export def head_deleted :: i64
fn head_deleted =
    let mut map = sequence 0 1 768
    let before = chunks (ref map)
    for key in 0i64 .. 511 do map = Map.remove key map
    let after = chunks (ref map)
    before * 100000 + after * 1000 + Map.length (ref map)

// 513 ascending keys are chunks of 512 and 1. Removing key 512 deletes the second chunk.
export def tail_deleted :: i64
fn tail_deleted =
    let map = sequence 0 1 513
    let before = chunks (ref map)
    let map = Map.remove 512 map
    before * 1000 + chunks (ref map)

// The sets A = 0..999 and B = 500..1499 give a union of 1,500 keys and an intersection and a difference of 500 keys. The chunks of what
// `union`, `intersect` and `difference` build hold 384 keys, except for the first, which holds what is left: 4, 2 and 2 chunks.
export def set_algebra_chunks :: i64
fn set_algebra_chunks =
    let mut left: Set<i64> = Set.empty()
    let mut right: Set<i64> = Set.empty()
    for key in 0i64 .. 999 do left = Set.insert key left
    for key in 500i64 .. 1499 do right = Set.insert key right
    let common = Set.intersect (ref left) (ref right)
    let different = Set.difference (ref left) (ref right)
    let both = Set.union left right
    assert (Set.invariants_hold (ref common) && Set.invariants_hold (ref different) && Set.invariants_hold (ref both))
    Set.chunk_count (ref both) * 10000 + Set.chunk_count (ref common) * 100 + Set.chunk_count (ref different)

// ---------------------------------------------------------------------------------------------------------------------
// The calls of `Ord.lt`. A key that holds a borrow of an Atomic counts them.

record Probe { value: i64, hits: ref Atomic<i64> }

instance Eq<Probe> {
    fn eq left right = left.value == right.value
    fn ne left right = left.value != right.value
}

instance Ord<Probe> {
    fn lt left right =
        let _before = Atomic.fetch_add left.hits 1
        left.value < right.value
    fn le left right = !(Ord.lt right left)
    fn gt left right = Ord.lt right left
    fn ge left right = !(Ord.lt left right)
}

def next_state :: i64u -> i64u
fn next_state state = state * 6364136223846793005 + 1442695040888963407

// floor(log2 n) for n >= 1.
def log2 :: i64 -> i64
fn log2 n =
    let mut value = n
    let mut result = 0
    while value > 1 do
        value = value / 2
        result = result + 1
    result

// The most `Ord.lt` calls of one lookup or removal in a map of `count` chunks: the binary search over the first count - 1 chunks makes
// floor(log2 (count - 1)) + 1 calls (none for one chunk), the binary search in a chunk of at most 512 keys makes 10, and comparing with the key found makes 1.
def lookup_bound :: i64 -> i64
fn lookup_bound count = (if count <= 1 then 0 else log2 (count - 1) + 1) + 11

// An insertion compares with the last key first, so it makes 1 call more.
def insert_bound :: i64 -> i64
fn insert_bound count = lookup_bound count + 1

// `size` random keys are inserted, looked up and removed. The result is 0 when no operation makes more calls than its bound and no phase makes more than
// size * (floor(log2 size) + 6) calls; otherwise it is 11, 12 or 13 for the first operation of the phase (insert, lookup, remove) over its bound,
// and 21, 22 or 23 for the first phase over its total.
export def comparison_bounds :: i64 -> i64
fn comparison_bounds size =
    let hits = Atomic.create 0i64
    let mut map: Map<Probe, i64> = Map.empty()
    let mut failure = 0
    let total_bound = size * (log2 size + 6)
    let mut state = 88172645463325252
    let mut phase_start = 0
    for index in 0i64 .. size - 1 do
        state = next_state state
        let before = Atomic.load (ref hits)
        let allowed = insert_bound (Map.chunk_count (ref map))
        map = Map.insert (Probe { value: (state >>> 20) as i64, hits: ref hits }) index map
        if Atomic.load (ref hits) - before > allowed && failure == 0 then failure = 11
    if Atomic.load (ref hits) - phase_start > total_bound && failure == 0 then failure = 21
    phase_start = Atomic.load (ref hits)
    state = 88172645463325252
    for _index in 0i64 .. size - 1 do
        state = next_state state
        let before = Atomic.load (ref hits)
        let found = Map.contains_key (Probe { value: (state >>> 20) as i64, hits: ref hits }) (ref map)
        assert found
        if Atomic.load (ref hits) - before > lookup_bound (Map.chunk_count (ref map)) && failure == 0 then failure = 12
    if Atomic.load (ref hits) - phase_start > total_bound && failure == 0 then failure = 22
    phase_start = Atomic.load (ref hits)
    state = 88172645463325252
    for _index in 0i64 .. size - 1 do
        state = next_state state
        let before = Atomic.load (ref hits)
        let allowed = lookup_bound (Map.chunk_count (ref map))
        map = Map.remove (Probe { value: (state >>> 20) as i64, hits: ref hits }) map
        if Atomic.load (ref hits) - before > allowed && failure == 0 then failure = 13
    if Atomic.load (ref hits) - phase_start > total_bound && failure == 0 then failure = 23
    assert (Map.is_empty (ref map))
    failure

// The multiples of 2 and of 3 below 2 * size and 3 * size: two sets of `size` keys that count their comparisons in `hits`.
def probe_sets :: ref Atomic<i64> -> i64 -> (Set<Probe> * Set<Probe>)
fn probe_sets hits size =
    let mut left: Set<Probe> = Set.empty()
    let mut right: Set<Probe> = Set.empty()
    for index in 0i64 .. size - 1 do
        left = Set.insert (Probe { value: index * 2, hits: hits }) left
        right = Set.insert (Probe { value: index * 3, hits: hits }) right
    (left, right)

// The `Ord.lt` calls of the intersection, the difference and the union (operation 0, 1, 2) of two sets of `size` keys, not counting the check of the result.
def algebra_calls :: i64 -> i64 -> i64
fn algebra_calls operation size =
    let hits = Atomic.create 0i64
    match probe_sets (ref hits) size with
    | (left, right) ->
        let before = Atomic.load (ref hits)
        if operation == 0 then
            let result = Set.intersect (ref left) (ref right)
            let calls = Atomic.load (ref hits) - before
            assert (Set.invariants_hold (ref result))
            calls
        elif operation == 1 then
            let result = Set.difference (ref left) (ref right)
            let calls = Atomic.load (ref hits) - before
            assert (Set.invariants_hold (ref result))
            calls
        else
            let result = Set.union left right
            let calls = Atomic.load (ref hits) - before
            assert (Set.invariants_hold (ref result))
            calls

// Each of the three operations on two sets of `size` keys makes at most 2 * (size + size) calls (`compare_keys` calls `Ord.lt` up to twice per step).
// 0 when all three stay within that, else the number (1, 2, 3) of the first that does not.
export def set_algebra_bounds :: i64 -> i64
fn set_algebra_bounds size =
    let limit = 2 * (size + size)
    if algebra_calls 0 size > limit then 1
    elif algebra_calls 1 size > limit then 2
    elif algebra_calls 2 size > limit then 3
    else 0

// ---------------------------------------------------------------------------------------------------------------------
// Keys and values that hold a borrow: every function must accept them, as it did before the chunks.

def use_map :: ref Atomic<i64> -> i64
fn use_map hits =
    let first = Probe { value: 1, hits: hits }
    let second = Probe { value: 2, hits: hits }
    let mut map: Map<Probe, Probe> = Map.singleton first second
    map = Map.insert second first map
    map = Map.insert first first map
    let found = Map.get first (ref map)
    let present = Map.contains_key second (ref map)
    let at_value = (Map.at first (ref map)).value
    let keys = Map.keys (ref map)
    let values = Map.values (ref map)
    let pairs = Map.to_array (ref map)
    let total = Map.fold (\sum key value -> sum + key.value + value.value) 0 (ref map)
    let mut seen = 0
    for pair in Map.iter (ref map) do
        match pair with
        | (key, value) -> seen = seen + key.value + value.value
    let sizes = keys.length + values.length + pairs.length
    let found_value = match found with | Maybe.Some probe -> probe.value | Maybe.None -> 0
    let removed = Map.remove second map
    sizes + total + seen + Map.length (ref removed) + at_value + (if present then 1 else 0) + found_value

def use_set :: ref Atomic<i64> -> i64
fn use_set hits =
    let first = Probe { value: 1, hits: hits }
    let second = Probe { value: 2, hits: hits }
    let mut left: Set<Probe> = Set.singleton first
    left = Set.insert second left
    let right: Set<Probe> = Set.singleton second
    let keys = Set.to_array (ref left)
    let common = Set.intersect (ref left) (ref right)
    let different = Set.difference (ref left) (ref right)
    let total = Set.fold (\sum key -> sum + key.value) 0 (ref left)
    let mut seen = 0
    for key in Set.iter (ref left) do seen = seen + key.value
    let sizes = keys.length + Set.length (ref common) + Set.length (ref different)
    let both = Set.union left right
    let present = Set.contains first (ref both)
    sizes + total + seen + Set.length (ref both) + (if present then 1 else 0)

// 33 = use_map 20 + use_set 13, both worked out by hand in the ticket.
export def borrowed_keys_and_values :: i64
fn borrowed_keys_and_values =
    let hits = Atomic.create 0i64
    use_map (ref hits) + use_set (ref hits)

// ---------------------------------------------------------------------------------------------------------------------
// A reference model that shares nothing with the ordered containers: a table indexed by key.

record Table { present: [bool], values: [i64] }

def blank :: i64 -> Table
fn blank range = Table { present: new [bool](range, \_ -> false), values: new [i64](range, \_ -> 0) }

def table_insert :: Table -> i64 -> i64 -> Table
fn table_insert table key value =
    Table { present: Array.update_at key true table.present, values: Array.update_at key value table.values }

def table_remove :: Table -> i64 -> Table
fn table_remove table key = Table { present: Array.update_at key false table.present, values: table.values }

def table_keys :: ref Table -> [i64]
fn table_keys table =
    let mut keys = Vec.empty()
    for key in 0i64 .. table.present.length - 1 do
        if table.present[key] then keys = Vec.push key keys
    Vec.to_array keys

def table_values :: ref Table -> [i64]
fn table_values table =
    let mut values = Vec.empty()
    for key in 0i64 .. table.present.length - 1 do
        if table.present[key] then values = Vec.push table.values[key] values
    Vec.to_array values

def same :: ref [i64] -> ref [i64] -> bool
fn same left right =
    let mut equal = left.length == right.length
    let mut index = 0
    while equal && index < left.length do
        equal = left[index] == right[index]
        index = index + 1
    equal

def map_agrees :: ref Table -> ref Map<i64, i64> -> bool
fn map_agrees table map =
    let keys = Map.keys map
    let values = Map.values map
    let expected_keys = table_keys table
    let expected_values = table_values table
    same (ref keys) (ref expected_keys) && same (ref values) (ref expected_values) && Map.length map == expected_keys.length

// kind 0 inserts, 1 removes, 2 looks up; the key is below `range`.
def run_map :: i64 -> [(i64 * i64)] -> bool
fn run_map range operations =
    let mut map: Map<i64, i64> = Map.empty()
    let mut table = blank range
    let mut ok = true
    let mut step = 0
    while ok && step < operations.length do
        match operations[step] with
        | (kind, key) ->
            if kind == 0 then
                map = Map.insert key step map
                table = table_insert table key step
            elif kind == 1 then
                map = Map.remove key map
                table = table_remove table key
            ok = ok && Map.contains_key key (ref map) == table.present[key]
            ok = ok && Maybe.default_value (-1) (Map.get key (ref map)) == (if table.present[key] then table.values[key] else -1)
        if step % 32 == 0 then ok = ok && Map.invariants_hold (ref map)
        step = step + 1
    ok && Map.invariants_hold (ref map) && map_agrees (ref table) (ref map)

def set_agrees :: ref Table -> ref Set<i64> -> bool
fn set_agrees table set =
    let keys = Set.to_array set
    let expected = table_keys table
    same (ref keys) (ref expected) && Set.length set == expected.length

def build_set :: i64 -> [(i64 * i64)] -> (Set<i64> * Table)
fn build_set range operations =
    let mut set: Set<i64> = Set.empty()
    let mut table = blank range
    for step in 0i64 .. operations.length - 1 do
        match operations[step] with
        | (kind, key) ->
            if kind == 0 then
                set = Set.insert key set
                table = table_insert table key step
            elif kind == 1 then
                set = Set.remove key set
                table = table_remove table key
    (set, table)

def run_set :: i64 -> [(i64 * i64)] -> bool
fn run_set range operations =
    match build_set range operations with
    | (set, table) -> Set.invariants_hold (ref set) && set_agrees (ref table) (ref set)

// The union, the intersection and the difference of the two sets that two lists of operations build.
def run_algebra :: i64 -> ([(i64 * i64)] * [(i64 * i64)]) -> bool
fn run_algebra range pair =
    match pair with
    | (first, second) ->
        match (build_set range first, build_set range second) with
        | ((left, left_table), (right, right_table)) ->
            let mut both = blank range
            let mut common = blank range
            let mut different = blank range
            for key in 0i64 .. range - 1 do
                if left_table.present[key] || right_table.present[key] then both = table_insert both key 0
                if left_table.present[key] && right_table.present[key] then common = table_insert common key 0
                if left_table.present[key] && !right_table.present[key] then different = table_insert different key 0
            let common_set = Set.intersect (ref left) (ref right)
            let different_set = Set.difference (ref left) (ref right)
            let both_set = Set.union left right
            Set.invariants_hold (ref common_set) && set_agrees (ref common) (ref common_set) && Set.invariants_hold (ref different_set) && set_agrees (ref different) (ref different_set) && Set.invariants_hold (ref both_set) && set_agrees (ref both) (ref both_set)

// The long runs. Profile 0 stays below 300 keys, 1 grows past 1,500, 2 mostly removes, and 3 churns about 3,000 keys.
def profile_range :: i64 -> i64
fn profile_range profile = if profile == 0 then 300 elif profile == 1 then 2500 elif profile == 2 then 2500 else 5000

def profile_count :: i64 -> i64
fn profile_count profile = if profile == 0 then 3000 elif profile == 3 then 12000 else 8000

def profile_inserts :: i64 -> i64
fn profile_inserts profile = if profile == 0 then 60 elif profile == 1 then 70 elif profile == 2 then 30 else 55

def draw :: i64u -> i64 -> i64
fn draw state bound = ((state >>> 33) % (bound as i64u)) as i64

// The (kind, key) pairs that a generator seeded with `seed` draws for `profile`.
def profile_operations :: i64 -> i64u -> [(i64 * i64)]
fn profile_operations profile seed =
    let range = profile_range profile
    let inserts = profile_inserts profile
    let mut operations = Vec.with_capacity (profile_count profile)
    let mut state = seed
    for _step in 0i64 .. profile_count profile - 1 do
        state = next_state state
        let choice = draw state 100
        state = next_state state
        let key = draw state range
        let kind = if choice < inserts then 0 elif choice < inserts + (100 - inserts) * 3 / 4 then 1 else 2
        operations = Vec.push (kind, key) operations
    Vec.to_array operations

// 1 when the Map, the Set, or the set operations agree with the model for the run of `profile` that `seed` draws.
export def model_map :: i64 -> i64 -> i64
fn model_map profile seed = if run_map (profile_range profile) (profile_operations profile (seed as i64u)) then 1 else 0

export def model_set :: i64 -> i64 -> i64
fn model_set profile seed = if run_set (profile_range profile) (profile_operations profile (seed as i64u)) then 1 else 0

export def model_algebra :: i64 -> i64 -> i64
fn model_algebra profile seed =
    let first = profile_operations profile (seed as i64u)
    let second = profile_operations profile ((seed as i64u) + 7919i64u)
    if run_algebra (profile_range profile) (first, second) then 1 else 0

// ---------------------------------------------------------------------------------------------------------------------
// Property tests (`tsuzuri test`).

def short_operations :: Gen<[(i64 * i64)]>
fn short_operations = Gen.array_up_to 64 (Gen.pair (Gen.range 0 2) (Gen.range 0 49))

def long_runs :: Gen<(i64 * i64)>
fn long_runs = Gen.pair (Gen.range 0 1000000) (Gen.range 0 3)

test "Map follows the table model on short operation lists" = Gen.for_all_cases 100 (short_operations()) (\operations -> run_map 50 operations)
test "Set follows the table model on short operation lists" = Gen.for_all_cases 100 (short_operations()) (\operations -> run_set 50 operations)
test "Map follows the table model on long seeded runs" = Gen.for_all_cases 12 (long_runs()) (\run -> match run with | (seed, profile) -> model_map profile seed == 1)
test "Set follows the table model on long seeded runs" = Gen.for_all_cases 12 (long_runs()) (\run -> match run with | (seed, profile) -> model_set profile seed == 1)
test "Set algebra follows the table model on long seeded runs" = Gen.for_all_cases 8 (long_runs()) (\run -> match run with | (seed, profile) -> model_algebra profile seed == 1)
```

## B. `tests/features.mjs` の変更（2 箇所）

**B1. suite `map_set` の `inspect`**（検索の関数が確保しないことを IR で調べる正規表現。関数名が変わるので直す。`lower_bound`・`found` は新しい `std` にない）。
変更する行は 1 行だけで、`inspect(ir)` の最初の `const lookups = ...` である。

```js
// 変更前
      const lookups = [...ir.matchAll(/^define internal [^\n]*@tz\.fn\.Map\.(?:lower_bound|found|get|at|contains_key)[^\n]*\{([\s\S]*?)^\}/gm)];
// 変更後
      const lookups = [...ir.matchAll(/^define internal [^\n]*@tz\.fn\.Map\.(?:locate|chunk_of|chunk_total|get|at|contains_key)[^\n]*\{([\s\S]*?)^\}/gm)];
```

**B2. suite `map_set_chunks` の追加**。`map_set` suite の閉じ括弧 `},` の直後（`arena: {` の直前）に、次を丸ごと足す。ケースは 5 + 4 + 4 + 1 + 3 + 12 = 29 件。

```js
  map_set_chunks: {
    cases: [
      // Hand-worked: ascending keys fill a chunk to 512 before they start the next; descending keys split the first chunk when it is full.
      ["ascending", [0n], 0n], ["ascending", [512n], 1n], ["ascending", [513n], 2n], ["ascending", [1024n], 2n], ["ascending", [1025n], 3n],
      ["descending", [512n], 1n], ["descending", [513n], 2n], ["descending", [768n], 2n], ["descending", [769n], 3n],
      ["middle_split", [], 1002n], ["merge_after_removals", [], 22001256n], ["head_deleted", [], 201256n], ["tail_deleted", [], 2001n],
      ["set_algebra_chunks", [], 40202n],
      ["comparison_bounds", [32768n], 0n], ["set_algebra_bounds", [16384n], 0n], ["borrowed_keys_and_values", [], 33n],
      ...[0n, 1n, 2n, 3n].flatMap((profile) => [["model_map", [profile, 11n], 1n], ["model_set", [profile, 11n], 1n], ["model_algebra", [profile, 11n], 1n]]),
    ],
    traps: [],
    inspect(ir) {
      const lookups = [...ir.matchAll(/^define internal [^\n]*@tz\.fn\.Map\.(?:locate|chunk_of|chunk_total|beyond_last|get|contains_key)[^\n]*\{([\s\S]*?)^\}/gm)];
      assert.ok(lookups.length > 0);
      for (const [, body] of lookups) assert.doesNotMatch(body, /@tz\.(?:alloc|realloc)\(/);
    },
  },
```

## C. `tests/map_set_chunks.rs`（新規）

`tsuzuri test tests/fixtures/map_set_chunks` を `-O0` と `-O3` で走らせ、5 つの性質テストがすべて通ることを確かめる（スタイルは `tests/property.rs`）。出力の終わりの行は `5 passed; 0 failed; 0 ignored`。

```rust
//! The property tests of the chunked `Map` and `Set` (C12): `tsuzuri test` runs the `test` declarations of
//! tests/fixtures/map_set_chunks at -O0 and -O3. The E2E cases of the same fixture are the suite `map_set_chunks` of tests/features.mjs.

use std::process::{Command, Output};

fn run(level: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args(["test", "tests/fixtures/map_set_chunks", level])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap()
}

#[test]
fn property_tests_pass_at_o0_and_o3() {
    for level in ["-O0", "-O3"] {
        let output = run(level);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "{level}\n{stdout}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(stdout.contains("5 passed; 0 failed; 0 ignored"), "{level}\n{stdout}");
    }
}
```

## D. `tests/map_set.rs` への追加（新しいフィールド名の `E1022`）

関数 `ordered_containers_are_opaque_noncopy_owned_values` の、`for source in [ ... ] {` の配列の**末尾**（既存の 5 件の後）に、次の 6 件を足す。既存の 5 件（`entries` を使うもの）は変えない
（`record_storage` の検査が、フィールドの探索より前に走るので、フィールド名が変わっても `E1022` のままである。新しい `std` を持つコンパイラで 6 件とも `E1022` を実測した）。

```rust
        "let map: Map<i64, i64> = Map { head: Vec.empty(), rest: Vec.empty(), length: 0 }\n0",
        "let map = Map.singleton 1 2\nmap.head.length",
        "let map = Map.singleton 1 2\nlet other = { map with length = 5 }\nMap.length (&other)",
        "let map = Map.singleton 1 2\nmatch map with | Map { rest = storage } -> Vec.length (&storage)",
        "let set = Set.singleton 1\nmatch set with | { head = storage } -> Vec.length (&storage)",
        "let set = Set.singleton 1\nset.rest.length",
```

## E. `scripts/lldb/tsuzuri_lldb.py` の置き換え（実 LLDB とモック試験で検証済み）

`def is_collection`・`def collection_summary`（`@guarded` 付き）・`class CollectionProvider` の 3 つを、次の text で**丸ごと**置き換える（ファイル内の位置は変えない。直前は `SequenceProvider`／`ListProvider` の定義、直後は `def union_node_type`）。
使うのは既存の `field_names`・`target`・`length_of`・`member`・`strip`・`child_at`・`child_index`・`LENGTH_LIMIT`・`guarded` だけで、新しい import はない。
`CollectionProvider` はもう `SequenceProvider` を継がない。チャンクの先頭の番地と終わりの通し番号を、LLDB が子を要求した分だけ `rest` から読む（1,000,000 エントリでも `frame variable` の表示は最初の 256 個だけ読む）。

<!-- file: scripts/lldb/tsuzuri_lldb.py (3 definitions) -->
```text
def is_collection(sbtype, _dict):
    """The std `Map<K, V>` or `Set<K>`: a record of the chunks `head` and `rest` that hold its sorted entries, and their number `length`."""
    name = sbtype.GetName() or ""
    return name.startswith(("Map<", "Set<")) and field_names(sbtype) == ["head", "rest", "length"]


@guarded
def collection_summary(valobj):
    return f"size={length_of(target(valobj))}"


class CollectionProvider:
    """The entries of a `Map` or the keys of a `Set`, in order: those of `head`, then those of each chunk of `rest`.
    The chunk headers of `rest` are read only as far as the children that LLDB asks for."""

    def __init__(self, valobj, _dict):
        self.valobj = valobj
        self.update()

    def update(self):
        self.count = 0
        self.chunks = 0
        self.read = 0
        self.firsts = []
        self.ends = []
        try:
            value = self.valobj.GetNonSyntheticValue()
            head = member(value, "head")
            data = member(head, "data")
            self.element = strip(data.GetType()).GetPointeeType()
            self.stride = self.element.GetByteSize()
            rest = member(value, "rest")
            rest_data = member(rest, "data")
            self.chunk_type = strip(rest_data.GetType()).GetPointeeType()
            self.chunk_stride = self.chunk_type.GetByteSize()
            self.chunk_address = rest_data.GetValueAsUnsigned()
            self.chunks = length_of(rest)
            self.firsts.append(data.GetValueAsUnsigned())
            self.ends.append(length_of(head))
            if self.element.IsValid() and self.stride > 0 and self.chunk_stride > 0:
                self.count = length_of(value)
        except Exception:
            self.count = 0
        return False

    def address(self, index):
        """The address of element `index`, or None when the chunks do not reach it."""
        while self.ends[-1] <= index:
            if self.read >= self.chunks:
                return None
            chunk = child_at(
                self.valobj,
                "chunk",
                self.chunk_address + self.read * self.chunk_stride,
                self.chunk_type,
            )
            if chunk is None:
                return None
            chunk = chunk.GetNonSyntheticValue()
            self.read += 1
            self.firsts.append(member(chunk, "data").GetValueAsUnsigned())
            self.ends.append(self.ends[-1] + length_of(chunk))
        low, high = 0, len(self.ends) - 1
        while low < high:
            middle = (low + high) // 2
            if self.ends[middle] <= index:
                low = middle + 1
            else:
                high = middle
        number = low
        before = self.ends[number - 1] if number else 0
        return self.firsts[number] + (index - before) * self.stride

    def num_children(self, max_children=LENGTH_LIMIT):
        return min(self.count, max_children)

    def has_children(self):
        return self.count > 0

    def get_child_index(self, name):
        return child_index(name)

    def get_child_at_index(self, index):
        if not 0 <= index < self.count:
            return None
        try:
            address = self.address(index)
        except Exception:
            return None
        if address is None:
            return None
        return child_at(self.valobj, f"[{index}]", address, self.element)
```

## F. `src/runtime/tsuzuri.natvis` の置き換え（XML・PDB 確認済み。Visual Studio 実 UI は未確認）

`<Type Name="Map&lt;*,*&gt;">` と `<Type Name="Set&lt;*&gt;">` の 2 つの `<Type>`（それぞれ `DisplayString` と `ArrayItems` を持つ）を、次の 2 つで置き換える。直前の `<Type Name="Vec&lt;*&gt;">` と直後の `<!-- A Maybe of a string ...` のコメントは残す。
`Vec` の natvis が `data`・`length` を使っているのと同じ名前を使う。`rest.data[chunk]` は `Vec<Entry>` で、その `length`・`data` を読む。

```xml
  <!-- C12: a Map or a Set keeps its entries in chunks: `head` is the first, `rest` holds the others. The items are those of `head`,
       then those of each chunk of `rest`, in order. -->
  <Type Name="Map&lt;*,*&gt;">
    <DisplayString>{{ size={length} }}</DisplayString>
    <Expand>
      <CustomListItems MaxItemsPerView="10000">
        <Variable Name="chunk" InitialValue="-1" />
        <Variable Name="offset" InitialValue="0" />
        <Variable Name="size" InitialValue="head.length" />
        <Size>length</Size>
        <Loop>
          <If Condition="offset &gt;= size">
            <Exec>chunk = chunk + 1</Exec>
            <Exec>offset = 0</Exec>
            <Break Condition="chunk &gt;= rest.length" />
            <Exec>size = rest.data[chunk].length</Exec>
          </If>
          <Item Condition="chunk &lt; 0">head.data[offset]</Item>
          <Item Condition="chunk &gt;= 0">rest.data[chunk].data[offset]</Item>
          <Exec>offset = offset + 1</Exec>
        </Loop>
      </CustomListItems>
    </Expand>
  </Type>
  <Type Name="Set&lt;*&gt;">
    <DisplayString>{{ size={length} }}</DisplayString>
    <Expand>
      <CustomListItems MaxItemsPerView="10000">
        <Variable Name="chunk" InitialValue="-1" />
        <Variable Name="offset" InitialValue="0" />
        <Variable Name="size" InitialValue="head.length" />
        <Size>length</Size>
        <Loop>
          <If Condition="offset &gt;= size">
            <Exec>chunk = chunk + 1</Exec>
            <Exec>offset = 0</Exec>
            <Break Condition="chunk &gt;= rest.length" />
            <Exec>size = rest.data[chunk].length</Exec>
          </If>
          <Item Condition="chunk &lt; 0">head.data[offset]</Item>
          <Item Condition="chunk &gt;= 0">rest.data[chunk].data[offset]</Item>
          <Exec>offset = offset + 1</Exec>
        </Loop>
      </CustomListItems>
    </Expand>
  </Type>
```

確認（Windows がなくてもできる）: `python3 -c "import xml.dom.minidom as m; m.parse('src/runtime/tsuzuri.natvis')"` が例外なしで終わること、
`cargo test --locked pdb_links_write_a_named_pdb_and_embed_the_natvis_views`（`src/driver.rs`。natvis の文字列をそのまま埋め込むことを調べる）が通ること。

## G. `tests/debugger.mjs` の追加（実 LLDB で検証済み）

既存の期待（`(Map<i32, i32>) table = size=1 {`・`[0] = (key = 1, value = 10)`・`(Set<i32>) keys = size=3 {...}`）は、1 チャンク（`head` だけ）の場合なので**変えない**。
複数のチャンクを調べるため、`extras` の節（`const edge = ...` と、その `assert` 群、`lookalike` の `assert.match` まで）の**後ろ**で、`// PM01:` のコメントの**前**に、次を足す。
`session`・`sections`・`root`・`mkdirSync`・`writeFileSync`・`join`・`assert` は、このファイルで既に定義・import 済みである。

```js
  // C12: the keys of a Set fill two chunks (512 and 1). The formatter reads the header of the second chunk only when a child in it is asked for.
  const chunked = join(root, "chunked");
  mkdirSync(chunked);
  writeFileSync(join(chunked, "Main.tz"), [
    "def build :: i64 -> Set<i64>",
    "fn build count =",
    "    let mut set: Set<i64> = Set.empty()",
    "    for key in 0i64 .. count - 1 do set = Set.insert key set",
    "    set",
    "",
    "def inspect :: i64 -> i64",
    "fn inspect count =",
    "    let many = build count",
    "    Set.length (ref many)",
    "",
    "inspect 513",
    "",
  ].join("\n"));
  const crossing = sections(session(chunked, ["breakpoint set -f Main.tz -l 10", "run", "frame variable many", "frame variable many[0] many[511] many[512]", "kill"]));
  const summary = crossing.find(part => part.command === "frame variable many").text;
  assert.match(summary, /^\(Set<i64>\) many = size=513 \{$/m, summary);
  assert.match(summary, /^ {2}\[0\] = 0$/m, summary);
  const picked = crossing.find(part => part.command === "frame variable many[0] many[511] many[512]").text;
  for (const index of [0, 511, 512]) assert.match(picked, new RegExp(`^\\(i64\\) many\\[${index}\\] = ${index}$`, "m"), picked);
```

## H. モック試験（LLDB なしで `CollectionProvider` を調べる。リポジトリには**追加しない**）

LLDB が `run` で止まる環境の代わりの確認。`target/c12-mock/mock_test.py`（`target/` は git 管理外。リポジトリには追加しない）として作り、`python3 target/c12-mock/mock_test.py "$PWD/scripts/lldb"` を実行する。
擬似メモリ上に `Map<i32, i32>` を作り、チャンク構成 `[0]`・`[1]`・`[3]`・`[2,3,1]`・`[4,1]`・`[1,1,1,1,1]`・`[512,512,7]` で、子の数・要約・各子の番地と名前（昇順・降順の両方の順に要求）・範囲外の `None` を確かめる。
合格なら `collection provider: ok` を出す。

<!-- file: mock_test.py -->
```text
"""Checks the `CollectionProvider` of tsuzuri_lldb.py against a simulated memory image, without LLDB."""
import struct
import sys
import types

fake = types.ModuleType("lldb")
fake.SBError = type("SBError", (), {"Fail": lambda self: False})
fake.eTypeClassStruct = 1
sys.modules["lldb"] = fake
sys.path.insert(0, sys.argv[1])
import tsuzuri_lldb as formatter  # noqa: E402

memory = bytearray(1 << 16)
cursor = [64]


def allocate(size):
    address = cursor[0]
    cursor[0] += (size + 15) // 16 * 16
    return address


class Field:
    def __init__(self, name, ty, offset):
        self.name, self.ty, self.offset = name, ty, offset

    def GetName(self):
        return self.name

    def GetType(self):
        return self.ty

    def GetOffsetInBytes(self):
        return self.offset


class Type:
    def __init__(self, name, size, fields=(), pointee=None, pointer=False):
        self.name, self.size, self.fields, self.pointee, self.pointer = name, size, list(fields), pointee, pointer

    def IsTypedefType(self):
        return False

    def GetCanonicalType(self):
        return self

    def GetName(self):
        return self.name

    def GetByteSize(self):
        return self.size

    def IsValid(self):
        return True

    def GetPointeeType(self):
        return self.pointee

    def IsPointerType(self):
        return self.pointer

    def GetNumberOfFields(self):
        return len(self.fields)

    def GetFieldAtIndex(self, index):
        return self.fields[index]


class Error:
    def Fail(self):
        return False


class Value:
    def __init__(self, ty, address, name="value"):
        self.ty, self.address, self.name = ty, address, name

    def GetNonSyntheticValue(self):
        return self

    def GetType(self):
        return self.ty

    def IsValid(self):
        return self.ty.name != "invalid"

    def GetError(self):
        return Error()

    def GetChildMemberWithName(self, name):
        for field in self.ty.fields:
            if field.name == name:
                return Value(field.ty, self.address + field.offset, name)
        return Value(Type("invalid", 0), 0, name)

    def GetValueAsUnsigned(self, default=0):
        return struct.unpack_from("<Q", memory, self.address)[0]

    def GetValueAsSigned(self, default=0):
        return struct.unpack_from("<q", memory, self.address)[0]

    def CreateValueFromAddress(self, name, address, ty):
        value = SyntheticValue if ty.name.startswith("Vec<") else Value
        return value(ty, address, name)


class SyntheticValue(Value):
    def GetNonSyntheticValue(self):
        return Value(self.ty, self.address, self.name)

    def GetChildMemberWithName(self, name):
        return Value(Type("invalid", 0), 0, name)


i32 = Type("i32", 4)
i64 = Type("i64", 8)
entry = Type("Map.Entry<i32, i32>", 8, [Field("key", i32, 0), Field("value", i32, 4)])
entry_pointer = Type("Map.Entry<i32, i32> *", 8, pointee=entry, pointer=True)
chunk = Type("Vec<Map.Entry<i32, i32>>", 24, [Field("data", entry_pointer, 0), Field("length", i64, 8), Field("capacity", i64, 16)])
chunk_pointer = Type("Vec<Map.Entry<i32, i32>> *", 8, pointee=chunk, pointer=True)
chunks = Type("Vec<Vec<Map.Entry<i32, i32>>>", 24, [Field("data", chunk_pointer, 0), Field("length", i64, 8), Field("capacity", i64, 16)])
map_type = Type("Map<i32, i32>", 56, [Field("head", chunk, 0), Field("rest", chunks, 24), Field("length", i64, 48)])


def vec(address, data, length):
    struct.pack_into("<QQQ", memory, address, data, length, length)


def build(sizes):
    """A map whose chunks hold `sizes` entries; its keys are 0, 1, ... and the address of key k is returned by `where`."""
    addresses = []
    first = allocate(8 * max(sizes[0], 1))
    addresses.append(first)
    rest = allocate(24 * max(len(sizes) - 1, 1))
    for number, size in enumerate(sizes[1:]):
        data = allocate(8 * size)
        addresses.append(data)
        vec(rest + 24 * number, data, size)
    root = allocate(56)
    vec(root, first, sizes[0])
    vec(root + 24, rest, len(sizes) - 1)
    struct.pack_into("<q", memory, root + 48, sum(sizes))
    where = []
    for number, size in enumerate(sizes):
        where.extend(addresses[number] + 8 * offset for offset in range(size))
    return Value(map_type, root), where


assert formatter.is_collection(map_type, {})
assert not formatter.is_collection(Type("Map<i32, i32>", 24, [Field("entries", chunk, 0)]), {})

for sizes in ([0], [1], [3], [2, 3, 1], [4, 1], [1, 1, 1, 1, 1], [512, 512, 7]):
    value, where = build(sizes)
    provider = formatter.CollectionProvider(value, {})
    total = sum(sizes)
    assert provider.num_children() == total, (sizes, provider.num_children())
    assert provider.has_children() == (total > 0)
    assert formatter.collection_summary(value, {}) == f"size={total}", formatter.collection_summary(value, {})
    # Ask for the children out of order too: the chunk headers are read on demand.
    for order in (range(total), reversed(range(total))):
        provider.update()
        for index in order:
            child = provider.get_child_at_index(index)
            assert child is not None and child.address == where[index] and child.name == f"[{index}]", (sizes, index)
    assert provider.get_child_at_index(total) is None
    assert provider.get_child_at_index(-1) is None
    assert provider.get_child_index("[3]") == 3
print("collection provider: ok")
```

## I. 文書の新しい文面（Phase 1 の PR に含める）

`c` はチャンクの個数（`Map`／`Set` のページの `n`・`m` は集合演算の 2 つの大きさなので、`c` を使う）。置き換えは、**行頭の文言**で行を探して、その行（表なら行）を丸ごと入れ替える。
文面は、現在のコンパイラの実装（付録 A・B）に合わせてある。置き換えたら `node scripts/check-docs.mjs <ページ>` を通す（本体「ドキュメント」の表）。

### I1. `README.md`

行頭が `- **順序付きマップ・セット (` の 1 行を、次に置き換える（`$O(n)$` と「平衡二分探索木」の記述を直す）。

```text
- **順序付きマップ・セット (`Map<K, V>` / `Set<K>`)**: キーの昇順に並べたエントリを、1〜512 エントリのチャンクの列で持つ、所有権を消費して更新するコレクションです（木ではありません）。検索は $O(\log n)$ 回の比較、挿入および削除は $O(\log n)$ 回の比較と 1 チャンク内の移動（最大 511 エントリ）で動作し、チャンクの分割と併合はチャンクの個数に比例する移動を伴います。詳細は [Map](_tsuzuri/language-reference/built-in-types-and-modules/map.md) / [Set](_tsuzuri/language-reference/built-in-types-and-modules/set.md) を参照してください。
```

### I2. `docs/language.md`（節「Map / Set」）

(a) `Map` の API の列挙（行頭が `` `contains_key (&map) key`、`get (&map) key` ``）の行末の `` `fold folder initial (&map)`。 `` を、次の行末に置き換える。

```text
`fold folder initial (&map)`、診断用の `chunk_count (&map)`（エントリを持つチャンクの個数。空なら 0）と `invariants_hold (&map)`（チャンクの規則を満たすか）。
```

(b) `Set` の API の列挙（行頭が `` `to_array (&set)`、`fold folder initial (&set)` ``）の行末 `` `difference (&left) (&right)`。 `` を、次に置き換える。

```text
`difference (&left) (&right)`、診断用の `chunk_count (&set)` と `invariants_hold (&set)`。
```

(c) 行頭が `各操作の計算量は、検索が $O(\log n)$、挿入および削除が $O(n)$` の行と、その次の行（`` `union` は片方の出力領域を再利用して… ``）の 2 行を、次の 3 行に置き換える。

```text
内部は、キーの昇順に並べた 1〜512 エントリのチャンクの列です（先頭のチャンクが `head`、残りが `rest`）。隣り合う 2 つのチャンクのエントリ数の合計は常に 256 を超え、チャンクの個数 $c$ は $2n/257 + 1$ 以下です。
各操作の計算量は、検索（`contains_key`・`get`・`at`・`contains`）が $O(\log n)$ 回の比較で確保なし、挿入および削除が $O(\log n)$ 回の比較と 1 チャンク内の移動（最大 511 エントリ）、チャンクの分割・併合を伴う更新がさらに $O(c)$ のチャンクの移動、スナップショット生成および `fold` が $O(n)$、集合演算が $O(n + m)$ です。キーが最大のキーより大きい挿入（昇順の挿入）は最後のチャンクへの追加で、チャンクが 512 エントリで満ちたら次のチャンクを始めます。
`union` は両辺のチャンクから要素を移動して 384 エントリずつの新しいチャンクを作り、`intersect` や `difference` は借用入力から要素をコピーして同じ形のチャンクを作ります。すべての走査および `fold` 処理は、キーの昇順に従って決定論的に実行されます。
```

### I3. `docs/architecture.md`（段落「順序付きコンテナ」。4 行を置き換える）

(a) 行頭が `型名は通常のジェネリックレコードであり、内部的には` の行:

```text
型名は通常のジェネリックレコードであり、内部的には `Map` が `head: Vec<Entry<K, V>>`・`rest: Vec<Vec<Entry<K, V>>>`・`length: i64` を、`Set` が `head: Vec<K>`・`rest: Vec<Vec<K>>`・`length: i64` を保持します（`head` が先頭のチャンク、`rest` が残りのチャンクで、各チャンクは 1〜512 エントリ、隣り合う 2 つの合計は 256 を超えます）。`Vec` が持つ非 Copy 性、メモリ確保、および clone／drop の仕組みをそのまま共有し、ランタイムに新しい特殊な内部表現を追加することはありません。
```

(b) 行頭が `二分探索（` で `lower_bound` を含む行:

```text
チャンクの最後のキーに対する二分探索と、チャンク内の二分探索（`locate`）によって借用比較を行います。更新は、対象のチャンクを `Vec.swap` と `Vec.pop`／`Vec.push` で取り出して（`take_chunk`）そのチャンクの中でエントリを移動し、戻します（`put_chunk`）。満ちた（512 エントリの）チャンクは半分に分け、隣り合う 2 つの合計が 256 以下になったら併合します。重複キーの挿入時には、既存のキーインスタンスを維持したまま値（value）のみを置換します。
```

(c) 行頭が `` `Set.union` は末尾からの要素比較と pop を利用して `` の行:

```text
`Set.union` は両辺のチャンクの末尾から要素を比較して pop し、384 エントリずつの逆順のチャンクを作った後に反転します。`intersect` および `difference` は、（チャンク番号, オフセット）の 2 本のカーソルを進めながら要素を選び、同じ形のチャンクを作ります。
```

(d) 行頭が `検証は `cargo test --locked --test map_set` および` の行:

```text
検証は `cargo test --locked --test map_set` と `cargo test --locked --test map_set_chunks`、および `cargo build --release --locked && node tests/features.mjs target/release/tsuzuri map_set` と、同じコマンドの `map_set_chunks` で行われています。
```

### I4. `_tsuzuri/language-reference/built-in-types-and-modules/map.md`

(a) 行頭が `` `Map<K, V>` は、キーの全順序（`Ord`）で昇順に並べた連想配列です。 `` の行（3 行目）:

```text
`Map<K, V>` は、キーの全順序（`Ord`）で昇順に並べた連想配列です。中身は 1〜512 エントリのチャンクに分けた整列配列で、二分探索木ではありません。検索は $O(\log n)$ 回の比較、挿入と削除は $O(\log n)$ 回の比較と 1 チャンク内の要素の移動（最大 511 エントリ）です。
```

(b) 行頭が `- 要素の検索は二分探索により` の行:

```text
- 要素の検索は二分探索により **$O(\log n)$**、整列状態を維持した挿入・削除は **$O(\log n)$ 回の比較と 1 チャンク内の移動**です（チャンクの分割・併合では、チャンクの個数に比例する移動も加わります）。
```

(c) 行頭が `` `Map.remove key map` は、キーが存在すれば `` の行: 文中の `後続要素をシフトして` だけを `同じチャンクの後続要素をシフトして` に直す。

(d) API 表の `insert` と `remove` の 2 行: 行末の `（$O(n)$）` を `（$O(\log n)$ 回の比較と 1 チャンクの移動）` に直す。

(e) 行頭が `` - `Map` はキー昇順の連続バッファです。 `` の行（「まとめ」の節）:

```text
- `Map` はキー昇順に並べた 1〜512 エントリのチャンクの列です。二分探索木ではありませんが、検索は $O(\log n)$ 回の比較、挿入と削除は $O(\log n)$ 回の比較と 1 チャンク内の移動です。
```

(f) 「## 計算量」の表の、検索から反復・配列化までの 5 行（見出し行と区切り行は残す）:

```text
| 検索 `contains_key` / `get` / `at` | $O(\log n)$ | チャンクの最後のキーへの二分探索と、チャンク内の二分探索。確保なし |
| 挿入 `Map.insert` | $O(\log n)$ 回の比較 + 1 チャンク内の移動 | 移動は最大 511 エントリ。満ちたチャンクは分割（さらに $O(c)$）。最大キーより大きい挿入は最後のチャンクへの追加 |
| 削除 `Map.remove` | $O(\log n)$ 回の比較 + 1 チャンク内の移動 | 隣り合う 2 つのチャンクの合計が 256 以下になれば併合（さらに $O(c)$）。チャンクの個数 $c \le 2n/257 + 1$ |
| 要素数の参照 `length` / `is_empty` | $O(1)$ | 長さのフィールドの参照 |
| 反復・配列化 `fold` / `keys` / `to_array` | $O(n)$ | 全要素を昇順に 1 回走査 |
```

(g) 「### 変換・反復」の表の直後、「## 計算量」の前に、次の小節を足す:

```text
### 診断用

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `chunk_count` | `ref Map<'key, 'value> -> i64` | エントリを持つチャンクの個数を返します（空のマップは 0）。個数は実装の詳細で、将来の版で変わりえます。テストで使います |
| `invariants_hold` | `Ord<'key> => ref Map<'key, 'value> -> bool` | チャンクの規則（各チャンク 1〜512 エントリ、隣り合う 2 つの合計が 256 を超える、キーが厳密に昇順、`length` が合計と一致する）を満たすかを $O(n)$ で調べます。テストで使います |
```

### I5. `_tsuzuri/language-reference/built-in-types-and-modules/set.md`

(a) 行頭が `` `Set<K>` は、重複のない要素を `` の行（3 行目）:

```text
`Set<K>` は、重複のない要素を `Ord` で昇順に並べた集合です。中身は 1〜512 要素のチャンクに分けた整列配列で、二分探索木ではありません。
```

(b) 行頭が `- 要素の検索・包含判定は二分探索により` の行:

```text
- 要素の検索・包含判定は二分探索により **$O(\log n)$**、整列状態を維持した挿入・削除は **$O(\log n)$ 回の比較と 1 チャンク内の移動**です（チャンクの分割・併合では、チャンクの個数に比例する移動も加わります）。
```

(c) 行頭が `` - `Set.remove key set`: `` の行、API 表の `insert` と `remove` の 2 行: `（$O(n)$）` を `（$O(\log n)$ 回の比較と 1 チャンクの移動）` に直す（3 行）。

(d) 行頭が `` - `Set` はキー昇順の連続バッファです。 `` の行（「まとめ」の節）:

```text
- `Set` はキー昇順に並べた 1〜512 要素のチャンクの列です。検索は $O(\log n)$ 回の比較、挿入と削除は $O(\log n)$ 回の比較と 1 チャンク内の移動です。
```

(e) 「## 計算量」の表の、`挿入`・`削除`・`和集合`・`積集合 / 差集合` の 4 行（`包含判定`・`長さの参照`・`反復・配列化` の行は変えない）:

```text
| 挿入 `Set.insert` | $O(\log n)$ 回の比較 + 1 チャンク内の移動 | 移動は最大 511 要素。満ちたチャンクは分割（さらに $O(c)$）。最大キーより大きい挿入は最後のチャンクへの追加 |
| 削除 `Set.remove` | $O(\log n)$ 回の比較 + 1 チャンク内の移動 | 隣り合う 2 つのチャンクの合計が 256 以下になれば併合（さらに $O(c)$） |
| 和集合 `Set.union` | $O(n + m)$ | 両辺のチャンクの末尾から比較して要素を移動し、384 要素ずつの新しいチャンクを作る |
| 積集合 / 差集合 `intersect` / `difference` | $O(n + m)$ | 2 本のカーソルを進めて要素をコピーし、384 要素ずつの新しいチャンクを作る |
```

(f) 「### 集合演算」の節の前に、`Map` の (g) と同じ形の小節 `### 診断用` を足す（`chunk_count :: ref Set<'key> -> i64`、`invariants_hold :: Ord<'key> => ref Set<'key> -> bool`。説明の「エントリ」を「要素」に直す）。

### I6. 周辺の文書・コメントの修正

- `_tsuzuri/language-reference/built-in-types-and-modules/hashmap.md`: 行頭が `> `with_seed` の SipHash-1-3 は` の行の末尾 `` `Map` は二分探索木ではなく、キー昇順の連続バッファです。 `` を `` `Map` は二分探索木ではなく、キー昇順に並べたチャンクの列です。 `` に直す。
- `_tsuzuri/language-reference/built-in-types-and-modules/array.md`:
  - 行頭が `並び・集合・連想配列のどれが要るかで選びます。` の行の `違いは、整列した連続バッファかハッシュ表かです。` を `違いは、キー昇順に並べたチャンクの列かハッシュ表かです。` に直す。
  - 行頭が `    assoc -->|必要 / Ord| map[` の行の `キー昇順の連続バッファ` を `キー昇順のチャンクの列` に直す。
  - 表の `**Map**` の行: `中身はキー昇順の連続バッファ` を `中身はキー昇順のチャンクの列` に、更新の列の `$O(n)$` を `$O(\log n)$ 回の比較 + 1 チャンクの移動` に直す。`**Set**` の行は、更新の列の `$O(n)$` だけを同じ文言に直す。
- `_tsuzuri/language-reference/built-in-types-and-modules/json.md`: 行頭が `- `Map` と `Set` の decode は、要素をすべて decode してから` の行の `整列済みの配列へ文書順に 1 つずつ挿入すると、降順の入力では毎回全体をずらす $O(n^2)$ になります。` を、次に直す。

  ```text
  整列済みのマップへ文書順に 1 つずつ挿入すると、降順の入力では毎回先頭のチャンク（最大 512 エントリ）の先頭へ入れるので、1 件ごとに最大 511 エントリの移動と、チャンクが満ちるたびの分割になります。
  ```

- `std/Json.tz` のコメント（`by_key` の直前の `///` の 5 行）: `every `Map.insert` or `Set.insert` appends; inserting` に続く `in document order would shift the entries after each smaller key (quadratic).` を、`in document order would move up to 511 entries of the first chunk for each smaller key.` に直す（`appends` は `appends to the last chunk` にする。1 行は 120 文字以内に折り返す）。
- `tests/fixtures/json/Main.tz` のコメント（`` `count` entries whose keys descend: `` で始まる `///` の 3 行）: `shift every entry (quadratic time).` を `move up to 511 entries of the first chunk each time.` に直す。
- `docs/benchmarks.md`: 表の行 `| map_count | 200,000／1,024 | 整列 Map／Set、二分探索、隣接交換による挿入 |` の 3 列目を `整列 Map（チャンク化した整列配列）、二分探索、チャンク内の移動による挿入` に直す（行番号は書かない。`grep -n "整列 Map／Set" docs/benchmarks.md` で 1 件）。

### I7. 変えない文書（確認済み）

- `_tsuzuri/language-reference/compiler/debugging.md`: `Map`・`Set` は `size=N` とキー順の要素 `[0]`、`[1]`… と書いてあり、LLDB の表示は変わらない。
- `docs/language.md` の `HashMap` の節: `entries` と swap-remove の記述は `HashMap` のもの。
- `_tsuzuri/language-reference/built-in-types-and-modules/seq.md`・`loops-and-conditionals/for-in.md`・`values-and-functions/keywords.md`: `Map`・`Set` の使用例だけで、計算量や内部の記述がない。
