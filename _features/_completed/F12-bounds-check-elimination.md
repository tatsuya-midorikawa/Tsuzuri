# F12: 境界検査の除去と検査コストの計測

| 項目 | 内容 |
| --- | --- |
| ID | F12 |
| 優先度 | P2 |
| 規模 | L |
| 依存 | – |
| 後続 | – |
| 状態 | done（Phase 0・1） |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 不要 |
| 改善する劣位 | C/C++ 比: 安全検査のコスト（[なぜ Tsuzuri か](../../_docs/learn/why-tsuzuri.md#cc-に対する劣位点)） |
| 手本にする既存実装 | 関数単位の事前解析: `src/call_specialization.rs` の `single_use_locals`（`FunctionEmitter::emit` の冒頭で一度計算し、field `FunctionEmitter::single_use` に保持）。保守的な変更検出: 同ファイルの `may_mutate`。子の走査: `src/check.rs` の `TypedExpr::children`。検査の省略先: `src/llvm.rs` の `element_pointer`（検査なしの GEP）と `checked_element_pointer`（検査付き）。E2E の suite と trap: `tests/features.mjs` の `simd` suite（`cases`・`traps`） |
| 主な影響ファイル | `src/ranges.rs`（新規）, `src/lib.rs`, `src/llvm.rs`, `tests/bounds_checks.rs`（新規）, `tests/fixtures/bounds_checks/Main.tz`（新規）, `tests/features.mjs`, `tests/trap_locations.rs`, `benchmarks/control/Main.tz`, `benchmarks/control/host.c`, `benchmarks/control/reference.c`, `benchmarks/control/reference.rs`, `benchmarks/run-control.mjs`, `docs/benchmarks.md`, `docs/architecture.md`, `_docs/guides/performance.md`, `_docs/feature-status.md`, `_features/README.md` |

## 目的

配列の境界検査のコストを、トラップの有無・位置・順序を変えずに減らす。
コンパイラが型付き IR の上で「添字が必ず範囲内」と証明できた位置だけ、検査の分岐を生成しない。
証明できない位置は今までどおり検査し、トラップの種類（`TrapKind::BoundsCheck`）と位置情報（`TrapSite`）を保つ。
あわせて、検査の残り方と時間を計測して退行を検出できるようにする。

範囲解析は新しいモジュール `src/ranges.rs`（新規）に置き、PR02（整数の範囲と overflow フラグ）がこの上に算術の事実を足す。

実装者は Phase 0（計測）と Phase 1（証明による省略）を実装する。Phase 2（ループの版分けなど）は設計方針だけで、人間が求めた場合だけ着手する。

## 着手条件と停止条件

### 着手条件

- 依存はない。PR02 は F12 の `src/ranges.rs` を前提にするため、F12 を先に完了させる（`_features/README.md` と
  `_perfs/README.md` の状態欄で確認: `grep -n "F12\|PR02" _features/README.md _perfs/README.md`）。
- 承認が要る決定はない（決定事項はすべて既定案）。
- GUIDE §2.3 の基準コマンドが成功し、実装手順 1 のベースライン（IR と `run-control.mjs --quick`）を保存していること。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- 証明規則（「仕様」の証明規則表）にない形を省略したくなった。規則の追加は人間の判断を要する。
- `Local::id` が一つの `CheckedFunction` の中で一意でない例が見つかった（解析の前提が崩れる）。
- 配列の束縛が、`TypedExprKind::Assign`・`TypedExprKind::Borrow(_, true)`・move 以外の経路で別の値に変わる例が見つかった。
- 省略のために `llvm.assume`・`!range`・`nsw`・`nuw`・`unsafe`・新しい crate・既定の WASM import が必要になった（D2、D7）。
- 既存テストの期待値（IR の文字列、trap 表、診断）を変える必要がある。「既存テストへの影響」に挙げたものは除く。
- `run-control.mjs` の既存 workload の中央値が、ベースラインの広がりを超えて遅くなった。
- stack-depth の 3 テスト（GUIDE §11.1 の一覧: `bounds_type_growing_polymorphic_recursion`、
  `bounds_recursive_and_flat_expression_depth`、`bounds_nested_builder_expansion_not_just_source_syntax`）のどれかが失敗した。
  解析を再帰で書いて stack が足りなくなった場合も含む。上限や stack の大きさは上げない。

## 現状（HEAD `f8dc655` で確認）

- 境界検査は `FunctionEmitter::guard(valid, TrapKind::BoundsCheck)` が作る。`guard` は `valid` で分岐し、失敗側で
  `emit_trap`（`call void @llvm.trap()`、`FunctionEmitter::trap_kind` を一時的に設定して trap 表へ種類を記録）と `unreachable` を出す。
- `BoundsCheck` の `guard` の呼び出し元:
  - `src/llvm.rs`: `checked_element_pointer`（配列・`Vec`・リストの添字）、`array_slice`（`0 <= start <= end <= length`）、
    文字列の添字（`expression` の `TypedExprKind::Index(string, index) if string.ty.is_string()`）、builtin の添字（`emitter.guard` の 1 か所）。
  - `src/llvm_bulk.rs`（`Array.set`・`update`・`swap` と `Vec` の builtin）、`src/llvm_io.rs`、`src/llvm_simd.rs`、`src/llvm_abi.rs`（公開 ABI の入口）。
- 配列の添字 `values[i]` は `FunctionEmitter::expression` の `TypedExprKind::Index(array, index)` と、place を作る関数の
  `TypedExprKind::Index(value, index)`（`shared_array_deref` で `ref [T]` を見分ける分岐を含む）の 2 か所で、どちらも
  `checked_element_pointer(ty, collection, index)` を呼ぶ。`checked_element_pointer` は長さを `extractvalue` で読み、
  `icmp ult i64 index, length` を `guard` へ渡し、成功側で `element_pointer`（`getelementptr inbounds`）を作る。
  `ult` の一回の比較で負の添字も拒否している。
- `for` の範囲ループは `src/llvm_control.rs` の `range_loop`。`start`・`step`・`finish` をループの前に一度だけ評価する。
  step が `1`（または符号付きの `-1`）の 64 bit の経路は、`icmp sle first, last` で空を判定し、`phi` の現在値を反復変数の slot へ
  store し、latch で `icmp eq current, last` を見てから `add`（フラグなし）で進む。終端を含む（`0 .. n - 1` は `0` から `n - 1`）。
  64 bit 未満の反復変数は `narrow_range_loop` を通り、そこだけが証明済みの範囲を `llvm.assume` で伝える（フラグは付けない）。
- 範囲解析・ループの版分けはない。整数の算術に `nsw`／`nuw` を付けないことを `src/llvm.rs` の
  `emits_explicit_tail_loop_and_checked_arithmetic` が確認している。
- 配列 `[T]` は生成後に長さも要素も不変。`let mut`／`ref mut [T]` でできるのは配列全体の置換だけ（docs/language.md の配列の節）。
  共有スライスの生存中は元配列の move・置換・排他借用を所有権検査が拒否する。
- 関数単位の解析の前例: `FunctionEmitter::emit` は冒頭で `call_specialization::single_use_locals(&self.function.body)` を計算する。
  `call_specialization` は `src/llvm.rs` の中の `#[path = "call_specialization.rs"] mod call_specialization;` で、crate の他の場所からは使えない。
- 計測: `benchmarks/run-control.mjs` の workload は 15 個（`while_mix` … `float64_mix`）で、配列は `array_sum`・`array_copy`
  （どちらも `for value in values`）だけ。添字ループの workload はない。
- 2026-09-29 の確認（arm64 macOS、`--emit object -O3`）: `for value in values` は要素ごとの検査を生成しない。
  `for i in 0 .. values.length - 1 do total = total + values[i]` は `-O0` の IR に要素ごとの検査があり、`-O3` では LLVM が除去して
  ベクトル化した。残ったのは公開 ABI の入口の検査だけ。入れ子の配列、別の配列から読んだ添字、非アフィンな添字、途中脱出のあるループは未計測。

### 再現（検証済み）

次の 2 関数は HEAD で `check` と `build` が成功する。`--emit llvm` の IR には `icmp ult i64` が 8 個ある。

```tsuzuri
export def sum :: ref [i64] -> i64
fn sum values =
    let mut total = 0
    for i in 0 .. values.length - 1 do
        total = total + values[i]
    total

export def pick :: i64 -> i64
fn pick index =
    let values = [10, 20, 30]
    let guarded = if index >= 0 && index < values.length then values[index] else 0
    values[2] + guarded
```

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
target/release/tsuzuri check /tmp/tz-work-F12/loop
target/release/tsuzuri build /tmp/tz-work-F12/loop --emit llvm -o /tmp/tz-work-F12/loop.ll
awk '/^define internal i64 @tz.fn.Main.sum/,/^}/' /tmp/tz-work-F12/loop.ll
```

`@tz.fn.Main.sum` のループ本体（抜粋、HEAD の出力そのまま）:

```llvm
b1:
  %v5 = phi i64 [ 0, %loop ], [ %v6, %b2 ]
  store i64 %v5, ptr %v7
  %v9 = load i64, ptr %v1
  %v10 = load %tz.array, ptr %v0
  %v11 = load i64, ptr %v7
  %v12 = extractvalue %tz.array %v10, 1
  %v13 = icmp ult i64 %v11, %v12
  br i1 %v13, label %b5, label %b6
b6:
  call void @llvm.trap()
  unreachable
b5:
  %v14 = extractvalue %tz.array %v10, 0
  %v15 = getelementptr inbounds i64, ptr %v14, i64 %v11
  %v16 = load i64, ptr %v15
```

## 仕様

### 前提とする他チケットのインターフェース

- 前提にするものはない。F12 は次のインターフェースを後続へ提供する（名前は D3 で確定）。
  - PR02: `src/ranges.rs`（新規）の `RangeFacts`・`analyze` に、算術の overflow の事実（`nsw`／`nuw` の根拠）を足す。
    F12 は `RangeFacts` の field を `pub(crate)` にしない。PR02 は同じファイルに問い合わせ関数を足す。
  - A16: `Type::FixedArray`（A16 の新規）の長さは型から分かる。定数長の規則 R4 を型へ広げるのは A16 の担当で、F12 は `Type::Array` だけを扱う。

### 意味（変えないもの）

- 省略してよいのは、`guard` の条件 `icmp ult i64 index, length` が常に真だと証明規則で示せた位置だけ。
  トラップする入力の位置では検査が残るので、トラップの有無・位置・種類・先行する副作用の順序は変わらない。
- 検査を移動・合併・前倒ししない。検査の分岐を消すだけで、`element_pointer`（`getelementptr inbounds`）と `load` はそのまま出す。
- 証明が誤っていれば範囲外の読み出し（未定義動作）になる。したがって規則は下の表で閉じ、表にない形はすべて検査を残す。
- `llvm.assume`・`!range`・`nsw`・`nuw` を新しく出さない（D2、D7）。
- 解析は LLVM より前の型付き IR で行うので、省略は `-O0`・`-O3`、native・WASM で同じになる。生成 IR は決定的なまま。
- 省略した位置は trap 表（`emit_with_trap_info` の `trap_sites`）に現れない。残った検査の `TrapKind::BoundsCheck` と `TrapSite::span` は変えない。

### 用語

- 配列の根 `root(A)`: 添字式の配列側 `A` から、`Borrow(_, false)`・`BorrowOperand`・`Dereference` を外して得る `Local(v)` の `v`。
  `v` の型は `Type::Array(_)` か `Type::Reference(Array, false)`。`Type::Reference(_, true)`・`Vec`・リスト・文字列は根にしない。
- 安定: 関数本体のどこにも、根が `v` の `Assign(target, _)`、`Borrow(target, true)`、型が `Type::Reference(_, true)` の
  `BorrowOperand(target)` がない。安定な `v` は、スコープの全体で同じ配列の値を指す（配列は生成後に長さが不変）。
- 長さ式 `len(v)`: `TypedExprKind::Length(A)`（`values.length`）、または std の `Array.length`（`std/Array.tz` の `length`、
  `FunctionOrigin::module == ModuleOrigin::Std`、`CheckedFunction::module == "Array"`、名前の `.$mono.` より前が `length`）を
  引数一つで呼ぶ `Call`。どちらも `root(A) == v`。AGENTS.md の「同等の builtin と演算子の形は同じ性能経路を選ぶ」に従い両方を認める。
- 反復変数: `ForRange::local` で型が `Type::I64` のもの。

### 証明規則

| 規則 | 型付き IR の形 | 条件 | 得る事実 |
| --- | --- | --- | --- |
| R1 前進ループ | `ForRange { local: i, start: Int(0), step: Int(1), finish: Binary(Subtract, len(v), Int(1)), .. }` | `v` と `i` が安定 | 本体で `0 <= i <= len(v) - 1` |
| R2 後退ループ | `ForRange { start: Binary(Subtract, len(v), Int(1)), step: Int(-1), finish: Int(0), .. }`（`-1` は `u128::MAX >> 64`） | 同上 | 同上 |
| R3 定数ループ | `ForRange { start: Int(a), step: Int(1) または Int(-1), finish: Int(b), .. }`、`a`・`b` はともに `2^63` 未満 | `i` が安定 | 本体で `min(a, b) <= i <= max(a, b)` |
| R4 定数長 | `Block` の束縛 `(v, e)` で、`e` が `Array(elements)`・`NewLiteral(Array(elements))`・`NewArray(Int(n), _)`（`n < 2^63`） | `v` が安定 | `len(v) == elements.len()` または `n` |
| R5 支配する検査 | `If { condition: C, then_branch: T, .. }` で、`C` を `Binary(And, ..)` で平らにした項に `i >= 0`（または `0 <= i`）と `i < len(v)`（または `len(v) > i`、`i <= len(v) - 1`）がある | `i`（型 `Type::I64`）と `v` が安定 | `T` の中で `0 <= i <= len(v) - 1` |

添字 `Index(A, I)` の検査を省くのは、`A` の型が（参照を外して）`Type::Array` で `root(A) == v` があり、次のどれかが成り立つときだけ。

| 証明 | `I` の形 | 根拠 |
| --- | --- | --- |
| P1 | `Local(i)` | R1・R2・R5 で `i` が `len(v) - 1` 以下（同じ `v`） |
| P2 | `Local(i)` | R3 で `hi` が上限、R4 で `len(v) == n`、`hi < n` |
| P3 | `Int(k)` | R4 で `len(v) == n`、`k < n`（負の定数は `k >= 2^63` なので自動的に外れる） |

R1 の妥当性: `range_loop` は `finish` をループの前に一度だけ評価する。`len(v) >= 0` なので `len(v) - 1` は折り返さず、`-1` なら空ループ。
本体の `i` は `phi` の値を store したもので、`0` から `len(v) - 1` までを動く。`v` は安定なので、本体で読む `len(v)` は入口の値と同じ。
R5 の妥当性: `&&` は短絡評価で、`T` に入るのは全項が真のときだけ。`i` と `v` は安定なので、`T` の中で値が変わらない。

閉包の本体（`Lambda`）の中は事実を作らない。閉包の本体を別の `CheckedFunction` として出す場合、その関数の解析は外側のループを見ないので検査が残る。

### 数値・トラップ・native と WASM の差

- 省略の判断は target に依存しない。native と WASM の IR は、既存の target 差（ABI・`@tsuzuri_trap_site` など）以外で一致する。
- 整数の算術・比較の命令は変えない。`len(v) - 1` の `sub` もフラグなしのまま。
- トラップする入力（範囲外の添字）では、省略されない位置の検査が今までどおり `llvm.trap` に至る。

### 診断

新しい診断はない。検査が残ったことを知らせる警告も出さない（D6）。

### 資源上限

- 解析は関数ごとに一回、型付き IR の節点を worklist で走査する（再帰しない）。節点数が `ranges::NODE_BUDGET`（新規、`65_536`）を
  超えた関数は `RangeFacts::default()`（事実なし、すべての検査を残す）にする（D5）。
- 表は `BTreeMap`／`BTreeSet`／`Vec` だけを使い、`HashMap` を使わない。
- 特殊化の上限（1,024）や stack の深さの上限には触れない。

### 例

次の例はすべて HEAD で `check` が成功する（「再現」と `/tmp/tz-work-F12/shapes` で確認）。F12 の後の期待を右に書く。

| 関数（本体の要点） | F12 後 |
| --- | --- |
| `for i in 0 .. values.length - 1 do total = total + values[i]`（`values: ref [i64]`） | R1・P1 で省略 |
| `for i in values.length - 1 .. -1 .. 0 do total = total + values[i]` | R2・P1 で省略 |
| `for i in 0 .. Array.length values - 1 do total = total + values[i]` | R1・P1 で省略（`.length` と同じ） |
| `let values = [10, 20, 30]` の後の `values[2]` | R4・P3 で省略 |
| `if index >= 0 && index < values.length then values[index] else 0` | R5・P1 で省略 |
| `bound index = { let values = [1, 2]; values[index] }`（`tests/trap_locations.rs`） | 残る（`index` に事実がない） |
| `let mut values = new [i64](count, \i -> i)` と、本体で `values = [1]` を代入するループ | 残る（`values` が安定でない） |
| `for i in 0 .. a.length - 1 do b[i]` | 残る（根が違う） |
| `for i in 0 .. values.length do values[i]` | 残る（上限が `len(v)`。最後の反復でトラップする） |
| `let mut i = 0` と `while i < values.length do (… values[i] …; i = i + 1)` | 残る（`while` は規則にない。Phase 2） |
| `values[3]`（`values` は長さ 3 のリテラル） | 残る（`3 < 3` でない。実行時にトラップする） |

### Phase 2（設計方針、実装しない）

- ループの版分け: アフィンな添字のループで全反復の範囲を入口で一度だけ検査し、成功なら検査なしの版、失敗なら元の検査付きの版を実行する。
  失敗時は元の版が走るので、トラップの位置と先行する副作用は変わらない。コードサイズの増加を計測してから決める。
- `while` の帰納変数、入れ子の配列 `m[i][j]` の外側、`Vec`（`push` で長さが変わる）、`array_slice`、`Array.set`・`update`・`swap`、
  文字列の添字、`assert` による支配、`min(len(a), len(b))` の上限。PR02 の範囲の事実を使えるものは PR02 の後に検討する。

## 設計

### データ構造

```rust
// src/ranges.rs（新規）
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const NODE_BUDGET: usize = 65_536;

#[derive(Clone, Debug, Default)]
pub(crate) struct RangeFacts {
    /// R1/R2: loop local id -> array local id; the local lies in `[0, len(array) - 1]`.
    below_length: BTreeMap<usize, usize>,
    /// R3: loop local id -> inclusive constant interval.
    constant: BTreeMap<usize, (u64, u64)>,
    /// R4: stable array local id -> statically known length.
    lengths: BTreeMap<usize, u64>,
    /// Locals that are never assigned or mutably borrowed in the function.
    stable: BTreeSet<usize>,
    /// R5: `(index local, array local)` pairs from enclosing `if` conditions; the emitter pushes and pops them.
    guards: Vec<(usize, usize)>,
}

pub(crate) fn analyze(function: &CheckedFunction) -> RangeFacts;

impl RangeFacts {
    pub(crate) fn index_in_bounds(&self, array: &TypedExpr, index: &TypedExpr) -> bool;
    /// Pushes R5 facts of `condition`; returns how many were pushed.
    pub(crate) fn enter_condition(&mut self, condition: &TypedExpr) -> usize;
    pub(crate) fn leave_condition(&mut self, pushed: usize);
}
```

`stable` は「安定でない局所変数」の補集合として作らず、関数の引数と、本体に出てくる `Local` の宣言（`Block` の束縛、`ForRange::local`、
`ForEach::local`、`Match::local`、パターンの束縛）のうち不安定の集合に入らないものを入れる。問い合わせは `stable.contains(&id)` だけで済む。

`FunctionEmitter` に field `ranges: crate::ranges::RangeFacts`（新規）を足す。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| 解析 | `src/ranges.rs`（新規） | `RangeFacts`, `analyze`, `index_in_bounds`, `enter_condition`, `leave_condition`, `NODE_BUDGET` | 証明規則 R1–R5 と P1–P3。単体テストを `#[cfg(test)] mod tests` に置く |
| crate | `src/lib.rs` | module の並び | `mod ranges;`（非公開。PR02 と `src/llvm.rs` から `crate::ranges` で使う） |
| 生成 | `src/llvm.rs` | `FunctionEmitter`, `FunctionEmitter::new` | field `ranges` を `RangeFacts::default()` で初期化（clone・drop・apply・entry の emitter は事実なしのまま） |
| 生成 | `src/llvm.rs` | `FunctionEmitter::emit` | `self.single_use = …` の直後に `self.ranges = crate::ranges::analyze(self.function);` |
| 生成 | `src/llvm.rs` | `emit_expression_mode` の `TypedExprKind::Index(array, index)` arm（文字列の arm ではない方） | `index` を文字列で覆い隠す前に `self.ranges.index_in_bounds(array, index)` を評価し、真なら `proven_element_pointer`（新規） |
| 生成 | `src/llvm.rs` | `place` の `TypedExprKind::Index(value, index)` arm | `shared_array_deref` の分岐と通常の分岐の両方で同じ判定 |
| 生成 | `src/llvm.rs` | `proven_element_pointer`（新規） | `Type::Array(element)` だけを受け、`extractvalue … , 0` と `element_pointer` を出す。長さは読まない |
| 生成 | `src/llvm.rs` | `emit_expression_mode` の `TypedExprKind::If` arm | `condition` を文字列で覆い隠す前に `enter_condition`、`then_branch` を出した直後に `leave_condition` |
| 生成 | `src/llvm.rs` | もう一つの `TypedExprKind::If {` の arm（`emit_expression_mode` より前） | then 側を出す箇所なら同じ push と pop。出さない（走査だけの）箇所なら変更なし |
| 生成 | `src/llvm.rs` | `checked_element_pointer`, `array_slice`, 文字列の添字, builtin の `emitter.guard` | 変更なし（検査を残す） |
| 生成 | `src/llvm_bulk.rs`, `src/llvm_io.rs`, `src/llvm_simd.rs`, `src/llvm_abi.rs` | `guard(…, TrapKind::BoundsCheck)` の各所 | 変更なし |
| 生成 | `src/llvm_control.rs` | `range_loop`, `narrow_range_loop`, `for_each` | 変更なし（`finish` を一度だけ評価する既存の性質に R1 が依存する。コメントを一行足す） |
| 計測 | `benchmarks/control/*`, `benchmarks/run-control.mjs` | workload `array_index_sum`（新規）、`inspection` | 実装手順 2 |

### 生成 IR とランタイム

ランタイム・header・WASM import は変えない。変わるのは証明済みの添字の IR だけ。

省略前（HEAD、「再現」の `sum`）は `extractvalue … , 1` → `icmp ult` → `br` → trap ブロック → `extractvalue … , 0` → GEP。
省略後（新しい形。番号は目安）:

```llvm
b1:
  %v5 = phi i64 [ 0, %loop ], [ %v6, %b2 ]
  store i64 %v5, ptr %v7
  %v9 = load i64, ptr %v1
  %v10 = load %tz.array, ptr %v0
  %v11 = load i64, ptr %v7
  %v12 = extractvalue %tz.array %v10, 0
  %v13 = getelementptr inbounds i64, ptr %v12, i64 %v11
  %v14 = load i64, ptr %v13
```

ループの入口（`icmp sle i64 0, %v4`）、latch（`icmp eq`）、`add`（フラグなし）は変わらない。

### アルゴリズム

```text
analyze(function):
    nodes = 0; unstable = {}; declared = parameters of function
    loops = []; literal = []                       # 候補。安定性は最後に確かめる
    work = [function.body]
    while e = work.pop():
        nodes += 1; if nodes > NODE_BUDGET: return RangeFacts::default()
        match e.kind:
            Assign(target, _) | Borrow(target, true)          -> unstable += place_root(target)
            BorrowOperand(target) if e.ty is Reference(_, true) -> unstable += place_root(target)
            Block { bindings, .. }  -> declared += locals; literal += (v, n) for R4 shapes
            ForRange { local, .. }  -> declared += local; loops += classify(R1, R2, R3)
            ForEach / Match / pattern bindings -> declared += locals
            Lambda { .. }           -> do not push its body          # 事実を作らない。不安定化は捕捉では起きない
        work.extend(e.children())
    stable = declared - unstable
    keep a loop fact only if its local and array are in stable; keep literal lengths only for stable arrays
```

`place_root(target)` は `Field`・`Index`・`Dereference` を外して `Local(id)` の `id` を返す（なければ何もしない）。
`classify` は「証明規則」の表の形だけを文字どおり照合する。定数の畳み込みや式の同値判定はしない（`values.length - 1 + 0` は外れる）。
`index_in_bounds` は P1–P3 を順に見る。`enter_condition` は `C` を `Binary(And, ..)` で平らにし（明示的なスタックで、再帰しない）、
下限の項と上限の項が同じ安定な `i` について揃った `(i, v)` を `guards` へ積む。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N が期待どおりかを必ず見る（GUIDE §3.1）。作業用の置き場は `/tmp/tz-f12/`、計測の生データは `target/perf/F12/`。

### 手順 1: ベースラインを取る

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行する。「再現」の 2 関数を `/tmp/tz-f12/loop/Main.tz` に置き、IR を保存する。
  比較用に HEAD の compiler を `/tmp/tz-f12/tsuzuri-before` へ複製する。
- 確認: 次がすべて成功する。stack-depth の 3 テストはそれぞれ `1 passed`。`run-control.mjs --quick` は 15 workload の JSON を出す。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
cp target/release/tsuzuri /tmp/tz-f12/tsuzuri-before
target/release/tsuzuri build /tmp/tz-f12/loop --emit llvm -o /tmp/tz-f12/before.ll
cargo test --locked --test trap_locations
cargo test --locked --test polymorphism bounds_type_growing_polymorphic_recursion
cargo test --locked --lib bounds_recursive_and_flat_expression_depth
cargo test --locked --test computations bounds_nested_builder_expansion_not_just_source_syntax
node benchmarks/run-control.mjs target/release/tsuzuri --quick > /tmp/tz-f12/control-quick.json
```

### 手順 2: workload `array_index_sum`（Phase 0）

- 変更: `benchmarks/control/Main.tz`、`benchmarks/control/host.c`、`benchmarks/control/reference.c`、`benchmarks/control/reference.rs`、
  `benchmarks/run-control.mjs`。
- 内容: `array_sum` の各所（`grep -n array_sum benchmarks/control/* benchmarks/run-control.mjs`）の直後に、同じ形で
  `array_index_sum` を足す。Tsuzuri 側は `/tmp/tz-work-F12/shapes` で検証した本体（`for i in 0 .. values.length - 1 do total = total + values[i]`）。
  C は添字のループ、Rust は `for i in 0..values.len() { total = total.wrapping_add(values[i]); }`（検査あり）。
  `host.c` は `DECLARE(array_index_sum)` と `WORKLOAD(array_index_sum, quick ? 1024 : 4000000)`。`run-control.mjs` は workload 名の
  `assert.deepEqual` の並びの `"array_sum"` の後と、`reference` の `collection` の配列に足す（期待値は `array_sum` と同じ式）。
- 確認: `node benchmarks/run-control.mjs target/release/tsuzuri --quick` が成功し、`workloads` が 16 個で `array_index_sum` を含む。

### 手順 3: fixture と E2E suite `bounds_checks`（Phase 0）

- 変更: `tests/fixtures/bounds_checks/Main.tz`（新規）、`tests/features.mjs`（suite `bounds_checks`、GUIDE §7.4）。
- 内容: 「テスト計画」の E2E の関数を置く。この時点では省略は入っていないので、全ケースが HEAD の compiler で成功することが、
  後の省略が意味を変えないことの基準になる。
- 確認: `node tests/features.mjs target/release/tsuzuri bounds_checks` が成功する（native・WASM × `-O0`・`-O3`、trap ケースを含む）。

### 手順 4: 形ごとの記録（Phase 0）

- 変更: なし（記録は手順 12 で `docs/benchmarks.md` に書く）。
- 内容: 「計測」の手順で、fixture の各関数について `-O3` 後に残る `llvm.trap` の数を `target/perf/F12/shapes-before.txt` に保存する。
- 確認: 9 関数すべての行がある。

### 手順 5: `src/ranges.rs` の骨格

- 変更: `src/ranges.rs`（新規）、`src/lib.rs`、`src/llvm.rs` の `FunctionEmitter`・`FunctionEmitter::new`・`FunctionEmitter::emit`。
- 内容: 「データ構造」の型と関数を置く。`analyze` は `RangeFacts::default()` を返し、`index_in_bounds` は常に `false`。
  `FunctionEmitter::emit` で `single_use` の直後に `analyze` を呼ぶ。
- 確認: `cargo test --locked` が成功する。`target/release/tsuzuri build /tmp/tz-f12/loop --emit llvm -o /tmp/tz-f12/after.ll` と
  `cmp /tmp/tz-f12/before.ll /tmp/tz-f12/after.ll` が一致する（release build 後）。

### 手順 6: 安定性と R1–R4

- 変更: `src/ranges.rs`。
- 内容: 「アルゴリズム」の worklist を書く。`analyze` は本体を `analyze_within(function, NODE_BUDGET)`（新規、非公開）へ渡す。
  `index_in_bounds` の P1–P3 を書く（R5 はまだない）。emitter はまだ結果を使わない。
- 確認: `cargo test --locked --lib ranges` が `running 5 tests` で成功する（「Rust テスト」の単体テストのうち R5 と予算以外の 5 個）。

### 手順 7: emitter で検査を省く

- 変更: `src/llvm.rs` の `proven_element_pointer`（新規）、`emit_expression_mode` と `place` の `TypedExprKind::Index` arm。
- 内容: 「段ごとの変更」のとおり。`proven_element_pointer` は `Type::Array` 以外で `unreachable!("range facts only prove arrays")`。
- 確認: `cargo test --locked` が成功する。`/tmp/tz-f12/after.ll` の `@tz.fn.Main.sum` から `icmp ult` がなくなり、`@tz.fn.Main.pick` の
  `values[2]` の検査もなくなる。`node tests/features.mjs target/release/tsuzuri bounds_checks` が成功する（release build 後）。

### 手順 8: R5（支配する `if`）

- 変更: `src/ranges.rs` の `enter_condition`・`leave_condition`、`src/llvm.rs` の `TypedExprKind::If` arm。
- 内容: 「段ごとの変更」のとおり。else 側では事実を積まない。
- 確認: `cargo test --locked --lib ranges` が `running 6 tests`。`@tz.fn.Main.pick` の `icmp ult` が 0 個になる。

### 手順 9: 予算

- 変更: `src/ranges.rs` の単体テスト。
- 内容: `analyze_within(function, 8)` が事実なしを返すことを確かめる。
- 確認: `cargo test --locked --lib ranges` が `running 7 tests`。

### 手順 10: 統合テスト

- 変更: `tests/bounds_checks.rs`（新規）。
- 内容: 「Rust テスト」の 4 テスト。
- 確認: `cargo test --locked --test bounds_checks` が `4 passed`。

### 手順 11: 全体の確認

- 変更: なし。
- 確認: `cargo test --locked`、手順 1 の stack-depth 3 テスト、`cargo test --locked --test trap_locations`（7 passed）、
  `cargo build --release --locked && node tests/features.mjs target/release/tsuzuri`、
  `npx --yes --package=node@24 node tests/primitives.mjs target/release/tsuzuri` がすべて成功する。

### 手順 12: 計測と文書

- 変更: `docs/benchmarks.md`、`docs/architecture.md`、`_docs/guides/performance.md`、`_docs/feature-status.md`、`_features/README.md`。
- 内容: 「計測」を実行し、「ドキュメント」のとおり書く。
- 確認: `node scripts/check-docs.mjs _docs/guides/performance.md _docs/feature-status.md` が成功する。`git diff --check` が空。

## テスト計画

### Rust テスト

`src/ranges.rs` の `#[cfg(test)] mod tests`。`crate::analyze(source)` で検査し、対象関数の本体から `TypedExprKind::Index` を
`TypedExpr::children` で集め、各添字に `index_in_bounds` を問う。R5 は `If` の then 側で `enter_condition` してから問う。

| テスト | 確かめること |
| --- | --- |
| `forward_and_backward_length_loops_prove_their_indices` | R1・R2・`Array.length` の形が P1 で真 |
| `constant_loops_and_literal_lengths_prove_constant_indices` | R3・R4 で P2・P3 が真。`values[3]`（長さ 3）は偽 |
| `assignments_and_mutable_borrows_make_arrays_unstable` | 本体で `values = [1]`、`ref mut values` があると偽 |
| `unmatched_shapes_keep_their_checks` | `0 .. values.length`、`0 .. a.length - 1` の `b[i]`、`while`、`values.length - 1 + 0`、`Vec`、リストがすべて偽 |
| `lambda_bodies_get_no_facts` | ループ内の閉包の本体の `values[i]` が偽 |
| `dominating_conjunction_proves_the_then_branch_only` | R5 で then 側が真、else 側と `if` の後が偽。`i >= 0` がない条件は偽 |
| `node_budget_falls_back_to_no_facts` | `analyze_within(function, 8)` の結果で R1 の形が偽 |

`tests/bounds_checks.rs`（新規）。`tsuzuri::llvm::emit_with_trap_info`（`Entry::Library`、native と WASM）の `trap_sites` を数える。

| テスト | 確かめること |
| --- | --- |
| `proven_indices_emit_no_bounds_guard` | 「例」の省略する 5 形で `TrapKind::BoundsCheck` の site が 0。IR を 2 回出して一致 |
| `unproven_indices_keep_bounds_guards_and_sites` | 「例」の残る 6 形で site 数と `TrapSite::span` が手順 1 の HEAD の値と一致 |
| `other_trap_kinds_are_unchanged` | 省略する形の `AllocationSize`・`Assert` の site 数が HEAD と一致 |
| `range_proofs_add_no_llvm_facts` | 省略する形の IR に `llvm.assume`・`!range`・` nsw `・` nuw ` がない |

### E2E

suite `bounds_checks`（`tests/features.mjs`、fixture `tests/fixtures/bounds_checks/Main.tz`）。関数はすべて `i64` を受けて配列を中で作る
（`new [i64](count, \i -> i * 3 + 1)` など、`/tmp/tz-work-F12/negative` で検証した形）。期待値は JS の BigInt で計算する:
`affine(n) = 3n(n - 1)/2 + n`（`3i + 1` の `i < n` の和）。

| 関数 | 形 | cases（引数 → 期待） | traps |
| --- | --- | --- | --- |
| `forward` | R1 | `0, 1, 5, 1000` → `affine(n)` | なし |
| `backward` | R2（`.. -1 ..`） | 同上 | なし |
| `builtin_length` | R1、`ref [i64]` を受ける補助関数で `Array.length values` | 同上 | なし |
| `pick` | R4・R5（「再現」の `pick`） | `-1, 0, 2, 3, min, max` → `30 + ([10, 20, 30][index] または 0)` | なし |
| `replaced` | 安定でない | `0 → 0`、`1 → 0` | `[2]`（2 回目の反復で長さ 1 の配列を添字 1 で読む） |
| `other` | 根が違う | なし | `[1]`（`b` が空） |
| `past_end` | 上限が `len(v)` | なし | `[0]`、`[3]` |
| `while_sum` | `while` | `0, 5` → `affine(n)` | なし |
| `literal_past` | 定数添字が範囲外 | なし | `[0]` |

`inspect(ir)` で `assert.doesNotMatch(ir, /llvm\.assume|!range| nsw | nuw /)`。native は `live == 0`、WASM は import なし（harness の既定）。

### 既存テストへの影響

なし。`tests/trap_locations.rs` の `bound` は添字が引数なので検査が残る。既存 fixture のうち証明できる形を含むものは trap 表の件数が減るが、
件数や IR の `icmp ult` を固定している既存テストはない（`grep -rn "icmp ult" tests/` で確認する。あれば停止条件）。

### 性能

時間の合否閾値は置かない（AGENTS.md）。計測は「計測」の手順で記録だけする。

### 計測

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
mkdir -p target/perf/F12
node benchmarks/run-control.mjs target/release/tsuzuri --baseline /tmp/tz-f12/tsuzuri-before \
  --artifacts target/perf/F12/control > target/perf/F12/control.json
target/release/tsuzuri build tests/fixtures/bounds_checks --emit llvm -o target/perf/F12/bounds.ll
clang -O3 -Wno-override-module -S -emit-llvm target/perf/F12/bounds.ll -o target/perf/F12/bounds.O3.ll
for f in forward backward builtin_length pick replaced other past_end while_sum literal_past; do
  printf "%s %s\n" "$f" "$(awk "/^define .*@tz_$f\\(/,/^}/" target/perf/F12/bounds.O3.ll | grep -c 'llvm.trap')"
done > target/perf/F12/shapes-after.txt
```

`run-control.mjs` は 1 回の実行で variant ごとに 3 標本の中央値を出す。3 回実行して `array_index_sum` と既存 workload の
`tsuzuri_median_ms`・`speedup` を並べる。環境は JSON の `environment` をそのまま転記する。`-O3` では単純なループの検査を LLVM が既に消すので、
時間の改善を主張するのは計測で差が広がりを超えた場合だけ。`-O0` の IR から検査が消えたことは「実装済み」として別に書く。

## ドキュメント

- `docs/architecture.md`: モジュール表に `src/ranges.rs` の行。`## 不変条件` に「境界検査の省略は `src/ranges.rs` の証明規則 R1–R5 に限る。
  省略しても GEP と load は残し、`llvm.assume`・`!range`・overflow フラグは出さない」を足す。
- `docs/benchmarks.md`: `## 制御構文の比較` に `array_index_sum` の行と `### 境界検査の残り方（F12）`（shapes-before と after の表、計測日、環境）。
- `_docs/guides/performance.md`: `## データと所有権` に、検査が残らない書き方（`for value in values`、`for i in 0 .. values.length - 1`、
  範囲の `if` の中の添字）と残る書き方（`while`、別の配列の長さ、ループ内での置換）を、「例」の検証済みの形で書く。
- `docs/language.md`: 変更なし（意味は変わらない）。
- `_docs/feature-status.md` の F12 行と `_features/README.md` の F12 の状態。

## 受け入れ条件

- [ ] 「例」の省略する 5 形で、`-O0` の IR と trap 表から境界検査が消える（native と WASM）。
- [ ] 残る 6 形で、検査・`TrapKind::BoundsCheck`・`TrapSite::span` が HEAD と同じ。
- [ ] suite `bounds_checks` が native・WASM × `-O0`・`-O3` で成功し、trap ケースがすべてトラップする。`live == 0`、WASM の import なし。
- [ ] `llvm.assume`・`!range`・`nsw`・`nuw` を新しく出していない。IR は決定的。
- [ ] `array_index_sum` が `run-control.mjs` にあり、before と after の計測と形ごとの記録が `docs/benchmarks.md` にある。
- [ ] stack-depth の 3 テストと `cargo test --locked` が成功する。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- `emit_expression_mode` の Index arm は `let index = self.expression(index);` で型付きの `index` を文字列で覆い隠す。
  証明の問い合わせはその前に行う。If arm の `condition` も同じ。
- `place` の Index arm は 2 経路（`shared_array_deref` とそれ以外）ある。片方だけ直すと、同じ添字が書き方で別の経路になる。
- `Vec` とリストも `checked_element_pointer` を通る。`Vec` は `push` で長さが変わるので、`proven_element_pointer` を `Type::Array` 以外で呼ぶと
  範囲外の読み出しになる。`unreachable!` で止める。
- 符号付き `-1` の表現は `Int(u128::MAX >> 64)`（`range_loop` の `minus_one` と同じ）。`u128::MAX` と比べると R2 が永遠に一致しない。
- 末尾再帰は `FunctionEmitter::emit` の `loop` ブロックへ戻り、引数を `phi` で差し替える（`Assign` の節点はない）。差し替えは末尾位置だけで起き、
  そのとき R1・R5 の範囲をすべて抜けているので事実は壊れない。R4 は `Block` の束縛だけを見るので引数には付かない。
  `puts_array_storage_before_the_tail_loop` が通ることで確かめる。
- clone・drop・apply・entry の `FunctionEmitter::new` は `emit` を通らない。解析を `new` で計算しない（別の関数を解析してしまう）。
- 64 bit 未満の反復変数（`for i = 0 to 2`）は `narrow_range_loop` を通り、添字には `i as i64` が要るので `I` が `Local` にならず検査が残る。不具合ではない。
- `-O3` では単純なループの検査を LLVM が既に消している。時間が変わらなくても失敗ではない。差がない結果を改善と書かない。

## 対象外

- 利用者が検査を無効にするオプションや unchecked API（安全性を下げる選択肢は提供しない）。
- `Vec`・リスト・文字列の添字、`array_slice`、`Array.set`・`update`・`swap`、SIMD の lane、公開 ABI の入口の検査。
- ループの版分け、`while` の帰納変数、入れ子配列の外側、`assert` による支配（Phase 2）。
- 算術への `nsw`／`nuw`（PR02）。固定長配列の型からの長さ（A16）。

## 決定事項

### D1: 省略の方式

- 決定: 証明できた添字では emitter が `guard` を出さない。LLVM へ事実（`llvm.assume`、`!range`）を渡して LLVM に消させる方式は採らない。
- 理由: 生成 IR が決定的なまま `-O0` でも効き、native と WASM で同じになる。長さは SSA の集成体からの `extractvalue` で、`!range` は
  load と call にしか付かない。`llvm.assume` は `-O0` で命令が増え、最適化を妨げることがある。
- 状態: 既定案（実装者はこの案に従う）

### D2: LLVM への事実を出さない

- 決定: F12 は `llvm.assume`・`!range`・`nsw`・`nuw` を新しく出さない。既存の `narrow_range_loop` の `llvm.assume` はそのまま。
- 理由: overflow フラグは PR02 の担当で、F12 の証明を根拠に PR02 が判断する。事実の効果は計測してから入れる。
- 状態: 既定案（実装者はこの案に従う）

### D3: モジュールと名前

- 決定: `src/ranges.rs` を `src/lib.rs` の `mod ranges;`（非公開）で置く。名前は `RangeFacts`・`analyze`・`index_in_bounds`・
  `enter_condition`・`leave_condition`・`NODE_BUDGET`・`analyze_within`。
- 理由: `call_specialization` は `src/llvm.rs` の中の path module で、PR02 が使う `src/control.rs` からは見えない。公開 API にはしない。
- 状態: 既定案（実装者はこの案に従う）

### D4: 安定性は関数全体で判定する

- 決定: 根の局所変数は、関数本体のどこにも代入・可変借用がないときだけ安定とする。ループの内側だけを見る判定はしない。
- 理由: 一回の走査で済み、事実の有効範囲を考えなくてよい。ループの後で置換する `let mut` の配列は証明を失うが、検査が残るだけで安全。
- 状態: 既定案（実装者はこの案に従う）

### D5: 解析の予算

- 決定: 関数あたり 65,536 節点。超えたら事実なし。走査は worklist で再帰しない。
- 理由: 予算切れの結果は「検査を残す」なので安全。再帰しなければ stack-depth テストに影響しない。
- 状態: 既定案（実装者はこの案に従う）

### D6: 診断を出さない

- 決定: 検査が残ったこと・省略したことを知らせる診断や警告は作らない。確認は IR と trap 表で行う。
- 理由: 意味は変わらず、利用者が対処すべき誤りではない。
- 状態: 既定案（実装者はこの案に従う）

### D7: 規則は閉じた表にする

- 決定: 証明は R1–R5 と P1–P3 の形の文字どおりの照合だけ。定数の畳み込み、式の同値判定、別名の追跡はしない。規則の追加は停止条件。
- 理由: 誤った証明は未定義動作になる。規則を小さく保てば、テストで全形を網羅できる。
- 状態: 既定案（実装者はこの案に従う）

### D8: 計測する target

- 決定: arm64 macOS の計測を必須にする。x86_64 Linux は実機があれば同じ手順で追記し、なければ「未計測」と書く。
- 理由: 旧未決事項の既定案を保ちつつ、実機がないことで完了が止まらないようにする。x86 の数値は推定しない。
- 状態: 既定案（実装者はこの案に従う）

### D9: trap 表

- 決定: 省略した位置は trap 表に載せない。表の形式、残った位置の `TrapKind` と `TrapSite::span` は変えない。
- 理由: 省略した位置はトラップしないことが証明済みで、報告されることがない。
- 状態: 既定案（実装者はこの案に従う）

## 実装と検証（2026-10-02、Phase 0・1）

Phase 0・1（手順 1–12）を実装した。Phase 2（ループの版分けと `llvm.assume`、「Phase 2（設計方針、実装しない）」の項）は、実測で不要と判断して実装していない（末尾の「Phase 2 の判断」）。承認は不要。
着手時の HEAD は `30b2d1d`。コミットは作っていない。このチケットは、人間が E12 と書くつもりで F12 と指示した取り違えのために先に実装した。
2026-10-02 に人間が「完了扱い」にすると決めたので、記録と `docs/benchmarks.md` を仕上げた。

### 実装

- `src/ranges.rs`（新規）: `RangeFacts`、`analyze(module, function)`、`index_in_bounds`、`enter_condition`・`leave_condition`、`NODE_BUDGET`（65,536）。
  規則 R1–R5・P1–P3、単体テスト 7 件。
- `src/lib.rs`: `mod ranges;`。
- `src/llvm.rs`: `FunctionEmitter` の field `ranges`・`proven_reads`、`emit` の `analyze`、`indexed_pointer`（証明済みなら検査のない GEP。`Index` の 2 つの arm が共有）、
  `If` の arm の `enter_condition`・`leave_condition`、`hint_loop(body, reads)`。
- `src/llvm_frame.rs`: `If` の arm（チケットが挙げていない 3 か所目）。
- `src/llvm_control.rs`: 各ループの emit で `proven_reads` を控えて `hint_loop` へ渡す。R1・R2 が依存する評価順序のコメント。
- `benchmarks/control/*`・`benchmarks/run-control.mjs`: workload `array_index_sum`（16 個目）。
- テスト: `tests/bounds_checks.rs`（新規、5 件）、`tests/fixtures/bounds_checks/Main.tz`（新規）、`tests/features.mjs` の suite `bounds_checks`（28 case）。
- 文書: `docs/benchmarks.md`（`array_index_sum` の行、「境界検査の残り方（F12）」「境界検査の省略の実測（2026-10-02）」）、`docs/architecture.md`、
  `_docs/guides/performance.md`、`_docs/feature-status.md`、`_features/README.md`、`_perfs/README.md`・`_perfs/PR02-integer-ranges.md`（F12 の完了への追従）。

### 決定事項への追記（チケットから外れた判断）

1. `analyze(module, function)`・`enter_condition(module, condition)`: `Array.length` の呼び出しを型付き IR で見分けるため、チケットの `analyze(function)` に `module` を足した。
2. `If` の emit にあたる箇所は、チケットの 2 か所に加えて `src/llvm_frame.rs` にもあり、同じ push と pop を入れた。
3. 同じ id が複数の scope で宣言されている local は不安定として事実を持たせない（保守側）。
4. fixture に `walk`・`tail_walk`（tail 位置の `If` の R5）を足し、E2E の suite は 22 から 28 case に、`tests/bounds_checks.rs` の省略する形は 5 から 7 形になった（`TAIL_GUARD` の形を含む）。
5. 新しい決定（`llvm.loop.unroll.enable` の扱い）: 最初の実装は `array_index_sum` を `-O3` で約 32% 遅くした。検査が消えたループに `hint_loop` が付ける展開ヒントが原因で、
   LLVM が `LoopVectorize` より前に実行時展開を行い、ベクトル化が弱い形になった。検査を省いた読み出しを本体に持つループにはヒントを付けない（`hint_loop(body, reads)`）ことで、
   変更前と同じ生成コードに戻した。`tests/bounds_checks.rs` の `unroll_hint_is_dropped_only_where_a_guard_was_removed` が境界を固定する。「D2: LLVM への事実を出さない」の範囲内の変更だが、
   ヒントの出し方を変えるので決定事項として追記する。
6. 計測は Node.js v20.17.0 で行い、BigInt の重い suite と wasm64 は Node 24.21.0 で実行した。計測した target は arm64 macOS だけで、x86_64 Linux は未計測（D8）。

### 確認（Apple M1 Max、macOS、Apple clang 21.0.0、Homebrew LLD 23.1.1、rustc 1.98.1）

- 手順 1: `cargo test --locked --test trap_locations` が 7 passed、§3.1 の stack-depth 3 テストが成功、`run-control.mjs --quick` が 15 workload を出した。
  HEAD のコンパイラを `/tmp/tz-f12/tsuzuri-before` と `target/perf/F12/tsuzuri-before`（SHA-256 `722dab3c…`）に保存した。
- 手順 2・3: `run-control.mjs --quick` が 16 workload（`array_index_sum` を含む）。fixture と suite `bounds_checks` は省略を入れる前の HEAD のコンパイラで全 case が成功した（22 case）。
- 手順 4: 形ごとの記録を `target/perf/F12/shapes-{before,after}.txt`・`shapes-{before,after}.O0.txt` に保存した（結果は `docs/benchmarks.md` の表）。
  `-O0` の `llvm.trap` は `forward` 2→1、`backward` 2→1、`sum_all` 1→0、`pick` 2→0、ほかの形は変更前と同じ。`-O3` は 9 関数すべてで変更前後が一致。
- 手順 5–9: `cargo test --locked --lib ranges` が 7 passed（R1–R5、不安定化、閉包、予算）。省略した形の `@tz.fn.Main.sum`・`pick` から `icmp ult` が消えた。
- 手順 10: `cargo test --locked --test bounds_checks` が 5 passed（チケットの 4 件と `unroll_hint_is_dropped_only_where_a_guard_was_removed`）。
- 手順 11: `cargo test --locked` は 548 passed・0 failed（E14・E12 を含む作業ツリー全体）。`cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings` が成功。
  `cargo test --locked --test trap_locations` は 8 passed（E14 の 1 件を含む）。E2E は 29 項目の gate script がすべて成功した（29 PASS・0 FAIL）。
  suite `bounds_checks`（28 case）は native・WASM × `-O0`・`-O3` で features（4958 case）の中で成功し、Node 24.21.0 の
  `TSUZURI_TEST_WASM_TARGET=wasm64` でも 28 case 成功した。primitives（Node 24）、control（270）、tasks（41 結果・4 trap）、computations（70）、simd（232）、
  e2e、strings、numeric_casts、integer_intrinsics、display_parse、examples、wasm_memory、wasm_threads、io、cache、debug_info、docgen、lsp_sessions も成功した。
- 手順 12: `node scripts/check-docs.mjs` が全体で成功した（83 pages・746 links・141 checked examples・246 native runs（O0/O3）・9 test projects）。`git diff --check` が空。
- 差分の検査: HEAD のコンパイラと新しいコンパイラで、省略する形・残す形ごとの trap site の数と `TrapSite::span` を比べる差分スクリプトを書いて確認した。さらにランダムな添字プログラムを
  両方で WASM `-O0`・`-O3` に build して結果とトラップを比べる差分テストを書いた（`/tmp/tz-f12/fuzz.mjs`）。最終のコンパイラで seed 201・202・203 の各 12 module
  （合計 16,632 比較、トラップ 1,946・1,631・1,567 件を含む）を実行し、不一致は 0 件だった。
- 計測（`docs/benchmarks.md` 「境界検査の省略の実測（2026-10-02）」）: `array_index_sum` は変更前 1.788 ms・変更後 1.786 ms（改善倍率 0.998、3 回の範囲 0.995–1.001）、
  C 1.784 ms・Rust 1.782 ms。全 16 種目の改善倍率は 0.993–1.020。展開ヒントを抑えない最初の実装は変更後 2.371 ms で改善倍率 0.761（約 32% 遅い）だった。
  生データ・IR・アセンブリは `target/perf/F12/control-{1,2,3}.json`・`control-prefix-{1,2,3}.json` と同名ディレクトリ。
- **速度の改善は主張しない。** `-O3` では LLVM が変更前から単純なループの検査を消すので、実行時間は変わらなかった。効果は `-O0` の IR と trap 表から検査が消えること。
  CI に速度の合否の閾値は追加していない。

### 残作業

- Phase 2 は実装しない判断をした（下の「Phase 2 の判断（2026-10-02）」）。PR02（整数の範囲と overflow フラグ）が `src/ranges.rs` の上に算術の事実を足す。
- x86_64 Linux の計測は未実施。
- 検査が残る形（`replaced`・`other`・`past_end`・`while_sum`・`literal_past` など）は R1–R5 の外なので、`-O0` では従来どおり検査が残る。

## Phase 2 の判断（2026-10-02）

Phase 2 の設計方針（ループの版分け、`while` の帰納変数、`assert` による支配、`min(len(a), len(b))` の上限、入れ子の配列）は、「コードサイズの増加を計測してから決める」とあった。
着手前の問いは「`-O3` で、いまの証明規則の外の形が証明のある形より遅いか」である。`benchmarks/bounds_shapes/`（新規。`Main.tz` と C の `bench.c`）で 4 つの形を測った。

| 形 | `-O3`（ms、200 回） | `sum_all` に対する比 | `-O0`（ms、200 回） | `sum_all` に対する比 |
| --- | --- | --- | --- | --- |
| `sum_all`（R1 で証明あり） | 21.601 | 1.000 | 378.243 | 1.000 |
| `sum_first`（可変の上限 `n - 1`） | 21.831 | 1.011 | 402.614 | 1.064 |
| `sum_checked`（`if n <= values.length` の後の `0 .. n - 1`） | 22.197 | 1.028 | 401.955 | 1.063 |
| `sum_while`（`while i < values.length`） | 21.767 | 1.008 | 486.269 | 1.286 |

- 4 つとも `-O3` でベクトル化される（`clang -O3 -S -emit-llvm` の出力に `<N x i64>` が現れる）。LLVM の `IndVarSimplify` が、ループを抜ける検査（行き先がトラップ）を
  ループの前の一回の比較へ移すためで、証明のない形も 3% 以内に収まる。版分けは同じ結果を、ループの本体を二つにして得るだけである。
- 決定: ループの版分け、`while` の帰納変数、`assert` による支配を**実装しない**。`-O3`（既定）の速度を上げず、コードサイズだけを増やす。`-O0` では `while` が約 29% 遅いが、
  それを縮めるには増分が本体の最後の文であることまで確かめる流れ依存の証明が要り、デバッグ用の `-O0` だけの利益に見合わない。
- `llvm.assume` と `!range` は、D2・D7 のとおり出していない。
- 測定は Apple M1 Max、`target/release/tsuzuri`（2026-10-02 のビルド）、Apple clang 21、1,048,576 個の `i64`、7 回の最小値。x86_64 Linux は未計測で、推定値は書かない。
  手順は `docs/benchmarks.md` の「境界検査の Phase 2 の形（F12）」にある。
