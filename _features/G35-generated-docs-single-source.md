# G35: 生成物による文書の単一情報源化（診断コード表・std 索引・benchmarks の JSON 化）

| 項目 | 内容 |
| --- | --- |
| ID | G35 |
| 優先度 | P2 |
| 規模 | L（Phase 1: S、Phase 2: M、Phase 3: M、Phase 4: M。種の「M」は 1 Phase の大きさで、4 Phase の合計は 2 週間ほど） |
| 依存 | G23（Phase 1 の `diagnostic-kinds` の生成と Phase 4 の診断コード表の検査が `tsuzuri explain --catalog --json` を使う。**未実装でも全 Phase を出荷できる**: 「前提とする他チケットのインターフェース」）。G26（種の依存だが、本チケットは使わない。`Documentation` の読み取り関数を G26 が呼べるように出すだけ） |
| 後続 | G21（`guide std` が `Documentation::complexity` を読める）、G26・G27（KB が `complexity`・`since`・`see` の欄を持てる）、C12（Map・Set の `@complexity` を更新する）、G33（規則 ID。マーカーは `<!-- rule:` で、本チケットの `<!-- generated:` と衝突しない） |
| 状態 | todo |
| 起票 | 2026-10-18（D-50。HEAD `a2313d19` で確認。作業ツリーの HEAD は `9dd0fbbc`＝`a2313d19` の親で、差は `docs/benchmarks.md` の 41 行（第 4 フェーズの `<!-- phase4:… -->` の枠）だけ） |
| 承認 | 承認済み（D-50: 人間の包括許可 2026-10-18）。決定事項 D1〜D20 はすべて承認済み |
| 改善する劣位 | 手書きと生成物の二重管理で文書が実装からずれる（D-50 の調査結論 (4)）。`docs/benchmarks.md` は 952,255 byte のうち 2 つの HTML ブロックが 495,437 byte（52.0%）を占め、diff もレビューも読めない。計算量の表は 7 ページと `docs/language.md` に手書きで、`Array.sort_in_place` の行は実装と合わない。README は旗・サブコマンド・環境変数を 6 つ欠く。`_specs/`（初期設計）は現行と食い違ったまま残る |
| 手本にする既存実装 | `benchmarks/report.mjs` の `updateSummary`（マーカーが 1 組だけの検査）・`writeAtomically`。`scripts/check-docs.mjs`（`headings`・リンクの正規表現・コンパイラの呼び出し）。`src/syntax.rs` の `Documentation::deprecation` と `src/warnings.rs` のテスト `deprecation_note_reads_the_first_tag_line`。`src/docgen.rs` の `render_declarations`（`tsuzuri doc std` のページの形）、`src/stdlib.rs` の `SOURCES`。`benchmarks/metrics.mjs` の `renderMetricsSummary`・`renderComparison`（Markdown の表）。`tests/benchmark_report.mjs`・`tests/docgen.mjs` |
| 主な影響ファイル | **Phase 1** 新規: `scripts/gen-docs.mjs`, `tests/gen_docs.mjs`, `.github/workflows/docs.yml`。変更: `scripts/check-docs.mjs`, `README.md`, `AGENTS.md`。**Phase 2** 新規: `tests/doc_tags.rs`, `tests/std_tags.rs`。変更: `src/syntax.rs`, `scripts/gen-docs.mjs`, `std/{Array,Vec,Map,Set,HashMap,HashSet,List,String}.tz`（`///` だけ）, 言語リファレンス 9 ページ, `docs/language.md`。**Phase 3** 新規: `benchmarks/render.mjs`, `docs/benchmarks-data/{summary,metrics}.json`, `scripts/check-links.mjs`, `tests/check_links.mjs`, `tests/benchmark_render.mjs`（一時: `benchmarks/migrate-html.mjs`・`benchmarks/legacy-report.mjs`）。変更: `benchmarks/{report,metrics}.mjs`, `scripts/gen-docs.mjs`, `tests/benchmark_report.mjs`, `docs/benchmarks.md`, `.github/workflows/docs.yml`。**Phase 4** 新規: `scripts/check-cli-docs.mjs`, `tests/check_cli_docs.mjs`, `_specs/README.md`。移動: `_specs/*.md` → `_specs/_archive/`。変更: `README.md`, `compiler/option.md`, `try-with-finally.md`, `docs/language.md` |

付録は [G35-appendix.md](G35-appendix.md)（A 実測・B 初期の `@complexity` 表 134 行・C `_specs` の決定表）、[G35-appendix-spec.md](G35-appendix-spec.md)（S 描画・関数の対応・移行の決定表・ドリフト検査の規則・`gen-docs.mjs` の出力）、[G35-appendix-code.md](G35-appendix-code.md)（D 参照コード・E 文書の文面・F テストの期待値）。

## 実装者向け早見

- **最初に読む**: `scripts/check-docs.mjs`（115 行）、`benchmarks/report.mjs`（153 行。`updateSummary`・`publishSummary`）、`benchmarks/metrics.mjs` の `renderMetricsSummary`・`updateMetricsSummary`・末尾の `report` の分岐、`src/syntax.rs` の `Documentation`、`tests/benchmark_report.mjs` の冒頭 84 行、`target/release/tsuzuri doc std -o <dir>` の出力（付録 A.1）。
- **最初に書くテスト**: Phase 1 は `tests/gen_docs.mjs` の `scanBlocks`（手順 1.1。関数を空にして失敗を確かめてから書く）。Phase 2 は `tests/doc_tags.rs`（手順 2.1）。Phase 3 は `tests/check_links.mjs`（手順 3.1。移行の前に緑にする）。Phase 4 は `tests/check_cli_docs.mjs`（手順 4.2）。
- **触ってはいけない**: コンパイラの診断・IR・`tsuzuri doc` の既存のページ構造（Phase 2 は `///` の行を足すだけで、字句・構文・型検査・コード生成を変えない。`FRONTEND_FORMAT` を上げない）。`metrics.mjs report --baseline` の比較表（`compareRuns`・`renderComparison`。`_perfs/` の多数のチケットが使う）。`docs/benchmarks.md` の 2 つのブロックと手順 3.6 の 3 か所の散文以外の行（1 byte も変えない）。`_specs/` の中身（移すだけ）。README の `Map` の説明（C12 が直す）。
- **新しい名前**（診断コードは増えない）: `scripts/{gen-docs,check-cli-docs,check-links}.mjs`、`benchmarks/render.mjs`、マーカー `<!-- generated:NAME -->`、doc タグ `@complexity`・`@since`・`@see`、`Documentation::{complexity, since, see}`、`docs/benchmarks-data/{summary,metrics}.json`（スキーマ名 `tz-bench-summary/1`・`tz-bench-metrics/1`）、`.github/workflows/docs.yml`、`_specs/_archive/`。
- **Phase の完了の見分け**:
  - Phase 1: `node scripts/gen-docs.mjs --check` が（G23 がなければ `skip diagnostic-kinds: …` の 1 行を出して）終了コード 0。`node tests/gen_docs.mjs` が緑。
  - Phase 2: `node scripts/gen-docs.mjs --check` が `16 blocks checked, 0 stale`（G23 があれば診断のブロックを足して 17）。`cargo test --locked --test std_tags --test doc_tags` が緑。
  - Phase 3: `docs/benchmarks.md` が 520,000 byte 以下になり、`node scripts/check-links.mjs` が `70 links into docs/benchmarks.md … 0 broken`、`node tests/benchmark_report.mjs` が 34 行・終了コード 0、`node tests/benchmark_render.mjs` が 8 行・終了コード 0。
  - Phase 4: `node scripts/check-cli-docs.mjs` が 4 つの検査すべて `ok`、`_specs/` の直下に `README.md` と `_archive/` だけがある。
- **規則**: 生成ブロックの中を手で直さない（`node scripts/gen-docs.mjs` で作り直す）。複雑さの値は付録 B の表と「仕様 3」の規則から決める。実装を読んで食い違いに気づいたら、値を合わせて黙るのではなく「停止条件」4 の手順で報告する。

## 目的

文書の事実を、1 か所の情報源（ソースの `///`、コンパイラの出力、JSON のデータ）から作り、ずれを CI で見つける。手書きの表が実装と食い違う、生成物が文書の半分を占めて読めない、初期設計の文書が現行の仕様に見える、という 3 つの問題を同じ仕組み（`<!-- generated:NAME -->` ブロックと検査）で直す。

HEAD で測った数字（再現コマンドは「現状」と付録 A）:

- `docs/benchmarks.md` は 7,280 行・952,255 byte。2 つの HTML ブロック（`benchmark-summary` 100,585 byte、`perf-metrics` 394,852 byte）は合計 495,437 byte で、ファイルの 52.0%（文字数で 64.4%）。同じ内容を Markdown にすると 2 ブロックで 32,251 byte（5,794 + 26,457。試作。付録 A.4）で、93.5% 小さい。
- std の公開 `def` 559 個のうち、Array・Vec・Map・Set・HashMap・HashSet・List・String の 8 モジュールは 134 個。`@complexity` を持つものは **0 個**。7 ページの `## 計算量` の表は計 53 行で手書き。`Array.sort_in_place` の行は `O(n log n)` だが、実装は SymMerge で、要素の移動は `O(n log^2 n)`。
- 診断コード 47 個・CLI の旗 38・サブコマンド 16・環境変数 10 を文書と突き合わせる検査は無く、README に無いものが 6、`compiler/option.md` に無いものが 2、`docs/language.md` の診断の表に無いものが 1。
- `_specs/` は 5 ファイル 21,524 byte。46 項目のうち 6 点が現行と食い違い、`try-with-finally.md` が `_specs/error-handling.md` を名指ししている。

利用者の価値: 計算量の表と診断の kind の表は、ソースを直せば文書が直る。AI も人も、ファイルの半分が HTML の `benchmarks.md` を読まずに済む。古い設計メモに従って誤ることがない。

**非目標**: 計算量の自動推定・測定（`@complexity` は人が書く宣言で、測定で検証しない）。`Builtin`（組み込みの操作: `Vec.push`・`Array.set`・`List.cons`）に doc を持たせること。README の `Map` の説明の修正（C12）。KB・`guide std` の実装（G21・G26・G27）。`_specs` の内容を言語リファレンスへ取り込むこと（取り込むものがない: 付録 C）。生の測定データ（`target/perf/**.jsonl`）をコミットすること。リポジトリ全体のリンク検査（既存の壊れたリンク 11 件は別件: 「現状」）。

## 着手条件と停止条件

### 着手条件

