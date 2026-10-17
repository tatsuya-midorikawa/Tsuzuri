# PR06: Task プールの低遅延化と work stealing

| 項目 | 内容 |
| --- | --- |
| ID | PR06 |
| 分類 | 実行速度 |
| 優先度 | P2 |
| 規模 | L |
| 依存 | PX01 |
| 関連 | F01, F02, B06, F06, F10 |
| 状態 | done（Phase 1・2 を実装・検証済み。Phase 3 のスピンは計測後に不採用。`cpp/task_parallel`（G3）の改善は主張しない） |
| 起票 | 2026-09-29。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`）。2026-10-16 に HEAD `6d15b437` で再検討（「着手時の再検討（2026-10-16）」） |
| 承認 | 2026-10-16 の全フェーズ実装の依頼で、Phase 2・3 と D1・D3・D4・D7・D10 の見直しを含めて承認済み（起票時の「Phase 1 だけ」の注記は適用しない） |
| 手本にする既存実装 | 原子的な添字の取得: `src/runtime/task-wasm-threads.c` の `Group`（`atomic_uint_least64_t next` を `atomic_fetch_add_explicit` で取る）と `run_one`。pthread 呼び出しの差し替えと時間に依らない観測: `tests/task_runtime.c` の `test_broadcast`・`test_wait`（`parked`・`broadcasts`・`peak`）と、失敗名を `argv` で渡す子プロセス方式（`tests/tasks.mjs` の `for (const operation of [...])`）。同じ IR に旧 runtime をリンクする比較: `benchmarks/run-tasks.mjs` の `--baseline-runtime` |
| 主な影響ファイル | `src/runtime/task.c`, `tests/task_runtime.c`, `tests/task_stress.c`（新規）, `tests/tasks.mjs`, `benchmarks/run-tasks.mjs`, `benchmarks/run-tasks-wasm.mjs`（新規）, `README.md`, `docs/architecture.md`, `docs/language.md`, `docs/benchmarks.md`, `_tsuzuri/language-reference/`（Task のページと WebAssembly のページ）。Phase 2: `src/runtime/task-wasm-threads.c`, `tests/wasm_threads_stress.mjs`（新規。`tests/wasm_threads.mjs` は変えない）。変えない: `src/runtime/task-wasm.ll`、コンパイラ（`src/*.rs`）、`src/runtime/task-windows.h`（`pthread_cond_signal` は F10 が足済み）、`tests/channel_runtime.c`・`sync_runtime.c`・`trap_boundary_runtime.c` の期待値 |
| 計測対象 | `benchmarks/run-tasks.mjs` の Task runtime 比較（`tiny`・`small`・`bulk`・`coarse`）と `--data-parallel`（`small`・`chunk-boundary`・`bulk`）・`--sweep`・`--gap-us`、`benchmarks/run-tasks-wasm.mjs`（WASM threads）、`benchmarks/run-cpp.mjs` の `cpp/task_parallel`・`cpp/task_sequential`（Node.js 24 が要り、この実装では測っていない） |

## 着手時の再検討（2026-10-16）

起票（HEAD `f8dc655`）の後に F10（`Channel`・`Mutex`・`Task.scope`）と E14（trap 境界）が入り、`src/runtime/task.c` は 655 行になりました。
着手前に HEAD `6d15b437` のコードと実測で照合した結果を記録します。**この節は、下の旧記述（現状・設計・手順・受け入れ条件・決定事項）と
食い違うところで優先します。** 旧記述のうち置き換えた箇所には、この節への注記を入れました。
2026-10-16 の依頼で、Phase 2・3 を含む全フェーズを実装します（Phase 1 を先に完了してから Phase 2 に入ります）。
調査の試作・計測・ログはセッションの作業域にあり、コミットしません。計測は他の agent が並行してビルドしている共有機
（load average 17〜167）の参考値で、静かな機械での正式値は統合後に取り直します。

### 食い違い

| # | 起票の記述 | 現行（`6d15b437`） | 影響 |
| --- | --- | --- | --- |
| 1 | group は `run`・`run_result`・`context`・`length`・`next`・`remaining`・`failure`・`next_group`。pool は mutex と 2 つの条件変数 | group に `owner`・`trap_index`・`trap`（E14）、pool に `busy`・`idle`・`waiters`（F10）が加わり、`tz_task_stop`（`min(failure, trap_index)`）・`TZ_STACK_GUARD`・item ごとの trap hook がある | execute・worker・submit・`tz_task_help_one`・`tz_channel_check_deadlock` を書き換えるとき、すべてを保つ |
| 2 | F10 が done なら、`tz_task_submit` の先頭の `tz_mutex_held` の検査を保つ | その検査は IR の `@tz.mutex.parallel*`（`src/runtime/sync.ll`）が `tsuzuri_mutex_parallel_ok` を呼ぶ形で、`tz_task_submit` にはない | 「位置を保つ」指示は不要。ABI は変えない |
| 3 | D7: `remaining` を削除して `helpers` を足す。既存テストの変更は `task_runtime.c`・`tasks.mjs` だけ | `tz_channel_check_deadlock` が `remaining` を読み、`tests/channel_runtime.c` が `.remaining = 1` の group を手で作って `busy`・`idle`・`waiters`・`help_depth` を white-box に検査する | **D7 は採らない。** `remaining` を残し、参加者ごとにまとめて減らす（下の設計） |
| 4 | submit: lock → 登録 → 起こす数の計算 → unlock → signal → **その後** に claim | F10 の規則: グループを始めたスレッドは、公開するときに最初の子どもを取り済みにする（channel.md の「待ちの規則」、wasm の「三段が忙しい機械で完了する」試験） | **この順は採らない。** 公開の前に最初の添字を取る（group を作るとき `next` を 1 にする。実装は公開する lock の前にこれを済ませるので、lock の中で取るのと同じく、worker が全添字を先に取る窓がない） |
| 5 | Windows adapter に `pthread_cond_signal` がない。`TZ_TASK_COND_SIGNAL` は新規 | どちらも F10 が足済み（`pthread_cond_init`・`pthread_cond_destroy` も）。`tz_waiter_wake` が既に signal を使う | adapter の変更は不要。新規は `tz_task_signal`・失敗注入の hook・`tasks.mjs` の `"cond_signal"` だけ |
| 6 | D10: sanitizer は `tests/tasks.mjs` に入れない。CI の clang に runtime があるとは限らない | `tasks.mjs` に `TSUZURI_TSAN=1`・`TSUZURI_ASAN=1` の枝が既にあり、`tests/trap_return.mjs` は `trap_boundary_runtime.c`（`task.c` を include）を TSan と ASan＋UBSan で常に実行する。`targets.yml` の Linux は `libclang-rt-21-dev` を入れる | 既存の枝を `task_runtime.c` と新しい stress 試験へ広げる |
| 7 | 機械語: after に `ldaddal`・`ldaddl` がなく `_pthread_cond_signal` がある。before の `_pthread_cond_signal` は 0 | HEAD は既に `_pthread_cond_signal` を 5 回参照する（F10）。`ldadd`・`ldaddal`・`ldaddl` は各 1 で起票時と同じ。`remaining` を残すので `ldaddal` も残る | 機械語の基準は外し、lock 回数と signal 回数の assert で判定する |
| 8 | `run-tasks.mjs --metrics` は未実装（PX01 Phase 2）。標本数は 7 | `--metrics` は実装済みで、その場合の標本は既に 9。suite 名は `--data-parallel` でも `tasks` 固定で、`writeRecords` の `wx` により同じ run に 2 つの mode は書けない。`--runtime`・`--sweep` はない | D10 の suite の分割は必要。CI の `run-tasks.mjs --quick --metrics`（`targets.yml`）を保つ |
| 9 | `cpp/task_parallel` の相対性能 0.768 | 古い値。2026-10-13 の集計（`docs/benchmarks.md`）は **0.94007**（Tsuzuri 0.240655 ms、C# 0.226232 ms）。16 件 × 約 79 µs を 10 論理 CPU（8P＋2E）で走らせる形で、理想の臨界経路は 2 件分の約 158 µs、実測は 235〜285 µs で HEAD と試作に差がない。支配するのは起床の遅延と E core | **G3 は Phase 1 では達成できない。** この種目の改善は主張しない |
| 10 | F01 作業時の tiny／small／bulk（1.3／4.6／423 µs） | 負荷の高い共有機では再現しない（HEAD は 6.6〜9.9／20〜22／603〜627 µs） | 静かな機械で測る。A/A の drift 実行（before2）を残す |
| 11 | WASM threads: `run_one` が添字ごとに 2 回 lock し、`signal_work` が添字ごとに `epoch` を増やす。新しいグループから探す | `signal_work` はない（`progress()` は新しい仕事・グループの終わり・チャンネルの変化・競合したロックの解放のときだけ。`wasm_threads.mjs` が 5000 個の子どもで `epoch` の移動を 8 回以下に制限する）。添字ごとに `queue_mutex` を 2 回取り、解錠のたびに `notify` する点は同じ。グループは古い順 | Phase 2 の記述（`helpers`、submit ごとの `notify(min(desired-1,length-1))`）を書き直す |
| 12 | 失敗: mutex の下で `length = stop` | `length` を不変にして `next` を `length` へ CAS で進めても同じ（停止の添字は以後のどの claim より小さい）。停止の添字は `min(failure, trap_index)` のまま | 擬似コードに `tz_task_stop` を使う。trap も同じ cold path |
| 13 | `task.md` の「Windows では Win32 のスレッドプール」 | `CreateThread` と SRWLOCK・CONDITION_VARIABLE による自前の pool（Win32 ThreadPool API ではない） | 同じ PR で文言を直す |
| 14 | `docs/architecture.md` の native と WASM threads の段落 | 現行は正しいが、設計の変更後は `remaining`・`next` の説明が誤りになる | 書き換える。`benchmarks.md` の「7 サンプル」も直す |
| 15 | （記述なし）`task_runtime.c` に assert を足す | `#undef NDEBUG` がない。`tests/windows.mjs` は zig cc で build し、`-O1` 以上は `NDEBUG` を定義する（AGENTS.md） | `#undef NDEBUG` を足す |

### 設計（Design R）

試作を HEAD の `task.c` に当て、`tests/task_runtime.c`・`channel_runtime.c`・`sync_runtime.c`・`trap_boundary_runtime.c`（変更なし）、
`tests/tasks.mjs`（`TSUZURI_TSAN=1` を含む）、channel_threads の E2E、複数 host の pipeline・入れ子の stress が、素の実行・TSan・ASan＋UBSan で
通ることを確かめました。

- `next` は `_Atomic uint64_t`。取得は relaxed の事前 load と `fetch_add`（尽きた後は RMW しない）。`length` は公開後に不変（失敗でも縮めない）。
- `remaining` は残す。参加者（worker・提出したスレッド・チャンネルを待つスレッドの手伝い）は自分が終えた添字の数を数え、止まるときに
  `fetch_sub(done, acq_rel)` を 1 回だけ行う。worker が join で **mutex の下で取った最初の添字** があるので、その参加者が減らすまで
  `remaining > 0` で、stack 上の group は参加者の下で消えない（`helpers` の代わり）。減らした後は group に触れない。
- 失敗・trap は cold path: mutex の下で `failure`・`trap_index`・`trap` を更新し、停止の添字が小さくなったら `next` を `length` へ CAS で進め、
  `remaining` から `length - 旧 next` を引き、`work_available` を broadcast する（D6。`task_runtime.c` の `result_failure` がこの broadcast に依る）。
- 提出したスレッドは、公開する前に最初の添字を取り（`next` の初期値を 1 にする）、公開して unlock の後に signal し、取り尽くして `remaining` を減らし、`remaining == 0` まで待つ
  （待つ間は今までどおり `--busy`・`tz_channel_check_deadlock`・`work_done`）。`tz_task_help_one`・worker・`tz_channel_check_deadlock` も同じ
  claim・release を使う。`busy`・`idle` は join と leave の単位で動かす。
- 新規: `tz_task_claim`・`tz_task_run_item`・`tz_task_drain`・`tz_task_release`・`tz_task_signal`。`tz_task_execute` は無くなる。

保つ不変条件。I1 と D14（worker の完了の数え方。下の「見つけた問題」）には、変異で必ず落ちる決定的な試験がある（`tests/task_runtime.c`。下に書く）。
ほかの I2〜I10 は、`channel_runtime.c`・`trap_boundary_runtime.c`・`task_stress.c`・`tasks.mjs`・sanitizer の実行が通ることで確かめているが、それぞれを外した実装が落ちることまでは確かめていない:

- I1 最初の子どもは、公開する前に投入したスレッドが取る（`next` の初期値 1）。I2 結合を待つスレッドは自分のグループだけを走らせる。I3 worker と `tz_task_help_one` は最も古いグループから選ぶ。
- I4 起こす側が `busy` を戻してから signal する。I5 `idle` は worker だけを数える。
- I6 判定は `busy == 0` で、`idle > 0` と未着手の組も、`remaining == 0` の group もないときだけ出す。I7 手伝いは `idle == 0` のときだけで、深さは 16 まで。
- I8 最後の release の後に group を触らない。I9 失敗・trap は最小添字で、skip した添字は `remaining` から引く。I10 `resume` は結合の後。

起こす数の生存性（signal の数を絞る方式に必要な論証）: 飛行中のトークン（signal と、起きたがまだ取っていない worker）の数は、尽きていない group の数以上である。
group は submit で `TZ_TASK_INITIAL_SIGNALS` 個（1 以上）足し、トークンが補われないのは group を尽くす claim のときだけだからである。連鎖の規則は「join で取った後、参加した group に未着手が残れば
残りの数に応じて 1 〜 `TZ_TASK_CHAIN_SIGNALS` 個 signal する」。これは論証であり証明ではないので、stress（C の `tests/task_stress.c`、E2E の繰り返し）で確かめる。
`tests/task_runtime.c` は連鎖を外した実装（CPU 3 以上で止まる）と submit の signal を外した実装（すべての CPU 数で落ちる）を検出する。

I1 と D14 の試験（`tests/task_runtime.c`。レビューの指摘で足した。それまでの試験は、I1 を外した実装を落とさず（stress も 3000 group の pipeline で 4〜13 件の誤判定にしか出ない）、D14 を崩した実装を 120 回に 1 回ほどしか落とさなかった）:

- I1 `first_child`: 眠っている pool で、投入側の最初の signal の時点で group の `next` が 1 以上であること（`probe_published_group`）と、その signal で投入側を止めて worker に残りの添字を全部走らせても、
  最初の添字を走らせたのが投入側であること（`first_child_on_main`）。
- D14 `last_worker`: 3 つの検査を hook に置く。(1) worker が pool の mutex を取った時点で、自分が実行中と数えられているなら、release していない group があること（`check_leaving`）。
  (2) 最後に終える worker が投入側を起こした後で mutex を放す・取るなら、投入側が戻って pool を見るまで止める（`hold_back`）。投入側が戻ると、`busy` がまだ数えられていて落ちる。
  (3) mutex を放すたびに、全添字が取られて未 release の group では `busy != 0` であること（`check_counted`）。
- 変異の検査（作業用の task.c）: I1（`next` の初期値を 0 にして、公開の後で claim する。レビューの変異）と、D14 の 3 形（release の後で unlock して lock し直す。レビューの変異。release を lock の外で行う。
  空きと数えてから、別の critical section で release する）は、CPU 数 2・3・4・8・32 の `-O0`・`-O3` の 10 構成のすべてで、20 回中 20 回、assert で落ちる（hang はない。落とす検査はそれぞれ
  `probe_published_group`、`main` の `busy`・`idle` の assert、`check_leaving`、`check_counted`）。`probe_published_group` を外しても I1 は、lock 回数の assert を外しても D14 の unlock・lock の形は、同じく 20 回中 20 回落ちる。
  変更前の試験は、I1 の変異を 100 回中 0 回、D14 の unlock・lock の形と、空きと数えてから release する形を 100 回中 4 回と 3 回（どちらも CPU 数 2 の lock 回数の assert だけ）しか落とさなかった。

### 決定事項の見直し

| 決定 | 結果 |
| --- | --- |
| D1 取得の方式 | 承認。group ごとの atomic な `next`。worker ごとの deque は入れない（ABI に spawn がなく、deque は最小添字・`started`・F10 の手伝いの規則を崩す）。 |
| D2 取得の単位 | 承認。1 回の `fetch_add` で 1 添字。 |
| D3 起床 | **見直し**。全 worker への broadcast はやめる。submit は unlock の後に `min(TZ_TASK_INITIAL_SIGNALS, length - 1, worker 数)` 回 signal し、join で最初の添字を取った worker が、参加した group に未着手が残れば、残りの数に応じて最大 `TZ_TASK_CHAIN_SIGNALS` 回 signal する（木の形）。**採用は初期数 1・連鎖 2**（C4 の初期案は初期数 2・連鎖 1。比較は `docs/benchmarks.md`）。 |
| D4 待機 | 承認。pthread の条件変数と Windows の SRWLOCK・CONDITION_VARIABLE のまま。スピンは入れない（20・100 µs を計測した結果は明確な利益がなく、CPU 時間は 4〜40 倍。`docs/benchmarks.md`）。 |
| D5 手伝いと選択順 | 承認。I1（最初の子どもは公開する前に投入したスレッドが取る）を明示する。 |
| D6 失敗の経路 | 承認。cold path で mutex の下、停止は CAS、停止の添字は `min(failure, trap_index)`、`work_available` の broadcast を残す。 |
| D7 完了の数え方 | **見直し（起票の案は採らない）**。`remaining` を残し、参加者ごとにまとめて 1 回だけ減らす。F10 の文言「`remaining` の acq-rel で公開される」はそのまま正しい。 |
| D8・D9 | 承認。CPU 数の読み方・stack の大きさ・`TZ_TASK_MAX_THREADS` は変えない。 |
| D10 計測と sanitizer | **見直し**。`run-tasks.mjs` に `--runtime`・`--sweep`・`--gap-us`（アイドルの後の cold wake）・p50・CPU 時間を足す。sanitizer は既存の `TSUZURI_TSAN`・`TSUZURI_ASAN` の枝を使う。 |
| D11 Phase 2・3 | 承認。Phase 2 の記述は書き直す（上の食い違い 11）。Phase 3 のスピンは既定 0 のまま、コードに入れずに実験だけ行い、結果を記録した（不採用）。 |
| D12 試験の順序（新規） | 試験を先に足し、既存の assert は弱めない。HEAD で通る試験（C2）を先に、lock 回数・signal 回数の assert は設計の変更（C3・C4）と同じ commit で足す。 |
| D13 計測（新規） | 共有機では排他の窓（`with-lock.sh measure`）で測り、load average を記録する。p50 と最小・最大、cold と warm を載せる。正式値は静かな機械で取り直す。 |
| D14 完了と空きの数え方（新規） | worker は `remaining` を減らすことと `busy`・`idle` の更新を 1 つの lock の中で行う（F10 の誤判定の修正。「見つけた問題」）。 |

### 実装手順（改訂）

起票時の「実装手順」（手順 1〜9）は旧設計の計画で、次の work package（WP）に置き換えます。各 WP は 1 commit で、その範囲の検証が通ってから次へ進みます。

| WP | 内容 | 主な変更 | 確認 |
| --- | --- | --- | --- |
| 0 | チケットの改訂 | この節 | `git diff --check` |
| 1 | 計測 harness | `benchmarks/run-tasks.mjs`: `--runtime`・`--sweep`・`--gap-us`、suite の分割、p50・CPU 時間 | `node --check`、`--quick --metrics` を 2 つの mode で同じ run に |
| 2 | 試験を先に（HEAD で通る） | `tests/task_runtime.c`（`#undef NDEBUG`、`hits ≤ 1`）、`tests/task_stress.c`（新規）、`tests/tasks.mjs` | HEAD で緑。TSan・ASan も緑 |
| 3 | native の核（Design R、broadcast は残す） | `src/runtime/task.c`、`tests/task_runtime.c` の lock 回数の assert | C 検査（`-O0`・`-O3` × CPU 数 `-1`・`0`・`1`・`2`・`4`・`1000`、channel 1〜4、trap_boundary、TSan、ASan＋UBSan）、`tasks.mjs`（`TSUZURI_TSAN=1` を含む）、`trap_return.mjs`、`cargo test --locked` |
| 4 | 起床の方針 | `tz_task_signal`、`"cond_signal"` の失敗注入、signal 回数の assert、連鎖 | 同じ matrix と stress |
| 5 | Phase 1 の計測と文書 | `docs/benchmarks.md`・`docs/architecture.md`・language-reference | `node scripts/check-docs.mjs`、`target/perf/PR06-*` |
| 6 | Phase 2（WASM threads） | `src/runtime/task-wasm-threads.c` | `node tests/wasm_threads.mjs`（変更なし）、`bindings_threads.mjs`、`features.mjs concurrency channel` |
| 7 | Phase 3（スピンの実験） | `TZ_TASK_SPIN`（時間で上限を持つ） | 明確な利益がなければ採用せず結果を記録 |

### 受け入れ条件（改訂）

起票時の「受け入れ条件」の機械語の項目は次に置き換えます。

- [x] 257 添字の group で、pool の mutex の取得回数が `2 + (CPU 数 - 1)` 以下（`run` と `results`。HEAD は 259 と 515）。成功した group の broadcast は 1 回以下（HEAD は 2 回）。（`tests/task_runtime.c` の `check_group_cost`。HEAD の runtime ではこの assert が落ちることを確かめた）
- [x] 長さ 2 の group の signal は 1 回、長さ 257 の group の signal は `初期数 + 連鎖数 × (CPU 数 - 1)` 以下（初期数 1・連鎖数 2）。broadcast は 0 か 1 回。
- [x] 複数 host から入れ子の group を同時に走らせても、合計が独立な参照式と一致する。pipeline の stress は終了する（判定の trap は許す）。（`tests/task_stress.c`）
- [x] C 検査が素の実行・TSan・ASan＋UBSan で通る。`tests/tasks.mjs` が素の実行と `TSUZURI_TSAN=1` で通る（`TSUZURI_ASAN=1` も）。`tests/wasm_threads.mjs`（Phase 2 のとき）と `tests/trap_return.mjs` が通る。
- [x] 既存の white-box テスト（`channel_runtime.c`・`sync_runtime.c`・`trap_boundary_runtime.c`）の assert を変えずに通る。（3 ファイルは変更なし。`task_runtime.c` は assert を足し、`hits[2] == 1` を `hits[0..2] == 1` に強めただけ）
- [x] `ir-same`（コンパイラを変えない）、`cargo test --locked`、Windows の `cargo check`（x86_64・aarch64 の MSVC）が通る。（結果は末尾の「実装と検証」）

## 目的

`Task.parallel`・`Task.parallel_results`・`Parallel.*` が共有する native の C scheduler（`src/runtime/task.c`）から、添字ごとの pool 全体 mutex、
全 worker を起こす broadcast、添字ごとの完了通知を除き、短い仕事の起動・完了待ちの遅延と、添字が多いグループの配布費用を減らす。
結果の順序・`Task.parallel_results` の最小添字の失敗・未開始の停止・全 join・trap と runtime 失敗の診断（F01・B06）は変えない。
.NET の thread pool や rayon と同等以上の fork/join 性能を目標にするが、主張するのは計測した結果だけにする。

- Phase 1（native）: `task.c` の取得・起床・完了待ちの規則を変える。Windows の adapter（`task-windows.h`）は変えない（`pthread_cond_signal` は F10 が足済み）。
- Phase 2（WASM threads）: `task-wasm-threads.c` の添字ごとの lock と通知を減らす（食い違い 11）。
- Phase 3（待機前のスピン）: Phase 1 の計測で park と起床が支配的なときだけ評価する。既定は 0。

2026-10-16 の依頼で Phase 1・2・3 を実装する（「着手時の再検討」）。既定の WASM（`task-wasm.ll` の逐次実行）はどの Phase でも変えない。

## 着手条件と停止条件

### 着手条件

- PX01 が `_perfs/README.md` の「一覧」の状態欄で done であること。確認: `grep -n "PX01" _perfs/README.md` と `ls benchmarks/metrics.mjs`。
  PR06 が使うのは PX01 の `benchmarks/metrics.mjs`（`makeRecord`・`writeRecords`・`report`）と `run-cpp.mjs --metrics` だけである。
  `run-tasks.mjs --metrics` は PX01 の Phase 2 の範囲なので、未実装なら PR06 の手順 2 で PX01 の「runner の対応」の規則どおりに足す。
- GUIDE §2.3 の基準コマンドが成功し、手順 1 の before（HEAD の `task.c` の写し、`tsuzuri-before`、計測）を保存していること。
- F10 が done なら、`grep -n "tz_mutex_held" src/runtime/task.c` で `tz_task_submit` の先頭の検査を確かめ、その位置（1 CPU の分岐と `pthread_once` より前）を保つ。
  F10 が todo なら何もしない（F10 はこの位置に検査を置くと仮定している）。
  （2026-10-16: F10 は done。この検査は `task.c` ではなく IR の `@tz.mutex.parallel*` にある。食い違い 2。）

### 前提とする他チケットのインターフェース

- PX01: PX01 形式（schema 1 の JSON Lines、`target/perf/<run_id>/<suite>.jsonl`）、`node benchmarks/metrics.mjs report <run> --baseline <run>`、
  `run-cpp.mjs --metrics <dir>`、PX01 の「計測手順」の before／after（before・after・before2）。
- F10（done）: F10 は、PR06 が `tsuzuri_task_parallel`・`tsuzuri_task_parallel_results` の ABI と「join を待つ thread は自分のグループの
  未配布の仕事を手伝う」保証を保つこと、入口を増やさないことを仮定する。PR06 はどちらも保つ（D5）。加えて、F10 の待ちの規則
  （最初の子どもを公開する前に取る、空きの worker がいなければ手伝う、全員が待てば判定する）と、`busy`・`idle`・`waiters` の数え方を保つ（I1〜I10）。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- `tsuzuri_task_parallel`・`tsuzuri_task_parallel_results` の引数・戻り値・`TZ_TASK_API` の属性、またはコンパイラ（`src/*.rs`）や `task-wasm.ll` を変える必要が出た。
- 既存の `tests/task_runtime.c`・`tests/tasks.mjs`・`tests/wasm_threads.mjs` の assert を弱める・消す必要が出た（「既存テストへの影響」に挙げた追加・置換を除く）。
- 証明のテストが sleep・時間の閾値・再試行なしには通らない。
- TSan・ASan・UBSan の報告を、設計の手順（添字の取得に mutex を使わない）を崩さずに直せない。
- `pthread_cond_signal`（Windows は `WakeConditionVariable`）以外の新しい OS API、`libatomic` などの追加ライブラリ、C11 以外の言語機能が必要になった。
- 手順 8 の計測で、after が before より `bulk`・`cpp/task_parallel` の時間で広がりを超えて遅い。スピンや定数の調整で取り繕わない。
- Phase 2 で `ThreadState`（`_Static_assert` の 28 bytes）か import（`tsuzuri_threads` の `spawn_workers`・`worker_ready`）、poison bit の規約を変える必要が出た。
- F10 の試験（`tests/channel_runtime.c`・`tests/tasks.mjs` の channel_threads・`tests/wasm_threads.mjs` の三段・入れ子）の assert を変えないと通らない。判定の件数が変わる。
- `length` を縮める処理を戻したくなった（TSan が競合を報告する）。
- 追加の試験は、条件変数・原子カウンター・alarm（`perl -e 'alarm N; exec @ARGV'`）だけで書く。sleep や時間の閾値は使わない。

## 現状と計測（HEAD `f8dc655`）

> 2026-10-16 の注記: この節は起票時（HEAD `f8dc655`）の記述です。現行（`6d15b437`）との差は「着手時の再検討」の食い違いの表を見てください
> （F10・E14 による group・pool の欄の追加、WASM threads の通知の変更、`task_parallel` の値の更新など）。

### native scheduler の構造

- `struct tz_task_group` は `run`・`run_result`・`context`・`length`・`next`（非 atomic）・`remaining`（`_Atomic uint64_t`）・`failure`・`next_group`。
  グループは `tz_task_submit` の stack にあり、pool 全体の `tz_task_pool.groups`／`tail` の単方向 list に末尾から入る（FIFO）。
- `tz_task_pool` は `mutex`・`work_available`・`work_done`（どちらも `pthread_cond_t`）・`threads[TZ_TASK_MAX_THREADS - 1]`・`thread_count`・`stopping`・`groups`・`tail`。
- worker 数: `tz_task_parallelism` が `TZ_TASK_SYSCONF(_SC_NPROCESSORS_ONLN)` を一度だけ読み、1〜`TZ_TASK_MAX_THREADS`（32）に丸めて `tz_task_limit` に置く。
  `tz_task_start_pool`（`TZ_TASK_ONCE` で一度）が `limit - 1` 本を `TZ_TASK_PTHREAD_CREATE(..., NULL, tz_task_worker_main, NULL)` で作り、`TZ_TASK_ATEXIT(tz_task_shutdown)` を登録する。
  入れ子のグループや host の thread は worker を増やさない。
- stack: thread の属性は `NULL`（OS 既定）。macOS の 2 本目以降の thread は `pthread_get_stacksize_np` で 536576 bytes（2026-09-29 に確認）。
  Linux は glibc の既定（`RLIMIT_STACK` に従う。未確認）、Windows は `CreateThread` の既定（実行ファイルの既定値）。
- 速い経路: `length == 0` は何もせず `UINT64_MAX`、`length == 1` は pool を起動せずその場で実行、`tz_task_parallelism() == 1` は添字順に逐次実行し、
  results なら最初の失敗の添字で止まる。
- runtime の失敗: `tz_task_check` が 0 以外を `tz_task_fail` に渡し、`Tsuzuri task runtime: <operation> failed (<error>)` を stderr に出して `abort`。
  shutdown 時に group が残れば `shutdown with active work`（`EBUSY`）、shutdown 後の submit は `submit after shutdown`（`EINVAL`）。
- 組み込み: `src/driver.rs` が `include_str!("runtime/task.c")`（と `task-windows.h`）でコンパイラに埋め込み、native の成果物へリンクする。
  したがって `tsuzuri build` の成果物に新しい scheduler が入るのは `cargo build --release --locked` の後である。`benchmarks/run-tasks.mjs` と
  `tests/task_runtime.c` は作業ツリーの `src/runtime/task.c` を直接コンパイルする。`src/driver.rs` のテストは `task.c` が `#include "task-windows.h"` を含むことを確かめる。

### 1 添字あたりの同期（遅延の原因）

- 取得: `tz_task_worker_main` は mutex を持ったまま list を先頭から走査し、`group->next < group->length` の最初のグループで `group->next++` し、unlock して
  `tz_task_execute` を呼び、戻ると再び lock する。`tz_task_submit` の呼び出し元も自分のグループだけを同じ形で取る。つまり全 thread の全添字が pool 全体の
  mutex を 1 回ずつ取る。
- 完了: `run` の経路は `remaining` を `memory_order_acq_rel` で減らし、最後の 1 件だけ lock して `work_done` を broadcast する。`run_result` の経路は
  **毎添字** lock し、失敗なら `failure`・`length` を縮めて `work_available` を broadcast し、`remaining` を減らして 0 なら `work_done` を broadcast する。
- 起床: `tz_task_submit` はグループを入れるたびに `work_available` を `TZ_TASK_COND_BROADCAST` で 1 回起こす。`length == 2` でも待機中の全 worker（最大 31）が
  起き、仕事のない worker は mutex を奪い合ってから再び待つ。
- 完了待ち: 呼び出し元は自分のグループの未配布の添字がなくなると `work_done` で待つ。他のグループは手伝わない（F01 の「グループと進行保証」は手伝ってよいと
  するが、HEAD は手伝わない）。`work_done` は pool で一つなので、どのグループの完了でも待機中の全呼び出し元が起きる。
- 失敗: `tz_task_execute` が mutex の下で `index < group->failure` なら `failure = index`、未配布の数 `length - next` を `remaining` から引き、`length = index` にする。
  取得は単調増加なので、失敗を記録した時点で `failure` 未満の添字はすべて開始済みであり、最小添字の失敗が確定する（B06 の「failure / cancellation」）。
- 集計の順序: `Parallel.*` の chunk 数は `src/llvm_parallel.rs` の `parallel_count`（4096 要素ごと、最大 1024）で入力長だけから決まる。`Parallel.sum` などの
  部分結果は chunk ごとの領域に書かれ、`tsuzuri_task_parallel` が戻った後に `array_loop` で chunk の添字順に結合される。scheduler の実行順は結果に影響しない。

### WASM と Windows

- WASM threads（`task-wasm-threads.c`、opt-in）: `Group` の `next` は atomic だが、`run_one` が添字ごとに `queue_mutex`（CAS と `memory.atomic.wait32` の lock）を
  取得の前後で 2 回取り、1 添字ごとに `signal_work` が `epoch` を増やして全待機者を `memory.atomic.notify(..., UINT32_MAX)` で起こす。新しいグループから探し
  （`groups`・`previous`）、呼び出し元は `run_one(&group)` で自分のグループを優先する。worker の trap は host が `ThreadState::failed` を立て、全 thread が
  `check_failed` で trap する。stack は `tsuzuri_thread_stack_size` の 262144 bytes。`ThreadState` は host との制御領域で `_Static_assert` により 28 bytes。
- 既定の WASM（`task-wasm.ll`）: `@tsuzuri_task_parallel`・`@tsuzuri_task_parallel_results` が添字 0 から逐次に実行する。import なし。
- Windows: `task-windows.h` が `pthread_mutex_t` を `SRWLOCK`、`pthread_cond_t` を `CONDITION_VARIABLE`、`pthread_create` を `CreateThread` に写す。
  `pthread_cond_broadcast`（`WakeAllConditionVariable`）・`pthread_cond_wait` はあるが `pthread_cond_signal` はない。
- 既存の検証は「テスト計画」の表の「既存」の行。すべて条件変数と原子カウンターにより、時間に依らない。

### 計測済みの事実

- 2026-09-26 の `cpp/task_parallel`（50000 × 16）の相対性能は 0.768 で、最速は C#（`_perfs/README.md` の「現状」）。個々の値は `docs/benchmarks.md` の
  横断比較の表の `cpp/task_parallel` 行。逐次の `task_sequence`・`task_sequential` は Tsuzuri が最速。
  （2026-10-16: 0.768 は 2026-09-26 の集計（Tsuzuri 0.2708 ms、C# 0.2079 ms）の値で古い。2026-10-13 の集計は 0.94007（0.240655 ms と 0.226232 ms）で、
  プールを変えずに ±15 % 動く。食い違い 9。）
- F01 作業時（Apple M1 Max、10 logical CPU、Apple Clang 21、O3 generic）の `run-tasks.mjs`（`docs/benchmarks.md` の「Task プール」）:

| 仕事 | 要素数／反復 | 新 pool cold ms | 旧 runtime cold ms | 新 pool warm us/call | 旧 runtime warm us/call |
| --- | --- | --- | --- | --- | --- |
| tiny | 4／0 | 0.079 | 0.070 | 1.319 | 45.147 |
| small | 32／16 | 0.109 | 0.096 | 4.578 | 42.996 |
| bulk | 1024／256 | 0.641 | 0.426 | 423.500 | 309.187 |

- 同じ節の結論: tiny・small の warm は短縮したが、cold と bulk は旧版より遅い。残る費用は「要素ごとの割当・通知・mutex 競合」。
- F02 の `--data-parallel`（`docs/benchmarks.md` の「データ並列 API」）: 64／8 で `Parallel.sum` 0.000313 ms に対し `Array.sum` 0.000266 ms。
- 待機前のスピン、worker ごとの queue、work stealing はない。

### 再現（2026-09-29 に確認、M1 Max 10 CPU、macOS Darwin 27.0.0、Apple clang 21.0.0）

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
mkdir -p /tmp/tz-pr06
clang -std=c11 -Wall -Wextra -Werror -pthread tests/task_runtime.c -o /tmp/tz-pr06/task-runtime
for n in 1 2 4 1000; do /tmp/tz-pr06/task-runtime $n && echo ok-$n; done
clang -std=c11 -g -O1 -fsanitize=thread -pthread tests/task_runtime.c -o /tmp/tz-pr06/task-runtime-tsan
TSAN_OPTIONS=halt_on_error=1:second_deadlock_stack=1 /tmp/tz-pr06/task-runtime-tsan 1000 && echo tsan-ok
clang -std=c11 -g -O1 -fsanitize=address,undefined -fno-sanitize-recover=undefined -pthread tests/task_runtime.c -o /tmp/tz-pr06/task-runtime-asan
ASAN_OPTIONS=detect_stack_use_after_return=1:abort_on_error=1 UBSAN_OPTIONS=halt_on_error=1:print_stacktrace=1 \
  /tmp/tz-pr06/task-runtime-asan 4 && echo asan-ok
node benchmarks/run-tasks.mjs target/release/tsuzuri --quick > /tmp/tz-pr06/quick.json && echo quick-ok
```

- 期待: `ok-1`・`ok-2`・`ok-4`・`ok-1000`・`tsan-ok`・`asan-ok`・`quick-ok`。`quick.json` の `mode` は `correctness-smoke`（HEAD で確認済み）。

## 目標と指標

目標は CI の合否条件にしない。専用の計測機で before（HEAD の `task.c`）と after を同じ条件で記録して判断する（GUIDE §14、PX01 の「計測手順」）。

- G1: 短いグループの起動から完了待ちまで（`tiny`・`small` の warm）を HEAD より短くする。
  （2026-10-17: 眠った worker の起床（`--gap-us`）では tiny・small とも満たした。連続呼び出しの small は窓 1（C4 の runtime）で短くなったが、最終の runtime の非排他の run では A/A と区別できない。連続呼び出しの tiny は、改善を示せていない。
  p50 が run によって 0.2 µs 前後から 1〜3 µs に上がり、平均に差が読めない。`docs/benchmarks.md` の PR06 の節。）
- G2: 添字が多いグループ（`bulk`、`--data-parallel` の `chunk-boundary`・`bulk`）の配布費用を減らす。F01 で旧 runtime より遅くなった `bulk` の warm を HEAD より短くする。
- G3: `cpp/task_parallel` の wall を C# の最速と同等以上にする（起票時の目標。未達でも Phase 1 の完了は妨げない）。
  （2026-10-16: この種目は 16 件 × 約 79 µs で、1 件ごとの claim 費用は仕事の 0.01 % にすぎず、起床の遅延と E core が支配する。Phase 1 では動かないので、改善を主張しない。）
- G4: CPU 時間の合計を増やさない（Phase 1 はスピンを入れず、起こす worker を減らす）。
- G5: 仕事の粒度を変えた種目で、`Task.parallel` が逐次版より遅くならない最小の粒度を記録する（起票時の目標）。

| 指標 | 単位・統計 | 対象 | 期待（計測で確かめる） |
| --- | --- | --- | --- |
| M1 warm 遅延 | us/call（9 標本の中央値・最小・最大） | `run-tasks.mjs` の `tiny`（4 添字）・`small`（32 添字）。`pool`（作業ツリー）と `baseline`（HEAD の写し）を同じ実行で交互に測る | 短縮。`tiny` で起こす worker は最大 3 本になる |
| M2 cold | ms（同上） | 同じ実行の 3 種目 | 変化なし（pool の起動は同じ） |
| M3 bulk | us/call（同上） | `bulk`（1024 添字 × 256 反復） | 短縮（添字ごとの mutex がなくなる） |
| M4 data-parallel | ms/call（同上） | `--data-parallel` の `parallel-init`・`parallel-map`・`parallel-sum` × `chunk-boundary`（8193 要素 = 3 chunk）・`bulk` | 短縮。`small`（64 要素 = 1 chunk）は `length == 1` の速い経路で変化なし |
| M5 言語間 | ms（PX01 の `wall_time`・`cpu_time`、runner の 12 標本の中央値） | `run-cpp.mjs` の `cpp/task_parallel`・`cpp/task_sequential` | `task_parallel` の `wall_time` が短縮し `cpu_time` は増えない。`task_sequential` は変化なし |
| M6 損益分岐の粒度 | `rounds` と 1 添字の逐次時間（us） | `run-tasks.mjs --sweep`（新規。32 添字、`rounds` = 16・64・256・1024・4096・16384） | 記録のみ。`parallel` の warm が `sequential` 以下になる最小の `rounds` |
| M7 スケーリング | us/call（`small`・`bulk`） | CPU 数を 1・2・4・8 に固定した runtime（計測手順の wrapper）と既定（10） | 記録のみ |

## 変えてはいけない意味

- ABI: `tsuzuri_task_parallel(void (*)(void *, uint64_t), void *, uint64_t)`・`tsuzuri_task_parallel_results(uint32_t (*)(void *, uint64_t), void *, uint64_t)` の型、
  `TZ_TASK_API`（native は `weak, visibility("hidden")`）、戻り値（`UINT64_MAX` が成功、それ以外は最小の失敗添字）。入口を増やさない。
- 成功時は各添字をちょうど一度実行する。`parallel_results` が f を返すとき、f 未満の全添字と f は実行済みで、f より大きい添字は「失敗の記録より前に取得したもの」
  だけが実行される（B06 の未開始の停止）。どの添字を開始したかは callback が `started` に書き、drop はコンパイラ側が決める。runtime は `started` を読まない。
- 返る失敗の添字は CPU 数・完了順に依存しない（常に最小）。既定の WASM の逐次実行と同じ値になる。
- 呼び出し元は、自分のグループの全添字が終わり、その書き込みが見えるまで戻らない（全 join）。
- 決定性: 結果は callback が添字の位置へ書く。`Parallel.*` の部分結果は、runtime が戻った後にコンパイラの `array_loop` が chunk の添字順に結合する（GUIDE D-14）。
  scheduler に完了順の結合・集計を持ち込まない。浮動小数点の再結合もしない。
- trap: task body の trap は native の異常終了・WASM trap のまま。runtime の失敗は `Tsuzuri task runtime: <operation> failed (<error>)` と `abort` のまま。
  既存の operation 名（`pthread_create`・`pthread_join`・`pthread_mutex_lock`・`pthread_mutex_unlock`・`pthread_cond_wait`・`pthread_cond_broadcast`・`pthread_once`・`atexit`）を保ち、
  新しい `pthread_cond_signal` を同じ形で足す。
- 速い経路: `length == 0`・`length == 1` は pool を起動しない。CPU 数 1 は thread を作らない（`tests/task_runtime.c` の `created == 0`）。
- 資源: 常駐 worker は `min(CPU 数, 32) - 1` 本以下（`peak == required - 1`）。入れ子と host の thread は worker を増やさない。shutdown で全 join（`created == joined`）。
  thread の stack は変えない（D9）。
- 安全性: データ競合を入れない（TSan）。stack 上のグループを呼び出し元が戻った後に触らない（ASan の `detect_stack_use_after_return`）。
- WASM: 既定の WASM は import なしの逐次（`task-wasm.ll` を変えない）。WASM threads の import・`ThreadState` の 28 bytes・`tsuzuri_thread_stack_size` を変えない。
- 生成 IR: コンパイラを変えないので、同じ入力の `--emit llvm` は byte 単位で同じ。overflow・丸め・NaN・符号付きゼロは scheduler が触らない。

## 設計

> 2026-10-16 の注記: この節は起票時の設計（`helpers` を足して `remaining` を消す、unlock の後に signal して claim する、`min(thread_count, length - 1)` の signal）です。
> 実装する設計は「着手時の再検討」の「設計（Design R）」と「決定事項の見直し」で、D3・D7 はこの節と異なります。D1・D2・D5・D6 の方針（atomic な添字の取得、
> 1 添字ずつ、自分のグループだけの手伝い、cold path の失敗）は残ります。

### 方式（Phase 1）

1. 添字の取得を `_Atomic uint64_t next` への `atomic_fetch_add_explicit(..., memory_order_relaxed)` にする。pool の mutex はグループの登録、worker の参加・離脱、
   失敗の記録、完了待ちだけに使う。1 添字あたりの同期は relaxed の RMW 1 回になる。
2. worker は mutex の下でグループに参加（`helpers` を増やす）し、取得できなくなるまで mutex なしで添字を取り続け、mutex の下で離脱する。`helpers` が 0 になったら
   `work_done` を broadcast する。
3. 呼び出し元は自分のグループの添字を取り尽くした後、mutex の下で `helpers == 0` まで `work_done` で待ち、同じ保持のまま list から外す。worker が参加しなかった
   グループでは一度も待たない。
4. 起床は submit ごとに `min(thread_count, length - 1)` 回の `pthread_cond_signal`。回数は mutex の下で計算し、signal は unlock の後に行う。
5. 失敗は冷たい経路として mutex の下で記録し、`next` を CAS で `length` まで進めて未開始の添字を止める。

### データ構造

```c
struct tz_task_group {
    void (*run)(void *, uint64_t);
    uint32_t (*run_result)(void *, uint64_t);
    void *context;
    uint64_t length;          /* 変更: 提出後は不変。失敗で縮めない */
    _Atomic uint64_t next;    /* 変更: atomic。tz_task_claim と tz_task_record_failure だけが書く */
    uint64_t failure;         /* pool の mutex の下 */
    unsigned helpers;         /* 新規: pool の mutex の下。参加中の worker の数 */
    struct tz_task_group *next_group;
};
/* remaining は削除（D7）。tz_task_pool の欄は変えない。 */

