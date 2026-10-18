# PB06: 常駐ビルドサーバーと watch モード

本文の起票時の設計（Phase 1 だけを実装する、`ResidentState` が G17 の `FrontendCache` と PB01 の `RuntimeObjects` を持つ、100 ms 級の目標など）と手順は調査履歴です。
現行の方針・決定事項・目標・停止条件は「着手時の再検討（2026-10-17）」と末尾の「実装と検証」を優先します。

| 項目 | 内容 |
| --- | --- |
| ID | PB06 |
| 分類 | ビルド速度 |
| 優先度 | P2 |
| 規模 | L |
| 依存 | PB02, (G17) |
| 関連 | G07, G12, G13, PB07 |
| 状態 | done（全 Phase 実装・出力の同値性検証済み。速度は共有機械での目安で、正式な測定は統合担当） |
| 起票 | 2026-09-29。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`）。2026-10-17 に HEAD 9dd0fbbc で再検討（「着手時の再検討」） |
| 承認 | 2026-10-17 の全フェーズ実装依頼で、D2（Phase 2 の着手。ローカル IPC の常駐サーバー）、D3〜D12 の既定案（「着手時の再検討」で改めた分を含む）、新設の D13〜D16 を含む全フェーズを承認済み（測定で効果を確認できない既定の切り替えは行わない。server は利用者が `tsuzuri serve` を起動したときだけ存在する opt-in で、server がなければ出力も挙動も今と同じ） |
| 手本にする既存実装 | 入力スレッド・`mpsc::sync_channel`・`recv_timeout` の主ループ: `src/lsp.rs` の `serve`。遅延実行: `Session::schedule`（200 ms）。上限つきの読み込み: `read_message`。私用ディレクトリ: `src/driver.rs` の `TemporaryDirectory::new`（`DirBuilderExt::mode(0o700)`）と `src/cache.rs` の `BuildCache::open`（`PermissionsExt`）。環境変数による置き場所の上書き: `cache::default_root`（`TSUZURI_CACHE_DIR`）。hash: `cache::Sha256`・`cache::file_digest`。オプションの検査: `src/main.rs` の `parse_arguments` の `--no-cache`。E2E: `tests/cache.mjs`（`cli`・`concurrent`）、`tests/lsp_sessions.mjs`（常駐プロセスとの対話） |
| 主な影響ファイル | `src/main.rs`, `src/lib.rs`, `src/watch.rs`（新規。Phase 1）, `src/server.rs`（新規。Phase 2）, `src/driver.rs`（Phase 2。常駐状態の受け渡し）, `src/cache.rs`（変更なし。`Sha256` を使う）, `src/lsp.rs`（変更なし。手本）, `tests/watch.mjs`（新規）, `tests/server.mjs`（新規）, `benchmarks/run-build.mjs`（PX03 が新規に作る。variant を足す）, `README.md`, `_docs/tools/command-line.md`, `_docs/tools/build-and-cache.md`, `docs/architecture.md`, `docs/benchmarks.md`, `_perfs/README.md` |
| 計測対象 | 1 モジュールの本体を変えた後の `check` と `build -O0` の時間（server 経由と `--no-server`）、watch の変更検出から完了までの時間、server の RSS と常駐 bytes |

## 目的

コンパイラの状態をプロセスをまたいで保持し、ファイルを保存してから結果が出るまでを短くする。目標は 1 ファイルの本体を変えた後の
`check` で 100 ms 級である（「目標と指標」）。

分担（調整役の決定）: G17 はディスク上のモジュール単位の前段 cache（parse 結果とモジュールの公開要約。content hash が key）を作る。
PB06 はそれと PB01 の事前ビルド済みランタイム object を常駐プロセスのメモリに保つ `tsuzuri serve` と、ファイルの変更で再実行する
`tsuzuri watch` を作る。PB07 はその上に関数単位の増分コード生成とリンクを作る。

結果（診断・stdout・stderr・終了コード・成果物）は常駐の有無によらずバイト単位で同じにする。server に何か問題があれば、CLI は黙って
プロセス内のビルドに戻る。LSP（G12）と REPL（G13）が server を使うかは各チケットで決める（対象外）。

実装者は Phase 1（`tsuzuri watch`）だけを実装する。Phase 2（`tsuzuri serve` と CLI の接続）は D2 の承認後、人間が求めた場合だけ着手する。

## 着手時の再検討（2026-10-17）

2026-10-17 の依頼で、全フェーズ（Phase 1 の `tsuzuri watch`、Phase 2 の `tsuzuri serve`・client・`--no-server`・常駐状態・PX03 の variant と計測、Phase 3 の各項目の採否）を実装する。上の「目的」の最後の段落（Phase 1 だけを実装する）は、この依頼で全フェーズに変わった。以降の「着手条件と停止条件」から「決定事項」までは起票時（HEAD `f8dc655`）の記録で、この節と食い違う箇所はこの節が優先する（旧 `_docs/tools/*.md` は `_tsuzuri/language-reference/compiler/` の各ページ。GUIDE §8.1）。

着手時の HEAD は `a2313d19`（main `9dd0fbbc` に計測の足場を足した commit）。偵察は `9dd0fbbc` の release compiler（`target/perf/phase4-baseline/tsuzuri-before`。9,465,008 bytes、SHA-256 `5e1b9687…`）と、同じ commit の strip しない build（`sample` 用）で、Apple M1 Max・macOS 27.0.1・Apple clang 21.0.0・rustc 1.98.1・Node 20.19.6、ほかの 6 つの作業と共有する機械（1 分の load average 9〜20）で行った。数値はどれも共有機の参考値で、標本数を併記する。生データと手順は `target/perf/PB06/`（commit しない）。静かな機械での正式な前後比較は統合後に行う。

### 起票時の記述と現行のコード

| 起票時 | 現行 | 影響 |
| --- | --- | --- |
| `src/main.rs` の出力は 20 か所。`Action` は 7 種。`run_formatter`・`run_test_action` の戻り値を `u8` にする | 4,439 行。`mod tests` の外に `println!`・`print!`・`eprintln!` が 44 か所（`eprintln!` 18、`println!`・`print!` 26）。`Action` は `Bench` が増えて 8 種（`Lsp`・`Check`・`Doc`・`Build`・`Run`・`Fmt`・`Test`・`Bench`）。`Action` の外に `script`・`toolchain info`・`new`・`fetch`・`publish`・`api-diff`・`bindgen`・`repl` の分岐がある。`ExitCode` は値を取り出せない | `cli` に移すのは `parse_arguments` より後だけ。`print_diagnostic`・`print_diagnostics`・`print_with_severity` は `write_*(err, …)` を新設し、既存の名前は標準エラーへ書く薄い wrapper にして `new` などの呼び出しを変えない。`cli` から呼ぶ `run_formatter`・`run_test_action`・`debug_test`・`run_bench_action`・`invalid_thread_setting` の戻り値を `u8` にする（約 40 か所の `ExitCode::SUCCESS／FAILURE`）。`script` の引数は `cli` の引数にする。server が実行するのは check と build だけで、ほかの action は parse の後に refusal する（D10 のまま） |
| `main` は `Project::load` → `analyze_all` → `print_diagnostics` → `run_action` を 1 回 | `Report::start`（TIME_PASSES）・`script` の引数の取り分け・`invalid_thread_setting`（`TSUZURI_THREADS`）の後、`Project::load`（`Test`・`Bench` は `load_for_tests`、`Doc` は `load_for_docs`、`script` は `load_script`）→ `with_manifest_wasm`・`validate` → `Project::analyze_cached(cache, kind)`（G17。`AnalysisKind::Program`・`Tests`・`Docs`）→ `links_apply` → `--warn implicit-copy`（W1006）→ 警告の表示と `--deny-warnings` → `run_test_action`／`run_bench_action`／`run_action`（`driver::build_linked`・`run_with_arguments`・`document`）→ `messages`（W2001） | `cli` の本体はこの全部。入口は `analyze_all` ではなく `analyze_cached` なので、常駐状態の差し込みは `analyze_cached` と `driver` の関数になる |
| `--no-cache is only valid with build or run` | `--no-cache is only valid with build, run, script, or repl`（check・test・doc は拒否）。`--no-server` は今 `unknown option` の E2000。`watch check hello`・`serve hello` は今も終了コード 2 の E2000（再現を更新済み） | `--no-server` の文面は `--no-server is only valid with check or build`、重複は `no-server specified more than once`。`src/main.rs` の `parse` のテスト（`mod tests` の `#[test]` は 21 件）は変えない |
| lib が標準エラーへ直接書くのは `TemporaryDirectory` の `Drop` と `run` の中継の 2 か所 | ① `TemporaryDirectory::drop`（`warning: cannot remove`。build で起こる）② `run_process`・`relay` の `io::stderr()`（run だけ）③ `timings::Report::drop`（TIME_PASSES。`main` が持つ）。`type_arena.rs` の `println!` はテスト。tool の出力は `run_tool` が `Command::output` で集めて `messages`（W2001）にする | check・build の経路に新しい直接出力はない（停止条件 3 は発火しない）。① は thread_local の sink で捕まえて `err` に流し、③ は要求ごとに `reset`／`take` して応答に載せる（D15）。panic の message は hook が server の標準エラーへ書くので、panic は `internal` で refusal して client が cold で再現する |
| `src/` に `Builder`・`stack_size` はなく、解析は main thread（8 MiB）。`thread::scope` は test_runner と cache のテストだけ | PB04 の `src/parallel.rs`: `ordered_for_each` は `std::thread::scope` ＋ `Builder::stack_size(8 MiB)` の worker を段ごとに起こして join する。CLI は `min(コア数, 8)`、library・LSP・REPL・`publish` は 1、`TSUZURI_THREADS` は呼び出しのたびに環境から読む（不正値は E2000、終了コード 2）。ほかに `bundle_identity`・`recursive`・`fetch` が scope、`capture` が `thread::spawn`。常駐する worker はない | D8（main thread が要求を 1 つずつ実行）は維持できる。worker は段ごとに作って捨てるので常駐に影響せず、`src/parallel.rs` は変えずに使える。`TSUZURI_THREADS` は関連する環境に入れる（D5） |
| 可変の `static` は `TEMP_COUNTER` だけ | `timings::PHASES`（Mutex。TIME_PASSES のときだけ貯まり、`Report` が最後に 1 回出す）と `ENABLED: OnceLock`、`frontend_cache::TEMPORARY_COUNTER`・`bundle_identity` の COUNTER（一時名の通し番号）、thread_local の `llvm_layout::{WEBASSEMBLY, MEMO}`・`target::CURRENT`（どちらも RAII の scope で戻る）、埋め込み runtime から決まる定数の `OnceLock`（`llvm.rs`・`runtime_cache.rs`）。型検査の interner は解析ごとの値 | 要求をまたいで無効化が要る cache 型の static はない。要求ごとに戻す必要があるのは TIME_PASSES の累積だけ |
| compiler の SHA-256（4,509,744 bytes。`shasum` 0.04 s）と環境変数 17 個。server は要求のたびに compiler を hash しない（D6） | compiler は 9,465,008 bytes。`cache::Sha256` は純 Rust（実測で約 200 MB/s）で、`build_key` は毎回 `file_digest(current_exe())` を呼び約 45 ms（`sample` で 44〜45）。`KEY_ENVIRONMENT` は 21 個（`TSUZURI_SYSROOT`・`TSUZURI_ZIG`・`TSUZURI_LLVM_CLANG`・`TSUZURI_RANGE_FLAGS` が増え、`TSUZURI_LINKER` は条件つき）。compiler が読む `TSUZURI_*` は 15 個 | server が自己点検で hash しないだけでは足りない。`build_key` の `file_digest` が同じ約 45 ms を毎回払うので、`file_digest` の stat 署名による memo が常駐状態に要る（D11） |
| G17 の `FrontendCache` は (1) プロセスで一度作って `&mut` で渡せる (2) key が compiler の同一性を含む (3) bytes を返す (4) 壊れていれば計算し直す | (2)(4) は成り立つ。(1) は成り立たない: `Project::analyze_cached` が解析のたびに `FrontendCache::open`（pack の読み込み）→ `lookup`／`commit` → `finish`（pack と manifest の書き込み。内部を空にする）を 1 回だけ行う単発の値。(3) はない。compiler の同一性は `open` のたびに `current_exe` の stat（len・mtime・dev・ino・ctime）から作る。`FrontendDelta::must_recheck` は `dead_code`（「PB06 と PB07 の再利用規則」）で、check 結果の再利用は G17 の D10 が先送りした | 「startup の identity を固定して `open_with` する」と「pack を要求の間メモリに残す」を別々に考える。pack の保持は効果が小さい見込みなので測ってから決める（gated。D11）。`FrontendCache` は process 全体の static ではないので停止条件 5 は発火しない |
| PB01 Phase 1 の `RuntimeObjects`（事前ビルド済み object を返す値）を常駐に含める | そういう値はない。object は `compile_runtime_object(unit, source, …)` がディスクの cache から復元する。build の miss の経路の `sample` では `compile_runtime_object` が約 12 ms（1 要求の約 1%）。同梱 toolchain の識別は PB03 が `bundle_identity` でディスクに memo 済み（14.6 s → 0.2 s） | `RuntimeObjects` は常駐させない（取り下げ。D11） |
| hello の `check` は 0.02〜0.03 s。200 module は 0.39 s、1,000 module は 2.12 s（RSS 237 MB・1.12 GB） | （baseline、`TSUZURI_TIME_PASSES=1`、中央値）process の起動（`--version`）は 3.6 ms、hello の `check` は 27.4 ms（25 標本）で、うち標準ライブラリの解析が約 21 ms。200 module の `check` は cold 237 ms、編集後（frontend cache あり。9 標本）268 ms（259〜303）。1,000 module は cold 1,599 ms、編集後 1,368 ms（3 標本） | 比較の基準はこの値。hello は server があっても標準ライブラリの解析（約 21 ms）が残る |
| G1「1,000 module で 1 module の本体を変えた後の server 経由の `check` を 100 ms 以下」 | 達成できない。編集後の 200 module の内訳は load 19、cache 19、parse 6.5、check 125、specialize 53、closures 9、ownership 8 ms（合計 262）。全体を通す段（check・specialize・closures・ownership）は約 195 ms で、1,000 module では約 865 ms 残る。G17 は parse と interface 要約だけで、check 結果の再利用は先送りした（`must_recheck` は未使用）。D11 も `CheckedModule` を常駐させない | G1 を取り下げる。常駐で消せるのは load・cache・parse・compiler の hash・process の起動で、見込みは 200 module で約 12〜16%、1,000 module で約 27〜34%（後者は下の `open()` の費用による。いずれも未検証の見込み）。「100 ms 級」は check 結果の再利用（G17 Phase 2・PB07）が要る |
| `Project::load` の費用は問題にしていない。watch は 1 s ごとに全ファイルを読み直す（CPU 5% を超えたら directory の mtime に変える。`ponytail`） | この機械（セキュリティ agent の on-access scan とみられる）で、新しい process の `open()` は 1 file 0.3〜0.7 ms、1 process あたり 400 file 程度を超えると同じ file を繰り返し開いても 0.17〜0.5 ms。`load` の段は 100・200 module で 4.7・8.5 ms（47・43 µs/file）、400 以上で 198〜283 ms（280〜496 µs/file）。1,000 module の `check` の約 45% が `read_source_text` の `open`（`sample` で 646/1,438 samples のうち 622）。`lstat` は 2〜4 µs/file で、署名が同じなら開き直す必要がない。`frontend_cache.rs` の冒頭も同じ現象を書いている | 常駐の主な利得は `open()` を避ける SourceMemo（新設。D11）。watch は 400 module 超で毎秒の全体読み直しが CPU 約 20〜60% になり、起票時の自分の条件（5%）を超える → D9 を stat 署名中心に改める。効果は環境依存（Linux の開発機では 1,000 file で約 25 ms）なので、結果は環境つきで記録する |
| 環境は `IGNORED_ENV`（`_`・`OLDPWD`・`SHLVL`）を除いて完全一致のときだけ受ける（D5） | この shell の環境は 52 変数で、`SSH_AUTH_SOCK`・`MACH_PORT_RENDEZVOUS_PEER_VALDATION`・`__CFBundleIdentifier`・`COPILOT_AGENT_SESSION_ID`・`XPC_SERVICE_NAME`・`PWD` のようにセッションやアプリごとに違うものが多い。server は前面の別の端末で起動する前提（D3）なので、完全一致では別の端末やエディターの task からの要求がほとんど断られる | D5 の比較を「コンパイラと tool が読む変数」に絞る。`std::env::set_var` が `unsafe` で環境を要求ごとに差し替えられない点は同じ |
| LSP は変更のたびに全体を再解析し、増分の状態を持たない | 同じ。解析は `analyze_inputs_semantic`（frontend cache を使わず、`Parallelism::library()` で 1 thread）。`Session::refresh` は `Project::load_with_overlays` で、開いていないファイルを毎回開く | LSP と server は cache も解析の経路も共有しない。LSP が得をするのは SourceMemo だけ（D16） |
| PX03 の `VARIANTS`・`measureVariant` に 5 variant を足す。env から `TSUZURI_TIME_PASSES` を除く | `VARIANTS` は `check`・`emit-llvm`・`build`・`build-warm`・`build-edit-body`・`build-edit-signature`・`build-runtime-*`。`check-edit-body` はない。env は `TSUZURI_TIME_PASSES=1`・`TSUZURI_CACHE_DIR` を持ち、`parseTimePasses` が最後の `time_passes` 行を読む | server の応答に要求ごとの `time_passes` の行を載せれば（D15）env を変えずに段別の時間が取れる。`check-edit-body-{server,no-server}`・`build-edit-body-{server,no-server}`・`watch-edit-body` を足す |
| `_docs/tools/command-line.md`・`build-and-cache.md` を更新する | `_tsuzuri/language-reference/compiler/usage.md`（`### check`・`### build`・`### lsp`・「ビルドキャッシュ」）・`option.md`・`diagnostics.md`（E2000・E2001 の行）。ほかに `README.md` の「CLI リファレンス」と「検証と性能測定」、`docs/architecture.md`、`_features/GUIDE.md` の D-30 の台帳（`tsuzuri watch`・`serve`、`--no-server`・`--idle-timeout`、`TSUZURI_SERVER_DIR` が仮の名前のまま） | 文書の対象をこの一覧に改める。D-30 の 3 行は「承認済み・実装済み」に更新する |
| 手本は `tests/cache.mjs`・`tests/lsp_sessions.mjs` | 加えて `tests/parallel_frontend.mjs`（PB04。`targets.yml` が実行する）と `tests/frontend_cache.mjs`（G17）が環境と cache を扱う手本。Windows CI は `tests/cache.mjs` を実行する。`rust-version` は 1.85、CI は 1.99.0 の lint。Windows の target は rustup の stable（1.96.1）にだけあり（1.99.0 にはない）、`rustup run stable cargo check --all-targets --target x86_64-pc-windows-msvc` で型検査する | `tests/watch.mjs` は Windows でも動く形（CRLF 安全）にして `windows.yml` と `targets.yml` に、`tests/server.mjs` は Unix だけ（Windows は理由つきの skip）で `targets.yml` に配線する案を最終報告に書く |

### 偵察で測った事実（2026-10-17、baseline compiler。生データは `target/perf/PB06/`）

- 200 module の `check`（`TSUZURI_THREADS` 未設定、9 標本、中央値と最小〜最大）: 編集後 268 ms（259〜303）。内訳は load 19.2、cache 19.4、parse 6.5、check 124.7、specialize 52.9、closures 8.8、ownership 8.3 ms。`TSUZURI_THREADS=1` は 397 ms（check 206、parse 24.7、ownership 28.9）。cold（cache なし）237 ms、編集なしの warm 229 ms（5 標本）。
- 200 module の `build -O0 --cpu generic`（5 標本）: cold の cache で 1,921 ms、artifact cache の hit が 644 ms（cache 144、emit 217）、編集後の miss が 1,639 ms（tool.clang 922、cache 184、emit 218）。1,000 module（3 標本）は hit 3,932 ms、編集後 8,153 ms（tool.clang 4,209、emit 2,096、cache 392）。
- `build` の `sample`（200 module、1 ms 間隔、PID 指定）: hit は `emit_program` 179、`build_key` の closure 79（`file_digest` 44、`Sha256::field` 35、`tool_identity` 22）、`BuildCache::load` 77（うち `rename` 67）、`evict` 24。miss は `run_tool` 808、`emit_program` 187、`build_key` 83（`file_digest` 45、`tool_identity` 36）、`BuildCache::store` 42、`compile_runtime_object` 12。
- `load` の段の規模依存（`check`、frontend cache あり、5 標本の中央値）: 100 module 4.7 ms、200 は 8.5、400 は 198、600 は 228、800 は 283、1,000 は 280 ms。400 を境に 1 file あたりが約 10 倍になる。
- `open()` の単体測定（C の小さな program、新しい process）: 200 file の最初の走査が 145 ms（722 µs/file）で、2 回目以降は 5 ms（26 µs/file）。400 file は最初が 163 ms、以降も毎回 70 ms。1,000 file は毎回 175〜182 ms。信頼された Node の `readFileSync`（1,000 file）は 30 ms、`lstat` は 2〜4 µs/file。新しく build した compiler の最初の実行では、200 module の `load` が `sample` つきで 933 ms だった（process の識別ごとに判定が貯まるように見える。原因の確認はしていない）。cold の CLI は 400 file を超えると毎回払い、常駐 process は署名が変わらない file を開かない。
- 1,000 module の `check` の `sample`（1 ms 間隔、1,438 samples）: `Project::load` 646、`analyze_cached` 792。
- 起動の床: `--version` 3.6 ms（25 標本）、hello の `check` 27.4 ms。

### 方針の改訂

**Phase 1（実装する）**: 設計のとおり `cli(raw, program_arguments, out, err)` と `tsuzuri watch check|build|test` を作る。状態行は起票時のまま（text: `watch: run <n> finished with exit code <code>`、`--json`: `{"type":"watch","run":<n>,"exit":<code>}`、どちらも標準エラー）。snapshot は `cli` の前に取り、`Project::load` の失敗（エラーの本文）も snapshot の一部にする（本文が変わる・直るで再実行する）。変える点は検出の方式だけ（D9）。Phase 1 だけでは解析を使い回さない点は同じで、`ResidentState` を受け取る口は WP3 で足す。

**Phase 2（実装する。D2 承認済み）**: 構成は起票時のまま（accept 側＋main thread、protocol v1、1 接続 1 要求、`status`・`shutdown`、client の黙った fallback）。変える点は D4〜D6・D11・D13〜D15。常駐状態は `src/resident.rs` に置き、`driver.rs` の関数は増やさず thread_local の scope で差し込む（D14）。server は opt-in で、既定の出力と挙動は server がなければ今と同じなので、既定の切り替えの条件（測定と fallback）は問題にならない。速さは測って記録するだけで、G1 は取り下げる。

**Phase 3（判断）**: D16 のとおり。socket での LSP・REPL との共有、Windows の named pipe、tool の問い合わせの memo は採らない。LSP・REPL が自分の process の中で `ResidentState` を使う案だけ、測って決める（WP9）。

### 決定事項の更新

- D1（Phase 分割）: 変更。Phase 1 と Phase 2 の両方を実装する。Phase 1（WP1〜WP2）を先に完成させて commit し、Phase 2 はその後。`watch` は WP3 で `ResidentState` を引数に取る形へ拡張する。
- D2（Phase 2 の着手）: 承認済み。server は opt-in で、client が接続するのは socket が `FileTypeExt::is_socket` のときだけ（G3）。server がなければ出力も挙動も今と同じ。
- D3（起動方式）: 維持（前面で起動、CLI は起動しない。起動の失敗は終了コード 1、引数の誤りは 2）。追加は D13。
- D4（通信路と protocol）: 維持（UDS・改行区切りの JSON・`PROTOCOL = 1`・要求 1 MiB・応答 16 MiB・1 接続 1 要求）。変更: ① socket 名は `<SHA-256(canonical root) の先頭 16 桁>.sock`（32 桁から短縮して `sun_path` の余裕を 16 bytes 増やす。hello に `root` を足し、client が照合するので衝突しても別 project には繋がらない）② 要求の読み取りに合計 2 s、応答の書き込みに 10 s の期限を設ける（止まった client が server を塞がない。client の応答待ちは起票時のとおり期限なし）③ `--no-server` は非 Unix でも受理する（何もしない）。非 Unix の `serve` は E2000。
- D5（置き場所と安全性）: 変更。ディレクトリの既定と 0700・symlink でない・probe の作成は起票時のまま。追加・変更: ① `TSUZURI_SERVER_DIR=`（空）は server を使わない意味にする（`TSUZURI_CACHE_DIR=` と同じ。client は接続せず、`serve` は E2000）② 私用の判定に「ディレクトリの `uid()` が probe ファイルの `uid()` と同じ」を足す（std の `MetadataExt::uid` だけで自分の uid が分かる）③ 環境の比較を、コンパイラと tool が読む変数に絞る: `cache::KEY_ENVIRONMENT`・`TSUZURI_*`・`HOME`・`XDG_CACHE_HOME`・`LOCALAPPDATA`・`TMPDIR`・`TEMP`・`TMP`・`LD_*`・`DYLD_*`・`CLANG_*`・`CCC_*`・`ZIG_*`・`TOOLCHAINS`。`src/` の `env::var`／`var_os` の名前がこの集合に含まれることを確かめる drift test（`src/` の文字列を走査する Rust テスト）を置く。client は関連する変数だけを要求に載せ、server が比べる ④ `cwd`・compiler の canonical パス・project root は完全一致のまま ⑤ umask・rlimit など process の属性は server のものになる（文書に書く）。
- D6（寿命）: 維持（idle 既定 1800 s、`--idle-timeout` は 1〜86400、常駐 1 GiB）。変更: compiler の自己点検は G17 の identity と同じ stat 署名（len・mtime・dev・ino・ctime）を起動時の値と要求ごとに比べる（違えば `compiler-changed` を返して終わる）。`FrontendCache` には起動時の identity を渡す（途中で差し替わった binary の identity で pack を書かない）。SHA-256 は起動時に 1 回だけ計算して `status` に出す。
- D7（正しさと fallback）: 維持（server の問題は client の黙った fallback。verify mode は作らない）。client は理由を表示せず、server のログと `serve --status` の `refused` で見る。
- D8（並行性と stack）: 維持。理由を PB04 後に読み替える（要求は main thread が実行し、worker は段ごとの scope で 8 MiB）。要求の処理は `catch_unwind` で守り、panic したら `internal` を返して `ResidentState` を捨てる。
- D9（ファイル監視）: 変更。検出は stat 署名（Unix: dev・ino・len・mtime・ctime。非 Unix: len・mtime）。100 ms ごとに既知の file の署名を比べ、1 s ごとに発見（追加・削除）をやり直す。署名が変わった file と racy な file（署名の時刻が読んだ時刻の 2 s 以内）だけ内容を読み、同じなら再実行しない（touch の吸収）。非 Unix は ctime がないので、起票時の「10 周期ごとの全体の内容比較」を残す。周期は 100 ms のまま、PX03 で CPU を測って 5% を超えたら file 数で伸ばす。新しい crate は使わない。
- D10（watch の CLI と server の対象）: 維持（`watch check|build|test`、server は check と build だけ）。
- D11（常駐状態の中身）: 変更。常駐する: ① SourceMemo（path → 署名と本文。`read_source_text` が `stat` した直後に引く。成功した読みだけを覚え、エラーは毎回本物の読みで出す）② DigestMemo（`file_digest` の署名つき memo。compiler と tool の binary）③（gated）frontend の pack。取り下げ: `RuntimeObjects`（約 1%）、tool の `--version`・`-mcpu=native` の問い合わせの memo（`/usr/bin/clang` のような shim は file の署名では中身の変更が見えず、価値は約 2%）。常駐させない: `CheckedModule`・IR・成果物。
- D12（計測）: 変更。variant は `check-edit-body-{server,no-server}`・`build-edit-body-{server,no-server}`・`watch-edit-body`。env は PX03 のまま（`TSUZURI_TIME_PASSES` を外さない。D15）。M4・M5 は `target/perf/PB06/`。
- D13（新設）: `tsuzuri serve --status DIR`・`--stop DIR`。protocol の `status`・`shutdown` を client として送る（DIR は project directory）。raw protocol を話せない利用者が server の状態を見て止められる。非 Unix は E2000。
- D14（新設）: 常駐状態の差し込みは `src/resident.rs` の thread_local の scope（`PlatformScope`・`target_scope` と同じ RAII）で行い、`build_complete`・`Project::load*` など他チケットが編集する関数の引数を増やさない。cold の CLI は scope に入らないので、処理も出力も今と同じ。
- D15（新設）: server の出力を cold と exact にする 2 つの手当て。① `TemporaryDirectory::drop` の警告を thread_local の sink へ流し、scope があれば `err` へ書く。② `TSUZURI_TIME_PASSES` は要求ごとに `timings` を reset／take し、応答の標準エラーの最後に `time_passes` の 1 行を足す。client は応答を使うとき自分の `Report` を出さない（server と client の両方が `=1` のときだけ。違えば `env` で refusal）。
- D16（新設。Phase 3 の採否）: ① LSP・REPL が server と socket で話す: 対象外のまま ② LSP・REPL が自分の process の中で同じ `ResidentState`（SourceMemo・DigestMemo）を使う: gated（WP9。REPL の cache hit の再評価で 5% 以上、または LSP の 1,000 file の `load` が 100 ms 以上減るなら採用し、そうでなければ理由を記録する）③ Windows の named pipe: 採らない（std に server 側の API がなく、Win32 の FFI か crate が要る。`unsafe` と新しい crate の禁止に反する）④ `build_key` の tool の問い合わせの memo: D11 で取り下げ（file の digest だけ memo する）。

### 作業の順序（コミット）

| 作業 | 内容 | 確認 |
| --- | --- | --- |
| WP0 | この再検討（このチケットだけ。`_perfs/README.md` は統合担当が更新する） | `git diff --check` |
| WP1 | `cli(raw, program_arguments, out, err)`・`write_*`・`u8`（出力は変えない） | `cargo test --locked --bin tsuzuri` の件数が同じ。baseline との出力比較（check・build の成功・失敗・`--json`・`--deny-warnings`・`--warn implicit-copy`、`test`・`bench`・`fmt`・`doc` の代表）の diff が空。`node tests/cache.mjs`・`tests/e2e.mjs` |
| WP2 | `FileStamp`・`src/watch.rs`・`tsuzuri watch`・`tests/watch.mjs`・文書（Phase 1 の完了） | `cargo test --locked --lib watch::`・`--bin tsuzuri watch_arguments`・`node tests/watch.mjs`・`cmp` |
| WP3 | `src/resident.rs`（`ResidentState`・`Scope`・SourceMemo・DigestMemo）と hook（`read_source_text`・`file_digest`）。`watch` へ接続 | 単体（racy の署名、同じ長さで時刻を戻した書き換え、エラーの文面が同じ）、`tests/watch.mjs`、`load` の段の前後 |
| WP4 | `src/server.rs`: protocol・私用ディレクトリ・socket 名・環境の比較（drift test）。非 Unix の分岐 | `cargo test --locked --lib server::` |
| WP5 | `serve` の主ループ・`status`／`shutdown`・client `request`・`--no-server`・`serve --status／--stop` | 偽の handler で主ループを動かす単体（queue・busy・期限・idle・stale socket） |
| WP6 | `tests/server.mjs`（一致・拒否・protocol・偽の server・stale・二重起動・権限・寿命・並行・常駐状態） | `node tests/server.mjs target/release/tsuzuri` |
| WP7 | D15（TIME_PASSES の要求ごとの report、`TemporaryDirectory` の警告の sink）。gated: frontend の pack の保持 | 単体と `tests/server.mjs` の追加 |
| WP8 | PX03 の variant・計測・`docs/benchmarks.md`（`phase4:PB06` の区間） | 9 標本の交互計測、M1〜M6 |
| WP9 | gated: REPL・LSP が自分の process で `ResidentState` を使う。または採らない理由の記録 | `benchmarks/run-repl.mjs`・`tests/lsp_sessions.mjs` |
| WP10 | 文書（言語リファレンス・README・architecture・HELP・GUIDE の D-30）、全体の確認、`_completed/` への移動 | `node scripts/check-docs.mjs`、`cargo test --locked`、各 suite |

### 他チケットとの重なり（統合の順は PR07 → PR08 → PB06 → PM02 → PM04 → PM03 → PB05）

- `src/main.rs`: PR07（`--profile-generate／--profile-use`）・PR08（`--emit bitcode`）・PB05（`--backend`）・PM08 の続きが `parse_arguments`・`Arguments`・`HELP`・`run_action` を編集する。PB06 は `main` の本体を `cli` へ移し、`Arguments` に `no_server` を足す。移すのは文面を変えない範囲にとどめ、他の関数の整形・並べ替えはしない。衝突は textual で、意味の衝突はない（server は `driver::build_linked` が返すものをそのまま使う）。
- `src/driver.rs`: PR07 の `build_profiled`・PR08 の `Emit::Bitcode`・PB05 の `Backend` は `build_complete` の中。PB06 の変更は `read_source_text` の hook、`TemporaryDirectory::drop` の 1 行、`Project::analyze_cached` の identity で、`build_complete` には触れない。`src/cache.rs`（PR07 が key に触る）では `file_digest` の hook だけ。
- `src/lib.rs`: `mod` の行を足す（衝突は自明）。`docs/benchmarks.md` は `phase4:PB06` の区間だけ。
- PB05 が足す環境変数・tool は `TSUZURI_*` の接頭辞か `KEY_ENVIRONMENT` に入るので、server の「関連する環境」の比較に自動で含まれる（drift test が見張る）。PM02・PM03・PM04 は IR の中身だけで、server の経路と無関係。

### 受け入れ条件の変更

- 取り下げ: G1（100 ms 以下）。代わりに、server と `--no-server` の `check`・`build -O0` を同じ編集で交互に測り、段ごとの差と、残る全体の段（check・specialize・closures・ownership）の時間を記録する。
- 追加 G5: 400 module 以上で、server と watch の `load` の段が署名の比較だけになること（SourceMemo。環境つきで記録）。
- G2（watch の検出）は、Unix は 1 周期（100 ms）以内、追加・削除は 1 s 以内。非 Unix は内容の比較が 1 s ごと。
- 「出力が byte で同じ」は、IR・object・wasm・`.trap.json` は exact、実行ファイルは PX03 の `artifactFingerprint` と実行結果（Mach-O の既知の差を除く）で確かめる。stdout・stderr・終了コード・診断は exact。
- Phase 2 の受け入れに、環境の drift test、`TSUZURI_TIME_PASSES` を通した応答、警告の sink の単体テストを足す。

### 停止条件の更新

- 3 は再確認して発火しない（上の表）。5 は発火しない（`FrontendCache` は static ではなく単発の値）。7 は socket 名を 16 桁にして余裕を作った（この利用者の既定は `/Users/<name>/Library/Caches/tsuzuri/server/` の 49 文字＋21 文字で合計 70 文字。利用者名が 44 文字までなら macOS の上限 103 文字に収まる）。超えるときは起票時のとおり E2000。
- 追加: SourceMemo が cold と違う本文や診断を返すテストが 1 件でも作れたとき（署名が同じで内容が違う、エラーの文面が違う）は SourceMemo を外し、証拠を記録する。`SourceFile::read`・`read_source_text` のエラーの文面と path は 1 byte も変えない。
- 読み替え: 起票時の 4 のうち「main thread 以外で動かす必要」は、PB04 の worker があるので「要求を処理する thread の stack が 8 MiB 未満になる」と読む（D8 は main thread のまま）。

### 実装中に測って決めること（gated）

1. frontend の pack を要求の間メモリに残すか（D11 ③）: SourceMemo・DigestMemo の後の server の `check` で `parse`＋`cache` が 10% を超えるときだけ実装する。書き込みの遅延は行わない（ディスクの状態を cold と同じに保つ）。
2. racy の窓の長さ（2 s で始め、HFS+ など 1 s 粒度の file system を想定）と watch の周期（CPU 5% 超なら file 数に応じて伸ばす）。
3. REPL・LSP の採用（D16 ②）。
4. `-mcpu=native`・tool の `--version` の問い合わせの費用が `--cpu native`（既定）で無視できないとき（WP8 の測定）は、memo ではなく別チケットとして記録する。

### 未検証・制約

Linux・x86-64・Windows は実機がなく、Windows は `watch` の型検査（`x86_64-pc-windows-msvc`）と非 Unix の分岐の単体テストだけ。`open()` の費用はこの機械の環境（セキュリティ agent）の性質なので、一般化せず環境つきで記録する。server は同じ OS 利用者だけを信頼する（0700 のディレクトリが唯一の防御。peer の資格は libc なしでは確かめられない）。umask・rlimit などの process の属性は server のものになる。

## 着手条件と停止条件

### 着手条件

- PB02 が `_perfs/README.md` の状態欄で done であること（メタデータの依存）。PX03 も done であること（PB02 の依存。計測は PX03 の
  `benchmarks/run-build.mjs` に variant を足して行う）。確認: `grep -n "PX03\|PB02\|PB06" _perfs/README.md`。
- Phase 2 だけ: D2 が承認済みであること。G17 のモジュール単位の前段 cache（G17 の「Phase 2: モジュール単位の解析キャッシュ」に当たる段）が
  `_features/README.md` の状態欄で done であること。確認: `grep -n "| G17 " _features/README.md`。PB01 Phase 1 は任意（done なら常駐状態に含める）。
- GUIDE §2.3 の基準コマンドが成功し、手順 1 のベースラインを保存していること。

### 前提とする他チケットのインターフェース

- PX03（計測）: `benchmarks/run-build.mjs` の `VARIANTS`・`measureVariant`、`benchmarks/build/generate.mjs` の `moduleSource(m, { salt })`、
  PX01 の `measureProcess(command, args, { cwd, env, runs, warmups, timeoutMs })`（`wall_ms`・`peak_rss_bytes` を返す）と
  `writeRecords(directory, suite, records)`。名前が違えば PX03 の実装に合わせて読み替える。
- G17（Phase 2）: 以下 `FrontendCache` と呼ぶ値（G17 の実際の名前に読み替える）。(1) プロセスで一度作り、解析の関数へ `&mut` で渡せる。
  (2) 項目の key がソースの内容の hash・コンパイラの同一性・解析に効く指定を含み、PB06 が無効化しなくても使い回しが安全である。
  (3) メモリ上の大きさを bytes で返す関数がある。(4) 項目がない・壊れているときは計算し直す。ディスクの層は G17 のものを使う。
- PB01（Phase 2、任意）: 事前ビルド済み object を返す値（以下 `RuntimeObjects`）が (1)(2)(4) と同じ性質を持つ。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

1. `unsafe` か新しい crate（`notify`・`libc`・`interprocess`・`windows-sys` など）が必要になった。std の `std::os::unix::net` と
   `std::os::unix::fs` だけで作る（D4・D9）。
2. server 経由と `--no-server` で、診断・stdout・stderr・終了コード・成果物のどれかが 1 byte でも違う。ただし 2 回の cold build 同士でも
   違うもの（既存の非決定性）は、その事実を報告して比較から外すかを人間に確認する。
3. check または build の経路で、lib が `eprintln!`・`io::stderr()` へ直接書く箇所が「現状」の 2 か所のほかに見つかった（server の応答に
   載らない出力になる）。
4. 解析を main thread 以外で動かす必要が出た（D8）。stack サイズを上げて回避しない。
5. G17 の cache がプロセス全体の `static` を使う、または 1 回の解析ごとに作り直す前提で、常駐プロセスで使い回せない。
6. 既存テストの期待値（`tests/cache.mjs`、`tests/lsp_sessions.mjs`、`src/main.rs` の `parse` のテスト）を変える必要がある。
7. socket の既定のパスが 100 bytes を超える環境が CI にある（`sun_path` は macOS 104 bytes、Linux 108 bytes）。
8. 計測のために `src/` に計測専用のオプションや隠し環境変数を足したくなった。

## 現状と計測（HEAD `f8dc655`）

### CLI の流れ

- `src/main.rs` の `main` は `parse_arguments` の後、`Project::load`（`test` は `Project::load_for_tests`、`doc` は `Project::load_for_docs`）→
  `Project::analyze_all` → `print_diagnostics`（警告）→ `run_action`（`driver::build`・`driver::run_with_diagnostics`・`driver::document`）
  または `run_test_action` を 1 回だけ実行して終わる。状態は残らない。
- 出力は `print_diagnostic`・`print_diagnostics`・`print_with_severity` と `main` の message のループが `eprintln!`・`println!` で直接書く
  （`src/main.rs` に計 20 か所）。書き先を差し替える引数はない。
- `Action` は `Lsp`・`Check`・`Doc`・`Build`・`Run`・`Fmt`・`Test`。先頭の語がどれでもなければ `build` として読む（HELP の
  `tsuzuri [build] source`）。`watch`・`serve` は今は入力パスとして読まれる（再現）。
- `--no-cache` は build と run だけで受け、ほかは `--no-cache is only valid with build or run`（`E2000`、終了コード 2）。
- `Project::load` は入力が symlink なら `E1011`、ディレクトリなら `Main.tz` を足し、その親をルートとして `load_from_root` を呼ぶ。
  `SourceFile` は `path`・`relative_path`・`name`・`text`・`origin`（`ModuleOrigin::User`・`ModuleOrigin::Std`）・`package` を持ち、std は
  仮想パス `std/<Name>.tz` でディスクにない。

### LSP（手本）

- `src/lsp.rs` の `serve` は入力を読むスレッドを `std::thread::spawn` で 1 本作り、`mpsc::sync_channel(8)` で main thread へ渡す。解析
  （`Session::refresh`）は `serve` を呼んだ thread で動く。`Session::schedule` は 200 ms 後に refresh を予約し、主ループは `recv_timeout` で待つ。
- 変更のたびにプロジェクト全体を再解析し、増分の状態は持たない（G07）。

### スレッドと stack

- `src/` に `std::thread::Builder`・`stack_size` の使用はない。`std::thread::spawn` は `src/lsp.rs` の入力スレッドだけ、`std::thread::scope` は
  `src/driver.rs` の `mod test_runner`（テストの並列実行）と `src/cache.rs` のテストだけにある。解析は CLI・LSP とも main thread
  （macOS・Linux の既定 8 MiB）で動く。debug build の Rust テストは 2 MiB の thread で動く。

### プロセス全体の状態と直接の出力

- 可変の `static` は `src/driver.rs` の `TEMP_COUNTER: AtomicU64`（一時ディレクトリ名 `.tsuzuri-<pid>-<counter>`）だけ。`src/llvm.rs` の
  `FIRST_METADATA: OnceLock<usize>` は埋め込みランタイムから決まる定数。`process::exit` の呼び出しはない。
- lib が標準エラーへ直接書くのは `TemporaryDirectory` の `Drop`（`warning: cannot remove ...`）と、`run` の子プロセスの出力の中継
  （`io::stderr()`）の 2 か所だけ。

### cache と環境

- `cache::build_key` は compiler の SHA-256（`file_digest(&std::env::current_exe()?)`）、`BuildOptions` の `Debug` 表現、IR、全ソースと
  manifest、環境変数 17 個（`TSUZURI_CLANG`・`TSUZURI_WASM_LD`・`TSUZURI_LLVM_LINK`・`TSUZURI_DSYMUTIL`・`PATH`・`PATHEXT`・`SDKROOT`・
  `MACOSX_DEPLOYMENT_TARGET`・`INCLUDE`・`LIB`・`LIBPATH`・`CPATH`・`C_INCLUDE_PATH`・`CPLUS_INCLUDE_PATH`・`LIBRARY_PATH`・
  `SOURCE_DATE_EPOCH`・`DEVELOPER_DIR`）、ツールの digest と `--version` の出力を hash する。起動するツールは環境全体を継承する。
- `cache::default_root` は `TSUZURI_CACHE_DIR`、なければ macOS `$HOME/Library/Caches/tsuzuri/build-cache`、Linux `$XDG_CACHE_HOME` か
  `$HOME/.cache` の下の `tsuzuri/build-cache`、Windows `%LOCALAPPDATA%\Tsuzuri\Cache\build-cache`。
- whole-build cache（G11）は解析と IR 生成の後に照合するので、hit しても解析の時間は残る。parse・check・IR の cache はない
  （docs/architecture.md の「不変条件」）。

### 依存と制約

- `Cargo.toml` の依存は `rustc_apfloat`・`serde_json`（Windows だけ `same-file`）。`[lints.rust]` は `unsafe_code = "forbid"`。
  edition 2024 では `std::env::set_var` が `unsafe` なので、要求ごとに環境変数を差し替える設計は取れない（D5）。
- std で使えるもの: `std::os::unix::net::{UnixListener, UnixStream}`、`std::os::unix::fs::{DirBuilderExt, PermissionsExt, MetadataExt}`、
  `std::fs::File::set_modified`（Rust 1.75。`rust-version` は 1.85）。Windows の named pipe を std だけで作る API はない（D4）。

### 計測済みの事実

- hello の `check` は 0.05 s（2026-09-29 の起票時）。2026-09-30 にこの計測機で 3 回測ると `real` 0.02〜0.03 s。規模別の時間と RSS は
  `_perfs/README.md` の「ビルド速度」の表（合成 200・1,000 モジュール）を基準にする。
- release の compiler は 4,509,744 bytes。`shasum -a 256` 1 回が `real` 0.04 s（プロセスの起動を含む）。要求のたびに compiler を
  hash しない理由（D6）。

### 再現（2026-09-30 に確認）

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
mkdir -p /tmp/tz-work-PB06/hello /tmp/tz-work-PB06/bad
printf '42\n' > /tmp/tz-work-PB06/hello/Main.tz
printf 'missing\n' > /tmp/tz-work-PB06/bad/Main.tz
target/release/tsuzuri run /tmp/tz-work-PB06/hello
for i in 1 2 3; do /usr/bin/time -p target/release/tsuzuri check /tmp/tz-work-PB06/hello; done
cd /tmp/tz-work-PB06 && /Users/tmidorikawa/Documents/git/Tsuzuri/target/release/tsuzuri check bad
/Users/tmidorikawa/Documents/git/Tsuzuri/target/release/tsuzuri watch check hello
```

`run` は `42`。`check bad` は終了コード 1 で次を出す（`--json` では `"code":"E1002"` と `"path":"bad/Main.tz"` の 1 行）。`watch check hello` と
`serve hello` は終了コード 2 で `<command line>:1:1: error[E2000]: pass one .tz, .tt, or .tc file or project directory; modules are loaded recursively`。

```text
bad/Main.tz:1:1: error[E1002]: unknown value 'missing'
  1 | missing
    | ^
```

## 目標と指標

目標は CI の合否条件にしない。計測機で before と after を記録して判断する。Phase 1 だけでは解析を使い回さないので速くならない
（watch の利便と、Phase 2 の土台）。短縮は Phase 2 と G17 の cache の組み合わせで初めて生じる。

- G1（Phase 2 + G17）: 1,000 モジュールの合成プロジェクトで、1 モジュールの本体を変えた後の server 経由の `check` を 100 ms 以下にする。
  `build -O0` は PB05 と合わせて 200 ms 以下（起票時の目標。PB05 は依存ではない）。
- G2（Phase 1）: watch は変更から再実行の開始まで 1 周期（100 ms）以内。追加・削除したファイルは 1 s 以内（D9）。
- G3: server を起動していない CLI を遅くしない（client の確認は socket の `symlink_metadata` 1 回）。
- G4: 常駐 bytes は上限（1 GiB、D6）を超えない。RSS は記録する。

| 指標 | 単位・統計 | 対象 | 期待 |
| --- | --- | --- | --- |
| M1 編集後の `check` | ms（9 標本の中央値・最小・最大） | PX03 の合成 200・1,000 モジュール。標本の前に `Module<m>.tz` を `moduleSource(m, { salt: s })` で書き直す | server 経由が `--no-server` より短い（Phase 2 + G17） |
| M2 編集後の `build -O0` | 同上 | 同上。`--cpu generic`、cache は有効（編集で miss） | 同上 |
| M3 watch の遅延 | ms（9 標本） | 同じ編集。書き込みの直前から `watch: run N finished` の行を読むまで | `--no-server` の M1 + 100 ms 以内（Phase 1） |
| M4 server の RSS | bytes（M1・M2 の後に 1 回） | `ps -o rss= -p <pid>` の値 × 1024 | 記録のみ |
| M5 常駐 bytes | bytes | `status` 応答の `resident_bytes` | 1 GiB 以下 |
| M6 server なしの追加費用 | ms（9 標本） | hello の `check`（socket なし）、before と after | 差が広がりの範囲内 |

## 変えてはいけない意味

- 出力のバイト列: 診断（順序・本文・`--json` の各行・省略の note）、stdout、stderr、終了コード（0・1・2）、成果物（exe・object・llvm・wasm・
  header・wgsl・`.trap.json`・DWARF の sidecar）、`W2001` の message。server 経由・watch・通常の CLI で同じにする。
- コード生成には触れない。overflow・丸め・NaN・符号付きゼロ・trap・評価順序・所有権・借用・IR の決定性・WASM の既定の import なし
  （D-18）は、既存の `driver::build` の経路をそのまま使うことで保つ。
- cache: `cache::build_key` の入力と `--no-cache` の意味を変えない。server は環境と作業ディレクトリが同じ要求だけを受ける（D5）ので、
  key も同じになる。
- 出力の保護: `protect_sources` と `E2003` の検査は `driver::build` の中で同じく動く。server は要求のパスを別の規則で書き換えない。
- 信頼の境界: 同じ OS 利用者だけ。TCP などネットワークの listener を開かない。
- 既存コマンド: server がなければ今と同じ。増えるのは先頭の語 `watch`・`serve` と、check・build の `--no-server` だけ。
- LSP（`tsuzuri lsp`）の挙動は変えない。

## 設計

### 全体の構成

```text
Phase 1: tsuzuri watch <check|build|test> ARGS
  main thread: loop { S = watch::snapshot; code = cli(ARGS); 状態行; watch::wait_for_change(S) }

Phase 2: tsuzuri serve DIR [--idle-timeout SECONDS]
  accept thread: accept → hello 行を書く → sync_channel(QUEUE) へ（満杯なら busy を書いて閉じる）
  main thread:   recv_timeout(idle) → 要求 1 行 → 検査 → cli(ARGS, &mut resident) → 応答 1 行 → ログ 1 行
Phase 2: tsuzuri check|build ARGS（--no-server でなく、socket がある）
  server::request(...) が Some(応答) → stdout・stderr を書いて応答の終了コードで終わる
                      None       → 今と同じくプロセス内で cli(ARGS)
```

### Phase 分割

- Phase 1（実装対象）: `src/main.rs` の出力先の差し替え（`cli`）、`tsuzuri watch`、`src/watch.rs`、`tests/watch.mjs`、文書。
  server・socket・`--no-server` は作らない。
- Phase 2（D2 の承認後、人間が求めた場合だけ）: `src/server.rs`、`tsuzuri serve`、`--no-server`、client、`ResidentState`（新規）、
  `tests/server.mjs`、PX03 の variant、文書。
- Phase 3（設計方針。対象外）: LSP・REPL との共有、Windows の named pipe、`build_key` のツール問い合わせの記憶。

### 出力先の差し替え（Phase 1、`src/main.rs`）

- `fn cli(raw: &[OsString], out: &mut dyn Write, err: &mut dyn Write) -> u8`（新規）に、今の `main` の `let json = ...` 以降を移す。
  Phase 2 で引数 `resident: &mut ResidentState` を足す。
- `print_diagnostic`・`print_diagnostics`・`print_with_severity` と、`main` の message のループ（`W2001` の JSON 行を含む）は
  `err: &mut dyn Write` を受け、`eprintln!` を `writeln!(err, ...)` にする。書き込みの失敗は `expect("failed printing to stderr")`
  （`eprintln!` と同じく panic）。文字列は 1 byte も変えない。
- `run_formatter` と `run_test_action` は戻り値を `ExitCode` から `u8`（`SUCCESS` は 0、`FAILURE` は 1）に変えるだけで、中の `println!`・
  `eprintln!` は変えない（server はこの 2 つを実行しない。watch は同じプロセスの stdout・stderr へ書くので差がない）。
- `main` は `--help`・`--version`・`lsp` の扱いを今のまま残し、先頭が `watch` なら watch へ、それ以外は
  `ExitCode::from(cli(&raw, &mut io::stdout().lock(), &mut io::stderr().lock()))`。

### watch（Phase 1）

新 API（実装後に有効。未検証）:

```text
tsuzuri watch check <source|directory> [check の options]
tsuzuri watch build <source|directory> [build の options]
tsuzuri watch test <source|directory> [test の options]
```

- 引数: `parse_watch_arguments(arguments)`（新規）が `parse_arguments(&arguments[1..])` を呼び、action が `Check`・`Build`・`Test` でなければ
  `E2000` `watch runs check, build, or test; use tsuzuri watch check <path>`（終了コード 2）。`tsuzuri watch` だけのときも同じ。
- 1 回ごとに `cli(&raw[1..], ...)` を実行し、stderr に状態行を書いて flush する。text: `watch: run <n> finished with exit code <code>`。
  `--json`: `{"type":"watch","run":<n>,"exit":<code>}`（`test --json` の `"type"` 行と同じ形）。
- 失敗しても終わらない。Ctrl-C（SIGINT）で終わる。後片付けは不要（socket も一時ファイルも作らない）。
- `run` は受けない（D10）。

### ファイル監視（Phase 1、`src/watch.rs`（新規））

```rust
pub const POLL: std::time::Duration = std::time::Duration::from_millis(100); // （新規）
pub const FULL_EVERY: u32 = 10; // （新規）10 周期（1 s）ごとに全体を読み直す

#[derive(PartialEq)]
pub struct Snapshot { // （新規）
    files: Vec<(PathBuf, u64, Option<SystemTime>)>, // 長さと更新時刻。std（ModuleOrigin::Std）は除く
    texts: Vec<(PathBuf, String)>,                  // path と内容。sources と manifests の順
}

pub fn snapshot(input: &Path, tests: bool) -> Snapshot; // （新規）Project::load（tests なら load_for_tests）。失敗なら空
pub fn wait_for_change(input: &Path, tests: bool, before: Snapshot); // （新規）内容が変わるまで戻らない
```

```text
wait_for_change(input, tests, before):
  polls = 0
  loop:
    sleep(POLL); polls += 1
    stat_changed = before.files のどれかの (len, modified) が違う、または読めない
    if stat_changed or polls % FULL_EVERY == 0:
      next = snapshot(input, tests)
      if next.texts != before.texts: return      # 内容・ファイルの集合が変わった
      before = next                              # 時刻だけの変化（touch）は吸収して再実行しない
```

- snapshot は `cli` の前に取る。実行中の保存は、次の周期で「前の snapshot との差」として見つかり、もう一度実行する（取りこぼさない）。
- 対象は `Project` の `sources` と `manifests` のうち `origin == ModuleOrigin::User` のもの（package のソースを含む）。発見の規則は
  `Project::load` と同じなので、除外（隠しファイル、上限）も一致する。成果物（exe・`.ll` など）は拡張子が違うので対象にならない。
- 1 周期は stat だけ。全体の読み直しは 1 s ごとと、stat が変わったときだけ。`// ponytail: 1 s ごとの全体読み直し。PX03 で CPU 5% を
  超えたらディレクトリの mtime で追加を検出する` を書く。

### server（Phase 2、`src/server.rs`（新規））

```rust
pub const PROTOCOL: u64 = 1; // （新規）
pub const MAX_REQUEST_BYTES: usize = 1024 * 1024; // （新規）
pub const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024; // （新規）LSP の 1 message の上限と同じ
pub const QUEUE: usize = 16; // （新規）
pub const HELLO_TIMEOUT: Duration = Duration::from_secs(2); // （新規）
pub const IGNORED_ENV: [&str; 3] = ["_", "OLDPWD", "SHLVL"]; // （新規）シェルの記録だけの変数

pub struct Request { pub cwd: PathBuf, pub compiler: PathBuf, pub env: Vec<(String, String)>, pub args: Vec<String> } // （新規）
pub struct Response { pub exit: u8, pub stdout: String, pub stderr: String } // （新規）
pub enum Refusal { Protocol, Busy, Root, Cwd, Env, Compiler, CompilerChanged, Unsupported, TooLarge, Internal } // （新規）

pub fn server_directory() -> Option<PathBuf>; // （新規）D5
pub fn project_root(input: &Path) -> Option<PathBuf>; // （新規）Project::load と同じ規則の canonical なルート
pub fn socket_path(directory: &Path, root: &Path) -> PathBuf; // （新規）<dir>/<SHA-256(root) の先頭 32 桁>.sock
pub fn serve(root: &Path, idle: Duration, handle: impl FnMut(&Request) -> Result<Response, Refusal>) -> Result<(), Diagnostic>; // （新規）
pub fn request(root: &Path, args: &[String]) -> Option<Response>; // （新規）None は「プロセス内で実行する」
```

- 非 Unix（`#[cfg(not(unix))]`）: `serve` は `E2000` `the build server needs unix domain sockets; use tsuzuri watch or build without a server`、
  `request` は常に `None`。
- protocol v1: UTF-8 の JSON を 1 行 1 message（改行区切り）。`serde_json` で読み書きする。要求は 1 MiB、応答は 16 MiB まで。

```json
{"protocol":1,"server":"tsuzuri","version":"0.1.0","pid":4242}
{"protocol":1,"op":"run","cwd":"/work","compiler":"/work/target/release/tsuzuri","env":[["HOME","/Users/me"],["PATH","/usr/bin:/bin"]],"args":["check","app"]}
{"protocol":1,"ok":true,"exit":1,"stdout":"","stderr":"app/Main.tz:1:1: error[E1002]: unknown value 'missing'\n  1 | missing\n    | ^\n"}
{"protocol":1,"ok":false,"error":"env","message":"PATH differs from the server environment"}
```

  1 行目は accept 直後の hello、2 行目が要求（`op` は `run`・`status`・`shutdown`）、3・4 行目が応答。`status` の応答は
  `{"protocol":1,"ok":true,"root":...,"requests":n,"served":n,"refused":n,"resident_bytes":n,"compiler_sha256":"..."}`、`shutdown` は
  `{"protocol":1,"ok":true}` を返してから socket を消して終了する。`error` の値は `Refusal` の順に `protocol`・`busy`・`root`・`cwd`・`env`・
  `compiler`・`compiler-changed`・`unsupported`・`too-large`・`internal`。
- 要求の検査（main thread、この順）: 行の長さと JSON と `protocol == 1`・`op`（`protocol`）→ compiler の自己点検（起動時の `current_exe` の
  canonical パスの `len`・`modified`・`ino` と今の値。違えば `compiler-changed` を返して終了）→ `compiler` が同じ canonical パス（`compiler`）→
  `cwd` が server の起動時の `current_dir` と同じ（`cwd`）→ 環境が起動時の `vars_os` と `IGNORED_ENV` を除いて完全一致（`env`。server か要求に
  UTF-8 でない値があれば常に `env`）→ `TSUZURI_TIME_PASSES` がない（`unsupported`。PX01 の段別時間は lib が直接 stderr に書く）→
  `handle`（引数の解釈、action が `Check`・`Build` か（`unsupported`）、`project_root` が server のルートと同じか（`root`））。
- client（`request`）: 引数がすべて UTF-8、環境がすべて UTF-8、`server_directory` がある、socket が `FileTypeExt::is_socket`、ディレクトリが
  私用（D5）の順に確かめ、どれかが偽なら接続せずに `None`。接続後は hello を `HELLO_TIMEOUT` で待ち、要求を書いて書き込み側を閉じ、
  応答は時間制限なしで待つ（前の要求の実行を待つことがある）。I/O の失敗・EOF・壊れた JSON・`protocol` の不一致・`ok:false` はすべて `None`。
  client は fallback の理由を表示しない（出力を cold と同じにするため。理由は server のログに出る）。
- `main`（Phase 2）: `parse_arguments` が成功し、action が `Check`・`Build`、`--no-server` なしのときだけ `request` を試す。`Some` なら
  stdout・stderr をそのまま書いて `ExitCode::from(exit)`。`None` なら `cli`。
- server のログ（server の stderr、1 要求 1 行）: `serve: listening on <socket> for <root>`、`serve: request <n> served (exit <code>)`、
  `serve: request <n> refused: <error> (<message>)`、`serve: idle for <s> s; stopping`、
  `serve: the compiler binary changed; stopping (start tsuzuri serve again)`。

### 常駐状態（Phase 2、`src/driver.rs`）

```rust
#[derive(Default)]
pub struct ResidentState { // （新規）
    frontend: FrontendCache,          // G17 の値（名前は G17 に合わせる）
    runtime: Option<RuntimeObjects>,  // PB01 Phase 1 が done のときだけ
}
impl ResidentState {
    pub fn bytes(&self) -> u64; // （新規）G17・PB01 の大きさの和
}
pub const MAX_RESIDENT_BYTES: u64 = 1 << 30; // （新規）
```

- CLI の 1 回の実行は `ResidentState::default()` を作って `cli` に渡す（今と同じ仕事量）。watch と server は 1 つを使い回す。
- 要求の後に `bytes() > MAX_RESIDENT_BYTES` なら `ResidentState::default()` に置き換え、ログに `serve: resident state dropped (<bytes> bytes)`。
  `// ponytail: 上限を超えたら全部捨てる。M4 で頻発するなら LRU` を書く。
- `CheckedModule`・IR・成果物は常駐させない（D11）。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| CLI（P1） | `src/main.rs` | `cli`（新規）、`main` | `main` の本体を `cli` へ移す。`main` は `watch` の分岐と `ExitCode::from(cli(...))` |
| CLI（P1） | `src/main.rs` | `print_diagnostic`、`print_diagnostics`、`print_with_severity` | 引数 `err: &mut dyn Write`。`writeln!` に置き換える |
| CLI（P1） | `src/main.rs` | `run_formatter`、`run_test_action` | 戻り値を `u8` に |
| CLI（P1） | `src/main.rs` | `parse_watch_arguments`（新規）、`watch_main`（新規） | 引数の検査、実行と状態行と `wait_for_change` のループ |
| CLI（P1） | `src/main.rs` | `HELP` | `tsuzuri watch <action> ...`（action は check・build・test）の 1 行。P2 で `tsuzuri serve directory [--idle-timeout SECONDS]` と `--no-server` |
| 監視（P1） | `src/watch.rs`（新規）、`src/lib.rs` | `POLL`、`FULL_EVERY`、`Snapshot`、`snapshot`、`wait_for_change` | 上の「ファイル監視」。`pub mod watch;` |
| server（P2） | `src/server.rs`（新規）、`src/lib.rs` | 上の「server」の定数・型・関数 | `pub mod server;` |
| CLI（P2） | `src/main.rs` | `Arguments::no_server`（新規）、`parse_arguments` | `--no-server` は check・build だけ。ほかは `--no-server is only valid with check or build`。2 回目は重複の E2000（`--no-cache` と同じ） |
| CLI（P2） | `src/main.rs` | `parse_serve_arguments`（新規）、`serve_main`（新規） | `serve` の引数と、`handle` の closure（`cli` を `Vec<u8>` に書く） |
| 常駐（P2） | `src/driver.rs` | `ResidentState`（新規）、`MAX_RESIDENT_BYTES`（新規）、G17 の解析の入口 | G17 の値を解析へ渡す |
| 計測（P2） | `benchmarks/run-build.mjs` | `VARIANTS` | `check-edit-body-server`・`build-edit-body-server`・`watch-edit-body`（計測手順） |
| テスト | `tests/watch.mjs`（新規）、`tests/server.mjs`（新規）、`src/watch.rs`・`src/server.rs`・`src/main.rs` の `mod tests` | テスト計画 | |
| 変更なし | `src/lsp.rs`、`src/cache.rs`、`src/llvm.rs`、`src/check.rs` | | コード生成・解析・cache の key は変えない |

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、`running N tests`
の N を必ず見る（GUIDE §3.1）。手順 1〜7 が Phase 1。手順 8〜13 は D2 の承認後だけ行う。

### 手順 1: ベースライン

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行し、基準のコンパイラを保存する。「再現」の `hello`・`bad` を作る。
- 確認: 次がすべて成功する。`cargo test` の N を記録する。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
mkdir -p target/perf/PB06 && cp target/release/tsuzuri target/perf/PB06/baseline-tsuzuri
cargo test --locked --bin tsuzuri
node tests/cache.mjs target/release/tsuzuri
node tests/lsp_sessions.mjs target/release/tsuzuri
grep -rn '"watch"\|"serve"' tests src/main.rs
```

最後の grep が空であること（既存テストが `watch`・`serve` を入力パスとして使っていない）。

### 手順 2: 出力先の差し替え（出力は変えない）

- 変更: `src/main.rs` の `cli`（新規）、`main`、`print_diagnostic`・`print_diagnostics`・`print_with_severity`、`run_formatter`・`run_test_action`。
- 内容: 設計「出力先の差し替え」。`watch` はまだ足さない。
- 確認: `cargo test --locked --bin tsuzuri` が手順 1 と同じ N。`cargo build --release --locked` の後、次の diff が空。`node tests/e2e.mjs target/release/tsuzuri`
  と `node tests/cache.mjs target/release/tsuzuri` が成功する。

```sh
cd /tmp/tz-work-PB06
for c in baseline-tsuzuri tsuzuri; do
  bin=/Users/tmidorikawa/Documents/git/Tsuzuri/target/perf/PB06/baseline-tsuzuri
  [ $c = tsuzuri ] && bin=/Users/tmidorikawa/Documents/git/Tsuzuri/target/release/tsuzuri
  { $bin check bad; echo "exit=$?"; $bin check bad --json; echo "exit=$?"; $bin check hello --no-cache; echo "exit=$?"; } > out-$c.txt 2>&1
done
diff out-baseline-tsuzuri.txt out-tsuzuri.txt
```

### 手順 3: `src/watch.rs`

- 変更: `src/watch.rs`（新規）、`src/lib.rs`（`pub mod watch;`）。
- 内容: 設計「ファイル監視」。1 周期の判定を `fn changed(input: &Path, tests: bool, before: &mut Snapshot, polls: u32) -> bool`（新規）に分け、
  `wait_for_change` は `sleep(POLL)` と `changed` のループだけにする（テストは `changed` を直接呼び、待たない）。
- 確認: `cargo test --locked --lib watch::` が `running 3 tests` で 3 passed（テスト計画）。

### 手順 4: `tsuzuri watch`

- 変更: `src/main.rs` の `parse_watch_arguments`（新規）・`watch_main`（新規）・`main`・`HELP`・`mod tests`。
- 内容: 設計「watch」。`main` の `lsp` の分岐の直後で `raw[0] == "watch"` を見る。
- 確認: `cargo test --locked --bin tsuzuri watch_arguments` が 1 passed。`cargo build --release --locked` の後、
  `target/release/tsuzuri watch run /tmp/tz-work-PB06/hello` が終了コード 2 で
  `error[E2000]: watch runs check, build, or test; use tsuzuri watch check <path>`。

### 手順 5: E2E `tests/watch.mjs`

- 変更: `tests/watch.mjs`（新規）。
- 内容: テスト計画の E2E の 1〜9。
- 確認: `node tests/watch.mjs target/release/tsuzuri` が終了コード 0。

### 手順 6: 文書（Phase 1）

- 変更: 「ドキュメント」の Phase 1 の行。
- 確認: `node scripts/check-docs.mjs _docs/tools/command-line.md` が成功する。

### 手順 7: Phase 1 の最終確認と報告

- 確認: GUIDE §2.3 の基準コマンド、`cargo test --locked`、`node tests/watch.mjs target/release/tsuzuri`、`node tests/cache.mjs target/release/tsuzuri`、
  `node tests/lsp_sessions.mjs target/release/tsuzuri`、「生成コードの確認」がすべて成功する。GUIDE §13 の形で報告して止まる。

### 手順 8: protocol と私用ディレクトリ（Phase 2。まだ使わない）

- 変更: `src/server.rs`（新規）の定数・型・`server_directory`・`project_root`・`socket_path`、`ensure_private_directory`（新規）、
  `read_line_limited`（新規）・`decode_request`（新規）・`encode_response`（新規）、`src/lib.rs`（`pub mod server;`）。
- 確認: `cargo test --locked --lib server::` が `running 5 tests` で 5 passed。

### 手順 9: `tsuzuri serve`

- 変更: `src/server.rs` の `serve`、`src/main.rs` の `parse_serve_arguments`（新規）・`serve_main`（新規）・`HELP`。
- 内容: accept thread、主ループ、`status`・`shutdown`、寿命（D6）、ログ。`handle` は `cli` を `Vec<u8>` へ書き、`String::from_utf8` が
  失敗すれば `Refusal::Internal`。
- 確認: `cargo test --locked --bin tsuzuri serve_arguments` が 1 passed。次で stderr に `serve: listening on /tmp/tz-work-PB06/srv/<32 桁>.sock for
  /private/tmp/tz-work-PB06/hello` が出て、5 s 後に `serve: idle for 5 s; stopping`、終了コード 0、socket が消える。

```sh
cd /tmp/tz-work-PB06 && TSUZURI_SERVER_DIR=/tmp/tz-work-PB06/srv /Users/tmidorikawa/Documents/git/Tsuzuri/target/release/tsuzuri serve hello --idle-timeout 5; ls srv
```

### 手順 10: client と `--no-server`

- 変更: `src/server.rs` の `request`、`src/main.rs` の `Arguments::no_server`・`parse_arguments`・`main`。
- 確認: `cargo test --locked --bin tsuzuri no_server` が 1 passed。手順 1 の `cargo test --locked --bin tsuzuri` の既存分が全部成功する。

### 手順 11: E2E `tests/server.mjs`

- 変更: `tests/server.mjs`（新規）。
- 内容: テスト計画の E2E の 1〜10。この時点の server は常駐状態を持たないので、一致の検査の土台になる。
- 確認: `node tests/server.mjs target/release/tsuzuri` が終了コード 0。

### 手順 12: `ResidentState` と G17・PB01

- 変更: `src/driver.rs` の `ResidentState`・`MAX_RESIDENT_BYTES`、`cli` と `watch_main` と `serve_main` の受け渡し。
- 内容: 設計「常駐状態」。G17 の値を解析へ渡す方法は G17 の API に従う。
- 確認: `cargo test --locked` が成功。`node tests/server.mjs target/release/tsuzuri` と `node tests/watch.mjs target/release/tsuzuri` が成功
  （2 回目以降の要求が G17 の cache を使い、なお cold と同じ出力になる）。`status` の `resident_bytes` が 0 より大きい。

### 手順 13: 計測と文書（Phase 2）

- 変更: `benchmarks/run-build.mjs` の `VARIANTS`、「ドキュメント」の Phase 2 の行。
- 確認: 「計測手順」を行い、`docs/benchmarks.md` に記録する。`node scripts/check-docs.mjs _docs/tools/command-line.md _docs/tools/build-and-cache.md`
  が成功する。GUIDE §10 の完了の定義を確かめる。

## 計測手順

GUIDE §14 と PX03 の「計測手順」に従う。PX01 形式の記録は `target/perf/<run_id>/build.jsonl`（`writeRecords`）、PB06 固有の記録は
`target/perf/PB06/`（どちらもコミットしない）。計測中はほかの重い処理を止め、電源に接続する。

### 環境

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
{ git rev-parse HEAD; git diff --stat; sysctl -n machdep.cpu.brand_string; sw_vers; clang --version | head -n 1; rustc --version; node --version; } > target/perf/PB06/env.txt
```

### variant（Phase 2。`benchmarks/run-build.mjs` に足す）

P・T・m・s は PX03 の「操作」と同じ。env は PX03 の env から `TSUZURI_TIME_PASSES` を除き、`TSUZURI_SERVER_DIR: join(T, "server")` を足したもの。
server・client・watch はすべて同じ env と `cwd: T` で起動する（D5 の一致の検査）。

| variant | 準備 | 標本 |
| --- | --- | --- |
| `check-edit-body-server` | 組の前に `tsuzuri serve P --idle-timeout 600` を起動し `serve: listening` を待つ。warm-up 1 回 | 標本の前に本体を編集し、`tsuzuri check P` |
| `check-edit-body-no-server` | なし | 同じ編集の後に `tsuzuri check P --no-server` |
| `build-edit-body-server` | server は上と同じ | `tsuzuri build P -O0 --cpu generic -o T/app-server` |
| `build-edit-body-no-server` | なし | `tsuzuri build P -O0 --cpu generic --no-server -o T/app-no-server` |
| `watch-edit-body` | `tsuzuri watch check P` を起動し run 1 の状態行を待つ | 書き込みの直前の `performance.now()` から次の状態行まで（`measureProcess` は使わない） |

- 各組は warm-up 1 回と 9 標本。server の標本ごとに `status` の `served` が 1 増えることと、同じ s の `--no-server` と終了コード・stdout・stderr
  が同じことを確かめる（違えば停止条件 2）。
- server の組の終わりに `status`（M5）と `ps -o rss= -p <pid>`（M4）を `target/perf/PB06/server-<P>.json` に書き、`shutdown` を送る。M4・M5 は
  PX01 形式に入れない（PX03 停止条件 2）。
- M6: `for i in 1 2 3 4 5 6 7 8 9; do /usr/bin/time -p target/perf/PB06/baseline-tsuzuri check /tmp/tz-work-PB06/hello; /usr/bin/time -p target/release/tsuzuri check /tmp/tz-work-PB06/hello; done 2> target/perf/PB06/m6.txt`。

### 記録

`docs/benchmarks.md` に「常駐ビルドサーバーと watch（PB06）」の節を足し、日付、計測機、before と after のコミット、ツールの版、M1〜M6 の表
（中央値・最小・最大）、生データの場所、G17・PB01 の状態（done か）、そろえられなかった条件を書く。

## 生成コードの確認

PB06 はコード生成を変えない。IR が before と同じことを確かめる。Phase 2 では server 経由の成果物も比べる。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
target/perf/PB06/baseline-tsuzuri build /tmp/tz-work-PB06/hello --emit llvm -O3 --no-cache -o /tmp/tz-work-PB06/before.ll
target/release/tsuzuri build /tmp/tz-work-PB06/hello --emit llvm -O3 --no-cache -o /tmp/tz-work-PB06/after.ll
cmp /tmp/tz-work-PB06/before.ll /tmp/tz-work-PB06/after.ll
```

`cmp` が何も出さない。Phase 2 では server を起動した状態で `tsuzuri build hello -O3 -o /tmp/tz-work-PB06/s` と
`tsuzuri build hello -O3 --no-server -o /tmp/tz-work-PB06/s2` を `cd /tmp/tz-work-PB06` から実行し、`cmp` が一致を示すこと。

## テスト計画

### Rust テスト

- `src/main.rs` の `mod tests`（`parse` と同じ形）:
  - `watch_arguments_accept_check_build_test_only`: 受理 `watch check Main.tz`、`watch build Main.tz -O0 -o out`、`watch test Specs.tz --filter x`。
    拒否（E2000）`watch`、`watch run Main.tz`、`watch fmt Main.tz`、`watch lsp`、`watch watch check Main.tz`、`watch check Main.tz --no-cache`。
  - `serve_arguments_need_one_directory_and_valid_timeout`（P2）: 受理 `serve app`、`serve app --idle-timeout 1`、`--idle-timeout 86400`。
    拒否 `serve`、`serve a b`、`--idle-timeout 0`・`86401`・`x`・重複、`serve app -O0`。
  - `no_server_is_only_valid_with_check_or_build`（P2）: 受理 `check Main.tz --no-server`、`build Main.tz --no-server`。拒否 `run`・`test` との組と重複。
- `src/watch.rs`（それぞれ一時ディレクトリを `std::env::temp_dir()` の下に作る）:
  - `snapshot_ignores_std_and_follows_project_order`: `ModuleOrigin::Std` がなく、`texts` の順が `Project::load` と同じ。
  - `changed_detects_same_length_edit_with_same_mtime`: 同じ長さに書き換え、`File::set_modified` で時刻を戻す。`polls = 1` では false、
    `polls = FULL_EVERY` では true。
  - `changed_ignores_touch_and_detects_addition_and_removal`: 時刻だけ変えると false。ファイルの追加・削除は `polls = FULL_EVERY` で true。
- `src/server.rs`（P2）:
  - `protocol_roundtrips_and_rejects_invalid_messages`: 要求と応答の往復。拒否（`protocol`）: 1 MiB を超える行、壊れた JSON、object でない、
    `protocol: 2`、`args` がない・文字列でない、未知の `op`、`env` の組が 2 文字列でない。
  - `socket_path_is_short_stable_and_root_specific`: 同じルートは同じ名前、違うルートは違う名前、名前は 32 桁の小文字 hex と `.sock`。
  - `private_directory_rejects_shared_or_linked_directories`: 0o755 と symlink は拒否、新規作成は 0o700、既存の 0o700 は受理。
  - `environment_comparison_ignores_only_shell_bookkeeping`: `IGNORED_ENV` の違いは受理。`PATH` の違い、変数の追加・欠落は `env`。
  - `project_root_matches_project_load`: ディレクトリはそれ自身、ファイルは親、親のない相対ファイルは作業ディレクトリ（canonical）、symlink は `None`。

### E2E

`tests/watch.mjs`（新規、Phase 1）。`tests/cache.mjs` の形（自分の一時ルート、`TSUZURI_CACHE_DIR`、`spawnSync`・`spawn`）。watch の子プロセスの
stderr を貯め、状態行を正規表現で待つ関数を作る（60 s で失敗。性能の閾値ではなく hang の検出）。各 run の stderr（前の状態行から次の状態行まで）が、
同じ内容で実行した cold の `tsuzuri check P`（または build）の stderr とバイト単位で同じことを毎回確かめる。`finally` で子を `SIGTERM` する。

1. `Main.tz` が `42` → `watch: run 1 finished with exit code 0`。
2. `missing` に書き換え → run 2、終了コード 1、`error[E1002]: unknown value 'missing'`。
3. 同じ長さの `mixxing` に書き換え、`utimesSync` で時刻を 2 の値に戻す → run 3（1 s の全体読み直しで見つかる）。
4. `utimesSync` で時刻だけ進め、次に `43` を書く → 次の状態行が run 4（touch で run が増えない）、終了コード 0。
5. 構文エラーの `Extra.tz` を足す → run 5、終了コード 1。
6. `Extra.tz` を消す → run 6、終了コード 0。
7. `watch build P -O0 --cpu generic -o T/app` → run 1 の後 `T/app` の stdout が `43`。`44` に書き換え → run 2 の後 `44`。
8. `watch check P --json` → 状態行が JSON として `{"type":"watch","run":1,"exit":0}`。
9. `watch`、`watch run P` → 終了コード 2、stderr に `watch runs check, build, or test`。

`tests/server.mjs`（新規、Phase 2）。env に `TSUZURI_CACHE_DIR: T/cache` と `TSUZURI_SERVER_DIR: T/srv`、server と client は同じ env と `cwd: T`。
socket は `T/srv/` + `sha256(realpathSync(P))` の先頭 32 桁 + `.sock`。protocol は `node:net` の `createConnection` で直接話す。

1. 一致: 状態（`42`、`43`、`missing`、構文エラーの `Extra.tz` を追加、削除、`Main.tz` の改名で `Main.tz` がない）× コマンド（`check P`、
   `check P --json`、`build P -O0 --cpu generic -o T/a`、`build P -O3 -o T/a`、`build P --target wasm32 -O0 -o T/a.wasm`、
   `build P --emit llvm -O0 -o T/a.ll`、`build P --no-cache -O0 -o T/a`）。server 経由の後に `--no-server` を同じ `-o` で実行し、終了コード・stdout・
   stderr・成果物のバイト列を比べる。server 経由の実行ごとに `status` の `served` が 1 増える。
2. 拒否と fallback: env に `EXTRA=1` を足す（`env`）、`cwd: P`（`cwd`）。どちらも出力は `--no-server` と同じで、server の stderr に
   `refused: env`・`refused: cwd`。`tsuzuri run P` と `tsuzuri test P` の後で `requests` が増えない。
3. protocol: 1 MiB + 1 byte の行、`"protocol":2`、壊れた JSON → `protocol`。別ディレクトリの `args` → `root`。別パスの `compiler` → `compiler`。
4. 偽の server（`shutdown` の後、同じパスに `net.createServer`）: 接続直後に閉じる、hello が壊れている、hello の後 `ok:false`、hello の
   `protocol` が 2、hello を送らない。どれも `check P` の出力が `--no-server` と同じ。
5. 古い socket: server を `SIGKILL` → CLI の出力が `--no-server` と同じ。新しい `serve` が起動できる。
6. 二重起動: 2 つ目の `serve P` が終了コード 1、`a build server is already running for`。
7. 権限: `T/srv` を 0o755 で先に作る → `serve` が終了コード 1 の `E2001`、CLI は fallback。正常時は `T/srv` が 0o700、socket が 0o600。
8. 寿命: `--idle-timeout 1` の server が自分で終了コード 0 で終わり socket が消える。compiler を `T/bin/tsuzuri` に複写して起動し、
   複写し直して時刻を進める → 次の要求で `compiler-changed`、server が終わり、CLI の出力は `--no-server` と同じ。
9. 並行: `build P -O0 -o T/p<i>` を 6 並列（`tests/cache.mjs` の `concurrent`）→ 全部 0、成果物が同一、`served` が 6 増える。
10. 常駐（手順 12 の後）: 1 の一致が常駐状態ありでも成り立ち、`resident_bytes` が 0 より大きい。

コード生成を変えないので、native・WASM × `-O0`・`-O3` の意味の suite と `live == 0` は既存の suite（`tests/e2e.mjs` など）が変わらず成功すること
で確かめる。

### 既存テストへの影響

なし。`tsuzuri watch`・`tsuzuri serve` が入力パスとして読まれなくなるが、手順 1 の grep で既存テストが使っていないことを確かめる。

### 性能

「計測手順」のとおり。CI に時間の閾値を置かない。

## ドキュメント

- `README.md` の「## ビルド」: Phase 1 で `tsuzuri watch`、Phase 2 で `tsuzuri serve`・`--no-server`・`TSUZURI_SERVER_DIR`。`src/main.rs` の `HELP` も同じ。
- `_docs/tools/command-line.md` の「## コマンド」（watch・serve）と「## build と run の主なオプション」（`--no-server`）。
- `_docs/tools/build-and-cache.md`（Phase 2）: 節「常駐ビルドサーバー」を足す（置き場所、権限、fallback、idle、compiler の変更、Windows 非対応）。
- `docs/architecture.md` の「## 不変条件」: watch の周期と検出の規則、server の信頼の境界、出力の一致、環境と作業ディレクトリの一致、main thread での解析。
- `docs/benchmarks.md`: 「計測手順」の記録。`_perfs/README.md`: PB06 の状態欄。

## 受け入れ条件

- [x] Phase 1: `tsuzuri watch` が check・build・test を受け、run・引数なしを `E2000` で拒否する。
- [x] Phase 1: 各 run の出力が cold の CLI とバイト単位で同じ（`tests/watch.mjs`）。同じ長さ・同じ時刻の編集、追加、削除を検出し、touch では再実行しない。
- [x] Phase 1: `cargo test --locked` と既存の suite が成功し、新しい crate と `unsafe` がない。「生成コードの確認」の `cmp` が一致する（`examples/*`・`tests/fixtures/*` の 114 project の IR が基準と同じ）。
- [x] Phase 2: `tests/server.mjs` の一致・拒否・fallback・protocol・権限・寿命・並行のテストが成功する。
- [x] Phase 2: M1〜M6 を計測し、生データの場所とともに `docs/benchmarks.md` に記録している（G1 は取り下げ。G5 は満たした）。
- [x] 「ドキュメント」の該当部分を更新している。GUIDE §10 の完了の定義を満たす。

## 落とし穴

- snapshot を `cli` の後に取ると、実行中の保存を取りこぼして古い結果のまま止まる。必ず前に取る（`changed` のテストでは防げないので E2E の 2〜4 で見る）。
- 更新時刻の粒度（HFS+ は 1 s）や同じ長さの書き換えは stat で見えない。1 s ごとの内容の比較が唯一の救済なので、`FULL_EVERY` を消さない。
- macOS の `/tmp` は `/private/tmp` の symlink。ルートは canonical で比べ、テストの socket 名は `realpathSync` から作る。
- `sun_path` は 104 bytes（macOS）。bind の前に socket のパスが 100 bytes 以下かを確かめ、超えれば `E2000`
  `the build server socket path is too long; set TSUZURI_SERVER_DIR to a shorter directory`。
- edition 2024 の `std::env::set_var` は `unsafe`。client の環境を server に取り込もうとせず、一致しなければ断る。
- 一時ディレクトリ名は pid と `TEMP_COUNTER` を含む。成果物に漏れると server と cold が違う。まず cold 2 回同士を比べる（停止条件 2）。
- `TemporaryDirectory` の `Drop` の警告は server の stderr に出る（まれ。仕様として文書に書く）。新しい直接出力を足さない（停止条件 3）。
- client は fallback の理由を表示しない。調べるときは server の stderr のログを見る。client 用の冗長表示の環境変数を足さない。
- accept thread で解析しない（2 MiB の stack で落ちる）。Rust テストで `serve` のループを動かさない（test thread も 2 MiB）。ループは E2E で試す。
- 待ち行列の client は時間制限なしで待つ。`-O3` の大きなビルドの後ろでは正常に長く待つ。時間制限は hello の 2 s だけ。
- 終了時の socket の削除は、bind した socket と `dev`・`ino` が同じときだけ。古い socket を消して起動した別の server の socket を消さない。
- `watch`・`serve` という名前のディレクトリは `tsuzuri build watch` か `tsuzuri ./watch` で扱う（文書に書く）。
- テストは必ず自分の `TSUZURI_SERVER_DIR` を使い、`finally` で `shutdown` か kill する。開発者の既定の置き場所を使わない。

## 対象外

- 複数の利用者で共有するビルドサーバー、リモートのビルド、TCP。
- LSP（G12）・REPL（G13）との状態の共有。server の protocol は公開 API として固定しない。
- Windows の server（named pipe は crate か `unsafe` が要る）。Windows でも `tsuzuri watch` は動く。
- CLI による server の自動起動（D3）。`run`・`test`・`doc`・`fmt` の server 経由の実行（D10）。
- `cache::build_key` のツールの digest と `--version` の問い合わせの記憶（M2 で大きいと分かったら別チケット）。
- OS のファイル変更 API（FSEvents・inotify）と `notify` crate（D9）。

## 決定事項

### D1: Phase 分割

- 決定: Phase 1 は `tsuzuri watch`（プロセス内。server なし）、Phase 2 は `tsuzuri serve`・client・常駐状態。実装者は Phase 1 だけを行う。
- 理由: watch は socket も権限の面も持たずに単独で出せ、出力先の差し替え（`cli`）は Phase 2 の土台になる。速さは G17 待ちである。
- 状態: 実装済み（2026-10-18。Phase 1 と Phase 2 の両方。「着手時の再検討」の D1 と「実装と検証」）

### D2: Phase 2 の着手

- 決定: 常駐サーバー（ローカル IPC、常駐プロセス、要求の受理）を作るかを人間が決める。
- 理由: 同じ利用者に限っても新しい攻撃面と運用（古い socket、二重起動、寿命）を持ち込む。効果は G17 の cache が done になるまで出ない。
- 状態: 承認済み・実装済み（2026-10-17 の全フェーズ実装依頼。server は opt-in で、なければ出力も挙動も今と同じ）

### D3: 起動方式

- 決定: `tsuzuri serve <directory> [--idle-timeout SECONDS]` で明示的に前面で起動する。CLI は server を起動しない。check・build は socket が
  あれば接続し、`--no-server` で避けられる。起動の失敗は終了コード 1、引数の誤りは 2。
- 理由: 起票時の既定案（自動起動しない）を保つ。std だけでは daemon 化（fork）ができず、前面なら寿命とログが利用者に見える。
- 状態: 実装済み（維持。`serve DIR [--idle-timeout S]`、`--status DIR`・`--stop DIR` を足した（D13））

### D4: 通信路と protocol

- 決定: Unix domain socket（`std::os::unix::net`）、改行区切りの JSON（`serde_json`）、`PROTOCOL = 1`、要求 1 MiB・応答 16 MiB、1 接続 1 要求。
  非 Unix では `serve` が `E2000`、client は使わない。
- 理由: 新しい crate と `unsafe` が要らない。1 接続 1 要求は状態機械を持たない。上限は LSP の `read_message` にそろえる。
- 状態: 実装済み（「着手時の再検討」の変更どおり: socket 名は 16 桁、hello に `root`、要求の読み取り 2 s・応答の書き込み 10 s の期限。accept thread は hello を接続の直後に書く。クライアントは半閉じをしない（macOS の `shutdown` が `ENOTCONN` で失敗するため））

### D5: 置き場所と安全性

- 決定: ディレクトリは `TSUZURI_SERVER_DIR`（空でなければ）、なければ macOS `$HOME/Library/Caches/tsuzuri/server`、Linux `$XDG_RUNTIME_DIR/tsuzuri`
  か `$XDG_CACHE_HOME`・`$HOME/.cache` の下の `tsuzuri/server`。`DirBuilderExt::mode(0o700)` で作り、symlink でない・`mode & 0o077 == 0`・
  中に `create_new` で probe ファイルを作って消せる、の 3 つで私用と判定する（client は作らず判定だけ）。socket は 0o600。要求は
  canonical なルート・`cwd`・compiler のパス・環境（`IGNORED_ENV` を除く）が server と同じときだけ受ける。
- 理由: libc なしで自分の uid を得られないが、0o700 で書き込めることは所有者である証拠になる。環境と作業ディレクトリをそろえれば
  `build_key`・ツールの起動・DWARF の `PWD` まで cold と同じになる。
- 状態: 実装済み（変更どおり: `TSUZURI_SERVER_DIR` は絶対パスだけで、空は使わない・相対は `serve` が `E2000`、環境の比較は `cache::KEY_ENVIRONMENT`・`TSUZURI_*`・キャッシュ・一時・ロケール・SDK の変数と接頭辞。`src/` の読み取りを走査する drift test がある。ディレクトリの所有者は probe ファイルの `uid` と比べる）

### D6: 寿命

- 決定: idle は既定 1800 s（`--idle-timeout` は 1〜86400）。compiler は起動時の canonical パスと `len`・`modified`・`ino` を要求ごとに比べ、違えば
  `compiler-changed` を返して終わる（SHA-256 は起動時に 1 回だけ計算し `status` に出す）。常駐 bytes が 1 GiB を超えたら全部捨てる。
- 理由: 4.5 MB の hash を要求ごとに計算すると目標の 100 ms を食う。LRU は計測で必要と分かるまで作らない。
- 状態: 実装済み（変更どおり: 自己点検は stat の識別。起動時の識別を `ResidentState` が固定し、`frontend_cache` はそれで鍵を作る。SHA-256 は起動時に 1 回だけ計算して `status` に出す。常駐 bytes の上限を超えたら全部捨てる）

### D7: 正しさと fallback

- 決定: server の問題はすべて client の黙った fallback（プロセス内の実行）にする。起票時の「定期的に全体を検査するモード」は作らない。
- 理由: 常駐状態は G17 の content key の cache だけで、cold と比べるには G17 のディスク層も止める必要がある。一致は `tests/server.mjs` と
  `--no-server` で確かめる。
- 見直し提案: 起票時の検査モードを削った。必要なら G17 の API が固まった後に `serve --verify` として足す。
- 状態: 実装済み（維持。server の問題は client の黙った fallback。`tests/server.mjs` が偽のサーバー 13 種・古い socket・共有ディレクトリなどで確かめる）

### D8: 並行性と stack

- 決定: 要求は main thread が 1 つずつ実行する（プロジェクトごとに同時に 1 ビルド）。accept thread は hello を書いて `sync_channel(QUEUE)` に
  入れるだけ。`stack_size` は使わない。
- 理由: HEAD の解析は CLI・LSP とも main thread で動き（8 MiB）、同じ条件を保てる。LSP の `serve` と同じ形で idle も `recv_timeout` で書ける。
- 状態: 実装済み（維持。PB04 の worker は段ごとの scope で、常駐しない。`serve` は `Handler` を受け取り、`cli` を知らない。要求は `catch_unwind` で守り、panic のあとは `ResidentState::reset` で読んだものだけを捨て、固定したコンパイラは残す。レビュー対応で、accept thread が要求の行を読んで `status`・`shutdown` に答え、主ループは実行する要求だけを受ける形にした）

### D9: ファイル監視

- 決定: std だけの polling。100 ms ごとに stat、1 s ごとと stat の変化時に `Project::load` で内容を比べる。
- 理由: 発見の規則を `Project::load` と共有でき、新しい crate が要らない。`notify` などの crate を使う場合は要承認（新しい crate）。
- 状態: 実装済み（「着手時の再検討」の変更どおり: stat の署名（Unix は inode と状態変更時刻を含む）、racy な窓 2 s、Unix 以外は 1 s ごとに全ファイルを読む）

### D10: watch の CLI と server の対象

- 決定: `tsuzuri watch check|build|test ...`。起票時の `tsuzuri build --watch` は採らない。server が実行するのは check と build だけ。
- 理由: 一つの前置きで 3 つの action を扱え、各 action のオプション検査に手を入れずに済む。`run` と `test` は子プロセスの stdin・stdout・
  終了コードを client 側で扱う必要がある。
- 状態: 実装済み（維持。server は check・build 以外の action、解釈できない引数、`--no-server`、別のルートを `unsupported`・`root` で断る）

### D11: 常駐状態の中身

- 決定: G17 の前段 cache の値と PB01 の object の値だけを保つ。`CheckedModule`・IR・成果物は保たない。
- 理由: どちらも content key で自分を検証するので、PB06 が無効化の規則を持たずに済む（起票時の「G11 と同じ入力の規則」は各 cache の key が担う）。
- 状態: 実装済み（「着手時の再検討」で改めた中身: SourceMemo と DigestMemo を `src/resident.rs` に作った。frontend の pack の常駐は、測定のゲート（`parse`＋`cache` が 10% 超）を満たさず実装しなかった。`RuntimeObjects`・ツールの問い合わせの memo は取り下げ）

### D12: 計測

- 決定: PX03 の `benchmarks/run-build.mjs` に 5 variant を足す。M4・M5 は `target/perf/PB06/` に置き、PX01 形式を広げない。
- 理由: 編集の手順・標本数・記録を PX03 と共有でき、PX01 の schema を変えない。
- 状態: 実装済み（`check-edit-body-server`・`build-edit-body-server`（server と `--no-server` を 1 標本ずつ交互に測り、`...-no-server` の record も出す）・`watch-edit-body`。`--variants` で variant を選べる）

## 実装と検証（2026-10-18）

### 実装した範囲

全 Phase を実装した（D2 は承認済み）。基準は `a2313d19`（main `9dd0fbbc` に計測の足場を足した commit）、branch `perf/p4-PB06`。

| commit | 内容 |
| --- | --- |
| `805bca5a` | WP0 着手時の再検討（文書だけ） |
| `b8809a9c` | WP1 `cli(raw, program_arguments, out, err)`・`write_*(err, …)`・`errln!`・終了コードの `u8`（出力は変えない。51 のコマンドの終了コード・stdout・stderr・ファイルが基準と同じ） |
| `72b44507` | WP2 Phase 1: `FileStamp`・`src/watch.rs`・`tsuzuri watch check\|build\|test`・`tests/watch.mjs`・文書 |
| `1676f300` | WP3 `src/resident.rs`（`ResidentState`・`Scope`・SourceMemo・DigestMemo・起動したコンパイラの固定）、`read_source_text`・`file_digest`・`frontend_cache::compiler_identity` の hook、`watch` の接続（差し替えたコンパイラで止まる、`TSUZURI_TIME_PASSES` の回ごとの報告） |
| `12b82438` | WP4 `src/server.rs` の protocol v1・私用ディレクトリ・ソケット名・環境の比較と、`src/` を走査する drift test |
| `65bbb967` | WP5・WP6 `serve` の主ループ・`request`・`control`・`--no-server`・`serve --status／--stop`・`tests/server.mjs`・文書 |
| `0a21ba08` | WP7 警告の sink（`resident::capture`・`warn`）と、プロセスの標準出力・標準エラーへ直接書く箇所の drift test |
| `0038c72a` | WP8・WP9 PX03 の variant（`check-edit-body-server`・`build-edit-body-server`・`watch-edit-body`・`--variants`）、`lsp::serve`・`repl::run` の `Scope`、`tests/lsp_sessions.mjs` の追加 |
| `31baa6fc` | `bind` の小さな修正（保護できない socket を消す） |
| `342b8715` | WP10 docs/benchmarks.md の PB06 の節、strategy.md、この節、チケットを `_completed/` へ |
| `78e10979` | WP10 `src/main.rs` の単体テスト: 直接出力する関数の一覧を固定する（server が引き受ける関数に `println!` が入ると失敗する） |
| `dfaabc81` | 統合 branch（PR07・PR08）の merge。conflict なし |
| `0b738480` | WP10 `tests/server.mjs` に `--emit bitcode`・`--profile-generate`・`--profile-use` の case、環境の drift test が新しい読み取りを見つけることの確認、usage.md の一文 |
| `ce45bcb7` | merge した木での検証の記録（この節） |
| `922c0976` | レビュー対応 1・4・5（watch・build key・panic・Windows の guard） |
| `bd6fdfbd` | レビュー対応 2・3・5（status・--stop・去ったクライアント・ロック・テスト） |
| この commit | レビュー対応の記録（この節） |

実装したもの:

- **Phase 1**: `tsuzuri watch check|build|test`（`cli` を同じプロセスで繰り返す。100 ms ごとの `stat`、1 s ごとの発見、同じ長さ・同じ更新時刻の書き換えは状態変更時刻で見つける、racy な窓 2 s、touch では実行しない。Unix 以外は 1 s ごとに全ファイルを読む）。
- **Phase 2**: `tsuzuri serve DIR [--idle-timeout S]`・`serve --status DIR`・`serve --stop DIR`・`--no-server`・`TSUZURI_SERVER_DIR`。server は check と build だけを実行し、コンパイラのパス・作業ディレクトリ・環境（drift test が守る集合）・プロジェクトのルートが同じ要求だけを受け、ほかは `unsupported`・`env`・`cwd`・`compiler`・`root`・`busy`・`too-large`・`protocol`・`internal`・`compiler-changed` で断り、client が黙って自分で実行する。常駐状態は SourceMemo と DigestMemo と、起動したコンパイラの識別の固定。
- **Phase 3 のうち採用した 1 つ**: LSP と REPL が自分のプロセスの中で同じ `ResidentState` を使う（条件の測定は docs/benchmarks.md の PB06 の節）。

### 実装しなかったもの

- **G1（100 ms 級）の取り下げ**。`check` の結果の再利用（G17 が先送りした）は実装していない。常駐で減るのは起動・`load`・`cache` の一部だけで、実測は 200 module で 0.87〜0.89 倍、1,000 module で 0.62〜0.70 倍（`check`）。
- frontend の pack の常駐: `parse`＋`cache` の段が `check` の 10% を超えるときだけ、というゲートを満たさなかった（4.9〜8.0%）。
- Windows の named pipe（std に server 側の API がなく、`unsafe` か crate が要る）。LSP・REPL が socket で server と話すこと。`build_key` のツールの問い合わせの memo。`RuntimeObjects` の memo（約 1%）。いずれも D16 のとおり採らなかった。
- Linux の実機、x86-64、Windows の実機での確認はない。

### 偏差

- `serve` は `Handler`（trait）を受け取り、`cli` を知らない（起票時は `handle: impl FnMut(&Request) -> Result<Response, Refusal>`）。`status` の返すものを handler が足せるため。
- クライアントは `shutdown(Write)` をしない。macOS では、相手がすでに閉じたソケットへの `shutdown` が `ENOTCONN` で失敗し、`status` のように速い要求が fallback してしまった（`tests/server.mjs` の `served` の数えで見つけた）。サーバーは `set_read_timeout` の `EINVAL`（相手が閉じたソケット。macOS）でも読みを続ける。
- accept thread は接続の直後に hello を書く。待っているクライアントが、実行中のビルドのあいだ hello の待ちで諦めないため。要求の行も accept thread が読み、`status`・`shutdown` と protocol・too-large の断りをその場で答え、実行する要求だけを待ち行列（16 件。満杯なら `busy`）へ渡す（レビュー対応。最初は主ループが読み、`status` と `--stop` が実行中の要求の後ろで待った）。
- 環境の比較の集合は起票時の `IGNORED_ENV` の補集合ではなく、許可リスト（`cache::KEY_ENVIRONMENT`、`TSUZURI_*`、キャッシュ・一時・ロケール・SDK の変数と接頭辞）にした。ほかのチケットが足す `TSUZURI_*` は自動で入る。drift test（`every_environment_variable_that_the_compiler_reads_is_compared`）は、`src/` の `env::var`／`var_os`／`resolve_tool` の名前がこの集合に入ることを確かめ、名前が文字列でない読みと環境全体の読みは数と場所を固定する。
- `TSUZURI_SERVER_DIR` は絶対パスだけ（相対は `serve` が `E2000`、client は使わない）。
- `watch` は、起動したコンパイラのファイルが差し替えられると、次の回を始める前に（待っているあいだも 100 ms ごとに見て）終了コード 1 で止まる（起票時にない。`serve` と同じ規則。最初の実装は「その回のあとで」止まり、待っているあいだは見ず、次の保存で古い image の回が走った。レビュー対応で直した）。
- `timings::Report::start()` は前の報告の段を捨て、`watch` と `serve` は回ごと・要求ごとの報告を出す（D15）。`main` の `Report` は `disarm()` と `silence_report()` で、`watch`・`serve`・サーバーが引き受けた `check`／`build` では何も出さない。
- PX03 の runner に `--variants` を足した（1 つの測定だけを選べる）。`measureServerPair` は server と `--no-server` を同じ runner の中で交互に測る。
- 文書: 起票時の `_docs/tools/*.md` は `_tsuzuri/language-reference/compiler/` の各ページ（usage・option・diagnostics）に書いた。

### 確認

- （統合 branch を merge する前、`31baa6fc` で）`RUST_MIN_STACK=4194304 cargo test --locked --no-fail-fast`: 1,494 passed、0 failed、1 ignored（103 の test binary と doc-test）。GUIDE §3.1 の回帰テスト 4 つは既定の stack で通る（各 `running 1 test`）。PB06 の新しいテスト: `cargo test --lib -- resident:: watch:: server::` が 42 件（resident 10、watch 10、server 22）、`cargo test --bin tsuzuri` が 25 件（`watch`・`serve`・`--no-server` の引数と、直接出力する関数の一覧の guard を含む）。
- E2E（`target/release/tsuzuri`）: `tests/watch.mjs`（差し替えたコンパイラで止まる・回ごとの報告を追加）、`tests/server.mjs`（5 回連続で成功。負荷の平均 42 のときを含む）、`tests/lsp_sessions.mjs`（常駐状態のあとで、同じ長さ・同じ更新時刻のファイルを読み直す scenario を追加）、`tests/repl.mjs`、`tests/script.mjs`、`tests/cache.mjs`、`tests/parallel_frontend.mjs`、`tests/e2e.mjs`、`tests/prebuilt_runtime.mjs`、`tests/bundle_identity.mjs`、`tests/handoff_tools.mjs`、`tests/llvm_handoff.mjs`、`tests/frontend_cache.mjs`、`tests/benchmark_report.mjs`。14 の suite を 1 つずつ順に実行し、すべて終了コード 0（`parallel_frontend` が 136 s、ほかは 5〜49 s）。
- 既定の出力が変わらないこと: `target/perf/PB06/compare.mjs`（51 のコマンド。終了コード・stdout・stderr・ファイル。`--help` と引数なしのヘルプの 2 つだけが違う）と `examples-compare.mjs`（`examples/*` と `tests/fixtures/*` の 114 project の `check` と `--emit llvm` の `-O0`・`-O3`、342 のコマンドで 0 件の差）を、基準の `9dd0fbbc` のコンパイラと最終のコンパイラで比べた（scratch。`target/perf/PB06/` に置いた）。
- `cargo +1.99.0 fmt --all -- --check`、`cargo +1.99.0 clippy --all-targets --locked -- -D warnings`、`cargo check --locked --all-targets --target x86_64-pc-windows-msvc`、`--target x86_64-unknown-freebsd`（Linux と同じ cfg の経路の型検査）が通る。`node scripts/check-docs.mjs`（usage・option・diagnostics・strategy）と `git diff --check` が通る。新しい crate と `unsafe` はない。
- 計測は docs/benchmarks.md の PB06 の節。生のレコードは `target/perf/PB06-after-1`・`PB06-after-2`（commit しない）。

### 統合 branch（PR07・PR08）との merge

統合 branch `perf/phase4-compiler-performance`（`acb5caa9`。PR08 の `--emit bitcode` と PR07 の `--profile-generate`／`--profile-use`）を merge した（`dfaabc81`）。文面の conflict はなかった。読んで確かめたこと:

- `--profile-generate DIR` は相対パスを `std::path::absolute`（プロセスの作業ディレクトリ）で絶対にする。server は自分と同じ作業ディレクトリの要求だけを実行するので、結果は単独の実行と同じ。
- `--profile-use` は profile を一時ディレクトリへ写し、`cache::file_digest` でその内容を hash する。写したばかりのファイルは更新時刻から 2 s 以内なので、DigestMemo は記憶せず、毎回読む（racy の規則）。
- `src/profile.rs` と `src/driver/lto.rs` にプロセス全体の状態（`static`・`OnceLock`・`thread_local!`）はない。
- 新しい環境変数は `TSUZURI_LLVM_PROFDATA`・`TSUZURI_LLVM_LINK`（どちらも `TSUZURI_*`）と `LLVM_PROFILE_FILE`（接頭辞 `LLVM_`。意味があるのは server が実行しない `run` だけだが、比べる集合には入るので、違えば断って自分で実行するだけ）。drift test は、この 3 つと、サブディレクトリの `driver/lto.rs` を走査していることを確かめるようにした。
- 直接出力する箇所は増えていない。ライブラリ側の期待の表は変えずに通り、`src/main.rs` には「直接出力する関数の一覧」を固定する単体テスト（`only_functions_that_a_server_never_runs_print_directly`）を足した。merge 後に check／build の経路へ `println!`・`eprintln!` が入ると、このテストが失敗する。
- `tests/server.mjs` に、`build --emit bitcode`、`--profile-generate DIR`、ない profile の `--profile-use` を足した。server 経由と `--no-server` が、すべての状態で同じ出力・成果物になる（最初の 2 つは、有効なプログラムで単独の実行が失敗するホストでは外し、その旨を 1 行書く）。

merge した木での結果（最終の commit で）:

- `RUST_MIN_STACK=4194304 cargo test --locked --no-fail-fast`: 1,533 passed、0 failed、1 ignored（104 の test binary と doc-test。`tests/lto.rs` の 17 件を含む）。GUIDE §3.1 の回帰テスト 4 つは既定の stack で通る（各 `running 1 test`）。`cargo test --bin tsuzuri` 29 件、PB06 の lib テスト 42 件。
- E2E 17 本（上の 14 本と `tests/pgo.mjs`〔14 sections passed、0 skipped、0 failed〕・`tests/bindings.mjs`・`tests/wasm_memory.mjs`）がすべて終了コード 0。
- 既定の出力: 51 のコマンドのうち 48 が基準（`9dd0fbbc`）と同じで、違うのは `--help`・引数なしのヘルプ（PB06 と PR07・PR08 の行）と、`--emit nonsense` のエラー文（PR08 の `bitcode` が一覧に入った）の 3 つ。`examples/*` と `tests/fixtures/*` の 115 project、345 のコマンドは 0 件の差。
- `node scripts/check-docs.mjs`（変更した 8 ページ）、`cargo +1.99.0 fmt`・`clippy -D warnings`、`cargo check --all-targets` の `x86_64-pc-windows-msvc`・`aarch64-pc-windows-msvc`・`x86_64-unknown-freebsd`、`sh scripts/check-runtime-includes.sh`、`git diff --check` が通る。

### レビュー対応（2026-10-18）

統合 branch（`ee6c45c8` で PB06 を取り込んだもの）への独立したコードレビューの指摘を直した。先に統合 branch（PM02 とレビュー修正を含む）の先頭へ branch を進めた。

| 指摘 | 直したこと | 確認 |
| --- | --- | --- |
| 1 差し替えたコンパイラで、`watch` が古い image の回を 1 回走らせ、その `build_key` が新しいファイルのハッシュを持った（macOS では、直したコンパイラの結果がキャッシュを消すまで出ない。Linux では `(deleted)` で `build cache disabled` の行が出る） | 回の始めと、待っているあいだの 100 ms ごとに固定したコンパイラを見て、差し替えられていればその回を始めずに止まる（編集の有無によらない）。`build_key` のコンパイラのハッシュは、固定した識別と同じファイルを読んだ最初の 1 回（読む前と後に識別を確かめる）で決めて `ResidentState` が持つ。最初の鍵より前に差し替えられたプロセスは、ハッシュを作らずキャッシュなしで進む | `resident`・`watch`・`cache` の単体テスト、`tests/watch.mjs` の場面 12（待機中の差し替えと、差し替えに続く編集。どちらも 2 回目の実行がない。直す前の binary では `the watch did not stop after 60 s` で失敗する） |
| 2 `status` と `--stop` が実行中の要求の後ろで待ち、時間制限もない。去ったクライアントの要求が後から実行される | accept thread が要求の行を読み、`status`・`shutdown`・protocol の断りをその場で答える（`running`・`queued`・`abandoned` を足した）。実行中の `--stop` は、番号と秒を言って終了コード 1 で返り、要求は止めず、終わったあとに止まる。`status`・`--stop` のクライアントは 10 s で諦める。待っているあいだに自分の側を閉じたクライアントの要求は、非ブロッキングの `read` の EOF で見つけて実行しない | `src/server.rs` の単体テスト 3 つ（実行中の `status`・`shutdown`、去ったクライアント、時間切れ）、`tests/server.mjs` の場面 8b（Clang を止めて、実際のプロセスで確かめる） |
| 3 古いソケットの引き取りの競合（同時に始めた 2 つのサーバーが、同じ古いソケットを消して `bind` し、片方が届かなくなる） | サーバーが生きているあいだ、ソケットの隣の `<名前>.lock` に OS のロック（`File::try_lock`）を持つ。取れなければ二重起動の `E2001` | 4 つを同時に始める単体テスト（1 つだけが走る）、ロックの残りと保持の単体テスト |
| 4 panic のあと `ResidentState::new()` が、そのとき手元にあるファイルを固定し直す | `ResidentState::reset` が、読んだものと数だけを捨て、固定したコンパイラ（識別とハッシュ）を残す。panic を包む部分を `CommandServer::guarded` にした | `resident` の単体テストと、panic するコマンドで `guarded` を動かす `main.rs` の単体テスト |
| 5 テスト: Windows の差し替え、skip が本物の失敗を隠す、idle の余裕 | 差し替えの場面は Windows では行わず、理由を書く。bitcode と `--profile-generate` は、単独の実行が E2002（道具がない）で失敗し、CI・`LLVM_PREFIX`・`TSUZURI_PGO_STRICT=1` がないときだけ外す（ほかの失敗はテストの失敗）。`a_request_resets_the_idle_time` は idle 3 s・待ち 1 s（余裕 2 s） | 上のとおり |

確認: `cargo test --lib`（381 件）、`cargo test --bin tsuzuri`（30 件）、`tests/watch.mjs`・`tests/server.mjs`・`tests/lsp_sessions.mjs`・`tests/repl.mjs`・`tests/cache.mjs`・`tests/e2e.mjs`・`tests/script.mjs` が通る。`cargo +1.99.0 fmt`・`clippy -D warnings`、`cargo check --all-targets`（`x86_64-pc-windows-msvc`・`aarch64-pc-windows-msvc`・`x86_64-unknown-freebsd`）、`node scripts/check-docs.mjs`（usage・diagnostics）が通る。

### 目標と指標の結果

- G1（100 ms 級）は取り下げた（実装しなかったものの先頭）。代わりの記録: 1 module の本体を書き換えた後の `check` は、200 module で 391〜426 → 347〜370 ms、1,000 module で 1,884〜1,975 → 1,223〜1,317 ms（server の段の total は 262・242 ms と 1,223・1,173 ms）。
- G2: 周期 100 ms の設計で、検出だけの遅延は測っていない（E2E は時間を assert しない）。M3 の全体は、200 module で 367〜369 ms、1,000 module で 1,159〜1,546 ms。
- G3: server を起動していない CLI は遅くならない（client の確認は、ソケットのディレクトリの `lstat` 1 回）。`check examples/hello` を基準と交互に 31 組・101 組で測って wall の中央値の差は +1.8%・+2.7%（最小は +1.1 ms・−1.5 ms）、user+sys は同じで、負荷の揺れの範囲。
- G4: 常駐 bytes は 0.69 MiB（200 module）・3.48 MiB（1,000 module）で上限 1 GiB に遠い。server の RSS は 1,000 module の `check` で 671〜681 MiB。
- G5: 1,000 module で server の `load` が 18〜29 ms（server なしは 476〜642 ms）、`watch` の `load` は 11〜13 ms。

### 既知の制限・未確認

- Linux・x86-64・Windows の実機では動かしていない（Linux 相当の cfg の経路は FreeBSD の型検査だけ）。Windows に server はない（`serve` は `E2000`、`--no-server` は受け付けて何もしない）。
- 速度はほかの作業と共有する機械（1 分の load average 19〜48）の参考値で、`open()` の費用はこの機械のセキュリティ agent の性質に強く依存する。正式な測定は統合担当が静かな機械で行う。
- サーバーが信頼するのは同じ利用者だけで、相手の資格は確かめない（`0700` のディレクトリが唯一の防御）。umask・資源の上限などはサーバーのもの。`check`・`build` のクライアントは応答を時間制限なしで待つ（`status`・`--stop` は 10 s）。実行中の要求は止められず、止まらない要求は `status` の `pid` で終わらせる。クライアントが返事を読み終わる前に自分の側を閉じると、待っている要求は捨てられる。ロックは `File::try_lock`（Rust 1.89 から）で、パッケージの `rust-version = "1.85"` はすでに let chain（1.88）で満たされていない。
- 3 つの drift test（環境変数の読み取り、`src/` の直接出力、`src/main.rs` で直接出力する関数の一覧）は、ほかのチケットが統合されたあとで、新しい読み取り・出力があれば失敗する。失敗は、その名前を比べる集合に入れるか、出力を書き込み先へ流すかの判断を求めるもので、期待の表を見直して更新する。`main.rs` の一覧に足してよいのは、server が断るコマンドの関数だけ。
- `tests/server.mjs` はソケットのパスのために短い `TMPDIR` を要する（macOS は 103 byte まで）。panic の経路は、`CommandServer::guarded` を panic するコマンドで動かす単体テストで確かめる（`Handler::run` の `internal` への対応づけは、E2E では起こせない）。

### 決定事項の最終状態（D13〜D16。D1〜D12 は各決定の「状態」）

- D13: 実装済み。`status` は `pid`・`root`・`compiler`・`compiler_sha256`・`idle_seconds`・`uptime_seconds`・`requests`・`served`・`refused`・`refusals`・`resident_bytes`・`resident_limit_bytes`・`source_hits`・`source_reads`・`digest_hits`・`digest_reads` などの JSON で、`--stop` はソケットが消えるまで（最大 5 s）待つ。サーバーがなければ終了コード 1 の `E2001`。非 Unix は `E2000`。
- D14: 実装済み。`resident::Scope`（thread-local。入れ子と panic に対して安全）が差し込み、`Project::load*`・`build_complete` の引数は増えていない。
- D15: 実装済み。`resident::capture`（`Captured`）と `warn`、`timings::begin_command`・`take_report`・`silence_report`、`Report::disarm`。
- D16: ① socket での LSP・REPL との共有は採らない。② LSP・REPL が自分のプロセスの中で常駐状態を使うことは、測定の条件を満たしたので採用（REPL の cache hit が `-O0` で約 7〜9%、`-O3` で約 30%、LSP の 1,000 module の最小が約 455〜480 ms）。③ Windows の named pipe は採らない。④ `build_key` のツールの問い合わせの memo は取り下げ（ファイルの digest だけ memo する）。
