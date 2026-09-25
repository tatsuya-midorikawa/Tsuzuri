# C04: 配列の一括操作 API
| 項目 | 内容 |
|---|---|
| ID | C04 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | A11, E02, C03, B01 |
| 後続 | D05, F02, F05 |
| 状態 | todo |
| 主な影響ファイル | `std/Array.tz`、`std/List.tz`、`src/check.rs` `Builtin`、`src/llvm.rs` `emit_builtin` / `FunctionEmitter::{array_loop, checked_element_pointer, clone_value}`、`src/call_specialization.rs`、`benchmarks/array`、`tests/array_bulk.rs`、`tests/fixtures/array_bulk/Main.tz`、`docs/language.md`、`docs/architecture.md`、`docs/benchmarks.md`、`README.md` |

## 目的

配列・連結リストの実用的な標準 API を提供する。読み取りだけの API は C03 の `&[T]` を受け取り、配列全体の copy を避ける。順序・数値意味・所有権を API 契約として明記し、将来の SIMD / 並列 / CPU dispatch でも結果を変えない。

D-14 に従い、浮動小数点の既定集計は左から右の逐次順序とする。pairwise、Kahan、parallel reduction は D05/F02 の別 API。

## 現状

- 配列・リストの primitive 操作は `.length`、`[index]`、`for...in`、literal / `new` のみ。
- `src/llvm_control.rs` `for_each` は配列を data + length の単純 loop に下げる。scratch で `clang -O3 -S -emit-llvm` を確認すると、整数配列の初期化・加算 loop は `vector.body` と `<2 x i64>` / `llvm.vector.reduce.add.v2i64` に最適化される。ただし浮動小数点 sum は順序保持のため vector reduction を期待しない。
- `src/call_specialization.rs` は既知の non-escaping callback を特殊化できるため、`Array.map xs (fx x -> ...)` のような API は std 実装でも callback 間接呼び出しを省ける可能性がある。
- `Builtin` は現状 scalar だけ。E02/C03 後に `Array.xxx` namespace と `&[T]` が使える。

## 仕様

### 前提とする他チケットのインターフェース

- E02: `std/Array.tz` / `std/List.tz` の同梱と `Array.xxx` 修飾解決。
- A11: `Eq`/`Ord` は borrowed signature で、`==`/`<` などの比較演算子は要素を消費しない。C04 の検索・等価・整列 API は比較のためだけに `Copy` を要求しない。
- C03: `&['a]` が fat slice `{ ptr, i64 }` として copy なしに渡せる。
- B01: `Option<'a>`。`find` / `try_*` 系で使う。
- A06/A07 は `Ord` / `Eq` の deriving を後で補強するが、C04 phase 1 は既存組み込み `Eq` / `Ord` とユーザー instance を使う。

### 他チケットへの提供インターフェース

- D05 は `Array.sum_pairwise` / `Array.sum_kahan` / FMA 明示 API を追加するとき、本チケットの `Array.sum` 順序契約を変更しない。
- F02 は `Parallel.map` / `Parallel.reduce` を追加するとき、本チケットの逐次 API と名前を分ける。
- F05 は `Array.sort` などの CPU dispatch を内部最適化として追加できるが、安定性・順序・NaN 契約を変えない。

### API 一覧

読み取り入力は原則 `&['a]`。返り値が新しい配列なら owned `['b]`。

