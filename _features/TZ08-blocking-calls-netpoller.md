# TZ08: ブロッキング呼び出しの P の引き継ぎ（sysmon）と netpoller

| 項目 | 内容 |
| --- | --- |
| ID | TZ08 |
| 優先度 | P1 |
| 規模 | XL。Phase 1（sysmon と、ブロッキング呼び出しの P の引き継ぎ）L、Phase 2（netpoller: Net の待ちを kqueue／epoll で G だけの待ちにする）L、Phase 3（Windows の netpoller と `Routine.with_locked_thread`）M |
| 依存 | TZ02, TZ05 |
| 後続 | TZ10（sysmon を使うプリエンプション）、TZ13（Net の `_async` の削除。Phase 2 が移行先） |
| 状態 | todo |
| 起票 | 2026-11-01（D-61。HEAD `9a76d8ac` で確認） |
| 承認 | 承認済み（D-61）。決定事項 D1〜D7 は既定案 |
| 手本にする既存実装 | Go の `entersyscall`／`exitsyscall`／`sysmon`／`retake`／`handoffp` の考え方（コードは写さない）。`src/runtime/net.c` の `tz_net_wait`（461 行。すべてのソケットの待ちが通る 1 か所）と、Async 用の poll のスレッド（`tz_net_poller_main` 794 行、起こすための pipe 700〜745 行）。TZ05 の `os.c` のフック（`tsuzuri_os_sleep_hook`。弱い定義の関数ポインター） |
| 主な影響ファイル | `src/runtime/tzroutine.c`・`src/runtime/os.c`・`src/runtime/io.c`・`src/runtime/net.c`・`src/runtime/gpu.c`（呼び出しの印だけ）・`src/llvm.rs`（`extern` の呼び出しの印）・`src/check.rs`（Phase 3 の組み込み）・`tests/tzroutine_runtime.c`・`tests/blocking.rs`（新規）・`tests/fixtures/blocking/Main.tz`（新規）・`tests/net.mjs`・`tests/tzroutine.mjs`・文書 |

## 実装者向け早見

- 範囲: tzroutine が OS を待つ呼び出し（`extern` の関数、ファイル・プロセス・標準入出力・名前解決・ソケット）に入っている間も、ほかの tzroutine が走り続けるようにする。Phase 1 は「呼び出しの間 P を手放す」（M は呼び出しの中で止まってよい）。Phase 2 は、ソケットの待ちを M ごと止めずに G だけの待ちにする（netpoller）。
- 最初に読む: ① 仕様 Phase 1 の 1〜3 ② TZ01 の付録 D-1・D-3・D-6 ③ `src/runtime/net.c` の 450〜480 行と 680〜910 行 ④ TZ05 の `os.c` のフックの設計。
- 最初に書くテスト: `tests/tzroutine_runtime.c` の `blocking_handoff`（`TSUZURI_MAXPROCS=1` で、1 つの G が 200 ms の `usleep` を「ブロッキング」として包んでいる間に、ほかの G が 1,000 回の往復を終える）。
- 触ってはいけない: tzroutine を使わないプログラムの IR（`extern` の呼び出しの印は `uses_tzroutine` のときだけ）。Net・File・Process の API と結果。Async の Net の経路（TZ13 まで）。
- 完了の見分け方: Phase 1 は上のテストと、E2E の `extern_sleep_does_not_stall`。Phase 2 は、`TSUZURI_MAXPROCS=1` のエコーサーバーが 100 本の接続を同時に扱える（各接続の tzroutine が `Net.read` で待つ）。

## 目的

TZ02 の時点では、tzroutine が `extern` の関数やファイルの読み書きで OS を待つと、その M が P を持ったまま止まり、`TSUZURI_MAXPROCS` が 1 なら全員が止まる。Go は、システムコールに入った M から P を取り上げて別の M に渡し（sysmon の retake）、ソケットの待ちは netpoller で G だけを止める。同じことをして、サーバー（接続ごとの tzroutine）を書けるようにする。

## 着手条件と停止条件