#ifndef TZ_TASK_COND_SIGNAL
#define TZ_TASK_COND_SIGNAL pthread_cond_signal
#endif
```

`tz_task_submit` の初期化子は `{ run, run_result, context, length, 0, UINT64_MAX, 0, NULL }`（HEAD も `_Atomic` の欄を波括弧で初期化している）。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| 差し替え境界 | `src/runtime/task.c` | `TZ_TASK_COND_SIGNAL`（新規）、`tz_task_signal`（新規） | `tz_task_wake` と同じ形で `tz_task_check("pthread_cond_signal", TZ_TASK_COND_SIGNAL(condition))` |
| データ | `src/runtime/task.c` | `struct tz_task_group` | 上のとおり。`remaining` を削除し `helpers` を追加 |
| 取得 | `src/runtime/task.c` | `tz_task_claim`（新規） | relaxed load で `length` 以上なら 0。そうでなければ `fetch_add`、戻り値が `length` 未満なら 1 |
| 実行 | `src/runtime/task.c` | `tz_task_execute` を `tz_task_run_claimed`（新規）で置き換える | 取得できる間 `run` か `run_result` を呼ぶ。0 以外なら `tz_task_record_failure` |
| 失敗 | `src/runtime/task.c` | `tz_task_record_failure`（新規） | lock。`index < failure` なら `failure = index`、`next` を CAS で `length` へ、`work_available` を broadcast。unlock |
| worker | `src/runtime/task.c` | `tz_task_worker_main` | 走査の条件を `atomic_load_explicit(&group->next, memory_order_relaxed) < group->length` に。参加・離脱と `work_done` の broadcast |
| 提出 | `src/runtime/task.c` | `tz_task_submit` | 初期化子、登録後の `wake` の計算、unlock 後の signal、`tz_task_run_claimed(&group)`、`helpers == 0` の待ち。unlink は HEAD のまま。F10 後は先頭の `tz_mutex_held` を保つ |
| 変更なし | `src/runtime/task.c` | `tz_task_parallelism`, `tz_task_start_pool`, `tz_task_shutdown`, `tz_task_fail`, `tz_task_check`, `tz_task_wake`, `tsuzuri_task_parallel`, `tsuzuri_task_parallel_results`, 速い経路, `#include "task-windows.h"` | そのまま（`src/driver.rs` のテストが include を確かめる） |
| Windows | `src/runtime/task-windows.h` | `pthread_cond_signal`（新規） | `static inline int pthread_cond_signal(pthread_cond_t *condition) { WakeConditionVariable(condition); return 0; }` を `pthread_cond_broadcast` の隣に置く |
| テスト | `tests/task_runtime.c` | `test_signal`（新規）、`signals`・`locks`（新規）、`test_lock`、`main` | 「テスト計画」 |
| テスト | `tests/tasks.mjs` | 失敗注入の operation の配列 | `"cond_signal"` を足す |
| 計測 | `benchmarks/run-tasks.mjs` | `--sweep`（新規）、`--metrics`（PX01 Phase 2 が未実装なら新規）、標本数 | D10 |
| Phase 2 | `src/runtime/task-wasm-threads.c` | `Group`, `run_one`, `submit`, `tsuzuri_thread_entry` | 「Phase 分割」 |

