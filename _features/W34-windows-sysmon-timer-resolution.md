# W34: Windows の sysmon の待ちがミリ秒に切り上がり、TZ08 の 20 µs の刻みが成り立たない

| 項目 | 内容 |
| --- | --- |
| ID | W34 |
| 優先度 | P2（Windows で tzroutine を起動するプログラムでは、OS のタイマーの刻み（既定で約 15.6 ms）より短いブロッキング呼び出しの P が、sysmon に取り上げられない。P が埋まっていると、ほかの tzroutine はその呼び出しの間ずっと走れない） |
| 規模 | M（ランタイムの C の 2 か所と Windows の OS の層、C のランタイムのテスト 1 件、計測の仕組み、文書 4 ページ） |
| 依存 | – |
| 後続 | [W41](W41-windows-timer-thread-resolution.md)（このチケットの高分解能の待機タイマーの補助を、起こせる待ちにまとめ直して TZ05 のタイマーのスレッドに使う） |
| 状態 | todo |
| 起票 | 2026-11-22（D-63。W01〜W30 の起票中に見つけ、利用者の依頼「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」に当たるので起票。HEAD `fa40eede` で確認） |
| 承認 | 不要（意味を変えない。sysmon の巡回の間隔だけが Windows で細かくなる。TZ08 D1・TZ10 D3 の定数、P の取り上げの条件、デッドロックの判定、POSIX と WebAssembly のランタイムは変えない） |
| 手本にする既存実装 | `src/runtime/tzroutine-windows.h`（Windows の OS の呼び出しを `tzr_` の小さな関数にまとめた層）と `tzroutine.c` の POSIX の `tzr_cond_wait_for`、`tests/tzroutine_runtime.c` の `blocking_handoff`・`blocking_reacquire`（`tzroutine.c` を直接 include して P の引き継ぎを確かめる）、`benchmarks/run-tasks.mjs` の `--runtime`・`--baseline-runtime`（C のランタイムだけを差し替えて比べる） |
| 主な影響ファイル | `src/runtime/tzroutine-windows.h`、`src/runtime/tzroutine.c`（`tzr_sysmon` の 2 か所）、`tests/tzroutine_runtime.c`、`tests/tzroutine_runtime.mjs`、`benchmarks/sysmon/handoff.c`（新規）、`benchmarks/run-sysmon.mjs`（新規）、`docs/benchmarks.md`、`docs/architecture.md`、`docs/language.md`、`_tsuzuri/language-reference/async-tasks-and-lazy/tzroutine.md` |

## 実装者向け早見

- 範囲: Windows の sysmon の周期の待ち（`tzr_sysmon` の `delay` の待ち）を、ミリ秒に切り上げる `SleepConditionVariableSRW` から、高分解能の待機タイマー（`CreateWaitableTimerExW` に `CREATE_WAITABLE_TIMER_HIGH_RESOLUTION`。Windows 10 1803 以降）に替える。作れなければ今の経路のまま。
- 目標は「OS のタイマーの刻み（約 15.6 ms）でなく、OS の高分解能タイマーの細かさ（起票時の開発機で約 0.5 ms）で巡回する」こと。**20 µs には届かない**（高分解能タイマーに 20 µs を頼んでも約 0.52 ms かかった）。TZ08 D1 の 20 µs は POSIX の値のままで、Windows の下限は文書に書く。
- 最初に読む: 「現状」の 1〜4 → `tzroutine.c` の `tzr_sysmon`・`tzr_retake`・`tzr_sysmon_kick` → `tzroutine-windows.h` の `tzr_cond_wait_for`。
- 最初に書くもの: `tests/tzroutine_runtime.c` の `blocking_short_calls`（5 ms の呼び出し 40 回のうち、ほかの G が進んだ回を数える）と、`benchmarks/sysmon/handoff.c` の before の計測。
- 触ってはいけない: `TZR_SYSMON_MIN`・`TZR_SYSMON_MAX`・`TZR_SYSCALL_LONG`・`TZR_PREEMPT_AFTER`、`tzr_retake`・`tzr_preempt_scan` の条件、すべての P が空いた間の眠り（`parked` と条件変数の Dekker）、`tzr_sysmon_kick`、TZ05 のタイマーのスレッド（`tzr_timerproc`）、POSIX と JSPI のランタイム。
- 完了の見分け方: Windows で `node tests/tzroutine_runtime.mjs` の `blocking_short_calls` が P 1 で通り、`blocking short calls: 40/40 calls saw the other tzroutine (high-resolution timer)` のような行を出す（今の HEAD の同じ構成は 0/40）。`docs/benchmarks.md` に GUIDE §14 の形の before・after の記録がある。

## 目的