- 依存するチケットはない。Phase 2・3・4 は Phase 1 の手順 1.1〜1.5 が前提（`gen-docs.mjs` と `docs.yml`）。Phase 4 の手順 4.1（`_specs`）だけは Phase 1 を待たずに着手してよい。
- GUIDE §2.3 の基準コマンドが成功する状態から始める（`cargo build --release --locked && cargo test --locked`）。加えて `node tests/benchmark_report.mjs` が 34 行を出して終了コード 0（HEAD で 4.2 秒）。
- 読む: 付録 A（実測）、`docs/stability.md`（説明の変更は互換性を保つ変更）、GUIDE D-50・D-55・D-56。Phase 2 の前に G34 の「Phase 3 の規則」を読む（同じ std の `///` に行を足す。順序は「仕様 3.4」）。

### 停止条件

止めて報告する。選択肢を付ける。

1. **マーカーが変わっている**: `grep -c "<!-- benchmark-summary:\|<!-- perf-metrics:" docs/benchmarks.md` が 4 でない、または `grep -c "<!-- generated:" docs/benchmarks.md` が 0 でない。A（現物の行を読み直して続ける）、B（変えた他のチケットを待つ）。
2. **移行の往復検査が通らない**（手順 3.5）: 解析した JSON から旧 `renderTable` で作り直した HTML が現物と 1 byte でも違う。A（現物の書式を付録 D.4 の解析器に足す）、B（その表だけ手で JSON にする）。**手で数値を直して合わせない。**
3. **G23 のブロックの形が違う**: G23 が出荷済みで、`diagnostics.md` の表の見出し行・区切り行・空行が「仕様 2」の既定と違う。A（`DIAGNOSTIC_KINDS_HEADER` を G23 の現物に合わせる。通常。止めない）、B（両方が通らないとき: `the_docs_list_every_kind` を `gen-docs.mjs --check` の呼び出しに置き換える。G23 の持ち主の承認が要る）。
4. **std の関数が付録 B と違う**（手順 2.4 の開始時）: 8 モジュールの公開 `def` が 134 個でない、または名前の並びが違う。A（増えた関数の値を「仕様 3.3」の規則の表から決め、付録 B と同じ形で足す）、B（規則の表にない形の関数があれば止めて報告）。
5. **タグの名前が使われている**: `Documentation` に `complexity`・`since`・`see` がある、または `git grep -c "@complexity\|@since\|@see" -- std` が 0 件でない。意味が違う可能性がある。
6. **リンクの基線が違う**（手順 3.1）: `check-links.mjs` が `docs/benchmarks.md` 向けの壊れたリンクを報告する。壊れたリンクの直しは本チケットの仕事ではない。別の PR にして待つ。
7. **`docs.yml` が新しい手順以外で赤い**（`cargo build`・`actions/*` の版・キャッシュ）: 使い捨てのブランチで直す（AGENTS.md の CI の教訓）。2 回直して緑にならなければ止める。
8. **CLI の基線が違う**（手順 4.3）: 旗が 38、サブコマンドが 16、環境変数が 10 でない。差が 4 つ以下なら文書へ足して続け、5 つ以上なら止める（別のチケットが CLI を大きく変えている）。
9. **`tsuzuri doc std` の形が変わった**: 節が `## \`name\`` で始まらない、署名が ```` ```tsuzuri ```` の直後でない。付録 D.2 の解析関数が前提にしている。
10. **基線の赤**: `node tests/benchmark_report.mjs` または `cargo test --locked --test docs` が着手時に赤い。直す前に他の変更をしない。

## 現状（HEAD `a2313d19` で確認）

再現に使ったコマンドと出力。実行はすべて作業ツリー（`9dd0fbbc`）で、`target/release/tsuzuri`（`tsuzuri --version` が `tsuzuri 0.1.0`）を使った。

### `docs/benchmarks.md` と生成コード

```text
$ grep -n "<!--" docs/benchmarks.md | cut -c1-48
256:<!-- benchmark-summary:start -->
735:<!-- benchmark-summary:end -->
2139:<!-- perf-metrics:start -->
5326:<!-- perf-metrics:end -->
$ wc -lc docs/benchmarks.md
    7280  952255 docs/benchmarks.md
$ sed -n 256,735p docs/benchmarks.md | wc -c ; sed -n 2139,5326p docs/benchmarks.md | wc -c
  100585
  394852
```

- 1 つ目のブロックは `benchmarks/report.mjs` が書く（`renderSummary` が見出し・マーカー・`<p>` 3 つ・HTML の表・注の `<p>` を作り、`updateSummary` が見出しから終了マーカーまでを置き換え、`publishSummary` が `writeAtomically` で書く。CLI は `node benchmarks/report.mjs result.json [...]` で、`benchmarks/run-managed.mjs` も `publishSummary([result])` を呼ぶ）。表は 38 行 × 10 列（`観測最速`と`Tsuzuri 相対性能`は導出列）。
- 2 つ目は `benchmarks/metrics.mjs` が書く（`renderMetricsSummary` が run ごとの `<p>` と、指標・target・最適化・CPU の組ごとの表を作り、`updateMetricsSummary` がマーカーの間を置き換える。入口は `node benchmarks/metrics.mjs report <dir>... --publish`）。run 2 つ（`PX02-formal-managed`・`PX03-formal-emit-v4`）、表 36、本体の行 314・セル 2,109。生の記録（`target/perf/<run_id>/<suite>.jsonl`）はコミットされていない。
- 2 つの表は `renderTable` の出力で書式が決まっている。試作（付録 A.3）で、HTML から `{caption, headers, rows}` を取り出して旧 `renderTable` で作り直すと、2 ブロックとも**現物と 1 byte も違わない**（`summary: headers 8 rows 38 round trip true`、`metrics: runs 2 tables 36 rows 314 cells 2109 round trip true`）。
- 散文の言及は 3 か所: 「サマリー表は HTML テーブル形式で…」の 2 文と、「`node benchmarks/metrics.mjs report <dir>... --publish` は、下のマーカーの間だけに…」の 1 文。残りの手書きの節（`## 第3フェーズの測定…` の 208,927 B を含む）は生成物ではない。
- `docs/benchmarks.md` を 81 ファイルが言及し、断片つき（`benchmarks.md#…`）の相対リンクは 46 件（17 種、17 ファイル。`docs/benchmarks.md` 自身の `](#…)` 2 件と、他の 16 ファイル）で、すべて解決する（付録 A.6）。リポジトリ全体では 256 の `.md` に相対リンク 2,178 件があり、**壊れたものが 11 件**（`_features/GUIDE.md`、`_features/_completed/` の 2 ファイル、`_perfs/_completed/PM06-…`、`docs/language.md` の 4 件）。`docs/benchmarks.md` 向けは 0 件。
- `tests/benchmark_report.mjs`（554 行）は両方の描画関数を検査する。HEAD で 4.2 秒・34 行（最初の行 `Benchmark report: placement, ranking/ties, units, escaping, idempotence and invalid-result guards passed`、続く 33 行が `Metrics: … passed`）。CI は `targets.yml` の「Benchmark correctness and metric records」で実行する。

### 計算量の表と std

```text
$ target/release/tsuzuri doc std -o <out>      # 0.29 秒。2 回の出力は diff -rq で同一
$ # 8 モジュールの公開 def（各 <out>/<Module>.md の「## `name`」の節のうち、署名が def で始まるもの）
Array 39, Vec 1, Map 14, Set 13, HashMap 24, HashSet 17, List 5, String 21  → 計 134
$ git grep -c "@complexity\|@since\|@see" -- std       # 出力なし（0 件）
```

- 134 個のうち `///` があるのは 16 個（Array 6、HashMap 6、HashSet 3、List 1）。残りの 118 個は説明がない。
- 手書きの計算量の表: `array.md`（11 行）・`vec.md`（8）・`map.md`（5）・`set.md`（7）・`hashmap.md`（7）・`hashset.md`（5）・`list.md`（10）の `## 計算量` が計 53 行。`string.md` には `## 計算量` が無く、4 つの表に `O(n)` の列がある。`docs/language.md` には `### HashMap / HashSet` に表（5 行）、`### Map / Set` に散文がある。
- 食い違い（付録 B の根拠）: `array.md` は `sort_in_place` を `O(n log n)`「安定マージソート」とするが、`std/Array.tz` の実装は 20 要素の挿入ソートと SymMerge で、移動は `O(n log^2 n)`。`List.iter` は先にリンクを 1 回たどるので `O(n)` だが、`list.md` の表に行がない。`Map.insert` は既存キーの置換が `O(log n)`、新しいキーが `O(n)`。
- 組み込み操作（`Vec.push`・`Array.set`・`List.cons`）は `std/*.tz` に `def` がなく、`tsuzuri doc std` に出ない。`Vec` の `def` は `iter` だけ。

### ドリフトの基線と `_specs/`（詳細は [付録 S.6](G35-appendix-spec.md)）

- **CLI**: `--help` から旗 38・サブコマンド 16・環境変数 10 を取れる。README に無いのは旗 3（`--debug-output`・`--version`・`--warn`）・サブコマンド `toolchain`・環境変数 2（`TSUZURI_DSYMUTIL`・`TSUZURI_LLVM_LINK`）、`compiler/option.md` に無いのは旗 `--git`・`--rev`。
- **診断コード**: `src/` が発行する 47 個は `compiler/diagnostics.md` の表と完全に一致。`docs/language.md` の表は 46 個（`W2001` が散文だけ）。**版**: `Cargo.toml` と文書の 22 箇所が `0.1.0`。**std 索引**: 559 個の公開 `def` がすべてページに言及されている。`tsuzuri explain --catalog --json` は HEAD で `E2000`・終了コード 2（G23 が無い判定に使う）。
- **`_specs/`**: 5 ファイル 21,524 byte。コード・テストは読まず、文書で名指すのは `try-with-finally.md` の 1 箇所だけ。具体的な例はすべて `tsuzuri check` を通り、食い違いは 6 点（`try` の戻り値型・シフト量の型・既定の名前空間・`utf8string` の説明・`byte[]` の表記・`when` のガード）。

### CI と既存の検査

- `scripts/check-docs.mjs`（115 行）は `_tsuzuri/` の `.md` だけを走査し、CI では動かない（README の「テストスイートの実行」と AGENTS.md が手順として挙げるだけ）。
- `targets.yml` は `docs/**`・`_tsuzuri/**`・`README.md`・`scripts/*.mjs` の変更では起動せず、起動すれば 4 ホスト（35〜85 分）になる。コンパイラの依存は `rustc_apfloat` と `serde_json` だけで、`cargo build --release --locked` に LLVM は要らず、`check`・`doc`・`--help` は clang を使わない。

## 変えてはいけない意味

