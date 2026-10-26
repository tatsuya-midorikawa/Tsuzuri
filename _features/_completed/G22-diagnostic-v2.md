# G22: 診断 v2 の基盤（関連位置・help・fix・expected/found。所有権の移動元・借用元）

| 項目 | 内容 |
| --- | --- |
| ID | G22 |
| 優先度 | P0（D-50 の表） |
| 規模 | L（全体）。Phase 1: M（3〜5 日）、Phase 2: M、Phase 3: M、Phase 4: S（1〜2 日） |
| 依存 | [G20](G20-error-recovery.md)（done。診断を 1 回の実行で集める経路。`Diagnostics`・`DiagnosticSet`・`recovered`）、[G12](G12-lsp-extensions.md)（done。LSP の `publishDiagnostics`・`codeAction`・`verify_edits`）。Phase 2・3・4 は Phase 1 に依存する。未実装の他チケットへの依存はない |
| 後続 | [G23](../G23-diagnostic-kinds-explain-hints.md)（`kind`・`explain`・ヒント DB）、[G24](../G24-tsuzuri-fix.md)（`tsuzuri fix`。`Fix`・`Edit`・`apply_edits` を使う）、[G25](../G25-typed-holes-and-find.md)（`scope`・`candidates`）、[G26](../G26-facts-model.md) Phase 5（`State.moved` の起点と `Loan.span` を facts と共有する）、[G29](../G29-mcp-server.md)（`tz_check` が診断 v2 の JSON を返す）、A20・A21・A22・A23（新しい診断が `related`・`fixes` を最初から持つ）、[B10](../B10-trap-backtrace-and-domains.md)（`E2005` の `trap`。B10 が先に入っていれば `Diagnostic.trap` を `DiagnosticData` へ移して `render_with`・`json_with` に通す。D-51） |
| 状態 | done（Phase 1〜4、2026-10-26） |
| 起票 | 2026-10-18（D-50。HEAD `a2313d19` で確認。作業ツリーの `main` は `9dd0fbbc` で、差は `docs/benchmarks.md` だけ） |
| 承認 | 承認済み（D-50: 人間の包括許可 2026-10-18）。決定事項 D1〜D14 はすべて承認済み。**D1 は D-51 の「Rust の型」の置き場所（`Diagnostic` のフィールドを `Box` に入れる）を変える**ので、D-51 への注記が要る（統括者が台帳へ追記する。根拠は測定値） |
| 改善する劣位 | D-50 の調査結論 (1): コンパイラは移動・借用の事実を内部で持つのに、診断は主スパン 1 個で、移動元も借用元も示さない（HEAD の `use of moved or partially moved value 'text'` は 2 回目の使用だけを指す）。rustc・C# コンパイラの診断（関連位置・help・修正案）に対する劣位。AI が診断 1 件から直せず、ファイルを探索し直す |
| 手本にする既存実装 | 診断の収集: `src/diagnostic.rs` の `Diagnostics`・`DiagnosticSet`・`Diagnostic::render_with_severity`・`json_with_severity`。出力: `src/main.rs` の `print_diagnostics`・`print_with_severity`。復旧: `src/check.rs` の `Checker::recover_expression`・`finish_recovery`・`recovered`。所有権: `src/ownership.rs` の `State`・`Loan`・`Checker::access`・`read_places`・`merge`、`src/ownership_control.rs` の `check_loop`・`loop_summary`。LSP: `src/lsp.rs` の `Session::verify_edits`・`code_actions`。テスト: `tests/diagnostics.rs`・`tests/warnings.rs`・`tests/lsp.rs::code_action_prefixes_unused_locals` |
| 主な影響ファイル | **Phase 1**: `src/diagnostic.rs`、`src/main.rs`（`print_diagnostics`・`parse_arguments`）、`src/docgen.rs`、`src/repl.rs`、`src/cache.rs`、`src/polymorph.rs` と `src/driver.rs` の単体テスト、`tests/{arena,concurrency,json,matrix,matrix_views,rc,specs,tensor}.rs`、`tests/diagnostic_v2.rs`（新規）。**Phase 2**: `src/ownership.rs`、`src/ownership_control.rs`。**Phase 3**: `src/polymorph.rs`、`src/check.rs`、`src/control.rs`、`src/exceptions.rs`、`src/exhaustiveness.rs`、`src/parser.rs`。**Phase 4**: `src/lsp.rs`、`tests/lsp.rs`、文書（「ドキュメント」の表）。付録 `_features/G22-appendix.md`（新規） |

## 実装者向け早見

