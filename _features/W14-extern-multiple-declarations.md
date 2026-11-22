# W14: 同じ symbol の `extern` を、下げた C の ABI で照合して複数宣言できるようにする

| 項目 | 内容 |
| --- | --- |
| ID | W14 |
| 優先度 | P1 |
| 規模 | S |
| 依存 | – |
| 後続 | [W27](W27-win32-package-from-winmd.md)（生成器が 1 つの関数に用途ごとの宣言を出す） |
| 状態 | todo |
| 起票 | 2026-11-22（D-63。利用者の依頼「X: D13 を狭く改める方針で決定。実現向けたチケットを @_features に W プリフィックスのチケットを作成して」と「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」から起票。HEAD `fa40eede` で確認） |
| 承認 | 要承認（[E12](_completed/E12-ffi-extensions.md) D2 の改訂）。決定事項 D1 が要承認、ほかは既定案。方針は [GUIDE D-63](GUIDE.md#d-63-tsuzuri-だけで-win32-のアプリを書くe12-d13-の狭い改訂w01w30) |
| 手本にする既存実装 | `src/check.rs` の `check_modules_collect` の `explicit_symbols`（今の照合）、`src/llvm_imports.rs` の `host_call`（`declare` の組み立てと `intrinsics` への登録）、`src/llvm_abi.rs` の `c_parameters`（下げた C の引数の並びの規則）、`Diagnostic::related` と `RelatedRole::FirstDefinedHere` |
| 主な影響ファイル | `src/abi.rs`、`src/check.rs`、`src/llvm_imports.rs`、`src/llvm.rs`、`src/bindings.rs`、`tests/ffi_extensions.rs`、`tests/ffi_extensions.mjs`・`tests/ffi_extensions_host.c`、`src/explain/codes/E1008.md`、`spec/rules/ABI.md`、文書 |

## 実装者向け早見

- 範囲: 同じリンク名の `extern "S" def` を、Tsuzuri の型を変えて（別の名前で）複数宣言できるようにする。照合は Tsuzuri の型ではなく、下げた C の引数と結果の位置ごとの ABI の分類で行う。LLVM の `declare` は symbol ごとに 1 つに正規化し、型の違う位置は呼び出し側で `ptrtoint`・`inttoptr` する。
- 最初に読む: 「現状」→ 仕様 1〜4 → `src/check.rs` の `explicit_symbols` の 20 行 → `src/llvm_imports.rs` の `host_call` の `declare` の組み立て。
- 最初に書くテスト: `tests/ffi_extensions.rs` の `same_symbol_with_one_c_abi_is_one_declare`。
- 触ってはいけない: 宣言が 1 つか、すべて同じ型の symbol の IR（今と 1 byte も変えない）。import の module の照合（違えば今の文の `E1008`）。リンク名のない extern（`tsuzuri_host_*`）。
- 完了の見分け方: 次が通り（今は 2 つ目と 4 つ目の宣言が `E1008`）、IR の `declare` は `SendMessageW` と `SetWindowLongPtrW` で 1 行ずつ。

```tsuzuri
extern type Hwnd
record Rect { left: i32, top: i32, right: i32, bottom: i32 }

@trusted
extern "SendMessageW" def send_message :: ref Hwnd -> i32u -> i64u -> i64 -> i64
@trusted
extern "SendMessageW" def send_rect :: ref Hwnd -> i32u -> i64u -> ref Rect -> i64

def on_event :: ref Hwnd -> i32u -> i64u -> i64 -> i64 = \_hwnd _msg _wparam lparam -> lparam

@trusted
extern "SetWindowLongPtrW" def set_window_long :: ref Hwnd -> i32 -> i64 -> i64
@trusted
extern "SetWindowLongPtrW" def set_window_proc :: ref Hwnd -> i32 -> (ref Hwnd -> i32u -> i64u -> i64 -> i64) -> i64
```

W13・W20 の後は、同じ `SendMessageW` に `@count(4, none)` 付きの `ref mut [i16u..]`（`WM_GETTEXT`）と `ref ZString`（`WM_SETTEXT`）の宣言も足せる（どちらも C ではポインター 1 つで、`i64` と同じ分類）。

## 目的

Win32 の 1 つの関数は、同じ位置に整数・ポインター・文字列・関数ポインターのどれでも受ける（`SendMessageW` の `wParam`・`lParam`、`SetWindowLongPtrW` の `dwNewLong`、`LoadCursorW` の `MAKEINTRESOURCE` の整数と名前の文字列）。Tsuzuri には型による多重定義がない（docs/language.md 445 行付近）ので、用途ごとに別の名前の `extern` を同じシンボルへ結べればよいが、今は Tsuzuri の型が違うと `E1008`。照合を C の ABI で行えば、安全性の前提（宣言の正しさは `@trusted` の責任。D-63 の背景 1）を変えずに許せる。W27 の生成器も、この形で 1 つの関数に複数の型付きの宣言を出せる。

## 着手条件と停止条件

### 着手条件

- 決定事項 D1 の承認。承認前は手順 2 以降に着手しない。
- `grep -n "explicit_symbols" src/check.rs` が「現状」と同じ 2 か所を示す。W13・W15・W20 のどれかが done なら、その ABI の形を仕様 1 の表に含める（「前提とする他チケットのインターフェース」）。

### 停止条件

- **S1** 宣言が 1 つか同じ型だけの symbol を持つプログラム（`tests/fixtures`・`examples` の全 `Main.tz`）の `--emit llvm -O1`（native と `--target wasm32`）が 1 byte でも変わる。
- **S2** 正規化した `declare` と呼び出し側の変換で、LLVM の検証か `-O3` の最適化、`--backend fast` が通らない（2 回直して直らなければ止める）。
- **S3** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

- `src/check.rs` の `check_modules_collect` は、リンク名のある extern（`import.explicit`）ごとに `explicit_symbols: BTreeMap<String, (Type, String)>` へ最初の宣言の `(signature.as_type(), wasm_module)` を入れ、後の宣言が違えば `ConflictingExternDeclaration`（`E1008`、kind `conflicting_extern_declaration`、文 `extern symbol '<S>' is declared with different signatures or import modules; declare it once and call it from other modules`）。位置は後の宣言の名前で、`related` はない。
- 再現（`target/debug/tsuzuri.exe`）: `extern "ffi_word" def word_a :: i64 -> i64` と `extern "ffi_word" def word_h :: ref Hwnd -> i64` は、どちらも C では 64-bit の整数レジスター 1 つなのに `E1008`。同じ型の 2 つの宣言は通り、`--emit llvm` の `declare i64 @ffi_word(i64)` は 1 行（`intrinsics` が宣言行の文字列の `BTreeSet` なので、同じ行は 1 つにまとまる）。
- `src/llvm_imports.rs` の `host_call` は、宣言ごとに引数の LLVM 型を並べて `declare <結果> @<S>(<型>)` を組み立てる。型の違う 2 つの宣言が届けば同じ名前の違う `declare` が 2 行になり、LLVM が拒む（E12 D2 の理由）。下げ方: out の結果（record・バッファ）は先頭の `ptr` で結果は `void`、`unit` の引数は省く、スカラーは `abi_type`（8/16/32-bit の整数と `bool` は `i32`）、ハンドル・`ref H`・record（値と `ref`）・コールバックは `ptr`、バッファは `ptr` と `i64`。
- native のターゲットは 64-bit のポインターだけ（`src/target.rs` が 32-bit の native を拒否する）。wasm32 の `ptr` は 32-bit。
- `src/bindings.rs` の `imports` は、届いた extern を WASM の import 名の `BTreeMap` に集める（doc comment: "A name names one import: an explicit symbol has one module and type (E12)"）。JS の糊の記述子は import 名ごとに 1 つで、native の `--emit bindings-cs`・`py`・`cpp` も `signatures` 経由でこの関数の結果のハンドルを数える。
- E12 D2（既定案）: 「同じ symbol の複数宣言は関数型と module が一致すれば許す」。理由「生成 IR 自身の `declare` と型が違うと LLVM が拒否する」。docs/language.md「リンク名」（473 行付近）、native-interop.md 178 行、`src/explain/codes/E1008.md`（"Declare each extern symbol once with one signature"）も同じ規則を書く。

## 仕様

### 1. 下げた C の並びと分類

各宣言の「下げた C の並び」を、今の `host_call`・`c_parameters` の規則で作る: out の結果があれば先頭に `ptr`、`unit` の引数は除く、スカラーは `abi_type`、ハンドル・`ref H`・record・コールバックは `ptr`、バッファは `ptr` と長さ（`i64`。W13 の後は `@count` の型、`none` なら無し）、W13 の `ref mut` と W20 の `ref ZString` と W15 の `Maybe<H>`・`Maybe<ref R>` は `ptr`。結果は `void`・`i32`・`i64`・`float`・`double`・`ptr`。

native の分類（すべて 64-bit のポインター）:

| 分類 | LLVM 型 | Tsuzuri の例 |
| --- | --- | --- |
| `I32` | `i32` | `i8`〜`i32`・`i8u`〜`i32u`・`bool`（符号を問わない） |
| `Word64` | `i64`・`ptr` | `i64`・`i64u`・ハンドル・`ref R`・バッファのポインター・コールバック・out の結果 |
| `F32`・`F64` | `float`・`double` | `f32`・`f64` |
| `Void` | 結果の `void` | `unit`・out の結果 |

同じ native の symbol の宣言は、(a) import の module が同じ、(b) 下げた C の引数の数が同じ、(c) すべての位置と結果の分類が同じ、のとき許す。どれかが違えば `E1008`（仕様 4）。照合はプログラムのすべてのモジュールの、リンク名のある宣言の間で行う（届くかどうかに依らない）。

### 2. 生成

- 正規の型: symbol ごとに、位置（と結果）ごとに全宣言の LLVM 型が同じならその型、違えば（`ptr` と `i64` の混在だけがありうる）`i64`。宣言の順に依らない。
- `declare` は正規の型で 1 行だけ出す。呼び出しでは、その宣言の型が正規の型と違う位置の値を `ptrtoint ptr %v to i64` にし、結果が正規の `i64` で宣言の結果が `ptr`（ハンドル）なら `inttoptr` する。
- 宣言が 1 つか、すべて同じ LLVM 型の symbol は、今の IR と 1 byte も変わらない（正規の型が宣言の型そのもので、変換を出さない）。

### 3. WASM

- wasm32・wasm64 では、届いた（生成する）同じ symbol の宣言の Tsuzuri の型が一致しなければ `E1008`（kind `conflicting_extern_declaration`、文案 `extern symbol '<S>' is reached through declarations of different types; WebAssembly imports one signature per name, so use one declaration for this target`、`related` は先に届いた宣言）。共通契約の「分類が違えば `E1008`」はこれに含まれる（wasm32 の `ptr` と `i64` は分類が違い、Tsuzuri の型も違う）。分類が同じでも型が違えば拒否するのは、JS の糊（`src/bindings.rs` の `imports`）が import 名ごとに 1 つの記述子で値を変換するため（決定事項 D5）。
- 届く宣言が 1 つだけなら、ほかの宣言の型は問わない（未使用の extern は import を作らない今の規則と同じ）。

### 4. 診断

- 分類の違い: `E1008`、kind `conflicting_extern_declaration`（既存）、文案 `extern symbol 'SendMessageW' has a different C ABI here: parameter 4 is a 32-bit integer, but a 64-bit integer or pointer in the first declaration; declarations of one symbol may differ only in types that pass the same way`。引数の数の違いは `… takes 3 C parameters here but 4 in the first declaration …`、結果の違いは `the result is …`。位置は後の宣言の名前、`related` に `first_defined_here`（最初の宣言の名前）。
- import の module の違い: 今と同じ文・位置（`related` も足さない。1 byte も変えない）。

## 設計

- `src/abi.rs`: `pub enum CType { I32, I64, F32, F64, Ptr, Void }`、`pub enum AbiClass { I32, Word64, F32, F64, Void }`、`CType::class(self) -> AbiClass`（native の分類。WASM は仕様 3 で別に扱う）、`pub struct CSignature { parameters: Vec<CType>, result: CType }`、`pub fn c_signature(signature: &Signature, import: &HostImport, types: &TypeContext) -> CSignature`。規則は `host_call` と同じ（`host_call` 自身も、型の文字列をこの関数から作るように寄せてよい。そのときは S1 の比較で IR が変わらないことを確かめる）。
- `src/check.rs`: `explicit_symbols` の値を `(CSignature, wasm_module, 最初の宣言の名前の Span)` にし、仕様 1 の照合と仕様 4 の診断を出す。module の違いは今の比較を先に行う（今の文のまま）。全宣言を見た後、LLVM 型が違う位置を持つ symbol だけについて正規の `CSignature` を `CheckedModule.extern_abis: BTreeMap<String, CSignature>` に入れる（その種類の symbol がなければ空。ほかの経路は変わらない）。
- `src/llvm_imports.rs`: `host_call` は今どおり引数を組み立てた後、`self.module.extern_abis.get(&import.native_symbol)` があれば、型の違う位置の値を変換し、`declare` と `call` を正規の型で書き、結果を変換する。なければ今のまま。
- `src/llvm.rs`・`src/llvm_imports.rs`: `wasm` のとき、`host_call` は symbol ごとに最初に生成した宣言の Tsuzuri の関数型と関数の番号を `Globals` に記録し、型の違う宣言を生成したらその番号も記録する。`emit_program` は関数の生成の後、違いがあれば仕様 3 の `E1008` を返す（生成した宣言だけを見るので、届かない宣言は問わない）。WASM の `abi_native_only` の共有の口（W13・W15 の設計）と同じ所で検査する。
- `src/bindings.rs`: `imports` を「名前ごとの宣言の列」にするか、`signatures` が届いた extern の関数をすべて返すようにする（native の bindings が、2 つ目の宣言だけが使うハンドルの typedef を落とさないため）。JS の表は仕様 3 で型が 1 つに決まるので 1 項目のまま。

## 前提とする他チケットのインターフェース

- 依存はない。W13（`ref mut`・`@count`）、W15（`Maybe<H>`・`Maybe<ref R>`）、W20（`ref ZString`）、W21（`@c_layout` の record は `ptr`）、W22（`FnPtr` は `ptr`）のうち先に done のものは、その形を `c_signature` に入れる。後から入るチケットは自分の形を `c_signature` に足す（このチケットの「他チケットへ提供するインターフェース」。足し忘れると、その形を持つ宣言の照合が `host_call` の下げ方とずれる）。
- W16（`@last_error`）は C の結果を変えない（結果の組は Tsuzuri の側だけ）ので、`c_signature` の結果は宣言の組の 1 番目の C の型。

## 他チケットへ提供するインターフェース

- `abi::c_signature`・`CType`・`AbiClass`・`CSignature`（W13・W15・W16・W17・W20・W21・W22 が使い、形を足す）。
- `CheckedModule.extern_abis` と「symbol ごとに 1 つの正規の `declare`」: W17 は `dllimport` をこの 1 行に付ける。W16 がコンパイラの側で `GetLastError` を宣言するとき、利用者の `extern "GetLastError"` と同じ正規化に乗せる。W27 は 1 つの関数に複数の宣言を出せる。

## 実装手順

1. **変更**: なし（下書きは `target/scratch/w14/`）。**内容**: `node scripts/repo-kb.mjs brief W14`、着手条件の `grep`。`tests/fixtures`・`examples` の全 `Main.tz` の `--emit llvm -O1`（native と `--target wasm32`）を保存する。**確認**: 保存した数を記録する。
2. **変更**: `src/abi.rs`・`src/check.rs`。**内容**: `c_signature` と照合・診断・`extern_abis`。**確認**: T0、`cargo test --locked --test ffi_extensions`（照合のテスト）、T1。
3. **変更**: `src/llvm_imports.rs`。**内容**: 正規化と変換。**確認**: `same_symbol_with_one_c_abi_is_one_declare`・`canonical_declare_ignores_order`、手順 1 の保存物との比較（S1）、T1。
4. **変更**: `src/llvm.rs`・`src/bindings.rs`。**内容**: WASM の検査と `imports`。**確認**: `wasm_rejects_differently_typed_reached_declarations`、`cargo test --locked --test bindings`、T1。
5. **変更**: `tests/ffi_extensions_host.c`・`tests/ffi_extensions.mjs`。**内容**: E2E。**確認**: `cargo build && node tests/ffi_extensions.mjs target/debug/tsuzuri` を `-O0`・`-O3`・`--backend fast` で。
6. **変更**: 文書・`spec/rules/ABI.md`・`src/explain/codes/E1008.md`。**確認**: T2、`node scripts/check-docs.mjs <変更したページ>`。PR に `ci:full` を付ける。

## テスト計画

| Rust（`tests/ffi_extensions.rs`） | 内容 |
| --- | --- |
| `same_symbol_with_one_c_abi_is_one_declare` | 早見の例が通り、IR に `declare i64 @SendMessageW(ptr, i32, i64, i64)` が 1 行、`send_rect` の呼び出しに `ptrtoint`。`SetWindowLongPtrW` も 1 行 |
| `canonical_declare_ignores_order` | 宣言の順を入れ替えても `declare` の行が同じ。2 回の `emit` が一致する |
| `same_types_keep_the_ir` | 同じ型の 2 つの宣言の IR が、宣言 1 つのときの IR と同じ（今の挙動の固定） |
| `class_mismatches_are_rejected` | 位置の `i32` と `i64`、`f64` と `i64`、`f32` と `i32`、バッファ（`ptr`・`i64` の 2 つ）とスカラー 1 つ、結果の `unit` と `i64` が `E1008` `conflicting_extern_declaration`。`related` に `first_defined_here` がある |
| `same_class_is_accepted` | `bool` の結果と `i32` の結果、`i8u` と `i32` の引数、コールバックと `i64`、out の record の結果（先頭の `ptr`）と先頭の `i64` |
| `module_mismatch_keeps_its_message` | `extern "a" "f"` と `extern "b" "f"` の文と位置が今と同じ |
| `wasm_rejects_differently_typed_reached_declarations` | `llvm::emit_target(…, true)` で、型の違う 2 つが届けば `E1008`、1 つだけ届くか同じ型なら成功 |

E2E（`tests/ffi_extensions.mjs`、native、`-O0`・`-O3`、`--backend fast`）: ホストの C の `int64_t ffi_word(int64_t kind, int64_t value)`（`kind` が 0 なら `value + 1`、1 なら `value` を `const int32_t *` として先頭の要素を返す）を、`word_int :: i64 -> i64 -> i64` と `word_rect :: i64 -> ref Rect -> i64` の 2 つの宣言で呼ぶ。期待値は `42 → 43` と `Rect { left: 7, … } → 7`（C の側の定義から決まる値）。Windows（W06 が done なら `tests/windows.mjs`）: メッセージ専用の窓に `WM_SETTEXT`（W20 が done なら `ref ZString`）と `WM_GETTEXTLENGTH`（整数）を同じ `SendMessageW` の 2 つの宣言で送る。

## ドキュメント

- docs/language.md「リンク名」の重複宣言の段落: C の ABI の分類で照合すること、分類の表、`declare` の正規化、WASM の規則。
- `_tsuzuri/language-reference/compiler/native-interop.md` 178 行の文と、用途ごとの宣言の例（`SendMessageW`）。`src/explain/codes/E1008.md` の「What it covers」と Fixes の "Declare each extern symbol once with one signature" を改める。
- `spec/rules/ABI.md`（番号は着手時の最大の次）: 「Declarations of one extern symbol MUST agree on the import module, the number of C parameters, and the C ABI class of every parameter and of the result; they MAY differ in Tsuzuri types (E1008 otherwise).」「The IR MUST declare an extern symbol once.」と適合性テスト。
- `_tsuzuri/language-reference/compiler/diagnostics.md` は kind を足さないので表は変わらない（文の例があれば直す）。

## 受け入れ条件

- [ ] 仕様 1〜4 のとおりに動く（native の `-O0`・`-O3`、`--backend fast`、wasm32）。
- [ ] 宣言が 1 つか同じ型だけの symbol を持つプログラムの IR・object・header・診断が、手順 1 の保存物と 1 byte も変わらない。WASM の既存の出力も変わらない。
- [ ] 型の違う宣言の拒否は、分類が違うときだけ新しい文（module の違いは今の文）。
- [ ] 文書・`spec/rules/ABI.md`・explain を更新した。T2 が通り、PR に `ci:full` を付けた。

## 落とし穴

- 正規の型を「届いた宣言」から決めると、使う関数の組み合わせで `declare` の行が変わる。全宣言から決める（D4）。
- `intrinsics` は宣言行の文字列の集合なので、正規化を忘れた経路が 1 つでもあると同じ symbol の 2 行が残る。`host_call` 以外に `declare` を出す経路（W16・W17 が足すもの）も `extern_abis` を通す。
- コールバックを取る extern は呼び出し側で `host_call` を出す（`src/llvm.rs` の `Call` の分岐）。その経路の変換もテストする。
- `with_blocking_externs`（tzroutine のプログラム）は `call` の直接の呼び先の symbol で照合する。正規化しても symbol は同じなので変わらない。
- ヘッダーはリンク名のある extern のプロトタイプを出さない（今どおり）ので、ヘッダーに正規化は要らない。

## 対象外

- 型による多重定義（同じ Tsuzuri の名前の複数の型）。用途ごとに別の名前で宣言する（D7）。
- リンク名のない extern（名前が一意なので重複しない）、可変長引数、値渡しの struct の分類（records は今どおりポインター）。
- 32-bit の native（ターゲットがない）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | 同じ symbol の宣言の照合を、Tsuzuri の関数型の一致から、下げた C の引数の数と位置ごとの ABI の分類の一致に改める（E12 D2 の「関数型と module が一致すれば許す」の改訂。module の一致は残す） | E12 D2 の理由は「`declare` の型が違うと LLVM が拒む」で、正規化すればその理由は消える。宣言の正しさは今も `@trusted` の責任（D-63 の背景 1）で、分類が同じなら C から見た呼び出しは同じ | 要承認（承認前は手順 2 以降に着手しない） |
| D2 | 分類は `I32`（32-bit 以下の整数と `bool`。符号を問わない）・`Word64`（`i64`・`i64u`・ポインター）・`F32`・`F64`・`Void` | native はすべて 64-bit のポインターで、x86-64（SysV・Win64）と AArch64 はポインターと 64-bit の整数を同じレジスターで渡す。32-bit 以下は `abi_type` がすでに `i32` に揃えている | 既定案（実装者はこの案に従う） |
| D3 | `declare` は symbol ごとに 1 行。型の混ざる位置は `i64` にし、呼び出し側で `ptrtoint`・`inttoptr` する | 宣言と違う関数型での呼び出し（opaque pointer では書ける）は LLVM の LangRef で未定義で、最適化が呼び出しを壊しうる。変換を明示すれば IR は検証を通る | 既定案（実装者はこの案に従う） |
| D4 | 正規の型は、届くかどうかに依らずプログラムの全宣言から、宣言の順に依らない規則で決める | IR を決定的にする（GUIDE §1 規則 3）。使う関数を変えても `declare` が変わらない | 既定案（実装者はこの案に従う） |
| D5 | WASM では、届いた同じ symbol の宣言の Tsuzuri の型が一致しなければ `E1008`（今の規則を届いた宣言に当てる） | JS の糊の記述子は import 名ごとに 1 つで、型の違う宣言の値を変換できない。wasm32 では `ptr` と `i64` の分類も違う | 既定案（実装者はこの案に従う） |
| D6 | 診断は既存の kind `conflicting_extern_declaration` に、位置と分類を示す文と `first_defined_here` を足す。module の違いの文は変えない | 同じ「宣言の衝突」の分類で、kind を増やさない（D-51）。module の違いの診断は意味が変わらないので 1 byte も変えない | 既定案（実装者はこの案に従う） |
| D7 | 構文は足さない（用途ごとに別の名前の `extern` を書く） | 言語は型による多重定義を持たない（docs/language.md 445 行付近）。名前で用途が読める | 既定案（実装者はこの案に従う） |