### 着手条件

- TZ02・TZ05 が done。Phase 2 の前に `node tests/net.mjs target/debug/tsuzuri` が通る（基準）。

### 停止条件

- **S1** P の引き継ぎで、デッドロックの判定が誤って成り立つ（呼び出しの中の M を数え損ねる）か、成り立たなくなる（呼び出しから戻った後の数え戻し忘れ）。TZ01 の C のテスト `failures` が通らなくなったら止める。
- **S2**（Phase 2）`tests/net.mjs` の既存のケース（tzroutine を使わない Net）の結果が変わる。
- **S3**（Phase 2）kqueue・epoll の版の違い（`EPOLLEXCLUSIVE` の有無など）で、Linux の CI の一部のホストだけが落ちる。選択肢: A 使う機能を最小にする（`EPOLLONESHOT` と `EPOLLIN`／`EPOLLOUT` だけ）、B 止めて報告する。
- **S4** `extern` の呼び出しの印で、既存の FFI のテスト（`tests/ffi*.rs`・`tests/bindings*.mjs`）の IR の期待（tzroutine を使わないもの）が変わる。
- **S5** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `9a76d8ac` で確認）

- TZ02 以降、tzroutine の中のブロッキング呼び出しは、その M と P を止める。TZ05 の `Time.sleep_ms` だけはフックで G の待ちになる。
- ソケットの待ちは、すべて `net.c` の `tz_net_wait(descriptor, events, deadline)`（1 本の fd の `poll`／`WSAPoll`）を通る。Async の `_async` 関数は、別の poll のスレッド（`tz_net_poller_main`）と `tsuzuri_async_post` で完了を届ける（TZ13 で消える）。
- `os.c`・`io.c` の公開関数はそれぞれシステムコールを直接呼ぶ（`tsuzuri_os_read`・`write`・`open`・`handle`・`close`・`spawn`・`random`、`tsuzuri_io_read_line`・`write`）。
- 生成コードの `extern def` の呼び出しは、`tsuzuri_host_<モジュール>_<名前>` かリンク名への普通の `call`。

## 変えてはいけない意味

- 呼び出しの結果・誤りの値・時間切れの意味（Net の `timeout`）。
- tzroutine を使わないプログラムの IR と挙動。
- 擬似 G（`main` のスレッド）の呼び出しは今までどおり、その OS スレッドで待つ（P を持たないので引き継ぎは要らない）。

## Phase 分割

| Phase | 内容 |
| --- | --- |
| 1 | sysmon、`tsuzuri_tz_blocking_enter`／`exit`、`os.c`・`io.c`・`net.c`・`gpu.c` の印、`extern` の呼び出しの印 |
| 2 | netpoller（macOS・BSD は kqueue、Linux は epoll）。`tz_net_wait` を G の待ちにする |
| 3 | Windows の netpoller（`WSAPoll` のスレッド）と `Routine.with_locked_thread` |

## 仕様

### Phase 1

#### 1. 印の付いた呼び出し

- 次の呼び出しは「ブロッキング」として扱う: `extern def` の関数（すべて。D2）、`os.c` の公開関数のうち `read`・`write`・`open`・`handle`・`close`・`spawn`・`random`・`sleep`（tzroutine の外）、`io.c` の `read_line`・`write`、`net.c` の `resolve`・`open`・`accept`・`read`・`write`・`send`・`close`（Phase 2 で待ちの部分は netpoller へ）、`gpu.c` の装置との同期の呼び出し。
- 呼び出しの間、その G を走らせている M は P を **手放せる状態** になる（`P.status = SYSCALL`）。sysmon が、同じ呼び出しが 20 µs 以上続いたのを見つけたら P を取り上げ、仕事があれば別の M に渡す。
- 呼び出しから戻った M は、P を取り戻せればそのまま続け、取り戻せなければ空いた P を探し、なければ G を全体の run queue に置いて M は眠る（G はあとで別の M で続く）。
- 擬似 G の呼び出しは何もしない（P を持たない）。

#### 2. sysmon

