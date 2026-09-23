# C07: ユーザー定義の反復プロトコル
| 項目 | 内容 |
|---|---|
| ID | C07 |
| 優先度 | P2 |
| 規模 | L |
| 依存 | B01, A06 |
| 後続 | – |
| 状態 | todo |
| 主な影響ファイル | `std/Seq.tz`、`src/check.rs` `Type` / `Builtin` / `TypedExprKind::ForEach`、`src/control.rs` `Checker::control_expression`、`src/ownership_control.rs`、`src/llvm_control.rs` `for_each`、`src/llvm.rs` closure/task helpers、`src/computation.rs` builder `For` lowering、`docs/language.md`、`docs/architecture.md`、`tests/iteration_protocol.rs`、`tests/fixtures/iteration_protocol/Main.tz` |

## 目的

`for pattern in source do ...` を配列・リスト・文字列・整数範囲以外のユーザー定義型にも拡張する。現行の型クラス制約では associated element type を表現できないため、Phase 1 は builtin lazy sequence `Seq 'a` を導入し、各型が `Module.iter : &Source -> Seq Element` を提供する方式を採用する。

## 現状

- `src/control.rs` `Checker::control_expression` は `for...in` source を `Type::Array` / `Type::List` / `Type::String` / integer range に限定する。その他は `E1005`。
- `src/llvm_control.rs` `FunctionEmitter::for_each` は `%tz.array` / `%tz.list` / string descriptor を直接走査する。
- `.tc` builder の `For` は `src/computation.rs` で `B.For values (fx element -> body)` へ展開され、通常の言語 `for...in` protocol とは別。
- `src/polymorph.rs` `Classes::collect` はユーザー定義 class を型変数 1 個に限定し、各 method はその型変数だけを含む必要がある。`Iterator source element` や associated type `Element source` は現状表現できない。

## 仕様

### 前提とする他チケットのインターフェース

- B01: `Option 'a`。`Seq.next` の戻り値に使う。
- A06: 型クラス拡張はあるが、Phase 1 では multi-parameter class / associated type はまだ使えない前提。
- C02/C06 が完了していれば `Vec.iter` / `Map.iter` / `Set.iter` を追加できる。C07 自体の必須依存にはしない。

### 他チケットへの提供インターフェース

- C02 は `Vec.iter : &Vec 'a -> Seq &'a` を実装できる。
- C06 は `Map.iter : &Map 'k 'v -> Seq (&'k * &'v)` と `Set.iter : &Set 'k -> Seq &'k` を実装できる。
- F02 は `Seq` を並列化しない。並列 API は `&[T]` などサイズ既知 input を使う。

### 設計選択肢の評価

#### Option A: builtin lazy `Seq 'a`

```text
Seq 'a
Seq.next : Seq 'a -> (Seq 'a * Option 'a)
```

各型が `iter` 関数を提供し、`for` は `Seq` を pull する。element type は `Seq 'a` の型引数として表現できる。現行型システムで実装可能。

採用。

#### Option B: multi-param / associated type class

```text
class Iterable 'source 'item { def iter :: &'source -> Seq 'item }
```

現行 `Classes::collect` の「型変数 1 個・method はその変数だけ」に反する。A10/A06 の範囲拡大が必要。C07 Phase 1 では不採用。

#### Option C: naming convention

`Module.iter source` があれば for が探す。型クラス不要だが、名前探索が ad-hoc になり、ローカル関数や module 衝突で決定性が落ちる。暗黙探索は GUIDE §1 の「暗黙の変換・読み替えをしない」に反するため不採用。

### Phase 1 API

組み込み型:

```text
Seq 'a
```

標準関数:

```text
Seq.next    : Seq 'a -> (Seq 'a * Option 'a)
Seq.empty   : Seq 'a
Seq.once    : 'a -> Seq 'a
Seq.map     : Seq 'a -> ('a -> 'b) -> Seq 'b
Seq.filter  : Seq 'a -> ('a -> bool) -> Seq 'a
Seq.to_array: Seq 'a -> ['a]        // C02 があれば Vec builder 使用、なければ two-pass 不可なので phase 2
```

