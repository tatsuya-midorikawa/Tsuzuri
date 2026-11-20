# PB08: 言語サーバーの 1 回の再解析と `check` の、捨てる診断を作る計算と重複した解析を除く

| 項目 | 内容 |
| --- | --- |
| ID | PB08 |
| 分類 | ビルド速度（エディターの応答と `check` のフロントエンド） |
| 優先度 | P1（言語サーバーは編集のたびと、クイックフィックスの検証のたびにこの再解析をする。同じ無駄は CLI の `check`・`build`・`test` にもあり、1 ファイルの `check` の約 4 割を占める） |
| 規模 | M（Phase 1。Phase 2 は別段で、2026-11-20 の利用者の指示で行う） |
| 依存 | – |
| 関連 | G39（言語サーバーの待ち行列と取り消し。Phase 3）, G40, PB04（フロントエンドの並列化）, PB06（常駐状態）, G17（フロントエンドの cache）, D-40（言語サーバーは opt-in の std を常に読む） |
| 状態 | doing（Phase 1 done・measured 2026-11-20（M1 の目標に届いた）。Phase 2 は 2026-11-20 の利用者の指示で実装中） |
| 起票 | 2026-11-19（G39 の調査で見つけ、利用者の依頼で別のチケットにした。HEAD `af644794` を、strip しない release で `sample` し、別の担当が試作（環境変数で切り替える patch）で効果と出力の同一性を測り直した） |
| 承認 | Phase 1 は不要（出力を変えない。D5 は言語サーバーが使うスレッドの数の方針で既定案）。Phase 2 の D7（D-40 を変え、名前を書いていない opt-in の std モジュールの補完・ホバー・診断の候補・シグネチャヘルプを別の索引から答える。答えは変えない）は、2026-11-19 に利用者が承認した（「承認が必要なものについては、あなたが考えるベストな方法であれば全て承認します」）。起票者が最善と判断した形のまま承認したので、Phase 1 の後の計測で目標に届かないときだけ行うという条件も含んでいた。この条件は、2026-11-20 の利用者の指示「もし実装に際し複数フェーズにまたがる場合には、その全てのフェーズを完了させること」で外れた。Phase 1 は目標に届いたが、Phase 2 も行う |
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
- （2026-11-20 に外れた）Phase 1 の計測で目標に届いているのに、Phase 2 に着手しようとしている。D7 の承認は届かないときだけ行うという条件つきだったが、2026-11-20 の利用者の指示「もし実装に際し複数フェーズにまたがる場合には、その全てのフェーズを完了させること」で Phase 2 も行うことになったので、この条件では止めない。

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
  編集は、測る文書（`Main.tz`。fixture は各 project の `Main.tz`）の全文の置き換えで、元の本文の末尾に、版 v が奇数なら `"\n"`、偶数なら `"\n\n"` を足したもの（起票時の `refresh-loop.mjs` と同じ。診断は変わらない）。`initialize` は `rootUri` を project の根にする。
- **M2**: 再解析の段の内訳（`TSUZURI_TIME_PASSES=1`。言語サーバーは終了時に累計を 1 回だけ出すので、再解析の回数で割る）と、strip しない release の `sample` の割合。
- **M3**: CLI の `check`（`TSUZURI_CACHE_DIR=` で cache なし、1 スレッドと自動、基本 std と全 std）と、PX03 の `run-build.mjs` の `check`（200・1,000 module）。
- **M4（利用者に見える値）**: `didChange` から `publishDiagnostics` まで（200 ms の debounce を含む）と、`big` の `codeAction` 1 回の時間。
  - `didChange`→`publishDiagnostics`: 編集は元の本文の末尾に `"\n// edit <i>\n"` を足した全文の置き換え（診断は変わらない）と、`"\ndef broken_<i> :: i64 = undefined_name_<i>\n"` を足したもの（`E1002` が 1 つ増える）を交互に。時間は、編集した文書の URI で `version` が送った版に等しい `publishDiagnostics` が届くまで（ほかの URI や前の版の通知では止めない）。
  - `codeAction`（`big` の `Main.tz`、上の本文のまま）: (1) 診断のない行（20 行目、0 始まりで 19）の 0 桁、`context: {diagnostics: [], triggerKind: 2}`、(2) 最初の診断の範囲、`context: {diagnostics: [その診断], triggerKind: 2}`、(3) (2) に `only: ["quickfix"]`、(4) 文書全体、`context: {diagnostics: <全部>, triggerKind: 1}`、(5) 0:0、`context: {diagnostics: [], only: ["source.fixAll"], triggerKind: 1}`。
    結果の action の題の一覧も記録し、before と after で同じであることを確かめる（起票時の `codeaction-cost.mjs` と同じ要求）。

目標（同じ機械・同じ条件の before と after の中央値。言語サーバーは既定の設定、つまり D5 の後）:

- after が before の hello 0.29 倍以下、big 0.48 倍以下、syn200 0.23 倍以下。起票時の Apple M3 では hello ≤ 150 ms、big ≤ 800 ms、syn200 ≤ 600 ms に当たる（今 527・1682・2629 ms。試作の O1+O3+O4(a) と 8 スレッドで 115・740・470 ms）。
  コアの数で `Parallelism::command_line()` のスレッドの数が変わるので、コアの数も記録する。
- CLI の `check` と PX03 の `check` は、どの workload でも遅くならない。速くなった値を `docs/benchmarks.md` に記録する。
- 共有の CI に速さの合否を入れない（GUIDE §14.7）。

## 変えてはいけない意味

- 診断: コード・kind・メッセージ・span・順序・件数（修正案の検証は件数で比べる。`src/lsp.rs` の `fix_removes`）。警告の有無と順序。
- 言語サーバーの応答: 補完・ホバー・定義・参照・セマンティックトークン・ドキュメントシンボル・インレイヒント・修正案（quickfix と `source.fixAll.tsuzuri`）。
- 生成する IR と機械語（O3 は instance の選び方、O4(b) は特殊化の後のモジュールに触れるので、IR を before の compiler と比べる。手順 2）。
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
| **O4(b)** | `costly_sites` の `call_facts` を、検査の `check_all_with`（`src/check.rs:9628`）が求めたものの再利用にする。`gpu_devices::lower_kernels`（`src/check.rs:9686`）は、デバイス呼び出しの instance があるときだけ利用者の本体を書き換える（`src/gpu_devices.rs:336-338` の早期 return の後、`:363` の `number`）。facts は書き換えがなかったときだけ、言語サーバーの解析の入口が返す crate 内の私的な結果（例えば `CheckedModule` と facts の組。`CheckedModule` の公開の field には置かない）で運び、言語サーバーはそのモジュールを変えずに私的な `costly_sites` の変種へ渡す。
`CheckedModule.functions` は公開で書き換えられ、`copies::costly_sites` は公開の API なので、公開の `costly_sites` は今と同じく facts を求め直す（呼び出し側が関数を変えたり足したりした後に古い facts を使うと、結果が古くなるか添字が範囲を外れる）。書き換えがあったモジュールでも求め直す | big 約 −157 ms（O4(a) の後に残る `call_facts` の時間） |
| **O2**（任意） | `unused_private` の辺を、到達した節点を取り出すときに作る（`src/warnings.rs:449-460` の走査は到達した節点の辺しか読まない）。入口の式の辺（`:438-446`）は入口の節点にまとめる | O1 の後の `unused_private` 全体が hello 約 8 ms（4.0 %）・big 約 11 ms（1.2 %）で、効果はその一部（上限） |
| **O5** | 言語サーバーの `analyze` と、それを呼ぶ `fix_edit`・rename の検証は、`Parallelism::command_line()` を使う（`TSUZURI_THREADS` があればそれ）。並列度を受け取る入口を `src/lib.rs` に足す | 今: −11 %（big）〜−20 %（conventions）。hello は −16 %。O1・O3・O4(a) の後: −41 %・−19 %・−28 %（hello・big・syn200。試作） |
| **O9** | `Session::refresh` で、`costly_sites` と `inferred_effects`（`src/lsp.rs:383-391`）を診断を送る処理（`:395-409`）の後へ移す。送る内容は同じで、診断が早く届く | big で約 −250 ms（今）、O4(a) の後は約 −190 ms（`costly_sites` が再解析の 20.8 %）、O4(b) の後は約 −30 ms（残るのは利用者の関数の `check_body` など 3.6 %）の `didChange`→`publishDiagnostics` |
| **O8**（任意） | 言語サーバーの process の中で、std の構文解析の結果を再解析をまたいで使う（G17 の `FrontendCache` は source の番号を付け替えられる。`src/frontend_cache.rs:1-13`） | 約 −18 ms（Phase 1 の後は hello の約 9 %） |