TZ08 は、ブロッキング呼び出し（`extern` 関数、ファイル、標準入出力など）の間、P を手放せる状態にし、sysmon が「同じ呼び出しが続いている」のを見つけたら P を取り上げて別の OS スレッドに渡す（D1: 20 µs から始まる周期）。Windows では sysmon の待ちがミリ秒に切り上がり、さらに OS のタイマーの刻みに丸められるので、1 回の巡回が約 16 ms になる。sysmon は同じ呼び出しを 2 回の巡回で見て初めて取り上げるので、それより短い呼び出しは決して取り上げられない。P の数が少ない（`TSUZURI_MAXPROCS=1`、または P がすべて埋まっている）と、ほかの tzroutine は呼び出しのたびに止まる。このチケットは、Windows の sysmon の周期の待ちを OS の高分解能タイマーで待つようにして、TZ08 の引き継ぎを Windows でも短い呼び出しに効かせる。

## 着手条件と停止条件

### 着手条件

- `node scripts/repo-kb.mjs brief W34`。`gh run list --branch main --workflow ci.yml --limit 1` と `windows.yml` の main の最新の結果を基準にする。
- GUIDE §14 の 1・2: 着手直前の `src/runtime/` を `target/perf/W34/runtime-before/` に写し（C のランタイムだけの変更なので、before はこの写し。`run-tasks.mjs` の `--baseline-runtime` と同じ考え方）、release のコンパイラを `target/perf/W34/tsuzuri-before` に写す。環境（CPU・コア数・OS の版・Clang・Node・コミット）を記録する。
- Windows の開発機（x64。MSVC の Developer PowerShell と LLVM の clang、または同梱の `tsuzuri-clang`）。Arm64 の Windows は `windows.yml` に頼る。

### 停止条件

- **S1** before の計測（手順 1）で、5 ms の呼び出しの構成でもほかの G が進む（今の HEAD の Windows の sysmon が、その機械では刻みより細かく巡回している）。前提が違うので、測った値を記録して報告する。
- **S2** その機械の高分解能タイマーが今の経路より細かくない（仮想機械などで、20 µs を頼んで今の経路と同じ程度かかる）。効果を主張せず、測った値を報告する。
- **S3** `tests/tzroutine_runtime.c` の既存の case（`blocking_*`・`locked_*`・`preempt_*`・`detector`）、`tests/tzroutine.mjs`、`tests/blocking.rs` の結果が変わる、またはデッドロックの判定が変わる。
- **S4** 生成 IR が変わる、または POSIX・JSPI の `tzroutine.c` の前処理後の本文（空行を除く）が変わる。
- **S5** sysmon の CPU 時間（計測 3）が、before と after の中央値の差として揺れの範囲を超えて増える。閾値で合否を決めず、値を記録して人間に判断を求める。
- **S6** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

1. **Windows の待ち**: `src/runtime/tzroutine-windows.h` の `tzr_cond_wait_for(c, m, ns)` は、`ns` を `ns / 1000000 + (ns % 1000000 != 0)` でミリ秒に切り上げ（1 日で頭打ち）、`SleepConditionVariableSRW(c, m, (DWORD)ms, 0)` に渡す。注釈は「rounded up to whole milliseconds」。POSIX の同名の関数（`tzroutine.c` の `#if !defined(_WIN32)` の部分）はナノ秒のまま `pthread_cond_timedwait`（macOS は `pthread_cond_timedwait_relative_np`）に渡す。
2. **sysmon**: `tzroutine.c` の `tzr_sysmon` は、`TZR_SYSMON_MIN`（20000 ns）から始め、何もしない回が 50 を超えると倍にし `TZR_SYSMON_MAX`（10 ms）まで、何かした回の後は 20 µs に戻す周期で、`tzr_cond_wait_for(&tzr_sysmon_state.wake, &tzr_sysmon_state.lock, delay)` で待ってから、ロックを外して `tzr_retake(now)` と `tzr_preempt_scan(now)` を呼ぶ。すべての P が空いていれば、`parked` を立てて期限なしの `tzr_cond_wait_for(..., -1)` で眠り、`tzr_pidleget` の `tzr_sysmon_kick` が起こす。`tzr_sysmon_state.wake` を signal するのは `tzr_sysmon_kick` だけで、`parked` を（ロックなしで）読んで 1 のときだけロックを取って signal する。したがって、**周期の待ちを意図して起こす者はいない**。ただし、2 つの `tzr_pidleget` がほぼ同時に `parked` の 1 を読むと、1 つ目の kick が sysmon を起こし、sysmon が周期の待ちに入った後で、2 つ目の kick がロックを取って signal し、今はその周期が早く終わる。この signal の役目（眠っている sysmon を起こす）はもう果たされているので、失われても意味は変わらない。
3. **取り上げの条件**: `tzr_retake` は、P が `SYSCALL` で、`syscall_tick` が前の巡回で見た値と同じときだけ取り上げる（違えば `sysmon_tick` と `sysmon_when` を覚えて次へ）。したがって、取り上げられるのは、続けて 2 回の巡回にまたがる呼び出し、つまり巡回の間隔より長い呼び出しだけである。巡回の間隔が、取り上げられる最も短い呼び出しを決める。TZ10 の横取りの要求（`tzr_preempt_scan`、`TZR_PREEMPT_AFTER` = 10 ms）も巡回でしか出ないので、Windows では巡回の刻みの単位で遅れる（コードから。計測していない）。
4. **Windows の待ちの実際の長さ**（起票時の観察。Windows 11 Enterprise 10.0.26300、x64、同梱の `tsuzuri-clang` でビルドした C で各 200 回、`QueryPerformanceCounter` で測った。受け入れの基準ではない）:

| 待ち | 最小 | 中央値 | 平均 | 最大 |
| --- | --- | --- | --- | --- |
| `SleepConditionVariableSRW` 1 ms（今の sysmon の 20 µs の待ちが切り上がった値） | 13.0 ms | 15.8 ms | 15.8 ms | 20.0 ms |
| 高分解能の待機タイマー 20 µs | 0.51 ms | 0.52 ms | 0.53 ms | 1.28 ms |
| 高分解能の待機タイマー 100 µs | 0.50 ms | 0.54 ms | 0.54 ms | 1.01 ms |
| 高分解能の待機タイマー 1 ms | 1.02 ms | 1.54 ms | 1.53 ms | 1.73 ms |
| 高分解能の待機タイマー 10 ms | 10.04 ms | 10.24 ms | 10.25 ms | 10.95 ms |
| 普通の待機タイマー 20 µs | 1.02 ms | 15.9 ms | 15.8 ms | 16.8 ms |
| `timeBeginPeriod(1)` の下の `SleepConditionVariableSRW` 1 ms（MSVC で別に 200 回） | 0.96 ms | – | 1.49 ms | 2.51 ms |

   - W29 の書き手も、同じ機械で 1 ms の `SleepConditionVariableSRW` の平均を約 15.6 ms と測っている（[W29](W29-callback-latency.md) の現状 6）。
   - 高分解能タイマーは、この機械では約 0.5 ms より短く待てない。今の経路の約 30 分の 1 だが、TZ08 D1 の 20 µs の約 25 倍である。
5. **再現**（起票時の試作。`tests/tzroutine_runtime.c` と同じく `src/runtime/tzroutine.c` を直接 include した C を、MSVC ABI の LLVM の clang（`--target=x86_64-pc-windows-msvc -fuse-ld=lld`）でビルドし、`TSUZURI_MAXPROCS=1` で 3 回ずつ動かした。G が 2 つ: 1 つは `tsuzuri_tz_blocking_enter` と `exit` の間で `tzr_nanotime` を見ながら 5 ms 回る呼び出しを 40 回、もう 1 つは数を足して `tsuzuri_tz_yield` を繰り返す）:
   - 今の HEAD: 40 回の呼び出しのうち、ほかの G が進んだ回は **0/40**（3 回とも。ほかの G の数は全体で 1）。
   - 呼び出しを 40 ms にすると **5/5**（取り上げそのものは働く。巡回より長い呼び出しだけが取り上げられる）。
   - 「設計」の待ちを入れた試作（`tzroutine-windows.h` と `tzr_sysmon` の写しを `target/scratch` で直したもの）: **40/40**（3 回とも）。同梱の MinGW（`-Wall -Wextra -Werror`）でビルドしても 40/40、直す前は 0/40。P が 2・4 なら前後とも 40/40（ほかの G が自分の P を持つ）。
6. **ヘッダー**: `CREATE_WAITABLE_TIMER_HIGH_RESOLUTION`（0x2）は、同梱の MinGW のヘッダー（`synchapi.h`）では `_WIN32_WINNT >= 0x0600` の下、Windows SDK 10.0.26100 の `synchapi.h` では `_WIN32_WINNT >= _NT_TARGET_VERSION_WIN10_RS4` の下で定義される。どちらも今の既定（`_WIN32_WINNT` 0x0A00）では定義されていた。`CreateWaitableTimerExW`・`SetWaitableTimerEx` は `kernel32` にあり、どちらのツールチェーンでも追加のライブラリなしにリンクできた。
7. **Tsuzuri がサポートする Windows の最も古い版**は、文書（`platforms.md`）に書かれていない（未確認）。1803 より前の Windows のために、今の経路を残す。

## 変えてはいけない意味