### アルゴリズム

```c
static int tz_task_claim(struct tz_task_group *group, uint64_t *index) {
    if (atomic_load_explicit(&group->next, memory_order_relaxed) >= group->length) return 0;
    *index = atomic_fetch_add_explicit(&group->next, 1, memory_order_relaxed);
    return *index < group->length;
}

static void tz_task_record_failure(struct tz_task_group *group, uint64_t index) {
    tz_task_lock();
    if (index < group->failure) {
        group->failure = index;
        uint64_t next = atomic_load_explicit(&group->next, memory_order_relaxed);
        while (next < group->length && !atomic_compare_exchange_weak_explicit(
                   &group->next, &next, group->length, memory_order_relaxed, memory_order_relaxed)) {}
        tz_task_wake(&tz_task_pool.work_available);
    }
    tz_task_unlock();
}

static void tz_task_run_claimed(struct tz_task_group *group) {
    uint64_t index;
    while (tz_task_claim(group, &index)) {
        if (!group->run_result) group->run(group->context, index);
        else if (group->run_result(group->context, index)) tz_task_record_failure(group, index);
    }
}
```

```text
worker（tz_task_worker_main）:
  lock
  while !stopping:
    group = list の先頭から最初の (atomic_load(next) < length) のグループ（FIFO。HEAD と同じ）
    なし: wait(work_available); continue
    group.helpers += 1; unlock
    tz_task_run_claimed(group)
    lock; group.helpers -= 1; 0 になったら broadcast(work_done)
  unlock

submit（length >= 2 かつ CPU 数 >= 2。それ以外の速い経路は HEAD のまま）:
  group = { ..., next = 0, failure = UINT64_MAX, helpers = 0 }
  lock; stopping なら tz_task_fail; list の末尾に登録
  wake = min(tz_task_pool.thread_count, length - 1); unlock
  wake 回 tz_task_signal(&work_available)
  tz_task_run_claimed(&group)
  lock; while group.helpers != 0: wait(work_done); list から外す（HEAD の走査）; unlock
  return group.failure
```