for-in lowering:

- Source が既存 direct iterable (`[T]`, `[|T|]`, `string`, range) なら既存 lowering を維持する。
- Source が `Seq 'a` なら `Seq.next` を loop で呼ぶ。
- ユーザー型からの for-in は暗黙探索しない。ユーザーは `for x in MyType.iter (&value) do ...` と明示する。

これにより「ユーザー定義プロトコル」は `iter : &T -> Seq U` を提供する library convention と、言語が `Seq U` を for-in 可能にする組み合わせになる。暗黙 associated type がない現状で最も安全。

### 構文

新しい構文はない。

```text
for item in Module.iter (&source) do
    body
```

将来 associated type が入ったら `for item in source do` へ拡張できるが、Phase 1 では暗黙呼び出しをしない。

### Seq 表現

内部型:

```rust
pub enum Type {
    // existing ...
    Seq(Box<Type>),
}
```

LLVM:

```llvm
%tz.seq = type { %tz.closure } ; next thunk/state closure
```

ただし `%tz.closure` は固定 `{ code, env, clone, drop }` なので、`Type::Seq(a)` は LLVM 上 `%tz.closure` と同じにしてもよい。

契約:

- `Seq 'a` は non-Copy。pull するたびに新しい `Seq` と `Option 'a` を返す一回進行の状態。
- `Seq.next` は sequence を消費する。
- `for` は sequence local を mutable state として保持するが、言語上は各 step で `Seq.next seq` の返した新 sequence に置き換える。
- `Seq` は cold task ではなく同期的な純粋値。外部スレッド・I/O は持たない。

### 型・所有権

- `Seq 'a` は常に non-Copy、needs_drop true。
- `Seq` の environment は closure と同じ clone/drop だが、`Seq` 自体は one-shot state として Copy 不可。
- `Seq` が `&T` を element として返す場合、sequence 自体が元 owner の loan を運ぶ。`Seq` を owner より長生きさせると `E1013`。
- `for x in seq` は `seq` を消費する。loop 後に元 `seq` は使えない。
- `Seq.map` / `filter` の callback は通常の関数値。捕捉・借用・Capture 制約は既存通り。

## 設計

### Type support

`src/check.rs`:

```rust
Type::Seq(Box<Type>)
```

性質:

```rust
is_copy = false
needs_drop = true
contains_reference = element.contains_reference()
carries_loans = element.carries_loans(records)
can_capture = element.can_capture(records)
can_send = element.can_send(records) // task captureはclosure envも検査
exportable = false
```

`polymorph.rs` `map_type` / `substitute` / `bounded_type` / `Inference::resolve` / `unify` / `type_expression` に追加。

### Builtin representation

`Seq.next` は builtin wrapper だが、実体は closure call。

Conceptual signature:

```text
Seq.next : Seq 'a -> (Seq 'a * Option 'a)
```

`Seq 'a` の value は next function closure。closure を呼ぶと tuple を返す。tuple の第 1 要素が次 state。

`Seq.empty`:

- next returns `(Seq.empty, None)`。
- clone/drop 可能な singleton closure として生成してよいが、deterministic IR のため通常 builtin function wrapper にする。

`Seq.once value`:

- state is union-like private record `{ used: bool, value: 'a }` が必要。`Seq` builtin closure environment で表現する。
- First next returns `(empty, Some value)`。
- If called after consumed state, returns `(empty, None)`。ただし `Seq.next` consumes state なので同じ state を二度呼ぶことは通常できない。

### ForEach typed IR

既存 `TypedExprKind::ForEach { owner, local, source, body }` を拡張し、source type で lowering を分岐する。

Type checking (`src/control.rs`):

```rust
match source.ty {
    Type::Array(e) | Type::List(e) | Type::String => existing,
    Type::Seq(e) => element = *e,
    _ => E1005
}
```

