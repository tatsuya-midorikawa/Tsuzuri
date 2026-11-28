# M03: Objective-C のクラスを Tsuzuri で定義する（`extern class export`・オーバーライド・プロトコル・`Super` への送信・`Anchor` の状態・構造体を受け返す IMP）

| 項目 | 内容 |
| --- | --- |
| ID | M03 |
| 優先度 | P2 |
| 規模 | XL（Phase 1・2。人間が求めない限り Phase 1 だけを実装する） |
| 依存 | [M02](_completed/M02-objc-typed-message-send.md), ([W36](_completed/W36-c-layout-by-value.md)), ([M09](_completed/M09-main-thread-dispatch-and-handles.md))（Phase 2 は W36 の Phase 2。`main` のクラスは M09 の Phase 2） |
| 後続 | –（弱い後続: [M04](M04-objc-blocks.md), [M07](M07-apple-sdk-bindgen-and-package.md)） |
| 状態 | todo |
| 起票 | 2026-11-26（D-67。利用者の依頼「「どのチケットにもない壁」と「調査中に見つけたコンパイラの不具合（自分で再現済み）」のそれぞれについて @_features/ に開発チケットまたは修正チケットを作成して」から起票。調査で報告した「壁 1（実行時のクラス定義・delegate・オーバーライドは、class_addMethod に FnPtr の IMP を渡す生アドレスの書き方。構造体を返す IMP は _objc_msgForward と forwardInvocation: の回避策。super は ObjcSuper record と上位形）」に当たる。HEAD `804b7970` で確認） |
| 承認 | [GUIDE D-67](GUIDE.md#d-67-tsuzuri-だけで-macos-のデスクトップアプリを書くm01m22) の分岐 1（X′）と分岐 4（(a)。W36 を Phase 2 まで実装することを人間が求めた）は承認済み（2026-11-26）。要承認: D-55（`extern class` の後の予約語 `export`、`with` によるプロトコルの列、メンバーの先頭の文脈キーワード `override`・`optional`、`extern protocol` の文脈キーワード `protocol`、クラス名の後の `Super`（`Name.Super.m`。D7）、`ObjC.set_state`）。決定事項 D1〜D4 と D11（Phase 2）が要承認。Phase 2 は W36 の停止条件 S1 の選択（未決）も待つ |
| 改善する劣位 | Objective-C・Swift 比: AppKit の delegate・target-action・`NSView` のサブクラス（`drawRect:`・`intrinsicContentSize`・`NSTextInputClient`）を、`i64` の id と `objc_allocateClassPair`・`class_addMethod` に `FnPtr` を渡す手書きの登録でしか書けない。IMP は id も構造体も返せず、構造体を返すメソッドは `_objc_msgForward` と `forwardInvocation:` の回避策が要り、super は `@c_layout` の record と上位形の `objc_msgSendSuper` が要る。型の誤りは型検査で捕まらず、実行時の SIGSEGV になる |
| 手本にする既存実装 | [W26](W26-com-objects-in-tsuzuri.md)（実装の関数の型 `ref I -> i64 -> P… -> R`、状態は `Anchor` の鍵、`Anchor.Error` の扱い。M03 は `I.Methods` の値の表の代わりに、宣言の中で `= impl` を静的に結ぶ）。`src/llvm_abi.rs` の `wrapper`（`WrapperKind::Callback`、`@tz.callback.<name>`、W12 の `@tz.main.done` の検査）と `src/abi.rs` の `callback`。`src/llvm.rs` の `with_blocking_externs`。`src/parser.rs` の `Parser::extern_declaration`。[M02](_completed/M02-objc-typed-message-send.md) のメンバーの構文と送信の下げ方。clang の参照（`-fno-objc-msgsend-selector-stubs` の `objc_msgSendSuper2` と `OBJC_METH_VAR_TYPE_` の型の符号） |
| 主な影響ファイル | `src/syntax.rs`・`src/parser.rs`・`src/syntax_codec.rs`・`src/frontend_cache.rs`（`FRONTEND_FORMAT`）・`src/formatter.rs`・`src/docgen.rs`・`src/api_diff.rs`・`src/semantic.rs`・`src/kg_capture.rs`・`src/find.rs`・`src/repl.rs`・`src/doctest.rs`・`src/guide_std.rs`・`src/stdlib.rs`（`grep -rln extern_types src` の全件）、`src/check.rs`・`src/abi.rs`・`src/objc_encoding.rs`（新規。M04 と共有）・`src/llvm.rs`・`src/llvm_abi.rs`・`src/llvm_imports.rs`・`src/effects.rs`・`src/pure_rules.rs`・`src/kg/effects.rs`・`src/capabilities.rs`・`src/pinned_rules.rs`・`src/diagnostic_kinds.rs`、`tests/objc_export_classes.rs`（新規）・`tests/objc_export_classes.mjs`（新規）・`tests/objc_export_classes_host.m`（新規）・`tests/fixtures/objc_export_classes/Main.tz`（新規）、`_tsuzuri/language-reference/compiler/objective-c.md`（M01 が作る）・`_tsuzuri/language-reference/compiler/native-interop.md`・`_tsuzuri/language-reference/values-and-functions/keywords.md`・`docs/language.md`・`docs/architecture.md` |

## 実装者向け早見

- 範囲: Objective-C のクラスを、`extern class export Name : Base [with P…] { [override] ["sel:"] m :: P… -> R = impl }` と宣言して Tsuzuri で定義する。メソッドの実装はトップレベルの関数 `impl :: ref Name -> i64 -> P… -> R` で、2 つ目の引数はオブジェクトの ivar `tz_state` に置いた `Anchor` の鍵。クラスはプログラムのロード時に constructor が実行時の API で登録する。`Name.Super.m` は `objc_msgSendSuper2` に下がる。`extern protocol` はプロトコルとプロトコルの型（`ref P`）を足す。Phase 2 は構造体を値で受け返す IMP（`drawRect:`・`intrinsicContentSize`・`selectedRange`）。
- **人間が求めない限り Phase 1 だけを実装する**（GUIDE §0）。承認が記録されるまで、該当する Phase に着手しない。
- 最初に読む: ① 仕様 1〜7 ② 「現状」の再現 1〜3（今の手書きの書き方と、その限界）③ `llvm_abi::wrapper` と `with_blocking_externs` ④ M02 のメンバーと送信の下げ方 ⑤ W26 の仕様 3・6。
- 最初に書くテスト: `tests/objc_export_classes.rs` の `export_class_registers_in_a_constructor`（`--emit llvm --target aarch64-apple-darwin` の `@llvm.global_ctors`・`objc_allocateClassPair`・`class_addIvar`・`class_addMethod` の行と、型の符号の文字列）→ `tests/objc_export_classes_host.m` が `NSInvocation` で呼ぶ `drive`。
- 触ってはいけない: 今のコールバック・`FnPtr`・`export def` の包みの ABI と IR（新しい形を使わないプログラムの IR・object・header・診断・既定の `.wasm` は 1 byte も変えない）。W12 の契約（`main` の後の呼び出しはトラップ）。`extern ... export def` の拒否（`ExportOnExternDeclaration`）。
- 完了の見分け方（Phase 1）: 次のプログラムを arm64 の macOS で `tsuzuri build <dir> -o counter`（`-lobjc` は M01 が足す）で作ると、終了コード 42 を返す。`--target wasm32` の build は `E1008`（`abi_native_only`）、`--target x86_64-unknown-linux-gnu` の build は `E1008`（`abi_apple_only`）。`check` はどの target でも通る。

```tsuzuri
@trusted
extern class NSObject : ObjC.Object {
    class alloc :: unit -> Self
    init :: unit -> Maybe<Self>
    "isEqual:" is_equal :: Maybe<ref ObjC.Object> -> bool
}

@trusted
extern protocol Counting {
    "countBy:" count_by :: i64 -> i64
}

@trusted
extern class export TzCounter : NSObject with Counting {
    override "isEqual:" is_equal :: Maybe<ref ObjC.Object> -> bool = on_is_equal
    "countBy:" count_by :: i64 -> i64 = on_count_by
}

def on_is_equal :: ref TzCounter -> i64 -> Maybe<ref ObjC.Object> -> bool = \self _token other ->
    TzCounter.Super.is_equal self other

def on_count_by :: ref TzCounter -> i64 -> i64 -> i64 = \_self token step ->
    match token |> Anchor.update (\(total: i64) -> total + step) with
    | Result.Ok _ -> step
    | Result.Error _ -> -1

def main :: unit -> i32 = \() ->
    let zero: i64 = 0
    let total = Anchor.create zero
    match TzCounter.init (TzCounter.alloc ()) with
    | Maybe.None -> 1
    | Maybe.Some counter ->
        ObjC.set_state (Anchor.token (ref total)) (ref counter)
        let _ = Counting.count_by (ObjC.upcast (ref counter)) 5
        let _ = Counting.count_by (ObjC.upcast (ref counter)) 37
        if TzCounter.is_equal (ref counter) (Maybe.Some (ObjC.upcast (ref counter))) then
            match Anchor.token (ref total) |> Anchor.read (\(n: ref i64) -> deref n) with
            | Result.Ok n -> n as i32
            | Result.Error _ -> 2
        else
            3
```

## 目的

AppKit のアプリは、Cocoa のクラスを継承したクラス（`NSView` のサブクラス、`NSApplicationDelegate`・`NSWindowDelegate` に従う delegate、target-action の受け手）を作らなければ書けない。今の Tsuzuri でも、`objc_allocateClassPair` と `class_addMethod` に `FnPtr` の IMP を渡せばクラスを実行時に作れる（「現状」の再現 1）。しかしこれは id を `i64` にした生アドレスの書き方で、次の問題がある。

- メソッドの型をコンパイラが知らない。型の符号の文字列（`"v@:@"`）も、IMP の引数の並びも手書きで、誤りは実行時の SIGSEGV になる。
- IMP は id を返せず（`i64` とみなせば返せるが型を失う）、構造体を返すメソッドは直接書けない（再現 2）。今は `_objc_msgForward` と Tsuzuri の `forwardInvocation:` で回避するしかない（再現 3）。
- super への送信は、`@c_layout record ObjcSuper` と上位形の `objc_msgSendSuper` の手書きが要る。
- オブジェクトごとの状態の置き場（ivar か associated object）も、鍵の読み出しも手書き。

このチケットは、[M01](_completed/M01-objc-class-types.md)・[M02](_completed/M02-objc-typed-message-send.md) の型付きのクラスの上に、クラスを定義する側を足す。D-67 の X′ の範囲（`extern` の宣言として書き、`@trusted` と権限 `ffi` の監査の対象。生ポインター型と `unsafe` は入れない）を出ない。Tsuzuri の値の型に継承は入れない（README の「採用しない改善」の「クラス継承」を改めない。ホストのクラスの階層を写すだけ）。

## 着手条件と停止条件

### 着手条件

- [M02](_completed/M02-objc-typed-message-send.md) の Phase 1 が done（M01 も含む）。確認: `grep -nE "^\| M0[12] " _features/README.md`。
- D-67 の分岐 1 は X′ で承認済み（2026-11-26）。決定事項の D1〜D3 と D-55 の登録（D4）が承認済みであること。**承認が記録されるまで、該当する Phase に着手しない。**
- **人間が求めない限り Phase 1 だけを実装する。** D-67 の分岐 4 は (a) で承認済み（2026-11-26）。人間が求めたのは W36 を Phase 2 まで実装することで、M03 の Phase 2 そのものではない。Phase 2 は、決定事項 D11 の承認と W36 の停止条件 S1 の選択（未決）が記録され、[W36](_completed/W36-c-layout-by-value.md) の Phase 2（AAPCS64 と呼ばれる側の `by_value_from_abi`・`by_value_to_abi`）が done で、人間が求めたときだけ。分岐 4 の (b)（HFA だけを先に扱い、M02 の Phase 2 の HFA の分類を使う）は選ばれなかった。
- `main` のクラス（`extern class export main`）の段は [M09](_completed/M09-main-thread-dispatch-and-handles.md) の Phase 2 が done のときだけ。なければ `main` の修飾子は足さず、export のクラスは `local` と `sync` だけ。
- ベースライン: `tests/fixtures/*`・`examples/*` の `--emit llvm`（native・`--target aarch64-apple-darwin`・`--target wasm32`）と `check --json`、コールバックを使う fixture（`tests/fixtures/ffi_extensions`）と M01・M02 の fixture の IR を `target/scratch/m03/before/` に保存する。
- macOS（arm64）の機械と Xcode の Command Line Tools（clang・SDK）。node のテストの段は macOS だけで走る。

### 停止条件

- **S1** IMP のために、コールバックの包み（`llvm_abi::wrapper`）を別の関数に複製したくなった（W12 の「包みは 1 種類」に反する）。`WrapperKind` に種類を足して共有する。
- **S2** export のクラスを使わないプログラムの IR・object・header・診断・既定の `.wasm` が 1 byte でも変わる。M01・M02 の fixture の IR が変わる。
- **S3** 登録の constructor の中で Tsuzuri の関数やランタイム（`tsuzuri_*`）を呼ばないと書けない（ランタイムの初期化より前に走るため）。
- **S4** IMP からアプリの状態へ、`Anchor` の鍵の表を通らない経路ができる（W12 の「判定の義務」。オブジェクトは状態を所有しない）。
- **S5** 静的なメタデータ（`class_ro_t`・`__objc_classlist`）を出さないと動かない場面が見つかった（D3 の前提が崩れる）。
- **S6** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `804b7970` で確認）

### 再現 1: 今の手書きのサブクラス（動く）

`objc_allocateClassPair`・`class_addIvar`・`class_addMethod` に静的コールバック（関数型の引数。`FnPtr.of` と同じ `@tz.callback.<name>` の包み）を IMP として渡し、ivar に `Anchor` の鍵を置き、super を `objc_msgSendSuper` で呼ぶ。libobjc だけで動く（`NSObject` は libobjc にある）。

```tsuzuri
// A subclass of NSObject made at run time: an IMP from a static callback, an Anchor token in an ivar,
// and a super call through objc_msgSendSuper with a @c_layout record.
@c_layout
record ObjcSuper { receiver: i64, super_class: i64 }

@trusted
@count(1, none)
extern "objc_getClass" def objc_get_class :: ref utf8string -> i64

@trusted
@count(1, none)
extern "sel_registerName" def sel_register_name :: ref utf8string -> i64

@trusted
extern "objc_msgSend" def send :: i64 -> i64 -> i64

@trusted
@count(2, none)
extern "objc_allocateClassPair" def objc_allocate_class_pair :: i64 -> ref utf8string -> i64 -> i64

@trusted
extern "objc_registerClassPair" def objc_register_class_pair :: i64 -> unit

@trusted
@count(2, none)
@count(5, none)
extern "class_addIvar" def class_add_ivar :: i64 -> ref utf8string -> i64 -> i8u -> ref utf8string -> i8u

@trusted
@count(2, none)
extern "class_getInstanceVariable" def class_get_instance_variable :: i64 -> ref utf8string -> i64

@trusted
extern "object_setIvar" def object_set_ivar :: i64 -> i64 -> i64 -> unit

@trusted
extern "object_getIvar" def object_get_ivar :: i64 -> i64 -> i64

@trusted
extern "object_getClass" def object_get_class :: i64 -> i64

@trusted
@count(4, none)
extern "class_addMethod" def class_add_method :: i64 -> i64 -> (i64 -> i64 -> i64) -> ref utf8string -> i8u

@trusted
extern "objc_msgSendSuper" def send_super :: ref ObjcSuper -> i64 -> i64

def sel :: ref utf8string -> i64 = \name -> sel_register_name name

def token_of :: i64 -> i64 = \self_ ->
    object_get_ivar self_ (class_get_instance_variable (object_get_class self_) (ref u8"tz_state\0"))

// -[TzCounter hash]: counts calls in the Anchor and returns [super hash] + 1.
def on_hash :: i64 -> i64 -> i64 = \self_ cmd ->
    let _ = token_of self_ |> Anchor.update (\(n: i64) -> n + 1)
    let sup = ObjcSuper { receiver: self_, super_class: objc_get_class (ref u8"NSObject\0") }
    send_super (ref sup) cmd + 1

def main :: unit -> i32 = \() ->
    let zero: i64 = 0
    let state = Anchor.create zero
    let cls = objc_allocate_class_pair (objc_get_class (ref u8"NSObject\0")) (ref u8"TzCounter\0") 0
    let _ = class_add_ivar cls (ref u8"tz_state\0") 8 3i8u (ref u8"q\0")
    let _ = class_add_method cls (sel (ref u8"hash\0")) on_hash (ref u8"Q16@0:8\0")
    objc_register_class_pair cls
    let obj = send (send cls (sel (ref u8"alloc\0"))) (sel (ref u8"init\0"))
    object_set_ivar obj (class_get_instance_variable cls (ref u8"tz_state\0")) (Anchor.token (ref state))
    let h1 = send obj (sel (ref u8"hash\0"))
    let h2 = send obj (sel (ref u8"hash\0"))
    let calls = match Anchor.token (ref state) |> Anchor.read (\(n: ref i64) -> deref n) with
        | Result.Ok n -> n
        | Result.Error _ -> -1
    do! IO.writeln $"hash - pointer = {h1 - obj}, same = {h1 == h2}, calls = {calls}"
    let again = objc_allocate_class_pair (objc_get_class (ref u8"NSObject\0")) (ref u8"TzCounter\0") 0
    do! IO.writeln $"second allocateClassPair with the same name = {again}"
    0
```

```text
$ tsuzuri build sub/Main.tz -l objc -o sub/app && ./sub/app
hash - pointer = 1, same = true, calls = 2
second allocateClassPair with the same name = 0
```

終了コード 0。`-O3` でも同じ出力である。`NSObject` の `hash` はポインターの値なので、super の結果 + 1 からポインターを引くと 1 になる。同じ実行時名の 2 回目の `objc_allocateClassPair` は NULL（0）を返す（D10 のトラップの根拠）。`class_addIvar` は型の符号の文字列（5 つ目）にも `@count(5, none)` が要る。付けないと長さの引数が 6 つ目として余分に渡る（`declare i32 @class_addIvar(i64, ptr, i64, i32, ptr, i64)`。arm64 では C の側が無視するので動いてしまう）。手書きの登録はこの種の誤りを型検査で捕まえない。`--emit llvm` の包みは `define internal i64 @tz.callback.Main.on_hash(i64 %arg0, i64 %arg1) nounwind` で、本体は `@tz.fn.Main.on_hash` を呼ぶだけ（`--emit llvm` は `Entry::Library` なので、実行ファイルの包みにある W12 の `@tz.main.done` の検査は出ない。`src/driver.rs:4078-4082`）。

調査の probe E9（作業ディレクトリは残っていない）は、AppKit の delegate に associated object で鍵を付けた。要る断片は次のとおりで、`[NSApp run]` の中の target-action・NSMenu・NSTimer から main のスレッドで呼ばれ、`action clicked: sender tag=7`・`menu item action fired`・`timer tick 1..3`・`main: -run returned, state clicks=102 ticks=3` で終了コード 0 だった。

```tsuzuri
// The Anchor token is stored on the delegate object as an NSNumber (associated object).
def token_of :: i64 -> i64 = \self_ ->
    let boxed = objc_get_associated_object self_ (sel (ref u8"tzAnchorKey\0"))
    send0 boxed (sel (ref u8"longLongValue\0"))
// in main: OBJC_ASSOCIATION_RETAIN_NONATOMIC = 1
let boxed = send1 (cls (ref u8"NSNumber\0")) (sel (ref u8"numberWithLongLong:\0")) tok
objc_set_associated_object delegate (sel (ref u8"tzAnchorKey\0")) boxed 1
```

ここで `send` は、W14 の「1 つの symbol に C の署名は 1 つ」に合わせて、すべての送信を 1 つの署名にそろえた上位形（整数 9 個と f64 4 個）で、空きを 0 で埋める（M02 の「現状」）。

### 再現 2: IMP の型の制限

コールバックの引数はスカラー・ハンドル・`Maybe<H>`、結果はスカラーか unit だけ（`src/abi.rs:661-672` の `callback`、結果の `exportable` は `src/check.rs:1134-1139` で `Integer(8|16|32|64)`・`Binary(32|64)`・`Bool`）。id を返す IMP と構造体を返す IMP は書けない。

```tsuzuri
extern type copy Id

@trusted
@count(4, none)
extern "class_addMethod" def class_add_method :: i64 -> i64 -> (Id -> i64 -> Id) -> ref utf8string -> i8u

def on_description :: Id -> i64 -> Id = \self_ _cmd -> self_

def main :: unit -> i32 = \() ->
    let _ = class_add_method 0 0 on_description (ref u8"@16@0:8\0")
    0
```

```text
idret/Main.tz:5:30: error[E1008]: extern callback parameters support functions over scalar, unit and extern handle types; buffers, records and nested functions are not supported
```

```tsuzuri
@c_layout
record Size { width: f64, height: f64 }

@trusted
@count(4, none)
extern "class_addMethod" def class_add_method :: i64 -> i64 -> FnPtr<(i64 -> i64 -> Size)> -> ref utf8string -> i8u

def on_intrinsic_size :: i64 -> i64 -> Size = \_self _cmd -> Size { width: 12.5, height: 40.0 }

def main :: unit -> i32 = \() ->
    let _ = class_add_method 0 0 (FnPtr.of on_intrinsic_size) (ref u8"{CGSize=dd}16@0:8\0")
    0
```

```text
structret/Main.tz:6:64: error[E1008]: FnPtr requires a C-callable function signature over scalar, unit, and extern handle types; buffers, records, and nested functions are not supported
```

`Maybe<Id>` を返すと、別の文の `E1008`（`'Maybe' at the C ABI needs an extern type or 'ref' to one ...`）になる。構造体の引数は、AAPCS64 の HFA（`NSRect` は d0〜d3）を利用して `f64` 4 個に分ければ受け取れる（probe E8 の `drawRect:` は `i64 -> i64 -> f64 -> f64 -> f64 -> f64 -> unit` で `rect=(0, 0, 160, 90)` を受けた）。M02 のセレクターの数の検査（`objc_selector_arity`）の後は、この形を `extern class` のメンバーには書けない。

### 再現 3: 構造体を返すメソッドの今の回避策（`_objc_msgForward`）

IMP を `_objc_msgForward` にし、Tsuzuri の `forwardInvocation:` が `-[NSInvocation setReturnValue:]` に `ref mut [f64]` を渡す。Foundation のリンクには SDK の `.tbd` へのリンクを使う（`mkdir fw && ln -s $(xcrun --show-sdk-path)/System/Library/Frameworks/Foundation.framework/Foundation.tbd fw/libFoundation.tbd`）。

```tsuzuri
// A struct-returning method (-intrinsicContentSize -> CGSize) written today: the IMP is
// _objc_msgForward and a Tsuzuri -forwardInvocation: fills the result with -setReturnValue:.
@trusted
@count(1, none)
extern "objc_getClass" def objc_get_class :: ref utf8string -> i64

@trusted
@count(1, none)
extern "sel_registerName" def sel_register_name :: ref utf8string -> i64

// W14 keeps one C signature per symbol: every objc_msgSend declaration takes four 64-bit words.
@trusted
extern "objc_msgSend" def send :: i64 -> i64 -> i64 -> i64 -> i64

@trusted
@count(3, none)
extern "objc_msgSend" def send_cstr :: i64 -> i64 -> ref utf8string -> i64 -> i64

@trusted
@count(3, none)
extern "objc_msgSend" def send_f64buf :: i64 -> i64 -> ref mut [f64] -> i64 -> i64

@trusted
@count(2, none)
extern "objc_allocateClassPair" def objc_allocate_class_pair :: i64 -> ref utf8string -> i64 -> i64

@trusted
extern "objc_registerClassPair" def objc_register_class_pair :: i64 -> unit

@trusted
@count(4, none)
extern "class_addMethod" def class_add_method :: i64 -> i64 -> (i64 -> i64 -> i64 -> unit) -> ref utf8string -> i8u

@trusted
@count(4, none)
extern "class_addMethod" def class_add_raw_method :: i64 -> i64 -> i64 -> ref utf8string -> i8u

@trusted
@count(2, none)
extern "dlsym" def dlsym :: i64 -> ref utf8string -> i64

def sel :: ref utf8string -> i64 = \name -> sel_register_name name

def on_forward :: i64 -> i64 -> i64 -> unit = \_self _cmd invocation ->
    if send invocation (sel (ref u8"selector\0")) 0 0 == sel (ref u8"intrinsicContentSize\0") then
        let mut size: [f64] = new [f64](2, \i -> if i == 0 then 12.5 else 40.0)
        let _ = send_f64buf invocation (sel (ref u8"setReturnValue:\0")) (ref mut size) 0
        ()
    else
        ()

def main :: unit -> i32 = \() ->
    let cls = objc_allocate_class_pair (objc_get_class (ref u8"NSObject\0")) (ref u8"TzSized\0") 0
    let forward = dlsym (0 - 2) (ref u8"_objc_msgForward\0")
    let a = class_add_raw_method cls (sel (ref u8"intrinsicContentSize\0")) forward (ref u8"{CGSize=dd}16@0:8\0")
    let b = class_add_method cls (sel (ref u8"forwardInvocation:\0")) on_forward (ref u8"v24@0:8@16\0")
    objc_register_class_pair cls
    let obj = send (send cls (sel (ref u8"alloc\0")) 0 0) (sel (ref u8"init\0")) 0 0
    let key = send_cstr (objc_get_class (ref u8"NSString\0")) (sel (ref u8"stringWithUTF8String:\0")) (ref u8"intrinsicContentSize\0") 0
    let boxed = send obj (sel (ref u8"valueForKey:\0")) key 0
    let mut out: [f64] = new [f64](2, \_ -> -1.0)
    let _ = send_f64buf boxed (sel (ref u8"getValue:size:\0")) (ref mut out) 16
    do! IO.writeln $"added={a}{b} size via valueForKey: = {out[0]} x {out[1]}"
    0
```

```text
$ tsuzuri build fwd/Main.tz -l objc -L fwd/fw -l Foundation -o fwd/app && ./fwd/app
added=11 size via valueForKey: = 12.5 x 40
```

`-O3` でも同じ出力で、終了コード 0。調査の検証では、同じ手で `NSView` のサブクラスの `intrinsicContentSize` を上書きすると、Auto Layout の `fittingSize` が 13×40 を返した（12.5 が切り上がる）。呼ぶたびに `NSInvocation` を組むので、頻度の高いメソッドには向かない。

### 生成とランタイムの事実

- **包み**: `llvm_abi::wrapper`（`src/llvm_abi.rs:793`）は `WrapperKind::Export`・`Callback`（`src/llvm_abi.rs:785-790`）を持ち、コールバックの名前は `tz.callback.<修飾名>`（`src/llvm_abi.rs:953`）。実行ファイル（`Entry::Console`）の包みは入口で `load atomic i32, ptr @tz.main.done acquire` を読み、真なら `@tz.callback.after_main()` でトラップする（`src/llvm_abi.rs:813-823`、`src/llvm.rs:2238-2261`）。spawn するプログラムでは `with_blocking_externs`（`src/llvm.rs:1840` から）が ` @tz.callback.` の `define` を `tsuzuri_tz_callback_enter`・`tsuzuri_tz_callback_scope_enter` で包む。
- **main の擬似 G**: `tsuzuri_tz_callback_enter`（`src/runtime/tzroutine.c:2157-2165`）は、M のないスレッド（main の擬似 G と、ランタイムに参加していないスレッド）では 0 を返して何もしない。ランタイムに参加していないスレッドの呼び出しは、`tsuzuri_tz_callback_scope_enter`（同 2178-2185）がその間だけの境界にする。
- **併用の拒否**: IR に `@tz.callback.` があると、`--trap-mode return` は `E2000`（`option_conflict`。`src/driver.rs:4100-4105`）、`Domain.run` も `E2000`（`src/llvm.rs:1545-1557` の `domain_without_callbacks`）。
- **constructor**: `src/` に `global_ctors` を出す所はない（`grep -rn global_ctors src` で 0 件）。
- **PB07 の分割**: `src/llvm_split.rs:584-596` は `appending` と `extern_weak` のリンケージを `Unsupported` にし、`src/driver/units.rs:281-284` は分割できなければ黙って分割なしに戻る（-O0 だけの経路）。
- **`export` の拒否**: `extern` の後の `export` は `src/parser.rs:4864-4870` で `E1008`（`export_on_extern_declaration`、文 `extern declarations cannot be exported`）。`export extern` の順は `src/parser.rs:969`。`class`・`export`・`with` は予約語（`src/lexer.rs:646`・`655`・`680`）で、edition で切り替える語の表 `EDITION_KEYWORDS` は空（`src/lexer.rs:10`）。
- **W26 と W36**: W26 の Phase 3 の呼ばれる側の値渡しは「どちらも Windows だけ」（W26:187-191）。W36 は `export def` とコールバックを変えず（W36:331 の D6、対象外の W36:318）、呼ばれる側の `by_value_from_abi`・`by_value_to_abi` を提供する（W36:221・238）。W36 の Phase 2 が AAPCS64（macOS）を扱うが、人間が求めない限り Phase 1 だけ（W36:7）。D-67 の分岐 4 で、人間は W36 を Phase 2 まで実装することを求めた（2026-11-26）。
- **Anchor**: 作ったスレッド以外は `WrongThread`（`_tsuzuri/language-reference/built-in-types-and-modules/anchor.md:84`）。再入は `Busy`（W23 の D2）。鍵 0 は `Stale`（同 82 行）。

### clang の参照（clang 21、`-target arm64-apple-macosx12.0 -fobjc-arc -fno-objc-msgsend-selector-stubs -S -emit-llvm`）

```objc
@implementation B
- (NSUInteger)hash { return [super hash] + 1; }
@end
@implementation Del
- (NSSize)intrinsicContentSize { return NSMakeSize(1,2); }
- (NSRange)selectedRange { return NSMakeRange(1,2); }
- (void)drawRect:(NSRect)r { (void)r; }
@end
```

```llvm
%struct._objc_super = type { ptr, ptr }
@"OBJC_CLASSLIST_SUP_REFS_$_" = private global ptr @"OBJC_CLASS_$_B", section "__DATA,__objc_superrefs,regular,no_dead_strip", align 8
  %3 = alloca %struct._objc_super, align 8
  store ptr %0, ptr %3, align 8
  %4 = load ptr, ptr @"OBJC_CLASSLIST_SUP_REFS_$_", align 8
  %5 = getelementptr inbounds nuw i8, ptr %3, i64 8
  store ptr %4, ptr %5, align 8
  %7 = call i64 @objc_msgSendSuper2(ptr noundef nonnull %3, ptr noundef %6)
declare ptr @objc_msgSendSuper2(ptr, ptr, ...) local_unnamed_addr
@OBJC_METH_VAR_TYPE_ = private unnamed_addr constant [8 x i8] c"Q16@0:8\00", section "__TEXT,__objc_methtype,cstring_literals", align 1
define internal %struct.CGSize @"\01-[Del intrinsicContentSize]"(ptr readnone captures(none) %0, ptr readnone captures(none) %1)
define internal [2 x i64] @"\01-[Del selectedRange]"(ptr readnone captures(none) %0, ptr readnone captures(none) %1)
define internal void @"\01-[Del drawRect:]"(ptr readnone captures(none) %0, ptr readnone captures(none) %1, [4 x double] %2)
```

型の符号は `"{CGSize=dd}16@0:8"`・`"{_NSRange=QQ}16@0:8"`・`"v48@0:8{CGRect={CGPoint=dd}{CGSize=dd}}16"`。super は今のクラス（superrefs が指すのは `B` 自身）を渡す `objc_msgSendSuper2`。`objc_msgSendSuper2` は `/usr/lib/libobjc.A.dylib` の re-export（`[re-export] _objc_msgSendSuper2 (from libobjcMsgSend)`。`dyld_info -exports`）で、SDK の `usr/include/objc` に宣言はなく、`usr/lib/libobjc.tbd` にはある。

### 計画の前提として確かめたこと（C、HEAD とは無関係）

constructor の中で、静的に参照した基底（`OBJC_CLASS_$_…`）から実行時にクラスを登録できることを、C で確かめた（D3）。

```c
extern struct objc_class OBJC_CLASS_$_NSPersonNameComponents;
static Class registered;
static ptrdiff_t state_offset;
static long answer(id self, SEL cmd) { return 40 + (long)*(int64_t *)((char *)self + state_offset); }
__attribute__((constructor)) static void reg(void) {
  Class c = objc_allocateClassPair((Class)&OBJC_CLASS_$_NSPersonNameComponents, "TzCtorProbe", 0);
  if (!c) __builtin_trap();
  class_addIvar(c, "tz_state", 8, 3, "q");
  class_addMethod(c, sel_registerName("answer"), (IMP)answer, "q16@0:8");
  objc_registerClassPair(c);
  state_offset = ivar_getOffset(class_getInstanceVariable(c, "tz_state"));
  registered = c;
}
// main: o = [registered new]; *(int64_t *)((char *)o + state_offset) = 2; [o answer];
```

`clang -O1 t.c -lobjc -framework Foundation` の出力は `answer=42 found_by_name=1 super=NSPersonNameComponents offset=16`（`objc_getClass("TzCtorProbe")` でも見つかる）。AppKit をリンクしたプロセスでは、`objc_getProtocol` が `NSApplicationDelegate`・`NSWindowDelegate`・`NSTextInputClient`・`NSMenuDelegate`・`NSTableViewDataSource`・`NSCopying`・`NSObject` のどれにも非 NULL を返した。

## 変えてはいけない意味

- W12 の契約: コールバック（と IMP）は `main` の境界の終わりまでどのスレッドからでも呼べ、その後の呼び出しはトラップする。包みは 1 種類。
- 今のコールバック・`FnPtr`・`export def` の型・ABI・IR、`extern ... export def` の拒否。
- `Anchor` の意味（W23）。オブジェクトは状態を所有しない。
- M01・M02 の所有の規則（`ref C` は +0 の借用、所有する `C` は +1、drop で `objc_release`）。
- コールバックの中のトラップがプロセスを終わらせること（W12 の「変えてはいけない意味」）。

## 仕様

### Phase の範囲

| Phase | 内容 | 規模 | 前提 |
| --- | --- | --- | --- |
| 1 | `extern class export` の宣言・メンバーの `= impl`・`override` とプロトコルとの照合・ロード時の登録・IMP の包み・`ObjC.set_state`・`Name.Super.m`・`extern protocol` とプロトコルの型。`main` のクラスは M09 の Phase 2 があるとき。libobjc だけで動く `NSObject` のサブクラスと、ホストの `.m` で検査する。AppKit の段は headless | L | M01・M02 の Phase 1 |
| 2 | 構造体を値で受け返す IMP（`drawRect:(NSRect)`・`intrinsicContentSize`・`selectedRange`）。W36 の Phase 2 の呼ばれる側の下げ方を AAPCS64 で使う | M | W36 の Phase 2 と S1 の選択・D11・D-67 の分岐 4（(a) で承認済み。(b) の HFA は選ばれなかった） |

### 1. 宣言（Phase 1）

```text
export_class = [ doc ] [ "@trusted" ] [ "private" ] "extern" [ RuntimeName ] "class" "export" [ "main" | "sync" ]
               Name ":" Base [ "with" Proto { "," Proto } ] [ "{" { member } "}" ]
member       = [ doc ] [ "override" ] [ Selector ] name "::" Type "=" impl      改行か ';' で区切る（M02 と同じ）
protocol     = [ doc ] [ "@trusted" ] [ "private" ] "extern" [ RuntimeName ] "protocol" Name
               [ ":" Proto { "," Proto } ] [ "{" { pmember } "}" ]
pmember      = [ doc ] [ "optional" ] [ Selector ] name "::" Type
RuntimeName  = 文字列（ObjC の識別子。既定は Tsuzuri の名前。M01 と同じ検査）
Selector     = 文字列（M02 の形。引数のないメンバーでは省略でき、名前と同じ）
impl         = 型引数のないトップレベルの関数の名前（修飾してよい）
```

- `export` は既存の予約語で、`extern` の宣言の中では `class` の直後だけに書ける（`extern class export`）。`extern ... export def` と `export extern` の拒否は今のまま（`src/parser.rs:4864-4870`・`969`）。`extern class` の分岐だけに `export` を足す（決定事項 D1）。
- 修飾子の順は `extern class [export] [main|sync] Name`。`main`・`sync` は M01・M09 の文脈キーワードで、export のクラスでも同じ意味（仕様 9）。
- `with` は既存の予約語で、基底の後にプロトコルの列を置く。export でない M01・M02 の `extern class` にも書ける（宣言した適合。決定事項 D8）。
- `override` と `optional` はメンバーの先頭だけの文脈キーワード、`protocol` は `extern`（と実行時名の文字列）の後だけの文脈キーワード（予約語にしない）。それ以外の位置では普通の名前。
- 基底は必須で、M01・M02 の `extern class` か、別の export のクラス。プロトコルの列の要素は `extern protocol` でなければならない。
- export のクラスのメンバーはすべて `= impl` を持つ。メンバーの前の `class`（クラスメソッド）は Phase 1 では書けない（対象外）。
- export のクラスのメンバー `m :: P1 -> … -> Pn -> R` は、M02 と同じく関数 `Name.m :: ref Name -> P1 -> … -> Pn -> R` になる。Tsuzuri から `Name.m (ref obj) …` と呼ぶと、M02 の型付きの送信（`objc_msgSend`）に下がり、派生のクラスの上書きが効く。
- ビルドに含まれるモジュールの export のクラスは、Tsuzuri のコードが参照しなくてもすべて登録する（NIB・`NSClassFromString` が名前で引くため）。

| 誤り | コード | kind（案） |
| --- | --- | --- |
| メンバーに `= impl` がない | `E0002` | `objc_export_member_without_impl` |
| export のクラスのメンバーの前の `class` | `E1008` | `objc_export_class_method_unsupported` |
| export でないクラスのメンバーに `= impl` | `E0002` | M02 の `extern_class_member_syntax` |
| メンバーの前の `override`・`optional` の置き場の誤り（export でないクラスの `override`、プロトコル以外の `optional`） | `E0002` | `misplaced_override`・`misplaced_optional` |
| `with` の要素がプロトコルでない、基底がクラスでない | `E1003` | `objc_protocol_not_protocol`・M01 の `objc_class_base_not_class` |
| 同じ実行時名の export のクラスが 2 つ（同じビルドの中） | `E1001` | `duplicate_objc_runtime_name` |
| 1 つの export のクラスで同じセレクターが 2 回 | `E1001` | `duplicate_objc_selector` |
| メンバーの名前 `Super` | `E1001` | `objc_member_reserved` |
| プロトコルの継承の循環、深さ・数の上限 | `E1008`・`E1017` | `objc_protocol_base_cycle`・M01 の `objc_class_limit` |

### 2. 実装の関数と型（Phase 1）

- `impl` の型は `ref Name -> i64 -> P1 -> … -> Pn -> R`（`m :: unit -> R` なら `ref Name -> i64 -> R`）でなければならない。違えば `E1003`（`objc_impl_type`）。1 つ目の引数は呼ばれたオブジェクト（`self`。呼び出しの間だけの借用）、2 つ目は ivar `tz_state` の鍵（仕様 4）。`_cmd` は渡さない。
- 型引数のある関数・ラムダ・部分適用・局所の関数・std の関数・`extern` は `E1008`（静的コールバックと同じ規則。`FnPtr.of` の誤りの kind を共有する）。1 つの関数を複数のメンバーと複数のクラスに結んでよい（クラスとメンバーごとに包みを作る）。
- `impl` の本体は固定された文脈（W19 の D5 のコールバックの本体と同じ）で、`local` のハンドルを触ってよい。
- 引数の型（ObjC の位置の型。M02 と同じ下げ方）:

| Tsuzuri の型 | C の型 | 包みのすること |
| --- | --- | --- |
| `bool` | `BOOL`（arm64 は `i1 zeroext`） | そのまま |
| 整数・`f32`・`f64` | 同じ幅の整数・`float`・`double` | そのまま |
| `ObjC.Sel`・`ObjC.Class` | `SEL`・`Class` | そのまま |
| `ref C`・`ref P`（`C` はクラス、`P` はプロトコル） | `id`（+0） | 呼び出しの間だけの借用。NULL なら `null extern handle` でトラップ |
| `Maybe<ref C>`・`Maybe<ref P>` | `id`（+0） | NULL は `None` |
| `ref mut bool` | 1 バイトの `BOOL *` | 1 バイトを局所に読み、`ref mut` で渡し、戻った後に書き戻す（非ゼロは true に正規化）。NULL ならトラップ |
| `ref mut T`（`T` は整数・`f32`・`f64`） | `T *` | 同じ。W26 の仕様 4 の変換が先に入っていればそれを使う |
| `ref ObjC.Block<F>`・`Maybe<ref ObjC.Block<F>>` | Block（+0） | [M04](M04-objc-blocks.md) の Phase 2 が入ったときだけ |

- 所有する `C` の引数は `E1003`（`objc_impl_type`）。IMP が受ける物体は +0 で、所有を受け取らない（M01 の `objc_owned_object_argument` と同じ理由）。
- 結果の型: `unit`、`bool`、整数・`f32`・`f64`、`ObjC.Sel`・`ObjC.Class`、所有する `C`・`Maybe<C>`・`P`・`Maybe<P>`。物体の結果は、包みが `objc_autoreleaseReturnValue` を呼んで +0 にして返す（族でないメソッドの Cocoa の規則）。`Maybe<C>` の `None` は nil。
- `@c_layout` record の引数・結果は Phase 1 では `E1008`（`abi_record_by_value_target`。W36・M02 と共有）。buffer（`ref [T]`・`ref string`）は長さを受け取れないので `E1003`（`objc_impl_type`）。

### 3. オーバーライドとプロトコルとの照合（Phase 1）

- **継承したメンバー**: 基底の連鎖（M02 の `extern class` のメンバーと、基底の export のクラスのメンバー）に同じセレクターのメンバーがあれば、そのメンバーには `override` が要る。なければ `E1008`（`objc_missing_override`）。カテゴリー（M02 の `extern category`）のメンバーは見ない。
- `override` のメンバーは、継承したメンバーと型が完全に一致しなければならない（`Self` は宣言した型に置き換えてから比べる）。違えば `E1003`（`objc_override_mismatch`。両方の型を示す）。継承に同じセレクターがないのに `override` を書くと `E1008`（`objc_override_without_base`）。
- **族と dealloc**: セレクターが init・copy・mutableCopy・new の族（M02 の `objc::method_family` の判定）か `dealloc` のメンバー、継承したメンバーに `@returns_retained`・`@returns_not_retained` があるメンバーは、Phase 1 では `E1008`（`objc_export_family_unsupported`）。族の所有の規則（init の受け手の消費と super の init、copy の +1）を IMP で守る手順が決まっていないため。export のクラスのメンバーの前の `@returns_retained`・`@returns_not_retained` は M01 の `E0002`（`misplaced_returns_retained_attribute`）。
- **プロトコル**: `with P` の各 `P`（と `P` の基底のプロトコル）の `optional` でないメンバーについて、同じセレクターで同じ型のメンバーを、このクラスか基底の連鎖が持たなければならない。ないと `E1008`（`objc_protocol_member_missing`）、型が違うと `E1003`（`objc_protocol_member_mismatch`）。`optional` のメンバーは実装しなくてよい。プロトコルのメンバーの実装に `override` は要らない（継承にも同じセレクターがあれば、継承の規則が先）。

### 4. 状態（Phase 1）

- export のクラスの連鎖の最も基底に近い export のクラスが、8 バイトの ivar `tz_state`（型の符号 `q`、整列 8）を足す。派生の export のクラスは同じ ivar を使う。
- 新しいオブジェクトの `tz_state` は 0（ObjC が 0 で埋める）。鍵 0 の `Anchor.read`・`Anchor.update` は `Stale`（W23）なので、`ObjC.set_state` の前に呼ばれたメソッドは状態に届かない。
- `ObjC.set_state :: i64 -> ref C -> unit` は、`C` の `tz_state` に鍵を書く。`C` が export のクラス（か、その派生）でなければ `E1005`（`objc_state_target_not_export_class`）。D-59 に従い対象を最後に置く（`ObjC.set_state key (ref obj)`）。
- 書き込みと読み出しは `monotonic` の atomic（別のスレッドの IMP が読んでも LLVM のデータ競合にならない。鍵の正しさは `Anchor` が検査する）。
- 実装は `Anchor.read`・`Anchor.update` で状態を読み書きする。`Anchor.Error`（`Stale`・`WrongType`・`WrongThread`・`Busy`）の扱いは実装が決める。再入（実装が呼んだ AppKit のメソッドが同じオブジェクトのメソッドを呼ぶ）は `Busy` で返り、トラップしない。

### 5. 呼ばれ方: IMP の包み（Phase 1）

ObjC がメソッドを呼ぶと、コンパイラが生成した IMP の包みが次をする。

1. 実行ファイルでは W12 の検査（`@tz.main.done` が立っていればトラップ）。M09 の Phase 2 があれば、本体が効果 `main_thread` を持つ包みは入口で `pthread_main_np()` を検査し、0 なら `Tsuzuri runtime: a callback that uses main-thread handles was called on another thread` を報告してトラップする（`main` の export のクラスの IMP は `ref Self` を触るので必ず当たる）。
2. `self` と ivar のオフセット（登録のときに読んだ値）から鍵を読む。
3. 引数を仕様 2 の表のとおりに Tsuzuri の値に直し、`impl(self, 鍵, 引数…)` を呼ぶ。
4. 結果を C の値に直す（物体は `objc_autoreleaseReturnValue`）。`ref mut` の引数を書き戻す。

- 包みの名前は `@tz.callback.objc-imp.<実行時名>.<メンバー名>`。`FnPtr` と静的コールバックの包みは `@tz.callback.<修飾名>`（`src/llvm_abi.rs:953`）で、修飾名は ASCII の識別子を `.` で結ぶ。名前空間の `::` も `.` になる（小文字のディレクトリ `objc/` の `Tz.tz` の `on_hash` を `FnPtr.of` に渡すと、包みは `@tz.callback.objc.Tz.on_hash` だった）。したがって `objc.` で始める名前は利用者の関数の包みと衝突しうる。`-` は識別子に現れないので、`objc-imp` の成分は衝突しない（LLVM は `-` を引用符なしの名前に許し、PB07 の分割も名前の文字に数える。`src/llvm_split.rs:169-171` の `identifier_byte`）。` @tz.callback.` で始まるので、spawn するプログラムでは `with_blocking_externs` が `tsuzuri_tz_callback_enter`・`_exit` と `tsuzuri_tz_callback_scope_enter`・`_leave` で包む（W12 のコールバックと同じ入口と出口。main の擬似 G では何もしない）。
- 包みは `llvm_abi::wrapper` を共有する（`WrapperKind` に ObjC のメソッドの種類を足す。S1）。
- export のクラスを持つプログラムでは、IR に `@tz.callback.` があるので `--trap-mode return` と `Domain.run` は今のコールバックと同じく `E2000`。

### 6. 登録（Phase 1）

- export のクラスを 1 つ以上持つビルドだけが、`@llvm.global_ctors` に 1 つの constructor `@tz.objc.register_classes`（優先度 65535）を出す。そのビルドでは M01 の `objc_used` を立てる（ObjC のモジュールのフラグと、M01 の `uses_objc` の判定による `-lobjc`。constructor が呼ぶ `objc_allocateClassPair` などは libobjc にある。M04・M05・M06・M10 と同じ形）。
- 登録の順は、export のクラスの継承の木のトポロジカルな順（基底が先）で、同じ深さの間は実行時名のバイト順（決定的）。クラスごとに次をする。
  1. 基底の `Class` を得る。M01・M02 の `extern class` は M01 の classref（`OBJC_CLASS_$_<実行時名>`）を読み、export のクラスは先に登録した `@tz.objc.class.<実行時名>` を読む。
  2. `objc_allocateClassPair(base, "<実行時名>", 0)`。NULL（同じ名前のクラスがプロセスにすでにある）なら、標準エラーへ `Tsuzuri runtime: the Objective-C class '<name>' is already registered; give the extern class export a unique runtime name` を書いてトラップする（`@tz.trap.write` と `llvm.trap`）。
  3. 連鎖の最も基底に近い export のクラスなら `class_addIvar(cls, "tz_state", 8, 3, "q")`。
  4. メンバーごとに `class_addMethod(cls, sel_registerName("<セレクター>"), <IMP の包み>, "<型の符号>")`。
  5. `with` のプロトコルごとに `objc_getProtocol("<実行時名>")` を呼び、NULL でなければ `class_addProtocol`。NULL なら足さない（`conformsToProtocol:` は NO を返す。文書に書く）。
  6. `objc_registerClassPair(cls)`。`@tz.objc.class.<実行時名>` に `cls` を、`@tz.objc.ivar.<実行時名>` に `ivar_getOffset(class_getInstanceVariable(cls, "tz_state"))` を書く。
- `class_addIvar`・`class_addMethod` が NO を返したら（コンパイラの誤り）、`Tsuzuri runtime: could not register the Objective-C class '<name>'` を書いてトラップする。`@tz.trap.write` は、トラップの報告がなければ W12 の `@tz.callback.after_main` と同じく 1 つだけ出す（`src/llvm.rs:2257-2260` の形）。
- constructor は libobjc の関数と生成した IR だけを呼び、Tsuzuri の関数もランタイムも呼ばない（S3）。`--emit object`・`--emit shared` でホストに組み込んでも、ロードのときに同じく走る。
- export のクラスへの M01・M02 のクラスの参照（`TzView.alloc ()` の受け手、`ObjC.downcast` の対象）は、`OBJC_CLASS_$_<名前>` ではなく `@tz.objc.class.<実行時名>` を読む（実行時に作るクラスには `OBJC_CLASS_$_` の symbol がない）。

### 7. `Super` への送信（Phase 1）

- `Name.Super.m recv a1 … an` は、`Name` の基底の連鎖のメンバー `m` を、`Name` の基底の実装で呼ぶ。`recv` の型は `ref Name`。型と所有の規則は M02 の送信と同じ（族の規則、+0 の結果は M01 の D6 の `clang.arc.attachedcall` の bundle で retain、`Self` は `Name`）。
- 書けるのは、`Name` のメンバーが `= impl` で結ぶ関数の本体の中（その中のラムダを含む）だけ。外では `E1008`（`objc_super_outside_export_class`）。`m` が基底の連鎖にない（`Name` 自身のメンバーだけにある）と `E1002`（`objc_super_unknown_member`）。
- 下げ方は clang と同じ: `alloca { ptr, ptr }` に `recv` と `@tz.objc.class.<Name の実行時名>`（今のクラス）を書き、`call <正確な型> @objc_msgSendSuper2(ptr %super, ptr %sel, <引数>)`。`declare ptr @objc_msgSendSuper2(ptr, ptr, ...)` は 1 行だけ（共通の規則。利用者の `extern` があればその行を使う）。
- 呼び出しはすべての引数を与えた直接の呼び出しだけ（関数値と部分適用は M02 の `objc_method_not_called`）。

### 8. `extern protocol` とプロトコルの型（Phase 1）

- `extern protocol P [: Q…] { … }` はプロトコルを宣言する。`@trusted` を付ける（なければ使った所で `W1010`）。メンバーは M02 のメンバーと同じ型の規則で、`optional` を付けたものは実装しなくてよい。
- プロトコルの名前は型として使える: `ref P`・`Maybe<ref P>`（+0 の `id<P>`）、所有する `P`・`Maybe<P>`（+1。drop で `objc_release`）。性質は W19 の `local`（M01 のクラスの既定と同じ）。
- 変換は `ObjC.upcast :: ref C -> ref P` と `ObjC.upcast_owned :: C -> P` だけで、`C` の宣言か基底の連鎖が `with P`（か `P` を継承するプロトコル）を書いていることを静的に検査する（違えば `E1003`、`objc_not_conforming`）。費用はない。プロトコルの型への `ObjC.downcast`（目的が `P`。実行時に `conformsToProtocol:` の検査が要る）は対象外で、M01 の `E1005`（`objc_target_not_class`）のまま。`ref P` の値からクラスへの `ObjC.downcast` は許す（M01 の下げ方 `objc_opt_isKindOfClass` は物体だけを見るので、受け取る側の型にプロトコルを足すだけでよい）。
- 送信は `P.m (ref obj) …`（`obj` は `P`、または `ref P` の値をそのまま）で、M02 の送信と同じ下げ方。`optional` のメンバーの送信は `E1008`（`objc_optional_member_called`。`respondsToSelector:` の検査がないと unrecognized selector の例外で落ちるため。決定事項 D8）。
- `setDelegate:` の引数は `Maybe<ref NSWindowDelegate>` と型付けでき、export のクラスの値を `ObjC.upcast` で渡す。

### 9. スレッド・target・効果（Phase 1）

- 既定の export のクラスは W19 の `local`（M01 の D1）。IMP は ObjC のどのスレッドからも呼ばれうる。別のスレッドで呼ばれたら、`Anchor` が `WrongThread` を返す（実装はトラップせずに扱える）。
- `sync` の export のクラスは Send・Sync（M01 と同じ。drop で release するので、再利用できる関数値には捕捉できない。`E1005`）。
- `main` の export のクラス（M09 の Phase 2 があるとき）は、IMP の入口で `pthread_main_np` を検査する（仕様 5。M09 は IMP の本体を `main_thread` の使える文脈に数え、入口の検査を同じ包みに置く）。
- target: 型検査は target を知らない。`llvm::emit_target` で、ビルドに export のクラスか `Name.Super.m`・`ObjC.set_state` があれば、wasm は `E1008`（`abi_native_only`）、Apple でない native は `E1008`（`abi_apple_only`）。どちらも `build` のときに出し、`check` は受理する。
- 効果と権限: IMP の登録・`Name.Super.m`・`ObjC.set_state` は効果 `extern` と権限 `ffi`（D-57）。`impl` の関数の効果は、その本体から推論する（コールバックと同じ）。`tsuzuri audit` の `extern` の行に、export のクラスを 1 行で出す（symbol は実行時名。`tz-audit/1` のキーは変えない）。

### Phase 2: 構造体を値で受け返す IMP

- メンバーの引数と結果に `ref` のない `@c_layout` record を許す（`drawRect:` の `NSRect`、`intrinsicContentSize` の `NSSize`、`selectedRange` の `NSRange`、`windowWillResize:toSize:`）。
- 分類は W36 の Phase 2 の `llvm_byvalue::classify`（AAPCS64、macOS は `alignstack` なし）。HFA の引数は `[n x double|float]`、結果は `{ double, … }`。16 バイト以下の非 HFA は、引数なら 8 バイト以下で `i64`・それより大きければ `[2 x i64]`、結果なら 8 バイト以下で `i{8s}`・それより大きければ `[2 x i64]`（W36 の仕様 3。AAPCS64 は仕様 2 の Windows Arm64 の行と同じ形）。それより大きい引数は複写へのポインター、結果は先頭の `ptr sret(%tz_record_…)`（x8）。IR の形は「現状」の clang の参照と一致させる（`define internal { double, double } @tz.callback.objc-imp.TzView.intrinsic_content_size(ptr %self, ptr %cmd)`）。
- 包みは、引数を `by_value_from_abi` で一時領域へ書いて `read_host_record` で Tsuzuri の値にし、結果を `write_host_record` から `by_value_to_abi` で ABI の値にする（W36 の D6 が呼ばれる側に用意した向き。W26 の Phase 3 と同じ形）。
- 型の符号の構造体は `{Tag=…}`（`Tag` は record の短い名前。決定事項 D12）。
- 分岐 4 の (b) を選んだ場合は、HFA だけを先に扱い、非 HFA（`NSRange`）は W36 の Phase 2 まで `abi_record_by_value_target` のままにする案だった。(b) は選ばれなかった（分岐 4 は 2026-11-26 に (a) で承認済み）。
- 文書から「今の回避策（`_objc_msgForward` と `forwardInvocation:`）」の案内を消し、要らなくなったことを書く。

## 設計

### 構文と AST

- `src/syntax.rs`: M01 のクラスの宣言に `export: Option<Span>`・`protocols: Vec<TypeName>` と、メンバーに `override_span: Option<Span>`・`implementation: Option<QualifiedName>` を足す。プロトコルの宣言 `ExternProtocolDecl { doc, visibility, runtime_name, name, bases, members }` を足し、メンバーに `optional_span` を持たせる。
- `src/parser.rs`: `Parser::extern_declaration` の `class` の分岐で、名前の前の `export`（`TokenKind::Export`）と `main`・`sync` を読む。`protocol` の分岐を足す（`type` は予約語なので手本にならない。`extern view` の `view`（`src/parser.rs:4871`）と同じく、`link_name` の後の識別子の綴りで見分け、同じ行に名前が続くときだけ文脈キーワードにする）。メンバーの先頭の `override`・`optional` も、同じ行に文字列か識別子が続くときだけキーワードにする（W18 の `copy` と同じ規則）。
- `src/syntax_codec.rs`・`src/frontend_cache.rs`（`FRONTEND_FORMAT` を 1 上げる）・`src/formatter.rs`（正準の並び `override` → セレクター → 名前 → 型 → `=` → 実装）と、`grep -rln extern_types src` の全件にプロトコルを足す。

### 型検査（`src/check.rs`）

- M01 のクラスの表（`CheckedModule` の脇の表）に `export: bool`・`protocols`・`members[].implementation` を足し、プロトコルの表 `ObjcProtocolInfo { runtime_name, bases, members }` を足す。
- 仕様 2・3 の照合は、M02 のメンバーの型（`Self` を置き換えた関数型）を使う 1 つの関数 `objc_member_signature` にまとめ、override・プロトコル・`impl` の 3 つの照合で共有する。
- `Name.Super` は、M02 のメンバーの名前の解決（`Checker::case_path` の形）に `Super` の枝を足す。今の関数が `= impl` で結ばれた関数かを、export のクラスの表から引く。
- `ObjC.set_state` は `Builtin` に足し、`Builtin::ALL`・`name`・`signature`・`emit_builtin`・効果と権限の表を更新する。

### 型の符号化器（`src/objc_encoding.rs`、新規。M04 と共有）

M03 と M04 のうち先に入ったほうが作る。clang の `@encode`（arm64 の Apple）と同じ文字列を作る。

```rust
/// M03/M04: the Objective-C type encoding that clang's @encode writes for arm64 Apple targets.
pub(crate) enum Receiver { Method, Block }
pub(crate) fn signature(receiver: Receiver, parameters: &[Type], result: &Type, types: &TypeContext<'_>) -> Result<String, Unencodable>;
pub(crate) fn type_code(ty: &Type, types: &TypeContext<'_>) -> Result<String, Unencodable>;
```

| Tsuzuri の型 | 符号 | Tsuzuri の型 | 符号 |
| --- | --- | --- | --- |
| `unit`（結果） | `v` | `bool` | `B` |
| `i8`・`i8u` | `c`・`C` | `i16`・`i16u` | `s`・`S` |
| `i32`・`i32u` | `i`・`I` | `i64`・`i64u` | `q`・`Q` |
| `f32`・`f64` | `f`・`d` | `ObjC.Sel`・`ObjC.Class` | `:`・`#` |
| クラス・プロトコル（所有・`ref`・`Maybe`） | `@` | `ObjC.Block<F>`（M04） | `@?` |
| `ref mut T` | `^` + `T` の符号 | `@c_layout` record（Phase 2） | `{Tag=…}` |

- 並びは「結果の符号、引数の枠の合計、各引数の符号とオフセット」。メソッドは先頭に `@0:8`、Block は `@?0`。各引数の枠は `max(大きさ, 4)` バイトで、オフセットはその累計（clang 21 で確かめた例: `"B48@0:8f16B20s24C28:32#40"`、`"q32@0:8@16^@24"`、`"v24@0:8@?16"`、`"Q16@0:8"`、Phase 2 の `"v56@?0{CGRect={CGPoint=dd}{CGSize=dd}}8{_NSRange=QQ}40"`）。
- 単体テストは clang の出力から手で写した表と比べる（出力をそのまま期待値にしない）。clang と SDK のあるホストでは、`.m` の `method_getTypeEncoding` と突き合わせる検査も走らせる（W36 の D4 と同じ形）。

### 生成（M01 の ObjC の生成のファイルと `src/llvm_abi.rs`）

- 大域変数（クラスごと。実行時名のバイト順に出す）:

```llvm
@tz.objc.class.TzCounter = internal global ptr null, align 8
@tz.objc.ivar.TzCounter = internal global i64 0, align 8
@tz.objc.name.TzCounter = private unnamed_addr constant [10 x i8] c"TzCounter\00"
@tz.objc.methtype.0 = private unnamed_addr constant [11 x i8] c"B24@0:8@16\00"
@llvm.global_ctors = appending global [1 x { i32, ptr, ptr }] [{ i32, ptr, ptr } { i32 65535, ptr @tz.objc.register_classes, ptr null }]
```

- 型の符号の文字列は内容ごとに 1 つにまとめ、番号は最初に現れた順（決定的）。
- `@tz.objc.register_classes`（`internal void () nounwind`）は仕様 6 の手順を直列に出す。`objc_allocateClassPair`・`class_addIvar`・`class_addMethod`・`class_addProtocol`・`objc_getProtocol`・`objc_registerClassPair`・`class_getInstanceVariable`・`ivar_getOffset`・`sel_registerName` の `declare` は symbol ごとに 1 行（共通の規則。利用者の `extern` が同じ symbol を宣言していればその行を使い、呼び出しは自分の関数型で `call` する。「現状」の再現 1 の `declare i32 @class_addMethod(i64, i64, ptr, ptr)` と共存する）。
- IMP の包み（`isEqual:` の例）:

```llvm
define internal zeroext i1 @tz.callback.objc-imp.TzCounter.is_equal(ptr %self, ptr %cmd, ptr %arg0) nounwind {
entry:
  ; W12: load atomic i32, ptr @tz.main.done acquire（実行ファイルだけ）
  %offset = load i64, ptr @tz.objc.ivar.TzCounter, align 8
  %slot = getelementptr inbounds i8, ptr %self, i64 %offset
  %token = load atomic i64, ptr %slot monotonic, align 8
  %r = call i1 @tz.fn.Main.on_is_equal(ptr %self, i64 %token, ptr %arg0)
  ret i1 %r
}
```

- 物体の結果は `%r1 = call ptr @objc_autoreleaseReturnValue(ptr %r)` を `ret` の前に置く。`Maybe<C>` の nil もそのまま渡してよい（`objc_autoreleaseReturnValue(nil)` は nil を返すので、分岐は要らない）。
- `ObjC.set_state key (ref obj)`: `@tz.objc.ivar.<連鎖の根の export のクラス>` を読み、`store atomic i64 %key, ptr %slot monotonic, align 8`。
- `Name.Super.m`: 仕様 7 の `alloca { ptr, ptr }` と `objc_msgSendSuper2`。selref は M01 のもの。
- `@llvm.global_ctors` は `appending` なので、PB07 の分割は -O0 で黙って分割なしに戻る。M01 の `@llvm.compiler.used` で ObjC のプログラムはすでに戻っているので、M03 が足す退行はない。export のクラスを持たないプログラムには出さない（S2）。

### 採らなかった案

静的なメタデータ・W26 と同じ値の表・associated object は決定事項 D2・D3・D5 で比べた。ほかに次を採らない。

| 案 | 採らなかった理由 |
| --- | --- |
| IMP で `object_getIvar` を呼ぶ | 呼び出しごとに関数呼び出しと ivar の探索が要る。オフセットを登録のときに 1 度読めば足りる |
| `forwardInvocation:` を構造体の結果の正式な下げ方にする | 呼び出しごとに `NSInvocation` を組み、Foundation に依存する。W36 の Phase 2 の分類を使えば clang と同じ直接の IMP になる（Phase 2） |
| クラスごとの ObjC のシム（`.m` を生成して clang でコンパイルする） | clang と SDK がビルドに要り、同梱の zig だけの構成で作れない。D-67 はポインターを触るコードをコンパイラが宣言から生成する方針 |

## 前提とする他チケットのインターフェース

- [M01](_completed/M01-objc-class-types.md): `extern class` の表（基底・実行時名・性質）、`ObjC.Object`・`ObjC.Sel`・`ObjC.Class`、classref・selref の生成、`ObjC.upcast`・`ObjC.upcast_owned`・`ObjC.downcast`、`objc_release` の drop、ObjC のモジュールのフラグと `-lobjc`、kind `abi_apple_only`・`objc_class_base_not_class`・`objc_class_limit`・`misplaced_returns_retained_attribute`・`objc_target_not_class`。
- [M02](_completed/M02-objc-typed-message-send.md): メンバーの構文（セレクター・`Self`・`class`）と `ObjcMember`・`ObjcMemberInfo`（`= impl` は M03 が足す）、`objc::{method_family, valid_selector, selector_arity}`、`abi::{objc_parameter, objc_result, objc_signature}`（IMP の型の検査に使う）、`Self` の置き換え、送信の引数の下げ方（`Name.Super.m` は呼び先だけが違う）と `objc_msgSend` の `declare` の共有、クラスの受け手のフック（export のクラスは `@tz.objc.class.<名前>`）、kind `objc_selector_arity`・`duplicate_objc_member`・`objc_method_not_called`・`extern_class_member_syntax`。Phase 2 の HFA の分類（分岐 4 の (b)。(b) は選ばれなかったので使わない）。
- [W12](_completed/W12-static-callback-lifetime.md)・[W22](_completed/W22-fnptr-values.md): `llvm_abi::wrapper`、`WrapperKind::Callback`、`@tz.main.done`、`with_blocking_externs`、静的コールバックの関数の規則。
- [W23](_completed/W23-anchor-state.md): `Anchor.create`・`token`・`read`・`update`、`Anchor.Error`、鍵 0 の `Stale`。
- [W19](_completed/W19-thread-bound-handles.md): `local` の性質、固定された文脈、`pinned_rules::builtin_position`。
- [M09](_completed/M09-main-thread-dispatch-and-handles.md)（Phase 2）: `main` の文脈キーワード、効果 `main_thread`、`pthread_main_np` の検査とその報告の文。
- [W36](_completed/W36-c-layout-by-value.md)（Phase 2）: `llvm_byvalue::{ByValue, classify}`（AAPCS64）、`FunctionEmitter::{by_value_slot, by_value_from_abi, by_value_to_abi}`、kind `abi_record_by_value_target`。
- [W26](W26-com-objects-in-tsuzuri.md)（あれば）: 仕様 4 のコールバックの `ref mut T` の変換。
- [M04](M04-objc-blocks.md)（Phase 2）: `ObjC.Block<F>` の引数の受け取り（+0 の `ref ObjC.Block<F>`）。

## 他チケットへ提供するインターフェース

- [M04](M04-objc-blocks.md): `src/objc_encoding.rs`（`signature` の `Receiver::Block`）。M03 が先に入れば M04 は表に `@?` の行を足すだけ。IMP の包みの `WrapperKind` と `ref mut bool` の 1 バイトの変換。
- [M07](M07-apple-sdk-bindgen-and-package.md): `extern protocol`（`optional` を含む）と `with` の構文。生成器は ObjCProtocolDecl を `extern protocol` に、`@interface C : B <P, Q>` を `with P, Q` に写す。export のクラスは生成しない（利用者が書く）。
- [M09](_completed/M09-main-thread-dispatch-and-handles.md)（Phase 2）: IMP の包みの入口（`main` の検査を置く所）。

## 実装手順

### Phase 1

1. **変更**: なし（ブランチ `feat/m03-objc-export-classes`、`target/scratch/m03/`）。**内容**: `node scripts/repo-kb.mjs brief M03`、着手条件、ベースライン。clang で「現状」の参照の IR と、仕様 2 の型の組み合わせの `.m`（`BOOL`・`float`・`short`・`unsigned char`・`SEL`・`Class`・`id`・`BOOL *`・`NSError **`）の型の符号を取り直す。**確認**: 保存した件数と、型の符号の表が「設計」の表と一致すること（違えば表を直してから進める）。
2. **変更**: `src/objc_encoding.rs`（M04 が先なら変更なし）。**内容**: 型の符号化器。**確認**: 単体テスト（手で写した表）。`cargo nextest run --locked objc_encoding`。
3. **変更**: `src/syntax.rs`・`src/parser.rs`・`src/syntax_codec.rs`・`src/frontend_cache.rs`・`src/formatter.rs` と `grep -rln extern_types src` の全件。**内容**: 構文と AST、整形。**確認**: `tests/objc_export_classes.rs` の構文の検査（受理・`E0002` の kind・`export def` の拒否が今のまま）。`cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`。
4. **変更**: `src/check.rs`・`src/diagnostic_kinds.rs`。**内容**: 仕様 1〜4・7・8 の型検査と kind。**確認**: 型の検査の Rust テスト。`cargo nextest run --locked`。
5. **変更**: `src/llvm_abi.rs`・M01 の ObjC の生成のファイル・`src/llvm.rs`。**内容**: IMP の包み、登録の constructor、`ObjC.set_state`、`Name.Super.m`、export のクラスの classref。**確認**: IR の検査（constructor・登録の順・型の符号・包み・`declare` が 1 行・2 回の生成で一致）。ベースラインと一致（S2）。
6. **変更**: `src/effects.rs`・`src/pure_rules.rs`・`src/kg/effects.rs`・`src/capabilities.rs`・`src/pinned_rules.rs`。**内容**: 効果・権限・固定された文脈・監査の行、件数の pin（今の値 + 足した数）。**確認**: `tests/capabilities.rs`・`tests/thread_bound_handles.rs`。
7. **変更**: `tests/objc_export_classes.mjs`・`tests/objc_export_classes_host.m`・`tests/fixtures/objc_export_classes/Main.tz`。**内容**: macOS の E2E（テスト計画）。**確認**: `cargo build` の後に `node tests/objc_export_classes.mjs target/debug/tsuzuri`（-O0・-O3、Apple の ld と同梱の zig）。終わった後に残ったプロセスがないこと。
8. **変更**: M09 の Phase 2 が done なら、`main` の修飾子と IMP の入口の検査。**確認**: 別のスレッドから呼ぶ case がトラップする。
9. **変更**: 文書。**確認**: T2、`node scripts/check-docs.mjs <変更したページ>`、`node scripts/gen-docs.mjs --check`、PR に `ci:full`。

### Phase 2

1. W36 の Phase 2 の `classify` と呼ばれる側の下げ方を、IMP の包みの引数と結果に使う。**確認**: IR の検査（「現状」の clang の参照の 3 つの `define` の形）。
2. 型の符号化器に `{Tag=…}` を足す。**確認**: 単体テスト（`"{CGSize=dd}16@0:8"`・`"{_NSRange=QQ}16@0:8"`・`"v48@0:8{CGRect={CGPoint=dd}{CGSize=dd}}16"` の形）。
3. E2E と AppKit の段（テスト計画の Phase 2）。文書の回避策の案内を消す。**確認**: T2、`ci:full`。

## テスト計画

### Rust（`tests/objc_export_classes.rs`、新規。どのホストでも）

| 検査 | 期待 |
| --- | --- |
| 構文 | `extern class export [main\|sync] Name : Base with P, Q { … }`、`extern protocol P : Q { optional … }` を受理する。`extern "x" export def` と `export extern def` は今の `export_on_extern_declaration`。`override`・`optional`・`protocol` を普通の名前として使うプログラムが今のまま通る |
| 型 | `impl` の型の誤り（`objc_impl_type`）、所有する `C` の引数、`override` の欠け・不一致・基底なし、族と `dealloc`、プロトコルのメンバーの欠け・不一致、`optional` の送信、外の `Super`、`ObjC.set_state` の対象、同じ実行時名・同じセレクター、`Super` という名前のメンバー |
| target | `--target wasm32` で `abi_native_only`、`--target x86_64-unknown-linux-gnu` で `abi_apple_only`。`check` は通る |
| IR | `@llvm.global_ctors` が 1 つ、登録の順（基底が先、同じ深さは実行時名の順）、型の符号の文字列（`isEqual:` は `"B24@0:8@16"`）、包みの形と `objc_autoreleaseReturnValue`、`objc_msgSendSuper2` と `{ ptr, ptr }`、`declare` が symbol ごとに 1 行、利用者が `class_addMethod` を `extern` で宣言したプログラム、2 回の生成で一致 |
| 不変 | export のクラスを持たない入力（全 fixture・example）の IR・診断がベースラインと一致 |

### E2E（`tests/objc_export_classes.mjs` と `tests/objc_export_classes_host.m`、macOS だけ）

ホストの `.m` は `-fobjc-arc` の clang でビルドし、`--link` で渡す。Tsuzuri の `main` がオブジェクトを作り、ホストの `tzobjc_drive(id)` に渡す。-O0・-O3、Apple の ld と同梱の zig（M20 の SDK の探索）の両方。

| case | 内容 | 期待 |
| --- | --- | --- |
| `counter` | 早見の例 | 終了コード 42 |
| `invoke` | ホストが `NSInvocation` と直接のキャスト（`((BOOL (*)(id, SEL, id))objc_msgSend)`・`double` の結果）で `BOOL`・`f64`・`f32`・`i8u` のメソッドを呼ぶ | ホストが記録した値が期待の列と一致 |
| `conforms` | `[obj conformsToProtocol:@protocol(TzCounting)]`（ホストが同じ名前のプロトコルを宣言して実行時に存在させる）と、存在しないプロトコルの `with` | YES と NO |
| `super` | `isEqual:` と `hash` の上書きが `Super` で基底を呼ぶ | 基底の結果に足した値 |
| `object_result` | 族でないメソッドが Tsuzuri で作った物体を返し、ホストの `@autoreleasepool` の中で受ける。ホストのクラスの `dealloc` が数える | 生きている数が 0 に戻る（釣り合い） |
| `stale` | `ObjC.set_state` の前にホストがメソッドを呼ぶ | 実装が `Stale` を受け取る |
| `wrong_thread` | ホストが `dispatch_async` でグローバルキューから呼ぶ | 実装が `WrongThread` を受け取り、状態が変わらない |
| `reentry` | 実装が `Anchor.update` の中でホストの関数を呼び、ホストが同じメソッドを呼ぶ | 内側は `Busy`、外側は成功 |
| `duplicate` | ホストの `.m` が同じ名前のクラスを静的に定義している | 標準エラーに `Tsuzuri runtime: the Objective-C class 'TzCounter' is already registered; ...`、トラップの終了コード |
| `by_name` | ホストが `NSClassFromString(@"TzCounter")` で作る | 同じクラス、`tz_state` は 0 |
| `blocking` | tzroutine を起動するプログラムで、ホストがブロッキングの `extern` の中から同じスレッドでメソッドを呼ぶ | 状態が更新され、tzroutine の結果も届く |
| `encoding` | ホストが `method_getTypeEncoding(class_getInstanceMethod(cls, sel))` を、同じ型の `.m` のメソッドと比べる | オフセットまで一致 |

- AppKit の段（headless。活性化ポリシー Prohibited か Accessory、`alphaValue` 0、`perl -e 'alarm 15; exec @ARGV'` の時間制限）: `NSView` のサブクラスで `isFlipped`（`bool`）と `acceptsFirstResponder` を上書きし、AppKit が `isFlipped` を読むこと。`NSButton` の target-action を export のクラスのメソッドにし、`performClick:` で呼ばれること（probe E9 と同じ手法）。終わった後に残ったプロセスがないこと。
- Phase 2: `intrinsicContentSize` を上書きした `NSView` の `fittingSize`（12.5×40 を返すと 13×40）、`drawRect:` が `cacheDisplayInRect:toBitmapImageRep:` で呼ばれ、読んだ画素が描いた色（probe E8 の手法）、`selectedRange` をホストが直接のキャストで呼んで `(7, 3)`。
- 性能: IMP の呼び出し（ホストが 1,000 万回呼ぶ空のメソッド）を、clang の `.m` の IMP と、今の手書きの `FnPtr` の IMP と比べて記録する（参考値。CI に速さの合否を置かない）。

## ドキュメント

- `_tsuzuri/language-reference/compiler/objective-c.md`（M01 が作る）: 節「クラスを定義する（extern class export）」（宣言・実装の関数・状態と `ObjC.set_state`・`Super`・プロトコル・登録の時期と実行時名・スレッド・`main` の後に呼ばれないように delegate を外す手順・族と `dealloc` を上書きできないこと）。Phase 2 で構造体の IMP と、`forwardInvocation:` の回避策が要らなくなったこと。
- `_tsuzuri/language-reference/compiler/native-interop.md`: 「静的コールバック」の節から objective-c.md へのリンク。
- `_tsuzuri/language-reference/values-and-functions/keywords.md`: 文脈キーワードに `override`・`optional`・`protocol` を足す。
- `docs/language.md`: 「ホスト関数のインポート」の節（449 行から）に export のクラスとプロトコルの小節。診断の表（kind の表は `<!-- generated:diagnostic-kinds -->` の生成の部分なので手で書かず、`src/diagnostic_kinds.rs` に登録してから `node scripts/gen-docs.mjs`）。
- `docs/architecture.md`: 登録の constructor、ivar のオフセット、IMP の包み、PB07 の分割との関係。
- README の「採用しない改善」の「クラス継承」の行は、D-67 の注記のとおり、M01 の出荷の PR が改める（M03 は変えない）。

## 受け入れ条件

- [ ] Phase 1 の仕様 1〜9 のとおりに動き、仕様の外の入力は表の診断で拒否される。
- [ ] export のクラスを使わないプログラムの IR・object・header・診断・既定の `.wasm` がベースラインと 1 byte も違わない。M01・M02 の fixture の IR も変わらない。
- [ ] 生成 IR が決定的で、`declare` が symbol ごとに 1 行。利用者が libobjc の関数を `extern` で宣言したプログラムも通る。
- [ ] `node tests/objc_export_classes.mjs` が macOS の -O0・-O3、Apple の ld と同梱の zig で通り、残ったプロセスがない。
- [ ] T2 が通る。PR に `ci:full` を付けた。
- [ ] Phase 2（求められたとき）: 構造体の IMP が clang の参照と同じ IR の形で、AppKit の段が通る。

## 落とし穴

- **区間のプールから呼ばれる IMP**（[M10](_completed/M10-objc-autorelease-pools.md) の後。2026-11-29）: Objective-C のイメージの tzroutine のランタイムは、tzroutine がスレッドを手放した時に実行区間のプールを pop する。その pop が走らせる `dealloc` から IMP が呼ばれると、IMP はスケジューラーのスタック（g0。このランタイムでは 1 MiB）の上で、現在の G のない擬似 G として走る。そこで待つと、その M は P を持ったまま眠る。IMP の包みの入口（W12・M09 の検査）は擬似 G で動く形のままにし、`dealloc` の上書きを足すチケットは「`dealloc` の本体は待たない」を文書に書く。
- **`main` の後に呼ばれる**: `NSApp` の delegate や `NSTimer` の target が残ったまま `main` が返ると、W12 のトラップになる。`[NSApp setDelegate:nil]` とタイマーの `invalidate` を `main` が返る前に呼ぶ手順を文書に書く。`[NSApp terminate:]` は `exit()` で終わり、`main` に戻らない（D-61 の待ちと Drop が走らない。M09 の文書と同じ）。
- **`@tz.main.done` の出し忘れ**: 今は `globals.callbacks` が空でないときだけ、`@tz.main.done` の大域と `main` の終わりの `store` を出す（`src/llvm.rs:2238` の `mark_main_done`、`src/llvm.rs:9999-10003`）。IMP の包みも入口で `@tz.main.done` を読むので、`FnPtr` を使わず export のクラスだけを持つ実行ファイルでも出るように、この条件に IMP の包みを足す。足さないと、IR が定義のない大域を参照して clang が拒む。
- **delegate は weak**: AppKit の `delegate` の多くは weak。export のクラスのオブジェクトの所有値を `main` が持ち続けないと、解放されて delegate が nil になる。
- **登録の時期**: constructor はランタイムの初期化より前に走る。Tsuzuri の関数・ランタイム・`IO` を呼ばない（S3）。トラップの報告は `@tz.trap.write` だけで書く。
- **export のクラスの classref**: 実行時に作るクラスに `OBJC_CLASS_$_` の symbol はない。M01 の classref を使うとリンクが失敗する。`@tz.objc.class.<名前>` を読む。
- **dylib の dlclose**: export のクラスを持つ `.dylib` を `dlclose` すると、登録したクラスの IMP が消えたアドレスを指す（実行時に作るクラスは image に結び付かない）。文書に「unload しない」と書く。
- **宣言にない基底のメソッド**: M02 の宣言が基底の全部のメソッドを書いているとは限らない。宣言にないセレクターと同じ名前のメンバーは、`override` の検査をすり抜けて黙って上書きする。テストの `encoding` の case のように、`method_getTypeEncoding` の基底の値と比べる手順を文書に書く。
- **`ref mut bool`**: ObjC の `BOOL *` は 1 バイト。今の `extern def` の `ref mut bool` は `int32_t *`（4 バイト）なので混ぜない（M02 の規則）。
- **`@llvm.global_ctors` と分割**: `appending` なので PB07 の分割を外す。export のクラスを持つプログラムだけに出す。
- **Swift の object との混在**: Swift のクラスの継承は対象外。同じ実行時名を Swift の側が持つと、どちらが先にロードされるかで `duplicate` のトラップになる。

## 対象外

- init・copy・mutableCopy・new の族と `dealloc` の上書き、`tz_state` 以外の ivar。族の所有の規則を IMP で守る手順は、後のチケットの決定事項にする。
- クラスメソッドの実装（`+ (Class)layerClass` など）。
- 静的なメタデータ（clang の `class_ro_t`・`__objc_classlist`）の出力（D3 で比べて採らない）。
- KVO・バインディング・Interface Builder の outlet の接続。
- Swift のクラスの継承。
- 例外（[M05](M05-objc-exception-boundary.md)）。
- プロトコルの型への `ObjC.downcast`（`conformsToProtocol:`）と、`optional` のメンバーの送信（`respondsToSelector:` の検査）。
- カテゴリーのメンバーの上書きの検査。

## 決定事項

| ID | 決定 | 採らない案 | 理由 | 状態 |
| --- | --- | --- | --- | --- |
| D1 | 綴りは `extern [RuntimeName] class export [main\|sync] Name : Base [with P…] { … }`。X′ の形はすべて `extern` の宣言にする（D-63 と同じ）。`extern ... export def` の拒否は変えず、`extern class` の分岐だけに `export` を足す | `export class`（`extern` なし）、新しい語 `implement class`・`objc class` | `export` は「Tsuzuri が実装してホストに見せる」という既存の予約語の意味に合う。予約語を足さない（GUIDE の「守る方針」）。`extern` の宣言にそろえると、`@trusted`・`ffi`・`audit` の扱いが今の extern と同じになる | 要承認（承認前は Phase 1 に着手しない） |
| D2 | 実装はメンバーの `= 関数名` で静的に結ぶ。関数の型は `ref Name -> i64 -> P… -> R`。引数と結果の型の集合は M02 の ObjC の位置の型と同じで、構造体は Phase 2 | W26 の値の表（`Name.Methods` の record と実行時の構築） | ObjC のクラスはプロセスに 1 つなので値の表は要らない。NIB と `NSClassFromString` が `main` の前に名前で引けるのは、静的に結んでロード時に登録するときだけ。鍵を明示して渡すのは W26 の D3 と同じ | 要承認（承認前は Phase 1 に着手しない） |
| D3 | クラスは実行時の API で登録する。生成した `@tz.objc.register_classes` を `@llvm.global_ctors` から呼び、基底を先にトポロジカルな順で登録する。`Class` は `@tz.objc.class.<名前>` に置く | 静的なメタデータ（`class_ro_t`・`__objc_classlist`）、`main` の先頭での登録 | 公開 API だけで、probe と「現状」の C で動いた。ロード時に登録するので NIB と `NSClassFromString` も見つけられる。静的なメタデータは非公開の配置（flags・ポインターの署名・メタクラス）を生成器が持つことになる。`main` の先頭では `--emit object`・`--emit shared` のホストから使えない | 要承認（承認前は Phase 1 に着手しない） |
| D4 | D-67 の分岐 1（X′）の下で、次を D-55 に登録する: `extern class` の後の予約語 `export`、`with` によるプロトコルの列、メンバーの先頭の文脈キーワード `override`・`optional`、`extern` の後の文脈キーワード `protocol`、クラス名の後の `Super`（`Name.Super.m`。D7。`Name` の名前空間の新しい名前で、`Super` という名前のメンバーは `E1001` `objc_member_reserved`）、組み込みの関数 `ObjC.set_state` | 新しい予約語 | 予約語を足さない方針。既存の予約語の新しい位置と文脈キーワードだけにする | 要承認（D-67 の分岐 1 は承認済み。承認前は Phase 1 に着手しない） |
| D5 | 状態は連鎖の根の export のクラスの ivar `tz_state`（8 バイト）に置く `Anchor` の鍵。IMP は登録のときに読んだオフセットから `monotonic` で読む。`ObjC.set_state :: i64 -> ref C -> unit`（D-59 に従い対象が最後）。鍵 0 は `Stale` | associated object、`object_getIvar`、オブジェクトが状態を所有する | 呼び出しごとの費用がオフセットの読み出しだけ。オブジェクトはどのスレッドでも解放されうるので、状態を所有しない（W26 の D2 と同じ） | 既定案（実装者はこの案に従う） |
| D6 | IMP の引数の物体は +0 の借用（`ref C`・`Maybe<ref C>`）だけ。族でないメソッドの物体の結果は、包みが `objc_autoreleaseReturnValue` で +0 にする。init・copy・mutableCopy・new の族と `dealloc` の上書きは Phase 1 では `E1008`（`objc_export_family_unsupported`） | 族の上書きを Phase 1 で許す | Cocoa の所有の規則どおり。init の上書きは受け手の消費と super の init の結果の扱い、`dealloc` は super の `dealloc` の呼び忘れの検査が要り、手順が決まっていない | 既定案（実装者はこの案に従う） |
| D7 | `Name.Super.m` は、`Name` のメンバーが結ぶ関数の本体の中だけに書け、`objc_msgSendSuper2` と `{ recv, 今のクラス }` に下げる | `objc_msgSendSuper` と基底のクラス、どこでも書ける | clang と同じ下げ方で、今のクラスは登録した `Class` から得られる。ObjC の `[super m]` もメソッドの実装の中だけ | 既定案（実装者はこの案に従う） |
| D8 | プロトコルの名前を型にし（`ref P`・`Maybe<ref P>`・`P`。性質は `local`）、変換は `with P` を宣言したクラスからの `ObjC.upcast`・`ObjC.upcast_owned` だけで静的に検査する。`with` は M02 の Apple のクラスにも書ける（宣言した適合）。`optional` のメンバーは実装しなくてよく、送信は `E1008` | `id<P>` を `ObjC.Object` のまま、`optional` の送信を `Maybe` で返す | `setDelegate:` などの引数を型付けできる。Apple のクラスの適合（`NSTextView` の `NSTextInputClient`）も M07 が写せる。`optional` の送信の安全な形は `respondsToSelector:` の検査が要り、後のチケットにする | 既定案（実装者はこの案に従う） |
| D9 | export のクラスの既定のスレッドの性質は `local`（M01 の D1）。`sync` は M01 と同じ。`main` は M09 の Phase 2 で、IMP の入口で `pthread_main_np` を検査する | 既定を `main` にする | M09 の Phase 2 がなくても入れられる。別のスレッドの呼び出しは `Anchor` の `WrongThread` が守る | 既定案（実装者はこの案に従う） |
| D10 | `objc_allocateClassPair` が NULL を返したら、`Tsuzuri runtime: the Objective-C class '<name>' is already registered; give the extern class export a unique runtime name` を報告してトラップする。同じビルドの中の重複は `E1001`（`duplicate_objc_runtime_name`） | 黙って既存のクラスを使う、名前に接尾辞を付ける | 既存のクラスを使うとメソッドの型が違いうる。接尾辞は NIB と `NSClassFromString` の名前を壊す | 既定案（実装者はこの案に従う） |
| D11 | Phase 2 は W36 の Phase 2 の `classify`（AAPCS64）と `by_value_from_abi`・`by_value_to_abi` を IMP の包みに使う（W26 の Phase 3 と同じ形）。分岐 4 の (b)（HFA だけを先に扱う）は選ばれなかった | `forwardInvocation:` の回避策を正式にする、M03 が独自の分類を持つ | clang と同じ直接の IMP になる。分類を 1 か所に保つ（W36 の D4） | 要承認（D-67 の分岐 4 は (a) で承認済み（2026-11-26）。この決定と W36 の S1 の選択の前は Phase 2 に着手しない） |
| D12 | 型の符号化器は `src/objc_encoding.rs`（M03 と M04 の先に入ったほうが作る）。clang の `@encode` と同じ文字列で、構造体の `Tag` は record の短い名前（Phase 2） | 型の符号を利用者に書かせる、構造体を `{?=…}` にする | 誤りの多い手書きをなくす。`NSMethodSignature` は大きさと配置だけを使い、`Tag` の違いは比べない。違いが問題になる API が見つかったら、Phase 2 の決定事項で属性を足す（D-55 が要る） | 既定案（実装者はこの案に従う） |
| D13 | `with` のプロトコルが実行時にない（`objc_getProtocol` が NULL）ときは `class_addProtocol` を飛ばし、ロード時に何も出力しない。`conformsToProtocol:` が NO を返すことを文書に書く | プロトコルを実行時に作る（`objc_allocateProtocol`）、標準エラーに報告する | 後でロードされる framework の同じ名前のプロトコルと衝突しうる。ロード時の出力は利用者のプログラムの出力を変える。AppKit の主なプロトコルは実行時にある（「現状」） | 既定案（実装者はこの案に従う） |