### Phase 2（D7。承認済み。2026-11-20 の利用者の指示で行う）

- **O6**: 言語サーバーも、ソース（未保存のバッファを含む）が名前を書いた opt-in の std だけを読む（`stdlib::sources_for`）。
  公開のライブラリの API `Project::load_with_overlays` は変えない（今と同じく全 std を `Project.sources` に入れる。別の索引を持たない利用者のため）。言語サーバー専用の、名前を書いた opt-in だけを読む入口を足し、`Session::refresh` と `fix_edit` だけがそれを使う。
  モジュール名の補完は `stdlib::SOURCES` の静的な一覧から出す。名前を書く前の `Json.` のメンバーの補完とホバーは、全 std の宣言の索引を process に 1 回だけ作って答える。
  同じ索引で、次も今と同じにする（変わってよいのは、名前を書いていない opt-in のモジュールの補完とホバーの出どころだけで、答えは今と同じ）。
  - 診断の候補と修正案: 未解決の名前の候補（`Matrx.init` に `Matrix` を勧める、など）は、読み込んでいない opt-in のモジュールの名前とメンバーも候補に入れる。今は全 std を読むので候補にあり、その置き換えの修正案とクイックフィックスが出る。
  - シグネチャヘルプ: 名前を書いた最初の編集が構文解析できない間（`Json.parse (` を打ち始めたところ）は、シグネチャヘルプが最後に成功した索引に戻る（`src/lsp.rs` の `view`）。その索引に `Json.parse` がないので、opt-in の索引からも引く。
  CLI の値で見積もると、O1・O3 の後でも hello の全 std と基本 std の差は約 70 ms（自動）。起票時は、Phase 1 で目標に届けば着手せず、1 コアの機械や遅い機械で足りないと分かったときの選択肢として残すことにしていた（D7）。Phase 1 は目標に届いたが、2026-11-20 の利用者の指示で Phase 2 も行う。

## 実装手順

### 手順 1: 計測の driver と before

- `benchmarks/` に言語サーバーの再解析の driver を足す（M1・M4。今の `target/scratch/g40-pb08/pb08-refresh-cost/refresh-loop.mjs`、`target/scratch/g39/lsp-latency-robustness/latency.mjs`・`codeaction-cost.mjs` を手本に。どれも追跡されない場所にあるので、ここで移す）。
  workload の作り方（下の「計測手順」）も同じ所に置き、作り直せるようにする。driver は PX01 の record（`benchmarks/metrics.mjs` の `makeRecord`・`writeRecords`）で書く。
- before の M1〜M4 を取る。

### 手順 2: golden の基準

- `check --json` の golden（454 個、1 スレッドと自動）と、言語サーバーの記録（すべての URI の診断、セマンティックトークン、全ファイルのインレイヒント、ドキュメントシンボル、決めた位置のホバーと補完（`Matrix.`・`Json.`・最上位のモジュールの一覧を含む）、quickfix と `source.fixAll.tsuzuri`）を before の binary で作る。
  試作の `target/scratch/g40-pb08/verify-pb08-refresh-cost/golden-cli.mjs` を手本にし、script は手順 1 の driver と同じ `benchmarks/` に置いて、そのコマンドをこのチケットに書く。
  golden は before の binary で 2 回作って比べ、before 同士で揺れる target を先に見つける（揺れがあれば、変更の前に報告する。上の `fx-json-allstd` の 1 件を参照）。
- 生成する IR は、既存の `tests/parallel_frontend.mjs` に before の compiler を渡して比べる: `TSUZURI_BASELINE=target/perf/PB08/tsuzuri-before node tests/parallel_frontend.mjs target/release/tsuzuri`（README の検証の節。`TSUZURI_BASELINE` を付けると、その compiler とも診断・IR・終了コードを比べる）。
  `check --json` と言語サーバーの記録には IR が入らないので、instance の選び方や特殊化が変わって生成コードだけが変わっても、それだけでは見つからない。各手順の後に動かす。

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

### 手順 9（Phase 2。2026-11-20 の利用者の指示で行う。起票時の条件は手順 8 の計測で目標に届かないときだけ）: O6

- 変更: `src/driver.rs`（言語サーバー専用の読み込みの入口。公開の `load_with_overlays` は変えない）、`src/lsp.rs`（`ModuleNames` の一覧、opt-in の索引）。
- 確認: 名前を書いていないプロジェクトで、最上位の補完が opt-in のモジュール名を出す。本文が構文解析できない最初の編集で `Json.` のメンバーを補完する。opt-in のモジュール名のホバー。`tests/lsp.rs` の `opt_in_matrix_module_completes_and_hovers_when_named`・`opt_in_view_and_tensor_modules_complete_when_named`。
  ほかに `Matrix` を参照しないプロジェクトで `Matrx.init` と書くと、今と同じ候補・置き換えの修正案・クイックフィックスが出る。`Json` を参照しない成功した解析の後、`Json.parse (` と打ち始めたところで `textDocument/signatureHelp` が `Json.parse` を返す。
  手順 2 の golden（Phase 2 用に、この 2 つの場面を足したもの）が一致する。名前を書いていない opt-in のモジュールの補完とホバーも、答えは今と同じでなければならない（変わるのは答えの出どころだけなので、比べる対象から外さない）。
- O6 の後に手順 8 の計測（M1〜M4、PX03）をもう一度行い、Phase 1 の後の値と分けて記録する。Phase 1 の値は Phase 1 の効果（目標に届いたこと）を、Phase 2 の値は最終の効果と退行のないことを示す（起票時は、Phase 1 の値が Phase 2 に入った理由を示すはずだった。Phase 2 は 2026-11-20 の利用者の指示で行う）。

## 計測手順

### 環境と条件

- GUIDE §14 の 1〜3。before は `target/perf/PB08/tsuzuri-before`。機械・CPU のコア数・OS・rustc・Node・コミットを記録する。
- 負荷の低い機械で測る。起票時の機械（Apple M3、8 コア）は普段の負荷が 3 前後（ブラウザー、ウイルス対策、Spotlight）で、細かい差（数 ms）は読めない。
- 言語サーバーは `TSUZURI_THREADS` なしと `=1`・`=8`、CLI は `TSUZURI_CACHE_DIR=` で cache を切り、`TSUZURI_THREADS=1` と自動。

### workload