- TZ08: ブロッキング呼び出しの間の P の状態、`tzr_retake` の条件（同じ `syscall_tick` を 2 回の巡回で見る、P の環が空で空いた P か spinning の M があり 10 ms 未満なら残す）、`tzr_handoffp`、`errno`・`GetLastError` の保存。
- TZ08 D1 と TZ10 D3 の定数（`TZR_SYSMON_MIN` 20 µs・`TZR_SYSMON_MAX` 10 ms・倍にする規則・`TZR_SYSCALL_LONG` 10 ms・`TZR_PREEMPT_AFTER` 10 ms）。Windows の実際の巡回は OS が丸める。
- すべての P が空いた間の sysmon の眠り（期限なしの条件変数）と `tzr_sysmon_kick`（`parked` と `npidle` の Dekker）。
- デッドロックの判定、タイマーのスレッド、`tzr_note`、ほかのすべての待ち。

## 仕様

1. Windows の sysmon の周期の待ちは、sysmon のスレッドが最初に作る高分解能の待機タイマー（自動リセット）で待つ。期限は `delay` を 100 ns 単位に切り上げた相対時刻（最小 100 ns）。待つ間は `tzr_sysmon_state.lock` を外し、戻る前に取り直す（今の条件変数の待ちと同じ状態で戻る）。
2. タイマーを作れない（SDK のヘッダーが `CREATE_WAITABLE_TIMER_HIGH_RESOLUTION` を Windows 10 1803（RS4）以降の条件の下で定義していることから、それより前の Windows では `CreateWaitableTimerExW` が失敗すると考える。1803 より前の Windows では試していない）か、`SetWaitableTimerEx` か待ちが失敗したら、そのとき以後は今の `tzr_cond_wait_for`（ミリ秒に切り上げる条件変数）で待つ。どちらで待っているかは、テストと計測が読める内部の値に残す。
3. すべての P が空いた間の眠り（`parked`、期限なし）と、`tzr_sysmon_kick` の起こし方は、今の条件変数のまま。
4. 定数（仕様の上の周期）は変えない。Windows の実際の巡回は、OS の高分解能タイマーの細かさが下限になる（起票時の開発機で約 0.5 ms。仕様には数を書かず、文書には測った日と機械とともに書く）。
5. POSIX と JSPI（`TZR_JSPI`）のランタイムは、前処理後の本文（空行を除く）が変わらない。生成 IR は変わらない。
6. TZ05 のタイマーのスレッド（`tzr_timerproc`）の待ちは変えない（対象外）。

## 設計

### 1. Windows の層（`src/runtime/tzroutine-windows.h`）

```c
#ifndef CREATE_WAITABLE_TIMER_HIGH_RESOLUTION
#define CREATE_WAITABLE_TIMER_HIGH_RESOLUTION 0x00000002 /* the MinGW and SDK headers guard it differently */
#endif
/* sysmon's periodic wait (W34): a high-resolution waitable timer (Windows 10 1803 and later), which
 * waits fractions of a millisecond where SleepConditionVariableSRW waits for the next timer tick. Only
 * the sysmon thread touches the handle; NULL means the condition variable's millisecond wait. */
static HANDLE tzr_sysmon_timer;
static _Atomic int tzr_sysmon_hires; /* 1 while the timer is in use: tests and benchmarks read it */
static void tzr_sysmon_wait_init(void) {
    tzr_sysmon_timer = CreateWaitableTimerExW(NULL, NULL, CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
                                              TIMER_MODIFY_STATE | SYNCHRONIZE);
    atomic_store_explicit(&tzr_sysmon_hires, tzr_sysmon_timer != NULL, memory_order_relaxed);
}
/* Waits at most `ns` (>= 0) nanoseconds and returns with `m` held, as tzr_cond_wait_for does. No one
 * means to end this wait early: tzr_sysmon_kick wakes the parked sysmon, and the signal of a late
 * kick, which the condition variable would see, is lost here without harm (sysmon is awake). */
static void tzr_sysmon_wait(tzr_cond *c, tzr_mutex *m, int64_t ns) {
    if (tzr_sysmon_timer != NULL) {
        const int64_t day = 86400 * (int64_t)1000000000;
        LARGE_INTEGER due;
        due.QuadPart = -(((ns < day ? ns : day) + 99) / 100); /* relative, in 100 ns units, rounded up */
        if (due.QuadPart == 0) due.QuadPart = -1;
        if (SetWaitableTimerEx(tzr_sysmon_timer, &due, 0, NULL, NULL, NULL, 0)) {
            tzr_mutex_unlock(m);
            DWORD result = WaitForSingleObject(tzr_sysmon_timer, INFINITE);
            tzr_mutex_lock(m);
            if (result == WAIT_OBJECT_0) return;
        }
        CloseHandle(tzr_sysmon_timer); /* the condition variable from now on */
        tzr_sysmon_timer = NULL;
        atomic_store_explicit(&tzr_sysmon_hires, 0, memory_order_relaxed);
    }
    tzr_cond_wait_for(c, m, ns);
}
```

