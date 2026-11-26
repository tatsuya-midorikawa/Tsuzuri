# A37: `Index` が土台の場所を借りないため、一時値の土台の窓が解放後のメモリを読み、`Tensor.Mut` から取った窓が書き込み中も生きる不具合の修正

| 項目 | 内容 |
| --- | --- |
| ID | A37 |
| 優先度 | P0（メモリ安全性。安全なコードだけで、解放後のメモリの読み出しと、共有の窓が生きている間の排他の書き込みが起きる。A38・NC16・NC18・NC20 の前提。手順 5 の orphan 規則は NC06 の演算子インスタンスの段と NC19 の前提。NC03・NC04 を含む最初のリリースの条件（D8）） |
| 規模 | M（手順 5 は別のコミットにする） |
| 依存 | –（土台の [NC04](_completed/NC04-generic-index-class-and-slice.md) と [A31](_completed/A31-in-place-writes-keep-stored-borrows.md) は完了済み） |
| 後続 | [A38](A38-index-evaluation-order.md), [NC16](NC16-index-ref-and-zero-alloc-element-reads.md), [NC18](NC18-multi-axis-bracket-indexing.md), [NC20](NC20-compound-assignment-and-mutable-index-views.md), [NC06](NC06-tensor-ufuncs-and-elementwise.md)（演算子インスタンスの段。手順 5 の後）, [NC19](NC19-operator-extension-instances.md)（手順 5 の後） |
| 状態 | done |
| 起票 | 2026-11-25（[GUIDE D-66](GUIDE.md#d-66-自然な添字範囲演算子d-65-の改訂a37nc16nc20)。HEAD `858a949c` で確認） |
| 承認 | 承認済み（D-66 の委任で決定、2026-11-25）: orphan 規則（D4。今ある 1 引数の算術クラスにも当てる）と、利用者と std のインスタンスの重なりの報告の位置（D7）。不具合の修正（D1〜D3・D5・D6）は承認が要らない（今受理している危険なプログラムが `E1013`・`E1014` になる）。NC03・NC04 を含む最初のリリースの前に入れること（D8）と、添字の評価順を A37 で変えないこと（D9）は決定（D-66 の委任で決定、2026-11-25） |
| 改善する劣位 | 不具合（メモリ安全性）: 結果が土台を借りる `Index`（`t[[Slice…]]`、`m[(Slice…, Slice…)]`、利用者のインスタンス）の土台に一時値を書くと検査を通り、窓は解放した一時値のメモリを読む。土台が region を持つ型（`Tensor.Mut`、`Tensor.View`、`MatrixView`、利用者の region 付きレコード）だと、窓は土台の場所を借りないので、窓が生きている間に土台へ書き込め、土台を置き換えた後も古いメモリを読める。同じことを関数の形（`Tensor.freeze (ref m)`、`pick (ref base)`）で書くと正しく `E1014` になる。不具合（一貫性）: 利用者のモジュールが、std の型と組み込みの型を振り分けの位置に持つ `Index`・`IndexMut` と 1 引数の算術クラスのインスタンスを書ける。利用者の `instance Mul<Tensor<f64>>` が `t * t` の意味を決め、`instance Neg<i32u>` が符号なし整数の `-x` を通す（現状 5）。std がインスタンスを足すと、こうした利用者のソースは std のファイルの位置の `E1016` で壊れる |
| 手本にする既存実装 | `src/constants.rs:308` の `temporary_borrows`（ふつうの呼び出しは、一時値の借用を、結果が loan を持たないときだけ `BorrowOperand` にする）、`src/check.rs:14926`（`IndexMut` の対象を `TypedExprKind::Borrow(raw_target, true)` で下げる）、`src/ownership.rs` の A34 の `share_returns`（3895〜3933 行付近。本文の検査の結果の loan から仮引数ごとの事実を求める）、`src/polymorph.rs:3913` の `validate_drop_instance`（std の型へのインスタンスを制限する） |
| 主な影響ファイル | `src/check.rs`、`src/polymorph.rs`（D4 の `dispatch_positions`・`validate_orphan_instance`、D7 の重なりの報告の位置）、`src/ownership.rs`、`src/diagnostic_kinds.rs`（D4 の kind）、`src/stdlib.rs`（D4 の 2 つのテスト）、場合により `std/Tensor.tz`・`std/MatrixView.tz`（`Index` の本文。手順 4）、`tests/index_class.rs`、`tests/polymorphism.rs`・`tests/visibility.rs`（D4）、`tests/fixtures/tensor/Main.tz`・`tests/tensor-cases.mjs`、`tests/fixtures/matrix_view/Main.tz`・`tests/matrix-view-cases.mjs`、`spec/rules/TYP.md`・`tests/conformance/TYP/`（D4 の規則。番号は実装の時点の TYP の次の空き）、`docs/stability.md`（既知の例外「std への instance の追加」）、`_tsuzuri/language-reference/built-in-types-and-modules/tensor.md`、`_tsuzuri/language-reference/built-in-types-and-modules/slice.md`、`_tsuzuri/language-reference/types-and-type-inference/type-classes.md`、`_tsuzuri/language-reference/ownership-and-memory/borrowing.md`、`_tsuzuri/language-reference/compiler/diagnostics.md`（生成）、`docs/architecture.md`、`docs/language.md`（「多相関数と型クラス」） |

## 実装者向け早見

- 範囲: (1) 土台を借りる `Index`（宣言の Output が参照・関数・region を持つレコードなど借用を運ぶもの。仕様 1）で、土台が場所なら `Index.index (ref k) (ref x)` と同じ場所の共有借用を取る（`TypedExprKind::Borrow(base, false)`）。(2) 土台が一時値なら、インスタンスの本文が「結果は土台の参照先が持つ借用だけを持つ」と証明できたとき（新しい事実 `returns_contents`）だけ受理し、ほかは既存の `E1013`（`borrow requires a local place`）にする。std の `Index<View, …, View>`・`Index<MatrixView, …, MatrixView>` は証明でき、`t[[Slice.range 1 3]][[Slice.at 1]]` のような連鎖は今のまま通る。(3) 値を返す `Index`（`t[[i, j]]`、`m[(i, j)]`、`map[k]`、マスク）と `IndexMut` は変えない。(4) `Index`・`IndexMut` と 1 引数の算術クラス（`Add`〜`Pow`・`Neg`）の利用者のインスタンスに、振り分けの位置（Self と Key、または T）の 1 つの頭がこのプログラムで宣言した型（レコード・名前付きの共用体・`extern type`）であることを求める orphan 規則を足す（`E1016`、新しい kind `orphan_instance`。D4）。[NC19](NC19-operator-extension-instances.md)・[NC20](NC20-compound-assignment-and-mutable-index-views.md) は同じ規則に自分の位置を足す。利用者と std のインスタンスの重なりは利用者の位置で報告する（D7）。(4) は手順 5 で、別のコミットにする。
- 最初に読む: ① `src/check.rs` 14748〜14837 行（`ExprKind::Index` の脱糖） ② `src/constants.rs` 302〜408 行（`temporary_borrows`） ③ `src/ownership.rs` 14688〜14690 行（`E::BorrowOperand`）と 12141〜12270 行（`read_places`） ④ 下の「再現」。
- 最初に書くテスト: `tests/index_class.rs` に「再現」の 1・2 と 3 の各形を `rejects(…, "E1013")`・`rejects(…, "E1014")` で足し（今は受理されるので失敗する）、std の窓の連鎖と要素の読み出しの一時値を `emits` で足す（今も修正後も受理）。
- 触ってはいけない: 組み込みの 6 型（`[T]`、`[|T|]`、`Vec<T>`、`[T; N]`、`string`、`utf8string`）の添字と IR、値を返す `Index` の IR、一時値のキー（NC04 の「キーの借用は式の終わりで閉じる」）、`IndexMut` の下げ方、関数の形の一時値の規則（`temporary_borrows`）、添字の評価順（[A38](A38-index-evaluation-order.md) が変える。D9）、既存の `builtin_instance_override`・`anonymous_union_instance` の条件（kind を `orphan_instance` に移さない）、インスタンスの登録の順。
- 完了の見分け方: 「再現」の 1〜3 がすべて `E1013` か `E1014` になり、「正しく扱われている形」がすべて今と同じ出力（または同じ診断）になり、要素の読み出しの IR が 1 バイトも変わらない。手順 5 の後は、D4 のテストと `src/stdlib.rs` の 2 つのテストが通り、`record K` と `instance Index<Map<K, i64>, K, i64>` の `E1016` が利用者のインスタンスの位置に出る。

## 目的

NC04 は `Index` の結果が「ベース（コンテナ）の loan だけを継承する」と定めた（NC04 の仕様 1・D1）。実装は、土台を `TypedExprKind::BorrowOperand` で貸し、土台の値が「持っている」借用だけを結果へ渡す。土台の場所そのものへの借用は取らない。土台が region を持たない局所の `Tensor` なら、所有権検査が読み出しのついでに場所の借用を足すので正しく動くが、一時値の土台と region を持つ型の土台では、結果は土台の記憶域を借りないまま、その中を指せる。

このチケットは、結果が土台を借りる `Index` の呼び出しを、同じことを関数で書いた `index (ref k) (ref x)` と同じ借用規則にそろえる。安全なコードで解放後のメモリを読むこと、共有の窓が生きている間に土台へ書き込むことをなくす。NC16（`ref` を必須にする）、NC18（多次元の角括弧 `t[i, a..b]`）、NC20（書き込み窓 `ref mut t[…]` と複合代入）は、この借用規則の上に作る。

## 着手条件と停止条件

### 着手条件

- GUIDE §2.3 の基準コマンドが成功し、「再現」を基準の dev バイナリ（`cargo build`）で確かめていること。
- D4（orphan 規則）と D7（重なりの報告の位置）は承認済み（D-66 の委任で決定、2026-11-25）。手順 5 もこのチケットの PR に入れる。
- リリースの条件（D8）: このチケット（手順 5 を含む）は、A38・NC16 とともに、NC03・NC04 を含む最初のリリースの前に入れる。リリースの数え方は D8。先にそのリリースが出ても手順 5 は入れ、次のリリースノートの Compatibility に書く。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- **S1** `std/`・`examples/`・`tests/`・`_tsuzuri/language-reference/` のプログラムのうち、「再現」の形でないものの診断・出力が変わる。「正しく扱われている形」の表とテスト計画の受理のテスト（総称関数の中の連鎖を含む）のどれかが拒否される（D2 の局所の `View` の move を除く）。HEAD で土台を借りる `Index` の構文を使うのは `tests/index_class.rs`（145・154・169・178 行）、`tests/matrix_views.rs`（241・248 行）、`slice.md`・`tensor.md` の例だけで、どれも局所の土台なので変わらない見込み（`rg -n -F '[[Slice'`・`rg -n -F '[(Slice'` で確認済み。`std/` と `examples/` には `Index` の構文がない）。ほかの `Index` の使用（`tests/index_class.rs`・`tests/api_diff.rs`・`tests/kg.rs` の `Grid`）は値を返すインスタンスで、変わらない。手順 5 の orphan 規則で `E1016` になるのは、HEAD では `tests/polymorphism.rs` 1060〜1062 行の `instance Add<[i32]>` だけの見込み（`tests/`・`examples/`・`_tsuzuri/`・`docs/`・`guide/` の `Index`・`IndexMut`・算術のインスタンスを `rg` で確認済み。ほかは利用者のレコードの頭か、今も `builtin_instance_override`・`E1018` になる形）。これ以外が `E1016` になったら止める。
- **S2** std の `Index<View<'a>, [Slice.Axis], View<'a>>`（`std/Tensor.tz:1676`）と `Index<MatrixView<'a>, (Slice.Axis * Slice.Axis), MatrixView<'a>>`（`std/MatrixView.tz:468`）で、事実 `returns_contents` が立たず、手順 4 の本文の書き換えでも立たない（連鎖 `t[[…]][[Slice…]]`・`m[(…)][(…)]` が `E1013` になる）。
- **S3** 値を返す `Index` の呼び出しの IR が変わる。または、土台を借りる `Index` の場所の土台の呼び出しの IR が変わる（`Borrow` と `BorrowOperand` はどちらも場所の番地を渡すので、変わらない見込み。設計の「生成 IR」）。
- **S4** 事実 `returns_contents` を求める検査が、A28・A31 の合同の不動点（`joint_facts`）と循環して決まらない。
- **S5** GUIDE §13.1 の共通の条件。
- **S6** 手順 5 の `std_instances_with_open_dispatch_positions_are_listed` が見つける std のインスタンスが、`Index<Map<'key, 'value>, 'key, 'value>`（`std/Map.tz:508`）と `Index<HashMap<'key, 'value>, 'key, 'value>`（`std/HashMap.tz:490`）の 2 つと違う（HEAD の std の `Index`・`IndexMut`・算術のインスタンスで、振り分けの位置に裸の型変数を持つのはこの 2 つだけ。`rg -n '^instance .*\b(Index|IndexMut|Add|Sub|Mul|Div|Rem|Pow|Neg)<' std` で確認済み）。または、std のどれかのモジュールを読み込むと `orphan_instance` が出る（std は検査の対象外のはず）。

## 現状（HEAD `858a949c` で確認）

### 土台を借りる `Index` と値を返す `Index`

- `Index` の読み出しの生成関数 `$instance.{id}.index`（`src/polymorph.rs:2639` 付近の `index_instance_function` の呼び出し）は、Output が共有参照（`ref T`）か region を 1 つ持つレコードのとき（`output_needs_region`、`src/polymorph.rs:3999`）に region `r` を持つ（4038〜4046 行）。Self が region を持つなら `self: ref (Self {r})`、持たないなら `self: ref {r} Self` で、結果は `Output {r}`（4105〜4138 行）。region を持たない Output でも、借用を運ぶ型（参照を含むタプル、関数）なら、本文は土台の借用を結果に入れられる（再現 3-g・3-h）。
- このチケットでは、宣言の Output が借用を運ぶ（`Type::carries_loans`、`src/check.rs:907`。型変数は数えない）インスタンスを **土台を借りる `Index`** と呼び、ほか（`'a`、`Tensor<'a>`、`i64` など）を **値を返す `Index`** と呼ぶ（仕様 1）。
- std の土台を借りる `Index`: `Index<Tensor<'a>, [Slice.Axis], View<'a>>`（`std/Tensor.tz:1670`）、`Index<View<'a>, [Slice.Axis], View<'a>>`（1676）、`Index<Matrix<'a>, (Slice.Axis * Slice.Axis), MatrixView<'a>>`（`std/MatrixView.tz:461`）、`Index<MatrixView<'a>, (Slice.Axis * Slice.Axis), MatrixView<'a>>`（468）。
- std の値を返す `Index`: `Tensor`・`View` の `[i64]`（`std/Tensor.tz:1660`・1666）、マスク（2883・2890）、`Matrix`・`MatrixView` の `(i64 * i64)`（`std/Matrix.tz:277`、`std/MatrixView.tz:455`）、`Map`（`std/Map.tz:508`）、`HashMap`（`std/HashMap.tz:490`）。
- region を持つ std のレコード: `Tensor.View`（`std/Tensor.tz:11`）、`Tensor.Mut`（16）、`MatrixView`（`std/MatrixView.tz:4`）、`MatrixView.Mut`（8）。`Tensor`（`std/Tensor.tz:5`）と `Matrix` は region を持たない。

### 原因

1. **脱糖が土台を直接 `BorrowOperand` にする。** `src/check.rs:14748` の `ExprKind::Index` は、組み込みの 6 型でなければ `resolve_index`（`src/polymorph.rs:2758`）でインスタンスを選び、キーも土台も `TypedExprKind::BorrowOperand` で包んだ呼び出しを作る（`src/check.rs:14822〜14834`。土台は 14830 行）。Output が借用を運ぶかどうかを見ない。
   - ふつうの呼び出しは、`ref` の仮引数への引数を `TypedExprKind::Borrow(value, false)` にし（`coerce_argument`、`src/check.rs:15423` 付近）、一時値の借用を `BorrowOperand` に変えるのは、呼び出しがすべての引数を一度に取り、結果が loan を持たないときだけ（`temporary_borrows`、`src/constants.rs:308`。判定は 390 行の `!single_stage || !self.loan_free(result)`）。ほかは `Borrow` のまま残り、所有権検査が `E1013` を出す（確認: `Tensor.view (mk 100)` も `Tensor.view (ref (mk 100))` も `E1013`）。`Index` の脱糖はこの規則を通らない。
   - `IndexMut` は対象を `TypedExprKind::Borrow(raw_target, true)` で下げている（`src/check.rs:14926`）ので、場所の排他借用を取る。
2. **所有権検査は `BorrowOperand` を値の読み出しとして評価する。** `E::BorrowOperand` は中身を `Use::Read` で評価する（`src/ownership.rs:14688〜14690`）。`read_places`（12141 行付近）は、型が loan を運ばないときだけ場所の共有借用を足し（12266〜12269 行）、loan を運ぶ型（`View`・`Mut`・`MatrixView`・region 付きのレコード）では、場所が持つ loan を写すだけで場所の借用を取らない（12228〜12265 行）。一時値なら、一時値が持つ loan だけになる（14393 行のコメント「A borrowed temporary's value already has only what the temporary holds.」）。
3. **コード生成は一時値を呼び出しの直後に解放する。** `BorrowOperand` の一時値は slot に置かれ、`release_operands` が呼び出しの直後に解放する（`operand_borrow`、`src/llvm.rs:7501〜7524`、`release_operands` 7526 行）。結果の窓が一時値の記憶域を指していれば、その後の読み出しは解放後のメモリを読む。
4. **本文の検査は呼び出し側の借用に頼っている。** 生成関数の本文は、Self の中の region `r` の下で参照先のフィールドの借用を返しても受理される（下のプログラム d の `ref x.own_data[0..]`）。安全性は呼び出し側が土台の場所を借りることに依っているが、`BorrowOperand` はそれを落とす。
5. **orphan 規則がない。** `register_index_instance`（`src/polymorph.rs:2495`）は Self と Key の宣言元を見ない。検査は、組み込みの 6 型と型変数の Self の `builtin_instance_override`（2519〜2535 行）と、匿名共用体の `E1041`（2536〜2547 行）だけである。1 引数のクラスの登録（2172〜2222 行付近）も、`E1041`、`builtin_instance_override`（組み込みの演算を持つ型など）、`Drop`・`Format` の検査だけで、std の型と組み込みの型の頭を拒否しない（`Drop` は `validate_drop_instance`、`Format` は `validate_format_instance`（3971 行）で、利用者が std の型に定義することを拒否している）。HEAD `858a949c` の dev バイナリ（マニフェストなし。`--edition 2026` でも同じ）で、次が受理される。
   - 利用者のモジュールの `instance Index<Tensor.Mut<i64>, …>`。上の穴に std の型で届く（再現 2）。
   - 利用者の `instance Mul<Tensor<f64>>`。本文が `Tensor.init [2] (\i -> 42.0)` を返すと、`t * t` は `-O0`・`-O3` とも `[42, 42]` になる。利用者のインスタンスが std の型の演算子の意味を決める。NC06 が std に足す `Mul<Tensor<'a>>` とは必ず重なる。
   - 利用者の `instance Neg<i32u> { fn neg _x = 99i32u }`。`let x = 5i32u` の `-x` が `-O0`・`-O3` とも `99` になる。`basic-types.md`（73 行）は、符号なし整数の単項 `-` を `E1005` と書いている。
   - 利用者の `instance Add<Maybe<i64>>`、`instance Add<bool>`、`instance Add<(i64 * i64)>`、`instance Add<ref Point>`。`extern type copy Buf` の `instance Index<Buf, i64, f64>` と `instance Add<Buf>` も受理される（D4 の後も受理する）。
   - 利用者のインスタンスと std のインスタンスが重なると、`overlapping_instance` は std の側の位置で報告される。`record K { v: i64 }` と `instance Index<Map<K, i64>, K, i64>` は `std/Map.tz:508:39` の `E1016`（`overlapping instance for Index<Map<'key, 'value>, 'key>`）になる。利用者は std のファイルを直せない。重なりの検査（1 引数のクラスは 2235〜2268 行付近、`Index` は 2574〜2614 行付近）は、登録している側のインスタンスの `instance.class.span` で報告する。`InstanceTemplate`（911 行）と `IndexInstanceTemplate`（892 行）は `span` を持つが、モジュールの起源を持たない。

`E1013` を `borrow_of_temporary`（`src/ownership.rs:7817`、文言は 7845 行）の中で直そうとしても届かない。`borrow_of_temporary` は `place()`（7706 行）からだけ呼ばれ、`BorrowOperand` は `place()` を呼ばない。

### 再現（検証済み。HEAD `858a949c` の dev バイナリ、macOS arm64 の native、マニフェストなし（edition 2027）。1・2 は `--edition 2026` でも同じ）

どれも `tsuzuri check` は何も報告しない。値は `node scripts/slot.mjs -- target/debug/tsuzuri run -O0 <dir>/Main.tz` と `-O3` の出力。解放後のメモリの値は不定で、環境によって変わる。プローブはファイル名を大文字で始める（`Main.tz`。`E1011`）。

#### 1. 一時値の土台

```text
def mk :: i64 -> Tensor<i64>
fn mk n = Tensor.init [4] (\i -> i + n)
let v = (mk 100)[[Slice.range 1 3]]
let junk = Tensor.init [4] (\i -> i + 7000)
$"{Tensor.to_array (Tensor.to_tensor (ref v))} {(Tensor.as_array (ref junk))[0]}"
```

| | 出力 |
| --- | --- |
| 期待 | `[101, 102] 7000` |
| `-O0` | `[3, 0] 7000` |
| `-O3` | `[0, 0] 7000` |
| 対照: 3 行目を `let base = mk 100` と `let v = base[[Slice.range 1 3]]` の 2 行にする | `-O0`・`-O3` とも `[101, 102] 7000` |

#### 2. `Tensor.Mut` から取った窓

```text
instance Index<Tensor.Mut<i64>, [Slice.Axis], Tensor.View<i64>> {
    fn index _axes self = Tensor.freeze self
}
let mut data = [1, 2, 3, 4]
let mut m = Tensor.borrow_mut [4] (ref mut data)
let v = m[[Slice.all()]]
let before = Tensor.to_array (Tensor.to_tensor (ref v))
Tensor.write [0] 99 (ref mut m)
let after = Tensor.to_array (Tensor.to_tensor (ref v))
$"{before} {after}"
```

- `-O0`・`-O3` とも `[1, 2, 3, 4] [99, 2, 3, 4]`。共有の窓 `v` が生きている間に、`m` への排他の書き込みが通る。
- 対照: インスタンスを消し、`let v = m[[Slice.all()]]` を `let v = Tensor.freeze (ref m)` にすると `error[E1014]: access conflicts with a live borrow`（位置は `Tensor.write [0] 99 (ref mut m)` の `(ref mut m)`。note の借用は `Tensor.freeze (ref m)` の `(ref m)`）。

#### 3. 同じ原因のほかの形（すべて受理される）

| 名前 | 形 | 期待 | `-O0` | `-O3` | 対照 |
| --- | --- | --- | --- | --- | --- |
| 3-a 連鎖した一時値 | `mk n = Tensor.init [2, 1] (\i -> i + n)` で `let x = (mk 100)[[Slice.range 0 2]][[1, 0]]`・`let z = (mk 100)[[Slice.range 0 2]][[0, 0]]`・`$"{x} {z}"` | `101 100` | `0 0` | `0 0` | `let b = mk 100` の後に `b[[…]][[…]]` は `101 100` |
| 3-b `Matrix` の一時値 | `mk n = Matrix.init 3 3 (\r c -> r * 3 + c + n)` で `let mv = (mk 100)[(Slice.range 1 3, Slice.all())]`・`let junk = Matrix.init 3 3 (\r c -> r * 3 + c + 7000)`・`$"{mv[(0, 0)]} {mv[(1, 2)]} {junk[(0, 0)]}"` | `103 108 7000` | `7003 7008 7000` | `0 0 7000` | – |
| 3-c 参照を返す利用者の `Index` | 下のプログラム c | `name-101 junk-7000` | `junk-7001 junk-7000` | `trap: allocation size overflow`（`E2005`） | `let g = mk 100`・`let s = g[1]`・`let moved = g` は `E1014`（正しい） |
| 3-d region 付きの利用者の型の一時値 | 下のプログラム d で `let z = (mk 100 (ref e))[0]` | `100 105 7000 9000` | `7000 7005 7000 9000` | `0 0 7000 9000` | – |
| 3-e region 付きの利用者の型の局所の土台を置き換える | プログラム d で `let mut base = mk 100 (ref e)`・`let z = base[0]`・`base = mk 900 (ref e)` の後に `z.b[0]` を読む | `E1014` | `7000 7005 7000 8000 900` | `0 0 7000 8000 900` | 同じ本文を `def pick { r } :: ref R { r } -> R { r }` にして `let z = pick (ref base)` と書くと `E1014`（`base = mk 900 (ref e)` の行） |
| 3-f 同じ呼び出しの中の重なり | 下のプログラム f | `E1014` | `[1, 1, 1, 1, 1]` | `[1, 1, 1, 1, 1]` | `overlap_copy (Tensor.freeze (ref m)) (ref mut m)` は `E1014`（`(ref mut m)` の位置） |
| 3-g region を持たない Output（参照を含むタプル） | プログラム c の `Index` を `instance Index<Grid, i64, (ref string * i64)> { fn index k self = (ref self.names[deref k], deref k) }` にし、`let p = (mk 100)[1]` を `match p with \| (s, i) -> $"{s} {i} {junk[0]}"` で表示 | `name-101 1 junk-7000` | `junk-7001 1 junk-7000` | `trap: allocation size overflow`（`E2005`） | – |
| 3-h region を持たない Output（関数） | 下のプログラム h | `8 junk-7000000` | `trap: index out of bounds`（`E2005`。ラムダの中の `self.names[i]`） | 同じ | – |

プログラム c（`ref string` を返す利用者の `Index` を一時値に使う）:

```text
record Grid { names: [string] }
instance Index<Grid, i64, ref string> {
    fn index k self = ref self.names[deref k]
}
def mk :: i64 -> Grid
fn mk n = Grid { names: Array.init 3 (\i -> $"name-{i + n}") }
let s = (mk 100)[1]
let junk = Array.init 3 (\i -> $"junk-{i + 7000}")
$"{s} {junk[0]}"
```

プログラム d（region 付きの利用者のレコードと、参照先のフィールドを借りて返す `Index`）:

```text
record R { r } { own_data: [i64], b: ref { r } [i64] }

instance Index<R, i64, R> {
    fn index _k x = R { own_data: [], b: ref x.own_data[0..] }
}

def mk { r } :: i64 -> ref { r } [i64] -> R { r }
fn mk n e = R { own_data: Array.init 6 (\i -> i + n), b: e }

let e = [1, 2, 3]
let z = (mk 100 (ref e))[0]
let junk = Array.init 6 (\i -> i + 7000)
let junk2 = Array.init 6 (\i -> i + 9000)
$"{z.b[0]} {z.b[5]} {junk[0]} {junk2[0]}"
```

3-e は、プログラム d の宣言（`record`・`instance`・`mk`）はそのままにして、`let e = [1, 2, 3]` 以降を次に置き換える。

```text
let e = [1, 2, 3]
let mut base = mk 100 (ref e)
let z = base[0]
base = mk 900 (ref e)
let junk = Array.init 6 (\i -> i + 7000)
let junk2 = Array.init 6 (\i -> i + 8000)
$"{z.b[0]} {z.b[5]} {junk[0]} {junk2[0]} {base.own_data[0]}"
```

プログラム f（`Mut` から取った窓と `ref mut` の同じ `Mut` を 1 つの呼び出しに渡す。`add_assign (ref <a の窓>) (ref mut <a>)` と同じ形）:

```text
instance Index<Tensor.Mut<i64>, i64, Tensor.View<i64>> {
    fn index _k self = Tensor.freeze self
}

def overlap_copy :: Tensor.View<i64> -> ref mut Tensor.Mut<i64> -> unit
fn overlap_copy src dst =
    let part = Tensor.narrow 0 0 4 (ref src)
    let mut tail = Tensor.narrow_mut 0 1 4 dst
    Tensor.copy_from (ref part) (ref mut tail)

let mut data = [1, 2, 3, 4, 5]
let mut m = Tensor.borrow_mut [5] (ref mut data)
overlap_copy m[0] (ref mut m)
$"{data}"
```

プログラム h（土台を捕捉した関数を返す `Index`。生成関数は region `r` を持たない）:

```text
record Grid { names: [string] }
instance Index<Grid, i64, (unit -> i64)> {
    fn index k self =
        let i = deref k
        let f = \() -> String.length self.names[i]
        f
}
def mk :: i64 -> Grid
fn mk n = Grid { names: Array.init 3 (\i -> $"name-{i + n}") }
let f = (mk 100)[1]
let junk = Array.init 3 (\i -> $"junk-{i + 7000000}")
$"{f ()} {junk[0]}"
```

### 正しく扱われている形（変えない）

| 形 | 今の結果（`-O0`・`-O3`） |
| --- | --- |
| 局所の `Tensor` の窓の連鎖 `t[[Slice.range 1 3]][[Slice.at 1]]`（`t = Tensor.init [4, 3] (\i -> i + 100)`） | `[106, 107, 108]`。2 段目の土台は一時値の `View` |
| 局所の `Matrix` の窓の連鎖 `m[(Slice.range 1 3, Slice.all())][(Slice.all(), Slice.at 2)]`（`m = Matrix.init 3 3 (\r c -> r * 3 + c + 100)`） | `mv[(0, 0)]`・`mv[(1, 0)]` が `105 108` |
| 局所の `Tensor` の窓と要素 `t[[Slice.range 1 3]][[Slice.at 0]]`・`t[[Slice.range 1 3]][[1, 2]]`（`t = Tensor.init [4, 3] (\i -> i)`） | `[3, 4, 5] 8` |
| 一時値の要素の読み出し `(mk 100)[[2]]`・`(mm 100)[(1, 1)]` | `102 103` |
| 局所の `Tensor` の窓が生きている間に `t` を move する | `E1014`（`tests/index_class.rs:176`） |
| 局所の `Tensor` の窓を関数から返す | `E1013`（`tests/index_class.rs:165`） |
| インスタンスの本文がキーの借用を返す | `E1013`（`tests/index_class.rs:189`） |
| 一時値のキー `t[[Slice.step 1 4 2, Slice.rev()]]`・`m[(1, 2)]`・`map["key"]` | 受理（`tests/index_class.rs:141`） |
| 関数の形 `Tensor.slice [Slice.at 1] (ref v)` の後に `let moved = v` | `E1014`。関数の形は、Self が region を持っても土台の場所の借用を結果に残す（`def fr { r } :: ref Tensor.Mut<i64> { r } -> Tensor.View<i64> { r }` を `fr (ref m)` で呼んでも、窓が生きている間の `Tensor.write [0] 99 (ref mut m)` は `E1014`） |
| 関数の形の一時値 `Tensor.slice [Slice.at 1] (ref (Tensor.view (ref t)))` | `E1013` |

## 変えてはいけない意味

- 組み込みの 6 型の添字の規則と IR（全整数型の添字、境界検査と F12 の範囲の証明、edition 2027 の添字代入）。共有スライス `ref xs[a..b]`・排他スライス `ref mut xs[a..b]`。
- 値を返す `Index` の呼び出しの型・借用・IR（`BorrowOperand` のまま）。一時値の土台からの要素の読み出しも受理のまま。
- 一時値のキー（NC04: キーの借用は式の評価の終わりで閉じ、インスタンスの本文がキーの借用を返すと `E1013`）。
- `IndexMut` による `x[k] = v` の下げ方（対象の `Borrow(raw_target, true)`）。評価順序（今はキー、右辺、対象の順）は A37 では変えない（[A38](A38-index-evaluation-order.md) が組み込みの添字にそろえる。D9）。
- 局所の `Tensor`・`Matrix` の土台からの窓の借用（今も場所の借用を持つ）と、その診断（`E1013`・`E1014`）。
- 関数の形の一時値の規則（`temporary_borrows`）。`Tensor.slice [Slice.at 1] (ref (Tensor.view (ref t)))` は `E1013` のまま。
- 一時値の解放の時点（`release_operands` が呼び出しの直後に解放する）。一時値の寿命を延ばして直さない（D3）。

## 仕様

### 1. 用語

- **土台を借りる `Index`**: 宣言の Output が借用を運ぶインスタンス。判定は宣言の Output（インスタンスの頭の型変数は置き換えない）に対する `Type::carries_loans`（`src/check.rs:907`）で、参照、関数（環境に借用を持てる）、借用を持つ `dyn`、それらを含むタプル・配列・レコード・共用体（region を持つレコードを含む）が真。型変数は偽（`_ => false`）。生成関数が region `r` を持つ場合（`output_needs_region`）より広い。region `r` を持たない Output でも、`(ref string * i64)` や `(unit -> i64)` は本文が土台の借用を入れて返せる（再現 3-g・3-h）。型変数の Output（`Index<Tensor<'a>, [i64], 'a>` を `'a = MatrixView<i64>` で使うなど）は、総称の本文が土台の借用をその型で作れないので、結果は土台の参照先が持つ借用しか持てない。
- **場所**: `TypedExpr::is_place`（`src/check.rs:5602`）が真の式。局所値、フィールド・共用体の中身・リストの tail の経路、組み込みの配列・リスト・`Vec`・固定長配列の要素、`deref r`。`ExprKind::Index` の土台は `autoderef`（15696 行）を通るので、`r: ref Tensor<i64>` の `r[[…]]` の土台は `deref r` で、場所である。
- **一時値**: 場所でない式（呼び出し、ブロック、`if` など）。
- **事実 `returns_contents(f, i)`**: 関数 `f` の完全な直接呼び出しの結果が、共有参照の仮引数 `i` から受け取る借用を、参照先が持っていた借用（A31 の `Part::Contents`）としてだけ持ち、参照そのものの借用も、参照先の部分（フィールド・要素）への新しい借用も持たない。設計の「事実 `returns_contents`」で求める。

### 2. 借用の規則

`x[k]` が `Index` のインスタンスに解決したとき（組み込みの 6 型でないとき）:

| 土台 `x` | インスタンス | 今 | 修正後 |
| --- | --- | --- | --- |
| どれでも | 値を返す | `BorrowOperand`。結果は `x` の値が持つ借用（region なしの局所なら場所の借用も） | 変えない |
| 場所 | 土台を借りる | `BorrowOperand`。借用を運ばない型の土台（`Tensor<i64>`・`Matrix<f64>`・利用者の region なしレコード）は場所の共有借用を持つ。借用を運ぶ型の土台（`View`・`Mut`・`MatrixView`・region 付きのレコード、要素が借用を運ぶ `Tensor<MatrixView<i64>>`）は場所の借用を持たない（`read_places` の `carries_loans` の枝） | **`Index.index (ref k) (ref x)` と同じ**: 場所 `x` の共有借用と、`x` の値が持つ借用を持つ。窓が生きている間の `x` の move・置換・排他借用は `E1014` |
| 一時値 | 土台を借り、`returns_contents($instance.index, 1)` が立つ（std の `Index<View, …, View>`・`Index<MatrixView, …, MatrixView>`） | 一時値が持つ借用 | 変えない（受理。一時値は呼び出しの直後に解放され、結果はその記憶域を指さない） |
| 一時値 | 土台を借り、事実が立たない（std の `Index<Tensor, …, View>`・`Index<Matrix, …, MatrixView>`、再現の利用者のインスタンス） | 借用なし（解放後に読む） | **`E1013`**（下の診断） |

- キーは今と同じく `BorrowOperand` で貸し、結果はキーの借用を持たない。
- `IndexMut` の対象は今と同じく排他借用（`Borrow(raw_target, true)`）。キーは `BorrowOperand`。
- 区別はインスタンスの本文の事実で決まり、std と利用者で同じ。利用者の region 付きの型でも、本文が参照先の中身だけを返すなら一時値の土台を許す。
- 場所の土台では、事実が立つインスタンス（std の `View`→`View`）でも場所の借用を取る（D2）。関数の形 `Tensor.slice axes (ref v)` と同じで、局所の `View` の窓が生きている間に `View` を move・置換すると `E1014` になる。HEAD のリポジトリにこの形のプログラムはない（S1）。

### 3. 診断

| 条件 | コード・kind | 位置 | メッセージ（英語） |
| --- | --- | --- | --- |
| 土台を借りる `Index` の土台が一時値で、事実が立たない | `E1013`・`borrow_of_temporary`（既存） | 土台の式 | `borrow requires a local place; bind the temporary with 'let' first`（既存の文言）。help を足す: `the result of 'x[k]' borrows 'x'; bind the value with 'let' first, as in 'let base = ...', and index 'base'` |
| 窓が生きている間に土台を move・置換・排他借用する、窓と `ref mut <土台>` を同じ呼び出しに渡す | `E1014`（既存の `access conflicts with a live borrow`） | 衝突したアクセス。note は `x[k]` の土台 | 既存のまま |
| 利用者のモジュールの `Index`・`IndexMut`・`Add`〜`Pow`・`Neg` のインスタンスで、振り分けの位置（Self と Key、または T）のどの頭も、このプログラムで宣言した型でない（D4） | `E1016`・新しい kind `orphan_instance`（NC19・NC20 と共有） | `instance` のクラス名 | `Index`・`IndexMut` は `{header} needs a type declared in this program as its self or key type`、算術は `{header} needs a type declared in this program as its operand type`（`{header}` は `Mul<Tensor<f64>>` のようなインスタンスの頭）。help: `types from the standard library and built-in types such as tuples, arrays and references belong to the language; wrap the value in a record declared in this program, or call a function of the type's module`（特定の std の関数を名指さない） |
| 利用者のモジュールのインスタンスと std のインスタンスが重なる（D7） | `E1016`・`overlapping_instance`（既存） | 利用者のインスタンスのクラス名（今は std の側の位置）。std のインスタンスを related（role `declared_here`）に付ける | 既存の文言（`overlapping instance for …`）。コード・kind・メッセージの形は変えない |

新しい診断コードは使わない（D5）。kind を足すのは互換の変更（`docs/stability.md`「診断の kind」）。

### 4. orphan 規則（D4。承認済み）

orphan 規則は A37・NC19・NC20 で 1 つの規則にし、A37 が入れる（手順 5）。

- 規則: 利用者のモジュール（`ModuleOrigin::User`。依存パッケージのモジュールを含む）が書く、対象のクラスのインスタンスでは、振り分けの位置の少なくとも 1 つの頭が、このプログラムで宣言した型でなければならない。違反は `E1016`、kind `orphan_instance`。位置はインスタンスのクラス名。
- このプログラムで宣言した型: 頭の型構築子が、利用者のモジュールで宣言したレコード、名前付きの共用体、`extern type` であるもの。型別名は展開してから頭を見る。
- このプログラムの型ではないもの（外の型）: std で宣言した型。組み込みの型（数値、`bool`、`char`、`unit`、`string`、`utf8string`、タプル、配列・リスト・`Vec`・固定長配列、関数、`FnPtr`、`Rc`・`Arc`、SIMD、匿名共用体）。言語の型構築子 `ref`・`dyn`（`ref Money`・`dyn Shapes.Shape` を含む）。型変数。頭だけを見るので、`Tensor<Money>` と `(Money * Money)` もこのプログラムの型ではない。
- 対象のクラス: `Index`・`IndexMut` と、今ある 1 引数の算術クラス `Add`・`Sub`・`Mul`・`Div`・`Rem`・`Pow`・`Neg`。NC19 が演算子の拡張インスタンス（OEI）の位置を、NC20 が `AddAssign` 系の位置を同じ表に足す。`Eq`・`Ord`・`Bits` など、ほかのクラスには当てない。std の型にほかのクラスのインスタンスを足す変更は、同じ変更でそのクラスを表に足すかを決める（std の `Tensor<bool>` に `Bits` を足す案は NC19 の D9 (i) で却下したので、今は `Bits` を足さない）。`IndexBorrowMut`（NC20）は利用者のインスタンスをすべて `builtin_instance_override` にするので、表に入れない。
- 振り分けの位置: `Index`・`IndexMut` は Self と Key。1 引数の算術 `C<T>` は T。Output（`IndexMut` の Value）は数えない。Output は (Self, Key) で決まり、インスタンスを選ばないからである。位置の表は 1 つの関数 `dispatch_positions` に置く。この関数は、クラス名と型引数の数から、ヘッダの型引数の添字を返す（`Index`・`IndexMut` は 0 と 1、1 引数の算術は 0）。コンパイラの検査と `src/stdlib.rs` のテストが共有する。
- 単位: プログラム（`Drop`・`Format` と同じ）。型を宣言したモジュールと、インスタンスを書くモジュールは違ってよい。パッケージ単位の規則は、`Drop`・`Format`・利用者のクラスを含めて別に決める。
- std のモジュール: コンパイラでは検査しない。std の中の重なりは、std のすべてのモジュールを読み込むテストで `overlapping_instance` として見つかる。opt-in のモジュールの置き場所は、テスト `opt_in_modules_are_reached_only_through_their_names`（`src/stdlib.rs:708`）が保つ。このテストは今 `instance.ty`（Self）の頭だけを見るので、`dispatch_positions` のすべての位置で、そのモジュールが宣言する頭を探すように直す。`std/MatrixView.tz:461` の `Index<Matrix<'a>, (Slice.Axis * Slice.Axis), MatrixView<'a>>` は、`PAIR_TRIGGERS`（`src/stdlib.rs:500`）で今のまま通る。
- 裸の型変数の位置: 新しいテスト `std_instances_with_open_dispatch_positions_are_listed`（`src/stdlib.rs`）を足す。対象のクラスの std のインスタンスのうち、振り分けの位置に裸の型変数を持つものを、固定の一覧と比べる。今の一覧は `Index<Map<'key, 'value>, 'key, 'value>`（`std/Map.tz:508`）と `Index<HashMap<'key, 'value>, 'key, 'value>`（`std/HashMap.tz:490`）の 2 つ。一覧に足すのは、利用者のインスタンスと重なりうる互換性のない変更で、承認とリリースノートの Compatibility を要する。NC19（スカラーの 24 個）・NC20（`AddAssign` 系の 10 個）・NC21（`Complex` の 8 個）は、それぞれの決定を承認として、同じ変更で自分の形を一覧に足す。
- 重なりの報告の位置: 利用者のインスタンスと std のインスタンスの重なりは、利用者の位置で報告する（D7）。
- 検査の順: 既存の `builtin_instance_override` と `anonymous_union_instance`（`E1041`。2 つの順は今のまま）、NC19 の `operator_instance_form`、`orphan_instance`、`overlapping_instance` の順。既存の条件の kind は移さない（`instance Add<i32>` と `instance Index<[i64], string, i64>` は `builtin_instance_override` のまま）。
- 適用: 全 edition（D6）。NC03・NC04 を含む最初のリリースの前に入れる（D8）。先にそのリリースが出ても規則は入れる。std の追加が同じプログラムを `overlapping_instance` で壊すからである。
- 文書: 規則の導入は、規則を含む最初のリリースのリリースノートの Compatibility と `type-classes.md` に書く。`docs/stability.md` の「既知の例外」には、続いて起きうる破壊として「std への instance の追加」の 1 項を書く（ドキュメントの節）。

### 5. edition・native と WASM・IR

- 全 edition（2026・2027）で同じ。不具合の修正なので edition を使わない（`docs/stability.md`「保証の単位」: バグの修正は同じ edition で行ってよい）。D4 の orphan 規則も全 edition で当てる（D6）。
- 変わるのは解析だけで、native と WASM の差はない。実行時の値・トラップ・解放の時点は変えない。
- IR: 値を返す `Index` は変えない。土台を借りる `Index` の場所の土台の呼び出しは、引数が `BorrowOperand(base)` から `Borrow(base, false)` に変わるが、どちらも `place` の番地を渡す（`src/llvm.rs:6183` の `Borrow` と 7501 行の `operand_borrow`）ので IR は変わらない見込み（S3）。事実が立つ一時値の土台は今と同じ `BorrowOperand`。

## 設計

### 段ごとの変更

| 段 | ファイル・関数 | 変更 |
| --- | --- | --- |
| 1 | `src/polymorph.rs` の `IndexInstanceTemplate`（892 行）・`ResolvedIndex`（903 行）・`register_index_instance`（2495 行）・`resolve_index`（2758 行） | テンプレートに `borrows_base: bool`（宣言の `output_ty.carries_loans(types)`。登録のときに 1 回求める）を持たせ、`ResolvedIndex` で返す。`IndexMut` は常に `false`。NC16 の `ref` の要否もこれを読む（判定は 1 つ。他チケットへ提供するインターフェース） |
| 2 | 同上と `src/check.rs` の `CheckedFunction`（5179 行付近）または `FunctionOrigin`（4892 行） | 土台を借りる `Index` の生成関数に印を付ける（例 `index_view: bool`）。特殊化で複製される関数にも引き継ぐ。所有権検査が呼び出しを見分けるのに使う。名前の接頭辞 `$instance.` では判定しない（ほかのインスタンスの生成関数も同じ接頭辞を持つ） |
| 3 | `src/check.rs` の `ExprKind::Index`（14748〜14837 行） | `resolved.borrows_base` が真で、土台が場所（`value.is_place()`）なら、土台の引数を `TypedExprKind::Borrow(Box::new(value), false)`（型は今と同じ `Type::Reference(val_ty, false)`）にする。土台が一時値なら今と同じ `BorrowOperand`。`borrows_base` が偽なら今と同じ。キーは変えない |
| 4 | `src/ownership.rs` の `CallFacts`（1336 行）・`FactsOutcome`（3796 行付近）・本文の検査（`share_returns` を求める 3895〜3933 行付近の隣） | 事実 `contents_returns: Vec<Option<Box<[bool]>>>` と問い合わせ `returns_contents(function, index)` を足す（`returns_share`・1430 行と同じ形）。下の「事実 `returns_contents`」 |
| 5 | `src/ownership.rs` の `eval_call` の引数の評価（14240〜14420 行付近） | callee が手順 2 の印を持つ関数で、引数 1 が場所でない `E::BorrowOperand` のとき、`returns_contents(id, 1)` が立てば今の扱い（`Use::Read`）、立たなければ `borrow_of_temporary(inner)`（`E1013`）に仕様 3 の help を付けて返す。位置は土台の式 |
| 6 | `std/Tensor.tz:1676`・`std/MatrixView.tz:468`（必要なときだけ） | 事実が立たなければ、本文を土台のフィールドを直接読む形に書き換える（`View`: `slice_raw (Array.copy axes) self.data self.offset (ref self.shape) (ref self.strides)`。`slice_raw` は 1588 行の非公開関数で `slice`（1657 行）と同じモジュールにある）。`MatrixView` は今の `deref self` を渡す形のまま確かめる。公開 API と結果は変えない |
| 7 | `src/polymorph.rs`（1 引数のクラスの登録 2172〜2222 行付近と重なりの検査 2235〜2268 行付近、`register_index_instance` 2495 行と重なりの検査 2574〜2614 行付近、`IndexInstanceTemplate` 892 行・`InstanceTemplate` 911 行）、`src/diagnostic_kinds.rs`（`E1016` の並び、474〜491 行。(code, id) の順なので `missing_explicit_constraint` と `overlapping_instance` の間。`tests/diagnostic_kinds.rs` 59 行の検査）、`src/stdlib.rs`（`opt_in_modules_are_reached_only_through_their_names` 708 行） | D4 の orphan 規則と D7 の重なりの報告の位置。(1) `dispatch_positions`（クラス名と型引数の数から、振り分けの位置の添字を返す）と `validate_orphan_instance(class, header: &[Type], types, names, instance_origin, span)` を置く（`validate_drop_instance` が手本）。std の起源なら何もしない。レコード・共用体の頭は `types.records[id].origin`・`types.unions[id].origin` で、`extern type` の頭（`Type::Handle` の修飾名、`src/check.rs:178`）は修飾名のモジュールの `names.origin` で判定する。`register_index_instance` と 1 引数のクラスの登録（`validate_drop_instance` の呼び出しの隣）から、既存の `builtin_instance_override`・`E1041` の検査の後に呼ぶ。(2) 2 つの重なりの検査は、重なった 2 つの一方が std、もう一方が利用者なら、どちらを先に登録していても、利用者のインスタンスのクラス名の位置で報告し、std のインスタンスを related（`RelatedRole::DeclaredHere`、`src/diagnostic.rs:415` の `related`）に付ける。2 つのテンプレートはモジュールの起源を持たない（HEAD で確認）ので、`origin: ModuleOrigin` を足す（内部の変更）。登録の順は変えない。(3) `opt_in_modules_are_reached_only_through_their_names` を `dispatch_positions` のすべての位置で判定するように直し、`std_instances_with_open_dispatch_positions_are_listed` を足す。kind `orphan_instance` と 2 つの関数は A37 が作る。NC19・NC20 は `dispatch_positions` に位置を足すだけ |

### 事実 `returns_contents`

- 意味: 仕様 1。結果が仮引数 `i` から得た loan を、すべて参照先の中身として持つ。
- 求め方: 本文の検査（`joint_facts` の 1 回の検査。`start_checker` が仮引数に外部の根 `usize::MAX - parameter.id` を置く）の終わりに、結果の `Value` の loan のうち、根が仮引数 `i` の外部の根に届くもの（`parents` をたどる。A34 の `share_returns` が `roots` で同じことをしている）が、すべて中身として読んだ loan なら真にする。中身として読んだ loan は 2 通りある。
  - region を名付けない参照を通して読んだ loan は `Value::through` に入る（`read_places` の外部の枝、12240〜12245 行付近。「A31: the reference's loans stand for what its target holds.」）。
  - 名前付きの region を持つ参照先（生成関数の `self: ref (Self {r})`）から読んだ loan は、その region の slot の `target`（12232〜12239 行付近の `slot.target`）として持つ。
  - 参照そのものの loan を値として持つ場合（`Tensor.freeze self` が `ref {r} Mut {r}` に参照を渡す）と、参照先の部分への新しい借用（`ref x.own_data[0..]`、`ref self.data`）は中身ではないので偽になる。
- 対象: 手順 2 の印を持つ関数だけ（費用を抑えるため）。region の契約を持つ関数は A28 の保持の事実（`parameter_retention`）の対象外なので（`docs/architecture.md` 1661 行付近）、この事実のために、印を持つ関数の本文を同じ `start_checker` で検査する。検査の失敗、予算の打ち切り、本文が見えない場合は偽（保守的）。
- 推論の抽象の本文（`infer`）では事実が空なので偽になる。抽象の検査で総称関数の中の連鎖が `E1013` にならないか、テスト計画の「総称関数の中の連鎖」で確かめる。なるなら、抽象の検査ではこの判定を行わず、特殊化した本文の検査だけで判定する。
- 期待する結果（手順 4 の確認）:

| インスタンス | 本文 | `returns_contents(·, 1)` |
| --- | --- | --- |
| std `Index<View<'a>, [Slice.Axis], View<'a>>` | `slice (Array.copy axes) self` | 真（必要なら手順 6 の書き換え） |
| std `Index<MatrixView<'a>, (Slice.Axis * Slice.Axis), MatrixView<'a>>` | `slice (ref r_axis) (ref c_axis) (deref self)` | 真 |
| std `Index<Tensor<'a>, [Slice.Axis], View<'a>>` | `slice_raw … (ref self.data) …` | 偽 |
| std `Index<Matrix<'a>, (Slice.Axis * Slice.Axis), MatrixView<'a>>` | `of_matrix self` の窓 | 偽 |
| 再現 2・3-f の `Index<Tensor.Mut<i64>, …, Tensor.View<i64>>` | `Tensor.freeze self` | 偽 |
| 再現 3-c の `Index<Grid, i64, ref string>` | `ref self.names[deref k]` | 偽 |
| 再現 3-d の `Index<R, i64, R>` | `R { own_data: [], b: ref x.own_data[0..] }` | 偽 |
| 再現 3-g の `Index<Grid, i64, (ref string * i64)>` | `(ref self.names[deref k], deref k)` | 偽 |
| 再現 3-h の `Index<Grid, i64, (unit -> i64)>` | `self` を捕捉したラムダ | 偽 |

### 生成 IR

- 値を返す `Index`: 変更なし。
- 土台を借りる `Index` の場所の土台: `Borrow(base, false)` は `self.place(value)`（`src/llvm.rs:6187`）、今の `BorrowOperand(base)` は `operand_borrow` の `self.place(operand)`（7517 行）で、同じ番地を渡す。手順 3 で IR を比べる。
- 土台を借りる `Index` の一時値の土台（事実が立つもの）: 変更なし（slot に置き、呼び出しの直後に解放する）。

## 前提とする他チケットのインターフェース

- [NC04](_completed/NC04-generic-index-class-and-slice.md)（完了）: 組み込みクラス `Index`・`IndexMut`、3 引数ヘッダ、`resolve_index`、生成関数 `index_instance_function`。
- [A31](_completed/A31-in-place-writes-keep-stored-borrows.md)（完了）: `CallFacts`、`Value::through`・`Part::Contents`、`joint_facts`。A34 の `share_returns`。

## 他チケットへ提供するインターフェース

- `ResolvedIndex::borrows_base`（土台を借りる `Index` かどうか）。[NC16](NC16-index-ref-and-zero-alloc-element-reads.md) は、これが真の `x[k]` に `ref` を要求し（`E1042`、kind `index_view_requires_ref`）、偽の `x[k]` に `ref` を付けると型の誤りにする。
- 土台を借りる `Index` の下げ方（場所は `Borrow(base, false)`、一時値は事実が立つときだけ `BorrowOperand`、ほかは `E1013`）。[NC18](NC18-multi-axis-bracket-indexing.md) の多成分のキー `t[i, a..b, ..]`（新しい AST ノード）はこの下げ方をそのまま使う。
- 事実 `returns_contents` と、土台を借りる `Index` の生成関数の印。NC18 が std に足す `Index<View, [Slice.Sel], View>` などの連鎖も、この事実で一時値の土台を許す。
- orphan 規則（D4・D7）: 検査関数 `validate_orphan_instance`、振り分けの位置の表 `dispatch_positions`、kind `orphan_instance`、裸の型変数の位置の一覧のテスト `std_instances_with_open_dispatch_positions_are_listed`、利用者と std の重なりの報告の位置。[NC19](NC19-operator-extension-instances.md) は OEI の位置（3 引数の L と R、`Neg<S, O>` の S）を、[NC20](NC20-compound-assignment-and-mutable-index-views.md) は `AddAssign` 系の Target と Rhs を `dispatch_positions` に足す。どちらもクラスと同じ変更で入れる自分の裸の型変数の形を一覧に足す（NC21 も同じ）。判定と std の扱いは変えない。NC19 の演算子表と NC20 の `AddAssign` 系の重なりの検査も、利用者と std の組を利用者の位置で報告する。`IndexBorrowMut` は NC20 の D3 のとおり std だけ（利用者のインスタンスは `builtin_instance_override`）。NC20 の書き込み窓は `IndexMut` と同じく `Borrow(base, true)` で下げる。
- 回帰テスト（`tests/index_class.rs` の再現の各形）。A37 のテストは `ref` なしで書く。NC16 が `ref` 付きへ移す。

## 実装手順

1. **変更**: なし（`target/scratch/a37/` に記録）。**内容**: 「再現」と「正しく扱われている形」を dev バイナリで確かめる。`tests/index_class.rs`・`tests/matrix_views.rs` の受理する例と、`tests/fixtures/tensor`・`tests/fixtures/matrix_view` を `tsuzuri build --emit llvm`（または `tests/index_class.rs` の `emits`）で IR にして保存する。**確認**: 「再現」の表と同じ出力（不定の値は違ってよい）。
2. **変更**: `tests/index_class.rs`。**内容**: テスト計画の Rust テストを足す。再現の各形の `rejects` は今は失敗する（受理される）。受理の各形の `emits` は通る。**確認**: `node scripts/slot.mjs -- cargo nextest run --locked --test index_class` で、足した `rejects` のテストだけが失敗する。
3. **変更**: `src/polymorph.rs`、`src/check.rs`（設計の段 1〜3）。**内容**: `borrows_base` と生成関数の印を足し、場所の土台を `Borrow(base, false)` に下げる。**確認**: T0。再現 2・3-e・3-f の `rejects(…, "E1014")` が通る。手順 1 の IR と `diff` して一致する（S3）。再現 1・3-a〜3-d・3-g・3-h はまだ受理される。
4. **変更**: `src/ownership.rs`（段 4・5）、必要なら `std/Tensor.tz`・`std/MatrixView.tz`（段 6）。**内容**: 事実 `returns_contents` を求め、一時値の土台の判定を入れる。設計の表の事実が立つことを、`tests/index_class.rs` の受理のテスト（連鎖）と拒否のテスト（再現 1・3-a〜3-d・3-g・3-h）で確かめる。std の 2 つで立たなければ段 6 の書き換えをし、それでも立たなければ S2。**確認**: T0、`tests/index_class.rs` の全件、`node scripts/slot.mjs -- cargo nextest run --locked --test matrix_views --test tensor --test tensor_ops --test types_ownership --test borrowed_records --test mutable_slices --test rc`、T1。
5. **変更**: `src/polymorph.rs`、`src/diagnostic_kinds.rs`、`src/stdlib.rs`（段 7）、`tests/index_class.rs`・`tests/polymorphism.rs`・`tests/visibility.rs`、`spec/rules/TYP.md`・`tests/conformance/TYP/`、`docs/stability.md`。**内容**: D4 の orphan 規則と kind、D7 の重なりの報告の位置、`src/stdlib.rs` の 2 つのテスト、TYP の規則（番号は実装の時点の TYP の次の空き）と適合性テスト、`docs/stability.md` の既知の例外の 1 項（ドキュメントの節）。`node scripts/gen-docs.mjs` で `diagnostics.md` の生成部分を更新する。手順 5 は別のコミットにする。PR の説明に、次のリリースノートの Compatibility の文案を置く（ドキュメントの節）。**確認**: T0。`node scripts/slot.mjs -- cargo test --locked --lib opt_in_modules_are_reached_only_through_their_names` と `node scripts/slot.mjs -- cargo test --locked --lib std_instances_with_open_dispatch_positions_are_listed`（どちらも `running 1 test` を確かめる）。`node scripts/slot.mjs -- cargo nextest run --locked --test polymorphism --test borrowed_arithmetic --test generic_records --test type_aliases --test visibility --test pure_functions --test control --test index_class --test kg --test api_diff --test diagnostic_kinds`。`node scripts/conformance.mjs target/debug/tsuzuri --rule <足した TYP の規則>`、`node scripts/check-spec.mjs --require-markers`、`node scripts/gen-docs.mjs --check`、`node scripts/check-cli-docs.mjs`。
6. **変更**: `tests/fixtures/tensor/Main.tz`・`tests/tensor-cases.mjs`・`tests/fixtures/matrix_view/Main.tz`・`tests/matrix-view-cases.mjs`、文書。**内容**: テスト計画の E2E と、ドキュメントの更新。**確認**: T2（下のテスト計画の E2E、`node scripts/check-docs.mjs <変更したページ>`）。

## テスト計画

### Rust テスト（`tests/index_class.rs`）

解析は native と WASM で共通なので、拒否のテストは `analyze` の 1 回で両方を覆う。受理のテストは `emits`（native と WASM の IR、決定性、宣言の重複なし）で確かめる。

- `index_view_on_a_temporary_base_is_rejected`: 再現 1、3-a（1 段目の一時値の `Tensor`）、3-b、3-c、3-d、3-g、3-h がそれぞれ `E1013`。メッセージが `borrow requires a local place` で始まり、help が `bind the value with 'let' first` を含む。位置が土台の式。
- `index_view_keeps_a_loan_of_its_base_place`: 再現 2、3-e、3-f がそれぞれ `E1014`。局所の `View` の窓（`let v = Tensor.view (ref t)`・`let s = v[[Slice.at 0]]`）が生きている間の `let moved = v` も `E1014`（D2。関数の形 `Tensor.slice [Slice.at 0] (ref v)` と同じ）。
- `std_view_chains_on_temporary_views_stay_accepted`（`emits`）: `t[[Slice.range 1 3]][[Slice.at 1]]`、`t[[Slice.range 1 3]][[Slice.at 0]]` と `t[[Slice.range 1 3]][[1, 2]]`、`m[(Slice.range 1 3, Slice.all())][(Slice.all(), Slice.at 2)]`、一時値からの要素の読み出し `(mk 100)[[2]]`・`(mm 100)[(1, 1)]`、`r: ref Tensor<i64>` の仮引数の `r[[Slice.at 0]]`。
- 総称関数の中の連鎖（`emits`。落とし穴の `infer`）: 次のプログラムが受理のまま（HEAD の `-O0`・`-O3` の出力は `[100, 101, 102] 103`）。`first_row` の 1 段目の土台は `deref v` で、修正後は `Borrow(deref v, false)` の再借用になる。同じ形の関数版 `fn first_row v = Tensor.slice [Slice.at 0] (ref (deref v))` は HEAD で受理される（確認済み）ので、宣言の region `r` と合う見込み。

  ```text
  def first_row { r } :: ref Tensor.View<'a> { r } -> Tensor.View<'a> { r }
  fn first_row v = v[[Slice.range 0 1]][[Slice.at 0]]

  def corner :: Copy<'a> => ref Tensor<'a> -> 'a
  fn corner t = t[[Slice.range 1 3]][[0, 0]]

  let t = Tensor.init [4, 3] (\i -> i + 100)
  let v = Tensor.view (ref t)
  let row = first_row (ref v)
  $"{Tensor.to_array (Tensor.to_tensor (ref row))} {corner (ref t)}"
  ```
- `index_instances_on_std_types_need_a_key_of_this_program`（手順 5）: 利用者の `instance Index<Tensor.Mut<i64>, [Slice.Axis], Tensor.View<i64>>`、`instance IndexMut<Tensor<i64>, i64, i64>`、`instance Index<(i64 * i64), i64, i64>` がそれぞれ `E1016`・kind `orphan_instance`（位置は `instance` のクラス名）。利用者の `record Key { i: i64 }` を Key にした `instance Copy<'a> => Index<Tensor<'a>, Key, 'a>` と、既存の `Grid` の各インスタンスは受理。`instance Index<[i64], string, i64>` は `builtin_instance_override` のまま。`record K { v: i64 }` と `instance Index<Map<K, i64>, K, i64>` は `overlapping_instance` のままで、位置が利用者のインスタンスのクラス名、related（role `declared_here`）が `std/Map.tz` の `Index` を指す（D7。HEAD では `std/Map.tz:508:39` に出る）。
- 既存のテスト（29・141・163・200 行の 4 つ）が変わらず通る。

### Rust テスト（D4 の算術クラスと単位。手順 5）

- `arithmetic_instances_need_an_operand_type_of_this_program`（`tests/polymorphism.rs`）: `E1016`・kind `orphan_instance` になるもの: `instance Mul<Tensor<f64>>`、`Add<Maybe<i64>>`、`Add<(i64 * i64)>`、`Add<[i32]>`、`Neg<i32u>`、`Add<bool>`、`Add<ref Point>`、`Add<dyn C>`（`C` は利用者のクラスで、`.tt` のファイルで宣言する）。`builtin_instance_override` のままのもの: `instance Add<i32>` と、`type Meters = f64` の `instance Add<Meters>`。受理するもの: 利用者の `Add<Point>`・`Add<Pair<i64, i64>>`、`extern type copy Buf` の `Add<Buf>` と `Index<Buf, i64, f64>`、`instance Eq<Maybe<'a>>` と `instance Eq<ref Point>`（表にないクラス）。HEAD の dev バイナリでは、`orphan_instance` にする 8 つの頭も受理するものの頭もすべて受理され、`Add<i32>`・`Add<Meters>` は今も `builtin_instance_override`（確認済み。`Eq` は `eq` と `ne` の両方を書く）。
- `orphan_rule_is_program_wide`（`tests/visibility.rs`。複数モジュールの `accepts`・`rejects` が 23・28 行にある）: モジュール A の `record Money` と、モジュール B の `instance Add<Money>` を受理する。
- `src/stdlib.rs` の 2 つのテスト: `opt_in_modules_are_reached_only_through_their_names` が `dispatch_positions` のすべての位置で判定する。`std_instances_with_open_dispatch_positions_are_listed` の一覧が `Map`・`HashMap` の `Index` の 2 つ（S6）。
- 適合性テスト: `tests/conformance/TYP/` に、D4 の TYP の規則（番号は実装の時点の次の空き）のテストを足す。

### E2E（`tests/features.mjs`、native と WASM の `-O0`／`-O3`、`live == 0`）

拒否はコード生成の前に決まるので Rust テストで覆う。E2E は、修正後も受理する一時値の土台（事実が立つ連鎖）と要素の読み出しが、値を正しく読み、一時値の `View` をちょうど 1 回解放することを確かめる。

- `tensor`: `tests/fixtures/tensor/Main.tz` に、`t[[Slice.range 1 3]][[Slice.at 1]]` の要素を `mix` でまとめる関数（期待 `[106, 107, 108]` 相当）と、一時値からの要素の読み出し（`(mk n)[[2]]`）の関数を足し、`tests/tensor-cases.mjs` に参照を足す（窓は添字の写像で表す既存のやり方）。
- `matrix_view`: `tests/fixtures/matrix_view/Main.tz` に `m[(Slice.range 1 3, Slice.all())][(Slice.all(), Slice.at 2)]` の関数（期待 `105`・`108` 相当）を足し、`tests/matrix-view-cases.mjs` に参照を足す。

### 既存テストへの影響

- 期待値の変更はない見込み（S1）。`tests/index_class.rs`・`tests/matrix_views.rs` の `Index` の例は局所の土台で、移した後も場所の借用を持つ。
- `tests/polymorphism.rs` 1060〜1062 行の `accepts("instance Add<[i32]> …")` を 2 つに分ける: 利用者のレコードで包んだ受理の例と、元の形の `rejects(…, "E1016")`（kind `orphan_instance`）（手順 5）。
- `tests/diagnostic_kinds.rs` が kind の並び（59 行）を検査するので、`orphan_instance` を `missing_explicit_constraint` と `overlapping_instance` の間に足す（手順 5）。

### 性能

実行時の振る舞いを変えない（解析だけ）。事実の検査は印を持つ関数の本文に限る。性能は主張しない。

### 検証コマンド

- T0: `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`node scripts/slot.mjs -- cargo nextest run --locked --test index_class --test matrix_views --test tensor`。
- T1: `node scripts/slot.mjs -- cargo nextest run --locked`。
- T2: T1、`cargo build`、`node tests/features.mjs target/debug/tsuzuri tensor`、同じく `matrix_view`・`matrix`・`borrowed_records`・`mutable_slices`・`in_place_writes`・`rc`（suite は 1 回に 1 つ）、`node tests/control.mjs target/debug/tsuzuri`、`TSUZURI_PROGFUZZ_KIND=owned node tests/progfuzz.mjs target/debug/tsuzuri`（README の「所有権・借用・region」の行）、`node scripts/check-docs.mjs <変更したページ>`、`node scripts/gen-docs.mjs --check`。手順 5 の分: 手順 5 の確認のコマンド（`src/stdlib.rs` の 2 つのテスト、`conformance.mjs`、`check-spec.mjs --require-markers`、`check-cli-docs.mjs`）。

## ドキュメント

### 修正までの注記（このチケットを起票する PR で置く）

`_tsuzuri/language-reference/built-in-types-and-modules/tensor.md` の「借用規則と診断」の表の後、`slice.md` の「使い方」の例の後、`types-and-type-inference/type-classes.md` の `Index` の段落（「`Format` も、このプログラムのレコードと共用体だけが実装できます」の後）に、次の趣旨の注記を置く（リンクは置くページからの相対パスなので、文案はコードブロックに入れる）。

```markdown
> 既知の不具合: 結果が土台を借りる添字（`t[[Slice.range 1 3]]`、`m[(Slice.range 0 2, Slice.all())]`、参照・関数・region を持つレコードなど借用を運ぶ型を返す利用者の `Index`）の土台に一時値を書くと、検査を通り、窓は解放した後のメモリを読みます。`(mk 100)[[Slice.range 1 3]]` ではなく、`let base = mk 100` と束縛してから `base[[Slice.range 1 3]]` と書いてください。また、`Tensor.Mut` など借用を持つ型に利用者が定義した `Index` の窓は、生きている間も土台への書き込みを止めません。修正は [A37](../../../_features/A37-index-base-place-borrow.md) です。
```

### 修正の PR で直すページ

- `tensor.md`「借用規則と診断」: 注記を消し、表に「一時値の土台から窓を取る（`(mk 100)[[Slice.range 1 3]]`）| `E1013`（`borrow requires a local place`）」と「窓が生きている間に、窓の土台の `View`・`Mut` を move・置換・排他借用する | `E1014`」を足す。窓の窓（`t[[Slice.range 1 3]][[Slice.at 1]]`）と一時値からの要素の読み出しは書けることを 1 文で書く。「この記事のポイント」13 行目の `Index` の説明は変えない。
- `slice.md`「使い方」: 注記を消し、添字の土台は局所の値か、窓の窓であることを 1 文で書く。
- `type-classes.md`: 注記を消し、組み込み型クラスの表の `Index` の行か直後の段落に「結果が土台を借りる `Index`（Output が参照・関数・region を持つレコードなど、借用を運ぶ型）の `x[k]` は、`Index.index (ref k) (ref x)` と同じく土台を借ります。土台が一時値なら、インスタンスが土台の中身だけを返すとき（std の `View`・`MatrixView` の窓の窓）を除いて `E1013` です」を書く。`Drop`・`Format` の段落（152・154 行）の後に、orphan 規則の段落を書く。対象のクラス（`Index`・`IndexMut`、`Add`〜`Pow`・`Neg`）、振り分けの位置（Self と Key、または T。出力は数えない）、このプログラムで宣言した型（レコード・名前付きの共用体・`extern type`）と外の型（std の型、組み込みの型、`ref`・`dyn`、型変数）、`E1016`（kind `orphan_instance`）、このプログラムのレコードで包む例（`record Prices { t: Tensor<f64> }` に `instance Mul<Prices>`）。規則の導入で、前のコンパイラが受理した std の型と組み込みの型への算術インスタンスが `E1016` になることも、このページに 1 文で書く（一度きりの変更）。
- `ownership-and-memory/borrowing.md` の一時値の段落（262 行付近。一時値は `ref T` の仮引数へ共有借用され、呼び出しが終わると解放される）: 土台を借りる添字の土台は、上の例外を除いて一時値にできないことを足し、`tensor.md` へリンクする。
- `compiler/diagnostics.md` の生成部分（`<!-- generated:diagnostic-kinds -->`）: D4 の kind。`node scripts/gen-docs.mjs` で作る（手で書かない）。
- `docs/architecture.md` の所有権検査の事実の段落（1661 行付近）: 土台を借りる `Index` の下げ方と、事実 `returns_contents` を足す。
- `docs/language.md`: 借用の規則については `Index` の記述がない（Tensor の節 3527 行付近、型クラスの節 730 行付近）ので、食い違いはない。D4 の orphan 規則の 1 段落を「多相関数と型クラス」（730 行）に足す。
- `docs/stability.md`「既知の例外」（119〜126 行）: 「**std への instance の追加**」の 1 項を足す（手順 5。NC19 の Q-std-instance-addition の決定と同じ項。NC06・NC19・NC21 は別の項を書かず、この項に例を足す）。書くのは次の 4 点。
  - 保つこと: orphan 規則により、振り分けの位置に利用者の型を持つ利用者のインスタンスは、std のインスタンスの追加と重ならない。
  - 例外 1: 振り分けの位置に裸の型変数を持つ std のインスタンスは、std の型を頭に書き、要素型などを利用者の型にした利用者のインスタンスと重なりうる。A37 の時点の例は `Map`・`HashMap` の `Index`（利用者の `Index<Map<K, i64>, K, i64>` と重なる）だけで、NC19（スカラーの形）・NC20（`AddAssign<Tensor<'a>, 'a>`）・NC21（`Complex` の形）が自分の例を足す。こうした std のインスタンスは固定の一覧のテストに載せ、その追加は承認を要する互換性のない変更として release notes で予告する。
  - 例外 2: orphan 規則の表にないクラス（`Eq`・`Ord`・`Bits` など）のインスタンスを std の型に足すと、同じ型への利用者のインスタンスと重なりうる。
  - 例外 3: std のインスタンスが増えると、候補が 2 つ以上になって曖昧さの誤り（`E1043`）になる式がありうる。`E1043` は HEAD にはなく、NC16 か NC19 が登録する。A37 がそれより先に入るなら、コードを書かずに「曖昧になる式」と書き、`E1043` を登録するチケットがコードを足す。
  - 規則の導入そのもの（利用者の算術インスタンスと `Index` の拒否）は一度きりの変更なので、この項には書かない。
- リリースノート: 規則を含む最初のリリースの Compatibility に、規則の導入を書く。拒否される形（std の型と組み込みの型への 1 引数の算術インスタンスと `Index`・`IndexMut`、`ref`・`dyn` の頭のインスタンス）と、このプログラムのレコードで包む書き換えを示す。手順 5 の PR の説明に文案を置く（0.x の先例: `vsc/CHANGELOG.md` の 0.1.4 の引数の順（**Breaking change:**）と 0.1.7 の `Task`・`Async` の削除（**Compatibility:**）は、リリースノートだけで知らせた）。
- `spec/rules/TYP.md`: D4 の規則を 1 行足す（番号は実装の時点の TYP の次の空き。HEAD の最後は `TYP-022`）。文書の参照先は `type-classes.md`。
- README: 書いてあること（特徴の一覧・検証手順）は変わらないので変更しない。
- `node scripts/check-docs.mjs _tsuzuri/language-reference/built-in-types-and-modules/tensor.md _tsuzuri/language-reference/built-in-types-and-modules/slice.md _tsuzuri/language-reference/types-and-type-inference/type-classes.md _tsuzuri/language-reference/ownership-and-memory/borrowing.md _tsuzuri/language-reference/compiler/diagnostics.md` を通す。`docs/stability.md` と `spec/rules/TYP.md` を変えたので、`node scripts/check-spec.mjs --require-markers` も通す。

## 受け入れ条件

- [ ] 再現 1、3-a、3-b、3-c、3-d、3-g、3-h が `E1013`（土台の式の位置、仕様 3 の help 付き）。
- [ ] 再現 2、3-e、3-f が `E1014`。
- [ ] 「正しく扱われている形」がすべて今と同じ出力・診断（局所の `View` の窓が生きている間の `View` の move だけは D2 のとおり `E1014`）。
- [ ] 値を返す `Index` の呼び出しと、土台を借りる `Index` の場所の土台の呼び出しの IR が、手順 1 の記録と一致する（native と WASM）。
- [ ] 事実 `returns_contents` が設計の表のとおりに立つ。std の窓の窓の連鎖が受理のまま。
- [ ] E2E の `tensor`・`matrix_view` の追加ケースが native と WASM の `-O0`／`-O3` で通り、`live == 0`。
- [ ] `Index`・`IndexMut` と 1 引数の算術クラスの orphan 規則のテスト（`index_instances_on_std_types_need_a_key_of_this_program`、`arithmetic_instances_need_an_operand_type_of_this_program`、`orphan_rule_is_program_wide`）と、D4 の TYP の規則の適合性テストが通る。
- [ ] `src/stdlib.rs` の `opt_in_modules_are_reached_only_through_their_names` と `std_instances_with_open_dispatch_positions_are_listed` が `dispatch_positions` を使い、どちらも `running 1 test` で通る。
- [ ] 利用者と std のインスタンスの重なりが、利用者のインスタンスの位置で報告され、std のインスタンスが related（`declared_here`）にある。
- [ ] `orphan_instance` が `diagnostics.md` の生成部分にある。`docs/stability.md` の既知の例外に「std への instance の追加」の 1 項がある。PR の説明にリリースノートの Compatibility の文案がある。
- [ ] 手順 5 が別のコミットになっている。NC03・NC04 を含む最初のリリースの前に入っている（D8。先にリリースが出ていれば、次のリリースノートの Compatibility に書いた）。
- [ ] T2 が通る。ドキュメントの注記が消え、修正後の規則が書かれている。

## 落とし穴

- `E1013` を `borrow_of_temporary`（`src/ownership.rs:7817`）の中で直そうとしない。`BorrowOperand` は `place()` を呼ばないので届かない（原因の最後の段落）。判定は `eval_call` の引数の評価に置く。
- 場所かどうかは `TypedExpr::is_place`（`src/check.rs:5602`）で決める。所有権検査の `place()`（`src/ownership.rs:7706`）は `E::Static` も場所とするが、`is_place` は含めない。静的データの土台は一時値として扱われ、事実が立たなければ `E1013` になる。`Index` のインスタンスを持つ型の値が静的データ（`const`）になりうるかを手順 1 で確かめ、なりうるなら `E::Static` も場所として `Borrow` に下げる（`llvm.rs` の `emit_place` は `Static` を扱う。6646〜6660 行付近）。
- 土台は `autoderef` を通る（`src/check.rs:14749`）。`r: ref mut Tensor.Mut<i64>` の `r[[…]]` の土台は `deref r` で、`Borrow(deref r, false)` は共有の再借用になる。`r` を同じ式の中で排他に使わなければ衝突しない。
- `settle_temporary_borrows`（`src/constants.rs:415`）と `temporary_borrows` は、場所でない `Borrow` だけを `BorrowOperand` に変える。手順 3 で作る `Borrow` は場所だけなので変えられない。一時値の土台を `Borrow` にすると、ここで `BorrowOperand` に変わらずに所有権検査が `E1013` を出し、事実の例外が効かない。一時値は `BorrowOperand` のまま作る。
- 事実は保守的に求める。分からなければ偽にし、一時値の土台を `E1013` にする。一時値を受理するのは、呼び出しの直後に解放されても結果が困らないと証明できたときだけ（コード生成の `release_operands`）。
- 生成関数の本文の検査は、Self の region の下で参照先の部分の借用を返すことを受理する（再現 3-d）。本文の側で拒否して直そうとしない（安全なインスタンス、たとえば std の `Index<Tensor, …, View>` も参照先の部分を借りて返す）。直すのは呼び出し側の借用。
- 推論の抽象の本文（`infer`）では事実が空になる。総称関数の中の連鎖のテストを必ず足す。
- キーは変えない。キーを `Borrow` にすると、一時値のキー（NC04 の `t[[Slice.range 1 5, Slice.rev()]]`）が `E1013` になる。
- std のモジュールはコンパイラで検査しない。`std/MatrixView.tz:461` の `Index<Matrix, …, MatrixView>` と、NC18 が足す `Index<Matrix, (Slice.Sel * Slice.Sel), MatrixView>` は、`src/stdlib.rs` の `PAIR_TRIGGERS` で保つ。std にも規則を当てると、ここが `E1016` になる。
- 既存の kind を `orphan_instance` に移さない。`instance Add<i32>`・`type Meters = f64` の `instance Add<Meters>`・`instance Index<[i64], string, i64>` は `builtin_instance_override` のまま、匿名共用体の頭は `E1041` のまま。検査の順（仕様 4）を守る。
- `Drop`・`Format` の「レコード・共用体だけ」を写さない。利用者の `extern type`（`Type::Handle`）もこのプログラムの型に数える。HEAD で受理している `extern type copy Buf` の `instance Index<Buf, i64, f64>` を拒否しない。逆に、`ref Point`・`dyn C`・`Tensor<Money>`・`(Money * Money)` は頭が外の型なので拒否する。

## 対象外

- 結果が土台を借りる添字への `ref` の必須化（NC16）。多次元の角括弧 `t[i, a..b, ..]`（NC18）。書き込み窓 `ref mut t[…]` と `IndexBorrowMut`、複合代入（NC20）。演算子の拡張インスタンスと、その振り分けの位置（NC19）。
- 添字の評価順の統一（[A38](A38-index-evaluation-order.md)。D9）。
- 関数の形の一時値の規則（`temporary_borrows`）を広げること。`Tensor.slice [Slice.at 1] (ref (Tensor.view (ref t)))` は `E1013` のまま（D3）。
- 一時値の寿命を文やブロックの終わりまで延ばすこと（D3）。
- Key が region を持つ型のときの規則の見直し（NC04 の規則のまま）。
- `Eq`・`Ord`・`Bits` など D4 に挙げないクラスの orphan 規則。`AddAssign` 系の位置（NC20）。パッケージ単位の規則（`Drop`・`Format`・利用者のクラスを含めて別に決める。GUIDE D-66 の残る問い）。

## 決定事項

### D1: 土台を借りるかどうかは、宣言の Output が借用を運ぶかで決める（既定案）

- 決定: 宣言の Output が借用を運ぶ（`Type::carries_loans`。型変数は数えない）インスタンスを土台を借りる `Index` とし、修正はそれだけに当てる。値を返す `Index` は `BorrowOperand` のまま。
- 理由: 生成関数が region `r` を持つか（`output_needs_region`）では足りない。region を持たない Output でも、参照を含むタプルや関数は本文が土台の借用を入れて返せ、一時値の土台で解放後のメモリを読む（再現 3-g・3-h）。一方、宣言の型変数の Output は、総称の本文が土台の借用をその型で作れないので、運ぶ借用は土台の参照先の中身に限られる（`BorrowOperand` が渡す一時値の loan で足りる）。要素の読み出しの IR と、一時値からの要素の読み出し（`(mk 100)[[2]]`）を変えずに済む。ふつうの呼び出しの規則（結果の型を具体化してから `loan_free` を見る）よりも、要素の読み出しに対して寛容になるが、上の理由で安全である。
- NC16 との関係（2026-11-25 の統合で揃えた）: [NC16](NC16-index-ref-and-zero-alloc-element-reads.md) の `ref` の要否（D2）も、この判定の結果 `ResolvedIndex::borrows_base` をそのまま読む。判定は 1 つだけで、`ref` の要否・土台の下げ方・一時値の土台の扱いが食い違わない。関数や参照を含むタプルを返す `Index` も `ref` が要る（`ref f[1]`）。
- 状態: 既定案（実装者はこの案に従う）。

### D2: 場所の土台は、事実にかかわらず場所の共有借用を取る（既定案）

- 決定: 土台を借りる `Index` の場所の土台は常に `Borrow(base, false)` にし、`Index.index (ref k) (ref x)` と同じ借用にする。事実 `returns_contents` は一時値の土台の判定にだけ使う。
- 理由: 関数の形（`Tensor.slice axes (ref v)`、`fr (ref m)`）と同じ規則で、`ref` が土台を借りるという NC16 の読み方とも合う。下げ方が check.rs で決まり、所有権検査に特別な場合を足さずに済む。代わりに、事実が立つインスタンスでは場所の借用を省く案（局所の `View` の窓が生きている間の `View` の move を受理し続ける）は、所有権検査の側で場所の借用を取るか決めることになり、`E::BorrowOperand` の特別な扱い（`Arg::temporary` など）と絡む。NC04 はリリース前で、HEAD のリポジトリにこの形のプログラムはない。
- 状態: 既定案。

### D3: 一時値の土台は、本文の事実で許す（既定案）

- 決定: 一時値の土台は、インスタンスの本文が結果に土台の中身だけを渡すと証明できたとき（`returns_contents`）だけ受理し、ほかは `E1013`。一時値の寿命は延ばさず、関数の形の一時値の規則も変えない。
- 理由: 窓の窓（`t[[…]][[Slice…]]`、`m[(…)][(…)]`）は NC04 から書ける形で、NC18 の `ref t[1..][.., 0]` の基本の形でもある。std の `View`→`View` は `slice` が shape と strides を複製し、データの参照だけを写すので、一時値の `View` が呼び出しの直後に解放されても安全（「正しく扱われている形」の表）。一律に `E1013` にするとこれを壊す。一時値の寿命を延ばす案は、解放の時点がほかの一時値と分かれ、`Drop` の順序の規則（A31）を変える。添字の形が関数の形より寛容になるが、受理するのは証明できた場合だけである。
- 状態: 既定案。

### D4: orphan 規則（承認済み。D-66 の委任で決定）

- 決定: 仕様 4。orphan 規則は A37・[NC19](NC19-operator-extension-instances.md)（D2）・[NC20](NC20-compound-assignment-and-mutable-index-views.md)（D4）で 1 つの規則にし、A37 が手順 5 で入れる。利用者のモジュール（依存パッケージを含む）の `Index`・`IndexMut` と 1 引数の算術クラス（`Add`〜`Pow`・`Neg`）のインスタンスでは、振り分けの位置（Self と Key、または T）の少なくとも 1 つの頭が、このプログラムで宣言した型（レコード・名前付きの共用体・`extern type`）でなければならない。std の型、組み込みの型、`ref`・`dyn`、型変数は外の型で、出力は数えない。単位はプログラム。std のモジュールはコンパイラで検査せず、`src/stdlib.rs` のテスト（`dispatch_positions` を使う置き場所のテストと、裸の型変数の位置の一覧のテスト `std_instances_with_open_dispatch_positions_are_listed`）で保つ。違反は `E1016`、kind `orphan_instance`。全 edition で当てる。NC19 は OEI の位置を、NC20 は `AddAssign` 系の位置を、同じ `dispatch_positions` に足す。
- 理由:
  1. std の追加で利用者のソースが壊れなくなる。NC06・NC16・NC18・NC19・NC20・NC21 は、std の `Tensor`・`View`・`Mut`・`Matrix`・`Complex` に多数のインスタンスを足す。規則がないと、利用者が先に書いた同じ頭のインスタンスは、std の追加のたびに `overlapping_instance` で壊れ、誤りは std のファイルの位置に出る（現状 5）。HEAD では利用者の `instance Mul<Tensor<f64>>` が `t * t` の意味を決めており、NC06 の `Mul<Tensor<'a>>` と必ず重なる。std のコードは利用者の宣言を見られない（`Names::tiers`、`src/check.rs:6653`）。そのため、振り分けの位置に利用者の型を 1 つでも持つインスタンスは、std のどのインスタンスとも重ならない。例外は、std のインスタンスがその位置に裸の型変数を持つ場合だけである。
  2. std の型と組み込みの型の添字と演算子の意味が、どのプログラムでも同じになる。インスタンスは `private` にできず（`E1022`、`tests/visibility.rs` 172〜180 行）、プログラム全体に効く。今は、言語が拒否すると決めた演算も差し替えられる（`instance Neg<i32u>` で `-x` が `99`）。
  3. 「このプログラムの型」を、std が名指せない型と一致させる。利用者のモジュールで宣言した `extern type` は、レコード・共用体と同じく std のインスタンスと重ならない。`Drop`・`Format` がレコード・共用体に限るのは、クラスに固有の理由による（`Format` は補間の穴がレコードと共用体だけを渡すから。`Drop` はレコードと共用体のすべての具体化を覆う規則だから。`src/polymorph.rs` 3968〜3970・3908〜3911 行のコメント）。HEAD で受理している `extern type copy Buf` の `instance Index<Buf, i64, f64>` を拒否しても、得るものがない。
  4. `ref`・`dyn` は言語の型構築子なので外の型に数える。参照の被演算子は [A40](A40-reference-operands.md) で扱いが変わるので、利用者の `Add<ref Money>` がその変更とぶつからないようにする。後で緩めるのは受理を増やす互換の変更なので、厳しい側から始める。
  5. 組み込みの型も外の型に数える。持ち主は言語で、言語がインスタンスを足すと利用者を壊す。依存パッケージどうしが同じ組み込みの型に別々の意味を与えると、組み合わせた時点で衝突する。
  6. 規則が 1 つで、先例と合う。単位は `Drop`・`Format` と同じプログラムである。Self と Key を見て出力を数えないのは、Rust の orphan 規則（関連型を数えない）と同じ形である（リポジトリの外の知識）。
  7. std は埋め込みのソースなので、テストで全部を覆える。規則が保証しない唯一の穴（裸の型変数の位置）の追加は、レビューに頼らず固定の一覧のテストで止める。
- 互換と edition: D6。規則の導入は一度きりの変更として、規則を含む最初のリリースのリリースノートの Compatibility と `type-classes.md` に書く。`docs/stability.md` の「既知の例外」には、続いて起きうる破壊として「std への instance の追加」の 1 項を書く（ドキュメントの節。NC19 の Q-std-instance-addition の決定と同じ項）。
- 採らなかった案: (a) `Index`/`IndexMut` だけに当てる（算術の穴が NC19 まで残り、NC06 の追加で壊れる）、(b) std のレコード・共用体だけを外の型にする（言語の追加で壊れ、`Neg<i32u>` が通る）、(c) 規則を入れない、(d) 警告だけ（警告は保証の対象外、`docs/stability.md` 46 行）、(e) edition 2028 からだけ（std は全 edition で 1 つのコピー、`docs/stability.md` 67 行）、(a') レコード・名前付きの共用体だけを数える（`extern type` のインスタンスを理由なく拒否する）。
- 状態: 承認済み（D-66 の委任で決定、2026-11-25）。

### D5: 新しい診断コードを使わない（既定案）

- 決定: 一時値の土台は既存の `E1013`（`borrow_of_temporary`）に help を足し、場所の借用の衝突は既存の `E1014`、orphan 規則は `E1016` の新しい kind `orphan_instance`（NC19・NC20 と共有）。
- 理由: どれも既存のコードの意味（`E1013` 借用が場所を要る、`E1014` 生きている借用との衝突、`E1016` インスタンスの宣言の誤り。`drop_instance_target`・`format_instance_target` と同じ種類）に収まる。kind の名前は、NC19（演算子）・NC20（複合代入）と 1 つの kind を共有するため、クラスの名前を含まない `orphan_instance` にした（2026-11-25 の統合で `index_instance_target` から改めた）。kind を足すのは互換の変更。D-55 の予約を使わない。
- 状態: 既定案。

### D6: edition を使わない（既定案）

- 決定: D1〜D3 は不具合の修正として、D4 はリリース前の NC04 の修正と、1 引数の算術の一度きりの変更として、全 edition で適用する。
- 理由: `docs/stability.md`「保証の単位」はバグの修正を同じ edition で許す。NC04 はどのリリースにも入っていない（D-66d と同じ。NC04 の `bd994606` も NC03 の `5d107cb5` も `vsc-v0.1.9` の祖先でない）。1 引数の算術インスタンスは `vsc-v*` のコンパイラが受理していたが、同じ次のリリースで NC03 がその引数を `ref` に変える。`vsc-v0.1.9` の `tests/polymorphism.rs` が受理する `instance Add<[i32]> { fn add x y = x }` は、すでに書き換えが要る。同じリリースに入れると、利用者の移行点が 1 つになる。0.x の一度きりの変更には先例がある（`vsc/CHANGELOG.md` の 0.1.4・0.1.7）。std は全 edition で 1 つのコピーなので（`docs/stability.md` 67 行）、edition で分けて旧 edition に孤児のインスタンスを残すと、D4 の理由 1 の保証が永久に成り立たない。
- 残る問い: `docs/stability.md` 7 行（同じ edition の中では受理していたプログラムを拒否しない）と、0.x のリリースの一度きりの変更の扱いを書く文は、GUIDE D-66 の残る問いとして今は決めない。
- 状態: 既定案。D4 の部分は承認済み（D-66 の委任で決定、2026-11-25）。

### D7: 利用者と std のインスタンスの重なりは、利用者の位置で報告する（承認済み。D-66 の委任で決定）

- 決定: 重なった 2 つのインスタンスの一方が std、もう一方が利用者のモジュールのものなら、`overlapping_instance`（`E1016`）を利用者のインスタンスのクラス名の位置で報告し、std のインスタンスを related（role `declared_here`）に付ける。コード・kind・メッセージの形は変えない。登録の順も変えない。1 引数のクラスの重なりの検査と `register_index_instance` の検査に入れ（段 7）、NC19 の演算子表と NC20 の `AddAssign` 系の検査も同じ扱いにする。インスタンスの記録がモジュールの起源を持たないので、A37 が足す（内部の変更）。
- 理由: orphan 規則の後も、std の裸の型変数の位置と重なる利用者のインスタンスは残る（`Index<Map<K, i64>, K, i64>`、NC19 の `Mul<Tensor<Money>, Money, Tensor<Money>>`、NC20 の `AddAssign<Tensor<Money>, Money>`）。今はその誤りが std のファイルの位置に出るので（現状 5）、利用者は何を直すかが分からない。報告の位置だけを変えれば、生成物は変わらない。role `declared_here` は既存の語彙で、関連する位置は増えてよい（`docs/stability.md` 40 行）。
- 採らなかった案: 今のまま（std のファイルの位置）、std のインスタンスを先に登録する（インスタンスの並びと生成物の順が変わりうる）。
- 状態: 承認済み（D-66 の委任で決定、2026-11-25）。

### D8: NC03・NC04 を含む最初のリリースの前に入れる（決定）

- 決定: A37（手順 5 を含む）、[A38](A38-index-evaluation-order.md)、[NC16](NC16-index-ref-and-zero-alloc-element-reads.md) の 3 つを、NC03・NC04 を含む最初のリリースの前に入れる。推奨順は A37 → A38 → NC16（NC16 の手順 1〜3 は A37 と並行してよい）。先にそのリリースが出ても A37 の手順 5 は入れ、次のリリースノートの Compatibility に書く。
- リリースの数え方（NC17 の未決の質問 3 の決定。利用者向けの定義は NC17 の Phase 2 が `docs/stability.md` の「リリースの数え方」に書く）: リリースは、コンパイラを利用者に配布する版である。今は、コンパイラを同梱する VS Code 拡張の版（タグ `vsc-v*`）だけ。同じコンパイラを複数の経路で配るときは 1 回と数える。前に数えたリリースからコンパイラが変わっていない版は数えない。変わったかどうかは、`git diff --name-only <前のタグ> <今のタグ> -- src std guide Cargo.toml Cargo.lock` が空かどうかで決める。コンパイラが前の版と同じ版は、NC03・NC04 を初めて含むリリースにならない。
- 理由: リリースの後に入れると、NC04 の `Index` の孤児のインスタンスと、A38・NC16 が拒否する形が、リリース済みのコンパイラの受理になり、それを拒否する変更が増える。リリースの前に入れれば、1 引数の算術インスタンスの拒否は NC03 がその引数を `ref` に変えるのと同じリリースに入り、利用者から見た破壊は 1 回で済む。同じリリースの数え方を使うので、コンパイラの変わらない拡張だけの版が条件をすり抜けることもない。
- 状態: 決定（D-66 の委任で決定、2026-11-25）。

### D9: 添字の評価順は A37 では変えない（決定）

- 決定: GUIDE D-66 の未決の問い (1)（添字の評価順を A37 か別のチケットで揃えるか）は、新しいチケット [A38](A38-index-evaluation-order.md) で揃えることに決まった。A37 は評価順を変えず、順を固定するテストも足さない。A38 は A37 の後、NC16 の前に入る。
- 理由: A37 はメモリ安全の修正で、値を返す `Index` の IR を変えないことを停止条件 S3 にしている。評価順は別の意味の変更である。
- 状態: 決定（D-66 の委任で決定、2026-11-25）。