- `examples/hello`・`examples/point`・`examples/polymorphism`（std の固定費）。
- `tests/fixtures/conventions`（21 KB）・`json`（24 KB。`Json` を名指しするので CLI も読む）・`anonymous_unions`・`list_fsharp`（43 KB）・`in_place_writes`（70 KB）。
- big（6 ファイル 215 KB、`Tsuzuri.toml` なしのフォルダー）: 下の `Main.tz`（415 B。修正案を持つ `W1001` が 6 つ）と、`tests/fixtures/{array_fsharp,vec_dotnet,in_place_writes,json,list_fsharp}/Main.tz` をそのまま `ArrayF.tz`・`VecD.tz`・`InPlace.tz`・`JsonT.tz`・`ListF.tz` に写したもの（起票時の写しは `target/scratch/g39/lsp-latency-robustness/projects/big` で、5 つの写しは fixture とバイト単位で同じ）。
  手順 1 で、この `Main.tz` と写し方を driver か `benchmarks/` の下の追跡されるファイルに入れる。

```tsuzuri
def alpha :: own i64 -> i64 = \n -> {
    let first = n + 1;
    let second = n + 2;
    n
}

def beta :: own i64 -> i64 = \m -> {
    let third = m * 2;
    let fourth = m * 3;
    m
}

def gamma :: own i64 -> own i64 -> i64 = \a b -> a

def delta :: own i64 -> i64 = \k -> {
    let fifth = k - 1;
    k
}

def main :: own unit -> i32 = \() ->
    do! IO.write_line (alpha 1 + beta 2 + gamma 3 4 + delta 5)
    0
```
- syn200: `benchmarks/build/generate.mjs` の `writeProject(dir, 200)`（201 ファイル、727 KB）。
- 全 std の CLI: hello の 1 行目のコメントにすべての opt-in のモジュール名を書いたもの（`sources_for` は本文の識別子を数えるので、CLI も全部読む）。

### コマンド

- M1: `node <手順 1 の driver> <compiler> <project> <回数>`（今の手本: `node target/scratch/g40-pb08/pb08-refresh-cost/refresh-loop.mjs <compiler> <project> 11`）。
- M3: `TSUZURI_CACHE_DIR= TSUZURI_TIME_PASSES=1 <compiler> check <project>`（`TSUZURI_THREADS=1` と自動）。PX03: `node benchmarks/run-build.mjs <compiler> --sizes 200,1000 --languages tsuzuri --variants check --out target/perf/PB08-px03-<before|after>`。
- M4: 手順 1 の driver（`didChange`→`publishDiagnostics` と、`big` の `codeAction` 1 回）。今の手本は `target/scratch/g39/lsp-latency-robustness/latency.mjs`・`codeaction-cost.mjs`。

## 生成コードの確認

生成する IR と機械語は変えない（フロントエンドの内部の計算だけを変える）。
手順 2 の `TSUZURI_BASELINE` つきの `tests/parallel_frontend.mjs` で、IR が before の compiler と同じことを示す。手順 8 の profile で、除いた frame が消えたことを示し、golden で診断と言語サーバーの応答が 1 byte も変わらないことを示す。

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

- [x] `TSUZURI_BASELINE=target/perf/PB08/tsuzuri-before node tests/parallel_frontend.mjs target/release/tsuzuri` が通る（IR が before と同じ）。
- [x] 手順 2 の golden が、1 スレッドと自動・8 スレッドで before と 1 byte も違わない（Phase 2 を行ったときも同じ。Phase 2 は名前を書いていない opt-in の std モジュールの補完・ホバー・診断の候補・修正案・シグネチャヘルプの出どころを別の索引に変えるが、答えは変えない）。Phase 2 を行ったときは、手順 8 の計測を O6 の後にもう一度行い、Phase 1 の後の値と分けて記録している。
- [x] M1 の目標（同じ機械の before に対して、hello 0.29 倍以下、big 0.48 倍以下、syn200 0.23 倍以下）に届くか、届かない理由を計測で示している。コアの数を記録している。
- [x] CLI の `check` と PX03 の `check` がどの workload でも遅くならず、値を `docs/benchmarks.md` に記録している。
- [x] profile で、捨てる診断を作る frame と 2 回目の `call_facts` が消えている。
- [x] 新しい単体テストと既存のテストが通る（T0・T1、`TSUZURI_THREADS=8 cargo nextest run --locked`、`node tests/lsp_sessions.mjs target/debug/tsuzuri`、`node tests/parallel_frontend.mjs target/release/tsuzuri`）。
- [ ] Phase 2 は、2026-11-20 の利用者の指示で行った（手順 8 の計測で Phase 1 は目標に届いたが、D7 の「届かないときだけ」という条件は指示で外れた）。Phase 2 の後の値（手順 9）は Phase 1 の後の値と分けて記録している。GUIDE の D-40 と、言語サーバーが全 std を読むと書いた文書を更新している。

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
  O4(b) の facts の再利用も、言語サーバーの解析の入口から私的な変種へ渡す経路だけにし、公開の `copies::costly_sites` は facts を求め直す（`CheckedModule` の field は公開で書き換えられるため）。
- **D5: 言語サーバーは CLI と同じ `Parallelism::command_line()` を使う**（既定案）。出力はスレッドの数に依らないことが検査済みで、エディターの中の短い並列は CLI の `check` と同じ。`TSUZURI_THREADS` で上限を決められる。
- **D6: 共有の CI に速さの合否を入れない**（既定案。GUIDE §14.7）。
- **D7: Phase 2 の O6（言語サーバーも、名前を書いた opt-in の std だけを読む）**（承認済み。2026-11-19。Phase 1 の後の計測で目標に届かないときだけ行うという条件つきだった。条件は 2026-11-20 の利用者の指示で外れた: Phase 1 は目標に届いたが、Phase 2 も行う）。D-40（言語サーバーは補完のため常に全部を読む）を変え、名前を書く前の opt-in のモジュールの補完とホバーを、別に作る索引から答えることになる。
  同じ索引から、読み込んでいない opt-in のモジュールの名前とメンバーを診断の候補（修正案）とシグネチャヘルプにも出し、それらは今と変えない（O6 の説明を参照）。

## 実装と検証（2026-11-20）

### 範囲

Phase 1 の手順 1〜5・7 と、手順 8 のうちコードの部分（driver・golden・文書の枠）を実装した。
**手順 8 の計測**は、実装の後に、負荷の低い機械で別に行った（実装の間は 3 つのチケットを同じ機械で並行して build しており、load average が 20〜145 だった）。結果と、O2・O3(b)・O8 と Phase 2 の判断は、下の「手順 8: 計測（2026-11-20、Phase 1）」にある。
手順 6（O2・O8。任意）も、計測で効果を見てから決めるので行っていない。手順 6 の「std の関数を通してだけ到達する利用者の私的な型と関数」のテストは、O2 で辺の作り方を変えるときの安全網なので、O2 と一緒に足す。

### コミット

| コミット | 内容 |
| --- | --- |
| `20bdee57` | 手順 1・2: `benchmarks/run-lsp.mjs`（M1・M4・M3 の PX01 record）、`benchmarks/lsp/workloads.mjs`（workload）、`benchmarks/lsp/golden.mjs`（golden）、`benchmarks/lsp/client.mjs`（JSON-RPC の client） |
| `70fb84ef` | O1 |
| `c0fd6cf7` | O3 |
| `6682a931` | O4(a)・O4(b)・O9 |
| `4c5d81a4` | O5 |
| `8bfb4efc` | 文書 |
| `b073473a` | この節（実装と検証） |
| `86934130` | レビューの指摘 1: copies のテストに `tests/copy_cost.rs` の残りの 5 つの入力を足した |

