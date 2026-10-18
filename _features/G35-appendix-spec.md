# G35 付録 S: 仕様の表（本文から移した部分）

[G35 本文](G35-generated-docs-single-source.md)の「仕様」と「現状」から、長い表を移した。内容は本文と同じ重みで、実装の規則として従う。S.1〜S.3 は Phase 3、S.4 は Phase 4、S.5 は手順 3.3、S.6 は Phase 4 の根拠。

## S.1 描画の規則（`benchmarks/render.mjs` の `renderTable(headers, rows, { timingColumns })`）

参照コードは [G35-appendix-code.md](G35-appendix-code.md) の D.3、本文の文は E.5。

| 入力 | 出力 |
| --- | --- |
| 見出し行 | 見出し + 導出列（`timingColumns` があれば `観測最速`、Tsuzuri の時間の列があれば `Tsuzuri 相対性能`）。各見出しは `cell()` |
| 区切り行 | 1 列目と `観測最速` は `---`、残りは `---:` |
| 1 列目 | `` `値` ``（値の中の縦棒は直前にバックスラッシュを付けて escape する。値にバッククォートがあればエラー） |
| 時間のセル | その行で `-` でなく有限の数の列が候補。最小値の列すべてが勝者で、勝者のセルは `**値**`。勝者でないセルは `cell(値)` |
| `観測最速` | 勝者がいれば見出しの ` (ms)` を除いた名前を ` / ` でつなぐ（2 つ以上なら末尾に `（同率）`）。いなければ `条件差あり` |
| `Tsuzuri 相対性能` | Tsuzuri の列が候補で最小値が正なら `最小値 / Tsuzuri の値` を `toFixed(5)`、`0.00001` 未満なら `&lt;0.00001`。比べられなければ `-` |

`cell(text)` は `&`→`&amp;`・`<`→`&lt;`・`>`→`&gt;`・`|`→`\|`・改行→空白。ブロックの本体は、サマリーが E.5 の 2 段落・環境の箇条書き 4 行・太字の題（`**38種目 / generic / scale 0.1 / 1回の中央値**`）・表・注の 1 段落。metrics は run ごとに `**run \`ID\`** / 日時 / commit \`SHA\`（未コミットの変更を含む）`・箇条書き 3 行・表ごとの太字の題と表。色・`style`・`data-*` は無くなる。

## S.2 関数の対応（旧 → 新）

| 旧 | 新 |
| --- | --- |
| `report.mjs` の `escape`・`renderTable`・`renderSummary`・`updateSummary` | 削除。`renderTable` は `render.mjs` の Markdown 版。`renderSummary` は `summarizeResults(results)`（検査と中央値。summary オブジェクトを返す）と `renderSummaryBlock(summary)` に分かれる。マーカーが無い文書への自動挿入はやめ、`add the generated:benchmark-summary markers to docs/benchmarks.md first` で失敗する |
| `publishSummary(results, path)` | `publishSummary(results, { document, data })`。既定は `docs/benchmarks.md`・`docs/benchmarks-data/summary.json`。不正な結果では**どちらも書かない**。`writeAtomically` は `report.mjs` に残す |
| `report.mjs` の CLI・`run-managed.mjs` の呼び出し | 同じ |
| `metrics.mjs` の `renderMetricsSummary(records)`・`updateMetricsSummary(document, runs)` | `summarizeRecords(records)`（検査と表の作成。run オブジェクトを返す）・`publishMetrics(runs, { document, data })`。`metrics.mjs` は `render.mjs` の `renderMetricsBlock`・`serializeJson`・`validateMetrics` を使う（`renderTable` は使わない）。実装は付録 D.8・D.9 |
| `report <dir>... --publish` | 同じ（`metrics.json` と文書のブロックを書く）。`--baseline`・引数なしの出力は変更なし |

## S.3 移行の決定表

| 対象 | 現物 | 移行後 | 方法 |
| --- | --- | --- | --- |
| `benchmark-summary` の表 | HTML 1 表 | `summary.json` の `headers`・`rows`（導出 2 列は捨てる） | 解析 → 往復検査 |
| 3 つの `<p>`（凡例・最終測定・脚注） | HTML | 凡例と脚注は固定文（「緑色・太字」は「太字」に）。最終測定・CPU・SHA・C#/JS は `environment` | 環境だけ解析 |
| `perf-metrics` の run の `<p>` と 36 表 | HTML | `metrics.json` の `runs[]` | 解析 → 往復検査 |
| マーカー | `benchmark-summary:start/end`・`perf-metrics:start/end` | `generated:benchmark-summary`・`generated:perf-metrics` | 文字列置換 |
| 見出し `## ベンチマーク サマリー`・`## 性能指標の記録` | 手書き | そのまま（ブロックの外） | 触らない |
| 散文 3 か所 | HTML 前提 | JSON 前提 | E.5 の文に置換 |
| 生の測定 `target/perf/**` | コミット外 | 変更なし | |