- 自動リセットのタイマー（`CREATE_WAITABLE_TIMER_MANUAL_RESET` を付けない）なので、待ちが満たされると非シグナルに戻る。次の巡回は `SetWaitableTimerEx` で設定し直す。
- 失敗の後は、その巡回も今の条件変数の待ちで待つ（ロックを持った状態で `tzr_cond_wait_for` を呼ぶ）。失敗を報告しない（利用者に見える意味はなく、遅くなるだけで今と同じ）。

### 2. sysmon（`src/runtime/tzroutine.c`）

- `tzr_sysmon` の先頭（`tzr_async_init` の後、最初の `tzr_mutex_lock` の前）に `#if defined(_WIN32)` の `tzr_sysmon_wait_init();` を置く。sysmon のスレッドの上で作るので、ハンドルを別のスレッドへ公開する同期は要らない。
- 周期の待ちの 1 行を、POSIX の本文を変えない形にする:

```c
#if defined(_WIN32)
        tzr_sysmon_wait(&tzr_sysmon_state.wake, &tzr_sysmon_state.lock, delay);
#else
        tzr_cond_wait_for(&tzr_sysmon_state.wake, &tzr_sysmon_state.lock, delay);
#endif
```

- 期限なしの眠り（`parked`）の `tzr_cond_wait_for(..., -1)` は変えない。sysmon の上の注釈（「It looks every 20 µs …」）に、Windows では OS の高分解能タイマーの細かさが下限で、それがなければタイマーの刻みになることを 1 文足す。
- `tzr_sysmon` は `#endif /* !TZR_JSPI */` の内側にあるので、JSPI のランタイムには入らない。`tsuzuri test` の実行器が部品を別々にコンパイルしても、`tzroutine-windows.h` は driver の `tzroutine_runtime_source_for` が中身で置き換えるので、新しいファイルは要らない（`scripts/check-runtime-includes.sh` は変わらない）。

### 3. 計測（`benchmarks/sysmon/`、GUIDE §14）

- `benchmarks/sysmon/handoff.c`: `tests/tzroutine_runtime.c` と同じく `tzroutine.c` を include する（include のディレクトリをランナーが選ぶ）。P 1 で、呼び出しの長さ 1・2・5・10・20 ms（`tzr_nanotime` を見て回る。OS のタイマーに頼らない）のそれぞれで、(a) ほかの G が進んだ呼び出しの数、(b) 引き継ぎの遅延（呼び出しの開始から、ほかの G が最初に走るまで）の 50・99 パーセンタイルと最大、(c) 呼び出しの間にほかの G が進んだ数、を出す。`tzr_sysmon_hires` も出す。
- `benchmarks/run-sysmon.mjs`: `--runtime <dir>`（既定 `src/runtime`）と `--baseline-runtime <dir>`（`target/perf/W34/runtime-before`）で 2 つを同じ clang でビルドし、順序を交互に入れ替えて 9 回以上ずつ走らせ、PX01 の JSON Lines（suite `sysmon`、workload は構成、variant は `before`・`after`）を `--metrics <dir>` に書く。`--quick` は (a) の数え上げと `tzr_sysmon_hires` を出すだけ（合否はない）。
- sysmon の CPU 時間: 計算だけをする G が 1 つ（P 1、2 秒）の workload と、OS で待つ呼び出しだけを繰り返す workload で、プロセスの CPU 時間（`GetProcessTimes`）を before と after で比べる（sysmon のほかの仕事は前後で同じ）。すべての P が空けば sysmon は眠るので、何もしないプログラムの費用は変わらない（同じ方法で確かめる）。
- Tsuzuri の版（任意）: [W11](_completed/W11-windows-os-api.md) の Phase 1 の後なら、`benchmarks/run-tzroutine.mjs` を Windows でも before と after で走らせ、計算の核が遅くならないことを見る。今の Windows では `Time.monotonic_ns` が `E2002` なので走らない。

## 前提とする他チケットのインターフェース

- なし。

## 他チケットへ提供するインターフェース

- ランタイムの内部（ABI ではない）: Windows の `tzr_sysmon_wait_init()`・`tzr_sysmon_wait(c, m, ns)`・`tzr_sysmon_hires`。
- [W29](W29-callback-latency.md) の D2 の `tzr_sysmon_nudge`（周期の待ちの最中でも sysmon を起こす）を、このチケットの後に入れる場合: Windows の周期の待ちは条件変数でなく待機タイマーなので、条件変数の signal では起きない。W29 は Windows の側で、(a) 同じタイマーを相対 −1（100 ns）で `SetWaitableTimerEx` し直す（起票時に確認: 1 秒で設定したタイマーを別のスレッドが 50 ms 後に設定し直すと、待っていたスレッドは約 0.5〜0.6 ms 後に起きた。3 回）か、(b) 待ちを `WaitForMultipleObjects`（タイマーと自動リセットのイベント）に替えて `SetEvent` する。(a) は高分解能タイマーの細かさ（約 0.5 ms）だけ遅れ、(b) はその遅れがないがハンドルが 1 つ増える。W29 が遅延の計測で選ぶ。このチケットは起こす手段を足さない（D6）。
- W29 が Windows で測るときは、このチケットが `main` に入っているかを記録し、入っていればその上で測る（W29 の D3。順序の依存は作らない）。W29 の D3 の理由（刻みが粗いと P の取り上げが遅れ、コールバックは速い経路を通りやすい）は、このチケットで前提が変わる。