### 実装

- **O1**（`src/check.rs`・`src/warnings.rs`）: `Names::type_lookup`（`check_type_path` の後に `type_choice`）に `named_type` の Ok の条件を 1 か所にまとめ、`named_type` もそれを使う。`Names::quiet_named_type` は `Ok(Choice::Found)` のときだけ型を返し、診断を作らない。`TypeReferences::named` はこれを使う（D2）。
- **O3**（`src/polymorph.rs`）: `Inference` に `quiet` を足し、`Inference::quiet()` は `matching_instance` だけが作る。3 か所の `mismatch(...)` を `type_mismatch`（quiet なら空のメッセージの `E1003`。コード・kind・span は同じ）にした。unify の制御の流れは変えていない（D3）。`matching_instance` の本体は `unifying_instance` に移し、`cfg(test)` のときだけ quiet でない試行とも比べられる。O3(b) は行っていない。
- **O4(a)**（`src/copies.rs`・`src/ownership.rs`）: `ownership::copy_reads_of_users` は利用者のモジュールの関数だけで `check_body` を動かす。`copies::costly_sites`（公開）もこの変種を使う。共有の `copy_reads`・`sites` の結果は変えていない（`sites` は `sites_of` に filter を渡すだけ）。
- **O4(b)**（`src/ownership.rs`・`src/check.rs`・`src/gpu_devices.rs`・`src/lib.rs`・`src/copies.rs`）: `check_all_with_facts` が検査の call facts を返し（`CopyFacts`、中身は私的な `CallFacts`）、`check_modules_collect` は `gpu_devices::lower_kernels`（書き換えたかどうかを `bool` で返すようにした）が本体を書き換えなかったときだけ facts を渡す。言語サーバーの入口 `analyze_inputs_for_editor` は `copies::EditorModule`（module と facts の組。field は `copies.rs` の私的なもので、module は警告を取り出すほかは読み取り専用）を返し、`EditorModule::costly_sites` が facts を使う。公開の `costly_sites` は facts を求め直す（D4）。
- **O9**（`src/lsp.rs`）: `Session::refresh` は、診断を送る loop の後に `costly_sites` と `inferred_effects` を求める。
- **O5**（`src/lib.rs`・`src/lsp.rs`）: `src/lsp.rs` の `analyze`（`refresh`・`fix_edit`・rename の検証が使う）は `analyze_inputs_for_editor` に `Parallelism::command_line()` を渡す（D5）。

`src/lsp.rs` の変更は、`analyze` の入口と並列度、`refresh` の中の `costly_sites`・`inferred_effects` の位置、`use` の 1 行、単体テスト 1 つだけ（G39 の Phase 2・3 との統合のため）。

### 手順 1: driver と workload

```sh
node benchmarks/lsp/workloads.mjs target/perf/PB08/projects            # workload を作り直す（--force で上書き）
node benchmarks/run-lsp.mjs <compiler> --out <dir> [--projects hello,big,syn200] [--threads unset,1,8] [--samples 11] [--parts refresh,publish,codeaction,cli] [--quick]
```

- workload: hello・point・polymorphism（`examples`）、conventions・json・anonymous_unions・list_fsharp・in_place_writes（`tests/fixtures`）の写し、big（上の `Main.tz` と 5 つの fixture の写し。215,197 B）、syn200（`writeProject(dir, 200)`。727,244 B）、全 std の CLI 用の `hello-allstd`・`big-allstd`（1 行目のコメントに `src/stdlib.rs` の `OPT_IN` のすべての名前）。
- `run-lsp.mjs` は suite `lsp` の record を書く。variant は `refresh-<auto|1|8>`（M1）、`tokens-<…>`（変更なしの `semanticTokens/full`）、`publish-comment-<…>`・`publish-error-<…>`（M4 の `didChange`→`publishDiagnostics`）、`codeaction-{cursor,diagnostic,quickfix,document,fixall}-<…>`（M4 の big の 5 つの要求。action の題は `projects.json`）、`check-1`・`check-auto`（M3。`TSUZURI_CACHE_DIR=`、`TSUZURI_TIME_PASSES=1`）。`metrics.mjs` の段の名前にない段（`effects`）が出たときは、その workload の段の record だけを省く。
- before の M1〜M4 は手順 8 と一緒に取る（上の「範囲」）。

### 手順 2: golden

```sh
node benchmarks/lsp/golden.mjs cli <compiler> target/perf/PB08/golden/<名>/cli-<1|auto|8>.jsonl --threads <1|auto|8> --jobs 3
node benchmarks/lsp/golden.mjs lsp <compiler> target/perf/PB08/golden/<名>/lsp-<unset|1|8> --threads <unset|1|8>
node benchmarks/lsp/golden.mjs compare <a.jsonl|a-dir> <b.jsonl|b-dir>
TSUZURI_BASELINE=target/perf/PB08/tsuzuri-before node tests/parallel_frontend.mjs <compiler>
```

- CLI: `check <target> --json` の終了コード・stdout・stderr と、`check` が書く知識グラフのファイル（`TSUZURI_KG_DIR` の別の置き場。`.tsuzuri-kg` の stamp は除く）。target は 442 個（`examples` 13、`tests/fixtures` 166、`tests/conformance` の `.tz` 251、workload 12。終了コードが 0 でないもの 262、知識グラフを書くもの 188）。
- 言語サーバー: 10 の workload（`-allstd` を除く）ごとに、すべての URI の診断（各変更の後）、全ファイルのセマンティックトークン・インレイヒント（big は 107 件）・ドキュメントシンボル、決めた位置（識別子を均等に Main.tz 60・ほかのファイル 20）のホバーと定義、prepareRename と rename、M4 の 5 つの `codeAction` と Main.tz の診断ごとの quickfix、`E1002` を足した編集・`Matrx.init` の編集・`Matrix.`・`Json.`・最上位の行の補完、元に戻した後。
- **before 同士の揺れ**: before の binary で 2 回作り（`before-a`・`before-b`）、CLI の 1 スレッド・自動（442/442 ずつ）、言語サーバー（10/10）がすべて一致した。CLI の 1 スレッドと自動も一致した。揺れる target はなかった（起票時の `fx-json-allstd` の 1 件に当たるものは出ていない）。

### 各手順の後の golden と IR

| binary | コミット | CLI 1 スレッド | CLI 自動 | CLI 8 スレッド | 言語サーバー 未設定 | 言語サーバー 1 | 言語サーバー 8 | IR（`parallel_frontend.mjs`、`TSUZURI_BASELINE`） |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `tsuzuri-o1` | `70fb84ef` | 442/442 | 442/442 | | 10/10 | | | ok |
| `tsuzuri-o3` | `c0fd6cf7` | 442/442 | 442/442 | | 10/10 | | | ok |
| `tsuzuri-o4` | `6682a931` | 442/442 | 442/442 | | 10/10 | | | ok |
| `tsuzuri-o5` | `4c5d81a4` | 442/442 | 442/442 | 442/442 | 10/10 | 10/10 | 10/10 | ok |

どれも before（`target/perf/PB08/tsuzuri-before`、main `295ffcb4` の release）の golden（CLI は 1 スレッド、言語サーバーは未設定）と 1 byte も違わない。binary はどれも `cargo build --release --locked` で、各コミットの `src/` を取り出して作った。

### 単体テスト（回帰テスト）

