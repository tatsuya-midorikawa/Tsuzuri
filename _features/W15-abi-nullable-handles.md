# W15: ABI での `Maybe<H>` と `Maybe<ref R>`（NULL、native だけ）

| 項目 | 内容 |
| --- | --- |
| ID | W15 |
| 優先度 | P1（Win32 の多くの API は NULL を取るか返す。`CreateWindowExW` の `hWndParent`・`hMenu`・`hInstance`・`lpParam`、`LoadCursorW` の `hInstance`、失敗で NULL を返す `CreateWindowExW`・`LoadImageW`・`TlsGetValue` などで、今はどれも型の契約の内で書けない） |
| 規模 | M（判定の関数と extern の側の述語 3 つ、生成の 3 経路（extern の引数・extern の結果・コールバックの引数）、ヘッダーの 2 つの走査、wasm の拒否、テストと文書） |
| 依存 | – |
| 後続 | [W21](W21-c-layout-records.md)（`@c_layout` の field の `Maybe<H>`）、[W24](W24-extern-view.md)、[W27](W27-win32-package-from-winmd.md)（win32metadata の `[Optional]` の写し先）、[NS01](NS01-nonnull-extern-handles.md)（`H` の非 Null 不変条件と `Maybe<H>` の niche）、[NS02](NS02-export-and-bindings-nullable.md)、[NS03](NS03-bindgen-nullability.md) |
| 状態 | todo |
| 起票 | 2026-11-22（D-63。利用者の依頼「X: D13 を狭く改める方針で決定。実現向けたチケットを @_features に W プリフィックスのチケットを作成して」と「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」から起票。HEAD `fa40eede` で確認） |
| 承認 | 要承認（E12 の対象外の解除）。決定事項 D1 が要承認、D2〜D8 は既定案 |
| 手本にする既存実装 | `src/abi.rs` の `abi_scalar`・`parameter`・`result`・`callback`（E12 が足したハンドルの判定）、`src/llvm_imports.rs` の `host_call`（`ref H` の引数の `load ptr`）、`src/llvm_abi.rs` の `wrapper`（コールバックの `ref H` の `spill`）・`c_parameters`・`handle_typedefs`・`record_types`、`src/llvm_layout.rs` の `construct_value`・`nullary_value`・`tag_value`・`payload_value`、`src/llvm_sync.rs` の compare-exchange（2 つの共用体の値を `construct_value` で作って `select` する形）、`src/llvm.rs` の `domain_unsupported`（target で拒否する診断を extern の位置に出す形） |
| 主な影響ファイル | `src/abi.rs`, `src/check.rs`, `src/llvm.rs`, `src/llvm_imports.rs`, `src/llvm_abi.rs`, `src/diagnostic_kinds.rs`, `tests/abi_nullable.rs`（新規）, `tests/ffi_extensions.mjs`, `tests/ffi_extensions_host.c`, `tests/fixtures/abi_nullable/Main.tz`（新規）, `tests/windows.mjs`, `docs/language.md`, `_tsuzuri/language-reference/compiler/native-interop.md`, `_tsuzuri/language-reference/built-in-types-and-modules/maybe.md`, `_tsuzuri/language-reference/compiler/webassembly.md` |

## 実装者向け早見

- 範囲: extern の引数に `Maybe<H>`・`Maybe<ref H>`・`Maybe<ref R>`（H は `extern type`、R はスカラー record）、extern の結果に `Maybe<H>`、コールバックの引数に `Maybe<H>`・`Maybe<ref H>`。`None` は C の NULL、`Some x` は `x` の C の値。native だけで、wasm32・wasm64 では到達したら `E1008`。
- 最初に読む: ① 仕様 1〜4 ② 設計の「型の文脈」 ③ `src/abi.rs` の 160〜235 行 ④ `src/llvm_imports.rs` の `host_call` ⑤ `src/llvm_layout.rs` の `niche_layout` と `construct_value`。
- 最初に書くテスト: `tests/abi_nullable.rs` の `nullable_positions_are_accepted` と `other_maybe_payloads_keep_their_errors`。
- 触ってはいけない: `Maybe` の内部の配置（`niche_layout`）、`H` の結果の意味（NULL でも `H` のまま）、`export def` の ABI、既存の E1008 の文、wasm の出力、この機能を使わないプログラムの IR・ヘッダー・診断。
- 完了の見分け方: 次のプログラムが Windows の native で終了コード 3 になる（kernel32 だけを使うので `-l` は要らない）。

```tsuzuri
extern type Module
extern type ModuleName
extern type Slot

@trusted
extern "GetModuleHandleW" def module_handle :: Maybe<ModuleName> -> Maybe<Module>
@trusted
extern "TlsAlloc" def tls_alloc :: unit -> i32u
@trusted
extern "TlsGetValue" def tls_get :: i32u -> Maybe<Slot>

def loaded :: Maybe<Module> -> i32 = \m ->
    match m with
    | Some _ -> 1
    | None -> 0

def empty :: Maybe<Slot> -> i32 = \s ->
    match s with
    | Some _ -> 0
    | None -> 2

def main :: unit -> i32 = \() ->
    loaded (module_handle None) + empty (tls_get (tls_alloc ()))
```

