# PB08: 言語サーバーの 1 回の再解析と `check` の、捨てる診断を作る計算と重複した解析を除く

| 項目 | 内容 |
| --- | --- |
| ID | PB08 |
| 分類 | ビルド速度（エディターの応答と `check` のフロントエンド） |
| 優先度 | P1（言語サーバーは編集のたびと、クイックフィックスの検証のたびにこの再解析をする。同じ無駄は CLI の `check`・`build`・`test` にもあり、1 ファイルの `check` の約 4 割を占める） |
| 規模 | M（Phase 1。Phase 2 は要承認の別段） |
| 依存 | – |
| 関連 | G39（言語サーバーの待ち行列と取り消し。Phase 3）, G40, PB04（フロントエンドの並列化）, PB06（常駐状態）, G17（フロントエンドの cache）, D-40（言語サーバーは opt-in の std を常に読む） |
| 状態 | todo |
| 起票 | 2026-11-19（G39 の調査で見つけ、利用者の依頼で別のチケットにした。HEAD `af644794` を、strip しない release で `sample` し、別の担当が試作（環境変数で切り替える patch）で効果と出力の同一性を測り直した） |
| 承認 | Phase 1 は不要（出力を変えない。D5 は言語サーバーが使うスレッドの数の方針で既定案）。Phase 2 は要承認: D7（D-40 を変え、名前を書いていない opt-in の std モジュールの補完とホバーが変わる） |
| 手本にする既存実装 | 型の名前の静かな照合: `src/check.rs` の `Names::check_type_path`・`type_choice`（`named_type` の Ok の条件そのもの）。CLI の並列度: `src/driver.rs` の `analyze_cached_core`（`Parallelism::command_line()`）。スレッドの数に依らない出力の検査: `src/lib.rs` の `warnings_and_the_semantic_index_do_not_depend_on_the_thread_count` など、`tests/parallel_frontend.mjs`。計測の形: PB06 の「現状と計測」、GUIDE §14 |
| 主な影響ファイル | `src/warnings.rs`（`TypeReferences::named`、`unused_private`）、`src/check.rs`（共有の照合の helper）、`src/polymorph.rs`（`Classes::matching_instance`、`Inference`）、`src/copies.rs`・`src/ownership.rs`（`costly_sites` だけが使う変種、`call_facts` の再利用）、`src/lsp.rs`（`analyze` の並列度、`refresh` の順序）、`src/lib.rs`（並列度を受け取る入口）、`benchmarks/`（言語サーバーの再解析の driver と golden の script。新規）、`docs/benchmarks.md`、`_tsuzuri/language-reference/compiler/option.md`・`usage.md`（O5 のスレッドの数）、`_perfs/README.md` |
| 計測対象 | 言語サーバーの 1 回の再解析（`didChange` からその直後の `textDocument/semanticTokens/full` の応答まで）、CLI の `check` の時間（1 スレッドと自動）、PX03 の `run-build.mjs` の `check`（200・1,000 module） |

## 目的

Phase 1 では、言語サーバーの 1 回の再解析を、出力（診断・補完・ホバー・インレイヒント・修正案）を 1 byte も変えずに速くする。
今の再解析は、小さいプロジェクトでは同じプロジェクトの CLI の `check` の約 5 倍（hello 527 ms と 93 ms）、大きいプロジェクトでも 2〜3 倍かかる（big 1,682 ms と 819 ms、syn200 2,629 ms と 848 ms）。
その半分以上は、作ってすぐ捨てる診断のための計算と、同じ解析の繰り返しです。
それを除くと、言語サーバーは編集から診断まで速くなり（G39 の原因 5 の遅れも縮む）、CLI の `check`・`build`・`test` のフロントエンドも速くなる。

## 着手条件と停止条件

### 着手条件

- GUIDE §14 の手順 1・2（before の release を `target/perf/PB08/tsuzuri-before` に写す、環境を記録する）を済ませていること。
- 下の「計測手順」の workload を作り、before の値を取ってあること。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- 「変えてはいけない意味」の golden の比較で、1 byte でも違いが出て、その理由を変更の誤りとして説明できない。
- O1 の静かな照合が `named_type` と食い違う型の名前（Ok と Some が一致しない）が見つかる。
- O4(b) で、`lower_kernels`（`src/check.rs:9686`）が本体を書き換えたモジュールにも、検査の `check_all_with` の facts を使う必要が出る。
- Phase 2 に着手する前に D7 の承認がない。

## 現状と計測（HEAD `af644794`）

