# G29: `tsuzuri mcp`（MCP サーバー）

| 項目 | 内容 |
| --- | --- |
| ID | G29 |
| 優先度 | P2 |
| 規模 | 全体 L（Phase 1: M、Phase 2: M、Phase 3: S、Phase 4: S）。D-50 の表は M だが、ツールが 11 個（種の 10 個 + `tz_search`）になり、Phase 2 が依存チケットごとの接続（各 S）になったので見直した |
| 依存 | Phase 1: なし（HEAD だけ。`tz_check` は `check --json` と同じ解析）。Phase 2 はツールごとに、`tz_guide` が [G21](G21-ai-guide-pack.md)、`tz_explain` が [G23](G23-diagnostic-kinds-explain-hints.md)、`tz_symbol`・`tz_brief`・`tz_search`・`tz_callers` が [G26](G26-facts-model.md)・[G27](G27-build-time-kb.md)・[G28](G28-kb-query-and-impact.md)、`tz_impact` が G28 Phase 3、`tz_find` が [G25](../G25-typed-holes-and-find.md)、`tz_fix_plan` が [G24](G24-tsuzuri-fix.md)、`tz_diagram` が [G30](G30-diagrams.md)。弱い依存: [G22](G22-diagnostic-v2.md)（診断の追加キー。なくても動き、あれば自動で出る）。Phase 3 は Phase 1 と、資源ごとに G21・G23・G27 |
| 後続 | [G31](G31-ai-cost-metrics.md)（計測アダプターは実装済み。MCP 方式の実測は本 G29 の提供後）。VS Code 拡張（`vsc/`）からの自動登録は対象外 |
| 状態 | done（Phase 1〜4。D-60 の KG。2026-10-28） |
| 起票 | 2026-10-18（D-50。HEAD `a2313d19` で確認。作業ツリーは `9dd0fbbc` で、差は `docs/benchmarks.md` だけ） |
| 承認 | 承認済み（D-50: 人間の包括許可 2026-10-18）。決定事項 D1〜D18 はすべて承認済み |
| 改善する劣位 | AI エージェントが診断・知識を得る道は、シェルで `tsuzuri check --json` を起動して標準エラーを読み、ファイルを `grep` することだけで、ツールの型も結果の形も機械可読に宣言されていない（D-50 の結論 (1)）。MCP は VS Code・Claude Code・Copilot CLI・Cursor が共通に持つ接続口で、本チケットは、その上に読み取り専用で型付きの 11 ツールを置く |
| 手本にする既存実装 | stdio の JSON-RPC と取り消し: `src/lsp.rs` の `serve`（入力を読むスレッド・`InputEvent`・`$/cancelRequest` の集合・`-32700`／`-32600`／`-32601`／`-32602`）。サブコマンドの形: `src/main.rs` の `api_diff_command`・`parse_repl_arguments`（`seen` で重複を拒否）・`repl_command`・`print_diagnostic`・`invalid_thread_setting`。解析: `src/driver.rs` の `Project::load`・`analyze_all`・`source_for`・`collect_sources`（シンボリックリンクを `E1011` で拒否）。診断の描画: `src/diagnostic.rs` の `render_with_severity`・`json_with_severity`（G22 の後は `render_with`・`json_with`）・`DiagnosticSet::omission_note`。`src/cache.rs` の `real_path`・`Sha256`。`src/parallel.rs` の `WORKER_STACK_BYTES`。テスト: `tests/lsp.rs` の `scripted`・`session`、`tests/api_diff.rs` の `tsuzuri(&arguments)`（子プロセス） |
| 主な影響ファイル | 新規: `src/mcp/mod.rs`・`rpc.rs`・`schema.rs`・`tools.rs`・`tools.json`・`session.rs`・`backend.rs`・`path.rs`・`check.rs`（Phase 1）、`knowledge.rs`・`cache.rs`（Phase 2）、`resources.rs`（Phase 3）、`tests/mcp.rs`、`tests/mcp_knowledge.rs`（Phase 2）、`_tsuzuri/language-reference/compiler/mcp.md`。変更: `src/lib.rs`（`pub mod mcp;`）、`src/main.rs`、`src/parallel.rs`（`WORKER_STACK_BYTES` を `pub(crate)` にするだけ）、文書（「ドキュメント」）。**`Cargo.toml` は変えない** |

詳しい JSON・Rust の骨格・検証記録は 2 つの付録にある（以下「付録 A〜H」）。付録 A〜D（仕様・JSON）は [G29-appendix.md](G29-appendix.md)、付録 E〜H（実装・テスト・測定）は [G29-appendix-impl.md](G29-appendix-impl.md)。付録 B の `tools.json` は、そのまま `src/mcp/tools.json` に写す。

## KG への更新（D-60、2026-10-28）

