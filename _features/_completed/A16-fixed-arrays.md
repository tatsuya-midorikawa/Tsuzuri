# A16: 固定長配列と const ジェネリクス

| 項目 | 内容 |
| --- | --- |
| ID | A16 |
| 優先度 | P2 |
| 規模 | XL |
| 依存 | A01, D06, (F04) |
| 後続 | C11, D11, E12 |
| 状態 | done（Phase 1・2） |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | D1・D10 は、2026-10-06 に利用者から「C08、A14、A16、F13、F08 の実装をすべて完遂して。…複数フェーズある場合には、すべてのフェーズを完了させること」と依頼され、承認として扱った（GUIDE D-03・D-07・D-13・D-15・D-39） |
| 改善する劣位 | Rust 比: Copy のコスト（[なぜ Tsuzuri か](../../_docs/learn/why-tsuzuri.md#rust-に対する劣位点)）／追加: ヒープなしの小さな配列・行列を表せない |
| 手本にする既存実装 | 値の集成体: `Type::Tuple`（`src/llvm.rs` の `llvm_type` の `{ ... }`・`storage_layout` の `aggregate`、`src/check.rs` の `Layouts::size`・`Validation::check`、`src/llvm_frame.rs` の `stack_size`）。添字と境界検査: `src/llvm.rs` の `TypedExprKind::Index` の arm・`emit_place`・`checked_element_pointer`・`guard(.., TrapKind::BoundsCheck)`。期待型からのリテラル: `src/check.rs` の `ExprKind::Array` の arm。期待型で決まる builtin: `BuiltinType::SimdLane`（`src/check.rs`）と `FamilyKind::SimdLane`（`src/polymorph.rs`）。引数の暗黙の借用: `Checker::coerce_argument` |
| 主な影響ファイル | `src/syntax.rs`, `src/parser.rs`, `src/formatter.rs`, `src/docgen.rs`, `src/semantic.rs`, `src/check.rs`, `src/polymorph.rs`, `src/ownership.rs`, `src/call_specialization.rs`, `src/constants.rs`, `src/llvm.rs`, `src/llvm_frame.rs`, `src/llvm_debug.rs`, `src/abi.rs`（変更なし・確認のみ）, `tests/fixed_arrays.rs`（新規）, `tests/arrays.rs`（期待値の変更）, `src/check.rs` の `tests` モジュール（期待値の変更）, `tests/fixtures/fixed_arrays/Main.tz`（新規）, `tests/features.mjs`, `docs/language.md`, `docs/architecture.md`, `_docs/language-reference/types.md`, `_docs/library-reference/arrays-and-lists.md`, `_docs/feature-status.md`, `_features/README.md` |

## 目的

長さを型に含む値型の配列（C/C++ の `T[N]`・`std::array`、Rust の `[T; N]`、C# の inline array）を追加する。
座標・小行列・lookup table・SIMD 前後の作業領域を、ヒープ確保なしで扱い、Copy 要素なら確保なしの値コピーで複製できるようにする。

実装者は Phase 1 だけを実装する。Phase 2（const ジェネリクス）は設計方針だけで、人間が求め、D10 が承認された場合だけ着手する。

### Phase 1 を新しい型構文なしで出せるか（結論: 出せない）

次の理由で、長さを型に持つ固定長配列には新しい型構文が必ず要る。したがって Phase 1 全体が D1 の承認待ちになる。

- 型パラメーターは型だけ（`'a`）。D-02 により型引数 `<...>` の各要素は完全な `type_expr` で、`FixedArray<f64, 3>` の `3` は型ではない。
  整数を型引数に許すのは D-02 の変更で、`[T; N]` と同じく新しい型構文になる。
- `const`（D06）は型検査の後で畳み込まれる（`src/check.rs` の検査の最後で `constants::fold` を呼ぶ）。型の解決時に `const` の値は存在しない。
- タプル `(f64, f64, f64)` は inline の値だが、`i64` の添字で読めず、長さを抽象化できない。`[T]` は長さが型にない。
- よって既存の仕組みだけで作れるのは「`[T]` を包む std 型」だけで、目的（ヒープなし・長さが型の一部）を満たさない。

## 着手条件と停止条件

### 着手条件

- D1 が承認済みであること。承認前はどの手順にも着手しない（GUIDE D-30 が A16 を承認待ちに挙げている）。
- 依存の A01・D06 が `_features/README.md` の状態欄で done であること。F04 は括弧付き（任意）で開始条件にしない。
  確認: `grep -nE "^\| (A01|D06|F04) " _features/README.md`。
- GUIDE §2.3 の基準コマンドが成功し、実装手順 1 のベースライン（IR と stack-depth テスト）を保存していること。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- `Type::FixedArray`（新規）を足すと `Type` が 4 語を超える（`std::mem::size_of::<Type>()` が 32 を超える）。
- 既存の `[T]`・タプル・レコードを使うだけのプログラムの IR が 1 byte でも変わる（手順 1 の保存 IR と比較する）。
- 期待値の変更が「既存テストへの影響」に挙げた 4 件以外に必要になる。
- `FixedArray.init` の型付けに `FamilyKind` の仕組みを使えず、`Inference` の単一化規則そのものを変える必要がある。
- 固定長配列の値を関数へ渡す・返すために、既存の呼び出し規約（SSA 値渡し）以外の経路（`sret`・`byval`・ポインター渡し）が必要になった。
- `[f64; 1024]` を値渡しする関数 1 個の `clang -O3` が native で 10 秒を超える（手順 9 の確認。現状の計測は「現状」を見よ）。
- stack-depth の 3 テスト（GUIDE §11.1 の一覧）か `honors_the_exact_specialization_limit` が失敗する。または上限・stack サイズを上げたくなった。
- `unsafe`、新しい crate、既定の WASM import が必要になった。

## 現状（HEAD `f8dc655` で確認）

- 配列型は `[T]` だけで、長さは型の一部ではない。`src/parser.rs` の型の一次式（`TypeExprKind::Array` を作る分岐）は、要素型の後に `;` が
  来ると `"array types use '[T]', without a length; use 'new [T](length, initializer)' to create an array"`（E0002）で拒否する。
  この拒否は `tests/arrays.rs` の `array_constructors_require_a_length_and_typed_initializer` の 3 件と、`src/check.rs` の
  `tests::rejects_invalid_programs_with_stable_codes` の 1 件（`fn f(a: [i64; 4]) -> i64 { 0 }` → E0002）が固定している。
- `[T]` は `%tz.array`（`{ ptr, i64 }`）の記述子。束縛した配列リテラルの要素は `src/llvm_frame.rs` の frame（entry block の alloca）、
  それ以外はヒープに置く。`ref [T]`（共有の配列借用）も 16 byte の記述子で、`Type::shared_array_element` が判定する。
- 値の集成体（レコード・タプル）は LLVM の first-class aggregate の SSA 値で、`insertvalue`／`extractvalue` と entry block の slot で扱う。
  タプルの LLVM 型は `{ T, U }`、`storage_layout` は各要素の実寸と align から計算する。
- 値の大きさの上限は `src/check.rs` の `MAX_VALUE_BYTES`（64 KiB）。保守的な見積もり（`Layouts::size`・`Validation::check`・
  `llvm_frame::stack_size`）はレコード・タプルの各要素を 16 byte 単位に切り上げる。超過は `size_error` の E1010
  `"value layout exceeds 65536 bytes; use smaller value types"`。4096 個の `i64` フィールドのレコードは受理、4097 個は E1010
  （`tests::accepts_exact_value_limits_and_rejects_the_next_scalar`）。
- 添字 `values[i]` は `Checker` の `ExprKind::Index` の arm が型付けし（`[T]`・`[|T|]`・`Vec<T>`・文字列）、それ以外は E1005
  `"indexing requires an array, list, string, or utf8string"`。LLVM では `checked_element_pointer` が `icmp ult` と
  `guard(.., TrapKind::BoundsCheck)` を出す。配列要素の読み出しは `clone_value` で複製を返す（配列は深く不変）。
- 呼び出し引数の暗黙の借用は `Checker::coerce_argument` が補う（`docs/language.md` の「呼び出し引数の暗黙の借用」の表）。
- 公開 ABI は `src/abi.rs` の `parameter`・`result` が判定し、外れると E1008（`exports support scalar values, ...`）。
- SIMD 型（`src/simd.rs`）は 128-bit 固定の値型で、任意長の固定配列の代わりにはならない。`Simd.load` は `ref [lane]` と `i64` を取る。
- 型パラメーターは型だけ（`'a`）。D06 の `const` は具体型の値だけで、型検査の後に `constants::fold` で展開される。

### 再現（検証済み）

`/tmp/tz-work-A16/reject/Main.tz` に次の 2 行を置いて `target/release/tsuzuri check /tmp/tz-work-A16/reject` を実行すると、
`1:14` に E0002 と上記メッセージが出る。

```tsuzuri
def f :: [i64; 4]
fn f = [1, 2, 3, 4]
```

### 大きな集成体のコンパイル時間（2026-09-29 に計測）

`[8192 x i64]` を SSA 値で返し、値渡しで受けて動的添字で読む 3 関数の手書き IR を `/opt/homebrew/opt/llvm@21/bin/clang -c` で翻訳した
（arm64 macOS、各 1 回、`date +%s` 単位）。`-O0` は native・wasm32 とも約 1 秒、`-O3` は native 13 秒・wasm32 6 秒だった。
値渡しの大きな first-class aggregate は LLVM の翻訳時間を大きく増やす。D3 の要素数上限 1,024 はこの計測に基づく
（1,024 要素は未計測。手順 9 で計測する）。

## 仕様

この節の構文と API はすべて実装後に有効で、HEAD では未検証である（HEAD では「現状」の再現のとおり E0002 になる）。
サンプルの `&[T]` は `ref [T]` と同じ型である（D-15 の別表記）。

### 前提とする他チケットのインターフェース

- A01（done）: `Type::Record(usize, Box<[Type]>)` と型引数の置換。`Maybe<[f64; 3]>` やジェネリックなレコードのフィールドに固定長配列を入れられるのはこの仕組みによる。
- D06（done）: `const` は型検査の後に `constants::fold` で展開する。したがって型の中の長さには使えない（D3）。
- F04（任意）: `Simd.load` は `ref [lane]` と `i64` を取る（`src/check.rs` の `Builtin::SimdLoad`）。固定長配列はスライス化（D7）でこの引数に渡る。F04 が未完了でも他の部分に影響しない。
- E05（done）: 公開 ABI は `src/abi.rs` の `parameter`・`result`・`Buffer::of` が判定する。Phase 1 はこれを変えない。

### 構文

```text
type_primary  = ... | "[" type_expr "]" | "[" type_expr ";" length "]" | "[|" type_expr "|]" | ...
length        = decimal integer literal without a suffix (checked later: 0 ..= 1024)
```

式の構文は増やさない。配列リテラル `[e1, ..., ek]` は期待型が固定長配列のときだけ固定長配列になる。生成関数は修飾名付き組み込み
`FixedArray.init`（D-07 の方式。`Int.popcount` と同じ登録）。

### 型規則

- `[T; N]` と `[T; M]` は N = M のときだけ同じ型。`[T; N]` と `[T]` は別の型で、暗黙の相互変換はない（例外は D7 のスライス化だけ）。
- 型変数に代入できる（`Maybe<[f64; 3]>`、`'a -> 'a` の `'a`）。長さを変数にする方法は Phase 2（D10）。
- 要素型の制約は `[T]` と同じ。`ref mut` を含む要素は `[T]` と同じコード・メッセージで拒否する（`tests/arrays.rs` の
  `arrays_cannot_store_mutable_references` が固定する検査を `Type::FixedArray` にも適用する）。
- リテラル: 期待型を `Inference::resolve` した結果が `[T; N]` なら、`[e1, ..., ek]` は k ≠ N で E1003、各 ei は期待型 T で検査する。
  期待型が固定長配列でなければ従来どおり `[T]`（`let xs = [1, 2, 3]` は `[i64]` のまま）。
- `FixedArray.init :: (i64 -> 'a) -> ['a; N]`。N は結果の期待型から決まり、決まらなければ E1015（D5）。
- 添字 `xs[i]`: `xs` が `[T; N]`・`ref [T; N]`・`ref mut [T; N]` なら結果は `T`、`i` は `i64`。
- `xs.length` は `i64` で、値は N。
- スライス `&xs[a..b]`（`ref xs[a..b]`）と呼び出し引数のスライス化（D7）の結果は `ref [T]`。元は place（束縛・フィールド・参照外しの連鎖）に限る。
- 性質は要素に従う: Copy ⇔ `T` が Copy、`needs_drop` ⇔ `T` が `needs_drop`。`can_capture`・`can_send`・`carries_loans` も `T` に従う。
  `exportable` ではない。構造的インスタンス（`Eq`・`Ord`・`Hash`・`Display`・`Default`）は持たない（D9）。

### 評価順序・所有権・借用

- リテラルの要素は左から右へ一度ずつ評価し、各要素を集成体へ移す（タプルと同じ）。
- `FixedArray.init f` は `f` を i = 0, 1, …, N−1 の昇順に N 回呼び、呼び終えたら `f` を drop する（`new [T](n, f)` と同じ）。
  途中のトラップで止まっても巻き戻しはない（D-10）。
- 値渡し・束縛: Copy なら確保なしの複製、非 Copy なら move（後の使用は E1012）。
- `xs[i]` は `xs` を消費せず、要素の複製（`clone_value`）を返す。Copy 要素は load だけ。要素の取り出し move はない（`[T]` と同じ）。
- 要素代入 `xs[i] = v` は既存の E1012。`let mut xs` への値全体の再代入は可能で、古い値を drop する。
- `&xs` は `ref [T; N]`（`ptr` 1 個）。`&xs[a..b]` とスライス化は `xs` の共有借用で、loan は `xs` に付く。`ref mut [T]` への変換はない（C08 の範囲）。
- `docs/language.md` の「呼び出し引数の暗黙の借用」の表に 1 行を足す: 実引数が `[T; N]` の place か `ref [T; N]`、仮引数が `ref [T]` のとき、全体のスライス。

### 数値・トラップ・native と WASM の差

- 添字は `icmp ult i64 %i, N` が偽なら `TrapKind::BoundsCheck`（負の添字も unsigned 比較で範囲外）。トラップ位置は添字式の span（既存の `guard`）。
- 添字が整数リテラル（型付き IR の `TypedExprKind::Int(v)`）で v < N なら検査を出さない。v ≥ N でも検査を残し、実行時にトラップする。
  コンパイルエラーにはしない（D6）。
- スライスは既存の `array_slice` と同じく start ≤ end ≤ N を検査し、違反は `TrapKind::BoundsCheck`。`FixedArray.init` 自体はトラップしない。
- native と WASM で意味の差はない。LLVM 型は両方で `[N x T]`、`storage_layout` は wasm32 の上界。差は stack の使用量だけ（落とし穴）。

### 診断

| コード | 条件 | メッセージ | 位置 |
| --- | --- | --- | --- |
| `E0002` | `[T;` の後が接尾辞なしの整数リテラルでない（名前、`-1`、`4i64`、式） | `a fixed array length must be an integer literal without a suffix, for example '[f64; 3]'` | `;` の次のトークン |
| `E0002` | `[\|T;` | `list types use '[\|T\|]', without a length; use '[T; N]' for a fixed-length array` | `;` |
| `E1010` | N > 1,024 | `fixed array length 1025 exceeds 1024 elements; use '[T]' for larger arrays`（数値は実際の N） | 型式 `[T; N]` |
| `E1010` | 保守的な大きさが 64 KiB 超 | 既存の `value layout exceeds 65536 bytes; use smaller value types` | 既存の `size_error` の位置 |
| `E1010` | 固定長配列を通る再帰（union を通らない） | 既存の `recursive value layout must pass through a union with a finite alternative` | 既存 |
| `E1003` | リテラルの要素数 ≠ N | `expected 3 elements for '[i64; 3]', found 2`（数値と型は実際の値） | リテラル全体 |
| `E1003` | 長さ・要素型の不一致、`[T; N]` と `[T]` の取り違え | 既存の `Checker::same` の不一致メッセージ（型は `[i64; 3]` と表示） | 既存 |
| `E1005` | `new` の後の固定長配列リテラル | `'new' creates a heap array or list; remove 'new' to create a fixed-length array value` | `new` 式 |
| `E1005` | `FixedArray.init` の結果の型が固定長配列でない | `'FixedArray.init' creates a fixed-length array; annotate a type such as '[i64; 4]'` | 呼び出し |
| `E1015` | `FixedArray.init` の長さが関数の終わりまで決まらない | `cannot determine the length of 'FixedArray.init'; annotate the result type, for example '[i64; 4]'` | 呼び出し |
| `E1005` | place でない固定長配列のスライス化（明示・暗黙とも） | `slicing a fixed-length array requires a named value; bind it with 'let' first` | 元の式 |
| `E1005` | `for x in xs`（`xs` が固定長配列） | `a fixed-length array is not enumerable; iterate 'for i in 0 .. xs.length' and index it` | 列挙の対象の式 |
| `E1020` | 配列パターンで固定長配列を分解 | `fixed-length arrays cannot be destructured by patterns; index the elements instead` | パターン |
| `E1012` | 要素代入 | 既存の `assignment replaces a mutable binding; record fields, array elements, and list elements are immutable` | 既存 |
| `E1008` | export のシグネチャに固定長配列 | 既存の `exports support scalar values, ...`（`src/check.rs` の文言のまま） | 既存 |

E1005 の `indexing requires an array, list, string, or utf8string` は文言を変えない（固定長配列も array に含まれる）。

### 資源上限

- 長さは 0 以上 1,024 以下（`MAX_FIXED_ARRAY_LENGTH`（新規）、D3）。
- 保守的な値の大きさは N × size(T)。size(T) は `Layouts::size` の要素の見積もりで、要素ごとの 16 byte 切り上げはしない（配列の stride は実寸で、
  見積もりは実寸以上）。`stack_size` も同じ式。レコードのフィールドになった場合は、従来どおりフィールド単位で 16 byte に切り上げる。
- 境界の例: `[i64; 1024]` は 8,192、`[[i64; 1024]; 8]` は 65,536 で受理、`[[i64; 1024]; 9]` は E1010。`[string; 1024]` は 16,384。
- 異なる `[T; N]` は異なる具体型で、ジェネリック関数の特殊化を別に作り、上限 1,024 に数える。
- LLVM の翻訳時間は手順 9 で計測する（停止条件: `[f64; 1024]` の値渡し関数 1 個の native `-O3` が 10 秒超）。

### 例

受理する例（新構文（実装後に有効。未検証））。`run` の結果は 10（1 + 5 + 4）、`dot` の例は 32.0。

```tsuzuri
def dot :: [f64; 3] -> [f64; 3] -> f64
fn dot a b = a[0] * b[0] + a[1] * b[1] + a[2] * b[2]

def squares :: [i64; 4]
fn squares = FixedArray.init (i -> i * i)

def first_two :: &[i64] -> i64
fn first_two xs = xs[0] + xs[1]

def run :: i64
fn run = { let v = squares; first_two v + first_two &v[1..3] + v.length }
```

拒否する例（新構文（実装後に有効。未検証））。各行を 1 つのソースとして検査する。

| ソース | 結果 |
| --- | --- |
| `def f :: [i64; n]` | E0002 |
| `def f :: [i64; 1025]` | E1010 |
| `def f :: [i64; 3]\nfn f = [1, 2]` | E1003 |
| `def f :: [i64; 3]\nfn f = new [1, 2, 3]` | E1005 |
| `def g :: [i64] -> i64\nfn g xs = xs.length\ndef f :: [i64; 2] -> i64\nfn f v = g v` | E1003 |
| `def f :: i64\nfn f = { let xs = FixedArray.init (i -> i); 0 }` | E1015 |
| `def f :: [i64; 2] -> i64\nfn f v = { v[0] = 1; 0 }` | E1012 |
| `export def f :: [i64; 2] -> i64\nfn f v = v[0]` | E1008 |
| `record Node { next: [Node; 2] }` | E1010 |

### Phase 2（設計方針、D10 の承認後）

- 型パラメーターとして長さを受け取る（例: `record Grid<'a, const N: i64> { cells: ['a; N] }`、関数の `[f64; N] -> [f64; N] -> f64`）。
  `const N: i64` は GUIDE D-30 が挙げる既存予約語の組み合わせで、新しい予約語を増やさない。
- `const` 名を長さに使うこと（旧仕様の「`i64` の `const` 名」）は Phase 2 で扱う。
- 長さの算術（`N * M`）は Phase 2 でも対象外。具体化ごとに別の特殊化になり、上限 1,024 に数える。
- 公開 ABI: スカラー要素の固定長配列をレコードのフィールドとして許可する（E12 と調整）。

## 設計

### データ構造

```rust
// src/syntax.rs
pub enum TypeExprKind {
    // ...
    /// `[T; N]`; lengths above `u64::MAX` saturate so the checker reports E1010.
    FixedArray(Box<TypeExpr>, u64),
}

// src/check.rs
pub const MAX_FIXED_ARRAY_LENGTH: u64 = 1024;
pub enum Type {
    // ...
    /// `[T; N]`: N elements stored inline, a value like a tuple.
    FixedArray(Box<Type>, u64),
}
pub enum Builtin { /* ... */ FixedArrayInit /* "FixedArray.init" */ }
pub enum BuiltinType { /* ... */ FixedArrayOf(Box<BuiltinType>) }

// src/polymorph.rs
enum FamilyKind { /* ... */ FixedArrayElement }
```

- `Type::FixedArray` は 2 語で、`Type` は 4 語のまま（停止条件）。`TypedExprKind` は増やさない。固定長配列のリテラルは
  `TypedExprKind::Tuple(values)` に `ty: Type::FixedArray(..)` を付けて表す（D5）。添字・長さ・スライスは既存の `Index`・`Length`・`Slice` を使う。

### 段ごとの変更

GUIDE §6.3（`Type` の variant）のチェックリストを HEAD の match に当てはめた一覧。規則: 型の走査と借用・loan の性質は `Type::Array` と同じ arm
（要素を辿る）、大きさ・再帰の有限性・LLVM の値は `Type::Tuple` と同じ扱い（inline の集成体）。「変更なし」の行には arm を足さない。

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| 構文 | `src/syntax.rs` | `TypeExprKind` | `FixedArray(Box<TypeExpr>, u64)` |
| 構文 | `src/parser.rs` | 型の一次式の `[`／`[\|` 分岐（HEAD の E0002 を出す箇所） | `;` なら `fixed_array_length`（新規、再帰しない）で長さを読み `]` を要求。`[\|` の後の `;` は E0002（新文言）。分岐は小さく保つ |
| 整形・文書 | `src/formatter.rs`、`src/docgen.rs` | `ty`、`type_text` | `[T; N]` と出力 |
| 型式の走査 | `src/semantic.rs`、`src/regions.rs`、`src/warnings.rs`、`src/polymorph.rs`、`src/check.rs` | `type_entry`、`ty`、`TypeExprKind::Array(inner)` を含む or-pattern の各 match、別名展開 `expand` | `Array` と同じ arm。`expand` は長さを保って再構築する |
| 検査 | `src/check.rs` | `resolve_type` | N > `MAX_FIXED_ARRAY_LENGTH` は E1010、それ以外は `Type::FixedArray` |
| 検査 | `src/check.rs` | `Type::display` | `[{T}; {N}]` |
| 検査 | `src/check.rs` | `is_copy`、`contains_error`、`contains_constructor`、`contains_reference`、`contains_mutable_reference`、`carries_loans`、`can_capture`、`can_send` | `Array` と同じ arm |
| 検査 | `src/check.rs` | `needs_drop` | `Self::FixedArray(element, _) => element.needs_drop(types)`（`Array` の常に true の列に入れない） |
| 検査 | `src/check.rs` | `exportable`、`shared_array_element` | 変更なし |
| 検査 | `src/check.rs` | `Layouts::size`、`Validation::check` | N × 要素、超過は `size_error`。`Validation::check` は `Type::Array` と同じ要素の検査も行う |
| 検査 | `src/check.rs` | `ExprKind::Array(values) \| ExprKind::List(values)` の arm | 期待型を解決し、固定長なら要素数の照合と `TypedExprKind::Tuple`（D5） |
| 検査 | `src/check.rs` | `ExprKind::NewLiteral` の arm | 結果が `Type::FixedArray` なら E1005 |
| 検査 | `src/check.rs` | `ExprKind::Index` の arm、`field_access`、`slice` | 要素型、`length`、`ref [T]` のスライス（place でなければ E1005） |
| 検査 | `src/check.rs` | `coerce_argument`、`fixed_array_view`（新規） | 暗黙のスライス化（アルゴリズム参照）。`coerce_argument` からは 1 行で呼ぶ |
| 検査 | `src/check.rs` | `Builtin`（`Builtin::SimdLoad` と同じく variant・名前・一覧・シグネチャの 4 か所）、`BuiltinType` | `FixedArrayInit` と `FixedArrayOf`。シグネチャは `(i64 -> 'a) -> FixedArrayOf('a)` |
| 検査 | `src/polymorph.rs` | `builtin_type`、`FamilyKind`、`solve_families` | `FixedArrayOf` は新しい推論変数と family を作る。解き方はアルゴリズム参照 |
| 検査 | `src/polymorph.rs` | `map_type`、`bounded_type`、`Inference::resolve`、`unify`、`type_expression` | 要素を辿る arm。`unify` は長さが等しいときだけ要素を単一化 |
| 検査 | `src/polymorph.rs` | `Classes::structural` と intrinsic メソッドの展開 | 変更なし（D9） |
| 検査 | `src/recursive.rs` | `Graph::visit`、`weight`、`stored_all` と `pending.extend(elements)` を持つ走査 | `Array` と同じく要素を辿る |
| 検査 | `src/recursive.rs` | `finite` | `Tuple` と同じく要素の有限性に従う（inline なので `_ => true` に落とさない） |
| 検査 | `src/control.rs` | `PatternKind::Array` の型付け、`for` の列挙対象の判定 | E1020、E1005 |
| 所有権 | `src/ownership.rs` | `owned`、`Ownership` の `is_copy`・`require_copy` | `Array` と同じ arm |
| 所有権 | `src/call_specialization.rs` | `mutable_reference` | 要素を辿る |
| 定数 | `src/constants.rs` | `fold` | 変更なし（`const` の型が固定長配列の場合を手順 5 のテストで確認する） |
| LLVM | `src/llvm.rs` | `llvm_type`、`canonical_type`、`storage_layout` | `[N x T]`、`fixed[N,T]`（D-03 の正規表記に追加）、(N × size, align) |
| LLVM | `src/llvm.rs` | 出力する型を集める走査（`Type::Tuple(types) => types.iter().for_each(...)`） | 要素を辿る |
| LLVM | `src/llvm.rs` | `TypedExprKind::Tuple` の arm | 変更なし（`self.ty(&expression.ty)` で `[N x T]` を組み立てる） |
| LLVM | `src/llvm.rs` | `is_place`、`emit_place`、`TypedExprKind::Index` と `TypedExprKind::Length` の arm、`array_slice` | 固定長配列の分岐（アルゴリズム参照） |
| LLVM | `src/llvm.rs` | `fixed_element_pointer`（新規）、`drop_value`、`clone_value` | 境界検査と GEP、要素のループ |
| LLVM | `src/llvm_bulk.rs` | `fixed_array_init`（新規）と builtin の振り分け | `Builtin::FixedArrayInit` の lowering |
| LLVM | `src/llvm_frame.rs` | `stack_size`、`field_types` | N × 要素、`Some(vec![element; N])`（入れ子の `[T]` リテラルを frame に置ける） |
| LLVM | `src/llvm_debug.rs` | `layout`、`Type::Tuple` を `(index.to_string(), ty)` の列にする match | 要素 N 個のメンバー `0`…`N-1`（D11） |
| LLVM | `src/llvm_compare.rs`、`src/llvm_hash.rs`、`src/llvm_display.rs` | 各 match | 変更なし（D9 により到達しない） |
| ABI | `src/abi.rs` | `parameter`、`result` | 変更なし（`exportable` が false なので E1008） |

### 生成 IR とランタイム

新しいランタイム記号・ヘッダー型・WASM import はない。`[N x T]` は無名の LLVM 型なので、固定長配列を書かないプログラムの IR は変わらない。

```llvm
; let v: [f64; 3] = [1.0, 2.0, 3.0]（束縛の slot は entry block の alloca）
%1 = insertvalue [3 x double] zeroinitializer, double 1.0, 0
%2 = insertvalue [3 x double] %1, double 2.0, 1
%3 = insertvalue [3 x double] %2, double 3.0, 2
store [3 x double] %3, ptr %v
; v[i]: 動的な添字
%4 = icmp ult i64 %i, 3
br i1 %4, label %ok, label %trap        ; guard(.., TrapKind::BoundsCheck)
%5 = getelementptr inbounds [3 x double], ptr %v, i64 0, i64 %i
%6 = load double, ptr %5
; v[1]: 範囲内の定数添字は検査なし
%7 = getelementptr inbounds [3 x double], ptr %v, i64 0, i64 1
; 引数 ref [f64] へのスライス化
%8 = insertvalue %tz.array zeroinitializer, ptr %v, 0
%9 = insertvalue %tz.array %8, i64 3, 1
```

関数の引数・戻り値は既存どおり SSA 値（`define [3 x double] @...([3 x double] %0)`）。`sret`・`byval` は使わない（停止条件）。

### アルゴリズム

```text
fixed_array_view(value, expected):              // coerce_argument の最初で呼ぶ。再帰しない
  element = expected.shared_array_element() else return None
  source = autoderef(value)
  [actual; n] = resolve(source.ty) else return None
  source が place でない → E1005（スライス化の文言）
  same(actual, element)?                        // 不一致は E1003
  return Slice { value: source, start: None, end: None } : ref [element]

fixed_element_pointer(ty = [T; N], base, index):  // base は [N x T] を指す ptr
  index が Int(v) かつ v < N でなければ guard(icmp ult i64 index, N, BoundsCheck)
  return getelementptr inbounds [N x T], ptr base, i64 0, i64 index

Index(array, index) で array.ty が [T; N]:
  base = is_place(array) ? place(array) : spill(read_operand(array))
  element = load T, fixed_element_pointer(..); result = clone_value(T, element)
  place でなければ release_operand                 // Length も同じ分岐で、結果は定数 N

array_slice で source.ty が [T; N]: view = { place(source), N }。以降は既存の [T] の経路と同じ

drop_value([T; N], value):   T が needs_drop のときだけ: slot = spill; array_loop(N): drop_value(T, load slot[i])
clone_value([T; N], value):  T が needs_drop でなければ value をそのまま返す
                             それ以外: src = spill, dst = slot; array_loop(N): store clone(load src[i]) → dst[i]; load dst

fixed_array_init(f, [T; N]): dst = slot; array_loop(N): store apply_value(f, i) → dst[i]; drop f; load dst

solve_families の FixedArrayElement:
  input を resolve: [e; n] → same(output, e) / 未定かつ最後でない → 保留 / 未定かつ最後 → E1015 / 他の型 → E1005
```

ループは既存の `array_loop` に `N` の文字列を渡して作る。要素数に比例する `extractvalue` の展開はしない（IR の大きさを N に比例させない）。

## 実装手順

D1 の承認前はどの手順にも着手しない。各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は
0 件でも成功するので、`running N tests` の N が期待どおりかを必ず見る。

### 手順 1: ベースラインを取る

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行し、固定長配列を使わない fixture の IR を保存する。stack-depth の 3 テストと特殊化上限のテストを実行する。
- 確認: 次がすべて成功する。4 つのテストはそれぞれ `1 passed`。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
mkdir -p /tmp/tz-a16
for f in arrays slices simd storage; do target/release/tsuzuri build tests/fixtures/$f --emit llvm -o /tmp/tz-a16/before-$f.ll; done
cargo test --locked --test polymorphism bounds_type_growing_polymorphic_recursion
cargo test --locked --lib bounds_recursive_and_flat_expression_depth
cargo test --locked --test computations bounds_nested_builder_expansion_not_just_source_syntax
cargo test --locked honors_the_exact_specialization_limit
```

### 手順 2: 型 `[T; N]`（共有変更）

- 変更: 「段ごとの変更」の構文・整形・文書生成・型式の走査・`resolve_type`・`Type` とその性質・`src/polymorph.rs` の走査と `unify`・
  `src/recursive.rs`（`finite` を除く）・`src/ownership.rs`・`src/call_specialization.rs`・`llvm_type`・`canonical_type`・`storage_layout`・
  型を集める走査・`stack_size`・`src/llvm_debug.rs`。`tests/fixed_arrays.rs`（新規）、`tests/arrays.rs`、`src/check.rs` の `tests`。
- 内容: parser の分岐は `fixed_array_length`（新規）を呼ぶだけにする。`tests/fixed_arrays.rs` に `tests/arrays.rs` の `accepts`・`rejects` を写し、
  メッセージも見る `diagnostic(source) -> Diagnostic` を足す。`tests/arrays.rs` の `array_constructors_require_a_length_and_typed_initializer`
  から E0002 の 3 件を消して `fixed_array_types_resolve_and_display` の受理へ移す。`rejects_invalid_programs_with_stable_codes` の
  `fn f(a: [i64; 4]) -> i64 { 0 }` を `fn f(a: [i64; n]) -> i64 { 0 }`（E0002 のまま）に替える。`src/check.rs` の `tests` に
  `type_stays_four_words`（新規、`assert!(std::mem::size_of::<Type>() <= 32)`）を足す。
- 確認: `cargo test --locked --test fixed_arrays` が `2 passed`、`cargo test --locked --test arrays` が `7 passed`、
  `cargo test --locked --lib type_stays_four_words` と `cargo test --locked --lib rejects_invalid_programs_with_stable_codes` が `1 passed`。
  `cargo build --release --locked` の後、手順 1 と同じコマンドで `/tmp/tz-a16/after-$f.ll` を出し、`cmp` が 4 組とも差分を出さない。
  手順 1 の 4 テストが成功する。

### 手順 3: 大きさと再帰の上限

- 変更: `Layouts::size`、`Validation::check`、`src/recursive.rs` の `finite`。
- 内容: 大きさは N × 要素の見積もり（「資源上限」）。`Validation::check` は `Type::Array` と同じ要素の検査（`ref mut` の拒否）も行う。
  `finite` は `Tuple` と同じく要素に従う。
- 確認: `fixed_array_layout_limit` を足し、`cargo test --locked --test fixed_arrays` が `3 passed`。
  `cargo test --locked --lib accepts_exact_value_limits_and_rejects_the_next_scalar` が `1 passed`。

### 手順 4: リテラル

- 変更: `ExprKind::Array(values) | ExprKind::List(values)` と `ExprKind::NewLiteral` の arm、`src/llvm_frame.rs` の `field_types`。
- 内容: 期待型を `Inference::resolve` してから分岐する。固定長の経路は要素数を先に照合し（E1003）、`TypedExprKind::Tuple` を返す。
  `TypedExprKind::Tuple` を match する 5 か所（`src/check.rs`、`src/llvm_frame.rs` の 2 か所、`src/llvm.rs`、`src/polymorph.rs`）を読み、
  `Type::Tuple` を前提に分解している箇所がないことを確かめる。
- 確認: `fixed_array_literals_need_the_expected_length` を足し、`cargo test --locked --test fixed_arrays` が `4 passed`。

### 手順 5: 添字と長さ

- 変更: `ExprKind::Index` の arm、`field_access`、`is_place`、`emit_place`、`TypedExprKind::Index`・`TypedExprKind::Length` の arm、
  `fixed_element_pointer`（新規）。
- 内容: アルゴリズムのとおり。place は slot のポインターから GEP し、配列全体を load しない。定数添字の省略は `TypedExprKind::Int(v)` を
  u128 のまま N と比べる。
- 確認: `fixed_array_indexing_and_length` を足し、`cargo test --locked --test fixed_arrays` が `5 passed`。

### 手順 6: Copy・move・drop・clone と型引数

- 変更: `drop_value`、`clone_value`。
- 内容: 要素が `needs_drop` のときだけ `array_loop` で要素ごとに drop・clone する。Copy の固定長配列は SSA 値のまま複製する。
- 確認: `fixed_arrays_copy_or_move_by_element` と `fixed_arrays_in_generic_and_record_types` を足し、`cargo test --locked --test fixed_arrays` が
  `7 passed`。`cargo test --locked` が成功する。

### 手順 7: `FixedArray.init`

- 変更: `Builtin`、`BuiltinType`、`builtin_type`、`FamilyKind`、`solve_families`、`src/llvm_bulk.rs` の `fixed_array_init`（新規）と builtin の振り分け。
- 内容: family は SIMD と同じく未定なら保留し、関数の終わり（`last`）で E1015 にする。lowering は `TypedExprKind::NewArray` の arm と同じ呼び出し方で
  `apply_value` を使い、格納先だけを slot にする。
- 確認: `fixed_array_init_takes_the_length_from_the_expected_type` を足し、`cargo test --locked --test fixed_arrays` が `8 passed`。
  `cargo test --locked honors_the_exact_specialization_limit` が `1 passed`（std の汎用関数を固定長配列ごとに具体化していない）。

### 手順 8: スライス化と残りの拒否

- 変更: `slice`、`coerce_argument`、`fixed_array_view`（新規）、`array_slice`、`src/control.rs` の `PatternKind::Array` の型付けと `for` の列挙対象の判定。
- 内容: `coerce_argument` の変更は `fixed_array_view` を呼ぶ 1 行だけ。期待型が `Type::Infer` のときは何もしない（型変数の仮引数には `[T; N]` のまま渡る）。
- 確認: `fixed_arrays_slice_into_shared_borrows` と `fixed_arrays_reject_patterns_iteration_and_exports` を足し、`cargo test --locked --test fixed_arrays` が
  `10 passed`。手順 1 の stack-depth 3 テストが成功する。

### 手順 9: 大きな固定長配列の翻訳時間

- 変更: なし（計測だけ）。
- 内容: `/tmp/tz-a16/big/Main.tz` に次を置き（新構文（実装後に有効。未検証））、IR と翻訳時間を記録する。
- 確認: 各コマンドが成功し、native `-O3` の `real` が 10 秒以下（超えたら停止条件）。4 つの時間を完了報告に書く。

```tsuzuri
def pick :: [f64; 1024] -> i64 -> f64
fn pick v i = v[i]

export def run :: i64 -> f64
fn run i = pick (FixedArray.init (k -> k as f64)) i
```

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
target/release/tsuzuri build /tmp/tz-a16/big --emit llvm -o /tmp/tz-a16/big.ll
for o in -O0 -O3; do for t in arm64-apple-macos wasm32; do
  /usr/bin/time -p /opt/homebrew/opt/llvm@21/bin/clang --target=$t $o -c /tmp/tz-a16/big.ll -o /dev/null
done; done
```

### 手順 10: E2E suite `fixed_arrays`

- 変更: `tests/fixtures/fixed_arrays/Main.tz`（新規）、`tests/features.mjs` の `suites`、`tests/fixed_arrays.rs`。
- 内容: 「E2E」の関数・期待値・トラップ・`inspect` を書く（GUIDE §7.2）。`tests/arrays.rs` の `runtime_fixture_lowers_for_both_targets` を写した
  同名のテストを足す。
- 確認: `cargo test --locked --test fixed_arrays` が `11 passed`。
  `cargo build --release --locked && node tests/features.mjs target/release/tsuzuri fixed_arrays` が `fixed_arrays: native/WASM at O0/O3 passed` を出す。

### 手順 11: 既存 IR の不変と生成コード

- 変更: なし。
- 内容: 手順 2 の `cmp` を再実行する。`target/release/tsuzuri build tests/fixtures/fixed_arrays --emit llvm -O3 -o /tmp/tz-a16/fixed-O3.ll` を読み、
  `fixed_copy` に `@tz.alloc` がないこと、`fixed_index` の GEP が `[4 x i64]` であることを目で確かめる。
- 確認: `cmp` が差分を出さない。`grep -c "@tz.alloc" /tmp/tz-a16/fixed-O3.ll` の値を完了報告に書く（`fixed_strings` などの確保だけが残る）。

### 手順 12: ドキュメント

- 変更: 「ドキュメント」の全ファイル。
- 内容: 仕様・診断・上限・例を書く。サンプルは実装後のコンパイラで検証する。
- 確認: `node scripts/check-docs.mjs _docs/language-reference/types.md _docs/library-reference/arrays-and-lists.md _docs/feature-status.md` が成功し、
  `git diff --check` が何も出さない。

### 手順 13: 最終確認

- 変更: なし。
- 内容: GUIDE §3 の全体検証と §10 の完了の定義を確かめる。
- 確認: `cargo test --locked`、`cargo build --release --locked && node tests/features.mjs target/release/tsuzuri`、手順 1 の 4 テストがすべて成功する。

## テスト計画

### Rust テスト

`tests/fixed_arrays.rs`（新規）。受理は `accepts`（IR を 2 回出して一致）、拒否は `rejects(source, code)`、新しい文言は `diagnostic` で全文を比べる。

| テスト | 内容 |
| --- | --- |
| `fixed_array_types_resolve_and_display` | `tests/arrays.rs` から移した 3 件の受理。`[i64; 3]` と `[i64; 4]`、`[i64; 3]` と `[i64]` の不一致が E1003 で、メッセージに `[i64; 3]` を含む |
| `fixed_array_lengths_are_bounded_literals` | `[i64; 0]`・`[i64; 1024]` を受理。`[i64; n]`・`[i64; -1]`・`[i64; 4i64]`・`[\|i64; 2\|]` は E0002（文言も比較）、`[i64; 1025]` は E1010（文言も比較） |
| `fixed_array_layout_limit` | `[[i64; 1024]; 8]` を受理し `[[i64; 1024]; 9]` は E1010。レコードのフィールドでも同じ境界。`record Node { next: [Node; 2] }` は E1010、`union Tree = Leaf \| Node of [Tree; 2]` は受理。`[ref mut i64; 2]` の格納は `[T]` と同じコード |
| `fixed_array_literals_need_the_expected_length` | 要素数の一致を受理、不一致は E1003（文言も比較）。`let xs = [1, 2]` は `[i64]` の仮引数へ渡せる。`new [1, 2]` を `[i64; 2]` に置くと E1005。`[u8; 0]` への `[]` を受理 |
| `fixed_array_indexing_and_length` | `v[i]`・`v.length`・`p.pos[i]`・`m[i][j]`・`ref [i64; 3]` 経由の添字を受理。IR に `getelementptr inbounds [3 x i64], ptr` があり、定数添字だけの関数に `icmp ult i64 %` がない。`v[0] = 1` は E1012 |
| `fixed_arrays_copy_or_move_by_element` | `[i64; 2]` を 2 回使えて IR に `@tz.alloc` がない。`[string; 2]` を move した後の使用は E1012。捕捉と `Task.run` への送出は要素の規則どおり |
| `fixed_arrays_in_generic_and_record_types` | `'a -> 'a` に `[f64; 3]` と `[f64; 4]` を渡すと特殊化が 2 個。`Maybe<[f64; 3]>`、ジェネリックなレコードのフィールド、`const` の値（`docs/language.md` の「コンパイル時定数」の構文）を受理 |
| `fixed_array_init_takes_the_length_from_the_expected_type` | 注釈・仮引数・レコードのフィールドから N が決まる場合を受理。注釈なしは E1015、`[i64]` を期待すると E1005（文言も比較） |
| `fixed_arrays_slice_into_shared_borrows` | `&[i64]` の仮引数への暗黙のスライス化、`&v[1..3]`、`ref [T; N]` からの変換、`Simd.load` への受け渡しを受理。関数呼び出しの結果のスライス化は E1005、`[i64]`（所有）の仮引数は E1003、`ref mut [i64]` の仮引数は E1003 |
| `fixed_arrays_reject_patterns_iteration_and_exports` | 配列パターンは E1020、`for x in v` は E1005（文言も比較）、export のシグネチャは E1008、`==` はインスタンスがない既存の診断 |
| `runtime_fixture_lowers_for_both_targets` | `tests/fixtures/fixed_arrays` を native と wasm32 の両方へ lowering できる |

`src/check.rs` の `tests`: `type_stays_four_words`（新規）と、`rejects_invalid_programs_with_stable_codes` の 1 件の差し替え。

### E2E

`tests/fixtures/fixed_arrays/Main.tz`（新規）の export はすべて `i64` を取り `i64` を返す。suite `fixed_arrays` は native/WASM × `-O0`/`-O3` で
実行し、各呼び出しの `live == 0` と WASM の import なしを既存の仕組みで検査する。期待値は JS の BigInt で書き、コンパイラの出力から写さない
（`min`・`max` は `tests/features.mjs` の定数）。

| export | 本体の要点 | 引数と期待値 |
| --- | --- | --- |
| `fixed_sum x` | `let v: [i64; 4] = [x, x + 1, x + 2, x + 3]` の 4 要素の和 | `0n` → `6n`、`min`・`max` → `BigInt.asIntN(64, 4n * x + 6n)` |
| `fixed_index i` | `let v: [i64; 4] = FixedArray.init (k -> k * k)` の `v[i]` | `0n` → `0n`、`3n` → `9n` |
| `fixed_copy` | `let mut a = [1, 2, 3]`（`[i64; 3]`）、`let b = a`、`a = [7, 8, 9]`、`b[0] * 100 + a[0]` | `107n` |
| `fixed_strings` | `[to_string 1, to_string 22, to_string 333]` を move し、要素の長さの和 | `6n` |
| `fixed_clone_nested` | `[[string; 2]; 2]` の `m[1]` を複製し、`row[0].length * 10 + row[1].length + m[1][1].length` | `38n` |
| `fixed_nested` | `[[1, 2], [3, 4]]` の `m[0][1] * 10 + m[1][0]` | `23n` |
| `fixed_slice` | `probe xs = xs[0] * 100 + xs.length` に `v: [i64; 5] = [1, 2, 3, 4, 5]` と `&v[1..4]` を渡した和 | `308n` |
| `fixed_record` | `Particle { pos: [1.0, 2.0, 3.0], id: 7 }` の `(p.pos[2] as i64) * 10 + p.id` | `37n` |
| `fixed_generic flag` | `pick :: bool -> 'a -> 'a -> 'a` で `[1, 2]` と `[3, 4]` を選び `c[0] * 10 + c[1]` | `1n` → `12n`、`0n` → `34n` |
| `fixed_init_closure n` | `let v: [i64; 8] = FixedArray.init (i -> i * n)` の `v[7]` | `3n` → `21n`、`max` → `BigInt.asIntN(64, 7n * max)` |
| `fixed_simd` | `[f32; 4] = [1.0, 2.0, 3.0, 4.0]` を `Simd.load v 0` で読み、lane 3 を `i64` にする（`simd` suite の fixture と同じ API） | `4n` |
| `fixed_length` | `[u8; 0]` の `[]` と `[i64; 1024]` の `FixedArray.init (i -> i)` で `v.length * 10000 + w.length + w[1023]` | `2047n` |

トラップ（`TrapKind::BoundsCheck`）: `fixed_index` の `-1n`・`4n`・`max`、`trap_constant`（`[i64; 2]` の `v[2]`）、`trap_slice`（`[i64; 5]` の `&v[3..6]`）。

`inspect(ir)`: `/getelementptr inbounds \[4 x i64\], ptr %[\w.]+, i64 0, i64/` と `/insertvalue \[3 x i64\]/` に一致する。`fixed_copy` の
関数本体（`define` から次の `}` まで）が `/@tz\.alloc/` と `/icmp ult i64 \d+, \d+/` に一致しない。

### 既存テストへの影響

- `tests/arrays.rs` の `array_constructors_require_a_length_and_typed_initializer` から E0002 の 3 件（`[i64; 4]` の関数、`[i64; 0]` のフィールド、
  `let xs: [i64; 1] = [1]`）を消す。受理になるのが新仕様だからで、`tests/fixed_arrays.rs` へ受理として移す。
- `src/check.rs` の `rejects_invalid_programs_with_stable_codes` の `fn f(a: [i64; 4]) -> i64 { 0 }` を `[i64; n]` にする。長さ無しの
  `fn f(a: [i64]) -> i64 { 0 }` は HEAD で受理される（W1001 だけ。2026-09-29 に確認）ので、`[i64; 4]` は新仕様で受理になる。
- これ以外の期待値は変えない（停止条件）。

### 性能

時間の合否条件は設けない。確認するのは生成コードだけ: 手順 11 の `@tz.alloc` の不在と GEP、手順 9 の翻訳時間。`[f64; 3]` と `[f64]`・C の
`double[3]` の実行時間の比較は任意で、行う場合は `docs/benchmarks.md` の手順で計測し、計測した数値だけを記録する。

## ドキュメント

- `docs/language.md`: `## 型とメモリ`（型の一覧と値の性質）、「スタックとヒープ（`new`）」の節（固定長配列は値で、`new` と組み合わせない）、
  `#### 呼び出し引数の暗黙の借用`（表に 1 行）、`### 配列・連結リストとレコード`（添字・長さ・スライス・パターン非対応）、
  `### 配列・リスト API`（`FixedArray.init`）、`## トラップ位置`（添字の BoundsCheck）、診断の一覧（E0002 の新文言、E1003・E1005・E1010・E1015・E1020 の追加文言）。
- `docs/architecture.md`: 値の LLVM 表現を説明する箇所（`grep -n "%tz.array" docs/architecture.md` で探す）に `[N x T]`、`fixed[N,T]`、
  大きさの見積もり、定数添字の検査の省略を書く。
- `_docs/language-reference/types.md`: `## 型の組み立て` と `## レイアウトと上限`。
- `_docs/library-reference/arrays-and-lists.md`: `## 共有スライス`（固定長配列からのスライス化）と、固定長配列の節（新規、`FixedArray.init`）。
- `_docs/feature-status.md` の A16 の行と `_features/README.md` の A16 の状態（Phase 1 完了の表記は GUIDE §10 に従う）。
- 承認の記録: GUIDE §9 の D-30 から A16 の項目を D-07（std `FixedArray`）・D-15（`[T; N]`）へ移し、D-03 に `fixed[N,T]`、D-13 にスライス化を追記する。

## 受け入れ条件

- [ ] D1 の承認が GUIDE §9 に記録されている。
- [ ] `[T; N]` を宣言・リテラル・`FixedArray.init` で作り、添字・`length`・スライス化・値渡しで使える。
- [ ] 「診断」の表のすべての行が Rust テストで固定されている（新しい文言は全文比較）。
- [ ] Copy の固定長配列の複製で `@tz.alloc` が出ない（手順 11）。
- [ ] 手順 1 の 4 fixture の IR が byte 単位で変わらない。
- [ ] suite `fixed_arrays` が native/WASM × `-O0`/`-O3` で成功し、`live == 0`、WASM の import がない。
- [ ] 手順 9 の翻訳時間が完了報告にあり、native `-O3` が 10 秒以下。
- [ ] 既存テストの期待値の変更は「既存テストへの影響」の 4 件だけ。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- リテラルを `TypedExprKind::Tuple` で表すので、`TypedExprKind::Tuple` の後で `let Type::Tuple(..) = &expression.ty else { unreachable!() }` と
  分解する箇所があれば panic する。`frame_aggregate` は `field_types` を使うので、`field_types` が `None` を返すと入れ子のリテラルで落ちる。
  手順 4 で 5 か所を読み、`[[string; 2]; 2]` の受理テストで確かめる。
- `Type::is_copy` の fallback は `_ => true`、`needs_drop` の fallback は `_ => false`。arm を忘れると `[string; 2]` が Copy 扱いになり二重解放になる。
  逆に `needs_drop` で `Array` の列（常に true）へ入れると Copy の配列に drop ループが出る。E2E の `live == 0` と手順 11 の IR で検出する。
- `recursive.rs` の `finite` の fallback は `_ => true`（ヒープを介す扱い）。固定長配列を `Array` 側に入れると無限の大きさの型を受理し、コンパイラか
  LLVM が stack を使い切る。`record Node { next: [Node; 2] }` の E1010 テストで検出する。
- LLVM の `extractvalue`・`insertvalue` は定数添字しか取れない。動的な添字は必ず slot へのポインターと GEP で扱う。place を `read_operand` で読むと
  `-O0` で配列全体を load するので、place は `place` のポインターを使う。
- `TypedExprKind::Int` の値は u128。定数添字の判定で `as u64` などに切り詰めると負の添字の検査を省いてしまう。u128 のまま N と比べる。
- 値の上限は具体化した型ごとに検査する（GUIDE D-03）。`'a` に `[[i64; 1024]; 9]` を代入した場合も `validate_size` の構築境界で E1010 になることを
  `fixed_array_layout_limit` に含める。
- 大きな値を多数の局所変数に置くと、特に WASM で stack を使い切る。コンパイラは値ごとの 64 KiB しか検査しない。`docs/language.md` の
  「スタックとヒープ（`new`）」の節に、大きな作業領域は `[T]` を使うと書く。
- 暗黙のスライス化は期待型が解決済みの `ref [T]` のときだけ。型変数の仮引数（`'a`）では起きないので、関数解決の曖昧さは生じない。
  `coerce_argument` に処理を書き込むと再帰フレームが大きくなるので、`fixed_array_view` へ分ける（stack-depth テスト）。
- `canonical_type` の表記は単射にする。`fixed[2,fixed[2,i64]]` と `fixed[2,array[i64]]` が区別されることを入れ子のテストで確かめる。

## 対象外

- 配列パターン・`for` による列挙・構造的インスタンス（`Eq`・`Ord`・`Hash`・`Display`・`Default`）（D9）。
- 要素代入と `ref mut [T]` への変換（C08 の方針に従う）。可変長の inline 配列。
- 長さへの `const` 名・const ジェネリクス・長さの算術（Phase 2、D10）。
- 公開 ABI の固定長配列（Phase 2、E12）。SIMD ベクトルとの直接の変換（bitcast）。
- 所有する `[T]` との変換関数。`new [T](N, i -> xs[i])` と `FixedArray.init (i -> ys[i])` で書ける（D7）。
- 範囲外の定数添字のコンパイルエラー化（D6）。ループ内の境界検査の除去（F12）。`DW_TAG_array_type` での DWARF 表現（G16）。

## 決定事項

### D1: 型構文 `[T; N]` の再導入

- 決定: 固定長配列の型を `[T; N]` と書く。std の名前 `FixedArray`（D-30 の仮割り当て）と、呼び出し引数の暗黙のスライス化を導入する。
  GUIDE §9 の D-30（A16 の承認）、D-07（std `FixedArray`）、D-15（型構文）、D-03（正規表記 `fixed[N,T]`）、D-13（固定長配列からの部分参照）を変更する。
- 理由: 長さを型に持つには新しい型構文が必要（「目的」の結論）。代替の `Array<T, N>`・`FixedArray<T, 3>` は D-02 の「型引数は完全な型」の例外を要し、
  C/C++・Rust の利用者にも `[T; N]` のほうが読みやすい。HEAD の E0002 の文言が `[T]` を案内しているのは旧構文を廃止した経緯による。
- 状態: 要承認（承認前は Phase 1 のどの手順にも着手しない）

### D2: 型の内部表現

- 決定: `Type::FixedArray(Box<Type>, u64)` と `TypeExprKind::FixedArray(Box<TypeExpr>, u64)`。`Type::Array` とは別の variant にする。
- 理由: `Array` に長さを持たせると `[T]` の全 match の意味が変わり、IR の不変を保てない。2 語なので `Type` は 4 語のまま。
- 状態: 既定案（実装者はこの案に従う）

### D3: 長さの書き方と上限

- 決定: 長さは接尾辞なしの十進整数リテラルで、0 以上 1,024 以下。超過は E1010。旧仕様の「`i64` の `const` 名」は Phase 2 へ移す。
- 理由: `const` は型検査の後に畳み込まれるので、型の解決時に値がない。1,024 は `[8192 x i64]` の値渡しで native `-O3` が 13 秒かかった計測から
  選び、手順 9 で確かめる。見直し提案: 旧仕様から `const` 名を外した（D1 の承認時に確認する）。
- 状態: 既定案（実装者はこの案に従う）

### D4: 値の表現と大きさ

- 決定: LLVM 型は `[N x T]` の SSA 値。束縛は entry block の slot、関数の引数・戻り値は SSA 値（`sret`・`byval` なし）。`storage_layout` は
  (N × 要素の大きさ, 要素の align)。保守的な見積もりは N × 要素の見積もりで、上限は `MAX_VALUE_BYTES`。
- 理由: タプル・レコードと同じ経路で、新しいヘッダー型もランタイムも要らない。要素ごとの 16 byte 切り上げは stride と一致せず、`[u8; N]` を過大に見積もる。
- 状態: 既定案（実装者はこの案に従う）

### D5: リテラルと `FixedArray.init` の型付け

- 決定: 固定長配列のリテラルは期待型が固定長配列のときだけで、型付き IR は `TypedExprKind::Tuple`。`FixedArray.init` は `BuiltinType::FixedArrayOf` と
  `FamilyKind::FixedArrayElement` で長さを期待型から決め、決まらなければ E1015、固定長配列でなければ E1005。
- 理由: `TypedExprKind` を増やすと全 match に arm が要る。タプルのリテラルは要素を順に評価して集成体へ移すだけで、意味が同じ。family は
  `FamilyKind::SimdLane` と同じ仕組みで、`Inference` の単一化規則を変えない。
- 状態: 既定案（実装者はこの案に従う）

### D6: 添字の境界検査

- 決定: 添字は常に実行時の `TrapKind::BoundsCheck` で検査する。範囲内の整数リテラルの添字だけ検査を出さない。範囲外の定数添字もコンパイルエラーにしない。
- 理由: 旧仕様の「定数添字でも意味を変えない」を保つ。コンパイルエラーにすると、実行されない分岐の添字でも受理が変わる。ループ内の証明は F12 の担当。
- 状態: 既定案（実装者はこの案に従う）

### D7: スライスと `[T]` との変換

- 決定: `&xs[a..b]` と、仮引数 `ref [T]` への暗黙のスライス化を提供する。元は place に限る。所有する `[T]` との暗黙の変換と専用の変換関数は作らない。
- 理由: 借用は確保なしで既存の `[T]` API（`Simd.load` を含む）を使える。所有への変換は確保の位置を隠すので、`new [T](N, i -> xs[i])` と明示させる。
  place に限るのは、一時値の slot を借用が越えて使うのを防ぐため。
- 状態: 既定案（実装者はこの案に従う）

### D8: Copy・move・drop・clone

- 決定: 性質は要素に従う。drop と clone は要素が `needs_drop` のときだけ、slot と `array_loop` で要素ごとに行う。Copy の配列は SSA 値の複製だけ。
- 理由: 要素ごとの `extractvalue` の展開は IR を N に比例させる。ループは `-O3` で小さい N なら展開される。
- 状態: 既定案（実装者はこの案に従う）

### D9: Phase 1 で持たない機能

- 決定: 配列パターン（E1020）、`for` による列挙（E1005）、構造的インスタンス、要素代入（既存の E1012）を Phase 1 では提供しない。
- 理由: それぞれ網羅性・反復プロトコル・比較関数の生成の設計が要る。分解はタプル、列挙は `for i in 0 .. xs.length` で代わりに書ける。明示の文言で拒否し、
  後から緩めても既存のプログラムを壊さない。
- 状態: 既定案（実装者はこの案に従う）

### D10: Phase 2 の const ジェネリクス

- 決定: 長さの型パラメーターは `const N: i64` を型パラメーター列に置く形式（例: `record Grid<'a, const N: i64>`）。`const` 名の長さもここで扱う。
  関数での宣言位置・推論・特殊化上限への数え方は Phase 2 の着手前レビューで決める。
- 理由: GUIDE D-30 が既存の予約語の組み合わせ `const N: i64` を優先すると定めている。D-02 の変更を伴う。
- 状態: 要承認（承認前は Phase 2 に着手しない）

### D11: DWARF の表現

- 決定: 固定長配列の debug 型はタプルと同じ構造体で、メンバー名は `0`…`N-1`。大きさは `src/llvm_debug.rs` の `layout` で N × 要素。
- 理由: 既存のタプルの経路をそのまま使える。`DW_TAG_array_type` への変更はデバッガー体験（G16）で扱う。
- 状態: 既定案（実装者はこの案に従う）

### D12: 公開 ABI と SIMD

- 決定: Phase 1 では export のシグネチャに固定長配列を置けない（`src/abi.rs` を変えず E1008）。SIMD とはスライス化経由（`Simd.load`）だけでつなぐ。
- 理由: C の配列引数はポインターに退化するので、ABI の形を E12 と決める必要がある。bitcast による変換は要素型と lane 数の規則が別に要る。
- 状態: 既定案（実装者はこの案に従う）

## 実装と検証（2026-10-06）

「C08、A14、A16、F13、F08 の実装をすべて完遂して」という依頼を D1・D10 の承認として扱い、Phase 1 と Phase 2（const ジェネリクスと公開 ABI のフィールド）を実装した。
着手時の HEAD は `ff84e4c`（ブランチ `Phase7-3`）で、C08・A14・F13・F08 と同じ変更に含めた。性能の改善は主張しない（生成コードと翻訳時間だけを確かめた）。

### 実装

- 構文: `TypeExprKind::FixedArray(Box<TypeExpr>, Box<TypeExpr>)` と `TypeExprKind::Length(u64)`。parser は角括弧の閉じ方を `close_bracket_type`、長さを
  `fixed_array_length`・`length_literal` に分け（いずれも `#[inline(never)]`）、型引数の列を `type_arguments` のループで読む（長さのリテラルを受け付ける）。
  `const N: i64` は型パラメーター名 `#N` になる。formatter・docgen・regions・semantic・warnings・polymorph に腕を足した。
- 型: `Type::FixedArray(Box<Type>, Box<Type>)` と `Type::Length(u64)`（長さパラメーターは `Type::Variable("#N")`）。`resolve_length` が長さを解決し、
  `Names::length_constants`（初期化式が整数リテラルの `i64` 定数）と型宣言の長さパラメーターを引く。リテラルは `fixed_array_literal`（`TypedExprKind::Tuple`）、
  `FixedArray.init` は `BuiltinType::FixedArrayOf` と `FamilyKind::FixedArrayElement`、暗黙のスライス化は `fixed_array_view`。
- 所有権・再帰・コピー: `src/ownership.rs`、`src/recursive.rs`、`src/copies.rs`、`src/call_specialization.rs`、`src/constants.rs`、`src/higher_kinds.rs`、
  `src/control.rs`（`for` と配列パターンの拒否）に腕を足した。
- 生成: `[N x T]`、`canonical_type` の `fixed[N,T]`、`fixed_element_pointer`（N 未満のリテラル添字は検査なし）、drop と clone の要素ループ、
  `fill_fixed_array`（`FixedArray.init` のインライン展開。関数値は結果の型ごとの builtin ラッパー）。`src/llvm_frame.rs`、`src/llvm_debug.rs`（メンバー `0`…`N-1`）。
- 公開 ABI（Phase 2）: `src/abi.rs` の `scalar_record` がスカラー型の固定長配列フィールドを許し、`src/llvm_abi.rs` が `[N x abi]`、ヘッダーの `T name[N];`、
  ホストとの読み書きのループを出す。
- テスト: `tests/fixed_arrays.rs`（新規 15 件。チケットの 11 件と、Phase 2 の `length_parameters_generalize_functions`・`length_parameters_on_records_unions_and_aliases`・
  `integer_constants_name_lengths`・`exported_records_hold_scalar_fixed_arrays`（C のホストと node の WASM ホスト））、`tests/formatter.rs` の
  `formats_fixed_arrays_and_length_parameters`、`src/check.rs` の `type_stays_four_words`、`tests/fixtures/fixed_arrays/Main.tz` と suite `fixed_arrays`
  （23 ケースとトラップ 5 つ）。`tests/arrays.rs` の 3 件と `rejects_invalid_programs_with_stable_codes` の 1 件（`[i64; -1]` にした）は「既存テストへの影響」のとおり。
- 文書: `docs/language.md`、`docs/architecture.md`、`_docs/language-reference/types.md`、`_docs/library-reference/arrays-and-lists.md`、
  `_docs/guides/native-interop.md`、`_docs/learn/why-tsuzuri.md`、`_docs/feature-status.md`、`_features/README.md`、`_features/GUIDE.md`（D-03・D-07・D-13・D-15・D-30・D-39）。

### 決定事項への追記（チケットから外れた判断）

1. **D2 の長さは `Box<Type>`。** Phase 2 の長さパラメーターを型変数と同じく置換・単一化・単相化するため、長さを `u64` ではなく型
   （`Type::Length(n)` か `Type::Variable("#N")`）で持つ。`Type` は 4 語のまま（`type_stays_four_words`）。
2. **D3 の `const` 名を Phase 2 で戻した。** 型の解決時に値が要るので、初期化式が整数リテラルの `i64` 定数だけを長さにできる（宣言の構文から読む）。
   式を初期化式に持つ定数は `E1005`。
3. **D10 の関数の長さパラメーターは暗黙。** 関数には型パラメーター列がないので、型注釈の中の解決できない名前の長さを `'a` と同じく暗黙のパラメーターにし、
   呼び出しごとに推論して特殊化する（既存の特殊化の上限で数える）。record・union・型エイリアスは `const N: i64` の宣言が必要で、未宣言・重複・未使用は `E1024`。
   型引数の位置での長さと型の取り違えは `E1004`。
4. **D12 の公開 ABI を Phase 2 で実装した。** E12 が done なので、スカラー型の固定長配列をレコードのフィールドに限って許し、C の `T name[N]` にした。
   引数・結果に直接置くと C では配列がポインターへ退化するので、引き続き `E1008`。
5. **`for` の文言。** 範囲 `a .. b` は終端を含むので、案内を `for i in 0 .. xs.length - 1` にした（チケットは `0 .. xs.length`）。
6. **排他スライス。** C08 で排他スライスができたが、固定長配列からは作らず `E1005` にした（値の一部の排他借用には、フレーム上の値の寿命と移動の規則が要る）。
   要素の置き換えは `let mut` の束縛への代入で行う。
7. **E2E の `fixed_clone_nested`。** 非 Copy の要素の読み取りは `E1012` なので、行は `ref copy[1]` で借りる。

### 確認（Apple M1 Max、macOS 27.0.1、Apple clang 21、Homebrew LLVM 21、rustc 1.98.1、Node v20.19.6）

- `cargo test --locked --test fixed_arrays`（15 passed）、`formats_fixed_arrays_and_length_parameters`、`--lib` の `type_stays_four_words`、
  GUIDE §3.1 の stack-depth テストが上限や stack を変えずに成功（`bounds_generic_lists_and_nested_types` は parser の分割の後も 2 MiB で成功）。
- suite `fixed_arrays` が native/WASM × `-O0`/`-O3` で成功（`live == 0`、WASM の import なし）。`TSUZURI_ASAN=1` でも成功。
- 手順 9（`[f64; 1024]` の値渡し）の翻訳時間: native `-O0` 0.34 s・`-O3` 1.07 s、wasm32 `-O0` 0.25 s・`-O3` 0.51 s。
- 手順 11: 既存の fixture と単一ファイルの例の IR（119 個）は `ff84e4c` と比べて 68 個が byte 一致、51 個は std の関数の追加による生成 id の一様なずれだけ
  （id を写す比較で差なし）。`tests/fixtures/fixed_arrays` の `-O3` の IR の `@tz.alloc` は 9 個（宣言 1、部分適用の環境 6、文字列の確保 2）で、
  `fixed_copy` などの固定長配列の複製には確保がない。
- 全体のゲート（fmt・clippy・`cargo test`・features の全 suite・`check-docs`）は 5 チケットの実装の後にまとめて実行した（F08 の記録を参照）。

### レビュー対応（PR #14）

- ISO C には長さ 0 の配列がないので、`[T; 0]` のフィールドを持つレコードの header は `T name[0];` という不正な C になっていた。
  `scalar_record` は長さ 1 以上の固定長配列だけを許し、`[T; 0]` のフィールドを持つレコードの export は他の公開できない型と同じ `E1008` にした
  （`tests/fixed_arrays.rs` に拒否と `[i64; 1]` の受理を追加）。