### 正しさの論拠

- 一度だけ: 添字は `fetch_add` の戻り値からだけ得る。`next` は単調増加で、CAS も `length` へ増やすだけである。
- use-after-return がない: worker がグループを選ぶことと `helpers` の増加は同じ mutex の保持内で起こり、呼び出し元の `helpers == 0` の確認と list からの除去も
  同じ mutex の保持内で起こる。前者が先なら呼び出し元は待ち、後者が先ならグループはもう list にない。`next` の古い値を読んで参加しても、取得に失敗して離脱するだけである。
- 公開: worker の callback の書き込み → `helpers` の減少 → unlock → 呼び出し元の lock（`work_done` の wait からの復帰）で happens-before が成り立つ。
- 最小添字: 取得は単調なので、f を取った時点で f 未満はすべて取得済みで、取得済みの添字は必ず実行される。よって f 未満の失敗もすべて記録され、`failure` は最小に収束する。
  CAS の後の取得は `length` 以上になり、未開始の添字は開始されない。
- 進行: 呼び出し元は自分のグループを一人で完了できる。signal の数は並列度だけに影響し、失われても進行は失われない。
- 入れ子: 待つ thread は自分のグループだけを手伝う。待ちの依存は常により深いグループへ向くので循環せず、stack の深さは HEAD と同じく入れ子の深さで決まる。