### 再解析の流れ

- `Session::refresh`（`src/lsp.rs:366-427`）は、`Project::load_with_overlays` でプロジェクトを読み、`analyze` で全体を検査し、`copies::costly_sites`（インレイヒント）と `inferred_effects` を求めてから診断を送る。
- `load_with_overlays` は、補完のためすべての std を読む（`src/driver.rs:2449-2450`、D-40）。66 モジュール 745,183 B。CLI は 34 の基本モジュール 390,808 B と、ソースが名前を書いた opt-in のモジュールだけを読む（`stdlib::sources_for`、`src/driver.rs:2678-2682`）。
- `analyze`（`src/lsp.rs:1518-1555`）は `analyze_inputs_indexed_all`（`src/lib.rs:223-228`）を通り、`Parallelism::library()`（`src/parallel.rs:72-75`。`TSUZURI_THREADS` がなければ 1 スレッド）で、フロントエンドの cache なしに検査する。
  CLI は `Parallelism::command_line()`（最大 8 スレッド。`src/driver.rs:3130-3131`）と G17 の cache を使う。
- std は再解析のたびにソースから構文解析し直し、型検査し直す。PB06 の常駐状態はファイルの本文だけ、G17 の cache は構文解析の結果だけを持つ。
- クイックフィックスの検証（`fix_edit`、`src/lsp.rs:1269-1324`）は、修正案ごとに同じ読み込みと `analyze` をもう一度する。

### 5 倍の内訳

Apple M3（8 コア）の macOS、release、`sample` 1 ms。普段の負荷が 3 前後の機械なので、各値は 9〜11 回の中央値で、負荷を記録してある（`target/scratch/g40-pb08/pb08-refresh-cost/timing/release-matrix.jsonl`・`summary.tsv`）。
_perfs/README の計測機（M1 Max）ではない。

- std の集合が 4.3 倍: 1 スレッドの CLI の `check` で、hello は基本の std で 124 ms、すべての opt-in の名前をコメントに書いて全部読ませると 534 ms。後者は言語サーバーの 1 回の再解析（527 ms）と同じ。
- スレッドが約 1.3 倍: CLI の hello は 1 スレッドで 124.4 ms、自動で 93.4 ms（1.33 倍）。言語サーバーに `TSUZURI_THREADS=8` を与えると、hello は 527 ms から 443 ms（1.19 倍）。
- 言語サーバーだけの処理（`semantic::collect`、インレイヒントなど）は小さいプロジェクトで約 2 %。大きいプロジェクトでは `costly_sites` が効く（下記）。読み込みは 0 %、構文解析は 3.4 %。

| workload | 言語サーバー 1 スレッド | 言語サーバー 8 スレッド | CLI 基本 std 自動 | CLI 基本 std 1 スレッド | CLI 全 std 1 スレッド |
| --- | ---: | ---: | ---: | ---: | ---: |
| hello（`examples/hello`） | 527 | 443 | 93.4 | 124.4 | 533.6 |
| conventions（21 KB） | 637 | 510 | 130.7 | | |
| json（24 KB） | 756 | 653 | 305.7 | | |
| in_place_writes（70 KB） | 774 | 652 | 228.3 | | |
| big（6 ファイル 215 KB） | 1682 | 1505 | 819 | 959 | 1460 |
| syn200（201 ファイル 727 KB） | 2629 | 2252 | 848 | 966 | |

単位は ms。

### 時間の行き先

再解析の main thread の標本の割合（`target/scratch/g40-pb08/pb08-refresh-cost/prof/{hello,big,syn200}.sample.txt`。別の担当の profile も約 1.5 point 以内で一致）。

| 処理 | hello | big | syn200 |
| --- | ---: | ---: | ---: |
| `unused_private`（うち捨てる診断を作る経路） | 51.6 %（49.8 %） | 32.5 %（31.6 %） | 69.4 %（67.1 %） |
| `Classes::matching_instance` の捨てる不一致の診断（`specialize`・本体の検査・`finish`・`recursion` の行に含まれる） | 11.8 % | 8.5 % | 5.1 % |
| `copies::costly_sites`（インレイヒント） | 0 % | 14.4 % | 2.2 % |
| `specialize` | 9.3 % | 16.7 % | 4.4 % |
| `ownership::check_all_with` | 〜0 % | 15.4 % | 1.7 % |
| 本体の検査・`infer_copy_all`・`finish`・`recursion` | 約 29 % | 約 14 % | 約 13 % |