## S.4 ドリフト検査の規則（`scripts/check-cli-docs.mjs`）

| 検査 | 抽出（正） | 検査される文書 | 規則 |
| --- | --- | --- | --- |
| `cli` | `compiler --help` の全文（終了コード 0）から、旗 `(?<![\w-])(--[a-z][a-z0-9-]*)`、サブコマンド `^ {2,4}tsuzuri (?:\[(build)\]\|([a-z][a-z-]*))`、環境変数 `\bTSUZURI_[A-Z0-9_]+\b` | README・`compiler/option.md` | 旗と環境変数は **README と `option.md` の両方**に文字列として含まれる。サブコマンドは **README** に `tsuzuri (?:\[NAME\]\|NAME)(?![A-Za-z0-9_-])` で含まれる（`usage.md` は必須にしない）。免除は `EXEMPT_FLAGS = { '--sysroot': 'C コンパイラの旗を述べた文' }` だけ |
| `codes` | カタログがあればその `code`。なければ `src/**/*.rs` を走査した 2 つの正規表現（`\(\s*"([EW][0-9]{4})"\s*,`、`\\"code\\":\\"([EW][0-9]{4})\\"`） | `compiler/diagnostics.md`・`docs/language.md` の表（各行の第 1 セルの `` `E1001`–`E1010` `` を展開し、`` `E2001` / `E2002` `` の結合も展開） | 2 つの文書の表の集合が、発行される集合と**一致する**。差は両方向を `FAIL codes: X is emitted but missing from PATH` / `FAIL codes: X is in PATH but no source emits it` |
| `version` | `Cargo.toml` の `version`。`compiler --version` | README・`docs/*.md`（`benchmarks.md` を除く）・`_tsuzuri/**/*.md` の `\b[Tt]suzuri v?(\d+\.\d+\.\d+)\b` | `--version` の出力が `tsuzuri ` + 版。すべての言及が版と一致する（`vsc/` は別の版系列で対象外） |
| `std` | `compiler doc std -o OUT` の各ページの公開 `def` | `STD_PAGES`（既定 `built-in-types-and-modules/<小文字のモジュール名>.md`、例外 `MatrixView` → `matrix-view.md`、`Async` → `../async-tasks-and-lazy/async.md`） | 各公開 `def` の名前が、ページに `` `name` `` か `Module.name` の形で含まれる。`def` を持つモジュールにページが無ければ `FAIL std: Module has N public functions but no page in STD_PAGES` |

**基線**（HEAD。数は検査の `ok` の行に出す）: `cli` は旗 38・サブコマンド 16・環境変数 10 で、失敗 8 件（README: `--debug-output`・`--version`・`--warn`・`toolchain`・`TSUZURI_DSYMUTIL`・`TSUZURI_LLVM_LINK`、`option.md`: `--git`・`--rev`）。`codes` は 47 で、`docs/language.md` の `W2001` 1 件。`version` は `0.1.0`・22 件で失敗 0。`std` は 559 で失敗 0。直し方は E.2。

## S.5 手順 3.3 のテストの書き換え表（`tests/benchmark_report.mjs` の冒頭 84 行の検査。書き換え後の全文は付録 D.10）

| 旧 | 新 |
| --- | --- |
| `updateSummary` が `## 表の読み方` の後・`## 記録` の前に入れる | 文書に `## ベンチマーク サマリー` の見出しと `<!-- generated:benchmark-summary -->` の対があるものを使い、`publishSummary` がその間だけを置き換え、外が同一。マーカーが無い文書では `add the generated:benchmark-summary markers` で失敗 |
| `data-fastest="true"` が 2 つ、`C++ / Rust（同率）` | `**…**` のセルが 2 つ、`観測最速` の列に `C++ / Rust（同率）` |
| `data-tsuzuri-performance="0.5"`・`0.50000`・`最速 = 1.00000` | セルに `0.50000`。`最速 = 1.00000` は段落の `1.00000` の語に |
| `Test &lt;CPU&gt;`・`font-variant-numeric`・`<strong><code style=` | `Test &lt;CPU&gt;` は同じ。装飾の 2 つは `assert.doesNotMatch(block, /<table\|style=\|<strong>/)` に |
| 2 回目の `updateSummary` が何も変えない・不正な結果（quick・条件違い・NaN・重複）で投げる・`&lt;0.00001`・`1.000e-9` | 同じ（`summarizeResults` が投げる）。不正なとき `summary.json` も文書も書かれない |
| `renderTable` が `<script>` を `&lt;script&gt;` に | 1 列目は `` `<script>` ``（コード span）、時間の列と見出しは `&lt;` |
| metrics: `peak_rss (bytes) / native / O0 / generic / 中央値`・`process/&lt;example&gt;`・列順 `Tsuzuri execute` < `C execute`・`1.250`・マーカーの検査・2 回目の `updateMetricsSummary` が何も変えない | 同じ内容を `summarizeRecords` + `renderMetricsBlock` に対して（`process/<example>` はコード span 内）。`updateMetricsSummary` の 3 つの guard は `publishMetrics` に |

