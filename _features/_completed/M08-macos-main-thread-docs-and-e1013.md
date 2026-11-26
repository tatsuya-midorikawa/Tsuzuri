# M08: macOS の GUI に `Routine.with_locked_thread` を勧める文書と `E1013` の誤り（with_locked_thread はワーカー M に固定し、メインスレッドにはならない）

| 項目 | 内容 |
| --- | --- |
| ID | M08 |
| 優先度 | P1（文書と診断が、macOS では実行時に落ちる直し方へ導く。`check` は通り、実行すると AppKit が `NSInternalInconsistencyException` を投げてプロセスが終わる） |
| 規模 | S |
| 依存 | ([W28](W28-ui-on-fibers.md))（同じ段落を W28 の D1 も改める。後に入るほうが文を合わせる） |
| 後続 | –（弱い後続: [M09](M09-main-thread-dispatch-and-handles.md)） |
| 状態 | done |
| 起票 | 2026-11-26（D-67。利用者の依頼「「どのチケットにもない壁」と「調査中に見つけたコンパイラの不具合（自分で再現済み）」のそれぞれについて @_features/ に開発チケットまたは修正チケットを作成して」から起票。調査で報告した「壁 3（文書と E1013 が、macOS では誤った直し方へ導く。check は通るが実行時に NSInternalInconsistencyException）」に当たる。HEAD `804b7970` で確認） |
| 承認 | 不要（文書と診断の文の誤りの修正。新しい名前を足さず、`E1013` のコード・kind・位置も変えない）。台帳は [GUIDE D-67](GUIDE.md#d-67-tsuzuri-だけで-macos-のデスクトップアプリを書くm01m22)（承認の要らない M08 はすぐに進めてよい） |
| 手本にする既存実装 | `src/pinned_rules.rs` の `report_spawned`・`report_function_value`（`;` で区切って直し方を並べる英語の診断の文体）、`tests/thread_bound_handles.rs` の `message.contains(…)` の検査、[W03](W03-windows-interop-doc-fixes.md)（言語リファレンスの誤りだけを直す修正チケット） |
| 主な影響ファイル | `src/pinned_rules.rs`、`src/polymorph.rs`、`src/explain/codes/E1013.md`、`tests/thread_bound_handles.rs`、`tests/tzroutine.mjs`、`_tsuzuri/language-reference/compiler/native-interop.md`、`_tsuzuri/language-reference/built-in-types-and-modules/routine.md`、`docs/language.md` |

## 実装者向け早見

- 範囲: (1) `native-interop.md` 630 行と `routine.md` 130 行から、GUI の UI スレッドに `with_locked_thread` を勧める文を外し、固定する先がワーカー M でメインスレッドではないこと、メインスレッドが要る API の置き場、戻す API がまだないこと（M09）を書く（仕様 2）。(2) `E1013` の 2 つの文の末尾に、target に依らない 1 文を足す（仕様 1）。(3) `E1013.md` に 1 行（仕様 3）。(4) 文書の主張を固定する macOS の E2E を 1 つ。
- 最初に読む: 「現状」の 1〜5 → `src/pinned_rules.rs` 710〜722 行（`report_spawned`）→ `src/polymorph.rs` 3755〜3775 行（`local` ハンドルの枝。3769 行が `task_requires_send` の文）→ `native-interop.md` 618〜632 行 → `routine.md` 128〜155 行 → [W28](W28-ui-on-fibers.md) の仕様と D1。
- 最初に書くテスト: `tests/thread_bound_handles.rs` の `main_thread_advice_in_thread_bound_messages`（2 つの文の末尾に新しい文があり、`kind`・`code`・`span` が今と同じ）。
- 触ってはいけない: `E1013` のコード・kind（`pinned_code_spawned`・`task_requires_send`）・主の位置・`related`・`help`、既存の文の先頭の部分（`tests/thread_bound_handles.rs` 176・303 行が `contains` で固定している）、`pinned_function_value` の文、ランタイムと生成 IR、`native-interop.md` 632 行目の「GUI は言語の一部ではありません」。
- 完了の見分け方: 「現状」2 のプログラムの `check` が、今の文の後に仕様 1 の 1 文を出す。`native-interop.md`・`routine.md` に GUI の UI スレッドへ `with_locked_thread` を勧める文が残っていない。

## 目的

Tsuzuri の `main` は、プロセスのメインスレッドで擬似 G として走る。`Routine.with_locked_thread` は、呼んだ時点で tzroutine を走らせている OS スレッドに固定するだけである。tzroutine の中で呼べば、固定する先はランタイムのワーカー M で、メインスレッドではない。

ところが、言語リファレンスの 2 か所は、GUI の UI スレッドに結び付いた API を tzroutine から呼ぶときは `with_locked_thread` で固定せよ、と書く。`E1013`（`pinned_code_spawned`）の文も「新しい tzroutine の中で `Routine.with_locked_thread` を使え」と勧める。macOS の AppKit はプロセスのメインスレッドでしか動かない。勧めに従って直すと型検査は通り、実行時に AppKit が例外を投げてプロセスが終わる。型検査はこの誤りを捕まえない（W19 の `local` は「どこかの固定したスレッド」しか表せない）。

このチケットは、文書と診断の文を、今のコンパイラとランタイムが実際にすることに合わせる。新しい API と型の検査は足さない（M09）。

## 着手条件と停止条件

### 着手条件

- `node scripts/repo-kb.mjs brief M08`。承認は要らない（D-67 で M08 はすぐに進めてよい）。
- 「現状」の 1〜5 を HEAD で繰り返し、同じ結果になることを確かめる。行番号は `grep -n` で取り直す。食い違えば、このチケットを直してから始める。
- [W28](W28-ui-on-fibers.md) の状態を `grep -n '^| W28 ' _features/README.md` で見る。W28 が先にマージされていれば、その文（D1: UI は `main` を既定にする、固定した tzroutine の UI は条件付きで使える）を読んでから、この文を合わせる（D5）。
- macOS（arm64）の開発機。E2E と「現状」の 1・4 の再現に要る。

### 停止条件

- **S1** 文を変えるために、`E1013` の kind・コード・主の位置・`related`・`help` のどれかを変える必要が出た（D-51 と `docs/stability.md` の「コードと位置の意味は保ちます」に触れる）。
- **S2** `tests/thread_bound_handles.rs` 以外のテスト（`tests/diagnostic_baseline`、適合性テスト、`tests/hints.rs`、`tests/explain.rs`）の期待が、文の変更で壊れる。どこが文を固定しているかを報告する。
- **S3** `src/explain/codes/E1013.md` が着手時に 1,421 文字を超えていて、D4 の 1 行（79 文字）を足すと `tests/explain.rs` の `CODE_PAGE_LIMIT`（1,500）を超える。ほかの文を削らずに止めて報告する。
- **S4** W28 が先に入り、その文が macOS の AppKit にも「固定した tzroutine の UI を使える」と読める形で書かれていて、D5 の段落を分ける形では食い違いが消えない。
- **S5** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `804b7970` で確認）

作業は `target/scratch/m08/` で行い、確認後に消した。macOS 27.2.1 arm64、`target/debug/tsuzuri`（HEAD で作り直した dev のバイナリ）。診断の出力は、ファイルのパスを `Main.tz` に縮め、ソースの行の引用を省いて書く。

### 1. `main` はメインスレッド、`with_locked_thread` はワーカー M

`src/llvm.rs` の `console_def_main`（9968 行から）は、C の `@main` から `@tz.fn.<main>` を直接呼ぶ（9994〜10006 行。tzroutine を起動するプログラムでは境界の `scope_enter`・`leave` で挟むだけで、スレッドを作らない）。`src/runtime/tzroutine.c` 2196〜2202 行の `tsuzuri_tz_lock_thread` は、今の M に G を固定し、M のない擬似 G では何もしない（注記「A pseudo-G is its thread already」）。

```tsuzuri
@trusted
extern "pthread_main_np" def pthread_main_np :: unit -> i32

def main :: unit -> i32 = \() ->
    do! IO.writeln $"main: pthread_main_np={pthread_main_np ()}"
    let in_main_locked = Routine.with_locked_thread (\() -> pthread_main_np ())
    do! IO.writeln $"main inside Routine.with_locked_thread: pthread_main_np={in_main_locked}"
    let done: Channel<i32> = Channel.make 1
    let out = Channel.share done
    tzroutine {
        let plain = pthread_main_np ()
        let locked = Routine.with_locked_thread (\() -> pthread_main_np ())
        do! IO.writeln $"tzroutine: pthread_main_np={plain} with_locked_thread={locked}"
        Channel.send out locked
    }
    match Channel.recv done with
    | Maybe.Some v -> v
    | Maybe.None -> 9
```

`target/debug/tsuzuri build <dir> -o a.out` の後、`TSUZURI_MAXPROCS=1` と `4` で実行した。どちらも同じ出力で、終了コードは 0 だった。

```text
main: pthread_main_np=1
main inside Routine.with_locked_thread: pthread_main_np=1
tzroutine: pthread_main_np=0 with_locked_thread=0
```

### 2. `E1013` の `pinned_code_spawned` は `with_locked_thread` を勧める

```tsuzuri
extern type local Window

@trusted
extern "host_window_create" def window_create :: unit -> Window

@trusted
extern "host_window_close" def window_close :: own Window -> unit

def open_window :: unit -> unit = \() ->
    let w = window_create ()
    window_close w

def main :: unit -> i32 = \() ->
    tzroutine {
        open_window ()
    }
    0
```

`target/debug/tsuzuri check <dir>` の結果（終了コード 1）:

```text
Main.tz:14:5: error[E1013]: this starts code that uses the thread-bound extern type 'Window' on another tzroutine or thread, which may run on any OS thread; do that work inside 'Routine.with_locked_thread' in the new tzroutine, or run it on the calling thread
  note: calls 'Main.open_window'
  note: uses thread-bound extern type 'Window' here
  help: reaches 'Window' via '$once.4' -> 'Main.open_window'
```

`--json` では `"kind":"pinned_code_spawned"`、主の位置は `tzroutine {` の式（14:5〜16:6）、`related` は `via_call` と `note` の 2 件、`help` は上の連なりである。文は `src/pinned_rules.rs` 714 行（`report_spawned`、710〜722 行）にある。`rg -n 'do that work inside' src tests _tsuzuri docs spec` は 714 行だけを返す。

### 3. 勧めに従うと `check` が通る

2 の `main` の本体を次に変えると、`check` は終了コード 0 になる（W19 は `with_locked_thread` の関数引数を固定する位置として受け入れる。`src/pinned_rules.rs` 35 行の `Position::Pinning`）。

```tsuzuri
def main :: unit -> i32 = \() ->
    tzroutine {
        Routine.with_locked_thread (\() -> open_window ())
    }
    0
```

`local` ハンドルを tzroutine へ送る形の `E1013`（`task_requires_send`）も同じ方向を勧める。次のプログラムの `check` は、終了コード 1 で次を出す（`src/polymorph.rs` 3769 行）。

```tsuzuri
extern type local Window

@trusted
extern "host_window_create" def window_create :: unit -> Window

def main :: unit -> i32 = \() ->
    let w = window_create ()
    let windows: Channel<Window> = Channel.make 1
    0
```

```text
Main.tz:8:36: error[E1013]: values that move to another tzroutine or thread must be Send; Main.Window holds a thread-bound extern handle ('extern type local'), which must stay on the OS thread that received it; use it there, or do the work inside 'Routine.with_locked_thread'
```

`--json` では `"kind":"task_requires_send"` で、主の位置は `Channel.make` の呼び出し（8:36〜8:48）、`related` と `help` はない。

`pinned_function_value` の文（`src/pinned_rules.rs` 728 行）も `with_locked_thread` に触れるが、勧めるのは「関数を直接呼ぶ」「`with_locked_thread` にそのまま渡す」「`for`・`match` を使う」で、どれも呼んだ側のスレッドで走らせる形である（D2）。

### 4. AppKit をワーカー M で呼ぶと落ちる

C を書かずに、AppKit を `dlopen` で読み込み、tzroutine の中の `with_locked_thread` で `[[NSWindow alloc] init]` を送る。

```tsuzuri
@trusted
@count(1, none)
extern "dlopen" def dlopen :: ref utf8string -> i32 -> i64

@trusted
@count(1, none)
extern "objc_getClass" def objc_get_class :: ref utf8string -> i64

@trusted
@count(1, none)
extern "sel_registerName" def sel_register_name :: ref utf8string -> i64

@trusted
extern "objc_msgSend" def send0 :: i64 -> i64 -> i64

@trusted
extern "pthread_main_np" def pthread_main_np :: unit -> i32

def make_window :: unit -> i64 = \() ->
    do! IO.writeln $"make_window: pthread_main_np={pthread_main_np ()}"
    let cls = objc_get_class (ref u8"NSWindow\0")
    let allocated = send0 cls (sel_register_name (ref u8"alloc\0"))
    send0 allocated (sel_register_name (ref u8"init\0"))

def main :: unit -> i32 = \() ->
    let appkit = dlopen (ref u8"/System/Library/Frameworks/AppKit.framework/AppKit\0") 2
    do! IO.writeln $"main: AppKit loaded={appkit != 0} pthread_main_np={pthread_main_np ()}"
    let done: Channel<i64> = Channel.make 1
    let out = Channel.share done
    tzroutine {
        let w = Routine.with_locked_thread (\() -> make_window ())
        Channel.send out w
    }
    match Channel.recv done with
    | Maybe.Some w -> if w != 0 then 0 else 1
    | Maybe.None -> 2
```

`target/debug/tsuzuri build <dir> -l objc -o a.out` の後、`perl -e 'alarm 15; exec @ARGV' ./a.out` の結果（終了コード 134）:

```text
main: AppKit loaded=true pthread_main_np=1
make_window: pthread_main_np=0
*** Terminating app due to uncaught exception 'NSInternalInconsistencyException', reason: 'NSWindow should only be instantiated on the main thread!'
*** First throw call stack:
(
)
libc++abi: terminating due to uncaught exception of type NSException
```

同じ `make_window ()` を `main` の本体で直接呼ぶ版は `make_window: pthread_main_np=1` を出し、終了コード 0 だった。どちらの後も、残ったプロセスはなかった（`pgrep`）。

### 5. 文書の勧め

- `_tsuzuri/language-reference/compiler/native-interop.md` 630 行（節「スレッドとホストの責任」、618 行から）: 「スレッドに結び付いたホストの API（GUI の UI スレッド、OpenGL の文脈、スレッドローカルな状態を持つ C のライブラリ）を tzroutine から呼ぶときは、`Routine.with_locked_thread`（routine.md へのリンク）で、関数の間その tzroutine を OS スレッドに固定します。」。632 行は「GUI は言語の一部ではありません。デスクトップの画面は、ホストがオブジェクトを呼ぶ側に置きます。」で、正しい（変えない）。
- `_tsuzuri/language-reference/built-in-types-and-modules/routine.md` 130 行（節「Routine.with_locked_thread」、128 行から）: 「GUI の UI スレッド、OpenGL の文脈、スレッドローカルな状態を持つ C のライブラリなど、…そうした API を呼ぶ間は `Routine.with_locked_thread` でスレッドに固定します。」。同じ節の 155 行の箇条は「tzroutine でないスレッド（`main` のトップレベル、`export` を呼んだホストのスレッド）は、もともとそのスレッドで走るので、何もしません。」で、正しい。
- `rg -n "with_locked_thread" _tsuzuri docs README.md src/explain` のほかの当たり（`routine.md` 12・25・176 行、`tzroutine.md`、`anchor.md`、`native-interop.md` 224 行、`docs/language.md` 482・2720 行、`docs/architecture.md`、`README.md`）は機能の説明で、勧めではない。`src/explain/codes/E1013.md`（1,412 文字）は `with_locked_thread` に触れない。
- メインスレッドへ処理を戻す API はない（`rg -nw 'MainThread' src std _tsuzuri` は 0 件。`-w` を付けないと `src/runtime/wasm-threads.mjs` の Node.js の `isMainThread` に当たる）。調査では、`dlsym` で `_dispatch_main_q` を引いて `dispatch_async_f` を呼ぶ回避策が動いたが、ABI の知識に頼る（D3）。
- [W28](W28-ui-on-fibers.md)（todo）の D1（179 行。仕様は 57 行）は、文書の既定を「UI は `main` のスレッド」にし、固定した tzroutine の UI を条件付きで残す。W28 は Windows の Fiber の話で、POSIX の挙動を触らない（22 行）。

### 6. 文を固定しているテスト

- `tests/thread_bound_handles.rs` 303 行は `pinned_code_spawned` の文の先頭の部分を、176 行は `local` の `task_requires_send` の文が `Routine.with_locked_thread` を含むことを、`contains` で確かめる。`rg -n 'thread-bound extern' tests spec _tsuzuri docs` はこのファイルだけを返す（`tests/diagnostic_baseline` にもない）。この 2 つの kind に `fix` はない。
- `tests/explain.rs` 412 行の `CODE_PAGE_LIMIT` は 1,500 文字、`Fixes` は 2〜6 個（440〜446 行）。`E1013.md` の `Fixes` は今 5 個。

## 変えてはいけない意味

- `E1013` の kind・コード・主の位置・`related`・`help`。診断が出る条件と順序。
- 既存の 2 つの文の、新しい文より前の部分（テストと利用者の検索を壊さない）。
- 生成 IR・object・ランタイム。このチケットは文字列と文書だけを変える。
- `native-interop.md` 632 行目の非目標（GUI は言語の一部ではない）。

## 仕様

### 1. 診断の文

target に依らない 1 文を、次の 2 つの文の末尾に `; ` で足す。足す文は 2 つとも同じ。

```text
an API that needs the process main thread (AppKit on macOS) must run in 'main' or in a callback that the main thread calls
```

- `pinned_code_spawned`（`src/pinned_rules.rs` 714 行）の新しい文: `this starts code that uses the thread-bound extern type '{handle}' on another tzroutine or thread, which may run on any OS thread; do that work inside 'Routine.with_locked_thread' in the new tzroutine, or run it on the calling thread; an API that needs the process main thread (AppKit on macOS) must run in 'main' or in a callback that the main thread calls`
- `local` ハンドルの `task_requires_send`（`src/polymorph.rs` 3769 行）の新しい文: `values that move to another tzroutine or thread must be Send; {type} holds a thread-bound extern handle ('extern type local'), which must stay on the OS thread that received it; use it there, or do the work inside 'Routine.with_locked_thread'; an API that needs the process main thread (AppKit on macOS) must run in 'main' or in a callback that the main thread calls`
- 型検査は target を知らない（`src/check.rs` は target を引数に取らない）ので、文は target に依らない。Windows・Linux の利用者にも出るが、誤りにはならない（メインスレッドが要る API を使わなければ当たらない）。
- `pinned_function_value`（728 行）、関数値の捕捉の文（`src/polymorph.rs` 3760 行。`E1005` の `capture_unsendable_in_function_value`）、`holds_thread_bound` の枝の文（3776 行から）は変えない（D2）。

### 2. 言語リファレンス

- `native-interop.md` の 630 行を 2 段落にする。1 段落目: スレッドに結び付いた API（OpenGL の文脈、スレッドローカルな状態を持つ C のライブラリ）を tzroutine から呼ぶときは `with_locked_thread` で固定する。固定する先は、呼んだ時点で tzroutine を走らせている OS スレッドで、tzroutine の中ではランタイムのワーカーのスレッドである。2 段落目: プロセスのメインスレッドでしか呼べない API（macOS の AppKit）は、`main` の本体と、メインスレッドで呼ばれるコールバック（AppKit のデリゲート、`NSTimer`、メインのディスパッチキューに積まれた関数）から呼ぶ。`main` はプロセスのメインスレッドで走る。tzroutine の中で `with_locked_thread` を使って呼ぶと、型検査は通るが、実行時に壊れる（例えば `NSWindow` の生成は `NSInternalInconsistencyException` を投げてプロセスを終わらせる。実行時に検査しない API でも、メインスレッドの外での呼び出しは保証されない）。tzroutine からメインスレッドへ処理を戻す API は今はない（計画は M09。ページからのリンクは `../../../_features/M09-main-thread-dispatch-and-handles.md`）。「GUI の UI スレッド」の例は 1 段落目から外す。
- `routine.md` の 130 行から「GUI の UI スレッド」を外し、同じ節に 1 段落を足す: 固定する先は呼んだ時点の OS スレッドで、tzroutine の中ではワーカーのスレッド、`main` の中では何もしない。プロセスのメインスレッドが要る API は `main` かメインスレッドのコールバックから呼ぶ（`native-interop.md#スレッドとホストの責任` へリンク）。12 行目の箇条に「tzroutine の中で呼んでも、プロセスのメインスレッドへは移りません」を足す（`main` の中で呼べば、もともとメインスレッドのまま。「現状」1）。
- `docs/language.md` 2720 行の `Routine.with_locked_thread` の説明に、同じ内容の括弧書きを足す（「固定する先は呼んだ OS スレッドで、tzroutine の中ではワーカーのスレッド。プロセスのメインスレッドへは移さない」）。
- 書くのは今のコンパイラとランタイムがすることだけである。`dlsym` の回避策は書かない（D3）。

### 3. `src/explain/codes/E1013.md`

`## Fixes` の末尾に次の 1 行を足す（改行を含めて 79 文字。今の 1,412 文字から 1,491 文字になる）。

```text
- Run main-thread APIs (AppKit) in `main` or in a callback on the main thread.
```

## 設計

| ファイル | 変更 |
| --- | --- |
| `src/pinned_rules.rs` | `report_spawned` の `format!` の文の末尾に仕様 1 の文を足す。ほかは変えない |
| `src/polymorph.rs` | `holds_local_handle` の枝の `TaskRequiresSend` の `format!`（3769 行）の末尾に同じ文を足す |
| `src/explain/codes/E1013.md` | 仕様 3 の 1 行 |
| `tests/thread_bound_handles.rs` | 新しいテスト（テスト計画） |
| `tests/tzroutine.mjs` | macOS だけの E2E を 1 つ（テスト計画） |
| 文書 | 仕様 2 |

- 同じ英文を 2 か所で使うので、`src/pinned_rules.rs` に `pub(crate) const MAIN_THREAD_ADVICE: &str` を置き、両方の `format!` から `{}` で参照する（M09 の Phase 2 の文も使える）。生成する文字列は、リテラルを 2 回書いた場合と同じである。
- 新しい kind・コード・`related` の役割は足さない（D-51）。

## 前提とする他チケットのインターフェース

- なし（W28 は弱い依存。D5 の文の合わせ方だけに関わる）。

## 他チケットへ提供するインターフェース

- [M09](M09-main-thread-dispatch-and-handles.md) の Phase 1: 2 つのページに置いた「メインスレッドへ戻す API は今はない（計画は M09）」の文。M09 の Phase 1 の PR が、`MainThread.post`・`after` の説明に置き換える。Phase 2: 定数 `MAIN_THREAD_ADVICE`。
- [W28](W28-ui-on-fibers.md): 2 段落の構成（一般の固定の段落と、プロセスのメインスレッドの段落）。W28 が後なら、Windows の Fiber の条件は 1 段落目に足す（D5）。

## 実装手順

1. **変更**: なし（ブランチ `fix/m08-macos-main-thread-docs`）。
   **内容**: 着手条件を確かめる。「現状」の 1〜4 を `target/scratch/m08/` で繰り返す。
   **確認**: 出力が「現状」と同じ。違えば、このチケットの「現状」を直す。
2. **変更**: `tests/thread_bound_handles.rs`。
   **内容**: テスト計画の Rust のテストを先に書く。
   **確認**: `cargo test --locked --test thread_bound_handles main_thread_advice` が `running 1 test` で失敗する。
3. **変更**: `src/pinned_rules.rs`、`src/polymorph.rs`。
   **内容**: 仕様 1 と設計。
   **確認**: 手順 2 のテストが通る。`cargo test --locked --test thread_bound_handles` の全件が通る（`running N tests` の N を控える）。T0（`cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`）。
4. **変更**: `src/explain/codes/E1013.md`。
   **内容**: 仕様 3。
   **確認**: `cargo test --locked --test explain` が通る。`wc -m src/explain/codes/E1013.md` が 1,500 以下（S3）。`target/debug/tsuzuri explain E1013` に新しい箇条が出る。
5. **変更**: `tests/tzroutine.mjs`。
   **内容**: テスト計画の E2E。
   **確認**: `node scripts/slot.mjs -- cargo build` の後、`node tests/tzroutine.mjs target/debug/tsuzuri` が macOS で通る。
6. **変更**: 「ドキュメント」の 3 つのファイル。
   **内容**: 仕様 2。W28 が先に入っていれば D5 のとおり合わせる。
   **確認**: `node scripts/check-docs.mjs _tsuzuri/language-reference/compiler/native-interop.md _tsuzuri/language-reference/built-in-types-and-modules/routine.md`。`rg -n 'UI スレッド' _tsuzuri/language-reference/compiler/native-interop.md _tsuzuri/language-reference/built-in-types-and-modules/routine.md` が `with_locked_thread` を勧める文を返さない。
7. **変更**: なし。
   **内容**: T1（`node scripts/slot.mjs -- cargo nextest run --locked`）と T2（変えたページの `check-docs`）。`target/scratch/m08/` を消す。PR に `ci:full` を付ける（macOS の E2E を PR の時点で走らせるため。`targets.yml` は `ci:full` か main への merge の後にしか動かない）。
   **確認**: すべて通る。

## テスト計画

### Rust（`tests/thread_bound_handles.rs`）

- `main_thread_advice_in_thread_bound_messages`: 「現状」2 のプログラム（ホストの関数はリンクしないので `check_err` で足りる）の診断が、kind `PinnedCodeSpawned`、コード `E1013` で、文が仕様 1 の新しい文と一致する（期待値はテストの中に英文で書き、出力を写さない）。主の位置は `tzroutine` の位置（ソースの `find("tzroutine")` で求める）。`related` が 2 件。
- 同じテストで、「現状」3 の `Channel<Window>` の診断が kind `TaskRequiresSend` で、文が仕様 1 の新しい文と一致する。
- 同じテストで、`pinned_function_value` の文が `main thread` を含まない（D2 の固定）。
- 変わらないもの: 176 行と 303 行の `contains`、`tests/diagnostic_baseline`、適合性テスト。

### E2E（`tests/tzroutine.mjs`、macOS だけ）

- `process.platform === "darwin"` のときだけ、「現状」1 のプログラムを `project(...)` で作り、`-O0`・`-O3` と `TSUZURI_MAXPROCS=1`・`4` で `run` する。出力が「現状」1 の 3 行と同じで、終了コードが 0。文書の主張（tzroutine の中の `with_locked_thread` はメインスレッドに固定しない、`main` はメインスレッド）を固定する回帰テストである。
- AppKit を読み込む E2E（「現状」4）は足さない（D6）。

### 既存のテストへの影響

- `tests/explain.rs`: `E1013.md` の文字数と `Fixes` の数（6 個）が上限の中に収まる。
- ほかに文を固定するテストはない（「現状」6）。

## ドキュメント

- `_tsuzuri/language-reference/compiler/native-interop.md`: 「スレッドとホストの責任」の 630 行（仕様 2）。
- `_tsuzuri/language-reference/built-in-types-and-modules/routine.md`: 12 行の箇条と「Routine.with_locked_thread」の節（仕様 2）。
- `docs/language.md`: 2720 行の `Routine.with_locked_thread` の説明（仕様 2）。
- `_tsuzuri/language-reference/compiler/diagnostics.md` は `E1013` の kind の表（生成した表）だけで、文を持たないので変えない。`README.md` は GUI の勧めを持たないので変えない。
- PR の説明に、変えたページと、メインスレッドへ戻す API が M09 の計画であることを書く。

## 受け入れ条件

- [ ] 「現状」2 と 3 の `check` が、仕様 1 の新しい文を出す。kind・コード・主の位置・`related`・`help` は今と同じ。
- [ ] `pinned_function_value` と関数値の捕捉の文が変わっていない。
- [ ] `native-interop.md`・`routine.md` に、GUI の UI スレッドに `with_locked_thread` を勧める文が残っていない。プロセスのメインスレッドが要る API の置き場と、メインスレッドへ戻す API がまだないこと（M09 へのリンク）が書いてある。
- [ ] `src/explain/codes/E1013.md` が仕様 3 の箇条を持ち、`tests/explain.rs` が通る。
- [ ] `tests/thread_bound_handles.rs` の新しいテストと、`tests/tzroutine.mjs` の macOS の E2E が通る。
- [ ] 生成 IR とランタイムが変わっていない（`git diff --stat` に `src/runtime/` と `src/llvm*.rs` がない）。
- [ ] 変えたページの `check-docs`、T1、T2 が通る。PR に `ci:full` を付けた。

## 落とし穴

- **target を知らない型検査**: 「macOS では」と条件を付けた診断は出せない。文は target に依らない形にする（仕様 1）。
- **`main` の意味**: 文の `'main'` は Tsuzuri の `main` 関数（とトップレベルのコード）を指す。ライブラリ出力（`--emit object`・`shared`）には C の `@main` がなく、`def main` から作る `tsuzuri_main` も export も、ホストが呼んだスレッドで走る。文書の 2 段落目は実行ファイルの話として書き、ホストの構成ではホストがメインスレッドを決めることを 1 文で添える。
- **`with_locked_thread` を `main` で呼ぶ場合**: 何もせず `main` のスレッドのまま関数を呼ぶ（「現状」1）。「`with_locked_thread` は常にワーカーへ移す」と書かない。
- **W28 との衝突**: W28 は同じ 2 段落を書き換える。先に入ったほうの文を消さずに、段落を分けて合わせる（D5）。
- **explain の文字数**: 上限まで 9 文字しか残らない。英文を長くしない。

## 対象外

- メインスレッドへの投入の API と、メインスレッドに結び付くハンドルの型の検査（[M09](M09-main-thread-dispatch-and-handles.md)）。
- ランタイムの変更（`with_locked_thread` をメインスレッドへ移すこと、待ちと run loop の統合）。
- W28 の Windows の Fiber の検証と、Windows の UI スレッドの勧め。
- 文書で `dlsym` の回避策や ObjC のシムを勧めること。
- AppKit を使う E2E（ObjC の型付きの連携は M01〜M03 の範囲）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | `pinned_code_spawned` と `local` ハンドルの `task_requires_send` の文の末尾に、target に依らない同じ 1 文を足す。採らない案: (a) `with_locked_thread` の勧めを消す（Windows の GDI・OpenGL では正しい）、(b) macOS のときだけ別の文を出す（型検査は target を知らない） | 誤りへ導く部分だけを補い、kind・コード・位置を保つ（D-51）。`task_requires_send` も「`with_locked_thread` の中で作業せよ」と勧め、tzroutine の中でハンドルを作る誤りへ導くので、起票時の範囲（714 行）に加えて直す | 既定案（実装者はこの案に従う） |
| D2 | `pinned_function_value` と関数値の捕捉の文は変えない。採らない案: `with_locked_thread` に触れる文すべてに同じ 1 文を足す | 勧める直し方（直接呼ぶ、`with_locked_thread` にそのまま渡す、`for`・`match`）は、どれも呼んだ側のスレッドで走らせる。呼んだ側が `main` ならメインスレッドのままで、誤りへ導かない。tzroutine の中で関数値にした場合は、同じ誤りに `pinned_code_spawned` も出て（`tzroutine` の位置で、`pinned_function_value` より前に並ぶ。HEAD で確かめた）、新しい文はそちらに付く | 既定案（実装者はこの案に従う） |
| D3 | 文書には今のコンパイラとランタイムがすることだけを書き、メインスレッドへ戻す API は「今はない（計画は M09）」と書く。採らない案: `dlsym` で `_dispatch_main_q` を引く回避策を文書に載せる | GUIDE §8 の 4。回避策は ABI の知識に頼り、型の保護がない。M09 の Phase 1 が出荷されたら、M09 がこの注記を実際の API の説明に置き換える | 既定案（実装者はこの案に従う） |
| D4 | `E1013.md` には、文字数の上限の中に収まる 1 行（79 文字）だけを足す。採らない案: 上限を上げる、ほかの箇条を削って場所を作る | `tests/explain.rs` の上限（1,500 文字、`Fixes` は 6 個まで）。ほかの kind の説明を変えると、この修正の範囲を超える（S3） | 既定案（実装者はこの案に従う） |
| D5 | 2 つのページの該当箇所を「一般の固定の段落」と「プロセスのメインスレッドの段落」に分ける。採らない案: 先に入ったほうの文を置き換える。W28 が先なら、その文（UI は `main` が既定、固定した tzroutine の UI は条件付き）を 1 段落目に残し、2 段落目で macOS の AppKit では固定した tzroutine の UI を使えないと書く。M08 が先なら、W28 が 1 段落目に Windows の条件を足す | 後に入るほうが文を合わせる（依存の欄）。W28 は Windows の Fiber、M08 は macOS のメインスレッドで、段落を分ければ食い違わない | 既定案（実装者はこの案に従う） |
| D6 | 文書の主張は、`pthread_main_np` を出すだけの E2E（macOS、`tests/tzroutine.mjs`）で固定する。採らない案: AppKit を読み込んで `NSWindow` の例外を確かめる E2E | AppKit の E2E は GUI の環境と時間制限が要り、例外の文は OS の版で変わりうる。固定したいのは Tsuzuri の側の事実（固定する先がワーカー M）だけである | 既定案（実装者はこの案に従う） |
