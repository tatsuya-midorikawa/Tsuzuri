# M07: Apple SDK のヘッダーからの宣言の生成器（`tsuzuri bindgen --objc`）と公式の `apple` パッケージ（W27 の macOS 版）

| 項目 | 内容 |
| --- | --- |
| ID | M07 |
| 優先度 | P2（M01・M02・M06・M20 の後に、手書きの宣言を減らす。AppKit のアプリは手書きの宣言でも書けるので P1 にしない） |
| 規模 | XL（Phase 1〜3。人間が求めない限り Phase 1 だけを実装する） |
| 依存 | [M01](M01-objc-class-types.md), [M02](M02-objc-typed-message-send.md), [M06](M06-extern-data-symbols-and-zutf8string.md), [M20](M20-macos-framework-link-inputs.md), [W17](W17-link-attribute-dllimport.md), ([M03](M03-objc-classes-defined-in-tsuzuri.md)), ([M04](M04-objc-blocks.md)), ([M05](M05-objc-exception-boundary.md)), ([M09](M09-main-thread-dispatch-and-handles.md)), ([M17](M17-macos-min-version-and-weak-import.md)), ([M23](M23-objc-out-parameters-and-weak-references.md)), ([W27](W27-win32-package-from-winmd.md)), ([W36](W36-c-layout-by-value.md))（M20 は Phase 2 の `@link(framework = …)` まで。M06 は Phase 1。M23 は Phase 1 の物体の出力引数の写像だけ） |
| 後続 | [M13](M13-desktop-os-services-package.md)（弱い後続: [M14](M14-tls-http-packages.md)） |
| 状態 | todo |
| 起票 | 2026-11-26（D-67。利用者の依頼「「どのチケットにもない壁」と「調査中に見つけたコンパイラの不具合（自分で再現済み）」のそれぞれについて @_features/ に開発チケットまたは修正チケットを作成して」から起票。調査で報告した「壁 2（宣言はすべて手書き。bindgen は AppKit のヘッダーで E2002、objc/runtime.h でも 114 件を省く）」に当たる。HEAD `804b7970` で確認） |
| 承認 | D-67 の分岐 1（X′）と分岐 5（配布の形）は承認済み（2026-11-26。[GUIDE D-67](GUIDE.md#d-67-tsuzuri-だけで-macos-のデスクトップアプリを書くm01m22)）。W27 の D3・D9 は W27 自身の決定として残る。分岐 1・分岐 5 は決定事項の D7・D8 に写した（承認済み）。要承認: D-55（`tsuzuri bindgen --objc`）。生成した宣言の `@trusted`（W27 と共有する決定）。決定事項 D2・D6 が要承認。D-55 の登録は決定事項の D9 に写した（要承認）。D14（`@optional` の字句の走査。停止条件 S1 の例外）も要承認で、承認までプロトコルは `// skipped` にする |
| 改善する劣位 | Swift（ClangImporter が SDK のヘッダーを直接読む）・Rust（`objc2` の生成済みの framework の crate）比: AppKit・Foundation の宣言を利用者が手で書くしかなく、セレクター・型・nullability・所有・スレッドの性質の誤りが宣言の数だけ入りうる。今の `tsuzuri bindgen` は AppKit のヘッダーで `E2002` になり、`objc/runtime.h` でも大半を省く |
| 手本にする既存実装 | `src/bindgen.rs`（`MARKER`・`TRUSTED_NOTE`・`MAX_AST_BYTES`・`HARMLESS_ATTRIBUTES`・位置の `Tracker`・属性の範囲の文字を読む `nonnull_attr_indices`・名前の関数）、`src/bindgen_driver.rs`（clang の起動、`capture`、`publish`・`protect_bindgen_output`）、`src/main.rs` の `bindgen_command`、`tests/bindgen.rs`（手で書いた JSON の AST で clang を起動しない検査）・`tests/bindgen.mjs`、[W27](W27-win32-package-from-winmd.md)（`--api` の一覧、閉包、名前空間ごとのモジュール、公式パッケージ、信頼の印） |
| 主な影響ファイル | `src/bindgen_objc.rs`（新規。模型・閉包・写像・書き出し）、`src/bindgen_objc_input.rs`（新規。clang の JSON の流し読みと symbol graph の読み取り）、`src/bindgen.rs`（部品を `pub(crate)` にする。出力は変えない）、`src/bindgen_driver.rs`、`src/main.rs`、`src/lib.rs`、`src/diagnostic_kinds.rs`、`tests/bindgen_objc.rs`（新規）、`tests/bindgen_objc.mjs`（新規。macOS と SDK があるときだけ動く）、`tests/fixtures/bindgen_objc/`（新規。`Fixture.h`・`api.txt`・期待の出力）、`.github/workflows/targets.yml`、`docs/language.md`、`docs/architecture.md`、`README.md`、`_tsuzuri/language-reference/compiler/native-interop.md`、`_tsuzuri/language-reference/compiler/option.md`、`_tsuzuri/language-reference/compiler/diagnostics.md`。Phase 2 はこのリポジトリの外の公式パッケージのリポジトリ。Phase 3: `src/package.rs`・`src/driver.rs`・`src/capabilities.rs`・`tests/audit.rs`・`tests/packages.rs`（W27 の Phase 3 と共有） |

## 実装者向け早見

- 範囲: Phase 1 は `tsuzuri bindgen --objc --framework <名前>... --api <一覧> -o <DIR>`。clang に Objective-C として framework の umbrella のヘッダーを読ませ、利用者が列挙した API とその型の閉包だけを、M01・M02・M06 の宣言（入っていれば M03・M04・M05・M09・M17 の形も）として書き出す。Phase 2 は同じ生成器で作る公式の git パッケージ `apple` と SDK の版ごとの再生成の手順、Phase 3 はルートが依存の `extern` を信頼する印（W27 の Phase 3 と同じもの）。
- **人間が求めない限り Phase 1 だけを実装する。**
- 最初に読む: ① 現状 5〜10・13（clang の JSON の AST に無い情報と、その取り方）② 仕様の Phase 1 ③ `src/bindgen.rs` の `Tracker`（429〜478 行）・`nonnull_attr_indices`（1913〜1954 行）・名前の関数（1764〜1846 行）④ M01・M02 の「他チケットへ提供するインターフェース」⑤ W27 の仕様 2・3・6。
- 最初に書くテスト: `tests/bindgen_objc.rs` の `maps_a_class_with_members`（手で書いた JSON の AST の `ObjCInterfaceDecl` 1 つから生成し、期待の `extern class` を手で書いて比べる。clang を起動しない）。
- 触ってはいけない: C のヘッダーからの `tsuzuri bindgen header.h`（引数・出力・診断・`lp64_target`・`snake_case`）、`MARKER`・`TRUSTED_NOTE` の文、`@trusted` の意味（E15）、D-59、std、M01・M02 の意味。
- 完了の見分け方（Phase 1）: macOS で次の一覧から `Apple/` を生成し、`check` が通り、下の `Main.tz` が headless で終了コード 0 を返し、プロセスが残らない。メンバーの名前は D2、`Maybe` の付き方は仕様 5 の規則に従う。名前空間の書き方と `Maybe` の外し方は、M01・M02 が出荷した綴りに合わせる。

```text
# api.txt
NSObject
NSApplication
NSWindow
NSString
NSApplicationActivationPolicy
```

```sh
target/debug/tsuzuri bindgen --objc --framework AppKit --api api.txt -o app/Apple
target/debug/tsuzuri check app
perl -e 'alarm 15; exec @ARGV' target/debug/tsuzuri run app; echo $?
```

```tsuzuri
def show :: Apple::AppKit.NSWindow -> i32 = \window ->
    Apple::AppKit.NSWindow.set_released_when_closed (ref window) false
    Apple::AppKit.NSWindow.set_alpha_value (ref window) 0.0
    let title: Apple::Foundation.NSString = ObjC.literal "Tsuzuri"
    Apple::AppKit.NSWindow.set_title (ref window) (ref title)
    Apple::AppKit.NSWindow.order_front (ref window) None
    let got = Apple::AppKit.NSWindow.title (ref window)
    let same = Apple::Foundation.NSString.is_equal_to_string (ref got) (ref title)
    Apple::AppKit.NSWindow.close (ref window)
    if same then 0 else 1

def main :: unit -> i32 = \() ->
    let app = Apple::AppKit.NSApplication.shared_application ()
    let _ = Apple::AppKit.NSApplication.set_activation_policy (ref app) Apple::AppKit.NSApplicationActivationPolicyAccessory
    match Apple::AppKit.NSWindow.alloc () with
    | Maybe.None -> 2
    | Maybe.Some raw ->
        match Apple::AppKit.NSWindow.init raw with
        | Maybe.None -> 3
        | Maybe.Some window -> show window
```

## 目的

M01・M02 がそろうと、Objective-C のクラスとメッセージ送信を型付きで宣言できる。しかし宣言そのものは手で書く。窓を 1 つ出すだけでも、NSApplication・NSWindow・NSString・NSObject のメンバー、enum の定数、framework のリンクを書く。セレクターの綴り、`BOOL` と整数の取り違え、nullability、所有の族、スレッドの性質の誤りは、宣言の数だけ入りうる。

今の `tsuzuri bindgen` は C のヘッダーだけを読む。AppKit のヘッダーでは clang が `E2002` で止まり、C のヘッダーでも ObjC の型と CF の型の大半を省く（現状 1・2）。W27 は winmd の生成器で、C のヘッダーの bindgen には触れない（W27:19・22）。

このチケットは、clang の出力から、M01・M02 の型で表せる範囲を機械的に生成し、表せない宣言は推測せずに理由付きで省く（GUIDE §1 規則 2）。clang の JSON の AST は Swift の注釈・availability・`@optional` を持たない（現状 5〜8）。その取り方を D1・D14 で先に決め、実装者が独自のヘッダーの解析に流れないようにする。

## 着手条件と停止条件

### 着手条件

- **人間が求めない限り Phase 1 だけを実装する**（GUIDE §0）。
- **承認が記録されるまで、該当する Phase に着手しない**（GUIDE §0 の 1）。D7（分岐 1 が X′）と D8（分岐 5）は承認済み（2026-11-26）。D2（名前）の承認まで手順 4、D9（D-55 の登録）の承認まで手順 6 に着手しない。D6 と W27 の D5 の承認まで Phase 3 に着手しない。D14 の承認までは、M03 があってもプロトコルを `// skipped`（`cannot tell @optional members`）にし、採用したプロトコルのメンバーをクラスに混ぜない（Phase 1 はこの形で完了できる）。
- Phase 1: M01・M02 の Phase 1・M06 の Phase 1・M20 の Phase 2・W17 が done。確認: `grep -nE "^\| \[?(M01|M02|M06|M20|W17)\b" _features/README.md`。弱い依存（M03・M04・M05・M09 の Phase 2・M17・M23 の Phase 1・W36 の Phase 2）は、入っていればその写像の行を有効にする。
- P2 のチケットは独立レビューを受けていない。着手の前に、その時点のコードと台帳でレビューし、指摘を反映してから実装する（GUIDE §0 の 2）。特に M01・M02 が出荷した綴りと仕様 5 の表を照らし合わせる。
- **Apple の SDK の利用条件の確認（Phase 2 の前提。Phase 1 の着手前に記録する）**: Xcode と SDK の利用許諾を読み、(a) 利用者の手元の SDK のヘッダーを読むこと、(b) 生成した `.tz` を公式パッケージとして再配布すること、(c) Tsuzuri の Apache-2.0 との組み合わせ、(d) 表示の義務を、このチケットの末尾に記録する。**起票の時点では確かめていない**（未確認）。
- macOS のホストに Command Line Tools か Xcode があり、`xcrun --show-sdk-path` が SDK を返す。SDK の版と `clang --version` を記録する。

### 停止条件

- **S1** clang の出力（JSON の AST、`-extract-api` の symbol graph、属性の spelling の範囲、承認の後の D14 の字句の走査）のどれでも必要な情報が取れず、ヘッダーを独自に解析したくなった。解析器を書かず、その宣言を `// skipped` にするか、止めて報告する。
- **S2** SDK の利用条件が再配布を許さない。Phase 1 は利用者が自分の SDK から生成する形として進め、Phase 2 は作らずに報告する。
- **S3** 手順 1 の計測で、核に絞った公式パッケージでも、依存の型検査が利用者の `check` の時間を支配する（hello の数倍）。D8 の分け方を変える前に報告する（CI に速度の閾値は入れない）。
- **S4** 写像の表の型が前提のチケットに無い、または綴りが違う。推測で別の型に写さず `// skipped`（理由 `needs <ID>`）にする。完了の見分け方の API が生成できなくなるなら止めて報告する。
- **S5** `unsafe` か新しい crate なしには実装できない（GUIDE §1 規則 10）。
- **S6** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `804b7970` で確認）

環境: macOS 27.2.1（arm64）、Apple clang version 21.0.0（clang-2100.4.1.1）、SDK `macosx27.2`（build `26B118`。`xcrun --show-sdk-path` は `/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk` を返し、これは `MacOSX27.2.sdk` への symlink）、HEAD で作り直した `target/debug/tsuzuri`。`SDK=$(xcrun --show-sdk-path)`。

1. **今の bindgen は AppKit のヘッダーで止まる**。`src/bindgen_driver.rs:63-76` は clang を `-x c -std=gnu17 -fsyntax-only -Xclang -ast-dump=json` で起動する。

```text
$ target/debug/tsuzuri bindgen "$SDK/System/Library/Frameworks/AppKit.framework/Headers/NSWindow.h" -o /tmp/tz-m07/NsWindow.tz
.../AppKit.framework/Headers/NSWindow.h:1:1: error[E2002]: 'clang' failed (exit status: 1):
In file included from .../MacOSX27.2.sdk/System/Library/Frameworks/AppKit.framework/Versions/C/Headers/NSWindow.h:8:
...
.../Foundation.framework/Headers/NSObjCRuntime.h:650:1: error: expected identifier or '('
.../Foundation.framework/Headers/NSObjCRuntime.h:652:9: error: unknown type name 'NSString'
...
$ echo $?
1
```

2. **C のヘッダーでも大半を省く**。出力の行を `grep -c` で数えた。`W2002` の表示は 50 件までで、残りは `warning: 64 more warnings not shown` のように数だけ出る。

| ヘッダー | `extern "…" def` | `extern type` | `const` | `// skipped` | 省いた主な理由（件数） |
| --- | --- | --- | --- | --- | --- |
| `usr/include/objc/runtime.h` | 4 | 6 | 9 | 114 | 型 `Class _Nullable`（38）・`id _Nullable`（13）・`Protocol * _Nonnull`（11）・`const char * _Nonnull`（10） |
| `CoreFoundation.framework/Headers/CFString.h` | 9 | 0 | 30 | 103 | 型 `CFStringRef`（32）・`CFAllocatorRef`（21）・`CFMutableStringRef`（20）、大域変数（16）、static（6）、可変長（3） |
| `CoreGraphics.framework/Headers/CGContext.h` | 50 | 1 | 48 | 77 | 値渡しの `CGRect`（19）、`bool`（10）、`CGSize`・`CGPoint`・`CGAffineTransform`（各 5） |

- `CFStringRef` は `const struct __CFString *` で、struct は `CFBase.h` にある。今の bindgen はヘッダー自身の宣言だけを読み（`src/bindgen.rs:404-427` の `owned`）、`const` の付いた不透明な struct のポインターを結果にしない（native-interop.md の「対応する形」）。
- `CGContext.h` の 10 件は、clang が型を `bool` と綴るので表に当たらない。`/tmp/tz-m07/b.h` に `#include <stdbool.h>`・`void f(bool b);`・`_Bool g(void);` を書くと `_Bool` と綴られ、`extern "f" def f :: bool -> unit` と `extern "g" def g :: unit -> i8u` が出た。C の経路はこのチケットで変えない。

3. **bindgen の部品**: `MARKER`（`src/bindgen.rs:21`）、`TRUSTED_NOTE`（24 行）、`MAX_AST_BYTES` = 256 MiB（26 行。`capture` は clang の標準出力がこの大きさを超えると子を止め、`E2002` `external_tool_output_too_large` を返す。`src/bindgen_driver.rs:176-186`）、`lp64_target`（308〜315 行）、配置を変えない属性の一覧 `HARMLESS_ATTRIBUTES`（537〜558 行。`ObjCBridgeAttr` は 546 行、`SwiftAttrAttr` は 549 行）、位置の `Tracker`（429〜478 行。clang は直前と同じ `file` を省くので、すべての位置を出力の順にたどる。`includedFrom` の `file` は数えない）、属性の引数を spelling の範囲のヘッダーの文字から読む前例 `nonnull_attr_indices`（1913〜1954 行）。位置引数はヘッダー 1 つ、出力は `.tz` のファイル 1 つだけ（247〜252 行）で、`W2002` は 1 つのヘッダーの位置に出す（`src/main.rs:3771-3780`）。`bindgen` の引数は `src/main.rs:4063` で一般の引数の解析より先に `bindgen_command` に渡る。clang は `tool("TSUZURI_CLANG", "clang")`（`src/driver.rs:6938`）で探し、環境変数、配布物の `bin/` の `tsuzuri-clang`、PATH の `clang` の順に使う（option.md の表）。出力の保護は `src/bindgen_driver.rs:231-276`。

4. **今は宣言をすべて手で書く**。調査で C を書かずに AppKit の窓を出した最小のプログラム（probe e4、100 行）は、W14 の同一形の規則を避けて `objc_msgSend` を整数 9 個と double 4 個の 1 つの形にそろえた。次の断片は HEAD で `check` が通る。

```tsuzuri
@trusted
@count(1, none)
extern "objc_getClass" def objc_get_class :: ref utf8string -> i64

@trusted
@count(1, none)
extern "sel_registerName" def sel_register_name :: ref utf8string -> i64

@trusted
extern "objc_msgSend" def send :: i64 -> i64 -> i64 -> i64 -> i64 -> i64 -> i64 -> i64 -> i64 -> f64 -> f64 -> f64 -> f64 -> i64

def main :: unit -> i32 = \() ->
    let cls = objc_get_class (ref u8"NSObject\0")
    if cls != 0 then 7 else 8
```

5. **JSON の AST は 3 つの属性の引数を持たない**。次のヘッダーを文と JSON の 2 つの形で出して比べた（文の出力は抜粋。アドレス・位置・`super`・`ParmVarDecl` の行を省いた）。

```objc
// /tmp/tz-m07/t.h
#import <Foundation/Foundation.h>
NS_ASSUME_NONNULL_BEGIN
NS_SWIFT_UI_ACTOR
API_AVAILABLE(macos(13.0))
@interface TzThing : NSObject
- (nullable instancetype)initWithName:(NSString *)name count:(NSInteger)count NS_SWIFT_NAME(init(name:count:));
- (NSString *)copyLabel NS_RETURNS_NOT_RETAINED;
+ (void)runWithBlock:(void (^)(BOOL finished))block API_AVAILABLE(macos(14.0));
@property (copy) NSString *title;
@end
NS_ASSUME_NONNULL_END
```

```text
$ clang -x objective-c -target arm64-apple-macosx12.0 -isysroot "$SDK" -fobjc-arc -fsyntax-only -Xclang -ast-dump -Xclang -ast-dump-filter=TzThing -I. t.h
ObjCInterfaceDecl ... line:5:12 TzThing
|-SwiftAttrAttr ... "@MainActor"
|-AvailabilityAttr ... macos 13.0 0 0 "" "" 0
|-ObjCMethodDecl ... - initWithName:count: 'instancetype _Nullable':'id'
| |-SwiftNameAttr ... "init(name:count:)"
| |-NSConsumesSelfAttr ... Implicit
| `-NSReturnsRetainedAttr ... Implicit
|-ObjCMethodDecl ... - copyLabel 'NSString * _Nonnull':'NSString *'
| `-NSReturnsNotRetainedAttr ...
|-ObjCMethodDecl ... + runWithBlock: 'void'
| `-AvailabilityAttr ... macos 14.0 0 0 "" "" 0
|-ObjCMethodDecl ... implicit - title 'NSString * _Nonnull':'NSString *'
`-ObjCMethodDecl ... implicit - setTitle: 'void'
```

- `-Xclang -ast-dump=json` では、`SwiftAttrAttr`・`AvailabilityAttr`・`SwiftNameAttr` の節点は `id`・`kind`・`range` だけを持ち、`"@MainActor"`・`macos 13.0`・`init(name:count:)` はどこにもない。
- JSON にあるもの: 族の暗黙の属性（`"implicit": true`）、明示の `NSReturnsNotRetainedAttr`、`returnType.qualType` の nullability、引数の `qualType`（`-fobjc-arc` では `NSString * _Nonnull __strong`）、プロパティの暗黙の getter と setter、`instance`、`"variadic": true`、`mangledName`（`-[NSWindow initWithContentRect:styleMask:backing:defer:]`）、クラスの `super` と `protocols`、カテゴリーの `interface`。

6. **`@MainActor` は継承をたどらないと分からない**。`NS_SWIFT_UI_ACTOR` は `NS_SWIFT_MAIN_ACTOR`（`NSObjCRuntime.h:248-253`。`__has_attribute(swift_attr)` のとき `swift_attr("@MainActor")`）。`NSResponder.h:20` の `@interface NSResponder` に付き、`NSWindow.h:283` の `@interface NSWindow : NSResponder` には付かない（Swift は global actor をサブクラスへ継承する）。JSON の NSResponder の `SwiftAttrAttr` の `range.begin.spellingLoc` は `NSObjCRuntime.h` の offset 9062（`tokLen` 10）、`end.spellingLoc` は offset 9085（`tokLen` 1）で、そのファイルの 9062〜9086 バイトは `swift_attr("@MainActor")` だった。`nonnull_attr_indices` と同じ方法で引数を読める。

7. **`NS_SWIFT_SENDABLE` は普通の clang では空になる**。`NSObjCRuntime.h:279` の `#if __SWIFT_ATTR_SUPPORTS_SENDABLE_DECLS` のときだけ `swift_attr("@Sendable")`（285 行）で、そうでなければ空（307 行）。`NSValue.h:180-181` の `NS_SWIFT_SENDABLE` と `@interface NSNumber : NSValue` は、`-D__SWIFT_ATTR_SUPPORTS_SENDABLE_DECLS=1` を付けたときだけ `SwiftAttrAttr "@Sendable"` を持ち、同時に `"@_nonSendable(_assumed)"`（`__NS_HEADER_AUDIT_BEGIN_sendability` の `#pragma clang attribute` から。spelling は `<scratch space>`）も持つ。NSMutableArray は後者だけを持つ。このマクロは Swift の ClangImporter が定義すると見られる（リポジトリの外の事実。未確認）。

8. **`@optional` はどの出力にもない**。`@protocol TzDelegate <NSObject> @optional - (void)thingDidChange:(id)thing; @end` を、JSON の AST（メソッドの節点の鍵は `id`・`inner`・`instance`・`kind`・`loc`・`mangledName`・`name`・`range`・`returnType`）、文の AST、`-Xclang -ast-print`、`-extract-api` で出したが、どれも `@optional` を表さなかった。AppKit と Foundation のヘッダーを正規表現で数えると（概数）、プロトコル 193 個のうち 121 個が `@optional` を持ち、13 個（NSApplicationDelegate・NSTableViewDelegate など）は本体に前処理の行を含むが、その 67 行はすべて `#pragma` だった（1 行は `NSToolbar.h` の `# pragma mark` で、`#` と `pragma` の間に空白がある）。

9. **availability は `clang -extract-api` の symbol graph から取れる**。

```text
$ H=$SDK/System/Library/Frameworks/AppKit.framework/Headers
$ clang -extract-api --product-name=AppKit -x objective-c-header -target arm64-apple-macosx12.0 -isysroot "$SDK" "$H/NSComboButton.h" "$H/NSResponder.h" -o two.json
c:objc(cs)NSResponder    objective-c.class  （macos の availability なし）
c:objc(cs)NSComboButton  objective-c.class  [{"domain": "macos", "introduced": {"major": 13, "minor": 0, "patch": 0}}]
```

- umbrella の `AppKit.h` だけを渡すと（`-F` を足しても）記号は 0 個。`Headers/*.h` をすべて渡すと動く: AppKit は 304 個で 1.25 秒、17,706,147 バイト、記号 12,153 個（クラス 322 個のうち macOS の `introduced` を持つもの 204 個）。Foundation（173 個）・CoreGraphics（52 個）・CoreFoundation（47 個）・`usr/include/objc`（17 個、記号 326 個）も終了コード 0 で `error:` はなかった。相対パスのヘッダーは、clang が作る `<extract-api-includes>` の `#import` で見つからず `fatal error` になる（絶対パスなら通る）。
- 識別子は USR（`c:objc(cs)NSComboButton`、`c:@F@NSStringFromClass`、`c:@NSDefaultRunLoopMode`）。メソッドは `(im)`・`(cm)`、プロパティは `(py)`・`(cpy)` で、プロパティの暗黙の accessor は別の記号を持たない（`c:objc(cs)NSWindow(py)title` はあるが `(im)setTitle:` はない。`+sharedApplication` は `c:objc(cs)NSApplication(cpy)sharedApplication`）。プロトコルのメンバーは `c:objc(pl)TzDelegate(im)thingDidChange:`。nullability と属性は落ちる（`- (nullable instancetype)initWithName:…` が `- (instancetype) initWithName:…` になる）ので、symbol graph だけでは足りない。

10. **AppKit の JSON の AST は `MAX_AST_BYTES` を超える**。`#import <AppKit/AppKit.h>` だけの `.m` の JSON は 386,491,013 バイト（`-fobjc-arc`。clang は 1.95 秒。仕様 2 の `-D__SWIFT_ATTR_SUPPORTS_SENDABLE_DECLS=1` を足すと 391,133,639 バイト）で、上限の 268,435,456 バイトを超える（Foundation は 203,451,382 バイト）。最上位の節点は 27,194 個で、最大は 2,450,937 バイト（`-D__SWIFT_ATTR_SUPPORTS_SENDABLE_DECLS=1` で測った）。同じ内容の文の AST は 33,429,445 バイト。`-fmodules` の `@import AppKit;` は取り込んだ宣言を出さない（55,431 バイト）。JSON の入れ子（物体と配列）は最も深いところで 51 段で、`serde_json` の上限 128 の内にある。
   - 同じクラスの `ObjCInterfaceDecl` は、`@class` の前方宣言ごとに最上位の要素として出る。前方宣言は `previousDecl` を持ち、`super` と `protocols` も写る。後の前方宣言は継承した属性（`"inherited": true` の `SwiftAttrAttr`・`AvailabilityAttr`）を持つ。AppKit の JSON の NSWindow は 25 個で、メソッドを持つのは 1 個だけだった。

11. **ObjC のモードでは dispatch の型が ObjC の物体になる**。`-x objective-c -fobjc-arc` の `dispatch_queue_create_with_target` の型は `DISPATCH_RETURNS_RETAINED dispatch_queue_t _Nonnull (const char * _Nullable, dispatch_queue_attr_t _Nullable __strong, …)`（`ns_returns_retained` 付き）で、`-x c` では `__strong` も属性もない。ObjC のモードには `__kindof NSObject * _Nullable`、`ObjectType _Nonnull`、`id<TzDelegate> _Nullable __strong`、`NSArray<NSString *> * _Nonnull`、`NSError *__autoreleasing _Nullable * _Nullable`、`void (^ _Nonnull __strong)(BOOL)` の綴りが現れた。

12. **パッケージはリンクの要求を `[native]` で運べない**。git・registry のパッケージ自身の `[native]` は `E2000` `native_link_setting_not_allowed` で拒まれ（`src/driver.rs:2814-2823`、公開は `src/fetch.rs:108-117`）、ルートも git・registry の依存に `native = true` を付けられない（`src/package.rs` の `invalid_git_dependency`・`invalid_registry_dependency`、テスト 1645〜1653 行）。公式パッケージの framework のリンクは、M20 の Phase 2 の `@link(framework = "...")` でしか運べない。W17 の対象外は framework（W17:247）。依存のモジュールはすべて型検査される（W27 の現状）ので、公式パッケージは核に絞る。予約されたモジュール名は 78 個（`src/stdlib.rs:82`）。

13. **`-fobjc-arc` は所有を壊すメンバーの一部だけを不可にする**。`objc/NSObject.h:36-39` の `retain`・`release`・`autorelease`・`retainCount` は `OBJC_ARC_UNAVAILABLE` で、`-fobjc-arc` のときだけ `UnavailableAttr`（`not available in automatic reference counting mode`）を持つ（`-fno-objc-arc` では属性がない）。`-dealloc`（75 行）と `+alloc`（74 行）は `OBJC_SWIFT_UNAVAILABLE` だけで、文の AST は `AvailabilityAttr swift ... Unavailable` を示す。macOS では使え、`UnavailableAttr` はない。

## 変えてはいけない意味

- C のヘッダーからの `tsuzuri bindgen header.h` の引数・出力・診断・`lp64_target`・名前の規則。`tests/bindgen.rs`・`node tests/bindgen.mjs` の期待を 1 byte も変えない。
- `MARKER`・`TRUSTED_NOTE` の文と、`@trusted` の意味（E15）。
- D-59 は std と組み込みの関数の規則で、std の外のパッケージの `extern` には当たらない。メンバーの受け手を先に置く形は M02 のとおり。
- 生成器は CLI の道具で、コンパイラの IR・object・header・診断・`.wasm` を変えない。生成物を wasm か Apple でない native で build すると、M01・M02 の `E1008`（`abi_native_only`・`abi_apple_only`。`check` は通る）になる。生成器はそれを避ける印や分岐を足さない。
- GUI フレームワークを std に入れない非目標（D-67 の「改めないもの」）。`apple` パッケージは std の外で、宣言の上の安全な包みを書かない。可変長引数を言語に入れない方針（docs/language.md:446）。

## 仕様

### Phase の範囲

| Phase | 内容 | 規模 | 前提 |
| --- | --- | --- | --- |
| 1 | `tsuzuri bindgen --objc`。clang の 2 回の起動と流し読み、一覧と閉包、framework ごとのモジュール、写像、スレッドの性質・所有・availability、`// skipped`、決定性と保護 | L | M01・M02 の Phase 1・M06 の Phase 1・M20 の Phase 2・W17。D2・D9（D7 は承認済み） |
| 2 | 公式の git パッケージ `apple`（核の一覧、生成の CI、再生成の手順） | M | 利用条件の記録（D8 は承認済み） |
| 3 | ルートが依存の `extern` を信頼する印。W27 の Phase 3 と同じもの | S | D6・W27 の D5 |

### Phase 1: 1. コマンド

```text
tsuzuri bindgen --objc --framework NAME [--framework NAME]... --api LIST -o DIR [-F DIR]... [--min-version X.Y] [--json]
```

- `--objc` は C のヘッダーの位置引数・`--include-dir`・`--buffer`・`--consume`・`--nonnull`・W27 の `--winmd` と同時に書けない。`--framework`・`--api`・`-o` は必須。
- `--framework NAME` は framework の名前だけ（M20 と同じ規則）で、繰り返せる。探す場所は SDK の `System/Library/Frameworks` と `-F DIR`（M20 の `framework_search` と同じ意味）。
- `-o DIR` はディレクトリ（無ければ作る）。
- `--min-version X.Y` は weak の判定と clang の `-target` に使う。無ければ `MACOSX_DEPLOYMENT_TARGET`（M17 と同じ）、それも無ければ `12.0`。
- `--framework NAME` の umbrella を取り込み、宣言はその umbrella が取り込むすべての framework から読む（仕様 2）。モジュールは閉包の宣言がある framework ごとに出る（`--framework AppKit` だけでも `Foundation.tz` が出る）。
- SDK は `xcrun --show-sdk-path` で探す（`SDKROOT` は `xcrun` が読むので、Tsuzuri は環境変数を足さない）。clang は今の bindgen と同じ `tool("TSUZURI_CLANG", "clang")`（環境変数、配布物の `bin/` の `tsuzuri-clang`、PATH の `clang`）。`-target` を明示するので `lp64_target` は通らない。生成物の対象は `arm64-apple-macosx` だけ（x86_64 は M18 で、D-67 の分岐 3 により着手しない。`BOOL` の型が違う）。
- 誤りの診断は仕様 9。

### Phase 1: 2. clang の起動と読み取り

clang を 2 回起動する（D1）。

1. **構造（JSON の AST）**: 一時ディレクトリに `#import <NAME/NAME.h>` を `--framework` の順に並べた `.m` を書き、次で読む。

```text
clang -x objective-c -fobjc-arc -fsyntax-only -target arm64-apple-macosx<min> -isysroot <SDK> [-F DIR]... -D__SWIFT_ATTR_SUPPORTS_SENDABLE_DECLS=1 -Xclang -ast-dump=json <tmp>.m
```

   - 標準出力を流しのまま読み、`TranslationUnitDecl` の `inner` の要素を 1 つずつ切り出す（文字列と入れ子の深さを追う字句の走査）。要素だけを `serde_json::Value` にし、`Tracker` で `file` を追い（読み捨てる要素も追う）、要る宣言を模型（設計）に写して `Value` を捨てる。
   - 1 つの要素は `MAX_AST_BYTES` まで、全体は 2 GiB まで（AppKit の約 5 倍）。
   - 読むのは、framework のヘッダー（SDK と `-F` の下で、パスに `/<名前>.framework/Headers/` を含むもの）と仕様 4 の表のディレクトリのヘッダーにある `ObjCInterfaceDecl`・`ObjCCategoryDecl`・`ObjCProtocolDecl`（と `ObjCMethodDecl`・暗黙の accessor）・`FunctionDecl`・`VarDecl`・`EnumDecl`・`RecordDecl`・`TypedefDecl`。`--framework` で選んだものに限らない（AppKit の宣言は Foundation・CoreGraphics の型を参照する）。
   - 同じクラスの `ObjCInterfaceDecl` は前方宣言の数だけ出る（現状 10）。メソッドを持つ定義を模型にし、前方宣言の `inherited` の属性は使わない。
2. **availability（symbol graph）**: 閉包（仕様 3）を求めた後に、閉包の宣言がある framework ごとに `Headers/*.h`（仕様 4 の表のディレクトリは、そのディレクトリの `*.h`）を名前のバイト順に絶対パスで渡す（相対パスは `fatal error`。現状 9）。

```text
clang -extract-api --product-name=NAME -x objective-c-header -fobjc-arc -target arm64-apple-macosx<min> -isysroot <SDK> [-F DIR]... -D__SWIFT_ATTR_SUPPORTS_SENDABLE_DECLS=1 <Headers/*.h> -o <tmp>.json
```

   - 読むのは各記号の `identifier.precise`（USR）と、`domain` が `macos` の `availability`（`introduced`・`deprecated`・`obsoleted`・`isUnconditionallyUnavailable`）だけ。模型の宣言から USR を作って照合する（`c:objc(cs)Name`・`c:objc(pl)Name`・`c:@F@name`・`c:@name`・`c:objc(cs)Class(im)sel`・`(cm)sel`。プロパティの暗黙の accessor はプロパティの `(py)name`・`(cpy)name`、プロトコルのメンバーは `c:objc(pl)Name(im)sel`）。照合できない宣言は availability なし。
3. **Swift の注釈の引数**: `SwiftAttrAttr` の `range` の `spellingLoc` の始まりから終わり（`offset + tokLen`）までのバイトを、SDK の中の実在のファイル（256 MiB まで）から読む。`swift_attr("@MainActor")`・`swift_attr("@Sendable")`（`__swift_attr__` も）だけを受け、ほかの文字列・`<scratch space>`・読めない範囲は「注釈なし」。
4. **プロトコルの `@optional`**（D14 の承認の後だけ）: D14 の字句の走査。使うのは、プロトコルを出すとき（M03）と、採用したプロトコルの required のメンバーをクラスに混ぜるとき。

### Phase 1: 3. API の一覧（`--api`）と閉包

1 行に 1 つ。`#` から行末は注釈。空行は読み飛ばす。W27 の書式に ObjC の形を足す（D10）。

| 書き方 | 意味 |
| --- | --- |
| `NSWindow` | クラス（`@interface` のメンバー、同じ framework の名前付きのカテゴリーのメンバー、採用したプロトコルの required のメンバー。最後のものは D14 で決められたときだけ） |
| `-[NSWindow setTitle:]`・`+[NSWindow alloc]` | メンバー 1 つ（clang の `mangledName` と同じ綴り）。クラスはそのメンバーだけの本体で出る |
| `NSString(NSStringDrawing)`・`NSString(*)` | カテゴリー 1 つ、またはそのクラスの読んだ framework のカテゴリーのすべて |
| `@protocol NSWindowDelegate` | プロトコル（M03 があるときだけ） |
| `CGContextFillRect` | C の関数、大域変数、enum の型、typedef、struct |
| `NSWindowStyleMask*` | 末尾の `*` の接頭辞（定数・関数・変数だけ） |
| `catch -[NSFileHandle readDataOfLength:]` | M05 があれば結果を `Result<R, ObjC.Exception>` にする |

閉包（D11）: 一覧に書いたものは本体ごと出す。署名と field が参照するクラスとプロトコルは本体のない宣言（M01 の `extern class X : Base`）で出し、基底の連鎖を根の `NSObject` まで足す。参照する enum・typedef・struct・CF の型は定義ごと足す。定数は一覧に書いたものと、閉包の enum の要素だけ。

### Phase 1: 4. 出力の配置と名前

- framework 1 つにつき 1 つのモジュール（`DIR/AppKit.tz`・`DIR/Foundation.tz`。D12）。framework の外のヘッダーは次の表だけを受け、ほかは `// skipped`（`not in a framework`）。

| SDK のディレクトリ | モジュール | 理由 |
| --- | --- | --- |
| `usr/include/objc/` | `Foundation` | `NSObject` のクラスとプロトコルがあり、Foundation の `NSObject.h` が取り込む |
| `usr/include/dispatch/` | `Dispatch` | GCD の C の API |

- モジュール名が `RESERVED_MODULES`（生成の時点の値。M01 の `ObjC`、M09 の `MainThread` を含む）と同じなら末尾に `_`。
- ほかのモジュールの宣言は、出力のルートからのモジュール名で書く（`Foundation.NSString`）。利用者は `-o Apple` で `Apple::AppKit`、公式パッケージは `namespace = "Apple"` と `-o .` で同じ道筋になる（名前空間を外側へ補う解決。W27 の現状）。
- 名前（D2）:
  - クラス・プロトコル・カテゴリーは ObjC の名前のまま。型名として受理されない名前は `extern "<実行時名>" class <PascalCase>`（M01）。先頭が `_` の名前は `// skipped`。
  - メンバーはセレクターの各片を snake_case にして `_` でつなぎ、末尾の `:` を落とす（`initWithContentRect:styleMask:backing:defer:` → `init_with_content_rect_style_mask_backing_defer`）。snake_case は今の `snake_case` に「大文字の連なりの最後の大文字が小文字の前にあれば、その前で切る」を足す（`URLForDirectory` → `url_for_directory`、`isHTTPS` → `is_https`）。セレクターの文字列は、名前と同じとき（`close`）だけ省く。
  - ObjC のモードの C の関数とデータシンボルも同じ snake_case（`NSDefaultRunLoopMode` → `ns_default_run_loop_mode`）。C の経路（`CGContextSaveGState` → `cgcontext_save_gstate`）は変えない。enum の定数は C の名前のまま、typedef と struct は今の `pascal_case`、予約語と組み込みの名前は末尾に `_`（`value_name`・`type_name`）。
  - 衝突: M02 の `duplicate_objc_member`（`E1001`）と同じ範囲（基底の連鎖と、それらのカテゴリー）で調べる。基底の連鎖の中でインスタンスメソッドと同じ名前になったクラスメソッドは先頭に `class_`（`+[NSObject description]` → `class_description`）。残る衝突は、インスタンス、クラス、セレクターのバイト順に並べて 2 つ目以降に `_2`・`_3`。衝突は一覧に依らず模型の全体（読んだ framework のすべてのメンバー）で決め、一覧を変えても名前が変わらないようにする。
  - 派生が基底と同じセレクターを同じ型で宣言し直したものは、基底のそのメンバーを出すとき（基底を本体ごと出すとき）だけ省く（M02 で基底のメンバーを派生の名前で呼べる）。基底が本体なしなら派生に出す。型が違えば `_2` を付けて出す。
  - メンバーの名前も、衝突を解いた後に `value_name` を通す（予約語と `_` は末尾に `_`。`-class` → `class_`）。

### Phase 1: 5. 写像

nullability: `_Nonnull` は `Maybe` なし。`_Nullable`・`_Nullable_result`・注釈なし・`_Null_unspecified` は `Maybe`（今の bindgen と同じ。`NS_ASSUME_NONNULL` の範囲では clang が `_Nonnull` を綴る）。`Maybe` は引数の `ref X` を `Maybe<ref X>` に、結果の `X` を `Maybe<X>` にする。`__strong`・`__autoreleasing`・`__unsafe_unretained`・`__kindof` は外して読む。ただし、物体のポインターのポインターの内側の修飾は、M23 の写像と仕様 7 の判定に使う。

| ObjC の型 | 引数 | 結果 | 前提 |
| --- | --- | --- | --- |
| メンバーの `BOOL`・`bool`・`_Bool`（arm64 ではどれも 1 バイト） | `bool` | `bool` | M02 |
| `NSInteger`・`NSUInteger`・`CGFloat`・固定幅の整数・`float`・`double` | `i64`・`i64u`・`f64`・対応する型 | 同じ | M02 |
| `SEL`・`Class` | `ObjC.Sel`・`ObjC.Class` | 同じ | M01 |
| `id`・型引数（`ObjectType`）・M03 が無いときの `id<P>` | `ref ObjC.Object` | `ObjC.Object` | M01 |
| `C *`・`__kindof C *`・`C<T> *`（型引数は消す）・`C<P> *` | `ref C` | `C` | M01 |
| `id<P>`（M03 があるとき） | `ref P` | `P` | M03 |
| `instancetype` | – | `Self` | M02 |
| `BOOL *`（出力） | `ref mut bool` | × | M02 |
| `C * __autoreleasing *`・`id __autoreleasing *`（物体の出力引数。`NSError **` の ARC の既定） | `ref mut Maybe<C>`（`C` は内側のポインターを上の行の結果の列で写した型。`NSError *` は `NSError`、`id` は `ObjC.Object`、`id<P>` は M03 があれば `P`。外側の nullability と `out` の印は写像を変えない） | × | [M23](M23-objc-out-parameters-and-weak-references.md) の Phase 1 |
| `NS_ENUM`・`NS_OPTIONS`・`CF_ENUM` | `type Name = T`（基底の整数型）と要素の `const NAME: T` | 同じ | – |
| 構造体の値（`NSRect`・`NSRange`） | `@c_layout` record | 同じ | M02 の Phase 2 |
| `const S *`（struct） | `ref S` | × | M02 |
| Block `R (^)(P1, …)` | `ref ObjC.Block<(P1 -> … -> R)>` | `ObjC.Block<F>` | M04 |
| `const char *` | `ref ZUtf8String` | × | M06 の Phase 2 |
| CF の型（`CFStringRef` など。仕様 6） | `CFString`（`extern type copy`） | 同じ | W18 |
| `void *` | `i64`（アドレスは `i64`。D-63・D-67。W27 と同じ） | `i64` | – |

- クラス: `@link(framework = "<framework>") extern class [main|sync] Name : Base [with P, Q] { members }`（`main` と `sync` はどちらか 1 つか無し。仕様 6）。`with` は M03 があり、D14 で採用したプロトコルの required が決まり、そのプロトコルが閉包にあるときだけ書く（M03 の「他チケットへ提供するインターフェース」）。`NSObject` は `: ObjC.Object` と `@link("objc")`（W17）。クラスメソッドはメンバーの先頭の `class`（M02）。
- カテゴリー: `extern category Name : Class { members }`（クラスがほかのモジュールなら `: Foundation.NSString`）。プロトコル（M03）: `extern protocol Name [: Q, …] { [optional] members }`（基底のプロトコルは `@protocol Name <Q>` から）。
- C の関数: 今の bindgen の表に、ObjC の型（M01 の `extern def` の位置）と `bool` の綴りを足す。`BOOL`・`bool` は今の `_Bool` と同じく、引数は `bool`、結果は `i8u`（M02 の契約で `extern def` の `bool` は `int32_t` のまま。C が保証するのは下位 8 bit だけ）。値渡しの struct は W36 の Phase 2 があれば `@c_layout` record。`@link(framework = …)` を付ける。
- データシンボル（M06 の Phase 1）: ObjC の型・スカラー・CF の型の `const` な大域変数は `extern "X" view def name :: unit -> T`、不完全な struct の大域変数（`_dispatch_main_q`）は `address def`。`__` で始まる symbol は今のとおり予約で `// skipped`。

### Phase 1: 6. スレッドの性質・所有・availability

- **スレッドの性質**（D3）: クラスの単位の `@MainActor` をそのクラスと基底の連鎖で探し、あれば `main`（M09 の Phase 2）。M09 の Phase 2 が無ければ既定（M01 の D1。推奨は W19 の `local`）にし、宣言の前に `// NSWindow: Swift @MainActor; the default thread property until M09 Phase 2` の行を出す。クラス自身に `@Sendable` があれば `sync`。両方あれば `main`。`@_nonSendable`・`~Sendable`・`(_assumed)` は既定。
- **所有**（D4）: 結果が ObjC の物体（M01 の `C`・`Maybe<C>`）のときだけ、明示の（`implicit` でない）`NSReturnsRetainedAttr`・`CFReturnsRetainedAttr` を `@returns_retained`、`NSReturnsNotRetainedAttr`・`CFReturnsNotRetainedAttr` を `@returns_not_retained` に写す。結果が物体でない（CF の型など）宣言に属性を書くと M01 の `E1008` `objc_returns_retained_not_object` になるので、属性を書かず、`CFReturnsRetainedAttr` なら下の Create の規則と同じ注記の行を出す。暗黙の族の属性は書かない（M01・M02 が族の規則を当てる）。
- **CF の型**: `ObjCBridgeAttr` か `ObjCBridgeMutableAttr` を持つ struct へのポインターの typedef（`CFStringRef`・`CGColorRef`）は `extern type copy`。名前は今の bindgen と同じく struct の tag の `pascal_case`（`__CFString` → `CFString`。`CFStringRef` と `CFMutableStringRef` は同じ型になる）。閉包の型ごとに、`Owned` に型名をつないだ record（`record OwnedCFString { handle: CFString }`）、`extern "CFRelease" def cf_release_<snake> :: <型名> -> unit`（W14 で同じ symbol を型ごとに宣言できる）、それを呼ぶ `instance Drop<OwnedCFString>` を出す（W27 の RAIIFree の `OwnedH` と同じ形）。名前に `Create`・`Copy` を含み、結果が CF の型の C の関数の前に `// CFStringCreateCopy: returns a +1 reference (Create rule); wrap it in OwnedCFString` の形の行を出す。
- **availability**（D5）: クラス・C の関数・データシンボルの `introduced` が `--min-version` より新しければ、M17 があれば `@weak`、無ければ `// <名前>: introduced in macOS X.Y; needs @weak (M17) below that version` の行。メンバーは symbol ではないので `// macOS X.Y` の行だけ。macOS で使えないもの（`isUnconditionallyUnavailable`、`unavailable`、`UnavailableAttr`）は `// skipped`。非推奨は `// deprecated in macOS X.Y` の行を付けて出す。

### Phase 1: 7. 生成しないもの（`// skipped` と `W2002`）

推測で別の型に写さず、宣言かメンバーの位置に `// skipped <名前>: <理由>` を書き、`W2002`（既存の `bindgen_skipped_declaration`）をその宣言のあるヘッダーの位置に出す。

| 対象 | 理由の文（案） |
| --- | --- |
| 可変長のメソッドと関数 | `variadic method`・`variadic function` |
| 物体へのポインターのポインター（`NSError **`）。[M23](M23-objc-out-parameters-and-weak-references.md) の Phase 1 があれば、内側の修飾が `__autoreleasing` のものは `ref mut Maybe<C>` に写し（仕様 5）、ここに残るのは内側が `__strong`（`NSNetServices.h:230` など）か `__unsafe_unretained` のものと、`const` の物体のポインター（ARC では `__unsafe_unretained`） | M23 の前は `pointer to an object pointer`。M23 の後は `strong object out-parameter`・`unretained object out-parameter` |
| 長さのないポインター（`unichar *`・`const CGFloat *`） | `pointer without a length` |
| 前提のチケットの形が無い型 | `needs M02 Phase 2`・`needs W36 Phase 2`・`needs M04`・`needs M06 Phase 2`・`needs M03` |
| `objc_direct` のメソッド（メソッドの表に無く送れない） | `objc_direct method` |
| 引数の `ns_consumed`、`objc_returns_inner_pointer` | `consumed argument`・`inner pointer result` |
| `-dealloc`（呼ぶと所有者の下で物体が消える。M01 の drop が release する） | `dealloc is called by the runtime` |
| macOS で使えない宣言、`NS_UNAVAILABLE`、`-fobjc-arc` の `UnavailableAttr`（`retain`・`release` など。現状 13） | `unavailable on macOS` |
| 名前のないカテゴリー、先頭が `_` のクラス・プロトコル・カテゴリー・セレクター（C の関数とデータシンボルは今の規則で、`__` だけを省く） | `class extension`・`private name` |
| union・bit-field・配置を変えうる属性・解けない衝突・`AsmLabelAttr` | 今の bindgen の文 |
| `@optional` を決められないプロトコル（D14。承認前はすべてのプロトコル） | `cannot tell @optional members` |

### Phase 1: 8. 決定性と出力の保護

- 各ファイルの先頭: `MARKER`、`// sdk: macosx27.2 (26B118)`（`SDKSettings.json` の `CanonicalName` と `xcrun --show-sdk-build-version`）、`// clang: <clang --version の 1 行目>`、`// target: arm64-apple-macosx<min>`、`// api: <一覧のファイル名> sha256=<16 進>`、`TRUSTED_NOTE`。絶対パス・日時を入れない（`sanitize`）。
- 並び: 定数 → 型別名 → `@c_layout` record → `extern type copy` と `Owned…` → プロトコル → クラス（基底を派生より先に、同じ深さは名前のバイト順）→ カテゴリー → C の関数 → データシンボル。メンバーはクラスメソッド、インスタンスメソッドの順に、それぞれセレクターのバイト順。一覧の行の順に依らず、同じ入力から同じバイト列。
- `MARKER` で始まるファイルだけを上書きし、生成しなくなったモジュールの古いファイル（`MARKER` で始まるもの）は消す（W27 と同じ）。

### Phase 1: 9. 診断

| コードと kind | 条件 |
| --- | --- |
| `E2000` `bindgen_argument_error`（既存） | 引数の組み合わせ・欠け、`--min-version` が 12.0 より前か形の誤り、`-o` がファイル |
| `E2000` `bindgen_unknown_api`（kind 案。W27 と共有） | 一覧の名前が無い・形の誤り・重複、M05 が無いのに `catch` |
| `E2000` `bindgen_unknown_framework`（kind 案） | `--framework` の名前が SDK と `-F` に無い |
| `E1017` `bindgen_api_limit`（kind 案。W27 と共有） | 閉包が 65,536 個を超える |
| `E2002` `bindgen_apple_sdk_missing`（kind 案） | macOS でない、`xcrun` が無い・失敗（`tsuzuri bindgen --objc needs the macOS SDK; install the Command Line Tools or Xcode`） |
| `E2002` `external_tool_failed`・`external_tool_output_too_large`・`bindgen_unreadable_ast`（既存） | clang の失敗、仕様 2 の上限、AST と symbol graph の形の誤り |
| `E2003` `bindgen_output_not_generated`（既存） | `MARKER` のないファイルへの出力 |
| `W2002` `bindgen_skipped_declaration`（既存） | 仕様 7 |

新しい診断コードは足さない（D-51）。W27 と共有する kind は先に入ったほうが登録する。kind は `src/diagnostic_kinds.rs` に登録してから `node scripts/gen-docs.mjs` で文書の表を作る。

### Phase 2: 公式の `apple` パッケージ

- このリポジトリと同じ所有者の別の git リポジトリ（名前・置き場・ライセンスは D8）。ルートに `Tsuzuri.toml`（`name = "apple"`、`namespace = "Apple"`）、`api.txt`、生成したモジュール（`Foundation`・`AppKit`・`CoreFoundation`・`CoreGraphics`・`Dispatch`）、出どころ（SDK の `CanonicalName` と build、clang と `tsuzuri` の版、`--min-version`）と利用条件の表示を書いた README。
- 核: 調査の最小のアプリ（NSApplication、NSWindow、描画、target-action、NSMenu、NSTimer）が使う宣言。一覧は P2-1 の計測（S3）で決める。M13 の Phase 1 が使う宣言は、M13 がこの一覧（`api.txt`）に足して生成し直す（M13 の手順 2）。ほかは利用者が生成する。
- 生成はそのリポジトリの CI（macOS のランナー）が、固定した Xcode と `tsuzuri` で行い、`git diff --exit-code` と headless の小さなアプリ（完了の見分け方の形）を確かめる。
- 再生成の手順（README）: Xcode を上げる → SDK の版を記録 → 同じ `--min-version`（推奨 `12.0`。パッケージが支える最低の版で、利用者の `[macos] min_version` が高くても `@weak` が残るだけ）で生成 → 差分を読む（消えた宣言、型が変わった宣言、新しい `@weak`）→ CI → 版を上げて tag。
- 利用: `apple = { git = "https://…", rev = "<40 桁>", capabilities = ["ffi"] }`。`[native]` は要らない（M20 の Phase 2）。

### Phase 3: ルートが依存の extern を信頼する印

- W27 の仕様 9 と同じ（ルートの直接の依存の `trusted = true`、`tsuzuri audit` の `trusted (dependency)`、`W1010` を出さない）。W27 の Phase 3 が先なら、M07 の Phase 3 は `apple` の README と言語リファレンスの例だけ。M07 が先なら W27 の仕様 9 のとおりに実装し、W27 の Phase 3 を同じ PR で満たす。

## 設計

### 置き場

| ファイル | 内容 |
| --- | --- |
| `src/bindgen_objc_input.rs`（新規） | 流しの JSON の要素の切り出し、`Tracker` での位置の追跡、symbol graph と spelling の範囲の読み取り。写像を知らない |
| `src/bindgen_objc.rs`（新規、`pub mod bindgen_objc`） | 模型、一覧と閉包、写像、名前、`// skipped`、書き出し |
| `src/bindgen.rs` | `Tracker`・`value_name`・`type_name`・`pascal_case`・`sanitize`・型の綴りの読み取りを `pub(crate)` にする。C の経路の出力は変えない |
| `src/bindgen_driver.rs` | `--objc` の経路: SDK の探索、一時の `.m`、clang の 2 回の起動、ディレクトリの保護と古いファイルの削除、`W2002` を各ヘッダーの位置に出す（パス、テキスト）の組 |
| `src/main.rs`・`src/diagnostic_kinds.rs` | `bindgen_command` の `--objc` の分岐と HELP（100 行・192 行）。仕様 9 の kind |

### 流れと模型

1. 引数を読み、SDK と clang の版を取る。2. JSON の AST を流して読み、framework と仕様 4 の表のヘッダーの宣言をすべて模型にする（この時点では閉包を知らない。位置と属性の節点は捨てる）。3. 一覧を読み、模型の上で閉包を求める。4. 閉包の宣言がある framework の symbol graph を読み、USR → availability の表にする。5. 写像・名前・衝突・`// skipped` を決める（名前の衝突は模型の全体で決める）。6. モジュールごとに文字列を作り、保護を確かめて一時ファイルと rename で書き（`publish` と同じ）、古いファイルを消す。7. `W2002` を出す。

```rust
struct Class {
    framework: String, header: String, super_name: Option<String>, protocols: Vec<String>,
    swift_attrs: Vec<SwiftAttr>, methods: Vec<Method>, unavailable: bool,
}
struct Method {
    selector: String, instance: bool, result: TypeText, parameters: Vec<TypeText>,
    variadic: bool, explicit_ownership: Option<Ownership>, direct: bool,
    consumed_arguments: Vec<usize>, inner_pointer: bool, unavailable: bool,
    optional: Option<bool>,              // プロトコルだけ。None は決められない（D14）
    position: (String, usize, usize),    // W2002 の位置
}
```

- `TypeText` は clang の `qualType`。写像のときに、今の bindgen の型の読み取りを ObjC の `*`・`<…>`・`^`・所有の修飾子に広げて解く。読めない綴りは `// skipped`。
- 模型は位置の節点を持たないので AppKit でも数十 MB に収まる見込み（手順 1 で測る。目標であり閾値ではない）。

### 生成物の例（M03・M04・M09 の Phase 2・M17 が無いとき。メンバーは一部）

```tsuzuri
// Generated by tsuzuri bindgen. Do not edit.
// sdk: macosx27.2 (26B118)
// clang: Apple clang version 21.0.0 (clang-2100.4.1.1)
// target: arm64-apple-macosx12.0
// api: api.txt sha256=<16 進>
// Review each extern declaration and add '@trusted' once you have checked its contract; bindgen never adds it.

const NSApplicationActivationPolicyAccessory: i64 = 1
const NSApplicationActivationPolicyProhibited: i64 = 2
const NSApplicationActivationPolicyRegular: i64 = 0

type NSApplicationActivationPolicy = i64

// NSResponder: Swift @MainActor; the default thread property until M09 Phase 2
@link(framework = "AppKit")
extern class NSResponder : Foundation.NSObject

// NSWindow: Swift @MainActor; the default thread property until M09 Phase 2
@link(framework = "AppKit")
extern class NSWindow : NSResponder {
    close :: unit -> unit
    // skipped initWithContentRect:styleMask:backing:defer:: needs M02 Phase 2
    "orderFront:" order_front :: Maybe<ref ObjC.Object> -> unit
    "setAlphaValue:" set_alpha_value :: f64 -> unit
    "setReleasedWhenClosed:" set_released_when_closed :: bool -> unit
    "setTitle:" set_title :: ref Foundation.NSString -> unit
    title :: unit -> Foundation.NSString
}
```

## 前提とする他チケットのインターフェース

- [M01](M01-objc-class-types.md): 本体のない `extern ["実行時名"] class [sync] Name : Base`、根 `ObjC.Object`、`ObjC.Class`・`ObjC.Sel`、`ref C`・`Maybe<ref C>`・`C`・`Maybe<C>`、`extern def` の +0 の結果、`@returns_retained`・`@returns_not_retained`（結果が物体でない宣言では `E1008` `objc_returns_retained_not_object`）、既定のスレッドの性質（M01 の D1）。M01 が派生と基底の性質の関係を決めていれば D3 はそれに従う。
- [M02](M02-objc-typed-message-send.md): 本体 `{ member }`、`[class] ["selector"] name :: …`、`Self`・`Maybe<Self>`、`extern category`、メンバーの `bool` と `ref mut bool`、可変長のセレクターの拒否、`ObjC.literal`。Phase 2 の `@c_layout` の値渡し。
- [M06](M06-extern-data-symbols-and-zutf8string.md): Phase 1 の `view def`・`address def`。Phase 2 の `ref ZUtf8String`（無ければ `const char *` を省く）。
- [M20](M20-macos-framework-link-inputs.md): Phase 2 の `@link(framework = "...")`（`extern class` とデータシンボルにも。git のパッケージにも書ける）。`-F` の意味。[W17](W17-link-attribute-dllimport.md): `@link("objc")`。
- 弱い依存: [M03](M03-objc-classes-defined-in-tsuzuri.md) の `extern protocol P [: Q…]`・`optional`・`ref P`・Apple のクラスの `with P, Q`（宣言した適合）、[M04](M04-objc-blocks.md) の `ObjC.Block<F>`、[M05](M05-objc-exception-boundary.md) の `Result<R, ObjC.Exception>`、[M09](M09-main-thread-dispatch-and-handles.md) の Phase 2 の `extern class main`、[M17](M17-macos-min-version-and-weak-import.md) の `@weak`、[M23](M23-objc-out-parameters-and-weak-references.md) の Phase 1 の `ref mut Maybe<C>`（物体の出力引数）と skip の理由の文、[W36](W36-c-layout-by-value.md) の Phase 2（C の関数の `CGRect` など）、[W27](W27-win32-package-from-winmd.md) の一覧の書式・共有の kind・Phase 3。W27 の配布の決定（W27 の D3・D9）は W27 自身の決定として残り、M07 の配布の形は D8（分岐 5。承認済み）で決まった。
- done: W18 の `extern type copy`、W21 の `@c_layout` と入れ子の record、W14 の同じ symbol の複数の宣言。

## 他チケットへ提供するインターフェース

- [M13](M13-desktop-os-services-package.md): 公式の `apple` パッケージ（Phase 2）。M13 は自分の Phase 1 が使う宣言（NSPasteboard・NSOpenPanel・NSUserDefaults など）を同じリポジトリの核の一覧 `api.txt` に足し、`tsuzuri bindgen --objc` で生成し直す。
- [M14](M14-tls-http-packages.md)（弱い）: Network.framework と dispatch の宣言の生成（ObjC のモードでは物体の型になる。現状 11）。
- 写像の表の拡張点: M03・M04・M05・M06 の Phase 2・M09 の Phase 2・M17・M23 の Phase 1・W36 の Phase 2 が M07 より後に入るときは、その PR で `src/bindgen_objc.rs` の写像の行を有効にし、`tests/bindgen_objc.rs` に期待を足す（`generated_modules_type_check` が食い違いを見つける）。
- `tsuzuri::bindgen_objc` の模型は写像を持たず、将来の iOS・Mac Catalyst の生成（対象外）が再利用できる形にする。

## 実装手順

### Phase 1

1. **変更**: なし（ブランチ `feat/m07-objc-bindgen`、`target/scratch/m07/`）。**内容**: `node scripts/repo-kb.mjs brief M07`。利用条件の記録。現状 1〜13 を HEAD で再現し、違えば追記する。配布物の `tsuzuri-clang`（Apple のものでない LLVM の clang）でも、SDK のヘッダーの JSON の AST と `-extract-api` が同じ形で取れるかを確かめる。4 つの framework の JSON の大きさ・最上位の要素の最大・symbol graph の大きさと時間、模型のメモリーを記録する。完了の見分け方の一覧を手で書いた宣言で `check` し、時間を測る（S3）。`-extract-api` を複数のヘッダーで使える clang の最低の版を確かめる（今の bindgen は 17 以上）。**確認**: 記録をこのチケットに書いた。
2. **変更**: `src/bindgen_objc_input.rs`。**内容**: 仕様 2 の 1〜3。**確認**: テスト計画の `splits_the_translation_unit_stream`・`tracks_elided_files_across_skipped_nodes`・`rejects_truncated_streams`・`reads_symbol_graph_availability`・`reads_swift_attr_spelling`。
3. **変更**: `src/bindgen_objc.rs`。**内容**: 模型、一覧（仕様 3）、閉包。**確認**: `listed_class_brings_bodyless_references`・`member_entries_select_one_member`・`categories_and_wildcards`・`unknown_names_are_e2000`・`closure_limit_is_e1017`・`forward_declarations_are_not_definitions`。
4. **変更**: `src/bindgen_objc.rs`。**内容**: D2 の承認の後に、名前（仕様 4）、写像（仕様 5・6）、`// skipped`（仕様 7）。**確認**: `maps_a_class_with_members` から `selector_names_and_collisions` まで（テスト計画）。期待はすべて手で書く（生成器の出力を写さない）。
5. **変更**: `src/bindgen_objc.rs`。**内容**: 書き出し（仕様 4・8）。**確認**: `generated_modules_type_check`・`generation_is_deterministic`。
6. **変更**: `src/bindgen_driver.rs`・`src/main.rs`・`src/diagnostic_kinds.rs`。**内容**: D9 の承認の後に、仕様 1・8・9。**確認**: `protects_hand_written_files`・`rejects_bad_arguments`、`tests/bindgen.rs` の `parses_bindgen_arguments`（C の経路の引数と文が変わらない）、macOS でないホストの `E2002`（`cfg` で分けた単体の検査）。`tests/bindgen.rs` と `node tests/bindgen.mjs target/debug/tsuzuri` が変わらず通る。
7. **変更**: `tests/fixtures/bindgen_objc/`・`tests/bindgen_objc.mjs`・`.github/workflows/targets.yml`。**内容**: macOS で `Fixture.h` から生成して期待と比べ（`// sdk:` と `// clang:` の行は置き換える）、完了の見分け方の一覧から生成して `check`・`build`・headless の実行（`perl -e 'alarm 15; exec @ARGV'`、終了コード 0、残ったプロセスなし）を確かめる。Linux では `skipped: needs macOS` を出して 0 で終わる。`targets.yml` の macOS のジョブに `if [ "$RUNNER_OS" = macOS ]; then node tests/bindgen_objc.mjs target/release/tsuzuri; fi` を足す（231 行の `debugger_static.mjs` と同じ形。workflow を変える前に `.github/instructions/ci.instructions.md` を読む）。**確認**: 手元で実行し、SDK の版と生成・省略の数をこのチケットに記録する。
8. **変更**: 文書。**確認**: T2、`node scripts/check-docs.mjs <変更したページ>`、`node scripts/gen-docs.mjs --check`、`node scripts/check-cli-docs.mjs`。PR に `ci:full` を付ける。

### Phase 2

1. **P2-1**: 核の一覧を下書きし、生成物を依存にした hello の `check` の時間を測る（S3）。D8 は承認済み（2026-11-26）。リポジトリは、利用条件の記録の後に名前・置き場・ライセンスを決めてから作る（D8）。
2. **P2-2**: リポジトリの CI（macOS、Xcode と `tsuzuri` の版の固定、生成、`git diff --exit-code`、headless のアプリ）。
3. **P2-3**: README（出どころ、利用条件の表示、再生成の手順、使い方）と、このリポジトリの言語リファレンスの使い方の節。

### Phase 3

- W27 の Phase 3 と同じ手順（`src/package.rs` の `Dependency::trusted`、`src/driver.rs`、`src/capabilities.rs` の `untrusted_externs`（1082 行）、`tests/audit.rs`・`tests/packages.rs`）。W27 の Phase 3 が先なら文書と README だけ。

## テスト計画

| テスト（`tests/bindgen_objc.rs`） | 内容 | 期待 |
| --- | --- | --- |
| `splits_the_translation_unit_stream` | 手で書いた JSON の流しを任意の境界で区切る | 同じ要素の列 |
| `tracks_elided_files_across_skipped_nodes` | `file` を省いた位置が、読み捨てた要素の後に続く | 宣言のヘッダーを正しく判定 |
| `rejects_truncated_streams` | 1 バイトずつ切り詰めた流し、壊した symbol graph | すべて `E2002`、パニックなし |
| `reads_symbol_graph_availability`・`reads_swift_attr_spelling` | 現状 6・9 の形（プロパティの `(py)`・`(cpy)` と `domain` が `swift` のものを含む）、範囲の外、`<scratch space>`、別の文字列 | `introduced`・`@MainActor`・`@Sendable` を読み、ほかは無し |
| `listed_class_brings_bodyless_references` ほか手順 3 の 5 つ | 一覧と閉包（`--framework` で選んでいない framework の型を含む） | 本体の有無、基底の連鎖、その framework のモジュールに出る、`E2000`・`E1017` |
| `maps_a_class_with_members` | `ObjCInterfaceDecl`・メソッド・暗黙の accessor | 手で書いた `extern class` |
| `nullability_follows_the_annotations` | `_Nonnull`・`_Nullable`・注釈なし・`instancetype _Nullable` | `ref C`・`Maybe<ref C>`・`Maybe<Self>` |
| `thread_property_inherits_main_actor` | 基底だけに `@MainActor` | M09 の Phase 2 があれば `main`、無ければ既定と注記 |
| `sendable_needs_the_class_itself` | `@Sendable` と `(_assumed)` の両方、基底だけに `@Sendable` | 自身にあれば `sync`、基底だけなら既定 |
| `explicit_ownership_attributes_only` | 暗黙の `NSReturnsRetainedAttr`（init）、明示の `NSReturnsNotRetainedAttr`、CF の型を返す関数の `CFReturnsRetainedAttr` | 物体の結果の明示のものだけが属性になり、CF の結果は注記だけ |
| `cf_types_get_owned_wrappers` | `ObjCBridgeAttr` の struct の typedef と `…Create…` | `extern type copy`・`OwnedCFString` の形の record・`Drop`・注記 |
| `forward_declarations_are_not_definitions` | 同じクラスの前方宣言（`previousDecl`、継承した属性）と定義 | 定義のメンバーと属性だけを使う |
| `ownership_breaking_members_are_skipped` | `-dealloc`、`UnavailableAttr` の `retain`、`swift` だけの不可の `+alloc` | `dealloc` と `retain` は `// skipped`、`alloc` は出す |
| `availability_marks_weak_or_notes` | 新しいクラス・関数・メンバー | M17 があれば `@weak`、無ければ注記。メンバーは注記だけ |
| `skips_with_reasons` | 仕様 7 の各行 | `// skipped` の文と `W2002` の件数 |
| `selector_names_and_collisions` | 頭字語、`+description` と `-description`、本体のある基底と本体のない基底の宣言し直し、予約語のセレクター（`class`）、一覧の違う 2 回の生成 | D2 どおり、一覧に依らず同じ名前 |
| `generated_modules_type_check`・`generation_is_deterministic` | 生成物の `check`（`tests/bindgen.rs` の `generated_modules_check_and_consume_moves_the_handle` と同じ形）、2 回の生成と一覧の入れ替え | 成功、同じバイト列 |
| `protects_hand_written_files`・`rejects_bad_arguments` | `MARKER` のないファイル、引数の誤り | `E2003`・`E2000` |
| `tests/bindgen_objc.mjs`（macOS） | 本物の SDK、`Fixture.h`、完了の見分け方 | 期待と一致、headless で 0、プロセスなし |

- Rust の検査は `tests/bindgen.rs` と同じく手で書いた JSON の AST と symbol graph を使い、clang を起動しない（どの OS でも動く）。
- `Fixture.h` は Foundation を取り込み、`NS_SWIFT_UI_ACTOR` の基底と派生、`NS_SWIFT_SENDABLE`、カテゴリー、`@optional` のあるプロトコル、`API_AVAILABLE`、`NS_RETURNS_RETAINED`、可変長、Block、`const char *`、`NSRect`、`NSError **` と `__strong` を明示した物体の出力引数、`NS_ENUM`、`extern NSString * const` を含む。期待は手で書く。物体の出力引数の期待は M23 の Phase 1 の有無で分ける: あれば `NSError **` は `ref mut Maybe<NSError>`、`__strong` のものは `// skipped`（`strong object out-parameter`）。無ければどちらも `// skipped`（`pointer to an object pointer`）。`api.txt` は `Fixture.h` の宣言だけを書き、SDK の版で期待が変わらないようにする（閉包の Foundation のクラスは本体なしで出る）。
- 既存への影響: `tests/bindgen.rs`・`tests/bindgen.mjs` は変わらない。生成の時間とメモリーは手順 1・7 で記録する（閾値は入れない）。

## ドキュメント

- `_tsuzuri/language-reference/compiler/native-interop.md`: 「C ヘッダーから extern を生成する」の後に「Objective-C のヘッダーから生成する」の節（コマンド、一覧、閉包、配置、写像、スレッドの性質・所有・availability、生成しないもの、`@trusted` を付けない理由。公式パッケージは Phase 2）。GUI の非目標の文（632 行）は変えない。M01 が作る `_tsuzuri/language-reference/compiler/objective-c.md` から、この節へリンクする。
- `_tsuzuri/language-reference/compiler/option.md`: `bindgen` の引数の段落（21 行）。`_tsuzuri/language-reference/compiler/diagnostics.md` と `docs/language.md` の診断の表は `node scripts/gen-docs.mjs` で作る。
- `docs/language.md`: 「C ヘッダーからの生成」（506 行）の後に小節。`README.md`: CLI の概要（973 行付近）と「C ヘッダーからの extern 生成」（856 行）。`docs/architecture.md`: 「bindgen」（422 行）に新しいファイルの構造と上限。
- Phase 2: `apple` の README。Phase 3: W27 と同じ（`docs/language.md` の「権限（E15）」と `docs/stability.md`）。

## 受け入れ条件

- [ ] SDK の利用条件の記録がこのチケットにある。
- [ ] Phase 1 の仕様 1〜9 のとおりに動き、`tests/bindgen_objc.rs` が通る（`running N tests` を確かめた）。
- [ ] 切り詰めた・壊した入力でパニックせず、`unsafe` と新しい crate を使っていない。生成が決定的で、手書きのファイルを上書きしない。
- [ ] 生成物が `check` を通り、完了の見分け方のアプリが macOS で headless で 0 を返し、プロセスが残らない。SDK の版と生成・省略の数を記録した。
- [ ] C のヘッダーからの `tsuzuri bindgen` の出力と診断が変わらない（`tests/bindgen.rs`・`node tests/bindgen.mjs target/debug/tsuzuri`）。
- [ ] 生成器を使わないプログラムの IR・object・header・診断・既定の `.wasm` が変わらない（`tests/fixtures/*`・`examples/*` の `--emit llvm` と `check --json` を着手前と比べた）。
- [ ] `node scripts/check-cli-docs.mjs`・`node scripts/gen-docs.mjs --check`・T2 が通る。PR に `ci:full` を付けた。
- [ ] Phase 2: 公式パッケージの CI が生成の差なしと headless のアプリを確かめた。Phase 3: `trusted = true` を書かないプロジェクトの `tsuzuri audit` の text と JSON が 1 byte も違わない。

## 落とし穴

- **`file` の省略**: 読み捨てる要素の中の位置も `Tracker` に通さないと、後の宣言のヘッダーを取り違える。
- **`NS_SWIFT_SENDABLE` は既定で空**: `-D__SWIFT_ATTR_SUPPORTS_SENDABLE_DECLS=1` を忘れると `sync` が 1 つも出ない（現状 7）。`Fixture.h` で `sync` の行を確かめる。`@_nonSendable(_assumed)` は監査の `#pragma` が広く付けるので判定に使わない。
- **`@MainActor` は継承する**: NSWindow 自身には付かない（現状 6）。`@Sendable` は自身だけで判定する（D3）。
- **dispatch の型**: ObjC のモードでは ObjC の物体になり、`__strong` と `ns_returns_retained` が付く（現状 11）。C の経路と結果が違ってよい。
- **`objc_direct`**: 宣言しても送ると「unrecognized selector」になる。必ず省く。
- **プロパティの `getter=`**: セレクターは暗黙の getter の名前（`isVisible`）を使う。
- **availability とメソッド**: メソッドは symbol ではないので `@weak` にならない。古い OS で新しいメソッドを送ると ObjC の例外で abort する。
- **閉包の爆発**: 参照するクラスを本体ごと足すと AppKit のほぼ全体になる（D11）。基底のメンバーを使うには基底も一覧に書く（完了の見分け方の `NSObject`）。
- **`releasedWhenClosed`**: NSWindow の既定は YES で、M01 の所有と組むと `close` の後に二重に release されうる。例と公式パッケージの例では `set_released_when_closed false` を書く。
- **予約名と SDK の違い**: M01・M09 で `ObjC`・`MainThread` が予約名に増える。後から増えると生成物が `E1011` になるので、公式パッケージの CI が新しい `tsuzuri` で生成し直す。Command Line Tools と Xcode の SDK は版が違いうるので、先頭の `// sdk:` で区別する。
- **`W2002` の位置**: 省いた宣言は多くのヘッダーに散る。今の `print_diagnostic` は 1 つのヘッダーしか取らない。
- **`-fobjc-arc` を外さない**: 外すと `retain`・`release`・`autorelease` が不可でなくなり、M01 の所有を壊すメンバーが生成される（現状 13）。`-dealloc` は `-fobjc-arc` でも不可にならないので、仕様 7 で名指しで省く。
- **Swift だけの不可**: `OBJC_SWIFT_UNAVAILABLE`（`+alloc`）・`NS_SWIFT_UNAVAILABLE` は `availability(swift, unavailable)` で、macOS では使える。symbol graph の `domain` が `swift` のもので省かない。JSON の `AvailabilityAttr` は引数（プラットフォーム）を持たない（現状 5）ので、その節点があることだけで省かない。
- **メンバーの単位の `NS_SWIFT_UI_ACTOR`**: `NSWindowDelegate` のメソッド（`NSWindow.h:831-834`）などに付く。スレッドの性質はクラスの単位だけなので写さない。
- **同じクラスが 2 つのモジュールにある**: 利用者の生成物と公式パッケージの両方に `NSString` があると別の型になる（M01）。公式パッケージを使うなら、足りない宣言は同じパッケージに生成する（M13 と同じ）。

## 対象外

- Swift の `.swiftinterface` からの生成と SwiftUI（D-67 は Swift の ABI を扱わない）。iOS・Mac Catalyst。x86_64 の macOS（M18。D-67 の分岐 3 で「5 つのプラットフォーム」の方針を改めないと決まり、M18 は着手しない）。
- 軽量ジェネリクスの型引数（消して基底のクラスにする）。
- macro だけの API（`NSLocalizedString`、inline の `dispatch_get_main_queue`）。整数の `#define` を一覧から読むことは後の改善とする。
- std への同梱（非目標）。宣言の上の安全な包みと、公式パッケージの宣言に人が `@trusted` を付けていく作業。
- C のヘッダーからの `tsuzuri bindgen` の改善（`bool` の綴り、ほかのヘッダーの struct の参照）。

## 決定事項

- **D1（既定案）入力**: 決定: 仕様 2（JSON の AST を要素ごとに流して読み、availability は `-extract-api` の symbol graph、`swift_attr` の引数は spelling の範囲のバイトから取る。`-D__SWIFT_ATTR_SUPPORTS_SENDABLE_DECLS=1` を渡し、SDK の版を記録する）。採らない案: (a) JSON を 1 つの `Value` で読む（AppKit で上限を超える）。(b) `-ast-dump-filter` を名前ごとに（部分一致で何度も起動する）。(c) `-fmodules`（取り込んだ宣言を出さない）。(d) 文の AST（機械向けでなく、名前と位置の綴りが省略され、JSON との照合が実行ごとのアドレスでしかできない）。(e) symbol graph だけ（nullability と属性が落ちる）。(f) ヘッダーの独自の解析（マクロと `#pragma clang attribute` の再実装。S1）。(g) `.tbd`（symbol の名前だけ）。理由: JSON の AST は型・nullability・族・継承を構造で持ち、足りない 2 つの引数（現状 5）を clang 自身の構造化された出力と、前例のある範囲の読み取り（`nonnull_attr_indices`）で補える。
- **D2（要承認）名前の規則**: 決定: 仕様 4 の名前（セレクターの片ごとの snake_case を `_` でつなぎ、頭字語の境界を切る。衝突は `class_` と `_2`・`_3` で、一覧に依らず模型の全体で決める。最後に `value_name`。ObjC のモードの C の関数とデータシンボルも同じ snake_case）。採らない案: (a) Swift の名前（`NS_SWIFT_NAME` と自動の短縮）。(b) 最初の片だけ（M02 の例の `init_with_content_rect`。規則で決まらず衝突が多い）。(c) C の経路の `snake_case`（`nsdefault_run_loop_mode` と読みにくい）。理由: セレクターから機械的に戻せ、Apple の文書で引ける。(a) は AST に引数が無く（現状 5）、自動の短縮の再実装が大きい。名前は公開の API で後から変えられないので要承認。承認前は手順 4 に着手しない。
- **D3（既定案）スレッドの性質**: 決定: 仕様 6（`@MainActor` は基底の連鎖で継承して `main`、クラス自身の `@Sendable` は `sync`、M09 の Phase 2 が無ければ既定と注記）。採らない案: `@Sendable` も継承する。理由: `@MainActor` の継承は Swift の規則で、AppKit の派生は NSResponder の注釈に頼る（現状 6）。派生は可変の状態を足しうるので、`@Sendable` は自身の明示だけを信じる。M01 が派生と基底の性質の関係を決めていれば合わせる。
- **D4（既定案）所有**: 決定: 仕様 6（結果が ObjC の物体のときだけ明示の属性を写す。CF の型は `extern type copy` と `OwnedCFString` の形の record・`CFRelease` の `Drop`、範囲は `ObjCBridgeAttr`・`ObjCBridgeMutableAttr` の struct の typedef。CF の結果の +1 は注記だけ）。採らない案: Create と Copy の名前から結果の型を `OwnedCFString` にする。CF の結果にも `@returns_retained` を付ける（M01 の `objc_returns_retained_not_object` で `check` が通らない）。理由: 族の規則は M01・M02 が当てる。CF の規則は名前の慣習で、AST は関数ごとの所有を明示しない（W27 の D6 と同じ）。
- **D5（既定案）availability**: 決定: 仕様 6（symbol だけ `@weak`、メンバーは注記、使えないものは省く）。採らない案: 新しいメンバーを省く。理由: `@weak` は symbol にだけ意味がある。省くと新しい OS だけを対象にする利用者が手で書くことになる。
- **D6（要承認。W27 と共有）`@trusted`**: 決定（推奨）: 生成した宣言に `@trusted` を付けず、`TRUSTED_NOTE` で確認を促し、`W1010` は Phase 3 のルートの印（W27 の D5）で消す。採らない案: 生成器が付ける、生成物の中の印で消す。理由: `@trusted` は人が確かめた印（E15）で、SDK は寿命・スレッド・例外の契約のすべてを表さない。ファイルの中の印は自己申告になる。承認前は Phase 3 に着手しない。
- **D7（承認済み（2026-11-26。D-67 の分岐 1））D-67 の分岐 1**: 決定: X′（`extern class`・`extern category`・`extern protocol`）を前提にする。選ばれなかった案での扱いは、Y′ ならこのチケットを閉じ、Z′ なら `objc_msgSend` の型付きの宣言を生成する形に計画し直す、だった。
- **D8（承認済み（2026-11-26。D-67 の分岐 5））D-67 の分岐 5（配布の形）**: 決定: std の外の公式の git パッケージ `apple`（`namespace = "Apple"`）で、中身は核の宣言だけ。リポジトリの名前・置き場・ライセンスは利用条件の記録の後に決める。採らない案: SDK の全体を 1 つのパッケージにする（依存の全モジュールを毎回型検査する。現状 12）、std にする（非目標）。起票のときは W27 の D3・D9 と共有する決定としたが、承認では W27 の D3・D9 は W27 自身の決定として残った（W27 は変えない）。
- **D9（要承認）D-55 の登録**: 決定: サブコマンドの形 `tsuzuri bindgen --objc` と引数 `--framework`・`-F`（M20 と同じ綴りと意味）・`--api`（W27 と共有）・`--min-version`、kind 案 `bindgen_unknown_framework`・`bindgen_apple_sdk_missing`（と W27 と共有の `bindgen_unknown_api`・`bindgen_api_limit`）。採らない案: 別のサブコマンド（`tsuzuri objc-bindgen`）、Tsuzuri が `SDKROOT` を読む（`xcrun` が読む）。理由: 既存の `bindgen` の約束（`MARKER`・`TRUSTED_NOTE`・`// skipped`・`W2002`・保護）をそのまま使える。承認前は手順 6 に着手しない。
- **D10（既定案）一覧の書式**: 決定: 仕様 3 の表（W27 の書式に `-[Class sel]`・`+[Class sel]`・`Class(Category)`・`@protocol Name`・`catch ` を足す）。採らない案: クラスの名前だけ（メンバーを選べない）、独自のセレクターの綴り。理由: `mangledName` は JSON にそのままあり、ObjC の文書と同じ綴り。`NSObject` はクラスとプロトコルの両方の名前なので `@protocol` で分ける。
- **D11（既定案）閉包**: 決定: 仕様 3（一覧のものは本体ごと、参照するクラスとプロトコルは本体なし、基底の連鎖は根まで）。採らない案: 参照するクラスも本体ごと（AppKit のほぼ全体になる）。理由: M01 の本体のない宣言で型の参照は足りる。依存の全モジュールの型検査の費用を抑える。
- **D12（既定案）モジュールの配置**: 決定: 仕様 4（framework ごとに 1 つ、固定の表、予約名は `_`）。採らない案: ヘッダーごと（数百のモジュールと循環の参照）、全部で 1 つ（framework と `@link` の対応が消える）。理由: framework は `@link(framework = …)` の単位で、利用者が探す単位でもある。
- **D13（既定案）型の選択**: 決定: 仕様 5 の表（メンバーの `BOOL` は `bool`、C の関数は今の表、M03 が無いときの `id<P>` は `ObjC.Object`、型引数は消す、`NSError **`・長さのないポインター・可変長は省く。`NSError **` は [M23](M23-objc-out-parameters-and-weak-references.md) の Phase 1 が入れば `ref mut Maybe<NSError>` に写す）。採らない案: `id<P>` のメソッドを省く。理由: `id<P>` は ABI と所有では `id` と同じで、失うのは静的な検査だけ。M23 の Phase 1 の前は、M01・M02 に `NSError **` を写す出力の物体の形が無い。M23 の後も内側が `__strong`・`__unsafe_unretained` のものを省くのは、pass-by-writeback の所有の規則と合わないから（`__strong` は二重の release になり、`__unsafe_unretained` は物体のバッファでありうる。M23 の落とし穴）。
- **D14（要承認）プロトコルの `@optional`**: 決定（推奨）: `ObjCProtocolDecl` の `range` の始まりからメソッドの `loc` までのバイトを字句で走査し（注釈と文字列を飛ばす）、最後の `@optional`・`@required` で決める。`#pragma`（`#` の後の空白を許す。現状 8）以外の前処理の行がある、またはメソッドの `loc` の spelling がプロトコルと同じファイルの範囲にない（マクロの展開など）ときは決められないとし、プロトコルを `// skipped` にする。使うのはプロトコルを出すとき（M03）と、採用したプロトコルの required のメンバーをクラスに混ぜるときだけ。採らない案: (a) すべて `optional`（required を実装しないクラスが通り、送られて落ちる）。(b) すべて required（AppKit は `respondsToSelector:` で delegate の振る舞いを変えるので、実装の強制は意味を変える）。(c) libclang の `clang_Cursor_isObjCOptional`（C の API で、`unsafe` の FFI か新しい crate が要る。GUIDE §1 規則 10）。(d) プロトコルを常に省く（承認されないときの形。M03 の delegate をすべて手で書く）。理由: clang の JSON の AST・文の AST・`-ast-print`・`-extract-api` のどれも `@optional` を持たない（現状 8）。走査は AST が与える範囲に限り、AppKit と Foundation の本体の前処理の行は `#pragma` だけだった。ただし、これは停止条件 S1（ヘッダーを独自に解析したくなった）の狭い例外なので、人の承認が要る。承認前は (d) で進める。
- **D15（既定案）テストの入力**: 決定: Rust の検査は手で書いた JSON の AST と symbol graph で clang を起動しない。本物の SDK は macOS の node の検査だけ。採らない案: SDK のヘッダーや clang の出力をリポジトリに入れる（大きく、利用条件が未確認）。理由: `tests/bindgen.rs` と同じ形でどの OS でも動く。期待は手で書く（GUIDE-LITE §2 規則 10）。