Error message:

```text
"for...in expects an array, list, UTF-8 string, integer range, or Seq value; call Module.iter explicitly for user-defined types"
```

### Ownership control

`src/ownership_control.rs` `eval_control`:

- For direct collections existing behavior unchanged。
- For `Seq`, `alias_source` should treat source as temporary/owned state, not borrowed collection。
- Loop fixed point:
  - Insert `owner` local with sequence value。
  - Each iteration consumes current sequence and reassigns owner to returned next sequence before body? Ownership model does not represent loop-carried reassignment in typed IR yet。

推奨 typed IR change:

```rust
TypedExprKind::ForSeq {
    owner: Local,
    local: Local,
    source: Box<TypedExpr>,
    body: Box<TypedExpr>,
}
```

Direct `ForEach` remains borrowed collection. `ForSeq` is consuming state loop.

Ownership for `ForSeq`:

1. Evaluate source with `Use::Consume` and bind owner。
2. Fixed-point loop body:
   - `Seq.next owner` consumes owner and produces `(next, option)`。
   - `owner` is reinitialized with next before body。
   - If `None`, loop exits。
   - If `Some value`, bind local for body。
3. References returned as element cannot outlive iteration local (`finish_control_scope`)。

Because modeling Option match inside ownership is complex, Phase 1 may lower `for x in seq` during type checking into an explicit `while`/`match` typed tree using existing constructs. However `while` condition cannot carry `Option` payload. Therefore dedicated `ForSeq` is clearer.

### LLVM lowering

`src/llvm_control.rs`:

```rust
pub(super) fn for_seq(&mut self, owner: &Local, local: &Local, source: &TypedExpr, body: &TypedExpr)
```

IR shape:

1. Evaluate `source`, store in owner slot.
2. Loop:
   - load owner seq closure.
   - call `Seq.next` builtin or direct closure apply。
   - result tuple `{ %tz.seq, %Option.a }`。
   - store next seq into owner slot.
   - inspect Option tag。
   - None -> exit。
   - Some payload:
     - store payload in local slot。
     - emit body。
     - drop local payload at end of iteration。
   - jump loop。
3. At exit drop owner seq。

`Option` layout comes from A02/B01. The ticket implementer must use A02 helper for union tag/payload rather than hardcoding if A02 provides functions.

### Computation builder `For`

Existing builder `For` lowering in `src/computation.rs` remains unchanged: it calls `B.For values (fx element -> body)` and does not use language for-in protocol. This is intentional.

Interplay:

- If a builder wants to support `Seq`, its `For` operation can have `Seq 'a -> ('a -> M) -> M` and call `Seq.next` itself.
- C07 must not rewrite computation `For` to language `for`.

### Vec/Map integration

Optional std functions when dependencies exist:

```text
Vec.iter : &Vec 'a -> Seq &'a
Map.iter : &Map 'k 'v -> Seq (&'k * &'v)
Set.iter : &Set 'k -> Seq &'k
Array.iter : &['a] -> Seq &'a
List.iter : &[|'a|] -> Seq &'a
```

Language direct for over arrays/lists stays faster and allocation-free. `Array.iter` is for higher-order composition.

## 実装手順

1. **Type::Seq**
   - Add type variant and all type walkers。
   - Confirm display `Seq i64`。

2. **B01/A02 union access helpers確認**
   - Identify Option tag/payload API from B01/A02。
   - If no helper, add small internal helper in LLVM module for `Option` inspection with documented layout。
   - 確認: `Seq.next Seq.empty()` returns None in IR test。

3. **Seq builtins**
   - Implement `Seq.empty`, `Seq.next`, `Seq.once`。
   - Confirm drop/clone of sequence environment。

4. **ForSeq typed IR**
   - Add `TypedExprKind::ForSeq` and children。
   - Type checker accepts `Seq 'a` in `for...in`。
   - 確認: `for x in Seq.once 1 do ...` accepted。

