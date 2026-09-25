# F01: 常駐ワーカープール
| 項目 | 内容 |
|---|---|
| ID | F01 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | なし |
| 後続 | F02, F06, B06 |
| 状態 | todo |
| 主な影響ファイル | `src/runtime/task.c`, `tests/task_runtime.c`, `tests/tasks.mjs`, `docs/language.md`, `docs/architecture.md`, `docs/benchmarks.md`, `README.md`, `benchmarks/run-tasks.mjs`（新規）, `benchmarks/tasks/*`（新規） |

## 目的

- `Task.parallel` のネイティブ実行で、呼び出しごとの `pthread_create` / `pthread_join` を避ける。
- 小さい仕事を多数投げる実用コードで、スレッド起動費用を毎回払わない実行基盤にする。
- 既存の言語意味を変えず、ユーザーから見える detach / background 実行を導入しない。
- `Task.run` と `Task.parallel` は引き続き同期的な fork/join で、呼び出しが戻った時点で全仕事が完了している。
- 入れ子の `Task.parallel` でも、ワーカー枯渇によるデッドロックを作らない。
- F02 の `Parallel.*`、F06 の WASM threads、B06 のキャンセル設計が再利用できるグループ実行の土台にする。

## 現状

- 調査時点は `_features/GUIDE.md` の基準コミット `19d8cdd`。
- `src/runtime/task.c` は POSIX pthread を直接使う C ランタイムである。
- `src/runtime/task.c` の `TZ_TASK_MAX_THREADS` は `32`。
- `src/runtime/task.c` の `tz_task_workers` は、実行中または作成済み未 join の追加 OS スレッド数を表す `atomic_uint`。
- `src/runtime/task.c` の `tz_task_limit` は `sysconf(_SC_NPROCESSORS_ONLN)` から遅延初期化する上限。
- `src/runtime/task.c` の `tz_task_parallelism` は CPU 数が取れない場合を `1` とみなし、追加スレッドなしにする。
- `src/runtime/task.c` の `tz_task_group` は `run`, `context`, `length`, `_Atomic uint64_t next` を持つ。
- `src/runtime/task.c` の `tz_task_work` は `atomic_fetch_add_explicit(&group->next, 1, memory_order_relaxed)` で添字を取り、範囲外で終了する。
- `src/runtime/task.c` の `tsuzuri_task_parallel` は `__attribute__((weak, visibility("hidden")))` 付きの外部ランタイム入口。
- `src/runtime/task.c` の `tsuzuri_task_parallel` は `length == 0` なら戻り、`length == 1` なら呼び出し元で `run(context, 0)` する。
- `src/runtime/task.c` の `tsuzuri_task_parallel` は呼び出しごとに最大 `TZ_TASK_MAX_THREADS - 1` 個の `pthread_t threads[]` を作る。
- `src/runtime/task.c` の `tsuzuri_task_parallel` は `atomic_compare_exchange_weak_explicit(&tz_task_workers, ...)` で共有枠を取得する。
- `src/runtime/task.c` の `tsuzuri_task_parallel` は追加スレッドが 0 個なら添字順に逐次実行する。
- `src/runtime/task.c` の `tsuzuri_task_parallel` は追加スレッドがある場合、呼び出し元も `tz_task_work(&group)` を実行する。
- `src/runtime/task.c` の `tsuzuri_task_parallel` は最後に全 `pthread_join` を行い、join 後に `tz_task_workers` を減らす。
- `src/runtime/task.c` の `tz_task_fail` は `Tsuzuri task runtime: <operation> failed (<errno>)` を stderr に出して `abort()` する。
- `src/runtime/task-wasm.ll` の `@tsuzuri_task_parallel` は import なしで `0..length-1` を順に呼ぶ逐次実装。
- `src/llvm.rs` の `emit_target` は出力 IR に `@tsuzuri_task_parallel(` が現れたときだけ native では宣言、WASM では `runtime/task-wasm.ll` を連結する。
- `src/llvm.rs` の `FunctionEmitter::parallel_tasks` は `%tz.array` のタスク配列と結果配列を使う。
- `src/llvm.rs` の `FunctionEmitter::parallel_tasks` は `@tz.task.item.<llvm type>` という internal callback を `intrinsics: BTreeSet<String>` に登録する。
- `src/llvm.rs` の `FunctionEmitter::parallel_tasks` は callback ABI を `void (ptr context, i64 index)` に固定している。
- `src/llvm.rs` の `FunctionEmitter::parallel_tasks` の context は `{ ptr source, ptr target }` の stack alloca。
- `src/llvm.rs` の `FunctionEmitter::parallel_tasks` は `tsuzuri_task_parallel` の後に入力タスク配列の `source` を `@tz.free` する。
- `src/driver.rs` の `build` は native かつ IR に `declare void @tsuzuri_task_parallel(` がある場合だけ `src/runtime/task.c` を一時ファイルに書き、`clang -std=c11 -fPIC -pthread -c` でランタイム object を作る。
- `src/driver.rs` の `build` は `--emit object` の場合、ランタイム object と module object を `clang -r -nostdlib` で結合する。
- `src/driver.rs` の `build` は wasm32 の場合 `-mbulk-memory` を clang に渡し、`wasm-ld` に `--stack-first`, `-z stack-size=1048576`, `--max-memory=16777216` を渡す。
- `tests/task_runtime.c` は `#define sysconf test_sysconf`, `#define pthread_create test_create`, `#define pthread_join test_join` で `src/runtime/task.c` を include し、能力検出と pthread 呼び出しを差し替える。
- `tests/task_runtime.c` の `rendezvous` は条件変数で 4 本の同時到着を検査し、速度や sleep に依存しない。
- `tests/task_runtime.c` の `nested` は入れ子の `tsuzuri_task_parallel` を検査する。
- `tests/task_runtime.c` の `host_group` は外部の複数 pthread から同時に `tsuzuri_task_parallel` を呼ぶ。
- `tests/tasks.mjs` は `tests/task_runtime.c` を `clang -std=c11 -Wall -Wextra -Werror -pthread` で直接ビルドし、作成失敗と join 失敗の診断を検査する。
- `tests/tasks.mjs` は native/WASM の `-O0` と `-O3`、結果順序、trap、所有値の解放、複数 object の weak runtime 結合を検査する。
- 現状の docs は `docs/language.md` と `docs/architecture.md` で「常駐ワーカープールはまだない」と説明している。
- `docs/benchmarks.md` は `Task.parallel` について「既存比較は速度比較ではない」「常駐ワーカープールは未実装」と明記している。

