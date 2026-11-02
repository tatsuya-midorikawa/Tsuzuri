# TZ01: tzroutine のランタイムの核（M:N スケジューラー・スタック・文脈切り替え・待ち・チャンネル・select）

| 項目 | 内容 |
| --- | --- |
| ID | TZ01 |
| 優先度 | P0 |
| 規模 | XL。Phase 1（POSIX: macOS・Linux の aarch64・x86-64。付録 B の参照実装を置き、C の単体テストで検査する）L、Phase 2（Windows x64・Arm64。Fiber と Win32 の待ち）L |
| 依存 | – |
| 後続 | TZ02（生成コードから呼ぶ）、TZ03（チャンネル）、TZ04（`select`）、TZ05（タイマー）、TZ06（同期）、TZ08（sysmon・netpoller）、TZ09（WASM）、TZ10（プリエンプション） |
| 状態 | done（2026-11-02） |
| 起票 | 2026-11-01（D-61。HEAD `9a76d8ac` で確認） |
| 承認 | 承認済み（D-61: 利用者の回答 2026-11-01）。決定事項 D1〜D9 は既定案（実装者はこの案に従う） |
| 手本にする既存実装 | `src/runtime/task.c`（weak hidden の公開、`TZ_STACK_GUARD` のときの `tsuzuri_stack_thread()`、テスト用のマクロの差し替え）、`src/runtime/task-windows.h`（pthread の Win32 への写像）、`tests/task_runtime.c`・`tests/tasks.mjs`（ランタイムの C を `#include` して単体で検査する形）、`src/runtime/stack.c`（ガードページと SIGSEGV の報告） |
| 主な影響ファイル | `src/runtime/tzroutine.c`（新規）、`src/runtime/tzroutine-windows.h`（新規。Phase 2）、`tests/tzroutine_runtime.c`（新規）、`tests/tzroutine_runtime.mjs`（新規）、`src/runtime/stack.c`、`.github/workflows/targets.yml`・`.github/workflows/windows.yml`、`docs/architecture.md`、`README.md`（検証の表） |

## 実装者向け早見

- 範囲: **ランタイムの C だけ**。コンパイラ（`src/*.rs`）は 1 行も変えない（生成コードからの呼び出しは TZ02）。Phase 1 は POSIX、Phase 2 は Windows。**指示がなければ Phase 1 だけ**。
- 最初に読む（この順）: ① [付録](TZ01-appendix.md) の A（実測）と D（アルゴリズム）② 決定事項 D1〜D9 ③ `src/runtime/task.c` の先頭 90 行（マクロと公開の形）④ `src/runtime/stack.c`（全文、140 行）⑤ GUIDE の D-61。
- 書くもの: 付録 B-1 の C を **そのまま** `src/runtime/tzroutine.c` に置き、付録 B-2 を `tests/tzroutine_runtime.c`、付録 B-3 を `tests/tzroutine_runtime.mjs` に置く。付録 B-4 の 2 か所を `src/runtime/stack.c` に足す。参照実装は macOS arm64 と x86-64（Rosetta）、P の数 1〜8、`-O0`〜`-O3`、`stack.c` との連結、TSan、ASan + UBSan で検査済み（付録 A）。**参照実装の論理を「直したく」なったら、まず停止条件 S3 を読む。**
- 最初に通すテスト: `node tests/tzroutine_runtime.mjs`（`tzroutine runtime: 112 runs passed`。Rosetta のない Linux では 98。`TSUZURI_TSAN=1 TSUZURI_ASAN=1` を付けると 136、Linux では 122）。
- 触ってはいけない: 既存の `task.c`・`async.c`・`sync.ll`・`channel-wasm.ll` と、それらを使うプログラムの IR と実行（TZ12・TZ13 が消すまで残る）。`stack.c` の既存の挙動（登録した OS スレッドの窓、報告の文 `trap: stack overflow`）。
- 完了の見分け方: Phase 1 は `node tests/tzroutine_runtime.mjs` が macOS と Linux（CI の `targets.yml`）で通り、`cargo nextest run --locked` が変わらず通る（Rust 側は変えていない）。Phase 2 は同じテストが `windows.yml` の x64 で通る。
- 迷ったら: 決定事項 → 付録 D → 停止条件 → 最も保守的な選択（その機能を入れずに止める）の順。

## 目的

D-61 の tzroutine（Go の goroutine に相当する、専用のスタックを持つ軽量な実行単位）を実行する **ランタイムの核** を作る。後続のチケットは、ここで作った C の関数を生成コードから呼ぶだけにする。

- **M:N のスケジューラー**: G（tzroutine）を、P（G を走らせる権利。既定は CPU 数）を持つ M（OS スレッド）が走らせる。P ごとの run queue と仕事の盗みで、生成・待ち・再開を OS を介さずに行う。
- **待ちの土台**: `tzr_park`／`tzr_ready`（待つ・起こす）、アドレスで待つ parking lot（Mutex・WaitGroup などの土台）、タイマー（`sleep`）、チャンネル（バッファなし・あり、`close`）、`select`（複数のチャンネルを同時に待ち、準備のできた腕を一様乱数で選ぶ）。
- **tzroutine でないスレッドの参加**: `main` のスレッドや、`export` を呼んだホストのスレッドは **擬似 G** として同じ待ちに参加する（自分の OS スレッドを止めて待つ）。
- **スコープと終了の待ち**: D-61 の決定 1（`main`・`export`・`test` は、その中で生まれた全 tzroutine の終了を待つ）を実装する `tsuzuri_tz_scope_enter`／`leave`。
- **デッドロックの判定**: 全員が待ちに入り、起こせる者がいないとき、Go と同じく判定して報告する。

HEAD で測った数字（付録 A。Apple M3、macOS 27.2、参照実装、`-O2`）:

