# M06: C の大域データシンボルの `extern` と NUL 終端の UTF-8 文字列 `ZUtf8String`

| 項目 | 内容 |
| --- | --- |
| ID | M06 |
| 優先度 | P1 |
| 規模 | L（Phase 1・2。人間が求めない限り Phase 1 だけを実装する） |
| 依存 | ([M01](M01-objc-class-types.md)), ([NS07](NS07-extern-addr-nonnull-view.md))（M01 は ObjC の型の値を読む段、NS07 は `address` の結果に `ExternAddr` を許す段） |
| 後続 | [M07](M07-apple-sdk-bindgen-and-package.md)（弱い後続: [M17](M17-macos-min-version-and-weak-import.md)） |
| 状態 | todo |
| 起票 | 2026-11-26（D-67。利用者の依頼「「どのチケットにもない壁」と「調査中に見つけたコンパイラの不具合（自分で再現済み）」のそれぞれについて @_features/ に開発チケットまたは修正チケットを作成して」から起票。調査で報告した「壁 1・M04・M09 の前提（`NSDefaultRunLoopMode`・`_dispatch_main_q`・`_NSConcreteGlobalBlock`・NSString の定数は `dlsym` と extern view でしか読めない。`const char*` には明示の NUL と `@count(n, none)` が要る）」に当たる。HEAD `804b7970` で確認） |
| 承認 | 要承認: D-55（`extern "sym"` の後の文脈キーワード `address`、`extern "sym" view def` の受理、Phase 2 の std のモジュール `ZUtf8String`（`RESERVED_MODULES` と `stdlib::OPT_IN`））。決定事項 D1（綴り。D-55）・D2（名前）・D3（結果の `const char *`）が要承認、ほかは既定案。[GUIDE D-67](GUIDE.md#d-67-tsuzuri-だけで-macos-のデスクトップアプリを書くm01m22) の分岐 1 とは独立で、Y′ でも実装する |
| 改善する劣位 | C・Objective-C・Swift 比: C の大域データシンボル（`NSDefaultRunLoopMode`・`kCFAllocatorDefault`・`kCFCoreFoundationVersionNumber`・`_dispatch_main_q`）を宣言できず、`dlsym` と extern view を重ねるしかない。NUL 終端の UTF-8（`const char *`）の型がなく、`objc_getClass`・`sel_registerName`・`getenv` に明示の `\0` と `@count(n, none)` を手で書く |
| 手本にする既存実装 | [W24](_completed/W24-extern-view.md) の extern view（`src/parser.rs` の `extern_declaration` の `view` の枝、`src/check.rs` の `HostImport.view`、`src/llvm_imports.rs` の `extern_view`、`src/llvm_abi.rs` の `read_host_record`、`src/llvm.rs` の `validate_wasm_externs`・`extern_symbols`）、[W20](_completed/W20-zstring.md) の `ZString`（`std/ZString.tz`、`src/stdlib.rs` の `OPT_IN`・`RESERVED_MODULES`・`opaque_record`、組み込み `ZString.literal`、`abi::zstring`）、[NS01](_completed/NS01-nonnull-extern-handles.md) の `TrapKind::NullHandle`、[W15](_completed/W15-abi-nullable-handles.md) の `nullable_from_c` |
| 主な影響ファイル | `src/syntax.rs`, `src/parser.rs`, `src/syntax_codec.rs`, `src/frontend_cache.rs`, `src/formatter.rs`, `src/api_diff.rs`, `src/check.rs`, `src/abi.rs`, `src/llvm.rs`, `src/llvm_imports.rs`, `src/llvm_abi.rs`, `src/effects.rs`, `src/pure_rules.rs`, `src/pinned_rules.rs`（Phase 2）, `src/capabilities.rs`, `src/diagnostic_kinds.rs`, `src/explain/codes/E1008.md`, `src/stdlib.rs`, `std/ZUtf8String.tz`（新規。Phase 2）, `tests/data_symbols.rs`（新規）, `tests/data_symbols.mjs`（新規）, `tests/data_symbols_host.c`（新規）, `tests/zutf8string.rs`（新規。Phase 2）, `tests/std_argument_order.rs`, `tests/std_examples.rs`, `vsc/src/core.ts`, `docs/language.md`, `docs/architecture.md`, `_tsuzuri/language-reference/compiler/native-interop.md`, `_tsuzuri/language-reference/built-in-types-and-modules/zutf8string.md`（新規。Phase 2）, `_tsuzuri/language-reference/index.md` |

## 実装者向け早見

- 範囲:
  - Phase 1: C の大域データシンボルを `extern` の宣言で読む。値を読む形 `@trusted extern "sym" view def f :: unit -> T` と、アドレスを取る形 `@trusted extern "sym" address def f :: unit -> H`。呼び出しの地点で、`@sym = external global [0 x i8]` からの `load` か `ptr @sym` に下がる。W24 の `extern_view_link_name` は、アドレスを取る view にだけ残す。
  - Phase 2: opt-in の std のモジュール `ZUtf8String`（NUL 終端の UTF-8）。`ref ZUtf8String` は `const char *` 1 つ、`Maybe<ref ZUtf8String>` の `None` は NULL。結果の `const char *` は D3（要承認）。
- **人間が求めない限り Phase 1 だけを実装する。** [M07](M07-apple-sdk-bindgen-and-package.md) が要るのは Phase 1 だけで、Phase 2 がなければ M07 は `const char *` の引数の関数を今のとおり飛ばす。
- 最初に読む: 「現状」→ 仕様 1〜5 → `src/parser.rs:4871-4904`（`view` の枝）→ `src/check.rs:8604-8613`・`9525-9556` → `src/llvm_imports.rs:31-63`。
- 最初に書くテスト: `tests/data_symbols.rs` の `parses_view_and_address_forms`（`let address = 1` が識別子のまま読めることを含む）→ `lowers_to_one_external_global`。
- 触ってはいけない: W24 の extern view の意味・IR（D8 の文を除く）、`extern def` の ABI、データシンボルを使わないプログラムの IR・object・header・診断・既定の `.wasm`（1 byte も変えない）。
- 完了の見分け方（Phase 1）: 次を macOS で `-L fw -l Foundation -l CoreFoundation`（`fw` は SDK の `Foundation.tbd`・`CoreFoundation.tbd` を `libFoundation.tbd`・`libCoreFoundation.tbd` として指すシンボリックリンクの置き場。[M20](M20-macos-framework-link-inputs.md) の後は `--framework`）でビルドすると、`-O0` と `-O3` で `length=21 allocator=none version=true label=21` を出して 0 を返す。`dlsym` を使わない。

```tsuzuri
extern type copy CFString
extern type copy CFAllocator
extern type copy Queue

@trusted
extern "NSDefaultRunLoopMode" view def default_run_loop_mode :: unit -> CFString
@trusted
extern "kCFAllocatorDefault" view def allocator_default :: unit -> Maybe<CFAllocator>
@trusted
extern "kCFCoreFoundationVersionNumber" view def cf_version :: unit -> f64
@trusted
extern "_dispatch_main_q" address def main_queue :: unit -> Queue

@trusted
extern "CFStringGetLength" def cf_string_length :: CFString -> i64
@trusted
extern "dispatch_queue_get_label" def queue_label :: Queue -> i64
@trusted
extern "strlen" def strlen :: i64 -> i64

def main :: unit -> i32 = \() ->
    let allocator = match allocator_default () with
        | Maybe.Some _ -> "some"
        | Maybe.None -> "none"
    let length = cf_string_length (default_run_loop_mode ())
    let label = strlen (queue_label (main_queue ()))
    do! IO.writeln $"length={length} allocator={allocator} version={cf_version () > 0.0} label={label}"
    0
```

`kCFAllocatorDefault` の値は NULL なので `Maybe` で受ける。21 は `kCFRunLoopDefaultMode` の文字数と `com.apple.main-thread` のバイト数で、同じ数は偶然である。

## 目的

C のライブラリは関数だけでなく大域変数を公開する。macOS の GUI に要るものだけでも、run loop の mode の名前 `NSDefaultRunLoopMode`（`NSString * const`）、メインキュー（`dispatch_get_main_queue()` はインライン関数で、中身は `&_dispatch_main_q`）、Block の isa `_NSConcreteGlobalBlock`、`kCFAllocatorDefault` がある。Tsuzuri にはこれを宣言する構文がなく、どのチケットにも計画がない。今は `dlsym(RTLD_DEFAULT, "name")` でアドレスを得て W24 の extern view で読む（「現状」）。この回避策は、`RTLD_DEFAULT` が Darwin で `(void *)-2` だという知識と、手で書く `\0` に頼り、リンク時に何も検査されない。

もう 1 つの壁は NUL 終端の UTF-8 である。ObjC の実行時の API と POSIX の多くの関数は `const char *` を取る。W20 の `ZString` は UTF-16 で、W20 は UTF-8 の NUL 終端を対象外にした（`_features/_completed/W20-zstring.md:167`）。`ref utf8string` は終端の 0 を持たない。

このチケットは、Phase 1 でデータシンボルを `extern` の宣言として書けるようにし、Phase 2 で W20 と同じ形の `ZUtf8String` を足す。生ポインター型・ポインター演算・`unsafe` は入れない（D-63・D-67）。足す形は `extern` の宣言で、`@trusted` と権限 `ffi`（D-57）の監査の対象になる。

## 着手条件と停止条件

### 着手条件

- `node scripts/repo-kb.mjs brief M06` を読む。**人間が求めない限り Phase 1 だけを実装する。**
- 承認が記録されるまで、該当する Phase に着手しない。Phase 1 の手順 2 以降は D1、Phase 2 は D2、仕様 8 は D3 の承認が要る。
- 依存はどちらも弱い。[M01](M01-objc-class-types.md) が done なら仕様 2 の ObjC の型の行も実装し、後なら M01 が足す。[NS07](NS07-extern-addr-nonnull-view.md) が done なら `address` の結果に `ExternAddr` を許し、後なら NS07 が足す。確認: `grep -nE "^\| (M01|NS07) " _features/README.md`。
- 「現状」の再現と行番号を着手時の `main` で確かめ直す。
- 基準の保存: `tests/fixtures/*/Main.tz` と `examples/*/Main.tz` の `--emit llvm -O1`（native と `--target wasm32`）、`--emit header`、`check --json` を `target/scratch/m06/base/` に保存する。

### 停止条件

- **S1** データシンボルを使わないプログラムの IR・header・診断が、基準と 1 byte でも違う（D8 の文を除く）。
- **S2** `HostImport.data` を足しても、`HostCall` を見る場所（`grep -rn "HostCall\|native_symbol\|\.view" src/`）で、データシンボルを関数として扱う経路（`declare`・ヘッダー・TZ08 の包み・W14 の `explicit_symbols`・バインディング）を閉じられず、新しい `TypedExprKind` を足したくなった。
- **S3** コンパイラが生成した IR を Apple の ld か同梱の zig（lld）でリンクすると、dylib・共有ライブラリのデータを読めない（手で書いた IR では両方とも読めた。「現状」の「参照: clang の IR」）。または `--backend fast` が別の値を読む（既存の `E2002` で止まるのは許す）。
- **S4** `ZUtf8String` を常に読み込むモジュールにしないと実装できない（W20 の落とし穴）。
- **S5** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `804b7970` で確認）

### 構文がない

`grep -rnE 'extern (let|const|static)|データ(の)?シンボル' _features docs` で計画は 0 件。次の 3 つ（それぞれに `def main :: unit -> i32 = \() -> 0` を足す）を `target/debug/tsuzuri check` にかけた。

```tsuzuri
// syn1
@trusted
extern "NSDefaultRunLoopMode" view def default_run_loop_mode :: unit -> i64
// syn2
extern type copy Queue

@trusted
extern "_dispatch_main_q" address def main_queue :: unit -> Queue
// syn3
@trusted
extern view def default_run_loop_mode :: unit -> i64
```

```text
syn1/Main.tz:2:8: error[E0002]: an extern view copies memory and links no symbol; remove the string
syn2/Main.tz:4:27: error[E0002]: expected 'def' after extern
syn3/Main.tz:2:17: error[E1008]: an extern view is 'i64 -> R' to read or 'i64 -> ref R -> unit' to write, with R a record
```

`check --json` の kind は順に `extern_view_link_name`・`expected_token`・`extern_view_signature`。

### 今の書き方: `dlsym` と extern view

macOS 27.2.1（arm64）で `target/scratch/m06/fw/` に `libFoundation.tbd`・`libCoreFoundation.tbd`（SDK の `.tbd` へのシンボリックリンク）を置き、次を `target/debug/tsuzuri build dl -L fw -l Foundation -l CoreFoundation -o dl-bin && ./dl-bin` にした。`-O0` と `-O3` で `address=true length=21 queue=true`、終了コード 0。

```tsuzuri
@c_layout
record Word { value: i64 }

@trusted
@count(2, none)
extern "dlsym" def dlsym :: i64 -> ref utf8string -> i64
@trusted
extern view def read_word :: i64 -> Word
@trusted
extern "CFStringGetLength" def cf_string_length :: i64 -> i64

def main :: unit -> i32 = \() ->
    // RTLD_DEFAULT is (void *)-2 on Darwin.
    let address = dlsym (0 - 2) (ref u8"NSDefaultRunLoopMode\0")
    let mode = (read_word address).value
    let queue = dlsym (0 - 2) (ref u8"_dispatch_main_q\0")
    do! IO.writeln $"address={address != 0} length={cf_string_length mode} queue={queue != 0}"
    0
```

- 同じ形で `kCFAllocatorDefault` は 0、`kCFCoreFoundationVersionNumber`（`double`。`@c_layout record Real { value: f64 }` で読む）は `5027.1` だった。
- `dlsym` で得た `_dispatch_main_q` を `dispatch_async_f` に渡し、`CFRunLoopRunInMode(mode, 1.0, true)` を呼ぶと、静的コールバックがメインキューで呼ばれ、戻り値は 4（`kCFRunLoopRunHandledSource`）だった。調査の probe E6 では hardened runtime の署名でも終了コード 0。
- アドレスはただの `i64` で、型による保護もリンク時の検査もない。`dlsym` は読み込まれていない image の symbol を見つけない。

### 参照: clang の IR

`extern struct dispatch_queue_s _dispatch_main_q; extern void *const NSDefaultRunLoopMode; extern const double NSAppKitVersionNumber; extern const void *const kCFAllocatorDefault;` を読む C の関数を `clang -target arm64-apple-macosx12.0 -O1 -S -emit-llvm`（Apple clang 21）にした。

```llvm
@_dispatch_main_q = external global %struct.dispatch_queue_s, align 1
@NSDefaultRunLoopMode = external local_unnamed_addr constant ptr, align 8
@NSAppKitVersionNumber = external local_unnamed_addr constant double, align 8
@kCFAllocatorDefault = external local_unnamed_addr constant ptr, align 8
  ret ptr @_dispatch_main_q
  %1 = load ptr, ptr @NSDefaultRunLoopMode, align 8, !tbaa !6
  %1 = load double, ptr @NSAppKitVersionNumber, align 8, !tbaa !10
```

SDK: `dispatch/queue.h:625-631`（`dispatch_get_main_queue` は `DISPATCH_GLOBAL_OBJECT(dispatch_queue_main_t, _dispatch_main_q)` を返す）、`NSRunLoop.h:16`、`NSApplication.h:36`、`CFBase.h:639`。

D7 の形（不透明な `[0 x i8]` の宣言）も手で書いた IR で確かめた。次の宣言から `load double`・`load i32`・`ptr @m06_queue` を作る `main` を `clang -O3` で作り、dylib（`int m06_value = 20;`、`double m06_version = 5027.1;`、`struct q { long a; } m06_queue;`、`m06_value` を 22 に書き換える `poke`、`&m06_queue` を返す関数）とリンクした。

```llvm
@m06_value = external global [0 x i8]
@m06_version = external global [0 x i8]
@m06_queue = external global [0 x i8]
```

- Apple の ld でも、VS Code 拡張 0.1.6 の同梱の `tsuzuri-clang`（zig の lld）でも `5027.1 42 1` だった。`poke` の前後の 2 回の `load` は畳まれず（20 + 22）、`ptr @m06_queue` は C の `&m06_queue` と一致した。
- 同じ形の `@kCFCoreFoundationVersionNumber = external global [0 x i8]` は、`-framework CoreFoundation` とのリンクで `5027.1` を読んだ。大きさ 0 の宣言からの `load` を、`-O3` は消さなかった。

### コード

- `src/parser.rs:4807-4905` の `extern_declaration` は、リンク名（`link_name`、4908 行）の後に `type`・`view def`・`def` だけを受ける。`view` の枝（4871-4885 行）は、署名を読む前にリンク名があれば `ExternViewLinkName`（4873-4880 行、位置はリンク名の文字列）を返す。署名は 4897 行、`signature.link`・`view` は 4902-4903 行（`src/syntax.rs:758-760`）。
- `src/check.rs:8604-8613`: view の `HostImport` は `native_symbol`・`wasm_module`・`wasm_name` が空で、`explicit`・`callbacks` が偽。`src/check.rs:9525-9556`: view の署名は `[i64] -> Record` か `[i64, ref Record] -> unit` だけで、record は `abi::view_record`（`src/abi.rs:385-457`）。明示のリンク名は `abi::link_name_error`（`src/abi.rs:58`。C の識別子、`RESERVED_SYMBOL_PREFIXES`（55 行）と `RESERVED_HOST_SYMBOLS`（13 行））を通り、同じ symbol は W14 の `explicit_symbols`（`src/check.rs:9705-9752`）で照合する。
- `src/llvm_imports.rs:31-63` の `extern_view` の読む形は `inttoptr` → `host_pointer`（`src/llvm_abi.rs:987`。NULL と整列を `BoundsCheck` で検査）→ `read_host_record`（`src/llvm_abi.rs:1170`）。handle の field は `NullHandle`（`src/llvm_abi.rs:1209-1214`）、`extern def` の結果の handle も `NullHandle`（`src/llvm_imports.rs:437-440`）。
- view を関数として扱わない場所: `src/llvm.rs:1801-1812`（`extern_symbols`、TZ08）、`2153-2154`（関数本体を出さない）、`src/llvm_imports.rs:12`（ヘッダー）、`src/llvm.rs:4165` の `validate_wasm_externs` の 4224-4229 行（WASM の `AbiNativeOnly`）。
- kind: `extern_view_link_name`（`src/diagnostic_kinds.rs:102`、`E0002`）、`abi_native_only`（286、`E1008`）、`conflicting_extern_declaration`（297、`E1008`）、`extern_view_signature`（308、`E1008`）。view の `@count` は `E0002` `misplaced_count_attribute`、性質付きの `@trusted` は `E1037` `trusted_view_property`（開発版で確かめた）。
- PB07 の分割は文字列にある宣言 `@x = external global i32` をそのまま写す（`src/llvm_split.rs:391`）。`--backend fast` も初期化子のない global を持てる（`src/backend/module.rs:78`）。

### NUL 終端の UTF-8

- `strlen` を `@count(1, none) extern "strlen" def strlen :: ref utf8string -> i64` と宣言し、`strlen (ref u8"abc")` と `strlen (ref u8"abc\0")` を呼ぶ `main` を `--emit llvm` にすると、`@tz.literal.0 = private unnamed_addr constant [3 x i8] c"\61\62\63"` と `@tz.literal.1 = … [4 x i8] c"\61\62\63\00"` が出る。前者を `strlen` に渡すのは範囲外の読み出しである。
- 今動く書き方（probe E2・E3d）: `@count(1, none)` の `ref utf8string` に `\0` を含むリテラルを渡す（`objc_get_class (ref u8"NSProcessInfo\0")`）。途中の NUL は検査されない。
- `tsuzuri bindgen $SDK/usr/include/objc/runtime.h -o Runtime.tz` は 4 個の関数と 6 個の `extern type` を出し、114 個を飛ばす（`Runtime.tz` の `// skipped` の行の数）。理由が `const char *` のものは 14 個（引数 9、結果 5。関数ごとに最初の不支持の位置だけを報告する）。例 `// skipped objc_getClass: parameter 1 has unsupported type 'const char * _Nonnull'`。残りの多くは `Class`・`id`・`Protocol *`（M07 の範囲）。
- `const char *` の結果は `i64` で受け、`strlen` と `@count(1, none) extern "memcpy" def memcpy :: ref mut [ubyte] -> i64 -> i64 -> i64` で写し、`Utf8String.from_bytes` で戻すしかない（`getenv` で確かめた。`M06_TEXT=こんにちは` で 15 バイトを読んだ）。
- `ZString`（W20）: `std/ZString.tz` の不透明な `record ZString { units: [i16u] }`、`src/stdlib.rs:166`（`RESERVED_MODULES`）・`232`（`opaque_record`）・`473-478`（`OPT_IN`）、`src/abi.rs:497-506` の `abi::zstring`（名前で見分ける）、`tests/std_argument_order.rs:161`、`tests/std_examples.rs:88`、`vsc/src/core.ts:443`。`RESERVED_MODULES` は 78 個（`src/stdlib.rs:903`）。WASM の `ref ZString` の extern は `abi_native_only`（W20:90）。
- 変換: `Utf8String.from_string`（`src/check.rs:2698`）、`Utf8String.to_bytes :: utf8string -> [i8u]`・`from_bytes :: [i8u] -> Maybe<utf8string>`（`src/check.rs:3253-3265`）。
- 字句: UTF-16 の文字列リテラルは対のないサロゲートを持てる。`let s = "a\u{D800}b"` は受理され、`s.length` は 3 だった。`u8"\u{d800}"` は `E0001`（`src/lexer.rs:1524` の `rejects_invalid_unicode_without_replacing_surrogates`）。

## 変えてはいけない意味

- W24 の extern view の構文・型・IR・トラップ・`tsuzuri audit` の出力。例外は、リンク名を書いたアドレスを取る view の `E0002` の文だけで（D8）、コード・kind・位置は保つ。
- `extern def` の ABI（`ref utf8string` は `ptr` と `i64`、`@count(n, none)` は `ptr` だけ）、W14 の照合、予約された symbol。`string`・`utf8string`・`ZString` の表現と ABI。
- データシンボルと `ZUtf8String` を使わないプログラムの出力。`address` は `extern "sym"` と `def` の間でだけ語になり、ほかでは識別子のまま。

## 仕様

### Phase の範囲

| Phase | 範囲 | 承認 |
| --- | --- | --- |
| 1 | 仕様 1〜5（データシンボルの 2 つの形、W24 の拒否を狭める、照合、target） | D1 |
| 2 | 仕様 6〜8（`ZUtf8String`、`ref ZUtf8String` の ABI、結果の `const char *`） | D2（仕様 8 は D3 も） |

### Phase 1

#### 1. 構文

```text
data_symbol = [ doc ] { attribute } [ "private" ] "extern" string ( "view" | "address" ) "def" ident "::" "unit" "->" type
```

- 文字列は 1 つで、C の symbol。`abi::link_name_error` を今のとおり通る。`extern "m" "s" view def` は `E0002`（kind 案 `data_symbol_module_name`、文案 `a data symbol has one link name, its C symbol; remove the module string`）。
- `address` は、リンク名の直後で次が `def` のときだけの文脈キーワード（D-55。W24 の `view` と同じ規則）。リンク名のない `extern address def` は `E0002`（kind 案 `data_symbol_needs_link_name`、文案 `'address' takes the address of a C data symbol; write extern "symbol" address def`）。
- アドレスを取る view にリンク名を書く（`extern "x" view def f :: i64 -> R`・`:: i64 -> ref R -> unit`、NS07 の後は第 1 引数が `ExternAddr`）と、今と同じ `E0002` `extern_view_link_name`（位置はリンク名）で、文を改める（D8）: `an extern view that takes an address links no symbol; remove the string, or declare 'unit -> T' to read the data symbol 'x'`。判定は構文の段で、引数の型の式が `unit` 1 つかで行う。リンク名のない `extern view def f :: unit -> T` は今どおり `extern_view_signature`。`address def` の引数が `unit` でなければ `E1008`（kind 案 `data_symbol_signature`）。
- `@trusted` は性質なしでだけ書ける（`E1037` `trusted_view_property`、文案 `'@trusted' on a data symbol takes no properties; the compiler knows its effects`）。`@count` は `E0002` `misplaced_count_attribute`（文案 `… not a data symbol`）。kind は今のもので、文を足す。edition に依らない。

#### 2. 型

値を読む形の `T`:

| `T` | 読み方 | 値が NULL のとき |
| --- | --- | --- |
| `i8`〜`i64`、`i8u`〜`i64u`、`f32`、`f64` | C の変数の幅どおりの `load`（`extern def` の 8・16 bit の広げはしない） | – |
| `extern type copy H` | `load ptr` | `TrapKind::NullHandle`（NS01。`extern def` の結果と同じ） |
| `Maybe<H>`（`H` は `copy`） | `load ptr` と `nullable_from_c` | `None` |
| W24 の view の record `R`（`abi::view_record` を通るもの） | symbol のアドレスで `read_host_record` | field の規則は W24 のまま |
| M01 のクラス `C`・`Maybe<C>`（M01 が done のとき） | `load ptr` の後に `objc_retain` で所有値にする | `C` は M01 の nil のトラップ、`Maybe<C>` は `None` |

ほかは `E1008` `data_symbol_signature`（文案 `data symbol 'f' reads '{T}', which is not a C scalar, a copy extern type, Maybe of one, or a view record; for a C bool read i8u`）。主なものは `bool`、所有する `extern type`、`local` の handle、`string`・配列・`ref`・関数・`FnPtr`・`ZString`・`unit`（D5）。

アドレスを取る形の `H` は `extern type copy H`（値は `ptr @sym`。リンカーが解決したアドレスなので非 Null）か、NS07 が done なら `ExternAddr`（`ptrtoint ptr @sym to i64` を `raw` にした値）。`i64`・`Maybe<H>`・所有する handle は `data_symbol_signature`。`@weak` のデータシンボルは [M17](M17-macos-min-version-and-weak-import.md) が `Maybe` で受ける。

#### 3. 意味

- 呼び出しは `()` を与えた直接の呼び出しだけ（W24 と同じ。関数値・部分適用は今の `extern view '…' must be called directly with all its arguments` の経路に文を足す）。
- 値を読む形は、呼ぶたびにその時点の値を読む（定数に畳まない）。ほかのスレッドとの同期はしない（原子的な読み出しではない）。型・大きさ・整列・スレッド安全の正しさは、今の extern と同じく `@trusted` の責任である。
- 効果と権限: 効果 `extern`、`ffi` の場所、`@trusted` がなければ `W1010`。W24 の view と同じく `@no_alloc`・`@no_copy`・`@terminates` は満たし、`pure` と `@no_trap` は拒む。
- `tsuzuri audit`: テキストは `default_run_loop_mode -> "NSDefaultRunLoopMode" data read  path:line:col  trusted`（アドレスの形は `data address`）、JSON はデータシンボルの行だけに `"data":"read"` を足す。
- TZ08 の包み・`--emit header`・`--emit bindings-*`・関数本体の生成の対象にしない（W24 の view と同じ）。

#### 4. 宣言の照合

| 状況 | 結果 |
| --- | --- |
| 同じ symbol の値を読む形が複数で、`T` がすべて同じ | 受理。global は 1 つ |
| 同じ symbol の値を読む形の `T` が違う | `E1008` `conflicting_extern_declaration`（文案 `data symbol 'sym' is read as '{T1}' here but '{T2}' in the first declaration`、related は最初の宣言） |
| 値を読む形とアドレスを取る形 | 受理（同じ global） |
| 関数の `extern "sym" def` とデータシンボル | `E1008` `conflicting_extern_declaration`（文案 `extern symbol 'sym' is declared as a function here but as a data symbol in the first declaration`） |
| 生成コードが関数として `declare` する名前のデータシンボルに到達 | `build` で同じ kind（LLVM は同じ名前の `declare` と global を拒む） |

#### 5. target

| target | 結果 |
| --- | --- |
| macOS（native、`aarch64-apple-darwin`）、Linux（native、`*-linux-gnu`・`*-linux-musl`） | 仕様 2 のとおり |
| Windows | 到達したら `E1008`（kind 案 `data_symbol_target`、文案 `data symbol 'sym' is supported on macOS and Linux; a DLL's data needs dllimport, which data symbols do not emit`）。D4 |
| `wasm32`・`wasm64` | 到達したら `E1008` `abi_native_only`（`validate_wasm_externs` に枝を足す） |
| `check`・LSP | 受理する |

到達しない宣言は拒まない（Windows・WASM と共有するパッケージが宣言を持てる）。

### Phase 2

#### 6. `ZUtf8String`

- `std/ZUtf8String.tz`（opt-in）に不透明な `record ZUtf8String { bytes: [i8u] }`。不変条件: 長さ 1 以上、最後の要素だけが 0、0 を除いた並びは正しい UTF-8。Copy でない。
- `union Error = InteriorNul of i64`（最初の U+0000 のバイトの位置）、`instance Display<Error>`（文案 `U+0000 at byte 3 cannot be in a NUL-terminated string`）。

| 関数 | 型 | 意味 | 費用 |
| --- | --- | --- | --- |
| `ZUtf8String.of` | `ref utf8string -> Result<ZUtf8String, Error>` | U+0000 があれば最初の位置の `Error`。なければ n + 1 バイトを 1 回だけ確保して写し、末尾に 0 | O(n)、確保 1 回 |
| `ZUtf8String.literal` | 文字列リテラル → `ref ZUtf8String` | コンパイル時に作った UTF-8 と末尾の 0 の静的データへの共有借用。同じ内容は 1 つのデータ | O(1)、確保なし |
| `ZUtf8String.to_utf8string` | `ref ZUtf8String -> utf8string` | 末尾の 0 を除いて写す | O(n)、確保 1 回 |

- `literal` の引数は、補間のない `string` の文字列リテラル（`"…"`・複数行・生）だけ。型は `ZString.literal` と同じく `string -> ref ZUtf8String` なので、`u8"…"` は型の誤りになる。ほかの式は `E1005`（kind 案 `zutf8string_literal_argument`）、U+0000 を含むものは `E1005`（`zutf8string_literal_interior_nul`）。UTF-16 のリテラルは対のないサロゲートを持てる（「現状」の「字句」）ので、それを含むものも `zutf8string_literal_argument`（文案 `ZUtf8String.literal needs text that is valid UTF-8; the literal has an unpaired surrogate`）。動的な `string` からは `Utf8String.from_string` を先に通す。

#### 7. ABI

- `extern` の引数 `ref ZUtf8String` は `const char *argN`（`bytes` の先頭）。ホストは呼び出しの間だけ最初の 0 までを読んでよく、書き込み・保持をしてはならない。写しも確保もしない。
- `Maybe<ref ZUtf8String>` の `None` は NULL（W15 の `Nullable` に `BorrowedZString` と同じ形の腕を足す）。`@c_layout` の field の `ref ZUtf8String` は、W21 の `ref ZString` の field と同じく入力専用（`abi::c_layout_input_only`）。
- 値の `ZUtf8String`、`ref mut`、コールバック、`export def` は今の `E1008`（`unsupported_extern_signature`・`unsupported_export_signature`）。`@count` の位置は `count_not_buffer`。`--emit header` は `const char *argN`。WASM で生成すると `abi_native_only`（モジュールの関数は WASM でも使える）。

#### 8. 結果の `const char *`（D3。承認まで着手しない）

- 既定案: extern の結果の `Maybe<ZUtf8String>` は、返った `const char *` を写して受ける。NULL は `None`。最初の 0 までを数え（上限なし）、n + 1 バイトを 1 回だけ確保して写し、UTF-8 として検査する。正しくない UTF-8 は `TrapKind::Encoding`。所有はホストに残る。`Maybe` のない `ZUtf8String` の結果は `E1008` `unsupported_extern_signature`。
- 承認の前は、結果の `ZUtf8String`・`Maybe<ZUtf8String>` は今のとおり `unsupported_extern_signature`。

## 設計

- **構文**: `SignatureDecl` に `data: Option<DataAccess>`（`Read`・`Address`）。値を読む形は `view: true`・`link: Some`・`data: Some(Read)`。`view` の枝のリンク名の拒否を署名の後（4897 行の後）に移し、引数が `unit` 1 つなら `data: Some(Read)`、そうでなければ D8 の文の `ExternViewLinkName`。`address` の枝を隣に足す。`src/syntax_codec.rs` に `data` を足して `src/frontend_cache.rs:32` の `FRONTEND_FORMAT`（今 27）を 1 上げ、`src/formatter.rs` は形を保ち、`src/api_diff.rs` は W24 の `view` とリンク名と同じく API の外とする（今の `api_diff` は extern の `view` もリンク名も比べない）。
- **検査**: `HostImport` に `data: Option<DataSymbol { symbol, access }>`。`native_symbol`・`wasm_module`・`wasm_name` は空、`explicit`・`callbacks` は偽（view と同じ）にし、関数の symbol を使う経路に入れない。`HostImport::lowers_inline()`（`view` か `data`）を足し、「関数本体を出さない」「symbol を呼ばない」の意味で `import.view.is_some()` を見ている場所を置き換える（手順 1 の一覧で場所ごとに決める。S2）。型は `abi::data_symbol_read_type`・`data_symbol_address_type`（record は `view_record`）。照合は `explicit_symbols` の隣の `data_symbols: BTreeMap<String, (DataRead, Span)>`。
- **生成**: `Globals::data_symbols: BTreeSet<String>` に到達した名前を集め、`emit_program` が関数の後に名前の順で `@<sym> = external global [0 x i8]` を 1 行ずつ出す。そのとき `intrinsics` に `@<sym>(` の `declare` があれば仕様 4 の誤り。`FunctionEmitter::data_symbol(access, result)`（`call` の `extern_view` の分岐の隣）は、スカラーを `load <ty>, ptr @sym, align <C の整列>`、handle を `load ptr` と `NullHandle` の `guard`（native）か `nullable_from_c`、record を `read_host_record(R, "@sym")`、アドレスを `ptr @sym` か `ptrtoint` で作る。`host_pointer` は通らない。M01 の型は `objc_retain`（`declare` は symbol ごとに 1 行の共通規則）で、M01 の `objc_used` を立てる（ObjC のモジュールのフラグ・`-lobjc`・M01 の target の検査。データシンボルのほかに ObjC の使用がないプログラムでも `objc_retain` が解決する）。Windows と WASM の判定は `validate_wasm_externs` と同じ所。
- **効果・audit**: `src/effects.rs`・`src/pure_rules.rs` は `HostCall` の腕で view と同じ（理由の文は `data <関数名>`）。`src/capabilities.rs` の `ExternUse`・`ExternReport` に `data`、`data` のない行は今の文字列。
- **Phase 2**: `std/ZUtf8String.tz` は Tsuzuri で書き（`of` は最初の 0 を探して `new [i8u](n + 1, …)`、`to_utf8string` は末尾を除いて `utf8string` にする。不変条件で UTF-8 は正しい）、ランタイムの C を足さない。`src/stdlib.rs` の `SOURCES`・`OPT_IN`・`RESERVED_MODULES`（+1）・`opaque_record`。組み込み `Builtin::ZUtf8StringLiteral`（`string -> ref ZUtf8String.ZUtf8String`）は `ZString.literal` と同じく検査し（`src/check.rs:10619` の `validate_zstring_literals`）、`Borrow(Static(id), false)` に書き換え（10672 行）、`static_definition`（`src/llvm.rs:3686`）が `[N x i8]` を出す。組み込みの表は `src/effects.rs:552`（facts）と 3838 行の数、`src/pure_rules.rs:201`、`src/pinned_rules.rs:167` に `ZStringLiteral` と並べて足す。`abi::zutf8string`、`host_call` は field 0 の `ptr` 1 つ、`c_parameters` は `const char *argN`。`MODULES` に `("ZUtf8String", subject(&["ZUtf8String"]))`、`tests/std_examples.rs` の床、`vsc/src/core.ts` の `libraryModules`。std の関数は権限を持たないので、`tests/fixtures/capabilities/std-table.txt` は変わらない（`ZString` も載っていない）。

## 前提とする他チケットのインターフェース

- [W24](_completed/W24-extern-view.md)・[W20](_completed/W20-zstring.md)・[W15](_completed/W15-abi-nullable-handles.md)・[W21](_completed/W21-c-layout-records.md)（done）: `HostImport.view`、`view_record`、`read_host_record`、`trusted_view_property`・`misplaced_count_attribute`、`ZString` の形、`Nullable`、入力専用の field。
- [M01](M01-objc-class-types.md)（弱い）: ObjC のクラスの型、`objc_retain`、nil のトラップ、`objc_used`（モジュールのフラグと `-lobjc`）。
- [NS07](NS07-extern-addr-nonnull-view.md)（弱い）: `ExternAddr`。NS07 の後もアドレスを取る view の判定（引数が `unit` 1 つか）は変わらない。

## 他チケットへ提供するインターフェース

- データシンボルの 2 つの形、`HostImport.data`、`HostImport::lowers_inline`、`Globals::data_symbols`。
- [M07](M07-apple-sdk-bindgen-and-package.md): `NSString * const`・`const double` などを `view def` に、Phase 2 があれば `const char *` の引数を `ref ZUtf8String` に写す。
- [M17](M17-macos-min-version-and-weak-import.md): `@weak` のデータシンボル（`extern_weak global`、結果を `Maybe` で受ける）。
- [M04](M04-objc-blocks.md)・[M09](M09-main-thread-dispatch-and-handles.md): `_NSConcreteGlobalBlock`・`_dispatch_main_q` を利用者が読む正規の形。[M20](M20-macos-framework-link-inputs.md) の Phase 2: `@link(framework = …)` をデータシンボルにも付ける。

## 実装手順

### Phase 1

1. **変更**: なし（下書きは `target/scratch/m06/`）。**内容**: 着手条件の基準の保存。`grep -rn "HostCall\|native_symbol\|\.view" src/` の全件に「置き換える」「view だけ」「変えない」の印を付ける。**確認**: すべてに印が付いた。
2. **変更**: `src/syntax.rs`・`src/parser.rs`・`src/syntax_codec.rs`・`src/frontend_cache.rs`・`src/formatter.rs`・`src/api_diff.rs`・`src/diagnostic_kinds.rs`。**内容**: 仕様 1。**確認**: T0、`parses_view_and_address_forms`・`syntax_rejections`、`tsuzuri fmt` が形を保つ、2026・2027 の edition で同じ。
3. **変更**: `src/check.rs`・`src/abi.rs`・`src/diagnostic_kinds.rs`・`src/explain/codes/E1008.md`。**内容**: 仕様 2・4、`@trusted`・`@count`。**確認**: `types_and_conflicts`、T1。
4. **変更**: `src/llvm.rs`・`src/llvm_imports.rs`・`src/llvm_abi.rs`・`src/effects.rs`・`src/pure_rules.rs`・`src/capabilities.rs`。**内容**: 生成、仕様 3・5。**確認**: `lowers_to_one_external_global`・`generated_declare_conflict`・`targets_reject`・`audit_rows`、基準との `cmp`（S1）、2 回の生成で同じ IR。
5. **変更**: `tests/data_symbols.mjs`・`tests/data_symbols_host.c`。**内容**: E2E。**確認**: `cargo build && node tests/data_symbols.mjs target/debug/tsuzuri`（macOS と Linux、`-O0`・`-O3`、`--backend fast`、同梱の zig）。
6. **変更**: 文書。**確認**: T2、`node scripts/check-docs.mjs <変更したページ>`、`node scripts/gen-docs.mjs --check`、`node scripts/check-cli-docs.mjs`。PR に `ci:full` を付ける。

### Phase 2（人間が求めたとき）

7. **変更**: `std/ZUtf8String.tz`・`src/stdlib.rs`・`tests/std_argument_order.rs`・`tests/std_examples.rs`・`vsc/src/core.ts`。**内容**: 型・`Error`・`of`・`to_utf8string`。**確認**: T0、`cargo test --locked --test std_argument_order --test std_examples`、`tsuzuri test --doc std`。
8. **変更**: `src/check.rs`・`src/llvm.rs`・`src/effects.rs`・`src/pure_rules.rs`・`src/pinned_rules.rs`・`src/diagnostic_kinds.rs`。**内容**: `ZUtf8String.literal`。**確認**: `literal_is_static`・`literal_rejections`。
9. **変更**: `src/abi.rs`・`src/llvm_imports.rs`・`src/llvm_abi.rs`・`src/llvm.rs`。**内容**: 仕様 7。**確認**: `ref_zutf8string_is_one_pointer`・`zutf8string_abi_rejections`・`wasm_rejects_ref_zutf8string`。
10. **変更**: 同上（D3 の承認の後だけ）。**内容**: 仕様 8。**確認**: `result_copies_and_checks`。
11. **変更**: E2E と文書。**確認**: T2。

## テスト計画

### Rust（`tests/data_symbols.rs`、新規）

| テスト | 内容 | 期待 |
| --- | --- | --- |
| `parses_view_and_address_forms` | 2 つの形、`private`、`let address = 1`、`extern "x" def address :: unit -> i64` | 受理。`address` は識別子のまま |
| `syntax_rejections` | `extern "m" "s" view def`、`extern address def`、`extern "x" view def :: i64 -> R`・`:: i64 -> ref R -> unit` | `data_symbol_module_name`・`data_symbol_needs_link_name`・`extern_view_link_name`（D8 の文、位置は文字列） |
| `types_and_conflicts` | 仕様 2・4 の各行、`bool`、所有する handle、`i64` のアドレス | 受理か `data_symbol_signature`・`conflicting_extern_declaration` |
| `lowers_to_one_external_global` | 2 つのモジュールが同じ symbol を読む | global の宣言が 1 行、幅どおりの `load`、handle の `NullHandle`、`Maybe` の分岐、`ptr @sym`、`host_pointer` の検査なし。2 回の生成が同じ |
| `generated_declare_conflict` | 生成コードが `declare` する名前を読む | `build` で `conflicting_extern_declaration` |
| `targets_reject` | `x86_64-pc-windows-msvc`・`wasm32`、到達しない宣言 | `data_symbol_target`・`abi_native_only`。到達しなければ成功、`check` は 0 |
| `effects_and_trust` | `@trusted` なし、`@trusted pure`、`@count`、`@no_alloc` からの読み出し | `W1010`・`trusted_view_property`・`misplaced_count_attribute`・受理 |
| `audit_rows` | `tsuzuri audit` のテキストと `--json` | `data read`・`data address`。ほかの行は今の文字列 |

### E2E（`tests/data_symbols.mjs`、新規。`-O0`・`-O3`）

- C のホスト `tests/data_symbols_host.c` に境界の値の大域変数（`int8_t` の -128、`uint16_t` の 65535、`int64_t` の最小値、`float` の -0.0、NaN の bit 列の `double`、NULL と非 NULL の `const void *`、`struct { int32_t x, y; }`）と、値を書き換える関数を置く。Tsuzuri は値、アドレス（C の関数で一致を確かめる）、`Maybe` の `None`、NULL の `copy H` のトラップ（`--trap-info` の文と終了コード）、書き換えの後の新しい値を検査する。期待値は C の側で書く。
- ホストを `.o` と共有ライブラリ（Linux は `.so`、macOS は `.dylib`）の両方で試す（GOT から読む経路）。
- macOS だけ: 早見のプログラムと、`_dispatch_main_q` を `dispatch_async_f` に渡して `CFRunLoopRunInMode(NSDefaultRunLoopMode, 1.0, true)` でコールバックを呼ばせる段（4 を返す。`perl -e 'alarm 15; exec @ARGV'` で時間を切る）。Apple の ld と同梱の zig の両方でリンクする。
- `--backend fast`（`-O0`）で同じ値を読む。`--backend fast` は `cargo build --features fast-backend` で作ったコンパイラだけが持つ。翻訳できなければ既存の `E2002` `fast_backend_unsupported_construct` で止まることを記録する。

### Phase 2（`tests/zutf8string.rs`、新規）

`types_and_opacity`（`E1022`）、`literal_is_static`（`ZUtf8String.literal "Tsuzuri"` が `[8 x i8] c"Tsuzuri\00"` のデータ、確保なし、同じリテラルは 1 つ）、`literal_rejections`（補間、変数、`u8"…"`、`"a\0b"`、`"a\u{D800}b"`）、`ref_zutf8string_is_one_pointer`（`declare i64 @strlen(ptr)`、ヘッダーの `const char *arg0`、`None` が `null`）、`zutf8string_abi_rejections`、`wasm_rejects_ref_zutf8string`、`result_copies_and_checks`（D3 の後。NULL、`こんにちは` の往復、`"\xff"` の `Encoding`）。E2E は `strlen`・`puts` に `literal` と `of` の結果（ASCII、BMP 外）を渡し、macOS で `objc_getClass (ZUtf8String.literal "NSObject")` が 0 でない。std の例（G34）は手で書いた期待値。

## ドキュメント

- `_tsuzuri/language-reference/compiler/native-interop.md`: 新しい節「C のデータシンボル（`view def`・`address def`）」（2 つの形、型の表、NULL、target、例）。「ホストメモリの読み書きビュー（`extern view`）」のリンク名の説明を、アドレスを取る形に限ると直す。Phase 2 で「NUL 終端の UTF-8（ZUtf8String）」の節。
- 新規 `built-in-types-and-modules/zutf8string.md`（Phase 2。「引数の順序」の節を含む）、`index.md`、`argument-order.md` の対象の表、`zstring.md`・`utf8string.md` からの案内。
- `values-and-functions/attributes.md` の `@trusted`・`@count` の書ける位置。`compiler/diagnostics.md` の生成された表（`node scripts/gen-docs.mjs`）、`src/explain/codes/E1008.md`。
- `docs/language.md` の「ホスト関数のインポート」（449 行）・「リンク名」（468 行）、`docs/architecture.md` のホスト境界。`README.md` の「変更の種類別」の host ABI の行に新しいテスト。release notes に std のモジュール名 `ZUtf8String`。

## 受け入れ条件

- [ ] Phase 1: 仕様 1〜5 のとおりに動く（macOS と Linux、`-O0`・`-O3`、Apple の ld と同梱の zig）。早見のプログラムが `dlsym` なしに期待の出力を返す。
- [ ] データシンボルを使わないプログラムの IR・object・header・診断・既定の `.wasm` が基準と一致する（D8 の文を除く）。global の宣言が重複せず、IR が決定的。
- [ ] 新しい診断コードがなく、新しい kind は D11 のものだけで、`diagnostics.md` に載った。
- [ ] Phase 2: 仕様 6・7（仕様 8 は D3 の後）のとおり。`ZUtf8String` を名指さないプログラムの IR が変わらず、`literal` に確保がない。`MODULES`・床・予約名・権限の表を更新した。
- [ ] 文書を直し、T2 が通り、PR に `ci:full` を付けた。

## 落とし穴

- **`address` を予約語にしない**。probe も「現状」も `let address = …` を使う。
- **リンク名の拒否を署名の前に返さない**。値を読む形を受けるには、署名を読んでから判定する。誤りの位置（文字列）は保つ。
- **`native_symbol` に symbol を入れない**。入れるとヘッダー・W14・TZ08・`declare` の経路が関数として扱う。
- **8・16 bit を広げない**。`extern def` は `i8`・`i16` を `int32_t` に広げる（`src/abi.rs:794` の `c_scalar`）が、データシンボルは変数の幅を読む。4 バイトで読むと隣の変数まで読む。
- **同じ名前の `declare` と global**。LLVM は 1 つのモジュールに置けない（仕様 4）。
- **Mach-O の `_`**。IR の名前は C の名前のまま（`@_dispatch_main_q`）で、LLVM が object で `__dispatch_main_q` にする。予約の `__` は C の名前に効く。
- **スレッドローカルの変数**は宣言から見分けられない。文書に書く。
- **`ZUtf8String` を常に読み込まない**（S4）。`literal` の静的データを値として取り出さない（W20 と同じ）。

## 対象外

- データシンボルへの書き込みの専用の形（NS07 の後は `address` の `ExternAddr` と W24 の書く形で書けるが、それは W24 の能力）、原子的な読み書き。
- CFSTR（M02 の `ObjC.literal`）、可変長引数（docs/language.md:446）、`__` で始まる symbol と予約された `free`・`malloc` の解禁。
- UTF-8 でない `char *`（`ref [ubyte]` と `@count` のまま）、`char **` の列、スレッドローカルの変数、Windows の DLL のデータ、関数ポインターを値に持つデータシンボル。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | 値を読む形を W24 の view の族として `extern "sym" view def f :: unit -> T`、アドレスを取る形を `extern "sym" address def f :: unit -> H` とする。`address` は `extern "sym"` の直後だけの文脈キーワード（D-55） | `kCFAllocatorDefault`（値がポインター）と `_dispatch_main_q`（symbol 自身が物体）は、どちらもポインターの型の `copy` handle になり、型だけでは区別できない。値を読む形は「symbol の場所を型付きで写す」ので W24 の view と同じ意味。退けた案: (a) `extern "sym" let x : T` は Tsuzuri の大域変数に見え、言語が排除するモジュール共有の可変状態（docs/language.md:446）と取り違えやすい。(b) アドレス演算子は D-63・D-67 が入れないポインターの構文。(c) 組み込みの `Extern.address "sym"` は新しいモジュール名が要り、型が付かない | 要承認（D-55。承認前は Phase 1 の手順 2 以降に着手しない） |
| D2 | NUL 終端の UTF-8 を std の opt-in のモジュール `ZUtf8String` にする（`RESERVED_MODULES` を 1 増やす）。API は W20 に倣い `of`・`literal`・`to_utf8string` | `string` と `ZString` の対と同じく `utf8string` と対になる。退けた案 `CString` は Rust の `CString`（任意のバイト）と同じ名前で、UTF-8 の保証がないように読める | 要承認（D-55。承認前は Phase 2 に着手しない） |
| D3 | 結果の `const char *` は `Maybe<ZUtf8String>` で写して受ける。上限なし、正しくない UTF-8 は `TrapKind::Encoding` | 返った文字列の寿命はホストが決める（`sel_getName` は不死、`[NSString UTF8String]` は pool まで）ので、すぐ写すのが安全。上限は範囲外の読み出しを防がない。UTF-8 を保てば `to_utf8string` が失敗しない。退けた案: `Result` に `InvalidUtf8` を足す（呼び出しごとに二重の分岐）、U+FFFD への置き換え（値を黙って変える。GUIDE §1 規則 2）、対象外にする（手書きの `strlen`・`memcpy` が残る） | 要承認（承認前は仕様 8 に着手しない） |
| D4 | データシンボルは macOS と Linux。Windows は到達したら `data_symbol_target`、WASM は `abi_native_only` | ELF と Mach-O は外部の global を GOT から読める。Windows の DLL の data は `dllimport` の間接参照が要り、MSVC は無しでリンクできない。W17 の `dllimport` は関数だけなので、data は W17 と合わせて別に決める | 既定案（実装者はこの案に従う） |
| D5 | 値を読む `T` は仕様 2 の表に限る（`bool`、所有する handle、`local` の handle は拒む） | C の変数の幅をそのまま読む型だけにする。`bool` は C の 1 バイトと `extern def` の `int32_t` のどちらにも決められない。所有する handle を作ると drop が大域の物体を解放する | 既定案（実装者はこの案に従う） |
| D6 | 表現は `HostCall` と `HostImport.data`（`native_symbol` は空） | W24 の D6 と同じく、権限・`W1010`・audit・効果・所有権が普通の extern と同じ経路で働く。`native_symbol` を空にすれば関数の経路に入らない | 既定案（実装者はこの案に従う） |
| D7 | IR は symbol ごとに 1 行の `@sym = external global [0 x i8]`、呼ぶたびの型付きの `load`、アドレスは `ptr @sym`。NULL と整列の検査をしない | 不透明な型なら、違う形で使っても宣言が 1 つで済む。リンカーのアドレスは NULL にならず、整列は C の定義が決める（clang も検査しない）。書き換えられる変数のために畳まない | 既定案（実装者はこの案に従う） |
| D8 | `extern_view_link_name` は、アドレスを取る view にリンク名を書いたときだけに残し、コード・kind・位置を保って文を改める | 今の文は値を読む形がある後では誤解を招く。新しい機能を使わないプログラムの診断で変わるのは、この 1 つの誤りの文だけ | 既定案（実装者はこの案に従う） |
| D9 | 同じ symbol の値を読む形は `T` が同じなら何度でも書け、違えば `conflicting_extern_declaration`。値とアドレスの形は併用でき、関数の extern とは衝突する | W14 の「同じ symbol は同じ ABI」と同じ考え方 | 既定案（実装者はこの案に従う） |
| D10 | `ZUtf8String` は不透明な `{ bytes: [i8u] }` で、末尾の 0 と正しい UTF-8 を不変条件にする。`of` は `ref utf8string` を取る | 引数はポインター 1 つで写しが要らない（W20 の D3）。入口を `utf8string` に限ると UTF-8 の検査が要らない | 既定案（実装者はこの案に従う） |
| D11 | 新しい診断コードを割り当てない。新しい kind は `E0002` の `data_symbol_module_name`・`data_symbol_needs_link_name`、`E1008` の `data_symbol_signature`・`data_symbol_target`、`E1005` の `zutf8string_literal_argument`・`zutf8string_literal_interior_nul`。ほかは今の kind に文を足す | D-51、E12 D12 の前例 | 既定案（実装者はこの案に従う） |
| D12 | `MODULES` の行は `subject(&["ZUtf8String"])`。std の関数は Tsuzuri で書き、ランタイムの C を足さない | D-59。W20 の D9・D10 と同じ | 既定案（実装者はこの案に従う） |