## 仕様

### ユーザー可視の意味

- Tsuzuri の構文・型・`Task.parallel` の型は変更しない。
- `Task.parallel : [Task<'a>] -> Task<['a]>` の結果配列の順序は入力配列の順序と同じ。
- タスク配列を作る式、各タスクの捕捉、`Task.run` の引数評価順序は既存どおり左から右。
- グループ内の各仕事の実行開始順・完了順は未規定のまま。
- 各仕事の内部の評価順序、所有権、借用、数値演算、trap は既存どおり。
- `tsuzuri_task_parallel` は戻る前に、渡された `run(context, index)` の全 index を完了させる。
- ユーザーから見える detach、fire-and-forget、join handle、スレッド ID は導入しない。
- `Task.run` が返った後に、その `Task.parallel` から派生した worker がユーザーコードを実行し続けていてはならない。
- ただし常駐ワーカーの OS スレッド自体はプロセス内で待機してよい。
- トラップ、`abort()`、メモリ不足、非停止時の巻き戻しや兄弟タスクのキャンセルは保証しない。
- 正常終了時は、既存の所有値回収規則と `tests/tasks.mjs` の `live == 0` を維持する。

### 並列度

- ランタイム全体で作る常駐ワーカー数は、既存と同じ実行上限から 1 を引いた数にする。
- 上限式は `max_extra = min(online_cpus, 32) - 1`。
- `online_cpus < 1` または取得失敗の場合、`online_cpus` は `1` とみなし、`max_extra == 0`。
- `max_extra == 0` の場合、追加スレッドは作らず、呼び出し元だけで逐次実行する。
- `length == 0` は何もしない。
- `length == 1` は呼び出し元で `run(context, 0)` する。
- 常駐ワーカーは初回の `length > 1` かつ `max_extra > 0` の呼び出しで遅延起動する。
- 起動する常駐ワーカー数は `max_extra` とする。
- 起動済みワーカー数は以後のグループ間で再利用し、グループ完了ごとに破棄しない。
- 明示的なスレッド数指定はこのチケットでは導入しない。

### グループと進行保証

