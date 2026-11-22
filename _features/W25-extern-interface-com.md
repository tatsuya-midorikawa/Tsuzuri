# W25: `extern interface`（COM の vtable のスロット呼び出し）

| 項目 | 内容 |
| --- | --- |
| ID | W25 |
| 優先度 | P2 |
| 規模 | XL（Phase 1〜3。人間が求めない限り Phase 1 だけを実装する） |
| 依存 | [W19](W19-thread-bound-handles.md), [W21](W21-c-layout-records.md), [W24](W24-extern-view.md)（Phase ごとに使うほかのチケットの型は「着手条件」と「前提とする他チケットのインターフェース」に書く） |
| 後続 | [W26](W26-com-objects-in-tsuzuri.md), [W27](W27-win32-package-from-winmd.md), [W30](W30-windows-hardening.md) |
| 状態 | todo |
| 起票 | 2026-11-22（D-63。利用者の依頼「X: D13 を狭く改める方針で決定。実現向けたチケットを @_features に W プリフィックスのチケットを作成して」と「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」から起票。HEAD `fa40eede` で確認） |
| 承認 | 承認済み（D-63）。細目は要承認（決定事項の D1・D3・D4・D7・D9・D10） |
| 改善する劣位 | C/C++ 比: COM の API（ファイルダイアログ、Direct2D、DirectWrite、タスクバー、UI Automation のクライアント）を C の shim なしに呼べない（E12 の「対象外」の C++ ABI、[E12](_completed/E12-ffi-extensions.md) D13） |
| 手本にする既存実装 | 構文: `src/parser.rs` の `Parser::extern_declaration`（`extern type` の読み取り）と `instance` の本体（`{` と改行区切りの要素）。名前: `src/check.rs` の `Checker::case_path`（`Union.Case` と `Module.Union.Case`）。型: `Type::Handle` の性質（`is_copy`・`needs_drop`・`can_capture`・`sync_here`）。呼び出し: `src/llvm_imports.rs` の `FunctionEmitter::host_call`・`host_result_slot`。読み込んだポインターの型付きの間接呼び出し: `src/llvm_abi.rs` の `native_allocator`（`@tsuzuri_trap_hooks` から読んだ `%allocate` を呼ぶ）。型の制限の診断: `Atomic.create` の `atomic_element_type`（`E1005`）。リンク: `src/driver.rs` の `-lws2_32`（Net を使う Windows のリンク）と `PlatformUnsupportedApi` の判定。印: E15 の `@trusted` |
| 主な影響ファイル | `src/syntax.rs`・`src/parser.rs`・`src/syntax_codec.rs`・`src/frontend_cache.rs`（`FRONTEND_FORMAT`）・`src/formatter.rs`・`src/docgen.rs`・`src/api_diff.rs`・`src/semantic.rs`・`src/kg_capture.rs`・`src/find.rs`・`src/repl.rs`・`src/doctest.rs`・`src/guide_std.rs`（`grep -rln extern_types src` の全件）、`src/stdlib.rs`（`RESERVED_MODULES`）、`src/check.rs`・`src/abi.rs`・`src/llvm.rs`・`src/llvm_imports.rs`・`src/llvm_com.rs`（新規）・`src/effects.rs`・`src/pure_rules.rs`・`src/kg/effects.rs`・`src/capabilities.rs`・`src/driver.rs`・`src/diagnostic_kinds.rs`、`tests/com_interfaces.rs`（新規）・`tests/com_interfaces.mjs`（新規）・`tests/com_interfaces_host.c`（新規）・`tests/fixtures/com_interfaces/Main.tz`（新規）・`tests/windows.mjs`（Phase 2）、文書 |

## 実装者向け早見

- 範囲: COM のインターフェースを `extern` の宣言として書き、メソッドを vtable のスロットの間接呼び出しに下げる。値は参照を 1 つ所有し、drop で `Release`、複製は `Com.clone`（`AddRef`）、型の変換は `Com.cast`（`QueryInterface`）、生成は `Com.create`（`CoCreateInstance`）。`[out] I**` は `Result<I, i32>`、HRESULT は `i32`。Tsuzuri で COM オブジェクトを**実装する**側は [W26](W26-com-objects-in-tsuzuri.md)。
- 最初に読む: ① 仕様 1〜5 ② `Parser::extern_declaration` ③ `host_call`（引数の下げ方を共有する）④ `with_blocking_externs`（`src/llvm.rs`。間接呼び出しを包まない）⑤ [W19](W19-thread-bound-handles.md) の `local` と効果 `pinned`。
- 最初に書くテスト: `tests/com_interfaces.rs` の `parses_and_types_an_interface`（宣言・スロットの番号・`I.m` の型）→ `tests/com_interfaces_host.c` の偽の COM オブジェクトと `tests/com_interfaces.mjs` の `counters`。
- 触ってはいけない: `extern def`・`extern type` の意味・ABI・header。interface を使わないプログラムの IR・object・header・診断（1 byte も変えない）。WASM の既存の出力。
- 完了の見分け方（Phase 1）: 次のプログラムを C の偽のオブジェクト（`tzcom_counter_create`）とリンクして実行すると 42 を返し、終わった時点で C の側の生きているオブジェクトの数が 0 になる。

```tsuzuri
@trusted
extern interface ICounter : Com.IUnknown "6f1d8a2e-4c1b-4f7e-9d38-2b5a7c90e011" {
    add :: i32 -> i32
    count :: unit -> i64
    twin :: unit -> Result<ICounter, i32>
}

@trusted
extern "tzcom_counter_create" def counter_create :: unit -> Result<ICounter, i32>

def main :: unit -> i32 = \() ->
    match counter_create () with
    | Error hr -> hr
    | Ok counter ->
        let other = Com.clone (ref counter)
        ICounter.add (ref counter) 40 |> ignore
        ICounter.add (ref other) 2 |> ignore
        ICounter.count (ref counter) as i32
```

## 目的

D-63 で利用者は E12 D13 を狭く改め、`extern` として宣言する型付きの形だけを足すと決めた。その 2 つ目が COM の呼び出しである。今は vtable を通る呼び出しを書けないので、`IFileOpenDialog`・Direct2D・DirectWrite・`ITaskbarList3` のような COM の API は C の shim を書くか、`extern view`（W24）でアドレスを読んで偽の `extern` を重ねるしかない。後者は「宣言が正しいこと」以外に安全の根拠がなく、しかも監査で見分けられない。

このチケットは、COM のインターフェースを宣言の形で書けるようにし、スロットの位置・`this`・参照カウント・`[out]` のインターフェース・HRESULT をコンパイラが下げる。安全性の根拠は今の `extern` と同じ「宣言が正しいこと」に閉じ、`@trusted`（E15）と権限 `ffi`（D-57）の監査の対象にする。生ポインター型・ポインター演算・`unsafe` は入れない（D-63 の範囲）。

## 着手条件と停止条件

### 着手条件

- Phase 1: [W19](W19-thread-bound-handles.md) が done（`local` の性質と効果 `pinned`）。[W21](W21-c-layout-records.md) の Phase 1 と [W24](W24-extern-view.md) の Phase 1 が done（`extern` の後の文脈キーワードの読み取りと、strategy.md・`_features/README.md` の「採用しない改善」の改訂が済んでいる）。確認: `grep -nE "^\| W(19\|21\|24) " _features/README.md`。
- Phase 2: [W13](W13-extern-ref-mut-and-mutable-buffers.md)（`ref mut T`）・[W15](W15-abi-nullable-handles.md)（`Maybe<H>`・`Maybe<ref ZString>`）・[W18](W18-copy-extern-type.md)（`extern type copy`）・[W20](W20-zstring.md)（`ZString`）が done。Phase 3: [W14](W14-extern-multiple-declarations.md)（同じ symbol の複数の宣言）と [W36](W36-c-layout-by-value.md) の Phase 1（`@c_layout` record の値渡し。Windows x64・Arm64）が done。どれも依存の欄（README の表）に無い Phase ごとの前提で、W14 は W19・W21・W24 のどの依存にも無い（W36 は W21 の後続）。
- 決定事項の D3・D4・D7 が承認済み。Phase 2 は D9、Phase 3 は D10 も。
- ベースライン: `tests/fixtures/*` と `examples/*` の `--emit llvm`（native・`--target wasm32`）、`--emit header`、`check --json` を保存する（受け入れ条件の比較に使う）。

