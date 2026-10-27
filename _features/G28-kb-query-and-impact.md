# G28: `tsuzuri kb` の照会と `impact`（facts の差分）

| 項目 | 内容 |
| --- | --- |
| ID | G28 |
| 優先度 | P1（D-50 の表） |
| 規模 | L。Phase 1 M（照会 7 コマンドと ID の解決）、Phase 2 S（`kb find`）、Phase 3 L（`kb impact`）、Phase 4 S（`test --affected`） |
| 依存 | [G27](_completed/G27-build-time-kb.md) Phase 2（`KbData`・`KbView`・`open`・`Card`・`kb` の分岐）。[G26](_completed/G26-facts-model.md)（行のキー）。Phase 2 は [G25](G25-typed-holes-and-find.md) の Phase 3（`tsuzuri find`）。graph・facts が無い KB には縮退する（「前提とする他チケットのインターフェース」） |
| 後続 | [G29](G29-mcp-server.md)（照会の関数と `impact` を MCP が呼ぶ）、G31（AI 作業コスト指標の KB 方式） |
| 状態 | todo |
| 起票 | 2026-10-18（D-50。HEAD `a2313d19` で確認。確認に使った作業ツリーは `9dd0fbbc`（main）で、`git diff --stat 9dd0fbbc a2313d19` は `docs/benchmarks.md` の 41 行だけ。ソースは同じ） |
| 承認 | 承認済み（D-50: 人間の包括許可 2026-10-18）。決定事項 D1〜D12 はすべて承認済み |
| 改善する劣位 | AI が「この関数は誰に呼ばれるか」「この変更でどのテストを回すか」を知るには、`grep` で探してファイルを全文読むしかない。変更の影響（署名・所有権の渡し方・効果・テスト）は `api-diff` が公開 API の署名しか見ないので、効果の追加や非公開の変更は人が読み解く。KB の行を、決定的な 1 項目 1〜3 行の出力で引けるようにし、2 つの KB の差を分類して出す（D-50 の調査結論 (1)・(3)） |
| 手本にする既存実装 | 差分の分類と出力: `src/api_diff.rs` の `Change`・`Report::render_text`・`render_json`・`compare`・`MAX_LISTED_CHANGES`、`src/main.rs` の `api_diff_command`・`api_diff_usage`。git の呼び出し: `src/fetch.rs` の `git_command`・`failure_line`。テストの選択: `src/test_runner.rs` の `TestOptions`（`indices`）、`src/main.rs` の `Arguments.test_indices` と `run_test_action`（`--index`・`--list` の扱い）。切り詰め: G27 の `Card`。引数のテスト: `rejects_ambiguous_or_unused_arguments`。CLI のテスト: `tests/api_diff.rs` の `root`・`tsuzuri` |
| 主な影響ファイル | **Phase 1**: 新規 `src/kb/query.rs`・`suggest.rs`・`tests/kb_query.rs`・`_tsuzuri/language-reference/compiler/kb.md`（節を足す）。変更 `src/kb/command.rs`・`store.rs`（G27）、`src/main.rs`。**Phase 2**: `src/kb/command.rs`。**Phase 3**: 新規 `src/kb/impact.rs`・`git.rs`、`tests/kb_impact.rs`、`tests/fixtures/kb_shop_head/`。変更 `src/fetch.rs`（可視性）。**Phase 4**: 変更 `src/main.rs`・`src/test_runner.rs`・`src/kb/command.rs`、`tests/kb_affected.rs`（新規）。文書は各 Phase で |

詳細な実例・参照実装は [付録](G28-appendix.md) にある（以下「付録 A〜F」）。

## KG への更新（D-60、2026-10-28）

