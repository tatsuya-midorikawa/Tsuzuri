# W26: COM オブジェクトを Tsuzuri で実装する（UI Automation・TSF・ドラッグ＆ドロップのため）

| 項目 | 内容 |
| --- | --- |
| ID | W26 |
| 優先度 | P2 |
| 規模 | XL（Phase 1〜3。人間が求めない限り Phase 1 だけを実装する） |
| 依存 | [W22](_completed/W22-fnptr-values.md), [W23](_completed/W23-anchor-state.md), [W25](W25-extern-interface-com.md)（Phase 2・3 の前提は「着手条件」と「前提とする他チケットのインターフェース」に書く） |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-22（D-63。利用者の依頼「X: D13 を狭く改める方針で決定。実現向けたチケットを @_features に W プリフィックスのチケットを作成して」と「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」から起票。HEAD `fa40eede` で確認） |
| 承認 | 要承認（宣言の形、D-55）。決定事項の D1・D6・D10 |
| 改善する劣位 | リスク「アクセシビリティと IME」: 独自に描画した窓は、UI Automation の提供者・TSF のテキストストア・OLE のドロップ先を COM オブジェクトとして OS に渡さなければならない。W25 は COM を呼ぶ側だけで、OS に渡すオブジェクトを Tsuzuri では作れない |
| 手本にする既存実装 | W25 の `InterfaceInfo`・`Result<J, i32>` の下げ方・IID の定数（`llvm_com::iid_constant`）。`src/llvm_abi.rs` の `wrapper`（`WrapperKind::Callback`、`@tz.callback.<name>`）と `src/abi.rs` の `callback`。`src/llvm.rs` の `with_blocking_externs`（コールバックの本体の `tsuzuri_tz_callback_enter`・`_exit`）。TZ05 のタイマーの記録（G より長く生きるランタイムの記録を `malloc` する） |
| 主な影響ファイル | `src/check.rs`・`src/abi.rs`・`src/llvm_abi.rs`（コールバックの包みの型を広げる）・`src/llvm_com.rs`（W25 の新規ファイルにオブジェクト・vtable・thunk を足す）・`src/llvm.rs`・`src/effects.rs`・`src/pure_rules.rs`・`src/kg/effects.rs`・`src/capabilities.rs`・`src/pinned_rules.rs`（W19）・`src/diagnostic_kinds.rs`、`tests/com_objects.rs`（新規）・`tests/com_objects.mjs`（新規）・`tests/com_objects_host.c`（新規）・`tests/fixtures/com_objects/Main.tz`（新規）・`tests/windows.mjs`、文書 |

## 実装者向け早見

- 範囲: `extern interface`（W25）の宣言から vtable を合成し、メソッドの実装を `FnPtr`（W22）で、状態を `Anchor`（W23）の鍵で結ぶ COM オブジェクトを `Com.object` で作る。参照カウントはコンパイラが生成する `AddRef`・`Release`・`QueryInterface` が持つ。Phase 1 は 1 つの interface（と基底の連鎖）、スカラーの out 引数、`[in]` のインターフェース、`[out]` のインターフェースの結果。
- 最初に読む: ① 仕様 1〜6 ② W25 の仕様 3〜4 ③ `llvm_abi::wrapper` と `abi::callback` ④ W23 の `Anchor.read`・`Anchor.update` と `Anchor.Error`。
- 最初に書くテスト: `tests/com_objects.rs` の `methods_record_follows_the_interface`（`I.Methods` の field の名前と型）→ `tests/com_objects_host.c` が vtable を通して呼ぶ `drive`。
- 触ってはいけない: 既存のコールバック・`export def` の包みの ABI（新しい型を使わないコールバックの IR は 1 byte も変えない）。W12 の契約（`main` の後の呼び出しはトラップ）。
- 完了の見分け方（Phase 1）: 次のプログラムを C の `tzobj_drive`（vtable を通して `tick` を 1〜5 で呼び、`last` で最後の値を受け取る）とリンクすると 15 を返す。

```tsuzuri
@trusted
extern interface ITicker : Com.IUnknown "1c3f6a52-9b0d-4e7a-8f21-5d6c7b8a9e03" {
    tick :: i32 -> i32
    last :: ref mut i32 -> i32
}

@trusted
extern "tzobj_drive" def drive :: ref ITicker -> i32 -> i32

def on_tick :: ref ITicker -> i64 -> i32 -> i32 = \_self token step ->
    match Anchor.update (\total -> total + (step as i64)) token with
    | Ok _ -> 0
    | Error e -> Com.anchor_error e

def on_last :: ref ITicker -> i64 -> ref mut i32 -> i32 = \_self token out ->
    match Anchor.read (\total -> deref total) token with
    | Ok total ->
        deref out = total as i32
        0
    | Error e -> Com.anchor_error e

def main :: unit -> i32 = \() ->
    let zero: i64 = 0
    let state = Anchor.create zero
    let ticker: ITicker =
        Com.object (Anchor.token (ref state)) (ITicker.Methods { tick: FnPtr.of on_tick, last: FnPtr.of on_last })
    drive (ref ticker) 5 |> ignore
    match Anchor.read (\total -> deref total) (Anchor.token (ref state)) with
    | Ok total -> total as i32
    | Error _ -> -1
```

## 目的

標準のコントロール（`BUTTON`・`EDIT` など）だけで作る窓なら、UI Automation の提供者は OS が出し、IME は IMM32 のメッセージ（`WM_IME_COMPOSITION` と `ImmGetCompositionStringW`・`ImmSetCompositionWindow`）で、ファイルのドロップは `DragAcceptFiles` と `WM_DROPFILES`・`DragQueryFileW` で扱える。どれも `extern` の関数（W13〜W20）だけで呼べ、このチケットは要らない。文書はこの順で勧める。