### Phase 分割

- Phase 1: 上のとおり（native と Windows adapter）。実装者はここまで。
- Phase 2（人間が求めた場合）: `task-wasm-threads.c` の `Group` から `remaining` を除いて `helpers`（新規）を足す。`run_one` を「`queue_mutex` の下で参加 →
  `check_failed` と `fetch_add` の取得を繰り返す → `queue_mutex` の下で離脱し、0 になったら `signal_work`」に置き換える。`submit` は登録後に `epoch` を増やして
  `__builtin_wasm_memory_atomic_notify((int *)&state.epoch, min(desired - 1, length - 1))` で起こし、自分の添字を取り尽くしたら `helpers == 0` まで `epoch` で待つ。
  失敗は `queue_mutex` の下で native と同じ CAS。新しいグループから探す順序（`groups`・`previous`）、`ThreadState`、import、stack の大きさは変えない。
- Phase 3（人間が求めた場合）: worker が `work_available` で待つ前に、unlock して submit ごとに増える `tz_task_pool.generation`（新規、atomic）を最大
  `TZ_TASK_SPIN`（新規）回だけ読む。候補 0・64・1024 を M1 と M5 の `cpu_time` で比べ、M1 の短縮が広がりを超え、かつ `cpu_time` の増加が広がりを超えない
  最大の値を採る。どれも満たさなければ 0（入れない）。

## 実装手順

> 2026-10-16 の注記: 手順 1〜9 は起票時の旧設計の計画です。実装は「着手時の再検討」の「実装手順（改訂）」（WP 0〜7）で進めます。
> 各手順の確認コマンド（`c_check`・`cargo test` の件数確認・`ir-same`）は WP の確認にそのまま使います。ただし HEAD の runtime は `6d15b437` の `task.c` です。

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、`running N tests` の N が 0 でないことを
必ず見る（GUIDE §3.1）。C の単体テストは次の関数（以下「C 検査」）で行う。TSan と ASan は別の binary にする。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
c_check() {
  for o in 0 3; do
    clang -std=c11 -Wall -Wextra -Werror -O$o -pthread tests/task_runtime.c -o /tmp/tz-pr06/rt-O$o || return 1
    for n in -1 0 1 2 4 1000; do /tmp/tz-pr06/rt-O$o $n || { echo "fail O$o $n"; return 1; }; done
  done
  clang -std=c11 -g -O1 -fsanitize=thread -pthread tests/task_runtime.c -o /tmp/tz-pr06/rt-tsan || return 1
  for n in 2 4 1000; do TSAN_OPTIONS=halt_on_error=1:second_deadlock_stack=1 /tmp/tz-pr06/rt-tsan $n || return 1; done
  clang -std=c11 -g -O1 -fsanitize=address,undefined -fno-sanitize-recover=undefined -pthread tests/task_runtime.c -o /tmp/tz-pr06/rt-asan || return 1
  for n in 2 4 1000; do
    ASAN_OPTIONS=detect_stack_use_after_return=1:abort_on_error=1 UBSAN_OPTIONS=halt_on_error=1:print_stacktrace=1 /tmp/tz-pr06/rt-asan $n || return 1
  done
  echo c-check-ok
}
```

### 手順 1: ベースラインと before の保存

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行し、HEAD の runtime・コンパイラ・IR・機械語を保存する。
- 確認: 次がすべて成功し、`c-check-ok` が出る。`before.s` の `ldadd`・`ldaddal`・`ldaddl` は各 1 回（HEAD で確認済み）。

```sh
cargo build --release --locked
mkdir -p /tmp/tz-pr06 && cp src/runtime/task.c /tmp/tz-pr06/task-before.c && cp target/release/tsuzuri /tmp/tz-pr06/tsuzuri-before
target/release/tsuzuri build benchmarks/tasks/Main.tz --emit llvm -o /tmp/tz-pr06/before.ll
target/release/tsuzuri build benchmarks/tasks/Main.tz --emit llvm -O3 -o /tmp/tz-pr06/before-O3.ll
clang -std=c11 -O3 -S src/runtime/task.c -o /tmp/tz-pr06/before.s
c_check && node tests/tasks.mjs target/release/tsuzuri && node tests/wasm_threads.mjs target/release/tsuzuri
```

### 手順 2: 計測 harness

- 変更: `benchmarks/run-tasks.mjs`。
- 内容（D10）: `--runtime <file>`（新規。既定 `src/runtime/task.c`。`backends` の runtime と `runtime_sha256` に使う）。`--sweep`（新規。workloads は
  `[16, 64, 256, 1024, 4096, 16384]` の各 `rounds` について ``{ name: `rounds-${rounds}`, count: 32, rounds, repetitions: quick ? 2 : 64 }``、backends は
  `[["parallel", runtime, 0], ["sequential", runtime, 2]]`（`functions` の 0 が `parallel_batch`、2 が `sequential_batch`）、`workload_family` は
  `"Task granularity sweep"`。`--data-parallel`・`--baseline-runtime` との併用は usage を throw）。標本数を `quick ? 3 : 7` から `quick ? 3 : 9` へ。
  `--metrics <dir>` がなければ PX01 の「runner の対応」の規則で足す: 各 backend の `warm_ms_per_call` の標本を `wall_time`、`language: "tsuzuri"`、`opt: "O3"`、
  `size: count`、`workload: name`、variant は `pool` → `default`、`baseline` → `before`、他は backend 名。suite は既定 `tasks`、`--data-parallel` は `tasks-data`、
  `--sweep` は `tasks-sweep`。
- 確認: 次が成功し、`sweep.json` の `results` が 6 種目 × 2 backend、併用の実行は usage で非 0 終了、`m-quick/tasks.jsonl` がある。続けて「計測手順」の before を測る
  （runtime はまだ HEAD）。

```sh
node --check benchmarks/run-tasks.mjs
node benchmarks/run-tasks.mjs target/release/tsuzuri --sweep --quick > /tmp/tz-pr06/sweep.json && echo sweep-ok
node benchmarks/run-tasks.mjs target/release/tsuzuri --sweep --data-parallel; echo "exit=$?"
node benchmarks/run-tasks.mjs target/release/tsuzuri --quick --metrics /tmp/tz-pr06/m-quick > /dev/null && ls /tmp/tz-pr06/m-quick
```

### 手順 3: HEAD で成り立つ証明を先に足す

- 変更: `tests/task_runtime.c` の `test_lock`、`locks`（新規、`atomic_uint`）、`main`。
- 内容: `test_lock` の先頭で `atomic_fetch_add(&locks, 1)`。`result_stop`（戻り値 3）の後に全添字 `hits[i] <= 1`、`result_failure`（戻り値 2）の後に
  `hits[0] == 1`・`hits[1] == 1` と全添字 `hits[i] <= 1` を足す。
- 確認: `c_check` が `c-check-ok`。

### 手順 4: 取得と完了待ちの置き換え

- 変更: `src/runtime/task.c` の `struct tz_task_group`、`tz_task_claim`・`tz_task_run_claimed`・`tz_task_record_failure`（新規）、`tz_task_execute`（削除）、
  `tz_task_worker_main`、`tz_task_submit`。`tests/task_runtime.c` の `main`。
- 内容: 「設計」のアルゴリズムどおり。起床はこの手順では HEAD の `tz_task_wake(&tz_task_pool.work_available)` のまま残す。`main` の 1000 回の反復の後に、
  `reset_hits` → `locks` を読む → `tsuzuri_task_parallel(visit, hits, ITEMS)` → 差が `required > 1 ? 2 + (required - 1) : 0` 以下 → `check_hits(1)` を足す。
- 確認: `c_check` が `c-check-ok`。`grep -n "remaining\|next++" src/runtime/task.c` が空。HEAD の runtime ではこの上界の assert が失敗することを、
  `/tmp/tz-pr06/task-before.c` を include するように書き換えた写しで一度確かめてよい（写しは commit しない）。

### 手順 5: 起床を signal にする

- 変更: `src/runtime/task.c`（`TZ_TASK_COND_SIGNAL`・`tz_task_signal`（新規）、`tz_task_submit`）、`src/runtime/task-windows.h`（`pthread_cond_signal`（新規））、
  `tests/task_runtime.c`（`test_signal`・`signals`（新規）、`#define TZ_TASK_COND_SIGNAL test_signal`、`main`）、`tests/tasks.mjs`（operation の配列に `"cond_signal"`）。
- 内容: submit の broadcast を「mutex の下で `wake` を計算し、unlock の後に `wake` 回 `tz_task_signal`」に置き換える。`test_signal` は `signals` を数え、
  `failure` が `"cond_signal"` なら `EINVAL` を返す。`main` に、`required > 1` のとき長さ 2 の呼び出しで `signals` の差が 1、長さ `ITEMS` で `required - 1` になる
  検査（前後で `reset_hits`、長さ 2 の後は `hits[0] == 1 && hits[1] == 1`）を足し、`parked` を待つ block に `signals > 0` を足す。
- 確認: `c_check` が `c-check-ok`。次のループが 9 行とも `ok <operation>` を出す。`grep -n "pthread_cond_signal" src/runtime/task-windows.h` が 1 行。

```sh
for op in create join mutex_lock mutex_unlock cond_wait cond_broadcast cond_signal once atexit; do
  /tmp/tz-pr06/rt-O0 $op 2> /tmp/tz-pr06/err.txt && echo "unexpected success $op" || { grep -q "failed" /tmp/tz-pr06/err.txt && echo "ok $op"; }
done
```

### 手順 6: コンパイラへの組み込みと E2E

- 変更: なし（`src/driver.rs` が `task.c` を埋め込むので、再ビルドで成果物に入る）。
- 確認: 次がすべて成功し、各 `cargo test` の `running N tests` の N が 0 でない。

```sh
cargo build --release --locked
cargo test --locked --lib && cargo test --locked --test tasks && cargo test --locked --test parallel
node tests/tasks.mjs target/release/tsuzuri && node tests/wasm_threads.mjs target/release/tsuzuri
```

### 手順 7: 生成コードと全体の確認

- 変更: なし。
- 確認: 「生成コードの確認」の `ir-same` と機械語の期待を満たす。`cargo test --locked` が成功。`git diff --stat` が「主な影響ファイル」の Phase 1 の範囲だけ。
  `git diff --exit-code -- src/runtime/task-wasm.ll src/runtime/task-wasm-threads.c src/driver.rs src/llvm.rs src/llvm_parallel.rs` が差分なし。

### 手順 8: 計測

- 変更: なし。
- 内容: 「計測手順」の after・before2・M7 を行う。停止条件（`bulk`・`cpp/task_parallel` の悪化）に当たれば止めて報告する。
- 確認: `target/perf/PR06-after/report.txt` と `drift.txt` があり、`metrics.mjs report` が throw しない。

### 手順 9: 文書

- 変更: 「ドキュメント」の各ファイル。
- 確認: `git diff --check` が空。`docs/benchmarks.md` の数値が `target/perf/PR06-*` の生データと一致する。

## 計測手順

### 環境と条件

PX01 の「計測手順」の「環境と条件」に従う（AC 電源、他の build・test・benchmark を止める、load average の確認）。run ごとに `env.txt` を残す。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
node24() { npx --yes --package=node@24 node "$@"; }
measure() {  # $1: run 名、$2: コンパイラ、$3: runtime の C ファイル（絶対 path）
  RUN=target/perf/$1; mkdir -p "$RUN" || return 1
  { git rev-parse HEAD; git status --porcelain | wc -l; sw_vers; sysctl -n machdep.cpu.brand_string hw.ncpu; uptime
    clang --version | head -1; node24 --version; } > "$RUN/env.txt"
  node24 benchmarks/run-cpp.mjs "$2" --metrics "$RUN" > "$RUN/cpp.json" &&
  node24 benchmarks/run-tasks.mjs "$2" --runtime "$3" --baseline-runtime /tmp/tz-pr06/task-before.c --metrics "$RUN" --artifacts "$RUN/tasks" > "$RUN/tasks.json" &&
  node24 benchmarks/run-tasks.mjs "$2" --runtime "$3" --data-parallel --metrics "$RUN" --artifacts "$RUN/tasks-data" > "$RUN/tasks-data.json" &&
  node24 benchmarks/run-tasks.mjs "$2" --runtime "$3" --sweep --metrics "$RUN" --artifacts "$RUN/tasks-sweep" > "$RUN/tasks-sweep.json"
}
```

### before・after・before2

```sh
measure PR06-before /tmp/tz-pr06/tsuzuri-before /tmp/tz-pr06/task-before.c     # 手順 2 の後
measure PR06-after target/release/tsuzuri "$PWD/src/runtime/task.c"            # 手順 7 の後
measure PR06-before2 /tmp/tz-pr06/tsuzuri-before /tmp/tz-pr06/task-before.c
node24 benchmarks/metrics.mjs report target/perf/PR06-before2 --baseline target/perf/PR06-before > target/perf/PR06-after/drift.txt
node24 benchmarks/metrics.mjs report target/perf/PR06-after --baseline target/perf/PR06-before > target/perf/PR06-after/report.txt
```

- warm-up と標本: run-tasks は種目ごとに cold 1 回の後に warm の反復を測り、9 標本で backend の順序を交互にする。run-cpp は PX01 の既存の 12 標本。
- M1〜M3 の主な証拠は `PR06-after/tasks/result.json` の `pool`（after）と `baseline`（HEAD）の比較（同じ実行で交互）。`PR06-before` の同じ表は同じ runtime
  どうしの A/A 比較で、その差を揺れの目安にする。`drift.txt` で `改善`・`悪化` が出た metric は「判定できない」と報告し、改善を主張しない。
- 生データは `target/perf/PR06-*` に残し Git に加えない。`docs/benchmarks.md` には M1〜M4・M6・M7 の中央値・最小・最大、環境、run の名前、commit を載せる。

### スケーリング（M7）

```sh
for n in 1 2 4 8; do
  printf '#define TZ_TASK_SYSCONF(name) %s\n#include "%s/src/runtime/task.c"\n' "$n" "$PWD" > /tmp/tz-pr06/task-cpu$n.c
  node24 benchmarks/run-tasks.mjs target/release/tsuzuri --baseline-runtime /tmp/tz-pr06/task-cpu$n.c --artifacts target/perf/PR06-after-cpu$n > /dev/null || break