| 測ったもの | 結果 |
| --- | --- |
| 10 万個の tzroutine を生成して終了を待つ（P 8） | 0.047 秒（1 個 約 470 ns。スタックは再利用） |
| バッファなしチャンネルの往復（tzroutine 同士、P 8／P 1） | 303 ns／120 ns |
| 同（`main` の擬似 G と tzroutine。OS スレッドの起床を含む） | 約 5.2 µs |
| 2,000 個が同時に 20 ms 眠る | 27 ms で全員が起きる |
| 既存の `Task` + `Channel`（比較。[channel.md](../_tsuzuri/language-reference/built-in-types-and-modules/channel.md#待ちの規則)） | 待つタスクの数がスレッド数を超えると完了しない。tzroutine はこの制約がない |

非目標（このチケットではしない）: コンパイラへの接続（TZ02〜）、ブロッキング呼び出しの P の引き継ぎと netpoller（TZ08）、プリエンプション（TZ10）、WASM（TZ09）、RwMutex などの追加の同期（TZ06）、チャンネルを使うタイマー（TZ05）。

## 着手条件と停止条件

### 着手条件

- 依存なし。GUIDE §2.3 の基準: `gh run list --branch main --workflow ci.yml --limit 1` が成功（赤なら赤い検査を手元で再現して無関係を記録）。
- 手元で `clang --version`（17 以降）と `node --version`（20 以降）。macOS の Apple silicon では `arch -x86_64 /usr/bin/true` が成功すれば x86-64 も手元で走る（Rosetta）。
- ベースラインとして `cargo nextest run --locked` の通過件数を控える（このチケットは Rust を変えないので、件数は変わらない）。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と選択肢を報告する（GUIDE §13）。

- **S1** `node tests/tzroutine_runtime.mjs` が Linux（CI の `targets.yml` の x86-64・aarch64）で落ちる。とくに `failures` の子プロセスがガードページの違反を `SIGSEGV`／`SIGBUS` で終わらない、`deep` が落ちる（`RLIMIT_STACK` やスタックの予約の扱いの違い）。選択肢: A 付録 D-7 のスタックの取り方を OS ごとに分ける、B Linux の該当テストを理由を書いて飛ばす（人間の承認が要る）。
- **S2** TSan・ASan が参照実装に対して報告を出す（手元の macOS では 0 件。付録 A-3）。報告を抑制するフラグ（`TSAN_OPTIONS=suppressions`、`no_sanitize` の属性）で黙らせない。
- **S3** 参照実装の論理（付録 D の規則 R1〜R4、spinning の手順、`select` の 3 段）を変えたくなった。性能の改善のためでも、このチケットでは変えない（TZ01 の範囲は参照実装を正しく置くこと。改善は計測付きの別チケット）。バグを見つけた場合は、再現する C のテストを `tests/tzroutine_runtime.c` に足してから直し、直した内容をチケット末尾に記録する。
- **S4** `-Wall -Wextra -Werror` で、手元にない OS・コンパイラ（gcc、古い clang、musl）から警告が出る。警告を `#pragma` で消さず、原因を直す。直し方が 1 つに決まらないなら止める。
- **S5**（Phase 2）Windows の Fiber で、`tests/tzroutine_runtime.c` の `deep`（深い再帰）・`fp`（浮動小数点レジスタ）・`external`（ホストのスレッド）のどれかが落ちる。選択肢: A `CreateFiberEx` の予約と commit の大きさを変える、B Windows の該当機能を `E2002` にして TZ02 が拒否する（人間の承認が要る）。
- **S6** 新しい crate・`unsafe` Rust・既定の WASM の import が要るように見える（このチケットは C だけで、要らないはず）。

## 現状（HEAD `9a76d8ac` で確認）

- 並行の実行は `src/runtime/task.c` のスレッドプールだけ。`tz_task_pool` の 1 つの `pthread_mutex` がすべてのグループ・チャンネル・待ちの数え上げを守る（`task.c` 115〜139 行）。追加ワーカーは `min(CPU 数, 32) - 1`（`TZ_TASK_MAX_THREADS = 32`、47 行）。チャンネルを待つスレッドは `tz_channel_wait`（612 行）で OS スレッドごと止まり、未着手の仕事を自分のスタックで手伝う（`tz_task_help_one`、深さ 16）。全員が待つと `tz_channel_check_deadlock`（302 行）が `deadlock: every task is waiting on a channel` を出す。
- ランタイムの C は、生成コードが記号を使うときだけ、1 つの翻訳単位に連結される（`src/driver.rs` 4673〜4722 行。順序は `os.c`・`net.c`・`async.c`・GPU・`trap.c`・`task.c`・`cpu.c`・`io.c`・`arguments.c`）。公開する関数は POSIX では `__attribute__((weak, visibility("hidden")))`（`task.c` 1〜8 行の `TZ_TASK_API`）。2 つの Tsuzuri の object を 1 つのホストに連結しても重複しないため。
- スタック溢れの報告は `src/runtime/stack.c`。各 OS スレッドのスタックの下端の窓（最大 64 本）を登録し、SIGSEGV／SIGBUS の handler が窓に入った違反だけを `trap: stack overflow` と報告して `abort()` する。プールのワーカーは `-DTZ_STACK_GUARD` のとき `tsuzuri_stack_thread()`（sigaltstack と窓の登録）を最初に呼ぶ（`task.c` 341〜343 行）。**専用のスタック（tzroutine のスタック）の違反は、いまの handler では既定の動作（理由なしの異常終了）になる。**
- 生成コードの関数の frame の上限は `MAX_FRAME_BYTES = 64 KiB`（`src/check.rs` 64 行）。生成 IR に stack probe（`"probe-stack"`）はない。
- 生成コードが直接使う TLS は、`--allocator small` のスレッドごとのキャッシュ（`heap-small.ll`、確保の中だけで使う）と Async の状態（TZ13 で消える）だけ。`trap.c` の `tz_frame` は `--trap-mode return` のときだけ。
- 再現（既存の制約。`tests/fixtures/channel` 相当）: `Task.parallel` で互いを待つ 3 段のパイプラインを、`TSUZURI_THREADS` ではなく CPU 1 の環境で走らせると、判定のトラップで終わる（[channel.md](../_tsuzuri/language-reference/built-in-types-and-modules/channel.md#待ちの規則) の表）。

## 変えてはいけない意味

- tzroutine を使わないプログラム（生成 IR に `tsuzuri_tz_` の記号がないプログラム）は、このチケットの前後で、IR・object・実行ファイル・実行結果が 1 byte も変わらない。このチケットは `tzroutine.c` をどこからも連結しないので、自動的に成り立つ（`stack.c` の変更は、`tsuzuri_tz_stack_guard_hit` を呼ぶ 1 か所だけで、tzroutine を使わないプログラムではフックが空のまま）。
- `stack.c` の既存の報告（文、`abort()`、登録した窓の外の違反は既定の動作）。
- 浮動小数点の環境: G の切り替えは、x86-64 の MXCSR と x87 の制御語、aarch64 の d8〜d15 を保存する（丸めモードと例外マスクを G ごとに保つ）。新しい G は既定値（MXCSR `0x1F80`、x87 `0x037F`）で始まる。

## Phase 分割

| Phase | 内容 | 検証 |
| --- | --- | --- |
| 1 | POSIX（macOS・Linux、aarch64・x86-64）の全機能。付録 B を置く | `node tests/tzroutine_runtime.mjs`（手元と CI の macOS・Linux） |
| 2 | Windows（x64・Arm64）。G を Fiber で表し、待ちを Win32 の SRWLOCK・条件変数・`WaitOnAddress` で表す | `windows.yml` で同じテスト |

## 仕様

### Phase 1

#### 1.1 用語と構造

| 名前 | C の型 | 意味 |
| --- | --- | --- |
| G | `tzr_g` | tzroutine 1 つ。専用のスタック、保存した `sp`、状態（`IDLE`・`RUNNABLE`・`RUNNING`・`WAITING`・`DEAD`）、数えられるスコープ、生成した子を数えるスコープ |
| 擬似 G | `tzr_g`（`kind == TZR_KIND_THREAD`） | tzroutine でない OS スレッド（`main`・ホストのスレッド）。スタックを持たず、待つときは自分のスレッドを `tzr_note` で止める。P を持たない |
| M | `tzr_m` | スケジューラーの OS スレッド。自分のスタック（g0）でスケジューラーを走らせ、G へ切り替える |
| P | `tzr_p` | G を走らせる権利。`nprocs` 個（`TSUZURI_MAXPROCS`、既定はオンラインの CPU 数、1〜256）。256 の環の run queue と `runnext` を持つ |
| スコープ | `tzr_scope` | 生成した tzroutine の数（孫を含む）と、それを待つ G。根のスコープ `tzr_root_scope` と、`tsuzuri_tz_scope_enter` が作るスコープ |
| sudog | `tzr_sudog` | チャンネルの待ちの記録（48 バイト）。待つ G のスタックに置く |

#### 1.2 公開する C の関数（生成コードとの ABI）

すべて `TZR_API`（`__attribute__((weak, visibility("hidden")))`）。状態の値は `TZR_OK = 0`・`TZR_CLOSED = 1`・`TZR_DEADLOCK = 2`・`TZR_WOULD_BLOCK = 3`。**`TZR_DEADLOCK` を受け取った生成コードはトラップする**（TZ02 以降。理由はランタイムが先に標準エラーへ書いている）。

| 関数 | 意味 | 後続 |
| --- | --- | --- |
| `void tsuzuri_tz_go(void (*code)(void *), void *env)` | G を作って実行可能にし、すぐ戻る。`code(env)` が新しい G の上で 1 回走る。親の生成スコープ（なければ親のスコープ、どちらもなければ根）で数える。スタックを確保できなければ `Tsuzuri runtime: cannot allocate a tzroutine stack ...` を書いて `abort()` | TZ02 |
| `void tsuzuri_tz_yield(void)` | 実行を譲る（全体の run queue の末尾へ）。擬似 G では `sched_yield()` | TZ02 |
| `int64_t tsuzuri_tz_count(void)` | 生きている tzroutine の数 + 1（呼んだスレッドの分。Go の `NumGoroutine` は main を含む） | TZ02 |
| `int64_t tsuzuri_tz_max_procs(void)`・`int64_t tsuzuri_tz_cpu_count(void)` | P の数、オンラインの CPU 数（1 以上） | TZ02 |
| `void *tsuzuri_tz_scope_enter(void)`・`int32_t tsuzuri_tz_scope_leave(void *scope)` | 新しいスコープを呼んだ G（擬似 G を含む）の生成スコープにする／そのスコープの全 tzroutine の終了を待ち、元の生成スコープへ戻す。`leave` は 0、デッドロックの判定で起こされたら 1 | TZ02 |
| `int32_t tsuzuri_tz_sleep(int64_t ns)` | `ns` ナノ秒以上眠る（G は待ち、擬似 G はスレッドを止める）。`ns <= 0` は `yield` と同じ。0 を返す | TZ05 |
| `int32_t tsuzuri_tz_addr_wait(_Atomic uint64_t *addr, uint64_t expected)`・`int64_t tsuzuri_tz_addr_wake(_Atomic uint64_t *addr, int64_t n)` | futex と同じ形の待ち: `*addr == expected` の間だけ待つ／`addr` で待つ者を最大 `n` 人（負なら全員）起こし、起こした数を返す。`wait` は 0 か 2 | TZ06 |
| `int32_t tsuzuri_tz_mutex_lock(_Atomic uint64_t *word)`・`void tsuzuri_tz_mutex_unlock(_Atomic uint64_t *word)` | 3 状態（0 空き・1 保持・2 待ちあり）のロック。`lock` は 0 か 2 | TZ06 |
| `int32_t tsuzuri_tz_waitgroup_add(_Atomic uint64_t *word, int64_t delta)`・`int32_t tsuzuri_tz_waitgroup_wait(_Atomic uint64_t *word)` | 数を足す（負になったら 1 を返し、生成コードがトラップ。0 になったら待つ者を全員起こす）／0 になるまで待つ（0 か 2） | TZ06 |
| `void tsuzuri_tz_chan_init(tzr_chan *c, int64_t elem_size, int64_t capacity)` | 生成コードが確保したブロック（128 バイトのヘッダー + `capacity * elem_size`）を初期化する。参照数 1 | TZ03 |
| `void tsuzuri_tz_chan_share(tzr_chan *c)`・`int32_t tsuzuri_tz_chan_release(tzr_chan *c)` | ハンドルを 1 つ増やす／減らす。最後なら 1（生成コードが `drain` で残りを取り出して drop し、ブロックを解放する） | TZ03 |
| `int32_t tsuzuri_tz_chan_drain(tzr_chan *c, void *out)` | 最後のハンドルを解放した後だけ: 残った要素を 1 つ `out` へ移す。なければ 0 | TZ03 |
| `int32_t tsuzuri_tz_chan_send(tzr_chan *c, const void *elem, int32_t block)` | `elem_size` バイトを渡す。待つ受け手がいれば直接渡し、空きがあれば入れ、なければ `block` なら待つ。閉じていれば `TZR_CLOSED`（生成コードがトラップ）。`block == 0` で入らなければ `TZR_WOULD_BLOCK` | TZ03 |
| `int32_t tsuzuri_tz_chan_recv(tzr_chan *c, void *out, int32_t block)` | 要素を `out` へ移す。閉じて空なら `TZR_CLOSED`（`None`）。`block == 0` で空なら `TZR_WOULD_BLOCK` | TZ03 |
| `int32_t tsuzuri_tz_chan_close(tzr_chan *c)` | 閉じて、待つ受け手と送り手を全員起こす（受け手は `TZR_CLOSED`、送り手も `TZR_CLOSED`＝トラップ）。2 回目は `TZR_CLOSED`（生成コードがトラップ） | TZ03 |
| `int64_t tsuzuri_tz_chan_length(tzr_chan *c)`・`int64_t tsuzuri_tz_chan_capacity(tzr_chan *c)` | 入っている要素の数・容量 | TZ03 |
| `int64_t tsuzuri_tz_select(tzr_scase *cases, int64_t n, int32_t block, int32_t *ok)` | 腕 `cases[0..n)` のうち準備のできたものを一様乱数で 1 つ選んで実行し、添字を返す。どれも準備がなく `block == 0` なら -1（`default`）。デッドロックの判定で起こされたら -2。`*ok` は受信で要素を得たら 1、閉じていたら 0。送信の腕が閉じたチャンネルを選んだら `*ok == 0`（生成コードがトラップ）。`n == 0` かつ `block` なら永久に待つ（判定の対象） | TZ04 |
| `int32_t tsuzuri_tz_stack_guard_hit(uintptr_t address)` | `stack.c` の handler が呼ぶ。この OS スレッドが走らせている G のガードに当たったか（async-signal-safe） | — |

構造体の配置（生成コードと共有。参照実装が `_Static_assert` で固定する）:

| 型 | 大きさ | LLVM の型（TZ03・TZ04 が使う） |
| --- | --- | --- |
| `tzr_chan`（チャンネルのヘッダー） | 128 バイト。要素は 128 バイト目から | `[16 x i64]`（中身は生成コードが読まない。容量は `tsuzuri_tz_chan_capacity`） |
| `tzr_scase`（`select` の腕） | 72 バイト | `{ ptr, ptr, i32, i32, [6 x i64] }`（チャンネル、要素の場所、送信なら 1、詰め物、ランタイムの作業域） |

#### 1.3 スケジューリングの規則

- 新しい G と、起こされた G は、起こした M の P の `runnext` へ入る（先にいた G は環の末尾へ）。M が P を持たない（擬似 G、タイマーのスレッド）ときは全体の run queue へ。どちらのあとも `wakep`。
- M は次の順に G を探す（`tzr_findrunnable`）: P の `schedtick` が 61 の倍数で全体に G があれば全体から 1 つ → `runnext` → 自分の環 → 全体（P の数で割った分を自分の環へ移す） → 盗み（4 巡。乱数で始める P を決め、ほかの P の環の半分。4 巡目は `runnext` も） → 全体を見直し → P を返して眠る。
- spinning（G を探して盗んでいる M）の数 `nmspinning` は、P を持って動いている M の数の半分まで。G を見つけた spinning の M は、自分が最後の spinning なら `wakep` で次の spinning を起こす。spinning をやめた M は、やめた後にもう一度すべての環を見る（見落とした G を拾う）。
- `wakep`: 空いた P があり、spinning の M が 0 なら、1 つの M（眠っている M、なければ新しい M）に P を渡して spinning で起こす。新しい M のスタックは 256 KiB（スケジューラーしか走らない）、detach する。
- 公平さ: 起こされた G は `runnext` に入るので、チャンネルの受け渡しはキャッシュの温かい P で続く。CPU を使い続ける G は、待つか `yield` するまで P を離さない（プリエンプションは TZ10）。

#### 1.4 待ちの規則（付録 D-1）

- **R1** G は、`tzr_park` が g0 で解放するロックを除き、`tzr_lock` を持ったまま切り替えない（待つ前に自分のスタックから降りきるまで、誰も G を起こせない）。
- **R2** スレッドローカルの値は `tzr_this_m()`（`noinline`）でだけ読む。切り替えのあと、G は別の OS スレッドで動いている可能性があるので、TLS のアドレスやそこから読んだ値を `tzr_park`・`tsuzuri_tz_yield` をまたいで持たない。
- **R3** ロックの順序: `tzr_sched.lock` が最も内側。チャンネルのロック（`select` ではアドレスの昇順）、parking lot のバケットのロック、タイマーのロック、スコープのロックを持ったまま `tzr_sched.lock` を取ってよい（`tzr_ready`）。逆はしない。
- **R4** `tzr_park` の解放の関数が解放してよいのは `tzr_lock` だけ（pthread の mutex は、ロックした G と別の文脈（g0）で解放すると、TSan が誤って報告する。付録 A-3）。
- `tzr_park(unlock, arg, reason)`: G を `WAITING` にしてから g0 へ切り替え、g0 が `unlock(arg)` を呼ぶ。擬似 G は `WAITING` にしてから `unlock(arg)`、デッドロックの判定、`tzr_note_sleep`。戻り値は擬似 G がデッドロックの判定で起こされたときだけ 1。
- `tzr_ready(g)`: `WAITING` → `RUNNABLE`（compare-exchange。違えば `Tsuzuri runtime: bad tzroutine status in ready` で abort）。擬似 G は `RUNNING` にして `tzr_note_wake`。

#### 1.5 スコープと終了の待ち（D-61 の決定 1）

- `tsuzuri_tz_scope_enter` は、呼んだ G（擬似 G を含む）の生成スコープを新しいスコープへ替え、元を保存する。`tsuzuri_tz_go` は親の生成スコープで子を数え（`live++`）、子はその数を自分のスコープとして持つ。孫は子の生成スコープ（子が `scope_enter` していなければ子のスコープ）で数えるので、**スコープは子孫すべてを数える**。
- G が終わると（`code(env)` が戻ると）、g0 で `live--` し、0 になったら待っている G を起こす。
- `tsuzuri_tz_scope_leave` は `live == 0` になるまで待ち、元の生成スコープへ戻してスコープを解放する。デッドロックの判定で起こされたら 1 を返し、スコープは解放しない（プロセスはトラップで終わる）。
- 入れ子のスコープ（`scope_enter` を 2 回）を使える（TZ11 の `Routine.scope`）。

#### 1.6 デッドロックの判定（付録 D-3）

- **判定の時点**: M が P を返して眠る直前（`tzr_stopm`）と、擬似 G が待つ直前。どちらも `tzr_sched.lock` を持って `tzr_checkdead` を呼ぶ。
- **条件（すべて成り立つとき）**: 眠っていない M が 0（`mcount - nmidle == 0`）、走っている擬似 G が 0、全体の run queue が空、すべての P の環が空、これから発火するタイマーが 0、生きている tzroutine か待っている擬似 G が 1 つ以上。TZ08 以降は、ブロッキング呼び出しの中の M と、netpoller で待つ G がいれば成り立たない。
- **成り立ったとき**: 1 度だけ（`deadlocked = 1`）、標準エラーへ `Tsuzuri runtime: all tzroutines are asleep - deadlock! (<待っている数> waiting: <理由>, ...)` を書く（理由は待つ G の `wait_reason` の重複を除いた並び。最大 400 文字）。そのあと、待っている擬似 G をすべて `doomed = 1` で起こす（その待ちの関数が `TZR_DEADLOCK` を返し、生成コードが呼び出し地点でトラップする）。待っている擬似 G が 1 つもなければ（ホストのスレッドがいない構成）、その場で `__builtin_trap()`。
- 理由の文（`wait_reason`）: `Channel.send`・`Channel.recv`・`select`・`select (no cases)`・`Mutex lock`・`WaitGroup.wait`・`Time.sleep`・`sync wait`・`end of scope (waiting for tzroutines)`。TZ03 以降が使う API の名前と一致させる。

#### 1.7 スタック（付録 D-7）

- 1 つの G のスタックは 1 つの private な匿名 mapping: 下端の `TZR_GUARD = 128 KiB` を `PROT_NONE`、その上が使える部分（既定 1 MiB。`TSUZURI_TZROUTINE_STACK_KIB` で 64〜1048576 KiB）。OS は触れたページだけを実メモリにする。ガードは `MAX_FRAME_BYTES`（64 KiB）より大きいので、1 つの frame がガードを飛び越えない。
- 終わった G は、スタックを付けたまま P ごとの空きリスト（64 個まで）か全体の空きリスト（1,024 個まで）へ戻し、次の生成で使う。全体へ戻すときは、上端の 16 KiB を除くページを `madvise`（macOS は `MADV_FREE`、Linux は `MADV_DONTNEED`）で OS へ返す。全体も満杯なら `munmap` して G を解放する。
- 新しい G の初期の frame（付録 D-2）: aarch64 は 160 バイトの保存領域の `x30` に開始関数、`x29` に 0。x86-64 は保存領域 + 戻り先、開始関数は `rsp ≡ 8 (mod 16)` で始まる（呼ばれたのと同じ）。
- スタック溢れ: 付録 B-4 のとおり `stack.c` の handler が `tsuzuri_tz_stack_guard_hit` を先に呼び、当たれば既存の文 `trap: stack overflow` を書いて `abort()`。M は `-DTZ_STACK_GUARD` のとき最初に `tsuzuri_stack_thread()` を呼ぶ（sigaltstack）。

#### 1.8 タイマー（`sleep`）

- タイマーの 2 分ヒープ（`tzr_timers`）を `tzr_lock` で守り、1 本のタイマーのスレッドがいちばん早い期限まで眠る。眠るのは別の pthread の mutex と条件変数で、新しいタイマーがいちばん早いときは `kicked = 1` で起こす（起こし損ねない）。
- 期限を過ぎたタイマーは、ヒープから外してから G を起こし、そのあとで `ntimers--`（判定はこの順序に依る）。期限の足し算は `INT64_MAX` で飽和する。時計は `CLOCK_MONOTONIC`。
- タイマーのスレッドは最初の `sleep` で作り、detach する。

#### 1.9 チャンネル（付録 D-4）

- Go の `hchan` と同じ規則。受け手が待っていれば送り手の値を受け手のスロットへ直接書く。バッファに空きがあれば入れる。バッファが満杯で送り手が待っているとき、受け手はバッファの先頭を取り、待っていた送り手の値を末尾へ入れる（順序を保つ）。
- 待つ G の sudog は G のスタックに置く。`tzr_dequeue` は `select` の sudog で、ほかの腕がすでに勝った G（`select_done` の compare-exchange が失敗）を飛ばして捨てる。
- `close` は、待つ受け手と送り手を全員外して `success = 0` で起こす。64 人ずつ、ロックを持ったまま起こしてよい（R3）。

#### 1.10 `select`（付録 D-5）

- **1 段目**: 腕の順序を一様乱数の置換（Fisher–Yates、G ごとの xorshift64* の状態）で決め、チャンネルのアドレスの昇順にすべてのロックを取る（同じチャンネルは 1 回）。乱数の順に、送信は「閉じている（選ぶ。`*ok = 0`）→ 待つ受け手 → 空き」、受信は「待つ送り手 → バッファ → 閉じている（選ぶ。`*ok = 0`）」を見て、最初に準備のできた腕を実行する。
- **`block == 0`** で準備がなければ -1。
- **2 段目**: すべての腕の sudog（`tzr_scase` の作業域）を各チャンネルの待ち行列に入れ、`select_done = 0` で待つ。
- **3 段目**: 起こした者が `g->param` に勝った sudog を入れている。全ロックを取り直し、勝った腕以外の sudog を待ち行列から外して、勝った腕の添字と `success` を返す。
- 腕の数は 65,535 まで（並びを `uint16_t` に置く。TZ04 が `E1017` で上限を検査する）。

#### 1.11 parking lot・Mutex・WaitGroup

- parking lot: 251 個のバケット（アドレス >> 3 の剰余）。`tzr_addr_wait` はバケットのロックの下で `*addr == expected` を確かめてから並び、`tzr_park` がバケットのロックを解放する。`tzr_addr_wake` は同じアドレスの者を先頭から起こす（64 人ずつ）。
- Mutex: 0 から 1 への compare-exchange（速い経路）→ 32 回の再試行 → 2 との交換と `addr_wait(word, 2)` の繰り返し。解放は 0 との交換で、前が 2 なら 1 人起こす。**所有者を記録しない**（入れ子の検査は TZ06 が生成コードで行う）。
- WaitGroup: 語が数。足して負なら 1、0 になったら全員起こす。`wait` は 0 でなければ今の値で `addr_wait`。

#### 1.12 環境変数（起動時に 1 回読む）

| 変数 | 値 | 既定 | 不正な値 |
| --- | --- | --- | --- |
| `TSUZURI_MAXPROCS` | 1〜256 の 10 進 | オンラインの CPU 数（256 で頭打ち、取れなければ 1） | 無視して既定 |
| `TSUZURI_TZROUTINE_STACK_KIB` | 64〜1048576 の 10 進 | 1024 | 無視して既定 |

参照実装（付録 B-1）の `tzr_env` が両方を読む（全体が 10 進の数で範囲内のときだけ。`64k`・空文字・`abc` は既定値）。スタックの大きさはページの倍数に切り上げる。

#### 1.13 資源と失敗

- 失敗は回復しない（D-10）。`calloc`・`mmap`・`pthread_create` の失敗は `Tsuzuri runtime: <理由>` を書いて `abort()`。Linux の `vm.max_map_count`（既定 65,530）は 1 つのスタックで mapping を 2 つ使うので、同時に生きる tzroutine はおよそ 3 万個が上限になる。超えると `cannot allocate a tzroutine stack` で終わる（文書に書く。TZ02）。
- `tzr_report_deadlock` の文の長さは固定の 512 バイトのバッファで打ち切る。

### Phase 2: Windows

- G の文脈は Fiber: 各 M は最初に `ConvertThreadToFiberEx(NULL, FIBER_FLAG_FLOAT_SWITCH)` で g0 を Fiber にし、G は `CreateFiberEx(64 * 1024, reserve, FIBER_FLAG_FLOAT_SWITCH, tzr_fiber_start, g)`（`reserve` は既定 1 MiB）。切り替えは `SwitchToFiber`。終わった G の Fiber は消さずに再利用する（Fiber の中の開始関数がループして次の G の `code` を呼ぶ。付録 C）。Fiber のスタックとガードは OS が管理し、溢れは `EXCEPTION_STACK_OVERFLOW` で、既存の Windows の挙動（理由なしの異常終了）のまま（`stack.c` は Unix だけ）。
- `tzr_lock` は `SRWLOCK`（8 バイト、0 で解放状態。同じスレッドで解放する規則 R1 を満たす）。`tzr_note` は `SRWLOCK` + `CONDITION_VARIABLE`。時計は `QueryPerformanceCounter`。タイマーのスレッドの待ちは `SleepConditionVariableSRW` の相対ミリ秒（切り上げ）。`sysconf` は `GetActiveProcessorCount(ALL_PROCESSOR_GROUPS)`。
- 擬似 G のスレッド終了時の解放は `FlsAlloc` のコールバック（POSIX の `pthread_key_create` の代わり）。
- TSan・ASan の注釈は Windows では使わない（MSVC ABI の clang の TSan はない）。

## 設計

### Phase 1: 変更表

| ファイル | 変更 |
| --- | --- |
| `src/runtime/tzroutine.c`（新規） | 付録 B-1 をそのまま置く（`TZ_STACK_GUARD` のときの `tsuzuri_stack_thread()` とフックの設定を含む） |
| `src/runtime/stack.c` | 付録 B-4: フックの変数 `tsuzuri_stack_guard_hook` の弱い定義と、handler の先頭の呼び出し |
| `tests/tzroutine_runtime.c`（新規） | 付録 B-2（`#include "../src/runtime/tzroutine.c"`） |
| `tests/tzroutine_runtime.mjs`（新規） | 付録 B-3 |
| `.github/workflows/targets.yml` | 既存の「Function values and tasks (PM02)」の手順の後に `node tests/tzroutine_runtime.mjs` の手順を足す（`TSUZURI_TSAN`・`TSUZURI_ASAN` は既存の手順と同じ条件で渡す） |
| `docs/architecture.md` | 新しい節「tzroutine のランタイム（TZ01）」: 付録 D の規則 R1〜R4・構造・判定の条件を要約する |
| `README.md` | 「検証と性能測定」の変更の種類の表に「tzroutine のランタイム → `node tests/tzroutine_runtime.mjs`」の行 |

`scripts/check-runtime-includes.sh` は `include_str!` を数えるので、このチケットでは変わらない（TZ02 が `driver.rs` から `include_str!("runtime/tzroutine.c")` するときに検査の対象になる）。`.gitignore` は `*.ll` だけを無視するので、`.c` の追加は追跡される。

### Phase 1: 参照実装の構成（付録 B-1 の順）

1. 前置き（機能マクロ、`#include`、TSan の検出、アーキテクチャの検査）
2. `tzr_fatal`、`tzr_lock`（macOS は `os_unfair_lock`、Linux は futex の 3 状態、TSan は atomic のスピン）、`tzr_note`、`tzr_nanotime`
3. 文脈切り替え `tzr_switch`（naked）と `tzr_initial_frame`（aarch64・x86-64）
4. 構造（`tzr_g`・`tzr_p`・`tzr_m`・`tzr_sched`）と TLS（`tzr_this_m`）、乱数
5. スタック（`tzr_stack_map`・`tzr_stack_trim`）、`tsuzuri_tz_stack_guard_hit`
6. run queue（`tzr_runqput`・`tzr_runqget`・`tzr_runqsteal`・全体の queue）
7. P と M（`tzr_startm`・`tzr_wakep`・`tzr_stopm`）、デッドロック（`tzr_checkdead`）
8. スケジューリング（`tzr_execute`・`tzr_findrunnable`・`tzr_schedule`・`tzr_mstart`・`tzr_init`）
9. 擬似 G、park／ready／yield、生成と終了、スコープ
10. parking lot・Mutex・WaitGroup、タイマー、チャンネル、`select`

### Phase 2: 変更表

| ファイル | 変更 |
| --- | --- |
| `src/runtime/tzroutine-windows.h`（新規） | 付録 C-1: `tzr_lock`・`tzr_note`・時計・スレッドの生成・Fiber の切り替えの Windows 版 |
| `src/runtime/tzroutine.c` | `#if defined(_WIN32)` で付録 C-2 の差し替え（`tzr_switch` の代わりに `SwitchToFiber`、スタックの mapping の代わりに `CreateFiberEx`、`tzr_pseudo_key` の代わりに `FlsAlloc`） |
| `tests/tzroutine_runtime.c` | `failures` の子プロセスを `CreateProcess` で起こす Windows 版（`fork`／`execl` の代わり）。`overflow` の確認は Windows では「異常終了する」だけを見る |
| `.github/workflows/windows.yml` | x64 のジョブに `node tests/tzroutine_runtime.mjs`（`TSUZURI_CLANG` は既存の MSVC ABI の clang） |

## 他チケットへ提供するインターフェース

- 仕様 1.2 の関数の名前・引数・戻り値・状態の値は固定する（TZ02〜TZ10 がこのまま呼ぶ）。足すのはよいが、変えるときは止めて報告する。
- 構造体の配置（`tzr_chan` 128 バイト、`tzr_scase` 72 バイト、`tzr_sudog` 48 バイト）は `_Static_assert` で固定する。
- 内部の拡張点（後続が足す）: `tzr_checkdead` の条件（TZ08 がブロッキング呼び出しの M と netpoller を足す）、`tzr_findrunnable`（TZ08 が netpoll の非ブロッキングの確認、TZ10 がプリエンプションの要求）、`tzr_timers`（TZ05 がチャンネルへ送るタイマーと停止を足す）。
- **TZ09 への注意**: WASM の版は、同じ `tzroutine.c` を `-DTZR_JSPI` で単一スレッドに組み替える（M は 1 つ、P は 1 つ、`tzr_switch` の代わりに JSPI の Suspending import）。Phase 1 の構造を、その組み替えができる形（OS の呼び出しを小さな関数に閉じ込めた形）のまま保つ。

## 実装手順

### Phase 1

1. **変更**: ブランチ `feat/tz01-tzroutine-runtime` を作る。**内容**: ベースラインの件数を控える。**確認**: `cargo nextest run --locked` の通過件数をメモ。
2. **変更**: `src/runtime/tzroutine.c`・`tests/tzroutine_runtime.c`・`tests/tzroutine_runtime.mjs` を付録 B-1〜B-3 のとおりに作る（コピーする。改行は LF、末尾に改行）。`src/runtime/stack.c` に付録 B-4 の 2 か所を足す。**内容**: 参照実装をそのまま置く。**確認**: `node tests/tzroutine_runtime.mjs` が `tzroutine runtime: 112 runs passed`（Rosetta のない環境は 98）。`TSUZURI_TSAN=1 TSUZURI_ASAN=1 node tests/tzroutine_runtime.mjs` が 136（Linux は 122）。所要は手元の M3 で約 1 分。`cargo build` のあと `node tests/stack_overflow.mjs target/debug/tsuzuri`（既存。`stack.c` の既存の報告）が変わらず通る。
3. **変更**: 手元にない構成の確認。**内容**: `clang --target=x86_64-linux-gnu -fsyntax-only -std=c11 -Wall -Wextra -Werror tests/tzroutine_runtime.c` と `--target=aarch64-linux-gnu` で、Linux の分岐（futex の `tzr_lock`、`MADV_DONTNEED`）が型として通ることを確かめる（ヘッダーがない手元では、`vsc/toolchain` の zig の clang か CI に任せる。G10 の手順と同じ）。**確認**: 警告もエラーもない。出たら S4。
4. **変更**: `targets.yml` に手順を足す（設計の変更表）。`README.md` の表と `docs/architecture.md` の節を書く。**内容**: CI で Linux の x86-64・aarch64 と macOS を確かめる。**確認**: `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`cargo nextest run --locked`（件数が手順 1 と同じ）。PR を作り、`ci:full` のラベルを付ける（`src/runtime/` を変えたため。AGENTS.md の CI）。`gh run watch` で `targets.yml` の新しい手順が全ホストで緑。赤なら S1。
5. **変更**: 完了の記録。**内容**: チケットの末尾に「実装と検証」の節、`_features/README.md` の状態。**確認**: GUIDE §13.2 の書式。

### Phase 2

1. **変更**: `src/runtime/tzroutine-windows.h` と `tzroutine.c` の `_WIN32` の分岐（付録 C）。**内容**: 仕様 Phase 2。`task-windows.h` と同じく、ドライバーが後で 1 つの翻訳単位に入れられるよう、`#include "tzroutine-windows.h"` ではなくヘッダーの中身を `#if defined(_WIN32)` の中で使える形にする（`task_runtime_source_for` と同じ扱いを TZ02 がする）。**確認**: macOS で `clang --target=x86_64-pc-windows-msvc -fsyntax-only -std=c11 -Wall -Wextra -Werror tests/tzroutine_runtime.c` が通る（Windows SDK のヘッダーがない手元では、G10 の手順の `vsc/toolchain` か CI で確かめる）。
2. **変更**: `windows.yml` に手順。**確認**: CI の Windows x64 で `node tests/tzroutine_runtime.mjs` が通る。落ちたら S5。

## テスト計画

| テスト | 内容 | 期待 |
| --- | --- | --- |
| `spawn` | 10 万個の生成と終了の待ち | 数が合う |
| `tree` | 深さ 14 の木（各 G が 2 つ生成。32,767 個） | 数が合う（子孫をスコープが数える） |
| `pingpong` | バッファなしの往復 20 万回（擬似 G と G、G と G） | 値が 1 ずつ増えて戻る |
| `mpmc` | 4 送り手・4 受け手、容量 0・1・64、最後に `close` | 合計が `4 × 50000 × 50001 / 2`。閉じた後の `recv` は `TZR_CLOSED`、`send` も `TZR_CLOSED`、2 回目の `close` は `TZR_CLOSED` |
| `select` | 常に準備のある 2 つのチャンネルを 2 万回 | 両方が 4,000 回以上選ばれる（乱数の公平さ）。準備のない腕だけで `block == 0` は -1。閉じたチャンネルの受信の腕は `*ok == 0` |
| `relay` | `select` の受信と送信の腕を往復 10 万回 | 失われも重複もない |
| `mutex` | 16 個の G × 2 万回、ロックの中で `yield` | 数が合う |
| `waitgroup` | 1,000 個の `done` と 3 人の待ち手 | 待ち手が数 1000 を見る。負になる `add` は 1 |
| `sleep` | 2,000 個が 20 ms | 20 ms 以上、全員が起きる |
| `external` | 4 本の pthread が擬似 G として往復とスコープ | 数が合う |
| `deep` | 約 900 KiB の再帰 | 成功 |
| `fp` | d8〜d15／MXCSR の保存（`yield` をはさむ浮動小数点の計算） | 逐次の結果と bit 単位で一致 |
| `stack_size` | 子プロセス: `TSUZURI_TZROUTINE_STACK_KIB=64` で 100 KiB の再帰、`4096` で 3.5 MiB、既定（1 MiB）で 3.5 MiB、不正な値（`0`・`abc`・`2000000`・`64k`・空）で 900 KiB | 溢れる・成功・溢れる・すべて成功（既定値）。再帰の 1 段の大きさは実行時に測る（コンパイラと sanitizer で違うため） |
| `failures` | 子プロセス: 擬似 G がチャンネルで永久に待つ／スコープで待つ／スタック溢れ | 終了コード 42・43（`TZR_DEADLOCK`／1 を受けた）、溢れは SIGSEGV・SIGBUS か SIGABRT |
| `stack.c` との連結（ドライバー） | `-DTZ_STACK_GUARD` と `src/runtime/stack.c` でビルドし、`child-overflow` を起こす。全ケースも走らせる | 標準エラーに `trap: stack overflow`、SIGABRT。全ケースが通る |

- すべてを `-O0`・`-O3` × `TSUZURI_MAXPROCS` 1・2・4 で実行する。`TSUZURI_TSAN=1`・`TSUZURI_ASAN=1` のときは、子プロセスが信号で終わる `stack_size`・`failures` を除いて各 1 回（P 4）。
- 既存テストへの影響: なし（Rust と既存の runtime を変えない。`stack.c` の変更はフックが空のとき既存と同じ）。
- 性能: 付録 A の表の数を `docs/benchmarks.md` には載せない（TZ02 以降の言語の上の測定が正）。共有 CI に速度の閾値を入れない。

## ドキュメント

- `docs/architecture.md` に節「tzroutine のランタイム（TZ01）」: G・M・P・擬似 G・スコープの関係、規則 R1〜R4、デッドロックの判定の条件、スタックの配置と既定値、環境変数。利用者向けの文書（`_tsuzuri/language-reference/`）は、言語から使えるようになる TZ02 で書く（このチケットでは書かない。利用者から見える変化がないため）。PR の説明に「言語リファレンスの更新なし（利用者から見える変化なし）」と書く。
- `README.md` の検証の表に行を足す。

## 受け入れ条件

- [ ] `src/runtime/tzroutine.c`・`tests/tzroutine_runtime.c`・`tests/tzroutine_runtime.mjs` が付録 B と一致する（手順 3・4 の追加を除く）。
- [ ] `node tests/tzroutine_runtime.mjs` が macOS（手元）と CI の Linux x86-64・aarch64 で通り、`TSUZURI_TSAN=1 TSUZURI_ASAN=1` でも通る。
- [ ] `stack.c` のフックで、tzroutine のスタック溢れが `trap: stack overflow` と報告される。`node tests/stack_overflow.mjs` が変わらず通る。
- [ ] `cargo fmt`・`clippy`・`nextest` が手順 1 と同じ件数で通る。
- [ ] `docs/architecture.md` と `README.md` を更新した。
- [ ] （Phase 2）Windows x64 の CI で同じテストが通る。

## 落とし穴

- **TLS の使い回し（R2）**: clang は、関数の中で一度計算した TLS のアドレスを、呼び出しをまたいで使い回してよいと考える（呼び出しでスレッドが変わらない前提）。`tzr_park` のあとに `tzr_tls_m` を直接読むと、別の M の値を読むことがある。必ず `tzr_this_m()` を呼び直す。`-O0` では出ず `-O3` だけで出るので、両方で試す。
- **pthread の mutex を g0 で解放しない（R4）**: TSan が「別のスレッドが解放した」と誤って報告し、`halt_on_error` で落ちる（付録 A-3 で実際に起きた）。タイマーのヒープが `tzr_lock` で、眠るための mutex が別なのはこのため。
- **fork した子では M が消える**: `fork()` の子にはスケジューラーのスレッドがないので、子でチャンネルを使うと永久に待つ。テストの子プロセスは `fork` + `execl` で自分を起こし直す（付録 B-2 の `run_child`）。
- **`select` の sudog を 2 回外す**: `tzr_dequeue` が飛ばした sudog はすでに待ち行列から外れている。3 段目の `tzr_unlink` は「待ち行列の先頭でも、前があるのでもない」なら何もしない（付録 B-1 の `tzr_unlink`）。この条件を消すと、外れた sudog の `next` で行列を壊す。
- **G の再利用と `allgs`**: 空きリストの G は `allgs` に残る（状態は `DEAD`）。デッドロックの報告は `WAITING` だけを数えるので問題ない。`munmap` する G だけを `allgs` から外す（`all_prev` を保つ）。
- **`tzr_globrunqget` の中の `tzr_runqput` が満杯になると自己デッドロック**: `tzr_runqput_slow` は `tzr_sched.lock` を取る。`tzr_globrunqget` は自分の環が空のとき（または 1 個だけ取るとき）にしか呼ばないので満杯にならない。呼ぶ場所を増やすときは、この条件を保つ。
- **macOS の `os_unfair_lock` はロックしたスレッドでしか解放できない**: g0 は同じ OS スレッドなので R1 の範囲では満たされる。G がロックを持ったまま別の M へ移ることはない（R1）。
- **Linux の mapping の上限**: 1 つの G で 2 つ（ガードと本体）。テストの `spawn` は同時に生きる G が数千なので当たらないが、利用者の 10 万個の同時生成は `vm.max_map_count` に当たる（仕様 1.13）。

## 対象外

- コンパイラへの接続、`tzroutine { }` の構文（TZ02）。チャンネル・`select`・同期の言語の API（TZ03・TZ04・TZ06）。
- ブロッキング呼び出しの P の引き継ぎ、sysmon、netpoller（TZ08）。プリエンプション（TZ10）。WASM（TZ09）。
- スタックの伸長（Go のスタックの複写）: 生成コードはスタック上の値への借用（`ref`）を持つので、スタックを移せない。固定の予約で表す（D2）。
- `GOMAXPROCS` を実行中に変えること（D5）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | stackful の M:N（Go の G・M・P）。stackless の変換（LLVM の coroutine）はしない | 関数に色を付けずに、どこからでも待てる。LLVM の coroutine は待ちうる関数と、関数値を通る呼び出しの全体の変換が要る | 既定案 |
| D2 | スタックは固定の予約（既定 1 MiB）+ 128 KiB のガード。伸長しない | 借用がスタックのアドレスを持つので複写できない。予約は仮想のアドレスだけで、触れたページだけが実メモリ | 既定案 |
| D3 | 文脈切り替えは POSIX では naked 関数の自作（callee-saved だけ）。`ucontext` を使わない | `swapcontext` は信号のマスクを保存するシステムコールで 1 回 µs 単位。macOS では非推奨 | 既定案 |
| D4 | tzroutine でないスレッドは擬似 G（OS スレッドを止めて待つ。P を持たない） | `main` やホストのスタックを別の OS スレッドで動かさない（ホストの TLS・ロックを壊さない） | 既定案 |
| D5 | P の数は起動時に 1 回だけ決める（`TSUZURI_MAXPROCS`）。実行中の変更はしない | Go の `GOMAXPROCS(n)` は世界を止める処理が要る。必要になったら別チケット | 既定案 |
| D6 | デッドロックの判定は待っている擬似 G を `doomed` で起こし、生成コードの呼び出し地点でトラップする | トラップの位置（ソースの行）を報告できる。擬似 G がいなければその場で `__builtin_trap()` | 既定案 |
| D7 | タイマーは 1 つのヒープと 1 本のスレッド | 実装が小さく正しさを確かめやすい。P ごとのタイマー（Go 1.14 以降）は計測で必要と分かったら別チケット | 既定案 |
| D8 | `select` は一様乱数（G ごとの xorshift64*）。再現用の種の環境変数は作らない | Go と同じく、特定の腕の飢餓を防ぐ。テストは不変条件だけを見る | 既定案 |
| D9 | ランタイムのメタデータ（G・スタック・タイマー）はシステムの `malloc`・`mmap` で確保し、Tsuzuri の確保の追跡（`live`）に数えない。チャンネルのブロックは生成コードが `@tz.alloc` で確保する（数える） | 利用者が所有する値だけを `live` で追う（既存の `Channel` のブロックと同じ） | 既定案 |

## 付録

[TZ01-appendix.md](TZ01-appendix.md): A 実測、B 参照実装（`tzroutine.c`・`tzroutine_runtime.c`・`tzroutine_runtime.mjs`・`stack.c` の差分）、C Windows の設計、D アルゴリズムの説明。

## 実装と検証（2026-11-02）

- **範囲**: Phase 1・Phase 2 をすべて実装した（1 つの PR）。着手時に GUIDE・D-61・HEAD（`73d4e881`）と照合し、チケットとの食い違いはなかった（起票後のコミットは文書だけ）。
- **Phase 1**: 付録 B-1〜B-3 を `src/runtime/tzroutine.c`・`tests/tzroutine_runtime.c`・`tests/tzroutine_runtime.mjs` に置き、B-4 の 2 か所を `src/runtime/stack.c` に足した。`node tests/tzroutine_runtime.mjs` は付録どおり `112 runs passed`、`TSUZURI_TSAN=1 TSUZURI_ASAN=1` で `136 runs passed`（macOS 27.2、Apple M3、Homebrew clang 23.1.4、Rosetta の x86-64 を含む）。手順 3 は手元に Linux のヘッダーがないので、配布物の VS Code 拡張に入っている zig 0.16 の `zig cc` で、`x86_64`／`aarch64` の `linux-gnu`・`linux-musl` へ `-std=c11 -Wall -Wextra -Werror -pthread` で通した（警告・誤りなし。gnu は連結まで）。CI の手順は `targets.yml` の「tzroutine runtime (TZ01)」（`TSUZURI_TSAN=1 TSUZURI_ASAN=1` は Linux だけ。macOS の Homebrew LLVM 21 は sanitizer のプログラムが起動しないため、ほかの手順と同じ扱い）。
- **Phase 2**: `src/runtime/tzroutine-windows.h`（新規）に、SRW ロックの `tzr_lock`・`tzr_mutex`、`CONDITION_VARIABLE` の `tzr_note`・`tzr_cond`、`InitOnceExecuteOnce`、`QueryPerformanceCounter` の時計、`CreateThread` の切り離したスレッド、Fiber の切り替えのマクロ `TZR_TO_G`／`TZR_TO_G0` と `CreateFiberEx` を置いた。`tzroutine.c` は付録 C-2 のとおり、切り替えの 5 か所を `TZR_TO_G`／`TZR_TO_G0` にし（POSIX では `tzr_switch` に展開）、G の開始を Fiber の中で繰り返す `tzr_fiber_start`、スタックの mapping を `CreateFiberEx`（終わった G を全体の空きリストにも置けないときは `DeleteFiber`）、擬似 G の解放を `FlsAlloc` のコールバックにした。`tests/tzroutine_runtime.c` の子プロセスは Windows では `CreateProcessA`（`TSUZURI_TZROUTINE_STACK_KIB` は `SetEnvironmentVariableA` で渡して戻す）、`external` は `CreateThread`。溢れの確認は Windows では NTSTATUS の誤りのコード（`STATUS_STACK_OVERFLOW`）で終わることだけを見る。`tests/tzroutine_runtime.mjs` は Windows で `--target=<arch>-pc-windows-msvc -fuse-ld=lld`、`.exe` の名前。`windows.yml` の x64・Arm64 の両方のジョブに「tzroutine runtime (TZ01)」を足した。手元では zig の `x86_64`／`aarch64-windows-gnu`（mingw のヘッダー）で `-Wall -Wextra -Werror` の連結まで通した。実行の確認は CI。
- **チケットから外れた判断**:
  1. POSIX の pthread・`sysconf`・`sched_yield` の呼び出しを、`tzr_mutex_lock`・`tzr_cond_wait_for`・`tzr_run_once`・`tzr_thread_start`・`tzr_os_yield`・`tzr_online_cpus`・`tzr_page_size` の小さな関数に移した（Windows の版を `tzroutine-windows.h` に置くため。TZ09 の注意「OS の呼び出しを小さな関数に閉じ込めた形」とも合う）。規則 R1〜R4・spinning・`select` の 3 段の論理は変えていない。`tzr_m` の使っていない `thread` の欄は消した。
  2. Fiber の commit は、予約が小さいとき（`TSUZURI_TZROUTINE_STACK_KIB=64`）に OS が予約を 1 MiB へ切り上げないよう、`min(予約 / 4, 64 KiB)` にした（仕様の `64 * 1024` は予約が 256 KiB 以上のとき同じ）。
  3. Windows の FLS のコールバックは、プロセスの終了中（`ExitProcess` がほかのスレッドを止めた後）にも走りうるので、`atexit` で立てる印があるときは何も解放しない。COFF では `TZR_API` を空にした（`task.c` と同じ）。
- **見つけた不具合（S3）**: タイマーのスレッドが待つ期限 `CLOCK_REALTIME + (next - now)` が、飽和した `sleep`（`INT64_MAX`）で `int64_t` を溢れていた（未定義動作）。先に `tests/tzroutine_runtime.c` に `long_sleep`（`INT64_MAX` 眠る G の横で短い sleep を 2 回）を足し、参照実装の UBSan の build で `runtime error: signed integer overflow: 1793577017496669000 + 9222983332253943807` で落ちることを確かめてから、1 回の待ちを最大 1 日にする（`tzr_cond_wait_for`。タイマーのスレッドは起きるたびに時計を見直す）ように直した。テストのケースは 15 個になった。
- **検証**: `node tests/tzroutine_runtime.mjs` `112 runs passed`（新しいケースの後は 120）、`TSUZURI_TSAN=1 TSUZURI_ASAN=1` で `146 runs passed`。`cargo nextest run --locked` 2,851 件すべて成功（4 件 skip。Rust は変えていない）。`cargo build` のあと `node tests/stack_overflow.mjs target/debug/tsuzuri` `22 cases`。`sh scripts/check-runtime-includes.sh` `59 files`（`tzroutine.c` はまだ `include_str!` されない）。
- **文書**: `docs/architecture.md` に節「tzroutine のランタイム（TZ01）」、`README.md` の検証の表（変更の種類の表と T2 のコマンドの一覧）。言語リファレンスの更新なし（利用者から見える変化なし。TZ02 で書く）。
- **変更したファイル**: `src/runtime/tzroutine.c`（新規）、`src/runtime/tzroutine-windows.h`（新規）、`src/runtime/stack.c`、`tests/tzroutine_runtime.c`（新規）、`tests/tzroutine_runtime.mjs`（新規）、`.github/workflows/targets.yml`、`.github/workflows/windows.yml`、`docs/architecture.md`、`README.md`、`_features/README.md`、このチケットと付録（`_completed/` へ移した）。
- **レビュー（PR #79）**: Copilot の 11 件をすべて確かめて直した。再現できたものは先に `tests/tzroutine_runtime.c` にケースを足し、直す前に落ちることを確かめた（`fp_mode`・`wake_many`・`detector` の 1 つ目）。(1) `stack.c` のフック `tsuzuri_stack_guard_hook` は `volatile` の非 atomic な関数ポインタで、`tzr_init` の書き込みと別のスレッドの handler の読み出しが競合していた。`_Atomic` にし、release で書いて acquire で読む（lock-free の読み出しは async-signal-safe）。(2) Windows の `tzr_nanotime` が周波数を初回だけ同期なしに書いていた。毎回 `QueryPerformanceFrequency` で読む（`async.c` と同じ）。(3) タイマーのスレッドの待ちが `CLOCK_REALTIME` の絶対時刻で、壁時計を戻すと短い sleep も遅れた。Linux は `pthread_condattr_setclock(CLOCK_MONOTONIC)` で初期化した条件変数、macOS は `pthread_cond_timedwait_relative_np`（Windows はもとから相対）。壁時計を変えるテストは root が要るので足していない。(4) 走っていた最後の擬似 G のスレッドが終わると、待っている者だけが残っても判定が走らず、ほかのホストのスレッドが永久に待った（`detector` の `child-exit-deadlock` で再現。直す前は監視のスレッドの時間切れ）。`tzr_pseudo_destroy` が数を減らした後に `tzr_checkdead` を呼ぶ。(5) Windows の擬似 G を `_Thread_local` で引き、解放は Fiber ごとの FLS で行っていたので、ホストが Fiber を消すと解放済みの擬似 G を引き続き使えた。Windows は引くのも FLS（`FlsGetValue`）にした。(6) `tzr_addr_wake` は有限の数では 64 人で止まり、全員を起こすときは 64 人ごとに再帰していた（`wake_many` で再現。200 人のうち 150 人を起こすと 64）。バケットのロックを持ったまま（R3 で許される）繰り返し、起こした者が同じ起こしに並び直さないようにした。待ち手の記録は待ち手のスタックにあるので、起こす前に外し、起こした後は読まない。(7) `tsuzuri_tz_waitgroup_add` の符号付きの足し算が、負の検査の前に溢れることがあった（`INT64_MAX` の後の 1）。符号なしで足し、`[0, INT64_MAX]` の外は誤り（1）にした（`wake_many` の後半。UBSan の build で確かめる）。(8) 最後のタイマーが起こした G が走って再び待つまでに `ntimers--` が済まないと、判定がタイマーを見て何もせず、その後の減算でも判定が走らなかった。最後のタイマーを外したときに `tzr_sched.lock` の下で判定し直す（`detector` の `child-timer-deadlock`。起こしてから減らす順序は保つ。直す前に落ちる確率は低い）。(9) `select` の 2 つの並びが可変長配列で、65,535 腕では約 256 KiB になり、64 KiB のスタックとガードを飛び越えることがあった。64 腕まではスタックの固定長の配列、それより多ければヒープにする（`detector` の `child-big-select`: 64 KiB のスタックで 40,000 腕）。(10) aarch64 の切り替えが FPCR を保存せず、丸めモードを変えた G が `yield` すると別の G のモードで再開し、新しい G も前のモードを引き継いでいた（`fp_mode` で再現。仕様の「丸めモードと例外マスクを G ごとに保つ」を満たしていなかった）。保存領域を 176 バイトにして FPCR を保存し、違うときだけ書き戻す。新しい G は 0（最近接への丸め、トラップなし）。(11) `tzr_after_exit` が `live` を 0 にしてから `ngoroutines` を減らしていたので、待ちを終えた直後の `tsuzuri_tz_count` が終わった G を数えることがあった。スコープのロックの下で、`live` を減らす前に減らす（`count_after_join`）。検証: `node tests/tzroutine_runtime.mjs` `152 runs passed`（19 ケース）、`TSUZURI_TSAN=1 TSUZURI_ASAN=1` で `186 runs passed`、zig の `x86_64`／`aarch64` の `linux-gnu`・`linux-musl`・`windows-gnu` で `-Wall -Wextra -Werror` が通る。