それで足りないのは、独自に描画したコントロールのアクセシビリティ（`WM_GETOBJECT` に `IRawElementProviderSimple` などを返す）、TSF の全機能（再変換、候補の位置、`ITextStoreACP`）、OLE のドロップ先（`IDropTarget`。ドラッグ中の効果の表示やファイル以外の形式）、ファイルダイアログのイベント（`IFileDialogEvents`）である。どれも OS が呼ぶ COM オブジェクトを、アプリが作って渡す必要がある。

このチケットは、その実装を Tsuzuri だけで書けるようにする。利用者の依頼「改善が必要なものはあなたが考える中で最も良いと考えられる方法で」に従い、案を比べて 1 つを既定案にした（設計の「採らなかった案」）。

## 着手条件と停止条件

### 着手条件

- [W22](_completed/W22-fnptr-values.md)・[W23](_completed/W23-anchor-state.md) が done、[W25](W25-extern-interface-com.md) の Phase 1 が done。確認: `grep -nE "^\| W(22\|23\|25) " _features/README.md`。
- Phase 2: [W21](W21-c-layout-records.md) の Phase 1 が done（`@c_layout` と `read_host_record`・`write_host_record`）。Phase 3: W25 の Phase 3、[W36](W36-c-layout-by-value.md) の Phase 1（`@c_layout` record の値渡しの分類と、呼ばれる側の下げ方）と [W13](_completed/W13-extern-ref-mut-and-mutable-buffers.md)（`ref mut [T]`・`@count`）が done。どれも依存の欄（README の表）に無い Phase ごとの前提。
- 決定事項の D1・D6 が承認済み（Phase 3 は D10 と W25 の D10 も）。
- ベースライン: `tests/fixtures/*`・`examples/*` の `--emit llvm`（native・`--target wasm32`）と `check --json`、コールバックを使う fixture（`tests/fixtures/ffi_extensions`）の IR を `target/scratch/w26/before/` に保存する。

### 停止条件

- **S1** 新しい引数・結果の型のために、コールバックの包み（`llvm_abi::wrapper`）を複製したくなった（W12 の「包みは 1 種類」に反する）。
- **S2** 新しい型を使わないコールバック・`export def` の IR、Com を使わないプログラムの IR・診断が 1 byte でも変わる。
- **S3** オブジェクトの `Release` を Tsuzuri のフレーム（`tsuzuri_tz_callback_enter` を通る経路）か、E14 の境界が追跡する確保（`tsuzuri_trap_hooks` の `allocate`）に頼らないと書けない。
- **S4** W23 の `Anchor` が別のスレッドの呼び出しを `Error` にせず、オブジェクトから所有者の値へ別のスレッドの経路ができる（W12 の「判定の義務」）。
- **S5** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

- **コールバックの型**: `abi::callback`（`src/abi.rs`）は、引数を `abi_scalar`（スカラー・ハンドル・ハンドルの `ref`）、結果をスカラーか `unit` に限る。`ref R`・`ref mut`・`Result` を取るコールバックは `E1008`（docs/language.md の「コールバック」節）。
- **包み**: `llvm_abi::wrapper`（`WrapperKind::Export`・`Callback`）が C の引数を Tsuzuri の値に直し、結果を C の値に直す。コールバックの包みは internal の `@tz.callback.<修飾名>`。spawn するネイティブのプログラムでは `with_blocking_externs`（`src/llvm.rs`）が、`@tz.callback.` の本体を `tsuzuri_tz_callback_enter`・`_exit` で挟む。
- **COM を実装する手段**: ない。vtable を作る構文も生成もなく、`extern interface` 自体が W25 で入る。利用者が record と `extern view`（W24）で vtable を書くと、メモリの確保とアドレスの計算が要り、D-63 の範囲（生ポインター・ポインター演算なし）を出る。
- **大域の状態**: 「モジュール共有の可変状態は言語レベルで徹底して排除」（docs/language.md の「ホスト関数のインポート」の前の段落）。OS が呼ぶオブジェクトからアプリの状態へ届く道は W23 の `Anchor` だけになる。
- **境界が追跡する確保**: E14 の境界の中では、公開の `tsuzuri_alloc` が `tsuzuri_trap_hooks` の `allocate`（`tsuzuri_tracked_malloc`）を通り（`src/llvm_abi.rs` の `native_allocator`、docs/architecture.md の `tsuzuri_trap_hooks` の段落）、境界のランタイムを使う構成ではヒープの IR も追跡する確保に差し替わる（`src/llvm.rs` の `heap_native_tracked`）。OS が持ち続けるオブジェクトをこの確保で作ると、境界のトラップの後片付けと食い違う。
- **SDK のシグネチャ**（`Include/10.0.26100.0/um/`）:

| interface（IID） | ヘッダー | このチケットに要る形 |
| --- | --- | --- |
| `IFileDialogEvents`（`973510db-7d7f-452b-8975-74a85828d354`） | `shobjidl_core.h` | 7 つのメソッド。引数は `IFileDialog *`・`IShellItem *`（`__RPC__in_opt`）と `[out] FDE_SHAREVIOLATION_RESPONSE *`・`FDE_OVERWRITE_RESPONSE *`。Phase 1 の型で書ける |
| `IRawElementProviderSimple`（`d6dd68d1-86fd-4332-8666-9abedea2d24c`） | `uiautomationcore.h` | `get_ProviderOptions(enum ProviderOptions *)`、`GetPatternProvider(PATTERNID, IUnknown **)`、`GetPropertyValue(PROPERTYID, VARIANT *)`、`get_HostRawElementProvider(IRawElementProviderSimple **)`。`VARIANT *` は Phase 2（`@c_layout`） |
| `ITfContextOwnerCompositionSink`（`5F20AA40-B57A-4F34-96AB-3576F377CC79`） | `msctf.h` | `OnStartComposition(ITfCompositionView *, BOOL *)` など 3 つ。Phase 1 の型で書ける |
| `ITextStoreACP` | `textstor.h` | vtable は 29 項目。`GetTextExt(…, RECT *, BOOL *)`、`GetStatus(TS_STATUS *)`、`GetText` は呼び出し側の `WCHAR *` の buffer に書く。Phase 2・3 |
| `IDropTarget`（`00000122-0000-0000-C000-000000000046`） | `oleidl.h` | `DragEnter(IDataObject *, DWORD, POINTL, DWORD *)`。`POINTL` は値渡し、`DWORD *` は `[out][in]`。Phase 3 |