## 実装手順

### 手順 1: before と計測の仕組み

- 変更: `benchmarks/sysmon/handoff.c`・`benchmarks/run-sysmon.mjs`（新規）。
- 内容: 着手条件の GUIDE §14 の 1・2。設計 3。before の値（`--baseline-runtime` だけ）を Windows で測る。macOS か Linux でも 1 回測り、POSIX の値（20 µs の巡回）を比べのために記録する。
- 確認: `node benchmarks/run-sysmon.mjs --quick` が Windows で 5 ms の構成の (a) を 0 か 0 に近い値で出す（S1）。同じ実行を 2 回して同じ傾向。

### 手順 2: 待ちの変更

- 変更: `src/runtime/tzroutine-windows.h`、`src/runtime/tzroutine.c`。
- 内容: 設計 1・2。
- 確認: Windows で `cargo check --all-targets`。`sh scripts/check-runtime-includes.sh`。POSIX と JSPI の前処理後の本文の比較（受け入れ条件 3 のコマンド）。

### 手順 3: テスト

- 変更: `tests/tzroutine_runtime.c`（`blocking_short_calls`）、`tests/tzroutine_runtime.mjs`（`cases` に足す）。
- 内容: テスト計画 1。
- 確認: Windows で `node tests/tzroutine_runtime.mjs`（MSVC の clang）。同梱の MinGW でも `tests/tzroutine_runtime.c` を `-Wall -Wextra -Werror` でビルドして `blocking_short_calls` を走らせる（記録する）。POSIX で `TSUZURI_TSAN=1 TSUZURI_ASAN=1 node tests/tzroutine_runtime.mjs`。`cargo build && node tests/tzroutine.mjs target/debug/tsuzuri`。`cargo test --locked --test blocking`（`running N tests` を確かめる）。

### 手順 4: after の計測

- 変更: なし。
- 内容: GUIDE §14 の 3〜5。`node benchmarks/run-sysmon.mjs --baseline-runtime target/perf/W34/runtime-before --samples 9 --metrics target/perf/W34-after`。速くなった理由は、ランタイムの経路（`tzr_sysmon_hires` と、5 ms の構成の (a)・(b) の前後）で示す。`tests/fixtures/blocking` と `examples/tzroutines` の `--emit llvm` が前後で byte 単位で同じであることを確かめる。
- 確認: 差が揺れの範囲で、経路の違いで説明できなければ効果を主張しない（S2）。CPU 時間は S5。

### 手順 5: 記録と文書

- 変更: 「ドキュメント」のページ。
- 内容: GUIDE §14 の 7。
- 確認: `node scripts/check-docs.mjs _tsuzuri/language-reference/async-tasks-and-lazy/tzroutine.md`。T2。PR に `ci:full` を付け、`windows.yml`（x64・Arm64）が通る。

## テスト計画

1. **C のランタイム**（`tests/tzroutine_runtime.c` の `blocking_short_calls`、すべてのホスト、`-O0`・`-O3` × P 1・2・4）: 「現状」5 の構成（5 ms の呼び出し 40 回は `tzr_nanotime` を見て回り、OS のタイマーに頼らない。ほかの G は数を足して `tsuzuri_tz_yield`）。ほかの G が進んだ呼び出しが **1 回以上**であることを確かめる（下限だけ。時間の閾値はない。AGENTS.md）。Windows で `tzr_sysmon_hires` が 0（高分解能タイマーがない）なら、検査をせずに `fallback` と出す（`tzr_sysmon_hires` は `tzroutine-windows.h` にだけあるので、読む所は `#if defined(_WIN32)` で囲み、POSIX のビルドも `-Werror` で通るようにする）。結果の数を 1 行で出す。POSIX では TZ08 D1（20 µs の巡回）の回帰の検査になる。
2. **既存**: `blocking_handoff`・`blocking_reacquire`・`blocking_deadlock`・`blocking_callback`・`locked_thread`・`locked_yield`・`locked_yielders`・`preempt_flag`・`preempt_sysmon`・`detector` が変わらない。`tests/tzroutine.mjs`（Windows の `busy` を含む）、`tests/blocking.rs`、`tests/net.mjs` の段 `tzroutine`（`vscode.yml` の Windows）。
3. **本文の同一性**: 受け入れ条件 3。
4. **性能**: `benchmarks/run-sysmon.mjs`（手元だけ。CI には `--quick` も入れない）。