- **最初に読む**: `src/diagnostic.rs`（全部）、`src/main.rs` の `print_diagnostics`、`src/ownership.rs` の `Checker::access`・`read_places`・`merge`・`eval_block`、D-51（[GUIDE.md](../GUIDE.md#d-51-診断-v2-の共通仕様g22g23g24g25-と診断を出すすべてのチケット)）。手元に付録 A（HEAD の実出力）と付録 B（試作の出力）を置く。
- **付録 F はコード、付録 G はテストの全文**。`main` の複製に Phase 1〜4 を実装し、`cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings`・全テストが通ったもの。名前・メッセージ・型はそのとおりにし、F のコードは写してよい。**G のテストは実装より先に書く**（手順 2・8・14）。
- **最初に書くテスト**: 付録 G-1（手順 2。HEAD で通る）。
- **D1（最重要）**: D-51 のとおりフィールドを `Diagnostic` に直接置くと、`size_of::<Diagnostic>()` が 80 から 264 bytes になり、GUIDE §3.1 の stack の回帰テスト 3 つがすべて stack overflow した（実測）。**`pub struct Diagnostic(Box<DiagnosticData>)` にして `Deref`・`DerefMut` を実装する**（8 bytes。必要 stack が 10〜26% 減る）。`diagnostic.related` のように読み書きでき、書き換えが要るのは構造体リテラルとフィールドの move の 18 か所だけ（手順 3）。
- **触ってはいけないもの**: `--json` の既存 5 キー（`severity`・`code`・`message`・`path`・`span`）の値と順序、診断の整列・重複除去のキー、診断コードの意味、既存のメッセージの文言（テストが照合する。**文言を変えない**。足すのは `related`・`help`・`fixes`・`expected`・`found`・`scope` だけ）、IR・ABI・WASM、`FRONTEND_FORMAT`（AST を変えない）。
- **追加の情報を持たない診断の出力は 1 byte も変わらない**（JSON もテキストも）。`related` は「付ける」と決めた診断（付録 D-1・D-2）にだけ付け、ほかには付けない。
- **Phase の完了**: Phase 1: `cargo test --locked` が緑、`diagnostic_is_one_pointer_wide` と GUIDE §3.1 の stack の 3 つが既定の stack で通り、付録 C の比較が空（**新しい情報は 1 件も出ない**）。Phase 2: 付録 B の `move1`・`loop1`・`loop2`・`branch1`・`branch2`・`partial1`・`borrow1`〜`borrow3`・`life1`・`life3`・`esc1`・`esc3` の出力と一致。Phase 3: 付録 D-2 の全行にテストがあり通る。Phase 4: `tests/lsp.rs` の新しい 3 件と `fix_removes` の単体テストが通り、`code_action_prefixes_unused_locals` は変わらない。
- **作業環境の注意**: debug の test は 2 MiB の stack で動く。この環境（rustc 1.98.1）では lib の `tests::the_deepest_accepted_nesting_is_checked_the_same_on_workers` が **HEAD でも** 既定の stack で溢れる（本チケットと無関係。全体は `RUST_MIN_STACK=4194304` で流す。README と同じ）。GUIDE §3.1 のほかの 3 つは既定の stack で流す。
- **迷ったら**: 「決定事項」→ GUIDE の D-51 → 最も保守的な選択（`related`・`fixes` を付けない）。

## 目的

診断 1 件から、AI も人間も、ほかのファイルを探索せずに直せるようにする。D-51 が決めた拡張（`related`・`help`・`fixes`・`expected`／`found`・`scope`）の**データモデルと出力と LSP を作り**、診断の出し手のうち、AI の修復に最も効く 3 族（所有権、型の不一致・名前・呼び出し、網羅性と重複）にその情報を付ける。

HEAD で測った現状（詳細は「現状」節）: 所有権の診断（`E1012`・`E1013`・`E1014`）42 か所は主スパン 1 個だけで、移動元も借用元も示さない（`move1`: 2 回目の使用だけを指す）。`E1021` は直す編集を持たず、`E1001`・`E1024` は 1 つ目の宣言を示さず、`E1003` は期待の由来を示さない。出力は 1 診断につき引用 1 つ、`--json` は 5 キー、LSP の診断は 5 キー（`relatedInformation`・`data` なし）で、quickfix は未使用ローカル（`W1001`）だけ。

利用者の価値: 「`text` は 5 行目で移動済み」を診断が自分で言い、AI は 1 回の `check` で修正箇所と理由を得る（往復は G31 が数える）。

**非目標**:

- `kind` の付与・登録簿・`explain`（G23）、fix の適用 `tsuzuri fix`（G24）、typed hole・候補の生成・`candidates`（G25）。本チケットの `kind` は常に `None`（キーを出さない）。
- 全 475 か所の移行。AI の修復に効く族だけに付け、残りは G23 が `kind` と一緒に扱う。
- 診断のメッセージの文言の変更、診断コードの追加（D-55 の割り当ては不要）。
- `tsuzuri repl` の診断への `related`・`fixes`（D8）。

## 着手条件と停止条件

### 着手条件

- G20 が `done`（`_features/README.md` の状態欄）。Phase 2 以降は Phase 1 が `done`。
- GUIDE §2.3 の基準を取る: `cargo build --release --locked && RUST_MIN_STACK=4194304 TSUZURI_THREADS=8 cargo test --locked` の通過件数を控える。GUIDE §3.1 の stack の回帰テスト（lib の `bounds_recursive_and_flat_expression_depth`、`tests/polymorphism.rs` の `bounds_type_growing_polymorphic_recursion`、`tests/computations.rs` の `bounds_nested_builder_expansion_not_just_source_syntax`）を**既定の stack で**流し、通ることを確かめる。
- 互換性の比較用に変更前のコンパイラを残す: `mkdir -p /tmp/tz-G22 && cp target/release/tsuzuri /tmp/tz-G22/tsuzuri-before`。付録 A のソースを `/tmp/tz-G22/<名前>/Main.tz` に作り、HEAD の出力が付録 A と一致することを確かめる。
- GUIDE の §6.7（診断コード）、§6.9（走査箇所）、§11.1 の「stack の深さ」、§13 を読む。**§11.1 の教訓**: 再帰する関数の frame は `Result<_, Diagnostic>` の一時値の数で決まる。D1 はこれを縮める変更で、増やす変更を足してはいけない。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と選択肢を報告する（GUIDE §13）。

1. 手順 3 のあと `diagnostic_is_one_pointer_wide` が通らない、または GUIDE §3.1 の stack の回帰テスト 3 つのどれかが既定の stack で溢れる。選択肢: A（`DiagnosticData` の大きい部分を 2 段目の `Box` に移す。D-51 のフィールド名は保つ）、B（`Result<_, Diagnostic>` を返す再帰関数の frame を `#[inline(never)]` の補助関数へ出す）。上限や stack の大きさを上げて通してはいけない。
2. 追加の情報を持たない診断の出力が 1 byte でも変わる（付録 C の比較が差を示す）。出力の部品（`render_with`・`json_with`）の誤りなので、診断の出し手側で直してはいけない。
3. 既存のテストの期待値（コード・メッセージ・位置）を変えないと通らない。メッセージの文言を変えたくなった場合も同じ。
4. `tests/parallel_frontend.mjs` が赤になる、または同じ入力の診断の件数・順序が `TSUZURI_THREADS=1` と `8` で変わる。原因の候補: `holders` が `HashMap`・`HashSet` の反復順に依存している、`related` を共有状態に書いている。
5. Phase 2 で `loop_summary` が span を含めて比較し、`E1017`（`loop ownership analysis exceeds the compiler limit`）が出る、または不動点の反復回数が増える。比較は場所（`Place`）の集合だけにする。
6. `related` の span が、空（`start == end`）・別のソースで `lookup` も知らない・主スパンと同一（ループ越しの `MovedHere` を除く）になった。
7. `ExpectedBecause` が別の原因の `E1003` に付いた（誤帰属）。付けてよいのは、期待の型を渡した式の「尾の位置」の span と一致する `E1003` だけ。
8. `E1021` の fix（`maybe`）を適用した結果が、付録 A の `match1`・`fix2`・`arms6` で通らない、または `arms5` 以外に「適用しても `E1021` が消えない」形が見つかった。選択肢: A（その形では fix を付けない条件を足す）、B（`help` だけにする）。
9. LSP の `codeAction` が `fix_edit` のために、`tests/lsp.rs` の小さいプロジェクトで 1 秒以上かかる。選択肢: A（検証を `codeAction/resolve` へ移す）、B（`isPreferred` の fix だけ検証する）。速度の閾値は CI に入れない（計測して報告する）。
10. D-51 の名前・型・JSON のキー・上限を変えたくなった（D1 の置き場所の変更と D3・D4 の細則を除く）。
11. 新しい予約語・crate・`unsafe`・既定の WASM の import・新しい診断コードが必要に見える。
12. 同じ失敗を直す試みが 2 回続けて失敗した。

## 現状（HEAD `a2313d19` で確認）

### `Diagnostic` と出力

- `src/diagnostic.rs`（318 行）: `Diagnostic { severity, code: &'static str, message: String, span: Span }`（80 bytes）。`Diagnostic::new`・`warning`・`render`・`render_with_severity`・`json`・`json_with_severity`。`Span { start, end, source: Option<usize> }`。収集は `Diagnostics`（`BTreeMap<DiagnosticKey, Diagnostic>`。キーは `(span.source.unwrap_or(root), start, end, severity, code, message)`。**同じキーは先に入った方が残る**）と `DiagnosticSet`。表示上限 50（`MAX_REPORTED_ERRORS`）、収集上限 1,000（`MAX_UNIQUE_DIAGNOSTICS`）。
- 人間向け: `path:line:column: severity[code]: message` に続けて、主の位置の行の引用 2 行（`  {line} \| {snippet}` と `    \| {indent}^`。行が空なら引用なし。`snippet` は列の 40 文字前から 120 文字）。`--json`: 5 キーの 1 行。`span` は `start`・`end`（UTF-8 バイト、`end` 排他）・`line`・`column`・`end_line`・`end_column`。
- 構築: `Diagnostic::new` 475 回、`warning` 8 回（`grep -rn "Diagnostic::new" src | wc -l`、`grep -rn "Diagnostic::warning" src | wc -l`）。構造体リテラルは `src/diagnostic.rs` の 2 つの構築関数と、更新構文の `src/docgen.rs`・`src/repl.rs`（`report` が span を入力の位置に直す）だけ。
- 消費: `src/main.rs` の `print_diagnostics`（呼び出し 3 か所）と `print_diagnostic`（32 か所。`grep -c "print_diagnostic(" src/main.rs` から定義の 1 を引く。ソースのない誤りは `""`）、`src/repl.rs` の `render`、`src/lsp.rs` の `diagnostic_json`・`refresh`・`code_actions`・`verify_edits`。`src/test_runner.rs` は診断を直接扱わない。

### 所有権の記録（Phase 2 の出発点）

- `src/ownership.rs`: `State { locals, aliases, moved: BTreeSet<Place>, generic_moves }`。**移動は場所だけを覚え、どこで移動したかを持たない**。`Loan { place, mutable, parents, view }`。**借用の作成位置を持たない**。`Checker::report(place, diagnostic)` は場所の根（`place.root`）ごとに最初の 1 件だけ報告する。
- 移動を記録するのは `read_places` の 1 か所。使用の検査は `access`。分岐・ループの合流は `Checker::merge`（`moved` を和集合）と `ownership_control.rs` の `check_loop`（`loop_summary` が `(moved, loans, copy_variables)` を比べて不動点を探す）。診断は 42 か所（`grep -c '"E101[234]"' src/ownership.rs src/ownership_control.rs` が 41 と 1。`E1012` 10、`E1013` 28、`E1014` 4。付録 D-1）。
- 関数ごとに別の `Checker`（`check_body`。ラムダの本体も別）。関数は並列に検査され、結果は関数の順に集める。

### 再現（HEAD の出力。`target/release/tsuzuri check <dir>`。ソースは付録 A）

`move1`（2 回目の使用だけを指す）:

```text
move1/Main.tz:6:21: error[E1012]: use of moved or partially moved value 'text'
  6 |     let b = consume text
    |                     ^
```

そのほか: `E1014`（`borrow1`〜`borrow3`。衝突する書き込みだけを指し、借用の作成も生きている理由も出ない）、`E1013`（`life1`）、`E1003`（`type1`・`call1`）、`E1002`・`E1004`・`E1007`（`name1`）、`E1021`（`match1`。`fixes` なし）、`E1001`・`E1024`（`dup2`〜`dup9`）。

### 種（seed）と HEAD の食い違い（実測）

| 項目 | 種の記述 | HEAD の実際 | 本チケットの扱い |
| --- | --- | --- | --- |
| 未解決の値名 | `E1004` | **`E1002`**（`unknown value 'countt'`。`Checker::name`）。`E1004` は未知の型・record・union・型別名（`unknown record, union, or type alias 'Pointt'`）。未知のフィールドは `E1007` | Phase 3 は `E1002`（`scope`・help）と `E1007`（help）に付ける。`E1004` は触らない（G25 が候補と一緒に扱う。D10） |
| 呼び出しの引数の数 | `E1005` | **`E1006`**（`cannot apply 3 arguments to a function accepting 2 arguments`。`call_signature` の 1 か所）。引数が足りないと関数値になり `E1003`（`expected i64, found i64 -> i64`）。型の不一致は `E1003`。`E1005` は `only a function value can be called` と機能の制限の診断 | `E1006`・`E1005`（関数でない値の呼び出し）に help、`E1003` の部分適用に help |
| `E1015` | Phase 3 の対象 | 34 か所は内部エラー・上限・種類の不整合などの雑多な族で、メッセージが注釈の書き方を既に含む | 触らない |
| `test_runner.rs` | 全消費箇所の 1 つ | 診断を扱わない | 対象外 |

### stack の測定（D1 の根拠。macOS arm64、rustc 1.98.1、debug、`RUST_MIN_STACK` の二分探索。刻み 16 KiB。方法は付録 E）

| テスト（既定の stack は 2,097,152） | HEAD | D-51 のままフィールドを直接置く（264 bytes） | D1（`Box`。8 bytes。Phase 1〜4 の試作） |
| --- | --- | --- | --- |
| lib の `bounds_recursive_and_flat_expression_depth` | 約 2,087,424 | **溢れる** | 約 1,544,192（−26%） |
| `tests/polymorphism.rs` の `bounds_type_growing_polymorphic_recursion` | 約 2,071,552 | **溢れる** | 約 1,609,728（−22%） |
| `tests/computations.rs` の `bounds_nested_builder_expansion_not_just_source_syntax` | 約 2,055,680 | **溢れる** | 約 1,847,296（−10%） |

HEAD の余裕は 10〜41 KB（0.5〜2%）しかない。`Diagnostic` に `Option<Box<_>>` を足す案（88 bytes）は、D1 の方が確実に小さいので測っていない。

## 変えてはいけない意味

- 追加の情報（`kind`・`related`・`help`・`fixes`・`expected`・`found`・`candidates`・`scope`）を 1 つも持たない診断は、`--json`・人間向けの出力・LSP の診断が **1 byte も変わらない**。Phase 1 は新しい情報を 1 件も出さない。Phase 2〜4 が足すのは、決めた診断だけ（付録 D）。
- 診断のコード・重要度・位置（主スパン）・メッセージの文言・件数・順序（整列キー）・重複除去のキー。**診断の有無も変えない**（所有権・型・網羅性の判定は 1 つも変えない）。
- 診断の出力はスレッド数・キャッシュの有無・実行の順序に依らない（`TSUZURI_THREADS`、frontend cache）。
- IR・ABI・WASM の import・`FRONTEND_FORMAT`・std。終了コード（成功 0、ソース・ツール・実行の失敗 1、引数の誤り 2）。
- 診断を作らない成功の経路（性能）: 成功する関数の検査は、`State.moved` の値（`Span`、32 bytes）と `Loan.span`（32 bytes）を持つだけで、探索も割り当ても足さない。`BorrowUsedHere` の探索は誤りを出す経路だけ（D8）。debug の `check` の中央値（12 回）は `tests/fixtures/net_wasm` で HEAD 1195.8 ms・試作 1211.0 ms（有意差なし）。

## Phase 分割

| Phase | 内容 | 規模 | 前提 | 単独で出荷できる理由 |
| --- | --- | --- | --- | --- |
| 1 | データモデルと出力: `Diagnostic` の boxed 化（D1）と D-51 の型・ビルダー、`render_with`・`json_with`（別ファイルの `related` を解決する）、`apply_edits`、`main.rs`・`repl.rs` の接続。診断の出し手は変えない | M | なし | 追加の情報を持つ診断がまだ 1 件もなく、出力は 1 byte も変わらない。stack は減る |
| 2 | 所有権の関連位置: `State.moved` の起点、`Loan.span`、`E1012`・`E1013`・`E1014` の 7 か所に `related`・`help`（付録 D-1） | M | Phase 1 | 追加するのは診断の追加情報だけで、所有権の判定は変えない |
| 3 | 型・名前・網羅性: `E1003`（`expected`・`found`・`ExpectedBecause`・部分適用の help）、`E1002`（`scope`）、`E1007`・`E1006`・`E1005`（help）、`E1021`（help・`maybe` の fix）、`E1001`・`E1024`（`FirstDefinedHere`） | M | Phase 1 | Phase 2 と独立。出し手の追加情報だけを足す |
| 4 | LSP と文書: `relatedInformation`・`data`・`fixes` からの quickfix（検証つき）、文書の更新 | S | Phase 1（出す内容は Phase 2・3 の分だけ増える） | LSP は診断にある情報をそのまま写す。Phase 2・3 が未実装でも、`fixes` が空なら quickfix は出ない |

## 仕様

### 型（`src/diagnostic.rs`。D-51 の型に D1 を足したもの。全文は付録 F-1）

```rust
/// D1: 診断は 1 つの `Box` を指す（8 bytes）。フィールドは `Deref`・`DerefMut` で読み書きする。
pub struct Diagnostic(Box<DiagnosticData>);          // `into_data(self) -> DiagnosticData`（フィールドを move するとき）

pub struct DiagnosticData {
    pub severity: Severity, pub code: &'static str, pub message: String, pub span: Span,
    pub kind: Option<&'static str>,                  // G23 が付ける。G22 は常に None
    pub related: Vec<Related>,                       // 最大 16
    pub help: Option<String>,                        // 1 行。改行を含めない
    pub fixes: Vec<Fix>,                             // 最大 8
    pub detail: Detail,
}
pub enum RelatedRole { MovedHere, BorrowedHere, BorrowUsedHere, DeclaredHere, ExpectedBecause, FirstDefinedHere, ViaCall, RequiredBy, Note }
pub struct Related { pub role: RelatedRole, pub message: String, pub span: Span }
pub enum Applicability { Certain, Likely, Maybe }
pub struct Edit { pub span: Span, pub text: String }                          // span の範囲を text に置き換える。空の span は挿入
pub struct Fix { pub title: String, pub applicability: Applicability, pub edits: Vec<Edit> }   // edits は 1〜64
pub struct Candidate { pub name: String, pub signature: String, pub how: String }             // 最大 10
pub struct ScopeEntry { pub name: String, pub ty: String }                    // 最大 32
#[derive(Default)] pub struct Detail { pub expected: Option<String>, pub found: Option<String>, pub candidates: Vec<Candidate>, pub scope: Vec<ScopeEntry> }
/// 付けたソース番号（`Span::source`）のパスと本文を返す。知らなければ `None`。
pub type SourceLookup<'a> = dyn Fn(usize) -> Option<(&'a str, &'a str)> + 'a;
```

`RelatedRole::as_str`（JSON の `role`）は variant 名の snake_case: `moved_here`・`borrowed_here`・`borrow_used_here`・`declared_here`・`expected_because`・`first_defined_here`・`via_call`・`required_by`・`note`。`Applicability::as_str` は `certain`・`likely`・`maybe`。定数: `MAX_RELATED = 16`・`MAX_FIXES = 8`・`MAX_EDITS = 64`・`MAX_CANDIDATES = 10`・`MAX_SCOPE = 32`。

**ビルダー**（`self` を消費して返す。上限を超える分は黙って捨てる。D3・D4）:

| メソッド | 動作 |
| --- | --- |
| `with_kind(&'static str)` | `kind` を設定 |
| `related(RelatedRole, impl Into<String>, Span)` | 末尾に追加。16 件目まで。**span が空でも追加する**（呼び出し側が空を渡さない。D5） |
| `with_help(impl Into<String>)` | `help` を設定（1 行） |
| `fix(impl Into<String>, Applicability, Vec<Edit>)` | 8 件目まで。**`edits` が空、または 65 件以上の fix は丸ごと捨てる**（途中までの適用は意味を壊すので切り捨てない。D3） |
| `expected_found(impl Into<String>, impl Into<String>)` | `detail.expected`・`detail.found` を設定 |
| `with_candidates(Vec<Candidate>)`・`with_scope(Vec<ScopeEntry>)` | 先頭から 10 件・32 件に切る |
| `add_related(&mut self, RelatedRole, impl Into<String>, Span)` | 非消費。`related` と同じ規則 |

`apply_edits(text: &str, edits: &[&Edit]) -> Option<String>`（`src/diagnostic.rs` の関数。LSP と G24 が使う）: `edits` を `(start, end)` の安定整列で並べ、`end <= text.len()`・`start <= end`・両端が文字境界で、隣り合う 2 つが `前.end <= 後.start`（重なりなし。同じ位置への複数の挿入は、渡された順に入る）なら適用した文字列、そうでなければ `None`。

### 出力

**JSON**（1 診断 1 行。値のあるキーだけ出す。順序は固定）: 既存の 5 キー → `kind` → `related` → `help` → `expected` → `found` → `candidates` → `scope` → `fixes`。

| キー | 型と形 | 出す条件 |
| --- | --- | --- |
| `kind` | 文字列 | `kind` が `Some` |
| `related` | `[{"role":..,"message":..,"path":..,"span":{6 キー}}]` | 解決できた要素が 1 件以上（解決できない要素は、その要素だけ省く） |
| `help` | 文字列 | `Some` |
| `expected`・`found` | 文字列 | それぞれ `Some` |
| `candidates` | `[{"name","signature","how"}]` | 空でない |
| `scope` | `[{"name","type"}]` | 空でない |
| `fixes` | `[{"title","applicability","edits":[{"path","span":{6 キー},"text"}]}]` | 解決できた fix が 1 件以上（edit が 1 つでも解決できない fix は丸ごと省く） |

`span` の 6 キー: `start`・`end`・`line`・`column`・`end_line`・`end_column`（既存と同じ。行と列はその span が指すソースの本文から数える）。文字列のエスケープは `json_string`。**解決**: `related`・`edits` の span のソースは、`span.source` が `None` か主スパンと同じなら主のパスと本文、そうでなければ `lookup(index)`。`lookup` が `None` を返す（または `lookup` を渡さない `json`・`render` を使う）ときは解決できない。実例は付録 B の JSON の 5 行。

**人間向け**（`render_with`）: 主の位置（既存の 3 行）に続けて、次を順に出す（各行頭の空白は固定）。

```text
  note: <related.message>
    --> <path>:<line>:<column>
  <line> | <snippet>              ← その行が空でなければ。主の位置と同じ形（`    | <indent>^` の行も）
  help: <help>
  fix (<certain|likely|maybe>): <title>
```

`note` は `related` ごとに 1 組（解決できないものは出さない）、`fix` は fix ごとに 1 行（解決できないものは出さない）。`expected`・`found`・`candidates`・`scope`・`kind` は人間向けには出さない。診断の間の空行は `main.rs` が出す（既存）。実例（付録 B の `move1`）:

```text
move1/Main.tz:6:21: error[E1012]: use of moved or partially moved value 'text'
  6 |     let b = consume text
    |                     ^
  note: value moved here
    --> move1/Main.tz:5:21
  5 |     let a = consume text
    |                     ^
  help: borrow the value with 'ref text' or move it last
```

### 決定表 Phase 2（所有権。全 42 か所は付録 D-1）

**起点の選び方（`MovedHere`）**: `access` が `E1012` を出すとき、`State.moved` の場所のうち、アクセスする場所と重なる（`Place::overlaps`）ものから、起点の span の `(start, end)` が最大のものを 1 つ選ぶ。`merge` で同じ場所の起点が 2 つ以上あるときも、`(start, end)` が大きい方を残す（交換法則が成り立つので、結合の順に依らない）。

| 条件 | `related`（`MovedHere`）のメッセージ | `help` |
| --- | --- | --- |
| 起点の場所が変数全体で、起点が使用より前（`(start, end)` が小さい） | `value moved here` | `borrow the value with 'ref {name}' or move it last` |
| 起点の場所がフィールド・要素（`fields` が空でない）で、起点が前 | `part of the value moved here` | `borrow the field with 'ref' instead of moving it out of '{name}', or move it last` |
| 起点が使用と同じか後（ループの前の反復の移動）で、変数全体 | `value moved here, in the previous iteration of the loop` | 全体と同じ |
| 起点が使用と同じか後で、部分 | `part of the value moved here, in the previous iteration of the loop` | 部分と同じ |
| 起点の span が空（生成されたコード） | **`related` を付けない** | 付ける |

**借用の衝突（`E1014`: `access conflicts with a live borrow`）**: 衝突した借用（`loan`）の作成位置（`Loan.span`）が `BorrowedHere`、その借用を生かしている変数の次の使用が `BorrowUsedHere`。

| 条件 | `related` | `help` |
| --- | --- | --- |
| 借用が外部（引数の借用。`self.external` の根）、または `Loan.span` が空・主スパンと同じ | `BorrowedHere` を付けない | — |
| 借用を持つ局所変数が見つかる（`holders`） | `BorrowedHere`: `value borrowed here`（`mutable` なら `value mutably borrowed here`） | `end the borrow before this access: stop using '{holder}' first, or move this access after its last use`（`holder` は名前が `$` で始まらない最小の id の局所変数） |
| その変数の使用が主スパンより後にある（ソース上で最も前のもの） | `BorrowUsedHere`: `the borrow is used here, so it is still alive` | 同上 |
| 後に使用がなく、前（ループの前の反復）にある | `BorrowUsedHere`: `the borrow is used here, in the next iteration of the loop, so it is still alive` | 同上 |
| 持つ変数が見つからない（呼び出しの実引数の一時的な保持） | `BorrowedHere` だけ | `end the borrow before this access: evaluate this access before the borrow is taken, or borrow a copy` |

**可変性（`E1014`: `mutable access requires 'let mut' ...`）**: アクセスする場所が `let` で宣言した immutable な局所変数（`via` が空、フィールドなし、名前が `$` で始まらず、span が空でない）なら `DeclaredHere`: `'{name}' is declared here without 'mut'`、`help`: `declare '{name}' with 'let mut', or take it by 'ref mut'`。そうでなければ `related` なしで `help`: `write through a 'let mut' variable that holds the whole value, or take it by 'ref mut'`。

**借用が所有者より長生きする（`E1013`。付録 D-1 の 4 か所）**: 所有者の宣言が `DeclaredHere`（ブロック・代入では `'{name}' is declared here and dropped at the end of this block`、関数の戻りでは `'{name}' is declared here and is dropped when the function returns`、パターン・反復では `'{name}' is bound here and ends with this arm or iteration`。名前が `$` で始まる・span が空なら付けない）、借用の作成位置が `BorrowedHere`（`value borrowed here`。span が空・主スパンと同じなら付けない）。`help` は付録 D-1。

そのほかの `E1013`（24 か所。`task`・`Mutex`・`Channel`・並列・`dyn` の制約、region の不一致）と、`E1012` の移動不可（`Drop`・要素・参照からの移動）、`E1014` の残り 2 か所は **`related` を付けない**。メッセージが修正方法を含む（D6・D13）。

### 決定表 Phase 3（型・名前・網羅性。全発行箇所は付録 D-2）

| コード | 発行箇所（HEAD） | 追加する情報 |
| --- | --- | --- |
| `E1003`（`expected {e}, found {f}`） | `src/polymorph.rs` の `Inference::unify` の最後の `Err`、同関数の 2 つの書き換え（`FixedArray` の長さ・`Record`／`Union` の型引数） | `expected_found(e, f)`（メッセージと同じ文字列）。3 か所とも新しい関数 `mismatch(expected: String, found: String, span: Span) -> Diagnostic`（`src/polymorph.rs`）を通す |
| `E1003` に `ExpectedBecause` | `src/check.rs` の `block_bindings`（`let x: T = e`）、`check_modules_collect` の関数本体の検査（戻り値）、`call_expression` の引数のループ（名前つきの関数の引数） | 付ける条件: `detail.expected` がある・`related` が空（内側の文脈が先に付ける）・原因の span が空でない・診断の span が、期待の型を渡した式の「尾の位置」（式自身と、ブロックの結果・`if` の 2 つの枝・`match` の腕の本体を再帰して集めた span）の 1 つ。メッセージ: 注釈 `the type annotation declares this type`、戻り値 `the result type of '{name}' is declared here`、引数 `parameter {n} of '{name}' is declared here`。span は注釈の型・`function.result.span`・宣言の引数の型 `function.parameters[n - 1].ty.span`。引数の文脈は callee が `FunctionRef::User`・`GenericFunction` で名前が `$` で始まらないとき（std の関数も含み、span は `std/…tz` の中） |
| `E1003`（引数が足りない） | `call_expression` の `self.same(&result, &hint, ..)` | `result` が関数型で `hint` が関数型・未確定・誤り型でないとき `help`: `a call with too few arguments evaluates to a function; add the missing argument(s) ({n} more needed)`（`n` は `result` の引数の数） |
| `E1002`（`unknown value '{name}'`） | `Checker::name` の最後の `Err` | `scope`（見える局所を内側から、名前の昇順。最大 32）と `help`: `check the spelling, or define the name with 'let' or 'def'; names in scope: a, b`（名前は最大 8 個。局所がなければ `; names in scope: ..` を省く） |
| `E1007`（未知のフィールド） | `field_access`、`record_literal`、`record_update` | `help`: `the fields of '{record}' are: x, y`（最大 16 個。超えたら `, ...`） |
| `E1006`（`cannot apply {n} arguments ..`） | `call_signature` | **`n > 受け取る数` のときだけ** `help`: `remove the {n - 受け取る数} extra argument(s)`（引数 0 個の呼び出しもこの診断になるので、無条件に引き算しない。落とし穴） |
| `E1005`（`only a function value can be called`） | `call_signature` の 2 か所 | `help`: 数値リテラルは `a number is not a function; remove the argument(s) or call a function`、それ以外は `a value of type {t} is not a function; remove the argument(s) or call a function` |
| `E1021` | `src/exhaustiveness.rs` の `check_coverage` | `help`: `add an arm for '{witness}', or end the match with a wildcard arm '\| _ -> ...'`。**`fix`（`maybe`）**: 明示の `match`（`MatchOrigin::Explicit`。関数ガードは付けない）で、`Checker::writable(witness)` が真のとき。`writable` は wildcard・`true`／`false`・`()`・union の case・タプル（要素も `writable`）だけ真で、レコード・配列長・リスト・リテラルの形は偽（表示が `_` でも）。title `Add the arm '\| {witness} -> unreachable ()'`、edit は `match` 式の末尾（`MatchCoverage.span.end`）への空の span の挿入 ` \| {witness} -> unreachable ()` |
| `E1001`・`E1024`（重複） | 付録 D-2 の 7 か所 | `FirstDefinedHere`: `first defined here`（1 つ目の宣言名の span）。1 つ目が分からない（予約名との衝突）ときは付けない |

### LSP（Phase 4）

| 要素 | 内容 |
| --- | --- |
| `publishDiagnostics` の `relatedInformation` | `related` のうち、ソースが利用者のファイル（`ModuleOrigin::User`）のものを `[{"location":{"uri","range"},"message"}]` にする（`range` は `PositionMapper` で変換）。1 つも作れなければ**キーを出さない** |
| `data` | `kind` があれば `"kind"`、fix があれば `"fixes":[{"title","applicability"}]`。どちらもなければ**キーを出さない** |
| `textDocument/codeAction` | 要求の範囲と重なる診断の `fixes` を、fix ごとに `{"title","kind":"quickfix","diagnostics":[..],"isPreferred": applicability == certain,"edit":{"changes":{uri:[{"range","newText"}]}}}` にする。edit の span が利用者のファイルでない fix は出さない。**出す前に `Session::fix_edit` で検証する**: edit を適用した全文（開いているバッファを重ねる）を再解析し、`fix_removes` が `Ok` を返すこと: (1) その診断のコードの件数が減る、(2) どのコードの件数も増えない。数えるのは誤りだけ（診断が警告のときは警告も）。誤りが直ると、あとの検査が新しい警告を出すことがあるため。満たさない fix は出さない。`context.only` に `quickfix` を含まなければ空の配列（既存） |
| 既存の `W1001` の quickfix | **変えない**。G24 が `W1001` に `certain` の `Fix` を付けてこの分岐を消す（`fix_removes` は警告も数えるので、同じ quickfix が出る） |

### 上限・決定性・互換

- 診断 1 件の上限: `related` 16、`fixes` 8、1 つの fix の `edits` 64（超える fix は丸ごと捨てる）、`candidates` 10、`scope` 32。`Diagnostics` の上限（収集 1,000・表示 50）は変えない。
- 決定性: 追加の情報は診断の整列・重複除去に影響しない（キーは変えない。同じキーなら先の診断が残り、その `related`・`fixes`・`detail` も残る）。`BTreeMap`・`BTreeSet` の反復順だけを使い、`HashMap` の順に依らない。所有権の検査は関数ごとに別の `Checker` で、結果は関数の順に集める（既存）。
- 互換（`docs/stability.md`）: 既存の 5 キーは変えない。足すキーは**値があるときだけ**出す。メッセージの文言と `help`・`related.message` は保証の対象外。`role`・`applicability` の文字列、キーの名前と順序は保証の対象（足すだけ）。edition の差はない。
- 資源上限: 追加の探索（`holders`・`later_use`）は、誤りを出す経路でだけ。`later_use` は関数ごとに合計 `LATER_USE_NODES = 1 << 20`（1,048,576）個の式を見たら以後 `None` を返す（`related` を付けないだけで、診断は出す）。同じ根に報告済みなら診断を作らない（探索もしない）。上限は `find_later_use` の引数 `budget` で、単体テストは小さい値で行う（手本: `honors_the_exact_specialization_limit`）。

## 設計

コードは付録 F（F-1〜F-8）にある。**名前と型は付録 F のとおりにする**。新規・変更する名前: `src/diagnostic.rs` の `DiagnosticData`・`RelatedRole`・`Related`・`Applicability`・`Edit`・`Fix`・`Candidate`・`ScopeEntry`・`Detail`・`SourceLookup`・`apply_edits`・`render_with`・`json_with`、`src/polymorph.rs` の `mismatch`、`src/check.rs` の `DeclaredFunction`・`Names.declared`・`Checker::{expected_because, declared_parameter, scope_entries, partial_application}`・`tail_spans`・`field_names_help`・`duplicate_of`、`src/parser.rs` の `duplicate_function`、`src/exhaustiveness.rs` の `MatchCoverage.explicit`・`Checker::writable`、`src/ownership.rs` の `State.moved`・`Loan.span`・`Checker::{conflict, holders, later_use, escape}`・`find_later_use`・`LATER_USE_NODES`、`src/lsp.rs` の `Session::{diagnostic_value, fix_edit}`・`diagnostic_data`・`fix_removes`。

### Phase 1（出力の部品。診断の出し手は変えない）

| 段 | ファイル・関数 | 変更 |
| --- | --- | --- |
| 1（手順 3） | `src/diagnostic.rs` | `Diagnostic` を `Diagnostic(Box<DiagnosticData>)`（フィールドは HEAD の 4 つだけ）にする。`Deref`・`DerefMut`・`into_data`。`new`・`warning` は Box を作る（`warning` は `new` のあとで severity を書き換える） |
| 1（手順 3） | 構造体リテラルとフィールドの move（18 か所） | `src/docgen.rs` の `map_err(\|error\| Diagnostic { span: .., ..error })` は `\|mut error\| { error.span = error.span.in_source(id); error }`。`src/repl.rs` の `report` は付録 F-8。`src/cache.rs` の `stage`・`src/main.rs` の `parse_arguments` の 3 か所（`error.message`）・`src/polymorph.rs` の単体テスト 1・`src/driver.rs` の単体テスト 3 は `error.into_data().message`（所有権が要らない箇所は `error.message.clone()` でもよい。`(error.code, error.message)` は `let data = error.into_data(); (data.code, data.message)`）。`tests/{arena,concurrency,json,matrix,matrix_views,rc,specs,tensor}.rs` の `rejects` の最後の `error.message` は `error.into_data().message`。見つけ方: コンパイラの E0507・E0560 が全部示す（`cargo check --all-targets`） |
| 2（手順 4） | `src/diagnostic.rs` | `DiagnosticData` に D-51 のフィールドを足す。型・定数・ビルダー・`as_str`（付録 F-1） |
| 3（手順 5） | `src/diagnostic.rs` | `render_with`・`json_with`。既存の `render_with_severity`・`json_with_severity` は空の `lookup`（`&\|_\| None`）で委譲する。`write_quote`・`span_json` は既存の出力の組み立てを切り出す（付録 F-2） |
| 4（手順 6） | `src/diagnostic.rs` | `apply_edits`（付録 F-2） |
| 5（手順 7） | `src/main.rs`・`src/repl.rs` | `print_diagnostics`: `files: Vec<(String, &str)>`（`project.sources` に `project.manifests` を続けた順。`Project::source_for` と同じ番号）から `lookup` を作り、1 件ずつ `render_with`・`json_with` の結果を `eprintln!`。`print_diagnostic`（ソースのない 32 か所）は変えない。REPL の `report` は `related`・`fixes` を空にする（付録 F-8） |

### Phase 2（所有権）

1. **`State.moved`**: `BTreeSet<Place>` を `BTreeMap<Place, Span>` にする。書き換えは、`read_places` の `insert(place.clone(), expression.span)`、`access` の検索（付録 F-3）、`ownership.rs` の `retain` 2 か所（`eval_block` と `Assign`）、`ownership_control.rs` の `retain` 5 か所（`view`・`finish_control_scope`・`check_loop`・`eval_match` の 2 か所。`\|place\| ..` を `\|place, _\| ..` に）、`loop_summary`（**場所の集合 `keys().cloned().collect()` だけを比べる**）。`revive_generic_moves` の `remove(&moved)` は変えない。
2. **`merge`**: `other.moved` の各 `(place, span)` を `entry(place).and_modify(.. 大きい (start, end) を残す).or_insert(span)`（付録 F-3）。
3. **`Loan.span`**: `Loan` に `span: Span`、`Checker::loan(place, mutable, parents, span)` に引数を足す。9 か所の呼び出しの span は付録 D-1 の表の「借用の作成位置」。`ownership_control.rs` の `view` は `Loan { .., span: projection.span }`。
4. **`Checker.body: &'a TypedExpr`**（`check_body` が `body` を渡す。ラムダの本体は別の `Checker` で、その本体）と `later_use_budget: usize`（`LATER_USE_NODES` で始める）。
5. **`access`**: 付録 F-3 の 3 か所（`E1012` の `MovedHere`、`E1014` の `DeclaredHere`、`E1014` の衝突。衝突は**同じ根が報告済みなら診断を作らずに戻る**）。`conflict`・`holders`・`later_use`・`find_later_use` は付録 F-4。
6. **`escape`**（付録 F-5）: `E1013` の 4 か所が共通で呼ぶ。`DeclaredHere`、`BorrowedHere`、`help`。

### Phase 3（型・名前・網羅性）

1. **`mismatch`**（付録 F-6）と `unify` の 3 か所。書き換えのクロージャの `mut error` は不要になるので `error` にする（残すと `-D warnings` で失敗する）。
2. **`expected_because`**（付録 F-6）: 文脈（`let` の注釈・関数の戻り値・引数）が `let from = self.recovered.len();` を控え、期待の型を渡して式を検査し、`result` と `recovered[from..]` の `E1003` に付ける（回復する経路では `E1003` が `Err` で返らず `recovered` に積まれる）。`tail_spans(&Expr)` は付録 F-6。
3. **`Names.declared`**: `check_modules_collect` の関数の id ごとのループで、`names.declared = vec![DeclaredFunction::default(); function_declarations.len()]` を先に作り、テスト・bench を除く関数に `DeclaredFunction { name, parameters: 引数の型の span }` を入れる（付録 F-6）。`declared_parameter(callee, index)` は callee が `FunctionRef::User(id)`・`GenericFunction(id, _)` のときだけ `Some`。
4. **`scope_entries`**: `self.scopes` を内側から、名前の昇順（`BTreeMap`）で、`_`・`$` 始まり・`Provenance::User` でない局所・既出の名前を除き、`Type::display` した型と一緒に最大 32 件。
5. **`E1021`**: `match_value(.., coverage: Option<MatchOrigin>)`（`Some` で網羅性を検査。呼び出し 3 か所は `control.rs` の 2 つが `None`・`origin.checks_coverage().then_some(*origin)`、`exceptions.rs` が `None`）、`MatchCoverage.explicit = origin == MatchOrigin::Explicit`、`check_coverage` が help と fix（付録 F-7）。
6. **重複の `FirstDefinedHere`**（7 か所。付録 F-7）:

| # | 場所 | 1 つ目の span |
| --- | --- | --- |
| 1 | `parser.rs` の `duplicate function signature` | `signatures.get(&name.text)` か `program.externs` の同名の `name.span` |
| 2 | `parser.rs` の `duplicate function definition`（`def name :: .. = ..` が定義済みの名前） | `definitions.iter().find(..)` の `name.span` |
| 3 | `parser.rs` の `duplicate function definition`（`fn` の実装） | 同上 |
| 4 | `check_modules_collect` の record の重複（`E1001`） | `record_declarations[..id]` の同じ module・同じ名前の `name.span` |
| 5 | 同 record のフィールドの重複 | `record.fields` で同名で span が違うもの |
| 6 | 同 関数の引数の重複 | `function.parameters` の同名の最初 |
| 7 | `collect_unions` の `duplicate case`（`E1024`） | `union.cases[..case]` の同名の `name.span` |

`parser.rs` の `let` 形の `duplicate function definition` は到達できない（分岐の条件が `!defined_names.contains(name)`）ので触らない。`let mut definitions = Vec::new();` は `Vec<Definition>` の注釈が要る（`.iter().find(..)` を書くと型が決まらない）。

### Phase 4（LSP）

1. **`Session::diagnostic_value`**（付録 F-8）: 既存の `diagnostic_json(mapper, text, diagnostic)` に、`related` ごとに、`span.source`（`None` は主と同じ）が `project.sources` の `ModuleOrigin::User` のときだけ `{"location": {"uri": self.uri(&file.path)?, "range": mappers[id].range(&file.text, span)?}, "message"}`（1 つも作れなければキーを出さない）と、`diagnostic_data`（`data`）を足す。`refresh` の `diagnostic_json` の呼び出しをこれに替える。`W1001` の quickfix の `diagnostics` の `diagnostic_json` は変えない。
2. **`Session::fix_edit`**（付録 F-8）: fix の edit を `span.source` で分け、`ModuleOrigin::User` のファイルだけを `apply_edits` し、開いているバッファに重ねて `Project::load_with_overlays` → `analyze` で再解析する。`fix_removes(diagnostic, &state.diagnostics, &再解析の診断)` が `Ok` なら `{"changes": {uri: [{"range","newText"}]}}` を返し、`Err` ならそれを返す。
3. **`code_actions`**: 既存の `W1001` のループのあとに、要求の範囲と重なる診断の `fixes` を順に `fix_edit` へ渡し、`Ok` のものだけ quickfix にする（付録 F-8）。

## 前提とする他チケットのインターフェース

本チケットは他の未実装のチケットを**使わない**（依存は done の G20・G12 だけ）。後続が使う名前:

| チケット | 使う名前 | 本チケットでの状態 |
| --- | --- | --- |
| G23 | `Diagnostic::with_kind`・`kind`・JSON の `kind`（5 キーの直後）。`Diagnostic::of(Kind, ..)` は G23 が足す | 器だけ。本チケットは `kind` を付けない |
| G24 | `Fix`・`Edit`・`Applicability`・`apply_edits`・JSON の `fixes[].edits[].span.{start,end}`。`W1001` の quickfix を `Fix` 経由にするのは G24 | `E1021` の `maybe` が最初の `fix`。LSP の既存の `W1001` の処理は変えない。`fix_edit` は警告の fix も検証できる |
| G25 | `Detail.candidates`・`Candidate`・`Detail.scope`・`Checker::scope_entries`（型が期待型と一致するものを先にする並べ替えは G25 が足す） | `E1002` が `scope` を持つ。`candidates` は G25 |
| G26 Phase 5 | `State.moved`（起点の span）と `Loan.span`。同じ記録から診断の `related` と facts の `ownership` の列を作る | 診断だけが読む。facts の sink は G26 が足す |
| A20・A21 | `RelatedRole::ViaCall`・`RequiredBy`（`Diagnostic::related(..)`。呼び出しの連鎖は最大 16 件） | 型と JSON の `via_call`・`required_by` だけ。本チケットは使わない |

## 実装手順

各手順の最後で `cargo build` と `cargo test --locked`（`RUST_MIN_STACK=4194304`。GUIDE §3.1 の 3 つは既定の stack）が緑であること。**1 段ずつ**進め、通過件数を控える。`cargo fmt --all` と `cargo clippy --all-targets -- -D warnings` は Phase の最後（手順 7・13・19・22）で通す。

### Phase 1

- **手順 1: ベースライン**。変更: なし。内容: 「着手条件」のとおり。`grep -c` の数（`Diagnostic::new` 475、`warning` 8、所有権の 42）と付録 A の出力を確かめる。確認: すべて一致。
- **手順 2: 出力の凍結（テスト先行）**。変更: `tests/diagnostic_v2.rs`（新規）。内容: 付録 G-1 をそのまま作る（定数 16 個は付録 A のソース）。HEAD の書式を書き下した参照 `old_text`・`old_json` と `Diagnostic::render`・`json` を、16 本のソースの全診断で比べる。`has_extras`・`has_text_extras` は `false` を返す版（手順 4 で入れ替える）。確認: **HEAD で通る**（`cargo test --test diagnostic_v2`）。`cargo clippy --all-targets -- -D warnings`。
- **手順 3: Box 化（D1）**。変更: 「設計」Phase 1 の段 1。内容: `Diagnostic(Box<DiagnosticData>)`（付録 F-1 の `DiagnosticData` から `kind`・`related`・`help`・`fixes`・`detail` を除いた 4 フィールド）と 18 か所の書き換え。単体テスト `diagnostic_is_one_pointer_wide`（付録 G-5）。確認: ビルドが通り、通過件数が手順 1 と同じ。GUIDE §3.1 の 3 つが既定の stack で通る（付録 E の値以下）。`cargo clippy --all-targets -- -D warnings`。
- **手順 4: 型・ビルダー・上限**。変更: `src/diagnostic.rs`、`tests/diagnostic_v2.rs`。内容: 付録 F-1。`tests/diagnostic_v2.rs` の `use` と `has_extras`・`has_text_extras` を付録 G-2 に入れ替える。確認: `builders_keep_the_limits`・`role_and_applicability_names_are_stable`（付録 G-5）と手順 2 のテストが通る。
- **手順 5: `render_with`・`json_with`**。変更: `src/diagnostic.rs`。内容: 付録 F-2（`apply_edits` 以外）。確認: 手順 2 のテストが通り続ける。`without_extras_the_output_is_unchanged`・`json_key_order_and_escaping`・`related_in_another_source_uses_the_lookup`・`unresolved_related_and_fixes_are_left_out`（付録 G-5）。
- **手順 6: `apply_edits`**。変更: `src/diagnostic.rs`。内容: 付録 F-2。確認: `apply_edits_orders_and_rejects_overlaps`。
- **手順 7: 接続**。変更: `src/main.rs`（`print_diagnostics`）、`src/repl.rs`（`report`）。内容: 「Phase 1」の表の段 5（付録 F-8）。`related` を持つ診断がまだないので、`lookup` の動作は手順 16 の `related_in_another_file_is_rendered` が初めて確かめる。確認: 付録 C の互換性の比較（変更前後で 1 byte も変わらない）、`cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test --locked`。**この Phase の完了**。

### Phase 2

- **手順 8: テスト先行**。変更: `tests/diagnostic_v2.rs`。内容: 付録 G-3 を足す（`use` と定数 `MOVE1`〜`NO6` を含む）。確認: `cli_json_keeps_old_keys_first` から `escaping_borrow_names_owner_and_borrow` までの 8 件が失敗する（`related` が空）。`moves_in_loops_still_reach_the_fixpoint`・`ownership_without_related_is_unchanged` は通る。
- **手順 9: `State.moved` を map にする（意味を変えない）**。変更: `src/ownership.rs`・`src/ownership_control.rs`。内容: 「設計」Phase 2 の 1・2。診断はまだ付けない。確認: `cargo test --locked` が手順 7 と同じ件数で通る（手順 8 の 8 件を除く）。`node tests/parallel_frontend.mjs target/release/tsuzuri`（要 `cargo build --release --locked`）。
- **手順 10: `MovedHere`**。変更: `src/ownership.rs` の `access`。内容: 付録 F-3 の 1 つ目。確認: `moved_value_names_the_move`・`moved_in_a_loop_says_previous_iteration`・`moved_in_branches_names_the_later_move`・`partial_move_says_part_of_the_value` が通る。
- **手順 11: 借用の作成位置と使用位置**。変更: `src/ownership.rs`・`src/ownership_control.rs`。内容: 「設計」Phase 2 の 3〜5（`Loan.span`・`Checker.body`・`later_use_budget`・`conflict`・`holders`・`later_use`・`find_later_use`。付録 F-3 の 3 つ目と F-4）。`later_use_tests` の単体テスト（付録 F-4）。確認: `borrow_conflict_names_borrow_and_use`・`later_use_tests::finds_the_use_after_and_before_a_span_and_honors_the_budget` が通る。`moves_in_loops_still_reach_the_fixpoint`。
- **手順 12: 可変性と `E1013`**。変更: 同上。内容: `E1014` の `DeclaredHere`（付録 F-3 の 2 つ目）と `escape`（付録 F-5）の 4 か所。確認: `immutable_access_names_the_declaration`・`escaping_borrow_names_owner_and_borrow`。
- **手順 13: Phase の確認**。変更: なし。内容: `ownership_without_related_is_unchanged`（付録 D-1 の「付けない」行の 6 件）が通ることを確かめる。確認: `node tests/parallel_frontend.mjs target/release/tsuzuri`、`node tests/examples.mjs target/release/tsuzuri`、付録 C の比較（Phase 2 の規則）、`cargo fmt`・`cargo clippy`。**この Phase の完了**。

### Phase 3

- **手順 14: テスト先行**。変更: `tests/diagnostic_v2.rs`。内容: 付録 G-4 を足す（`use` と定数を含む）。確認: `inner_mismatch_has_no_because`・`reserved_names_have_no_first_definition` 以外の 15 件が失敗する。
- **手順 15: `mismatch`**。変更: `src/polymorph.rs`（`mismatch` と `unify` の 3 か所。`mut error` を `error` にする）。確認: `mismatch_has_expected_and_found`。
- **手順 16: `ExpectedBecause`**。変更: `src/check.rs`（`DeclaredFunction`・`Names.declared`・`expected_because`・`declared_parameter`・`tail_spans`・3 つの文脈）。確認: `annotation_mismatch_names_the_annotation`・`result_mismatch_names_the_result_type`・`argument_mismatch_names_the_parameter`・`inner_mismatch_has_no_because`・`std_parameter_note_points_into_std`・`related_in_another_file_is_rendered`。
- **手順 17: help と `scope`**。変更: `src/check.rs`（`partial_application`・`scope_entries`・`Checker::name`・`field_access`・`record_literal`・`record_update`・`field_names_help`）、`src/polymorph.rs`（`call_signature`）。確認: `partial_application_gets_a_help`・`unknown_value_lists_the_scope`・`unknown_field_lists_the_fields`・`call_helps`。`tests/computations.rs` が通る（引数 0 個の `E1006` の回帰）。
- **手順 18: `E1021`**。変更: `src/control.rs`・`src/exceptions.rs`・`src/exhaustiveness.rs`。確認: `non_exhaustive_match_offers_a_fix_that_compiles`・`no_fix_for_unwritable_witnesses`・`nested_inline_match_fix_is_not_verified_here`・`cli_json_of_a_fix_has_edits_with_spans`。
- **手順 19: `FirstDefinedHere`**。変更: `src/parser.rs`・`src/check.rs`（7 か所）。確認: `duplicates_name_the_first_declaration`・`reserved_names_have_no_first_definition`。`node tests/parallel_frontend.mjs`、付録 C の比較（Phase 3 の規則）、`cargo fmt`・`cargo clippy`。**この Phase の完了**。

### Phase 4

- **手順 20: `relatedInformation`・`data`**。変更: `src/lsp.rs`、`tests/lsp.rs`。内容: 付録 F-8 の `diagnostic_value`・`diagnostic_data`・`refresh` の変更と、テスト `publishes_related_information_and_data`。確認: そのテストが通り、`code_action_prefixes_unused_locals`・`capabilities_are_advertised_exactly` が変わらず通る。
- **手順 21: `fix_edit` と quickfix**。変更: `src/lsp.rs`、`tests/lsp.rs`。内容: 付録 F-8 の `fix_edit`・`fix_removes`・`code_actions` の追加、テスト `code_action_offers_a_verified_fix`・`code_action_omits_a_fix_that_does_not_verify` と `src/lsp.rs` の単体テスト `a_fix_must_remove_its_diagnostic_and_add_nothing`。確認: 3 件の新しいテストと単体テスト 1 件と既存の `tests/lsp.rs` が通る。
- **手順 22: 文書と最終確認**。変更: 「ドキュメント」の全ページ。確認: `node scripts/check-docs.mjs <変更した全ページ>`、`cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`RUST_MIN_STACK=4194304 TSUZURI_THREADS=8 cargo test --locked`、GUIDE §3.1 の 3 つ（既定の stack）、`node tests/parallel_frontend.mjs target/release/tsuzuri`、付録 C の比較。

## テスト計画

期待値は付録 G（試作で通ったテストの全文）と付録 B の出力。**コンパイラの現在の出力を写さない**。拒否のテストは診断コードを照合する。ソースは付録 A の名前（例: `move1`）で、テストの中に定数として書く。パスは `ends_with` で比べる（Windows の区切り）。

### 単体テスト

`src/diagnostic.rs`（付録 G-5）: `diagnostic_is_one_pointer_wide`（`Diagnostic`・`Result<(), Diagnostic>`・`Option<Diagnostic>` が `usize` と同じ大きさ）、`without_extras_the_output_is_unchanged`（`W1003` の既存の期待値 `M.tz:2:3: warning[W1003]: unreachable\n  2 | bc d\n    |   ^` と手で書いた 5 キーの JSON）、`builders_keep_the_limits`（`related` 17 件目・`fix` 9 件目・`candidates` 11 件目・`scope` 33 件目が入らない。edit が 0 件と 65 件の fix は入らず、64 件は入る）、`role_and_applicability_names_are_stable`、`json_key_order_and_escaping`（全キーを持つ診断が手で書いた 1 行と完全一致。`"`・`\n`・`\\`・U+0001 のエスケープ）、`related_in_another_source_uses_the_lookup`（別のソースの path・line・column・引用になる）、`unresolved_related_and_fixes_are_left_out`、`apply_edits_orders_and_rejects_overlaps`（順不同、同じ位置への 2 つの挿入は渡した順、重なり・範囲外・文字境界の途中は `None`、隣り合う編集は重なりでない）。`src/ownership.rs`（付録 F-4）: `later_use_tests::finds_the_use_after_and_before_a_span_and_honors_the_budget`。`src/lsp.rs`（付録 F-8）: `a_fix_must_remove_its_diagnostic_and_add_nothing`（誤りの fix は、直ったあとに現れる警告を数えない。警告の fix は警告を数える）。

### 統合テスト（`tests/diagnostic_v2.rs`。新規。28 件。入力と期待の全文は付録 G）

- Phase 1: `json_without_extras_is_byte_identical`（16 本のソースの全診断。追加の情報がなければ `render`・`json` が `old_text`・`old_json` と完全一致、あれば `old_*` で始まる。**HEAD で通る**）。
- Phase 2（付録 B の出力と同じ）: `cli_json_keeps_old_keys_first`（`move1` の `--json` の 1 行。キーの順序・`related` の 6 キー）、`moved_value_names_the_move`（`move1`）、`moved_in_a_loop_says_previous_iteration`（`loop1`・`loop2`: `related[0].span == span`）、`moved_in_branches_names_the_later_move`（`branch1`・`branch2`: 後ろの枝）、`partial_move_says_part_of_the_value`（`partial1`）、`borrow_conflict_names_borrow_and_use`（`borrow1`・`borrow2`）、`immutable_access_names_the_declaration`（`borrow3`）、`escaping_borrow_names_owner_and_borrow`（`life1`・`life3`・`esc1`・`esc3`）、`moves_in_loops_still_reach_the_fixpoint`（`loop2` が `E1012` 1 件だけ）、`ownership_without_related_is_unchanged`（`no1`〜`no6`: `related`・`help` なしで、メッセージが HEAD と同じ）。
- Phase 3: `related_in_another_file_is_rendered`（`multi1` を CLI で。`Lib.tz:1:19` を指す `note`、JSON の `related[0].path`）、`mismatch_has_expected_and_found`（`type1`・`mismatch1`・`mismatch2`）、`annotation_mismatch_names_the_annotation`・`result_mismatch_names_the_result_type`・`argument_mismatch_names_the_parameter`・`inner_mismatch_has_no_because`（`type1`・`call1`）、`std_parameter_note_points_into_std`（`std2`。`related[0].path` が `std/Array.tz`）、`partial_application_gets_a_help`（`call1` の `f`）、`unknown_value_lists_the_scope`（`name1`）、`unknown_field_lists_the_fields`（`field1`）、`call_helps`（`call1` の `g`・`nofn`）、`non_exhaustive_match_offers_a_fix_that_compiles`（`match1`・`fix2`・`arms6`: 全部適用した結果が `analyze_all` を通る）、`no_fix_for_unwritable_witnesses`（`fix3`・`fix4`: 件数 `[0, 1, 0]`、関数ガードは fix なし）、`nested_inline_match_fix_is_not_verified_here`（`arms5`: 適用後も `E1021` が残る）、`duplicates_name_the_first_declaration`（`dup2`・`dup3`・`dup5`〜`dup9` の 7 か所）、`reserved_names_have_no_first_definition`（`dup12`）、`cli_json_of_a_fix_has_edits_with_spans`（`fix2`）。

### LSP（`tests/lsp.rs`。付録 F-8 に全文）

`publishes_related_information_and_data`（`move1` を LSP に開く。`relatedInformation[0]` の `location.uri` が同じファイル、`range` が行 4 の `text`（0 始まり）、`message` が `value moved here`。`data` が出ない）、`code_action_offers_a_verified_fix`（単一の `match` の `E1021`: 1 件の quickfix、`title` が `Add the arm '| Triangle _ -> unreachable ()'`、`isPreferred` が `false`、edit の `range` が行 5・文字 19 の空、`data.fixes[0].applicability` が `maybe`）、`code_action_omits_a_fix_that_does_not_verify`（`arms5` 相当: quickfix が 0 件）。

### 既存テストへの影響

**期待値が変わるテストはない**。根拠: 診断を出すのは追加の情報だけで、コード・メッセージ・位置・件数・順序は変えない。試作（Phase 1〜4 を実装）で `RUST_MIN_STACK=4194304 TSUZURI_THREADS=8 cargo test --locked --no-fail-fast` が 108 のテスト対象・1,490 件すべて通り（`ignore` の 1 件は既存）、既存のテストの修正は 18 か所（手順 3）だけだった。増えるのは `tests/diagnostic_v2.rs` の 28 件・`src/diagnostic.rs` の単体テスト 8 件・`src/ownership.rs` と `src/lsp.rs` の単体テスト各 1 件・`tests/lsp.rs` の 3 件。GUIDE §3.1 の 3 つは既定の stack で通る（付録 E の値）。

### E2E・IR

診断は IR に影響しない。`node tests/examples.mjs target/release/tsuzuri` と `node tests/parallel_frontend.mjs target/release/tsuzuri`（診断の順序・件数が `TSUZURI_THREADS` に依らないこと）を Phase 2・3 の最後に流す。native・WASM の `-O0`／`-O3` の確認は不要（生成コードに触れない）。

## ドキュメント

機能を変える Phase ごとに、同じ PR で更新する（AGENTS.md の「Language reference」）。変えたページは `node scripts/check-docs.mjs <ページ>` を通す。`text` ブロックの出力は検証されないので、その例のソースを `tsuzuri check` にかけた**実出力**を貼る（付録 B の形と同じになる）。

| Phase | ページ | 更新 |
| --- | --- | --- |
| 1 | `docs/architecture.md` の「ファイル \| 責務」の `src/diagnostic.rs` の行 | 追加情報（関連位置・help・fix・detail）、`Diagnostic` が `Box`（8 bytes。stack の理由）、`apply_edits` |
| 1 | `docs/stability.md` の「安定として保つもの」の診断の行と「安定として保たないもの」の診断の行 | 追加キー（`kind`・`related`・`help`・`expected`・`found`・`candidates`・`scope`・`fixes`）は値があるときだけ出す。`role`・`applicability` の文字列とキーの名前・順序は安定。`related.message`・`help`・fix の `title` と `text` は保証しない |
| 2 | `grep -rn 'error\[E101[234]\]' _tsuzuri/language-reference` が示す例のうち、メッセージが `use of moved or partially moved`・`access conflicts with a live borrow`・`mutable access requires`・`borrowed value does not live long enough` のもの（`ownership-and-memory/{ownership,borrowing,lifetimes}.md`、`async-tasks-and-lazy/{async,task}.md`、`built-in-types-and-modules/owned.md`） | 出力に `note`・`help` の行を足す。`MovedHere` などの意味を `ownership.md`・`borrowing.md`・`lifetimes.md` に 1 段落ずつ。付けない診断（付録 D-1 の「付けない」行。例: `cannot move a non-Copy element ..`）の例は変えない |
| 2・3 | `_tsuzuri/language-reference/compiler/diagnostics.md` | 「表示形式」に関連位置・help・fix の節と例（`move1`）。「JSON の 1 行 1 オブジェクト」の表に追加キーと省略規則。「エラーコード」の `E1002`・`E1003`・`E1006`・`E1007`・`E1012`〜`E1014`・`E1021`・`E1001`・`E1024` の行に追加情報を 1 句 |
| 3 | `docs/language.md` の「診断」（JSON の例のあと） | 追加キーと人間向けの出力（`note`・`help`・`fix` の行）の段落 |
| 4 | `compiler/diagnostics.md` の「LSP での表示」（「クイックフィックスは `W1001` 向け」の段落）、`vsc/README.md`（未使用ローカルのクイックフィックスの行と「`W1001` 以外のクイックフィックスには未対応」の行）、`README.md`（「開発・AI 支援」の行の「位置情報付き JSON 診断」と LSP の「主要機能」の「クイックフィックス」） | `relatedInformation`・`data`・`fixes` の quickfix（検証済みのもの。現在は `E1021` の `maybe` だけ）。拡張は LSP の `codeAction` をそのまま出すので、拡張の機能の記述を実際に合わせる（拡張のコードは変えない） |

検索: `grep -rnE "E1012|E1013|E1014|E1021|relatedInformation|quickfix|位置情報付き" _tsuzuri/language-reference docs README.md` の全ヒットを確かめる。`_features/README.md` の状態欄・GUIDE の台帳（D-51 への D1 の注記、`apply_edits`・`SourceLookup`・role の文字列）は統括者が更新する。完了時に `_features/` のこのチケットを `_completed/` へ移す。

## 受け入れ条件

### Phase 1

- [ ] `Diagnostic` が 8 bytes で、`diagnostic_is_one_pointer_wide` が通る。GUIDE §3.1 の 3 つが既定の stack で通る（付録 E の値以下）。
- [ ] 18 か所の書き換え以外の既存のコードを変えていない（`git diff --stat` で確認）。
- [ ] 追加の情報を持たない診断の出力が、付録 C の手順で変更前と 1 byte も同じ（JSON・テキスト・終了コード）。`json_without_extras_is_byte_identical` が通る。
- [ ] 型・ビルダー・上限・`render_with`・`json_with`・`apply_edits` の単体テストが通る。`cargo fmt`・`cargo clippy --all-targets -- -D warnings` が緑。
- [ ] `docs/architecture.md`・`docs/stability.md` を更新した。

### Phase 2

- [ ] 付録 B の `move1`・`loop1`・`loop2`・`branch1`・`branch2`・`partial1`・`borrow1`〜`borrow3`・`life1`・`life3`・`esc1`・`esc3` の出力と一致する（行・列・メッセージ・`help`）。
- [ ] 付録 D-1 の 7 か所に付き、35 か所には付かない。診断の件数・コード・メッセージ・位置・順序が変わらない。
- [ ] `TSUZURI_THREADS=1` と `8` で出力が同じ（`tests/parallel_frontend.mjs`）。`E1017` の新しい発生がない。
- [ ] 「ドキュメント」の Phase 2 の行のページと `diagnostics.md` を更新し、`check-docs.mjs` が通る。

### Phase 3

- [ ] 付録 D-2 の全行にテストがあり、通る。`E1021` の fix は付録 A の `match1`・`fix2`・`arms6` で適用して通り、`arms5` は `nested_inline_match_fix_is_not_verified_here` が記録する。
- [ ] `ExpectedBecause` は期待の型を渡した式の尾の位置だけに付き、内側の文脈が先に付く。`inner_mismatch_has_no_because` が通る。
- [ ] `E1002` の `scope` が 32 件以下で、内側から並ぶ。`tests/computations.rs` が通る。

### Phase 4

- [ ] `tests/lsp.rs` の新しい 3 件と `src/lsp.rs` の単体テスト 1 件が通り、`code_action_prefixes_unused_locals`・`capabilities_are_advertised_exactly` が変わらず通る。
- [ ] 検証に通らない fix は出ない。`fix_removes` が編集の前後で診断の件数を比べ、単体テストが誤りの fix と警告の fix の両方を確かめる。
- [ ] `compiler/diagnostics.md` の LSP の節・`vsc/README.md`・`README.md` を更新し、`grep -rn "W1001" _tsuzuri/language-reference vsc/README.md README.md` に残りの古い記述（クイックフィックスは `W1001` だけ）がない。

## 落とし穴

| 落とし穴 | 検出 |
| --- | --- |
| `loop_summary` に span を入れると、不動点が収束せず `E1017` になる | 手順 9 の確認と `moves_in_loops_still_reach_the_fixpoint`。`loop_summary` が `keys()` だけであること |
| `merge` で「自分の起点を残す」と、分岐の順で結果が変わる（交換法則が成り立たない） | 大きい `(start, end)` を残す（付録 F-3）。`HashMap` を使わない |
| `diagnostic.help.unwrap()` のように `Diagnostic` からフィールドを move すると E0507。`Diagnostic { span, ..error }` の更新構文は D1 のあと書けない（E0560） | 18 か所の一覧（「設計」Phase 1）。新しい箇所は `into_data()`・`.clone()`・`error.span = ..` |
| `E1006` は引数 0 個の呼び出し（ビルダーの操作の arity の誤り）でも出る。`count - parameters.len()` を無条件に書くと debug で `attempt to subtract with overflow`（試作で踏んだ。`tests/computations.rs` の 2 件が落ちる） | `count > parameters.len()` のときだけ help（付録 F-6）。`cargo test --test computations` |
| `render_with` のテキストで、別のソースの行の列を主のソースの本文から数える | `with_source` のコールバックの中で、**そのソースの本文**で `location` を呼ぶ（付録 F-2）。`related_in_another_source_uses_the_lookup` |
| `recovering` のとき `E1003` は `Err` で返らず `self.recovered` に積まれる。`expected_because` が `Err` だけを見ると効かない | 付録 F-6。`result` と `recovered[from..]` の両方を見る。テストは回復する経路（`analyze_all`）で書く |
| `E1003` の `expected`・`found` をメッセージの文字列から切り出さない（`expected an integer, found f64` のような型でない期待の診断に付けてはいけない） | `mismatch` を通った診断だけが `detail.expected` を持つ。`expected_because` は `detail.expected.is_some()` を条件にする |
| `E1014` の衝突を、報告済みの根でも作ってから捨てると、同じ根への衝突が多い関数で `holders`・`later_use` の走査が何度も走る | `access` で `self.reported.contains(&place.root)` なら作らずに戻る（付録 F-3）。`later_use` は関数ごとの `budget` を持つ（付録 F-4） |
| `E1021` の fix のテキストは字下げを持たない（`Checker` にソースの本文がない）。複数行の `match` の末尾に ` \| X -> ..` が続く形になり、`tsuzuri fmt` が整える。`arms5`（最後の腕が行内の入れ子の `match`）では入れ子の側に付く | D11。適用の前に LSP が `fix_edit` で検証する。`nested_inline_match_fix_is_not_verified_here` |
| LSP の `relatedInformation` に std の仮想パス（`std/Array.tz`）を出すと、エディターが開けない | 利用者のファイルだけ（`ModuleOrigin::User`）。`publishes_related_information_and_data` |

## 対象外

- `kind`・`explain`・ヒント DB（G23）、`tsuzuri fix` の適用エンジン・`W1001` の `Fix` 化・edition の移行（G24）、typed hole・候補・未知の型名（`E1004`）の候補（G25）、facts と KB（G26〜）。
- 475 か所全部への `related`・`help`（付録 D の表にある族だけ。残りは G23 が `kind` と一緒に扱う）。`E1015`・`E1005` の大半・`E1016`・`E1017`・`E1020`・`E2000` 系。
- `if`／`match` の枝の型の不一致（`then` の型を期待にしたもの）・二項演算子の左右・配列の要素の `ExpectedBecause`（期待の型が宣言ではなく兄弟の式から来る。D9）。
- `tsuzuri repl` の `related`・`fixes`（D8）。VS Code 拡張のコードの変更（`vsc/README.md` の記述の更新は Phase 4）。診断の多言語化。生成コード・IR・ABI・WASM。

## 決定事項

**D1: `Diagnostic` を `Box` にする（承認済み。D-51 の置き場所の変更）**。D-51 のフィールドを直接置くと 264 bytes になり、GUIDE §3.1 の stack の回帰テストがすべて溢れる（実測。「現状」の表）。`pub struct Diagnostic(Box<DiagnosticData>)` は 8 bytes で、必要 stack が 10〜26% 減る。フィールドの名前と型は D-51 のまま `DiagnosticData` にあり、`Deref`・`DerefMut` で読み書きできる。**書き換えが要るのは 18 か所だけ**。却下: ①D-51 のまま（溢れる）。②`Option<Box<Extra>>` を足す（88 bytes。余裕が 10 KB の lib テストに一時値ごと 8 bytes 足す）。③`Result<_, Box<Diagnostic>>` に変える（全関数の署名の変更。数千か所）。④stack を増やす・上限を下げる（GUIDE §11.1 が禁止）。
**D2: `role` の文字列は variant 名の snake_case**（`moved_here`・`borrow_used_here` の形）。D-51 は variant 名だけを決めていた。キー・値とも安定として扱う。
**D3: 上限は builder で適用し、診断を作った側は意識しない**。`related` 16、`fixes` 8、`candidates` 10、`scope` 32 は D-51 のとおり。**1 つの fix の `edits` が 0 件または 65 件以上なら fix を丸ごと捨てる**（D-51 は「超えた分を切り捨てる」。edit を途中で切ると、半分だけ適用される fix になり意味が壊れる）。
**D4: 解決できない `related`・`fixes` は出さない**。`lookup` を渡さない旧来の入口（`json`・`render`）でも主のソースの分は出す。edit が 1 つでも解決できない fix は丸ごと出さない（部分適用を防ぐ）。
**D5: `related` の span は空でなく、主スパンと違うものだけ**（`MovedHere` のループ越しだけ同じ span を許す。理由は「前の反復」と書ける）。生成されたコード（`$` で始まる名前・空の span）には付けない。
**D6: 所有権の `related` は 7 か所に付ける**（付録 D-1）。残り 35 か所はメッセージが修正方法を含み、`related` の対象が定まらない（`task`・`Channel`・`Mutex`・region の制約）か、起点がない（`Drop`・要素・参照からの移動）。却下: 全 42 か所に `Note`（情報がない）。
**D7: 起点は 1 つだけ**（`(start, end)` が最大）。却下: すべての起点（最大 16 件）は、分岐の多いコードで診断が長くなり、AI が読む量が増える。最初の起点は、分岐の合流で最も遠いものを指す。
**D8: `BorrowUsedHere` は誤りの経路でだけ探す**（`holders`・`later_use`）。関数ごとに合計 `LATER_USE_NODES` 個の式まで、同じ根に報告済みなら探さない。却下: `State` に各変数の最後の使用を持つ案は、成功する全関数にコストを足す（AGENTS.md の性能方針）。診断ごとの上限（診断の多い関数で線形に増える）。**`tsuzuri repl` は `related`・`fixes` を出さない**（生成した別のプログラムの位置を持つ。入力の行へ写す処理が要る）。
**D9: `ExpectedBecause` は 3 つの文脈（注釈・戻り値・名前つきの関数の引数）の、期待の型を渡した式の尾の位置だけ**。期待は `expression(expr, Some(&ty))` で下へ渡されるだけで原因を持たないので、文脈が検査の前後に `recovered` を見て付ける。却下: 全 `same` に原因を渡す（約 41 か所と `expression` の全呼び出しの改修）、期待に原因を持たせる（`Option<&Type>` を構造体にする全面改修）。
**D10: 種との食い違い**: `E1004`→`E1002`（値）・`E1007`（フィールド）、`E1005`→`E1006`（引数の数）・`E1003`（型）、`E1015` と `test_runner.rs` は対象外（「現状」の表）。
**D11: `E1021` の fix は `maybe` で 1 つ、` | {witness} -> unreachable ()` を `match` 式の末尾に挿入**。字下げや改行を持たない（`Checker` にソースの本文がなく、列を知れない。行内の腕は文法で許される）。却下: 改行と字下げを推測する（誤った字下げは構文エラー）、`todo` のような新しい名前（D-50 は予約語・新しい組み込みを足さない）。トラップする仮の本体なので `maybe`（自動では適用しない）。
**D12: LSP の quickfix は `fix_edit` の検証を通ったものだけ**。`isPreferred` は `certain` のときだけ `true`。件数は誤りだけ数え、診断が警告のときは警告も数える。却下: 検証せずに出す（`arms5` の形で、適用しても直らない fix を出す）。誤りだけを数える（G24・A22 の警告の fix が出ない）。全診断を数える（誤りの fix が、あとの検査の新しい警告で落ちる）。
**D13: `help` は、メッセージが同じことを言っているときは付けない**（`Drop`・要素・参照からの移動の `E1012`）。
**D14: fix の `title` は先頭大文字の命令形の英語**（`Add the arm '..'`）。LSP の既存の `Prefix 'spare' with '_'`・G24 の D13・A22・G25 と同じ。メッセージと `help` は D-51 のとおり小文字始まり。

## 付録

長い表・実測・試作のコード・テストの全文は [G22-appendix.md](G22-appendix.md) にある。

| 付録 | 内容 |
| --- | --- |
| A | 再現ソースと HEAD の出力（`move1`・`loop1`・`loop2`・`branch1`・`branch2`・`partial1`・`borrow1`〜`borrow3`・`life1`・`life3`・`esc1`・`esc3`・`no1`〜`no6`・`type1`・`call1`・`name1`・`field1`・`nofn`・`std2`・`mismatch1`・`mismatch2`・`multi1`・`match1`・`fix2`〜`fix4`・`arms1`〜`arms6`・`dup2`・`dup3`・`dup5`〜`dup9`・`dup12`） |
| B | 試作の出力（各 Phase の期待。実装後の出力はこれと一致する） |
| C | 互換性の確認手順（変更の前後で追加の情報を持たない診断が 1 byte も変わらないこと） |
| D-1 | Phase 2: 所有権の診断 42 か所の表（付ける 7、付けない 35）と借用の作成位置 |
| D-2 | Phase 3: 族ごとの全発行箇所の表と「触らないもの」 |
| E | stack の測定の手順と値、試作で通ったテスト |
| F-1〜F-8 | 試作のコード（型・出力・所有権・型と名前・`E1021` と重複・LSP） |
| G-1〜G-5 | テストの全文（`tests/diagnostic_v2.rs` の段階ごと、`src/diagnostic.rs` の単体テスト） |

## 実装と検証（2026-10-26）

- 基盤は `main` の `62939ba5`（起票時の `a2313d19` から 462 コミット）。Phase 1〜4 の手順 1〜22 を順に実装し、Phase ごとにコミットした。名前・型・メッセージ・JSON のキーと順序・上限は本文と付録 F のとおり。
- **HEAD との食い違いと最新化**（付録 A・B・F-8・G-4 を更新済み）:
  - edition 2027 が manifest のない CLI 入力の既定になり、通常の `def` の非 Copy 引数は既定で読み取り（A18）になった。`move1`・`loop1`・`loop2`・`branch1`・`branch2`・`partial1`・`life3` と LSP の `MOVE_TWICE` は所有権を受け取る引数を `own` で書く（どの edition でも同じ意味）。`own` がないと CLI では移動が起きず、HEAD でも診断が出なかった。
  - `std2` は、`std/Array.tz` の `def length` が 9 行目にあり、メッセージが `expected [an undetermined type], found i32`（`ref` なし）。`std_parameter_note_points_into_std` は行を std のファイルから数える。CLI（2027）の `nofn` の 3 件目は、読み取り引数の型どおり `a value of type ref string ...`（`analyze_all` の 2026 では `string`。テストは 2026 で照合）。
  - D1 の書き換えは 18 か所ではなく 23 か所（`src/profile.rs` 1・`src/resident.rs` の単体テスト 2・`src/main.rs` の引数解析 5 を含む）。`print_diagnostics` は `write_diagnostics(err, ..)` を通す形に変わっていたので、`lookup` をそこで作る（直接実行とサーバー経由の `check` の両方が通る）。
  - `E1012` の可変性の条件は HEAD で `usage != Use::Write && ..` に変わっていた。`DeclaredHere` の条件（付録 F-3）はそのまま使えた。
- **stack**（GUIDE §3.1。`RUST_MIN_STACK` の 16 KiB 刻みの二分探索。debug、opt-level 1）: HEAD 約 325,632 / 357,376 / 595,456 → 完了時 約 325,632 / 357,376 / 563,712（lib / polymorphism / computations）。dev profile が opt-level 1 になったため、起票時の値（約 2 MB）より大幅に小さい。付録 F-6 の形のまま `expected_because` を `Result<T, _>` に通すと computations が 659 KB に増えた（`let` の注釈の経路で `TypedExpr` の一時値が frame に残る）ので、**`expected_because` は `Option<&mut Diagnostic>` だけを受け取り、`#[cold] #[inline(never)]` の `annotation_because`・`argument_because` から、検査が誤りを出したとき（`Err` か `recovered` が増えたとき）だけ呼ぶ**形にした。成功する検査は `tail_spans` の割り当ても `format!` もしない。`E1002` の構築も `#[cold] #[inline(never)]` の `unknown_value` に出した。付ける条件と結果は付録 F-6 と同じ。
- **互換**（付録 C）: Phase 1 の完了時、付録 A の 43 本と `tests/fixtures/*`・`examples/*` の 163 プロジェクト・489 ファイル（JSON・テキスト・終了コード）が変更前と 1 byte も同じ。Phase 2・3 の完了時、追加の情報を取り除くと 489 ファイルすべて同じで、増えたのは付録 A のソース（70 ファイル）だけ（`tests/fixtures`・`examples` は 1 byte も変わらない）。`TSUZURI_THREADS=1` と `8` の出力は同じ。付録 B の全出力を照合し、`life3`（`own` の 4 列）・`std2`・`nofn`（上記）以外は一字一句同じ。
- **テスト**: `tests/diagnostic_v2.rs` 28 件（レビューの回帰テスト 11 件を足して 39 件。手順 2・8・14 で先に書き、手順の予告どおりの件数が失敗してから実装した）、`src/diagnostic.rs` の単体テスト 8 件、`ownership::later_use_tests` 1 件、`lsp::tests::a_fix_must_remove_its_diagnostic_and_add_nothing` 1 件、`tests/lsp.rs` 3 件。既存のテストの期待値は 1 つも変えていない（D1 の書き換えの 8 ファイルの `rejects` の `into_data()` を除く）。`codeAction` の検証つき quickfix は release の 1 セッション全体で 0.53 秒（停止条件 9 の 1 秒未満）。
- **実行したコマンド**: `cargo fmt --all -- --check`、`cargo clippy --all-targets --locked -- -D warnings`、`RUST_MIN_STACK=4194304 cargo nextest run --locked`（各 Phase の最後。最終は「最終確認」の行）、`node tests/parallel_frontend.mjs`・`node tests/examples.mjs`（release）、`node tests/{lsp_sessions,repl,frontend_cache,server,watch,control}.mjs`・`node tests/features.mjs .. borrowed_records`（dev）、`node scripts/check-docs.mjs`（変更した 4 ページ: 57 links、23 checked examples、46 native runs）。
- **最終確認**（手順 22）: `cargo fmt --all -- --check`・`cargo clippy --all-targets --locked -- -D warnings` が緑。`RUST_MIN_STACK=4194304 TSUZURI_THREADS=8 cargo nextest run --locked` が **1,926 件すべて通過**（2 件は既存の skip。HEAD の 1,873 件 + 本チケットの 53 件。レビューの修正後。stack の必要量は変わらない）。GUIDE §3.1 の 3 つは既定の stack で通過。`node tests/ai_cost.mjs`（G31。`diag-v2` の測定は、計測用のコーパスの最初の診断が `related` を持たないので、今の基準値のまま無効）、`node tests/repo_kb.mjs`（78 件）、`node scripts/check-guide-lite.mjs`（85 件）が通る。
- **レビューでの修正**（PR #53 の Copilot のレビュー）: (1) 括弧で囲んだ `match`（`(match b with | true -> 1)`）は式の span が括弧を含むので、`E1021` の修正案を `MatchCoverage.span.end` に挿入すると括弧の外に付いて構文が壊れた。`MatchCoverage.explicit: bool` を `append_at: Option<usize>`（明示の `match` の最後の腕の本体の終わり）に替え、そこへ挿入する（括弧のない形では従来と同じ位置）。(2) 予約名 `_` のフィールドの重複（`record R { _: i64, _: i64 }`）で、後ろの `_` を「first defined here」と示していた。前のフィールドだけを探し、予約名（`_`、予約された record 名）には付けない。(3) 呼び出しの前の引数が借用を保持したまま後の引数を評価する衝突（`both r (ref mut n)`）で、前の使用を「in the next iteration of the loop」と示していた。前の使用を示すのは、ループの本体（`LoopFlow.span`）が前の使用と衝突の位置の両方を含み、保持中の引数（`held`）が借用を生かしていないときだけにした（`Checker::repeated_use`）。回帰テスト `parenthesized_match_fix_goes_inside_the_parentheses`・`an_earlier_use_is_named_only_when_a_loop_repeats_it`・`reserved_fields_have_no_first_definition` を足した。2 回目のレビューで、(4) 生成された `test`・`bench` の関数（`$test.0`）の結果の型と束縛（`$case`）の注釈は書かれていないので `ExpectedBecause` を付けない（`Ident.provenance` と `$` で判定）、(5) `scope_entries` は 32 件に達したら型の表示を作らずに止める、(6) 借用を持つ変数全体への代入（`r = ref m`）は古い借用を終わらせるので、`find_later_use` は代入の場所を使用と数えず、その代入より後の使用を後の使用とせず、本体が持つ変数に代入するときは前の使用も示さない、を直した。回帰テスト `generated_declarations_have_no_because`・`a_reassigned_holder_is_not_a_later_use` を足した。3 回目のレビューで、(7) 後の使用と代入の打ち切りを借用を持つ変数ごとに数え、残った使用のうち最も前のものを示し、`help` もその変数を名指す（`r` を置き換えても `keep` の後の使用は残る）、(8) `ExpectedBecause` は、不一致の `expected` が宣言の型（参照の引数なら参照先の型も）と同じ表示のときだけ付ける（`let x: i64 = !1i64` の `expected bool` は `!` が決めたもので、注釈ではない）、(9) `E1021` の修正案は、足りない腕を witness から行に戻して探索し直し、8 個までの書ける腕で網羅できるときに全部を 1 つの edit で足す（タイトルは `Add the arms '..'`）。それ以外は修正案を付けない、(10) `E1014` の `BorrowedHere` を外すのは引数そのものの借用（親のない外部の loan）だけにし、引数の再借用（`ref (deref p)`）の位置は示す、を直した。回帰テスト `each_holder_keeps_its_own_later_use`・`a_reborrow_of_a_parameter_names_where_it_was_taken`・`an_operator_in_the_tail_is_not_explained_by_the_declaration`・`a_fix_adds_every_missing_case_or_none` を足した。4 回目のレビューで、(11) 演算子（単項・二項・範囲・cast・補間・代入）は自分の被演算子の型で検査して自分の span に報告するので、`tail_spans` に入れない（`let x: bool = 1i64 && 2i64` は表示が同じでも注記なし。`let x: string = 1 + 2` の注記も失うが保守的な側）、(12) `find_later_use` を制御の流れに沿わせた: 各使用と代入に、包む `if`・`match` の腕とループ・`&&`／`||` の右辺・`try` の handler の選択（`Choice` の列）を持たせ、衝突の位置と別の腕の使用を外し、代入はその使用へのすべての経路で先に走るとき（代入の選択が使用の選択に含まれるとき）だけ打ち切りにし、前の使用はループの本体が両方を含むものから選ぶ（保持中の引数が借用を生かしているときは選ばない）。衝突の位置の式が本体に見つからなければ注記を付けない、(13) LSP の修正案の検証は、表示の上限（50 件）で切った診断ではなく、`DiagnosticSet.counts`（収集した全診断の重要度・コードごとの件数）で数える。収集の上限に達したときは検証できないので出さない、を直した。回帰テスト `an_operand_type_is_not_explained_by_the_declaration`・`borrow_uses_follow_the_control_flow`・`code_action_verifies_a_fix_past_the_display_limit`（`tests/lsp.rs`）を足し、`later_use_tests` と `fix_removes` の単体テストを広げた（`tests/diagnostic_v2.rs` は 39 件、`tests/lsp.rs` の新規は 4 件）。5 回目のレビューで、(14) `match` の guard を腕の本体と別の選択にした（腕 i の本体はほかの腕の本体と、後の腕の guard と排他。guard どうしは排他でなく、guard は自分の腕の本体より先に走る）。偽になる guard の中の衝突のあとも、後の腕の使用を示す、(15) 候補の選択の比較（排他の判定・代入の打ち切り）も同じ `budget` から引き、代入を持つ変数ごとに分け、候補はソースの順に調べて最初に条件を満たすもので止める（予算が尽きたら注記を付けない）、を直した。6 回目のレビューで、(16) 排他の判定は、条件に合わない選択も 1 歩ずつ予算から引く、(17) 前の使用を外す代入は、その使用を繰り返すループの部分の中のものだけにする（ループの前の `r = ref n` では外さない）、(18) `while` の条件も次の反復で走るので、`LoopFlow.span` を条件から本体の終わりまでにした、を直した。
- **チケットから外れた判断**: 上記の stack のための構造（結果は同じ）。`E1002` の説明が `compiler/diagnostics.md` で「型シグネチャの不整合」になっていたのを、HEAD の実際（未知の値・関数・定数・ケースの名前）に直した。所有権・非同期・`Owned` のページの出力例は、最初の 1 行だけを引用する形（`error[E1012]: ...`）で、その形のままでも正しいので変えず、`ownership.md`・`borrowing.md`・`lifetimes.md` の最初の例に完全な出力（`note`・`help`）と各 role の説明を足した。
- **更新した文書**: `_tsuzuri/language-reference/compiler/diagnostics.md`（表示形式に「関連する位置・help・修正案」、JSON の追加キーと `role`、エラーコードの表、LSP）、`ownership-and-memory/{ownership,borrowing,lifetimes}.md`、`docs/language.md`（診断）、`docs/stability.md`（安定として保つもの・保たないもの）、`docs/architecture.md`（`src/diagnostic.rs`・`src/lsp.rs` の行）、`README.md`（開発・AI 支援、LSP の主要機能）、`vsc/README.md`（機能と制限。拡張のコードは変えない）。
- **変更したファイル**: `src/{diagnostic,main,repl,docgen,cache,profile,resident,driver,polymorph,ownership,ownership_control,check,control,exceptions,exhaustiveness,parser,lsp}.rs`、`tests/diagnostic_v2.rs`（新規）、`tests/lsp.rs`、`tests/{arena,concurrency,json,matrix,matrix_views,rc,specs,tensor}.rs`、`tests/repo_kb.mjs`（このチケットのパス）、上記の文書、`_features/`（本チケットと付録の移動と更新、`README.md` の状態、他チケットからのリンク）。