- **import library**: `UIAutomationCore.lib`・`Ole32.Lib`（SDK 10.0.26100 の `um/x64`）、`uiautomationcore.def`・`ole32.def.in`（同梱の zig の `lib-common`）がある。`msctf.lib` は SDK にない（TSF の `ITfThreadMgr` は `CoCreateInstance` で得る）。

## 変えてはいけない意味

- W12 の契約: 静的コールバック（と `FnPtr`）は `main` の境界の終わりまでどのスレッドからでも呼べ、その後の呼び出しはトラップ。オブジェクトのメソッドの実装もこれに従う。
- 既存のコールバック・`export def` の型・ABI・IR。
- `Anchor` の意味（W23）。オブジェクトは状態を所有しない。

## 仕様

### Phase の範囲

| Phase | 内容 | 規模 | 前提 |
| --- | --- | --- | --- |
| 1 | `I.Methods`・`Com.object`（1 つの interface）・`Com.anchor_error`、生成する `QueryInterface`・`AddRef`・`Release`、コールバックの型の `ref J`・`Maybe<ref J>`・スカラーの `ref mut T`・`Result<J, i32>`。C の偽の OS で全ネイティブ target を検査し、Windows で `IFileDialogEvents` を登録する | L | W22・W23・W25 Phase 1 |
| 2 | 複数の interface のオブジェクト、`@c_layout` の `ref R`・`ref mut R`。UI Automation の提供者（`WM_GETOBJECT`）と TSF のコンポジションのシンク | M | W21 Phase 1 |
| 3 | `IDropTarget`（値渡しの `POINTL`）と `ITextStoreACP`（呼び出し側の buffer に書く引数） | L | W25 Phase 3・W36 Phase 1・W13 |

### 1. メソッドの表 `I.Methods`（Phase 1）

- `extern interface I` ごとに、コンパイラが record 型 `I.Methods` を作る。名前の解決は W25 のメソッドと同じ（宣言したモジュールでは `I.Methods`、別のモジュールからは `Module.I.Methods`）。メソッドに `Methods` という名前は付けられない（`E1001`、kind 案 `interface_member_reserved`）。
- field は連鎖のメソッド（`Com.IUnknown` の 3 つを除く）で、名前はメソッドの名前、型は `FnPtr<(ref I -> i64 -> P1 -> … -> Pn -> R)>`（`m :: unit -> R` は `FnPtr<(ref I -> i64 -> R)>`）。1 つ目の引数は呼ばれたオブジェクト自身（呼び出しの間だけの借用）、2 つ目は `Com.object` に渡した鍵。
- 構築は record のリテラル（`ITicker.Methods { tick: FnPtr.of on_tick, last: FnPtr.of on_last }`）だけで、すべての field を書く（欠けた field と型の違いは record のリテラルの既存の `E1003`）。field は名前で照合し、宣言の順に依らない（同じ型のメソッド `on_folder_change`・`on_selection_change`・`on_type_change` を取り違えない）。
- `I.Methods` は Copy（field がすべて Copy）。ABI の位置には置けない（`E1008`）。

### 2. `Com.object`（Phase 1）

| 関数 | 型 | 意味 |
| --- | --- | --- |
| `Com.object` | `i64 -> I.Methods -> I` | 鍵とメソッドの表からオブジェクトを作り、参照を 1 つ持つ `I` を返す |
| `Com.anchor_error` | `Anchor.Error -> i32` | `Anchor.Error` を HRESULT にする（仕様 6） |

- `I` は期待型と表の型の両方から決まり、interface でなければ `E1005`（W25 と共有する kind `com_target_not_interface`）。
- 効果は `extern` と `alloc`、権限は `ffi`。戻り値は W25 の interface なので W19 の `local`（`Com.object` を呼ぶ所も固定された文脈）。WASM で到達したら `E1008`（`abi_native_only`）。
- 引数の順序（D-59）: どちらも対象のない構築か唯一の引数なので例外は要らない。`Com` は W25 が予約する組み込みのモジュールで、2 つの関数名を D-55 に登録する（決定事項 D1）。

### 3. 呼ばれ方（Phase 1）

OS がスロット `s` を呼ぶと、コンパイラが生成した thunk が次をする。

1. 結果が `i32`（HRESULT とみなす）か `Result<_, i32>` のメソッドで、`ref J`・`ref mut T` の引数か `[out]` のポインターが NULL なら、実装を呼ばずに `-2147467261`（`E_POINTER`）を返す。それ以外の結果のメソッドでは、包みが NULL をトラップにする（COM の `[out]`・`[in]` は NULL を渡さない決まりで、破るのはホストの誤り）。
2. オブジェクトの先頭から鍵と `FnPtr` を読み、`FnPtr` の包み（`@tz.callback.<実装の名前>`。W22・W12 の 1 種類の包み）を `(this, 鍵, 引数…)` で呼び、その結果を返す。

- 包みは W12 の契約に従う: `main` の境界の後に呼ばれたらトラップする。OS がまだオブジェクトのメソッドを呼びうるなら、`main` が返る前に登録を外す（`IFileDialog::Unadvise`・`RevokeDragDrop`・`UiaReturnRawElementProvider(hwnd, 0, 0, NULL)`）。`AddRef`・`Release`・`QueryInterface` は Tsuzuri のコードを走らせないので、`main` の後もどのスレッドからでも呼べる（仕様 5）。
- spawn するプログラムでは、包みの `tsuzuri_tz_callback_enter`・`_exit`（今の `with_blocking_externs` の扱い）が P を取り戻す。thunk 自身は Tsuzuri のコードを走らせないので包まない。