```text
// Phase 1: scalar/borrowed read helpers
Array.length        : &['a] -> i64
Array.is_empty      : &['a] -> bool
Array.get           : Copy<'a> => &['a] -> i64 -> Option<'a>
Array.at            : &['a] -> i64 -> &'a
Array.sub           : Copy<'a> => &['a] -> i64 -> i64 -> ['a]

// Phase 1: construction / conversion
Array.init          : i64 -> (i64 -> 'a) -> ['a]
Array.reverse       : Copy<'a> => &['a] -> ['a]
Array.append        : Copy<'a> => &['a] -> &['a] -> ['a]
Array.concat        : Copy<'a> => &[['a]] -> ['a]
Array.zip           : (Copy<'a>, Copy<'b>) => &['a] -> &['b] -> [('a * 'b)]
Array.to_list       : Copy<'a> => &['a] -> [|'a|]

// Phase 1: callbacks that receive values require Copy input.
Array.map           : Copy<'a> => &['a] -> ('a -> 'b) -> ['b]
Array.mapi          : Copy<'a> => &['a] -> (i64 -> 'a -> 'b) -> ['b]
Array.fold          : Copy<'a> => &['a] -> 's -> ('s -> 'a -> 's) -> 's
Array.fold_back     : Copy<'a> => &['a] -> 's -> ('a -> 's -> 's) -> 's
Array.reduce        : Copy<'a> => &['a] -> ('a -> 'a -> 'a) -> Option<'a>

// Phase 1: _ref variants support non-Copy input by passing element borrows.
Array.map_ref       : &['a] -> (&'a -> 'b) -> ['b]
Array.mapi_ref      : &['a] -> (i64 -> &'a -> 'b) -> ['b]
Array.fold_ref      : &['a] -> 's -> ('s -> &'a -> 's) -> 's
Array.fold_back_ref : &['a] -> 's -> (&'a -> 's -> 's) -> 's

// Phase 1: numeric reductions. Empty arrays return numeric identities.
Array.sum           : (Numeric<'a>, Add<'a>) => &['a] -> 'a
Array.product       : (Numeric<'a>, Mul<'a>) => &['a] -> 'a

// Phase 1: comparison-only APIs use A11 borrowed Eq/Ord and do not require Copy.
Array.min           : Ord<'a> => &['a] -> Option<&'a>
Array.max           : Ord<'a> => &['a] -> Option<&'a>
Array.any           : &['a] -> (&'a -> bool) -> bool
Array.all           : &['a] -> (&'a -> bool) -> bool
Array.count         : &['a] -> (&'a -> bool) -> i64
Array.find          : &['a] -> (&'a -> bool) -> Option<&'a>
Array.index_of      : Eq<'a> => &['a] -> &'a -> Option<i64>
Array.contains      : Eq<'a> => &['a] -> &'a -> bool
Array.equal         : Eq<'a> => &['a] -> &['a] -> bool

// Phase 1: sort returns an owned copy, so Copy is required for the initial clone.
Array.sort          : (Ord<'a>, Copy<'a>) => &['a] -> ['a]
Array.sort_by       : Copy<'a> => &['a] -> (&'a -> &'a -> i64) -> ['a]
Array.binary_search : Ord<'a> => &['a] -> &'a -> Option<i64>

// Phase 2 (C02 Vec required): one-pass predicate with owned output.
Array.filter        : Copy<'a> => &['a] -> (&'a -> bool) -> ['a]
```

List phase 1:

```text
List.length    : &[|'a|] -> i64
List.is_empty  : &[|'a|] -> bool
List.map       : Copy<'a> => &[|'a|] -> ('a -> 'b) -> [|'b|]
List.map_ref   : &[|'a|] -> (&'a -> 'b) -> [|'b|]
List.fold      : Copy<'a> => &[|'a|] -> 's -> ('s -> 'a -> 's) -> 's
List.fold_ref  : &[|'a|] -> 's -> ('s -> &'a -> 's) -> 's
List.reverse   : Copy<'a> => &[|'a|] -> [|'a|]
List.to_array  : Copy<'a> => &[|'a|] -> ['a]
```

Phase 1 で List API は O(n) 走査に限定し、sort / binary_search は配列だけ。

### 順序契約

- `map`, `mapi`, `_ref` variants, `fold`, `reduce`, `sum`, `product`, `find`, `index_of`, `equal` は添字 0 から `len-1` へ左から右。
- `fold_back` は `len-1` から 0。
- `any` / `all` / `find` / `index_of` / `contains` / `binary_search` は短絡する。predicate / comparator の評価回数と順序を文書化する。
- `sort` は stable sort。比較関数に副作用はない前提にしないため、bottom-up merge schedule と comparator call sequence を仕様として固定する（設計節参照）。
- `sort_by cmp` は comparator に要素 borrow を渡し、`cmp (&a) (&b) < 0` を a < b、`== 0` を等価、`> 0` を a > b とする。非推移的 comparator の結果は panic/trap ではなく、algorithm の決定的な出力になるが意味は未規定。
- `Array.sum` for floats は左から右の逐次 `+`。LLVM に fast-math / reassociation / vector reduction を許可しない。整数 sum は wrapping addition で結合的なので内部 vectorization 可。

### NaN / signed zero