- 各 `tsuzuri_task_parallel` 呼び出しは stack 上の `tz_task_group` を作る。
- `tz_task_group` は最低限 `run`, `context`, `length`, `next`, `remaining`, `next_group`, `done` 相当の情報を持つ。
- `next` はそのグループ内の次の添字を配る `_Atomic uint64_t`。
- `remaining` は未完了の添字数を表す `_Atomic uint64_t`。
- ワーカーと呼び出し元は `next` から添字を取り、`index < length` のときだけ callback を呼ぶ。
- callback 完了後に `remaining` を減らす。
- `remaining` が 0 になった worker は完了待ちの condvar を signal/broadcast する。
- callback は `src/llvm.rs` の `FunctionEmitter::parallel_tasks` が生成する `@tz.task.item.*` のように、結果配列へ `store` してから完了を報告する。
- そのため `remaining` の RMW は relaxed では join として不十分。
- `remaining` を減らす全 callback 完了経路は `atomic_fetch_sub_explicit(&group->remaining, 1, memory_order_acq_rel)` を使う。
- caller が `tsuzuri_task_parallel` から戻る直前、かつ active list から group を外す前に、`atomic_load_explicit(&group->remaining, memory_order_acquire) == 0` を確認する。
- または `remaining` の decrement と 0 判定を常に pool mutex 下で行い、caller も同じ mutex 下で 0 を確認する。この場合も result memory の可視化を mutex acquire/release で説明する。
- 推奨は `next` relaxed、`remaining` acq-rel / acquire load。
- メモリ順序の根拠: worker の callback 内の result store は、同じ thread の `remaining` acq-rel decrement より前に sequenced-before される。caller の acquire load が 0 を観測すると、最後の decrement と synchronizes-with し、release sequence を通じて全 worker の completion RMW 前の result store が caller に見える。caller はその後に結果配列を返すため、join 後の読み取りは未同期の書き込みを読まない。
- 呼び出し元は自分のグループを優先して進める。
- 自分のグループの未配布添字がないが未完了仕事が残る場合、呼び出し元は他の ready グループを手伝ってよい。
- 手伝える仕事が全くない場合だけ condvar で park する。
- 入れ子グループの caller も同じ規則で、待っている間に他グループの仕事を進める。
- この規則により、全ワーカーが入れ子グループの join 待ちになっても、join 待ちの thread 自身が work stealing して進行できる。
- `pthread_mutex_t` は ready group list と condvar 状態の保護だけに使い、callback 実行中は保持しない。
- callback 実行中に global mutex を保持してはならない。
- global mutex を保持したまま Tsuzuri callback、`@tz.alloc`、`@tz.free`、`pthread_join` を呼んではならない。
- atomic work index `next` だけは `memory_order_relaxed` でよい。これは各 index の一意な割り当てだけに使い、result memory の publish には使わないため。
- `remaining` の完了通知は acq-rel RMW と acquire join load、または mutex 保護で result memory の publish を保証する。
- C のデータレースを避けるため、active group list の連結変更は必ず mutex 下で行う。
- stack 上の `tz_task_group` は active list から外し、どの worker も参照しなくなった後で関数から戻る。

### parking / wakeup

- idle な常駐 worker は `pthread_cond_wait` で park する。
- 新しい group が active list に入ったら `pthread_cond_broadcast(&pool->work_available)` する。
- group の `remaining` が 0 になったら `pthread_cond_broadcast(&pool->work_done)` する。
- caller join loop は「自分で実行できる work がない」ことを mutex 下で再確認してから `work_done` を待つ。
- spurious wakeup は許容し、全 wait は while loop で条件を再確認する。
- wakeup の正しさは timing や sleep に依存させない。
- `tests/task_runtime.c` では条件変数とカウンターで待機・起床を観測する。

### プロセス終了

- 正常な `Task.run` / `Task.parallel` の戻り時点でユーザー仕事は完了している。
- 常駐 worker はプロセス終了まで残ってよいが、ASan/TSan とテストのために shutdown 経路を用意する。
- 初回 pool 起動時に `atexit(tz_task_shutdown)` を登録する。
- `tz_task_shutdown` は `stopping = true` を設定し、worker condvar を broadcast し、起動済み worker を join する。
- `pthread_join` 失敗は `tz_task_fail("pthread_join", error)` と同じ診断で abort する。
- `atexit` 登録失敗を検出できる環境では `tz_task_fail("atexit", error)` とする。
- `abort()` や trap で異常終了する場合、shutdown と捕捉値解放は保証しない。これは既存の trap 規則と同じ。

### 失敗診断

