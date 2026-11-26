# W41: Windows のタイマーのスレッドの待ちがミリ秒に切り上がり、短い `Time.sleep` が大きく延びる

| 項目 | 内容 |
| --- | --- |
| ID | W41 |
| 優先度 | P2（Windows では TZ05 のすべての期限（`Time.sleep`・`Time.after`・`Time.tick`・`Timer`・`Ticker`・`Time.after_func`、TZ07 の `Context` の期限、TZ08 のネットワークの待ちの期限）が OS のタイマーの刻みの単位に延びる。10 ms 以下の `Time.sleep` は 1 回約 15.8 ms、20 ms は約 31.7 ms、1 ms の `Ticker` は約 15.8 ms ごとにしか送らない） |
| 規模 | M（Windows の OS の層の補助 1 つと `tzroutine.c` の 3 か所、C のランタイムのテスト 1 件、計測、文書 4 ページ） |
| 依存 | W34 |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-22（D-63。W01〜W37 の起票中に見つけ、利用者の依頼「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」に当たるので起票。HEAD `fa40eede` で確認） |
| 承認 | 不要（意味を変えない。「少なくとも `d` 待つ」と期限の順は保ち、Windows の待ちが OS の高分解能タイマーの細かさに近づくだけ。TZ05 はタイマーの分解能を保証しない（TZ05 の対象外）。POSIX と WebAssembly（JSPI）のランタイム、生成 IR は変えない。台帳は [GUIDE D-63](GUIDE.md#d-63-tsuzuri-だけで-win32-のアプリを書くe12-d13-の狭い改訂w01w30)） |
| 手本にする既存実装 | [W34](W34-windows-sysmon-timer-resolution.md) の `tzr_sysmon_wait`（高分解能の待機タイマー、失敗したら条件変数のミリ秒の待ちに戻る、`tzr_sysmon_hires`）と `benchmarks/run-sysmon.mjs`、`tests/tzroutine_runtime.c` の `sleep`・`timer_stop`・`long_sleep` |
| 主な影響ファイル | `src/runtime/tzroutine-windows.h`、`src/runtime/tzroutine.c`（`tzr_heap_push` の起こしと `tzr_timerproc` の 2 つの待ち）、`tests/tzroutine_runtime.c`、`tests/tzroutine_runtime.mjs`、`benchmarks/sysmon/timers.c`（新規）、`benchmarks/run-sysmon.mjs`、`docs/benchmarks.md`、`docs/architecture.md`、`docs/language.md`、`_tsuzuri/language-reference/built-in-types-and-modules/time.md` |

## 実装者向け早見

- 範囲: Windows のタイマーのスレッド（`tzr_timerproc`）の 2 つの待ち。期限つきの待ちは W34 の高分解能の待機タイマーと自動リセットのイベントを `WaitForMultipleObjects` で、期限なしの待ちはイベントで待つ。`tzr_heap_push` の起こし（`kicked`）は、今の条件変数の signal に加えてイベントを立てる。どちらかを作れなければ今の経路のまま。
- W34 の補助は sysmon だけのもので、早く起こす手段を持たない（W34 D6）。そのまま呼ぶと、新しい最も早いタイマーが来ても今の期限まで起きない。このチケットは W34 の補助を、起こせる待ちにも使える `tzr_hires` にまとめ直す。
- 最初に読む: 「現状」の 1〜3 → `tzroutine.c` の `tzr_heap_push`・`tzr_timers_run`・`tzr_timerproc` → W34 の設計 1。
- 最初に書くテスト: `tests/tzroutine_runtime.c` の `timer_hires`。
- 触ってはいけない: `tzr_timers_run`（期限の判定 `when <= now`）、`tzr_deadline`、`kicked` の意味、sysmon の期限なしの眠り（`parked`）、POSIX と JSPI の本文。
- 完了の見分け方: Windows で「現状」3 の試作と同じく、50 回の 1 ms の `tsuzuri_tz_sleep` の平均が OS の高分解能タイマーの細かさ（起票時の開発機で約 1.5 ms）になり、早く起きた回が 0。`timer_hires` が `hires 1` を出す。

## 目的

TZ05 のタイマーは 1 つのヒープと 1 本のタイマーのスレッドで動き、スレッドは最も早い期限まで眠る。Windows ではこの眠りが `tzr_cond_wait_for` でミリ秒に切り上がり、さらに OS のタイマーの刻み（既定で約 15.6 ms）に丸められる。そのため、どの期限も刻みの単位に延び、`Ticker` の短い周期は刻みより細かくならない。W34 は sysmon の周期の待ちを高分解能の待機タイマーにしたが、タイマーのスレッドは対象外とした（W34 D7）。理由は、このスレッドが「新しい最も早いタイマー」で起こされる必要があり、起こし方も変える必要があるからである。このチケットは W34 の補助に起こす手段を足し、タイマーのスレッドの待ちを Windows でも OS の高分解能タイマーの細かさにする。

## 着手条件と停止条件

### 着手条件

- `node scripts/repo-kb.mjs brief W41`。[W34](W34-windows-sysmon-timer-resolution.md) が `main` に入っていること（`grep -n "CreateWaitableTimerExW" src/runtime/tzroutine-windows.h`）。入っていなければ始めない。
- GUIDE §14 の 1・2: 着手直前の `src/runtime/` を `target/perf/W41/runtime-before/` に写し、環境を記録する。
- Windows の開発機（x64。MSVC の Developer PowerShell と LLVM の clang、同梱の MinGW）。Arm64 は `windows.yml` に頼る。

### 停止条件

- **S1** before の計測（手順 1）で、10 ms 以下の `tsuzuri_tz_sleep` の平均が刻みより細かい（その機械では前提が違う）。測った値を記録して報告する。
- **S2** 早く起きた `sleep` が 1 回でもある、またはタイマーが期限の順に発火しない。
- **S3** 既存のランタイムのテスト（`sleep`・`long_sleep`・`timer_*`・`ticker*`・`after_func`・`boundary_timers`・`context_deadline*`・`context_after`・`netpoll_timeout`・`detector`）、`tests/tzroutine.mjs`、W34 の `blocking_short_calls` の結果が変わる。
- **S4** 生成 IR、または POSIX・JSPI の `tzroutine.c` の前処理後の本文（空行を除く）が変わる。
- **S5** タイマーを使うプログラムのプロセスの CPU 時間が、before と after の中央値の差として揺れの範囲を超えて増える。閾値で合否を決めず、値を記録して人間に判断を求める。
- **S6** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

1. **コード**: `src/runtime/tzroutine.c` の `tzr_timerproc` は `tzr_timers_run()`（期限の来たタイマーを `when <= now` で発火し、次の期限を返す）の後、`tzr_timers.wake_mutex` の下で `kicked` が 0 なら、次の期限がなければ `tzr_cond_wait_for(&tzr_timers.wake, …, -1)`、あれば時計を読み直した残り `left` で `tzr_cond_wait_for(…, left)` を呼ぶ。`tzr_heap_push` は、足したタイマーがヒープの先頭なら、`wake_mutex` の下で `kicked = 1` にして `tzr_cond_signal` する。Windows の `tzr_cond_wait_for`（`tzroutine-windows.h`）は `ns` をミリ秒に切り上げて `SleepConditionVariableSRW` に渡す（`ns < 0` は 1 日）。ヒープには `WAKE`（`Time.sleep`）・`SEND`・`TICK`・`FUNC`・`CTX`（TZ07）・`NET`（TZ08）のすべてが入るので、どれも同じ待ちで遅れる。タイマーのスレッドは最初のタイマーで始まるので、タイマーを使わないプログラムには関係しない。
2. **Tsuzuri のプログラムでの測定**（開発版、MSVC の clang 22.1.8 と同梱の MinGW、各 2 回。Windows では `Time.monotonic_ns` が `E2002` なので、時計は `Time.after 0` の受け取った値で読んだ。50 回の平均）:

| 待ち | MSVC | MinGW |
| --- | --- | --- |
| `Time.sleep` 100 µs・1・2・5・10 ms | 15.4〜15.9 ms | 15.5〜15.9 ms |
| `Time.sleep` 20 ms | 31.6〜31.7 ms | 31.7〜31.8 ms |
| `Time.after` 1・5 ms を受け取るまで | 15.8〜15.9 ms | 15.8〜15.9 ms |
| `Time.after` 20 ms | 31.7 ms | 31.7〜31.8 ms |
| `Ticker` 1・5 ms の 1 回の間隔 | 15.8〜16.0 ms | 15.8〜15.9 ms |
| `Ticker` 20 ms の 1 回の間隔 | 19.9 ms（期限を `when += d` で進めるので平均は保たれるが、1 回ごとは刻みの倍数） | 20.0 ms |

   - `for _i in 1 ..= 50 do Time.sleep (Time.millisecond())` をプロセスの外から測ると 1.6〜2.2 秒だったが、`Time.sleep 0` の同じプログラムが 0.7〜1.4 秒かかった（起動の時間）。差の約 0.8〜0.9 秒が上の 50 × 約 15.8 ms に当たる。
3. **試作**（`tests/tzroutine_runtime.c` と同じく `tzroutine.c` を直接 include した C。`target/scratch` に写したランタイムに「設計」の待ちを入れ、MSVC ABI の clang で `-O2`、2 回ずつ。主スレッド（擬似 G）と tzroutine の中でそれぞれ 50 回）:

| `tsuzuri_tz_sleep` | 今の HEAD の平均 | 試作の平均（最小） |
| --- | --- | --- |
| 100 µs | 15.0〜15.8 ms | 0.53〜0.56 ms |
| 1 ms | 15.9 ms | 1.54〜1.55 ms（1.18 ms） |
| 5 ms | 15.8〜15.9 ms | 5.21〜5.26 ms |
| 20 ms | 31.7 ms | 20.3〜20.4 ms |

   - 早く起きた回はどちらも 0。10 秒のタイマーを待っている間に別の G が 1 ms 眠ると、今は約 16 ms、試作は約 1.2 ms で起きた（イベントの起こしが効く）。同梱の MinGW（`-Wall -Wextra -Werror`）でも同じ傾向（1 ms は平均 1.55 ms）。
   - 値は W34 の「現状」4 の高分解能の待機タイマーの測定（1 ms の要求で中央値 1.54 ms）と合う。
4. **W34**: HEAD にはまだない（`CreateWaitableTimerExW` は `src/` に 0 件）。W34 の補助は sysmon のスレッドだけが触る 1 つのハンドルで、早く起こす手段を足さない（W34 D6）。待ちが失敗したら、ロックを取り直して、その回も条件変数で待つ。
5. **文書**: `time.md` は `Time.sleep d` を「少なくとも `d` ナノ秒待ちます」と書き、Windows の節は時計が `QueryPerformanceCounter` であることだけを書く。待ちの細かさは書いていない。TZ05 の対象外に「タイマーの分解能の保証」がある。

## 変えてはいけない意味

- 期限は `tzr_deadline(now, d)`、発火は `tzr_timers_run` の `when <= now` だけで決まる（早く起きない）。待ちが QPC の期限より少し早く終わっても、何も発火せずにもう一度待つ。
- `kicked` の意味（ロックの下で立て、スレッドが待つ前に見る。起こしを失わない）、ヒープとロックの順序、境界でのタイマーの停止（TZ05 D4）、デッドロックの判定（`ntimers`）。
- POSIX と JSPI のランタイム、sysmon の期限なしの眠り。

## 仕様

1. Windows のタイマーのスレッドは、期限つきの待ちを高分解能の待機タイマーとイベントのどちらかが来るまで、期限なしの待ちをイベントが来るまで待つ。期限は残りを 100 ns 単位に切り上げた相対時刻。待つ間は `wake_mutex` を外し、戻る前に取り直す。
2. `tzr_heap_push` の起こしは、`kicked = 1` と条件変数の signal に加えて、Windows ではイベントを立てる。
3. タイマーかイベントを作れなければ、今の条件変数の待ちのまま。ロックを外した後に待ちが失敗したら、その回は待たずに戻り（呼び出し側が `kicked` と期限を見直す）、以後は条件変数で待つ。
4. 早く戻ること（前の回に立ったイベント、期限の少し前の満了）は許す。`tzr_timers_run` が何も発火しないだけである。
5. POSIX と JSPI の `tzroutine.c` の前処理後の本文（空行を除く）と生成 IR は変わらない。

## 設計

### 1. Windows の層（`src/runtime/tzroutine-windows.h`。W34 の補助をまとめ直す）

```c
/* A high-resolution waitable timer (W34) and, for a wait that others end early, an auto-reset event
 * (W41). Only the waiting thread touches `timer`; `wake` is made before the thread starts and lives
 * as long as the process, so any thread may set it. */
typedef struct {
    HANDLE timer;      /* NULL: the condition variable's millisecond wait */
    HANDLE wake;       /* NULL: nobody ends the wait early (sysmon) */
    _Atomic int hires; /* 1 while `timer` is in use: tests and benchmarks read it */
} tzr_hires;
static void tzr_hires_init(tzr_hires *h, int wakeable);   /* the event too when wakeable; none of them on failure */
/* Waits at most ns (>= 0) nanoseconds, or with ns < 0 until woken (only with `wake`), and returns with
 * `m` held. After a failure with `m` released it returns at once: the caller looks again. */
static void tzr_hires_wait(tzr_hires *h, tzr_cond *c, tzr_mutex *m, int64_t ns);
static void tzr_hires_wake(tzr_hires *h);                 /* SetEvent when there is an event */
```

- `tzr_hires_wait` の期限つきの待ちは `SetWaitableTimerEx` の後で `WaitForMultipleObjects`（タイマーとイベント、`FALSE`、`INFINITE`）、期限なしの待ちは `WaitForSingleObject(wake, INFINITE)`。`timer` が NULL か、`ns < 0` で `wake` が NULL なら `tzr_cond_wait_for`。
- W34 の `tzr_sysmon_wait_init`・`tzr_sysmon_wait`・`tzr_sysmon_hires` は名前と意味を保ち、`tzr_hires_init(&…, 0)`・`tzr_hires_wait` で書き直す（W34 のテストと計測は変えない）。違いは、ロックを外した後の失敗でその回を待たずに戻ることだけで、sysmon では巡回が 1 回増えるだけである。

### 2. タイマーのスレッド（`src/runtime/tzroutine.c`）

- `static tzr_hires tzr_timer_wait;`（Windows だけ）。`tzr_heap_push` の `!tzr_timers.started` の枝で、`tzr_thread_start` の前に `tzr_hires_init(&tzr_timer_wait, 1)` を呼ぶ（`tzr_timers.lock` の下で、`started` より先に書くので、後の起こし手は必ず作り終えたハンドルを見る）。
- 起こしの枝（`tzr_cond_signal(&tzr_timers.wake);` の後）と `tzr_timerproc` の 2 つの待ちを、W34 と同じく POSIX の本文を変えない形にする:

```c
#if defined(_WIN32)
                tzr_hires_wait(&tzr_timer_wait, &tzr_timers.wake, &tzr_timers.wake_mutex, left);
#else
                tzr_cond_wait_for(&tzr_timers.wake, &tzr_timers.wake_mutex, left);
#endif
```

- 起こしは条件変数の signal とイベントの両方にする。タイマーのスレッドが条件変数の経路に戻った後も、起こしを失わない。イベントを立てるのは今 signal している所（足したタイマーが先頭のとき）だけで、回数は増えない。

### 3. 計測（GUIDE §14）

- `benchmarks/sysmon/timers.c`（W34 の `handoff.c` と同じく `tzroutine.c` を include）: 主スレッドと tzroutine で、100 µs・1・2・5・10・20 ms の `tsuzuri_tz_sleep` を 50 回ずつ。平均・中央値・最小・最大・早く起きた回、`Time.after` に当たる 1 回の `SEND` のタイマー（`tsuzuri_tz_timer_new` の周期 0）の 1・5・20 ms の受け取りまでの時間、`Ticker` の 1・5・20 ms の間隔の分布、`tzr_timer_wait.hires`。
- `benchmarks/run-sysmon.mjs` に workload `timers` を足す（同じ `--runtime`・`--baseline-runtime`、PX01 の JSON Lines、9 回以上、順序を交互）。CPU 時間（`GetProcessTimes`）は、1 ms の `Ticker` を 2 秒受け取る workload で前後を比べる。
- Tsuzuri の版（任意）: 「現状」2 のプログラム（時計は `Time.after 0`）を before と after のコンパイラで。

## 前提とする他チケットのインターフェース

- [W34](W34-windows-sysmon-timer-resolution.md): `tzroutine-windows.h` の `#ifndef CREATE_WAITABLE_TIMER_HIGH_RESOLUTION`、`tzr_sysmon_wait_init`・`tzr_sysmon_wait`・`tzr_sysmon_hires`、`benchmarks/run-sysmon.mjs`。

## 他チケットへ提供するインターフェース

- ランタイムの内部（ABI ではない）: Windows の `tzr_hires`・`tzr_hires_init`・`tzr_hires_wait`・`tzr_hires_wake`、`tzr_timer_wait`。
- [W29](W29-callback-latency.md) の D2 の `tzr_sysmon_nudge` が W34 の (b)（イベントと `WaitForMultipleObjects`）を選ぶなら、sysmon の `tzr_hires` を `wakeable` で作り、`tzr_hires_wake` で起こせる。

## 実装手順

### 手順 1: before と計測の仕組み

- 変更: `benchmarks/sysmon/timers.c`（新規）、`benchmarks/run-sysmon.mjs`。
- 内容: 着手条件の GUIDE §14 の 1・2。設計 3。before を Windows で測る。
- 確認: `node benchmarks/run-sysmon.mjs --quick`（`timers` を含む）が、10 ms 以下の sleep の平均を刻みの近く（約 15.6 ms）で出す（S1）。

### 手順 2: 待ちの変更

- 変更: `src/runtime/tzroutine-windows.h`、`src/runtime/tzroutine.c`。
- 内容: 設計 1・2。
- 確認: Windows で `cargo check --all-targets`、`sh scripts/check-runtime-includes.sh`。POSIX と JSPI の前処理後の本文の比較（W34 の受け入れ条件 3 と同じコマンド）。

### 手順 3: テストと after の計測

- 変更: `tests/tzroutine_runtime.c`（`timer_hires`）、`tests/tzroutine_runtime.mjs`（`cases`）。
- 内容: テスト計画 1。GUIDE §14 の 3〜5（`--baseline-runtime target/perf/W41/runtime-before --samples 9 --metrics target/perf/W41-after`）。`tests/fixtures/tzroutine`・`examples/tzroutines` の `--emit llvm` が前後で同じ。
- 確認: Windows（MSVC の clang と同梱の MinGW の `-Wall -Wextra -Werror`）と POSIX（`TSUZURI_TSAN=1 TSUZURI_ASAN=1`）で `node tests/tzroutine_runtime.mjs`。`cargo build && node tests/tzroutine.mjs target/debug/tsuzuri`。

### 手順 4: 記録と文書

- 変更: 「ドキュメント」のページ。
- 内容: GUIDE §14 の 7。
- 確認: `node scripts/check-docs.mjs _tsuzuri/language-reference/built-in-types-and-modules/time.md`。T2。PR に `ci:full` を付け、`windows.yml`（x64・Arm64）が通る。

## テスト計画

1. **`timer_hires`**（`tests/tzroutine_runtime.c`、すべてのホスト、`-O0`・`-O3`）: (a) 主スレッドと tzroutine で 1 ms の `tsuzuri_tz_sleep` を 50 回ずつ。どの回も `tzr_nanotime` で測って 1 ms 以上（下限だけ。速度の閾値は入れない。AGENTS.md）。平均と最小を 1 行で出す。(b) 10 秒の `SEND` のタイマー（`timer_block` の記録）を足して少し眠った後、別の G が 1 ms 眠る。その G が、10 秒のタイマーの期限より前に起きる（起こしを失えば 10 秒後になる）。確かめた後で 10 秒のタイマーを止めて解放する（境界を待たせない）。Windows で `tzr_timer_wait.hires` が 0 なら、(a)・(b) の検査はして `fallback` と出す（読む所は `#if defined(_WIN32)` で囲み、POSIX も `-Werror` で通す）。
2. **既存**: 停止条件 S3 の case、`tests/tzroutine.mjs`（Windows の段を含む）、`tests/net.mjs` の段 `tzroutine`（`vscode.yml` の Windows）、W34 の `blocking_short_calls`。
3. **本文の同一性**: 受け入れ条件 3。
4. **性能**: 設計 3（手元だけ。CI には入れない）。

## ドキュメント

- `docs/benchmarks.md`: 節「Windows のタイマーのスレッドの待ち（W41）」。方法・環境・コミット・before と after の中央値と最小〜最大・CPU 時間・生データの場所。「現状」2・3 の観察も測った日と機械とともに。測っていないもの（Arm64、仮想機械、1803 より前の Windows）。
- `docs/architecture.md` の TZ05 のタイマーの段落: Windows のタイマーのスレッドは高分解能の待機タイマーとイベントで待ち、作れなければ条件変数のミリ秒の待ちに戻ること。
- `docs/language.md` の TZ05 の段落と `time.md` の Windows の節: 期限の細かさは OS で決まり、Windows では Windows 10 1803 以降で OS の高分解能タイマーの細かさ（数は測った日と機械とともに）、それより前は既定のタイマーの刻みになること。「少なくとも `d`」は変わらないこと。

## 受け入れ条件

- [ ] 「仕様」1〜5 のとおりに動き、`timer_hires` が Windows（MSVC の CI の x64・Arm64、手元の同梱の MinGW）と POSIX（TSan・ASan を含む）の `-O0`・`-O3` で通る。早く起きた回がない。
- [ ] 既存のランタイムのテスト・`tests/tzroutine.mjs`・W34 の `blocking_short_calls` の結果と、生成 IR が変わらない。
- [ ] POSIX と JSPI の `tzroutine.c` の前処理後の本文が、空行を除いて前後で同じ（W34 の受け入れ条件 3 のコマンド）。
- [ ] `docs/benchmarks.md` に GUIDE §14 の記録があり、測っていない効果を主張していない。CI に速度の閾値を足していない。
- [ ] 文書を直し、`check-docs` が通る。T2 が通る。PR に `ci:full` を付け、`windows.yml` が通る。

## 落とし穴

- **W34 の `tzr_sysmon_wait` をそのまま呼ばない**: 早く起こす手段がないので、10 秒のタイマーを待っている間に来た 1 ms のタイマーが 10 秒待つ。
- イベントは自動リセットにする。手動リセットだと、1 回の起こしの後の待ちがすべて即座に終わり、スレッドが CPU を回し続ける。前の回に立って残ったイベントは、次の待ちを 1 回早く終わらせるだけで害はない。
- 待つ間に SRW のロックを持たない（起こし手が `wake_mutex` を取れず止まる）。ロックを外した後に待ちが失敗したら、条件変数の待ちに落ちずに戻る。外していた間の起こしは条件変数に届いておらず、そのまま待つと失う。
- 待機タイマーの相対時刻は QPC と別の時計で数えるので、QPC の期限より少し早く満了することがある。期限の判定は `tzr_timers_run` に任せ、待ちの側で「早く起きない」を作ろうとしない。
- `CREATE_WAITABLE_TIMER_MANUAL_RESET` を付けない、期限は負の値（相対）にする、`timeBeginPeriod` を使わない（W34 の落とし穴と D3 と同じ）。
- テストの待ちを `Sleep` で作らない（刻みに丸まる）。時間の上限で合否を決めない。

## 対象外

- [W11](_completed/W11-windows-os-api.md) が入れる Windows の `Time.sleep_ms` の、tzroutine の外の待ち（`Sleep` の繰り返し。刻みに丸まる）。tzroutine の中ではフック（TZ05 D6）でこのチケットの待ちに乗る。
- sysmon の待ち（[W34](W34-windows-sysmon-timer-resolution.md)）とコールバックの遅延（[W29](W29-callback-latency.md)）。
- 1803 より前の Windows での速さ（今の経路のまま）。
- TZ05 の対象外（分解能の保証）を改めること。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | Windows のタイマーのスレッドの期限つきの待ちを、W34 の高分解能の待機タイマーで待つ | 試作で、10 ms 以下の sleep の平均が約 15.8 ms から要求＋約 0.5 ms に、20 ms が 31.7 ms から 20.3 ms になり、早く起きた回はなかった（「現状」3）。`timeBeginPeriod` のようにタイマーの刻みを設定する API を呼ばない（高分解能タイマーが待つ間にシステムのタイマーの割り込みの頻度を上げるかは未確認。CPU 時間は S5 と設計 3 で測る） | 既定案（実装者はこの案に従う） |
| D2 | 早く起こす手段は自動リセットのイベントと `WaitForMultipleObjects`（W34 の「他チケットへ提供するインターフェース」の (b)） | 起こしの遅れがない（試作で 10 秒の待ちの最中の 1 ms のタイマーが約 1.2 ms で起きた）。退けた案: タイマーを相対 −1 で設定し直す (a)（高分解能タイマーの細かさ約 0.5 ms だけ遅れ、起こし手の設定とスレッドの設定が競合しないよう設定をロックの下で行う必要がある） | 既定案（実装者はこの案に従う） |
| D3 | W34 の補助を `tzr_hires` にまとめ、sysmon の名前と意味を保つ | 待機タイマーの扱い（`#ifndef`、期限の計算、失敗時の戻り）を 1 か所にする。W34 のテストと計測を変えない | 既定案（実装者はこの案に従う） |
| D4 | 期限なしの待ちもイベントで待ち、起こしは条件変数の signal とイベントの両方にする | スレッドが条件変数の経路に戻った後も起こしを失わない。イベントを立てるのは今 signal している所だけ | 既定案（実装者はこの案に従う） |
| D5 | ハンドルはタイマーのスレッドを始める前に、`tzr_timers.lock` の下で作る | 起こし手が読むイベントのハンドルを、同期なしに安全に公開できる（スレッドの開始と `started` の書き込みの前に書く） | 既定案（実装者はこの案に従う） |
| D6 | ロックを外した後の待ちの失敗は、その回を待たずに戻り、以後は条件変数で待つ。報告しない | 外していた間の起こしを失わない。遅くなるだけで意味は変わらない（W34 D2 と同じ） | 既定案（実装者はこの案に従う） |
| D7 | `timeBeginPeriod` と spin を使わない | W34 の D3・D4 と同じ理由 | 既定案（実装者はこの案に従う） |