### 4. コールバックの型を広げる（Phase 1。`FnPtr` と `extern` の関数型の引数に共通）

| Tsuzuri の型 | C の型 | 包みのすること |
| --- | --- | --- |
| 引数 `ref J`（`J` は interface） | `J *` | 呼び出しの間だけの借用として渡す。NULL は仕様 3 |
| 引数 `Maybe<ref J>` | `J *` | NULL は `None` |
| 引数 `ref mut T`（`T` はスカラーか W18 の `copy` ハンドル） | `T *` | `*p` を局所へ読み、`ref mut` で渡し、戻った後に書き戻す。NULL は仕様 3 |
| 結果 `Result<J, i32>`・`Result<Maybe<J>, i32>` | HRESULT を返し、最後の引数 `J **` | `Ok j` は `*out = j`（参照を移す。`Release` しない）で 0、`Ok None` は `*out = NULL` で 0、`Error h` は `*out = NULL` で `h` |

- `Error h` の `h` はそのまま返す（負の値を使う。0 以上を返せば、呼んだ側には成功と NULL に見える。文書に書く）。
- これらの型を使わないコールバックの包みの IR は変わらない（S2）。

### 5. 参照カウントと `QueryInterface`（Phase 1）

- オブジェクトはコンパイラが生成する IR が C の `malloc` で確保し、参照数が 0 になったら `free` する（`@tz.alloc` も E14 の境界が追跡する確保も使わない。現状の「境界が追跡する確保」）。配置は `{ ptr vtable, i32 参照数, i32 予約, i64 鍵, [n x ptr] 実装 }`（8 バイト整列）。
- `AddRef`: 参照数を atomic に 1 増やし、新しい値を返す。2^31 を超えるなら `llvm.trap`（Rust の `Arc` と同じく、数の折り返しによる解放後の使用を防ぐ）。
- `Release`: atomic に 1 減らし（acq_rel）、0 になったら `free` して 0 を返す。状態（`Anchor` の値）には触れないので、どのスレッドの `Release` でも安全。
- `QueryInterface(riid, ppv)`: どちらかが NULL なら `E_POINTER`。`riid` が `IUnknown` か連鎖のどれかの IID なら `*ppv = this`、`AddRef`、0。それ以外は `*ppv = NULL`、`-2147467262`（`E_NOINTERFACE`）。`IMarshal`・`IAgileObject`・`INoMarshal` にも答えないので、COM は別のアパートメントへ渡すとき標準のマーシャリングでオブジェクトのスレッドへ呼び出しを戻す（オブジェクトは agile でない）。
- この 3 つは Tsuzuri のコードを走らせず、`tsuzuri_tz_callback_enter` を通らず、トラップしない（`AddRef` の溢れだけ）。

### 6. 状態とスレッド（Phase 1）

- オブジェクトが持つのは `Anchor.token` の鍵（`i64`）だけで、`Anchor<T>` の所有者も `T` も持たない（S4。W12 の「判定の義務」に対し、オブジェクトは別のスレッドの値への経路を作らない。届くのは W23 が検査する鍵の表だけ）。
- 実装は `Anchor.read`・`Anchor.update` で状態を読み書きし、`Error` を `Com.anchor_error` で HRESULT にして返す。

| `Anchor.Error`（W23 の `Stale \| WrongType \| WrongThread \| Busy`） | HRESULT | 値 |
| --- | --- | --- |
| `Stale`（所有者が drop された・鍵が 0 か偽） | `RPC_E_DISCONNECTED` | `-2147417848`（`0x80010108`） |
| `WrongType`（預けた型と読む型が違う。プログラムの誤り） | `E_UNEXPECTED` | `-2147418113`（`0x8000FFFF`） |
| `WrongThread`（預けたスレッドでない。COM がアパートメントを越えて直接呼んだ） | `RPC_E_WRONG_THREAD` | `-2147417842`（`0x8001010E`） |
| `Busy`（`read`・`update` の最中の再入） | `E_ILLEGAL_METHOD_CALL` | `-2147483634`（`0x8000000E`） |

- 再入（実装が呼んだ OS の関数が同じオブジェクトのメソッドを呼ぶ）はトラップせず、`Anchor` の `Busy` で返る。モーダルな関数を呼ぶ前に状態を取り出しておく規律は W23 の文書に従う。

### 7. Phase 2: 複数の interface・`@c_layout`・UI Automation・TSF

- `Com.object :: i64 -> (I1.Methods, …, Ik.Methods) -> I1`（`k` は 2〜8。決定事項 D7）。同じ interface を 2 度書く・一方が他方の基底（`E1008`、`com_duplicate_interface`・`com_redundant_interface`）、9 個以上（`E1017`、`interface_limit`）。配置は `{ [k x ptr] vtable, i32 参照数, i32, i64 鍵, 実装 }` で、interface `j` の thunk は `this` から `8 × (j − 1)` を引いて先頭を求める。`QueryInterface(IUnknown)` は常に先頭（同一性の規則）。
- コールバックの型に `ref R`・`ref mut R`（`R` は W21 の `@c_layout` record。読み込み・書き戻しは W21 の `read_host_record`・`write_host_record`）。
- UI Automation の例: 窓のプロシージャ（W12 の静的コールバック）が `WM_GETOBJECT` で `lParam` が `UiaRootObjectId`（-25）なら `UiaReturnRawElementProvider(hwnd, wParam, lParam, provider)` を返し、`WM_DESTROY` で `UiaReturnRawElementProvider(hwnd, 0, 0, NULL)`。提供者は `IRawElementProviderSimple`。`GetPropertyValue` の `VARIANT *` は `@c_layout record Variant { vt: i16u, reserved1: i16u, reserved2: i16u, reserved3: i16u, data: i64, extra: i64 }`（x64・ARM64 で 24 バイト。名前の文字列は `SysAllocString` が返す `BSTR` のアドレスを `data` に入れ、所有は呼んだ側へ移る）。`GetPatternProvider` は `Result<Maybe<Com.IUnknown>, i32>`（対応しないパターンは `Ok None`）。
- TSF の例: `ITfContextOwnerCompositionSink` を、テキストストアと同じオブジェクトの 2 つ目の interface にする。