1. **`unused_private` が捨てる診断を作る**（`src/warnings.rs:286`、`src/check.rs:9525` から）。`TypeReferences::named`（`src/warnings.rs:472-477`）は、std を含むすべての関数のすべての型の名前で `Names::named_type` を呼び、Ok だけを使う。
   名前が見つからないと（`i64`・`string` などの基本型はすべて。検査器は先に `numeric::primitive` で解決するが（`src/check.rs:10475`）、この走査はしない）、`named_type`（`src/check.rs:6880-6907`）は `find::with_suggestions(…, &self.type_pool(module))` で候補つきの `UnknownTypeName` を作る。
   `type_pool`（`src/find.rs:1211-1233`）はプログラムのすべての型の名前で `type_choice` を呼ぶので、1 回の空振りが宣言された型の数に比例する。本当の照合は 1 % 未満です。
   CLI の `check` も同じ経路を通り、hello の 29 %、全 std の 48 %、syn200 の 51 % を占める。
2. **`matching_instance` が捨てる不一致の診断を作る**（`src/polymorph.rs:2385-2423`）。型クラスの各 instance と unify を試し、失敗した unify は `Type::display_with` と `format!` で不一致の診断を作り（`src/polymorph.rs:411-418`）、`.ok()?`（`:2416`）が捨てる。
3. **`costly_sites` が所有権の解析を繰り返す**（`src/copies.rs:106-117`）。`ownership::copy_reads`（`src/ownership.rs:326-347`）が、検査の `check_all_with` が求めたのと同じ `call_facts(module, false)`（`src/ownership.rs:363`）をもう一度求め、std の特殊化を含むすべての関数で `check_body` を動かしてから、利用者の関数の箇所だけを残す（`copies.rs:112`）。
   CLI は `--warn-implicit-copy` のときだけ求める（`src/main.rs:4256-4260`）。
4. **言語サーバーは 1 スレッド**（上記）。`unused_private`・`specialize`・`costly_sites` は直列で、本体の検査・`finish`・`infer_copy_all`・`check_all_with`・構文解析は並列にできる。

1 と 2 は、宣言された型と instance の数に比例して伸びるので、std の集合を倍にすると検査が約 4 倍になる（1 と 2 を除くと、全 std と基本 std の差は +416 ms から +124 ms に縮んだ）。

### 試作で測った効果

別の担当が `af644794` の写しに、O1・O3・O4(a) を環境変数で切り替える patch を当てて測った（`target/scratch/g40-pb08/verify-pb08-refresh-cost/patch.py`、`out/toggles.jsonl`・`out/cli.jsonl`・`out/summary.tsv`）。
切り替えなしの値は main の binary と同じ（hello 527〜534 ms）。

| 段 | hello | big | syn200 |
| --- | ---: | ---: | ---: |
| 今 | 533.9 | 1681.5 | 2446.7 |
| O1 | 255.6 | 1118.8 | 776.1 |
| O1+O3 | 207.2 | 972.2 | 653.5 |
| O1+O3+O4(a) | 194.4 | 913.5 | 656.3 |
| O1+O3+O4(a)、8 スレッド | 115.4 | 740.1 | 469.8 |

言語サーバーの 1 回の再解析、1 スレッド（最後の行を除く）、ms。

| workload | スレッド | 今 | O1 | O1+O3 |
| --- | --- | ---: | ---: | ---: |
| hello | 1 | 125.8 | 77.1 | 73.4 |
| hello | 自動 | 92.3 | 53.0 | 48.2 |
| 全 std | 1 | 528.1 | 255.4 | 192.9 |
| syn200 | 1 | 960.6 | 395.2 | 373.5 |
| syn200 | 自動 | 842.8 | 273.2 | 256.9 |

CLI の `check`、ms。

出力は同じだった。`check --json`（終了コード・stdout・stderr）は、`examples/`・`tests/fixtures/`・`tests/conformance/` と上の workload の 454 個で、1 スレッドと自動のどちらも一致した。
言語サーバーの記録（すべての URI の診断、セマンティックトークン、各ファイルのインレイヒント、ドキュメントシンボル）も、10 の workload で、切り替えの有無と 8 スレッドで一致した。
ただし、自動の最初の全件の実行で 1 回だけ、`fx-json-allstd`（全 std を読む json）に 1 件の違いが出た。内容は記録しておらず、2 回の全件の再実行と 34 回の個別の再実行では再現しない。

## 目標と指標

