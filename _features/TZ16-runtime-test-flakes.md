# TZ16: tzroutine のランタイムの C のテストが CI でときどき落ちる（Windows x64 の `stack_size` は Xeon Platinum 8573C の runner だけで、`donor` の spinning の数は Windows x64 と Linux arm64 の ThreadSanitizer で）

| 項目 | 内容 |
| --- | --- |
| ID | TZ16 |
| 優先度 | P2（CI の再実行の原因。2026-11-18 は 2 回続けて Windows x64 のゲートを落とし、そのたびに 25 分の job を流し直した。調べた結果がランタイムの不具合（あふれが報告されない、spinning の数が戻らない）なら P1 に上げる） |
| 規模 | M（まず失敗を説明できる情報をテストに出させ、runner で再現する。直す側は原因しだいで、`src/runtime/tzroutine-windows.h`・`src/runtime/tzroutine.c` か `tests/tzroutine_runtime.c` の検査） |
| 依存 | TZ01, TZ02, TZ08（すべて done） |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-18（CI の短縮（PR #109・#110）の `ci:full` の実行で、Windows x64 のゲートの `tzroutine runtime (TZ01)` が 2 回続けて別の検査で落ちたため。利用者の依頼で起票。2026-11-02 以降の `windows.yml` の 493 回と `targets.yml` の 82 回の実行のログから、失敗を検査・runner の CPU・ビルドごとに分けた） |
| 承認 | 不要（検査の情報を増やし、原因に合わせてランタイムかテストを直す。言語の意味・診断・生成コードは変えない。ランタイムの振る舞いを変えるときは、その差をこのチケットに記録する） |
| 改善する劣位 | CI の信頼性: 変更と関係なく Windows x64 のゲートが落ち、失敗した job を流し直すまでマージできない。不具合の可能性: (1) ある CPU で、tzroutine のスタックのあふれが Windows の `STATUS_STACK_OVERFLOW` で終わらない。(2) スケジューラーの spinning の M の数 `tzr_sched.nmspinning` が 10 秒たっても 0 に戻らない（数え間違いか、起こした M が起きない） |
| 手本にする既存実装 | `tests/tzroutine_runtime.c` の `run_child_with`（Windows は `CreateProcessA` と `GetExitCodeProcess`）、`test_stack_size`・`test_donor`、`tests/tzroutine_runtime.mjs`（`-O0`・`-O3` × `TSUZURI_MAXPROCS` 1・2・4、Linux の TSan・ASan のビルド）、`src/runtime/tzroutine-windows.h` の `tzr_fiber_create`（`CreateFiberEx(commit, reserve, …)`。64 KiB の reserve なら commit は 16 KiB）、`src/runtime/tzroutine.c` の `tzr_wakep`・`tzr_startm`・`tzr_findrunnable`・`tzr_serve_pwait`・`tzr_schedule`（`nmspinning` を増減する所） |
| 主な影響ファイル | `tests/tzroutine_runtime.c`, `tests/tzroutine_runtime.mjs`, `src/runtime/tzroutine-windows.h`, `src/runtime/tzroutine.c`（原因しだい）, `.github/instructions/ci.instructions.md`（Windows の教訓）, `_features/README.md` |

## 目的

`tests/tzroutine_runtime.c` の 2 つの検査が、変更と関係なく CI でときどき失敗する理由を突き止め、原因を直す。
失敗は CI の再実行を強いるだけでなく、ランタイムの不具合を隠している可能性がある。
どちらの検査も、今は失敗したときに原因を判断する情報（子の終了コード、スケジューラーの状態）を出さない。

1. **`stack_size`**: 64 KiB の tzroutine のスタックで 100 KiB の再帰をする子が、`STATUS_STACK_OVERFLOW` で終わらない（`tests/tzroutine_runtime.c:485`、`check failed: overflowed(s)`）。
2. **`donor`**: 100 回の各回の後、`tzr_sched.nmspinning` が 10 秒以内に 0 に戻らない（`tests/tzroutine_runtime.c:972`、`check failed: atomic_load(&tzr_sched.nmspinning) == 0`）。

## 現状（2026-11-18、`main` の `f01b5b8b` で確認）

### 調べた範囲