## 目的

Win32 と多くの C の API は「無い」を NULL で表す。E12 は `Option<H>` と NULL を「`abi_scalar` が型の文脈を持たない」という理由で見送った（[E12](_completed/E12-ffi-extensions.md) の「追加した拡張と確認」の「実装しなかったもの」）。そのため今は、NULL を渡す引数と NULL を返す結果を `i64` と宣言して 0 と比べるしかなく、ハンドルの型の区別も、結果が NULL でないことの保証も失う。

このチケットは、std の `Maybe` を ABI の境界でだけ NULL に写す。`None` ⇔ NULL、`Some x` ⇔ `x`。内部の配置は変えず、境界で変換する。E12 の見送りの理由には、判定の関数 1 つ（`abi::nullable`）を検査と生成の両方で使い、`abi_type`・`c_type` には C から見た型（`abi::c_view`）を渡すことで答える（設計の「型の文脈」）。[GUIDE D-63](GUIDE.md#d-63-tsuzuri-だけで-win32-のアプリを書くe12-d13-の狭い改訂w01w30) の X の範囲（生ポインター型を入れない）の内にあり、値を参照外しすることはない。

## 着手条件と停止条件

### 着手条件

- `node scripts/repo-kb.mjs brief W15` を読み、D-63 が GUIDE の台帳にあること。決定事項 D1 の承認があること（承認前は手順 2 以降に着手しない）。
- 「現状」の確かめ方のコマンドを着手時の `main` で実行し、食い違えばこのチケットを直してから始める。
- W14（同じ symbol の複数宣言）・W18（`extern type copy`）・W20（`ZString`）・W21 が先に入っているかを見る。入っていれば「前提とする他チケットのインターフェース」の該当の段を行う。

### 停止条件

次の場合は即興で回避せず、止めて状況と候補を報告する（GUIDE §13）。

- **S1** `Maybe` の内部の配置（`niche_layout` の結果）か、この機能を使わないプログラムの IR・ヘッダー・診断を変えないと実装できない。
- **S2** wasm の拒否を「到達した extern だけ」にするのに、生成と別の到達の規則（新しい根の選び方）が要る。
- **S3** `--backend fast`（PB05）が新しい IR を、`E2002` でなく別の結果に訳す。
- **S4** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

### 再現

次の 3 つの宣言（それぞれ `extern type` と `def main :: unit -> i32 = \() -> 0` を添えた `Main.tz`）を `target/debug/tsuzuri.exe check` にかけると、どれも同じ `E1008` になる。

```text
extern "GetModuleHandleW" def get_module :: Maybe<Hmodule> -> Hmodule
  → 3:31: error[E1008]: extern signatures support scalar/unit, shared ABI buffers and scalar records, and owned ABI buffer or scalar record results
extern "GetForegroundWindow" def foreground :: unit -> Maybe<Hwnd>      → 同じ E1008
extern "probe_rect" def probe :: Maybe<ref Rect> -> i32                 → 同じ E1008
```

今の回避は、型の外に出る `i64` の宣言である。`extern "GetModuleHandleW" def module_handle :: i64 -> Module` に `0` を渡し、`extern "TlsGetValue" def tls_get :: i32u -> i64` の結果を `0` と比べると、MinGW（同梱の `tsuzuri-clang`）で動く（終了コード 5 で確認）。ただし NULL を返す API の結果をハンドルとして受けられず、`i64` とハンドルを取り違えても型検査は気づかない。

### 判定（`src/abi.rs`）

- `abi_scalar(ty: &Type)` と `callback(ty: &Type)` は型の文脈（`TypeContext`）を取らない。std の `Maybe` は `Type::Union(id, ..)` で、名前（`TypeContext::unions[id].name == "Maybe.Maybe"` と `origin == ModuleOrigin::Std`）を見なければ分からない。`parameter(ty, types)`・`result(ty, types)` はすでに `types` を取る。
- 呼び出しは 4 か所: `src/abi.rs` の `parameter` と `callback` の中、`src/llvm_abi.rs` の `extended`・`uses_host_abi`。検査の側は `src/check.rs` の `check_modules_collect` で、export（`UnsupportedExportSignature`、`abi::parameter`・`abi::result`）と extern（`abi::callback` の後に `UnsupportedExternSignature`）が同じ判定を使う。
- `src/check.rs` の `HostImport` は `native_symbol`・`wasm_module`・`wasm_name`・`explicit`・`callbacks` だけを持つ。

### 生成（`src/llvm.rs`・`src/llvm_imports.rs`・`src/llvm_abi.rs`）

- `src/llvm.rs` の `abi_type(ty)`・`c_type(ty)` は module を受け取らず、扱わない型は `unreachable!("the type checker enforces scalar exports")`。呼び出しは約 20 か所（多くは record の field）。
- `host_call` は `ref H` の引数を `load ptr` してハンドルそのものを渡し、record は `host_result_slot` に `write_host_record` した一時領域を渡す。`wrapper` はコールバックの `ref H` の引数を `spill` した slot で本体に渡す。
- `handle_typedefs` と `record_types` の `visit` は `Handle`・`Reference`・`Function`（`record_types` は `Record`）だけを辿り、共用体の型引数を見ない。`Maybe<H>` の中のハンドルの `typedef`、`Maybe<ref R>` の中の `%tz_record_...` の型の定義はそのままでは出ない。

### 内部の配置（`src/llvm_layout.rs` の `niche_layout`）

- 隙間の符号化（niche）は文字列・配列・関数値と、wasm 以外の「1 つの値への参照」だけ。`Type::Handle` の腕はないので、`Maybe<H>` はタグ付きの共用体（タグと `ptr`）である。
- `Maybe<ref R>` は native では `Niche::NullPointer`（1 つの `ptr`、`None` が null）で、wasm にはない（`--stack-first` で番地 0 に有効なオブジェクトがありうるため）。`def width :: Maybe<ref Rect> -> i32` は `define internal i32 @tz.fn.Main.width(ptr %arg0)` に下がる（`--emit llvm` で確認）。ただし指す先は Tsuzuri の内部の record の配置で、C に渡す正規化した一時領域ではない。

### 決定の記録

- E12 の対象外に「`Option<H>` と NULL の対応」があり、理由は「`abi_scalar` が型の文脈を持たないため、ABI 判定の関数すべてに文脈を通す変更になる」。
- [B07](_completed/B07-user-drop.md) の「チケットから外れた判断」6 は、wasm32 の番号付きハンドルが 0 を有効な値に使いうるので、null・0 を印に使わないと書く。
- `docs/language.md` の「不透明ハンドル」は、ハンドルを置ける ABI の位置を「引数（`H`・`ref H`）と結果（`H`）」に限り、`Maybe` の中は「ABI 境界以外」とする。`tsuzuri bindgen` の段落は「NULL は区別できず」と書く。

確かめ方: `grep -n "pub fn abi_scalar\|pub fn callback\|pub fn parameter\|pub fn result" src/abi.rs`、`grep -n "fn abi_type\|fn c_type" src/llvm.rs`、`grep -n "fn niche_layout" -A25 src/llvm_layout.rs`、`grep -n "Option<H>" _features/_completed/E12-ffi-extensions.md`。

## 変えてはいけない意味

- `Maybe` の内部の配置。`Maybe<H>` は今のタグ付きの共用体のまま。NULL を `None` の印にする内部の niche は足さない（今の `H` の結果は NULL でありうるので、`Some h` の `h` が NULL のことがある。それを `None` と読み違えないため）。
- `H` の結果は今と同じく NULL でも `H` になる。NULL を区別したい API は利用者が結果を `Maybe<H>` と宣言する。
- `export def` の ABI、wasm の出力、既存の E1008 の文、この機能を使わないプログラムの IR・ヘッダー・診断（1 byte も変えない）。

## 仕様

### 1. 置ける位置

| 位置 | 型 | C の値 |
| --- | --- | --- |
| extern の引数 | `Maybe<H>` | `None` → NULL、`Some h` → `h`（`h` を move する） |
| extern の引数 | `Maybe<ref H>` | `None` → NULL、`Some (ref h)` → `h` の値（借用。`ref H` と同じく、ポインターのポインターにしない） |
| extern の引数 | `Maybe<ref R>`（R はスカラー record） | `None` → NULL、`Some (ref r)` → `r` を正規化した一時領域へのポインター（`ref R` と同じ一時領域） |
| extern の結果 | `Maybe<H>` | NULL → `None`、それ以外 → `Some h`（呼び出し側が所有する） |
| コールバックの引数 | `Maybe<H>`・`Maybe<ref H>` | NULL → `None`、それ以外 → `Some`（`ref H` はコールバックの間だけ借りる） |

- `Maybe` は std の `Maybe` だけ。同じ形の利用者の共用体（`union Opt<'a> = None \| Some of 'a`）は今どおり `E1008`（今の文のまま）。
- 置けないもの（`E1008`、kind `abi_nullable_payload`）: `Maybe<i64>` などスカラーの `Maybe`、値の `Maybe<R>`、`Maybe<string>`・`Maybe<ref [T]>`（NUL 終端の文字列は W20 の `ZString`）、入れ子の `Maybe<Maybe<H>>`、`Maybe<ref R>` の結果、コールバックの結果の `Maybe<H>`（コールバックの結果は今どおりスカラーか `unit`）、`ref Maybe<H>`。
- `export def` の引数と結果の `Maybe` は今どおり `E1008`（`UnsupportedExportSignature`。D3）。
- `Maybe<ref ZString>` は W20 の型ができてから使える（D7）。

### 2. 所有権と借用

- `Maybe<H>` を値で渡すと、`Some h` の `h` は move される（その後の使用は `E1012`）。親の窓のように渡した後も使うハンドルは `Maybe<ref H>` で渡す（`Some (ref parent)`）。W18 の `extern type copy` のハンドルなら `Maybe<H>` は複製される（生成は同じ）。
- `Maybe<ref H>`・`Maybe<ref R>` は呼び出しの間だけの共有借用で、ホストは呼び出しが返った後にポインターを持ち続けてはならない（今の `ref` の規則と同じ）。
- 引数は今どおり左から右に一度だけ評価する。`Maybe<ref R>` の一時領域への複写は全引数の評価の後、呼び出しの直前に行う（今の `ref R` と同じ順序）。

### 3. C のヘッダー

明示の symbol のない `extern def` と、そのコールバックのプロトタイプでは、`Maybe<H>`・`Maybe<ref H>` は `H` と同じ `tz_handle_...`、`Maybe<ref R>` は `const tz_record_... *` になる。C のポインターはもともと NULL を取れるので、型の綴りは変えない。`Maybe` の中にだけ現れるハンドルと record にも `typedef` と型の定義を出す。明示の symbol の extern はこれまでどおりプロトタイプを出さない。

### 4. native と WASM

| 項目 | native | wasm32・wasm64 |
| --- | --- | --- |
| 宣言（`check`） | 受理 | 受理（`check` は target を知らない） |
| 到達した extern・コールバック | 上の変換 | `E1008`（kind `abi_native_only`）、extern の宣言の名前の位置 |
| 到達しない extern | IR に出ない | IR に出ない（エラーにしない） |

wasm を拒否する理由は D2。JavaScript の糊（`--emit bindings-js`）も同じ `E1008` で止まる。

### 5. 診断

| コード | kind（案） | 条件 | メッセージ（案） | 位置 |
| --- | --- | --- | --- | --- |
| E1008 | `abi_nullable_payload` | std の `Maybe` を ABI に置いたが、1 の表にない形 | `'Maybe' at the C ABI needs an extern type or 'ref' to one (or, as an extern parameter, 'ref' to a scalar record); None passes a null pointer` | extern の名前 |
| E1008 | `abi_native_only` | wasm の build で、`Maybe` を境界に置く extern かコールバックの呼び出しを生成した | `'Maybe' at the C ABI passes None as a null pointer, which only native targets support; build for a native target` | extern の名前 |

`abi_native_only` は W13・W16・W20 などと共有する kind で、先に入ったチケットが G23 の登録簿（`src/diagnostic_kinds.rs`）へ足す。文は機能ごとに変えてよい（kind は 1 つ）。`Maybe` を使わない宣言の誤りの文（`UnsupportedExternSignature`・`UnsupportedExportSignature`・コールバックの文）は 1 byte も変えない。

## 設計

### 型の文脈（E12 の見送りの理由への答え）

`src/abi.rs` に、std の `Maybe` を境界の形として認める関数を 1 つ足し、検査と生成の両方がそれだけを呼ぶ。

```rust
/// W15: a std `Maybe<P>` whose `None` is the C null pointer at the ABI.
pub enum Nullable<'a> {
    Handle(&'a Type),          // Maybe<H>
    BorrowedHandle(&'a Type),  // Maybe<ref H>
    BorrowedRecord(&'a Type),  // Maybe<ref R>, R a scalar record
}
pub fn nullable<'a>(ty: &'a Type, types: &TypeContext<'_>) -> Option<Nullable<'a>>;
/// The type the C side sees: `Maybe<H>` → `H`, `Maybe<ref H>` → `ref H`, `Maybe<ref R>` → `ref R`,
/// any other type itself.
pub fn c_view<'a>(ty: &'a Type, types: &TypeContext<'_>) -> Cow<'a, Type>;
```

- extern の側の述語を 3 つ足す: `extern_scalar(ty, types)`（`abi_scalar` か `Maybe<H>`・`Maybe<ref H>`。1 つの機械のスカラー）、`extern_parameter(ty, types)`（`parameter` か `extern_scalar` か `Maybe<ref R>`。W13 が同じ名前で足す関数で、先に入ったほうが作り、後のほうが形を足す）、`extern_result(ty, types)`（`result` か `Maybe<H>`）。`callback(ty, types)` は `types` を取り、引数を `extern_scalar` で見る（呼び出しは `check_modules_collect` の 1 か所）。
- `abi_scalar`・`parameter`・`result` は変えない。export の検査（`UnsupportedExportSignature`）と `extended` はこれらを使い続けるので、export は `Maybe` を今の文で拒否し続ける（D3）。
- `abi_type`・`c_type` は文脈を取らないまま残す。module を持つ呼び出し側（`host_call`・`wrapper`・`c_parameters`・`header`）が先に `c_view` を通すので、`Maybe` がそこへ届くことはない（約 20 か所の呼び出しに module を通さない）。
- 検査の順序: extern の引数・結果に std の `Maybe` があり `nullable` が `None` なら、汎用の `UnsupportedExternSignature` より先に `abi_nullable_payload` を出す。`Maybe` を含まない宣言は今の経路のまま（文は不変）。

### 生成

- **extern の引数**（`host_call`）: `Maybe<H>` は `tag_value` と `payload_value` で `Some` のときのハンドルを取り、`select i1 %some, ptr %h, ptr null`。`Maybe<ref H>` は内部が 1 つの `ptr`（null か slot へのポインター）なので、null を読まないように、null を入れた slot と `select` してから `load ptr` する。`Maybe<ref R>` は null と比べて分岐し、`Some` の側でだけ `load` → `write_host_record` → 一時領域のポインター、`None` の側は null、合流で `phi ptr`。一時領域は今の `ref R` と同じく `host_result_slot`（関数の先頭の `alloca`）。
- **extern の結果**: C の `ptr` を受けて `icmp eq ptr %r, null`、`nullary_value(ty, "None")` と `construct_value(ty, some, Some((%r, "ptr")))` を作って `select`（`src/llvm_sync.rs` の compare-exchange と同じ形。タグを書くのは `construct_value` だけ、PM01）。
- **コールバックの引数**（`wrapper`）: `ptr %argN` を null と比べ、`Maybe<H>` は結果と同じ `construct_value` の 2 つと `select`、`Maybe<ref H>` は `%argN` を `spill` した slot と null の `select`（`Some` のときだけ意味を持つ）。
- **ヘッダーと型の定義**: `c_parameters` と `header` は `c_view` で `H`・`ref H`・`ref R` に直してから今の分岐へ入る。`handle_typedefs` と `record_types` の `visit` は、引数と結果の型を `c_view` で直してから辿る。
- **ABI の補助の要不要**: `uses_host_abi` の extern の側の判定を `extern_scalar(ty, types)` にするので、`Maybe<H>`・`Maybe<ref H>` だけの extern では `%tz.abi.buffer` も `tsuzuri_alloc` の export も出ない（E12 の落とし穴と同じ理由）。`Maybe<ref R>` は今の `ref R` と同じく record の型の定義が要る。
- **wasm の拒否**: `host_call` と `wrapper` は、`self.globals.wasm` で `Maybe` を境界に置く型に出会ったら、変換の IR を作らずに（wasm の `Maybe<ref R>` は niche を持たないので native の下げ方を当てはめない）`Globals` に extern（またはコールバックを取る extern）の関数の番号を記録する。`emit_program` は関数の生成の後、記録があれば最も小さい番号の extern の宣言の位置に `E1008`（`abi_native_only`）を返す。到達は「生成したか」で決まるので新しい根の規則は要らない（S2）。
- **`--backend fast`**: 新しい IR は `icmp`・`select`・`phi`・`load`・`br` と既存の補助だけで、集成値の `select` は `llvm_sync.rs` の前例がある。訳せない構文があれば PB05 の規則どおり `E2002` になること（意味を変えないこと）をテストで見る。

## 前提とする他チケットのインターフェース

- なし（単独で出荷できる）。次は、先に入っていれば合わせる。
- W14 が先なら: W14 の `abi::c_signature` に、`Maybe<H>`・`Maybe<ref H>`・`Maybe<ref R>` を `CType::Ptr` として足す（`c_view` の結果を分類すればよい。W14 の「他チケットへ提供するインターフェース」のとおり、後から入るチケットが自分の形を足す）。
- W18 が先なら: `copy` のハンドルの `Maybe<H>` は move でなく複製になる。生成は同じ。
- W20 が先なら: `Nullable::BorrowedZString` を足し、`Maybe<ref ZString>` の引数を `None` → NULL、`Some` → W20 の `const uint16_t *` にする（D7）。

## 他チケットへ提供するインターフェース

- `abi::Nullable`・`abi::nullable(ty, types)`・`abi::c_view(ty, types)`・`abi::extern_scalar`・`abi::extern_parameter`・`abi::extern_result`（W14 の分類、W21 の field、W22 の `FnPtr` の引数の型、W27 の生成器の `[Optional]`）。
- `FunctionEmitter::nullable_to_c(ty, value) -> String`（`Maybe` の値から C の `ptr`）と `nullable_from_c(ty, pointer) -> String`（C の `ptr` から `Maybe<H>` の値）。W21 の `@c_layout` の `Maybe<H>` の field の読み書きと W24 はこれを使う。
- 診断の kind `abi_native_only`・`abi_nullable_payload`。

## 実装手順

### 手順 1: ベースライン

- 変更: なし（`target/scratch/w15/` の下書きだけ）。
- 内容: `node scripts/repo-kb.mjs brief W15`。「現状」の確かめ方のコマンド。`tests/fixtures/*` と `examples/*` の `Main.tz` を `--emit llvm`（native と `--target wasm32`）と `--emit header` にして保存する（比較の基準）。`Builtin::ALL` などの共有の数は今の値を控える（GUIDE-LITE §11）。
- 確認: 保存した件数を記録する。

### 手順 2: 判定と検査

- 変更: `src/abi.rs`（`Nullable`・`nullable`・`c_view`・`extern_scalar`・`extern_parameter`・`extern_result`、`callback` の引数）、`src/check.rs`（`check_modules_collect` の extern の検査、kind）、`src/llvm_abi.rs`（`uses_host_abi` の extern の側）、`src/diagnostic_kinds.rs`。
- 内容: 仕様 1・5。この段では生成がまだないので、受理したプログラムの生成は手順 3 まで `tests/abi_nullable.rs` で `analyze` だけを見る。
- 確認: T0。`nullable_positions_are_accepted`・`other_maybe_payloads_keep_their_errors` が成功する。T1。

### 手順 3: extern の引数と結果

- 変更: `src/llvm_imports.rs`（`host_call`）、`src/llvm.rs`（`Globals`、`emit_program` の wasm の拒否）、`src/llvm_abi.rs`（`nullable_to_c`・`nullable_from_c`）。
- 内容: 設計の「生成」の extern の引数・結果と wasm の拒否。`Maybe<ref R>` の分岐は `Some` の側でだけ一時領域へ複写する。
- 確認: `nullable_arguments_lower_to_null_or_the_value` と `nullable_results_test_for_null`（IR の形と決定性）、`wasm_rejects_reached_nullable_externs_only`。T1。

### 手順 4: コールバックとヘッダー

- 変更: `src/llvm_abi.rs`（`wrapper`・`c_parameters`・`handle_typedefs`・`record_types`）、`src/llvm_imports.rs`（`header`）。
- 内容: 設計の「生成」のコールバックの引数と、ヘッダーと型の定義（`c_view` を通してから今の分岐へ入る）。
- 確認: `nullable_callback_parameters_decode_null` と `headers_name_handles_and_records_inside_maybe`。手順 1 の基準と、この機能を使わない入力の IR とヘッダーが `cmp` で一致する。T1。

### 手順 5: E2E

- 変更: `tests/fixtures/abi_nullable/Main.tz`、`tests/ffi_extensions_host.c`、`tests/ffi_extensions.mjs`、`tests/windows.mjs`。
- 内容: 「テスト計画」の E2E（ホストの C の期待値は手で書く）。
- 確認: 「テスト計画」の E2E が native の `-O0`・`-O3` で成功し、`live == 0`。wasm32・wasm64 の `build` が `E1008`。

### 手順 6: 文書と最終確認

- 変更: 「ドキュメント」の節のページ。
- 内容: 動作が固まったところで文書を一度に直し、`node scripts/gen-docs.mjs` で診断の表を作り直す。
- 確認: `node scripts/check-docs.mjs <変更したページ>`、T2。PR に `ci:full` を付ける（Windows の挙動を変える）。

## テスト計画

### Rust テスト（`tests/abi_nullable.rs`、新規）

- `nullable_positions_are_accepted`: 仕様 1 の 6 つの形（引数 3、結果 1、コールバックの引数 2）。複数のモジュールにまたがる `extern type` を含む。
- `other_maybe_payloads_keep_their_errors`: `Maybe<i64>`・値の `Maybe<R>`・`Maybe<string>`・`Maybe<ref [i64]>`・`Maybe<Maybe<H>>`・`Maybe<ref R>` の結果・コールバックの結果の `Maybe<H>`・コールバックの引数の `Maybe<ref R>` が `E1008` の `abi_nullable_payload`。利用者の `Opt<H>` と `export def` の `Maybe<H>` は今の文のまま（文を今の文字列と比べる。kind も今のまま）。
- `nullable_arguments_lower_to_null_or_the_value`: `Maybe<H>` の引数の IR に `select i1 ..., ptr ..., ptr null` があり、`Maybe<ref R>` の `None` の経路が `write_host_record` の複写を通らない。`llvm::emit` を 2 回して同じ。
- `nullable_results_test_for_null`: 結果の `icmp eq ptr ..., null` と `select`。
- `nullable_callback_parameters_decode_null`: `tz.callback.*` の引数が `ptr` で、本体へ渡す前に null と比べる。
- `headers_name_handles_and_records_inside_maybe`: `Maybe` の中にだけ現れるハンドルの `typedef` が 1 回、`Maybe<ref R>` の引数が `const tz_record_... *`。
- `wasm_rejects_reached_nullable_externs_only`: `llvm::emit_target(.., true)` が、到達する extern では `E1008` の `abi_native_only`（位置は extern の名前）、到達しない extern だけなら成功する。
- 既存の `tests/ffi_extensions.rs`・`tests/host_abi.rs`・`tests/host_imports.rs` は変えずに成功する。

### E2E

- `tests/ffi_extensions.mjs` に段を足す。ホストの C（`tests/ffi_extensions_host.c`）: `int32_t w15_is_null(void *p)`（NULL なら 1）、`void *w15_maybe(int32_t k)`（`k == 0` なら NULL、それ以外は静的な番地）、`int32_t w15_width(const struct ... *r)`（NULL なら -1、それ以外は `right - left`）、コールバックを NULL と非 NULL で 1 回ずつ呼ぶ `int32_t w15_visit(int32_t (*f)(void *), void *h)`。期待値は C の側で手で書いた値（GUIDE-LITE §2 規則 10）。native の `-O0`・`-O3`、object と IR のリンクの両方、`live == 0`。
- 同じ fixture を `--target wasm32`・`wasm64` で `build` すると `E1008`、`--emit bindings-js` も `E1008`、`check` は成功する。
- `tests/windows.mjs`: 早見のプログラム（kernel32 の `GetModuleHandleW`・`TlsAlloc`・`TlsGetValue`）が MSVC と同梱の MinGW の両方で終了コード 3。`--backend fast` でも 3 か `E2002`。

### 比較

- 手順 1 の基準と、この機能を使わない全入力の `--emit llvm`（native・wasm32）と `--emit header` が byte 単位で一致する（TZ05 の「実装と検証」の比べ方）。

## ドキュメント

- `docs/language.md` の「ホスト関数のインポート」（引数と戻り値に許す型の列挙）、「不透明ハンドル」（ABI の位置の段落と「ABI 境界以外」の文）、「コールバック」（引数の型）。
- `_tsuzuri/language-reference/compiler/native-interop.md` の「不透明ハンドル」に「NULL を渡す・受ける」の小節と例（早見のプログラム）、「静的コールバック」の引数の型。`H` の結果は NULL でも `H` のままであること、`Maybe<ref H>` で渡せば move しないこと。
- `_tsuzuri/language-reference/built-in-types-and-modules/maybe.md` に、C ABI では `None` が NULL になること（native だけ）と native-interop.md へのリンク。
- `_tsuzuri/language-reference/compiler/webassembly.md` の ABI の表の後に、`Maybe` を境界に置く extern は wasm で `E1008` であること。
- `compiler/diagnostics.md` の生成された表（`node scripts/gen-docs.mjs`）。`node scripts/check-docs.mjs` を変えたページに通す。
- README の機能一覧は、書いてあることが誤りになるときだけ直す。

## 受け入れ条件

- [ ] 仕様 1 の 6 つの形が native で動き、`live == 0`（`-O0`・`-O3`、MSVC・MinGW・Linux・macOS）。
- [ ] 仕様 1 で置けない形が `E1008` の `abi_nullable_payload` で、`Maybe` を含まない宣言の診断は文も kind も変わらない。
- [ ] wasm32・wasm64 は到達した extern だけ `E1008` の `abi_native_only`。`check` は通る。
- [ ] `Maybe` の内部の配置と、この機能を使わないプログラムの IR・object・ヘッダー・診断が byte 単位で変わらない（比べた件数を記録した）。
- [ ] `abi_type`・`c_type` に `Maybe` の腕も module の引数も足していない。
- [ ] 文書を直し、`check-docs` が通る。T2 が成功し、PR に `ci:full` を付けた。

## 落とし穴

- `Maybe<ref H>` の内部は native では 1 つの `ptr`（slot へのポインター）で、`Maybe<H>` はタグ付きの共用体である。同じ「`Maybe` のハンドル」でも下げ方が違う。タグの位置や番号を手で書かない（`tag_value`・`construct_value`・`nullary_value` を通す）。
- `Maybe<ref R>` の `Some` の指す先は内部の配置の record で、C に渡すのは正規化した一時領域である。内部のポインターをそのまま渡すと、狭い整数の広げ方と field の順序が C と合わない。
- `None` の経路で null を `load` しない。`Maybe<ref H>` と `Maybe<ref R>` は分岐か、null を入れた slot との `select` を通す。
- `handle_typedefs`・`record_types` を直し忘れると、ヘッダーに `typedef` がない、IR に `%tz_record_...` の定義がない、という形で壊れる（`Maybe` の中だけに現れる型）。
- `uses_host_abi` を `Maybe<H>` で真にすると、ハンドルだけのプログラムに `tsuzuri_alloc` の export と `%tz.abi.buffer` が出て IR が変わる。
- `abi_scalar`・`parameter`・`result` は export の検査と `extended` も使う。そこへ `Maybe` を足すと export が `Maybe` を受理し、E13 の 4 つの生成器が知らない型に出会う。`Maybe` は extern の側の 3 つの述語だけに足す。
- `Some h` の `h` が NULL のこともある（NULL を返した `H` の結果を包んだもの）。境界では NULL のまま渡す。それを `None` と同じにしたければ利用者が結果を `Maybe<H>` と宣言する。

## 対象外

- `export def` の `Maybe`（D3）と、E13 の C#・Python・C++・JavaScript の生成器の対応。
- `INVALID_HANDLE_VALUE`（-1）のような NULL 以外の印（W27 が win32metadata の `[InvalidHandleValue]` をどう写すかを決める）。
- NUL 終端の文字列の `Maybe`（W20 の後。D7）、値の record・バッファの `Maybe`。
- `@c_layout` の field の `Maybe<H>`（W21 が `nullable_to_c`・`nullable_from_c` で行う）。
- `ref mut Maybe<H>`（ホストが書く `H *` の出力）。W13 の copy-in/copy-out の上に、W21 か後のチケットが足す。
- `tsuzuri bindgen` が NULL を取りうるポインターを `Maybe` にすること（LP64 だけの生成器で、W27 が Win32 の生成器を作る）。
- `Maybe` の内部の配置の最適化（ハンドルの niche）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | extern の引数の `Maybe<H>`・`Maybe<ref H>`・`Maybe<ref R>`、結果の `Maybe<H>`、コールバックの引数の `Maybe<H>`・`Maybe<ref H>` を、`None` ⇔ NULL の境界の変換として許す。E12 の対象外の「`Option<H>` と NULL の対応」を解除する | NULL を取る・返す API を `i64` の宣言なしで書ける。値を参照外ししないので D-63 の X の範囲（生ポインター型を入れない）の内にある。`Maybe<ref H>` を足すのは、Copy でないハンドルを `Some h` で move させないため（W18 の前でも親の窓を渡せる） | 要承認（承認前は手順 2 以降に着手しない） |
| D2 | native だけ。wasm32・wasm64 は到達した extern・コールバックだけを `E1008`（`abi_native_only`）にし、宣言だけなら拒否しない | wasm32 のハンドルは JS の番号で 0 を有効な値に使いうる（B07 の判断 6）。`Maybe<ref R>` の内部も wasm では niche を持たない。到達だけで拒否すれば、Windows 向けの宣言を持つパッケージを wasm のプログラムが読み込める | 既定案（実装者はこの案に従う） |
| D3 | `export def` の `Maybe` は今どおり `E1008` | E13 の 4 つの生成器（C#・Python・C++・JS）が対応を要し、Win32 の用途（Tsuzuri が呼ぶ側）には要らない。後で足すときは生成器と一緒に | 既定案（実装者はこの案に従う） |
| D4 | 型の文脈は、判定の関数 `abi::nullable` 1 つを検査と生成の両方で使い、extern の側の述語（`extern_scalar`・`extern_parameter`・`extern_result`）と `callback` にだけ `types` を通す。`abi_type`・`c_type` には `abi::c_view` で直した型を渡す | E12 が恐れた「すべての判定に文脈を通す」変更は、extern の側の 3 つの述語と `callback` で済み、export と共有する `abi_scalar`・`parameter`・`result` は変えない。`abi_type`・`c_type` の約 20 か所（多くは record の field）に module を通すより狭い。採らなかった案: (a) 位置ごとの ABI の種類を `HostImport` に保存する（コールバックの wrapper は利用者の関数の型から作るので、保存する場所がもう 1 つ要る）、(b) `Maybe<H>` の内部の配置に NULL の niche を足す（NULL を返した `H` を包んだ `Some` が `None` になる。wasm の 0 も同じ問題） | 既定案（実装者はこの案に従う） |
| D5 | `H` の結果は NULL でも `H` のまま。NULL を区別するのは `Maybe<H>` と宣言した結果だけ | 今の宣言の意味を変えない（GUIDE §1 規則 2 の暗黙の変換をしない） | 既定案（実装者はこの案に従う） |
| D6 | 新しい誤りは E1008 の新しい kind（`abi_nullable_payload`・`abi_native_only`）。`Maybe` を含まない宣言の文と kind は変えない | E12 D12・D-51 の前例。今の利用者の診断の出力を変えない | 既定案（実装者はこの案に従う） |
| D7 | `Maybe<ref ZString>` は、W15 と W20 のうち後に入るほうが `Nullable` に腕を足して有効にする | 共通契約の W15 の行は `Maybe<ref ZString>` を挙げるが、W15 は W20 に依存しない。D-53 の「先に入ったチケットが口を作り、後のチケットが足す」と同じ扱い | 既定案（実装者はこの案に従う） |
| D8 | `INVALID_HANDLE_VALUE` など NULL 以外の印は扱わない | 印は API ごとに違い、Win32 では win32metadata が型ごとに持つ（W27 で決める） | 既定案（実装者はこの案に従う） |