### 停止条件

- **S1** W18 の `HandleKind` に種類（`Interface`）を足すだけでは interface を表せず、`Type` に新しい variant を足したくなった（E12 の停止条件と同じく `Type` の大きさを増やさない）。
- **S2** `host_call` の引数の下げ方を、直接の呼び出しとスロットの呼び出しで共有できず、複製したくなった。
- **S3** interface を使わないプログラムの IR・header・診断が 1 byte でも変わる。
- **S4** `with_blocking_externs` を変えずにスロットの呼び出しを包めない、または包むと既存の tzroutine の suite（`node tests/tzroutine.mjs`）が変わる。
- **S5** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

- **構文**: `src/parser.rs` の `Parser::extern_declaration` は、リンク名の文字列（`link_name`）の後に `type` か `def` だけを受ける。`extern interface` は `E0002`（`expected 'def' after extern`）。`interface`・`object`・`view` は予約語でも文脈キーワードでもない（`src/lexer.rs`・`src/parser.rs` に綴りがない）。`ExternDeclaration` は `Type` と `Function` の 2 つ（`src/parser.rs` 41 行）、`Program` は `externs` と `extern_types` を持つ（`src/syntax.rs` 294 行）。
  再現: `target/scratch/w-com/iface/Main.tz` に下の宣言を置き、`target/debug/tsuzuri.exe check target/scratch/w-com/iface` → `3:8: error[E0002]: expected 'def' after extern`。

  ```tsuzuri
  extern type Hwnd
  @trusted
  extern interface IModalWindow : Com.IUnknown "b4db1657-70d7-485e-8e3e-6fcb5a5c1802" {
      show :: Maybe<Hwnd> -> i32
  }
  def main :: unit -> i32 = \() -> 0
  ```

- **外部の呼び出し**: `FunctionEmitter::host_call`（`src/llvm_imports.rs`）は常に `declare <型> @<symbol>(...)` を `intrinsics` に入れ、`call <型> @<symbol>(...)` を出す。呼び先は常に symbol で、値として読んだ関数を呼ぶ経路はない。生成 IR で読んだポインターを型付きで呼ぶのは、`src/llvm_abi.rs` の `native_allocator` が `@tsuzuri_trap_hooks` から読んだ `%allocate`・`%release` を呼ぶ所だけである。
- **record の受け渡し**: record の結果は先頭の out ポインター（`abi::out_result`、`host_result_slot`）で受け、record の引数は正規化した一時構造体へのポインターで渡す（docs/language.md の「ホスト関数のインポート」）。C の値渡し・値返しの struct の ABI はない。
- **ハンドル**: `extern type` は `Type::Handle(Box<str>)`（`src/check.rs` 176 行）。`is_copy` は偽、`needs_drop` は `_` の腕で偽（drop glue なし。E12 D3）、`sync_here` は偽、捕捉できない。`abi::abi_scalar` はハンドルと `ref` ハンドルを 1 つのポインターとして渡す。コールバックの型（`abi::callback`）の引数は `abi_scalar` だけ、結果はスカラーか `unit` だけ。
- **ブロッキングの包み**: `with_blocking_externs`（`src/llvm.rs`）は、spawn するネイティブのプログラムで、`direct_callee` が `extern_symbols(module)` の symbol を呼ぶ行だけを `tsuzuri_tz_blocking_enter`・`_exit` で包む。symbol の集合が空なら IR をそのまま返す。間接呼び出しは包まれない（モーダルな `Show` が P を持ったまま待つことになる）。
- **型で修飾した名前**: `Checker::case_path`（`src/check.rs`）は `Case`・`Module.Case`・自分のモジュールの `Union.Case`・`Module.Union.Case` を解決する。再現: `Main.tz` に `union Shape` を置くと `Shape.Circle 1.0` は通り、`Shapes.tz` に置くと別のモジュールから `Shapes.Shape.Circle 1.0` は通るが `Shape.Circle 1.0` は `E1002`（`unknown value 'Shape'`）。
- **型の明示の適用**: 呼び出しの地点で型引数を書く構文はない。決まらない多相の使用は `E1015`（`ambiguous_polymorphic_use`。再現: `let v = Vec.empty`）。
- **予約名**: `RESERVED_MODULES`（`src/stdlib.rs` 78 行）は 73 個で `Com` を含まない（`assert_eq!(RESERVED_MODULES.len(), 73)`）。利用者の `Com.tz` は今は通る（再現: `Com.tz` に `def x :: unit -> i64 = \() -> 1`、`Main.tz` から `Com.x ()`。`check` が 0）。
- **リンク**: 同梱の MinGW（`T=/c/Users/<user>/.vscode/extensions/tmidorikawa.tsuzuri-0.1.6-win32-x64/toolchain`、`TSUZURI_CLANG=$T/bin/tsuzuri-clang.exe target/debug/tsuzuri.exe build . -o a.exe`）では、`extern "CoInitializeEx"`・`"D2D1CreateFactory"`・`"DWriteCreateFactory"` を呼ぶプログラムが `lld-link: error: undefined symbol: CoInitializeEx` などで `E2002` になる。`-l ole32 -l d2d1 -l dwrite` を足すとリンクでき、`CoInitializeEx(NULL, COINIT_APARTMENTTHREADED)` は 0 を返した。Net の Windows のリンクは driver が `-lws2_32` を足す（`src/driver.rs` のリンク引数の組み立て。ランタイムの `#pragma comment(lib, ...)` を守らないリンカーがあるため）。
- **import library**: 同梱の zig の `lib/libc/mingw/lib-common/` に `d2d1.def`・`dwrite.def`・`uiautomationcore.def`・`msctf.def`・`ole32.def.in`・`oleaut32.def.in`・`shell32.def`・`comdlg32.def`・`propsys.def`・`windowscodecs.def`、Windows SDK 10.0.26100 の `Lib/10.0.26100.0/um/x64/` に `d2d1.lib`・`dwrite.lib`・`Ole32.Lib`・`OleAut32.Lib`・`UIAutomationCore.lib`・`Uuid.Lib` がある（`msctf.lib` はない。TSF は `CoCreateInstance` で得る）。
- **GUID**: `record Guid { data1: i32u, data2: i32u, data3: i32u, data4: i32u }` の `--emit header` は 4 つの `uint32_t` の 16 バイトの struct で、C の `GUID` と同じ大きさと整列である。
- **SDK のシグネチャ**（`Include/10.0.26100.0/um/shobjidl_core.h`）: `IModalWindow : IUnknown`（IID `b4db1657-70d7-485e-8e3e-6fcb5a5c1802`）の `Show(HWND hwndOwner)` がスロット 3。`IFileDialog : IModalWindow`（`42f85136-db7e-439c-85f1-e4075d135fc8`）の `GetResult(IShellItem **ppsi)` がスロット 20、`SetTitle(LPCWSTR)` が 17。`IFileOpenDialog`（`d57c7288-d4ad-4768-be02-9d969532d960`）の vtable は 29 項目。`IShellItem`（`43826d1e-e718-42ee-bc55-a1e261c37bfe`）の `GetDisplayName(SIGDN, LPWSTR *ppszName)` はスロット 5 で `_Outptr_result_nullonfailure_`。`CLSID_FileOpenDialog` は `DC1C5A9C-E88A-4dde-A5A1-60F82A20AEF7`。
- **対象と呼び出し規約**: 32-bit の target はない（`src/target.rs` の `unknown_target_message` が `i686` などを拒む）。Windows は x86-64 と AArch64（MSVC・GNU）だけ。SDK の C の形の vtable（`shobjidl_core.h` の `IModalWindowVtbl`）は `HRESULT (STDMETHODCALLTYPE *Show)(IModalWindow * This, HWND hwndOwner)` で、`this` が先頭の引数。`STDMETHODCALLTYPE` は `__stdcall`（`shared/basetyps.h`）で、64-bit の x86-64・AArch64 では C の規約と同じ扱いになる（コンパイラの一般の規則。このリポジトリでは未確認で、手順 4 の IR を clang の下げ方と比べて確かめる）。
- **方針の文**: `_features/README.md` の「採用しない改善」の「`unsafe` ブロック・生ポインター型」の行は D-63 への注記を持ち、文言は W24 の出荷の PR で改める。