- `Array.min` / `Array.max` は `Ord` の `<` / `>` を使う。既存 float `Ord` は NaN 比較が false なので、NaN を含む場合:
  - 初期 best は最初の要素。
  - 以後 `candidate < best` / `candidate > best` が true のときだけ更新。
  - best が NaN なら最後まで NaN のまま。
  - candidate が NaN なら更新しない。
- `-0.0` と `+0.0` は既存比較では等価。先に現れた値を保持する。
- `Array.sort` / `Array.sort_by` の primitive `f32` / `f64` は sort 専用の total preorder を使う。すべての数値（±∞を含む）はすべての NaN より前、NaN 同士は等価で stable、`-0.0` と `+0.0` は等価で stable。NaN payload は比較に使わず、stable sort により入力順を保つ。`Array.min` / `max` は上記の逐次 `Ord` 契約を維持し、sort 専用 preorder へ変更しない。

### 実装場所

分類:

| API | 実装 | 理由 |
|---|---|---|
| `length`, `is_empty`, `get`, `at`, `init`, `sub`, `append`, `concat`, `zip`, `reverse`, `to_list` | compiler builtin IR | allocation / bounds / contiguous copy を最小化し、slice fat view を直接扱う。owned output を作るものは `Copy` 制約 |
| `map`, `mapi`, `map_ref`, `mapi_ref`, `fold`, `fold_ref`, `fold_back`, `fold_back_ref`, `reduce`, `any`, `all`, `count`, `find`, `index_of`, `contains`, `equal` | std Tsuzuri source first | callback 特殊化が効く。仕様を読みやすく保つ |
| `sum`, `product`, primitive numeric variants | std generic + optional builtin specialization | 整数は vectorization しやすい builtin variant を後で追加可。float は順序保持 |
| `sort`, `sort_by`, `binary_search` | compiler builtin IR phase 1 | stable sort と allocation/drop が複雑で、Tsuzuri source だと Vec 依存や再帰制限が強い |
| List API | std Tsuzuri source + 必要最小 builtin | リストは contiguous でないため SIMD 期待なし |

`Array.filter` は Phase 2（C02 Vec 完了後）に送る。Vec なしの two-pass 実装は predicate を 2 回呼び、副作用・trap の順序契約を壊すため採用しない。`Array.concat` は predicate を持たず二重評価の問題がないため、Phase 1 に `Copy<'a>` 制約付き builtin として残す。

## 設計

### Builtin signatures

E02 後の signature table に builtin 実装分を追加。`Array.length` は既存 `.length` と同じ lowering を共有する。

```rust
ArrayLength: &[a] -> i64
ArrayInit: i64 -> (i64 -> a) -> [a]
ArraySub: Copy a => &[a] -> i64 -> i64 -> [a]
ArrayAppend: Copy a => &[a] -> &[a] -> [a]
ArraySort: (Ord a, Copy a) => &[a] -> [a]
```

`&[a]` は `Type::Reference(Box::new(Type::Array(Box::new(a))), false)`。

### LLVM shapes

`Array.sub xs start count`:

- evaluate xs, start, count.
- `count < 0` trap。
- `start >= 0 && start <= len && count <= len - start` trap check。
- allocate result count。
- loop `k=0..count-1`: load `xs[start+k]`, `clone_value`, store result[k]。

`Array.append left right`:

- allocate `left_len + right_len` with overflow check。
- copy left then right。
- evaluation order: left, right。

`Array.init n f`:

- same as existing `NewArray`; call `allocate_array` and initializer left-to-right by index。
- `n < 0` trap through `allocation_size` unsigned limit。

`Array.sort`:

- Phase 1 builtin stable merge sort into a new array.
- Copy input to owned result first。
- Use deterministic bottom-up iterative merge sort to avoid recursion and stack growth。
- Allocate scratch array of same len。
- Alternate source/destination buffers per width。
- Comparator:
  - `Ord<'a>` lowering through method call for generic。
  - For primitive ints, direct compare can be builtin specialization。
  - For primitive `f32` / `f64`, use the sort-specific total preorder: numbers before NaNs, NaNs mutually equal/stable, signed zeros mutually equal/stable。