### 8. Phase 3: `IDropTarget` と `ITextStoreACP`

- `IDropTarget`: `POINTL` の値渡しの引数を、[W36](W36-c-layout-by-value.md) の分類（Phase 1 は Windows x64・Arm64。`llvm_byvalue::classify`）でコールバックにも許す。呼ばれる側の wrapper は、引数を W36 の `by_value_from_abi` で、結果を `by_value_to_abi` で下げる（W36 は呼ばれる側を対象外にし、W26 の範囲とする。W36 の D6）。`OleInitialize`・`RegisterDragDrop(hwnd, ref target)`・`RevokeDragDrop`。`DWORD *pdwEffect` は `ref mut i32u`。
- `ITextStoreACP`: 呼び出し側が渡した buffer に書く引数（`GetText` の `WCHAR *pchPlain` と長さ `cchPlainReq`）を、コールバックの引数 `ref mut [i16u]` として渡す。長さの位置は interface の本体の `@count`（W25 の Phase 3、W13）で示す。`TS_RUNINFO *` の配列は `ref mut [R]`（`R` は `@c_layout`）。
- どちらも Windows だけで、`E2002`・`E1008` の規則は W25 と同じ。

## 設計

### 型検査

- `src/check.rs`: `InterfaceInfo` から `I.Methods` の record を作る（W25 の名前の解決に `Methods` の枝を足す）。`Builtin::ComObject`・`ComAnchorError` を足し、`Builtin::ALL`・`name`・`signature`・`emit_builtin`・効果と権限の表・W19 の `pinned_rules::builtin_position` を更新する。`Com.object` の型は「期待型 `I` と表の型 `I.Methods`」の照合で、Phase 2 は組の要素を順に照合する。
- `src/abi.rs`: `callback` に仕様 4 の型を足す（`callback_parameter`・`callback_result` に分ける）。`abi_scalar` は変えない。

### 生成（`src/llvm_com.rs`、`src/llvm_abi.rs`）

- 包み（`llvm_abi::wrapper`）: 仕様 4 の引数と結果の変換を足す。新しい型の変換は、W25 の `Result<J, i32>` の下げ方を逆向きに使う 1 つの関数にまとめる。包みは 1 種類のまま（S1）。
- クラス: `Com.object` に到達した interface の並び（Phase 1 は 1 つ）を「クラス」とし、長さ付きの符号化（`llvm_abi::record_name` と同じ）でクラス名を作る。クラスごとに、vtable の定数 `@tz.com.vtable.<クラス>.<j>`（`[3 + n x ptr]`）、thunk `@tz.com.thunk.<クラス>.<j>.<slot>`、`IUnknown` の 3 つの入口を生成する。クラス名の `BTreeMap` の順に出す（決定的）。
- `IUnknown`: 共通の `@tz.com.object_add_ref(ptr)`・`@tz.com.object_release(ptr)` と、クラスごとの `QueryInterface`（IID の定数を `memcmp` の代わりに 2 つの `i64` の比較で照合）。`declare ptr @malloc(i64)` と `declare void @free(ptr)` は既存のヒープの IR と同じ宣言の行にする（`intrinsics` で 1 つになる）。
- thunk: `define internal <C の結果> @tz.com.thunk…(ptr %this, <C の引数>) nounwind`。仕様 3 の NULL の検査、先頭の計算、`%fn` の読み出し、`call … %fn(ptr %this, i64 %token, …)`。thunk は `@tz.callback.` で始まらない名前なので `with_blocking_externs` は包まない（Tsuzuri のコードは包みの中だけで走る）。
- `Com.object`: `malloc` が NULL ならトラップ（確保の失敗の既存の扱い）。vtable・参照数 1・鍵・表の `FnPtr` を書いて返す。

### 採らなかった案

| 案 | 採らなかった理由 |
| --- | --- |
| 宣言の形 `extern object Name : I { m = f }`（vtable をクラスごとの定数にし、thunk が実装を直接呼ぶ） | 新しい宣言の構文（構文・符号化・整形・`FRONTEND_FORMAT`）が要るうえ、`FnPtr`（W22）という「Tsuzuri の関数を C のポインターにする」唯一の仕組みと別の経路を作る。間接呼び出しが 1 回減るだけで、COM の呼び出しはもともと間接呼び出し。起票時の既定案でもない |
| interface ごとのランタイムの C の shim | interface の数だけ C が要り、増やせない。D-63 は、ポインターを触るコードをコンパイラが宣言から生成する方針 |
| 捕捉のある関数値と実行時に作る thunk（libffi の closure） | 実行できるメモリの確保（W^X・ACG）と CFG の表に載らない呼び先（W30）、寿命の契約（E12 の Phase 2 の未決）が要る |
| オブジェクトが状態を所有し、最後の `Release` で drop する | `Release` は OS のどのスレッドからでも来るので、Send でない状態を別のスレッドで drop する。最後の `Release` の中の再入も起きる |
| 利用者が `@c_layout` の record と `extern view` で vtable を書く | メモリの確保・アドレスの計算・参照カウントを利用者が書くことになり、D-63 の範囲を出る |

## 前提とする他チケットのインターフェース

