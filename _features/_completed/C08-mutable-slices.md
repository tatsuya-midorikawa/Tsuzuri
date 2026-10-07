# C08: 可変スライスと要素のその場更新

| 項目 | 内容 |
| --- | --- |
| ID | C08 |
| 優先度 | P2 |
| 規模 | L |
| 依存 | C03, (A13) |
| 後続 | F08, F10, C11 |
| 状態 | done（Phase 1・2） |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | D1・D11 は、2026-10-06 に利用者から「C08、A14、A16、F13、F08 の実装をすべて完遂して。…複数フェーズある場合には、すべてのフェーズを完了させること」と依頼され、承認として扱った（GUIDE D-13・D-39） |
| 改善する劣位 | Rust 比: 可変スライスがない（[なぜ Tsuzuri か](../../_docs/learn/why-tsuzuri.md#rust-に対する劣位点)） |
| 手本にする既存実装 | 共有スライス（C03）: `src/parser.rs` の `index_or_slice`・`prefix`・`keyword_prefix`（`slice_context`）、`src/check.rs` の `slice`・`Type::shared_array_element`、`src/ownership.rs` の `eval_value` の `E::Slice` 分岐、`src/llvm.rs` の `array_slice`・`shared_array_deref`・`emit_place`。要素の書き込み: `src/llvm.rs` の `emit_typed_builtin` の `Builtin::ArraySet \| ArrayUpdate \| ArraySwap` 分岐と `checked_element_pointer`。排他参照の貸し直し: `src/check.rs` の `coerce_argument`・`reborrow_operand`・`require_mutable_reference`・`autoderef`。スタック上の値の移送: `src/llvm.rs` の `TypedExprKind::Borrow` 生成（`frame_of_place`・`relocate`） |
| 主な影響ファイル | 変更: `src/syntax.rs`, `src/parser.rs`, `src/formatter.rs`, `src/docgen.rs`, `src/semantic.rs`, `src/check.rs`, `src/ownership.rs`, `src/polymorph.rs`, `src/higher_kinds.rs`, `src/recursive.rs`, `src/regions.rs`, `src/llvm.rs`, `src/llvm_frame.rs`, `src/llvm_debug.rs`, `std/Array.tz`, `tests/formatter.rs`, `tests/features.mjs`, `docs/language.md`, `docs/architecture.md`, `_docs/library-reference/arrays-and-lists.md`, `_docs/library-reference/api/Array.md`, `_docs/language-reference/ownership.md`, `_docs/language-reference/lifetimes.md`, `_docs/learn/why-tsuzuri.md`, `_docs/feature-status.md`, `_features/README.md`, `_features/GUIDE.md`（§9 D-13・D-30。承認後）。新規: `tests/mutable_slices.rs`, `tests/fixtures/mutable_slices/Main.tz`。`src/warnings.rs`（`ty` の走査に arm、`Slice` に `..`）。コンパイルエラーに従って `..` を足すだけ: `src/computation.rs`。確認のみ（変更しない）: `src/control.rs`, `src/call_specialization.rs`, `src/abi.rs`, `std/Parallel.tz`, `tests/slices.rs`, `tests/arrays.rs`, `tests/storage.rs`, `tests/host_abi.rs` |

## 目的

`ref mut xs[a..b]` で配列の部分範囲を排他的に貸し、`Array.write` などでその場で要素を更新できるようにする（Rust の `&mut [T]` に相当）。
in-place sort、分割統治、ブロック単位の書き込みを、消費と再構築（`Array.set`）を繰り返さずに書けるようにする。
既存の `ref mut [T]`（配列全体の置換）、共有スライス `ref [T]`、消費的更新の意味と生成 IR は変えない。

実装者は Phase 1 だけを実装する。Phase 2（並列の分割書き込み。D11）は別途承認され、人間が求めた場合だけ着手する。

## 着手条件と停止条件

### 着手条件

- D1 が承認済みであること。人間が `## 決定事項` の D1 の `状態` を「承認済み」に書き換えたことで確かめる。承認前はどの手順にも着手しない。
- C03 が done であること。確認: `grep -n "| C03 " _features/README.md` の状態欄が done。
- A13 は必須ではない（「前提とする他チケットのインターフェース」）。A13 の状態にかかわらず手順は同じ。
- GUIDE §2.3 の基準コマンドが成功し、手順 1 のベースライン（既存 fixture の IR、テスト件数、stack-depth テスト）を保存していること。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- 「テスト計画」の拒否ケースが受理される（健全性の問題）。特に R5（範囲が違っても二つの排他スライスを同時に作れない）、R6（引数評価中の読み取り）、R17・R18（分割の半分と親の競合）。
- 半分ごとに別の loan を持たせたくなった、または `Loan`・`Value`・`Place`・`Use` の構造を変える必要が出た（D6。半分は一つの loan を共有する設計で進める）。
- 既存テストの期待値（診断コード、メッセージ断片、IR の断片）を変える必要が出た。本チケットが文を変える E0002・E1005 の 2 文（「診断」）は、HEAD でその文を検査するテストがないことを確認済みなので除く。
- 手順 1 で保存した既存 fixture（`slices`・`arrays`・`storage`・`computations`）の IR が変わる。
- `ExprKind` の大きさが増える、または stack-depth の 3 テストが失敗する。上限や stack サイズを上げたくなった。
- `ref mut [T]` と `ref mut [T..]` で具体化したジェネリック関数が同じシンボルになる（T8 が LLVM の重複定義で失敗する）。
- `frame_of_place` が `TypedExprKind::Dereference` を元とするスライスで空でない値を返す（移送の前提が違う）。
- `Array.sort_in_place` の結果が NaN を含まない入力で `Array.sort` と一致しない。期待値を変えて合わせない。
- `unsafe`、新しい crate、既定の WASM import、新しい `TrapKind` が必要になった。

## 現状（HEAD `f8dc655` で確認）

### 現在の構文

- `src/parser.rs` の `index_or_slice` は、`..` を含む添字を `slice_context` が真のときだけ `ExprKind::Slice { value, start, end }` にする。
  偽なら `E0002 "array slices must be shared borrows; write 'ref xs[start..end]' or '&xs[start..end]'"`、両端とも省略なら
  `E0002 "a slice needs at least one bound; borrow the whole array with 'ref xs'"`。
- `slice_context` は `bool`。`prefix` は共有の `&` のときだけ、`keyword_prefix` は共有の `ref` のときだけ真にする。
  `prefix` は結果が `ExprKind::Slice` なら `Borrow` で包まずにそのまま返す（span だけ広げる）。`ref mut`／`&mut` は常に偽なので、`ref mut xs[a..b]` は上の E0002 になる。
- 型の構文は `type_primary` の `[` 分岐が要素型の後に `]` を求める（`;` なら `[T; N]` 拒否の専用文）。`reference_type` は `ref mut {r} T` の region を読む。

### 型

- `Type::Array(Box<Type>)` と `Type::Reference(Box<Type>, bool)`。共有スライス `ref [T]` は `Reference(Array(T), false)` で、
  `Type::shared_array_element` がこれを判定する。`grep -n "shared_array_element()" src/*.rs` は 12 件（`src/check.rs` 2、
  `src/llvm_debug.rs` 4、`src/llvm_frame.rs` 1、`src/llvm.rs` 5）で、すべて表現（`%tz.array`、大きさ 16）の判定に使う。
  多くは `Type::Reference(_, false) if ty.shared_array_element().is_some()` の形の腕である。
- `src/check.rs` の `slice` は元を `autoderef` し、`Type::Array` でなければ `E1005 "a slice requires an array; lists and strings do not support array slicing"`。
  範囲は `i64` で型付けし、結果の型は `Reference(Array(T), false)`。`TypedExprKind::Slice { value, start, end }` に可変性はない。
- `ref mut [T]` は所有者の記述子の置き場所への `ptr` である。`deref r = v` は配列全体を置き換え、長さも変わってよい。
  次がこれを使う: `tests/fixtures/arrays/Arrays.tz` の `replace`、`tests/fixtures/storage/Storage.tz` の `replace`・`forward`、
  `tests/fixtures/computations/Optimization.tz` の `replace_values`、`tests/arrays.rs`、`tests/storage.rs`、`tests/slices.rs`
  （`shared_views_use_descriptors_and_copy_only_on_value_dereference` は `@tz.fn.Main.replace(ptr` を検査する）。
- `Validation::check` の `Array`／`List`／`Vec` 分岐は、要素が排他参照を含むと
  `E1005 "array and list elements cannot contain mutable references; collections are deeply immutable"`。
- `Type::is_copy` は `Reference(_, true)` を非 Copy、`can_capture` は `Reference(_, true)` を拒否、`can_send` は参照をすべて拒否する。
- `coerce_argument` は参照型の仮引数に対し、実引数の型が参照先と違えば `reborrow_operand`（参照なら `Dereference` を被せる）を通して
  `TypedExprKind::Borrow(value, mutable)` を作る。排他なら `require_mutable_reference` が最外の `Dereference` の参照が共有かを見て
  `E1014 "cannot mutate or exclusively reborrow through a shared reference"`。参照でない仮引数には `autoderef` を適用する。
- `autoderef` は `Type::Reference` を剥がして `Dereference` を重ねる。`src/control.rs` の `ExprKind::For` 分岐も `autoderef` の後に
  `Type::Array | List | Vec` から要素型を取る。

### 所有権

- `src/ownership.rs` の `eval_value` の `E::Slice` 分岐は、元の place を `Use::Borrow` で検査して共有 loan を作り、その値を `held` に積んだまま範囲を評価する。
- `Checker::access` は `Use::MutBorrow`／`Use::Write` で、via が空なら根 local の `mutable`、空でなければ via の全 loan の `mutable` を要求し、
  `place.fields` が空でなければ
  `E1014 "mutable access requires 'let mut' or an exclusive reference ('ref mut' or '&mut'); record fields, array elements, and list elements are immutable"`。
  via 以外の生きた loan と重なれば
  `E1014 "access conflicts with a live borrow; use the reference or end its last use before moving, replacing, or borrowing exclusively"`。
- `Checker::place` の `E::Index` 分岐は place に `ELEMENT` を足す（要素は保守的に全体と重なる）。したがって `ref mut xs[0]` は上の E1014 になる。
- `check_body` は借用を持つパラメーターに外部 loan を作り、`mutable = matches!(parameter.ty, Type::Reference(_, true))`。

### 生成

- `src/llvm.rs` の `llvm_type` は `Type::Reference(_, false) if ty.shared_array_element().is_some()` を `%tz.array`（`{ ptr, i64 }`）、他の参照を `ptr` にする。
- `array_slice` は「元の記述子 → start → end → `icmp ule start, end` と `icmp ule end, len` → `guard(.., TrapKind::BoundsCheck)` → `element_pointer`」の順。
- `emit_typed_builtin` の `ArraySet`／`ArrayUpdate`／`ArraySwap` 分岐は `checked_element_pointer`（`icmp ult` と BoundsCheck）の後で旧要素を
  `drop_value` して store する。`ArraySwap` は二つの添字を両方検査してから、等しければ何もしない。
- `TypedExprKind::Borrow` の排他の生成は、元がスタックフレーム上にあれば（`frame_of_place` が空でない）`relocate` でヒープへ移してから貸す
  （「借り手が置換・解放し得るため」）。`shared_array_deref`／`emit_expression_mode`／`emit_place` は共有スライスの `Dereference` を記述子のまま扱う。
- `std/Array.tz` の `sort :: Ord<'a> => ref ['a] -> ['a]` は `Array.sort_by`（`Builtin::ArraySortBy`）で安定整列する。`Array.set`／`update`／`swap` は所有配列を消費する。

### 再現（検証済み）

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
mkdir -p /tmp/tz-c08/now
printf 'export def f :: i64\nfn f =\n    let mut values = [1, 2, 3, 4]\n    let part = ref mut values[1..3]\n    part.length\n' > /tmp/tz-c08/now/Main.tz
target/release/tsuzuri check /tmp/tz-c08/now
```

期待: `error[E0002]: array slices must be shared borrows; write 'ref xs[start..end]' or '&xs[start..end]'`、終了コード 1。
同じ手順で、`def g :: ref mut [i64..] -> i64` は `E0002 expected ']'`、`Array.write (ref mut values) 0 5` は
`E1002 module 'Array' has no function or union case 'write'`、`ref mut values` を `task` に捕捉すると
`E1013 tasks require owned values; ref mut [i64] contains a reference`、ラムダに捕捉すると
`E1005 cannot capture ref mut [i64] in a reusable function; fully apply exclusive borrows and keep single-use tasks in task blocks` になる。

既存構文（検証済み。`check` が出力なしで成功する）:

```tsuzuri
def replace :: ref mut [i64] -> unit
fn replace values = deref values = [7, 8]

export def f :: i64
fn f =
    let mut values = [1, 2, 3, 4]
    replace (ref mut values)
    let first = ref values[0..1]
    values.length * 10 + first[0]
```

## 仕様

### GUIDE §9 D-13 の変更（D1）

現在の文（HEAD の `_features/GUIDE.md`）:

```text
- 部分参照（スライス）は **`&[T]` そのもの**（C03）。`&xs[a..b]` で作る。`&mut [T]` は従来どおり配列全体の置換用。
```

承認後の文（手順 13 で置き換える）:

```text
- 部分参照（スライス）は共有が `&[T]`（C03）、排他が `&mut [T..]`（C08）。`&xs[a..b]`／`&mut xs[a..b]` で作る。
  `&mut [T]` は配列全体の置換用のままで、呼び出しの引数では `&mut [T..]` へ変換できる。
  コレクションは共有されている間は不変で、排他スライスを通してだけ `Array.write`／`Array.swap_in`／`Array.sort_in_place` で要素をその場で置換できる。
  要素単位の排他借用（`&mut xs[i]`）と代入構文（`xs[i] = v`）は導入しない。
```

変わること: 新しい型 `ref mut [T..]`、排他スライスを通したその場の要素更新、`ref mut [T]` から `ref mut [T..]` への引数変換、
docs の「コレクションは深く不変」の記述。変わらないこと: `ref mut [T]` の表現（`ptr`）と全体置換、`ref [T]`、`Array.set` などの消費的更新、既存プログラムの判定・結果・IR。

### 前提とする他チケットのインターフェース

- C03（done）: `ref [T]` は `%tz.array` 記述子、`ref xs[a..b]` は `TypedExprKind::Slice`。本チケットはこれを再利用し、共有の経路は変えない。
- A13（任意）: 本チケットは A13 に依存しない。`ref mut [T..]` は内部で `Type::Reference(_, true)` なので、A13 が done なら
  `record Cursor {r} { part: ref mut {r} [i64..], position: i64 }` は A13 の規則でそのまま許され、A13 が todo なら既存の
  `E1013 "record fields cannot store mutable references; use a shared borrow"` のまま。本チケットのテストは record への格納を検査しない。
- 後続へ提供する（C11・F08・F10）: 型 `ref mut [T..]`、`Array.write`／`Array.swap_in`／`Array.split_at_mut`／`Array.sort_in_place`、
  `Type::ArrayView`（新規）と `Type::slice_element`（新規）。F04 の文書にある `Simd.store : &mut [T] -> ...` は `&mut [T..]` と読み替える。

### 構文

```text
type_primary ::= ... | ("ref" | "&") "mut" region? "[" type ".." "]"     (* 新規: 排他スライス型 ref mut [T..] *)
slice        ::= ("ref" | "&") postfix "[" bound? ".." bound? "]"          (* 既存: 共有スライス *)
               | ("ref" "mut" | "&" "mut") postfix "[" bound? ".." bound? "]" (* 新規: 排他スライス *)
bound        ::= expression                                                (* 両端のうち少なくとも一方が必要 *)
```

- `[T..]` は `ref mut`（または `&mut`、region 付き `ref mut {r}`）の直後だけに書ける。
- 範囲の開始の省略は `0`、終了の省略は元の長さ。`ref mut xs[..]` は E0002（配列全体は `ref mut xs[0..]`）。
- `ref mut xs` は従来どおり `ref mut [T]`（所有者への参照）で、排他スライスにはならない。

### 型規則

1. `ref mut [T..]` は排他スライス型。内部表現は `Type::Reference(Box::new(Type::ArrayView(T)), true)`、表示は `ref mut [T..]`。
   `Type::ArrayView` は参照先としてだけ現れ、値・束縛・フィールド・要素の型にはならない。`Reference(ArrayView(_), false)` は作らない。
   非 Copy・Capture 不可・Send 不可・export 不可（E1008）は `ref mut T` と同じ既存規則による。
2. `ref mut e[a..b]` の元 `e` は、参照を剥がした後に次のどれか。`let mut` の配列 local（`[T]`）、`ref mut [T]` の参照先、`ref mut [T..]` の参照先。
   剥がす途中で共有参照（`ref [T]` を含む）を通れば E1014（既存文）。配列でなければ既存の E1005。place でない一時値は E1014 M4。
   record フィールドや要素（`ref mut r.items[0..]`、`ref mut grid[0][1..]`）は既存の `access` の規則で E1014。結果は `ref mut [T..]`。
3. `ref s[a..b]`（`s: ref mut [T..]`）は共有スライス `ref [T]` を作る。
4. `s: ref mut [T..]` を通した読み取り（`s[i]`、`ref s[i]`、`s.length`、`for x in s do`、`deref s`／`*s` の値としての読み取り、
   `ref [T]` の仮引数への受け渡し）は、`s` の共有の貸し直し `ref s` を通した読み取りと同じ型・意味になる。`deref s` は共有スライスと同じく複製を返す。
5. 貸し直し: `ref mut s`、`&mut *s`、呼び出し時の暗黙の貸し直しは `ref mut [T..]` を返す。`ref s`、`&*s` は `ref [T]` を返す。
6. 呼び出しの実引数だけで、次の変換を行う（`let` の型注釈や返却値では変換しない）。

| 仮引数の型 | 実引数の型 | 結果 |
| --- | --- | --- |
| `ref mut [T..]` | `ref mut [T..]` | 貸し直し（既存の `coerce_argument` の経路） |
| `ref mut [T..]` | `ref mut [T]`（所有者への参照） | 配列全体の排他スライス（範囲なしの `Slice`） |
| `ref mut [T..]` | `[T]` の place（`let mut xs` など） | 配列全体の排他スライス（`ref mut xs[0..]` と同じ） |
| `ref mut [T..]` | `ref [T]` | E1014（既存文 `cannot mutate or exclusively reborrow through a shared reference`） |
| `ref mut [T..]` | place でない `[T]` | E1014 M4 |
| `ref [T]` | `ref mut [T..]` | 共有の貸し直し（範囲なしの `Slice`） |
| `ref mut [T]` | `ref mut [T..]` | E1005 M2（全体置換の仮引数へ部分を渡せない） |

7. `deref s = v`／`*s = v`（`s: ref mut [T..]`）は E1014 M3。`ref mut s[i]`・`ref mut xs[i]` は既存の E1014 のまま。
8. 単一化では `[T..]` と `[T]` は別の型構成子。型変数 `'a` は `ref mut [T..]` で具体化できる。

### 標準ライブラリ API

新 API（実装後に有効。未検証）。`write`・`swap_in`・`split_at_mut` は組み込み（`Builtin`、`std/Array.tz` に `def` を書かない。`Array.set` と同じ）、
`sort_in_place` は `std/Array.tz` の Tsuzuri 関数。

```tsuzuri
def write :: ref mut ['a..] -> i64 -> 'a -> unit
def swap_in :: ref mut ['a..] -> i64 -> i64 -> unit
def split_at_mut :: ref mut ['a..] -> i64 -> (ref mut ['a..] * ref mut ['a..])
def sort_in_place :: Ord<'a> => ref mut ['a..] -> unit
```

| API | 意味 | 検査と順序 | 費用 |
| --- | --- | --- | --- |
| `Array.write s i v` | `s` の `i` 番目を `v` に置き換え、旧要素を drop する | 引数を左から右に評価 → `0 <= i < len`（`icmp ult`） → 旧要素の drop → store。トラップ時は何も書かない | O(1)、確保なし |
| `Array.swap_in s i j` | 二要素を交換する | 引数評価 → `i` と `j` の両方を検査 → 等しければ何もしない → 交換 | O(1)、確保なし |
| `Array.split_at_mut s m` | `[0, m)` と `[m, len)` の二つの排他スライス | 引数評価 → `m <= len`（`icmp ule`、負はトラップ） | O(1)、確保なし |
| `Array.sort_in_place s` | 安定な昇順整列。結果は `Array.sort (ref s)` と要素ごとに一致する | 比較は `Ord.lt` だけ。要素は `swap_in` でだけ動かす | 比較 O(n log n)、交換 O(n log² n)、確保なし、再帰の深さ O(log n) |

### 評価順序・所有権・借用

- 作成の実行順序は共有スライスと同じ（元の記述子 → start → end → 検査 → view）。所有権検査は二相で行う（D8）。
  範囲の評価中は元に共有の guard loan を保持し（元の読み取りは可、書き込み・move・排他借用は E1014）、範囲の評価後に元へ `Use::MutBorrow` の検査をして排他 loan を作る。
  したがって `ref mut xs[1..xs.length - 1]` は受理し、`ref mut xs[{ xs = [3]; 0 }..1]` は E1014。
- 排他 loan は元の place に作り、parents は元の loan（`Checker::loan` が根 local の loan を加える既存の規則）。スライスが生きている間、元へのアクセスはすべて E1014。
  最後の使用の後は元を再び使える（既存の NLL）。範囲が重ならない二つの `ref mut xs[..]` も同時には作れない（E1014。検査器は範囲を比べない）。
- 分割（D6）: `Array.split_at_mut s m` の結果は、実引数の暗黙の貸し直しの loan（`s` の loan の子）を一つ持ち、二つの半分はその同じ loan を共有する。
  重なりがないことは実行時の構成（`m <= len` と `[0, m)`／`[m, len)`）だけが保証し、検査器は半分を区別しない。その結果、
  両方の半分を同時に保持し、交互に書き込める。親 `s` は半分が生きている間 E1014。一方の半分から作った借用（`ref left[0..1]`、`ref mut left[..]`、
  `Array.split_at_mut left ..` の結果）が生きている間は、もう一方の半分へのアクセスも E1014（安全側の偽陽性）。
  呼び出しの実引数の評価中は先の実引数の貸し直しが生きているので、`Array.write right 0 left[0]` は E1014。値を先に `let` へ束縛する。
- Task と関数値: `ref mut [T..]` は既存の規則で task へ渡せず（E1013）、再利用可能な関数値に捕捉できない（E1005）。
  `Parallel` による互いに素なチャンクへの並列書き込みは Phase 2（D11）。
- `Vec`: `Vec` の要素範囲の排他スライスは作らない（既存の E1005 のまま）。将来許す場合も、スライスの loan が生きている間は `Vec.push` など
  所有値を消費する操作が E1014 になるので、再確保による別名は生じない。
- Copy 配列: `let b = a` の後で `a` を使うなら既存の `clones_on_take` の規則で複製されるので、`a` の排他スライスへの書き込みは `b` に見えない。
  `Array.set` の一意バッファ再利用は所有値の消費だけで起き、スライスの生存中は元を消費できない（E1014）ので両者は混ざらない。
- スタックフレーム: 排他スライスを作るとき、元の place が `frame_of_place` を持てば、`ref mut` と同じく `relocate` でヒープへ移してから view を作る（D13）。
  `Array.write` が旧要素を drop するので、フレーム上の要素を `tz.free` しないためである。
- 反復: `for x in s do` は共有の貸し直しを通した反復で、本体の中で `s` へ書き込むと E1014。書き込む反復は添字の `while` と `Array.write` で書く。

### 数値・トラップ・native と WASM の差

- 検査はすべて符号なし比較（`icmp ult`／`icmp ule`）で、負の添字・位置もトラップする。トラップは既存の `TrapKind::BoundsCheck` だけ。新しいトラップ種別は作らない。
- native と WASM で同じ IR の形。ランタイム関数・WASM import は増えない。浮動小数点の演算はない。
- `sort_in_place` は `Ord` の比較だけを使う。`Ord` が strict weak order でない値（例: 比較が一貫しない独自インスタンス）では `Array.sort` との一致を保証しない。

### 診断

M1〜M5 は本チケットで新しく作る（または文を変える）メッセージ。他は既存の文をそのまま使う。

| コード | 条件 | メッセージ | 位置 |
| --- | --- | --- | --- |
| E0002 | 借用なしの範囲添字 `xs[a..b]`（既存条件。文を変える: M1） | `array slices must be borrows; write 'ref xs[start..end]' for a shared slice or 'ref mut xs[start..end]' for an exclusive slice` | 添字式全体 |
| E0002 | 両端とも省略した排他スライス `ref mut xs[..]`（共有の同条件の文は変えない） | `a slice needs at least one bound; write 'ref mut xs[0..]' for an exclusive slice of the whole array` | 添字式全体 |
| E1005 | `[T..]` が `ref mut` の直後以外（`[T..]`、`ref [T..]`、`[[T..]]`、`Vec<[T..]>`） | `'[T..]' is only valid directly after 'ref mut'; write 'ref mut [T..]' for an exclusive slice, 'ref [T]' for a shared slice, or '[T]' for an owned array` | `[T..]` の型式 |
| E1005 | `ref mut [T..]` を `ref mut [T]` の仮引数へ渡す（M2） | `an exclusive slice cannot be passed as 'ref mut [T]' because that parameter may replace the whole array; declare the parameter as 'ref mut [T..]'` | 実引数 |
| E1005 | 配列・リスト・Vec の要素が排他参照を含む（既存条件。文の後半を変える: M5） | `array and list elements cannot contain mutable references; collections cannot hold exclusive borrows, so keep exclusive slices in locals or split them with 'Array.split_at_mut'` | 既存と同じ |
| E1005 | 排他スライスの元が配列でない（既存） | `a slice requires an array; lists and strings do not support array slicing` | 添字式全体 |
| E1005 | 再利用可能な関数値による捕捉（既存） | `cannot capture ref mut [i64..] in a reusable function; fully apply exclusive borrows and keep single-use tasks in task blocks` | 関数値 |
| E1008 | `ref mut [T..]` を含む export（既存の非公開型の規則） | 既存の文のまま | 既存と同じ |
| E1012 | move した排他スライスの使用（既存） | `use of moved or partially moved value '{name}'` | 使用箇所 |
| E1013 | スライスが元の寿命を超える（既存） | `cannot return a reference to a local value` | 本体 |
| E1013 | task への受け渡し（既存） | `tasks require owned values; ref mut [i64..] contains a reference` | task 式 |
| E1014 | 共有参照・共有スライスを通して排他スライスを作る、または `ref mut [T..]` の仮引数へ `ref [T]` を渡す（既存文） | `cannot mutate or exclusively reborrow through a shared reference` | 添字式または実引数 |
| E1014 | `let mut` でない配列、record のフィールド、要素から作る（既存） | `mutable access requires 'let mut' or an exclusive reference ('ref mut' or '&mut'); record fields, array elements, and list elements are immutable` | 添字式 |
| E1014 | 生きている借用との衝突（既存） | `access conflicts with a live borrow; use the reference or end its last use before moving, replacing, or borrowing exclusively` | 衝突したアクセス |
| E1014 | `deref s = v`／`*s = v`（M3） | `cannot replace an exclusive slice as a whole because its length is fixed; write elements with 'Array.write' or replace the array through its owner ('ref mut [T]')` | 代入式 |
| E1014 | place でない配列から排他スライスを作る（M4） | `an exclusive slice must borrow a named array; bind the value with 'let mut' first` | 添字式または実引数 |

### 資源上限

解析の上限は変えない。`sort_in_place` の再帰の深さは O(log n)（n = 2^40 でも数十段）で、既存の実行時スタックで足りる。

### 例

新構文・新 API（実装後に有効。未検証）。受理され、`total` は 63 になる（`tail` は `[2, 3, 4]` → `[20, 3, 4]` → `[20, 4, 3]` → `[20, 40, 3]`）。

```tsuzuri
def rec negate_all :: ref mut [i64..] -> unit
fn rec negate_all values =
    let length = Array.length values
    if length == 1 then
        let value = values[0]
        Array.write values 0 (0 - value)
    else if length > 1 then
        let (left, right) = Array.split_at_mut values (length / 2)
        negate_all left
        negate_all right

export def example :: i64
fn example =
    let mut values = [1, 2, 3, 4]
    let tail = ref mut values[1..]
    Array.write tail 0 20
    Array.swap_in tail 1 2
    let scaled = tail[1] * 10
    Array.write tail 1 scaled
    let mut total = 0
    for value in tail do total = total + value
    total
```

`scaled` を先に束縛する。`Array.write tail 1 (tail[1] * 10)` は先の実引数の貸し直しが生きているので E1014（R6）。E2E の期待値は「テスト計画」の表を使う。

拒否される例（新構文。未検証）:

```tsuzuri
let mut values = [1, 2, 3]
let left = ref mut values[0..1]
let right = ref mut values[1..3]
Array.write left 0 5
right.length
```

期待: 3 行目で E1014（範囲が重ならなくても、`left` の生存中に二つ目は作れない。`Array.split_at_mut` を使う）。

## 設計

### データ構造

```rust
// src/syntax.rs
TypeExprKind::ArrayView(Box<TypeExpr>)             // 新規: `[T..]`。位置の検査は resolve_type（E1005）
ExprKind::Slice { value, start, end, mutable: bool } // mutable を追加
// src/parser.rs の Parser（bool から変更。None: 借用の外、Some(false): ref/&、Some(true): ref mut/&mut）
slice_context: Option<bool>,
// src/check.rs
Type::ArrayView(Box<Type>)                         // 新規: Reference(ArrayView(T), true) の形でだけ現れる
pub(crate) fn slice_element(&self) -> Option<&Type> // 新規: Reference(Array(T), false) と Reference(ArrayView(T), true) の T
Builtin::{ArrayWrite, ArraySwapIn, ArraySplitAtMut} // 新規
```

- `TypedExprKind::Slice { value, start, end }` は変えない。可変性は式の型（`Type::Reference(_, true)`）で区別する。両端 `None` の `Slice` は引数変換（型規則 6）だけが作る。
- `Type` の大きさは変わらない。`ExprKind` が大きくなれば停止条件。`std/Array.tz` の helper（新規）: `sort_in_place_insertion`、`sort_in_place_merge`、`sort_in_place_rotate`。

### 段ごとの変更

GUIDE §6.3 と型式の追加の手順を HEAD に当てはめた一覧。`grep -n "TypeExprKind::Array\|Type::Array(" src/*.rs` の全箇所で `ArrayView` が `_ =>` に落ちないことを確かめる。

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| 構文 | `src/syntax.rs` | `TypeExprKind`、`ExprKind::Slice`、`Slice` の子を列挙する match | `ArrayView` と `mutable`。子の列挙は `..` を足すだけ |
| 構文 | `src/parser.rs` | `type_primary` の `[` 分岐 | 要素型の後が `..` なら `]` を求めて `TypeExprKind::ArrayView`。`;` の既存の拒否は変えない |
| 構文 | `src/parser.rs` | `index_or_slice`、`prefix`、`keyword_prefix` | `prefix`・`keyword_prefix` は `ref mut`／`&mut` でも `slice_context = Some(true)` とし、結果が `Slice` なら `Borrow` で包まない（HEAD の `shared &&` 条件を外す）。`index_or_slice` は保存・復元を保ち、`None` なら M1、`Some(true)` で両端省略なら排他用の E0002、`Slice` に `mutable` を入れる |
| 整形 | `src/formatter.rs` | `ty`、式の `Slice` 分岐 | `[T..]`。`mutable` なら共有の `ref ` と同じ位置に `ref mut ` |
| 文書生成 | `src/docgen.rs` | `type_text` | `ArrayView(e) => format!("[{}..]", type_text(e))` |
| 型式の走査 | `src/check.rs`（型別名の `expand`・`substitute`・走査 2 か所）、`src/polymorph.rs`、`src/regions.rs`、`src/semantic.rs`、`src/warnings.rs` の `ty` | `TypeExprKind::Array(inner)` を含む match | `ArrayView(inner)` を `Array(inner)` と同じ arm に足す |
| 式の走査 | `src/computation.rs`、`src/warnings.rs` | `ExprKind::Slice` の分岐 | `..` を足すだけ |
| 検査 | `src/check.rs` | `Type`、`Type::display`、`Type::slice_element`（新規） | `ArrayView`。表示は `[T..]`（`ref mut ` は `Reference` 側が出す） |
| 検査 | `src/check.rs` | `is_copy`、`needs_drop`、`contains_reference`、`contains_mutable_reference`、`contains_stored_reference`、`carries_loans`、`can_capture`、`exportable`、`can_send` | 変更なし（`Reference(_, true)` の arm が先に決まる） |
| 検査 | `src/check.rs` | `resolve_type` | `Reference(inner, true)` の inner が `ArrayView(e)` なら `Reference(ArrayView(e'), true)`。他の位置の `ArrayView` は E1005（`[T..]` の文） |
| 検査 | `src/check.rs` | `Validation::check` | `Array`／`List`／`Vec` 分岐の文を M5 に変え、`ArrayView` を同じ arm に足す |
| 検査 | `src/check.rs` | `autoderef`、`ExprKind::Dereference` の分岐、`dereferenced_type`（新規） | `Reference(ArrayView(T), _)` を剥がした `Dereference` の型は `Array(T)`。読み取り（添字、`length`、`for`、値としての `deref`）を共有スライスと同じ経路に通す |
| 検査 | `src/check.rs` | `ExprKind::Slice` の分岐、`slice` | `mutable` を渡す。排他なら `require_mutable_reference`、元が place でなければ M4、型は `Reference(ArrayView(T), true)` |
| 検査 | `src/check.rs` | `ExprKind::Borrow` の分岐 | 被演算子が排他スライスの `Dereference` か排他スライス型の値なら型規則 5（`ref mut` は view、`ref` は `ref [T]`）。後者は `reborrow_operand` を被せる |
| 検査 | `src/check.rs` | `ExprKind::Assign` の分岐 | place が排他スライスの `Dereference` なら M3 |
| 検査 | `src/check.rs` | `coerce_argument`、`coerce_slice_argument`（新規） | 型規則 6 の表。`coerce_argument` からは 1 行で呼ぶ（stack の深さ） |
| 検査 | `src/check.rs` ほか | `Builtin` と、その名前・型・分類の表（`grep -n "ArraySwap" src/*.rs` の全箇所） | 新しい 3 つを `ArraySwap` に並べる。型は「標準ライブラリ API」 |
| 型推論 | `src/polymorph.rs` | `map_type`、`substitute`、`unify`、`type_expression` | `ArrayView` を `Array` と同形で扱う。`unify` で `ArrayView` と `Array` は不一致 |
| 型推論 | `src/higher_kinds.rs`、`src/recursive.rs` | `Type::Array(` を含む match | 要素へ降りる arm に `ArrayView` を足す。参照の内側に降りない match は変えない |
| 所有権 | `src/ownership.rs` | `eval_value` の `E::Slice` 分岐 | 型が `Reference(_, true)` なら二相（「アルゴリズム」）。共有の経路は変えない |
| 生成 | `src/llvm.rs` | `llvm_type`、`canonical_type`、`storage_layout` | `Reference(..) if ty.slice_element().is_some()` は `%tz.array`。`canonical_type` の `ArrayView(e)` は `view[{e}]`（`refmut[view[..]]` と `refmut[array[..]]` を区別）。`llvm_type(ArrayView)` は `unreachable!` |
| 生成 | `src/llvm.rs`、`src/llvm_frame.rs`、`src/llvm_debug.rs` | `shared_array_element()` の 12 箇所 | すべて `slice_element()` に置き換える（表現の判定だけに使われている） |
| 生成 | `src/llvm.rs` | `emit_expression_mode` の `TypedExprKind::Slice` 分岐 | 型が排他で `frame_of_place(value)` が空でなければ、`TypedExprKind::Borrow` の排他分岐と同じ呼び方で `relocate` してから `array_slice` |
| 生成 | `src/llvm.rs` | `emit_typed_builtin` | `ArrayWrite` は `ArraySet` と同じ検査・drop・store を記述子に対して行う（結果は `unit`）。`ArraySwapIn` は `ArraySwap` と同じ。`ArraySplitAtMut` は新しい分岐 |
| 標準 | `std/Array.tz` | `sort_in_place` と helper 3 つ（新規） | 「アルゴリズム」 |

### 生成 IR とランタイム

- `ref mut [T..]` は `%tz.array`（`{ ptr, i64 }`）の値で、共有スライスと同じく値渡しにする。data ポインターと長さは作成後に変わらない。
- 作成は共有スライスと同じ `array_slice`。元がフレーム上の local なら、その前に `relocate` する（D13）。
- `Array.write s i v` の形（要素 `i64`、native。旧要素の drop は `needs_drop` の型だけ）:

```llvm
%len = extractvalue %tz.array %s, 1
%ok = icmp ult i64 %i, %len
; guard(%ok, TrapKind::BoundsCheck)
%data = extractvalue %tz.array %s, 0
%slot = getelementptr i64, ptr %data, i64 %i
; needs_drop の型なら: 旧値を load して drop_value
store i64 %v, ptr %slot
```

- `Array.split_at_mut s m`: `icmp ule i64 %m, %len` → BoundsCheck → `{ %data, %m }` と `{ gep %data %m, %len - %m }` を `{ %tz.array, %tz.array }` に詰める。
- ランタイム関数、`%tz.*` の型、WASM import は増えない。本チケットの構文を使わないプログラムの IR は byte 単位で変わらない（手順 11）。

### アルゴリズム

所有権の二相検査（D8）。`guard` は locals にも `held` にも残らないので、pop の後は `active` に入らず二相目と衝突しない。

```text
E::Slice { value, start, end } で expression.ty が Reference(_, true):
  places = self.place(value, &during)?
  guard = result.clone()                       -- loan を持たない空の値
  for (place, via) in places: access(place, via, Use::Borrow); guard.loans.insert(loan(place, false, via))
  held.push(guard); for bound in start, end: eval(bound, Use::Consume, &during)?; held.pop()
  for (place, via) in places: access(place, via, Use::MutBorrow); result.loans.insert(loan(place, true, via))
```

`sort_in_place`（D10。Go の `sort.Stable` と同じ構成）。`lt(x, y)` は `Ord.lt (ref s[x]) (ref s[y])`、`swap(x, y)` は `Array.swap_in s x y`。中点は `lo + (hi - lo) / 2`。

```text
sort_in_place s: n = length s; block = 20; a = 0
  while a + block <= n: insertion(a, a + block); a = a + block
  insertion(a, n)
  while block < n:
    a = 0
    while a + 2 * block <= n: merge(a, a + block, a + 2 * block); a = a + 2 * block
    if a + block < n: merge(a, a + block, n)
    block = block * 2
insertion(a, b): for i in a + 1 .. b - 1: j = i; while j > a && lt(j, j - 1): swap(j, j - 1); j = j - 1
merge(a, m, b):                                -- sort_in_place_merge（def rec）
  if m - a == 1: i = 最初の i in [m, b) で !lt(i, a)（二分探索）; for k = a .. i - 2: swap(k, k + 1); return
  if b - m == 1: i = 最初の i in [a, m) で lt(m, i)（二分探索）; for k = m downto i + 1: swap(k, k - 1); return
  mid = a + (b - a) / 2; n = mid + m
  (start, r) = if m > mid then (n - b, mid) else (a, m)
  p = n - 1; while start < r: c = start + (r - start) / 2; if !lt(p - c, c) then start = c + 1 else r = c
  end = n - start
  if start < m && m < end: rotate(start, m, end)
  if a < start && start < mid: merge(a, start, mid)
  if mid < end && end < b: merge(mid, end, b)
rotate(a, m, b): i = m - a; j = b - m      -- sort_in_place_rotate
  while i != j: if i > j then (swap_range(m - i, m, j); i = i - j) else (swap_range(m - i, m + j - i, i); j = j - i)
  swap_range(m - i, m, i)                   -- swap_range(x, y, k): 0 <= t < k で swap(x + t, y + t)
```

安定性は二分探索の境界（`!lt(i, a)` と `lt(m, i)`）で決まる。境界を入れ替えると `Array.sort` と一致しない（停止条件）。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので `running N tests` を必ず見る（GUIDE §3.1）。「stack-depth 3 テスト」は手順 1 の `cargo test` の最初の 3 行。

### 手順 1: ベースラインを取る

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンド。既存 fixture 4 つを別々の一時 root に写して IR を保存する（E03）。失敗するものは中の `.tz` を 1 つずつ指定する。「再現」の 5 診断も確かめる。
- 確認: すべて成功し、4 つの `cargo test` はどれも `1 passed` を含む。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
mkdir -p /tmp/tz-c08/base
for name in slices arrays storage computations; do
  rm -rf /tmp/tz-c08/base/$name && cp -R tests/fixtures/$name /tmp/tz-c08/base/$name
  target/release/tsuzuri build /tmp/tz-c08/base/$name --emit llvm -o /tmp/tz-c08/base/$name.ll
  target/release/tsuzuri build /tmp/tz-c08/base/$name --emit llvm -O3 -o /tmp/tz-c08/base/$name-O3.ll
done
cargo test --locked --test polymorphism bounds_type_growing_polymorphic_recursion
cargo test --locked --lib bounds_recursive_and_flat_expression_depth
cargo test --locked --test computations bounds_nested_builder_expansion_not_just_source_syntax
cargo test --locked honors_the_exact_specialization_limit
```

### 手順 2: `Type::ArrayView` と `slice_element`（共有変更）

- 変更: 「段ごとの変更」の `Type`・`display`・`slice_element`、型推論の 2 行、`llvm_type`・`canonical_type`・`storage_layout`、`shared_array_element()` の 12 箇所。
- 内容: variant を足すが、まだどこでも作らない。12 箇所を `slice_element()` に置き換える。`shared_array_element` は他に使われなくなれば削除する。
- 確認: `cargo test --locked` が成功する。手順 1 の for 文を出力先 `/tmp/tz-c08/after` で実行し、`for f in /tmp/tz-c08/base/*.ll; do cmp "$f" "/tmp/tz-c08/after/${f##*/}"; done` が何も出さない。