## ドキュメント

- `docs/benchmarks.md`: 新しい節「Windows の sysmon の待ち（W34）」。計測の方法・構成・環境・コミット・before と after の中央値と最小〜最大・遅延の分布・CPU 時間・生データの場所（`target/perf/W34-before`・`W34-after`）。「現状」4 の待ちの長さの観察も、測った日と機械とともに書く。測っていない効果（Arm64 の Windows、仮想機械）は「測っていないもの」に書く。
- `docs/architecture.md` の TZ08 の段落（「sysmon は最初の spawn で始まる 1 本のスレッドで、20 µs ごと…」）: Windows は周期の待ちを高分解能の待機タイマーで待ち、作れなければ条件変数のミリ秒の待ちに戻ること。実際の巡回は OS のタイマーの細かさが下限であること。
- `docs/language.md` の「OS を待つ呼び出し（TZ08）」の箇条と `_tsuzuri/language-reference/async-tasks-and-lazy/tzroutine.md` の「P の引き継ぎ」（どちらも「同じ呼び出しが 20 µs ほど続いている」）: Windows では OS のタイマーの細かさ（Windows 10 1803 以降は高分解能タイマー、それより前は既定のタイマーの刻み）が下限になり、それより短い呼び出しの P は取り上げられないことを書く。数は書くなら測った日と機械とともに。

## 受け入れ条件

- [ ] 「仕様」の 1〜6 のとおりに動き、`blocking_short_calls` が Windows（MSVC の CI の x64・Arm64、手元の同梱の MinGW）と POSIX（TSan・ASan を含む）の `-O0`・`-O3` × P 1・2・4 で通る。
- [ ] 既存のランタイムのテスト・`tests/tzroutine.mjs`・`tests/blocking.rs` の結果と、生成 IR が変わらない。
- [ ] POSIX と JSPI の `tzroutine.c` の前処理後の本文が、空行を除いて変更の前後で同じ: 同梱の `tsuzuri-clang` で `-target x86_64-linux-gnu -std=c11 -D_GNU_SOURCE -E -P src/runtime/tzroutine.c` と `-target aarch64-macos -std=c11 -E -P src/runtime/tzroutine.c`（起票時にこの 2 つが Windows の機械で動くことを確かめた）、と wasm32 の `-DTZR_JSPI`（driver が JSPI のランタイムに渡す旗で）。
- [ ] `docs/benchmarks.md` に GUIDE §14 の記録があり、測っていない効果と 20 µs を Windows で主張していない。CI に速度の閾値を足していない。
- [ ] 文書を直し、`check-docs` が通る。T2 が通る。PR に `ci:full` を付け、`windows.yml` が通る。

## 落とし穴

- **20 µs には届かない**: 高分解能タイマーにも下限がある（起票時に約 0.5 ms）。仕様・文書・PR の説明で「TZ08 D1 の 20 µs が Windows でも成り立つ」と書かない。
- 待つ間はロックを外す。`tzr_sysmon_kick` は `parked` を（ロックなしで）読んでからロックを取るので、`parked` が 1 の間に読んだ kick は、眠りから戻った sysmon が次の周期の待ちに入ってからロックを取りにくることがある。条件変数の待ちは中でロックを外すので、今はすぐに取れる。タイマーの待ちでロックを持ったままにすると、P を取った M（`tzr_pidleget` が kick を呼ぶ）が最大で 1 周期（10 ms）止まる。
- 期限は負の値（相対）にする。正の値は絶対時刻で、0 は過去の絶対時刻になる。
- 周期の待ちを起こす経路を足さない（今も意図して起こす経路はなく、遅れて来た kick の signal だけが今は周期を早く終わらせる。「現状」2）。足すなら W29 の D2 で、方法は「他チケットへ提供するインターフェース」のとおり。
- `CREATE_WAITABLE_TIMER_MANUAL_RESET` を付けると、待ちが満たされてもシグナルのままで、次の巡回の待ちが即座に終わり、sysmon が CPU を回し続ける。
- マクロ `CREATE_WAITABLE_TIMER_HIGH_RESOLUTION` は、MinGW と SDK のヘッダーで定義の条件が違う。`#ifndef` で定義する（「現状」6）。
- テストの呼び出しを `Sleep` で作らない（`Sleep(5)` 自体がタイマーの刻みに丸められ、約 16 ms の呼び出しになる。今の `block_ms` は `Sleep`）。`tzr_nanotime` を見て回る。
- 速度の閾値をテストに入れない。CI の Windows のランナーの高分解能タイマーの細かさは未確認なので、テストは「1 回以上」の下限だけにする。
- 20 µs から 0.5 ms までの周期はどれも約 0.5 ms で待つので、何もしない回が 50 を超えて倍にし始めるまでの時間は、POSIX（20 µs × 50 で約 1 ms）より長い（約 0.5 ms × 50 で約 25 ms）。その間 sysmon は約 0.5 ms ごとに起きる。CPU 時間の計測（設計 3）で確かめる。

