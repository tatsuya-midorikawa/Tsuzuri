# C06: Map／Set
| 項目 | 内容 |
|---|---|
| ID | C06 |
| 優先度 | P2 |
| 規模 | L |
| 依存 | A11, A02, A06, A07, C02, B01 |
| 後続 | – |
| 状態 | todo |
| 主な影響ファイル | `std/Map.tz`、`std/Set.tz`、`src/check.rs` `Type` / `Builtin`、`src/polymorph.rs`、`src/llvm.rs`、`src/llvm_frame.rs`、`src/ownership.rs`、`src/call_specialization.rs`、`docs/language.md`、`docs/architecture.md`、`tests/map_set.rs`、`tests/fixtures/map_set/Main.tz` |

## 目的

決定的な順序で反復できる連想コンテナ `Map<'k, 'v>` と `Set<'k>` を導入する。キーは `Ord` で整列し、挿入・削除は C01/C02 と同じく所有値を消費して新しい値を返す。一意所有時は内部 storage を in-place 更新してよい。

Phase 1 は B-tree/AVL ではなく **sorted contiguous entries** を採用する。これにより A04 recursive heap types を追加依存にしない。lookup は O(log n)、insert/remove は shift のため O(n)。将来、大規模データ向けに B-tree 実装へ移す場合は A04 を依存に追加する。

## 現状

- `Type` に Map/Set はない。
- C02 完了後に `Vec` と `@tz.realloc` が使える。
- A02/B01 完了後に `Option<'a>` がある。
- A11 後は `Ord` / `Eq` が borrowed signature になり、Map/Set の key comparison は key を消費しない。Phase 1 で `Copy<'k>` が残るのは、key を値として返す snapshot API などに限る。
- レコード field は immutable で、可変共有状態はない。Map/Set も immutable value として扱う。

## 仕様

### 前提とする他チケットのインターフェース

- A02/B01: `Option<'a> = None | Some of 'a`。
- A11: `Ord` / `Eq` comparison は borrowed であり、key comparison のためだけに `Copy<'k>` を要求しない。
- A06: 型クラス拡張後も既存 `Ord` / `Eq` の組み込み・ユーザー instance が使える。
- A07: `Ord` / `Eq` deriving により record/union key を使える。
- C02: `Vec` と `@tz.realloc` の growth/drop/clone helper。

### 他チケットへの提供インターフェース

- C07 は `Map` / `Set` の for-in を iteration protocol の標準実装例にできる。
- D01/Debug は `Map.to_array` 経由で表示を実装できる。

### 型

Surface:

```text
Map<'k, 'v>
Set<'k>
```

内部:

```rust
pub enum Type {
    // existing ...
    Map(Box<Type>, Box<Type>),
    Set(Box<Type>),
}
```

LLVM phase 1:

```llvm
%tz.map = type { ptr, i64, i64 } ; entries, len, cap
%tz.set = type { ptr, i64, i64 } ; keys, len, cap
```

`%tz.map` の `ptr` は `[cap x { K, V }]`、`%tz.set` は `[cap x K]`。`len` 個だけ初期化済み。常に key 昇順。重複 key なし。

性質:

- `Map` / `Set` は常に non-Copy。
- `needs_drop = true`。
- `contains_reference` / `carries_loans` / `can_capture` / `can_send` は要素型へ再帰。
- `exportable = false`。
- `layout_size` は 32 bytes。

### API

Phase 1 signatures:

```text
Map.empty        : Map<'k, 'v>
Map.singleton    : 'k -> 'v -> Map<'k, 'v>
Map.length       : &Map<'k, 'v> -> i64
Map.is_empty     : &Map<'k, 'v> -> bool
Map.insert       : Ord<'k> => Map<'k, 'v> -> 'k -> 'v -> Map<'k, 'v>
Map.remove       : Ord<'k> => Map<'k, 'v> -> 'k -> Map<'k, 'v>
Map.contains_key : Ord<'k> => &Map<'k, 'v> -> 'k -> bool
Map.get          : (Ord<'k>, Copy<'v>) => &Map<'k, 'v> -> 'k -> Option<'v>
Map.at           : Ord<'k> => &Map<'k, 'v> -> 'k -> &'v
Map.to_array     : (Copy<'k>, Copy<'v>) => &Map<'k, 'v> -> [('k * 'v)]
Map.keys         : Copy<'k> => &Map<'k, 'v> -> ['k]
Map.values       : Copy<'v> => &Map<'k, 'v> -> ['v]
Map.fold         : &Map<'k, 'v> -> 's -> ('s -> &'k -> &'v -> 's) -> 's

Set.empty        : Set<'k>
Set.singleton    : 'k -> Set<'k>
Set.length       : &Set<'k> -> i64
Set.insert       : Ord<'k> => Set<'k> -> 'k -> Set<'k>
Set.remove       : Ord<'k> => Set<'k> -> 'k -> Set<'k>
Set.contains     : Ord<'k> => &Set<'k> -> 'k -> bool
Set.to_array     : Copy<'k> => &Set<'k> -> ['k]
Set.union        : Ord<'k> => Set<'k> -> Set<'k> -> Set<'k>
Set.intersect    : (Ord<'k>, Copy<'k>) => &Set<'k> -> &Set<'k> -> Set<'k>
Set.difference   : (Ord<'k>, Copy<'k>) => &Set<'k> -> &Set<'k> -> Set<'k>
```

