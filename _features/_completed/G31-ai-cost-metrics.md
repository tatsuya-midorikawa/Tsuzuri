# G31: AI 作業コスト指標（入力量と往復数を決定的に測る）

| 項目 | 内容 |
| --- | --- |
| ID | G31 |
| 優先度 | P1 |
| 規模 | 全体 L 寄りの M（D-50 の表は M。Phase 1 M、Phase 2 S、Phase 3 M。D-50 の M は Phase 1〜2 の規模）。当初の Phase 別 PR 案に対し、全 Phase の承認に従って PR #43 で一括実装 |
| 依存 | なし（Phase 1・2 は HEAD の `tsuzuri check`・`tsuzuri test --list` を呼ぶだけ）。**弱い依存**（あれば測る行が増え、なければその行だけを出さない）: Phase 2 の `diag-v2` は [G22](G22-diagnostic-v2.md) の関連位置、`diag-fix` は [G24](G24-tsuzuri-fix.md) Phase 1。Phase 3 の静的サイズは [G21](G21-ai-guide-pack.md)・[G27](G27-build-time-kb.md)・G28、`kb` 方式は G27 Phase 2 と G28 Phase 1、MCP の行は [G29](../G29-mcp-server.md) Phase 1 |
| 後続 | G21・G22・G24・G27・G28・G29（各チケットの「AI の作業コストが下がる」という主張を、`docs/ai-cost.md` の数値で裏づける）。G32 の `brief` を入力量に使う案は取り込まない（「対象外」） |
| 状態 | done（Phase 1〜3。[PR #43](https://github.com/tatsuya-midorikawa/Tsuzuri/pull/43)。実装 head `5097b87` の CI・3 環境の targets は成功、Copilot は actionable な指摘なし。G21・G27・G28・G29 本体は未実装） |
| 起票 | 2026-10-18（D-50。HEAD `a2313d19` で確認。作業ツリーは `9dd0fbbc` で、差は `docs/benchmarks.md` の 41 行の削除だけ） |
| 承認 | 承認済み（D-50: 人間の包括許可 2026-10-18）。決定事項 D1〜D15 はすべて承認済み |
| 改善した劣位 | 着手前の基準 `41f80bc7` には、AI の作業コストを測る仕組みも基準値もなかった。現在は `benchmarks/ai` の決定的な計測器と [104 行の基準値](../../docs/ai-cost.md)を提供する。ガイド G21・診断 v2 G22・`fix` G24・KB G27/G28・MCP G29 の接続口は能力別に有効にするが、それらのコンパイラ機能自体が実装済みという意味ではない |
| 手本にする既存実装 | `benchmarks/report.mjs` の `updateSummary`（マーカーの間だけを書き換える）・`writeAtomically`（一時ファイル → rename。export 済みなので import する）。`benchmarks/metrics.mjs` の JSON Lines の書き方（`schema` の定数。ただし日時・ホストは入れない）。`benchmarks/run-frontend.mjs`（`--out` を要求し、コンパイラを子プロセスで呼ぶ流儀）。`tests/benchmark_report.mjs`（assert だけの Node テスト。CI は `.github/workflows/targets.yml` の段 `Benchmark correctness and metric records` で実行）。`scripts/check-docs.mjs`（Markdown の走査） |
| 主な影響ファイル | Phase 1（すべて新規）: `benchmarks/ai/run.mjs`、`benchmarks/ai/lib/{measure,corpus,tasks,derive,strategies,report}.mjs`、`benchmarks/ai/tasks/{a-definition,b-callers,d-type-search,e-spec,f-impact}.json`、`benchmarks/ai/corpus/shop/*.tz`（6）、`tests/ai_cost.mjs`、`docs/ai-cost.md`。変更: `.github/workflows/targets.yml`（1 行）、`README.md`・`docs/architecture.md`・`docs/benchmarks.md`（各 1 行）。Phase 2: 新規 `benchmarks/ai/tasks/c-diagnostic.json`・`benchmarks/ai/corpus/diag/c01`〜`c07`（各 `Main.tz`）。変更 `lib/strategies.mjs`・`lib/tasks.mjs`・`run.mjs`・`tests/ai_cost.mjs`。Phase 3: 新規 `lib/{external,static,kb,mcp}.mjs`。変更 `run.mjs`・`lib/report.mjs`・`docs/ai-cost.md`・`tests/ai_cost.mjs` |

## 実装者向け早見

- **作るもの**: LLM を呼ばずに、AI エージェントの作業コスト — 読んだ文字数 `chars`・ツール呼び出しの往復数 `calls`・推定トークン `est_tokens` — を、**模擬エージェント**の規則（擬似コード）で決定的に数える Node のハーネス `node benchmarks/ai/run.mjs {run|verify|report|task}`。タスクは 6 型 37 件（Phase 1 は 30 件）、結果は `target/ai-cost/results.jsonl` と、生成する `docs/ai-cost.md`。
- **最初に読む**: [付録 A〜H](G31-appendix.md)（A タスク表・B タスクの JSON・C コーパスの**そのまま置く全文**・D golden の数値・E 出力と検査の書式・F 後続 Phase の戦略・G テスト計画の表・H `docs/ai-cost.md` の骨格）→ 本チケットの「仕様」→「実装手順」。コード: `benchmarks/report.mjs`（`writeAtomically`）、`tests/benchmark_report.mjs`（assert の流儀）。
- **最初に書くテスト**: 手順 3 の `tests/ai_cost.mjs::estimate_tokens_matches_the_formula`（ASCII 5 字 → 2、非 ASCII 1 字 → 2）。実装より先に失敗させる。
- **触ってはいけない**: コンパイラ（`src/`・`std/`・`tests/*.rs`）と既存のテスト、`benchmarks/metrics.mjs`・`benchmarks/report.mjs`・`scripts/check-docs.mjs`（import するだけ）、`docs/benchmarks.md` の既存の記述。結果に LLM・ネットワーク・時計・乱数・環境変数・絶対パスを入れない。速度の合否の閾値を置かない。`/tmp` を使わない（作業ディレクトリは `--out` の下）。
- **動かしてはいけない定数**: `GREP_MAX_LINES` 100、`CHUNK_LINES` 500、`WINDOW_LINES` 40、`DIAG_V2_WINDOW` 5。変えると golden（付録 D）がすべて変わる。
- **停止の目印**: golden が 1 つでも合わない（ハーネスの誤り。golden はハーネスと独立のシェルで計算した）。shop コーパスが HEAD で `check` を通らない。Node 20 で動かない API が要る（`Map.groupBy` を使いたくなったとき）。
- **Phase 1 の完了**: `node benchmarks/ai/run.mjs run` が 97 行の `results.jsonl` を書き、`verify` が通り、`report --publish` が `docs/ai-cost.md` の表を作り、`node tests/ai_cost.mjs target/release/tsuzuri` が `0 failed`（Phase 1 の関数がすべて `ok`）。
- **Phase 2 の完了**: `run` が 104 行（C の 7 行が増える）。`diag-v2`・`diag-fix` は、コンパイラに G22・G24 が入っているときだけ行が増える。
- **Phase 3 の完了**: 外部エージェントの結果ファイルが `docs/ai-cost.md` の表に合算され、静的サイズの表と上限テストが動き、`kb` 方式と MCP の `tools/list` の行が（G27・G28・G29 が入っていれば）出る。入っていなければ「測らなかった戦略」として理由が出る。
- **新しい名前**: 実行 `node benchmarks/ai/run.mjs`、ディレクトリ `benchmarks/ai/`・`docs/ai-cost.md`・`target/ai-cost/`、schema `tz-ai-task/1`・`tz-ai-result/1`・`tz-ai-static/1`・`tz-ai-meta/1`・`tz-ai-external/1`、戦略名 `baseline-grep`・`baseline-grep-chunk`・`baseline-grep-window`・`baseline-docs`・`diag-v1`・`diag-v2`・`diag-fix`・`kb`・`kb-oriented`。診断コード・コンパイラの CLI・環境変数・予約語・crate の追加はない。

## 目的

D-50 は、ガイド・診断 v2・`fix`・KB・MCP が AI の作業を楽にすると言う。「楽」を主張するには基準値がいる。このチケットは、AI エージェントが仕事を終えるまでに**読む文字数**と**ツールを呼ぶ回数**を、LLM を呼ばず**規則で決定的に**数える。同じリポジトリの状態からは、誰が・どのマシンで動かしても 1 byte も違わない結果を出す。

測る量（仕様 1）は 3 つ: 入力文字数 `chars`、往復数 `calls`、推定トークン `est_tokens`（`ceil(ASCII 字数 / 4 + 非 ASCII 字数 × 1.3)`。「推定」であり実トークナイザではない）。タスクは 6 型 37 件で、型は (A) シンボルの定義と署名を知る、(B) 呼び出し元の一覧を知る、(C) 診断の原因を特定して直す、(D) 型から std の関数を探す、(E) 仕様の規則を引く、(F) 変更の影響を受けるテストを知る。

**HEAD で測った数字**（試作の実行器。タスクの型ごとの下位の中央値。全表は付録 D。`std`・文書のタスクの値は std・文書の編集で動く）: B（std の呼び出し元）は、素朴な全文読み（`baseline-grep`）が 34,454 字・3 往復、窓読み（`baseline-grep-window`）が 7,777 字・3 往復。F（影響範囲。shop）は 3,860 字・6 往復（素朴な全文読みは 4,320 字・8 往復）。C（診断。`diag-v1`）は 1,666 字・6 往復。A・D・E は、経路を知っている読み方なら 1 往復・68〜799 字（素朴な全文読みは A 7,743 字、D 9,448 字、E 375,348 字。E は `baseline-docs` なら 15,183 字・2 往復）。

読み取れること: 「1 か所を引く」問い（A・D・E）は、経路を知っている模擬エージェントなら 1 往復で済み、素朴な読み方との差が大きい。高くつくのは、結果が複数の場所にまたがる B・F と、直して確かめる C（往復 6 回）。KB・診断 v2・`fix` の効果が見えるのはここである。**効果の主張は、最も厳しい下限 `baseline-grep-window` との比で書く**（決定 D4）。

**非目標**:
- 実際の LLM の品質・応答時間・料金・トークナイザ別の数の測定（外部の枠で人手が置く値だけを合算する）。
- 実行速度の測定と、共有 CI への合否の閾値（AGENTS.md）。
- 模擬エージェントの「賢さ」の改良。規則は固定の下限・上限であり、実エージェントの再現ではない。
- ガイドの「最初の 1 回で書けるか」の測定（G21 の効果は静的サイズと外部の枠で扱う）。

## 着手条件と停止条件

### 着手条件

- 依存なし。GUIDE §2.3 の基準: このチケットはコンパイラを変えないので、`cargo build`・`cargo test` は不要。`ls target/release/tsuzuri` が存在し、`node --version` が 20 以上、`node tests/benchmark_report.mjs` が通ること（既存の Node テストの動作確認）。`git status --short` に自分の変更以外がないこと。
- 読む: AGENTS.md の方針（共有 CI に速度の閾値を置かない）、GUIDE の D-51（診断 JSON の `related`）・D-52（KB の ID・brief の上限）・D-55・D-56（名前とサイズ）、`docs/stability.md`、本チケットの付録 A〜H。
- 子プロセスの `tsuzuri` は `TSUZURI_CACHE_DIR` を気にしなくてよい（`check`・`test --list` はビルドキャッシュを使わない。HEAD の実測は 0.26 秒と 0.60 秒）。`run`・`test` を足したくなったら、共有キャッシュのロックで 1 分以上待つので `TSUZURI_CACHE_DIR=$PWD/target/ai-cost/cache` を付ける。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と選択肢を報告する（GUIDE §13）。

1. shop コーパス（付録 C.1）が HEAD の `tsuzuri check`（無出力・終了コード 0）または `tsuzuri test --list`（16 行）と合わない。選択肢: A（コンパイラの仕様の変更だけが原因なら、コーパスを HEAD に合わせ、golden を付録 D の方法で再計算して同じ PR に入れる）／B（理由を報告して指示を待つ）。
2. golden（付録 D の表）が、ハーネスの出力と 1 つでも違う。ハーネスの誤り（規則の読み違い）である。golden はハーネスと独立のシェルで計算した。**golden を出力に合わせて書き換えない。** 規則が曖昧で直せないなら報告する。
3. `verify` が shop・diag（`benchmarks/ai/corpus/` の下。安定したコーパス）のタスクで失敗する。`expect` を導出結果に合わせて直さない。導出規則か、手で書いたコーパスの誤りである。
4. `verify` の drift（std・文書・`tests/fixtures` のタスクの不一致）が 5 件を超える（HEAD が付録から動いた）。選択肢: A（関数の改名・移動だけなら `needs`・`answer`・`expect` を HEAD に合わせ、付録 B も直す）／B（意味が変わったタスクは、同じ型の別の問いに差し替える。各型 5 件以上・全体 30 件以上を保ち、報告する）。
5. Node 20 で動かない API が要る、または `package.json`・外部パッケージが要る。書き換える。Node の最低版は上げない（README は 20 以降）。
6. 実時間・乱数・日時・ホストの値が、結果に必要に見える。止める（決定 D1・D9）。
7. `benchmarks/report.mjs`・`benchmarks/metrics.mjs`・`scripts/check-docs.mjs` を変えないと書けないように見える。import できる関数だけを使う（`renderTable` は使わない。D10）。
8. Phase 2: `check --json` の先頭の診断が、付録 D の表（コードと行）と違う。選択肢: A（行番号やコードの変更だけなら `expect` を直し、付録も直す）／B（診断が出なくなったタスクは、同じコードを出す別の誤りのプログラムに差し替える）。
9. Phase 3: G28 が出荷した `kb` の引数が、「前提とする他チケットのインターフェース」の表と違う。表（`KB_COMMANDS`）だけを直す。コマンドが存在しないなら、その戦略を出さない（偽の出力を作らない）。
10. 同じ失敗を直す試みが 2 回続けて失敗した。

## 現状（HEAD `a2313d19` で確認）

確認した作業ツリーは `9dd0fbbc`。`target/release/tsuzuri` と Node 20.19.6 で実行した（CI は Node 24、README は 20 以降）。

- **AI の作業コストを測る仕組みはない**: `ls benchmarks/ai` は存在しない。`benchmarks/` は実行速度・ビルド時間・メモリの測定（`run-*.mjs`・`metrics.mjs`・`report.mjs`）。`docs/benchmarks.md` はその記録で、入力量・往復数の記録はない。
- **コンパイラにまだない入口**（`tsuzuri guide`・`fix`・`kb`・`mcp`）は、`build` の省略形として最初の語をソースの名前と解釈して失敗する。Phase 2・3 の「能力の検出」はこれを前提にする（実出力）:

```text
$ tsuzuri guide        guide:1:1: error[E2001]: cannot inspect source 'guide': No such file or directory (os error 2)   （終了コード 1。`fix`・`kb`・`mcp` も同じ形）
$ tsuzuri kb callers   <command line>:1:1: error[E2000]: pass one .tz, .tt, or .tc file or project directory; modules are loaded recursively   （終了コード 2）
$ tsuzuri --version    tsuzuri 0.1.0
```

- **`check --json`**: 診断は **stderr** に 1 行 1 JSON（`severity`・`code`・`message`・`path`・`span`）で出る。stdout は空。作業ディレクトリをプロジェクトにして `check .` と実行すると `path` は `./Main.tz`。診断がなければ stderr も空で終了コード 0。実例（付録 C.2 の `c01`）: `{"severity":"error","code":"E1012","message":"use of moved or partially moved value 'entry'","path":"./Main.tz","span":{"start":1179,"end":1189,"line":37,"column":30,"end_line":37,"end_column":40}}`。7 つの診断プログラムの先頭行は付録 D。
- **`test --list`**: 1 行が `{索引} {Module}.{テスト名}`（例: `6 Money.show pads the cents`）。コンパイル（LLVM）なしで動く。`shop` は 16 行。
- **既存の測定ハーネスの流儀**: `benchmarks/run-frontend.mjs` は位置引数の先頭がコンパイラのパスで `--out DIR` を要求する。`tests/*.mjs` は assert だけのスクリプトで、先頭の引数がコンパイラのパス（`node tests/benchmark_report.mjs`、CI の段 `Benchmark correctness and metric records`）。`benchmarks/report.mjs` の `writeAtomically(path, text)` は export 済み。
- **コーパスの大きさ**（`std/` は `cat $(find std -type f \( -name '*.tz' -o -name '*.tt' -o -name '*.tc' \)) | wc -m`、文書は `cat $(find _tsuzuri/language-reference -name '*.md') | wc -m` と `wc -m docs/language.md`）: `std/` 47 ファイル 417,695 字。`_tsuzuri/language-reference/` 111 ページ 1,081,198 字（`index.md` は 158 行・5,914 字）。`docs/language.md` 350,810 字。`examples/` は 13 ディレクトリ・`.tz` 11 ファイルで合計 5,191 字しかない（呼び出し元・影響範囲の問いが立たない。コーパスに使わない。決定 D7）。
- **試作で確かめたこと**（付録 D。`target/` の下で動かした約 300 行の実行器。全文は載せない）: 37 タスクの `needs`・`answer` がすべて解決する。導出した `callers`・`impact` の 12 件が `expect` と一致する。shop の 9 タスクの値を、実行器を使わずシェルの `grep -rnF | sort | wc -m` で独立に再計算して一致した。7 つの診断タスクは、壊れたソースの先頭の診断が `expect` と一致し、`fix` のあとの `check` が空になる。`and` で始まる行（相互再帰の 2 つ目以降）を項目の先頭にしても、12 件の導出結果と数値は変わらない。

## 変えてはいけない意味

- コンパイラ・std・`tests/*.rs`・既存の Node テスト・既存の文書の記述は 1 byte も変わらない。変更は新規ファイルと、`.github/workflows/targets.yml`・`README.md`・`docs/architecture.md`・`docs/benchmarks.md` への各 1 行の追記だけ。
- `tsuzuri` の CLI・診断・JSON・終了コード・IR・ABI・WASM の import は変わらない。ハーネスはコンパイラを子プロセスで呼ぶだけで、`check`・`test --list` 以外（Phase 2・3 の `fix`・`kb`・`guide`・`mcp`）は、**あるときだけ**呼ぶ。
- 結果（`results.jsonl`・`static.jsonl`・`report` の出力）は決定的: 同じリポジトリの状態・同じコンパイラから、byte 単位で同じ。日時・ホスト名・絶対パス・環境変数・乱数・実行時間を入れない（出所を示す `meta.json` だけが、コミットとコンパイラの版を持つ）。
- shop コーパス（6 ファイル）と diag コーパス（7 ファイル）は、付録 C のとおりに **1 byte も違わず**置く。golden の数値はこの文字列に依存する。コーパスを直したら、golden（付録 D・`tests/ai_cost.mjs`）を同じ PR で再計算する。
- 速度の合否の閾値を共有 CI に置かない。テストが強制するのは、定義（式・規則・書式）と、**静的なサイズ上限**（D-56・D-52 の文字数。成果物の大きさであって速度ではない）だけ。

## Phase 分割

| Phase | 内容 | 規模 | 前提 | 単独で出荷できる理由 |
| --- | --- | --- | --- | --- |
| 1 | ハーネス `benchmarks/ai/`（測る量・仮想ファイル・ツール・導出規則）、タスク 30 件（A・B・D・E・F。付録 B）、shop コーパス、戦略 `baseline-grep`・`-chunk`・`-window`・`baseline-docs`、`run`・`verify`・`report`・`task`、`docs/ai-cost.md`、`tests/ai_cost.mjs`、CI の 1 行 | M | なし | コンパイラの `check`・`test --list` だけを呼ぶ。コンパイラのコードに触らない。基準値だけでも価値がある（他チケットの「前」の数値） |
| 2 | 診断・修復ループ: C の 7 件（diag コーパス）と戦略 `diag-v1`（スクリプトの「修正器」で往復を数える）。**G22 が入ったら** `diag-v2`、**G24 が入ったら** `diag-fix` を足す（足すのは戦略 1 つずつ。能力の検出で自動で有効になる） | S | Phase 1（v2: G22 の関連位置、fix: G24 Phase 1） | `check` だけで動く。v2・fix は、コンパイラが対応していなければ行を出さず、理由を `meta.json` と `docs/ai-cost.md` に書く |
| 3 | 外部エージェントの結果の枠（`tz-ai-external/1`）、静的サイズの表と上限テスト、KB 方式（`kb`・`kb-oriented`）、MCP の `tools/list` の大きさ | M | Phase 1。KB: G27 Phase 2 と G28 Phase 1。MCP: G29 Phase 1。静的: G21・G27・G28 の出力 | 4 つの部分は互いに独立。外部の枠は何にも依存しない。コンパイラに入口がなければ、その行だけを出さない |

## 仕様

### 1. 測る量

| 名前 | 定義 | 備考 |
| --- | --- | --- |
| `chars` | 模擬エージェントが受け取ったツールの結果の文字列すべての Unicode コードポイント数の合計 | `for (const ch of text)` で数える（UTF-16 の `length` ではない。絵文字は 1） |
| `calls` | ツールを呼んだ回数の合計 | 結果が空文字列でも 1 回 |
| `est_tokens` | `floor((10 * ascii + 52 * other + 39) / 40)`。`ascii` は結果のコードポイントのうち 128 未満の数の合計、`other` は 128 以上の数の合計 | `ceil(ascii / 4 + 1.3 * other)` と同じ値。整数で計算する（1.3 は 2 進数で表せない）。結果ごとに丸めず、合計に 1 回だけ適用する |
| `rounds` | （型 C だけ）修正の試行の数。試行 1 回 = `edit` 1 回 + `check` 1 回。最初の `check` は含めない | |
| `complete` | 戦略が、答えに必要な情報を得たか | 戦略ごとの規則（仕様 5） |

数えないもの: タスクの `prompt`、エージェントの出力・思考、ツール呼び出しの**引数**（パターン・パス・範囲・編集の内容）、`kb build` の処理。

`est_tokens` の例: ASCII 4 字 → 1、ASCII 5 字 → 2、非 ASCII 1 字 → 2、`a😀あ`（3 コードポイント。ascii 1・other 2）→ 3。0 字 → 0。

**下位の中央値** `lowerMedian(values)`: 昇順に並べた配列の `floor((n - 1) / 2)` 番目（整数のまま。平均しない。`n = 0` は例外）。`[5, 1, 3]` → 3、`[4, 1, 3, 2]` → 2。

### 2. コーパスと仮想ファイル（`Tree`）

模擬エージェントはファイルシステムを `Tree` 越しに見る。パスはすべて**論理パス**（リポジトリの根からの相対、区切りは `/`）。

| 項目 | 規則 |
| --- | --- |
| 対象の拡張子 | `.tz` `.tt` `.tc` `.md` `.toml` `.json` |
| 除外 | 名前が `.` で始まるもの、`target`、`node_modules`（ディレクトリもファイルも） |
| `files(root)` | `root` がディレクトリなら下の対象ファイルすべて、ファイルなら（対象の拡張子のとき）その 1 つ。歩き方に依らず、結果は**比較関数なしの `sort()`**（UTF-16 コード単位の順。`localeCompare` を使わない）。`root` がなければ例外 `corpus root '{root}' does not exist` |
| 複数の `root` | `corpus` の順に、`root` ごとの `files` を連結する |
| 行 | UTF-8 で読み、`"\n"` で分割し、最後の要素が `""` なら 1 つ捨てる。行番号は 1 から。`\r` は取り除かない |
| 物理パス | `Tree` のコンストラクタの `base`（論理パス `.` に当たる物理ディレクトリ）を前置する。リポジトリのコーパスは `base = ROOT`（`lib/corpus.mjs` が `import.meta.url` から求める）。診断タスクは作業コピーが `base` |

コーパス（タスクの `corpus` と `project`）:

| 名前 | 論理パス | 大きさ | 変わりやすさ |
| --- | --- | --- | --- |
| `std` | `std` | 47 ファイル・417,695 字 | std の編集で変わる |
| 文書 | `_tsuzuri/language-reference`・`docs/language.md` | 111 ページ・1,081,198 字と 350,810 字（日本語） | 文書の編集で変わる |
| `shop` | `benchmarks/ai/corpus/shop` | 6 ファイル・5,183 字・テスト 16 件 | **安定**（固定） |
| `diag` | `benchmarks/ai/corpus/diag/c01`〜`c07` | 各 1 ファイル・31〜49 行・合計 8,420 字 | **安定**（固定） |
| fixture | `tests/fixtures/unions`（A-06 だけ） | 2 ファイル | fixture の編集で変わる |

**安定なコーパス** = 論理パスが `benchmarks/ai/corpus/` で始まるもの。タスクの `corpus` に安定でないものが 1 つでもあれば、そのタスクは**非安定**（`verify` の drift の対象。仕様 9）。

### 3. ツール

模擬エージェントは、次のツールだけを呼ぶ。`Agent` が `calls`・`ascii`・`other`・`seen`（`Map<path, Set<行番号>>`）を持ち、ツールを呼ぶたびに結果を `emit(tool, text)` で数える。

| ツール | 引数 | 結果の文字列 | `seen` |
| --- | --- | --- | --- |
| `grep(pattern, roots)` | 固定文字列（空でない）、論理パスの列 | `roots` の順に `files(root)` の各ファイルを、行の上から見て `line.includes(pattern)` の行を `{path}:{行番号}:{行}\n` で並べる。**全体で 100 行**（`GREP_MAX_LINES`）まで。超えた分は黙って捨てる。一致がなければ `""` | 出した行だけ |
| `read(path, from, to)` | 1 始まりの両端を含む範囲 | `from` を 1 以上、`to` を行数以下に丸めたあと `lines.slice(from - 1, to).join("\n") + "\n"`。丸めた範囲が空なら例外（実装の誤り） | 丸めた範囲 |
| `chunk(path, k)` | `k ≥ 1` | `read(path, (k - 1) * 500 + 1, k * 500)`（`CHUNK_LINES` = 500） | 同上 |
| `check(dir)` | プロジェクトのディレクトリ | `tsuzuri check . --json` を `cwd = dir` で実行した **stderr の全文**（stdout は空）。終了コードは見ない | なし |
| `edit()` | なし | `"ok\n"`（3 字。固定） | なし |
| `fix(dir)`（Phase 2） | ディレクトリ | `tsuzuri fix .` を `cwd = dir` で実行した stdout と stderr をこの順に連結したもの | なし |
| `command(args, cwd)`（Phase 3） | `tsuzuri` への引数、論理パスの `cwd` | stdout。終了コードが 0 でなければ stdout の後ろに stderr を連結 | なし |

子プロセスは 1 回 60 秒（`CHILD_TIMEOUT_MS` = 60000）で打ち切り、超えたら例外にする（`error: tsuzuri {args} timed out`）。

### 4. タスクの JSON（`tz-ai-task/1`）

`benchmarks/ai/tasks/` の型ごとの 1 ファイル（Phase 1 は a・b・d・e・f の 5 つ、Phase 2 が c）。全文は[付録 B](G31-appendix.md#付録-b-タスクの-json)。形は `{"schema":"tz-ai-task/1","type":"<型名>","tasks":[ … ]}`。1 行に 1 タスク。`loadTasks()` は全ファイルを読み、`id` の `sort()` で整列する。

| 型（記号） | ファイル | `type` | 必須のキー（`id`・`type`・`prompt` に加えて） | 省略できるキー |
| --- | --- | --- | --- | --- |
| A | `a-definition.json` | `definition` | `corpus`・`grep`・`needs`・`answer` | `docs`・`kb` |
| B | `b-callers.json` | `callers` | `corpus`・`symbol`・`expect` | |
| C | `c-diagnostic.json` | `diagnostic` | `project`・`expect`・`needs`・`fix` | `first_try` |
| D | `d-type-search.json` | `type-search` | `corpus`・`grep`・`needs`・`answer` | `find`・`kb` |
| E | `e-spec.json` | `spec` | `corpus`・`grep`・`needs`・`answer` | `docs` |
| F | `f-impact.json` | `impact` | `corpus`・`change`・`expect` | |

| キー | 型 | 規則 |
| --- | --- | --- |
| `id` | 文字列 | `^[A-F]-[0-9]{2}$`。先頭の記号は型と一致。全体で一意 |
| `prompt` | 文字列 | 英語 1〜2 文。外部エージェントがそのまま読む（`task` コマンド） |
| `corpus` | 文字列の配列（1 個以上） | 論理パス。仕様 2 |
| `project` | 文字列 | C だけ。`benchmarks/ai/corpus/diag/cNN` |
| `grep` | 文字列の配列（1 個以上） | A・D・E のパターン。B・F は `symbol`・`change` から導出する（書かない） |
| `needs` | `[path, match, before, after]` の配列 | `match` は `path` の中でちょうど 1 行に含まれる部分文字列、`before`・`after` は 0 以上の整数。**答えを知るために見えていなければならない行**の範囲（`[m - before, m + after]`）。A・D・E は書く。B・F は導出する。C の `path` は `project` からの相対（`Main.tz`） |
| `answer` | 文字列の配列 | 答えの正解の文字列。`verify` が、`needs` の本文に含まれることを確かめる |
| `docs` | `{"needs": [...]}` | `baseline-docs` が読むページの `needs`（`index.md` から辿れる文書の側） |
| `kb` | `{"id": "Module.name", "contains": [...]}`（`contains` は省略可） | Phase 3。A の `kb show` の ID。`contains` は成功判定の部分文字列（なければ仕様 5 の既定） |
| `find` | 文字列 | Phase 3。D の型の問い合わせ（G25 の `find` に渡す） |
| `symbol` | `Module.name` | B。`^[A-Za-z][A-Za-z0-9_]*\.[A-Za-z_][A-Za-z0-9_]*$` |
| `change` | `Module.name` | F。同上 |
| `expect` | B・F: `Module.name` または `Module.テスト名` の配列（`sort()` 済み）。C: `{"code": "E1012", "line": 37}` | 導出の結果の独立な控え（仕様 6）。C は壊れたソースの先頭の診断 |
| `first_try` | `[{path, find, replace}]` | C。最初に試して**失敗する**誤った直し |
| `fix` | `[{path, find, replace}]` | C。正しい直し。`find` は元のファイルにちょうど 1 回現れる |

**未知のキーはエラー**（`task A-01: unknown key 'foo'`）。必須のキーがなければ `task A-01: missing key 'grep'`。Phase 1 の `loadTasks` も `kb`・`find`・`project`・`fix` などの Phase 2・3 のキーを**受け付ける**（付録 B のファイルをそのまま置くため）。

例（A-05。付録 B から）: `{"id":"A-05","type":"definition","prompt":"Which fields does the record Item have, and what are their types?","corpus":["benchmarks/ai/corpus/shop"],"grep":["record Item"],"needs":[["benchmarks/ai/corpus/shop/Catalog.tz","record Item {",0,0]],"answer":["record Item { id: i64, name: string, price: i64, tax_percent: i64 }"],"kb":{"id":"Catalog.Item"}}`。

### 5. 戦略（模擬エージェントの擬似コード）

共通: `needs` の解決 `resolveNeeds(rows, tree)` は、各 `[path, match, before, after]` について `tree.lines(path)` の中で `match` を含む行を探し、**ちょうど 1 行**でなければ例外（`need {path} "{match}": {n} matching lines`）、その行番号 `m` から `{path, from: max(1, m - before), to: min(N, m + after)}` を作る。`agent.covers(need)` は `from..to` のすべての行が `agent.seen.get(path)` にあること。`groupByPath(needs)` は `Map<path, Need[]>` を作り、パスの `sort()` の順に回す。`WINDOW_LINES` = 40。

| 戦略 | 型 | 模すもの | 位置づけ |
| --- | --- | --- | --- |
| `baseline-grep` | A B D E F | `grep` → ヒットしたファイルを**全文**読む | 素朴な上限 |
| `baseline-grep-chunk` | A B D E F | `grep` → 必要な行が見えるまで 500 行ずつ読む | 中間 |
| `baseline-grep-window` | A B D E F | `grep` → 必要な行の ±40 行だけ読む | 最も厳しい下限（**効果の主張はこれとの比**） |
| `baseline-docs` | `docs` を持つ A・E | 言語リファレンスの目次 → ページを 500 行ずつ | 文書から引く |
| `diag-v1` | C | `check` → 周辺を読む → 直す → `check` | 診断 v1 の往復 |
| `diag-v2` | C | v1 と同じ流れで、読む範囲を `related` に絞る | G22 の効果 |
| `diag-fix` | C | `check` → `fix` | G24 の効果 |
| `kb` | A B D F | `kb` の 1 コマンド | KB の効果（Phase 3） |
| `kb-oriented` | A B D F | `kb brief --project` のあと `kb` の 1 コマンド | 先に全体像を読む場合 |

戦略の並び順（行・表の順）は上の表のとおり。`grep` の列は、A・D・E ではタスクの `grep`、B・F では導出した `grep`（仕様 6）。

```js
// 1. baseline-grep: 素朴。ヒットしたファイルを全文。足りない行は 500 行ずつ
function baselineGrep(task, grep, needs, tree):
  agent = new Agent(tree)
  for (p of grep) agent.grep(p, task.corpus)
  for (path of [...agent.seen.keys()].sort()) agent.read(path, 1, tree.lines(path).length)   // 1 ファイル 1 往復
  chunkUntilCovered(agent, needs)
  return { agent, complete: true }

// 2. baseline-grep-chunk: 全文は読まない
function baselineGrepChunk(task, grep, needs, tree):
  agent = new Agent(tree);  for (p of grep) agent.grep(p, task.corpus)
  chunkUntilCovered(agent, needs);  return { agent, complete: true }

function chunkUntilCovered(agent, needs):
  for ([path, list] of groupByPath(needs)):
    open = list.filter(n => !agent.covers(n))
    for (k = 1; !open.every(n => agent.covers(n)); k++) agent.chunk(path, k)    // 先頭の塊から。k が行数を超えたら実装の誤り

// 3. baseline-grep-window
function baselineGrepWindow(task, grep, needs, tree):
  agent = new Agent(tree);  for (p of grep) agent.grep(p, task.corpus)
  for ([path, list] of groupByPath(needs)):
    open = list.filter(n => !agent.covers(n));  if (open.length == 0) continue
    N = tree.lines(path).length
    for ([from, to] of mergeWindows(open.map(n => [max(1, n.from - 40), min(N, n.to + 40)]))) agent.read(path, from, to)
  return { agent, complete: true }

function mergeWindows(windows):            // from の昇順に並べ、前の窓の to + 1 以下から始まる窓（隣り合う窓を含む）を結合する
  out = []
  for (w of windows sorted by w[0]) if (out.length > 0 && w[0] <= out.at(-1)[1] + 1) out.at(-1)[1] = max(out.at(-1)[1], w[1]); else out.push([w[0], w[1]])
  return out

// 4. baseline-docs: 目次 1 往復 + ページを先頭から 500 行ずつ
function baselineDocs(task, tree):
  agent = new Agent(tree);  index = "_tsuzuri/language-reference/index.md"
  agent.read(index, 1, tree.lines(index).length)
  for ([path, list] of groupByPath(resolveNeeds(task.docs.needs, tree))) for (k = 1; !list.every(n => agent.covers(n)); k++) agent.chunk(path, k)
  return { agent, complete: true }
```

**`diag-v1`（Phase 2。C）** の流れ（擬似コード全文は[付録 F.1](G31-appendix.md#付録-f-後続-phase-の戦略)）:

1. `task.project` を `<out>/work/<id>/` へ複製し、`check`（1 往復）。先頭の `error` の行（`severity == "error"` の最初の JSON 行）の `span.line` を `L` とする。
2. `L` の ±40 行と、`L` を含む最上位の項目（`itemRange`）の全体を、`read` 1 回で読む（`min(L - 40, 項目の先頭)`〜`max(L + 40, 項目の末尾)`）。
3. 必要（`needs`）のうちまだ見えていないものを、±40 行で 1 回ずつ読む。
4. 試行: `first_try`（あれば）→ `fix` の順に、**元のソースの複製**へ編集を当て（`find` がちょうど 1 回現れなければ例外）、`edit`（`ok\n`）+ `check`。`error` を含まない `check` が出たら止める。`rounds` = 試行の数、`complete` = 最後の `check` に `error` がない。

`calls = 1 + 1 + (読み足した必要の数) + 2 * rounds`。例（C-02。`rounds = 1`）: `check` 276 字 + 読み 962 字 + `edit` 3 字 + 直後の `check` 0 字 = 1,241 字・4 往復。

**後続の戦略**（擬似コードと能力の検出は[付録 F](G31-appendix.md#付録-f-後続-phase-の戦略)）:

| 戦略 | 流れ（往復の数え方） | 要点 |
| --- | --- | --- |
| `diag-v2`（G22 が入っているとき） | `check` → 主スパンと `related` の各スパンの ±5 行（`DIAG_V2_WINDOW`）だけを読む → 足りない `needs` を ±5 行で読む → `tryAttempts` | `diag-v1` と同じ試行・同じ `complete`。変わるのは読む範囲だけ |
| `diag-fix`（G24 が入っているとき） | `check` → 複製の上で `tsuzuri fix .`（2 往復）。終了コード 0 で stderr に `error[E` がなければ完了（`rounds` = 0）。そうでなければ `diag-v2` の流れを続ける | 元のソースは触らない。やり直し（revert）は数えない（下限） |
| `kb` / `kb-oriented`（A・B・D・F） | 型ごとに 1 コマンド: A `kb show`・B `kb callers`・D `kb find`・F `kb tests-for`。`kb-oriented` は先に `kb brief --project` | `chars` = コマンドの stdout。`complete` は文字列の一致で近似（`expect`・`kb.contains`） |

### 6. 導出規則（呼び出し元と影響範囲）

B・F の `needs` と `grep` は、ソースから決定的に導く。`expect` はその独立な控えで、`verify` が一致を確かめる。

```js
KEYWORD     = /^(?:def|fn|and|private|export|const|record|union|class|instance|type|test|bench|extern|namespace|using)\b/
DECLARATION = /^(?:(?:private|export|const)\s+)*(def|fn|and)\s+(?:rec\s+)?([A-Za-z_][A-Za-z0-9_]*)/
TEST_HEAD   = /^test\s+"((?:[^"\\]|\\.)*)"/

stripComment(line):    // 文字列の外の最初の `//` から後ろを捨てる
  inString = false
  for (i = 0; i < line.length; i++): c = line[i]
    if (inString && c == "\\") { i++; continue }
    if (c == '"') inString = !inString
    else if (!inString && c == "/" && line[i + 1] == "/") return line.slice(0, i)
  return line

parseItems(roots, tree) -> Item[]       // Item = { path, module, name, id, from, to, lines, kind }
  for (root of roots) for (path of tree.files(root)) if (拡張子が .tz .tt .tc):
    module = 拡張子を除いたファイル名;  lines = tree.lines(path).map(stripComment)
    starts = lines の KEYWORD に一致する行の行番号（1 始まり）の昇順
    for (k, from of starts):
      to = (starts[k + 1] ?? lines.length + 1) - 1;  while (to > from && lines[to - 1].trim() == "") to--
      if (m = DECLARATION.exec(lines[from - 1])) 項目 { kind: "function", name: m[2], id: module + "." + m[2], ... }
      else if (m = TEST_HEAD.exec(lines[from - 1])) 項目 { kind: "test", name: m[1], id: module + "." + m[1], ... }
      // `DECLARATION`・`TEST_HEAD` のどちらにも一致しない項目の先頭行（record・union・instance の行）も、前の項目を終わらせるが、項目としては集めない
```

注: `def f ::`（署名）と `fn f …`（本体）は別の項目で、同じ `id` を持つ。`and g …`（相互再帰の 2 つ目以降）も 1 つの項目。インデントされた行（`instance` の中のメソッド）は独立の項目にならない（近似。決定 D6）。

```js
referenceLines(item, symbol):       // symbol = "Module.name"
  [module, name] = symbol を最初の "." で分ける
  qualified = new RegExp(`(?<![A-Za-z0-9_.])${module}\\.${name}(?![A-Za-z0-9_])`)
  bare      = new RegExp(`(?<![A-Za-z0-9_.])${name}(?![A-Za-z0-9_])`)
  return item.lines の各行 i のうち qualified に一致、または (item.module == module かつ bare に一致) する行の行番号 (item.from + i)

callersOf(items, symbol) -> Map<id, [{item, last}]>      // id の sort() の順
  for (item of items) if (item.kind == "function" && item.id != symbol && referenceLines(item, symbol) が空でない)
    result[item.id].push({ item, last: referenceLines の最後 })
```

**B（呼び出し元）**: `callers = callersOf(parseItems(corpus), symbol)`。`grep = [name]`（`symbol` の `.` の後ろ）。`needs` = 各 `{item, last}` の `{path: item.path, from: item.from, to: last}`（呼び出した行の最後まで読めば名前が分かる）。答えは `callers` の `id` の列で、`expect` と一致しなければならない。

**F（影響範囲）**: `change` から**呼び出しグラフを遡る幅優先探索**。

```js
parent = new Map([[change, null]]);  order = [change];  tests = []
for (head = 0; head < order.length; head++):
  target = order[head]
  for ([id, entries] of callersOf(items, target)) if (!parent.has(id)) { parent.set(id, { callee: target, entries }); order.push(id) }
  for (item of items)    // 読み込み順
    if (item.kind == "test" && !parent.has(item.id) && referenceLines(item, target).length > 0)
      { parent.set(item.id, { callee: target, entries: [{ item, last: referenceLines(item, target).at(-1) }] }); tests.push(item.id) }
grep  = order のうち、items に kind == "function" の項目がある id の、`.` より後ろの名前（探索の順。`main` も入る）
needs = tests を sort() した各 id について、cur = id から parent.get(cur) が null になるまで callee をたどり、各 entries の {path, from: item.from, to: last}
        同じ (path, from) は to の大きい方 1 つにまとめる
答え   = tests を sort() したもの（`expect` と一致しなければならない）
```

エージェントの想定: 変更点の名前を grep し、見つかった呼び出し元の名前をまた grep する（発見した**関数ごとに 1 回**、テストにつながらないものも含めて）。テストの名前が分かるまで、チェーン上の項目の先頭から最後の参照行まで読む。

### 7. 出力

**`results.jsonl`**（`DIR/results.jsonl`。1 行 1 オブジェクト。`run` のたびに丸ごと書き直す。`writeAtomically`）。キーの順序は固定: `schema`（`"tz-ai-result/1"`）・`task`・`type`・`strategy`・`chars`・`calls`・`est_tokens`・（C だけ）`rounds`・`code`（壊れたソースの先頭の診断のコード）・`complete`。行は `task` の `sort()` → 戦略の並び順。

```jsonl
{"schema":"tz-ai-result/1","task":"A-05","type":"definition","strategy":"baseline-grep","chars":1048,"calls":2,"est_tokens":262,"complete":true}
{"schema":"tz-ai-result/1","task":"A-05","type":"definition","strategy":"baseline-grep-chunk","chars":107,"calls":1,"est_tokens":27,"complete":true}
{"schema":"tz-ai-result/1","task":"C-02","type":"diagnostic","strategy":"diag-v1","chars":1241,"calls":4,"est_tokens":311,"rounds":1,"code":"E1014","complete":true}
```

**`static.jsonl`**（`DIR/static.jsonl`。Phase 3）: `{"schema":"tz-ai-static/1","artifact":"guide-l0","chars":5429,"est_tokens":1358,"limit":6000}`。`limit` は整数か `null`。

**`meta.json`**（`DIR/meta.json`。整形せず 1 行 + 改行）: `schema`（`"tz-ai-meta/1"`）・`compiler`（`tsuzuri --version` の 1 行目）・`commit`（`git rev-parse --short=12 HEAD`。失敗は `"unknown"`）・`dirty`（`git status --porcelain` が空でなければ `true`）・`rows`（`results.jsonl` の行数）・`skipped`（`[{"strategy","reason"}]`）・`skipped_tasks`（`[{"task","reason"}]`）。**これだけが出所の情報を持つ。** `results.jsonl`・`static.jsonl` はホストやコミットに依らない。

**外部エージェントの結果**（Phase 3。`DIR/external/*.jsonl`。人間・外部のエージェントが置く。1 行 1 オブジェクト）: `{"schema":"tz-ai-external/1","task":"B-05","agent":"claude-haiku","variant":"plain","tokens_in":12345,"tokens_out":321,"turns":4,"success":true}`。キーはこの 8 つで、`task` は読み込んだタスクの `id`、`agent`・`variant` は短い名前（`variant` は条件の名前。例 `plain`・`guide`・`kb`・`mcp`）、`tokens_in`・`tokens_out`・`turns` は 0 以上の整数、`success` は真偽。知らないキーは無視する。不正な行は `report` が終了コード 1 で拒否する（キーの規則とメッセージは[付録 E.2](G31-appendix.md#付録-e-出力と検査の書式)）。

**`report` の出力**（Markdown。`docs/ai-cost.md` の `<!-- generated:ai-cost -->` と `<!-- /generated:ai-cost -->` の間に入る本文と同一。例と全規則は[付録 E.1](G31-appendix.md#付録-e-出力と検査の書式)）: 1 行目 `測定: {compiler}、コミット {commit}[（未コミットの変更あり）]、タスク {n} 件、行 {m}`、続けて節を、データのあるものだけ次の順に出す。**型ごとの値**（列 `型|戦略|件数|chars|calls|est_tokens|complete`。`chars`・`calls`・`est_tokens` は下位の中央値、`complete` は `真の数/件数`。行は型の記号順 → 戦略の並び順）、**診断の修復（C）のタスクごとの値**（`タスク|コード|戦略|chars|calls|rounds|est_tokens`）、**静的な成果物の大きさ**（`成果物|chars|est_tokens|上限`。`limit` が `null` なら `-`）、**外部エージェントの実測**（`agent|variant|型|件数|成功|tokens_in|tokens_out|turns`）、**測らなかったもの**（`meta.json` の `skipped`・`skipped_tasks`）。数値は 10 進の整数で桁区切りなし。表は Markdown のパイプ表（HTML の `renderTable` は使わない。D10）。

**`docs/ai-cost.md`**: 手で書く本文（目的・測る量・戦略・読み方・限界・再現手順）と、生成する領域。マーカーは G35 の対の記法で、1 行ずつ `<!-- generated:ai-cost -->` と `<!-- /generated:ai-cost -->`（G35 の `PLACEMENTS` には足さない。本体は `report --publish` だけが書く）。`updateReportDocument(document, generated)` はマーカーの間（マーカーを含まない）を `"\n" + generated`（`generated` は末尾に改行 1 つ）に置き換える。どちらかがない・2 つ以上ある・順序が逆なら例外（`docs/ai-cost.md must contain exactly one <!-- generated:ai-cost --> before one <!-- /generated:ai-cost -->`）。同じ入力で 2 回続けても同じ文書になる。

### 8. CLI と終了コード

```text
node benchmarks/ai/run.mjs run    [--compiler PATH] [--out DIR] [--tasks DIR] [--types A,B,..] [--strategies NAME,..]
node benchmarks/ai/run.mjs verify [--compiler PATH] [--tasks DIR] [--strict]
node benchmarks/ai/run.mjs report [--out DIR] [--doc PATH] [--publish]
node benchmarks/ai/run.mjs task ID [--tasks DIR]
```

| オプション | 既定 | 意味 |
| --- | --- | --- |
| `--compiler PATH` | `target/release/tsuzuri`（ROOT 基準） | 子プロセスで呼ぶコンパイラ |
| `--out DIR` | `target/ai-cost`（ROOT 基準） | 結果の置き場。作業ディレクトリは `DIR/work`（`run` の最初に消して作り直し、終わりに消す） |
| `--tasks DIR` | `benchmarks/ai/tasks` | タスクの JSON の置き場（テストが壊したタスクを渡すため） |
| `--types` | すべて | 記号（大文字小文字は区別しない）。`A,C` |
| `--strategies` | 使えるものすべて | 名前の列。**明示して使えなければエラー**（暗黙の縮退をしない） |
| `--strict` | 無効 | `verify` で drift も失敗にする |
| `--publish` | 無効 | `report` が `--doc` の文書を書き換える（なければ標準出力に出すだけ） |
| `--doc PATH` | `docs/ai-cost.md`（ROOT 基準） | `--publish` が書き換える文書（テストが複製を渡すため） |

引数の相対パス（`--compiler`・`--out`・`--tasks`・`--doc`）は**カレントディレクトリ**から `path.resolve` で解決する。既定値だけがリポジトリの根（`ROOT`）から。

- `run`: 全タスク × 使える戦略を走らせて `results.jsonl`・`meta.json`（と Phase 3 の `static.jsonl`）を書く。標準出力は 1 行目 `wrote {n} rows to {DIR}/results.jsonl`、続けて `skipped strategy {name}: {reason}`・`skipped task {id}: {reason}` を 1 行ずつ。**非安定のタスクの `needs` が解決できない（drift）ときは、そのタスクを飛ばして続ける**。安定なタスクの失敗は例外（終了コード 1）。
- `verify`: 仕様 9 の検査。最後の行は `verified {n} tasks: {f} failures, {d} drift`。
- `report`: `DIR/results.jsonl`（なければ `error: {path} not found; run 'run' first`、終了コード 1）・`static.jsonl`・`meta.json`・`external/*.jsonl`（ファイル名の `sort()`）から Markdown を標準出力へ。`--publish` なら `--doc` の文書を書き換えて `updated {--doc の値}` を出す。
- `task ID`: タスクの**エージェントに見える部分**だけを 1 行の JSON で出す（キーは `id`・`type`・`prompt`、そして `corpus` か `project`。`answer`・`needs`・`expect`・`fix` などは出さない）。

終了コード: 0 = 成功。1 = 検証の失敗・安定なタスクの実行の失敗・`--strategies` で明示した戦略が使えない・外部の結果の不正・子プロセスの異常。2 = 使い方の誤り（引数なし・知らないコマンド・知らないオプション・`--types` の誤り・知らない戦略名・知らないタスク ID）。エラーは標準エラーに `error: {メッセージ}` の 1 行（使い方の誤りは続けて `usage: …`）。メッセージの全文は[付録 E.4](G31-appendix.md#付録-e-出力と検査の書式)。

### 9. `verify` の検査

安定なタスク（仕様 2）の失敗は**失敗**、非安定なタスクの失敗は **drift**（`--strict` なら失敗）。検査（メッセージ・全 8 項目は[付録 E.3](G31-appendix.md#付録-e-出力と検査の書式)）: (1) タスクのファイルが仕様 4 の形。(2) A・D・E の `grep` のパターンが `corpus` で 1 行以上に一致。(3) すべての `needs`・`docs.needs` が一意に解決し、`needs` のパスがコーパスの中、`docs.needs` のパスが `_tsuzuri/language-reference` の中。(4) `answer` の各文字列が `needs` の本文に含まれる。(5) B・F の導出した答えが `expect` と一致。(6) `corpus` に現れる安定なコーパス（`shop`）の、`parseItems` のテスト項目の ID の集合が `tsuzuri test {dir} --list` の集合と一致（導出規則をコンパイラで裏づける）。(7) `shop` が `tsuzuri check` を通る。(8)（Phase 2）C の先頭の診断が `expect` と一致し、`fix` で `check` に `error` がなくなり、`first_try` では残る。最後の行は `verified {n} tasks: {f} failures, {d} drift`。失敗が 1 件でもあれば終了コード 1。

### 10. 決定性・上限・互換

- **決定性**: 結果は、リポジトリの内容とコンパイラだけの関数。整列はすべて `sort()`（比較関数なし）か、明示した規則。ホストの照合順序・時計・乱数・環境変数・並列実行の順序に依らない。`run` を 2 回続けて `results.jsonl` が 1 byte も違わないことを、テストが確かめる。診断のタスクの `chars` はコンパイラの診断の文言に依る（文言を直せば変わる。それは測りたいこと）ので、テストは `calls`・`rounds` だけを固定する。
- **上限**: `GREP_MAX_LINES` 100、`CHUNK_LINES` 500、`WINDOW_LINES` 40、`DIAG_V2_WINDOW` 5、`CHILD_TIMEOUT_MS` 60000、MCP の応答待ち 10 秒。タスクの数に上限はない（37 件で `run` は約 5 秒）。
- **互換**: 結果ファイルは開発用ツールの内部形式で、`docs/stability.md` の保証の対象外。ただし schema の番号を持ち、足すだけの変更は番号を保つ（読み手は知らないキーを無視する）。コンパイラの出力・IR・ABI・edition・診断コードに影響しない（D-45・D-54 の規則は無関係）。Windows は対象外（`process.platform === "win32"` なら `tests/ai_cost.mjs` は `skip` を出して 0 で終わる。決定 D14）。

## 設計

### 新規ファイルと関数（名前・シグネチャを変えない。Node 20、外部パッケージなし、ES modules）

| ファイル | 責務 | export |
| --- | --- | --- |
| `benchmarks/ai/lib/measure.mjs` | 定数・数え方・`Agent` | `GREP_MAX_LINES`・`CHUNK_LINES`・`WINDOW_LINES`・`DIAG_V2_WINDOW`・`CHILD_TIMEOUT_MS`、`countText(text) -> {chars, ascii, other}`、`estimateTokens(ascii, other) -> number`、`lowerMedian(values) -> number`、`class Agent`（`constructor(tree, compiler = null)`、`calls`・`ascii`・`other`・`seen`、`emit(tool, text)`、`grep(pattern, roots)`、`read(path, from, to)`、`chunk(path, k)`、`check(dir) -> string`、`edit()`、`covers(need)`、`get chars()`・`get tokens()`。Phase 2 で `fix(dir)`、Phase 3 で `command(args, cwd)`） |
| `benchmarks/ai/lib/corpus.mjs` | `Tree` | `ROOT`（`lib/corpus.mjs` の 3 つ上のディレクトリ。`resolve(dirname(fileURLToPath(import.meta.url)), "../../..")`）、`class Tree`（`constructor(base = ROOT)`、`files(root) -> string[]`、`lines(path) -> string[]`、キャッシュつき）、`isStablePath(path) -> boolean` |
| `benchmarks/ai/lib/tasks.mjs` | タスク | `TYPES`（記号 → `{name, file}`）、`loadTasks(dir) -> Task[]`、`resolveNeeds(rows, tree) -> Need[]`、`groupByPath(needs) -> [path, Need[]][]`、`isStableTask(task) -> boolean` |
| `benchmarks/ai/lib/derive.mjs` | 導出 | `stripComment`・`parseItems`・`referenceLines`・`callersOf`・`itemRange`・`deriveCallers(task, tree) -> {callers, needs, grep}`・`deriveImpact(task, tree) -> {tests, needs, grep}` |
| `benchmarks/ai/lib/strategies.mjs` | 戦略 | `STRATEGIES`（並び順の配列。要素 `{name, types, run(ctx, task) -> {agent, complete, rounds?}}`）、`mergeWindows`・`firstError`・`hasError`・`applyEdits`・`tryAttempts`。Phase 2 で `diagV2`・`diagFix`、Phase 3 で `kb` を足す |
| `benchmarks/ai/lib/report.mjs` | 行・レポート | `makeRow(task, strategy, outcome) -> object`、`serializeRows(rows) -> string`、`renderReport({rows, statics, meta, externals}) -> string`、`updateReportDocument(document, generated) -> string` |
| `benchmarks/ai/run.mjs` | CLI | `parseArguments(argv)`、`main(argv)`（`run`・`verify`・`report`・`task`）。`import.meta.url` が実行ファイルのときだけ `main` を呼ぶ |
| Phase 3: `lib/external.mjs`・`lib/static.mjs`・`lib/kb.mjs`・`lib/mcp.mjs` | 外部の結果・静的な成果物・KB・MCP | `readExternal(dir, taskIds) -> Frame[]`、`STATIC_ARTIFACTS`・`measureStatic(ctx) -> rows`、`KB_COMMANDS`・`kbProject`・`resolveKbId`・`kbSucceeded`、`mcpToolsList(compiler, cwd)` |

```js
/** @typedef {{path: string, from: number, to: number}} Need */
/** @typedef {{path: string, module: string, name: string, id: string, from: number, to: number, lines: string[], kind: "function"|"test"}} Item */
/** @typedef {{agent: Agent, complete: boolean, rounds?: number}} Outcome */
/** @typedef {{schema: "tz-ai-result/1", task: string, type: string, strategy: string, chars: number, calls: number, est_tokens: number, rounds?: number, code?: string, complete: boolean}} Row */
/** @typedef {{name: string, types: string[], applies(task): boolean, probe(ctx): string|null, run(ctx, task): Outcome}} Strategy */
// probe が null なら使える。文字列なら使えない理由（meta.json の skipped.reason）。applies は ctx.caps（能力の検出の結果）を見てよい
```

`ctx` = `{ compiler, tree, out, work, caps }`。`Task` と `Need` の形は仕様 4・5。**状態を持つのは `Agent` だけ**で、タスクごとに新しく作る（`seen` を共有しない）。ファイルの書き込みは `writeAtomically`（`benchmarks/report.mjs`）、診断タスクの複製は `fs.cpSync(..., { recursive: true })`、子プロセスは `child_process.spawnSync(compiler, args, { cwd, encoding: "utf8", timeout: CHILD_TIMEOUT_MS })`（Phase 3 の MCP だけ `spawn`）。`import` するのは `node:` の組み込みと `benchmarks/report.mjs` の `writeAtomically` だけ。

**アルゴリズムの計算量**: 1 タスクあたり、コーパスのファイル数 × 行数に比例（`std` 全体の grep で約 4 万行）。`Tree` がファイルごとに行をキャッシュし、`run` は 1 つの `Tree` を全タスクで共有する（診断タスクは複製ごとに新しい `Tree`）。

## 前提とする他チケットのインターフェース

| チケット | 使うもの（D-51・D-52・D-55 の名前） | 未実装のとき |
| --- | --- | --- |
| G22 | `check --json` の診断に `related`（`[{"role","message","path","span"}]`。`span` は `line`・`end_line` を持つ）。`E1012`・`E1013`・`E1014` に付く | `diag-v2` を出さない |
| G24 | `tsuzuri fix PATH`（stdout: `fixed Main.tz: 1 fix`・`applied 1 fix in 1 file`・`no fixes to apply`。stderr: 適用後の診断を `check` と同じ人間向けの形式で。終了コード 0 = 完了） | `diag-fix` を出さない |
| G21 | `tsuzuri guide`・`--level 1`・`std Array`・`--topic std-all`、`tsuzuri new` が書く `AGENTS.md`・`llms.txt` | 静的な表にその行が出ない |
| G27 | `tsuzuri kb build [PATH] [--std]`、`kb export [PATH] --symbols`（各行に `id`。D-52 の ID。std は `Array.fold`） | `kb` 系を出さない |
| G28 | `kb brief [--project\|--module M\|ID]`・`kb show ID`・`kb callers ID`・`kb tests-for ID`（テキスト出力。書式は G28 が決める） | その型の行だけ出さない |
| G25 | `tsuzuri find QUERY`（`kb find QUERY` と同じ実装。D-55） | D の `kb` を出さない |
| G29 | `tsuzuri mcp`（stdio。改行区切りの JSON-RPC 2.0、`protocolVersion` `2025-06-18`、`tools/list`） | `mcp-tools-list` の行を出さない |
| D-56・D-52 | サイズの上限（ガイド L0 6,000・L1 20,000・std の 1 モジュール 6,000・std 全体 45,000・`AGENTS.md` 3,500・`llms.txt` 1,500、brief L0 2,400・L1 1,600・L2 700） | 上限は `STATIC_ARTIFACTS`（付録 F.4）に定数で持つ |

コンパイラの対応は**能力の検出**（付録 F.2）で決める。G28・G25 が出荷した引数が表と違えば、直すのは `KB_COMMANDS` だけ。**暗黙に縮退しない**: 検出で使えない戦略は `meta.json`・`docs/ai-cost.md` に `reason` を残し、`--strategies` で明示されたら終了コード 1 のエラー。

## 実装手順

各手順の後でテストが緑（`node tests/ai_cost.mjs target/release/tsuzuri`）。1 手順の変更は 300 行以内。

### Phase 1

**手順 1: ベースライン（変更なし）。** 確認: `node --version` が v20 以上、`ls target/release/tsuzuri`、`node tests/benchmark_report.mjs` が通る、`git status --short` が空、`cat $(find std -type f \( -name '*.tz' -o -name '*.tt' -o -name '*.tc' \)) | wc -m` が 417695（違えば HEAD が動いている。停止条件 4 の見込み）。

**手順 2: データを置く（新規）。** `benchmarks/ai/corpus/shop/{Catalog,Main,Money,Orders,Report,Stock}.tz`（付録 C.1 のブロックの中身**そのまま**。末尾に改行 1 つ）、`benchmarks/ai/tasks/{a-definition,b-callers,d-type-search,e-spec,f-impact}.json`（付録 B のブロックそのまま）。確認: `LC_ALL=en_US.UTF-8 wc -m benchmarks/ai/corpus/shop/*.tz` が 941・460・770・1394・662・956（Catalog・Main・Money・Orders・Report・Stock の順）、`target/release/tsuzuri check benchmarks/ai/corpus/shop` が無出力で終了コード 0、`target/release/tsuzuri test benchmarks/ai/corpus/shop --list | wc -l` が 16、5 つの JSON が `JSON.parse` でき、タスクが 6 件ずつ。

**手順 3: テスト先行。** 新規 `tests/ai_cost.mjs`: 小さな実行器（`test(name, fn)`・`skip(reason)`。引数は先頭がコンパイラのパス、2 つ目が名前の部分文字列のフィルタ。末尾に `ai_cost: {p} passed, {f} failed, {s} skipped`、失敗があれば終了コード 1。`win32` なら `skip` を出して 0）と、テスト計画の 1・2（`estimate_tokens_matches_the_formula`・`chars_count_code_points_not_utf16_units`）。確認: `lib/measure.mjs` がないので `ERR_MODULE_NOT_FOUND` で終了コード 1。

**手順 4: `lib/measure.mjs`・`lib/corpus.mjs`。** 仕様 1〜3。`Agent` のツール（`grep`・`read`・`chunk`）と `Tree`。確認: 計画の 1〜7 が通る（`lowerMedian`・`Tree`・`grep` の書式と上限・`read` の丸め）。

**手順 5: `lib/tasks.mjs`。** `loadTasks`（仕様 4 の検証。未知のキーは `unknown key`）・`resolveNeeds`・`groupByPath`・`isStableTask`。確認: 計画の 8〜10。

**手順 6: `lib/derive.mjs`。** 仕様 6 のとおり。確認: 計画の 11〜13（shop の `callers`・`impact` が `expect` と一致、`test --list` と一致）。

**手順 7: `lib/strategies.mjs`（ベースライン 4 つ）。** 仕様 5 の 1〜4。確認: 計画の 14〜17（shop の golden が付録 D の表と 1 つ残らず一致）。

**手順 8: `lib/report.mjs`・`run.mjs`。** `run`・`verify`・`task`（`report` は手順 9）。`meta.json`（`rows`・`skipped`・`skipped_tasks`）。確認: 計画の 18〜23。`node benchmarks/ai/run.mjs run` の標準出力の 1 行目が `wrote 97 rows to target/ai-cost/results.jsonl`（std・文書が HEAD と同じなら）、`node benchmarks/ai/run.mjs verify` が `verified 30 tasks: 0 failures, 0 drift`（drift が出たら停止条件 4）。

**手順 9: `report` と `docs/ai-cost.md`。** `renderReport`・`updateReportDocument`・`--publish`・`--doc`。`docs/ai-cost.md` を新規に書く（骨格は付録 H。マーカー 2 行）。確認: 計画の 24〜26、`node benchmarks/ai/run.mjs report --publish` で `docs/ai-cost.md` に付録 D の表（型ごとの値）と同じ数値が入り、もう一度実行して `shasum docs/ai-cost.md` が変わらない。

**手順 10: CI と文書。** `.github/workflows/targets.yml` の段 `Benchmark correctness and metric records` の `node tests/benchmark_apps.mjs` の次に `node tests/ai_cost.mjs target/release/tsuzuri`、`README.md`・`docs/architecture.md`・`docs/benchmarks.md` に 1 行ずつ（「ドキュメント」）。確認: Node 20 で `node tests/ai_cost.mjs target/release/tsuzuri` が `0 failed`、`grep -nE "groupBy|\.union\(|\.intersection\(|\.difference\(|withResolvers|fromAsync|localeCompare" -r benchmarks/ai tests/ai_cost.mjs` が空。

### Phase 2

**手順 11: データとテスト先行。** 新規 `benchmarks/ai/corpus/diag/c01`〜`c07/Main.tz`（付録 C.2 そのまま）、`benchmarks/ai/tasks/c-diagnostic.json`（付録 B）。計画の 27・28 を書く。確認: `LC_ALL=en_US.UTF-8 wc -m benchmarks/ai/corpus/diag/c0*/Main.tz` が 1434・962・1214・1075・1249・1265・1221、各ディレクトリで `check . --json` の先頭が付録 D の表の `code`・`line`。

**手順 12: `diag-v1`。** `lib/strategies.mjs` に `diagV1`・`tryAttempts`・`firstError`・`hasError`・`applyEdits`・`itemRange`、`run`・`verify` が C を扱う（`verify` の検査 8）。確認: 計画の 29〜30。`run` が 104 行、`verify` が `verified 37 tasks: 0 failures, 0 drift`。

**手順 13: `diag-v2`。** `diagV2`・`diagV2From`（付録 F.1）、能力の検出（付録 F.2）。G22 がなければ `run` が `skipped strategy diag-v2: check --json has no related spans` を出す。確認: 計画の 31・33。G22 が入っているコンパイラでは、`diag-v2` の C-01 の行の `chars` が `diag-v1` と違う値で出る（読む範囲が変わる）。

**手順 14: `diag-fix`。** `diagFix`、`Agent.fix`。確認: 計画の 32・33。G24 が入っていれば、`fix` が直せるタスクの `calls` が 2・`rounds` が 0。

### Phase 3

**手順 15: 外部エージェントの枠。** `lib/external.mjs`、`report` の外部の表、`task` コマンド（Phase 1 で完成済み）の確認。確認: 計画の 34・35（`target/ai-cost/external/` に例の 1 行を置いて `report` に表が出る）。

**手順 16: 静的な成果物。** `lib/static.mjs`・`static.jsonl`・`run` への組み込み。確認: 計画の 36・37。HEAD では全行が出ない（`guide` がない）ので、`static.jsonl` は空のファイル、`report` に静的の節が出ない。

**手順 17: `kb`・`kb-oriented`。** `lib/kb.mjs`（付録 F.3）。確認: 計画の 38・39。`kb` がなければ `run` が `skipped strategy kb: …` を出して他は変わらない。

**手順 18: MCP の `tools/list`。** `lib/mcp.mjs`（付録 F.5）、`static.jsonl` の `mcp-tools-list`。確認: 計画の 40。

**手順 19: 文書と記録。** `docs/ai-cost.md` の本文に Phase 2・3 の説明を足し、`run` → `report --publish`。確認: `node tests/ai_cost.mjs target/release/tsuzuri` が `0 failed`、`git status --short` が新規ファイルと各 1 行の追記だけ。

## テスト計画

`tests/ai_cost.mjs`（新規。手本 `tests/benchmark_report.mjs` の assert の流儀）。**全 40 テストの入力と期待値は[付録 G](G31-appendix.md#付録-g-テスト計画の表)**。番号は「実装手順」が引く。期待値は独立した参照から作る（数式は手計算の表、golden は付録 D のシェルの計算、導出は `tsuzuri test --list` とシェルの `grep`）。一時ファイルは `target/ai-cost-test-{pid}/` の下に作り、`finally` で消す。

| 区分 | 関数（`tests/ai_cost.mjs::`）— 番号 |
| --- | --- |
| Phase 1 の測る量と `Tree` | 1 `estimate_tokens_matches_the_formula`、2 `chars_count_code_points_not_utf16_units`、3 `lower_median_picks_the_lower_middle`、4 `files_are_sorted_and_filtered`、5 `lines_drop_one_trailing_empty_element`、6 `grep_formats_orders_and_caps`、7 `read_clamps_and_marks_lines` |
| タスクと導出 | 8 `needs_must_match_exactly_one_line`、9 `task_files_are_valid`、10 `unknown_and_missing_task_keys_are_rejected`、11 `items_follow_the_keyword_rule`、12 `callers_and_impact_match_expect_on_shop`、13 `compiler_test_list_matches_parsed_tests` |
| 戦略（**golden**） | 14 `shop_goldens_for_the_grep_baselines`（付録 D の表のとおり）、15 `chunks_are_read_until_the_needs_are_covered`、16 `windows_merge_adjacent_ranges`、17 `docs_strategy_reads_the_index_first` |
| CLI・出力 | 18 `run_is_deterministic`、19 `result_rows_have_the_documented_keys`、20 `cli_usage_errors_exit_two`、21 `task_command_hides_the_answer`、22 `verify_passes_on_the_repository_tasks`、23 `verify_fails_on_a_broken_stable_task`、24 `report_tables_are_exact`、25 `report_replaces_only_the_marked_region`、26 `report_publish_updates_the_document` |
| Phase 2 | 27 `diag_corpus_reports_the_expected_first_diagnostic`、28 `diag_patches_behave`、29 `diag_v1_counts_calls_and_rounds`（`calls`/`rounds`: C-01 6/2、C-02 4/1、C-03 6/2、C-04 6/2、C-05 6/2、C-06 4/1、C-07 4/1）、30 `diag_rows_have_rounds_and_code`、31 `diag_v2_is_skipped_or_reads_related`、32 `diag_fix_is_skipped_or_finishes_in_two_calls`、33 `unavailable_strategy_is_an_error` |
| Phase 3 | 34 `external_frames_are_validated`、35 `external_table_uses_lower_medians`、36 `static_artifact_limits`、37 `static_rows_are_deterministic`、38 `kb_strategies_are_skipped_or_use_one_call`、39 `kb_success_uses_substring_checks`、40 `mcp_tools_list_row_or_skip` |

能力に依るテスト（31・32・33・36・38・40）は、コンパイラにその入口がなければ `skip(理由)` を出して通る（失敗にしない）。入口があるコンパイラでは本検査をする。診断の `chars` はコンパイラの文言に依るので固定しない（`calls`・`rounds` だけ固定）。

**既存テストへの影響**: なし。コンパイラ・既存のテスト・文書の記述に触れず、`targets.yml` は 1 行の追記だけ。**E2E・IR・差分テスト**: 対象外（コンパイラを変えない。ネイティブ・WASM の `-O0`／`-O3` に影響しない）。**資源上限**: 子プロセス 60 秒（`CHILD_TIMEOUT_MS`）、`grep` 100 行、`tests/ai_cost.mjs` 全体で約 15 秒（`run` 2 回を含む。HEAD の実測は `run` 1 回が約 5 秒）。

## ドキュメント

- **`docs/ai-cost.md`（新規。日本語）**: [付録 H](G31-appendix.md#付録-h-docsai-costmd-の骨格)の骨格（目的・測る量・タスクと戦略・結果（マーカーの間。`report --publish` が書く）・再現手順・限界の 6 項目）を置き、Phase 2・3 の戦略が加わったら表と説明を足す。**効果の主張は `baseline-grep-window` との比**と、限界の 6 項目は消さない。
- **`README.md`**: 検証コマンドの列で `node tests/parallel_frontend.mjs target/release/tsuzuri …` の行の直後に `node tests/ai_cost.mjs target/release/tsuzuri   # AI 作業コスト指標（G31）。模擬エージェントの読む量・往復数の定義、shop の golden、導出、診断タスク、レポートの書式。LLM を呼ばず決定的。結果は docs/ai-cost.md` を足す。
- **`docs/architecture.md`**: 「検証コマンド」の `sh` ブロックの `node tests/host_bindings.mjs target/release/tsuzuri` の行の直後に `node tests/ai_cost.mjs target/release/tsuzuri` を足す。
- **`docs/benchmarks.md`**: 冒頭の説明（最初の `###` 見出しの前）の末尾に 1 段落 `AI の作業コスト（入力量と往復数。速度ではない）は [ai-cost.md](ai-cost.md)。` を足す。
- **`.github/workflows/targets.yml`**: 手順 10。
- **更新しない**: `_tsuzuri/language-reference/`・`docs/language.md`・`docs/stability.md`（コンパイラの利用者向けの挙動が変わらない開発用ツール。PR の説明に「言語リファレンスの更新は不要」と書く）。`_features/README.md`・GUIDE の台帳は統括者が更新する（完了報告に新しい名前を列挙する）。
- 検証: `node tests/ai_cost.mjs target/release/tsuzuri`。`docs/ai-cost.md` は `_tsuzuri` の外なので `scripts/check-docs.mjs` の対象外（コード例を置かない）。

## 受け入れ条件

**Phase 1**
- [ ] `node tests/ai_cost.mjs target/release/tsuzuri` が Node 20 で `0 failed`（Phase 1 の関数 1〜26 がすべて `ok`）。
- [ ] `run` が 97 行の `results.jsonl` を書く（std・文書が HEAD と同じとき）。shop の golden が付録 D と 1 つ残らず一致する。
- [ ] `verify` が `verified 30 tasks: 0 failures, 0 drift`。`report --publish` が `docs/ai-cost.md` に表を書き、再実行で差分がない。
- [ ] `git status` が、新規ファイルと README・architecture・benchmarks・targets.yml の各 1 行の追記だけを示す。コンパイラ・既存のテストに差分がない。
- [ ] `grep -rn "/tmp" benchmarks/ai tests/ai_cost.mjs` が空。結果にホスト・日時・絶対パスがない（`results.jsonl` を `grep -c "/Users\|/home"` で 0）。

**Phase 2**
- [ ] C の 7 件が通り、`run` が 104 行、`verify` が `verified 37 tasks`。`calls`/`rounds` が計画の 29 の表のとおり。
- [ ] G22・G24 が入っていないコンパイラで `diag-v2`・`diag-fix` が `skipped strategy …` と理由つきで飛ばされ、明示するとエラー。入っているコンパイラで行が出る。

**Phase 3**
- [ ] 外部の結果の枠が検証され、`report` に表が出る。静的な成果物の上限テストが、測れたもの（0 件でも）で通る。
- [ ] `kb`・`kb-oriented`・`mcp-tools-list` が、コンパイラに G27・G28・G29 が入っているとき行を出し、入っていなければ理由つきで飛ばす。
- [ ] `docs/ai-cost.md` の本文が Phase 2・3 を説明し、再現手順が動く。

## 落とし穴

1. **Node 20 で動かない API**（CI は Node 24 なので見逃す）: `Map.groupBy`・`Object.groupBy`・`Set.prototype.union`・`Promise.withResolvers`・`Array.fromAsync`。検出: `grep -rnE "groupBy|\.union\(|\.intersection\(|\.difference\(|withResolvers|fromAsync" benchmarks/ai tests/ai_cost.mjs` が空、Node 20 で全テスト。
2. **コードポイントと UTF-16**: `"😀".length` は 2。`[...text].length` は大きな文字列で遅い。`for (const ch of text)` で数える（計画の 2）。
3. **整列とロケール**: `sort()` は比較関数なし（UTF-16 の順）。`localeCompare`・`Intl` を使わない（検出: `grep -rn "localeCompare\|Intl\." benchmarks/ai`）。シェルで独立に確かめるときは `LC_ALL=C sort` と `LC_ALL=en_US.UTF-8 wc -m`（`C` ロケールの `wc -m` はバイト数）。
4. **`check .` の `path`**: `cwd` を診断プログラムのディレクトリにして `.` を渡すと `./Main.tz`。先頭の `./` を取ってから `Tree` に渡す。別の渡し方（絶対パス）にすると `path` が変わり、字数も変わる。
5. **隠れた 2 つ目の誤り**: 診断は 1 つ直すと次が出ることがある（c03・c05 は直した）。`hasError` は**最後の試行の `check` 全体**を見る。プログラムを「簡単にする」整形をしない（付録 C.2 と 1 byte でも違うと golden と `fix` が壊れる）。
6. **`applyEdits` で `String.prototype.replace` を使わない**: 置換文字列の `$&`・`$1` が解釈される。`indexOf` と `slice` で置換する。
7. **`seen` の共有**: `Agent` を使い回すと前のタスクの行が「読んだ」ことになり `calls` が減る。タスクごとに作る（計画の 14 の golden が検出する）。
8. **`std`・文書の値を固定しない**: テストが固定してよいのは shop と diag の `calls`・`rounds`。std・文書の値は `docs/ai-cost.md` に書く。drift は `verify` の出力で見える（`--strict` は手作業の確認用）。
9. **結果の丸ごとの書き直し**: `results.jsonl` に追記しない（`writeAtomically`）。作業ディレクトリは `DIR/work`（`/tmp` ではない）。`finally` で消す。
10. **`est_tokens` の丸め**: 結果ごとに丸めて足すと値が変わる。合計の `ascii`・`other` に 1 回だけ適用する（計画の 14 の `A-05` が検出する）。

## 対象外

- LLM の呼び出し・実モデルの自動測定・料金・応答時間・実トークナイザ。外部の結果は人手で置く枠だけ。
- 速度の測定と合否の閾値。実行ファイルの性能（`docs/benchmarks.md`）。
- 実エージェントの再現。模擬エージェントは規則で定めた下限・上限。
- G32（`repo-kb brief`）の出力量を入力量に使う戦略（G32 は Tsuzuri 自身の開発用で、このチケットのタスクは利用者のプロジェクトを扱う。必要なら `STRATEGIES` に 1 つ足せる）。
- G23 の `explain`（診断 → `explain` の往復）、G29 のツールごとの戦略（`text` が CLI と同じなので `kb` と同じ値。固定費の `tools/list` だけ測る）、G30 の図。
- Windows（決定 D14）。診断のタスクの大きなプログラム版（31〜49 行を超える版。必要なら別の型 C のタスクを足す）。

## 決定事項

すべて承認済み。

- **D1: LLM を呼ばず、模擬エージェントの規則で決定的に数える。** 理由: 再現性・費用・機密。却下: 実モデルの自動測定（非決定的。モデルの更新で基準が動く）。人手の結果は外部の枠（D15）で合算する。
- **D2: 測る量は `chars`・`calls`・`est_tokens`。数えるのはツールの結果だけで、`est_tokens` は整数の式。** 理由: プロンプトや引数の長さはモデルの癖に依り、浮動小数の 1.3 は丸めが揺れる。却下: 実トークナイザ（依存と版の差）。
- **D3: ベースラインは 3 段（全文・500 行ずつ・±40 行）と `baseline-docs`。** 理由: 実エージェントの読み方は散るので範囲を示す。却下: 全文だけ（改善が過大に出る）。
- **D4: 効果の主張は `baseline-grep-window` との比で書く。** 理由: 最も厳しい下限との比なら、実際の削減は少なく見積もっても成り立つ。
- **D5: 完全性は `needs`（見えていなければならない行の範囲）で定める。** 却下: 回答の文字列の採点（LLM が要る）。
- **D6: 呼び出し元・影響範囲は規則で導出し、`expect` を独立の控えにする。項目は桁 0 のキーワード行（近似）で、shop は `tsuzuri test --list` と照合する。** 却下: 完全な構文解析（コンパイラの内部 API が要る。ハーネスは外から使うだけ）、手書きだけ（ずれる）。
- **D7: コーパスは `std`・言語リファレンス・合成の shop と diag・`tests/fixtures/unions`。`examples/` は使わない。** 理由: shop は呼び出し・テストの連鎖を持つ固定のコーパスで、golden を独立に検算できる。`examples/` は 11 ファイル 5,191 字で連鎖がない。
- **D8: C は壊れた小さな独立のプログラム 7 つ（E1012・E1014・E1003・E0002 に E1013・E1021・E1005 を足した）。直しは `first_try`（誤り）と `fix`（正）で、試行ごとに `edit` + `check`。** 却下: `examples/` の改変（例が変わると壊れる）。
- **D9: 結果は決定的で、時計・乱数・ホスト・環境変数を入れない。共有 CI に閾値を置かない。** テストが強制するのは定義と静的なサイズ上限だけ。出所（コミット・コンパイラ）は `meta.json` だけが持つ。
- **D10: `report` は Markdown のパイプ表。`benchmarks/report.mjs` からは `writeAtomically` だけを使う。** 理由: `renderTable` は HTML で、時間の列と「最速」の印を前提にする。
- **D11: 非安定なコーパス（std・文書・fixture）の不一致は drift（`verify` の既定は失敗にせず、`--strict` で失敗）。安定なコーパス（`benchmarks/ai/corpus/`）は常に失敗。** 理由: std・文書の無関係な編集が CI を落とさず、更新の通知は残る。
- **D12: 後続の戦略（v2・fix・kb・mcp）は能力の検出で自動。明示して使えなければエラー。** 理由: AGENTS.md（自動選択には検出と代替、明示した不在は誤り）。却下: 設定で有効化（出荷と同時に有効にならない）。
- **D13: KB 方式は文字列の一致で `complete` を近似し、ID は `kb export --symbols` で解決する（数えない）。MCP は `tools/list` の固定費だけ。** 理由: G28 の書式が未確定。MCP のツールの `text` は CLI と同じ（G29）。
- **D14: Node 20 以降・パッケージなし・Windows 対象外。** 理由: README の要件。CI は Linux・macOS。パスと CRLF の差を持ち込まない。
- **D15: 外部エージェントの結果は `tz-ai-external/1` の JSON Lines を人手で置く。`variant` は自由な条件名。** 理由: 実測が要る効果（ガイドで最初に書けたか）を同じ表で比べる。却下: 自動収集（鍵とネットワークが要る）。

## 実装と検証（2026-10-23）

- 人間の全 Phase 承認に従い、Phase 1〜3 を同じ隔離 worktree・ブランチで実装しました。Node 20.19.6、外部パッケージ・LLM・ネットワーク呼び出しなしです。`src/`・`std/`・既存レポート helper は変更していません。既存テストの例外は、後述する承認済みの formatter の負例分類だけです。
- 着手前に README へ D-59 と旧コーパスの照合をコミットしました。shop・diag は付録 C と byte 単位で同一、shop は 5183 字・16 テストです。D-02 の `get` → `try_item` と B-04 の現行 15 呼び出し元だけを、署名・独立した awk で確認してタスクに反映しました。ほかの B/F 11 件は元の期待値のままです。
- 手順 3 のテストを先に実行し、`ERR_MODULE_NOT_FOUND` で失敗することを確認してから計測コードを実装しました。`node scripts/slot.mjs -- node tests/ai_cost.mjs <既存 dev compiler>` は **40 passed、0 failed、5 skipped**（実コンパイラにない G22/G24/G21・KB/MCP）。未実装入口の成功経路もローカル子プロセスの fixture で検査しました。fixture の値を実測には含めません。
- 独立した付録 D の shop golden 27 組が全一致し、`sh benchmarks/ai/golden.sh` の grep/sort/wc による 9 件の検算も一致しました。コンパイラの `check`・`test --list`、診断 7 件のコード・行・正誤パッチ、修復の calls/rounds を検査しました。
- `run` は基準コンパイラで 104 行（Phase 1 の 97 行＋C の 7 行）です。`verify --strict` は **37 tasks、0 failures、0 drift**。異なる出力先への 2 回の実行で results/static/meta の byte 一致と work の消去を検査しました。
- 公開レポートも 2 回生成して byte 一致を確認しました。変更文書のリンク・アンカー 6 件が成功し、既存の `check-docs.mjs`（検査対象は `_tsuzuri/` のみ）で関連する引数順序のページを検査しました: **1 ページ・41 リンク・3 例・native O0/O3 の 6 実行が成功**。コンパイラを走らせる最終文書検証には隔離した `TSUZURI_CACHE_DIR` を使い、共有キャッシュは変更・削除していません。
- Phase 3 の外部 JSONL の検証・表、10 種の静的成果物の計測・サイズ上限、KB の事前構築・ID 解決・1/2 往復、MCP initialize/tools/list と終了処理を実装しました。G21/G27/G28/G29 自体は実装していません。不在の能力は測定値を捏造せず、明示した不在戦略はエラーです。
- PR #43 の Copilot レビューの 7 指摘を修正しました: KB コピーの basename 保持、正しいプロジェクトでの能力検出、falsy な needs の拒否、ファイル欠落時の論理パスによる決定的な理由、制御した文書 fixture、スキップ数に応じた件数検査、CI では drift を許す通常 verify です。各回帰ケースと深い MCP 応答の安全な拒否を追加し、再び **40 passed・0 failed・5 skipped** を確認しました。固定 golden は未変更です。
- 再レビューの KB 2 件も修正しました。型別の不在を戦略全体の不在と混同しないテストと、build/export 失敗時にも短い ID で照会し、成功した baseline と不完全な KB 行を保存する回帰テストを追加しました。export 成功時の ID 欠落・曖昧性は引き続きエラーです。export 失敗の回帰は修正前に `0 matches` で失敗し、修正後の全体は **40 passed・0 failed・5 skipped** です。
- CI `39968979163` は Rust **1717 passed・1 failed・2 skipped** で、唯一の失敗は formatter の成功コーパス走査が意図した構文エラー `diag/c04/Main.tz` を読んだことでした。コーディネーターの明示承認で、当初の「既存テストを変更しない」の例外として `tests/formatter.rs` のみを調整しました。負例の `benchmarks/ai/corpus/diag` だけを除外し、shop の全 6 ファイルが引き続き検査される件数アサートを加えました。コンパイラ・13 コーパスファイル・golden は未変更です。
- このアサートの負例検証では、一時的に AI コーパス全体を除外すると **0 != 6** で失敗しました。正しい `diag` だけの除外に戻して、formatter **19 passed**（`--test-threads 2`）、fmt、clippy が成功しました。隔離 worktree で作った dev binary でも Node 全体は **40 passed・0 failed・5 skipped**、変更文書の 6 リンク・アンカーも成功です。
- 統括者の同一タスク追加指示（2026-10-24）で、完了カタログの G31 2 行を Phase 1〜3 の実装済みに更新しました。カタログの運用規則に従って本チケットと付録を `_completed/` へ移し、参照リンクを更新しました。文書 5 ページの 243 ローカルリンク・アンカーを検査しました（`check-docs.mjs` の対象外なので同じリンク・見出し規則を使用）。付録 B・C・D と生成した測定表の byte 一致、実行コードの無差分を確認し、Rust・機能テストの再実行はしていません。
- 文書: README、docs/ai-cost.md、docs/architecture.md、docs/benchmarks.md、付録の現行 API 追補。targets.yml の既存ベンチマーク検証に 1 行を追加しました。言語利用者の挙動は変わらないため、言語リファレンス・docs/language.md の更新は不要です。計測自体に Rust の再ビルドは不要ですが、承認済みの formatter テスト変更は隔離 worktree でビルド・検証します。main の基準 CI `39954858404`（`41f80bc7`）は成功でした。
- 主なファイル: benchmarks/ai/run.mjs、lib/{measure,corpus,tasks,derive,strategies,report,external,static,kb,mcp}.mjs、tasks/*.json（6）、corpus/shop/*.tz（6）・corpus/diag/c0*/Main.tz（7）、golden.sh、tests/ai_cost.mjs と上記文書です。

## 付録

[G31-appendix.md](G31-appendix.md): A タスクの一覧（37 件）、B タスクの JSON、C コーパスの全文、D 検証の記録（golden・型ごとの中央値・診断の実出力）、E 出力と検査の書式、F 後続 Phase の戦略、G テスト計画の表、H `docs/ai-cost.md` の骨格。