### 手順 3: 型式 `[T..]` と読み取り

- 変更: `TypeExprKind::ArrayView`、`type_primary`、`ty`（formatter）、`type_text`、型式の走査の全 match、`resolve_type`、`Validation::check`（M5）、
  `autoderef`・`ExprKind::Dereference` の分岐・`dereferenced_type`、`tests/mutable_slices.rs`（新規）。
- 内容: テストファイルは `tests/slices.rs` と同じ import と書き方にする。この段では仮引数 `ref mut [T..]` を受け取って読むだけの関数が書ける。
- 確認: `cargo test --locked --test mutable_slices` が `1 passed`（`exclusive_slice_types_resolve_only_after_ref_mut`: T1、R1、R2、R21）。`cargo test --locked` が成功する。

### 手順 4: 排他スライスの作成

- 変更: `ExprKind::Slice` の `mutable`、`slice_context`、`index_or_slice`・`prefix`・`keyword_prefix`、formatter の `Slice`、`src/computation.rs`・`src/warnings.rs` の `..`、
  `slice`、`eval_value` の `E::Slice`（二相）、`emit_expression_mode` の `Slice`（`relocate`）、`tests/formatter.rs` の `formats_exclusive_slices`（新規）。
- 内容: M1 と排他用 E0002 の文は「診断」の表から写す。所有権は必ず同じ手順で入れる（作成だけ先に入れると R5 が受理される）。
- 確認: `cargo test --locked --test mutable_slices` が `2 passed`（`exclusive_slices_borrow_named_mutable_arrays`: T2〜T4、R3〜R5、R7〜R9、R13、R14）、
  `cargo test --locked --test formatter formats_exclusive_slices` が `1 passed`、`cargo test --locked --test slices` が `3 passed`、stack-depth 3 テストが成功。