- **コンパイラ**: 診断（コード・メッセージ・位置・順序）、生成 IR、`.wasm`、`tsuzuri doc` の既存のページ構造（`# Module`・`Namespace:`・`## \`name\``・署名のコードブロック・本文）、LSP の応答、`FRONTEND_FORMAT`。Phase 2 は `std/*.tz` に `///` の行を足し、`Documentation` に読み取り関数を足すだけ。std の関数本体・署名は 1 文字も変えない。タグの有無で IR は 1 byte も変わらない（`@deprecated` と同じ）。
- **`metrics.mjs`**: `process`・`size` の測定、`report <dir>`（行ごとの出力）、`report <dir> --baseline <dir>`（`compareRuns`・`renderComparison` の Markdown の表）。`--publish` だけが変わる。
- **`report.mjs` の CLI**: `node benchmarks/report.mjs result.json [...]` は引き続きサマリーを公開する（`run-managed.mjs` も同じ関数を呼ぶ）。quick の結果を公開しない・異なる条件の結果を混ぜないという検査も変えない。
- **`docs/benchmarks.md`**: 2 つのブロック（Phase 3 で内容が変わる）と手順 3.6 の 3 か所の散文以外の行。見出しはすべて残す（アンカーの検査: 手順 3.1）。
- **言語リファレンスの例**: `node scripts/check-docs.mjs` が通る例を壊さない。計算量の表は ` ```tsuzuri ` を含まない。
- **Phase の独立**: 各 Phase は、後の Phase を待たずに出荷できる。G23 が無いとき、Phase 1 の診断のブロックは休眠し、既定の動作は何も変わらない。

## Phase 分割

| Phase | 内容 | 規模 | 前提 | 単独で出荷できる理由 |
| --- | --- | --- | --- | --- |
| 1 | `scripts/gen-docs.mjs`（ブロックの走査・生成・`--check`・`--list`）、診断 kind の表の生成（G23 がなければ休眠）、`check-docs.mjs` からの呼び出し、`docs.yml` | S | なし（G23 は任意） | 生成するブロックが無ければ何も書かず、文書を 1 文字も変えない。検査は読むだけ |
| 2 | doc タグ `@complexity`・`@since`・`@see`、`Documentation` の読み取り関数、std 8 モジュール 134 関数へのタグ、計算量の表の生成（言語リファレンス 8 ページ・`docs/language.md`）、`tests/std_tags.rs` | M | Phase 1 | タグは説明の本文。診断・IR・`tsuzuri doc` のページ構造を変えない。表は 1 ページずつ置き換えられる |
| 3 | benchmarks の JSON 化: `docs/benchmarks-data/*.json`、Markdown の表、`report.mjs`・`metrics.mjs` の書き込み、HTML ブロックの移行、リンク検査 | M | Phase 1 | `docs/benchmarks.md` の 2 つのブロック以外と、`--baseline` の比較表を変えない。`report.mjs` の CLI は同じ |
| 4 | `_specs/` の整理、ドリフト検査（`scripts/check-cli-docs.mjs`: CLI・診断コード・版・std の言及）、文書の欠けの補完 | M | Phase 1（CI）。G23 は任意 | 検査は読むだけ。文書の補完は文書だけの変更 |

## 仕様

### 1. 生成ブロックと `scripts/gen-docs.mjs`（Phase 1）

**ブロックの記法**（行全体。先頭に空白を置かない）:

```text
start = "<!-- generated:" NAME " -->"
end   = "<!-- /generated:" NAME " -->"
NAME  = [a-z][a-z-]* [ ":" [A-Za-z0-9]+ ]      例: diagnostic-kinds, complexity:Map, benchmark-summary
```

| 場合 | 結果 |
| --- | --- |
| 行頭が ```` ``` ```` で挟まれた範囲の中 | マーカーとして扱わない（文書がマーカーの書き方を例示できる） |
| 先頭に空白のある行、`NAME` が上の形でない行 | マーカーではない |
| 開始の後、終了の前に別の開始 | エラー `PATH:LINE: nested block 'B' inside 'A'` |
| 同じ `NAME` が同じファイルに 2 回 | エラー `PATH:LINE: duplicate block 'NAME'` |
| 終了だけ、または名前が合わない終了 | エラー `PATH:LINE: end marker without start 'NAME'` |
| 開始だけ | エラー `PATH:LINE: missing end marker for 'NAME'` |

ブロックの**本体**は、2 つのマーカー行の間の行（空行を含む）。生成器は本体を丸ごと置き換え、マーカー行とファイルの他の部分は 1 byte も変えない。文書の改行は `\n` だけで、`\r` を含むファイルはエラー（`PATH: CRLF is not supported`）。

**CLI**: `node scripts/gen-docs.mjs [--check] [--list] [--only NAME]... [--bin PATH]`

| 指定 | 動作 | 終了コード |
| --- | --- | --- |
| なし | すべての置き場（`PLACEMENTS`）のブロックを生成して書く。変わらないファイルには触らない（mtime も変えない）。書き込みは一時ファイルに `wx` で書いて `rename`（`benchmarks/report.mjs` の `writeAtomically` と同じ） | 0 |
| `--check` | 書かずに、生成した本体とファイルの本体を比べる | 一致 0、ずれ・不正な入力 1 |
| `--list` | 置き場を 1 行ずつ `NAME<TAB>PATH` で出す（`PLACEMENTS` の順）。コンパイラを呼ばない | 0 |
| `--only NAME` | 繰り返せる。`placement.name === NAME` か `placement.name` が `NAME:` で始まるものだけを対象にする（`--only complexity` は `complexity:*` すべて）。どれにも一致しなければ `gen-docs: no block named 'NAME'` で終了コード 2 | |
| `--bin PATH` | コンパイラ。既定は環境変数 `TSUZURI_BIN`、なければ `target/release/tsuzuri`（`check-docs.mjs` と同じ）。無ければ `gen-docs: compiler not found: PATH` で終了コード 2 | |
| 未知の旗 | `gen-docs: unknown option '--x'` と `usage: node scripts/gen-docs.mjs [--check] [--list] [--only NAME]... [--bin PATH]` | 2 |

**出力**（stdout は件数の 1 行と `skip` の行、ずれの報告は stderr）の例と `stale` の報告の規則は [付録 S.7](G35-appendix-spec.md)。件数の行は `gen-docs: N blocks checked, 0 stale`（`--check`）と `gen-docs: N blocks checked, M updated`（書き込み）。

**置き場の表 `PLACEMENTS`**: `{ name, file, requires? }` の配列を `scripts/gen-docs.mjs` の先頭に置く。走査するのは置き場のファイルだけ。ファイルに置き場の `name` のブロックが無ければエラー `PATH: missing block 'NAME'`、置き場にない `name` のブロックがあればエラー `PATH: unknown block 'NAME'`（マーカーの打ち間違いを見つける）。Phase 1 の置き場は 1 つ:

| name | file | requires |
| --- | --- | --- |
| `diagnostic-kinds` | `_tsuzuri/language-reference/compiler/diagnostics.md` | `explain` |

`requires: 'explain'` の置き場は、先に `hasExplain`（`<compiler> explain --catalog --json` が終了コード 0）を調べる。偽なら `skip …` を出し、そのブロックを検査も生成もしない（ファイルにブロックがなくてもエラーにしない）。真でブロックが無ければ、`missing block` のエラーに `(G23 step 4.2 adds the markers)` を添える。

**呼び出し元**: `scripts/check-docs.mjs` は、ページの検査のあとに `checkBlocks({ root, compiler, files })`（`files` は指定されたページ。無指定なら全置き場）を呼び、ずれがあれば `assert` で失敗させる。最後の行は `Docs: … , N generated blocks.` に変える。`gen-docs.mjs` は import しただけでは何もしない（`process.argv[1]` が自分のときだけ `main`）。

### 2. 診断 kind の表（Phase 1。G23 の後に有効）

入力は `<compiler> explain --catalog --json`（G23 の S4: 1 行 1 オブジェクト `{"kind","code","severity","since","title","page"}`）。出力（本体）:

```text
| コード | kind |
| --- | --- |
| `E0001` | `lex_error`, … |
| `E1012` | `use_after_move`, … |
```

コードの昇順（文字列の比較）、同じコードの kind は id の昇順、`, ` でつなぎ、各 kind をバッククォートで囲む。見出し行と区切り行は定数 `DIAGNOSTIC_KINDS_HEADER`（上の 2 行）。**G23 の手順 4.2 が置く現物と見出し・区切り・空行が違えば、現物に合わせる**（停止条件 3 の A。`tests/explain.rs::the_docs_list_every_kind` も同じ本体を要求するので、両方が通ることを手順 1.4 で確かめる）。更新の手順（`diagnostics.md` の「kind の一覧」節）は「`node scripts/gen-docs.mjs` を実行する」に書き換える。

### 3. doc タグ（Phase 2）

**3.1 タグの行の規則**（`@deprecated` と同じ。`docs/language.md` の「ドキュメントコメント」の節に書く）: 説明の各行の先頭の空白を除いた最初の語が、タグそのものである行が「タグの行」。タグの直後は行末か空白（スペースかタブ）。`@complexityX` はタグではない。タグの値は、タグの後ろから行末まで、前後の空白を除いたもの（空でもよい）。コードフェンスの中の行も区別しない。タグの行も説明として表示される（`tsuzuri doc`・LSP のホバーは変えない）。

**3.2 3 つのタグ**

| タグ | 行の数 | 値の文法 | 意味 | 検査 |
| --- | --- | --- | --- | --- |
| `@complexity` | 最初の 1 行だけ（2 つ目以降は通常の説明） | `complexity = clause \| qualified ", worst " clause`、`clause = "O(" expr ")"`、`qualified = ("amortized" \| "expected") " " clause`。`expr = product { " + " product }`、`product = factor { " * " factor }`、`factor = "1" \| var \| [ var " " ] ("log" \| "log^2") " " var`、`var = "n" \| "m" \| "k"` | 1 回の呼び出しの最悪の時間（`worst` を伴う `amortized`・`expected` は、その前が平均・償却の値） | 文法: `gen-docs.mjs`（std の 8 モジュールの全公開 `def` に必須）。有無: `tests/std_tags.rs` |
| `@since` | 最初の 1 行だけ | `[0-9]+\.[0-9]+\.[0-9]+`（コンパイラの版 `Cargo.toml` の `version` 以下） | その宣言が最初に入った版。新しい公開関数から付ける（既存の関数には付けない） | 形と大小: `tests/std_tags.rs` |
| `@see` | 何行でも | `Module` か `Module.name`（`[A-Z][A-Za-z0-9]*(\.[A-Za-z_][A-Za-z0-9_]*)?`） | 関連する宣言。1 行に 1 つ | std では `tests/std_tags.rs` が、指す先が存在する公開 `def`（または std のモジュール）であることを検査する |

文法に合わない値は**コンパイラの診断にしない**（ユーザーのパッケージのタグは自由な説明）。検査するのは std の 8 モジュール（`gen-docs.mjs`）と std 全体（`tests/std_tags.rs`）だけで、診断コードは増えない。

**3.3 計算量の読み方**（`@complexity` の規則。generated ブロックの凡例にもなる）:

| 項目 | 規則 |
| --- | --- |
| 単位 | 1 回の呼び出しが返るまでの、要素の読み書き・比較・コールバック呼び出しの回数。コールバックと `Eq.eq`・`Ord.lt`・`Hash.hash`・`Copy` の複製は 1 回を `O(1)` と数える。確保は確保した大きさと数える |
| `n` | 主な入力の要素数（`string` ではコード単位数）。主な入力は、関数が読む・畳む・探す・複製する対象（`values`・`map`・`set`・`list`・`text`・`parts`）。引数の順序と関係なく、`String.find needle text` の `n` は `text` の長さ |
| `m` | もう 1 つの入力の要素数（`right`・他方の集合・`needle`・`separator`・`prefix`・`suffix`） |
| `k` | 新しいコレクションを作る関数で、結果の要素数が `n` と違うときの結果の要素数（`init`・`sub`・`slice`・`repeat`・`join`・`replace`・`with_capacity` の表の大きさ） |
| `Seq` を返す関数 | `Seq` を返すまでの計算量。取り出しは 1 要素ごとに `O(1)`（`Array.iter` は `O(1)`）。`List.iter` は先に参照を集めるので `O(n)` |
| `expected`・`amortized` | キーのハッシュが一様に分散するときの値（`HashMap`・`HashSet`）。`worst` は全キーが衝突するときの値 |
| `worst` の省略 | 1 つの clause だけなら、それが最悪の値 |

**3.4 置き場所と順序**: タグの行は、関数の `def` の直前の `///` の中に置く。順序は「説明（0 行以上）→ `///` だけの行（説明があるときだけ）→ `@complexity` → `@since` → `@see`（複数行）」。G34 の例の ```` ```tsuzuri ```` は、その後ろに `///` だけの行を 1 行置いてから続ける。説明が無い関数は、タグの行だけの doc になる（G34 が後から説明の 1 文をタグの前に足す。G34 の「説明が無い」は「タグの行とフェンス以外の行が無い」を指す）。

```tsuzuri
/// Returns a new array with a copy of every element, the explicit form of an implicit array copy.
///
/// @complexity O(n)
def copy :: Copy<'a> => ref ['a] -> ['a]
```

**3.5 コンパイラ側**: `Documentation` に `pub fn complexity(&self) -> Option<&str>`・`pub fn since(&self) -> Option<&str>`・`pub fn see(&self) -> Vec<&str>` を足す。共通の私的関数 `tag_notes` に `deprecation()` も寄せる（振る舞いは同じ。`deprecation_note_reads_the_first_tag_line` が守る）。`tsuzuri doc`・ホバー・`api-diff`（説明の変更は `non-breaking`）は何も変えない。

### 4. 計算量の表（Phase 2）

**ブロックの本体**（`complexity:Module`、`Module` は 8 モジュールのどれか）:

```text
| 関数 | 計算量 |
| --- | --- |
| `Map.empty` | $O(1)$ |
| `Map.insert` | $O(n)$ |
```

- 行は `tsuzuri doc std` のページに出る順（`std/Module.tz` の宣言の順）。公開 `def` の全部（`record`・`union`・`type` の宣言は除く）。
- 列は `関数`・`計算量`。**どれかの行に `@since` があれば `版` の列**（無い行は `-`）、**どれかの行に `@see` があれば `関連` の列**（各 ID を `` `Module.name` `` と書いて `, ` でつなぐ。無い行は `-`）を、この順に足す。両方の列が空なら足さない。
- 計算量のセル: 各 clause を `$O(…)$` にする。`expr` の `log` → `\log`、`log^2` → `\log^2`、` * ` → ` \cdot `。`amortized` → `償却 `、`expected` → `平均 `、`worst` → `最悪 `（clause の前に置く）。2 つの clause は `、` でつなぐ。例: `amortized O(1), worst O(n)` → `償却 $O(1)$、最悪 $O(n)$`、`O(n log^2 n)` → `$O(n \log^2 n)$`、`O(n * m + k)` → `$O(n \cdot m + k)$`。
- 8 モジュールの全公開 `def` に有効な `@complexity` が要る。無い・文法に合わない場合は `Module.name: missing @complexity` / `Module.name: invalid @complexity "VALUE"`（`--check` も書き込みも終了コード 1）。

**置き場**（Phase 2 の手順 2.6 で足す。計 16）:

| name | file |
| --- | --- |
| `complexity:Array` | `_tsuzuri/language-reference/built-in-types-and-modules/array.md`、`docs/language.md` |
| `complexity:Vec` | `…/vec.md`、`docs/language.md` |
| `complexity:Map`・`complexity:Set`・`complexity:HashMap`・`complexity:HashSet`・`complexity:List` | それぞれ `…/map.md`・`set.md`・`hashmap.md`・`hashset.md`・`list.md` と `docs/language.md` |
| `complexity:String` | `…/string.md`（新しい `## 計算量` の節）、`docs/language.md` |

同じ `name` が 2 つのファイルにあるのは、ファイルごとに 1 回なので重複ではない。ページごとの手書きの残り（組み込み操作の表・凡例の 1 文・補足の文）は付録 E.3 の表のとおり。

### 5. benchmarks のデータと表（Phase 3）

**`docs/benchmarks-data/summary.json`**（スキーマ `tz-bench-summary/1`）のキーはこの順: `schema`・`measured_at`（結果の `measured_at` の最大。無ければ `"未記録"`）・`cpu_mode`（`generic` か `native`）・`scale`（正の有限数）・`runs`（中央値を取った結果の数。1 以上）・`environment`（`cpu`・`architecture`・`tsuzuri_sha256`・`csharp`・`javascript`。すべて string）・`headers`（導出列を除く見出し。HEAD は `種目`・`仕事量`・`Tsuzuri (ms)`・`C (ms)`・`C++ (ms)`・`Rust (ms)`・`C# (ms)`・`JavaScript (ms)`）・`timing_columns`（時間の列の添字。HEAD は `[2,3,4,5,6,7]`）・`rows`（各行は `headers` と同じ長さ。セルは**表示の文字列**: `"1.304265"`・`"-"`・`"800000"`・`"1024 x 1024"`。時間のセルは `-` か正の有限数）。

**`docs/benchmarks-data/metrics.json`**（`tz-bench-metrics/1`）: `{ schema, runs: [{ run_id, date, commit, dirty, host, tables }] }`。`commit` は 12 桁の 16 進、`dirty` は boolean、`host` は `cpu`・`arch`・`cores`（整数）・`os`・`os_version`・`clang`（string か `null`）・`rustc`（同）・`node`・`tsuzuri_sha256`、`tables` は `[{ caption, headers, rows }]`（`rows` の各行は `headers` と同じ長さの string 配列）。直列化は両方とも `JSON.stringify(value, null, 2) + "\n"`。検査関数は `validateSummary`・`validateMetrics`（`benchmarks/render.mjs`）。

**描画・関数の対応・移行**（[付録 S](G35-appendix-spec.md)）: `renderTable` の規則（勝者は `**値**`、導出列 `観測最速` と `Tsuzuri 相対性能`、`cell()` のエスケープ）は S.1、旧関数から新関数への対応は S.2、移行の決定表（何が JSON になり、何が固定文になり、何を触らないか）は S.3。要点: 色・`style`・`data-*` は無くなる。`publishSummary`・`publishMetrics` は JSON を書いてから、文書のブロックだけを置き換える。不正な入力では何も書かない。`renderComparison`・`compareRuns` は変えない。

**リンク検査** `node scripts/check-links.mjs [--into PATH]... [--list-headings PATH]`: `git ls-files '*.md'` の各ファイルの相対リンク（コードフェンスの外）のうち、行き先が `--into` のファイル（既定 `docs/benchmarks.md`）のものについて、ファイルの存在と断片の見出し（`check-docs.mjs` の `headings` と同じ規則）を調べる。stdout `check-links: 70 links into docs/benchmarks.md (46 with a fragment, 17 distinct), 0 broken`、壊れたものは stderr に `FILE: missing heading TARGET` で、終了コード 1。`--list-headings PATH` は見出しの slug を文書の順に 1 行ずつ出して終了コード 0（`PATH` は実行したディレクトリからの相対パス）。

### 6. `_specs/`（Phase 4）

`git mv _specs/*.md _specs/_archive/` で 5 ファイルを移し、`_specs/README.md`（付録 E.4 の全文）を置く。内容は 1 字も変えない。取り込む項目は 0（付録 C の決定表: 全 46 項目のうち、現行に同じ形である 40 項目と、現行が異なる 6 項目に分け、すべて「破棄（移すだけ）」）。`try-with-finally.md` の「注意点」の 3 つ目の箇条を付録 E.4 の文に直す。残りの参照（GUIDE の D-34 の節と、コード・テストのコメント）は履歴なので直さず、台帳への追記を報告する。

### 7. ドリフト検査 `scripts/check-cli-docs.mjs`（Phase 4）

`node scripts/check-cli-docs.mjs [--only cli|codes|version|std]... [--bin PATH]`（既定は 4 つ全部。`--bin` は `gen-docs.mjs` と同じ）。stdout に `ok NAME: …`、stderr に `FAIL NAME: …`、最後に `check-cli-docs: 4 checks, 0 failed`。終了コード: 全部 ok で 0、失敗 1、使い方の誤りとコンパイラ無しは 2。

4 つの検査の抽出規則・検査される文書・規則・基線は [付録 S.4](G35-appendix-spec.md) の表、参照コードは付録 D.7。要点:

- `cli`: 旗と環境変数は README と `option.md` の両方、サブコマンドは README に含まれる。免除は `--sysroot` だけ。基線の失敗 8 件（README 6・`option.md` 2）。
- `codes`: `compiler/diagnostics.md` と `docs/language.md` の表の集合が、発行される集合（G23 があればカタログ、なければ `src/` の走査）に一致する。基線の失敗 1 件（`docs/language.md` の `W2001`）。
- `version`: `Cargo.toml` の版・`--version`・文書の `Tsuzuri X.Y.Z` がすべて一致する。基線 22 件・失敗 0。
- `std`: 559 個の公開 `def` が、そのモジュールのページに言及される。基線 559/559・失敗 0。

### 8. 診断・互換性・決定性

- **診断コード**: 追加なし。スクリプトのエラーはコンパイラの診断ではない（D-55 の予約を使わない）。タグの文法違反もコンパイラは報告しない。
- **互換性**（`docs/stability.md`）: タグは説明の本文で、追加・変更は「説明の変更」＝互換性を保つ変更（`tsuzuri api-diff` は `non-breaking: documented def …`）。edition は変えない（予約語・構文・属性を足さない。D-45・D-54）。IR・ABI・WASM の import・`unsafe`・crate は変わらない。
- **決定性**: 生成物は入力（`tsuzuri doc std`・カタログ・JSON）の純関数。日時・絶対パス・環境を入れない。`gen-docs.mjs` は 2 回続けて実行すると 2 回目は何も書かない。行の順序は入力の順序（std は宣言順、カタログはコード昇順・kind は id 昇順、JSON は配列順）。`gen-docs.mjs --check` は `doc std` と `explain --catalog --json` を各 1 回（0.3 秒）呼ぶだけ。

## 設計

### 新規ファイルと関数

| ファイル | 内容（名前は固定） |
| --- | --- |
| `scripts/gen-docs.mjs`（新規） | export は付録 D.2 の `export` の全部（`PLACEMENTS`・`scanBlocks`・`bodyOf`・`replaceBlock`・`replaceNamedBlock`・`diagnosticKindsTable`・`tagNotes`・`parseComplexity`・`renderComplexity`・`parseStdPage`・`complexityTable`・`planBlocks`・`checkBlocks`・`writeBlocks`・`createContext`・`readStd`・`main`。名前と引数を変えない）。`context = { root, hasExplain(), catalog(), std() }`（`createContext` が作る。テストは同じ形の偽の context を渡す）。`main` は `process.argv[1]` が自分のときだけ動く |
| `benchmarks/render.mjs`（新規） | export: `cell`、`renderTable`、`renderSummaryBlock(summary)`、`renderMetricsBlock(metrics)`、`validateSummary`、`validateMetrics`、`serializeJson`。依存は標準ライブラリだけ |
| `benchmarks/report.mjs`・`metrics.mjs` | 付録 S.2 の「関数の対応」の表のとおり。実装は付録 D.8（`report.mjs` の全文）・D.9（`metrics.mjs` の 4 か所） |
| `scripts/check-links.mjs`（新規） | 「リンク検査」のとおり。`headings` は `check-docs.mjs` と同じ式を複製（関数 1 つ。共有モジュールにしない: `check-docs.mjs` は import しただけで走る） |
| `scripts/check-cli-docs.mjs`（新規） | export は付録 D.7 の `export` の全部（`extractCli`・`checkCli`・`documentedCodes`・`scanEmittedCodes`・`emittedCodes`・`checkCodes`・`versionMentions`・`checkVersion`・`STD_PAGES`・`stdPage`・`checkStd`・`CHECKS`・`runChecks`・`main`。名前と引数を変えない）。`check*` はメッセージの文字列の配列を返し、`runChecks` は各 `{ name, summary, messages }` を返す。`createContext` は `gen-docs.mjs` から import |
| `src/syntax.rs` | `Documentation::{complexity, since, see}` と私的 `tag_notes`（付録 D.1） |
| `tests/{gen_docs,check_links,check_cli_docs}.mjs`・`tests/{doc_tags,std_tags}.rs` | 「テスト計画」 |

### アルゴリズム

**ブロックの計画 `planBlocks(context, { only, files, placements })`**（付録 D.2）: 置き場を `only`・`files` で絞り、対象ファイルを 1 回ずつ読み（`\r` があればエラー）、`scanBlocks` する。置き場に無い名前のブロックは `unknown block`、置き場のブロックが無ければ `missing block`、`requires: 'explain'` で `hasExplain` が偽なら `skipped`。`expected` は名前に応じた生成関数の結果、`actual` は `bodyOf`。**`checkBlocks`** は 2 つが行の配列として違う置き場を `stale` に入れる。**`writeBlocks`** はファイルごとにブロックを後ろから `replaceBlock` し（行番号がずれない）、変わったときだけ書く。計算量の表の検査は仕様 3.2・4、移行は手順 3.4・3.5 と付録 D.4、ドリフト検査は付録 D.7。

## 前提とする他チケットのインターフェース

| チケット | 使うもの | 未実装のとき |
| --- | --- | --- |
| G23 | `tsuzuri explain --catalog --json`（1 行 1 オブジェクト `{"kind","code","severity","since","title","page"}`。コード昇順・id 昇順で出る）。`compiler/diagnostics.md` の `<!-- generated:diagnostic-kinds -->` ブロック（G23 手順 4.2 が置く）。`tests/explain.rs::the_docs_list_every_kind` | `hasExplain` が偽でブロックを検査しない（`skip` の行）。`codes` 検査は `src/` の走査 |
| G34 | std の `///` に同じ関数の例を足す。`tsuzuri doc` の出力で、例の隠し行を除き `Output:` を足す（タグの行には影響しない） | 順序は「仕様 3.4」だけで足りる。G34 が先なら、タグの行を説明の後ろ・例の前に足す |
| C12 | Map・Set の `insert`・`remove` の計算量を変え、公開関数 `chunk_count`・`invariants_hold` を足す | C12 が先: 手順 2.5 の Map・Set は C12 の §5 の値と上の 2 関数で付ける。G35 が先: C12 の手順 3 の (1b) が `@complexity` を直し、`map.md`・`set.md` のブロックの外の 1 文を直して `node scripts/gen-docs.mjs` で表を作り直す（生成した本体は手で直さない） |
| G21・G26・G27 | 本チケットが出す `Documentation::{complexity, since, see}` と、`tsuzuri doc std` のタグの行 | 使わなくてよい。本チケットは KB の欄・`guide std` の出力を変えない |
| G33（D-58） | 規則 ID のマーカー `<!-- rule: AREA-NNN -->` は `<!-- generated:` と衝突しない。G33 の `<!-- generated:spec-coverage -->` ブロック（`conformance.md`）は `node scripts/check-spec.mjs --write` が**唯一の書き手**で、`PLACEMENTS` に足さず、`gen-docs` は走査しない | 本チケットは G33 のためのブロック名を予約しない |
| G31・G21 | G31 の `docs/ai-cost.md` は同じ対のマーカー `<!-- generated:ai-cost -->`／`<!-- /generated:ai-cost -->` を使い、本体は `benchmarks/ai/run.mjs report --publish` だけが書く。G21 の `guide/` の生成節（Phase 4）も同じ対の記法で、`generator(NAME)` が作る。どちらも `PLACEMENTS` に足さず、`gen-docs` は走査しない | 本チケットは G31・G21 のブロック名を予約しない |
| D-50・D-55・D-56 | 診断コード・名前・サイズ上限 | 診断コードを増やさない。新しい名前（スクリプト・マーカー・タグ・スキーマ）は D-55 の対象外で、台帳へ追記を依頼する |

## 実装手順

各手順の最後でビルドとテストが緑。**手順の見出しの「確認」は、そのまま実行して期待の出力を得る。**

### Phase 1

**手順 1.1: ブロックの走査（テスト先行）。** 変更: `tests/gen_docs.mjs`（新規）、`scripts/gen-docs.mjs`（新規）。内容: 先に `tests/gen_docs.mjs` に `scanBlocks` のテストを書く（付録 F.1 の 8 件: 正常とフェンスの中、`replaceBlock`、入れ子、開始だけ、終了だけ、重複、先頭に空白、本文が空）。`scripts/gen-docs.mjs` に `scanBlocks`・`bodyOf`・`replaceBlock` を付録 D.2 のとおり書き、`main` の骨組み（`process.argv[1]` の判定、未知の旗で終了コード 2）を置く。確認: `node tests/gen_docs.mjs` が終了コード 0 で、`gen-docs: <テスト名> passed` の行をちょうど 8 行出す（`tests/benchmark_report.mjs` の `Metrics:` と同じ書き方: `const test = (name, check) => { check(); console.log(`gen-docs: ${name} passed`); };` を定義し、付録 F.1 の 1 行につき `test` を 1 回呼ぶ。以降、この形の行だけを出して終了コード 0 になることを「緑」と呼ぶ）。

**手順 1.2: 診断 kind の生成・`PLACEMENTS`・`checkBlocks`・`writeBlocks`。** 変更: 同 2 ファイル。内容: 先に付録 F.2 のテスト（偽の context と `placements` を渡し、カタログの 3 行から表を作る・`--check` が一致で 0、ずれで 1 と報告・書き込みを 2 回続けても 2 回目は何も更新しない・`hasExplain` が偽で `skip`・ブロックが無いと `missing block`・`unknown block`）を書く。次に付録 D.2 の `diagnosticKindsTable`・`planBlocks`・`checkBlocks`・`writeBlocks`・`createContext` と、`--list`・`--only`・`--bin` を含む `main` を書く。確認: `node tests/gen_docs.mjs` が緑（F.1 の 8 行と F.2 の 8 行で `passed` の行が 16）。`node scripts/gen-docs.mjs --list` が `diagnostic-kinds<TAB>_tsuzuri/language-reference/compiler/diagnostics.md` の 1 行。`node scripts/gen-docs.mjs --check`（G23 が無い HEAD）が `skip diagnostic-kinds: …` と `gen-docs: 0 blocks checked, 0 stale`・終了コード 0。

**手順 1.3: `check-docs.mjs` からの呼び出し。** 変更: `scripts/check-docs.mjs`。内容: 先頭に `import { checkBlocks } from './gen-docs.mjs';`、ページのループのあとに `checkBlocks` を呼んで `assert.equal(stale.length, 0, stale.join('\n'))`、最後の行を `Docs: … , N generated blocks.` に変える。確認: `node scripts/check-docs.mjs _tsuzuri/language-reference/compiler/diagnostics.md` が通り、最後の行が `…, 0 generated blocks.`（G23 が無いとき）。

**手順 1.4: G23 のブロックとの突き合わせ。** 変更: `_tsuzuri/language-reference/compiler/diagnostics.md`（G23 が出荷済みのときだけ）。内容: G23 が出荷済みなら、`<!-- generated:diagnostic-kinds -->` の現物と `node scripts/gen-docs.mjs --check` を比べ、違えば `DIAGNOSTIC_KINDS_HEADER` を現物に合わせる（停止条件 3）。「kind の一覧」節の更新手順を「`node scripts/gen-docs.mjs` を実行する」に書き換える。出荷済みでなければ何もしない（この手順の確認は「何も変えていない」: `git diff --stat -- _tsuzuri` が空）。確認: G23 あり: `node scripts/gen-docs.mjs --check` と `cargo test --locked --test explain the_docs_list_every_kind` が両方緑。

**手順 1.5: `docs.yml` と文書。** 変更: `.github/workflows/docs.yml`（新規。付録 D.5 の全文）、`README.md`（「テストスイートの実行」のコードブロックに `node scripts/gen-docs.mjs --check` と `node tests/gen_docs.mjs` を足す）、`AGENTS.md`（`check-docs.mjs` の箇条の次に「生成ブロックを持つページを直したら `node scripts/gen-docs.mjs` を実行する」を足す）、`docs/architecture.md`（スクリプトの 1 行）。確認: 使い捨てのブランチを push して `gh run list --workflow docs.yml` の最新が成功（`gh run view <run> --log-failed` が空）。ブランチと run は消す。

### Phase 2（Phase 1 が済んでから）

**手順 2.1: `Documentation` の読み取り関数（テスト先行）。** 変更: `tests/doc_tags.rs`（新規）、`src/syntax.rs`。内容: 先に `tests/doc_tags.rs` を書く（付録 F.3 の 3 件。`Documentation { text, span: Span::default() }` を作る）。次に付録 D.1 のとおり `tag_notes`・`complexity`・`since`・`see` を足し、`deprecation()` を `self.tag_notes("@deprecated").next()` に寄せる。確認: `cargo test --locked --test doc_tags` が 3 件、`cargo test --locked --lib warnings` が緑（`deprecation_note_reads_the_first_tag_line`）、`cargo clippy --all-targets -- -D warnings` が緑。

**手順 2.2: タグの文法と計算量の表の関数（テスト先行）。** 変更: `scripts/gen-docs.mjs`、`tests/gen_docs.mjs`、`tests/fixtures/gen-docs/std-doc/Sample.md`（付録 F.4）。内容: 先に付録 F.5 のテスト（`tagNotes` の 11 件の表、`parseComplexity` の正 13 件・誤 16 件、`renderComplexity` の 5 件、`parseStdPage`、`complexityTable` の列の出入り・`missing`・`invalid` のメッセージ）を書く。次に付録 D.2 の関数を書く。確認: `node tests/gen_docs.mjs` が緑。

**手順 2.3: `tests/std_tags.rs`（ratchet）。** 変更: `tests/std_tags.rs`（新規）。内容: `const TAGGED: [&str; 0] = [];`（完全にタグ付けしたモジュール）と、3 件のテスト: `tagged_modules_have_a_complexity_on_every_public_function`（`TAGGED` の各モジュールの `std/<M>.tz` を `stdlib::SOURCES` から取り、`tsuzuri::parser::parse` した `program.functions` の `visibility == Visibility::Public` の全部で `doc.as_ref().and_then(Documentation::complexity)` が空でない）、`since_tags_are_versions_not_newer_than_the_crate`（全 std の `since()` が `^\d+\.\d+\.\d+$` で、`env!("CARGO_PKG_VERSION")` 以下）、`see_tags_name_existing_functions`（全 std の `see()` の各値が `Module.name`（std の公開 `def`）か `Module`）。確認: `cargo test --locked --test std_tags` が 3 件緑（`TAGGED` が空なので最初の 1 件は何も調べない）。

**他のチケットが公開関数を足した木**: 付録 B の表と下の `grep -c` の数は HEAD のもの。F15 が済んでいれば `Array.fold_monoid`（`@complexity O(n)`・`@see Array.fold`）、C12 が済んでいれば `Map`・`Set` の `chunk_count`（`O(1)`）・`invariants_hold`（`O(n)`）にも同じ規則でタグを付ける（数が増える。付けないと `tests/std_tags.rs` が赤。G35 が先なら足す側が付ける）。付録 E.3 の 1 文は、C12 が済んでいれば C12 の仕様 §5 の文にする。

**手順 2.4: Array のタグ（39 関数）。** 変更: `std/Array.tz`（`///` の追加だけ）、`tests/std_tags.rs`（`TAGGED` に `"Array"`）。内容: 付録 B.1 の表のとおり、各 `def` の直前の `///` に `@complexity` と `@see`（表にあれば）を「仕様 3.4」の順序で足す。説明がある関数は、説明の後ろに `///` だけの行を 1 行置いてから足す。説明が無い関数は `/// @complexity …` の行だけ。付録 B の「説明の追加」欄に文がある関数は、その文を説明に足す。確認: `cargo test --locked --test std_tags` が緑。`target/release/tsuzuri doc std -o <dir>`（ディレクトリ `std` を直接読むので、`std/*.tz` を直しても再ビルドは要らない）の `Array.md` に `grep -c "^@complexity" <dir>/Array.md` が 39。`git diff --stat -- std/Array.tz` が追加行だけ（削除 0）。

**手順 2.5: Vec・Map・Set・List（33 関数）。** 変更: `std/{Vec,Map,Set,List}.tz`、`TAGGED` に 4 つ。内容・確認: 手順 2.4 と同じ（付録 B.2・B.3・B.4・B.7。`grep -c` は Vec 1・Map 14・Set 13・List 5）。

**手順 2.6: HashMap・HashSet（41 関数）。** 変更: `std/{HashMap,HashSet}.tz`、`TAGGED` に 2 つ。内容・確認: 同じ（付録 B.5・B.6。`grep -c` は HashMap 24・HashSet 17。`@see` の行は HashMap 8・HashSet 4）。

**手順 2.7: String（21 関数）。** 変更: `std/String.tz`、`TAGGED` に `"String"`。内容・確認: 同じ（付録 B.8。`grep -c` は 21）。`TAGGED` が 8 モジュールすべて。

**手順 2.8: 仕様の文書。** 変更: `docs/language.md`（「ドキュメントコメント」の節に付録 E.1 の段落）、`_tsuzuri/language-reference/organizing-tsuzuri/documentation-comment.md`（新しい節 `## 構造化タグ` を `### 警告 W1005` の節のあと・`## tsuzuri doc によるドキュメント生成` の前に。付録 E.1）、`docs/stability.md`（「公開 API の差分」の「互換性を保つ変更」に `@complexity`・`@since`・`@see` の追加と変更）、`docs/architecture.md`（`src/syntax.rs` の行に 3 関数）。確認: `node scripts/check-docs.mjs _tsuzuri/language-reference/organizing-tsuzuri/documentation-comment.md` が通る。

**手順 2.9: 計算量の表の置き場と CI。** 変更: 8 ページ（付録 E.3 の表）、`docs/language.md`（新しい `### 標準コレクションの計算量` を `### 配列・リスト API` の後・`### 演算子` の前に。`### HashMap / HashSet` の古い表を 1 文の案内に）、`scripts/gen-docs.mjs`（`PLACEMENTS` に 16 件、`GENERATORS` に `complexity:*`）。内容: 各ページの手書きの表を、付録 E.3 に従い「凡例の 1 文 + マーカーのペア + 手書きの残りの表」に直し、`node scripts/gen-docs.mjs` で本体を作る（`gen-docs.mjs` は `target/release/tsuzuri doc <リポジトリ>/std` を読み、ディレクトリ `std` をディスクから読むので、`std/*.tz` を直した直後でも再ビルドは要らない）。`docs.yml` は変えない（`gen-docs.mjs --check` が新しい 16 件を検査する。Rust のテストは `targets.yml` の「Rust tests」が走らせる）。確認: `node scripts/gen-docs.mjs` が `16 blocks checked, 16 updated`、続けて `--check` が `16 blocks checked, 0 stale`（2 回目の書き込みは `0 updated`）。`node scripts/check-docs.mjs <変えた 8 ページ>` が通る。`grep -c "^#" docs/language.md` が変更前より 1 だけ増える。使い捨てのブランチで `docs.yml` が成功。

### Phase 3（Phase 1 が済んでから）

**手順 3.1: リンク検査（移行の前に緑）。** 変更: `scripts/check-links.mjs`・`tests/check_links.mjs`（新規）。内容: 先に `tests/check_links.mjs`（付録 F.6: 偽の文書の木で、壊れたファイル・壊れた断片・フェンスの中・`--list-headings`）を書き、次に「リンク検査」の仕様どおり書く。**移行の前の基線を取る**: `node scripts/check-links.mjs --list-headings docs/benchmarks.md > target/g35-headings-before.txt`。確認: `node scripts/check-links.mjs` が `check-links: 70 links into docs/benchmarks.md (46 with a fragment, 17 distinct), 0 broken`（停止条件 6）。

**手順 3.2: 新しい描画（追加だけ）。** 変更: `benchmarks/render.mjs`（新規）、`tests/benchmark_render.mjs`（新規。付録 F.7 の全文）、`benchmarks/legacy-report.mjs`（`git show HEAD:benchmarks/report.mjs > benchmarks/legacy-report.mjs`。手順 3.7 で消す）。内容: 先に付録 F.7 のテストを写して、`render.mjs` が無いので失敗するのを確かめ（`renderTable` の勝者・同率・条件差・`<0.00001`・エスケープ・コード span、`renderSummaryBlock`、`renderMetricsBlock`、`validateSummary`・`validateMetrics` の拒否、`serializeJson` の 8 件）、付録 D.3 の関数を書く。確認: `node tests/benchmark_render.mjs` が `Render: … passed` を 8 行出して終了コード 0。`node tests/benchmark_report.mjs` は 34 行のまま変わらない（既存の `report.mjs`・`metrics.mjs` に触っていない）。

**手順 3.3: `report.mjs`・`metrics.mjs` の切り替え。** 変更: `benchmarks/report.mjs`（付録 D.8 の全文）、`benchmarks/metrics.mjs`（付録 D.9 の 4 か所）、`tests/benchmark_report.mjs`（付録 D.10。付録 S.5 の表を実装したもの）、`scripts/gen-docs.mjs`（付録 D.11 の 1・2: `benchmark-summary`・`perf-metrics` の生成関数。**`PLACEMENTS` にはまだ足さない**）。内容: 上の付録のとおり。`publishSummary`・`publishMetrics` は、置き換え後の文書を先に作り（マーカーが無ければここで失敗して何も書かない）、JSON、文書の順に `writeAtomically` で書く。確認: `node tests/benchmark_report.mjs` が 34 行（`Benchmark report: … passed` が 1 行、`Metrics: … passed` が 33 行）で終了コード 0。`node benchmarks/metrics.mjs report <dir> --baseline <dir>` の出力が変更前と同じ（`renderComparison` に触っていない: テスト `comparison table` が守る）。`node scripts/gen-docs.mjs --check` は変わらず `0 blocks checked`。**手順 3.3〜3.5 は 1 つの PR にまとめる**（3.3 の後、3.5 の前は、`docs/benchmarks.md` に新しいマーカーが無いので `publishSummary` が `add the generated:benchmark-summary markers …` で失敗する）。

**手順 3.4: 移行スクリプト。** 変更: `benchmarks/migrate-html.mjs`（新規。付録 D.4 の全文）。内容: 付録 D.4 を写す。**テストの代わりに書き込み前の検査を先に通す**: `node benchmarks/migrate-html.mjs --check` は何も書かずに解析と往復検査を行い、3 行（`summary: headers 8 rows 38 round trip true`・`metrics: runs 2 tables 36 rows 314 cells 2109 round trip true`・`outside the blocks: unchanged`）と `--check: nothing written` を出す。スクリプトは `docs/benchmarks.md` を読み、2 ブロックを解析して往復検査し、通ったときだけ `summary.json`・`metrics.json` を書き、文書のブロックとマーカーを置き換える。**検査が通らなければ何も書かない。** 確認: `node benchmarks/migrate-html.mjs --check` が上の 4 行を出して終了コード 0。`git status --short -- docs` が空（`--check` は何も書かない）。

**手順 3.5: 移行の実行と検証。** 変更: `docs/benchmarks-data/summary.json`・`metrics.json`（新規）、`docs/benchmarks.md`。内容: `node benchmarks/migrate-html.mjs` を実行する。検証を全部行う: (1) 往復が一致（スクリプトが `summary: headers 8 rows 38 round trip true`・`metrics: runs 2 tables 36 rows 314 cells 2109 round trip true` を出す）、(2) `rows.length` 38、`runs` 2、`tables` 合計 36、本体の行 314・セル 2,109 が JSON と一致、(3) ブロックの外の行が 1 byte も変わらない（スクリプトが `outside the blocks: unchanged` を出す。手動では移行前後のファイルからブロックを除いて `cmp`）、(4) `node scripts/check-links.mjs --list-headings docs/benchmarks.md` が `target/g35-headings-before.txt` と同一、(5) `node scripts/check-links.mjs` が 70 リンク・0 broken、(6) `wc -c docs/benchmarks.md` が 520,000 以下（試作で 489,211）。確認: 上の 6 つ。(1) か (3) が違えば停止条件 2。

**手順 3.6: 置き場の有効化と散文。** 変更: `scripts/gen-docs.mjs`（付録 D.11 の 3: `PLACEMENTS` に `benchmark-summary`・`perf-metrics`、いずれも `docs/benchmarks.md`）、`docs/benchmarks.md`（散文 3 か所を付録 E.5 の文に）、`docs.yml`（`node tests/benchmark_render.mjs`・`node tests/check_links.mjs`・`node scripts/check-links.mjs` を足す）。確認: `node scripts/gen-docs.mjs --check` が `…, 0 stale`（ブロック数 +2）。

**手順 3.7: 一時物の削除。** 変更: `benchmarks/migrate-html.mjs`・`benchmarks/legacy-report.mjs` を削除。確認: `node tests/benchmark_report.mjs` が緑。`git grep -n "legacy-report\|migrate-html" -- . ':!_features'` が 0 件。`cargo test --locked --test docs` が緑。

### Phase 4（手順 4.1 以外は Phase 1 が済んでから）

**手順 4.1: `_specs/`。** 変更: `git mv _specs/*.md _specs/_archive/`、`_specs/README.md`（新規。付録 E.4）、`try-with-finally.md`（付録 E.4 の箇条）。確認: `ls _specs` が `README.md _archive`、`ls _specs/_archive | wc -l` が 5、`git diff -M --stat` で 5 ファイルが rename（100%）、`node scripts/check-docs.mjs _tsuzuri/language-reference/exception-handling/try-with-finally.md` が通る。

**手順 4.2: `check-cli-docs.mjs` の骨組みと `version`（テスト先行）。** 変更: `tests/check_cli_docs.mjs`（付録 F.8）、`scripts/check-cli-docs.mjs`（新規）。内容: 先にテストを書き、`versionMentions`・`checkVersion`・CLI の枠（`--only`・終了コード・出力）を書く。確認: `node tests/check_cli_docs.mjs` が緑。`node scripts/check-cli-docs.mjs --only version` が `ok version: 0.1.0 (22 mentions)`。

**手順 4.3: `cli`。** 変更: `scripts/check-cli-docs.mjs`、`README.md`・`compiler/option.md`（付録 E.2 の 8 件）、`tests/check_cli_docs.mjs`。内容: 先に `extractCli` のテスト（`--help` の断片から旗・サブコマンド・環境変数を取り、`--sysroot` を免除する）を書き、`checkCli` を書く。まず文書を直す前に実行して 8 件が失敗することを確かめ、それから直す。確認: 直す前 `FAIL cli:` が 8 行、直した後 `ok cli: 38 flags, 16 commands, 10 variables`。

**手順 4.4: `codes`。** 変更: `scripts/check-cli-docs.mjs`、`docs/language.md`（`W2001` の行。付録 E.2）、テスト。内容: 先に `documentedCodes`（範囲・結合セルの展開）と `emittedCodes`（走査と、偽のカタログ）のテストを書く。確認: 直す前 `FAIL codes: W2001 is emitted but missing from docs/language.md`、直した後 `ok codes: 47 codes`。

**手順 4.5: `std`。** 変更: `scripts/check-cli-docs.mjs`、テスト。内容: 先に偽のページと `STD_PAGES` のテストを書き、`checkStd` を書く（`readStd` は `gen-docs.mjs` の `createContext(...).std()`）。確認: `ok std: 559 public functions in 42 modules`（`tsuzuri doc std` の 47 ページのうち、公開 `def` を持つのが 42 モジュール）。

**手順 4.6: CI と文書。** 変更: `docs.yml`（`node scripts/check-cli-docs.mjs` と `node tests/check_cli_docs.mjs`）、`README.md`（「テストスイートの実行」に 2 行）、`AGENTS.md`（1 行）。確認: 使い捨てのブランチで `docs.yml` が成功。`node scripts/check-cli-docs.mjs` が `check-cli-docs: 4 checks, 0 failed`。

## テスト計画

| テスト（ファイル::名前） | 入力 | 期待 | Phase |
| --- | --- | --- | --- |
| `tests/gen_docs.mjs`: `scanBlocks`・`replaceBlock` | 付録 F.1 の 8 件 | メッセージが「仕様 1」の表と一致 | 1 |
| 同: `diagnosticKindsTable`・`checkBlocks`・`writeBlocks`・`skip` | 偽の context とカタログ 3 行（付録 F.2） | 表が F.2 のとおり。`--check` が一致で 0・ずれで 1。2 回目の書き込みは何も書かない。`hasExplain` が偽で `skip`。`missing block`・`unknown block` | 1 |
| 同: `tagNotes`・`parseComplexity`・`renderComplexity`・`parseStdPage`・`complexityTable` | 付録 F.4・F.5 | 正 13・誤 16・描画 5・列の出入り・メッセージ。`tagNotes` は Rust の `deprecation_note_reads_the_first_tag_line` と同じ 10 件で同じ結果（規則の一致） | 2 |
| `tests/doc_tags.rs`: `tag_notes_follow_the_deprecated_rule`・`complexity_since_and_see_read_their_lines`・`deprecation_is_unchanged` | 付録 F.3 | `deprecation()` の値が変更前と同じ。2 つ目の `@complexity` と `@complexityX` は無視。`see()` は全行 | 2 |
| `tests/std_tags.rs`: `tagged_modules_have_a_complexity_on_every_public_function`・`since_tags_are_versions_not_newer_than_the_crate`・`see_tags_name_existing_functions` | `stdlib::SOURCES` | 手順 2.3。最終的に `TAGGED` が 8 モジュール | 2 |
| `tests/benchmark_render.mjs`（新規） | 付録 F.7 | `Render: … passed` が 8 行・終了コード 0 | 3 |
| `tests/benchmark_report.mjs`（HTML の検査の書き換え） | 付録 S.5・D.10 | 34 行・終了コード 0（基線と同じ行数） | 3 |
| `tests/check_links.mjs` | 偽の木（付録 F.6） | 壊れたファイル・壊れた断片・フェンスの中・`--list-headings` | 3 |
| `tests/check_cli_docs.mjs` | 偽の `--help`・偽の文書・偽のカタログ（付録 F.8） | `extractCli`・`documentedCodes`・`emittedCodes`・`versionMentions`・`STD_PAGES`・終了コード | 4 |

**期待値の出どころ**: 計算量の値は付録 B（実装を読んで手で導いた。既存の手書きの表の 53 行と比べ、一致しないのは `sort_in_place` だけ）。文法の正誤は列挙。HEAD の数は付録 A のコマンドの出力。benchmarks の移行は旧 `renderTable` による往復（現物の HTML が外部の参照）。**既存テストへの影響**: `tests/benchmark_report.mjs` の HTML を検査する箇所だけ期待が変わる（付録 S.5）。これ以外にはない。Phase 2 は `///` の行を足すだけで IR・診断・`tsuzuri doc` のページ構造が変わらず（`tests/docgen.mjs` は `sum_pairwise` の有無と節の形だけを見る）、`git grep -E "std/[A-Za-z0-9]+\.t[zc]:[0-9]+" -- tests src` は 0 件で std の行番号を固定するテストもない。

## ドキュメント

| 文書 | Phase | 更新 |
| --- | --- | --- |
| `README.md`・`AGENTS.md` | 1・4 | 「テストスイートの実行」と AGENTS.md の `check-docs.mjs` の箇条に、`gen-docs.mjs --check`・`check-cli-docs.mjs`・`check-links.mjs` と、生成ブロックのページを直したら `node scripts/gen-docs.mjs` を実行すること。README の CLI 書式・主要オプション・環境変数の 6 件（付録 E.2）。`Map` の説明は直さない（C12） |
| `docs/language.md` | 2・4 | 「ドキュメントコメント」にタグ（付録 E.1）、新しい `### 標準コレクションの計算量`、`### HashMap / HashSet` の古い表を案内に、診断の表に `W2001` の行 |
| `…/organizing-tsuzuri/documentation-comment.md` | 2 | 新しい `## 構造化タグ`（付録 E.1） |
| `…/built-in-types-and-modules/{array,vec,map,set,hashmap,hashset,list,string}.md` | 2 | `## 計算量` の置き換え（付録 E.3） |
| `…/compiler/diagnostics.md`・`option.md`・`exception-handling/try-with-finally.md` | 1・4 | 「kind の一覧」の更新手順（G23 の後）、サブコマンドの文に `fetch`・`publish`（付録 E.2）、「注意点」の 3 つ目の箇条（付録 E.4） |
| `docs/stability.md`・`docs/architecture.md` | 2 | 互換性を保つ変更にタグ。`src/syntax.rs` の行に 3 関数。スクリプトの 1 行 |
| `docs/benchmarks.md`・`_specs/README.md`（新規） | 3・4 | 2 ブロック（生成）と散文 3 か所（付録 E.5）。`_specs/README.md`（付録 E.4） |
| `_features/GUIDE.md`・台帳 | — | 変更しない。報告に追記を書く（新しい名前・doc タグ・`generated:` マーカー・`docs.yml`・`_specs/_archive/`） |

`index.md` はページを増やさないので変えない。例（` ```tsuzuri `）を足さない。検証: 各 Phase の最後に `node scripts/gen-docs.mjs --check`、変えたページごとに `node scripts/check-docs.mjs <ページ>`。

## 受け入れ条件

**Phase 1**
- [ ] `node tests/gen_docs.mjs` が緑。`node scripts/gen-docs.mjs --list`・`--check`・`--only`・未知の旗の終了コードが「仕様 1」の表のとおり。
- [ ] G23 が無い HEAD で `--check` が `skip diagnostic-kinds: …` を出して 0。G23 があれば `diagnostics.md` のブロックが `--check` と `the_docs_list_every_kind` の両方に通る。
- [ ] `check-docs.mjs` の最後の行に `generated blocks` が出る。`docs.yml` が使い捨てのブランチで成功した。

**Phase 2**
- [ ] `cargo test --locked --test doc_tags --test std_tags` が緑。`deprecation_note_reads_the_first_tag_line` が緑のまま。`cargo clippy --all-targets -- -D warnings` が緑。
- [ ] 8 モジュール 134 関数すべてに有効な `@complexity`（付録 B と同じ値）。`@see` の行が 20、`@since` は 0。`git diff --stat -- std` が追加行だけ。
- [ ] 8 ページと `docs/language.md` の 16 ブロックが `--check` で 0 stale。`Array.sort_in_place` の行は `$O(n \log^2 n)$`。手書きの表は組み込み操作の行だけ残る。
- [ ] `tsuzuri doc std` の出力は、`@complexity` の行が増えた以外（節の構造・署名）が変わらない。`cargo test --locked --test docs` と `node tests/docgen.mjs target/release/tsuzuri` が緑。

**Phase 3**
- [ ] `docs/benchmarks.md` が 520,000 byte 以下で、2 ブロックの外が 1 byte も変わらず、見出しの一覧が同じ、`check-links.mjs` が 70 リンク・0 broken。
- [ ] `summary.json`（行 38）と `metrics.json`（run 2・表 36・本体の行 314・セル 2,109）が往復検査を通った。`node tests/benchmark_report.mjs` が緑。`report.mjs` と `metrics.mjs report --publish` が JSON と文書を書き、2 回目は何も変えない。
- [ ] `migrate-html.mjs`・`legacy-report.mjs` が無い。`metrics.mjs report --baseline` の出力が変更前と同じ。

**Phase 4**
- [ ] `node scripts/check-cli-docs.mjs` が 4 つ `ok`、`node tests/check_cli_docs.mjs` が緑。README と `option.md` と `docs/language.md` の 9 件（6 + 2 + 1）が直った。
- [ ] `_specs/` の直下が `README.md` と `_archive/` だけ。5 ファイルは rename 100%。`try-with-finally.md` が直った。
- [ ] `docs.yml` に 7 つの手順（`gen_docs`・`gen-docs --check`・`benchmark_render`・`check_links`・`check-links`・`check_cli_docs`・`check-cli-docs`）があり、使い捨てのブランチで成功した。

## 落とし穴

| 間違い | 検出 |
| --- | --- |
| 生成ブロックの中を手で直す | `node scripts/gen-docs.mjs --check` が `stale`。直し方は `node scripts/gen-docs.mjs` |
| タグの行の前に `///` だけの行を置かず、説明と同じ段落になる | `target/release/tsuzuri doc std -o <dir>` のページで `@complexity` が前の文の続きに見える。`grep -B1 "^@complexity" <dir>/Array.md` の直前の行が空 |
| `String.find needle text` の `n` を `needle` の長さと取り違える | 規則は「`n` は主な入力。`text`」。`find`・`rfind`・`contains` は `O(n * m)`、`starts_with`・`ends_with` は `O(m)`（付録 B.8） |
| `Map.insert` を `O(log n)` にする | 新しいキーは `O(n)`（要素をずらす）。`O(log n)` は既存キーの置換。タグは最悪の値 |
| Markdown の表のセルの `\|`・`<` を escape しない | `tests/benchmark_report.mjs` の `cell` のテスト。`process/<example>` は 1 列目のコード span の中ではそのまま、見出しと残りのセルでは `&lt;` |
| `docs/benchmarks.md` の見出しを消す・変える | `node scripts/check-links.mjs --list-headings docs/benchmarks.md` を移行の前後で `diff`。アンカーは 17 種 |
| 移行の解析を、往復が合わないまま手で JSON に直す | 停止条件 2。往復検査が通らない JSON をコミットしない |

## 対象外

- 計算量の測定による検証、`@complexity` の自動推定。`Builtin` への doc と複雑さ（`Vec.push`・`Array.set`・`List.cons` は手書きの表）。
- README の `Map` の説明（`平衡二分探索木`）の修正（C12）。KB・`guide std` が `complexity` を出すこと（G21・G26・G27）。`tsuzuri doc` が `@complexity` を整形して描くこと（G34 の `render_doc` と衝突するので、行はそのまま説明として出す）。
- `@since` を既存の関数に付けること。`@see` の自動生成。
- 生の測定データ（`target/perf/**.jsonl`）のコミット、`docs/benchmarks.md` の手書きの節の整理、`_perfs/` のチケットの書き換え。
- `_specs` の内容の言語リファレンスへの取り込み（付録 C: 取り込む項目が 0）。GUIDE の D-34 の節（履歴）の書き換え。
- リポジトリ全体のリンク検査と、既存の壊れたリンク 11 件の修正。`compiler/usage.md` に全旗を載せること。
- `vsc/` の版（`0.1.1`）の検査。Windows での `gen-docs.mjs` の実行（`docs.yml` は Ubuntu）。

## 決定事項

すべて承認済み（D-50 の包括許可）。

| ID | 決定（理由） | 却下した案 |
| --- | --- | --- |
| D1 | マーカーは `<!-- generated:NAME -->`／`<!-- /generated:NAME -->` の 1 種類。benchmarks も改める（生成物を 1 つの規則で見つけて検査する） | ツール別の `…:start/end` を残す。ブロックを別ファイルに分ける（既存のリンクが壊れる） |
| D2 | 生成器は `scripts/gen-docs.mjs` 1 つで、`PLACEMENTS` に必須のブロックを列挙する（ブロックが消されても気づく） | 文書ごとのスクリプト。マーカーの走査だけ |
| D3 | コンパイラは CLI（`doc std`・`explain --catalog --json`）だけで読む（構文解析を JS に複製しない） | JS で std を再解析。`tsuzuri doc --json` の拡張（既存の意味を変える） |
| D4 | タグは説明の本文。文法違反を診断にしない（診断コードを増やさず、利用者の説明を縛らない） | 警告コード。属性にする（構文の変更） |
| D5 | `@complexity` の文法を閉じる（表を機械で作れる） | 自由文。LaTeX を直接書く |
| D6 | 組み込み操作（`Vec.push`・`Array.set`・`List.cons`）は手書きの表のまま | `Builtin` に doc を持たせる（`check.rs` を大きく触る）。側車ファイル（情報源が 2 つに戻る） |
| D7 | `Documentation` に 3 関数を足し、`deprecation()` も共通の `tag_notes` に寄せる（規則を 1 実装に） | 汎用 `tag(&str)` の公開。`deprecation()` を触らない |
| D8 | 表は「関数・計算量」（+ 空でなければ版・関連）。署名は載せない（`guide std` と重複しない） | 署名の列。手書きの行の維持 |
| D9 | 初期値は実装を読んだ付録 B。`sort_in_place` は `O(n log^2 n)` に直す | 既存の値を写す。測定で決める（共有 CI に閾値を置かない） |
| D10 | `@see` は値渡し版と参照版の対 12 本と、総和・内積の対。`@since` は新しい関数から | 全関数に `@since 0.1.0`（情報がない） |
| D11 | benchmarks のデータは表示文字列の JSON（生の標本はコミットされていない） | 生の標本。HTML との併存。CSV |
| D12 | Markdown の表。最速は太字、導出列は残す。色・`data-*` は捨てる（93.5% 減） | HTML のまま JSON だけ足す |
| D13 | 移行は解析 → 旧 `renderTable` での往復検査 → 一回限りのスクリプトを消す（数値を手で触らない） | 手作業の移行。往復検査なし |
| D14 | リンク検査は `--into docs/benchmarks.md` に限る（既存の壊れたリンク 11 件は別件） | 全体の検査と許容リスト |
| D15 | `_specs` は `git mv` で `_archive/` へ、README を置く。取り込みは 0。`try-with-finally.md` の 1 文を直す | README だけ。削除。言語リファレンスへの取り込み |
| D16 | ドリフト検査は 1 つのスクリプトに 4 検査（`--only`）。必須の組を表で固定し、免除は理由つき。`usage.md` は必須にしない | 検査ごとのスクリプト。全文書に全項目を要求 |
| D17 | 診断コードの集合は G23 のカタログがあればそれ、なければ `src/` の走査（G23 の移行でコード文字列がタプルになり、走査が壊れる） | 走査だけ。カタログだけ |
| D18 | CI は新しい `docs.yml`（Ubuntu 1 本、`cargo build --release`。LLVM が要らず 10 分ほど） | `targets.yml` に足す（4 ホスト 35〜85 分）。CI に載せない |
| D19 | 規模は L、G26 は使わない。G34 との並びは「仕様 3.4」 | G26 の欄に `complexity` を足す |
| D20 | G23 のブロックの形に合わせる（`DIAGNOSTIC_KINDS_HEADER`）。両方の検査を通す | G23 のテストを黙って置き換える |

## 付録

- [G35-appendix.md](G35-appendix.md): **A** 実測（A.1 `doc std` のページの形、A.2 数え方、A.3 往復検査の試作、A.4 Markdown の大きさ、A.5 ドリフトの測定、A.6 リンクの測定）、**B** 初期の `@complexity`・`@see`（B.1〜B.8。134 行）、**C** `_specs` の決定表（46 項目）。
- [G35-appendix-spec.md](G35-appendix-spec.md): **S** 本文から移した仕様の表（S.1 描画の規則、S.2 関数の対応、S.3 移行の決定表、S.4 ドリフト検査の規則と基線、S.5 `tests/benchmark_report.mjs` の書き換え表、S.6 現状の詳細、S.7 `gen-docs.mjs` の出力）。
- [G35-appendix-code.md](G35-appendix-code.md): **D** 参照コード（D.1 `Documentation` と `tests/doc_tags.rs`、D.2 `gen-docs.mjs`、D.3 `render.mjs`、D.4 移行の解析、D.5 `docs.yml`、D.6 `check-links.mjs`、D.7 `check-cli-docs.mjs`、D.8 `report.mjs` の全文、D.9 `metrics.mjs` の 4 か所、D.10 `tests/benchmark_report.mjs` の書き換え、D.11 `gen-docs.mjs` への Phase 3 の追加）、**E** 文書の文面（E.1 タグの仕様、E.2 README・`option.md`・`language.md` の補完、E.3 ページごとの置き換え、E.4 `_specs`、E.5 benchmarks の文）、**F** テストの期待値（F.1〜F.8）。