- Stability: when `right < left` false, take left first。
- Deterministic schedule:
  - `width = 1, 2, 4, ...` while `width < len`。
  - For each pass, merge runs `[base, min(base+width,len))` and `[min(base+width,len), min(base+2*width,len))` for `base = 0, 2*width, 4*width, ...`。
  - Comparator calls are exactly the standard merge loop: compare current left and current right once per output position while both runs are non-empty; if `right < left` take right, otherwise take left. Remaining tail is moved without comparator calls。
  - This fixes D-14 observable order for user comparators.
- Drop:
  - Initial clone from borrowed input initializes the result buffer。
  - Each merge pass moves elements between result/scratch buffers by `load` + `store zeroinitializer` to the source slot, never by per-pass `clone_value`。
  - Track which ranges in each buffer are initialized after each pass. At any time an element is initialized in exactly one buffer。
  - At the end, the final initialized buffer becomes the returned array. The emptied scratch buffer is freed without dropping moved-out slots. If the final data is in scratch, return a descriptor pointing at scratch and free the original result buffer; if final data is in result, free scratch。
  - On normal completion `drop_value` later drops only the returned final buffer. During sort, no value is double-dropped。
  - Phase 1 は `Array.sort : (Ord<'a>, Copy<'a>) => &['a] -> ['a]` とする。`Copy` は borrowed input から owned output を作るためであり、比較のためではない。将来 Clone/Move-aware sort または consuming sort で Copy 制約を外す。

`Array.binary_search`:

- left=0, right=len。
- while left < right: mid = left + (right-left)/2 with no overflow。
- compare xs[mid] and target using A11 borrowed `Ord`/`Eq` (`Ord.lt (&xs[mid]) target`, `Eq.eq (&xs[mid]) target`)。二項演算子を borrowed target に直接使わない。
- Found returns first matching index? 既定案: lower_bound then equality check, so duplicate keys return first index。
- Requires sorted input by same `Ord`; unsorted input result unspecified but deterministic。
- target は `&'a` で受け取り、各 probe では A11 の borrowed `Ord`/`Eq` を使う。target も配列要素も comparison では消費しない。

### Std Tsuzuri source sketches

Value callback APIs require `Copy<'a>` because the input is borrowed but the callback receives an owned value. Non-Copy elements use `_ref` variants.

`Array.fold`:

```text
def fold :: Copy<'a> => &['a] -> 's -> ('s -> 'a -> 's) -> 's
fn fold xs state folder = {
    let mut acc = state;
    for x in xs do
        acc = folder acc x;
    acc
}
```

この sketch は `x` が値として渡るため `Copy<'a>` が必要になる。非 Copy 要素向け fold は `fold_ref : &['a] -> 's -> ('s -> &'a -> 's) -> 's` を使う。

既定案:

```text
Array.fold      : Copy<'a> => &['a] -> 's -> ('s -> 'a -> 's) -> 's
Array.fold_ref  : &['a] -> 's -> ('s -> &'a -> 's) -> 's
Array.map       : Copy<'a> => &['a] -> ('a -> 'b) -> ['b]
Array.map_ref   : &['a] -> (&'a -> 'b) -> ['b]
```

要求の API 名は使いやすさ重視で `Copy<'a>` 版にし、非 Copy 版は `_ref` suffix を追加する。C04 ticket 実装者はこの制約を docs に必ず書く。比較-only API は A11 により borrowed `Eq`/`Ord` を使うため Copy を要求しない。

### Determinism

- sort は stable かつ input order deterministic。
- `BTreeMap` / `BTreeSet` で builtin specialization 名を決定。
- benchmark や tests で速度 threshold を置かない。

## 実装手順

1. **API の phase 分割確定**
   - Phase 1: `length/is_empty/get/at/init/sub/append/concat/zip/to_list/fold/fold_ref/fold_back/fold_back_ref/map/map_ref/mapi/mapi_ref/reduce/sum/product/min/max/any/all/count/find/index_of/contains/equal/reverse/sort/sort_by/binary_search`。
   - Phase 2(C02 後): `filter` の一回評価版。
   - 確認: このチケット内の対象外/未決事項を更新。

2. **std module 追加**
   - `std/Array.tz` / `std/List.tz` を E02 規約で追加。
   - helper は E01 後なら `private`。
   - 確認: 未使用 std 関数も型検査される。

3. **builtin IR 追加**
   - allocation 系 API を builtin に追加。
   - `Array.length` は `.length` と同じ IR。
   - 確認: `Array.sub` bounds-before-GEP。