- [W25](W25-extern-interface-com.md): `InterfaceInfo`（平らなスロット・IID）、`llvm_com::iid_constant`、`Result<J, i32>` の下げ方、`ref J`・`Maybe<ref J>` の ABI の種類、`Com.IUnknown`、組み込みのモジュール `Com`、kind `com_target_not_interface`。Phase 3 の interface の本体の `@count`。
- [W36](W36-c-layout-by-value.md)（Phase 3）: `llvm_byvalue::{ByValue, classify}` と `FunctionEmitter::{by_value_slot, by_value_from_abi, by_value_to_abi}`（`POINTL` の値渡しの分類と、呼ばれる側の下げ方）。
- [W22](_completed/W22-fnptr-values.md): `FnPtr<F>`・`FnPtr.of name`（`ptr @tz.callback.<name>` に下がる。Copy・Send・Sync）。W26 は `F` に使える型を仕様 4 だけ広げる。
- [W23](_completed/W23-anchor-state.md): `Anchor.create`・`Anchor.token`・`Anchor.read`・`Anchor.update` と `Anchor.Error`（`Stale`・`WrongType`・`WrongThread`・`Busy`）。所有者のスレッドの検査と、`read`・`update` の最中の再入の `Busy`。W23 のランタイム（`tsuzuri_tz_anchor_*`）を持つので、Windows の `--emit object` は W23 と同じく既存の `E2002`。W23 の D8（`Anchor<T>` は Send でも Sync でもなく、`T` に Send を求めないので、W19 の `local` のハンドルを持てる）により、Phase 2 の例は提供者の interface の値を `Anchor<T>` の `T` に入れる。
- [W12](_completed/W12-static-callback-lifetime.md): 仕様 1・2 の契約（`main` の後はトラップ）と「包みは 1 種類」。
- [W19](_completed/W19-thread-bound-handles.md): `pinned_rules::builtin_position`、固定された文脈（コールバックと `FnPtr.of` の本体を含む。W19 の D5）。
- [W21](W21-c-layout-records.md): `@c_layout`、`read_host_record`・`write_host_record`（Phase 2）。[W13](_completed/W13-extern-ref-mut-and-mutable-buffers.md): `ref mut [T]` と `@count`（Phase 3）。
- [W06](_completed/W06-windows-gui-test-harness.md): `tests/windows_gui_shim.c`（`tz_gui_send`・`tz_gui_post` など）と `buildGui`（仕様 4。Phase 2 の Windows の段）。W06 の窓は `HWND_MESSAGE` だけ（W06 の D3）なので、`WM_GETOBJECT` を受けるメッセージ専用でない隠れた窓を作る関数（案 `tz_gui_hidden_window(proc)`）は W26 が W06 の仕様 4 の形で shim に足す。
- kind `abi_native_only`（W13 などと共有）。

## 他チケットへ提供するインターフェース

- [W27](W27-win32-package-from-winmd.md): 生成器は `I.Methods` を生成しない（コンパイラが interface の宣言から作る）。シンクの interface（`IFileDialogEvents` など）も `extern interface` として生成すれば、そのまま `Com.object` に使える。
- [W30](W30-windows-hardening.md): vtable の項目（thunk と `IUnknown` の入口）は定数に置かれる関数のアドレスなので、CFG の表（`cfguard` の module flag）を有効にすれば LLVM が有効な呼び先に載せる。

## 実装手順

### Phase 1

1. **変更**: なし（ブランチ `feat/w26-com-objects`、`target/scratch/w26/`）。**内容**: `node scripts/repo-kb.mjs brief W26`、着手条件、ベースライン。**確認**: 保存した件数を控える。
2. **変更**: `src/abi.rs`・`src/llvm_abi.rs`。**内容**: 仕様 4（コールバックの型と包みの変換）。**確認**: `tests/com_objects.rs` の包みの検査（`ref mut i32` の書き戻し、`Maybe<ref J>` の NULL、`Result` の 3 つの場合の IR）。`tests/fixtures/ffi_extensions` の IR がベースラインと一致（S2）。`node tests/ffi_extensions.mjs target/debug/tsuzuri`。
3. **変更**: `src/check.rs`・`src/diagnostic_kinds.rs`。**内容**: `I.Methods`、`Com.object`・`Com.anchor_error` の型、診断。**確認**: `tests/com_objects.rs` の型の検査。`cargo nextest run --locked`。
4. **変更**: `src/llvm_com.rs`・`src/llvm.rs`。**内容**: クラス・vtable・thunk・`IUnknown`・`Com.object` の生成。**確認**: IR の検査（vtable の並び、`malloc`・`free` の宣言が 1 つ、2 回の生成で一致）。ベースラインと一致（S2）。
5. **変更**: `src/effects.rs`・`src/pure_rules.rs`・`src/kg/effects.rs`・`src/capabilities.rs`・`src/pinned_rules.rs`。**内容**: 分類と件数の pin（現在の値 + 足した数）。**確認**: `tests/capabilities.rs`・`tests/thread_bound_handles.rs`（W19）。
6. **変更**: `tests/com_objects_host.c`・`tests/com_objects.mjs`・`tests/fixtures/com_objects/Main.tz`、`tests/windows.mjs`。**内容**: E2E（下）。**確認**: `node tests/com_objects.mjs target/debug/tsuzuri`、Windows で `node tests/windows.mjs target/debug/tsuzuri.exe`。
7. **変更**: 文書。**確認**: T2、`node scripts/check-docs.mjs <変更したページ>`、PR に `ci:full`。

### Phase 2・3

- Phase 2: 組の `Com.object` → `@c_layout` の `ref R`・`ref mut R` → UI Automation の段 → TSF のシンクの段。Phase 3: 値渡しの `POINTL` → `IDropTarget` の段 → buffer の引数 → `ITextStoreACP` の最小の段（ASCII の 1 行のテキストの `GetText`・`GetStatus`・`GetTextExt`）。各段で T1 と追加した検査。

## テスト計画

### Rust（`tests/com_objects.rs`、新規）