- 常駐 worker の `pthread_create` 失敗は現状と同じく stderr に `Tsuzuri task runtime: pthread_create failed (<errno>)` を出して `abort()`。
- mutex / condvar / once / join の失敗も `tz_task_fail("<operation>", error)` で明示し、成功形の結果を返さない。
- `pthread_cond_wait` 失敗も同じ。
- CPU 数取得失敗はエラーではなく、既存どおり追加スレッドなしの逐次実行。
- 明示的な fallback 成功に見せかける silent default は禁止。

### シンボルとリンク

- 外部から見えるランタイム入口は引き続き `tsuzuri_task_parallel` だけ。
- `tsuzuri_task_parallel` は `__attribute__((weak, visibility("hidden")))` を維持する。
- 内部 helper と global state は `static` にする。
- `tz_` prefix は公開 ABI 専用なので使わない。
- 新しい外部 C シンボルを増やす場合は `tsuzuri_` prefix にするが、このチケットでは増やさない。
- 複数の Tsuzuri object を同じホストにリンクしても、weak/hidden の `tsuzuri_task_parallel` が一つに coalesce される前提を維持する。
- driver の `--emit object` で結合済み object を作る経路を壊さない。

### 前提とする他チケットのインターフェース

- なし。
- F01 は現行の `Task.parallel` と C callback ABI を維持し、F02 以降が依存できる native runtime だけを強化する。

### 他チケットへの提供インターフェース

- F02 は `tsuzuri_task_parallel(void (*run)(void *, uint64_t), void *context, uint64_t length)` を chunk 実行にも使える。
- F02 は `length` を「要素数」ではなく「chunk 数」として渡してよい。
- F06 は同じ group/callback/atomic-index の意味を WASM threads 版に移植できる。
- B06 は `tz_task_group` に cancellation / failure propagation の状態を追加できる。
- 後続チケットは worker の存在をユーザー API に公開してはならない。

## 設計

### C ランタイムの構造

- `src/runtime/task.c` に `struct tz_task_pool` を追加する。
- `struct tz_task_pool` は次のフィールドを持つ。
- `pthread_mutex_t mutex`。
- `pthread_cond_t work_available`。
- `pthread_cond_t work_done`。
- `pthread_t threads[TZ_TASK_MAX_THREADS - 1]`。
- `unsigned thread_count`。
- `unsigned started` または `atomic_bool initialized`。
- `int stopping`。
- `struct tz_task_group *groups`。
- `struct tz_task_group *groups_tail` は任意だが FIFO 挿入に使ってよい。
- `pthread_once_t once` を使う場合は `PTHREAD_ONCE_INIT`。
- `struct tz_task_group` は次のフィールドを持つ。
- `void (*run)(void *, uint64_t)`。
- `void *context`。
- `uint64_t length`。
- `_Atomic uint64_t next`。
- `_Atomic uint64_t remaining`。
- `struct tz_task_group *next_group`。
- `unsigned active_callers` は不要。stack lifetime は active list と `remaining` で守る。
- `tz_task_parallelism` は現状の名前を維持してよいが、返す値は「呼び出し元を含む上限」とする。
- `tz_task_extra_workers()` を追加し、`parallelism - 1` を返す helper にしてもよい。
- production runtime で `pthread_once_t once` を使う場合、テストはそれを同一プロセス内で reset できると仮定しない。
- `tests/task_runtime.c::reset` のような CPU 数・作成数のリセットは、persistent worker pool では同一 process 内の後続 case に効かない。
- topology ごとの検査は子プロセスを分けるか、production singleton とは別に `struct tz_task_pool` を明示的に渡せる test-only harness を使う。
- 既定案は子プロセス分離。test-only pool object は、fork が使えない platform 対応が必要になった場合だけ追加する。

### worker loop

- 常駐 worker entry は `static void *tz_task_worker_main(void *pointer)` とする。
- worker は `pointer` で pool を受け取る。
- worker は停止フラグが立つまで次を繰り返す。
- mutex を取る。
- active list を先頭から走査し、`next < length` の group を探す。
- group があれば `atomic_fetch_add(&group->next, 1)` で index を予約する。
- 予約した index が `length` 未満なら group と index を保持して mutex を離す。
- 予約競合で範囲外なら同じ mutex 保持中に次 group を探し直す。
- group がなければ `stopping` を確認する。
- `stopping` なら mutex を離して return。
- そうでなければ `pthread_cond_wait(&work_available, &mutex)` で park する。
- mutex を離した状態で `group->run(group->context, index)` を呼ぶ。
- 完了後に `atomic_fetch_sub_explicit(&group->remaining, 1, memory_order_acq_rel)` を行う。
- 直前値が 1 なら mutex を取り `work_done` を broadcast して mutex を離す。
- worker は callback 中に active list を触らない。