## 変えてはいけない意味

- `extern def`・`extern type`・コールバック・`export def` の型・ABI・header・診断。
- interface を使わないプログラムの IR・object・header・`.wasm`・診断（1 byte も変えない）。
- 評価順序（左から右）。`this` の式は最初の引数として先に評価する。
- トラップの意味。COM の失敗（負の HRESULT）はトラップにせず値で返す。

## 仕様

### Phase の範囲

| Phase | 内容 | 規模 | 前提 |
| --- | --- | --- | --- |
| 1 | 宣言・スロットの呼び出し・`Release` の drop・`Com.clone`・`Com.cast`・`Com.create`・`Result<I, i32>`・HRESULT・ブロッキングの包み・監査。C の偽のオブジェクトで全ネイティブ target を検査 | L | W19・W21 Phase 1・W24 Phase 1 |
| 2 | Windows の `IFileOpenDialog` の例（`CoInitializeEx` の STA、`local`）。`CoTaskMemAlloc` の out 文字列 `Result<ZString, i32>`、`Com.Iid<I>` | M | W13・W15・W18・W20 |
| 3 | Direct2D・DirectWrite。メソッドの値渡しの `@c_layout` record の引数（W36 の分類をそのまま使う）、MSVC のメンバー関数の record の結果、interface の本体の `@count` | L | W21 Phase 1・W14・W36 Phase 1 |

### 1. 宣言（Phase 1）

```text
interface  = [ doc ] [ "@trusted" ] [ "private" ] "extern" "interface" Name ":" Base Iid "{" { member } "}"
member     = [ doc ] name "::" Type                 改行か ';' で区切る（instance の本体と同じ）
Base       = "Com.IUnknown" | 型の名前（別の extern interface。修飾してよい）
Iid        = "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"  16 進 32 桁（大文字小文字は問わない。波かっこは書かない）
```

- `interface` は `extern` の直後だけの文脈キーワード（予約語にしない）。
- 基底は必須で、連鎖の根は組み込みの `Com.IUnknown`（スロット 0〜2 の `QueryInterface`・`AddRef`・`Release`）。根を暗黙にしない（GUIDE §1 規則 2）。
- スロットの番号: 基底の連鎖のメソッドを根から順に並べ、その後に自分のメソッドを宣言順に並べる。`Com.IUnknown` の 3 つの後なので、最初の利用者のメソッドは 3。
- 誤り: `interface` の前のリンク名の文字列（`E0002`、kind 案 `extern_interface_with_link_name`）、型引数・`=`・基底の欠落（`E0002`、`extern_interface_syntax`）、IID の形（`E0002`、`invalid_interface_iid`）、基底が interface でない（`E1003`、`interface_base_not_interface`）、基底の循環（`E1008`、`com_interface_base_cycle`）、連鎖の中で同じメソッド名（`E1001`、`duplicate_interface_method`）、連鎖の深さ 32・メソッド 1,024 を超える（`E1017`、`interface_limit`）、`@trusted` の性質（`E1037`、`trusted_interface_property`。`extern type` と同じく性質を取らない）、同じ名前の型（`E1001`）。

### 2. 型（Phase 1）

- `I` は、COM の参照を 1 つ所有する不透明な値。Copy でも Clone でもない（複製は `Com.clone`）。drop で `Release`（スロット 2）を呼ぶ。値の移動で参照も移る。record・union・`Maybe`・`Vec` に入れてよく、それらは drop glue を持つ。
- W19 の `local` の性質を持つ: Send でない、Sync でない、関数値に捕捉できない、使えるのは固定された文脈だけ（W19 の効果 `pinned`）。COM の STA を想定した既定で、Send にできる interface（agile）は対象外。
- `Eq`・`Display` のインスタンスはない。`Com.IUnknown` も同じ性質の型として書ける。

### 3. メソッドの呼び出し（Phase 1）

- メソッド `m :: P1 -> … -> Pn -> R` は、関数 `I.m :: ref I -> P1 -> … -> Pn -> R` になる（`m :: unit -> R` は `ref I -> R`）。`this` は暗黙の先頭の引数で、C には `I *` として渡る。
- 名前の解決は union の case と同じ（`Checker::case_path`）: 宣言したモジュールの中では `I.m`、別のモジュールからは `Module.I.m`（`Module` は名前空間の付いた `Ns::Module` でもよい）。`I.tz` に宣言した interface は、モジュールと同じ名前の型の規則で、どこからでも `I.m`。
- 基底のメソッドは派生の名前で呼べる（`IFileOpenDialog.show (ref dialog) None`）。基底の型への暗黙の変換はしない（`IModalWindow.show` に `ref IFileOpenDialog` は渡せない。`Com.cast` を使う）。
- 呼び出しはすべての引数を与えた直接の呼び出しだけ。関数値・部分適用は `E1008`（`com_method_not_called`）。コールバックを取る `extern` と同じ制限で、スロットの呼び出しのための関数本体を生成しないため。
- 引数の型（Phase 1）: `extern def` の引数と同じ集合（スカラー、`unit`、ハンドルとその `ref`、スカラーの record とその `ref`、`ref [T]`・`ref string` の buffer、コールバック）と、`ref J`・`Maybe<ref J>`（`J` は interface。NULL は `None`）。W13・W15・W18・W20・W21・W22 が `extern def` に足す型は、出荷されていればそのまま使え、下げ方も同じ（`host_call` を共有する）。所有する `J` の引数は `E1008`（`com_owned_interface_argument`。COM の `[in]` は所有権を移さない）。
- 結果の型（Phase 1）: スカラー（HRESULT は `i32`）、`unit`（`void`）、`Result<J, i32>`、`Result<Maybe<J>, i32>`。record・buffer の結果は `E1008`（`com_record_result`。MSVC のメンバー関数は record を `this` の後の隠れたポインターで返し、C の規則と違う。Phase 3）。所有する `J` を直接返す形は `E1008`（`com_interface_result`。参照を移すかが宣言から分からない）。

### 4. `[out] J**` と `Result`（Phase 1）

結果を `Result<J, i32>` か `Result<Maybe<J>, i32>` と宣言したメソッドと `extern def` は、C では HRESULT（`int32_t`）を返し、最後の引数に `J **`（呼び出し側が NULL で初期化した一時領域）を取る。