| 検査 | 期待 |
| --- | --- |
| `I.Methods` | field の名前と型が仕様 1（基底のメソッドを含み、`Com.IUnknown` の 3 つを含まない）。別のモジュールからは `Module.I.Methods` |
| 拒否 | field の欠け・型の違い（`E1003`）、`Com.object` の期待型が record（`E1005`）、メソッド名 `Methods`（`E1001`）、`FnPtr.of` にラムダ（W22 の診断）、`ref mut string` のコールバックの引数（`E1008`）、wasm で到達（`E1008`） |
| IR | vtable の項目の順（`QueryInterface`・`AddRef`・`Release`・スロット順の thunk）、`@malloc`・`@free` の宣言が 1 つ、`tsuzuri_tz_callback_enter` が thunk ではなく包みにある、2 回の生成で一致、Com を使わない入力の IR が変わらない |

### E2E（`tests/com_objects.mjs` と `tests/com_objects_host.c`、全ネイティブ target）

C の側が OS の役をする（vtable を通して呼び、`AddRef`・`Release` の戻り値を記録する）。IID は C の側に手で書いた定数。`tests/ffi_extensions.mjs` と同じく `clang -c` の object を `--link` で渡し、`-O0`・`-O3`、`--allocator counting` で `live == 0`。

| case | 内容 | 期待 |
| --- | --- | --- |
| `drive` | 早見の例 | 15 |
| `counts` | C が `AddRef`（2 を返す）、`QueryInterface(IUnknown)`（同じポインター、3）、`Release`（2）、知らない IID（`E_NOINTERFACE` と NULL）、`ppv` が NULL（`E_POINTER`） | 記録した値が期待の列と一致 |
| `last_release` | C が `AddRef` して保持し、Tsuzuri が drop した後に C が `Release` | 0 を返す |
| `null_out` | C が `last(NULL)` | `E_POINTER`、実装は呼ばれない（状態が変わらない） |
| `wrong_thread` | C が新しいスレッドから `tick` | `-2147417842`、状態が変わらない |
| `reentry` | `tick` の実装が `Anchor.update` の中で C の `tzobj_reenter` を呼び、C が同じオブジェクトの `tick` を呼ぶ | 内側は `-2147483634`、外側は 0 |
| `stale` | 所有者を drop した後に、C が保持したオブジェクトの `tick` を呼ぶ | `-2147417848` |
| `child` | `make_child :: unit -> Result<ITicker, i32>` の実装が新しいオブジェクトを返し、C がその `tick` を呼んで `Release` | 0 を返し、C の側で生きている数が 0 |
| `blocking` | spawn するプログラム（先に tzroutine を 1 つ起動し、チャンネルで結果を待つ）で、`main` の擬似 G が `drive (ref ticker) 5` を呼ぶ。C はブロッキングの `extern` の中から、同じ main スレッドで `tick` を呼ぶ（interface は `local` なので tzroutine には渡せず、別の M からの呼び出しは D2 のとおり `WrongThread` になる） | `drive` の後の状態が 15、tzroutine の結果も届く（包みの `tsuzuri_tz_callback_enter`・`_exit` を通る） |

- `main` の後の呼び出しのトラップは W12 の検査に任せる（`atexit` などの終了処理は W12 の契約の対象外）。

### Windows（`tests/windows.mjs` の段）

- Phase 1: `CoInitializeEx` の後、`Com.create` した `IFileDialog`（表示しない）に `IFileDialogEvents` のオブジェクトを `advise`（`ref mut i32u` の cookie。W13）し、`unadvise` して drop する。HRESULT が 0 で、異常終了しない。MSVC と同梱の MinGW の両方。
- Phase 2: W06 の shim にこのチケットが足すメッセージ専用でない隠れた窓（「前提とする他チケットのインターフェース」）に `SendMessageW(hwnd, WM_GETOBJECT, 0, -25)` を送り、窓のプロシージャが `UiaReturnRawElementProvider` の結果（0 でない）を返す。UI Automation のクライアント（`CUIAutomation` の `ElementFromHandle`）を別のスレッドから使う検査は、CI の環境で動くかを着手時に確かめてから足す（未確認）。

## ドキュメント

- `_tsuzuri/language-reference/compiler/native-interop.md`: 新しい節「COM オブジェクトを実装する」（`I.Methods`・`Com.object`・`Anchor` の鍵・HRESULT の表・登録を `main` の前に外すこと・agile でないこと）。冒頭に、標準のコントロール・IMM32・`DragAcceptFiles` で足りるなら W26 は要らないという順の勧め。
- `docs/language.md` の「コールバック」節（仕様 4 の型）と W25 の小節。`_tsuzuri/language-reference/compiler/diagnostics.md`・`docs/language.md` の診断の表（kind の表は `<!-- generated:diagnostic-kinds -->` の生成の部分なので手で書かず、`src/diagnostic_kinds.rs` に登録してから `node scripts/gen-docs.mjs`）。
- `docs/architecture.md`: オブジェクトの配置、thunk、`IUnknown` の生成、確保に `malloc` を使う理由。

## 受け入れ条件

- [ ] Phase 1 の仕様 1〜6 のとおりに動き、仕様外の入力は表の診断で拒否される。
- [ ] 新しい型を使わないコールバック・`export def` の IR と、Com を使わないプログラムの IR・診断がベースラインと 1 byte も違わない。WASM の既存の出力が変わらない。
- [ ] 生成 IR が決定的で、`malloc`・`free`・IID の宣言が重複しない。
- [ ] `node tests/com_objects.mjs` が全ネイティブ target の `-O0`・`-O3` で通り、`live == 0` と C の側の記録を確かめた。
- [ ] Windows の段が MSVC と同梱の MinGW の両方で通る。PR に `ci:full` を付けた。
- [ ] T2 が通る。

## 落とし穴

