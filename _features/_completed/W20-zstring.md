# W20: NUL 終端の UTF-16 文字列 `ZString`

| 項目 | 内容 |
| --- | --- |
| ID | W20 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | [W13](W13-extern-ref-mut-and-mutable-buffers.md) |
| 後続 | [W21](W21-c-layout-records.md)（Phase 2: `ref ZString` の field）、[W27](W27-win32-package-from-winmd.md)（PCWSTR → `ref ZString`） |
| 状態 | done |
| 起票 | 2026-11-22（D-63。利用者の依頼「X: D13 を狭く改める方針で決定。実現向けたチケットを @_features に W プリフィックスのチケットを作成して」と「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」から起票。HEAD `fa40eede` で確認） |
| 承認 | 要承認（D-17 の扱い、D-55）。決定事項 D1・D2 が要承認、ほかは既定案。方針は [GUIDE D-63](GUIDE.md#d-63-tsuzuri-だけで-win32-のアプリを書くe12-d13-の狭い改訂w01w30) |
| 手本にする既存実装 | `std/Timer.tz`（不透明な std の record と opt-in のモジュール。`src/stdlib.rs` の `opaque_record`・`OPT_IN`・`RESERVED_MODULES`）、TZ05 の組み込み `Time.after`（std の型を名指す組み込み）、`src/const_static.rs` と `src/llvm.rs` の `static_definition`・`static_operand`（読み取り専用の静的データと `Borrow(Static)`）、`std/String.tz` の `new [i16u](n, \index -> … text[index] …)`、`src/llvm_imports.rs` の `host_call` の共有バッファの経路 |
| 主な影響ファイル | `std/ZString.tz`（新規）、`src/stdlib.rs`、`src/check.rs`（組み込み `ZString.literal`）、`src/abi.rs`、`src/llvm_imports.rs`、`src/llvm_abi.rs`、`src/llvm.rs`、`src/effects.rs`・`src/pure_rules.rs`・`src/kg/effects.rs`・`src/capabilities.rs`、`src/diagnostic_kinds.rs`、`src/explain/codes/E1005.md`・`E1008.md`、`tests/zstring.rs`（新規）、`tests/std_argument_order.rs`、`tests/std_examples.rs`、`tests/ffi_extensions.mjs`・`tests/ffi_extensions_host.c`、`vsc/src/core.ts`、`spec/rules/ABI.md`、文書 |

## 実装者向け早見

- 範囲: opt-in の std のモジュール `ZString`。不透明な型 `ZString`（途中に U+0000 を持たず、末尾に 0 を持つ UTF-16）、`ZString.of :: ref string -> Result<ZString, ZString.Error>`、`ZString.literal "..."`（文字列リテラルだけを取り、確保なしの静的データへの `ref ZString`）、`ZString.to_string :: ref ZString -> string`。native の `extern` の引数 `ref ZString` は `const uint16_t *` 1 つに下がる。
- 最初に読む: 「現状」→ 仕様 1〜4 → `std/Timer.tz`（不透明な record の書き方）→ `src/llvm.rs` の `static_definition`。
- 最初に書くテスト: `tests/zstring.rs` の `ref_zstring_is_one_pointer`（`declare` の引数が `ptr` 1 つ）→ `literal_is_static`。
- 触ってはいけない: `string` の表現とその ABI（`ref string` は今どおり `ptr` と `i64`）、`ZString` を名指さないプログラムの IR（`ZString` は opt-in。常に読み込むモジュールに足すと全プログラムの内部の名前の番号がずれる。TZ05 の再検討 1）。
- 完了の見分け方: 次が Windows で 7 を返し、`rename` が窓の題名を変える。

```tsuzuri
extern type Hwnd

@trusted
extern "SetWindowTextW" def set_window_text :: ref Hwnd -> ref ZString -> i32

@trusted pure no_alloc
extern "lstrlenW" def lstrlen :: ref ZString -> i32

def rename :: ref Hwnd -> ref string -> Result<i32, ZString.Error> = \hwnd text ->
    match ZString.of text with
    | Result.Ok title -> Result.Ok (set_window_text hwnd (ref title))
    | Result.Error error -> Result.Error error

def main :: unit -> i32 = \() -> lstrlen (ZString.literal "Tsuzuri")
```

## 目的

Win32 の文字列の引数（LPCWSTR）は NUL 終端の UTF-16 のポインター 1 つで、窓の題名、クラス名、`MessageBoxW` の本文、フォント名など、ほとんどの API に現れる。今の `ref string` は `const uint16_t *` と `int64_t` の長さの 2 つで終端の 0 を持たない（D-17・E05）ので、どれも契約の中で呼べない。D-17 は「借用時の暗黙変換・コピーは行わない」と決めている。そこで、暗黙の写しではなく、写しがソースに見える明示の型を足す。変換は 1 回だけ行って何度も使え（クラス名・題名）、リテラルは確保なしの静的データにできる。

## 着手条件と停止条件

### 着手条件

- W13 が done（`abi::extern_parameter`、WASM の `abi_native_only` の共有の口、`count_not_buffer` を使う）。
- 決定事項 D1・D2 の承認。承認前は手順 2 以降に着手しない。
- `grep -n "pub(crate) fn opaque_record\|pub const RESERVED_MODULES\|RESERVED_MODULES.len()" src/stdlib.rs` で、今の不透明な record の表と予約名の数を読む（`RESERVED_MODULES` は HEAD で 73。着手時の値から 1 増やす）。

### 停止条件

- **S1** `ZString.literal` を `Borrow(Static(…))` へ書き換える経路が、定数の番号付け（`CheckedModule::statics` の鍵）か、`ZString` を使わないプログラムの IR を変えずには作れない。選択肢: A 専用の型付き IR の式の種類を足す（全 `match` の確認が要る）、B リテラルも評価ごとに確保する（「確保なし」を外すので要承認）。止めて報告する。
- **S2** `ZString` を名指さないプログラム（`tests/fixtures`・`examples` の全 `Main.tz`）の `--emit llvm -O1`（native と wasm32）か `check --json` が 1 byte でも変わる。
- **S3** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

- `ref string` の extern の引数は `ptr` と `i64` の 2 つ（開発版の `--emit llvm` で `declare i32 @tsuzuri_host_Main_host_rect(ptr, ptr, i64)`、ヘッダーで `const uint16_t *arg1_ptr, int64_t arg1_len`）。終端の 0 はない。compact 文字列（`TSUZURI_STRINGS=compact`）の Latin-1 は、呼び出しごとに `tz.string.borrow_utf16` で UTF-16 に広げた写しを作り、呼び出しの後に解放する（`src/llvm_imports.rs` の `host_call` の `widen`・`free_copies`）。
- 文字列リテラルは評価ごとにヒープへ写す（string.md「所有権を移す変換」の「定数領域のリテラルは、先にヒープへコピーされます」）。読み取り専用の静的データは D11 Phase 2 の `TypedExprKind::Static` だけで、共有の `Borrow` の被演算子にしかならない。`src/const_static.rs` の `eligible` は、ヒープの配列を持つ record を静的データにしない（配列は定数の全体のときだけ）。
- 不透明な std の record は `src/stdlib.rs` の `opaque_record` の表で決まり、std の外での構築・field の参照は `E1022`（`opaque_type_representation`）。std のモジュール `ZString` はなく、`RESERVED_MODULES` は 73 個（`src/stdlib.rs` のテストの `assert_eq!(RESERVED_MODULES.len(), 73)`）。std の record は内部の並べ替えの対象外（docs/architecture.md のレコードの格納順の段落）。
- 呼び出し先が埋めた `[i16u]` から `string` を作るのは、今の関数で足りる（開発版で確認）: `String.from_code_units (Array.truncate n (ref buf))`（長さが分かるとき）と、`Array.index_of (ref zero) (ref buf)` で最初の 0 を探してから同じ形（最初の NUL まで）が、どちらも `Hi` を出した。`Array.truncate :: i64 -> ref ['a] -> ['a]` は k 個を写し、`String.from_code_units :: [i16u] -> string` は既定の UTF-16 ではバッファを移すだけ。
- 途中の NUL の前例: OS の API は、パス・環境変数の名前・`Process.run` の引数の NUL を `InvalidInput` のエラーにする（docs/language.md 1387 行付近）。切り捨てもトラップもしない。
- D-17: 「E05 は…string の ABI を UTF-16/i16u のコード単位…とする。借用時の暗黙変換・コピーは行わず、返却 buffer の所有権だけをホストへ移す」。

## 仕様

### 1. 型

- `std/ZString.tz`（opt-in。ソースが `ZString` を名指すと読み込む）に `record ZString { units: [i16u] }`。不透明（std の外での構築・`z.units` は `E1022`）。不変条件: `units` の長さは 1 以上で、最後の要素だけが 0。Copy でない（ムーブ）。利用者の `Drop` はない（配列は drop の接着で解放）。Send・Sync は `[i16u]` と同じ。
- `union Error = InteriorNul of i64`（最初の U+0000 の UTF-16 の位置）。`instance Display<Error>`（文案 `U+0000 at UTF-16 index 3 cannot be in a NUL-terminated string`）。case は major の edition でだけ足す（`Os.ErrorKind` と同じ約束）。

### 2. 関数

| 関数 | 型 | 意味 | 費用 |
| --- | --- | --- | --- |
| `ZString.of` | `ref string -> Result<ZString, Error>` | U+0000 があれば最初の位置の `Error`。なければ n + 1 単位を 1 回だけ確保して写し、末尾に 0 を置く（compact の Latin-1 は写しながら広げる） | O(n)、確保 1 回 |
| `ZString.literal` | 文字列リテラル → `ref ZString` | コンパイル時に UTF-16 と末尾の 0 を作った読み取り専用の静的データへの共有借用。評価しても確保しない。同じ内容のリテラルは 1 つのデータを指す | O(1)、確保なし |
| `ZString.to_string` | `ref ZString -> string` | 末尾の 0 を除いて写す | O(n)、確保 1 回 |

- `ZString.literal` の引数は、補間のない文字列リテラル（通常・複数行・生の文字列）だけ。変数・`$"…"`・関数の結果は `E1005`（kind 案 `zstring_literal_argument`）、U+0000 を含むリテラル（`"a\0b"`）は `E1005`（`zstring_literal_interior_nul`）、`ZString.literal` を関数値として使うのも `zstring_literal_argument`。
- `ZString.literal` の値は定数の静的データの借用と同じ規則で、`let` に束縛し、`ref ZString` を取る関数へ渡せる。

### 3. ABI

- `extern` の引数 `ref ZString` は `const uint16_t *argN`（`units` の先頭の要素のアドレス）。ホストは呼び出しの間だけ、最初の 0 まで（0 を含む）を読んでよく、書き込み・保持をしてはならない。写しも確保もしない（常に UTF-16 なので compact の広げも要らない）。
- `ZString` の値の引数、結果、`ref mut ZString`、コールバックの引数・結果、`export def` の引数・結果は `E1008`（`unsupported_extern_signature`・`unsupported_export_signature`）。`@count` を `ref ZString` の位置に書くと `E1008` `count_not_buffer`（W13）。record の field は W21 の Phase 2、NULL を取る `Maybe<ref ZString>` は、W15 と W20 のうち後に入るほうが W15 の `Nullable` に腕を足して有効にする（W15 の D7）。
- wasm32・wasm64 で `ref ZString` の extern を生成すれば `E1008` `abi_native_only`（W13 の口）。モジュールの関数（`of`・`literal`・`to_string`）そのものは WASM でも使える。
- `--emit header` の、リンク名のない extern のプロトタイプは `const uint16_t *argN`。

### 4. 呼び出し先が埋めたバッファの読み戻し

新しい関数は足さない（決定事項 D8）。W13 の `ref mut [i16u..]` で受けたバッファは、長さが分かれば `String.from_code_units (Array.truncate n (ref buf))`、最初の NUL までなら `Array.index_of` で位置を求めてから同じ形で読む。どちらも k 単位の写し 1 回とバッファの移動で、`zstring.md` と native-interop.md に例を書く。

## 設計

- `std/ZString.tz`: `of` は `text[index]` を走査して最初の 0 を探し、なければ `new [i16u](n + 1, \index -> if index < n then text[index] else 0)` で 1 回だけ確保する（`std/String.tz` の `to_ascii_lower` と同じ形）。`to_string` は `String.from_code_units (Array.truncate (Array.length (ref zstring.units) - 1) (ref zstring.units))`。どちらも Tsuzuri で書き、ランタイムの C を足さない。公開関数に G34 の形式の例（`///`）を付ける。
- `src/stdlib.rs`: `SOURCES`・`OPT_IN`・`RESERVED_MODULES`（+1）・`opaque_record` に `"ZString.ZString"`。
- 組み込み `Builtin::ZStringLiteral`（名前 `ZString.literal`。`Builtin::ALL`・`name`・`signature`・効果の分類（A19・A20 の表）を全部更新する。GUIDE-LITE §7）。署名は `string -> ref ZString.ZString`（TZ05 の `Time.after` と同じく std の型を名指す）。型検査は引数が `TypedExprKind::String` の補間のないリテラルであることを確かめ、UTF-16 の単位に 0 がないことを確かめ、呼び出しを `Borrow(Static(id), false)` に書き換える。`id` は、どの定数の宣言とも重ならない鍵で `CheckedModule::statics` に足す合成の `StaticData`（値は `ZString { units: [<単位>, 0] }` のリテラルの木）で、同じ単位の列は同じ鍵を使う（`BTreeMap` で決定的に）。静的データの予算 `MAX_STATIC_BYTES` に数える。
- 生成: `static_definition` に `ZString.ZString` の枝を足し、`@tz.static.<id>.data = private unnamed_addr constant [N x i16] [...]` と `@tz.static.<id> = private unnamed_addr constant %<ZString の型> { %tz.array { ptr @tz.static.<id>.data, i64 N } }` を出す（`N` は 0 を含む数）。`Borrow(Static)` は既存の経路でそのアドレスになる。読み取り専用で、drop されない（共有借用だけが指す）。
- ABI: `abi::zstring(ty, types)`（`Type::Record(id, [])` で `types.records[id].opaque()` かつ名前が `ZString.ZString`）を足し、W13 の `abi::extern_parameter` が `Reference(zstring, false)` を受ける。`host_call` は field 0（`units`。std の record は並べ替えない）の `%tz.array` を読んで `extractvalue … 0` の `ptr` 1 つを渡す。`c_parameters` は `const uint16_t *argN`。WASM の `abi_native_only` の共有の口（W13・W15）に `ref ZString` を足す。W14 が done なら `c_signature` で `ptr` 1 つ。
- 効果・権限: `ZString.literal` は効果なし（`alloc` も `trap` もない。`@no_alloc` の関数から使える）、`of`・`to_string` は `alloc`。権限は要らない（`src/capabilities.rs` と `tests/fixtures/capabilities/std-table.txt` の公開関数の行）。`src/kg/effects.rs`・`src/pure_rules.rs` の分類。件数の pin（`Builtin::ALL` の長さなど）は着手時の値から数える（GUIDE-LITE §11）。
- `tests/std_argument_order.rs` の `MODULES` に `("ZString", subject(&["ZString"]))`。`tests/std_examples.rs` の床の表（FLOORS）に `ZString`。`vsc/src/core.ts` のモジュール名に `ZString`。

## 前提とする他チケットのインターフェース

- W13: `abi::extern_parameter`、WASM の `abi_native_only` の共有の口、`count_not_buffer`、`host_call` の引数の分岐。
- W14（任意）: `abi::c_signature` に `ref ZString` を `ptr` 1 つとして足す（W14 が先なら W20 が足す。W20 が先なら W14 が足す）。

## 他チケットへ提供するインターフェース

- `abi::zstring` と、`ref ZString` の下げ方（`units` の先頭のポインター 1 つ）。W15 の `Maybe<ref ZString>`（NULL かこのポインター。W15 と W20 のうち後に入るほうが腕を足す。W15 の D7）、W21 の Phase 2 の `ref ZString` の field（呼び出しの間だけ有効）、W16 の例（`SetWindowTextW`）、W27 の PCWSTR の写像が使う。
- 合成の静的データ（`ZString.literal`）の経路。

## 実装手順

1. **変更**: なし（下書きは `target/scratch/w20/`）。**内容**: `node scripts/repo-kb.mjs brief W20`、着手条件の `grep`。`tests/fixtures`・`examples` の全 `Main.tz` の `--emit llvm -O1`（native と `--target wasm32`）と `check --json` を保存する。**確認**: 数を記録する。
2. **変更**: `std/ZString.tz`、`src/stdlib.rs`、`tests/std_argument_order.rs`、`tests/std_examples.rs`、`vsc/src/core.ts`。**内容**: 型・`Error`・`of`・`to_string`（組み込みなし）。**確認**: T0、`cargo test --locked --test std_argument_order --test std_examples`、`tsuzuri test --doc std`（`ZString` の例）、T1。
3. **変更**: `src/check.rs`・`src/llvm.rs`・効果と権限の表・`src/diagnostic_kinds.rs`。**内容**: `ZString.literal`（G23 の手順で kind を登録）。**確認**: `literal_is_static`・`literal_rejections`、T1。
4. **変更**: `src/abi.rs`・`src/llvm_imports.rs`・`src/llvm_abi.rs`・`src/llvm.rs`。**内容**: `ref ZString` の ABI と WASM の拒否。**確認**: `ref_zstring_is_one_pointer`・`zstring_abi_rejections`・`wasm_rejects_ref_zstring`、T1。
5. **変更**: `tests/ffi_extensions_host.c`・`tests/ffi_extensions.mjs`（W06 が done なら `tests/windows.mjs`）。**内容**: E2E。**確認**: `cargo build && node tests/ffi_extensions.mjs target/debug/tsuzuri` を `-O0`・`-O3`、`TSUZURI_STRINGS=compact` でも。
6. **変更**: 文書・`spec/rules/ABI.md`・explain。手順 1 の保存物と比べる。**確認**: T2、`node scripts/check-docs.mjs <変更したページ>`、`node scripts/check-cli-docs.mjs`、`node scripts/gen-docs.mjs --check`。PR に `ci:full` を付ける。

## テスト計画

| Rust（`tests/zstring.rs`） | 内容 |
| --- | --- |
| `types_and_opacity` | `ZString.of` の型、std の外の `ZString { units: … }` と `z.units` が `E1022` |
| `literal_is_static` | IR に `private unnamed_addr constant [8 x i16]`（`Tsuzuri` と 0）があり、`ZString.literal` を含む関数に `@tz.alloc` がない。同じリテラル 2 つが 1 つのデータ。2 回の `emit` が一致 |
| `literal_rejections` | 変数・`$"…"`・関数の結果・関数値としての使用が `E1005` `zstring_literal_argument`、`"a\0b"` が `zstring_literal_interior_nul` |
| `ref_zstring_is_one_pointer` | `declare i32 @lstrlenW(ptr)`、ヘッダー（リンク名のない extern）が `const uint16_t *arg0` |
| `zstring_abi_rejections` | 値の `ZString`・結果・`ref mut ZString`・コールバックの引数・export の引数が `E1008`（今の kind）、`@count` の位置が `count_not_buffer` |
| `wasm_rejects_ref_zstring` | `llvm::emit_target(…, true)` が `abi_native_only`。extern のない `ZString` の関数だけなら成功する |
| `no_alloc_literal` | `@no_alloc def` から `@trusted no_alloc` の extern に `ZString.literal` を渡せる。`ZString.of` は `E1031` |

std の例（G34）: `of`（成功と `InteriorNul 1`）、`literal`、`to_string` の往復。期待値は手で書いた値。

E2E（`tests/ffi_extensions.mjs`、native、`-O0`・`-O3`、既定と `TSUZURI_STRINGS=compact`）: ホストの C の `int64_t zlen(const uint16_t *s)`（0 までの数を数え、0 の位置まで読めることを確かめる）に、`ZString.literal`、`ZString.of` の結果（Latin-1 の `"abc"` と、BMP 外を含む文字列）を渡し、`String.length` と一致すること。`to_string` の往復。Windows（W06 が done なら `tests/windows.mjs`）: `lstrlenW`、メッセージ専用の窓の `SetWindowTextW` と W13 の `GetWindowTextW` の往復。

## ドキュメント

- 新規 `_tsuzuri/language-reference/built-in-types-and-modules/zstring.md`（型、3 つの関数、`Error`、ABI、読み戻しの書き方、「引数の順序」の節、ターゲット）。`index.md`、`argument-order.md`（対象になる値の表に `ZString`）、`string.md`（NUL 終端の文字列と読み戻しへの案内）。
- `compiler/native-interop.md`「ホスト関数を import する」に `ref ZString` と C の型、docs/language.md「ホスト関数のインポート」（引数の型の列挙、D-17 との関係）と予約名の一覧、`organizing-tsuzuri/modules.md`（予約名の数）。
- `compiler/diagnostics.md` の kind の表（`node scripts/gen-docs.mjs`）。`src/explain/codes/E1005.md`・`E1008.md`。
- `spec/rules/ABI.md`（番号は着手時の最大の次）: 「An extern `ref ZString` argument MUST pass one pointer to NUL-terminated UTF-16 without a copy.」と、`spec/rules/STD.md` に「`ZString.of` MUST return `InteriorNul` for a text with U+0000.」、適合性テスト。
- README は記述が誤りにならないので変えない。新しく予約した std のモジュール名 `ZString` を release notes に載せる。

## 受け入れ条件

- [ ] 仕様 1〜4 のとおりに動く（native の `-O0`・`-O3`、compact 文字列、WASM の `ZString` の関数）。
- [ ] `ZString` を名指さないプログラムの IR・object・header・診断が、手順 1 の保存物と 1 byte も変わらない。WASM の既存の出力も変わらない。
- [ ] `ZString.literal` の関数に確保がない（IR と `@no_alloc` のテスト）。
- [ ] `MODULES`・FLOORS・予約名・権限の表・文書・規則を更新した。T2 が通り、PR に `ci:full` を付けた。

## 落とし穴

- `ZString` を常に読み込むモジュールにしない（全プログラムの内部の名前の番号がずれて S2 を破る。TZ05 の再検討 1）。opt-in で、組み込みの名前 `ZString.literal` も `ZString` を名指すので読み込みの規則を満たす。
- 静的データの `ZString` は共有借用だけが指す。値として取り出す（`take`）経路を作ると、静的な配列を `free` する。`Borrow(Static)` の外で使わない。
- compact 文字列の `text[index]` は Latin-1 でも単位を返すので、`of` は表現に依らない。テストは両方で回す。
- `to_string` の長さは `units` の長さ − 1。不変条件（長さ 1 以上）があるので負にならない。
- `ref ZString` の `host_call` は `widen` の経路に入れない（`Buffer` ではない）。`Buffer::of` に足すと長さの引数まで出る。

## 対象外

- `ZString` の結果（ホストが返す PCWSTR の所有は決まらない）、`ref mut ZString`、二重 NUL 終端の列（`OPENFILENAMEW.lpstrFilter`。`[i16u]` で組む）、UTF-8 の NUL 終端（`char *`）。
- `Text` のインスタンス、`ZString` どうしの連結・比較・複製（`ZString.of (ZString.to_string (ref z))` で書ける）。
- `string` そのものに終端の 0 を保証すること（下の D3）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | LPCWSTR は明示の型 `ZString` で表す。呼び出しごとに暗黙に写す属性（`@nul` など）は採らない。D-17 は改めず、「写しはソースに見える `ZString.of` だけ」と読む | D-17 の「借用時の暗黙変換・コピーは行わない」に沿う。暗黙の写しは、ループの中で同じ文字列を毎回確保し、`@no_alloc` から見えない確保を作る。明示の型は 1 回の変換を何度も使え、リテラルは確保なしにできる | 要承認（D-17 の扱い。承認前は手順 2 以降に着手しない） |
| D2 | std のモジュール名 `ZString` を D-55 に登録し、opt-in にする（`RESERVED_MODULES` を 1 増やす） | std と組み込みのモジュール名は D-55 の登録。opt-in なら使わないプログラムの IR が変わらない | 要承認（D-55。承認前は手順 2 以降に着手しない） |
| D3 | 表現は不透明な record `{ units: [i16u] }` で、末尾の 0 を持つ | 引数はポインター 1 つで写しが要らない。`string` 自体に 0 を保証する案は、確保・連結・compact の Latin-1 のすべての経路に手が入り、Latin-1 は結局広げる写しが要る | 既定案（実装者はこの案に従う） |
| D4 | 途中の U+0000 は `ZString.of` の `Error`（`InteriorNul` と位置） | 切り捨ては黙って値を変える（GUIDE §1 規則 2）。トラップは入力で落ちる。OS の API の NUL の前例もエラーを返す | 既定案（実装者はこの案に従う） |
| D5 | `ZString.literal` はリテラルだけを取り、静的データへの `ref ZString` を返す | 所有する値にすると drop が静的データを解放するので、評価ごとに確保する（今の文字列リテラルと同じ）しかない。共有借用なら確保なしで、定数の静的データ（D11 Phase 2）と同じ扱いになる | 既定案（実装者はこの案に従う） |
| D6 | ABI は extern の引数の `ref ZString` だけ（値・結果・可変・コールバック・export は `E1008`） | ホストが所有を受け取る・返す形は契約が決まらない。Win32 の要求は入力の LPCWSTR | 既定案（実装者はこの案に従う） |
| D7 | `ref ZString` の extern は native だけ（WASM は `abi_native_only`） | W チケットの共通規則。JS の糊に NUL 終端の記述子がない | 既定案（実装者はこの案に従う） |
| D8 | 呼び出し先が埋めたバッファから `string` を作る関数は足さない | `Array.truncate` と `String.from_code_units`（と最初の NUL までなら `Array.index_of`）で、写し 1 回の同じ費用で書ける（開発版で確認） | 既定案（実装者はこの案に従う） |
| D9 | `MODULES` の行は `subject(&["ZString"])` | 今の関数は 1 引数だが、後で足す関数も `ZString` を最後に取る規則を表に残す（D-59） | 既定案（実装者はこの案に従う） |
| D10 | `of`・`to_string` は std の Tsuzuri で書き、ランタイムの C を足さない | 既存の配列と文字列の操作で確保 1 回・O(n) にでき、native と WASM で同じ実装になる | 既定案（実装者はこの案に従う） |
| D11 | 診断は既存のコードの新しい kind（`E1005` の `zstring_literal_argument`・`zstring_literal_interior_nul`）。ABI の誤りは W13 の kind | D-51（新しいコードを割り当てない）。リテラルだけを取る呼び出しの誤りは関数呼び出しの不備（`E1005`）の分類 | 既定案（実装者はこの案に従う） |