| C の結果 `hr` | `*out` | `Result<J, i32>` | `Result<Maybe<J>, i32>` |
| --- | --- | --- | --- |
| `hr < 0` | どれでも | `Error hr`（`*out` は読まず、`Release` もしない。COM は失敗のとき NULL を書く決まりで、NULL でない値は信用できない） | 同じ |
| `hr >= 0` | NULL でない | `Ok p`（参照を 1 つ所有する。`AddRef` しない） | `Ok (Some p)` |
| `hr >= 0` | NULL | `Error -2147467261`（`E_POINTER` = `0x80004003`。成功の値で NULL を返すことを型が許していないため。黙って既定値にしない） | `Ok None` |

- 成功の値の区別（`S_OK` と `S_FALSE`）は `Ok` では失われる。区別が要る API は今の `i32` の結果とスカラーの out を使う（Phase 1 の対象外）。

### 5. 組み込みのモジュール `Com`（Phase 1）

| 関数 | 型 | 意味 |
| --- | --- | --- |
| `Com.clone` | `ref I -> I` | `AddRef`（スロット 1）を呼び、同じポインターの所有値を返す |
| `Com.cast` | `ref I -> Result<J, i32>` | `QueryInterface`（スロット 0）を `J` の IID で呼ぶ。結果の規則は仕様 4。`J` は期待型から決まり、決まらなければ `E1015`（`ambiguous_polymorphic_use`）。`J` が interface でなければ `E1005`（kind 案 `com_target_not_interface`。`atomic_element_type` と同じ形）。`I = J` も許す |
| `Com.create` | `string -> i32u -> Result<I, i32>` | `CoCreateInstance(clsid, NULL, context, IID_I, &out)`。最初の引数はコンパイル時に決まる文字列（リテラルか `string` の `const`）で、IID と同じ形の CLSID。そうでなければ `E1008`（`com_clsid_not_constant`・`com_invalid_clsid`）。`I` は期待型から決まる。Windows だけ |

- `Com` は std のファイルを持たない組み込みのモジュール（`Routine` と同じ）で、`RESERVED_MODULES` に足す（現在の値 + 1）。利用者のモジュール `Com.tz` は `E1011` になる（互換を破る。D-55 の登録と `docs/stability.md` の記録。決定事項 D4）。
- 引数の順序（D-59）: 3 つとも対象は最後（`clone`・`cast` は唯一の引数、`create` は対象のない構築）なので例外は要らない。メソッド `I.m` は利用者の宣言で std の規則の外だが、並びは D-59 の例外 (1) 資源のハンドルと同じ（ハンドルが先頭）。
- `Com.create` はコンパイラが `CoCreateInstance` を宣言する。到達した Windows のプログラムには driver が `-lole32` を足す（`-lws2_32` と同じ所）。Windows でない native の target で到達したら `E2002`（既存の `platform_unsupported_api`。driver の OS API の判定と同じ所）。`CoInitializeEx` は呼ばない（暗黙の初期化をしない。利用者が `extern` で呼ぶ。呼ばなければ `CO_E_NOTINITIALIZED` の `Error`）。

### 6. ABI の位置・WASM・ブロッキング（Phase 1）

- `ref J`・`Maybe<ref J>` を置けるのは、`extern def` とメソッドの引数。`Result<J, i32>` 系は `extern def` とメソッドの結果。`export def` の引数・結果と `@c_layout` の field に interface を置くと `E1008`（`com_interface_in_export`・`com_interface_field`）。C の header には何も出さない。コールバック（`FnPtr`・`extern` の関数型の引数）の引数としての `ref J` は [W26](W26-com-objects-in-tsuzuri.md) が足す。
- WASM: 型検査は target を知らない（`src/check.rs` は target を引数に取らない）ので、`llvm::emit_target` の `wasm = true` で、到達したメソッドの呼び出し・`Com.*`・interface を ABI に持つ `extern` の呼び出しを `E1008`（W13・W15 などと共有する kind `abi_native_only`。最初に出荷したチケットが登録簿へ足す）にする。宣言だけなら受理する（同じプロジェクトを wasm でも検査できる）。WASM の出力は変えない。
- spawn するネイティブのプログラムでは、スロットの呼び出しも `tsuzuri_tz_blocking_enter`・`_exit` で包む（`extern def` の呼び出しと同じ。モーダルな `Show` の間に P を手放す）。

### 7. 効果・権限・監査（Phase 1）

- メソッドの呼び出し・`Com.clone`・`Com.cast`・`Com.create`・interface の drop は効果 `extern`（D-53 の語彙。drop も外部のコードを呼ぶので `user_drop` ではなく `extern`）と権限 `ffi`（D-57）を持つ。
- `@trusted` のない `extern interface` のメソッドか `Com.*` を、`extern` でない関数が使うと、宣言の名前に `W1010`（`untrusted_extern`）。`tsuzuri audit` の `extern` の行は interface ごとに 1 行で、`name` は interface の名前、`symbol` は `{<IID>}`、`used` はメソッドと `Com.*` の使用の数（`tz-audit/1` のキーは変えない）。

### 8. Phase 2: Windows の `IFileOpenDialog`

- 例（`examples/` には置かず、言語リファレンスの例と `tests/windows.mjs` の段）: `CoInitializeEx(NULL, COINIT_APARTMENTTHREADED)` → `Com.create "DC1C5A9C-E88A-4dde-A5A1-60F82A20AEF7" 1`（`CLSCTX_INPROC_SERVER`）で `IFileOpenDialog` → `IFileOpenDialog.set_title (ref title)`（`ref ZString`。W20）→ `show (ref dialog) None`（`Maybe<Hwnd>`。W15・W18）→ `get_result` → `IShellItem.get_display_name (ref item) SIGDN_FILESYSPATH` → 表示。interface の値は `CoUninitialize` の前に drop されるよう、COM を使う処理を別の関数に分ける（落とし穴）。
- **out 文字列** `Result<ZString, i32>`（決定事項 D9）: メソッドと HRESULT を返す `extern def` の結果に書ける。C では最後の引数が `LPWSTR *`。成功で NULL でなければ、NUL までの UTF-16 を新しい `ZString` に複写してから `CoTaskMemFree` を呼ぶ。2^31 単位を超えても NUL がなければトラップ（既存の `TrapKind::AllocationSize`）。失敗・NULL の扱いは仕様 4 と同じ。「COM の out 文字列は `CoTaskMemAlloc` で確保する」という規則は API の文書の記述で、ヘッダーには `_Outptr_result_nullonfailure_` しかない（未確認）。Phase 2 の着手時に、`IShellItem::GetDisplayName` と `SHGetKnownFolderPath` の文書で確かめて記録し、規則が API ごとに違うなら、この形を `CoTaskMemFree` で解放する API に限ると文書に書く。
- `Com.create` と out 文字列は `ole32` の `CoCreateInstance`・`CoTaskMemFree` を宣言する。driver はコンパイラが宣言した `ole32` の symbol のどれかがあれば `-lole32` を足し、Windows でない native では `E2002` にする（設計の driver）。
- **`Com.Iid<I>`**（決定事項 D9）: `REFIID riid` と `void **ppv` を取る API（`SHCreateItemFromParsingName`・`IShellItem::BindToHandler`）のため、引数の型 `Com.Iid<I>` を足す。値は組み込みの `Com.iid`（型は期待型から決まる）だけで、C には `I` の IID の定数へのポインターとして渡る。宣言の結果が `Result<I, i32>` 系で、`Com.Iid<I>` の `I` と同じでなければ `E1008`（`com_iid_mismatch`）。呼び出し側は IID を選べないので、IID と結果の型の食い違いは起きない。
- テスト（Windows だけ）: ダイアログを表示せずに作って `set_title`・`get_options`（`ref mut i32u`。W13）・`set_options`、表示の前の `get_result` が `Error`、`Com.cast` で `IFileDialog`・`IModalWindow`・`Com.IUnknown` に変換でき、関係のない IID（`IShellItem`）は `Error -2147467262`（`E_NOINTERFACE`）。`SHCreateItemFromParsingName` で一時ディレクトリの `IShellItem` を作り、`get_display_name` の文字列がそのパスと一致する。W06 が done なら、別のスレッドがダイアログの窓へ `WM_CLOSE` を送り、`show` が `Error -2147023673`（`HRESULT_FROM_WIN32(ERROR_CANCELLED)`）を返すことも確かめる（`timeout 60`）。

