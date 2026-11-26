# NS02: export def とホストバインディング（C#・Python・C++）での Maybe<H>・Maybe<ref R>

| 項目 | 内容 |
| --- | --- |
| ID | NS02 |
| 優先度 | P2（公開 C ABI とホスト言語バインディングでオプション引数・欠損ハンドルを `0` や別フラグに頼らず型安全に受け渡す） |
| 規模 | M（`abi::parameter`・`result` の拡張、`wrapper` の入出力変換、`bindings_native.rs` の 3 言語対応、WASM の `abi_native_only` 検査、テストと文書） |
| 依存 | W15, NS01 |
| 後続 | –（弱い後続: [M11](M11-swift-bindings.md)。`Maybe` を Swift の Optional に写す） |
| 状態 | todo |
| 起票 | 2026-11-22（[GUIDE D-64](GUIDE.md#d-64-null-安全性の徹底ns01ns08)。Null 安全性の調査報告に対する改善チケット作成依頼から起票。HEAD `edc61676` で確認） |
| 承認 | 要承認（[W15](_completed/W15-abi-nullable-handles.md) D3 の解除と公開 ABI の拡張。決定事項 D1 が要承認、D2〜D5 は既定案） |
| 手本にする既存実装 | [W15](_completed/W15-abi-nullable-handles.md) の `abi::nullable`・`c_view` とコールバック引数のデコード、`src/llvm_abi.rs` の `wrapper`・`try_wrappers`・`try_prototype`、`src/bindings_native.rs` の `Shape` と C#・Python・C++ の入出力コード生成 |
| 主な影響ファイル | `src/abi.rs`, `src/check.rs`, `src/llvm.rs`, `src/llvm_abi.rs`, `src/bindings.rs`, `src/bindings_native.rs`, `tests/abi_nullable.rs`, `tests/host_bindings.mjs`, `docs/language.md`, `_tsuzuri/language-reference/compiler/native-interop.md`, `_tsuzuri/language-reference/built-in-types-and-modules/maybe.md`, `_tsuzuri/language-reference/compiler/webassembly.md` |

## 実装者向け早見

- 範囲: [W15](_completed/W15-abi-nullable-handles.md) が `extern def` とコールバック引数に限っていた NULL 境界変換を、ネイティブの `export def` の引数（`Maybe<H>`・`Maybe<ref H>`・`Maybe<ref R>`）と結果（`Maybe<H>`）に広げ、`--emit header`・`--trap-mode return`・`--emit bindings-cs`・`bindings-py`・`bindings-cpp` を対応させる。WASM（`wasm32`・`wasm64` および `--emit bindings-js`）では W15 D2 と同じく `E1008`（`abi_native_only`）で拒否する。
- 最初に読む: ① 仕様 1〜3 ② [W15](_completed/W15-abi-nullable-handles.md) と [NS01](_completed/NS01-nonnull-extern-handles.md) ③ `src/llvm_abi.rs` の `wrapper`・`try_prototype` ④ `src/bindings_native.rs` の `Shape`（140〜175 行）と 3 言語の生成部。
- 最初に書くテスト: `tests/abi_nullable.rs` の `export_nullable_positions_are_accepted` と `wasm_rejects_nullable_exports`。
- 触ってはいけない: `Maybe` を `export def` に使わないモジュールの IR・ヘッダー・バインディング出力、WASM の `bindings-js` の既存出力。
- 完了の見分け方: `export def find_item :: Maybe<ref Filter> -> Maybe<Item>` を `--emit shared` と `bindings-cs`・`bindings-py`・`bindings-cpp` でビルドし、3 言語すべてで `null` / `None` / `std::nullopt` の往復が成功する。

## 目的

[W15](_completed/W15-abi-nullable-handles.md) D3 は、実装規模を抑えるため `export def` での `Maybe` を `E1008` のまま残した。しかし Tsuzuri を C・C++・C#・Python ホストへ組み込むライブラリとして使う際、`export def` 側で省略可能なレコード参照や「見つからなければ無し」のハンドル結果を表現できなければ、公開 API の利用者・実装者の双方がダミー値や別引数フラグに頼らざるを得ない。[NS01](_completed/NS01-nonnull-extern-handles.md) によって `H` と `Maybe<H>` の境界表現が整理された上で、公開 ABI とネイティブ 3 言語のバインディング生成器まで一貫して Null 安全にする。

## 着手条件と停止条件

### 着手条件

- [W15](_completed/W15-abi-nullable-handles.md) と [NS01](_completed/NS01-nonnull-extern-handles.md) が `done` であること。
- 決定事項 D1 の承認があること。
- `node scripts/repo-kb.mjs brief NS02` を読み、`ABI_VERSION`（`src/bindings.rs`）と既存の `tests/host_bindings.mjs` の構成を確認する。

### 停止条件

- **S1** `Maybe` を使わない `export def` の IR・ヘッダー・生成バインディングが 1 byte でも変わる。
- **S2** `--trap-mode return` の `tsuzuri_try_<name>` で `Maybe<H>` の結果スロットの型や NULL 時の扱いが `tz_<name>` と食い違う。
- **S3** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `edc61676` で確認）

- `src/check.rs` の `UnsupportedExportSignature` は `abi::parameter` と `abi::result` で `export def` を検査しており、`Type::Union`（`Maybe` を含む）はすべて `E1008` になる。
- `src/bindings_native.rs` の `Shape`（145〜170 行）は `Scalar`・`Handle`・`Slice`・`Owned`・`Record`・`Unit` の 6 種類のみで、オプショナル型を持たない。C# ファイルの先頭にはすでに `#nullable enable`（499 行）が出力されている。

## 変えてはいけない意味

- `Maybe` を使わない `export def` の C ヘッダー・IR・生成バインディング。
- WASM の公開 ABI と `--emit bindings-js` の仕様（`Maybe` を持つ `export def` は WASM ターゲットでは `E1008` `abi_native_only`）。

## 仕様

### 1. `export def` で許す `Maybe` の位置（native）

| 位置 | 型 | C ヘッダーの型 | C との受け渡し |
| --- | --- | --- | --- |
| 引数 | `Maybe<H>` | `tz_handle_...` | `NULL` → `None`、非 `NULL` → `Some h`（所有権は関数へ移動） |
| 引数 | `Maybe<ref H>` | `tz_handle_...` | `NULL` → `None`、非 `NULL` → `Some (ref h)`（呼び出し中のみ借用） |
| 引数 | `Maybe<ref R>`（`R` はスカラー record） | `const tz_record_... *` | `NULL` → `None`、非 `NULL` → アライメント検査後に `read_host_record` して `Some` |
| 結果 | `Maybe<H>` | `tz_handle_...`（`tsuzuri_try_*` では `tz_handle_... *result`） | `None` → `NULL`、`Some h` → `h` |

- それ以外の `Maybe`（`Maybe<i64>`、値の `Maybe<R>`、結果の `Maybe<ref R>`、`Maybe<string>` など）は W15 と同じく `E1008`（`abi_nullable_payload`）。
- WASM（`--target wasm32`・`wasm64`、`--emit bindings-js`）では、`export def` が常にモジュールの根として到達するため `E1008`（`abi_native_only`）とする。

### 2. ネイティブ言語バインディング（`bindings-cs`・`bindings-py`・`bindings-cpp`）

| Tsuzuri の型 | C#（`#nullable enable`） | Python | C++（C++20） |
| --- | --- | --- | --- |
| 引数 `Maybe<H>` / `Maybe<ref H>` | `Handle?`（`null` → `0`、`Some` で `Value == 0` なら `ArgumentException`） | `Handle \| None`（`None` → `None`、`Handle(0)` は NS01 どおり拒否） | `std::optional<Handle>`（`std::nullopt` → `nullptr`、`Some(nullptr)` は `std::invalid_argument`） |
| 引数 `Maybe<ref R>` | `in R?` または `R?`（`HasValue` のときだけ pin / 参照渡し、`null` は null ポインタ） | `R \| None`（`None` → `None`、値ありは `_ctypes.byref`） | `const R *` または `std::optional<R>`（D4: `std::optional<R>` を受け、値ありは一時変数へのポインタ、`std::nullopt` は `nullptr`） |
| 結果 `Maybe<H>` | `Handle?`（`0` → `null`、非 `0` → `new Handle(value)`） | `Handle \| None`（`None` / `0` → `None`、それ以外 → `Handle(raw)`） | `std::optional<Handle>`（`nullptr` → `std::nullopt`） |

## 設計

- **`src/abi.rs` と `src/check.rs`**: W15 の `abi::nullable` を `export` の検査でも参照し、仕様 1 の 4 形を受理、それ以外の std `Maybe` を `abi_nullable_payload` で報告する。`uses_host_abi` / `extended` は、`Maybe<ref R>` があるときだけ `%tz_record_*` 定義を有効にする（`Maybe<H>` 単体では `tsuzuri_alloc` を出さない）。
- **`src/llvm_abi.rs`（`wrapper`）**:
  - `Maybe<H>` 引数: NS01 により内部表現も `ptr`（`None` = `null`）なのでそのまま渡す。
  - `Maybe<ref H>` 引数: `null` と比べ、非 `null` のとき `spill` したスロットのアドレス、`null` のとき `null` を `select` する（W15 のコールバック引数と共通関数化）。
  - `Maybe<ref R>` 引数: `null` で分岐し、非 `null` 枝でのみ `host_pointer(..., align, false)`（アライメント検査）→ `read_host_record` → `spill` を行い、`null` 枝の `null` と `phi ptr` で合流する。
  - `Maybe<H>` 結果: 内部表現が `ptr` なのでそのまま `ret ptr %value`。`try_prototype` は `c_view` を通して `tz_handle_... *result` を出す。
- **`src/bindings_native.rs`**: `Shape` に `NullableHandle(String)` と `NullableRecord(String)` を追加し、3 言語の型注釈・変換コード・`--trap-mode return` ラッパーを更新する。

## 前提とする他チケットのインターフェース

- [W15](_completed/W15-abi-nullable-handles.md): `abi::Nullable`, `abi::nullable`, `abi::c_view`, `DiagnosticKind::AbiNullablePayload`, `DiagnosticKind::AbiNativeOnly`。
- [NS01](_completed/NS01-nonnull-extern-handles.md): ネイティブの `Maybe<H>` の `Niche::NullPointer` レイアウトと非 `Maybe` ハンドルの非 Null ガード。

## 他チケットへ提供するインターフェース

- 公開 C ABI および C#・Python・C++ 生成バインディングでの Nullable ハンドル・レコード入力。

## 実装手順

1. **変更**: なし（ブランチ `feat/ns02-export-nullable`）。**内容**: `node scripts/repo-kb.mjs brief NS02`、既存バインディング出力のスナップショット保存。**確認**: 基準値を記録。
2. **変更**: `src/abi.rs`, `src/check.rs`, `src/llvm.rs`, `src/llvm_abi.rs`。**内容**: `export def` での `Maybe` 受理、`wrapper`・ヘッダー・`try_prototype` 対応、WASM と `bindings-js` での `abi_native_only` 拒否。**確認**: `tests/abi_nullable.rs`、T1。
3. **変更**: `src/bindings_native.rs`, `tests/host_bindings.mjs`。**内容**: C#・Python・C++ バインディングの `NullableHandle`・`NullableRecord` 生成と実行テスト。**確認**: `node tests/host_bindings.mjs target/debug/tsuzuri`（通常と `--trap-mode return` の両方）。
4. **変更**: 文書。**内容**: `docs/language.md`・`native-interop.md`・`maybe.md`・`webassembly.md` の更新。**確認**: T2、`node scripts/check-docs.mjs <変更したページ>`。PR に `ci:full`。

## テスト計画

- `tests/abi_nullable.rs`: `export_nullable_positions_are_accepted`、`export_other_maybe_payloads_fail_with_abi_nullable_payload`、`wasm_and_bindings_js_reject_nullable_exports`、`try_wrapper_handles_nullable_export_result`。
- `tests/host_bindings.mjs`: `Maybe<H>` 引数・結果と `Maybe<ref Point>` 引数を持つ共有ライブラリをビルドし、C#・Python・C++ の各ホストから `null` / 非 `null` の両方を渡して受け取るテスト（通常モード・`--trap-mode return` の両方）。アライメント違反ポインタを C から渡したときにトラップになることも確認。

## ドキュメント

- `docs/language.md`: 「公開 ABI」の型対応表に `Maybe<H>`・`Maybe<ref H>`・`Maybe<ref R>`（native のみ）を追加。
- `_tsuzuri/language-reference/compiler/native-interop.md`: 「公開できる型」と「共有ライブラリと各言語のバインディング」の表を更新。
- `_tsuzuri/language-reference/built-in-types-and-modules/maybe.md`、`_tsuzuri/language-reference/compiler/webassembly.md`。

## 受け入れ条件

- [ ] 仕様 1 の 4 形がネイティブの `export def`（通常・`--trap-mode return`）で動作し、`live == 0`。
- [ ] C#・Python・C++ の生成バインディングで Null / 非 Null の双方向受け渡しが通る。
- [ ] WASM ビルドと `--emit bindings-js` が `E1008`（`abi_native_only`）で拒否する。
- [ ] `Maybe` を使わない `export def` の IR・ヘッダー・バインディング出力が 1 byte も変わらない。

## 落とし穴

- C# で `Handle` は `readonly record struct`（値型）なので `Handle?` は `System.Nullable<Handle>` になる。`new Handle(0)` が `Some(new Handle(0))` として渡された場合は `None` に潰さず `ArgumentException` にする（`Some(NULL)` を許さない）。
- `Maybe<ref R>` の非 `NULL` ポインタには、通常の `ref R` と同じくアライメント検査（`host_pointer(..., align, false)`）を必ず通す。

## 対象外

- WASM の `export def` と `--emit bindings-js` での `Maybe` 対応（wasm32 ハンドル `0` 問題のため）。
- 所有バッファや文字列の `Maybe` 結果（例: `Maybe<string>`）。必要になった場合は別チケットで out 記述子の表現を決める。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | ネイティブの `export def` に `Maybe<H>`・`Maybe<ref H>`・`Maybe<ref R>` 引数と `Maybe<H>` 結果を許し、[W15](_completed/W15-abi-nullable-handles.md) D3 の制限を解除する | 公開 C ABI とホスト言語バインディングでも `NULL` を型安全に扱うため | 要承認 |
| D2 | WASM と `--emit bindings-js` は `E1008`（`abi_native_only`）のまま拒否する | W15 D2 と同じ理由（wasm32 ハンドル番号 `0` と WASM の参照 niche 不在） | 既定案（実装者はこの案に従う） |
| D3 | C# は `T?`（`Nullable<T>`）、Python は `T \| None`、C++ は `std::optional<T>` に写す | 各言語の標準的な Null 安全オプショナル型と一致させ、生の `0` や `nullptr` 操作を利用者に書かせないため | 既定案（実装者はこの案に従う） |
| D4 | C++ の `Maybe<ref R>` 引数は `std::optional<R>`（またはオーバーロードで `const R *` ではなく `const std::optional<R> &`）を受ける | C++ 利用者コードから生ポインタを排除し `std::nullopt` で統一するため | 既定案（実装者はこの案に従う） |
| D5 | C# の `Some(new Handle(0))` と C++ の `std::optional<Handle>(nullptr)` は `NULL` として黙って送らず引数例外にする | Tsuzuri と同様、ホスト側ラッパーでも `Some(NULL)` の混入を防ぐため | 既定案（実装者はこの案に従う） |