- `windows.yml`: 2026-11-02 以降の 493 回の実行のうち、`tzroutine runtime (TZ01)` を実行した 195 個の job。成功 138、失敗 22、skip 35。
- 失敗した 22 個のうち、11-02〜11-04 の TZ 系のブランチの多くは開発中に直った失敗です。`fp_mode` の x87 の状態、`netpoll_pipe`（デバッグ用のブランチ）、検査の行を出さずに落ちた TZ07・TZ08・TZ11・TZ12 のブランチの 8 個がそれです。残りが下の 2 種類です。
- `targets.yml`（Linux・macOS）: 2026-11-02 以降の 82 回の実行の `tzroutine, Channel and select` の手順。成功 161、失敗 17。失敗のうち `donor` は 3 個でした。
- runner の CPU は、各 job の `Show tools` が出す `Win32_Processor` の `Name` です。

### 1. `stack_size`：Windows x64 の Xeon Platinum 8573C の runner だけで落ちる

失敗した 4 回は、すべて `INTEL(R) XEON(R) PLATINUM 8573C` の runner でした。

| 日付 | ブランチ（run） | ビルド | 検査 |
| --- | --- | --- | --- |
| 2026-11-02 | `feat/tz04-select`（41475396555） | `tzroutine-O0.exe`, `TSUZURI_MAXPROCS=1` | `tests/tzroutine_runtime.c:472`（当時の行）`overflowed(s)` |
| 2026-11-03 | `feat/tz06-sync`（41713573957） | 同上 | 同上 |
| 2026-11-06 | `main`（42189140369） | 同上 | `:485` `overflowed(s)` |
| 2026-11-18 | `test/split-slow-tests`（PR #110、44261343261） | 同上 | 同上 |

同じ期間に TZ01 の手順が成功した Windows x64 の job の CPU は次のとおりです。

| CPU | 成功 | `stack_size` の失敗 |
| --- | --- | --- |
| AMD EPYC 7763 | 29 | 0 |
| AMD EPYC 9V74 | 20 | 0 |
| AMD EPYC 9V45 | 7 | 0 |
| Intel Xeon Platinum 8573C | 5 | 4 |
| Intel Xeon Platinum 8370C | 3 | 0 |

8573C では 9 回中 4 回落ち、ほかの CPU では 59 回落ちていません。
Windows arm64（Cobalt 100）、Linux、macOS ではこの検査は落ちていません。
落ちるのは最初の子（`child-deep-100` を `TSUZURI_TZROUTINE_STACK_KIB=64` で動かす）です。`CHECK` は失敗するとその場でプロセスを終えるので、落ちた実行では後の子は動いていません。8573C で成功した 5 回では、同じ関数の 3 つ目の子（既定の 1 MiB のスタックで 3.5 MiB の再帰をする `child-deep-3500`、`:489`）を含め、すべての子が期待どおりに終わっています。

Windows の子の判定は `overflowed(status)` が `(DWORD)status == STATUS_STACK_OVERFLOW` で、失敗しても実際の終了コードを出しません。
そのため、子が別の例外で終わったのか（`0xC0000005` のアクセス違反、`0xC0000409` の fail fast など）、終了コード 0 で終わったのかは、今のログからは分かりません。

### 2. `donor`：Windows x64 と Linux arm64 の TSan で spinning の数が戻らない

| 日付 | ブランチ（run） | runner | ビルド | 検査 |
| --- | --- | --- | --- | --- |
| 2026-11-04 | `feat/tz12-remove-task`（41875924001） | Windows x64、AMD EPYC 9V45 | `tzroutine-O0.exe`, `TSUZURI_MAXPROCS=2` | `:967`（当時の行） |
| 2026-11-04 | `feat/tz13-remove-async`（41847001735） | Linux arm64 | `tzroutine-tsan`, `TSUZURI_MAXPROCS=4` | `:967` |
| 2026-11-11 | `main`（42938720307） | Linux arm64 | `tzroutine-tsan`, `TSUZURI_MAXPROCS=4` | `:972` |
| 2026-11-12 | `feat/a24-anonymous-unions`（43229349711） | Linux arm64 | `tzroutine-tsan`, `TSUZURI_MAXPROCS=4` | `:972` |
| 2026-11-18 | `ci/parallel-jobs`（PR #109、44250846909） | Windows x64、AMD EPYC 9V74 | `tzroutine-O3.exe`, `TSUZURI_MAXPROCS=2` | `:972` |