- **M1（主）**: 言語サーバーの 1 回の再解析。`textDocument/didChange` を送ってから、その直後に送った `textDocument/semanticTokens/full` の応答まで。1 つのサーバーで warm-up 2 回、測定 11 回以上、中央値と最小・最大。変更なしの `semanticTokens/full` だけの時間（今 0〜3 ms）も別に測る。
- **M2**: 再解析の段の内訳（`TSUZURI_TIME_PASSES=1`。言語サーバーは終了時に累計を 1 回だけ出すので、再解析の回数で割る）と、strip しない release の `sample` の割合。
- **M3**: CLI の `check`（`TSUZURI_CACHE_DIR=` で cache なし、1 スレッドと自動、基本 std と全 std）と、PX03 の `run-build.mjs` の `check`（200・1,000 module）。
- **M4（利用者に見える値）**: `didChange` から `publishDiagnostics` まで（200 ms の debounce を含む）と、`big` の `codeAction` 1 回の時間。

目標（同じ機械・同じ条件の before と after の中央値。言語サーバーは既定の設定、つまり D5 の後）:

- after が before の hello 0.29 倍以下、big 0.48 倍以下、syn200 0.23 倍以下。起票時の Apple M3 では hello ≤ 150 ms、big ≤ 800 ms、syn200 ≤ 600 ms に当たる（今 527・1682・2629 ms。試作の O1+O3+O4(a) と 8 スレッドで 115・740・470 ms）。
  コアの数で `Parallelism::command_line()` のスレッドの数が変わるので、コアの数も記録する。
- CLI の `check` と PX03 の `check` は、どの workload でも遅くならない。速くなった値を `docs/benchmarks.md` に記録する。
- 共有の CI に速さの合否を入れない（GUIDE §14.7）。

## 変えてはいけない意味

- 診断: コード・kind・メッセージ・span・順序・件数（修正案の検証は件数で比べる。`src/lsp.rs` の `fix_removes`）。警告の有無と順序。
- 言語サーバーの応答: 補完・ホバー・定義・参照・セマンティックトークン・ドキュメントシンボル・インレイヒント・修正案（quickfix と `source.fixAll.tsuzuri`）。
- スレッドの数に依らない出力（`src/lib.rs:345-349`、`src/check.rs:9124-9127` の不変条件）。
- `costly_sites` 以外の `copies::sites`・`ownership::copy_reads` の利用者の結果。`effects.rs:2185`（`@no_copy`・`@no_alloc` と `inferred_effects`）、`llvm.rs:2594-2618`（暗黙の複製の一覧の検証）、`kg/facts.rs:188` が std を含む全関数の結果を使う。
- Phase 1 では、言語サーバーが読む std の集合（D-40）。

## 設計

### Phase 1（出力を変えない）

