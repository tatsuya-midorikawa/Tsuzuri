# W29: コールバックの P の取り戻しの遅延を計測して短い経路を速くする

| 項目 | 内容 |
| --- | --- |
| ID | W29 |
| 優先度 | P2 |
| 規模 | M |
| 依存 | W12 |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-22（D-63。利用者の依頼「X: D13 を狭く改める方針で決定。実現向けたチケットを @_features に W プリフィックスのチケットを作成して」と「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」から起票。HEAD `fa40eede` で確認） |
| 承認 | 不要（意味を変えない。スケジューラーの順序だけを変える） |
| 改善する劣位 | リスク「コールバックの遅延」: tzroutine を起動するプログラムで、固定した tzroutine の UI がモーダルループ（窓の移動・大きさの変更、`MessageBoxW`、`DialogBoxW`、`TrackPopupMenu`、`DoDragDrop`）に入ると、WndProc が呼ばれるたびに P を取り戻す。sysmon が P を持ち去っていて、空いた P がなければ、ほかの tzroutine が譲るまで待つ。その遅延を誰も測っていない |
| 手本にする既存実装 | `benchmarks/run-tzroutine.mjs`（結果の照合と `--baseline`）、`benchmarks/run-tasks.mjs`（`--runtime`・`--baseline-runtime` で C のランタイムだけを差し替える）、`benchmarks/metrics.mjs`（PX01 の JSON Lines）、`tests/tzroutine_runtime.c`（`tzroutine.c` を直接 include する C のテスト。`blocking_callback`・`locked_thread`・`preempt_sysmon`）、`src/runtime/tzroutine.c` の `tzr_exitsyscall`・`tzr_acquirep`・`tzr_pidleput`・`tzr_serve_pwait`・`tzr_retake`・`tzr_preempt_scan`・`tzr_sysmon_kick` |
| 主な影響ファイル | `benchmarks/callbacks/latency.c`（新規）、`benchmarks/callbacks/Main.tz`（新規）、`benchmarks/callbacks/host.c`（新規）、`benchmarks/run-callbacks.mjs`（新規）、`src/runtime/tzroutine.c`、`tests/tzroutine_runtime.c`・`tests/tzroutine_runtime.mjs`、`docs/benchmarks.md`、`docs/architecture.md`、`_tsuzuri/language-reference/async-tasks-and-lazy/tzroutine.md` |

## 実装者向け早見

- 範囲: (1) コールバックの入口（`tsuzuri_tz_callback_enter`）と出口の費用と遅延の分布を、3 つの構成で計測する仕組みを作る。(2) 遅延の大部分が「空いた P がなく、P を待つ」ことだと計測で分かったときだけ、コールバックの入口が P を待つときに優先する（D2）。(3) GUIDE §14 の手順で前後を測り、`docs/benchmarks.md` に記録する。意味（コールバックは P を持って走り、M に固定され、P の数を超えて走らない）は変えない。
- 最初に読む: 現状の 1〜7、`tzroutine.c` の上の関数、GUIDE §14。
- 最初に書くもの: `benchmarks/callbacks/latency.c` の構成 1・2（`main` の擬似 G と、空いた P のある固定した G）。結果の照合（呼んだ回数）だけで成功・失敗を決める `--quick`。
- 触ってはいけない: `tsuzuri_tz_callback_enter() -> i32`・`tsuzuri_tz_callback_exit(i32)` の型と、W12 が足す `tsuzuri_tz_callback_scope_enter`／`_leave` との順序、`src/llvm.rs` の `with_blocking_externs` が出す IR、`tzr_exitsyscall` の `errno`・`GetLastError` の保存、デッドロックの判定、TZ10 の 10 ms（コールバックの入口が P を待つ場合のほか）。
- 完了の見分け方: `node benchmarks/run-callbacks.mjs target/release/tsuzuri --baseline target/perf/W29/tsuzuri-before --samples 9 --metrics target/perf/W29-after` が 3 つの構成の前後の中央値・最小・最大と遅延の分布を出し、`docs/benchmarks.md` に記録がある。D2 を入れたなら、`tests/tzroutine_runtime.c` の `callback_urgent` が通る。

## 目的

