# G23: 診断の kind・`tsuzuri explain`・ヒント DB

| 項目 | 内容 |
| --- | --- |
| ID | G23 |
| 優先度 | P1（D-50 の表） |
| 規模 | L。Phase 1 = M（3〜5 日）、Phase 2 = L（1〜3 週。機械的な移行）、Phase 3 = M、Phase 4 = S |
| 依存 | [G22](G22-diagnostic-v2.md) Phase 1（`Diagnostic` の `kind`・`help`・`fixes` と JSON の形。D-51）。Phase 1・2 が使うのは `kind` だけで、G22 が未完了なら手順 1.1 が足す。**Phase 3 は G22 Phase 1 が済んでいなければ着手しない** |
| 後続 | G21（Phase 3 の `hints::TABLE` からガイドの「よくある誤り」を作る）、G29（`tz_explain`）、G35（kind の一覧を文書へ流し込む）、A22（`E0004` の kind を登録する）。第3期の新しい診断は、すべて kind を登録する（D5） |
| 状態 | todo |
| 起票 | 2026-10-18（D-50。`src/`・`tests/` は `a2313d19` と同一の HEAD `9dd0fbbc` で確認。両者の差は `docs/benchmarks.md` だけ） |
| 承認 | 承認済み（D-50: 人間の包括許可 2026-10-18）。決定事項 D1〜D16 はすべて承認済み |
| 改善する劣位 | Rust 比（`rustc --explain`・lint 名）・C#/F# 比（診断 ID と説明）: Tsuzuri の診断は粗いコード 47 種だけで、同じ `E1002` が「未定義の名前」「モジュールにない関数」「型クラスにないメソッド」を表す。原因を引く鍵（kind）も説明（`explain`）もなく、AI は同じ誤り（`pair.0`・`Display.to_string`・`let new = 1`）を繰り返し、診断は「何が違うか」だけを言い「どう書くか」を言わない（D-50 の調査結論 (1)） |
| 手本にする既存実装 | サブコマンド: `src/main.rs` の `api_diff_command`・`api_diff_usage`（`raw.first() == "api-diff"` の分岐）と `tests/api_diff.rs`。表示: `print_diagnostic`、`src/diagnostic.rs` の `json_with_severity`・`json_string`。`include_str!` の表: `src/stdlib.rs` の `SOURCES`。`macro_rules!`: `src/syntax_codec.rs` の `integers!`。トークン: `src/lexer.rs` の `lex_all`、`src/syntax.rs` の `Token`・`TokenKind`。LSP: `src/lsp.rs` の `diagnostic_json`・`Session::handle` の `textDocument/hover` |
| 主な影響ファイル | **新規**: `src/diagnostic_kinds.rs`、`src/explain.rs`、`src/explain/<kind>.md`、`src/explain/codes/<CODE>.md`、`src/hints.rs`、`tests/diagnostic_kinds.rs`、`tests/explain.rs`、`tests/hints.rs`。**変更**: `src/lib.rs`、`src/diagnostic.rs`、`src/main.rs`、`src/lsp.rs`（Phase 4）と、発行箇所のある 44 ファイル（付録 C）、`tests/diagnostics.rs`・`tests/warnings.rs`、`docs/stability.md`・`docs/architecture.md`・`docs/language.md`、`README.md`、`_tsuzuri/language-reference/compiler/{diagnostics,usage,option}.md` |

付録 A〜F（ページ 8 本・登録簿・発行箇所の表・ヒント 40 件・補助関数と実測・Phase 3/4 の手順）は末尾の表にある。

## 実装者向け早見

- 最初に読む: 「仕様」の S1〜S6、「実装手順」の手順 1.1〜1.7、付録 A（Phase 1 のページ 8 本）。Phase 2 は付録 C と E の表を見ながら進める。
- 最初に書くテスト: `tests/diagnostic_kinds.rs::registry_is_consistent`（手順 1.2。kind の id・コード・重複を検査する）。
- **触ってはいけないもの**: 既存の診断の `code`・`message`・`span`・`severity`、重複除去のキー、`--json` の既存の 5 キー、終了コード。この変更は**診断に `kind` を足すだけ**で、`kind` キーを持たない出力は 1 byte も変わらない（Phase 1・2 の途中で移行していない発行箇所を含む）。
- 着手する Phase は人間の指示に従う（指示がなければ Phase 1 だけ）。
- Phase 1 の完了の見分け: `target/release/tsuzuri explain use_after_move` が付録 A.1 のページを出し、`tsuzuri check --json` の `E1012` の行の末尾に `,"kind":"use_after_move"` が付く。ほかの診断の JSON は変わらない。
- Phase 2 の完了の見分け: `Diagnostic::new`・`Diagnostic::warning` が存在せず（コンパイラが保証）、`tsuzuri explain --catalog` が 653 行、全コードの説明ページがあり、厳選 84 kind の専用ページがある。
- Phase 3 の完了の見分け: `check --json` で `let x = (5: i64)` の診断に `"help":"annotate the binding instead, as in 'let x: i64 = 5'"` が付く。Phase 4: LSP の hover が診断の範囲で kind と `explain` の案内を返し、`tests/explain.rs::the_docs_list_every_kind` が緑。
- 小さく進む: 1 手順の変更は 300 行以内を目安にし、各手順の最後に `cargo test --locked`・`cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings` を通す。

## 目的

**問題。** HEAD の診断は `code`（47 種）・`message`・位置だけで、原因を表す安定した識別子がない。実測（「現状」）: 発行箇所の行は **1,077**（直接の `Diagnostic::new` が 444、`Diagnostic::warning` が 8、補助関数の呼び出しと手で見つけた箇所が 625）。たとえば `E0002` 1 つが 115 種類の構文の誤りを表す。AI は診断のメッセージを読んで直すが、同じ誤りの形（`pair.0` の読み方、`Display.to_string`、Rust 風の `match x { 1 => 2 }`）を繰り返す。`tsuzuri explain` はなく（`tsuzuri explain use_after_move` は今、`E2000` で `pass one .tz, .tt, or .tc file...` を返す）、よくある誤りの「どう書くか」を答える表もない。

**価値。** (1) 診断ごとに**安定した `kind`**（例: `use_after_move`）が付き、`--json`・LSP・KB・MCP が原因で引ける。(2) `tsuzuri explain <kind|コード>` が、原因・悪い例・良い例（どちらもテストで検証済み）を出す。(3) **ヒント DB**（40 件）が、構文の誤り・Rust 風の書き方のうち HEAD で診断になるものに、`help`（と `likely`／`maybe` の `fix`）を付け、同じ表が G21 のガイドの生成元になる。

**数字（HEAD `9dd0fbbc` で測定。コマンドは「現状」）。** 発行箇所の行 1,077、kind 653（付録 C の行から 652、本チケットが足す `unknown_explain_target` が 1）、コード 47、行を持つファイル 44、1 行だけの kind 517。`E2000`（CLI の引数・組み合わせ）は 48 kind・231 行で、最も多い kind は `option_requires_build`（54 行）。

**非目標。**
- メッセージの文言・コード・位置・重要度の変更（直してよいが本チケットでは触らない）。
- `fix` の適用（G24）、関連位置・`related`（G22）、型つき穴と候補（G25）。
- 説明の多言語化（ページは英語。D-56）、LLM による説明の生成、全 kind の専用ページ（厳選 84 と全コードだけ）。
- 成功するコンパイルの速度の変更（診断は失敗した経路だけ）。

## 着手条件と停止条件

### 着手条件

- [G22](G22-diagnostic-v2.md) Phase 1 が済んでいるか確かめる: `grep -n "pub kind: Option<&'static str>" src/diagnostic.rs`。1 件なら G22 の形（D-51）で `kind`・`help`・`fixes` がある。0 件なら Phase 1・2 は手順 1.1 で `kind` だけを足して進める。**Phase 3 は `help`・`fixes`・`Applicability`・`Edit` が要るので、0 件のまま着手しない**。
- ベースライン: `cargo build --release --locked && cargo test --locked` の通過件数を控える（手順 1.1。失敗があれば本チケットと無関係であることを記録する）。
- 読む: [GUIDE-LITE](GUIDE-LITE.md)、GUIDE の D-51・D-55・D-56、`docs/stability.md` の「診断のコード・重要度・JSON のキー」、本ファイルの「仕様」と付録 C の冒頭の表。
- 承認: 不要（新しい crate・`unsafe`・予約語・既定の WASM の import を足さない）。

### 停止条件

次のどれかに当てはまったら、即興で回避せず止めて報告する。

1. `grep -n "pub kind" src/diagnostic.rs` が 0 件で、G22 が別の名前・型で `Diagnostic` を広げていた。→ 選択肢: A（G22 の形に合わせて手順 1.1 を変える）、B（G22 を先に完了させる）。
2. Phase 2 で、付録 C の行と HEAD のソースが合わない箇所が 1 手順で 10 行を超えた（HEAD が進んだ）。→ 付録 C を作り直すか差分だけ直すかを報告して選ぶ。
3. 付録 C にない発行箇所を `deprecated` のエラーが指した。→ 規則 D5 で kind を決めて登録簿と付録 C へ足す。1 手順で 10 行を超えたら止める。
4. 移行（Phase 2）で、既存テストの期待値（`code`・`message`・`span`）を変えないと通らない。→ kind の選び間違いか表の誤り。表を直す（期待値は変えない）。2 回続けて直せなければ止める。
5. `#![deny(deprecated)]` を置いたモジュールで、`Diagnostic::new` 以外の `deprecated` が出る。→ 補助関数を `#[deprecated]` にしてよいのは付録 E.1 に載せたものだけ。それ以外は止める。
6. 登録簿の導入で `cargo build --release` の時間が 20% 以上増えた、または `target/release/tsuzuri` が 1 MiB 以上増えた。→ 止めて報告（選択肢: ページの埋め込みを減らす、`from_id` を二分探索にする）。
7. `explain` のページの `Bad` が、診断のコードやメッセージを変えないと指定の kind を出さない。→ ページ（例）を直す。診断側は変えない。
8. ヒントの `patterns` が実トークンと合わず、1 つのヒントで 3 回直しても `bad` に付かない。→ そのヒントを外して報告する（表から消し、付録 D に理由を書く）。
9. G22 の JSON のキーの順序が D-51 と違う、または `help`・`fixes` の型が D-51 と違う（Phase 3 が使えない）。→ 止める。
10. hover の変更で、`tests/lsp_sessions.mjs` の `stale` のケース（手順 4.1 で位置を変える 1 か所）以外の既存の期待値を変えたくなった。→ 止める（hover は診断の範囲にあるときだけ、ブロックを足す）。
11. 新しい crate・`unsafe`・予約語・既定の WASM の import が必要に見える。→ 止める。