### 手順 5: 貸し直しと代入の拒否

- 変更: `ExprKind::Borrow` と `ExprKind::Assign` の分岐（型規則 5・7、M3）。
- 確認: `3 passed`（`exclusive_slices_reborrow_and_read_like_shared_slices`: T5、R10、R15、R16、R19、R20、R22、R23）。`cargo test --locked` が成功する。

### 手順 6: 呼び出しの引数の変換

- 変更: `coerce_argument`、`coerce_slice_argument`（新規。M2）。
- 内容: 型規則 6 の表の 7 行を上から順に判定する。`ref mut [T]` の実引数からの変換は `Slice { value: Dereference(実引数), start: None, end: None }`。
- 確認: `4 passed`（`call_arguments_convert_to_exclusive_slices`: T6〜T8、R11、R12）。stack-depth 3 テスト（特に `bounds_nested_builder_expansion_not_just_source_syntax`）が成功する。

### 手順 7: `Array.write` と `Array.swap_in`

- 変更: `Builtin::ArrayWrite`・`ArraySwapIn` と名前・型の表、`emit_typed_builtin`。
- 内容: `write` は `ArraySet` の分岐から「検査 → 旧要素の drop → store」を切り出して共用する。記述子の置き場所は書き換えない。
- 確認: `5 passed`（`in_place_writes_check_bounds_before_drop_and_store`: T9、R6、IR で `icmp ult` → drop → `store` の順）。`cargo test --locked --test consuming_update` が成功する（`Array.set` の経路を壊していない）。