| テスト | 内容 | base での結果 |
| --- | --- | --- |
| `check::warnings::tests::a_successful_check_builds_no_type_suggestions` | 成功する検査が型の候補の表（`type_pool`）を作らない。報告する未知の型は候補を持つ | 失敗（2,062 回作っていた） |
| `check::warnings::tests::quiet_type_lookup_agrees_with_named_type` | `quiet_named_type` と `named_type` が一致し、手で書いた期待とも一致する: 基本型・std の型・自分と import した record・別名・extern type・私的な型・曖昧な名前（2 つのモジュールの `Item`、2 つの `using` の `Text`）・`::` のパス・`.` でつないだ名前空間（`namespace_spelling`）・モジュール名を繰り返したパス（`Sample::Point.Point`、`Maybe.Maybe`）・`Async`・`Async.Operation`・`bigint` | 新しい関数を呼ぶので base では build できない（同値の検査） |
| `check::polymorph::tests::instance_trials_write_no_mismatch_message` | 成功する解析が不一致のメッセージを書かない。報告する `E1003` は書く | 失敗（O1 の後で 29,713 回） |
| `check::polymorph::tests::quiet_inference_unifies_like_the_loud_one` | 19 組の型で、quiet と通常の unify の成否・コード・kind・span・解が同じ。`E1003` だけメッセージが空 | build できない（同値） |
| `check::polymorph::tests::quiet_instance_trials_choose_like_loud_ones` | 12 の fixture（instance の多いもの）と std の解析で、`matching_instance` の quiet な試行と通常の試行が同じ instance と置き換えを選ぶ（1,000 回以上比べる） | build できない（同値） |
| `copies::tests::costly_sites_from_the_check_facts_are_the_recomputed_ones` | `tests/copy_cost.rs` の 19 の入力すべて（レビューの指摘 1 で、初めの 14 に `MOVED_TWICE`・`SCALAR_RECORD`・`ARGUMENT_MOVED`・`EXPLICIT`・`EXPLICIT_LIST` を足した）で、facts を使う `EditorModule::costly_sites`・公開の `costly_sites`・PB08 の前の計算（全関数の `sites` を filter）が同じで、facts を使うほうは `call_facts` を求めない。`tests/gpu.rs` の `DEVICE_PROGRAM` に配列の複製を足した例は facts を持たず、求め直して同じ | build できない（新しい入口） |
| `tests::the_editor_analysis_does_not_depend_on_the_thread_count` | 30 module の言語サーバーの解析の module・索引・costly sites が 1・2・8 スレッドで同じ | build できない（新しい入口） |
| `lsp::tests::analysis_uses_the_threads_of_the_command_line` | 言語サーバーの解析が並列の段を動かす（`command_line()` の数が 2 以上のとき） | 失敗（`Parallelism::library()` に戻すと並列の段 0） |

ほかに、`matching_instance` の比較を常に動かす一時的な build で `cargo test --locked --test polymorphism --test text_class` が通った（35・13 件。手順 4 の確認。build は戻した）。

### T0・T1・T2

- T0（各編集）: `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings` は通った。area のテスト: `cargo test --locked --lib warnings`（12 件）、`--test warnings`（14 件）、`--lib polymorph::tests`（7 件）、`--test copy_cost`（16 件）・`--test gpu`（17 件）、`--test lsp`（52 件、`inlay_hints_show_implicit_copies` を含む）、`--test polymorphism`（35 件）・`--test text_class`（13 件）。
- T1: `node scripts/slot.mjs -- cargo nextest run --locked` は 3,359 件が通過、4 件が skip（1,061 秒。load average 60〜145）。
- `TSUZURI_THREADS=8 node scripts/slot.mjs -- cargo nextest run --locked` は 3,359 件が通過、4 件が skip（976 秒）。
- T2: `cargo build` の後、`node tests/lsp_sessions.mjs target/debug/tsuzuri` は UTF-16・UTF-8 の session と features の 4 つが通った。`TSUZURI_BASELINE=… node tests/parallel_frontend.mjs` は上の表。`TSUZURI_BIN=target/perf/PB08/tsuzuri-o5 node scripts/check-docs.mjs _tsuzuri/language-reference/compiler/option.md _tsuzuri/language-reference/compiler/usage.md` は通った（2 pages、180 links、6 examples、native の O0/O3 6 runs、test project 1）。`node scripts/gen-docs.mjs --check`（19 blocks、0 stale）、`node scripts/check-cli-docs.mjs`（4 checks、0 failed）、`node scripts/check-links.mjs`（0 broken）、`node scripts/check-guide-lite.mjs`（0 failed）。`src/runtime/` は変えていない。
- VS Code の拡張のテストは動かしていない（拡張は変えておらず、言語サーバーの応答は golden で before と同じであることを示した）。

### 速さの目安（参考。手順 8 の計測ではない）

負荷の高い共有機（Apple M3、8 コア、macOS 27.2.1、rustc 1.99.1、Node v24.22.2）で、`node benchmarks/run-lsp.mjs <compiler> --out target/scratch/pb08/sanity-<before|after> --projects hello,big,syn200 --threads unset --parts refresh,publish,cli --quick`（標本 3 個）を before、after の順に 1 回ずつ動かした。before の間の load average は 13〜18、after の間は 33〜39 で、条件は揃っていない。値は中央値（ms）で、手順 8 の判定には使わない。

| workload | M1 before | M1 after | M4 `publish-comment` before | M4 after |
| --- | ---: | ---: | ---: | ---: |
| hello | 669 | 371 | 814 | 568 |
| big | 2,248 | 2,046 | 2,686 | 1,470 |
| syn200 | 3,519 | 1,112 | 3,759 | 1,222 |

big の M1 は、after の 8 スレッドがこの負荷で取れるコアが少ないので差が小さく出た。`TSUZURI_THREADS=1` で before と after を交互に 2 回ずつ（各 4 回の再解析。`benchmarks/lsp/client.mjs` を使う使い捨ての script で、server の `TSUZURI_TIME_PASSES` も読んだ。load average 約 40）動かすと、big の 1 回の再解析は 3,731〜3,987 ms から 1,637〜1,886 ms（約 0.46 倍）で、server の `TSUZURI_TIME_PASSES` の再解析あたりの `check` の段は約 1,910 → 約 560 ms、`specialize` は約 620 → 約 420 ms だった。

CLI の `check` の wall はこの負荷では読めなかった（3 標本の中央値が前後で逆転するものがあった）。before と after を交互に 5 回ずつ動かした `TSUZURI_THREADS=1 TSUZURI_TIME_PASSES=1 TSUZURI_CACHE_DIR= tsuzuri check target/perf/PB08/projects/hello` の `total_ms` は、中央値で 321 → 217 ms（`check` の段は 258 → 170 ms。load average 約 50）。

### 手順 8: 計測（2026-11-20、Phase 1）

GUIDE §14 のとおり、before（`target/perf/PB08/tsuzuri-before`、main `295ffcb4` の release、SHA-256 `48068cd7…`）と after（`target/perf/PB08/tsuzuri-o5`、`4c5d81a4` の release、`f4bc339c…`。その後のコミットはテストと文書だけ）を交互に測った。

