# M22: 本体のない `extern def` の直後の `@c_layout`・`@literal` が `E0002`（制約行）になる

| 項目 | 内容 |
| --- | --- |
| ID | M22 |
| 優先度 | P1（本体のない `extern def` の直後に `@c_layout` の record か `@literal` の定数を書くと、正しいプログラムが関係のない `E0002`（`constraint lines must be indented after the 'def' signature`）で拒否される。回避策は宣言の順を入れ替えるか、間に `///` の doc を挟むことだが、診断はそれを示さない。属性を足すチケット（W16・W17・M01・M17・M20）は、そのたびに同じ一覧へ名前を足す必要がある） |
| 規模 | S |
| 依存 | – |
| 後続 | –（弱い後続: [M01](M01-objc-class-types.md), [M17](M17-macos-min-version-and-weak-import.md), [M20](M20-macos-framework-link-inputs.md)。[W16](W16-last-error-attribute.md)・[W17](W17-link-attribute-dllimport.md) の `signature` の先読みの段も要らなくなる） |
| 状態 | done |
| 起票 | 2026-11-26（D-67。利用者の依頼「「どのチケットにもない壁」と「調査中に見つけたコンパイラの不具合（自分で再現済み）」のそれぞれについて @_features/ に開発チケットまたは修正チケットを作成して」から起票。調査で報告した「調査中に見つけたコンパイラの不具合 B2（自分で再現済み）」に当たる。HEAD `804b7970` で確認） |
| 承認 | 不要（正しいプログラムを受理する構文の修正。新しい名前・診断コード・kind を足さない）。台帳は [GUIDE D-67](GUIDE.md#d-67-tsuzuri-だけで-macos-のデスクトップアプリを書くm01m22)（承認の要らない M22 はすぐに進めてよい。属性を足す M01・M17・M20 の Phase 2・W16・W17 より先にマージする） |
| 手本にする既存実装 | `src/parser.rs` の `Parser::signature` の先読み（2389-2401 行。次の宣言の属性で signature を終える）と制約行の検査（2402-2410 行）、`Parser::attributes`（1408 行。トップレベルの属性の読み）、`tests/effects_attributes.rs` の `an_attribute_after_constraint_lines_starts_the_next_declaration`（#63 のレビューで足した同じ種類の検査） |
| 主な影響ファイル | `src/parser.rs`、`tests/effects_attributes.rs`、`tests/alias.rs`、`_features/W16-last-error-attribute.md`、`_features/W17-link-attribute-dllimport.md` |

## 実装者向け早見

- 範囲: 本体のない signature（`extern def`、本体を後の `fn` で書くトップレベルの `def`、クラスの本体の `def m :: T`）の直後の行の `@`。
- 原因: `Parser::signature` は、signature の後の `@` を、効果の属性と `cpu`・`trusted`・`count` の名前のときだけ次の宣言の属性として抜け、それ以外は制約行として読む。`c_layout`・`literal`・`alias`・`checked` は一覧にないので、制約行の字下げの誤り（`E0002` `constraint_line_indentation`）になる。
- 直し方: 名前の一覧をやめ、`@` の次のトークンの種類で決める。新しい行で `def` の列以下にある `@` + 識別子は次の宣言のものとして抜ける。`@` + 型変数は今のとおり制約行として検査する。
- 最初に読む: 「現状」の表 → `src/parser.rs` 2364 行の `Parser::signature`（2387 行の `constraint_column`、2388 行の `while`、2391-2398 行の条件）→ 呼び出し元 887 行（クラスの本体）・1009 行（トップレベル）・4897 行（`extern`）→ 1408 行の `attributes`、1782 行の `literal_attribute_ahead`、1709 行の `alias_ahead`。
- 最初に書くテスト: `tests/effects_attributes.rs` の `attributes_after_a_bodyless_signature_start_the_next_declaration`（「現状」の b1〜b5・b7 を受理し、c1・c3・b8〜b11・k2 の診断を固定する）。
- 触ってはいけない: 制約行の構文と検査（字下げの足りない `@'a` は今と同じ `E0002` `constraint_line_indentation`）、`def` より深い `@` + 識別子の診断、今受理しているプログラムの構文木と IR、`FRONTEND_FORMAT`（構文木の形は変わらない）。
- 完了の見分け方: 次のプログラムの `check` が終了コード 0 になる（今は `3:1: error[E0002]: constraint lines must be indented after the 'def' signature`）。

```tsuzuri
@trusted
extern "abs" def c_abs :: i32 -> i32
@c_layout
record P { x: i32, y: i32 }

def main :: unit -> i32 = \() -> c_abs (0 - 5)
```

## 目的

`extern def` は本体を持たない。その直後の行に次の宣言の属性を書くのは、FFI の宣言を並べるときのふつうの書き方である（`@trusted` の extern の次に `@c_layout` の record を置く、など）。ところが今のパーサーは、signature の後の `@` を、知っている属性の名前の一覧と照らして読み分ける。一覧にない `@c_layout`・`@literal` は前の宣言の制約行と読まれ、関係のない字下げの誤りで拒否される。

一覧の方式は、新しい属性を足すたびに同じ一覧へ名前を足すことを求める。W16（`@last_error`）・W17（`@link`）・M01（`@returns_retained`）・M17（`@weak`）・M20 の Phase 2（`@link(framework = …)`）は、どれもその手順と「足し忘れると同じ不具合になる」という落とし穴を書いている。このチケットは、読み分けを名前ではなくトークンの種類（型変数か識別子か）で行い、不具合と将来の足し忘れを同時になくす。

## 着手条件と停止条件

### 着手条件

- `node scripts/repo-kb.mjs brief M22`。承認は要らない（D-67 で M22 はすぐに進めてよい）。
- 「現状」の表を HEAD で繰り返し、同じ結果になることを確かめる。`grep -n 'fn signature(\|while self.at(&TokenKind::At) {' src/parser.rs` が 1 件ずつ当たり、条件の本文が「現状」と一致すること。
- W16・W17・M01・M17・M20 のどれかが先に `main` に入っていたら、それが `signature` の先読みに足した名前（`last_error`・`link`・`returns_retained`・`returns_not_retained`・`weak` など）を確かめる。このチケットは一覧ごと置き換えるので、足された名前も消える（D4）。

### 停止条件

- **S1** 今受理しているプログラムの構文木・IR・診断（警告を含む）が 1 byte でも変わる。対象は `tests/fixtures`・`examples` の `Main.tz`（native と `--target wasm32`）、`tests/conformance` の全件、「現状」の a1〜a6・c2。
- **S2** 拒否していたプログラムで、診断（位置・コード・文・kind）が「仕様」の 6 の規則と違う形に変わるものが見つかる。規則は、本体のない signature の後の `def` の列以下の `@` + 識別子の行が、前の宣言に本体があるときの同じ行（「現状」の r8〜r11）と同じ診断になり、クラスの本体では k2 が k1 と同じになることである。
- **S3** 修正が `Parser::signature` の先読みの条件の外（制約行の構文、トップレベルの属性の読み、クラスの本体の読み）に及ぶ。
- **S4** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `804b7970` で確認）

開発版 `target/debug/tsuzuri check --json`（macOS 27.2.1 arm64）で確かめた。作業は `target/scratch/m22/` で行い、確認後に消した。行は `/` で区切って書く。特に書かない限り、プロジェクトの `Main.tz` で、最後に `def main :: unit -> i32 = \() -> c_abs (0 - 5)` がある。「本体のない extern」は `@trusted` / `extern "abs" def c_abs :: i32 -> i32` の 2 行を指す。

| # | コード | 今の結果 |
| --- | --- | --- |
| b1 | 本体のない extern / `@c_layout` / `record P { x: i32, y: i32 }`（早見のプログラム） | `3:1: error[E0002]: constraint lines must be indented after the 'def' signature`（kind `constraint_line_indentation`） |
| b2 | b1 の 3 行目を `@c_layout(pack = 1)` | 同じ（`3:1`） |
| b3 | b1 の 3・4 行目を `@literal def K :: i64 = 3` | 同じ（`3:1`） |
| b4 | b1 の 2 行目と 3 行目の間に空行 | 同じ（`4:1`） |
| b5 | 本体を後で書くトップレベルの `def id :: i32 -> i32` / `@c_layout` / `record P { x: i32, y: i32 }` / `fn id x = x`（main は `id 0`） | 同じ（`2:1`） |
| b6 | `.tc` の `def Return :: own 'a -> 'a` / `@alias ident` / `fn Return value = value`（と `Bind`）、`Main.tz` は `ident { let! x = 3; return x - 3 }` | `Ident.tc:2:1` の同じ誤り |
| b7 | 本体のない extern / `@checked (c_abs (0 - 5) + 1)`（トップレベルのコードの最初の式。main はない） | 同じ（`3:1`） |
| b8 | 本体のない extern / `@foo`（知らない属性）/ `def main …` | 同じ（`3:1`、`constraint_line_indentation`） |
| b9 | 本体のない extern / `@requires true` / `def main …` | 同じ（`3:1`、`constraint_line_indentation`） |
| b10 | 本体のない extern / `@alias foo`（`.tz` の中）/ `def main …` | 同じ（`3:1`、`constraint_line_indentation`） |
| b11 | 本体のない extern / `@c_layout` / `def f :: i32 -> i32 = \x -> x` / `def main …` | 同じ（`3:1`、`constraint_line_indentation`） |
| d2 | b8 から本体のない extern を除いた形（ファイルの先頭の `@foo`） | `1:1: error[E0002]: expected an expression; '@checked' is the only attribute of an expression`（kind `expected_expression_after_attribute`） |
| a1 | 本体のない extern / `@trusted` / `extern "labs" def c_labs :: i64 -> i64` | 受理する |
| a2 | 本体のない extern / `@no_alloc` / `def f :: i32 -> i32 = \x -> x` | 受理する |
| a3 | 本体のない extern / `@count(1, none)` / `@trusted` / `extern "memset" def c_memset :: ref mut [i32u] -> i32 -> i64u -> i64` | 受理する |
| a4 | 本体のない extern / `@cpu ["avx2"]` / `def f :: i32 -> i32 = \x -> x` | 受理する |
| a5 | b1 の 2 行目と 3 行目の間に `/// A point.` | 受理する（doc のトークンで signature が終わる） |
| a6 | b6 の `@alias ident` を `fn Return value = value` の後に移した形 | 受理する |
| c1 | `def id :: 'a -> 'a` / `@'a : Eq`（字下げなし）/ `fn id x = x` | `2:1: error[E0002]: constraint lines must be indented after the 'def' signature` |
| c2 | c1 の制約行を `    @'a : Eq` に字下げした形 | 受理する |
| c3 | 本体のない extern / `    @trusted`（`def` より深い）/ `extern "labs" def c_labs :: i64 -> i64` | `3:6: error[E0002]: expected a type variable such as 'a`（kind `expected_token`） |
| k1 | クラス `class Sized<'a> {` / `    def size :: 'a -> i64` / `    @trusted` / `    def other :: 'a -> i64` / `}` | `3:5: error[E0002]: expected a 'fn' or 'let' method definition, or '}'`（kind `expected_token`） |
| k2 | k1 の `@trusted` を `@c_layout` | `3:5: error[E0002]: constraint lines must be indented after the 'def' signature`（`constraint_line_indentation`） |

k1・k2 は `Main.tz` に書いたので、構文の誤りがなければ `E1018`（`class_outside_class_file`）になる形である。構文の誤りが先に出る。

前の宣言に本体があるときの同じ行（比べるための形。レビューで確かめた）: 本体のない extern の 2 行を `def g :: i32 -> i32 = \x -> x` の 1 行に替えたもの。行番号は 1 つ小さい。

| # | `def g …` の次の行から | 今の結果 |
| --- | --- | --- |
| r8 | `@foo` / `def main …` | `2:1: error[E0002]: expected an expression; '@checked' is the only attribute of an expression`（d2 と同じ） |
| r9 | `@requires true` / `def main …` | `2:1: error[E1033]: a contract line must start a function body: write '@requires' and '@ensures' on their own lines right after '=' or '->', before the first statement`（kind `contract_misplaced`） |
| r10 | `@alias foo` / `def main …` | `2:8: error[E1018]: '@alias foo' names a computation builder, so it belongs in a .tc file; move it to the builder's .tc file or remove it`（kind `alias_outside_builder_file`。位置は名前） |
| r11 | `@c_layout` / `def f :: i32 -> i32 = \x -> x` / `def main …` | `2:1: error[E0002]: '@c_layout' applies to a record declaration`（kind `misplaced_c_layout_attribute`） |

同じように、`record` の前の `@literal` は次の行の `expected_token`、`def` の前の `@checked` は次の行の `expected_expression`、`@old` は `E1033`（`contract_old_misplaced`）になる。本体のない extern の後では、今はどれも `3:1` の `constraint_line_indentation` である。

### 原因（コード）

- `src/parser.rs:2364` の `Parser::signature(name, exported, column, pure)`: 型を読んだ後、2387 行で `constraint_column`（signature の次のトークンの列）を取り、2388 行の `while self.at(&TokenKind::At)` で制約行を読む。
- 2389-2401 行: 新しい行で（`newline_before_current`）、`@` の列が `def` の列（引数 `column`）以下で、かつ次の 2 つのどちらかなら `break` する。`effect_attribute_ahead()`（1640 行。`@` の次が効果の属性の名前）か、`@` の次が `Ident` で名前が `"cpu" | "trusted" | "count"`（2397 行）。
- 2402-2410 行: それ以外の `@` は制約行として扱う。改行がないか、`constraint_column` が `def` の列以下か、`@` の列が `constraint_column` と違えば、`ConstraintLineIndentation`（`constraint lines must be indented after the 'def' signature`）を返す。字下げが合えば `@` を取り、`type_variable`（2136 行。`TokenKind::TypeVariable` を期待する）を読む。c3 の `expected a type variable such as 'a` はここで出る。
- トップレベルでは、`attributes`（1408 行）が効果の属性・`@trusted`・`@count`・`@c_layout`（1501 行の `c_layout_attribute`）・`@cpu` を読み、`literal_attribute_ahead`（1782 行）が `@literal def` を、`alias_ahead`（1709 行）が `.tc` の `@alias` を読む。`@checked` は式の属性で、ほかの `@名前` は `src/parse_control.rs:903` の `expected an expression; '@checked' is the only attribute of an expression` になる。先読みの一覧は、このうち `@c_layout`・`@literal`・`@alias`・`@checked` を持たない。
- 制約行の `@` の次は必ず型変数（`TokenKind::TypeVariable`。字句は `'a`）で、属性の名前はどれも `TokenKind::Ident` である。
- `signature` の呼び出し元は 3 つ: クラスの本体の `def m :: T`（887 行）、トップレベルの `def`（1009 行）、`extern`（4897 行）。b5 と b6 はトップレベルの `def` の経路、k2 はクラスの本体の経路で、同じ条件に当たる。
- 一覧は `cpu`・`trusted` から始まり、W13 の commit `da1073d2` が `count` を足した（`git log -S'"cpu" | "trusted" | "count"' -- src/parser.rs`）。

### 一覧に名前を足す計画を書いているチケット

- W16:122・229（`last_error`）と W17:122・238（`link`）は、「`signature` の次の宣言の属性の先読みに足す」手順と、足し忘れたときの落とし穴を書いている。起票の PR で、両方に「M22 が先に入っていれば足さなくてよい」という参照を足した。
- W16:57 は「属性の読み込み口」で今の先読み（`"cpu" | "trusted" | "count"` と効果の属性）を説明し、W16:20 は「最初に読む」に `signature` の先読みを挙げている。M22 の後は、どちらも今の規則と合わなくなる（手順 4）。
- M01（`@returns_retained`・`@returns_not_retained`）・M17（`@weak`）・M20 の Phase 2（`@link(framework = …)`）は、M22 を弱い依存にし、「M22 が後なら先読みに足す」と書いている。

## 変えてはいけない意味

- 制約行は `def` の signature の次の行に、`def` より深く字下げして書く（`attributes.md` の「制約行は属性ではない」）。字下げが足りない `@'a` は `E0002`（`constraint_line_indentation`）のまま。
- 制約行の中の `@`（同じ列の 2 行目以降の制約行）と、制約行の後の次の宣言の属性（`tests/effects_attributes.rs` の `an_attribute_after_constraint_lines_starts_the_next_declaration`）の読み方。
- トップレベルの属性・`@literal`・`@alias`・`@checked` の読み方と診断。抜けた後は、それらがそのまま読む。

## 仕様

1. 本体のない signature の後で、新しい行の `@` が `def` の列以下にあり、`@` の次のトークンが識別子（`TokenKind::Ident`）なら、signature はそこで終わる。`@` は次の宣言かトップレベルのコードのものとして読む。属性の名前は見ない。
2. `@` の次が型変数なら制約行で、今の検査を続ける。字下げの誤りは `E0002`（`constraint_line_indentation`）。
3. `def` より深く字下げした `@` + 識別子は、今と同じく制約行の位置として読み、`expected a type variable such as 'a`（`E0002`、`expected_token`）になる（c3）。
4. 抜けた後の扱いは、同じ `@` をトップレベルに書いたときと同じである。属性なら次の宣言の属性（置き場の誤りは今の `misplaced_*` の診断）、`@checked` なら式、`.tc` の `@alias` なら別名（`.tz` では `E1018` の `alias_outside_builder_file`）、`@requires`・`@ensures`・`@old` なら契約の置き場の誤り（`E1033`）、知らない名前なら `expected an expression; '@checked' is the only attribute of an expression`。
5. クラスの本体の `def m :: T` も同じ規則である。クラスの本体に書ける属性はないので、メソッドの列の `@` + 識別子は、今の `@trusted`（k1）と同じ `expected a 'fn' or 'let' method definition, or '}'` になる。
6. 結果: b1〜b7 を受理する。拒否していたプログラムで、`def` の列以下の `@` + 識別子の行の診断は、前の宣言に本体があるときに同じ行を書いた場合（「現状」の r8〜r11）と同じになる。b8 は r8（`E0002`、`expected_expression_after_attribute`）、b9 は r9（`E1033`、`contract_misplaced`）、b10 は r10（`E1018`、`alias_outside_builder_file`。位置は `@` でなく名前）、b11 は r11（`E0002`、`misplaced_c_layout_attribute`）と同じになる。コード・kind・文・位置は、この規則のとおりに変わりうる。クラスの本体では、k2 が k1 と同じ `expected_token` になる。a1〜a6・c1〜c3・k1 は今と同じ。

## 設計

`src/parser.rs` の `Parser::signature` の 2391-2398 行の条件だけを変える。

```rust
// The attribute of the next declaration, at its column, after a signature without a body (and
// after its constraint lines, which are indented further): an identifier after '@', since a
// constraint line names a type variable (M22).
if self.newline_before_current()
    && self.column(self.current().span) <= column
    && matches!(
        self.tokens.get(self.position + 1).map(|token| &token.kind),
        Some(TokenKind::Ident(_))
    )
{
    break;
}
```

- `effect_attribute_ahead()` は `@` の次が `Ident` のときだけ `Some` を返すので、新しい条件に含まれる。`"cpu" | "trusted" | "count"` も同じ。
- 2402 行からの制約行の検査は変えない。型変数の `@`、深い字下げの `@`、同じ行の `@` は今と同じ経路を通る。
- 構文木の形は変わらないので、`src/syntax_codec.rs` と `FRONTEND_FORMAT` は変えない（フロントエンドのキャッシュの鍵はコンパイラの実行ファイルの同一性も含む。`src/frontend_cache.rs` の `executable_identity`）。

### 起票時の試作

起票時に、HEAD のソースの複製（`target/scratch/` の下。確認後に消した）に上の変更を当てて開発版を作り、次を確かめた。HEAD のコードには入っていない。

- b1〜b7 を受理した。b8 は `3:1` の `expected_expression_after_attribute`（d2 と同じ文）、k2 は k1 と同じ `3:5` の `expected_token` になった。a1〜a6・c1〜c3・k1 は今と同じだった。
- `tests/fixtures` の 152 個の `Main.tz` の `--emit llvm`（native）、`examples` の 9 個（native と `--target wasm32`）、`tests/conformance` の 256 個の `check --json` が、HEAD と 1 byte も変わらなかった。
- `cargo test --test effects_attributes --test trusted --test c_layout --test extern_ref_mut --test extern_view` がすべて通った。

レビューでも、HEAD のソースの複製（`git archive HEAD`、`target/scratch/` の下。確認後に消した）に同じ変更を当てた開発版で確かめ直した。b1〜b7 と a1〜a6・c2 を受理し、c1・c3・k1 は今と同じ、k2 は k1 と同じだった。b8〜b11 は、それぞれ r8〜r11 と同じコード・kind・文になった（位置は `3:1`、b10 だけ名前の `3:8`）。`record` の前の `@literal`、`def` の前の `@checked`、`@old` も、前の宣言に本体があるときと同じ診断になった。`tests/fixtures` の 152 個のうち 150 個の IR（残る `domain`・`domain_host` は HEAD でも試作でも同じ `E1038` で IR を出さない）、`examples` の 9 個（native と `--target wasm32`）の IR、`tests/conformance` の 256 個の `check --json` が HEAD と同じだった。

### 採らない案

- **一覧に `c_layout`・`literal`・`alias` を足す**: 今の不具合は直るが、W16・W17・M01・M17・M20 の属性のたびに同じ手順が要り、足し忘れると同じ不具合が戻る。`@checked`（b7）も足す必要がある。
- **`def` の列以下の `@` をすべて抜ける（次のトークンを見ない）**: 字下げの足りない制約行 `@'a : Eq`（c1）がトップレベルの `@'a` として読まれ、`constraint lines must be indented …` という的確な診断が `expected an expression …` に変わる（S2）。
- **知らない名前の `@foo` だけ今の `constraint_line_indentation` に残す**: 名前の一覧が要る方式に戻る。知らない属性の診断を良くするなら、トップレベルの `@foo`（d2）と合わせて別に行う（「対象外」）。

## 前提とする他チケットのインターフェース

- なし。

## 他チケットへ提供するインターフェース

- `Parser::signature` の規則: 本体のない signature の後の、`def` の列以下の `@` + 識別子は、名前によらず次の宣言のものとして抜ける。新しい属性（W16 の `@last_error`、W17 の `@link`、M01 の `@returns_retained`・`@returns_not_retained`、M17 の `@weak`、M20 の `@link(framework = …)`）は、`attributes` に読みを足すだけでよく、`signature` の先読みには何も足さない。
- このチケットが後に入った場合は、先に入ったチケットが一覧に足した名前を、一覧ごと消す（D4）。

## 実装手順

1. **変更**: なし（基準の記録）。
   **内容**: 着手条件の確認。HEAD の開発版で、`tests/fixtures`・`examples` の `Main.tz` の `build --emit llvm`（native、`examples` は `--target wasm32` も）と、`tests/conformance` の全件の `check --json` を `target/scratch/m22/before/` に保存する。「現状」の各行の `check --json` も保存する。
   **確認**: 保存した数を控える。
2. **変更**: `tests/effects_attributes.rs`、`tests/alias.rs`（テスト計画）。
   **内容**: 新しいテストを先に書く。
   **確認**: `cargo test --locked --test effects_attributes attributes_after_a_bodyless` と `cargo test --locked --test alias an_alias_after_a_bodyless` が `running 1 test` で失敗する。
3. **変更**: `src/parser.rs`（設計）。
   **内容**: 先読みの条件。
   **確認**: 手順 2 のテストが通る。T0。`cargo test --locked --test effects_attributes --test alias --test computations --test trusted --test c_layout`。手順 1 の集合と `cmp` で一致し、「現状」の行は仕様 6 のとおりにだけ変わる（S1・S2）。手順 1 の fixtures のうち IR を出さないもの（起票時は `domain`・`domain_host` の `E1038`）は、診断の文と終了コードで比べる。
4. **変更**: `_features/W16-last-error-attribute.md`（20・57・122・229 行）、`_features/W17-link-attribute-dllimport.md`（122・238 行）。先に入ったチケットが一覧に名前を足していたら `src/parser.rs` のその名前も。
   **内容**: 「`signature` の先読みに足す」段を、「M22 の後は `signature` の先読みは属性の名前を見ないので、足す段はない」に改める。落とし穴の行も同じく改める。W16:57 の「属性の読み込み口」は今の一覧（`"cpu" | "trusted" | "count"` と効果の属性）を説明しているので、M22 の後の規則（`def` の列以下の `@` + 識別子で抜ける）に書き直す。W16:20 の「`signature` の先読み」も、読む必要がなくなったことに合わせる。行番号は着手時に `rg -n 'signature' _features/W16-*.md _features/W17-*.md` で取り直す。
   **確認**: `node scripts/check-guide-lite.mjs`。T1、T2。`target/scratch/m22/` を消す。

## テスト計画

### Rust

- `tests/effects_attributes.rs` の `attributes_after_a_bodyless_signature_start_the_next_declaration`（既存の `accepts`・`rejects` を使う）:
  - 受理する: b1、b2、b3、b4、b5、b7。b1 の record を `record Q { tag: i8, x: i32 }` にし、続けて `@trusted` / `extern view def read_q :: i64 -> Q` を置いた形も受理する。`@c_layout` がなければ、この `extern view` は `E1008`（`extern_view_record`。8 bit の field）になるので、属性が record に付いたことの確かめになる（起票時に、HEAD で属性なしの形が `E1008`、試作で属性ありの形が受理されることを確かめた）。b3 では `K` を使う式（`c_abs (0 - 5) + (K as i32)`）が型検査を通る。
  - 今と同じに受理する: a1〜a5、c2。
  - 拒否する（今と同じ）: c1 は `E0002`・`constraint_line_indentation`、c3 は `E0002`・`expected_token`。
  - 拒否する（変わるもの。仕様 6）: b8 は `E0002`・`expected_expression_after_attribute`、b9 は `E1033`・`contract_misplaced`、b10 は `E1018`・`alias_outside_builder_file`、b11 は `E0002`・`misplaced_c_layout_attribute`、k2 は `E0002`・`expected_token`（k1 と同じ）。b8〜b11 は、r8〜r11 の形（前の宣言に本体がある）も同じ helper で拒否させ、コード・kind・文が同じことを確かめる。期待値は仕様から手で書き、コンパイラの出力を写さない（GUIDE-LITE §2 規則 10）。
- `tests/alias.rs`（D-43 の `@alias` のテスト）の `an_alias_after_a_bodyless_builder_signature`: 既存の `ir(&[("Main.tz", …), ("Ident.tc", …)])` の形で、b6 を受理し、`ident { … }` が型検査を通る。a6 の形も受理する。`.tc` は `builder(header)` を使わず、`def Return :: 'a -> 'a` の直後の行に `@alias ident` を書いた文を直接書く（`builder` は見出しを最初の `def` より前に置く）。
- 既存の `an_attribute_after_constraint_lines_starts_the_next_declaration` は変えずに通る。

### 既存のテストへの影響

- `tests/` と `tests/diagnostic_baseline` に、`constraint_line_indentation` や `constraint lines must be indented` を固定するものはない（起票時に `grep -rln` で確かめた）。`tests/effects_attributes.rs:275-276` の説明の行だけが言及する。

## ドキュメント

- 不要。`rg -n 'constraint lines' _tsuzuri docs` は当たらない。`constraint_line_indentation` は `compiler/diagnostics.md:347` の `E0002` の kind の一覧（343-415 行の `<!-- generated:diagnostic-kinds -->` の生成の区画。kind は消えないので変わらない）にだけ現れる。`values-and-functions/attributes.md` の「制約行は属性ではない」（352 行から）は「`def` の次の行に、より深くインデントして `@'a : Eq` と書くのは制約行」「インデントが足りないと `E0002`」と書き、修正の後もそのとおりである。PR の説明に、ページを変えない理由としてこれを書く。

## 受け入れ条件

- [ ] b1〜b7 を受理し、b8〜b11・k2 の診断が仕様 6 のとおりになり（b8〜b11 は r8〜r11 と同じ）、ほかの「現状」の行は今と同じ。
- [ ] 手順 1 の集合（fixtures・examples の IR、conformance の診断）が 1 byte も変わらない。
- [ ] W16・W17 の先読みの段を改めた。先に入ったチケットが足した名前があれば、`src/parser.rs` から消えている。
- [ ] 新しいテストと既存の全件、T1、T2 が通る。

## 落とし穴

- **クラスの本体も通る**: `signature` はクラスの本体の `def m :: T`（887 行）からも呼ばれ、`column` はメソッドの列である。メソッドの列の `@` + 識別子は抜けた後にクラスの本体の読みで誤りになる（k2）。テストに含める。
- **`@alias` を壊さない**: `.tc` の `@alias` はトップレベルの `alias_ahead` が読む。抜けた後に同じ経路に届くこと（b6）を `tests/alias.rs` の `analyze_modules` で確かめる。
- **型変数のトークン**: 制約行の `@` の次は `TokenKind::TypeVariable` である。`TokenKind::Ident` と取り違えない。`'a` を識別子として字句にするような字句解析の変更はしない。
- **空行と doc**: b4 のとおり、空行を挟んでも今は同じ誤りになる。a5 のとおり、`///` の doc は signature を終える。どちらも新しい規則で変わらない。

## 対象外

- 制約行の構文（位置、`@'a : C1, C2`、`(#f: T)` の形）の変更。
- 知らない属性の新しい診断。抜けた後の `@foo` は、トップレベルの `@foo`（d2）と同じ `expected an expression; '@checked' is the only attribute of an expression` になる。改善するなら別のチケットで、トップレベルと合わせて行う。
- クラスの本体に属性を書けるようにすること。
- 適合性の規則（`spec/rules/SYN.md`）の追加（D5）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | 先読みを、属性の名前の一覧から「`@` の次が識別子か」に改める。退けた案: 一覧に名前を足す（属性のたびに足し、忘れると同じ不具合）、`def` の列以下の `@` をすべて抜ける（c1 の的確な診断を失う） | 制約行の `@` の次は必ず型変数、属性の名前は必ず識別子で、トークンの種類だけで決まる。属性を足すチケットの手順が 1 つ減る | 既定案（実装者はこの案に従う） |
| D2 | 抜けた後の `@名前` は、前の宣言に本体があるときの同じ行と同じ診断にする。知らない `@foo`（b8）は `expected_expression_after_attribute`、`@requires`（b9）は `E1033`、`.tz` の `@alias`（b10）は `E1018`、置き場を誤った `@c_layout`（b11）は `misplaced_c_layout_attribute` になり、コード・kind・位置が今と変わる | 同じ `@` の行が、前の宣言に本体があるかどうかで別の誤りになるのは一貫しない。抜けた後はトップレベルの読みにそのまま任せるので、新しい診断は足さず、名前ごとの分岐も要らない | 既定案（実装者はこの案に従う） |
| D3 | クラスの本体の `def m :: T` にも同じ規則を当てる（k2 の kind が変わる） | `signature` を共有する 3 つの経路で規則を分けると、`column` の意味に応じた分岐が要る。クラスの本体に書ける属性はなく、今の `@trusted`（k1）と同じ誤りにそろう | 既定案（実装者はこの案に従う） |
| D4 | 同じ PR で W16・W17 の先読みの段を改める。W16・W17・M01・M17・M20 が先に一覧へ名前を足していたら、一覧ごと消す | 計画の文書と実装の食い違いを残さない（README の「綴りを変えるときは関係するチケットを同じ PR で直す」） | 既定案（実装者はこの案に従う） |
| D5 | 適合性の規則は足さず、Rust のテストで固定する。言語リファレンスも変えない | 構文の規則（制約行は深く字下げした `@'a`、属性はそれ以外）は `attributes.md` の今の記述どおりで、変わるのは実装だけである。規則を足すと文書に印（`<!-- rule: … -->`）が要り、文書を変えない修正の範囲を超える | 既定案（実装者はこの案に従う） |