Windows x64 では 60 回あまりのうち 2 回、Linux arm64 では 50 回あまりのうち 3 回（すべて ThreadSanitizer のビルド）です。
OS をまたいで起きるので、Windows に固有の問題ではなく、スケジューラーの問題と考えられます。
検査のコメントは「M は数マイクロ秒で休む。混んだ runner は起こした M をしばらく動かさないことがある（#88 の windows.yml で 2 秒を超えた）。10 秒の上限は休まない spinner のため」としています。
10 秒は、混んだ runner での遅れとしては長すぎます。

### 関連する観測（このチケットの対象外）

Linux の `locked_yielders`（`:2090`、`yields_after_queued < 100`）も 3 回落ちています。11-04 の TZ12・TZ13 のブランチの x64 の TSan、11-05 の `main` の arm64 の guard 付きのビルドです。
11-05 より後は落ちていません。同じように時間に依存する検査なので、調べる途中で似た原因が見つかれば記録します。

## 仮説

### `stack_size`

- **H1（例外の配送に要るスタック）**: 8573C（第 5 世代 Xeon、Emerald Rapids）は AMX を持ち、表のほかの CPU（Xeon 8370C と 3 つの AMD EPYC）は持ちません。
  例外を配送するとき、OS は `CONTEXT` と拡張状態（XSAVE の領域）を利用者のスタックに積みます。拡張状態が大きいと、64 KiB の reserve のファイバーでは、guard page を踏んだ後に配送する余地が足りないことがあります。そのとき、`STATUS_STACK_OVERFLOW` ではなく別の例外か fail fast で終わります。
  8573C で成功した実行で 1 MiB のスタックの子が落ちなかったことと、8573C でも毎回は落ちないこと（VM が拡張状態をどう見せるか、再帰がどこで guard page を踏むか）は、この仮説と矛盾しません。ただ、確かめてはいません。
- **H2（子の別の終わり方）**: 子が overflow の前か後に別の理由で終わっている（scheduler のスレッドの終了の競合など）。終了コードが分かれば H1 と区別できます。

### `donor`

- **H3（数え間違い）**: `nmspinning` を増やした後に減らさない経路がある。`tzr_wakep` は数を 1 にしてから `tzr_startm(NULL, 1)` を呼びます。idle の P が無ければ `tzr_startm` が減らします。M を起こすとき（`tzr_note_wake`）か新しいスレッドを作るときは、その M が spinning のまま走り始め、`tzr_findrunnable`・`tzr_schedule`・`tzr_serve_pwait` のどこかで減らします。どこかで二重に足すか、減らす前に別の経路へ抜けると、数が戻りません。
- **H4（起こした M が起きない）**: `tzr_startm` が idle の M に P を渡して起こしたが、その M が起きない（Windows の note、Linux の futex の待ちで合図を失う）。この場合、その M が持つ P も使われないままになります。
- **H5（遅れ）**: 起こした M が 10 秒以上走らない。TSan や混んだ Windows の VM でも 10 秒は長いので、可能性は低いと見ています。H3・H4 を退けてから考えます。

## 調査と修正の手順

### 手順 1: 失敗を説明できる情報をテストに出させる

- `run_child_with` の呼び出し側が、期待と違う終了を報告するとき、実際の値を出す。Windows は 16 進の終了コード、POSIX は signal と終了コードです。`CHECK` を、値を書く形（例えば `CHECK_STATUS(overflowed, s)`）にする。
- `test_donor` の待ちが 10 秒を超えたとき、`CHECK` の前にスケジューラーの状態を出す。`nmspinning`・`npidle`・`nmidle`・`npwait`・`mcount`・`runq_size`、`allm` をたどった M ごとの `spinning`・`p`・`nextp`・待ちの状態、`allp` の各 P の状態と run queue の長さです。テストは `src/runtime/tzroutine.c` を `#include` していて `tzr_sched` を直接読めるので、テスト側の関数で足ります。
- Windows では、`stack_size` の子の前に、使える拡張状態（`GetEnabledXStateFeatures`）と、`InitializeContext2(CONTEXT_ALL | CONTEXT_XSTATE)` が求める大きさを 1 行出す（H1 の材料）。