[GUIDE D-63](GUIDE.md#d-63-tsuzuri-だけで-win32-のアプリを書くe12-d13-の狭い改訂w01w30) の Win32 のアプリは、WndProc などの静的コールバックを OS から何度も呼ばれる。tzroutine を起動するプログラムでは、コールバックの入口が P を取り戻し（TZ08）、出口で P を手放せる状態に戻す。普段はロックのない compare-exchange 1 回で済むが、sysmon が P を持ち去った後は、空いた P を全体のロックの下で取るか、P が返ってくるまで OS スレッドを眠らせる。モーダルループの中の UI では、メッセージのたびにこの遅い経路を通りうる。このチケットは、まずその費用と遅延を測り、遅延の原因が P の待ちなら、コールバックの入口を優先して短くする。

## 着手条件と停止条件

### 着手条件

- [W12](W12-static-callback-lifetime.md) が done（静的コールバックを、受け取った `extern` が戻った後も呼べる契約。構成 3 の、`RegisterClassExW` の後に `DispatchMessageW` から呼ばれる WndProc はこの契約の上で測る）。W12 の前でも、手順 1・2 の `extern` の呼び出しの中で呼ばれる構成（1・2 と、構成 3 の C だけの版）は測ってよい。
- `node scripts/repo-kb.mjs brief W29`。GUIDE §14 の 1・2（before のコンパイラを `target/perf/W29/tsuzuri-before` に写し、環境を記録する）。
- Windows の機械（構成 3 のモーダルループと、現状 6 の時計）と、macOS か Linux の機械（同じ経路の比較）。

### 停止条件

- **S1** 計測の結果、構成 2・3 の遅延の大部分が P の待ちではない（たとえば全体のロックの取り合い、OS のメッセージの配送、sysmon の周期そのもの）。D2 を入れずに、内訳と候補を記録して報告する。
- **S2** D2 を入れると、`tests/tzroutine_runtime.c`（`blocking_*`・`locked_*`・`preempt_*`・`detector`）、`tests/tzroutine.mjs`、`tests/blocking.rs` の結果が変わる、またはデッドロックの判定が変わる。
- **S3** 生成 IR を変える必要が出た（このチケットはランタイムの C と計測だけ）。
- **S4** 前後の差が揺れの範囲で、生成コードやランタイムの経路の違いで説明できない。効果を主張しない。
- **S5** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

1. **入口と出口の下げ方**: `src/llvm.rs` の `with_blocking_externs` は、tzroutine を起動する native のプログラム（`spawning && !wasm`）だけで、すべての `extern` の呼び出しを `tsuzuri_tz_blocking_enter`／`exit` で囲み、`tz.callback.*` と `tz_<export>` の本体の入口に `%tz.callback = call i32 @tsuzuri_tz_callback_enter()`、各 `ret` の前に `tsuzuri_tz_callback_exit(i32 %tz.callback)` を入れる。起動しないプログラムのコールバックは普通の C の関数で、P の概念はない。
2. **`main` は P を扱わない**: `tsuzuri_tz_callback_enter` は `tzr_this_m()` が NULL か、`m->curg` が NULL か、`m->syscall_depth` が 0 なら何もせず 0 を返す。`main` のスレッドは M ではなく擬似 G（`tzr_current` が `tzr_pseudo` を返す）なので、`main` のトップレベルの UI のコールバックは P を取り戻さない。遅延が問題になるのは、`Routine.with_locked_thread` で固定した tzroutine の UI と、tzroutine の中のブロッキングの `extern` から呼ばれるコールバックだけである。
3. **速い経路**: `tsuzuri_tz_callback_enter` は深さを保存して 0 にし、`tzr_exitsyscall` を呼ぶ。P の状態が `SYSCALL` のままなら compare-exchange で `RUNNING` に戻して終わり、`tzr_lockm` で G を M に固定する。出口の `tsuzuri_tz_callback_exit` は `tzr_unlockm` と `tzr_entersyscall`（atomic の書き込みと加算）で戻る。
4. **遅い経路**: sysmon が P を持ち去っていると、`tzr_exitsyscall` は `errno` と `GetLastError` を保存して `tzr_acquirep` を呼ぶ。`tzr_acquirep` は `tzr_sched.lock` を取り、`tzr_pidleget` で空いた P を取る。なければ `tzr_pwait_push` で待ち行列の**末尾**に並び、`m->park` で眠る。P は、別の M が P を手放すとき（`tzr_pidleput`）、G と G の間（`tzr_serve_pwait`）、固定した G の譲渡（`tzr_yield_locked`）で、来た順に渡される。
5. **sysmon**: `tzr_sysmon` は 20 µs から、何もしない 50 回の後は倍にして 10 ms まで待つ周期で回る。`tzr_retake` は、前の回から同じ呼び出しにいる P を取り上げる（P の行列が空で、空いた P か探している M があり、呼び出しが 10 ms より若ければ残す）。`tzr_preempt_scan` は、同じ G を 10 ms 走らせている P に、待っている仕事（P の行列、全体の行列、`npwait`）があれば譲るよう求める（TZ10 D3）。したがって、すべての P が計算で埋まっているとき、コールバックの M は、どれかの G が譲るか横取りの要求に応えるまで（最短でも 10 ms の判定と sysmon の 1 回）待つ。`tzr_sysmon_kick` は sysmon が眠り込んでいる（`parked`）ときだけ起こし、周期の待ちの最中は起こさない。
6. **Windows の時計**: `tzroutine-windows.h` の `tzr_cond_wait_for` はナノ秒を**ミリ秒に切り上げて** `SleepConditionVariableSRW` に渡す。起票時に Windows 11 Enterprise 10.0.26300 の開発機で、1 ms の `SleepConditionVariableSRW` を 50 回測る小さな C（同梱の MinGW でビルド。削除した）を 2 回走らせると、最小は約 4.9 ms と 10.8 ms、平均は約 15.6 ms だった（OS の既定のタイマーの刻み）。つまり Windows の sysmon の 1 回は、TZ08 D1 の想定（20 µs）ではなく、OS のタイマーの刻みに近い。これはコールバックの遅延を測るときの前提で、このチケットでは変えない（D3。直すのは [W34](W34-windows-sysmon-timer-resolution.md)）。
7. **非同期の横取りは POSIX だけ**: `TZR_ASYNC` は `__APPLE__` か `__linux__` のときだけ定義される（TZ10 Phase 2）。Windows では、呼び出しのないループを走る G は横取りの要求に応えない（`tests/tzroutine.mjs` の `phaseOneOnly`）。
8. **計測がない**: コールバックの費用と遅延を測る benchmark も記録もない（`benchmarks/run-tzroutine.mjs` は横取りの検査の費用だけ）。

## 変えてはいけない意味

- コールバックの本体は P を持って走り、終わるまで M に固定される（TZ08 の「着手時の再検討」2）。同時に Tsuzuri のコードを走らせるスレッドは P の数を超えない（`Routine.max_procs`）。
- `tzr_exitsyscall` の後の `errno`・`GetLastError` は呼び出しの直後の値（`net.c` と `extern` の C が P を取り戻した後に読む。TZ08）。
- デッドロックの判定（`tzr_checkdead`）、TZ10 の 10 ms の要求（コールバックの入口が P を待つ場合のほか）、TZ08 の `retake` の条件。

## 仕様

- 利用者から見える意味・API・IR は変わらない。変わりうるのは、すべての P が埋まっているときに、コールバックの入口が P を得るまでの時間だけ（D2）。
- 計測の結果は `docs/benchmarks.md` に、日付・環境・コミット・中央値・最小・最大・生データの場所とともに書く。測っていない効果は書かない。CI に速度の合否を入れない。

## 設計

### 1. 計測（`benchmarks/callbacks/`）

| 構成 | 内容 | 何を測るか |
| --- | --- | --- |
| 1 `main` | `main` の擬似 G で、C のホストの `extern` がコールバックを N 回呼ぶ | 入口と出口が何もしない経路の費用（基準） |
| 2 `locked-idle` | 固定した tzroutine の中の `extern` が、(a) すぐに N 回、(b) sysmon が P を取り上げる長さだけ OS で待ってから 1 回、を繰り返す。ほかの P は空いている | (a) compare-exchange の速い経路、(b) `tzr_pidleget` の経路 |
| 3 `locked-busy` | 構成 2(b) と同じ。ただし残りの P はすべて計算する tzroutine（横取りの検査を通るループ）で埋める | P の待ちの遅延（`tzr_pwait_push` から P を得るまで） |
| 3w `modal`（Windows、W06 と W12 の後） | 固定した tzroutine がメッセージ専用の窓のループを回し、別のスレッドが `PostMessageW` を一定の間隔で送り、WndProc の入口で `QueryPerformanceCounter` の差を記録する。残りの P は構成 3 と同じく計算で埋める | 実際の UI に近い、送ってから WndProc の本体までの遅延 |

- C の版（`latency.c`）は `tests/tzroutine_runtime.c` と同じく `src/runtime/tzroutine.c` を直接 include し、`tsuzuri_tz_go`・`tsuzuri_tz_blocking_enter`・`tsuzuri_tz_callback_enter` を呼んで、入口から出口までと「OS の待ちを抜けてから本体に入るまで」を単調時計で測る。Tsuzuri の版（`Main.tz` と `host.c` を `--link`）は生成したコールバックの包みを含めて同じ構成を測る。
- 出力: 各構成について、N 回の合計の経過時間（PX01 の `wall_time`、suite `callbacks`、workload は構成の名前、variant は `before`・`after`）と、遅延の 50・99 パーセンタイルと最大（ランナーの `--json` の別ファイル。PX01 の指標の集合は変えない）。コールバックを呼んだ回数を照合し、合わなければ失敗（`--quick` はこれだけ）。
- 遅い経路の内訳を知るため、計測のビルドだけで有効な数え上げ（`-DTZR_CALLBACK_STATS`。`tzr_exitsyscall` の速い経路・`tzr_pidleget`・`tzr_pwait_push` の回数と待ちの時間）を `tzroutine.c` に `#if defined(TZR_CALLBACK_STATS)` で足してよい。ソースの text は変わるが、既定のビルドでコンパイルされるコードと生成 IR は変えない（S3 に当たらない）。

### 2. 改善（D2。手順 2 の内訳で P の待ちが大部分のときだけ）

- `tsuzuri_tz_callback_enter` だけが使う `tzr_acquirep_callback(m)` を足す（`tzr_exitsyscall` に引数か別の入口で渡す。ほかの呼び出し元の `tzr_acquirep` は変えない）。
  1. `tzr_sched.lock` の下で `tzr_pidleget`。取れればそこで終わり（今と同じ）。
  2. 取れなければ、M を待ち行列の**先頭**に入れ（`tzr_pwait_push_front`）、`tzr_sched.npwait_urgent` を 1 増やしてロックを外す。
  3. sysmon を、周期の待ちの最中でも起こす（`tzr_sysmon_nudge`: `parked` を問わずロックの下で条件変数を signal する）。[W34](W34-windows-sysmon-timer-resolution.md) が入った後の Windows では、周期の待ちは条件変数でなく高分解能の待機タイマーなので、条件変数の signal では起きない（黙って効かない）。Windows では W34 の「他チケットへ提供するインターフェース」の 2 つの方法（タイマーを相対 −1 で設定し直す、`WaitForMultipleObjects` とイベント）のどちらかを、遅延の計測で選ぶ。
  4. `tzr_preempt_scan` は、`npwait_urgent > 0` なら、10 ms を待たずに、同じ G を最も長く走らせている `RUNNING` の P 1 つに譲るよう求める（`tzr_preempt_ask`）。求められた G は次の検査で全体の行列の末尾へ譲り、その M が `tzr_serve_pwait` で先頭のコールバックの M に P を渡す。
  5. P を得たら `npwait_urgent` を 1 減らす。
- 意味: 走る G の数は P の数を超えない。優先するのは P を取り上げられたコールバックの入口だけで、コールバックの本体が長く走れば、普通の G と同じく 10 ms の要求の対象になる。出口は今と同じく P を手放せる状態に戻すので、ホストがコールバックを続けて呼んでも、ほかの G は sysmon の取り上げで進む。
- Windows で呼び出しのないループを走る G は要求に応えない（現状 7）。その場合の遅延は今と同じで、文書に書く。

### 3. 採らない案

- コールバックを P なしで走らせる: P の数の上限が破れ、本体の横取りの検査・`spawn`・チャンネルの起床は P を前提にする。
- P を本体が必要になるまで取らない（遅延の取得）: ランタイムのすべての入口に検査が要り、誤ると P なしで G を起こす。
- 固定した G の P を sysmon が取り上げない: UI が `GetMessageW` で長く待つ間、ほかの tzroutine がその P を使えなくなる。
- UI の G に P を 1 つ専用に割り当てる: `Routine.max_procs` の意味と文書が変わる。
- `timeBeginPeriod(1)` でプロセス全体のタイマーの刻みを細かくする: 電力の費用があり、ライブラリや `export` の利用ではホストのプロセス全体に効いてしまう。

## 前提とする他チケットのインターフェース

- [W12](W12-static-callback-lifetime.md): 静的コールバックを、受け取った `extern` が戻った後も、どのスレッドからでも呼べる契約。W12 は `tsuzuri_tz_callback_enter() -> i32`・`tsuzuri_tz_callback_exit(i32)` の名前と型を保ち、`tz.callback.*`（export でない方）に `tsuzuri_tz_callback_scope_enter()`／`_leave(ptr)` を足す（入るときは `callback_enter` → `callback_scope_enter`、出るときは `callback_scope_leave` → `callback_exit`。W12 の「他チケットへ提供するインターフェース」が「W29 はこの経路を含めて遅延を測る」と書く）。構成 1〜3 の Tsuzuri の版は、この 2 つを含めた W12 の最終の包みで測り、C の版も同じ順で呼ぶ。D2 はこの順序を変えない。
- [W34](W34-windows-sysmon-timer-resolution.md)（入っていれば）: Windows の sysmon の周期の待ちの `tzr_sysmon_wait`（高分解能の待機タイマー）と `tzr_sysmon_hires`。D2 の `tzr_sysmon_nudge` の Windows の起こし方は、W34 の「他チケットへ提供するインターフェース」の (a)・(b) から選ぶ。
- [W06](W06-windows-gui-test-harness.md)（構成 3w だけ）: `tests/windows_gui_shim.c` の `tz_gui_window`・`tz_gui_wait`・`tz_gui_post`（別のスレッドからの `PostMessageW(WM_APP)`）と `buildGui`。名前が変わっていたら W06 に合わせる。

## 他チケットへ提供するインターフェース

- `benchmarks/run-callbacks.mjs` と `docs/benchmarks.md` の記録（W26 の COM のコールバック、W28 の UI の検証が遅延の比較に使える）。
- D2 を入れた場合は `tzr_acquirep_callback`（ランタイムの内部。ABI ではない）。

## 実装手順

### 手順 1: 計測の仕組み

- 変更: `benchmarks/callbacks/latency.c`・`Main.tz`・`host.c`、`benchmarks/run-callbacks.mjs`。
- 内容: 設計 1 の構成 1〜3（と、W06・W12 が done なら 3w）。GUIDE §14 の 1〜3（before のコンパイラ、環境、9 回以上の標本）。
- 確認: `node benchmarks/run-callbacks.mjs target/release/tsuzuri --quick` が呼んだ回数を照合して成功する。2 回続けて実行して同じ照合の結果になる。

### 手順 2: before の計測と内訳

- 変更: なし（計測のビルドの `-DTZR_CALLBACK_STATS` を足すなら `src/runtime/tzroutine.c` のその `#if` の部分だけ。既定のビルドでコンパイルされるコードと生成 IR は変えない）。
- 内容: Windows と macOS か Linux で、`--samples 9 --metrics target/perf/W29-before`。構成 3・3w の遅延を、速い経路・`tzr_pidleget`・P の待ちに分ける。
- 確認: 記録をこのチケットの「実装と検証」に書く。P の待ちが大部分でなければ S1 で止める。

### 手順 3: 改善（D2）

- 変更: `src/runtime/tzroutine.c`（`tzr_acquirep_callback`・`tzr_pwait_push_front`・`npwait_urgent`・`tzr_sysmon_nudge`・`tzr_preempt_scan`）、`tests/tzroutine_runtime.c`（`callback_urgent`）、`tests/tzroutine_runtime.mjs` の `cases`。
- 内容: 設計 2。`callback_urgent` は、P を 1・2・4 にし、残りの P を検査を通る計算で埋めたまま、P を取り上げられたコールバックの入口が計算の終わりより前に P を得ること（順序で見る。時間の上限は比べない）、終わった後に `nmsys`・`npwait`・`npwait_urgent` が 0 であることを確かめる。
- 確認: `node tests/tzroutine_runtime.mjs`（Windows は MSVC の clang）、POSIX で `TSUZURI_TSAN=1 TSUZURI_ASAN=1 node tests/tzroutine_runtime.mjs`。`cargo build && node tests/tzroutine.mjs target/debug/tsuzuri`。`cargo test --locked --test blocking`（`running N tests` を確かめる）。

### 手順 4: after の計測と生成コードの確認

- 変更: なし。
- 内容: GUIDE §14 の 3〜5。`--baseline target/perf/W29/tsuzuri-before --samples 9 --metrics target/perf/W29-after`。速くなった理由を、ランタイムの経路（数え上げの内訳）で示す。生成 IR が前後で同じことを `--emit llvm` の `cmp` で確かめる。
- 確認: 差が揺れの範囲なら S4。

### 手順 5: 記録と文書

- 変更: `docs/benchmarks.md`（新しい節「コールバックの P の取り戻し（W29）」）、`docs/architecture.md`（TZ08 の節のコールバックの入口）、`async-tasks-and-lazy/tzroutine.md`（コールバックの段落に、P が埋まっているときの振る舞いと Windows の制限。測った値は書くなら日付と環境つきで）。
- 内容: GUIDE §14 の 7。
- 確認: `node scripts/check-docs.mjs _tsuzuri/language-reference/async-tasks-and-lazy/tzroutine.md`。T2。PR に `ci:full`（Windows のランタイムを変える）。

## テスト計画

- C: `tests/tzroutine_runtime.c` の `callback_urgent`（手順 3）。既存の `blocking_callback`・`locked_thread`・`locked_yield`・`locked_yielders`・`blocking_deadlock`・`preempt_sysmon`・`detector` が変わらない。
- E2E: `tests/tzroutine.mjs`（変わらない）。
- 性能: `benchmarks/run-callbacks.mjs`（手元だけ。CI には `--quick` も入れない。照合は手順 1 の確認で行う）。
- IR: このチケットの前後で `tests/fixtures/blocking` と `examples/tzroutines` の `--emit llvm` が byte 単位で同じ。

## ドキュメント

- `docs/benchmarks.md`: 計測の方法・構成・環境・中央値・最小・最大・遅延の分布・生データの場所（`target/perf/W29-before`・`W29-after`）。現状 6 の Windows の時計の観察も、測った日と機械とともに書く。
- `docs/architecture.md` の TZ08 の節: D2 を入れたなら、コールバックの入口の優先と `npwait_urgent`。
- `async-tasks-and-lazy/tzroutine.md`: コールバックの費用の段落（`main` のトップレベルのコールバックは P を扱わないこと、固定した tzroutine の UI は P が埋まっていると待ちうること、Windows では呼び出しのないループが譲らないこと）。

## 受け入れ条件

- [ ] `benchmarks/run-callbacks.mjs` が構成 1〜3（W06・W12 の後は 3w も）を測り、GUIDE §14 の形で before と after を記録した。
- [ ] 内訳で P の待ちが大部分なら D2 を入れ、`callback_urgent` が 1・2・4 の P、`-O0`・`-O3`、TSan・ASan（POSIX）、Windows で通る。そうでなければ内訳を記録して報告した。
- [ ] 既存のランタイムのテスト・`tests/tzroutine.mjs`・`tests/blocking.rs` の結果と、生成 IR が変わらない。
- [ ] `docs/benchmarks.md` に記録があり、測っていない効果を書いていない。CI に速度の閾値を足していない。
- [ ] T2 が通る。PR に `ci:full` を付けた。

## 落とし穴

- `main` のスレッドで測っても遅い経路は出ない（現状 2）。固定した tzroutine の中で測る。
- sysmon に P を取り上げさせるには、OS で待つ長さが sysmon の 1 回より長く、しかも取り上げる条件（P の行列に仕事があるか、空いた P も探す M もないか、10 ms を超えたか）を満たす必要がある。Windows の 1 回はタイマーの刻みに近い（現状 6）ので、POSIX と同じ待ちの長さでは取り上げられないことがある。構成ごとに `-DTZR_CALLBACK_STATS` の数え上げで、遅い経路を通った回数を確かめる。
- 計算で P を埋める tzroutine は、横取りの検査を通るループにする（C の版は `tests/tzroutine_runtime.c` の `preempt_sysmon` と同じく、`tsuzuri_tz_preempt_pending` が 0 でなければ `tsuzuri_tz_preempt_check(&tsuzuri_tz_preempt_pending)` を呼ぶ。Windows には非同期の横取りがない）。検査のないループだと、構成 3 は D2 の有無にかかわらず P を返さない。
- 待ち行列の先頭に入れる M と、`tzr_pidleput`・`tzr_serve_pwait`・`tzr_yield_locked` の渡し方が食い違うと、M が二重に起こされるか取り残される。`npwait` と `npwait_urgent` は同じロックの下で増減する。
- `tzr_exitsyscall` の遅い経路の `errno`・`GetLastError` の保存と復元は、新しい入口でも同じ位置に置く。
- 時間の検査は順序と下限だけにする（AGENTS.md の速度の閾値の禁止）。

## 対象外

- Windows の sysmon の待ちの刻み（TZ08 D1 の 20 µs が Windows では保てないこと）を高分解能の waitable timer で直すこと。コールバックの遅延ではなく、ブロッキングの呼び出しの P の引き継ぎと横取りの全体の周期の問題なので、[W34](W34-windows-sysmon-timer-resolution.md) が直す（D3）。
- Windows の非同期の横取り（TZ10 Phase 2 の Windows 版）。
- コールバックの本体の中の待ち（`Channel`・`Mutex`）の費用、W12 の範囲を持たない呼び出しの境界の費用。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | まず測り、P の待ちが遅延の大部分だと分かったときだけ D2 を入れる | 速い経路は compare-exchange 1 回で、測らずに変えると得がない。GUIDE §14 と AGENTS.md の「測ってから主張する」 | 既定案（実装者はこの案に従う） |
| D2 | P を取り上げられたコールバックの入口は、空いた P がなければ待ち行列の先頭に並び、sysmon を起こし、10 ms を待たずに 1 つの P に譲るよう求める | UI の遅延は 1 回のメッセージの待ちで決まり、10 ms とタイマーの刻みの待ちは目に見える。走る G の数・本体の横取り・デッドロックの判定は変えずに、順序だけを変える。TZ10 D3 の 10 ms は、この場合だけ待たない | 既定案（実装者はこの案に従う） |
| D3 | Windows の sysmon の待ちの刻みは、このチケットでは変えず、観察として記録する。刻みの修正は [W34](W34-windows-sysmon-timer-resolution.md) | 刻みが粗いと P の取り上げが遅れ、コールバックはむしろ速い経路を通りやすい。刻みの修正はブロッキングの呼び出しと横取り全体に効き、電力の費用も測る必要がある。W34 が入ると刻みは高分解能タイマーの細かさ（W34 の起票時の開発機で約 0.5 ms）になり、この理由の前提が変わるので、Windows の計測は W34 が `main` に入っているかを記録し、入っていればその上で測る | 既定案（実装者はこの案に従う） |
| D4 | 計測は C の版（ランタイムを直接）と Tsuzuri の版（生成した包み）の両方で、PX01 の `wall_time` と別ファイルのパーセンタイルに出す | 内訳を見るには C の版、利用者の経路を見るには生成コードの版が要る。PX01 の指標の集合を増やさない | 既定案（実装者はこの案に従う） |
| D5 | ABI・IR は変えない | `with_blocking_externs` の出力と `tsuzuri_tz_callback_*` の型は、ホストが連結する LLVM IR とランタイムの契約 | 既定案（実装者はこの案に従う） |