done
```

`baseline` が CPU 数 n に固定した after、`pool` が既定（10）。PX01 形式には入れず、`result.json` だけを残す（variant の `before` と意味が違うため）。

## 生成コードの確認

### IR（コンパイラは不変）

```sh
target/release/tsuzuri build benchmarks/tasks/Main.tz --emit llvm -o /tmp/tz-pr06/after.ll
target/release/tsuzuri build benchmarks/tasks/Main.tz --emit llvm -O3 -o /tmp/tz-pr06/after-O3.ll
cmp /tmp/tz-pr06/before.ll /tmp/tz-pr06/after.ll && cmp /tmp/tz-pr06/before-O3.ll /tmp/tz-pr06/after-O3.ll && echo ir-same
```

### runtime の機械語（arm64、Apple clang 21、`-O3`）

> 2026-10-16 の注記: この小節の期待は採りません（食い違い 7）。HEAD は既に `_pthread_cond_signal` を 5 回参照し、`remaining` を残すので `ldaddal` も残ります。
> 受け入れは lock 回数と signal 回数の assert（「受け入れ条件（改訂）」）で判定します。機械語は参考として `target/perf/PR06-*` の `.s` に残すだけです。

```sh
clang -std=c11 -O3 -S src/runtime/task.c -o /tmp/tz-pr06/after.s
for f in before after; do
  echo "$f"; grep -oE "\b(ldadd|ldaddal|ldaddl|cas|casal)\b" /tmp/tz-pr06/$f.s | sort | uniq -c
  grep -c "_pthread_cond_signal" /tmp/tz-pr06/$f.s
done
```

- before（HEAD で確認済み）: `ldadd` 1・`ldaddal` 1・`ldaddl` 1（`remaining` の減算）、`_pthread_cond_signal` 0。
- after（期待。違えば命令の実際の形を報告に書く）: `ldaddal`・`ldaddl` が 0（`remaining` の削除）、`ldadd` が 1 以上（relaxed の `fetch_add`）、`cas` が 1 以上
  （失敗の CAS）、`_pthread_cond_signal` が 1 以上。x86_64 では `lock xadd` と `lock cmpxchg` になる（この機械では確かめない）。
- 添字ごとに mutex を取らないことの証拠は機械語ではなく、`tests/task_runtime.c` の lock 数の上界である。

## テスト計画

### C ランタイム（`tests/task_runtime.c`）

期待値はすべて設計の規則から数えた値で、現在の実装の出力から取らない。

| 検査 | 内容 | 期待 |
| --- | --- | --- |
| `rendezvous`（既存） | CPU 数ぶんの添字が同時に待ち合わせる | `arrivals == required`、`peak == required - 1` |
| `check_hits`（既存） | 1000 回 × 257 添字、`publish`、`nested`（32 × 257）、`host_group`（8 本） | 全添字が回数どおり、終了後 `groups`・`tail` が `NULL` |
| 最小添字（既存＋手順 3） | `result_stop`、`result_failure` | 3 と 2。失敗未満の添字は 1 回、全添字 1 回以下 |
| lock 数の上界（新規、手順 4） | 257 添字の 1 グループでの `tz_task_lock` の回数 | `required > 1` なら `2 + (required - 1)` 以下、CPU 1 は 0。HEAD は 257 以上で失敗 |
| 起床数（新規、手順 5） | 長さ 2 と 257 の submit での `pthread_cond_signal` の回数 | 1 と `required - 1`（`required > 1` のとき） |
| park の観測（既存＋手順 5） | `parked > 0` を条件変数で待つ | `broadcasts > 0`・`signals > 0` |
| 失敗注入（既存＋手順 5） | `tests/tasks.mjs` の 9 operation | 非 0 終了と `Tsuzuri task runtime: pthread_<operation> failed`（`atexit` は `atexit failed`） |

実行: `tests/tasks.mjs`（`-O0`・`-O3` × CPU 数 `-1`・`0`・`1`・`4`・`1000`）と C 検査（TSan・ASan＋UBSan は CPU 数 2・4・1000）。sanitizer の実行は
`tests/tasks.mjs` に入れない（D10）。

### E2E

- `node tests/tasks.mjs target/release/tsuzuri`: native・WASM × `-O0`・`-O3` の結果順序、`parallel_results` の最小 error と未開始の drop、trap、`live == 0`、
  既定の WASM の import が空。期待値は変えない。
- `node tests/wasm_threads.mjs target/release/tsuzuri`: Phase 1 では runtime が変わらないことの確認。Phase 2 では主な検査（`tz_concurrent` 12、`tz_bulk` 200010000、
  `tz_fail` の trap、heap の live bytes）。
- `cargo test --locked --test tasks`・`--test parallel`・`--lib`（`src/driver.rs` の include の検査を含む）。

### 既存テストへの影響

- `tests/task_runtime.c`: assert の追加と `test_lock` の計数、`test_signal` の追加だけ。`result_failure` は変えない（D6 で `work_available` の broadcast を残す）。
- `tests/tasks.mjs`: operation の配列に `"cond_signal"` を足すだけ。
- その他の期待値は変わらない。

### 性能

合否の閾値は置かない。計測は「計測手順」だけで行い、共有 CI では実行しない。

## ドキュメント

- `docs/architecture.md`: 「ネイティブランタイムは pthread_once により」で始まる段落の 3 文（`mutex が FIFO の active group list と next index の割当を保護し`、
  `remaining は`、`最終 decrement 後の worker は`）を、atomic な添字の取得、`helpers` による参加・離脱、`helpers == 0` の確認と list からの除去を同じ mutex の
  保持で行うこと、起こす worker の数（D3）に置き換える。「性能設計の原則」の表の「複数 CPU コア」の行に「添字は atomic に取得し、起こす worker は仕事の数まで」を足す。
  `tests/task_runtime.c` を説明する段落に lock 数と起床数の上界を足す。
- `docs/benchmarks.md`: 「Task プール」の節のコマンドに `--runtime`・`--sweep`・`--metrics` と 9 標本を反映し、PR06 の before／after（M1〜M4）、M6、M7 の表、
  環境、生データの場所を足す。「7標本」の記述を 9 標本に直す。横断比較の表へ混ぜない。
- `_perfs/README.md`: 状態欄の PR06 を done にする。「現状」の `task_parallel` の値は、言語間の比較を計測し直した場合だけ更新する。
- `_docs/` は変えない（利用者に見える意味は変わらない）。

## 受け入れ条件

> 2026-10-16 の注記: 「着手時の再検討」の「受け入れ条件（改訂）」が優先します。次の項目のうち、機械語の項目は採りません。完了時の状態は項目ごとに書きました。

- [x] C 検査が `c-check-ok`（`-O0`・`-O3` × CPU 数 `-1`・`0`・`1`・`2`・`4`・`1000`、TSan、ASan＋UBSan）。
- [x] 失敗注入 9 種がすべて診断と非 0 終了になる（`cond_signal` を足した 9 種）。
- [x] lock 数と起床数の上界の assert が成功する。
- [x] `node tests/tasks.mjs`・`node tests/wasm_threads.mjs`・`cargo test --locked` が成功する。
- [x] `ir-same` が出て、`task-wasm.ll`・コンパイラに差分がない（`task-wasm-threads.c` は Phase 2 で変えた。起票時のこの項目は Phase 1 だけの条件）。
- [x] 機械語の項目（`after.s` に `ldaddal`・`ldaddl` がなく、`_pthread_cond_signal` がある）は**採らず**（食い違い 7）、lock 回数と signal 回数の assert で判定した。参考に測った機械語（arm64、Apple clang 21、`-O3`）: before は `ldadd` 1・`ldaddal` 1・`ldaddl` 1・`_pthread_cond_signal` の参照 5、
  after は `ldadd` 5（relaxed の取得ほか）・`ldaddal` 3（`remaining` の減算。参加者ごと）・`cas` 1（失敗の停止）・`_pthread_cond_signal` の参照 7。
- [ ] before・after・before2 を PX01 形式で記録し、M1〜M7 を `docs/benchmarks.md` に載せた。**M1〜M4・M6 は載せた**（窓 1 は before・after・before2、最終の runtime は同じ run の A/B と A/A）。
  **M5（`cpp/task_parallel`）と M7（CPU 数 1・2・4・8）は未測定**（`run-cpp.mjs` が Node.js 24 を要し、この機械は 20.19.6。macOS に CPU を固定する手段がない）。`drift.txt` 相当の判定は、A/A の比で代えた。静かな機械での取り直しのコマンドは `docs/benchmarks.md` にある。
- [x] 「ドキュメント」の更新が終わっている。
- [x] GUIDE §10 の完了の定義を満たす（末尾の「実装と検証」に各項目の結果）。

## 落とし穴

- `tz_task_signal` を呼び出しより先に足すと、`-Wall -Wextra -Werror` の未使用関数で `tests/tasks.mjs` の build が落ちる。境界と呼び出しは手順 5 で同時に足す。
- `helpers` の減算を mutex の外で行う、または減算と unlock の後にグループを読むと、呼び出し元が戻った後の stack を触る。離脱の後は list の先頭から走査し直す。
  ASan の `detect_stack_use_after_return=1` で検出できる。
- `length` を unlocked で読めるのは提出後に不変だからである。失敗で `length` を縮める HEAD の処理を戻さない（`next` の CAS で止める）。
- `atomic_compare_exchange_weak_explicit` は偽の失敗をし、失敗時に `next` を書き換える。ループの条件で毎回 `next < length` を確かめる。
- `tz_task_claim` の事前の load を省くと、取り尽くした後も `fetch_add` が続き cache line を奪い合う。正しさは保たれるので、計測で初めて気付く。
- signal を mutex の保持中に行うと、起きた worker がすぐ mutex で待つ。数は保持中に計算し、signal は unlock の後に行う。
- `result_failure` は失敗の経路の `work_available` の broadcast で失敗の記録を観測する。これを消すと CPU 数 2 以上でテストが止まる（D6）。
- `cond_broadcast` の失敗注入は、worker の離脱か `tz_task_shutdown` の broadcast で診断に届く。submit に broadcast を戻して取り繕わない。
- `check_hits` は `tail == NULL` も確かめる。FIFO の `tail` を消さない（D5）。
- `task.c` はコンパイラに埋め込まれる。`cargo build --release --locked` を忘れると `tests/tasks.mjs` と `run-cpp.mjs` は古い scheduler を測り、`run-tasks.mjs` と
  `tests/task_runtime.c` だけが新しいものを使う。
- `run-tasks.mjs` の `--baseline-runtime`・`--runtime` は絶対 path で渡す（clang の cwd はリポジトリ根）。wrapper の `#include` も絶対 path。
- macOS の ASan は leak 検出に対応しない。`detect_leaks=1` を付けない。TSan と ASan を一つの binary に混ぜない。
- F10 の `tz_mutex_held` の検査が入った後なら、`tz_task_submit` の先頭（`length` の速い経路と 1 CPU の分岐より前）から動かさない。1 CPU の機械でだけ
  trap しなくなる。
- Windows はこの機械で build できない。adapter の変更は `pthread_cond_broadcast` と同じ形の 1 関数に留め、`src/driver.rs` の include の検査を `cargo test --locked --lib`
  で通す。
- Phase 2: `memory.atomic.notify` の数は u32。`min(desired - 1, length - 1)` は u32 に収めてから渡す。`check_failed` を取得ごとに呼ばないと、他の worker の trap に
  気付くのが遅れる。

## 対象外

