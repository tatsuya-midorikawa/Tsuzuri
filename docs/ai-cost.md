# AI 作業コスト（入力量と往復数）

Tsuzuri の AI 向けの機能が、エージェントの読む量と往復を減らすかを、LLM・ネットワーク・外部パッケージなしで決定的に測ります。速度の記録は [benchmarks.md](benchmarks.md)、完了した設計・検証記録は [G31](../_features/_completed/G31-ai-cost-metrics.md) です。

## 測る量

| 名前 | 意味 |
| --- | --- |
| `chars` | 受け取ったツール結果の Unicode コードポイント数の合計（絵文字も 1 文字） |
| `calls` | ツール呼び出しの回数。空の結果も 1 回です |
| `est_tokens` | `ceil(ASCII / 4 + 非 ASCII × 1.3)`。整数の式 `floor((10 × ASCII + 52 × 非 ASCII + 39) / 40)` で合計を一度だけ丸めます |
| `rounds` | 診断を直す試行（`edit` と `check`）の回数 |
| `complete` | 必要な情報を取得したか（KB は部分文字列による近似） |

問い、エージェントの出力・思考、ツール引数、KB の事前構築は数えません。推定トークンは実トークナイザではなく、日本語を ASCII の 5.2 倍に数える概算です。表は各列の**下位の中央値**で、偶数件でも平均しません。

## タスクと戦略

6 型 37 件（定義 A: 6、呼び出し元 B: 6、診断 C: 7、型検索 D: 6、仕様 E: 6、影響範囲 F: 6）です。`node benchmarks/ai/run.mjs task ID` は問いだけを出し、正解・必要な行・修正パッチは隠します。

| 戦略 | 模すもの |
| --- | --- |
| `baseline-grep` | 固定文字列を grep し、ヒットしたファイルを全文読みます（素朴な上限） |
| `baseline-grep-chunk` | 必要な行が見えるまで先頭から 500 行ずつ読みます |
| `baseline-grep-window` | grep 後、まだ見ていない必要な行の ±40 行を読みます（最も厳しい下限） |
| `baseline-docs` | 言語リファレンスの目次から、ページを先頭から 500 行ずつ読みます |
| `diag-v1` | `check` → ±40 行と項目全体 → 手書きの修正試行 → `check` |
| `diag-v2` | 主スパン・関連位置と不足する必要行の ±5 行を読み、同じ修正試行を行います |
| `diag-fix` | `check` → 複製で `fix`。直らなければ元のソースから v2 の試行へ戻ります |
| `kb`・`kb-oriented` | 型ごとの KB 照会 1 回。oriented は先に `kb brief --project` を読みます |

**効果の主張は、最も厳しい下限 `baseline-grep-window` との比で書きます。** 診断・KB の入口は実際のコンパイラで検出し、不在なら理由つきで省きます。`--strategies` で不在の戦略を明示するとエラーです。

Phase 3 の外部実測の取り込み、静的サイズ測定・上限検査、KB/MCP アダプターも実装済みです。ただし基準 `41f80bc7` のコンパイラに G21（guide/生成する agent ファイル）、G27/G28（KB）、G29（MCP）は**ありません**。該当の測定行は出さず、架空の成果物・KB・MCP 実装や改善値を作りません。MCP は `tools/list` の固定費だけを測り、個々のツールの効果とはしません。G32 の repo-kb は対象外です。

KB の可用性は照会の種類ごとに判定します。たとえば `kb find` がなくても、利用できる種類の行は測ります。事前構築の失敗では中断せず、シンボル export が非ゼロ終了なら短い ID で照会を計測し、その応答から `complete` を判定します。export が成功したのに ID が 0 件・複数件なら、設定不整合としてエラーです。

## 結果

<!-- generated:ai-cost -->
測定: tsuzuri 0.1.0、コミット 3ce98f8ce35b、タスク 37 件、行 104

### 型ごとの値（下位の中央値）

| 型 | 戦略 | 件数 | chars | calls | est_tokens | complete |
| --- | --- | --- | --- | --- | --- | --- |
| definition | baseline-grep | 6 | 7743 | 3 | 1936 | 6/6 |
| definition | baseline-grep-chunk | 6 | 218 | 1 | 55 | 6/6 |
| definition | baseline-grep-window | 6 | 218 | 1 | 55 | 6/6 |
| definition | baseline-docs | 1 | 15122 | 2 | 6966 | 1/1 |
| callers | baseline-grep | 6 | 34476 | 4 | 8619 | 6/6 |
| callers | baseline-grep-chunk | 6 | 27412 | 3 | 6853 | 6/6 |
| callers | baseline-grep-window | 6 | 7781 | 3 | 1946 | 6/6 |
| diagnostic | diag-v1 | 7 | 1666 | 6 | 417 | 7/7 |
| type-search | baseline-grep | 6 | 9451 | 2 | 2363 | 6/6 |
| type-search | baseline-grep-chunk | 6 | 127 | 1 | 32 | 6/6 |
| type-search | baseline-grep-window | 6 | 127 | 1 | 32 | 6/6 |
| spec | baseline-grep | 6 | 397843 | 5 | 320291 | 6/6 |
| spec | baseline-grep-chunk | 6 | 799 | 1 | 470 | 6/6 |
| spec | baseline-grep-window | 6 | 799 | 1 | 470 | 6/6 |
| spec | baseline-docs | 6 | 15410 | 2 | 7868 | 6/6 |
| impact | baseline-grep | 6 | 4320 | 8 | 1080 | 6/6 |
| impact | baseline-grep-chunk | 6 | 3860 | 6 | 965 | 6/6 |
| impact | baseline-grep-window | 6 | 3860 | 6 | 965 | 6/6 |