- **環境**: Apple M3（8 コア。`Parallelism::command_line()` は 8 スレッド）、macOS 27.2.1、Node v24.22.2。ほかのワークツリーの作業は止めていた。
- **負荷**: 計測中の load average は 2.9〜6.0。Spotlight（`mds_stores`）、ブラウザー、エディター、ウイルス対策が動いていた。各 chunk の前後の `uptime` は `target/perf/PB08-chunks/orchestrate.log` にある。
- **手順**: chunk ごとに before と after の順を入れ替え、各 chunk を `node scripts/slot.mjs --` で動かした。chunk は次の 7 つ: hello・big・syn200 の M1 と M4 の `publish`（`TSUZURI_THREADS` 未設定・1・8）、小さい 7 つの workload の M1（未設定）、big の `codeAction`（未設定・1・8）、CLI の `check`（12 の workload、1 スレッドと自動）、PX03。
- **標本**: M1・M4 は warm-up 2 回の後に各 11 回、CLI は warm-up 1 回の後に 11 回、PX03 は runner の既定（9 回）。
- **コマンド**: `target/scratch/pb08/measure/orchestrate.mjs` が `benchmarks/run-lsp.mjs`・`benchmarks/run-build.mjs` を呼び、chunk を `target/perf/PB08-before`・`target/perf/PB08-after`（run_id ごとに 1 つ）にまとめた。
- **生データ**: chunk ごとの記録と `projects.json`（標本ごとの値、各 server の前後の負荷、code action の題）は `target/perf/PB08-chunks/` にある。`node benchmarks/metrics.mjs report target/perf/PB08-after --baseline target/perf/PB08-before` の表は `target/scratch/pb08/measure/report-phase1.md`。
- record の `commit` 欄はどちらも driver のコミット（`8d01556e`）で、比べた binary は `host.tsuzuri_sha256` で区別できる。

**M1（主）**: 1 回の再解析（ms）。目標の 3 つ（未設定、つまり既定の設定）はすべて届いた。hello 0.271 倍（目標 0.29 倍以下）、big 0.377 倍（0.48 倍以下）、syn200 0.203 倍（0.23 倍以下）。

| workload・variant | before 中央値（最小〜最大） | after 中央値（最小〜最大） | after/before |
| --- | ---: | ---: | ---: |
| hello `refresh-auto` | 493（490〜518） | 134（125〜139） | 0.271 |
| hello `refresh-1` | 488（483〜497） | 190（189〜201） | 0.390 |
| hello `refresh-8` | 407（403〜411） | 126（124〜127） | 0.309 |
| big `refresh-auto` | 1576（1560〜1633） | 594（581〜619） | 0.377 |
| big `refresh-1` | 1538（1535〜1547） | 725（719〜747） | 0.471 |
| big `refresh-8` | 1366（1365〜1385） | 574（567〜595） | 0.420 |
| syn200 `refresh-auto` | 2288（2277〜2319） | 464（456〜514） | 0.203 |
| syn200 `refresh-1` | 2285（2278〜2313） | 615（613〜620） | 0.269 |
| syn200 `refresh-8` | 2100（2092〜2195） | 456（453〜503） | 0.217 |
| point `refresh-auto` | 488（486〜503） | 129（120〜172） | 0.265 |
| polymorphism `refresh-auto` | 490（489〜511） | 122（121〜124） | 0.249 |
| conventions `refresh-auto` | 562（561〜563） | 140（139〜142） | 0.249 |
| json `refresh-auto` | 693（691〜711） | 232（230〜251） | 0.335 |
| anonymous_unions `refresh-auto` | 608（606〜612） | 167（166〜170） | 0.275 |
| list_fsharp `refresh-auto` | 615（613〜635） | 181（179〜183） | 0.294 |
| in_place_writes `refresh-auto` | 711（710〜762） | 209（209〜211） | 0.294 |

変更なしの `semanticTokens/full`（`tokens-*`）は before・after とも 0.1〜3 ms で、`metrics.mjs` の判定はほぼ「差なし」だった。syn200 の `tokens-8` だけが 0.62 → 1.12 ms（広がり 0.34 ms）で「悪化」と出た。この要求は変更なしでは再解析しない（変えた経路を通らない）。1 ms 未満の差で、同じ after の `tokens-auto`（同じ 8 スレッド）は 0.63 ms なので、測り方の揺れとみる。Phase 2 の計測でもう一度見る。

**M4（利用者に見える値）**: `didChange` → `publishDiagnostics`（ms、200 ms の debounce を含む）と、big の `codeAction`（ms）。

| workload・variant | before 中央値（最小〜最大） | after 中央値（最小〜最大） | after/before |
| --- | ---: | ---: | ---: |
| hello `publish-comment-auto` | 735（728〜746） | 370（355〜386） | 0.504 |
| hello `publish-comment-1` | 737（720〜754） | 445（432〜451） | 0.603 |
| hello `publish-comment-8` | 644（636〜648） | 372（363〜373） | 0.578 |
| hello `publish-error-auto` | 419（391〜423） | 341（320〜342） | 0.814 |
| hello `publish-error-1` | 420（404〜427） | 395（367〜398） | 0.941 |
| hello `publish-error-8` | 345（339〜350） | 340（339〜343） | 0.985 |
| big `publish-comment-auto` | 1778（1753〜1818） | 781（767〜804） | 0.439 |
| big `publish-comment-1` | 1781（1755〜1788） | 926（910〜955） | 0.520 |
| big `publish-comment-8` | 1607（1575〜1654） | 771（755〜820） | 0.480 |
| big `publish-error-auto` | 499（470〜506） | 425（407〜492） | 0.853 |
| big `publish-error-1` | 497（486〜528） | 503（490〜529） | 1.012 |
| big `publish-error-8` | 389（385〜408） | 420（400〜436） | 1.079 |
| syn200 `publish-comment-auto` | 2521（2514〜2547） | 652（644〜659） | 0.259 |
| syn200 `publish-comment-1` | 2530（2517〜2552） | 814（805〜818） | 0.322 |
| syn200 `publish-comment-8` | 2329（2319〜2367） | 652（633〜664） | 0.280 |
| syn200 `publish-error-auto` | 615（600〜618） | 498（473〜514） | 0.810 |
| syn200 `publish-error-1` | 613（583〜635） | 605（597〜612） | 0.987 |
| syn200 `publish-error-8` | 470（458〜484） | 497（462〜520） | 1.058 |

`publish-error`（`E1002` を足す編集）は、検査が名前の解決で止まり、O1（`unused_private`）・O4・O9 の経路を通らないので、ほとんど変わらない。`metrics.mjs` の判定は、1・8 スレッドの big・syn200 で「差なし」。ただし中央値は +1〜+8 % で、広がりより小さい。自動は 0.81〜0.85 倍。

| workload・variant | before 中央値（最小〜最大） | after 中央値（最小〜最大） | after/before |
| --- | ---: | ---: | ---: |
| big `codeaction-cursor-auto` | 2225（2215〜2264） | 1032（1021〜1119） | 0.464 |
| big `codeaction-cursor-1` | 2574（2569〜2577） | 1314（1313〜1320） | 0.510 |
| big `codeaction-cursor-8` | 2250（2210〜2379） | 1022（1018〜1026） | 0.454 |
| big `codeaction-diagnostic-auto` | 3540（3529〜3562） | 1569（1565〜1577） | 0.443 |
| big `codeaction-diagnostic-1` | 3902（3882〜3955） | 2007（2002〜2017） | 0.514 |
| big `codeaction-diagnostic-8` | 3357（3349〜3416） | 1566（1555〜1579） | 0.466 |
| big `codeaction-quickfix-auto` | 1317（1311〜1320） | 541（539〜543） | 0.411 |
| big `codeaction-quickfix-1` | 1321（1304〜1350） | 688（684〜694） | 0.520 |
| big `codeaction-quickfix-8` | 1134（1132〜1189） | 540（536〜568） | 0.476 |
| big `codeaction-document-auto` | 10124（10096〜10254） | 4282（4259〜4421） | 0.423 |
| big `codeaction-document-1` | 10441（10422〜10505） | 5468（5436〜5505） | 0.524 |
| big `codeaction-document-8` | 9127（9032〜9246） | 4258（4248〜4319） | 0.467 |
| big `codeaction-fixall-auto` | 2229（2215〜2253） | 1027（1022〜1038） | 0.461 |
| big `codeaction-fixall-1` | 2572（2565〜2587） | 1320（1313〜1341） | 0.513 |
| big `codeaction-fixall-8` | 2242（2237〜2277） | 1021（997〜1302） | 0.456 |