## 対象外

- TZ05 のタイマーのスレッド（`tzr_timerproc`）の待ち。同じ `tzr_cond_wait_for` でミリ秒に切り上がり、`Time.sleep (Time.millisecond())` を 50 回繰り返すプログラム（開発版、同梱の MinGW）は、全体で 1.33 秒と 2.74 秒かかった（2 回）。この待ちは新しいタイマーで起こされる（`tzr_timers.kicked` と条件変数の signal）ので、待機タイマーにするには起こし方も変える必要があり、`Time.sleep`・`Ticker` の利用者に見える長さが変わる。TZ05 はタイマーの分解能を保証しない（対象外）。この待ちは [W41](W41-windows-timer-thread-resolution.md) が扱う（このチケットに依存する。上の全体の時間はプロセスの起動を含み、W41 の「現状」2 がプログラムの中で 1 回ごとの待ちを測り直した）。
- 20 µs に近づけるための spin（D4）と、`timeBeginPeriod`（D3）。
- Windows の非同期の横取り（TZ10 Phase 2 の Windows 版）と、コールバックの P の取り戻しの遅延（[W29](W29-callback-latency.md)）。
- `tzr_note` の待ち（期限なし）とネットワークの待ち（netpoller）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | Windows の sysmon の周期の待ちだけを、高分解能の待機タイマーで待つ | 「現状」4・5 のとおり、今の経路の約 30 分の 1 の間隔で巡回でき、5 ms の呼び出しの P を取り上げられた（試作で 0/40 → 40/40）。OS の文書化された API で、タイマーの刻み（プロセスやシステムの設定）を変えない。周期の待ちを意図して起こす者はいない（遅れて来た kick の signal は失われるが、sysmon はもう起きているので意味は変わらない。「現状」2）ので、条件変数を手放せる | 既定案（実装者はこの案に従う） |
| D2 | タイマーを作れないか、設定・待ちに失敗したら、今の条件変数のミリ秒の待ちに戻り、報告しない | 1803 より前の Windows（SDK のヘッダーの条件からの推定。試していない）と予期しない失敗でも、今と同じ振る舞いで動き続ける。サポートする最も古い Windows の版は未確認（「現状」7）。遅くなるだけで意味は変わらないので、利用者に知らせることがない。テストと計測は `tzr_sysmon_hires` で見分ける | 既定案（実装者はこの案に従う） |
| D3 | `timeBeginPeriod` を使わない | 測った待ちは平均約 1.5 ms で高分解能タイマーより粗い。タイマーの刻みを変える設定で電力の費用があり、`export` やライブラリとして連結されるとホストのプロセスにも効く。`winmm` のリンクも要る（[W29](W29-callback-latency.md) の採らない案と同じ） | 既定案（実装者はこの案に従う） |
| D4 | 20 µs に近づけるために spin（`SwitchToThread`・`YieldProcessor` で期限まで回る）しない | sysmon は何かした回の後 20 µs に戻るので、引き継ぎの続くプログラムでは sysmon が 1 つの CPU を回し続け、P の計算と CPU を取り合う | 既定案（実装者はこの案に従う） |
| D5 | `TZR_SYSMON_*` などの定数を Windows 用に変えない | OS が下限で丸めるので、定数を変えなくても実際の間隔は OS のタイマーの細かさになる。POSIX と同じ本文を保ち、TZ08 D1・TZ10 D3 を改めずに済む | 既定案（実装者はこの案に従う） |
| D6 | 周期の待ちを起こす手段（`tzr_sysmon_wake` など）はこのチケットで足さない | このチケットに使う者がいない（`tzr_sysmon_kick` は眠っている sysmon だけを起こす）。時間に頼らずに試験できないコードを残さない。W29 の D2 が要るなら、W29 が遅延の計測で方法を選ぶ（他チケットへ提供するインターフェース） | 既定案（実装者はこの案に従う） |
| D7 | TZ05 のタイマーのスレッドの待ちは変えない | 起こし方（`kicked` と signal）を変える必要があり、`Time.sleep`・`Ticker` の利用者に見える長さが変わる。TZ05 は分解能を保証しない。[W41](W41-windows-timer-thread-resolution.md) で扱う（このチケットの補助に起こす手段を足す） | 既定案（実装者はこの案に従う） |
