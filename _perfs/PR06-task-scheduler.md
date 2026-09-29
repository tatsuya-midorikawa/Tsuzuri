# PR06: Task プールの低遅延化と work stealing

| 項目 | 内容 |
| --- | --- |
| ID | PR06 |
| 分類 | 実行速度 |
| 優先度 | P2 |
| 規模 | L |
| 依存 | PX01 |
| 関連 | F01, F02, B06, F06, F10 |
| 状態 | todo |
| 起票 | 2026-09-29。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 不要（D1・D4 は起票時の施策・既定案を置き換える既定案。D7 は F10 の文言の修正が必要。各項の「見直し提案」を参照） |
| 手本にする既存実装 | 原子的な添字の取得: `src/runtime/task-wasm-threads.c` の `Group`（`atomic_uint_least64_t next` を `atomic_fetch_add_explicit` で取る）と `run_one`。pthread 呼び出しの差し替えと時間に依らない観測: `tests/task_runtime.c` の `test_broadcast`・`test_wait`（`parked`・`broadcasts`・`peak`）と、失敗名を `argv` で渡す子プロセス方式（`tests/tasks.mjs` の `for (const operation of [...])`）。同じ IR に旧 runtime をリンクする比較: `benchmarks/run-tasks.mjs` の `--baseline-runtime` |
| 主な影響ファイル | `src/runtime/task.c`, `src/runtime/task-windows.h`, `tests/task_runtime.c`, `tests/tasks.mjs`, `benchmarks/run-tasks.mjs`, `docs/architecture.md`, `docs/benchmarks.md`, `_perfs/README.md`（状態欄）。Phase 2: `src/runtime/task-wasm-threads.c`, `tests/wasm_threads.mjs`。変えない: `src/runtime/task-wasm.ll`、コンパイラ（`src/*.rs`） |
| 計測対象 | `benchmarks/run-tasks.mjs` の Task runtime 比較（`tiny`・`small`・`bulk`）と `--data-parallel`（`small`・`chunk-boundary`・`bulk`）、`benchmarks/run-cpp.mjs` の `cpp/task_parallel`・`cpp/task_sequential` |

## 目的

`Task.parallel`・`Task.parallel_results`・`Parallel.*` が共有する native の C scheduler（`src/runtime/task.c`）から、添字ごとの pool 全体 mutex、
全 worker を起こす broadcast、添字ごとの完了通知を除き、短い仕事の起動・完了待ちの遅延と、添字が多いグループの配布費用を減らす。
結果の順序・`Task.parallel_results` の最小添字の失敗・未開始の停止・全 join・trap と runtime 失敗の診断（F01・B06）は変えない。
.NET の thread pool や rayon と同等以上の fork/join 性能を目標にするが、主張するのは計測した結果だけにする。

- Phase 1（native、本チケットの実装範囲）: `task.c` の取得・起床・完了待ちの規則を変える。Windows の adapter（`task-windows.h`）に 1 関数を足す。
- Phase 2（WASM threads）: `task-wasm-threads.c` を同じ規則にそろえる。
- Phase 3（待機前のスピン）: Phase 1 の計測で park と起床が支配的なときだけ評価する。

実装者は Phase 1 だけを実装する。Phase 2・3 は人間が求めた場合だけ着手する。既定の WASM（`task-wasm.ll` の逐次実行）はどの Phase でも変えない。

## 着手条件と停止条件

### 着手条件

- PX01 が `_perfs/README.md` の「一覧」の状態欄で done であること。確認: `grep -n "PX01" _perfs/README.md` と `ls benchmarks/metrics.mjs`。
  PR06 が使うのは PX01 の `benchmarks/metrics.mjs`（`makeRecord`・`writeRecords`・`report`）と `run-cpp.mjs --metrics` だけである。
  `run-tasks.mjs --metrics` は PX01 の Phase 2 の範囲なので、未実装なら PR06 の手順 2 で PX01 の「runner の対応」の規則どおりに足す。