action の題の一覧（`projects.json` の `titles`）は、5 つの要求と 3 つのスレッドの設定のすべてで before と after が同じだった。

**M3**: CLI の `check`（ms、`TSUZURI_CACHE_DIR=`）。どの workload もどちらのスレッドの数でも遅くなっていない（判定は「改善」か、1 スレッドの hello・point・polymorphism の「差なし」。広がりは 60〜70 ms で、中央値は 0.64〜0.74 倍）。

| workload・variant | before 中央値（最小〜最大） | after 中央値（最小〜最大） | after/before |
| --- | ---: | ---: | ---: |
| hello `check-1` | 118（114〜150） | 76（74〜146） | 0.643 |
| hello `check-auto` | 101（93〜133） | 57（54〜82） | 0.563 |
| hello-allstd `check-1` | 486（484〜524） | 198（187〜252） | 0.407 |
| hello-allstd `check-auto` | 407（403〜429） | 125（123〜131） | 0.307 |
| point `check-1` | 119（116〜186） | 81（73〜102） | 0.679 |
| point `check-auto` | 102（96〜117） | 69（59〜93） | 0.677 |
| polymorphism `check-1` | 120（117〜178） | 89（78〜98） | 0.742 |
| polymorphism `check-auto` | 106（96〜109） | 61（54〜86） | 0.573 |
| conventions `check-1` | 157（154〜213） | 96（93〜111） | 0.610 |
| conventions `check-auto` | 129（127〜146） | 74（67〜95） | 0.569 |
| json `check-1` | 333（327〜389） | 244（225〜285） | 0.734 |
| json `check-auto` | 274（272〜282） | 176（172〜180） | 0.642 |
| anonymous_unions `check-1` | 266（264〜306） | 164（155〜204） | 0.615 |
| anonymous_unions `check-auto` | 217（214〜235） | 115（114〜136） | 0.531 |
| list_fsharp `check-1` | 193（183〜237） | 126（124〜188） | 0.653 |
| list_fsharp `check-auto` | 154（151〜167） | 101（94〜113） | 0.658 |
| in_place_writes `check-1` | 263（257〜302） | 169（161〜219） | 0.643 |
| in_place_writes `check-auto` | 220（217〜232） | 124（122〜148） | 0.564 |
| big `check-1` | 889（878〜947） | 582（577〜632） | 0.655 |
| big `check-auto` | 759（755〜787） | 464（459〜484） | 0.611 |
| big-allstd `check-1` | 1315（1305〜1389） | 663（657〜754） | 0.504 |
| big-allstd `check-auto` | 1137（1132〜1161） | 517（514〜557） | 0.455 |
| syn200 `check-1` | 886（875〜972） | 377（366〜448） | 0.426 |
| syn200 `check-auto` | 776（771〜782） | 265（262〜270） | 0.341 |

**PX03**: `run-build.mjs` の `check`（ms）。12 の workload の wall の中央値は 0.29〜0.57 倍で、遅くなったものはない。代表の行を挙げる（全体は `report-phase1.md`）。

| workload・variant | before 中央値（最小〜最大） | after 中央値（最小〜最大） | after/before |
| --- | ---: | ---: | ---: |
| benchmarks/computations phase_time [check] | 80（79〜84） | 34（33〜35） | 0.422 |
| benchmarks/computations phase_time [total] | 96（94〜100） | 49（48〜50） | 0.507 |
| benchmarks/computations wall_time | 106（102〜107） | 56（55〜62） | 0.528 |
| examples/hello phase_time [check] | 73（72〜84） | 33（32〜34） | 0.450 |
| examples/hello phase_time [total] | 87（86〜101） | 45（45〜47） | 0.522 |
| examples/hello wall_time | 94（93〜112） | 53（52〜55） | 0.563 |
| synthetic (200) phase_time [check] | 652（643〜679） | 135（133〜151） | 0.207 |
| synthetic (200) phase_time [total] | 775（767〜816） | 261（251〜291） | 0.336 |
| synthetic (200) wall_time | 785（778〜826） | 272（262〜301） | 0.346 |
| synthetic (1000) phase_time [check] | 3442（3433〜3496） | 557（548〜612） | 0.162 |
| synthetic (1000) phase_time [total] | 4100（4092〜4147） | 1201（1189〜1257） | 0.293 |
| synthetic (1000) wall_time | 4114（4106〜4161） | 1213（1201〜1268） | 0.295 |

**M2・profile**: strip しない release を、before・after それぞれの src から別の target dir に作った（`CARGO_PROFILE_RELEASE_STRIP=false CARGO_PROFILE_RELEASE_DEBUG=line-tables-only`）。測り方は次のとおり。
- 1 つの server に再解析を続けて送りながら `sample <pid> 12 1` を取った（`target/scratch/pb08/measure/profile.mjs`）。
- `TSUZURI_TIME_PASSES=1` の累計は、再解析の回数で割った。
- frame の CPU 時間（すべてのスレッドの 1 ms の標本の数）も、`sample` の窓の中の再解析の回数で割った（`frames2.py`。再帰は 1 回と数える）。
- 記録は `target/scratch/pb08/measure/profile/`（`*.sample.txt`、`llvm-cxxfilt` で demangle した `*.dm.txt`、`frames.txt`、`*-passes.json`）。

| 1 回の再解析あたり（ms） | hello before | hello after | big before | big after | syn200 before | syn200 after |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 段 `check`（wall） | 490.1 | 133.2 | 932.3 | 221.3 | 2300.8 | 367.2 |
| 段 `specialize`（wall） | 55.3 | 29.4 | 296.2 | 212.6 | 123.4 | 88.4 |
| 段 `ownership`（wall） | 0.2 | 0.2 | 264.1 | 233.0 | 59.0 | 41.8 |
| 段 `parse`（wall） | 22.4 | 9.6 | 32.8 | 15.0 | 52.6 | 24.1 |
| `find::with_suggestions`（CPU） | 23.4 | 0 | 49.1 | 0 | 104.4 | 0 |
| `polymorph::mismatch`（CPU） | 12.7 | 0 | 28.3 | 0 | 28.4 | 0 |
| `check::type_display` など型の表示（CPU） | 23.8 | 1.2 | 69.4 | 6.4 | 69.6 | 9.8 |
| `warnings::unused_private`（CPU） | 163.9 | 4.6 | 411.3 | 9.8 | 1110.6 | 21.6 |
| `costly_sites` の下の 2 回目の `call_facts`（CPU） | 0 | 0 | 112.7 | 0 | 12.8 | 0 |
| `matching_instance`（CPU） | 58.5 | 31.9 | 139.1 | 69.4 | 133.8 | 83.2 |
| std を含む構文解析（CPU） | 11.7 | 22.9 | 18.7 | 39.7 | 28.8 | 70.0 |

