# NS05: 強制アンラップ警告 --warn unwrap（Maybe.get・Result.get・Result.get_error の静的検出）

| 項目 | 内容 |
| --- | --- |
| ID | NS05 |
| 優先度 | P2（型システム上 `Maybe` / `Result` を使っていても、安易な `.get` 呼び出しが事実上の NullPointerException になるのを CI・開発時に静的に抑止する） |
| 規模 | S（`--warn unwrap` フラグ、`W1011` と `unchecked_unwrap` kind、呼び出し・関数値参照の走査、テストと文書） |
| 依存 | – |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-22（[GUIDE D-64](GUIDE.md#d-64-null-安全性の徹底ns01ns08)。Null 安全性の調査報告に対する改善チケット作成依頼から起票。HEAD `edc61676` で確認） |
| 承認 | 要承認（CLI `--warn unwrap` と警告コード `W1011` の登録（D-55）。決定事項 D1・D2 が要承認、D3〜D5 は既定案） |
| 手本にする既存実装 | `src/main.rs` の `WARNINGS`（3274 行: `implicit-copy`・`canonical`・`untrusted-extern`）、`src/capabilities.rs` の `untrusted_externs`（ルートパッケージ限定の警告生成）、[A15](_completed/A15-copy-cost-visibility.md) の `--warn implicit-copy`（`W1006`） |
| 主な影響ファイル | `src/main.rs`, `src/driver.rs`, `src/warnings.rs`, `src/diagnostic_kinds.rs`, `src/explain/codes/`, `tests/warnings.rs`, `README.md`, `docs/language.md`, `_tsuzuri/language-reference/compiler/option.md`, `_tsuzuri/language-reference/compiler/diagnostics.md`, `_tsuzuri/language-reference/built-in-types-and-modules/maybe.md`, `_tsuzuri/language-reference/built-in-types-and-modules/result.md` |

## 実装者向け早見

- 範囲: `check`・`build`・`run`・`test`・`script` に `--warn unwrap` を追加する。指定時はルートパッケージの利用者コード（`test`・`bench` 宣言の本体を除く、D3）から標準ライブラリの `Maybe.get`・`Result.get`・`Result.get_error` を呼び出すか関数値として参照した箇所に警告 `W1011`（kind `unchecked_unwrap`）を出す。`--deny-warnings` と併用すればエラー（終了コード 1）になる。既定（フラグなし）の挙動と生成 IR は 1 byte も変わらない。
- 最初に読む: ① 仕様 1〜3 ② `src/main.rs` 284〜290 行・671〜695 行・3272〜3275 行・6555〜6620 行 ③ `src/warnings.rs` ④ `std/Maybe.tc`（`get`）・`std/Result.tc`（`get`・`get_error`）。
- 最初に書くテスト: `tests/warnings.rs` の `warn_unwrap_reports_maybe_and_result_get_in_root_package`。
- 触ってはいけない: `--warn unwrap` を付けないときの診断・終了コード・生成 IR、利用者が自モジュールに定義した同名の `get` 関数への解決。
- 完了の見分け方: `Maybe.get (Some 1)` を含む `Main.tz` で `tsuzuri check Main.tz --warn unwrap` が `W1011` を出し、`--deny-warnings` 付きで終了コード 1 になる。

## 目的

Tsuzuri には `@no_trap` 属性があるが、`@no_trap` は配列添字・除算・`assert`・`extern` などあらゆるトラップ要因をまとめて禁じるため、通常のアプリケーション層や IO 関数には付けにくい。一方、Null 安全言語（Rust の `clippy::unwrap_used`、Kotlin / Swift の強制アンラップ lint に相当）において実務上もっとも防ぎたい事故は、AI や人間が分岐処理を書く手間を惜しんで `Maybe.get` / `Result.get` を呼んでしまうことである。

`--warn unwrap` を独立した警告スイッチとして提供することで、IO や配列アクセスを含む通常のコードでも「検査なしアンラップゼロ」を静的に検査・強制できるようにする。

## 着手条件と停止条件

### 着手条件

- 決定事項 D1・D2 の承認があること。
- `node scripts/repo-kb.mjs brief NS05` を読み、`src/main.rs` の `WARNINGS` 定数の現在の要素数（起票時 3）と `DiagnosticKind` の個数を控える（GUIDE-LITE §11）。

### 停止条件

- **S1** `--warn unwrap` を指定しない実行で出力・診断・IR が変わる。
- **S2** `node scripts/check-cli-docs.mjs` が失敗する（`--help`、`README.md`、`compiler/option.md` の記載漏れ）。
- **S3** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `edc61676` で確認）

- `std/Maybe.tc` 45 行に `def get :: own Maybe<'a> -> 'a`、`std/Result.tc` に `def get :: own Result<'a, 'e> -> 'a` と `def get_error :: own Result<'a, 'e> -> 'e` があり、いずれも不一致時に `unreachable ()` でトラップする。
- `src/main.rs` 3274 行: `const WARNINGS: [&str; 3] = ["implicit-copy", "canonical", "untrusted-extern"];`。未知の `--warn` 名や `--warn` 単体には、この配列から `supported: implicit-copy, canonical, untrusted-extern` を組み立てて `E2000`（終了コード 2）を返す。
- D-55 の予約枠に `W1011`・`W1015` が空いている（`W1013`・`W1014` は D-66 で NC17 に割り当て済み）。

## 変えてはいけない意味

- `Maybe.get`・`Result.get`・`Result.get_error` 自体の型と実行時動作。
- `--warn unwrap` 未指定時のすべてのコマンドの出力。

## 仕様

### 1. CLI オプション

- `WARNINGS` の末尾に `"unwrap"` を追加し、固定順を `implicit-copy, canonical, untrusted-extern, unwrap` とする（D-55 の規則）。
- 有効なサブコマンドは既存の `--warn` と同じ（`check`・`build`・`run`・`test`・`script`。`test --doc` も含む。`fmt` などは既存の `E2000`）。同じ `--warn unwrap` を 2 回指定した場合は既存の重複エラー `warn unwrap specified more than once`。

### 2. 検出対象

次のすべてを満たす式（直接呼び出し `Maybe.get m`、パイプライン `m |> Maybe.get`、部分適用・関数値参照 `Array.map Maybe.get xs` のすべてを含む）に警告 `W1011` を出す。

1. 参照先の関数が `ModuleOrigin::Std` に属する `"Maybe.get"`・`"Result.get"`・`"Result.get_error"` のいずれかである（利用者が定義した `union Maybe` や独自モジュール `Maybe` の `get` は対象外）。
2. 使用箇所が**ルートパッケージ**の利用者ソース内にあり（依存パッケージ内と `std` 内部、および `deriving` やコンパイラ合成コード `Provenance::Generated` は対象外）、かつ `test "..." = ...` および `bench "..." = ...` の本体の外である（D3）。

### 3. 診断形式

- コード: `W1011`、kind: `unchecked_unwrap`
- 位置: 参照した関数名式（`Maybe.get` など）の `Span`
- メッセージ:
  - `Maybe.get`: `'Maybe.get' traps on 'None'; match on the 'Maybe' or use 'Maybe.default_value' / 'Maybe.default_with'`
  - `Result.get`: `'Result.get' traps on 'Error'; match on the 'Result', use 'Result { let! ... }', or use 'Result.default_value'`
  - `Result.get_error`: `'Result.get_error' traps on 'Ok'; match on the 'Result' instead`

## 設計

- `CheckedFunction`（および単相化前の `TypedExpr` 走査）で `TypedExprKind::Function(FunctionRef::User(callee))` / `Closure(callee, _)` を見て、`module.functions[callee]` が `origin.module == ModuleOrigin::Std` かつ `qualified_name()` が上記 3 関数のいずれかである箇所を集める。単相化**前**の型付き AST（またはルートパッケージの `CheckedFunction` 群）で 1 回走査することで、ジェネリック関数の具体化数によらずソース上の 1 箇所につき厳密に 1 回だけ警告し、呼ばれない関数内の `Maybe.get` も漏れなく検出する。
- `test`・`bench` 本体は `FunctionOrigin`（テスト・ベンチマーク由来のフラグ）で除外する。

## 前提とする他チケットのインターフェース

- なし。

## 他チケットへ提供するインターフェース

- 警告コード `W1011`（`unchecked_unwrap`）と CLI `--warn unwrap`。

## 実装手順

1. **変更**: なし（ブランチ `feat/ns05-warn-unwrap`）。**内容**: `node scripts/repo-kb.mjs brief NS05`、`WARNINGS` と `DiagnosticKind` の現在値を確認。**確認**: 基準値を記録。
2. **変更**: `src/diagnostic_kinds.rs`, `src/explain/codes/W1011.md`（または既存の explain 構成に従う）, `src/warnings.rs`, `src/driver.rs`, `src/main.rs`。**内容**: `W1011` 登録、`--warn unwrap` の解析と検査配線。既存の `src/main.rs` の `supported: ...` テスト文字列に `, unwrap` を反映。**確認**: T0、`cargo test --locked --test warnings`、T1。
3. **変更**: `tests/warnings.rs`。**内容**: 直接呼び出し・パイプライン・関数値渡し・ジェネリック関数内・自前 `Maybe.get` 非誤検知・依存パッケージ除外・`test` 宣言内除外・`--deny-warnings` 併用のテストを追加。**確認**: T1。
4. **変更**: 文書と `node scripts/gen-docs.mjs`。**内容**: `README.md`・`docs/language.md`・`option.md`・`diagnostics.md`・`maybe.md`・`result.md` を更新。**確認**: `node scripts/gen-docs.mjs --check`、`node scripts/check-cli-docs.mjs`、`node scripts/check-docs.mjs <変更したページ>`、T2。

## テスト計画

- `tests/warnings.rs`:
  - `Maybe.get`・`std::Maybe.get`・`Result.get`・`Result.get_error` の直接呼び出し・パイプライン・高階関数引数で `W1011` が出ること。
  - 利用者定義モジュールの `Maybe.get` では出ないこと。
  - `test "t" = assert (Maybe.get (Some 1) == 1)` と依存パッケージ内では出ないこと。
  - ジェネリック関数が 2 つの型で具体化されても警告は 1 回だけ出ること。
  - `--deny-warnings` と併用すると終了コード 1 になること。

## ドキュメント

- `README.md`（CLI オプション表）、`docs/language.md`（診断表）、`_tsuzuri/language-reference/compiler/option.md`、`_tsuzuri/language-reference/compiler/diagnostics.md`（`gen-docs.mjs`）、`_tsuzuri/language-reference/built-in-types-and-modules/maybe.md`、`result.md`。

## 受け入れ条件

- [ ] `--warn unwrap` 指定時に仕様 2・3 どおり `W1011` が出力され、`tsuzuri explain W1011` / `unchecked_unwrap` が動作する。
- [ ] `--warn unwrap` 未指定時の出力・IR が一切変わらない。
- [ ] `check-cli-docs.mjs`・`gen-docs.mjs --check`・`check-docs.mjs` がすべて通る。

## 落とし穴

- `src/main.rs` の単体テスト（6580〜6585 行付近）は `supported: implicit-copy, canonical, untrusted-extern` の文字列を厳密比較している（D-55 で明記された仕様）。`unwrap` 追加時はこのテスト文字列を固定順どおり更新する。
- 単相化後の関数群を素朴に走査すると、ジェネリック関数内の 1 つの `Maybe.get` が具体化ごとに重複報告される。必ず `Span` で重複排除するか単相化前のルートパッケージ関数を走査する。

## 対象外

- `Arena.at`・`Vec.at`・配列添字 `xs[i]` の警告（それらは境界・ハンドル検査であり、論理トラップ全般の禁止には `@no_trap` を使う）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | `--warn unwrap` を追加し、`WARNINGS` の 4 番目として登録する（D-55） | 既存の `--warn` 群と同じ UX・`--deny-warnings` 連携で強制アンラップを抑止するため | 要承認 |
| D2 | 警告コードは `W1011`（kind `unchecked_unwrap`）を D-55 予約枠から割り当てる | 独立した `explain` と JSON フィルタリングを可能にするため | 要承認 |
| D3 | `test`・`bench` 宣言の本体は警告対象から除外する | テストコード内では `assert` と同様に `Maybe.get` で失敗時即トラップさせる書き方が正当でノイズになるため | 既定案（実装者はこの案に従う） |
| D4 | ルートパッケージのみを警告対象とし、依存パッケージは除外する | `--warn untrusted-extern`（E15）と同じ規則にそろえるため | 既定案（実装者はこの案に従う） |
| D5 | `Array.get` / `Vec.get`（Tsuzuri の std での要素取得名）は対象に含めない | 本警告は `Maybe` / `Result` の値不在アンラップに焦点を絞るため（添字範囲検査は `@no_trap` の担当） | 既定案（実装者はこの案に従う） |