### 手順 8: `Array.split_at_mut`

- 変更: `Builtin::ArraySplitAtMut`、`emit_typed_builtin` の新しい分岐。
- 内容: 結果の値が実引数の貸し直しの loan を持つことを R17・R18 で確かめる。持たなければ停止条件。
- 確認: `6 passed`（`split_halves_share_one_loan`: T10、R17、R18）。

### 手順 9: `Array.sort_in_place`

- 変更: `std/Array.tz`（`sort_in_place` と helper 3 つ）。
- 内容: 「アルゴリズム」を写す。helper は `def rec`（`sort_in_place_merge`）と通常の `def`。`std/Array.tz` の既存 `def` と同じ書き方にする。
- 確認: `7 passed`（`sort_in_place_specializes_only_when_used`: T11）、`cargo test --locked honors_the_exact_specialization_limit` が `1 passed`。

### 手順 10: E2E suite `mutable_slices`

- 変更: `tests/fixtures/mutable_slices/Main.tz`（新規）、`tests/features.mjs`（GUIDE §7.4 の手順で suite を足す）。
- 確認: `cargo build --release --locked && node tests/features.mjs target/release/tsuzuri mutable_slices` で E2E 表の全ケースが native/WASM × `-O0`/`-O3` で成功。

### 手順 11: 既存 IR の不変と生成コード

