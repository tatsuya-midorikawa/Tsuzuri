# NC16: 土台を借りる添字に `ref` を必須にし、要素の読み出しを確保なしにする（NC04 の後続）

| 項目 | 内容 |
| --- | --- |
| ID | NC16 |
| 優先度 | P0（A37・A38 の後、NC03・NC04 を含む最初のリリースより前に入れる。リリースの数え方は NC17 の Q3 の決定（コンパイラを利用者に配布する版。今は `vsc-v*` だけ。docs/stability.md に書く定義）。リリースの後に `ref` を必須にするには edition が要る） |
| 規模 | M（型検査での `ref` の吸収と診断 3 種、`resolve_index` の候補の試行と `E1043`、`std/Tensor.tz` の添字インスタンスの書き直しとタプルのキー、既存の利用箇所と文書の移行） |
| 依存 | [A37](A37-index-base-place-borrow.md), [A38](A38-index-evaluation-order.md)（どちらも手順 4 以降の依存。手順 1〜3 は並行してよい。着手条件） |
| 後続 | [NC18](NC18-multi-axis-bracket-indexing.md), ([NC19](NC19-operator-extension-instances.md)), [NC20](NC20-compound-assignment-and-mutable-index-views.md)（Phase 3） |
| 状態 | todo |
| 起票 | 2026-11-25（[GUIDE D-66](GUIDE.md#d-66-自然な添字範囲演算子d-65-の改訂a37nc16nc20)。HEAD `858a949c` で確認） |
| 承認 | `ref` 必須は承認済み（D-66d、決定事項 D1）。添字の評価順（D8）は決定（D-66 の委任で決定、2026-11-25）で、A38 が実装する。ほかは既定案（D2〜D7） |
| 手本にする既存実装 | `src/parser.rs` 5215・5287 行付近（`ref xs[a..b]` の `ref` を `ExprKind::Slice` に吸収する）、`std/Matrix.tz:277`（タプルのキーで確保なしに要素を読む `Index`）、`src/polymorph.rs:720` 付近の `Inference::unifies`（A32。単一化を試して巻き戻す）、`E1041` の注入の曖昧さ（[A24](_completed/A24-anonymous-unions.md)。既定を選ばず候補を並べる） |
| 主な影響ファイル | `src/check.rs`、`src/polymorph.rs`、`src/diagnostic_kinds.rs`、`src/explain.rs`、`src/explain/codes/E1042.md`（新規）、`src/explain/codes/E1043.md`（新規）、`std/Tensor.tz`、`tests/index_class.rs`、`tests/matrix_views.rs`、`tests/alloc_contracts.rs`、`tests/fixtures/alloc_contracts/Main.tz`、`tests/fixtures/tensor/Main.tz`、`tests/fixtures/matrix_view/Main.tz`、`tests/features.mjs`、`_tsuzuri/language-reference/types-and-type-inference/type-classes.md`、`_tsuzuri/language-reference/built-in-types-and-modules/tensor.md`、`_tsuzuri/language-reference/built-in-types-and-modules/slice.md`、`_tsuzuri/language-reference/built-in-types-and-modules/matrix-view.md`、`_tsuzuri/language-reference/ownership-and-memory/borrowing.md`、`_tsuzuri/language-reference/compiler/diagnostics.md`、`docs/language.md` |

## 実装者向け早見

- 範囲: (1) 結果（Output）が土台を借りる `Index` の使用に `ref` を必須にする（ないと `E1042`）。`ref t[[…]]` の `ref` は `ref xs[a..b]` と同じく式に吸収され、型は `Tensor.View<'a>`（`ref Tensor.View<'a>` ではない）。(2) 土台を借りない添字（要素・マスク・`Map`）に `ref` を付けると `E1003`、`ref mut` で窓を取ろうとすると `E1042`。(3) `resolve_index` が候補の先頭を黙って選ぶのをやめ、2 つ以上残れば `E1043`。候補の試行は `Inference` の複製ではなく単一化の巻き戻しで行う。(4) `Tensor`・`Tensor.View` の要素の読み出しと `Tensor.Mut` の書き込みを確保 0 回にし、軸の数 1〜4 のタプルのキー（`i64`・`(i64 * i64)`・`(i64 * i64 * i64)`・`(i64 * i64 * i64 * i64)`）を足す。(5) 既存の tests と文書の `t[[Slice…]]`・`m[(Slice…, …)]` を `ref` 付きに移す。
- 最初に読む: ① [A37](A37-index-base-place-borrow.md)（土台を場所として借りる下げ方と「Output が借用を持つ」の判定）② [A38](A38-index-evaluation-order.md)（添字の評価順。土台、キーの成分を左から右）③ 現状 1〜8 ④ 仕様 1〜4 ⑤ `src/check.rs` 14748〜14876 行付近（`ExprKind::Index` と `ExprKind::Borrow` の腕）。
- 最初に書くテスト: `tests/index_class.rs` に、`let v = t[[Slice.all()]]` が `E1042`、`let v: Tensor.View<i64> = ref t[[Slice.all()]]` が通る、`f (ref t[[1, 2]])` が `E1003`、`Index<Foo, i8, string>` と `Index<Foo, i16, string>` の `f[1]` が宣言の順によらず `E1043`。`tests/fixtures/alloc_contracts/Main.tz` に `t[[1, 2]]`・`t[(1, 2)]` の確保 0 回。
- 触ってはいけない: 組み込みの 6 型（配列・リスト・`Vec`・固定長配列・`string`・`utf8string`）の添字とスライスの意味と IR（`ref xs[i]` は今どおり要素への `ref T`）。A37 の土台の下げ方。`Tensor.at`・`Tensor.get`・`Tensor.write`・`Tensor.slice` など公開関数のシグネチャ（docs/stability.md）。要素の値とトラップの条件。A38 の添字の評価順（土台、キーの成分を左から右。D8）。
- 完了の見分け方: 次が native/WASM `-O0`/`-O3` で `8 120 110` を出し、`t[[1, 2]]`・`t[(1, 2)]` の読み出しが `Alloc.since` で 0 回になる。`ref` を外した `t[[Slice.range 1 3]]` は `E1042`、`ref t[[1, 2]]` は `E1003`。

```tsuzuri
let t = Tensor.init [3, 4] (\i -> i * 10)
let rows = ref t[[Slice.range 1 3]]
let e = t[[1, 2]] + t[(1, 2)]
let f = (ref t[[Slice.at 2]])[[3]]
$"{Tensor.count rows} {e} {f}"
```

## 目的

- NC04 の `t[[Slice…]]`・`m[(Slice…, Slice…)]` は、`ref` なしで土台を借りる窓を返す。配列のスライスは `ref xs[a..b]` で（`ref` がないと `E0002`）、マスク `t[mask]` は `ref` なしで新しい `Tensor` を返すので、書き方から「借用か、値か」が見分けられない。利用者の決定 D-66d で、結果が土台を借りる添字はどの綴り（`t[[…]]`・`m[(…)]`・NC18 の `t[a, b]`）でも `ref` を要ることにした。NC18 の `ref t[1..3, ..]` もこの票の規則と診断を使う。
- NC04 はどのリリースにも入っていない（現状 6）。今なら edition を使わずに拒否を足せる。リリースの後では docs/stability.md の「受理していたプログラムを拒否する変更は、新しい edition でだけ行います」に当たる。
- `t[[i, j]]` は配列の `xs[i]` と同じ見た目なのに、1 回の読み出しで 3 回確保する（現状 3）。NumPy 風の二重ループが要素ごとに確保を払う。Matrix の `m[(i, j)]` は 0 回で、同じ意味の書き方が別の性能の道を選んでいる（AGENTS.md「Equivalent builtin and operator forms must not accidentally select different performance paths」）。
- `resolve_index` は候補が 2 つ以上残ると宣言の順の先頭を選ぶので、インスタンスを宣言した順で結果の型と整数の幅が変わる（現状 4）。GUIDE §1 規則 2（暗黙の既定値を選ばない）に反する。NC19 の演算子の拡張インスタンスも同じ曖昧さの規則（`E1043`）を使うので、先にここで決める。

## 着手条件と停止条件

### 着手条件

- 手順 4 以降は [A37](A37-index-base-place-borrow.md) と [A38](A38-index-evaluation-order.md) が done であること。A37 からは土台の下げ方と「Output が借用を持つ」の判定を使う。A38 は、手順 4 が `index_access` に移す `ExprKind::Index` の腕（`src/check.rs` 14748 行付近）の評価順を変える（`IndexMut` の腕（14895 行付近）も変える）。腕を移すのは A38 の後にする（D8）。A38 は依存であり、推奨の順だけではない。手順 1〜3（`E1043` と、要素の読み出しの確保なし）は A37・A38 が変えるコード（`ExprKind::Index` の腕と `src/ownership.rs`）に触れないので、A37・A38 と並行して着手してよい。ただし手順 3 が書き直す `Index<View<'a>, [Slice.Axis], View<'a>>` の本文は、A37 の段 6 も書き換えうる（手順 3 の内容と落とし穴の注意を守る）。
- `ref` 必須（D1）は承認済み（D-66d）。評価順（D8）は決定（D-66 の委任で決定、2026-11-25）。ほかの決定は既定案で、承認待ちのものはない。
- 基準は `main` の最新の `ci.yml` の結果（`gh run list --branch main --workflow ci.yml --limit 1`。GUIDE-LITE §3）。

### 停止条件

- **S1** A37 が `ResolvedIndex::borrows_base`（宣言の Output に対する `Type::carries_loans` の結果。型変数は数えない。D2）を用意していない、または別の判定にしていて、どちらに合わせるかが一つに決まらない。`ref` の要否と土台の下げ方が食い違うと、`ref` のない借用や、借用しないのに `ref` を求める添字ができる。
- **S2** 組み込みの 6 型の添字・スライスの型付き IR か生成 IR、または F12 の範囲の証明が 1 byte でも変わる。
- **S3** `E1043` が std・examples・既存の tests・文書の例のプログラムを拒否する。先頭の候補に頼るコードが想定より多いので、期待型で絞る案（対象外）を相談する。
- **S4** NC04 を含むリリース（数え方は NC17 の Q3 の決定。今は `vsc-v*` のタグ）が出ていた（`git tag --contains bd994606` が空でない）。`ref` 必須は edition の要る変更になるので止める。
- **S5** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `858a949c` で確認）

再現は、記載のソースを `/tmp/tz-nc16-<名前>/Main.tz` に置き（ファイル名は大文字で始める）、`target/debug/tsuzuri run -O0 /tmp/tz-nc16-<名前>/Main.tz` で実行する。

1. **`ref` を式に吸収するのは配列のスライスだけ。** `prefix`（`src/parser.rs` 5215 行付近）と `keyword_prefix`（5287 行付近）は、`ref`・`ref mut`（2026 の `&`・`&mut` も）の対象が `ExprKind::Slice` のときだけ `ref` をスライスの `mutable` に吸収する。それ以外は `ExprKind::Borrow(value, mutable, notation)` になる。`index_or_slice`（4511 行付近）は `slice_context` を取り出して（4514 行）最後に戻す（4549 行）ので、後置の連鎖の 2 つ目の `[…]` も同じ `ref` を見る。`ref xs[1..][0..2]` は 2 つのスライスとして受理され、`.length` は `2`。括弧は AST に残らない（`ExprKind` に括弧のノードはない）。
2. **`Index` の脱糖は `ref` を見ない。** `src/check.rs` の `ExprKind::Index` の腕（14748 行付近）は、組み込みの 6 型でなければ `index_key_hint`（14769 行）でキーを検査し、`resolve_index`（14774 行）でインスタンスを選び、`Index.index (ref k) (ref c)` の呼び出しを作る。キーと土台はどちらも `TypedExprKind::BorrowOperand`（14825・14830 行）で、式の型は Output。`ref t[[…]]` は `ExprKind::Borrow`（14839 行の腕）で、一時値の Output の借用になる:

   ```tsuzuri
   let t = Tensor.init [3, 4] (\i -> i * 10)
   let v = ref t[[Slice.range 1 3, Slice.all()]]
   Tensor.count v
   ```

   は `error[E1013]: borrow requires a local place; bind the temporary with 'let' first`（2:13）。呼び出しの引数では、`Tensor.count (ref t[[Slice.range 1 3, Slice.all()]])` も `Tensor.count t[[Slice.range 1 3, Slice.all()]]` も `8` を出す（後者は引数の暗黙の借用）。`def f :: ref i64 -> i64` に `f (ref t[[1, 2]])` と渡すと、一時値の要素を借りて `61` を出す。`ref mut t[[…]]` は引数でも `E1013`。
3. **要素の読み出しが確保する。** std のインスタンス（`std/Tensor.tz` 1660〜1683 行）:
   - `Index<Tensor<'a>, [i64], 'a>` は `view self`（207〜208 行。形を `Array.copy` し、`dense_strides`（108〜109 行）が刻みの配列を作る）の後に `at (Array.copy coords)` を呼ぶ。
   - `Index<View<'a>, [i64], 'a>` は `at (Array.copy coords) self`。`at`（298 行）が `own [i64]` を取るため、借りたキーを複製している。借りたキーのまま使える `offset_of :: ref View<'a> -> ref [i64] -> i64`（282 行）がある。
   - `Index<Tensor<'a>, [Slice.Axis], View<'a>>` は `dense_strides` と `Array.copy axes` の後に `slice_raw`（1588 行。`own [Slice.Axis]` を取る）を呼ぶ。`Index<View<'a>, [Slice.Axis], View<'a>>` は `slice (Array.copy axes) self`。
   - `IndexMut<Mut<'a>, [i64], 'a>` は `write (Array.copy coords) value self`（`write` は 716 行。借りたキーで読める `offset_of_mut` が 612 行にある）。
   - 対照の `Index<Matrix<'a>, (i64 * i64), 'a>`（`std/Matrix.tz:277`）と `std/MatrixView.tz` の 455・461・468 行のインスタンスは、借りたタプルを `match` するだけ。

   `--allocator counting` の計測（トップレベルで `Alloc` を使うので `--allocator counting` が要る。ないと `E1018`。`-O0` と `-O3` で同じ値）:

   | 式 | `allocs` |
   | --- | --- |
   | `t[[1, 2]]`（`t` は 3×4 の `Tensor<i64>`。以下同じ） | 3（`view` の 2 と `Array.copy` の 1。リテラルのキーはスタックにあり確保しない） |
   | ループの中の `t[[i, j]]` を 12 回 | 36 |
   | `t[k]`（`let k = [1, 2]`） | 3 |
   | `v[[1, 2]]`（`v = Tensor.view (ref t)`。以下同じ） | 1 |
   | `Tensor.view (ref t)` | 2 |
   | `t[[Slice.range 1 3, Slice.all()]]` | 4 |
   | `v[[Slice.range 1 3, Slice.all()]]` | 3 |
   | `Tensor.slice [Slice.range 1 3, Slice.all()] (ref v)` | 3（リテラルの `own` の引数 1 と結果の 2） |
   | `m[[0, 1]] = 7`（`m: Tensor.Mut<i64>`） | 1 |
   | `mx[(1, 2)]`・`mx[(Slice.range 0 2, Slice.all())]`（`mx: Matrix<i64>`） | 0・0 |
   | `ref xs[1..<3]`（配列） | 0 |

   計測のソース（`/tmp/tz-nc16-alloc/Main.tz`。`target/debug/tsuzuri run -O3 --allocator counting /tmp/tz-nc16-alloc/Main.tz` が `elem=3 view=4 arrslice=0 loop12=36 60 8 2 660` を出す）:

   ```tsuzuri
   let t = Tensor.init [3, 4] (\i -> i * 10)
   let xs = new [i64](12, \i -> i)
   let s0 = Alloc.snapshot ()
   let e = t[[1, 2]]
   let u0 = Alloc.since s0
   let s1 = Alloc.snapshot ()
   let v = t[[Slice.range 1 3, Slice.all()]]
   let u1 = Alloc.since s1
   let s2 = Alloc.snapshot ()
   let w = ref xs[1..<3]
   let u2 = Alloc.since s2
   let s3 = Alloc.snapshot ()
   let mut acc = 0
   for i in 0 ..< 3 do
       for j in 0 ..< 4 do
           acc = acc + t[[i, j]]
   let u3 = Alloc.since s3
   $"elem={u0.allocs} view={u1.allocs} arrslice={u2.allocs} loop12={u3.allocs} {e} {Tensor.count (ref v)} {w.length} {acc}"
   ```

   tensor.md の 106 行は「`at` と `get` は、添字の配列を 1 つ確保します」と書き、添字の構文の費用は書いていない。
4. **`resolve_index` は先頭の候補を選ぶ。** `src/polymorph.rs` の `resolve_index`（2758 行）は、候補ごとに `inference.clone()`（2780 行）で `Inference` を丸ごと複製して Self とキーを単一化し、リテラルの制約に合わない候補を落とし（2796〜2827 行付近の `compatible`）、残った候補の先頭 `candidates[0]`（2832 行）を選ぶ。`index_key_hint`（2717 行。2734 行で複製）と `has_index_instances`（2687 行。2703 行で複製）も候補ごとに複製する。登録（`register_index_instance`、2495 行）は Self とキーの頭が単一化できる 2 つのインスタンスを `E1016` で拒否するので、候補が 2 つ以上残るのは、キーが接尾辞のないリテラルか、型の決まっていない式のときだけ。

   ```tsuzuri
   record Foo { v: i64 }
   instance Index<Foo, i8, string> {
       fn index _k _self = "i8"
   }
   instance Index<Foo, i16, string> {
       fn index _k _self = "i16"
   }
   let f = Foo { v: 1 }
   f[1]
   ```

   は `i8` を出し、2 つのインスタンスの順を入れ替えると `i16` を出す。型の決まっていないキーも先頭を選ぶ: `let t = Tensor.init [2, 2] (\i -> i)`、`let f = \k -> t[k]`、`f [1, 1]` は `3`（`k` を `[i64]` に決める）。`let f = \(k: [i64]) -> t[k]` と注釈しても `3`。
5. **`ref` のない窓の添字の利用箇所。** `rg -n '\[\[Slice\.|\[\(Slice\.' --glob '!_features/**' --glob '!target/**' .` は 9 行で、std・examples・`docs/`・`tests/*.mjs`・fixture にはない:
   - `tests/index_class.rs` 145 行（`let sub = t[[Slice.step 1 4 2, Slice.rev()]]`）、154 行（`let mv = m[(Slice.range 0 2, Slice.every 2)]`）、169 行（`E1013` を確かめる関数の本体 `local[[Slice.all(), Slice.rev()]]`）、178 行（`E1014` を確かめる `let sub = t[[Slice.all(), Slice.rev()]]`）
   - `tests/matrix_views.rs` 241 行（`row_huge[(Slice.at 2, Slice.all())]`）、248 行（`col_huge[(Slice.all(), Slice.at 2)]`）
   - `_tsuzuri/language-reference/built-in-types-and-modules/slice.md` 9 行・15 行、`tensor.md` 13 行

   これは HEAD の数。A37 が `ref` なしの窓のテストと fixture と文書の例を足す（A37 の「他チケットへ提供するインターフェース」）ので、手順 4 の着手時に同じ `rg` で数え直す。

   std の `///` の例にも、変数のキーで窓を取る添字はない（`rg -n '[a-z_)]\[(\[|\()' std/*.tz` は組み込みの配列の添字だけ）。`ref` を `Index` の式に付けた既存のコード（`rg -n 'ref (mut )?[a-z_][a-z_0-9]*\[(\[|\()'`）も、組み込みの配列だけ（`std/Matrix.tz` 167・180 行、`routine.md` 101 行など）。
6. **NC04 はリリースされていない。** NC04 の実装は `bd994606`。`git tag` はすべて `vsc-v*`（VS Code 拡張）で、`git tag --contains bd994606` は空。`Cargo.toml` の版は `0.1.0`。
7. **評価順が組み込みの添字と違う。** 組み込みの配列の `(arr ())[idx ()]` は `arr` を先に評価し（`Debug.print` の順が `base`、`key`）、`Index` の `(base ())[key ()]` はキーを先に評価する（`key`、`base`）。脱糖の引数の順が `Index.index (ref k) (ref c)`（D-59 で対象が最後）だから。`IndexMut` の `t[k] = v` も、引数が「キー、右辺、対象」の順（`src/check.rs` 14971〜14986 行）で、組み込みの `xs[i] = v`（EVL-012。右辺が先）と違う。[A38](A38-index-evaluation-order.md) が `Index`・`IndexMut` の経路を組み込みの添字の順（土台、キーの成分を左から右。代入は右辺が先）にそろえる。NC16 は A38 の後に入り、その順を保つ（D8）。
8. **診断コード。** `E1042`・`E1043` は `src/diagnostic_kinds.rs` に登録がなく、HEAD の GUIDE D-55 では予約（未割り当て）の行にあった。D-66 がこの 2 つを割り当て、D-55 に登録した（`E1042` は NC16 と NC18、`E1043` は NC16・NC19・NC20）。`docs/language.md` に `Index` の記述はない（`rg -n 'Index' docs/language.md` は該当なし）。

## 変えてはいけない意味

- 組み込みの 6 型の添字（全整数型のキー、境界の検査、F12 の範囲の証明による検査の除去、2027 の `xs[i] = v`）、`ref xs[i]`（要素への `ref T`）、`ref xs[a..b]`・`ref mut xs[a..b]` の意味と IR。
- `Index`・`IndexMut` が返す値と書く位置、トラップの条件（キーの成分の数が軸の数と違う、または成分が `0 ≤ c < 長さ` の外ならトラップ。今の `offset_of` の `assert` と同じ）。マスクの添字がコピーを返すこと（NC05）。
- A37 の土台の下げ方（Output が借用を持つなら `Borrow(base, false)`、要素を値で返すなら `BorrowOperand`）と、A37 が許す一時値の土台（`Tensor.View` から `Tensor.View` を返す std のインスタンスの連鎖）。`ref` の吸収は型だけを変え、型付き IR に `Borrow` の包みを足さない。
- A38 の添字の評価順（土台、キーの成分を左から右。キーを評価する間は土台に共有の守り。代入は右辺が先）。`index_access` への切り出しと `ref` の吸収で、順も守りも変えない。
- 公開関数 `Tensor.at`・`Tensor.get`・`Tensor.write`・`Tensor.slice` のシグネチャ（`own [i64]`・`own [Slice.Axis]`）と、それらが添字の配列を確保すること。

## 仕様

### 1. `ref` の規則（D-66d、全 edition）

用語:

- **添字クラスの式**: `c[k]`（`ExprKind::Index`）のうち、`c` の型（自動の参照外しの後）が組み込みの 6 型でなく、`Index` のインスタンスに解決されたもの。
- **土台を借りる添字**: 解決したインスタンスの宣言の Output が借用を運ぶ型であるもの。判定は [A37](A37-index-base-place-borrow.md) の仕様 1 と同じ `Type::carries_loans`（`src/check.rs:907`。参照、関数、借用を持つ `dyn`、それらを含むタプル・配列・レコード・共用体。region を持つレコード `View<'a> { r }` を含む）を宣言の Output（型変数は置き換えない）に当てたもので、A37 がその結果を `ResolvedIndex::borrows_base` として渡す（D2）。型変数（`'a`）を通して借用を運ぶだけの Output は借りない。std では `[Slice.Axis]` から `Tensor.View<'a>`、`(Slice.Axis * Slice.Axis)` から `MatrixView<'a>` を返す 4 つのインスタンスが借りる。座標のキーで `'a` を返すもの、マスクで `Tensor<'a>` を返すもの、`Map`・`HashMap` の `'value` は借りない。
- **`ref` が覆う**: `ref e`・`ref mut e`（edition 2026 の `&e`・`&mut e` も同じ）の `e` が添字の式（組み込みでも添字クラスでもよい）なら、`ref` は `e` と、`e` の土台をたどって続く添字の式（`e = a[k1][k2]` の `a[k1]`）を覆う。配列の `ref xs[1..][0..2]` で `ref` が 2 つのスライスに掛かるのと同じ（現状 1）。括弧は AST に残らないので `ref (t[[A]])` は `ref t[[A]]` と同じ。フィールド・呼び出し・`deref` で連鎖は切れる（`ref (f t[[A]])` の `t[[A]]` は覆われない）。

規則:

| 添字の式 | 書き方 | 結果 |
| --- | --- | --- |
| 土台を借りる | `ref` が覆う | 受理。`ref` は吸収され、式の型は Output そのもの（`ref t[[A]]` は `Tensor.View<i64>`）。型付き IR は A37 で下げた `ref` のない呼び出しと同じ |
| 土台を借りる | 覆われない（`t[[A]]`、`f t[[A]]`、`t[[A]][[1, 0]]` の内側の `t[[A]]`） | `E1042`（`index_view_requires_ref`） |
| 土台を借りる | `ref mut` が覆う | `E1042`（`index_view_not_writable`）。書き込める窓は [NC20](NC20-compound-assignment-and-mutable-index-views.md) |
| 借りない | 覆われない | 受理（今どおり） |
| 借りない | `ref`・`ref mut` の直接の対象（`ref t[[1, 2]]`、`ref t[mask]`、`ref mp[k]`、`ref t[[A]][[1, 0]]` の外側） | `E1003`（`index_ref_on_value`） |
| 借りない | 連鎖の内側で覆われる（`ref g[0][[A]]` の `g[0]`） | この規則では誤りにしない。外側の窓の土台が一時値になるので、A37 の規則が受理か `E1013` かを決める |
| 組み込みの 6 型 | どれでも | 今どおり（`ref xs[i]` は要素への `ref T`、`ref xs[a..b]` はスライス） |

- `ref` で受けた窓は値として扱う。`let rows = ref t[[A]]` の `rows` は `Tensor.View<i64>` で、`Tensor.count rows`（引数の暗黙の借用）も `Tensor.count (ref rows)` も通る。`Tensor.count (ref t[[A]])` は、窓の一時値を `ref View<'a>` の仮引数へ暗黙に借りて渡すので、今と同じく通る。
- 窓から要素を読むときは、窓だけに `ref` を付ける: `(ref t[[Slice.at 2]])[[3]]`。`ref t[[Slice.at 2]][[3]]` は外側の要素の読み出しを `ref` が対象にするので `E1003`。
- 所有権は A37、評価順は A38 のとおり（`ref` は型だけを変える）。一時値の土台（`ref (mk ())[[A]]`）は A37 の規則で `E1013`。
- edition による違いはない（[GUIDE D-66](GUIDE.md#d-66-自然な添字範囲演算子d-65-の改訂a37nc16nc20) の「edition の扱い」: NC16 は全 edition）。

例（`t = Tensor.init [3, 4] (\i -> i * 10)`、`m = Matrix.of_array 2 3 [1, 2, 3, 4, 5, 6]`）:

| 書き方 | 結果 |
| --- | --- |
| `ref t[[Slice.range 1 3]]` | `Tensor.View<i64>`（2×4） |
| `t[[Slice.range 1 3]]` | `E1042` |
| `ref mut t[[Slice.range 1 3]]` | `E1042`（`index_view_not_writable`） |
| `ref m[(Slice.range 0 2, Slice.every 2)]` | `MatrixView<i64>`（2×2） |
| `t[[1, 2]]`・`t[(1, 2)]` | `60`（確保 0 回） |
| `ref t[[1, 2]]`、`ref t[mask]` | `E1003` |
| `(ref t[[Slice.at 2]])[[3]]` | `110` |
| `ref t[[Slice.range 0 2]][[Slice.at 1]]` | `Tensor.View<i64>`（長さ 4。`ref` が両方の窓を覆う） |

### 2. 診断

メッセージは英語・小文字始まり・末尾にピリオドなし（GUIDE-LITE §6）。下の文言は既定案で、テストはコードと kind を照合する。kind は `src/diagnostic_kinds.rs` の `since "0.1.0"` の組に、(コード, id) の順で足す。

| コード | kind | 条件 | メッセージの例 | 位置 | help・修正案 |
| --- | --- | --- | --- | --- | --- |
| `E1042` | `index_view_requires_ref` | 土台を借りる添字を `ref` が覆わない | `this index gives Tensor.View<i64>, which borrows its base; write 'ref' before it` | 添字の式 | help `an index whose result borrows its base is written like an array slice, for example 'ref t[[...]]'`。修正案 `certain`: 添字の式を `(ref …)` で包む（2 つの挿入。意味も IR も変わらない） |
| `E1042` | `index_view_not_writable` | `ref mut` が土台を借りる添字を覆う | `'ref mut' cannot take a writable window from this index; it gives Tensor.View<i64>, a shared borrow of its base` | `ref mut` の式 | 修正案 `likely`: `ref mut` を `ref` にする |
| `E1003` | `index_ref_on_value` | 土台を借りない添字が `ref`・`ref mut` の直接の対象 | `this index gives a value of type i64, not a borrow of its base; remove 'ref'` | `ref` の式 | 修正案 `likely`: `ref `（`ref mut `）を消す。連鎖の内側が窓のとき（`ref t[[A]][[3]]`）は help `put 'ref' on the window only: '(ref t[[...]])[[...]]'` |
| `E1043` | `ambiguous_instance_candidates` | 候補の試行の後に Index（IndexMut）のインスタンスが 2 つ以上残る | `more than one Index instance fits Foo with this key: Index<Foo, i8, string>, Index<Foo, i16, string>; give the key a type` | キーの式 | related: 候補のインスタンスの宣言（`declared_here`、宣言の順、8 件まで）。help: キーが接尾辞のない数のリテラルを含めば接尾辞（`for example '1i8'`）、型の決まっていない式なら注釈（`\(k: [i64]) -> t[k]`） |

- `E1042`・`E1043` は新しいコード（D-55 の割り当ては D-66）。`E1043` の kind は演算子の解決（NC19）も使うので、メッセージの `Index` の部分だけを場所ごとに変え、kind と説明（`tsuzuri explain E1043`）は一般の「インスタンスの候補が 2 つ以上」として書く。
- `index_ref_on_value` は既存のコード `E1003`（型の誤り）に足す kind。D-66 の設計で「マスクに `ref` を付けると型の誤り」としたものに当たる。
- 値のない追加キー（`related`・`help`・`fixes`）は出さない（D-51）。

### 3. 候補の解決（`E1043`）

- `resolve_index`（Index と IndexMut の両方）は、宣言の順に候補を試す。試行は今と同じ条件（Self とキーを、インスタンスの型変数を新しい推論変数にした頭と単一化し、`compatible` でリテラルなどの未解決の制約を満たすか確かめる）で、`Inference` を複製せず `Inference::unifies`（A32）で単一化して巻き戻す。
- 残った候補が 0 個なら今どおり `None`（呼び出し側が `E1005` の `no_instance` か `index_non_indexable`）、1 個ならそれを選ぶ（今どおり）、2 個以上なら `E1043`。結果は宣言の順によらない。
- `E1016` の重なりの検査（登録）は変えない。
- `index_key_hint` と `has_index_instances` も複製をやめて同じ試行を使う（振る舞いは変えない）。
- 期待型（Output）で候補を絞ることはしない（対象外）。

### 4. 要素の読み出しと書き込み（確保 0 回）

`std/Tensor.tz` のインスタンスを次のとおりにする（D6・D7）。Copy の制約は今と同じ。

| Self | キー | Output | 本体 | `allocs` |
| --- | --- | --- | --- | --- |
| `Tensor<'a>` | `[i64]`（既存） | `'a` | 形から行優先のオフセットを直接求める。窓を作らず、キーを複製しない | 0（今 3） |
| `Tensor<'a>` | `i64`・`(i64 * i64)`・`(i64 * i64 * i64)`・`(i64 * i64 * i64 * i64)`（新規） | `'a` | 同上（軸の数を固定。ループなし） | 0 |
| `View<'a>` | `[i64]`（既存） | `'a` | `offset_of self coords` | 0（今 1） |
| `View<'a>` | 同じ 4 つのキー（新規） | `'a` | `offset + Σ c_d * strides[d]` | 0 |
| `Tensor<'a>` | `[Slice.Axis]`（既存） | `View<'a>` | `slice_raw` にキーを借りて渡し、土台の刻みの配列を作らない | 2（今 4。結果の形と刻み） |
| `View<'a>` | `[Slice.Axis]`（既存） | `View<'a>` | `slice_raw` にキーを借りて渡す | 2（今 3） |
| `Mut<'a>`（`IndexMut`） | `[i64]`（既存） | `'a` | `offset_of_mut` で借りたキーのまま書く | 0（今 1） |
| `Mut<'a>`（`IndexMut`） | 同じ 4 つのキー（新規） | `'a` | 同上（軸の数を固定） | 0 |

- 意味: どのキーでも、読む要素は `let v = Tensor.view (ref t)` の後の `deref (Tensor.at coords (ref v))` と同じ、書く位置は `Tensor.write coords value (ref mut m)` と同じ。トラップの条件も同じ（キーの成分の数 ≠ 軸の数、成分が範囲外）。2 次元のタプルを 3 次元のテンソルに使うとトラップする（`t[0]` は 1 次元のテンソルの要素で、2 次元のテンソルではトラップ。NC18 が `ref t[i, ..]` を案内する）。
- オフセットの計算は、各成分を境界と照合した後でだけ掛け算と足し算をする。テンソルの行優先は `offset = offset * shape[d] + c_d`（Horner）で、途中の値は要素数より小さいので溢れない。窓は `View` の不変条件（全要素が `data` の中にある）から溢れない。
- 軸が 0 本のテンソルは今どおり長さ 0 の `[i64]` のキーで読む（リテラル `[]` は型の注釈が要る。`t[[]]` は今も `E1004`）。タプルのキーは 1〜4 軸だけで、5 軸以上は `[i64]`。
- マスクの添字（2883〜2900 行）と、公開関数の確保は変えない。

## 設計

### データ構造

- 「土台を借りる」は A37 が足す `ResolvedIndex::borrows_base`（`src/polymorph.rs:903` 付近の `ResolvedIndex`。宣言の Output に対する `Type::carries_loans`）をそのまま読む。この票は判定を二重に持たない。
- 構文木は変えない（新しい `ExprKind` はない）。`syntax_codec.rs` の `FRONTEND_FORMAT` も変わらない。

### 段ごとの変更

| ファイル | 関数・箇所 | 変更 |
| --- | --- | --- |
| `src/polymorph.rs` | `resolve_index`・`index_key_hint`・`has_index_instances` | 候補の試行を `Inference::unifies` に。Self とキーの 2 組は 1 つのタプル型にまとめて渡す。試行の前に `quiet` を立て（PB08。失敗した単一化のメッセージを作らない）、`compatible` の「試行前の型」は試行の前に解決しておく。2 個以上で `E1043` を作る関数（例: `ambiguous_index_candidates`）を足す |
| `src/check.rs` | `ExprKind::Index` の腕（14748 行付近） | 本体を `Checker::index_access(expression, base, key, borrow: Option<bool>)` に移す。`borrow` は覆う `ref` の有無と種類。`borrow.is_some()` で土台も添字の式なら、土台を `self.expression` ではなく `index_access(…, borrow)` で検査する（連鎖を覆う）。解決の後に仕様 1 の表で `E1042` を判定する。`index_access` は、AST を型付けする部分と、型付きの土台とキーを受けて解決・`ref` の判定（`E1042`・`E1003`）・A37 の下げ方を行う `Checker::index_access_typed(span, base: TypedExpr, key: TypedExpr, borrow: Option<bool>)` に分ける（NC18 の `MultiIndex` は成分から型付きのキーを作ってこちらを呼ぶ） |
| `src/check.rs` | `ExprKind::Borrow` の腕（14839 行付近） | 対象が `ExprKind::Index` なら `index_access(…, Some(mutable))` を呼ぶ。添字クラスに解決したら、借りる Output はそのまま返し（`TypedExprKind::Borrow` で包まない）、借りない Output と `ref mut` は仕様 1 の表の診断にする。組み込みの 6 型なら、検査済みの値で今の `Borrow` の処理を続ける（`ref xs[i]` の型付き IR を変えない） |
| `src/check.rs` | `ExprKind::Assign` の `IndexMut`（14898 行付近） | 変えない（`resolve_index` の `E1043` だけが新しく届く） |
| `src/diagnostic_kinds.rs` | 登録簿 | `IndexViewRequiresRef`・`IndexViewNotWritable`（`E1042`）、`AmbiguousInstanceCandidates`（`E1043`）、`IndexRefOnValue`（`E1003`） |
| `src/explain.rs`、`src/explain/codes/` | 説明 | `E1042.md`・`E1043.md` を足し、表（367 行付近）に登録する |
| `std/Tensor.tz` | 1660〜1683 行のインスタンス、`slice_raw`（1588 行）、`slice`（1657 行） | 仕様 4。非公開の補助関数（例: `dense_offset :: ref [i64] -> ref [i64] -> i64`）を足す。`slice_raw` は `ref [Slice.Axis]` と、刻みを `stride_of shape axis` で求める行優先の印（`own bool`）を取る。公開の `slice` はシグネチャを保ち、`slice_raw (ref axes) …` を呼ぶ |

書き直しの形（`std/Tensor.tz`。名前と細部は実装者が決めてよい）:

```tsuzuri
// The row-major offset of `coords` in a tensor of `shape`; traps unless there is one entry per
// axis, each inside its extent. No allocation; the running value stays below the element count.
private def dense_offset :: ref [i64] -> ref [i64] -> i64
fn dense_offset shape coords =
    assert (coords.length == shape.length)
    let mut offset = 0
    for axis in 0i64 ..= (coords.length - 1) do
        assert (coords[axis] >= 0 && coords[axis] < shape[axis])
        offset = offset * shape[axis] + coords[axis]
    offset

instance Copy<'a> => Index<Tensor<'a>, [i64], 'a> {
    fn index coords self = self.data[dense_offset (ref self.shape) coords]
}

instance Copy<'a> => Index<Tensor<'a>, (i64 * i64), 'a> {
    fn index coords self =
        match coords with
        | (i, j) ->
            assert (self.shape.length == 2)
            assert (i >= 0 && i < self.shape[0] && j >= 0 && j < self.shape[1])
            self.data[i * self.shape[1] + j]
}

instance Copy<'a> => Index<View<'a>, [i64], 'a> {
    fn index coords self = self.data[offset_of self coords]
}
```

### 生成 IR

- `ref` の吸収は型だけを変える。`ref t[[A]]` の型付き IR と生成 IR は、A37 で下げた `t[[A]]`（`ref` なし）の呼び出しと同じ。
- 要素の読み出しは境界の比較と掛け算・足し算・読み込みだけになり、`@tz.alloc` を呼ばない。`--emit llvm` の出力は Clang の最適化の前の IR なので（option.md 128 行付近）、確保の呼び出しが消えたことは `clang -x ir -O3 -S -emit-llvm` を通した IR か逆アセンブルで確かめる（NC06 の設計の注意と同じ）。

## 前提とする他チケットのインターフェース

- [A37](A37-index-base-place-borrow.md): 土台を借りる `Index` の土台を `TypedExprKind::Borrow(base, false)` で下げ、要素を値で返す `Index` は `BorrowOperand` のままにすること。土台を借りる添字の一時値の土台は `E1013`、ただし `Tensor.View` から `Tensor.View` を返すような「Self の内側の region だけを返す」と証明できるインスタンスは一時値の土台を許すこと（連鎖 `ref t[[A]][[B]]` が通る）。「Output が借用を持つ」の判定を `ResolvedIndex::borrows_base`（宣言の Output に対する `Type::carries_loans`。型変数は数えない）として渡すこと（D2。違えば S1）。A37 の orphan 規則（A37 の D4。承認済み（D-66 の委任で決定、2026-11-25）。A37・NC19・NC20 で 1 つの規則）はこの票に影響しない。規則は利用者のモジュールのインスタンスだけを検査し、std のモジュールはコンパイラで検査しない。この票が足すインスタンスは `std/Tensor.tz` にあり、振り分けの位置（Self と Key）に裸の型変数を持たない（Self の頭は `Tensor`・`View`・`Mut`、Key は `i64` か `i64` のタプル）。そのため A37 の一覧のテスト `std_instances_with_open_dispatch_positions_are_listed` にも足さない。
- [A38](A38-index-evaluation-order.md): 添字の評価順の規則。先に土台（場所の部分か一時値）、次にキーの成分を左から右へ評価し、境界の解決・インスタンスの呼び出し・トラップはその後に行う。キーを評価する間は土台に共有の守りがあり、土台へ代入・move・排他借用するキーは `E1014`（kind `borrow_conflict`）。代入 `b[k] = v` は右辺が先（EVL-012）。`ExprKind::Index` と `IndexMut` の腕がこの順で下がっていること。土台が A38 の設計の「単純な土台」なら型付き IR は変えず、守りだけが `src/ownership.rs` にある（キーの成分がすべて定数の式なら、守りも置かない）。土台が A38 の設計の「単純な土台」でなく、キーの成分が定数の式でないときだけ並べ替える（組み込みの添字の段と、可変な束縛の参照の参照外しを含む。読み出しは共有の再借用を束縛する。A38 の D6・D9）。順を固定するテスト（`tests/index_class.rs` の `index_evaluates_the_base_before_the_key`・`index_mut_evaluates_the_right_side_first`・`index_key_cannot_write_its_base`・`index_key_may_read_its_base`・`index_temporary_base_is_dropped_on_early_exit_in_key`）は A38 が足す。
- [NC04](_completed/NC04-generic-index-class-and-slice.md)（done）: `Index`・`IndexMut` の 3 引数ヘッダ、`index :: ref Key -> ref Self -> Output`、`Slice.Axis`。
- [NC02](_completed/NC02-tensor-core-views-and-broadcasting.md)・[NC05](_completed/NC05-tensor-shape-indexing-and-masking.md)（done）: `Tensor.View`・`Tensor.Mut` の不変条件、マスクのインスタンス。
- A32（done）: `Inference::unifies`。

## 他チケットへ提供するインターフェース

- **`ref` の規則と `E1042`**: `Checker::index_access(expression, base, key, borrow)` と、型付きの土台とキーを受ける `Checker::index_access_typed(span, base, key, borrow)`（判定には A37 の `ResolvedIndex::borrows_base` を使う）。`index_access_typed` は A38 の評価順の下げ方（土台が A38 の「単純な土台」でなく、キーの成分が定数の式でないときに、土台の部分を先に評価する並べ替え。A38 の設計の表）も含み、`src/ownership.rs` はその型付き IR に A38 の共有の守りを当てる。これを呼ぶ経路はどれも同じ順になる。[NC18](NC18-multi-axis-bracket-indexing.md) の `MultiIndex` は、成分から型付きのキー（タプル・配列・`Slice.Sel` の木）を作って `index_access_typed` を呼び、パーサーが吸収した `ref`・`ref mut` をその `borrow` に渡し、`ref t[a, b]` の判定と診断（`index_view_requires_ref`・`index_ref_on_value`）を共有する。[NC20](NC20-compound-assignment-and-mutable-index-views.md) は、`ref mut` で覆われた土台を借りる添字（今は `index_view_not_writable`）のうち `IndexBorrowMut` のインスタンスを持つものだけを受理に変える。また、複合代入の対象の土台が組み込みでない型の添字の式（`t[[A]][[B]] += u` の `t[[A]]`）のときは、内側を書き込み窓として解決するので `index_view_requires_ref` を出さない。内側に `IndexBorrowMut` のインスタンスがあれば（`[Slice.Sel]` のキーの `Tensor`・`Tensor.Mut`）`E1013`（`borrow_of_temporary`）、土台を借りる `Index` でインスタンスがなければ `index_view_not_writable` を出す（位置と文言は NC20 の §6。`ref mut` のない形なので修正案は付けない。NC20 の D14）。
- **`E1043`（`ambiguous_instance_candidates`）**: [NC19](NC19-operator-extension-instances.md) が演算子の拡張インスタンスの候補が 2 つ以上のときに使う。候補を `Inference::unifies` で試す形も NC19 の手本になる。
- **タプルのキーのインスタンス**: `Tensor<'a>`・`View<'a>` の `Index` と `Mut<'a>` の `IndexMut` に `i64`・`(i64 * i64)`・`(i64 * i64 * i64)`・`(i64 * i64 * i64 * i64)`。NC18 は、成分が整数だけの `t[i, j]` をこのタプルのキー（5 軸以上は `[i64]`）へ脱糖する。軸の数が合わないとトラップする。
- **`slice_raw` の借りたキー**: `std/Tensor.tz` の非公開の `slice_raw` が `ref [Slice.Axis]` と行優先の印を取る形。NC18 の `Slice.Sel` のインスタンスが同じ形の非公開の関数を使う。

## 実装手順

1. **変更**: なし。**内容**: 基準を取る。現状 5 の `rg` で利用箇所の 9 行を控える。`tests/fixtures/slices/Main.tz`・`tests/fixtures/mutable_slices/Main.tz` と、組み込みの添字だけを使う小さなプログラムの `--emit llvm -O0`・`-O3` の出力を `target/scratch/nc16/` に保存する（S2 の比較用。作業の終わりに消す）。現状 3 の計測のソースで確保の表を取り直す。**確認**: 表が現状 3 と一致する。
2. **変更**: `src/polymorph.rs`、`src/diagnostic_kinds.rs`、`src/explain.rs`、`src/explain/codes/E1043.md`、`tests/index_class.rs`。**内容**: 仕様 3（試行の巻き戻しと `E1043`）。**確認**: T0、`cargo test --locked --test index_class`（新しい `E1043` のテストと既存のテスト。`running N tests` の N を控える）、T1。既存のテストが `E1043` で落ちたら S3。
3. **変更**: `std/Tensor.tz`、`tests/fixtures/alloc_contracts/Main.tz`、`tests/alloc_contracts.rs`、`tests/fixtures/tensor/Main.tz`、`tests/features.mjs`。**内容**: 仕様 4（既存のインスタンスの書き直し、タプルのキーのインスタンス、`slice_raw` の借りたキーと行優先の印）。`Index<View<'a>, [Slice.Axis], View<'a>>` の本文は、A37 の事実 `returns_contents` が立つ形に保つ（参照先のフィールド `self.data` を値として渡し、`ref self.data` や `freeze` を作らない）。A37 が先に入っていれば、A37 の段 6 の呼び出しを新しい `slice_raw` の引数に直し、`tests/index_class.rs` の `std_view_chains_on_temporary_views_stay_accepted` を確認に加える。**確認**: T0、`cargo test --locked --test alloc_contracts --test index_class --test tensor --test tensor_ops --test matrix_views`、`cargo build && node tests/features.mjs target/debug/tsuzuri tensor`、`target/debug/tsuzuri test --doc std`、T1。
4. **変更**: `src/check.rs`、`src/diagnostic_kinds.rs`、`src/explain.rs`、`src/explain/codes/E1042.md`、`tests/index_class.rs`、`tests/matrix_views.rs`、`tests/fixtures/tensor/Main.tz`、`tests/fixtures/matrix_view/Main.tz`（と、先に入っていれば NC06・NC19 のテスト）。**内容**: A37 と A38 が done になってから、仕様 1・2 の `ref` の規則と診断。`ExprKind::Index` の腕の本体を `index_access` に移すときは、A38 が入れた評価順の下げ方（土台を先に評価する並べ替え）をそのまま移し、`IndexMut` の腕の「右辺、土台、キー」の順も変えない（D8）。着手時に `rg -n '\[\[Slice\.|\[\(Slice\.' --glob '!_features/**' --glob '!target/**' .` が挙げるすべての箇所を `ref` 付きにする。HEAD の現状 5 の 9 行に加え、A37 が足した `tests/index_class.rs` の再現 1〜3・窓の連鎖・総称関数 `first_row`／`corner`、`tests/fixtures/tensor/Main.tz`・`tests/fixtures/matrix_view/Main.tz` の追加ケース、先に入っていれば NC06・NC19 のテストも含む（文書の例は手順 5）。窓の後に要素を読む連鎖は `(ref t[[A]])[[i, j]]` と書く（`ref t[[A]][[i, j]]` は仕様 1 の表で `E1003`。`corner` の `t[[Slice.range 1 3]][[0, 0]]` は `(ref t[[Slice.range 1 3]])[[0, 0]]`）。窓の窓は `ref t[[A]][[B]]` で両方を覆う。一時値の土台の `E1013`・場所の借用の `E1014` を確かめるテスト（HEAD の 169・178 行、A37 の再現）は、`ref (mk 100)[[…]]` のように `ref` を足しても同じコードを確かめ続けること。**確認**: T0、`cargo test --locked --test index_class --test matrix_views --test slices --test mutable_slices`（A38 の順のテストを含む）、`cargo build` の後 `node tests/features.mjs target/debug/tsuzuri tensor` と `node tests/features.mjs target/debug/tsuzuri matrix_view`、手順 1 で保存した IR と `diff` して差がないこと（S2）、T1。
5. **変更**: 文書（下の「ドキュメント」）。**内容**: 動作が固まったところで一度だけ直す。A37 が書いた文書の行（`tensor.md`・`slice.md`・`type-classes.md`・`borrowing.md`）の窓の添字も `ref` 付きにする。**確認**: `node scripts/gen-docs.mjs`、`node scripts/gen-docs.mjs --check`、`node scripts/check-cli-docs.mjs`、`node scripts/check-docs.mjs <変更したページ>`。
6. **変更**: なし。**内容**: T2。**確認**: `cargo nextest run --locked`、`cargo build` の後、`node tests/features.mjs target/debug/tsuzuri tensor`、`node tests/features.mjs target/debug/tsuzuri matrix`、`node tests/features.mjs target/debug/tsuzuri matrix_view`、`node tests/features.mjs target/debug/tsuzuri slices` を 1 つずつ（suite は 1 回に 1 つ）、手順 5 の文書の検査。Clang `-O3` 後の IR で、`t[(i, j)]`・`t[[i, j]]` を回すループに `@tz.alloc` の呼び出しがないことを見る（生成 IR の節）。`target/scratch/nc16/` を消す。

## テスト計画

- **Rust（`tests/index_class.rs`）**:
  - 受理: `let v: Tensor.View<i64> = ref t[[Slice.all()]]`、`Tensor.count (ref t[[Slice.range 1 3]])`、`let rows = ref t[[A]]` の後の `Tensor.count rows`、`ref m[(Slice.range 0 2, Slice.every 2)]`、`ref t[[Slice.range 0 2]][[Slice.at 1]]`（連鎖）、`(ref t[[Slice.at 2]])[[3]]`、edition 2026 の `&t[[Slice.all()]]`（`analyze_with_edition` で `Edition::E2026`）、利用者のインスタンス `Index<Holder, i64, R>`（`record R { r } { b: ref { r } [i64] }`）の `ref h[0]`。どれも `emits` で IR が決定的なこと。
  - `E1042`（`index_view_requires_ref`）: `let v = t[[Slice.all()]]`、`m[(Slice.all(), Slice.all())]`、変数のキー `t[axes]`、引数 `Tensor.count t[[Slice.all()]]`、連鎖の内側 `t[[Slice.range 1 3]][[1, 0]]`、`ref (f t[[A]])` の内側、上の `Holder` の `h[0]`。
  - `E1042`（`index_view_not_writable`）: `let mut t = …` の後の `ref mut t[[Slice.all()]]`。
  - `E1003`（`index_ref_on_value`）: `f (ref t[[1, 2]])`、`ref t[mask]`、`ref mp["tea"]`（`Map`）、`ref t[[Slice.at 2]][[3]]`。
  - `E1043`: `Index<Foo, i8, string>` と `Index<Foo, i16, string>` の `f[1]`（宣言の順を入れ替えた 2 つで同じコード、`related` が 2 件）。`f[1i16]` は受理。`\k -> t[k]` は `E1043`、`\(k: [i64]) -> t[k]` は受理。`IndexMut<Foo, i8, …>` と `IndexMut<Foo, i16, …>` の `f[1] = …` も `E1043`。
  - 変わらないこと: `ref xs[1]` が `ref i64`、`ref xs[1..3]`・`ref mut xs[0..]` が今の型。`t["bad"]` は `E1005`、`Matrix` の `m[0]` は `E1003`（既存のテスト）。A38 が足した順のテスト（「前提とする他チケットのインターフェース」の 5 つ）が、`index_access` への切り出しの後も変えずに通る。NC16 は順のテストを新しく足さない（D8）。
  - 修正案: `std::process::Command` で `tsuzuri check --json` を呼び（手本 `tests/diagnostics.rs`）、`E1042` の `fixes` の `applicability` が `certain` であること。`tsuzuri fix` の後のソースが `tsuzuri check` を通ること。
- **確保（`tests/fixtures/alloc_contracts/Main.tz`）**: 規則のコメント付きで 3 つ足す。「Tensor の要素を添字で読む: 確保なし」（`t[[1, 2]]`・`t[(1, 2)]`、1 軸の `t[1]`、3 軸・4 軸のタプル、`v[[1, 2]]`・`v[(1, 2)]`、ループで 12 回。`expect … 0 0 0 0`）、「Tensor.Mut の要素を添字で書く: 確保なし」（`m[[0, 1]] = 7`・`m[(0, 1)] = 7`）、「添字の窓: 結果の形と刻みだけ」（`ref t[[Slice.range 1 3, Slice.all()]]` が `expect_events … 2 0 0`）。`tests/alloc_contracts.rs` の件数の検査（今 `42`）を `45` にする。native・wasm32 × `-O0`・`-O3` × 2026・2027 は既存の仕組みで回る。
- **E2E（`tests/fixtures/tensor/Main.tz`・`tests/features.mjs` の `tensor`）**: タプルのキーと `[i64]` のキーの読み出しが `Tensor.at` と同じ値になる関数（負のストライドの窓、`permute` した窓、1〜4 軸、5 軸の `[i64]`）と、書き込みが `Tensor.write` と同じ位置に書く関数を `cases` に足す。`traps`（`trapKind: "assertion failed"`）に、2 次元のタプルを 3 軸に使う、負の成分、長さと等しい成分、1 軸の `t[0]` を 2 軸に使う、を足す。native/WASM `-O0`/`-O3`、`live == 0`。
- **既存のテストへの影響**: 着手時の `rg` が挙げるすべての窓の添字に `ref` を足す（期待する診断コードと値は変えない）。HEAD では現状 5 の `tests/index_class.rs` 4 か所と `tests/matrix_views.rs` 2 か所。A37 が足したテスト（`tests/index_class.rs` の再現・連鎖・総称関数の中の連鎖）と `tests/fixtures/tensor/Main.tz`・`tests/fixtures/matrix_view/Main.tz` の追加ケースも含み、窓の後に要素を読む連鎖は `(ref t[[A]])[[i, j]]` と書く（`ref t[[A]][[i, j]]` は `E1003`）。`tests/alloc_contracts.rs` の件数。ほかの既存のテストの期待値が変わったら §13.1 で止める。
- **性能**: 速度の合否の閾値は足さない。確保の回数は上の `Alloc` のテストで構造的に確かめる。速さを主張するなら `--release` で同じ処理（3×4 の `t[[i, j]]` と `m[(i, j)]` の二重ループ）を測り、条件と結果を完了報告に書く。

## ドキュメント

- `_tsuzuri/language-reference/types-and-type-inference/type-classes.md`: 128 行の `Index` の行に、土台を借りる添字は `ref` が要ること（`E1042`）、借りない添字に `ref` は付けないこと（`E1003`）、候補が 2 つ以上なら `E1043` を足す。表の後に短い節（`run=` の例 1 つ）を足してよい。
- `_tsuzuri/language-reference/built-in-types-and-modules/tensor.md`: 13 行の箇条を `ref t[[Slice…]]` に直し、タプルのキー `t[(i, j)]` を足す。106 行の段落に、添字の構文での要素の読み出し（`t[[i, j]]`・`t[(i, j)]`）は確保しないこと、`at`・`get` は今どおり 1 回確保することを書き、内側のループの勧めを添字の構文へ直す。280 行付近の計算量の表に、添字の要素の読み出し（$O(\text{rank})$、確保なし）と添字の窓（形と刻みの 2 つの配列）を足す。添字の節（`run=` の例）を足す。
- `_tsuzuri/language-reference/built-in-types-and-modules/slice.md`: 9 行と 15 行の例を `ref t[[…]]` にする（`run=` の出力は変わらない）。
- `_tsuzuri/language-reference/built-in-types-and-modules/matrix-view.md`: `ref m[(Slice…, Slice…)]` と `m[(i, j)]` を足す（今は添字の記述がない）。
- `_tsuzuri/language-reference/ownership-and-memory/borrowing.md`: 「呼び出し引数の暗黙の借用」（229 行）に、土台を借りる添字は引数でも `ref` を書くことを足す。「配列のスライスと可変スライス」（288 行）の後に、`Index` の窓も `ref` で書くことを 1 段落足す。
- `_tsuzuri/language-reference/compiler/diagnostics.md`: 「エラーコード」の範囲の記述（`E1001`〜`E1041`）を `E1043` までにし、`E1042`・`E1043` の行を足す。`E1003` の kind の追加を含め、kind の表は `node scripts/gen-docs.mjs` で作り直す（手で書かない）。
- `docs/language.md`: 「呼び出し引数の暗黙の借用」（1902 行付近）に、土台を借りる添字は `ref` が要ることを 1 段落足す。`Index` の節は今もないので新しくは作らない。
- README は変えない（`Index` の記述がない）。

## 受け入れ条件

- [ ] 土台を借りる添字は、`ref` が覆うときだけ受理され、型は Output そのもの（仕様 1 の表のすべての行のテストがある）。
- [ ] `E1042`（2 つの kind）・`E1043`・`E1003` の `index_ref_on_value` が仕様 2 の位置と修正案で出る。`E1042` の `index_view_requires_ref` の修正案は `certain` で、`tsuzuri fix` の後のソースが通る。
- [ ] `resolve_index` が `Inference` を複製せず、候補が 2 つ以上なら宣言の順によらず `E1043`。std・examples・既存の tests に `E1043` で拒否されるものがない。
- [ ] `Tensor`・`View` の要素の読み出し（`[i64]` と 1〜4 軸のタプル）と `Mut` の書き込みが `Alloc.since` で 0 回、添字の窓が 2 回で、値とトラップが `Tensor.at`・`Tensor.write` と同じ（native/WASM `-O0`/`-O3`）。Clang `-O3` 後の IR で読み出しのループに `@tz.alloc` がない。
- [ ] 組み込みの 6 型の添字とスライスの IR が変わらない（手順 1 の保存との `diff`）。
- [ ] A38 の評価順のテストが手順 4 の後も通る（`Index` と `IndexMut` の順を NC16 が変えていない）。
- [ ] 現状 5 の利用箇所と、A37 などが後から足した窓の添字がすべて `ref` 付きで、`rg -n '\[\[Slice\.|\[\(Slice\.' --glob '!_features/**' --glob '!target/**' .` の各行が `ref` を持つ。
- [ ] 文書（上の節）が更新され、`node scripts/check-docs.mjs`・`node scripts/gen-docs.mjs --check`・`node scripts/check-cli-docs.mjs` と T2 が通る。

## 落とし穴

- `ref` を吸収するのは添字クラスの式だけ。組み込みの `ref xs[i]` まで吸収すると要素への `ref T` が値になり、S2 に当たる。`ExprKind::Borrow` の腕では、対象を検査してから組み込みかどうかで道を分ける。
- 括弧は AST に残らない。`ref (t[[A]])[[3]]` は `ref t[[A]][[3]]` と同じで、外側を対象にするので `E1003`。窓だけを借りるには `(ref t[[A]])[[3]]` と書く。help でこの形を示す。
- 「借りる」の判定は宣言の Output で行う。具体化した型で判定すると、`Tensor<ref i64>` の要素の読み出し（Output `'a` が `ref i64`）に `ref` を求めてしまう。
- `Inference::unifies` は `quiet` を立てない。試行の前に立てないと、失敗した単一化ごとにメッセージを作る。`compatible` の「試行前の型」は試行の中では求められない（試行中は解が入っている）ので、先に解決しておく。候補の型変数の新しい推論変数は試行の前に作る（巻き戻されないが、解かれないまま残るだけ）。
- `E1042` は型検査の誤りなので、所有権の検査より先に出る。`E1013`・`E1014` を確かめる既存のテスト（`tests/index_class.rs` 169・178 行）は、`ref` を足さないと `E1042` に変わってしまう。
- `tests/alloc_contracts.rs` は fixture の `test "` の数（今 `42`）を数えている。足したら数も直す。
- タプルのキーを足しても `index_key_hint` は変わらない（`Tensor`・`View`・`Mut` は今も候補が 2 つ以上でヒントを出さない）。`Matrix` だけを読み込むプログラムの `m[0]` が `E1003` のままか、既存のテストで確かめる。
- `Index<View<'a>, [Slice.Axis], View<'a>>` の本文を書き直すときは、A37 の事実 `returns_contents` が立つ形に保つ（参照先のフィールド `self.data` を値として渡し、`ref self.data` や `freeze` を作らない）。崩すと std の窓の窓 `ref t[[A]][[B]]` が `E1013` になる（A37 の S2）。A37 が先に入っていれば、A37 の段 6 の呼び出しを新しい `slice_raw` の引数に直す。
- `slice_raw` の引数を `ref` にしたら、呼び出し 3 か所（`slice` と 2 つのインスタンス。`grep -n slice_raw std/Tensor.tz`）をすべて直す。行優先の印のときは刻みの配列を読まない（空の配列 `[]` を渡す。`[]` は確保しない）。
- `--emit llvm` は Clang の前の IR。確保の呼び出しが消えたかは `clang -x ir -O3` の後で見る。
- 評価順は A38 が決める（D8）。`index_access` への切り出しで、A38 が入れた並べ替えと共有の守りを落とさない。脱糖の引数の順（キー、土台。D-59 で対象が最後）は変えない。評価順は引数の位置ではなく A38 の下げ方で決まるので、引数を入れ替えて順を直そうとしない（A37 の下げ方に触れる）。

## 対象外

- 期待型（Output）でインスタンスの候補を絞ること（`let s: string = f[1]` で `i8` の候補だけを選ぶなど）。候補が 2 つ以上なら `E1043`。
- 書き込める窓 `ref mut t[[…]]`（[NC20](NC20-compound-assignment-and-mutable-index-views.md)）と、多次元の角括弧 `t[i, a..b]`（[NC18](NC18-multi-axis-bracket-indexing.md)）。
- マスクの添字とファンシーインデキシングの確保の削減（`view cond`・`view self` の確保。結果の `Tensor` はもともと確保する）。
- 窓の形と刻みの配列をなくす表現（小さい軸の数を値として持つ `View` など）。窓の作成は $O(\text{rank})$ の確保 2 回のまま。
- 公開関数 `Tensor.at`・`Tensor.get`・`Tensor.write` の確保（シグネチャが `own [i64]` なので変えない）。
- `Index`・`IndexMut` と 1 引数の算術クラスの orphan 規則（A37 の D4。承認済み（D-66 の委任で決定、2026-11-25）。A37・NC19・NC20 で 1 つの規則にし、A37 の手順 5 が入れる）。
- 添字の評価順の統一（[A38](A38-index-evaluation-order.md)。D8）。

## 決定事項

- **D1（承認済み。D-66d）**: 結果が土台を借りる `Index` の使用は、どの綴りでも `ref` を要る。ないと型検査の `E1042`（kind `index_view_requires_ref`。型に依存するので構文の `E0002` にしない）。`ref` は `ref xs[a..b]` と同じく式に吸収され、型は Output（`Tensor.View<'a>`）。NC04 はリリースされていないので edition を使わず、全 edition で同じ。
- **D2（既定案。2026-11-25 の統合で A37 の D1 に揃えた）**: 「土台を借りる」は、インスタンスの宣言の Output に対する `Type::carries_loans`（`src/check.rs:907`。型変数は数えない）で決め、A37 の `ResolvedIndex::borrows_base` を読む。理由: A37 の土台の下げ方と同じ判定なので、`ref` の要否と土台の借用が食い違わない（判定は 1 つ）。生成関数が region `r` を持つか（`output_needs_region`、`src/polymorph.rs:3999`）で決めると、参照を含むタプルや関数を返す `Index`（A37 の再現 3-g・3-h）が土台を借りるのに `ref` なしで書けてしまう。関数を返す `Index` も `ref` が要る（`ref f[1]`。std にはない）。インスタンスの宣言だけで決まり利用者が予測できる。具体化した型の借用（`Tensor<ref i64>` の要素）は土台の借用ではない（違えば S1）。
- **D3（既定案）**: `ref` は添字の式の連鎖を覆う（`ref t[[A]][[B]]` は 2 つの窓の両方）。理由: パーサーが配列の `ref xs[1..][0..2]` を同じように扱っている（現状 1）。NC18 の `ref t[1..][.., 0]` もこの規則で書ける。
- **D4（既定案）**: 土台を借りない添字に `ref`・`ref mut` を付けると `E1003`（kind `index_ref_on_value`。D-66 の設計の「付けると型の誤り」）。土台を借りる添字に `ref mut` を付けると `E1042`（kind `index_view_not_writable`）。理由: `ref` があれば土台を借りる、ないなら値、を書き方から読めるようにする。NC20 は、`IndexBorrowMut` のインスタンスを持つ型でだけ `index_view_not_writable` を受理に変える（ほかの型の誤りのコードは変わらない）。この 2 つの kind は設計の識別子の表になかったので、2026-11-25 の統合で GUIDE D-55 に登録した。NC18 の D3 は、位置だけのキーに付けた `ref`（`ref t[i, j]`）を構文の `E0002` ではなくこの票と同じコードと kind の誤りにする（設計の案の改訂。D-66 に記録）ので、`index_ref_on_value` を共有する。
- **D5（既定案）**: `resolve_index` は候補が 2 つ以上残れば `E1043`（kind `ambiguous_instance_candidates`）。期待型では絞らない。候補の試行は `Inference::unifies` による巻き戻しで、`index_key_hint`・`has_index_instances` も同じにする。理由: GUIDE §1 規則 2。`E1041`（匿名共用体の注入の曖昧さ）と同じく既定を選ばない。
- **D6（既定案）**: `Tensor<'a>`・`View<'a>` の `Index` と `Mut<'a>` の `IndexMut` に、軸の数 1〜4 のキー `i64`・`(i64 * i64)`・`(i64 * i64 * i64)`・`(i64 * i64 * i64 * i64)` を足す。5 軸以上と 0 軸は `[i64]`。軸の数が合わなければトラップ。理由: Matrix の `(i64 * i64)` と同じ道で、軸の数を固定したループのない読み出しになり、NC18 の `t[i, j]` の脱糖先になる。書き込みは同じ補助関数を使うので同時に直す。
- **D7（既定案）**: 添字のインスタンスは借りたキーをそのまま使い、`Array.copy` も `view` も作らない。`[Slice.Axis]` の窓は `slice_raw` にキーを借りて渡し、`Tensor` の土台では刻みの配列を作らない（窓の作成は結果の形と刻みの 2 回）。公開関数のシグネチャは変えない。
- **D8（決定（D-66 の委任で決定、2026-11-25）。起票時は既定案「NC16 では変えず、統一は人間が決める」）**: 添字の評価順は [A38](A38-index-evaluation-order.md) が組み込みの添字にそろえる。規則は、組み込みの 6 型の添字と `Index`・`IndexMut`・`IndexBorrowMut`・NC18 の `b[c1, …, cn]` のすべてで 1 つ。先に土台（場所の部分か一時値）を評価し、次にキーの成分を左から右へ評価する（範囲の成分の中は始点・刻み・終点の順、`^k` はその被演算子）。長さ・境界・`Slice.Sel` の解決、インスタンスの呼び出し、トラップはその後。キーを評価する間は土台に共有の守りがあり、土台へ代入・move・排他借用するキーは `E1014`（kind `borrow_conflict`）。代入 `b[k] = v` は右辺が先（EVL-012）。変わるのは `Index`・`IndexMut` の経路だけで、edition は使わない。NC16 は A38 に依存し（手順 4 以降）、A38 の順を保つ。順を固定するテストは A38 が足し、NC16 は足さない。
  - 理由: 同じ `x[k]` の形で、型によって副作用の順が変わると、利用者も AI も順を予測できない（D-66 の「文脈で意味が変わる記号を増やさない」と同じ考え）。組み込みの順はリリース済みで文書にもあるので、変えるのはリリース前の `Index` の側にする。評価順は `ref` と確保 0 回とは別の意味の変更なので、NC16 に混ぜず A38 に分けた。A38 も NC03・NC04 を含む最初のリリースの条件なので、NC16 と同じ窓に入る。
  - 選ばなかった案: NC16 の中で直す、A37 で直す、今の順（キーが先）を残す。
