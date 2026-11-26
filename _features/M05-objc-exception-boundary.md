# M05: Objective-C の例外の境界（`Result<R, ObjC.Exception>` と宣言した呼び出しの地点だけで受ける。D-10 の狭い解釈。D-67 の分岐 2 で承認）

| 項目 | 内容 |
| --- | --- |
| ID | M05 |
| 優先度 | P2（正しく書いた AppKit の呼び出しで NSException はふつう起きない。例外を投げうる呼び出しは ObjC のシムの `@try` でも包める。D-67 の分岐 2 は承認済み（2026-11-26）） |
| 規模 | M（結果の位置の検査、`invoke` と landingpad の生成、`ObjC.Exception` と 2 つの関数、ブロッキングの包みの対応、macOS の E2E、文書） |
| 依存 | [M02](M02-objc-typed-message-send.md) |
| 後続 | –（弱い後続: [M07](M07-apple-sdk-bindgen-and-package.md)） |
| 状態 | todo |
| 起票 | 2026-11-26（D-67。利用者の依頼「「どのチケットにもない壁」と「調査中に見つけたコンパイラの不具合（自分で再現済み）」のそれぞれについて @_features/ に開発チケットまたは修正チケットを作成して」から起票。調査で報告した「壁 1 の @try と配布の注意（NSException が Tsuzuri のフレームを越えると abort。exit 134）」に当たる。HEAD `804b7970` で確認） |
| 承認 | [GUIDE D-67](GUIDE.md#d-67-tsuzuri-だけで-macos-のデスクトップアプリを書くm01m22) の分岐 2（D-10 の狭い解釈。README の「採用しない改善」の「例外と巻き戻し」の行への注記。決定事項 D1）と分岐 1（X′）は承認済み（2026-11-26）。要承認（D-55（`ObjC.Exception`・`ObjC.exception_name`・`ObjC.exception_reason`））。決定事項の D9 が要承認、ほかは既定案 |
| 改善する劣位 | Objective-C・Objective-C++（`@try`／`@catch`）比: AppKit・Foundation のメソッドが投げる NSException（`NSRangeException`・`NSInvalidArgumentException` など）を Tsuzuri の中で値として受けられない。Tsuzuri のフレームを越えると、外側にホストの `@catch` があってもプロセスが終わる（exit 134）。今の回避策は、`@try` を持つ ObjC のシムを C のツールチェーンで作って包むことだけである（Swift も NSException を受けられずシムに頼るが、Tsuzuri は D-67 で AppKit を直接呼ぶので影響が大きい） |
| 手本にする既存実装 | D-34 の `try ... with`（同じ関数本体の中で受け、関数の境界を越えない。型検査 `src/exceptions.rs`、生成 `src/llvm_exception.rs` の `try_expression`）。W07 の `traps::with_unwind_tables`（`src/llvm_traps.rs:443-447`。関数の属性を IR の行に足す所）。`src/llvm_imports.rs` の `FunctionEmitter::host_call`（引数の下げ方と呼び出しの後の後始末）。`src/llvm.rs` の `with_blocking_externs`（TZ08 の包み）。clang 21 の `@try`／`@catch (id e)` の参照の IR（「現状」） |
| 主な影響ファイル | `src/check.rs`, `src/abi.rs`, `src/llvm_imports.rs`, `src/llvm_objc.rs`（M01・M02 が作る ObjC の生成）, `src/llvm.rs`, `src/llvm_preempt.rs`, `src/llvm_traps.rs`, `src/effects.rs`, `src/pure_rules.rs`, `src/kg/effects.rs`, `src/pinned_rules.rs`, `src/capabilities.rs`, `src/diagnostic_kinds.rs`, `tests/objc_exceptions.rs`（新規）, `tests/objc_exceptions.mjs`（新規）, `tests/fast_backend.rs`, `tests/objc_exceptions_host.m`（新規）, `tests/fixtures/objc_exceptions/Main.tz`（新規）, `_tsuzuri/language-reference/compiler/objective-c.md`（M01 が作る）, `_tsuzuri/language-reference/compiler/native-interop.md`, `_tsuzuri/language-reference/exception-handling/exception-handling.md`, `_tsuzuri/language-reference/exception-handling/try-with-finally.md`, `docs/language.md`, `docs/architecture.md`, `_features/README.md`, `_features/GUIDE.md` |

## 実装者向け早見

- 範囲: M02 のメンバーと `extern def` の結果を `Result<R, ObjC.Exception>`（void は `Result<unit, ObjC.Exception>`）と書けるようにする。その呼び出しの地点だけを `invoke` にし、ObjC の物体の例外（`catch ptr @OBJC_EHTYPE_id`）を受けて `Error e` にする。受けるのは、投げたホストのフレームからその呼び出しを含む関数までの間だけで、Tsuzuri のフレームは巻き戻さない。ほかのすべての経路（`Result` でない呼び出し、コールバックのフレーム、C++ の例外）は今のまま abort する。
- 最初に読む: ① 「現状」の再現（exit 134 と、`-O3` の末尾呼び出しで結果が変わる例）② 「設計」の検証した下げ方（手で書いた IR が `-O0`・`-O3` で受けた）③ `host_call` の呼び出しの後の後始末（`src/llvm_imports.rs:380-449`）④ `with_blocking_externs`（`src/llvm.rs:1840-1915`）と `direct_callee`（`src/llvm.rs:1815-1820`）⑤ 関数の見出しに属性と `section` を足す 4 つの後工程の pass（「設計」の「後工程の text の pass」。どれも ` {` の直前に足すので、`personality` の後ろに入って見出しを壊す）。
- 最初に書くテスト: `tests/objc_exceptions.rs` の `boundary_result_is_accepted_only_on_extern_results`（受理と `E1008` の位置）と `boundary_lowers_to_invoke_only_where_declared`（`--emit llvm --target aarch64-apple-darwin` で `personality` を持つ関数が境界の関数だけ）。次に `tests/objc_exceptions_host.m` の `@throw` するメソッドと `tests/objc_exceptions.mjs` の `nsexception`。
- 触ってはいけない: 境界を使わないプログラムの IR・object・header・診断・既定の `.wasm`（1 byte も変えない）。Tsuzuri の関数の `nounwind`。`uwtable` を Windows 以外に足さない。`try ... with`（D-34）の意味。C++ の例外の振る舞い（今のとおり terminate）。
- 完了の見分け方: 次のプログラムを `tests/objc_exceptions_host.m` の `TzThrower`（`-boom` が `@throw [NSException exceptionWithName:@"TzProbe" reason:@"from host" userInfo:nil]` を実行する）とリンクして動かすと、`-O0`・`-O3` の両方で `TzProbe: from host` を出して 0 で終わる。今は同じことを書く手段がなく、手書きの `extern "objc_msgSend"` で `-boom` を送ると exit 134 で終わる（「現状」）。

```tsuzuri
@trusted
extern class TzThrower : ObjC.Object {
    class "new" make :: unit -> Self
    boom :: unit -> Result<unit, ObjC.Exception>
}

def main :: unit -> i32 = \() ->
    let thrower = TzThrower.make ()
    match TzThrower.boom (ref thrower) with
    | Ok () -> 1
    | Error e ->
        do! IO.writeln $"{ObjC.exception_name (ref e)}: {ObjC.exception_reason (ref e)}"
        0
```

## 目的

AppKit と Foundation は、プログラムの誤り（範囲外の添字、知らないセレクター、nil を許さない引数への nil）を NSException で知らせる。Objective-C では `@try`／`@catch` で受けられるが、Tsuzuri が呼んだホストのメソッドが投げた例外は、今はすべてプロセスを終わらせる。Tsuzuri が生成する関数は巻き戻しの表を持たないので、例外は最初の Tsuzuri のフレームで探索が止まり、外側にホストの `@catch` があっても届かない（「現状」）。

D-67 の分岐 1（X′）で AppKit を Tsuzuri から直接呼ぶと、例外を投げうる呼び出しの数が増える。投げうると分かっている呼び出し（`-[NSArray objectAtIndex:]`、`-[NSDictionary setObject:forKey:]` に外から来た値を渡す呼び出し、`-[NSFileHandle readDataOfLength:]` など）を、シムなしに値で扱える必要がある。

このチケットは、利用者が `Result<R, ObjC.Exception>` と宣言した呼び出しの地点だけで例外を受ける。D-34 の `try ... with` と同じく「同じ関数の中で受ける」形で、Tsuzuri のフレームは巻き戻さない。これを D-10「例外・巻き戻しは導入しない」の狭い解釈として記録する。この解釈は D-67 の分岐 2 で承認された（2026-11-26。決定事項 D1）。

## 着手条件と停止条件

### 着手条件

- [M02](M02-objc-typed-message-send.md) の Phase 1 が done（メンバーの型付きの送信、selref、`declare` を 1 つにする規則、送信のブロッキングの包み）。M02 は [M01](M01-objc-class-types.md)（`ObjC.Object`・所有・`abi_apple_only`）を前提にする。確認: `grep -nE "^\| M0[12] " _features/README.md`。
- [GUIDE D-67](GUIDE.md#d-67-tsuzuri-だけで-macos-のデスクトップアプリを書くm01m22) の分岐 1（X′）と分岐 2 は承認済み（2026-11-26。分岐 2 は決定事項 D1）。決定事項 D9 が承認されている。**承認が記録されるまで、該当する Phase に着手しない**（このチケットは Phase に分けないので、全体に着手しない）。分岐 2 を認めずにこのチケットを閉じる案（D1 の退けた案 (a)）は選ばれなかった。
- 「現状」の再現を着手時の HEAD で実行し、食い違えばこのチケットを直してから始める。
- ベースライン: `tests/fixtures/*/Main.tz` と `examples/*/Main.tz` の `--emit llvm`（native、`--target aarch64-apple-darwin`、`--target wasm32`）、`--emit header`、`check --json` を `target/scratch/m05/base/` に保存する（受け入れ条件の比較に使う）。M01・M02 の fixture（ObjC を使うが境界を使わない）も含める。

### 停止条件

- **S1** 境界を受けるのに、Tsuzuri の関数の属性を広く変える必要がある（すべての関数の `nounwind` を外す、`uwtable` を macOS のすべての関数に足す、など）。
- **S2** 境界を使わないプログラムの IR・object・header・診断・既定の `.wasm` が、保存したベースラインと 1 byte でも違う。
- **S3** `-O3` で境界の `invoke` が `call` に変わる、または landingpad が消え、境界の関数の中の局所的な手段（呼び出し先の `declare` に `nounwind` を付けない、など）では防げない。
- **S4** M02 の送信の下げ方（引数の下げ方と呼び出しの後の後始末）を、`call` と `invoke` で共有できず、複製したくなった。
- **S5** Apple の ld か同梱の zig（lld）のどちらかで、境界の関数の LSDA と compact unwind が正しく作られず、例外が受からない。同梱の zig は、[M20](M20-macos-framework-link-inputs.md) が SDK の `-F`・`-L` を足すまで `-l objc` を探せず `E2002` になる（M01 の「現状」）。M20 が入っていなければ zig の確認は飛ばし、理由を PR に書く（M20 は依存ではない）。
- **S6** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `804b7970` で確認）

確かめた環境は macOS 27.2.1（arm64）、Apple clang 21.0.0（clang-2100.4.1.1）、SDK 27.2、`target/debug/tsuzuri`（HEAD で作り直したもの）である。

- **例外を受ける構文がない**: `ObjC` のモジュールはない。`Result` を `extern def` の結果に書くと `E1008`（kind `unsupported_extern_signature`、`extern parameters support scalars, ... results support scalars, unit, handles, owned buffers and scalar records`）。`Result<i64, ObjC.Exception>` は `E1004`（`unknown record, union, or type alias 'ObjC.Exception'`）。
- **巻き戻しの表**: `uwtable` を付けるのは Windows の IR だけである。`windows_abi`（`src/llvm.rs:237`）の最後で `traps::with_unwind_tables(&output)`（`src/llvm.rs:368`）を呼び、`with_unwind_tables`（`src/llvm_traps.rs:443-447`）が `define` の行に ` uwtable` を足す。生成する Tsuzuri の関数は `define internal ... nounwind` で、`personality` を持つ関数はない（`grep -rn personality src/llvm*.rs` はテストの 1 行だけ）。`extern def` の `declare` は属性を持たない（`declare i64 @objc_msgSend(i64, i64)`）。
- **作られる unwind の情報**: 下の再現 1 の実行ファイルを `objdump --macho --unwind-info` で見ると、`tz.fn.*` などの生成した関数の encoding はすべて `0x00000000`（記録なし）である。0 でないのは、生成した C の `@main`（`_main`。`define i32 @main(...)` は `nounwind` を持たないので、DWARF の記録 `0x03000014` を持つ）と、ランタイムの C の `_tsuzuri_io_write`（`0x04000000`）だけだった。
- **契約の文**: `_tsuzuri/language-reference/compiler/native-interop.md:626`「ホストは信頼境界です。例外の unwind、呼び出しが返ったあとのポインタの保持、回復不能な失敗から Tsuzuri のフレーム越しに戻ることはしません。」。`docs/language.md:465` に同じ趣旨の文がある。
- **決定**: D-10（`_features/GUIDE.md:751-757`）は「例外・巻き戻しは導入しない」と定め、D-34 の改訂（同 757 行、本文は 1115-1117 行）で `@checked` の overflow だけを `try ... with` が同じ関数本体の中で捕まえるとした。`_features/README.md:112` の「採用しない改善」に「例外と巻き戻し、`?` 演算子」の行があり、`_features/GUIDE.md:1649` の「守る方針」が「採用しない改善」を変えないと定める。
- **予約された symbol**: `__` で始まる名前は利用者の `extern` に書けない（`src/abi.rs:55` の `RESERVED_SYMBOL_PREFIXES`）。`__objc_personality_v0` は利用者には宣言できないが、コンパイラが生成するのはよい（D-67 の共通の規則）。
- **libobjc と SDK**: `dyld_info -exports /usr/lib/libobjc.A.dylib` に `_OBJC_EHTYPE_id`・`___objc_personality_v0`・`_objc_begin_catch`・`_objc_end_catch`・`_objc_exception_throw`・`_objc_opt_isKindOfClass` がある。SDK の `usr/include/objc/objc-exception.h:51-57` は `objc_begin_catch`・`objc_end_catch` を宣言するが、`__objc_personality_v0` と `OBJC_EHTYPE_id` を宣言するヘッダーは SDK の `usr/include` にない。`NSException` のクラスは CoreFoundation にある（`CoreFoundation.tbd` の `objc-classes` と `objc-eh-types: [ NSException ]`）。
- **ブロッキングの包み**: `with_blocking_externs`（`src/llvm.rs:1840-1915`）は、tzroutine を起動するネイティブのプログラムで、`direct_callee`（`src/llvm.rs:1815-1820`）が extern の symbol を呼ぶ行の前後に `tsuzuri_tz_blocking_enter`・`_exit` の行を足す。`direct_callee` は `call ` の後の `@` を探すので、`invoke` の行は照合されず、包まれない。照合したとしても、終端の命令である `invoke` の後に行を足すと IR が壊れる。
- **横取り（TZ10）**: `src/llvm_preempt.rs:346-358` の `TERMINATORS` は `invoke ` を含み、基本ブロックの区切りは扱える（`src/llvm_preempt.rs:1148` のテストに `invoke` の行がある）。ただし、ループを重くする呼び出しの判定は終端の行を見ない（`src/llvm_preempt.rs:417` の `(start..terminator)`）。`Parallel.*` の補助の解析（`helper_wait`）は `invoke` を「待ちうる」と保守的に扱う（`src/llvm_preempt.rs:728`）。
- **fast のコード生成器（PB05）**: `src/backend/module.rs:452` は関数の属性 `personality` を見ると `bail!("the function attribute '{word}'")` し、`E2002`（kind `fast_backend_unsupported_construct`、`the fast backend cannot translate ... ; build with --backend llvm and report this program`。`src/backend/error.rs:33-46`）になる。
- **`host_call` の後始末**: `src/llvm_imports.rs` の `host_call`（65 行から）は、呼び出しの後に `ref mut` の書き戻し（`finish_copy_outs`、380 行）、Latin-1 の文字列の一時の複写の解放（`free_copies`）、借用した operand の後始末（`release_operands`、423・449 行）をする。`extern def` は本体が `HostCall` だけの Tsuzuri の関数（`tz.fn.<Module>.<name>`）になり、呼び出しはその関数の中にある。

### 再現 1: 知らないセレクターを送ると exit 134

`target/scratch/m05/exc/Main.tz`:

```tsuzuri
@trusted
@count(1, none)
extern "objc_getClass" def objc_get_class :: ref utf8string -> i64

@trusted
@count(1, none)
extern "sel_registerName" def sel :: ref utf8string -> i64

@trusted
extern "objc_msgSend" def msg0 :: i64 -> i64 -> i64

def main :: unit -> i32 = \() ->
    let obj = msg0 (msg0 (objc_get_class (ref u8"NSObject\0")) (sel (ref u8"alloc\0"))) (sel (ref u8"init\0"))
    do! IO.writeln "sending an unknown selector (raises NSInvalidArgumentException)"
    let _ = msg0 obj (sel (ref u8"noSuchMethodForProbe\0"))
    do! IO.writeln "not reached"
    0
```

```sh
SDK=$(xcrun --show-sdk-path)
target/debug/tsuzuri build target/scratch/m05/exc -O0 -l objc --link $SDK/System/Library/Frameworks/Foundation.framework/Foundation.tbd -o target/scratch/m05/exc/exc
target/scratch/m05/exc/exc; echo "exit=$?"
```

出力（`-O3` も同じ。呼び出しの履歴の 6 行目が `-O0` は `tz.fn.Main.msg0 + 48`、`-O3` は `main + 356` で、履歴はそこで終わる）:

```text
sending an unknown selector (raises NSInvalidArgumentException)
*** Terminating app due to uncaught exception 'NSInvalidArgumentException', reason: '-[NSObject noSuchMethodForProbe]: unrecognized selector sent to instance 0x102a24890'
...
libc++abi: terminating due to uncaught exception of type NSException
exit=134
```

Foundation をリンクしないと、例外ではなく libobjc の `unrecognized selector ... (no message forward handler is installed)` で abort する（同じく exit 134）。

### 再現 2: ホストの `@try` が Tsuzuri のコールバックを包んでも受からない（`-O3` の末尾呼び出しだけは例外）

`target/scratch/m05/cb/host.m`（`clang -c -fobjc-arc target/scratch/m05/cb/host.m -o target/scratch/m05/cb/host.o`）:

```objc
#import <Foundation/Foundation.h>
typedef void (*tz_cb)(void);
int tzhost_try(tz_cb cb) {
    @try { cb(); return 0; } @catch (id e) { return 1; }
}
void tzhost_throw(void) {
    @throw [NSException exceptionWithName:@"TzProbe" reason:@"from host" userInfo:nil];
}
```

`target/scratch/m05/cb/Main.tz`:

```tsuzuri
@trusted
extern "tzhost_throw" def host_throw :: unit -> unit

@trusted
extern "tzhost_try" def host_try :: (unit -> unit) -> i32

def on_call :: unit -> unit = \() ->
    host_throw ()
    do! IO.writeln "not reached"

def main :: unit -> i32 = \() ->
    let caught = host_try on_call
    do! IO.writeln $"host caught: {caught}"
    0
```

```sh
target/debug/tsuzuri build target/scratch/m05/cb -O0 --link target/scratch/m05/cb/host.o -l objc --link $SDK/System/Library/Frameworks/Foundation.framework/Foundation.tbd -o target/scratch/m05/cb/cb
```

`-O0` と `-O3` のどちらも `*** Terminating app due to uncaught exception 'TzProbe', reason: 'from host'` と `libc++abi: terminating due to uncaught exception of type NSException` で exit 134。ホストの `@catch` には届かない。

ところが `on_call` を `\() -> host_throw ()`（呼び出しが末尾）にすると、`-O0` は exit 134 のままだが、`-O3` は `host caught: 1` を出して 0 で終わる。`-O3` では `tz.callback.Main.on_call` が `b _tzhost_throw`（末尾呼び出し。`otool -tv` で確認）になり、投げた時点のスタックに Tsuzuri のフレームが残らないからである。今の「abort する」は、Tsuzuri のフレームがスタックに残っている場合の振る舞いで、最適化の水準で結果が変わる。

### 参照: clang 21 の `@try`／`@catch (id e)`

`long probe(id obj, NSString **name) { @try { return (long)[obj hash]; } @catch (id e) { *name = [e description]; return -1; } }` を `clang -target arm64-apple-macosx12.0 -fobjc-arc -fno-objc-msgsend-selector-stubs -O1 -S -emit-llvm` で下げると、関数は `personality ptr @__objc_personality_v0` と属性 `ssp uwtable(sync)` を持ち、送信は `%5 = invoke i64 @objc_msgSend(ptr noundef %0, ptr noundef %4) to label %21 unwind label %6` になる。landing pad は `landingpad { ptr, i32 } catch ptr @OBJC_EHTYPE_id`（`@OBJC_EHTYPE_id = external global %struct._objc_typeinfo`）で、選択子を `llvm.eh.typeid.for` と比べ、`objc_begin_catch` の結果を `llvm.objc.retain` してから `objc_end_catch` を呼ぶ。宣言は `declare ptr @objc_begin_catch(ptr)`・`declare void @objc_end_catch()`・`declare i32 @__objc_personality_v0(...)` で、`objc_msgSend` の `declare` は `nonlazybind` だけを持ち `nounwind` を持たない。`NSRange` を値で取る `-getCharacters:range:` は `call void @objc_msgSend(ptr, ptr, ptr, [2 x i64])` に下がる（決定事項 D3 で使う）。

## 変えてはいけない意味

- D-10 の本体: Tsuzuri のフレームは巻き戻さない。`Drop` を飛ばす経路を作らない。トラップは今のまま巻き戻さない。
- `try ... with`（D-34）の意味。`try` は ObjC の例外を受けない（関数の境界も越えない）。
- 境界を使わないプログラムの IR・object・header・診断・既定の `.wasm`（1 byte も変えない）。境界を使うプログラムでも、境界の関数（とそれを取り込んだ関数）、足す宣言と大域、M01 のモジュールのフラグのほかは IR を変えない。
- 評価順序（左から右）。境界の呼び出しの引数も、`Result` でない呼び出しと同じ順と同じ後始末で扱う。
- C++ の例外は今のとおり受けず、探索の段で terminate する（ホストのフレームのデストラクターは走らない）。

## 仕様

### 1. 書ける位置

- 結果の型 `Result<R, ObjC.Exception>` は、M02 の `extern class`・`extern category` のメンバーと、`extern def` の結果にだけ書ける。`R` は、その位置に `Result` なしで書ける結果の型（M01・M02 の規則。スカラー、`unit`、`bool`、`C`、`Maybe<C>`、`Self`、`Maybe<Self>` など）である。void の呼び出しは `Result<unit, ObjC.Exception>`。
- 誤りの型が `ObjC.Exception` でない `Result` は、今のとおり `E1008`（`extern def` は `unsupported_extern_signature`、メンバーは M02 の kind）。`R` が `Result` のもの（入れ子）も同じ。W25 の `Result<J, i32>`（HRESULT と out の引数）とは誤りの型で区別する。
- `export def`・コールバックの型・`FnPtr`・`@c_layout` の field に `ObjC.Exception` を置くと、M01 の `E1008` `objc_object_position`。
- Tsuzuri のふつうのコードでは `Result<R, ObjC.Exception>` はふつうの型で、`match`・`Result.map`・record の field などで使える。
- 宣言は target に依らず受理する（`check` は通る）。wasm32・wasm64 で境界の呼び出しに到達したビルドは `E1008` `abi_native_only`、Apple でない native は `E1008` `abi_apple_only`（M01 の kind。最初に出荷したチケットが登録する）。どちらも `build` のときに出す。

### 2. 呼び出しの意味

| 呼び出しの間に起きたこと | 結果 |
| --- | --- |
| ふつうに戻った | `Ok r`。`r` の作り方は `Result` なしの宣言と同じ（+0 の物体の結果の retain、`bool` の正規化、nil の `Maybe`、`Maybe` でない物体の結果の nil は NS01 の `null extern handle` のトラップ） |
| ObjC の物体の例外（`@throw`・`-[NSException raise]`。NSException でない物体も含む） | `Error e`。`e : ObjC.Exception` は投げられた物体の強い参照を 1 つ所有する |
| C++ の例外とそのほかの外来の例外 | 受けない。今のとおり探索の段で terminate する（exit 134、`libc++abi: terminating due to uncaught exception of type ...`） |

- 巻き戻すのは、投げたホストのフレームから、その呼び出しを含む関数（境界の関数）までの間のホストのフレームだけである。境界の関数は戻り、Tsuzuri の呼び出し元は何も巻き戻されない。
- ホストのフレームの後始末は、そのホストのコードの性質に従う。clang の ARC は既定（`-fno-objc-arc-exceptions`）で例外の経路の release を省くので、ホストの側で物体が漏れることがある（Objective-C の `@catch` と同じ）。
- 例外の経路でも、呼び出しの後の後始末は同じに行う: 一時の複写の解放（`free_copies`）と、借用した operand の後始末（`release_operands`）。`ref mut` の record とスカラーの書き戻し（`finish_copy_outs`）は行わない（呼び出しが完了していないので、Tsuzuri の場所は呼び出しの前の値のまま）。複写せずに渡す `ref mut` の buffer は、ホストが投げる前に書いた分が残りうる（決定事項 D6）。
- init の族のメンバー（受け手を消費する）が投げたときは、受け手は消費済みとみなし、release しない（漏れうるが二重解放しない。clang の ARC の既定と同じ。決定事項 D6）。
- 境界でない呼び出し（`Result<_, ObjC.Exception>` でない宣言）と、コールバック・export のフレームを越える例外は、今のまま abort する。ただし最適化でその Tsuzuri のフレームが消えていれば（末尾呼び出し。「現状」の再現 2）、外側のホストの `@catch` に届くことがある。言語はこれを保証しないので、頼ってはならない（決定事項 D10）。
- tzroutine の中でも受けられる。巻き戻しはその tzroutine のスタックの中で、投げたフレームから境界の関数までで終わる。

### 3. `ObjC.Exception` と取り出しの関数

| 名前 | 型 | 意味 |
| --- | --- | --- |
| `ObjC.Exception` | 型 | 組み込みの ObjC のクラスの型。根は `ObjC.Object`。投げられた物体の強い参照を 1 つ所有する（+1）。drop で `objc_release`。Copy でない。M01 の `sync` の性質（Send・Sync。drop で release するので、M01 の `sync` のクラスと同じく再利用できる関数値には捕捉できない。決定事項 D4）。`ObjC.upcast (ref e)` で `ref ObjC.Object` に、`ObjC.upcast_owned e` で `ObjC.Object` に変換できる |
| `ObjC.exception_name` | `ref ObjC.Exception -> string` | NSException（とその派生）なら `-name` の文字列。そうでなければ空の文字列 |
| `ObjC.exception_reason` | `ref ObjC.Exception -> string` | NSException なら `-reason` の文字列（nil は空）。そうでなく `-description` に応える物体なら `-description` の文字列。どちらでもなければ空 |

- NSException かどうかは、実行時に `objc_lookUpClass("NSException")` で引いたクラスと `objc_opt_isKindOfClass` で決める。クラスが読み込まれていなければ（nil）NSException ではない。静的な classref を使わないので、境界は CoreFoundation のリンクを要しない（決定事項 D3）。`-description` に応えるかは `class_respondsToSelector(object_getClass(e), @selector(description))` で確かめる。
- 文字列の取り出しは、内部の送信 `-length` と `-getCharacters:range:`（`NSRange` は clang と同じ `[2 x i64]`）で UTF-16 のコード単位をそのまま写す。対になっていないサロゲートも、そのまま `string` のコード単位になる。
- 取り出しの送信は境界ではない。そこで例外が起きれば abort する。`-name`・`-reason`・`-length` は例外を投げず、`-getCharacters:range:` は範囲を `-length` に合わせるので `NSRangeException` にならない。
- 引数の順序（D-59）: どちらも 1 引数で、例外は要らない。`ObjC` は std のファイルを持たない組み込みのモジュールで、`tests/std_argument_order.rs` の対象の外（M01 が記録する）。
- `ObjC.Exception` は std の `Exception`（クラス `Err`）のインスタンスではなく、`try ... with` の `is OverflowException` の節には現れない。`Display`・`Eq` のインスタンスもない。

### 4. 効果・権限・監査

- 境界の呼び出しの効果と権限は、`Result` なしの同じ宣言と同じ（効果 `extern`、権限 `ffi`。`@trusted` がなければ `W1010`）。
- `ObjC.exception_name`・`ObjC.exception_reason` は効果 `extern` と `alloc`（文字列を作る）、権限 `ffi`。組み込みの関数で利用者の宣言を持たないので `W1010` は出さない。`ObjC.Exception` の drop は M01 の規則（効果 `extern`）。
- `tsuzuri audit` の `extern` の行は変えない（`tz-audit/1` のキーを変えない）。

### 5. ブロッキングの包みと fast のコード生成器

- tzroutine を起動するネイティブのプログラムでは、境界の呼び出しも M02 の送信と同じく `tsuzuri_tz_blocking_enter`・`_exit` で包む。enter は `invoke` の直前、exit は正常の後続のブロックの先頭と、landingpad のブロックの `landingpad` の命令の直後に置く（決定事項 D7）。
- `--backend fast` は `personality` を訳さない。境界を含むプログラムの fast のビルドは、今の `E2002`（`fast_backend_unsupported_construct`）になる（`fast-backend` の feature で作ったバイナリ。既定のビルドでは `--backend fast` そのものが `E2000` `fast_backend_unavailable`）。黙って `--backend llvm` に切り替えない（決定事項 D8）。

## 設計

### 検証した下げ方（手で書いた IR）

次の IR と `host.m` を `clang -O0`・`-O3` でそれぞれ object にし、`clang b.o host.o -framework Foundation` でリンクして実行した。どちらも `caught kind=0 e=0x...` と `caught kind=1 e=0x...` を出して exit 2（2 回受けた）になった。`-O3` では `@boundary` が `@main` に取り込まれ、`@main` が `.cfi_personality 155, ___objc_personality_v0` と LSDA を持ち、3 つの `bl _tzhost_throw` は landing pad を持つ呼び出しのまま残った。`nounwind` の関数でも、LLVM は personality を持つ関数に巻き戻しの表を作る。

`host.m`:

```objc
#import <Foundation/Foundation.h>
void tzhost_throw(long kind) {
    if (kind == 0) @throw [NSException exceptionWithName:@"TzProbe" reason:@"from host" userInfo:nil];
    if (kind == 1) @throw @"a string, not an NSException";
}
```

`boundary.ll`（`@printf` の書式の長さは 22）:

```llvm
target triple = "arm64-apple-macosx12.0.0"
@OBJC_EHTYPE_id = external global { ptr, ptr, ptr }
declare void @tzhost_throw(i64)
declare i32 @__objc_personality_v0(...)
declare ptr @objc_begin_catch(ptr)
declare void @objc_end_catch()
declare ptr @objc_retain(ptr)
declare void @objc_release(ptr)
declare i32 @printf(ptr, ...)
@fmt = private unnamed_addr constant [22 x i8] c"caught kind=%ld e=%p\0A\00"

define internal i64 @boundary(i64 %kind) nounwind personality ptr @__objc_personality_v0 {
entry:
  invoke void @tzhost_throw(i64 %kind) to label %ok unwind label %lpad
ok:
  ret i64 0
lpad:
  %lp = landingpad { ptr, i32 } catch ptr @OBJC_EHTYPE_id
  %raw = extractvalue { ptr, i32 } %lp, 0
  %obj = call ptr @objc_begin_catch(ptr %raw)
  %kept = call ptr @objc_retain(ptr %obj)
  call void @objc_end_catch()
  %u = call i32 (ptr, ...) @printf(ptr @fmt, i64 %kind, ptr %kept)
  call void @objc_release(ptr %kept)
  ret i64 1
}

; @main は @boundary を kind 0・1・2 で呼び、結果の和を終了コードにする（2 は投げない）。
```

同じ `-O3` の object を、`throw 42` する C++ の `tzhost_throw`（デストラクターで文字を出す局所変数を持つ）とリンクすると、`libc++abi: terminating due to uncaught exception of type int` で exit 134 になり、デストラクターは走らなかった。C++ の例外は探索の段で止まり、ホストのフレームは巻き戻されない（決定事項 D2）。

同じ IR で次の 2 つも確かめた。

- `@main` から直接呼ぶ代わりに、`nounwind noinline` の内部関数 `@caller`（Tsuzuri の関数と同じ属性）が `@boundary` を 3 回呼ぶ形にした。`-O3` で `@boundary` は `@caller` に取り込まれ、`@caller` が `.cfi_personality 155, ___objc_personality_v0` を持ち、exit 2 だった。呼び出し元の `nounwind` は `invoke` を `call` に変えない。
- 同梱の toolchain の zig 0.16.0 でリンクした（`zig cc -target aarch64-macos.12.0 b.o host.o -F$SDK/System/Library/Frameworks -L$SDK/usr/lib -framework Foundation -lobjc`）。`-O0`・`-O3` の object のどちらも exit 2 だった。これは clang が作った object を zig でリンクしただけである。Tsuzuri の driver が zig でリンクする経路は、M20 の前は `-l objc` が `E2002` になるので確かめていない（S5）。

### 生成（`src/llvm_imports.rs`・`src/llvm_objc.rs`）

- 境界の呼び出しは、引数の下げ方をそのまま使い、呼び出しの命令だけを `invoke` にする。メンバーは M02 の `FunctionEmitter::objc_send` が `call` を出す 1 か所（`ObjcCallSite`。M02 が M05 のために用意する）で、`extern def` は `host_call` の呼び出しの形を、新しい enum の `HostCallKind::Call`（今の形）と `HostCallKind::ObjCBoundary`（`invoke`）に分けて替える（S4）。今の形の IR は変えない（S2）。
- 境界の呼び出しと `ObjC.exception_name`・`ObjC.exception_reason` に到達したら、M01 の `objc_used` を立てる。personality・`OBJC_EHTYPE_id`・`objc_begin_catch` は libobjc にあるので、M01 のモジュールのフラグと `-lobjc` が要る（今は利用者が `-l objc` を渡さないと `E2002` の未解決の symbol になる）。M01 の target の検査（`validate_objc_target`）も、結果が `Result<_, ObjC.Exception>` の `extern def` の呼び出しを「ObjC の型を結果に持つ呼び出し」として数える。
- 境界を含む関数（`extern def` の包みの `tz.fn.<Module>.<name>`、M02 のメンバーの包み）の `define` の行に ` personality ptr @__objc_personality_v0` を足す。`nounwind` は外さない（決定事項 D5）。ほかの関数の属性は変えない。
- 形（名前は決定的に、関数の中の通し番号で付ける）:

```llvm
  call void @tsuzuri_tz_blocking_enter()          ; tzroutine を起動するプログラムだけ
  %r = invoke <R> @objc_msgSend(ptr %self, ptr %sel, <引数>) to label %tz.objc.ok.<n> unwind label %tz.objc.catch.<n>
tz.objc.ok.<n>:
  call void @tsuzuri_tz_blocking_exit()           ; 同上
  ; Result なしの宣言と同じ結果の変換（retain、bool、Maybe、NULL の検査）、copy_outs
  br label %tz.objc.join.<n>
tz.objc.catch.<n>:
  %lp = landingpad { ptr, i32 } catch ptr @OBJC_EHTYPE_id
  call void @tsuzuri_tz_blocking_exit()           ; 同上
  %raw = extractvalue { ptr, i32 } %lp, 0
  %obj = call ptr @objc_begin_catch(ptr %raw)
  %kept = call ptr @objc_retain(ptr %obj)
  call void @objc_end_catch()
  br label %tz.objc.join.<n>
tz.objc.join.<n>:
  ; Ok r と Error %kept の Result を phi で作り、free_copies と release_operands を両方の経路の後に 1 回
```

- +0 の物体の結果の retain は、`Result` なしの呼び出しと同じく M01 の D6 の形にする（`invoke` に `[ "clang.arc.attachedcall"(ptr @llvm.objc.retainAutoreleasedReturnValue) ]` を付ける）。LLVM は `invoke` の bundle も受け付ける（起票時に `invoke ptr @make() [ "clang.arc.attachedcall"(…) ] to label … unwind label …` と `catch ptr @OBJC_EHTYPE_id` の `.ll` を Apple clang 21 の `-O0`・`-O3` で `-c` し、`-O0` の目的ファイルが `bl _make; mov x29, x29; bl _objc_retainAutoreleasedReturnValue` になることを確かめた）。M01 の D6 が `i64` の結果に素の retain を使う場合は、その `call` を `tz.objc.ok.<n>` の中に置く。
- 節は `catch ptr @OBJC_EHTYPE_id` だけで、cleanup の節を持たない。この landing pad は ObjC の物体の例外のときにしか入らないので、clang の選択子の比較（`llvm.eh.typeid.for`）と `resume` は置かない。
- `objc_begin_catch` の結果は `objc_end_catch` までしか生きないので、その間に `objc_retain` する（clang の ARC と同じ順）。間にほかの呼び出しを置かない（入れ子の landing pad を作らない）。
- 宣言: `declare i32 @__objc_personality_v0(...)`、`@OBJC_EHTYPE_id = external global { ptr, ptr, ptr }`、`declare ptr @objc_begin_catch(ptr)`、`declare void @objc_end_catch()`、`declare ptr @objc_retain(ptr)`。D-67 の共通の規則に従い、`declare` と `external global` は symbol ごとに 1 行にし、利用者の `extern` が同じ symbol を宣言していればその行を残して、コンパイラは自分の関数型で `call` する。呼び出し先（`objc_msgSend` など）の `declare` に `nounwind` を付けない（付けると `-O3` で `invoke` が `call` に変わる。S3）。
- `ObjC.exception_name`・`ObjC.exception_reason` は組み込みの関数（`Builtin::ObjCExceptionName`・`ObjCExceptionReason`）として、M02 の selref と送信の下げ方で生成する。`objc_lookUpClass`・`objc_opt_isKindOfClass`・`class_respondsToSelector`・`object_getClass` の宣言も上の共通の規則に従う。
- 組み込みの関数の本体を `emit_typed_builtin`（`src/llvm.rs:8689`）で作るときは、`shared_globals.traps` が `None` なら一部の値だけを写した `local_globals`（8711-8722 行）を使う。戻すのは `recursive_types`・`shared_types`・`anchor_types` の 3 つだけである（9002-9012 行）。本体が selref を登録しても、M01・M02 の selref の表と `objc_used` を `shared_globals` に戻さなければ、selref の大域が出ずに未定義の参照になる。戻す処理を足し、テストで固定する。

### 型検査（`src/check.rs`・`src/abi.rs`）

- `extern def` の結果の検査と M02 のメンバーの結果の検査で、`Result<R, ObjC.Exception>` を見たら `R` を同じ位置の規則で検査し、`FunctionOrigin` か `HostImport` に「ObjC の例外の境界」の印（`objc_boundary: bool`）を持たせる。`abi::c_signature` の C の形は `R` のもの（境界は C の型を変えない）。W14 の同じ symbol の複数の宣言の照合（`CSignature::mismatch`）は `R` の C の形で行う。
- `ObjC.Exception` は M01 の `HandleKind` の ObjC の種類と脇の表に、根 `ObjC.Object`・実行時名なし（実行時のクラスを読まない）・性質 `sync` として組み込みで足す。
- 組み込みの関数を足すときは、`Builtin::ALL`・`name`・`signature`・`emit_builtin`、効果の表（`src/effects.rs` の `builtin_facts`、`src/pure_rules.rs` の `builtin_effect`・`builtin_impure`、`src/kg/effects.rs` の `builtin_effect`）、`src/pinned_rules.rs` の `builtin_position`（関数の引数を取らないので `Position::Escaping`）、権限の表（`src/capabilities.rs`）をすべて更新する。

### 後工程の text の pass

- `with_blocking_externs` の照合は変えない。境界の包みは生成の段で上の形に直接置く。M02 は送信の包みを `with_blocking_externs` の照合に任せる（M02 の D12）ので、`invoke` の行が照合されない（二重に包まない）ことをテストで固定する。
- 生成の段で包むには、2 つの点を足す。
  - 包むかどうかの条件は `src/llvm.rs:1646` の `spawning && !wasm`（1660 行）である。`FunctionEmitter` が読める `Globals.spawns` は `spawns(module)` だけで、`Parallel.*` の `scheduler_parallel` を含まない。`Globals` に同じ条件の値を足す（[M10](M10-objc-autorelease-pools.md) も同じ値を要る。先に入れたほうが足す）。
  - `declare void @tsuzuri_tz_blocking_enter()`・`_exit()` は、今は `with_blocking_externs` が 1 つでも行を包んだとき（`calls`）だけ末尾に足す（1903-1907 行）。生成の段が `intrinsics` に同じ宣言を入れると、ほかの extern の呼び出しがあるプログラムで 2 行になり、`invoke` しかないプログラムで `calls` が偽のままになる。そこで `with_blocking_externs` の末尾の宣言を、入力に同じ `declare` がないときだけ足す形にする。今は生成の段がこの宣言を出さないので、境界を使わないプログラムの出力は変わらない（S2）。
- 利用者が `objc_begin_catch`・`objc_end_catch`・`objc_retain` を `extern` で宣言していると、`extern_symbols` に入り、landing pad の中のコンパイラの呼び出しも行ごとに包まれる。`landingpad` の後なので IR は壊れない。begin と end の間に enter・exit の呼び出しが入るが、どちらも例外を投げないので意味は変わらない。その IR をテストで記録する。
- 関数の見出しに属性か `section` を足す後工程の pass は、今はどれも ` !dbg ` か ` {` の直前に足す。`with_stack_probes`（`src/llvm.rs:1706-1723`。tzroutine を起動するプログラムの `"probe-stack"="inline-asm"`）、`with_generated_section`（`src/llvm_preempt.rs:835-892`。TZ10 の非同期の横取りを使う実行ファイルの `section "__TEXT,__tz_text,regular,pure_instructions"`）、`with_thread_storage_noinline` の `push_line`（`src/llvm_preempt.rs:647-657`。` noinline`）、`with_function_attribute`（`src/llvm_traps.rs:449-485`。`with_frame_pointers`・`with_unwind_tables`）の 4 つである。LLVM は関数の属性と `section` を `personality` より前に要る。`define void @f() nounwind personality ptr @p "probe-stack"="inline-asm" {` と、同じ位置の `section "…"` は、どちらも clang 21 で `error: expected '{' in function body` になった（属性、`section`、`personality` の順なら通る）。そこで、見出しに ` personality ` があればその前に足すように 4 つの pass を直す。personality を持たない見出しでは挿入の位置は変わらない（S2）。macOS で tzroutine を起動するプログラムでは `with_stack_probes` が必ず `"probe-stack"` を足すので、直さないと境界を使う tzroutine のプログラムはどれも LLVM で読めない。
- 横取り（`src/llvm_preempt.rs`）: 検査の行が `landingpad` の前に入らないこと（landing pad のブロックは unwind の辺からしか入れず、ループの頭にならない）と、境界の `invoke` だけを含むループに検査が入るか（417 行は終端の行を重さの判定に含めない）を確かめて記録する。

## 前提とする他チケットのインターフェース

- [M01](M01-objc-class-types.md): 組み込みのモジュール `ObjC`、`ObjC.Object`、`HandleKind` の ObjC の種類と脇の表、`ObjC.upcast`・`ObjC.upcast_owned`、`sync` の性質、drop の `objc_release`、+0 の結果の受け取り方（M01 の D6 の `clang.arc.attachedcall` の bundle）、kind `objc_object_position`・`abi_apple_only`、`Globals` の `objc_used` とモジュールのフラグ、`validate_objc_target`、ObjC を使うビルドへの `-lobjc`。名前が違えば止めて報告する（GUIDE-LITE §11）。
- [M02](M02-objc-typed-message-send.md)（Phase 1）: メンバーの宣言と結果の検査（`abi::objc_result`）、M01 の `selref`、`FunctionEmitter::objc_send` と `call` を出す 1 か所 `ObjcCallSite`（M05 はここで呼び出しの命令だけを `invoke` に替える）、`declare` を 1 つにする規則、init の族の受け手の消費、送信のブロッキングの包み（M02 の D12。`extern_symbols` に `objc_msgSend` を足し、`with_blocking_externs` の照合に任せる）。
- TZ08 の `tsuzuri_tz_blocking_enter`・`_exit`（`src/runtime/tzroutine.c`）。
- PB05 の fast のコード生成器の `E2002` `fast_backend_unsupported_construct`（変えない）。

## 他チケットへ提供するインターフェース

- [M07](M07-apple-sdk-bindgen-and-package.md)（弱い後続）: 生成器は、ヘッダーと文書で例外を投げると分かっているメンバー（SDK の注記には印がないので、生成器の設定の一覧で指定する）を `Result<R, ObjC.Exception>` で写せる。M05 がなければ、そのメンバーも `R` のまま写す。
- [M03](M03-objc-classes-defined-in-tsuzuri.md)・[M04](M04-objc-blocks.md): IMP と Block の invoke の包みは境界ではない。その中で投げられた例外は今のとおり abort する（コールバックの中から例外を投げ返すことは対象外）。その中の境界の呼び出しは受けられる。
- [M10](M10-objc-autorelease-pools.md): 境界で受けた物体は +1 で所有するので、プールの pop で解放されない。逆に、`+exceptionWithName:...` や `-[NSException raise]` で作られた例外の物体は autorelease されてから投げられるので、プールのない tzroutine で受けると、その分が漏れる（「落とし穴」の実験）。tzroutine で境界を繰り返し呼ぶ処理は `ObjC.with_autorelease_pool` で包むと文書に書く（M10 が先に入っていれば `tzroutine` の E2E もそう書く）。ブロッキングの包みの条件として `Globals` に足す値（`spawning && !wasm`）は、M10 の固定の条件と同じもので、先に入れたほうが足す。

## 実装手順

1. **変更**: なし（ブランチ `feat/m05-objc-exception-boundary`）。**内容**: 着手条件の確認、`node scripts/repo-kb.mjs brief M05`、ベースラインの保存（`target/scratch/m05/base/`）。「設計」の手で書いた IR を着手時の clang と SDK でもう一度動かす。**確認**: 保存した件数を控える。手で書いた IR が `-O0`・`-O3` で exit 2、C++ の例外で exit 134。
2. **変更**: `src/check.rs`・`src/abi.rs`・`src/diagnostic_kinds.rs`。**内容**: 仕様 1 の位置の検査、`ObjC.Exception` の型、`objc_boundary` の印。**確認**: `tests/objc_exceptions.rs` の受理と拒否（下の表）。`cargo nextest run --locked`。
3. **変更**: `src/llvm_imports.rs`・`src/llvm_objc.rs`、見出しの 4 つの pass（`src/llvm.rs`・`src/llvm_preempt.rs`・`src/llvm_traps.rs`）。**内容**: `invoke` の形、personality、宣言、`Result` の組み立て、後始末の合流、`personality` の前に足す挿入の位置。**確認**: ベースラインと `cmp`（S2）。`tests/objc_exceptions.rs` の IR の検査（境界の関数だけが personality を持つ、`declare` が重複しない、`emit` を 2 回して同じ）。
4. **変更**: 同上と `src/llvm.rs`（`Globals` の値、`with_blocking_externs` の末尾の宣言の条件）。**内容**: 仕様 5 のブロッキングの包み（「後工程の text の pass」の 2 つの点）。**確認**: tzroutine を起動するプログラム（`tzroutine { }` と `Parallel.map` の両方）の IR で、enter が `invoke` の直前、exit が 2 つの後続の先頭にあり、`invoke` の後に行がない。境界のほかに extern の呼び出しがあるプログラムとないプログラムの両方で `declare void @tsuzuri_tz_blocking_enter()` が 1 行。`node tests/tzroutine.mjs target/debug/tsuzuri` が変わらない。
5. **変更**: `src/llvm_objc.rs`・効果と権限の表。**内容**: 仕様 3・4（`exception_name`・`exception_reason`）。**確認**: `tests/capabilities.rs`・`tests/audit.rs` に境界の case。件数の pin（`src/effects.rs:3839` の `assert_eq!(Builtin::ALL.len(), 375)` などは現在の値 + 2）。`emit_typed_builtin` の `local_globals` から selref の表が戻ること（「生成」）。
6. **変更**: `tests/objc_exceptions_host.m`・`tests/objc_exceptions.mjs`・`tests/fixtures/objc_exceptions/Main.tz`。**内容**: E2E（下）。**確認**: macOS で `node tests/objc_exceptions.mjs target/debug/tsuzuri`。Apple の ld と、M20 があれば同梱の zig（lld）でも（S5）。
7. **変更**: 文書。**内容**: 「ドキュメント」の節。**確認**: T2、`node scripts/check-docs.mjs <変更したページ>`、PR に `ci:full`。

## テスト計画

### Rust（`tests/objc_exceptions.rs`、新規。どのホストでも）

| 検査 | 期待 |
| --- | --- |
| 受理 | `extern def` の `Result<i64, ObjC.Exception>`・`Result<unit, ObjC.Exception>`、メンバーの `Result<Maybe<Self>, ObjC.Exception>` |
| 拒否 | `Result<i64, i32>` の `extern def`（`E1008` `unsupported_extern_signature`、今と同じ文）、入れ子の `Result`、`export def` の `Result<i64, ObjC.Exception>` と `ObjC.Exception` の引数（`E1008` `objc_object_position`） |
| IR（`--emit llvm --target aarch64-apple-darwin`） | personality を持つ `define` は境界の包みだけ。`invoke` の節が `catch ptr @OBJC_EHTYPE_id` だけ。`@__objc_personality_v0`・`@OBJC_EHTYPE_id`・`@objc_begin_catch` の宣言が 1 つずつ。利用者が `extern "objc_end_catch"` を宣言していても `declare` が重複しない。`emit` を 2 回して同じ |
| 変えないこと | ベースラインの全件（ObjC を使うが境界を使わない M01・M02 の fixture を含む）の IR・header・`check --json` が 1 byte も違わない |
| target | `--target wasm32` で境界に到達したビルドが `E1008` `abi_native_only`、`--target x86_64-unknown-linux-gnu` が `E1008` `abi_apple_only`。宣言だけなら成功 |
| ブロッキング | tzroutine を起動するプログラムで、enter・exit の位置（仕様 5）。起動しないプログラムには包みがない |
| 見出しの pass | 境界を使う tzroutine のプログラム（`"probe-stack"` が付く）、`--emit exe` で非同期の横取りの `section` が付く形、`-g` で、境界の関数の見出しが「属性、`section`、`personality`」の順になり、`--emit object` が通る。4 つの pass の単体テスト（`personality` のある見出しとない見出し） |
| fast | `--backend fast` で境界を含むプログラムが `E2002` `fast_backend_unsupported_construct`。`fast-backend` の feature で作ったバイナリだけで検査する（`#![cfg(all(feature = "fast-backend", unix))]` の `tests/fast_backend.rs` に足す。既定のビルドは `E2000` `fast_backend_unavailable`）。`personality` の拒否そのものは `tests/fast_backend.rs:194` が既に固定している |

### E2E（`tests/objc_exceptions.mjs` と `tests/objc_exceptions_host.m`、macOS だけ）

- ホストは `clang -c -fobjc-arc tests/objc_exceptions_host.m` で作り、`--link host.o -l objc --link <SDK>/.../Foundation.tbd`（M20 が入れば `--framework Foundation`）でリンクする。ホストの `TzThrower` は `-boom`（NSException）、`-boomString`（`@throw @"..."`）、`-boomKept`（大域の強い参照 `last` に入れた NSException を投げ、`tz_rc_now()` が `CFGetRetainCount(last)` を返す）、`-boomCxx`（`.mm` の C++ の `throw 42`）、`-value`（投げずに 7 を返す）を持つ。
- どれも `-O0` と `-O3`、`--allocator counting` で `live == 0`。

| case | 内容 | 期待 |
| --- | --- | --- |
| `nsexception` | 早見の例 | `TzProbe: from host`、exit 0 |
| `not_nsexception` | `-boomString` を境界で受ける | name は空、reason は投げた文字列、exit 0 |
| `ownership` | `-boomKept` を受け、`e` をそのまま捨てる・record に入れて返してから捨てる・`Vec` に入れて捨てる | 受けた直後と捨てた後の `tz_rc_now()` の差がどれも 1（Tsuzuri は 1 回だけ release する。ホストの ARC が例外の経路で残す分は差に入らない） |
| `ok_path` | `-value` を境界で呼ぶ | `Ok 7` |
| `cxx_passes` | `-boomCxx` を境界で呼ぶ | exit 134、`libc++abi: terminating due to uncaught exception of type int`、ホストのデストラクターの文が出ない |
| `no_boundary_aborts` | `Result` でない宣言で `-boom` を呼ぶ（呼び出しの後に処理を置き、末尾呼び出しにしない） | exit 134 |
| `callback_aborts` | 「現状」の再現 2（末尾でない形）を境界の宣言のあるプログラムで | exit 134 |
| `tzroutine` | tzroutine の中で境界を 1000 回呼び、毎回受ける（`TSUZURI_MAXPROCS=1` と `4`） | 全部 `Error`、exit 0 |
| `temporaries` | `ref string` の引数（Latin-1 の複写が要る文字列）と `ref mut` の record を渡すメンバーが投げる | `live == 0`、`ref mut` の record は呼び出しの前の値 |
| `compact` | `TSUZURI_STRINGS=compact` で `nsexception` | 同じ出力 |
| `linkers` | `nsexception` を同梱の zig（lld）でリンク（M20 がなければ理由を出して飛ばす） | 同じ出力 |

## ドキュメント

- `_tsuzuri/language-reference/compiler/objective-c.md`（M01 が作る）: 新しい節「例外」。`Result<R, ObjC.Exception>` の書ける位置、受けるもの（ObjC の物体の例外だけ）と受けないもの（C++ の例外）、`ObjC.exception_name`・`ObjC.exception_reason`、境界でない呼び出しとコールバックは abort すること、`-O3` の末尾呼び出しでホストの `@catch` に届くことがあるが頼らないこと、ホストの ARC の漏れ、`ref mut` の書き戻しをしないこと、`--backend fast` の `E2002`。例は `run=` にしない（ホストの `.m` が要る）。
- `_tsuzuri/language-reference/compiler/native-interop.md:626` の文に、ObjC の境界の宣言を除くという注記と objective-c.md へのリンク。
- `_tsuzuri/language-reference/exception-handling/exception-handling.md` と `try-with-finally.md`: `try` はホストの例外を受けないこと、ObjC の例外は境界の宣言で値に変えることを 1 文とリンクで。
- `docs/language.md`: 465 行の文の注記と、「ホスト関数のインポート」の節（449 行から）の小節。「検査付き算術と例外」の節（2982 行から）に、ObjC の境界が `try` と別の仕組みであることを 1 文。
- `docs/architecture.md`: 「ホスト境界」の節（1812 行から）に、境界の関数だけが personality と LSDA を持つこと、`nounwind` を保つ理由、ブロッキングの包みの位置。
- `_tsuzuri/language-reference/compiler/diagnostics.md` の kind の表は `<!-- generated:diagnostic-kinds -->` の生成の部分なので手で書かない（新しい kind はない。M01 の kind を使う）。
- `_features/README.md` の「採用しない改善」の「例外と巻き戻し、`?` 演算子」の行と、`_features/GUIDE.md` の D-10（`_features/GUIDE.md:751-757`）に、出荷した形への改訂（D-67 の「文言の改訂の時期」。D-34 の改訂と同じ書き方で、ObjC の境界の宣言の地点だけで受け、Tsuzuri のフレームは巻き戻さないと書く）。

## 受け入れ条件

- [ ] 仕様 1〜5 のとおりに動き、仕様外の入力は表の診断で拒否される。
- [ ] 境界を使わないプログラムの IR・object・header・`check --json`・既定の `.wasm` がベースラインと 1 byte も違わない。
- [ ] personality を持つ関数が境界の関数（と、それを取り込んだ関数）だけで、生成 IR が決定的で、`declare` と `external global` が重複しない。
- [ ] `tests/objc_exceptions.rs` と `node tests/objc_exceptions.mjs` が `-O0`・`-O3` で通り、`live == 0` と参照の数の差（`ownership`）を確かめた。Apple の ld で通り、M20 があれば同梱の zig でも通る。
- [ ] `-O3` で境界の `invoke` が残ることを IR とアセンブリ（`otool -tv` の LSDA と `bl`）で確かめた。
- [ ] D-55 の表と `docs/stability.md` の扱い（`ObjC` の新しい名前。互換を破らない）を記録した。PR に `ci:full` を付けた。
- [ ] T2 が通る。

## 落とし穴

- **`objc_end_catch` の後の物体**: `objc_begin_catch` が返した物体は `objc_end_catch` で解放されうる。retain してから end する。逆にすると解放後の使用になる。
- **IR の形の規則**: 呼び出し先の `declare` に `nounwind` が付くと LLVM は `invoke` を `call` に変える（S3。利用者の宣言の行を使い回すときも属性を足さない）。`invoke` は終端の命令なので、その後に行を置けない。`landingpad` はブロックの最初の命令（phi を除く）でなければならない。
- **見出しの `personality` の位置**: 属性と `section` は `personality` より前でなければならない。今の 4 つの後工程の pass は ` {` の直前に足すので、そのままでは見出しが壊れる（「後工程の text の pass」）。
- **tzroutine のスタック**: 巻き戻しは同じスタックの中で、投げたフレームから境界の関数までで終わる。境界の関数より下（Tsuzuri の呼び出し元とランタイムの起動の枠）は読まない。
- **ホストの ARC とプール**: clang の ARC は既定で例外の経路の release を省き、ホストの `@autoreleasepool` も例外の経路では pop されない。手で書いた IR の実験では、`[[NSException alloc] init...]` を投げた物体も `+exceptionWithName:...` の物体も、受けて release した後に dealloc されなかった（後者は Tsuzuri の側で push したプールの pop の後に dealloc された）。大域の強い参照を持つ例外では、`CFGetRetainCount` が投げる前 1、受けて retain した後 3、release した後 2 だった。所有のテストは dealloc の数ではなく、受けた後と捨てた後の参照の数の差（1）で確かめる。
- **データシンボルの宣言**: M06 が入ると、利用者が `OBJC_EHTYPE_id` をデータシンボルとして宣言できる。`external global` の型が違う 2 つ目の宣言を LLVM は拒むので、`declare` と同じく symbol ごとに 1 行の規則を当てる。
- **inline**: `-O3` では境界の関数が呼び出し元に取り込まれ、呼び出し元が personality を持つ（「設計」の実験）。「personality を持つのは境界の関数だけ」の検査は `-O0` の IR と、最適化の前の IR で行う。
- **codegen units（PB07）**: `-O0` の分割（`src/llvm_split.rs`）で、personality の参照と `@OBJC_EHTYPE_id` の `external global` が各 unit に宣言されるか、分割が `Unsupported` で黙って戻るかを確かめて記録する。
- **`-O3` の末尾呼び出し**: 境界でない呼び出しが末尾呼び出しになると、Tsuzuri のフレームが消えて外側のホストの `@catch` に届く。abort を固定するテストは、呼び出しの後に処理を置いた形にする（「現状」の再現 2）。

## 対象外

- Tsuzuri のフレームの巻き戻し（D-10）と、関数の境界を越えて例外を運ぶこと。
- C++ の例外の捕捉（今のとおり terminate）。`@finally`。
- コールバック・IMP（M03）・Block（M04）の中から例外を投げ返すこと、`ObjC.throw` に当たる操作。
- Windows の SEH と Linux の巻き戻し、Intel Mac（D-67 の分岐 3 は方針を改めないと決め、M18 は着手しない（2026-11-26）。後で M18 が x86_64 の macOS を扱うなら、同じ形を足すかはそこで決める）。
- `ObjC.Exception` の `userInfo`・`callStackSymbols` の取り出し（要るなら M02 のメンバーとして NSException を宣言して送る）。
- 例外を投げうるメンバーの一覧の作成（M07 の設定）。
- `--backend fast` の `invoke`・landingpad の対応。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | 例外を受けるのは、利用者が `Result<_, ObjC.Exception>` と書いた呼び出しの地点だけ。巻き戻すのは投げたホストのフレームから境界の関数までだけで、Tsuzuri のフレームは巻き戻さない。ほかの経路は今のまま abort する。これを D-10 の狭い解釈として docs/language.md と README の「採用しない改善」の行に記録する | D-34 の `@checked` と同じ「同じ関数の中で受ける」形で、`Drop` を飛ばす経路を作らない。境界は宣言の型に現れ、監査で見える。退けた案: (a) M05 を閉じ、投げうる呼び出しを ObjC のシムに置く（分岐 2 を認めない場合の案で、選ばれなかった。C のツールチェーンが要る）。(b) すべての呼び出しで受けてトラップにする（すべての関数に表が要り IR が変わる。S1・S2）。(c) `try ... with` を ObjC の例外に広げる（どの呼び出しが投げうるかが宣言に現れない） | 承認済み（2026-11-26。D-67 の分岐 2） |
| D2 | 受けるのは ObjC の物体の例外だけ（`catch ptr @OBJC_EHTYPE_id`）。C++ の例外は探索の段で terminate する | `@catch (id e)` と同じ範囲で、C++ の例外の物体は Tsuzuri の値で表せない。退けた案: `catch ptr null`（ホストのフレームを巻き戻してデストラクターを走らせてから terminate することになり、今の振る舞いを変える） | 既定案（実装者はこの案に従う） |
| D3 | `ObjC.Exception` は所有の型で drop で release。`name`・`reason` は取り出すときに内部の送信で写す。NSException でなければ name は空、reason は `-description`（応えなければ空）。判定は `objc_lookUpClass` と `objc_opt_isKindOfClass`、文字列は `-length` と `-getCharacters:range:` | 受けるときの費用を retain だけにし、物体を `ObjC.upcast` で使えるように残す。CoreFoundation の静的なリンクを要しない。退けた案: `-UTF8String`（寿命がプールに結び付き、プールのない tzroutine で漏れる）、受けた時点で文字列に写す（毎回確保し、物体を失う） | 既定案（実装者はこの案に従う） |
| D4 | `ObjC.Exception` は M01 の `sync` | 投げた後の物体は読むだけで、retain と release は atomic。退けた案: M01 の既定の `local`（`Result<_, ObjC.Exception>` を渡す関数がすべて `pinned` になり、tzroutine で値を返せない） | 既定案（実装者はこの案に従う） |
| D5 | 境界の関数に personality を付け、`nounwind` は保つ。呼び出し先に `nounwind` を付けない。begin → retain → end を続けて置き、選択子の比較と `resume` は置かない。見出しに属性と `section` を足す後工程の pass は、`personality` の前に足す | 手で書いた IR が `-O0`・`-O3` で受け、C++ の例外は探索の段で止まった（「設計」）。退けた案: すべての関数に `uwtable`（S1）、境界の関数の `nounwind` を外す（属性の推論が呼び出し元に広がりうる） | 既定案（実装者はこの案に従う） |
| D6 | 例外の経路でも一時の複写の解放と operand の後始末をし、`ref mut` の書き戻しはしない。init の族の受け手は消費済みとして release しない | `live == 0` を保ち、完了していない呼び出しの値を書かない。clang の ARC の既定と同じく二重解放より漏れを選ぶ。退けた案: 例外の経路でも `ref mut` を書き戻す（途中までしか書かれていない値を Tsuzuri の場所に写す）、init の族の受け手を release する（ホストが既に解放していれば二重解放） | 既定案（実装者はこの案に従う） |
| D7 | ブロッキングの包みは生成の段で置く（enter は `invoke` の直前、exit は 2 つの後続の先頭）。条件の値を `Globals` に足し、`with_blocking_externs` の末尾の宣言は入力に同じ `declare` がないときだけ足す | `invoke` の後に行を置けず、`with_blocking_externs` は `invoke` を照合しない。退けた案: `with_blocking_externs` に `invoke` の照合を足し、後続のブロックの先頭を text で探して exit を置く（ブロックの名前と `landingpad` の位置を text で追うことになり壊れやすい） | 既定案（実装者はこの案に従う） |
| D8 | `--backend fast` は今の `E2002` `fast_backend_unsupported_construct` のまま | 明示的に頼まれた使えない経路は誤りを報告する（AGENTS.md の Development policy）。退けた案: 境界を含むプログラムだけ黙って `--backend llvm` に切り替える（頼まれた backend を黙って替える）、fast に `invoke` と landingpad の翻訳を足す（PB05 の範囲を広げる。対象外） | 既定案（実装者はこの案に従う） |
| D9 | 名前は `ObjC.Exception`・`ObjC.exception_name`・`ObjC.exception_reason`（D-55） | std の `Exception` と区別でき、取り出しは 1 引数（D-59）。退けた案: std の `Err` のインスタンスにする（D-34 の `try` の例外と混ざる） | 要承認（D-55。承認前は手順 2 に着手しない） |
| D10 | 今の abort を文書に書き、末尾でない形をテストで固定する。`-O3` の末尾呼び出しでホストの `@catch` に届くことは「保証しない」と書く | 「現状」の再現 2 のとおり結果が最適化の水準で変わる。退けた案: extern の呼び出しの末尾呼び出しを禁じる（性能が落ち、境界を使わないプログラムの IR も変わる。S2）、末尾呼び出しで届くことを保証する（最適化の水準で変わる振る舞いを言語の約束にできない） | 既定案（実装者はこの案に従う） |