- 内容: 手順 2 の `cmp` を繰り返す。fixture の IR（`-O3`）の `fill` と `negate_all` に `call ptr @tz.alloc` がなく、`Array.write` 1 回に `icmp ult` が 1 回あることを確かめ、Rust テスト `exclusive_slice_ir_is_deterministic_and_allocation_free` を足す。
- 確認: `cmp` が何も出さない。`cargo test --locked --test mutable_slices` が `8 passed`。

### 手順 12: ドキュメント

- 変更: 「ドキュメント」の全ファイル。
- 確認: `node scripts/check-docs.mjs _docs/library-reference/arrays-and-lists.md _docs/language-reference/ownership.md _docs/language-reference/lifetimes.md _docs/learn/why-tsuzuri.md _docs/feature-status.md` が成功する。

### 手順 13: GUIDE と最終確認

- 変更: `_features/GUIDE.md` §9 D-13（「仕様」の承認後の文に置き換える）と D-30（承認待ちの一覧から C08 を外す）、`_features/README.md` の状態。
- 確認: `cargo fmt --check`、`cargo clippy --locked --all-targets -- -D warnings`、`cargo test --locked`、`node tests/features.mjs target/release/tsuzuri`、手順 1 の 4 テストが成功する。

## テスト計画

### Rust テスト

