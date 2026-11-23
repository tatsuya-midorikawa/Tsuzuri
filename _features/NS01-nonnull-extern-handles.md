# NS01: 不透明ハンドル extern type の非 Null 不変条件と Maybe<H> の NULL niche

| 項目 | 内容 |
| --- | --- |
| ID | NS01 |
| 優先度 | P1（非 `Maybe` の `extern type` に `NULL` が混入すると Tsuzuri 側で判定できないまま後続の C 呼び出しで未定義動作を起こし、[W15](W15-abi-nullable-handles.md) でも `Some(NULL)` が生じる） |
| 規模 | M（境界の非 Null ガード 3 か所、`Niche::NullPointer` の拡張 1 か所、W15 の境界変換の簡素化、生成バインディング 3 言語のガード、テストと文書） |
| 依存 | W15 |
| 後続 | [NS02](NS02-export-and-bindings-nullable.md), [NS03](NS03-bindgen-nullability.md) |
| 状態 | todo |
| 起票 | 2026-11-22（[GUIDE D-64](GUIDE.md#d-64-null-安全性の徹底ns01ns08)。Null 安全性の調査報告に対する改善チケット作成依頼から起票。HEAD `edc61676` で確認） |
| 承認 | 要承認（[E12](_completed/E12-ffi-extensions.md) と [W15](W15-abi-nullable-handles.md) D4・D5 の改訂。決定事項 D1・D2 が要承認、D3〜D6 は既定案） |
| 手本にする既存実装 | `src/llvm_abi.rs` の `host_pointer`（レコード・バッファの入口で null を検査してトラップする形）と `wrapper`、`src/llvm_imports.rs` の `host_call`、`src/llvm_layout.rs` の `Niche::NullPointer`（native のみ有効、WASM は対象外）、`src/bindings_native.rs` の UTF-8 検査ガード（呼び出し前にホスト側の例外を送出する形）、[W25](W25-extern-interface-com.md) 仕様 4（インターフェース値が決して NULL でない不変条件） |
| 主な影響ファイル | `src/llvm_imports.rs`, `src/llvm_abi.rs`, `src/llvm_layout.rs`, `src/trap.rs`, `src/bindings_native.rs`, `tests/abi_nullable.rs`, `tests/host_bindings_native.mjs`, `tests/ffi_extensions.mjs`, `tests/ffi_extensions_host.c`, `docs/language.md`, `docs/architecture.md`, `_tsuzuri/language-reference/compiler/native-interop.md`, `_tsuzuri/language-reference/built-in-types-and-modules/maybe.md`, `_tsuzuri/language-reference/built-in-types-and-modules/union.md` |

## 実装者向け早見

- 範囲: ネイティブターゲットにおいて、不透明ハンドル `extern type H`（および `ref H`）の値は**常に非 Null ポインタ（`ptr != null`）**であるという不変条件を確立する。非 `Maybe` の `H` を返す `extern def` が `NULL` を返したとき、および `export def`・コールバックの非 `Maybe` 引数 `H`・`ref H` にホストが `NULL` を渡したときは、境界で即座にトラップする。C#・Python・C++ の生成バインディングも非 `Maybe` のハンドルに対する `0` / `None` / `nullptr` を例外で拒否する。この不変条件の上で、ネイティブの `Maybe<H>` に `Niche::NullPointer`（タグなし 1 ポインタ、`None` = `null`）を与える。WASM は対象外（wasm32 のハンドルは JS 側の整数番号で `0` が有効値になり得るため、現行どおり）。
- 最初に読む: ① 仕様 1〜4 ② `src/llvm_abi.rs` の `wrapper`（497〜620 行）と `host_pointer`（656〜695 行）③ `src/llvm_imports.rs` の `host_call`（31〜195 行）④ `src/llvm_layout.rs` の `Niche`（15〜35 行）と `niche_layout` ⑤ [W15](W15-abi-nullable-handles.md) の D4・D5・落とし穴。
- 最初に書くテスト: `tests/abi_nullable.rs` の `nonnull_handle_result_traps_on_null`・`nonnull_handle_export_and_callback_trap_on_null`・`maybe_handle_uses_null_pointer_niche_on_native`。
- 触ってはいけない: WASM の `H` と `Maybe<H>` の表現（タグ付きのまま）、`Maybe` も `extern type` も使わないプログラムの IR・ヘッダー・診断（1 byte も変えない）。
- 完了の見分け方: ネイティブで `extern "null_source" def bad :: unit -> H` が C から `NULL` を返された瞬間に `null extern handle` でトラップし、`Maybe<H>` の LLVM 型が `ptr` 1 つ（8 バイト）になり、`Some h` が決して `NULL` を持たない。

## 目的

現行仕様（`docs/language.md` 536 行）では `extern type` の値について「NULL は区別できず、NULL の扱いは C の API の契約に従います」としており、`src/llvm_imports.rs` にも `src/llvm_abi.rs` にも NULL 検査がない。そのため C 関数が失敗時に `NULL` を返しても Tsuzuri 側では正常な `H` として受け取り、`H` には `Eq` も `NULL` 判定手段もないまま後続の `extern def` に渡して C ホスト側で NULL ポインタ逆参照（未定義動作）を起こす。

さらに [W15](W15-abi-nullable-handles.md) は `Maybe<H>` で `None` ⇔ `NULL` を導入するが、D5 で「非 `Maybe` の `H` の結果は `NULL` でも `H` のまま」を残しているため、誤って `NULL` を受け取った `h: H` を `Some h : Maybe<H>` に包むと、**Tsuzuri の `match` では `Some` なのに C ABI へ渡すと `NULL`（＝`None`）になる**という `Some(NULL)` の破綻が生じる（W15「落とし穴」7 項）。また、`Some(NULL)` があり得るせいで `Maybe<H>` に `Niche::NullPointer` を使えず 16 バイトのタグ付き共用体のままになる。

本チケットは Rust の `NonNull<T>` / `Option<NonNull<T>>` および [W25](W25-extern-interface-com.md) の COM インターフェースと同じく、「`H` は非 Null、NULL になり得る境界は `Maybe<H>`」という不変条件を境界検査で強制し、Null 安全性の穴と表現コストを同時に解消する。

## 着手条件と停止条件

### 着手条件

- [W15](W15-abi-nullable-handles.md) が `done` であること。
- 決定事項 D1・D2 の承認があること（承認前は手順 2 以降に着手しない）。
- `node scripts/repo-kb.mjs brief NS01` を読み、着手時の `TrapKind::ALL` の要素数（起票時 18）を控える（GUIDE-LITE §11）。

### 停止条件

- **S1** `extern type` を持たないプログラムの IR・ヘッダー・診断が 1 byte でも変わる。
- **S2** WASM の `H`（番号 `0` を有効なハンドルとして使う既存のテスト `tests/host_imports.rs`・`tests/bindings_js.mjs`）が失敗する。
- **S3** [W18](_completed/W18-copy-extern-type.md)（`extern type copy`）が先に入っている場合で、整数識別子をポインタ型に載せる Win32 API（例: `HMENU` にメニュー ID を入れる用法）と非 Null 検査が衝突する場合は、即興で緩めず対象 API を記録して報告する。
- **S4** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `edc61676` で確認）

- **`host_call`（`src/llvm_imports.rs` 108〜195 行）**: `Type::Handle` の引数はそのまま `ptr`（`ref H` は `load ptr`）で渡し、結果の `Type::Handle` は `call ptr @symbol(...)` の戻り値を無検査でそのまま返す。
- **`wrapper`（`src/llvm_abi.rs` 569〜588 行）**: レコード引数は `emitter.host_pointer(..., false)`、バッファ引数は `emitter.host_pointer(..., true)` で null と整列を検査してトラップするが、`Type::Handle` と `Type::Reference(Type::Handle, false)` は `decode_host_scalar` の `_ => value.into()` を通るだけで null 検査がない。
- **`niche_layout`（`src/llvm_layout.rs` 15〜35 行）**: `Niche::NullPointer` は「スライスでない `ref T`（native のみ）」だけに適用され、`Type::Handle` は対象外のため `Maybe<H>` は `{ i8, ptr }`（16 バイト）になる。
- **生成バインディング（`src/bindings_native.rs`）**:
  - C#（651〜682 行）: `readonly record struct Handle(nint Value)` で、`default(Handle)`（`Value == 0`）をそのまま渡せる。戻り値も `value == 0` を検査しない。
  - Python（1228〜1302 行）: 引数は `isinstance(arg, Handle)` だけを見て `arg.value`（`0` や `None` になり得る）を `_ctypes.c_void_p` に渡し、戻り値は `return Handle(raw or 0)` として `ctypes` が `NULL` に対して返した `None` を黙って `0` に変換している。
  - C++（1614〜1656 行）: C ヘッダーの `typedef struct tz_handle_..._s *tz_handle_...;` をそのまま使うため、`nullptr` を渡せる。

## 変えてはいけない意味

- WASM（`wasm32`・`wasm64`）における `extern type` の表現（`i32` / `i64`）と動作（`0` を有効なハンドル番号として渡せること。B07 の判断 6）。
- `extern type` を使わないプログラムの IR・object・ヘッダー・バインディング・診断。
- `Maybe<H>` のソース上の型検査・所有権移動・網羅性検査の意味。

## 仕様

### 1. 非 Null 不変条件と境界トラップ（native）

ネイティブターゲットにおいて、`Type::Handle`（`H`）および `ref H` の値は常に非 Null（`ptr != null`）とする。

| 境界の向き | 型 | C / ホストが `NULL` を渡した・返したとき |
| --- | --- | --- |
| `extern def` の結果 | `H` | 呼び出し直後に `TrapKind::NullHandle`（メッセージ `null extern handle`）でトラップ。位置はその `extern` 呼び出し式（直接呼び出しでない関数値経由は `extern` 宣言） |
| `export def` の引数 | `H`・`ref H` | 本体に入る前に `TrapKind::NullHandle` でトラップ（`--trap-mode return` ではステータス 1 と `trap->kind` でホストへ返る） |
| 静的コールバックの引数 | `H`・`ref H` | 本体に入る前に `TrapKind::NullHandle` でトラップ |
| 上記すべて（W15・NS02 対応位置） | `Maybe<H>`・`Maybe<ref H>` | トラップせず `None` になる |

- Tsuzuri からホストへ `H`・`ref H` を渡す向き（`extern def` の引数、`export def` の結果）は、すべての生成元で非 Null が保証されているため追加の実行時検査を行わない。

### 2. `Maybe<H>`（および同じ形の 2 ケース共用体）の NULL niche（native）

- `src/llvm_layout.rs` の `niche_layout` の対象（2 ケースで片方がペイロードなし、もう片方が単一ペイロード、`Drop` なし、非再帰）に、**native（`!webassembly()`）の `Type::Handle`** を追加し、`Niche::NullPointer`（LLVM 型 `ptr`、`None` = `null`、`Some h` = `h`）とする。
- これにより、W15 が導入した native の `Maybe<H>` の境界変換は、`None` ⇔ `null`・`Some h` ⇔ `h` のビット表現が内部と C ABI で完全に一致するため、分岐も `select` も不要な恒等変換（`ptr` をそのまま渡す・受ける）になる。
- WASM では `webassembly()` が真のため従来どおりタグ付き共用体のままとし、境界の `Maybe<H>` は W15 D2 どおり `E1008`（`abi_native_only`）のまま拒否する。

### 3. 生成バインディング（C#・Python・C++）の非 Null 検査

`--emit bindings-cs`・`bindings-py`・`bindings-cpp` において、非 `Maybe` の `H` / `ref H` は呼び出し側・返却側で `0` / `None` / `nullptr` を拒否する。

| 言語 | 非 `Maybe` 引数に null / 0 を渡したとき | 非 `Maybe` 結果に C 側が NULL を返したとき（`--trap-mode` なしの外部破損への多段防御） |
| --- | --- | --- |
| C# | `if (arg.Value == 0) throw new ArgumentNullException(nameof(arg));` | `if (value == 0) throw new InvalidOperationException("...");` |
| Python | `Handle.__init__` と引数検査で `value is None or value == 0` を `ValueError` | `if raw is None: raise RuntimeError("...")`（既存の `raw or 0` を廃止） |
| C++ | `if (arg == nullptr) throw std::invalid_argument("...");` | `if (value == nullptr) throw std::runtime_error("...");` |

## 設計

- **`TrapKind::NullHandle`**: `src/trap.rs` の `TrapKind` 末尾に `NullHandle`（`description`: `"null extern handle"`）を追加し、`TrapKind::ALL` の長さを +1 する。`src/effects.rs` では `extern` 呼び出しはすでに `extern` 効果を持つ（`@no_trap` は `@trusted no_trap` がない限り `extern` を禁じる。`@trusted no_trap` を付けた `extern` は人間が非 Null 返却も保証したものとして扱う）。
- **`host_call`（`src/llvm_imports.rs`）**: `!self.globals.wasm && matches!(result, Type::Handle(..))` のとき、戻り値 `%v` に対して `%ok = icmp ne ptr %v, null` と `self.guard(&ok, TrapKind::NullHandle)` を出力する。
- **`wrapper`（`src/llvm_abi.rs`）**: `!emitter.globals.wasm` で引数が `Type::Handle(..)` または `Type::Reference(inner, false) if matches!(inner.as_ref(), Type::Handle(..))` のとき、`%ok = icmp ne ptr %argN, null` と `emitter.guard(&ok, TrapKind::NullHandle)` を `spill` や本体呼び出しの前に出力する。
- **`niche_layout`（`src/llvm_layout.rs`）**: `!webassembly()` の `Niche::NullPointer` の対象に `Type::Handle(..)` を加える。`W15` の `nullable_to_c` / `nullable_from_c` の `Nullable::Handle` 腕は、レイアウトが `Niche::NullPointer` なので値 `%v` そのもの（`ptr`）を返し、余分な `select` を出さない。

## 前提とする他チケットのインターフェース

- [W15](W15-abi-nullable-handles.md): `abi::Nullable`（`Handle`・`BorrowedHandle`・`BorrowedRecord`）、`nullable_to_c`・`nullable_from_c`、`tests/abi_nullable.rs`。
- [W18](_completed/W18-copy-extern-type.md)（先に入っている場合）: `Type::Handle(Box<str>, HandleKind)` のパターンに合わせる（`Owned`・`Copy` ともに同じ非 Null 規則）。

## 他チケットへ提供するインターフェース

- ネイティブにおける `Type::Handle` の非 Null 不変条件と `Maybe<H>` の単一 `ptr` レイアウト（[NS02](NS02-export-and-bindings-nullable.md) の `export def` 境界、[NS03](NS03-bindgen-nullability.md) の `bindgen` 生成型、[W21](W21-c-layout-records.md) の `@c_layout` フィールドの `Maybe<H>` がそのまま 8 バイト `ptr` として扱える）。
- `TrapKind::NullHandle`。

## 実装手順

1. **変更**: なし（ブランチ `feat/ns01-nonnull-extern-handles`）。**内容**: `node scripts/repo-kb.mjs brief NS01`、`TrapKind::ALL` の長さと `extern type` を使わない fixture 群の IR を保存。**確認**: 基準値を記録。
2. **変更**: `src/trap.rs`, `src/llvm_imports.rs`, `src/llvm_abi.rs`。**内容**: `TrapKind::NullHandle` と `extern` 結果・`export` 引数・コールバック引数の非 Null ガード。**確認**: T0、`nonnull_handle_result_traps_on_null`・`nonnull_handle_export_and_callback_trap_on_null`、T1。
3. **変更**: `src/llvm_layout.rs`, `src/llvm_abi.rs`（W15 の `Nullable::Handle` 変換）。**内容**: ネイティブの `Type::Handle` に `Niche::NullPointer` を与え、`Maybe<H>` の境界変換を恒等化する。**確認**: `maybe_handle_uses_null_pointer_niche_on_native`、WASM の既存テスト（`tests/host_imports.rs`）が通ること、T1。
4. **変更**: `src/bindings_native.rs`, `tests/host_bindings_native.mjs`。**内容**: C#・Python・C++ 生成バインディングの非 Null 検査と Python の `raw or 0` 廃止。**確認**: `node tests/host_bindings_native.mjs target/debug/tsuzuri`。
5. **変更**: `tests/ffi_extensions.mjs`, `tests/ffi_extensions_host.c`, 文書。**内容**: `-O0`・`-O3` と `--trap-mode return` での E2E 検証、仕様書とリファレンスの更新。**確認**: T2、`node scripts/check-docs.mjs <変更したページ>`。PR に `ci:full`。

## テスト計画

- `tests/abi_nullable.rs`:
  - `nonnull_handle_result_traps_on_null`: `extern` の `-> H` の IR に `icmp ne ptr ..., null` と `NullHandle` トラップがあること。WASM では出ないこと。
  - `nonnull_handle_export_and_callback_trap_on_null`: `export def` と `tz.callback.*` の `H`・`ref H` 引数に入口ガードがあること。
  - `maybe_handle_uses_null_pointer_niche_on_native`: ネイティブの `Maybe<H>` の型が `ptr`（タグなし）であり、WASM ではタグ付き構造体のままであること。
- `tests/ffi_extensions.mjs`: C ホストが `-> H` に `NULL` を返したとき・`tz_take(NULL)` を呼んだとき・コールバックに `NULL` を渡したときに `--trap-info` で `trap: null extern handle`（`--trap-mode return` ではステータス 1）になること。`Maybe<H>` では `None` として正常動作し `live == 0` であること。
- `tests/host_bindings_native.mjs`: C# で `default(Handle)`、Python で `Handle(0)` / `Handle(None)`、C++ で `nullptr` を渡すとホスト側例外になること。

## ドキュメント

- `docs/language.md`: 「不透明ハンドル」「公開 ABI」「共用体（union）」（niche の対象にネイティブの `extern type` を追記）。「NULL は区別できず」の記述を非 Null 不変条件と `Maybe<H>` の説明に改める。
- `_tsuzuri/language-reference/compiler/native-interop.md`、`_tsuzuri/language-reference/built-in-types-and-modules/maybe.md`、`_tsuzuri/language-reference/built-in-types-and-modules/union.md`。

## 受け入れ条件

- [ ] ネイティブで非 `Maybe` の `H` / `ref H` に `NULL` が入ろうとする全境界（`extern` 結果・`export` 引数・コールバック引数）が `null extern handle` でトラップする。
- [ ] ネイティブの `Maybe<H>` が `Niche::NullPointer`（`ptr` 1 つ）になり、`Some(NULL)` が作れない。
- [ ] WASM で番号 `0` のハンドルが従来どおり動作する。
- [ ] C#・Python・C++ バインディングが非 `Maybe` ハンドルの `0` / `None` / `nullptr` を拒否する。
- [ ] `extern type` を使わないプログラムの IR・ヘッダー・診断が byte 単位で一致する。

## 落とし穴

- WASM では `Niche::NullPointer` を `Type::Handle` にも有効にしてはならない（wasm32 のハンドルは `i32` で `0` が最初の有効スロットになり得る）。必ず `!webassembly()` の分岐内に置く。
- `Maybe<ref H>` は `ref H` がスロットへのポインタ（`ptr`）であり、`Niche::NullPointer` で `None` = スロットポインタが `null`、`Some` = スロットポインタが非 null かつスロットの中身の `H` も非 null という 2 段の非 Null になる。`Maybe<H>`（値自身が `ptr`）と混同しない。

## 対象外

- `INVALID_HANDLE_VALUE`（`-1`）など `NULL` 以外のセンチネル値（API ごとに異なるため [W27](W27-win32-package-from-winmd.md) で扱う）。
- `export def` での `Maybe<H>` 対応（[NS02](NS02-export-and-bindings-nullable.md)）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | ネイティブの `extern type H` と `ref H` を非 Null とし、非 `Maybe` の境界（`extern` 結果・`export` 引数・コールバック引数）での `NULL` を即座にトラップにする（[W15](W15-abi-nullable-handles.md) D5 を改訂） | `NULL` を返し得る C 関数は W15 で `Maybe<H>` と書けるようになるため、`H` に `NULL` を通す理由がなくなる。境界で止めれば C 側の未定義動作と `Some(NULL)` を根絶できる | 要承認 |
| D2 | ネイティブの `Type::Handle` を `Niche::NullPointer` の対象にする（[W15](W15-abi-nullable-handles.md) D4 の見送り理由を解消） | D1 により `H` が `null` を取り得なくなるため、`Some(NULL)` と `None` の衝突が消え、`Maybe<H>` を 8 バイト `ptr` にできる | 要承認 |
| D3 | トラップ種類は `TrapKind::NullHandle`（`"null extern handle"`）を新設する | `index out of bounds`（`BoundsCheck`）よりも FFI 宣言の非 Null 契約違反であることが一目で分かる | 既定案（実装者はこの案に従う） |
| D4 | WASM の `H` は従来どおり `0` を有効値として許し、検査も niche も入れない | JS 側のテーブル添字 `0` を壊さないため（B07 判断 6、W15 D2） | 既定案（実装者はこの案に従う） |
| D5 | C#・Python・C++ の生成バインディングも非 `Maybe` ハンドルの `0` / `None` / `nullptr` を呼び出し前の例外にする | トラップでプロセスを落とす前にホスト言語の型・引数エラーとして安全に報告するため（UTF-8 検証と同じ方針） | 既定案（実装者はこの案に従う） |
| D6 | `@trusted no_trap` を付けた `extern def` が `-> H` を返す場合、効果検査上は `trap` 効果を増やさない | `@trusted no_trap` は人間が「トラップ要因（NULL 返却を含む）がない」ことを保証した印であるため（E15） | 既定案（実装者はこの案に従う） |
