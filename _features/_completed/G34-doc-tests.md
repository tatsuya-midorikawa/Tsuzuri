# G34: doc-test（`///` の例を実行する）と std の例

| 項目 | 内容 |
| --- | --- |
| ID | G34 |
| 優先度 | P2 |
| 規模 | M（Phase 1: M、Phase 2: S、Phase 3: L だが機械的で、手順 3.0〜3.24 は 1 手順ずつ独立してマージできる） |
| 依存 | なし（Phase 2 は Phase 1、Phase 3 は Phase 1・2 に依存）。関連: G23（診断の kind。後述の「前提とする他チケットのインターフェース」）、G35（doc の構造化タグ。**本チケットは `@complexity`・`@since`・`@see` を導入しない**） |
| 後続 | G35 Phase 2（std の doc コメントの規則を共有）、G21（ガイドが std の例を読む）、G26・G27（KB の `doc` 欄）、D14（`Text` の例） |
| 状態 | done（全 Phase） |
| 起票 | 2026-10-18（D-50。HEAD `a2313d19` で確認。作業ツリーの HEAD は `9dd0fbbc`＝`a2313d19` の親で、差は `docs/benchmarks.md` の 41 行だけ） |
| 承認 | 承認済み（D-50: 人間の包括許可 2026-10-18）。決定事項 D1〜D15 はすべて承認済み |
| 改善する劣位 | Rust 比（`cargo test --doc`）・Python 比（`doctest`）: `///` の例が実行されず、書いた例が古くなっても気づけない（[docs/language.md](../../docs/language.md) に「doc-test の実行は行いません」と明記）。std の公開関数 559 個のうち説明があるのは 305 個で、例は **0 個**（`grep -c '```' std/*.tz std/*.tc` が全ファイルで 0）。引数の順序は署名から読めず（`String.split "," "a,b,c"` は区切りが先で、`String.contains "hello" "ell"` は型が通って `false` を返す）、AI も人も誤る（D-50 の調査結論 (4)） |
| 手本にする既存実装 | 例の枠: `src/driver.rs` の `Project::load_with_overlays`・`Project::load_from_root`（`overlays`・`all_std`）・`Project::single_main`・`Project::load_for_docs`（`std` ディレクトリの判定）。実行と出力の取得: `src/driver.rs` の `run_captured`・`run_process`（`RunStdio::Capture`）・`capture`・`relay`、`src/repl.rs` の `evaluate`。テストの実行と報告: `src/test_runner.rs` の `TestOptions::includes`・`run_tests_linked`、`src/main.rs` の `run_test_action`（text／JSON／`E2006`）・`print_output`・`output_field`。並列: `src/parallel.rs` の `Parallelism::command_line`・`ordered_for_each`。doc の描画: `src/docgen.rs` の `section`・`render_declarations`。引数: `src/main.rs` の `parse_arguments`（`--seed` の分岐）・`rejects_ambiguous_or_unused_arguments`。記法: `scripts/check-docs.mjs`（フェンスの正規表現と `run=` の `decodeURIComponent`）。テスト: `tests/docs.rs`・`tests/docgen.mjs`・`tests/bench.rs`（CLI の統合テストの型）・`tests/lsp.rs` の `semantic_docs_follow_function_and_type_definition_targets` |
| 主な影響ファイル | **Phase 1** 新規: `src/doctest.rs`, `tests/doctest.rs`, `tests/fixtures/doctest/{pass,fail,notation,main}/`。変更: `src/lib.rs`（`pub mod doctest;`）, `src/driver.rs`（`Project::for_doc_example`・`run_captured_quiet`・`RunStdio::Capture` の `echo`・`capture`）, `src/main.rs`（`--doc`・`Arguments.test_doc`・`run_doctest_action`・`--help`）, `docs/language.md`, `README.md`, `docs/architecture.md`, `docs/stability.md`, `_tsuzuri/language-reference/` の 5 ページ（「ドキュメント」節）, `.github/workflows/targets.yml`, `_features/GUIDE.md`（D-55 に `E2011`）。**Phase 2** 変更: `src/doctest.rs`（`render_doc`）, `src/docgen.rs`（`section`）, `src/semantic.rs`（`SemanticIndex::document`）, `tests/docs.rs`, `tests/lsp.rs`, 文書。**Phase 3** 新規: `tests/std_examples.rs`。変更: `std/*.tz`・`std/*.tc` のうち `def` を持つ 42 ファイル（`///` の追加だけ）, `.github/workflows/targets.yml`, 文書 |

## 方針の最新化（2026-10-30。実装前の再検討）

起票時（HEAD `a2313d19`）の前提と、G23・G25・G28・A18・A22・A23・F14・C12・A21 の実装後の現状が食い違う点を、実装前と実装中に次のとおり改めた（人間の包括許可による判断。D1〜D15 の趣旨は保つ）。

| 起票時の前提 | 現状 | 改めた方針 |
| --- | --- | --- |
| 診断は `Diagnostic::new("E2011", …)` | G23 の登録簿があり、診断は `Diagnostic::of(DiagnosticKind::…)` で作る | kind `doc_example_invalid`（`E2011`）・`doc_test_failed`（`E2006`）・`doc_example_limit`（`E1017`）を登録簿に足して `of` で出す。`explain` の `E2011` のページを足し、`E2006` のページに doc の例を足す |
| `--doc` と併用できない旗は「CLI」の表の 6 種 | G28 の `--affected`、A23 の `--allow-overflow` が `test` に増えた | どちらも `--doc cannot be combined with …`（`E2000`）にする。`--doc` は知識グラフを書かない（例のプロジェクトは一時的なもの） |
| std の公開 `def` は 559、目標は 507（付録 C） | F14 の `Alloc`、C12 の `chunk_count`・`invariants_hold`、A21 の `Gen.for_all_where`・`check_where` などで、大文字で始まらない公開関数は 819（免除 10 を除く）、モジュールは 48 | `FLOORS` は 48 モジュールすべてを、例を持たせた数（＝公開関数の数）で並べる。手順 3.1〜3.23 は 1 つの PR にまとめ（AGENTS.md の 1 チケット 1 PR）、手順 3.24 の `every_public_std_function_has_an_example` も同じ PR で足す。`BUILDER_FLOORS` は 4 モジュールとも 1 から始める |
| 例は書いた edition の構文で書く | A22 で edition 2027 が正準形（`..=`・`..<`、接尾辞 `i64`・`f32`）を要し、A18 で無印の引数は借用。`std` の例はマニフェストのない `Main.tz`（最新の edition）として走る | std の例は edition 2027 の正準形と D-59 の引数の順序で書く |
| fixture の `E2005` の文言は付録 B のとおり | 契約（A21）の追加で、トラップの文言の後半が変わった（`a contract`） | `tests/doctest.rs` は付録 B の方針どおり先頭の `E2005: ` だけを確かめる |
| 例を足すたびに `cargo build --release` して確かめる | 819 関数を 1 つずつ再ビルドして確かめるのは遅い | 作業中は、例を 1 つずつ独立した `Main.tz` にして既存のコンパイラで `check`・`run`（`-O0`・`-O3`）・`test` する補助スクリプト（`tsuzuri test --doc` と同じ判定）で確かめ、最後に dev ビルドの `tsuzuri test --doc std -O0`・`-O3` で全件を確かめた |
| ビルドキャッシュは並行する例のビルドで壊れない | 例を並列に走らせると、別のビルドが消したステージングの項目を `BuildCache::evict` が読みに行き、`build cache write failed` が出ることがあった | `evict` は一覧と読み取りの間に消えた項目（`NotFound`）を飛ばす（`src/cache.rs`。プロセス間の並行ビルドにも効く） |

## 実装者向け早見

- **最初に読む**: `src/driver.rs` の `Project::load_from_root`（`overlays` で `Main.tz` を差し替えられる）と `run_captured`、`src/test_runner.rs` の冒頭 280 行、`src/main.rs` の `run_test_action`、`src/parallel.rs` の `ordered_for_each`、`scripts/check-docs.mjs`（記法の元）。
- **最初に書くテスト**: `src/doctest.rs` の `decodes_percent_escapes_like_decode_uri_component`（手順 2。本体を `todo!()` にして失敗を確かめてから実装する）。
- **触ってはいけない**: パーサーと AST（新しい構文・トークン・予約語はない。`FRONTEND_FORMAT` を上げない）。`--doc` なしの `tsuzuri test` の出力。`tsuzuri doc` の既存の出力（オプションも隠し行もない例は **1 byte も変えない**。`tests/docs.rs::renders_public_apis_in_signature_order_and_keeps_markdown` と `tests/docgen.mjs` が守る）。生成 IR。std の関数本体（Phase 3 は `///` の行だけを足す）。
- **新しい名前**: `--doc`（`test` の旗）、診断 `E2011`（D-55 の予約から割り当てる。手順 8 で GUIDE の表に追記）、`src/doctest.rs`、`Project::for_doc_example`、`driver::run_captured_quiet`、`tests/std_examples.rs`。
- **Phase の完了の見分け方**:
  - Phase 1: `target/release/tsuzuri test --doc tests/fixtures/doctest/pass` が「仕様 > 出力」の想定出力と一致して終了コード 0、`.../fail` が 7 件失敗して終了コード 1、`.../notation` が `E2011` を 6 件出して何も実行せず終了コード 1。
  - Phase 2: `tsuzuri doc tests/fixtures/doctest/pass -o <dir>` の `Counter.md` に `# ` で始まる行がなく、`run=` の例の後ろに `Output:` の節がある。ホバーも同じ。
  - Phase 3: `cargo test --locked --test std_examples` が通り（免除を除く 507 関数と builder 4 モジュールに例がある）、`tsuzuri test --doc std` が `-O0` と `-O3` の両方で全件成功する。
- **規則**: 例の期待値は、関数の説明と手計算（または別の言語）から作る。コンパイラの出力を写さない。食い違ったら期待値を直さず止めて報告する（「停止条件」8）。

## 目的

`///` の doc コメントに書いた ```` ```tsuzuri ```` の例を、`tsuzuri test --doc` で検査する。例が `check` を通る・期待どおりの出力をする・`test` が通る・期待した診断で拒否される、のどれかを機械で確かめ、古くなった例を CI で見つける。`tsuzuri doc` の出力とエディターのホバーは、例を読みやすく見せる（setup 用の行を隠し、`run=` の出力を併記する）。そのうえで、std の公開関数のほぼすべてに、検証された例を付ける。

HEAD で測った数字（再現コマンドは「現状」）:

- std の公開 `def` は 559 個（47 ファイルのうち `def` を持つのは 42 個）。説明があるのは 305 個。**例は 0 個**。
- 例が無いと、型が通るのに意味が違う使い方が書ける。`String.contains "hello" "ell"` は `false`、`String.contains "ell" "hello"` は `true`（引数は部分文字列が先）。`Array.contains [1, 2, 3] 2` は `E1003`（`ref 2` が要る）。
- 言語リファレンスの例 584 個は `scripts/check-docs.mjs` が検査するが、ソースの `///` の例を検査する道具は無い。