## 現状（HEAD `9dd0fbbc` で確認）

### 診断の型と発行

`src/diagnostic.rs` の `Diagnostic` は 4 フィールドだけ: `severity: Severity`・`code: &'static str`・`message: String`・`span: Span`。作る関数は `Diagnostic::new(code, message, span)`（エラー）と `Diagnostic::warning(code, message, span)`（警告）。`json_with_severity` が `{"severity","code","message","path","span":{start,end,line,column,end_line,end_column}}` の 1 行を作り、`Diagnostics::push` が重複を `(source, start, end, severity, code, message)` で除く。`kind` も `related`・`help`・`fixes` もない（G22 が足す）。

### 数え方と数（再現できるコマンド）

| 何を | コマンド | 結果 |
| --- | --- | --- |
| `Diagnostic::new(`／`Diagnostic::warning(` | `grep -rn 'Diagnostic::new(' src \| wc -l`（`warning` も同様） | 475（inline テスト 4 を含む）／ 8 |
| `driver_error(`・`io_error(` の呼び出し（`src/driver.rs` の補助） | `grep -rn 'driver_error(' src \| grep -v 'fn driver_error' \| wc -l`（`io_error` も同様） | 182／113（`io_error` は固定コード `E2001` で、呼び出しは行にしない） |
| `self.error(`（`parser.rs`・`parse_control.rs`） | `grep -n 'self\.error(' src/parser.rs src/parse_control.rs \| wc -l` | 75 |
| コードのリテラル、テストの構築、CLI の文字列エラー | 付録 E.3 | 879 個、7 行（`tests/`）、91 行（`src/main.rs`。`Result<_, String>` の 5 関数） |

**付録 C の行の定義。** 診断をどの条件で作るかが決まる場所を 1 行にした（`Diagnostic::new`／`warning` の直接の呼び出し、コードを引数に取る補助とコードを内側に持つ補助の呼び出し、分岐ごと、手で見つけた箇所）。形ごとの意味と直し方は付録 C の冒頭の表、数え方は付録 E.3。数は **1,077 行 = `new` 444 + `driver_error` 182 + `manual` 122 + `par` 74 + `failure` 58 + `unsupported` 51 + `error` 45 + `Line::error` 34 + `Location::error` 22 + 分岐 17 + closure 11 + `rec_error` 9 + `warning` 8**。

### 実測した出力（`target/release/tsuzuri`）

```text
$ tsuzuri check use_after_move-bad            # `text` は `to_string text` で移動済み
use_after_move-bad/Main.tz:4:5: error[E1012]: use of moved or partially moved value 'text'
  4 |     text.length + copy.length
    |     ^
$ tsuzuri check use_after_move-bad --json
{"severity":"error","code":"E1012","message":"use of moved or partially moved value 'text'","path":"use_after_move-bad/Main.tz","span":{"start":90,"end":94,"line":4,"column":5,"end_line":4,"end_column":9}}
```

### 誤りの形を HEAD に与えた結果（付録 E.4 に全件）

`tsuzuri check --json` に、AI が書きがちな誤り 89 件（付録 E.4 に全件）を与えた。63 件が診断になり（ヒントの対象。付録 D の 40 件）、26 件は受理された（診断が出ないのでヒントを付けない）。受理される例: `fn add(a: i64, b: i64) -> i64 { a + b }`・`return x`・`a + "b"`・`Some(x)`・`g(1, 2)`・`$"v={x}"`・`a != 1`・`n -> n`・`const X: i64 = 5`。診断になる例（kind は付録 C）:

| ソース | 結果 | kind |
| --- | --- | --- |
| `pair.0` | `E0002` `expected an identifier`（`0` の位置） | `expected_identifier` |
| `(5: i64)` | `E0002` `expected the closing delimiter`（`:` の位置） | `expected_token` |
| `Display.to_string 5` | `E1002` `type class 'Display' has no method 'to_string'` | `class_has_no_method` |

### コードを引数に取る補助・特別な箇所（付録 E.1・E.2）

`Diagnostic` を作る形は一様でない: 作った診断のコードを**書き換える**（`error.code = "E1015"`）箇所が 4（`check.rs` 1・`polymorph.rs` 3）、コードを**条件で選ぶ**箇所が 2（`check.rs` の `record_arity`、`polymorph.rs` の `solve_families`）、`(コード, メッセージ)` の**組を返す**関数が 1（`fetch.rs` の `parse_tree`。10 行）、**手で作った JSON の 1 行**が 5（`main.rs`）、位置から元ファイルを引くためだけに作る**空の診断**（`Diagnostic::new("E2000", "", case.span)`）が 2（`main.rs`。`Project::source_for` が `&Diagnostic` を取るため）。全 22 項目は付録 E.2。

## 変えてはいけない意味

| 対象 | 約束 |
| --- | --- |
| 既存の診断の `code`・`message`・`span`・`severity`・出る順序・重複除去のキー | 1 byte も変えない。移行（Phase 2）は `Diagnostic::new("E1012", …)` を `Diagnostic::of(DiagnosticKind::UseAfterMove, …)` に置き換えるだけで、同じコードと同じメッセージを出す。kind の `code` は元のコードと同じ |
| `--json` の既存 5 キー（`severity`・`code`・`message`・`path`・`span`） | 消さず・改名せず・順序を変えない。`kind` は `span` の次（D-51 のキーの順序）に、値があるときだけ足す。**`kind` を持たない診断の JSON は 1 byte も変わらない** |
| 人間向けのテキスト出力 | 変えない（行を足さない。`explain` の案内は載せない。D8） |
| 終了コード | 成功 0・コンパイルや実行の失敗 1・引数の誤り 2。`explain` は成功 0・引数の誤り（不明な対象を含む）2 |
| 生成 IR・ABI・WASM の import・実行結果 | 変わらない（診断の経路だけを変える。確認: `tests/` の IR 比較と `node tests/features.mjs` が緑） |
| 成功するコンパイルの速度 | 変えない（ヒントは失敗した経路の終わりで 1 回、`kind` は `&'static str`） |
| コードの意味（D-16） | 変えない。1 つの kind は 1 つのコードに属す |
| `tsuzuri explain` の説明ページの文言 | 保証の対象外（直してよい）。JSON のキーは保証する（足すだけ） |

## Phase 分割

| Phase | 内容 | 規模 | 前提 | 単独で出荷できる理由 |
| --- | --- | --- | --- | --- |
| 1 | `DiagnosticKind` の登録簿（最初の 12 kind＝ページ 8 本の kind と `explain` のエラーの 4 kind）、`Diagnostic::of`・`recode`、その 9 箇所の発行箇所の移行、`tsuzuri explain`（kind・コード・`--catalog`・`--json`）、ページ 8 本、文書 | M | G22 Phase 1 の `kind`（なければ手順 1.1 が足す） | `check` の診断は 9 箇所だけ `kind` が付く。`explain` は登録簿にある kind とコードだけを説明する。ほかの診断は何も変わらない |
| 2 | 全 1,077 行の移行（登録簿を 653 kind に広げる）、`Diagnostic::new`／`warning` の削除、全 47 コードの説明ページ、厳選 84 kind の専用ページ（Phase 1 の 8 を含む）、文書 | L | Phase 1 | モジュール単位で出荷できる（移行したモジュールの診断から `kind` が付く）。最後の手順で旧 API を消す。ページは専用がなければコードのページへ縮退する |
| 3 | `src/hints.rs`（40 件の表、照合、`help`・`fix` の付与）、`tests/hints.rs`、`hints::TABLE` を G21 に公開 | M | Phase 2 の kind、**G22 Phase 1 の `help`・`fixes`** | 表がなければ何も付かない。ヒントは `help` が空の診断にだけ付く |
| 4 | LSP の hover に kind と `explain` の案内、`compiler/diagnostics.md` の kind 一覧（生成ブロック）と一致テスト、文書の仕上げ | S | Phase 1（kind）、Phase 2（一覧の完全性） | hover は診断の範囲でだけ変わる。文書は生成ブロックの更新だけ |

## 仕様

### S1 kind の規則

| 規則 | 内容 |
| --- | --- |
| 形式 | ASCII の `[a-z][a-z0-9_]*`、48 文字以内、登録簿の全体で一意、別の意味に再利用しない（D-51）。テストで強制する |
| コードとの関係 | 1 つの kind は 1 つのコードに属す。診断の `code` は kind の `code()` と同じ。`severity` はコードの先頭で決まる（`W` なら warning、`E` なら error） |
| variant 名 | id を `_` で区切り、各語の先頭を大文字にしてつなぐ（`use_after_move` → `UseAfterMove`）。テストで `id` との対応を検査する |
| `since` | その kind を初めて出したコンパイラの版（`Cargo.toml` の `version`）。HEAD の 653 kind はすべて `"0.1.0"`。新しい kind は新しい版の `since` グループに足す |
| 互換 | kind の追加は互換。削除・改名・別のコードへの移動は互換のない変更で行わない（`docs/stability.md` に書く）。1 つの kind を 2 つに分けるのは許す（元の kind は残りの条件に使い続ける） |

**粒度の決定表**（付録 C はこの表で一行ずつ決めてある）:

| 状況 | kind |
| --- | --- |
| (a) 同じ規則の違反で、名前や型だけが違う（`unknown value 'm'` と `unknown value 'x'`） | 同じ kind |
| (b) 規則が違う（`use of moved or partially moved value` と `cannot move a non-Copy element out of an array`） | 別の kind。コードが同じ `E1012` でも分ける |
| (c) 同じ規則を別のモジュールが検査する（`pattern alternatives exceed the compiler limit` が `control.rs` と `exhaustiveness.rs` にある） | 同じ kind（付録 C に同じ kind が書いてある） |
| (d) 1 つの発行箇所の中でメッセージが分岐する（`let message = if … else …`） | 規則が同じなら 1 kind（行は 1 つ）。規則が違うなら分岐ごとの kind（付録 C の `br` 行） |
| (e) 補助関数の呼び出しごとに条件が違う（`Parser::error(message)`） | 呼び出しごとに kind を決め、補助関数に `kind` 引数を足す（付録 E.1） |
| (f) 補助関数が 1 つの固定の条件を表す（`const_machine.rs` の `unsupported(span)`、`driver.rs` の `io_error`） | 補助関数の中の 1 kind。呼び出しは行にしない |
| (g) `Result<_, String>` を返す関数が 1 つの診断になる（`target::parse_target`、`bindgen::parse_arguments`） | 受け取る側で 1 kind。関数の型は変えない |
| (h) 資源上限（`E1017`）と内部エラー（`internal_*`） | 条件ごとに kind。専用ページは書かない |