- 非同期 I/O の実行器（B08）、GPU への自動移送（F09）、自動並列化。
- worker ごとの deque と Chase–Lev 型の work stealing（D1）、添字のまとめ取り（D2）、OS ごとの軽量な待機（D4）。
- CPU affinity・cgroup の quota の反映、worker 数を指定する環境変数（D8）。
- thread の stack の大きさの変更（D9。PM09 の範囲）。
- Task の優先度・timeout・取り消しの追加（B06 の契約のまま）。
- Windows 実機での build と計測（G10）。既定の WASM の逐次実行。

## 決定事項

> 2026-10-16 の注記: 各決定の現在の結果は「着手時の再検討」の「決定事項の見直し」にあります（D3・D7・D10 は見直し、D12・D13 は新規）。
> 以下は起票時の記録で、D3・D7 の本文は置き換えられています。

### D1: 取得の方式と work stealing

- 決定: グループごとの `_Atomic uint64_t next` を `fetch_add` で取る方式にする。worker ごとの deque（Chase–Lev など）は入れない。
- 理由: runtime の ABI は「長さが既知のグループの添字」だけを配り、個々の task を spawn する入口がない。グループの共有 counter からの取得は、添字の粒度で
  そのまま負荷を分け合う（空いた thread が残りを取る）。deque は添字の範囲を分割して持つ必要があり、取得の順序が単調でなくなるため、B06 の最小添字の論拠
  （f を取った時点で f 未満は開始済み）が崩れる。seq_cst の fence と伸びる buffer の検証の費用にも見合わない。
- 見直し提案: 起票時の施策「入れ子のグループに worker ごとの deque と work stealing を導入」をこの方式で置き換える。題名は変えない。
- 状態: 既定案（実装者はこの案に従う）

### D2: 1 回に取る添字の数

- 決定: 1 回の `fetch_add` で 1 添字。まとめ取りはしない。
- 理由: `Task.parallel` の 1 添字は 1 Task、`Parallel.*` の 1 添字は最大 4096 要素の chunk（最大 1024 chunk）で、relaxed の RMW 1 回は仕事に比べて小さい。
  まとめ取りは「取得＝開始」を崩し、添字ごとの失敗の確認が要る。D-14 の分割はコンパイラの `parallel_count` が入力長だけで決めており、runtime は関与しない。
- 状態: 既定案（実装者はこの案に従う）

### D3: 起床と完了の通知

- 決定: submit は `min(thread_count, length - 1)` を mutex の下で計算し、unlock の後にその回数だけ `pthread_cond_signal(work_available)` する。完了は worker の
  離脱で `helpers` が 0 になったときの `work_done` の broadcast（pool で一つ）で知らせる。グループごとの条件変数は作らない。
- 理由: 待機中の worker 数を数えない単純な上界で、`tiny` でも 31 本を起こさない。signal の数は並列度だけに影響し進行には影響しない。完了を待つのは入れ子の深さと
  host の thread の数だけで少ない。グループごとの条件変数は POSIX で init・destroy が要り、Windows の adapter にも関数が増える。
- 状態: 既定案（実装者はこの案に従う）

### D4: 待機の API とスピン

- 決定: macOS・Linux は pthread の条件変数、Windows は `task-windows.h` の `SRWLOCK`・`CONDITION_VARIABLE` のまま。futex・`os_sync_wait_on_address`・
  `WaitOnAddress` は使わない。Phase 1 は待機前にスピンしない。スピンは Phase 3 で計測して決める。
- 理由: HEAD の遅延の主因は添字ごとの mutex と全員の broadcast で、条件変数そのものではない。OS ごとの待機は OS の版に依存する API と別の fallback を要し、
  `TZ_TASK_COND_WAIT` の差し替え境界（時間に依らない証明）を通らない。
- 見直し提案: 起票時の既定案「OS ごとの軽量な待機を使い、使えない環境では条件変数に戻す」を置き換える。Phase 1 の計測で park と起床が支配的なら、Phase 3 の
  スピンの後に再検討する。
- 状態: 既定案（実装者はこの案に従う）

### D5: 待つ thread の手伝いとグループの選択順

- 決定: 呼び出し元は自分のグループの添字だけを手伝い、他のグループは手伝わない（HEAD と同じ）。worker は list の先頭から最初に取得可能なグループを選ぶ（FIFO、
  HEAD と同じ）。入口は増やさない。
- 理由: 自分のグループだけを手伝えば stack の深さは入れ子の深さで決まり、待ちの依存が循環しない。F10 が仮定する「join を待つ thread は未配布の仕事を手伝う」と
  `tz_mutex_held` の位置を保つ。選択順を変えると `check_hits` の `tail` の検査と F01 の「active list の選択順」を変えることになる。
- 状態: 既定案（実装者はこの案に従う）

### D6: 失敗の経路

- 決定: `run_result` の 0 以外は `tz_task_record_failure` で mutex の下に記録する。`failure` を最小に更新したときだけ `next` を CAS で `length` まで進め、
  `work_available` を broadcast する。`failure` は atomic にしない。
- 理由: 失敗は冷たい経路で、1 グループで高々添字の数しか起きない。broadcast は HEAD と同じで、`tests/task_runtime.c` の `result_failure` がこれで失敗の記録を観測する。
- 状態: 既定案（実装者はこの案に従う）

### D7: 完了の数え方と公開

- 決定: `remaining` を削除し、「呼び出し元が取り尽くした後に `helpers == 0`」を完了とする。書き込みの公開は `helpers` の減算の unlock と呼び出し元の lock で行う。
- 理由: 各添字の実行者は呼び出し元か参加中の worker に限られるので、`helpers == 0` で全添字が終わっている。1 添字あたりの競合する RMW が 1 回減る。
- 見直し提案: F10 の「前提とする他チケットのインターフェース」は F01 の説明として「`remaining` の acq-rel で公開される」と書いている。保証（戻る前に全書き込みが
  見える）は変わらないが、F10 の文言は PR06 の後に mutex による公開へ直す必要がある。
- 状態: 既定案（実装者はこの案に従う）

### D8: CPU 数と過剰な thread

- 決定: `tz_task_parallelism`（`sysconf(_SC_NPROCESSORS_ONLN)`、1〜32）と常駐 worker `limit - 1` 本を変えない。入れ子・host の thread で worker を増やさない。
  affinity・cgroup・環境変数による上書きは入れない。起こす数は D3 の上界。
- 理由: CPU 数の読み方を変えると利用者に見える並列度が変わり、`tests/task_runtime.c` の `peak` の契約も変わる。計測は wrapper の `TZ_TASK_SYSCONF` で行える（M7）。
- 状態: 既定案（実装者はこの案に従う）

### D9: stack の大きさ

- 決定: `pthread_create` の属性は `NULL` のまま（macOS 536576 bytes、Linux・Windows は OS の既定）。WASM threads の 262144 bytes も変えない。
- 理由: D5 により手伝いで stack が深くならない。stack の大きさは利用者に見える上限で、PM09 の範囲である。
- 状態: 既定案（実装者はこの案に従う）

### D10: 計測 harness と sanitizer の実行場所

- 決定: `run-tasks.mjs` に `--runtime`・`--sweep` を足し、標本を 9 にし、`--metrics` がなければ足す（suite は `tasks`・`tasks-data`・`tasks-sweep`）。
  スケーリングは `TZ_TASK_SYSCONF` を定義する wrapper で測る。TSan・ASan＋UBSan は C 検査として手元で実行し、`tests/tasks.mjs` には入れない。
- 理由: 同じ IR に before と after の runtime をリンクして交互に測るのが最も揺れに強い。PX01 は同じ run に同じ suite を二度書けないので mode ごとに suite を分ける。
  sanitizer の runtime は CI の clang にあるとは限らない。
- 状態: 既定案（実装者はこの案に従う）

### D11: Phase 2・3 の範囲

- 決定: Phase 2 は `task-wasm-threads.c` を「設計」の Phase 分割のとおりにそろえ、`ThreadState`・import・新しいグループから探す順序を変えない。Phase 3 は
  スピンを候補 0・64・1024 から計測で選ぶ。どちらも人間が求めた場合だけ着手する。
- 理由: Phase 1 で native の規則と証明を固めてから移す方が、WASM の host 制御領域を壊す危険が小さい。スピンは CPU 時間を増やすので、計測なしには入れない。
- 状態: 既定案（実装者はこの案に従う）

## 実装と検証（2026-10-16）

ブランチ `perf/phase3/PR06`（基底 `c6864cf5` = main `6d15b437` + 計画の commit）。起票時の記述は F10・E14 の前のコードに基づくので、着手時に `6d15b437` のコードと実測で照合し直し、
「着手時の再検討（2026-10-16）」へ記録しました（D7 と「unlock → signal → claim」の順は採らず、G3 は Phase 1 では動かない）。この節は、その再検討に沿って実装した結果です。

### 実装した範囲（commit と内容）

| 段 | commit | 内容 |
| --- | --- | --- |
| WP0 | `4efd73c2` | 着手時の再検討（食い違い 15 行、Design R、不変条件 I1〜I10、D1〜D13、WP0〜7、改訂した受け入れ条件） |
| WP1 | `a5488f67` | `benchmarks/run-tasks.mjs`: `--runtime`・`--sweep`・`--gap-us`・p50・CPU 時間・suite の分割（`tasks`・`tasks-data`・`tasks-sweep`・`tasks-gap`）。CI の `--quick --metrics` は維持 |
| WP2 | `498d9e6d` | 試験を先に（HEAD の runtime で緑）: `tests/task_stress.c`（新規）、`tests/task_runtime.c` の `#undef NDEBUG` と `hits ≤ 1`、`tests/tasks.mjs` の配線 |
| WP3 | `5ae9256d` | native の核（Design R2）: `next` の atomic な取得、`length` は不変、失敗・trap は cold path で CAS、`remaining` は参加者ごとに 1 回の `fetch_sub`、worker は release と `busy`・`idle` を 1 つの lock の中で動かす（D14）。lock 回数の assert |
| WP4 | `5118945a`・`0525963e` | 起床: broadcast をやめ、submit は signal 1 本（`TZ_TASK_INITIAL_SIGNALS`）、加わった worker は未着手が残れば最大 2 本（`TZ_TASK_CHAIN_SIGNALS`）を起こす木の形。signal・broadcast の回数の assert、`cond_signal` の失敗注入、眠っている pool が必要な数だけ起きる試験、worker が最後に終える group の試験。C4 は初期 2・連鎖 1 で、計測の後に `0525963e` で現在の形にした |
| WP4b | `31442c35` | 試験の強化: macOS は「待つ者がいなかった signal」を覚えていて、連鎖を外しても冷えた起床の試験が止まらなかったので、signal の hook を POSIX の規則（待っている者がいるときだけ届く）に直し、静止を待つ |
| WP5 | `e4e3b7f9`・`f757f593`・`cfd462f4`・`f63da8bd` | 文書（`docs/architecture.md`・`docs/language.md`・Task のページ・README・GUIDE の試験の行）と、計測の記録（`docs/benchmarks.md`）、`benchmarks/run-tasks.mjs` の `coarse` 種目、`benchmarks/run-tasks-wasm.mjs`（新規） |
| WP6 | `30737c9c` | Phase 2: WASM threads の取得・完了を native と同じ形に（`join_one`・`drain`・`release`）。要素 1 の group は公開せず、その場で走らせる。`tests/wasm_threads_stress.mjs`（新規） |
| WP7 | （commit なし） | Phase 3: スピン（20・100 µs）と添字 counter の cache line 分離の実験。作業用の変種で測り、どちらも採用しない（結果は `docs/benchmarks.md`） |
| レビュー対応 | `f548f424` と、続く文書の commit | 統合前のレビューの指摘 2 件。(1) I1・D14 を決定的に守る試験（`tests/task_runtime.c` の `first_child` と `last_worker` の hook）と、変異の検査（「不変条件」の節）。(2) `docs/benchmarks.md` の「連続呼び出しでは worker が起きたままなので差は出ない」を、観測だけの記述に直し、G1 の達成の範囲（gap の tiny・small。連続の tiny は示せていない）を、この節とチケットの G1 に記録した |

### 実装しなかったもの

- WASM threads の起床の絞り込み（`memory.atomic.notify` の個数指定）と、ロックの 3 状態化（uncontended の解錠で notify しない）: 眠るスレッドは idle worker・join・lock 待ち・チャンネル待ちが同じ `epoch` の語を待ち、
  個数指定は別の理由で眠っているスレッドを起こして idle worker を飢えさせうる（そうなると pool は誤った deadlock 判定を出す）。host の `fail()` と `bindings-threads.mjs` の変更が要るので、`ThreadState`・import・poison の規則を変えない方針では見送りました。
- worker ごとの deque と work stealing（D1）、複数の添字をまとめて取る方式（D2）、CPU 数・stack の大きさの変更（D8・D9）、待機前のスピン（D4。実験のみ）。
- `cpp/task_parallel` の改善の主張（G3）と、その before／after の測定（M5）、CPU 数を変えた拡大（M7）。
- Windows の実機での実行（`windows.yml` の green だけが証拠）。

### 確認コマンドと結果