4. **callback API**
   - `map` / `fold` を std 実装し、`call_specialization` が効く IR を確認。
   - 確認: 既知 lambda の hot loop に `@tz.closure.clone` が残らない代表 case。

5. **sort / binary_search**
   - stable sort は output/scratch のため Copy 制約付きで実装し、比較自体は A11 の borrowed `Ord` で行う。
   - bottom-up merge schedule、move-by-load+zero、initialized range tracking、scratch free を実装する。
   - `binary_search` は target を `&'a` で受け、Copy 制約なしで比較できる。
   - 確認: 重複 key の相対順を保つ record test。Copy だが drop が必要な payload（例: 関数値 field を持つ record）で heap tracking `live == 0`。

6. **E2E / benchmarks**
   - fixture と matched C++ benchmark plan。
   - 確認: native/WASM × O0/O3、heap tracking。

## テスト計画

### Rust: `tests/array_bulk.rs`

受理:

- `let xs = [1,2,3]; Array.map (&xs) (x -> x + 1)` → `[2,3,4]`
- `Array.map_ref` with bound `[string]`: `let xs = ["x"]; Array.map_ref (&xs) (s -> s.length)`。
- `let xs = [1,2,3]; Array.fold (&xs) 0 (acc -> x -> acc + x)`。
- `let xs = [1i64,2,3]; Array.sum (&xs)`。
- `let xs = [1.0, -0.0]; Array.sum (&xs)` preserves left-to-right in IR (no fast flags)。
- `let xs = [1,2,3]; Array.find (&xs) (x -> *x == 2)` returns reference to 2。Predicate receives `&i64`; binary operators do not autoderef, so use `*x` or class method explicitly。
- `let xs = [1,2,1]; let needle = 1; Array.index_of (&xs) (&needle)` returns `Some 0`。
- `let strings = ["a","b"]; let needle = "b"; Array.contains (&strings) (&needle)` and `Array.equal (&left_strings) (&right_strings)` work for non-Copy `string` elements without Copy just for comparison。
- `Array.equal` with bound literals: `let left = [1,2]; let right = [1,2]; Array.equal (&left) (&right)` true。
- `let xs = [3,1,2,1]; Array.sort (&xs)` stable。
- float sort: bound array containing numbers, NaNs, `-0.0`, `+0.0`; expected order is numbers first, NaNs last, NaNs stable, signed zeros stable by bit pattern on native/WASM `-O0/-O3`。
- `let xs = [1,2,2,3]; let needle = 2; Array.binary_search (&xs) (&needle)` returns `Some 1`。
- comparison class method form: `Array.index_of (&strings) (&needle)` implementation/tests use `Eq.eq element target` or explicit deref for scalar refs; do not rely on `==` autoderef of borrowed target。

拒否:

- `let xs = ["x"]; Array.map (&xs) (s -> s.length)` with value API → `E1005` / Copy constraint missing。
- `let xs = ["x"]; Array.sum (&xs)` → `E1005` because `sum` is `(Numeric<'a>, Add<'a>)` and string is not `Numeric`。
- `let xs: [i64] = []; Array.reduce (&xs) ...` compiles and returns None, not reject。
- `Array.sort` for non-Copy element in phase 1 → `E1005` Copy constraint missing。理由は owned output/scratch 生成であり、比較ではない。
- Temporary borrow examples like `Array.map (&[1,2,3]) ...` should be rejected with `E1013` until temporary lifetime extension exists; tests must bind literals before borrowing。

IR:

- `Array.sub` contains one allocation and one copy loop。
- `Array.append` copies left then right。
- `Array.sum i64` optimized by Clang `-O3` may contain vector reduction; test should not require it in Rust unit。
- `Array.sum f64` generated Tsuzuri IR has sequential `fadd` and no fast-math flags。

### E2E fixture

公開関数と JS reference:

- `map_checksum(n)`。
- `fold_checksum(n)`。
- `sum_i64(n)` using BigInt wrapping。
- `sum_f64_order()` with values where reassociation changes result。
- `min_max_nan()` verifies NaN contract。
- `sort_stable()` returns encoded stable order。
- `sort_float_total_order()` verifies numbers before NaNs, NaNs stable, and `-0.0`/`+0.0` stable by checking sign bits / payload-insensitive order on native/WASM `-O0`/`-O3`。
- `sort_copy_droppable_payload()` uses a Copy-but-droppable payload such as a record containing a function value, and heap tracking verifies no double drop/leak while merge passes move elements between buffers。
- `binary_search_duplicates()`。
- `sub_bounds(start,count)` trap cases。
- `list_map_fold(n)`。