**新しい kind を足す手順（D5）。** (1) 決定表で既存の kind に当てはまるか調べる（`tsuzuri explain --catalog | grep <語>`）。(2) なければ id を決め、付録 B.1 の形で、現在の版の `since` の群（なければ登録簿の末尾に新しい群を作る）へ、（コード, id）の昇順の位置に足す。(3) 発行箇所を `Diagnostic::of(DiagnosticKind::…, …)` にする。(4) 厳選 kind（付録 E.5）でなければページは任意。(5) `docs/stability.md`・付録の表は直さない（付録は HEAD の記録）。

### S2 登録簿（`src/diagnostic_kinds.rs`。新規）

次の `macro_rules!` を `rustc 1.98.1`（edition 2024）でコンパイルして動かして確かめた（付録 E.6）。**そのまま使う。**

```rust
macro_rules! diagnostic_kinds {
    ($(since $since:literal { $(($variant:ident, $id:literal, $code:literal)),+ $(,)? })+) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum DiagnosticKind { $($($variant,)+)+ }

        impl DiagnosticKind {
            pub const ALL: &'static [DiagnosticKind] = &[$($(DiagnosticKind::$variant,)+)+];
            pub const fn id(self) -> &'static str { match self { $($(DiagnosticKind::$variant => $id,)+)+ } }
            pub const fn code(self) -> &'static str { match self { $($(DiagnosticKind::$variant => $code,)+)+ } }
            pub const fn since(self) -> &'static str { match self { $($(DiagnosticKind::$variant => $since,)+)+ } }
            pub fn from_id(id: &str) -> Option<DiagnosticKind> {
                match id { $($($id => Some(DiagnosticKind::$variant),)+)+ _ => None }
            }
        }
    };
}
```

`diagnostic_kinds! { since "0.1.0" { (UseAfterMove, "use_after_move", "E1012"), … } }` の本体は、Phase 1 では付録 A の 8 kind と、`explain` のエラーの 4 kind（`E2000` の `option_conflict`・`unknown_explain_target`・`unknown_option`・`usage_error`）だけ、Phase 2 の手順 2.1 で付録 B.1 の全 653 行にする。重複した variant はコンパイルエラー、重複した id は `unreachable pattern`（clippy の `-D warnings` で失敗）になる。

手書きで足す関数（同じファイル）:

```rust
impl DiagnosticKind {
    pub fn severity(self) -> Severity            // code が 'W' で始まれば Warning、そうでなければ Error
    pub fn kinds_with_code(code: &str) -> Vec<DiagnosticKind>   // ALL から code が等しいものを id の辞書順で
    pub fn codes() -> Vec<&'static str>          // 全コードを辞書順に、重複なしで
}
```

`title()`・`page()` は `src/explain.rs` に置く（登録簿はページに依存しない）。

### S3 `Diagnostic` の追加（`src/diagnostic.rs`）

| 関数 | 振る舞い |
| --- | --- |
| `Diagnostic::of(kind: DiagnosticKind, message: impl Into<String>, span: Span) -> Diagnostic` | `new`・`warning` の構築（G22 が入っていれば `Box<DiagnosticData>` の組み立て）を、非公開の `fn build(severity: Severity, code: &'static str, kind: Option<&'static str>, message: String, span: Span) -> Diagnostic` に切り出す。`new` ＝ `build(Severity::Error, code, None, …)`、`warning` ＝ `build(Severity::Warning, code, None, …)`、`of` ＝ `build(kind.severity(), kind.code(), Some(kind.id()), …)`。ほかの拡張フィールド（G22）は空 |
| `Diagnostic::recode(mut self, kind: DiagnosticKind) -> Diagnostic` | `severity`・`code`・`kind` のフィールドを kind のものに書き換えて返す（G22 の `DerefMut` でも同じ書き方）。`message`・`span`・`related`・`help`・`fixes`・`detail` はそのまま。`error.code = "E1015"` の書き換えの置き換え先 |
| `Diagnostic::kind_of(&self) -> Option<DiagnosticKind>` | `self.kind.and_then(DiagnosticKind::from_id)` |

`Diagnostic::new`・`Diagnostic::warning` は手順 2.1 で `#[deprecated(note = "use Diagnostic::of")]` にし、手順 2.20 で削除する。重複除去のキーは変えない（`kind` を入れない）。

### S4 `tsuzuri explain`（`src/main.rs` と `src/explain.rs`）

```
tsuzuri explain TARGET [--json]
tsuzuri explain --catalog [--json]
```

`TARGET` は kind の id（`use_after_move`）かコード（`E1012`・`W1006`）。**コードは大文字だけ**（`e1012` は不明な対象）。出力は stdout、エラーは stderr（`--json` ならエラーも JSON の 1 行）。`--json`・`--catalog` の繰り返しはエラーにしない（`check --json --json` と同じ）。

| 入力 | 結果 | 終了コード |
| --- | --- | --- |
| `explain use_after_move` | kind の説明（テキスト） | 0 |
| `explain E1012` | コードの説明 | 0 |
| `explain use_after_move --json` | 1 行の JSON | 0 |
| `explain --catalog` / `explain --catalog --json` | 全 kind の一覧（コード → id の順） | 0 |
| `explain`（引数なし）・`explain a b`（対象が 2 つ） | `E2000` `usage_error` | 2 |
| `explain --catalog use_after_move` | `E2000` `option_conflict` | 2 |
| `explain --nope` | `E2000` `unknown_option` | 2 |
| `explain no_such_kind`・`explain e1012` | `E2000` `unknown_explain_target`（候補つき） | 2 |

メッセージ（英語。`<command line>:1:1`・`Span::default()`）:

| kind | メッセージ |
| --- | --- |
| `usage_error` | `usage: tsuzuri explain TARGET [--json] \| tsuzuri explain --catalog [--json]` |
| `option_conflict` | `--catalog cannot be combined with a target; list the kinds, or explain one` |
| `unknown_option` | `unknown option '--nope'; use --help` |
| `unknown_explain_target` | `unknown diagnostic 'no_such_kind'; run 'tsuzuri explain --catalog' to list every kind`。候補があれば `unknown diagnostic 'use_after_mov'; did you mean 'use_after_move'? run 'tsuzuri explain --catalog' to list every kind`（候補は最大 5 つを `'a', 'b'` と並べる） |

**テキスト（kind）。** 1 行目 `{kind}  {code}  {severity}  since {since}`（区切りは空白 2 つ）、2 行目 `{title}`、3 行目は空行、続けてページの本体（ページの `# ` の行と、その直後の空行を除いた残り）。専用ページがなければ、本体の位置に `(no dedicated page for this kind; the page of {code} follows)` と空行、コードのページの本体。コードのページもなければ `(no explanation yet; the message of the diagnostic says what to change)`。

```text
use_after_move  E1012  error  since 0.1.0
Use of a moved value

## Why
Passing a value to a function that takes ownership, …
```

**テキスト（コード）。** `{code}  {severity}`、`{コードの題}`、空行、`kinds:`、続けて kind ごとに `  {id}  {title}`（id の辞書順）、空行、コードのページの本体（なければ省く）。コードの題はコードのページの `# ` の行、なければ `Diagnostics with code {code}`。

**JSON（kind）。** 1 行。キーの順序: `kind`・`code`・`severity`・`since`・`title`・`page`（本体の文字列か `null`）・`code_page`（文字列か `null`）・`siblings`（同じコードの別の kind の id、辞書順）。文字列は `diagnostic::json_string` でエスケープする。キーは省略しない（`siblings` は空でも `[]`）。**JSON（コード）**: `code`・`severity`・`title`・`page`（文字列か `null`）・`kinds`（`[{"kind","title","page":true|false}]`、id の辞書順）。例（Phase 1 の想定出力。カタログの 1 行と、`explain E1012 --json`）:

```text
{"kind":"use_after_move","code":"E1012","severity":"error","since":"0.1.0","title":"Use of a moved value","page":true}
{"code":"E1012","severity":"error","title":"Diagnostics with code E1012","page":null,"kinds":[{"kind":"use_after_move","title":"Use of a moved value","page":true}]}
```

**カタログ。** テキストは 1 行に `{code}  {id}  {title}`（コードの昇順、同じコードの中は id の昇順。`ALL` の順に頼らず明示的に整列する）。`--json` は 1 行 1 オブジェクト `{"kind","code","severity","since","title","page":true|false}`（`page` は専用ページがあるか）。順序と内容は決定的（日時・絶対パスを入れない）。

**題（title）。** 専用ページの `# ` の行。なければ `humanize(id)`＝`_` を空白にして先頭 1 文字を大文字にした文字列（`option_requires_build` → `Option requires build`）。

**候補の探索**（付録 F.1 の `suggest`）。入力そのままと、全 kind の id・全コードとの Levenshtein 距離（挿入・削除・置換が各 1）を測る。上限は、対象が 5 文字以上なら 2、4 文字以下なら 1。対象が 3 文字以上なら、id がその前置きで始まるものも候補にする。(距離, 文字列) の昇順で最大 5 件。コードは、対象の大文字化がコードに一致するときだけ候補にする（`e1012` → `E1012`）。

### S5 説明ページ

| 種類 | 置き場所 | 登録 | 必須 |
| --- | --- | --- | --- |
| kind のページ | `src/explain/<kind>.md` | `src/explain.rs` の `KIND_PAGES: &[(&str, &str)]`（id の昇順。`include_str!`） | Phase 1 の 8 つ。Phase 2 で厳選 84 kind（付録 E.5）。それ以外は任意 |
| コードのページ | `src/explain/codes/<CODE>.md` | `CODE_PAGES: &[(&str, &str)]`（コードの昇順） | Phase 2 で全 47 コード |

