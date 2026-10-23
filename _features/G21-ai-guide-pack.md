# G21: AI ガイドパック（`tsuzuri guide`、`tsuzuri new` の `AGENTS.md`・`llms.txt`）

| 項目 | 内容 |
| --- | --- |
| ID | G21 |
| 優先度 | P1 |
| 規模 | 全体 L（D-50 の表は M。L1 の本文と Phase 3 の std 索引を詳細化して見積もりを上げた）。Phase 1 M、Phase 2 S、Phase 3 M、Phase 4 S |
| 依存 | なし。Phase 4 は [G23](G23-diagnostic-kinds-explain-hints.md) のヒント DB（`hints::TABLE`）が出荷済みなら使う（なければ静的な版。「前提とする他チケットのインターフェース」） |
| 後続 | G23（落とし穴の表をガイドの生成元にする）、[G24](G24-tsuzuri-fix.md)・A22・A18・A19（出荷するとき `guide/card.md` の行を足す・替える。[付録 H](G21-appendix.md#付録-h-他チケットへの依頼guide-の行の差し替え)）、G29（`tz_guide` が `guide::render` を呼ぶ）、G31（ガイドの静的なサイズ上限をテストで強制する）、G32（リポジトリの `AGENTS.md` に「AI エージェントへ」の節を足す案の手本） |
| 状態 | todo |
| 起票 | 2026-10-18（D-50。HEAD `a2313d19` で確認） |
| 承認 | 承認済み（D-50: 人間の包括許可 2026-10-18）。決定事項 D1〜D15 はすべて承認済み |
| 改善する劣位 | 学習データにない言語を AI が最初の 1 回で書けない。言語リファレンスは日本語で 111 ページ・1,081,198 字（`_tsuzuri`）、`docs/language.md` は 350,810 字、README は 65,435 字あり、AI 向けの短い英語の入口がない。`tsuzuri new` は `AGENTS.md` も `llms.txt` も作らない。D-50 の調査結論 (3)（同じ意味に複数の書き方があり、落とし穴が多い）と (4)（文書が大きすぎる）に当たる |
| 手本にする既存実装 | サブコマンドの引数解析と usage の `E2000`: `src/main.rs` の `new_project`・`api_diff_command`・`api_diff_usage`。ファイルを作る: `src/package.rs` の `create_project`（`create_new(true)`）。埋め込み: `src/stdlib.rs` の `SOURCES`（`include_str!("../std/…")`）。宣言の 1 行描画: `src/docgen.rs` の `signature`・`type_text`・`render_declarations`。例の抽出と実行: `scripts/check-docs.mjs`。JSON の文字列: `src/diagnostic.rs` の `json_string`。子プロセスで CLI を試験する: `tests/editions.rs` の `tsuzuri`・`root` の補助関数、`tests/api_diff.rs` |
| 主な影響ファイル | Phase 1: `guide/card.md`・`guide/language.md`・`guide/ownership.md`・`guide/pitfalls.md`・`guide/workflow.md`・`src/guide.rs`・`scripts/check-guide.mjs`・`tests/guide.rs`・`tests/fixtures/guide/pitfalls.txt`・`tests/fixtures/guide_broken/card.md`（すべて新規）、`src/lib.rs`、`src/main.rs`（`HELP`・`main`・`guide_command`）。Phase 2: `guide/AGENTS.md.tmpl`・`guide/llms.txt.tmpl`（新規）、`src/package.rs`（`create_project` とそのテスト）、`src/main.rs`（`new_project`）、`tests/editions.rs`。Phase 3: `src/guide_std.rs`（新規）、`src/guide.rs`、`src/main.rs`、`guide/card.md`。Phase 4: `src/guide.rs`、`src/main.rs`。文書: `README.md`・`docs/language.md`・`docs/architecture.md`・`docs/stability.md`・`_tsuzuri/language-reference/compiler/usage.md`・`organizing-tsuzuri/packages.md`・`languages/how-about-tsuzuri.md`・`vsc/README.md` |

## 実装者向け早見

- **作るもの**: コンパイラ自身が出す、英語の短いガイド。`tsuzuri guide`（L0。6,000 字以内）、`--level 1`（4 章。20,000 字以内）、`--topic NAME`、`--list`、`--json`。`tsuzuri new` が `AGENTS.md`・`llms.txt` も書く（`--no-agent-files` で書かない）。既存プロジェクトには `tsuzuri guide --agents`・`--llms-txt`。Phase 3 は std の索引（`tsuzuri guide std [MODULE]`）、Phase 4 は edition の切り替えと生成節。
- **最初に読む**: [付録 A〜C](G21-appendix.md)（`guide/` に**そのまま置く**ファイルの全文。検証済み）→ 本チケットの「仕様」→「設計」→「実装手順」。コード: `src/main.rs` の `new_project`・`api_diff_command`、`src/package.rs` の `create_project`、`src/stdlib.rs` の `SOURCES`。GUIDE の D-55・D-56（名前とサイズ）。
- **最初に書くテスト**: 手順 3 の `tests/guide.rs::the_guide_prints_level_zero_by_default`（`tsuzuri guide` の stdout が `guide/card.md` と 1 byte も違わない）。その前に手順 2 で `scripts/check-guide.mjs` が付録 A の 2 つの例を通すことを確かめる。
- **触ってはいけない**: 型検査・IR・ABI・診断、`check`／`build`／`run`／`test`／`fmt`／`doc` の出力と終了コード、`tsuzuri new` が作る既存 3 ファイル（`Tsuzuri.toml`・`Main.tz`・`.gitignore`）の内容、`scripts/check-docs.mjs`（変えない。D7）、`Edition` の定数、`Cargo.toml`（crate・`build.rs` を足さない）。`_tsuzuri/` の既存の記述は直さず、足すだけ。
- **停止の目印**: ガイドの例が HEAD で通らない（HEAD が付録の前提から動いた）。`card.md` が 6,000 字に収まらない。既存テストの期待値を変えないと通らない（`package.rs` の `created.len()` の 3→5 を除く）。`unsafe`・新しい crate・`build.rs` が要る。
- **Phase 1 の完了**: `tsuzuri guide` が付録 A を出し、`--level 1`・`--topic`・`--list`・`--json` が仕様どおりで、`node scripts/check-guide.mjs` と `cargo test --locked --test guide` が通る。L1 の 4 章が上限内で、例がすべて検証済み。
- **Phase 2 の完了**: `tsuzuri new demo` が 5 ファイルを作り、`--no-agent-files` なら 3 ファイル（従来と同じ）。`tsuzuri guide --agents`・`--llms-txt` が既存プロジェクトの値で描画する。
- **Phase 3 の完了**: `tsuzuri guide std`・`std Array`・`--topic std-all` が、std のソースと組み込み関数から決定的に作られ、上限（6,000／45,000 字）内。
- **Phase 4 の完了**: `--edition` が edition ブロックを選び、生成節の機構がある（G23 の表が出荷済みなら落とし穴の章を生成物に替える）。
- **新しい名前**（D-55 にあるのは `guide` と `TSUZURI_GUIDE_EXAMPLES` だけ。ほかは「追加が必要な名前」）: サブコマンド `guide`、フラグ `--level`・`--topic`・`--list`・`--agents`・`--llms-txt`・`--edition`（`guide` の中だけ）・`--no-agent-files`（`new` の中だけ）、環境変数 `TSUZURI_GUIDE_EXAMPLES`（`check-guide.mjs` だけが読む）、JSON の schema `tz-guide/1`、診断 `E2000` の kind（`guide_*`）。診断コード・予約語・std・crate の追加はない。

## 目的

学習データにない言語は、AI が最初の 1 回で正しく書けない。リファレンスは日本語で 108 万字あり、小さいモデルの文脈に入らない。このチケットは、**コンパイラのバージョンと一致する短い英語のガイド**を、コンパイラ自身が出すようにする（D-56）。

| 成果物 | 上限（D-56。文字数＝Unicode のコードポイント。テストで強制） | 内容 |
| --- | --- | --- |
| ガイド L0（`guide/card.md`） | 6,000 | 構文の早見・所有権の規則 5 つ・数値の規則・落とし穴・作業の流れ。付録 A に全文（5,429 字） |
| ガイド L1（4 章） | 20,000（合計） | 型クラス・コンピュテーション式・Task・モジュールとパッケージ・std の地図・所有権・落とし穴・作業の流れ。付録 F に章立てと配分 |
| `AGENTS.md` のひな形 | 3,500 | プロジェクトの AI エージェント向けの指示。付録 B に全文（2,348 字） |
| `llms.txt` のひな形 | 1,500 | <https://llmstxt.org> の形の索引。付録 C に全文（1,002 字） |
| std の索引（Phase 3） | 1 モジュール 6,000／全体 45,000 | 1 関数 1 行の署名（`ref`・`ref mut`・値渡しの規約を含む） |

**すべての例とエラーコードは検証する。** 例は `tsuzuri check`（`run=` のものは `-O0`／`-O3` で実行、`test` を含むものは `tsuzuri test`）を通し、落とし穴の表の「誤り → コード」は 29 件の小さなプログラムを `check` してコードを照合する。実行を通らない例は置かない（D-56）。

HEAD で測った数字（`wc -m` はコードポイント数。UTF-8 のロケールで実行する）:

| 対象 | 値 | コマンド |
| --- | --- | --- |
| 言語リファレンス | 111 ページ・1,081,198 字 | `find _tsuzuri -name '*.md' \| wc -l`、`find _tsuzuri -name '*.md' -exec cat {} + \| wc -m` |
| `docs/language.md`・`README.md` | 350,810 字・65,435 字 | `wc -m docs/language.md README.md` |
| 本チケットの L0・AGENTS・llms | 5,429 字・2,348 字・1,002 字 | `wc -m target/tz-G21/guide/*`（付録 A〜C と同じ内容） |
| `tsuzuri new` の既定のファイル | 3 個 | `tsuzuri new <dir>` の出力（付録 D） |

非目標:

- 日本語のガイド、`--lang`。D-56 は AI 向け成果物を英語に固定している（トークン数が少なく、モデルの習熟度が高い）。
- LLM を呼んで文章を作ること。ガイドは書いたファイルと、コンパイラの表・std のソースから決定的に作る。
- ガイドの互換性の保証。文言は直してよい（D-56）。`--json` のキーだけが足すだけの規則に従う。
- 言語リファレンス（日本語）の置き換えや再構成。KB（G27）・MCP（G29）・診断の説明（G23 の `explain`）はこのチケットの対象外。

## 着手条件と停止条件

### 着手条件

- 依存なし。GUIDE §2.3 の基準: `cargo build --release --locked` と `cargo test --locked` を実行し、件数と、失敗があれば自分の変更と無関係であることを記録する。`node scripts/check-docs.mjs _tsuzuri/language-reference/compiler/usage.md` が通る。
- Node.js 20 以降（外部パッケージなし）。`run=` の例の検査には native の LLVM（Clang 17 以降）が要る。
- 手元の `run` は専用のキャッシュで速くなる: `export TSUZURI_CACHE_DIR=$PWD/target/guide-cache`（共有のキャッシュが他のプロセスにロックされると 1 分以上待つ。付録 D）。`guide-cache` は `.gitignore` の `target` の下なのでコミットされない。
- 読む: GUIDE の D-55（名前の割り当て）・D-56（ガイドの言語とサイズ）・§8（文書更新）、`docs/stability.md`。本チケットの付録 A〜C。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と選択肢を報告する（GUIDE §13）。

1. 付録 A の例が HEAD のコンパイラで `check`／`run` を通らない、または付録 E の表のコードが HEAD と違う。選択肢: A（HEAD が変わっただけなら、例とコードを HEAD に合わせて付録と `guide/` を直す。意味が変わっていたら B）／B（変更の理由を報告して指示を待つ）。
2. `guide/card.md` が 6,000 字に収まらない（行を足す・例を直すとき）。選択肢: 優先度の低い落とし穴の行を L1 へ移す。**上限を上げない**（D-56 の変更は承認が要る）。
3. `scripts/check-docs.mjs` を変えないと `check-guide.mjs` が書けないように見える。選択肢: A（fence の抽出を `check-guide.mjs` へ写す。決定 D7）／B（共通ライブラリへ切り出す。ドキュメントの検査の関門に触れるので承認が要る）。A を使う。
4. 新しい crate・`build.rs`・`unsafe` が必要に見える。`include_str!` と標準ライブラリだけで書ける。
5. 既存テストの期待値を変えないと通らない。例外は `src/package.rs` の `create_project` のテストの `created.len()`（3 → 5）と呼び出しの引数の追加だけ。
6. `tsuzuri new` の既存 3 ファイルの内容や、終了コードを変える必要が出た。Phase 2 は**足すだけ**。
7. Phase 3: `Builtin` の型（`BuiltinType`）を文字列にする関数で、新しい variant を `_ =>` で受けたくなった。全 variant を wildcard なしで書く（新しい variant で壊れるのは意図どおり）。書けないなら停止。
8. Phase 3: std の索引が上限（6,000 字・45,000 字）を、仕様の溢れの規則でも収められない。選択肢: A（行を省く規則を足す。承認が要る）／B（D-56 の上限を上げる。承認が要る）。
9. Phase 4: `Edition::SUPPORTED` が 2 つ以上になっている（A22 か A18 が出荷済み）。edition ブロックに入れる内容は付録 H の行に従う。付録 H にない差が見つかったら停止。
10. 同じ失敗を直す試みが 2 回続けて失敗した。

## 現状（HEAD `a2313d19` で確認）

確認した作業ツリーは `9dd0fbbc`（`a2313d19` との差は `docs/benchmarks.md` の 41 行の削除だけで、コードは同じ）。`target/release/tsuzuri` で実行した。

- **サブコマンドの分岐**: `src/main.rs` の `main` は、`script` の引数の取り分けのあと、`--help`・`--version`・`toolchain info`・`new`・`fetch`・`publish`・`api-diff`・`bindgen`・`repl` を先頭の語で振り分け、残りを `parse_arguments`（`check`・`doc`・`fmt`・`test`・`bench`・`run`・`build` と、`build` の省略形）に渡す。`guide` はない。
- **`tsuzuri guide` の現状**（`build` の省略形として、ディレクトリ `guide` を探す）:

```text
$ tsuzuri guide
guide:1:1: error[E2001]: cannot inspect source 'guide': No such file or directory (os error 2)
（終了コード 1）
```

- **`tsuzuri new`**: `src/main.rs` の `new_project` が `--namespace NAME` を読み、`src/package.rs` の `create_project(directory, namespace)` を呼ぶ。3 ファイルを `OpenOptions::create_new(true)` で作り、`Created <path>` を 1 行ずつ出す。ひな形は `Tsuzuri.toml`（`[package]` の `name`・`version`・`edition`・`namespace`）・`Main.tz`（`namespace` の宣言、`main`、`test "adds numbers"`）・`.gitignore`（`.tsuzuri/`）。実出力は付録 D（`check` は無出力で終了コード 0、`run` は `Hello, Tsuzuri!`、`test` は 1 件通る）。
- **`create_project` の呼び出しとテスト**: `src/main.rs`（1 か所）と `src/package.rs` のテスト（`create_project(...)` が 5 か所: `hello-world`・再実行・`other`・不正な名前 4 件）。テストは `created.len() == 3` と `created[0]`（`Tsuzuri.toml`）・`created[1]`（`Main.tz`）を使う。ほかに `tests/editions.rs::new_projects_declare_the_latest_edition`（CLI を呼ぶ）、`scripts/toolchain/smoke.mjs`（`new` したディレクトリを `build`）、`vsc/src/workflow.ts`（`new` を呼んでフォルダーを開く）。**どれも余分なファイルがあっても壊れない**（`.tz`・`.tt`・`.tc` 以外はソースとして読まれない）。
- **埋め込みの手本**: `src/stdlib.rs` の `pub const SOURCES: &[(&str, &str)]` は 47 個の `("std/Name.tz", include_str!("../std/Name.tz"))`。`RESERVED_MODULES` は 53 語、`OPT_IN` は 15 モジュール。`Cargo.toml` に `include`・`build.rs` はなく、依存は `rustc_apfloat`・`serde_json`（と Windows の `same-file`）だけ。
- **宣言の 1 行描画**: `src/docgen.rs` の `signature(prefix, name, regions, constraints, parameters, result)` と `type_text(&TypeExpr)` は `pub(crate)`。`&T` と `ref T` は構文では同じ `Reference` なので、`type_text` は常に `ref T`（`ref mut T`）で出す。`Program` は `functions`・`externs`・`extern_types`・`records`・`unions`・`type_aliases`・`constants`・`classes` を持ち、各宣言に `visibility`（`Visibility::Private`）がある。
- **組み込み関数**: `src/check.rs` の `Builtin::ALL` は 246 個（`#[cfg(test)]` の 3 個を含む）、`Builtin::name()` と `Builtin::scheme()`（`BuiltinScheme { parameters, result, variables, constraints }`）がある。`BuiltinType` は 17 variant（`Var`・`Concrete`・`Std`・`Array`・`ArrayView`・`List`・`Vec`・`Tuple`・`Task`・`Shared`・`Reference`・`Function`・`UnsignedOf`・`WidenOf`・`SimdLane`・`SimdMask`・`FixedArrayOf`）。**文字列への描画はまだない**。利用者向けは 193 個（付録 I）。
- **例の検査**: `scripts/check-docs.mjs`（115 行）は `_tsuzuri` の全ページを検査して終わるトップレベルのスクリプトで、**関数を export していない**。fence の書式は ` ```tsuzuri [project=P] [file=F] [run=R] `（`run` は `decodeURIComponent` した文字列が `run` の出力の trim と一致。`test "` を含めば `tsuzuri test`）。CI には入っていない（README が手で実行する検査として書く）。
- **診断の出力**: `check --json` は診断を **stderr** に 1 行 1 JSON（`severity`・`code`・`message`・`path`・`span`）で出し、`test --json` は stdout に 1 行 1 JSON。作業ディレクトリで `check .` とすると `path` は `./Main.tz`（作業ディレクトリの名前が出ない）。例は付録 D。
- **edition**: `Edition::parse`・`supported_names`・`LATEST`・`SUPPORTED`（`src/syntax.rs`）。`SUPPORTED` は `[2026]` だけ。
- **ガイドが引く落とし穴**: GUIDE §12 の表（`pair.0` ほか）。HEAD で 29 件を `check` して、コードがすべて一致した（付録 E）。

## 変えてはいけない意味

- `tsuzuri new <dir>` が作る `Tsuzuri.toml`・`Main.tz`・`.gitignore` の内容と、`Created …` の最初の 3 行。Phase 2 はその**あとに** `AGENTS.md`・`llms.txt` を足す。`--no-agent-files` を付けると出力は従来と 1 byte も変わらない。
- `check`・`build`・`run`・`test`・`fmt`・`doc`・`fetch`・`publish`・`api-diff`・`bindgen`・`repl`・`script` の出力・終了コード・診断・生成物（IR・object・`.wasm` は 1 byte も変わらない）。`--json` の既存の 5 キー。
- `tsuzuri [build] <path>` は、パスが `guide` のときだけ意味が変わる（`tsuzuri guide` はサブコマンド）。`tsuzuri build guide` と `tsuzuri check guide` は従来どおり。
- ガイドが「今のコンパイラの振る舞い」だけを書くこと。計画中の機能（`tsuzuri explain`・`fix`・`..<` ほか）を、出荷前に書かない（`tests/guide.rs::the_guide_names_only_existing_subcommands` が強制する）。
- `docs/stability.md` の edition の規則。ガイドは**全 edition が受理する書き方（正準形）だけ**を教える（D-54）。

## Phase 分割

| Phase | 内容 | 規模 | 前提 | 単独で出荷できる理由 |
| --- | --- | --- | --- | --- |
| 1 | `guide/`（L0 と L1 の 4 章）、`src/guide.rs`、`tsuzuri guide [--level\|--topic\|--list\|--json]`、`scripts/check-guide.mjs`、`tests/guide.rs`、落とし穴の検証表 | M | なし | 新しいサブコマンドと新しいファイルだけ。既存の挙動を変えない |
| 2 | `guide/AGENTS.md.tmpl`・`llms.txt.tmpl`、`tsuzuri new` が 5 ファイルを作る（`--no-agent-files`）、`tsuzuri guide --agents`・`--llms-txt` | S | Phase 1 | Phase 1 だけでも `tsuzuri guide` は完結する。`new` は足すだけ |
| 3 | std の索引: `tsuzuri guide std [MODULE]`・`--topic std-all`（`src/guide_std.rs`） | M | Phase 1 | 新しいトピックを足すだけ。Phase 2 を待たない |
| 4 | edition ブロックと `--edition`、生成節の機構（G23 の表が出荷済みなら落とし穴の章を生成物へ） | S | Phase 1（G23 は任意） | `--edition` を付けない出力は Phase 3 までと同じ |

Phase 1 の手順 2〜3（インフラと L0）で `tsuzuri guide` が使える状態になり、手順 4〜7（L1 の 4 章）は 1 章ずつ検証して進める。途中の状態では `CHAPTERS` に登録した章だけが出る（L1 の章が 1 つもないあいだ `--level 1` は `E2000`）。マージは Phase 1 の全手順のあと。

## 仕様

### ファイルと埋め込み

`guide/` は新しいトップレベルのディレクトリ。`src/guide.rs` が `include_str!("../guide/<file>")` で埋め込む（実行時にファイルを読まない。バイナリ単体で動く）。

| ファイル | 役割 | 上限（字） | Phase |
| --- | --- | --- | --- |
| `guide/card.md` | L0。トピック名 `card` | 6,000 | 1（付録 A のとおり） |
| `guide/language.md` | L1 の第 1 章。トピック名 `language` | 8,500 | 1（付録 F） |
| `guide/ownership.md` | L1 の第 2 章。`ownership` | 4,500 | 1 |
| `guide/pitfalls.md` | L1 の第 3 章。`pitfalls` | 4,000 | 1 |
| `guide/workflow.md` | L1 の第 4 章。`workflow` | 2,990 | 1 |
| `guide/AGENTS.md.tmpl` | `AGENTS.md` のひな形 | 3,500 | 2（付録 B） |
| `guide/llms.txt.tmpl` | `llms.txt` のひな形 | 1,500 | 2（付録 C） |

- 4 章の上限の和は 19,990。L1 の出力は 4 章を上の順に `"\n"` で連結したもの（各ファイルの末尾は `"\n"` なので、章の間に空行が 1 行入る。連結の改行 3 字を足しても 19,993 字で、D-56 の L1 の 20,000 字以内）。
- すべて **ASCII だけ**（`str::is_ascii`）。改行は LF。`include_str!` の文字列に `\r\n` があれば（Windows の autocrlf）、出力の前に `\n` へ直す（`normalized`）。サイズは直したあとの文字数。
- 例（` ```tsuzuri ` の fence）は `scripts/check-docs.mjs` と同じ書式（`project=`・`file=`・`run=`。`run` の値は URL エンコード）。fence の外のコードは `` `...` `` の中に書く。

### CLI

```text
tsuzuri guide [--level 0|1] [--topic NAME] [--list] [--json] [--edition YEAR]
tsuzuri guide --agents [DIR]
tsuzuri guide --llms-txt [DIR]
tsuzuri guide std [MODULE]                       （Phase 3）
tsuzuri new directory [--namespace NAME] [--no-agent-files]   （Phase 2。既存のコマンドに足す）
```

引数の解析は `guide::parse_arguments(&[String]) -> Result<Invocation, String>`（純関数。単体テストする）。`--agents`・`--llms-txt` の直後の引数が `-` で始まらなければ、それを `DIR` とする（既定は `.`）。`std` は最初の位置引数のときだけ。同じフラグが 2 回、未知のフラグは `E2000`。

| 入力 | 動作 |
| --- | --- |
| （引数なし） | L0（`card`）を stdout に出す。終了コード 0 |
| `--level 0` | 同上 |
| `--level 1` | L1 の 4 章を連結して出す |
| `--level` の値が 0・1 以外、または値がない | `E2000`（`guide_bad_arguments`）、終了コード 2 |
| `--topic NAME` | 章 `NAME` を出す。`NAME` は `card`・`language`・`ownership`・`pitfalls`・`workflow`（Phase 3 から `std`・`std-all`）。未知は `E2000`（`guide_unknown_topic`） |
| `--level` と `--topic` を両方 | `E2000`（`guide_bad_arguments`） |
| `--list` | トピックの一覧（下の書式）。併用できるのは `--json` だけ |
| `--json` | 出力を 1 行の JSON に包む（下のキー） |
| `--edition YEAR` | edition ブロックを選ぶ（Phase 4。`Edition::parse` が受理する値だけ）。Phase 1〜3 では未知のフラグとして `E2000` |
| `--agents [DIR]`・`--llms-txt [DIR]`（Phase 2） | `DIR` の `Tsuzuri.toml` の値でひな形を描画して stdout に出す。ほかのフラグとは併用できない（`--json` も不可） |
| `std`・`std MODULE`（Phase 3） | std の索引（下）。`--json` だけ併用できる |

終了コード: 0 = 出力した。1 = `--agents`・`--llms-txt` で `DIR` の `Tsuzuri.toml` を読めない。2 = 引数の誤り（`E2000`）。`--json` が引数にあれば、誤りの診断も JSON（stderr）で出す（`api-diff` と同じ）。

### 出力の形式

**テキスト**（既定）: 選んだ章の `normalized` なテキストをそのまま stdout へ（`print!`。余分な改行・ヘッダー・色を足さない）。`tsuzuri guide` の stdout は `guide/card.md` と 1 byte も違わない。

**`--list` のテキスト**: トピックごとに 1 行 `format!("{:<10} L{} {:>6}  {}\n", name, level, chars, title)`。`chars` は出力の文字数。順序は `card`・`language`・`ownership`・`pitfalls`・`workflow`（Phase 3 から `std`・`std-all`）。`title` は次の表。

| name | title |
| --- | --- |
| `card` | `Language card: syntax, numbers, ownership, pitfalls, workflow` |
| `language` | `Language: modules, declarations, patterns, type classes, builders, tasks, std map` |
| `ownership` | `Ownership: moves, borrows, lifetimes, sharing, performance` |
| `pitfalls` | `Pitfalls: wrong form, error code, right form` |
| `workflow` | `Workflow: commands, diagnostics, tests, debugging` |
| `std`（Phase 3） | `Std modules with function counts` |
| `std-all`（Phase 3） | `Std index: every function, one line each` |

**`--json`**（schema `tz-guide/1`。キーは**この順序**。足すだけの変更で番号は保つ。文字列は `diagnostic::json_string`）:

| コマンド | 出力（stdout、1 行） |
| --- | --- |
| `--topic NAME`・`--level 0`・既定 | `{"schema":"tz-guide/1","compiler":"0.1.0","edition":"2026","level":0,"topic":"card","title":"…","chars":5429,"text":"…"}`。`compiler` は `env!("CARGO_PKG_VERSION")`、`edition` は `--edition` か `Edition::LATEST`、`level` は章の level（整数）、`chars` は `text` の文字数 |
| `--level 1` | 同じキー。`"level":1`、`"topic":null`、`"title":"Guide level 1: language, ownership, pitfalls, workflow"` |
| `--list` | `{"schema":"tz-guide/1","compiler":"0.1.0","topics":[{"name":"card","level":0,"chars":5429,"title":"…"}, …]}` |
| `std MODULE`（Phase 3） | `topic` は `"std:MODULE"`、`level` は 1 |

**例**（`tsuzuri guide --list`。字数は L1 の本文を書いたあとの値で変わる）:

```text
card       L0   5429  Language card: syntax, numbers, ownership, pitfalls, workflow
language   L1   8310  Language: modules, declarations, patterns, type classes, builders, tasks, std map
```

### テンプレートと `tsuzuri new`（Phase 2）

- 置換するのは `{{name}}`・`{{namespace}}`・`{{edition}}`・`{{version}}` の 4 語だけ（`str::replace` を、この順に 1 回ずつ）。値: `name` = マニフェストの `name`、`namespace` = マニフェストの `namespace`、`edition` = `Edition` の `Display`（`2026`）、`version` = `env!("CARGO_PKG_VERSION")`。値は検証済みの識別子・数字だけなので（`create_project` が `valid_namespace`・`package_name` を通す）、`{{` を含まない。描画後に `{{` が残らないことをテストで確かめる。
- `tsuzuri new` は 3 ファイルの**あとに** `AGENTS.md`・`llms.txt`（この順）を作り、`Created <path>` を 5 行出す。`--no-agent-files` は 1 回だけ指定でき、指定すると 3 ファイル・3 行（従来どおり）。`create_project(directory, namespace, agent_files: bool)`。返す `Vec<PathBuf>` の先頭 3 つは従来と同じ順序。
- `tsuzuri guide --agents [DIR]`・`--llms-txt [DIR]`: `DIR/Tsuzuri.toml` を `driver::read_manifest` で読み、`Manifest` の `name`・`namespace`・`edition` で描画して出す。ファイルは書かない（利用者が `> AGENTS.md` とする）。`Tsuzuri.toml` がなければ `E2000`（`guide_manifest_missing`）、終了コード 1。

### std の索引（Phase 3）

`tsuzuri guide std` はモジュール一覧、`tsuzuri guide std MODULE` は 1 モジュールの索引、`--topic std-all` は全体。元データは (1) `stdlib::SOURCES` を `parser::parse_with_source` で読んだ公開の宣言と、(2) `Builtin::ALL` のうち利用者向けの 193 個（`Vec.push`・`Int.abs`・`Rc.new` など最頻の関数は組み込み）。**規則の全文は [付録 J](G21-appendix.md#付録-j-std-索引phase-3の規則)**。要点:

- 関数 1 つが 1 行 `Module.name :: <署名>`（例: `Array.fold :: Copy<'a> => ('state -> 'a -> 'state) -> 'state -> ref ['a] -> 'state`）。署名は `ref`・`ref mut`・値渡しで借用の規約を示す（`&T` は `ref T` に正規化される）。名前の昇順。`private` のもの、名前が大文字で始まるもの（ビルダーの演算）、名前が `__` で始まるものを除く。組み込み関数の署名は `builtin_signature`（`BuiltinType` の 17 variant を wildcard なしで描画）。
- `std MODULE` のページは 6,000 字以内（超えたら末尾を `... N more: tsuzuri guide --topic std-all` に替える）。`std` は 1 モジュール 1 行（`Json  38 functions  (opt-in)`）。`std-all` は 45,000 字以内で、溢れたら opt-in モジュールを大きい順に 1 行へ畳む。
- 不明なモジュール名は `E2000`（`guide_unknown_std_module`）。

### edition ブロックと生成節（Phase 4）

- ガイドのファイルは、行だけの `<!--if edition=YYYY-->` と `<!--endif-->` で囲んだブロックを持てる（入れ子にしない）。出力の前に `filter_edition(text, edition)`: `edition` の文字列と一致するブロックだけ残し（マーカーの行は消す）、ほかは消す。マーカーの不整合（入れ子・対応のない `endif`・閉じ忘れ）は、ガイドのファイルの誤りなので `tests/guide.rs::every_guide_file_filters_for_every_supported_edition` が失敗する（実行時は `Err(String)` を `E2000` にしない。出力に `Err` が出るのは開発中の誤りだけ）。
- `--edition YEAR` の既定: カレントディレクトリの `Tsuzuri.toml` があればその `edition`、なければ `Edition::LATEST`。`Edition::parse` が `None` なら `E2000`（`guide_edition_unsupported`）。**Phase 4 時点で edition ブロックを持つファイルはない**（`SUPPORTED` は `[2026]` だけ）。A22 などが 2 つ目の edition を足したとき、付録 H の行を edition ブロックにして足す。
- 生成節: G35 の対のマーカー（行だけの `<!-- generated:NAME -->` と `<!-- /generated:NAME -->`）の間の**本体**を、`generator(NAME)` の戻り値（`"\n"` で終わる文字列）に置き換え、マーカー行は出力に残さない。NAME が未知・対でないならガイドのファイルの誤り（テストが失敗）。**Phase 4 は機構だけを出荷し、`guide/` のファイルにマーカーを置かない**（G23 の `hints::TABLE` が出荷されたとき、G23 の実装者が `pitfalls` の生成器を足し、`pitfalls.md` の表を対のマーカーに替える。付録 H）。

### 診断（このチケットが出すもの）

すべて `E2000`（CLI の使い方の誤り）。メッセージは英語・小文字始まり・ピリオドなし。スパンは `Span::default()`、パスは `<command line>`（`api-diff` と同じ）。kind は G22 の `Diagnostic::with_kind` が main にあるときだけ付ける（`grep -n "fn with_kind" src/diagnostic.rs`。なければ付けない）。

| 条件 | メッセージ | kind | 終了コード |
| --- | --- | --- | --- |
| 未知のフラグ | `unknown option '{option}' for guide; supported: --level, --topic, --list, --json, --agents, --llms-txt` | `guide_bad_arguments` | 2 |
| 同じフラグが 2 回 | `{flag} specified more than once` | `guide_bad_arguments` | 2 |
| `--level` の値がない・不正 | `--level needs 0 or 1` | `guide_bad_arguments` | 2 |
| `--topic` の値がない | `--topic needs a name; run 'tsuzuri guide --list' for the names` | `guide_bad_arguments` | 2 |
| `--level` と `--topic` | `--level and --topic cannot be combined` | `guide_bad_arguments` | 2 |
| 併用できないフラグ | `{flag} cannot be combined with {other}` | `guide_bad_arguments` | 2 |
| 未知の topic | `unknown guide topic '{name}'; run 'tsuzuri guide --list' for the names` | `guide_unknown_topic` | 2 |
| L1 の章がない（開発中だけ） | `this build has no level 1 guide` | `guide_unknown_topic` | 2 |
| `--agents`・`--llms-txt` の `Tsuzuri.toml` がない・読めない | `no readable Tsuzuri.toml in '{dir}'; run it in a project directory` | `guide_manifest_missing` | 1 |
| `--edition` が未対応（Phase 4。Phase 1〜3 は未知のフラグ） | `unsupported edition '{value}'; supported editions: {Edition::supported_names()}` | `guide_edition_unsupported` | 2 |
| 未知の std モジュール（Phase 3） | `unknown std module '{name}'; run 'tsuzuri guide std' for the list` | `guide_unknown_std_module` | 2 |

例（3 つ以上）:

```text
$ tsuzuri guide --level 2
<command line>: error[E2000]: --level needs 0 or 1
（終了コード 2）
$ tsuzuri guide --topic stdd
<command line>: error[E2000]: unknown guide topic 'stdd'; run 'tsuzuri guide --list' for the names
（終了コード 2）
$ cd /empty-dir && tsuzuri guide --agents
<command line>: error[E2000]: no readable Tsuzuri.toml in '.'; run it in a project directory
（終了コード 1）
```

人間向けの表示は `print_diagnostic` が決める（上の 1 行目は `api-diff` の usage の出力と同じ形になる。実装後の実出力で `tests/guide.rs` はコードと終了コードだけを照合し、全文は固定しない）。

### 上限・決定性・互換

- 上限はこのチケットが決める数ではなく D-56 のもの（上の表）。テストが強制する。`std` の索引は規則で収める。
- 出力は決定的: 時刻・絶対パス・乱数・`HashMap` の反復順を入れない。std の索引は `BTreeMap` と明示の整列だけ。同じコンパイラは同じ出力を返す。
- **互換**（`docs/stability.md`）: `guide` の本文・`AGENTS.md`・`llms.txt` のひな形の文言は保証の対象外（D-56）。サブコマンド名・フラグ名・`--json` のキー（足すだけ）・終了コードは保つ。edition の規則に触れない（ガイドは正準形だけを教える）。IR・ABI・WASM の import・既存の診断コードの意味は変えない。
- Windows: パスの `\` は出力に出ない。`include_str!` の CRLF は `normalized` で吸収する。

## 設計

### 新規・変更するファイル

| ファイル | 変更 | Phase |
| --- | --- | --- |
| `guide/*.md`・`guide/*.tmpl`（新規。上の表） | 付録 A〜C は全文をそのまま置く。L1 の 4 章は付録 F に従って書く | 1・2 |
| `src/guide.rs`（新規） | 章の表・描画・引数の解析・テンプレート・JSON・（Phase 4）edition と生成節 | 1〜4 |
| `src/guide_std.rs`（新規） | std の索引（収集・組み込みの署名・整形・溢れの規則） | 3 |
| `src/lib.rs` | `pub mod guide;`（`gpu_devices` の次、アルファベット順）、Phase 3 で `mod guide_std;`（非公開。`guide` が呼ぶ） | 1・3 |
| `src/main.rs` | `HELP` に `guide` の行と段落、`main` に `guide` の分岐、`guide_command`（新規）、Phase 2 で `new_project` に `--no-agent-files` | 1・2 |
| `src/package.rs` | `create_project` に `agent_files: bool`、`{3 ファイル}` の後に 2 ファイル、テストの更新 | 2 |
| `scripts/check-guide.mjs`（新規） | 例の検査・サイズ・`trap` の主張。`check-docs.mjs` は変えない | 1 |
| `tests/guide.rs`・`tests/fixtures/guide/pitfalls.txt`・`tests/fixtures/guide_broken/card.md`（新規）、`tests/editions.rs` | テスト | 1〜4 |

### データ構造（`src/guide.rs`。名前と型を変えない）

```rust
pub struct Chapter { pub name: &'static str, pub level: u8, pub title: &'static str, pub text: &'static str }
pub const CHAPTERS: &[Chapter] = &[
    Chapter { name: "card", level: 0, title: "Language card: syntax, numbers, ownership, pitfalls, workflow", text: include_str!("../guide/card.md") },
    // 手順 4〜7 で language・ownership・pitfalls・workflow を、上の title の表のとおりに足す
];
pub const AGENTS_TEMPLATE: &str = include_str!("../guide/AGENTS.md.tmpl");   // Phase 2
pub const LLMS_TEMPLATE: &str = include_str!("../guide/llms.txt.tmpl");      // Phase 2
pub const LIMIT_L0: usize = 6_000;  pub const LIMIT_L1: usize = 20_000;
pub const LIMIT_AGENTS: usize = 3_500;  pub const LIMIT_LLMS: usize = 1_500;
pub const LIMIT_STD_MODULE: usize = 6_000;  pub const LIMIT_STD_ALL: usize = 45_000;
pub const CHAPTER_LIMITS: &[(&str, usize)] = &[("language", 8_500), ("ownership", 4_500), ("pitfalls", 4_000), ("workflow", 2_990)];

pub enum Action { Level(u8), Topic(String), List, Agents(String), LlmsTxt(String), Std(Option<String>) }
pub struct Invocation { pub action: Action, pub json: bool, pub edition: Option<String> }
pub struct Rendered { pub level: u8, pub topic: Option<String>, pub title: String, pub text: String }
pub struct Values<'a> { pub name: &'a str, pub namespace: &'a str, pub edition: Edition, pub version: &'a str }

pub fn chars(text: &str) -> usize                                   // text.chars().count()
pub fn parse_arguments(arguments: &[String]) -> Result<Invocation, String>   // 上の CLI の表。Err は E2000 のメッセージ
pub fn render(action: &Action, edition: Edition) -> Result<Rendered, String> // Level・Topic・Std。List・Agents は別の関数
pub fn list_text() -> String;  pub fn list_json() -> String
pub fn json_text(rendered: &Rendered, edition: Edition) -> String   // 上の JSON のキーの順序
pub fn render_template(template: &str, values: &Values<'_>) -> String
pub fn agents_md(values: &Values<'_>) -> String;  pub fn llms_txt(values: &Values<'_>) -> String
pub fn project_values(directory: &Path) -> Result<(String, String, Edition), Diagnostic>  // name・namespace・edition（driver::read_manifest）
pub fn filter_edition(text: &str, edition: Edition) -> Result<String, String>             // Phase 4
pub fn expand_generated(text: &str, generate: &dyn Fn(&str) -> Option<String>) -> Result<String, String>  // Phase 4
```

`src/guide_std.rs`（Phase 3）: `pub(crate) struct ModuleIndex { pub name: String, pub opt_in: bool, pub types: Vec<String>, pub functions: Vec<String> }`、`pub(crate) fn collect() -> Vec<ModuleIndex>`、`builtin_signature(&BuiltinScheme) -> String`、`render_modules`・`render_module`・`render_all`（上の規則）。

### アルゴリズム

**描画（`render` の Level・Topic）**: ① 章の `text` に `normalized`（`\r\n` → `\n`）。② Phase 4 では `filter_edition`、続けて `expand_generated`。③ `Level(0)` は章 `card`、`Level(1)` は `level == 1` の章を `CHAPTERS` の順に `"\n"` で連結（なければ `Err("this build has no level 1 guide")`）、`Topic(n)` は名前の一致（なければ `Err("unknown guide topic …")`）。④ `Rendered.chars` は呼ぶ側が `chars(&text)` で数える。

**引数の解析（`parse_arguments`）**: ① `"--"` は特別扱いしない（`guide` は位置引数を `std` と `DIR` だけに限る）。② 先頭から 1 語ずつ: `--json`・`--list` は真偽（2 回で `"{flag} specified more than once"`）。`--level`・`--topic`・`--edition` は次の語を値とする（なければ上の表のメッセージ）。`--agents`・`--llms-txt` は次の語が `-` で始まらなければ値（`DIR`）、なければ `"."`。`std` は最初の位置引数のとき `Action::Std`（次の語が `-` で始まらなければ `Some(module)`）。それ以外の `-` で始まる語は未知のフラグ。③ 組み合わせの検査を上の表の順に: `--level`＋`--topic`、`--list` と `--json` 以外、`--agents`・`--llms-txt` と他のフラグ、`std` と `--level`・`--topic`・`--list`。④ 値の検査: `--level` は `0`・`1`、`--edition` は Phase 4 で `Edition::parse`。⑤ `Action` を決める（何もなければ `Level(0)`）。

**テンプレート（`render_template`）**: `template.replace("{{name}}", name).replace("{{namespace}}", namespace).replace("{{edition}}", &edition.to_string()).replace("{{version}}", version)`。

**std の索引の収集（`collect`）**: ① `BTreeMap<String, ModuleIndex>`。② `for (path, text) in stdlib::SOURCES`: `module = stdlib::module_name(path)`（`None` なら飛ばす）、`program = parser::parse_with_source(text, 0, Edition::LATEST)`（`Err` ならそのモジュールを飛ばして `debug_assert!(false)`。埋め込みの std は毎回読まれるので失敗しない）。`program.functions`・`externs`・`program.classes` の各メソッド・型の宣言を付録 J の規則で行にする（`.tt` のクラスのメソッドも `Text.find :: Text<'s> => ref 's -> ref 's -> Maybe<i64>` のような 1 行になる。D14 の `Text`・F15 の `Monoid` は、この規則で最初から索引に載るので、それらのチケットは G21 に何も足さない）。③ `for b in Builtin::ALL`: `name = b.name()`、除外（`__`・`$`・`cfg(test)` の `Int.test_*`）、`.` で割って `(module, function)`（なければ `("prelude", name)`）、行は `Module.function :: …`（`prelude` は `name :: …`）。④ 各モジュールの `functions` を昇順に整列して重複を除く（`Math.abs` のように、ソースと組み込みに同じ名前があれば組み込みの行を 1 行だけ残す）。

**std-all の溢れの規則（`render_all`）**: ① 全モジュールの節を作り、合計文字数 `total` を数える。② `order` = opt-in のモジュールを（関数の行の文字数の合計の降順、同じなら名前の昇順）、続けて opt-in でないものを同じ順で並べた列。③ `for m in order` で `total <= LIMIT_STD_ALL` になるまで、`m` の節の中身を 1 行 `m: N functions; run 'tsuzuri guide std m'` に置き換え、`total` を引き直す。④ 全部置き換えても超えるなら `debug_assert!` で落とす（実際は約 3,500 字）。

**`scripts/check-guide.mjs`**: ① ガイドのディレクトリ = `process.env.TSUZURI_GUIDE_EXAMPLES ?? <repo>/guide`、コンパイラ = `TSUZURI_BIN ?? <repo>/target/release/tsuzuri`（`check-docs.mjs` と同じ）。② `*.md`・`*.tmpl` を読み、fence を集める（`check-docs.mjs` の 54〜70 行と同じ規則。**写して、コメントに「`check-docs.mjs` から写した。あちらは export がない」と書く**）。③ 各プロジェクトを一時ディレクトリへ書き、`check`、`run=` があれば `-O0`・`-O3` で `run`（trim が一致）、`test "` があれば `test`。④ 付録 G の 3〜5。⑤ 成功の 1 行。**Node 20 で動くこと**（`Map.groupBy`・`Object.groupBy` は Node 21 以降なので使わない）。

## 前提とする他チケットのインターフェース

- **G22**（未実装でよい）: `Diagnostic::with_kind(&'static str)` があれば `guide_*` の kind を付ける（`grep -n "fn with_kind" src/diagnostic.rs`）。なければ kind なし。
- **G23**: `hints::TABLE`（`pub`。`src/hints.rs`）が出荷済みなら、本チケットの Phase 4 の実装者が `expand_generated` の生成器に `pitfalls` を足す（手順 19）。Phase 4 が先で G23 が後なら、G23 の実装者が足す。どちらでも表を読むのは生成器だけで、`guide` の他の部分は表を読まない。`tsuzuri explain`・G24 の `fix`・A22 の `..<`・A18／A19 は、出荷時に `guide/card.md` の行を足す・替える（付録 H）。
- **G29**: `tz_guide` が `tsuzuri::guide::render`・`list_text` を呼ぶ（`pub` にしておく）。G27・G28（KB）は無関係。

## 実装手順

各手順の最後で `cargo build --release --locked`・`cargo test --locked` が緑。Node の検査は `export TSUZURI_CACHE_DIR=$PWD/target/guide-cache` を付けて実行する。

### Phase 1

#### 手順 1: ベースライン

- 変更: なし。
- 内容: `cargo build --release --locked && cargo test --locked` の件数を控える。`target/release/tsuzuri guide; echo $?` が `error[E2001]` と `1` になることを確かめる（現状）。付録 A の 2 つの例を、リポジトリの外の作業ディレクトリの `Main.tz` に書き、`tsuzuri check`・`run -O0`・`run -O3`（`24 55 7` と `5`）・`test` を手で実行して通ることを確かめる。付録 E の 29 件のうち 3〜4 件を、手で `tsuzuri check . --json` して付録のコードになることを確かめる。
- 確認: ベースラインの件数を記録。ずれがあれば停止条件 1。

#### 手順 2: 検査スクリプトと L0

- 変更: `scripts/check-guide.mjs`（新規）、`guide/card.md`（新規。付録 A をそのまま）。
- 内容: 上の「`scripts/check-guide.mjs`」の ①〜③・⑤（④の `tsuzuri guide` の出力の長さと `trap` の主張は手順 4 以降で足す。この手順では `guide` サブコマンドがないので、`card.md` のファイルの長さを直接数える）。
- 確認: `node scripts/check-guide.mjs` が `Guide: 2 checked examples, 4 native runs (O0/O3), 1 test projects.` を出して終了コード 0。`wc -m guide/card.md` が 5,429。`LC_ALL=C grep -c '[^ -~]' guide/card.md`（空白〜`~` 以外の文字を含む行の数）が 0。`card.md` の例を 1 文字壊して（たとえば `0` を `0x`）、スクリプトが非 0 で失敗することを確かめてから戻す。

#### 手順 3: テスト先行（`tests/guide.rs`）

- 変更: `tests/guide.rs`（新規）、`tests/fixtures/guide/pitfalls.txt`（付録 E の全文）、`tests/fixtures/guide_broken/card.md`（` ```tsuzuri ` の fence 1 つに型エラーのソース: `def main :: unit -> i32 = \() -> "x"` と末尾の改行）。
- 内容: 「テスト計画」の Phase 1 のテストを書く。補助関数（`root`・`tsuzuri`・`examples`・`repo`）を `tests/editions.rs` から写す。L1 に関するテストは、**ディスクの `guide/` に存在する章のファイルだけ**を期待値に使う（`language`・`ownership`・`pitfalls`・`workflow` の順。1 つもなければ `--level 1` は `E2000`）。章を足す手順 5〜8 のたびに同じテストが通り続ける。
- 確認: `cargo test --locked --test guide` が**失敗する**（`guide` サブコマンドがない）。失敗の理由が `E2001` であることを見る。

#### 手順 4: `tsuzuri guide`（L0・`--list`・`--json`）

- 変更: `src/guide.rs`（新規）、`src/lib.rs`、`src/main.rs`。
- 内容: 「データ構造」のうち Phase 1 の部分（`CHAPTERS` は `card` だけ）を書き、`parse_arguments`・`render`・`list_text`・`list_json`・`json_text`・`chars`・`normalized` を実装する。`main.rs`: `HELP` に `  tsuzuri guide [--level 0|1] [--topic NAME] [--list] [--json]`（`tsuzuri new` の行の次）と、段落 ``\`tsuzuri guide\` prints the language guide for AI agents (about 6 KB at level 0, English; --level 1 for the full guide, --topic NAME for one chapter, --list for the names).``（`tsuzuri new` の段落の次）を足す。`main` の `new` の分岐の次に `guide` の分岐。`guide_command(&[OsString]) -> ExitCode`（`api_diff_command` を手本に）: 文字列に直す → `parse_arguments`（`Err(m)` は `E2000` で終了コード 2）→ 出力して `ExitCode::SUCCESS`。`List` は `list_text`（`--json` なら `list_json`）、ほかは `render` の結果を `print!`（`--json` なら `json_text`）。
- 確認: `cargo test --locked --test guide` が通る（L1 の章がまだないので `--level 1` は `E2000` を期待する分岐）。`cargo test --locked` が手順 1 の件数＋新規で通る。`target/release/tsuzuri guide | head -3` が付録 A の先頭 3 行。

#### 手順 5〜8: L1 の 4 章（1 章ずつ）

- 変更: 手順 5 `guide/language.md`、6 `guide/ownership.md`、7 `guide/pitfalls.md`（と `tests/fixtures/guide/pitfalls.txt` への追加）、8 `guide/workflow.md`。各手順で `src/guide.rs` の `CHAPTERS` に 1 行足す。
- 内容: 付録 F の節・配分・出典に従って書く。1 節ごとに、出典のページを読み、**英語に要約し**、例は `tsuzuri check`／`repl` で確かめてから書く。章ごとの上限（`CHAPTER_LIMITS`）を超えない。ASCII だけ。`pitfalls.md` の項目は、必ず `pitfalls.txt` に検証のプログラムを足す。
- 確認: 各手順で `node scripts/check-guide.mjs`（例がすべて通る）と `cargo test --locked --test guide`（サイズ・ASCII・`--level 1` の連結・`--topic`）が通る。`wc -m guide/<章>.md` が上限以下。

#### 手順 9: 文書と Phase 1 の全体

- 変更: 「ドキュメント」の Phase 1 の行。`scripts/check-guide.mjs` に ④（`tsuzuri guide` の出力の長さ）と `trap` の主張を足す。
- 確認: `node scripts/check-docs.mjs <変更した _tsuzuri のページ>`、`node scripts/check-guide.mjs`、`cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test --locked`。`tsuzuri guide`・`--level 1`・`--topic ownership`・`--list`・`--json` を手で実行して目で確かめる。

### Phase 2

#### 手順 10: テスト先行（ひな形と `new`）

- 変更: `tests/guide.rs`、`src/package.rs` のテスト。
- 内容: 「テスト計画」の Phase 2 のテストを書く。`src/package.rs` のテストの `create_project(...)` を新しい引数（`true`、不正な名前の検査は `true`）で書き、`created.len()` を 5、`created[3]`・`created[4]` が `AGENTS.md`・`llms.txt` と期待する。
- 確認: ビルドが通らない（引数の数）。それを失敗として記録する。

#### 手順 11: ひな形と `create_project`

- 変更: `guide/AGENTS.md.tmpl`・`guide/llms.txt.tmpl`（付録 B・C をそのまま）、`src/guide.rs`（`AGENTS_TEMPLATE`・`LLMS_TEMPLATE`・`Values`・`render_template`・`agents_md`・`llms_txt`）、`src/package.rs`（`create_project`）、`src/main.rs`（`new_project`）。
- 内容: `create_project(directory, namespace, agent_files)`: `files` の配列は従来の 3 つを変えず、`agent_files` のとき `AGENTS.md`・`llms.txt` を `guide::agents_md(&values)`・`guide::llms_txt(&values)` で足す。`values` は `name`・`namespace`（検証済みの値）・`Edition::LATEST`・`env!("CARGO_PKG_VERSION")`。`new_project`: `--no-agent-files` を 1 回だけ受け付け、usage を `usage: tsuzuri new directory [--namespace NAME] [--no-agent-files]` にする。README・`HELP` の `new` の行も同じに直す。
- 確認: `cargo test --locked`（`package.rs` のテスト・`tests/editions.rs`・`tests/guide.rs`）が通る。`tsuzuri new target/guide-try --namespace Acme::Try` が 5 行の `Created` を出し、`target/release/tsuzuri check target/guide-try`・`test` が通る。`--no-agent-files` が 3 行。

#### 手順 12: `--agents`・`--llms-txt`

- 変更: `src/guide.rs`（`project_values`・`Action::Agents`・`Action::LlmsTxt`）、`src/main.rs`。
- 内容: `project_values(dir)`: `driver::read_manifest(&dir.join("Tsuzuri.toml"))` の `Manifest` から `(name, namespace, edition)`。失敗は `E2000`（`guide_manifest_missing`）・終了コード 1。出力は `print!`（ファイルは書かない）。
- 確認: `tests/guide.rs::guide_agents_prints_the_template_for_a_project` ほかが通る。`tsuzuri guide --agents target/guide-try | diff - target/guide-try/AGENTS.md` が差なし。

#### 手順 13: Phase 2 の文書と確認

- 確認: 「ドキュメント」の Phase 2 の行を直し、`node scripts/check-docs.mjs <ページ>`・`cargo test --locked`・`node scripts/check-guide.mjs`。`scripts/toolchain/smoke.mjs` の `new` の部分（`tsuzuri new` したディレクトリを `build` する）が、5 ファイルでも通ることを、`new` → `build` を手で実行して確かめる。

### Phase 3

#### 手順 14: テスト先行と収集

- 変更: `src/guide_std.rs`（新規。まず単体テスト）、`tests/guide.rs`。
- 内容: 「テスト計画」の Phase 3 のテストを書く。続けて `collect` の std ソースの部分（関数・クラスのメソッド・型）を実装する。
- 確認: `cargo test --locked guide_std`（`running N tests` の N を確かめる）。`Array` の `fold`・`sort` が `Array.fold :: Copy<'a> => ('state -> 'a -> 'state) -> 'state -> ref ['a] -> 'state` の形になる。

#### 手順 15: 組み込み関数の署名

- 変更: `src/guide_std.rs`（`builtin_signature`・`BuiltinType` の描画・`collect` の組み込みの部分）。
- 内容: 17 variant を wildcard なしで描画する。`Vec.push`・`Int.abs`・`Math.sqrt`・`Rc.new`・`sqrt`（`prelude`）の行を手で確かめる。
- 確認: 単体テスト `builtin_signatures_render_every_variant`（17 variant を 1 つずつ含む scheme を作って期待の文字列と比べる）。`Vec.push` が `Vec.push :: 'a -> Vec<'a> -> Vec<'a>` になるか、`Builtin::scheme()` の実際の型から手で導いた値と一致する。

#### 手順 16: 描画・溢れの規則・CLI

- 変更: `src/guide_std.rs`、`src/guide.rs`（`Action::Std`・`--topic std`・`std-all`・`list_text` に 2 行）、`src/main.rs`、`guide/card.md`（付録 H の 1 行目）。
- 内容: `render_modules`・`render_module`・`render_all` と溢れの規則。`CHAPTERS` には入れず、`render` の `Topic("std"|"std-all")`・`Action::Std` が `guide_std` を呼ぶ。
- 確認: `tests/guide.rs` の Phase 3 のテストが通る。`tsuzuri guide std | head -5`、`tsuzuri guide std Array | head -8`、`tsuzuri guide --topic std-all | wc -m`（45,000 以下）。

#### 手順 17: Phase 3 の文書と確認

- 確認: 「ドキュメント」の Phase 3 の行、`node scripts/check-guide.mjs`（`card.md` が 6,000 字以内）、`cargo test --locked`。

### Phase 4

#### 手順 18: edition ブロックと `--edition`

- 変更: `src/guide.rs`（`filter_edition`・`--edition`）、`src/main.rs`、`tests/guide.rs`。
- 内容: テストを先に書き（`filter_edition` の単体と CLI）、実装する。`--edition` の既定はカレントの `Tsuzuri.toml`（`driver::read_manifest`。なければ `Edition::LATEST`）。
- 確認: `tsuzuri guide --edition 2026` が `tsuzuri guide` と 1 byte も違わない。`--edition 2030` が `E2000`。

#### 手順 19: 生成節の機構

- 変更: `src/guide.rs`（`expand_generated`）。
- 内容: 対のマーカーの本体の置換（マーカー行は出力に残さない）。`guide/` のファイルにマーカーは置かない。G23 の `hints::TABLE` が `src/` にあれば（`ls src/hints.rs`）、`pitfalls` の生成器を足して `guide/pitfalls.md` の表を対のマーカーに替える（G23 の表の行 → 付録 E の形の検証表にも足す）。なければ何もしない（機構だけを出荷）。
- 確認: 単体テスト `expand_generated_replaces_marker_lines_and_rejects_unknown_names`。

#### 手順 20: 最終確認

- 確認: 全 Phase の受け入れ条件。`cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings`・`cargo test --locked`・`node scripts/check-guide.mjs`・`node scripts/check-docs.mjs <変更したページ>`。完了報告（GUIDE §13.2）をチケットの末尾に書く。

## テスト計画

### `tests/guide.rs`（結合。新規。手本 `tests/editions.rs`・`tests/api_diff.rs`）

補助関数: `repo(path)`（`env!("CARGO_MANIFEST_DIR")` からの相対）、`root(label, files)`・`tsuzuri(args)`（`tests/editions.rs` から写す）、`examples(text) -> Vec<Example>`（fence の抽出。`check-docs.mjs` と同じ書式。未知のオプションは panic）、`chapters_on_disk() -> Vec<(&str, String)>`（`guide/` に存在する L1 の章を `language`・`ownership`・`pitfalls`・`workflow` の順で。期待値の元はコンパイラの出力ではなく**ファイル**）。

| テスト（`tests/guide.rs::`） | 入力 → 期待 | Phase |
| --- | --- | --- |
| `the_guide_prints_level_zero_by_default` | `tsuzuri guide` → 終了 0、stdout が `guide/card.md`（`\r\n` を `\n` に直したもの）と 1 byte 一致、stderr 空 | 1 |
| `level_zero_and_the_card_topic_are_the_same` | 引数なし・`--level 0`・`--topic card` の stdout が同じ | 1 |
| `level_one_joins_the_chapters` | `--level 1` の stdout = `chapters_on_disk()` の本文を `"\n"` で連結（章が 0 個なら終了 2 と `E2000`） | 1 |
| `every_topic_prints_its_file` | 存在する各章で `--topic NAME` の stdout がファイルと一致 | 1 |
| `list_names_every_topic` | `--list` の各行が `^(card\|language\|ownership\|pitfalls\|workflow) +L[01] +[0-9]+  .+$`（存在する章だけ、この順）、`chars` がファイルの文字数。`--list --json` は `{"schema":"tz-guide/1","compiler":…,"topics":[{"name":…,"level":…,"chars":…,"title":…}]}` | 1 |
| `json_output_has_the_documented_keys` | `--json` が 1 行で、`{"schema":"tz-guide/1","compiler":"<CARGO_PKG_VERSION>","edition":"2026","level":0,"topic":"card","title":"` で始まり、`chars` が `text` の文字数、`text` がファイルと一致。`--level 1 --json` は `"level":1,"topic":null` | 1 |
| `bad_arguments_are_rejected_with_e2000` | 表: `--level 2`・`--level`・`--topic`・`--topic nope`・`--level 0 --topic card`・`--list --topic card`・`--bogus`・`--json --json`・`--level 1 --level 1`・`std`（Phase 3 まで: 位置引数は未知のフラグと同じ扱いで `unknown option 'std'`）→ 終了 2、stderr に `E2000` と各メッセージの断片。`--json` を付けたときの stderr は `"code":"E2000"` を含む JSON | 1 |
| `guide_sizes_stay_within_the_limits` | `card` ≤ 6,000、存在する各章 ≤ `CHAPTER_LIMITS`、**描画した L1（`render(Level(1))`。連結の改行を含む）の文字数 ≤ 20,000**、全ファイルが `is_ascii()` で `\r` を含まない | 1 |
| `the_guide_examples_type_check` | `guide/*.md` の fence を 1 つずつ一時ディレクトリへ書き、`tsuzuri check` が終了 0。`card.md` に 2 つ以上あること | 1 |
| `the_guide_names_only_existing_subcommands` | `guide/*` の `` `tsuzuri <語>` `` の語が、`tsuzuri`（引数なし）の usage の行頭 `tsuzuri <語>`（`[build]` は `build`）の集合に入る | 1 |
| `pitfall_claims_hold_and_appear_in_the_card` | `pitfalls.txt` の全項目（`{word}` を展開して 29 件）を `check . --json` し、先頭の診断の `code` が表のどれか。`card.md` と存在する `pitfalls.md` の**ある 1 行**に、snippet と全コードが現れる | 1 |
| `every_pitfall_line_has_a_verification_entry` | `guide/pitfalls.md`（あれば）の `- ` で始まる行のそれぞれに、`pitfalls.txt` の snippet のどれかが含まれる | 1 |
| `the_checker_rejects_a_broken_example` | `node scripts/check-guide.mjs` を `TSUZURI_GUIDE_EXAMPLES=tests/fixtures/guide_broken`・`TSUZURI_BIN=$CARGO_BIN_EXE_tsuzuri` で実行 → 終了コードが 0 でなく、stderr に `card.md`。`node` がなければ飛ばす（`eprintln!` で理由を出す） | 1 |
| `new_writes_the_agent_files` | `tsuzuri new D --namespace Acme::Try` → 終了 0、stdout の `Created` が 5 行（`Tsuzuri.toml`・`Main.tz`・`.gitignore`・`AGENTS.md`・`llms.txt` の順）。`AGENTS.md` が `agents_md` の描画と一致し、`Acme::Try`・`2026` を含み `{{` を含まない | 2 |
| `new_without_agent_files_keeps_the_old_output` | `--no-agent-files` → `Created` が 3 行で、3 ファイルの内容が従来のひな形（`package.rs` の文字列。`Main.tz` の全文を直書きで比べる）と一致。`AGENTS.md`・`llms.txt` がない。2 回指定は終了 2・`E2000` | 2 |
| `rendered_templates_stay_within_the_limits` | `hello-world`・`HelloWorld`・`2026`・`0.1.0` で描画した `AGENTS.md` ≤ 3,500、`llms.txt` ≤ 1,500。生のひな形も同じ上限。`{{` が残らない。ASCII | 2 |
| `guide_agents_prints_the_template_for_a_project` | `new` したディレクトリで `guide --agents D` の stdout が `D/AGENTS.md` と一致。`--llms-txt` も同様。`--agents`（引数なし）はカレントの `Tsuzuri.toml` を使う | 2 |
| `guide_agents_requires_a_manifest_and_rejects_other_flags` | 空のディレクトリ → 終了 1・`E2000`。`--agents --json`・`--agents --level 0` → 終了 2 | 2 |
| `guide_std_lists_every_module` | `guide std` の各行 `^[A-Za-z0-9]+ +[0-9]+ functions( +\(opt-in\))?$`。`stdlib::SOURCES` の 47 モジュールと `Int`・`Simd`・`Rc`・`Arc`・`FixedArray`・`Dyn`・`Task`・`prelude` を含み、`Json` と `Atomic` に `(opt-in)` が付き、`Array` には付かない。名前の昇順（`prelude` が最後） | 3 |
| `guide_std_page_has_one_line_per_function` | `guide std Array` の `Array.` で始まる行が、`std/Array.tz` の公開（`private` でない）で小文字始まりの `def` の名前を全部含み、各行が `Array.<name> :: ` で始まる。`Array.set`（組み込み）の行もある。行はすべて昇順 | 3 |
| `guide_std_renders_borrow_conventions` | `Array.fold` の行が `Array.fold :: Copy<'a> => ('state -> 'a -> 'state) -> 'state -> ref ['a] -> 'state` と完全一致（独立した参照: `std/Array.tz` の `def fold ::` の行）。`Maybe.is_some`（ソースは `&(Maybe<'a>)`）が `ref Maybe<'a>` で出る | 3 |
| `guide_std_pages_stay_within_6000` | 全モジュールの `std MODULE` ≤ 6,000 字。`--topic std-all` ≤ 45,000 字。`--list` の `chars` と一致 | 3 |
| `guide_std_is_deterministic_and_unknown_modules_fail` | 2 回の出力が同じ。`std Nope` → 終了 2・`E2000`。`--json` の `topic` が `std:Array` | 3 |
| `guide_edition_flag_selects_the_edition` | `--edition 2026` の出力が引数なしと同じ。`--edition 2030` → 終了 2・`E2000`・`supported editions: 2026` | 4 |
| `every_guide_file_filters_for_every_supported_edition` | `guide/*` の全ファイルに `filter_edition` を全 `Edition::SUPPORTED` で適用して `Ok`（マーカーの不整合がない） | 4 |

### `src/guide.rs` の単体テスト（`#[cfg(test)] mod tests`）

`parse_arguments_accepts_the_documented_forms`（`[]`・`--level 1`・`--topic card --json`・`--list --json`・`--agents`・`--agents dir`・`--llms-txt dir`・`std`・`std Array`）、`parse_arguments_rejects_each_malformed_form`（CLI の表の全行。メッセージの断片）、`normalized_turns_crlf_into_lf`、`chars_counts_code_points_not_bytes`（`"é"` が 1）、`json_text_escapes_and_orders_keys`（`"` と `\n` と制御文字のエスケープ、キーの順序）、`render_template_replaces_exactly_four_words`（`{{other}}` はそのまま残る）、`filter_edition_keeps_only_the_matching_block`・`filter_edition_rejects_nested_or_unclosed_blocks`・`expand_generated_replaces_marker_lines_and_rejects_unknown_names`（Phase 4）。

### `src/guide_std.rs` の単体テスト（Phase 3）

`builtin_signatures_render_every_variant`（17 variant を 1 つずつ含む `BuiltinScheme` を手で作り、期待の文字列と比べる）、`collect_excludes_private_builder_and_internal_names`（`Maybe.Bind`・`Os.__read`・`Array.iter_from`（private）が出ない）、`collect_merges_a_source_and_a_builtin_with_the_same_name`（`Math.abs`）、`collect_lists_class_methods`（std に `.tt` がまだなくても通るよう、合成のソース `class Sized<'a> {` ⏎ `    def size :: ref 'a -> i64` ⏎ `    def same :: ref 'a -> ref 'a -> bool` ⏎ `}` を `parse_with_source` で読み、`Sized.size :: Sized<'a> => ref 'a -> i64` と `Sized.same :: Sized<'a> => ref 'a -> ref 'a -> bool` の 2 行が出る。メソッドが `private` なら出ない。D14・F15 が済んでいる木では、実際の索引に `Text.find`・`Monoid.combine` の行があることも `guide std Text`・`guide std Monoid` で確かめる）、`render_module_truncates_with_a_more_line`（合成の索引で 6,000 字超 → 末尾が `... N more: tsuzuri guide --topic std-all`、全体が 6,000 字以下）、`render_all_collapses_opt_in_modules_first`（合成の索引で 45,000 字超 → 大きい opt-in から 1 行に置き換わり、収まったところで止まる）。

### `src/package.rs` の既存テスト（変更。Phase 2）

`create_project` のテスト（`hello-world`・再実行・`other`・不正な名前）の呼び出しに第 3 引数 `true` を足し、`created.len()` を 3 から 5 に、`created[0..3]` の内容の検査は変えない。`created[3]`・`created[4]` のファイル名が `AGENTS.md`・`llms.txt` であることを足す。

### 既存テストへの影響

- `src/package.rs` の上のテスト（引数と件数。意図した変更）。
- `tests/editions.rs::new_projects_declare_the_latest_edition`: 変わらない（`new` は 0 で終わり、`check` は `.tz` だけ読むので `AGENTS.md`・`llms.txt` があっても通る）。根拠: ローダーは `.tz`・`.tt`・`.tc` 以外を読まない。
- `scripts/toolchain/smoke.mjs`・`vsc/src/workflow.ts`: 変わらない（同じ根拠）。
- ほかのテスト: なし。IR・object・`.wasm` はガイドを読まないので 1 byte も変わらない（Phase ごとに、`examples/hello` を `build --emit llvm -O0` した出力を手順 1 の基準と `cmp` で比べる）。
- `tests/guide.rs` の新規テストは、`tsuzuri guide` を呼ぶだけで LLVM を使わない（Rust のテストは LLVM 不要の原則）。`run=` の例の実行は `scripts/check-guide.mjs`（LLVM が要る）が受け持つ。

### 性能

`tsuzuri guide` は埋め込みの文字列を出すだけ。`std-all` は std 47 ファイルの構文解析を伴う。`time target/release/tsuzuri guide --topic std-all >/dev/null` を 5 回測って完了報告に書く（閾値は置かない）。

## ドキュメント

| Phase | 更新するページ | 内容 |
| --- | --- | --- |
| 1 | `_tsuzuri/language-reference/compiler/usage.md` | コマンド一覧に `guide`、`### new` の次に `### guide`（使い方・フラグの表・出力・`--json` のキー・終了コード・例）。例は ` ```sh `／` ```text `（`check-docs.mjs` が検査するのは ` ```tsuzuri ` だけ） |
| 1 | `_tsuzuri/language-reference/compiler/option.md` | 「どのサブコマンドが受け付けるか」の表に `guide` と `--level`・`--topic`・`--list`（G35 の `check-cli-docs` が `--help` の旗を README と `option.md` の両方に求める） |
| 1 | `README.md` | 「コマンド書式」に `tsuzuri guide [--level 0\|1] [--topic NAME] [--list] [--json]`、特長の一覧（22 行付近）に AI 向けガイド、「プロジェクトの作成」の次に節「AI エージェント向けのガイド」、検証コマンドに `node scripts/check-guide.mjs` |
| 1 | `docs/language.md` | `tsuzuri new` の段落（236 行付近）の次に `guide` の段落 |
| 1 | `docs/architecture.md` | ファイル表に `src/guide.rs`、トップレベルの `guide/` の説明、`src/main.rs` の行に `guide` |
| 1 | `docs/stability.md` | 「安定として保たないもの」の表に `tsuzuri guide` の本文・`AGENTS.md`・`llms.txt` のひな形の文言（D-56。直してよい）。保つもの: サブコマンド名・フラグ・`--json` のキー（足すだけ） |
| 1 | `_tsuzuri/language-reference/languages/how-about-tsuzuri.md` | コマンド表（277 行付近）に `tsuzuri guide` |
| 2 | `usage.md` の `### new`、`compiler/option.md`（`--no-agent-files`・`guide --agents`・`--llms-txt`）、`organizing-tsuzuri/packages.md`（`tsuzuri new` の節）、`README.md`（`tsuzuri new` の 2 か所）、`docs/language.md`（236 行）、`how-about-tsuzuri.md`（`new` の行）、`vsc/README.md`（36 行） | 6 ファイル、`--no-agent-files`、`guide --agents`・`--llms-txt` |
| 3 | `usage.md` の `### guide`、`README.md` | `std` |
| 4 | `usage.md`、`compiler/option.md`（`guide --edition`）、`docs/architecture.md` | `--edition`、edition ブロック、生成節 |

`_features/README.md`・GUIDE の台帳は統括者が更新するので、本チケットでは変えない（報告に追記が要る項目を書く）。検証: `node scripts/check-docs.mjs _tsuzuri/language-reference/compiler/usage.md _tsuzuri/language-reference/compiler/option.md _tsuzuri/language-reference/organizing-tsuzuri/packages.md _tsuzuri/language-reference/languages/how-about-tsuzuri.md`。

## 受け入れ条件

### Phase 1
- [ ] `tsuzuri guide` の stdout が `guide/card.md` と 1 byte 一致し、5,429 字（付録 A のとおり）。`--level 1`・`--topic`・`--list`・`--json` が仕様どおり。
- [ ] L1 の 4 章が上限（8,500・4,500・4,000・2,990）以内で、描画した L1 が 20,000 字以内。全ファイルが ASCII。
- [ ] `node scripts/check-guide.mjs` が通り、`cargo test --locked --test guide` の Phase 1 の全テストが通る（件数を記録）。`pitfalls.txt` の全項目が HEAD のコードと一致。
- [ ] 既存のコマンドの出力・IR が変わらない。`cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings`・`cargo test --locked` が緑。
- [ ] 「ドキュメント」の Phase 1 の行を更新し、`node scripts/check-docs.mjs` が通る。

### Phase 2
- [ ] `tsuzuri new` が 5 ファイルを作り、`--no-agent-files` なら従来と 1 byte も違わない 3 ファイル。描画後のサイズが上限内で `{{` がない。
- [ ] `tsuzuri guide --agents`・`--llms-txt` が `new` の出力と一致。`src/package.rs` と `tests/guide.rs` の Phase 2 のテストが通る。

### Phase 3
- [ ] `tsuzuri guide std`・`std MODULE`・`--topic std-all` が仕様どおりで、全ページが上限内、出力が決定的。`Array.fold` の行が参照と完全一致。
- [ ] `guide/card.md` に付録 H の Phase 3 の行を足しても 6,000 字以内。

### Phase 4
- [ ] `--edition` が動き、`--edition 2026` は既定と同じ出力。edition ブロックと生成節の機構の単体テストが通る。

## 落とし穴

- **共有のビルドキャッシュのロック**: `run` が 1 分以上返らないことがある（付録 D）。`export TSUZURI_CACHE_DIR=$PWD/target/guide-cache` を付ける。検出: `time tsuzuri run <dir>` の real が user より桁違いに長い。
- **`check-docs.mjs` は import できない**（読み込むと全ページ検査が走る）。`grep -n "^export" scripts/check-docs.mjs` が 0 件であることを確かめ、抽出を写す（D7）。Node 20 に `Map.groupBy`・`Object.groupBy` はない（`grep -n groupBy scripts/check-guide.mjs` が 0 件）。
- **`print!` と `println!`**: ガイドの出力に、ファイルの末尾以外の改行を足さない（テスト `the_guide_prints_level_zero_by_default` が 1 byte のずれで落ちる）。
- **`check .` と `check <dir>` の `path`**: ガイドの JSON の例は `./Main.tz`（`check .` のとき）。`<dir>/Main.tz` を書くと、読者が同じ出力を得られない。
- **隠れた診断**: `check` は型の誤りを直すまで所有権の検査（`E1012`・`E1013`・`E1014`）に進まないことがある（`E1003` を直したら `E1012` が出る）。落とし穴の「直し方」は、直したあとの `check` が通ることを確かめる（付録 E は先頭の診断のコードだけを照合する）。
- **`Vec.empty()` と `Vec.empty ()`**: 前者は引数なしの呼び出し、後者は `E1006`。`f()` と `f ()` を混ぜない。
- **CRLF と `include_str!`**: Windows の autocrlf で `\r\n` になるとサイズが増える（`normalized` を通す）。相対パスは `src/guide.rs` から見て `../guide/<file>`。
- **ディレクトリ名 `guide`**: 従来 `tsuzuri guide`（`build` の省略形）が通ったディレクトリは、`tsuzuri build guide` と書く（`new`・`fetch` などと同じ）。
- **計画中の機能を書かない**: `tsuzuri explain`・`fix`・`..<` は出荷されるまで書かない（`the_guide_names_only_existing_subcommands` が検出する）。

## 対象外

- 日本語のガイド、`--lang`。ガイドの文言の互換性の保証。KB（G27）・MCP（G29）・`explain`（G23）・`fix`（G24）の実装。
- 言語リファレンス（日本語）の書き直し。本チケットは参照して英語に要約するだけ。
- std の API の追加・変更。std の索引はソースと組み込みの表を読むだけで、署名を変えない。
- ガイドの自動翻訳・LLM による生成。
- `AGENTS.md` を書き込む（`guide --agents` は stdout に出すだけ。既存の `AGENTS.md` を上書きしない）。
- リポジトリ自身の `AGENTS.md`（開発方針）の変更。それは G32。

## 決定事項

承認済み（D-50）。実装者は蒸し返さない。

### D1: ガイドは top-level の `guide/` に置き、`include_str!` で埋め込む
- 決定: `guide/*.md`・`*.tmpl` を `src/guide.rs` が `include_str!("../guide/…")` で埋め込み、実行時にファイルを読まない。理由: バイナリ単体で動き、コンパイラの版とガイドが必ず一致する（D-56 の指定）。却下: `build.rs` で連結、実行時にデータディレクトリから読む（配布物が増え版がずれる）、`docs/` の下（日本語の利用者向け文書と混ざる）。状態: 承認済み。

### D2: L0 は 1 ファイル、L1 は 4 章
- 決定: `card.md` が L0。L1 は `language`・`ownership`・`pitfalls`・`workflow`（上限 8,500・4,500・4,000・2,990）で、`--level 1` はこの順に連結し、`--topic` は 1 章を出す。理由: 小さいモデルは必要な章だけ読める（G31 が測る入力量が下がる）。章単位でレビューとテストができる。却下: 26 KB の 1 ファイル、実行時にトピックを分割する機構。状態: 承認済み。

### D3: ガイドは全 edition が受理する書き方だけを教える
- 決定: 今は `..` が両端を含むことをそのまま書き（落とし穴に載せる）、A22 が `..<`・`..=` を出荷したらそのチケットが行を替える（付録 H）。edition ごとの差が出たら Phase 4 の edition ブロックで書く。理由: D-54（正準形は全 edition が受理する）。今書けるのは今のコンパイラが受理する形だけ。却下: `..<` を先に書く（今は `E0002`）、edition ごとに別のガイド。状態: 承認済み。

### D4: 出力はファイルのバイト列そのまま（ヘッダーなし）、`--json` は包みだけ
- 決定: テキストは `print!`（余分な改行・見出し・色なし）。版は `--json` の `compiler` に持たせる。理由: 「出力＝ファイル」を 1 byte で検査でき、サイズの上限が出力の字数と一致する。却下: 先頭にバージョンの行（上限と等価性の検査が複雑になる）、色付き整形。状態: 承認済み。

### D5: ひな形の置換は 4 語だけ
- 決定: `{{name}}`・`{{namespace}}`・`{{edition}}`・`{{version}}`。条件・繰り返し・環境変数はない。理由: 値が検証済みで、描画が `str::replace` の 4 回で済み、未置換をテストで見つけられる。却下: テンプレートエンジン（crate が要る）、`{{#if}}`。状態: 承認済み。

### D6: `tsuzuri new` は既定で `AGENTS.md`・`llms.txt` を書き、`--no-agent-files` で止める
- 決定: 追加のファイルは**あと**に作る。`--no-agent-files` の出力は従来と同じ。理由: 目的は AI が最初から使うこと。オプトインでは使われない。既存の 3 ファイルの内容を変えないので互換。却下: `--agent-files` のオプトイン、設定ファイルでの既定、既存のプロジェクトへの自動書き込み（`--agents` は stdout だけ）。状態: 承認済み。

### D7: `check-guide.mjs` は `check-docs.mjs` を変えず、fence の抽出を写す
- 決定: `check-docs.mjs`（export なし）は変えず、抽出の約 30 行を写して、その旨をコメントに書く。理由: 文書の検査の関門に触れる変更は本チケットの範囲外で、事故が文書全体に波及する。書式の変化は `the_checker_rejects_a_broken_example` と `the_guide_examples_type_check` が検出する。却下: 共通ライブラリへの切り出し（承認が要る別の変更）、`check-docs.mjs` を import（全ページ検査が走る）、Rust だけで実装（`run=` に LLVM が要る）。状態: 承認済み。

### D8: ガイドは ASCII だけ、サイズは出力の Unicode のコードポイント数
- 決定: ファイルは `is_ascii()`、上限は `chars().count()`。理由: D-56（英語）の機械的な形で、バイトと字数が一致する。却下: バイト数、トークン数（G31 の推定は上限に使わない）。状態: 承認済み。

### D9: 落とし穴の主張は検証表（`pitfalls.txt`）で縛る
- 決定: ガイドが書く「誤り → コード」を 29 件の小さなプログラムで `check` して照合し、ガイドの行にコードが書かれていることまで確かめる。L1 の `pitfalls.md` の全行は表に項目を持つ。理由: D-56「実行を通らない例を置かない」を散文の主張にも広げる。コードが変わったら（G22・G23 の作業）テストが落ちる。却下: 散文を信じる、G23 の表を待つ（G23 は表を同じ検証表へ移せる）。状態: 承認済み。

### D10: std の索引はソースと組み込みの表から作り、溢れは opt-in から畳む
- 決定: `stdlib::SOURCES` の構文解析と `Builtin::ALL` の `scheme()` から 1 関数 1 行。溢れの規則は付録 J。理由: `Vec.push`・`Int.abs`・`Rc.new` のような最頻の関数が組み込みで、ソースだけでは出ない。見積もりは上限ぎりぎり（付録 I）。却下: `docgen` のページの流用（説明文を含み 6,000 字を超える）、ソースだけ、`build.rs` での生成。状態: 承認済み。

### D11: edition の差は行ブロックで書く（Phase 4）
- 決定: `<!--if edition=YYYY-->`…`<!--endif-->`。今は使わない。理由: ファイルを複製せず、1 つのテキストから版ごとの出力を作れる。却下: edition ごとのファイル・ディレクトリ。状態: 承認済み。

### D12: 生成節は G35 と同じ対のマーカー `<!-- generated:NAME -->`／`<!-- /generated:NAME -->` で、機構だけを出荷する
- 決定: 本チケットは生成器を持たず、G23 が `pitfalls` を足す。理由: D-56「落とし穴の表は 1 か所（G23 のヒント DB）」。G23 が先でも後でもガイドが完結する。却下: G23 を待って出荷（P1 の G21 を止める）、本チケットで `hints` の表を作る（G23 の範囲）。状態: 承認済み。

### D13: `--agents` は `Tsuzuri.toml` を要し、書かずに stdout へ
- 決定: マニフェストがなければ終了コード 1。理由: 名前・名前空間・edition を推測しない（暗黙の既定値を入れない原則）。stdout なので既存のファイルを壊さない。却下: フォルダー名から推測、`--name` の指定。状態: 承認済み。

### D14: 英語だけ、`--lang` なし
- 決定: D-56 のとおり。日本語は言語リファレンスが受け持つ。却下: 日本語版・`--lang`。状態: 承認済み。

### D15: 計画中の機能をガイドに書かない
- 決定: 存在するサブコマンドだけを書き、`the_guide_names_only_existing_subcommands` で強制する。新しい機能の実装者が行を足す（付録 H）。理由: AI は書いてあるとおりに実行する。存在しないコマンドを案内すると最初の 1 回が失敗する。却下: 「(planned)」付きで書く。状態: 承認済み。

## 付録

- [G21-appendix.md](G21-appendix.md): 付録 A（`guide/card.md` 全文）・B（`AGENTS.md.tmpl`）・C（`llms.txt.tmpl`）・D（検証の記録）・E（落とし穴の検証表）・F（L1 の章立て）・G（`check-guide.mjs` の骨子）・H（他チケットへの依頼）・I（std 索引の実測）・J（std 索引の規則）。