### caller join loop

- `tsuzuri_task_parallel` は group を active list に登録してから join loop に入る。
- join loop は `atomic_load_explicit(&remaining, memory_order_acquire) == 0` まで続く。
- まず自分の group から index を予約して実行する。
- 自分の group に予約可能 index がない場合、`tz_task_take_any` で他 group も含めて一つ実行する。
- 実行できる work があれば callback 実行後に loop 先頭へ戻る。
- 実行できる work がなければ mutex 下で acquire load により `remaining == 0` を再確認する。
- まだ完了していなければ `pthread_cond_wait(&work_done, &mutex)` する。
- acquire load で `remaining == 0` を確認したら active list から自 group を外す。
- active list から外す処理は mutex 下で行う。
- 外した後に `work_available` を broadcast し、範囲外 fetch をしていた worker を起こせるようにする。
- caller が他 group を手伝うため、入れ子 group は枠がなくても進行する。

### active list の選択順

- 基本は FIFO でよい。
- caller は自分の group を優先する。
- worker は active list の先頭から走査する。
- 実行順は言語仕様上未規定なので、fairness は性能上の目標であり意味ではない。
- starvation を避けるため、完了 group は速やかに list から外す。
- F02 の決定的 reduction は runtime の実行順に依存してはならない。

### テスト用境界

- `tests/task_runtime.c` が差し替えられる境界を `src/runtime/task.c` に集約する。
- 直接 `sysconf` を呼ばず、`TZ_TASK_SYSCONF(name)` macro 経由にする。
- 直接 `pthread_create` を呼ばず、`TZ_TASK_PTHREAD_CREATE(thread, attr, start, arg)` 経由にする。
- 直接 `pthread_join` を呼ばず、`TZ_TASK_PTHREAD_JOIN(thread, result)` 経由にする。
- 直接 `pthread_mutex_lock` / `unlock` を呼ばず、macro 境界を用意する。
- 直接 `pthread_cond_wait` / `signal` / `broadcast` を呼ばず、macro 境界を用意する。
- 直接 `atexit` を呼ばず、`TZ_TASK_ATEXIT(function)` を用意する。
- macro の既定値は実 pthread / sysconf / atexit を呼ぶ。
- `tests/task_runtime.c` はこれらを上書きして、作成数、待機数、broadcast 数、peak active worker 数、shutdown join 数を観測する。
- テスト専用に runtime の本番 ABI を増やさない。
- persistent singleton を直接 include する `tests/task_runtime.c` は、各 CPU topology・failure scenario を `argv` で指定して子プロセスとして起動する。
- 親プロセスは各子の exit status と stderr を検査し、同一 process 内で `processors` や `pthread_once` を reset しない。
- `pthread_create` failure 子は pool 起動時に失敗して非 0 終了し、stderr の `pthread_create failed` を検査する。
- `pthread_join` failure 子は一度 pool を正常起動し、`fail_join` を立ててから test-only の明示 shutdown hook を呼ぶ。shutdown の join が失敗して非 0 終了し、stderr の `pthread_join failed` を検査する。
- join failure を通常の `tsuzuri_task_parallel(visit, ...)` だけで発火させてはならない。常駐 worker は group 完了ごとには join しないため、その形では failure path を通らない。

### LLVM / driver の変更有無

- `src/llvm.rs` の `FunctionEmitter::parallel_tasks` はこのチケットでは変更しない。
- callback ABI と context shape は維持する。
- `src/driver.rs` の `build` は引き続き `src/runtime/task.c` を C としてコンパイルする。
- ランタイムが mutex/condvar/atexit を使っても、追加の link flag は `-pthread` だけで足りる。
- WASM の `src/runtime/task-wasm.ll` はこのチケットでは逐次のまま。
- ただし docs では「native は常駐 pool、WASM は逐次 fallback」と区別して書く。

## 実装手順

1. **テスト境界を増やす。**
   - `src/runtime/task.c` の `sysconf`, `pthread_create`, `pthread_join` を macro 境界へ置き換える。
   - mutex/condvar/atexit 用の境界も追加する。
   - まだ fork/join 実装は変えない。
   - 確認: `clang -std=c11 -Wall -Wextra -Werror -pthread tests/task_runtime.c -o /tmp/tz-task-runtime && /tmp/tz-task-runtime`。

