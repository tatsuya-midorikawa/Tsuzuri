# M09: メインスレッドへの投入（`MainThread`）とメインスレッドに結び付くハンドル（`extern type main`・`extern class main`）

| 項目 | 内容 |
| --- | --- |
| ID | M09 |
| 優先度 | P2（回避策はある: `dlsym` で `_dispatch_main_q` を引いて `dispatch_async_f` を呼ぶ。型の保護はなく、ABI の知識に頼る） |
| 規模 | L（Phase 1〜3。人間が求めない限り Phase 1 だけを実装する） |
| 依存 | ([M01](_completed/M01-objc-class-types.md)), ([M08](_completed/M08-macos-main-thread-docs-and-e1013.md))（M01 は Phase 2 の `extern class main` の段だけ） |
| 後続 | [M14](M14-tls-http-packages.md)（弱い後続: [M03](M03-objc-classes-defined-in-tsuzuri.md), [M04](M04-objc-blocks.md), [M07](M07-apple-sdk-bindgen-and-package.md), [M13](M13-desktop-os-services-package.md)） |
| 状態 | todo |
| 起票 | 2026-11-26（D-67。利用者の依頼「「どのチケットにもない壁」と「調査中に見つけたコンパイラの不具合（自分で再現済み）」のそれぞれについて @_features/ に開発チケットまたは修正チケットを作成して」から起票。調査で報告した「壁 3（メインスレッドへ戻す API がなく、メインスレッド専用を型で強制できない。run loop と tzroutine の待ち、terminate: の exit、GCD のスレッドからの Anchor の WrongThread、Time.after_func がワーカーで発火）」に当たる。HEAD `804b7970` で確認） |
| 承認 | 要承認: Phase 1 は D-55（組み込みのモジュール `MainThread`）。Phase 2 は D-55（`extern type`・`extern class` の後の文脈キーワード `main`）と D-53 の改訂（効果 `main_thread`。語彙が 15 から 16）。Phase 3（別のスレッドから引ける預け先）は別に承認する。決定事項の D1・D11・D12・D16。D-67 の分岐 1 には依らない。分岐 1 は X′ で承認済み（2026-11-26）なので、Phase 2 の `extern class main` の段も M01 の後に実装する。台帳は [GUIDE D-67](GUIDE.md#d-67-tsuzuri-だけで-macos-のデスクトップアプリを書くm01m22) |
| 改善する劣位 | Swift の `DispatchQueue.main.async`・`@MainActor`、Kotlin の `Dispatchers.Main` 比: tzroutine や GCD のスレッドから UI のスレッドへ処理を戻す API がない。メインスレッドでしか呼べない API を型で表せず、`Routine.with_locked_thread` の誤用が実行時の例外になる（M08 の「現状」4） |
| 手本にする既存実装 | TZ05 の `Time.after_func`（`src/llvm_timer.rs` の `Builtin::TimeAfterFunc` の腕、`@tz.tzroutine.owned`・`@tz.tzroutine.owned.drop`、境界で呼ばずに drop する規則）、W19（`HandleKind::Local`、`src/pinned_rules.rs` の `builtin_position`・`visit_spawned_arg`、`src/effects.rs` の `pinning_argument_mask`、`E1013` の 2 つの kind）、W12（`src/llvm.rs` の `with_blocking_externs`、`@tz.callback.after_main`）、driver の `os_runtime`・`net_runtime` と `PlatformUnsupportedApi` の判定 |
| 主な影響ファイル | Phase 1: `src/check.rs`、`src/stdlib.rs`、`src/effects.rs`、`src/pure_rules.rs`、`src/kg/effects.rs`、`src/pinned_rules.rs`、`src/llvm.rs`、`src/llvm_mainthread.rs`（新規）、`src/driver.rs`、`src/diagnostic_kinds.rs`、`src/runtime/mainthread-darwin.c`（新規）、`tests/main_thread.rs`（新規）、`tests/main_thread.mjs`（新規）、`tests/fixtures/main_thread/Main.tz`（新規）、`_tsuzuri/language-reference/built-in-types-and-modules/mainthread.md`（新規）、`_tsuzuri/language-reference/built-in-types-and-modules/routine.md`、`_tsuzuri/language-reference/compiler/native-interop.md`、`_tsuzuri/language-reference/index.md`、`docs/language.md`、`.github/workflows/targets.yml`、`README.md`。Phase 2: `src/syntax.rs`、`src/parser.rs`、`src/syntax_codec.rs`、`src/frontend_cache.rs`、`src/llvm_abi.rs`、`tests/thread_bound_handles.rs`、`tests/fixtures/main_thread/host.c`（新規） |

## 実装者向け早見

- 範囲（Phase 1）: 組み込みのモジュール `MainThread`（`post`・`after`・`is_current`）。macOS ではランタイムの C が、所有する関数をメインのディスパッチキューに積み、メインスレッドで 1 回呼んで drop する。境界の終わりに残った投入は呼ばずに drop する。ほかの native は `E2002`、wasm は `E2000`。型の検査は足さない。Phase 2（要承認）は `extern type main`・`extern class main` と効果 `main_thread`、Phase 3（要承認）はどのスレッドからでも引ける預け先。
- 人間が求めない限り Phase 1 だけを実装する。承認が記録されるまで、該当する Phase に着手しない。
- 最初に読む: 「現状」3〜6 → `src/llvm_timer.rs` の `TimeAfterFunc` の腕と `timer_new` → `src/pinned_rules.rs` 33〜56 行と 672〜708 行 → `src/driver.rs` 4250〜4320 行・4440〜4465 行・4890 行。
- 最初に書くテスト: `tests/main_thread.rs` の型と予約（テスト計画の r1・r4）、次に `tests/main_thread.mjs` の `runloop`。
- 触ってはいけない: `MainThread` を使わないプログラムの IR・object・header・診断・既定の `.wasm`、prebuilt のランタイムの単位の文字列、`Time.after_func`・`Routine.go` の意味、`local` だけのプログラムの W19 の判定。
- 完了の見分け方（Phase 1）: 仕様の「Phase 1: 例」が、macOS の `-O0`・`-O3`、`TSUZURI_MAXPROCS=1`・`4` で `posted on main: true` と `after on main: true` を出し、終了コード 0 で終わる。

## 目的

Tsuzuri の `main` と、AppKit のデリゲート・`NSTimer`・メインのディスパッチキューから呼ばれるコールバックは、プロセスのメインスレッドで走る。重い処理を tzroutine で行い、結果を UI へ戻すのが、どの UI ツールキットでも基本の形である。ところが、tzroutine や GCD のワーカーから処理をメインスレッドへ戻す API がない。今は `dlsym` で `_dispatch_main_q` を引いて `dispatch_async_f` を呼び、状態を整数の鍵（`Anchor` の token）で渡すしかない。`Time.after_func` はワーカーで発火するので、UI を直接は駆動できない。

もう 1 つの問題は、メインスレッドでしか呼べない API を型で表せないことである。W19 の `local` と `with_locked_thread` が保証するのは「どこかの固定したスレッド」までで、tzroutine の中の `with_locked_thread` で AppKit を呼ぶコードは型検査を通り、実行時に例外になる。

Phase 1 は戻す API を足す。M08 が文書に置いた「計画は M09」の行き先で、効果の語彙（D-53）を変えない。Phase 2 は、メインスレッドに結び付くハンドルと効果 `main_thread` で、誤ったスレッドでの使用を `E1013` にする。Phase 3 は、ランタイムに参加していないスレッドから tzroutine へ値を渡す預け先で、M14 が要る。

## 着手条件と停止条件

### 着手条件

- 人間が求めない限り Phase 1 だけを実装する（GUIDE §0）。承認が記録されるまで、該当する Phase に着手しない。
- Phase 1: D1（D-55 の `MainThread`）が承認済み。[M08](_completed/M08-macos-main-thread-docs-and-e1013.md) が done であることが望ましい（M08 の注記を置き換える。M08 が後なら、M08 が文書に API の説明を書く）。確認: `grep -nE '^\| M0[89] ' _features/README.md`。
- Phase 2: D11・D12 が承認済み。`extern class main` の段は [M01](_completed/M01-objc-class-types.md) が done のときだけ。Phase 3: D16 が承認済み（M14 の着手より前に決める）。
- 共通: `node scripts/repo-kb.mjs brief M09`。P2 なので、着手前にその時点のコードと台帳でこのチケットをレビューする（GUIDE §0 の 2）。macOS（arm64）の開発機。ベースライン（`tests/fixtures/*`・`examples/*` の `--emit llvm`（native と `--target wasm32`）・`--emit header`・`check --json`）を `target/scratch/m09/before/` に保存する。

### 停止条件

- **S1** `MainThread` を使わないプログラムの IR・object・header・診断・既定の `.wasm` が 1 byte でも変わる。
- **S2** `MainThread` を使わないプログラムの C の単位の文字列が変わる（`src/runtime_cache.rs` の prebuilt の照合が外れる）。
- **S3** 投入した関数を呼ぶために、`main` の擬似 G・境界・D-61 の待ちの意味を変える必要が出た（D8）。
- **S4** `<dispatch/dispatch.h>` も自前の宣言（設計の「Phase 1: ランタイム」）も、Apple の ld か同梱の zig（lld）でリンクできない。
- **S5** Phase 2 で `EffectSet`（`u16`）に 17 個目のビットが要る、または `local` だけのプログラムの判定が変わる。
- **S6** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `804b7970` で確認）

作業は `target/scratch/m09/` で行い、確認後に消した。macOS 27.2.1 arm64、`target/debug/tsuzuri`（HEAD の dev のバイナリ）。

### 1. API がない

- `rg -nw 'MainThread' src std _tsuzuri` は 0 件（`-w` がないと `src/runtime/wasm-threads.mjs` の Node.js の `isMainThread` に当たる）。`RESERVED_MODULES`（`src/stdlib.rs` 82 行から）は 78 個（903 行で固定）で、`MainThread` を含まない。利用者の `MainThread.tz`（`def post :: unit -> i64 = \() -> 1`）を `MainThread.post ()` で呼ぶプログラムは、今は `check` が通る。`docs/stability.md` 123 行は、std に足したモジュールの名前は予約され、同じ名前の利用者のモジュールは `E1011` になると書く。
- `Routine` の API（`routine.md` 17〜25 行）に、メインスレッドで実行する関数はない。

### 2. スレッドの事実

- `main` は OS のメインスレッドの擬似 G（`src/llvm.rs` 9994〜10006 行）。tzroutine と、その中の `with_locked_thread` はワーカー M で走る（`src/runtime/tzroutine.c` 2196〜2202 行。M08 の「現状」1）。
- `tsuzuri_tz_blocking_enter`（2125〜2129 行）は M のない擬似 G では何もしない。`main` が `extern` の run loop の中にいても P を持たないので、ほかの tzroutine は `TSUZURI_MAXPROCS=1` でも進む（3 で確かめた）。
- `tsuzuri_tz_callback_enter`（2157〜2165 行）は M でないスレッドでは何もしない。W12 の `tsuzuri_tz_callback_scope_enter`（2178〜2185 行）は、境界の中のスレッドでは `tzr_host_enter` だけをし、境界の外では自分の境界を作る。
- `src/runtime/test-runner.c` は、テストごとに別のプロセスで、C の `main` から `tsuzuri_test_run(index)` を呼ぶ（テストの本体はメインスレッドで走る）。

### 3. 回避策: `dlsym` と `dispatch_async_f`・`dispatch_after_f`

`dispatch_get_main_queue()` は C のマクロで、データシンボル `_dispatch_main_q` のアドレスを返す。データシンボルの `extern` はない（M06 の範囲）ので、`dlsym` で引く。

```tsuzuri
@trusted
@count(2, none)
extern "dlsym" def dlsym :: i64 -> ref utf8string -> i64

@trusted
extern "dispatch_async_f" def dispatch_async_f :: i64 -> i64 -> (i64 -> unit) -> unit

@trusted
extern "dispatch_after_f" def dispatch_after_f :: i64u -> i64 -> i64 -> (i64 -> unit) -> unit

@trusted
extern "dispatch_time" def dispatch_time :: i64u -> i64 -> i64u

@trusted
extern "CFRunLoopGetMain" def cf_run_loop_get_main :: unit -> i64

@trusted
extern "CFRunLoopRun" def cf_run_loop_run :: unit -> unit

@trusted
extern "CFRunLoopStop" def cf_run_loop_stop :: i64 -> unit

@trusted
extern "pthread_main_np" def pthread_main_np :: unit -> i32

def on_main :: i64 -> unit = \context ->
    do! IO.writeln $"posted: context={context} pthread_main_np={pthread_main_np ()}"
    ()

def on_main_later :: i64 -> unit = \context ->
    do! IO.writeln $"after: context={context} pthread_main_np={pthread_main_np ()}"
    cf_run_loop_stop (cf_run_loop_get_main ())

def main :: unit -> i32 = \() ->
    // RTLD_DEFAULT is (void *)-2 on Darwin.
    let queue = dlsym (0 - 2) (ref u8"_dispatch_main_q\0")
    tzroutine {
        Time.sleep (Time.millisecond() * 20)
        do! IO.writeln $"tzroutine: pthread_main_np={pthread_main_np ()}"
        dispatch_async_f queue 1 on_main
        // DISPATCH_TIME_NOW is 0.
        dispatch_after_f (dispatch_time 0i64u (Time.millisecond() * 50)) queue 2 on_main_later
    }
    cf_run_loop_run ()
    do! IO.writeln "main: run loop returned"
    0
```

`CF=$(xcrun --show-sdk-path)/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation.tbd` として `target/debug/tsuzuri build <dir> -O0 --link $CF -o a.out`。`-O0`・`-O3` と `TSUZURI_MAXPROCS=1`・`4` の 4 通りで同じ出力、終了コード 0 だった。

```text
tzroutine: pthread_main_np=0
posted: context=1 pthread_main_np=1
after: context=2 pthread_main_np=1
main: run loop returned
```

調査では `performSelectorOnMainThread:withObject:waitUntilDone:` でも戻せ、hardened runtime で署名しても動いた。どの形でも、状態は整数の鍵でしか渡せない。

### 4. `main` の終わりの境界の待ちは、メインのキューを処理しない

3 の `dlsym`・`dispatch_async_f` の宣言をそのまま使い、残りを次に変えた（CoreFoundation は使わない）。tzroutine は、投入した関数が `Channel` に送る結果を待つ。投入した関数は、`main` で作った `Anchor` の token から送り手を読む。

```tsuzuri
def on_main :: i64 -> unit = \token ->
    do! IO.writeln "posted ran"
    let _ = Anchor.read (\(out: ref Channel<i64>) -> Channel.send out 1) token
    ()

def main :: unit -> i32 = \() ->
    let queue = dlsym (0 - 2) (ref u8"_dispatch_main_q\0")
    let done: Channel<i64> = Channel.make 1
    let anchor = Anchor.create (Channel.share done)
    let token = Anchor.token (ref anchor)
    tzroutine {
        dispatch_async_f queue token on_main
        match Channel.recv done with
        | Maybe.Some _ -> do! IO.writeln "tzroutine: got the result"
        | Maybe.None -> do! IO.writeln "tzroutine: closed"
    }
    do! IO.writeln "main: returning"
    0
```

`target/debug/tsuzuri build <dir> -o a.out` の後の実行で、`on_main` は呼ばれず、終了コード 133 で次を出した。

```text
main: returning
Tsuzuri runtime: all tzroutines are asleep - deadlock! (2 waiting: Channel.recv, end of scope (waiting for tzroutines))
```

対照として、3 の CoreFoundation の 3 つの宣言を足し、`on_main` の最後で `cf_run_loop_stop (cf_run_loop_get_main ())` を呼び、`main` で `do! IO.writeln "main: returning"` の前に `cf_run_loop_run ()` を呼ぶ版（`--link $CF`）は、`posted ran`・`tzroutine: got the result`・`main: returning` を出して終了コード 0 だった（`TSUZURI_MAXPROCS=1`・`4`。4 では後の 2 行の順が入れ替わることがある）。

### 5. `exit()` は境界を待たない

`[NSApp terminate:]` は最後に `exit()` を呼ぶ。同じことを `extern` の `exit` で確かめた。

```tsuzuri
@trusted
extern "exit" def c_exit :: i32 -> unit

def main :: unit -> i32 = \() ->
    tzroutine {
        let mut n = 0
        while true do
            Time.sleep (Time.millisecond())
            n = n + 1
    }
    Time.sleep (Time.millisecond() * 10)
    do! IO.writeln "main: calling exit(0) with a live tzroutine"
    c_exit 0
    do! IO.writeln "main: after exit"
    1
```

出力は `main: calling exit(0) with a live tzroutine` だけで、終了コードは 0 だった。D-61 の「境界の終わりを待ち、Drop を必ず走らせる」は成り立たない。ランタイムの `atexit` は GPU の後始末だけ（`src/runtime/gpu.c` 766 行、`src/runtime/gpu-vulkan.c` 1427 行）。調査では、`applicationShouldTerminate:` が `NSTerminateCancel` を返し、`[NSApp stop:]` と自前のイベント（`postEvent:atStart:`）で `-run` を戻すと、`main` が正常に返って境界の終わりが走った。

### 6. Tsuzuri のタイマーと `Anchor` はメインスレッドへ届かない

- `Time.after_func :: i64 -> Owned.Function<unit, unit> -> Timer`（`src/check.rs` 2043 行）は新しい tzroutine で呼ぶ（`time.md` 175 行）。`main` の後に残った関数は呼ばずに drop する（`time.md` 196 行）。
- `main` で作った `Anchor` の token を tzroutine で `Anchor.read` すると `Error WrongThread`（`anchor.md` 84 行）。W23 は「別のスレッドから引ける預け先」を対象外にした（`_features/_completed/W23-anchor-state.md` 240 行）。

```tsuzuri
@trusted
extern "pthread_main_np" def pthread_main_np :: unit -> i32

def main :: unit -> i32 = \() ->
    let signal: Channel<i32> = Channel.make 1
    let out = Channel.share signal
    let _timer = Time.after_func (Time.millisecond()) (Owned.function (\() -> Channel.send out (pthread_main_np ())))
    let on_timer = match Channel.recv signal with
        | Maybe.Some v -> v
        | Maybe.None -> 9
    do! IO.writeln $"Time.after_func: pthread_main_np={on_timer}"
    let anchor = Anchor.create 42i64
    let token = Anchor.token (ref anchor)
    let answer: Channel<string> = Channel.make 1
    let reply = Channel.share answer
    tzroutine {
        let text = match Anchor.read (\(v: ref i64) -> deref v) token with
            | Result.Ok v -> $"Ok {v}"
            | Result.Error e -> $"Error {e}"
        Channel.send reply text
    }
    let from_tzroutine = match Channel.recv answer with
        | Maybe.Some text -> text
        | Maybe.None -> "none"
    do! IO.writeln $"Anchor.read in a tzroutine: {from_tzroutine}"
    0
```

`TSUZURI_MAXPROCS=1`・`4` のどちらも、終了コード 0 で次を出した。

```text
Time.after_func: pthread_main_np=0
Anchor.read in a tzroutine: Error anchor token belongs to another thread
```

### 7. 使える仕組み

- `Owned.function` の捕捉は Send に限る（B07、`src/closures.rs` の `LambdaKind::Owned`）。`Time.after_func` は閉包を `@tz.alloc(i64 16)` の箱に入れ、`@tz.tzroutine.owned`（呼んで drop）と `@tz.tzroutine.owned.drop`（呼ばずに drop）をランタイムに渡す（`src/llvm_timer.rs` 57〜65 行）。
- W19: `builtin_position`（`src/pinned_rules.rs` 33 行）は wildcard を持たず、位置は `Pinning`・`Inline`・`Spawned`・`Escaping`（20〜30 行）。`visit_spawned_arg`（688 行）は `Owned.function (\() -> …)` を剥がし、`visit_allowed_callback`（672 行）は剥がさない。`pinned` は `pinning_argument_mask`（`src/effects.rs` 2199 行）の辺を除いて求める。
- `Effect` は 15 個（`src/effects.rs` 20〜36 行、数は 3778 行で固定）、`EffectSet` は `u16`（84 行）、`Builtin::ALL` は 375 個（3839 行）、`HandleKind` は `Owned`・`Copy`・`Local`（`src/syntax.rs` 775〜782 行）。
- W12: `with_blocking_externs`（`src/llvm.rs` 1840 行から）は、spawn する native のプログラムで `extern` の symbol があるときだけ、`@tz.callback.*` の入口と出口に `callback_enter`・`scope_enter` などを入れる。`main` の後のコールバックは `@tz.callback.after_main` でトラップする（2238〜2261 行）。
- driver: 部品は IR の `declare` の接頭辞で選ぶ（`src/driver.rs` 4261〜4263 行）。対応しない native は `E2002` `platform_unsupported_api`（4440〜4465 行。`--emit llvm` では出さない）。C の単位は 9 つの部品の後に、使うときだけ部品を足す（4890 行）。`--freestanding` の表は 4294 行。`--trap-mode return` は tzroutine・`Anchor`・同期の型（3893〜3914 行）とコールバック（4100 行）を `E2000` `option_conflict` で拒む。

## 変えてはいけない意味

- S1・S2 の出力。`main` の擬似 G と D-61 の待ち。`main` の終わりは投入を待たない（タイマーと同じ）。
- `Time.after_func`・`Routine.go`・`with_locked_thread` の意味と、`local` だけのプログラムの W19 の判定。
- トラップの意味。投入した関数の中のトラップは、ほかのトラップと同じくプロセスを終わらせる。

## 仕様

### Phase の範囲

| Phase | 内容 | 規模 | 承認 |
| --- | --- | --- | --- |
| 1 | `MainThread.post`・`after`・`is_current`（macOS）、境界での drop、target の診断、W19 の位置、規律の文書 | M | D1 |
| 2 | `extern type main`・`extern class main`（M01 の後）と効果 `main_thread`、`E1013` の 2 つの kind、入口の検査 | M | D11・D12 |
| 3 | ランタイムに参加していないスレッドから引ける、Send・Sync の値の預け先 | S〜M | D16 |

### Phase 1: API（`MainThread.post`・`after`・`is_current`）

| 関数 | 型 | 意味 |
| --- | --- | --- |
| `MainThread.post` | `Owned.Function<unit, unit> -> unit` | プロセスのメインスレッドで 1 回呼ぶよう予約し、すぐ戻る。呼んだ後に drop する |
| `MainThread.after` | `i64 -> Owned.Function<unit, unit> -> unit` | 少なくとも `d` ナノ秒後にメインスレッドで 1 回呼ぶ。`d` が 0 以下なら `post` と同じ |
| `MainThread.is_current` | `unit -> bool` | 呼んだスレッドがプロセスのメインスレッドなら真 |

- 型は `Routine.go`・`Time.after_func` と同じ形（D2）。文書の表は `routine.md` と同じく `own Owned.Function<unit, unit>` と書く。
- どのスレッド（`main`、tzroutine、コールバック、export を呼んだホストのスレッド）からも呼べる。メインスレッドから呼んでも、その場では呼ばない。
- 同じスレッドからの `post` は投入の順に呼ばれる（メインのキューは直列の FIFO）。`after` は期限の順で、同じ期限どうしの順は決めない。
- 呼ばれるのは、メインスレッドがメインのキューを処理するとき（`[NSApp run]`、`CFRunLoopRun`、`dispatch_main`）だけである。Tsuzuri の待ち（`Channel.recv`・`Time.sleep`・境界の待ち）は処理しない（「現状」4）。
- 捕捉は `Owned.function` の規則（Send）に従う。効果は `task` と `alloc`（`Time.after_func` と同じ。`pure def` の中では `E1029`）、`is_current` は `task`（`Routine.count` と同じ）。

### Phase 1: 呼ばれ方と境界

- 投入した関数は W12 の静的コールバックと同じ境界の規則で呼ぶ（D5）。`main` の間は `main` の境界に属し、中で起動した tzroutine は `main` の終わりで待たれる。境界の外のメインスレッド（ライブラリ出力）では、その 1 回の呼び出しが自分の境界になる。
- `main`・`test`・`bench` の境界の終わりに、未実行の投入はすべて呼ばずに drop する（D6）。drop は、境界の本体が返った直後、境界の待ち（`tsuzuri_tz_boundary_leave`）より前に、メインスレッドで行う。drop は捕捉した値の `Drop` を走らせ、それが起動した tzroutine とタイマーは、続く境界の待ちが受け持つ（`Time.after_func` の drop を境界の中で行う `tzr_boundary_settle` と同じ考え方）。それより後の `post`・`after` は、呼んだスレッドでその場で drop する。境界の待ちの間、メインスレッドはキューを処理しないので（「現状」4）、早く drop しても呼ばれる投入は減らない。
- `bench` は 1 つのプロセスで `tsuzuri_bench_sample` を何度も呼ぶ。標本の境界の入口で、前の標本の終わりの印を下ろす（設計の `tsuzuri_mainthread_boundary_open`）。`test` はテストごとに別のプロセスである（「現状」2）。
- ライブラリ出力（`--emit object`・`shared`・`llvm`・`bitcode`）では、投入はプロセスの寿命の間残る（D10）。
- `after` の期限を `dispatch_time` が `DISPATCH_TIME_FOREVER` に丸めたら、キューに積まずに境界の drop を待つ。

### Phase 1: target と診断

- `check` はどの target でも受け入れる。`build` は macOS（`aarch64-apple-darwin` と macOS の既定の native）だけで動く（D3）。
- Apple でない native の target: `E2002` `platform_unsupported_api`、`MainThread is available only on macOS targets`（`--emit llvm` では出さない）。`wasm32`・`wasm64`: `E2000`、新しい kind `wasm_main_thread_unsupported`、`MainThread is not available on WebAssembly`。
- `--trap-mode return`: `E2000` `option_conflict`、`--trap-mode return cannot be combined with MainThread: a posted function runs outside the call that posted it`。`--freestanding`: 表に `("@tsuzuri_mainthread_", "MainThread")`（D17）。
- 利用者のモジュール `MainThread.tz` は `E1011`（互換のない変更として `docs/stability.md` に記録する）。

### Phase 1: 規律（文書に書く）

- **メインスレッドで待たない**: `main` やメインスレッドのコールバックの `Channel.recv`・`Time.sleep`・`WaitGroup.wait` は run loop を止め、UI が固まる。処理は tzroutine で行い、結果を `post` で戻す（D8）。
- **終了**: `[NSApp terminate:]` は `exit()` を呼び、境界の待ちも Drop も走らない。`applicationShouldTerminate:` で `NSTerminateCancel` を返し、`[NSApp stop:]` と自前のイベントで `-run` を戻して `main` を返す。終わらない tzroutine は `Context` で止める（D9）。
- **タイマー**: UI の周期の処理は `after` を投入し直すか `NSTimer` を使う。`Time.after_func`・`Timer`・`Ticker` はワーカーで発火する。

### Phase 1: W19 との関係

- `post`・`after` の関数引数は新しい位置 `Posted`（D7）。`pinned` な関数を渡せ、`Owned.function (\() -> …)` を剥がして中を見る。その辺は `pinned` を呼んだ側へ運ばない。
- 理由: 関数はメインスレッドという 1 つの固定したスレッドで走る。`local` のハンドルは Send でないので捕捉できず、関数の中で作るか、メインスレッドで作った `Anchor` から読むしかない（ほかのスレッドで作った `Anchor` は `WrongThread`）。

### Phase 1: 例（`tests/fixtures/main_thread/Main.tz` の核）

```tsuzuri
@trusted
extern "CFRunLoopGetMain" def cf_run_loop_get_main :: unit -> i64

@trusted
extern "CFRunLoopRun" def cf_run_loop_run :: unit -> unit

@trusted
extern "CFRunLoopStop" def cf_run_loop_stop :: i64 -> unit

def report :: string -> unit = \label ->
    do! IO.writeln $"{label}: {MainThread.is_current ()}"
    ()

def main :: unit -> i32 = \() ->
    tzroutine {
        MainThread.post (Owned.function (\() -> report "posted on main"))
        MainThread.after (Time.millisecond() * 50) (Owned.function (\() -> {
            report "after on main";
            cf_run_loop_stop (cf_run_loop_get_main ())
        }))
    }
    cf_run_loop_run ()
    0
```

起票時のコンパイラにはない API の例である。`MainThread.post` を `Routine.go` に、`MainThread.after` を `let _t = Time.after_func` に、`MainThread.is_current ()` を `pthread_main_np () != 0` に置き換えた版で `check` が 0 になることを確かめた（ラムダの本体は、`unit` を返す関数の呼び出しか `{ …; … }` のブロックにする）。

### Phase 2: 宣言（`extern type main`・`extern class main`）

- `extern type main H`、`extern class main C : Base`（M01 の綴り。修飾の順は `extern class [export] [main|sync] Name`）。`main` は `extern type`・`extern class`（M03 の `export` があればその後）の直後で、同じ行に型名が続くときだけの文脈キーワード（W18 の `copy` と同じ規則）。`copy`・`local`・`sync` との併用は `E0002` `extern_type_modifier_conflict`。
- `main` のハンドルは Copy・Send・Sync・Capture のどれも満たさない。`holds_thread_bound` は真で、触れる関数は `pinned` も持つ。

### Phase 2: 効果 `main_thread`

- **源**: `main` のハンドル（`ref H`・`Maybe<H>` を含む）に型が届く関数。`main` を宣言しないプログラムでは解析を走らせない。
- **伝播**: 呼び出し・参照・`user_drop` の辺。運ばない辺: `post`・`after` の関数引数（メインスレッドが保証される）、コールバックを取る `extern` のコールバックの位置と `FnPtr.of`（入口の検査が守る）。`with_locked_thread` の関数引数の辺は運ぶ（D11）。`main` の中の `with_locked_thread` は擬似 G で何もしないので通り、tzroutine の中なら起動の位置で拒まれる。
- **使える文脈**: 関数値を通らずに到達する `main` の本体、`post`・`after` の本体、コールバック・`FnPtr`・export・`test`・`bench`・M03 の IMP・M04 の Block の本体（入口の検査つき）。

### Phase 2: 拒否（`E1013`）

- `main_thread_code_spawned`（tzroutine の本体、`Routine.go`・`WaitGroup.go`・`ErrGroup.go`・`Routine.scope`・`Time.after_func`・`Context.after_func`・`Parallel.*` の関数引数で関数値にした）: `this starts code that uses the main-thread extern type '{H}' on another tzroutine or thread; 'Routine.with_locked_thread' pins a tzroutine to a worker thread, not to the main thread; run it in 'main' or in a callback that the main thread calls, or pass it to 'MainThread.post'`。
- `main_thread_function_value`（それ以外の位置で関数値にした）: `this function value runs code that uses the main-thread extern type '{H}', and a function value may reach another thread; call the function directly, or pass it straight to 'MainThread.post' or to a callback 'extern'`。
- 位置・`related`・`help` は W19 の 2 つの kind と同じ形。同じ関数値に `pinned_*` を重ねて出さない（D14）。

### Phase 2: 入口の検査

- `main_thread` を持つ本体を呼ぶ包み（コールバック・`FnPtr`・export・`test`・`bench`・ライブラリの `tsuzuri_main`・M03・M04 の包み）だけが、入口で `pthread_main_np()` を呼ぶ。0 なら `Tsuzuri runtime: a callback that uses main-thread handles was called on another thread` と改行を書いてトラップする（`@tz.callback.after_main` と同じ形。`TrapKind` を増やさない）。実行ファイルの `main` と `post`・`after` の本体には入れない。
- ほかの包みの IR は変えない。検査は Apple の target でだけ下げる。ほかの native で検査が要れば `E2002`、wasm は `E2000` `wasm_main_thread_unsupported`。検査の要らないプログラムはどの target でも作れる（D13）。
- `main` の型への M02 の送信は、ブロッキングの包み（`tsuzuri_tz_blocking_enter`・`_exit`）を省く（D15）。

### Phase 3: 別のスレッドから引ける預け先（要承認）

- GCD のワーカーや完了ハンドラーが、tzroutine へ値を渡せるようにする（「現状」6）。
- 案（D16。名前は承認で決める）: `Anchor.create_shared :: (Send<'a>, Sync<'a>) => 'a -> Anchor.Shared<'a>`、`Anchor.shared_token :: ref Anchor.Shared<'a> -> i64`、`Anchor.read_shared :: (ref 'a -> 'b) -> i64 -> Result<'b, Anchor.Error>`。`read_shared` はどのスレッドからでも呼べ、`WrongThread` を返さない。書き換えの関数は持たない（変えたい値は `Mutex` か `Channel` に入れて預ける）。所有者の drop は、読んでいる途中の読み終わりを待ってから値を drop する。D-59: 鍵が最後の引数。

## 設計

### Phase 1: 型検査と分類

- `Builtin` に `MainThreadPost`・`MainThreadAfter`・`MainThreadIsCurrent`（`src/check.rs`。scheme は `RoutineGo`・`TimeAfterFunc` の腕が手本）。`RESERVED_MODULES` に `MainThread`（79 個）。`src/effects.rs` の `facts`・`src/pure_rules.rs`・`src/kg/effects.rs` に分類を足し、`Builtin::ALL` の固定を 378 にする。`Builtin::spawns` には足さない（投入した関数の中の `tzroutine { }` は、その式で spawn として数えられる）。
- `src/pinned_rules.rs`: `Position::Posted`。`builtin_position` で `post`・`after` を `Posted`、`is_current` を `Escaping`。`visit_call_parts` の `Posted` の腕は、引数が `Owned.function` の直接の呼び出しなら中の関数を `visit_allowed_callback` へ渡す。`src/effects.rs` の `pinning_argument_mask` の呼び側で、その中の関数への辺を固定する位置として扱う。

### Phase 1: ランタイム（`src/runtime/mainthread-darwin.c`）

```c
void tsuzuri_mainthread_post(int64_t ns, void (*run)(void *), void (*drop)(void *), void *box);
int32_t tsuzuri_mainthread_is_current(void);
void tsuzuri_mainthread_boundary_open(void);
void tsuzuri_mainthread_boundary_done(void);
```

- 記録 `{ prev, next, run, drop, box, state }` を `malloc` し、未実行の記録の双方向リスト（`pthread_mutex_t`）に入れる。`ns <= 0` は `dispatch_async_f(dispatch_get_main_queue(), rec, trampoline)`、そうでなければ `dispatch_after_f(dispatch_time(DISPATCH_TIME_NOW, ns), …)`。
- `trampoline`（GCD がメインスレッドで呼ぶ）: 鍵の中で記録の状態を見る。drop 済みなら記録を解放して戻る。そうでなければリストから外し、鍵の外で `run(box)` を呼び、記録を解放する。
- `boundary_done`: 鍵の中で `done` を立て、未実行の記録をリストから外して drop 済みにする。鍵を放してから、外した記録ごとに `drop(box)` を呼ぶ（キューに積んだ記録は `trampoline` が、積まなかった記録はここで解放する）。`done` の後の `post` は、鍵を放してから `drop(box)` をその場で呼ぶ。`boundary_open` は `done` を下ろす（`bench` の標本の入口）。`is_current` は `pthread_main_np() != 0`。
- `<dispatch/dispatch.h>` を include し、`dispatch_get_main_queue()` のマクロを使う（データシンボルの構文は要らない）。同梱の zig の sysroot にヘッダーがなければ、`extern struct dispatch_queue_s _dispatch_main_q;` と 3 つの関数を自前で宣言する（clang の参照の IR で `_dispatch_main_q` は `external global`）。

### Phase 1: 生成（`src/llvm_mainthread.rs`）

- `post`・`after`: 閉包を `@tz.alloc(i64 16)` の箱に入れ、`call void @tsuzuri_mainthread_post(i64 <ns>, ptr @tz.mainthread.run, ptr @tz.tzroutine.owned.drop, ptr %box)`（`post` の `ns` は 0）。
- `@tz.mainthread.run(ptr %box)`: `@tz.tzroutine.owned` と同じ本体で、spawn するプログラムでは `tsuzuri_tz_callback_scope_enter`・`_leave` で挟む（D5）。名前を `@tz.callback.` で始めない（`with_blocking_externs`・`--trap-mode return` の判定・`@tz.main.done` の守りから外す。メインスレッドは M にならないので `callback_enter` は要らない）。
- `is_current`: `call i32 @tsuzuri_mainthread_is_current()` を `icmp ne` で `i1` に。
- 境界の終わり: `MainThread` に到達したプログラムで、`Entry` が `Console`・`TestRunner`・`BenchRunner` のときだけ、境界の本体が返った直後、境界の待ち（`tsuzuri_tz_boundary_leave`）より前に `call void @tsuzuri_mainthread_boundary_done()` を置く。場所は、`console_def_main` と `console_main` の `@main` の本体の呼び出しの後（`{leave}` の前）、`IO` の entry では境界を持つ `tsuzuri_main`（`with_tzroutine_scopes` が包む）の各 `ret` の待ちの前、`tsuzuri_test_run`・`tsuzuri_bench_sample` の各 `ret` の待ちの前である。tzroutine を起動しないプログラムには待ちがないので、本体の後に置くだけである。`tsuzuri_bench_sample` の入口には `call void @tsuzuri_mainthread_boundary_open()` を置く。`Entry::Library` には置かない（D10）。`declare` は symbol ごとに 1 行、並びは決定的にする。

### Phase 1: driver

- `main_thread_runtime = runtime_symbols.prefix("tsuzuri_mainthread_")`。wasm なら `E2000`、native で `!platform.mach_o()` かつ `emit != Llvm` なら `E2002`、`--trap-mode return`・`--freestanding` は仕様の「Phase 1: target と診断」。
- C の単位: 9 つの部品の後に、使うときだけ `include_str!("runtime/mainthread-darwin.c")` を足す（S2）。`src/diagnostic_kinds.rs` に `(WasmMainThreadUnsupported, "wasm_main_thread_unsupported", "E2000")`。

### Phase 2: 型・効果・包み

- `src/syntax.rs` の `HandleKind::Main`、`src/parser.rs` の `extern type` の修飾（W18・W19 の読み取り）に `main`、`src/syntax_codec.rs` の符号化と `FRONTEND_FORMAT` + 1。M01 があれば `extern class` の修飾にも（M01 の設計の `HandleKind::ObjcMain`。M01 の `ObjcLocal`・`ObjcSync` と並べる）。
- `src/check.rs`: `Type::holds_main_handle`、`holds_thread_bound` は `Main` も真。`CheckedModule::main_bound`。
- `src/effects.rs`: `Effect::MainThread`（`main_thread`）、`ALL` を 16 個。`pinned` と同じく運ばない辺を除いた部分グラフで求める。運ばない辺は `pinned` の固定する位置から `with_locked_thread` を除き、`post`・`after` を加えたもの。
- `src/pinned_rules.rs`: 2 つの効果で解析し、`main_thread` を先に判定する。`src/diagnostic_kinds.rs` に 2 つの kind。
- 包み（`src/llvm_abi.rs` の `host_abi::wrapper`、export・`test`・`bench` の入口）: 本体の要約が `main_thread` を持つときだけ入口に検査。`@tz.main_thread.wrong` は `@tz.callback.after_main` と同じ形（`cold noreturn nounwind`、`@tz.trap.write`、`llvm.trap`）。`declare i32 @pthread_main_np()` は利用者の同じ symbol の `extern` と 1 行にまとめる。
- `with_blocking_externs`: 行末の印（例 `; tz.main-thread`）を持つ呼び出しを包まない。M02 の `main` の型への送信が付ける（M02 が先ならこの Phase で、後なら M02 が付ける）。

### 採らない案

- **run loop と tzroutine の待ちの統合**（`main` が待つ間に `CFRunLoopRunInMode` を回す、Swift の `MainActor` のように待ちを中断にする）: D-61 は擬似 G の待ちが OS スレッドを止めると決めている。統合にはランタイムが CoreFoundation と AppKit を知り、`main` の待ちの意味を OS ごとに変える（S3）。UI のスレッドで同期に待たないのはどのツールキットでも同じ規則で、tzroutine と `post` の形は今日動く（「現状」3）。
- **`exit()` に D-61 の待ちを守らせる**（`atexit` で境界を待つ）: `exit` の中でもほかのスレッドは動き、`atexit` の順も保証されない。
- **`Routine.post_main`**: `Routine` は tzroutine の起動と問い合わせのモジュールで、投入した関数は tzroutine で走らない。
- **`performSelectorOnMainThread:`・`CFRunLoopPerformBlock`**: ObjC のランタイムか Block の ABI が要る。メインのキューは NSApp・CFRunLoop・`dispatch_main` のどれでも処理され、C の関数ポインターで積める。

## 前提とする他チケットのインターフェース

- [M08](_completed/M08-macos-main-thread-docs-and-e1013.md): 2 つのページの 2 段落と「メインスレッドへ戻す API は今はない（計画は M09）」の注記（Phase 1 が置き換える）、定数 `MAIN_THREAD_ADVICE`（Phase 2 の文の言い回しをそろえる）。
- [M01](_completed/M01-objc-class-types.md)（Phase 2 の `extern class main` の段だけ）: `extern class` の修飾の読み取りとクラスの性質の表現。名前が違えば M01 に合わせる。
- W19・W12・TZ05 の既存の仕組み（「現状」7）。

## 他チケットへ提供するインターフェース

- [M14](M14-tls-http-packages.md): `MainThread.post`（Phase 1）と預け先（Phase 3）。M14 は、項目を作った関数が戻った後も項目を生かし、鍵で解放する操作も求める（M14 の D5・S1）。D16 の案（所有者 `Anchor.Shared` の drop で解放する）にはこの操作がないので、D16 の承認の議題に含める。
- [M03](M03-objc-classes-defined-in-tsuzuri.md): `extern class export main` の `main` と、IMP の包みの入口の検査（Phase 2）。[M04](M04-objc-blocks.md): Block の invoke の包みの入口の検査（Phase 2）。
- [M07](M07-apple-sdk-bindgen-and-package.md): `NS_SWIFT_UI_ACTOR` のクラスを `extern class main` に写す（Phase 2 がなければ `local`）。[M13](M13-desktop-os-services-package.md): ダイアログの完了を UI へ戻す `post`。
- 名前: `tsuzuri_mainthread_post`・`_is_current`・`_boundary_open`・`_boundary_done`、`@tz.mainthread.run`、`Position::Posted`、`Effect::MainThread`、`HandleKind::Main`（M01 があれば `HandleKind::ObjcMain` も）、kind `wasm_main_thread_unsupported`・`main_thread_code_spawned`・`main_thread_function_value`。

## 実装手順

### Phase 1

1. **変更**: なし（ブランチ `feat/m09-main-thread-post`）。**内容**: 着手条件。「現状」3〜6 を `target/scratch/m09/` で繰り返し、ベースラインを保存する。仕様の「Phase 1: 例」の `check` が `E1002` になることを確かめる。OS API の `E2002` がツールを呼ばずに出る target と出力の組を確かめる（r9 の形を決める）。**確認**: 「現状」と同じ出力。
2. **変更**: `src/check.rs`・`src/stdlib.rs`・`src/effects.rs`・`src/pure_rules.rs`・`src/kg/effects.rs`。**内容**: 設計の「Phase 1: 型検査と分類」（W19 を除く）。**確認**: r1〜r4 と数の固定。`cargo test --locked --test main_thread` が `running N tests`（N は 1 以上）で通る。
3. **変更**: `src/pinned_rules.rs`・`src/effects.rs`。**内容**: `Position::Posted`。**確認**: r5〜r7。`cargo test --locked --test thread_bound_handles` が変わらず通る。
4. **変更**: `src/runtime/mainthread-darwin.c`・`src/llvm_mainthread.rs`・`src/llvm.rs`・`src/driver.rs`・`src/diagnostic_kinds.rs`。**内容**: 設計の「Phase 1: ランタイム」「生成」「driver」。**確認**: r8〜r12、ベースラインと `cmp`（S1）、`MainThread` を使わない fixture の C の単位の比較（S2）、`sh scripts/check-runtime-includes.sh`。
5. **変更**: `tests/main_thread.mjs`・`tests/fixtures/main_thread/Main.tz`・`.github/workflows/targets.yml`・`README.md`。**内容**: E2E。`targets.yml` の darwin-arm64 の `suites` の群（`tests/tzroutine.mjs` を走らせる段）に `node tests/main_thread.mjs target/release/tsuzuri` を足し、README の「検証と性能測定」の変更の種類別の表に `MainThread`（M09）の行（`tests/main_thread.rs`、`main_thread.mjs`（macOS だけ））を足す（`targets.yml` に足さないと E2E は CI で走らない）。**確認**: `node scripts/slot.mjs -- cargo build` の後、`node tests/main_thread.mjs target/debug/tsuzuri` を Apple の ld と同梱の zig（`TSUZURI_CLANG` に同梱の `tsuzuri-clang`）で。残ったプロセスがない。
6. **変更**: 文書。**確認**: 変えたページの `node scripts/check-docs.mjs`、`node scripts/gen-docs.mjs`（と `--check`）、`node scripts/check-cli-docs.mjs`、T1、T2。PR に `ci:full`。`target/scratch/m09/` を消す。

### Phase 2

1. **変更**: `src/syntax.rs`・`src/parser.rs`・`src/syntax_codec.rs`・`src/frontend_cache.rs`。**内容**: `HandleKind::Main` と文脈キーワード。**確認**: 構文の受理と `E0002`、`cargo test --locked --test formatter`、符号化の固定。
2. **変更**: `src/check.rs`・`src/effects.rs`・`src/pinned_rules.rs`・`src/diagnostic_kinds.rs`。**内容**: 効果と 2 つの kind。**確認**: `tests/thread_bound_handles.rs` の新しい表。`local` だけのプログラムの診断が変わらない（S5）。
3. **変更**: `src/llvm_abi.rs`・`src/llvm.rs`・`src/driver.rs`。**内容**: 入口の検査、`E2002`・`E2000`、印。**確認**: 検査が要る包みだけに入る IR の検査、E2E のトラップ、ベースラインと `cmp`。
4. M01 が done なら `extern class main`。文書、T1、T2、`ci:full`。

### Phase 3

- 承認された名前で `std/Anchor.tz` と `src/runtime/tzroutine.c` の anchor の表に共有の表を足す。C のホストの pthread から `read_shared` を呼び、`Channel` で tzroutine へ送る E2E。

## テスト計画

### Rust（`tests/main_thread.rs`）

| # | 入力 | 期待 |
| --- | --- | --- |
| r1 | 3 つの関数を型の注釈つきの `let` に束縛 | 通る |
| r2 | `Rc` を捕捉した `Owned.function` を `post` に渡す | `E1013` `task_requires_send`（主の位置はラムダ。HEAD の `Routine.go (Owned.function (\() -> … Rc.get (ref shared) …))` と同じ診断） |
| r3 | `pure def` の中の `post` | `E1029` |
| r4 | 利用者の `MainThread.tz` | `E1011` |
| r5 | `extern type local Window` に触れる関数を `post (Owned.function (\() -> touch ()))` | 通る |
| r6 | 同じ関数を `Time.after_func` に渡す | `E1013` `pinned_code_spawned`（今と同じ） |
| r7 | r5 の `post` を tzroutine の中で呼ぶ | 通る（辺は `pinned` を運ばない） |
| r8 | `--target wasm32` の build | `E2000` `wasm_main_thread_unsupported` |
| r9 | Apple でない native の target の build | `E2002` `platform_unsupported_api` |
| r10 | `--trap-mode return`・`--freestanding` | `E2000` `option_conflict` |
| r11 | `MainThread` を使わない fixture | IR・header・診断がベースラインと同じ |
| r12 | `post` を 2 回使うプログラムの IR | `declare void @tsuzuri_mainthread_post` が 1 行、`@tz.mainthread.run` が 1 つ、`@main` の本体の呼び出しの後で `tsuzuri_tz_boundary_leave` より前に `boundary_done`。`--emit llvm`（`Entry::Library`）には `boundary_done` がない。`tsuzuri bench` の IR は `tsuzuri_bench_sample` の入口に `boundary_open`。2 回の生成で同じ |

### E2E（`tests/main_thread.mjs`、macOS だけ）

- CoreFoundation は `xcrun --show-sdk-path` の `CoreFoundation.tbd` を `--link` で渡す（「現状」3）。各場面を `-O0`・`-O3`、`TSUZURI_MAXPROCS=1`・`4` で、`perl -e 'alarm 15; exec @ARGV'` の時間制限つきで実行し、残ったプロセスがないことを確かめる。
  - `runloop`: 仕様の「Phase 1: 例」。2 行が真、終了コード 0。
  - `from_main`: `main` が `CFRunLoopRun` の前に `post` を 3 つ積む。投入の順に呼ばれる。
  - `drop_at_end`: `main` が run loop を回さずに返る。`Drop` を持つ値（`drop` で 1 行を書く record）を捕捉した投入は呼ばれず、`main` の本体の後に `Drop` の行が 1 回だけ出る。
  - `spawn_in_post`: 投入した関数の中の `tzroutine { }` が `main` の終わりで待たれる。
  - `gcd_worker`: `dispatch_get_global_queue` のワーカーから呼ばれた静的コールバックが `post` し、メインスレッドで呼ばれる。
  - `after_order`: `after` の 30 ms と 10 ms が期限の順に呼ばれる。
- Phase 2: C のホスト `tests/fixtures/main_thread/host.c` を `clang -c -pthread` で object にして `--link` で渡す（`tests/tzroutine.mjs` の `callback_lifetime` と同じ形）。ホストの pthread から `main_thread` の本体を持つコールバックを呼ぶと、仕様の文を出してトラップする（終了コード 133）。メインスレッドから呼べば通る。
- 既存: `tests/thread_bound_handles.rs`・`tests/tzroutine.mjs`・`tests/blocking.rs` は変わらない。

## ドキュメント

- 新しいページ `built-in-types-and-modules/mainthread.md`（API、呼ばれる条件、境界での drop、ライブラリ出力、target の診断、規律、「引数の順序」の節、Swift・Kotlin との比較）と `index.md`。
- `routine.md`・`native-interop.md`: M08 の「計画は M09」を `MainThread.post`・`after` の説明とリンクに置き換える（GUIDE §8 の 3）。`time.md`: `Time.after_func` はワーカーで発火し、UI には `MainThread.after` を使う。
- `docs/language.md` の `Routine` の一覧（2720 行）の隣、`README.md` の機能の一覧の tzroutine の項、`compiler/diagnostics.md`（`gen-docs`）、`compiler/platforms.md`（macOS だけの API）。
- Phase 2: `native-interop.md` の「スレッド拘束ハンドル」、`docs/language.md` の「不透明ハンドル」と効果の語彙（16 個）。M01 が done なら、M01 が作る `compiler/objective-c.md` に `extern class main` と、AppKit のクラスを `main` で宣言する例を足す。

## 受け入れ条件

- [ ] （Phase 1）例と E2E の 6 場面が、macOS の `-O0`・`-O3`、`TSUZURI_MAXPROCS=1`・`4`、Apple の ld と同梱の zig で通り、残ったプロセスがない。
- [ ] （Phase 1）境界の終わりに残った投入は呼ばれずに drop され、捕捉した値の `Drop` が 1 回だけ走る。
- [ ] （Phase 1）r1〜r12 が通る。S1・S2 の出力が変わらない。
- [ ] （Phase 1）M08 の注記を置き換えて新しいページを足し、`check-docs`・`gen-docs --check`・`check-cli-docs`・`check-runtime-includes.sh`・T1・T2 が通る。PR に `ci:full`。
- [ ] （Phase 2）2 つの kind、検査が要る包みだけの入口の検査、ほかのスレッドからの呼び出しのトラップ、`local` だけのプログラムの判定が変わらないこと。
- [ ] （Phase 3）承認された名前で、ランタイムに参加していないスレッドから値を tzroutine へ渡せる。

## 落とし穴

- **キューは run loop の中でしか処理されない**: E2E は必ず `CFRunLoopRun` と `CFRunLoopStop` で囲み、時間制限をかける。tzroutine が投入の結果を待ち、`main` が run loop を回さずに返るとデッドロックの報告になる（「現状」4）。
- **`DISPATCH_TIME_FOREVER`**: Apple の文書は `dispatch_after` に渡すことを未定義とする。積まずに境界の drop を待つ。
- **drop と trampoline の競合**: 同じ鍵で状態を見る。`run` と `drop` は鍵の外で呼ぶ（投入した関数も、捕捉した値の `Drop` も、`post` を呼べるため。鍵の中で呼ぶと同じ鍵を取り直して止まる）。
- **drop の時点**: 境界の待ちの後に drop すると、`Drop` が起動した tzroutine は根のスコープ（`tzr_spawn_scope` の `tzr_root_scope`）に入り、誰も待たない。drop は待ちの前に行う（仕様の「Phase 1: 呼ばれ方と境界」）。
- **`bench` の標本**: `done` を下ろさないと、2 つ目以降の標本の投入がすべてその場で drop される。
- **prebuilt のランタイム**: 新しい部品を 9 つの部品の中に入れると、すべてのプログラムの C の単位が変わる（S2）。
- **`@tz.callback.` の名前**: trampoline に付けると、`--trap-mode return` の拒否と `with_blocking_externs` に巻き込まれる。
- **Phase 2**: `EffectSet` の `u16` の最後のビットを使う。`EffectSet::ALL` の `(1 << 15) - 1`（`src/effects.rs` 88 行）は 16 個では `u16` の `1 << 16` で溢れるので、`u16::MAX` に書き換える。`Effect` は名前の順に並べる規則（3778 行のテスト）なので、`main_thread` は `io` と `net` の間に入り、後ろの効果のビットの位置が 1 つずれる。利用者が `extern "pthread_main_np"` を宣言していればその `declare` を使う（LLVM は型の違う 2 つ目の `declare` を拒む）。
- **性能**: 1 回の投入は確保 2 回（箱と記録）、鍵 1 回、`dispatch_*_f` 1 回である。数字は測るまで書かない。

## 対象外

- tzroutine の待ちと CFRunLoop の統合、`exit()` に D-61 の待ちを守らせること（採らない案）。
- Windows の UI スレッドへの投入（W28 の範囲の外）、Linux の `is_current`、メインスレッドとワーカー M の QoS。
- 結果を待つ同期の形（`MainThread.call`）。メインスレッドから呼ぶとデッドロックする。要るなら `Channel` で返す。
- AppKit の宣言（M07）、ObjC の型（M01〜M03）、Block（M04）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | 組み込みのモジュール `MainThread`（`post`・`after`・`is_current`）を足し、名前を予約する（78 → 79）。採らない案: std のファイル（ランタイムの部品と生成の口が要る）、`Routine` に足す（投入した関数は tzroutine で走らない） | D-55 の登録。利用者の `MainThread.tz` を壊す（`docs/stability.md` 123 行） | 委任による決定（2026-11-28。利用者の指示「判断が必要なものは最高の選択肢で実装することを常に許可」に基づき、チケットの既定案を採用） |
| D2 | 型は `Routine.go`・`Time.after_func` と同じ形にし、D-59 の例外を足さない。採らない案: 遅延を最後に取る `after f d` | `post`・`is_current` は対象のない関数、`after` は `Time.after_func` と同じく期間の後に関数を取る。同じ形の 2 つの関数の引数の順をそろえる | 既定案（実装者はこの案に従う） |
| D3 | Phase 1 は macOS だけ。ほかの native は `E2002`、wasm は `E2000` の新しい kind `wasm_main_thread_unsupported`。`is_current` も macOS だけ。採らない案: wasm を `E1008` `abi_native_only` にする（ABI の形ではない）、Linux に `is_current` だけを先に足す | 使えないことを `build` で明示する（AGENTS.md）。`check` は target に依らない。コードは足さず kind だけ（D-51） | 既定案（実装者はこの案に従う） |
| D4 | ランタイムの C が `dispatch_async_f`・`dispatch_after_f` でメインのディスパッチキューに積む。採らない案: `performSelectorOnMainThread:`・`CFRunLoopPerformBlock`（採らない案の節） | NSApp・CFRunLoop・`dispatch_main` のどれでも処理され、C の関数ポインターで書ける。データシンボルの構文（M06）は要らない | 既定案（実装者はこの案に従う） |
| D5 | 投入した関数は W12 の静的コールバックと同じ境界の規則で呼ぶ。採らない案: 投入ごとに新しい境界を作って中の tzroutine を待つ | 中で起動した tzroutine の行き先をコールバックと同じにし、規則を増やさない。`main` の間にメインスレッドで待つと UI が止まる | 既定案（実装者はこの案に従う） |
| D6 | `main`・`test`・`bench` の境界の終わりに残った投入は呼ばずに drop し、終わりは投入を待たない。drop は境界の本体が返った直後、境界の待ちの前に行い、`bench` は標本ごとに印を下ろす。採らない案: (a) 境界の終わりで投入を待つ、(b) 境界の待ちの後に drop する、(c) W12 の D7 に合わせて `test`・`bench` ではプロセスの寿命の間残す | `Time.after_func` と同じく、境界の中で drop する。境界で待つ間はキューが処理されず、待てばデッドロックする（「現状」4）。待ちの後の drop では、`Drop` が起動した tzroutine を誰も待たない。`test`・`bench` の本体の後にメインのキューを処理する者はいないので、残しても呼ばれず、`Drop` が走らない。捕捉した値の Drop は必ず走る | 既定案（実装者はこの案に従う） |
| D7 | W19: `post`・`after` の関数引数は新しい位置 `Posted`（`pinned` な関数を許し、辺は `pinned` を運ばない）。採らない案: `Spawned`（UI のハンドルに触れる関数を投入できない） | メインスレッドは 1 つの固定したスレッドで、`local` のハンドルは捕捉できない | 既定案（実装者はこの案に従う） |
| D8 | 待ちと run loop を統合せず、待たない規律と、tzroutine で処理して `post` で戻す形を文書にする。採らない案: 統合（採らない案の節） | D-61 の擬似 G の意味を保つ（S3） | 既定案（実装者はこの案に従う） |
| D9 | 終了は文書の手順（`applicationShouldTerminate:` で取り消し、`stop:` と自前のイベントで `-run` を戻す。tzroutine は `Context` で止める）で扱い、`exit()` は変えない。採らない案: `atexit` で境界を待つ | 調査で `main` が正常に返ることを確かめた。`exit` の中で境界を待つのは安全でない | 既定案（実装者はこの案に従う） |
| D10 | ライブラリ出力では投入をプロセスの寿命の間残す。採らない案: export の境界の終わりで drop する | W12 の D7 と同じ。export が返った後もホストの run loop が回り、投入はそこで走る | 既定案（実装者はこの案に従う） |
| D11 | Phase 2: 効果 `main_thread`（D-53 の語彙 15 → 16）。`pinned` とは別のビットで、`main` のハンドルは `pinned` も持つ。`with_locked_thread` は境界にしない（辺が効果を運ぶ）。コールバック・`FnPtr`・export・`test`・`bench` は入口の検査つき、`post`・`after` は検査なしで使える文脈。採らない案: `with_locked_thread` への引数そのものを拒む（`main` の中の無害な使い方まで拒む）、`pinned` の意味を広げる（Windows の GDI の `local` まで縛る） | 語彙は閉じているので改訂が要る。辺で効果を運べば、tzroutine の中の誤りは起動の位置で `E1013` になる。呼ばれるスレッドを静的に知れない入口だけを実行時に守る | 委任による決定（2026-11-28。利用者の指示「判断が必要なものは最高の選択肢で実装することを常に許可」に基づき、チケットの既定案を採用） |
| D12 | Phase 2: `main` を `extern type`・`extern class` の直後だけの文脈キーワードにし、`copy`・`local`・`sync` との併用は `E0002`。採らない案: 属性（`@main_thread extern type H`） | D-55 の登録。予約語は足さない。修飾の語は W18・W19・M01 と同じ位置にそろえる | 委任による決定（2026-11-28。利用者の指示「判断が必要なものは最高の選択肢で実装することを常に許可」に基づき、チケットの既定案を採用） |
| D13 | Phase 2: 入口の検査は `main_thread` を持つ本体の包みだけに入れ、Apple の target で `pthread_main_np` に下げる。ほかの target で要れば `E2002`（wasm は `E2000`）。費用は測って記録する。採らない案: すべての包みに入れる | ほかの包みの IR を変えない。Linux は `gettid() == getpid()` で書けるが、`post` のない target に先に足さない | 既定案（実装者はこの案に従う） |
| D14 | Phase 2: `E1013` に `main_thread_code_spawned`・`main_thread_function_value`。同じ関数値に `pinned_*` を重ねない。採らない案: 両方を出す | 1 つの誤りに 1 つの診断。メインスレッドの文のほうが直し方が具体的 | 既定案（実装者はこの案に従う） |
| D15 | Phase 2: `main` の型への M02 の送信だけ、行末の印でブロッキングの包みを省く。採らない案: `main_thread` の関数の中のすべての `extern` 呼び出しから外す（測ってから決める） | メインスレッドは M にならず、包みは何もしない。受け手の型が根拠になる呼び出しに限る | 既定案（実装者はこの案に従う） |
| D16 | Phase 3: Send・Sync の値の預け先（案 `Anchor.create_shared`・`shared_token`・`read_shared`）の名前と意味は承認で決め、書き換えを持たせない案を既定にする。採らない案: `Anchor.update` をどのスレッドからでも呼べるようにする（`Busy` と再入の規則が変わる） | D-55 の登録と W23 の対象外の解除。M14 の完了ハンドラーが要る | 委任による決定（2026-11-28）: 名前は案のとおり `Anchor.Shared`・`Anchor.create_shared`・`Anchor.shared_token`・`Anchor.read_shared`。M14 の D5・S1 のために `Anchor.detach_shared :: own Anchor.Shared<'a> -> i64` と `Anchor.release_shared :: i64 -> bool` を足した（D18） |
| D17 | `--trap-mode return` と `--freestanding` は `MainThread` と組み合わせられない（`E2000` `option_conflict`）。採らない案: 投入した関数のトラップだけをプロセスの終了にする | 投入した関数は投入した呼び出しの外で走り、トラップを返す先がない。部品には C のライブラリと libdispatch が要る | 既定案（実装者はこの案に従う） |
| D18 | Phase 3: 所有者を手放す `Anchor.detach_shared` と、鍵で解放する `Anchor.release_shared` を足す。`release_shared` は手放された預け先だけを解放し、所有者のある預け先には `false` を返す。採らない案: 鍵だけで所有者のある預け先も解放できるようにする（所有者の drop と鍵の解放のどちらが値を捨てるかが曖昧になる） | M14 の S1（項目を作った関数の外で生かし、鍵で解放する操作がないと M14 は止まる）。所有は「所有者の値」か「鍵」のどちらか一方にある | 委任による決定（2026-11-28） |
| D19 | Phase 3: 所有者の drop と `release_shared` は読み取りの終わりを待たない。以後の読み取りを `Stale` にし、値は最後の読み取りの終わりにそのスレッドで drop する（W23 の DYING と同じ仕組み）。採らない案: 仕様の案の「読み終わりを待ってから drop する」 | 待つ形は、読み取りのコールバックの中で所有者を drop する（または `release_shared` を呼ぶ）と自分を待って止まる。値は Send なので別のスレッドで drop してよい | 委任による決定（2026-11-28。仕様の Phase 3 の文から外れる） |
| D20 | Phase 2: 入口の検査は `@tz.main_thread.check` に下げ、ランタイムの `tsuzuri_mainthread_is_current`（中身は `pthread_main_np`）を呼ぶ。採らない案: IR で `pthread_main_np` をじかに宣言する（利用者の同じ symbol の `extern` と型が違うと LLVM が 2 つ目の `declare` を拒む） | driver の Phase 1 の判定（Apple でない native は `E2002`、wasm は `E2000`）がそのまま検査にも効き、D13 の診断が 1 か所で決まる。`TrapKind` は増やさない（実行時の種類は `Assert`） | 委任による決定（2026-11-28。D13 の「`pthread_main_np` に下げる」の形だけが違う） |
| D21 | Phase 2: `main` の基底を持つクラスは `main` でなければならない（`E1008` の新しい kind `objc_class_main_base`）。`main` のクラスへのメッセージ送信は、型に物体が現れないクラスメソッドも `main_thread` の源にする。採らない案: 型が届く関数だけを源にする（仕様の文のとおり） | 基底のクラスの物体をメインスレッドの外で使える穴と、D15 の「送信はメインスレッドで行われる」の前提が崩れる穴を塞ぐ | 委任による決定（2026-11-28。D-55 に kind を足した） |
| D22 | Phase 2: D15 の印は行末のコメントではなく、呼び出しの属性 `"tz-main-thread"` にする | `!tz.site`・`!dbg` を行末から読む後工程の pass を壊さない | 委任による決定（2026-11-28） |

## 実装と検証（2026-11-28）

ブランチ `feat/m09-main-thread-post`（`main` = `faa091b2` から）。Phase 1・2・3 をすべて実装した。承認の要る決定（D1・D11・D12・D16）は、利用者の委任（「判断が必要なものは最高の選択肢で実装することを常に許可」）で決めた（決定事項の表）。

### 実装した範囲

- **Phase 1**: `Builtin::MainThreadPost`・`MainThreadAfter`・`MainThreadIsCurrent`（`Builtin::ALL` 384 → 387）、`RESERVED_MODULES` に `MainThread`（82 → 83）、効果の分類（`task`。`post`・`after` は `alloc` と callbacks）、`Position::Posted`（`pinned` を運ばない辺。`Owned.function` の直接の呼び出しを剥がす。パイプラインの 2 つの形も）、`src/runtime/mainthread-darwin.c`（未実行の記録の FIFO、`boundary_done`・`boundary_open`、飽和した期限は積まない）、`src/llvm_mainthread.rs`（`@tz.mainthread.run`。spawn するプログラムでは `tsuzuri_tz_callback_scope_enter`・`_leave` で挟む）、`@main`・`tsuzuri_main`（IO の entry）・`tsuzuri_test_run`・`tsuzuri_bench_sample` の境界の drop、driver と test・bench の runner（C の単位へは使うときだけ、いちばん後ろに足す。wasm は `E2000` `wasm_main_thread_unsupported`、Apple でない native は `E2002`、`--trap-mode return`・`--freestanding` は `E2000` `option_conflict`）。
- **Phase 2**: `HandleKind::Main`・`ObjcMain`、`extern type main H`・`extern class main C : Base`（`ExternClassDecl::main`、`FRONTEND_FORMAT` 37 → 38）、`CheckedModule::main_bound`、`Effect::MainThread`（`Effect::ALL` 15 → 16、`EffectSet::ALL` は `u16::MAX`、内部の `BITS` 18 → 19）、辺が運ぶ効果を `Boundary`（`None`・`Locked`・`Entry`）で表す形への整理、kind `main_thread_code_spawned`・`main_thread_function_value`（`E1013`）と `objc_class_main_base`（`E1008`）、入口の検査（コールバック・`FnPtr`・export の 2 種類の包み・`test`・`bench`・ライブラリの `tsuzuri_main`）、`main` のクラスへの送信の `"tz-main-thread"`（D15）、`api-diff` の束縛の強さの順序。
- **Phase 3**: `Builtin::AnchorCreateShared`・`AnchorReadShared`・`AnchorDetach`・`AnchorReleaseShared`（`Builtin::ALL` 387 → 391）、`std/Anchor.tz` の `Shared<'a>`・`shared_token`・`detach_shared`、ランタイムの `tsuzuri_tz_anchor_create_shared`・`_begin_shared`・`_detach`・`_release_shared`（W23 の表を共有し、所有スレッド 0 が共有の預け先）。

### チケットから外れた判断

1. E2E の fixture は `tests/fixtures/main_thread/` ではなく `tests/main_thread/app/`・`tests/main_thread/handles/` と `tests/main_thread_host.c` に置いた。`tests/fixtures/` の下はリポジトリ全体のテストがホストの target で build するので、Apple だけの機能の fixture は置けない（M01 の `tests/objc/` と同じ）。
2. `E2002`・`E2000` の文に直し方を足した（`MainThread is available only on macOS targets; build for aarch64-apple-darwin or on a Mac`、`MainThread is not available on WebAssembly; build for a macOS target`）。入口の検査だけが理由のときは、検査を名指しする別の文にした。
3. 境界の終わりの drop は投入の順にした（仕様は順序を決めていない）。
4. `extern type` の修飾の衝突の文は、`copy`・`local` だけのときは W19 の文のままにし、`main` が関わるときだけ 3 つを挙げる（`tests/thread_bound_handles.rs` の期待値を変えない）。
5. `HandleKind::Main` は `Local` の次に置いた（宣言が書く tag を 0〜3 に連ねる。既存の 5 つの順序は変わらない）。
6. D18〜D22（決定事項の表）。
7. `docs/language.md` の効果の語彙は 14 個（`pinned` が抜けていた）と書かれていたので、16 個に直した。
8. D13 の「費用は測って記録する」は未実施（数字は書いていない）。

### 実行したコマンドと結果

- `cargo test --locked --test main_thread`（r1〜r12）: 13 passed。`--test main_thread_handles`（Phase 2）: 10 passed。`--test shared_anchor`（Phase 3）: 4 passed。`--test thread_bound_handles`: 変更なしで通過。
- `node tests/main_thread.mjs target/debug/tsuzuri`（`TSUZURI_BUNDLED_CLANG` に同梱の `tsuzuri-clang`）: 51 checks passed（7 場面 × `-O0`・`-O3` × `TSUZURI_MAXPROCS=1`・`4`、`tsuzuri test`・`bench`、Phase 2 の 3 場面、Phase 3 の 1 場面、`--allocator counting`、同梱の zig でのリンク）。
- `node tests/features.mjs target/debug/tsuzuri anchor`: native・WASM の `-O0`・`-O3` で 13 cases（共有の 4 件を含む）。
- S1: `tests/fixtures/*`・`examples/*` の `--emit llvm`（native・`--target wasm32`）・`--emit header`・`check --json` の 1,269 ファイルを、変更前のバイナリと Phase 1・Phase 2 の後のバイナリで `diff -rq` し、差分なし。S2: C の単位へは `MainThread` を使うプログラムでだけ、既存の部品の後ろに足す。
- `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`sh scripts/check-runtime-includes.sh`（65 files）、`node scripts/gen-docs.mjs --check`、`node scripts/check-cli-docs.mjs`、`cargo nextest run --locked`（T1）: 結果は PR の説明に書く。

### 更新した文書

- `docs/language.md`（不透明ハンドル、Objective-C、`MainThread` と共有の `Anchor`、効果の語彙、予約モジュール、生成した診断の表）、`README.md`（機能の一覧と検証の表）、`_features/GUIDE.md`（D-53・D-67）。
- Tsuzuri-Reference: `docs/stdlib/sync/mainthread.md`（新規）、`docs/index.md`、`docs/stdlib/sync/routine.md`、`docs/stdlib/sys/time.md`、`docs/stdlib/memory/anchor.md`、`docs/compiler/native-interop.md`、`docs/compiler/objective-c.md`、`docs/compiler/platforms.md`、`docs/compiler/diagnostics.md`（生成）。

### 変更したファイル

`src/check.rs`、`src/stdlib.rs`、`src/effects.rs`、`src/pure_rules.rs`、`src/kg/effects.rs`、`src/pinned_rules.rs`、`src/ownership.rs`、`src/polymorph.rs`、`src/capabilities.rs`、`src/syntax.rs`、`src/parser.rs`、`src/syntax_codec.rs`、`src/frontend_cache.rs`、`src/formatter.rs`、`src/api_diff.rs`、`src/objc.rs`、`src/diagnostic_kinds.rs`、`src/llvm.rs`、`src/llvm_mainthread.rs`（新規）、`src/llvm_abi.rs`、`src/llvm_anchor.rs`、`src/llvm_imports.rs`、`src/llvm_traps.rs`、`src/driver.rs`、`src/test_runner.rs`、`src/cache.rs`、`src/runtime/mainthread-darwin.c`（新規）、`src/runtime/tzroutine.c`、`std/Anchor.tz`、`tests/main_thread.rs`・`tests/main_thread_handles.rs`・`tests/shared_anchor.rs`・`tests/main_thread.mjs`・`tests/main_thread/app/Main.tz`・`tests/main_thread/handles/Main.tz`・`tests/main_thread_host.c`（新規）、`tests/fixtures/anchor/Main.tz`、`tests/features.mjs`、`tests/std_argument_order.rs`、`.github/workflows/targets.yml`、`README.md`、`docs/language.md`、`_features/GUIDE.md`。
