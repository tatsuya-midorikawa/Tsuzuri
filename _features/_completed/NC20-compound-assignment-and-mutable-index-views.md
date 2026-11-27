# NC20: 複合代入 `+=`・`-=`・`*=`・`/=`・`%=` と書き込み窓 `ref mut t[…]`（組み込みの `AddAssign` 系と `IndexBorrowMut`）

| 項目 | 内容 |
| --- | --- |
| ID | NC20 |
| 優先度 | P2（NumPy の `a += b`・`a[1:3, :] += b` を、要素の確保なしで書けるようにする。組み込み型の `x += 1` は単独でも価値がある） |
| 規模 | L（Phase 1: 組み込み型の複合代入、Phase 2: `AddAssign` 系と `Tensor` の in-place カーネル、Phase 3: `IndexBorrowMut`・書き込み窓・添字の対象への複合代入） |
| 依存 | [A37](_completed/A37-index-base-place-borrow.md), [A38](_completed/A38-index-evaluation-order.md), [A39](_completed/A39-field-exclusive-borrow.md), [NC18](_completed/NC18-multi-axis-bracket-indexing.md), [NC19](_completed/NC19-operator-extension-instances.md)（Phase ごとの依存は「着手条件」。Phase 1 はどれにも依存しない: D8。Phase 2 は A39・NC19 とその依存の NC06、Phase 3 は A37・A38・A39・NC16・NC18）, ([A40](_completed/A40-reference-operands.md))（順序はどちらでもよい。後に入る方が合わせる: A40 の D4） |
| 後続 | – |
| 状態 | done |
| 起票 | 2026-11-25（[GUIDE D-66](GUIDE.md#d-66-自然な添字範囲演算子d-65-の改訂a37nc16nc20)。HEAD `858a949c` で確認） |
| 承認 | Phase 1 は不要（字句 `+=` などの追加は D-66 で承認済み）。D1〜D4 は承認済み（D-66 の委任で決定、2026-11-25）: D1（`AddAssign` 系の組み込みクラスとメソッドの形）、D2（フィールドの排他借用。std だけの特権を採らず、一般の規則 [A39](_completed/A39-field-exclusive-borrow.md) に置き換えた）、D3（組み込みクラス `IndexBorrowMut`）、D4（`AddAssign` 系の orphan 規則。A37 の D4 と同じ 1 つの規則）。D5 は承認済み（D-66 の委任で決定、2026-11-25）。D13 は決定（D-66 の委任で決定、2026-11-25。(a) は [PM10](../_perfs/_completed/PM10-std-consume-copies.md)、(b) は A38）。D14 は決定（D-66 の委任で決定、2026-11-25。連鎖した書き込み窓は受理せず、`E1013` に help を付ける）。ほかは既定案 |
| 手本にする既存実装 | `src/check.rs` 14895 行付近（`ExprKind::Assign` の型検査と、NC04 Phase 2 の `IndexMut` への脱糖）と 15003〜15017 行（2027 のフィールドと要素への射影の書き込み）、`src/ownership.rs` 7895 行付近の `exclusive_slice`（境界を評価する間だけ共有の守りを置き、その後に排他の loan を取る）、`src/polymorph.rs` 2494 行付近の `register_index_instance`（組み込み限定の多引数ヘッダ）、`src/polymorph.rs` 5262 行付近（型検査の後で決まった型を確かめる `ref_operand_inferred_late`）、`std/Tensor.tz` 803 行の `map_in_place` と 868 行の `narrow_mut` |
| 主な影響ファイル | `src/lexer.rs`、`src/parser.rs`、`src/syntax.rs`、`src/syntax_codec.rs`、`src/frontend_cache.rs`（`FRONTEND_FORMAT`）、`src/formatter.rs`、`src/computation.rs`、`src/warnings.rs`、`src/check.rs`、`src/polymorph.rs`（`BUILTIN_CLASSES`、`dispatch_positions`）、`src/ownership.rs`（複合代入の右辺の守りと `IndexBorrowMut` の守り。フィールドの排他借用は A39 が変える）、`src/stdlib.rs`（A37 の一覧のテスト `std_instances_with_open_dispatch_positions_are_listed` に、Rhs が `'a` の 10 個を足す）、`src/diagnostic_kinds.rs`、`src/hints.rs`、`src/explain/expected_expression.md`、`guide/card.md`、`guide/AGENTS.md.tmpl`、`std/Tensor.tz`、`spec/rules/EVL.md`、`spec/rules/OWN.md`、`tests/compound_assignment.rs`（新規）、`tests/index_class.rs`、`tests/tensor.rs`、`tests/hints.rs`、`tests/diagnostic_kinds.rs`、`tests/diagnostic_baseline/compound-assignment/`、`tests/formatter.rs`、`tests/std_argument_order.rs`、`tests/std_examples.rs`、`tests/fixtures/effects_sweep/`、`tests/fixtures/compound_assignment/`（新規）、`tests/fixtures/compound_assignment_2028/`（新規。Phase 3）、`tests/features.mjs`、`tests/tensor-cases.mjs`、`tests/conformance/EVL/`、`tests/conformance/OWN/`、`_tsuzuri/language-reference/`（「ドキュメント」の表）、`docs/language.md`、`docs/architecture.md` |

## 実装者向け早見

- 範囲: (1) 字句 `+=`・`-=`・`*=`・`/=`・`%=` を足し、`=` と同じ位置（`expression_inner` の最小優先度 0）で解析する。型は `unit`。(2) 組み込みの算術を持つ型（整数・浮動小数点・SIMD の数値ベクトル・文字列。型で決める）では、`p op= e` を `p = p op e` と同じ型付き IR に下げる（演算子がその型に組み込みでない `f64` の `%=`・`string` の `-=` は `p = p op e` と同じ誤り）。評価順は「場所の部分を 1 回評価 → 古い値を読む → `e` → 書く」で、右辺が対象を書き換える・move する・排他借用するプログラムはどの型でも `E1014` にする。(3) 組み込みでない型には組み込みクラス `AddAssign<Target, Rhs>` 系（メソッド `add_assign :: ref Rhs -> ref mut Target -> unit`）を足し、`Tensor`・`Tensor.Mut` へ要素の確保なしの in-place 更新を入れる。対象は `let mut` のローカルと `ref mut` の参照先（全 edition）と、それらを根にしたフィールドの経路（edition 2027 から。A39 の排他借用で通る）。(4) 組み込みクラス `IndexBorrowMut<Self, Key, Output>`（std のインスタンスだけ）で `ref mut t[1..3, ..]` を `Tensor.Mut` にし、`t[1..3, ..] += u`・`t[i, j] += 1.0` を書けるようにする。
- 最初に読む: ① 仕様 §2（評価順と右辺の守り）と §8（診断の表） ② `src/check.rs` 14895〜15022 行付近（`=` と `IndexMut`） ③ `src/ownership.rs` 7400〜7440 行付近（排他のアクセスの拒否）と 7895 行付近（`exclusive_slice`） ④ 決定事項 D1・D2（メソッドの形と、フィールドの排他借用を std の特権ではなく A39 の一般の規則に任せる理由）。
- 最初に書くテスト: `tests/compound_assignment.rs` に、`x += 2` が `x = x + 2` と同じ値・同じ IR（位置の情報を除く）になること、`x += bump (ref mut x)` が `E1014` になること、`deref (pick (ref mut x) 1) += say 2` の出力順が `place 1`、`2` になること。
- 触ってはいけない: 複合代入を書かないプログラムの生成 IR（1 byte も変えない）、`=` の評価順（EVL-012: 右辺が先）、`IndexMut` の `t[k] = v` の脱糖（順は A38 の規則: 右辺、土台、キー）、edition 2026 のフィールドの不変性（`b.f = v` は `E1012`）、整数の折り返しと `@checked`、浮動小数点の丸め・NaN・符号付きゼロ（FMA・再結合・融合なし）、`Tensor.Mut` の単射性。組み込みでない型の `x += y` を、確保する `x = x + y` へ黙って落とさない。
- 完了の見分け方: `x += 1`（`i64`）、`s += "c"`、`x += row`（`Tensor<f64>` へ行ベクトルのブロードキャスト）、`t[1, ..] *= 2.0`（edition 2028）、`t[1, 2] += 1.0` が native/WASM × `-O0`/`-O3` で一致し、同じ形の `Tensor` どうしの `x += y` とスカラーの `x += 1.0` が `Alloc.since` で確保 0 回（`+=` の文の前後で測る）、`Tensor<f64>` のフィールドを持つ `let mut` のレコードの `model.w += g` が edition 2027 で文の確保 0 回・2026 で `E1012`、`a[1..] += ref a[..^1]` が `E1014`。

## 目的

Tsuzuri には複合代入がなく、`x += 1` は `E0002` になる（ヒントが `x = x + 1` と書くよう案内する）。数値のループでは `total = total + v` と同じ名前を 2 回書くことになり、NumPy（Python）・C#・Rust から来た利用者がまずつまずく。

テンソルではもっと深刻である。`x = x + y` は（NC06 の演算子インスタンスが入っても）新しいテンソルを確保し、`Tensor.update` で書き込み窓を開いて関数で書く方法も、要素が Copy の型では中身を 2 回複製する（「現状」の再現 R4）。所有するテンソルの一部（`a[1:3, :]`）へ書く方法は、`Tensor.borrow_mut` で作った窓を `narrow_mut` で狭める以外になく、所有する `Tensor` から直接は取れない（`ref mut t[[Slice.all()]]` は `E1013`、`t[[0]] = 5` は `E1005`）。

このチケットは、(1) 組み込み型の複合代入を `p = p op e` と同じ意味・同じ IR で入れ、(2) 組み込みでない型にはインスタンスがあるときだけ in-place の更新を許し（ないときは `E1005`。確保する書き方へ黙って落とさない）、(3) 所有するテンソルとその窓の一部を、借用の規則に守られた書き込み窓として取れるようにする。重なる更新（`x += x`、`a[1..] += ref a[..^1]`）は暗黙のコピーをせず `E1014` にして、明示のスナップショットを案内する。

## 着手条件と停止条件

### 着手条件

- **Phase 1**（組み込み型の複合代入。仕様 §1〜§3）: `gh run list --branch main --workflow ci.yml --limit 1` が成功していること。GUIDE の D-66 と D-15 の追記（字句 `+=`・`-=`・`*=`・`/=`・`%=`）が入っていること。A37・A38・A39・NC18・NC19 を待たない（D8）。
- **Phase 2**（`AddAssign` 系と `Tensor` の in-place。仕様 §4・§5）: Phase 1 が done。[A39](_completed/A39-field-exclusive-borrow.md) が done（std の本体の `ref mut target.data[0..]` と、利用者のフィールドの対象 `model.w += g` が使う）。[NC19](_completed/NC19-operator-extension-instances.md)（と、その依存の [NC06](_completed/NC06-tensor-ufuncs-and-elementwise.md)）が done。A37 の手順 5（orphan 規則と `dispatch_positions`）は NC19 の前提なので、このときには入っている。
- **Phase 3**（`IndexBorrowMut`・書き込み窓・添字の対象。仕様 §6・§7）: Phase 2 が done。[A37](_completed/A37-index-base-place-borrow.md)・[A38](_completed/A38-index-evaluation-order.md)・[A39](_completed/A39-field-exclusive-borrow.md)・[NC16](_completed/NC16-index-ref-and-zero-alloc-element-reads.md)・[NC18](_completed/NC18-multi-axis-bracket-indexing.md) が done。
- 着手時に `rg -lw 'Assign' src/` で代入を扱う全箇所を控え（`use ExprKind::*` の後の素の `Assign(` も拾う。HEAD で 25 ファイル。`src/kg/find.rs` の `Goal::Assign` は無関係）、新しい AST の変種を足したときに `_ =>` で黙って捨てる `match` がないか確かめる（GUIDE §11.1）。

### 停止条件

- **S1** 組み込み型の単純な場所（仕様 §3）で、`p op= e` の IR を位置の情報を除いて `p = p op e` と同じにできない。
- **S2** 複合代入を書かないプログラム（std・`tests/fixtures/`・`examples/`）の診断か生成 IR が変わる。例外は「既存テストへの影響」に挙げたヒントと baseline の 2 つだけ。
- **S3** A39 の規則だけでは std の本体（`ref mut target.data[0..]`、`IndexBorrowMut` の窓）が書けない。std だけの特権や許可表で回避しない（D2。A39 で健全さを示せない構成は、A39 の側で持ち主の全体に loan を置く粗い規則へ縮める）。
- **S4** 同じ形の `Tensor` どうしの `x += y` か、スカラーの `x += 1.0` で、`+=` の文の要素数に比例する確保を 0 回にできない（`Alloc.since` のテストを緩めない）。`src/last_use.rs` の除外（A17 の X1）を変えたくなったら止める（PM10 の範囲。D13）。
- **S5** `AddAssign` の解決に、NC19 の演算子の解決（リテラルを要素型に合わせる規則・1 つの表・`E1043`）を超える型推論の探索が要る。
- **S6** `IndexBorrowMut` の結果が、土台の場所の排他 loan 以外の loan（キーの loan）を持つ、または A37・A38 の後でも右辺の窓と書き込み窓の重なりを `E1014` にできない。
- **S7** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `858a949c` で確認）

再現の各プログラムは `/tmp/tz-nc20-<名前>/Main.tz`（ファイル名は大文字で始める）に置き、`node scripts/slot.mjs -- target/debug/tsuzuri run -O0 /tmp/tz-nc20-<名前>/Main.tz` で動かした。`-O3` でも同じ結果だった。

### 字句と構文

- 字句の最長一致の表は `src/lexer.rs` 1048 行の `symbol` の中（1055〜1079 行付近）にあり、`->`・`**`・`==`・`!=`・`<=`・`>=` などを 1 文字の記号より先に取る。`+=`・`-=`・`*=`・`/=`・`%=` はない。
- 代入は `src/parser.rs` 4263〜4272 行付近で、`expression_inner` が `minimum == 0` のときだけ `=` を取って `ExprKind::Assign(左辺, 右辺)` を作る（右結合）。文ではなく式で、型は `unit`（TYP-013）。`if c then x = x + 1` は通る。
- R1（複合代入はない）:

  ```tsuzuri
  let mut x = 1
  x += 2
  x
  ```

  `error[E0002]: expected an expression`（位置は `+=` の `=`）、help `there is no compound assignment; write 'x = x + 1'`。`x %= 2` も `E0002` だが help はない。help は `src/hints.rs` 521〜541 行付近のヒント `compound-assignment`（パターンは `+`・`-`・`*`・`/` の後の `=` だけ）が付ける。`tests/hints.rs` 22 行は表の件数 40 を固定し、`tests/diagnostic_baseline/compound-assignment/Main.tz`（`x += 1`）と `tests/diagnostic_kinds.rs` 255 行付近の `baseline_diagnostics_are_unchanged`（48 件）がこの `E0002` を固定している。`src/explain/expected_expression.md` も「there is no compound assignment」と書き、AI 向けのガイドも `guide/card.md` 82 行（「`x += 1` is E0002」）と `guide/AGENTS.md.tmpl` 33 行（「There is no … `x += 1`」）で同じことを書く。
- 範囲を含む角括弧に `ref`／`ref mut` がないと、`src/parser.rs` 4560〜4566 行付近の `index_or_slice` が `]` を読んだ時点で `SliceMustBeBorrowed`（`E0002`、kind `slice_must_be_borrowed`）を出す。後ろに何が続くかは見ない。`ref`／`ref mut` の吸収は `ExprKind::Slice` だけ（`prefix` 5197 行付近、`keyword_prefix` 5259 行付近）。
- `Index`・`IndexMut` の 3 引数のヘッダは組み込み限定で、`src/parser.rs` 915 行付近で読む。制約には書けず（`def f :: Index<'c, i64, i64> => …` は `E0002 expected exactly one type argument`）、メソッドの値にもならない（`let g = Index.index` は `E1002 type class 'Index' has no method 'index'`）。

### 代入の評価順

- `=` は右辺を先に評価し、その後で代入先を決める（EVL-012、`spec/rules/EVL.md` 18 行。`docs/language.md` 1838 行・1961 行、`op-and-expressions.md`「評価順序」の表）。
- R2（右辺が対象を書き換える）:

  ```tsuzuri
  def bump :: ref mut i64 -> i64 = \r ->
      deref r = 10
      0
  let mut x = 1
  x = x + bump (ref mut x)
  x
  ```

  `1` を出す（右辺の `x` を先に読み、`bump` が書いた 10 を代入で上書きする）。今は受理される。
- R3（添字の代入の順）: 書き込み先の添字と右辺の両方で `tick`（ログ配列の先頭に呼ばれた回数を、続く位置に呼ばれた順の引数を書く関数）を呼ぶと、組み込みの配列の `xs[tick 1 …] = tick 2 …` はログ `[2, 2, 1, 0]`（右辺が先、添字が後）、`Tensor.Mut` の `m[[tick 1 …]] = tick 2 …` はログ `[2, 1, 2, 0]`（キーが先、右辺が後）になる。`IndexMut` の脱糖が引数を「キー、右辺、対象」の順に並べるため（`src/check.rs` 14969〜14983 行付近）。組み込みの添字の代入（EVL-012 どおり）と食い違っている。A38 が `IndexMut` の `=` を EVL-012 にそろえる（右辺、土台、キーの成分を左から右。D13 (b)）。このチケットでは `=` を変えない。
- 2027 は可変な所有者・排他参照からのフィールドと、配列・リスト・`Vec`・固定長配列の要素への代入を許す（`src/check.rs` 15003〜15017 行付近の `read_conventions && projection`）。2026 では `E1012`（kind `assign_to_immutable_place`）。
- R5（右辺が土台を置き換える）: `let mut xs = [1, 2, 3]` の後の `xs[2] = { xs = [9]; 5 }` は、範囲の検査を右辺の後で行うので `trap: index out of bounds` になる（範囲外へ書かない）。

### `Index`・`IndexMut` と書き込み

- 読み出しの `Index` は、キーと土台を `TypedExprKind::BorrowOperand` で貸す（`src/check.rs` 14823〜14833 行付近）。所有権の検査は `BorrowOperand` を `Use::Read` で評価する（`src/ownership.rs` 14688 行付近）。結果が借用を持つときに土台の場所を借りない不具合は A37 が直す。
- 書き込みの `IndexMut` は、対象が `ref mut` ならそのまま、そうでなければ `require_mutable_reference` の後で `TypedExprKind::Borrow(対象, true)` として渡す（`src/check.rs` 14919〜14931 行付近）。メソッドの型は `index_mut :: ref Key -> Value -> ref mut Self -> unit`（`src/polymorph.rs` 2649 行付近）。インスタンスがなければ `E1005 no instance for IndexMut<…>`（14953 行付近）。
- `BUILTIN_CLASSES` は 36 個（`src/polymorph.rs` 947 行）。組み込みの演算かどうかは `Classes::intrinsic`（3648 行付近）と `builtin_instance_by_type`（7361 行付近。`Add` は数値と文字列、`Rem` は整数だけ）が決める。
- 型検査の後で決まった型を確かめて誤りにする前例: `polymorph.rs` 5262 行付近の `finish` が `undecided_borrows` を見て `E1015`（kind `ref_operand_inferred_late`）を出す。
- 同じ呼び出しで場所の `BorrowOperand` と `Borrow(同じ場所, true)` が並ぶと `E1014`（kind `borrow_conflict`）になる（`instance IndexMut<Foo, Foo, i64>` の `f[f] = 1`）。`Array.blit 0 1 3 xs xs` も `E1014`。

### 所有権の制限

- 排他のアクセス（`Use::MutBorrow`）は、レコードのフィールドを経由する場所では拒否される（`src/ownership.rs` 7429 行付近の `!unmarked_fields(place).is_empty()`、`E1014` kind `mutate_immutable_value`）。region を持つフィールド（`Tensor.Mut` の `data: ref mut {r} ['a..]` など）を経由する場所だけは許す（`unmarked_fields` は 103 行付近）。

  ```tsuzuri
  record Box2 { data: [i64] }
  let mut b = Box2 { data: [1, 2, 3] }
  let s = ref mut b.data[0..]
  ```

  は `E1014`。`instance IndexMut<Foo, …>` の本体で `self: ref mut Foo` のフィールドへ `Array.write … self.data` しても `E1014`。したがって std も、所有する `Tensor` の `data` を排他借用する本体を書けない。A39 が、edition 2027 以降のソースでレコードのフィールドの経路の排他借用を許す（D2）。
- フィールドへの代入と排他借用は edition で違う（HEAD の dev バイナリで確認。edition は `Tsuzuri.toml` の `edition` で切り替えた。次の項目も同じ）。`record B { n: i64, xs: [i64] }` と `let mut b = B { n: 1, xs: [1, 2, 3] }` の後で、`b.n = 5` は 2026 で `E1012`（kind `assign_to_immutable_place`）、2027 で受理される。`let r = ref mut b.xs` は両方の edition で `E1014`（kind `mutate_immutable_value`、help `write through a 'let mut' variable that holds the whole value, or take it by 'ref mut'`）。
- 2027 では、`ref mut` を通した射影の書き込みが `IndexMut` の本体で動く（HEAD の dev バイナリで確認）。`record Grid { data: [i64] }` の `instance IndexMut<Grid, i64, i64>` の本体 `self.data[deref k] = v` は、2027 で `g[1] = 42` の後の `g.data[1]` が `42` になる。2026 では同じ本体が `E1012`（kind `assign_to_immutable_place`）。std は `Edition::LATEST`（今は 2027。`src/syntax.rs` 35 行）で読む（`src/stdlib.rs` 978 行）ので、std のインスタンスの本体はこの書き込みを使える。
- `Drop.drop` の中で、根を `ref mut` として渡すと `E1012`（kind `drop_pass_exclusive_value`。7400〜7410 行付近）。共有参照を経由する書き込み・排他の再借用は `E1014`（kind `mutate_through_shared_reference`。`shared_exclusive`、7866 行付近）。`IndexMut` の対象を共有参照や `let` の束縛にすると `E1014`（`mutate_immutable_value`、help `declare 'm' with 'let mut', or take it by 'ref mut'`）。
- `exclusive_slice`（7895 行付近）は、土台の場所に共有の守り（`self.held`）を置いて境界を評価し、その後で排他の loan を取る。境界は土台を読めるが変えられない。
- 一時値を `ref mut` で借りると `E1013`（kind `borrow_of_temporary`、`borrow requires a local place; bind the temporary with 'let' first`）。

### `Tensor` の書き込み経路と確保

- `Tensor<'a>` は `record Tensor<'a> { shape: [i64], data: ['a] }`（`std/Tensor.tz` 5 行）で、不透明（`src/stdlib.rs` 178 行付近の `opaque_record` に `Tensor.Tensor`・`Tensor.View`・`Tensor.Mut`）。モジュールの外からフィールドを読むと `E1022`。
- 書き込める窓は `Tensor.Mut`（16 行。単射で、長さ 2 以上の軸のストライドは 0 にならない）。作る方法は `borrow_mut`（630 行。`let mut` の配列から）と `update`（644 行。所有するテンソルを move して窓を開く）と、`Mut` を狭める `narrow_mut`（868 行。`ref mut {r} Mut<'a> {r} -> Mut<'a> {r}`）。`Mut` への書き込みは `write`（716 行）・`fill`（738 行）・`copy_from`（770 行）・`map_in_place`（803 行）と、`IndexMut<Mut<'a>, [i64], 'a>`（1680 行付近）・`IndexMut<Mut<'a>, Tensor<bool>, 'a>`（2896 行付近）。`Index` のインスタンスは `Tensor` と `View` だけで、`Mut` にはない。
- R6（所有するテンソルへの書き込み）: `let mut t = Tensor.zeros [4]` の後の `t[[0]] = 5` は `E1005 no instance for IndexMut<Tensor<…>, […]>`、`ref mut t[[Slice.all()]]` は `E1013`、`ref mut t[1..3]` は `E1005 a slice requires an array; lists and strings do not support array slicing`。`ref t[[Slice.all()]]` の型は `ref Tensor.View<i64>`（NC16 が `Tensor.View<i64>` にする）。
- R4（`Tensor.update` は要素が Copy の型で中身を 2 回複製する）:

  ```tsuzuri
  def noop :: ref mut Tensor.Mut<i64> -> unit
  fn noop _m = ()

  let t1 = Tensor.init [1000] (\i -> i)
  let s1 = Alloc.snapshot ()
  let t2 = Tensor.update noop t1
  let u1 = Alloc.since s1
  $"{u1.allocs}/{u1.bytes} {Tensor.to_array t2 |> Array.length}"
  ```

  `--allocator counting` で `-O0`・`-O3` とも `7/16040 1000`（8,000 バイトの中身を 2 回）。`update` の本体（645〜650 行）の `let mut data = tensor.data` はフィールドの射影なので複製し（`--warn implicit-copy` の `W1006` と同じ位置）、`Tensor { shape: shape, data: data }` の `data` は借用（`ref mut data`）の根なので最後の使用でも move しない（`src/last_use.rs` 55 行付近。A17 の規則 X1「借用の根になる局所は関数全体で移動しない」、`_features/_completed/A17-last-use-move.md` 189・194・645 行）。docstring（636〜637 行）の「without copying」と食い違う。同じ理由で、`let mut out = new [i64](1000, …)` を `ref mut out[0..]` で書いてから `out` を返す関数は 2 回・16,000 バイト確保する（借用しなければ 1 回・8,000 バイト）。この複製と、同じ形の 8 つの関数（`reshape_infer`・`try_reshape` を含む）の複製は [PM10](../_perfs/_completed/PM10-std-consume-copies.md) が直す（D13 (a)）。docstring と `tensor.md`・`matrix.md` の「複製しない」は、このチケット群の PR で今の振る舞いと既知の制限の注記に直す。
- `Tensor.view`（207〜208 行）は shape の複製と `dense_strides` で 2 回確保する。`broadcast_to`（1006 行）は右辺を対象の形へ一方向にブロードキャストする（`Result<View<'a>, Error>`）。`is_contiguous_mut`（680 行）、`clone`（586 行）がある。`Tensor.add_into` など NC06 の関数と、NumPy の `np.add.at` に当たる `Tensor.add_at` は HEAD にない。
- 計画（コードではない）: NC06 の `*_into` は `out = a op b` の形で、出力先と入力が同じテンソルだと借用が重なって `E1014` になるので `x += y` には使えない。そのため NC06 は累積のインプレース版 `*_in_place` とその非公開の双子 `*_in_place_or_trap` を計画に持つ（NC06 の D8・仕様 5）。NC20 はこれを使う。
- VS Code の文法は演算子を `[+*/%=!<>&|^~?\\-]+` でまとめて色付けする（`vsc/syntaxes/tsuzuri.tmLanguage.json` 28 行）ので、`+=` は追加の作業なしに色が付く。

## 変えてはいけない意味

- 複合代入を書かないプログラムの構文・型・評価順・所有権・診断・生成 IR。`=` の評価順（EVL-012）と、`IndexMut` による `t[k] = v` の脱糖。`t[k] = v` の順は A38 の規則（右辺、土台、キーの成分を左から右）で、NC20 は変えない。
- edition 2026 のフィールドの不変性。2026 では `b.f = v` が `E1012`、`ref mut b.f` が `E1014` のままで、NC20 もフィールドの経路の対象を 2026 で受理しない（A39 は 2027 から）。
- 組み込み型の算術: 整数の折り返し、`@checked` の検査（注釈が直接評価する演算子だけ。呼び出した関数の中は検査しない）、ゼロ除算と `MIN / -1` のトラップ、浮動小数点の丸め・NaN・符号付きゼロ。複合代入でも同じ演算子・同じ命令を使う。
- テンソルの要素の演算は要素型の演算子そのもの（`t_i op r_i`）。暗黙の FMA・再結合・融合・fast-math なし。
- 借用の規則: 書き込み窓は単射（`Tensor.Mut` の不変条件）。共有の窓が生きている間の排他の書き込みは `E1014`。マスクとファンシーインデキシングは書き込み窓にならない。重なる更新に暗黙のスナップショットを作らない。
- `Index`・`IndexMut` を制約やメソッドの値に使えないこと（NC04 D1）。一般の利用者向けの多引数クラスは開かない。
- 配列の共有スライス・排他スライス（`ref xs[a..b]`・`ref mut xs[a..b]`）の構文・意味・IR。

## 仕様

### 1. 字句と構文

- 新しい字句: `+=`・`-=`・`*=`・`/=`・`%=`（全 edition）。最長一致の表では `->`・`**` より後に置く。`**=` は今と同じく `**` と `=` に分かれ、`->` は矢印のまま、`/*` はコメントのまま。新しい予約語はない。
- 文法（EBNF 風）:

  ```text
  assignment := expression assign_op expression          // 右結合。左辺は場所
  assign_op  := "=" | "+=" | "-=" | "*=" | "/=" | "%="
  ```

  `=` と同じく `expression_inner` の `minimum == 0` の位置で取る。式であり、型は `unit`。`if c then x += 1` も書ける。行頭の `+=` は前の行の続きにならない（`=` と同じ）。
- 新しい AST の変種 `ExprKind::CompoundAssign(BinaryOp, Box<Expr>, Box<Expr>)`（演算子は `Add`・`Subtract`・`Multiply`・`Divide`・`Remainder`）。`src/syntax.rs` の走査（1015・1164 行付近）、`src/syntax_codec.rs`（1871 行付近の `Assign` の隣）と `FRONTEND_FORMAT` を 1 上げる、`src/formatter.rs`（`target += value` と空白を入れて印字。`x = x + 1` を `x += 1` へ書き換えない）、`src/computation.rs` 245 行付近（計算式の継続の中の代入と同じ扱い）、`src/warnings.rs` の `ExprKind` の走査（588〜674 行付近の `Assign(left, right)` の腕）、`src/check.rs` 11164 行付近の `tail_spans` を更新する。
- 足さない形: `**=`・`&&&=`・`|||=`・`^^^=`・`<<<=`・`>>>=`・`&&=`・`||=`（今と同じ `E0002`。ヒントが案内する: D10）。
- 範囲を含む角括弧の対象（Phase 3）: `index_or_slice` は、`]` の次の字句が `+=` などの複合代入のときだけ `SliceMustBeBorrowed` を保留する。その角括弧の式がそのまま複合代入の左辺になったときに取り消し、借用のない節点を作る（2 成分以上は NC18 の `ExprKind::MultiIndex { value, keys, borrow }` を `borrow: None` で、1 成分は `ExprKind::Slice` に「借用なし」を表せるようにして。形は NC18 の節点に合わせて決める）。左辺が別の式になったとき（`f t[1..3] += u`、`a + t[1..3] += u`）は、保留した `E0002` を今と同じ位置・同じ文言で出す。誤りを出す場所は NC18 が 1 つの関数にまとめる（NC18 の仕様 1）ので、保留と取り消しはその関数の呼び出し側で行う。`]` の次が `=` のときは今と同じく保留しない（D9）。
- 左辺に `ref`／`ref mut` を付けた形（`ref t[1..3, ..] += u`）は、`ref xs[0..2] = 5` と同じく `E1012`（kind `assign_to_immutable_place`）で、help `write the target without 'ref': 't[…] += …'`。

### 2. すべての型に共通の意味

- **場所**: 複合代入の左辺に書けるのは、`=` の左辺に書ける場所（`let mut` のローカル、`ref mut` の参照先 `deref r`、2027 のフィールドと配列・リスト・`Vec`・固定長配列の要素）と、Phase 3 の添字の対象（§7）。組み込みでない型の対象は、このうち要素を経由しない場所に限る（§4）。それ以外は `=` と同じ診断（`E1012` kind `assign_to_immutable_place`、`let` の束縛は `E1014` kind `mutate_immutable_value`）。2026 のフィールドの対象（`b.n += 1`、`model.w += g`）は `b.n = b.n + 1` と同じ `E1012` で、複合代入のときだけ help `record fields can be updated with '+=' from edition 2027` を付ける（`=` の診断は変えない: S2）。参照そのものではなく参照先を更新するときは `deref r += e` と書く（自動の参照外しはしない）。対象の型が参照（`m: ref mut Tensor.Mut<f64>` の `m += 1.0`、`r: ref mut i64` の `r += 1`）なら `E1005`（kind `compound_assign_unsupported_target`）、help `write 'deref m += …' to update what 'm' refers to`。
- **評価順（EVL の新しい規則。番号は実装の時点の次の空き。A38 の EVL の規則が先に入れば、その後の番号）**: `p op= e` は、(1) 場所の部分を 1 回だけ評価し、(2) 古い値を読み、(3) `e` を評価し、(4) `古い値 op e` を書く。場所の部分は、参照を作る式と添字の部分である。添字の中は A38 の添字の評価順に従い、土台の部分を先に、次にキーの成分を左から右へ評価する。これは `p = p op e` の右辺が最初の `p` を評価する位置と同じで、場所の部分は 1 回だけ評価する。順は対象の型に依らない。組み込みの添字は今もこの順なので（A38 の現状）、Phase 1 は A38 を待たない。`=` の EVL-012（右辺が先）とは違い、場所の部分が右辺より先である（Python・C#・Java と同じ）。範囲の検査・`Slice.Sel` の形への解決・アドレスの計算は、読むときと書くときにそれぞれ行う（右辺の前に求めたアドレスを右辺の後で使い回すことを前提にしない）。
- **右辺の守り（OWN の新しい規則。番号は同じく実装の時点の次の空き。A38・A39 の OWN の規則が先に入れば、その後の番号）**: 右辺 `e` は対象の場所を読めるが、書き込み（代入）・move・排他借用はできない。所有権の検査は、`e` を評価する間だけ対象の場所に共有の守りを置く（`exclusive_slice` の境界と同じ `self.held`）。違反は `E1014`（kind `borrow_conflict`）で、関連する位置に対象、help `the right side of '+=' cannot change 'x'; compute it into a 'let' first`（演算子と名前は実際のもの）。これで (2) と (3) の順はどの型でも観測できない。R2 の `x += bump (ref mut x)`、`x += { x = 5; 1 }`、`xs[2] += { xs = [9]; 5 }`（今の `=` ではトラップする R5 の形）、非 Copy の対象を右辺で move する形は、すべてコンパイル時に `E1014`。
- **型**: 複合代入の式の型は `unit`（TYP-013 の「代入の式」に複合代入を含める）。
- **閉包と計算式**: 捕捉した変数は閉包の中で不変なので、閉包の中の `x += 1` は `x = x + 1` と同じく `E1014`。計算式の継続の中の外側の `let mut` への複合代入も `=` と同じく `E1014`。`Parallel.*` のコールバックの中で書き込みが競合することはない。

### 3. 組み込み型（Phase 1）

- **対象の判定**: 型検査の時点で対象の型が分かっていて、組み込みの算術を持つ型（整数・浮動小数点・マスクを除く SIMD の数値ベクトル・`string` と `utf8string`。`Classes::intrinsic` が `Add`・`Sub`・`Mul`・`Div`・`Rem` のどれかで真になる型）なら組み込みの道を取る。演算子ごとの `Classes::intrinsic` ではなく型で決めるので、演算子がその型に組み込みでない形（`f64` の `%=`、`string` の `-=`）は、下の型の規則のとおり `p = p op e` と同じ誤りになる。対象の型がそれ以外の組み込みの型（`bool`・`char`・SIMD のマスク・タプル・配列など。参照の型の対象を除く）なら、`AddAssign` のインスタンスを引かずに `p = p op e` と同じ型付き IR に下げ、`p = p op e` と同じ結果にする（`b += true` は `b = b + true` と同じ `E1005 no instance for Add<bool>`。A37 の手順 5 の前に書ける利用者の `instance Add<bool>`・`instance Add<[i64]>` があれば、`b = b + true` と同じくそれを使う。手順 5 の後はこれらが `orphan_instance` になるので起きない）。§4 と D4 の「`Target` が組み込みの型なら組み込みの道を通る」は、この 2 つの扱いを指す。
- **脱糖**: 型付き IR は `p = p op e` と同じ（`TypedExprKind::Assign(p, Binary(op, p の読み出し, e))`）。場所の部分が単純（ローカル、ローカルを根とするフィールドの経路、ローカルかリテラルの添字、ローカルの参照の `deref`）なら、そのまま書き下した形と型付き IR が一致し、LLVM IR も位置の情報（トラップの位置・デバッグ情報・`--overflow-report` の地点）を除いて一致する。単純でない場所は部分の値をコンパイラの一時の束縛に 1 回だけ評価する（`xs[next (ref mut k)] += 1` は `{ let $i = next (ref mut k); xs[$i] = xs[$i] + 1 }`、`deref (pick (ref mut x) 1) += e` は `{ let $r = pick (ref mut x) 1; deref $r = deref $r + e }`）。
- **型の規則**: `p = p op e` と同じ（`same` で両辺を合わせる、リテラルは対象の型を取る、`%=` は整数だけ: `f64` の `x %= 2.0` は `x = x % 2.0` と同じ `E1005 no instance for Rem<f64>`、`s -= t` は `string` の `-` と同じ誤り）。`@checked (x += 1)` は溢れると `OverflowException` を投げ、`x` は書き換わらない。`--overflow-report` は折り返した地点を `+=` の位置で報告する。
- **文字列**: `s += t` は `s = s + t` と同じく新しい文字列を作る（長さに比例する確保）。多数を繋ぐときは `String.concat`・`String.join` を使う、と文書に書く。
- **型が決まらない対象**: 型検査の時点で対象の型が推論中（`Type::Infer`。`let mut total = 0` の直後など）なら組み込みの道で型検査し、`finish`（`src/polymorph.rs` 5252 行付近）で解決した型が組み込みでない型（`Tensor<f64>` など。§4 の道を取るはずだった型）なら `E1005`（新しい kind `compound_assign_unsupported_target`）`'+=' must know the type of its target where it is checked; annotate 'x'`。組み込みの型に解決したとき（演算子がその型に組み込みでない `let mut s = Default.default ()` の後の `s -= "x"`、ほかの組み込みの型の `let mut b = Default.default ()` の後の `b += true`）は、型が分かっている対象と同じく `s = s - "x"`・`b = b + true` と同じ結果（`E1005` の `no instance for Sub<string>`・`no instance for Add<bool>`）で、この診断は出さない。
- **型変数の対象**: 総称関数の型変数 `'a` を型に持つ対象は `E1005`（同じ kind）`'+=' needs a concrete target type; write 'x = x + y' for a generic 'a`。`AddAssign` は制約に書けないので、特殊化の先で黙って確保する道を作らない（D6）。
- **組み込みでない型（Phase 1 の時点）**: `E1005`（同じ kind）`'+=' on Tensor<f64> is not available; write the update without '+='`。Phase 2 で `AddAssign` の解決に置き換わる（kind は型変数と推論中の対象のために残る）。

### 4. `AddAssign` 系（Phase 2）

- **クラス**: 組み込みクラス `AddAssign<Target, Rhs>`・`SubAssign`・`MulAssign`・`DivAssign`・`RemAssign`（`BUILTIN_CLASSES` 36 → 41）。メソッドはそれぞれ `add_assign`・`sub_assign`・`mul_assign`・`div_assign`・`rem_assign` で、型は **`ref Rhs -> ref mut Target -> unit`**（対象は最後: D-59。`IndexMut` の `index_mut :: ref Key -> Value -> ref mut Self -> unit` と同じ形）。D-66 の設計の案（反証の修正を含む）の「値を消費して返す形」と、右辺を値で受ける形、`Add` への暗黙の代替を採らない理由は D1。
- **ヘッダ**: `instance AddAssign<Target, Rhs>` の 2 引数のヘッダを、`Index` の 3 引数のヘッダと同じく組み込みのクラスに限って許す。制約には書けず（`E0002`）、メソッドの値にもならない（`E1002`）。`Add.add` のような名前での呼び出しはない。
- **登録の規則**（D4）:
  - `Target` が型変数か組み込みの型なら `E1016`（kind `builtin_instance_override`）。組み込みの型は、数値・`string`・SIMD のような組み込みの演算を持つ型だけでなく、タプル・配列・関数などすべての組み込みの型を含む。`Target` が組み込みの型なら、解決はインスタンスを引かずに組み込みの道を通るので、そのインスタンスは選ばれないからである。匿名共用体は既存の `E1041`（kind `anonymous_union_instance`）。利用者の `extern type` は、組み込みでない名前付きの型として `Target` になれる。
  - `Rhs` の型変数は `Target` に現れること（`AddAssign<Tensor<'a>, 'a>` は可。違反は `register_index_instance` と同じ `E1027` kind `instance_context_variable`）。
  - 利用者のモジュール（依存パッケージを含む）のインスタンスは、`Target` か `Rhs` の少なくとも 1 つの頭が、このプログラムで宣言した型（レコード・名前付きの共用体・`extern type`）であること。std の型、組み込みの型、`ref`・`dyn` は、このプログラムの型ではない。違反は `E1016`（kind `orphan_instance`）。これは A37 の D4 と同じ 1 つの規則で、NC20 は `dispatch_positions` に `AddAssign` 系の位置（`Target` と `Rhs`、[0, 1]）を足すだけで、判定の関数は A37 の `validate_orphan_instance` を使う。単位はプログラムで、`Target` か `Rhs` を宣言したモジュールである必要はない。`Target` が std の型（`Tensor`・`Tensor.Mut` など）なら、実際には `Rhs` が利用者の型でなければならない。std のモジュールにはコンパイラの規則を当てず、`src/stdlib.rs` の読み込みのテストが `dispatch_positions` で確かめる（A37 の D4）。
  - `(Target, Rhs)` が単一化で重なれば `E1016`（kind `overlapping_instance`）。重なりの報告の位置は A37 のとおり（利用者と std の組は利用者のインスタンスの位置）。
  - 検査の順は `builtin_instance_override`・`anonymous_union_instance`（`E1041`）、`orphan_instance`、`overlapping_instance`。
- **解決**: 対象の型の頭が分かっていて組み込みでない名前付きの型なら、`Target` が単一化できるインスタンスを候補にする。候補が 1 つならその `Rhs` を右辺の期待型にする。複数なら、右辺が接尾辞のないリテラル（構文上のリテラルと、その単項マイナス）のときは `Rhs` が要素型の型変数の候補を選び（NC19 の規則 (a) と同じ）、それ以外は右辺を期待型なしで型検査してから、`(Target, Rhs)` の両方が単一化できる候補を選ぶ（候補の試行は NC16 の単一化の巻き戻しで行う。`Tensor<Tensor<f64>>` の対象に `Tensor<f64>` の右辺なら、`Rhs` が要素型の候補だけが残る）。0 件は `E1005`（kind `no_instance`）`no instance for AddAssign<BigInt, BigInt>`。help は、対象の型に対応する演算子のインスタンス（`Add<BigInt>` など）があれば `write 'x = x + y'`（確保する書き方を利用者が明示して選ぶ）、なければ `define an instance of AddAssign<BigInt, BigInt>`。2 件以上は `E1043`（kind `ambiguous_instance_candidates`。NC16・NC19 と共通）。
- **脱糖**（D1）: 場所の部分が単純なとき（§3 の「単純」と同じ）、`p op= e` は `TypedExprKind::Call(メソッド, [BorrowOperand(e), Borrow(p, true)])` に下げる（`IndexMut` と同じ。`src/check.rs` 14919〜14931 行付近）。`p` が `deref r` なら、`Borrow` を作らずに `r` を渡す（再借用。`IndexMut` が `ref mut` の対象をそのまま渡すのと同じ）。場所の部分が単純でないときは、§3 と同じく部分の値を一時の束縛に 1 回だけ評価してから、同じ呼び出しにする（`deref (pick (ref mut x) 1) += y` は `{ let $r = pick (ref mut x) 1; AddAssign.add_assign (ref y) $r }`）。評価順は §2 の「場所の部分 → 右辺 → 書く」で、場所の部分の中は A38 の添字の評価順（土台、キーの成分を左から右）に従う。右辺は自動で貸すので `x += y` は `y` を move しない（A40 の前は、`x += ref y` と書くと `Rhs` が `ref Tensor<…>` になってインスタンスがなく、help `the right side is lent automatically; write 'x += y'`）。右辺の型が参照（`ref T`・`ref mut T`）のときは A40 の規則に従う（NC20 と A40 は、後に入る方が合わせる。A40 が入った後は、`x += ref y` を `x += y` と同じく受理する（A40 の仕様 1 の「被演算子の式の形は問わない」と、A40 の「前提とする他チケットのインターフェース」の NC20 の項: `ref Rhs` には参照をそのまま貸す）。後に入る方が §8 の行とテストを合わせる）。所有権の検査は、右辺を評価する間は §2 の守りを置き、その後で対象を排他借用する。右辺の loan は呼び出しの終わりで閉じ、インスタンスの本体が右辺の借用を対象へ書き込んだときの loan は A31 の規則（書き込み先の持ち主に残る）に従う。
- **場所の規則**（D1・D2）: この道の対象は `=` の左辺の規則（§2）に従う。書けるのは、`let mut` のローカル、`ref mut` の参照先、それらを根にしたレコードのフィールドの経路（`model.w += g`、`r: ref mut Model` の `r.w += g`）である。フィールドの経路は A39 のフィールドの排他借用で通り、edition 2027 以降のソースに限る。2026 では `=` と同じ `E1012`（§2。help `record fields can be updated with '+=' from edition 2027`）。組み込みの配列・リスト・`Vec`・固定長配列の要素を経由する経路（`ts[i] += y`、`model.ws[i] += y`）は、要素単位の排他借用がないので `E1014`（kind `mutate_immutable_value`）で、help `'+=' on Tensor<f64> borrows its target exclusively, which an element cannot be; keep the value in a 'let mut' local or a record field`。組み込みでない型の添字の対象（`t[1, 2] += 1.0`、`t[1..3, ..] += u`）はこの規則の外で、§7 の道（`Index`＋`IndexMut`、`IndexBorrowMut`）が扱う。共有参照を経由すると `E1014`（kind `mutate_through_shared_reference`）、`Drop.drop` の根は `E1012`（kind `drop_pass_exclusive_value`）、`Drop.drop` の中の持ち主のフィールドは、フィールドへの代入と同じ `E1012`（kind `drop_replace_whole_value`。A39）。
- **重なり**: `x += x`（`Tensor`）は同じ呼び出しで `BorrowOperand(x)` と `Borrow(x, true)` が並ぶので `E1014`（kind `borrow_conflict`）。help `copy the right side first with 'Tensor.clone', bound with 'let'`。`model.w += model.w` も同じ `E1014`。`model.w += model.g` は別のフィールドの loan なので受理する（A39。`src/ownership.rs` 140〜142 行の `Place::overlaps` は、2 つのフィールドの経路を短い方の長さまで比べるので、別のフィールドを重ねない）。暗黙のスナップショットは作らない。
- **std のインスタンス（NC20）**: 演算子 `Add`・`Sub`・`Mul`・`Div`・`Rem` のそれぞれについて、`Target` ∈ {`Tensor<'a>`, `Tensor.Mut<'a>`} × `Rhs` ∈ {`Tensor<'a>`, `Tensor.View<'a>`, `'a`} の 6 個、計 30 個。文脈は NC06 と同じく使う演算子のクラスと `Copy<'a>`（`instance (Add<'a>, Copy<'a>) => AddAssign<Tensor<'a>, Tensor<'a>>`）。`View`・`MatrixView` などの読み取り専用の窓を `Target` に持つインスタンスはない（`v += y` は `E1005`）。本体は §5 のカーネルを 1 回呼ぶ。`Rhs` が `'a` の 10 個（`AddAssign<Tensor<'a>, 'a>`・`AddAssign<Tensor.Mut<'a>, 'a>` × 5 クラス）は振り分けの位置に裸の型変数を持つので、A37 のテスト `std_instances_with_open_dispatch_positions_are_listed`（`src/stdlib.rs`）の一覧に同じ変更で足す。クラスと同じ変更で入るので、先にある利用者のインスタンスとは重ならない。後から振り分けの位置に裸の型変数を持つ std の `AddAssign` 系を足すのは、互換性のない変更として別に承認を得る。std の `AddAssign<Tensor<'a>, 'a>` は利用者の `AddAssign<Tensor<Money>, Money>` と重なり、`overlapping_instance` を利用者のインスタンスの位置に報告する。
- **意味**: 右辺を対象の形へ一方向にブロードキャストする（対象の形は変わらない）。できなければ、1 要素も書かずにトラップする（NC06 の `*_in_place_or_trap` と同じ `assertion failed`）。各要素を `t_i = t_i op r_i`（スカラーなら `t_i op s`）で更新する。整数は折り返し（`@checked` はインスタンスの中を検査しない）、整数のゼロ除算は要素の途中でもトラップする（それまでに書いた要素は書き換わったまま。トラップは `try` で捕まらず、`Domain` の中なら外の値は変わらない）。浮動小数点は要素型の演算子そのもの。

### 5. `Tensor` の in-place カーネルと確保（Phase 2）

- **NC06 から使うもの**: 累積のインプレース版 `Tensor.add_in_place`・`sub_in_place`・`mul_in_place`・`div_in_place`・`rem_in_place :: { r } (<Op><'a>, Copy<'a>) => ref View<'a> -> ref mut Mut<'a> { r } -> Result<unit, Error>`（`target_i = target_i op rhs_i`、右辺だけをブロードキャスト。NC06 の D8）と、その非公開の双子 `*_in_place_or_trap`（形が合わなければ書く前に `assertion failed` でトラップ。NC06 の仕様 5・D6）。`AddAssign<Tensor.Mut<'a>, View<'a>>` の本体は `add_in_place_or_trap` を 1 回呼ぶだけ。公開の関数と双子は NC06 の `map2_in_place_kernel`（か演算ごとの直接ループ）を共有するので、演算子の形と関数の形が別の性能経路を選ばない（AGENTS.md）。
- **NC20 が足す公開の関数**（スカラーの右辺。NC06 のスカラー版 `add_scalar` などは新しいテンソルを返し、既存の `map_in_place` は呼び出しごとに shape と strides を複製するので、確保 0 回の `x += 1.0` に使えない）: `Tensor.add_scalar_in_place`・`sub_scalar_in_place`・`mul_scalar_in_place`・`div_scalar_in_place`・`rem_scalar_in_place :: { r } (<Op><'a>, Copy<'a>) => own 'a -> ref mut Mut<'a> { r } -> unit`（`m_i = m_i op s`。対象の窓が最後: D-59。形の不一致がないので双子は作らない。NC06 の仕様 5 の「双子を作らないもの」と同じ扱い）。連続の窓は 1 次元の単純なループ（LLVM の自動ベクトル化が効く形。NC06 の設計の確認規則のとおり `clang -x ir -O3` の後の IR で確かめる）、それ以外はストライドの走査で、どちらも shape・strides を複製しない。名前は既定案（D7）。
- **同じ形の `Tensor` の右辺**: `AddAssign<Tensor<'a>, Tensor<'a>>` の本体は、形が同じなら A39 の規則で `ref mut target.data[0..]` を取り、`rhs.data` と NC06 の要素の演算と同じ 1 次元のループで直接走査する（`Tensor.view` は 2 回確保するので作らない）。形が違えば `view` を作って `*_in_place_or_trap` を呼ぶ。
- **フィールドの排他借用（D2。A39 の規則）**: std の本体は、利用者のコードと同じ A39 の一般の規則でフィールドを排他借用する。std だけの特権、許可表（起票時の案の `exclusive_field_record`）、インスタンスの本体かどうかの判定は置かない。A39 は edition 2027 以降のソースでだけ働き、std は `Edition::LATEST`（今は 2027）で読むので、`std/Tensor.tz` の本体は `ref mut target.data[0..]` を書ける。`Tensor` の不変条件（`data.length` と形の積）は、排他スライスの長さが固定である（`ReplaceExclusiveSlice`、`src/check.rs` 14995 行付近）ことで保たれる。ほかの不変条件は、今と同じくモジュールの中のコードが守り、不透明なレコードのフィールドにはモジュールの外から触れない（`E1022`）。
- **確保**（`Alloc.since` を `+=` の文の直前と直後に置いて測る。関数の戻り値の複製は含めない。速度の合否の閾値は置かない）:
  - スカラーの右辺（`x += 1.0`、`deref m += 1.0`）: 0 回。
  - `Tensor` の対象と同じ形の `Tensor` の右辺（`x += y`）: 0 回（本体は A39 の規則で `ref mut target.data[0..]` を取って `rhs.data` と直接走査し、`Tensor.view` を作らない）。
  - フィールドの対象（edition 2027 の `model.w += g`。`w`・`g` は同じ形の `Tensor<f64>`）: 0 回。
  - 連続な `Mut` の対象と、同じ形の連続な `View`・`Tensor` の右辺: 0 回。
  - ブロードキャストやストライドのある右辺: 軸の数に比例する作業領域（`broadcast_to` の形とストライド）だけで、要素数に比例する確保はしない。
  - `+=` の後で対象の局所を返す関数の複製は、対象の型で決まる。`Tensor`・`Tensor.Mut` と、それをフィールドに持つレコードは Copy でないので、返すときも move で、複製しない（HEAD で確認: `ref mut t` を取った後に `Tensor.zeros [1000]` の `t` を返す関数は、返すだけの関数と同じ 3 回・8,016 バイト）。対象の型が Copy（配列をフィールドに持つ利用者のレコードなど）なら、`Borrow(p, true)` が対象の局所を借用の根にし、A17 の X1（借用の根になる局所は関数全体で移動しない。`src/last_use.rs` 55〜58 行）が移動を止めるので、返すときに複製する（`record Acc { xs: [f64] }` は 1 回・8,000 バイトが 2 回・16,000 バイトになる）。`IndexMut` の `t[k] = v` と同じ今の制限で、PM10 の Phase 2 が直す。フィールドの対象では持ち主の局所が根になるので、Copy の持ち主を後で move しても同じく複製する。
  - `Tensor.update` は使わない（R4 の複製のため。PM10）。

### 6. 書き込み窓 `IndexBorrowMut`（Phase 3）

- **クラス**（D3）: 組み込みクラス `IndexBorrowMut<Self, Key, Output>`（`BUILTIN_CLASSES` 41 → 42）。メソッド `index_borrow_mut :: ref Key -> ref mut Self -> Output`（`Index` の `index :: ref Key -> ref Self -> Output` の排他版）。インスタンスは std のモジュールだけが定義できる。利用者のモジュールのインスタンスは `E1016`（kind `builtin_instance_override`）`IndexBorrowMut instances are provided only by the standard library`（判定は登録の既存の `builtin_instance_override` の検査で行い、インスタンスを宣言したモジュールの由来が `ModuleOrigin::Std` でなければ拒否する。A37 の仕様 4 の検査の順で orphan 規則より前に行い、A37 の `validate_orphan_instance` と振り分けの位置の表 `dispatch_positions` には入れない。A37 の仕様 4 の「表に入れない」と、このチケットの D4 の「この規則の外」と同じ）。std だけに限るのは公開の約束を小さく始めるためで、信頼の問題ではない（本体は普通の関数として借用検査される。利用者に開くのは受理を増やす拡張なので後からできる）。キーの型が借用を持たないことを登録で検査するので、結果が持つ loan は土台の場所の排他 loan（と、土台の値が持つ loan）だけになる。制約・メソッドの値にならない。
- **書き方**: `ref mut t[c1, …, cn]`（NC18 の角括弧のキーに `ref mut` を付けた形）と、長い形の `ref mut t[[k1, …]]`。結果の型は `Output`（`Tensor.Mut<'a>`。`ref mut Tensor.Mut<'a>` ではない。`ref` が式に吸収される NC16 と同じ）。成分がすべて整数の `ref mut t[i, j]` は、NC16・NC18 の `ref t[i, j]` と同じ `E1003`（kind `index_ref_on_value`。要素単位の排他借用はない）。
- **脱糖と借用**: 土台は `IndexMut` と同じく `Borrow(土台, true)`（土台が `ref mut` の参照ならその再借用）、キーは `BorrowOperand`。評価順と所有権の検査は A38 の添字の規則に従い、`exclusive_slice` と同じ形にする。土台の場所の部分を先に評価し、土台の場所に共有の守りを置いてキーの成分を左から右へ評価し（キーは土台を読めるが、代入・move・排他借用は `E1014` kind `borrow_conflict`）、その後で排他の loan を取る。結果が持つのは土台の場所の排他 loan（と、土台の値が持つ loan）だけで、キーの loan は式の終わりで閉じる（NC04 の `Index` の規則と同じ）。窓が生きている間、土台はどの形でも使えない（`E1014`）。`Tensor.Mut` の土台は `narrow_mut` と同じ再借用なので、窓が死んだ後は土台の `Mut` をまた使える。
- **土台の場所**: `let mut` のローカルと、`ref mut` の参照先（全 edition）。それらを根にしたレコードのフィールドの経路（`ref mut model.w[0, ..]`、`model.w[0, ..] += g`）は A39 の規則で通り、edition 2027 以降のソースに限る（単独の `..` の成分は 2028 から。2027 では長い形 `ref mut model.w[[…]]` で書く）。2026 では今と同じ `E1014`（kind `mutate_immutable_value`。help は A39 のとおり edition 2027 を案内する）。要素の経路（`ref mut ts[0][1, ..]`）は `E1014`（`mutate_immutable_value`）、共有参照を経由すると `E1014`、`Drop.drop` の根は `E1012`（`drop_pass_exclusive_value`）、一時値の土台（`ref mut (mk 100)[1..2, ..]`）は `E1013`（`borrow_of_temporary`）。
- **キー**: NC18 の厳格なキー `Slice.Sel` の配列 `[Slice.Sel]` だけ（D3。`Slice.Axis` は範囲外を丸めるので、書き込み先が黙って縮むのを避ける）。窓になれる成分は `Item`、刻みが 0 でない `Span`、`All`、`Rest`、`NewAxis`（どれも単射を保つ。刻み 0 はトラップで、文は NC18 の仕様 3.4 のとおり）。範囲外の位置はトラップ。境界の意味（`^k`、`a..b` の終端など）は NC17・NC18 の基準表のまま。`IndexBorrowMut` のインスタンスがないときは NC16・NC18 の診断のまま: 土台を借りる `Index` の窓（`[Slice.Axis]` のキー、`View`・`Matrix`・`MatrixView` の土台）の `ref mut` は `E1042`（kind `index_view_not_writable`）で、NC20 は `[Slice.Axis]` のキーにだけ help `use Slice.Sel keys for a writable window, for example 'ref mut t[1..<3, ..]' in edition 2028` を足す。マスク（`Tensor<bool>`）と位置の一覧はコピーを選ぶ（`Index` の結果が土台を借りない）ので、`ref mut t[mask]` は NC16 の `E1003`（kind `index_ref_on_value`）。
- **連鎖した書き込み窓**（D14）: `ref mut t[1..3, ..][0, ..]` では、NC16 の「`ref` が覆う」規則で内側の `t[1..3, ..]` も書き込み窓になり、外側の土台はその結果の一時値の `Tensor.Mut` になる。土台の規則のとおり `E1013`（`borrow_of_temporary`。位置は内側の角括弧の式）で、help を足す。help は内側と外側の角括弧の綴りをそのまま引く（`ref mut` の連鎖は `bind the inner window first: 'let mut w = ref mut t[1..3, ..]', then 'ref mut w[0, ..]'`、複合代入の対象は `bind the inner window first: 'let mut w = ref mut t[[A]]', then 'w[[B]] += …'`）。複合代入の対象の `t[1..3, ..][0, ..] += u` は、内側の `]` の次の字句が `[` なので §1 の保留に当たらず、NC18 のままの `E0002`（`slice_must_be_borrowed`。位置は内側の角括弧。HEAD は複数の成分の角括弧を読めず、カンマの位置の `expected_token` の `E0002`）。長い形か式だけの成分で書いた連鎖（`t[[A]][[B]] += u`、`t[[A]][[0, 1]] += 1.0`。`A`・`B` は `[Slice.Sel]` の値。全 edition）は、パーサーの `E0002` に当たらない。複合代入の対象の土台が組み込みでない型の添字の式（NC16 の添字クラスの式）なら、内側も書き込み窓（`IndexBorrowMut`）として解決する。内側に `IndexBorrowMut` のインスタンスがあれば、外側の土台がその一時値になるので、`ref mut t[1..3, ..][0, ..]` と同じ `E1013` にする（help は上のとおり複合代入の形）。インスタンスがなく、内側が土台を借りる `Index` の窓（`[Slice.Axis]` のキー、`View`・`Matrix`・`MatrixView` の土台）なら `E1042`（`index_view_not_writable`）。ソースに `ref mut` がないので、位置は内側の角括弧の式、文言は `'+=' cannot write through this index; it gives Tensor.View<f64>, a shared borrow of its base` とし、NC16 の修正案（`ref mut` を `ref` にする）は付けない（`[Slice.Axis]` のキーには §6 の help）。内側が土台を借りない `Index`（マスク、値を返す利用者の `Index`）なら、外側の土台が一時値になるので help のない `E1013`（HEAD の `g[0][1] = 5` と同じ）。土台が組み込みの 6 型の添字（`ts: [Tensor<f64>]` の `ts[0][[1, 2]] += 1.0`）なら、§6 の要素の経路と同じ `E1014`（`mutate_immutable_value`）。NC16 の `E1042`（kind `index_view_requires_ref`）は出さない。`ref` を足す案内に従うと土台が `View` になり、`View` には `IndexBorrowMut` がない。書き方は `let mut w = ref mut t[1..3, ..]` の後の `w[0, ..] += u`（`Tensor.Mut` の土台の窓）。共有の窓の連鎖 `ref t[1..3, ..][0, ..]` は NC18 のとおり通る（A37 の事実 `returns_contents`）。
- **std のインスタンス**: `IndexBorrowMut<Tensor<'a>, [Slice.Sel], Tensor.Mut<'a>>`（A39 の規則で `self.data` を排他借用して窓を作る。特権は要らない）と `IndexBorrowMut<Tensor.Mut<'a>, [Slice.Sel], Tensor.Mut<'a>>`（`narrow_mut` と同じ再借用。特権は要らない）。どちらも NC18 の非公開の `sel_layout`（キーと土台の配置から出力の offset・shape・strides を求める。データに触れない）を共有し、`View` の代わりに `Mut` を作る。`Matrix`・`MatrixView` は対象外。
- **edition**: 範囲・`^`・単独 `..` の成分は NC18 のとおり 2028 から。式だけの成分（`Slice.Sel` の値、整数）は全 edition。

### 7. 添字の対象への複合代入（Phase 3）

- **窓のキー**（範囲・単独 `..`・`Slice.Sel` の成分を含むキー。`^k` は NC18 の D2 のとおり位置で、窓の印ではない）: `t[k] op= e` は次に下げる。順序は「土台の部分、キー、右辺、書く」で、§2 の評価順（場所の部分 → 右辺 → 書く）と A38 の添字の順（土台、キーの成分を左から右）に一致する。古い値は `add_assign` のカーネルが右辺の後に読む（§2 の守りがあるので、読む時点の違いは観測できない）。

  ```text
  { let $b = …                                   // 土台の場所の部分。呼び出しか一時値を含むときだけ（例: deref (pick (ref mut t)) の参照）
    let $k = k                                   // キーの成分を左から右へ。土台の場所に共有の守り（A38 の規則。仕組みは「段ごとの変更」の生成の束縛の印）。形への解決はしない
    let $e = e                                   // 土台の場所に共有の守りを置いて評価（§2。仕組みは同じ生成の束縛の印）
    let mut $w = IndexBorrowMut.index_borrow_mut (ref $k) (ref mut t)   // 今の形で解決し、排他 loan
    AddAssign.add_assign (ref $e) (ref mut $w) } // §4 の Tensor.Mut のインスタンス
  ```

  土台が副作用のない場所（ローカルとそのフィールドの経路）なら `$b` を作らない（A38 と同じく、型付き IR に余分な束縛を足さない）。可変な束縛の参照の参照外し（`let mut r` の `deref r`）は、A38 の D9 のとおり `$b` を作らず、守りを `r` にも置く。`$b` があるときは `ref mut t` の代わりに `$b` の参照を渡す（§4 の `deref r` と同じ再借用）。`$e` が `t` を借りたまま（`a[1..] += ref a[..^1]`）だと、窓を取る時点で `E1014`（`borrow_conflict`）、help `the right side borrows 'a', which '+=' writes; copy it first with 'Tensor.to_tensor' or 'Tensor.clone', bound with 'let'`。`Tensor.Mut` の土台（`deref m` の一部）でも、A37 の後は右辺の窓が `m` の場所を借りるので同じく `E1014`。
- **要素のキー**（成分がすべて位置: 整数と `^k`。`^k` を含むキーは、NC18 の Phase 2 で `Slice.Pos` のインスタンスが入るまで NC18 の `E1005`（`from_end_element_read_unsupported`））: `t[k] op= e` は、同じ `(Self, Key)` の `Index`（読み出し）と `IndexMut`（書き込み）があり、値の型が同じで、その型が §3 の組み込みの道を取る型（整数・浮動小数点・マスクを除く SIMD の数値ベクトル・`string` と `utf8string`。演算子ごとの `Classes::intrinsic` ではなく型で決める）のときに、次に下げる。演算子がその型に組み込みでない（`f64` の `%=`）ときは、§3 の `x %= 2.0` と同じく `$n` の `E1005`（`no instance for Rem<f64>`）になる。どちらかのクラスがなければ、ないほうのクラスを名指して `E1005`。値の型が型検査の時点で推論中（`Type::Infer`。`let mut t = Tensor.zeros [4]` の直後の `t[[0]] += 1.5` など）なら、§3 の推論中の対象と同じく要素の道で型検査し、`finish`（`src/polymorph.rs`）で解決した型が組み込みの道を取る型でなければ、下の「値の型がそれ以外」のときと同じ `E1005`（`compound_assign_unsupported_target`。`'+=' on an element of type Complex<f64> is not available`）にする（D6）。値の型が型変数なら、§3 の型変数の対象と同じ `E1005` と文言にする。値の型がそれ以外（`bool`・タプルなどほかの組み込みの型、`Tensor<Complex<f64>>` の要素、配列をフィールドに持つ Copy の利用者のレコード）なら `E1005`（kind `compound_assign_unsupported_target`）`'+=' on an element of type Complex<f64> is not available` で、help は §4 と同じく演算子のインスタンスがあるときだけ `write 't[k] = t[k] + e'` を付ける。組み込みでない型の `+=` は `AddAssign` だけで解決し、`Add` へ暗黙に代えない（D1・D11）。順序は「土台の部分、キー、古い値、右辺、書く」。

  ```text
  { let $b = …                                       // 窓のキーと同じ（必要なときだけ）
    let $k = k                                       // 土台の場所に共有の守り（A38 の規則。仕組みは「段ごとの変更」の生成の束縛の印）
    let $old = Index.index (ref $k) (ref t)          // 要素の値（Index が値で返す。数値・SIMD は Copy、文字列は Index が返した所有する値）
    let $e = e                                       // 土台の場所に共有の守りを置いて評価（§2。窓のキーの $e と同じ生成の束縛の印）
    let $n = $old op $e                              // 組み込みの演算（@checked なら溢れはここで例外）
    IndexMut.index_mut (ref $k) $n (ref mut t) }     // 引数はすべて生成の局所なので、A38 の右辺の束縛は要らない
  ```

  右辺 `e` は、窓のキーと同じく生成の束縛 `$e` で守りの下に評価する。A38 の `IndexMut` の守りはキーの引数を評価する間だけなので、`e` を `index_mut` の引数の中に書くと守りが効かない（`t[1, 2] += { t = Tensor.zeros [4, 4]; 1.0 }` を受理してしまう）。

  std には `Index<Tensor.Mut<'a>, [i64], 'a>`（`Mut` の要素の読み出し。結果は借用を持たない）と `IndexMut<Tensor<'a>, [i64], 'a>`（所有するテンソルの要素の書き込み。本体は 2027 の射影の書き込み `self.data[offset] = v` で書くので、特権も A39 も要らない: 「現状」の所有権の制限）を、NC16 の確保なしの要素の読み出しと同じ書き方（キーを借用で受け、`Array.copy` も `view` も作らない）で足す。NC16 が足した 1〜4 軸のキー K（`i64`・`(i64 * i64)`・`(i64 * i64 * i64)`・`(i64 * i64 * i64 * i64)`）については、`IndexMut<Tensor<'a>, K, 'a>`（同じく射影の書き込み）と `Index<Tensor.Mut<'a>, K, 'a>` だけを足す。`Index<Tensor<'a>, K, 'a>` と `IndexMut<Tensor.Mut<'a>, K, 'a>` は NC16 が足しているので足さない（足すと `E1016` の `overlapping_instance`）。要素のキーの形（タプルか配列か）は NC18 の仕様 2.3 の規則を `Index` と `IndexMut` の両方に当て、両方にタプルのインスタンスがあるときだけタプルにする（D11）。これで `t[[0]] = 5`（R6）も所有するテンソルで通る。
- `IndexBorrowMut` と、`Index`＋`IndexMut` のどちらを使うかはキーの型で決まる（`[Slice.Sel]` は窓、`[i64]` とタプルは要素）。両方の候補が残ることはない。
- 土台の場所の規則は、窓のキーも要素のキーも §6 と同じ。フィールドの経路の土台（`model.w[1, 2] += 1.0`）は A39 の排他借用で通り、edition 2027 以降に限る。

### 8. 診断

新しいコードは使わない（`E1043` は NC16・NC19 と共通）。新しい kind は `compound_assign_unsupported_target` の 1 つだけで、G23 の登録簿へ足し、`node scripts/gen-docs.mjs` で `diagnostics.md` の生成表を更新する。

| 条件 | コード | kind | メッセージ（英語。help） | 位置 |
| --- | --- | --- | --- | --- |
| 右辺が対象を書き換える・move する・排他借用する | `E1014` | `borrow_conflict` | 既存の文言。help `the right side of '+=' cannot change 'x'; compute it into a 'let' first`。関連する位置に対象 | 右辺の中の衝突したアクセス |
| 推論中だった対象の型が組み込みでない型に決まる（§3） | `E1005` | `compound_assign_unsupported_target` | `'+=' must know the type of its target where it is checked; annotate 'x'` | 対象 |
| 型変数の対象 | `E1005` | `compound_assign_unsupported_target` | `'+=' needs a concrete target type; write 'x = x + y' for a generic 'a` | 対象 |
| 対象の型が参照 | `E1005` | `compound_assign_unsupported_target` | `'+=' updates a place, not a reference`。help `write 'deref m += …' to update what 'm' refers to` | 対象 |
| 組み込みでない型（Phase 1 の時点だけ） | `E1005` | `compound_assign_unsupported_target` | `'+=' on Tensor<f64> is not available; write the update without '+='` | 演算子 |
| `AddAssign` のインスタンスがない | `E1005` | `no_instance` | `no instance for AddAssign<BigInt, BigInt>`。help `write 'x = x + y'`（演算子のインスタンスがあるとき）か `define an instance of AddAssign<BigInt, BigInt>` | 演算子 |
| 候補が 2 つ以上 | `E1043` | `ambiguous_instance_candidates` | NC16・NC19 と同じ形（候補を並べる） | 演算子 |
| 組み込みでない型の対象が組み込みのコレクションの要素を経由する（`ts[i] += y`） | `E1014` | `mutate_immutable_value` | 既存の文言。help `'+=' on Tensor<f64> borrows its target exclusively, which an element cannot be; keep the value in a 'let mut' local or a record field` | 対象 |
| edition 2026 のフィールドの対象（`b.n += 1`、`model.w += g`） | `E1012` | `assign_to_immutable_place` | `=` と同じ文言。複合代入のときだけ help `record fields can be updated with '+=' from edition 2027` | 対象 |
| edition 2026 のフィールドの経路を土台にした書き込み窓（`ref mut model.w[[…]]`） | `E1014` | `mutate_immutable_value` | A39 のとおり（help は edition 2027 を案内する） | `ref mut` の式 |
| `x += x`、`model.w += model.w`、右辺の窓が対象を借りたまま | `E1014` | `borrow_conflict` | 既存の文言。help はスナップショット（§4・§7） | 対象の排他借用 |
| 書き込み窓・添字の対象のキーが土台へ代入・move・排他借用する | `E1014` | `borrow_conflict` | A38 の文言（新しいコード・kind はない） | キーの中の衝突したアクセス |
| 左辺に `ref`／`ref mut` | `E1012` | `assign_to_immutable_place` | 既存の文言。help `write the target without 'ref': 't[…] += …'` | 左辺 |
| `x += ref y`（組み込みでない型。A40 の前の診断。A40 が先に入っていれば `x += ref y` は受理する。後に入る方がこの行とテストを合わせる） | `E1005` | `no_instance` | 既存の形。help `the right side is lent automatically; write 'x += y'` | 右辺 |
| 利用者の `AddAssign` 系のインスタンスで、`Target`・`Rhs` のどの頭もこのプログラムで宣言した型でない | `E1016` | `orphan_instance` | `AddAssign<Tensor<f64>, f64> needs a type declared in this program as its target or right-hand type`（help は A37 と同じ） | クラス名 |
| `AddAssign` 系の `Target` が型変数か組み込みの型 | `E1016` | `builtin_instance_override` | `register_index_instance` と同じ形 | クラス名 |
| 利用者の `IndexBorrowMut` のインスタンス | `E1016` | `builtin_instance_override` | `IndexBorrowMut instances are provided only by the standard library` | クラス名 |
| `IndexBorrowMut` のない窓の `ref mut`（`[Slice.Axis]` のキー、`View` の土台など） | `E1042` | `index_view_not_writable`（NC16） | NC16 の文言。`[Slice.Axis]` には §6 の help | `ref mut` の式 |
| `IndexBorrowMut` のない窓を内側にした複合代入の対象（`t[[Slice.range 1 3]][[B]] += u`） | `E1042` | `index_view_not_writable`（NC16） | `'+=' cannot write through this index; it gives Tensor.View<f64>, a shared borrow of its base`。修正案なし。`[Slice.Axis]` には §6 の help | 内側の角括弧の式 |
| 土台を借りない添字を内側にした複合代入の対象（`g[0][1] += 5`、`t[mask][[B]] += u`） | `E1013` | `borrow_of_temporary` | 既存の文言（help なし。HEAD の `g[0][1] = 5` と同じ） | 内側の角括弧の式 |
| マスク・位置の一覧の `ref mut` | `E1003` | `index_ref_on_value`（NC16） | NC16 の文言 | `ref mut` の式 |
| マスク・位置の一覧を対象にした `t[mask] += 1.0` | `E1005` | `no_instance` | `no instance for IndexMut<Tensor<f64>, Tensor<bool>>` など欠けたクラス。help `a mask or a list of positions selects copies; '+=' needs positions, ranges or '..'` | 角括弧 |
| 要素のキーの対象で、値の型が §3 の組み込みの道を取る型でない（`Tensor<Complex<f64>>` の `t[[0]] += c`、`bool`・タプルなどほかの組み込みの型、配列をフィールドに持つ Copy の利用者のレコード） | `E1005` | `compound_assign_unsupported_target` | `'+=' on an element of type Complex<f64> is not available`。help（演算子のインスタンスがあるとき）`write 't[k] = t[k] + e'` | 演算子 |
| 一時値の土台の書き込み窓 | `E1013` | `borrow_of_temporary` | 既存の文言 | 土台 |
| 連鎖した書き込み窓 `ref mut t[1..3, ..][0, ..]`、長い形の連鎖を対象にした `t[[A]][[B]] += u`（外側の土台が内側の窓の一時値） | `E1013` | `borrow_of_temporary` | 既存の文言。help は内側と外側の角括弧の綴りを引く（`bind the inner window first: 'let mut w = ref mut t[1..3, ..]', then 'ref mut w[0, ..]'`。複合代入の対象は `then 'w[[B]] += …'`） | 内側の角括弧の式 |
| 配列のスライスを対象にした `xs[1..3] += 1` | `E0002` | `slice_must_be_borrowed` | 今と同じ文言（型検査が土台を配列と知って出す） | 角括弧の式 |
| `AddAssign`・`IndexBorrowMut` を制約に書く／メソッドの値にする | `E0002`／`E1002` | 既存 | `Index` と同じ | 既存 |
| 形の不一致、刻み 0、範囲外 | トラップ | – | NC19 の演算子・NC18 の窓と同じ文言 | 実行時 |

### 9. native/WASM と資源上限

- native と WASM、`-O0` と `-O3` で同じ結果。WASM の import は増えない。新しい実行時の関数はない（std の Tsuzuri のコードと既存の命令だけ）。
- 構文の入れ子は `=` と同じ経路なので深さの上限 128 は変わらない。脱糖が足す一時の束縛は 1 つの複合代入あたり 5 つまで（§7 の要素のキーの `$b`・`$k`・`$old`・`$e`・`$n`。窓のキーは `$b`・`$k`・`$e`・`$w` の 4 つ）。インスタンスの解決は NC19 の上限（候補の数）に従う。

### 10. 例（実装後の意味。範囲の成分は edition 2028）

```tsuzuri
// Phase 1: 組み込み型
let mut x = 1
x += 2
x *= 5          // x は 15
let mut s = "ab"
s += "c"        // s = s + "c" と同じ（新しい文字列）
```

```tsuzuri
// 評価順: 場所の部分 → 右辺 → 書く。出力は "place 1"、"2"、"3"
def say :: i64 -> i64 = \n ->
    do! IO.writeln (to_string n)
    n

def pick :: ref mut i64 -> i64 -> ref mut i64 = \r n ->
    do! IO.writeln ("place " + to_string n)
    r
def f :: unit -> i64 = \() ->
    let mut x = 1
    deref (pick (ref mut x) 1) += say 2
    x
f ()
```

```tsuzuri
// Phase 2: Tensor の in-place（要素の確保なし）
let mut x = Tensor.zeros [2, 3]                  // Tensor<f64>
let row = Tensor.of_array [3] [1.0, 2.0, 3.0]
x += row                                         // 各行へブロードキャスト
x *= 2.0                                         // x の要素は [2.0, 4.0, 6.0, 2.0, 4.0, 6.0]

def bump :: ref mut Tensor.Mut<f64> -> unit
fn bump m = deref m += 1.0                       // 参照の先の窓を更新（再借用。確保なし）

let mut data = [1.0, 2.0, 3.0, 4.0]
let mut m = Tensor.borrow_mut [2, 2] (ref mut data)
m += 1.0                                         // let mut の Mut のローカル
bump (ref mut m)                                 // data は [3.0, 4.0, 5.0, 6.0]
```

```tsuzuri
// Phase 2: レコードのフィールドの対象（edition 2027 から。A39。2026 では E1012）
record Model { w: Tensor<f64>, g: Tensor<f64> }
let mut model = Model { w: Tensor.zeros [3], g: Tensor.of_array [3] [0.5, 0.5, 0.5] }
model.w -= model.g                               // NumPy: self.w -= grad。別のフィールドなので受理、文の確保なし
model.w += model.w                               // E1014（同じフィールドを貸して書く）
```

```tsuzuri
// Phase 3: 書き込み窓と添字の対象（edition 2028。NumPy: a[1, :] *= 2、a[:, 0:2] += 1）
let mut t = Tensor.init [3, 4] (\i -> i as f64)
t[1, ..] *= 2.0
t[.., ..<2] += 1.0
t[2, 3] -= 0.5                                   // 要素（全 edition）
let mut w = ref mut t[0..1, ..]                  // 行 0・1 の Tensor.Mut<f64>
Tensor.fill 0.0 (ref mut w)                      // w が生きている間 t は使えない

let mut a = Tensor.init [5] (\i -> i * 10)       // [0, 10, 20, 30, 40]
a[1..] += ref a[..^1]                            // E1014（NumPy は暗黙に複製する）
let v = ref a[..^1]
let prev = Tensor.to_tensor (ref v)              // 明示のスナップショット
a[1..] += prev                                   // [0, 10, 30, 50, 70]
```

## 設計

### 段ごとの変更

| Phase | ファイル | 関数・型 | 変更 |
| --- | --- | --- | --- |
| 1 | `src/lexer.rs` | `symbol` の表、`TokenKind` | `+=`・`-=`・`*=`・`/=`・`%=`（`PlusEqual` などの名前） |
| 1 | `src/parser.rs` | `expression_inner`（4263 行付近） | `=` の分岐の隣で複合代入を取り、`ExprKind::CompoundAssign` を作る |
| 1 | `src/syntax.rs`・`src/syntax_codec.rs`・`src/frontend_cache.rs`・`src/formatter.rs`・`src/computation.rs`・`src/warnings.rs` | AST の走査・符号化・印字・計算式・警告の走査 | 新しい変種。`FRONTEND_FORMAT` を 1 上げる。`src/warnings.rs` の `ExprKind` の走査（588〜674 行付近。`Assign(left, right)` の腕）にも足す |
| 1 | `src/check.rs` | `ExprKind::CompoundAssign` の型検査 | §3 の判定と脱糖。単純でない場所の一時の束縛。`finish` で確かめる対象の一覧（`undecided_borrows` と同じ形） |
| 1 | `src/check.rs`・`src/ownership.rs` | 複合代入の印 | 所有権の検査が §2 の守りを置く印。既定案は、型付き IR に変種を足さず、検査済みの関数に「複合代入の `Assign`・呼び出しの位置」の集合を持たせる（`Assign` を扱う 25 ファイル（`rg -lw 'Assign' src/`）を変えないため）。所有権の検査は、印の付いた `E::Assign` の値と、印の付いた呼び出しの右辺の引数を評価する間だけ `self.held` に対象の場所の共有 loan を置く |
| 1 | `src/polymorph.rs` | `finish` | 推論中だった対象の型が組み込みでない型なら `E1005`（§3） |
| 1 | `src/diagnostic_kinds.rs`・`src/hints.rs`・`src/explain/expected_expression.md`・`guide/card.md`・`guide/AGENTS.md.tmpl` | kind とヒントと AI 向けのガイド | `compound_assign_unsupported_target` を足す。ヒント `compound-assignment` を `**=` などの未対応の形の案内へ直す（D10） |
| 1 | `spec/rules/EVL.md`・`spec/rules/OWN.md`・`tests/conformance/` | 規則 | EVL・OWN の新しい規則（番号は実装の時点の次の空き。A38（EVL・OWN）・A39（OWN）が先に入れば、その後の番号）と適合性テスト |
| 2 | `src/polymorph.rs` | `BUILTIN_CLASSES`、2 引数ヘッダの登録（`register_index_instance` と同じ形の `register_assign_instance`）、解決（NC19 の表を使う）、`dispatch_positions` | `AddAssign` 系 5 クラス。§4 の登録の規則。`dispatch_positions` に [0, 1] を足し、orphan 規則の判定は A37 の `validate_orphan_instance` を使う（D4） |
| 2 | `src/parser.rs` | 915 行付近 | 2 引数のヘッダを `AddAssign` 系に許す |
| 2 | `src/check.rs` | 複合代入の組み込みでない道 | §4 の脱糖（`IndexMut` の 14919〜14983 行付近と同じ形。`deref r` は `r` を渡す。単純でない場所は一時の束縛）。2026 のフィールドの対象の help |
| 2 | `src/stdlib.rs` | A37 の一覧のテスト `std_instances_with_open_dispatch_positions_are_listed` | `Rhs` が `'a` の 10 個を一覧に足す（D4）。std だけの特権の許可表は作らない（D2） |
| 2 | `std/Tensor.tz` | スカラーの in-place と 30 個のインスタンス | §4・§5（窓・テンソルの右辺は NC06 の `*_in_place_or_trap`。同じ形の `Tensor` の右辺は A39 の規則で `ref mut target.data[0..]`） |
| 3 | `src/polymorph.rs`・`src/parser.rs` | `IndexBorrowMut` の登録（std だけ。キーの型が借用を持たないことの検査） | D3 |
| 3 | `src/check.rs` | `ref mut` の付いた添字（NC18 のノードと長い形）、添字の対象への複合代入 | §6・§7 の脱糖。評価順は A38 の規則（土台の部分、キーの成分、右辺）。複合代入の対象の土台が組み込みでない型の添字の式なら、内側も書き込み窓として解決する（D14。インスタンスがあれば `E1013` と help、なければ土台を借りる窓は `E1042`、土台を借りない添字は help のない `E1013`）。要素のキーの値の型が組み込みの道を取る型でなければ `E1005`（§7。推論中なら `finish` で確かめる） |
| 3 | `src/ownership.rs` | `IndexBorrowMut` の呼び出し、§7 の生成の束縛 `$k`・`$e` | `exclusive_slice` と A38 と同じ「守り → 排他 loan」。§7 の生成の束縛 `$k` と `$e` の初期化式を評価する間、土台の場所（`t`、`deref $b`、フィールドの経路）に共有の守りを置く。印は Phase 1 の複合代入の印と同じく、検査済みの関数に「束縛の位置 → 守る土台の式」の集合で持たせ、守りを置く処理は A38 の段 2 と共有する。連鎖した書き込み窓（D14）では、外側の `IndexBorrowMut`（または要素のキーの `IndexMut`）の土台が内側の `IndexBorrowMut` の結果の一時値のときだけ、`E1013`（`borrow_of_temporary`。7844 行付近で出す）に §6 の help を付ける |
| 3 | `src/parser.rs` | `index_or_slice` | `SliceMustBeBorrowed` の保留（`op=` のときだけ） |
| 3 | `src/syntax.rs`・`src/syntax_codec.rs`・`src/frontend_cache.rs`・`src/formatter.rs` | `ExprKind::Slice`（NC17 の `Slice { value, range, mutable: bool }`）、符号化、`FRONTEND_FORMAT`、印字 | 借用のない 1 成分の節点を表す（借用を `Option<bool>` にするか、借用のない 1 成分の節点を足す）。`FRONTEND_FORMAT` を 1 上げる。`t[1..3] += u` を冪等に印字する |
| 3 | `std/Tensor.tz` | `IndexBorrowMut` 2 個、`Index<Mut, [i64], 'a>`、`IndexMut<Tensor, [i64], 'a>`、NC16 の 1〜4 軸のキー K の `IndexMut<Tensor, K, 'a>`・`Index<Mut, K, 'a>`（`Index<Tensor, K>`・`IndexMut<Mut, K>` は NC16 にあるので足さない） | §6・§7 |

### 生成 IR の形

- 組み込み型: 書き下した `p = p op e` と同じ（読み出し、右辺、演算、store）。単純でない場所は、場所の部分の値を 1 回だけ計算する分だけ違う。
- `AddAssign`: インスタンスのメソッドの直接の呼び出し 1 回（`ref Rhs` は既存の `BorrowOperand` と同じく、一時値ならスタックの値を貸す）。
- 添字の対象: `index_borrow_mut` と `add_assign` の 2 回の呼び出しと、窓の形・ストライドの配列の drop。要素のキーは `index` と `index_mut` の 2 回。

## 前提とする他チケットのインターフェース

- [A37](_completed/A37-index-base-place-borrow.md): 結果が借用を持つ `Index` の土台を `Borrow(土台, false)` で下げる（右辺の窓 `ref a[..^1]` が `a` の場所を借り、書き込み窓との重なりが `E1014` になる）。D4 の orphan 規則（1 つの規則）: 判定の関数 `validate_orphan_instance`、表 `dispatch_positions`、kind `orphan_instance`、`src/stdlib.rs` の一覧のテスト `std_instances_with_open_dispatch_positions_are_listed`、重なりの報告の位置（利用者と std の組は利用者のインスタンスの位置）。
- [A38](_completed/A38-index-evaluation-order.md): 添字の評価順（土台、キーの成分を左から右、その後に境界の解決・呼び出し）。キーを評価する間の土台の共有の守り（キーが土台へ代入・move・排他借用すると `E1014` kind `borrow_conflict`）。排他の借用（`IndexMut`・`IndexBorrowMut`）はキーの後。`IndexMut` の `=` は右辺が先。NC20 の §2・§6・§7 の順はこれに従う。
- [A39](_completed/A39-field-exclusive-borrow.md): edition 2027 以降のソースで、`let mut` の持ち主か `ref mut` の参照先を根とし、経路がレコードのフィールドだけからなる場所の排他借用（loan はフィールドの場所に置く。別のフィールドとは重ならない）。2026 は今の `E1014` のまま。NC20 の Phase 2・3 の std の本体（`ref mut target.data[0..]`、`IndexBorrowMut` の窓）と、利用者のフィールドの対象（`model.w += g`、`ref mut model.w[[…]]`）が使う。
- [A40](_completed/A40-reference-operands.md): `op=` の右辺の参照の被演算子は A40 の規則に従う（NC20 と A40 は、後に入る方が合わせる）。組み込み型の `s += r`（`r: ref string`）は `s = s + r` と同じ型付き IR に下がるので、A40 の `string` の `+` の 3 つの道（`src/llvm.rs` の `binary`、`src/llvm_frame.rs` の `frame_concat` と `concat_length`）の規則に従う。参照の被演算子 `r` を `take_operand`・`drop_framed` に渡さない（A40 の仕様 3 の `string` の `+` の行）。
- [NC16](_completed/NC16-index-ref-and-zero-alloc-element-reads.md): 土台を借りる添字の `ref` の吸収（`ref t[[…]]` が `Tensor.View`）、`E1042`・`E1043`、要素のタプルのキー（あれば）。
- [NC17](_completed/NC17-edition-2028-inclusive-range-family.md): edition 2028 の範囲の形と `^k`（例と、範囲の成分の意味）。
- [NC18](_completed/NC18-multi-axis-bracket-indexing.md): 角括弧のキーのノード `ExprKind::MultiIndex { value, keys, borrow }`（成分は NC17 の `IndexComponent`）、`Slice.Sel`（`Item`・`Span`・`Whole`・`Rest`・`AddAxis`）、`[Slice.Sel]` のキーの解決（`slice_raw` 相当）、成分の規則（足りない末尾の成分は軸全体）。
- [NC19](_completed/NC19-operator-extension-instances.md): 演算子の解決の規則（リテラルを要素型に合わせる、1 つの表、`E1043`）。`AddAssign` の解決はこれを使う。orphan 規則は A37 の D4 と同じ 1 つの規則で、NC19 と NC20 は `dispatch_positions` の行が違うだけ。
- [NC06](_completed/NC06-tensor-ufuncs-and-elementwise.md): 累積のインプレース版 `*_in_place`（D8）と非公開の双子 `*_in_place_or_trap`（仕様 5・D6。形の不一致は書く前に `assertion failed`）、共有の要素のループ（`map2_in_place_kernel`）、要素の演算の定義と `left -> right` の順（D4）。
- 依存しない関連: [PM10](../_perfs/_completed/PM10-std-consume-copies.md)（std の所有を移す関数の複製の除去と、A17 の X1 の精密化。Phase 2 は NC20 の `+=`・`IndexMut`・`IndexBorrowMut` の対象の借用の根にも当てる。NC20 は PM10 を待たない）。
- 完了済み: [NC02](_completed/NC02-tensor-core-views-and-broadcasting.md)（`Tensor.Mut`・`broadcast_to`・`narrow_mut`）、[NC04](_completed/NC04-generic-index-class-and-slice.md)（`Index`/`IndexMut` と 3 引数のヘッダ）、[NC05](_completed/NC05-tensor-shape-indexing-and-masking.md)（マスクの `IndexMut`）、[A31](_completed/A31-in-place-writes-keep-stored-borrows.md)（書き込みが借用を残す規則）。

## 他チケットへ提供するインターフェース

- 字句 `+=`・`-=`・`*=`・`/=`・`%=` と `ExprKind::CompoundAssign`（GUIDE D-15）。
- 組み込みクラス `AddAssign`・`SubAssign`・`MulAssign`・`DivAssign`・`RemAssign`（メソッド `add_assign` など `:: ref Rhs -> ref mut Target -> unit`）と `IndexBorrowMut`（`index_borrow_mut :: ref Key -> ref mut Self -> Output`、std だけ）（GUIDE D-55）。
- std: `Tensor.add_scalar_in_place` など 5 個（`Tensor.add_in_place` などと双子は NC06 が提供する）、`Tensor`・`Tensor.Mut` の `AddAssign` 系 30 個、`IndexBorrowMut` 2 個、`Index<Tensor.Mut<'a>, [i64], 'a>`、`IndexMut<Tensor<'a>, [i64], 'a>`、NC16 の 1〜4 軸のキー K の `IndexMut<Tensor<'a>, K, 'a>`・`Index<Tensor.Mut<'a>, K, 'a>`。`Matrix`・`BigInt` などへの複合代入を後で足すチケットは、このクラスにインスタンスを足す（フィールドの排他借用は A39 の規則を使う）。
- kind `compound_assign_unsupported_target`（`E1005`）。EVL・OWN の新しい規則（番号は実装の時点の次の空き）。

## 実装手順

1. **変更**: なし。**内容**: 「着手条件」の一覧を控え、`tests/fixtures/` と `examples/` の IR を保存する（S2 の比較用）。**確認**: 一覧の記録。
2. **変更**: `src/lexer.rs`, `src/parser.rs`, `src/syntax.rs`, `src/syntax_codec.rs`, `src/frontend_cache.rs`, `src/formatter.rs`, `src/computation.rs`, `src/check.rs`（`tail_spans`）。**内容**: Phase 1 の字句と構文（§1）。型検査はまだ `E1005` で止めてよい。**確認**: T0、`cargo test --locked --test frontend --test formatter`（`x += 1` の解析・印字・往復、`**=` が今と同じ `E0002`、`x\n+= 1`）、`running N tests` の N。
3. **変更**: `src/check.rs`, `src/ownership.rs`, `src/polymorph.rs`, `src/diagnostic_kinds.rs`。**内容**: 組み込みの道（§3）、複合代入の印と右辺の守り（§2）、推論中・型変数・組み込みでない対象の `E1005`。**確認**: T0、`cargo test --locked --test compound_assignment`（受理・拒否・IR の一致・評価順）、`cargo test --locked --test diagnostic_kinds`。
4. **変更**: `src/hints.rs`, `src/explain/expected_expression.md`, `guide/card.md`, `guide/AGENTS.md.tmpl`, `tests/hints.rs`, `tests/diagnostic_baseline/compound-assignment/`, `tests/diagnostic_kinds.rs`, `spec/rules/EVL.md`, `spec/rules/OWN.md`, `tests/conformance/EVL/`, `tests/conformance/OWN/`, `tests/fixtures/compound_assignment/`, `tests/features.mjs`, 文書。**内容**: ヒントと baseline の書き換え（D10）、EVL・OWN の新しい規則（番号は `spec/rules/EVL.md`・`OWN.md` の実装の時点の次の空き）と適合性テスト、E2E、Phase 1 の文書。**確認**: T1（`cargo test --locked --test hints --test diagnostic_kinds --test guide` を含む）、`node tests/features.mjs target/debug/tsuzuri compound_assignment`、`node scripts/conformance.mjs target/debug/tsuzuri --rule <EVL の新しい番号>` と `--rule <OWN の新しい番号>`（番号は実装の時点の次の空き）、`node scripts/check-spec.mjs --require-markers`、`TSUZURI_BIN=target/debug/tsuzuri node scripts/check-guide.mjs`、`node scripts/check-docs.mjs <変更したページ>`、T2。Phase 1 の PR はここまで。
5. **変更**: `src/polymorph.rs`, `src/parser.rs`, `src/check.rs`。**内容**: `AddAssign` 系の登録・解決・脱糖（§4）。`dispatch_positions` に `AddAssign` 系の [0, 1] を足し、orphan 規則は A37 の `validate_orphan_instance` で判定する（D4）。フィールドの対象は A39 の規則で通るので、`src/ownership.rs` のフィールドの排他アクセスの拒否には触れない。std のインスタンスはまだ足さず、利用者のレコードのインスタンスで確かめる。**確認**: T0、`cargo test --locked --test compound_assignment`（利用者のインスタンス、`E1016`（`builtin_instance_override`・`orphan_instance`・`overlapping_instance`）・`E1005`・`E1043`、制約とメソッドの値の拒否、利用者の型のフィールドの対象（`AddAssign<Money, Money>` のインスタンスと、`Money` のフィールドを持つ `let mut` のレコードの `acct.balance += m`）が 2027 で受理され 2026 で `E1012` になる組）、`cargo test --locked --test mutable_slices --test types_ownership --test borrowed_records`（既存の拒否が変わらない）。
6. **変更**: `std/Tensor.tz`, `src/stdlib.rs`（一覧のテスト）, `tests/tensor.rs`, `tests/std_argument_order.rs`, `tests/std_examples.rs`, `tests/fixtures/effects_sweep/`。**内容**: スカラーの in-place（§5）と 30 個のインスタンス（右辺が窓・テンソルの本体は NC06 の `*_in_place_or_trap` を 1 回呼ぶ。同じ形の `Tensor` の右辺は §5 の直接のループで、A39 の規則で `ref mut target.data[0..]` を取る）。`Rhs` が `'a` の 10 個を A37 の `std_instances_with_open_dispatch_positions_are_listed` の一覧に足す（D4）。公開関数に `///` の例を付ける。**確認**: T0、`cargo test --locked --test tensor --test std_argument_order --test std_examples --test effects_alloc_conservative`、`node scripts/slot.mjs -- cargo test --locked --lib std_instances_with_open_dispatch_positions_are_listed` と `node scripts/slot.mjs -- cargo test --locked --lib opt_in_modules_are_reached_only_through_their_names`（どちらも `running 1 test` を確かめる。後者は A37 が `dispatch_positions` で直した置き場所のテストで、`AddAssign` 系の [0, 1] と 30 個のインスタンスを通すことを確かめる）、`target/debug/tsuzuri test --doc std`、`Alloc.since` の確保 0 回のテスト（`+=` の文の前後で測る。2027 の `model.w += g` を含む）、`rg -n 'exclusive_field_record' src/` が何も出さない（特権を作っていない: D2）、`honors_the_exact_specialization_limit`、連続の経路を `clang -x ir -O3 -S -emit-llvm` の後の IR で確かめる。
7. **変更**: `tests/features.mjs`, `tests/tensor-cases.mjs`, 文書。**内容**: Phase 2 の E2E と文書。**確認**: T1、`node tests/features.mjs target/debug/tsuzuri tensor` と `compound_assignment`、`node scripts/check-docs.mjs <変更したページ>`、T2。Phase 2 の PR はここまで。
8. **変更**: `src/polymorph.rs`, `src/parser.rs`, `src/check.rs`, `src/ownership.rs`。**内容**: `IndexBorrowMut` の登録（std だけ。キーの型が借用を持たないことの検査）、`ref mut t[…]` と `ref mut t[[…]]` の脱糖と借用（§6。評価順は A38 の規則）。**確認**: T0、`cargo test --locked --test index_class`（窓の型、土台の場所の規則（2027 のフィールドの経路の受理と 2026 の `E1014`）、キーが土台を読む・書き換える（A38 の `E1014`）、窓が生きている間の土台の使用、マスクと `Slice.Axis` のキー）。
9. **変更**: `std/Tensor.tz`。**内容**: `IndexBorrowMut` の 2 インスタンス、`Index<Mut, [i64], 'a>`、`IndexMut<Tensor, [i64], 'a>`、NC16 の 1〜4 軸のキー K の `IndexMut<Tensor<'a>, K, 'a>` と `Index<Tensor.Mut<'a>, K, 'a>`（`Index<Tensor<'a>, K, 'a>`・`IndexMut<Tensor.Mut<'a>, K, 'a>` は NC16 が足しているので足さない）。**確認**: T0、`cargo test --locked --test tensor --test index_class --test std_examples --test effects_alloc_conservative`。`std/Tensor.tz` を読み込むプログラムが `E1016` にならない。`m[1, 2] += 1.0`（`m: Tensor.Mut<f64>`）と `t[1, 2] += 1.0`（`t: Tensor<f64>`）が通る。
10. **変更**: `src/syntax.rs`（`ExprKind::Slice` の借用を `Option<bool>` にするか、借用のない 1 成分の節点を足す）、`src/syntax_codec.rs`、`src/frontend_cache.rs`（`FRONTEND_FORMAT` を 1 上げる）、`src/formatter.rs`、`src/parser.rs`、`src/check.rs`、`src/ownership.rs`。**内容**: `SliceMustBeBorrowed` の保留（§1）と、添字の対象への複合代入（§7。生成の束縛 `$k`・`$e` の守りを含む）。**確認**: T0、`cargo test --locked --test compound_assignment --test index_class --test frontend`（順序の出力、重なりの `E1014` と help、キーか右辺が土台に代入する形の `E1014`、配列のスライスの `E0002` が今と同じ、`f t[1..3] += u` が保留した `E0002`）、`syntax_codec` の全タグの見本、`tsuzuri fmt` で `t[1..3] += u` が冪等。
11. **変更**: `tests/features.mjs`, `tests/tensor-cases.mjs`, `tests/fixtures/compound_assignment_2028/`（新規。`Main.tz` と `edition = "2028"` の `Tsuzuri.toml`）, 文書。**内容**: Phase 3 の E2E と文書。edition 2028 の構文を使う Phase 3 のケースは、Phase 1 の `tests/fixtures/compound_assignment/`（全 edition）にも `tests/tensor-cases.mjs`（`tensor` の fixture はマニフェストがなく `LATEST` で読む）にも入れず、`edition = "2028"` の `Tsuzuri.toml` を持つ別の fixture（`tests/fixtures/compound_assignment_2028/` と suite `compound_assignment_2028`）に置く。**確認**: T1、`node tests/features.mjs target/debug/tsuzuri tensor`・`compound_assignment`・`compound_assignment_2028`・`mutable_slices`（suite は 1 回に 1 つ）、`node scripts/check-docs.mjs <変更したページ>`、`node scripts/gen-docs.mjs --check`、T2。

## テスト計画

- **Rust（`tests/compound_assignment.rs`、新規）**:
  - 受理: `i8`〜`i128u` の 5 演算子、`f32`・`f64` の `+=`・`-=`・`*=`・`/=`、SIMD の数値ベクトルの `+=`・`-=`・`*=`（浮動小数点のベクトルは `/=` も）、`string`・`utf8string` の `+=`、`deref r += e`、2027 のフィールドと要素（2026 は `E1012`）、`if c then x += 1`、`const def` の本体の `let mut` への複合代入（`=` と同じく許す）。
  - IR の一致: ローカル・`deref r`・ローカルの添字の `xs[i]`・フィールドで、`p op= e` と `p = p op e` の `--emit llvm` が位置の情報を除いて一致する。2 回のビルドで同じ IR。
  - 評価順: 例の `deref (pick (ref mut x) 1) += say 2` の出力、`xs[next (ref mut k)] += 1` で `next` が 1 回だけ呼ばれる。
  - 右辺の守り: `x += bump (ref mut x)`、`x += { x = 5; 1 }`、`xs[2] += { xs = [9]; 5 }`、非 Copy の対象の move がすべて `E1014`（`borrow_conflict`、help と関連する位置）。`x += x`（`i64`）は受理されて 2 倍になる。
  - 型の誤りが書き下した形と同じ: `f64` の `%=`、`string` の `-=`、型の合わない右辺。
  - `@checked (x += 1)` の溢れが `OverflowException` で、`x` が書き換わらない。閉包の中と計算式の継続の中の `+=` が `E1014`。
  - 推論中・型変数・組み込みでない対象（Phase 1）の `E1005`（`compound_assign_unsupported_target`）。`bool` の `b += true` と、推論中から `string` に決まる `s -= "x"`（`let mut s = Default.default ()` の後）は、`p = p op e` と同じ `E1005`（`no instance for Add<bool>`・`no instance for Sub<string>`）で、この kind を出さない。
  - 2026 のフィールドの対象 `b.n += 1`（`i64`）が `b.n = b.n + 1` と同じ `E1012` で、help `record fields can be updated with '+=' from edition 2027` が付く。同じ形の `b.n = b.n + 1` の診断は変わらない（S2）。
  - 負のリテラルの添字（NC17 の Phase 2 の後。先に入った方が足す）: 組み込みの配列の `xs[-1] += 1` は `W1014`（kind `negative_literal_index`。`index_operand` を通る）で、実行するとトラップする。
  - Phase 2（利用者のインスタンス）: 利用者のレコードの `AddAssign`（2027 のフィールドの代入で更新する本体）、`E1005`（インスタンスなし、`x += ref y`。A40 の前の診断。A40 が先に入っていれば `x += ref y` は受理する。後に入る方がこのテストを合わせる）、`E1043`、制約（`E0002`）とメソッドの値（`E1002`）の拒否、`View` の対象の `E1005`。
  - Phase 2（登録の規則。D4）: `orphan_instance`（`E1016`）は、利用者の `instance AddAssign<Tensor<f64>, f64>` と `instance AddAssign<Tensor.Mut<f64>, Tensor.View<f64>>`。受理は、`instance AddAssign<Money, Money>` と `instance AddAssign<Money, Tensor<f64>>`（`Money` を宣言したモジュールと別のモジュールに書いても受理）。`overlapping_instance`（`E1016`）は、std の `AddAssign<Tensor<'a>, 'a>` と重なる利用者の `instance AddAssign<Tensor<Money>, Money>` で、位置は利用者のインスタンス。`builtin_instance_override`（`E1016`）は、`instance AddAssign<i64, i64>` と `instance AddAssign<(i64 * i64), Money>`。利用者の `extern type` の `Target` は受理。
  - Phase 2（場所の規則。D1・D2）: `Tensor<f64>` のフィールドを持つ `let mut` のレコードの `model.w += g` は、edition 2027 で受理され、`+=` の文の要素数に比例する確保が 0 回。同じソースを edition 2026 で読むと `E1012`（kind `assign_to_immutable_place`、help `record fields can be updated with '+=' from edition 2027`）。この 2 つを組で置く。`model.w += model.g` は受理（別のフィールドの loan）。`model.w += model.w` は `E1014`（`borrow_conflict`）。`r: ref mut Model` の `r.w += g` は 2027 で受理。`ts[i] += y`（`ts: [Tensor<f64>]`）は `E1014`（`mutate_immutable_value`）で、help `'+=' on Tensor<f64> borrows its target exclusively, which an element cannot be; keep the value in a 'let mut' local or a record field`。`m: ref mut Tensor<f64>` の `deref m += 1.0` は再借用で確保なし。`Tensor` の `x += x` は `E1014`。共有参照を経由する対象は `E1014`（`mutate_through_shared_reference`）。
  - Phase 2（確保の測り方）: `Alloc.since` は `+=` の文の直前と直後に置き、関数の戻り値の複製を含めない。`+=` の後で対象の局所を返すときの複製は、対象の型が Copy のときだけ起きる（PM10 の Phase 2 までの今の制限。§5）。NC20 の確保 0 回のテストの対象にしない。`Tensor` の `t += u` の後に `t` を返す関数が、`+=` を書かない同じ関数と同じ確保になる（move）ことは、このチケットのテストで確かめる。Copy の対象（`record Acc { xs: [f64] }` の利用者の `AddAssign`）を `+=` の後に返す関数が複製しないテストは、PM10 が先に done ならこのチケットが足し（受け入れ条件）、そうでなければ PM10 の手順 9 が足す。
- **Rust（`tests/tensor.rs`）**: `+=` などと NC20 のスカラーの in-place の値（同じ形・行と列のブロードキャスト・0 次元のスカラー・空のテンソル・負のストライドの窓・非連続の窓）、`x += y` と `Tensor.add_in_place` の値がビット単位で同じ（同じ要素のループ）、整数の折り返し、整数のゼロ除算のトラップ、`AddAssign` の形の不一致のトラップ（何も書かない）、`Alloc.since` で同じ形の `Tensor` どうしとスカラーの確保が 0 回・ブロードキャストが要素数に依らない回数。
- **Rust（`tests/index_class.rs`）**: `ref mut t[[Slice.Sel…]]` の型が `Tensor.Mut`、利用者の `IndexBorrowMut` の `E1016`、一時値の土台の `E1013`、共有参照・`let` の束縛・要素の経路の `E1014`、`Drop.drop` の根の `E1012`、窓が生きている間の土台の読み出し・move の `E1014`、キーが土台を読むのは受理・代入・move・排他借用するのは `E1014`（`borrow_conflict`。A38）、フィールドの土台（D3。`Tensor<f64>` のフィールド `w`・`b` を持つ `let mut model` の `ref mut model.w[[Slice.Sel…]]` は 2027 で `Tensor.Mut` の窓になり、窓が生きている間 `model` は使えない（`E1014`）が、隣のフィールド `model.b` は読める。同じソースを 2026 で読むと `E1014`（`mutate_immutable_value`））、`Tensor.Mut` の窓が死んだ後は土台の `Mut` をまた使える、マスクの `ref mut` の `E1003`（`index_ref_on_value`）と `[Slice.Axis]` のキーの `ref mut` の `E1042`（`index_view_not_writable`。§6 の help）、マスクを対象にした `t[mask] += 1.0` の `E1005`（`no_instance`）、連鎖した書き込み窓 `ref mut t[1..3, ..][0, ..]` の `E1013`（`borrow_of_temporary`。位置は内側の角括弧の式、help は §6）と `t[1..3, ..][0, ..] += u` の `E0002`（`slice_must_be_borrowed`。内側の角括弧）、長い形の連鎖 `t[[A]][[B]] += u` と `ref mut t[[A]][[B]]` の `E1013`（help は利用者の角括弧の綴りを引き、複合代入では `w[[B]] += …` を案内する。`E1042` でない）、組み込みの 6 型の添字を土台にした `ts[0][[1, 2]] += 1.0` の `E1014`（`mutate_immutable_value`）、`[Slice.Axis]` のキーの長い形の連鎖 `t[[Slice.range 1 3]][[B]] += u` の `E1042`（`index_view_not_writable`）、値を返す利用者の `Index` を内側にした `g[0][1] += 5` の help のない `E1013`、`Tensor<Complex<f64>>` の要素の `t[[0]] += c` の `E1005`（`compound_assign_unsupported_target`）、`Tensor<f64>` の要素の `t[[0]] %= 2.0` の `E1005`（`no instance for Rem<f64>`）、推論中の要素（`let mut t = Tensor.zeros [4]` の直後の `t[[0]] += 1.5`）の受理、束縛した窓の `let mut w = ref mut t[1..3, ..]` の後の `ref mut w[0, ..]` と `w[0, ..] += u` の受理、刻み 0 のトラップ、要素の対象の `t[[1, 2]] += 1.0` と `deref m` の要素、重なり `a[1..] += ref a[..^1]` の `E1014`（`Tensor` の土台と `Tensor.Mut` の土台の両方）とスナップショットでの受理、`t[1..3, ..] += { t = Tensor.zeros [1]; u }`（`u: Tensor<f64>`。右辺は自動で貸す。窓のキーの右辺が土台に代入する形）が `E1014`（`borrow_conflict`。右辺の中の `t` への代入）、`t[1..3, ..] += { t = Tensor.zeros [1]; ref u }` は `E1005`（`no_instance`、help `the right side is lent automatically`。A40 の前の診断。A40 が先に入っていれば `x += ref y` は受理し、`{ t = …; ref u }` は右辺の中の `t` への代入で `E1014`（`borrow_conflict`）。後に入る方がこの行とテストを合わせる）。キーが土台に代入する添字の対象（§7 の `$k` の守り）: 窓のキーの `t[{ t = Tensor.zeros [4, 4]; 1 }, ..] += u` と要素のキーの `t[{ t = Tensor.zeros [4, 4]; 1 }, 2] += 1.0` が、どちらも `E1014`（`borrow_conflict`。位置はキーの中の代入）。要素のキーの右辺が土台に代入する形（§7 の `$e` の守り）: `t[1, 2] += { t = Tensor.zeros [4, 4]; 1.0 }` が `E1014`（`borrow_conflict`。位置は右辺の中の代入）。`t[^0, 0] += 1.0` は NC18 の Phase 1 では `E1005`（`from_end_element_read_unsupported`）、`t[^0, ..] += 1.0` は窓。
- **既存テストへの影響（S2 の例外）**: `tests/hints.rs`（ヒント `compound-assignment` の `bad`・`good`・`help` を D10 の形へ。件数 40 は変えない）、`tests/diagnostic_baseline/compound-assignment/Main.tz` を `x **= 1` へ書き換えて `baseline.jsonl` の行を更新し、`tests/diagnostic_kinds.rs` の `baseline` の doc コメントに G25 の `question-mark` と同じく理由を書く（件数 48 は変えない）。ほかの期待値は変えない。
- **E2E（`tests/features.mjs` の suite `compound_assignment`、`tests/fixtures/compound_assignment/Main.tz`）**: 組み込み型の値、評価順の出力、`Tensor` の in-place と書き込み窓の値を native/WASM × `-O0`/`-O3` で照合し、解放追跡（`live == 0`）と WASM の import なしを確かめる。`tests/tensor-cases.mjs` に書き込み窓のケース（添字の写像で表す。C11 Phase 2 の形）を足す。edition 2028 の構文を使う Phase 3 のケース（`t[1, ..] *= 2.0`、`a[1..] += …`）は、Phase 1 の `tests/fixtures/compound_assignment/`（全 edition）にも `tests/tensor-cases.mjs`（`tensor` の fixture はマニフェストがなく `LATEST` で読む）にも入れず、`edition = "2028"` の `Tsuzuri.toml` を持つ別の fixture（`tests/fixtures/compound_assignment_2028/` と suite `compound_assignment_2028`）に置く（Phase 1 の fixture を 2028 にすると Phase 1 が NC17 に依存し、D8 に反する）。
- **適合性**: `tests/conformance/EVL/EVL-0xx-…tz`（例の出力 `place 1`、`2`、`3`）、`tests/conformance/OWN/OWN-0xx-…tz`（`E1014`）。`0xx` は実装の時点の次の空きの番号で、規則と同じ番号にする。
- **性能**: 速度の閾値は置かない。`Alloc.since` の回数と、連続の経路の自動ベクトル化（`clang -x ir -O3` の後の IR か逆アセンブル）を確かめて完了報告に書く。速さを主張する場合だけ `docs/benchmarks.md` に計測を記録する。

## ドキュメント

| ページ | 更新 |
| --- | --- |
| `_tsuzuri/language-reference/values-and-functions/op-and-expressions.md` | 優先順位の表の後の NOTE（`=` と複合代入はどの二項演算子よりも弱い）、「評価順序」の表に `p op= e` の行（場所の部分 → 古い値 → 右辺 → 書く。場所の部分の中は A38 が書く添字の行のとおり土台、キーの成分を左から右。`=` との違い）、複合代入の節（組み込み型は `p = p op e` と同じ、右辺の守り、`Tensor` の in-place、フィールドの対象は 2027 から、確保の有無と、対象を後で返すときの複製（PM10 までの制限）） |
| `_tsuzuri/language-reference/values-and-functions/values.md` | 「可変な束縛と再代入 (let mut)」に複合代入 |
| `_tsuzuri/language-reference/values-and-functions/tokens.md` | 記号の表に `+=`・`-=`・`*=`・`/=`・`%=` |
| `_tsuzuri/language-reference/types-and-type-inference/type-classes.md` | 組み込み型クラスの表に `AddAssign` 系と `IndexBorrowMut`、個数の文を `BUILTIN_CLASSES` と合わせる（HEAD では文が 31 個、`BUILTIN_CLASSES` は 36 個。数え方を確かめて直す） |
| `_tsuzuri/language-reference/built-in-types-and-modules/tensor.md` | 「この記事のポイント」（`+=`・`ref mut t[…]`）、in-place の関数の API リファレンスと「引数の順序」、「借用規則と診断」の表（書き込み窓・重なり・フィールドの対象は 2027 から）、「トラップ」の表、「できないこと」（要素ごとの算術の注記を NC06・NC19・NC20 の実際に合わせる）。このチケット群の PR が足す既知の制限の注記（PM10）の「所有するテンソルを複製せずに書き換える方法は、今はない」を、`+=` と `ref mut t[…]` で書けるという今の振る舞いに直す（PM10 へのリンクは、所有を移す関数の複製の注記として残す） |
| `_tsuzuri/language-reference/built-in-types-and-modules/slice.md` | `Slice.Sel` の書き込み窓（NC18 が書いた節へ）。`Slice.Axis` は書き込み窓に使えないこと |
| `_tsuzuri/language-reference/ownership-and-memory/borrowing.md` | 書き込み窓の loan（土台の場所の排他 loan）、複合代入の右辺の守り、フィールドの対象（2027 から。A39 が書いた節へ） |
| `_tsuzuri/language-reference/compiler/diagnostics.md` | 239 行付近のヒントの一覧の `+=` を D10 の形に、生成表（`node scripts/gen-docs.mjs`）に新しい kind |
| `_tsuzuri/language-reference/built-in-types-and-modules/numpy-migration.md` | NC07 が作っていれば、`a += b`（同じ）、`a[1:] += a[:-1]`（NumPy は暗黙に複製、Tsuzuri は `E1014`）、`np.add.at`（未対応）を追記 |
| `docs/language.md` | 1960〜1961 行付近（代入）と 1837〜1840 行付近（2027 の代入）に複合代入、「式と評価順序」（3010 行付近）に EVL の新しい規則（番号は実装の時点のもの） |
| `docs/architecture.md` | `Tensor` の節（1164 行付近）に書き込み窓と in-place、型クラスの組み込みの多引数ヘッダの説明に `AddAssign` 系・`IndexBorrowMut`、std の本体が A39 の規則でフィールドを排他借用することと、`Tensor` の `data.length` の不変条件が排他スライスの固定の長さで保たれること |
| `guide/card.md`・`guide/AGENTS.md.tmpl`（AI 向け。英語: D-56） | `guide/card.md` 82 行の「`x += 1` is E0002」と 42 行付近の `x = x + 1`、`guide/AGENTS.md.tmpl` 33 行の「There is no … `x += 1`」を Phase 1 の意味に直す（`**=` などが無いことは残す）。`TSUZURI_BIN=target/debug/tsuzuri node scripts/check-guide.mjs` で例と文字数の上限を確かめる |
| `README.md` | 書いてあることが誤りにならないので必須ではない（「状態管理」の行に `+=` を足すかは README の規則に従う） |

文書の例は、範囲の例が edition 2028 の意味であることを書き、`run=` の例は `node scripts/check-docs.mjs` で native の `-O0`／`-O3` を通す。

## 受け入れ条件

- [ ] `+=`・`-=`・`*=`・`/=`・`%=` が全 edition で使え、組み込み型では `p = p op e` と同じ値で、単純な場所の IR が位置の情報を除いて一致する。
- [ ] 評価順が EVL の新しい規則（場所の部分 → 古い値 → 右辺 → 書く。場所の部分の中は A38 の添字の順）のとおりで、右辺が対象を書き換える・move する・排他借用するプログラムがどの型でも `E1014`。規則の番号は実装の時点の次の空きで、`node scripts/check-spec.mjs --require-markers` が通る。
- [ ] 組み込みでない型の `+=` は、`AddAssign` のインスタンスがあるときだけ通り、ないときは `E1005`（確保する書き方へ黙って落ちない）。
- [ ] `AddAssign` 系の登録が §4 の規則（`builtin_instance_override`・`E1041`・`orphan_instance`・`overlapping_instance` の順。orphan 規則は A37 の 1 つの規則で、`dispatch_positions` に [0, 1]）のとおりで、`Rhs` が `'a` の std の 10 個が `std_instances_with_open_dispatch_positions_are_listed` の一覧にある。
- [ ] 同じ形の `Tensor` どうしの `x += y` とスカラーの `x += 1.0` が、`+=` の文の前後の `Alloc.since` で確保 0 回。ブロードキャストの確保が要素数に依らない。連続の経路が `clang -O3` の後にベクトル化されているかを IR で確かめ、結果（実行時の重なりの検査が残るかを含む）を完了報告に書く。
- [ ] `model.w += g` が edition 2027 で受理されて文の確保 0 回、edition 2026 で `E1012`。`ts[i] += y` が新しい help の `E1014`。
- [ ] `t += u` の後に `t` を返す関数と、`model.w += g` の後に `model` を返す関数（`Tensor` は Copy でないので move）が、`+=` を書かない同じ関数と同じ確保になるテストを足した。Phase 2 の時点で [PM10](../_perfs/_completed/PM10-std-consume-copies.md) が done なら、Copy の対象の `acc += d` の後に `acc` を返す関数が要素を複製しないテストを `tests/fixtures/last_use`（`node tests/features.mjs target/debug/tsuzuri last_use`）に足した。done でなければ、PM10 の手順 9 が足す。
- [ ] `ref mut t[…]` が `Tensor.Mut` を返し、窓が生きている間の土台の使用、マスク・位置の一覧の窓、一時値の土台、要素の経路、2026 のフィールドの経路が規定の診断になる。2027 のフィールドの経路の土台は窓になる。
- [ ] `x += x`（`Tensor`）と `a[1..] += ref a[..^1]` が `E1014` で、help が明示のスナップショットを案内する。
- [ ] 複合代入を書かないプログラムの診断と IR が変わらない（S2）。native/WASM × `-O0`/`-O3` が一致し、`live == 0`、WASM の import なし、IR が決定的。
- [ ] std の本体は A39 の一般の規則だけでフィールドを排他借用する。std だけの特権・許可表・インスタンスの本体の判定がない（`rg -n 'exclusive_field_record' src/` が何も出さない）。
- [ ] 「ドキュメント」の表のページを更新し、`node scripts/check-docs.mjs`・`node scripts/gen-docs.mjs --check`・`node scripts/check-spec.mjs --require-markers` が通る。
- [ ] Phase 2 は A39・NC19 が done になってから、Phase 3 は A37・A38・A39・NC16・NC18 が done になってから着手した（D1〜D4 は承認済み。D-66 の委任で決定、2026-11-25）。

## 落とし穴

- **`=` と評価順が違う。** `=` は右辺が先（EVL-012）、複合代入は場所の部分が先（NC20 の EVL の規則）。書き下した形への脱糖で、単純でない場所の部分を 2 回評価しない（一時の束縛に 1 回）。逆に、単純な場所で一時の束縛を作ると IR が一致しなくなる（S1）。
- **`IndexMut` の `=` の順は A38 で変わる。** HEAD の `IndexMut` の `m[k] = v` はキーが先だが（R3）、A38 で右辺が先になり、`xs[i] = v` とそろう。複合代入はどの型でも「土台の部分 → キー → 右辺 → 書く」。NC20 は `=` の順を変えない（D13 (b)）。
- **守りは右辺の間だけ。** 守りを書き込みまで残すと、自分の書き込みと衝突して `E1014` になる。守りは読み出しを妨げない（`x += x` の `i64` は受理）。
- **`SliceMustBeBorrowed` の保留は取り消し忘れと出し忘れの両方が危ない。** 左辺そのものかどうかを位置で比べ、`a + t[1..3] += u` のように左辺が別の式になったら保留した誤りを出す。`=` では保留しない（D9）。
- **`Tensor.update` を `+=` に使わない。** 要素が Copy の型で中身を 2 回複製する（R4）。値を消費して返す形のメソッドも、本体が Copy の局所（取り出した `data` の配列）を借りてから返すと、A17 の X1 のために同じ複製を起こす（D1）。直すのは PM10（D13 (a)）。
- **文の確保と関数の確保を分けて測る。** `+=` の文は確保 0 回。`+=` の後で対象の局所（フィールドの対象なら持ち主）を返すか move するとき、その型が Copy なら A17 の X1 のために複製する。`Tensor` とそれをフィールドに持つレコードは Copy でないので move で、複製しない（Copy の対象と取り違えて、`Tensor` の複製を前提にしたテストを書かない）。PM10 の Phase 2 までの今の制限なので、NC20 で `src/last_use.rs` を変えない（S4）。
- **`Tensor.view` は確保する。** 同じ形の `Tensor` の右辺は、本体で `view` を作らずにフィールドを直接走査しないと確保 0 回にならない。
- **フィールドの排他借用は A39 の一般の規則だけで行う。** std だけの特権や許可表を作らない（D2・S3）。A39 は edition 2027 以降でだけ働くので、2026 のフィールドの対象は `E1012`（複合代入）か `E1014`（書き込み窓）のままにする。`Matrix` などへ `AddAssign` を足す後続のチケットも、型ごとの不変条件をモジュールの中のコードで守る。
- **`BUILTIN_CLASSES` を増やすと予約名が増える。** `AddAssign` などの名前のレコードの宣言が `E1001` になるテストと、`type-classes.md` の個数を忘れない（NC04 の落とし穴と同じ）。
- **`syntax_codec.rs` と `FRONTEND_FORMAT`。** AST の変種を足したら 1 上げる（GUIDE-LITE §7）。上げ忘れると古いキャッシュを読む。
- **ヒントと baseline。** `x += 1` が正しいプログラムになるので、`tests/hints.rs` と `baseline_diagnostics_are_unchanged` が赤になる。D10 の形に直し、件数は変えない。

## 対象外

- 窓への `=` の代入（NumPy の `a[1:3, :] = b`）。代入用のクラスの設計が要る（後続の候補）。`t[k] = v` は今の `IndexMut` のまま。
- ファンシーインデキシング（位置の一覧）とマスクを対象にした `+=`（重複した位置の扱いが要る。NumPy の `np.add.at` に当たる逐次の関数も HEAD にない）。マスクへの代入 `m[mask] = v` は今のまま。
- `**=`・ビット演算・シフト・論理演算の複合代入。
- `Matrix`・`MatrixView`・`BigInt` など `Tensor` 以外の std の型の `AddAssign` と `IndexBorrowMut`。
- 利用者の `IndexBorrowMut` のインスタンス。`AddAssign` を制約やメソッドの値に使うこと。
- 重なる更新の暗黙のスナップショット、読んでから書く規則。
- std の所有を移す関数（`Tensor.update` など 9 つ）の複製と、A17 の X1 の精密化（PM10。D13 (a)）。
- 連鎖した書き込み窓（`ref mut t[1..3, ..][0, ..]`）の受理（D14。NC20 では `E1013` で、内側の窓を束縛してから書く）。
- 組み込みでない要素型の要素の複合代入（`Tensor<Complex<f64>>` の `t[[0]] += c`。要素の更新のクラスの設計が要る。§7、D11）。
- `=` の評価順の変更（`IndexMut` の `=` の順をそろえるのは A38。D13 (b)）。
- レコードのフィールドの経路の排他借用そのもの（A39）と、edition 2026 でのフィールドの対象の受理。

## 決定事項

- **D1（承認済み（D-66 の委任で決定、2026-11-25））**: 組み込みクラス `AddAssign<Target, Rhs>`・`SubAssign`・`MulAssign`・`DivAssign`・`RemAssign` を足す（`BUILTIN_CLASSES` 36 → 41、GUIDE D-55）。メソッドは `add_assign`・`sub_assign`・`mul_assign`・`div_assign`・`rem_assign` で、型は `ref Rhs -> ref mut Target -> unit`。対象は最後に置く（D-59。`IndexMut` と同じ）。2 引数のヘッダは組み込みのクラスに限る。制約には書けず（`E0002`）、メソッドの値にもならない（`E1002`）。場所の部分が単純なとき、`p op= e` は `Call(メソッド, [BorrowOperand(e), Borrow(p, true)])` に下げる。`p` が `deref r` なら、`Borrow` を作らずに `r` を渡す（再借用。`IndexMut` と同じ）。場所の部分が単純でないときは、§3 と同じく部分の値を一時の束縛に 1 回だけ評価してから、同じ呼び出しにする。評価順は §2 の「場所の部分 → 右辺 → 書く」で、場所の部分の中は A38 の添字の評価順（土台、キーの成分を左から右）に従う。所有権の検査は、`e` を評価する間だけ `p` に共有の守りを置き（§2 の OWN の規則）、その後で `p` を排他借用する。右辺は自動で貸すので、`x += y` は `y` を move しない。この道の対象は `=` の左辺の規則に従う。`let mut` のローカル、`ref mut` の参照先、それらを根にしたフィールドの経路（2027 から。A39 の排他借用で通る）を書ける。組み込みの配列・リスト・`Vec`・固定長配列の要素を経由する経路（`ts[i] += y`）は、要素単位の排他借用がないので `E1014`（`mutate_immutable_value`）で、help は §4。組み込みでない型の添字の対象（`t[1, 2] += 1.0`、`t[1..3, ..] += u`）はこの規則の外で、§7 の道が扱う。`+=` の文そのものは要素に比例する確保をしない。ただし、対象の型が Copy なら（配列をフィールドに持つ利用者のレコードなど）、対象の局所を後で返すと、A17 の X1（借用の根になる局所は関数全体で移動しない）により返すときに複製する（`Tensor` は Copy でないので move する）。これは `IndexMut` の `t[k] = v` と同じ今の制限で、PM10 の Phase 2 が直す。採らない案: 値を消費して返す形（`ref Rhs -> own Target -> Target`。D-66 の設計の案（反証の修正を含む）で、`x = AddAssign.add_assign (ref y) x` に下げて `Tensor.update` で確保 0 回にする案）、右辺を値で受ける Rust の順の形（`ref mut Target -> own Rhs -> unit`）、`Add` への暗黙の代替。
  理由: (a) HEAD の `Tensor.update` は要素が Copy の型で中身を 2 回複製する（R4: 1,000 要素で 7 回・16,040 バイト）。(b) 値を消費して返す本体は、Copy の局所（`Tensor.update` の本体が取り出す `data` の配列など）を借りてから返すと、A17 の X1（`src/last_use.rs` 55〜58 行）で複製する（`out` を `ref mut` で書いて返す関数が 2 回・16,000 バイト。借用しなければ 1 回・8,000 バイト）。A39 で `Tensor` の持ち主の `data` を借りる本体なら、持ち主は Copy でないので move で、複製しない。対象の型が Copy なら、利用者の関数のどこかで対象を借りていれば、`x = add_assign (ref y) x` の受け渡しも複製になる（`Tensor` は Copy でないので move）。`ref mut` の形は、文の中ではこのどちらにも依存しない。(c) 設計の案が `ref mut Target` を退けた理由（所有する `Tensor` のフィールドを排他借用できない）は、A39 のフィールドの排他借用で解ける。A39 は 2027 以降で働き、std は 2027 で読むので、std の本体にも利用者の `model.w += g` にも使える。(d) `ref mut` の形は `deref m += y` にも使えるが、消費する形は参照の先から move できない（非 Copy の型は `deref r = f (deref r)` が `E1012`、Copy の型は全体の複製）。(e) 重なりを既存の検査で拒否できる。右辺の `BorrowOperand` と対象の `Borrow(p, true)` が同じ呼び出しに並ぶので、`x += x` と、右辺の窓が対象を借りる形は `E1014` になる（前例は `f[f] = 1` と `Array.blit 0 1 3 xs xs`）。(f) 主要な言語と同じ形である: Rust の `AddAssign::add_assign(&mut self, rhs)`、Swift の `inout` の `+=`、C# 14 の利用者定義の複合代入（C# は外部の事実で、リポジトリでは確かめていない）。C# は `+=` がないときに `+` へ落ちるが、Tsuzuri は確保する書き方へ黙って落とさない（D-66）。(g) PR01 は `ref mut` のスライスの引数に属性を付けない（`src/llvm_facts.rs` 57〜58 行。`noalias` がない）。連続のカーネルで LLVM が実行時の重なりの検査を省けるかは `clang -O3` の後の IR で確かめ、確かめるまでベクトル化を主張しない。(h) Copy の対象を返すときの複製は X1 の問題で、メソッドの形を変えても消えないので、PM10 の Phase 2 で直す。(i) 右辺を `ref Rhs` で受けるのは NC03 の D1-A（算術の演算子は被演算子を消費しない）と同じで、右辺を値で受ける形は D-59（対象は最後）にも反する。
- **D2（承認済み（D-66 の委任で決定、2026-11-25））**: std だけの特権は作らない。許可表 `exclusive_field_record` も、インスタンスの本体かどうかの判定も置かない。代わりに新しいチケット [A39](_completed/A39-field-exclusive-borrow.md) で、レコードのフィールドの経路の排他借用を一般の規則として入れる。対象は、`let mut` の持ち主か `ref mut` の参照先を根とし、region の印（A13）の後の段がすべてレコードのフィールドである場所（`b.f`・`b.f.g`・`r.f`）。可変かどうかの判定は変えない。loan はフィールドの場所に置くので、別のフィールドとは重ならず、持ち主の全体の読み出し・move・代入とは重なる。要素・共用体のペイロード・リストの tail・タプルの成分を含む経路、共有参照を経由する経路、`Drop.drop` の中の持ち主のフィールドは今のまま拒否する。A39 は edition 2027 以降のソースでだけ働く。2026 はレコードのフィールドを不変に保つ（`b.f = v` が `E1012`）ので、2026 で `ref mut b.f` を受理すると、代入できないフィールドを借用を通して書けてしまうためである。A39 は 2027 の A18 の射影の代入の借用の形で、宣言済みの 2027 の差の中で受理を増やすだけなので、新しい edition の差は要らない。2026 は今の `E1014` のまま。std は `Edition::LATEST`（今は 2027）で読むので、`std/Tensor.tz` の本体は A39 の規則で `ref mut target.data[0..]` を取れる。結果として、フィールドの対象（`model.w += g`）とフィールドの経路の書き込み窓の土台は 2027 から通り、`let mut` のローカルの対象（`t += u`、`ref mut t[1..3, ..]`）は全 edition で通る。NC20 の Phase 2・3 は A39 に依存する。A39 で健全さを示せない構成が見つかったら、特権には戻さず、loan を持ち主の全体に置く粗い規則へ縮める（A39 の側）。採らない案: std の不透明なレコードだけの特権（起票時の案）、std だけの組み込み関数、ランタイムのカーネル、射影の書き込みのループだけ（書き込み窓を作れない）。
  理由: (1) 健全さの義務は、特権でも一般の規則でも変わらない。特権は誤りに触れるプログラムの数を減らすだけで、正しさの根拠を増やさない。一般の規則なら、適合性テストとすべてのプログラムが検査する。(2) 特権の境界は何も守らない。2027 では、std のモジュールのどの関数も `self.data[i] = v` で `Tensor` の中身を書き換えられ、不変条件を破る `Tensor { … }` も作れる。(3) 利用者の型に性能の崖と書きにくさを作らない。特権のままでは `record Model { w: Tensor<f64>, b: Tensor<f64> }` の `model.w += g` は `E1014` で、迂回は値を取り出して作り直すことになる。`Tensor<f64>` は Copy でないので、`let mut w = model.w` は move で複製しないが、その後の `{ model with w = w }` は部分 move の `E1012` になる（HEAD で確認）。`model = Model { w: w, b: model.b }` とすべてのフィールドを並べるか、`match` で分解する（`Tensor.clone` は全要素を複製する）。Copy の配列のフィールド（`record B { xs: [f64] }`）では、`let mut xs = b.xs` が全要素を複製する。(4) 排他スライスは長さが固定なので、`Tensor` の `data.length` の不変条件も保たれる。(5) 受理していたプログラムの意味は変わらない。HEAD では明示の `ref mut b.data[0..]` などがすべて `E1014` で、複製した一時値へ黙って落ちる道はない。
- **D3（承認済み（D-66 の委任で決定、2026-11-25））**: 組み込みクラス `IndexBorrowMut<Self, Key, Output>` を足す（`BUILTIN_CLASSES` 41 → 42、GUIDE D-55）。メソッドは `index_borrow_mut :: ref Key -> ref mut Self -> Output`。インスタンスは std のモジュールだけが定義でき、利用者のモジュールのインスタンスは `E1016`（`builtin_instance_override`）。キーの型が借用を持たないことを登録で検査するので、結果が持つ loan は土台の場所の排他 loan（と、土台の値が持つ loan）だけになる。`ref mut t[…]` と `ref mut t[[…]]` は、土台を `Borrow(土台, true)`（`ref mut` の参照ならその再借用）、キーを `BorrowOperand` にした呼び出しに下げる。評価順と所有権の検査は A38 の添字の規則に従う: 土台の場所の部分を先に評価し、土台の場所に共有の守りを置いてキーの成分を左から右へ評価し、その後で排他 loan を取る（`exclusive_slice` と同じ順）。キーは土台を読めるが、変えられない。std のキーは `[Slice.Sel]` だけにする。窓になれる成分は `Item`・刻みが 0 でない `Span`・`All`・`Rest`・`NewAxis`（設計の案の列挙に、反証が求めた `All`・`Rest` を足した。どれも単射を保つ）。`Slice.Axis`（範囲外を丸める）・マスク・位置の一覧は書き込み窓にならない。std のインスタンスは `IndexBorrowMut<Tensor<'a>, [Slice.Sel], Tensor.Mut<'a>>`（A39 の規則で `self.data` を排他借用する。特権は要らない）と `IndexBorrowMut<Tensor.Mut<'a>, [Slice.Sel], Tensor.Mut<'a>>`（`narrow_mut` と同じ再借用）の 2 つ。土台には、`let mut` の持ち主か `ref mut` の参照先からのフィールドの経路も書ける（2027 から。A39）。要素の経路と一時値（`E1013`）は今のまま拒否する。std だけに限るのは、公開の約束を小さく始めるための API の範囲の判断で、信頼の問題ではない（本体は普通の関数として借用検査される。利用者に開くのは受理を増やす拡張なので、後からできる）。採らない案: 利用者に今すぐ開く、名前で認識する関数（`Tensor.slice_mut`）への脱糖、`Slice.Axis` のキーも許す（書き込み先が黙って縮む）、`IndexMut` に窓を返させる（要素単位の排他参照がなく、NC04 の setter の契約を変える）。
- **D4（承認済み（D-66 の委任で決定、2026-11-25））**: [A37](_completed/A37-index-base-place-borrow.md) の D4 と同じ 1 つの orphan 規則を当てる。利用者のモジュール（依存パッケージを含む）の `AddAssign` 系のインスタンスでは、`Target` か `Rhs` の少なくとも 1 つの頭が、このプログラムで宣言した型（レコード・名前付きの共用体・`extern type`）でなければならない。std の型、組み込みの型、`ref`・`dyn` は、このプログラムの型ではない。違反は `E1016`（kind `orphan_instance`）。`Target` が型変数か組み込みの型（すべての組み込みの型）なら、orphan 規則より先に `builtin_instance_override` になる。匿名共用体は `E1041`。利用者の `extern type` は `Target` になれる。`Rhs` の型変数は `Target` に現れなければならない（`E1027`）。`Target` が std の型なら、実際には `Rhs` が利用者の型でなければならない。単位はプログラムで、std のモジュールにはコンパイラの規則を当てず、`src/stdlib.rs` の読み込みのテストが `dispatch_positions` で確かめる。NC20 は `dispatch_positions` に [0, 1] を足すだけで、判定の関数は A37 のものを使う。検査の順と重なりの報告の位置は §4 のとおり。`IndexBorrowMut` は D3 のとおり std だけで、この規則の外。std の `Rhs` が `'a` の 10 個は、A37 のテスト `std_instances_with_open_dispatch_positions_are_listed` の一覧に足す。採らない案: 起票時の案（`Target` か `Rhs` を宣言したモジュールだけ）、利用者は `AddAssign` を書けない（std だけ）、規則なし。
  理由: (1) A37・NC19 と同じ規則にすると、`x[k]`・`l op r`・`x op= r` のどれでも、std と組み込みの型だけで書いた式の意味が利用者のインスタンスで変わらない。違いは `dispatch_positions` の 1 行だけである。(2) NC20 は std に 30 個の `AddAssign` 系を置き、NC06・NC18 の書き込み窓の追加も続く。利用者が先に `instance AddAssign<Tensor<f64>, f64>` を書けると、std の追加がそのソースを `E1016` で壊す。規則はこれを防ぐ。(3) 利用者のレコードの `+=`（`AddAssign<Money, Money>`、`AddAssign<Money, Tensor<f64>>`）は、そのまま書ける。(4) 組み込みの型の `Target` を登録で拒否するのは、解決がその `Target` ではインスタンスを引かないからである（`Format` が補間の穴から届かない型を拒否するのと同じ理由）。(5) `Rhs` の裸の型変数の形は規則が保証しない穴なので、テストの一覧に載せて追加を止める。(6) `AddAssign` 系はまだ存在しない（`BUILTIN_CLASSES` は 36。`src/polymorph.rs` 947 行）ので、この規則が拒否する既存のプログラムはない。
- **D5（承認済み（D-66 の委任で決定、2026-11-25））**: 評価順は「場所の部分 → 古い値 → 右辺 → 書く」（NC20 の EVL の規則。番号は実装の時点の次の空き）。場所の部分の中は A38 の添字の順（土台、キーの成分を左から右）で、順は型に依らない。右辺は対象を読めるが、書き込み・move・排他借用は `E1014`（NC20 の OWN の規則）。設計の案は「排他借用・move」を挙げたが、右辺の中の代入（`x += { x = 5; 1 }`）も順を観測させるので含める。守りは `exclusive_slice` と同じ仕組みで、新しい解析を作らない。
- **D6（既定案）**: 組み込みの道は §3 の型（整数・浮動小数点・マスクを除く SIMD の数値ベクトル・`string` と `utf8string`。演算子ごとの `Classes::intrinsic` ではなく型で決め、演算子が組み込みでない形は `p = p op e` と同じ誤り）と、推論中の型（`finish` で確かめる）。型変数の対象は `E1005`（`compound_assign_unsupported_target`）。総称関数で `Add<'a>` から `x = x + y` を作ると、`Tensor` へ特殊化したときに黙って確保するため。
- **D7（既定案）**: 右辺が窓・テンソルの更新は NC06 の `*_in_place`（公開、`Result`）と `*_in_place_or_trap`（非公開の双子）を使い、NC20 は足さない。NC20 が足すのはスカラーの `Tensor.*_scalar_in_place`（失敗しないので `unit`、双子なし）だけ。既存の `map_in_place` の命名に合わせた。NC06 の `*_in_place`・`*_in_place_or_trap` の名前と照合済み（2026-11-25 の統合）。
- **D8（既定案）**: Phase 1（組み込み型の複合代入）は A37・A38・A39・NC18・NC19 に依存しない（`Index` も演算子インスタンスもフィールドの排他借用も使わない。組み込みの添字は今も土台、キーの順で評価する）。チケット全体の依存は A37・A38・A39・NC18・NC19 で（[README の追加チケット（2026-11-25）](README.md#追加チケット2026-11-25自然な添字範囲演算子)）、Phase ごとの依存は「着手条件」のとおり（Phase 2 は A39・NC19、Phase 3 は A37・A38・A39・NC16・NC18）。
- **D9（既定案）**: `SliceMustBeBorrowed` の保留は `]` の次が `op=` のときだけ。窓への `=` は意味（代入用のクラス）が決まっていないので対象外とし、今と同じ `E0002` を保つ。設計の案は `=` も挙げたが、取り消した後に受ける規則がないため。
- **D10（既定案）**: ヒント `compound-assignment` を消さず、`**=`・`&&&=`・`|||=`・`^^^=`・`<<<=`・`>>>=` の案内へ直す（help `only + - * / % have an '=' form; write 'x = x op y'`。今の help と同じ長さに抑える: ガイドの pitfalls の章はヒントの help から生成され、`src/guide.rs` 67 行付近の上限 4,000 字がある）。表の件数 40 と `diagnostics.md` の件数を保つ。baseline の case は `x **= 1` へ書き換える。ヒントの表は互換の保証の対象外（docs/stability.md「安定として保たないもの」）。
- **D11（既定案）**: 要素の対象（成分がすべて位置のキー）は、同じ `(Self, Key)` の `Index` と `IndexMut` の組で更新する。std に `Index<Tensor.Mut<'a>, [i64], 'a>`・`IndexMut<Tensor<'a>, [i64], 'a>`・`IndexMut<Tensor<'a>, K, 'a>`・`Index<Tensor.Mut<'a>, K, 'a>`（K は NC16 の 1〜4 軸のキー）を足す（`Index<Tensor, K>` と `IndexMut<Tensor.Mut, K>` は NC16 にあるので足さない）。設計の案は要素の対象に触れていないが、`t[1, 2] += 1.0` は NumPy で最もよく書く形である。`IndexMut<Tensor<'a>, …>` の本体は 2027 の射影の書き込み（`self.data[offset] = v`。std は 2027 で読む）で書けるので、特権も A39 も要らず小さく足せる。値の型は §3 の組み込みの道を取る型（推論中なら `finish` で確かめる）に限り、それ以外（`Tensor<Complex<f64>>` の要素、配列をフィールドに持つ Copy の利用者のレコード）は `E1005`（`compound_assign_unsupported_target`）にする。組み込みでない型の `+=` は `AddAssign` だけで解決し、`Add` へ暗黙に代えない（D1・D-66。§4 と同じ規則）。要素の値の `AddAssign` を引いて更新する道は、要素の更新のクラスの設計が要るので対象外にする（窓のキーと対象の全体の `+=` は、§4 の std のインスタンスの中で要素型の演算子を使うので、`Tensor<Complex<f64>>` でも通る）。
- **D12（既定案）**: 「確保 0 回」は、スカラーの右辺と、同じ形で連続な右辺に限る。ブロードキャスト・ストライドのある右辺と書き込み窓は、軸の数に比例する形とストライドの配列を確保する（`Tensor.Mut` が `shape`・`strides` の配列を持つため）。要素数に比例する確保はどの場合もしない。
- **D13（決定（D-66 の委任で決定、2026-11-25））**: (a) `Tensor.update` は要素が Copy の型で中身を 2 回複製し、docstring の「without copying」と食い違う（R4）。NC06 が `*_into` の出力先を `Tensor.update` で用意する計画も、同じ複製を受け継ぐ。所有を移す（`own Tensor<'a>`・`own Matrix<'a>` を取る）のに、要素が Copy の型で中身を複製する std の関数は 9 つある（`Tensor.update`・`to_array`・`reshape`・`reshape_infer`・`try_reshape`・`of_matrix`・`to_matrix`、`Matrix.to_array`・`set`。起票の時点の記録は、文書が「複製しない」と書く 7 つで、`reshape_infer`・`try_reshape` は PM10 の再測定で見つけた同じ形の関数）。これを性能のチケット [PM10](../_perfs/_completed/PM10-std-consume-copies.md) にした。PM10 の Phase 1 は std の本体の修正、Phase 2 は A17 の D1 のとおり型付き IR から X1 を精密にする規則で、A39 のフィールドの借用と NC20 の `+=`・`IndexMut`・`IndexBorrowMut` の対象の借用の根にも当てる。PM10 は A39 にも NC20 にも依存しない。9 つの関数の docstring と `tensor.md`・`matrix.md` は、このチケット群の PR で今の振る舞いと既知の制限の注記に直した（コメントと文書だけで、コードは変えない）。NC20 には入れない（X1 の精密化は横断の変更で、独自の差分テストを持つ別の PR にする）。採らない案: `update` だけを直す PM10、文書だけの修正、NC20 に入れる、新しい複製なしの API。(b) `IndexMut` の `t[k] = v` がキーを右辺より先に評価し、組み込みの添字の代入（EVL-012）と順が違う（R3）。これは [A38](_completed/A38-index-evaluation-order.md) で解決（D-66 の委任で決定、2026-11-25）。A38 は、組み込みの添字とすべての `Index` の経路の評価順を 1 つの規則（土台、キーの成分を左から右）にそろえ、`IndexMut` の `=` も EVL-012 のとおり右辺を先にする。複合代入の順は NC20 の規則（場所の部分が先）のままで、NC20 の §2・§6・§7 の順は A38 の規則に合う。
- **D14（決定（D-66 の委任で決定、2026-11-25））**: 連鎖した書き込み窓 `ref mut t[1..3, ..][0, ..]` は受理せず、`E1013`（`borrow_of_temporary`）に help `bind the inner window first: 'let mut w = ref mut t[1..3, ..]', then 'ref mut w[0, ..]'` を付ける（§6）。複合代入の対象の `t[1..3, ..][0, ..] += u` は、§1 の保留が `]` の次の `op=` だけなので、NC18 のままの `E0002`。長い形の連鎖（`t[[A]][[B]] += u`）は、土台が組み込みでない型の添字で、内側に `IndexBorrowMut` のインスタンスがあれば同じ `E1013` にし、help は利用者の角括弧の綴りで `w[[B]] += …` を案内する（インスタンスがなければ §6 のとおり `E1042` の `index_view_not_writable` か、help のない `E1013`。NC16 の `E1042` の `ref` を足す案内は、`View` の土台に `IndexBorrowMut` がないので出さない）。採らない案: A37 の事実 `returns_contents`（一時値の土台を許す）を排他の窓 `IndexBorrowMut<Tensor.Mut<'a>, [Slice.Sel], Tensor.Mut<'a>>` に広げて連鎖を受理する、パーサーの保留を連鎖にも広げる。
  理由: (1) 受理には、一時値の `Tensor.Mut` の排他の再借用が内側の窓の loan だけを運ぶことを所有権の検査で示す新しい規則が要る。NC18 の S4 が共有の窓について挙げた危険と同じ種類で、NC20 の範囲をさらに広げる。(2) 束縛してから書く形（`let mut w = ref mut t[1..3, ..]` の後の `ref mut w[0, ..]`・`w[0, ..] += u`）で同じことが書け、費用も同じ（窓は配置の計算だけで、データに触れない）。(3) 受理を増やす拡張なので、後から edition なしで足せる。暗黙の誤りでなく、help の付いた診断にしておけば、利用者が迷わない。