本チケットは [D-60](GUIDE.md#d-60-kb-から-kg知識グラフへの再設計g26g27g28g29g30) で KB から **KG（知識グラフ）** に改めた。コマンドは `tsuzuri kg …`、テストは `tests/kg_query.rs`・`tests/kg_impact.rs`、fixture は `kg_shop_head`。照会はすべて `KgIndex` の上のグラフの探索で、アルゴリズムは `src/kg/algo.rs` の 1 か所にある。

| 本文 | KG での実装 |
| --- | --- |
| `callers`・`callees`（幅優先） | `algo::bfs`（有界・親つき）。出力の規則（§3）は同じ |
| `impact` の影響を受けるテスト（逆引き・深さ 8・クラスのメソッドの辺） | `algo::reverse_reach`（深さつき。`calls` と、`instance_of` から作る仮想の辺）。`--json` の `tests` の行に、選んだテストごとの最短の根拠の鎖 `paths`（`[{"test","via":[ID…]}]`。変更したノードからテストまで）を足す（キーの追加。テキストの出力は同じ） |
| Phase 2 `kb find`（G25 を待つ） | **G25 を待たない**。`kg find TYPE` は、KG の関数のノード（プロジェクトと std の KG）の `params`・`result` の型の文字列を解析し、問い合わせの型と型変数の付け替えで照合する（`exact`・`generalizes`・`specializes`）。G25 の `tsuzuri find` は後でこの実装を呼ぶ |
| 新しい照会 | `kg path FROM TO [--rel R[,R…]] [--depth N]`（最短路。D-60） |
| `kb` の分岐の `COMMANDS` | `build, verify, clean, export, brief, show, callers, callees, uses, tests-for, search, find, impact, path, diagram` |

## 実装者向け早見

- **最初に読む**: [GUIDE-LITE.md](GUIDE-LITE.md)、`GUIDE.md` の D-52、G27 の §7（`Card`）と「設計」、G26 の「仕様」（行のキー）、このチケットの「仕様」。例は付録 B（照会）・C（`impact`）。
- **最初に書くテスト**: Phase 1 は手順 2 の `tests/kb_query.rs::show_matches_the_appendix_output`。Phase 3 は手順 10 の `tests/kb_impact.rs::public_changes_equal_api_diff`。
- **触ってはいけないもの**: 既存のコマンドの出力・終了コード（`api-diff` を含む）、KB のファイルの形（G26・G27）、`tsuzuri test` の既定の動き（`--affected` を付けなければ変わらない）、`test_runner` の実行の意味。
- **D-55 に足す名前**: `kb` のサブコマンド `brief`・`show`・`callers`・`callees`・`uses`・`tests-for`・`search`・`find`・`impact`、オプション `--path`・`--budget`・`--depth`・`--module`・`--project`・`--std`（`search`）・`--git`・`--from`・`--to`、`test --affected`。`E2009` の kind は増やさない。
- **照会は KB を読むだけ**: 解析しない。古ければ G27 の `open` が作り直す（`--no-refresh` で止める）。関数は `&KbView` を取りディスクに触れない（G29 が使う）。
- **1 項目 1〜3 行・決定的**: 整列は UTF-8 の byte 順。文字数は `chars().count()`。ID が曖昧なら**選ばずに**候補を出して止まる。
- **`impact` の終了コードは 0**（エラー以外）。判定は出力の区分（`breaking`・`non-breaking`・`info`）。
- 迷ったら「全テストを回す・何も決めずに候補を出す」に倒す（保守側）。

## 目的

AI が KB を**少ない文字数で引ける**ようにし、変更の**影響を機械的に分類**して返す。

1. 照会 7 コマンド（`brief`・`show`・`callers`・`callees`・`uses`・`tests-for`・`search`）。ID は完全名でなくても、一意に決まれば解決し、曖昧なら候補を出す。出力は既定でテキスト（1 項目 1〜3 行）、`--json` で JSON Lines。`--budget N` で文字数を絞る（優先度の低い行から `…N more`）。
2. `kb impact`: 2 つの KB（または 2 つのプロジェクト、または git のリビジョンと作業ツリー）の差を、署名・所有権の渡し方・効果・site の件数・警告・影響を受けるテストに分け、`breaking`・`non-breaking`・`info` に分類する。公開 API の行は `api-diff` と同じ文字列。
3. `tsuzuri test --affected`: `impact` が返したテストだけを実行する。判断できないときは全テストを実行する（保守的）。

**HEAD の数字**（付録 A）: `check`（KB なし）は 1／200／1,000 モジュールで 39／70／446 ms。照会は KB のファイルを読むだけなので、解析の費用を払わない。`api-diff` は 2 つのプロジェクトを**それぞれ解析**して公開の署名だけを比べる。

## 着手条件と停止条件

### 着手条件

- 依存: G27 Phase 2 が `done`（`grep -n "pub struct KbData" src/kb/store.rs`・`grep -n "pub fn open" src/kb/open.rs`・`grep -n "pub struct Card" src/kb/brief.rs` が各 1 件）。Phase 2 は G25 の Phase 3 が `done`（`grep -n "fn find_command" src/main.rs` が 1 件）。facts・graph の行は G26 のあるものだけ使う。
- ベースライン（GUIDE §2.3）: `cargo build --release --locked && cargo test --locked` の件数を控える。`tests/fixtures/kb_shop`（G27 の付録 D）を `check` して `target/tz/kb` ができることを確かめる。

### 停止条件（止めて報告する）

1. G27 Phase 2 が無い、または `KbData` が付録 E の型と違い、行を引く手段（ID・辺）が無い。選択肢: A＝G27 に足す（`store.rs` の索引。手順 1）、B＝待つ。
2. G26 の行のキーが「照会が使う行のキー」（§3 の末尾）と食い違う。選択肢: A＝表と付録を G26 に合わせる、B＝G26 に追加を求める。
3. `impact` の公開 API の行（`breaking`・`non-breaking`）が `api-diff` の行と、意図した差（§5 の「差」）以外で違う（手順 10 の `public_changes_equal_api_diff`）。
4. `--git` が、実行の前後で作業ツリー・index・`git stash list`・ブランチを変える、または正常終了・エラー・`panic` の後に `git worktree list` に一時ディレクトリが残る。（Ctrl-C と `SIGKILL` は `Drop` が走らないので対象外。§5 の 6）
5. 同じ入力で 2 回の出力が 1 byte でも違う（`HashMap` の順の漏れ）。
6. Phase 4: 差分テスト（編集の列）で、全テストを回して落ちたテストが `--affected` の集合に入っていない。選択肢: A＝逆引きの条件を足す、B＝Phase 4 を出荷しない。
7. Phase 2: `find_command` が `fn find_command(arguments: &[OsString]) -> ExitCode` でない。選択肢: 呼び出しだけ直す。
8. `unsafe`・新しい crate・既定の WASM の import・予約語が必要に見える。IR を変える。

## 現状（HEAD `a2313d19`。作業ツリー `9dd0fbbc`）

実測は `target/release/tsuzuri`（9,465,008 byte）。環境: Apple 製 macOS、`rustc 1.98.1`、git 2.55.0。作業ディレクトリは `target/tz-G27-work/`（`/tmp` を使えない環境のため）。

**KB の照会は存在しない。** `grep -rn "kb_query\|tests-for\|--affected\|impact" src` は `src/` に該当なし（`impact` は `_features` の文書だけ）。G27 が出荷すると `kb` サブコマンドの分岐（`build`・`verify`・`clean`・`export`）と `KbData`・`open` ができる。

**`api-diff`**（`src/api_diff.rs`）: `api(&Project)` が公開宣言の表を作り、`compare(&Api, &Api)` が `Report`（`Change { breaking, action, kind, name, old, new, note }`）を返す。`name` は完全名（`Acme::Shop::Cart.Line.tax`。G26 の ID と同じ綴り）。`render_text` は `{severity}: {action} {kind} {name}: \`旧\` => \`新\` ({note})` の行を最大 `MAX_LISTED_CHANGES`（1,000）件、超えたら `note: N more changes not shown`、最後に `summary: B breaking, N non-breaking`。`render_json` は `{"type":"change","severity":…,"action":…,"kind":…,"name":…,"old":…,"new":…,"note":…}` を 1 行ずつ、最後に `{"type":"summary",…}`。**終了コードは breaking があれば 1**、使い方の誤りは `E2000` で 2。分類の規則（`compare_items`・`compare_members`）: 削除は breaking、追加は non-breaking（フィールド・case の追加は breaking、デフォルトのないメソッドの追加は breaking）、署名の変更は breaking、`export` を外す・`const` を外すは breaking、`derive` を外すは breaking、`@deprecated` と説明の変更は non-breaking、フィールド・case の順序の変更は breaking。

実例（`tests/fixtures/kb_shop` → `kb_shop_head`。付録 A-2。G27 の付録 D のプロジェクトを編集したもの。実測の作業場では `shop` → `shop-head`）:

```text
breaking: added field Acme::Shop::Cart.Line.tax: `tax: i64` (constructions of the record without it stop compiling)
breaking: changed def Acme::Shop::Report.print_total: `def print_total :: ref [Line] -> unit` => `def print_total :: [Line] -> unit`
non-breaking: added def Acme::Shop::Cart.discount: `def discount :: ref [Line] -> i64 -> i64`
summary: 2 breaking, 1 non-breaking
note: version 0.1.0 is compatible with 0.1.0, so registry dependents that accept 0.1.0 would take it; raise the major version (the minor version before 1.0.0) for a breaking change
```

終了コードは 1。最後の `note:` は 2 つの `Tsuzuri.toml` の `version` が同じで breaking があるときの行で、`impact` は出さない（`impact` はバージョンを見ない）。
**git**（`src/fetch.rs`）: `git_command()` は `GIT_*` の環境変数を消し、システムの設定と端末の問い合わせを無効にした `Command`。`failure_line(stderr)` は git の標準エラーの最初の `fatal:`／`error:` の行を返す。どちらも非公開。`git worktree add --detach DIR REV` は DIR に REV の検査済みのコピーを作り、`git worktree remove --force DIR` が消す（付録 A の A-3）。

**テストの選択**: `tsuzuri test --list` は `0 Cart.line total` の形で全テストに番号を振る。`--index N`（繰り返せる）は `TestOptions.indices`、`--filter` は `filter` に入る（`--list`・`--filter`・`--index` は `test` と `bench` だけが受け付ける。`parse_arguments` の検査）。**ソース探索**（`collect_sources`）は `target/` を読むので、一時チェックアウトは `target/` の下に置けない（D6）。

**ID の綴り**: G26 の ID は `Acme::Shop::Cart.total`、`test:Acme::Shop::Cart.line total`、`instance:Add<Main.Point>`。std の関数は graph の `to` にだけ出る（`IO.write_line`・`sqrt`・`Eq.eq`）。

## 変えてはいけない意味

- 既存のコマンドの標準出力・標準エラー・終了コード・`--json` の診断。`api-diff` の出力と終了コード（このチケットは `api_diff.rs` を変えない）。
- KB のファイルの形（G26・G27 のキー・整列・manifest）。照会と `impact` は**読むだけ**で、KB に書くのは G27 の `open` が作り直すときだけ。
- `tsuzuri test` の既定（`--affected` なし）の動き。`--affected` は選択を絞るだけで、テストの実行・結果・終了コードの意味を変えない。
- Phase を足しても、前の Phase のコマンドの出力を変えない。

## Phase 分割

| Phase | 内容 | 規模 | 前提 | 単独で出荷できる理由 |
| --- | --- | --- | --- | --- |
| 1 | `kb brief`・`show`・`callers`・`callees`・`uses`・`tests-for`・`search`、ID の解決、`--budget`、`--json`、`suggest.rs` | M | G27 Phase 2 | 読み取り専用のコマンドが増えるだけ |
| 2 | `kb find`（G25 の実装を呼ぶ） | S | Phase 1・G25 | 1 つの分岐が増えるだけ |
| 3 | `kb impact`（2 つの KB・2 つのプロジェクト・`--git REV`）、分類、影響を受けるテスト | L | Phase 1 | 新しいコマンドだけ。`api-diff` は変わらない |
| 4 | `tsuzuri test --affected` | S | Phase 3 | 新しいフラグだけ |

実装の順序は 1 → 2 → 3 → 4。Phase 2 は G25 が出荷されるまで着手しない（G25 の完了が前提）。

## 仕様

### 1. コマンドと共通オプション（Phase 1）

```text
tsuzuri kb brief    [ID | --project | --module M] [--budget N] [共通]
tsuzuri kb show     ID [--budget N] [共通]
tsuzuri kb callers  ID [--depth N] [--budget N] [共通]
tsuzuri kb callees  ID [--depth N] [--budget N] [共通]
tsuzuri kb uses     TYPE [--budget N] [共通]
tsuzuri kb tests-for ID [--budget N] [共通]
tsuzuri kb search   TEXT [--std] [--budget N] [共通]
tsuzuri kb find     …（Phase 2。G25 の引数）
tsuzuri kb impact   (--git REV | --from PATH [--to PATH]) [共通]   （Phase 3）
共通 = [--path PATH] [--kb-dir DIR] [--no-refresh] [--json]
```

| オプション | 既定 | 意味 |
| --- | --- | --- |
| `--path PATH` | `.` | プロジェクト（`check` と同じ入力。ディレクトリかファイル）。G27 の `kb build [PATH]` と違い、位置引数は ID・TEXT に使うので `--path` で渡す |
| `--kb-dir DIR` | G27 の §1 の置き場 | KB の置き場（`TSUZURI_KB_DIR` も同じ。`kb` は `TSUZURI_KB` を読まない） |
| `--no-refresh` | なし | KB を作り直さず読むだけ。ない・古い・不完全なら `E2009`（G27 の `open` の `Refresh::Never`） |
| `--json` | なし | JSON Lines（§3）。`--budget` は無視する |
| `--budget N` | `brief` は 2,400（L0）・1,600（L1）・700（L2）、`show` は 3,000、一覧は 2,000 | テキストの文字数（`chars().count()`）の上限。100 以上 100,000 以下 |
| `--depth N` | 1 | `callers`・`callees` の深さ。1 以上 8 以下 |
| `--module M` | なし | `brief` でモジュール（ID か key）の L1 を引く |
| `--project` | なし | `brief` でプロジェクトの L0 を引く（既定と同じ） |
| `--std` | なし | `search` で std の KB の行も引く（G27 Phase 4。なければ作る。`--no-refresh` では作らない） |

- **KB の取得**: `Project::load_for_tests(PATH)` → G27 の `open(PATH, kb_dir, refresh, json)`。`Refresh::Auto` で古ければ解析して作り直し、解析の診断は `check` と同じに標準エラーへ出す。型検査が通らないプロジェクトは `E2009` `kb_incomplete`（終了コード 1）。照会の関数（`query.rs`）は `&KbView` を取りディスクに触れない。
- **std の ID**: `show`・`callers`・`callees`・`uses`・`tests-for`・`brief` は、プロジェクトの KB で見つからない ID を std の KB（G27 の `KbView`）でも引く（std の KB が作れないときは引かない）。`graph.jsonl` の `to` にだけ現れる ID（`IO.write_line`・`sqrt`・`Eq.eq`）は、`callers`・`callees` の相手として引ける（`show` は `symbol: not in this KB` の 1 行と `callers` の群だけを出す）。
- **graph・facts が無い KB**（G26 の Phase 2・3 が未出荷）: `callers`・`callees`・`uses`・`tests-for` は見出しと `note: this KB has no graph.jsonl` と `summary: 0 …` を出して 0 で終わる。`show` は無い群を飛ばす。JSON は `{"type":"note","message":"…"}` の行。

引数の拒否（`E2000`、終了コード 2。`<command line>:1:1: error[E2000]: …`。メッセージは英語で固定）:

| 入力 | メッセージ |
| --- | --- |
| 同じオプションの重複 | `{option} specified more than once`（`{option}` は `--budget`・`--depth`・`--path`・`--kb-dir`・`--module`・`--no-refresh`・`--json`・`--project`・`--std` のつづり。`--git`・`--from`・`--to` も同じ） |
| 値のないオプション | `--budget needs a value`（`next_value` の形） |
| `--budget` が範囲外・整数でない | `--budget must be an integer from 100 to 100000` |
| `--depth` が範囲外・整数でない | `--depth must be an integer from 1 to 8` |
| 位置引数の過不足 | `usage: tsuzuri kb show ID [--budget N] [--path PATH] [--kb-dir DIR] [--no-refresh] [--json]`（コマンドごとの使い方） |
| `brief` で `ID`・`--project`・`--module` が 2 つ以上 | `kb brief takes one of ID, --project, or --module` |
| `--depth` を `callers`・`callees` 以外に | `--depth is only valid with kb callers or kb callees` |
| `--std` を `search` 以外に（`build`・`clean` は G27 の分岐が受け付ける） | `--std is only valid with kb search, kb build, or kb clean` |
| `--module`・`--project` を `brief` 以外に | `--module is only valid with kb brief`、`--project is only valid with kb brief` |
| `--budget` を `impact` に | `--budget is not valid with kb impact` |
| `--git`・`--from`・`--to` を `impact` 以外に | `--git is only valid with kb impact`（`--from`・`--to` も同じ形） |
| 未知のオプション | `unknown option 'X'; use --help` |
| `kb` のサブコマンド名の誤り | `unknown kb command 'X'; use build, verify, clean, export, brief, show, callers, callees, uses, tests-for, search, find, or impact`。並べるのは出荷済みのコマンドだけ（`COMMANDS` の定数。Phase 1 の時点では末尾の `find`・`impact` を含めず `…, tests-for, or search`） |

### 2. ID の解決（決定表）

候補の集合 U = `symbols.jsonl` の `id` ∪ `graph.jsonl` の `to`。`--module` は `kind` が `module` の行の ID と、manifest の `modules[].key` を集合にする。`text` に対して**上から順に**調べ、最初に 1 件以上見つかった段で決める。

| 段 | 条件（`id` が…） | 1 件 | 2 件以上 |
| --- | --- | --- | --- |
| 1 | `text` と完全に一致 | 決まる | （一意なので起きない） |
| 2 | `"::" + text` または `"." + text` で終わる（大文字小文字を区別） | 決まる | 曖昧 |
| 3 | `text` と大文字小文字を除いて一致 | 決まる | 曖昧 |
| 4 | 段 2 の大文字小文字を除いた版 | 決まる | 曖昧 |
| 5 | `text` で始まる | 決まる | 曖昧 |
| 6 | 段 5 の大文字小文字を除いた版 | 決まる | 曖昧 |
| — | どの段にも無い | 未知 | — |

- **曖昧**: 解決せず、候補を ID の byte 順に 10 件まで挙げて終了コード 1（`E2000`）。`ambiguous id 't'; candidates: A, B, C`（11 件目以降は `, …N more`）。
- **未知**: `text` の最後の名前（最後の `.` か `::` の後ろ）を小文字にし、U の各 ID の最後の名前（同じ規則）との**編集距離**（OSA。隣接の入れ替えを 1 とする。文字単位）が、`text` の最後の名前の長さが 4 以下なら 1 以下、5 以上なら 2 以下のものを、(距離, ID の byte 順) で 5 件まで: `unknown id 'totl'; did you mean: Acme::Shop::Cart.total`（終了コード 1、`E2000`）。候補が 0 件なら `unknown id 'X'; use tsuzuri kb search TEXT to find ids`。
- `text` が空文字列なら `E2000` `id must not be empty`（終了コード 2）。
- 実例（付録 B-1）: `Line`→`Acme::Shop::Cart.Line`（段 2）、`line total`→`test:Acme::Shop::Cart.line total`（段 2）、`Acme::Shop::Cart.l`→`Acme::Shop::Cart.line_total`（段 5）、`t`→曖昧（段 5 で 3 件）、`totl`→未知（候補 1 件）。

### 3. 出力（Phase 1）

出力はすべて G27 の `Card`（`gap` は `"\n"`）で組む。**見出しの行と `summary` の行は優先度 0、項目の行は優先度 1（`spread` なし。後ろから落とす）**。`--budget` を超えるときは、項目を後ろから落とし、項目の最後に `…N more` を足す。項目が 0 件でも見出しと `summary` は出す。実例は付録 B。

**一覧のコマンド**:

| コマンド | 見出し | 項目 | 末尾 |
| --- | --- | --- | --- |
| `callers`・`callees` | `callers of {ID} (depth {D})`／`callees of …` | `{深さ} {ID}  {path}:{line}:{column}` ＋ 深さ 1 のとき `  args {conv,…}` ＋ 同じ相手への呼び出しが 2 か所以上なら `  +{n} sites` ＋ 深さ 2 以上なら `  via {ID}` | `summary: {n} callers, depth {D}`（1 件は `1 caller`） |
| `uses` | `uses of {ID}` | `{rel} {from}  {path}:{line}:{column}` | `summary: {n} uses` |
| `tests-for` | `tests for {ID}` | `{test の ID}  {path}:{line}:{column}` | `summary: {n} tests` |
| `search` | `search '{TEXT}' (project)`（`--std` で `(project, std)`） | `{連番} {ID}  {kind}  {clip(norm(signature),100)}` | `summary: {n} matches` |

- `callers`・`callees`: `graph.jsonl` の `calls` の行を幅優先でたどる（`callers` は `to == 相手`、`callees` は `from == 相手`）。各 ID は**最小の深さで 1 回**だけ出し、整列は (深さ, ID の byte 順)。位置は、その ID との辺のうち `(span.path, span.start)` が最小のもの。`+n sites` は、その深さでたどった相手との辺の数 − 1。`via` は幅優先の親（開始点に近い側の ID）。`callers` には test（`test:…`）も出る。
- `uses`: `to` が ID の `uses_type`・`implements`・`instance_of` の行（`calls` は含まない）。整列は (`rel`, `from`, `span.start`)。
- `tests-for`: `from` が ID の `tested_by` の行。整列は `to` の byte 順。
- `search`: `name`・`id`・`signature`・`doc` の最初の行を、TEXT（小文字）で、**順位**（1＝`name` が一致、2＝`name` が TEXT で始まる、3＝`name` に含む、4＝`id` に含む、5＝`signature` に含む、6＝`doc` の最初の行に含む）をつけて探し、(順位, ID の byte 順) に並べる。TEXT が空なら `E2000` `search text must not be empty`（2）。`--std` は std の KB の行も加える。0 件は正常（0）。

**`show ID`**（既定の上限 3,000）: 行と優先度は次のとおり（0 は落とさない）。群の項目は ID の byte 順（`callees`・`callers` は (相手の ID, `span.start`)、`used by` は (ID, `rel`, `span.start`)）。`ID` が symbols になく graph の `to` にだけある（`IO.write_line`）ときは、`{ID}`・`symbol: not in this KB` の 2 行（優先度 0）と `callers` の群だけを出す（JSON は `"symbol":null`）。

| 行 | 条件 | 優先度 |
| --- | --- | --- |
| `{ID}` | | 0 |
| `{kind} \| {public または private}[ export][ pure] \| {path}:{line}:{column}` | | 0 |
| `signature: {clip(norm(signature),160)}` | | 0 |
| `doc: {説明の最初の空でない行を clip(norm,100)}` | 説明がある | 1 |
| `params:` と、各引数 `  {名前 または _}  {conv}  {type}`、`result: {result}` | `def`・`extern`・`method`・`const` で `params` が空でない | 1 |
| `fields:`／`cases:`／`methods:` と、各子の `  {signature}`（宣言順） | record／union／class で子がある | 1 |
| `effects: {a, b}`（なければ `none`） | facts がある | 1 |
| `sites: trap {n}, alloc {n}, copy {n}`（`proven: true` は数えず、あれば末尾に ` ({k} proven trap sites not counted)`） | facts がある | 1 |
| `recursion: {self\|self_tail\|mutual}` | `none` でない | 1 |
| `callees ({n}):`・`callers ({n}):`・`tests ({n}):`・`used by ({n}):` | その辺がある | 2 |
| 各項目 `  {ID}  {path}:{line}:{column}`（`callees`・`callers` は ` args {conv,…}`、`used by` は `{ID}  {rel}  …`） | | 3 |

**`brief`**: 引数なし・`--project` は L0、`--module M` は L1、`ID` は L2（ID がモジュールなら L1）。`--budget` なしなら**保存したカードのファイルと同じ byte**（ファイルがなければ G27 の `l0`・`l1`・`l2` を同じ規則で作る。L2 は `module`・`test`・`instance`・`field`・`case` の行にも作れる）。`--budget N` ありなら、上限を N にして `l0`・`l1`・`l2` で作り直す。

**`--json`**（1 行 1 オブジェクト。キーの順は固定）:

| コマンド | 行 |
| --- | --- |
| `brief` | `{"type":"brief","level":"L0"\|"L1"\|"L2","id":ID または null,"text":"…"}` |
| `show` | `{"type":"symbol","symbol":{symbols の行},"facts":{facts の行} または null,"callees":[{graph の行}],"callers":[…],"tests":[…],"used_by":[…]}` |
| `callers`・`callees` | 項目ごとに `{"type":"edge","depth":n,"id":…,"via":… または null,"sites":n,"span":{…},"args":[…] または null}`、最後に `{"type":"summary","count":n,"depth":D}` |
| `uses` | `{"type":"use","rel":…,"from":…,"span":{…}}`、最後に `{"type":"summary","count":n}` |
| `tests-for` | `{"type":"test","id":…,"span":{…}}`、最後に `{"type":"summary","count":n}` |
| `search` | `{"type":"match","rank":n,"id":…,"kind":…,"signature":…}`、最後に `{"type":"summary","count":n}` |
| KB に無い部分 | `{"type":"note","message":"this KB has no graph.jsonl"}` |

**照会が使う行のキー**（G26 の「仕様」と同じ。違うなら止める: 停止条件 2）: symbols = `id`・`kind`・`module`・`name`・`signature`・`params`（`name`・`type`・`conv`）・`result`・`public`・`export`・`pure`・`doc`・`deprecated`・`span`（`path`・`start`・`line`・`column`）。graph = `rel`・`from`・`to`・`args`・`span`。tests = `id`・`index`・`span`。facts = `id`・`effects`・`trap_sites`・`alloc_sites`・`copy_sites`（`proven`）・`recursion`。manifest = `schema`・`compiler`・`source_hash`・`counts.warnings`・`modules`（`name`・`key`・`source`）。diag = `severity`・`code`・`message`・`path`・`span.line`。

### 4. `kb find`（Phase 2）

`tsuzuri kb find …` は `tsuzuri find …` と同じ（引数・出力・終了コード・使い方の誤りの `E2000` の文言）。例（G25 の例 6。`tsuzuri kb find 'ref [i64] -> i64'` の先頭 4 行。G25 が出荷するまで HEAD にない試作の出力）:

```text
exact   Array.length :: ref ['a] -> i64
exact   Array.product :: Numeric<'a> => ref ['a] -> 'a
exact   Array.sum :: Numeric<'a> => ref ['a] -> 'a
exact   Parallel.sum :: Numeric<'a>, Add<'a>, Copy<'a>, Send<'a> => ref ['a] -> 'a
```

`main` の `kb` の分岐が、2 語目が `find` のときだけ `find_command(&raw[2..])`（G25 が `src/main.rs` に足す `fn find_command(arguments: &[OsString]) -> ExitCode`）を呼ぶ。`--path`・`--kb-dir`・`--no-refresh`・`--budget`・`--json` は渡らない（`find` の `unknown option` の `E2000`）。KB を読まず、`open` も呼ばない（G25 の `find` は std と `--project` を自分で解析して署名の表を作る）。G25 が `done` でない間は Phase 2 を出荷せず、`kb find` は §1 の「サブコマンド名の誤り」になる。

### 5. `kb impact`（Phase 3）

**入力**（決定表。上から最初の行）:

| 指定 | base | head |
| --- | --- | --- |
| `--git REV` | REV のチェックアウト（リポジトリの中の同じ相対位置） | `--path`（既定 `.`）の作業ツリー |
| `--from A` | A | `--path` |
| `--from A --to B` | A | B |
| `--git` と `--from`・`--to` の併用、`--to` だけ、どれもなし | `E2000`（終了コード 2）`usage: tsuzuri kb impact (--git REV \| --from PATH [--to PATH]) [--path PATH] [--kb-dir DIR] [--no-refresh] [--json]` |

A・B はディレクトリ。`.tsuzuri-kb` があれば KB としてそのまま読む（作り直さない）。なければプロジェクトとして G27 の `open`（`Refresh::Auto`。`--no-refresh` なら既存の KB を読むだけ）で KB を得る。`--kb-dir` は head にだけ効く。読めないなら `E2001` `cannot read '{path}'`。**ラベル**: base は `--git` なら `git {コミットの先頭 12 桁}`、`--from A` なら `A`（与えられたとおり）。head は `--to B` なら `B`（与えられたとおり）、それ以外（`--path` のプロジェクト）は `working tree`。`impact` は 2 つの `KbData` から `impact::diff(&KbData, &KbData, &str, &str) -> Impact` を呼ぶだけで、ディスクに触れない（G29 が使う）。

**git の扱い**（`src/kb/git.rs`。`fetch::git_command`・`failure_line` を `pub(crate)` にして使う）:

1. REV が空、または `-` で始まるなら `E2000` `invalid git revision '{rev}'`（2）。
2. `git -C {root} rev-parse --show-toplevel`（`root` は `Project::root_directory()`）。失敗なら `E2001` `'{root}' is not inside a git repository`。
3. `git -C {top} rev-parse --verify --quiet --end-of-options {REV}^{commit}`。標準出力の 40 桁が SHA。失敗（終了コード 1・出力なし）なら `E2000` `unknown git revision '{rev}'`（1）。
4. `TemporaryDirectory::new(&std::env::temp_dir())`（`.tsuzuri-<pid>-<n>`）を作り、その中に空のディレクトリ `no-hooks` を作る。`git -c core.hooksPath={temporary}/no-hooks -C {top} worktree add --detach {temporary}/checkout {SHA}` を実行する（`git worktree add` は `post-checkout` フックを走らせるので、空のフックの置き場に向ける。付録 A-3 の実測）。git に渡す `{temporary}` は `cache::real_path` を通した値にする（Windows の verbatim パス `\\?\` を git が読めないため。`fetch.rs` の `Repository::create` と同じ）。標準出力・標準エラーは捨て、失敗は `E2001` `cannot run git: {failure_line}`。`git worktree prune` は呼ばない（今は見えない場所、たとえば外した外付けディスクにある他の worktree の管理情報まで消すため）。
5. base のルートは `{temporary}/checkout/{top から root への相対パス}`。なければ `E2001` `the project directory '{rel}' does not exist at git revision '{rev}' ({sha12})`。ここで G27 の `open` を呼ぶ（KB は一時ディレクトリの中にできる）。
6. **ガード**（`Drop`）: `git -C {top} worktree remove --force {temporary}/checkout` を実行し、続けて `TemporaryDirectory` が残りを消す。正常終了・エラー・`panic` のどれでも、`impact` の戻りの前に走る。Ctrl-C（SIGINT）と `SIGKILL` では `Drop` が走らず、`{temporary}` と git の登録（`git worktree list` に `prunable` と出る）が残る。`unsafe` なしにシグナルを捕まえられないので許容し、利用者の `git worktree prune` に任せる（残った登録は次の `git worktree add` を妨げない。付録 A-3）。
7. **一時チェックアウトを `target/` の下に置かない**: `collect_sources` が `target/` の中の `.tz` を読み、現在のプロジェクトに混ざる（D6）。

**公開 API ではない行の扱い**: `impact` は非公開の変更も出す（`non-breaking`）。`api-diff` は公開の宣言だけを見る。

**分類の決定表（symbols。`kind` が `module` の行は比べない。行は `id` で対応づける）**:

| # | 条件 | `action kind` | 分類 | note |
| --- | --- | --- | --- | --- |
| 1 | base にあり head にない。member（`field`・`case`・`method`）で親（`id` の最後の `.` の前）も head にない | 出さない（親の削除に含む） | — | — |
| 2 | 同上で `public` が真 | `removed {kind}` | breaking | `field`＝`uses of the field stop compiling`、`case`＝`uses of the case stop compiling`、`method`＝`calls and instances of the method stop compiling`、ほか空 |
| 3 | 同上で `public` が偽（test・bench・非公開） | `removed {kind}` | non-breaking | 空 |
| 4 | head にあり base にない。member で親も base にない | 出さない（親の追加に含む） | — | — |
| 5 | 同上で `field`・`case`・class の `method`（親の `kind` が `class`）で `public` が真 | `added {kind}` | breaking | `field`＝`constructions of the record without it stop compiling`、`case`＝`exhaustive matches over the union stop compiling`、`method`＝`existing instances do not define the method` |
| 6 | 同上でそれ以外（instance の `method`、非公開、ほかの kind） | `added {kind}` | non-breaking | 空 |
| 7 | 両方にあり、型変数を出現順に `'t0`・`'t1`… に直した `signature` が違う | `changed {kind}`（`old`・`new` は `signature`） | base の `public` が真なら breaking、偽なら non-breaking | 空 |
| 8 | `signature` が同じで、`params` の `conv` の列が違う | `changed conv`（`old`・`new` は `conv` を `,` でつないだもの） | 7 と同じ | `ownership passing of an argument changed` |
| 9 | `public` が真→偽／偽→真 | `changed visibility`（`public`・`private`） | breaking／non-breaking | 空 |
| 10 | `export` が真→偽／偽→真 | `changed export`（`true`・`false`） | breaking／non-breaking | 空 |
| 11 | `pure` が真→偽／偽→真 | `changed pure` | breaking／non-breaking | 空 |
| 12 | `deprecated` が `null`→値／値→`null` | `deprecated {kind}`／`undeprecated {kind}`（`new`／`old` は注記。空なら省く） | non-breaking | 空 |
| 13 | `doc` が違い、`deprecated` の有無は同じ | `documented {kind}` | non-breaking | 空 |
| 14 | record・union の field・case の並び（共通の名前だけ。`span.start` の順）が違う | `changed order`（`old`・`new` は名前を `, ` でつないだもの） | base の `public` が真なら breaking、偽なら non-breaking | `the order of fields and cases is part of the layout and of derived comparisons` |
| 15 | `implements` の辺（graph）が base にあり head にない。`from` が head の公開の行 | `removed implements`（名前は `{from}.{to}`） | breaking | 空 |
| 16 | `implements` の辺が head にあり base にない | `added implements` | non-breaking | 空 |

`signature` の比較は `norm`（空白の連続を 1 つにする）した文字列で、型変数の名前の付け替え（`'a`→`'b`）は違いに数えない。`message` は `api-diff` の `Change::message` と同じ形（`{action} {kind} {name}: \`{old}\` => \`{new}\` ({note})`。`old`・`new`・`note` がなければその部分を省く）。

**分類の決定表（facts。関数的な行＝ `kind` が `def`・`method`・`extern`。両方の KB に facts があり、`manifest.compiler` が同じで、両方に行がある ID だけ）**:

| 条件 | `action kind` | 分類 | note |
| --- | --- | --- | --- |
| head の `effects` にあって base にない効果 `e`、`public` が真 | `added effect {e} on {ID}` | breaking | `reached by {n} functions and {m} tests`（head の `calls` を逆向きに深さ 8 までたどって `{ID}` に届く関数の数と test の数。1 のときは `function`・`test`。両方 0 なら note なし） |
| 同上で `public` が偽 | 同上 | info | 同上 |
| base の `effects` にあって head にない効果 | `removed effect {e} on {ID}` | `public` が真なら non-breaking、偽なら info | 空 |
| `trap_sites`・`alloc_sites`・`copy_sites` の `proven: false` の件数が違う | `increased {key} {ID}`／`decreased …`（`old`・`new` は件数） | info | 空 |
| `recursion` が違う | `changed recursion {ID}`（`old`・`new` は値） | info | 空 |

`manifest.compiler` が違う（または一方に facts がない）ときは facts を比べず、`info` の 1 行（`action` が `skipped`、`kind` が `facts`、`name` は空、`note` が `the two KBs were built by different compilers or one has no facts`。テキストは `info: skipped facts (the two KBs were built by different compilers or one has no facts)`）を足す。test・bench の行は比べない。

**警告**: 両方の `diag.jsonl` の `severity` が `warning` の行を `(code, message, path)` の多重集合で比べる（行番号は動くので比べない）。head に増えた分は `new warning {code} at {path}:{line}: {message}`、減った分は `fixed warning …`（`info`）。

**影響を受けるテスト**（決定表。上から最初の行。n＝head の `kind` が `test` の行数）:

| 条件 | 出力 |
| --- | --- |
| head に `graph.jsonl` がない | `tests: all {n} (the KB has no graph)` |
| base と head で `manifest.compiler` か `schema` が違う | `tests: all {n} (the compiler changed)` |
| `source_hash` が同じ | `tests: 0 of {n} affected` |
| `source_hash` が違うが、`modules[].source` に差のあるモジュール（片方にだけあるものを含む）が 1 つもない | `tests: all {n} (the manifest or a dependency changed)` |
| 変更モジュールに、プロジェクトが宣言していない class（例: std の `Add`・`Eq`・`Ord`）の `instance` の行が base か head にある（`graph` の `instance_of` の `to` が symbols にない。辺がなければ該当） | `tests: all {n} (an instance of a std class changed)` |
| 上のどれでもなく、逆引きを深さ 8 まで広げた後の次の 1 段にまだ未訪問の呼び出し元がある | `tests: all {n} (callers deeper than 8 calls)` |
| それ以外 | `tests: {k} of {n} affected` と、集合の各 ID を byte 順に 2 スペースのインデントで 1 行ずつ |

集合 = (a) 変更モジュールにある head の行（`module` 以外）から、head の `calls` を逆向きに深さ 8 までたどって届く test。(b) 変更モジュールにある head の test。(c) base の `calls` で、head から消えた ID を呼んでいた test のうち head にあるもの。逆引きには、「`calls` の `to` が `C.m` で、`instance_of` の `to` が `C` の instance `I` があれば `I.m` も呼ばれる」という辺を足す（クラスのメソッドの呼び出しは、その class のすべての instance のメソッドに届く）。変更モジュール = `modules[].source` が違う、または片方にしかない `name`。

**出力**（テキスト。実例は付録 C）:

```text
impact: {base のラベル} -> {head のラベル}
{severity}: {message}          ← 1 変更 1 行。breaking → non-breaking → info。各群は (name, kind, action, old, new) の順（下記）
note: {N} more changes not shown   ← 1,000 件（`MAX_LISTED_CHANGES`）を超えたとき
tests: {k} of {n} affected     ← または tests: all {n} ({理由})
  {test の ID}                   ← 集合の各 ID
summary: {B} breaking, {N} non-breaking, {I} info
```

effect・site・警告の `message`: `added effect io on {ID} (reached by 3 functions and 2 tests)`、`increased trap_sites {ID}: \`1\` => \`2\``、`changed recursion {ID}: \`none\` => \`self\``、`new warning W1001 at Cart.tz:22: unused local 'rate'; …`。**`--json`**: 1 行 1 オブジェクト。`{"type":"impact","base":…,"head":…}`、変更ごとに `{"type":"change","severity":"breaking"|"non-breaking"|"info","action":…,"kind":…,"name":…,"old":…,"new":…,"note":…}`（`api-diff` と同じキー。`name` は ID、effect は `{ID}`、警告は `code`）、`{"type":"tests","mode":"some"|"all","affected":[…],"total":n,"reason":null|"…"}`（`all` のとき `affected` は `[]`）、最後に `{"type":"summary","breaking":B,"non_breaking":N,"info":I,"omitted":O}`。**終了コードは 0**（エラーのときだけ 1・2）。

**`api-diff` との意図した差**: (1) `info` の区分と facts・警告・非公開の変更、(2) 終了コードが常に 0、(3) `derive`・`namespace`・`const` は G26 の行に無いので行が出ない（`implements` と `export`・`pure` の行で代わる）、(4) class のメソッドの `default` は G26 の行に無いので、class へのメソッドの追加は常に breaking、(5) `note` に所有権の説明を足さない（`api-diff` と同じ文字列にするため。`conv` だけが違う行 8 は `api-diff` に対応する行がなく、`note` を持つ）、(6) `public` の変化は `api-diff` では `removed`／`added` だが、`impact` は `changed visibility`（行 9）。`public_changes_equal_api_diff`（手順 10）は、`kb_shop`→`kb_shop_head` で `api-diff` の `breaking`・`non-breaking` の全行が `impact` の同じ区分の行に**文字列まで同じ**で含まれ、`impact` だけの行が facts・非公開・警告のものだけであることを確かめる。

### 6. `tsuzuri test --affected`（Phase 4）

`tsuzuri test [PATH] --affected VALUE`（`test` だけが受け付ける。ほかのコマンドでは `--affected is only valid with test`、2）。

| 状況 | 結果 |
| --- | --- |
| `--filter` か `--index` と同時 | `E2000` `--affected cannot be combined with --filter or --index`（2） |
| `--no-kb` か `TSUZURI_KB=0` と同時 | `E2000` `--affected needs the KB; remove --no-kb or TSUZURI_KB=0`（2） |
| `--list` と同時 | 選んだテストだけを番号つきで出す（実行しない） |

- **VALUE の読み方**（上から）: 存在するディレクトリで `.tsuzuri-kb` がある→KB。存在するディレクトリ→プロジェクト。それ以外→git のリビジョン（`./name` でパス、`refs/heads/name` でリビジョンを強制できる）。base＝VALUE、head＝`PATH` のプロジェクトの KB（`open`。`Refresh::Auto`）。
- `impact::diff(base, head).tests` を使い、選ばれた ID を head の `tests.jsonl` の行で引いて `index` の昇順にし、`TestOptions.indices` に入れる（`tests.jsonl` に無い ID は無視する）。
- **出力**: 実行の前に標準出力へ 1 行 `affected: {k} of {n} tests ({base のラベル})` または `affected: all {n} tests ({理由})`。`--json` は `{"type":"affected","mode":"some"|"all","affected":k,"total":n,"base":"…","reason":null|"…"}`（`all` のとき `k` は `n`）。**k が 0 なら何も実行せず終了コード 0**。それ以外は `--index` で選んだときと同じ実行・結果・終了コード。
- **実装の場所**（`src/main.rs`）: `Arguments.affected: Option<String>`。`parse_arguments` の検査は `--filter`・`--list`・`--index` の検査（`!matches!(action, Action::Test | Action::Bench) && …` の `if`）の隣に置く: `test` 以外は `--affected is only valid with test`、`--filter`・`--index` との併用と `--no-kb` との併用は表のとおり。`TSUZURI_KB=0` との併用は `kb::settings::resolve` の後に `main` が検査する。`run_test_action` が `options` を作った直後に `select_affected` を呼び、`affected:` の行を出し、`reason` がなければ `options.indices` に `indices` を入れる（`--list` は `options.includes` で選んだテストだけを出す）。`indices` が空なら `ExitCode::SUCCESS` を返す。範囲外の番号の検査（`test index is out of range`）はその後に走る。

### 7. エラーと終了コード

| 状況 | コード | 終了 | メッセージ |
| --- | --- | --- | --- |
| 曖昧な ID・未知の ID | `E2000` | 1 | §2 |
| KB がない・古い（`--no-refresh`）・壊れている・不完全 | `E2009` | 1 | G27 の付録 I-1 |
| git のリビジョンが不正 | `E2000` | 2 | `invalid git revision '{rev}'` |
| 未知のリビジョン | `E2000` | 1 | `unknown git revision '{rev}'` |
| リポジトリの外 | `E2001` | 1 | `'{root}' is not inside a git repository` |
| git が実行できない・worktree の作成に失敗 | `E2001` | 1 | `cannot run git: {failure_line}` |
| リビジョンにプロジェクトのディレクトリがない | `E2001` | 1 | `the project directory '{rel}' does not exist at git revision '{rev}' ({sha12})` |
| `--from`・`--to` が読めない | `E2001` | 1 | `cannot read '{path}'` |
| 使い方の誤り | `E2000` | 2 | §1 |

## 設計

### データ構造と関数の骨格（名前・型を変えない。型の全フィールドは付録 E）

```rust
// src/kb/suggest.rs（新規）
pub fn osa_distance(a: &str, b: &str) -> usize;                       // 隣接の入れ替えを 1 とする編集距離（文字単位）
pub fn suggestions(names: &[(String, String)], text: &str) -> Vec<String>;   // (ID, 最後の名前)。§2 の未知の規則。5 件まで
// src/kb/query.rs（新規。ディスクに触れない）
pub enum Resolution { One(String), Many(Vec<String>), Unknown(Vec<String>) }   // Many は 10 件まで、Unknown は 5 件まで
pub fn resolve(view: &KbView, text: &str, modules: bool) -> Resolution;
pub struct Options { pub budget: Option<usize>, pub depth: usize }
pub enum BriefTarget { Project, Module(String), Id(String) }
pub struct Output { pub text: String, pub rows: Vec<String> }            // rows＝JSON Lines の各行（改行なし）
pub enum QueryError { Ambiguous { text: String, candidates: Vec<String>, more: usize }, Unknown { text: String, suggestions: Vec<String> }, Empty(&'static str) }
impl QueryError { pub fn diagnostic(&self) -> Diagnostic; }               // E2000。Ambiguous・Unknown は終了コード 1
pub fn brief(view: &KbView, target: &BriefTarget, options: &Options) -> Result<Output, QueryError>;
pub fn show(view: &KbView, id: &str, options: &Options) -> Result<Output, QueryError>;
pub fn callers(view: &KbView, id: &str, options: &Options) -> Result<Output, QueryError>;   // callees・uses・tests_for も同じ形
pub fn search(view: &KbView, text: &str, std: bool, options: &Options) -> Result<Output, QueryError>;
// src/kb/impact.rs（新規。ディスクに触れない）
pub enum Severity { Breaking, NonBreaking, Info }
pub struct Change { pub severity: Severity, pub action: &'static str, pub kind: String, pub name: String, pub old: Option<String>, pub new: Option<String>, pub note: String }
pub enum Tests { Some(Vec<String>), All(String) }                          // All の中身は理由
pub struct Impact { pub base: String, pub head: String, pub changes: Vec<Change>, pub tests: Tests, pub total_tests: usize }
pub fn diff(base: &KbData, head: &KbData, base_label: &str, head_label: &str) -> Impact;
impl Impact { pub fn render_text(&self) -> String; pub fn render_json(&self) -> String; pub fn counts(&self) -> (usize, usize, usize); }
// src/kb/git.rs（新規）
pub struct Checkout { /* 一時ディレクトリ、リポジトリの top、チェックアウトのパス、SHA */ }
impl Checkout { pub fn create(root: &Path, rev: &str) -> Result<Self, Diagnostic>; pub fn project_root(&self) -> &Path; pub fn sha12(&self) -> &str; }
impl Drop for Checkout { fn drop(&mut self); }                             // worktree remove --force、続けて一時ディレクトリを消す
```

G27 の `store.rs` に足す読み取り口（G29 も使う）: `KbData::symbol(&self, id) -> Option<&Symbol>`、`edges_from(&self, rel, id) -> Vec<&Edge>`・`edges_to`、`children(&self, parent_id, kind) -> Vec<&Symbol>`（G27 の「宣言順の子」）、`all_ids(&self) -> Vec<&str>`（symbols の `id` ∪ graph の `to`。byte 順）、`test_index(&self, id) -> Option<usize>`（`tests.jsonl` の `index`）、`module_key(&self, id) -> Option<&str>`（manifest の `modules`）。索引は `open` のときに `BTreeMap` で 1 回作る。`KbView` は同名のメソッドでプロジェクトを先に、std を後に引く。

### アルゴリズム

- **`resolve`**: §2 の表を `all_ids()` に対して上から。段 2〜6 は集合を `BTreeSet` に取って byte 順。未知は `suggest::suggestions`。
- **幅優先**（`callers`・`callees`）: `visited` に開始点を入れ、深さ 1〜D で、前の深さの ID を byte 順に取り、辺を `(相手, span.path, span.start)` の順に見て、未訪問の相手を `(深さ, 相手, 最初の辺, 辺の数)` で記録する。`via` は前の深さの ID（深さ 1 は `None`）。
- **`show` の行**: §3 の表の順に `Vec<(String, u8)>` を作り、`Card::new(limit, "\n")` に 1 ブロックで渡す（G27 の §7 の規則。`spread` なし）。
- **`canon`**（`impact`）: `norm` した署名を左から読み、`'` に英字・数字・`_` が続く並びを、現れた順に `'t0`・`'t1`… に置き換える（同じ名前は同じ番号）。正規表現の crate は使わない。
- **`diff`**: ① 行を `id` で `BTreeMap` に入れる。② §5 の決定表を symbols → facts → 警告の順に適用して `Change` を作る。③ `(区分, name, kind, action, old, new)` で整列する（`api-diff` の `compare` の末尾と同じ。区分は breaking → non-breaking → info、`name`・`kind`・`old`・`new` は byte 順で `None` が先、`action` は `removed`・`added`・`changed`・`deprecated`・`undeprecated`・`documented`・`increased`・`decreased`・`new`・`fixed`・`skipped` の順）。④ テストの集合を §5 の表で求める。
- **`Checkout::create`**: §5 の「git の扱い」の 1〜4。`Drop` は 6。`git` は `fetch::git_command()` を使い、`-C {top}` を付け、引数は `Command::arg`（シェルを通さない）。

### 段ごとの変更表

| ファイル | 関数・型 | 変更 |
| --- | --- | --- |
| `src/kb/mod.rs`（G26・G27） | `mod` 宣言 | Phase 1: `query`・`suggest`。Phase 3: `impact`・`git` |
| `src/kb/store.rs`（G27） | `KbData`・`KbView`・`Meta` | 上の読み取り口と索引。`Meta` に `schema`・`compiler`（manifest の同名キー。`impact` が使う）を足す |
| `src/kb/command.rs`（G27） | `kb` の分岐 | `brief`・`show`・`callers`・`callees`・`uses`・`tests-for`・`search`（Phase 1）、`find`（Phase 2）、`impact`（Phase 3）。引数の解析は G27 の `build`・`verify`・`clean`・`export` と同じ関数に並べる |
| `src/main.rs` | `kb` の使い方（`HELP`）、`Arguments` | `HELP` に 9 コマンドと新しいオプション。Phase 4: `Arguments.affected: Option<String>`、`parse_arguments` の `--affected`（`test` だけ）とその検査、`run_tests` の前の選択 |
| `src/fetch.rs` | `git_command`・`failure_line` | `pub(crate)` にする（本体は変えない） |
| `src/test_runner.rs` | `TestOptions` | 変えない（`indices` に入れる） |
| `tests/fixtures/kb_shop_head/`（Phase 3） | 新規 | 付録 A-2 のソース |

## 前提とする他チケットのインターフェース

| 相手 | 使うもの | 未実装・違うときの代替 |
| --- | --- | --- |
| G27 Phase 2 | `store.rs` の `KbData`・`KbView`、`open.rs` の `open(path, kb_dir, refresh, json)`・`Refresh`、`brief.rs` の `Card`・`l0`・`l1`・`l2`、`command.rs` の `kb` の分岐、`write.rs` の `stem`・`unique_names`、`driver.rs` の `Project::root_directory`（G27 の Phase 1 が `pub(crate)` にする） | 停止条件 1 |
| G27 Phase 4 | `KbView` の std（`StdSource`）。`search --std` と std の ID の引き | 無ければ std の ID を引かず、`--std` は `E2000` `std KB is not available` |
| G26 Phase 1・2 | symbols・graph の行（キーは §3 の末尾） | graph が無ければ「graph が無い KB」の規則（§1） |
| G26 Phase 3〜5 | facts の `effects`・`*_sites`・`recursion` | 無ければ `show` の facts の行と `impact` の facts の比較を飛ばす |
| G26 に任意で頼むこと | class の `method` の行に `default`（bool） | 無ければ class へのメソッドの追加は常に breaking（§5 の 5）。足されたら 5 を直す |
| G25 Phase 3 | `src/main.rs` の `find_command`（`tsuzuri find` の入口。`src/find.rs` の `parse_arguments`・`run`・`render` を呼ぶ） | Phase 2 は着手しない（`kb find` は「サブコマンド名の誤り」） |
| G29 | `query::*`・`impact::diff` を MCP が呼ぶ | 名前は G29 の「前提とする他チケットのインターフェース」と同じ |

## 実装手順

### 手順 1: ベースラインと `store.rs` の読み取り口（Phase 1）

- 内容: GUIDE §2.3 の基準（`cargo build --release --locked && cargo test --locked` の件数）。`tests/fixtures/kb_shop` を `check` して KB ができることを確かめる。`src/kb/store.rs` に「データ構造」の読み取り口を足し、単体テスト `store_indexes_find_edges_children_and_ids`（付録 B-0 の KB で、`edges_to("calls", "Acme::Shop::Cart.line_total")` が 2 件、`children("Acme::Shop::Cart.Line", "field")` が 3 件、`all_ids()` に `IO.write_line` がある）。
- 確認: `cargo test --locked kb::store`。既存の件数は変わらない。

### 手順 2: テスト先行（Phase 1）

- 内容: `tests/kb_query.rs`（新規）に、付録 B の出力を期待値にしたテストを `#[ignore = "手順 7 で有効にする"]` で書く。テスト計画の Phase 1 の表のうち `show_matches_the_appendix_output` を最初に。CLI のテストは `env!("CARGO_BIN_EXE_tsuzuri")` を呼び、`kb_shop` を一時ルートへ写す（手本 `tests/api_diff.rs` の `root`・`tsuzuri`）。
- 確認: `cargo test --locked --test kb_query -- --ignored` が失敗する。通常の `cargo test --locked` は緑。

### 手順 3: `suggest.rs`（Phase 1）

- 内容: `osa_distance`（動的計画法。`chars()` の配列で）と `suggestions`（§2 の未知の規則）。単体テスト: `osa_counts_a_swap_as_one`（`tota` と `ttoa`）、`suggestions_use_the_last_name_and_the_length_rule`（`totl`→`Acme::Shop::Cart.total`、名前が 4 字以下で距離 2 は出ない）。
- 確認: `cargo test --locked kb::suggest`。

### 手順 4: `resolve` とエラー（Phase 1）

- 内容: `query.rs` の `Resolution`・`resolve`・`QueryError::diagnostic`（`E2000` `ambiguous id '{id}'; candidates: …`・`unknown id …`・`id must not be empty`）。単体テスト: §2 の実例の 5 つと `module` の解決（`Cart`→`Acme::Shop::Cart`、key `Cart`）。
- 確認: 付録 B-1 の表と一致。

### 手順 5: 一覧のコマンド（Phase 1）

- 内容: `callers`・`callees`（幅優先）・`uses`・`tests_for`・`search`（順位 1〜6）。`Card` に `gap = "\n"` で渡し、`--json` の `rows` も作る。graph が無い KB の `note`。
- 確認: 付録 B の `callers`・`callees`・`uses`・`tests-for`・`search` のテキストと JSON が byte 一致。`budget_cuts_items_from_the_end`（`--budget 160` で `…N more`、見出しと `summary` は残る）。

### 手順 6: `show` と `brief`（Phase 1）

- 内容: `show` の行（§3 の表）、`brief`（保存したカードを返す。`--budget` ありは `l0`・`l1`・`l2` で作り直す）。
- 確認: 付録 B の `show` と `brief` が byte 一致。`brief_without_budget_equals_the_card_file`（`brief/L0.md` と同じ byte）。

### 手順 7: CLI（Phase 1）

- 内容: `command.rs` と `main.rs` に §1 の引数の解析・検査・使い方・`HELP` を足し、手順 2 のテストの `#[ignore]` を外す。`rejects_ambiguous_or_unused_arguments` に拒否の行を足す。
- 確認: `cargo test --locked`（`kb_query` の全テストが緑）。`cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings`。

### 手順 8: 文書（Phase 1）

- 内容: 「ドキュメント」の Phase 1。`node scripts/check-docs.mjs <変更したページ>`。

### 手順 9: `kb find`（Phase 2。G25 が `done` のとき）

- 内容: `main.rs` の `kb` の分岐に、2 語目が `find` のとき `find_command(&raw[2..])` を返す 1 行を足し、`command.rs` の `COMMANDS` に `find` を足す（`HELP` と使い方の文言も）。テスト: `kb_find_prints_what_find_prints`（同じ引数で `tsuzuri find` と `tsuzuri kb find` の標準出力・終了コードが同じ。引数は G25 の例 6 の `'ref [i64] -> i64'`）。

### 手順 10: テスト先行（Phase 3）

- 内容: `tests/fixtures/kb_shop_head/`（付録 A-2）を作り、`fmt --check`・`check`（警告 2 件）・`test`（3 件通る）を確かめる。`tests/kb_impact.rs`（新規）に、付録 C の出力を期待値にしたテストを `#[ignore = "手順 14 で有効にする"]` で書く。最初に `public_changes_equal_api_diff`。
- 確認: `cargo test --locked` が緑（新規は ignored）。

### 手順 11: `impact.rs` の symbols（Phase 3）

- 内容: `canon`、§5 の symbols の決定表（1〜16）、`Change` の `message`、整列、`render_text`・`render_json`。
- 確認: 単体テスト `canon_ignores_type_variable_names`、`each_row_of_the_symbols_table`（付録 F-1 の 31 行を、小さな手書きの 2 つの `KbData` で）。

### 手順 12: facts・警告・テスト（Phase 3）

- 内容: facts の決定表、警告、影響を受けるテスト（逆引きと instance への辺）。
- 確認: 単体テスト `effects_added_to_a_public_function_are_breaking`・`private_effects_are_info`（付録 F-2 の X1〜X9b）、`tests_table_rows`（付録 F-3 の T1〜T11）、`dispatch_reaches_instance_methods`（T7）。

### 手順 13: `git.rs`（Phase 3）

- 内容: `Checkout`（§5 の git の扱い）。`fetch.rs` の `git_command`・`failure_line` を `pub(crate)` にする。
- 確認: `tests/kb_impact.rs` の `worktree_is_removed_on_success_and_on_failure`、`git_status_is_unchanged`（付録 F）。

### 手順 14: CLI（Phase 3）

- 内容: `kb impact` の引数・入力の決定表・`--git`。手順 10 のテストの `#[ignore]` を外す。
- 確認: `cargo test --locked`、`cargo fmt`・`clippy`。

### 手順 15: 文書（Phase 3）

- 内容: 「ドキュメント」の Phase 3。`impact` の実例（付録 C）。

### 手順 16: `test --affected` と記録（Phase 4）

- 内容: `parse_arguments` の `--affected`、VALUE の読み方、選択、出力。`tests/kb_affected.rs`（新規）。文書。`docs/benchmarks.md` の節 `## KB の照会と impact（G28）`（`kb show`・`kb impact` の時間。付録 F の手順）。GUIDE の D-55 の台帳（新しいサブコマンド・オプション）。
- 確認: `affected_tests_cover_every_failing_test`（差分テスト。付録 F）、`affected_with_no_changes_runs_nothing`、`affected_conflicts_are_rejected`。

## テスト計画

期待値は付録の手書きの出力（Python の参照実装 D が出したもの）と、テスト内の独立した計算から作る。CLI のテストは `env!("CARGO_BIN_EXE_tsuzuri")` を呼び、プロジェクトは 1 つずつ別の一時ルートに置く。git のテストは一時ディレクトリに `git init` し（`user.name`・`user.email` を `-c` で与える）、リポジトリの外の `git` の設定に依らない。拒否のテストは診断コードを照合する。入力・期待の詳細は付録 F。

- **`tests/kb_query.rs`（Phase 1）**: `show_matches_the_appendix_output`、`callers_and_callees_with_depth`、`uses_tests_for_and_search_match_the_appendix`、`ambiguous_unknown_and_empty_ids`（§2 の実例。終了コードと `E2000`）、`brief_project_module_and_symbol`、`brief_without_budget_equals_the_card_file`、`budget_cuts_items_from_the_end`、`json_rows_match_the_appendix`、`queries_read_a_kb_without_graph_and_facts`（graph・facts を消した KB で `note`）、`no_refresh_reports_e2009`（KB なし・古い）、`a_stale_kb_is_rebuilt_before_the_query`、`output_is_deterministic`（2 回・別ルートで同じ byte）、`std_ids_resolve_when_the_std_kb_exists`（Phase 4 の G27 の後）。
- **`tests/kb_impact.rs`（Phase 3）**: `public_changes_equal_api_diff`、`impact_text_and_json_match_the_appendix`（付録 C の場面 A〜F。G は `changes_are_cut_at_1000`）、`impact_from_two_kb_directories_and_two_projects`、`impact_exit_code_is_zero_with_breaking_changes`、`impact_options_are_validated`（§5 の表の拒否）、`worktree_is_removed_on_success_and_on_failure`、`git_status_is_unchanged`、`hooks_do_not_run`、`unknown_revision_and_not_a_repository`、`project_directory_missing_at_revision`、`changes_are_cut_at_1000`（`note: N more changes not shown`）、`facts_are_not_compared_across_compilers`。
- **`tests/kb_affected.rs`（Phase 4）**: `affected_tests_cover_every_failing_test`、`affected_with_no_changes_runs_nothing`、`affected_all_when_the_manifest_changes`、`affected_conflicts_are_rejected`、`affected_value_is_a_directory_a_kb_or_a_revision`。
- **単体（`src/kb/*.rs`）**: 手順 1・3・4・11・12 のもの。
- **既存テストへの影響**: **なし**。根拠: 既存のコマンドと `api_diff.rs` を変えない。`fetch.rs` は可視性だけ。`main.rs` は `kb` の分岐と `--affected` の分岐を足すだけで、`--affected` なしの `test` の動きは変わらない（`rejects_ambiguous_or_unused_arguments` に行を足す）。

**性能**: 合否は置かない。`kb show`（KB が新鮮なとき）と `kb impact`（200・1,000 モジュール）の時間を手順 16 で記録する。

## ドキュメント

**Phase 1**: `_tsuzuri/language-reference/compiler/kb.md`（G27 が作る。「照会」の節: 7 コマンド・ID の解決・`--budget`・`--json`、例は付録 B）、`compiler/usage.md` の `### kb`、`compiler/option.md`（「どのサブコマンドが受け付けるか」の表に `--path`・`--budget`・`--depth`・`--module`・`--project`・`--std`・`--no-refresh` の行）、`README.md`（CLI 一覧）、`docs/architecture.md`（`src/kb/query.rs`・`suggest.rs`）。**Phase 2**: `kb.md`・`usage.md` に `kb find`。**Phase 3**: `kb.md` の「impact」の節（分類の表・出力・`--git`・`api-diff` との差、例は付録 C）、`usage.md`、`option.md`（`--git`・`--from`・`--to`）、`docs/architecture.md`（`impact.rs`・`git.rs`）、`docs/stability.md`（`impact` の JSON のキー: `type`・`severity`・`action`・`kind`・`name`・`old`・`new`・`note`、`mode`・`affected`・`total`・`reason` の約束）。**Phase 4**: `usage.md` の `### test` に `--affected`、`option.md`、`README.md`、`docs/benchmarks.md`、GUIDE の台帳。検証: `node scripts/check-docs.mjs <変更したページ>`。

## 受け入れ条件

**Phase 1**

- [ ] 7 コマンドの出力が付録 B と byte 一致し、2 回・別ルートで同じ。`--budget` で項目が後ろから落ち、見出しと `summary` が残る。`--json` が付録 B の行と一致する。
- [ ] ID の解決が §2 の表のとおり（曖昧は選ばず 10 件まで、未知は 5 件までの候補）。KB が古ければ作り直し、`--no-refresh` では `E2009`。
- [ ] 既存のテストの件数と結果が変わらない。`cargo fmt`・`clippy -D warnings`。文書と `check-docs.mjs`。

**Phase 2**: `kb find` が `find` と同じ出力と終了コード（G25 が `done` のとき）。**Phase 3**

- [ ] `impact` の出力が付録 C と byte 一致。公開 API の行が `api-diff` と同じ文字列（`public_changes_equal_api_diff`）。終了コードが 0。
- [ ] `--git` が作業ツリー・index・stash を変えず、終了後に worktree が残らない（成功・失敗とも）。

**Phase 4**: `test --affected` が差分テストで見逃さず、0 件なら何も実行しない。`docs/benchmarks.md` の節と台帳。

## 落とし穴

- **曖昧な ID の最初の 1 件を黙って選ぶ**: AI が誤った ID を使い続ける。`ambiguous_unknown_and_empty_ids`。
- **文字数を `len()`（byte）で測る**: 必ず `chars().count()`。`budget_cuts_items_from_the_end`（`…` は 3 byte）。
- **`HashMap` の反復順が出力に出る**: `BTreeMap`・整列済みの `Vec` だけ。`output_is_deterministic`。
- **一時チェックアウトを `target/` の下に置く**: `collect_sources` が読んで、現在のプロジェクトに混ざる。`temp_dir()` の下に置く。
- **`Drop` が走らない経路**（`std::process::exit`）で worktree が残る: `kb impact` は `ExitCode` を返し、`Checkout` を `main` の関数の中で落とす。`worktree_is_removed_on_success_and_on_failure`。
- **`git worktree add` の標準出力・標準エラーが `--json` の出力に混ざる**: 捨てる（`Stdio::null()`）。
- **ユーザーの `git` の設定・環境変数（`GIT_DIR`・`GIT_WORK_TREE`・`GIT_INDEX_FILE`）で動きが変わる**: `fetch::git_command()` が `GIT_*` を消す。
- **型変数の名前の付け替えを署名の変更にする**: `canon`。`canon_ignores_type_variable_names`。
- **効果の比較を、効果の解析が違う KB の間でする**: `manifest.compiler` が違えば比べない（`facts_are_not_compared_across_compilers`）。
- **`impact` を終了コードで使えると思う**: 常に 0。判定は出力の `breaking` の行。

## 対象外

MCP のツール（G29）、図表（G30）、`tsuzuri find` の実装（G25）。解析の再実行（照会は KB を読むだけ）。複数のリビジョンの履歴（2 つの比較だけ）。クラスのメソッドの `default` の比較（G26 の行にない）。`namespace` の変更の検出（KB の manifest に名前空間がない）。自然言語の検索・埋め込み。`impact` の判定に基づく自動のコミット・ブロック。base の KB の `git` のコミットごとのキャッシュ。

## 決定事項

すべて承認済み（D-50）。

- **D1 照会は KB のファイルだけを読む。** 解析しない（`--no-refresh` で止める）。古ければ G27 の `open` が作り直す。照会の関数は `&KbView` を取りディスクに触れない（G29 が使う）。
- **D2 ID の解決は 6 段の決定表で、曖昧なら選ばない。** 却下: 最初の 1 件を選ぶ（誤りを黙って通す）、あいまい検索の得点（再現しにくい）。
- **D3 出力は `Card` で `--budget` に収める。** 項目を後ろから落とす（見出しと `summary` は残す）。JSON は全行で `--budget` を無視する。
- **D4 公開 API の分類は `api-diff` と同じ文字列。** パリティのテストで固定する。facts・警告・非公開の変更は `info`・`non-breaking` として足すだけ。
- **D5 `impact` の終了コードは 0。** 判定は出力の区分。`api-diff` は breaking で 1（変えない）。
- **D6 `--git` は `git worktree add --detach` を `temp_dir()` の下に作り、`Drop` で `worktree remove --force`（正常終了・エラー・`panic`。Ctrl-C と `SIGKILL` は例外）。** `target/` の下は `collect_sources` が読むので不可。フックは空の置き場へ向けて走らせない。`worktree prune` は呼ばない。却下: `git archive`（種の指定。`.git` を汚さないが、種は worktree と決めている）、作業ツリーでの `git stash`（利用者の状態を変える）、`git worktree prune` で残りを消す（他の worktree の管理情報も消す）。
- **D7 影響を受けるテストは、モジュール単位の変更検出と呼び出しグラフの逆引き（深さ 8）。** 判断できなければ全テスト（保守的）。利用者の class のメソッドの呼び出しは全 instance に届くとみなす。std の class（演算子）の instance は `calls` の辺なしに呼ばれる（G26 の D7）ので、その instance が変更モジュールにあれば全テスト。
- **D8 `test --affected` は `tests.jsonl` の `index` を `--index` に変える。** 0 件なら何も実行しない。`--filter`・`--index` とは併用しない。
- **D9 `kb find` は G25 の `find_command` を呼ぶだけ。** KB を読まない。オプションの共通化もしない（`tsuzuri find` と同じ綴りだけを受け付ける）。
- **D10 class のメソッドの追加は常に breaking。** G26 の行に `default` が無い。足されたら直す。
- **D11 カードの文言（`NEXT`）は変えない。** G27 の golden を動かさず、コマンドの案内は `--help` と文書に置く。
- **D12 `--path` で位置を渡す。** 位置引数は ID・TEXT に使う。G27 の `build`・`verify`・`clean`・`export` は位置引数の `PATH` のまま。

## 付録

[G28-appendix.md](G28-appendix.md): A 実測と `kb_shop_head`（ソース）、B 照会の全出力（テキストと JSON）、C `impact` の全出力、D 参照実装（Python）、E Rust の型の全フィールド、F 各テストの入力・期待と測定の手順。