`tests/mutable_slices.rs`（新規）の 8 関数と担当ケースは手順 3〜9・11 の `確認:`。受理は IR を 2 回出して一致を、拒否はコードとメッセージ断片を見る。
`tests/formatter.rs` の `formats_exclusive_slices`（新規）は `ref mut [i64..]`、`ref mut values[1..]`、`&mut values[..2]` を整形し、2 回目の整形で変わらない。

| ID | 入力（要点。新構文） | 期待 |
| --- | --- | --- |
| T1 | `def total :: ref mut [i64..] -> i64` で `values.length`・`values[0]`・`for`、`def count :: ref mut ['a..] -> i64` | 受理（`['a..]` を字句解析できる） |
| T2・T3 | `ref mut values[1..3]`、`[1..]`、`[..2]`、`[1..values.length - 1]`（`let mut values`。最後は範囲の評価中の読み取り） | 受理 |
| T4 | `part` の最後の使用の後に `values.length` と二つ目の `ref mut values[0..1]` | 受理 |
| T5 | `fill part 1` を 2 回、`ref mut *part`、`ref part[0..1]` | 受理 |
| T6 | `fill (ref mut values) 1`、`fill values 1`、`ref mut [i64]` の仮引数 `whole` から `fill whole 1`（`fill :: ref mut [i64..] -> i64 -> unit`） | 受理（全体の排他スライス） |
| T7 | `sum_shared part`（`sum_shared :: ref [i64] -> i64`） | 受理 |
| T8 | `def pass :: 'a -> 'a` を `ref mut values` と `part` で呼ぶ | 受理。IR に `pass` の `define` が 2 つあり LLVM の重複定義がない |
| T9 | `while` と `Array.write`・`Array.swap_in`（要素 `i64` と `string`） | 受理 |
| T10 | `negate_all`（「例」）と、二つの半分への交互の書き込み | 受理 |
| T11 | `Array.sort_in_place` を `i64` と `string` で使う。使わないプログラムの IR に `sort_in_place` がない | 受理 |
| R1・R2 | `def f :: [i64..] -> i64`／`def f :: ref [i64..] -> i64` | E1005 `'[T..]' is only valid directly after 'ref mut'` |
| R3 | `ref mut values[..]` | E0002 `write 'ref mut xs[0..]'` |
| R4 | `let part = values[1..2]` | E0002 M1 |
| R5 | 「例」の拒否例（`left` の生存中に `ref mut values[1..3]`） | E1014 `access conflicts with a live borrow`（3 行目） |
| R6 | `Array.write tail 1 (tail[1] * 10)` | E1014 同上 |
| R7 | `let values = [1, 2, 3]` の `ref mut values[0..1]` | E1014 `mutable access requires 'let mut'` |
| R8 | `s: ref [i64]` の `ref mut s[0..1]` | E1014 `cannot mutate or exclusively reborrow through a shared reference` |
| R9 | `ref mut [1, 2, 3][0..2]` | E1014 M4 |
| R10 | `deref s = [1]`（`s: ref mut [i64..]`） | E1014 M3 |
| R11 | `replace s` と `replace (ref mut *s)`（`replace :: ref mut [i64] -> unit`） | E1005 M2（両方） |
| R12 | `fill shared 1`（`shared: ref [i64]`） | E1014 R8 と同じ文 |
| R13・R14 | `let part = ref mut values[0..2]` の後 `values.length + part.length`／`ref mut values[{ values = [3]; 0 }..1]` | E1014 R5 と同じ文 |
| R15 | `\x -> Array.write s 0 x` | E1005 `cannot capture ref mut [i64..] in a reusable function` |
| R16 | task ブロックで `s` を使う | E1013 `tasks require owned values; ref mut [i64..] contains a reference` |
| R17 | `split_at_mut s 1` の結果の生存中に `Array.write s 0 1` | E1014 R5 と同じ文 |
| R18 | `let inner = ref mut left[0..1]`、`Array.write right 0 1`、`Array.write inner 0 2` | E1014（2 行目） |
| R19 | `export def f :: ref mut [i64..] -> i64` | E1008 |
| R20 | 局所配列の `ref mut values[0..1]` を返す | E1013 `cannot return a reference to a local value` |
| R21 | `def f :: [ref mut [i64..]] -> i64` | E1005 M5 |
| R22 | `for x in s do Array.write s 0 x` | E1014 R5 と同じ文 |
| R23 | `items: Vec<i64>` の `ref mut items[0..1]` | E1005 `a slice requires an array` |

### E2E

`tests/fixtures/mutable_slices/Main.tz`（新規）に 1 ケース 1 export で書き、suite `mutable_slices` で native/WASM × `-O0`/`-O3` を実行する。全ケースで `live == 0`、WASM の import は空。
期待値は手計算（下の式）で、コンパイラの出力から作らない。`sort_signed_zero` は `Ord<f64>` が HEAD にあることが前提で、なければ除いて報告する。