| 案 | 変更 | 効果（試作か profile の割合） |
| --- | --- | --- |
| **O1** | `TypeReferences::named`（`src/warnings.rs:472-477`）は、`named_type` ではなく、診断を作らない照合を使う。`check_type_path(module, name, span).is_ok()` で、`type_choice(module, name)` が `Choice::Found` のときだけ参照に足す。`named_type` の Ok の条件と同じ（`src/check.rs:6880-6907`）なので結果は変わらない。`warnings.rs` は `check` の子のモジュール（`src/check.rs:60-61`）なので、私的な `check_type_path`・`type_choice` を直接呼べる。`named_type` と同じ条件を 1 か所に置く helper にする | hello −278 ms（−52 %）、big −563、syn200 −1671（試作）。CLI の `check` も −39〜−68 % |
| **O3** | `Classes::matching_instance`（`src/polymorph.rs:2385-2423`）だけが使う「静かな」`Inference` にする。不一致のとき、`mismatch()` の文字列を作らずに空のメッセージの `Diagnostic` を返す。制御の流れ（unify の中の `E1003` の判定を含む）は変えず、捨てられる Err の中身だけが変わる | hello −63 ms（−12 %）、big −131、syn200 −113（試作） |
| **O3(b)**（任意） | instance を（型クラス、頭の型構成子）で索引し、`variables(&instance.head)` を前もって求める。変数・別名・匿名共用体は保守的に扱い、instance の順を保つ | O3 の後も `matching_instance` は hello の 16.7 %。約 −30 ms（見積もり） |
| **O4(a)** | `costly_sites` だけが使う変種（例えば `copies::sites_where`・`ownership::copy_reads_where`）で、利用者の関数だけの `check_body` を動かす。共有の `copy_reads`・`sites` は変えない | big −59 ms（試作）、ほかは 0 |
| **O4(b)** | `costly_sites` の `call_facts` を、検査の `check_all_with`（`src/check.rs:9628`）が求めたものの再利用にする。`gpu_devices::lower_kernels`（`src/check.rs:9686`）は、デバイス呼び出しの instance があるときだけ利用者の本体を書き換える（`src/gpu_devices.rs:336-338` の早期 return の後、`:363` の `number`）。facts は書き換えがなかったときだけ `CheckedModule` で運び、書き換えたときは運ばずに、`costly_sites` が今と同じく求め直す（言語サーバーは検査の外で `costly_sites` を呼ぶため） | big 約 −157 ms（O4(a) の後に残る `call_facts` の時間） |
| **O2**（任意） | `unused_private` の辺を、到達した節点を取り出すときに作る（`src/warnings.rs:449-460` の走査は到達した節点の辺しか読まない）。入口の式の辺（`:438-446`）は入口の節点にまとめる | O1 の後の `unused_private` 全体が hello 約 8 ms（4.0 %）・big 約 11 ms（1.2 %）で、効果はその一部（上限） |
| **O5** | 言語サーバーの `analyze` と、それを呼ぶ `fix_edit`・rename の検証は、`Parallelism::command_line()` を使う（`TSUZURI_THREADS` があればそれ）。並列度を受け取る入口を `src/lib.rs` に足す | 今: −11 %（big）〜−20 %（conventions）。hello は −16 %。O1・O3・O4(a) の後: −41 %・−19 %・−28 %（hello・big・syn200。試作） |
| **O9** | `Session::refresh` で、`costly_sites` と `inferred_effects`（`src/lsp.rs:383-391`）を診断を送る処理（`:395-409`）の後へ移す。送る内容は同じで、診断が早く届く | big で約 −250 ms（今）、O4(a) の後は約 −190 ms（`costly_sites` が再解析の 20.8 %）、O4(b) の後は約 −30 ms（残るのは利用者の関数の `check_body` など 3.6 %）の `didChange`→`publishDiagnostics` |
| **O8**（任意） | 言語サーバーの process の中で、std の構文解析の結果を再解析をまたいで使う（G17 の `FrontendCache` は source の番号を付け替えられる。`src/frontend_cache.rs:1-13`） | 約 −18 ms（Phase 1 の後は hello の約 9 %） |

### Phase 2（要承認: D7）

- **O6**: 言語サーバーも、ソース（未保存のバッファを含む）が名前を書いた opt-in の std だけを読む（`stdlib::sources_for`）。
  モジュール名の補完は `stdlib::SOURCES` の静的な一覧から出す。名前を書く前の `Json.` のメンバーの補完とホバーは、全 std の宣言の索引を process に 1 回だけ作って答える。
  CLI の値で見積もると、O1・O3 の後でも hello の全 std と基本 std の差は約 70 ms（自動）。Phase 1 で目標に届けば着手しない。1 コアの機械や遅い機械で足りないと分かったときの選択肢として残す。

## 実装手順

### 手順 1: 計測の driver と before

- `benchmarks/` に言語サーバーの再解析の driver を足す（M1・M4。今の `target/scratch/g40-pb08/pb08-refresh-cost/refresh-loop.mjs`、`target/scratch/g39/lsp-latency-robustness/latency.mjs`・`codeaction-cost.mjs` を手本に。どれも追跡されない場所にあるので、ここで移す）。
  workload の作り方（下の「計測手順」）も同じ所に置き、作り直せるようにする。driver は PX01 の record（`benchmarks/metrics.mjs` の `makeRecord`・`writeRecords`）で書く。
- before の M1〜M4 を取る。

### 手順 2: golden の基準

- `check --json` の golden（454 個、1 スレッドと自動）と、言語サーバーの記録（すべての URI の診断、セマンティックトークン、全ファイルのインレイヒント、ドキュメントシンボル、決めた位置のホバーと補完（`Matrix.`・`Json.`・最上位のモジュールの一覧を含む）、quickfix と `source.fixAll.tsuzuri`）を before の binary で作る。
  試作の `target/scratch/g40-pb08/verify-pb08-refresh-cost/golden-cli.mjs` を手本にし、script は手順 1 の driver と同じ `benchmarks/` に置いて、そのコマンドをこのチケットに書く。
  golden は before の binary で 2 回作って比べ、before 同士で揺れる target を先に見つける（揺れがあれば、変更の前に報告する。上の `fx-json-allstd` の 1 件を参照）。

### 手順 3: O1