利用者の価値: ライブラリの作者は、例を書けば CI で守られる。読者と AI は、std の各関数の使い方を、動くと確かめられた例から学べる。

**非目標**: 例の実行結果のキャッシュ・差分実行。WASM と native 以外の target での実行。`tsuzuri test`（`--doc` なし）が doc の例も走らせること。`@complexity`・`@since`・`@see`（G35）。言語リファレンス（`_tsuzuri/`）の例の検査の変更（`check-docs.mjs` は触らない）。VS Code の Testing ビューへの doc の例の表示。

## 着手条件と停止条件

### 着手条件

- 依存するチケットはない。Phase 2 は Phase 1 の手順 1〜8、Phase 3 は Phase 2 の完了が前提。
- GUIDE §2.3 の基準コマンドが成功する状態から始める（`cargo build --release --locked && cargo test --locked`。件数を控える）。
- 読む: GUIDE §6.8（CLI の旗）・§6.7（診断コード）・§7（テスト）・§8（文書）、D-45（edition）、D-51（診断）、D-55（`E2011` の割り当て先）。`docs/stability.md` の「安定として保つもの」。
- Phase 3 は、Phase 1・2 が `main` に入っていること、および「手順 3.0」の `tests/std_examples.rs` が緑であること。

### 停止条件

次のときは即興で回避せず、作業を止めて報告する（GUIDE §13 に加える）。

1. `Project::for_doc_example`（`load_from_root` の `all_std = false` と `Main.tz` の overlay）で組んだプロジェクトの `analyze_all` が、同じソースを実ディレクトリに置いた `tsuzuri check` の診断と違う。選択肢: A overlay をやめて `Project` を複製して `Main.tz` を差し替える（`SourceFile` に `Clone` を足す）、B 例ごとに一時ディレクトリへ書く。
2. `run_captured_quiet` を足したことで、`tests/repl.mjs`（REPL）の出力が 1 byte でも変わる。
3. 例を並列に実行すると、直列で通る例が落ちる、または結果が実行ごとに変わる。選択肢: 並列度を 1 に固定する（`workers = 1`）。理由を添えて報告する。
4. 既存のテストの期待値（`tests/docs.rs`・`tests/docgen.mjs`・`tests/lsp.rs`・`src/main.rs` の `mod tests`）を変えないと通らない。
5. 新しい crate・`unsafe`・AST／構文の変更・予約語が要るように見える（本チケットは doc コメントの本文を読むだけで、どれも要らない）。
6. `std` に `///` を足した後で、`cargo test --locked` のどれかが落ちる（std の行番号や全文に依存するテストがあった）。落ちたテストと理由を報告する。
7. `tsuzuri test --doc std` の結果が `-O0` と `-O3` で違う。
8. 例の `run=` の期待値と、コンパイラの出力が食い違う（「規則」）。**期待値を直して通さない**。関数・例・期待値・実際の出力・どちらが正しいと考えるかを報告する（std のバグか、説明の誤りの可能性がある）。
9. 例が `check` を通らないのは、関数の署名と説明が食い違うからに見える（署名が説明より狭い・引数の順序が説明と逆）。説明を直して通さず、報告する。
10. `--doc` の旗を足したことで、`tsuzuri test` の既存の旗の検査（`rejects_ambiguous_or_unused_arguments` の既存の行）が変わる。
11. 同じ失敗を直す試みが 2 回続けて失敗した。

## 現状（HEAD `a2313d19` で確認）

再現は、`target/` の下の作業ディレクトリ（例: `target/tz-G34`。`target/` は git の管理外）で行う。`T` は `target/release/tsuzuri` の絶対パス。以下の出力は 2026-10-18 に `a2313d19` の親 `9dd0fbbc` の release ビルドで取った実出力である（`docs/benchmarks.md` 以外に差はない）。出力は `cd` した作業ディレクトリからの相対パスで出る。

### doc コメントの扱い

- **字句**: `src/lexer.rs` の `Lexer::token` が `///` の行を `TokenKind::DocComment(text)` にする。`text` は `///` と、その直後の空白 1 文字（あれば）を除いた行の残りで、トークンの `span` は `///` から行末まで。**構文**: `src/parser.rs` の `Parser::take_doc` が、続く `DocComment` を `\n` でつないで `Documentation { text, span }`（`src/syntax.rs`）にし、`def`・`extern def`・`extern type`・`record`・`union`・`type`・`const`・`class`・クラスのメソッドの `doc` に入れる。空行と `//` のコメントは字句が捨てるので、間に挟まった `///` の行も 1 つの `Documentation` になる。
- **出力**: `src/docgen.rs` の `section` が `doc.text` をそのまま出す。`private` と `Provenance::Generated` の union は出さない。`src/semantic.rs` の `SemanticIndex::document` が `doc.text.clone()` を索引に入れ、LSP のホバーと補完が使う。
- **同じ出力の固定**: `tests/docs.rs::renders_public_apis_in_signature_order_and_keeps_markdown` と `tests/docgen.mjs` が、`/// ```tsuzuri` の例を含む doc の出力を 1 byte まで固定している。

```sh
mkdir -p fx/pass && cp <repo>/tests/fixtures/doctest/pass/Counter.tz fx/pass/   # 手順 7 で作る fixture（全文は付録 B）
$T doc fx/pass -o fx/out && sed -n 10,22p fx/out/Counter.md
```
```text
Adds one to `n`.

```tsuzuri run=3
Counter.add_one 2
```

A hidden line sets up a value that the reader does not see.

```tsuzuri run=11
# let base = 10
Counter.add_one base
```
```
`run=3` も `# ` の行も、そのまま出る。行頭の `#` は、ソースの文としては使えない（`# let x = 1` は `E0002: expected an expression`）ので、隠し行の印にして衝突しない。

### `test --doc` は無い

```sh
$T test --doc fx/pass; echo "exit=$?"
```
```text
<command line>:1:1: error[E2000]: unknown option '--doc'; use --help
exit=2
```

### `Main.tz` を例で置き換える枠は、今のコンパイラで動く

`fx/pass` は `Counter.tz` だけを持つ（`Main.tz` はない）。例の本文を `fx/pass/Main.tz` に書いて、既存のコマンドで走らせた結果（`run` の 2 件は `-O0` と `-O3` で同じ）:

| 例（`Counter.tz` の位置） | `Main.tz` の本文 | コマンド | 結果 |
| --- | --- | --- | --- |
| `add_one#1`（3 行目、`run=3`） | `Counter.add_one 2` | `$T run fx/pass` | `3` |
| `add_one#2`（9 行目、`run=11`、隠し行あり） | `let base = 10` と `Counter.add_one base` | `$T run fx/pass` | `11` |
| `add_one#3`（14 行目、`fails=E1003`） | `Counter.add_one "two"` | `$T check fx/pass` | `fx/pass/Main.tz:1:17: error[E1003]: expected i64, found string`、終了 1 |
| `double#1`（22 行目、`test`） | `test "doubles" = Test.equal (ref (Counter.double 4)) (ref 8)` | `$T test fx/pass` | `ok 1 - Main doubles` と `1 passed; 0 failed; 0 ignored` |
| `double#2`（26 行目、検査だけ） | `let _answer = Counter.double 21` | `$T check fx/pass` | 終了 0 |

失敗の側（`fx/fail/Counter.tz`。全文は付録 B）も、同じ枠で次のように見える。

| 例 | 結果 |
| --- | --- |
| `let wrong: bool = Counter.add_one 1` | `Main.tz:1:19: error[E1003]: expected bool, found i64` |
| `Array.sum ["a"]`（`fails=E1003` の例。実際は別のコード） | `{"severity":"error","code":"E1005","message":"no instance for Numeric<string>; define an instance or use a supported type",...,"span":{"start":0,"end":9,"line":1,"column":1,...}}` |
| `test "fails on purpose" = Test.equal (ref (Counter.add_one 1)) (ref 3)` | `not ok 1 - Main fails on purpose` と `  failure: trapped or terminated by signal`、終了 1 |
| `let _nothing = Counter.add_one 1`（`test` の例だが `test` 宣言がない） | `$T test fx/fail` は `0 passed; 0 failed; 0 ignored` で**終了 0**。doc の実行器が自分で検出する必要がある |
| `Array.sub 2 5 [1, 2, 3]` | `trap: assertion failed at fx/fail/Main.tz:1:1` と `error[E2005]: program terminated with signal: 5 (SIGTRAP); ...` |

`Project::load_with_overlays` と `Project::load_from_root` の `overlays`（`BTreeMap<PathBuf, String>`）が、実在しない `Main.tz` を足す・ある `Main.tz` を置き換える道を既に持つ。`Project::single_main(path, text)` は、`Main.tz` だけを持ち、`text` が名指す std のモジュールだけを足す `Project` を作る（REPL が使う）。`Project::load_for_docs` は、入力が `std` という名前のディレクトリで、中身が `stdlib::SOURCES` と同じなら、全ソースを `ModuleOrigin::Std` に付け替える（`tsuzuri doc std` が動く理由）。

### 実行と出力

- `driver::run_captured(module, project, options, timeout)` は、`tsuzuri run` と同じようにビルドして実行し、標準入力を空に、標準出力を集めて返す（上限 16 MiB、時間切れは `E2005`）。**標準エラーは `relay(.., echo = true)` で端末へ流す**ので、並列に何件も走らせると混ざり、成功した例の出力も出る。`tsuzuri test` は成功したテストの出力を出さない。
- `tsuzuri test` の text は `ok N - <module> <name>`／`not ok N - ...` と `  failure: ...`、最後に空行と `N passed; M failed; K ignored`。JSON は `{"type":"test",...}` と `{"type":"summary","passed":..,"failed":..,"ignored":..,"duration_ms":..}`。失敗があれば最初の失敗のスパンへ `E2006: N test(s) failed`（標準エラー）。`--list --json` は `serde_json::json!` で作るのでキーは辞書順（`index`・`module`・`name`・`path`・`range`・`type`）。
- 並列: `src/parallel.rs` の `ordered_for_each(count, workers, chunk, claim, produce, consume)` は、`produce(i)` を 8 MiB の stack の worker で評価し、`consume` に添字順で渡す。`Parallelism::command_line()` は `TSUZURI_THREADS`（1〜32）か、コア数（最大 8）。
- 速さ（M1 Max、warm）: `check` の 1 プログラムは約 0.03 秒、`run`（ビルドと実行）は約 0.2 秒。std の 500 件超を 8 並列で走らせると十数秒の見込み。

### std の公開関数と例（モジュール別の表は付録 C）

`$T doc std -o fx/stdapi` の 47 ページを数えた結果（`def` の種別のセクション。数える script は付録 D）:

