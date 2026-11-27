# M01: Objective-C のクラスの型と所有（`extern class`・`ObjC.Object`・`ObjC.Sel`、retain/release を所有に下げる、nil は `Maybe`）

| 項目 | 内容 |
| --- | --- |
| ID | M01 |
| 優先度 | P1（M02・M04・M07・M10 がこの型と所有の規則に立つ。今は id と SEL が型のない `i64` で、セレクターの綴りの誤りや受け手の取り違えが型検査を通り、実行時の abort か SIGSEGV になる） |
| 規模 | L（Phase に分けない 1 PR。構文と符号化、型の性質と継承の木、所有と drop、`extern def` の位置、組み込みのモジュール `ObjC`、classref・selref・モジュールのフラグ、`-lobjc`、診断・テスト・文書） |
| 依存 | ([M22](_completed/M22-attribute-after-bodyless-extern-def.md)), ([M20](_completed/M20-macos-framework-link-inputs.md))（M22 が後なら `@returns_retained` を `signature` の先読みに足す。M20 は同梱の zig で `-lobjc` を探す段だけ） |
| 後続 | [M02](M02-objc-typed-message-send.md), [M04](M04-objc-blocks.md), [M07](M07-apple-sdk-bindgen-and-package.md), [M10](M10-objc-autorelease-pools.md), [M23](M23-objc-out-parameters-and-weak-references.md)（弱い後続: [M06](_completed/M06-extern-data-symbols-and-zutf8string.md), [M09](M09-main-thread-dispatch-and-handles.md), [M12](M12-gpu-presentation-to-window.md), [M17](_completed/M17-macos-min-version-and-weak-import.md)） |
| 状態 | done（2026-11-27。`extern class`・`ObjC`・所有・`@returns_*`・`@weak` のクラス・データシンボルの物体・監査・`-lobjc`。「実装と検証（2026-11-27）」） |
| 起票 | 2026-11-26（D-67。利用者の依頼「「どのチケットにもない壁」と「調査中に見つけたコンパイラの不具合（自分で再現済み）」のそれぞれについて @_features/ に開発チケットまたは修正チケットを作成して」から起票。調査で報告した「壁 1（ObjC を型安全に連携する仕組みがない。id と SEL は型のない i64 で、誤りは実行時の SIGSEGV か abort になる）」に当たる。HEAD `804b7970` で確認） |
| 承認 | [GUIDE D-67](GUIDE.md#d-67-tsuzuri-だけで-macos-のデスクトップアプリを書くm01m22) の分岐 1（X′。README の「採用しない改善」の「クラス継承」の行への注記を含む。ホストのクラスの階層を型に写すだけで、Tsuzuri の型に継承は入れない。決定事項 D8）は承認済み（2026-11-26）。要承認: D-55 の登録（予約語 `class` の `extern` の後の新しい位置、`extern class` の後の文脈キーワード `sync`、組み込みのモジュール `ObjC`（`RESERVED_MODULES` が現在の値 + 1）、属性 `@returns_retained`・`@returns_not_retained`。D3・D9）と、`E1008` の kind `abi_apple_only`（D10）。決定事項 D1（既定のスレッドの性質）・D3（属性の名前）が要承認。ほかは既定案。**D1・D3・D9・D10 は 2026-11-27 に承認済み（D-67 の委任で決定）**: 利用者の依頼「実装を進めるうえで何か判断が必要なものがあれば、あなたが考える最高の選択肢で実装することを常に許可します」に基づき、どれもチケットの推奨の案で決めた（[GUIDE D-67](GUIDE.md#d-67-tsuzuri-だけで-macos-のデスクトップアプリを書くm01m22) の「委任による決定（2026-11-27）」） |
| 改善する劣位 | Swift・Objective-C（clang の ARC）比: AppKit・Foundation の物体を型で区別できない。id・SEL・Class は型のない `i64` で、綴りの誤りは `unrecognized selector` の abort（134）、取り違えは SIGSEGV（139）になる。retain と release は手で数え、`Drop` の record で包むしかない |
| 手本にする既存実装 | `src/parser.rs` の `Parser::extern_declaration`（W18 の `copy`・`local`、W24 の `view` の分岐）と `attributes`。`src/check.rs` の `extern type` の登録と `Type::Handle` の性質の述語、W19 の `thread_bound` の門、`Builtin::RcGet`（引数を借りる結果）。`src/polymorph.rs` の `validate_instance`（制約 `AtomicValue` の型引数を kind `atomic_element_type`（`E1005`）で拒む）。`src/llvm_imports.rs` の `FunctionEmitter::host_call`、`src/llvm.rs` の `drop_value`・`validate_wasm_externs`、`src/llvm_debug.rs` の `with_codeview`、`src/driver.rs` の `-lws2_32`。章立ては [W25](W25-extern-interface-com.md) の仕様 1・2・5〜7。独立した参照は clang の `-fobjc-arc -S -emit-llvm`（GUIDE-LITE §2 規則 10） |
| 主な影響ファイル | `src/{syntax,parser,syntax_codec,frontend_cache,formatter,docgen,api_diff,semantic,kg_capture,find,repl,doctest,guide_std,stdlib,check,polymorph,abi,pinned_rules,effects,pure_rules,capabilities,llvm,llvm_imports,llvm_debug,llvm_escape,driver,test_runner,diagnostic_kinds}.rs`, `src/kg/ids.rs`, `src/kg/effects.rs`, `src/llvm_objc.rs`（新規）, `src/explain/codes/E1008.md`, `tests/objc_classes.rs`（新規）, `tests/objc_classes.mjs`（新規）, `tests/objc_classes_host.m`（新規）, `tests/fixtures/objc_classes/Main.tz`（新規）, `.github/workflows/targets.yml`, `docs/language.md`, `docs/architecture.md`, `docs/stability.md`, `_tsuzuri/language-reference/compiler/objective-c.md`（新規）, `_tsuzuri/language-reference/index.md`, `_tsuzuri/language-reference/compiler/native-interop.md`, `_tsuzuri/language-reference/values-and-functions/attributes.md`, `_tsuzuri/language-reference/compiler/platforms.md`, `_tsuzuri/language-reference/compiler/webassembly.md`, `_tsuzuri/language-reference/organizing-tsuzuri/modules.md`, `_tsuzuri/language-reference/built-in-types-and-modules/argument-order.md`, `_tsuzuri/language-reference/languages/strategy.md`, `_features/README.md` |

## 実装者向け早見

- 範囲: `extern ["実行時名"] class [sync] Name : Base` の宣言と、強い参照を 1 つ所有する値（drop で `objc_release`）、`ObjC.retain`・`upcast`・`upcast_owned`・`downcast`・`sel`、`extern def` の引数 `ref C`（+0）と結果 `C`（+0 を受けて retain）、nil は `Maybe`。送信は [M02](M02-objc-typed-message-send.md)、クラスの定義は [M03](M03-objc-classes-defined-in-tsuzuri.md)、Block は [M04](M04-objc-blocks.md)、プールは [M10](M10-objc-autorelease-pools.md)。
- 最初に読む: ① 仕様 1〜5 ② `Parser::extern_declaration`（`src/parser.rs:4807`）③ `Type::Handle` の述語（`src/check.rs:684` から）と `thread_bound`（10407 行）④ `host_call` のハンドルの結果（`src/llvm_imports.rs:426-440`）⑤ 「現状」6 の参照の IR と 8 の +0 の結果の受け取り方。
- 最初に書くテスト: `tests/objc_classes.rs` の `parses_a_class_chain_and_types_its_values` と `lowers_like_the_clang_reference`（`--emit llvm --target aarch64-apple-darwin`）→ `tests/objc_classes.mjs` の `balance`。
- 触ってはいけない: `extern type`・`extern def`・`extern view`・コールバック・`export def` の意味・ABI・header。利用者の `extern "objc_msgSend"` に対する W14 の規則。ObjC を使わないプログラムの IR・object・header・診断・既定の `.wasm`（1 byte も変えない）。
- 完了の見分け方: 次のプログラムを `tests/objc_classes_host.m`（`clang -fobjc-arc -c`。テスト計画）と `--link` でつなぐと、`-O0`・`-O3` で `42 0` を出す（2 つ目はホストの生きている物体の数）。`-lobjc` は driver が足す。

```tsuzuri
@trusted
extern class NSObject : ObjC.Object

@trusted
extern class TzProbe : NSObject

@trusted
extern class TzProbeChild : TzProbe

@trusted
extern "tzobjc_make_child" def make_child :: unit -> TzProbeChild

@trusted
extern "tzobjc_tag" def tag :: ref TzProbe -> i64

@trusted
extern "tzobjc_live" def live :: unit -> i64

@trusted
extern "objc_autoreleasePoolPush" def pool_push :: unit -> i64

@trusted
extern "objc_autoreleasePoolPop" def pool_pop :: i64 -> unit

def use_objects :: unit -> i64 = \() ->
    let child = make_child ()
    let base: TzProbe = ObjC.upcast_owned (ObjC.retain (ref child))
    let again: Maybe<TzProbeChild> = ObjC.downcast (ref base)
    let found = match again with
        | Maybe.Some c -> tag (ObjC.upcast (ref c))
        | Maybe.None -> 0
    found + tag (ObjC.upcast (ref child))

def main :: unit -> i32 = \() ->
    let pool = pool_push ()
    let total = use_objects ()
    pool_pop pool
    do! IO.writeln $"{total} {live ()}"
    0
```

## 目的

- 調査（D-67 の経緯）で、今でも C なしで AppKit のアプリが動くと確かめた。ただし id・SEL・Class は型のない `i64` で、所有は手で数える（現状 2・3）。
- このチケットは、ObjC のクラスを型として宣言する形と所有の規則を足す。値は強い参照を 1 つ所有し、drop で release する。nil は `Maybe` だけで表す。継承はホストの階層を型の検査に写すだけで、Tsuzuri の値の型に継承は入れず、暗黙の変換もしない（W25 の `extern interface I : Base` と同じ）。
- 安全性の根拠は「宣言が正しいこと」に閉じ、`@trusted`（E15）と権限 `ffi`（D-57）の監査の対象にする。生ポインター型・ポインター演算・`unsafe` は入れない（D-63・D-67 の X′）。M02・M04・M07・M10 はこの上に立つ。M01 だけでも、`object_getClass`・`NSStringFromClass` などの C の API と、W14 の規則の中で宣言した `objc_msgSend` を、型と所有の付いた形で呼べる。

## 着手条件と停止条件

### 着手条件

- 承認: D-67 の分岐 1 は X′ に決まった（承認済み、2026-11-26。決定事項 D8）。決定事項の D1・D3・D9・D10 の承認が記録されている（`grep -n '利用者が決めた点' _features/GUIDE.md`）。承認が記録されるまで、このチケットに着手しない（Phase がないのでチケット全体が対象）。
- 依存: 強い依存はない。[M22](_completed/M22-attribute-after-bodyless-extern-def.md) がなければ、手順 4 で `signature` の先読み（`src/parser.rs:2390-2401`）に属性の名前を足す。[M20](_completed/M20-macos-framework-link-inputs.md) がなければ、同梱の zig の E2E を飛ばして理由を出す。[W25](W25-extern-interface-com.md) は依存ではない（先に入っていれば `HandleKind::Interface` と `host_call` の分割の形に合わせる）。[W17](_completed/W17-link-attribute-dllimport.md) が先なら `-lobjc` を W17 の `system_libraries` に寄せる。
- 環境: macOS arm64 と Xcode Command Line Tools の SDK（E2E と参照の IR）。Rust のテストはどのホストでも走る。
- ベースライン: `tests/fixtures/*`・`examples/*` の `--emit llvm`（native と `--target wasm32`）・`--emit header`・`check --json` を `target/scratch/m01/before/` に保存する。`node scripts/repo-kb.mjs brief M01`。

### 停止条件

- **S1** `HandleKind` に種類を足すだけでは表せず、`Type` に variant を足したくなった（W25 の S1 と同じ）。
- **S2** ObjC を使わないプログラムの IR・object・header・診断・既定の `.wasm` が 1 byte でも変わる。
- **S3** `HandleKind` の判定の書き換えで、既存の `tests/thread_bound_handles.rs`・`tests/ffi_extensions.rs`・`node tests/ffi_extensions.mjs` の期待値が変わる。
- **S4** 生成した selref・classref・フラグの形が clang の参照（現状 6）と合わず、どちらに合わせるか決まらない。Apple の ld と同梱の zig で実行の結果が違う。
- **S5** retain・release の数がホストの dealloc の数と合わず、原因が一つに決まらない。
- **S6** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `804b7970` で確認）

再現は `/tmp/tz-m01/<名前>/Main.tz` に置き、`/tmp/tz-m01` で HEAD の dev のバイナリを実行した（macOS 27.2.1 arm64、Apple clang 21.0.0、SDK 27.2）。

### 1. 構文と名前

- `Parser::extern_declaration`（`src/parser.rs:4807-4905`）は、リンク名の文字列（`link_name`、4908 行）の後に `type`・`view def`・`def` だけを受ける（`copy`・`local` は 4825-4826 行、`view` は 4871-4885 行）。`ExternDeclaration` は `Type`・`Function`（55-58 行）、`Program` は `externs`・`extern_types`（`src/syntax.rs:294-295`）。`class`・`export`・`with` は予約語（`src/lexer.rs:655`・`646`・`680`）。

  ```tsuzuri
  @trusted
  extern class NSObject : ObjC.Object

  def main :: unit -> i32 = \() -> 0
  ```

  `tsuzuri check a` → `a/Main.tz:2:8: error[E0002]: expected 'def' after extern`（kind `expected_token`）。`extern "NSWindow" class Window : NSResponder` も `2:19` で同じ。
- `ObjC.Object` を型に書くと `E1004`（`unknown record, union, or type alias 'ObjC.Object'`）、`ObjC.sel "setTitle:"` は `E1002`（`unknown value 'ObjC'`）。`RESERVED_MODULES`（`src/stdlib.rs:82`）は 78 個で（903 行の `assert_eq!`）`ObjC` を含まず、利用者の `ObjC.tz` を `ObjC.x ()` で呼ぶと `check` は 0 で終わる。予約名（例 `Routine.tz`）は `E1011` `reserved_std_namespace`。
- `Attributes`（`src/parser.rs:19-25`）は `cpu`・`effects`・`trusted`・`counts`・`c_layout` だけ。`@returns_retained` を `extern def` の前に書くと `E0002`（`expected an expression; '@checked' is the only attribute of an expression`）。本体のない `extern def` の直後なら、先読み（2397 行の `"cpu" | "trusted" | "count"`）が制約行として読み、`E0002`（`constraint lines must be indented after the 'def' signature`）。
- ObjC の扱いはどこにもない（`grep -rli objc src/runtime` は 0 件。`src/bindgen.rs:545-548` が clang の ObjC の属性の名前を無視の一覧に持つだけ）。

### 2. 今の書き方と誤りの出方

```tsuzuri
@trusted
@count(1, none)
extern "objc_getClass" def objc_get_class :: ref utf8string -> i64

@trusted
@count(1, none)
extern "sel_registerName" def sel :: ref utf8string -> i64

@trusted
extern "objc_msgSend" def send0 :: i64 -> i64 -> i64

@trusted
extern "objc_release" def objc_release :: i64 -> unit

def main :: unit -> i32 = \() ->
    let cls = objc_get_class (ref u8"NSObject\0")
    let obj = send0 cls (sel (ref u8"new\0"))
    do! IO.writeln $"retainCount={send0 obj (sel (ref u8"retainCount\0"))}"
    // A misspelt selector type-checks: it is only an i64.
    let n = send0 obj (sel (ref u8"lenght\0"))
    objc_release obj
    n as i32
```

```text
$ tsuzuri build f -l objc -o f/a.out && ./f/a.out; echo "exit=$?"
retainCount=1
objc[92887]: -[NSObject lenght]: unrecognized selector sent to instance 0x1050ed350 (no message forward handler is installed)
exit=134
```

受け手とセレクターを入れ替えた `let obj = send0 s cls` も型検査を通り、実行は `exit=139`（SIGSEGV）。IR（`--emit llvm --target aarch64-apple-darwin`）は `declare i64 @objc_msgSend(i64, i64)`・`declare void @objc_release(i64)` で、id は整数として渡る。

### 3. 所有と nil

- 所有するハンドル（`extern type H`）は drop で何もしない（`needs_drop`、`src/check.rs:713-737` の `_ => false`。native-interop.md:212）。release は record と `Drop` で書く。次のプログラム（2 の `objc_get_class`・`sel`・`send0`・`objc_release` の 4 つの宣言を先頭に置く）は `tsuzuri build s -O0 -l objc`・`-O3` とも `before=1 inside=2 after=1` を出す。+0 と +1 の区別、retain の忘れ、二重の release は型で止まらない。

  ```tsuzuri
  @trusted
  extern "objc_retain" def objc_retain :: i64 -> i64

  record Strong { raw: i64 }

  instance Drop<Strong> {
      fn drop value = objc_release value.raw
  }

  def count :: i64 -> i64 = \obj -> send0 obj (sel (ref u8"retainCount\0"))

  def with_extra :: i64 -> i64 = \obj ->
      let extra = Strong { raw: objc_retain obj }
      count extra.raw

  def main :: unit -> i32 = \() ->
      let cls = objc_get_class (ref u8"NSObject\0")
      let obj = send0 cls (sel (ref u8"new\0"))
      let before = count obj
      let inside = with_extra obj
      let after = count obj
      do! IO.writeln $"before={before} inside={inside} after={after}"
      objc_release obj
      0
  ```

- `extern type copy Class` を `Maybe<Class>` で受けると、知らないクラスは `None`。`Maybe` でない `Class` で nil を受けるとトラップする。次の `i/Main.tz` を `tsuzuri build i --trap-info -l objc` で作って動かすと、`None` の後に `trap: null extern handle at i/Main.tz:6:28` を出し、`exit=133`。位置は呼び出しの地点でなく `extern def` の名前である。検査は `host_call` の 436-440 行（`matches!(result, Type::Handle(..))`）、トラップは `TrapKind::NullHandle`（`src/trap.rs:76`）。

  ```tsuzuri
  @trusted
  extern type copy Class

  @trusted
  @count(1, none)
  extern "objc_getClass" def get_class :: ref utf8string -> Class

  @trusted
  @count(1, none)
  extern "objc_getClass" def find_class :: ref utf8string -> Maybe<Class>

  def main :: unit -> i32 = \() ->
      let missing = match find_class (ref u8"NoSuchClass\0") with
          | Maybe.Some _ -> "Some"
          | Maybe.None -> "None"
      do! IO.writeln missing
      let _c = get_class (ref u8"NoSuchClass\0")
      0
  ```

### 4. ハンドルの性質と ABI の検査

- `HandleKind` は `Owned`・`Copy`・`Local`（`src/syntax.rs:775-782`）。`Type::Handle(Box<str>, HandleKind)`（`src/check.rs:178`）の名前は `Module.Name`、登録は 8829-8853 行。
- 述語は `matches!` で種類を名指す: `is_copy`（684 行。腕は 693 行）、`can_capture`（1081 行。利用者の `Drop` を持つ値は 1084 行で、`Owned`・`Local` のハンドルは 1099・1113 行で捕捉できない。関数値の複製が捕捉を複製するため）、`can_send`（1156・1173 行が `Local`）、`holds_local_handle` 系（1228-1245 行）、`can_sync`（1270 行。ハンドルの腕は 1283 行）、`holds_host_state`（1370 行。腕は 1372 行）。所有ハンドル `Res` を普通の関数値 `\() -> touch (ref r2)` に捕捉すると `E1005` `capture_exclusive_in_reusable_function`（`cannot capture Main.Res in a reusable function; fully apply exclusive borrows, or move the value into a tzroutine body, which runs once`）。同じ式を `Owned.function` に渡す形と、`tzroutine { close_res r }` へのムーブは通る（`touch :: ref Res -> unit`・`close_res :: Res -> unit` を宣言して `check` で確かめた）。`HandleKind::` の出現は `src/syntax.rs` の外に 28 か所（`src/check.rs` 14、`src/llvm.rs`・`src/parser.rs` 各 3、`src/api_diff.rs`・`src/find.rs` 各 2、`src/abi.rs`・`src/llvm_escape.rs`・`src/syntax_codec.rs`・`src/kg/ids.rs` 各 1）。
- `pinned` の解析は `local` の `extern type` を宣言したプログラムだけで走る（`thread_bound`、`src/check.rs:10407-10413`）。
- `is_handle`（`src/abi.rs:474-476`）はどの `Type::Handle` でも真で、`abi_scalar`（481 行）・`result`（465 行）・`callback`（664 行）が使う。種類を足しただけでは、`export def`・コールバックの引数・`@c_layout` の field に黙って通る。`link_name_error`（58-108 行）はリンク名を C の識別子に限るので、`OBJC_CLASS_$_…` と利用者の `extern` は衝突しない。`@trusted pure extern type` は `E1037` `trusted_type_property`（`src/parser.rs:776`）。

### 5. 生成とリンク

- `drop_value`（`src/llvm.rs:6721`）にハンドルの腕はない。WASM の検査は到達した関数を見る `validate_wasm_externs`（4165 行。呼ぶのは 2006-2008 行）で、kind `abi_native_only`（`E1008`）は登録済み。生成中の target は `current_platform()`（`src/target.rs:446`）、Mach-O は `Platform::mach_o()`（310 行）。
- `with_blocking_externs`（`src/llvm.rs:1840`）は利用者の `extern` の symbol（`extern_symbols`、1801 行）を呼ぶ行を、行の文字列で照合して包む。
- Tsuzuri の IR に `@llvm.compiler.used` はない。`!invariant.load` は 594 行の `, !invariant.load !{}` だけ。大域の番号は emitter の出現順（`@tz.literal.<n>`、5976 行）。`!llvm.module.flags` は `-g` のときだけ出し（`src/llvm_debug.rs:204`）、`with_codeview`（71-96 行）は既存の一覧に足す。
- PB07 は `appending`・`extern_weak` を `Unsupported` にし（`src/llvm_split.rs:588-589`）、`-O0` は黙って分割なしに戻る（`src/driver/units.rs:283`）。
- `-lws2_32` は 3 か所: `src/driver.rs:5329-5331`（`native_runtime` の分岐の中）と `src/test_runner.rs:566-568`・`1088-1090`。`.dwarf` 用のリンクと `link_shared` には足していない。リンク行は 5 つ: `build_complete` の exe（5353 行の `links.add_to`）と `.dwarf` 用（5453 行）、`link_shared`（5806 行）、`src/test_runner.rs` の `build_debug_runner`（573 行）・`compile_native_runner`（1094 行）。
- 同梱の zig（0.16.0）で `-l objc` は `E2002`（`unable to find dynamic system library 'objc' using strategy 'paths_first'. searched paths: none`）。

### 6. clang の参照の IR と、計画の形の確認

- `clang -target arm64-apple-macosx12.0 -fobjc-arc -fno-objc-msgsend-selector-stubs -O1 -S -emit-llvm k.m`:

  ```objc
  #import <Foundation/Foundation.h>
  NSString *f(id x) {
    if ([x isKindOfClass:[NSString class]]) return (NSString *)x;
    return nil;
  }
  NSString *g(Class c) { return NSStringFromClass(c); }
  ```

  `isKindOfClass:` は `call i1 @objc_opt_isKindOfClass(ptr %0, ptr %4)` と `declare i1 @objc_opt_isKindOfClass(ptr, ptr)`。C の関数の +0 の結果には `[ "clang.arc.attachedcall"(ptr @llvm.objc.retainAutoreleasedReturnValue) ]` が付く。
- 同じ設定で `[w setTitle:s]`・`[w title]` を書いた AppKit の `t.m` は、selref を `internal externally_initialized global ptr …, section "__DATA,__objc_selrefs,literal_pointers,no_dead_strip"`（名前は `__TEXT,__objc_methname,cstring_literals`）、classref を `internal global ptr @"OBJC_CLASS_$_NSWindow", section "__DATA,__objc_classrefs,regular,no_dead_strip"` に置き、どちらも `@llvm.compiler.used` に入れる。selref の読み出しだけに `!invariant.load` が付く。フラグは下の 6 つ（ARC のときだけ `clang.arc.retainAutoreleasedReturnValueMarker` も付く）。
- 計画の形を手で書いた次の IR は、Apple clang の `-O0`・`-O3` と `-lobjc` で終了コード 1（`objc_opt_isKindOfClass` が真）になった。同梱の zig（`zig cc -target aarch64-macos.12.0 s2.o -L$SDK/usr/lib -lobjc`）でも 1 で、`-L` がないと上の `E2002` と同じ文で失敗した。Swift の目的ファイル（`swiftc -c -parse-as-library`、`__objc_imageinfo` は `0x06040740`）と `swiftc` で 1 つにすると実行は同じで、合わさった `__objc_imageinfo` は `0x06040740` だった。目的ファイルは `__objc_classrefs` を持つが、Apple の ld の出力からは消える（clang が `.m` から作った出力でも同じ。zig の出力には残る）。

  ```llvm
  @"OBJC_CLASS_$_NSObject" = external global [0 x i8]
  @tz.objc.classref.NSObject = internal global ptr @"OBJC_CLASS_$_NSObject", section "__DATA,__objc_classrefs,regular,no_dead_strip", align 8
  @tz.objc.methname.0 = private unnamed_addr constant [4 x i8] c"new\00", section "__TEXT,__objc_methname,cstring_literals", align 1
  @tz.objc.selref.0 = internal externally_initialized global ptr @tz.objc.methname.0, section "__DATA,__objc_selrefs,literal_pointers,no_dead_strip", align 8
  @llvm.compiler.used = appending global [3 x ptr] [ptr @tz.objc.classref.NSObject, ptr @tz.objc.methname.0, ptr @tz.objc.selref.0], section "llvm.metadata"

  declare ptr @objc_msgSend(ptr, ptr, ...)
  declare ptr @objc_retain(ptr)
  declare void @objc_release(ptr)
  declare i1 @objc_opt_isKindOfClass(ptr, ptr)

  define i32 @main() {
    %cls = load ptr, ptr @tz.objc.classref.NSObject, align 8
    %new = load ptr, ptr @tz.objc.selref.0, align 8, !invariant.load !{}
    %obj = call ptr @objc_msgSend(ptr %cls, ptr %new)
    %again = call ptr @objc_retain(ptr %obj)
    call void @objc_release(ptr %again)
    %kind = call i1 @objc_opt_isKindOfClass(ptr %obj, ptr %cls)
    call void @objc_release(ptr %obj)
    %r = zext i1 %kind to i32
    ret i32 %r
  }

  !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}
  !0 = !{i32 1, !"Objective-C Version", i32 2}
  !1 = !{i32 1, !"Objective-C Image Info Version", i32 0}
  !2 = !{i32 1, !"Objective-C Image Info Section", !"__DATA,__objc_imageinfo,regular,no_dead_strip"}
  !3 = !{i32 1, !"Objective-C Garbage Collection", i8 0}
  !4 = !{i32 1, !"Objective-C Class Properties", i32 64}
  !5 = !{i32 1, !"Objective-C Enforce ClassRO Pointer Signing", i8 0}
  ```

  （先頭に `target triple = "arm64-apple-macosx12.0.0"`。`objc_msgSend` を可変長で宣言して固定の型で `call` しても LLVM は受け付けた。opaque pointer の `call` は自分の関数型を持つ）

### 7. libobjc と SDK

- `/usr/lib/libobjc.A.dylib` は `_objc_retain`・`_objc_release`・`_objc_retainAutoreleasedReturnValue`・`_objc_autoreleaseReturnValue`・`_objc_opt_isKindOfClass` を export する（`dyld_info -exports`）が、SDK の `usr/include/objc` に宣言はない（`grep -rn` で 0 件）。clang が ARC のために呼ぶ ABI で、最初の 4 つは clang の ARC の文書（Runtime support の節）が記す（リポジトリの外の事実。着手時に確かめる）。
- `NSObjCRuntime.h`: `NS_RETURNS_RETAINED` 126 行、`NS_RETURNS_NOT_RETAINED` 134 行、`NS_SWIFT_UI_ACTOR` 253 行、`NS_SWIFT_SENDABLE` 285 行。
- 方針の文: `_features/README.md:116`（「クラス継承」を対象外）と 114 行（`unsafe`・生ポインター型）、strategy.md:200（`unsafe` と生ポインター型）・202（クラス継承）、native-interop.md:632（GUI は言語の一部でない。D-67 は改めない）。

### 8. +0 の結果の受け取り方とハンドシェイク

+0 の結果を受ける 3 つの形を、ARC のホストと手書きの IR で比べた。ホストは `clang -fobjc-arc -O2 -c host.m`。

```objc
#import <objc/NSObject.h>
static int live;
@interface P : NSObject @end
@implementation P
- (instancetype)init { if ((self = [super init])) live++; return self; }
- (void)dealloc { live--; }
@end
P *make(void) { return [P new]; }
int get_live(void) { return live; }
```

呼ぶ側の IR は、プールを push し、`make` の結果を retain して release し、`get_live` を読み（`inside`）、プールを pop して読む（`after`）。結果を受ける行だけを変えた。

```llvm
; plain: a separate call right after the call
%o = call ptr @make()
%r = call ptr @objc_retainAutoreleasedReturnValue(ptr %o)
; marker: the inline assembly that clang's ObjCARCContract pass inserts
%o = call ptr @make()
call void asm sideeffect "mov\09fp, fp\09\09// marker", ""()
%r = call ptr @objc_retainAutoreleasedReturnValue(ptr %o)
; attached: the operand bundle that clang 21 emits for a C function's +0 result
%o = call ptr @make() [ "clang.arc.attachedcall"(ptr @llvm.objc.retainAutoreleasedReturnValue) ]
; with: declare ptr @llvm.objc.retainAutoreleasedReturnValue(ptr)
```

| 形 | Apple clang 21 と Homebrew clang 21.1.8（同梱のツールチェーンの clang も同じ版を名乗る）の `-O0`・`-O3` |
| --- | --- |
| plain | `inside=1 after=0`（物体はプールに入り、pop まで残る） |
| marker | `inside=0`。ただし結果を後でも使うと（`%o` を alloca に保存する）`-O0`・`-O3` とも `inside=1`。`-O0` の逆アセンブルでは `bl _make` の直後に `str x0, [sp, #0x10]` が入り、印の `mov x29, x29` が隣でなくなる |
| attached | `inside=0`。結果を後でも使っても `-O0`・`-O3` で `inside=0`。`-O3` の目的ファイルは `bl _make; mov x29, x29; bl _objc_retainAutoreleasedReturnValue`。同梱の zig でリンクしても同じ |

- 実行は macOS 27.2.1 arm64。どの形も物体は解放され（`after=0`）、違いはプールに入るかどうかだけである。ハンドシェイクはランタイムの最適化で、保証ではない。
- `clang.arc.attachedcall` の bundle は `-fobjc-arc` も ObjCARC のパスもない `.ll` の compile で下がる。LLVM の検証器は、bundle の付いた `call` の結果が `i64` だと拒む（`a call with operand bundle "clang.arc.attachedcall" must call a function returning a pointer or a non-returning function that has a void return type`）。
- `host_call` は、W14 の同じ symbol の宣言の正準形が `i64` のとき、`ptr` の結果を `i64` で受けて `inttoptr` する（`convert_ptr_result`、`src/llvm_imports.rs:339-360`・428 行）。
- LLVM の parser は 2 行目の `!llvm.module.flags` を 1 行目に併合して受け付ける（Apple clang 21 で確かめた）。

## 変えてはいけない意味

- `extern def`・`extern type`・`extern view`・コールバック・`export def` の型・ABI・header・診断と、利用者の `extern "objc_msgSend"` への W14 の規則。
- ObjC を使わないプログラムの IR・object・header・`.wasm`・診断（1 byte も変えない）。評価順序（左から右）と drop の順。`Maybe` でない位置の nil は今の `null extern handle`。
- 予約された symbol の規則。コンパイラが呼ぶ libobjc の symbol は予約しない（今日の手書きの宣言を壊さない）。

## 仕様

### 1. 宣言

```text
class_decl  = [ doc ] [ "@trusted" ] [ "private" ] "extern" [ RuntimeName ] "class" [ "sync" ] Name ":" Base
RuntimeName = 文字列リテラル 1 つ（ObjC の実行時のクラス名。C の識別子で 255 バイトまで。省略すると Name）
Base        = "ObjC.Object" | 型の名前（ほかの extern class。Module.Name と修飾してよい）
```

- `class` は既存の予約語の新しい位置。`sync` は `class` の直後で、同じ行に識別子が続くときだけの文脈キーワード（W18 の `copy` と同じ。`extern class sync : NSObject` は `sync` という名前のクラス）。
- 基底は必須で、根は組み込みの `ObjC.Object`（根を暗黙にしない。GUIDE §1 規則 2）。宣言の連鎖はホストの階層の部分列でよい（`NSNumber : NSObject` と間を省ける）。正しさは宣言の責任。
- 本体はない（M02 が `{ … }` を足す）。誤りは仕様 7。`private extern class` と、ほかのモジュールからの `Module.Name` は `extern type` と同じ規則。

```tsuzuri
@trusted
extern class NSResponder : NSObject

/// A window. Its runtime name is NSWindow.
@trusted
extern "NSWindow" class Window : NSResponder

@trusted
extern class sync NSNumber : NSObject
```

### 2. 型の性質

- `C`（`extern class` の型）と `ObjC.Object` の値は、ObjC の物体への強い参照を 1 つ所有する。Copy でも Clone でもなく（複製は `ObjC.retain`）、代入・引数・戻り値はムーブ。drop で `objc_release` を 1 回呼ぶ。record・union・タプル・`Maybe`・`Vec`・配列に入れてよい。
- 既定（`sync` なし）と `ObjC.Object` は W19 の `local` と同じ: Send・Sync でなく、関数値に捕捉できず（`E1005`）、tzroutine などで開始する関数が触ると `E1013` `pinned_code_spawned`、触る関数は効果 `pinned`。`Anchor<T>` には入れられる（W23 の D8）。
- `sync` は Send かつ Sync: tzroutine の本体・`Owned.Function`・チャンネルへムーブでき、`Arc<C>`・`Routine.scope` で共有できる。再利用できる関数値には捕捉できない（`E1005`）。関数値の複製は捕捉を複製するので、drop で release する値を捕捉すると二重の release になる（`src/check.rs:1082-1083` の注記）。所有ハンドル `extern type H` も 1113 行で捕捉できず、「現状」4 のとおり `Owned.function` と `tzroutine { }` にだけ渡せる。`sync` のクラスはこの形に合わせる。
- `ObjC.Class`・`ObjC.Sel` は `extern type copy` と同じ（Copy・Send・Sync・捕捉可。所有せず drop しない）。
- `Eq`・`Display`・`Hash` のインスタンスはない。`const`・`pure def` の `own` では今のハンドルと同じく平たいデータでない。基底への暗黙の変換はしない（`ObjC.upcast` を使う）。
- 借用 `ref C` は `id` を +0 で渡す。`Maybe<C>` は NS01 の NULL の niche で `ptr` 1 つ（`None` = nil）。`Maybe` でない `C` の位置の nil は境界で `null extern handle`。

### 3. `extern def` の位置と所有の属性

| 位置 | 書ける型 | 所有 |
| --- | --- | --- |
| 引数 | `ref C`・`Maybe<ref C>` | +0（借用。retain しない） |
| 結果 | `C`・`Maybe<C>` | +0 として受け、呼び出しに `clang.arc.attachedcall` の bundle を付けて直後に `objc_retainAutoreleasedReturnValue` を呼ぶ（D6）。`@returns_retained` なら +1 として受け、retain しない |
| 引数・結果・静的コールバックの引数 | `ObjC.Class`・`ObjC.Sel`（`Maybe`・`ref` を含む） | 所有なし |

- 所有する `C` の引数は `E1008` `objc_owned_object_argument`。ObjC の物体の型を `export def`・`@c_layout` の field・`FnPtr<F>`・静的コールバックの引数と結果・`extern view`・`ref mut`・バッファの要素に置くと `E1008` `objc_object_position`。M01 だけでは `ref mut` の物体の型はすべてこの kind で、[M23](M23-objc-out-parameters-and-weak-references.md) の Phase 1 が `extern def`・メンバーの引数の `ref mut Maybe<C>` を受理し、`ref mut C` を `E1008` `objc_inout_object` に分ける（M23 の D4）。`ObjC.Class`・`ObjC.Sel` も表の位置のほかは同じ kind。ObjC の型は W14 の分類で `ptr` に下がり、同じ symbol の規則は変えない。
- `@returns_retained`・`@returns_not_retained` は `extern def` の前だけ（M02 がメンバーの前を足す）。結果の既定は +0（C の関数の ARC の規則）で、`@returns_not_retained` は既定と同じだが、M07 が SDK の注釈を機械的に写すので受理する。+1 を返す関数の属性の忘れは 1 つ漏れるだけ（安全側）、+0 の関数に `@returns_retained` を付けるのは解放後の使用（宣言の誤り。監査の対象）。

### 4. 組み込みのモジュール `ObjC`

| 名前 | 型 | 意味 |
| --- | --- | --- |
| `ObjC.Object`・`ObjC.Class`・`ObjC.Sel` | 型 | `id`（所有・`local`）、`Class`・`SEL`（copy） |
| `ObjC.retain` | `ref C -> C` | `objc_retain` を呼び、同じ物体の所有値を返す |
| `ObjC.upcast` | `ref C -> ref B` | 基底の型の借用として見る。費用なし。結果は引数と同じ場所を借りる（`Rc.get` と同じ） |
| `ObjC.upcast_owned` | `C -> B` | 所有値の型を基底に変える。費用なし |
| `ObjC.downcast` | `ref C -> Maybe<D>` | `objc_opt_isKindOfClass(物体, D の classref)` が真なら `objc_retain` して `Some`、偽なら `None`。`D` が `ObjC.Object` なら検査せず `Some` |
| `ObjC.sel` | `string -> ObjC.Sel` | コンパイル時の文字列（リテラルか `string` の `const`）の selref を読む |

- `C` は引数から、`B`・`D` は期待型から決まる。決まらなければ `E1015` `ambiguous_polymorphic_use`、クラスでなければ `E1005` `objc_target_not_class`。`B` は `C` 自身か宣言の連鎖の祖先（`ObjC.Object` を含む）でなければ `E1003` `objc_not_superclass`。`D` は任意のクラス。
- セレクターは ASCII の識別子 `[A-Za-z_][A-Za-z0-9_]*` 1 つ（引数なし）か、`識別子:` の 1 回以上のくり返し。空の部分（`foo::`）やほかの文字は `objc_invalid_selector`（M02 のメンバーのセレクターも同じ検査を使う。M02 の仕様 2）。長さの上限は 1,024 バイトで、超えると `E1017` `objc_class_limit`（M02 と同じ上限。SDK には 255 バイトを超えるセレクターがある。起票の横断の検査で、macOS 27.2 の SDK の `MPSSVGF.h` に 392 バイト、`MTLIndirectCommandEncoder.h` に 257 バイトのセレクターを見つけた）。
- どれも直接の呼び出しだけ（関数値は `E1008` `objc_builtin_not_called`）。1 引数か対象のない関数なので D-59 の例外は要らない。`ObjC` は std のファイルを持たない組み込みのモジュールで、`RESERVED_MODULES` に足す（HEAD では 78 から 79）。利用者の `ObjC.tz` は `E1011` になる（互換を破る。D9、`docs/stability.md`）。

### 5. target・WASM・リンク

- 型検査は target を知らず、`check` は宣言も使用も受理する。`build` の生成の段で、到達した ObjC の使用（`ObjC.*` の呼び出しと、ObjC の型を引数か結果に持つ `extern def` の呼び出し。物体の値はこの 2 つからしか生まれない。静的コールバックの引数の `ObjC.Class`・`ObjC.Sel` は所有がなく、ObjC の symbol も生成しないので数えない）があれば、wasm32・wasm64 は `E1008` `abi_native_only`、Apple でない native は `E1008` `abi_apple_only`（新しい kind。M02〜M05・M10・M17・M23 が共有し、最初に出荷したチケットが登録する。メッセージ案 `Objective-C classes and 'ObjC' need an Apple target; build for aarch64-apple-darwin`）。宣言だけなら、どの target でも出力は変わらない。
- Apple の target（`Platform::mach_o()`）では classref・selref・6 つのフラグを出す（設計）。
- ObjC のフラグを持つ IR を Tsuzuri がリンクするとき（実行ファイル・共有ライブラリ・テストとベンチ）、driver が `-lobjc` を足す。framework のクラスの classref はその framework のリンクが要り、利用者が `--link <SDK>/…/AppKit.tbd` か M20 の `--framework` で渡す。`--emit object` などには何も足さない（ホストが渡す）。同梱の zig は M20 まで今の `E2002` のまま（M01 は SDK を探さない。文書に `TSUZURI_CLANG` の手順を書く）。

### 6. 効果・権限・監査

- `ObjC.retain`・`ObjC.downcast`・物体の drop・結果の retain は効果 `extern`（drop も外部のコードを呼ぶので `user_drop` でない。W25 と同じ）と権限 `ffi`。`upcast`・`upcast_owned`・`sel` は効果なし。`local` のクラスを触る関数は `pinned`。
- `@trusted` のない `extern class` を `extern` でない関数が使うと、クラスの名前に `W1010` `untrusted_extern`。
- `tsuzuri audit` はクラスごとに `extern` の行を 1 つ足す（`name` はクラス名、`symbol` は `OBJC_CLASS_$_<実行時名>`、`used` はそのクラスを型引数に持つ `ObjC.*` と、引数か結果に持つ `extern def` の呼び出しの数。drop は数えない）。`tz-audit/1` のキーは変えない。

### 7. 診断

新しいコードは割り当てない（D-51・E12 D12）。足す kind:

| kind | コード | 条件 |
| --- | --- | --- |
| `extern_class_syntax` | E0002 | 基底がない、`:` の後が型の名前でない、`<`・`=`・`{` が続く、文字列が 2 つ、`local`・`copy`（本体は `an extern class declaration has no body; remove the braces`） |
| `misplaced_returns_retained_attribute`・`returns_retained_conflict` | E0002 | 所有の属性が `extern def` の前にない・両方か 2 回 |
| `objc_class_base_not_class`・`objc_not_superclass` | E1003 | 基底がクラスでない・`upcast` の目的が祖先でない |
| `objc_target_not_class` | E1005 | `ObjC.*` の型引数がクラスでない |
| `objc_class_base_cycle`・`objc_class_sync_base`・`invalid_objc_class_name` | E1008 | 基底の循環（自分を含む）・`sync` の基底の非 `sync` の派生・実行時名の形 |
| `objc_owned_object_argument`・`objc_object_position`・`objc_returns_retained_not_object` | E1008 | 所有する `C` の引数・仕様 3 の表にない位置・結果が物体でない関数の所有の属性 |
| `objc_selector_not_constant`・`objc_invalid_selector`・`objc_builtin_not_called` | E1008 | `ObjC.sel` の引数が定数でない・形が違う、`ObjC.*` を関数値にした |
| `abi_apple_only` | E1008 | Apple でない native で到達した |
| `objc_class_limit` | E1017 | 連鎖の深さが 32 を超える、セレクターが 1,024 バイトを超える（M02 がメンバーの数、M03 がプロトコルの深さと数にも使う） |

既存の kind を使うもの: 同じ名前の型 `duplicate_name`（E1001）、`@trusted pure extern class` の `trusted_type_property`（E1037）、`ambiguous_polymorphic_use`（E1015）、`abi_native_only`（E1008）、`reserved_std_namespace`（E1011）、知らない基底の `E1004`、W19 の `E1005`・`E1013`、`use_after_move`（E1012）。

## 設計

- **構文**（`src/syntax.rs`・`src/parser.rs`）: `ExternClassDecl { doc, visibility, runtime_name: Option<(String, Span)>, sync: Option<Span>, name: Ident, base: TypeExpr }`、`Program.extern_classes`（`extern_types` の次）、`Program.returns_retained: Vec<OwnershipAttribute { target, retained, span }>`（`trusted` と同じ脇のリスト）。`extern_declaration` は `link_name` の後で `TokenKind::Class` を見る。`sync` は W18 の `modifier_kind` と同じ判定。`Attributes` に所有の印を足し、置き場の検査を `TrustedWrongTarget` の所（663-670 行）とトップレベルの分岐（761-827 行）に足す。M22 がなければ 2397 行の先読みに名前を足す。`src/syntax_codec.rs`・`GOLDEN`・`FRONTEND_FORMAT`（HEAD で 27）+ 1。`grep -rln extern_types src` の各所にクラスを足す（`tsuzuri fmt` は `extern "NSWindow" class sync Window : NSResponder`、`api-diff` は削除・基底・実行時名・`sync` の変更を互換のない変更、KG は `extern type` と同じ `handle` の節点で header に宣言の行）。
- **型**（`src/check.rs`・`src/abi.rs`）: `HandleKind` に `ObjcLocal`・`ObjcSync`（M09 の Phase 2 が `ObjcMain`）。`ObjC.Object` は `Type::Handle("ObjC.Object", ObjcLocal)`、`ObjC.Class`・`ObjC.Sel` は `Copy` の組み込みの名前で、型の式の解決で先に引く。`HandleKind` に述語 `thread_bound()`・`owned()`・`objc()`・`syncable()` を足し、28 か所をこの述語か網羅的な `match` に書き換える。性質: `is_copy`・`can_capture` は偽、`needs_drop` は真、`can_send`・`can_sync` は `ObjcSync` だけ真、`holds_host_state` は真。`thread_bound` の門は、`sync` でない `extern class` の宣言か `ObjC.Object` の解決でも真にする。`Names.objc_classes: BTreeMap<String, ObjcClassInfo { name, runtime_name, base, sync, trusted, depth, span }>` を登録の後に作り、循環・深さ・`sync` の規則を一度だけ検査して `CheckedModule` の脇の表に写す。ABI は `objc_object(ty, types)` を足し、`is_handle`・`abi_scalar` を使うすべての所で物体の型を先に拒む。
- **組み込み**: `Builtin::ObjcRetain`・`ObjcUpcast`・`ObjcUpcastOwned`・`ObjcDowncast`・`ObjcSel`。`Builtin::ALL`（`src/check.rs:2246`）・`name`・`signature`・`emit_builtin`（`src/llvm.rs:8458`）、`src/effects.rs`（`builtin_facts`・`builtin_why`）・`src/pure_rules.rs`（`builtin_effect`・`builtin_impure`）・`src/kg/effects.rs`（`builtin_effect`）・`src/capabilities.rs`・`pinned_rules::builtin_position`（`src/pinned_rules.rs:33`）を更新する。`ObjcUpcast` の結果の region は `Builtin::RcGet` と同じ。型引数がクラスであることの検査（`E1005` `objc_target_not_class`）は、`AtomicValue` の制約と同じ形で `src/polymorph.rs` の `validate_instance`（3684 行。`AtomicValue` の腕は 3817 行）に置く。
- **生成**（`src/llvm_objc.rs`）: `globals` に `objc_classrefs: BTreeSet<String>`・`objc_selectors: BTreeMap<String, usize>`（最初に使った順の番号。emitter は 1 本で決定的）・`objc_used`。大域は「現状」6 の IR と同じ形で末尾に出す（classref は名前の順、selref は番号の順、`@"OBJC_CLASS_$_<名前>" = external global [0 x i8]`）。selref の読み出しにだけ `!invariant.load !{}`。drop は `drop_value` の腕で `call void @objc_release(ptr %v)`（`Maybe<C>` は今の niche の drop glue を通る）。`ObjC.retain` は `objc_retain`、`downcast` は classref の読み出し・`objc_opt_isKindOfClass`・真の枝の `objc_retain`。`upcast` 系は命令なし。+0 の結果は、`host_call` が 426 行の `self.value(call)` に渡す `call` の文字列の末尾に ` [ "clang.arc.attachedcall"(ptr @llvm.objc.retainAutoreleasedReturnValue) ]` を付け、`declare ptr @llvm.objc.retainAutoreleasedReturnValue(ptr)` を 1 行足す（D6）。結果は retain 済みの値で、NULL の検査（436-440 行）と `Maybe` の変換はその後に今のとおり行う。W14 の正準形で結果が `i64` に下がる呼び出し（`convert_ptr_result`）には bundle を付けられないので、`inttoptr` の後に素の `call ptr @objc_retainAutoreleasedReturnValue(ptr %value)` を置く（ハンドシェイクはなく、物体はプールに入る。意味は同じ）。M23 の物体の出力引数を持つ呼び出しだけは、この `inttoptr` と素の retain を `finish_copy_outs` の前に置く（M23 の仕様 2・設計）。M01 だけの呼び出しの並びは変わらない。`declare` は使った symbol だけ 1 行ずつ（形は参照と同じ）で、利用者の `extern` が同じ symbol を宣言していれば入れず、呼び出しは自分の関数型で `call` する。フラグは `objc_used` のときだけ足し、`-g` の一覧があればそこに足す（`with_codeview` と同じ）。`validate_objc_target` を `validate_wasm_externs` の隣に置き、ObjC の宣言も使用もないモジュールでは何もしない。コンパイラの呼び出しは `with_blocking_externs` で包まない（利用者が同じ symbol を宣言していれば行の照合で包まれる。意味は同じで費用だけ増える）。
- **driver**: `llvm::uses_objc(text)`（`!"Objective-C Version"` を含むか）と `objc_link_libraries(text, platform)` を足し、Mach-O でリンクするときに 5 つのリンク行で利用者のリンク入力の後に `-lobjc` を足す（`native_runtime` の分岐の外）。

## 前提とする他チケットのインターフェース

- [W18](_completed/W18-copy-extern-type.md)・[W19](_completed/W19-thread-bound-handles.md): `HandleKind`、`Type::Handle`、`holds_local_handle`、`Effect::Pinned`、`pinned_rules::builtin_position`、`E1013` の kind、`thread_bound`。名前が違えば止めて報告する（GUIDE-LITE §11）。[NS01](_completed/NS01-nonnull-extern-handles.md): 非 NULL の不変条件と niche、`TrapKind::NullHandle`。[W14](_completed/W14-extern-multiple-declarations.md)・[W15](_completed/W15-abi-nullable-handles.md): `CSignature` と `Maybe<ref H>`。[E15](_completed/E15-trust-boundary-and-capabilities.md): `@trusted`・`W1010`・`ExternReport`。
- [M22](_completed/M22-attribute-after-bodyless-extern-def.md)（弱い）: 名前によらない属性の先読み。[M20](_completed/M20-macos-framework-link-inputs.md)（弱い）: 同梱の zig の SDK の探索。

## 他チケットへ提供するインターフェース

- [M02](M02-objc-typed-message-send.md): `ObjcClassInfo`、`llvm_objc::classref(runtime_name)`・`selref(selector)`（読み出した値の名前を返す）、+0 の結果の下げ方、所有の属性の脇のリスト（メンバーの置き場を足す）、`abi_apple_only`・`objc_object_position`。M02 が本体を受理した後も `extern_class_syntax` は基底の欠落などに残る。
- [M03](M03-objc-classes-defined-in-tsuzuri.md)・[M04](M04-objc-blocks.md)・[M05](M05-objc-exception-boundary.md)・[M10](M10-objc-autorelease-pools.md): `HandleKind` の種類と述語、`ObjC` のモジュール（`ObjC.Block`・`ObjC.Exception`・`ObjC.with_autorelease_pool` を足す）、`uses_objc` と `-lobjc`、`@llvm.compiler.used` の表。M04 の `ObjC.Block<F>` は型引数を運ぶので `HandleKind` の種類ではなく、名前で特別に扱う record で表す（M04 の D10）。`ObjC` は std のファイルを持たないので、その record の置き場は M04 の D10 で決める。[M06](_completed/M06-extern-data-symbols-and-zutf8string.md): 読んだ値の retain と、そのとき立てる `objc_used`（モジュールのフラグ・`-lobjc`・target の検査）。[M09](M09-main-thread-dispatch-and-handles.md): `sync` と同じ位置の `main` と `ObjcMain`。[M17](_completed/M17-macos-min-version-and-weak-import.md): classref の大域（`@weak` のクラスの NULL の検査）。[M07](M07-apple-sdk-bindgen-and-package.md): 文法、`NS_SWIFT_SENDABLE` から `sync`、SDK の注釈から所有の属性、深さ 32。
- [M23](M23-objc-out-parameters-and-weak-references.md): 物体の型の判定（`HandleKind::ObjcLocal`・`ObjcSync` と述語）、`Maybe<C>` が NS01 の niche（`ptr` 1 つ）であること（出力引数の場所のアドレスを `objc_storeStrong` にそのまま渡す。M23 の S1）、位置の検査と kind `objc_object_position`（M23 が `extern def`・メンバーの引数の `ref mut Maybe<C>` を通し、`ref mut C` を `objc_inout_object` に分ける）・`objc_target_not_class`・`objc_builtin_not_called`・`abi_apple_only`、`validate_objc_target`（引数の型を `ref mut` と `Maybe` の中まで見て、ObjC の型を引数に持つ呼び出しに数える）、`objc_used`、`declare` の 1 行の規則、D6 の +0 の結果の下げ方、監査の行（`used` は出力引数の `C` も数える）。

## 実装手順

1. **変更**: なし（ブランチ `feat/m01-objc-class-types`）。**内容**: 着手条件の確認、ベースラインの保存、clang の参照の IR（「現状」6）の作り直し。**確認**: 件数を控える。
2. **変更**: `src/syntax.rs`・`src/parser.rs`・`src/syntax_codec.rs`・`src/frontend_cache.rs`・`src/formatter.rs` と `extern_types` の各所。**内容**: 仕様 1 と `extern_class_syntax`。**確認**: 構文の検査、`cargo test --locked --test formatter`、`pins_encoding_format`。
3. **変更**: `src/check.rs`・`src/abi.rs`・`src/stdlib.rs`・`src/diagnostic_kinds.rs`・`src/pinned_rules.rs`。**内容**: `HandleKind` と述語、28 か所、`ObjcClassInfo`、`thread_bound`、`ObjC` の予約、位置の検査、`ObjC.*` の型。**確認**: 型の検査、`cargo nextest run --locked`（S3）、`reserves_the_d07_table` を 79 に。
4. **変更**: `src/parser.rs`・`src/check.rs`。**内容**: 所有の属性。**確認**: 属性の検査。
5. **変更**: `src/llvm_objc.rs`・`src/llvm.rs`・`src/llvm_imports.rs`・`src/llvm_debug.rs`。**内容**: 生成のすべて。**確認**: IR の検査（期待は「現状」6 の参照から手で書く）、ベースラインと `cmp`（S2）、2 回の IR が一致。
6. **変更**: `src/llvm.rs`。**内容**: `validate_objc_target`。**確認**: wasm32 と Linux の検査。
7. **変更**: `src/driver.rs`・`src/test_runner.rs`。**内容**: `-lobjc`。**確認**: `objc_link_libraries` の単体テスト、macOS で `-l objc` なしにリンクできる。
8. **変更**: `src/effects.rs`・`src/pure_rules.rs`・`src/kg/effects.rs`・`src/capabilities.rs`。**内容**: 仕様 6。**確認**: `tests/capabilities.rs`・`tests/audit.rs`・`tests/trusted.rs` にクラスの case、件数の pin。
9. **変更**: `tests/objc_classes_host.m`・`tests/objc_classes.mjs`・`tests/fixtures/objc_classes/Main.tz`・`.github/workflows/targets.yml`。**内容**: E2E。**確認**: `node tests/objc_classes.mjs target/debug/tsuzuri`。
10. **変更**: 文書。**確認**: T2、`node scripts/gen-docs.mjs --check`、`node scripts/check-docs.mjs <変更したページ>`、PR に `ci:full`。

## テスト計画

### Rust（`tests/objc_classes.rs`。どのホストでも）

| 検査 | 期待 |
| --- | --- |
| 受理 | 早見の連鎖、実行時名つき、`sync`、`extern class sync : NSObject`（名前 `sync`）、`private` |
| 拒否 | 仕様 7 の kind をそれぞれ 1 件以上（基底なし・`{`・文字列 2 つ・`local`、`"9x"`、基底が record と `ObjC.Class`、循環 2 つと自己、深さ 33 と 1,025 バイトのセレクター（1,024 バイトと 392 バイトは通る）、`sync` の規則、同じ名前、`@trusted pure`、`ObjC.tz`、位置の 7 種、属性の 3 種、`ObjC.sel` の 2 種、関数値、`upcast` の非祖先と注釈なし、`downcast` の目的が record） |
| 性質 | 既定のクラスの捕捉が `E1005`、tzroutine が `E1013`、二重の使用が `E1012`、`sync` は tzroutine へムーブできる、再帰の union の中の既定のクラスも Send でない |
| IR（`--target aarch64-apple-darwin`） | classref・methname・selref・`@llvm.compiler.used`・フラグ 6 つが「現状」6 と同じ形、`!invariant.load` は selref だけ、同じセレクターの selref は 1 つ、drop・`@returns_retained`・`downcast` の形、+0 の結果の `call` に `clang.arc.attachedcall` の bundle が付き `declare ptr @llvm.objc.retainAutoreleasedReturnValue(ptr)` が 1 行、2 回の IR が一致 |
| 重複と `-g` | 利用者の `extern "objc_release" def release :: i64 -> unit` とクラスの drop で `declare` が 1 行・呼び出しは `ptr`。同じ symbol を `i64` の結果とクラスの結果で 2 回宣言すると、クラスの結果の呼び出しは bundle なしの素の `objc_retainAutoreleasedReturnValue` になる。`-g` の `!llvm.module.flags` が 1 行で DWARF と ObjC の両方 |
| TZ08 | tzroutine を起動するプログラムで、+0 の結果の `call` の行（bundle を含む）が `tsuzuri_tz_blocking_enter`・`_exit` の間にそのまま残る |
| target | wasm32 で到達すると `abi_native_only`、`x86_64-unknown-linux-gnu` で `abi_apple_only`、宣言だけなら成功。ObjC のない入力の IR はベースラインと同じ |

`src/driver.rs` の単体テスト: `objc_link_libraries` がフラグのある IR と Mach-O で `["objc"]`、それ以外で空。

### E2E（`tests/objc_classes.mjs`・`tests/objc_classes_host.m`。macOS だけ）

- ホストは `clang -fobjc-arc -c`（CI の Homebrew の clang には `-isysroot $(xcrun --show-sdk-path)`）。`#import <objc/NSObject.h>` だけを使い、`-lobjc` だけでリンクできる。`TzProbe : NSObject`（`init` で `tzobjc_live` を増やし `dealloc` で減らす。tag 21）、`TzProbeChild : TzProbe`、`TzShared : NSObject`。関数は +0 の `tzobjc_make_child`・`tzobjc_make`、`NS_RETURNS_RETAINED` の `tzobjc_new_probe`、`tzobjc_maybe(int)`、`tzobjc_nil`、`tzobjc_tag`、`tzobjc_live`、`int tzobjc_sel_is(SEL, const char *)`。期待はホストの dealloc の数（独立した参照）から作る。
- M10 の前なので fixture は `objc_autoreleasePoolPush`・`Pop` を `extern` で呼ぶ。`-O0`・`-O3`、`--allocator counting` で `live == 0`。

| case | 期待 |
| --- | --- |
| `balance`（早見の例） | `42 0` |
| `retained`（`@returns_retained`）/ `forgotten_attribute`（同じ関数を属性なし） | `tzobjc_live` が 0 / 1（安全側の漏れ） |
| `retain_and_drop` | 1 つ目の drop の後 1、2 つ目の後 0 |
| `handshake`（+0 の `tzobjc_make` の値をプールの pop の前に捨てる） | pop の前に `tzobjc_live` が 0（「現状」8 の attached と同じ。ランタイムの最適化に頼る case なので、ほかの macOS で 1 になったら期待を書き換えずに止めて報告する） |
| `maybe_nil` / `nil_trap`（`--trap-info`） | `None` / `trap: null extern handle at …`、終了コード 133 |
| `downcast`（親を子へ、子を親と `NSObject` へ） | `None`・`Some`・`Some` |
| `moves`（record と `Vec`）/ `sync_thread`（`TzShared` を tzroutine で捨てる） | どちらも `tzobjc_live` が 0 |
| `selector`（`ObjC.sel "description"`） | 1 |
| `zig` / `swift_imageinfo`（`swiftc` があるとき `--emit object` と Swift の目的ファイル） | M20 の後は同じ出力（前は飛ばし理由を出す）/ 実行でき、`__objc_imageinfo` が Swift の値 |

`targets.yml` の darwin-arm64 の段に `tests/debugger_static.mjs` と同じ形（`if [ "$RUNNER_OS" = macOS ]`）で足す。GUI は使わない。性能の主張はしない（PB07 の分割なしのビルド時間だけ記録し、閾値は置かない）。既存の `tests/thread_bound_handles.rs`・`tests/ffi_extensions.*`・`tests/trusted.rs`・`tests/audit.rs`・`tests/capabilities.rs` は足した case のほかは変わらない。

## ドキュメント

- `_tsuzuri/language-reference/compiler/objective-c.md`（新規。`index.md` に足す）: 宣言、`local` と `sync`、所有と nil、位置と所有の属性、`ObjC` と「引数の順序」、target、リンク（`-lobjc`、`.tbd` の `--link`、同梱の zig の制限）、プールの注意（今は `extern` で push・pop。組み込みは計画中で、ページから M10 へリンクする。ページからの相対パスは `../../../_features/M10-objc-autorelease-pools.md`）。例はホストが要るので `run=` にしない。
- `compiler/native-interop.md`（212 行の不透明ハンドルからリンク）、`values-and-functions/attributes.md`、`compiler/platforms.md`・`compiler/webassembly.md`、`organizing-tsuzuri/modules.md`・`docs/language.md` の予約名（modules.md は HEAD で 13 行に「74 個」、114 行に「73 個」と書き、`RESERVED_MODULES` の 78 個と合わない。足すときに一覧と数を `RESERVED_MODULES` に合わせる）、`built-in-types-and-modules/argument-order.md` の `ObjC` の行、`docs/language.md` の「ホスト関数のインポート」（449 行）に小節。
- 診断の表は `src/diagnostic_kinds.rs` に登録して `node scripts/gen-docs.mjs`。`src/explain/codes/E1008.md` に段落。`docs/stability.md` に `ObjC` の予約。`docs/architecture.md` に生成の形・`declare` の規則・PB07。
- `_features/README.md:114・116` と strategy.md:200・202 を、出荷した形に合わせて改める（D-67 の「文言の改訂の時期」）。README の特徴の一覧が C ABI の連携を挙げていれば ObjC を足す。

## 受け入れ条件

- [ ] 仕様 1〜6 のとおりに動き、仕様外の入力は仕様 7 の診断で拒否される。
- [ ] ObjC を使わないプログラムの IR・header・`check --json` がベースラインと 1 byte も違わず、WASM の出力も変わらない（例外は利用者の `ObjC.tz` が `E1011` になることだけ。D9）。
- [ ] 生成 IR が決定的で、classref・selref・`declare` が重複しない（利用者の `extern` と同じ symbol でも 1 行）。
- [ ] `tests/objc_classes.rs` と、macOS の `-O0`・`-O3` の `node tests/objc_classes.mjs` が通り、`tzobjc_live == 0` と `live == 0` を確かめた。
- [ ] Apple の ld で通り、M20 があれば同梱の zig でも通る。PR に `ci:full` を付けた。
- [ ] 登録（D-55）・`docs/stability.md`・言語リファレンスを更新した。T2 が通る。

## 落とし穴

- `matches!(ty, Type::Handle(_, HandleKind::Local))` は `ObjcLocal` を拾わず、既定のクラスが黙って Send になる。述語か網羅的な `match` に寄せる。
- `is_handle` はどのハンドルも通すので、物体の型を明示に拒まないと `export def`・コールバック・`@c_layout` に入り、header にも出る。
- `thread_bound` が `extern type local` だけを見ると、クラスしか宣言しないプログラムで `pinned` の解析が走らない。
- `!llvm.module.flags` は 1 行にまとめる。LLVM の parser は 2 行目を併合して受け付ける（「現状」8）が、`with_codeview`（`src/llvm_debug.rs:71`）は最初の行だけを書き換え、同じ意味の IR が 2 通りの形になる。
- `@llvm.compiler.used` で `-O0` の PB07 の分割が外れる。ObjC のプログラムに限り、時間の差を記録する。
- +0 の結果を素の `objc_retainAutoreleasedReturnValue` で受けると、ハンドシェイクは効かず、物体は autorelease pool に入る（「現状」8 で Apple clang と Homebrew clang の `-O0`・`-O3` とも）。意味は同じだが、M10 の前のプールのない tzroutine ではスレッドの終わりまで残る。印の inline assembly も、結果を後で使うと `-O0` の spill で隣でなくなる。bundle は `call` の一部なので、TZ08 の `with_blocking_externs` が行の前後に `tsuzuri_tz_blocking_enter`・`_exit` を挟んでも隣のまま残る。素の形を使うのは `convert_ptr_result` の呼び出しだけにする。
- bundle の下がる先は deployment target と最適化で変わる。Apple clang 21 で、bundle を付けた `call` の `.ll` と `invoke`（M05 の形）の `.ll` を `-target arm64-apple-macosx{12.0,13.0,27.2}` の `-O0`・`-O3` で `-c` すると、`-O0` と 12.0 の `-O3` は `bl; mov x29, x29; bl _objc_retainAutoreleasedReturnValue`、13.0 と 27.2 の `-O3` は印のない `bl _objc_claimAutoreleasedReturnValue` になった（起票の横断の検査で確かめた）。どちらも libobjc が export し、意味は同じ。目的ファイルを検査するテストは、この symbol の名前を固定しない。HEAD の既定の native の minos はビルド機の版（実測で 27.2）なので、`--target` のない `-O3` の実行ファイルは `objc_claimAutoreleasedReturnValue` を引き、`--target aarch64-apple-darwin`（12.0）は `objc_retainAutoreleasedReturnValue` を引く。D-67 の分岐 6 は、既定を名前付きの target と同じ 12.0 に固定すると決めた（承認済み、2026-11-26。実装は [M17](_completed/M17-macos-min-version-and-weak-import.md)）。M17 の後は、版を上げない限り `--target` のない `-O3` も `objc_retainAutoreleasedReturnValue` を引く。
- IR の文字列を読む後段（`with_blocking_externs` の `direct_callee`、`src/llvm_facts.rs` の `memory_free_call`、`src/llvm_preempt.rs` の `with_preemption_checks`）が、operand bundle の付いた `call` の行を読み違えないか確かめる。HEAD の Tsuzuri は bundle を出さない（`src/llvm_facts.rs:1028` のテストが `deopt` の bundle を読むだけ）。
- 2 つのモジュールが同じクラスを宣言すると別の型になる（M07 に集める）。連鎖の嘘は `upcast` を誤らせ、M02 の送信が `unrecognized selector` になる（監査の対象）。
- `downcast` は classref を直接読むので `D` の `+initialize` は走らない（比較には要らない）。Apple の ld は外部のクラスの `__objc_classrefs` を出力から除くので、節は目的ファイルで確かめる。

## 対象外

- メンバー・送信・クラスメソッド・`Self`・カテゴリー・`ObjC.literal`・`+class`（M02）、クラスの定義・プロトコル・`Super`（M03）、Block（M04）、例外（M05）、データシンボル（M06）、`main` の性質（M09）、プールの組み込み（M10）、Intel Mac（M18）、同梱の zig の SDK の探索と `--framework`（M20）。
- ARC の最適化（retain と release の対の除去）と、`llvm.objc.*` の intrinsic の呼び出しと ObjCARC のパス（D4。`clang.arc.attachedcall` の被演算子に名前を置く `@llvm.objc.retainAutoreleasedReturnValue` だけを宣言する。D6）。軽量ジェネリクス、同一性の比較（`==`）、弱い参照（`__weak`）と `NSError **` の引数（物体の出力引数）は [M23](M23-objc-out-parameters-and-weak-references.md) が扱う（出力引数は W13 の `ref mut` を所有する物体に広げる設計で、M01・M02 の後の M23 の Phase 1。弱い参照はアドレスの変わらない領域が要り、M01 の後の M23 の Phase 2）。
- `export def`・`@c_layout`・`FnPtr`・コールバック・`extern view` の物体の型（`E1008`）。GNUstep などのランタイム。CoreFoundation の型と toll-free bridging（今の `extern type` で書く）。利用者の `objc_msgSend` への W14 の規則。PB07 の分割器の `@llvm.compiler.used` の併合。

## 決定事項

| ID | 決定 | 採らない案 | 理由 | 状態 |
| --- | --- | --- | --- | --- |
| D1 | 既定のスレッドの性質は W19 の `local`。`sync` は Send・Sync（`NS_SWIFT_SENDABLE` のクラス向け。M07 が写す）。明示の `local` は書かない。`main` は M09 の Phase 2 | 既定を Send（`extern type` と同じ）か `sync` にする。`local` の明示を求める | AppKit の物体を tzroutine へ送る誤りを型で止める最も保守的な既定。前 2 つは非スレッド安全な物体を別のスレッドで release させる。明示は綴りを 2 つにする | 承認済み（D-67 の委任で決定、2026-11-27） |
| D2 | `HandleKind` に `ObjcLocal`・`ObjcSync` を足し、クラスの情報は脇の表に置く。述語で 28 か所を書き換える | `Type` に variant を足す。`Local` を再利用して脇の表だけで区別する | 前者は `Type` の大きさとすべての `match` を変える（S1）。後者は `sync`（所有・Send・Sync・drop）を表せない | 既定案（実装者はこの案に従う） |
| D3 | 属性の名前は `@returns_retained`・`@returns_not_retained`。M01 は `extern def` の前、M02 がメンバーの前 | `@ns_returns_retained`、`@owned`、結果の包み `Retained<C>` | `ns` は Foundation の接頭辞で CF の注釈と食い違う。`@owned` は `own` の規約と紛らわしい。包みは型の登録が要り署名が長い | 承認済み（D-67 の委任で決定、2026-11-27） |
| D4 | retain と release は素の `objc_retain`・`objc_release` の呼び出し（+0 の結果の受け取りは D6） | `llvm.objc.*` の intrinsic の呼び出しと ObjCARC のパス | clang は ARC のパスを `-fobjc-arc` のときだけ足し、`.ll` の compile では走らない。意味をパスに頼らず IR を決定的に保つ | 既定案（実装者はこの案に従う） |
| D5 | `downcast` は `objc_opt_isKindOfClass` | `isKindOfClass:` を送る | clang 21 が 12.0 の target で出す形で、型付きの送信（M02）を要らない | 既定案（実装者はこの案に従う） |
| D6 | +0 の結果は、呼び出しに `[ "clang.arc.attachedcall"(ptr @llvm.objc.retainAutoreleasedReturnValue) ]` を付けて受ける（clang 21 と同じ形）。`llvm.objc.*` の名前はこの被演算子の `declare` 1 行だけで、呼び出さず、ObjCARC のパスも要らない。W14 の正準形で結果が `i64` に下がる呼び出しだけは、直後に素の `objc_retainAutoreleasedReturnValue` を呼ぶ | 素の `objc_retainAutoreleasedReturnValue` だけ。印の inline assembly（`mov fp, fp`）と素の呼び出し。`objc_retain` | 「現状」8 で、素の呼び出しは Apple clang 21・Homebrew clang 21.1.8 の `-O0`・`-O3` のどれでもハンドシェイクが効かず、+0 の物体がすべてプールに入った。印は結果を後で使うと spill で隣でなくなった。bundle はどの場合も効き、TZ08 の包みの中でも `call` と隣のまま残る。LLVM の検証器は `i64` の結果の bundle を拒む。`objc_retain` もプールに入る。性能の主張はしない（プールに残るかどうかだけを確かめた） | 既定案（実装者はこの案に従う） |
| D7 | classref・selref・フラグは clang 21 と同じ形（`@llvm.compiler.used` を含む） | `@llvm.compiler.used` を使わず PB07 を保つ。実行時に `objc_getClass`・`sel_registerName` を呼ぶ | 前者は参照から外れ、classref の読み出しの畳み込みを防げるか確かめていない。後者は使うたびに費用がかかり、クラスの欠落がリンクで分からない | 既定案（実装者はこの案に従う） |
| D8 | 言語の範囲は D-67 の分岐 1 の X′。README:116 の「クラス継承」に注記を置く（分岐 1）。README:116 と strategy.md:202 の文言は出荷の PR で改める（言語リファレンスは今のコンパイラがすることだけを書く。D-67 の「文言の改訂の時期」） | Y′（ObjC を入れない）、Z′（W14 の例外の属性だけ）。どちらも M01 を閉じる案で、分岐 1 で選ばれなかった | M02〜M05・M07・M10 の土台。D-63 は広げず別の決定にする | 承認済み（2026-11-26。D-67 の分岐 1） |
| D9 | D-55 に `class` の新しい位置・文脈キーワード `sync`・組み込みのモジュール `ObjC`（`ObjC.tz` は `E1011`）を登録する | 新しい予約語、モジュール名 `ObjectiveC` | 予約語を足さない方針。`ObjectiveC` は長く、共通の綴りと食い違う | 承認済み（D-67 の委任で決定、2026-11-27） |
| D10 | `E1008` の kind `abi_apple_only` を登録して共有する（D-67 の共通の規則で、最初に出荷したチケットが登録する。M01 より先に [M17](_completed/M17-macos-min-version-and-weak-import.md) が出荷していれば、その登録を使う） | `E2002` の `platform_unsupported_api`、`E2000` | 到達した使用を生成の段で拒む点で `abi_native_only` と同じ種類。前者は driver の OS の API の判定、後者は CLI と target の誤り | 承認済み（D-67 の委任で決定、2026-11-27） |
| D11 | ObjC のフラグを持つ IR のリンクに driver が `-lobjc` を足す（5 か所） | 利用者が `-l objc` を書く。W17 の `@link` を待つ | 前者はすべての ObjC のプログラムが未定義の symbol で失敗する（`-lws2_32` の前例）。W17 は todo で、入れば寄せられる | 既定案（実装者はこの案に従う） |
| D12 | 新しいコードは割り当てず仕様 7 の kind を足す。本体の `{` は `extern_class_syntax` で拒む | 本体だけの kind を作り M02 で消す | kind は消さない（`docs/stability.md:18`）。`extern_class_syntax` は M02 の後も基底の欠落などに残る | 既定案（実装者はこの案に従う） |
| D13 | 物体を置ける位置は仕様 3 の表だけ。`ObjC.Class`・`ObjC.Sel` は静的コールバックの引数にも置ける | 所有する `C` を「消費」として許す。コールバックの引数に物体を許す | C の API はほとんど引数を消費せず、init の族は M02 が扱う。IMP の所有の規則（M03）なしに +0 の借用を保持させる | 既定案（実装者はこの案に従う） |
| D14 | `sync` の基底を持つクラスは `sync` でなければならない | 派生を `local` にしてよい。`sync` を暗黙に受け継ぐ | 前者は `upcast` で `local` の物体を別のスレッドへ送れる。後者は宣言から読めない（GUIDE §1 規則 2） | 既定案（実装者はこの案に従う） |
| D15 | `ObjC.*` は直接の呼び出しだけ | 関数値として許す | 型引数ごとに包みが要り、既定のクラスでは `pinned_function_value` がほとんどを拒む。例が出てから緩める | 既定案（実装者はこの案に従う） |
| D16 | wasm と Apple でない native は、到達した使用だけを生成の段で拒む | 宣言の時点で拒む | 型検査は target を知らず、macOS のモジュールを含むプロジェクトを wasm や Linux で検査できなくなる（W25 の D11 と同じ） | 既定案（実装者はこの案に従う） |

## 実装と検証（2026-11-27）

Phase のないチケットなので、手順 1〜10 のすべてを 1 つのブランチ（`feat/m01-m02-w36-objc-and-by-value`。M02・W36 と同じ PR）で実装した。

### 実装したもの

- **構文**: `ExternClassDecl`（`Program.extern_classes`）と所有の属性の脇のリスト `Program.returns_retained`（`OwnershipAttribute`）。`extern ["実行時名"] class [sync] Name : Base`、`@returns_retained`・`@returns_not_retained`、`@weak`（M17 のクラスの段）。`FRONTEND_FORMAT` は 33 から 34。`tsuzuri fmt`（`:` の両側に空白）、`doc`、`api-diff`（見出し・実行時名・`sync`・基底・`@weak`・`@returns_retained` の変更は互換なし）、KG（`handle` の節点）、索引、REPL、doctest に足した。
- **型**: `HandleKind::ObjcLocal`・`ObjcSync` と述語（`copy`・`owned`・`thread_bound`・`objc`・`syncable`）。`needs_drop` は `objc()` だけ真。`resolve_objc_classes` が基底・循環・深さ 32・`sync` の規則・実行時名を検査して `objc::ObjcClasses`（`CheckedModule::objc`）に写す。`ObjC.Object`・`ObjC.Class`・`ObjC.Sel` は名前表の最後に登録した組み込みのハンドル。`thread_bound` は `sync` でないクラスの宣言か `ObjC.Object` の解決でも真。
- **組み込み**: `ObjC.retain`・`upcast`・`upcast_owned`・`downcast`・`sel`（`Builtin::ALL` は 378 から 384。M02 の `ObjC.literal` を含む）。型引数・祖先・セレクターの検査は `validate_objc_builtins`。
- **ABI**: `abi::objc_extern_misuse`・`reaches_objc`・`objc_object_result`。`export def`・`@c_layout` の field・`FnPtr.of`・データシンボルの位置の検査。M06 のデータシンボルは物体（`C`・`Maybe<C>`）を retain して読む。
- **生成**（`src/llvm_objc.rs`）: classref・methname・selref・`@llvm.compiler.used`・6 つのモジュールフラグ（`-g` の行に併合）、`objc_release` の drop、`objc_retain`、`objc_opt_isKindOfClass`、+0 の結果の `clang.arc.attachedcall`（W14 で `i64` に下がる結果だけ素の `objc_retainAutoreleasedReturnValue`）、`declare` の 1 行の規則、`@weak` のクラスの検査、`validate_objc_target`。
- **driver**: `objc_link_libraries`・`add_objc_libraries`（W17 の system library に `objc` を足す）。同梱のツールチェーンでは M20 の SDK の探索を libobjc にも使う（`prepare_link_extras`）。
- **効果・権限・監査**: `ObjC.retain`・`ObjC.downcast` と物体の drop は効果 `extern`（`objc-release`）。`tsuzuri audit` はクラスごとに行を出し、`W1010` は `@trusted` のないクラスの使用に出る。

### 検証

- `cargo test --locked --test objc_classes`: 17 件が通過（受理・拒否の各 kind・性質・clang の参照と同じ形の IR・重複する `declare`・`-g`・TZ08 の包み・target・`@weak`・データシンボル）。期待の IR は clang の参照（「現状」6・8）から手で書いた。
- `node tests/objc_classes.mjs target/debug/tsuzuri`（arm64 macOS 27.2、Apple clang 21）: 32 検査が通過。`-O0`・`-O3` の 11 case と `nil_trap`、目的ファイルの `__objc_classrefs`・`__objc_selrefs`・`__objc_imageinfo`、`--allocator counting`、同梱のツールチェーン（zig・lld）でのリンクと実行。期待はホスト（`tests/objc_classes_host.m`）の dealloc の数から書いた。`handshake` は pool の pop の前に 0 になった。
- 早見のプログラムは `-O0`・`-O3` で `42 0` を出す。`-l objc` は要らない。
- T1・T2 の結果は PR の説明にまとめた（M02・W36 と同じ実行）。

### チケットから外れた判断

- **E2E のプログラムの置き場**: `tests/fixtures/objc_classes/` でなく `tests/objc/classes/` に置いた。`tests/fixtures/` のディレクトリは、リポジトリ全体のテスト（`llvm::split` の `splits_the_programs_of_the_repository`、`tests/llvm_handoff.rs`、`tests/lto.rs`）がホストの target でビルドするので、Apple の target でしか到達できないプログラムを置くと、ほかのホストで `abi_apple_only` になる。
- **`ObjC.*` の下げ方**: `retain`・`upcast`・`upcast_owned`・`downcast` は、ほかの組み込みと同じく `$builtin` の包みを通る（`upcast` は `-O0` で包みの呼び出しが残る。意味は同じ）。`ObjC.sel` はコンパイル時の文字列を呼び出し地点で読むので、`FnPtr.of` と同じく包みにしない。
- **基底の `ObjC.Class`・`ObjC.Sel` と一般の型**: 組み込みの 3 つの型は `Names::handles` に登録した（型の式の解決で先に引くだけでは、公開 API の検査や別名の展開など `named_type` を通る経路で未知の型になった）。
- **`@weak` のクラス**（M17 が後のチケットに残した段）と **データシンボルの物体**（M06 が残した段）を、このチケットで実装した。
- **PB07**: `@llvm.compiler.used` で `-O0` の分割が外れるのは、Objective-C を使うプログラムに限られることを確かめた（`tests/objc/` のプログラムは分割器のテストの対象外に置いた）。時間の閾値は置いていない。

### 更新した文書

`docs/language.md`（「Objective-C のクラスとメッセージ送信」、予約モジュール名、診断の表）、`docs/architecture.md`（「Objective-C の連携（M01・M02）」）、`docs/stability.md`、`README.md`、`src/explain/codes/E1008.md`。Tsuzuri-Reference: `docs/compiler/objective-c.md`（新規）、`docs/index.md`、`docs/compiler/native-interop.md`・`platforms.md`・`webassembly.md`・`diagnostics.md`、`docs/expressions/contracts-and-attributes.md`、`docs/modules/namespaces-and-modules.md`、`docs/stdlib/conventions.md`。

### 変更したファイル

`src/{syntax,parser,syntax_codec,frontend_cache,formatter,docgen,api_diff,semantic,kg_capture,find,repl,doctest,guide_std,stdlib,check,polymorph,closures,abi,pinned_rules,effects,pure_rules,capabilities,llvm,llvm_imports,driver,test_runner,diagnostic_kinds,lib}.rs`、`src/kg/effects.rs`、`src/objc.rs`（新規）、`src/llvm_objc.rs`（新規）、`src/explain/codes/E1008.md`、`tests/objc_classes.rs`・`tests/objc_classes.mjs`・`tests/objc_classes_host.m`・`tests/objc/classes/Main.tz`（新規）、`.github/workflows/targets.yml`、上の文書。