- 最初の `tsuzuri_tz_go` で起動する 1 本のスレッド（detach）。眠る時間は 20 µs から始め、何もしなかった回が 50 回続いたら倍にし（上限 10 ms）、何かしたら 20 µs に戻す（Go の sysmon と同じ間隔の考え方）。
- 毎回: 各 P について、状態が `SYSCALL` で、前回見たときと `syscall_tick`（呼び出しのたびに増える数）が同じなら、`SYSCALL` → `IDLE` の compare-exchange に勝ったときに引き継ぐ: 全体か、どれかの P の環に G があれば `tzr_startm(p, 0)`、なければ `tzr_pidleput(p)`（`tzr_sched.lock` の下）。
- すべての M と擬似 G が眠り、G も待ちもないとき（`ngoroutines == 0`）、sysmon は条件変数で眠る（CPU を使わない）。次の `tsuzuri_tz_go` が起こす。

#### 3. デッドロックの判定

- ブロッキング呼び出しの中の M の数 `nmsys` を数え、`tzr_checkdead` の `running` に足す（呼び出しから戻る M が誰かを起こしうるので、判定しない）。

### Phase 2

#### 4. netpoller

- macOS（と FreeBSD）は `kqueue`（`EVFILT_READ`／`EVFILT_WRITE`、`EV_ADD | EV_ONESHOT`、起こすための `EVFILT_USER`）、Linux は `epoll`（`EPOLLIN`／`EPOLLOUT` + `EPOLLONESHOT`、起こすための `eventfd`）。1 本の netpoller のスレッドが `kevent`／`epoll_wait` で無期限に待つ。
- `tz_net_wait(fd, events, deadline)` が、tzroutine の G の上で呼ばれたら（フック `tsuzuri_net_wait_hook`。TZ05 の `os.c` のフックと同じ弱い定義の関数ポインター）、ランタイムの `tzr_netpoll_wait(fd, events, deadline)` を呼ぶ: 待ちの記録 `{ g, fd, state }`（G のスタック）を作り、fd を `ONESHOT` で登録し、期限があれば TZ05 のタイマー（種類 `NETWAIT`）を作り、G を待たせる（理由 `Net wait`）。準備ができたか時間切れで、記録の `state` を compare-exchange で `READY`／`TIMEDOUT` にした者が G を起こす（もう一方は何もしない）。戻り値は `tz_net_wait` と同じ（0・`TZ_NET_E_TIMEDOUT`）。
- 同じ fd を 2 つの G が同時に待つ（読みと書き）ことはできる。同じ fd・同じ向きを 2 つの G が待つ使い方は、Net の API では起きない（1 つのハンドルは 1 つの所有者）。
- `Net.close` が、待っている G のいる fd を閉じると、その G は準備ができたものとして起きる（次のシステムコールが誤りを返す。今の `tz_net_wait` の「閉じれば待ちが終わる」と同じ）。
- デッドロックの判定: netpoller で待つ G の数 `nnetwait` を、判定の `running` に足す（外からの入力が誰かを起こしうる）。

### Phase 3

#### 5. Windows

- netpoller は、`WSAPoll` を使う 1 本のスレッドと、自分宛ての UDP のソケットで起こす（`net.c` の Async 用のスレッドと同じ方法）。IOCP は使わない（D6）。

#### 6. `Routine.with_locked_thread`

- `with_locked_thread :: (unit -> 'a) -> 'a`: 関数の間、呼んだ tzroutine をその M に固定する（ほかの M へ移らない。G が待つと M も眠り、ほかの G を走らせない）。スレッドに結び付いたホストの API（GUI、OpenGL の文脈、スレッドローカルを使う C のライブラリ）を呼ぶため（Go の `runtime.LockOSThread`）。擬似 G から呼ぶと何もしない（もともと固定）。

## 設計

### ランタイム（Phase 1）