捨てる診断を作る frame（`find::with_suggestions`。その下の `Names::type_pool` は inline されて名前が出ない。`polymorph::mismatch`、型の表示）と、`costly_sites` の下の 2 回目の `call_facts` は消えた（型の表示の残りは報告する診断と hover のもの）。
after の構文解析の CPU が増えたのは、O5 で 8 スレッドに分けたためで、段 `parse` の wall は半分以下になった。

**任意の案の判断（D1）**:
- **O2（`unused_private` の辺）: 行わない。** O1 の後の `unused_private` 全体が 1 回の再解析あたり 4.6・9.8・21.6 CPU ms（再解析の約 2.5・1.3・3.3 %）で、O2 が除けるのはその一部。M1 の広がり（14〜58 ms）より小さく、効果を計測で示せない。手順 6 の「std を通してだけ到達する私的な宣言」のテストは、O2 の安全網として決めたものなので、足さない。
- **O3(b)（instance の索引）: 行う（Phase 2 と一緒に、O6 より前のコミットで）。** `matching_instance` は after でも hello の再解析あたり 31.9 CPU ms（段 `check` の wall 133 ms に対して大きい）で、名前の分かる frame の中で最も大きい。起票時の見積もり（約 −30 ms）は M1 の広がりより大きい。
- **O8（std の構文解析の結果を再解析をまたいで使う）: 行わない。** O5 の後の段 `parse` の wall は hello で 9.6 ms。Phase 2（O6）で読む std が基本の 34 モジュール（約半分のバイト数）になると、さらに約半分になる見込み。O8 が除けるのはその一部で、M1 の広がり（hello 28 ms）より小さい。

**Phase 2**: D7 の条件（Phase 1 で目標に届かないときだけ）では行わないところだが、2026-11-20 の利用者の指示「もし実装に際し複数フェーズにまたがる場合には、その全てのフェーズを完了させること」で Phase 2 も行う（承認・D7・停止条件の記述を改める）。Phase 2 の後に M1〜M4 と PX03 をもう一度測り、ここの Phase 1 の値と分けて記録する。

### 文書

- `_tsuzuri/language-reference/compiler/option.md`: 「コンパイラのスレッド数」の表で `lsp` を「使える core 数（最大 8）」の側へ移し、言語サーバーの検証の再解析も同じと書いた。
- `_tsuzuri/language-reference/compiler/usage.md`: `lsp` の節にスレッドの数と、診断を先に送ることを書いた。
- `docs/architecture.md`: `src/copies.rs`・`src/lsp.rs` の行と、フロントエンドのスレッドの行。
- `docs/benchmarks.md`: 「言語サーバーの再解析（PB08、2026-11-20〜）」の節（実装した経路、測り方とコマンド、結果は計測待ち）。
- `docs/language.md`・`README.md` は誤りにならないので変えていない（README の「CLI は使える core 数」は `lsp` を含む CLI のコマンドの説明として正しくなった）。

### チケットから外れた判断

- golden の target は 442 個（起票時の 454 個は、試作の workload の写し 23 個を含んでいた。ここでは手順 1 の workload 12 個）。golden には、チケットの項目に加えて、知識グラフのファイル、定義・rename・`Matrx.init` の修正案を入れた。
- O4(b) の facts を運ぶ型は、チケットの例（`CheckedModule` と facts の組）の代わりに `copies::EditorModule` にした。field を `copies.rs` の私的なものにして、crate の中でも module を書き換えられない（警告を取り出すだけ）ようにするため。`check::check_modules_captured` は、facts の引数を足した `check_modules_with_capture` に統合した。
- O4(a) は公開の `costly_sites`（CLI の `--warn-implicit-copy`）にも効く（`costly_sites` だけの変種なので、チケットの範囲）。
- テストのために `cfg(test)` の計数と hook を足した（`find::TYPE_POOLS`、`warnings::NAMES_PROBE`、`polymorph::MISMATCH_TEXTS`・`COMPARE_INSTANCE_TRIALS`、`ownership::CALL_FACTS`）。release の binary には入らない。

### レビュー（2026-11-20）

独立したレビュー（HEAD `b073473a` と origin/main `295ffcb4` の比較）は、blocker・major の問題はなく、minor の指摘を 1 つ挙げた。レビューは自分で作った HEAD の release で、CLI の golden（自動、442/442）、言語サーバーの golden（未設定・`TSUZURI_THREADS=8`、10/10）、GPU のデバイス呼び出しの project と `effects_sweep` の transcript（2/2）、`parallel_frontend.mjs`（ok）、check が通る golden の target の `build --emit llvm -O0 --no-cache` の IR（180/180 が同じ）、`check` の最大 RSS（±1 % 以内）が before と同じであることと、新しい 8 つのテストがそれぞれの変更を戻すと失敗することを確かめた。

| 指摘 | 結果 |
| --- | --- |
| 1（minor）: `copies::tests::costly_sites_from_the_check_facts_are_the_recomputed_ones` の `SOURCES` は `tests/copy_cost.rs` の 19 の入力のうち 14 だけで、`MOVED_TWICE`・`SCALAR_RECORD`・`ARGUMENT_MOVED`・`EXPLICIT`・`EXPLICIT_LIST` が抜けていた（`EXPLICIT`・`EXPLICIT_LIST` は O4(a) の利用者の関数だけの filter が効く、std の関数が複製する例）。手順 5 の確認は「`tests/copy_cost.rs` の入力で」で、記録は「14 の入力」と書いていた | **直した**。コードで確かめると指摘のとおりだった。5 つを `tests/copy_cost.rs` と 1 byte も違わない literal で `SOURCES` に足し（`[&str; 19]`、`tests/copy_cost.rs` の順）、上の単体テストの表の記録を直した。`cargo test --locked --lib -- copies::tests::costly_sites_from_the_check_facts_are_the_recomputed_ones` は `TSUZURI_THREADS` 未設定・`=1`・`=8` で通った。検査するコードは変えておらず（テストの入力を足しただけ）、テストは base では build できない（新しい入口）ので、base で失敗することは確かめられない |

#### レビューの後の検証

- T0: `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings` は通った。`cargo test --locked --lib copies`（4 件）、`--test copy_cost`（16 件）・`--test gpu`（17 件）・`--test lsp`（52 件）が通った。
- T1: `node scripts/slot.mjs -- cargo nextest run --locked` は 3,359 件が通過、4 件が skip（1,938 秒。load average 50〜83）。
- T2: `cargo build --locked` の後、`node tests/lsp_sessions.mjs target/debug/tsuzuri` は 4 つの session が通り、`TSUZURI_BASELINE=target/perf/PB08/tsuzuri-before node tests/parallel_frontend.mjs target/debug/tsuzuri` は ok。`node scripts/check-links.mjs`（`--into _perfs/README.md` も）・`node scripts/check-guide-lite.mjs`・`node scripts/gen-docs.mjs --check` は clean。言語リファレンスと `src/runtime/` は変えていないので、`check-docs.mjs` と `check-runtime-includes.sh` は動かしていない。テストの入力を足しただけで release の binary は変わらないので、golden は作り直していない。

### 残っている問題

- 手順 8 は済んだ（上の「手順 8: 計測」）。syn200 の `tokens-8`（変更なしの `semanticTokens/full`）の 0.5 ms の差は、Phase 2 の計測でもう一度見る。
- G39 の Phase 3 で再解析を途中で取り消すなら、O5 で並列になった段の worker にも届くようにする（「落とし穴」）。