**kind のページの形**（付録 A に 8 本の全文）: 1 行目 `# <題>`（英語、先頭を大文字、ピリオドなし、60 文字以内）。続いて `## Why`（1〜3 文。原因と、コードを `` `E1012` `` の形で 1 回）、`## Bad`（フェンスつきの例を 1 つ）、`## Good`（例を 1 つ。末尾に直し方の 1 文を足してよい）、`## See also`（任意。`` - `kind` `` の列。登録簿にある kind だけ）。全体で 1,200 文字以内。英語（D-56）。

**コードのページの形**: `# <題>`、`## What it covers`（1〜3 文）、`## Fixes`（`- ` の箇条書き 2〜6 件）、`## See also`（任意）。例は置かない（kind のページが持つ）。1,500 文字以内。

**例の検証**（`tests/explain.rs::every_page_example_behaves`）: フェンスの情報文字列が `tsuzuri` なら（1 ファイルで完結する例だけ。複数ファイルの例は `text`）、`Bad` を `analyze_modules_all(&[("Main", bad)])` に与え、**そのページの kind を持つ診断が含まれる**こと（警告の kind は、成功した解析の `warnings` に含まれること）、`Good` を同じ入口に与えて**エラーがない**ことを確かめる。情報文字列が `text` の例は検証しない（資源上限・内部エラー・環境に依る条件・CLI の例。Phase 2 の `sh` の検証は D12）。

### S6 ヒント DB（`src/hints.rs`。新規。Phase 3）

```rust
pub struct Hint {
    pub id: &'static str,                        // 'tuple-dot-index'。表の中で一意、小文字とハイフン
    pub title: &'static str,                     // ガイドの見出し（英語）
    pub kind: &'static str,                      // 付ける診断の kind の id（登録簿にあること）
    pub message_has: &'static [&'static str],    // 診断のメッセージに全部含まれる部分文字列。空なら条件なし
    pub patterns: &'static [&'static [&'static str]],  // 代替の列。空なら条件なし。どれか 1 つが合えばよい
    pub help: &'static str,                      // 命令形の 1 文（英語、小文字始まり、ピリオドなし）
    pub bad: &'static str,                       // この診断を出す最小のプログラム（検証する）
    pub good: &'static str,                      // 直したプログラム（エラーなしで通る。検証する）
    pub fix: Option<HintFix>,
}
pub struct HintFix { pub applicability: Applicability, pub replace: &'static str }   // 主トークンを replace に置き換える。Likely か Maybe だけ
pub static TABLE: &[Hint] = &[ … ];              // 付録 D の 40 件。順序が優先順位
pub(crate) fn annotate(set: &mut DiagnosticSet, inputs: &[SourceInput<'_>])
```

**1 つの `pattern` の意味。** 要素は 1 トークンの条件で、`@` を前置きした 1 つが**主トークン**（診断の主スパンの先頭のトークン）。要素の書き方: 文字列そのもの（`"."`・`"=>"`・`"return"`。トークンのソースの文字列と比べる）、または `<int>`（`TokenKind::Integer`）・`<ident>`（`TokenKind::Ident`）・`<upper>`（先頭が ASCII の大文字の `Ident`）・`<string>`（`TokenKind::String`）・`<any>`（任意の 1 トークン）。照合は、主トークンの前後に続く要素を `tokens[i + j - 主トークンの位置]` と比べる。範囲外なら不一致。

**主トークンの決め方。** `tokens = lexer::lex_all(text, edition).0`。診断の `span.start` に等しい開始位置を持つトークンがあればそれ。なければ、span の位置のソース文字列を 1 トークンとみなす**疑似トークン**（字句エラーでトークンにならない文字）。疑似トークンの照合は、`patterns` の要素が 1 つ（`["@?"]` や `["@<any>"]`）のときだけ許す。

**付ける順序と条件。** `analyze_inputs_with` が失敗の `DiagnosticSet` を返す直前に `annotate` を 1 回呼ぶ（擬似コードは付録 F.1）。診断ごとに: ① `help` が空で、`kind` が `Some`、入力が `ModuleOrigin::User` のときだけ進む。② `kind` が等しい `TABLE` の項目を表の順に調べ、`message_has` を全部含み、`patterns` が空かどれか 1 つ合う最初の項目を選ぶ。③ `help` を設定し、`fix` があれば、`fixes` が 8 件未満のとき `Fix { title: format!("replace with '{}'", replace), applicability, edits: vec![Edit { span: 主トークンの span, text: replace }] }` を足す。**既存の `help` は上書きしない。ヒントの `fix` は `certain` にしない**（経験則であり、`tsuzuri fix` が既定で適用しない）。`lex_all` はモジュールごとに最大 1 回（`BTreeMap`）、`annotate` は診断の収集上限（`MAX_UNIQUE_DIAGNOSTICS`）の内側の診断にだけ働き、表は 40 件で kind の `BTreeMap` で引く。

### S7 LSP と文書（Phase 4）

hover（`textDocument/hover`）: カーソルの位置が、そのドキュメントの診断（`ProjectState.diagnostics`）の `span` に入り、その診断が `kind` を持つとき、次の Markdown を応答に足す（最大 3 件）:

```text
**error[E1012]** `use_after_move`  
use of moved or partially moved value 'text'  
Run `tsuzuri explain use_after_move` for an explanation.
```

索引の応答（型・doc コメント）があるときは、ブロックを先頭に置き、`---` を挟んで続ける。**エラーがあると索引がなく、HEAD の hover は `Null` を返す**ので、そのときの応答はブロックだけ（`range` は先頭の診断の範囲）。診断がなければ応答は変わらない。`compiler/diagnostics.md` の `<!-- generated:diagnostic-kinds -->` と `<!-- /generated:diagnostic-kinds -->` の間は、`explain --catalog` から作る表（コードごとに 1 行。kind をバッククォートで囲み、`, ` でつなぐ。コードの昇順、kind は id の昇順）で、`tests/explain.rs::the_docs_list_every_kind` が一致を検査する。

### S8 本チケットが新しく出す診断

| kind | コード | 条件・位置 | メッセージ |
| --- | --- | --- | --- |
| `unknown_explain_target` | `E2000` | `explain` の対象が kind でもコードでもない。位置は `<command line>`（`Span::default()`）、`related`・`help` なし | 「S4 のメッセージ」の表 |

例（実装後の想定出力。終了コードはすべて 2）:

```text
$ tsuzuri explain use_after_mov
<command line>:1:1: error[E2000]: unknown diagnostic 'use_after_mov'; did you mean 'use_after_move'? run 'tsuzuri explain --catalog' to list every kind
$ tsuzuri explain no_such_kind --json
{"severity":"error","code":"E2000","message":"unknown diagnostic 'no_such_kind'; run 'tsuzuri explain --catalog' to list every kind","path":"<command line>","span":{"start":0,"end":0,"line":1,"column":1,"end_line":1,"end_column":1},"kind":"unknown_explain_target"}
$ tsuzuri explain --catalog use_after_move
<command line>:1:1: error[E2000]: --catalog cannot be combined with a target; list the kinds, or explain one
```

### S9 決定性・上限・互換

- 出力は決定的（`ALL` と表は固定順、`BTreeMap`／ソート済み、日時・絶対パスなし）。ページは `include_str!` で埋め込み、`explain` は CLI の経路でファイルを読まない。
- 上限: ページ 1,200／1,500 文字、候補 5 件、`TABLE` の項目の `help` は 160 文字以内（テストで強制）。
- 互換: `docs/stability.md` の「診断のコード・重要度・JSON のキー」に `kind`（足すだけのキー）、「kind の追加は互換、削除・改名・別コードへの移動は互換のない変更」、`explain` の JSON のキー（足すだけ）、ページと `help` の文言は保証の対象外、を足す。edition は kind を変えない（A22 の `E0004` も kind で表記を区別する）。
- 診断の JSON に `kind` が付くと、**文書の例の 4 か所**（`compiler/diagnostics.md` の 2 か所・`compiler/option.md`・`docs/language.md`）が変わる。手順 2.26 で直す。

## 設計

### 新規ファイルと関数

| ファイル | 内容 |
| --- | --- |
| `src/diagnostic_kinds.rs`（新規） | `diagnostic_kinds!` マクロ、`DiagnosticKind`、`severity`・`kinds_with_code`・`codes`（S2）。ほかのモジュールに依存しない（`crate::diagnostic::Severity` だけ使う） |
| `src/explain.rs`（新規） | `KIND_PAGES`・`CODE_PAGES`、`kind_page(kind)`・`code_page(code)`、`page_title(page) -> Option<&str>`・`page_body(page) -> &str`、`title(kind) -> String`・`code_title(code) -> String`、`humanize(id) -> String`、`enum Target { Kind(DiagnosticKind), Code(&'static str) }`、`resolve(target: &str) -> Result<Target, Vec<String>>`（`Err` が候補）、`suggest(target) -> Vec<String>`、`render_kind(kind, json) -> String`・`render_code(code, json) -> String`・`render_catalog(json) -> String`、テストが使う `sections(page) -> Vec<(&str, &str)>`・`examples(section) -> Vec<(&str, String)>`（フェンスの言語と本文） |
| `src/explain/<kind>.md`・`src/explain/codes/<CODE>.md`（新規） | S5 のページ |
| `src/hints.rs`（新規。Phase 3） | `Hint`・`HintFix`・`TABLE`・`annotate`・`matches`（非公開） |
| `src/main.rs` | `explain_command(arguments: &[OsString]) -> ExitCode`、`raw.first() == "explain"` の分岐（`api-diff` の分岐の次）、`HELP` に 2 行 |
| `src/lib.rs` | `pub mod diagnostic_kinds; pub mod explain; pub mod hints;`、`analyze_inputs_with` を薄い関数にして中身を `analyze_inputs_unannotated` に移し、失敗の集合に `hints::annotate` を 1 回かける（Phase 3）、`#![allow(deprecated)]`（Phase 2 の間だけ） |
| `src/diagnostic.rs` | `Diagnostic::of`・`recode`・`kind_of`（S3）。G22 がなければ `kind` フィールド・`with_kind`・JSON の `kind`（手順 1.1） |
| `src/lsp.rs` | hover の診断ブロック（Phase 4） |

### アルゴリズム