### 9. Phase 3: Direct2D・DirectWrite

- **値渡しの `@c_layout` record の引数**: `ID2D1RenderTarget::DrawLine(D2D1_POINT_2F, D2D1_POINT_2F, …)` のため、メソッドの引数に `@c_layout` record の値（`ref` なし）を書けるようにする。分類と下げ方、対応する target、Phase の外の target の `E1008`（kind `abi_record_by_value_target`）と WASM の `E1008`（`abi_native_only`）は、すべて [W36](W36-c-layout-by-value.md) の `extern def` の規則（W36 の仕様 1・2・4）と同じである。スロットの呼び出しは `host_call` の引数の下げ方を共有する（停止条件 S2）ので、W36 の下げ方がそのまま効き、W25 は分類を持たない（W36 の「他チケットへ提供するインターフェース」）。`D2D1_POINT_2F` は x86-64 Windows では `i64`、AArch64 Windows では `[2 x float]` になる（W36 の参照表）。`extern def` の引数と結果の値渡しと、E05・[W21](W21-c-layout-records.md) D4 の改訂は W36 の範囲（W36 の D1）。
- **record の結果**: メソッドの `@c_layout` record の結果は、MSVC の C++ のメンバー関数の規則（大きさによらず、`this` の次の隠れたポインターで返し、そのポインターを返す）に下げる（`ID2D1RenderTarget::GetSize`）。W36 の値返しの分類は使わない（W36 の対象外）。`extern def` の record の結果は、印のない record は今の out ポインターの規則のまま、`@c_layout` record は W36 の C の値返し（W36 の仕様 2）。
- **interface の本体の `@count`**（W13）: `IDWriteFactory::CreateTextLayout(WCHAR const *, UINT32, …)` の長さの型を選ぶため、メソッドの前に `@count` を書けるようにする（D-53 の属性の置き場の追加。決定事項 D10）。
- 工場: `D2D1CreateFactory`・`DWriteCreateFactory` は `Com.Iid<I>` で宣言する（同じ symbol を別の `I` で宣言するのは W14）。
- テスト（Windows だけ）: 窓を作らずに、`CLSID_WICImagingFactory` → `IWICBitmap` → `ID2D1Factory::CreateWicBitmapRenderTarget` → `BeginDraw`・`Clear`・`DrawLine`・`EndDraw`（HRESULT 0）、`GetSize` の値、DirectWrite の `CreateTextFormat`・`CreateTextLayout`・`GetMetrics` の幅が正。

## 設計

### 構文と AST（Phase 1）

- `src/syntax.rs`: `ExternInterfaceDecl { doc, visibility, name, base: TypeExpr, iid: ([u8; 16], Span), methods: Vec<InterfaceMethod> }`、`InterfaceMethod { doc, name: Ident, ty: TypeExpr }`、`Program.extern_interfaces`（`extern_types` の次）。IID は構文の段で 16 バイトに直す（`Data1` を u32、`Data2`・`Data3` を u16 のリトルエンディアン、`Data4` の 8 バイトをそのまま。例 `42f85136-db7e-439c-85f1-e4075d135fc8` → `36 51 F8 42 7E DB 9C 43 85 F1 E4 07 5D 13 5F C8`）。Tsuzuri の native target はすべてリトルエンディアン。
- `src/parser.rs`: `ExternDeclaration::Interface`。`extern_declaration` は `link_name` の後で、`Ident("interface")` を文脈キーワードとして見る（W24 の `view` と同じ分岐）。本体は `instance` と同じく `{` と改行区切り。`@trusted` の読み込み口（`program_all`）は変えず、`extern interface` を印の置き場に足す。
- `src/syntax_codec.rs`・`src/frontend_cache.rs`: `extern_interfaces` の符号化、`GOLDEN`、`FRONTEND_FORMAT` を現在の値 + 1。`src/formatter.rs`・`src/docgen.rs`・`src/api_diff.rs`・`src/semantic.rs`・`src/kg_capture.rs`・`src/find.rs`・`src/repl.rs`・`src/doctest.rs`・`src/guide_std.rs` は `grep -rn extern_types src` の各所に interface を足す（GUIDE §6.9）。`api-diff` はメソッドの削除・並べ替え・型の変更・IID・基底の変更を互換のない変更、末尾へのメソッドの追加を互換として報告する。

### 型検査（Phase 1）

- `src/check.rs`: `Names` に `interfaces: BTreeMap<String, InterfaceInfo>`。`InterfaceInfo { name, base: Option<String>, iid: [u8; 16], slots: Vec<Slot { name, ty: Type, index: u32, declared_in: String }>, trusted: bool, span }`（`slots` は根からの平らな並び。基底の解決の後で一度だけ作る）。
- 型は W18 の `Type::Handle(<修飾名>, HandleKind)` に種類 `HandleKind::Interface` を足して表す（`Type` の大きさは変わらない。S1）。`needs_drop` は `Interface` で真、`is_copy`・`can_capture`・`can_send`・`sync_here` は W19 の `Local` と同じく偽、W19 の `Type::holds_thread_bound` は `Interface` も真にする（`pinned` の源も `local` と同じ規則）。`Type::Handle` を `match` する所（HEAD の `src/check.rs` の 176・548・657・1057・1069・1158・1200・1257・1287 行付近と `src/llvm*.rs`。W18 の後の位置で数え直す）をすべて確かめる。
- `Com.*` の組み込みは W19 の `pinned_rules::builtin_position`（wildcard のない `match`）にも分類を足す（関数の引数を取らないので位置は持たない）。
- 型付き IR: `TypedExprKind::InterfaceCall(Box<InterfaceCall { interface: String, slot: u32, signature: Type }>, Vec<TypedExpr>)`（引数の先頭が `this`）。`Builtin::ComClone`・`ComCast`・`ComCreate`（Phase 2 で `ComIid`）。新しい組み込みは `Builtin::ALL`・`name`・`signature`・`emit_builtin`・効果の表（`src/effects.rs`・`src/pure_rules.rs` の `builtin_effect`・`builtin_impure`、`src/kg/effects.rs` の `builtin_effect`）・権限の表（`src/capabilities.rs`）をすべて更新する。
- ABI の検査（`src/abi.rs`）: `parameter`・`result` に interface の場合を足す関数 `interface_parameter`・`interface_result` を足し、`extern def` とメソッドの宣言の検査から呼ぶ。`abi_scalar` は変えない（コールバックへの広げは W26）。

### 生成（Phase 1、`src/llvm_com.rs`）