- C 検査: `tests/{task,channel,sync,trap_boundary}_runtime.c`（CPU 数 -1・0・1・2・4・1000、channel は 1〜4、trap_boundary は 0・1・2・4・32）と `tests/task_stress.c`（CPU 数 / host 数 1/3・2/1・2/4・3/3・4/1・4/6・8/8）を
  `-O0`・`-O3` で、素の実行・ThreadSanitizer・AddressSanitizer＋UBSan のすべてで通した（最終の `task.c` で再確認）。`-Os`・`-Oz`（PM08 が使う）でも `task_runtime.c`・`task_stress.c` が通る。
  取りこぼしの探索として、`task_runtime.c` を CPU 数 2・3・4・8・32 で各 150〜500 回、`task_stress.c` を単一 host（CPU 2〜8）で 1500〜3000 round、複数 host（最大 16 host × 16 CPU、32 CPU × 4 host）で実行し、失敗も hang もなかった。
  変異の検査: 連鎖を外すと CPU 3 以上で冷えた起床の試験が止まり、submit の signal を外すとすべての CPU 数で assert が落ちる。
  レビュー後に足した I1・D14 の検査（「不変条件」の節）は、最終の `task.c` に対して CPU 数 2・3・4・8・32 の `-O0`・`-O3` で素の実行を各 200 回、ThreadSanitizer・AddressSanitizer＋UBSan を各 40 回、失敗も hang もなかった。
  I1 の変異 1 つと D14 の変異 3 つは、同じ 10 構成のすべてで 20 回中 20 回 assert で落ちる。
- `node tests/tasks.mjs target/release/tsuzuri` が、素の実行・`TSUZURI_TSAN=1`・`TSUZURI_ASAN=1` で通る（native／WASM、4 trap、`-O0`・`-O3`。失敗注入は `cond_signal` を足した 9 種）。
  `tests/trap_return.mjs`・`tests/trap_return_sync.mjs`・`tests/trap_boundary.mjs`（17 case、`-O0`・`-O3`）・`tests/features.mjs concurrency channel`（71 case）が通る。
  channel_threads の E2E は、CPU 1〜4・`-O0`/`-O3` で最終の runtime に 2896 回繰り返して失敗 0（Phase 1 の各段階でも 1456 回・2896 回）。
  Task を native で走らせるほかの suite も、最終の release コンパイラで通る: `tests/async.mjs`（native の test・debug test・bench の `-O0`/`-O3`。JSPI の検査は Node 20 のため省略）、`tests/stack_overflow.mjs`（20 case）、`tests/allocator.mjs`。
- WASM threads: `node tests/wasm_threads.mjs`（変更なしで `-O0`・`-O3`）、新規の `tests/wasm_threads_stress.mjs`（1・2・3・5・7 worker、25 round と 12 round × 8 回、CPU を埋めた状態も。前の runtime でも通る）、`tests/bindings_threads.mjs`（ブラウザと tsc の検査は環境がなく省略）が通る。
  WASM threads の object は `-O3` で 8731 → 9481 バイト、`-Oz` で 6276 → 6645 バイト。
- 生成 IR: `tests/fixtures/tasks`・`examples/tasks` の `--emit llvm` は基準コンパイラと `cmp` で一致（native、`-O0`・`-O3`）。同じコンパイラで 2 回作った出力（native IR、wasm32、wasm32 threads）も一致する。
  既定の wasm32 は基準と byte で一致し、threads の wasm は runtime が変わった分だけ違う。コンパイラ（`src/*.rs`）は変えていない。
- GUIDE §3.1 の回帰テスト 4 件（既定の stack で）が通る。
- Rust: `RUST_MIN_STACK=4194304 cargo test --locked`（全体、`with-lock.sh heavy` の下）が、`30737c9c` で 101 の test binary・1305 件、最終の木（コードは `0525963e`。その後は文書と計測用の script だけ）でも
  101 の test binary・1305 件（ignored 1）で通り、失敗 0。`--test tasks`（13）・`--test parallel`（2）・`--test concurrency`（19）は最終の runtime で個別にも通る。`cargo fmt --all -- --check`、`cargo clippy --all-targets --locked -- -D warnings`、
  `sh scripts/check-runtime-includes.sh`（47 ファイル）が通る。
- Windows: `cargo check --all-targets --locked --target {x86_64,aarch64}-pc-windows-msvc` が通る（`cargo +stable` は `/opt/homebrew/bin/rustc` を先に見つけて `core` を探せないので、
  `PATH=<stable の bin>:$PATH cargo check ...` で実行した）。Rust のファイルは変えていない。同梱の zig で `tests/task_runtime.c` と `src/runtime/task.c` を `x86_64-windows-gnu`・`aarch64-windows-gnu`（`-O0`・`-O3`、`-Wall -Wextra -Werror`）へ
  compile した（実行はできない）。`task_runtime.c` は `#undef NDEBUG` で始まる（zig cc は `-O1` 以上で `NDEBUG` を定義し、`assert(...)` の中の呼び出しが消えるため）。Windows の green は `windows.yml` の実行でだけ確かめられる。
- 文書: `node scripts/check-docs.mjs`（Task のページと WebAssembly のページ: 57 の link、12 の例、18 の native 実行）が通る。

### 計測

条件と結果の全体は `docs/benchmarks.md` の「Task プールのスケジューラ（PR06、2026-10-16）」にあります。生データは `target/perf/PR06-*`（git 管理外）、before のコンパイラは `target/perf/PR06/tsuzuri-before`（SHA-256 `da78ba18…55fb`）です。
計測は他の agent がビルドしている共有機（load average 20〜100）で、排他の窓（`with-lock.sh measure`）を 2 回取りました。最終の runtime の値は排他の窓ではなく、A/A の揺れ付きです。**静かな機械での正式値は、統合後に取り直します**（コマンドは同じ節の末尾）。

- 回数（機械に依らない）: 257 添字の group の mutex の取得は `Task.parallel` で 259、`Task.parallel_results` で 515 だったのが、`2 + (CPU 数 - 1)` 以下になった。broadcast は 2 から 1 以下、signal は 1 〜 `1 + 2 × (CPU 数 - 1)`。
- 眠った worker の後の最初の呼び出し（1 ms 休む）: 窓 1（C4 の runtime）で tiny 16.1 → 6.17 µs、small 21.8 → 5.83 µs、bulk 590.8 → 252.3 µs。最終の runtime は HEAD 比で tiny・small の p50 が 0.17〜0.27 倍、bulk が 0.46〜0.47 倍、CPU が 0.13〜0.42 倍（A/A は 0.76〜1.13）。
- 粒度: 窓 1 で、並列が逐次と並ぶ最小の逐次時間が約 168 µs から約 42 µs になった。256 rounds（約 10 µs）の group は、どちらも逐次より遅いまま。
- 休まない連続呼び出し: bulk は窓 2 の全変種で HEAD の 0.41〜0.52 倍。tiny の p50 は、HEAD が測った全 run で 0.17〜0.21 µs なのに対し、signal を使う方針は run ごとに 0.17〜0.21 µs か 0.8〜7 µs になる（最終は、このチケットの 3 run 中 1 run が 1.0 µs、
  統合前のレビューの再測定の 1 run が 2.21 µs）。small の p50 は HEAD 自身が 1〜42 µs の間で動くので、差を確定できない。原因は調べていない。
- 目標 G1（tiny・small の warm を HEAD より短くする）: 眠った worker の起床（gap）では tiny・small とも満たした。連続呼び出しの small は、窓 1（C4 の runtime）で平均が 16.2 → 2.39 µs（before2 は 17.3 µs）と短くなったが、
  最終の runtime の非排他の run では 0.33〜1.90 倍で A/A（0.80〜1.42）と区別できない。**連続呼び出しの tiny は、改善を示せていない**（p50 が上がる run があり、平均に差が読めない）。静かな機械での取り直しで確かめる。
- coarse（16 添字 × 約 79 µs、`cpp/task_parallel` と同じ形）: 全方針で HEAD と揺れの範囲で同じ（0.94〜1.17）。G3 は主張しない。
- WASM threads（9 worker、Node 20）: tiny の連続 1.68 → 0.88 µs、small 4.68 → 2.90 µs、bulk の p50 544 → 141 µs（before は 0.21 ms と 0.82 ms の二つの値に割れる）、1 ms 休んだ後の p50 は tiny 40.3 → 29.5 µs、small 88.4 → 54.6 µs、bulk 823 → 545 µs。cold は変わらない。
- 不採用: 添字 counter を別の cache line に置く（差なし）、眠る前の 20・100 µs の spin（連続の平均が tiny ×7〜9、small ×5〜7、CPU が ×4〜×40。休み 50・200 µs でも p50 に利益なし）。
- 主張しないこと: `cpp/task_parallel`（M5）、CPU 数別の拡大（M7）、Linux・Windows の時間。CI に速度の合否の閾値は入れていない。

### 決定事項の追記と、見つけた問題

- 決定事項（結果は「着手時の再検討」の「決定事項の見直し」の表）: D3 は初期 signal 1・連鎖 2 の木の形を採った（起票の既定案の `min(thread_count, length - 1)` 本は採らない。C4 の初期案は初期 2・連鎖 1 で、計測の後に変えた）。
  D4 はスピンなし（実験は不採用）。D7 は採らない（`remaining` を残す）。D10 は `--runtime`・`--sweep`・`--gap-us` に加え、`coarse` 種目と `benchmarks/run-tasks-wasm.mjs` を足した。D11 の Phase 2 は書き直した範囲だけ実装した（上の「実装しなかったもの」）。D12〜D14 は新規。
- 最初の子どもを公開の前に取る規則（食い違い 4）は、F10 の規則として保った。起票時の「unlock → signal → claim」の順は採らない。
- F10 の誤判定（native、修正済み）: 単一の host の 2 段の pipeline が、直前の group の worker がまだ走っているものとして数えられている間に block すると、空きの worker がいないと見て consumer を自分の上へ積み、
  「deadlock」と誤判定した（HEAD で 60000 group 中 1〜11 件、CPU 2〜3）。直前の group の最後の worker が、`remaining` を減らして投入側を帰した後で `busy--`・`idle++` していたのが原因で、
  同じ critical section で行う（D14）と 0 / 180000 になった。WASM threads の `run_item` は元から同じ critical section だった。`tests/task_stress.c` が「単一の host で段数が CPU 数以下なら判定を出さない」を assert するが、
  これは偶然に頼る検査で（誤判定は数千 group に 1 件ほど）、決定的な検査は `tests/task_runtime.c` の `last_worker` と `first_child`（「不変条件」の節）にある。
- macOS の `pthread_cond_signal` は、待つ者がいなかった signal を覚えていて、後から眠った worker を起こす。signal の取りこぼしの変異（連鎖を外す）が冷えた起床の試験で見つからなかったので、
  hook を POSIX の規則に直した（WP4b）。Linux・Windows の取りこぼしの検出は CI の実行でだけ確かめられる。
- 起床の生存性（トークンの数が、尽きていない group の数以上）は論証で、証明ではない。stress と変異の検査で確かめた。
- `cargo +stable` が Windows target の `core` を見つけられない（上の Windows の項）。
- 共有機のロック（`with-lock.sh`）: 保持中の job を `stop_bash` で止めると slot が孤児になり、全員が待ち続ける（今回 14:28〜14:58）。実行中のスクリプトを書き換えると、動いている `sh` が新しいファイルの途中を読んで
  コマンドを再実行する。統合側で気を付けてほしい。

### 更新した `_tsuzuri/language-reference/` のページ

- `async-tasks-and-lazy/task.md`: native の pool の説明（添字を atomic に配ること、mutex の取得回数、起床の数と木の形）。Windows の説明を「Win32 のスレッドプール」から `CreateThread` と SRWLOCK・条件変数の常駐 pool に直した。
- `compiler/webassembly.md`: WASM threads で、グループを始めたスレッドが最初の子どもを公開する前に取る、という説明に直した。
- 例と `run=` の出力は変えていない。`node scripts/check-docs.mjs` は 2 ページで通る。

### 変更したファイル

- runtime: `src/runtime/task.c`、`src/runtime/task-wasm-threads.c`。
- 試験: `tests/task_runtime.c`（assert の追加と、`hits[2] == 1` を `hits[0..2] == 1` に強めた）、`tests/task_stress.c`（新規）、`tests/tasks.mjs`、`tests/wasm_threads_stress.mjs`（新規）。
- 計測: `benchmarks/run-tasks.mjs`、`benchmarks/run-tasks-wasm.mjs`（新規）。
- 文書: `README.md`、`_features/GUIDE.md`（試験の行）、`docs/architecture.md`、`docs/language.md`、`docs/benchmarks.md`、`_tsuzuri/language-reference/async-tasks-and-lazy/task.md`、`_tsuzuri/language-reference/compiler/webassembly.md`、
  このチケット（`_perfs/` から `_perfs/_completed/` へ移動）。
- 変えていない: `src/*.rs`（コンパイラ）、`src/runtime/task-wasm.ll`、`src/runtime/task-windows.h`、`tests/wasm_threads.mjs`、`tests/channel_runtime.c`・`sync_runtime.c`・`trap_boundary_runtime.c`、`_perfs/README.md`（統合側が PR06 の行を更新する）。