- **登録を外さずに `main` を返す**: OS がメソッドを呼ぶと W12 のトラップになる。`Unadvise`・`RevokeDragDrop`・`UiaReturnRawElementProvider(hwnd, 0, 0, NULL)` を窓の破棄か `main` の終わりの前に呼ぶ。`AddRef`・`Release` は後でもよい。
- **`Error` の値**: 0 以上の `Error` は呼んだ側に成功と NULL に見える。負の HRESULT を使う。
- **`Release` から状態に触れない**: 最後の `Release` は OS のどのスレッドからも、`main` の後にも来る。オブジェクトは鍵しか持たない。
- **同じ型のメソッドの取り違え**: `I.Methods` は field の名前で結ぶ。生成する thunk もスロットの番号ではなくメソッドの名前で表の位置を引く。
- **`QueryInterface` の同一性**: Phase 2 の複数の interface でも、`IUnknown` は常に先頭のポインターを返す。違うと UI Automation が要素を同一視できない。
- **境界の中の確保**: E14 の境界の中で `Com.object` を呼んでも、オブジェクトは追跡されない `malloc` で確保する（境界のトラップで OS が持つオブジェクトが解放されないように）。
- **`BSTR` の所有**: `GetPropertyValue` に入れた `BSTR` は呼んだ側が解放する。同じ `BSTR` を 2 度渡さない。

## 対象外

- 集約（`pUnkOuter`）、`IDispatch` の実装（型情報・遅延束縛）、`IConnectionPoint` の汎用の実装、agile なオブジェクト（`IAgileObject`・自由スレッドのマーシャラー）、レジストリに登録するクラスと `DllGetClassObject`（シェル拡張の DLL。W05 の後の別チケット）。
- `VARIANT`・`BSTR` の型と所有（Phase 2 は `@c_layout` の record と `i64` のアドレスで扱う）。
- UI Automation のイベントの発火（`UiaRaiseAutomationEvent`）と、パターンの提供者の網羅。TSF の表示属性・候補の UI。
- オブジェクトの実装の自動の生成（W27 は interface の宣言だけを生成する）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | 実装は `Com.object :: i64 -> I.Methods -> I` で作る。`I.Methods` は interface の宣言からコンパイラが作る record で、field は `FnPtr`。`Com.object`・`Com.anchor_error` と型の名前 `Methods` を D-55 に登録する | 起票時の既定案。新しい宣言の構文を足さず、W22 の `FnPtr` を唯一の「関数を C のポインターにする」経路のまま使う。field の名前で結ぶので同じ型のメソッドを取り違えない（「採らなかった案」） | 要承認（承認前は手順 3 に着手しない） |
| D2 | オブジェクトは `Anchor` の鍵だけを持ち、状態を所有しない | `Release` はどのスレッドからでも来る。所有者のスレッドの検査と再入の `Busy` は W23 が持つ | 既定案（実装者はこの案に従う） |
| D3 | 実装の関数の型は `ref I -> i64 -> P… -> R` | 自分自身を `Com.cast` で別の interface にして返せる（`GetPatternProvider`）。鍵を明示して渡し、`this` から状態を推測しない | 既定案（実装者はこの案に従う） |
| D4 | `QueryInterface`・`AddRef`・`Release` はコンパイラが生成する IR で、C の `malloc`・`free` と atomic の参照数を使い、Tsuzuri のコードを走らせない | OS は `main` の後にもどのスレッドからでも `Release` を呼ぶ。E14 の境界の追跡する確保と食い違わない。到達したクラスの分だけを生成し、新しいランタイムの C のファイルを足さない（`Anchor` のランタイムは W23 のもの） | 既定案（実装者はこの案に従う） |
| D5 | thunk は、HRESULT の形のメソッドの NULL の `[in]`・`[out]` に `E_POINTER` を返す。`i32` の結果を HRESULT とみなす。`main` の後の呼び出しは W12 のトラップのまま | COM の決まり（NULL を渡されたら `E_POINTER`）。W12 の契約を変えない（`main` の後の問い合わせの口を W12 に足さない） | 既定案（実装者はこの案に従う） |
| D6 | コールバックの型に `ref J`・`Maybe<ref J>`・スカラーの `ref mut T`・`Result<J, i32>` を足す（`FnPtr` と `extern` の関数型の引数に共通） | 包みを 1 種類に保つ（W12）。E12 D11 と docs/language.md の「コールバック」節の型の制限を改める | 要承認（承認前は手順 2 に着手しない） |
| D7 | 複数の interface は `Com.object` に `I.Methods` の組を渡す（2〜8 個） | D1 の `Com.object` の引数の型を広げるだけで、名前も承認済みの決定も足さない。新しい構文も要らない。UI Automation（`Simple`・`Fragment`・`FragmentRoot`）と TSF（テキストストアとシンク）に要る数 | 既定案（実装者はこの案に従う） |
| D8 | オブジェクトは agile にしない（`IMarshal`・`IAgileObject` に答えない） | 別のアパートメントからの呼び出しを COM の標準のマーシャリングで作ったスレッドへ戻す。`Anchor` の `WrongThread` が最後の守り | 既定案（実装者はこの案に従う） |
| D9 | `Com.anchor_error` は仕様 6 の表の HRESULT を返す | 実装ごとに違う値を返すと、呼んだ側の再試行やエラーの表示がそろわない | 既定案（実装者はこの案に従う） |
| D10 | Phase 3 にコールバックの値渡しの record（`POINTL`）と buffer の引数（`ref mut [T]` と `@count`）を足す | `IDropTarget`・`ITextStoreACP` に要る。値渡しの分類は [W36](W36-c-layout-by-value.md) の D1・D4（E05 の決定と W21 D4 を `@c_layout` の extern の位置に限って改めたもの）のものを、コールバックの呼ばれる側にも使う（呼ばれる側への広げはこの決定の範囲）。`@count` の置き場は W25 の D10 に従う | 要承認（承認前は Phase 3 に着手しない） |