- スロットの呼び出し: `this` のポインター `%p` から `%vt = load ptr, ptr %p`、`%sp = getelementptr inbounds ptr, ptr %vt, i64 <slot>`、`%tz.com.slot.<n> = load ptr, ptr %sp`、`call <結果> %tz.com.slot.<n>(ptr %p, <引数>)`。`invariant.load` などのメタデータは付けない。
- `host_call` を 2 つに分ける: 引数と結果の下げ方（一時領域、Latin-1 の複写と解放、`out_result`）を持つ共通の部分と、呼び先（`HostCallee::Symbol(&str)` は今の `declare` と直接の `call`、`HostCallee::Slot { this, index }` は上の間接呼び出し）。既存の extern の IR は変わらない（S2・S3）。
- `Result<J, i32>`: `alloca ptr` を NULL で初期化して最後の引数に渡し、呼び出しの後で仕様 4 の表のとおりに分岐する。ポインターが interface の値になるこの所で、W24 と同じ整列の検査（8 バイト。違えば W24 と同じトラップ）を 1 回だけ行う。NULL は仕様 4 の表で値にならないので、スロットの呼び出しのたびには検査しない（interface の値は `Result` の `Ok`・`Maybe` の `Some`・`Com.clone`・W26 の `Com.object` からしか生まれず、どれも NULL でない）。
- IID の定数: `@tz.com.iid.<32 桁の 16 進> = private unnamed_addr constant [16 x i8] c"...", align 4`。IID の 16 バイトを鍵にした `BTreeMap` で 1 つにする（決定的。`-luuid` は要らない）。
- drop: `Release` のスロット 2 を `i32 (ptr)` として呼ぶ（結果は使わない）。
- `Com.create`: `declare i32 @CoCreateInstance(ptr, ptr, i32, ptr, ptr)` を `intrinsics` に入れる（宣言の行の文字列の集合なので、利用者の同じ宣言と 1 つになる。W14・W17 の正規化に従う）。CLSID は IID と同じ定数。
- ブロッキング: `with_blocking_externs` は、`direct_callee` の照合に加えて、呼び先が `%tz.com.slot.` で始まる値の `call` も包む。symbol の集合が空でもスロットの呼び出しがあれば走らせる（`extern_symbols` の早い戻りを直す）。

### driver（Phase 1）

- IR にコンパイラが宣言した `ole32` の symbol（Phase 1 は `@CoCreateInstance`、Phase 2 で `@CoTaskMemFree`。`llvm_com` の 1 つの表に並べる）の宣言があれば `com_runtime` とし、Windows の実行ファイルのリンクに `-lole32`（`net_runtime` の `-lws2_32` の隣）。Windows でない native で `Emit::Llvm` 以外なら `E2002`（`platform_unsupported_api`。メッセージ案 `Com.create and COM out-strings need Windows (ole32); declare the platform's own functions with extern instead`）。

## 前提とする他チケットのインターフェース

- [W18](W18-copy-extern-type.md): `syntax::HandleKind`（`Owned`・`Copy`）と `Type::Handle(Box<str>, HandleKind)`、修飾の語を読む `Parser::extern_declaration` の枠。W25 は `HandleKind::Interface` を足す。名前が違えば止めて報告する（GUIDE-LITE §11）。
- [W19](W19-thread-bound-handles.md): `HandleKind::Local` の性質（Send・Sync・捕捉が偽）、`Type::holds_thread_bound`、`effects::Effect::Pinned`、`pinned_rules::builtin_position`、`E1013` の kind `pinned_code_spawned`・`pinned_function_value`。固定された文脈は `main`・`export def`・`test`・`bench` の本体と `with_locked_thread`・コールバック・`FnPtr.of` の本体（W19 の D5）。
- [W24](W24-extern-view.md): `extern` の後の文脈キーワードの分岐（`view`）、「0・整列の検査 → 型付きの load」の規則とトラップ、監査の表示の `HostImport` の種類の分岐、strategy.md・`_features/README.md` の「採用しない改善」の改訂。W25 の PR はその文に `extern interface` の出荷を足す。
- [W21](W21-c-layout-records.md): `@c_layout record`（Phase 3 のメソッドの値渡しの引数と record の結果）。
- [W36](W36-c-layout-by-value.md)（Phase 3）: `llvm_byvalue::{ByValue, classify}` と `FunctionEmitter::{by_value_slot, by_value_to_abi}`、kind `abi_record_by_value_target`。メソッドの値渡しの引数は `host_call` の共有でこの下げ方に乗る。
- [W13](W13-extern-ref-mut-and-mutable-buffers.md) の `ref mut T` と `@count`、[W15](W15-abi-nullable-handles.md) の `Maybe<H>`・`Maybe<ref ZString>`、[W18](W18-copy-extern-type.md) の `extern type copy`、[W20](W20-zstring.md) の `ZString`（Phase 2）。[W14](W14-extern-multiple-declarations.md) の同じ symbol の複数の宣言（Phase 3）。
- [W06](W06-windows-gui-test-harness.md): `tests/windows_gui_shim.c` と `tests/windows.mjs` の `buildGui`（仕様 4）。W06 の shim には別のスレッドから任意の窓へ `WM_CLOSE` を送る関数が無い（`tz_gui_post` は `WM_APP` だけ）ので、Phase 2 の任意の検査に使う関数（案 `tz_gui_close_later(delay_ms)`: 別のスレッドで待ってから前面のダイアログの窓へ `WM_CLOSE` を送る）は W25 が W06 の仕様 4 の形で shim に足す。
- kind `abi_native_only`（W13 などと共有）。

## 他チケットへ提供するインターフェース

- [W26](W26-com-objects-in-tsuzuri.md): `InterfaceInfo`（平らなスロットの並びと IID）、`llvm_com::iid_constant(&[u8; 16]) -> String`、仕様 4 の `Result<J, i32>` の下げ方（W26 は実装する側の逆向きに使う）、`ref J`・`Maybe<ref J>` の ABI の種類、`Com.IUnknown`、組み込みのモジュール `Com`、`%tz.com.slot.` の印。
- [W27](W27-win32-package-from-winmd.md): 宣言の文法と規則（連鎖の中の同じメソッド名は `E1001` なので生成器が名前を変える）、`Com.create` の `const` の CLSID、`Com.Iid<I>`（Phase 2）、メソッドの値渡しの record（W36 の規則）と record の結果（Phase 3）、interface の本体の `@count`（Phase 3）。
- [W30](W30-windows-hardening.md): スロットの呼び出しはふつうの間接呼び出し（`call ptr %fn`）で、CFG の検査を有効にすれば LLVM が計装する。W25 は CFG のための特別な IR を出さない。

## 実装手順

### Phase 1

1. **変更**: なし（ブランチ `feat/w25-extern-interface`）。**内容**: 着手条件の確認と、ベースライン（`tests/fixtures/*`・`examples/*` の IR・header・`check --json`、`target/scratch/w25/before/`）。`node scripts/repo-kb.mjs brief W25`。**確認**: 保存した件数を控える。
2. **変更**: `src/syntax.rs`・`src/parser.rs`・`src/syntax_codec.rs`・`src/frontend_cache.rs`・`src/formatter.rs` ほか `extern_types` の各所。**内容**: 仕様 1 の構文と `E0002` の kind。**確認**: `tests/com_interfaces.rs` の構文の 6 件（受理 1・拒否 5）、`cargo test --locked --test formatter`、`pins_encoding_format`。
3. **変更**: `src/check.rs`・`src/abi.rs`・`src/stdlib.rs`・`src/diagnostic_kinds.rs`。**内容**: `InterfaceInfo`・スロットの番号・名前の解決・型の性質・引数と結果の検査・`Com.*` の型。`RESERVED_MODULES` に `Com`。**確認**: `tests/com_interfaces.rs` の型の検査（下の表）。`cargo nextest run --locked`。
4. **変更**: `src/llvm_imports.rs`・`src/llvm_com.rs`・`src/llvm.rs`。**内容**: `host_call` の分割、スロットの呼び出し、`Result`、drop、IID の定数、`Com.*`。**確認**: ベースラインと `cmp`（S3）。`tests/com_interfaces.rs` の IR の検査（スロットの番号、IID のバイト、IR が 2 回で一致）。
5. **変更**: `src/llvm.rs`（`with_blocking_externs`）。**内容**: 仕様 6 のブロッキング。**確認**: `node tests/tzroutine.mjs target/debug/tsuzuri` が変わらない。`com_interfaces` の tzroutine の case（下）。
6. **変更**: `src/driver.rs`。**内容**: `-lole32` と `E2002`。**確認**: `tests/com_interfaces.rs` の driver の検査（`clang -###` で `ole32` が linker の行に出る。E12 の `link_inputs_reach_the_msvc_linker_as_libpath_and_lib_names` と同じ形）。
7. **変更**: `src/effects.rs`・`src/pure_rules.rs`・`src/kg/effects.rs`・`src/capabilities.rs`。**内容**: 仕様 7。**確認**: `tests/capabilities.rs`・`tests/audit.rs`・`tests/trusted.rs` に interface の case。件数の pin（`Builtin::ALL` などは現在の値 + 足した数）。
8. **変更**: `tests/com_interfaces_host.c`・`tests/com_interfaces.mjs`・`tests/fixtures/com_interfaces/Main.tz`。**内容**: E2E（下）。**確認**: `node tests/com_interfaces.mjs target/debug/tsuzuri`。
9. **変更**: 文書。**内容**: 「ドキュメント」の節。**確認**: T2、`node scripts/check-docs.mjs <変更したページ>`、PR に `ci:full`。