- `tzr_p` に `_Atomic int status`（`TZR_P_IDLE`・`RUNNING`・`SYSCALL`）と `_Atomic uint32_t syscall_tick`、`tzr_m` に `tzr_p *oldp`、`tzr_sched` に `_Atomic int nmsys`。
- `TZR_API void tsuzuri_tz_blocking_enter(void)`: `m = tzr_this_m()`。`m == NULL || m->curg == NULL` なら何もしない（擬似 G）。`p = m->p`、`m->oldp = p`、`p->syscall_tick++`、`atomic_store(&p->status, SYSCALL)`、`nmsys++`。**ロックを取らない**（速い経路）。
- `TZR_API void tsuzuri_tz_blocking_exit(void)`: 擬似 G なら何もしない。`nmsys--`。`SYSCALL` → `RUNNING` の compare-exchange に勝てば終わり。負けたら（sysmon が取り上げた）: `m->p = NULL`。`tzr_sched.lock` の下で `tzr_pidleget()` が取れれば `m->p` にして `RUNNING` で終わり。取れなければ、`m->after = tzr_after_exitsyscall` で g0 へ切り替える。`tzr_after_exitsyscall`（g0）: G を `RUNNABLE` にして全体の run queue へ、`tzr_wakep()`、`tzr_sched.lock` を取って `tzr_stopm(m)`（P をもらうまで眠る）。`tzr_execute` の後の `tzr_schedule` のループは、戻ったとき `m->p` があるので続けられる。
- `tzr_findrunnable` が P を返すとき（`tzr_pidleput`）、`status = IDLE`。`tzr_startm`・`tzr_stopm` で P を受け取った M は `status = RUNNING`。
- sysmon のスレッドは仕様 2。`tzr_sched.lock` を取るのは引き継ぎのときだけ。
- フック: `os.c`・`io.c`・`net.c`・`gpu.c` に、弱い定義の 2 つの関数ポインター `tsuzuri_blocking_enter_hook`・`tsuzuri_blocking_exit_hook`（`= 0`）と、マクロ `TZ_BLOCKING_ENTER()`／`TZ_BLOCKING_EXIT()`（非 0 なら呼ぶ）を置き、仕様 1 の各関数のシステムコールを囲む。`tzroutine.c` は `tzr_init` でフックを設定する。複数の C で同じ弱い定義を書くので、定義は 1 つのヘッダー片（`tz_hooks` の定義を各 C の先頭にコピーする。翻訳単位は 1 つに連結されるので、`#ifndef TZ_BLOCKING_HOOKS` で 1 回だけにする）。
- `extern` の呼び出し（`src/llvm.rs`）: `globals.tzroutine`（TZ06 の `uses_tzroutine`）のときだけ、`extern def` の関数の呼び出しを `call void @tsuzuri_tz_blocking_enter()` と `call void @tsuzuri_tz_blocking_exit()` で囲む。`extern` の関数が Tsuzuri のコールバック（E12）を呼び返すことがあるので、コールバックの入口（`tz.callback.*` の関数）では `tsuzuri_tz_blocking_exit` 相当（P を取り戻す）をし、戻るときに `enter` 相当をする（`callback_enter`／`callback_exit` の 2 つの関数を足す）。

### ランタイム（Phase 2）

- `tzr_netpoll` の節: `tzr_netpoll_init`（最初の待ちで `kqueue()`／`epoll_create1(EPOLL_CLOEXEC)` と起こす fd を作り、スレッドを起動）、`tzr_netpoll_wait(fd, events, deadline)`、スレッドの本体（`kevent`／`epoll_wait` → 各イベントの `udata` の記録の `state` を `READY` に CAS → `tzr_ready(g)`）。
- 記録の `state`: `WAITING` → `READY`（netpoller）か `TIMEDOUT`（タイマー）。負けた側は何もしない。G は起きたあと、時間切れなら fd の登録を `EV_DELETE`／`EPOLL_CTL_DEL` で外す（`ONESHOT` で外れていなければ）。
- `tzr_sched` に `_Atomic int nnetwait`（待ちの数）。`tzr_checkdead` に足す。

### 生成と組み込み（Phase 3）