本チケットは [D-60](../GUIDE.md#d-60-kb-から-kg知識グラフへの再設計g26g27g28g29g30) で KB から **KG（知識グラフ）** に改めた。オプションは `--kg-dir`（既定 `<根>/target/tz/kg`）、能力は `kg`、資源は `tz://kg/brief/L0`・`tz://kg/brief/{id}`。

| 本文 | KG での実装 |
| --- | --- |
| 11 ツール | 12 ツール。**`tz_path`**（`from`・`to` が必須、`relations`（既定 `["calls"]`）・`depth`（1〜16、既定 8）。G28 の `kg path`）を `tz_callers` の次に足す。`tools/list` の応答は 16,384 bytes 以下を保つ |
| `tz_find`（G25 が `done` のとき） | G28 Phase 2 の KG の型検索（D-60）で有効にする |
| KB の 4 ツールの `kb_incomplete` | `kg_incomplete`（本文は同じ） |
| `tests/mcp_knowledge.rs` | 同じ名前。KG のツールのテストを足す |

## 実装と検証（2026-10-28）

[D-60](../GUIDE.md#d-60-kb-から-kg知識グラフへの再設計g26g27g28g29g30) の KG の名前で Phase 1〜4 を完了した（ブランチ `feat/g29-mcp-server`）。依存のチケット（G21・G23・G24・G26・G27・G28・G30）がすべて `done` なので、12 のツール（D-60 の `tz_path` を含む）をすべて有効にした。`tz_find` は G25 でなく G28 の KG の型検索（D-60）。

**実装したもの**: `src/mcp/{mod,backend,rpc,schema,tools,session,path,check,knowledge,cache,resources}.rs`・`src/mcp/tools.json`（付録 B に D-60 の変更: `tz_path` を `tz_callers` の次に、説明の knowledge base を knowledge graph に）、`src/main.rs`（`tsuzuri mcp`・`parse_mcp_arguments`・`HELP`）、`src/lib.rs`（`pub mod mcp;`）、`src/parallel.rs`（`WORKER_STACK_BYTES` を `pub(crate)`）、`src/kg/build.rs`（`memory`: KG をメモリの上だけに作る。frontend のキャッシュも使わない）、`src/kg/find.rs`（`Match.parameters`。G25 の `how`）、`src/diagnostic_kinds.rs`（`E2000` の `mcp_bad_arguments`、`E2010` の `mcp_root_unusable`・`mcp_io_failed`・`mcp_worker_failed`）、`src/explain/codes/E2010.md`。テスト `tests/mcp.rs`（46 件: 群 P 7・T 11・C 10・K 6・L 6、資源 5、敵対的な入力 1）・`tests/mcp_knowledge.rs`（17 件）、単体テスト（`rpc` 4・`schema` 4・`tools` 5・`path` 2・`resources` 1・`cache` 1、`main.rs` の `parses_mcp_arguments`）、`tests/fixtures/mcp/tool-examples.json`。

**実行したコマンドと結果**: `cargo nextest run --locked` の全件（2,347 件）が通過。`cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings` が通過。`node scripts/check-docs.mjs` で変更した 5 ページが通過。手順 1 の比較: `examples/` と `tests/fixtures/` の 123 のプロジェクトで、G29 の前のバイナリ（release）と後のバイナリの `check --json --no-kg` の標準出力・標準エラー・終了コードが 123 件すべて同じ。**公式 SDK**（付録 F.2。`@modelcontextprotocol/sdk` 1.32.1、Node 20.17、リポジトリの外に置いた）で `kg_shop` の写しに接続し、`initialize`・`ping`・`tools/list`（12 ツール）・12 ツールの `callTool`（全部 `ok`・`structured`。クライアントが `structuredContent` を `outputSchema` で検証する）・`resources/list`・`resources/templates/list`・`resources/read`（`tz://kg/brief/L0`）が通り、最後が `DONE`。`tools/list` の応答は 2025-06-18 で 16,062 bytes（16,384 以下）。

**期待値の出どころ**: プロトコルの期待は付録 C の JSON（MCP の仕様の例）と本文の表から、テストの中に書いた。`tz_check` の診断は同じ根の `tsuzuri check --json` の行、知識グラフ・`explain`・`guide`・`fix`・図の期待は、それぞれの CLI（`kg show`・`kg brief`・`kg search`・`kg callers`・`kg uses`・`kg path`・`kg find`・`kg impact`・`kg diagram`・`explain`・`guide`・`fix --dry-run`）の出力で、MCP の出力を写していない。`tool-examples.json`（偽のバックエンドが返し、`outputSchema` で検証する入力）は、付録 D の「依存チケットの実装後に実際の出力で置き換える」に従い、`examples/point` での実際の出力にした。

**チケットから外れた判断**:

1. D-60 の名前: オプション `--kg-dir`（既定 `<根>/target/tz/kg`）、`Capabilities::kg`、資源 `tz://kg/brief/L0`・`tz://kg/brief/{id}`、不完全な KG のメッセージは本文のまま。
2. 付録 G.1 の誤りのプログラム（`consume text` を 2 回）は、今のコンパイラでは誤りにならない（`Tsuzuri.toml` のない入力は edition 2027 で、読むだけの引数は借用）。テストは `own string` の引数で `E1012` を出す同じ形のプログラム（5:26）を使う。`structuredContent` の診断は G22 の後の `check --json` と同じく `kind`・`related`・`help` を持つ。
3. Phase 1〜4 を 1 つの変更で出したので、`initialize` は最初から `resources: {}` を宣言する（付録 C.1 の Phase 3 の形）。
4. KG の取得: `manifest::inspect` が新しいと言えば `KgData::open`（`facts/`・`layout/` は G28 の遅延読み込みで、読むだけ）、そうでなければ `kg::build::memory`。どちらも書かない（`kg_acquisition_writes_nothing` が、全ツールの呼び出しの前後で根の下の全ファイルのパス・大きさ・更新時刻が同じことを確かめる）。std の KG は `StdSource::Memory`。
5. `tz_symbol` の `symbol` は、辺の行き先にだけ現れる ID（`IO.write_line`）では `{"id","kind":"external","name"}`（`outputSchema` の必須キーを満たす）。
6. `tz_callers` の `uses_type` で `depth` が 2 以上のときは、`uses_type` の辺を逆に幅優先でたどった行と、`kg callers` と同じ形の text（`uses of X (depth D)`）。`depth` 1 の text は `kg uses` と同じ。`calls` は `kg callers` と同じ（開始の関数の自己呼び出しは再び出さない）。
7. `tz_search` の `kind`・`limit` で行を落としたときの text は、`kg search` と同じ形の行を残ったものから作る。落とさなければ `kg search` の text そのもの。
8. `tz_find` の text の最後の `... N more; raise --limit (at most 100)` は、ツールの引数に合わせて `raise limit (at most 10)` にする。`how` は引数があれば `call`、なければ `value`（G25 の規則）。
9. `tz_impact` の `old`・`new` は根の下のディレクトリで、マーカーのある KG はそのまま読み、プロジェクトはメモリの上で KG にする。ラベルは与えた値、`new` を省くと根のプロジェクトで `working tree`。`.tsuzuri-kg` のマーカーも `Tsuzuri.toml` もソースもないディレクトリは `'{p}' is neither a knowledge graph nor a project directory`。
10. 結果のキャッシュの鍵は、ツール名・引数・入力のハッシュ（プロジェクトを読むツールは KG の `source_hash`、`tz_guide`・`tz_explain` は空）。`tz_impact` は別のディレクトリを読むのでキャッシュしない。誤りの結果はキャッシュしない。
11. 未知の ID のメッセージは `unknown id '{id}'; use tz_search to find ids` に、G28 の候補があれば `; did you mean: …` を足す。
12. `src/main.rs` の「直接出力してよい関数」の一覧に `mcp_command` を足した（サーバーは `mcp` を実行しない）。
13. `tools/list` が 16,384 bytes に近い（16,062）。説明を足すときは `tools_list_stays_small` が止める。

**更新した文書**: `_tsuzuri/language-reference/compiler/mcp.md`（新規）・`usage.md`（`### mcp`）・`option.md`（`mcp` の引数と `TSUZURI_THREADS`）・`diagnostics.md`（`E2010`・`E2000` と kind の一覧）・`index.md`、`README.md`（開発支援、「MCP サーバー」の節、CLI）、`docs/language.md`（`E2010` と `tsuzuri mcp` の段落）、`docs/architecture.md`（ファイル表と「MCP サーバー（G29、D-60）」）、`docs/stability.md`。

## 実装者向け早見

1. **作るもの**: `tsuzuri mcp [--root DIR] [--kb-dir DIR]`。stdio の MCP サーバー。改行区切りの JSON-RPC 2.0。**読み取り専用**で、ファイルを書かず、子プロセスを起こさず、ネットワークを使わず、自分では環境変数を読まない。
2. **最初に読む**: 付録 A（仕様の規則）→「仕様」→「設計」→ `src/lsp.rs` の `serve`、`src/main.rs` の `api_diff_command`・`parse_repl_arguments`、`src/driver.rs` の `Project::load`・`analyze_all`。
3. **最初に書くテスト**: 手順 2 の `tests/mcp.rs::initialize_negotiates_the_protocol_version`（先に赤にする）。
4. **触ってはいけない**: `check --json` の出力・終了コード、`src/lsp.rs`、`Diagnostic` の型、IR、`Cargo.toml`（crate も feature も足さない。`serde_json` の `preserve_order` は **足さない**。出力のキーは辞書順になる）。
5. **新しい名前**: サブコマンド `mcp`、フラグ `--root`・`--kb-dir`（`--kb-dir` は D-55 の既存）、診断 `E2010`（D-55）、ツール名 `tz_*` の 11 個。新しい環境変数・予約語・crate・WASM の import はない。
6. **設計の核**: 本体は `Backend`（付録 E）の 1 つの trait。ツールの表は `tools.json` 1 つ。依存チケットが未完でも、そのツールを `tools/list` に出さないだけで、残りは動く（`Capabilities`）。
7. **stdout は JSON の行だけ**。`println!`・`print!`・`dbg!` を `src/mcp/` に書かない（静的な検査のテストがある）。診断やログは stderr。
8. **tools/call は 1 本ずつ、先に来たものから**。`ping`・`initialize`・`tools/list`・取り消しは、ツールの実行を待たずに主スレッドで答える。
9. **Phase 1 の完了**: `tsuzuri mcp` が公式の TypeScript SDK のクライアントで `initialize`→`tools/list`→`tools/call tz_check` を通り（付録 F）、`tests/mcp.rs` が緑で、`tools/list` に `tz_check` だけが出る。
10. **Phase 2 の完了**: 依存が `done` のツールが 1 つずつ `tools/list` に出て、そのツールの `tests/mcp_knowledge.rs` のテストが緑。**未完の依存のツールは足さない**（手順を飛ばす）。
11. **Phase 3 の完了**: `initialize` が `resources: {}` を宣言し、`resources/list`・`templates/list`・`read` が付録 C のとおり。**Phase 4 の完了**: 敵対的な入力の表のテストと文書。
12. 各手順の最後で `cargo build --locked`・`cargo test --locked`・`cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings` が緑。`cargo test <名前>` は 0 件でも成功するので `running N tests` の N を確かめる。

## 目的

**問題。** HEAD の AI は、診断を得るために `tsuzuri check --json` を子プロセスで起動し、標準エラーの 1 行 1 JSON を読む。そこには主スパン 1 個の診断しかなく、プロジェクトの構造・呼び出し・効果を知るには、ファイルを探索して読む。`tsuzuri mcp` は今ない: `tsuzuri mcp` は `mcp:1:1: error[E2001]: cannot inspect source 'mcp'`（終了コード 1）、`tsuzuri mcp --root .` は `error[E2000]: unknown option '--root'`（終了コード 2）になる（「現状」1）。

**価値。**

1. 診断を、構造化した 1 回の呼び出し（`tz_check`）で得る。パスは根からの相対で、決定的。
2. KB・`explain`・`find`・`impact`・`fix`・図を、ファイルを探索せずに呼べる（G26〜G30 が出荷された順にツールが増える）。
3. 引数と結果が JSON Schema で宣言される。小さいモデルでも呼び間違いにくく、検証の失敗はモデルに見える（`isError`）。

**HEAD で測った数字**（付録 G）: `check` は小さなプロジェクトで 0.01〜0.03 秒（7 回の中央値。ディスクのキャッシュの有無で変わらない）。11 ツールの `tools/list` の応答は 14,410 bytes（全クライアントの文脈に入る量なので 16,384 bytes 以下を保つ）。

**非目標**: Streamable HTTP・認可、prompts・completions・logging・サンプリング・elicitation・roots、サーバーからの通知（`listChanged` は常に `false`）、ファイルを書く・fix を適用する・`build`／`run`／`test` を実行するツール、複数の根、MCP の 2025-11-25 以降の版（D2）、VS Code 拡張からの自動登録。

## 着手条件と停止条件

### 着手条件

- GUIDE §2.3 の基準（`cargo build --release --locked`・`cargo test --locked`）が通る。件数を控える（手順 1）。
- 読む: GUIDE-LITE、D-51・D-52・D-55、`docs/stability.md`、付録 A。承認は不要（新しい crate・`unsafe`・予約語・既定の WASM の import がない）。
- Phase 2 の手順 N に着手する前に、そのツールの依存チケットが `_features/README.md` で `done` か、`git log` で main にあることを確かめる。`done` でなければ**その手順を飛ばす**（停止ではない）。

### 停止条件

止めて、状況と選択肢を報告する（GUIDE §13）。

1. **`serde_json` のキー順**: 結果のキーを辞書順でなく固定の順にしたくなった、または `preserve_order` が要りそうになった。`indexmap` が増えるので新しい crate になる。選択肢: A（採用）辞書順のまま。B 手書きのシリアライザ（`diagnostic::json_string` 流）で組み立てる（全ツールの出力を書き直す）。
2. **stdout の汚染**: `Project::load`・`analyze_all` の実行中に標準出力へ書く経路が見つかった（`grep -rn "println!\|print!(" src/*.rs` が `main.rs`・`repl.rs` 以外に出す）。
3. **stack**: ワーカー（8 MiB）で解析すると、メインスレッドで通る入力が stack overflow する。選択肢: A 上限や stack を変えず、解析を主スレッドで行い、`ping` だけをリーダースレッドが書く（出力を `Mutex` で共有する）構造に変える。B `WORKER_STACK_BYTES` を上げる（`parallel.rs` の根拠のコメントを変える。人間の承認が要る）。
4. **G27 の `KbView` が std の KB の作成でキャッシュのディレクトリへ書く**（メモリ上だけの経路がない）。選択肢: A G27 に「書かない」モードを足してもらう。B std の KB を使わずに出荷する（`tz_symbol` が std の ID を引けないと文書に書く）。**ディスクへ書く実装にしない**（D6）。
5. **G28 の照会 API** が、`KbData`（メモリ上）でなく、ディスクの KB のパスしか受け取れない。選択肢: A G28 に `KbData` を受ける形を足してもらう。B MCP が KB を `target/tz/kb` へ書く（読み取り専用の約束を外す。人間の承認が要る）。
6. 付録 B・C の JSON が、公式の `schema.json`（付録 A）で検証できない、または公式 SDK のクライアントが結果を拒否する（付録 F）。
7. `tools/list` の応答が 16,384 bytes を超えた。説明を削る前に報告する（クライアントの文脈の費用に直結する）。
8. 取り消しで、ワーカーが長く塞がる実例（解析に数十秒かかるプロジェクト）が見つかった。選択肢: 報告のうえ、Phase 2 で `tz_check` のワーカーを 2 本にするか、解析に取り消しの点を足す（本チケットの範囲外）。
9. `unsafe`・新しい crate・環境変数・子プロセスが必要に見えた。既存のテストの期待値を変えないと通らない。同じ失敗を直す試みが 2 回続けて失敗した。

## 現状（HEAD `a2313d19`）

作業ツリーは `9dd0fbbc`（main）。`git diff --stat 9dd0fbbc a2313d19` は `docs/benchmarks.md` の 41 行だけで、ソースは同じ。以下は `target/release/tsuzuri` で実行した結果（一時ディレクトリは `/tmp/tz-G29-*` の例）。

1. **`mcp` はない。**

```sh
cd /tmp/tz-G29-try && tsuzuri mcp; echo "exit=$?"
mcp:1:1: error[E2001]: cannot inspect source 'mcp': No such file or directory (os error 2)
exit=1
tsuzuri mcp --root .; echo "exit=$?"
<command line>:1:1: error[E2000]: unknown option '--root'; use --help
exit=2
```

2. **診断の JSON は 1 行 5 キー**（標準エラー）。`Main.tz` が付録 G の `g29-bad`（`consume text` を 2 回呼ぶ 9 行）のとき:

```sh
tsuzuri check Main.tz --json
{"severity":"error","code":"E1012","message":"use of moved or partially moved value 'text'","path":"./Main.tz","span":{"start":143,"end":147,"line":5,"column":26,"end_line":5,"end_column":30}}
```

`path` は渡した入力に `./` がついた形で、`start`・`end` は UTF-8 の byte offset、`column` は文字数。警告だけのプログラム（付録 G の `g29-warn`）は終了コード 0 で、`W1001` の同じ形の行が標準エラーに出る。`Main.tz` のないディレクトリを `tsuzuri check .` すると `./Main.tz:1:1: error[E2001]: cannot inspect source './Main.tz': No such file or directory (os error 2)`、`.tz` のファイルを渡すと通る（ライブラリ）。

3. **LSP は別の framing**（`Content-Length` の見出し）で、キーは辞書順。MCP の 1 行をそのまま入れると:

```sh
printf '{"jsonrpc":"2.0","id":1,"method":"ping"}\n' | tsuzuri lsp
Content-Length: 95

{"error":{"code":-32700,"message":"invalid or oversized LSP header"},"id":null,"jsonrpc":"2.0"}
```

`Cargo.lock` に `indexmap` はなく（`grep -c indexmap Cargo.lock` が 0）、`serde_json` は `preserve_order` なしなので、`Value` のキーは辞書順で出る。`serde_json` の既定の再帰の深さは 128 で、`src/lsp.rs` のテスト `framing_roundtrips_and_rejects_invalid_inputs` が 129 の入れ子を拒否している。`src/lsp.rs` の解析は主スレッドで行う。

4. **解析の入口**: `Project::load(&Path)` は、ディレクトリなら `Main.tz` を要求し、**std のソースも `Project::sources` に入れる**（`origin == ModuleOrigin::Std`。ユーザーのソースは `User`）。`Project::analyze_all()` は `Parallelism::library()`（既定 1 スレッド）でディスクのキャッシュを使わない。**7 回の中央値**（付録 G）は、`examples/point` が cache あり 0.03 s・なし 0.03 s、`tests/fixtures/llvm_handoff`（12 モジュール）が 0.02 s・0.01 s。したがって MCP はディスクのキャッシュを使わない（D6）。
5. **ソースの探索**: `collect_sources` はシンボリックリンクのソース・ディレクトリを `E1011` で拒否し、ディレクトリ 1,024・深さ 16・ファイル 4,096 を超えると `E1017`。コンパイラのライブラリ側（`src/*.rs`。`main.rs`・`repl.rs` を除く）に、テスト以外の `println!`・`print!` はない。
6. **ワーカーの stack**: `src/parallel.rs` の `const WORKER_STACK_BYTES: usize = 8 << 20;`（コメント: メインスレッドが受理するプログラムでワーカーが溢れてはならない）。可視性は private。
7. **テストの手本**: `tests/lsp.rs` の `session` は一時の root（`std::env::temp_dir()` に一意の名前・`canonicalize`）を作り、`serve(Cursor::new(入力), &mut 出力, ..)` を同じプロセスで呼ぶ。`tests/api_diff.rs` は `Command::new(env!("CARGO_BIN_EXE_tsuzuri"))` で子プロセスを起こす。
8. **MCP の仕様**（2026-10-18 に取得。付録 A）: 実装する版は **2025-06-18**。同日の時点で新しい版が 2 つある。2025-11-25 は追加だけ、2026-07-28 は `initialize` を廃止した stateless の版で、その stdio の後方互換の規則は、クライアントが先に `server/discover` を送り、**どんなエラーでも**（`-32601` を含む）`initialize` に戻る、である（D2）。

## 変えてはいけない意味

- `check`・`build`・`run`・`test`・`doc`・`fmt`・`lsp` ほか既存のサブコマンドの標準出力・標準エラー・終了コード・生成物は 1 byte も変わらない（`tsuzuri mcp` を使わない限り）。
- 診断のコードと意味、`--json` の 5 キー。`E2000` の意味（引数の誤り）は変えない。`E2010` は新しい。
- MCP は読み取り専用: プロジェクトのファイル・KB・キャッシュのディレクトリ・ホームのどこにも書かない。子プロセス・ネットワーク・環境変数の読み取り（`src/mcp/` の中。`TSUZURI_*` はコンパイラ本体が従来どおり読む）をしない。
- stdout に出るのは MCP のメッセージ（1 行 1 JSON）だけ。
- 同じプロジェクト・同じ引数なら、`tools/call` の結果は byte 単位で同じ（時刻・乱数・絶対パス・スレッド数を入れない）。

## Phase 分割

| Phase | 内容 | 規模 | 前提 | 単独で出荷できる理由 |
| --- | --- | --- | --- | --- |
| 1 | `tsuzuri mcp`、プロトコル、版の交渉、ツールの表と検証、`tz_check`、パスの閉じ込め、取り消し・並行、上限、文書の骨格 | M | なし | 診断だけを返す読み取り専用のサーバーとして完結する。ほかのツールは `tools/list` に出ない |
| 2 | 残りの 10 ツールの接続（依存が `done` のものから 1 つずつ）、結果のキャッシュ、KB の読み込み | M | ツールごとの依存（上の表）。ほかのツールとは独立 | 各ツールは、自分の依存が `done` のときだけ `tools/list` に出る |
| 3 | `resources/list`・`resources/read`・`resources/templates/list` | S | Phase 1。資源ごとに G21（ガイド）・G23（explain）・G27（brief） | それまで `resources/*` は `-32601`。`initialize` は `resources` を宣言しない |
| 4 | 敵対的な入力の表のテスト、公式 SDK での確認、文書の仕上げ | S | Phase 1〜3 | テストと文書だけで、挙動を変えない |

## 仕様

### CLI

```text
tsuzuri mcp [--root DIR] [--kb-dir DIR]
```

| オプション | 意味 | 既定 | 誤り |
| --- | --- | --- | --- |
| `--root DIR` | プロジェクトの根。ツールのパスはこの下だけ（「パスの閉じ込め」） | カレントディレクトリ | 値なし・2 回: `E2000`。存在しない・ディレクトリでない: `E2010` |
| `--kb-dir DIR` | KB の置き場を読む場所（Phase 2。読むだけ。`TSUZURI_KB_DIR` は読まない。D-52） | `<根>/target/tz/kb` | 値なし・2 回: `E2000` |

- ほかの引数・位置引数・未知のオプションは `E2000`、終了コード 2。メッセージは `mcp takes no paths; supported options are --root and --kb-dir`、重複は `{option} specified more than once`、値なしは `{option} needs a directory`。`parse_repl_arguments` と同じ形（`seen` の集合）。`--json`・`-O0`〜`-O3`・`--no-cache`・`--target` を含め、`--root` と `--kb-dir` 以外のオプションは未知のオプションである。
- `TSUZURI_THREADS` が空でも未設定でもなく、1〜32 の整数（前後の空白は許す）でないとき（`0`・`33`・`abc` のとき）は、`lsp` と同じく、読み込みの前に `E2000`（終了コード 2。`invalid_thread_setting(false)`）。`mcp` の解析は `lsp` と同じ 1 スレッド（`TSUZURI_THREADS` が未設定のとき）。
- `--root` は `cache::real_path` で正規化する（Windows の `\\?\` を出さない）。正規化できない・ディレクトリでなければ `E2010`: `project root '{path}' is not a directory`、終了コード 1。
- 終了コード: 0 = 入力（stdin）が閉じ、キューの仕事の応答を出し終えた。1 = `E2010`（根が使えない、`cannot write to the client: {error}`、`cannot read the client's input: {error}`、`cannot start the worker thread: {error}`）。2 = `E2000`。
- 診断の表示は `print_diagnostic(&Diagnostic::new(code, message, Span::default()), Path::new("<command line>"), "", false)`（`<command line>:1:1: error[E2010]: ...`）。標準エラーにだけ出し、標準出力には出さない。

### トランスポートと JSON-RPC

1 メッセージは 1 行の JSON（UTF-8）。行は `\n` で終わり、直前の `\r` は取り除く。出力は `serde_json::to_string`（改行を含まない）に `\n` を付ける。**出力のキーは辞書順**（`serde_json` の既定。付録 C の例はその形）。

入力 1 行の扱い（上から順に調べ、最初に当てはまった行）:

| 入力 | 応答（`id` は書かれた値。取れなければ `null`） |
| --- | --- |
| 空の行（`\r` を除いて 0 byte） | なし |
| 4 MiB（4,194,304 bytes）を超える行 | `-32600` `invalid request: message exceeds 4194304 bytes`。行の残りは次の `\n` まで読み捨てる |
| UTF-8 でない | `-32700` `parse error: the message is not valid UTF-8` |
| JSON でない、または入れ子が 128 を超える | `-32700` `parse error: {serde_json のメッセージ}`（文言は版で変わりうる。テストは接頭辞だけを見る） |
| JSON の配列 | `-32600` `invalid request: batch requests are not supported`（D3） |
| object でない値 | `-32600` `invalid request: expected a JSON object` |
| `jsonrpc` が `"2.0"` でない | `-32600` `invalid request: jsonrpc must be "2.0"` |
| `id` があり、文字列でも整数（`i64`・`u64`）でもない（`null`・小数・真偽・配列・object） | `-32600` `invalid request: id must be a string or an integer`（`id` は `null`） |
| `method` がなく、`result` も `error` もない | `-32600` `invalid request: method must be a string` |
| `method` がなく、`id` があり、`result` か `error` がある | クライアントの応答。何もしない（サーバーは要求を送らない） |
| `method` が文字列でない | `-32600` `invalid request: method must be a string` |
| `method` あり・`id` あり | 要求 |
| `method` あり・`id` なし | 通知。**どんな場合も応答しない** |
| 要求の `params` があり object でない | `-32602` `invalid params: expected an object`（通知なら無視） |

応答の組み立て: 成功は `{"jsonrpc":"2.0","id":<id>,"result":<値>}`、失敗は `{"jsonrpc":"2.0","id":<id>,"error":{"code":<整数>,"message":<文字列>}}`（`data` は `-32002` だけ）。`id` は受け取った値をそのまま返す。**ツールの `isError` と JSON-RPC のエラーを混ぜない**（「tools/call」）。

エラーコード: `-32700`・`-32600`・`-32601`・`-32602`・`-32603`（JSON-RPC）、`-32002`（資源が見つからない。2025-06-18 の規則。Phase 3）、`-32000`（`too many pending requests (limit 64)`。実装定義）。

### ライフサイクルと版の交渉

状態は `Fresh`（`initialize` の前）と `Negotiated`（応答した後）の 2 つ。`notifications/initialized` は状態を変えない（受け取る前でも `tools/*` を受理する。pipeline で送るクライアントがある）。

| 状態 | 要求・通知 | 動作 |
| --- | --- | --- |
| どちらでも | `ping` | `{}`（ツールの実行を待たない） |
| どちらでも | 未知のメソッドの要求（`server/discover`・`prompts/list`・`logging/setLevel`・`completion/complete`・`resources/subscribe`・Phase 3 より前の `resources/*` を含む） | `-32601` `method not found: {method}`。**状態の検査より先**（2026-07-28 のクライアントの探り `server/discover` に、初期化の前でも `-32601` を返し、`initialize` へ戻らせる） |
| どちらでも | 未知の通知 | 無視 |
| `Fresh` | `initialize` | 版を交渉して応答し、`Negotiated` に移る |
| `Fresh` | `tools/list`・`tools/call`・`resources/*` | `-32600` `server not initialized: send initialize first` |
| `Fresh` | `notifications/initialized`・`notifications/cancelled` | 無視 |
| `Negotiated` | `initialize` | `-32600` `server is already initialized` |
| `Negotiated` | `tools/list`・`tools/call`・`resources/*` | 通常の処理 |
| `Negotiated` | `notifications/cancelled` | 「取り消し」 |

`initialize` の `params`: object で `protocolVersion` が文字列であること。ほかのキー（`capabilities`・`clientInfo`）は検査しない（欠けても受理する。仕様は必須だが、拒否しても得るものがない。D2）。欠落・文字列でない場合は `-32602` `invalid params: protocolVersion must be a string`。

| 要求の `protocolVersion` | 応答の `protocolVersion` |
| --- | --- |
| `2025-06-18`・`2025-03-26`・`2024-11-05` | 同じ値 |
| それ以外の文字列（`2025-11-25`・`2026-07-28`・`1.0.0`・空） | `2025-06-18` |

応答（付録 C に全文）: `{"protocolVersion","capabilities":{"tools":{"listChanged":false}},"serverInfo":{"name":"tsuzuri","title":"Tsuzuri compiler","version":"<CARGO_PKG_VERSION>"},"instructions":"<347 文字の固定の文>"}`。`resources` の宣言は Phase 3 から（`{"resources":{}}`）。`logging`・`prompts`・`completions` は宣言しない。`instructions` の本文は付録 C。

**版による出し分け**（交渉した版で決まる。`Features` の表 1 か所）:

| 項目 | 2025-06-18 | 2025-03-26 | 2024-11-05 |
| --- | --- | --- | --- |
| `serverInfo.title`、ツール・資源・テンプレートの `title` | あり | なし | なし |
| ツールの `outputSchema`、結果の `structuredContent` | あり | なし | なし |
| ツールの `annotations` | あり | あり | なし |

### メソッド

| メソッド | 種類 | 処理する場所 | 結果 |
| --- | --- | --- | --- |
| `initialize` | 要求 | 主スレッド | 上の表 |
| `notifications/initialized`・`notifications/cancelled` | 通知 | 主スレッド | なし |
| `ping` | 要求 | 主スレッド | `{}` |
| `tools/list` | 要求 | 主スレッド | `{"tools":[...]}`。有効なツールだけ、`tools.json` の順。`params.cursor` があれば（どんな値でも）`-32602` `invalid params: invalid cursor`（`nextCursor` を出さないので有効なカーソルはない）。ページ分割はしない |
| `tools/call` | 要求 | **ワーカー（FIFO）** | 「tools/call」 |
| `resources/list`・`resources/templates/list` | 要求 | 主スレッド（Phase 3） | 付録 C |
| `resources/read` | 要求 | **ワーカー（FIFO）**（Phase 3） | 「リソース」 |

### tools/call

`params` は `{"name": 文字列, "arguments": object（省略可）}`。ワーカーが次の順で処理する。

| # | 条件 | 結果 |
| --- | --- | --- |
| 1 | `params` が object でない / `name` が文字列でない / `arguments` があり object でない | JSON-RPC エラー `-32602` `invalid params: expected an object` / `invalid params: name must be a string` / `invalid params: arguments must be an object` |
| 2 | 名前のツールがない、または `Capabilities` で無効 | `-32602` `unknown tool: {name}` |
| 3 | `arguments`（省略は `{}`）が `inputSchema` に合わない | 結果 `isError: true`、本文 `invalid arguments: {pointer}: {reason}`（検証器。「検証器」） |
| 4 | バックエンドが `Err(ToolError(message))` | 結果 `isError: true`、本文は `message` |
| 5 | バックエンドが panic | 結果 `isError: true`、本文 `internal error: tool '{name}' panicked; please report this`（サーバーは続く） |
| 6 | バックエンドが `Ok(ToolOutput)` | 下の「結果の組み立て」 |
| 7 | 実行中に取り消された | 応答しない |

**結果の組み立て**（`shape`）: `text` は先頭 200,000 文字までを残し、超えたら `\n… truncated: {N} more characters`（`N` は削った文字数）を付け、`truncated` を `true` にする。`structured` は object で、`truncated` がなければ `false` を足し、切り詰めたなら `true` にする。`structured` を `serde_json::to_string` した長さが 400,000 bytes を超えたら、結果を `isError: true`・本文 `result too large ({n} bytes); narrow the request` に置き換える。交渉した版が 2025-06-18 なら `{"content":[{"type":"text","text":<text>}],"structuredContent":<structured>}`、それより前なら `{"content":[{"type":"text","text":<text>}]}`。

**検証器**（`schema::validate`）が使うのは、`type`（文字列か、`"object"`・`"array"`・`"string"`・`"integer"`・`"number"`・`"boolean"`・`"null"` の配列）、`properties`、`required`、`additionalProperties`（`false` だけ）、`items`、`enum`、`const`、`minimum`、`maximum`、`minLength`、`maxLength`、`minItems`、`maxItems`、注釈（`description`・`default`・`title`）だけ。`integer` は `i64`/`u64` で、`1.0` は整数でない。最初の違反を、次の順で調べて返す: 型、`enum`/`const`、数、文字列（文字数）、配列（`items` は添字 0 から）、object（`required` の順 → 引数のキーの辞書順に、`properties` にないキーは `additionalProperties: false` なら違反、あれば値）。メッセージは `{pointer}: {reason}`（`pointer` は `""` が根、子は `/key`・`/0`）:

| 違反 | `reason` |
| --- | --- |
| 型 | `must be a string`・`an integer`・`a number`・`a boolean`・`an object`・`an array`・`null`（複数なら ` or ` でつなぐ） |
| `enum` | `must be one of "a", "b"`（`enum` の順。文字列は引用符つき、数は 10 進） |
| `minimum`／`maximum` | `must be at least {n}`／`must be at most {n}` |
| `minLength`／`maxLength` | `must have at least {n} characters`／`must have at most {n} characters` |
| `minItems`／`maxItems` | `must have at least {n} items`／`must have at most {n} items` |
| 必須のキーがない | `{親}/{key}: is required` |
| 未知のキー | `{親}/{key}: unknown property` |

例: `{"path": 5}` → `invalid arguments: /path: must be a string`。`{"budget": 10}` → `invalid arguments: /budget: must be at least 200`。

### ツール

付録 B が全 11 ツールの定義（`inputSchema`・`outputSchema`・`annotations`）。すべて `readOnlyHint: true`・`destructiveHint: false`・`idempotentHint: true`・`openWorldHint: false`。`structured` は `outputSchema` に合うこと（テストが検証する）。`text` は、対応する CLI の text 出力。

| ツール | `Capabilities` | 必須の引数 | `text` の出所 | `structured` の出所 | 主な `isError`（メッセージ） |
| --- | --- | --- | --- | --- | --- |
| `tz_check` | `check` | なし | 診断の描画（「`tz_check`」） | 診断の JSON と件数 | パスの誤り（「パスの閉じ込め」） |
| `tz_guide` | `guide` | なし | G21 の `guide::render` のページ本文 | `{topic,text,truncated}` | `unknown guide topic '{t}'; call tz_guide with topic "list" to see the topics` |
| `tz_explain` | `explain` | `query` | G23 の `explain::render_kind`／`render_code`（`json = false`）の text | 同じ関数の `json = true` の 1 行から `code`・`kind`（kind を引いたときだけ）・`title`・`since`（同）を写し、`text` と `truncated` を足す | `unknown diagnostic '{q}'; use a code such as E1012 or a kind such as use_after_move`（`resolve` の候補があれば `; did you mean: A, B` を足す） |
| `tz_symbol` | `kb` | `id` | G28 の `show` の text | `{symbol,facts,callees,tests,callers}`（KB の行は D-52 のまま） | `unknown id '{id}'; use tz_search to find ids`（近い ID が 5 件まであれば `; did you mean: A, B`）、`ambiguous id '{id}'; candidates: A, B, C`（10 件まで） |
| `tz_brief` | `kb` | なし | G28 の `brief` の text（`budget` 文字まで） | `{id,level,text}` | 同上の ID の誤り |
| `tz_search` | `kb` | `query` | G28 の `search` の text | `{query,matches[{id,kind,signature}]}` | なし（0 件は正常） |
| `tz_callers` | `kb` | `id` | G28 の `callers`／`uses` の text | `{id,relation,depth,callers[graph の行 + depth]}` | 同上の ID の誤り |
| `tz_find` | `find` | `query` | G25 の `find` の text | `{query,candidates[{name,signature,how}]}`（D-51 の `Candidate`） | `cannot parse the type query: {G25 の診断のメッセージ}` |
| `tz_impact` | `impact` | `old` | G28 の `impact` の text | `{old,new,records[]}`（`records` は `kb impact --json` の各行） | パスの誤り、`'{p}' is neither a knowledge base nor a project directory` |
| `tz_fix_plan` | `fix` | なし | G24 の `--dry-run` の標準出力 | `{applicability,rounds,stop,fixes,dropped,diff,remaining_errors,remaining_warnings}`（G24 の `--json` の `fix`・`file`・`summary` の行から） | パスの誤り |
| `tz_diagram` | `diagram` | `kind` | G30 の text（図そのもの。`ownership`・`layout` は Markdown） | `{kind,format,text,nodes,truncated}`（`truncated` は `DiagramOutput.folded`） | `diagram kind '{k}' needs an id`、`format 'dot' is not available for kind '{k}'`、ID の誤り（G30 の `diagram_unknown_id`・`diagram_wrong_id_kind` のメッセージ） |

KB の 4 ツールが KB 不完全（G27 の `kb_incomplete`: プロジェクトが型検査に通らない）を受けたら、`isError`・本文 `the project does not type-check, so there are no facts yet; call tz_check and fix the errors first`。KB の行のキーは D-52 で固定なので、`symbol`・`callers` の行を MCP 側で作り替えない。

### `tz_check`

入力: `path`（省略は根。ディレクトリは `Main.tz` を要求する。ライブラリは `.tz` ファイルを渡す）、`warnings`（既定 `true`）、`implicit_copy`（既定 `false`）。

1. `resolve_path` で入力のパスを決める（「パスの閉じ込め」）。
2. `Project::load(&input)`。`Err(SourceError{path, diagnostic})` なら、診断 1 件（`path` は `relative_text(root, &error.path)`、ソースは空）の結果にする（`ok: false`、`files: 0`）。
3. `project.analyze_all()`。`Ok(module)` なら、診断は `module.warnings`（`implicit_copy` が真なら `copies::warnings(&module)` を足す）を `DiagnosticSet::from_diagnostics(.., 0)` に通したもの、`ok = true`。`Err(set)` なら、診断は `set.diagnostics`、`ok = false`。件数の `omitted` は `set.omitted`。
4. `warnings: false` なら警告を除く（`omitted` は変えない）。`errors`・`warnings` は返す診断の中の件数、`files` は `project.sources` のうち `origin == ModuleOrigin::User` の数。
5. 診断ごとに、`project.source_for(&d)` のソースで `d.json_with(severity, 表示パス, &source.text, &lookup)`（G22 の前は `json_with_severity(severity, 表示パス, &source.text)`）を作り、`serde_json::from_str` で `Value` にする（`kind`・`related`・`help`・`fixes` のキーは自動で入る）。`lookup` は `|i| project.sources.get(i).map(|s| (paths[i].as_str(), s.text.as_str()))`（`paths` は全ソースの表示パス。別ファイルの `related`・fix を出すため）。表示パス: `source.path` が根の下なら、根からの相対を `/` 区切りにしたもの。そうでない（依存・std）なら `source.relative_path` を `/` 区切りにしたもの。
6. `text`: 各診断を `d.render_with(severity, 表示パス, &source.text, &lookup)`（G22 の前は `render_with_severity`）で描き、空行 1 行で区切る。`omitted > 0` なら空行と `omission_note` を足す（`3 more errors not shown`）。診断がなければ要約だけ。最後に空行と要約 `{OK|FAILED}: {e} error(s), {w} warning(s) in {f} file(s)`（1 のとき単数）。
7. 診断・`path`・メッセージに根の絶対パスが入らない: `Project::load` が返すメッセージ（`E2001`・`E1011`）の絶対パスは `scrub(message, root)`（`root` の文字列を探し、`{root}/` と `{root}\` を空に、残りの `{root}` を `.` に置き換える）で消す。OS のエラー文（`No such file or directory (os error 2)`）は OS のまま。

`structured` は `{ok,files,errors,warnings,omitted,diagnostics,truncated}`。診断の順序はコンパイラのもの（`DiagnosticSet` の整列）で、最大 50 件（`MAX_REPORTED_ERRORS`）。

### パスの閉じ込め

`resolve_path(root, arg)`: `root` は起動時に `cache::real_path` で正規化済み。`arg` が空なら `.`。次の表を上から調べ、最初に当てはまった行で `ToolError` を返す。

| 条件 | メッセージ |
| --- | --- |
| 4,096 文字を超える | `path is too long` |
| NUL・`\`・先頭の `/`・`X:`（ASCII の英字 + `:`）で始まる | `path must be relative to the project root: '{arg}'` |
| `..` の区間がある（`/` で区切って調べる。`.` と空の区間は読み飛ばす） | `path must not contain '..': '{arg}'` |
| 根から各区間までの途中に `symlink_metadata` で存在しないものがある | `no such file or directory: '{arg}'` |
| 途中のどれかがシンボリックリンク | `path goes through a symbolic link: '{arg}'` |
| 最終のパスの `real_path` が `root` の下でない（念のための検査。Windows のジャンクション） | `path escapes the project root: '{arg}'` |
| ディレクトリでも `.tz`・`.tt`・`.tc` のファイルでもない | `path is neither a directory nor a .tz, .tt or .tc file: '{arg}'` |

通れば、`root` に区間をつないだ**正規化していない**パスを返す（`Project::load` がシンボリックリンクを再度拒否する）。`tz_impact` の `old`・`new` も同じ関数（ファイルでなくディレクトリを要求）。メッセージの `{arg}` は引数のまま（絶対パスを足さない）。

### 取り消しと並行

スレッドは 3 つ: リーダー（stdin を 1 行ずつ読み、`sync_channel(8)` で主スレッドへ）、主スレッド（唯一の書き手。状態・キュー・即答するメソッド）、ワーカー（8 MiB。`tools/call` と `resources/read` を FIFO で 1 本ずつ）。

- 主スレッドは `tools/call`・`resources/read` を `queue`（最大 64 件。実行中を含む）へ積む。65 件目は `-32000` `too many pending requests (limit 64)`。実行中・待ちの要求と同じ `id` の要求は `-32600` `invalid request: request id is already in use`。
- `notifications/cancelled {requestId}`: `requestId` が実行中の `id`（JSON として等しい。`"1"` と `1` は別）なら `Cancel` を立て、ワーカーが終わったら応答を捨てる。待っている要求なら `queue` から外して応答しない。ほかは無視（未知・終了済み・`initialize`）。
- 取り消しは**結果を捨てるだけ**で、解析は止まらない（コンパイラに中断の点がない。LSP も同じ）。次のジョブは実行中のジョブが終わってから始まる（停止条件 8）。`ProjectBackend` は `ToolRequest.cancel` を読まない（`run_job` が `Backend::call` の前後で見る）。
- EOF: リーダーが止まっても、`queue` と実行中のジョブの応答を出し終えてから終了する（パイプで要求を流し込む使い方が確定的になる）。入力の読み取りの失敗は `E2010`。

### リソース（Phase 3）

`initialize` が `resources: {}` を宣言する。URI は次の 5 種類。**有効なものだけ**（`Capabilities`）を一覧・テンプレートに出す。

| URI | `mimeType` | 本文の出所 | 一覧 |
| --- | --- | --- | --- |
| `tz://guide/L0` | `text/markdown` | `tz_guide`（トピックなし）と同じ | `resources/list`（`guide`） |
| `tz://guide/L1` | `text/markdown` | `tz_guide` の `topic: "language"` | `resources/list`（`guide`） |
| `tz://kb/brief/L0` | `text/markdown` | `tz_brief`（`id` なし）と同じ | `resources/list`（`kb`） |
| `tz://kb/brief/{id}` | `text/markdown` | `tz_brief` の `id` | `resources/templates/list`（`kb`）。`{id}` は KB の ID の**パーセントエンコード**（unreserved `A-Za-z0-9-._~` 以外のバイトを `%XX`。大文字の 16 進。読むときは `%XX` を復号するだけ） |
| `tz://explain/{kind}` | `text/markdown` | `tz_explain` の `query`（コードか kind） | `resources/templates/list`（`explain`） |

`resources/read` の `params.uri` が文字列でない: `-32602`。上の形でない、無効なツール、ID・kind がない: `-32002` `resource not found: {uri}`（`data: {"uri": uri}`）。成功は `{"contents":[{"uri","mimeType","text"}]}`（本文は 200,000 文字まで。超えたら `shape` と同じ印を付けて切る）。`resources/list`・`templates/list` の `cursor` はツールと同じ扱い。項目は `name`・`title`・`description`・`mimeType`（版が古いと `title` なし）。付録 C に全文。

### 上限（まとめ）

| 対象 | 値 |
| --- | --- |
| 入力 1 行 | 4,194,304 bytes |
| JSON の入れ子 | 128（`serde_json` の既定） |
| 待ち行列 | 64 件、リーダーの channel は 8 |
| `content[0].text` | 200,000 文字 |
| `structuredContent` | 400,000 bytes |
| `tools/list` の応答 | 16,384 bytes 以下（テストで強制。付録 B の 11 ツールで 14,410） |
| `tz_check` の診断 | 50 件（コンパイラの上限）。省略は `omitted` |
| ツールの実行時間 | 上限なし（コンパイラが中断できない）。クライアントのタイムアウトと取り消しで止める |

各ツールの件数の上限（`tz_symbol` の `callees` 50・`tests` 20、`tz_callers` の `limit` 既定 50・最大 200・深さ 8、結果のキャッシュ 8 件）は、その手順にある。解析の資源の上限と `E1017` は、コンパイラのものを変えない。

### 診断（このチケットが出すもの）

| コード | kind | いつ | メッセージ | 終了コード |
| --- | --- | --- | --- | --- |
| `E2000` | `mcp_bad_arguments` | `mcp` の引数の誤り | 「CLI」の表 | 2 |
| `E2010` | `mcp_root_unusable`（根が使えない）・`mcp_io_failed`（stdin・stdout の入出力の失敗）・`mcp_worker_failed`（ワーカーを作れない） | 致命的な誤り | 「CLI」の表 | 1 |

`E2010` は D-55 の割り当て（G29。「MCP の致命的な誤り」）。kind は D-51 の 3 通り: G23 があれば S1 の手順で登録して `of` で出し、G22 だけなら `with_kind`、どちらもなければ付けない。MCP のメッセージの誤り（`-32xxx`）は診断コードではない。診断は標準エラーにだけ出す。

### 決定性

同じプロジェクト・同じ引数・同じコンパイラなら、`tools/call` の結果（`text`・`structuredContent`）は byte 単位で同じ。ツールの順序は `tools.json` の順、診断の順序はコンパイラの整列、JSON のキーは辞書順、パスは根からの相対で `/` 区切り。時刻・絶対パス・スレッド数・乱数を入れない。複数のメッセージの**書かれる順序**は、即答するメソッドとワーカーの完了の順で変わりうる（`id` で突き合わせる）。`tools/call` どうしは FIFO。

### 互換性

- **MCP の版**: 2025-06-18 を実装し、2025-03-26・2024-11-05 を出し分けで受ける（D2）。2025-03-26 の「バッチの受信」には対応しない（D3。既知の逸脱）。modern（2026-07-28）だけのクライアントは接続できない（`initialize` に戻るクライアントなら動く）。
- `docs/stability.md`: 「安定として保つもの」に「`tsuzuri mcp` のツール名・引数のキー・`structuredContent` のキー（キーが増えることはあります）」、「保たないもの」に「MCP の `content` の本文・ツールの説明・`instructions`」を足す（手順 10）。`tools.json` のキーの削除・改名は互換のない変更。
- edition・IR・ABI・WASM の import・既存の診断コード: 影響なし。`tsuzuri mcp` を使わない限り何も変わらない。

## 設計

### 新規・変更するファイル

| ファイル | 責務 |
| --- | --- |
| `src/mcp/mod.rs`（新規） | 定数（`SUPPORTED_VERSIONS`・`MAX_LINE_BYTES`・`MAX_PENDING`・`MAX_TEXT_CHARS`・`MAX_STRUCTURED_BYTES`）、`Options`、`Fatal`、`serve`、`serve_with` |
| `src/mcp/rpc.rs`（新規） | `read_line`・`Line`、`parse`・`Incoming`、`result_message`・`error_message`、`write_line`、エラーコードの定数 |
| `src/mcp/schema.rs`（新規） | `validate`、`SUPPORTED_KEYWORDS`、`keywords_outside_the_subset` |
| `src/mcp/tools.json`（新規） | 付録 B（11 ツール） |
| `src/mcp/tools.rs`（新規） | `Registry`（`include_str!` を解析）、`list`（版・`Capabilities`）、`shape`、`cap_text`、`Features` |
| `src/mcp/session.rs`（新規） | 主ループ、状態、ワーカー、取り消し、`tools/call` の処理（`run_tool`） |
| `src/mcp/backend.rs`（新規） | `Backend`・`Capabilities`・`ToolRequest`・`ToolOutput`・`ToolError`・`Cancel`・`ProjectBackend` |
| `src/mcp/path.rs`（新規） | `resolve_path`・`relative_text`・`scrub` |
| `src/mcp/check.rs`（新規） | `tz_check` |
| `src/mcp/knowledge.rs`・`cache.rs`（Phase 2） | KB の取得と KB 系のツール、結果のキャッシュ |
| `src/mcp/resources.rs`（Phase 3） | URI の分解・一覧・読み取り |

`src/lib.rs`・`src/main.rs`・`src/parallel.rs` の変更は「変更の一覧（Phase 1）」。Rust の骨格（名前と型を変えない）は付録 E。要点:

```rust
pub trait Backend: Send + Sync {
    fn capabilities(&self) -> Capabilities;
    fn call(&self, request: &ToolRequest<'_>) -> Result<ToolOutput, ToolError>;
    fn read_resource(&self, uri: &str, cancel: &Cancel) -> Result<Option<String>, ToolError> { Ok(None) } // Phase 3
}
```

### 主ループ（`session::run`）

```text
入力: backend（Arc）、input（Read + Send + 'static）、output（Write。主スレッドだけが書く）
1. (events_tx, events_rx) = sync_channel::<Event>(8)。リーダー: BufReader(input) から read_line を繰り返し、
   Bytes・TooLong・Eof・失敗を events_tx へ送る（送れなければ終わる）。
2. (jobs_tx, jobs_rx) = channel::<Job>()。ワーカー: Builder::new().name("tsuzuri-mcp-worker")
   .stack_size(WORKER_STACK_BYTES).spawn: jobs_rx の各 Job について catch_unwind(run_job) を実行し、
   events_tx.clone() へ Event::Done{id, outcome} を送る。spawn できなければ Err(Fatal)。
3. 状態: phase=Fresh、version=2025-06-18、queue: VecDeque<Job>（待ち）、running: Option<(Value, Cancel)>、
   eof=false。
4. loop:
     event = events_rx.recv()（Err は eof 扱い）
     Line(Bytes) → handle_line（分類 → 即答するものは output に書く、tools/call・resources/read は検査 3 点
                    （重複 id・MAX_PENDING・Fresh）の後 queue へ）
     Line(TooLong) → -32600 を書く
     Line(Eof) → eof = true
     ReadFailed(m) → return Err(Fatal("cannot read the client's input: {m}"))
     Done{id, outcome} → running = None。取り消されていなければ応答を書く
     running が None で queue があれば、先頭を running にして jobs_tx へ送る
     eof && running == None && queue が空 → return Ok(())
   書き込みの失敗は return Err(Fatal("cannot write to the client: {e}"))。
```

`run_job`（ワーカー）: `tools/call` は「tools/call」の表の 1〜6、`resources/read` は「リソース」。`Backend::call` の前後で `cancel.is_cancelled()` を見て、立っていれば `Outcome::Cancelled`。

### 結果の出し分けと切り詰め（`tools.rs`）

```text
fn list(version, caps) -> Vec<Value>:
    tools.json の各 tool について caps.allows(name) のものだけ、
    version < 2025-06-18 なら "title" と "outputSchema" を除く、version < 2025-03-26 なら "annotations" も除く
fn cap_text(text) -> (String, bool):
    chars が 200,000 以下なら (text, false)
    そうでなければ 先頭 200,000 文字 + "\n… truncated: {N} more characters"、true
fn shape(version, out: ToolOutput) -> Value        // 上の「結果の組み立て」
```

`Capabilities::allows(name)`: `tz_check`←`check`、`tz_guide`←`guide`、`tz_explain`←`explain`、`tz_symbol`・`tz_brief`・`tz_search`・`tz_callers`←`kb`、`tz_impact`←`impact`、`tz_find`←`find`、`tz_fix_plan`←`fix`、`tz_diagram`←`diagram`。`ProjectBackend::capabilities()` は、Phase 1 では `check` だけ `true`。**各手順がその行を `true` にする**。

### `ProjectBackend`（`backend.rs`）

`ProjectBackend { root: PathBuf, kb_dir: PathBuf, results: Mutex<ResultCache>（Phase 2）}`。`call` は `match request.name { "tz_check" => check::run(self, request), ... , name => Err(ToolError(format!("tool '{name}' is not available in this build"))) }`（`Capabilities` が正しければ到達しない）。Phase 2 の接続は、各ツールの 1 つの `match` の腕と、その腕が呼ぶ関数（`knowledge.rs`・`check.rs` の隣）。

### 変更の一覧（Phase 1）

| ファイル | 変更 |
| --- | --- |
| `src/main.rs` | `parse_mcp_arguments(&[OsString]) -> Result<McpArguments, String>`（`McpArguments { root: Option<PathBuf>, kb_dir: Option<PathBuf> }`。`parse_repl_arguments` と同じ `seen`）。`mcp_command(&[OsString]) -> ExitCode`: 引数の誤り→`E2000`（2）、`invalid_thread_setting(false)`、根の決定（`--root` かカレント → `cache::real_path` → ディレクトリか。失敗は `E2010`）、`tsuzuri::mcp::serve(Options{..}, std::io::stdin(), std::io::stdout().lock())`、`Err(Fatal(m))` は `E2010`（1）。`main`: `api-diff` の分岐の次に `if raw.first() == "mcp" { return mcp_command(&raw[1..]); }`。`HELP`: 使い方に `tsuzuri mcp [--root DIR] [--kb-dir DIR]`、説明に 3 行 |
| `src/lib.rs` | `pub mod mcp;` |
| `src/parallel.rs` | `pub(crate) const WORKER_STACK_BYTES` |

## 前提とする他チケットのインターフェース

G29 は、他チケットの関数を `ProjectBackend` の腕とその補助関数でだけ呼ぶ。**名前と型は、着手時に各チケットの「設計」で確かめ、違えば右の列の呼び出しだけを直す**（MCP の JSON の形は変えない）。2026-10-18 に読んだ `G21`・`G23`・`G24`・`G26`・`G27`・`G30` の各チケットの名前を書く。G25・G28 は起票中のため、G29 が必要とする形を書く。

| ツール | 使うもの | 未実装のとき |
| --- | --- | --- |
| `tz_check` | HEAD の `Project::load`・`analyze_all`・`copies::warnings`。G22 後は `Diagnostic::json_with_severity` が追加キー（D-51: `kind`・`related`・`help`・`expected`・`found`・`candidates`・`scope`・`fixes`）を出す | G22 なしでも 5 キーで動く |
| `tz_guide` | G21 の `guide::render(&Action, Edition) -> Result<Rendered, String>`（`Action::{Level(u8), Topic(String), List, Std(Option<String>)}`、`Rendered { level, topic, title, text }`）と `guide::list_text() -> String`。引数の対応は手順 11 | G21 が未完なら `guide` を `false` |
| `tz_explain` | G23 の `explain::resolve(&str) -> Result<Target, Vec<String>>`（`Err` は候補）、`Target::{Kind(DiagnosticKind), Code(&'static str)}`、`render_kind(kind, json) -> String`・`render_code(code, json) -> String`（`json = true` は 1 行の JSON。kind は `kind`・`code`・`severity`・`since`・`title`・`page`・`code_page`・`siblings`、コードは `code`・`severity`・`title`・`page`・`kinds`） | `explain` を `false` |
| `tz_symbol`・`tz_brief`・`tz_search`・`tz_callers` | G27 `src/kb/store.rs` の `KbData`（`KbData::open(dir)`・`KbData::from_parts(..)`・`symbol(id)`）、`src/kb/manifest.rs` の `inspect(dir, source_hash, stamp) -> Freshness`、`src/kb/input.rs` の `source_hash`・`stamp`、G26 の `kb::collect(&Project)`、G27 の `KbView`（`new(Some(Arc<KbData>), StdSource::Memory)`。`Memory` は G27 Phase 4 から。なければ `StdSource::None`）、G28 の照会（ID の解決・`show`・`brief`・`search`・`callers`/`uses`）。KB の行は D-52 のキー | `kb` を `false` |
| `tz_impact` | G28 Phase 3 の `impact`（2 つの `KbData` の差分）。`records` は `kb impact --json` の各行 | `impact` を `false` |
| `tz_find` | G25 の `find::run`（G28 の `kb find` と同じ入口。`Candidate { name, signature, how }`。D-51） | `find` を `false` |
| `tz_fix_plan` | G24 の `fix::run(&mut Project, &Options, &mut analyze) -> Report`（ファイルを書かない。`Report { rounds, stop, records, files, last }`）と `render_text`・`render_json`（`pub(crate)`）。`Options { threshold, scope, only_codes }` | `fix` を `false` |
| `tz_diagram` | G30 の `diagram::{Input::from_kb(&KbData), DiskSources { root }, render(&Input, &dyn Sources, &DiagramRequest) -> Result<DiagramOutput, DiagramError>}`（型は G30 の「ライブラリ API」。`DiagramOutput { format, text, nodes, folded }`） | `diagram` を `false` |

**KB の取得**（`knowledge.rs`。KB 系 4 ツールと `tz_impact` と `tz_diagram` が共有）:

```text
fn knowledge(&self) -> Result<Arc<KbView>, ToolError>:
  project = Project::load_for_tests(&self.root)               // Main.tz 不要（G27 §8 の build と同じ）
  hash = source_hash(&project); stamp = stamp()
  data: Arc<KbData> = match inspect(&self.kb_dir, &hex(hash), &stamp):
     Fresh      → Arc::new(KbData::open(&self.kb_dir)?)       // 読むだけ
     Incomplete → Err(ToolError("the project does not type-check, so there are no facts yet; call tz_check and fix the errors first"))
     それ以外    → モデル = kb::collect(&project)             // Err(DiagnosticSet) なら上と同じ Incomplete の文
                  Arc::new(KbData::from_parts(モデル, ...))   // メモリ上。ディスクへ書かない
  KbView::new(Some(data), StdSource::Memory)                  // manifest.json の SHA-256 か hash でメモリにキャッシュする
```

`kb::open::open(.., Refresh::Auto, ..)` は KB を**書く**ので使わない（D6）。std の KB は `StdSource::Memory`（キャッシュのディレクトリへ書かない。停止条件 4）。`tz_diagram`・`tz_impact` が要る `&KbData` は `view.project()`。

## 実装手順

各手順の後に `cargo build --locked`・`cargo test --locked`・`cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings` が緑。`running N tests` の N で件数を確かめる。

### Phase 1

**手順 1: ベースライン。** 変更: なし。内容: `cargo build --release --locked && cargo test --locked` の件数を控える。`mkdir -p /tmp/tz-G29/before && cp target/release/tsuzuri /tmp/tz-G29/tsuzuri-before`。`examples/*` と `tests/fixtures/*` の各プロジェクトで `tsuzuri-before check <dir> --json 2> /tmp/tz-G29/before/<名前>.json; echo $? >> ...` を保存する（`check` できないものも一覧に残す。以後「`check --json` が変わらない」の確認に使う）。付録 G の測定を 1 回再現する。`grep -rn "mcp" src tests Cargo.toml` が 0 件であることを確かめる。確認: すべて通る。

**手順 2: テスト先行（骨格とハーネス）。** 変更: `src/lib.rs`、`src/mcp/mod.rs`（`Backend`・`Capabilities`・`ToolRequest`・`ToolOutput`・`ToolError`・`Cancel`・`Options`・`Fatal` の型と、`serve_with` が `Err(Fatal("not implemented"))` を返す仮実装）、`tests/mcp.rs`（新規）。内容: `tests/mcp.rs` に `Client` ハーネス（`send`・`send_raw`・`recv`・`request`・`finish`。入力は `mpsc` で包んだ `Read`、出力は `Arc<Mutex<Vec<u8>>>` + `Condvar`。`recv` は最大 10 秒で失敗するだけ）と `FakeBackend`（付録 E。付録 D の例を返し、`gate` で止められる）を書き、付録 H の群 P（7 件）を足す。「応答がないこと」は、後続の `ping` の応答が**次の行**であることで確かめる（時間待ちをしない）。確認: `cargo test --locked --test mcp` が**コンパイルでき**、7 件が失敗（`7 failed`）。ほかのテストは手順 1 の件数のまま通る。

**手順 3: `rpc.rs`。** 変更: `src/mcp/rpc.rs`（新規）、`src/mcp/mod.rs`。内容: 「トランスポートと JSON-RPC」の表のとおり `read_line`・`parse`・応答の組み立て・`write_line`。`read_line` は `BufRead` から 1 行を読み、`\n` までを `take(MAX_LINE_BYTES + 1)` で制限し、超えたら `Line::TooLong` を返して次の `\n` まで読み捨てる。単体テスト（付録 H の rpc の 4 件。`parses_requests_notifications_and_invalid_messages` は表の全行、`ids_round_trip_integers_and_strings` は `i64::MIN`・`u64::MAX`・`"x"`）。確認: `cargo test --locked mcp::rpc` が 4 件通る。

**手順 4: `schema.rs`。** 変更: `src/mcp/schema.rs`（新規）。内容: 「検証器」の規則どおりに `validate(schema, value) -> Result<(), String>` と `SUPPORTED_KEYWORDS`（16 個: 上の一覧）、`keywords_outside_the_subset`。単体テスト（付録 H の schema の 4 件。メッセージは上の表の全行）。確認: 4 件通る。

**手順 5: `tools.json`・`tools.rs`。** 変更: `src/mcp/tools.json`（付録 B をそのまま）、`src/mcp/tools.rs`（新規）。内容: `Registry::load()`（`include_str!`）、`list`、`cap_text`、`shape`、`Features`。単体テスト（付録 H の tools の 5 件。`tools_json_is_well_formed` は、名前が一意・`tz_` で始まる・128 文字以下・両 schema の根が object・`inputSchema.additionalProperties == false`・`outputSchema.required` に `truncated`・全キーワードが部分集合・各 `required` のキーが `properties` にあること）。確認: 5 件通る。

**手順 6: `session.rs`。** 変更: `src/mcp/session.rs`（新規）、`src/mcp/mod.rs`（`serve_with` と `serve` を実装）。内容: 「主ループ」と各表のとおり。`serve_with(backend, input, output)`、`serve(options, input, output)`（`ProjectBackend::new(options)` を作って `serve_with`）。**`tz_check` が未実装でも**、`FakeBackend` で次が通る。確認: 手順 2 の 7 件が緑になり、付録 H の群 T（11 件）と群 C（10 件）を足して全部通る（合計 28 件）。

**手順 7: `path.rs`・`check.rs`。** 変更: `src/mcp/path.rs`・`check.rs`（新規）、`src/mcp/backend.rs`（`ProjectBackend` と `Capabilities` の `check` を `true`）。内容: 「パスの閉じ込め」「`tz_check`」のとおり。テスト（付録 H の群 K の 8 件。`tests/mcp.rs` が一時の root に付録 G の `Main.tz` を書く。`tz_check_matches_check_json` は、同じ root で `tsuzuri check Main.tz --json` を子プロセスで実行し、標準エラーの各行を `Value` にして、`path` の `./` を除いたものと `diagnostics` を比べる）。確認: 8 件通る。

**手順 8: CLI。** 変更: `src/main.rs`。内容: 「変更の一覧（Phase 1）」のとおり。単体テスト（付録 H の `parses_mcp_arguments` 1 件）。確認: `cargo test --locked` が通る。手順 1 の `check --json` の保存と、現在のバイナリの出力が 1 byte も変わらない（`diff`）。

**手順 9: 子プロセスのテストと静的な検査。** 変更: `tests/mcp.rs`。内容: 付録 H の群 L（6 件）。`mcp_sources_use_no_process_env_fs_writes_or_stdout` は `src/mcp/*.rs` を読み、禁止の語を含む行が 0 件であることを見る簡易の検査でよい（`tests` モジュールと文字列リテラルを除く）。確認: 6 件通る。

**手順 10: 文書（Phase 1）。** 「ドキュメント」の Phase 1 の行。確認: `node scripts/check-docs.mjs _tsuzuri/language-reference/compiler/mcp.md _tsuzuri/language-reference/compiler/usage.md _tsuzuri/language-reference/compiler/option.md _tsuzuri/language-reference/compiler/diagnostics.md _tsuzuri/language-reference/index.md` が通る。**Phase 1 の最終確認**: 付録 F の SDK のクライアントで、実サーバーに接続して `tz_check` を呼ぶ。

### Phase 2（依存が `done` のツールだけ。順は自由）

各手順の型: 変更 = `ProjectBackend::call` の腕 + 補助関数（`knowledge.rs`・`check.rs`）と `capabilities()` の 1 行、`tests/mcp_knowledge.rs`。**テストは、実際のコンパイラの出力（KB・fix・explain）と、MCP の結果の `structured` を比べる**（期待値を MCP の出力から作らない）。

**手順 11: `tz_guide`（G21）。** `topic` を `action` に次のとおり写し、`guide::render(&action, Edition::LATEST)` を呼ぶ。`topic` なし → `Action::Level(0)`（`structured.topic` は `"card"`）、`"list"` → `guide::list_text()`（`structured.topic` は `"list"`）、`"std"` → `Action::Std(module)`（`module` は引数のまま）、そのほか → `Action::Topic(topic)`。`Err(message)` は `isError`（未知は `unknown guide topic`）。テスト `tz_guide_returns_the_card_and_lists_topics`（`tsuzuri guide` の標準出力と `text` が等しい）。`guide` を `true`。

**手順 12: `tz_explain`（G23）。** `explain::resolve(query)` が `Err(候補)` なら `isError`（表のメッセージ。候補は先頭から最大 5 つ）。`Ok(target)` なら、`render_kind`／`render_code` を `json = false` で `text`、`json = true` を `serde_json::from_str` で読んで `code`・`kind`・`title`・`since` を写す（コードを引いたときは `kind`・`since` を出さない）。テスト `tz_explain_returns_a_page_and_rejects_unknown_queries`（`tsuzuri explain E1012` の標準出力と `text` が等しい）。`explain` を `true`。

**手順 13: KB の取得（G26・G27）。** 「KB の取得」の関数 `knowledge()` と、`Arc<KbView>` のメモリのキャッシュ（キーは `manifest.json` の SHA-256 または `hash`）。テスト `kb_acquisition_reads_a_fresh_disk_kb_and_rebuilds_in_memory_when_stale`・`kb_acquisition_writes_nothing`（呼び出しの前後で一時 root の全ファイルの（パス・サイズ・更新時刻）が同じ）・`kb_tools_report_an_incomplete_kb_for_a_failing_project`。**ツールはまだ足さない**。

**手順 14: `tz_symbol`・`tz_brief`・`tz_search`・`tz_callers`（G28 Phase 1）。** 4 つの腕。ID の解決は G28 の決定表（曖昧・未知のメッセージは「ツール」の表）。`callees` は 50 件、`tests` は 20 件で切り、`truncated`。`tz_callers` は `relation` で `calls` か `uses_type`、深さの幅優先で訪問済みの集合を持つ（循環で止まる）。テスト（`tests/mcp_knowledge.rs`）: `tz_symbol_matches_the_kb_row`（`examples/point` を一時 root へ写し、`symbols.jsonl` の `Point.hypotenuse` の行と `structured.symbol` が等しい。行は G26 の golden から）・`kb_tools_agree_with_the_cli`（`kb brief`・`search`・`callers` の `--json` と `structured`、text と `text`）・`tz_callers_follows_depth_and_stops_on_cycles`・`kb_tools_reject_unknown_and_ambiguous_ids`。`kb` を `true`。

**手順 15: `tz_find`（G25）。** `project.analyze_all()` が `Ok(module)` のとき `find` を呼び、`limit` で切る。解析が失敗したときは `isError`・本文 `the project does not type-check; call tz_check first`。テスト `tz_find_matches_the_cli`。`find` を `true`。

**手順 16: `tz_impact`（G28 Phase 3）。** `old`・`new` を `resolve_path`（ディレクトリ）。各ディレクトリが KB（`.tsuzuri-kb` のマーカーがある）なら `KbData::open`、そうでなければプロジェクトとして `kb::collect` でメモリ上に作る。`records` は `impact --json` の各行を `Value` にしたもの。テスト `tz_impact_reports_a_signature_change`・`tz_impact_rejects_paths_outside_the_root`。`impact` を `true`。

**手順 17: `tz_fix_plan`（G24）。** 入力を `Project::load`、`Scope`（ファイルなら `File(index)`）と `Threshold` から `Options`、`fix::run(&mut project, &options, &mut |p| fix::analyze_project(p, false))`。`render_text` を `text`、`render_json` の各行を解析して `fix`（`status: applied`）→ `fixes`、`skipped` → `dropped`、`file` の `diff` を連結 → `diff`、`summary` → `rounds`・`stop`・`remaining_*`。**ファイルは書かない**（`fix::run` はメモリ上だけを書き換える）。テスト `tz_fix_plan_matches_fix_dry_run_and_changes_no_file`（`tsuzuri fix --dry-run` の標準出力と `text`、実行の前後でファイルが同じ）。`fix` を `true`。

**手順 18: `tz_diagram`（G30）。** `knowledge()` の `view.project()` から `Input::from_kb`、`DiskSources { root }`、`DiagramRequest { kind, id, depth（既定 2）, max_nodes（既定 60）, format }` で `render` を呼ぶ。`text`・`format`（`mermaid`・`dot`・`markdown`）・`nodes` を写し、`folded` を `truncated` に。`DiagramError` は `isError`（本文は `message`）。テスト `tz_diagram_matches_kb_diagram`（`tsuzuri kb diagram` の標準出力と `text`）。`diagram` を `true`。

**手順 19: 結果のキャッシュ（`cache.rs`）。** `ResultCache`（`VecDeque<(String, ToolOutput)>`、8 件、最近使った順）。キー = SHA-256（`cache::Sha256`）に、ツール名、正規化した引数（`serde_json::to_string` の辞書順）、入力ハッシュ（`tz_check`・`tz_find`・`tz_fix_plan` は `project.sources` の User の全テキストと `manifests` のパスと内容。KB 系は `source_hash`）を `field` で入れた値。ヒットしたら解析せずに同じ `ToolOutput` を返す。`CheckedModule` は保持しない（D12）。テスト `result_cache_returns_identical_bytes_and_skips_the_analysis`（`ProjectBackend` の `analysis_count`（`pub(crate)` のカウンタ）が 1 のまま）・`result_cache_invalidates_when_a_source_changes`・`result_cache_keeps_eight_entries`。確認: 3 件通る。

**手順 20: 文書（Phase 2）。** ツールの表と、ツールごとの例を `mcp.md` に足す。`check-docs.mjs` が通る。

### Phase 3

**手順 21: テスト先行。** `tests/mcp.rs` に `resources_are_declared_and_listed_as_in_the_appendix`・`resources_read_returns_markdown`・`resources_read_not_found_is_32002`・`resources_uri_decoding_table`（`%3A%3A` → `::`、`%E6%97%A5`、`%`・`%G0` は `-32602`）・`resources_follow_capabilities`。失敗する。

**手順 22: `resources.rs`。** 「リソース」の表。`initialize` の `capabilities` に `resources: {}`（Phase 3 から）。`resources/read` は `Backend::read_resource`（`ProjectBackend` は、`tz_guide`・`tz_brief`・`tz_explain` の腕と同じ関数を呼ぶ）。確認: 手順 21 のテストが通る。

**手順 23: 文書（Phase 3）。** `mcp.md` の資源の節。

### Phase 4

**手順 24: 敵対的な入力。** `tests/mcp.rs` に `hostile_inputs_never_break_the_session`（表。各行を送り、期待のコード、直後の `ping` が通る）。表: 不正な UTF-8（`FF FE`）、NUL を含む JSON 文字列、対にならないサロゲート `"\ud800"`、129 段の入れ子、`id` が `1e400`・`18446744073709551616`、`id` が `i64::MIN`、`method` が 1 MiB の文字列、`arguments` に 10,000 個の未知のキー、`path` に `../`・`..\`・`//etc`・`C:\`・NUL・4,097 文字、`tools/call` の `name` が 1 MiB、100 件の `tools/call` を連投（65 件目以降は `-32000`）、`notifications/cancelled` に不正な `requestId`、`resources/read` の `uri` が `tz://kb/brief/%`。

**手順 25: 公式 SDK での確認。** 付録 F の手順で、Phase 1〜3 の実サーバーに `@modelcontextprotocol/sdk` のクライアントが接続し、`tools/list` と各ツールの `callTool` が通る（結果が `outputSchema` に合わなければクライアントが拒否する）。結果を完了報告に書く。

**手順 26: 文書の仕上げ。** 「ドキュメント」の全項目。`node scripts/check-docs.mjs` を全ページに。

## テスト計画

期待値の出どころ: プロトコルは**MCP 仕様の例と公式 `schema.json`**（付録 A・C）、`tz_check` の診断は HEAD の `tsuzuri check --json` の実出力（付録 G）、KB・fix・explain・図は**それぞれの CLI の出力**。MCP の出力を期待値に写さない。

**Phase 1 のテストの全一覧は付録 H**（`tests/mcp.rs` の群 P 7 件・群 T 11 件・群 C 10 件・群 K 8 件・群 L 6 件、単体テスト 14 件。合計 56 件）。Phase 2 は `tests/mcp_knowledge.rs`（手順 11〜19。各ツールで「CLI の出力と `structured`・`text` が等しい」「ID の誤り」「KB が不完全」「何も書かない」）。Phase 3 は手順 21、Phase 4 は手順 24。

**既存テストへの影響: なし。** `src/main.rs` は `mcp` の分岐と関数を足すだけで、`HELP` の既存行を変えない（`HELP` のテストは `contains` で見るだけ）。`src/parallel.rs` は可視性だけ。手順 1 と手順 8 の `check --json` の保存の比較で確かめる。

**資源上限のテスト**: 4 MiB の行・129 段の入れ子・65 件の待ち・200,000 文字の本文・400,000 bytes の `structured`（手順 6・24）。**決定性**: `tz_check_output_is_deterministic`。**native と WASM の差**: なし（コード生成に触れない）。

## ドキュメント

更新するページ（手順 10・20・23・26）:

| ページ | 内容 |
| --- | --- |
| `_tsuzuri/language-reference/compiler/mcp.md`（新規） | 概要（読み取り専用）・起動・クライアントの設定（付録 F）・ツールの表（引数・結果・誤り）・資源・上限・安全（根の外に出ない・書かない・信頼できないプロジェクトの結果をモデルへの命令として扱わない）・対応する MCP の版と 2025-11-25 以降の扱い・トラブルシューティング（`E2010`、stdout に何も出ない、`tools/list` が空）。誤りを含むソースの例は ```` ```text ````（```` ```tsuzuri ```` は `check` を通る例だけ） |
| `_tsuzuri/language-reference/index.md` | コンパイラの節に `[MCP サーバー](./compiler/mcp.md)` |
| `_tsuzuri/language-reference/compiler/usage.md` | `tsuzuri lsp` の説明の隣に `tsuzuri mcp` |
| `_tsuzuri/language-reference/compiler/option.md` | 「どのサブコマンドが受け付けるか」に `mcp`（`--root DIR`・`--kb-dir DIR` だけ）、`TSUZURI_THREADS` の表の `repl`・`lsp`・`publish` の並びに `mcp` |
| `_tsuzuri/language-reference/compiler/diagnostics.md` | `E2010` の行（`E2007` の次）、`E2000` の行に `mcp` の引数 |
| `docs/language.md` | 診断表に `E2010`、`check`／`build`／`run`／`test`／`doc`／`lsp` の列挙（`git` もネットワークも使わない）に `mcp`、`tsuzuri api-diff` の説明の隣に `tsuzuri mcp` の 1 段落 |
| `README.md` | 冒頭の開発支援の箇条書きと「開発・AI 支援」の行に MCP、「言語サーバー (`tsuzuri lsp`)」の次に「MCP サーバー (`tsuzuri mcp`)」、CLI の一覧に `tsuzuri mcp [--root DIR] [--kb-dir DIR]` |
| `docs/architecture.md` | ファイルの表に `src/mcp/*.rs`、「言語サーバー」の段落の次に MCP の段落（スレッド 3 つ・上限・読み取り専用） |
| `docs/stability.md` | 「互換性」の 2 行（手順 10） |

検索: `grep -rnE "tsuzuri lsp|E2007|api-diff" README.md docs _tsuzuri/language-reference | head -40` で隣り合う箇所を見つけ、同じ変更で直す。検証: `node scripts/check-docs.mjs <変更したページ>`。

## 受け入れ条件

### Phase 1

- [x] `tsuzuri mcp` が、付録 F の公式 SDK のクライアントで `initialize`・`tools/list`・`tools/call tz_check` を通る。
- [x] `tests/mcp.rs` の Phase 1 のテスト（手順 2・6・7・9 の名前）がすべて通り、`src/mcp/*.rs` の単体テストが通る。
- [x] `tools/list` は `tz_check` だけで、`tools.json` の該当要素と等しい。16,384 bytes 以下。
- [x] stdout は JSON の行だけ。ファイルを書かない・子プロセス・環境変数の読み取り（`src/mcp/`）がない（静的な検査）。
- [x] 手順 1 の `check --json` の保存と、現在のバイナリの出力が 1 byte も変わらない。
- [x] 文書（手順 10）。`cargo fmt`・`clippy`・`cargo test --locked` が緑。

### Phase 2

- [x] 依存が `done` のツールすべてが `tools/list` に出て、それぞれのテストが通る。未完の依存のツールは出ない。
- [x] KB の取得が何も書かない（手順 13 のテスト）。結果のキャッシュのテストが通る。
- [x] `tools/list` が 16,384 bytes 以下のまま。

### Phase 3 と Phase 4

- [x] 資源のテストと敵対的な入力の表のテストが通る。`initialize` が `resources: {}` を宣言する。
- [x] 公式 SDK のクライアントで、全ツールと資源が通った（結果を完了報告に書く）。
- [x] 文書の全ページが `check-docs.mjs` を通る。

## 落とし穴

| 落とし穴 | 検出 |
| --- | --- |
| `println!` を書いて stdout を汚す | 手順 9 の静的な検査と、子プロセスのテスト |
| 通知にエラーを返す（JSON-RPC は通知に応答しない） | `notifications_never_get_a_response` |
| 応答の `id` を作り直す（`1` と `"1"` を混ぜる） | `ids_round_trip_integers_and_strings` |
| `1.0` を整数と受理する | `integer_rejects_floats` |
| 根の絶対パスが `message`・`path` に漏れる | `tz_check_never_shows_the_root_path` |
| ワーカーの stack が小さい | `WORKER_STACK_BYTES`（8 MiB）。`std::thread::spawn` を使わない |
| `Backend` のテストで時間を待つ | ゲート（`Mutex` + `Condvar`）と「次の行が `ping` の応答」で順序を確かめる |
| KB 系のツールが `kb build` や `Refresh::Auto` を呼び、ディスクに書く | 手順 13 の書き込みなしのテスト |
| `tools.json` の `description` を足して `tools/list` が膨らむ | `tools_list_stays_small` |

## 対象外

HTTP の transport と認可、prompts・completions・logging・sampling・elicitation・roots、サーバーからの通知と `resources/subscribe`、MCP 2025-11-25 以降の版（D2）、バッチ（D3）、書き込み・適用・実行をするツール、複数の根、daemon 化、`vsc/` の拡張からの自動登録、ツールごとの権限（E15 の `audit`）、進捗の通知（`_meta.progressToken` は無視する）。

## 決定事項

すべて承認済み（D-50）。

- **D1: stdio だけ。** 理由: MCP は「クライアントは stdio を可能なら支える SHOULD」で、ローカルの読み取り専用の用途に足りる。HTTP は Origin の検査・認可・セッションが要る。却下: Streamable HTTP（攻撃面が増え、利益がない）。
- **D2: 実装する版は 2025-06-18。2025-03-26・2024-11-05 は echo して出し分け、未知（2025-11-25・2026-07-28 を含む）の要求には 2025-06-18 を返す。** 理由: 指示と種が 2025-06-18 で、公式の `schema.json` と公式 SDK のクライアントで検証できた版（付録 A・F）。2025-11-25 は追加だけだが、実クライアントで確かめるまで「対応」と書かない。2026-07-28 は `initialize` を廃止した別の設計（付録 A.3）で、modern だけのクライアントは接続できない。却下: 2025-11-25 を echo（未検証の主張）、2026-07-28 も実装（別チケット）。**このリスクは完了報告に書く**。
- **D3: JSON-RPC のバッチは受けない**（`-32600`）。理由: 2025-06-18 が削除した。応答の集約にワーカーの完了待ちが要り、利用に見合わない。2025-03-26 の「バッチの受信」は既知の逸脱。却下: 同期のメソッドだけ受ける（半端）。
- **D4: `content[0].text` は CLI の text 出力、`structuredContent` は JSON。** 仕様の「同じ JSON を TextContent にも入れる SHOULD」には従わない。理由: 多くのクライアントは `content` だけをモデルに渡し、JSON の複製は入力のトークンを倍にする。`text` は CLI と同じ関数が作るので決定的。却下: JSON の文字列を `text` に入れる。
- **D5: 引数の検証の失敗は `isError: true`。未知のツール・`params` の形の誤りは `-32602`。** 理由: 2025-06-18 は「invalid input data」をツールの実行の誤りに数え、2025-11-25 は検証の誤りを自己修正のために `isError` にすると明記した。却下: 検証の失敗を `-32602`（モデルに見えない）。
- **D6: 読み取り専用。** ディスクに何も書かない（プロジェクト・KB・frontend cache・std の KB のキャッシュ）。`tz_check` は `analyze_all`（測定で速さは変わらない）、KB は既存の KB を読み、古ければ**メモリ上**に作る。`tz_fix_plan` は編集を返すだけ。理由: クライアントの sandbox で動く。却下: `Refresh::Auto` で KB を書く（sandbox で失敗する）、fix の適用（書き込みのツールは確認の設計が別に要る）。
- **D7: `Backend` と `Capabilities`。** 理由: 依存が未完でも出荷でき、試験が偽のバックエンドで書ける。却下: `cfg` の feature（`Cargo.toml` を変える）、未実装のツールを出して `isError` にする（モデルが無駄に呼ぶ）。
- **D8: ワーカー 1 本（stack 8 MiB）、FIFO、取り消しは結果を捨てる。** 理由: 解析は主スレッドの stack で動く前提で、`WORKER_STACK_BYTES` が同じ大きさ。中断の点がなく、`ping` は即答する。却下: 要求ごとのスレッド、子プロセス（禁止）、リーダースレッドでの実行（`ping` が詰まる）。
- **D9: 上限は 200,000 文字・400,000 bytes・4 MiB・64 件。** 超えたときの動き（切り詰め・エラー・`-32000`）を決めて曖昧さをなくす。
- **D10: パスは拒否し、丸めない。** 理由: `..` を黙って取り除くと、別のファイルを検査したことが利用者に見えない。
- **D11: 出力のキーは辞書順**（`serde_json` の既定）。理由: `preserve_order` は `indexmap` を足す。JSON の object に順序の意味はない。
- **D12: キャッシュは `ToolOutput` だけ（8 件）。** 理由: `CheckedModule` は `Rc` を含み、スレッド間で動かせない可能性が高い。出力は決定的なので、出力のキャッシュで足りる。
- **D13: LSP と解析を共有しない。** 理由: LSP の `Session` は開いたバッファ・UTF-16 の位置・debounce・意味の索引に結びつく。MCP は `Project::load` と解析の入口を直接呼ぶ。
- **D14: ツールは 11 個（種の 10 個 + `tz_search`）。`tz_callers` は `relation` で `calls` と `uses_type` を選ぶ。** 理由: G28 の `search`・`callees`・`uses`・`tests-for` を MCP から届かせる（`callees`・`tests` は `tz_symbol` に入れる）。ツールを増やすと `tools/list` が全クライアントの文脈を増やすので、これ以上は増やさない。
- **D15: `tools/list` はページ分割しない。** `cursor` はどんな値でも `-32602`。
- **D16: Phase の切り方を種から変えた。** パスの閉じ込め・取り消し・応答の上限は最初のツールの出荷に要るので Phase 1 に入れ、Phase 2 は依存チケットごとの接続とキャッシュにした。
- **D17: `E2010` は致命的な誤りだけ**（根・入出力・ワーカー）。MCP の誤り（`-32xxx`）は診断コードにしない。
- **D18: `instructions`（347 文字）を返す。** クライアントがシステムプロンプトに入れられる。`tz_check` を最初に勧める。

## 付録

[G29-appendix.md](G29-appendix.md)（A: MCP 仕様の抜き書きと検証、B: `tools.json`、C: 往復の例、D: ツールごとの例）と [G29-appendix-impl.md](G29-appendix-impl.md)（E: Rust の骨格、F: クライアントの設定と公式 SDK での確認、G: 測定と再現、H: Phase 1 のテストの一覧）。