| ケース | 内容 | 期待 |
| --- | --- | --- |
| `write_swap` | 「例」の `example` | 63（`values` は `[1, 20, 40, 3]`） |
| `negate` | `[3, -1, 4, 1, 5]` に `negate_all (ref mut values)` の後の和 | -12 |
| `alternating` | `[1, 2, 3, 4]` を 2 で分割し left 0←10、right 0←30、left 1←20、right 1←40。`v0 + 2v1 + 3v2 + 4v3` | 300 |
| `copy_snapshot` | `let mut a = [1, 2, 3]`、`let b = a`、`Array.write (ref mut a[0..]) 0 9`、`b[0] * 10 + a[0]` | 19 |
| `strings_drop` | リテラル `["a", "b", "c"]` に write 0 ← `"xyz"`、`swap_in 1 2`、長さの和 | 5 |
| `convert` | 長さ 5 の配列に `fill (ref mut values) 7` の後の和 | 35 |
| `sort_sizes` | 長さ 0, 1, 2, 19, 20, 21, 40, 41, 1000 で `x = (x * 1103515245 + 12345) % 2147483648`（初期値 1）の `x % 100` を並べ、`sort_in_place` と `Array.sort` が全要素一致した長さの数 | 9 |
| `sort_signed_zero` | `[0.0, -0.0, 1.0, -0.0, 0.0]` を整列し、`1.0 / v[i] > 0.0` なら `2^i` の和 | 25（安定なら `[+0, -0, -0, +0, 1]`） |
| `traps` | `write` の添字 = 長さ、`split_at_mut` の m = 長さ + 1 と -1、`swap_in` の j = 長さ、`ref mut values[2..1]` | 5 つとも BoundsCheck のトラップ |

### 既存テストへの影響

なし。M1・M5 は既存の文を変えるが、HEAD でその文を検査するテストはない（停止条件）。性能の計測は必須でなく、生成コードの条件は手順 11 で確かめる。

## ドキュメント

- `docs/language.md`: 配列・連結リストとレコードの節（型 `ref mut [T..]`、作成、読み取り、引数の変換）、配列・リスト API の節（4 API）、所有権の節の「コレクションは深く不変」を「共有されている間は不変。排他スライスを通してだけ要素を置き換えられる」へ、診断の一覧（M1〜M5 の文）。
- `docs/architecture.md`: `%tz.array` を排他スライスにも使うこと、二相の所有権検査、分割の loan、作成時の `relocate`。
- `_docs/library-reference/arrays-and-lists.md`: 「排他スライスとその場の更新」の節（`write`・`swap_in`・`split_at_mut`・`sort_in_place`、`Array.set` との使い分け）。
- `_docs/library-reference/api/Array.md`: 手で編集せず `target/release/tsuzuri doc std -o _docs/library-reference/api` で再生成する。
- `_docs/language-reference/ownership.md`・`lifetimes.md`: 借用規則と分割の例。`_docs/learn/why-tsuzuri.md`: Rust 比の劣位。`_docs/feature-status.md`・`_features/README.md`: done。GUIDE（手順 13）。

## 受け入れ条件

- [ ] D1 が承認済みで、GUIDE §9 D-13 が「仕様」の承認後の文になっている。
- [ ] `tests/mutable_slices.rs` の 8 テストと `formats_exclusive_slices` が成功し、R1〜R23 がすべて期待のコードで拒否される。
- [ ] suite `mutable_slices` が native/WASM × `-O0`/`-O3` で成功し、`live == 0`、WASM の import は空。
- [ ] 手順 1 の既存 fixture の IR が `-O0`・`-O3` とも byte 単位で変わらず、新しいランタイム関数・`TrapKind`・crate・WASM import がない。
- [ ] stack-depth 3 テストと `honors_the_exact_specialization_limit` が、上限や stack を変えずに成功する。
- [ ] ドキュメントが更新されて `node scripts/check-docs.mjs` が成功し、GUIDE §10 の完了の定義を満たす。

## 落とし穴

- `Dereference` の型: 排他スライスを剥がした型を `ArrayView(T)` のままにすると添字・`for`・`length` が E1005 になる。`ExprKind::Borrow`・`ExprKind::Assign` で特別扱いを忘れると、
  `ref mut *s` が `ref mut [T]` になって全体置換が通る（健全性の穴）。R10・R11 で検出する。元が `Dereference` のスライスは移送しない（停止条件）。
- `_ =>` fallback: `display` や `canonical_type` が `ArrayView` を黙って別の型として扱うと、診断の文や T8 のシンボルが壊れる。手順 2 で `grep` の全箇所を見る。
- 表現の判定: `shared_array_element()` を 1 か所でも残すと、排他スライスが `ptr` として扱われ、`-O0` で誤った値か segfault になる。`grep -n "shared_array_element()" src/*.rs` が空であることを確かめる。
- `relocate` は記述子を読む前に行う。後で行うと view がフレームを指したまま残り、`Array.write` の旧要素の drop が `tz.free` をフレームに対して呼ぶ（WASM で壊れる）。`strings_drop` で検出する。
- parser: `index_or_slice` は `slice_context` と `stop_at_slice_dotdot` を必ず復元する。`[T..]` の分岐で `type_expr` を余計に再帰させない（stack 深さ）。
- `coerce_argument` の中に変換を書き足すと、引数の検査の frame が大きくなり `bounds_nested_builder_expansion_not_just_source_syntax` が落ちる。`coerce_slice_argument` に出す。
- `sort_in_place`: 中点を `(lo + hi) / 2` にしない。二分探索の境界（`!lt(i, a)` と `lt(m, i)`）を入れ替えると安定でなくなり、`sort_signed_zero` が 25 以外になる。
- Copy 配列: `let b = a` の後で `a` をスライスすると、`clones_on_take` は `a` を単一使用とみなさず複製する。`copy_snapshot` が 99 なら複製が省かれている。

## 対象外

- 代入構文 `xs[i] = v`、要素の排他借用 `ref mut xs[i]`、`for` での可変反復、`List`・`string`・`Vec` の排他スライス、共有スライスからの可変化。
- 排他スライスの record への格納（A13 の規則に従う。本チケットは検査しない）、export と host ABI（E1008 のまま）。
- `Parallel` による並列の分割書き込み（Phase 2、D11）、`chunks_mut` 相当の API、`Array.fill` などの一括のその場更新、SIMD の store（F04・F08）。

## 決定事項

### D1: GUIDE §9 D-13 の変更

- 決定: 「仕様」の承認後の文に置き換える。排他スライス `ref mut [T..]` を足し、`ref mut [T]` は全体置換のまま残す。
- 理由: 既存の `ref mut [T]` のプログラムと IR を変えずに Rust の `&mut [T]` 相当を足せる。`ref mut [T]` の意味を部分へ広げると全体置換と両立しない。
- 状態: 要承認（承認前はどの手順にも着手しない）

### D2: 型の内部表現

- 決定: `Type::Reference(Box::new(Type::ArrayView(T)), true)`。`ArrayView` は参照先にだけ現れる。
- 理由: `Reference(_, true)` の既存の性質（非 Copy・捕捉不可・送信不可・E1008・loan）をそのまま使える。`Reference` に 3 つ目の field を足すと全 match が変わる。
- 状態: 既定案（実装者はこの案に従う）

### D3: 構文

- 決定: 型は `ref mut [T..]`、式は `ref mut xs[a..b]`／`&mut xs[a..b]`。`[T..]` は parser がどこでも読み、`resolve_type` が位置を検査する（E1005）。
- 理由: parser に文脈を持たせずに済み、誤用に修正方法つきの診断を出せる。
- 状態: 既定案（実装者はこの案に従う）

### D4: API の名前と実装の形

- 決定: `Array.write`・`Array.swap_in`・`Array.split_at_mut` は `Builtin`、`Array.sort_in_place` は `std/Array.tz` の関数。
- 理由: 前の 3 つは記述子と境界検査を直接扱う。`swap` は消費的な既存 API と名前が衝突するので `swap_in`。整列は既存の `Ord.lt` で書ける。
- 状態: 既定案（実装者はこの案に従う）

### D5: 暗黙の変換は呼び出しの実引数だけ

- 決定: 型規則 6 の表の変換を `coerce_slice_argument` だけで行う。`let` の注釈と返却値では変換しない。
- 理由: 既存の暗黙の借用も引数だけで起きる。変換の位置を増やすと推論と診断が複雑になる。
- 状態: 既定案（実装者はこの案に従う）

### D6: 分割の半分は一つの loan を共有する

- 決定: `split_at_mut` の結果は実引数の貸し直しの loan を一つ持ち、半分を区別しない。
- 理由: `Loan`・`Place` に範囲を持たせずに済む。偽陽性（R18）は安全側で、`let` へ分ければ回避できる。
- 状態: 既定案（実装者はこの案に従う）

### D7: LLVM の表現

- 決定: `ref mut [T..]` は `%tz.array` の値。判定は `Type::slice_element` に集め、`canonical_type` は `view[..]`。
- 理由: 共有スライスの経路（`array_slice`・`Dereference`・デバッグ情報）をそのまま使え、ポインターを 1 段減らせる。長さが固定なので値で足りる。
- 状態: 既定案（実装者はこの案に従う）

### D8: 所有権の二相検査

- 決定: 範囲の評価中は共有の guard loan、評価後に排他 loan（「アルゴリズム」）。
- 理由: 実行順（元の記述子を先に読む）と一致し、範囲の中での読み取り（T3）を許しつつ書き込み（R14）を拒否できる。
- 状態: 既定案（実装者はこの案に従う）

### D9: 構文木の可変性の持ち方