### Phase 2・3

- Phase 2: 仕様 8 を 1 段ずつ（out 文字列 → `Com.Iid` → Windows の段）。Phase 3: 仕様 9（メソッドの値渡しの引数が W36 の下げ方に乗ることの確認（`declare` と呼び出しの型を W36 の参照表と比べる）→ record の結果 → `@count` → Windows の段）。各段の確認は Phase 1 と同じく T1 と追加した検査。

## テスト計画

### Rust（`tests/com_interfaces.rs`、新規）

| 検査 | 期待 |
| --- | --- |
| 宣言の受理と `I.m` の型 | `ICounter.add :: ref ICounter -> i32 -> i32`、`twin` のスロットが 5 |
| 基底の連鎖のスロット | `IFileOpenDialog` を `IModalWindow`（1 つ）・`IFileDialog`（23 個）・自分（2 つ）で宣言すると、`get_results` が 27、`get_selected_items` が 28（SDK の `IFileOpenDialogVtbl` の並びと一致） |
| 拒否 | リンク名付き・IID の形・基底なし（`E0002`）、基底が record（`E1003`）、循環（`E1008`）、同じ名前（`E1001`）、所有する interface の引数・record の結果・interface の直接の結果・関数値・`export def` の interface（`E1008`）、`Com.cast` の注釈なし（`E1015`）・対象が record（`E1005`）、`@trusted pure`（`E1037`）、`Com.tz`（`E1011`） |
| 型の性質 | interface の値を関数値に捕捉すると `E1005`、tzroutine へ渡すと `E1013`、固定されていない文脈で使うと W19 の `E1013`、2 回使うと `E1012` |
| IR | スロットの呼び出しの形、IID の定数のバイト列（仕様の例を手で求めた値）、同じ IID の定数が 1 つ、`emit` を 2 回して同じ、interface を使わない入力の IR が変わらない |
| WASM | 到達した呼び出しが `E1008`（`abi_native_only`）、宣言だけなら成功 |

### E2E（`tests/com_interfaces.mjs` と `tests/com_interfaces_host.c`、全ネイティブ target）

- C の側は 2 つの interface（`ICounter`・`IOther`）を持つ偽のオブジェクトを手で書く（vtable は C の struct、参照カウントは生きているオブジェクトの数 `tzcom_live` と一緒に数える）。IID は C の側に手で書いた `GUID` の定数（独立した参照。GUIDE-LITE §2 規則 10）で、`QueryInterface` は `memcmp` で照合する。
- 実行: `clang -c tests/com_interfaces_host.c` → `tsuzuri run tests/fixtures/com_interfaces --link host.o`（`tests/ffi_extensions.mjs` と同じ形）、`build -O0`・`-O3`、`--allocator counting` で `live == 0`。

| case | 内容 | 期待 |
| --- | --- | --- |
| `counters` | 早見の例 | 42、`tzcom_live` が 0 |
| `twin` | `twin` の `Ok` を使って捨てる | 参照の数が戻る |
| `cast` | `Com.cast` で `IOther`・`Com.IUnknown`、知らない IID | `Ok`・`Ok`・`Error -2147467262` |
| `null_success` | C が `S_OK` と NULL を返す | `Result<J, i32>` は `Error -2147467261`、`Result<Maybe<J>, i32>` は `Ok None` |
| `failure_garbage` | C が負の HRESULT と NULL でない値を返す | `Error hr`、C の側で `Release` が呼ばれない |
| `moves` | 値を record に入れて返し、`Vec` に入れて捨てる | 生きているオブジェクトが 0 |
| `blocking` | spawn するプログラムで、`Routine.with_locked_thread` に渡した関数（P を持つ本物の G）の中でオブジェクトを作り、200 ms 眠る C のメソッドを呼ぶ。その間にほかの tzroutine がチャンネルへ送る（`TSUZURI_MAXPROCS=1`。包みがなければ P を持ったまま眠るので、送りは後になる） | 受け取る順（tzroutine の送りが先）で確かめる（時間の上限は入れない） |
| `wasm` | `--target wasm32` で到達するプログラム | `E1008` |

- Windows（Phase 1 から、`tests/windows.mjs` の段）: `CoInitializeEx` の後の `Com.create "DC1C5A9C-E88A-4dde-A5A1-60F82A20AEF7" 1` が `Ok`、MSVC と同梱の MinGW の両方で `-lole32` なしにリンクできる。存在しない CLSID は `Error -2147221164`（`REGDB_E_CLASSNOTREG`）。`CoInitializeEx` なしは `Error -2147221008`（`CO_E_NOTINITIALIZED`）。Linux・macOS の `Com.create` は `E2002`。

## ドキュメント

- `_tsuzuri/language-reference/compiler/native-interop.md`: 新しい節「COM のインターフェース（`extern interface`）」（宣言、スロット、所有と `Release`、`Com.*`、`Result` の表、STA と `local`、`CoUninitialize` の前に drop する書き方、WASM と Windows 以外）。
- `docs/language.md` の「ホスト関数のインポート」に小節、予約モジュール名の一覧に `Com`。`_tsuzuri/language-reference/organizing-tsuzuri/modules.md` の予約名。
- `_tsuzuri/language-reference/compiler/diagnostics.md` と `docs/language.md` の診断の表（新しい kind）（kind の表は `<!-- generated:diagnostic-kinds -->` の生成の部分なので手で書かず、`src/diagnostic_kinds.rs` に登録してから `node scripts/gen-docs.mjs`）。`compiler/platforms.md`（`Com.create` は Windows だけ）、`compiler/webassembly.md`（`E1008`）。
- `docs/stability.md`: `Com` の予約（`Com.tz` が `E1011` になる互換のない変更）。
- strategy.md の「`unsafe` と生ポインター型」と `_features/README.md` の「採用しない改善」の行（W24 が改めた文）に `extern interface` を足す。E12 D13 の文言も同じ PR（D-63）。
- `docs/architecture.md`: スロットの呼び出し・IID の定数・ブロッキングの包み。

## 受け入れ条件