- 変更: `src/warnings.rs`、`src/check.rs`。
- 確認: `src/warnings.rs` の単体テスト（`check_type_path`・`type_choice` は `Names` の私的な関数なので、`tests/` からは呼べない）で、静かな照合と `named_type` が次で一致する（Some と Ok）。
  基本型・自分と import した record・別名・handle・私的な型・曖昧な名前・`::` のパス・名前空間を `.` でつないだパス（`namespace_spelling` が `::` の綴りを返す枝。`src/check.rs:6892-6902`）・モジュール名を繰り返したパス（`check_module_type` の誤り。`src/check.rs:6596-6614`）・`Async`・`bigint`。
  `cargo test --locked --lib warnings`（`running N tests` の N が 0 でない）と `cargo test --locked --test warnings`。手順 2 の golden が一致する。

### 手順 4: O3（と任意の O3(b)）

- 変更: `src/polymorph.rs`。
- 確認: 静かな unify と今の unify が、多相の suite（`tests/polymorphism.rs`・`tests/text_class.rs`）で同じ instance と同じ置き換えを選ぶ。golden が一致する。

### 手順 5: O4(a)・O4(b)・O9

- 変更: `src/copies.rs`、`src/ownership.rs`、`src/check.rs`、`src/lsp.rs`。
- 確認: `tests/copy_cost.rs` の入力で `costly_sites` が前後で同じ。GPU のデバイス呼び出しを含む例で同じ。全ファイルのインレイヒントの golden が一致する（`big` の 107 件は `Main.tz` 以外にある）。`tests/lsp.rs` の `inlay_hints_show_implicit_copies`。

### 手順 6: O2（任意）・O8（任意）

- 確認: `tests/warnings.rs` の `private_reachability_includes_types_aliases_and_public_roots`・`active_patterns_instances_and_entry_annotations_keep_private_dependencies`・`suppresses_generated_and_standard_library_bindings`。std の関数を通してだけ到達する利用者の私的な型と関数の例を、作れるなら足す（作れないときは、その理由をチケットに記録する）。

### 手順 7: O5

- 変更: `src/lib.rs`、`src/lsp.rs`。
- 確認: `src/lib.rs` のスレッドの数に依らない検査と `tests/parallel_frontend.mjs`。言語サーバーの golden が 1 スレッドと 8 スレッドで一致する。

### 手順 8: after の計測と記録

- GUIDE §14 のとおり、before と after を交互に、各組 9 回以上（M1 は 11 回以上）測る。`node scripts/slot.mjs --` を通し、各組の前後の `uptime` を記録する。生データは `target/perf/PB08-before`・`target/perf/PB08-after`。
- strip しない release（`CARGO_PROFILE_RELEASE_STRIP=false CARGO_PROFILE_RELEASE_DEBUG=line-tables-only`）の `sample` で、`Names::type_pool`・`find::with_suggestions`（`TypeReferences::named` の下）、`polymorph::mismatch`・`Type::display_with`（`matching_instance` の下）、2 回目の `call_facts` が消えたことを示す。
- `docs/benchmarks.md` に、日付・環境・コミット・中央値・生データの場所を書く。CLI の `check` と PX03 の値も書く。
- `node benchmarks/metrics.mjs report target/perf/PB08-after --baseline target/perf/PB08-before` の表を完了報告に添える（_perfs/README の運用ルール）。

### 手順 9（Phase 2。D7 の承認の後だけ）: O6

- 変更: `src/driver.rs`（`load_with_overlays` の std の選び方）、`src/lsp.rs`（`ModuleNames` の一覧、opt-in の索引）。
- 確認: 名前を書いていないプロジェクトで、最上位の補完が opt-in のモジュール名を出す。本文が構文解析できない最初の編集で `Json.` のメンバーを補完する。opt-in のモジュール名のホバー。`tests/lsp.rs` の `opt_in_matrix_module_completes_and_hovers_when_named`・`opt_in_view_and_tensor_modules_complete_when_named`。

## 計測手順

### 環境と条件

- GUIDE §14 の 1〜3。before は `target/perf/PB08/tsuzuri-before`。機械・CPU のコア数・OS・rustc・Node・コミットを記録する。
- 負荷の低い機械で測る。起票時の機械（Apple M3、8 コア）は普段の負荷が 3 前後（ブラウザー、ウイルス対策、Spotlight）で、細かい差（数 ms）は読めない。
- 言語サーバーは `TSUZURI_THREADS` なしと `=1`・`=8`、CLI は `TSUZURI_CACHE_DIR=` で cache を切り、`TSUZURI_THREADS=1` と自動。