## S.6 現状の詳細

### ドリフトの基線

付録 A.5 の測定スクリプトの結果:

- **CLI**（`tsuzuri --help` 全文から旗 38・サブコマンド 16・環境変数 10。正規表現は S.4）: README に無いのは旗 `--debug-output`・`--version`・`--warn`（と、C コンパイラの旗を述べた文の `--sysroot`）、サブコマンド `toolchain`、環境変数 `TSUZURI_DSYMUTIL`・`TSUZURI_LLVM_LINK`。`compiler/option.md` に無いのは旗 `--git`・`--rev`（`publish` の旗）だけで、サブコマンドの文に `fetch`・`publish` が無い。`compiler/usage.md` は旗 9・環境変数 2 を欠くが、使い方を説明するページなので必須にしない。
- **診断コード**: `src/*.rs` の発行（コンストラクタと、`main.rs` の JSON 文字列。正規表現は S.4）は 47 個。`compiler/diagnostics.md` の表（`E1001`–`E1010` の範囲を展開）も 47 個で**完全に一致**。`docs/language.md` の表（`` `E2001` / `E2002` `` の結合セルを展開）は 46 個で、`W2001` だけが表になく、散文で述べている。`E2026` は edition の定数で診断コードではない。
- **版**: `Cargo.toml` の `version = "0.1.0"`。README・`docs/`（`benchmarks.md` を除く）・`_tsuzuri/` に `Tsuzuri X.Y.Z` の形が 22 箇所あり、すべて `0.1.0`。`tsuzuri --version` は `tsuzuri 0.1.0`。
- **std 索引**: `tsuzuri doc std` の全 559 個の公開 `def` が、そのモジュールの言語リファレンスのページに `` `name` `` か `Module.name` の形で言及されている。559/559。
- `tsuzuri explain --catalog --json` は HEAD で `E2000`（`unknown option '--catalog'; use --help`）・終了コード 2。G23 が無いことの判定に使う。

### `_specs/` とその参照

- 5 ファイル（`error-handling.md`・`functions.md`・`literals.md`・`namespace-and-module.md`・`operators.md`）、計 21,524 byte。最後の変更は 2026-10-05。どのコード・テストも読まない（`tests/specs.rs`・`src/exceptions.rs`・`tests/fixtures/exceptions/Main.tz` は名前をコメントに書くだけ）。
- 名指しする文書: `_tsuzuri/language-reference/exception-handling/try-with-finally.md` の「注意点」の 3 つ目の箇条（`設計メモの _specs/error-handling.md には、…`）と、GUIDE（D-34 の節。履歴）。
- 項目ごとの照合（付録 C。実行して確かめた）: 46 項目のうち、現行に同じ形のものが 40、現行が異なるものが 6。コードブロック 20 件のうち 15 件が `tsuzuri check` を通り、`run` の出力は書かれたとおり。通らない 5 件は構文の雛形 4 件と、定義が次のブロックにある断片 1 件。食い違い 6 点のうち、実行して確かめたのは 3 点（残りの 3 点は付録 C の L4・L5・F2）: `try` は関数の戻り値型を `Result` にしない（`E1003: expected i32, found Result<i32, an undetermined type>`）、シフト量は左辺と同じ型（`E1003: expected i32, found i64`。`_specs` は「i32」）、既定の名前空間はパッケージ名の kebab-case を PascalCase にしたもの（`_specs` は「ルートフォルダー名」）。

## S.7 `scripts/gen-docs.mjs` の出力

stdout は件数の 1 行と `skip` の行、ずれの報告は stderr。

```text
gen-docs: 17 blocks checked, 0 stale
gen-docs: 17 blocks checked, 3 updated
skip diagnostic-kinds: this compiler has no 'explain --catalog --json' (G23); the block is not checked
stale _tsuzuri/language-reference/built-in-types-and-modules/map.md (complexity:Map)
  line 6 of the block
    expected: | `Map.insert` | $O(n)$ |
    found:    | `Map.insert` | $O(\log n)$ |
run 'node scripts/gen-docs.mjs' to update
```

`stale` の報告は、最初に違う行 1 つ（ブロック内の行番号、期待、現物）だけを出す。長さが違うときは、短い方の最後の次の行を `(none)` として出す。`--check` で 1 つでも `stale` があれば最後に `run 'node scripts/gen-docs.mjs' to update` を stderr に出して終了コード 1。