この手順だけでもマージしてよい。以後の CI の失敗が、原因を判断する情報を残すようになります。

### 手順 2: 再現する

- **`stack_size`**: 使い捨てのブランチの workflow（`.github/instructions/ci.instructions.md` の、Windows だけで起きる失敗を、そのブランチの `on: push` だけで動く workflow で調べる手順）で、`windows-2022` の job を十数個並べます。各 job で `stack_size` の子を、64・96・128・256 KiB のスタックでそれぞれ数十回動かし、CPU と終了コードを記録します。8573C が割り当たった job で H1・H2 を判断します。
- **`donor`**: Linux arm64 のコンテナ（Apple Silicon ならネイティブ）で、TSan のビルドと `TSUZURI_MAXPROCS=4` で `donor` を繰り返し動かします。CPU に負荷をかけて（`stress` など）、手順 1 の状態を集めます。Windows x64 は上と同じ使い捨ての workflow で `-O0`・`-O3`、`TSUZURI_MAXPROCS=2` を繰り返します。

### 手順 3: 直す

- **H1 が正しい場合**: ランタイムの側で、あふれを確実に報告する方法を選びます。小さい reserve のときに guard の余地を足す（`tzr_fiber_create` の commit と reserve の関係を見直す）、各スレッドに `SetThreadStackGuarantee` を設定する、の比較です。テストの判定を広げる（別の終了コードも overflow とみなす）のは、その終了コードが overflow だけから来ると説明できたときに限ります。どちらにしたかと、その理由をこのチケットに記録します。
- **H3・H4 が正しい場合**: スケジューラーの数え方か待ちを直し、回帰テスト（その経路を必ず通る、短い `donor` の変形）を足します。
- **H5 だけが残る場合**: 検査を「数が 0 に戻る」から、遅れに左右されない形（その M が P を持たずに休んでいることなど）に変えます。上限を伸ばして通すことはしません。

## 受け入れ条件

- 2 つの検査が、失敗したときに原因を判断する情報（終了コード、スケジューラーの状態）を出す。
- 原因を特定し、このチケットに記録する（どの仮説か、再現の記録、直した場所）。
- 直した後、使い捨ての workflow で、8573C の runner での `stack_size` と、`donor`（Windows x64 の `-O0`・`-O3`、Linux arm64 の TSan）を繰り返して、1 回も落ちない（回数は手順 2 の再現率から決め、記録する）。
- `windows.yml` の `tzroutine runtime (TZ01)` と `targets.yml` の `tzroutine, Channel and select` が、この 2 つの検査で落ちなくなる。直した後の `main` の 10 回の実行で確かめる。
- ランタイムを変えたときは、`tests/tzroutine_runtime.mjs`（全ビルド）、`node tests/tzroutine.mjs`、`node tests/features.mjs <binary> tzroutine` と、Windows の `windows.yml` が通る。

## 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- 原因がランタイムの不具合で、直すとスケジューラーの性能（`benchmarks/run-tzroutine.mjs`）か、利用者から見える振る舞いが変わる。
- 8573C の runner が十分な回数割り当たらず、H1 を判断できない。その場合は手順 1 だけをマージし、以後の CI の失敗が出す情報を待つ。
- 検査の上限を伸ばすか、検査を外さないと通らない。

## 対象外

- `locked_yielders` の失敗（上の「関連する観測」）。似た原因と分かったときだけ扱う。
- Windows のあふれの報告の内容（理由の表示）。Windows の `src/runtime/stack.c` は Unix だけで、Windows は今も理由を出さない。

## ドキュメント

- 原因と直した方法を `.github/instructions/ci.instructions.md` の Windows の教訓に 1 行足す（テストの不安定さだったとき）。ランタイムの不具合だったときは `docs/architecture.md` の tzroutine のスタックかスケジューラーの節と、利用者に見える違いがあれば言語リファレンスの該当ページ（`grep -rn "TSUZURI_TZROUTINE_STACK_KIB" _tsuzuri/language-reference`）を直す。
- `_features/README.md` の TZ16 の行の状態。
