# M02: 型付きのメッセージ送信（`extern class` のメンバー、`objc_msgSend` を宣言どおりの C の型で呼ぶ、`BOOL`・`f32`・`f64`・構造体の結果、カテゴリー、定数の NSString）

| 項目 | 内容 |
| --- | --- |
| ID | M02 |
| 優先度 | P1 |
| 規模 | XL（Phase 1・2。人間が求めない限り Phase 1 だけを実装する） |
| 依存 | [M01](M01-objc-class-types.md), ([W36](W36-c-layout-by-value.md))（Phase 2 は W36 の Phase 2 と、W36 の停止条件 S1 の選択と、決定事項 D14 の承認が要る。D-67 の分岐 4 は承認済み） |
| 後続 | [M03](M03-objc-classes-defined-in-tsuzuri.md), [M05](M05-objc-exception-boundary.md), [M07](M07-apple-sdk-bindgen-and-package.md), [M23](M23-objc-out-parameters-and-weak-references.md)（弱い後続: [M04](M04-objc-blocks.md), [M10](M10-objc-autorelease-pools.md), [M12](M12-gpu-presentation-to-window.md), [M18](M18-intel-mac-and-universal2.md)） |
| 状態 | todo |
| 起票 | 2026-11-26（D-67。利用者の依頼「「どのチケットにもない壁」と「調査中に見つけたコンパイラの不具合（自分で再現済み）」のそれぞれについて @_features/ に開発チケットまたは修正チケットを作成して」から起票。調査で報告した「壁 1（objc_msgSend を上位形と 0 埋めで呼ぶしかない。f64・構造体の結果は method_invoke か NSInvocation。NSRect は f64 4 個に分解。BOOL の幅の誤り）」に当たる。HEAD `804b7970` で確認） |
| 承認 | [GUIDE D-67](GUIDE.md#d-67-tsuzuri-だけで-macos-のデスクトップアプリを書くm01m22) の分岐 1（X′）と分岐 4（(a)。W36 を Phase 2 まで実装することを人間が求めた）は承認済み（2026-11-26）。要承認: D-55（`extern category` の文脈キーワード `category`、メンバーの先頭の予約語 `class`、型の文脈キーワード `Self`、`ObjC.literal`）。決定事項 D1〜D4 が要承認（Phase 2 は D14 も要承認。W36 の停止条件 S1 の選択も未決）、ほかは既定案 |
| 改善する劣位 | Objective-C・Swift・Rust（objc2 クレート）比: Cocoa のメソッドを宣言どおりの型で呼べない。今は `objc_msgSend` を 1 つの上位形にそろえて空きを 0 で埋め、f64 と構造体の結果を `method_invoke`・`NSInvocation` で受け、NSRect を f64 4 個に分け、BOOL の結果を下位 8 bit に切る（E12 D13、[W14](_completed/W14-extern-multiple-declarations.md) の同一形の規則） |
| 手本にする既存実装 | 構文: `src/parser.rs` の `Parser::extern_declaration`（4807 行〜）と `instance` の本体。呼び出しの形: [W25](W25-extern-interface-com.md) の仕様 3（`I.m :: ref I -> …`、`com_method_not_called`、`host_call` の共有）。引数の下げ方: `src/llvm_imports.rs` の `FunctionEmitter::host_call`（65 行〜）。宣言の 1 行: `src/llvm_imports.rs:339-373`。literal の引数: `src/check.rs` の `validate_zstring_literals`（10619 行）。ブロッキング: `src/llvm.rs` の `extern_symbols`（1801 行）・`with_blocking_externs`（1840 行）。clang の参照との突き合わせ: [W36](W36-c-layout-by-value.md) の D4 |
| 主な影響ファイル | `src/syntax.rs`・`src/parser.rs`・`src/syntax_codec.rs`・`src/frontend_cache.rs`・`src/formatter.rs` ほか `grep -rln extern_types src` の全件、`src/check.rs`・`src/abi.rs`・`src/objc.rs`（新規）・`src/llvm.rs`・`src/llvm_imports.rs`・`src/llvm_objc.rs`（M01 が作る。無ければ新規）・`src/effects.rs`・`src/pure_rules.rs`・`src/kg/effects.rs`・`src/capabilities.rs`・`src/pinned_rules.rs`・`src/diagnostic_kinds.rs`・`src/explain/codes/E1008.md`、`tests/objc_send.rs`（新規）・`tests/objc_send.mjs`（新規）・`tests/objc_send_host.m`（新規）・`tests/objc_send_reference.m`（新規）・`tests/fixtures/objc_send/Main.tz`（新規）・`tests/fixtures/objc_send_appkit/Main.tz`（新規）、`_tsuzuri/language-reference/compiler/objective-c.md`（M01 が作る）・`_tsuzuri/language-reference/compiler/native-interop.md`・`_tsuzuri/language-reference/values-and-functions/attributes.md`・`_tsuzuri/language-reference/compiler/diagnostics.md`・`_tsuzuri/language-reference/built-in-types-and-modules/argument-order.md`・`docs/language.md`・`docs/architecture.md`・`README.md` |

## 実装者向け早見

- 範囲: [M01](M01-objc-class-types.md) の `extern class` に本体 `{ member }` を足し、メンバーを `objc_msgSend` の直接の呼び出しに下げる。呼び出しの関数型は宣言の型から作る（`bool` は `i1 zeroext`、8・16 bit の整数は幅のまま、`f32`・`f64` は浮動小数点のレジスター）。所有は Cocoa の族の規則（alloc・new・copy・mutableCopy は +1、init は受け手を消費して +1、ほかは +0 を受けた直後に retain）。`extern category`、instancetype の `Self`、定数の NSString `ObjC.literal` も足す。構造体の値渡しと値返しは Phase 2。
- **人間が求めない限り Phase 1 だけを実装する。** 承認が記録されるまで、該当する Phase に着手しない。
- 最初に読む: ① 「現状」の再現と clang の参照の IR ② 仕様 1〜6 ③ M01 の所有と classref・selref ④ W25 の仕様 3 と D3 ⑤ `host_call`。
- 最初に書くテスト: `tests/objc_send.rs` の `parses_members_and_categories` と `lowers_with_exact_c_types`（`--emit llvm --target aarch64-apple-darwin` の `call` の型を手で書いた期待と比べる）→ `tests/objc_send_host.m` の dealloc を数えるクラス。
- 触ってはいけない: `extern def`・`extern type`・コールバック・`export def` の型・ABI・header・診断（`extern def` の `bool` は `int32_t` のまま）。利用者の `extern "objc_msgSend"` に対する W14 の規則。送信を使わないプログラムの IR・object・header・診断・既定の `.wasm`（1 byte も変えない）。
- 完了の見分け方（Phase 1）: 次のプログラムを arm64 の Mac で `tsuzuri build <dir> -o probe --link $SDK/System/Library/Frameworks/Foundation.framework/Foundation.tbd`（[M20](_completed/M20-macos-framework-link-inputs.md) があれば `--framework Foundation`）で作ると、`-O0`・`-O3` で `2.5 2.5 true 5 true` を出して 0 で終わる。IR の `declare ptr @objc_msgSend(ptr, ptr, ...)` は 1 行で、`call double @objc_msgSend(`・`call float @objc_msgSend(`・`call zeroext i1 @objc_msgSend(` がある。

```tsuzuri
@trusted
extern class NSObject : ObjC.Object {
    class alloc :: unit -> Self
    init :: unit -> Maybe<Self>
}

@trusted
extern class NSValue : NSObject

@trusted
extern class NSNumber : NSValue {
    class "numberWithDouble:" with_double :: f64 -> Self
    "doubleValue" double_value :: unit -> f64
    "floatValue" float_value :: unit -> f32
    "boolValue" bool_value :: unit -> bool
}

@trusted
extern class NSString : NSObject {
    "initWithBytes:length:encoding:" init_with_bytes :: ref [ubyte] -> i64u -> Maybe<Self>
    length :: unit -> i64u
    "isEqualToString:" is_equal_to_string :: ref NSString -> bool
}

def main :: unit -> i32 = \() ->
    let n = NSNumber.with_double 2.5
    let bytes = Utf8String.to_bytes u8"こんにちは"
    match NSString.init_with_bytes (NSString.alloc ()) (ref bytes) 4 with
    | None -> 1
    | Some s ->
        let expected: NSString = ObjC.literal "こんにちは"
        let d = NSNumber.double_value (ref n)
        let f = NSNumber.float_value (ref n)
        let b = NSNumber.bool_value (ref n)
        let count = NSString.length (ref s)
        let same = NSString.is_equal_to_string (ref s) (ref expected)
        do! IO.writeln $"{d} {f} {b} {count} {same}"
        0
```

## 目的

D-67 の分岐 1 の X′ は、Objective-C のために `extern` の宣言として書く型付きの形を足す。M01 はクラスの型と所有を足すが、メソッドを呼ぶ手段を持たない。今の Tsuzuri で Cocoa を呼ぶには、`objc_msgSend` を 1 つの上位形で宣言し、使わない位置を 0 で埋めるしかない。W14 が同じ symbol の宣言を同じ C の形に限るからである。この書き方は f64・f32・構造体の結果を受け取れず、`BOOL` の結果の上位の bit を捨てる計算を利用者に強い、Apple の arm64 のスタックの引数の詰め方とも合わない。どの誤りもコンパイラは検出できない。

このチケットは、メンバーの宣言から呼び出しの C の型を作り、`objc_msgSend` をその型で直接呼ぶ（clang と同じ下げ方）。Cocoa の命名規則（族）を所有の移動に下げ、retain と release を利用者に書かせない。安全性の根拠は今の `extern` と同じ「宣言が正しいこと」で、`@trusted`（E15）と権限 `ffi`（D-57）の監査の対象にする。生ポインター型・ポインター演算・`unsafe` は入れない（D-67 の X′ の範囲）。

## 着手条件と停止条件

### 着手条件

- **人間が求めない限り Phase 1 だけを実装する**（GUIDE §0）。承認が記録されるまで、該当する Phase に着手しない。
- Phase 1: D-55 の登録（`category`・メンバーの先頭の `class`・`Self`・`ObjC.literal`）と決定事項 D1〜D4 が承認されている（D-67 の分岐 1 は X′ で承認済み、2026-11-26）。承認の前は手順 1 まで。
- Phase 1: [M01](M01-objc-class-types.md) が done（`grep -nE "^\| M01 " _features/README.md`）。M01 の名前（`ObjcClassInfo`・`HandleKind::ObjcLocal`・`llvm_objc::classref`・`selref` など）を「前提とする他チケットのインターフェース」と突き合わせる。
- Phase 2: D-67 の分岐 4 は (a) で承認済み（2026-11-26）。人間が求めたのは W36 を Phase 2 まで実装することで、M02 の Phase 2 そのものではない。D14 が承認され、[W36](W36-c-layout-by-value.md) の Phase 2 が done（`grep -n "fn classify" src/llvm_byvalue.rs`）であること。W36 は今、停止条件 S1（W21 が ref の無い `@c_layout` record をポインターで渡して出荷した）に当たり、S1 の選択（A か B）は未決である。S1 で選ばれた綴りを確かめる（D14）。W36 を待たない (b) は選ばれなかった。
- 検証のホスト: arm64 の macOS と Apple clang と SDK。Rust の IR の検査はどのホストでも走り、clang の参照との突き合わせと E2E は macOS だけ。
- ベースライン: `tests/fixtures/*`・`examples/*` の `--emit llvm`（native・`--target aarch64-apple-darwin`・`--target wasm32`）、`--emit header`、`check --json` を `target/scratch/m02/base/` に保存する。

### 停止条件

- **S1** M01 の `HandleKind::ObjcLocal`・`ObjcSync` と `ObjcClassInfo` で、メンバー・`Self`・カテゴリーを表せず、`Type` に variant を足したくなった。
- **S2** `host_call` の引数の下げ方（buffer・`ref mut`・record の一時領域・コールバック・`Maybe`）を、`extern def` と送信で共有できず、複製したくなった。
- **S3** 送信を使わないプログラムの IR・header・診断・`.wasm` が、ベースラインと 1 byte でも違う。
- **S4** 参照の集合のメソッドで、このチケットの `call` の型と clang の `-fno-objc-msgsend-selector-stubs` の IR が食い違う。規則を即興で直さず、2 つの `call` を記録して報告する（W36 の S2 と同じ）。
- **S5** テストに使う実在の API が、宣言から族の規則で決まらない所有を要する（`objc_method_family` 属性など）。
- **S6** 利用者の `extern "objc_msgSend"` の `declare` を使い回すと LLVM が IR を拒む、または `-O3` で呼び出しの型が変わる。
- **S7** ブロッキングの包みで `node tests/tzroutine.mjs target/debug/tsuzuri` の結果が変わる。
- **S8** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `804b7970` で確認）

macOS 27.2.1 arm64、Apple clang 21.0.0、SDK 27.2、`target/debug/tsuzuri`。再現のソースは `/tmp/tz-m02/<名前>/Main.tz` に置き、`/tmp/tz-m02` で `T=<repo>/target/debug/tsuzuri`、`SDK=$(xcrun --show-sdk-path)` として実行した（GUIDE §0 の 6）。

### 1. 構文

`Parser::extern_declaration`（`src/parser.rs:4807` 〜）は、リンク名の後に `type`・`view def`・`def` だけを受ける。`class` は予約語（`src/lexer.rs:655`、`EDITION_KEYWORDS` は空）。`cls/Main.tz` の `$T check cls`:

```tsuzuri
@trusted
extern class NSWindow : NSResponder

def main :: unit -> i32 = \() -> 0
```

```text
cls/Main.tz:2:8: error[E0002]: expected 'def' after extern
```

本体のない `extern class` は M01 が足す（本体の `{` は M01 では `E0002` `extern_class_syntax`。M01 の D12）。メンバー・カテゴリーの綴りはどこにもない。`Self` は予約語でも文脈キーワードでもなく、型の位置では `E1004`（`unknown record, union, or type alias 'Self'`、kind `unknown_type_name`）。`std/`・`tests/fixtures/`・`examples/` に `Self` という型はない。

### 2. W14 の同一形の規則

W14 は、同じ symbol の `extern` を、下げた C の引数の数・位置ごとの分類・結果の分類がすべて一致するときだけ許す（`CSignature::mismatch` は `src/abi.rs:764-792`、`E1008` は `src/check.rs:9705-9752`、`explicit_symbols` は 9234 行）。分類は `I32`・`Word64`（`i64` と `ptr`）・`F32`・`F64`・`Void`（`AbiClass`、`src/abi.rs:690-696`）。

```tsuzuri
// w14a/Main.tz
@trusted
extern "objc_msgSend" def msg0 :: i64 -> i64 -> i64

@trusted
@count(3, none)
extern "objc_msgSend" def msg_cstr :: i64 -> i64 -> ref utf8string -> i64

def main :: unit -> i32 = \() -> 0
```

```text
w14a/Main.tz:6:27: error[E1008]: extern symbol 'objc_msgSend' has a different C ABI here: takes 3 C parameters here but 2 in the first declaration; declarations of one symbol may differ only in types that pass the same way
```

2 つ目を `msg0_ret_f64 :: i64 -> i64 -> f64` にした `w14b` は `the result is a 64-bit float, but a 64-bit integer or pointer in the first declaration`。リンク名は C の識別子に限られ（`link_name_error`、`src/abi.rs:58-71`）、`extern "objc_msgSend$setTitle:"` は `error[E1008]: invalid extern symbol 'objc_msgSend$setTitle:'; use a C identifier of at most 255 bytes`。

### 3. 今の下げ方（`bool`・小さい整数・record）

- `c_scalar`（`src/abi.rs:794-804`）は `bool` と 8〜32 bit の整数を `CType::I32` に下げる。`host_call` は `bool`・`i8u`・`i16u` を `zext`、`i8`・`i16` を `sext` して `i32` で渡し（`src/llvm_imports.rs:285-293`）、`bool` の結果を `icmp ne i32` で戻す（442 行）。
- `ref mut bool` は 4 バイトの一時領域で渡る（231 行の `("i32", 4)`、241-243 行、401-402 行）。`boolrun` は C の `void probe_flip(bool *p) { *p = !*p; }` と `bool probe_is_visible(long long x) { return x != 0; }` を `clang -c host.c` し、`--link host.o` で呼ぶ。

```tsuzuri
@trusted
extern "probe_flip" def flip :: ref mut bool -> unit

@trusted
extern "probe_is_visible" def is_visible :: i64 -> bool

def main :: unit -> i32 = \() ->
    let mut a = false
    flip (ref mut a)
    let mut b = true
    flip (ref mut b)
    do! IO.writeln $"a={a} b={b} v0={is_visible 0} v7={is_visible 7}"
    0
```

  実行は `a=true b=false v0=false v7=true`、IR は `%v4 = alloca i32, align 4`・`call void @probe_flip(ptr %v4)`・`declare i32 @probe_is_visible(i64)`。動くのは、リトルエンディアンで下位の 1 バイトが C の `bool` に重なり、上位の 3 バイトが呼ぶ前の `zext` で 0 だからで、C の型は `int32_t *` のまま `BOOL *`（1 バイト）と違う。逆向き（[W26](W26-com-objects-in-tsuzuri.md) の Phase 1 の後に、コールバックが受けた `BOOL *` へ同じ下げ方で 4 バイトを書く場合）は隣の 3 バイトを壊す（ABI からの推論。今のコールバックは `ref mut bool` を受けられず、`extern "probe_each" def each :: (ref mut bool -> unit) -> unit` は `error[E1008]: extern callback parameters support functions over scalar, unit and extern handle types; buffers, records and nested functions are not supported`）。
- `c_signature`（`src/abi.rs:806-868`）は record の結果を先頭の out ポインターに（814 行）、ref の無い record の引数をポインターに下げる（853 行）。`@c_layout` も同じ。`rect`:

```tsuzuri
@c_layout
record CGRect { x: f64, y: f64, width: f64, height: f64 }

@trusted
extern "CGRectGetWidth" def rect_width :: CGRect -> f64

def main :: unit -> i32 = \() ->
    let r = CGRect { x: 1.0, y: 2.0, width: 3.0, height: 4.0 }
    do! IO.writeln $"w={rect_width r}"
    0
```

```text
$ $T build rect --emit llvm -o rect.ll && grep CGRectGetWidth rect.ll
  %v11 = call double @CGRectGetWidth(ptr %v2)
declare double @CGRectGetWidth(ptr)
$ $T build rect -o rect.bin --link $SDK/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics.tbd && ./rect.bin
w=2.13394945054166e+233
```

16 バイトを超える HFA でない構造体（48 バイトの `CGAffineTransform`）は AAPCS64 の「複写へのポインター」と偶然同じ形で、今も正しく渡る（調査 ffi-calls の検証）。

### 4. 今の回避策

`upper`（調査の probe e3b）は `objc_msgSend` を `i64` 6 個と `f64` 4 個の 1 つの形で宣言し、f64 の結果は別の symbol の `method_invoke`（SDK の `objc/message.h:257`、古い原型の分岐では 267 行）を f64 の結果で宣言して受ける。

```tsuzuri
@trusted
@count(1, none)
extern "objc_getClass" def objc_get_class :: ref utf8string -> i64

@trusted
@count(1, none)
extern "sel_registerName" def sel :: ref utf8string -> i64

@trusted
extern "objc_msgSend" def send :: i64 -> i64 -> i64 -> i64 -> i64 -> i64 -> f64 -> f64 -> f64 -> f64 -> i64

@trusted
@count(3, none)
extern "objc_msgSend" def send_cstr :: i64 -> i64 -> ref utf8string -> i64 -> i64 -> i64 -> f64 -> f64 -> f64 -> f64 -> i64

@trusted
extern "method_invoke" def invoke_f64 :: i64 -> i64 -> i64 -> i64 -> i64 -> i64 -> f64 -> f64 -> f64 -> f64 -> f64

@trusted
extern "class_getInstanceMethod" def class_get_instance_method :: i64 -> i64 -> i64

@trusted
extern "object_getClass" def object_get_class :: i64 -> i64

def msg0 :: i64 -> i64 -> i64 = \obj s -> send obj s 0 0 0 0 0.0 0.0 0.0 0.0

def main :: unit -> i32 = \() ->
    let ns_string = objc_get_class (ref u8"NSString\0")
    let s = send_cstr ns_string (sel (ref u8"stringWithUTF8String:\0")) (ref u8"hello, AppKit\0") 0 0 0 0.0 0.0 0.0 0.0
    let len = msg0 s (sel (ref u8"length\0"))
    let ns_number = objc_get_class (ref u8"NSNumber\0")
    let num = send ns_number (sel (ref u8"numberWithDouble:\0")) 0 0 0 0 3.25 0.0 0.0 0.0
    let int_value = msg0 num (sel (ref u8"integerValue\0"))
    let m = class_get_instance_method (object_get_class num) (sel (ref u8"doubleValue\0"))
    let back = invoke_f64 num m 0 0 0 0 0.0 0.0 0.0 0.0
    do! IO.writeln $"len={len} integerValue={int_value} doubleValue(method_invoke)={back}"
    0
```

```text
$ $T build upper -O0 -o upper0 -l objc --link $SDK/System/Library/Frameworks/Foundation.framework/Foundation.tbd && ./upper0
len=13 integerValue=3 doubleValue(method_invoke)=3.25
```

`-O3` も同じ出力。IR の宣言は W14 が正規化した 1 行 `declare i64 @objc_msgSend(i64, i64, i64, i64, i64, i64, double, double, double, double)` で、受け手もセレクターも `i64` で渡る。調査の検証では、ほかに次が動いた。C を書かない AppKit のアプリ（probe e4）は `i64` 9 個と `f64` 4 個の形で、`initWithContentRect:styleMask:backing:defer:` の NSRect を d0〜d3 に分けて渡し、`isVisible` の結果を `% 256` で切る。構造体の結果は `NSInvocation` の `getReturnValue:` で受け、`rangeValue` が `range=(5, 7)`、48 バイトの `transformStruct` が `transform=(1, 0, 0, 1, 3, 4)`。上位形はレジスターに収まる引数の間だけ成り立つ。Apple の arm64 はスタックの引数を自然な大きさで詰めるので、スタックに回る整数の引数（`self` と `_cmd` を含めて 9 個目以降）が `char`・`short` のメソッドでは配置が合わない（ABI からの推論。未実行）。

### 5. Objective-C の ABI の事実

- arm64 の macOS の `BOOL` は `bool`（SDK の `objc/objc.h:68-87`）。`clang -x objective-c -target arm64-apple-macosx12.0 -dM -E /dev/null` は `__OBJC_BOOL_IS_BOOL 1`、x86_64 は `0`（`signed char`）。
- `objc/message.h:52-53`: `objc_msgSend` は呼ぶ前に適切な関数ポインターの型にキャストしなければならない（引数のレジスターを保ったまま IMP へ飛ぶ）。
- `/usr/lib/libobjc.A.dylib` は `objc_msgSend`・`method_invoke` を libobjcMsgSend から re-export し、`objc_retainAutoreleasedReturnValue`・`objc_alloc`・`objc_alloc_init`・`objc_opt_new` を export する（`dyld_info -exports`）。`objc_retainAutoreleasedReturnValue` は SDK のヘッダーに宣言がない。
- clang の参照（`t.m` を `xcrun clang -target arm64-apple-macosx12.0 -fobjc-arc -fno-objc-msgsend-selector-stubs -O1 -S -emit-llvm`）。

```objc
#import <AppKit/AppKit.h>
@interface Del : NSObject
@end
@implementation Del
- (NSSize)intrinsicContentSize { return NSMakeSize(1,2); }
- (NSRange)selectedRange { return NSMakeRange(1,2); }
- (void)drawRect:(NSRect)r { (void)r; }
@end
double f(NSWindow *w, NSString *s, BOOL *stop) {
  [w setTitle:s];
  NSRect fr = [w frame];
  CGFloat sc = [w backingScaleFactor];
  BOOL v = [w isVisible];
  *stop = v;
  NSWindow *x = [[NSWindow alloc] initWithContentRect:fr styleMask:15 backing:2 defer:NO];
  NSString *t = [w title];
  CGAffineTransform at = [[NSAffineTransform transform] transformStruct].m11 ? CGAffineTransformIdentity : CGAffineTransformIdentity;
  (void)x; (void)t; (void)at;
  return fr.size.width + sc;
}
```

```llvm
%5 = load ptr, ptr @OBJC_SELECTOR_REFERENCES_, align 8, !invariant.load !13
tail call void @objc_msgSend(ptr noundef %0, ptr noundef %5, ptr noundef %1)
%8 = tail call %struct.CGRect @objc_msgSend(ptr noundef %0, ptr noundef %7)
%16 = tail call double @objc_msgSend(ptr noundef %0, ptr noundef %15)
%18 = tail call zeroext i1 @objc_msgSend(ptr noundef %0, ptr noundef %17)
%21 = tail call ptr @objc_alloc(ptr %20)
%27 = tail call ptr @objc_msgSend(ptr noundef %21, ptr noundef %22, [4 x double] %26, i64 noundef 15, i64 noundef 2, i1 noundef zeroext false)
%29 = tail call ptr @objc_msgSend(ptr noundef %0, ptr noundef %28) [ "clang.arc.attachedcall"(ptr @llvm.objc.retainAutoreleasedReturnValue) ]
call void @objc_msgSend(ptr dead_on_unwind nonnull writable sret(%struct.NSAffineTransformStruct) align 8 %4, ptr noundef nonnull %32, ptr noundef %35)
declare ptr @objc_msgSend(ptr, ptr, ...) local_unnamed_addr #3   ; #3 = { nonlazybind }
```

  - selref は `internal externally_initialized global`、`section "__DATA,__objc_selrefs,literal_pointers,no_dead_strip"`。`externally_initialized` があるので LLVM は load をメソッド名の文字列に畳まない。
  - `declare` は可変長の 1 行で、地点ごとに型を変えて `call` する。`sret` の送信の前には、受け手が nil なら結果を `memset` で 0 にする分岐がある。
  - `-O0` では、送信の直後に `call void asm sideeffect "mov\09fp, fp\09\09// marker for objc_retainAutoreleaseReturnValue", ""()` と `llvm.objc.retainAutoreleasedReturnValue` を置き、アセンブリでは `blr x8` の直後に `mov x29, x29` と `bl _objc_retainAutoreleasedReturnValue` が並ぶ。
  - 既定（stubs あり）では 8 か所の送信が `@"objc_msgSend$setTitle:"(ptr, ptr, ...)` などのスタブを呼び、スタブごとに可変長の `declare` が 1 行ある（`[NSWindow alloc]` は `objc_alloc` のまま）。
  - 呼ばれる側: `intrinsicContentSize` は `%struct.CGSize`、`selectedRange` は `[2 x i64]` を返し、`drawRect:` は `[4 x double]` を受ける。
- 小さい整数（`ints.m`、同じ条件）: `takeChar:short:uchar:ushort:flag:f:`（`char`・`short`・`unsigned char`・`unsigned short`・`BOOL`・`float`）の送信は `i8 noundef signext 1, i16 noundef signext 2, i8 noundef zeroext 3, i16 noundef zeroext 4, i1 noundef zeroext true, float noundef 1.500000e+00`。結果は `signext i8`・`zeroext i16`・`zeroext i1`・`float`。

### 6. 族の規則（clang の実測）

`fam.m`（`-fobjc-arc -O1`）で、`new2Thing`・`copy_x`・`_copyBar`・`mutableCopyZ` は +1（`retainAutoreleasedReturnValue` なし）、`copyfoo`・`newer` は +0（あり）だった。最初のセレクターの部分から先頭の `_` をすべて除き、族の語の次の文字が小文字でなければ（大文字・数字・`_`・終わり）族に入る。

### 7. 定数の NSString

`cf.m`・`cf2.m`（`-fobjc-arc -O1`）で、clang は `@"..."` を `{ ptr @__CFConstantStringClassReference, i32 flags, ptr data, i64 length }`（`private global`、`section "__DATA,__cfstring"`、整列 8、属性 `"objc_arc_inert"`）に下げる。`__CFConstantStringClassReference` は `external global [0 x i32]`。

| literal | データ | flags | 長さ |
| --- | --- | --- | --- |
| `@"Hello"`・`@""` | `[N x i8]`、`__TEXT,__cstring,cstring_literals`、整列 1 | 1992 | 5・0 |
| `@"日本"`・`@"café"` | `[N x i16]`（終端の 0 を含む）、`__TEXT,__ustring`、整列 2 | 2000 | 2・4 |
| `@"a\0b"` | 途中の NUL がある ASCII も UTF-16 | 2000 | 3 |

利用者の `extern` は `__` で始まる symbol を書けない（`src/abi.rs:55`）。`extern "__CFStringMakeConstantString"` は `error[E1008]: extern symbol '__CFStringMakeConstantString' is reserved by Tsuzuri; choose a name that does not start with 'tz_', 'tsuzuri' or '__'`。

### 8. 宣言・ブロッキング・そのほか

- `host_call` は `declare` の行をモジュールで 1 つの `BTreeSet<String>`（`intrinsics`）に入れ、利用者の宣言が W14 の正規化を受けたら `module.extern_abis` の形で `declare` する（`src/llvm_imports.rs:339-373`）。
- `with_blocking_externs`（`src/llvm.rs:1840` 〜）は tzroutine を起動する native のプログラムでだけ呼ばれ（1660-1664 行）、`direct_callee`（1815-1820 行。`call ` の後の最初の `@` から `(` まで）が `extern_symbols`（1801-1812 行。本体が `HostCall` の extern だけで、`extern view` を除く）の symbol を呼ぶ行を `tsuzuri_tz_blocking_enter`・`_exit` で包む。集合が空なら何もしない。`tsuzuri_tz_blocking_enter`（`src/runtime/tzroutine.c:2125-2135`）は `m == NULL || m->curg == NULL`（擬似 G の main）なら何もしない。
- `!invariant.load` の前例: `src/llvm.rs:594` の `", !invariant.load !{}"`。
- WASM の検査は `validate_wasm_externs`（`src/llvm.rs:4165` 〜）、kind `abi_native_only`（`src/diagnostic_kinds.rs:286`）。`abi_record_by_value_target`・`abi_apple_only` は登録簿にない。
- `--backend fast`（既定は `llvm`）の翻訳器は、可変長と宣言された関数の呼び出しを拒む（`src/backend/call.rs:102-110`）。可変長引数は言語の外（`docs/language.md:446`）。
- `ZString.literal` の引数の規則は `validate_zstring_literals`（`src/check.rs:10619` 〜）、kind `zstring_literal_argument`（`E1005`）。
- [W36](W36-c-layout-by-value.md) は todo。人間が求めない限り Phase 1 だけ（W36:7）、Phase 2 が AAPCS64 の macOS を含み（W36:19・161-164。macOS は `alignstack` なし）、Phase の外の macOS は `E1008` `abi_record_by_value_target`（W36:171）、`llvm_byvalue::{ByValue, classify, classify_call}` と `FunctionEmitter::{by_value_slot, by_value_to_abi, by_value_from_abi}` を提供する（W36:236）。D-67 の分岐 4 で、人間は W36 を Phase 2 まで実装することを求めた（2026-11-26）。停止条件 S1 の選択（A・B）は未決で、W21 の出荷により W36 の Phase 1 にも当たる。

## 変えてはいけない意味

- `extern def`・`extern type`・コールバック・`export def` の型・ABI・header・診断。`extern def` の `bool` は `int32_t`。
- 利用者の `extern "objc_msgSend"` に対する W14 の規則と、利用者の symbol の予約（`__`・`tz_`・`tsuzuri`）。コンパイラが生成する送信と CFString の symbol は、この検査の外。
- 送信を使わないプログラムの IR・object・header・`.wasm`・診断。メンバーの外の `Self`（今の `E1004`）。
- 評価順序（左から右。インスタンスメソッドの受け手が最初）。M01 の所有の規則。

## 仕様

### Phase の範囲

| Phase | 内容 | 規模 | 前提 |
| --- | --- | --- | --- |
| 1 | メンバーの構文、セレクターの検査、受け手と族の所有、`Self`、`extern category`、ObjC の位置の C の型（構造体の値を除く）、`objc_msgSend` の直接の呼び出し、ブロッキング、`ObjC.literal`、診断と監査 | L | M01、X′（承認済み）、D1〜D4 |
| 2 | メンバーの `@c_layout` record の値渡しと値返し | M | 分岐 4（(a) で承認済み）、D14、W36 の Phase 2 と S1 の選択（未決）。(b) の HFA だけの先行は選ばれなかった |

### Phase 1: 型付きのメッセージ送信

#### 1. 宣言

```text
class_decl  = [doc] ["@trusted"] ["private"] "extern" [RuntimeName] "class" ["sync"] Name ":" Base [ "{" { member } "}" ]
category    = [doc] ["@trusted"] ["private"] "extern" "category" Name ":" Class "{" { member } "}"
member      = [doc] { member_attr } ["class"] [Selector] name "::" Type      改行か ';' で区切る
member_attr = "@returns_retained" | "@returns_not_retained"
Selector    = 文字列の literal（"setTitle:"、"initWithContentRect:styleMask:backing:defer:"）
```

- `{` より前は M01 の綴り。M02 は本体を受理する。M01 が本体を拒むのに使った `extern_class_syntax` は、基底の欠落などに残す（kind は消さない。`docs/stability.md:18`）。本体は空でもよい。
- 先頭の `class` はクラスメソッドの印（既存の予約語の新しい位置。D2）。`category` は `extern` の直後だけの文脈キーワード（D4）で、リンク名の文字列も `sync` も取らない。カテゴリーは型ではなく名前空間。
- `Type` は関数型で、引数のないメソッドは `unit -> R`。メンバーの本体（`= …`）は M03 の `extern class export` だけ。ここでは `E0002` `extern_class_member_syntax`。
- `@returns_retained`・`@returns_not_retained`（M01）のメンバーの前の置き場を足す（置き場は M01 の D3 の承認に含まれる）。誤りは M01 の kind を使う: 両方か 2 回は `E0002` `returns_retained_conflict`、結果が物体でないメンバーは `E1008` `objc_returns_retained_not_object`、ほかの置き場は `E0002` `misplaced_returns_retained_attribute`。`@count` などはメンバーの前に置けない（既存の `misplaced_count_attribute` など。D10）。
- 上限: 1 つのクラスかカテゴリーのメンバー 4,096、セレクター 1,024 バイト。超えると `E1017`（M01 の `objc_class_limit`）。

#### 2. セレクター

- 形: 識別子 `[A-Za-z_][A-Za-z0-9_]*` 1 つ（引数なし）か、`識別子:` の 1 回以上のくり返し。空の部分（`foo::`）やほかの文字は `E1008` `objc_invalid_selector`（M01 が `ObjC.sel` の形の誤りに登録する kind。kind は 1 つのコードにしか属さないので、M02 も型検査で同じ関数 `objc::valid_selector` と同じコードを使う）。
- 省略: 引数のないメンバーだけ。名前がセレクターになる（`length :: unit -> i64u`）。引数のあるメンバーで省略すると `E1008` `objc_selector_arity`（この検査を下の数の検査より先に行う）。
- 数（D10）: `k` を `:` の数、`n` を unit を除く Tsuzuri の引数の数（受け手を数えない）、`c` を下げた C の引数の数（`self`・`_cmd` を除く。buffer はポインターと長さの 2 つ）とする。`c == k` なら通る。`n > k` は `E1008` `objc_variadic_method`（`:` を超える引数は可変長の引数でしかない）。ほかの `c != k` は `objc_selector_arity`（buffer があれば 2 つの引数になると文に書く）。
- 構造体を f64 に分けたメンバー（今の回避策）は `n > k` で `objc_variadic_method` になる。Phase 2 までは、構造体のメソッドは利用者の `extern "objc_msgSend"`（上位形。W14 の規則のまま使える）で呼ぶ。
- 連鎖の中（基底を含む）とカテゴリーの中の同じ名前は `E1001` `duplicate_objc_member`。同じセレクターに別の名前を付けるのは許す。

#### 3. 呼び出しの形と名前の解決

- インスタンスメソッド `m :: P1 -> … -> R` は `C.m :: ref C -> P1 -> … -> R`（`m :: unit -> R` は `ref C -> R`。W25 と同じ）。クラスメソッドは受け手を取らない（`class alloc :: unit -> Self` は `C.alloc :: unit -> C`）。カテゴリーは `Cat.m` で受け手は `ref Class`。
- 名前の解決は W25 の D3 と同じ（`Checker::case_path`、`src/check.rs:7829`）: 宣言したモジュールでは `C.m`、ほかからは `Module.C.m`、`C.tz` のクラスはどこからでも `C.m`。
- 基底のメンバーは派生の名前で呼べる（`NSWindow.alloc ()`）。`Self` と受け手の型は呼び出しに使った型、クラスメソッドの受け手は派生のクラス。基底への暗黙の変換はしない（M01 の `ObjC.upcast`・`upcast_owned`）。
- `Self`（D3）: メンバーの結果の `Self`・`Maybe<Self>` だけ。呼び出しに使った型名に置き換える（instancetype。カテゴリーでは `Class`）。メンバーの中のほかの位置（`ref Self`・`Vec<Self>`・引数）は `E0002` `objc_self_position`。メンバーの外は今のまま。
- 呼び出しは、すべての引数を与えた直接の呼び出しだけ。関数値と部分適用は `E1008` `objc_method_not_called`（D17）。

#### 4. 受け手と所有

- 族（D8）: 最初のセレクターの部分から先頭の `_` を除き、`alloc`・`copy`・`mutableCopy`・`new`・`init` で始まり次の文字が小文字でなければ、その族（「現状」6）。族は結果が物体の型（`C`・`Maybe<C>`・`Self`・`Maybe<Self>`）のときだけ効き、`init` はインスタンスメソッドに限る（clang の `ObjCMethodDecl::getMethodFamily` と同じ）。

| メンバー | 結果 | 下げ方 |
| --- | --- | --- |
| alloc・copy・mutableCopy・new の族、`@returns_retained` | +1 | そのまま所有する |
| init の族 | +1 | 受け手を消費し、結果を所有する |
| 族の外、`@returns_not_retained` | +0 | M01 の D6 と同じ形で retain する（送信の `call` に `clang.arc.attachedcall` の bundle を付け、直後に `objc_retainAutoreleasedReturnValue` が走る） |

- `@returns_*` は結果の所有だけを変え、init の受け手の消費は変えない。
- 受け手（D9）: インスタンスメソッドは `ref C`（+0）。init の族だけは所有する `C` を消費する（`C.init :: C -> Maybe<C>`）。送信の後に受け手を drop しない。失敗した init の nil では、受け手は呼ばれた側が release 済みなので release しない。成功した init が別の物体を返す場合（クラスクラスター）も正しい。
- `Maybe` でない物体の結果の nil は M01 の規則で `null extern handle` のトラップ。`Maybe<…>` は `None`。
- 受け手は型で非 nil。clang の `sret` の前の 0 埋め（Phase 2）は要らない。クラスの受け手の classref が NULL になりうるのは [M17](_completed/M17-macos-min-version-and-weak-import.md) の `@weak` のクラスだけで、M17 が検査する。
- 引数の物体は `ref C`・`Maybe<ref C>`（+0）。所有する `C` の引数は M01 の `E1008` `objc_owned_object_argument`（init の受け手を除く）。

#### 5. ObjC の位置の C の型（D7）

| Tsuzuri | 引数 | 結果 |
| --- | --- | --- |
| `bool` | `i1 zeroext` | `zeroext i1` |
| `i8`・`i16` | `i8 signext`・`i16 signext` | `signext i8`・`signext i16` |
| `i8u`・`i16u` | `i8 zeroext`・`i16 zeroext` | `zeroext i8`・`zeroext i16` |
| `i32`・`i32u`・`i64`・`i64u`・`f32`・`f64` | `i32`・`i64`・`float`・`double` | 同じ |
| `unit` | 引数にならない | `void` |
| `ref C`・`Maybe<ref C>` | `ptr`（nil は `None`） | 書けない |
| `C`・`Maybe<C>`・`Self`・`Maybe<Self>` | init の受け手だけ | `ptr`（仕様 4） |
| `ObjC.Sel`・`ObjC.Class` | `ptr` | `ptr` |
| `ref mut bool` | 1 バイトの一時領域への `ptr`。戻った後に非ゼロを `true` にする | 書けない |
| `ref mut Maybe<C>`（[M23](M23-objc-out-parameters-and-weak-references.md) の Phase 1 の後） | 8 バイトの一時領域への `ptr`（C の引数 1 つで、セレクターの数でも 1 つ）。戻った後に `objc_storeStrong` で場所へ書き戻す（`extern def` と同じ。M23 の仕様 2・3） | 書けない |
| `extern def` の引数と同じ形（ハンドル・その `ref`・`Maybe<H>`・`ref mut T`・`ref R`・`ref mut R`・buffer・コールバック・`FnPtr`） | `extern def` と同じ（`host_call` を共有。スカラーの幅は上の行に従う） | ハンドルと `Maybe<H>` は `extern def` と同じ |
| `@c_layout` record `R`（ref なし） | Phase 2。Phase 1 は `E1008` `abi_record_by_value_target` | 同じ |

- buffer（`ref [T]`・`ref mut [T]`・`ref string`・`ref utf8string`）はいつもポインターと長さ（要素の数、`i64`）の 2 つの C の引数で渡る。
- 印のない record の値の引数、buffer と record の結果は `E1008` `objc_member_type`（黙ってポインターや out 引数にしない）。
- `ObjC.Class` の値は、`NSObject` に宣言する `+class` のクラスメソッドで得る（D-67 の共通の綴り。M01 は型だけを足す）: `class "class" class_object :: unit -> ObjC.Class`。メンバーの名前 `class` は予約語なので、Tsuzuri の名前を別に付けてセレクターを文字列で書く。`ObjC.Class` は Copy で所有しないので、族の規則も retain も効かない。

#### 6. 下げ方

- セレクター: `load ptr, ptr @tz.objc.selref.<n>, !invariant.load !{}`（selref の大域は M01 が `externally_initialized` で出す）。クラスの受け手: 呼び出しに使った型の classref（M01 の `@tz.objc.classref.<実行時名>`。M03 の export のクラスは M03 の大域 `@tz.objc.class.<実行時名>` を読むフック）。
- 呼び出し: `call <R> @objc_msgSend(ptr %recv, ptr %sel, <args>)`。型は仕様 5。可変長の型では呼ばない（Apple の arm64 は可変長の引数をスタックに置く）。
- 宣言（D11）: `declare ptr @objc_msgSend(ptr, ptr, ...)` を 1 行。モジュールの生成の最後に `intrinsics` を見て、利用者の `extern "objc_msgSend"` の `declare` があればそれを使い回す。呼び出しはどちらでも自分の関数型。+0 の結果の bundle の被演算子 `declare ptr @llvm.objc.retainAutoreleasedReturnValue(ptr)` も M01 と共有する（M01 の D6）。

```llvm
; NSNumber.with_double 2.5（クラスメソッド、族の外: +0。M01 の D6 の形）
%cls = load ptr, ptr @tz.objc.classref.NSNumber
%sel = load ptr, ptr @tz.objc.selref.1, !invariant.load !{}
%obj = call ptr @objc_msgSend(ptr %cls, ptr %sel, double 2.500000e+00) [ "clang.arc.attachedcall"(ptr @llvm.objc.retainAutoreleasedReturnValue) ]   ; 結果は retain 済み。その後に NULL の検査（M01）
; NSString.init_with_bytes (NSString.alloc ()) (ref bytes) 4
%a = call ptr @objc_msgSend(ptr %cls2, ptr %sel_alloc)             ; +1
%s = call ptr @objc_msgSend(ptr %a, ptr %sel_init, ptr %p, i64 %len, i64 4)   ; %a は drop しない
; NSWindow.is_visible (ref w)
%v = call zeroext i1 @objc_msgSend(ptr %recv, ptr %sel2)
```

- 速い経路（`objc_alloc` など、`mov fp, fp` の印、`nonlazybind`）は使わない（D6）。+0 の結果の `clang.arc.attachedcall` の bundle は速い経路ではなく、M01 の D6 の受け取り方そのものである。bundle でハンドシェイクが効いても、呼ばれた側の中で autorelease された物体は pool に入るので、tzroutine では [M10](M10-objc-autorelease-pools.md) の pool が要る。スタブ `objc_msgSend$<sel>` は使わない（D5）。

#### 7. 定数の NSString（`ObjC.literal`、D13）

- `ObjC.literal "text"`（M01 の組み込みのモジュール `ObjC`）の引数は補間のない文字列の literal 1 つ。ほかは `E1005` `objc_literal_argument`（`zstring_literal_argument` と同じ形）。関数値にすると M01 の `E1008` `objc_builtin_not_called`。
- 結果の型は期待型: 実行時名 `NSString` のクラス、その基底の連鎖のクラス（`NSObject` など）、`ObjC.Object`。ほか（`NSMutableString` などの派生を含む）は `E1003` `objc_literal_type`。決まらなければ `E1015` `ambiguous_polymorphic_use`。
- 下げ方は clang と同じ（「現状」7）: ASCII で NUL がなければ `__cstring`・flags 1992・長さはバイト数、ほかは UTF-16 の `__ustring`・flags 2000・長さは単位の数。名前は `@tz.objc.cfstring.<n>`・`@tz.objc.cfstring.data.<n>`、同じ文字列は 1 つ、番号は最初に使った順（M01 の selref と同じ。emitter は 1 本で決定的）。
- 値は所有値で、drop は M01 の `objc_release`（定数の release は実行時に何もしない。drop の効果と権限は M01 の規則のまま）。`ObjC.literal` の評価自体は効果も権限も持たない。CoreFoundation のリンクが要る（Foundation の `.tbd` は CoreFoundation と libobjc を re-export する）。

#### 8. target・ブロッキング・効果と監査

- target（D15）: `check` は受け付ける。`build` で到達した送信と `ObjC.literal` は、wasm で `E1008` `abi_native_only`、Apple でない native で `E1008` `abi_apple_only`（M01 の kind。最初に出荷したチケットが登録する）。`--backend fast` は既存の `E2002` `fast_backend_unsupported_construct`。
- ブロッキング（D12）: tzroutine を起動する native のプログラムでは、送信を `tsuzuri_tz_blocking_enter`・`_exit` で包む（`runModal` の間に P を手放す）。`extern_symbols` は送信があれば `objc_msgSend` を含める。retain と release は包まない（ただし利用者が `extern "objc_release"` などを宣言していれば、名前で包む今の規則でコンパイラの呼び出しも包まれる。意味は変わらず費用だけが増える）。M09 の Phase 2 の後は `main` の型への送信を包まない。費用を測って objective-c.md に書く。
- 効果・権限: 送信は効果 `extern` と権限 `ffi`。`@trusted` のない `extern class`・`extern category` のメンバーを `extern` でない関数が使うと `W1010`。
- 監査: クラスの行（M01）の `used` にメンバーの使用を数える。カテゴリーは 1 行（`name` はカテゴリー名、`symbol` はクラスの `OBJC_CLASS_$_<実行時名>`）。`tz-audit/1` のキーは変えない。
- D-59（D16）: 受け手を先に置くのは W25 と同じで、利用者が宣言する `extern` のメンバーなので D-59 の対象の外。

#### 9. 診断

新しいコードは足さず、kind を `src/diagnostic_kinds.rs` に登録する（D18）。

| 状況 | コード | kind |
| --- | --- | --- |
| メンバーの形（`::` がない、型引数、本体、関数型でない） | `E0002` | `extern_class_member_syntax` |
| カテゴリーの見出し（リンク名、`: Class` がない、`sync`） | `E0002` | `extern_category_syntax` |
| メンバーの引数・入れ子の `Self` | `E0002` | `objc_self_position` |
| セレクターの形 | `E1008` | `objc_invalid_selector`（M01 と共有） |
| メンバーの前の所有の属性の重複・物体でない結果 | `E0002`・`E1008` | `returns_retained_conflict`・`objc_returns_retained_not_object`（M01） |
| 同じ名前のメンバー | `E1001` | `duplicate_objc_member` |
| `ObjC.literal` の期待型・引数 | `E1003`・`E1005` | `objc_literal_type`・`objc_literal_argument` |
| `:` の数・`:` を超える引数 | `E1008` | `objc_selector_arity`・`objc_variadic_method` |
| 関数値・送れない型 | `E1008` | `objc_method_not_called`・`objc_member_type` |
| `ObjC.literal` の関数値 | `E1008` | `objc_builtin_not_called`（M01） |
| `@c_layout` の値（Phase 1） | `E1008` | `abi_record_by_value_target`（W36 と共有。先に出荷したほうが登録） |
| wasm・Apple でない native | `E1008` | `abi_native_only`（既存）・`abi_apple_only`（M01） |

### Phase 2: 構造体の値渡しと値返し

- メンバーの `@c_layout` record `R`（ref なし）を C の値渡しと値返しにする。分類は W36 の Phase 2 の `classify`（AAPCS64、macOS は `alignstack` なし）で、M02 は分類を持たない（D14）。
- HFA（同じ浮動小数点の葉 1〜4 個）の引数は `[n x double|float]`、結果は `{ double, … }`。HFA でない record は、`s ≤ 8` なら引数 `i64`・結果 `i{8s}`、`s ≤ 16` なら引数も結果も `[2 x i64]`（W36:156 の Arm64 と同じ形）。大きい引数は複写への `ptr`、結果は先頭の `ptr sret(%tz_record_…) align 8`（x8。受け手は x0）。arm64 に `_stret` はない（x86_64 は M18）。
- 代表: NSPoint・NSSize（HFA 2）、NSRect・NSEdgeInsets（HFA 4）、NSRange（`[2 x i64]`）、CGAffineTransform・NSAffineTransformStruct（48、引数 `ptr`・結果 `sret`）。nil の受け手はないので `sret` の前の 0 埋めは出さない。d0〜d7 に収まらない HFA は W36 の `classify_call` でスタックに置く。
- 分岐 4 の (b)（選ばれなかった。分岐 4 は 2026-11-26 に (a) で承認済み。記録として残す）: M02 が HFA だけを W36 の `classify` と同じ入出力の一時の関数で扱い、HFA でない record は `abi_record_by_value_target` のまま。W36 の Phase 2 の後に置き換える。
- Phase 2 の後は仕様 2 の上位形の注記を消し、`initWithContentRect:styleMask:backing:defer:`・`frame` を型付きで書ける。

## 設計

### 構文と型検査

- `src/syntax.rs`: M01 の `ExternClassDecl` に `members: Vec<ObjcMember>`、`ObjcMember { doc, returns: Option<(Ownership, Span)>, class_method: Option<Span>, selector: Option<(Box<str>, Span)>, name: Ident, ty: TypeExpr }`、`ExternCategoryDecl { doc, visibility, name, class: TypeExpr, members }` と `Program.extern_categories`。大きい field は `Box`（GUIDE-LITE §10）。
- `src/parser.rs`: M01 の `class` の分岐の後に `{` を読み、`instance` の本体と同じく改行か `;` で区切る。`Ident("category")` は W24 の `view` と同じ分岐。`Self` の位置はここで検査する（セレクターの形は型検査。仕様 2）。`src/syntax_codec.rs`・`src/frontend_cache.rs`（`FRONTEND_FORMAT` は着手時の値 + 1。HEAD は 27）と `grep -rn extern_types src` の各所を直す。`api-diff` はメンバーの削除・型・セレクター・属性の変更を互換のない変更、追加を互換とする。
- `src/check.rs`: M01 の `ObjcClassInfo` に `members: Vec<ObjcMemberInfo>` を足す。`ObjcMemberInfo { name, selector, class_method, family, result_ownership, parameters, result, declared_in, span }`、カテゴリーの表。連鎖を平らにした名前の表は基底の解決の後で 1 度だけ作る。族とセレクターの純粋な関数は `src/objc.rs`（`method_family`・`valid_selector`・`selector_arity`。単体テストを持つ）。M01 が `ObjC.sel` の形の検査を別の所に置いていれば、`valid_selector` に寄せて 1 つにする。
- 型付き IR: `TypedExprKind::ObjcSend(Box<ObjcSend>, Vec<TypedExpr>)`、`ObjcSend { selector, receiver: Receiver::{Instance, Class(実行時名)}, signature, ownership, result_nullable }`。`Self` は呼び出しの地点で置き換えてから作る。`TypedExprKind` を `match` する所（`src/check.rs:5805`・`5950` 付近など）をすべて直す。
- `ObjC.literal` は `Builtin::ObjcLiteral`。`Builtin::ALL`・`name`・`signature`（期待型から決める）・`emit_builtin`・`builtin_effect`（`src/pure_rules.rs:184`・`src/kg/effects.rs:18`）・`builtin_impure`・`src/capabilities.rs`・`pinned_rules::builtin_position`（`src/pinned_rules.rs:33`）を更新し、引数の検査は `validate_zstring_literals` の走査に足す。
- `src/abi.rs`: `objc_parameter`・`objc_result`・`objc_signature(parameters, result, types)`（仕様 5 の型と、セレクターの数の `c`）。`c_scalar`・`c_signature` は変えない。

### 生成

- `host_call` を、引数の下げ方の共通の部分と、呼び先と宣言の部分に分ける（S2）。スカラーの幅だけを `HostAbi::C`（今の `i32` の正規化）と `HostAbi::ObjcMember`（仕様 5）で分ける。W25 も同じ分割を要るので、先に入ったほうが分ける。既存の extern の IR は変わらない（S3）。
- `src/llvm_objc.rs` の `FunctionEmitter::objc_send`: selref と classref の load、引数、`call`、NULL の検査と retain、init の受け手の移動。`call` を出す所は 1 か所（`ObjcCallSite`）にし、M05 が `invoke` に替えられるようにする。
- 宣言は `emit_program` の最後に `intrinsics` を走査して決める。CFString は `BTreeMap<Vec<u16>, usize>` に集め、関数の後で出す。`extern_symbols` は送信を含むモジュールなら `objc_msgSend` を足す（`with_blocking_externs` は変えない）。driver の変更はない（`-lobjc` は M01、framework は M20）。

### Phase 2: 値渡しの下げ方

- `objc_signature` の `@c_layout` の値を W36 の `classify(layout, platform, result)` に置き換え、W36 の `by_value_slot`・`by_value_to_abi`・`by_value_from_abi` で生成する。`Memory` の結果は `sret` を最初の IR の引数にする。(b) では `classify_hfa(layout) -> Option<ByValue>` を一時に置く案だった（(b) は選ばれなかったので置かない）。

## 前提とする他チケットのインターフェース

- [M01](M01-objc-class-types.md): 本体のない `extern class` の AST（`ExternClassDecl`）、`HandleKind::ObjcLocal`・`ObjcSync` と述語、クラスの脇の表 `ObjcClassInfo`（実行時名・基底・`sync`）、基底の連鎖の解決、組み込みのモジュール `ObjC` と `ObjC.Object`・`ObjC.Class`・`ObjC.Sel`、`llvm_objc::classref(&mut self, runtime_name)`・`selref(&mut self, selector)`（`tz.objc.selref.<n>`・`tz.objc.classref.<実行時名>`、`externally_initialized`、`@llvm.compiler.used`、モジュールのフラグ）、`-lobjc`（`llvm::uses_objc`）、drop の `objc_release`、+0 の結果の retain と nil のトラップ、`@returns_retained`・`@returns_not_retained`、`ref C`・`Maybe<ref C>` の検査、kind `abi_apple_only`・`objc_owned_object_argument`・`objc_object_position`・`objc_class_limit`・`misplaced_returns_retained_attribute`・`returns_retained_conflict`・`objc_returns_retained_not_object`・`objc_invalid_selector`・`objc_builtin_not_called`・`extern_class_syntax`、監査の行。名前が違えば止めて報告する（GUIDE-LITE §11）。
- [W14](_completed/W14-extern-multiple-declarations.md): `module.extern_abis` と `intrinsics`。[W13](_completed/W13-extern-ref-mut-and-mutable-buffers.md)・[W15](_completed/W15-abi-nullable-handles.md)・[W18](_completed/W18-copy-extern-type.md)・[W21](_completed/W21-c-layout-records.md): `host_call` が下げる形（共有する）。
- [W36](W36-c-layout-by-value.md)（Phase 2）: `llvm_byvalue::{ByValue, classify, classify_call}`、`FunctionEmitter::{by_value_slot, by_value_to_abi, by_value_from_abi}`、`abi::extern_record_passing`、kind `abi_record_by_value_target`。
- [M20](_completed/M20-macos-framework-link-inputs.md)（あれば）: `--framework`。無ければテストは SDK の `.tbd` を `--link` で渡す。

## 他チケットへ提供するインターフェース

- [M03](M03-objc-classes-defined-in-tsuzuri.md): `ObjcMember`・`ObjcMemberInfo`（`= impl` は M03 が足す）、`objc::{method_family, valid_selector, selector_arity}`、`abi::{objc_parameter, objc_result, objc_signature}`（IMP の型の検査）、`Self` の置き換え、`objc_send` の引数の部分（`Name.Super.m` は呼び先だけが違う）、クラスの受け手のフック（`@tz.objc.class.<実行時名>`）。
- [M05](M05-objc-exception-boundary.md): `call` を出す 1 か所（`ObjcCallSite`）。`Result<R, ObjC.Exception>` のメンバーをそこで `invoke` にする。
- [M23](M23-objc-out-parameters-and-weak-references.md): `abi::objc_parameter`（`ref mut Maybe<C>` を `ptr` 1 つにする）、セレクターの数の `c`（同じく 1 つ）、`host_call` の分割（引数の共通の部分が物体の出力引数の写し込みと書き戻しを持つ）、`ObjcCallSite`。
- [M07](M07-apple-sdk-bindgen-and-package.md): 宣言の文法（`[class] "sel" name :: T`、`extern category`、`@returns_*`、`Self`）、同じ名前は `E1001` なので生成器が名前を変える、buffer は 2 つの C の引数、可変長は書けない、Phase 2 の前は構造体のメソッドを飛ばす。
- [M04](M04-objc-blocks.md)（弱い）: `objc_parameter` に `ref ObjC.Block<F>`・`Maybe<ref ObjC.Block<F>>` を足す口。[M10](M10-objc-autorelease-pools.md)（弱い）: +0 の結果は M01 の D6 の bundle で retain する（返された物体はハンドシェイクが効けば pool を経ないが、呼ばれた側の中で autorelease された物体は pool に入る）。[M12](M12-gpu-presentation-to-window.md)（弱い）: CAMetalLayer への送信。[M18](M18-intel-mac-and-universal2.md)（弱い）: 仕様 5 の表を target ごとの表にし、x86_64 の行（`BOOL` は `i8 signext`、`objc_msgSend_stret`）を足せる形にする。
- [M09](M09-main-thread-dispatch-and-handles.md) の Phase 2: `main` の型への送信の包みを省く印。[M17](_completed/M17-macos-min-version-and-weak-import.md): クラスの受け手の classref の load に NULL の検査を差し込む所。

## 実装手順

### Phase 1

1. **変更**: なし（ブランチ `feat/m02-objc-typed-message-send`）。**内容**: `node scripts/repo-kb.mjs brief M02`、着手条件、ベースライン。`tests/objc_send_reference.m`（`tests/objc_send_host.m` のメソッドを同じ型で呼ぶ ObjC の関数）の clang の IR を `target/scratch/m02/reference/` に保存する。**確認**: 件数を控え、「現状」5 と食い違えば記録する。
2. **変更**: `src/syntax.rs`・`src/parser.rs`・`src/syntax_codec.rs`・`src/frontend_cache.rs`・`src/formatter.rs` ほか・`src/diagnostic_kinds.rs`。**内容**: 仕様 1・2 の構文と `E0002` の kind。**確認**: `tests/objc_send.rs` の構文の検査、`cargo test --locked --test formatter`、`pins_encoding_format`。
3. **変更**: `src/check.rs`・`src/abi.rs`・`src/objc.rs`。**内容**: メンバーの表、名前の解決、`Self`、族と受け手、セレクターの数、仕様 5、`objc_method_not_called`、`ObjC.literal` の型。**確認**: 型の検査、族の単体テスト、`cargo nextest run --locked`。
4. **変更**: `src/llvm_imports.rs`・`src/llvm_objc.rs`・`src/llvm.rs`。**内容**: `host_call` の分割、送信、宣言、所有、CFString。**確認**: ベースラインと `cmp`（S3）、IR の検査、2 回の `emit` が同じ、macOS で参照と突き合わせ（S4）。
5. **変更**: `src/llvm.rs`（`extern_symbols`・`validate_wasm_externs`）と M01 の Apple の検査。**内容**: ブロッキングと target。**確認**: `node tests/tzroutine.mjs target/debug/tsuzuri` が変わらない（S7）、target の検査。
6. **変更**: `src/effects.rs`・`src/pure_rules.rs`・`src/kg/effects.rs`・`src/capabilities.rs`・`src/pinned_rules.rs`。**内容**: 効果・権限・監査。**確認**: `tests/capabilities.rs`・`tests/audit.rs`・`tests/trusted.rs` の case。件数の pin は現在の値 + 足した数。
7. **変更**: `tests/objc_send_host.m`・`tests/objc_send.mjs`・fixture 2 つ。**内容**: E2E。**確認**: `node tests/objc_send.mjs target/debug/tsuzuri` が `-O0`・`-O3` で通り、残ったプロセスがない。
8. **変更**: なし。**内容**: release のビルドで、包んだ送信の費用と D6 の候補を同じ仕事量と `-O3` で測り、生成コードを見る。**確認**: 結果を objective-c.md とチケットの「実装と検証」に書く（共有 CI に閾値を置かない）。
9. **変更**: 文書。**確認**: T2、`node scripts/check-docs.mjs <変更したページ>`、`node scripts/gen-docs.mjs --check`、`node scripts/check-cli-docs.mjs`。PR に `ci:full`。Apple の ld と同梱の zig（lld）の両方でリンクする。

### Phase 2（人間が求めたとき）

10. **変更**: なし。**内容**: D14 の承認（分岐 4 は (a) で承認済み）、W36 の Phase 2 と S1 の選択と綴りを確かめ、参照の `.m` に構造体のメソッドを足す。**確認**: 参照の行数を控える。
11. **変更**: `src/abi.rs`・`src/llvm_objc.rs`。**内容**: Phase 2 の仕様。**確認**: IR の検査と参照との突き合わせ（S4）。
12. **変更**: テストと文書。**内容**: Phase 2 の E2E、仕様 2 の注記の削除。**確認**: T2、`ci:full`。

## テスト計画

### Rust（`tests/objc_send.rs`、新規）

期待は ABI の文書と clang の参照から手で書く（GUIDE-LITE §2 規則 10）。

| 検査 | 期待 |
| --- | --- |
| `parses_members_and_categories` | 早見の宣言とカテゴリーが通り、`NSWindow.set_title :: ref NSWindow -> ref NSString -> unit`、`NSString.init_with_bytes :: NSString -> ref [ubyte] -> i64u -> Maybe<NSString>` |
| 構文・セレクターの形の拒否 | 引数の `Self`・`Vec<Self>`（`objc_self_position`）、本体 `= f`（`extern_class_member_syntax`）、カテゴリーのリンク名（`extern_category_syntax`）、メンバーの前の `@count`（`misplaced_count_attribute`）、両方の所有の属性（`returns_retained_conflict`）、`i64` の結果の `@returns_retained`（`objc_returns_retained_not_object`）、`foo::`・`set title:`（`E1008` `objc_invalid_selector`）。メンバーの外の `Self` は今と同じ `E1004` |
| セレクターの数 | `"setTitle:"` に 2 引数（`objc_variadic_method`）、`"a:b:"` に 1 引数と省略（`objc_selector_arity`）、`"initWithBytes:length:encoding:"` に `ref [ubyte] -> i64u`（通る）、`"stringWithUTF8String:"` に `ref utf8string`（`objc_selector_arity`） |
| 族 | `alloc`・`new2Thing`・`copy_x`・`_copyBar`・`mutableCopyZ`・`initWithFoo` は族、`copyfoo`・`newer`・`initialize`・`allocated` と結果が物体でない `newCount :: unit -> i64`、クラスメソッドの `init…` は族の外 |
| 受け手・名前・型 | init に渡した受け手の再使用は `E1012`、派生の名前の基底のメンバーと `Self`、`NSObject.init` に `NSWindow` は `E1003`、`Module.C.m`、同じ名前（`duplicate_objc_member`）、関数値（`objc_method_not_called`）、印のない record の値・buffer の結果（`objc_member_type`）、`@c_layout` の値（`abi_record_by_value_target`） |
| `lowers_with_exact_c_types` | 仕様 5 の各型の `call` の型、`ref mut bool` が `alloca i8`、selref の load の `!invariant.load !{}` |
| 所有と宣言 | +0 の結果の送信だけに M01 の D6 の bundle（`declare ptr @llvm.objc.retainAutoreleasedReturnValue(ptr)` は 1 行）、init の受け手に `objc_release` が無い、`declare … @objc_msgSend` が 1 行、利用者の上位形と併用するとその `declare` だけが残る |
| `ObjC.literal` | 「現状」7 の表の構造体・section・flags、同じ文字列が 1 つ、`NSMutableString`（`objc_literal_type`）、注釈なし（`E1015`）、補間（`objc_literal_argument`）、関数値（`objc_builtin_not_called`） |
| `matches_clang_reference` | `xcrun clang` と SDK のあるホストだけ（ほかは理由を出して飛ばす）。fixture のメンバーの `call` の型を、`tests/objc_send_reference.m` を `-fno-objc-msgsend-selector-stubs -O1` で下げた `call` と比べる（`noundef`・`tail` を除き、`ptr` と `i64` だけを同じと見なす） |
| target・不変 | `--target wasm32` は `abi_native_only`、`--target x86_64-unknown-linux-gnu` は `abi_apple_only`、`check` は 0。2 回の `emit` が同じ。送信を使わない fixture の IR が変わらない。spawn するプログラムだけ送信が包まれる |

### E2E（`tests/objc_send.mjs`、macOS arm64。`-O0`・`-O3`）

`clang -fobjc-arc -c tests/objc_send_host.m` と `tsuzuri build tests/fixtures/objc_send --link host.o --link <Foundation.tbd>`。ホストのクラス `TzProbe` は init で生きている数を増やし dealloc で減らす（`tzprobe_live()`）。pool はホストの `tzprobe_pool_push`・`_pop` で作る（M10 の前）。期待はホストの C の側で書く。

| case | 内容 | 期待 |
| --- | --- | --- |
| `bools` | `YES`・`NO` の結果、`BOOL` の引数、`BOOL *` への書き込み（`*(unsigned char *)p = 2` も） | 一致、`2` は `true` |
| `numbers` | `float`・`double`（`-0.0`・NaN の bit 列）、`char`・`short` の境界の値（`-128`・`255`・`-32768`・`65535`） | bit 単位で一致 |
| `stack_args` | 整数の引数 10 個（`self`・`_cmd` の後の 7 個目以降がスタックに回り、そこに `char`・`short` を置く）と double 9 個 | ホストが各引数を検査して 0 |
| `ownership` | +0 の結果、`newProbe`・`copy`・`mutableCopy`、`@returns_*` の上書き、nil を返す init、別の物体を返す init | pool の後に `tzprobe_live() == 0`、二重解放なし |
| `inherited`・`category` | 基底の `class make :: unit -> Self` を派生の名前で、ホストのカテゴリーのメソッド | 派生のクラス名、呼べる |
| `literal` | ASCII・日本語・空・途中の NUL をホストが `isEqualToString:` で `@"…"` と比べる。`ref (ObjC.literal "x")` の一時の借用 | すべて一致 |
| `foundation` | 早見のプログラム | `2.5 2.5 true 5 true` |
| `blocking` | spawn するプログラムで、`Routine.with_locked_thread` の中の物体に 200 ms 眠るメソッドを送る間に、ほかの tzroutine がチャンネルへ送る（`TSUZURI_MAXPROCS=1`） | 受け取る順で確かめる（時間の閾値なし） |
| `wrong_selector`・`fast_backend` | 存在しないセレクター、`--backend fast -O0`（`fast-backend` の feature で作ったバイナリだけ。既定のビルドは `E2000` `fast_backend_unavailable` なので、理由を出して飛ばす） | 今の abort（134。M05 の後も、`Result<R, ObjC.Exception>` と宣言しないメンバーでは同じ）、`E2002` |

- AppKit の段（`tests/fixtures/objc_send_appkit/Main.tz`、`perl -e 'alarm 15; exec @ARGV'`）: `NSApplication.shared ()`、`set_activation_policy` に 1（Accessory）、`NSWindow.init (NSWindow.alloc ())`、`set_released_when_closed false`、literal の `set_title`、`set_frame_from_string`、`set_alpha_value 0.0`、`order_front_regardless`、`title` の往復、`is_visible` が `true`、`backing_scale_factor` が 1 以上、`close`。run loop は回さない。終了コード 0 と、残ったプロセスがないこと。Apple の ld と同梱の zig の両方でリンクする。

### Phase 2: 構造体

- IR: NSPoint・NSRect の引数 `[2 x double]`・`[4 x double]`、結果 `{ double, double }`・`{ double, double, double, double }`、NSRange `[2 x i64]`、48 バイトの結果は先頭の `sret`、`memset` と `alignstack` が無い。参照の clang と比べる（`ptr` と `i64` だけを同じと見なす。W36 と同じ）。
- E2E: ホストのメソッドが各構造体を受けて返し、field を検査する。d0〜d7 が尽きる 3 つ目の NSRect も入れる。AppKit の段に `init_with_content_rect` と `frame`。分岐 4 は (a) なので、HFA でない record（NSRange）も値で受け渡す（(b) の `abi_record_by_value_target` のままの形は選ばれなかった）。

## ドキュメント

- `_tsuzuri/language-reference/compiler/objective-c.md`（M01 が作る）: 「メンバーとメッセージ送信」（構文、セレクター、`C.m`、派生の名前、`Self`）、「所有」（族の表、init の受け手、`@returns_*`、+0 の結果と pool、tzroutine では M10）、「BOOL と数の型」（`extern def` との違い）、「buffer と長さ」、「カテゴリー」、「定数の NSString」、「構造体」（Phase 1 は上位形、Phase 2 の後は値）、「スレッドとブロッキング」（測った費用）、「制限」（可変長、スタブ、super は M03、例外は M05、`releasedWhenClosed`）。今のコンパイラがすることだけを書く。
- `native-interop.md` の「ホスト関数を import する」から objective-c.md への参照。`attributes.md` の `@returns_*` のメンバーの置き場。`diagnostics.md` と `docs/language.md` の診断の表は `node scripts/gen-docs.mjs` で生成し、`src/explain/codes/E1008.md` を直す。`platforms.md`・`webassembly.md` の `E1008`。`argument-order.md` に D16。`docs/language.md` の「ホスト関数のインポート」（449 行〜）の小節、`docs/architecture.md` の送信・宣言・CFString。`README.md` の「変更の種類別」の host ABI の行に `tests/objc_send.rs`・`.mjs`。

## 受け入れ条件

- [ ] Phase 1 の仕様 1〜9 のとおりに動き、仕様の外の入力は表の診断で拒否される。
- [ ] 参照の集合のすべてのメソッドで、`call` の型が clang の `-fno-objc-msgsend-selector-stubs` の IR と一致する。
- [ ] 送信を使わないプログラムの IR・header・`check --json`・`.wasm` がベースラインと 1 byte も違わない。
- [ ] 生成 IR が決定的で、`objc_msgSend` の `declare` が 1 行、CFString の大域が重複しない。
- [ ] `tests/objc_send.rs` と `node tests/objc_send.mjs` が `-O0`・`-O3` で通り、`tzprobe_live() == 0` を確かめた。AppKit の段の後に残ったプロセスがない。
- [ ] ブロッキングの費用と D6 の候補を測り、文書とチケットに書いた。kind を登録し、D-55 の登録を記録した。
- [ ] T2 が通る。PR に `ci:full`。Apple の ld と同梱の zig の両方でリンクを確かめた。
- [ ] （Phase 2）構造体の値渡しと値返しが参照と一致し、E2E が通る。

## 落とし穴

- **呼び出しの型の一致**: 型が 1 つでも違えば値が壊れる（`objc/message.h:52-53`）。宣言の正しさは `@trusted` の責任で、コンパイラは検査できない。
- **`bool` の二つの ABI**: メンバーは `BOOL`（`i1 zeroext`）、`extern def` は `int32_t`。`extern def` を変えると既存の C のホストが壊れる。
- **小さい整数の幅**: Apple の arm64 はスタックの引数を自然な大きさで詰めるので、`i32` に広げるとスタックに回る引数（`self`・`_cmd` を含めて 9 個目以降の整数）の配置が変わる。
- **selref の `externally_initialized`**: ないと `-O3` で load がメソッド名の文字列に畳まれ、送信が壊れる（M01 が出す。IR の検査でも確かめる）。
- **init の受け手**: 送信の後に drop しない。nil の結果で release しない。継承した `init` を派生の名前で呼ぶと受け手の型は派生。
- **`releasedWhenClosed`**: コードで作った NSWindow は既定で `close` のときに自分を release する。所有と二重になるので、`close` の前に `setReleasedWhenClosed:` に `false` を送る。
- **+0 の結果と pool**: 素の `objc_retainAutoreleasedReturnValue` を別の `call` で置くと、呼ばれた側の autorelease を省けない（M01 の「現状」8）。送信には M01 の D6 の bundle を付ける。bundle でも、呼ばれた側の中で autorelease された物体は pool に入り、pool の無いスレッドでは終わるまで残る（M10）。
- **セレクターの省略**: 名前とセレクターが同じときだけ。`double_value :: unit -> f64` はセレクター `double_value` に送り、実行時に例外で abort する。
- **buffer の長さは要素の数**: `ref [i64]` を `initWithBytes:length:` に渡すとバイト数にならない。
- **利用者の型 `Self`**: メンバーの結果では instancetype。利用者の `record Self` は `Module.Self` と書く。
- **W36 の綴り**: S1 で `@c_layout` の値渡しが別の綴りになったら、Phase 2 のメンバーも合わせる。
- **PB07 の分割**: M01 の `@llvm.compiler.used` で -O0 の分割が外れる（`src/llvm_split.rs`）。ObjC を使うプログラムに限られることを確かめる。

## 対象外

- super への送信（M03。今のクラスの Class が要る）。クラスの定義とプロトコル（M03）。Block の引数（M04）。例外（M05）。
- セレクターのスタブ `objc_msgSend$<sel>`（リンカーの支援が要る。D5）。
- 可変長のメソッド（`stringWithFormat:` など。`E1008`）。空の部分を持つセレクター（`foo::`）。
- 軽量ジェネリクス。`objc_method_family` 属性。動的な `ObjC.Class` の値への送信。メンバーの前の `@count`。
- string と NSString の変換の関数（M07 のパッケージが Tsuzuri で書く）。
- `extern def` の `bool` の ABI（今の `int32_t` のまま）。
- x86_64 の `_stret`（M18）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | メンバーの構文は `[doc] [attrs] ["class"] [selector] name :: Type`（改行か `;`）。セレクターは引数のないメンバーでだけ省略できる。メンバーは `C.m :: ref C -> …` で、名前は W25 の D3 と同じく `C.m`・`Module.C.m` で解決する | 正確なセレクターを文字列で持ち、Tsuzuri の名前を別に付ける。W25 と同じ形。採らない案: 名前からセレクターを作る規則（逆写像が一意でなく `URLForResource:` の大文字が崩れる）、Swift の引数ラベル（新しい構文）、`@selector("…")` 属性（長く、D-53 の登録も要る）、メンバーごとの `extern "objc_msgSend"` と W14 の除外（分岐 1 で選ばれなかった Z′。受け手と所有の型の保護がない） | 要承認（承認前は手順 2 に着手しない） |
| D2 | クラスメソッドの印はメンバーの先頭の予約語 `class`（Swift の `class func`） | 新しい語が要らない。採らない案: `static`（新しい文脈キーワード）、ObjC の `+`・`-`（行頭の記号が単項の演算子と紛れる）、`class { … }` の入れ子 | 要承認（承認前は手順 2 に着手しない） |
| D3 | `Self` はメンバーの結果の `Self`・`Maybe<Self>` だけに書ける文脈キーワードで、呼び出しに使った型名に置き換える。メンバーの中のほかの位置は `E0002`、メンバーの外は今のまま | instancetype（clang と同じ）。メンバーの外を変えないので、使わないプログラムの診断は変わらない。採らない案: クラス名の明示（派生で呼ぶと基底が返る）、型引数（`E1015` と多相の機構が要る） | 要承認（承認前は手順 2 に着手しない） |
| D4 | カテゴリーは `@trusted extern category Name : Class { … }` の名前空間で、ほかのモジュールの型を開き直さない | モジュールが型を持つ規則と `api-diff` を保つ。採らない案: 2 つ目の `extern class` 宣言にメンバーを足す（同じ型の宣言の衝突）、`NSString.m` と型名で呼ぶ（型を開き直す）、`extension`（新しい語） | 要承認（承認前は手順 2 に着手しない） |
| D5 | selref と直接の `objc_msgSend` を使い、スタブ `objc_msgSend$<sel>` は採らない | スタブはリンカーの合成が要り、同梱の zig と lld で使えるか分からない。意味は同じ（大きさの最適化） | 既定案（実装者はこの案に従う） |
| D6 | 速い経路（`objc_alloc`・`objc_alloc_init`・`objc_opt_new`、`mov fp, fp` の印、`nonlazybind`）は Phase 1 で使わず、同じ意味のものを測ってから個別に入れる。+0 の結果の受け取り方は M01 の D6 に従い、送信にも `clang.arc.attachedcall` の bundle を付ける（`extern def` の結果と送信の結果で受け取り方を分けない） | 性能の主張は測定の後（AGENTS.md）。`mov fp, fp` の印は clang の -O0 と同じだが、結果を後で使うと spill で送信の隣でなくなる（M01 の「現状」8）。bundle は M01 の D6 が `extern def` の +0 の結果に決めた形で、物体が pool に残るかどうかという意味の側の理由で選ぶ（性能の主張ではない）。同じ問いを 2 つのチケットで別に決めない（AGENTS.md の「同じ意味の形が別の経路を選ばない」）。採らない案: clang の -O1 と同じ速い経路を最初から全部使う（測定のない性能の主張になる）。送信だけ素の `objc_retainAutoreleasedReturnValue` にする（`extern def` の結果と pool の振る舞いが分かれる） | 既定案（実装者はこの案に従う） |
| D7 | ObjC の位置の C の型は仕様 5 の表。`extern def` は変えない | `objc/message.h` は正確な型を求め、clang も同じ型で呼ぶ。採らない案: `extern def` の `i32` の正規化の流用（スタックの配置と `BOOL *` の幅が違う） | 既定案（実装者はこの案に従う） |
| D8 | 所有は clang の族の規則と `@returns_*` で決める（仕様 4） | ARC と同じで、Cocoa の宣言をそのまま写せる（M07）。採らない案: すべて +0（+1 が漏れる）、属性を必須（付け忘れが漏れか二重解放） | 既定案（実装者はこの案に従う） |
| D9 | init の族の受け手は所有する `C` を消費する（仕様 4） | ARC の `ns_consumes_self` と同じ。採らない案: `ref C` と送信の前の retain（retain と release が増える） | 既定案（実装者はこの案に従う） |
| D10 | buffer はいつもポインターと長さの 2 つの C の引数で渡し、セレクターの数は下げた C の引数の数と比べる（Tsuzuri の引数が `:` を超えれば `objc_variadic_method`）。`@count` はメンバーに置けない | `initWithBytes:length:` の形に長さが安全に結び付く。`@count` の置き場は D-53 の改訂が要る（W25 の D10）。`const char *` は M06 の Phase 2 の `ZUtf8String` か `initWithBytes:length:encoding:` で書ける。採らない案: Tsuzuri の引数の数と比べる（長さを別の引数に分け、領域との結び付きが失われる） | 既定案（実装者はこの案に従う） |
| D11 | `objc_msgSend` の `declare` は 1 行で、利用者の `declare` があれば使い回す（仕様 6） | LLVM は型の違う 2 つ目の `declare` を拒む（D-67 の共通の規則）。採らない案: `objc_msgSend` を予約された symbol にする（今日の手書きの宣言が壊れる）、送信ごとに別名の `declare` を出す（リンク名が C の symbol でなくなる） | 既定案（実装者はこの案に従う） |
| D12 | 送信をブロッキングの包みで囲む（仕様 8） | モーダルな送信の間に P を手放す（`extern def` と同じ）。採らない案: 囲まない（`runModal` の間ほかの tzroutine が止まる）、retain と release も囲む（費用だけが増える） | 既定案（実装者はこの案に従う） |
| D13 | `ObjC.literal` は clang の CFSTR と同じ構造体に下げ、結果の型は期待型の `NSString` とその基底の連鎖のクラスか `ObjC.Object`。引数は補間のない literal だけ | 確保も呼び出しもない。定数は変更できないので `NSMutableString` などの派生として型付けしない。採らない案: 実行時の `CFStringCreateWithCharacters`（評価ごとの確保と release）、`__CFStringMakeConstantString`（予約された `__`） | 既定案（実装者はこの案に従う） |
| D14 | Phase 2 は W36 の Phase 2 の `classify` をメンバーに使い、M02 は分類を持たない。分岐 4 の (b)（HFA だけを先に扱って後で置き換える）と、どちらも選ばれない場合（Phase 2 に着手しない）は、分岐 4 が (a) に決まったので当たらない。W36 の S1 で別の綴りが選ばれたらメンバーも合わせる | 分類は 1 か所（W36 の D4）。NSRect・NSPoint・NSSize は HFA で、AppKit の大半は (b) でも書ける。採らない案: M02 だけの AAPCS64 の分類（重複）、f64 に分けた書き方（数の検査と型の保護を失う） | 要承認（D-67 の分岐 4 は (a) で承認済み（2026-11-26）。この決定と W36 の S1 の選択の前は Phase 2 に着手しない） |
| D15 | target の誤りは `build` で出す（仕様 8） | 同じプロジェクトを wasm と Linux でも検査できる（W25 の D11）。既定の WASM は import なし（D-18）。採らない案: `check` で宣言を拒む（型検査は target を知らず、macOS のモジュールを含むプロジェクトをほかの target で検査できない） | 既定案（実装者はこの案に従う） |
| D16 | メンバーの受け手を先に置く形は D-59 の対象の外と文書に書く | 利用者が宣言する `extern` で std の関数ではない（W25 と同じ）。`ObjC.literal` は 1 引数。採らない案: 受け手を最後に置く（`[recv sel:a]` と宣言の順が食い違い、W25 の `I.m` とも揃わない） | 既定案（実装者はこの案に従う） |
| D17 | メンバーは直接の完全な呼び出しだけ | `Self`・classref・init の受け手が地点の型で決まり、関数本体と関数値の ABI を作らない（W25 の D6）。採らない案: メンバーごとに関数本体を生成して関数値を許す（派生の名前ごとに本体が要る） | 既定案（実装者はこの案に従う） |
| D18 | 新しい診断コードは割り当てず、仕様 9 の kind を足す。`abi_record_by_value_target` は W36 と、`abi_apple_only` は M01 と共有する | D-51、E12 D12 の前例。セレクターの形は M01 の `objc_invalid_selector`（`E1008`）を共有する（kind は 1 つのコードにしか属さない）。採らない案: 新しいコード、メンバーのセレクターだけの `E0002` の kind（同じ誤りに 2 つの kind） | 既定案（実装者はこの案に従う） |