### workload

- `examples/hello`・`examples/point`・`examples/polymorphism`（std の固定費）。
- `tests/fixtures/conventions`（21 KB）・`json`（24 KB。`Json` を名指しするので CLI も読む）・`anonymous_unions`・`list_fsharp`（43 KB）・`in_place_writes`（70 KB）。
- big（6 ファイル 215 KB）: G39 の調査の `Main.tz`（関数 4 つ、`W1001` が 6 つ）と、`tests/fixtures/{array_fsharp,vec_dotnet,in_place_writes,json,list_fsharp}/Main.tz` をそのまま `ArrayF.tz`・`VecD.tz`・`InPlace.tz`・`JsonT.tz`・`ListF.tz` にしたもの（今の写しは `target/scratch/g39/lsp-latency-robustness/projects/big`）。手順 1 で作り方を driver に入れる。
- syn200: `benchmarks/build/generate.mjs` の `writeProject(dir, 200)`（201 ファイル、727 KB）。
- 全 std の CLI: hello の 1 行目のコメントにすべての opt-in のモジュール名を書いたもの（`sources_for` は本文の識別子を数えるので、CLI も全部読む）。

### コマンド

- M1: `node <手順 1 の driver> <compiler> <project> <回数>`（今の手本: `node target/scratch/g40-pb08/pb08-refresh-cost/refresh-loop.mjs <compiler> <project> 11`）。
- M3: `TSUZURI_CACHE_DIR= TSUZURI_TIME_PASSES=1 <compiler> check <project>`（`TSUZURI_THREADS=1` と自動）。PX03: `node benchmarks/run-build.mjs <compiler> --sizes 200,1000 --languages tsuzuri --variants check --out target/perf/PB08-px03-<before|after>`。
- M4: 手順 1 の driver（`didChange`→`publishDiagnostics` と、`big` の `codeAction` 1 回）。今の手本は `target/scratch/g39/lsp-latency-robustness/latency.mjs`・`codeaction-cost.mjs`。

## 生成コードの確認

生成する IR と機械語は変えない（フロントエンドの内部の計算だけを変える）。
代わりに手順 8 の profile で、除いた frame が消えたことを示し、golden で出力が 1 byte も変わらないことを示す。

## テスト計画

- 手順 2 の golden（CLI 454 個、言語サーバーの記録）を before と after で比べる。どの手順の後も一致する。
- 新しい単体テスト: O1 の照合の一致（手順 3）、O3 の instance の選び方の一致（手順 4）、`costly_sites` の一致と GPU の例（手順 5）、std を通して到達する私的な宣言（手順 6）。
- O5 はフロントエンドの並列の呼び出し（`src/parallel.rs` の `Parallelism`）を変えるので、`TSUZURI_THREADS=8 cargo nextest run --locked` も通す（README の検証の注記）。
- 既存のテスト: `tests/warnings.rs`、`src/warnings.rs` の単体テスト、`tests/lsp.rs`、`tests/lsp_sessions.mjs`、`tests/fix.rs`、`tests/copy_cost.rs`、`tests/polymorphism.rs`、`tests/text_class.rs`、`src/lib.rs` のスレッドの数の検査、`tests/parallel_frontend.mjs`。
- HEAD で落ちる機能のテストはない（振る舞いを変えないため）。効果は手順 8 の計測と profile で示す。

## ドキュメント

- `docs/benchmarks.md`: 手順 8 の結果。
- O5 の後、言語サーバーが使うスレッドの数（CLI と同じく、`TSUZURI_THREADS` がなければ使えるコアを最大 8 つ）を書く。
  `_tsuzuri/language-reference/compiler/option.md` の「コンパイラのスレッド数」の表で、`lsp` を「`repl`・`lsp`・`publish` とライブラリ API は 1」から「使える core 数（最大 8）」の側へ移し、`_tsuzuri/language-reference/compiler/usage.md` の `lsp` の節にも書く。`node scripts/check-docs.mjs <変えたページ>` を通す。
- Phase 2 を行うときは、D-40 の記述（`_features/GUIDE.md` の D-40）と、言語サーバーが全 std を読むと書いた文書。
- 完了したら、`_perfs/README.md` の PB08 の行を done にし、このチケットを `_perfs/_completed/` へ移す。

## 受け入れ条件

