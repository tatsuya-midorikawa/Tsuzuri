# M23: Objective-C の物体の出力引数（`NSError **`）と弱い参照（`ObjC.Weak<C>`）

| 項目 | 内容 |
| --- | --- |
| ID | M23 |
| 優先度 | P2（Cocoa の失敗の報告の大半は `NSError **` で返り、M01・M02 だけでは型付きで受け取れない。今も `ref mut i64` と手で書く retain で受け取れ、循環も手で切れるので P1 にしない） |
| 規模 | L（Phase 1・2。人間が求めない限り Phase 1 だけを実装する） |
| 依存 | [M01](M01-objc-class-types.md), [M02](M02-objc-typed-message-send.md) |
| 後続 | –（弱い後続: [M07](M07-apple-sdk-bindgen-and-package.md)。今は `NSError **` を取るメンバーを `W2002` で省いている） |
| 状態 | todo |
| 起票 | 2026-11-26（D-67。利用者の依頼「「どのチケットにもない壁」と「調査中に見つけたコンパイラの不具合（自分で再現済み）」のそれぞれについて @_features/ に開発チケットまたは修正チケットを作成して」から起票。調査で報告した「壁 1「Objective-C との型付きの連携がない」のうち ARC の一部（物体の出力引数 `NSError **` と弱い参照 `__weak`）。M01 の対象外に理由なしで置かれ、M07 は `NSError **` を取るメンバーを `W2002` で省くので、行き先のチケットがなかった。起票の後の網羅性の検査で見つけた」に当たる。HEAD `804b7970` で確認） |
| 承認 | [GUIDE D-67](GUIDE.md#d-67-tsuzuri-だけで-macos-のデスクトップアプリを書くm01m22) の分岐 1（X′）は承認済み（2026-11-26）。要承認: D-55（Phase 1 の出力引数の型の位置の受理と `E1008` の kind `objc_inout_object`（D4・D7）、Phase 2 の `ObjC.Weak` と、その作成・読み出し・複写の関数の名前 `ObjC.weak`・`ObjC.load_weak`・`ObjC.copy_weak`）。決定事項のうち、出力引数の型の綴り（D1）・所有の規則（D2）・弱い参照の表現（D8）と名前（D9）は要承認。ほかは既定案 |
| 改善する劣位 | Objective-C（clang の ARC）・Swift 比: `NSError **` の API を型付きで呼べず、`ref mut i64` と手で書く retain に頼る（忘れると解放後の使用。「現状」3）。`__weak` に当たるものがなく、W13 の `ref mut` で `objc_initWeak` を呼ぶと libobjc が誤用を報告して落ちる（「現状」5） |
| 手本にする既存実装 | `src/llvm_imports.rs` の `host_call` の `CopyOut`（87-99 行）と `finish_copy_outs`（380-413 行。W13）、`src/abi.rs` の `extern_parameter`（601 行）・`has_invalid_extern_maybe`（625 行）、`src/llvm_abi.rs` の `nullable_to_c`（1032 行）。箱は `src/llvm_anchor.rs:61` の `@tz.alloc`、不透明な std の record は `std/Anchor.tz:3` と [M04](M04-objc-blocks.md) の D10。出力引数の前例は [W25](W25-extern-interface-com.md) の仕様 4。独立した参照は clang の `-fobjc-arc -S -emit-llvm`（GUIDE-LITE §2 規則 10） |
| 主な影響ファイル | `src/abi.rs`, `src/check.rs`, `src/llvm_imports.rs`, `src/llvm_objc.rs`（M01 が作る）, `src/llvm.rs`, `src/polymorph.rs`, `src/effects.rs`, `src/pure_rules.rs`, `src/kg/effects.rs`, `src/capabilities.rs`, `src/pinned_rules.rs`, `src/diagnostic_kinds.rs`, `src/explain/codes/E1008.md`, `tests/objc_out_params.rs`（新規）, `tests/objc_out_params.mjs`（新規）, `tests/objc_out_params_host.m`（新規）, `tests/fixtures/objc_out_params/Main.tz`（新規）, Phase 2 の `tests/objc_weak.rs`・`tests/objc_weak.mjs`・`tests/objc_weak_host.m`・`tests/fixtures/objc_weak/Main.tz`（新規）, `.github/workflows/targets.yml`, `docs/language.md`, `docs/architecture.md`, `_tsuzuri/language-reference/compiler/objective-c.md`（M01 が作る）, `_tsuzuri/language-reference/compiler/native-interop.md`, `_tsuzuri/language-reference/built-in-types-and-modules/argument-order.md`, `_features/README.md` |

## 実装者向け早見

- 範囲: Phase 1 は `extern def`（M01）とメンバー（M02）の引数に `ref mut Maybe<C>`（`C` は ObjC の物体の型）を許し、clang の ARC の pass-by-writeback と同じに下げる。呼ぶ前に場所の値を一時の領域へ写し（retain しない）、戻った後に `objc_storeStrong(場所, 一時の領域の値)` を呼ぶ。Phase 2 は弱い参照 `ObjC.Weak<C>` と `ObjC.weak`・`ObjC.load_weak`・`ObjC.copy_weak` を足し、弱い参照の場所をアドレスの変わらないヒープの箱に置く。
- **人間が求めない限り Phase 1 だけを実装する。** 承認が記録されるまで、該当する Phase に着手しない。
- 最初に読む: ① 「現状」2（W13 の下げ方）・6（clang の参照）・7（計画の形を手で書いた IR の結果）② 仕様 2 の所有の表 ③ M01 の仕様 3 と D6 ④ M05 の仕様 2（例外の経路では書き戻さない）。
- 最初に書くテスト: `tests/objc_out_params.rs` の `accepts_object_out_parameters` と `lowers_like_arc_writeback`（`--emit llvm --target aarch64-apple-darwin`）→ `tests/objc_out_params.mjs` の `fail_then_drain`。
- 触ってはいけない: W13 の `ref mut R`・`ref mut T`・`ref mut H`（`copy`）の意味・IR・診断。M01 の所有の規則。出力引数と `ObjC.Weak` を使わないプログラムの IR・object・header・診断・既定の `.wasm`（1 byte も変えない）。
- 完了の見分け方（Phase 1）: 次のプログラムを arm64 の Mac で `--link <SDK>/System/Library/Frameworks/Foundation.framework/Foundation.tbd`（[M20](M20-macos-framework-link-inputs.md) があれば `--framework Foundation`）で作ると、`-O0`・`-O3` で `true false 260 4` を出して 0 で終わる。`NSZombieEnabled=YES` でも同じで、`deallocated instance` を出さない（260 は `NSFileReadNoSuchFileError`、4 は `NSFileNoSuchFileError`）。

```tsuzuri
@trusted
extern class NSObject : ObjC.Object

@trusted
extern class NSError : NSObject {
    code :: unit -> i64
}

@trusted
extern class NSArray : NSObject

@trusted
extern class NSString : NSObject {
    "writeToFile:atomically:encoding:error:" write_to_file :: ref NSString -> bool -> i64u -> ref mut Maybe<NSError> -> bool
}

@trusted
extern class NSFileManager : NSObject {
    class "defaultManager" default_manager :: unit -> Self
    "contentsOfDirectoryAtPath:error:" contents_of_directory :: ref NSString -> ref mut Maybe<NSError> -> Maybe<NSArray>
}

@trusted
extern "objc_autoreleasePoolPush" def pool_push :: unit -> i64

@trusted
extern "objc_autoreleasePoolPop" def pool_pop :: i64 -> unit

def code :: Maybe<NSError> -> i64 = \error ->
    match error with
    | Maybe.Some e -> NSError.code (ref e)
    | Maybe.None -> 0

def main :: unit -> i32 = \() ->
    let mut listing: Maybe<NSError> = None
    let mut writing: Maybe<NSError> = None
    let pool = pool_push ()
    let manager = NSFileManager.default_manager ()
    let dir: NSString = ObjC.literal "/nonexistent/tz-m23"
    let file: NSString = ObjC.literal "/nonexistent/tz-m23/a.txt"
    let items = NSFileManager.contents_of_directory (ref manager) (ref dir) (ref mut listing)
    let written = NSString.write_to_file (ref dir) (ref file) true 4 (ref mut writing)
    pool_pop pool
    // Foundation autoreleased both errors; the places keep their own references after the pool.
    do! IO.writeln $"{Maybe.is_none (ref items)} {written} {code listing} {code writing}"
    0
```

## 目的

- 調査（D-67 の経緯）の壁 1 は「Objective-C との型付きの連携がない」である。M01・M02 がクラスの型・所有・送信を足すが、ARC が扱うもののうち物体の出力引数（`NSError **`）と弱い参照（`__weak`）に行き先がなかった（「現状」1）。
- 出力引数は Cocoa の失敗の報告の標準の形で、Foundation と AppKit のヘッダーのメソッドの引数だけで 209 か所ある（「現状」8）。M07 がこれを省くと、ファイル・JSON・文書の API の多くが生成されない。
- 弱い参照は、delegate・target・親への参照で循環を切る道具である。NSTimer は invalidate まで target を強く持ち（`NSTimer.h:88`）、AppKit の `delegate` は弱い参照の property である（`NSWindow.h:338`）。
- 安全性の根拠は M01・M02 と同じく「宣言が正しいこと」で、`@trusted`（E15）と権限 `ffi`（D-57）の監査の対象にする。生ポインター型・ポインター演算・`unsafe` は入れない（D-63・D-67 の X′）。Phase 1 は新しい構文を足さず、既存の `ref mut` の位置に物体の `Maybe` を許す。

## 着手条件と停止条件

### 着手条件

- **人間が求めない限り Phase 1 だけを実装する**（GUIDE §0）。承認が記録されるまで、該当する Phase に着手しない（`grep -n '利用者が決めた点' _features/GUIDE.md`）。
- Phase 1: D1・D2 の承認と D-55 の登録（D-67 の分岐 1 は X′ で承認済み、2026-11-26）。[M01](M01-objc-class-types.md) と [M02](M02-objc-typed-message-send.md) の Phase 1 が done（`grep -nE "^\| M0[12] " _features/README.md`）。`extern def` の形は M01 だけで足りるが、テストと完了の例がメンバーを使う。
- Phase 2: D8・D9 の承認と D-55 の登録（D-67 の分岐 1 は X′ で承認済み）、M01 が done。Phase 1 が done でなくてもよい。
- 任意: [M05](M05-objc-exception-boundary.md) があれば例外の経路のテストを足す。[M10](M10-objc-autorelease-pools.md) があれば E2E のプールを `ObjC.with_autorelease_pool` で書く（なければ `extern` の `objc_autoreleasePoolPush`・`Pop`）。[M03](M03-objc-classes-defined-in-tsuzuri.md) のプロトコルの型と [M09](M09-main-thread-dispatch-and-handles.md) の Phase 2 の `main` のクラスも、入っていれば `C` に含める。[M04](M04-objc-blocks.md) が先なら、型引数を持つ std の record の仕組み（M04 の D10）を Phase 2 が使う。
- 環境: arm64 の macOS と Apple clang と SDK。Rust のテストはどのホストでも走る。
- ベースライン: `tests/fixtures/*`・`examples/*` の `--emit llvm`（native・`--target aarch64-apple-darwin`・`--target wasm32`）・`--emit header`・`check --json` を `target/scratch/m23/base/` に保存する。`node scripts/repo-kb.mjs brief M23`。

### 停止条件

- **S1** `Maybe<C>` の場所の表現が NS01 の NULL の niche（`ptr` 1 つ）でない場合があり、`objc_storeStrong` に場所のアドレスを渡せない（M01 の仕様 2 の前提が崩れた）。
- **S2** `CopyOut` に物体の腕を足すだけでは表せず、`extern def` と M02 の送信で別々の書き戻しを持ちたくなった（M02 の S2 と同じ）。
- **S3** 新しい形を使わないプログラムの IR・header・`check --json`・`.wasm` がベースラインと 1 byte でも違う。
- **S4** 生成した並びが clang の参照（「現状」6）と意味で食い違い、どちらに合わせるか決まらない。
- **S5** E2E のホストの dealloc の数と仕様 2 の表が合わない、または `NSZombieEnabled=YES` で `deallocated instance` が出る。
- **S6** Phase 2 で `ObjC.Weak<C>` の型引数を M04 の D10 の形で運べず、`Type` に variant を足したくなった（M01 の S1・M04 の S5 と同じ）。
- **S7** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `804b7970` で確認）

macOS 27.2.1 arm64、Apple clang 21.0.0、SDK 27.2、HEAD の `target/debug/tsuzuri`（`$T`）。再現は `target/scratch/m23/<名前>/Main.tz` で行い、消した（GUIDE §0 の 6）。`SDK=$(xcrun --show-sdk-path)`。

### 1. チケットの文と行き先

- M01 の仕様 3（`_features/M01-objc-class-types.md:366`）は、物体の型を `ref mut` に置くと `E1008` `objc_object_position` にする。対象外（同 513 行）は起票の時点で弱い参照と `NSError **` を理由なしで外していた（M23 の起票に合わせて M23 を指す文に直された）。
- M07 の仕様 7（`_features/M07-apple-sdk-bindgen-and-package.md:339`）は `NSError **` を `pointer to an object pointer` で `// skipped` にして `W2002` を出し、D13（574 行）の理由は起票の時点で「M01・M02 に出力の物体の形が無い」だった（M23 の起票に合わせて「M23 の Phase 1 の前は」の文に直された）。200 行は `-fobjc-arc` の AST の綴り `NSError *__autoreleasing _Nullable * _Nullable` を記録している。
- `__weak` を扱うチケットはない（`rg -n '__weak|objc_initWeak|ObjC\.Weak' _features` はこのファイルのほかに M01 の対象外だけ）。[M17](M17-macos-min-version-and-weak-import.md) の「弱い参照」は symbol の弱い import で、別のものである。

### 2. W13 の `ref mut` の下げ方

- `host_call`（`src/llvm_imports.rs:65-451`）は `ref mut` の record とスカラーを一時の領域へ写して渡し、戻った直後に書き戻す。表は `enum CopyOut { Record, Scalar }`（87-99 行）、スカラーは関数の入口の `alloca`（230-258 行。`copy` のハンドルは 237 行の `ptr`）。`finish_copy_outs`（380-413 行）は、ハンドルが NULL に書き換えられていればトラップする（397-400 行）。
- 順序: 結果のある呼び出しは `call` → 書き戻し（427 行）→ W14 の正準形が `i64` なら `inttoptr`（428-432 行）→ `free_copies`（433 行）→ `Maybe` の変換か NULL の検査（434-448 行）。
- 型の検査: `ref mut` を受けるのは `mutable_extern_scalar`（`src/abi.rs:567-569`。平たいスカラーと `HandleKind::Copy`）と双方向の record（`extern_parameter`、601-618 行）。`ref`・`ref mut` の先が `Maybe` の引数は `has_invalid_extern_maybe`（625-647 行。`ref_to_maybe` は 630 行）が先に拒む（`src/check.rs:9557-9566`）。wasm では `uses_w13_native_only`（`src/abi.rs:582-599`）がどの `ref mut` でも真で `abi_native_only`。
- 「現状」3 の `send_error` の IR は `alloca i64, align 8` への写し込み、`ptrtoint`（W14 の正準形が `i64`）、`call`、書き戻しの順。

### 3. 今の書き方: `NSError **` を `ref mut i64` で受ける

W14 の同一形の規則（M02 の「現状」2）があるので、`objc_msgSend` は 4 引数の上位形にそろえる。`e1/Main.tz`:

```tsuzuri
@trusted
@count(1, none)
extern "objc_getClass" def objc_get_class :: ref utf8string -> i64

@trusted
@count(1, none)
extern "sel_registerName" def sel :: ref utf8string -> i64

@trusted
extern "objc_msgSend" def send :: i64 -> i64 -> i64 -> i64 -> i64

@trusted
@count(3, none)
extern "objc_msgSend" def send_cstr :: i64 -> i64 -> ref utf8string -> i64 -> i64

@trusted
extern "objc_msgSend" def send_error :: i64 -> i64 -> i64 -> ref mut i64 -> i64

@trusted
extern "objc_autoreleasePoolPush" def pool_push :: unit -> i64

@trusted
extern "objc_autoreleasePoolPop" def pool_pop :: i64 -> unit

def main :: unit -> i32 = \() ->
    let pool = pool_push ()
    let fm = send (objc_get_class (ref u8"NSFileManager\0")) (sel (ref u8"defaultManager\0")) 0 0
    let path = send_cstr (objc_get_class (ref u8"NSString\0")) (sel (ref u8"stringWithUTF8String:\0")) (ref u8"/nonexistent/tz-m23\0") 0
    let mut err = 0
    let items = send_error fm (sel (ref u8"contentsOfDirectoryAtPath:error:\0")) path (ref mut err)
    let code = send err (sel (ref u8"code\0")) 0 0
    do! IO.writeln $"items={items} code={code} has_error={err != 0}"
    pool_pop pool
    0
```

`$T build e1 -l objc --link $SDK/System/Library/Frameworks/Foundation.framework/Foundation.tbd` は `-O0`・`-O3` とも `items=0 code=260 has_error=true`。`code` を読む行を `pool_pop pool` の後へ動かした `e2` は `-O0` で `exit=139`、`NSZombieEnabled=YES` では `*** -[NSError code]: message sent to deallocated instance 0x...` と `exit=133`。書かれた物体は autorelease 済みで、誰も retain しないとプールの pop で解放される。型はこの誤りを止めない。

### 4. 物体の型を `ref mut` に置く今の診断

M01 はまだないので、ハンドルで確かめた（`extern "probe_write" def write :: i64 -> … -> bool` を `check`）。

| 宣言 | 結果 |
| --- | --- |
| `extern type copy Failure` と `ref mut Maybe<Failure>` | `E1008` `abi_nullable_payload`（`'Maybe' at the C ABI needs an extern type or 'ref' to one (or, as an extern parameter, 'ref' to a scalar record); None passes a null pointer`） |
| `extern type Failure`（所有）と `ref mut Failure` | `E1008` `unsupported_extern_signature`（W13 の仕様 1 が E12 D5 のまま拒む） |
| `extern type copy Failure` と `ref mut Failure` | 通る（W13） |

### 5. 弱い参照を今の言語で書くと

`objc_initWeak(id *location, id val)` は場所のアドレスを libobjc の表に登録する。W13 の `ref mut` は一時の領域のアドレスを渡す。`w1/Main.tz`（先頭に 3 の `objc_get_class`・`sel` の宣言を置く）:

```tsuzuri
@trusted
extern "objc_msgSend" def send0 :: i64 -> i64 -> i64

@trusted
extern "objc_release" def objc_release :: i64 -> unit

@trusted
extern "objc_initWeak" def init_weak :: ref mut i64 -> i64 -> i64

@trusted
extern "objc_loadWeakRetained" def load_weak_retained :: ref mut i64 -> i64

@trusted
extern "objc_destroyWeak" def destroy_weak :: ref mut i64 -> unit

def main :: unit -> i32 = \() ->
    let obj = send0 (objc_get_class (ref u8"NSObject\0")) (sel (ref u8"new\0"))
    let mut weak = 0
    let _ = init_weak (ref mut weak) obj
    let alive = load_weak_retained (ref mut weak)
    do! IO.writeln $"alive={alive == obj}"
    objc_release alive
    objc_release obj
    let after = load_weak_retained (ref mut weak)
    do! IO.writeln $"after_is_nil={after == 0}"
    destroy_weak (ref mut weak)
    0
```

`$T build w1 -l objc`（`-O0`・`-O3` とも）:

```text
alive=true
objc[98841]: __weak variable at 0x16bc82428 holds 0x10417c984 instead of 0x104955350. This is probably incorrect use of objc_storeWeak() and objc_loadWeak(). Break on objc_weak_error to debug.
objc[98841]: Weak reference loaded from 0x16bc82438 contains 0x104955350 which is not in the weak references table
exit=139
```

解放のときに libobjc は登録した（もう死んだ）スタックの場所を消そうとし、`weak` には古いアドレスが残る。弱い参照の場所はアドレスが変わってはならない。Tsuzuri の move はビット単位の複写なので、値の中に場所を置いても同じことが起きる（D8）。

### 6. clang の参照の IR

次の `ref.m` を `xcrun clang -target arm64-apple-macosx12.0 -fobjc-arc -fno-objc-msgsend-selector-stubs -S -emit-llvm` で下げた（出力引数は `-O0`、弱い参照は `-O0` と `-O1`）。

```objc
#import <Foundation/Foundation.h>
@interface W : NSObject
- (BOOL)write:(NSError **)error;
- (BOOL)writeOut:(out NSError **)error;
@end
@implementation W
- (BOOL)write:(NSError **)error { if (error) *error = [NSError errorWithDomain:@"d" code:1 userInfo:nil]; return NO; }
- (BOOL)writeOut:(out NSError **)error { return YES; }
@end
BOOL c_plain(NSError **error);
BOOL c_strong(NSError * __strong *error);
BOOL call_prev(W *w, NSError *prev) { NSError *err = prev; BOOL r = [w write:&err]; return r && err; }
BOOL call_prev_out(W *w, NSError *prev) { NSError *err = prev; BOOL r = [w writeOut:&err]; return r && err; }
BOOL call_plain(NSError *prev) { NSError *err = prev; BOOL r = c_plain(&err); return r && err; }
BOOL call_strong(void) { NSError *err = nil; BOOL r = c_strong(&err); return r && err; }
BOOL call_null(W *w) { return [w write:NULL]; }
id sink(id);
void weak_demo(id obj, id other) { __weak id w = obj; id s = w; sink(s); __weak id w2 = w; w = other; sink(w2); }
```

`-O0` の要点:

```llvm
; call_prev: %5 is err (__strong), %7 the __autoreleasing temporary
  %11 = load ptr, ptr %5, align 8
  store ptr %11, ptr %7, align 8                 ; copy-in, no retain
  %13 = call zeroext i1 @objc_msgSend(ptr noundef %10, ptr noundef %12, ptr noundef %7)
  %14 = load ptr, ptr %7, align 8
  call void @llvm.objc.storeStrong(ptr %5, ptr %14)
; call_prev_out: an 'out' parameter starts from nil
  store ptr null, ptr %7, align 8
; call_strong: a '__strong *' parameter gets the address of err itself, with no writeback
  %3 = call zeroext i1 @c_strong(ptr noundef %1)
```

- ARC の `NSError **` の引数は `NSError * __autoreleasing *` で、`__strong` の変数を渡すとメソッドでも C の関数でも一時の領域を使う。`out` の印のない引数は今の値を写し込み、`out` の引数は nil で始める。戻った後は `storeStrong`（新しい値を retain して書き、前の値を release）。`[w write:NULL]` は `ptr noundef null` で一時の領域はない。
- 呼ばれる側の `-[W write:]` は物体を `llvm.objc.autorelease` してから `*error` へ `store` する（書かれる物体は +0 でプールが持つ）。
- 弱い参照: `__weak id w = obj;` は `llvm.objc.initWeak`、`id s = w;` は `llvm.objc.loadWeakRetained`（`-O0`。`-O1` では直前の `initWeak` の結果の `retain` に畳まれ、`sink(w2)` の読み出しに残る）、`__weak id w2 = w;` は `llvm.objc.copyWeak`、`w = other;` は `llvm.objc.storeWeak`、スコープの終わりは `llvm.objc.destroyWeak`。`-O1 -c` の目的ファイルの未定義の symbol は `_objc_initWeak`・`_objc_loadWeakRetained`・`_objc_copyWeak`・`_objc_storeWeak`・`_objc_destroyWeak`。`llvm.objc.moveWeak` は ObjC++ の move の構築（`Holder(std::move(a))`）にだけ現れた。ARC の `__weak` の変数はスタックか物体の中にあり、動かない。

### 7. 計画の下げ方を手で書いた IR で確かめた

ホスト（`clang -fobjc-arc -O2 -c host.m`）は `init` で生きている数を増やし `dealloc` で減らす `TzErr` と、`BOOL tz_fail(TzErr **out)`（`*out = [TzErr new]`）・`tz_succeed`（書かない）・`tz_peek`（`*out != nil` を返す）を持つ。呼ぶ側は仕様 3 の形で書いた。

```llvm
define internal zeroext i1 @out_call(ptr %fn, ptr %place) noinline {
  %tmp = alloca ptr, align 8
  %old = load ptr, ptr %place, align 8
  store ptr %old, ptr %tmp, align 8
  %r = call zeroext i1 %fn(ptr %tmp)
  %new = load ptr, ptr %tmp, align 8
  call void @objc_storeStrong(ptr %place, ptr %new)
  ret i1 %r
}
```

| 確かめたこと（Apple clang 21 の `-O0`・`-O3`） | 結果 |
| --- | --- |
| `tz_fail` → `tz_succeed` → `tz_peek` → `tz_fail` → プールの pop | 結果は `0 1 1 0`。`tz_succeed` の後の場所は同じ物体、`tz_peek` は写し込んだ値を読めた。pop の後に生きている物体は 1（2 回目の書き戻しが最初の物体を release した）、場所を release すると 0。`objc_retain`・`store`・`objc_release` の 3 つで書いた版も同じ |
| Foundation の `contentsOfDirectoryAtPath:error:` と `writeToFile:atomically:encoding:error:` に存在しないパス | `list_nil=1 code=260 write=0 code2=4`（`-code` はプールの pop の後に読んだ）。`NSZombieEnabled=YES` でも同じで、解放後の使用の報告はない |
| `malloc(8)` の箱に `objc_initWeak`、2 つ目の箱に `objc_copyWeak`、物体の解放の前と後に `objc_loadWeakRetained`、`objc_destroyWeak` と `free` | 解放の前は 2 つとも同じ物体、後は 2 つとも nil、最後に生きている物体は 0 |

### 8. libobjc・SDK・実在の API

- `/usr/lib/libobjc.A.dylib` は `_objc_storeStrong`・`_objc_initWeak`・`_objc_loadWeakRetained`・`_objc_loadWeak`・`_objc_destroyWeak`・`_objc_copyWeak`・`_objc_moveWeak`・`_objc_storeWeak`・`_objc_initWeakOrNil` を export する（`dyld_info -exports`）。SDK の `usr/include/objc` が宣言するのは `objc_loadWeak`（`runtime.h:1607`。retain して autorelease した物体を返す）と `objc_storeWeak`（1620 行）だけで、ほかは clang の ARC の ABI（M01 の「現状」7 と同じ）。`-allowsWeakReference` は `objc/NSObject.h:97` で `UNAVAILABLE_ATTRIBUTE`。
- `-allowsWeakReference` が `NO` のホストのクラスに `objc_initWeak` を呼ぶと `objc[...]: Cannot form weak reference to instance (0x...) of class TzNoWeak. It is possible that this object was over-released, or is in the process of deallocation.` で abort する（exit 134）。`objc_initWeakOrNil` は abort せず nil を入れた。Apple の古い文書（リポジトリの外）が弱い参照を作れないとした NSFont・NSColorSpace・NSParagraphStyle・NSTextView・NSWindow・NSViewController・NSImage・NSFontManager は、macOS 27.2.1 ではどれも作れた。
- タグ付きポインターの物体（`[[NSNumber alloc] initWithLongLong:7]`）と定数の文字列（`@"abc"`）は、release の後も弱い参照から読めた（解放されない）。
- Foundation と AppKit のヘッダーのメソッドの引数（`grep -rhoE`）: `NSError` のポインターのポインターは 209 か所（`(NSError **)` 188、`out` 付き 20、`(NSError * _Nullable *)` 1）。`__strong` を明示した出力引数は `NSNetServices.h:230`（`out __strong NSInputStream * _Nullable * _Nullable`）と非推奨の `NSPropertyList.h:76`・`84`。`NSError` でない例は `NSURL.h:315` の `getResourceValue:(out id _Nullable * _Nonnull)value forKey:error:`。`NSURL.h:1477` の `checkResourceIsReachableAndReturnError:` は `NS_SWIFT_NOTHROW`（`NSObjCRuntime.h:488` の `swift_error(none)`）で、Swift も `throws` に写さない。
- 実在の API: `NSString.h:914`（`writeToFile:atomically:encoding:error:`、`BOOL`）、`NSJSONSerialization.h:80`（`JSONObjectWithData:options:error:`。不正な JSON で `code=3840` を確かめた）、`NSFileManager.h:313`（`contentsOfDirectoryAtPath:error:`）。`NSTimer.h:38`（run loop は invalidate までタイマーを強く持つ）。
- 生成コードのヒープは `@tz.alloc(i64)`・`@tz.free(ptr)`（`src/llvm_anchor.rs:61`、`src/llvm_imports.rs:459`）で、`--allocator counting` が数える。std の record はモジュールの外で不透明（`Anchor { token: 12345 }` は `E1022` `the representation of 'Anchor' is opaque; use its module API`）。

## 変えてはいけない意味

- W13 の `ref mut R`・`ref mut T`・`ref mut H`（`copy`）の意味・IR・診断・header。`ref mut` の一般の意味（copy-in/copy-out。D1 は物体の `Maybe` に広げるだけ）。物体でない `ref mut Maybe<T>` の今の `abi_nullable_payload`。
- M01 の所有の規則と D6、M02 の族の規則、M05 の例外の経路（書き戻さない）。評価順序（左から右）と drop の順。
- 新しい形を使わないプログラムの IR・object・header・`.wasm`・診断（1 byte も変えない）。D-18、D-59、予約された symbol の規則（コンパイラが呼ぶ libobjc の symbol は予約しない）。

## 仕様

### Phase の範囲

| Phase | 内容 | 規模 | 前提 |
| --- | --- | --- | --- |
| 1 | 引数の `ref mut Maybe<C>`、pass-by-writeback、`ref mut C` の拒否、診断・監査・文書、M07 への写像の情報 | M | M01・M02、X′（承認済み）、D1・D2 |
| 2 | `ObjC.Weak<C>` と 3 つの関数、箱の確保と解放、スレッドの性質、診断・文書 | M | M01、X′（承認済み）、D8・D9 |

### Phase 1: 物体の出力引数

#### 1. 書ける位置と形

- `extern def` と `extern class`・`extern category` のメンバーの引数に `ref mut Maybe<C>` を書ける。`C` は ObjC の物体の型（`extern class` の型、`ObjC.Object`、M03 のプロトコルの型、M05 の `ObjC.Exception`、M09 の `main` のクラス）。C の型は `C * _Nullable __autoreleasing * _Nonnull`。

```tsuzuri
@trusted
extern class NSJSONSerialization : NSObject {
    class "JSONObjectWithData:options:error:" parse :: ref NSData -> i64u -> ref mut Maybe<NSError> -> Maybe<ObjC.Object>
}

@trusted
extern class NSURL : NSObject {
    "getResourceValue:forKey:error:" resource_value :: ref mut Maybe<ObjC.Object> -> ref NSString -> ref mut Maybe<NSError> -> bool
}
```

- `ref mut Maybe<C>` は C の引数 1 つ（`ptr`）で、M02 のセレクターの数の検査でも 1 つ。W14 の分類は `Word64` で、同じ symbol の `ref mut i64` と同じ形（`CSignature::mismatch` は変えない）。
- 実引数は排他借用の場所（`ref mut x`・`ref mut x.field`）で、W13 の仕様 2 と同じく同じ呼び出しで同じ場所を 2 回借りると `E1014`。
- 書けない形（仕様 5）: `ref mut C`（`Maybe` でない物体。呼ばれた側は書かないことも nil を書くこともある。`__strong` の inout の綴りは足さない。D4）、`ref mut Maybe<ref C>` と物体でない `ref mut Maybe<T>`（今のまま）、`export def`・静的コールバックと `FnPtr`・`@c_layout` の field・`extern view`・M03 の IMP の引数・M04 の Block の型の中の `ref mut Maybe<C>` と物体のバッファ（呼ばれる側の向き。対象外）。`NULL` を渡す形は足さない（D6）。エラーが要らなければ捨てる場所を渡す。

#### 2. 呼び出しの意味（所有の規則）

ARC の pass-by-writeback（`out` の印のない引数。「現状」6）と同じにする（D2）。

| 時点 | すること | 所有 |
| --- | --- | --- |
| 実引数の評価（その位置で、左から右） | 場所の値（`Some p` なら `p`、`None` なら nil）を一時の領域へ写す | retain しない。場所は呼び出しの間も参照を持ち、排他借用で誰も drop できない |
| 呼び出しの間 | 呼ばれた側は一時の領域を読み書きしてよい。書く物体は Cocoa の規約で autorelease 済み（+0） | 一時の領域は所有しない |
| 戻った直後（W13 の書き戻しの順） | `objc_storeStrong(場所, 一時の領域の値)`。同じ値なら何もしない | 場所は新しい値を +1 で持ち、前の値は release される。nil なら `None` |
| M05 の例外の経路 | 書き戻さない（M05 の仕様 2 がそのまま効く） | 場所は前の値のまま。投げる前に書かれた物体はプールが持つ |

- 呼ばれた側が書かなければ場所は前の値のまま（W13 の in/out と同じ）。`out` の印のある引数（Foundation と AppKit の `NSError` の出力引数のうち 20 か所）では ARC は nil で始めるので、ARC と違うのは「呼ぶ前が `Some` で、呼ばれた側が書かなかった」ときだけ。宣言は `out` を表さない。ARC と同じにしたければ呼ぶ前に `None` を入れる。
- 書かれた物体は retain するので、プールの pop の後も場所に残る（「現状」7）。呼ばれた側の autorelease の分はプールに残るので、プールのない tzroutine では解放されずに溜まる（M10 の再現 1）。
- `Maybe<C>` は NS01 の niche（M01 の仕様 2）なので、場所のアドレスを `objc_storeStrong` にそのまま渡す（S1）。
- 順序: `call`（+0 の結果は M01 の D6 の bundle が `call` の一部として retain する）→ 書き戻し（宣言の順。W13 の record・スカラーと同じ表）→ `free_copies` → 結果の `Maybe` の変換か NULL の検査。M01 の D6 が W14 の正準形が `i64` の結果に使う素の `objc_retainAutoreleasedReturnValue`（と、その前の `inttoptr`）も、物体の出力引数を持つ呼び出しでは書き戻しの前に置く（落とし穴）。物体の出力引数のない呼び出しの並びは M01 のまま変えない（S3）。

#### 3. 下げ方

```llvm
; NSFileManager.contents_of_directory (ref manager) (ref dir) (ref mut listing)
  %v20 = alloca ptr, align 8                     ; at the function entry, like W13's slots
  ...
  %old = load ptr, ptr %listing                  ; nullable_to_c: the niche is the pointer itself
  store ptr %old, ptr %v20
  %r = call ptr @objc_msgSend(ptr %recv, ptr %sel, ptr %dir, ptr %v20) [ "clang.arc.attachedcall"(ptr @llvm.objc.retainAutoreleasedReturnValue) ]
  %new = load ptr, ptr %v20
  call void @objc_storeStrong(ptr %listing, ptr %new)
  ; then Maybe<NSArray> from %r (M01)
declare void @objc_storeStrong(ptr, ptr)
```

- 書き戻しは NULL を検査しない（W13 の `copy` のハンドルの 397-400 行のトラップとは違う）。`objc_retain`・`objc_release` の 2 つにしない（D3）。
- `declare void @objc_storeStrong(ptr, ptr)` は使ったモジュールにだけ 1 行（D-67 の共通の規則。利用者の `extern "objc_storeStrong"` があればその行を残し、コンパイラは自分の関数型で `call` する）。`with_blocking_externs` で包まない（M01 の retain・release と同じ）ので、書き戻しは `tsuzuri_tz_blocking_exit` の後に来る。
- W14 の正準形が `i64` の宣言では一時の領域のアドレスを今のとおり `ptrtoint` で渡す。出力引数を持つ呼び出しは M01 の `objc_used` を立てる（フラグと `-lobjc`）。

#### 4. target・効果・監査

- `check` は受理する。`build` で到達した呼び出しは、wasm32・wasm64 で `E1008` `abi_native_only`（W13 の口と M01 の検査のどちらでも同じ kind）、Apple でない native で `E1008` `abi_apple_only`（M01 の `validate_objc_target` が「ObjC の型を引数に持つ呼び出し」として数える）。
- 効果と権限は呼び出しのまま（`extern`・`ffi`。書き戻しの retain と release も同じ）。`local` のクラスの `C` は M01 の規則で `pinned`。`@trusted` のない宣言は `W1010`。監査は M01 のクラスの行の `used` が出力引数の `C` も数える（`tz-audit/1` は変えない）。header は M01 の物体の型の規則に従う。

#### 5. 診断

新しいコードは割り当てない（D-51・E12 D12）。

| 状況 | コード | kind |
| --- | --- | --- |
| `extern def`・メンバーの引数の `ref mut C` | `E1008` | `objc_inout_object`（新。文案 `an Objective-C object out-parameter is 'ref mut Maybe<C>': the callee may leave it nil or write nil, and an in-out '__strong' parameter is not supported`） |
| 仕様 1 にない位置の `ref mut Maybe<C>`、物体のバッファ | `E1008` | `objc_object_position`（M01） |
| `ref mut Maybe<ref C>`、物体でない `ref mut Maybe<T>` | `E1008` | `abi_nullable_payload`（既存） |
| 同じ場所を 2 回借りる | `E1014` | 既存 |
| wasm・Apple でない native で到達 | `E1008` | `abi_native_only`・`abi_apple_only`（M01） |

### Phase 2: 弱い参照（`ObjC.Weak<C>`）

#### 6. 型

- `ObjC.Weak<C>`（`C` は仕様 1 と同じ物体の型）は、物体への弱い参照を 1 つ所有する値。物体を生かさず、物体が解放されると読んでも `None` になる。
- 値は、libobjc に登録した場所を入れたヒープの箱（8 バイト、`@tz.alloc`）を所有する。move は箱のアドレスの複写で、箱は動かない（D8）。drop で `objc_destroyWeak(箱)` と `@tz.free(箱)`。
- Copy でも Clone でもない（複製は `ObjC.copy_weak`）。record・union・タプル・`Maybe`・`Vec`・配列・`Anchor<T>`（W23）に入れてよい。`Eq`・`Display`・`Hash` はない。
- スレッドの性質は `C` と同じ（D10）: 既定（`local`）なら Send・Sync でなく触る関数は `pinned`、`sync` なら Send・Sync、M09 の `main` も同じ規則。drop で外部の関数を呼ぶので、再利用できる関数値には捕捉できない（`E1005`）。
- 表現は M04 の D10 と同じく、型引数を持つ std の record に名前で特別な扱いを付ける。中身は箱のアドレス 1 つ（`Anchor<'a>` の `token: i64` と同じく `i64`。D-63・D-67）で、外では不透明（`E1022`）。利用者は箱のアドレスを作れも読めもしない。
- ABI の位置（`extern def`・メンバー・`export def`・コールバック・`FnPtr`・`@c_layout`・`extern view`）に置くと `E1008` `objc_object_position`。

#### 7. 関数

| 名前 | 型 | 意味 |
| --- | --- | --- |
| `ObjC.weak` | `ref C -> ObjC.Weak<C>` | 箱を確保し `objc_initWeak(箱, 物体)` |
| `ObjC.load_weak` | `ref ObjC.Weak<C> -> Maybe<C>` | `objc_loadWeakRetained(箱)`。生きていれば retain 済みの `Some`、解放済みか解放中なら `None` |
| `ObjC.copy_weak` | `ref ObjC.Weak<C> -> ObjC.Weak<C>` | 新しい箱に `objc_copyWeak(新しい箱, 箱)` |

- `C` は引数の型か期待型から決まる（基底の型の弱い参照は `let w: ObjC.Weak<NSResponder> = ObjC.weak (ObjC.upcast (ref window))`。`upcast` の `B` は期待型の `NSResponder` から決まる）。決まらなければ `E1015` `ambiguous_polymorphic_use`、物体の型でなければ M01 の `E1005` `objc_target_not_class`。関数値は M01 の `E1008` `objc_builtin_not_called`。1 引数なので D-59 の例外は要らない。
- 弱い参照を許さないクラスでは libobjc が「現状」8 の文で abort する（`__weak` と同じ。D11）。タグ付きポインターと定数の文字列（`ObjC.literal`）は解放されないので `None` にならない。
- 例（NSTimer は invalidate まで target を強く持ち、run loop はタイマーを強く持つ。target の状態がタイマーを強く持つと invalidate まで循環が残る）:

```tsuzuri
@trusted
extern class NSTimer : NSObject {
    invalidate :: unit -> unit
}

/// The run loop owns the scheduled timer; the controller only watches it.
record Controller { timer: ObjC.Weak<NSTimer> }

def stop :: ref Controller -> unit = \controller ->
    match ObjC.load_weak (ref controller.timer) with
    | Maybe.Some timer -> NSTimer.invalidate (ref timer)
    | Maybe.None -> ()
```

  delegate も同じ考え方で使う。AppKit の `delegate` は弱い property なので、delegate の物体（M03 の export のクラス）は誰かが強く持たなければならない。その持ち主が window の側にある（window が強く持つ controller など）とき、delegate の状態から window へ戻る参照を強くすると循環になるので、`ObjC.Weak<NSWindow>` にする。

#### 8. 下げ方と費用

```llvm
  %box = call ptr @tz.alloc(i64 8)                        ; ObjC.weak (ref obj)
  %0 = call ptr @objc_initWeak(ptr %box, ptr %obj)
  %s = call ptr @objc_loadWeakRetained(ptr %box)          ; ObjC.load_weak: +1 or null
  %box2 = call ptr @tz.alloc(i64 8)                       ; ObjC.copy_weak
  call void @objc_copyWeak(ptr %box2, ptr %box)
  call void @objc_destroyWeak(ptr %box)                   ; drop
  call void @tz.free(ptr %box)
```

- 費用: `ObjC.weak`・`copy_weak` は確保 1 回（8 バイト）と libobjc の呼び出し 1 回（弱い参照の表への登録。表の lock を取る）。`load_weak` は呼び出し 1 回（lock と retain）、drop は呼び出し 1 回と解放 1 回。速さの主張はしない。release のビルドで同じ仕事量の clang の `__weak` と比べ、生成コードを見て objective-c.md に書く（閾値は置かない）。
- `objc_loadWeak`（autorelease。D12）・`objc_storeWeak`（D13）・`objc_moveWeak`（箱は動かない。D8）は使わない。`objc_initWeak` の戻り値は使わない（clang と同じ）。`declare` は使った symbol だけ 1 行、`with_blocking_externs` で包まない、到達したら `objc_used` を立てる（Phase 1 と同じ規則）。

#### 9. target・効果・監査・診断

- `check` は受理し、`build` で到達した 3 つの関数と drop は wasm で `abi_native_only`、Apple でない native で `abi_apple_only`。型を書くだけなら出力は変わらない。
- 効果: `weak`・`copy_weak` は `extern` と `alloc`、`load_weak` と drop は `extern`。権限 `ffi`。組み込みの関数なので `W1010` は出さない（M05 の `exception_name` と同じ）。監査の `extern` の行は増やさず、M01 のクラスの行の `used` に数える。

| 状況 | コード | kind |
| --- | --- | --- |
| `ObjC.Weak<T>`・`ObjC.weak` の `T` が物体の型でない | `E1005` | `objc_target_not_class`（M01） |
| ABI の位置の `ObjC.Weak<C>` | `E1008` | `objc_object_position`（M01） |
| 3 つの関数の関数値 | `E1008` | `objc_builtin_not_called`（M01） |
| 捕捉、`local` の `C` の弱い参照を tzroutine へ | `E1005`・`E1013` | 既存（W19） |
| 表現への直接の触れ方 | `E1022` | 既存 |

## 設計

### Phase 1

- **型の検査**（`src/abi.rs`・`src/check.rs`）: `abi::objc_out_parameter(ty, types)`（`Reference(inner, true)` で `inner` が `std::Maybe<C>`、`C` が M01 の物体の型）を足す。`has_invalid_extern_maybe` の `ref_to_maybe` はこの形を除き、`extern_parameter` はこの形を受ける。M01 の位置の検査（`objc_object_position`）は `extern def` とメンバーの引数のこの形だけを通し、`ref mut C` を `objc_inout_object` にする。M02 の `abi::objc_parameter` はこの形を `ptr` 1 つにする。kind は `src/diagnostic_kinds.rs` に登録する。
- **生成**（`src/llvm_imports.rs`）: `CopyOut` に `Object { slot, place }` を足す。`ref mut` の分岐（216-259 行）で先にこの形を見て、`nullable_to_c` で一時の領域へ写す。`finish_copy_outs` の腕は `load ptr` と `objc_storeStrong`、`intrinsics` に `declare`。`copy_outs` に `Object` があるときだけ、M01 の `inttoptr` と素の `objc_retainAutoreleasedReturnValue`（`convert_ptr_result`）を `finish_copy_outs`（427 行）より前に置く（ないときの並びは変えない。S3）。M05 の `invoke` は正常の後続のブロックだけが `finish_copy_outs` を呼ぶ（M05 の設計のまま）。
- **target**: M01 の `validate_objc_target` が引数の型を `ref mut` と `Maybe` の中まで見る。W13 の wasm の口は今のまま。
- 構文・`FRONTEND_FORMAT`・formatter は変えない。

### Phase 2

- **型**: M04 の D10 の仕組み（M04 が先なら使い、M23 が先なら作って M04 が使う）で `ObjC` の std の record を置く。性質（`is_copy`・`can_capture` は偽、`needs_drop` は真、`can_send`・`can_sync`・`holds_local_handle` は `C` に委ねる）を、M01 が `HandleKind` の述語に寄せた所と同じ所に足す。型引数の検査は `validate_instance`（`src/polymorph.rs:3684`。`AtomicValue` の腕は 3817 行）。
- **組み込み**: `Builtin::ObjcWeak`・`ObjcLoadWeak`・`ObjcCopyWeak` と、`Builtin::ALL`・`name`・`signature`・`emit_builtin`・`src/effects.rs`・`src/pure_rules.rs`・`src/kg/effects.rs`・`src/capabilities.rs`・`pinned_rules::builtin_position`。`load_weak` の結果は所有値（`Rc.get` と違い region を持たない）。
- **生成**（`src/llvm_objc.rs`）: 仕様 8。drop は record の drop glue（`drop_value`、`src/llvm.rs:6721`）。

## 前提とする他チケットのインターフェース

- [M01](M01-objc-class-types.md): 物体の型の判定（`HandleKind::ObjcLocal`・`ObjcSync` と述語）、`Maybe<C>` の niche、位置の検査と kind `objc_object_position`・`objc_target_not_class`・`objc_builtin_not_called`・`abi_apple_only`、`validate_objc_target`、`objc_used`、`declare` の 1 行の規則、D6、監査の行。名前が違えば止めて報告する（GUIDE-LITE §11）。
- [M02](M02-objc-typed-message-send.md): `abi::objc_parameter`、セレクターの数の `c`、`host_call` の分割、`ObjcCallSite`。
- [W13](_completed/W13-extern-ref-mut-and-mutable-buffers.md): `CopyOut`・`finish_copy_outs`・`uses_w13_native_only`。[W15](_completed/W15-abi-nullable-handles.md): `nullable_to_c`、`abi_nullable_payload`。[W23](_completed/W23-anchor-state.md): `Anchor<T>` に入れる規則。
- 任意: [M04](M04-objc-blocks.md) の D10（Phase 2）、[M05](M05-objc-exception-boundary.md) の例外の経路、[M10](M10-objc-autorelease-pools.md) の `ObjC.with_autorelease_pool`。

## 他チケットへ提供するインターフェース

- [M07](M07-apple-sdk-bindgen-and-package.md)（弱い後続。M07 の D13 が参照する）:
  - `-x objective-c -fobjc-arc` の AST の引数の型が物体のポインターへのポインターで、内側の修飾が `__autoreleasing`（`T **` の引数の既定。綴りは `NSError *__autoreleasing _Nullable * _Nullable`）なら `ref mut Maybe<C>`。`C` は内側のポインターを M07 の仕様 5 の表で写した型（`NSError *` は `NSError`、`id` は `ObjC.Object`、`id<P>` は M03 があれば `P`）。外側の `_Nullable`・`_Nonnull` と `out` の印は写像を変えない。
  - 内側が `__strong`（`NSNetServices.h:230` など）か `__unsafe_unretained` のものは今のとおり `// skipped`・`W2002`（理由の文案 `strong object out-parameter`・`unretained object out-parameter`）。`const` の物体のポインター（`const ObjectType []`）は ARC では `__unsafe_unretained` なので、この行に入る。`CFErrorRef *` は対象外。
  - `Result` への自動の写像はしない（D5）。`swift_error` から `Result` を返す Tsuzuri の包みを生成するかは M07 の後の Phase で決める。
- [M05](M05-objc-exception-boundary.md): 書き戻しは `CopyOut` の表の中にあり、例外の経路では行われない。
- [M03](M03-objc-classes-defined-in-tsuzuri.md)・[M09](M09-main-thread-dispatch-and-handles.md): 状態（`Anchor` の値）に `ObjC.Weak<C>` を置ける。M03 の IMP の引数の `ref mut Maybe<C>`（呼ばれる側が書く向き）は `objc_object_position` のまま。
- [M04](M04-objc-blocks.md): Phase 2 が先なら、型引数を持つ std の record の仕組みを使える。

## 実装手順

### Phase 1

1. **変更**: なし（ブランチ `feat/m23-objc-out-parameters`）。**内容**: brief、着手条件、ベースライン。「現状」6 の `.m` を `target/scratch/m23/reference/` で下げ直す。**確認**: `out` の有無・C の関数・`__strong *` の並びが「現状」6 と同じ（違えば記録。S4）。
2. **変更**: `src/abi.rs`・`src/check.rs`・`src/diagnostic_kinds.rs`。**内容**: 型の検査と `objc_inout_object`。**確認**: `tests/objc_out_params.rs` の受理と拒否、`cargo test --locked --test ffi_extensions` が変わらない。
3. **変更**: `src/llvm_imports.rs`。**内容**: `CopyOut::Object`、copy-in、書き戻し、`declare`、順序。**確認**: IR の検査（期待は「現状」6・7 から手で書く）、ベースラインと `cmp`（S3）、2 回の IR が同じ。
4. **変更**: `src/llvm.rs`（M01 の `validate_objc_target`）。**確認**: wasm32 と `x86_64-unknown-linux-gnu`。
5. **変更**: `tests/objc_out_params_host.m`・`tests/objc_out_params.mjs`・fixture・`.github/workflows/targets.yml`。**確認**: `node tests/objc_out_params.mjs target/debug/tsuzuri` が `-O0`・`-O3` で通る。
6. **変更**: 文書。**確認**: T2、`node scripts/gen-docs.mjs --check`、`node scripts/check-docs.mjs <変更したページ>`。PR に `ci:full`。Apple の ld と同梱の zig（M20 の後）でリンクする。

### Phase 2（人間が求めたとき）

7. **変更**: なし。**内容**: D8・D9 の承認と M04 の D10 の状態を確かめる。
8. **変更**: `src/check.rs`・`src/polymorph.rs`・M04 の D10 の置き場。**内容**: 型・性質・組み込み・診断。**確認**: `tests/objc_weak.rs`、`cargo nextest run --locked`。
9. **変更**: `src/llvm_objc.rs`・`src/llvm.rs`。**内容**: 仕様 8 と drop。**確認**: IR の検査、2 回の IR が同じ。
10. **変更**: `src/effects.rs`・`src/pure_rules.rs`・`src/kg/effects.rs`・`src/capabilities.rs`・`src/pinned_rules.rs`。**確認**: `tests/capabilities.rs`・`tests/audit.rs` の case。
11. **変更**: `tests/objc_weak_host.m`・`tests/objc_weak.mjs`・fixture、文書。**内容**: E2E と費用の計測（release のビルド）。**確認**: `-O0`・`-O3` で通り、計測と生成コードを objective-c.md に書く。T2、`ci:full`。

## テスト計画

### Phase 1: Rust（`tests/objc_out_params.rs`。どのホストでも）

期待は ARC の規則と clang の参照から手で書く（GUIDE-LITE §2 規則 10）。

| 検査 | 期待 |
| --- | --- |
| `accepts_object_out_parameters` | `extern def` とメンバーの `ref mut Maybe<NSError>`・`Maybe<ObjC.Object>`・`Maybe<NSString>`、途中の位置（`getResourceValue:forKey:error:`）、同じ symbol の `ref mut i64` の宣言（W14 が通す）、セレクターの数 |
| 拒否 | `ref mut NSError`（`objc_inout_object`）、`ref mut Maybe<ref NSError>` と `ref mut Maybe<i64>`（`abi_nullable_payload`）、`export def`・静的コールバック・`FnPtr`・`@c_layout`・`extern view` の位置（`objc_object_position`）、同じ場所を 2 回（`E1014`） |
| `lowers_like_arc_writeback`（`--target aarch64-apple-darwin`） | 入口の `alloca ptr, align 8`、引数の位置の `load`・`store`、`call` の引数に一時の領域、直後の `load ptr` と `call void @objc_storeStrong(ptr <場所>, ptr <値>)`、書き戻しに NULL の検査がない、bundle は `call` に付き結果の NULL の検査は `objc_storeStrong` の後、`declare void @objc_storeStrong(ptr, ptr)` が 1 行、2 回の IR が同じ |
| 重複と順序 | 利用者の `extern "objc_storeStrong"` と併用して `declare` が 1 行。2 つの出力引数は宣言の順。正準形が `i64` の結果では素の `objc_retainAutoreleasedReturnValue` が `objc_storeStrong` より前。出力引数がなく `ref mut i64` だけを持つ同じ形の呼び出しは、M01 の並び（書き戻しの後に `inttoptr` と retain）のまま |
| TZ08・M05 | spawn するプログラムで `objc_storeStrong` は `tsuzuri_tz_blocking_exit` の後。M05 があれば landingpad の側に `objc_storeStrong` がない |
| target・不変 | wasm32 は `abi_native_only`、`x86_64-unknown-linux-gnu` は `abi_apple_only`、`check` は 0。新しい形のない fixture の IR・`check --json` がベースラインと同じ |

### Phase 1: E2E（`tests/objc_out_params.mjs`・`tests/objc_out_params_host.m`。macOS、`-O0`・`-O3`）

ホストは `clang -fobjc-arc -c`（CI の Homebrew の clang には `-isysroot $(xcrun --show-sdk-path)`）。`TzErr`（`tzout_live` で数える）と、`tzout_fail`・`tzout_succeed`・`tzout_peek`・`tzout_write_nil`・`tzout_throw_after_write`（M05 の後）を C の関数とメソッドの両方で持つ。期待はホストの数から作り、すべての case を `NSZombieEnabled=YES` でも走らせて標準エラーに `deallocated instance` がないことを確かめる。

| case | 期待 |
| --- | --- |
| `fail_then_drain`（書かれた物体をプールの pop の後に読む） | 読める。pop の後の数は 1、場所の drop の後に 0 |
| `succeed_keeps`・`peek` | 場所は同じ物体で数は変わらない。写し込んだ値を呼ばれた側が読める |
| `replace`・`write_nil` | 前の物体が release される。`write_nil` の後は `None` |
| `foundation`（早見）・`json`（`dataWithBytes:length:` と `JSONObjectWithData:options:error:` に `{oops`） | `true false 260 4`・`None` と 3840 |
| `in_tzroutine`（M10 があれば `ObjC.with_autorelease_pool`、なければ `extern` のプール） | 数が 0 に戻る |
| `exception`（M05 があるときだけ。ないときは理由を出して飛ばす） | `Error`、場所は呼ぶ前の値、数は増えない |

`targets.yml` の darwin-arm64 の段に M01 と同じ形（`if [ "$RUNNER_OS" = macOS ]`）で足す。

### Phase 2

- Rust（`tests/objc_weak.rs`）: record・`Vec`・`Maybe`・`Anchor` の中の受理、仕様 9 の表の各行、`sync` のクラスの弱い参照は tzroutine へ送れる。IR は仕様 8 の形で `objc_moveWeak`・`objc_storeWeak`・`objc_loadWeak` を含まない。target の 2 つ。
- E2E（`tests/objc_weak.mjs`・`tests/objc_weak_host.m`、`-O0`・`-O3`、`--allocator counting`）: `alive_then_dead`、`moves`（record・`Vec`・関数の結果へ動かした後に物体を解放して `None`。標準エラーに `objc[` の行がない）、`copy`、`immortal`（タグ付きの NSNumber と `ObjC.literal`）、`refusing_class`（exit 134 と libobjc の文。文書の挙動を固定する）、`sync_thread`、`timer`（`timerWithTimeInterval:target:selector:userInfo:repeats:` で作ったタイマーを解放して `None`）。どれも生きている数と確保の数が 0 に戻る。

## ドキュメント

- `_tsuzuri/language-reference/compiler/objective-c.md`（M01 が作る）: 「出力引数（`NSError **`）」（書ける形、所有の表、`out` との差、プール、`ref mut C` を拒む理由、`__strong` の出力引数を宣言しないこと、エラーを捨てる書き方）と、Phase 2 の「弱い参照（`ObjC.Weak`）」（関数、スレッドの性質、費用、許さないクラスの abort、解放されない物体、NSTimer と delegate の例、M17 の `@weak` とは別）。例はホストが要るので `run=` にしない。
- `compiler/native-interop.md` の「可変参照（ref mut）と @count」（226-231 行）から objective-c.md への 1 文。`argument-order.md` の `ObjC` の行に Phase 2 の 3 つ。`docs/language.md` の ObjC の小節（M01 が作る）と `docs/architecture.md`（書き戻しの順、`objc_storeStrong`、箱）。kind は登録して `node scripts/gen-docs.mjs`、`src/explain/codes/E1008.md` に `objc_inout_object`。

## 受け入れ条件

- [ ] Phase 1 の仕様 1〜5 のとおりに動き、仕様の外の入力は表の診断で拒否される。
- [ ] 新しい形を使わないプログラムの IR・header・`check --json`・`.wasm` がベースラインと 1 byte も違わない。生成 IR が決定的で、`objc_storeStrong` の `declare` が 1 行。
- [ ] `tests/objc_out_params.rs` と macOS の `-O0`・`-O3` の `node tests/objc_out_params.mjs`（`NSZombieEnabled=YES` を含む）が通り、`tzout_live == 0` を確かめた。
- [ ] Apple の ld で通り、M20 があれば同梱の zig でも通る。PR に `ci:full`。kind と D-55 の登録、言語リファレンスを更新した。T2 が通る。
- [ ] （Phase 2）仕様 6〜9 のとおりに動き、`tests/objc_weak.rs`・`node tests/objc_weak.mjs` が通り、生きている数と確保の数が 0 に戻る。費用を測って書いた。

## 落とし穴

- **写し込みを retain しない**: 一時の領域は所有しない。retain すると 1 つ漏れる。
- **書き戻しを NULL で止めない**: W13 の `copy` のハンドルの腕（397-400 行）を通すと、エラーのない呼び出しがトラップする。
- **素の retain の位置**: 物体の出力引数を持つ呼び出しで、M01 の D6 の素の `objc_retainAutoreleasedReturnValue` を今の並び（427-428 行の後）に置くと、書き戻しの release が先に走る。+0 の結果が前の値にだけ生かされていれば解放後の使用になりうる。並びを変えるのはこの呼び出しだけにする（ほかの呼び出しまで変えると、新しい形を使わないプログラムの IR が変わる。S3）。
- **配列を `ref mut Maybe<C>` で宣言しない**: 長さを別の引数で渡す物体のバッファ（`id *` に `count` 個を書く形）を出力引数として宣言すると、呼ばれた側が 8 バイトの一時の領域の外へ書く。ARC は印のない `id *` の引数も `__autoreleasing` にするので、型からは見分けられない（宣言の誤り。監査の対象）。Foundation と AppKit の配列の引数は `__unsafe_unretained` か `const` で、`const` の物体のポインターも ARC では `__unsafe_unretained` になる（`-ast-dump` で確かめた）ので、M07 の写像には入らない。
- **`__strong` の出力引数**: `T * __strong *` の関数を `ref mut Maybe<C>` で宣言すると、呼ばれた側が写し込んだ値を release し、書き戻しがもう一度 release する。宣言の誤りでコンパイラは見分けられない（監査の対象。M07 は省く）。
- **`out` の印**: 呼ぶ前が `Some` で呼ばれた側が書かなければ、ARC では nil、Tsuzuri では前の値。
- **弱い参照の場所を値に置かない**: 場所のアドレスが変わると libobjc の表が壊れる（「現状」5）。箱のアドレスを外に出さない。
- **2 つの「弱い」**: M17 の `@weak` は symbol の弱い import。文書と診断の文で取り違えない。
- **許さないクラス**: `objc_initWeakOrNil` に替えると誤りが黙って `None` になる（D11）。
- **解放されない物体**: タグ付きポインターと定数の文字列で「解放された」ことを確かめるテストを書かない。

## 対象外

- 言語の一般の出力引数の構文の変更（`ref mut` の意味は W13 のまま）。`__unsafe_unretained`・`__autoreleasing` を利用者が書く形と `__strong` の inout の綴り。
- ARC の最適化（retain と release の対の除去）。Swift の `throws` への自動の写像（D5）。
- Core Foundation の `CFErrorRef *`（toll-free bridging は M01 の範囲の外）。
- 呼ばれる側の出力引数（M03 の IMP・M04 の Block が書く向き）、`NULL` を渡す形（D6）、物体の配列の出力（`__unsafe_unretained` のバッファ）、`ObjC.Class`・`ObjC.Sel` の出力引数。
- `ObjC.store_weak`（D13）、弱い参照の比較と hash。

## 決定事項

| ID | 決定 | 採らない案 | 理由 | 状態 |
| --- | --- | --- | --- | --- |
| D1 | 物体の出力引数の綴りは `ref mut Maybe<C>`（W13 の copy-in/copy-out を所有する物体の `Maybe` に広げる）。新しい構文・型・予約語はない。D-55 には、`extern def` とメンバーの引数の `ref mut Maybe<C>` の受理（既存の型の形の新しい位置）として登録する | 包みの型 `ObjC.Out<C>`、属性 `@out`、結果のタプル `(R * Maybe<C>)`、`ref mut C` | C の `T **` と引数が 1 対 1 で、M07 の写像も 1 対 1。包みと属性は D-55・D-53 の登録が要る。タプルは引数の位置と in/out を失う。`ref mut C` は nil を表せない（D4） | 要承認（承認前は Phase 1 に着手しない） |
| D2 | 所有の規則は ARC の pass-by-writeback（`out` の印のない形）: 写し込みは retain せず、戻った後に `objc_storeStrong`。メソッドと C の関数で同じ。`out` の印は読まない | (B) nil で始め、書かれなければ場所を `None` にする。(C) 呼ぶ前に前の値を release して `None` にする | clang の参照と W13 の in/out の両方と同じ。(B) は物体だけ `ref mut` の意味が変わり、書かれなかった呼び出しで場所の値を黙って捨てる。(C) は呼ばれた側が入力を読めず、M05 の例外の経路で場所が `None` になる。代わりに、`out` の印のある引数（Foundation と AppKit の 20 か所）で呼ぶ前が `Some` で呼ばれた側が書かなかったときだけ、ARC（`None`）と結果が違う（前の値のまま。仕様 2・落とし穴） | 要承認（承認前は Phase 1 に着手しない） |
| D3 | 書き戻しは `objc_storeStrong` の呼び出し 1 つ | `objc_retain`・`store`・`objc_release`。`llvm.objc.storeStrong` の intrinsic | clang の `-O0` と同じ関数で、同じ値なら何もしない。3 つの版も「現状」7 で同じ結果だが呼び出しが 2 つ。intrinsic は M01 の D4 と同じく使わない。性能の主張はしない | 既定案（実装者はこの案に従う） |
| D4 | `ref mut C` は新しい kind `objc_inout_object`（`E1008`）で拒む | `objc_object_position` のまま文だけ変える。受理して nil でトラップする | 正しい綴りと `__strong` の inout を拒む理由を文に書ける。受理すると、書かない・nil を書く Cocoa の呼び出しがトラップする。M01 が先に出荷していれば、この形の kind だけが変わる（コードは同じ）。これは `docs/stability.md` の「1 つの kind を 2 つに分けるのは互換の変更で、元の kind は残りの条件に使い続けます」に当たる | 既定案（実装者はこの案に従う） |
| D5 | `Result` への自動の写像は Phase 1 で足さない | W25 の `[out] J**` と同じく、結果を `Result<R, NSError>` と書けば失敗の印と出力引数をまとめる | COM は「HRESULT と最後の `[out]`」が一様で失敗が型から決まる（W25 の仕様 4）。Cocoa の失敗の印は `BOOL` の NO・nil・`swift_error` の指定と様々で、`NS_SWIFT_NOTHROW` のように `BOOL` 自体が答えのメソッドもある（「現状」8）。写像には新しい宣言の形とメソッドごとの指定が要る。`Result` を返す包みは Tsuzuri で数行で書け、費用も同じ | 既定案（実装者はこの案に従う） |
| D6 | `NULL` を渡す形（`Maybe<ref mut Maybe<C>>`）は足さない | W15 の `Maybe<ref …>` を可変借用に広げる | W15 の `nullable` は共有借用だけ。捨てる場所の費用は一時の領域 1 つと、書かれたときの retain・release だけ | 既定案（実装者はこの案に従う） |
| D7 | 新しい診断コードは割り当てず、kind は `objc_inout_object` だけを足す（D-55 の「診断の kind（既存のコード）」の行に登録する） | 出力引数ごとの kind | D-51・E12 D12。位置の誤りは M01 の kind で足りる | 既定案（実装者はこの案に従う） |
| D8 | 弱い参照の場所は値が所有するヒープの箱（8 バイト、`@tz.alloc`）に置き、値は箱のアドレスを持つ不透明な std の record（M04 の D10 の形）。`objc_moveWeak` は使わない | 値の中に場所を置き move のたびに `objc_moveWeak`。動かせない型を言語に足す。弱い ivar を持つ ObjC の物体を箱にする | move はビット単位の複写で、move の地点に処理を差し込む仕組みがない（場所が動くと「現状」5 のとおり壊れる）。動かせない型は新しい概念。ObjC の物体は実行時のクラスの登録が要り重い。箱は確保 1 回で `__weak` と同じ関数を使う。既定の WASM は変わらない（D-18） | 要承認（承認前は Phase 2 に着手しない） |
| D9 | 名前は `ObjC.Weak<C>`・`ObjC.weak`・`ObjC.load_weak`・`ObjC.copy_weak`（D-55） | `ObjC.make_weak`、Rust に倣う `ObjC.upgrade`、`Clone` のインスタンス | `ObjC.retain`・`ObjC.copy_block`（M04）と同じく動詞の名前で、`load` は libobjc の語。M01 は ObjC の型を Clone にしない | 要承認（承認前は Phase 2 に着手しない） |
| D10 | `ObjC.Weak<C>` のスレッドの性質は `C` と同じ | 常に Send・Sync（libobjc の関数はスレッド安全） | 読むと `C` の強い参照ができ、最後の release の dealloc がそのスレッドで走る（M01 の D1 と同じ理由） | 既定案（実装者はこの案に従う） |
| D11 | 作成は `objc_initWeak`（許さないクラスでは abort） | `objc_initWeakOrNil` で `None` にする。結果を `Maybe` にする | `__weak` と同じ関数で、誤りが黙って消えない。`objc_initWeak` は clang の ARC が出す関数（「現状」6 の未定義の symbol）で、`OrNil` は clang が出さない（どちらも SDK のヘッダーには宣言がない）。macOS 27.2.1 の AppKit の代表のクラスは許した | 既定案（実装者はこの案に従う） |
| D12 | 読み出しは `objc_loadWeakRetained`（+1） | `objc_loadWeak`（autorelease する） | プールに入らず、所有値にそのままなる | 既定案（実装者はこの案に従う） |
| D13 | `objc_storeWeak` の代入の関数は足さない | `ObjC.store_weak :: ref C -> ref mut ObjC.Weak<C> -> unit` | 新しい値を作って前の値を drop すれば書ける。確保 1 回を省く関数は測ってから足す（AGENTS.md） | 既定案（実装者はこの案に従う） |
| D14 | target の誤りは `build` で出す（M01 の D16） | `check` で拒む | 同じプロジェクトを wasm と Linux でも検査できる | 既定案（実装者はこの案に従う） |