- GUIDE §2.3 の基準コマンドが成功し、手順 1 の before（HEAD の `task.c` の写し、`tsuzuri-before`、計測）を保存していること。
- F10 が done なら、`grep -n "tz_mutex_held" src/runtime/task.c` で `tz_task_submit` の先頭の検査を確かめ、その位置（1 CPU の分岐と `pthread_once` より前）を保つ。
  F10 が todo なら何もしない（F10 はこの位置に検査を置くと仮定している）。

### 前提とする他チケットのインターフェース

- PX01: PX01 形式（schema 1 の JSON Lines、`target/perf/<run_id>/<suite>.jsonl`）、`node benchmarks/metrics.mjs report <run> --baseline <run>`、
  `run-cpp.mjs --metrics <dir>`、PX01 の「計測手順」の before／after（before・after・before2）。
- F10（todo、並行して詳細化中）: F10 は、PR06 が `tsuzuri_task_parallel`・`tsuzuri_task_parallel_results` の ABI と「join を待つ thread は自分のグループの
  未配布の仕事を手伝う」保証を保つこと、入口を増やさないことを仮定する。PR06 はどちらも保つ（D5）。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- `tsuzuri_task_parallel`・`tsuzuri_task_parallel_results` の引数・戻り値・`TZ_TASK_API` の属性、またはコンパイラ（`src/*.rs`）や `task-wasm.ll` を変える必要が出た。
- 既存の `tests/task_runtime.c`・`tests/tasks.mjs`・`tests/wasm_threads.mjs` の assert を弱める・消す必要が出た（「既存テストへの影響」に挙げた追加・置換を除く）。
- 証明のテストが sleep・時間の閾値・再試行なしには通らない。
- TSan・ASan・UBSan の報告を、設計の手順（添字の取得に mutex を使わない）を崩さずに直せない。
- `pthread_cond_signal`（Windows は `WakeConditionVariable`）以外の新しい OS API、`libatomic` などの追加ライブラリ、C11 以外の言語機能が必要になった。
- 手順 8 の計測で、after が before より `bulk`・`cpp/task_parallel` の時間で広がりを超えて遅い。スピンや定数の調整で取り繕わない。
- Phase 2 で `ThreadState`（`_Static_assert` の 28 bytes）か import（`tsuzuri_threads` の `spawn_workers`・`worker_ready`）を変える必要が出た。

## 現状と計測（HEAD `f8dc655`）

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
- G2: 添字が多いグループ（`bulk`、`--data-parallel` の `chunk-boundary`・`bulk`）の配布費用を減らす。F01 で旧 runtime より遅くなった `bulk` の warm を HEAD より短くする。
- G3: `cpp/task_parallel` の wall を C# の最速と同等以上にする（起票時の目標。未達でも Phase 1 の完了は妨げない）。
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

- [ ] C 検査が `c-check-ok`（`-O0`・`-O3` × CPU 数 `-1`・`0`・`1`・`2`・`4`・`1000`、TSan、ASan＋UBSan）。
- [ ] 失敗注入 9 種がすべて診断と非 0 終了になる。
- [ ] lock 数と起床数の上界の assert が成功する。
- [ ] `node tests/tasks.mjs`・`node tests/wasm_threads.mjs`・`cargo test --locked` が成功する。
- [ ] `ir-same` が出て、`task-wasm.ll`・`task-wasm-threads.c`・コンパイラに差分がない。
- [ ] `after.s` に `ldaddal`・`ldaddl` がなく、`_pthread_cond_signal` がある。
- [ ] before・after・before2 を PX01 形式で記録し、M1〜M7 を `docs/benchmarks.md` に載せた。改善と書く指標は `drift.txt` で「判定できない」でないものだけ。
- [ ] 「ドキュメント」の更新が終わっている。
- [ ] GUIDE §10 の完了の定義を満たす。

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