- 決定: `ExprKind::Slice` に `mutable: bool`、parser の `slice_context: Option<bool>`。`TypedExprKind::Slice` は変えず型で区別する。
- 理由: typed の match（`src/call_specialization.rs` など）を変えずに済む。
- 状態: 既定案（実装者はこの案に従う）

### D10: `sort_in_place` のアルゴリズム

- 決定: 長さ 20 の挿入整列の後に SymMerge と回転で併合する（Go の `sort.Stable` と同じ構成）。作業領域は確保しない。
- 理由: 比較 O(n log n)、交換 O(n log² n)、再帰の深さ O(log n) で確保がない。`lt` だけで安定なので `Array.sort` と一致する。
- 状態: 既定案（実装者はこの案に従う）

### D11: Phase 2（並列の分割書き込み）

- 決定: `split_at_mut` で得た互いに素な半分を Task へ渡す API を Phase 2 で設計する。Phase 1 では `ref mut [T..]` を Send 不可のままにする。
- 理由: Send の例外は D-13 以上の意味の変更で、F10 の同期規則とも関わる。
- 状態: 要承認（承認前は Phase 2 に着手しない）

### D12: 診断

- 決定: 既存のコードだけを使い、新しい文は M1〜M5（「診断」の表）。E0002 の M1 と E1005 の M5 は既存の文を置き換える。
- 理由: 条件は既存コードの分類に収まる。GUIDE D-30 の新しいコードを要しない。
- 状態: 既定案（実装者はこの案に従う）

### D13: 作成時のヒープ移送

- 決定: 元がフレーム上の local なら、排他スライスの作成時に `relocate` する。`Dereference` を元とするスライスでは移送しない。
- 理由: `Array.write` は旧要素を drop するので、フレーム上の要素を `tz.free` してはならない。既存の排他 `Borrow` と同じ規則で、共有スライスの IR は変わらない。
- 状態: 既定案（実装者はこの案に従う）

## 実装と検証（2026-10-06）

「C08、A14、A16、F13、F08 の実装をすべて完遂して」という依頼を D1・D11 の承認として扱い、Phase 1 と Phase 2 を実装した。
着手時の HEAD は `ff84e4c`（ブランチ `Phase7-3`）で、A16・A14・F13・F08 と同じ変更に含めた。性能の改善は主張しない（生成コードの条件だけを確かめた）。

### 実装

- 構文: `TypeExprKind::ArrayView`（`[T..]`。parser はどこでも読み、`resolve_type` が `ref mut` の直後以外を E1005 にする）と
  `ExprKind::Slice` の `mutable`。parser の `slice_context: Option<bool>` は `index_or_slice` で `take()` する。formatter・docgen・regions・semantic・warnings・polymorph に腕を足した。
- 型: `Type::ArrayView` と `Type::slice_element`（`shared_array_element` を置き換え、表現の判定を集約）、`is_view`、`dereferenced`（ビューの参照外しは `[T]` として型付け）。
  排他スライスはすべて `TypedExprKind::Slice`（貸し直しと配列全体への変換は `whole_view`）。`exclusive_borrow` と `coerce_slice_argument` は
  `#[inline(never)]` で分け、引数の検査の frame を大きくしない。
- 所有権: `src/ownership.rs` の `exclusive_slice`（範囲の評価中は共有の guard loan、評価後に `MutBorrow`）。`Parallel.for_each_chunk` は
  `Builtin::parallel_callback` で callback の位置を引き、入力の要素型は `slice_element` で取る。
- 生成: `%tz.array` を排他スライスにも使い、`canonical_type` は `view[T]`。作成時に `own_heap_storage`（`src/llvm_frame.rs`）がフレーム上の配列を移送する。
  `Array.write`・`Array.swap_in`・`Array.split_at_mut` は `emit_typed_builtin`。`Parallel.for_each_chunk` は `src/llvm_parallel.rs` の `parallel_chunks`
  （`size > 0` の検査、`ceil(len / size)` 個のチャンク）で、既存の要素ループは `parallel_items` に切り出した。
- std: `Array.sort_in_place` と private の helper 3 つ（長さ 20 の挿入整列、`def rec` の SymMerge、回転）。ループは `for` の範囲で書き、
  `hint_loop` の展開の警告を出さない。
- テスト: `tests/mutable_slices.rs`（新規 9 件。T1〜T11、R1〜R23、Phase 2 の受理と拒否）、`tests/formatter.rs` の `formats_exclusive_slices`、
  `tests/fixtures/mutable_slices/Main.tz` と suite `mutable_slices`（38 ケースとトラップ 6 つ）。
- 文書: `docs/language.md`、`docs/architecture.md`、`_docs/library-reference/arrays-and-lists.md`・`parallel.md`・`api/Array.md`（再生成）、
  `_docs/language-reference/ownership.md`・`lifetimes.md`、`_docs/learn/why-tsuzuri.md`、`_docs/feature-status.md`、`_features/README.md`、`_features/GUIDE.md`（D-13・D-30・D-39）。

### 決定事項への追記（チケットから外れた判断）

1. **D5 を緩めた。** 明示の `ref mut xs` は、排他スライスを期待する位置（`let` の注釈、汎用関数の引数を含む）で配列全体の排他スライスになる。
   引数だけに限ると `let whole: ref mut [i64..] = ref mut values` が `E1003` になり、同じ式の型が位置によって変わる。暗黙の借用（所有値や `ref mut [T]` の値をそのまま渡す形）は引数だけのまま。
2. **D11 の Phase 2 は `Parallel.for_each_chunk`。** `split_at_mut` の半分を Task へ送るには排他スライスを Send にし、task の持つ借用の寿命を join まで延ばす規則が要る（F10 の `Task.scope`）。
   既存の Parallel と同じ fork/join の組み込みなら、借用は呼び出しの中に閉じ、チャンクが互いに素であることをコンパイラが保証できる。
   callback は `i64 -> ref mut ['a..] -> unit`（チャンクの先頭の添字とスライス）で、他の Parallel と同じく借用の捕捉と関数値化は `E1013`。`size <= 0` はトラップ（`RangeStepZero`）。
   チャンク境界は利用者の `size` だけで決まり、F02 の固定チャンクの式は使わない（書き込みの粒度を利用者が選べるようにする）。
3. **診断の文。** R20（局所配列の排他スライスを返す）は既存の `E1013 "borrowed value does not live long enough to leave this block"` になる。
   チャンクの要素が Send でないときは既存の `E1013 "tasks require owned values; ..."`。どちらもコードはチケットどおり。
4. **排他借用フィールド。** A13 が done なので `record Cursor {r} { part: ref mut {r} [string..], position: i64 }` はそのまま使える。E2E の `record_cursor` で確かめた。

### 確認（Apple M1 Max、macOS 27.0.1、Apple clang 21、Homebrew LLVM 21、rustc 1.98.1、Node v20.19.6）

- `cargo test --locked --test mutable_slices`（9 passed）、`formats_exclusive_slices`、GUIDE §3.1 の stack-depth テストと `honors_the_exact_specialization_limit` が上限や stack を変えずに成功。
- suite `mutable_slices` が native/WASM × `-O0`/`-O3` で成功（`live == 0`、WASM の import なし）。`TSUZURI_ASAN=1` と `TSUZURI_TSAN=1` でも成功。
  `parallel`・`slices`・`array_bulk` の suite も成功。
- 既存の fixture と単一ファイルの例の IR（119 個）は、`ff84e4c` と比べて 68 個が byte 一致、51 個は std の関数の追加による生成 id の一様なずれだけが異なる（id を写す比較で差なし）。
  チケットの「byte 単位で変わらない」は、A15 と同じく id のずれを除いて満たす。`fill` と `negate_all` は `-O3` で `@tz.alloc` を呼ばない。
- 全体のゲート（fmt・clippy・`cargo test`・features の全 suite・`check-docs`）は 5 チケットの実装の後にまとめて実行した（F08 の記録を参照）。

### レビュー対応（PR #14）

- `Parallel.for_each_chunk` はチャンクごとに 1 つの仕事を作っていたため、直接呼べない callback（実行時に選ぶ関数値など）では
  起動前にチャンクの数だけ callback を複製していた（`size = 1` なら要素数と同じ数）。仕事の数を他の並列操作と同じ最大 1024 にし、
  各仕事が連続するチャンクを順に処理する（`parallel_chunk_body`。チャンク番号の分割は `parallel_bound`）。各チャンクの開始添字・範囲・
  呼び出しの回数は変わらない。
- 確認: fixture の `parallel_dynamic`（callback を実行時に選ぶ）で 1024 を超えるチャンク数を native・WASM の `-O0`・`-O3` で検査し、
  `TSUZURI_TSAN=1`・`TSUZURI_ASAN=1`・`TSUZURI_TEST_ALLOCATOR=host` も成功。`tests/allocator.mjs` は `--allocator counting` の WASM で
  100,000 要素・`size = 1` の最大使用量を測り、修正前の 3,200,056 バイトが 824,632 バイト（配列 800,016 バイトと 1024 個の複製）になった。