5. **Ownership**
   - Add `eval_control` case for ForSeq。
   - Ensure element borrow cannot escape。
   - Confirm source seq consumed and move-after-use rejected。

6. **LLVM lowering**
   - Implement `for_seq` loop。
   - Confirm no alloca inside loop except entry slots。

7. **Std iter functions**
   - Add `Array.iter` / `List.iter` first。
   - Vec/Map/Set iter only if C02/C06 available。

8. **Docs/tests**
   - Document explicit `Module.iter` convention and no implicit search。
   - E2E native/WASM。

## テスト計画

### Rust: `tests/iteration_protocol.rs`

受理:

- `for x in Seq.once 42 do total = total + x`
- `for x in Seq.empty() do ...` executes zero times。
- `Array.iter (&xs) |> Seq.map ...` if pipe usage works。
- `for r in Array.iter (&strings) do total = total + r.length` borrowed string elements。
- User module:
  ```text
  record Counter { start: i64, finish: i64 }
  def iter :: &Counter -> Seq i64
  ```
  then `for x in Counter.iter (&c) do ...`

拒否:

- `for x in counter do ...` without explicit iter → `E1005`。
- `let seq = Seq.once "x"; let _ = Seq.next seq; Seq.next seq` → `E1012`。
- sequence returning borrowed element outliving owner → `E1013`。
- task capturing `Seq &i64` → `E1013`。

IR:

- `ForSeq` has loop with `Seq.next` call and Option tag branch。
- entry block contains allocas; loop body does not create dynamic alloca。
- direct array for-in still uses old `for_each` and does not call Seq。

### E2E fixture

公開関数:

- `seq_once()` returns 42。
- `seq_range_sum(n)` user-defined Counter.iter reference sum。
- `seq_map_filter(n)` expected JS reference。
- `seq_borrowed_strings()` heap tracking。
- `seq_empty()` zero iterations。

Targets:

- native/WASM `-O0/-O3`。
- heap tracking `live == 0`。
- WASM imports empty。

## ドキュメント

- `docs/language.md`
  - `for...in` accepts `Seq`。
  - User-defined iteration is explicit `Module.iter (&value)`。
  - `Seq` is one-shot/non-Copy。
- `docs/architecture.md`
  - Reason associated type is not expressible with current Classes。
  - `ForSeq` lowering and ownership fixed point。
- `README.md`
  - Short example。

## 受け入れ条件

- [ ] `Seq 'a` 型と `Seq.empty` / `Seq.once` / `Seq.next` が動く。
- [ ] `for...in` accepts `Seq 'a` and direct collections unchanged。
- [ ] User-defined iter is explicit; no implicit name search。
- [ ] Seq is non-Copy and consumed by iteration。
- [ ] Borrowed elements cannot escape owner/iteration。
- [ ] Computation builder `For` behavior unchanged。
- [ ] native/WASM E2E and heap tracking pass。

## 落とし穴

- Single-parameter class limitation means `Iterable source item` cannot be encoded。無理に型クラスで表現しない。
- `Seq` を Copy にすると one-shot state が二重実行される。
- `ForSeq` を `while` + `Option` desugar にすると ownership の再初期化が漏れやすい。専用 IR が安全。
- direct array for-in を Seq 経由に退化させると性能と allocation が悪化する。既存 lowering を維持。
- Builder `For` と language `for...in` を混同しない。

## 対象外

- Associated types / multi-parameter classes。
- Implicit `iter` lookup。
- Async streams。
- Parallel sequence processing。
- Infinite sequence termination guarantee。
- Reusable Copy iterator。

## 未決事項

- 推奨は Option A: builtin `Seq 'a`。Option B/C は不採用。
- `Seq.to_array` は C02 Vec がある場合だけ phase 2 で実装。C07 phase 1 では省略可。
- `for x in source` の暗黙 `Module.iter` は将来 associated type が入るまで導入しない。
- 台帳の見直し提案: なし。A06/A10 で associated type を導入する場合に C07 phase 2 として再設計する。