2. **pool のデータ構造を追加する。**
   - `struct tz_task_pool` と拡張 `struct tz_task_group` を導入する。
   - 既存の `tz_task_workers` は削除または pool 内の `thread_count` / `started` に置換する。
   - `tz_task_limit` は CPU 検出 cache として残してよい。
   - 確認: まだ single-thread 経路だけを通す compile-time define で `tests/task_runtime.c` が通る。

3. **lazy worker 起動を実装する。**
   - `tz_task_start_pool` を追加し、`max_extra` 本の worker を作る。
   - `pthread_create` 失敗時は既存の `tz_task_fail("pthread_create", error)`。
   - 初回だけ `atexit(tz_task_shutdown)` を登録する。
   - 確認: `tests/task_runtime.c` に `processors = 1000` のケースで `created == TZ_TASK_MAX_THREADS - 1` を追加して通す。

4. **active group list と worker loop を実装する。**
   - worker が condvar で待機し、group 追加で wake する。
   - callback 実行中に mutex を保持しないことをコード上で確認する。
   - 確認: `tests/task_runtime.c` の rendezvous が sleep なしで `arrivals == 4` を満たす。

5. **caller participation と join loop を実装する。**
   - caller は自 group を優先し、必要なら他 group を steal する。
   - acquire load で `remaining == 0` を観測するまで戻らない。
   - callback が結果配列などへ書いた後、`remaining` の acq-rel decrement を行うことをコード上で確認する。
   - 確認: `tests/task_runtime.c` の `visit` で全 index がちょうど 1 回だけ処理される。

6. **入れ子 group の deadlock 回避を検証する。**
   - `nested` と複数 host thread のケースを新実装に合わせる。
   - peak worker 数は常駐 worker 数で観測するため、旧 `outstanding` の意味を更新する。
   - 確認: `nested` が `ITEMS * outer_length` 回の hit を持ち、`peak <= max_extra`。

7. **shutdown を実装する。**
   - `tz_task_shutdown` で停止フラグ、broadcast、join を行う。
   - すでに shutdown 済みの場合は no-op。
   - active group がある正常経路で shutdown が走らないよう、`tsuzuri_task_parallel` が戻るまで group 完了を保証する。
   - 確認: `tests/task_runtime.c` で明示的に shutdown hook を呼べる test macro を使い、全 worker join を検査する。

8. **失敗診断を戻す。**
   - `pthread_create` 失敗の stderr 正規表現を `tests/tasks.mjs` と揃える。
   - `pthread_join` 失敗は test-only 明示 shutdown hook で検査する。常駐 pool では通常の group 完了時に join しないため、従来の `"join"` 引数で `tsuzuri_task_parallel` だけを呼ぶ形は使わない。
   - mutex/condvar 失敗を unit test で注入できる場合は追加する。
   - 確認: `tests/tasks.mjs` の create failure 子プロセスと explicit-shutdown join failure 子プロセスが非 0 終了し、対応する診断を出す。

9. **複数 object の単一 pool を証明する。**
   - 既存の「リンクできる」検査だけでは不十分。
   - `--emit object` で作った二つの Tsuzuri object を同じ host へリンクし、link map（例: `clang -Wl,-map,<file>` または platform 相当）か `nm` / `objdump` の symbol resolution を検査して、最終実行ファイルで `tsuzuri_task_parallel` の解決先が一つだけであることを確認する。
   - さらに二つの object の exported 関数を別 host pthread から同時に呼び、それぞれが `Task.parallel` を実行する fixture を追加する。
   - この concurrent cross-object test は runtime instrumentation で「合計 worker 数」が `min(online CPUs, 32) - 1` を超えないことを確認する。object ごとに別 pool があるなら合計が bound を超えるため失敗する。
   - instrumentation は本番 ABI を増やさず、test build の macro / direct-include harness で worker create/wait counters を集約する。
   - 確認: link-map assertion と concurrent cross-object counter assertion の両方が通る。

10. **docs を更新する。**
   - `docs/language.md` のタスク backend 節から「常駐ワーカープールはまだありません」を削除し、新 semantics を書く。
   - `docs/architecture.md` のタスク節を pool 方式に更新する。
   - `README.md` のタスク説明を更新する。
   - `docs/benchmarks.md` に新しい task overhead ベンチの条件を追加する。
   - 確認: docs に native/WASM の actual support と planned support が分かれている。