- [ ] Phase 1 の仕様 1〜7 のとおりに動き、仕様外の入力は表の診断で拒否される。
- [ ] interface を使わないプログラム（`tests/fixtures/*`・`examples/*`）の IR・header・`check --json` がベースラインと 1 byte も違わない。WASM の既存の出力が変わらない。
- [ ] 生成 IR が決定的で、IID の定数と `declare` が重複しない。
- [ ] `tests/com_interfaces.rs` と `node tests/com_interfaces.mjs` が native の `-O0`・`-O3` で通り、`live == 0` と C の側の生きているオブジェクト 0 を確かめた。
- [ ] Windows の段が MSVC と同梱の MinGW の両方で通る。PR に `ci:full` を付けた。
- [ ] `Com` の予約を D-55 の表・`docs/stability.md`・言語リファレンスに記録した。
- [ ] T2 が通る。

## 落とし穴

- **record の結果**: MSVC の C++ のメンバー関数は、小さい record でも `this` の次の隠れたポインターで返す（C の関数の規則と違う）。Phase 1 は `E1008` にし、Phase 3 で専用の規則にする。`extern def` の今の out ポインターの規則とも、W36 の C の値返し（`sret`。W36 の落とし穴）とも混ぜない。
- **`CoUninitialize` の後の `Release`**: 同じスコープで `co_uninitialize ()` を呼ぶと、interface の値の drop がその後になる。COM を使う処理を関数に分け、その関数が戻ってから `CoUninitialize` を呼ぶ形を文書の例にする（型で防ぐ案は対象外）。
- **失敗のときの `*out`**: 読まない・`Release` しない。NULL で初期化した一時領域を渡すのは、NULL を書かない実装への備えでもある。
- **`this` は interface のポインター**: オブジェクトの先頭ではない（多重継承の C++ のオブジェクトでは別の値）。`Com.cast` の結果は QI が返した値をそのまま使う。
- **IID のバイト順**: `Data1`〜`Data3` はリトルエンディアン、`Data4` はそのまま。文字列の順に 16 バイトを並べない。
- **スロットの番号**: 基底の連鎖を根から数える。SDK の `…Vtbl` の struct の並びと突き合わせる。
- **`BOOL` と `bool`**: COM の `BOOL` は `i32`。`bool` の結果は 0 以外を真にするので、`-1` を返す API（`GetMessageW` 型）を区別できない。W27 は `BOOL` を `i32` にする。
- **ブロッキングの包み**: `with_blocking_externs` は symbol の集合が空のとき何もしない。スロットの呼び出しだけのプログラムでも包む。
- **`local`**: interface の値を tzroutine に渡せない。固定した G（`Routine.with_locked_thread`）か `main` で使う。

## 対象外

- Tsuzuri で COM オブジェクトを実装すること（vtable の合成、シンクへの登録。[W26](W26-com-objects-in-tsuzuri.md)）。
- Send にできる interface（agile・MTA）、集約（`pUnkOuter`）、`IDispatch` の遅延束縛、`VARIANT`・`BSTR` の型、`HSTRING`（WinRT）。
- 基底の型への暗黙の変換と、静的な上向きの変換の組み込み（要れば後続）。
- `CoInitializeEx`・`CoUninitialize` を型で囲む組み込み（drop の順を保証する仕組み）。
- `IUnknown` を根に持たない vtable（`ID3DInclude` など）。
- C の header への interface の出力と E13 のバインディング。
- `@c_layout` record の値渡しの分類と、それを使う target・診断（[W36](W36-c-layout-by-value.md)。Phase 3 はその分類をそのまま使う）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | 構文は `extern interface Name : Base "IID" { m :: T }`。基底は必須で、根は組み込みの `Com.IUnknown`。`interface` を `extern` の直後だけの文脈キーワードとして D-55 に登録する | 共通契約（[共通の綴り](README.md#共通の綴り起票時に決めた契約)）の綴りに従う。根を暗黙にしない（GUIDE §1 規則 2）。予約語は増やさないが、文脈キーワードの追加は D-55 の登録で、D-63 の承認は登録までは含まない | 要承認（承認前は手順 2 に着手しない） |
| D2 | 型は W18 の `Type::Handle(_, HandleKind)` に `HandleKind::Interface` を足して表し、`Type` に variant を足さない。スロットと IID は修飾名で引く `InterfaceInfo` に持つ | E12 の停止条件（`Type` の大きさ）を守り、ABI の既存のハンドルの扱いと W19 の `holds_thread_bound` を再利用する | 既定案（実装者はこの案に従う） |
| D3 | メソッドの名前は union の case と同じ規則（`I.m`・`Module.I.m`）で解決する | `Checker::case_path` という既存の型で修飾した名前の規則があり、新しい解決の規則を作らずに 共通契約の `IFileDialog.show` の綴りを満たす（宣言したモジュールの中と `I.tz`）。別のモジュールからは `Module.I.m` | 要承認（承認前は手順 3 に着手しない） |
| D4 | 組み込みのモジュール `Com`（`clone`・`cast`・`create`、Phase 2 の `iid`・`Iid`、型 `IUnknown`）を予約する | D-55 の登録。`Com.tz` を壊す（`docs/stability.md`）。`AddRef`・`Release` を直接呼ばせない（共通契約） | 要承認（承認前は手順 3 に着手しない） |
| D5 | `[out] J**` は `Result<J, i32>`（成功で NULL は `Error E_POINTER`）と `Result<Maybe<J>, i32>`（`Ok None`）、失敗のときは `*out` を読まない | 成功で NULL を黙って既定値にしない（GUIDE §1 規則 2）。COM の決まりに従い、信用できない値を `Release` しない | 既定案（実装者はこの案に従う） |
| D6 | メソッドは直接の完全な呼び出しだけ | コールバックを取る `extern` と同じ。スロットの呼び出しの関数本体と関数値の ABI を作らない | 既定案（実装者はこの案に従う） |
| D7 | `Com.create`（と Phase 2 の out 文字列）は Windows だけで、コンパイラが宣言した `ole32` の symbol があれば driver が `-lole32` を足す。`CoInitializeEx` は呼ばない | 同梱の MinGW は `-l ole32` なしでリンクに失敗する（現状）。`-lws2_32` の前例。初期化を暗黙にしない | 要承認（承認前は手順 6 に着手しない） |
| D8 | interface は既定で W19 の `local`。agile は対象外 | 共通契約（STA を想定）。Send にする宣言の形は使う例が出てから決める | 既定案（実装者はこの案に従う） |
| D9 | Phase 2 に out 文字列 `Result<ZString, i32>`（`CoTaskMemFree`）と `Com.Iid<I>` を足す | `IShellItem::GetDisplayName` が `LPWSTR *` を返すので、共通契約の項目だけでは例がパスを出せない。`riid`・`ppv` の API を、IID と結果の型が食い違わない形で宣言する | 要承認（承認前は Phase 2 に着手しない） |
| D10 | Phase 3 に、MSVC のメンバー関数の record の結果と interface の本体の `@count` を足す。メソッドの値渡しの `@c_layout` record の引数は [W36](W36-c-layout-by-value.md) の規則で受け、分類を W25 に持たない | Direct2D の `DrawLine`・`GetSize`、DirectWrite の `UINT32` の長さに要る。値渡しの分類は `extern def` とスロットの呼び出しで同じなので 1 か所（W36 の `llvm_byvalue`）に置く。E05 の決定と W21 D4 の改訂は W36 の D1 が持つ。`@count` の置き場の追加は D-53 の改訂 | 要承認（承認前は Phase 3 に着手しない） |
| D11 | WASM は到達した使用を生成の段で `E1008`（`abi_native_only`）にし、宣言は受理する | 型検査は target を知らない。宣言だけで拒むと、Windows のモジュールを含むプロジェクトを wasm で検査できない | 既定案（実装者はこの案に従う） |