`Map.at` は key が無ければ trap。`Map.get` は `Option` を返すが value を値として返すため `Copy<'v>`。非 Copy value には `Map.at` + `clone_string` など借用 API を使う。

`remove` / `contains_key` / `get` / `at` の query key は値として受け取り、検索中は内部で借用して比較し、呼び出し後に drop する。既存 key を保持したい呼び出し元は `clone_string` などで明示的に所有値を用意する。将来、必要なら borrowed-key overload を別 API として追加する。

`Map.insert map key value`:

- key が無ければ挿入。
- key が等価なら既存 key は保持し、旧 value を drop、新 key を drop、新 value を格納する。
- これにより `Ord` 上等価だが bit 表現が違う key の代表は最初に挿入されたもの。

Iteration order:

- `Map.to_array` / `Map.fold` / `for...in` は key 昇順。
- `Set.to_array` / `Set.fold` は key 昇順。

### for-in

Phase 1 で専用 `for...in` を追加する場合:

- `for (k, v) in map do` は `('k * 'v)` を値として渡すため `Copy k, Copy v` が必要。
- 非 Copy 対応は `Map.fold` の borrowed callback を推奨。

既定案: C06 では language `for...in` への追加は行わず、C07 の iteration protocol に委ねる。Phase 1 は `Map.fold` / `to_array`。

## 設計

### 実装方式の選択

採用: compiler builtin opaque type + sorted contiguous storage。

理由:

- std の public generic record だと内部 sorted invariant をユーザーが壊せる。
- private record にすると型自体を公開しにくい。
- B-tree/AVL を Tsuzuri source で書くと A04 recursive types が必要。C06 の依存表に A04 がないため不採用。
- sorted contiguous storage は小〜中規模で cache locality が高く、実装・検証が容易。

追加依存:

- A04 は追加しない。将来 tree 実装へ変更する場合は C06 phase 2 として A04 を追加する。

### Binary search helper

Map/Set lowering は helper を持つ。

```rust
fn map_find(&mut self, key_ty: &Type, entries: &str, len: &str, key: &str) -> (found: String, index: String);
```

- lower_bound を返す。
- `found` は `index < len && !(key < entries[index].key) && !(entries[index].key < key)`。
- comparisons は A11 の borrowed `Ord` method を呼ぶ。query key と stored key は必要回数借用でき、比較のために `Copy<'k>` を要求しない。
- primitive numeric key は直接 compare builtin specialization を許す。

### Insert

Pseudo:

1. lower_bound。
2. found:
   - old entry pointer。
   - load old value、drop old value。
   - drop incoming key。
   - store incoming value into old value slot。
   - return same map descriptor。
3. not found:
   - reserve len+1。
   - shift entries `[index..len)` one slot right from back to front。bitwise move なので moved-from slots は未初期化扱い。
   - store key/value at index。
   - len+1。

Shift は overlap があるため `llvm.memmove` を使うか backward loop。要素に ownership があるため memmove 後に old slots を drop してはいけない。descriptor len を更新して新しい範囲だけ drop 対象にする。

### Remove

1. lower_bound。
2. not found: return map unchanged。
3. found:
   - drop key/value at index。
   - shift `[index+1..len)` left one slot。
   - len-1。
   - optional zero last slot for debug。

### Drop / clone

`drop_value(Type::Map(k,v))`:

- loop `0..len`。
- drop key if needed。
- drop value if needed。
- `@tz.free(data)`。

`clone_value(Type::Map)`:

- for function environment clone only。
- allocate len capacity len。
- clone each key/value with `clone_value` helper。

`Map.clone` public API は Phase 1 では提供しない。必要なら `(Copy k, Copy v) => &Map k v -> Map k v` を後で追加。

### Set

Set は Map の value なし版。`Set.union` は two-pointer merge:

- Inputs are owned for `union` to allow in-place reuse of left buffer when capacity sufficient。
- Since both sets consumed, result may reuse larger capacity buffer from left。右 buffer は drop/free。
- `intersect` / `difference` は borrowed inputs and new allocation。

### Diagnostics

- Type mismatch: `E1003`。
- Missing Ord/Copy: `E1005` no instance。
- Public ABI: `E1008`。
- Borrow escape: `E1013`。
- Move after insert/remove: `E1012`。

