# M04: Block（`^{}`）を作る・受け取る（`ObjC.Block<F>`）

| 項目 | 内容 |
| --- | --- |
| ID | M04 |
| 優先度 | P2 |
| 規模 | L（Phase 1・2。人間が求めない限り Phase 1 だけを実装する） |
| 依存 | [M01](M01-objc-class-types.md), ([M02](M02-objc-typed-message-send.md)), ([M03](M03-objc-classes-defined-in-tsuzuri.md)), ([M09](M09-main-thread-dispatch-and-handles.md))（M02 はメンバーの引数、M03 は型の符号化器を共有） |
| 後続 | [M14](M14-tls-http-packages.md)（弱い後続: [M07](M07-apple-sdk-bindgen-and-package.md), [M13](M13-desktop-os-services-package.md)） |
| 状態 | todo |
| 起票 | 2026-11-26（D-67。利用者の依頼「「どのチケットにもない壁」と「調査中に見つけたコンパイラの不具合（自分で再現済み）」のそれぞれについて @_features/ に開発チケットまたは修正チケットを作成して」から起票。調査で報告した「壁 1（Block は calloc・FnPtr の @c_layout・memcpy・dlsym で手作りし、解放もできない）」に当たる。HEAD `804b7970` で確認） |
| 承認 | 要承認: [GUIDE D-67](GUIDE.md#d-67-tsuzuri-だけで-macos-のデスクトップアプリを書くm01m22) の分岐 1（X′）。D-55（`ObjC.Block`・`ObjC.block`・`ObjC.copy_block`・`ObjC.call_block`）。Phase 2 の `ObjC.call_block` は、AGENTS.md の D-59 の資源のハンドルの一覧に `ObjC.Block` を足す（要承認）。決定事項 D5・D6 が要承認 |
| 改善する劣位 | Objective-C・Swift・C（clang の Blocks）比: Block を取る API（`enumerateObjectsUsingBlock:`、`dispatch_async`、`NSEvent` のローカルモニター、完了ハンドラー、Network.framework の `nw_*` のハンドラー）を、`dlsym` でデータシンボルを引き、`FnPtr` を入れた `@c_layout` の literal を手で組む書き方でしか渡せない。flags・記述子・捕捉の位置は手書きで、型の符号（`BLOCK_HAS_SIGNATURE`）はなく、捕捉を読むにはアドレスの計算が要る。受け取った Block は呼べない |
| 手本にする既存実装 | [W22](_completed/W22-fnptr-values.md)（`FnPtr.of` の実引数の規則と、`ptr @tz.callback.<name>` への下げ方）、[W12](_completed/W12-static-callback-lifetime.md)（コールバックの寿命と `@tz.main.done`）、[W26](W26-com-objects-in-tsuzuri.md) の仕様 4（コールバックの型を広げる表）、`src/llvm_abi.rs` の `wrapper`（`WrapperKind::Callback`）、`src/llvm.rs` の `with_blocking_externs`、[M03](M03-objc-classes-defined-in-tsuzuri.md) の型の符号化器、clang の参照（`_NSConcreteStackBlock` の literal と `__block_descriptor`） |
| 主な影響ファイル | `src/check.rs`・`src/abi.rs`・`src/objc_encoding.rs`（新規。M03 と共有）・`src/llvm.rs`・`src/llvm_abi.rs`・`src/llvm_imports.rs`・`src/effects.rs`・`src/pure_rules.rs`・`src/kg/effects.rs`・`src/capabilities.rs`・`src/pinned_rules.rs`・`src/diagnostic_kinds.rs`、`tests/objc_blocks.rs`（新規）・`tests/objc_blocks.mjs`（新規）・`tests/objc_blocks_host.m`（新規）・`tests/fixtures/objc_blocks/Main.tz`（新規）、`_tsuzuri/language-reference/compiler/objective-c.md`（M01 が作る）・`_tsuzuri/language-reference/built-in-types-and-modules/anchor.md`・`_tsuzuri/language-reference/built-in-types-and-modules/fnptr.md`・`_tsuzuri/language-reference/compiler/native-interop.md`・`docs/language.md`・`docs/architecture.md`、Phase 2 の承認の後に `AGENTS.md`・`_tsuzuri/language-reference/built-in-types-and-modules/argument-order.md` |

## 実装者向け早見

- 範囲: 型 `ObjC.Block<(P1 -> … -> R)>`（ヒープの Block を 1 つ所有する。drop で `_Block_release`）と、値を作る `ObjC.block f token`（`f` は型 `i64 -> P1 -> … -> R` のトップレベルの関数の名前、`token` は `Anchor` の鍵）、複製の `ObjC.copy_block`。`extern def` と M02 のメンバーの引数に `ref ObjC.Block<F>`・`Maybe<ref ObjC.Block<F>>` を書ける。invoke はコンパイラが生成する W12 の族の包み。Phase 2 は ObjC から受け取った Block（M03 の IMP の引数、結果の `ObjC.Block<F>`）と `ObjC.call_block`。
- **人間が求めない限り Phase 1 だけを実装する**（GUIDE §0）。承認が記録されるまで、該当する Phase に着手しない。
- 最初に読む: ① 仕様 1〜6 ② 「現状」の再現（今の手作りの Block）と clang の参照 ③ `llvm_abi::wrapper` と `with_blocking_externs` ④ W22 の仕様 1・2（`FnPtr.of` の実引数の規則）⑤ M03 の型の符号化器。
- 最初に書くテスト: `tests/objc_blocks.rs` の `block_literal_matches_clang`（`--emit llvm --target aarch64-apple-darwin` の literal の 6 つの `store`、flags `-1073741824`、記述子 `{ i64 0, i64 40, ptr, ptr }`、型の符号の文字列）→ `tests/objc_blocks_host.m` の `tzblk_call_now`。
- 触ってはいけない: 今のコールバック・`FnPtr`・`export def` の包みの ABI と IR、`extern def` の `bool`（`int32_t`）と `ref mut bool`（`int32_t *`）。新しい形を使わないプログラムの IR・object・header・診断・既定の `.wasm` は 1 byte も変えない。
- 完了の見分け方（Phase 1）: 次のプログラムを arm64 の macOS で `tsuzuri build <dir> -o blocks` で作ると（libdispatch と libsystem_blocks は libSystem にあり、リンクの指定は要らない）、終了コード 42 を返す。`--target wasm32` の build は `E1008`（`abi_native_only`）、`--target x86_64-unknown-linux-gnu` の build は `E1008`（`abi_apple_only`）。

```tsuzuri
extern type copy Queue

@trusted
extern "dispatch_get_global_queue" def global_queue :: i64 -> i64u -> Queue

@trusted
extern "dispatch_sync" def dispatch_sync :: Queue -> ref ObjC.Block<(unit -> unit)> -> unit

def on_tick :: i64 -> unit = \token ->
    let _ = token |> Anchor.update (\(n: i64) -> n + 1)
    ()

def main :: unit -> i32 = \() ->
    let zero: i64 = 0
    let calls = Anchor.create zero
    let block = ObjC.block on_tick (Anchor.token (ref calls))
    let queue = global_queue 0 0i64u
    dispatch_sync queue (ref block)
    dispatch_sync queue (ref block)
    match Anchor.token (ref calls) |> Anchor.read (\(n: ref i64) -> deref n) with
    | Result.Ok n -> (n * 21) as i32
    | Result.Error _ -> 1
```

## 目的

Apple の API の多くは Block を取る。`dispatch_async` と `dispatch_after`、`-[NSArray enumerateObjectsUsingBlock:]`、`+[NSEvent addLocalMonitorForEventsMatchingMask:handler:]`、`NSOpenPanel` の `beginWithCompletionHandler:`、`NSAnimationContext` の完了、Network.framework の `nw_connection_set_state_changed_handler` は、Block のほかに書き方がない（`dispatch_async_f` のような関数ポインターの版があるのは libdispatch だけ）。

今の Tsuzuri でも Block は手で作れる（「現状」）。しかし `dlsym` でデータシンボル（`_NSConcreteStackBlock`）を引き、`FnPtr` を入れた `@c_layout` の literal と記述子を手書きし、捕捉の鍵をアドレスの計算（`block + 32`）で読む。flags や大きさを誤れば、ランタイムが黙ってメモリーを壊す。型の符号もない。

このチケットは、Block の作成をコンパイラが clang と同じ形で生成するようにし、Block を取る API を型付きで呼べるようにする。捕捉は鍵 1 つ（`Anchor` の鍵）に限り、Tsuzuri の任意の値は捕捉しない。D-67 の X′ の範囲（`extern` の宣言と組み込みのモジュール `ObjC` の型。生ポインター型と `unsafe` は入れない）を出ない。M14（Network.framework）の前提になる。

## 着手条件と停止条件

### 着手条件

- [M01](M01-objc-class-types.md) が done（組み込みのモジュール `ObjC`、`abi_apple_only`、ObjC の型の位置の規則）。確認: `grep -nE "^\| M01 " _features/README.md`。M02 があればメンバーの引数の段、M03 があれば型の符号化器を共有する。
- D-67 の分岐 1 で X′ が承認され、決定事項の D6（D-55 の登録）が承認済み。**承認が記録されるまで、該当する Phase に着手しない。**
- **人間が求めない限り Phase 1 だけを実装する。** Phase 2 は人間が求めたときだけで、M03 の Phase 1 が done（IMP の引数）。`ObjC.call_block` の段は決定事項 D5 の承認の後（承認がなければ、Phase 2 は受け取った Block を保持して渡すだけにする）。
- ベースライン: `tests/fixtures/*`・`examples/*` の `--emit llvm`（native・`--target aarch64-apple-darwin`・`--target wasm32`）と `check --json`、コールバックを使う fixture（`tests/fixtures/ffi_extensions`）と M01〜M03 の fixture の IR を `target/scratch/m04/before/` に保存する。
- macOS（arm64）の機械と Xcode の Command Line Tools（clang・SDK）。node のテストの段は macOS だけで走る。

### 停止条件

- **S1** invoke のために、コールバックの包み（`llvm_abi::wrapper`）を別の関数に複製したくなった（W12 の「包みは 1 種類」に反する）。
- **S2** Block を使わないプログラムの IR・object・header・診断・既定の `.wasm` が 1 byte でも変わる。M01〜M03 の fixture の IR が変わる。
- **S3** 鍵 1 つより多い捕捉、`BLOCK_HAS_COPY_DISPOSE`、`__block` の変数が要ると分かった（対象外の範囲に踏み込む）。
- **S4** Block の invoke からアプリの状態へ、`Anchor` の鍵の表を通らない経路ができる（W12 の「判定の義務」）。
- **S5** `ObjC.Block<F>` の `F` を運ぶために、`Type` に variant を足したくなった（M01 の S1 と同じ。決定事項 D10）。
- **S6** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `804b7970` で確認）

### 再現: 今の手作りの Block（動く）

literal の `isa` を `_NSConcreteStackBlock` にし、`ref` の一時領域として `_Block_copy` に渡すと、ヒープの Block が返り、`_Block_release` で解放できる。記述子は `calloc` の領域に `extern view` で書く（解放できないので、種類ごとに 1 つを使い回す）。`free` は予約されていて宣言できない（下）。

```tsuzuri
// A Block built by hand today, without a C file: a literal whose isa is _NSConcreteStackBlock is
// passed as a `ref` temporary to _Block_copy, which returns a heap Block that _Block_release frees.
@c_layout
record BlockDescriptor { reserved: i64, size: i64 }

@c_layout
record BlockLiteral { isa: i64, flags: i32, reserved: i32, invoke: FnPtr<(i64 -> unit)>, descriptor: i64, token: i64, sema: i64 }

@c_layout
record Word { value: i64 }

@trusted
@count(2, none)
extern "dlsym" def dlsym :: i64 -> ref utf8string -> i64

@trusted
extern "calloc" def calloc :: i64 -> i64 -> i64

@trusted
extern view def write_descriptor :: i64 -> ref BlockDescriptor -> unit

@trusted
extern view def read_word :: i64 -> Word

@trusted
extern "_Block_copy" def block_copy :: ref BlockLiteral -> i64

@trusted
extern "_Block_release" def block_release :: i64 -> unit

@trusted
extern "dispatch_get_global_queue" def global_queue :: i64 -> i64 -> i64

@trusted
extern "dispatch_sync" def dispatch_sync :: i64 -> i64 -> unit

@trusted
extern "dispatch_async" def dispatch_async :: i64 -> i64 -> unit

@trusted
extern "dispatch_semaphore_create" def semaphore_create :: i64 -> i64

@trusted
extern "dispatch_semaphore_signal" def semaphore_signal :: i64 -> i64

@trusted
extern "dispatch_semaphore_wait" def semaphore_wait :: i64 -> i64u -> i64

@trusted
extern "pthread_main_np" def pthread_main_np :: unit -> i32

// invoke(block): the captures follow the 32-byte header (token at +32, semaphore at +40).
def on_block :: i64 -> unit = \block ->
    let token = (read_word (block + 32)).value
    let sema = (read_word (block + 40)).value
    match token |> Anchor.update (\(n: i64) -> n + 1) with
    | Result.Ok _ -> do! IO.writeln $"invoke: main={pthread_main_np ()} Ok"
    | Result.Error e -> do! IO.writeln $"invoke: main={pthread_main_np ()} Error {e}"
    let _ = semaphore_signal sema
    ()

def main :: unit -> i32 = \() ->
    let zero: i64 = 0
    let calls = Anchor.create zero
    let sema = semaphore_create 0
    let descriptor = calloc 1 16
    let header = BlockDescriptor { reserved: 0, size: 48 }
    write_descriptor descriptor (ref header)
    let isa = dlsym (0 - 2) (ref u8"_NSConcreteStackBlock\0")
    let literal = BlockLiteral { isa: isa, flags: 0i32, reserved: 0i32, invoke: FnPtr.of on_block, descriptor: descriptor, token: Anchor.token (ref calls), sema: sema }
    let block = block_copy (ref literal)
    let queue = global_queue 0 0
    dispatch_sync queue block
    let _ = semaphore_wait sema 18446744073709551615i64u
    dispatch_async queue block
    let _ = semaphore_wait sema 18446744073709551615i64u
    block_release block
    let n = match Anchor.token (ref calls) |> Anchor.read (\(c: ref i64) -> deref c) with
        | Result.Ok c -> c
        | Result.Error _ -> -1
    do! IO.writeln $"main: calls={n}"
    0
```

```text
$ tsuzuri build heap/Main.tz -o heap/app && ./heap/app
invoke: main=1 Ok
invoke: main=0 Error anchor token belongs to another thread
main: calls=1
```

`-O3` でも同じ出力で、終了コード 0。`dispatch_sync` はグローバルキューの Block を呼んだスレッド（main）で呼び、`dispatch_async` は GCD のワーカーで呼ぶ。ワーカーの invoke から `Anchor` を引くと `WrongThread` になる（`_tsuzuri/language-reference/built-in-types-and-modules/anchor.md:84`。別のスレッドから引ける預け先は W23 の対象外。W23:240）。この再現は tzroutine を起動しないので、包みに入口と出口は付かない（`with_blocking_externs` は spawn するプログラムにだけ働く。`src/llvm.rs:1660-1664`。この再現の `--emit llvm` に `tsuzuri_tz_callback_scope_enter` は 0 件）。spawn するプログラムでは、ランタイムに参加していないスレッド（GCD のワーカー）の呼び出しを、`tsuzuri_tz_callback_scope_enter`（`src/runtime/tzroutine.c:2178-2185`）がその間だけの境界にする（W12）。別の試し（捕捉が鍵だけの 40 バイトの literal、flags 0）で、`_Block_copy` の結果の `isa` は `_NSConcreteMallocBlock`、flags は `16777218`（`BLOCK_NEEDS_FREE` と参照数の欄）だった。

`--emit llvm` の invoke の包みは `define internal void @tz.callback.Main.on_block(i64 %arg0) nounwind`、利用者の宣言は `declare i64 @_Block_copy(ptr)` と `declare void @_Block_release(i64)`（M04 がコンパイラの呼び出しを足すときは、この行を使い回して自分の型で `call` する。共通の規則）。

調査の probe E8 は、`isa` を `_NSConcreteGlobalBlock`、flags を `268435456`（`BLOCK_IS_GLOBAL`）にした literal を `calloc` の領域へ `memcpy` で写し、`enumerateObjectsUsingBlock:` から invoke を 3 回呼ばせた（`block invoked: idx=0 obj=alpha` から `idx=2 obj=gamma`）。調査 callbacks の検証では、同じ形で `dispatch_sync` を 2 回と `dispatch_sync_f` を 1 回呼び `calls=102` だった。グローバルの Block は `_Block_release` で解放されず、`calloc` の領域も `free` できないので、作るたびに漏れる。

### 手作りを妨げる検査

FnPtr を持つ record は `extern view` で書けない（`src/abi.rs:433-438`。`c_layout_input_only`）。

```tsuzuri
@c_layout
record BlockLiteral { isa: i64, flags: i32, reserved: i32, invoke: FnPtr<(i64 -> unit)>, descriptor: i64 }

@trusted
extern view def write_block :: i64 -> ref BlockLiteral -> unit

def on_block :: i64 -> unit = \_block -> ()

def main :: unit -> i32 = \() ->
    write_block 0 (ref BlockLiteral { isa: 0, flags: 0i32, reserved: 0i32, invoke: FnPtr.of on_block, descriptor: 0 })
    0
```

```text
viewfn/Main.tz:5:17: error[E1008]: extern view 'write_block' copies '@c_layout' record 'BlockLiteral', which holds a FnPtr or borrowed ZString field; extern views only copy data and handle fields
```

`free` は予約されている（`src/abi.rs:13-51` の `RESERVED_HOST_SYMBOLS` の 31 行、文は同 84-92 行）。`extern "free" def c_free :: i64 -> unit` は `error[E1008]: extern symbol 'free' is reserved by Tsuzuri because the generated code declares it; wrap the host function under another symbol`。`_Block_copy` などは `__` で始まらない（Mach-O の名前の先頭の `_` は C の名前に含まれない）ので、`RESERVED_SYMBOL_PREFIXES`（同 55 行）に当たらない。

コールバックの引数はスカラー・ハンドル・`Maybe<H>`、結果はスカラーか unit だけ（`src/abi.rs:661-672` の `callback`）。`ref mut` の引数はコールバックに書けない（`extern_scalar`、`src/abi.rs:558-564`）。`extern def` の `bool` は `int32_t`、`ref mut bool` は `alloca i32` の `int32_t *` に下がる（`src/abi.rs:794-797` の `c_scalar`。`extern "takes_flag" def takes_flag :: ref mut bool -> bool -> unit` の IR は `alloca i32, align 4` と `call void @takes_flag(ptr %v5, i32 %v7)`）。この下げ方を Block の 1 バイトの `BOOL *stop` に使うと、4 バイトの書き込みが隣の 3 バイトを壊す。

### clang の参照（clang 21、`-target arm64-apple-macosx12.0 -fobjc-arc -fno-objc-msgsend-selector-stubs -O0 -S -emit-llvm`）

```objc
extern void use(long);
void go(NSArray *a, long k) {
  [a enumerateObjectsUsingBlock:^(id o, NSUInteger i, BOOL *stop) { use(k); *stop = NO; }];
}
```

```llvm
@_NSConcreteStackBlock = external global ptr
@.str = private unnamed_addr constant [16 x i8] c"v32@?0@8Q16^B24\00", align 1
@OBJC_LAYOUT_BITMAP_ = private unnamed_addr constant [1 x i8] zeroinitializer, section "__TEXT,__cstring,cstring_literals", align 1
@"__block_descriptor_40_e15_v32\01?0\018Q16^B24l" = linkonce_odr hidden unnamed_addr constant { i64, i64, ptr, ptr } { i64 0, i64 40, ptr @.str, ptr @OBJC_LAYOUT_BITMAP_ }, align 8
  %5 = alloca <{ ptr, i32, i32, ptr, ptr, i64 }>, align 8
  store ptr @_NSConcreteStackBlock, ptr %7, align 8
  store i32 -1073741824, ptr %8, align 8
  store i32 0, ptr %9, align 4
  store ptr @__go_block_invoke, ptr %10, align 8
  store ptr @"__block_descriptor_40_e15_v32\01?0\018Q16^B24l", ptr %11, align 8
  store i64 %13, ptr %12, align 8
  call void @objc_msgSend(ptr noundef %6, ptr noundef %14, ptr noundef captures(none) %5)
define internal void @__go_block_invoke(ptr noundef %0, ptr noundef %1, i64 noundef %2, ptr noundef %3)
```

- flags `-1073741824` は `0xC0000000`（`BLOCK_HAS_SIGNATURE` と `BLOCK_HAS_EXTENDED_LAYOUT`）。`-fno-objc-arc` でも同じ。物体の捕捉がないので layout は空の文字列。
- 同じ target の C（`-fblocks`）で `long` を捕捉する Block（`take(^(long x) { use(k + x); })`）は、flags `1073741824`（`BLOCK_HAS_SIGNATURE` だけ）、記述子 `{ i64 0, i64 40, ptr @.str, ptr null }` だった。`BLOCK_HAS_EXTENDED_LAYOUT` と空の layout は Objective-C のときだけ付く（決定事項 D3）。
- 逃げる Block（関数から返す `double (^)(double)`）は `call ptr @llvm.objc.retainBlock(ptr %3)` でヒープに移す。記述子は `{ i64 0, i64 40, ptr @.str.1, ptr … }` で、型の符号は `"d16@?0d8"`。
- グローバルの Block（捕捉なし）は `@__block_literal_global = internal constant { ptr, i32, i32, ptr, ptr } { ptr @_NSConcreteGlobalBlock, i32 1342177280, i32 0, ptr …, ptr … }`（`BLOCK_IS_GLOBAL | BLOCK_HAS_SIGNATURE`）、記述子は `{ i64 0, i64 32, ptr @.str, ptr null }`。
- ほかの Block の型の符号（同じ clang）: `void (^)(float, BOOL, short, unsigned char)` は `"v24@?0f8B12s16C20"`、`BOOL (^)(SEL, Class, long long, unsigned int)` は `"B36@?0:8#16q24I32"`、`NSInteger (^)(id, id)` は `"q24@?0@8@16"`、`void (^)(NSRect, NSRange)` は `"v56@?0{CGRect={CGPoint=dd}{CGSize=dd}}8{_NSRange=QQ}40"`。

### ランタイムの事実

- `_Block_copy`・`_Block_release`・`_Block_signature`・`_NSConcreteStackBlock`・`_NSConcreteMallocBlock`・`_NSConcreteGlobalBlock` は `/usr/lib/system/libsystem_blocks.dylib` が export する（`dyld_info -exports`）。libSystem の一部なので、リンクの指定は要らない（再現も `-l` なしで動いた）。
- `objc_retainBlock` は `/usr/lib/libobjc.A.dylib` が export する。SDK の `usr/include/objc` に宣言はない（ARC と clang が使う ABI）。
- `tsuzuri_tz_callback_enter`（`src/runtime/tzroutine.c:2157-2165`）は main の擬似 G とランタイムに参加していないスレッドでは何もしない。spawn するプログラムでは `with_blocking_externs`（`src/llvm.rs:1840` から）が ` @tz.callback.` の `define` を包む。実行ファイルの包みは `@tz.main.done` を検査する（`src/llvm_abi.rs:813-823`）。
- IR に `@tz.callback.` があると、`--trap-mode return` と `Domain.run` は `E2000`（`src/driver.rs:4100-4105`、`src/llvm.rs:1545-1557`）。

## 変えてはいけない意味

- W12 の契約: コールバック（と Block の invoke）は `main` の境界の終わりまでどのスレッドからでも呼べ、その後の呼び出しはトラップする。包みは 1 種類。
- 今のコールバック・`FnPtr`・`export def` の型・ABI・IR。`extern def` の `bool` と `ref mut bool` の下げ方。
- `Anchor` の意味（W23）。Block は状態を所有しない。
- コールバックの中のトラップがプロセスを終わらせること。

## 仕様

### Phase の範囲

| Phase | 内容 | 規模 | 前提 |
| --- | --- | --- | --- |
| 1 | `ObjC.Block<F>`・`ObjC.block`・`ObjC.copy_block`、drop の `_Block_release`、`extern def` と M02 のメンバーの引数 `ref ObjC.Block<F>`・`Maybe<ref ObjC.Block<F>>`、invoke の包み、`F` の型の集合、型の符号。ホストの `.m` と libdispatch で検査する | M | M01（M02・M03 があれば使う） |
| 2 | ObjC から受け取った Block: M03 の IMP の引数（+0 の `ref ObjC.Block<F>`）、`extern def` と M02 のメンバーの結果 `ObjC.Block<F>`、`ObjC.call_block`（D5 の承認の後） | M | M03 の Phase 1 |

### 1. 型 `ObjC.Block<F>`（Phase 1）

- `F` は関数型 `(P1 -> … -> Pn -> R)` か `(unit -> R)`（引数のない Block）。`F` が関数型でなければ `E1005`（kind 案 `objc_block_not_function`）。
- 値はヒープの Block への参照を 1 つ所有する（C の `R (^)(P1, …, Pn)`）。drop で `_Block_release` を呼ぶ。値の移動で所有も移る。
- Copy でも Clone でもない（複製は `ObjC.copy_block`）。Send・Sync（決定事項 D7）。drop（`_Block_release`）を持つので、M01 の `sync` のクラスと同じく再利用できる関数値には捕捉できない（`E1005`。関数値の複製が捕捉を複製し、二重の release になる。`src/check.rs:1081-1086`）。record・union・`Maybe`・`Vec` に入れてよく、それらは drop glue を持つ。`Maybe<ObjC.Block<F>>` は 1 つのポインター（`None` は NULL）。
- `Eq`・`Display` のインスタンスはない。
- `export def` の引数・結果、`@c_layout` の field、`FnPtr` とコールバックの型に置くと `E1008`（M01 の `objc_object_position`）。

### 2. `ObjC.block` と `ObjC.copy_block`（Phase 1）

| 関数 | 型 | 意味 |
| --- | --- | --- |
| `ObjC.block` | `(i64 -> P1 -> … -> Pn -> R) -> i64 -> ObjC.Block<(P1 -> … -> Pn -> R)>` | `f` を invoke にし、鍵を捕捉した Block を作る |
| `ObjC.copy_block` | `ref ObjC.Block<F> -> ObjC.Block<F>` | `_Block_copy` を呼び、同じ Block の所有値をもう 1 つ返す（ヒープの Block では参照数が 1 増える） |

- `ObjC.block` の最初の引数は、型引数のないトップレベルの利用者の関数の名前だけ（静的コールバックと W22 の `FnPtr.of` と同じ規則。ラムダ・部分適用・局所・std の関数・`extern` は `E1008`、kind は `FnPtr.of` のものを共有する）。`f` の最初の引数の型は `i64` でなければならない（違えば `E1003`、`objc_block_function_type`）。`f :: i64 -> R` は `ObjC.Block<(unit -> R)>` を作る。
- 2 つ目の引数は鍵（`Anchor.token` の値。`Anchor.read f token` と同じ並び）。鍵の正しさは invoke の中の `Anchor` が検査する。
- 期待型があれば、`F` はそれと一致しなければならない（`E1003`）。
- 引数の順序（D-59）: `ObjC.block` は対象のない構築、`ObjC.copy_block` は唯一の引数なので例外は要らない。
- 下げ方（決定事項 D1・D3）: clang と同じスタックの literal を作り、`_Block_copy` でヒープに移す。結果が NULL（確保の失敗）ならトラップする（確保の失敗の既存の扱い）。

```llvm
%lit = alloca <{ ptr, i32, i32, ptr, ptr, i64 }>, align 8
store ptr @_NSConcreteStackBlock, ptr %lit                 ; isa
store i32 -1073741824, ptr %flags                          ; BLOCK_HAS_SIGNATURE | BLOCK_HAS_EXTENDED_LAYOUT
store i32 0, ptr %reserved
store ptr @tz.callback.objc-block.Main.on_tick, ptr %invoke
store ptr @tz.objc.block.descriptor.0, ptr %descriptor
store i64 %token, ptr %capture
%block = call ptr @_Block_copy(ptr %lit)
```

### 3. Block を渡す位置（Phase 1）

- `extern def` と M02 のメンバーの引数に `ref ObjC.Block<F>` と `Maybe<ref ObjC.Block<F>>`（`None` は NULL）を書ける。C には Block のポインターを +0 で渡す。保持する呼び出し先は Block の ABI のとおり自分で `_Block_copy` する（ヒープの Block なので参照数が増えるだけ）。
- 所有する `ObjC.Block<F>` の引数は `E1008`（M01 の `objc_owned_object_argument`。C の API は Block を消費しない）。
- `extern def` の結果の `ObjC.Block<F>` は Phase 2。M02 のメンバーの引数の段は M02 が done のときだけ。
- ブロッキングの包み・効果・権限は、その `extern def`・メンバーの今の規則のまま。

### 4. invoke の包み（Phase 1）

C が Block を呼ぶと（`block->invoke(block, a1, …, an)`）、コンパイラが生成した包みが次をする。

1. 実行ファイルでは W12 の検査（`@tz.main.done` が立っていればトラップ）。
2. literal の 32 バイト目から鍵を読む。
3. 引数を仕様 5 の表のとおりに Tsuzuri の値に直し、`f(鍵, a1, …, an)` を呼ぶ。
4. 結果を C の値に直し、`ref mut` の引数を書き戻す。

- 包みの名前は `@tz.callback.objc-block.<f の修飾名>`。` @tz.callback.` で始まるので、spawn するプログラムでは `with_blocking_externs` が W12 のコールバックと同じ入口と出口で包む。ランタイムに参加していないスレッド（GCD のワーカー）の呼び出しは、その間だけの境界になる（W12）。
- `FnPtr` の包みは `@tz.callback.<修飾名>`（`src/llvm_abi.rs:953`）で、修飾名は ASCII の識別子を `.` で結ぶ。名前空間の `::` も `.` になり、成分の数は決まらない（小文字のディレクトリ `block/` の `Main.tz` の `on_tick` を `FnPtr.of` に渡すと `@tz.callback.objc-block.Main.on_tick` になる）。`-` は識別子に現れないので、`objc-block` の成分は利用者の関数の包みと衝突しない（LLVM は `-` を引用符なしの名前に許し、PB07 の分割も名前の文字に数える。`src/llvm_split.rs:169-171`）。M03 の IMP の包みは `@tz.callback.objc-imp.…` で、これとも衝突しない。
- 同じ `f` を `FnPtr.of` と `ObjC.block` の両方に使ってよい（C の型が違うので、包みは別の関数になる）。
- 包みは `llvm_abi::wrapper` を共有する（`WrapperKind` に Block の種類を足す。S1）。
- Block を作るプログラムでは、IR に `@tz.callback.` があるので `--trap-mode return` と `Domain.run` は今のコールバックと同じく `E2000`。

### 5. `F` に書ける型（Phase 1）

| Tsuzuri の型（引数） | C の型 | 包みのすること |
| --- | --- | --- |
| 整数・`f32`・`f64` | 同じ幅の整数・`float`・`double` | そのまま |
| `bool` | `BOOL`・`bool`（arm64 は `i1 zeroext`） | そのまま。`extern def` の `bool`（`int32_t`）と違う |
| `ObjC.Sel`・`ObjC.Class` | `SEL`・`Class` | そのまま |
| ハンドル `H`・`ref H`・`Maybe<H>`・`Maybe<ref H>` | ポインター | 今のコールバックと同じ（非 Maybe の NULL は `null extern handle` でトラップ） |
| `ref C`・`Maybe<ref C>`（M01 のクラス。M03 があればプロトコル） | `id`（+0） | 呼び出しの間だけの借用。`ref C` の NULL はトラップ |
| `ref mut bool` | 1 バイトの `BOOL *` | 1 バイトを局所に読み、`ref mut` で渡し、戻った後に書き戻す（非ゼロは true に正規化）。NULL ならトラップ |
| `ref mut T`（`T` は整数・`f32`・`f64`） | `T *` | 同じ。W26 の仕様 4 か M03 の変換が先に入っていればそれを使う |

- 結果: `unit`（`void`）、整数・`f32`・`f64`・`bool`（`i1 zeroext`）。物体・ハンドル・record の結果は `E1008`（`objc_block_type`）。
- 引数の `@c_layout` record（`void (^)(NSRect)`）と buffer は `E1008`（`objc_block_type`）。
- 型の符号は M03 と共有する `src/objc_encoding.rs` の `signature(Receiver::Block, …)` で作る（`(Maybe<ref ObjC.Object> -> i64u -> ref mut bool -> unit)` は `"v32@?0@8Q16^B24"`、`(unit -> unit)` は `"v8@?0"`）。

### 6. スレッド・target・効果（Phase 1）

- Block は ObjC と GCD のどのスレッドからも呼ばれうる。invoke の中で `Anchor` を引くと、作ったスレッド以外では `WrongThread` になる。状態を main で更新するには、[M09](M09-main-thread-dispatch-and-handles.md) の Phase 1 の `MainThread.post` で main に移る（鍵と引数は `i64` なので捕捉できる）。

```tsuzuri
record AppState { changes: i64, last: i32 }

def on_state :: i64 -> i32 -> unit = \token state ->
    MainThread.post (Owned.function (\() ->
        let _ = token |> Anchor.update (\(s: AppState) -> AppState { changes: s.changes + 1, last: state })
        ()))
```

- M09 の Phase 2 があれば、本体が効果 `main_thread` を持つ invoke の包みは、入口で `pthread_main_np()` を検査してトラップする（M09 の規則。Block は GCD のスレッドから呼ばれうるため）。
- target: 型検査は target を知らない。`llvm::emit_target` で、到達した `ObjC.block`・`ObjC.copy_block`・Block の drop・Block を ABI に持つ呼び出しを、wasm は `E1008`（`abi_native_only`）、Apple でない native は `E1008`（`abi_apple_only`）にする。どちらも `build` のときに出し、`check` は受理する。
- 効果と権限: `ObjC.block`・`ObjC.copy_block` は効果 `extern` と `alloc`、drop は `extern`（W25 の interface の drop と同じく `user_drop` ではない）。権限は `ffi`（D-57）。組み込みの関数は宣言を持たないので `W1010` の対象は、Block を渡す `extern def`・メンバーの宣言（今のまま）。`f` の効果はその本体から推論する（コールバックと同じ）。`f` の本体は固定された文脈（W19 の D5）。

### Phase 2: 受け取った Block と `ObjC.call_block`

- **IMP の引数**: M03 の export のクラスのメンバーの引数に `ref ObjC.Block<F>`・`Maybe<ref ObjC.Block<F>>` を書ける（完了ハンドラーを取るメソッド）。+0 の借用で、呼び出しの間だけ有効（呼ぶ側がスタックの Block を渡すことがある）。保持するには `ObjC.copy_block` で所有値にする（`_Block_copy` がスタックの Block をヒープに移す）。型の符号は `@?`。
- **結果**: `extern def` と M02 のメンバーの結果に `ObjC.Block<F>`・`Maybe<ObjC.Block<F>>` を書ける。+0 の結果は受け取った直後に `objc_retainBlock` を呼ぶ。`@returns_retained` が付いていれば呼ばない（M01 の規則）。
- **`ObjC.call_block (ref b) a1 … an`**（決定事項 D5 の承認の後）: Block の 16 バイト目の invoke を読み、`call <R> %invoke(ptr %b, <a1…an>)` を宣言の型で呼ぶ。すべての引数を与えた直接の呼び出しだけ（関数値と部分適用は `E1008`、`objc_block_not_called`）。引数の下げ方は仕様 5 の逆向き（`ref mut` は一時領域と書き戻し）。効果は `extern`、権限は `ffi`。spawn するプログラムでは `extern def` と同じブロッキングの包み。資源のハンドルを先頭に置くので、D-59 の例外の一覧（AGENTS.md の resource handles）に `ObjC.Block` を足す。
- 受け取った Block の本当の型と `F` が一致することは、宣言（`@trusted`）の責任。テストでは `_Block_signature` と期待の型の符号を比べる。
- D5 が承認されなければ、Phase 2 は IMP の引数と結果の受け取り、保持（`ObjC.copy_block`）、別の API への受け渡しだけにする。

## 設計

### 型検査（`src/check.rs`・`src/abi.rs`）

- 組み込みの型 `ObjC.Block<F>` の名前は、W22 の `FnPtr<F>` と同じ所（`resolve_type_with_kinds` の組み込みの型の腕、`builtin_type_head`、`reserved_type_name`）で解決する。
- 表現（決定事項 D10）: 今の `Type::Handle(Box<str>, HandleKind)`（`src/check.rs:178`）は型引数を持てないので、M01 の `HandleKind` の種類だけでは `F` を運べない。型引数を持つ組み込みの値は、`Anchor<'a>`（`std/Anchor.tz:3` の `record Anchor<'a> { token: i64 }`。`'a` は値に現れない型引数で、drop は `instance Drop`）と `Owned.Function<'a, 'b>`（`std/Owned.tz`）のように、std の record に名前で特別な扱い（Copy でない・drop・ABI）を付ける形が先例である。`ObjC.Block<F>` もこの形にし、性質は D7、drop は `objc_release` ではなく `_Block_release`（libobjc をリンクしないプログラムでも正しい）。`FnPtr(Box<Type>)`（`src/check.rs:191`）と同じ `Type` の variant を足したくなったら、停止条件 S5 で止める（M01 の S1 と同じ）。
- Block だけを使うプログラム（`_Block_*` と `_NSConcreteStackBlock` は libSystem）は libobjc を要らない。M01 は Objective-C のモジュールのフラグ（`!"Objective-C Version"`）を持つ IR に `-lobjc` を足す（M01 の `llvm::uses_objc`）ので、Block の生成はこのフラグを足さない（clang も C の Block には付けない）。Phase 2 の `objc_retainBlock` は libobjc にあるので、それを呼ぶ IR では M01 の `objc_used` を立てる（モジュールのフラグが付き、M01 の `uses_objc` の判定のまま `-lobjc` が足される。M05・M06・M10 と同じ形で、M01 の判定そのものは変えない）。
- `Builtin::ObjcBlock`・`ObjcCopyBlock`（Phase 2 で `ObjcCallBlock`）を足し、`Builtin::ALL`・`name`・`signature`・`emit_builtin`・効果と権限の表・`pinned_rules::builtin_position` を更新する。`ObjC.block` の実引数の検査は `validate_callbacks`（`plain_function`・`direct_only`）を共有する。
- `src/abi.rs`: `block_parameter`・`block_result`（仕様 5）を足す。`callback` は変えない。`extern def` の引数の分類に `ref ObjC.Block<F>`・`Maybe<ref ObjC.Block<F>>`（`Ptr`）を足す。

### literal と記述子の生成

- 記述子と型の符号は、型の符号の文字列ごとに 1 つ（literal の大きさはどれも 40 バイトなので、記述子は文字列だけで決まる）。番号は最初に現れた順（決定的）。

```llvm
@_NSConcreteStackBlock = external global ptr
@tz.objc.block.layout = private unnamed_addr constant [1 x i8] zeroinitializer
@tz.objc.block.signature.0 = private unnamed_addr constant [6 x i8] c"v8@?0\00"
@tz.objc.block.descriptor.0 = private unnamed_addr constant { i64, i64, ptr, ptr } { i64 0, i64 40, ptr @tz.objc.block.signature.0, ptr @tz.objc.block.layout }
```

- `_NSConcreteStackBlock` の `external global`、`_Block_copy`・`_Block_release`・`objc_retainBlock`（Phase 2）の `declare` は symbol ごとに 1 行（共通の規則。利用者の `extern` が同じ symbol を宣言していれば、その行を使い、呼び出しは自分の関数型で `call` する。「現状」の `declare i64 @_Block_copy(ptr)` と共存する）。
- drop は `call void @_Block_release(ptr %b)`。`Maybe` の `None` では呼ばない。

### invoke の包み（`src/llvm_abi.rs`）

```llvm
define internal void @tz.callback.objc-block.Main.on_tick(ptr %block) nounwind {
entry:
  ; W12: load atomic i32, ptr @tz.main.done acquire（実行ファイルだけ）
  %slot = getelementptr inbounds i8, ptr %block, i64 32
  %token = load i64, ptr %slot, align 8
  call void @tz.fn.Main.on_tick(i64 %token)
  ret void
}
```

- `ref mut bool` の引数は `%b = load i8, ptr %p` → `icmp ne i8 %b, 0` を `alloca i1` に置いて渡し、戻った後に `zext` して `store i8` で書き戻す。`ref mut T` は `alloca T` の読み書き。NULL の検査は既存の `null extern handle` のトラップを使う。
- `bool` の引数と結果は `i1 zeroext`（M02 の ObjC の位置と同じ）。

### 採らなかった案

| 案 | 採らなかった理由 |
| --- | --- |
| スタックの Block をそのまま渡し、逃げるときだけヒープに移す（clang と同じ） | 寿命の解析（引数が呼び出しの後に保持されるか）が要り、`ObjC.Block<F>` の値の意味が 2 つになる。作るたびの確保 1 回で意味を 1 つにする（D1） |
| グローバルの Block（`_NSConcreteGlobalBlock`）の定数にする | 鍵を捕捉できない（定数は実行時の値を持たない）。鍵を大域変数に置くことは「モジュール共有の可変状態の排除」に反する |
| 任意の値の捕捉（Tsuzuri のクロージャーを Block にする） | `BLOCK_HAS_COPY_DISPOSE` の helper、捕捉した値の drop と別のスレッドでの解放、Send の検査が要る（W26 の採らなかった案の「捕捉のある関数値」と同じ理由）。状態は `Anchor` に置く |
| 型の符号を付けない（`BLOCK_HAS_SIGNATURE` なし） | `NSInvocation`・XPC・Swift の橋渡しが符号を読む。clang はいつも付ける |
| Block を `extern type` と `FnPtr` の利用者の宣言で作らせる（今の書き方を正式にする） | アドレスの計算と `dlsym` が要り、D-67 の範囲（ポインターを触るコードはコンパイラが宣言から生成する）を出る |

## 前提とする他チケットのインターフェース

- [M01](M01-objc-class-types.md): 組み込みのモジュール `ObjC`（std のファイルを持つか。決定事項 D10 の record の置き場）、ObjC の所有の規則と drop、`ref C`・`Maybe<ref C>` の ABI、kind `abi_apple_only`・`objc_object_position`・`objc_owned_object_argument`、`@returns_retained`（Phase 2）。
- [M02](M02-objc-typed-message-send.md)（あれば）: メンバーの引数の分類（`abi::objc_parameter` に `ref ObjC.Block<F>`・`Maybe<ref ObjC.Block<F>>` を足す口）、ObjC の位置の `bool`（`i1 zeroext`）と `ref mut bool`（1 バイト）の規則、結果の受け取り（Phase 2）。
- [M03](M03-objc-classes-defined-in-tsuzuri.md): `src/objc_encoding.rs`（M04 が先なら M04 が作る）。Phase 2 の IMP の引数の型の表と包み。
- [W12](_completed/W12-static-callback-lifetime.md)・[W22](_completed/W22-fnptr-values.md): `llvm_abi::wrapper`、`WrapperKind::Callback`、`@tz.main.done`、`with_blocking_externs`、`validate_callbacks`。
- [W23](_completed/W23-anchor-state.md): `Anchor.token`・`read`・`update`、`WrongThread`。
- [M09](M09-main-thread-dispatch-and-handles.md)（あれば）: `MainThread.post`（文書の例）。Phase 2 の `main` の型は Block の本体を使える文脈に数える。
- [W26](W26-com-objects-in-tsuzuri.md)（あれば）: コールバックの `ref mut T` の変換。

## 他チケットへ提供するインターフェース

- [M14](M14-tls-http-packages.md): `ObjC.Block<F>` と `ObjC.block`（Network.framework の `nw_connection_set_state_changed_handler`・`nw_connection_receive` のハンドラー）。M14 の信頼を足す形（verify の Block）は、さらに Phase 2 の `ObjC.call_block`（D5）と、`F` の引数に `ref ObjC.Block<G>` を置く拡張を求める（M14 の D10・S2）。後者は仕様 5 の表にも Phase 2 にもなく、決まっていない（D5 の承認の議題に含める）。
- [M07](M07-apple-sdk-bindgen-and-package.md): Block の型の写し方（`void (^)(id, NSUInteger, BOOL *)` は `ObjC.Block<(Maybe<ref ObjC.Object> -> i64u -> ref mut bool -> unit)>`）。仕様 5 の外の Block の型は生成器が飛ばす。
- [M13](M13-desktop-os-services-package.md): M13 の Phase 2 の通知の完了ハンドラー（`requestAuthorizationWithOptions:completionHandler:` の `(bool -> Maybe<ref NSError> -> unit)`）。ファイルダイアログの `beginWithCompletionHandler:` は M13 の対象外。
- [M03](M03-objc-classes-defined-in-tsuzuri.md)（Phase 2）: IMP の引数の `ref ObjC.Block<F>`。

## 実装手順

### Phase 1

1. **変更**: なし（ブランチ `feat/m04-objc-blocks`、`target/scratch/m04/`）。**内容**: `node scripts/repo-kb.mjs brief M04`、着手条件、ベースライン。clang で「現状」の参照の IR と、仕様 5 の型の組み合わせの Block の型の符号を取り直す。**確認**: flags・記述子・型の符号が「現状」と一致すること（違えば仕様を直してから進める）。
2. **変更**: `src/objc_encoding.rs`（M03 が先なら `@?` と Block の並びを確かめるだけ）。**確認**: 単体テスト（clang の出力から手で写した表）。
3. **変更**: `src/check.rs`・`src/abi.rs`・`src/diagnostic_kinds.rs`。**内容**: 型・`ObjC.block`・`ObjC.copy_block`・引数の位置・`F` の型の集合・kind。**確認**: `tests/objc_blocks.rs` の型の検査。`cargo nextest run --locked`。
4. **変更**: `src/llvm_abi.rs`・`src/llvm.rs`・`src/llvm_imports.rs`。**内容**: literal・記述子・invoke の包み・drop・`extern def` の引数。**確認**: IR の検査（clang の参照と同じ 6 つの `store`、`declare` が 1 行、2 回の生成で一致）。ベースラインと一致（S2）。
5. **変更**: `src/effects.rs`・`src/pure_rules.rs`・`src/kg/effects.rs`・`src/capabilities.rs`・`src/pinned_rules.rs`。**内容**: 効果・権限・件数の pin（今の値 + 足した数）。**確認**: `tests/capabilities.rs`。
6. **変更**: `tests/objc_blocks.mjs`・`tests/objc_blocks_host.m`・`tests/fixtures/objc_blocks/Main.tz`。**内容**: macOS の E2E。**確認**: `cargo build` の後に `node tests/objc_blocks.mjs target/debug/tsuzuri`（-O0・-O3、Apple の ld と同梱の zig）。
7. **変更**: 文書。**確認**: T2、`node scripts/check-docs.mjs <変更したページ>`、`node scripts/gen-docs.mjs --check`、PR に `ci:full`。

### Phase 2

1. M03 の IMP の引数に `ref ObjC.Block<F>` を足し、`extern def`・メンバーの結果に `objc_retainBlock` を足す。**確認**: IR の検査と E2E の `completion` の case。
2. D5 が承認されていれば `ObjC.call_block`、AGENTS.md と `argument-order.md` の資源のハンドルの一覧に `ObjC.Block`。**確認**: `tests/std_argument_order.rs` が通ること（`ObjC` は組み込みだけのモジュールなので `MODULES` の行は要らない）、E2E の `call_received`。

## テスト計画

### Rust（`tests/objc_blocks.rs`、新規。どのホストでも）

| 検査 | 期待 |
| --- | --- |
| 型 | `ObjC.Block<i64>`（`objc_block_not_function`）、`ObjC.block` にラムダ・部分適用・局所（`FnPtr.of` の kind）、最初の引数が `i64` でない（`objc_block_function_type`）、`F` の外の型（`objc_block_type`）、所有する Block の引数（`objc_owned_object_argument`）、`export def` と `@c_layout` の field（`objc_object_position`）、期待型との不一致 |
| 性質 | Copy でない（2 回の使用は移動の誤り）、`tzroutine` に送れる、再利用できる関数値には捕捉できない（`E1005`）、`Owned.function` にはムーブできる |
| target | `--target wasm32` で `abi_native_only`、`--target x86_64-unknown-linux-gnu` で `abi_apple_only`。`check` は通る |
| IR | literal の 6 つの `store` と flags `-1073741824`、記述子 `{ i64 0, i64 40, ptr, ptr }` が型の符号ごとに 1 つ、型の符号の文字列（仕様 5 の例）、invoke の包みの形（鍵を 32 バイト目から読む、`ref mut bool` の 1 バイトの読み書き）、`_Block_copy`・`_Block_release` の `declare` が 1 行、利用者が `_Block_copy` を `extern` で宣言したプログラム、同じ関数を `FnPtr.of` と `ObjC.block` に使うと包みが 2 つ、2 回の生成で一致 |
| 不変 | Block を使わない入力（全 fixture・example）の IR・診断がベースラインと一致 |

### E2E（`tests/objc_blocks.mjs` と `tests/objc_blocks_host.m`、macOS だけ）

ホストの `.m` は `-fobjc-arc` の clang でビルドし、`--link` で渡す。-O0・-O3、Apple の ld と同梱の zig の両方。

| case | 内容 | 期待 |
| --- | --- | --- |
| `dispatch` | 早見の例 | 終了コード 42 |
| `call_now` | ホストの `tzblk_call_now(void (^)(long))` が同じスレッドで 3 回呼ぶ | 状態が 3 |
| `worker` | ホストが `dispatch_async` でグローバルキューから呼び、`f` は `Anchor.update` の結果をホストの `tzblk_record` に渡す | 記録が `WrongThread` の値、状態は変わらない |
| `enumerate` | ホストの `tzblk_enumerate` が `@[@"a", @"b", @"c"]` に `enumerateObjectsUsingBlock:` を呼び、`f` が添字 1 で `stop` を true にする。`stop` の領域は Foundation のものでホストが番兵を置けないので、続けてホストが同じ Block を `b(@"b", 1, &s.stop)`（`s` は `struct { BOOL stop; uint8_t guard[7]; }`）で直接呼ぶ | `enumerateObjectsUsingBlock:` の中で呼ばれた回数が 2。直接の呼び出しの後、`s.stop` が YES で、番兵の 7 バイトが変わらない |
| `signature` | ホストが `_Block_signature(b)` を読む | `"v32@?0@8Q16^B24"`（`enumerate` の Block） |
| `refcount` | ホストが Block の flags の参照数の欄（`BLOCK_REFCOUNT_MASK`）を、作った直後・ホストの `_Block_copy` の後・`ObjC.copy_block` の後・Tsuzuri の drop の後に読み、最後にホストが `_Block_release` する | 2・4・6・4（論理の参照数の 2 倍）、`isa` は `_NSConcreteMallocBlock` |
| `bool_result` | `BOOL (^)(id)` の Block をホストが呼ぶ | true と false が正しく届く |
| `after_main` | ホストが Block を保持し、`atexit` で呼ぶ | 標準エラーに `Tsuzuri runtime: a callback was called after main returned; ...`、トラップの終了コード |
| `blocking` | tzroutine を起動するプログラムで、ホストがブロッキングの `extern` の中から同じスレッドで呼ぶ | 状態が更新され、tzroutine の結果も届く |
| `member` | M02 が done なら、`NSArray` の `enumerateObjectsUsingBlock:` をメンバーとして宣言して呼ぶ | `enumerate` と同じ |

- Phase 2: `completion`（ホストが export のクラスの `fetchWithCompletion:` を呼び、Tsuzuri が `ObjC.copy_block` で保持して後で別の `extern` に渡す）、`returned`（ホストの関数が返す +0 の Block を受け取り、drop する。参照数が釣り合う）、`call_received`（D5 の承認の後。受け取った Block を `ObjC.call_block` で呼ぶ）。
- 性能: `ObjC.block` の作成（`_Block_copy` の確保 1 回）と、clang のスタックの Block（逃げなければ確保なし）・`dispatch_async_f` の費用を、ホストが 100 万回作って比べ、結果を文書に書く（参考値。CI に速さの合否を置かない）。

## ドキュメント

- `_tsuzuri/language-reference/compiler/objective-c.md`（M01 が作る）: 節「Block」（`ObjC.Block<F>`・`ObjC.block`・`ObjC.copy_block`・`F` の型の表・作るたびに確保 1 回の費用（測定値）・GCD のスレッドから呼ばれたときの `WrongThread` と `MainThread.post`・`main` の後に呼ばれないように取り消す手順）。Phase 2 で受け取った Block と `ObjC.call_block`。
- `_tsuzuri/language-reference/built-in-types-and-modules/anchor.md`: 別のスレッドの Block・コールバックから引くと `WrongThread` になること、M09 があれば `MainThread.post` で main に移ること。
- `_tsuzuri/language-reference/built-in-types-and-modules/fnptr.md`: `FnPtr<F>`（C の関数ポインター）と `ObjC.Block<F>`（Apple の Block）の違いと使い分け。
- `_tsuzuri/language-reference/compiler/native-interop.md`: 「静的コールバック」の節から objective-c.md へのリンク。
- `docs/language.md`: 「ホスト関数のインポート」の節に Block の小節。診断の表は `node scripts/gen-docs.mjs` の生成の部分を使う。
- `docs/architecture.md`: literal の配置、記述子と型の符号、invoke の包みの名前、ヒープに移す理由。
- Phase 2（D5 の承認の後）: `AGENTS.md` と `_tsuzuri/language-reference/built-in-types-and-modules/argument-order.md` の資源のハンドルの一覧に `ObjC.Block`。

## 受け入れ条件

- [ ] Phase 1 の仕様 1〜6 のとおりに動き、仕様の外の入力は表の診断で拒否される。
- [ ] Block を使わないプログラムの IR・object・header・診断・既定の `.wasm` がベースラインと 1 byte も違わない。M01〜M03 の fixture の IR も変わらない。
- [ ] literal・記述子・flags が clang の参照と一致し、生成 IR が決定的で、`declare` が symbol ごとに 1 行。
- [ ] `node tests/objc_blocks.mjs` が macOS の -O0・-O3、Apple の ld と同梱の zig で通り、残ったプロセスがない。
- [ ] 作成の費用を測り、文書に書いた。
- [ ] T2 が通る。PR に `ci:full` を付けた。

## 落とし穴

- **libsystem_blocks の symbol**: `_NSConcreteStackBlock`・`_Block_copy`・`_Block_release` は libSystem の libsystem_blocks にある。コンパイラが生成する参照なので、利用者の `extern` の `__` の予約は関係ない。予約もしない（今日の手書きの宣言を壊さない）。
- **大きさの一致**: literal（40 バイト）と記述子の size を一致させる。鍵は POD なので `BLOCK_HAS_COPY_DISPOSE` を立てない。立てると `_Block_copy` が存在しない helper を呼ぶ。
- **drop の忘れ**: 漏れるだけ（安全側）。逆に、C の API に渡した Block を Tsuzuri が drop しても、C が `_Block_copy` で保持していれば生きている。
- **`main` の後**: `dispatch_after`・`NSEvent` のモニター・`nw_connection` のハンドラーが `main` の後に Block を呼ぶと W12 のトラップになる。`removeMonitor:`・`nw_connection_cancel`・`dispatch_source_cancel` を `main` が返る前に呼ぶ手順を文書に書く。
- **`@tz.main.done` の出し忘れ**: 今は `globals.callbacks` が空でないときだけ、`@tz.main.done` の大域と `main` の終わりの `store` を出す（`src/llvm.rs:2238` の `mark_main_done`、`src/llvm.rs:9999-10003`）。invoke の包みも入口で `@tz.main.done` を読むので、`FnPtr` を使わず Block だけを作る実行ファイルでも出るように、この条件に invoke の包みを足す。足さないと、IR が定義のない大域を参照して clang が拒む。
- **`bool` の幅**: `F` の中の `bool` は `i1`、`ref mut bool` は 1 バイト。`extern def` の `bool`（`int32_t`）と混ぜない。同じ C の関数を `extern def` で宣言したときの `bool` は今のまま。
- **受け取った Block（Phase 2）**: IMP が受けた `ref ObjC.Block<F>` はスタックの Block のことがあり、呼び出しの後は無効。保持するなら `ObjC.copy_block`。
- **利用者の宣言との共存**: 「現状」のように利用者が `_Block_copy` を別の型で `extern` 宣言していても、`declare` を 2 つ出さない（LLVM が拒む）。

## 対象外

- 任意の値の捕捉（捕捉は鍵 1 つ。状態は `Anchor`）。
- `__block` の変数、copy・dispose の helper の要る捕捉。
- Swift のクロージャーとの橋渡し。
- グローバルな Block の静的な定数（`_NSConcreteGlobalBlock`）を利用者に見せること。
- 物体・ハンドル・record を返す Block、record を値で取る Block（`void (^)(NSRect)`）。
- Apple でない OS の Blocks（clang の libBlocksRuntime）。

## 決定事項

| ID | 決定 | 採らない案 | 理由 | 状態 |
| --- | --- | --- | --- | --- |
| D1 | `ObjC.Block<F>` の値は常にヒープの Block（スタックの literal を `_Block_copy` で移す） | 逃げないときはスタックのまま渡す、グローバルの Block | スタックの Block の寿命の解析が要らず、意味が 1 つになる。費用は作るたびの確保 1 回で、測定して文書に書く | 既定案（実装者はこの案に従う） |
| D2 | `ObjC.block` の最初の引数はトップレベルの関数の名前（静的コールバックと同じ規則で、ラムダ・部分適用・局所は `E1008`）。2 つ目は鍵（`Anchor.read f token` と同じ並び） | 関数値（クロージャー）を取る、鍵を先に置く | 包みを静的に作れる（W22 の `FnPtr.of` と同じ）。対象のない構築なので D-59 の例外は要らない | 既定案（実装者はこの案に従う） |
| D3 | flags は clang が Objective-C で作るスタックの Block と同じ `BLOCK_HAS_SIGNATURE \| BLOCK_HAS_EXTENDED_LAYOUT`（`-1073741824`）、記述子は `{ 0, 40, 型の符号, 空の layout }`。型の符号は `src/objc_encoding.rs`（M03 と先に入ったほうが作る）で作る | `BLOCK_HAS_SIGNATURE` だけで layout を NULL にする（clang が C の `-fblocks` で作るスタックの Block と、グローバルの Block の形。「現状」）、型の符号なし | `NSInvocation` と Swift の橋渡しが符号を読む。捕捉は鍵（POD）だけなので、どちらの flags でも libsystem_blocks の扱いは同じである。テストが突き合わせるホストの `.m` の参照（Objective-C）と 1 対 1 で比べられる形にする | 既定案（実装者はこの案に従う） |
| D4 | invoke の包みは `(block, 引数…)` を受け、literal の鍵を読み、`f(鍵, 引数…)` を呼ぶ。名前は `@tz.callback.objc-block.<修飾名>` で、W12 の族と同じ扱い（`main` の後の呼び出しはトラップ、tzroutine の P の取り戻し、ランタイムの外のスレッドの境界） | Block 専用の入口と出口、包みを複製する | 包みを 1 種類に保つ（W12・S1）。GCD のスレッドの扱いは今のコールバックで確かめてある（「現状」） | 既定案（実装者はこの案に従う） |
| D5 | Phase 2 の `ObjC.call_block (ref b) a1 … an` は、`Owned.call (ref f) x` と同じくハンドルを先頭に置く。AGENTS.md の D-59 の資源のハンドルの一覧（閉じている）に `ObjC.Block` を足す | 対象を最後に置く（`ObjC.call_block a1 … an (ref b)`）、呼べないままにする | 引数の数が `F` で変わるので、最後に置くと部分適用とパイプの形が崩れる。資源のハンドルの例外は .NET の受け手と同じ並び。承認されなければ、Phase 2 は受け取った Block を呼ばずに保持して渡すだけにする | 要承認（承認前は `ObjC.call_block` の段に着手しない） |
| D6 | D-67 の分岐 1（X′）の下で、`ObjC.Block`（型）・`ObjC.block`・`ObjC.copy_block`・`ObjC.call_block`（組み込みの関数）を D-55 に登録する（`ObjC` は M01 が予約する組み込みのモジュール） | 別のモジュール `Block` を作る | `RESERVED_MODULES` を増やさない。Block は ObjC のランタイムの概念で、M01 の所有の規則（`ref` は +0 の借用、所有値は drop で解放）を共有する | 要承認（承認前は Phase 1 に着手しない） |
| D7 | `ObjC.Block<F>` は Send・Sync で、Copy・Clone ではない。drop を持つので、再利用できる関数値には捕捉できない（M01 の `sync` のクラスと同じ） | M01 のクラスと同じ `local` | Block の参照数は atomic で、どのスレッドで drop してもよい。Block は鍵しか持たず、状態の検査は `Anchor` が持つ。GCD に渡す Block を tzroutine で作れる | 既定案（実装者はこの案に従う） |
| D8 | `F` の型の集合は、コールバックの型に `ref C`・`Maybe<ref C>`・`ObjC.Sel`・`ObjC.Class`・1 バイトの `ref mut bool`・スカラーの `ref mut T` を足したもの。`bool` は `i1 zeroext`。結果はスカラーか unit | `extern def` と同じ `int32_t` の `bool`、物体の結果を許す | Block は Apple の C の型（`BOOL`・`bool` は 1 バイト）で呼ばれる。物体の結果は autorelease の規則が要り、要る API が少ない | 既定案（実装者はこの案に従う） |
| D9 | Phase 2 の受け取り: IMP の引数は +0 の借用（スタックの Block のことがある）、結果の +0 は `objc_retainBlock`、保持は `ObjC.copy_block` | 結果に `objc_retainAutoreleasedReturnValue` を使う、IMP の引数を所有にする | `objc_retainBlock` はスタックの Block もヒープに移すので、どんな呼び出し先にも安全。IMP の物体の引数を +0 にする M03 の D6 と同じ規則 | 既定案（実装者はこの案に従う） |
| D10 | `ObjC.Block<F>` の表現は、`Anchor<'a>`・`Owned.Function<'a, 'b>` と同じく、型引数を持つ std の record に名前で特別な扱い（Copy でない・drop で `_Block_release`・ABI では Block のポインター）を付ける形にする。`Type` に variant を足さない。`ObjC` に std のファイルが要るなら、M01 の `ObjC` の作り方に合わせて足す。[M23](M23-objc-out-parameters-and-weak-references.md) の Phase 2 の `ObjC.Weak<C>` も同じ仕組みを使う。M04 と M23 の Phase 2 のうち先に入ったほうが作る | M01 の `HandleKind` の種類だけで表す、`FnPtr(Box<Type>)` と同じ `Type` の variant を足す | `Type::Handle(Box<str>, HandleKind)` は型引数を持てず、`F` を運べない（「設計」）。`Type` の variant は M01 の S1 が避ける変更で、全域の `match` に腕が要る。幽霊の型引数を持つ std の record は `Anchor<'a>` に先例がある | 既定案（実装者はこの案に従う） |