`resolve`・`suggest`（候補の探索）・`annotate`・`matches`（ヒントの照合）の擬似コードは [付録 F.1](G23-appendix-procedures.md#f1-アルゴリズム擬似コード) にある。

### Phase 2 の移行のしくみ

- **旧 API を消すまでの足場。** 手順 2.1 で `Diagnostic::new`・`warning` を `#[deprecated]` にし、`src/lib.rs`・`src/main.rs`・`tests/diagnostics.rs`・`tests/warnings.rs` の先頭に `#![allow(deprecated)]` を置く。移行したモジュールのファイルの先頭に `#![deny(deprecated)]` を置く（旧 API の使い残しはコンパイルエラーになる。`rustc` で、クレート側の `allow` よりモジュールの `deny` が勝つことを確かめた。付録 E.6）。**子モジュールを宣言するファイルの `deny` は、子がすべて終わってから置く**（`check.rs` は `closures`・`computation`・`constants`・`control`・`derive`・`exceptions`・`exhaustiveness`・`higher_kinds`・`polymorph`・`recursion`・`recursive`・`regions`・`warnings` を `#[path]` で持つ。`parser.rs` は `parse_control`、`closures.rs` は `closures_rec`、`llvm.rs` は `llvm_traps`・`llvm_ranges`・`llvm_cpu`、`driver.rs` は `test_runner`・`bindgen_driver`、`ownership.rs` は `ownership_control`、`gpu.rs` は `spirv`、`constants.rs` は `const_machine`・`const_rules`・`const_static` を持つ）。
- **補助関数の変更の方式**は付録 E.1: ファイルの中で完結するものは**その場で**シグネチャを変える（コンパイラが呼び出しをすべて指す）。複数の手順にまたがるもの（`Parser::error`・`rec_error`・`driver_error`）は、`kind` を取る兄弟（`error_of`・`rec_error_of`・`driver_error_of`）を足して旧関数を `#[deprecated]` にし、全部移したら旧関数を消す。
- **完了の保証。** 手順 2.20 で `Diagnostic::new`・`warning` を**消す**。残りがあればコンパイルエラーなので、漏れは残らない。手順 2.1 の `tests/diagnostic_kinds.rs::every_kind_has_an_emission_site` は、`src/` を読んで登録簿の全 variant が 1 回以上使われていることを検査する（途中は未使用の kind の一覧 `UNEMITTED` を置き、手順ごとに縮め、最後に空にする）。表の kind のとりこぼしと打ち間違いをここで見つける。

### Phase 2 の手順の割り当て

ファイルごとの手順と行の範囲（付録 C のファイル内の通し番号）は**付録 E.7**の表にある。順序: 2.1（登録簿・足場）→ 2.2（小さなモジュール 20 個）→ 2.3〜2.5（字句・構文）→ 2.6〜2.8（マニフェスト・定数・領域・制御・所有権・LLVM）→ 2.9〜2.12（多相・検査）→ 2.13〜2.14（GPU・fetch）→ 2.15〜2.17（driver）→ 2.18〜2.19（main）→ 2.20（旧 API の削除）→ 2.21〜2.25（ページ）→ 2.26（文書）。**子モジュールは親より先**に移す。

## 前提とする他チケットのインターフェース

| 名前 | 形（D-51） | G23 の使い方 | 未実装のとき |
| --- | --- | --- | --- |
| `Diagnostic.kind` | `Option<&'static str>`、`.with_kind(&'static str)` | `Diagnostic::of`・`recode` が直接フィールドを設定する | 手順 1.1 が `kind` フィールド・`with_kind`・JSON の `kind` キー（`span` の次）だけを足す。G22 の実装者は `kind` を重ねて足さない |
| `Diagnostic` の形（G22 の D1） | `Diagnostic(Box<DiagnosticData>)`・`Deref`/`DerefMut`・`into_data()` | フィールドは `Deref` で読み書きする。診断から `message` を move せず、`error.message.clone()` を使う | 4 フィールドの構造体のまま。同じ書き方で動く |
| JSON のキーの順序 | 既存 5 キー → `kind` → `related` → `help` → … → `fixes` | `kind` は `span` の次。Phase 3 の `help`・`fixes` は G22 の書き方に従う | 同上（`kind` だけ）。Phase 3 は始めない |
| `Diagnostic.help`・`.with_help` | `Option<String>` | ヒントを付ける | **Phase 3 を始めない**（停止条件 9） |
| `Fix`・`Edit`・`Applicability` | `Fix { title, applicability: Certain\|Likely\|Maybe, edits }`、`Edit { span, text }` | ヒントの `fix`（`Likely`・`Maybe` だけ） | 同上 |

本チケットが他へ提供するもの: G21 は `tsuzuri::hints::TABLE`（`&'static [Hint]`。フィールドは S6）を読む。G29 は `tsuzuri::explain::{resolve, render_kind, render_code, render_catalog}` と `DiagnosticKind` を呼ぶ。G35 は `explain --catalog --json` を読む。G24 は `Likely`・`Maybe` の `fixes` を受け取る。A22・A20・A21・G25・E15（新しい診断を足すチケット）は S1 の「新しい kind を足す手順」で kind を登録する。**新しいコード・サブコマンド・フラグ・環境変数の割り当ては要らない**（`explain` は D-55 の割り当て済み。診断のコードはすべて既存）。

## 実装手順

共通: 各手順の最後に `cargo build --locked && cargo test --locked`（通過件数が手順 1.1 の基準以上）、`cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings` が緑であること。テストは**先に書く**（失敗を確かめてから実装する）。

### Phase 1: 登録簿・`Diagnostic::of`・`tsuzuri explain`

#### 手順 1.1: 基準の固定と `kind` の足場

- 変更: `tests/fixtures/diagnostic_baseline/`（新規）、`tests/diagnostic_kinds.rs`（新規）、（G22 がなければ）`src/diagnostic.rs`・`tests/diagnostics.rs`。
- 内容: ① 基準を取る（`cargo build --release --locked && cargo test --locked` の件数を控える。変更前の `target/release/tsuzuri` を `cp target/release/tsuzuri target/tsuzuri-before` で残す。`target/` は git が無視する）。② `tests/fixtures/diagnostic_baseline/<名前>/Main.tz` を 48 個作る（付録 A の 8 つの `Bad` は名前を kind に、付録 D の 40 件の `bad` は名前を id にして、そのまま書く）。③ 変更前のバイナリ（`target/tsuzuri-before`。cwd を変えるので絶対パスで指す）で、各ディレクトリを `cwd` にして `check . --json` を実行し、`{"case":"<名前>","diagnostics":[<stderr の各行>]}`（ほとんどは 1 行で、`print-macro`・`struct-keyword` は 2 行）を名前の順に並べて `tests/fixtures/diagnostic_baseline/baseline.jsonl` に保存する（`path` は `./Main.tz`）。④ `tests/diagnostic_kinds.rs` に `baseline_diagnostics_are_unchanged` を書く: 現在のバイナリで同じ手順を行い、各ケースの診断の行数が基準と同じで、現在の各行が**基準の行から末尾の `}` を除いたもので始まり**、残りが空か `,"` で始まる足されたキー（`kind`・`related`・`help`・`expected`・`found`・`candidates`・`scope`・`fixes`。G22 の順序で `span` の後ろ）だけであることを検査する。⑤ G22 がなければ `kind` の足場を足す: `pub kind: Option<&'static str>`（`new`・`warning` は `None`）、`pub fn with_kind(mut self, kind: &'static str) -> Self`、`json_with_severity` の `span` オブジェクトの直後に、`kind` が `Some` のときだけ `,"kind":<json_string(kind)>` を足す。`tests/diagnostics.rs` に `kind_key_is_added_only_when_present` を足す（`kind` のない診断の JSON が従来と 1 byte も同じ、`with_kind("x")` を付けると末尾に `,"kind":"x"` が付く）。
- 確認: `cargo test --locked --test diagnostic_kinds` が 1 件通る。`git diff --stat` が上のファイルだけ。全体の件数が基準＋追加分。

#### 手順 1.2: 登録簿（テスト先行）

- 変更: `tests/diagnostic_kinds.rs`、`src/diagnostic_kinds.rs`（新規）、`src/lib.rs`（`pub mod diagnostic_kinds;`）。
- 内容: 先に `tests/diagnostic_kinds.rs` へ次を書く（コンパイルが通らないので最初は `use` だけで失敗する）。`registry_is_consistent`: `DiagnosticKind::ALL` の全 kind について、id が `^[a-z][a-z0-9_]*$` で 48 文字以内、`from_id(id)` が自分に戻る、id が重複しない、コードが `^[EW][0-9]{4}$`、`severity()` がコードの先頭と合う、`since` が `d.d.d`、`format!("{kind:?}")` が id を UpperCamel にしたものと等しい、`ALL` の `since` が同じ連続した区間ごとに（コード, id）の昇順。`kinds_with_code_is_sorted`。次に `src/diagnostic_kinds.rs` を S2 のとおり書き、本体は次の 12 行だけ置く（付録 B.1 の順）: `E1002` の `unknown_module_member`・`unknown_value`、`E1003` の `type_mismatch`、`E1012` の `use_after_move`、`E1013` の `borrow_outlives_block`、`E1014` の `borrow_conflict`・`mutate_immutable_value`、`E1021` の `non_exhaustive_match`、`E2000` の `option_conflict`・`unknown_explain_target`・`unknown_option`・`usage_error`。
- 確認: `cargo test --locked --test diagnostic_kinds` が通る。

#### 手順 1.3: `Diagnostic::of`・`recode`・9 箇所の移行

- 変更: `tests/diagnostic_kinds.rs`、`src/diagnostic.rs`、`src/ownership.rs`・`src/exhaustiveness.rs`・`src/check.rs`・`src/polymorph.rs`。
- 内容: 先に `of_sets_code_severity_and_kind`（`Diagnostic::of(DiagnosticKind::UseAfterMove, "m", Span::new(1, 2))` の `code == "E1012"`・`severity == Error`・`kind == Some("use_after_move")`）、`recode_replaces_code_and_kind_only`（`recode` で `code`・`kind`・`severity` が変わり、`message`・`span` は同じ）、`json_adds_only_the_kind_key`（手順 1.1 の基準の `use_after_move` の診断の行の末尾の `}` の前に `,"kind":"use_after_move"` を足したものと、現在の出力が等しい）を書く。次に S3 の 3 関数を足し、次の 9 箇所を `Diagnostic::of(DiagnosticKind::…, <同じメッセージ>, <同じスパン>)` に直す（コード・メッセージ・スパンは変えない）:

| ファイル | 関数 | 現在 | kind |
| --- | --- | --- | --- |
| `src/ownership.rs` | `Checker::access` | `error("E1012", …"use of moved or partially moved value '{name}'")` | `UseAfterMove` |
| `src/ownership.rs` | `Checker::access` | `error("E1014", …"mutable access requires 'let mut' …")` | `MutateImmutableValue` |
| `src/ownership.rs` | `Checker::access` | `error("E1014", …"access conflicts with a live borrow; …")` | `BorrowConflict` |
| `src/ownership.rs` | `Checker::eval_block` | `error("E1013", …"borrowed value does not live long enough to leave this block")` | `BorrowOutlivesBlock` |
| `src/exhaustiveness.rs` | `Checker::check_coverage` | `Diagnostic::new("E1021", …"match is not exhaustive; missing: {}")` | `NonExhaustiveMatch` |
| `src/check.rs` | `Checker::name` | `Diagnostic::new("E1002", …"unknown value '{}'")` | `UnknownValue` |
| `src/check.rs` | `Checker::value_expression` | `Diagnostic::new("E1002", message, field.span)`（モジュールのメンバーがない） | `UnknownModuleMember` |
| `src/polymorph.rs` | `Inference::unify` | `Diagnostic::new("E1003", …"expected {}, found {}")` | `TypeMismatch` |
| `src/polymorph.rs` | `Checker::finish` | `Diagnostic::new("E1003", …"expected an integer, found {}")` | `TypeMismatch` |

- 確認: `cargo test --locked` が緑（件数は基準＋追加分）。`baseline_diagnostics_are_unchanged` が緑（`kind` 以外が基準と同じ）。`target/release/tsuzuri check` の `use_after_move` の `Bad` の JSON の末尾が `,"kind":"use_after_move"}` で、`tuple-dot-index`（例）のような**移行していない診断の JSON は変わらない**。

#### 手順 1.4: `src/explain.rs` とページ 8 本（ライブラリ）

- 変更: `tests/explain.rs`（新規）、`src/explain.rs`（新規）、`src/explain/<kind>.md`（8 本）、`src/lib.rs`（`pub mod explain;`）。
- 内容: 先に `tests/explain.rs` に書く（入力と期待値は「テスト計画」の表）: `every_page_example_behaves`・`pages_have_the_required_shape`（S5 の形。1 行目が `# `、`## Why`・`## Bad`・`## Good` がこの順、1,200 文字以内）・`resolve_accepts_kinds_and_codes`・`resolve_suggests_close_names`・`humanize_titles`（`option_requires_build` → `Option requires build`）・`render_kind_text_and_json`（付録 A.1 の出力の先頭 3 行とキーの順序）・`render_catalog_is_sorted`。次に S4・S5 のとおり実装し、付録 A のページ 8 本を `src/explain/<kind>.md` に**そのまま**置く。`KIND_PAGES` は id の昇順に `("borrow_conflict", include_str!("explain/borrow_conflict.md"))` の形。
- 確認: `cargo test --locked --test explain` が通る。

#### 手順 1.5: CLI の `explain`

- 変更: `tests/explain.rs`、`src/main.rs`。
- 内容: 先に `tests/explain.rs` に CLI のテストを書く（`env!("CARGO_BIN_EXE_tsuzuri")`、手本 `tests/api_diff.rs`）: `cli_explains_a_kind`（stdout が `use_after_move  E1012  error  since 0.1.0` で始まり、終了コード 0）、`cli_explains_a_code`、`cli_json_has_the_documented_keys`（キーの順序）、`cli_catalog_lists_every_kind`（行数 = `DiagnosticKind::ALL.len()`）、`cli_rejects_bad_usage`（S4 の表の各行: 終了コード 2、stderr の JSON の `code` と `kind`）。次に `src/main.rs` に `explain_command` を足す: 引数を `--json`・`--catalog`・その他に分け、S4 の表のとおり判定して `tsuzuri::explain` を呼ぶ。エラーは `print_diagnostic(&Diagnostic::of(kind, message, Span::default()), Path::new("<command line>"), "", json)` で出して `ExitCode::from(2)`。`main` に `raw.first() == "explain"` の分岐を `api-diff` の分岐の次に足す。`HELP` に `tsuzuri explain TARGET [--json]` と `tsuzuri explain --catalog [--json]` の 2 行を足す。`rejects_ambiguous_or_unused_arguments` は変えない（`explain` は通常の引数解析の前に分岐する）。
- 確認: `cargo test --locked --test explain` が緑。`cargo build --release --locked && target/release/tsuzuri explain use_after_move` が付録 A.1 のページを出し、`target/release/tsuzuri explain no_such_kind; echo $?` が `E2000` と `2`。

#### 手順 1.6: Phase 1 の文書

- 変更: 「ドキュメント」の一覧のうち `compiler/usage.md`（`### explain`）、`compiler/diagnostics.md`（JSON の表の `kind` と節「kind と explain」）、`README.md`、`docs/language.md`、`docs/stability.md`、`docs/architecture.md`。
- 確認: `node scripts/check-docs.mjs _tsuzuri/language-reference/compiler/usage.md _tsuzuri/language-reference/compiler/diagnostics.md` が通る。

#### 手順 1.7: Phase 1 の全体確認

- 内容: `cargo test --locked`、`cargo clippy --all-targets -- -D warnings`、`node tests/features.mjs target/release/tsuzuri`（IR・結果が変わらないこと。診断の経路だけの変更）。`tests/fixtures/diagnostic_baseline` の基準が緑。付録 A の 8 つの `Bad` を `check --json` に与え、各 JSON の末尾の `kind` が付録 A の表と一致すること。
- 確認: 受け入れ条件の Phase 1 の欄がすべて満たされる。

### Phase 2: 全発行箇所の移行と全説明ページ

#### 手順 2.1: 登録簿を全 kind に広げる・旧 API を `deprecated` にする・足場

- 変更: `tests/diagnostic_kinds.rs`、`src/diagnostic_kinds.rs`、`src/diagnostic.rs`、`src/lib.rs`・`src/main.rs`・`tests/diagnostics.rs`・`tests/warnings.rs`（先頭に `#![allow(deprecated)]`）。
- 内容: 先に `every_kind_has_an_emission_site` を書く: `src/**/*.rs`（`src/diagnostic_kinds.rs`・`src/explain.rs`・`src/hints.rs` を除く）を `std::fs` で読み、`DiagnosticKind::<Variant>` として現れる variant の集合を作り、登録簿の全 variant のうち**現れないもの**が、テスト内の定数配列 `UNEMITTED: &[&str]`（id）と**等しい**ことを検査する（配列にあるのに使われている kind も、配列にないのに使われていない kind も失敗する）。`UNEMITTED` は、付録 B.1 の全 653 kind から、手順 1.3・1.5 で使った 12 kind を除いた 641 個を id の昇順で書く。次に登録簿の本体を付録 B.1 のブロックに置き換える（`registry_is_consistent` が緑のまま）。`Diagnostic::new`・`warning` に `#[deprecated(note = "use Diagnostic::of")]` を付け、4 ファイルの先頭に `#![allow(deprecated)]` を置く。
- 確認: `cargo test --locked` 緑。`cargo clippy --all-targets -- -D warnings` に `deprecated` の警告が出ない。`target/release/tsuzuri explain --catalog | wc -l` が 653（`--release` を作り直してから）。

#### 手順 2.2〜2.19: モジュールの移行（共通の流れ）

**変更**は付録 E.7 の表のファイルと、その `use`（`use crate::diagnostic_kinds::DiagnosticKind;`。`use super::*` で入るモジュールは不要なことがある）。**内容**は次の流れ。

1. 付録 C のファイルの表を開く。行はソースの順に並べてある。`grep -n 'Diagnostic::new\|Diagnostic::warning\|<補助関数>' <ファイル>` の結果を先頭から表の行に順に当て、メッセージの先頭で照合する（照合できない行が手順ごとに 10 行を超えたら停止条件 2）。
2. 行の `形` に従って直す。形ごとの直し方は付録 C の各ファイルの冒頭の表にある（コード・メッセージ・スパンは変えない）。代表: `Diagnostic::new("E1012", m, s)` → `Diagnostic::of(DiagnosticKind::UseAfterMove, m, s)`、`self.error(m)` → `self.error_of(DiagnosticKind::ExpectedToken, m)`、`driver_error("E2000", m)` → `driver_error_of(DiagnosticKind::OptionConflict, m)`。
3. 移し終えたファイルの先頭に `#![deny(deprecated)]` を置く（親モジュールは子が終わってから。上の「Phase 2 の移行のしくみ」）。`deny` は `#[cfg(test)]` の中も指す（`cargo test` でだけ失敗する）ので、`recursive.rs`（手順 2.2）・`check.rs`（手順 2.12）の inline テストの `Diagnostic::new` はこのとき手順 2.20 の対応表の kind に直す。`cargo fmt --all`。
4. `UNEMITTED` から、このファイルで初めて使う kind を消す。

**確認**（各手順）: (1) `grep -n 'Diagnostic::new\|Diagnostic::warning' <移したファイル>` が 0 件（`src/diagnostic.rs` の inline テストは手順 2.20）。(2) `cargo build --locked` が通る（`deny` が旧 API の使い残しを止める）。(3) `cargo test --locked` が緑で、通過件数が減らない。(4) `baseline_diagnostics_are_unchanged` と `every_kind_has_an_emission_site` が緑。(5) `cargo clippy --all-targets -- -D warnings`。**コードが元と同じこと**は、そのファイルの診断を出す既存のテストと (4) で保たれる。疑わしい行は `git diff -U0 -- <ファイル>` で、消えた `"E…"` と足された `DiagnosticKind::…` を並べ、付録 C の `コード` 列と照らす。

手順 2.3 は `Parser::error_of`・`rec_error_of`（シグネチャは付録 E.1 の 2・3）を `src/parser.rs` に足す。本体は旧関数のコピーで、`Diagnostic::new("E0002", …)` を `Diagnostic::of(kind, …)` に変えるだけ（`rec_error_of` の `None` の枝は `self.error_of(kind, message)`）。旧 `Parser::error`・`rec_error` に `#[deprecated(note = "use error_of")]` を付け、手順 2.5 で消す。手順 2.15 は `driver_error_of`（E.1 の 12。本体は `Diagnostic::of(kind, message, Span::default())`）を足し、旧 `driver_error` に `#[deprecated]` を付け、手順 2.17 で消す。

#### 手順 2.20: 旧 API の削除

- 変更: `tests/diagnostics.rs`・`tests/warnings.rs`・`src/diagnostic.rs`（inline テストと `new`・`warning` の削除）、`src/lib.rs`・`src/main.rs`（`allow` の削除）、`tests/diagnostic_kinds.rs`、全モジュールの `deny`。
- 内容: テストが任意のコードの診断を作っている箇所のうち残りの 9 箇所（`src/diagnostic.rs` の 2、`tests/diagnostics.rs` の 4、`tests/warnings.rs` の 3。付録 E.2 の 21）を、**同じコードを持つ実在の kind** に直す: `E1003` → `DiagnosticKind::TypeMismatch`、`E1002` → `UnknownValue`、`W1001` → `UnusedLocal`、`W1003` → `UnreachableMatchArm`、`E0001` → `UnexpectedCharacter`。（`E1017`＝`check.rs` の `verify_children` のテストは `PolymorphicTypeExpansionLimit`、`E1010`＝`recursive.rs` のテストは `InfiniteRecursiveLayout` で、手順 2.2・2.12 で直してある。）`Diagnostic::new`・`warning` を**削除**する。すべての `#![allow(deprecated)]`・`#![deny(deprecated)]` を消す。`UNEMITTED` の配列と、それを使う検査の例外を消す（空であることを `every_kind_has_an_emission_site` が要求する形に直す）。
- 確認: `grep -rn 'Diagnostic::new\|Diagnostic::warning' src tests` が 0 件。`grep -rn 'allow(deprecated)\|deny(deprecated)' src tests` が 0 件。`cargo test --locked`・`cargo clippy --all-targets -- -D warnings` が緑。`tsuzuri explain --catalog | wc -l` が 653。

#### 手順 2.21: 全コードのページ

- 変更: `tests/explain.rs`、`src/explain.rs`（`CODE_PAGES`）、`src/explain/codes/<CODE>.md`（47 本）。
- 内容: 先に `every_code_has_a_page`（`DiagnosticKind::codes()` の全部が `CODE_PAGES` にあり、昇順で、形が S5 のとおり、1,500 文字以内、`See also` の kind が登録済み）と、手順 1.5 の `cli_explains_a_code` の期待をページの本体に更新するテストを書く。次に 47 本を書く: 内容の元は `_tsuzuri/language-reference/compiler/diagnostics.md` の「エラーコード」の表（日本語）と、付録 B.3 の kind の一覧。英語で、`## What it covers`（そのコードが表す原因の分類）と `## Fixes`（直し方 2〜6 件）。
- 確認: `cargo test --locked --test explain` が緑。`tsuzuri explain E1012` がページを出す。

#### 手順 2.22〜2.25: 厳選 kind のページ（20 本ずつ）

- 変更: `tests/explain.rs`、`src/explain.rs`（`KIND_PAGES`）、`src/explain/<kind>.md`。
- 内容: 厳選 kind は付録 E.5（84 個。Phase 1 の 8 を含む）。先に `curated_kinds_have_pages` を書く: 付録 E.5 の 84 個の id をテスト内の定数 `CURATED` に並べ、`CURATED` の各 kind が `KIND_PAGES` にあることを検査し、まだ書いていない kind は `PENDING: &[&str]` に並べて「ここにある kind はページがない」ことも検査する（手順ごとに `PENDING` を縮め、2.25 で空にする）。ページは次の手順で書く: ① `grep -rn '"<コード>"' tests/*.rs | head` で、その kind を出す既存のテストのソースを探す（付録 C の行のメッセージで当てる）。② 付録 D の `bad`／`good` に同じ kind があればそれを使う。③ `tsuzuri check --json` で `Bad` が当該 kind の診断を出すことを確かめる（出なければ別の例を探す。見つからない kind は、`text` の例にして理由を `Why` に書く）。④ `Good` を書き、`every_page_example_behaves` を通す。
- 確認: 各手順で `cargo test --locked --test explain` が緑、`PENDING` が 20 縮む。

#### 手順 2.26: Phase 2 の文書

- 変更: 「ドキュメント」の一覧のとおり。
- 確認: `node scripts/check-docs.mjs _tsuzuri/language-reference/compiler/diagnostics.md _tsuzuri/language-reference/compiler/option.md _tsuzuri/language-reference/compiler/usage.md` が通る。

### Phase 3: ヒント DB

手順 3.1〜3.5 は [付録 F.2](G23-appendix-procedures.md#f2-phase-3-ヒント-db-の手順) にある（3.1 照合の機構、3.2 表の 40 件、3.3 `analyze_inputs_with` への接続、3.4 ヒントの `fix`、3.5 G21 への公開と文書）。Phase 3 は G22 Phase 1 が済んでいなければ始めない。

### Phase 4: LSP と文書の仕上げ

手順 4.1〜4.3 は [付録 F.3](G23-appendix-procedures.md#f3-phase-4-lsp-と文書の仕上げの手順) にある（4.1 hover、4.2 文書の kind 一覧と一致テスト、4.3 全体の確認）。

## テスト計画

| テスト（ファイル::関数） | 入力 → 期待値 |
| --- | --- |
| `tests/diagnostic_kinds.rs::registry_is_consistent` | `ALL` の全 kind: id の形・長さ・一意、`from_id` の往復、コードの形、`severity` とコードの先頭、`since`、variant 名と id の対応、`ALL` の `since` が同じ連続した区間ごとに（コード, id）の昇順 |
| `…::of_sets_code_severity_and_kind`・`recode_replaces_code_and_kind_only` | S3 の表 |
| `…::json_adds_only_the_kind_key` | `use_after_move` の `Bad` の JSON ＝ 変更前の行 ＋ `,"kind":"use_after_move"` |
| `…::baseline_diagnostics_are_unchanged` | 48 ケースの `check . --json`（cwd＝ケース）の各行が、`baseline.jsonl`（**変更前のバイナリ**で作る）の行から末尾の `}` を除いたもので始まり、残りが空か足されたキー（`kind`・`related`・`help`・`expected`・`found`・`candidates`・`scope`・`fixes`）だけ。行数も一致 |
| `…::every_kind_has_an_emission_site` | 登録簿の全 variant が `src/` で使われる（`UNEMITTED` は 2.20 で空）。ほかの `src/` ファイルに存在しない variant 名を書くとコンパイルエラー |
| `tests/diagnostics.rs::kind_key_is_added_only_when_present`（G22 がないとき） | `kind` なしの JSON は従来と同じ、`with_kind("x")` で末尾に `,"kind":"x"` |
| `tests/explain.rs::every_page_example_behaves` | 全 kind のページ: `Bad` が当該 kind を出し、`Good` がエラーなし |
| `…::pages_have_the_required_shape`・`every_code_has_a_page`・`curated_kinds_have_pages` | S5 の形、`See also` の kind が登録済み、`KIND_PAGES`・`CODE_PAGES` の id が登録簿にあり昇順・重複なし、全 47 コード、厳選 84 kind（`PENDING` は 2.25 で空） |
| `…::resolve_accepts_kinds_and_codes`・`resolve_suggests_close_names`・`humanize_titles` | `use_after_move`・`E1012`・`W1006` が解決し、`e1012` は不明で候補 `E1012`、`use_after_mov` の候補が `use_after_move`、`zzzz` は候補なし |
| `…::render_kind_text_and_json`・`render_catalog_is_sorted` | S4 の形・キーの順序・行数＝`ALL.len()` |
| `…::cli_explains_a_kind`・`cli_explains_a_code`・`cli_json_has_the_documented_keys`・`cli_catalog_lists_every_kind`・`cli_rejects_bad_usage` | S4 の表の各行（終了コード 0／2、stderr の `code`・`kind`） |
| `…::the_docs_list_every_kind` | `compiler/diagnostics.md` の生成ブロックが `ALL` から作る表と一致 |
| `src/hints.rs` の `mod tests`、`tests/hints.rs` | 付録 F.2 の手順 3.1〜3.5 に、テスト名と入力・期待値がある |
| `tests/lsp_sessions.mjs`（hover のケース。既存の `stale` のケースは位置を診断の外の `bad` に変える） | 付録 F.3 |

**期待値の出どころ。** 診断の `code`・`message`・`span` は変更前のバイナリ（`baseline.jsonl`）と既存のテストが保証する。ページの例は、ページを書いた人の意図（`Bad` が出すべき kind）と実際のコンパイラの出力を照合する（別の kind が出たらページを直す）。**既存テストへの影響**: 期待値が変わるものはない（`kind` を足すだけ。`tests/diagnostics.rs::cli_json_lists_every_recovered_error_in_order` は `starts_with` と JSON のキーで見る）。Phase 4 は `tests/lsp_sessions.mjs` の `stale` のケースの位置を 1 か所変える（手順 4.1）。書き換えるのは、`tests/diagnostics.rs`・`tests/warnings.rs` の 7 行と inline の 4 行（手順 2.2・2.12・2.20）、`src/main.rs` の inline テストの補助 `parse`・`parse_repl`（手順 2.18。`map_err(|error| error.message.clone())` を足す）。決定性: `explain --catalog` を 2 回実行して一致。性能: 測らない（失敗した経路の終わりで 1 回。成功するコンパイルは変わらない）。

## ドキュメント

更新するページ（`node scripts/check-docs.mjs <ページ>` で検証。例は `tsuzuri check` が通る形で書く）:

| ページ | 内容 |
| --- | --- |
| `_tsuzuri/language-reference/compiler/diagnostics.md` | JSON の表に `kind` の行。**例 2 か所**（`E1003` の行に `"kind":"type_mismatch"`、`file is not formatted` の行に `"kind":"file_not_formatted"`）。節「kind と explain」「ヒント」「kind の一覧」（生成ブロック）。ポイントに 1 行 |
| `_tsuzuri/language-reference/compiler/usage.md` | `### explain`（使い方・出力・終了コード・例。`### api-diff` の次）、ポイントに 1 行、終了コードの節に `explain` |
| `_tsuzuri/language-reference/compiler/option.md` | `E2000` の例（`missing .tz, .tt, or .tc input…`）に `"kind":"missing_input"` |
| `docs/language.md` | 診断の節の JSON の例（`E1003`）に `kind`、`kind`・`explain`・ヒントの説明 |
| `docs/stability.md` | 診断のキーに `kind`（足すだけ）、kind の追加は互換・削除・改名・別コードへの移動は互換のない変更、`explain` の JSON のキー、ページと `help` の文言は保証の対象外 |
| `docs/architecture.md` | ファイルの表に `diagnostic_kinds.rs`・`explain.rs`・`explain/`・`hints.rs` |
| `README.md` | CLI の一覧に `tsuzuri explain` の 2 行、機能の説明に 1 文 |
| `_features/README.md`・`GUIDE.md` の台帳 | 統括者が更新（完了時: G23 を done に、D-51 の kind の節に「G23 で全診断が kind を持つ」を追記） |

## 受け入れ条件

**Phase 1**
- [ ] `tests/diagnostic_kinds.rs`・`tests/explain.rs` の Phase 1 のテストが緑。付録 A の 8 ページが置かれ、各 `Bad`／`Good` の検証が緑。
- [ ] `tsuzuri explain use_after_move`・`explain E1012`・`explain --catalog`・`--json` が S4 の形で動き、不明な対象は `E2000` と終了コード 2。
- [ ] `check` の診断は 9 箇所だけに `kind` が付き、`baseline_diagnostics_are_unchanged` が緑。全体のテスト・clippy・fmt が緑。
- [ ] 文書（手順 1.6）が更新され、`check-docs.mjs` が通る。

**Phase 2**
- [ ] `Diagnostic::new`・`warning` がなく、`allow(deprecated)`・`deny(deprecated)` が残っていない。`UNEMITTED` が空。
- [ ] `tsuzuri explain --catalog` が 653 行。全 47 コードと厳選 84 kind のページがあり、検証が緑。
- [ ] 既存のテストがすべて変更なしで緑（書き換えは「既存テストへの影響」の分だけ）。`baseline_diagnostics_are_unchanged` が緑。
- [ ] 文書（手順 2.26）が更新され、`compiler/diagnostics.md` の JSON の例が `kind` を含む。

**Phase 3**
- [ ] `TABLE` が 40 件で、全項目の `bad`／`good` の検証が緑。`help` が既存の `help` を上書きしない。ヒントの `fix` が `certain` でない。
- [ ] `analyze_inputs_with` の成功の経路が変わらない（`help` は失敗した診断だけ）。`tests/hints.rs` が緑。

**Phase 4**
- [ ] hover が診断の範囲で kind と案内を返し、ほかの位置の応答が変わらない。`the_docs_list_every_kind` が緑。
- [ ] `README.md`・`docs/language.md`・言語リファレンスが食い違わない。

## 落とし穴

| 間違いやすい点 | 検出 |
| --- | --- |
| `cargo test <名前>` は 0 件でも成功する | 出力の `running N tests` の N を見る |
| kind を選び間違えて、コードが変わる（例: `E1012` の行に `E1014` の kind を付ける） | `baseline_diagnostics_are_unchanged`、既存の `rejects(source, "E…")` のテスト。疑わしい行は `git diff -U0` で消えた `"E…"` と足した `DiagnosticKind::…` を並べる |
| `error.code = "…"` を残す（kind とコードがずれる） | `grep -rn '\.code = ' src` が 0 件（`recode` を使う） |
| 親モジュールに先に `deny(deprecated)` を置き、未移行の子が止まる | 子が終わってから置く（「Phase 2 の移行のしくみ」） |
| `Diagnostic::of` の引数の順（kind, message, span）を、旧 `new`（code, message, span）と同じ順だと思い込んで、`error(kind, …)` の補助の順を変える | 補助は第 1 引数だけ型を変え、順序を変えない |
| ヒントの `patterns` が実トークンと合わない（`+=` は `+` と `=` の 2 トークン、キーワードは `<ident>` に合わない） | `every_hint_matches_its_bad_source`。`lex_all` の結果を `dbg!` で出して `patterns` だけを直す |
| 手作りの JSON の行（`W2001`・`file is not formatted`）に `kind` を足し忘れる | `grep -n 'W2001\|file is not formatted' src/main.rs` の 5 行 |
| `include_str!` のパスを `src/explain.rs` からの相対で書かない | `explain/<kind>.md`（`src/explain.rs` から見た相対）。ビルドが失敗する |
| 付録 C の「関数」の列が、closure の中の行で外側の関数を指す | メッセージの先頭で行を当てる（関数の列は目安） |

## 対象外

メッセージの文言・コード・位置の変更（直してよいが本チケットでは触らない）。関連位置・`related`（G22）。`tsuzuri fix` と編集の適用（G24）。型つき穴と候補（G25）。説明ページの多言語化（英語のみ。D-56）。全 kind の専用ページ（厳選 84 と全コードだけ。残りは任意に足す）。`sh`（CLI の例）の検証（D12）。実行時の診断（トラップの文言。B10）。`explain` のオンライン版・Web 公開。LLM による説明の生成。

## 決定事項

| ID | 決定 | 理由・却下した案 | 状態 |
| --- | --- | --- | --- |
| D1 | kind は enum `DiagnosticKind` で表し、`Diagnostic::of` でしか診断を作れなくする（旧 API は消す） | 誤字・重複・欠落をコンパイラが止める。却下: 各箇所に `with_kind("…")` を書く（打ち間違いと付け忘れが残る） | 承認済み |
| D2 | 粒度は「規則」（付録 C の 653）。決定表 S1 | 却下: メッセージごと（言い換えで kind が増える）、コードごと（47＝今と同じで原因を区別できない） | 承認済み |
| D3 | ページは「全 47 コード」＋「厳選 84 kind」。残りの kind はコードのページへ縮退する。**種からの逸脱** | 653 kind のうち 517 は 1 行だけで、全部に例つきの専用ページを要求すると Phase 2 が数倍になる。却下: 生成するだけのページ | 承認済み |
| D4 | 題は専用ページの `# ` の行だけ（登録簿に題を持たない）。種の「先頭 3 行 `code:`・`title:`・`since:`」はやめ、コードと `since` は登録簿から出す。**種からの逸脱** | 二重管理で食い違う。却下: 登録簿に題を持つ | 承認済み |
| D5 | 新しい診断は kind を登録する（A22・A20・A21・G25・E15 も）。`Diagnostic::new` を消すので強制される | 却下: kind なしの診断を許す | 承認済み |
| D6 | 移行は `#[deprecated]`＋モジュールごとの `deny(deprecated)`、最後に旧 API を削除 | 途中でも緑で、残りを数えられる。却下: 一括置換（レビューできない）、旧 API を残す | 承認済み |
| D7 | `kind` の型は D-51 のまま `Option<&'static str>`（常に `Some`） | D-51 の型を変えない。`kind_of` で enum に戻す | 承認済み |
| D8 | テキスト出力は変えない（`explain` の案内の行を足さない） | 出力に依存するテストと AI のトークンを増やさない（AI は `--json` を使う）。却下: rustc 風の案内の行 | 承認済み |
| D9 | ヒントは後処理の `annotate`（表）で、パーサ・検査器に入れない | 表が 1 か所にあり G21 が読める。却下: 発行箇所ごとにヒントを書く（表が散らばる） | 承認済み |
| D10 | 照合は「kind＋`message_has`＋主トークンの前後のトークン」 | crate を足さず、エラーのとき AST がなくても動く。却下: 正規表現（crate が要る）、AST の照合 | 承認済み |
| D11 | ヒントの `fix` は `Likely`・`Maybe` だけ | 経験則であり、`tsuzuri fix` が既定で適用してはならない（`certain` は意味を保つ書き換えだけ） | 承認済み |
| D12 | ページの CLI の例（`sh`）の検証は、必要になった時に足す。HEAD の `E2000` の kind のページは `text` | Phase 1・2 で `tsuzuri`・`text` だけで足りる。却下: 先に作る（使われない） | 承認済み |
| D13 | `Result<_, String>` を返す `parse_target`・`bindgen::parse_arguments` は型を変えず、受け側で kind を付ける | `src/target.rs` の inline テストと `tests/bindgen.rs` の書き換えが増える割に得がない。却下: 型を `Diagnostic` にする | 承認済み |
| D14 | CLI の引数の解析 5 関数（`parse_arguments`・`next_value`・`parse_size`・`parse_repl_arguments`・`script_arguments`）は `Result<_, Diagnostic>` にして `?` で伝える | `options.validate()` の診断が kind を保ったまま通る。却下: 専用の `ArgumentError` 型（変換が増える） | 承認済み |
| D15 | `since` はマクロの群 `since "x.y.z" { … }` で書く | 653 行に同じ文字列を書かない。却下: 各行に `since` | 承認済み |
| D16 | `explain` は stdout に出し、不明な対象は終了コード 2、コードは大文字だけ | 引数の誤りは 2（既存の規則）。却下: 小文字のコードも受理（誤りを黙って直さない） | 承認済み |

## 付録

| 付録 | ファイル | 内容 |
| --- | --- | --- |
| A | [G23-appendix-pages.md](G23-appendix-pages.md) | Phase 1 のページ 8 本の全文 |
| B | [G23-appendix-registry.md](G23-appendix-registry.md) | 登録簿のブロック（貼る）、コードごとの件数、kind の一覧（653） |
| C | [1](G23-appendix-sites-1.md)・[2](G23-appendix-sites-2.md)・[3](G23-appendix-sites-3.md) | 発行箇所 1,077 行（ファイル・関数・形・コード・kind・メッセージ） |
| D | [G23-appendix-hints.md](G23-appendix-hints.md) | ヒントの表 40 件（実測済み） |
| E | [G23-appendix.md](G23-appendix.md) | E.1 補助関数、E.2 特別な箇所、E.3 数え方、E.5 厳選 kind、E.6 試作の結果、E.7 Phase 2 の手順の割り当て。E.4 は [G23-appendix-probes.md](G23-appendix-probes.md)（誤りの形 89 件） |
| F | [G23-appendix-procedures.md](G23-appendix-procedures.md) | F.1 擬似コード、F.2 Phase 3 の手順、F.3 Phase 4 の手順 |