Targets:

- native/WASM `-O0/-O3`。
- heap tracking `live == 0`。
- WASM imports empty。

### Benchmark plan

Add optional `benchmarks/array-bulk`:

- Tsuzuri `Array.sum i64`, C++ `std::accumulate` over `std::vector<int64_t>`。
- Tsuzuri `Array.sort i64`, C++ `std::stable_sort`。
- Tsuzuri `Array.map` with simple affine function, C++ loop。
- Same allocation/input generation costs included or excluded consistently; document both if measured。
- `--quick` only correctness; no speed threshold。
- Inspect optimized IR/assembly:
  - integer sum may vectorize。
  - float sum must remain ordered。
  - map should not allocate closure per element for known lambda。

## ドキュメント

- `docs/language.md`
  - Array/List module API table。
  - Copy value API と `_ref`/borrowed target API の違い。比較は A11 により非消費だが、値を返す API は `Copy` が必要な場合がある。
  - float sum order、NaN min/max。
- `docs/architecture.md`
  - 一括 API の lowering 方針、vectorization が許される条件。
  - sort stable invariant。
- `docs/benchmarks.md`
  - 測定した場合だけ結果。
- `README.md`
  - 配列 API の短い例。

## 受け入れ条件

- [ ] Phase 1 API が `Array` / `List` 修飾名で利用できる。
- [ ] 読み取り API は `&[T]` を受け、配列全体を clone しない。
- [ ] 順序契約が tests/docs に反映される。
- [ ] `Array.sum f32/f64` は左から右で、fast-math / reassociation なし。
- [ ] `Array.sort` は stable。
- [ ] `Array.sort` の primitive float は sort-specific total preorder（numbers before NaNs、NaNs stable、signed zeros stable）を守る。
- [ ] `Array.binary_search` は重複時 first index。
- [ ] `Array.contains` / `index_of` / `equal` / `binary_search` は non-Copy 要素・target を比較だけの理由で拒否しない。
- [ ] bounds-before-GEP。
- [ ] native/WASM × `-O0`/`-O3`、heap tracking、WASM imports empty。
- [ ] 性能主張をする場合は IR inspection と matched benchmark を docs に記録し、CI threshold なし。

## 落とし穴

- `filter` を two-pass predicate で実装すると副作用順序が変わる。C02 なしでは phase 1 から外す。
- `sum` / `product` は `Numeric<'a>` と、それぞれ `Add<'a>`／`Mul<'a>` に限定し、空配列の identity は numeric 型だけに定義する。任意 Add 型の空配列 sum は提供しない。
- float sum を LLVM vector reduction にしない。
- callback に要素値を渡す API は `Copy<'a>` を要求する。非 Copy は `_ref`。
- stable sort の scratch buffer ownership は難しい。Phase 1 Copy 制約で安全にするが、比較そのものは A11 の borrowed `Ord` を使う。
- binary operators do not autoderef borrowed targets. `Eq.eq element target` / `Ord.lt element target` または `*ref` を使う。
- borrowed input から owned output を作る `sub` / `reverse` / `append` / `concat` / `zip` / conversions は `Copy` 制約なしに実装しない。public deep-clone contract はこのチケットでは導入しない。

## 対象外

- Parallel API（F02）。
- pairwise / Kahan / FMA（D05）。
- CPU runtime dispatch（F05）。
- Hash-based distinct/groupBy。
- `filter` の C02 なし実装。
- list sort。

## 未決事項

- `Array.sum` の空配列: `(Numeric<'a>, Add<'a>)` の numeric builtin だけ 0 identity、`product` は 1 identity。generic Add/Mul は提供せず `reduce` を使わせる。
- `Array.sort` の Copy 制約: phase 1 は `(Ord<'a>, Copy<'a>)`。これは borrowed input から owned output を作るためで、比較のためではない。将来 Clone/Move-aware sort で緩和。
- float NaN sort: 既定案は sort-specific total preorder（numbers before NaNs、NaNs stable、signed zeros stable）。`min`/`max` はこの preorder を使わない。
- 台帳の見直し提案: なし。D-14 に従い、順序違いの高速集計は別 API にする。