11. **ベンチマークを追加する。**
    - `benchmarks/run-tasks.mjs` と `benchmarks/tasks/Main.tz` を追加する。
    - matched workload は同じ仕事量、同じ結果、同じ allocation 条件で旧コンパイラ baseline と新コンパイラを比較する。
    - 共有 CI では `--quick` 正しさだけにする。
    - 確認: `node benchmarks/run-tasks.mjs target/release/tsuzuri --quick` が結果一致だけを検査する。

## テスト計画

- Rust テストは既存の `tests/tasks.rs` を維持し、IR の宣言数と WASM 逐次 backend を引き続き検査する。
- C ランタイム単体テストは `tests/task_runtime.c` を拡張する。
- `tests/task_runtime.c` は `sysconf` の差し替えを維持する。
- `tests/task_runtime.c` は `pthread_create` / `pthread_join` の差し替えを維持する。
- `tests/task_runtime.c` は mutex/condvar/atexit の差し替えを追加する。
- `tests/task_runtime.c` は `length == 0` で pool を起動しないことを検査する。
- `tests/task_runtime.c` は `length == 1` で pool を起動しないことを検査する。
- `tests/task_runtime.c` は `processors <= 1` で追加 worker が 0 本であることを検査する。
- `tests/task_runtime.c` は `processors = 4` で常駐 worker が 3 本を超えないことを検査する。
- `tests/task_runtime.c` は `processors = 1000` で常駐 worker が 31 本を超えないことを検査する。
- これらの CPU topology case はそれぞれ別 child process で実行し、persistent worker と `pthread_once` を同一 process で reset しない。
- `tests/task_runtime.c` は rendezvous により、実際に caller + worker が同時に進むことを検査する。
- `tests/task_runtime.c` は各 index がちょうど 1 回実行されることを atomic hit array で検査する。
- `tests/task_runtime.c` は nested group で全仕事が完了し、deadlock しないことを検査する。
- `tests/task_runtime.c` は複数 host pthread から同時に呼んでも global bound を超えないことを検査する。
- `tests/task_runtime.c` は group 完了後に active list が空になることを検査する。
- `tests/task_runtime.c` は shutdown 後に全 worker が join されることを検査する。
- `tests/task_runtime.c` は `pthread_join` 失敗を explicit shutdown hook 経由で検査する。
- `tests/task_runtime.c` は sleep / timeout / wall-clock を合否条件にしない。
- `tests/tasks.mjs` は native/WASM `-O0` / `-O3` を維持する。
- `tests/tasks.mjs` は native の trap ケースを子プロセスで確認する。
- `tests/tasks.mjs` は `WebAssembly.Module.imports(module)` が空であることを維持する。
- `tests/tasks.mjs` は weak/hidden runtime を複数 object でリンクする検査を維持する。
- `tests/tasks.mjs` は複数 object の link map / symbol resolution を検査し、`tsuzuri_task_parallel` が単一解決であることを確認する。
- `tests/tasks.mjs` は複数 object の exported 関数を同時実行し、runtime instrumentation の合計 worker count が global bound 内であることを確認する。
- `tests/tasks.mjs` は native の heap tracking で各公開関数呼び出し後 `live == 0` を確認する。
- ASan が使える環境では `TSUZURI_ASAN=1 node tests/tasks.mjs target/release/tsuzuri` を任意で実行する。
- 速度は合否条件にしない。

## ドキュメント

- `docs/language.md` の「タスク」節に、native は lazy persistent worker pool、WASM は import なし逐次 fallback と書く。
- `docs/language.md` には `Task.parallel` が戻る前に全仕事が完了することを明記する。
- `docs/language.md` には常駐 worker はユーザーから見える detach ではないことを明記する。
- `docs/architecture.md` の「タスク」節を、呼び出しごとの fork/join から pool + group + caller participation に更新する。
- `docs/architecture.md` の「性能設計の原則」は変更せず、pool が小さい仕事で常に速いとは書かない。
- `README.md` の「タスクと並列処理」を更新し、WASM は引き続き逐次と書く。
- `docs/benchmarks.md` に `benchmarks/run-tasks.mjs` の条件、制限、出力 JSON の意味を追加する。
- `docs/benchmarks.md` では speed threshold を共有 CI に入れないことを明記する。

## 受け入れ条件

