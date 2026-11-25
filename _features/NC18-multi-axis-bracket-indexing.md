# NC18: 多次元の角括弧の添字 `t[i, a..b, ..]` と厳格なキー `Slice.Sel`

| 項目 | 内容 |
| --- | --- |
| ID | NC18 |
| 優先度 | P1（NumPy の `a[i, j]`・`a[1:4, :]` を素直に書けるようにする。NC19・NC20 の例と書き込み窓の土台） |
| 規模 | L（構文の新しい節点 `MultiIndex`、型で決める脱糖、std の厳格なキー型と窓のインスタンス。Phase 2 は `^k` の要素の読み出し） |
| 依存 | [A37](A37-index-base-place-borrow.md), [A38](A38-index-evaluation-order.md), [NC16](NC16-index-ref-and-zero-alloc-element-reads.md), [NC17](NC17-edition-2028-inclusive-range-family.md) |
| 後続 | [NC20](NC20-compound-assignment-and-mutable-index-views.md), ([NC19](NC19-operator-extension-instances.md)) |
| 状態 | todo |
| 起票 | 2026-11-25（[GUIDE D-66](GUIDE.md#d-66-自然な添字範囲演算子d-65-の改訂a37nc16nc20)。HEAD `858a949c` で確認） |
| 承認 | 方向は承認済み（D-66a 範囲の意味、D-66d `ref` 必須。添字の角括弧の中のカンマは D-65「パーサーの文法を増やさない」の改訂として D-66 に記録）。評価順（D7）は決定（D-66 の委任で決定、2026-11-25）で、規則は A38。2028 の受理（D1・D8）を 2028 を含むリリースの後に入れてよいことと、窓の整数の成分への `W1014`（D15）も決定（同。NC17 の Q2・Q1）。ほかの決定事項は既定案。std に足す名前（`Slice.Sel`・`Slice.Pos`・`Slice.Bound`・`Slice.rest`・`Slice.resolve_sel`）と新しい診断の kind は GUIDE D-55 に登録済み（2026-11-25 の統合） |
| 手本にする既存実装 | `src/parser.rs` の `index_or_slice`（4511 行付近）、`src/check.rs` の `ExprKind::Index`（14748 行付近）と `slice`（16028 行付近）、`std/Tensor.tz` の `slice_raw`（1588 行付近）、`std/MatrixView.tz` の `slice`（428 行付近）、`std/Slice.tz` の `resolve` |
| 主な影響ファイル | `src/syntax.rs`、`src/parser.rs`、`src/syntax_codec.rs`・`src/frontend_cache.rs`（`FRONTEND_FORMAT`）、`src/formatter.rs`、`src/computation.rs`、`src/parse_canonical.rs`、`src/warnings.rs`、`src/check.rs`、`src/polymorph.rs`、`src/diagnostic_kinds.rs`、`src/stdlib.rs`、`std/Slice.tz`、`std/Tensor.tz`、`std/MatrixView.tz`、`tests/multi_index.rs`（新規）、`tests/editions.rs`、`tests/index_class.rs`、`tests/std_argument_order.rs`、`tests/std_examples.rs`、`tests/fixtures/multi_index/`（新規）、`tests/fixtures/effects_sweep/`、`tests/features.mjs`、`_tsuzuri/language-reference/`（下の「ドキュメント」）、`docs/language.md` |

## 実装者向け早見

- 範囲: (1) 添字の角括弧にカンマ区切りの成分を許す `base[c1, …, cn]`。成分は式（位置か `Slice.Sel` の値）、範囲（NC17 の形と刻み）、単独の `..`。2 成分以上は新しい節点 `ExprKind::MultiIndex` にし、`ref`／`ref mut` を `Slice` と同じく吸収する。(2) 型検査で脱糖する。位置だけのキーは要素の読み出し（`m[i, j]` は `m[(i, j)]` と同じ）、範囲・単独 `..`・`Slice.Sel` の成分を含むキーは窓（`ref` が要る）。(3) 範囲外を丸めず負の位置を末尾から数えない std の厳格なキー `Slice.Sel` と、Tensor・Matrix の窓のインスタンス。
- 最初に読む: ① 仕様 1〜4 ② `src/parser.rs` の `index_or_slice` と `prefix`・`keyword_prefix` の `Slice` の吸収（5215・5287 行付近） ③ `src/check.rs` の `ExprKind::Index`（14748〜14832 行付近）④ `std/Tensor.tz` の `slice_raw` と `Index` インスタンス（1588〜1682 行付近）⑤ [A38](A38-index-evaluation-order.md) の評価順の規則（仕様 5）。
- 最初に書くテスト: `tests/multi_index.rs` で `t[i, j]` と `t[(i, j)]` の IR が同じこと、`ref t[1..<3, 0]` と長い形 `ref t[[Slice.Sel.Span (…), Slice.Sel.Item (…)]]` の IR が同じこと、`Slice.resolve_sel` の境界の表（仕様 3.4）。
- 触ってはいけない: 組み込みの 6 型（配列・リスト・`Vec`・固定長配列・`string`・`utf8string`）の添字とスライスの IR と F12 の境界検査の除去。`Slice.Axis` とその関数（丸めと負の位置の規則のまま）。マスク `t[mask]` とファンシーインデキシング（コピー）。成分の暗黙の整数変換をしない。
- 完了の見分け方（edition 2028）: 次がすべて native/WASM × `-O0`/`-O3` で一致し、`t[1, 2]` の確保が 0 回、窓 1 つの確保が多くとも 2 回。

```tsuzuri
let t = Tensor.init [3, 4] (\i -> i)
let e = t[1, 2]              // 6
let row = ref t[1, ..]       // [4, 5, 6, 7]
let col = ref t[.., 0]       // [0, 4, 8]
let last = ref t[^0, ..]     // [8, 9, 10, 11]
let block = ref t[0..1, 1..<3]   // [[1, 2], [5, 6]]
```

## 目的

NC04 の `Index` で Tensor・Matrix に `x[k]` が使えるようになったが、NumPy の `a[i, j]` は `t[[i, j]]`、`a[1:4, :]` は `t[[Slice.range 1 4, Slice.all()]]` と書く必要があり、`Slice.range` は範囲外を丸め、負の位置を末尾から数える（`std/Slice.tz` 3〜4 行）。D-66 で範囲の構文（終端を含む `..`、`..<`、`<..`、`<..<`、`^k`）を一本化したので、その構文を多次元の添字にそのまま使えるようにする。

- 数値計算のコードを NumPy に近い形で書ける（`ref a[1..^1, ..]`、`ref t[.., 0]`、`m[i, j]`）。
- 書いた範囲がそのまま選ぶ位置になる。範囲外はトラップし、黙って丸めない（D-66a・D-66b、配列のスライスと同じ規則）。
- 窓は `ref` で始まるので、借用と「要素をコピーしない」ことが見た目で分かる（D-66d）。

## 着手条件と停止条件

### 着手条件

- [A37](A37-index-base-place-borrow.md)、[A38](A38-index-evaluation-order.md)、[NC16](NC16-index-ref-and-zero-alloc-element-reads.md)、[NC17](NC17-edition-2028-inclusive-range-family.md) の edition 2028・範囲の形・`^k` の段（NC17 の Phase 2）が done。NC17 の `LATEST` の移動（NC17 の Phase 3）は待たない。
- 2028 を含むリリースが先に出ていても着手してよい。NC17 の Phase 2 が、NC18 の 2028 の受理を含む 2028 の差の全体を宣言するので、NC18 は宣言した差を後から実装し、受理を増やすだけになる（NC17 の Q2 の決定。リリースを止めない）。
- 基準: `gh run list --branch main --workflow ci.yml --limit 1` が緑。`rg -n 'ExprKind::Slice|Slice \{' src/` で `ExprKind` を辿る箇所を控える（設計 2）。

### 停止条件

- **S1** 依存チケットのインターフェースが「前提とする他チケットのインターフェース」と違い、意図を保って合わせる方法が 1 つに決まらない（例: NC17 が角括弧の中の刻みや `^k` を別の形で表した、NC16 がタプルの要素のインスタンスを入れていない）。
- **S2** `t[i, j]` と `t[(i, j)]`（タプルのインスタンスがなければ `t[[i, j]]`）の IR を同じにできない。
- **S3** 新しい構文と新しいインスタンスを使わない既存の fixture の IR が 1 byte でも変わる。組み込みの 6 型の添字・スライスの IR か F12 の除去が変わる。
- **S4** A37 の事実 `returns_contents`（結果が土台の参照先の持つ借用だけを持つ）が、新しい `View → View` の `[Slice.Sel]` インスタンスで立たず、連鎖した窓 `ref t[1..3, ..][0, ..]` を通すのに所有権の新しい規則が要る。
- **S5** Matrix だけを名指すプログラムの窓の構文で `Slice`・`MatrixView` を読み込む方法が、`Matrix` を使うすべてのプログラムで `MatrixView` を読み込む以外にない（読み込みの方針の判断になる。決定事項 D11）。
- **S6** edition 2028 の文書の例を `node scripts/check-docs.mjs` で検査する手段が NC17 にない（`LATEST` が 2027 のまま、例に edition を指定できない）。NC17 は Phase 2 で例の属性 `edition=YYYY` を足す計画（NC17 の手順 Phase 2 の 10）なので、それが入っていなければ止める。
- **S7** 2028 を含むリリース（数え方は NC17 の Q3 の決定）の後で、NC18 の実装に、2028 が受理するプログラムの拒否か意味の変更（バグの修正を除く）が要る、または NC17 の Phase 2 が宣言していない 2028 の差が要る（NC17 の Q2 の決定）。新しい edition か承認が要るので止める。リリースの前なら、NC17 の規則を直してから続けてよい。
- **S8** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `858a949c` で確認）

コードの参照:

- `src/parser.rs` 4511〜4592 行付近の `index_or_slice` は、角括弧の中を 1 つの式と省略できる範囲（`..`・`..<`・`..=`）だけで読む。範囲があれば `ExprKind::Slice`、なければ `ExprKind::Index` を作る。範囲を含むのに `ref` がなければ `E0002`（kind `slice_must_be_borrowed`、4560〜4567 行付近）、境界がなければ `E0002`（kind `slice_without_bounds`）。成分を区切るカンマはない。
- `ref`／`ref mut` の吸収: `prefix`（5215 行付近）と `keyword_prefix`（5287 行付近）は、オペランドが `ExprKind::Slice` のときだけ借用の節点を作らずに `Slice` を返す。`index_or_slice` は外側の借用の文脈 `slice_context` を受け取り、終わりに戻す（4514・4549 行付近）ので、`ref xs[1..3][0..1]` は両方の角括弧が借用の文脈を見る。
- `src/syntax.rs` 1380〜1389 行: `ExprKind::Index(Box<Expr>, Box<Expr>)` と `ExprKind::Slice { value, start, end, mutable, op: RangeOp }`。`RangeOp` は `Bare`・`Closed`・`Open`（1477 行付近）。
- `src/check.rs` 14748〜14832 行付近の `ExprKind::Index`: 組み込みの 6 型は `index_operand`（全整数型を受ける）で読む。それ以外は `index_key_hint`（`src/polymorph.rs` 2717 行付近。Self に合う候補が 1 つのときだけキーの型を示す）でキーを型付けし、`resolve_index`（2758 行付近。候補が残ると宣言順の先頭、2832 行付近）でインスタンスを選び、`Call(インスタンス, [BorrowOperand(key), BorrowOperand(value)])` に下げる（14825・14830 行付近）。引数の順にキーが先、土台が後に評価される（A38 で土台が先になる）。
- `src/check.rs` 14898 行付近: `target[key] = rhs` は組み込みでない型で `IndexMut` を引き、対象を `Borrow(raw_target, true)` で下げる。呼び出しの引数は「キー、右辺、対象」の順（14971〜14986 行付近）で、この順に評価される（A38 で右辺、土台、キーの成分の順になる）。
- `src/check.rs` 16028〜16110 行付近の `slice`: 配列と（共有の）固定長配列のほかは `E1005`（kind `slice_non_array`、"a slice requires an array; lists and strings do not support array slicing"）。
- `std/Slice.tz`: `union Axis`（5〜12 行。`All`・`At`・`Range`・`From`・`To`・`Step`・`NewAxis`）は負の位置を `len + i` と読み、範囲を軸の端に丸める（`clamp_pos`・`clamp_neg`、118〜130 行）。`union Resolved`（40 行）は `Index of i64`・`Slice of (i64 * i64 * i64)`（始点・個数・刻み）・`InsertAxis`。`resolve`（172 行）。
- `std/Tensor.tz`: `View<'a> { r }`（11 行）と `Mut<'a> { r }`（16 行）は `shape`・`strides` の配列を所有する。`slice_raw`（1588 行付近）は `own [Slice.Axis]` を取り、出力の shape と strides を `Vec.push_mut` で作る。`Index` インスタンスは 1660〜1682 行付近（`[i64]` の要素、`[Slice.Axis]` の窓。どちらも `Array.copy` でキーを複製）と 2883〜2900 行付近（マスク）。`Mut` を Self に取る読み出しの `Index` はない。
- `std/Matrix.tz:277` に `Index<Matrix<'a>, (i64 * i64), 'a>`。`std/MatrixView.tz` 455〜478 行付近に `MatrixView` の要素、`(Slice.Axis * Slice.Axis)` の窓、`IndexMut<Mut<'a>, (i64 * i64), 'a>`。`validate_2d_axis`（405 行付近）は `At` の軸を長さ 1 で残し（`(pos, 1, 1)`）、負の刻みと `NewAxis` でトラップする。`MatrixView.strided`（62 行）は負の引数でトラップする（負のストライドを持てない）。
- `src/stdlib.rs`: opt-in の読み込みはソースの語で決まる（`sources_for`、541〜610 行付近）。`MatrixView` は `Matrix` と `Slice` を、`Tensor` は `Matrix`・`MatrixView`・`Slice` を使う（346・352 行付近）。`PAIR_TRIGGERS`（500 行）は `Matrix` と `Slice` の両方が名指されたときだけ `MatrixView` を読む。
- `src/syntax.rs:35` の `Edition::LATEST` は 2027。マニフェストのないソースと std はこれで読む。
- `tests/std_argument_order.rs:142` は `("Slice", subject(&["Axis"]))`、`tests/std_examples.rs:74` は `("Slice", 10)`。

再現（`/tmp/tz-nc18/<Name>/Main.tz` に置き、`node scripts/slot.mjs -- target/debug/tsuzuri run -O0 /tmp/tz-nc18/<Name>/Main.tz`。ファイル名は大文字で始める）:

| 名前 | ソース（要点） | 結果 |
| --- | --- | --- |
| Comma | `let t = Tensor.init [2, 3] (\i -> i * 10)` / `t[1, 2]` | `error[E0002]: expected ']'`（kind `expected_token`） |
| Trailing | `let xs = [1, 2, 3]` / `xs[1,]` | 同上 |
| OneRange | `let t = Tensor.init [4] (\i -> i)` / `let v = ref t[1..3]` | `error[E1005]: a slice requires an array; lists and strings do not support array slicing` |
| ShortKey | `let t = Tensor.init [2, 3] (\i -> i)` / `t[[0]]` | `trap: assertion failed`（軸の数と添字の数が違う。`std/Tensor.tz` の `offset_of` の `assert`） |
| I32Key | `let i: i32 = 1` / `t[[i, 2]]` | `error[E1005]: no instance for Index<Tensor<i64>, [i32]>` |
| TupleMap | `let h = HashMap.insert (3i32, 4i32) 8 (HashMap.empty())` / `h[(3, 4)]` | `8`（リテラルはキーの型 `i32` を取る） |
| MatRow | `let m = Matrix.of_array 2 3 [1, 2, 3, 4, 5, 6]` / `m[(Slice.at 1, Slice.all())]` | 行 1・列 3 の `MatrixView`、`[4, 5, 6]` |
| NegStep | `m[(Slice.all(), Slice.rev())]` | `trap: assertion failed` |
| RefElem | `let r = ref t[[1, 2]]` | `error[E1013]: borrow requires a local place; bind the temporary with 'let' first` |
| Order | 土台とキーがどちらも `assert false` する関数で `(bad_base ())[[bad_key ()]]` | キーの `assert` が先に止まる。配列の `(bad_base ())[bad_key ()]` は土台が先。A38 で `Index` の経路も土台が先になる |
| ViewAlloc | `--allocator counting -O3` で `Alloc.since` | `t[[Slice.range 1 3, Slice.at 2]]` は 4 回、`t[[Slice.at 1, Slice.at 2]]` は 2 回、`t[[1, 2]]` は 3 回確保する |

2 次元の値の確認（今の長い形。範囲内なら新しい構文の結果と同じ）: `Tensor.init [3, 4] (\i -> i)` で `t[[Slice.at 1]]` は `[4, 5, 6, 7]`、`t[[Slice.all(), Slice.at 0]]` は `[0, 4, 8]`、`t[[Slice.at (-1)]]` は `[8, 9, 10, 11]`、`t[[Slice.range 0 2, Slice.range 1 3]]` は `[1, 2, 5, 6]`、`t[[Slice.from 2, Slice.from 2]]` は `[10, 11]`、`t[[Slice.rev()]]` は `[8, 9, 10, 11, 4, 5, 6, 7, 0, 1, 2, 3]`、`t[[Slice.all(), Slice.every 2]]` は `[0, 2, 4, 6, 8, 10]`、`t[[Slice.new_axis()]]` の shape は `[1, 3, 4]`、`t[[1, 2]]` は `6`。

## 変えてはいけない意味

- 組み込みの 6 型の添字・スライスの規則と IR（全整数型の添字、境界検査と F12 の除去、`ref xs[a..b]` と `ref mut xs[a..b]`。範囲の意味は NC17 の edition の規則のまま）。1 成分の配列のスライスは `ExprKind::Slice` のまま。
- 長い形 `t[[…]]`・`m[(…)]` に `Slice.Axis` を渡したときの意味（丸め、負の位置を末尾から数える）。`Slice.Axis` の case と関数（stability.md「std の union への case の追加」）。
- マスク `t[mask]` とファンシーインデキシング（`Tensor.take` など）。どちらもコピーで、`ref` を付けない。
- A38 の添字の評価順（土台、キーの成分を左から右。キーを評価する間は土台に共有の守り。代入は右辺が先）。
- 関数版の契約: `Tensor.slice`・`MatrixView.slice`・`Slice.resolve`（丸める）、`Result`・`Maybe` を返す関数。構文だけがトラップする。
- A37・NC16 の借用の規則（土台の場所の共有借用、`E1013`・`E1014`、`ref` 必須）。
- 新しい構文と新しいインスタンスを使わないプログラムの IR（使わない std の関数は出力されない。手順 0 で確かめる）。
- D-59（対象は最後の引数）。

## 仕様

範囲の例は edition 2028 の意味で書く。範囲の 1 成分の意味と境界の規則は NC17（D-66a〜c、[GUIDE D-66](GUIDE.md#d-66-自然な添字範囲演算子d-65-の改訂a37nc16nc20) の基準表と境界の規則、NC17 の仕様 1・2）と同じで、このチケットはそれを多次元に広げる。

### 1. 構文

```text
index_suffix = "[" component { "," component } "]"
component    = expression        (* 位置（整数）か Slice.Sel の値。NC17 の ^k を含む *)
             | range             (* NC17 の形: a..b  a..<b  a<..b  a<..<b  ..b  ..<b  a..  a<..  と刻み付きの a .. s .. b  a .. s ..< b  .. s ..  など *)
             | ".."              (* 単独: 軸全体。2 成分以上のキーの成分としてだけ *)
```

- 成分が 1 つ: 今の節点のまま。式は `ExprKind::Index`、範囲は `ExprKind::Slice`（NC17 が広げた形）、単独 `..` は `E0002`（kind `slice_without_bounds`）。
- 成分が 2 つ以上: 新しい節点 `ExprKind::MultiIndex { value, keys, borrow }`（設計 1）。`borrow` は `ref` なら `Some(false)`、`ref mut` なら `Some(true)`。`prefix`・`keyword_prefix` は `Slice` と同じく `MultiIndex` を吸収する。`ref` は項を 1 つ取るので、`ref t[1..3, ..][0, ..]` では両方の角括弧が借用の文脈を見る（今の `Slice` と同じ）。NC16 は `ref t[[…]]` の `ref` を型検査で吸収するので、`borrow` が `Some` の `MultiIndex` の土台をたどって続く `ExprKind::Index`（`ref t[[A]][1..2, 0]` の `t[[A]]`）は、NC16 の「`ref` が覆う」連鎖として扱う。
- 末尾のカンマ `t[i,]` は `E0002`（kind `expected_token`、"expected an index component after ','; remove the trailing comma"）。
- 成分の中の式は今の添字と同じく、外側の `stop_at_arm`・`stop_at_arrow`・`stop_at_slice_dotdot` を成分ごとに保存して戻す（NC17 の B3 の修正を前提にする）。前置の `^` は NC17 のとおり `[`・`,`・範囲の演算子の直後だけ。オペランドの後ろの `^` は XOR のまま（`t[a ^ 1, 0]`）。
- 構文だけで決まる誤り（パーサー）:
  - 範囲か単独 `..` の成分を含むのに `ref` がない: `E0002`（kind `slice_must_be_borrowed`、文を一般化して "index windows must be borrows; write 'ref t[...]' for a shared window"）。NC20 が `t[1..3, ..] += …` で取り消せるように、出す場所を 1 つの関数にまとめる。
  - キーが単独 `..` だけ（`ref t[.., ..]`）: `E0002`（kind `slice_without_bounds`、"a key made only of '..' selects everything; borrow the value itself with 'ref t'"）。
- edition: 整数の位置と式だけのカンマ区切りのキー（`t[i, j]`、`ref t[Slice.rest(), 0]`）は全 edition。範囲・`^k`・単独 `..` の成分を含む多成分のキーは 2028 から（`^k` は位置だが、前置の `^` そのものが NC17 で 2028 からの構文）。2026・2027 では `E0002`（NC17 の kind `range_form_needs_edition_2028` に文を 1 つ足す: "range components in a multi-axis key need edition 2028; set 'edition = \"2028\"' in Tsuzuri.toml"。位置は最初の範囲の成分。2026-11-25 の統合で、新しい kind を作らず NC17 の kind を共有することにした）。`^k` と新しい範囲の形そのものの edition の規則は NC17 のもの。`..<`・`..=` の成分も 2026・2027 では拒否する（1 成分の配列と違い、旧来の意味を持つ使い方がまだないので、新しく書くコードを 2028 にそろえる）。
- 2028 からの受理（範囲・`^k`・単独 `..` の成分を含む多成分のキーと、仕様 2.5 の 1 成分の窓）は、NC17 の Phase 2 が docs/stability.md の 5 つ目の差（角括弧の中の範囲の構文の族の全体）として宣言する差の一部である。NC18 はその差を後から実装し、受理を増やすだけなので、2028 を含むリリースの後に入っても凍結の規則に反しない（NC17 の Q2 の決定）。2026・2027 の拒否（上の `E0002` と仕様 2.5 の `E1005`）も同じ宣言に含まれる。どちらも NC18 の前から拒否されているプログラムで（多成分のキーは現状の Comma の `E0002`、1 成分の窓は現状の OneRange の `E1005`）、NC18 は拒否の kind・文・help だけを変える。
- 字句: 新しい予約語・記号はない（カンマは今 `E0002` の位置にだけ足す）。

### 2. 脱糖（型検査）

`MultiIndex` は型検査で既存の `Index`・`IndexMut` の呼び出しに下げる。新しい `TypedExprKind` は作らない。

1. 土台を型付けし、参照を外す（今の `ExprKind::Index` と同じ `autoderef`）。組み込みの 6 型なら `E1005`（新しい kind `builtin_index_one_component`、"arrays, lists, Vec, and strings take one index; write 'xs[i][j]' for an element of a nested array"）。
2. モードを決める。
   - 範囲か単独 `..` の成分が 1 つでもあれば **窓のモード**。
   - そうでなければ、式の成分を型付けする（Self の `index_key_hint` が n 個組のタプルか配列のときは、その要素の型を各成分の期待型にする。`m[(i, j)]` と同じ型付けにするため）。成分の型が 1 つでも `Slice.Sel` なら **窓のモード**、そうでなければ **要素のモード**。型がまだ決まらない成分は位置として扱う。
   - どちらのモードでも、成分の型が `Slice.Axis` なら `E1003`（新しい kind `index_component_type`、"a bracket key takes positions, ranges, or Slice.Sel values, not Slice.Axis; keep Slice.Axis in the long form 't[[...]]'"）。`Slice.Axis` を新しい構文に混ぜない（決定事項 D4）。
3. **要素のモード**（位置だけ。`^k` を含む場合は Phase 1 では下の 6）:
   - Self と単一化した後のキーの型が n 個組のタプルと単一化できるインスタンスがあれば、キーはタプル `(c1, …, cn)`。`m[i, j]` は `m[(i, j)]` と同じ型付きの木になる（`HashMap<'key, _>` のように総称のキーは、Self から決まった `'key` で判断する）。
   - なければ、同じくキーが配列 `[_]` と単一化できるインスタンスがあれば、キーは配列リテラル `[c1, …, cn]`（スタックに置く）。
   - どちらもなければ `E1005`（kind `no_instance`。タプルのキーの型で "no instance for Index<Self, (T1 * … * Tn)>"）。
   - 負の整数リテラルの成分（`t[-1, 0]`）に `W1014` は出さない。要素のモードのキーは `Index` のキーで、意味はインスタンスが決めるからである（NC17 の Q1 の決定。D15）。
   - あとは今の `ExprKind::Index` と同じ（`resolve_index`、NC16 の `E1043`、A37 の土台の下げ方）。成分は選ばれたインスタンスのキーの型と単一化する。暗黙の整数変換はしない（`h[3, 4]` で `HashMap<(i32 * i32), _>` のリテラルは `i32`、`t[i, j]` で `i: i32` は `E1005`）。
   - `ref` の要否は NC16 の規則に従う（A37 の `ResolvedIndex::borrows_base` で決まる）。Output が土台を借りるインスタンス（利用者の型の `Index<Grid, (i64 * i64), Row<'a>>` など）は `ref` が要り、なければ `E1042`（`index_view_requires_ref`）。Output が土台を借りない要素の読み出し（Tensor・Matrix の `t[i, j]`）に `ref` を付けると、NC16 の `E1003`（kind `index_ref_on_value`。`ref t[mask]` と同じ）。help に "add a range component such as 'ref t[i, ..]' for a window" を足す（決定事項 D3）。
4. **窓のモード**:
   - 各成分を `Slice.Sel` の値にする（仕様 3.3 の対応表）。範囲の境界・刻み・`^k` のオペランドは `i64` と単一化する（`i32` なら `E1003` kind `type_mismatch`、help "Tensor and Matrix positions are i64; write 'a as i64'"）。式の成分は `Slice.Sel` ならそのまま、そうでなければ `i64` と単一化して `Sel.Item (Pos.Front e)`。どちらでもなければ `E1003`（kind `index_component_type`、"expected an i64 position or a Slice.Sel, found T"）。`Sel.Item (Pos.Front e)` にする成分 `e` が負の整数リテラル（値が 0 のリテラルを除く。`-0`・`-0x0` など）なら、キーを受けるインスタンスによらず `W1014`（kind `negative_literal_index`。`ref t[-1, ..]`、`ref t[Slice.rest(), -1]`）を出す（D15）。
   - キーの形: Self にキーが `[Slice.Sel]` のインスタンスがあれば配列リテラル（スタック）。なければ、キーが `Slice.Sel` だけの m 個組（m ≥ 成分の数。m = 1 は `Slice.Sel` そのもの）のインスタンスのうち最小の m のもの。足りない末尾は `Sel.All` で埋める（`ref m[0..<2]` は `(Span…, All)`）。どちらもなければ `E1005`（kind `no_instance`、"no instance for Index<Self, [Slice.Sel]>"）。
   - 窓の Output は土台を借りるので `ref` が要る。`ref` がなく構文で窓と分かる場合はパーサーの `E0002`（仕様 1）、型で窓と分かった場合（`t[Slice.rest(), 0]`）は NC16 の `E1042`（kind `index_view_requires_ref`）。範囲の成分を含むキーは構文で `ref` を要るので、土台を借りない Output を返す `[Slice.Sel]` のインスタンスは角括弧の構文では使えない（`ref` を付けると D3 の誤り。長い形 `g[[…]]` で使う）。
   - `ref mut`（`borrow == Some(true)`）は NC20 まで NC16 の `E1042`（kind `index_view_not_writable`）。今の `ref mut t[[…]]` と同じ扱い。NC20 が `IndexBorrowMut` の解決に置き換える。
5. **1 成分の窓**: edition 2028 から、組み込みでない型への 1 成分の `ExprKind::Slice`（`ref t[1..3]`、`ref t[^2..]`、`ref t[0 .. 2 .. 4]`）を、窓のモードの 1 成分として扱う（キーの形は 4 の規則）。キーの型が `Slice.Sel` の 1 成分の `ExprKind::Index`（`ref t[Slice.rest()]`）は、今の経路でキーをそのまま受けるインスタンスがなければ同じく窓のモードの 1 成分にする（全 edition。範囲の構文を使わないので）。リスト・`Vec`・`string`・`utf8string` は今の `E1005`（`slice_non_array`）のまま。2026・2027 では組み込みでない型にも今の `E1005` を出し、help に "in edition 2028, 'ref t[a..b]' takes a window of a type with an Index instance for [Slice.Sel]" を足す（決定事項 D8）。2028 の受理と 2026・2027 の拒否は、NC17 の Phase 2 が宣言する 2028 の差に含まれる（仕様 1 の edition の項）。
6. **`^k` を含む要素の読み出し**（`t[^0, 0]`、1 成分の `t[^0]`。組み込みの型の `xs[^k]` は NC17）: Phase 1 は `E1005`（新しい kind `from_end_element_read_unsupported`、"'^' positions in element reads of this type are not supported yet; compute the position from the shape, or take a window such as 'ref t[^0, ..]'"）。Phase 2 で Tensor・Matrix に対応する（仕様 7）。
7. **添字の代入**: `p[c1, …, cn] = v`（位置だけ）は、3 と同じ規則で `IndexMut` のキーをタプルか配列にし、今の `target[key] = rhs` と同じに下げる（評価順は右辺、土台、キーの成分を左から右。EVL-012 と A38）。edition の扱いは今の添字の代入と同じ。範囲を含む代入の対象は仕様 1 の `E0002`（NC20 は複合代入 `+=` などの対象にだけ広げ、`=` は `E0002` のまま。NC20 の D9）。

### 3. 厳格なキー `Slice.Sel`

#### 3.1 型

`std/Slice.tz` に足す。`Slice.Axis` には触れない。

```tsuzuri
/// A position on one axis of length n: `Front i` is i (0 is the first), `Back k` is n - 1 - k
/// (`^k`; `Back 0` is the last). Neither wraps nor clamps; a position outside the axis traps.
union Pos =
    | Front of i64
    | Back of i64
    deriving (Eq)

/// One end of a `Span`: none (the edge of the axis in the step's direction), or a position that
/// the span includes or excludes.
union Bound =
    | Unbounded
    | Included of Pos
    | Excluded of Pos
    deriving (Eq)

/// A strict selector for one component of a bracket key such as `ref t[i, a..b, ..]`.
union Sel =
    | Item of Pos
    | Span of (Bound * Bound * i64)
    | All
    | Rest
    | NewAxis
    deriving (Eq)
```

| case | 意味 | 軸 |
| --- | --- | --- |
| `Item p` | 1 つの位置 | 軸を除く（Matrix は長さ 1 で残す。3.5） |
| `Span (start, end, step)` | 範囲。`step != 0` | 残す |
| `All` | 軸全体（単独 `..`） | 残す |
| `Rest` | 必要な数だけの軸全体（NumPy の `...`）。キーに 1 つまで | 残す |
| `NewAxis` | 長さ 1・ストライド 0 の新しい軸。入力の軸を使わない | 足す |

#### 3.2 関数

| 関数 | 型 | 説明 |
| --- | --- | --- |
| `Slice.rest` | `pure def rest :: Sel` | `Sel.Rest`。成分として `ref t[Slice.rest(), 0]` と書く |
| `Slice.resolve_sel` | `pure def resolve_sel :: own i64 -> ref Sel -> Resolved` | 長さ `len` の軸に対して厳格に解く（3.4）。`Rest` は呼び出し側が展開するので、渡すとトラップ。対象 `ref Sel` が最後（D-59） |

新しい軸は union の case で `Slice.Sel.NewAxis` と書く（既存の `Slice.new_axis()` は `Slice.Axis` を返すので成分にならない。決定事項 D4）。そのほかの値も case で作れる（`Slice.Sel.Item (Slice.Pos.Back 0)`）。どの関数も `pure`。`tests/std_argument_order.rs` の行は `("Slice", subject(&["Axis", "Sel"]))` にする。

#### 3.3 構文からの対応

| 成分 | `Slice.Sel` |
| --- | --- |
| `e`（整数） | `Item (Front e)` |
| `^k` | `Item (Back k)` |
| `a..b`（`..=` は別名） | `Span (Included a', Included b', 1)` |
| `a..<b` | `Span (Included a', Excluded b', 1)` |
| `a<..b` | `Span (Excluded a', Included b', 1)` |
| `a<..<b` | `Span (Excluded a', Excluded b', 1)` |
| `..b` / `..<b` | `Span (Unbounded, Included b', 1)` / `Span (Unbounded, Excluded b', 1)` |
| `a..` / `a<..` | `Span (Included a', Unbounded, 1)` / `Span (Excluded a', Unbounded, 1)` |
| `..`（単独） | `All` |
| `a .. s .. b` / `a .. s ..< b` | `Span (Included a', Included b', s)` / `Span (Included a', Excluded b', s)` |
| `.. s ..` / `a .. s ..` / `.. s .. b` | `Span (Unbounded, Unbounded, s)` / `Span (Included a', Unbounded, s)` / `Span (Unbounded, Included b', s)` |
| `Slice.Sel` の式 | そのまま |

`a'` は境界が整数なら `Front a`、`^k` なら `Back k`。`<..` は刻みを取らない（NC17）。

#### 3.4 解き方（`resolve_sel` と窓のカーネルが共有する。D-66 の境界の規則）

- 位置: `Front i` は `i`、`Back k` は `n - 1 - k`。はみ出す計算はトラップと同じ扱いにする（正確な値が軸の外なので）。
- `Item p`: `0 ≤ p < n` なら `Resolved.Index p`、そうでなければトラップ。
- 正の刻み `s`: 半開の境界 (lo, hi) を `Unbounded` の始点 → 0、`Included p` → p、`Excluded p` → p + 1、`Unbounded` の終点 → n、`Included q` → q + 1、`Excluded q` → q で求める。`0 ≤ lo ≤ hi ≤ n` なら有効、そうでなければトラップ。位置は lo, lo + s, … < hi。個数は hi > lo なら (hi − lo − 1) / s + 1、そうでなければ 0。`Resolved.Slice (lo, 個数, s)`。
- 負の刻み `s`: 境界は `Included` か `Unbounded` だけ（`Excluded` はトラップ。字句で負の刻みと `..<`・`<..` を書いたときは NC17 の `E0002`）。始点 p は `Unbounded` なら N = n − 1、終点 q は `Unbounded` なら 0。軸を i ↦ N − i に反転して正の規則を当てたのと同じで、`p ≤ N` かつ `0 ≤ q ≤ p + 1` なら有効（`q = p + 1` は空）。位置は p, p + s, … ≥ q、個数は p ≥ q なら (p − q) / |s| + 1（`|s|` は `i64u` で計算）。`Resolved.Slice (p, 個数, s)`。
- 刻み 0: トラップ（`Mut` の単射性を守る）。std の `assert` なら `assertion failed`。`range step is zero` の文にするなら、std から `TrapKind::RangeStepZero`（`src/trap.rs` 62 行付近。今はコンパイラがループの下げ方でだけ出す: `src/llvm_control.rs` 129・245 行付近）を出す方法（刻み 0 の `for` を通すなど）を担当が決めて決定事項に書く。
- `All`: `Resolved.Slice (0, n, 1)`。`NewAxis`: `Resolved.InsertAxis`。
- トラップは std の `assert` で、報告の位置は添字の式（今の `t[[0]]` と同じ）。配列の境界検査とはトラップの文が違うが、トラップする条件は同じ。
- 刻み付きの範囲は書いた境界に境界の規則を当てる。`ref t[0 .. 4 ..< 10]` は n = 9 でトラップする（`for i in 0 .. 4 ..< 10` は 0・4・8 を回る）。「`for` と同じ位置を選ぶ」とは書かない。

基準表（1 次元、`xs = [10, 20, 30, 40, 50]` を `let t = Tensor.of_array [5] xs` などで作った Tensor。D-66 の基準表（NC17 の仕様 1）と同じ結果になること）:

| 書き方 | 結果 |
| --- | --- |
| `ref t[1..3]` / `ref t[1..<3]` / `ref t[1<..3]` / `ref t[1<..<3]` | `[20, 30, 40]` / `[20, 30]` / `[30, 40]` / `[30]` |
| `ref t[..2]` / `ref t[..<2]` / `ref t[2..]` / `ref t[2<..]` | `[10, 20, 30]` / `[10, 20]` / `[30, 40, 50]` / `[40, 50]` |
| `ref t[..^1]` / `ref t[1..^1]` / `ref t[^2..]` / `ref t[^2<..]` | `[10, 20, 30, 40]` / `[20, 30, 40]` / `[30, 40, 50]` / `[40, 50]` |
| `ref t[0..^0]` / `ref t[..<^0]` | 全部 / `[10, 20, 30, 40]` |
| `ref t[2..1]` / `ref t[2..<2]` / `ref t[..^5]` | `[]` / `[]` / `[]` |
| `ref t[3..1]` / `ref t[0..5]` / `ref t[^5..]` | トラップ |
| `ref t[0 .. 2 .. 4]` / `ref t[.. 2 ..]` | 位置 0, 2, 4 |
| `ref t[.. -1 ..]` / `ref t[3 .. -1 .. 1]` | 位置 4, 3, 2, 1, 0 / 3, 2, 1 |

#### 3.5 窓のカーネルと std のインスタンス

- Tensor: 非公開の `sel_layout`（`std/Tensor.tz`）がキー `ref [Slice.Sel]`、土台の offset・shape・strides から出力の (offset, shape, strides) を求める。データには触れない（NC20 が同じ関数から `Mut` を作る）。
  - `Rest` を「入力の軸の数 − 軸を使う成分（`Item`・`Span`・`All`）の数」個の `All` に展開する。`Rest` が 2 つ以上、軸を使う成分が軸の数より多い、出力の軸が 64 を超える、のどれでもトラップ。足りない末尾の軸は全体。
  - 出力の軸の数を先に数え、shape と strides を 1 回ずつ確保する（`Vec.push_mut` を使わない）。土台が `Tensor` のときは `dense_strides` を確保せず、ストライドを shape から計算する。キーを複製しない。NC16 が書き直した `slice_raw`（借りたキーと行優先の印で確保 2 回）と同じ作りにし、軸ごとの処理を共有できるなら共有する。
  - `View`・`MatrixView` を Self に持つ窓のインスタンスは、A37 の事実 `returns_contents` が立つ本文にする（一時値の窓の窓 `ref t[1..3, ..][0, ..]` のため）。
  - 空の入力・空の出力・刻みとストライドの積のはみ出しは今の `slice_raw` と同じに扱う（空ならストライド 0、積がはみ出せばトラップ）。
- Matrix: 非公開の `sel_layout_2d`（`std/MatrixView.tz`）が 2 つの `ref Slice.Sel` と土台の配置から (offset, rows, cols, row_stride, col_stride) を求める。`MatrixView` は常に 2 次元なので、`Item` はその軸を長さ 1 で残す（今の `validate_2d_axis` と同じ。`ref m[1, ..]` は 1 行の `MatrixView`）。負の刻みと `NewAxis` はトラップ（`MatrixView` は負のストライドを持てない。Tensor へ変換して使う）。`Rest` は `All` と同じ（2 成分のタプルでは常に 1 軸）。
- 足すインスタンス（キーは `ref` で受け、`Array.copy` しない）:

| インスタンス | 置き場 |
| --- | --- |
| `Index<Tensor<'a>, [Slice.Sel], View<'a>>` | `std/Tensor.tz` |
| `Index<View<'a>, [Slice.Sel], View<'a>>` | `std/Tensor.tz` |
| `Index<Mut<'a>, [Slice.Sel], View<'a>>`（`freeze` してから窓。A37 で土台の `Mut` に共有借用が付く） | `std/Tensor.tz` |
| `Index<Matrix<'a>, (Slice.Sel * Slice.Sel), MatrixView<'a>>` | `std/MatrixView.tz` |
| `Index<MatrixView<'a>, (Slice.Sel * Slice.Sel), MatrixView<'a>>` | `std/MatrixView.tz` |
| `Index<Mut<'a>, (Slice.Sel * Slice.Sel), MatrixView<'a>>`（`MatrixView.Mut`） | `std/MatrixView.tz` |

- 利用者の型も `[Slice.Sel]` か `Slice.Sel` だけのタプルのキーで窓のインスタンスを定義できる。A37 の orphan 規則（A37 の D4。承認済み（D-66 の委任で決定、2026-11-25））の範囲で、振り分けの位置（Self と Key）の少なくとも 1 つの頭がこのプログラムで宣言した型（レコード・名前付きの共用体・`extern type`）でなければならない。`Slice.Sel` は std の型、`[Slice.Sel]` とタプルは組み込みの型なので、Key だけでは条件を満たさない。利用者のモジュールの `Index<Grid, [Slice.Sel], GridView<'a>>`（`Grid` は利用者のレコード）は通り、`Index<Tensor<'a>, (Slice.Sel * Slice.Sel), View<'a>>` は `E1016`（kind `orphan_instance`）。

### 4. 成分の規則

- 整数（`Item`）の成分は Tensor の軸を除く。足りない末尾の成分は軸全体（窓のキー）。成分が軸の数より多いとトラップ。`Rest` は 1 つまで。
- 要素の読み出しで位置の数が軸の数と違うとトラップする（Tensor の軸の数は実行時に決まる。`std` の `assert` は文を持てないので、案内は文書に書く）。2 次元の Tensor の `t[0]` はトラップで、行は `ref t[0, ..]`。Matrix は軸の数が型で決まるので、`m[i]` は誤り。診断は今のまま: `MatrixView` が読み込まれていないプログラムは `E1003`（`expected (i64 * i64), found i32`。`index_key_hint` の候補が 1 つなので。`tests/index_class.rs:218` が固定している）、読み込まれていれば `E1005`（kind `no_instance`）。どちらにも help "write 'ref m[i, ..]' for a row" を足す。
- `ref t[i, j]`（位置だけ）は仕様 2.3 の誤り。単独 `..` だけのキーは `E0002`。
- 角括弧の中の刻みは Tensor（負の刻みは負のストライドの窓。NC02）と Matrix（正の刻みだけ）の窓に使える。配列に刻みを書くと NC17 の `E1005`（"array slices are contiguous"）。
- マスクと範囲の混在（`ref t[mask, 0..<2]`）は `E1003`（kind `index_component_type`）。マスクとファンシーインデキシングの意味は変えない。

### 5. 評価順序・所有権・借用

- 評価順は [A38](A38-index-evaluation-order.md) の規則に従う（決定事項 D7）。先に土台（場所の部分か一時値）を評価し、次にキーの成分を左から右へ評価する。範囲の成分の中は始点、刻み、終点の順で、`^k` はその被演算子を評価する。境界の解決（`^k` の位置、範囲の検査、`Slice.Sel` の解決）、インスタンスの呼び出し、トラップはその後に、インスタンスの中で行う。
- キーを評価する間は土台に共有の守りがある。キーは土台を読める。土台へ代入する・move する・排他借用するキーは `E1014`（kind `borrow_conflict`。A38）。
- `t[i, j]` と `t[(i, j)]` は同じ順なので IR も同じ。`MultiIndex` は NC16 の `Checker::index_access_typed` を通るので、A38 の下げ方をそのまま受け継ぎ、順を独自に組まない。
- 添字の代入 `p[i, j] = v` は右辺が先で、その後に土台、キーの成分の順（EVL-012 と A38）。`ref mut t[…]`（NC20）の排他の借用は、キーを評価した後に取る（A38）。
- 窓は土台の借用だけを持つ（A37 の下げ方。Output が region を持つので土台を場所の共有借用で下げる）。一時値の土台は A37 の `E1013`。ただし A37 の事実 `returns_contents` が立つインスタンス（`View → View`）は一時値の土台を許すので、`ref t[1..3, ..][0, ..]` は通る（NC16 の「`ref` が覆う」連鎖の規則で、`ref` は両方の窓に掛かる）。窓から要素を読むときは窓だけに `ref` を付ける: `(ref t[1..3, ..])[0, 1]`。`ref t[1..3, ..][0, 1]` は外側の要素の読み出しが `ref` の対象になるので `E1003`（`index_ref_on_value`）。
- キーはスタックに作る一時値で、借用は式の評価で閉じる（NC04 の規則）。インスタンスの本体がキーの借用を返すと `E1013`。
- `Mut` から取った窓が生きている間に同じ `Mut` を書くと `E1014`（A37 の場所の借用による。下の受け入れ条件の再現テスト）。

### 6. 例

2 次元（edition 2028。`let t = Tensor.init [3, 4] (\i -> i)`、行優先で `[[0, 1, 2, 3], [4, 5, 6, 7], [8, 9, 10, 11]]`）:

| 書き方 | 結果 |
| --- | --- |
| `t[1, 2]` | `6`（`t[(1, 2)]` と同じ） |
| `ref t[1, ..]` / `ref t[.., 0]` / `ref t[^0, ..]` | `[4, 5, 6, 7]` / `[0, 4, 8]` / `[8, 9, 10, 11]` |
| `ref t[0..1, 1..<3]` | shape `[2, 2]`、`[1, 2, 5, 6]` |
| `ref t[1<.., ^1..]` | shape `[1, 2]`、`[10, 11]` |
| `ref t[.. -1 .., ..]` | 行を逆順、`[8, 9, 10, 11, 4, 5, 6, 7, 0, 1, 2, 3]` |
| `ref t[.., .. 2 ..]` | `[0, 2, 4, 6, 8, 10]` |
| `ref t[Slice.rest(), 0]` | `[0, 4, 8]` |
| `ref t[Slice.Sel.NewAxis, ..]` | shape `[1, 3, 4]` |
| `ref t[3, ..]` / `ref t[.., 0..4]` / `ref t[0, .., 1]` | トラップ（行 3 がない / hi = 5 > 4 / 成分が軸より多い） |
| `t[0]` / `t[0, 1, 2]` | トラップ（位置の数が軸の数と違う） |
| `ref t[1, 2]` | 仕様 2.3 の誤り |
| `ref t[.., ..]` / `t[1..2, 0]` | `E0002` |

Matrix（`let m = Matrix.of_array 3 4 [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]`）: `m[1, 2]` は `6`。`ref m[1, ..]` は 1 行 4 列の `MatrixView`（`[4, 5, 6, 7]`）。`ref m[0..<2]` は 2 行 4 列。`ref m[.., .. -1 ..]` と `ref m[Slice.Sel.NewAxis, ..]` はトラップ。`ref m[0, 1, 2]` は `E1005`。

NumPy との対応（`docs` の表にも載せる）:

| NumPy | NC04 の長い形（NC16 の後） | このチケット |
| --- | --- | --- |
| `a[i, j]` | `t[[i, j]]` | `t[i, j]` |
| `a[1:4]` | `ref t[[Slice.range 1 4]]` | `ref t[1..<4]` または `ref t[1..3]` |
| `a[0]`（2 次元の行） | `ref t[[Slice.at 0]]` | `ref t[0, ..]`（`t[0]` はトラップ） |
| `a[:, 0]` | `ref t[[Slice.all(), Slice.at 0]]` | `ref t[.., 0]` |
| `a[-1, :]` | `ref t[[Slice.at (-1)]]` | `ref t[^0, ..]` |
| `a[:-1]` / `a[1:-1]` / `a[-3:]` | `Slice.until (-1)` / `Slice.range 1 (-1)` / `Slice.from (-3)` | `ref t[..^1]` / `ref t[1..^1]` / `ref t[^2..]` |
| `a[1:7:2]` / `a[::2]` / `a[::-1]` / `a[4:0:-1]` | `Slice.step 1 7 2` / `Slice.every 2` / `Slice.rev()` / `Slice.step 4 0 (-1)` | `ref t[1 .. 2 ..< 7]` / `ref t[.. 2 ..]` / `ref t[.. -1 ..]` / `ref t[4 .. -1 .. 1]` |
| `a[None, :]` / `a[..., 0]` | `Slice.new_axis()` / なし | `ref t[Slice.Sel.NewAxis, ..]` / `ref t[Slice.rest(), 0]` |
| `a[-1, 0]`（要素） | なし | Phase 2 で `t[^0, 0]` |
| `a[mask]` | `t[mask]` | 同じ（コピー、`ref` なし） |

NumPy と違う点（文書に書く）: 範囲外はトラップ（NumPy は空や切り詰め）。短い入力で `ref a[1..]`（n = 0）や `ref t[1..^1]`（n ≤ 1）がトラップする。`a[0]` は行ではなくトラップ。負の数は末尾から数えない（`^k`）。範囲の境界と窓の整数の成分の負の整数リテラル（`ref t[..-1, 0]`・`ref t[-1, ..]`）には `W1014` が出る。要素の読み出しの `t[-1, 0]` はトラップするが、`Index` のキーなので警告は出ない（D15）。

### 7. Phase 2: `^k` を含む要素の読み出し

- 位置だけのキーに `^k` が 1 つでもあれば、すべての成分を `Slice.Pos` にする（整数は `Pos.Front e`、`^k` は `Pos.Back k`）。キーの形は仕様 2.3 と同じ規則（n 個組のタプルか `[Slice.Pos]`）。
- std に足すインスタンス: Tensor・View の読み出しは `Slice.Pos`（1 軸）、`(Pos * Pos)`、`(Pos * Pos * Pos)`、`(Pos * Pos * Pos * Pos)`、`[Slice.Pos]`。Matrix・MatrixView は `(Pos * Pos)`。書き込みは Tensor の `Mut` と `MatrixView.Mut` に同じキーの `IndexMut`。確保 0 回で、位置ごとに `n - 1 - k` の計算と範囲の検査を足すだけ。
- 対応したら `from_end_element_read_unsupported` は出なくなる。`Slice.Pos` のインスタンスを持たない利用者の型は普通の `E1005`（help に `Slice.Pos` のキー）。

### 8. 診断のまとめ

| 条件 | コード | kind | 段 |
| --- | --- | --- | --- |
| 末尾のカンマ | `E0002` | `expected_token`（既存） | 構文 |
| 範囲か単独 `..` を含むのに `ref` がない | `E0002` | `slice_must_be_borrowed`（既存。文を一般化） | 構文 |
| 単独 `..` だけのキー | `E0002` | `slice_without_bounds`（既存） | 構文 |
| 2026・2027 で範囲か単独 `..` を含む多成分のキー | `E0002` | `range_form_needs_edition_2028`（NC17。文を足す） | 構文 |
| 組み込みの 6 型に 2 成分以上 | `E1005` | `builtin_index_one_component`（新規） | 型 |
| 成分が `Slice.Axis`、マスク、そのほか位置でも `Sel` でもない | `E1003` | `index_component_type`（新規） | 型 |
| 範囲の境界・刻みが `i64` でない | `E1003` | `type_mismatch`（既存） | 型 |
| キーに合うインスタンスがない | `E1005` | `no_instance`（既存） | 型 |
| 2026・2027 の組み込みでない型への 1 成分の範囲 | `E1005` | `slice_non_array`（既存。help を足す） | 型 |
| 土台を借りない Output の要素の読み出しに `ref`（`ref t[i, j]`） | `E1003` | `index_ref_on_value`（NC16。help を足す） | 型 |
| 型で窓と分かったのに `ref` がない | `E1042` | `index_view_requires_ref`（NC16） | 型 |
| 候補が 2 つ以上 | `E1043` | `ambiguous_instance_candidates`（NC16） | 型 |
| `ref mut t[…]`（NC20 まで） | `E1042` | `index_view_not_writable`（NC16） | 型 |
| `^k` を含む要素の読み出し（Phase 1） | `E1005` | `from_end_element_read_unsupported`（新規） | 型 |
| 範囲の成分の境界か `^` の被演算子が負の整数リテラル（`ref t[..-1, 0]`、`ref t[^-1, ..]`） | `W1014` | `negative_literal_range_bound`（NC17） | 型 |
| 窓のモードの整数の成分（`Sel.Item (Pos.Front e)` に下げるもの）が負の整数リテラル（`ref t[-1, ..]`、`ref t[Slice.rest(), -1]`）。キーを受けるインスタンスによらない。要素のモードのキー（`t[-1, 0]`）には出さない | `W1014` | `negative_literal_index`（NC17。NC18 が窓の成分へ広げる。D15） | 型 |

新しいコードは使わない。`W1014` の 2 つの行は、NC17 が足す負の整数リテラルの検出を使う（2 つの kind が同じ検出を共有する。範囲の成分には `Checker::negative_bound_warnings(&RangeKey)` を呼ぶ）。出す範囲は NC17 と同じ（利用者の関数とトップレベルのコード）で、修正案は付けない。`negative_literal_index` のメッセージは NC17 のもの（`'{text}' is a negative position; an index never counts from the end`）。help は窓の成分に合わせ、2028 は `^{k-1}`（例 `ref t[^0, ..]`）、2026・2027 は `Slice.Sel.Item (Slice.Pos.Back {k-1})` を示す（文言は既定案。テストはコードと kind を照合する）。新しい kind は G23 の登録簿（`src/diagnostic_kinds.rs`）に足し、`node scripts/gen-docs.mjs` で `compiler/diagnostics.md` の生成部分を更新する。メッセージは英語・小文字始まり・修正方法を含む（GUIDE-LITE §6）。

### 9. native／WASM・資源

- std だけの変更でランタイムは変えない。native と WASM で同じ結果（整数の計算だけ）。
- 要素の読み出し `t[i, j]` は NC16 のタプルのインスタンスと同じ IR で、確保 0 回。
- 窓 1 つの確保は多くとも 2 回（出力の shape と strides）。要素はコピーしない。HEAD の 4 回（上の ViewAlloc）は NC16 で 2 回になり、`Slice.Sel` の窓も同じ 2 回にする。文書では「要素をコピーしない。shape と strides の 2 つを確保する」と書き、確保のない「ゼロコピー」とは書かない。
- 軸は 64 本まで（今の Tensor と同じ）。

## 設計

### 1. 構文の節点

```rust
/// `base[c1, ..., cn]` with two or more components (NC18). `borrow` is `Some(false)` for
/// `ref`, `Some(true)` for `ref mut`, as `Slice::mutable` is for one-component slices.
MultiIndex {
    value: Box<Expr>,
    keys: Box<[IndexComponent]>,
    borrow: Option<bool>,
},
```

`IndexComponent`（`Expr(Expr)`・`Range(Box<RangeKey>)`・`Whole(Span)`）と `RangeKey { start, end, step, ends }` は NC17 が `src/syntax.rs` に定義し、`Parser::index_component` が 1 成分を読む（NC17 の設計の「データ構造」）。このチケットは定義を重ねず、カンマで繰り返して 2 成分以上を `MultiIndex` にする。`Expr` は位置（整数。NC17 の `ExprKind::FromEnd` の `^k` を含む）か `Slice.Sel` の値。`ExprKind` の variant は小さく保つ（大きい field は `Box`。GUIDE-LITE §10）。深さは成分の深さの最大 + 1（`Parser::make`）。

### 2. 段ごとの変更

| ファイル | 関数・場所 | 変更 |
| --- | --- | --- |
| `src/syntax.rs` | `ExprKind`、2 つの訪問（1014・1163 行付近の `Index` の腕の近く） | `MultiIndex` を足し、訪問で成分（NC17 の `IndexComponent`）を辿る |
| `src/parser.rs` | `index_or_slice`、`prefix`・`keyword_prefix` | 成分の並びを読む。2 成分以上は `MultiIndex`。`ref` の吸収に `MultiIndex` を足す。仕様 1 の `E0002` と edition の検査 |
| `src/syntax_codec.rs`・`src/frontend_cache.rs` | 符号化、`FRONTEND_FORMAT` | 新しい variant の番号を足し、`FRONTEND_FORMAT` を 1 上げて golden を更新する |
| `src/formatter.rs` | 式の出力 | `t[i, a..b, ..]` を `, ` 区切りで 1 行に出す。`ref` は節点から出す。`^` と刻みの空白は NC17 の規則 |
| `src/computation.rs` | `expand` | 成分を再帰的に辿る |
| `src/parse_canonical.rs` | 場所の判定（262・310 行付近） | `MultiIndex` の土台を `Index` と同じに扱う。範囲の成分は NC17 の非正準の記録（`Parser::record_slice_range` で `W1013` の記録）に渡す |
| `src/warnings.rs` | `ExprKind` の走査（588 行付近から） | 成分を辿る |
| `src/check.rs` | `ExprKind::Index` の組み込みでない経路、`ExprKind::Assign`、`slice` | NC16 が切り出した `Checker::index_access_typed(span, base, key, borrow)`（型付けした土台とキーから解決・`ref` の判定・A37 の下げ方・A38 の評価順の下げ方を行う）を `MultiIndex` から呼ぶ。`ExprKind::Index` も同じ関数を通るので、IR と評価順が同じになることを構造で保つ。`MultiIndex` のモードの決定とキーの形成（仕様 2）、1 成分の窓（仕様 2.5）、添字の代入（仕様 2.7）。窓のモードの範囲の成分（`IndexComponent::Range`）ごとに NC17 の `Checker::negative_bound_warnings` を呼ぶ（`W1014` の `negative_literal_range_bound`）。モードを決めた後、窓のモードで `Sel.Item (Pos.Front e)` にする整数の成分に、NC17 が `negative_literal_index` に使う検出関数を呼ぶ（`W1014`。D15） |
| `src/polymorph.rs` | `index_instances` を引く補助 | Self に「n 個組のタプルのキー」「配列のキー」「`[Slice.Sel]` のキー」「`Slice.Sel` のタプルのキー」のインスタンスがあるかを答える関数（`has_index_instances` と同じ作り。NC16 の巻き戻しを使う） |
| `src/diagnostic_kinds.rs` | 登録簿 | 仕様 8 の新しい kind |
| `src/stdlib.rs` | `sources_for` | 窓の角括弧の読み込みの契機（決定事項 D11） |
| `std/Slice.tz` | | `Pos`・`Bound`・`Sel`・`rest`・`resolve_sel` |
| `std/Tensor.tz` | | `sel_layout` と 3 つの `[Slice.Sel]` インスタンス |
| `std/MatrixView.tz` | | `sel_layout_2d` と 3 つの `(Slice.Sel * Slice.Sel)` インスタンス |

### 3. 型検査の手順（擬似コード）

```text
multi_index(value, keys, borrow):
  base = autoderef(check(value))
  if builtin6(base.ty): E1005 builtin_index_one_component
  if any key is Range or Whole: mode = Window
  else:
    hints = component_hints(index_key_hint(base.ty), keys.len)
    typed = [check(k, hint) for k, hint]
    if any typed is Slice.Axis or (neither integer nor Sel nor undecided): E1003 index_component_type
    mode = Window if any typed is Slice.Sel else Element
  Element:
    if any component is ^k: Phase 1 E1005 from_end_element_read_unsupported / Phase 2 Pos keys
    key = Tuple(typed) if has_tuple_key(base.ty, n) else Array(typed) if has_array_key(base.ty) else E1005
    call = index_call(base, key)          // the helper ExprKind::Index also uses
    ref rule of NC16: borrows_base and borrow is None -> E1042; not borrows_base and borrow is Some -> E1003 index_ref_on_value
  Window:
    sels = [to_sel(k) for k in keys]        // section 3.3; bounds and steps unify with i64
    W1014 negative_literal_index for each integer component that is a negative literal (D15)
    key = Array(sels) if has_sel_array_key(base.ty) else Tuple(pad_all(sels, m)) if has_sel_tuple_key(base.ty, m) else E1005
    if borrow == Some(true): E1042 index_view_not_writable (NC16; NC20 replaces)
    call = index_call(base, key)          // E1042 from NC16 when borrow is None
```

成分は 1 回だけ型付けし、`Slice.Sel` の値は型付きの木を直接作る（成分を二度検査しない）。型付けの順（土台、次に成分）は評価順を決めない。評価順は `index_call`（NC16 の `index_access_typed`）が A38 の規則で下げる。長い形との IR の一致はテストで確かめる。`Slice` が読み込まれていないプログラムで窓のモードになったら、`E1005`（"no instance for Index<Self, [Slice.Sel]>"）。

### 4. 生成 IR の形

- `t[i, j]`: スタックの 2 個組を `BorrowOperand` で貸し、`Index<Tensor<'a>, (i64 * i64), 'a>`（NC16）を呼ぶ。`t[(i, j)]` と同じ。
- `ref t[1..<3, 0]`: スタックの `[Slice.Sel]`（2 要素）を貸し、`Index<Tensor<'a>, [Slice.Sel], View<'a>>` を呼ぶ。`ref t[[Slice.Sel.Span (Slice.Bound.Included (Slice.Pos.Front 1), Slice.Bound.Excluded (Slice.Pos.Front 3), 1), Slice.Sel.Item (Slice.Pos.Front 0)]]` と同じ。

## 前提とする他チケットのインターフェース

- [A37](A37-index-base-place-borrow.md): `ResolvedIndex::borrows_base`（窓を返す `Index` か）。真なら場所の土台を `Borrow(base, false)`（場所の共有借用）で下げる。一時値の土台は、事実 `returns_contents` が立つインスタンス（std の `View → View`・`MatrixView → MatrixView`）のほかは `E1013`。orphan 規則（A37 の D4。承認済み（D-66 の委任で決定、2026-11-25）。A37・NC19・NC20 で 1 つの規則にし、A37 の手順 5 が入れる）: 利用者のモジュールの `Index`・`IndexMut` のインスタンスは、振り分けの位置（Self と Key）の少なくとも 1 つの頭がこのプログラムで宣言した型でなければ `E1016`（kind `orphan_instance`）。std のモジュールはコンパイラで検査しない。NC18 の 6 つの窓のインスタンスと Phase 2 の `Slice.Pos` のキーのインスタンスは std にあり、振り分けの位置に裸の型変数を持たないので、A37 の一覧のテスト `std_instances_with_open_dispatch_positions_are_listed` に足さない。`Index<Matrix<'a>, (Slice.Sel * Slice.Sel), MatrixView<'a>>` は `std/MatrixView.tz` に置き、今の `Index<Matrix<'a>, (Slice.Axis * Slice.Axis), MatrixView<'a>>`（461 行）と同じく `PAIR_TRIGGERS`（`src/stdlib.rs:500`）で opt-in の置き場所のテスト `opt_in_modules_are_reached_only_through_their_names`（708 行）を通る（Self の頭が `Matrix` で、キーの型が `Slice` を含むため。761 行の `pair_triggered`）。
- [A38](A38-index-evaluation-order.md): 添字の評価順の規則（仕様 5）。先に土台、次にキーの成分を左から右へ（範囲の成分の中は始点・刻み・終点、`^k` はその被演算子）。境界と `Slice.Sel` の解決、呼び出し、トラップはその後。キーを評価する間は土台に共有の守りがあり、土台へ代入・move・排他借用するキーは `E1014`（kind `borrow_conflict`）。代入は右辺が先（EVL-012）。A38 は NC18 の `b[c1, …, cn]` も規則の対象に含めて定める。NC18 は NC16 の `index_access_typed` を通してこの順を受け継ぐ。A38 の EVL と OWN の規則（番号は実装の時点の次の空き）の適合性テストに、多成分のキーの行を足す。
- [NC16](NC16-index-ref-and-zero-alloc-element-reads.md): `E1042`（`index_view_requires_ref`・`index_view_not_writable`）、`E1003`（`index_ref_on_value`）、`E1043`（`ambiguous_instance_candidates`）。`borrows_base` の添字に `ref` を要る規則と、型検査での `ref` の吸収・「`ref` が覆う」連鎖の規則。型付きの土台とキーを受ける入口 `Checker::index_access_typed(span, base, key, borrow)`（`MultiIndex` は成分から型付きのキーを作ってこれを呼ぶ。NC16 が用意していなければ停止条件 S1）。Tensor・View の `Index` と `Mut` の `IndexMut` のタプルのキー（`i64`・`(i64 * i64)`・`(i64 * i64 * i64)`・`(i64 * i64 * i64 * i64)`）とキーを借用で受けるインスタンス（確保 0 回）、書き直した `slice_raw`（窓の確保 2 回）。`resolve_index` の巻き戻し（`Inference::unifies`）による候補の試行。
- [NC17](NC17-edition-2028-inclusive-range-family.md): `Edition::E2028`。範囲の形（`<..`・`<..<`・終端を含む `..`）と角括弧の中の刻みの構文木（`RangeKey` など）。`^k` の構文木と、`,` の直後の前置 `^`。B3（`stop_at_slice_dotdot` の漏れ）の修正。表記の種類ごとの edition の範囲（`rejected_in`／`warned_in`）。`W1013`・`W1014` を記録する関数（`W1014` は `negative_literal_range_bound` と、NC17 の Q1 の決定で足す `negative_literal_index` の 2 つの kind。同じ検出関数）。docs/stability.md の 5 つ目の差（角括弧の中の範囲の構文の族の全体）の宣言に、NC18 の多成分のキーの範囲・`^k`・単独 `..` の成分と、組み込みでない型への 1 成分の範囲（D8）、それらの 2026・2027 の拒否を含めてあること（NC17 の Q2 の決定。含まれていなければ S1、2028 を含むリリースの後なら S7）。字句で負の刻みと `..<`・`<..` を組み合わせたときの `E0002`。配列の刻みの `E1005`。2026・2027 の `E0002`（kind `range_form_needs_edition_2028`。NC18 は文を 1 つ足して共有する）。D-66 の基準表を共有する参照表 `tests/range-reference-cases.mjs`（NC18 はテンソルの列と刻みの行の `pending: "NC18"` を外す）。edition 2028 の文書の例を検査する `scripts/check-docs.mjs` の例の属性 `edition=YYYY`。
- 完了済み: [NC04](_completed/NC04-generic-index-class-and-slice.md)（`Index`・`IndexMut`・`Slice`）、[NC02](_completed/NC02-tensor-core-views-and-broadcasting.md)（負のストライドの窓）、[NC05](_completed/NC05-tensor-shape-indexing-and-masking.md)（マスク）。

## 他チケットへ提供するインターフェース

- [NC20](NC20-compound-assignment-and-mutable-index-views.md) が使うもの:
  - `ExprKind::MultiIndex { borrow: Some(true) }`（`ref mut t[…]` の構文）。NC18 は型検査で NC16 の `E1042`（`index_view_not_writable`）を出し、NC20 が `IndexBorrowMut` の解決に置き換える。
  - 窓のモードのキーの形成（`[Slice.Sel]` か `Slice.Sel` のタプル）を行う型検査の補助関数。
  - 書き込み窓に使える `Slice.Sel` の case: `Item`、`Span`（刻みは 0 でない。0 は解くときにトラップ）、`All`、`Rest`、`NewAxis`（長さ 1 なのでストライド 0 でも単射）。単射な土台からはどの `Sel` のキーでも単射な窓になる。
  - データに触れない配置の関数 `sel_layout`（`std/Tensor.tz`）と `sel_layout_2d`（`std/MatrixView.tz`）。`Mut` はここから作る。
  - 範囲を含む角括弧の `E0002`（`slice_must_be_borrowed`）を出す 1 つの関数（`]` の次が `+=` などの複合代入のときだけ NC20 が保留する。`=` は保留しない。NC20 の D9）。
  - 位置だけのキーの添字の代入 `p[i, j] = v`（仕様 2.7）。
- [NC19](NC19-operator-extension-instances.md): 1 成分の範囲の窓を Tensor へ回すこと（例 `ref a[1..] - ref a[..^1]`）。
- 全般: `Slice.Pos`・`Slice.Bound`・`Slice.Sel`・`Slice.rest`・`Slice.resolve_sel`。窓の 6 つのインスタンス（仕様 3.5）。Phase 2 の `Slice.Pos` のキーのインスタンス。

## 実装手順

0. **変更**: なし。**内容**: 依存の完了と A38・NC16・NC17 のインターフェース（上の節）を確かめる。NC17 の Phase 2 が docs/stability.md に宣言した 5 つ目の差に、NC18 の 2028 の受理が含まれていることを読む（S1・S7）。`tests/fixtures/tensor/`・`tests/fixtures/matrix*`・`tests/index_class.rs` の例など、添字・スライスを使う既存の fixture の `tsuzuri build --emit llvm` を `target/scratch/nc18/before/` に保存する。使わない std の関数が IR に出ないことを確かめる（`Tensor` を使う小さなプログラムの IR に `slice` の定義がないこと）。**確認**: 一覧を記録。
1. **変更**: `std/Slice.tz`、`tests/std_argument_order.rs`、`tests/std_examples.rs`、`tests/fixtures/effects_sweep/Main.tz`。**内容**: `Pos`・`Bound`・`Sel`・`rest`・`resolve_sel`（仕様 3.1〜3.4）と `///` の例。**確認**: T0、`cargo nextest run --locked --test std_argument_order --test std_examples --test effects_alloc_conservative`、`target/debug/tsuzuri test --doc std`。`tests/multi_index.rs` の `resolve_sel` の表（正・負の刻み、空、トラップ、`Back k`、はみ出し）。
2. **変更**: `std/Tensor.tz`、`std/MatrixView.tz`。**内容**: `sel_layout`・`sel_layout_2d` と 6 つのインスタンス（仕様 3.5）。この段ではまだ長い形 `ref t[[Slice.Sel.…]]` で使う（全 edition）。**確認**: T0、長い形の基準表のテスト、`Alloc.since` で窓の確保が多くとも 2 回、`Mut` の窓と書き込みの `E1014`、`node scripts/slot.mjs -- cargo nextest run --locked --test tensor --test matrix_views --test index_class`。
3. **変更**: `src/syntax.rs`、`src/parser.rs`、`src/syntax_codec.rs`、`src/frontend_cache.rs`、`src/formatter.rs`、`src/computation.rs`、`src/parse_canonical.rs`、`src/warnings.rs`、`src/diagnostic_kinds.rs`。**内容**: `MultiIndex` の構文、`ref` の吸収、仕様 1 の `E0002` と edition の検査、整形。型検査は `MultiIndex` を受けたら一時的に `E1005` で拒否してよい。**確認**: T0、`tests/multi_index.rs` の構文のテスト（受理・拒否・kind）、`tests/editions.rs` の表、`tsuzuri fmt` の冪等性、codec の golden。
4. **変更**: `src/check.rs`、`src/polymorph.rs`、`src/diagnostic_kinds.rs`。**内容**: 仕様 2 の脱糖（補助関数の切り出し、要素・窓のモード、1 成分の窓、添字の代入、診断、窓の整数の成分の `W1014`）。評価順は NC16 の `index_access_typed` に任せ、A38 の順を保つ（仕様 5）。**確認**: T0、IR の一致のテスト（テスト計画）、評価順のテスト `multi_axis_key_evaluates_base_then_components`、段 0 の IR と比べて変わらない、T1。
5. **変更**: `src/stdlib.rs`。**内容**: Matrix の窓の読み込み（決定事項 D11）。**確認**: `Matrix` だけを名指すプログラムで `ref m[0..<2, 1]` が通る。`sources_for` のテスト（`Matrix` を名指さないプログラムの読み込みが変わらない）。
6. **変更**: `tests/fixtures/multi_index/`（`Main.tz` と、`edition = "2028"` の `Tsuzuri.toml`）、`tests/features.mjs`、NC17 の共通の参照表のテスト、文書。**内容**: E2E と文書（下の「ドキュメント」）。fixture のマニフェストで edition 2028 にする（`LATEST` が 2027 の間は、マニフェストがないと範囲の成分が `E0002` になる）。**確認**: T2（`cargo build` の後 `node tests/features.mjs target/debug/tsuzuri multi_index` と関係する suite、`node scripts/check-docs.mjs <変更したページ>`、`node scripts/gen-docs.mjs --check`、`node scripts/check-cli-docs.mjs`）。
7. **Phase 2（人間が求めたとき）**: 仕様 7。**確認**: `t[^0, 0]` の参照値と確保 0 回、Phase 1 の kind が出ないこと、T2。

## テスト計画

- **Rust（`tests/multi_index.rs`、新規）**:
  - IR の一致（`analyze_modules` に edition を指定して `llvm::emit` を比べる）: `t[i, j]` と `t[(i, j)]`、`m[i, j]` と `m[(i, j)]`、5 成分の `t[a, b, c, d, e]` と `t[[a, b, c, d, e]]`、`HashMap<(i32 * i32), i64>` の `h[3, 4]` と `h[(3, 4)]`、`ref t[1..<3, ^0, ..]` と `Slice.Sel` の長い形、`ref m[0..1, 2]` と長い形、1 成分の `ref t[1..3]` と `ref t[[Slice.Sel.Span (…)]]`、`mt[i, j] = v` とタプルか配列の添字の代入。IR は決定的（2 回出して同じ）。
  - 受理: 仕様 6 の表の型が通るもの。利用者の型の `[Slice.Sel]` インスタンス。連鎖した窓 `ref t[1..3, ..][0, ..]`。`(ref t[1..3, ..])[0, 1]`。
  - 拒否（コードと kind を照合）: 仕様 8 の各行。`Map<string, i64>` の `m["a", "b"]`（タプルのキーにも配列のキーにも合わない）の `E1005`。`t[i, j]` で `i: i32` の `E1005`。`ref t[mask, 0..<2]` の `E1003`。一時値の土台 `ref (mk 1)[0, ..]` の `E1013`（A37）。`Mut` の窓が生きている間の `Tensor.write … (ref mut m)` の `E1014`。Matrix の `m[0]` が、`Matrix` だけを読み込むプログラムで `E1003`、`MatrixView` も読み込むプログラム（`let a = Slice.all()` を足す）で `E1005`（`no_instance`）で、どちらも help "write 'ref m[i, ..]' for a row" を持つ（仕様 4）。
  - 警告: `ref t[..-1, 0]`（2028）が `W1014`（kind `negative_literal_range_bound`。NC17 の `negative_bound_warnings`）。`ref t[-1, ..]`（2028）と `ref t[Slice.rest(), -1]`（2026・2027・2028）が `W1014`（kind `negative_literal_index`。D15）。利用者のレコード `Grid` と `Index<Grid, [Slice.Sel], GridView<'a>>` の `ref g[-1, ..]` と、`(Slice.Sel * Slice.Sel)` のキーのインスタンスの `ref g2[Slice.rest(), -1]` にも出る（インスタンスによらない）。出さないもの: 要素のモードの `t[-1, 0]`・`m[-1, 0]`・`HashMap<(i64 * i64), i64>` の `h[-1, 0]`、`ref t[0 - 1, ..]`、負の変数の成分、`-0`・`-0x0`。
  - 評価順（A38）: 利用者のレコード `record Grid { w: i64, data: [i64] }` と `Index<Grid, (i64 * i64), i64>` で、`let mut g` の成分が土台へ代入する `g[{ g = Grid { w: 1, data: [0] }; 0 }, 0]` と、土台を排他借用する成分は `E1014`（kind `borrow_conflict`）。成分が土台を読む `g[g.w - 1, 0]` は受理する（仕様 5）。
  - `resolve_sel` と `sel_layout` の参照表（仕様 3.4 と 6。Rest の展開、`NewAxis`、軸の数を超える成分、空の軸、刻みとストライドの積のはみ出し）。
  - 確保: `Alloc.since` で `t[1, 2]` が 0 回（NC16 のインスタンス）、窓が多くとも 2 回。
- **`tests/editions.rs`**: 2026・2027・2028 で、`t[i, j]`（全 edition で受理）、`ref t[Slice.rest(), 0]`（全 edition）、`ref t[1..<3, 0]`・`ref t[.., 0]`・`ref t[^0, ..]`（2028 だけ。ほかは `E0002`）、1 成分の `ref t[1..<3]`（2028 だけ。ほかは `E1005`）、配列の `ref xs[1..<3]`（変わらない）。
- **E2E（`tests/features.mjs` の suite `multi_index`、`tests/fixtures/multi_index/Main.tz`）**: 仕様 3.4 の 1 次元の表を Tensor で（NC17 の共通の参照表のテストに Tensor の列を足す。配列・文字列と同じ位置を選ぶこと）、仕様 6 の 2 次元と Matrix の表、トラップの行（`traps`）、Phase 2 は `t[^0, 0]`。評価順のケース `multi_axis_key_evaluates_base_then_components`: 土台と各成分（範囲の始点・刻み・終点を含む）を、評価した順を記録する関数で作り、要素の読み出し `t[i, j]`、窓 `ref t[a..b, c]`、刻み付きの範囲の成分、添字の代入 `mt[i, j] = v` で、記録が「土台、成分を左から右（範囲の中は始点・刻み・終点）」（代入は「右辺、土台、成分」）になること（A38）。native と WASM、`-O0` と `-O3`、`live == 0`。fixture `tests/fixtures/multi_index/` に `[package]` と `edition = "2028"` の `Tsuzuri.toml`（`tests/fixtures/conventions/Tsuzuri.toml` と同じ形）を置く（`tests/features.mjs` はフィクスチャのディレクトリをビルドするので、edition はそのマニフェストで決まる。`LATEST` が 2027 の間は、マニフェストがないと範囲の成分が `E0002` になる）。全 edition の `t[i, j]` は 2026 の Rust テスト（`tests/editions.rs`）で確かめる。
- **既存テストへの影響**: `tests/index_class.rs`（NC04。218 行の Matrix の `m[0]` が `E1003` のままであることを含む）と `tests/tensor.rs`・`tests/matrix_views.rs`・`tests/slices.rs`・`tests/mutable_slices.rs` は変えずに通る。今 `E0002` の `t[1, 2]` を期待するテストがあれば、この変更で受理に変わる（`rg -n 'expected .\]' tests/` で確かめ、変わったものを完了報告に書く）。`OneRange` の `E1005` は 2026・2027 でだけ残る。
- **性能**: 速さの閾値は置かない。確保の回数（上）と、`t[i, j]` の IR が NC16 の要素の読み出しと同じことだけを確かめる。SIMD・並列の主張はしない。

## ドキュメント

- `_tsuzuri/language-reference/built-in-types-and-modules/tensor.md`: 「この記事のポイント」（13 行の `t[[i, j]]` の説明）、新しい節「角括弧の添字」（要素・窓・`Rest`・`NewAxis`・刻み・NumPy の対応表と違い）、「トラップ」（位置の数、範囲外、`t[0]`）、「計算量」（要素 0 回、窓 2 回の確保）、「できないこと」（Phase 1 の `^k` の要素の読み出し、マスクと範囲の混在）、「関連項目」。
- `slice.md`: `Slice.Sel`・`Pos`・`Bound`・`rest`・`resolve_sel`。`Slice.Axis` は丸める旧来の長い形で、角括弧の構文と `Sel` は厳格だと対比する。引数の順序の節。
- `matrix.md`・`matrix-view.md`: `m[i, j]`、Matrix の窓（`Item` は長さ 1 で残る、負の刻みと `NewAxis` はトラップ）。
- `array.md`「共有スライス」: カンマ区切りのキーは `Index` を持つ型のためのもので、配列は 1 成分（`xs[i, j]` は `E1005`）。
- `values-and-functions/op-and-expressions.md`: 優先順位の表の順位 16 の `[index]` に `[i, j]`、「メモリとその他の演算子」の `..` の行、「評価順序」の添字の行は 1 行のままにする（A38 が足す「添字 `xs[i]`・`t[k]`: 土台、キーの成分」）。その行に `t[c1, …]` と範囲の成分の中の順（始点、刻み、終点）を足し、別の行を作らない。
- `values-and-functions/tokens.md`: 角括弧の説明に `t[i, j]`。
- `types-and-type-inference/type-classes.md`: `Index` の行に、カンマ区切りのキーの脱糖（タプル・配列・`Slice.Sel`）。
- `compiler/diagnostics.md`: 新しい kind（`node scripts/gen-docs.mjs` の生成部分）。
- `W1014` の説明（NC17 が足す `src/explain/codes/W1014.md`）: `negative_literal_index` が窓の整数の成分にも出ることと、要素のモードのキー（`t[-1, 0]`）に出ない理由（`Index` のキーの意味はインスタンスが決める）を足す。
- `numpy-migration.md` があれば（NC07 が作る）、仕様 6 の対応表と違いの行。
- `docs/language.md`: §「式と評価順序」、§「配列・連結リストとレコード」の添字の段落（3068 行付近）、§Tensor（3527 行付近）。
- 変更が要らないページ: `values-and-functions/keywords.md`（予約語を足さない）。
- 範囲を含む例は edition 2028 の例として検査する（停止条件 S6）。

## 受け入れ条件

- [ ] `t[i, j]`・`m[i, j]`・`h[a, b]` が全 edition で動き、`t[(i, j)]`・`m[(i, j)]`・`h[(a, b)]`（タプルのインスタンスがなければ `t[[i, j]]`）と IR が同じ。
- [ ] edition 2028 で仕様 3.4 と 6 の表がすべて一致する（native/WASM × `-O0`/`-O3`）。NC17 の共通の参照表で、配列と 1 次元の Tensor が同じ位置を選ぶ。
- [ ] `ref t[1..<3, ^0, ..]` などの構文と `Slice.Sel` の長い形の IR が同じ。
- [ ] 範囲外を丸めず、負の位置を末尾から数えない。トラップの条件が D-66 の境界の規則（境界モデル）と一致する。
- [ ] 成分の暗黙の整数変換がない（`HashMap<(i64u * i64u), _>` のキーが変わらない、`t[i, j]` の `i: i32` は `E1005`）。
- [ ] 要素の読み出しの確保が 0 回、窓の確保が多くとも 2 回（`Alloc.since`）。
- [ ] 仕様 8 の診断がすべてコードと kind で照合される。新しいコードを使っていない。
- [ ] 2026・2027 で範囲を含む多成分のキーと組み込みでない型への 1 成分の範囲を拒否し、整数の位置と式だけのキー（`^k` を含まない）は受理する。
- [ ] `Mut` の窓が生きている間の書き込みが `E1014`、一時値の土台が `E1013`、連鎖した `View` の窓が通る。
- [ ] 評価順が A38 の規則（土台、キーの成分を左から右。代入は右辺が先）と一致する（`multi_axis_key_evaluates_base_then_components`、native/WASM × `-O0`/`-O3`）。土台を書き換える成分が `E1014`（`borrow_conflict`）。
- [ ] 窓のモードの整数の成分の負の整数リテラルに `W1014`（`negative_literal_index`）が出て、要素のモードのキーには出ない。
- [ ] `Matrix` だけを名指すプログラムで Matrix の窓が使える。`Matrix` を名指さないプログラムの読み込みと IR は変わらない。
- [ ] 新しい構文と新しいインスタンスを使わない既存の fixture の IR が変わらない（段 0 と比べる）。
- [ ] `tsuzuri fmt` が `MultiIndex` を冪等に整形する。`FRONTEND_FORMAT` を上げた。
- [ ] T2 と文書の検査が通る。

## 落とし穴

- `ref` の吸収は `prefix`（`&`）と `keyword_prefix`（`ref`）の 2 か所。片方だけだと `&t[i, ..]` が `Borrow(MultiIndex)` になる。
- 成分ごとに `stop_at_slice_dotdot` などを保存して戻す。戻さないと、成分の中の括弧・ブロック・ラムダの `for` が壊れる（B3 と同じ形）。
- `t[1..3, 0]` で `ref` がないのはパーサーの `E0002`、`t[Slice.rest(), 0]` で `ref` がないのは型の `E1042`。前者は構文、後者は型で分かる。同じテストに混ぜない。
- `Slice.Sel` の判定は型で行う。型がまだ決まらない成分（推論中の引数など）は位置として扱う。`Slice.Sel` を渡したいときは注釈を付ける。
- 要素のモードのキーの形は「Self に n 個組のタプルのインスタンスがあるか」で決め、成分の型では選ばない。Tensor は NC16 で `(i64 * i64)` などを持つので、2〜4 成分はタプル、5 成分以上は `[i64]`。
- Matrix の窓は 2 次元のまま。`ref m[1, ..]` は 1 行の `MatrixView` で、Tensor の `ref t[1, ..]`（1 次元）と軸の数が違う。
- Tensor の位置・境界・刻みは `i64` だけ。配列は全整数型を受けるので、移植したコードで `E1005`・`E1003` が出る。help の `as i64` を案内する。
- 評価順は A38 の順（土台、キーの成分を左から右）を保つ（決定事項 D7）。`MultiIndex` で順を独自に組まず、NC16 の `index_access_typed` を通す。モードを決めるための成分の型付けの順と、評価の順を混同しない。
- `W1014` の `negative_literal_index` は、モードを決めた後に窓のモードの整数の成分にだけ出す。`ref t[Slice.rest(), -1]` は型で窓になるので、構文だけで判定すると漏れる。要素のモードの `t[-1, 0]` に出すと D15 に反する。
- std の本体と std の `///` の例では 2028 の構文を使わない（std は `LATEST` で読む。NC17 の Phase 3 で `LATEST` が 2028 になるまで）。
- 読み込みはソースの語で決まる。窓の構文は `Slice` も `MatrixView` も名指さないので、契機を足さないと Matrix の窓が `E1005` になる（決定事項 D11）。
- `slice_raw` のように `Vec.push_mut` で shape を作ると確保が増える。出力の軸の数を先に数える。
- `FRONTEND_FORMAT` を上げないと、古いキャッシュが新しい節点を読めない。

## 対象外

- 書き込み窓 `ref mut t[…]` と複合代入 `+=`（[NC20](NC20-compound-assignment-and-mutable-index-views.md)）。範囲を含む角括弧への代入。
- View・スカラーとの演算子（[NC19](NC19-operator-extension-instances.md)）。
- コロン区切りのスライス（D-65、NC04 D3）、`...` のトークン、`end`・`begin` の語、NumPy の負の位置の折り返し、範囲外を空にする寛容なスライス。
- マスクと範囲を混ぜたキー、`ref t[mask]`（NC16）、角括弧の整数の配列によるファンシーインデキシング（`t[[0, 2]]` は今どおり座標の要素の読み出し。NumPy の `a[[0, 2]]` は `Tensor.take`）。
- 配列の刻み付きスライス（NC17 の `E1005`）、文字列の角括弧の範囲（借用の文字列の窓の型ができるまで `E1005`）。
- `Slice.Axis` の非推奨化と、長い形を新しい構文へ書き換える `fix`（丸めと負の位置の意味が違うので `certain` にできない）。
- 窓の shape・strides を確保しない表現（小さい軸の数のインライン化）。必要なら `_perfs/` の別チケット。

## 決定事項

- **D1（承認済み: D-66a・D-66d）**: 添字の角括弧にカンマ区切りの成分を許し、2 成分以上を `ExprKind::MultiIndex` で表す。`ref`／`ref mut` を吸収する。整数の位置と式だけのキーは全 edition、範囲・`^k`・単独 `..` を含むキーは 2028 から。1 成分の配列のスライスは `ExprKind::Slice` のまま。D-65 の「パーサーの文法を増やさない」を改めるが、コロンを足さず予約語も足さないので D-65 の理由は保つ。範囲・`^k`・単独 `..` を含むキーの 2028 の受理は、NC17 の Phase 2 が宣言した 2028 の差の実装なので、2028 を含むリリースの後に入っても凍結の規則に反しない（NC17 の Q2。決定（D-66 の委任で決定、2026-11-25））。リリースを止めて NC18 を待つ案と、プレビューの edition にする案は採らなかった。
- **D2（既定案）**: `^k` は位置の 1 つで、窓を作る印ではない。位置だけ（整数と `^k`）のキーは要素の読み出し、範囲・単独 `..`・`Slice.Sel` の成分を含むキーは窓。`^k` を含む要素の読み出しは Phase 2 で、`Slice.Pos` をキーの要素にしたインスタンス（仕様 7）で対応する。理由: `^` で窓に変わると `t[^0, 0]` が `ref` を要求し、`ref t[^0, 0]` が 0 次元の窓になって、`t[1, 2]` と意味が食い違う。要素の読み出しは確保 0 回の NC16 の経路で、`Slice.Pos` のインスタンスは軸の数ごとに倍になるので段を分ける。Phase 1 では `from_end_element_read_unsupported` で拒否し、窓（`ref t[^0, ..]`）を案内する。D-66 の設計の案の「`^` を含む → 窓」をこのとおり改める（D-66 に記録）。
- **D3（既定案）**: 土台を借りない Output の要素の読み出しに付けた `ref`（Tensor・Matrix の `ref t[i, j]`）は型の誤りで、NC16 が `ref t[mask]` に出すのと同じ `E1003`（kind `index_ref_on_value`）を使う。パーサーは整数の成分と `Slice.Sel` の値を区別できず（`ref t[Slice.rest(), 0]` は正しい窓）、位置だけのキーで土台を借りる利用者のインスタンスもあり得るので、構文の `E0002` にはしない。D-66 の設計の案の「`ref t[i, j]` は `E0002`」をこのとおり改める（D-66 に記録）。`ref mut` の窓も NC16 の `E1042`（`index_view_not_writable`）を使い、新しい kind を作らない。
- **D4（既定案）**: 厳格なキーは `Slice.Sel`（`Item of Pos`・`Span of (Bound * Bound * i64)`・`All`・`Rest`・`NewAxis`）、位置は `Slice.Pos`（`Front`・`Back`）、範囲の端は `Slice.Bound`（`Unbounded`・`Included`・`Excluded`）。関数は `Slice.rest` と `Slice.resolve_sel` だけで、ほかの値は case で書く。新しい構文は `Slice.Axis` の値を受けず（`E1003` `index_component_type`）、新しい軸は `Slice.Sel.NewAxis` と書く。`Slice.new_axis()` は `Slice.Axis` を返すので成分にならない（`Slice.Axis` を混ぜない、`Slice.Axis` を変えない、暗黙に変換しない、の 3 つを守るため）。`Pos`・`Bound`・`resolve_sel` は D-55 に足す。
- **D5（既定案）**: 要素のモードのキーは、Self に n 個組のタプルのキーのインスタンスがあればタプル、なければ配列。窓のモードは `[Slice.Sel]` があれば配列、なければ成分の数以上で最小の `Slice.Sel` の組（1 個組は `Slice.Sel` そのもの。足りない末尾は `All`）。成分の型でインスタンスを試し比べない（推論を単純に保ち、`m[i, j]` が必ず `m[(i, j)]` と同じになる）。
- **D6（既定案）**: Matrix の窓は常に 2 次元。`Item` はその軸を長さ 1 で残し、負の刻みと `NewAxis` はトラップ、`Rest` は `All`。今の `MatrixView.slice` と同じで、1 次元にしたいときは Tensor を使う。
- **D7（決定（D-66 の委任で決定、2026-11-25）。起票時は既定案「今の `Index` の経路の順のまま。配列とそろえるかは人間が決める」）**: 評価順は [A38](A38-index-evaluation-order.md) の規則に従う。先に土台、次にキーの成分を左から右へ（範囲の成分の中は始点・刻み・終点、`^k` はその被演算子）。境界と `Slice.Sel` の解決、インスタンスの呼び出し、トラップはその後。キーを評価する間は土台に共有の守りがあり、土台へ代入・move・排他借用するキーは `E1014`（kind `borrow_conflict`）。代入 `p[i, j] = v` は右辺が先（EVL-012）。`t[i, j]` と `t[(i, j)]` は同じ順なので IR も同じ。NC18 は A38 に依存し、A38 の後に入る。
  - 理由: 配列の `xs[i][j]` とテンソルの `t[i, j]` で副作用の順が違うと、利用者も AI も順を予測できない。A38 が組み込みとクラスのすべての添字の順を 1 つにするので、NC18 は規則を重ねず、NC16 の `index_access_typed` を通して受け継ぐ。
  - 選ばなかった案: 今の `Index` の経路の順（キーの成分、次に土台）を保つ。
- **D8（既定案）**: 組み込みでない型への 1 成分の範囲（`ref t[1..3]`）は 2028 から。2026・2027 で許すと、素の `..` の旧来の半開の意味が新しい型にも広がるので、今の `E1005` に help を足すだけにする。2028 の受理は NC17 の Phase 2 が宣言した 2028 の差の実装で、2026・2027 は今の拒否（`E1005`）のままなので、2028 を含むリリースの後に入っても凍結の規則に反しない（NC17 の Q2。決定（D-66 の委任で決定、2026-11-25））。
- **D9（既定案）**: `Tensor.Mut` と `MatrixView.Mut` にも読み出しの窓のインスタンスを足す。A37 で土台の `Mut` に共有借用が付くので、窓が生きている間の書き込みは `E1014` になる。NC20 の重なりの検査（`m[1..] += ref m[..^1]`）の前提。
- **D10（既定案）**: 成分を暗黙に変換しない。要素のモードは選ばれたインスタンスのキーの型、窓のモードは `i64`（`Slice.Pos` の中身）と単一化する。
- **D11（既定案）**: `sources_for` に契機を足す。`Matrix` が読み込まれ、ソースに範囲の演算子か単独 `..` を含む添字の角括弧（識別子・`)`・`]` の直後の `[`）があれば、`Slice` と `MatrixView` を読み込む。`Matrix` を名指さないプログラムの読み込みは変えない。`Matrix` だけのプログラムでも配列のスライスがあれば読み込みが増えるが、`MatrixView` を常に読み込むより狭い。判断が要るなら停止条件 S5。
- **D12（既定案）**: 窓は要素をコピーせず、shape と strides を 1 回ずつ確保する（多くとも 2 回）。確保をなくす表現は対象外。文書は確保の回数を書く。
- **D13（既定案）**: 境界は D-66 の境界の規則（境界モデル）だけを使う（仕様 3.4）。負の刻みは `Included` と `Unbounded` の端だけ（字句の組み合わせは NC17 の `E0002`、実行時に負になった刻みと `Excluded` はトラップ）。刻み付きの範囲も書いた境界に規則を当てる。
- **D14（既定案。2026-11-25 の統合で決めた）**: 2026・2027 の範囲を含む多成分のキーの拒否は、新しい kind を作らず NC17 の `range_form_needs_edition_2028`（`E0002`）に文を 1 つ足して共有する。新しい範囲の形・`^k`・角括弧の中の刻みの拒否と同じ kind なので、利用者と `tsuzuri explain` が「edition 2028 が要る」を 1 つの kind で調べられる。
- **D15（決定（D-66 の委任で決定、2026-11-25）。NC17 の Q1）**: 窓のモードの整数の成分（`Sel.Item (Pos.Front e)` に下げるもの）が負の整数リテラル（値が 0 のリテラルを除く。`-0`・`-0x0` など）なら、キーを受けるインスタンスによらず `W1014`（kind `negative_literal_index`）を出す。全 edition で既定で有効。要素のモードのキー（`t[-1, 0]`）には出さない。
  - 理由: 負の `Front` を作るのは構文で、`Slice.Pos.Front i` の文書の意味（先頭から i 番目、0 が最初）に負の数はない。同じ窓の範囲の境界にも `negative_literal_range_bound` が出るので、「負の数は末尾から数えない」を 1 つのコードで調べられる。要素のモードのキーは `Index` のキーで、意味はインスタンスが決める（`HashMap<(i64 * i64), _>` の `h[-1, 0]` は正しい読み出し）。Tensor・Matrix の要素のキーだけを判定するには std の型の名前を埋め込むか新しい印が要るので、出さない。誤りにはしない（組み込みの添字の `negative_literal_index` と同じ扱い。到達しないコードもある）。
  - 選ばなかった案: 要素のモードのキーにも出す、窓の成分にも出さない。