- `Builtin::RoutineWithLockedThread`（`(unit -> 'a) -> 'a`）。ランタイムの `tsuzuri_tz_lock_thread()`／`unlock_thread()` を呼び、その間にコールバックを呼ぶ。`tzr_g` に `locked_m`。`tzr_ready` は `locked_m` のある G を、その M の専用の欄へ入れて M を起こす。M は `locked_m` の G が待つ間、`tzr_note` で眠る（`tzr_findrunnable` を回さない）。

## 前提とする他チケットのインターフェース

- TZ01 の M・P・`tzr_stopm`・`tzr_startm`・`tzr_checkdead`。TZ05 のタイマー（`NETWAIT` の種類）と `os.c` のフックの書き方。TZ06 の `uses_tzroutine`。

## 他チケットへ提供するインターフェース

- sysmon（TZ10 が、長く走る G の検出に使う）。`tsuzuri_tz_blocking_enter`／`exit`（TZ09 の WASM では何もしない版）。netpoller（TZ13 で Net の `_async` を消した後、Net の非同期の待ちはすべてここ）。

## 実装手順

### Phase 1

1. **変更**: ブランチ `feat/tz08-blocking-calls`。基準。
2. **変更**: ランタイム（P の状態、enter／exit、sysmon、`nmsys`）と C のテスト `blocking_handoff`（早見）・`blocking_reacquire`（戻ったとき P が取り上げられていても G が続く。`TSUZURI_MAXPROCS=1` で 2 つの G が交互に 50 ms のブロッキングをして、合計の時間が 50 ms × 回数より大きく短い、ではなく、もう一方の G の往復が進むことを順序で確かめる）・`blocking_deadlock`（ブロッキング中は判定しない）。**確認**: `TSUZURI_TSAN=1 TSUZURI_ASAN=1 node tests/tzroutine_runtime.mjs`。
3. **変更**: C のフック（`os.c`・`io.c`・`net.c`・`gpu.c`）。**確認**: `node tests/os.mjs`・`node tests/net.mjs`・`node tests/io.mjs`（あれば）が変わらず通る。
4. **変更**: `extern` の呼び出しの印とコールバックの入口（`src/llvm.rs`）。**確認**: `tests/blocking.rs`（tzroutine を使わないプログラムの `extern` の IR が変わらない。使うプログラムは enter／exit が対になって入る）。既存の FFI のテスト（S4）。
5. **変更**: E2E（`tests/fixtures/blocking`）。**確認**: `node tests/tzroutine.mjs target/debug/tsuzuri`。
6. **変更**: 文書、T2、CI（`ci:full`）。

### Phase 2

1. **変更**: netpoller（kqueue・epoll）と C のテスト `netpoll_pipe`（pipe の読み手の G 100 個が、書き手が書いた順に起きる）・`netpoll_timeout`・`netpoll_close`。
2. **変更**: `net.c` の `tz_net_wait` のフック。**確認**: `node tests/net.mjs`（既存）と、新しい E2E `echo_server`（`TSUZURI_MAXPROCS=1` で、接続ごとの tzroutine が 100 本同時に読み書き）。
3. **変更**: 文書（`net.md` の tzroutine の節）、CI。

### Phase 3

- Windows の netpoller、`with_locked_thread`。`windows.yml` で確かめる。

## テスト計画

| E2E | 内容 | 期待 |
| --- | --- | --- |
| `extern_sleep_does_not_stall` | `extern def host_sleep_ms :: i64 -> unit`（テストのホストの C が `usleep`）を 1 つの tzroutine が呼ぶ間、`TSUZURI_MAXPROCS=1` でほかの tzroutine が 1,000 回往復 | 往復が先に終わる（受け取る順） |
| `file_read_in_routines` | 8 つの tzroutine が別々のファイルを読む | 内容の長さの和 |
| `callback_from_extern` | tzroutine の中の `extern` がコールバックで Tsuzuri を呼び返し、その中でチャンネルへ送る | 受け取れる |
| `echo_server`（Phase 2） | `Net.bind` + `accept` のループ、接続ごとの tzroutine、100 本のクライアントの tzroutine | 全員がエコーを受ける、`TSUZURI_MAXPROCS=1` でも |
| `read_timeout`（Phase 2） | 書き手のいない接続の `Net.read` に 50 ms の時間切れ | `TimedOut`、ほかの tzroutine は進む |