| 項目 | 件数 |
| --- | --- |
| 公開 `def`（`export`・`const`・`rec` を含む） | 559（`def` を持つのは 42 ファイル。`Atomic`・`Channel`・`Exception`・`Mutex`・`Owned` は `def` を持たない） |
| 説明のある公開 `def` | 305 |
| 例（` ``` ` を含む doc）のある公開 `def` | 0 |
| ビルダー操作（`.tc` の大文字で始まる `def`: `Return`・`ReturnFrom`・`Bind`・`Zero`・`Delay`・`Combine`・`For`・`While`・`Run`・`Using`・`MergeSources`・`BindReturn`・`Bind2`。`Async` 7・`IO` 11・`Maybe` 12・`Result` 12） | 42 |
| 公開 `record` 43（説明 35）・`union` 20（説明 15）・`type` 1 | 例は対象外 |

組み込み関数（例: `Math.sin`・`Int.abs`・`Vec.push`。`std/*.tz` に宣言がないもの）は、`///` を付ける場所がないので対象外。言語リファレンスのページが例を持つ。

### 例を書くときに踏む落とし穴（実測。付録 A に検証済みの例 22 件）

- `Maybe`・`Result`・record・union の値は、`run` の末尾の式として**何も出力しない**（`Maybe.Some 3` は空）。`match` で取り出すか、`Maybe.default_value`・`Result.default_value` で値にするか、`test` の例にする。`()` も何も出さない。配列・組・数・文字列・`bool` は出る（`[1, 2, 3]`、`(1, "a")`）。
- 数値リテラルを `ref` の引数に渡すには `(ref 2)` と書く（`Array.contains [1, 2, 3] 2` は `E1003`、`Test.equal 6 6` も同じ）。整数リテラルの既定の型は `i32`。
- `Array.filter` の述語は `ref` を受ける（`\x -> deref x > 1`）。`Array.zip` は要素が `Copy` の型だけ。`Map.empty` は `Map.empty()`（空白なしの呼び出し）。型注釈つきの式 `(e: T)` は書けず、`let x: T = e` にする。

## 変えてはいけない意味

- `--doc` を付けない `tsuzuri test` の動作・出力・終了コード・既存の旗の検査は 1 byte も変えない。`tsuzuri doc` の出力は、**`tsuzuri` のフェンスが「オプションも隠し行も持たない」doc では 1 byte も変えない**（Phase 2。持つ doc だけが変わる）。
- 構文・字句・AST・型検査・所有権・生成 IR・WASM の import・ABI・診断の既存のコードと意味は変えない。`FRONTEND_FORMAT` を上げない。
- std の関数の本体・署名・公開範囲は変えない。Phase 3 の差分は `///` の行の追加だけ（既存の説明文を直すのは、誤りが見つかったときだけで、その差分を報告に書く）。
- `driver::run_captured` の動作（標準エラーを流す）は変えない。REPL の出力は 1 byte も変わらない。
- `tsuzuri api-diff` の分類は変えない（doc の変更は `non-breaking: documented def ...` のまま）。

## Phase 分割

| Phase | 内容 | 規模 | 前提 | 単独で出荷できる理由 |
| --- | --- | --- | --- | --- |
| 1 | 記法の解析、例の収集、実行器、`tsuzuri test --doc`（`--list`・`--filter`・`--index`・`--json`）、`E2011`、fixture と統合テスト、文書 | M | なし | `--doc` を付けない限り何も変わらない。`tsuzuri doc` と LSP は従来どおり（例の記法は doc の本文のまま出る） |
| 2 | `tsuzuri doc` と LSP のホバーで、隠し行を除き、`run=`・`fails=` の期待を併記する | S | Phase 1 の `scan`・`parse_options` | 隠し行・オプションを書いた doc だけが変わる。Phase 1 の動作に影響しない |
| 3 | std の公開関数 507 個とビルダー 4 モジュールに例を付ける。手順 3.0（床の検査と CI）、3.1〜3.23（モジュール単位）、3.24（厳格化と文書） | L | Phase 1・2 | 1 手順ごとに床を上げる ratchet で、途中で止めても緑のまま。std の挙動は変わらない |

## 仕様

### 1. 例の記法

doc コメントの本文（`Documentation.text`）に、次の形のフェンスがあれば例である。

| 項目 | 規則 |
| --- | --- |
| 開きの行 | 0 桁目から `` ` `` が 3 個以上並び、その後ろに `` ` `` を含まない行。後ろの文字列（前後の空白を除く）が**情報文字列**。`~~~` のフェンスは扱わない（普通の本文） |
| 言語 | 情報文字列の最初の空白区切りの語。`tsuzuri` のときだけ例。`text`・`sh`・言語なしを含む、`tsuzuri` 以外のフェンスは例ではなく、その中の行は例の探索から除く（`tsuzuri` に見える行があっても開きにしない） |
| 閉じの行 | 開きより後ろで、`` ` `` だけが（末尾の空白を除いて）開きと同じ個数以上並ぶ最初の行 |
| 閉じがない | `tsuzuri` の例なら `E2011`。`tsuzuri` 以外の言語のフェンスなら、文書の終わりまでをそのフェンスの中身として扱い、診断しない |
| インデント | 0 桁目でなく、空白かタブのあとに `` ```tsuzuri `` が続く行は例にならない。`test --doc` は `E2011`（`scripts/check-docs.mjs` の「インデントした tsuzuri のフェンスは検査されない」と同じ規則） |
| 本文 | 開きと閉じの間の行。行が 0 個、または空白だけなら `E2011` |

情報文字列の `tsuzuri` の後ろは、空白かタブで区切った**オプション**。

| オプション | 意味 | 値 |
| --- | --- | --- |
| （なし） | `check`: 型検査が通る。実行しない | — |
| `run=OUT` | `check` に加えて native で実行し、標準出力が OUT と同じ | `OUT`: 下の符号化。空でもよい（`run=` は「何も出力しない」） |
| `test` | 本文の `test "名前" = ...` がすべて通る（1 件以上） | — |
| `fails=CODE` | 型検査が拒否され、最初の error の `code` が CODE | `E` と数字 4 桁（例: `fails=E1012`）。`W` は不可 |
| `ignore` | 検査しない。`--list` には出る | — |

`run=`・`test`・`fails=`・`ignore` は**高々 1 つ**。同じ語の重複、未知の語、`fails=` の値の誤り、2 つ以上の併用は `E2011`。

**`run=` の符号化**: percent-encoding。`%` と 16 進 2 桁を 1 byte にし、それ以外の文字はそのまま、全体を UTF-8 として読む（JavaScript の `decodeURIComponent` と同じ結果。`+` は `+` のまま）。書くとき必ず符号化する文字は `%`→`%25`、空白→`%20`、タブ→`%09`、LF→`%0A`、CR→`%0D`、`` ` ``→`%60`。この 6 種以外は書いたまま（`[1,%202,%203]`、`"a"`）。`%` の後ろに 16 進 2 桁がない、または復号した byte 列が UTF-8 でないものは `E2011`。

**出力の比較**: 実際の標準出力と OUT の両方で、`\r\n` を `\n` にし、末尾の空白（改行・空白・タブ）を除いてから、完全一致で比べる。先頭の空白は意味を持つ。

**隠し行**: 本文の行が `#` だけ、または `# ` で始まるものは隠し行。コンパイルするときは行頭の `#` と、その直後の空白 1 文字（あれば）を取る（`# let base = 10` → `let base = 10`、`#     x` → `    x`、`#` → 空行）。`#x`・`#!` で始まる行は隠し行ではなく、そのまま本文に入る（`E0002` になる）。`tsuzuri doc` とホバーは隠し行を出さない（Phase 2）。

**本文からプログラムへ**: 行を順にそのまま `\n` でつなぐ（隠し行は印を取ったもの）。包みも補完もしない。結果の文字列全体が `Main.tz` の中身になる（トップレベルの式が入口で、`let` も `def` も `test` も書ける）。

### 2. 例を集める範囲と順序

- 対象のソース: `Project::load_for_docs(PATH)` が返す `Project` で、`std` の入力（ユーザーのソースが 0 個）なら `origin == Std` の全ソース、それ以外は `origin == User` かつ `Project::dependency_package(source)` が `None` のソース（根のパッケージ）。依存パッケージの例は検査しない。
- 対象の宣言: `tsuzuri doc` が出す宣言と同じ。`private` でない `def`・`extern def`・`extern type`・`record`・`union`（`Provenance::Generated` を除く）・`type`・`const`、すべての `class`、説明のあるクラスのメソッド。`private` や `instance` の doc の例は集めない（**文書に出る例だけを検査する**）。
- 順序と番号: `Project.sources` の順 → ソース内の doc の開始位置の順 → doc 内の例の順。通しの 0 始まりの番号を `index` とする（`ignore` の例も数える）。1 つの doc 内の例は 1 始まりの `number`。
- 名前: `module` = `source.name` の `.` を `::` に直したもの（`Geometry::Point`）、`owner` = 宣言の名前（メソッドは `Class.method`）、例の名前 = `{owner}#{number}`、**ラベル** = `{module}.{owner}#{number}`（`--filter` の照合文字列）。
- 位置: `line` = 開きの行の、ファイルでの 1 始まりの行。`span` = 開きの行の `DocComment` トークンの `span`（`.in_source(source index)`）。
- 上限: 例の総数 10,000（`MAX_DOC_EXAMPLES`）。超えたら `E1017`（`the package has more than 10000 doc examples; split the package`）。

### 3. 実行の枠と判定

各例は独立して、次のプロジェクトで走る。1 件の失敗で打ち切らない。

- パッケージの入力: `Project::for_doc_example(PATH, program)`。PATH のパッケージの全ソース・マニフェスト・依存を読み、`Main.tz` を例で置き換える（無ければ足す）。std は例が名指すモジュールだけを足す。
- `std` の入力: `Project::single_main(PathBuf::from("<doctest>").join("Main.tz"), program)`。std は組み込みのものを使う（`std/*.tz` の例を足したら、先に `cargo build --release --locked`）。
- `Main.tz` を持つパッケージで、`Main.tz` の doc の例は実行できない（置き換えると、例が自分の宣言を見られない）。`ignore` と同じく `ignored` に数え、1 回だけ通知を出す（「出力」）。

| 種別 | 手順 | 成功 | 失敗の `failure` |
| --- | --- | --- | --- |
| `check` | `analyze_all` | 成功（warning は無視） | `{code}: {message}{at}`（最初の error） |
| `run=OUT` | `analyze_all` → `run_captured_quiet`（30 秒） | 標準出力が OUT | 解析の error は check と同じ。実行の失敗は `{code}: {message}{at}`（例: `E2005`）。不一致は `expected output {E}, found {A}` |
| `test` | `analyze_all` → `Main` のテストを `run_tests_linked` | 1 件以上あり全件通過 | 0 件: `the example declares no test`。失敗: `test {name:?} failed: {failure}`（標準エラーの末尾は `output`） |
| `fails=CODE` | `analyze_all` | error があり、最初の error の `code` が CODE | 受理: `expected {CODE}, but the example was accepted`。別のコード: `expected {CODE}, found {code}: {message}{at}` |
| `ignore` | なし | — | — |

`{at}` は ` (example line {L}, column {C})`。診断のスパンの `source.unwrap_or(root)` が例の `Main` で、スパンが空でないときだけ付ける（`L`・`C` は `diagnostic::location` で、プログラムの文字列に対する 1 始まりの行・桁。隠し行の印を取った後の行）。`{E}`・`{A}` は正規化した文字列を `{:?}` で出したもの（200 文字を超えたら切って `...` を足す）。`test` の例が走らせるのは、`CheckedTest.span.source.unwrap_or(root)` が例の `Main` であるテストだけ（パッケージ自身のテストは走らせない）。

### 4. CLI

```text
tsuzuri test --doc PATH [--list] [--filter TEXT] [--index N]... [--json] [-O0|-O1|-O2|-O3] [--deny-warnings] [--warn implicit-copy]
```

`PATH` の読み方・`--filter`（ラベルの部分一致）・`--index`（通しの番号）・`--list`・`--json` は `tsuzuri test` と同じ。既定は `-O0`。`--doc` は `E2000` で次を拒否する（`src/main.rs` の `parse_arguments`。メッセージは英語）。

| 条件 | メッセージ |
| --- | --- |
| `--doc` を `test` 以外に付ける | `--doc is only valid with test` |
| `--doc` を 2 回付ける | `doc specified more than once` |
| `--doc` と `--coverage`・`--seed` | `--doc cannot be combined with --coverage`／`--doc cannot be combined with --seed` |
| `--doc` と `-g`・`-o` | `--doc cannot be combined with -g or -o` |
| `--doc` と `--target` | `--doc does not use --target; examples run on this machine` |
| `--doc` と `--link`・`-l`・`-L` | `--doc does not use --link, -l, or -L; list native inputs in Tsuzuri.toml` |
| `--index` が通しの番号の範囲外 | `doc example index is out of range; refresh the list`（解析のあと。`E2000`、終了コード 1） |

パッケージの `[native]`（`Project.native`）は、例のリンクに使う。終了コード: 全件成功か 0 件が 0、失敗・`E2011`・型検査の失敗・読み込みの失敗が 1、引数の誤りが 2。

### 5. 出力

通知とログは標準エラー、結果は標準出力（`tsuzuri test` と同じ）。`--json` のキーの順序と、省略するキーを下に固定する。`duration_ms` 以外は実行ごとに同じ。`ignore`・`Main.tz`・`--filter`／`--index` で外れた例は、`ignored` に数えるだけで 1 件ずつは出さない。

| 出力 | 形式 |
| --- | --- |
| text の結果 | `ok {index+1} - {module} {owner}#{number}`／`not ok {index+1} - ...`。失敗の下に `  failure: {failure}`、`test` の `output` の各行を 2 空白で字下げ、`  at: {path}:{line}` |
| text の要約 | 空行のあと `{passed} passed; {failed} failed; {ignored} ignored` |
| JSON の結果 | `{"type":"doctest","index":I,"module":M,"name":"{owner}#{number}","status":"passed"\|"failed",["failure":F,["output":O,]]"path":P,"line":L,"duration_ms":D}` |
| JSON の要約 | `{"type":"summary","passed":P,"failed":F,"ignored":I,"duration_ms":D}` |
| `--list` の text | `{index} {module}.{owner}#{number} [{kind}]`（`kind` は `check`・`run`・`test`・`fails`・`ignore`） |
| `--list --json` | `serde_json::json!` の 1 行（キーは辞書順）: `index`・`kind`・`line`・`module`・`name`・`path`・`type`（`"doctest"`） |
| 失敗の診断 | 最初に失敗した例の `span` へ `E2006`: `1 doc test failed`／`N doc tests failed`（標準エラー） |
| `Main.tz` の通知 | `N doc example{s} in Main.tz not run; move the documented declaration to another module`（text は標準エラーに 1 行。`--json` は `{"severity":"warning","code":"W2001","message":...}`） |
| ビルドの通知 | `run_captured_quiet` が返す `messages` を重複なく辞書順で、`tsuzuri test` と同じ形式で出す |

`path` は `Project.sources[i].path` を `to_string_lossy` したもの。例のプログラムが標準エラーに書いた内容は、失敗した `test` の例の `output` 以外は表示しない。

### 6. 診断

| コード | 条件 | メッセージ（英語。`{}` は値） | 主スパン |
| --- | --- | --- | --- |
| `E2011` | 未知のオプション | `unknown option '{word}' in a tsuzuri code fence; use run=OUTPUT, test, ignore, or fails=CODE` | 開きの行 |
| `E2011` | 重複・併用 | `option '{word}' is given twice; write it once`／`a tsuzuri code fence takes one of run=, test, ignore, and fails=; found '{a}' and '{b}'` | 開きの行 |
| `E2011` | `fails=` の値 | `fails= needs an error code such as E1012; found '{word}'` | 開きの行 |
| `E2011` | `run=` の符号化 | `'%' in run= must be followed by two hexadecimal digits; write a literal percent sign as %25`／`run= is not valid UTF-8 after decoding; encode the output as UTF-8` | 開きの行 |
| `E2011` | インデント・閉じがない・空の本文 | `a tsuzuri code fence must start at column 1 of the comment; remove the indentation`／`the tsuzuri code fence is not closed; end it with a line of three backticks`／`a tsuzuri example needs at least one line of code` | 開きの行 |
| `E2006` | 例の失敗（既存コード。意味は「言語内テストの失敗」の仲間） | `1 doc test failed`／`N doc tests failed` | 最初に失敗した例の開きの行 |
| `E1017` | 例が 10,000 を超える | 「集める範囲」の文 | `Span::default()` |
| `E2000` | 旗の誤り | 「CLI」の表 | — |

`E2011` はすべて集めて（最大 50 件。`DiagnosticSet::from_diagnostics`）から出し、1 件でもあれば例を 1 件も実行せず終了コード 1。`tsuzuri doc`・LSP・`check` は `E2011` を出さない（記法が壊れたフェンスは、素の本文として出す）。

### 7. `tsuzuri doc` とホバー（Phase 2）

`docgen::section` と `SemanticIndex::document` は、本文を `doctest::render_doc(text)` に通す。規則:

| 入力 | 出力 |
| --- | --- |
| 本文に ```` ```tsuzuri ```` がない | 本文そのまま（高速経路。1 byte も変えない） |
| 情報文字列が `tsuzuri` だけで、隠し行がない例 | そのまま（開き・本文・閉じとも） |
| オプションか隠し行のある例 | 開きの行を `` ```tsuzuri ``（個数は元のまま）にし、隠し行を除く。閉じの後ろに下の節を足す |
| `run=OUT`（復号した OUT の末尾の空白を除くと空でない） | 空行、`Output:`、空行、`` ```text ``、OUT の行、`` ``` `` |
| `fails=CODE` | 空行、``Compile error: `CODE`.`` |
| オプションが壊れている例（`E2011` になるもの） | 変えずにそのまま |
| `tsuzuri` 以外のフェンスと、フェンスの外の行 | そのまま |

`run=` の出力のフェンスは、OUT が `` ``` `` を含むなら、含まない最短の個数（4 個以上）にする。`api-diff`・KB・`tsuzuri guide` が読む `Documentation.text` は生のまま（`render_doc` は人間向けの表示だけに使う）。

### 8. 互換性

- `tsuzuri test` の新しい旗 `--doc`・`E2011`・`--json` の新しい `type`（`doctest`）は追加だけで、既存の旗・コード・キーの意味を変えない。`docs/stability.md` の「安定として保つもの」に、**例の記法**（フェンスのオプション `run=`・`test`・`ignore`・`fails=` の意味。新しいオプションは追加できる）と `--doc` の JSON のキーを足す（手順 8）。
- edition: 構文・予約語を足さないので、D-45・D-54 の対象外。全 edition で同じ。
- IR・ABI・WASM の import・`api-diff`・既存の診断のコード: 変わらない。std の `///` の追加は IR を変えない（`tests/docs.rs::attaches_docs_to_api_declarations_without_changing_ir`）。std の行番号が動くが、std 内のトラップは呼び出し側の位置で報告され、`std/…:行` を固定するテストは無い（`grep -rnE "std/[A-Za-z0-9]+\.(tz|tc):[0-9]+" tests src _tsuzuri docs README.md` が 0 件）。

## 設計

### データ構造（`src/doctest.rs`、新規。`lib.rs` に `pub mod doctest;` を `docgen` の次に足す）

```rust
pub const MAX_DOC_EXAMPLES: usize = 10_000;
pub const EXAMPLE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expectation { Check, Run(String), Test, Fails(String), Ignore }   // Run: 復号した OUT
impl Expectation { pub fn kind(&self) -> &'static str }                    // "check" "run" "test" "fails" "ignore"

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnerKind { Function, ExternFunction, ExternType, Record, Union, Alias, Constant, Class, Method }

#[derive(Clone, Debug)]
pub struct Example {
    pub index: usize,            // 通しの 0 始まり（collect が付ける。examples_in_source は 0）
    pub source: usize,           // Project::sources の添字
    pub module: String,          // `Geometry::Point`
    pub owner: String,           // `sum`、メソッドは `Eq.eq`
    pub owner_kind: OwnerKind,
    pub number: usize,           // doc 内の 1 始まり
    pub expectation: Expectation,
    pub program: String,         // 隠し行の印を取った本文。各行の後ろに '\n'
    pub line: usize,             // 開きの行のファイルでの 1 始まりの行
    pub span: Span,              // 開きの行の DocComment トークン。source を設定済み
}
impl Example { pub fn name(&self) -> String; pub fn label(&self) -> String }   // "sum#1"、"Array.sum#1"

pub struct Options { pub build: BuildOptions, pub level: u8, pub filter: Option<String>, pub indices: Vec<usize> }
impl Options { pub fn includes(&self, example: &Example) -> bool }          // indices が空か含み、かつ label が filter を含む
pub struct ExampleResult { pub example: Example, pub failure: Option<String>, pub output: String, pub duration_ms: u128 }
pub struct Report { pub results: Vec<ExampleResult>, pub ignored: usize, pub duration_ms: u128, pub messages: Vec<String> }
```

### 関数

| 関数 | 可視性 | 役割 |
| --- | --- | --- |
| `scan(lines: &[&str]) -> Scan` | 非公開 | フェンスの走査。`Scan { fences: Vec<Fence>, problems: Vec<(usize, String)> }`、`Fence { open, close, ticks, info }`（添字は `lines` の）。`tsuzuri` の例だけを `fences` に入れ、`problems` は `E2011` のメッセージ（インデント・閉じがない） |
| `parse_options(info: &str) -> Result<Expectation, String>` | 非公開 | `info` は `tsuzuri` の後ろ。`Err` は `E2011` のメッセージ |
| `decode_output(token: &str) -> Result<String, String>` | 公開 | `run=` の復号 |
| `program_of(body: &[&str]) -> String`、`is_hidden(line: &str) -> bool` | 非公開 | 隠し行の処理 |
| `documented_items(program: &Program) -> Vec<Item>` | 非公開 | `Item { owner, kind, doc: &Documentation }`。「集める範囲」の宣言を、`doc.span.start` の昇順で |
| `examples_in_source(text, edition, source, module) -> Result<Vec<Example>, Vec<Diagnostic>>` | 公開 | 1 ソースの例。`Err` は `E2011` の列（`index` は 0） |
| `collect(project: &Project) -> Result<Vec<Example>, DiagnosticSet>` | 公開 | 全ソース。`index` を付け、上限を確かめる |
| `run(project: &Project, input: &Path, examples: &[Example], options: &Options) -> Report` | 公開 | 実行器 |
| `render_doc(text: &str) -> String` | 公開（Phase 2） | 表示用の本文 |

### アルゴリズム

**`scan(lines)`**: `i = 0` から `lines.len()` まで。`ticks` = 行頭の `` ` `` の数。
1. `ticks >= 3` かつ `lines[i][ticks..]` に `` ` `` がない → 開きの行。`info` = その残りの `trim()`、`language` = `info` の最初の空白区切りの語。`close` = `j > i` で `lines[j].trim_end()` が長さ `>= ticks` かつ全部 `` ` `` である最小の `j`。
   - `language == "tsuzuri"` かつ `close = Some(j)`: `Fence { open: i, close: j, ticks, info }` を足し、`i = j + 1`。
   - `language == "tsuzuri"` かつ `close = None`: `problems` に `(i, "the tsuzuri code fence is not closed; end it with a line of three backticks")` を足し、ループを終える。
   - それ以外の言語: `close = Some(j)` なら `i = j + 1`、`None` ならループを終える。
2. 開きの行でなく、行が空白かタブで始まり、`trim_start()` が `` ```tsuzuri `` で始まってその直後が行末か空白 → `problems` に `(i, "a tsuzuri code fence must start at column 1 of the comment; remove the indentation")`、`i += 1`。
3. それ以外は `i += 1`。

**`examples_in_source`**: (1) `parser::parse_with_source(text, source, edition)`（失敗は `Err(vec![d])`）。(2) `lexer::lex_all(text, edition)` のトークンから `DocComment` だけを順に取る。(3) `documented_items` の各 `Item`（`doc.span.start` の昇順）について、`span` が `item.doc.span` の中にある `DocComment` を `lines`（本文）と `spans`（トークンの `span`）にする。`DocComment` のトークンも位置順なので、トークンの添字を前へ進めるだけで集める（計算量は O(トークン数 + 宣言数)）。(4) `scan(&lines)`。`problems` の `(k, message)` は `E2011` を `spans[k].in_source(source)` に出す。(5) 各 `Fence` について、`info` の `tsuzuri` より後ろを `parse_options`（`Err` は `E2011` を `spans[open]` に出して次へ）、本文 = `lines[open+1..close]`（空なら `E2011: a tsuzuri example needs at least one line of code`）、`program = program_of(本文)`、`line = diagnostic::location(text, spans[open].start).0`、`number` は doc 内の通し（`ignore` を含む）。(6) `owner_kind` と `owner` は `Item` から。

**`run`**（`run_one` は worker の中）:
1. `selected` = `options.includes` を満たす例。`ignored = examples.len() - selected.len()`。
2. `selected` から `Ignore` の例と、`Main.tz` の例（`project.sources[e.source]` の `relative_path == "Main.tz"` かつ `origin == User` かつ `dependency_package` が `None`）を除いて `ignored` に足す。`Main.tz` の例が 1 件以上あれば、通知を `messages` に入れる。残りが `runnable`。
3. `std_mode = !project.sources.iter().any(|s| s.origin == ModuleOrigin::User)`。
4. `workers = Parallelism::command_line().workers(runnable.len(), 1)`。`ordered_for_each(runnable.len(), workers, 1, None, |i| run_one(...), |_, item| { results.push(item.get()); ControlFlow::Continue(()) })`。結果は添字順になる。`run_one` が返す build の通知は、`BTreeSet` に集めて `messages` に足す。
5. `run_one(example)`: 開始時刻を取り、「判定表」の手順を行う。プロジェクトは `std_mode` なら `Project::single_main`、そうでなければ `Project::for_doc_example(input, &example.program)`（`Err(SourceError)` は `{code}: {message}` の失敗）。`test` の例は `driver::TestOptions { optimization: options.level, indices, ..Default::default() }` を `driver::run_tests_linked(&module, &options, &example_project.native)` に渡す。`run=` の例は `driver::run_captured_quiet(&module, &example_project, options.build, &example_project.native, Some(EXAMPLE_TIMEOUT))`。`example_project` は例のために組んだ `Project`（`[native]` はパッケージの manifest のもの）。
6. `Report.duration_ms` は全体の時間。計算量は例の数に比例。メモリは例ごとのプロジェクト 1 個ずつ（同時に `workers` 個まで）。

**`render_doc`**（Phase 2）: 本文に `` ```tsuzuri `` が無ければそのまま返す。あれば `text.split('\n')` を `scan` し、各 `Fence` を前から順に: `parse_options` が `Err` なら元の行をそのまま、`Ok` で情報文字列が `tsuzuri` だけかつ隠し行がなければ元の行をそのまま、そうでなければ「仕様 7」の表のとおりに書き換え、`Fence` の外の行はそのまま写して、`\n` でつなぐ。

### 既存コードの変更表

| ファイル | 変更 |
| --- | --- |
| `src/lib.rs` | `pub mod doctest;` を `pub mod docgen;` の次に足す |
| `src/driver.rs` | (1) `impl Project` に `pub fn for_doc_example(input: &Path, text: &str) -> Result<Self, SourceError>`（`load_with_overlays` と同じに `directory` を求めて `fs::canonicalize` し、`overlays = {directory/Main.tz → text}` で `Self::load_from_root(&directory, None, &overlays, false, true)`）。(2) `enum RunStdio` の `Capture` に `echo: bool` を足し、`run_process` の腕から `capture(.., echo)` へ、`capture` の `relay(stderr, &mut collected, echo)` へ渡す。`run_captured` は `echo: true`。(3) `pub fn run_captured_quiet(module, project, options, links: &LinkInputs, timeout: Option<Duration>) -> Result<(Vec<String>, Vec<u8>), Diagnostic>`（`echo: false`、`limit: MAX_CAPTURED_OUTPUT`）。`grep -n "RunStdio::Capture" src/driver.rs` が 3 箇所（構築・定義・腕）を見つけるので全部直す |
| `src/main.rs` | `Arguments` に `test_doc: bool`（`lsp` の早期 return のリテラルを含め、`test_seed` を持つ全リテラルに足す。`grep -n test_seed src/main.rs`）。`parse_arguments` に `Some("--doc")` の腕と、「CLI」の表の拒否。読み込みは `Action::Test` かつ `test_doc` なら `Project::load_for_docs`、`AnalysisKind::Docs`。警告の表示と `--deny-warnings` の後で `run_doctest_action` を呼ぶ。`--help` の `tsuzuri test` の行と説明に `--doc` を足す。`mod tests` に `parses_test_doc_options` を足し、`rejects_ambiguous_or_unused_arguments` に拒否の行を足す |
| `src/docgen.rs`（Phase 2） | `section` の `text.push_str(&doc.text)` を `text.push_str(&crate::doctest::render_doc(&doc.text))` に |
| `src/semantic.rs`（Phase 2） | `SemanticIndex::document` の `doc.text.clone()` を `crate::doctest::render_doc(&doc.text)` に |

`run_doctest_action(arguments, project) -> ExitCode`（`main.rs`）: `doctest::collect` の `Err` は `print_diagnostics` して `FAILURE`。`--index` の範囲検査。`--list`。`doctest::run`。結果と要約の出力（「出力」の表）。失敗があれば `E2006` を、最初に失敗した例の `span` で `print_diagnostic(.., &source.path, &source.text, json)`（`source = project.source_for(&diagnostic)`）。

## 前提とする他チケットのインターフェース

- **G22**（`Diagnostic` の拡張）: `Diagnostic::new(code, message, span)` だけを使い、構造体リテラルで作らない。G22 が先に入っていてもそのまま動く。
- **G23**（kind）: D-51 の 3 通り。G23 があれば S1 の手順で `E2011` の kind `doc_example_invalid`、doc の失敗の `E2006` の `doc_test_failed` を登録して `of` で出し、G22 だけなら `with_kind` で付け、どちらもなければ付けない（kind は条件ごとに 1 つ。`test_failed` を流用しない）。G23 があれば、説明ページに `E2011` を足す。
- **G21・G26・G27**: `doctest::examples_in_source` と `render_doc` は公開で、名前と型を変えない（ガイドが std の例を読む）。KB の `doc` 欄は、生の `Documentation.text` を使う（決定的で、`render_doc` の書式変更に影響されない）。
- **G35**: doc の構造化タグ（`@complexity`・`@since`・`@see`）は行頭の `@` の行で、フェンスの外にある。本チケットの走査はそれを見ない。

## 実装手順

各手順の最後でビルドとテストが緑であること。fixture（`tests/fixtures/doctest/…`）の全文と、想定出力の詳細は付録 B。`$T` は `target/release/tsuzuri`。

### Phase 1

**手順 1: ベースライン。** 変更なし。`cargo build --release --locked && cargo test --locked` の件数と `sh scripts/check-runtime-includes.sh` を控える。`$T doc std -o <dir>` を付録 D の script で数え、559・305・0 を確かめる。付録 B の `pass`・`fail` を `target/tz-G34` に作り、「現状」の表の結果（5 件と 5 件）を再現する（食い違えば停止条件 1）。`node tests/repl.mjs target/release/tsuzuri` が通ることを控える。確認: すべて一致。

**手順 2: 記法の純粋関数（テスト先行）。** 変更: `src/doctest.rs`（新規）、`src/lib.rs`。内容: 「データ構造」の型と `scan`・`parse_options`・`decode_output`・`program_of`・`is_hidden` を、本体を `todo!()` にして書き、`mod tests` に次の 7 件を書く。`decodes_percent_escapes_like_decode_uri_component`（`""`→`""`、`%20`→空白、`%E3%81%82`→`あ`、`%2B`→`+`、`a+b`→`a+b`、`%25`→`%`）、`rejects_malformed_percent_escapes`（`%`・`%2`・`%zz`・`%FF`）、`parses_each_option_and_rejects_the_rest`（`""`→Check、`run=3`、`run=`→`Run("")`、`test`、`ignore`、`fails=E1012`、`fails=W1001`・`fails=e1012`・`foo`・`test ignore`・`test test`・`run=%zz` は `Err`）、`finds_fences_only_at_column_zero_with_the_tsuzuri_language`、`longer_fences_close_only_on_an_equal_or_longer_run`、`unclosed_and_indented_fences_are_problems`、`hidden_lines_lose_their_marker_and_keep_indentation`。まず `cargo test --locked --lib doctest` が `not yet implemented` で 7 件落ちるのを確かめ、実装して通す。確認: `running 7 tests` で 7 件通る。`cargo test --locked` は手順 1 の件数＋7。

**手順 3: 例の収集。** 変更: `src/doctest.rs`。内容: `documented_items`・`examples_in_source`・`collect`・`Example::name`・`label`・`Expectation::kind`・上限の検査 `check_limit(count)`。`mod tests` に追加: `examples_map_to_file_lines_across_blank_and_comment_lines`、`collect_skips_private_generated_and_instance_docs`、`class_method_examples_are_named_class_dot_method`、`examples_are_numbered_per_doc_and_indexed_across_the_project`、`the_limit_is_ten_thousand_examples`（`check_limit(MAX_DOC_EXAMPLES + 1)` が `E1017`）。確認: `cargo test --locked --lib doctest` が 12 件。

**手順 4: driver。** 変更: `src/driver.rs`。内容: 「変更表」の (1)〜(3)。`mod tests` に `for_doc_example_replaces_main_and_keeps_the_other_modules`・`for_doc_example_adds_main_to_a_library`・`for_doc_example_loads_only_the_std_modules_it_names`（`Json` を名指す本文と名指さない本文で `Std` のソース数が違う）。確認: `cargo test --locked --lib driver` が緑で、`cargo build --release --locked && node tests/repl.mjs target/release/tsuzuri` の結果が手順 1 と同じ（停止条件 2）。

**手順 5: 実行器。** 変更: `src/doctest.rs`。内容: `Options`・`ExampleResult`・`Report`・`run`・`run_one` と補助 `normalize`・`shown`・`location_suffix`。`mod tests` に `normalizes_line_endings_and_trailing_whitespace_only`・`shown_truncates_at_200_characters`・`location_suffix_uses_example_lines`。確認: `cargo test --locked --lib doctest` が 15 件。

**手順 6: fixture と統合テスト（テスト先行）。** 変更: `tests/fixtures/doctest/{pass,fail,notation,main}/`（付録 B の全文）、`tests/doctest.rs`（新規）。内容: 「テスト計画」の `tests/doctest.rs` の全件を書き、CLI を使うものには `#[ignore = "G34 step 7"]` を付ける（`--doc` がまだ無いため）。ライブラリ API だけを使う `library_api_judges_check_fails_and_ignore_without_a_toolchain` は `ignore` を付けず、ここで通す。確認: `cargo test --locked --test doctest` が `1 passed; 0 failed; 11 ignored`。

**手順 7: CLI。** 変更: `src/main.rs`。内容: 「変更表」の `main.rs` の行と `run_doctest_action`。`mod tests` に `parses_test_doc_options` と、`rejects_ambiguous_or_unused_arguments` への拒否の行（「CLI」の表の 7 件。`--index` の範囲外は除く）。手順 6 の `ignore` をすべて外す。確認: `cargo test --locked --bin tsuzuri` が緑。`cargo build --release --locked && $T test --doc tests/fixtures/doctest/pass` が、付録 B の想定出力と一致して終了コード 0。`$T test --doc tests/fixtures/doctest/fail` が 7 件失敗して終了コード 1、`.../notation` が `E2011` を 6 件（行 3・7・11・15・19・22）出して終了コード 1、`.../main` が `1 passed; 0 failed; 1 ignored`。`cargo test --locked --test doctest` が `12 passed`。

**手順 8: Phase 1 の文書と CI。** 変更: 「ドキュメント」の Phase 1 の行すべて、`.github/workflows/targets.yml`（`tests/features.mjs` の手順の近くに、`target/release/tsuzuri test --doc tests/fixtures/doctest/pass` の 1 手順）、`_features/GUIDE.md`（D-55 の「予約」の行から `E2011` を外し、診断コードの行に `E2011 | G34 | doc の例の記法の誤り` を足す）、`docs/stability.md`。確認: `node scripts/check-docs.mjs <変更したページ>` が通る。`cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings`・`cargo test --locked` が緑。

### Phase 2（Phase 1 が済んでから）

**手順 9: `render_doc`（テスト先行）。** 変更: `src/doctest.rs`。内容: `render_doc` を `todo!()` で書き、`mod tests` に `render_doc_is_the_identity_without_options_or_hidden_lines`（`tests/docs.rs` の `"First.\n```tsuzuri\nfirst()\n```"` と、フェンスなしの本文、`text` のフェンス）・`render_doc_hides_lines_and_shows_outputs`（`run=3` と `# let base = 10` の例が「仕様 7」の表どおり）・`render_doc_shows_the_error_code_of_a_failing_example`・`render_doc_keeps_malformed_fences`（`run=%zz`・`sloppy`）・`render_doc_uses_a_longer_fence_for_outputs_with_backticks` を書いて失敗を確かめ、実装する。確認: `cargo test --locked --lib doctest` が 20 件。

**手順 10: つなぐ。** 変更: `src/docgen.rs`・`src/semantic.rs`・`tests/docs.rs`・`tests/lsp.rs`。内容: 「変更表」の 2 行。追加テスト: `tests/docs.rs::docs_hide_setup_lines_and_show_outputs`（`docgen::render_module` が `Counter.md` の想定出力どおり）、`tests/lsp.rs::semantic_docs_hide_the_setup_lines_of_examples`（`index.doc_for(..)` に `# ` の行がなく `Output:` がある）。確認: `cargo test --locked --test docs --test lsp` が緑で、既存の `renders_public_apis_in_signature_order_and_keeps_markdown` と `node tests/docgen.mjs target/release/tsuzuri` の期待値が**変わらず**通る（停止条件 4）。

**手順 11: Phase 2 の文書。** 変更: 「ドキュメント」の Phase 2 の行。確認: `node scripts/check-docs.mjs _tsuzuri/language-reference/organizing-tsuzuri/documentation-comment.md`。

### Phase 3（Phase 2 が済んでから。付録 C の表と名前の一覧に従う）

**手順 3.0: 床の検査と CI。** 変更: `tests/std_examples.rs`（新規）、`.github/workflows/targets.yml`。内容: 付録 C の「`tests/std_examples.rs` の骨子」を写す。`FLOORS` は、そのときの `stdlib::SOURCES` の全モジュールを 0 で並べる（47 は HEAD の数。D14・F15 など他のチケットが先に std へ足していれば増える。付録 C の表に無い関数・モジュールは、手順 3.23 の後に 1 手順を足して例を付け、手順 3.24 に間に合わせる）。`EXEMPT` は `Json` の 10 個、`BUILDERS` は `Async`・`IO`・`Maybe`・`Result`。テスト 4 件: `std_examples_meet_the_floors`（表を `--nocapture` で出す）、`exemptions_stay_below_one_fifth_of_the_public_functions`、`every_std_example_has_valid_notation`（`examples_in_source` が全 std ソースで `Ok`）、`builder_modules_show_a_computation_expression`（`BUILDER_FLOORS`＝`Async`・`IO`・`Maybe`・`Result` が各 0 から始まり、`Bind` に `ignore` でない例が床の数以上ある。手順 3.4 で `Maybe`・`Result`、3.18 で `IO`、3.19 で `Async` の床を 1 に上げる）。CI に `target/release/tsuzuri test --doc std -O0` と `-O3` の手順を足す（0 件で通る）。確認: `cargo test --locked --test std_examples` が 4 件通り、`--nocapture` の表が付録 C と同じ件数（559・305・例 0）。

**手順 3.1〜3.23: モジュール単位。** 付録 C の「手順」の表の 1 行が 1 手順。各手順で、そこに挙がった関数（`*` は説明が無い関数）の doc コメントに、次の規則で例を足す。

| 規則 | 内容 |
| --- | --- |
| 場所 | 関数の `def` の直前の `///`。説明が無ければ 1 文を足す（英語、大文字で始め、ピリオドで終える。関数の本体と署名から確かめられることだけ。引数の順序は本文に書く）。説明と例の間に `///` だけの行を 1 行置く。G35 の `@complexity`・`@since`・`@see` の行は説明に数えない: タグの行がある関数は、説明の 1 文をタグの行の前に足し、タグの行と例の間にも `///` だけの行を 1 行置く（順序は G35 の「仕様 3.4」: 説明、`///` だけの行、タグ、`///` だけの行、例） |
| 形 | 1 関数に 1〜2 例。1 例は 1〜6 行。`run=` を第一候補にし、決定的でない・OS に触れる・時間・乱数・ネットワーク・標準入力・GPU の関数は `` ```tsuzuri ``（検査だけ）。`Gen`・`Test` は `test` |
| 出力が出ない値 | `Maybe`・`Result`・record・union は末尾の式にしても何も出ない。`match` で取り出す、`Maybe.default_value`・`Result.default_value` で値にする、`test` にする |
| 期待値 | 説明と手計算（または別の言語）から作る。**コンパイラの出力を写さない**。食い違ったら止める（停止条件 8・9） |
| 符号化 | `run=` の値は「仕様 1」の符号化（`%20`、`%0A`、`%25`、`%60`）。複数行の出力は `%0A` |
| 隠し行 | 準備の行は `# ` で隠してよい（Phase 2 の後なので使える）。読者に見せたい行は隠さない |
| 検証済みの形 | 付録 A に 22 件。モジュールの近い関数の例を真似る |
| 同時に | 手順の最後に `FLOORS` の該当モジュールを、その手順で例を持たせた関数の累計（付録 C の「目標」）へ上げる |

確認（各手順）: `cargo build --release --locked`（std は `include_str!` で埋め込まれる）、`target/release/tsuzuri test --doc std --filter "<モジュール名>."` を `-O0` と `-O3` で実行して全件 `ok`、`cargo test --locked --test std_examples`、`target/release/tsuzuri doc std -o build/std-api` が成功して該当ページの例が読める、`cargo test --locked` が緑。`Map.` の絞り込みは `HashMap.` の例も選ぶ（部分一致）。選ばれても問題ない。

**手順 3.24: 厳格化と文書。** 変更: `tests/std_examples.rs`、文書。内容: テスト `every_public_std_function_has_an_example` を足す（`EXEMPT` と大文字で始まる名前を除く全公開 `def` に `ignore` でない例がある）。以後、新しい std の公開関数は例を持たなければならない。**手順 3.0（`tests/std_examples.rs`）の後に std のモジュールや公開関数を足すチケット**（D14 の `Text`・`RegexText`、F14 の `Alloc`、F15 の `Monoid`・`Monoids`・`Array.fold_monoid`・`Parallel.fold`、B10 の `Domain`、C12 の `chunk_count`・`invariants_hold`、A21 の `Gen.for_all_where` など）は、足した公開関数に本チケットの形式の例を付け、新しいモジュールを `FLOORS`（目標＝そのときの公開関数の数）に足す。`std_examples_meet_the_floors` は `FLOORS` のキーと `SOURCES` の一致を検査するので、足さないと赤になる。`FLOORS` は削除せず、記録として残す。文書の更新（「ドキュメント」の Phase 3 の行）。確認: `cargo test --locked --test std_examples` が 5 件通り、`$T test --doc std` が `-O0`・`-O3` で `0 failed`、`passed` の件数が付録 C の合計（507 関数＋builder 4 の例）以上。

## テスト計画

**単体（`src/doctest.rs` の `mod tests`）**: 手順 2・3・5・9 の 20 件（名前は各手順）。`decode_output` の期待値は JavaScript の `decodeURIComponent`（`node -e "console.log(decodeURIComponent('%E3%81%82'))"` で `あ`）と照合した値。

**統合（`tests/doctest.rs`、新規。手本 `tests/bench.rs`の `project`・`tsuzuri`・`clean`）**: fixture は `tests/fixtures/doctest/` の 4 つ（付録 B）。`TSUZURI_CACHE_DIR` を一時ディレクトリにして `env!("CARGO_BIN_EXE_tsuzuri")` を呼ぶ。

| テスト | 入力 | 期待 |
| --- | --- | --- |
| `library_api_judges_check_fails_and_ignore_without_a_toolchain` | 一時パッケージの `Lib.tz`: `check` 1・合格する `fails=E1003` 1・受理されてしまう `fails=E1003` 1・`ignore` 1。`Project::load_for_docs` → `collect` → `run` | `failure` が `None`・`None`・`Some("expected E1003, but the example was accepted")`、`ignored == 1`。clang 不要（`ignore` を付けない） |
| `list_shows_every_example_with_its_kind` | `pass` の `--list` と `--list --json` | text 6 行（付録 B）。JSON 6 行で、キーは辞書順 `index`・`kind`・`line`・`module`・`name`・`path`・`type`、`line` は 3・9・14・22・26・30 |
| `passing_fixture_passes_at_o0_and_o3` | `pass` を `-O0`・`-O3` | stdout が付録 B の想定出力（`ok 1`〜`ok 5`、`5 passed; 0 failed; 1 ignored`）、終了 0、stderr 空。2 回の出力が一致 |
| `passing_fixture_json_has_the_documented_keys` | `pass --json` | 結果 5 行が「出力」の JSON のキー順（行の文字列で確かめる。`duration_ms` を除く）、最後に `{"type":"summary","passed":5,"failed":0,"ignored":1,...}` |
| `failing_fixture_reports_each_failure_kind` | `fail` | 終了 1。7 件の `failure` が付録 B のとおり（`E2005` の件は先頭 `E2005: ` だけ）。stderr の最後が `error[E2006]: 7 doc tests failed`（`Counter.tz:3`） |
| `notation_errors_are_e2011_and_nothing_runs` | `notation --json` | 終了 1、stdout 空、stderr に `E2011` が 6 行（`span.line` が 3・7・11・15・19・22。メッセージの先頭は「診断」の 6 種） |
| `main_tz_examples_are_ignored_with_a_note` | `main` | `1 passed; 0 failed; 1 ignored`、stderr に `1 doc example in Main.tz not run`、終了 0 |
| `filter_and_index_select_examples` | `pass --filter add_one#2`／`--index 3`／`--index 99` | `1 passed; 0 failed; 5 ignored`（2 回）／`E2000` と `doc example index is out of range`（終了 1） |
| `check_only_examples_need_no_native_toolchain` | `check`・`fails`・`ignore` だけの一時パッケージを、`TSUZURI_CLANG` を存在しないパスにして | 終了 0（`tests/docgen.mjs` と同じ作り）。`run=` の例を足すと、その例だけが失敗として報告される |
| `rejected_option_combinations_exit_with_2` | `check --doc`、`test --doc` に `--coverage x`・`--seed 1`・`-g -o x`・`--target native`・`-l m` | 終了 2。stderr に「CLI」の表のメッセージ |
| `package_with_a_manifest_and_nested_modules_runs_its_examples` | `tsuzuri new` と同じ `Tsuzuri.toml` と `Main.tz`（`def main` と `test` つき）、`Geometry/Point.tz`（`run=0` の例 `Geometry::Point.origin_x()`） | `1 passed; 0 failed; 0 ignored`（`Main.tz` は置き換わり、元の `def main` と衝突しない。パッケージ自身の `test` は走らない） |
| `std_directory_is_read_as_std` | `<repo>/std` の `--list --json` | 終了 0、全行の `path` が `std` を含む（Phase 1 の時点では 0 行。Phase 3 の後は 507 行以上で `module` に `Array` がある） |

`src/main.rs` の `mod tests`: `parses_test_doc_options`（`parse(&["test","Lib","--doc","--filter","x","--list","--json","-O3"])` で `test_doc`・`test_filter`・`test_list`・`json`・最適化）、`rejects_ambiguous_or_unused_arguments` に「CLI」の表の 7 行（`--doc` と `check`、2 回、`--coverage`、`--seed`、`-g`、`--target`、`-l`）。

**既存テストへの影響: なし。** ① `--doc` なしの `tsuzuri test` は新しい分岐に入らない（`test_doc` が偽）。② `render_doc` は、オプションも隠し行もない `tsuzuri` のフェンスの本文を 1 byte も変えない（`tests/docs.rs` の `renders_public_apis_in_signature_order_and_keeps_markdown` と `tests/docgen.mjs` の `twice 21` が通る）。③ `run_captured` は `echo: true` で従来どおり。④ std の `///` の追加は IR を変えない。

**決定性**: `--list` と `--json`（`duration_ms` 以外）は、同じ入力で 2 回とも一致する。`TSUZURI_THREADS=1` と `8` で結果の順序が同じ（`passing_fixture_passes_at_o0_and_o3` が `TSUZURI_THREADS=1` でも 1 回走る）。**資源上限**: `the_limit_is_ten_thousand_examples`。30 秒の時間切れは、30 秒かかるのでテストにしない（`capture` の時間切れは REPL 側が持つ）。**Windows**: `tests/doctest.rs` は、パスは末尾（`Counter.tz`）だけ、トラップの文言は `E2005: ` の接頭辞だけを検査する。CRLF は実装が正規化する。

## ドキュメント

`grep -rnE "doc-test|--seed|ドキュメント生成" _tsuzuri docs README.md` で漏れを探してから、次を更新する。

| ファイル | Phase | 内容 |
| --- | --- | --- |
| `_tsuzuri/language-reference/organizing-tsuzuri/documentation-comment.md` | 1・2 | 「オプションと制限」の「**doc-test は無い**」の行を、`tsuzuri test --doc` への導線に置き換える。新しい節「例を書く（doc-test）」: 記法の 2 つの表・符号化・隠し行・`Main.tz` の例・`--doc` の使い方と出力・`E2011`（例は `check-docs.mjs` を通る形で、`project=`・`file=`・`run=` を使う）。Phase 2 で `tsuzuri doc` の出力（隠し行・`Output:`・`Compile error:`）とホバーの節 |
| `_tsuzuri/language-reference/compiler/option.md` | 1 | 表に `--doc`（`test` の列だけ○）。エラーの表に「CLI」の 7 件のメッセージ |
| `_tsuzuri/language-reference/compiler/usage.md` | 1 | `test` の節（`--seed` の段落の後ろ）に `--doc`。`doc` の節の導線 |
| `_tsuzuri/language-reference/built-in-types-and-modules/test.md` | 1 | オプションの表に `--doc`（`--seed` の行の次） |
| `_tsuzuri/language-reference/compiler/diagnostics.md` | 1 | `E2011` の行。「`E2`（`E2000`〜…）」の範囲を台帳の最大のコードまでに直す |
| `docs/language.md` | 1・2 | 「ドキュメントコメント」の「また doc-test の実行は行いません」を置き換え、記法・`--doc`・`E2011`・出力の変更を足す |
| `README.md` | 1・3 | 「テストランナー」に `--doc` の 1 例、「ドキュメント生成」に記法の 3 行、「コマンド書式」の `tsuzuri test` の行、「主要オプション一覧」に `--doc`、検証の節に `tsuzuri test --doc std -O0`／`-O3` |
| `docs/architecture.md` | 1・3 | ファイル表（`src/docgen.rs` の行の次）に `src/doctest.rs`、テスト表に `tests/doctest.rs`・`tests/std_examples.rs` |
| `docs/stability.md` | 1 | 「安定として保つもの」に「doc の例の記法（`run=`・`test`・`ignore`・`fails=` の意味。追加はよい）と `--doc` の JSON のキー」 |
| `_features/GUIDE.md` | 1 | D-55 に `E2011`。§4 のコードマップに `src/doctest.rs` |

検証: `node scripts/check-docs.mjs` に、変更した `_tsuzuri` の 5 ページを渡す。

## 受け入れ条件

**Phase 1**
- [ ] 手順 1〜8 を実施し、各手順の確認が通る。
- [ ] `tsuzuri test --doc` が `pass` で 5 件成功・1 件 ignored・終了 0、`fail` で 7 件失敗・終了 1、`notation` で `E2011` 6 件・終了 1、`main` で 1 件成功・1 件 ignored。
- [ ] `--doc` なしの `tsuzuri test` の出力・終了コードと、REPL の出力が、手順 1 と同じ。
- [ ] `tests/doctest.rs` の 12 件、`src/doctest.rs` の 15 件、`src/main.rs` の追加分が通る。`cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings`・`cargo test --locked` が緑。文書の検証が通る。

**Phase 2**
- [ ] `tsuzuri doc` とホバーが隠し行を出さず、`run=`・`fails=` の期待を併記する。オプションも隠し行もない例の出力は 1 byte も変わらない（既存の 2 つの期待値が変わらない）。

**Phase 3**
- [ ] 付録 C の全手順が済み、`tests/std_examples.rs` の 5 件が通る（免除を除く 507 関数と builder 4 モジュールに例）。
- [ ] `target/release/tsuzuri test --doc std` が `-O0` と `-O3` で `0 failed`（件数を完了報告に書く）。`cargo test --locked` が緑。
- [ ] 見つけた std のバグと説明の誤りを、完了報告の「残作業」に、直したものと直していないものに分けて挙げた。
- [ ] GUIDE §13.2 の書式の完了報告をチケット末尾に書いた。コミットは人間の指示まで作らない。

## 落とし穴

- **`Main.tz` を置き換える**: 元の `Main.tz` の `def main` と式は例のプロジェクトにない。`Main.tz` の例は実行しない（`ignored`）。他のモジュールの例が `Main.f` を呼んでいると `E1002` になるので、関数を別のモジュールへ移すか `ignore` にする。
- **std は組み込み**: `std/*.tz` に例を足したら `cargo build --release --locked` を先に行う。忘れると、例は新しいファイルから、std は古い埋め込みから読まれ、新しい関数が `E1002` になる。
- **出力が出ない値**: `Maybe.Some 3` を末尾にした例は、実際の出力が空になる。`run=Some%203` は不一致で落ちるが、`run=`（空）は**通ってしまう**。`run=` を空にするのは、本当に何も出さない例だけ。
- **`ref` と型**: 数値リテラルを `ref` の引数へは `(ref 2)`。型注釈つきの式 `(e: T)` は書けず `let x: T = e`。整数リテラルの既定は `i32`。`Test.equal` の数値も `(ref 6)`。
- **`check` は実行しない**: 印のない例の `assert false` はトラップしない。実行して確かめたいなら `run=`。
- **標準エラーは捨てる**: `Debug.print` の出力を確かめる例は書けない。
- **タイムアウトの文言**: 時間切れは `E2005: evaluation exceeded the 30-second limit and was stopped; use --timeout to change it`（REPL 向けの文言。`--doc` に `--timeout` はない）。そのまま `failure` に入れる。
- **0 件のとき**: `tsuzuri test --doc <dir> --list` が 0 件なら、フェンスの言語（`tsuzuri`）・桁（0 桁目）・宣言の公開（`private` は集めない）・`Main.tz` を疑う。

## 対象外

- `--doc` なしの `tsuzuri test` が doc の例も走らせること。WASM・named native target・`--coverage`・`-g`・`--seed` との併用。
- `@complexity`・`@since`・`@see`（G35）。言語リファレンスの例の検査（`check-docs.mjs`）の変更。
- 複数の例が状態を共有する機能（各例は独立）。`Main.tz` の例の実行。
- VS Code の Testing ビューへの表示。例の結果のキャッシュと差分実行。
- std の組み込み関数（例: `Math.sin`。`std/*.tz` に宣言がないもの）の例。型・定数・クラスの例の義務（付けてよいが検査の床にしない）。

## 決定事項

すべて承認済み（D-50）。

- **D1 枠**: 例を `Main.tz` として、`load_from_root` の `overlays` で組む。理由: 既存の道で、`run`・`test` と同じ解釈になり、manifest・依存・名前空間をそのまま使える。却下: (a) 例を `test` 宣言に包んで `tsuzuri test` へ流す（`run=` の標準出力を取れない）、(b) 関数に包んで REPL 方式で生成する（例の中で `def`・`record` を書けない）、(c) 一時ディレクトリへ複製する（`path` 依存の相対パスが壊れる）。
- **D2 同じプロセスで実行**: 診断を型で受け取れ、`ordered_for_each` で並列にできる。却下: 子プロセスで CLI を呼ぶ（遅く、診断が文字列になる）。
- **D3 記法**: `check-docs.mjs` と同じ `run=`（percent-encoding）に、`test`・`ignore`・`fails=CODE` を足す。却下: Rust 風の `no_run`・`should_panic`・`compile_fail`（`fails=` が `compile_fail` に、`test` と `assert` が `should_panic` に当たる）、出力用の別フェンス、引用符つきの `run="…"`（符号化と二重になる）。
- **D4 隠し行は `# `**: 行頭の `#` は Tsuzuri のソースで常に `E0002` なので、本物の行と衝突しない。却下: `##` のエスケープ（要らない）、`//` の行（普通のコメントと区別できない）。
- **D5 `--doc` は doc の例だけ走らせる**: 却下: `test` と一緒に既定で走らせる（Rust 方式。既存の出力・時間・終了コードが変わる）。
- **D6 `Main.tz` の例は走らせない**: 却下: `Main.tz` への追記（入口が 2 つになる）、元の `Main.tz` を別名で残す（`Main.f` の参照が壊れる）。
- **D7 比較は正規化した完全一致**: 却下: 部分一致・正規表現（誤りを隠す）。
- **D8 文書に出る例だけ検査する**: 読者が見る例と検査される例を同じにする。`private` の例は集めない。
- **D9 診断**: 失敗は既存の `E2006`、記法の誤りは新しい `E2011`。却下: 記法の誤りを `E2000`（引数の誤りと混ざる）や `E0002`（構文の誤りと混ざる）にする。
- **D10 並列**: `Parallelism::command_line()` と `ordered_for_each`（8 MiB の worker、添字順）。却下: 独自のスレッドプール。
- **D11 標準エラーを流さない**: `run_captured_quiet` を足す。却下: `run_captured` の流用（並列で混ざる）、`run_captured` の既定を変える（REPL が変わる）。
- **D12 std の完了条件**: 免除を除く全公開関数（507）とビルダー 4 モジュールの `Bind`。免除は `.tc` のビルダー操作 42 個（各モジュールに `Bind` の例を 1 つ置いて代える）と `Json` の derive 用 10 個。全体の「80% 以上」は、`EXEMPT` が公開 `def` の 20% 未満という検査に置き換える（どれを飛ばしてよいかが一覧で見える）。却下: 比率だけの条件（飛ばす関数が曖昧になる）。
- **D13 std の例は Phase 2 の後**: 隠し行を使えるようにするため。
- **D14 期待値は独立に作る**: 食い違ったら期待値を直さず止めて報告する（std のバグを隠さない）。
- **D15 `render_doc` は表示専用**: api-diff・KB・ガイドは生の `Documentation.text` を読む。

付録: [G34-appendix.md](G34-appendix.md)（A 検証済みの例 22 件、B fixture と想定出力、C std の作業表、D 数え方）。

## 実装と検証（2026-10-30）

- **範囲**: Phase 1〜3（手順 3.0〜3.24）をすべて実装した（1 つの PR）。冒頭の「方針の最新化」のとおり、起票後の変更に合わせた。
- **Phase 1**: `src/doctest.rs`（`Expectation`・`OwnerKind`・`Example`・`Options`・`ExampleResult`・`Report`、フェンスの走査 `scan`、`parse_options`・`decode_output`・隠し行、`examples_in_source`・`collect`（上限 10,000）・`run`（`parallel::ordered_for_each`、`Main.tz` の例は `ignored` と通知 1 行））、`src/driver.rs`（`Project::for_doc_example`・`run_captured_quiet`・`RunStdio::Capture { echo }`）、`src/main.rs`（`--doc` の解析と拒否、`run_doctest_action` の text・JSON・`--list`・要約・`E2006`）、`src/diagnostic_kinds.rs` の 3 つの kind、`src/explain/codes/E2011.md`。fixture は `tests/fixtures/doctest/{pass,fail,notation,main}`（付録 B）。
- **Phase 2**: `doctest::render_doc` を `docgen::section` と `SemanticIndex::document` が使う（隠し行を除き、`Output:`・``Compile error: `CODE`.`` を足す。印も隠し行もない本文は 1 byte も変えない）。
- **Phase 3**: std の 43 ファイルに `///` の例 823 個（公開関数 819 個に 1 個ずつと、`Maybe`・`Result`・`IO`・`Async` の `Bind` に計算式の例 1 個ずつ）と、説明のなかった関数への 1 文の説明（`///` の追加だけで、コードは変えていない）。`tests/std_examples.rs`（`FLOORS` 48 モジュール、`EXEMPT` は `Json` の 10、`BUILDER_FLOORS`、テスト 5 件）。`targets.yml` に `tsuzuri test --doc tests/fixtures/doctest/pass` と `--doc std` の `-O0`・`-O3`。種別は `run=` が大半で、OS・ファイル・ネットワーク・時間・OS の乱数・標準入力・ホストに触れる関数は検査だけ、`Gen`・`Test`・`Alloc` と、`unit` を返す `iter` の系列の一部は `test`。期待値は説明と本体から手で求め（`HashMap.sip13` は SipHash-1-3 の独立した実装、`Random.pcg_next_u32`・`pcg_next_u64` は公開されている pcg32 の値）、コンパイラの出力を写していない。ハッシュの順に依存する出力はない。
- **検証**: `cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings` 成功。`tests/doctest.rs` 12 件、`tests/std_examples.rs` 5 件、`src/doctest.rs` の単体 20 件、`src/main.rs`・`src/driver.rs` の追加分、`tests/docs.rs`・`tests/lsp.rs` の隠し行のテストが通る。`target/debug/tsuzuri test --doc std -O0` と `-O3` がどちらも `823 passed; 0 failed; 0 ignored`（macOS arm64、各約 2 分）。`tsuzuri test --doc` の fixture は `pass` 5 件成功・1 件 ignored、`fail` 7 件失敗、`notation` で `E2011` 6 件、`main` で 1 件成功・1 件 ignored。
- **文書**: `documentation-comment.md`（節「例を書く（doc-test）」: 記法・印・符号化・隠し行・`tsuzuri test --doc`・失敗の文・`E2011`・`tsuzuri doc` とホバーの表示・std の例、比較の表）、`option.md`（`--doc` の列と誤りの 7 行）、`usage.md`（`test` の `--doc`、`doc` からの導線）、`test.md`（オプションの表）、`diagnostics.md`（`E2011`、`E2` の範囲、`E2006`・`E1017` の kind）、`docs/language.md`（「doc の例（doc-test）」と表示の規則）、`README.md`（テストランナー・ドキュメント生成・CLI 書式・オプション表・テスト表・検証のコマンド）、`docs/architecture.md`、`docs/stability.md`、`_features/GUIDE.md`（§4 のコードマップ、D-55 の `--doc` の行）。
- **残作業（例を書く間に見つけた std と文書の問題）**:
  - 直したもの: `vec.md` と `docs/language.md` は `Vec.sort_with`・`sort_range_with` が「NaN を特別に扱わない」としていたが、実装は `Array.sort_in_place_with` で並べるので `f32`・`f64` の NaN を後ろに置く（`sort_withr`・`sort_range_withr` は渡した関数のとおり）。実測に合わせて直した。`List.containsr` の説明（「`target` は借用なので `Copy` でなくてよい」。`contains` も `Copy` を要しない）を「呼び出し側が持ち続ける」に、`Array.random_sample_by` の範囲 `0 .. source.length` を `0 ..= source.length` に直した。
  - 直していないもの: (1) `Array.zip`・`zip3`・`unzip`・`unzip3` の署名に `Copy<'a>` がないが、`Copy` でない要素（`string`）は使う側で `E1005`（`no instance for Copy<string>`）になる。署名に制約を足すのは公開 API の署名の変更なので、別のチケットで扱う。(2) `List.iter`・`iteri`・`mapi`・`fold`・`fold_back` の署名の `Copy<'a>` は edition 2026（`_owned`）の形で、2027 は借用の形に読み替わる（`list.md` には書いてある）。(3) 補間 `$"{x}"` は `x: ref i32` で `E1005`（`ref string` は通る）。(4) パイプラインで配列リテラルを借用の引数に渡す（`Vec.of_array [1, 2] |> Vec.add_range [3, 4]`）と `E1013`（`let` で束縛すれば通る）。
- **レビュー（PR #70）**: (1) 例はパッケージのコードなので、パッケージの `[capabilities]`（E15）で縛る。例の解析の結果に `Project::check_capabilities` の `E1036` を含め、`check`・`run=`・`test` の例は失敗し、`fails=E1036` で確かめられる。(2) `/// ``` tsuzuri run=1`（バッククォートと言語の間の空白）を `scan` は受け付けるのに、前段の絞り込みと `render_doc` の高速経路が ```` ```tsuzuri ```` の連なりしか見ず、例を 0 件として素通りしていた。絞り込みを「```` ``` ```` と `tsuzuri` を含む」にした（`scan` が正）。(3) `test` の例に `--backend`・`--allocator` を渡す。(4) マニフェストのないプロジェクトの `--edition`（A22）を、作り直した例のプロジェクトにも当てる。テストは `tests/doctest.rs` の 3 件（権限・edition・空白のある開き）。
- **レビュー 2 回目（PR #70）**: (1) 例の警告は無視するが、`--deny-warnings` のときは、例の `Main` に出た最初の警告（`--warn implicit-copy`・`canonical`・`untrusted-extern` で有効にしたものを含む）で例を失敗にする（パッケージ自身の警告は、例を走らせる前に `check` と同じく報告・拒否される）。`doctest::Options.warnings`。テスト `deny_warnings_fails_an_example_with_a_warning`。(2) `--doc` と `--contracts` の併用は `E2000`（`--doc does not use --contracts; examples check contracts as run does`）。例の契約は `run` と同じく検査する。
