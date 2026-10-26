# A22: edition 2027 の正準形（`..<`／`..=`・接尾辞・借用の表記・旧ラムダ）と移行

| 項目 | 内容 |
| --- | --- |
| ID | A22 |
| 優先度 | P1 |
| 規模 | L（全体）。Phase 1 = S、Phase 2 = M、Phase 3 = M、Phase 4 = S、Phase 5 = M |
| 依存 | **Phase 1・2・4: なし**。Phase 3 の fix の付与（手順 12）は [G22](_completed/G22-diagnostic-v2.md) Phase 1（`Diagnostic::fix`・`with_kind`・`with_help`）、Phase 5 の統合試験（手順 19）は [G24](G24-tsuzuri-fix.md) Phase 3 が要る。未実装ならその手順だけを飛ばして残りを出荷できる（「前提とする他チケットのインターフェース」）。[A18](_completed/A18-parameter-conventions.md) とは edition `2027` の足場（`Edition::E2027`・`SUPPORTED`・`docs/stability.md` の表・`tests/editions.rs`）だけを共有する（先に実装した方が足す。D-54） |
| 後続 | G24 Phase 3（`fix::canonical_warnings` の本体を本チケットの `canonical::warnings` に差し替える）、G21（ガイドは正準形だけを教える）、A18（同じ edition `2027` に差を足す） |
| 状態 | todo |
| 起票 | 2026-10-18（D-50。HEAD `a2313d19` で確認。作業ツリーの `9dd0fbbc` との差は `docs/benchmarks.md` だけ） |
| 承認 | 承認済み（D-50: 人間の包括許可 2026-10-18）。決定事項 D1〜D14 はすべて承認済み |
| 改善する劣位 | D-50 の調査結論 (3)「同じ意味に複数の書き方があり、同じ記号の意味が文脈で違う」。`..` は `for` で両端を含みスライスで半開、`&r` と `ref r` は別の意味、接尾辞は 1 文字の大小で幅が変わる（`l`=`i64`、`L`=`i128`）。AI も人間も書き方を 1 つに絞れない。Rust・Swift・Mojo は借用の書き方が 1 つで、範囲は端の扱いを記号が示す（`..`／`..=`） |
| 手本にする既存実装 | edition: `src/syntax.rs` の `Edition`、`docs/stability.md`「edition を追加するとき」、`tests/editions.rs`。表記を AST に残す: `Notation`。範囲の下げ方: `src/llvm_control.rs` の `range_loop`、`src/ranges.rs` の `loop_kind`。警告の運び方: `src/copies.rs` の `warnings`、`src/main.rs` の `--warn`。符号化: `src/syntax_codec.rs`、`src/frontend_cache.rs` の `FRONTEND_FORMAT`。整形の安全検査: `src/formatter.rs` の `canonicalize` |
| 主な影響ファイル | Phase ごとのファイルは各手順の「変更」。新規: `src/canonical.rs`・`src/parse_canonical.rs`・`tests/range_forms.rs`・`tests/canonical_forms.rs`・`tests/edition_2027.rs`・`tests/fixtures/range_forms/`・`tests/edition_2027.mjs`（G24 後）。文書: 「ドキュメント」 |

## 実装者向け早見

- **読む順**: 「決定表」（仕様の最初の表）→ 「設計」の「データ構造」→ 自分の Phase の手順。コードは `src/syntax.rs`（`Edition`・`ExprKind::Range`・`Notation`）、`src/parser.rs`・`src/parse_control.rs`（`range_expression`・`prefix`・`lambda`・`literal`・`program_all`）、`src/llvm_control.rs` の `range_loop` の順に読む。
- **最初に書くテスト**: Phase 1 は `tests/editions.rs::edition_2027_is_supported_and_changes_no_generated_code`、Phase 2 は `tests/range_forms.rs::half_open_and_closed_ranges_are_accepted_in_every_edition`、Phase 3 は `tests/canonical_forms.rs::every_decision_row_is_reported`、Phase 4 は `tests/edition_2027.rs::edition_2027_rejects_every_decision_row`。
- **変えない**: `Edition::LATEST`・`MANIFEST_DEFAULT`・std の `.tz`（`2026` のまま）、`EDITION_KEYWORDS`（空のまま）、`2026` のソースの受理・拒否・診断・IR（`..<`・`..=` を使わない限り 1 byte も）、閉区間の `for` の IR（`range_loop` の分割は IR を変えない切り出し）、既存の診断コードの意味。
- **作る新しい名前**: `Edition::{E2027, NEW_PROJECT, rejects_noncanonical}`、`TokenKind::{DotDotEqual, DotDotLess}`、`RangeOp`、`ForRange.exclusive`、`Noncanonical`・`FixLevel`・`TextEdit`、`Program.noncanonical`、`CheckedModule.noncanonical`、`src/canonical.rs`・`src/parse_canonical.rs`、`lexer::numeric_suffix`、`W1007`・`E0004`（12 個の kind）、`--warn canonical`・`--edition`。
- **Phase の完了**: 「受け入れ条件」のとおり。コーパスの IR が 1 byte も変わらないことが全 Phase の共通の合格線。
- **edition 2027 の凍結**: `docs/stability.md` は「リリースに含めた edition は凍結し、差を足しません」と定める。**Phase 1〜4 を同じリリースに入れる**（Phase 4 が入るまで 2027 を含むリリースを切らない。切る前に `git tag --list` で確かめる）。
- **迷ったら**: 「決定事項」→ 最も保守的な選択（`fix` を付けない、`likely` に下げる、拒否する）。診断の文言と kind は「診断」の表のとおりに書く。

## 目的

HEAD には同じ意味の書き方が複数あり、同じ記号の意味が文脈で違う（実出力は「現状」）: `for` の `..` は両端を含みスライスの `..` は半開、短い接尾辞は 1 文字の大小（`l`／`L`、`f`／`F`、`m`／`M`）で型の幅が変わる、`&x`／`ref x`・`*r`／`deref r`・`&T`／`ref T` が共存し `&r` と `ref r` は別の意味、`x -> e` と `\x -> e` が共存し `x -> y -> e` は `\x -> \y -> e` と IR が違う。`..<`・`..=` は字句にもない。

本チケットは、edition `2027` を作り、**非推奨にした表記を拒否する**ことだけを変える（意味は変えない。D-54）。正準形は**全 edition が受理する**。

1. **全 edition で正準形を受理する**（Phase 2）: `..=`（閉）と `..<`（半開）を `for` の範囲とスライスで受理する。`2026` の既存のソースの IR は 1 byte も変わらない。
2. **`2026` で非正準な表記を警告する**（Phase 3）: 警告 `W1007`（既定は無効。`--warn canonical` で有効）。診断 v2 の `fixes` に `certain` の書き換えを付ける。
3. **`2027` で拒否する**（Phase 4）: 同じ 6 種類の表記を `E0004` で拒否する。
4. **移行の道具**（Phase 5）: `tsuzuri fix --edition 2027`（G24）が `W1007` の `certain` の fix をすべて当てる。本チケットは、その fix が**意味を保つ**ことを、コーパスの IR の比較で確かめる。`tsuzuri new` は `edition = "2027"` のマニフェストを書く。

HEAD で測った規模（コマンドは「付録 C」）: 非正準な表記は std に 82 行の `for` の素の `..`・16 行の記号の借用・3 行の旧ラムダ、`tests/fixtures` に 146・204・164 行ある。std は `Edition::LATEST`（`2026`）で読むので、本チケットは std を書き換えない。非目標は「対象外」の節に書く。

## 着手条件と停止条件

### 着手条件

- 承認は済んでいる（D-50）。GUIDE §2.3 の基準（`cargo build --release --locked && cargo test --locked`）を取り、件数を控える。
- 読む: `docs/stability.md` の「edition」と「edition を追加するとき」、`tests/editions.rs`、[A18](_completed/A18-parameter-conventions.md) の「edition・manifest・cache」、[G24](G24-tsuzuri-fix.md) の「Phase 3: edition の移行」と「A22 との接点」、GUIDE の D-45・D-51・D-54・D-55。
- `grep -n "E2027" src/syntax.rs` が 0 件なら手順 2 を行う。1 件以上なら A18 が足し済みなので、足場は再実装せず手順 2 の「確認」だけを行う。
- 手順 1 のとおり、変更前のコンパイラとコーパスの IR を残す。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

1. `..<`・`..=` の字句の追加で、既存のコーパス（`tests/fixtures`・`examples`・`std`）のどのプログラムの受理・診断・IR も変わった。選択肢: A 新トークンにする条件を絞る、B 戻して報告する。
2. 閉区間の `for` の IR が 1 byte でも変わった（`range_loop` の切り出しで）。選択肢: A 切り出しをやり直す、B 報告する。
3. `certain` の fix を当てたコーパスのソースの IR が元と違った。選択肢: A その表記の fix を `likely` に下げて決定表を直す（先に報告する）、B 検出の規則を直す。**黙って期待値を直さない**。
4. 半開の `for i in 0 ..< xs.length` が `loop_kind` で `BelowLength` にならず、`for i in 0 .. xs.length - 1` より多くの境界検査が残った。選択肢: A `loop_kind` の半開の分岐の条件を直す、B 記録して続ける（性能の主張をしない）。
5. W1007 と E0004 に同じ kind を割り当てたくなった（D-51 は「1 つの kind は 1 つの code に属す」。D8）。
6. edition `2027` を含むリリースが Phase 4 より前に切られそうになった（凍結の規則）。選択肢: A Phase 4 まで `SUPPORTED` に足さない、B 報告する。
7. std・fixture・文書の例を書き換えないと通らない（`LATEST` は `2026` のはず）。
8. 2 MiB のテストの stack が溢れた（GUIDE §3.1）。`Parser` に足す記録の関数は `#[inline(never)]` にする。
9. G22 の `Diagnostic` の型が D-51 と違い、`fix`・`with_kind`・`with_help` が使えない。選択肢: A 手順 12 を飛ばす、B 報告する。
10. 新しい予約語・crate・`unsafe`・既定の WASM の import が必要に見えた。
11. 同じ失敗を直す試みが 2 回続けて失敗した。

## 現状（HEAD `a2313d19` で確認）