### 診断の修復（C）のタスクごとの値

| タスク | コード | 戦略 | chars | calls | rounds | est_tokens |
| --- | --- | --- | --- | --- | --- | --- |
| C-01 | E1012 | diag-v1 | 1824 | 6 | 2 | 456 |
| C-02 | E1014 | diag-v1 | 1241 | 4 | 1 | 311 |
| C-03 | E1003 | diag-v1 | 1757 | 6 | 2 | 440 |
| C-04 | E0002 | diag-v1 | 1666 | 6 | 2 | 417 |
| C-05 | E1013 | diag-v1 | 1681 | 6 | 2 | 421 |
| C-06 | E1021 | diag-v1 | 1462 | 4 | 1 | 366 |
| C-07 | E1005 | diag-v1 | 1449 | 4 | 1 | 363 |

### 測らなかったもの

- 戦略 `diag-v2`: check --json has no related spans
- 戦略 `diag-fix`: tsuzuri fix is not available
- 戦略 `kb`: tsuzuri kb show is not available (definition); tsuzuri kb callers is not available (callers); tsuzuri kb find is not available (type-search); tsuzuri kb tests-for is not available (impact)
- 戦略 `kb-oriented`: tsuzuri kb brief is not available
- 戦略 `mcp-tools-list`: tsuzuri mcp did not answer tools/list
<!-- /generated:ai-cost -->

## 再現手順

Node.js 20 以降と既存のコンパイラを使います（計測の実行に Rust の再ビルドは不要です）。

```sh
node scripts/slot.mjs -- node benchmarks/ai/run.mjs verify --compiler target/debug/tsuzuri --strict
node scripts/slot.mjs -- node benchmarks/ai/run.mjs run --compiler target/debug/tsuzuri
node benchmarks/ai/run.mjs report --publish
node scripts/slot.mjs -- node tests/ai_cost.mjs target/debug/tsuzuri
sh benchmarks/ai/golden.sh
```

CLI は `run|verify|report|task ID` です。`run` の `--types A,C` と `--strategies NAME,..` で絞れます。`--compiler` の既定は `target/release/tsuzuri`、`--out` は `target/ai-cost`、`--tasks` は `benchmarks/ai/tasks` です。既定値だけがリポジトリ基準で、明示したパスはカレントディレクトリ基準です。`verify --strict` は std・文書・fixture の drift も失敗にします。固定コーパスの不一致は常に失敗です。

結果は `results.jsonl`（`tz-ai-result/1`）、`static.jsonl`（`tz-ai-static/1`）、出所だけを持つ `meta.json`（`tz-ai-meta/1`）です。結果に日時・ホスト・絶対パスは入れません。作業コピーは出力先の `work/` に置き、終了時に消します。`report --publish` はマーカー間だけを原子的に書き換えます。`report --doc PATH --publish` なら別の複製に公開できます。

外部の実測は `target/ai-cost/external/*.jsonl` に置くと、agent・variant・型ごとの表に合算されます。例の 1 行は次の形式です（これは実測値ではありません）。

```json
{"schema":"tz-ai-external/1","task":"B-05","agent":"example","variant":"plain","tokens_in":12345,"tokens_out":321,"turns":4,"success":true}
```

不正な JSON、未知のタスク、名前の形式違反、負数・非整数・安全でない整数、真偽以外の success は拒否します。未知のキーは無視し、結果には転記しません。ハーネス自身はエージェント・API・LLM を呼びません。

### 固定データと独立した参照

shop の 6 ファイルは旧付録の全文と byte 単位で同じ（5183 文字）、テスト 16 件です。全 27 組の shop golden は旧付録 D の手計算値を固定し、`golden.sh` が独立した grep/sort/wc の足し算でも検算します。結果を期待値にコピーしません。診断 7 件は元のコード・行と正しい修正・誤った初回修正を検査し、文言に依存する文字数ではなく calls/rounds を固定します。

着手時の D-59・現在の API との照合は [README](../README.md#検証と性能測定) に記録しました。D-02 は `Array.get` から `Array.try_item`、B-04 は独立に確認した現在の 15 呼び出し元に合わせただけです。std・文書の量は現 HEAD から測ります。能力が不在の場合も、テスト用の小さなローカル子プロセスで各アダプターの成功・失敗を検査します（実測の行へは混ぜません）。

診断コーパスには意図した構文エラーも含まれるため、有効なプログラムを対象とする Rust の formatter 検証では `corpus/diag` だけを除外します。`corpus/shop` の全 6 ファイルは引き続き構文木・冪等性を検査し、その件数も明示的に確認します。これはテスト範囲の分類で、コンパイラや固定コーパスの変更ではありません。

## 限界

- 模擬エージェントは「経路を知っている」下限と素朴な上限の範囲であり、実エージェントの再現ではありません。
- 呼び出し元・影響範囲の導出は近似（桁 0 のキーワード行を項目とします）。
- 診断のプログラムは 31〜49 行で、±40 行の窓がファイル全体になります（`diag-v1` の読みが小さく出ます）。
- std・文書のタスクの値は、それらの編集で動きます。
- KB 方式の `complete` は文字列の一致による近似です。
- LLM の回答の質・応答時間・料金は測りません。

Windows は対象外で、テストは理由を出してスキップします。共有 CI に速度の閾値は設けません。ガイド・brief の文字数上限だけを契約として検査します。