- [ ] 手順 2 の golden が、1 スレッドと自動・8 スレッドで before と 1 byte も違わない（Phase 2 を行ったときは、D7 で変わると決めた、名前を書いていない opt-in の std モジュールの補完とホバーを除く）。
- [ ] M1 の目標（同じ機械の before に対して、hello 0.29 倍以下、big 0.48 倍以下、syn200 0.23 倍以下）に届くか、届かない理由を計測で示している。コアの数を記録している。
- [ ] CLI の `check` と PX03 の `check` がどの workload でも遅くならず、値を `docs/benchmarks.md` に記録している。
- [ ] profile で、捨てる診断を作る frame と 2 回目の `call_facts` が消えている。
- [ ] 新しい単体テストと既存のテストが通る（T0・T1、`TSUZURI_THREADS=8 cargo nextest run --locked`、`node tests/lsp_sessions.mjs target/debug/tsuzuri`、`node tests/parallel_frontend.mjs target/release/tsuzuri`）。
- [ ] Phase 2 は D7 の承認があるときだけ行っている。

## 落とし穴

- O4(a) を `ownership::copy_reads` や `copies::sites` の中に入れると、`effects.rs:2185`・`llvm.rs:2594-2618`・`kg/facts.rs:188` が使う std の結果が消える。`costly_sites` だけの変種にする。
- O4(b) で `check_all_with` の facts をそのまま使うと、その後の `gpu_devices::lower_kernels`（`src/check.rs:9686`）が書き換えた本体と合わない。書き換えたモジュールでは facts を運ばない。
- O1 の静かな照合を `named_type` と別に書くと、後で片方だけが変わる。条件を 1 か所の helper に置く。基本型を先に飛ばす変種は、利用者が基本型と同じ名前の型を宣言できるかを確かめてから（静かな照合だけならその確認は要らない）。
- O3 の静かな `Inference` は `matching_instance` だけで使う。ほかの unify の診断のメッセージを変えない。
- O5 の後、言語サーバーは編集のたびに最大 8 つのコアを短く使う。G39 の Phase 3 の取り消しで途中で止めるなら、並列の段の worker にも届くようにする。
- `TSUZURI_TIME_PASSES` の段を `check` の中に足すと、`benchmarks/run-build.mjs:43` の「段が重ならない」検査に落ちる。足すなら `check` を止めてから計り、また始める（`timings::finish` は名前で足す）。
- PX03 の `run-build.mjs` は `--out` が要る（`benchmarks/run-build.mjs:529`）。
- 計測機の負荷が高いと、数 ms の差は読めない。O2・O8 のような小さい案の効果は、profile の割合で示す。

## 対象外

- **O7**: 検査済みの std を再解析をまたいで持つこと。`check_modules_collect`（`src/check.rs:7705-9688`）は全体を 1 つの表で検査し、std の関数の番号は利用者のソースの後に付く（`src/driver.rs:2683-2695`）ので、モジュール単位の検査と interface の要約が要る（XL。PB06 の G1 も見送った。`docs/benchmarks.md:4569`）。別のチケットにする。
- 言語サーバーの解析を別のスレッドで動かし、古い解析を途中で捨てること（G39 の対象外と同じ）。

## 決定事項

- **D1: O1・O3・O4(a)・O4(b)・O5・O9 を Phase 1 で行い、O2・O3(b)・O8 は計測で効果が見えるときだけ足す**（既定案）。
- **D2: O1 は `named_type` の Ok の条件と同じ照合を helper にして、`named_type` と `TypeReferences::named` の両方から使う**（既定案）。
- **D3: O3 は `Inference` の「静かな」設定で、捨てる Err の中身だけを変える**（既定案）。unify の制御の流れを変えないので、成功のときの置き換えは同じ。
- **D4: O4(a) は `costly_sites` だけの変種にする**（既定案）。共有の `copy_reads` の結果は std を含めて使われているため。
- **D5: 言語サーバーは CLI と同じ `Parallelism::command_line()` を使う**（既定案）。出力はスレッドの数に依らないことが検査済みで、エディターの中の短い並列は CLI の `check` と同じ。`TSUZURI_THREADS` で上限を決められる。
- **D6: 共有の CI に速さの合否を入れない**（既定案。GUIDE §14.7）。
- **D7: Phase 2 の O6（言語サーバーも、名前を書いた opt-in の std だけを読む）**（要承認）。D-40（言語サーバーは補完のため常に全部を読む）を変え、名前を書く前の opt-in のモジュールの補完とホバーを、別に作る索引から答えることになる。Phase 1 の後の計測で足りないときだけ判断する。