- [ ] `src/runtime/task.c` は lazy persistent worker pool を持つ。
- [ ] `tsuzuri_task_parallel` の C ABI は変わっていない。
- [ ] `tsuzuri_task_parallel` は weak/hidden のまま。
- [ ] 追加 C helper は `static` で、公開 ABI を増やしていない。
- [ ] global 追加 worker 数は `min(online CPUs, 32) - 1` を超えない。
- [ ] CPU 数が取れない場合は追加 worker なしで逐次実行する。
- [ ] `length == 0` と `length == 1` は pool 起動なしで動く。
- [ ] caller が仕事を実行する。
- [ ] `remaining` は completion publish に acq-rel RMW を使い、caller は戻る前に acquire load で 0 を確認する。
- [ ] 入れ子 group が枠枯渇で deadlock しない。
- [ ] callback 実行中に global mutex を保持しない。
- [ ] idle worker は condvar で park する。
- [ ] group 追加と完了時に wakeup が行われる。
- [ ] `pthread_create` 失敗は既存メッセージ形式で abort する。
- [ ] `pthread_join` 失敗も明示診断で abort する。
- [ ] `tests/task_runtime.c` は timing に依存しない同時実行検査を持つ。
- [ ] CPU topology / failure tests は persistent pool singleton を同一 process で reset しようとしない。
- [ ] 複数 Tsuzuri object を同時リンクしても単一 pool instance であることを link map と concurrent counter test で証明する。
- [ ] native/WASM の `tests/tasks.mjs` が `-O0` / `-O3` で通る。
- [ ] WASM の `task-wasm.ll` はこのチケットで import を増やしていない。
- [ ] docs は actual support と planned support を分けている。
- [ ] task overhead ベンチは matched workload で、CI に速度閾値を入れていない。

## 落とし穴

- 常駐 worker を作っても、`Task.parallel` の完了前に stack 上の group を破棄してはならない。
- callback 実行中に mutex を持つと、入れ子 `Task.parallel` で即 deadlock する。
- worker が自分の group だけを待つと、全 worker が join 待ちになったとき進行しない。
- group 完了通知を signal し忘れると caller が眠ったままになる。
- condvar は spurious wakeup するので、if ではなく while で条件を再確認する。
- `remaining` を減らす前に完了通知すると、caller が active list から外して stack lifetime を壊す。
- `remaining` を relaxed にすると、`FunctionEmitter::parallel_tasks` の callback が結果配列へ store した内容を caller が join 後に見る保証がない。
- `pthread_create` 成功後に `thread_count` を増やす順序を間違えると shutdown が未初期化 thread を join する。
- `atexit` shutdown と通常実行が同時に走る可能性を、process 終了時の順序として過信しない。
- `tz_task_limit` の cache を test reset できないと `tests/task_runtime.c` の CPU 数ケースが独立しない。
- `pthread_once` と persistent worker は reset 不能なので、CPU 数や failure injection を同一 process 内で切り替える旧テスト形は無効。
- 複数 object の単なる link 成功は単一 pool を証明しない。
- `pthread_cond_wait` の wrapper を test で差し替えるとき、mutex ownership を壊さない。
- F02 の deterministic reduction を runtime の worker 選択順に依存させてはならない。
- 速度改善を主張する前に、起動費、allocation、join、解放を含む matched workload を測る。

## 対象外

- `Task.parallel` の型や構文の変更。
- detach / cancellation / failure propagation。
- ワーカー数をユーザーが指定する CLI または言語 API。
- Windows native 対応。
- WASM threads 対応。
- F02 の `Parallel.*` API。
- GPU / SIMD backend。
- タスク trap 時の巻き戻しや兄弟タスクの停止保証。

## 未決事項

- pool shutdown を本番で必ず `atexit` 登録するか、テスト専用 hook に限定するか。
  - 既定案: ASan/TSan と leak 検査のため、本番でも `atexit` shutdown を登録する。
- active list の公平性を FIFO にするか LIFO にするか。
  - 既定案: worker は FIFO scan、caller は自 group 優先。
- worker 数を初回に上限まで作るか、需要に応じて段階的に増やすか。
  - 既定案: 実装を単純にし、既存上限と同じ `max_extra` 本を初回に作る。
- `pthread_mutex_*` / `pthread_cond_*` 失敗の注入テストをどこまで行うか。
  - 既定案: create/join は必須、mutex/condvar は wrapper と診断経路を単体で通す。
- 台帳の見直し提案: なし。D-14, D-17, D-18 と整合する。