実測の全記録（ソース・出力・IR の比較）は [付録 A](A22-appendix.md#付録-a-現状の実測本文現状の全記録)。ここには要点だけを書く（すべて HEAD の `target/release/tsuzuri` の実出力。1 ファイル 1 プログラムを `tsuzuri script` で実行した。`check`・`build` はファイルを渡しても同じ親ディレクトリの兄弟ファイルをすべて読むので、IR の比較は 1 プログラム 1 ディレクトリの `Main.tz` で行う。着手時に再確認する）。

### 範囲

- `for i in 1 .. 3` は両端を含む（`6`）。ステップは真ん中（`1 .. 2 .. 9` は `25`、`3 .. 1` は空）。`ref xs[1..3]` は半開（`[20, 30]`）。
- `1 ..< 3`・`1 ..= 3`・`ref xs[1..=3]` は `error[E0002]: expected an expression`。`let r = 1 .. 3` は `E1005`。借用でない `xs[1..3]` は `E0002`（`array slices must be borrows; ...`）。
- `ExprKind::Range` は `for i = a to b` / `downto`（`counted`）と共用。スライスの `&`・`ref` はスライスの式が飲み込み、`ExprKind::Borrow` を作らない（`Notation` が残らない）。

### 接尾辞

- `numeric::SHORT_SUFFIXES` は 15 個、`NUMERIC_NAMES` は 20 個（17 個の数値型名と `ubyte`・`sbyte`・`byte`）。`Lexer::number` は短い接尾辞を型名に直して `Integer("86i8u")` にするので、**綴りはトークンから失われる**（`Token.span` は元の綴りを覆う）。
- 18 通りの綴りと、16 進・2 進・桁区切り・小数・指数・負の数との組み合わせは、型名の綴り版と `tsuzuri run` の出力が同じで、`--emit llvm` の IR は `-O0`・`-O3` とも **1 byte も違わない**。

### 借用と参照外し

- `&x`／`ref x`、`&mut y`／`ref mut y`、`*r`／`deref r`、`**rr`、型の `&T`／`ref T`、`&&T` はどれも受理される（`&&x` は式では `E0002`）。
- **同じ意味ではない場合**: `&r`（`r: ref i64`）は `ref ref i64`、`ref r` は借り直した `ref i64`（`reborrow_operand`）。`len2 (&[])` は通る（`0`）が `len2 (ref [])` は `E1004`（記号の借用だけが期待する型を被演算子へ渡す）。
- **被演算子の範囲が違う**: 先頭の `*`・`&` の被演算子は application を含む（`PREFIX` = 15 < `APPLICATION` = 16。`*id_ref r` は `*(id_ref r)`）が、キーワードの `deref`・`ref` と引数位置の記号の接頭辞の被演算子は term（`TERM` = 17）。`deref id_ref r` と `add deref r deref r` は `E0002`。スライスの式だけを括弧で包むと `E0002`（`ref (xs[1..3])`）。`(ref xs[1..3])` は通る。
- `api-diff` は `&i64` と `ref i64` を同じ署名とみなす。

### 旧ラムダ

- `x -> x + 1` は受理。引数は識別子 1 つだけ（`(p, q) -> ..` は `E0002`）。`x -> y -> e`・`x -> \y -> e`・`x -> (\y -> e)` は**1 つの 2 引数ラムダに結合**される。
- ローカルの `let a = <ラムダ>` の IR（`--emit llvm -O0`）: `x -> y -> x + y` と `\x y -> x + y` は **1 byte も違わない**（3,236 byte）。`\x -> \y -> x + y` は違う（3,686 byte。クロージャを返す間接呼び出しと、その結果の間接呼び出しになる）。**書き換えは `\x y -> e`**（D11）。

### そのほか

- `&&&`・`|||`・`^^^`・`~~~`、`Array<T>`、`fn(A) -> B`、`else if` も受理される（対象外。[付録 D](A22-appendix.md)）。
- `--warn` が受ける名前は `implicit-copy` だけ（`--warn canonical` は `<command line>:1:1: error[E2000]: unknown warning 'canonical' for --warn; supported: implicit-copy`）。`copies::warnings(&module)` は `main.rs` の `run` の経路で、フラグがあるときだけ呼ばれ、`module.warnings` に連ねて出る。診断の JSON は現行の 5 キー（`severity`・`code`・`message`・`path`・`span`）。
- `Edition::SUPPORTED` は `[E2026]` だけで、`tests/editions.rs` と `src/package.rs` の単体テスト（2 箇所）が `supported editions: 2026` を固定している。`tsuzuri new` は `Edition::LATEST` を書く。`E0004`・`W1007` は HEAD のどこにもない。
- `Parser::new(source, tokens)` は edition を持たない。構文解析は `Result<Program, Vec<Diagnostic>>`（`program_all`）で、**成功した構文解析だけ**が frontend cache に入る。後戻りしない。補間の穴の式のスパンは実際のファイルの位置。コーパスの数は [付録 C](A22-appendix.md)。

## 変えてはいけない意味

- `2026`（manifest なしと std を含む）のソースの受理・拒否・診断・生成 IR。`..<`・`..=` を使わないソースは、どの Phase でも IR が 1 byte も変わらない（手順 1 のコーパスの IR と比べる）。
- `for i in a .. b`（両端を含む）、`xs[a..b]`（半開）、`for i = a to b` の意味。素の `..` はスライスで正準のまま。
- 閉区間の `for` の IR（`range_loop` の 3 つの経路。切り出しは IR を変えない）。
- `&r`（`r` が参照）の意味（参照への参照）。`2026` では今のまま。`2027` では書けなくなる（D7）。
- 既存の診断コードの意味と JSON の 5 キー、`Edition::LATEST`・`MANIFEST_DEFAULT`・std の edition。
- 診断のない入力の出力（`tsuzuri check` の標準出力・標準エラー・終了コード）。W1007 は `--warn canonical` のときだけ出る。

## Phase 分割

| Phase | 内容 | 規模 | 前提 | 単独で出荷できる理由 |
| --- | --- | --- | --- | --- |
| 1 | edition `2027` の足場（`Edition::E2027`、`rejects_noncanonical`、manifest・`--edition`）。差はまだない | S | なし | `2027` は `2026` と同じ意味で通る。既定の挙動は変わらない |
| 2 | 範囲の正準形 `..=`・`..<`（全 edition。`for` とスライス）。半開の `for` を実装する | M | なし | 受理を増やすだけ。使わない限り IR は変わらない |
| 3 | 非正準な表記の検出（6 種類）、警告 `W1007`（`--warn canonical`）、`fixes`、`reference_borrows` | M | Phase 2（fix が `..=` を書く）。fix の付与は G22 | 既定は無効。フラグなしなら出力は 1 byte も変わらない |
| 4 | edition `2027` の拒否 `E0004`（6 種類） | S | Phase 1・3 | `2027` を選んだパッケージだけに効く |
| 5 | コーパスの移行試験、`tsuzuri new` の既定を `2027` に、`tsuzuri fix --edition 2027` の統合、文書 | M | Phase 1〜4。統合は G24 | 移行は任意。`new` だけが新しい既定になる |

**着手する Phase は人間の指示に従う（指示がなければ Phase 1 だけ）。Phase 1〜4 は同じリリースに入れる（D13）。**

## 仕様

### 決定表: 非正準な表記（Phase 3 が検出し、Phase 4 が `2027` で拒否する 6 種類）

次の表が本チケットの**唯一の対象**である。各行を実機で確かめた（「現状」。`r01`・`d01` のような番号は [付録 A](A22-appendix.md#付録-a-現状の実測本文現状の全記録) の実験の番号）。W1007 と E0004 の kind は別（D8）。E0004 は fix を持たない（D9）。

| # | 表記（`2026` で受理） | 正準形 | kind（W1007 / E0004） | fix | 意味が同じである根拠 |
| --- | --- | --- | --- | --- | --- |
| 1 | `for` の素の `..` の**最後の**演算子（`a .. b`、`a .. s .. b` の 2 つ目） | `..=` | `bare_range_in_for` / `bare_range_in_for_rejected` | `certain`: `..` を `..=` に置換 | 素の `..` は両端を含む（r01・r02）。`RangeOp::Bare` と `RangeOp::Closed` は同じ型付き IR を作る（手順 5 の `bare_and_closed_ranges_have_identical_ir`） |
| 2 | 15 個の短い接尾辞（`y uy s us u l ul L UL hf f F hm m M`）の数値リテラル | 型名の接尾辞（`i8 i8u i16 i16u i32u i64 i64u i128 i128u f16 f32 f128 d32 d64 d128`） | `short_literal_suffix` / `short_literal_suffix_rejected` | `certain`: 接尾辞の部分を型名に置換 | `Lexer::number` は両方を同じトークンにする。18 通りで IR が 1 byte も違わない（`-O0`・`-O3`。「現状」） |
| 3 | 式の `&x`、`&mut x`（スライスの `&xs[a..b]` を含む） | `ref x`、`ref mut x` | `symbolic_borrow` / `symbolic_borrow_rejected` | 被演算子が**場所の式**: `certain`。それ以外（リテラル・呼び出し・演算）: `likely`。被演算子の型が参照: fix なし（help だけ） | 場所の式なら型付き木が同じ。記号の借用だけが期待する型を被演算子へ渡す（`&[]` は通り `ref []` は `E1004`）ので場所以外は `likely`。参照への借用 `&r` は `ref ref T`、`ref r` は借り直し（`ref T`）で型が違う（D7） |
| 4 | 式の `*r`、`**r` | `deref r`、`deref deref r` | `symbolic_deref` / `symbolic_deref_rejected` | `certain`（被演算子の範囲が変わるときは括弧を足す） | チェッカーは `Notation` を見ない（メッセージだけ）。型付き木が同じ（d01〜d04）。範囲の違いは括弧で吸収する（D7） |
| 5 | 型の `&T`、`&mut T`、`&&T`、`&&mut T`、`&{r} T` | `ref T`、`ref mut T`、`ref ref T`、`ref ref mut T`、`ref {r} T` | `symbolic_reference_type` / `symbolic_reference_type_rejected` | `certain`: `&`・`&&` を `ref `・`ref ref ` に置換 | 同じ `TypeExprKind::Reference` を作る（`reference_type`）。`api-diff` は 2 つを同じ署名とみなす（「現状」） |
| 6 | 旧ラムダ `x -> e`、連鎖 `x -> y -> e`、`x -> \y -> e` | `\x -> e`、`\x y -> e` | `legacy_lambda` / `legacy_lambda_rejected` | `certain`（連鎖は `\x y -> e` に畳む）。本体が括弧に包まれたラムダなら fix なし | 結合する旧ラムダと `\x y ->` は IR が 1 byte も違わない。`\x -> \y ->` は違う（D11。「現状」） |

**正準として残す（W1007 を出さず、`2027` でも拒否しない）もの**:

| 表記 | 理由 |
| --- | --- |
| `byte`・`ubyte`・`sbyte` の接尾辞（`86ubyte`） | 型名である（`NUMERIC_NAMES` の一部。署名でも `ubyte`・`[byte]` と書く）。D-54 は「短い数値接尾辞（型名の接尾辞に）」。std と fixture に計 211 件ある（付録 C。D2） |
| `86I`（`bigint`） | 型名の綴りがない |
| スライスの素の `..`（`xs[a..b]`、`xs[a..]`、`xs[..b]`）と型の `[T..]` | 半開という 1 つの意味しかない。`2027` では `..` はこの文脈（半開）だけになり、文脈で意味が割れない（D3） |
| `for i = a to b`・`downto` | 別の構文（ループ変数は `i32`）。範囲の `..` ではない |
| 決定表の「正準形」の列の表記（`ref`・`ref mut`・`deref`・`\x -> e`・`..=`・`..<`・`ref T`）と、型の `[T]` | 正準形そのもの |

### 範囲の構文と意味（Phase 2。**全 edition**）

字句: `Lexer::symbol` の最長一致の表に `..=`（`TokenKind::DotDotEqual`）と `..<`（`TokenKind::DotDotLess`）を `..` より前に足す。`a..=b`・`a ..= b` は `..=`、`a..<b`・`a ..< b` は `..<`、`a..b` は従来の `..`。空白があれば別のトークン（`a.. =b` は従来の `E0002`）。`[T..]` は変わらない（`..` の直後が `]`）。`1..<2` は `1` `..<` `2`（小数点は「`.` の直後が数字」のときだけなので影響しない）。HEAD で `..<`・`..=` を含む入力が受理されることはない（`..` の直後に式が要り、式は `<`・`=` で始まらない）ので、既存の受理プログラムは変わらない（手順 5 の確認で数える）。

```ebnf
range        = start RANGE_OP finish                  (* ステップなし *)
             | start ".." step RANGE_OP finish        (* ステップ付き。最初の ".." はステップの印 *)
RANGE_OP     = ".." | "..=" | "..<"
slice_index  = [ expr ] SLICE_OP [ expr ]             (* 角括弧の中。借用の中だけ *)
SLICE_OP     = ".." | "..=" | "..<"
```

`range` は `for ... in` の列挙式の位置だけ（ほかの位置は従来どおり `E1005`）。ステップ付きの最初の `..` は**素の `..` に限る**（`a ..= s ..= b` は書けない）。

**`for` の範囲の意味**（`for i in A OP B`、`for i in A .. S OP B`）。`A`・`S`・`B` は同じ整数型（8〜128 bit、符号付き・符号なし）で、この順に 1 回ずつ評価する。`S` を省くと `1`。

| 規則 | 内容 |
| --- | --- |
| ステップ 0 | 空かどうかによらず、ループに入る前に言語トラップ（`range step is zero`）。3 つの `OP` で同じ |
| `S > 0`（符号なしは常に） | `A`、`A+S`、… を、`..`・`..=` なら `v <= B`、`..<` なら `v < B` の間だけ列挙する。次の値が型の範囲を外れるなら、そこで止まる |
| `S < 0` | `A`、`A+S`、… を、`..`・`..=` なら `v >= B`、`..<` なら `v > B` の間だけ列挙する |
| 空 | `S > 0` で `A > B`（`..<` は `A >= B`）、`S < 0` で `A < B`（`..<` は `A <= B`） |
| `..` と `..=` | 完全に同じ意味。同じ型付き IR（`RangeOp` だけが違う） |

列挙される値の全表（`i32`・`i8`・`i8u`・`i64` の境界、ステップ付き、降順、ステップ 0 のトラップ）は [付録 F](A22-appendix.md#付録-f-範囲とスライスの値の表)。たとえば `0 ..< 3` は `[0,1,2]`、`0 ..= 3` は `[0,1,2,3]`、`3 ..< 3` は `[]`、`10 .. -3 ..< 1` は `[10,7,4]`、`i8` の `125 .. 2 ..< 127` は `[125]`。独立した参照は手順 8 の JavaScript の `referenceRange`。

**スライスの意味**（`xs[a OP b]`。借用の中だけ。`ref`・`ref mut`・`&`・`&mut` は従来どおり）:

| 書き方 | 意味 | 型付き IR |
| --- | --- | --- |
| `xs[a..b]`、`xs[a..<b]`、`xs[..b]`、`xs[..<b]`、`xs[a..]` | `[a, b)`（半開。省いた始点は 0、終点は長さ） | 変更なし（`Slice { start, end }`） |
| `xs[a..=b]`、`xs[..=b]` | `[a, b]`。`end = b + 1` を **2 の補数で巻き戻る足し算**（`TypedExprKind::Binary(BinaryOp::Add, b, Int(1))`、型 `i64`）で作り、既存の `Slice` の検査（符号なし比較 `start <= end`・`end <= length`、`TrapKind::BoundsCheck`）に渡す | `Slice { end: Some(b + 1) }` |

`xs = [10,20,30,40,50]` で、`xs[1..=3]` は `[20,30,40]`、`xs[2..=1]` と `xs[0..=-1]` は `[]`、`xs[0..=5]`・`xs[2..=0]`・`xs[0..=-2]`・`xs[0..=MAX]` は `BoundsCheck` のトラップ（全表は付録 F）。**足し算のオーバーフローそのものはトラップにしない**（`@checked` の中でも、この合成した `+ 1` は検査付き算術にしない）。

**範囲の構文の誤り**（`E0002`。メッセージは固定）:

| 入力 | メッセージ（位置は演算子） |
| --- | --- |
| `xs[a..=]`、`xs[..=]` | `an inclusive slice needs an end bound; write 'ref xs[start..=end]', or 'ref xs[start..]' to slice to the end` |
| `xs[a..<]`、`xs[..<]` | `a half-open slice with '..<' needs an end bound; write 'ref xs[start..<end]', or 'ref xs[start..]' to slice to the end` |
| `for i in a ..= b .. c`、`a ..< b ..= c`、`a .. s .. b ..= c`（範囲の後に範囲の演算子が続き、どちらかが `..=`・`..<`） | `a range takes at most one step; write 'start .. step ..= finish'` |

既存の `xs[..]` の `E0002`（`a slice needs at least one bound; ...`）と、スライスが借用でないときの `E0002` は変えない。

### 接尾辞の規則（Phase 3）

`lexer::numeric_suffix(text: &str) -> &str`（`pub(crate)`。新規）は、字句解析を通った数値リテラルの原文から**接尾辞の部分**を返す。`Lexer::number` と同じ走査で、基数の接頭辞（`0x`・`0b`）、桁と桁区切り `_`、小数点、指数（`e`／`E`、符号、桁）までを数値の部分とし、残りが接尾辞である。16 進・2 進は桁を貪欲に取る（`0x1Ful` の接尾辞は `ul`、`0xFFf` は接尾辞なし）。`Parser::literal` が `Integer`・`Float` のトークンのたびに呼び、`SHORT_SUFFIXES` のキーのどれかと**完全に一致**したときだけ記録する（`numeric_suffix` が `u` を返すのは `86u` だけで、`86i32u` は `i32u` なので一致しない）。

| 原文 | 接尾辞 | 記録 | 書き換え後 |
| --- | --- | --- | --- |
| `86uy`・`1_000l`・`0x10l`・`0b101uy` | `uy`・`l`・`l`・`uy` | する | `86i8u`・`1_000i64`・`0x10i64`・`0b101i8u` |
| `1.5f`・`2e3f`・`1F`・`1hm`・`1M` | `f`・`f`・`F`・`hm`・`M` | する | `1.5f32`・`2e3f32`・`1f128`・`1d32`・`1d128` |
| `86i64`・`86i32u`・`86ubyte`・`86I`・`1f32`・`0xFFf`・`3` | （型名・なし） | しない | — |

### 借用・参照外し・型の規則（Phase 3）

被演算子と位置の分類で、書き換えの編集（`TextEdit`）を決める。

**被演算子が項（term）の形か**（`is_term_shaped(expr)`。再帰）: 真になるのは次だけ。`Name`・`QualifiedFunction`・`TypeFunction`・`Integer`・`BigInt`・`Float`・`String`・`Char`・`Utf`・`Bool`・`Unit`・`Array`・`List`・`Interpolated`、`Field`・`Index`・`Slice`・`Borrow`・`Dereference`（`base`・`value`・`operand` が項の形のとき）、**括弧で包まれた式**（`is_group(expr)`: 式の最初のトークンが `(` で、対応する `)` が最後のトークン。`Tuple` を含む）。それ以外のすべての `ExprKind`（`Call`・`Binary`・`Unary`・`Cast`・`Lambda`・`If`・`Match`・`Block`・`Record` を含む）は偽で、括弧を足す側に倒れる（余分な括弧は意味を変えない）。

**位置**: `prefix(.., term)` の `term` が真なら**項の位置**（`f &x y` の引数）、偽なら**先頭の位置**。

| 位置・被演算子 | 編集（`&x`・`*r` の例。`op` は記号のトークン） |
| --- | --- |
| 先頭の位置・被演算子が項の形 | `op` だけを置換: `&` → `ref `、`*` → `deref `、`**` → `deref deref `（`&mut` は `&` だけを置換し、`mut` は残る）。`&x` → `ref x`、`&mut x` → `ref mut x`、`*r` → `deref r` |
| 先頭の位置・被演算子が項の形でない（`&f x`、`*f x`） | `op` を同じように置換し、**被演算子だけ**を括弧で包む（被演算子の始まりに `(`、終端に `)` を挿入。幅 0 の編集）: `&f x` → `ref (f x)`、`&mut f x` → `ref mut (f x)`、`*f x` → `deref (f x)` |
| 項の位置（`f &x y` の引数） | **式全体**を括弧で包む: `op` を `(ref `／`(deref `／`(deref deref ` に置換し、被演算子の終端に `)` を挿入: `f &x y` → `f (ref x) y`、`f &mut x y` → `f (ref mut x) y` |

スライスの借用 `&xs[a..b]` の被演算子はスライスの式全体（`op` の直後から `]` まで）で、項の形（`xs` が項の形のとき）。**スライスの式だけを括弧で包まない**（`ref (xs[1..3])` は `E0002`。d07）。項の位置は `f (ref xs[1..3])` になる。

被演算子の分類（借用だけ）: `is_place_path(expr)` = `Name`・`Field(place, _)`・`Index(place, _)`・`Dereference(place, _)`・`Slice { value: place, .. }` → `FixLevel::Certain`。それ以外 → `FixLevel::Likely`。型検査が被演算子の型を参照と解決した借用（`reference_borrows`）→ `FixLevel::Unavailable`（編集を捨てる）。参照外し・型・範囲・接尾辞・ラムダは常に `Certain`（ラムダの括弧つきの本体を除く）。

**型**: `type_primary` が `&`（`TokenKind::Ampersand`）を食べるとき、`&` のトークンの範囲を `ref ` に置換する編集を 1 つ。`&&`（`TokenKind::AndAnd`）は `ref ref ` に置換。`&mut`・`&{r}` の `mut`・`{r}` は残る。括弧は要らない。

### 旧ラムダの規則（Phase 3）

`identifier_expression` が識別子の直後の `->` を食べて `lambda(name, arrow, ..)` を呼ぶ。`lambda` は本体を構文解析したあと、次の 4 つに分ける（本体 `body` の種類は `body.kind` と原文の最初の文字で決める）。

| 本体 | 記録（1 件。主のスパンは `x ->`（引数の始まりから `->` の終わり）） | 編集 |
| --- | --- | --- |
| ラムダでない | `Certain`。`spelling = "x ->"`、`canonical = "\x ->"` | `x` の始まりに `\` を挿入 |
| `\` で始まる明示のラムダ | `Certain`。`spelling = "x ->"`、`canonical = "\x"` + 内側の引数 | `x` の始まりに `\` を挿入。`x` の終わりから内側の `\` の終わりまでを 1 つの空白に置換（` -> \` を消す） |
| 旧ラムダ（識別子で始まる） | `Certain`。`spelling` は `x -> y ->`（連鎖の引数を `->` でつないだもの）、`canonical` は `\x y ->` | 内側の記録を**取り除いて**（`kind == LegacyLambda` かつ `span.start == body.span.start` の最後の記録）、内側の編集のうち先頭の `\` の挿入を除いたものを引き継ぐ。さらに `x` の始まりに `\` を挿入し、`x` の終わりから内側の引数の始まりまでを 1 つの空白に置換 |
| `(` で始まる（括弧で包まれたラムダ） | `Unavailable`。help だけ（`write the lambda as '\x y -> ...'`） | なし |

外側が結合するときは内側が先に構文解析を終えている（内側の記録が先に入る）。取り除いた内側の記録は、診断として出ない。

### 診断 `W1007`（非正準な表記。既定は無効）と `E0004`（edition 2027 が拒否する表記）

| 項目 | `W1007` | `E0004` |
| --- | --- | --- |
| 重要度 | 警告（`--warn canonical` のときだけ出る。`--deny-warnings` では警告として数える） | 誤り（edition `2027` で読んだソースだけ。構文解析の段で出し、構文解析は失敗する） |
| メッセージ | `'{spelling}' is not canonical; write '{canonical}'` | `'{spelling}' is not allowed in edition 2027; write '{canonical}'` |
| 主のスパン・`help` | 下の表（`related` は両方とも付けない） | 同じ |
| `kind` | 決定表の `W1007` の kind | 決定表の `_rejected` の kind |
| `fixes` | 決定表の「fix」。1 件。`title` は下の表。`edits` は最大 64 件（超える連鎖は fix なし） | **なし**（D9。移行は 2026 のうちに `W1007` の fix で行う） |
| 出す対象 | 根のパッケージ（`SourceInput.package == None`）の `ModuleOrigin::User` のソースだけ（D10） | edition `2027` で読んだすべてのソース（`2027` の依存パッケージのソースにも出る。std は `2026` なので出ない） |
| 順序・重複 | `(source, start)` の昇順。D-51 の重複除去のキーに従う | 構文解析の順（出現順）。ほかの構文の誤りと同じ列に入る |

| 表記 | 主のスパン | `spelling` → `canonical` | help | fix の `title` |
| --- | --- | --- | --- | --- |
| 素の `..` | 最後の演算子のトークン（2 文字） | `..` → `..=` | `write '..=' to include the end or '..<' to exclude it` | `Replace '..' with '..='` |
| 短い接尾辞 | リテラル全体 | `86uy` → `86i8u`（原文の綴りを保つ。桁区切り・小数・指数・基数の接頭辞はそのまま） | `write the type name after the number, as in '86i8u' or '1f32'` | `Replace the suffix 'uy' with 'i8u'` |
| `&x`・`&mut x`（式） | `&` のトークン（`&mut` なら `&` から `mut` の終わり） | `&` → `ref`、`&mut` → `ref mut` | `write 'ref x' to borrow x`。型が参照のとき: `a reference to a reference cannot be written with 'ref'; 'ref r' reborrows r, so pass 'r' itself or restructure the code` | `Replace '&' with 'ref'`／`Replace '&mut' with 'ref mut'` |
| `*r`・`**r`（式） | `*`・`**` のトークン | `*` → `deref`、`**` → `deref deref` | `write 'deref r' to read through the reference r` | `Replace '*' with 'deref'`／`Replace '**' with 'deref deref'` |
| `&T`・`&&T`（型） | `&`・`&&` のトークン | `&` → `ref`、`&&` → `ref ref` | `write 'ref T' for a shared reference and 'ref mut T' for an exclusive one` | `Replace '&' with 'ref'`／`Replace '&&' with 'ref ref'` |
| 旧ラムダ | `x ->`（引数の始まりから `->` の終わり。連鎖は `x -> y ->`） | `x ->` → `\x ->`、`x -> y ->` → `\x y ->` | `start the lambda with a backslash, as in '\x y -> body'` | `Replace 'x ->' with '\x ->'` |

kind はすべて ASCII の `snake_case`・48 文字以内・全体で一意（D-51）。12 個の kind の一覧は `canonical::KINDS`（新規）が持ち、単体テストが一意性と長さを検査する。

**例 1**（想定出力。`Main.tz` = `let mut total = 0` / `for i in 1 .. 3 do` / `    total = total + i` / `total`。`tsuzuri check --warn canonical Main.tz`。G22 の後は `fix` の行も出る）:

```text
Main.tz:2:12: warning[W1007]: '..' is not canonical; write '..='
  2 | for i in 1 .. 3 do
    |            ^
  help: write '..=' to include the end or '..<' to exclude it
  fix (certain): Replace '..' with '..='
```

同じソースの `--json`、例 2〜5（接尾辞 `86uy`、借用 `&x`、旧ラムダの連鎖 `x -> y ->`、参照の借用 `&r`。fix の行がない）、`E0004` の例（例 6）は [付録 B](A22-appendix.md#付録-b-診断の例本文の例-1以外)。

**`tsuzuri check --json` の終了コード**: `W1007` だけなら 0（`--deny-warnings` を付ければ 1）。`E0004` があれば 1。

### CLI

| 入力 | 動作 | 終了コード |
| --- | --- | --- |
| `--warn canonical`（`check`・`build`・`run`・`test`・`script`。`fmt` は従来どおり警告の系統のオプションを拒否する） | `W1007` を有効にする。`--warn implicit-copy` と併用できる（それぞれ 1 回まで） | 警告だけなら 0 |
| `--warn` の誤り | `--warn requires a warning name; supported: implicit-copy, canonical`／`unknown warning '{name}' for --warn; supported: implicit-copy, canonical`／`warn canonical specified more than once`（`E2000`、`<command line>:1:1`。一覧は D-55 の固定順で、E15 の `untrusted-extern` が入っていれば末尾に付く） | 2（既存の引数の誤りと同じ） |
| `--edition 2026\|2027`（`check`・`build`・`run`・`test`・`script`。マニフェストのない入力の edition） | 根のソース（`ModuleOrigin::User`）を指定の edition で読む。std は `LATEST` | 診断がなければ 0 |
| `--edition` の引数の誤り | 値が対応外: `unsupported edition '{value}' for --edition; supported editions: 2026, 2027`。値がない: `--edition requires an edition year; supported editions: 2026, 2027`。2 回: `edition specified more than once`。ほかのコマンド: `--edition is only valid with check, build, run, test, script, or new`（`E2000`、`<command line>:1:1`） | 2 |
| `--edition` とマニフェスト | **マニフェストがあれば**（`Project::root_package` が `Some`）、読み込みの後に `E2000`（パスは `Tsuzuri.toml`）: `--edition cannot be used in a project that has a Tsuzuri.toml; set 'edition' there` | 1 |
| `tsuzuri new DIR [--namespace NAME] [--edition 2026\|2027]`（Phase 5） | 書くマニフェストの `edition` を選ぶ。既定は `2027`。値の誤りは上の 3 つの文言。usage は `usage: tsuzuri new directory [--namespace NAME] [--edition 2026\|2027]` | 0（誤りは 2） |
| `tsuzuri script [--edition 2027] FILE` | `script_arguments` が `run` の引数へそのまま渡す。`script_json` も値を飛ばす | `run` と同じ |

`--help` の本文（`main.rs` の `fn help` が返す文字列。`--warn implicit-copy` の行と `tsuzuri new` の行）に `--warn canonical` と `--edition` を足す。

### edition・manifest・cache

- `Edition::E2027` を足し、`SUPPORTED = [E2026, E2027]`。**`LATEST`・`MANIFEST_DEFAULT` は `E2026` のまま**。`Edition::parse("2027")` が通り、`unsupported edition '2030'; supported editions: 2026, 2027` になる。
- `Edition::rejects_noncanonical(self) -> bool { self >= Self::E2027 }`（新規）。**この `>=` は意図**（一度拒否した表記は後の edition でも拒否し続ける）。拒否の判定は必ずこの関数を使い、`== E2027` と書かない。
- manifest の `edition = "2027"` を受理する。`2027` と `2026` は意味が同じで、拒否する表記だけが違う。IR は edition の値によらず同じ。
- std は `LATEST` = `2026` で読むので、非正準な表記は拒否されず `W1007` も出ない。依存パッケージは自分の `edition` で読む（`2027` の根が `2026` の依存を使っても依存のソースは拒否されない。逆も使える）。
- frontend cache の parse key は edition を含む（`parse_key`）。`FRONTEND_FORMAT` は Phase 2 で 4 → 5、Phase 3 で 5 → 6（AST の符号化が変わるたびに 1 上げる）。`E0004` で失敗した構文解析は cache に入らない（既存の規則）。
- `tsuzuri fmt` はマニフェストの edition で読む（既存）。`2027` のソースに非正準な表記があると、`fmt` は構文解析の段で `E0004` になり、ファイルを書き換えない。`doc`・`api-diff`・LSP は各ソースの `edition` で読む（既存）。

### 上限・決定性

| 名前 | 値 | 超えたとき |
| --- | --- | --- |
| 1 ソースの記録（`Program.noncanonical`）の件数 `canonical::MAX_FINDINGS` | 65,536 | それ以降は記録しない（`W1007` は欠ける。`E0004` は `MAX_UNIQUE_DIAGNOSTICS` で止まるので影響しない）。診断は出さない |

生成は決定的: 記録は構文解析の順、診断は `(source, start)` の昇順、編集は位置の昇順。`BTreeMap`・`BTreeSet` だけを使い、`HashMap` の反復で順序を決めない。

## 設計

### データ構造

```rust
// src/syntax.rs（Phase 1）
impl Edition {
    pub const E2027: Self = Self(2027);
    pub const SUPPORTED: &'static [Self] = &[Self::E2026, Self::E2027];
    /// What `tsuzuri new` writes (Phase 5). `LATEST` and `MANIFEST_DEFAULT` stay `E2026`.
    pub const NEW_PROJECT: Self = Self::E2027;
    /// `>=` on purpose: a spelling that one edition rejects stays rejected in later ones.
    pub fn rejects_noncanonical(self) -> bool { self >= Self::E2027 }
}

// src/syntax.rs（Phase 2）。TokenKind に 2 つ足す: `DotDotEqual`（`..=`）・`DotDotLess`（`..<`）
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RangeOp { Bare, Closed, Open }   // `..`、`..=`、`..<`
// ExprKind::Range { start, step, finish, counted, descending, op: RangeOp }   （`for i = a to b` は op: Closed）
// ExprKind::Slice { value, start, end, mutable, op: RangeOp }

// src/syntax.rs（Phase 3）
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoncanonicalKind { BareRange, ShortSuffix, SymbolicBorrow, SymbolicDeref, SymbolicReferenceType, LegacyLambda }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FixLevel { Certain, Likely, Unavailable }
#[derive(Clone, Debug, PartialEq)]
pub struct TextEdit { pub span: Span, pub text: String }   // 幅 0 のスパンは挿入
#[derive(Clone, Debug, PartialEq)]
pub struct Noncanonical {
    pub kind: NoncanonicalKind,
    pub span: Span,            // 主のスパン（決定表の「主のスパン」）
    pub spelling: String,      // メッセージの `{spelling}`: "..", "86uy", "&mut", "*", "x -> y ->"
    pub canonical: String,     // メッセージの `{canonical}`: "..=", "86i8u", "ref mut", "deref", "\x y ->"
    pub level: FixLevel,
    pub edits: Vec<TextEdit>,  // `Unavailable` のとき空
}
// Program に `pub noncanonical: Vec<Noncanonical>` を足す（構文解析の順）。

// src/check.rs（Phase 2・3）
// TypedExprKind::ForRange { local, start, step, finish, exclusive: bool, body }
// CheckedModule に `pub noncanonical: Vec<Noncanonical>` を足す（根のパッケージの User のソースだけ。`reference_borrows` の捨てを適用済み）
```

`Parser` に `edition: Edition`（`Parser::new(source, tokens, edition)`）と `noncanonical: Vec<Noncanonical>` を足す。`parse(source)`（edition なしの入口）は `Edition::LATEST`、`parse_with_source`・`parse_with_source_all` は引数の edition を渡す。`Checker` に `symbolic_borrows: Vec<(Type, Span)>` と `reference_borrows: Vec<Span>` を足す（`undecided_borrows` と同じ形）。

### 新規ファイル

| ファイル | 内容 |
| --- | --- |
| `src/canonical.rs` | `pub const KINDS: [(&str, &str); 12]`（W1007 と E0004 の kind。`(kind, "W1007"\|"E0004")`）、`pub const MAX_FINDINGS: usize = 65_536`、`pub fn warnings(module: &CheckedModule) -> Vec<Diagnostic>`（`--warn canonical` と G24 が呼ぶ）、`pub(crate) fn rejections(findings: &[Noncanonical]) -> Vec<Diagnostic>`（構文解析が呼ぶ）、`fn message`・`fn help`・`fn title`・`fn kind_name(kind, rejected)`。単体テスト（kind の一意性・48 文字以内・メッセージの雛形） |
| `src/parse_canonical.rs` | `Parser` の補助（`#[path = "parse_canonical.rs"] mod noncanonical;` を `parser.rs` に足す。`parse_control.rs` と同じ形）: `record_bare_range`・`record_short_suffix`・`record_symbolic_prefix`・`record_symbolic_type`・`record_legacy_lambda`（すべて `#[inline(never)]`。GUIDE §11.1）、`is_group`・`is_term_shaped`・`is_place_path` |
| `tests/range_forms.rs`・`tests/canonical_forms.rs`・`tests/edition_2027.rs`・`tests/fixtures/range_forms/Main.tz`・`tests/edition_2027.mjs` | 「テスト計画」 |

### アルゴリズム

**A1. 範囲の構文解析（`expression_inner`・`range_expression`）。** `expression_inner` の `self.eat(&TokenKind::DotDot)` を、`..`・`..=`・`..<` のどれかを食べる形にし、食べたトークン `op1` を `range_expression(left, op1, ..)` へ渡す。`op1` が `..` で、次の式のあとにまた 3 つのトークンのどれかが来るなら、ステップ付き（次の式が step、2 つ目が演算子）。範囲の後にさらに演算子が続き、どちらかが `..=`・`..<` なら `E0002`（「範囲の構文の誤り」の 3 行目）。演算子が `..` なら `record_bare_range`（Phase 3）。`for_source`（`for i = a to b`）の `Range` は `op: RangeOp::Closed`。疑似コードは [付録 E](A22-appendix.md#付録-e-アルゴリズムの詳細)。
**A2. スライスの構文解析（`index_or_slice`）。** `first` を読む前の `self.at(&TokenKind::DotDot)` と、`let slice = self.eat(&TokenKind::DotDot)` を、3 つのトークンを見る形にして、食べたものから `op` を決める。`op != Bare` で `last` がなければ、決定表の `E0002`（位置は演算子）。`ExprKind::Slice` に `op` を入れる。スライスの素の `..` は記録しない（正準）。

**A3. 範囲の型検査。** `control_expression` の `ExprKind::Range { .. }` の分岐は、`op == RangeOp::Open` なら `TypedExprKind::ForRange` の `exclusive` を `true` にする（`Bare`・`Closed` は `false`）。`Checker::slice` は `op == Closed` のとき、`end` を `TypedExpr { kind: TypedExprKind::Binary(BinaryOp::Add, Box::new(end), Box::new(TypedExpr { kind: TypedExprKind::Int(1), ty: Type::I64, span })), ty: Type::I64, span }` に包む（`finish_binary`・`checked_kind` を**通さない**: 巻き戻る足し算）。

**A4. 閉区間ループの切り出しと半開（`llvm_control.rs`）。** `range_loop` の本体のうち、3 つの式の評価と `enter_counter` のあと（`let Type::Integer(bits, signed) = local.ty` から `self.begin(&exit)` まで）を、そのまま `closed_range_loop(local, step, first, stride, last, body)` へ移す（**IR を 1 byte も変えない切り出し**）。`range_loop` は `exclusive` を受け取り、次のようにする。

```text
range_loop(local, start, step, finish, exclusive, body):
  first = expression(start); stride = expression(step); last = expression(finish)
  ranges.enter_counter(module, local, start, step, finish)      // 半開でも閉区間の式のまま。上端が 1 大きい上界で、健全
  if not exclusive: closed_range_loop(local, step, first, stride, last, body); return
  join = label()
  last_incl = half_open_last(local, step, first, stride, last, join)
  closed_range_loop(local, step, first, stride, last_incl, body)
  jump(join); begin(join)

half_open_last(...):                       // 空なら join へ。空でなければ prep で「閉区間の終端」を作って返す
  one = step is Int(1); minus_one = signed && step is Int(-1)
  if one:        nonempty = icmp (signed ? slt : ult) first, last
  elif minus_one: nonempty = icmp sgt first, last
  else:          // 一般のステップ: 0 の検査を閉区間と同じトラップで先に行う
                 guard(icmp ne stride, 0, TrapKind::RangeStepZero)
                 if signed: positive = icmp sgt stride, 0
                            nonempty = select positive, (icmp slt first, last), (icmp sgt first, last)
                 else:      nonempty = icmp ult first, last
  prep = label(); branch(nonempty, prep, join); begin(prep)
  if one or (unsigned general):  return `sub ty last, 1`
  if minus_one:                  return `add ty last, 1`
  // signed general: 両方を計算して選ぶ（フラグなし。どちらも非空が保証した方向だけが使われる）
  return select positive, (sub ty last, 1), (add ty last, 1)
```

`sub`・`add` に `nsw`・`nuw` は付けない（非空の枝の中でしか使わないので溢れない。LLVM が分岐の条件から導く）。閉区間の経路（`narrow_range_loop`・符号付き／符号なしの ±1・一般のステップ）は `last_incl` を受け取って**そのまま**走る。

**A5〜A9（定数評価器 `const_machine.rs`、`ranges.rs`、接尾辞・借用・参照外し・型・旧ラムダの検出）、A11（警告の運び）、A12（`--edition`）の詳細は [付録 E](A22-appendix.md#付録-e-アルゴリズムの詳細)。** 要点: A5 は半開のとき `finish` を `finish ∓ 1` にして既存の `within` を呼ぶ。A6 は `loop_kind` に `exclusive` の分岐（`0 ..< xs.length` だけ `BelowLength`）。A7 は `Parser::literal` が `numeric_suffix` を呼ぶ。A8 は `prefix`・`type_primary` から `record_symbolic_*`。A9 は `lambda` の 4 つの場合。A11 は `check_modules_collect` が根の User の記録を集め、`reference_borrows` で `SymbolicBorrow` の fix を捨てる。A12 は `Project::with_edition`。

**A10. 記録から診断へ。** `program_all` の `if !diagnostics.is_empty() { return Err(diagnostics); }`（`program.active_patterns = ...` の直前）の**前**で、`self.edition.rejects_noncanonical()` なら `canonical::rejections(&self.noncanonical)`（`E0004`）を `diagnostics` に足す（既存の `if` が `Err` を返す。`program()` は最初の 1 件を返す）。成功のときは `program.dyn_types = self.dyn_types;` の直前で `program.noncanonical = std::mem::take(&mut self.noncanonical)`。`record_*` は `MAX_FINDINGS` に達したら何もしない。

## 前提とする他チケットのインターフェース

| 相手 | 使う名前（D-51・D-54・D-55 に固定） | 未実装のとき |
| --- | --- | --- |
| G22（診断 v2。手順 12 だけが要る） | `Diagnostic::warning(code, message, span)`（既存）に、`.with_kind(&'static str)`・`.with_help(impl Into<String>)`・`.fix(title: impl Into<String>, applicability: Applicability, edits: Vec<Edit>)`。`Edit { span: Span, text: String }`、`Applicability::{Certain, Likely, Maybe}`。JSON のキーの順は D-51（`kind`、`related`、`help`、…、`fixes`） | **手順 12 を飛ばして Phase 3 を出荷する**: `W1007` はメッセージだけ（`kind`・`help`・`fixes` なし）。`module.noncanonical` に編集は入っているので、手順 13 の試験は G22 なしで通る。G22 の実装後に手順 12 を行う（`canonical::warnings` の 1 関数の変更） |
| G24（`tsuzuri fix`。手順 19 だけが要る） | `fix::canonical_warnings(project: &Project, module: &CheckedModule) -> Vec<Diagnostic>`（G24 の Phase 3 は**空を返す**。本チケットが本体を `crate::canonical::warnings(module)` にする。署名は変えない）。`fix --edition 2027` は `W1007` の `certain` の fix だけを当て、`recheck_as` が根のソースの `edition` をメモリ上で `2027` にして再解析する（本チケットの `E0004` が構文解析の段で出る）。`package::set_edition` は G24 が持つ | 手順 19 を飛ばす。G24 が先に実装済みで署名が合わなければ、G24 の停止条件 7 の選択肢 B（本チケットの入口を包む）を使う |
| A18（edition `2027` の足場の共有） | `Edition::E2027`・`SUPPORTED`。A18 の D3・D4 と同じく `LATEST` は `E2026` のまま | 先に実装した方が足す。A18 が先なら手順 2 は確認だけ。**A18 の差が入る前に 2027 をリリースしない**（凍結） |
| G23（kind の登録簿） | 本チケットの 12 個の kind を、G23 があれば S1 の手順で登録簿へ足して `Diagnostic::of` で出し、G22 だけなら `with_kind` で直接付ける（D-51。名前は変えない） | — |
| G21（ガイド） | ガイドは正準形だけを教える。`tsuzuri new` の `AGENTS.md` のひな形は `edition` を置換する（Phase 5 の `new --edition` の値） | — |

**G24 との境界**: 何を直せるか（fix の内容）は本チケット、適用の仕組みとマニフェストの `edition` の書き換えは G24。本チケットの試験は G24 に依存しない。

## 実装手順

各手順は「変更 → 内容 → 確認」。各手順の最後で `cargo build --release --locked` と `cargo test --locked` が通る（`running N tests` の N を控える。GUIDE §3.1）。`/tmp/tz-A22/` は作業用の一時ディレクトリでリポジトリには置かない。

### Phase 1: edition `2027` の足場

**手順 1: ベースライン。**
- 変更: なし。
- 内容: `cargo build --release --locked && cargo test --locked` の件数を控える。`mkdir -p /tmp/tz-A22/bin /tmp/tz-A22/before && cp target/release/tsuzuri /tmp/tz-A22/bin/tsuzuri-before`。コーパスの IR を残す（**IR 比較のループ**。以後「手順 1 の IR」と呼ぶ）:

```sh
for d in tests/fixtures/*/ examples/*/; do
  [ -f "$d/Main.tz" ] || continue
  n="$(basename "$(dirname "$d")")_$(basename "$d")"
  /tmp/tz-A22/bin/tsuzuri-before build "$d" --emit llvm -O0 -o "/tmp/tz-A22/before/$n.ll" >/dev/null 2>&1 || echo "FAILED $d"
done
ls /tmp/tz-A22/before/*.ll | wc -l
```

  `Main.tz` のないライブラリの fixture 7 件とトップレベルの `tests/fixtures/Semantics.tz` は `check` だけで扱う（手順 16）。「現状」の表のプログラムを `/tmp/tz-A22/x/` に作り直し、結果が一致することを確かめる。`git tag --list` を控える。
- 確認: すべて通る。IR は **102 件**（fixtures 93 + examples 9）で `FAILED` なし。`grep -rEn '\.\.[<=]' std tests/fixtures examples benchmarks --include='*.tz' --include='*.tt' --include='*.tc'` が 0 件。

**手順 2: `Edition::E2027`。**
- 変更: `src/syntax.rs`（末尾に `#[cfg(test)] mod tests`）、`tests/editions.rs`、`src/package.rs`（単体テスト 2 箇所）、`docs/stability.md`。
- 内容: 「データ構造」の `Edition` の 4 つを足す。A18 が先に `E2027` を足していれば再実装せず、`NEW_PROJECT`・`rejects_noncanonical` だけを足す。`tests/editions.rs` の既存テストを直す（「既存テストへの影響」）。新しいテストを足す: `edition_2027_is_supported_and_changes_no_generated_code`（`Tsuzuri.toml` に `edition = "2027"` と `"2026"` を書いた同じソースの IR が native と wasm32 で同じ。`the_edition_key_changes_no_generated_code` と同じ形）と `2027_is_a_supported_manifest_edition`（`tests/editions.rs`）、`rejects_noncanonical_starts_at_2027`（`src/syntax.rs` の単体テスト。`E2026` は偽、`E2027` と `Edition(2028)` は真。`Edition` のフィールドは `pub(crate)` なので、`Edition(2028)` は crate の中でしか作れない）。`docs/stability.md` の edition の表に `2027` の行（予約語の追加はなし。差は「非正準な表記の拒否」。状態は「準備中。リリース前」）を足す。
- 確認: `cargo test --locked --test editions` が通る。`cargo test --locked` は手順 1 の件数 + 3。手順 1 の IR と 1 byte も違わない（ループを `after` へ出して `cmp`）。

**手順 3: `--edition`。**
- 変更: `src/main.rs`（`parse_arguments`・`Arguments`・`script_arguments`・`script_json`・`fn help`・`SCRIPT_OPTIONS`・単体テスト）、`src/driver.rs`（`Project::with_edition`）、`tests/editions.rs`。
- 内容: 「CLI」の表のとおり。`Arguments` に `edition: Option<Edition>`。`parse_arguments` は `--edition` の値を `Edition::parse` で読み、`check`・`build`・`run`・`test` 以外の `Action` では「ほかのコマンド」の誤りにする（`script` は `script_arguments` が `--edition` と値を `run` の引数へ渡す。`script_json` も値を飛ばす）。`main` の `Project::load*` の結果を受けた直後で `with_edition` を呼ぶ（誤りは読み込みの誤りと同じく `print_diagnostic` して `ExitCode::FAILURE`）。`new` は別の入口（`new_project`）なので手順 17。単体テスト: `parses_edition_option`・`rejects_bad_edition_options`（値なし・対応外・2 回・`fmt` と併用）。
- 確認: `tests/editions.rs` に `cli_edition_reads_a_project_without_a_manifest`（マニフェストなしの `Main.tz` が `check --edition 2027` で通り、`--edition 2030` が `E2000`、マニフェストのあるプロジェクトで `--edition 2027` が `E2000`）と `script_accepts_edition`。`cargo test --locked` が通る。

### Phase 2: 範囲の正準形

**手順 4: テスト先行。**
- 変更: `tests/range_forms.rs`（新規）、`tests/fixtures/range_forms/Main.tz`（新規。手順 8 で完成させる）、`tests/features.mjs`（suite の雛形）。
- 内容: 「テスト計画」の `tests/range_forms.rs` の名前付きテストをすべて書く（この時点では `E0002` で失敗する）。
- 確認: `cargo test --locked --test range_forms` が、書いた件数だけ失敗する（`running N tests` の N = 書いた件数）。

**手順 5: トークン・構文・`..=` の意味。**
- 変更: `src/syntax.rs`、`src/lexer.rs`、`src/parser.rs`、`src/parse_control.rs`、`src/control.rs`、`src/check.rs`、`src/syntax_codec.rs`、`src/frontend_cache.rs`、`src/formatter.rs`、ほか `ExprKind::Range`・`ExprKind::Slice` の全 `match`（`grep -rn "ExprKind::Range\|ExprKind::Slice" src/`。GUIDE §6.9）。
- 内容: `TokenKind` に `DotDotEqual`・`DotDotLess` を足し、`Lexer::symbol` の表の `("..", DotDot)` の前に `("..=", DotDotEqual)`・`("..<", DotDotLess)` を足す。`RangeOp` を足し、`ExprKind::Range` と `ExprKind::Slice` に `op` を足す（A1・A2。`for_source` の `Range` は `op: RangeOp::Closed`）。`control_expression` は `op` を読む（この手順では `exclusive` なし。`..<` の `for` は `E1005` の仮のメッセージ `a half-open for range is not supported yet` で拒否し、手順 7 で外す）。`Checker::slice` は `Closed` で `end + 1`（A3）。符号化: `impl Wire for ExprKind` の `Range`・`Slice` に `op` を足し、`unit_enum!(RangeOp { Bare, Closed, Open })` と `("RangeOp", 3)`、3 つの値を含む例を単体テストの `Program` に足す。`FRONTEND_FORMAT` を 5 に。`formatter.rs` の `Range`・`Slice` の `Canonical` は `op` をそのまま残す。
- 確認: `cargo test --locked` が通る（`E1005` の仮のメッセージ以外の新規テストが通る）。手順 1 の IR と 1 byte も違わない。`cargo test --locked --lib syntax_codec` が通る。

**手順 6: スライスの `..=`・`..<`。**
- 変更: `src/parser.rs`（`index_or_slice` の `E0002`）、`tests/range_forms.rs`。
- 内容: 「範囲の構文の誤り」の `E0002` を足す。`xs[a..<b]` と `xs[a..b]` が同じ IR を作ること、`xs[a..=b]` が `add i64 .., 1` に続く `icmp ule` の 2 つの比較を作ることを `tests/range_forms.rs` で確かめる。
- 確認: スライスのテストが通る（半開の `for` 以外）。

**手順 7: 半開の `for`（`exclusive`）。**
- 変更: `src/check.rs`（`TypedExprKind::ForRange` と、`ForRange` を持つ全 `match`。`grep -rn "ForRange" src/` は HEAD で 14 ファイル 24 行）、`src/control.rs`、`src/llvm_control.rs`、`src/llvm.rs`、`src/const_machine.rs`、`src/ranges.rs`、ほかコンパイラが `missing field` と言う箇所（`..` を足す。意味は変えない）。
- 内容: A3・A4・A5・A6。**先に切り出しだけ**を行う（`closed_range_loop` を作り、`range_loop` が呼ぶだけにして、手順 1 の IR と 1 byte も違わないことを確かめる）。そのあとで `exclusive` を足し、仮の `E1005` を外す。
- 確認: 切り出しの直後に IR 比較のループが 102 件すべて一致。最後に `cargo test --locked` が通る。`for i in 0 ..< xs.length` と `for i in 0 .. xs.length - 1` を `-O3` の IR で `diff` し、ループの本体（`phi`・`icmp`・`add`）が同じ形かを確かめて**記録する**（違っても停止条件ではない。性能の主張はしない）。`ranges.rs` の単体テストに `half_open_ranges_prove_bounds`（`0 ..< xs.length` で `xs[i]` の境界検査が省かれる、`0 ..< xs.length + 1` では省かれない）を足して通す。

**手順 8: E2E の suite `range_forms`。**
- 変更: `tests/fixtures/range_forms/Main.tz`、`tests/features.mjs`。
- 内容: 「テスト計画」の E2E の関数の表のとおり fixture を書き、suite に `cases`・`traps`・`inspect(ir)` を足す。期待値は suite の中の JavaScript の `referenceRange`（BigInt）で作る。
- 確認: `cargo build --release --locked && node tests/features.mjs target/release/tsuzuri range_forms` が通る（native と wasm32 の `-O0`・`-O3`、`live == 0`、WASM の import が空）。全 suite（`node tests/features.mjs target/release/tsuzuri`）が通る。

### Phase 3: 非正準な表記の検出と `W1007`

**手順 9: 記録のデータ。** 変更: `src/syntax.rs`・`src/syntax_codec.rs`・`src/frontend_cache.rs`・`src/formatter.rs`・`src/parser.rs`（`Parser::new` に `edition` を足し全呼び出しを直す）・`src/parse_canonical.rs`（新規。骨格）。内容: 「データ構造」の `Noncanonical` 一式と `Program.noncanonical`（符号化は `Mode::Full` だけ。`unit_enum!` と単体テストの数）、`Program` を作る全箇所（`grep -rn "declaration_starts" src/`）に `noncanonical: Vec::new()`、`canonicalize` に `program.noncanonical.clear()`、`FRONTEND_FORMAT` を 6 に。確認: `cargo test --locked`・`cargo test --locked --lib syntax_codec` が通り、手順 1 の IR が 1 byte も違わない。

**手順 10: 6 種類の検出。** 変更: `src/lexer.rs`（`numeric_suffix`）・`src/numeric.rs`（`SHORT_SUFFIXES` を `pub(crate)`）・`src/parser.rs`・`src/parse_control.rs`・`src/parse_canonical.rs`・`tests/canonical_forms.rs`（先に書く）。内容: A7・A8・A9 と A1 の `record_bare_range`。`numeric_suffix` の単体テスト（「接尾辞の規則」の表と `0x1Ful`・`0xFFf`・`1e5m`・`86i32u`・`86ubyte`・`86I` を含む 25 行以上。`lex` の型名と `suffix_type(numeric_suffix(原文))` の一致の検査も）。`every_decision_row_is_reported` は `parse_with_source_all(..).noncanonical` を直接見る。確認: `cargo test --locked --test canonical_forms` が通り、手順 1 の IR が 1 byte も違わない。

**手順 11: 診断と `--warn canonical`。** 変更: `src/check.rs`（`Checker` の 2 つのフィールド・`finish`・`check_modules_collect`・`CheckedModule.noncanonical`）・`src/canonical.rs`（新規）・`src/lib.rs`（`pub mod canonical;`）・`src/main.rs`（`Arguments.warn_canonical`・`parse_arguments`・`fn help`・`fmt` との併用拒否・`--warn` の既存の単体テストの文言）・`tests/canonical_forms.rs`。内容: A8 の後半と A11。G22 がなければ `Diagnostic::warning("W1007", message, span)` だけ。確認: `tsuzuri check --warn canonical` が「例 1」の出力（G22 の前は fix の行なし）になる。`--warn canonical` なしの出力は 1 byte も変わらない（fixture の `check` の標準出力・標準エラーを前後で `diff`）。`--deny-warnings` と併用すると 1 で終わる。

**手順 12: fix の付与（G22 が要る）。** 変更: `src/canonical.rs`・`tests/canonical_forms.rs`。内容: `warnings` が `with_kind`・`with_help`・`fix` をつなぐ（`Unavailable` には `fix` をつけない）。確認: 「例 1」の `--json` が一致し、`fixes` の `applicability` が `certain`・`likely` に分かれる。

**手順 13: 適用して IR を比べる試験。** 変更: `tests/canonical_forms.rs`。内容: 補助 `apply(source, findings, minimum) -> String`（`level` が `minimum` 以上の記録の編集を `(start, end)` の降順に適用。重なる編集を含む記録は次のラウンドへ。最大 5 ラウンド）と、`corpus_certain_fixes_keep_the_ir`（各プロジェクトを `Project::load`、`module.noncanonical` の `Certain` を当て、`edition` を `2027` にして `analyze_all()` が誤りなく、`llvm::emit(&module, Entry::Library)` が元と**同一**）。`Likely`・`Unavailable` の残りは数えて報告する。確認: 通る。差が出たら停止条件 3。

### Phase 4: edition `2027` の拒否

**手順 14: `E0004`。**
- 変更: `src/parser.rs`（`program_all` の末尾。`Parser.edition`）、`src/canonical.rs`（`rejections`）、`tests/edition_2027.rs`（新規。先に書く）。
- 内容: A10。`parse_source`・`parse_with_source_all`・`parse` が `Parser::new` に edition を渡す（手順 9 で配線済み）。
- 確認: `tests/edition_2027.rs` の `edition_2027_rejects_every_decision_row`・`edition_2027_reports_every_occurrence_in_one_run`・`edition_2027_accepts_the_canonical_forms` が通る。手順 1 の IR と 1 byte も違わない（コーパスは `2026`）。

**手順 15: std・依存・cache・`fmt` の確認。** 変更: `tests/edition_2027.rs`。内容: `edition_2027_does_not_reject_std_or_2026_dependencies`・`fmt_stops_on_noncanonical_forms_in_2027`・`a_2027_project_uses_the_frontend_cache_after_fixing`（`E0004` を直した次の `check` が成功し、2 回目の `check` が cache に当たる）。確認: すべて通る。

### Phase 5: 移行・`tsuzuri new`・文書

**手順 16: コーパスの移行試験（Rust）。**
- 変更: `tests/edition_2027.rs`。
- 内容: `corpus_migrates_without_changing_the_ir`: `Main.tz` のある 102 件に加え、`Main.tz` のない fixture 7 件とトップレベルの `Semantics.tz` を、`Project::load` の入力をファイルにして `analyze_all` と IR（`Entry::Library`）で比べる。`Certain` だけを当てた結果が `2027` で通る数・通らない数（`Likely` が残る数）を kind ごとに標準エラーへ出す（失敗にしない。**実測を完了報告に書く**）。二度目の適用: 当てた結果にもう一度適用しても何も変わらない。
- 確認: すべて通る。

**手順 17: `tsuzuri new`。**
- 変更: `src/package.rs`（`create_project_with(directory, namespace, edition)` を新設し、既存の `create_project(directory, namespace)` は `Edition::NEW_PROJECT` で呼ぶ薄い包みにする。単体テストの 4 箇所の呼び出しは変えない）、`src/main.rs`（`new_project` が `--edition` を読む。usage の文言と `fn help`）、`tests/editions.rs`。
- 内容: マニフェストの `edition` と `valid_namespace` の引数を、渡された edition にする。`new_project` は `--namespace` と同じ形で `--edition VALUE` を読み、`Edition::parse` で調べる（誤りは「CLI」の表の文言、終了コード 2）。`new_projects_declare_the_latest_edition` を `new_projects_declare_edition_2027` に直す（`edition = "2027"` を期待し、`tsuzuri check` が通る）。`new_project_edition_can_be_chosen`（`--edition 2026` が `edition = "2026"`、対応外は `E2000`）を足す。
- 確認: `cargo test --locked --test editions` が通る。生成された `Main.tz`（`\() -> do! IO.writeln ...`・`assert (1 + 2 == 3)`）が `2027` で `check` を通る。

**手順 18: 文書。**「ドキュメント」のとおり。`node scripts/check-docs.mjs <変更したページ>` を通す。

**手順 19: G24 との統合（G24 が要る）。**
- 変更: `src/fix.rs`（`canonical_warnings` の本体）、`tests/edition_2027.mjs`（新規）。
- 内容: `canonical_warnings` を `crate::canonical::warnings(module)` にする（G24 の手順 14 のテストが空として扱っていれば本物に合わせて直す。G24 の停止条件 7 の選択肢 B）。`tests/edition_2027.mjs`: コーパスから代表 20 個以上（旧ラムダ・`&`・`..`・接尾辞を含む fixture を新規に足して含める）を一時ディレクトリへ写し、`tsuzuri fix --edition 2027` を当て、`Tsuzuri.toml` が `edition = "2027"` になり、native の `-O0` の実行結果が移行前と同じ（`live == 0`）ことを確かめる。2 回目の `fix` は何も変えない。
- 確認: `node tests/edition_2027.mjs target/release/tsuzuri` が通る。

**手順 20: 全体の確認。** 「受け入れ条件」の最後から 2 番目の項目のコマンドをすべて通す。手順 1 の IR と、2026 のコーパスの `check` の出力が前後で 1 byte も違わない。

## テスト計画

テストの名前・入力・期待値の全体は [付録 G](A22-appendix.md#付録-g-テストの表)。新規のテストファイルは `tests/range_forms.rs`（手順 4〜7）、`tests/features.mjs` の suite `range_forms`（手順 8。native/wasm32 × `-O0`/`-O3`、`live == 0`、WASM の import が空）、`tests/canonical_forms.rs`（手順 10〜13）、`tests/edition_2027.rs`（手順 14〜16）、`tests/edition_2027.mjs`（手順 19。G24 後）。期待値は独立した参照（手で書いた書き換え後のソース、BigInt の `referenceRange`、型名の綴りの IR）から作り、**コンパイラの現在の出力を写さない**。拒否は診断コードを照合する。

**既存テストへの影響**（これ以外の期待値が変わるなら停止条件）: `tests/editions.rs` の `edition_names_parse_exactly`（`SUPPORTED == [E2026, E2027]`）・`rejects_unknown_and_unsupported_editions`（3 つの文言が `supported editions: 2026, 2027`）・`new_projects_declare_the_latest_edition`（手順 17 で改名）、`src/package.rs` の単体テスト 2 箇所の同じ文言、`src/main.rs` の `--warn` の単体テスト（`supported: implicit-copy, canonical`）、`src/syntax_codec.rs` の tag の数と `FRONTEND_FORMAT` を固定する箇所（`grep -rn "FRONTEND_FORMAT" tests src`）。ほかは変わらない（新しい構文とフラグを使わない限り、診断・IR・出力は 1 byte も変わらないことを、コーパスの IR の比較と `check` の出力の `diff` で示す）。

**性能**: 性能を主張しない。半開の `for` は手順 7 で `-O3` の IR を閉区間の書き方と比べて記録する。

## ドキュメント

構文と意味が変わるので更新する（GUIDE §8）。漏れは `grep -rnE "\.\.|E0003|W1006|--warn|86uy|ref x|&x" _tsuzuri/language-reference docs README.md` で探す。

- `loops-and-conditionals/for-in.md`: 整数範囲の節に `..=`・`..<`・ステップ付き `a .. s ..= b` を足し、素の `..` は `2027` で拒否されることと `0 ..< words.length` を書く。`for-to.md` は関連項目に `for-in.md` の範囲を足すだけ。
- `literals-and-strings/literals.md`（型名の接尾辞が正準、短い接尾辞は `2027` で拒否、大小の注意）、`ownership-and-memory/borrowing.md`（`ref`・`ref mut`・`deref`・`ref T` が正準、`&x`・`*r`・`&T` は `2027` で拒否、`&r` と `ref r` の違い）、`values-and-functions/lambda-expressions.md`（旧ラムダ `x -> e` の注記）、`values-and-functions/tokens.md`・`op-and-expressions.md`（`..=`・`..<`）。
- `compiler/diagnostics.md`（`E0004`・`W1007` の行と節。コードの範囲の説明を `E0004`・`W1007` まで）、`compiler/usage.md`（`--warn canonical`・`--edition`・`tsuzuri new --edition`）、`organizing-tsuzuri/packages.md` の「言語版（edition）」（edition `2027`）、`index.md`（導線）。
- `docs/stability.md`（edition の表の `2027` の行と差、「edition を追加するとき」の実施記録、凍結の運用メモ。**「edition を追加するとき」の `LATEST` の更新の一文は、「`LATEST` は、新しい edition が受理を減らす間は動かさない（マニフェストのない入力と std が `LATEST` で読まれ、`2027` にすると既存の単独ファイルが `E0004` になるため）。マニフェストのない入力は `--edition` で選ぶ」に書き換え、表の「マニフェストのない入力 … 最新の edition」を「`LATEST`（現在は `2026`）」に直す**。D-54）、`docs/language.md`（範囲・接尾辞・借用・ラムダ・診断の表・`--warn`）、`README.md`（CLI の表）、`docs/architecture.md`（`src/canonical.rs`・`src/parse_canonical.rs`・`Program.noncanonical`・`closed_range_loop`・`FRONTEND_FORMAT`）、`_features/GUIDE.md` §4 のコードマップと D-55 の確認、`_features/README.md` の状態。
- 例は `node scripts/check-docs.mjs <ページ>` を通す。`2027` の拒否を示す例は `LATEST` = `2026` の検査で通らないので `run=` を付けない `text` のブロックにする。`..=`・`..<`・`ref`・型名の接尾辞の例は `run=` で検証してよい。既存の例は書き換えない（D14）。

## 受け入れ条件

- [ ] **Phase 1**: `Edition::E2027`・`SUPPORTED`・`NEW_PROJECT`・`rejects_noncanonical`。`edition = "2027"` が `2026` と同じ IR。`--edition` が「CLI」の表のとおり。`LATEST`・`MANIFEST_DEFAULT` は `2026` のまま。手順 1 の 102 件の IR が 1 byte も変わらない。
- [ ] **Phase 2**: `..=`・`..<` が全 edition で `for` とスライスに通り、「範囲の構文と意味」と付録 F が E2E（suite `range_forms`）と一致する。`..` と `..=` の IR が同一。閉区間のコーパスの IR が変わらない。`for i in 0 ..< xs.length` が境界検査を省く。`E0002` の 6 つの文言。
- [ ] **Phase 3**: `--warn canonical` が決定表の 6 種類を、主のスパン・メッセージ・（G22 の後）`kind`・`help`・`fixes` のとおりに報告する。std・依存は出さない。`--warn canonical` なしの出力は 1 byte も変わらない。`corpus_certain_fixes_keep_the_ir` が通る。`Likely`・`Unavailable` の件数を完了報告に書く。
- [ ] **Phase 4**: edition `2027` が 6 種類を `E0004`（fix なし）で拒否し、すべての出現を 1 回で報告する。std と `2026` の依存は拒否されない。`fmt` は書き換えずに止まる。
- [ ] **Phase 5**: `tsuzuri new` が `edition = "2027"` を書き `check` が通る。`corpus_migrates_without_changing_the_ir` が通り、kind ごとの件数を報告した。G24 があれば `tests/edition_2027.mjs` が通る。文書を更新し `check-docs.mjs` が通る。
- [ ] `cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings`・`cargo test --locked`・全 E2E・GUIDE §3.1 の回帰テスト・`sh scripts/check-runtime-includes.sh` が通る。IR は決定的。
- [ ] `_features/README.md` の状態を `done（Phase N）` にし、GUIDE §13.2 の書式の完了報告を末尾の `## 実装と検証（YYYY-MM-DD）` に書く。コミットは人間の指示があるまで作らない。

## 落とし穴

- **スライスの借用は `Notation` を残さない。** `record_symbolic_prefix` を `prefix()` の早い `return`（スライスの借用）の**前**に呼ぶ。スライスの式だけを括弧で包まない（`ref (xs[1..3])` は `E0002`）。
- **キーワードの `ref`・`deref` は被演算子が term で、後ろに引数が続くと `E0002`。** 引数位置は式全体を、先頭の位置で被演算子が項の形でなければ被演算子だけを括弧で包む。`&r`（`r` が参照）は `ref r` にしない（型が変わる。`reference_borrows`）。
- **短い接尾辞の判定は完全一致。** 末尾の文字で判定すると `86i32u` が `u` に化ける。16 進は桁が貪欲（`0xFFf` は接尾辞なし、`0x1Ful` は `ul`）。
- **旧ラムダの連鎖は 1 件。** 内側の記録を取り除かないと編集が重なる。`\x -> \y ->` へ書き換えると IR が変わる。
- **`Edition` の比較は `rejects_noncanonical` を使う**（`== E2027` と書かない）。std は `2026` で読むので拒否も警告もしない。
- **記録は `Mode::Full` だけ符号化する**（interface に入れると依存側の再コンパイルが増える）。`canonicalize` で空にしないと整形の安全検査（`ast_fingerprint`）が位置の違いで失敗する。`FRONTEND_FORMAT` を上げ忘れると古い cache の AST が `op` を持たないまま読まれる。
- **閉区間の IR を変えない。** `closed_range_loop` の切り出しの直後に 102 件の IR を比べる。ステップ 0 のトラップは空でも出る（既存）ので、半開の前置きでも先に調べる。`enter_counter` は半開でも閉区間の式のまま（上端が 1 大きい上界で健全）で、`loop_kind` だけが `exclusive` を区別する。
- **記録の関数は `#[inline(never)]`**。`Parser` に大きなフィールドを足さない（GUIDE §11.1）。
- **E0004 と W1007 の kind は別。** `canonical::KINDS` の単体テストが 12 個の一意性と 48 文字以内を検査する。

## 対象外

- `..` の意味の変更、`Edition::LATEST`・`MANIFEST_DEFAULT`・std の edition の変更、std・fixture・既存の文書の例の書き換え（D14）、予約語・crate・`unsafe`・既定の WASM の import の追加。
- D-54 が挙げていない同じ意味の書き方（付録 D の `&&&`・`Array<T>`・`fn(A) -> B`・`else if`・`for i = a to b`）の正準化、`tsuzuri fmt` による書き換え、LSP の `source.fixAll`（G24）、`E0004` の fix（D9）、`&r`（参照への参照）を `2027` で書く方法。
- `tsuzuri fix` の適用の仕組みとマニフェストの書き換え（G24）、`explain` のページ（G23）、`W1007` を既定で有効にすること。

## 決定事項

状態はすべて承認済み（D-50）。

- **D1: 正準形は全 edition が受理し、edition `2027` は拒否だけを足す。`..` の意味は変えない。** 理由: D-54 と `docs/stability.md`（差は「前の edition で非推奨にした構文の削除」）。却下: `2027` で `for i in a .. b` を半開にする（旧ソースが黙って別の意味になる）。
- **D2: 対象は 6 種類。`byte`・`ubyte`・`sbyte` の接尾辞は型名なので正準として残す。** 理由: 署名でも使う型名で、D-54 は「型名の接尾辞に」。却下: `i8u`・`i8` に直す（型名と綴りが割れる）。
- **D3: スライスの素の `..` は正準のまま。`..<`・`..=` はスライスにも足す。** 理由: `2027` で `..` が半開の 1 つの意味だけになる。却下: スライスの `..` も拒否する（`xs[a..]` に `..<` の形がない）。
- **D4: 半開の `for` を本チケットで実装し、閉区間の経路は変えない。** 理由: D-54 の正準形に `..<` があり、`0 ..< xs.length` が境界検査を省く。却下: `end - 1` への書き換え（符号なしと `MIN` で意味が壊れる）、仮の一時束縛（`ranges.rs` が見えなくなる）。
- **D5: `xs[a..=b]` は `end = b + 1`（巻き戻る足し算）に下げ、既存の符号なし比較に任せる。** 理由: 新しい型付き IR を足さず、`b = MAX` はトラップになる。却下: 検査付きの足し算（別のトラップになる）。
- **D6: 検出は構文解析で行い `Program.noncanonical` に残し、`E0004` も構文解析の段で出す。** 理由: 表記は構文の情報で、成功した構文解析だけが cache に入る。却下: 型検査の段（型の誤りと混ざる）、字句解析の段（旧ラムダと借用は文脈が要る）。
- **D7: 借用の fix は、被演算子が場所の式なら `certain`、それ以外は `likely`、参照を借りるものは fix なし。参照外しは常に `certain`。** 理由: 記号の借用だけが期待する型を渡し（`&[]`）、`&r` は参照への参照。却下: すべて `certain`（`ref []` が `E1004`）、すべて `likely`（移行が進まない）。
- **D8: `W1007` と `E0004` の kind は別にする（`_rejected` を足す）。** 理由: D-51 は「1 つの kind は 1 つの code に属す」。種の「同じ kind」からの逸脱。
- **D9: `E0004` は fix を持たない。** 理由: 構文解析の段では借用の fix の確度が決まらない。移行は `2026` のうちに `W1007` の fix で行う。却下: 構文だけで決まる 4 種類の fix を足す（fix があるものとないものが混ざる）。
- **D10: `W1007` は根のパッケージの User のソースだけに出す。** 理由: 依存と std は利用者が直せない（W1005 と同じ）。
- **D11: 旧ラムダの連鎖は `\x y ->` に畳む。括弧で包まれた本体は fix なし。** 理由: `\x -> \y ->` は IR が変わる（実測）。
- **D12: `--edition` はマニフェストがあるとき `E2000`（値が同じでも）。** 理由: 真実の源を 1 つにする。種の「矛盾したら」より厳しい。
- **D13: Phase 1〜4 を同じリリースに入れる。** 理由: `docs/stability.md` の凍結の規則。
- **D14: std・fixture・既存の文書の例を書き換えない。新しく書く例は正準形。** 理由: `LATEST` は `2026` で、書き換えは 100 行を超え、移行の試験の材料でもある。

## 付録

実測・診断の例・コーパスの数・同義表記・アルゴリズムの詳細・値の表・テストの表は [A22-appendix.md](A22-appendix.md)（付録 A〜G）。