## 実装手順

1. **型追加**
   - `Type::Map` / `Type::Set` と polymorph walkers。
   - 確認: `Map<i64, string>` display。

2. **LLVM drop/clone**
   - `%tz.map` / `%tz.set` header。
   - drop/clone loops。
   - 確認: function environment clone with Map has independent storage。

3. **Basic builtins**
   - empty/singleton/length/is_empty/to_array。
   - 確認: IR 決定性。

4. **Binary search and lookup**
   - contains/get/at。
   - 確認: duplicate keys first representative。

5. **Consuming updates**
   - insert/remove, Set insert/remove/union。
   - 確認: move後使用、drop old values、heap tracking。

6. **Fold / borrowed iteration**
   - `Map.fold` / `Set.fold` with borrowed key/value。
   - 確認: non-Copy value can be inspected without copy。

7. **Docs / E2E**
   - deterministic order and complexity table。
   - native/WASM checks。

## テスト計画

### Rust: `tests/map_set.rs`

受理:

- `Map.empty() |> Map.insert 2 "b" |> Map.insert 1 "a"` equivalent curried syntax。
- `Map.get (&m) 1 == Some "a"` for Copy values like i64。
- `Map.at (&m) 1` borrowed string length。
- replacing existing key drops old value and keeps length。
- `Map.remove` absent key is no-op。
- `Map.to_array` sorted by key。
- `Set.union`, `intersect`, `difference` sorted and duplicate-free。
- `Map.fold` over string values without Copy。

拒否:

- key type without Ord → `E1005`。
- non-Copy key with `Map.keys`, `Map.to_array`, `Set.to_array`, borrowed-input `Set.intersect`/`difference` → `E1005` Copy constraint。理由は返却用の値コピーであり、key comparison ではない。
- non-Copy value with `Map.get` → `E1005` Copy constraint。
- `let m2 = Map.insert m 1 2; Map.length (&m)` → `E1012`。
- `export def f :: Map<i64, i64>` → `E1008`。

IR:

- insert uses binary search loop then shift。
- lookup does not allocate。
- to_array allocates exactly one array。

### E2E fixture

公開関数:

- `map_insert_lookup(n)` with JS `Map` reference sorted by numeric key。
- `map_replace_drop(n)` string values and heap tracking。
- `map_remove(n)`。
- `set_algebra(n)` with JS `Set` reference。
- `map_at_missing()` trap child process。

Targets:

- native/WASM `-O0/-O3`。
- heap tracking `live == 0`。
- WASM imports empty and 16 MiB trap case。

## ドキュメント

- `docs/language.md`
  - `Map` / `Set` type and API。
  - Complexity: lookup O(log n), insert/remove O(n), iteration O(n)。
  - `Copy` が必要なのは key/value を値として返す API に限り、lookup/update の key comparison は A11 により非消費であること。
- `docs/architecture.md`
  - sorted contiguous representation, deterministic iteration。
  - future B-tree requires A04。
- `README.md`
  - short example。

## 受け入れ条件

- [ ] `Map<'k, 'v>` / `Set<'k>` 型が使える。
- [ ] key order deterministic。
- [ ] insert/remove consumes owner and rejects move-after-use。
- [ ] lookup by borrow allocates no storage。
- [ ] `Map.at` borrowed value lifetime is tied to map input。
- [ ] duplicate insert replaces value and keeps length。
- [ ] native/WASM E2E and heap tracking pass。
- [ ] docs include complexity and Phase 1 limitations。

## 落とし穴

- A11 の borrowed `Ord` を使わず key を値として比較すると、non-Copy key が使えなくなる。binary search helper は必ず stored key と query key を借用して比較する。
- Shift with `memmove` must not double-drop moved entries。
- Replacing equal key must drop incoming key if not stored。
- `Map.to_array` for non-Copy values would clone/move from borrowed map; require Copy。
- If representation is public, users can break sorting invariant。Use compiler builtin opaque type。

## 対象外

- HashMap/HashSet。Hash は A07 だが ordered deterministic container が first。
- B-tree/AVL tree。A04 phase 2。
- Mutable iterators。
- Range queries。
- Custom comparator stored in map。
- Public ABI。

## 未決事項

- A11 により key comparison 用の `Copy<'k>` は不要。Phase 1 で `Copy<'k>` を残すのは `Map.keys` / `Map.to_array` / `Set.to_array` / borrowed-input set algebra のように key を返却用に複製する API だけ。
- Implementation is sorted contiguous storage。A04 は追加しない。tree に変更する場合だけ A04 を依存へ追加する。
- `Map.at` missing key は trap、`Map.get` は Copy value Option の既定案。
- 台帳の見直し提案: なし。D-20/A11 の borrowed `Ord` を前提にする。