## ドキュメント

- `async-tasks-and-lazy/tzroutine.md`: 「OS を待つ呼び出し」の節（P の引き継ぎ、Phase 2 の後はソケットの待ちが G だけの待ち、`extern` の呼び出しの費用、コールバック）。
- `built-in-types-and-modules/net.md`: tzroutine の中の Net（Phase 2）。`compiler/native-interop.md`: `extern` の呼び出しが tzroutine の中でどう扱われるか、スレッドに結び付く API と `with_locked_thread`（Phase 3）。`built-in-types-and-modules/routine.md`（`with_locked_thread`）。
- `docs/architecture.md`（sysmon・netpoller）、`docs/language.md`。
- `spec/rules/TSK.md`: `TSK-031`（A tzroutine blocked in an `extern` call MUST NOT stop the other tzroutines from running.）。

## 受け入れ条件

- [ ] 各 Phase の仕様のとおりに動く。
- [ ] tzroutine を使わないプログラムの IR・Net・File の挙動が変わらない。
- [ ] C のテストが TSan・ASan で通る。
- [ ] 文書・規則 ID を更新した。

## 落とし穴

- **enter／exit の間に G が移らない**: ブロッキング呼び出しの中では G は切り替わらないので、enter の M と exit の M は同じ。exit で P を取り戻せないとき初めて G が移る（R2: exit の後の TLS は読み直す）。
- **コールバックの入れ子**: `extern` → コールバック → `extern` と入れ子になる。enter／exit を数で数える（`m->blocking_depth`）。0 → 1 で `SYSCALL`、1 → 0 で戻す。
- **sysmon が P を取り上げた直後の exit**: compare-exchange の負けを必ず扱う（設計の exit）。
- **`EPOLLONESHOT` の再登録**: 次の待ちで `EPOLL_CTL_MOD` を使う（`ADD` は 2 回目に `EEXIST`）。fd ごとに「登録済み」を覚えるか、`ADD` の `EEXIST` で `MOD` に切り替える。
- **sysmon の CPU**: 何もない時に回り続けない（仕様 2 の眠り）。CI の時間の検査は入れないが、`top` で 0 % 近いことを手元で確かめる。

## 対象外

- ファイルの非同期 I/O（io_uring、IOCP のファイル）。ファイルは Phase 1 の引き継ぎで足りる（Go も同じ）。
- `extern` を「ブロッキングでない」と印を付けて費用を省く属性（D2）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | Go と同じく、ブロッキング呼び出しの間 P を手放せる状態にし、sysmon が 20 µs で取り上げる | 短い呼び出しは費用 0 に近く、長い呼び出しだけ引き継ぐ | 既定案 |
| D2 | `extern` の呼び出しはすべてブロッキングとして印を付ける（属性で省く手段は作らない） | どの `extern` が待つかを型から知れない。印の費用は 2 回の atomic の書き込み | 既定案 |
| D3 | 印は C のランタイムのフック（弱い定義）で付け、IR は `extern` の呼び出しだけを変える | os.c などの関数の IR の呼び出し側を変えない | 既定案 |
| D4 | netpoller は 1 本のスレッド（Go のように `findrunnable` へ統合しない） | 実装が小さい。統合は計測で必要と分かったら | 既定案 |
| D5 | Linux は epoll、macOS は kqueue。`poll` の版は作らない | `poll` は接続数に比例する | 既定案 |
| D6 | Windows は `WSAPoll` のスレッド（IOCP は使わない） | `net.c` の既存の方法と同じ。IOCP は別チケット | 既定案 |
| D7 | `with_locked_thread` は関数の範囲で固定する（解除を別の呼び出しにしない） | 所有権の範囲で必ず解除される | 既定案 |
