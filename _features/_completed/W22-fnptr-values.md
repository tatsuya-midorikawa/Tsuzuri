# W22: 静的な関数ポインターの値 `FnPtr<F>`

| 項目 | 内容 |
| --- | --- |
| ID | W22 |
| 種別 | 機能 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | [W12](W12-static-callback-lifetime.md)（静的コールバックの寿命の契約） |
| 後続 | [W21](W21-c-layout-records.md)（Phase 2 の `FnPtr` の field）、[W26](W26-com-objects-in-tsuzuri.md)、[W27](W27-win32-package-from-winmd.md) |
| 状態 | done |
| 起票 | 2026-11-22（D-63。利用者の依頼「X: D13 を狭く改める方針で決定。実現向けたチケットを @_features に W プリフィックスのチケットを作成して」と「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」から起票。HEAD `fa40eede` で確認） |
| 承認 | 要承認（D-55）。D1 は要承認、D2〜D7 は既定案。台帳は [GUIDE D-63](GUIDE.md#d-63-tsuzuri-だけで-win32-のアプリを書くe12-d13-の狭い改訂w01w30) |
| 手本にする既存実装 | コールバックの下げ方: `src/llvm_imports.rs` の `FunctionEmitter::host_call`（`ptr @tz.callback.<name>` と `Globals::callbacks`）、`src/llvm_abi.rs` の `wrapper`（`WrapperKind::Callback`）。実引数の検査: `src/check.rs` の `validate_callbacks`（`plain_function`・`direct_only`）。組み込みの型の名前: `src/check.rs` の `resolve_type_with_kinds` の `Vec`・`SharedKind::named` の腕、`builtin_type_head`、`reserved_type_name`。ABI の判定: `src/abi.rs` の `callback`・`abi_scalar`・`parameter`・`result` |
| 主な影響ファイル | `src/check.rs`・`src/abi.rs`・`src/llvm.rs`・`src/llvm_imports.rs`・`src/llvm_abi.rs`・`src/llvm_debug.rs`・`src/polymorph.rs`・`src/stdlib.rs`（`RESERVED_MODULES`）・`src/effects.rs`・`src/pure_rules.rs`・`src/kg/effects.rs`・`src/driver.rs`・`src/diagnostic_kinds.rs`・`Type::Handle` を名指す 23 ファイル（下の「設計」）・`tests/fnptr.rs`（新規）・`tests/fixtures/callback_lifetime/`（W12 の fixture に足す）・`tests/windows.mjs`・文書 |

## 実装者向け早見

- 範囲: 組み込みの型 `FnPtr<(A -> B -> R)>` と、値を作る唯一の方法 `FnPtr.of name`。値は W12 の静的コールバックと同じ包み `@tz.callback.<name>` のアドレスで、Copy・Send・Sync。`extern` の引数に渡せ、Tsuzuri のデータ構造（record、配列、`Maybe`）に入れられる。Tsuzuri からは呼べない。ホストから受け取れない。native だけ。
- 最初に読む: ① 「現状」 ② 仕様 1〜4 ③ `validate_callbacks` ④ W12 の「他チケットへ提供するインターフェース」。
- 最初に書くテスト: `tests/fnptr.rs` の `fnptr_and_callback_argument_share_one_wrapper`（同じ関数をコールバックの引数と `FnPtr.of` に使うと、`define … @tz.callback.Main.f` が 1 つで、2 つの呼び出しの引数が同じ `ptr @tz.callback.Main.f`）。
- 触ってはいけない: 既存のコールバックの引数（関数型の引数、直接・全引数の呼び出し）の検査と IR。`FnPtr` を使わないプログラムの IR・header・診断。WASM の出力。
- 完了の見分け方: 下が native で 42 を返し（ホストは保持したポインターを後で呼ぶ）、`--target wasm32` で `E1008`。

```tsuzuri
@trusted
extern "w_register" def register :: FnPtr<(i64 -> i64)> -> unit
@trusted
extern "w_fire" def fire :: i64 -> i64
def triple :: i64 -> i64 = \x -> x * 3
record Handlers { on_value: FnPtr<(i64 -> i64)>, code: i32 }
def main :: unit -> i32 = \() ->
    let handlers = Handlers { on_value: FnPtr.of triple, code: 1 }
    let later = register          // FnPtr を取る extern は普通の関数値
    later handlers.on_value
    (fire 14) as i32
```

## 目的

Win32 は関数ポインターを値として扱う: `WNDCLASSEXW.lpfnWndProc` は構造体の field（W21 Phase 2）、`SetWindowLongPtrW(hwnd, GWLP_WNDPROC, …)` は整数の位置（W14 の複数宣言で `FnPtr` の型を載せる）、メッセージごとの処理の表は Tsuzuri の値である。今のコールバックは「コールバック extern の直接の呼び出しの実引数に、関数の名前を書く」形しかない。静的な寿命（W12）を持つ包みのアドレスを、型の付いた値として扱えるようにする。

## 着手条件と停止条件

### 着手条件

- W12 が done（`grep -nE "^\| W12 " _features/README.md`）。W12 の契約がなければ、保持される値を出す根拠がない。
- `grep -rn "Type::Handle" src/ | wc -l` で、型の網羅的な `match` の数を控える（HEAD では 23 ファイル・50 か所）。

### 停止条件

- **S1** `Type` に variant を足すと 4 語を超える（`Type::FnPtr(Box<Type>)` は 1 語の箱なので超えないはず）。
- **S2** `FnPtr` を使わないプログラムの IR・header・診断が 1 byte でも変わる。
- **S3** `FnPtr.of` の値のために、包みを `WrapperKind::Callback` と別の形で出したくなった（W12 の「包みは 1 種類」に反する）。
- **S4** 型 `FnPtr` の予約で、`tests/` や `examples/` の既存のプログラムが `E1001` になる（`type FnPtr` や `record FnPtr` がある）。
- **S5** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

- コールバックは引数の位置にしか書けない: `src/check.rs` の `validate_callbacks` は、コールバックを取る extern（`HostImport.callbacks`）の呼び出しに全引数を求め（`extern_callback_call_form`）、関数型の位置の実引数に `plain_function`（利用者のモジュールのトップレベル、型引数なし、`HostCall` でない）を求める（`extern_callback_argument`）。開発版で確認: `let f = triple` の後の `register f` は `E1008`（`extern_callback_argument`）、`let r = register` は `E1008`（`extern_callback_call_form`）。
- 下げ方: `FunctionEmitter::host_call` が関数型の実引数を `ptr @tz.callback.<修飾名>` にし、`Globals::callbacks`（`BTreeSet<usize>`）へ id を足す。包みは emit の最後に id 順で出る。関数値は `%tz.closure { ptr, ptr }` で、C から呼べるものではない。
- ABI: `src/abi.rs` の `callback` は、引数が `abi_scalar`（数・`bool`・ハンドル・`ref` ハンドル。唯一の `unit` は引数なし）で結果が数か `unit` の関数型だけを受ける。`result` は関数型を受けない。record の field に関数型を書いた `ref Class` の extern は `E1008`（`unsupported_extern_signature`。開発版で確認）。E12 の D5・D11 がハンドルと関数ポインターを ABI の record の field から外している。
- ヘッダー: `extern def register_proc :: (ref Hwnd -> i32u -> i64u -> i64 -> i64) -> unit`（リンク名なし）は `void tsuzuri_host_Main_register_proc(int64_t (*arg0)(tz_handle_4Main_4Hwnd, uint32_t, uint64_t, int64_t));` になる（開発版で確認。WNDPROC の x64 の形と同じ）。
- 名前: `type FnPtr = i64` は今 `check` を通る（開発版で確認）。`reserved_type_name` は `Array`・`Vec`・`Rc`・`Arc` と原始型と組み込みの型クラスだけを予約している。
- `--trap-mode return`（`src/driver.rs`）と `Domain.run`（`src/llvm.rs` の `domain_without_callbacks`）は IR の `@tz.callback.` を見て拒否し、`with_blocking_externs` も ` @tz.callback.` の `define` を見る（W12 の印も包みに入る）。
- target に依存する `E1008` は HEAD にない。`check` は target を取らない（`tsuzuri check --help` に `--target` がない）。

## 仕様

### 1. 型と値

- 型 `FnPtr<F>`。`F` は関数型で、`abi::callback` を満たす（W13・W15・W20 がコールバックの引数の型を広げたら、そのまま従う）。`F` に型変数を含む `FnPtr` は `E1015`（extern の署名と同じ）。`FnPtr` に型引数がない・2 つ以上は `E1004`（`type_argument_count`）。関数型でない `F`、または C から呼べない関数型の `F` は `E1008`（kind 案 `fnptr_signature`）。
- 型引数の比較は C の署名だけで行う。`FunctionParameters` のうち `types`（`ref` は `Type::Reference` としてここにある。`abi::callback` もここだけを見る。HEAD で確認）と結果は残し、`pure` の印は落とす（`FnPtr.of` に `pure def` を渡しても、注釈 `FnPtr<(i64 -> i64)>` と同じ型）。`conventions`（`Own`・既定の読み・edition 2027 の `mut T` の `Mut`。`src/check.rs` の `Scheme::parameter_conventions` を作る所で確認）は C の署名を変えないが、包みが `Mut` の引数をどう渡すかは未確認なので、着手時に今のコールバックの引数（extern の関数型の引数と実引数の関数の型を合わせる単一化）と同じ規則にそろえる: 今のコールバックが受ける組み合わせだけを受け、落とすか比べるかもそれに従う。
- 値は `FnPtr.of name` だけで作る。`name` は W12 のコールバックの実引数と同じ条件（`plain_function`）のトップレベルの利用者関数で、型は `FnPtr<その関数の型>`。ラムダ、局所の束縛、部分適用、std・組み込み・extern の関数、型引数を持つ関数は `E1008`（kind 案 `fnptr_argument`。文は `extern_callback_argument` と同じ）。`FnPtr.of` を全引数で直接呼ばない（関数値にする、パイプで渡す）と `E1008`（kind 案 `fnptr_call_form`）。
- 性質: Copy（ビットの複製）・Send・Sync・関数値に捕捉できる。`Eq`・`Ord`・`Hash`・`Display` の instance はない。`const` の初期化には書けない（`E1026`、kind 案 `const_fnptr`）。

### 2. 置ける場所（ABI の向き）

| 位置 | 扱い |
| --- | --- |
| `extern` の引数（値 `FnPtr<F>`） | 可。C では `F` の関数ポインター（コールバックの引数と同じ型）。この extern は「コールバックを取る extern」ではないので、関数値にも部分適用にもできる |
| W21 Phase 2 の `@c_layout` の field で、Tsuzuri からホストへ渡す向き（`ref R` の引数） | 可（W21 が実装する。W22 は型の情報を渡す） |
| `extern` の結果、`ref mut` の copy-out（W13）、`export def` の引数と結果、コールバックの引数と結果 | `E1008`（kind 案 `fnptr_from_host`。ホストが作った関数ポインターを `FnPtr` として受けると、包みでない番地を型で偽れる。外部の関数ポインターは W25） |
| Tsuzuri の中（局所、record、union、配列、`Maybe`、関数値の捕捉） | 可（ABI の外。ふつうの Copy の値） |

### 3. 呼び出しと変換

- Tsuzuri から `FnPtr` を呼ぶ方法はない（関数の名前で直接呼べばよい）。`p 1` は `E1005`（`call_non_function`）で、help に「`FnPtr` は Tsuzuri から呼べない。関数を名前で呼ぶ」を足す。
- 整数との変換、比較は足さない（対象外）。

### 4. target と寿命

- native だけ。WASM の build（`--target wasm32`・`wasm64`、`tsuzuri test --target wasm32` を含む）で `FnPtr.of` に到達すると `E1008`（kind 案 `abi_native_only`。位置は最初の `FnPtr.of`）。`check` は target を知らないので出さない。
- 寿命と呼ばれ方は W12 の仕様 1 の契約に従う（保持してよい。実行ファイルでは `main` の境界の終わりまで、ライブラリではプロセスの寿命の間、どのスレッドからでも）。

## 設計

- **型**: `src/check.rs` の `Type` に `FnPtr(Box<Type>)`（中身は正規化した `Type::Function`）。`resolve_type_with_kinds` に `Apply(head, args) if head.text == "FnPtr"` と `Named("FnPtr")` の腕（`Vec`・`SharedKind::named` の腕と同じ形）、`builtin_type_head` と `reserved_type_name` と、`collect` の重複の検査の 3 つの一覧（`"Array" | "Vec" | "Rc" | "Arc"`）に `FnPtr`。`is_copy`・`can_send`（すべての種類を名指す `match`）・`sync_here`・`can_capture` は真、`is_frozen` は不変のデータとして扱う。`Type::Handle` を名指す 23 ファイル（`src/abi.rs`・`anonymous_union.rs`・`bindings.rs`・`bindings_native.rs`・`check.rs`・`const_rules.rs`・`const_static.rs`・`find.rs`・`higher_kinds.rs`・`kg/ids.rs`・`kg/layout.rs`・`llvm.rs`・`llvm_abi.rs`・`llvm_debug.rs`・`llvm_escape.rs`・`llvm_facts.rs`・`llvm_imports.rs`・`llvm_layout.rs`・`polymorph.rs`・`prove_smt.rs`・`pure_rules.rs`・`semantic.rs`・`type_arena.rs`）の `match` をすべて確かめる（GUIDE §6.9）。型の表示は `FnPtr<(i64 -> i64)>`。
- **組み込み**: `Builtin::FnPtrOf`（`name` は `FnPtr.of`、型 `'f -> FnPtr<'f>`）。`Builtin::ALL`・`name`・`signature`・生成・効果の表（A19・A20。純粋、確保なし、トラップなし、権限なし）。`FnPtr` を `src/stdlib.rs` の `RESERVED_MODULES` へ（std のファイルを持たない組み込みのモジュール。`Routine` と同じ）。
- **検査**: `validate_callbacks` の `plain_function`・`direct_only` を外へ出して共有し、`FnPtr.of` の呼び出しを検査する。`validate_callbacks` はコールバックを取る extern が 1 つもなければ先頭で戻る（`if !(0..module.functions.len()).any(is_callback_extern) { return Ok(()) }`）ので、`FnPtr.of` の検査はこの早い戻りの外に置く（`FnPtr` を取る extern はコールバック extern ではない。D6）。関数の id の一覧（`FnPtr.of` の対象）を走査で数えられるようにする（W19 が固定された文脈の根に使う）。
- **ABI**: `src/abi.rs` に `fnptr(ty)`（`Type::FnPtr` で中身が `callback`）。`parameter` は `fnptr` を受け、`result` と `callback` の引数と結果、`export` の判定は受けない（`fnptr_from_host`）。`abi_type` は `ptr`、`c_type` はコールバックの引数と同じ関数ポインターの宣言子（W21 の field も使う）。大きさと整列はポインター（native の 64-bit で 8）。
- **生成**: `FnPtr.of name` の式は `self.globals.callbacks.insert(id)` をして値 `ptr @tz.callback.<修飾名>` を返す（`host_call` の関数型の実引数と同じ 2 行）。`llvm_type` は `ptr`。`host_call` は `Type::FnPtr` の実引数をそのまま `ptr` で渡す。包みは W12 のまま 1 種類で、`main` の後の検査、コールバックの境界、P の取り戻し、`--trap-mode return`・`Domain.run` の拒否（`@tz.callback.` を見る）がそのまま効く。拒否の文の「extern callbacks」は「extern callbacks or FnPtr values」に直す。
- **WASM の拒否**: `src/driver.rs` の、`--trap-mode return` と tzroutine の拒否（`llvm::spawns`）と同じ段で、`options.target.is_wasm()` かつ到達する関数が `FnPtr.of` を持てば `E1008`。位置を返す関数は `crate::check::domain_calls` と同じ形で `check.rs` に足す。

### 採らなかった案

| 案 | 採らない理由 |
| --- | --- |
| 関数値（`%tz.closure`）を ABI で関数ポインターに変える | 捕捉のある値は文脈が要り、寿命の契約が要る（E12 Phase 2） |
| コールバックの引数の位置を record の field などへ広げる（値の型を足さない） | `SetWindowLongPtrW` の整数の位置や、Tsuzuri の表に入れる用途を表せない |
| `FnPtr.call` で Tsuzuri から呼ぶ | 包みを通して C ABI に入り直すだけで、名前で呼ぶのと同じ。外部の関数ポインターの呼び出しは W25 の `extern interface` の範囲 |
| ホストから `FnPtr` を受け取る | 包みでない番地を型で偽れる。受けるなら別の型（W25） |
| std の不透明な record（`record FnPtr<'f> { address: i64 }`）で表す | ABI で record を特別扱いすることになり、wasm32 のポインター幅とも合わない。組み込みの型の方が `abi_type`・`c_type` の分岐が素直 |

## 前提とする他チケットのインターフェース

- W12: 仕様 1 の契約、「包みは 1 種類」（`Globals::callbacks` に id を足して `wrapper` を使う）、`@tz.callback.<修飾名>` の名前。

## 他チケットへ提供するインターフェース

- `Type::FnPtr(Box<Type>)`、`abi::fnptr`、`FnPtr` の `c_type`（関数ポインターの宣言子）と `abi_type`（`ptr`）、大きさ・整列（W21 Phase 2 の `@c_layout` の field、ヘッダーの `typedef struct` の field）。
- 向きの規則: `FnPtr` を含む型は Tsuzuri からホストへの向きだけ（W21 は `@c_layout` の結果と copy-out に `FnPtr` の field を持つ record を `fnptr_from_host` で拒む）。
- `FnPtr.of` の対象の関数の一覧（W19 の `pinned` の根、W26 の vtable の合成）。W19 が先に入っていれば、`FnPtr.of` を W19 の `pinned_rules::builtin_position` に `Pinning`（コールバックの位置と同じ）で分類する。W26 は `F` に使える型を W26 の仕様の分だけ広げ、`FnPtr` を Tsuzuri の record（`I.Methods`）の field に置く（ABI の外なので仕様 2 の範囲内）。

## 実装手順

1. **変更**: なし（ブランチ `feat/w22-fnptr`）。**内容**: `node scripts/repo-kb.mjs brief W22`、着手条件。`tests/fixtures/*`・`examples/*` の `--emit llvm`（native と wasm32）と `--emit header` を保存する。**確認**: `grep -rnwE "FnPtr" tests examples` が 0 件（S4）。
2. **変更**: `src/check.rs`（型、名前の予約、`Builtin::FnPtrOf`、検査）、型の `match` の全か所。**内容**: 型と検査だけ。この段のテストは `analyze` の受理・拒否だけを見る（生成は手順 3）。**確認**: `cargo test --locked --test fnptr`（型・拒否のテスト）、`cargo nextest run --locked`。
3. **変更**: `src/abi.rs`・`src/llvm.rs`・`src/llvm_imports.rs`・`src/llvm_abi.rs`・`src/llvm_debug.rs`。**内容**: ABI と生成とヘッダー。**確認**: `tests/fnptr.rs` の IR とヘッダーのテスト。手順 1 の IR・header と `cmp` で一致。
4. **変更**: `src/driver.rs`。**内容**: WASM の拒否と、`--trap-mode return`・`Domain.run` の文。**確認**: `tests/fnptr.rs` の CLI のテスト（`env!("CARGO_BIN_EXE_tsuzuri")`）。
5. **変更**: `tests/fixtures/callback_lifetime/`（W12 の fixture に `FnPtr` のケース）、`tests/tzroutine.mjs`、`tests/ffi_extensions.mjs`、`tests/windows.mjs`。**確認**: 各 suite（「テスト計画」）。
6. **変更**: 効果の表と件数の pin（A19・A20、KG、`Builtin::ALL` の長さ、予約名の数は着手時の現在値から数える。GUIDE-LITE §11）、文書。**確認**: T2、`node scripts/check-docs.mjs <変更したページ>`、`node scripts/check-cli-docs.mjs`。PR に `ci:full`。

## テスト計画

| 置き場 | ケース | 期待 |
| --- | --- | --- |
| `tests/fnptr.rs`（cargo） | `fnptr_and_callback_argument_share_one_wrapper` | 包みの `define` が 1 つ、2 つの呼び出しが同じ定数を渡す |
| 同上 | `fnptr_is_checked_without_callback_externs`: コールバックを取る extern が 1 つもないプログラムで `FnPtr.of (\x -> x)` と `FnPtr.of` の関数値 | `E1008`（`fnptr_argument`・`fnptr_call_form`）。生成まで届かない |
| 同上 | 受理: record・配列・`Maybe` の中の `FnPtr`、関数値への捕捉、`FnPtr` を取る extern の部分適用、`pure def` の `FnPtr.of` と注釈の一致 | 受理、IR が決定的 |
| 同上 | 拒否（コードと kind を照合）: ラムダ・局所・部分適用・std・extern・ジェネリック関数の `FnPtr.of`（`E1008`/`fnptr_argument`）、`FnPtr.of` の関数値（`fnptr_call_form`）、`FnPtr<i64>`・`FnPtr<(string -> i64)>`（`fnptr_signature`）、`FnPtr`・`FnPtr<A, B>`（`E1004`）、`FnPtr<('a -> 'a)>`（`E1015`）、extern の結果・export の引数と結果・コールバックの引数（`fnptr_from_host`）、`p 1`（`E1005`）、`const`（`E1026`）、`record FnPtr`（`E1001`） | 各コード |
| 同上 | ヘッダー: リンク名のない extern の `FnPtr<(ref Hwnd -> i32u -> i64u -> i64 -> i64)>` の引数 | 「現状」のコールバックの引数と同じ宣言子 |
| 同上（CLI） | `build --target wasm32` と `tsuzuri test --target wasm32` | `E1008`/`abi_native_only`。`check` は 0 |
| 同上 | `FnPtr` を使わない fixture の IR・header | 手順 1 と一致 |
| `tests/fixtures/callback_lifetime` + `tests/tzroutine.mjs`（ci の suites。POSIX） | 早見の例（record から取り出して部分適用した extern に渡し、ホストが後で呼ぶ）、表から選んだ `FnPtr` を別のスレッドが呼ぶ、tzroutine を起動するプログラムで `FnPtr` 経由の呼び出しが W12 の境界になる | 42、ホストの参照値、子が終わってから戻る |
| `tests/windows.mjs`（windows.yml W5。W06 の段） | W06 の shim `tests/windows_gui_shim.c` に `FnPtr` を取る版の `tz_gui_register_class`（W12 が足す関数の引数を `FnPtr<…>` にしたもの）を足し、`RegisterClassExW` の `lpfnWndProc` に入れ、Tsuzuri から窓を作ってメッセージを送る（fixture は `tests/fixtures/windows_gui/`、`buildGui`） | メッセージの数がホストの参照値と一致 |

`-O0`・`-O3`。期待値はホストの C の参照実装と手計算から作る（GUIDE-LITE §2 規則 10）。

## ドキュメント

- 新規 `_tsuzuri/language-reference/built-in-types-and-modules/fnptr.md`（型、`FnPtr.of`、置ける場所の表、Tsuzuri から呼べない理由、WASM、「引数の順序」の節: 対象を持たない 1 引数の関数だけ）。`index.md`。
- `native-interop.md` の「静的コールバック」節に「関数ポインターの値」の項。`docs/language.md` の「コールバック」節と予約名の表。`organizing-tsuzuri/modules.md` の予約名の数（着手時の現在値から）。
- `compiler/diagnostics.md` と `docs/language.md` の診断の表（新しい kind）（kind の表は `<!-- generated:diagnostic-kinds -->` の生成の部分なので手で書かず、`src/diagnostic_kinds.rs` に登録してから `node scripts/gen-docs.mjs`）。`docs/architecture.md`（包みの共有）。
- `spec/rules/ABI.md`: 着手時の番号の次に「A `FnPtr` value MUST lower to the address of the same wrapper as a callback argument naming the same function.」「A `FnPtr` MUST NOT be received from the host.」。
- release notes に新しい予約名 `FnPtr`（型とモジュール）。

## 受け入れ条件

- [ ] 仕様 1〜4 のとおりに動き、「テスト計画」が通る（`-O0`・`-O3`）。
- [ ] `FnPtr` を使わないプログラムの IR・object・header・診断、WASM の出力が手順 1 と byte 単位で一致する。
- [ ] 同じ関数のコールバックの引数と `FnPtr.of` は同じ包みを使い、IR が決定的。
- [ ] 文書・診断の表・規則 ID を更新し、`check-docs`・`check-cli-docs`・`gen-docs --check` が通る。
- [ ] PR に `ci:full` を付け、`windows.yml` が緑。

## 落とし穴

- **正規化を忘れない**: `Type::Function` の `FunctionParameters` は `pure` と `conventions` を持つ。`FnPtr` の型引数で比べると、`pure def` を渡した `FnPtr` が注釈と一致しない。
- **`FnPtr` を取る extern は `HostImport.callbacks` を立てない**: 立てると `validate_callbacks` が部分適用を拒み、早見の例が通らない。その代わり `validate_callbacks` の早い戻りに `FnPtr.of` の検査が巻き込まれないようにする（コールバック extern のないプログラムでラムダの `FnPtr.of` が生成まで届き、`unreachable!` か不正な IR になる）。
- **網羅的な `match`**: `can_send` はすべての種類を名指す `match` なので足し忘れはコンパイルエラーになるが、`_ =>` の `match`（`can_capture`・`is_copy` など）は黙って通る。`Type::Handle` の 50 か所を 1 つずつ見る。
- **包みの到達**: 包みは `Globals::callbacks` に入ったものだけが出る。`FnPtr.of` を特殊化の中で使っても、emit の最後の id 順の生成に入ること（特殊化の後に包みを出す今の順序を変えない）。
- **ヘッダーの重複**: リンク名を持つ extern はヘッダーに出ない（今どおり）。`FnPtr` の `typedef` を足さない（宣言子を引数の中に書く今の形）。

## 対象外

- C から返る関数ポインター、外部の関数ポインターの呼び出し（W25）。
- `FnPtr` と整数の変換、比較、`Eq`（使い道は W14 の複数宣言で足りる。必要になったら別チケット）。
- WASM の `FnPtr`（保持できる WASM のコールバックと一緒に設計する）。
- `export def` の結果としてホストへ `FnPtr` を返すこと（必要になったら向きの規則の範囲で別に足す）。
- `const` の `FnPtr`。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | 組み込みの型 `FnPtr<F>` とモジュール `FnPtr`（`FnPtr.of`）を足す。型の名前 `FnPtr` を `reserved_type_name` に、モジュール名を `RESERVED_MODULES` に予約する | D-55 の名前の割り当て。今 `type FnPtr = i64` を書いた利用者のプログラムは `E1001` になる | 要承認（承認前は手順 2 に着手しない） |
| D2 | 値は `FnPtr.of name` だけで作り、名前の条件は W12 のコールバックの実引数と同じ。全引数の直接の呼び出しだけ | 包みは静的な関数ごとに 1 つで、寿命の契約（W12）がそのまま効く。ラムダや局所を通すと捕捉のある値が入りうる | 既定案（実装者はこの案に従う） |
| D3 | Tsuzuri からは呼べない | 名前で呼べば同じで、C ABI に入り直す意味がない。外部の関数ポインターは W25 | 既定案（実装者はこの案に従う） |
| D4 | Tsuzuri からホストへの向きだけ（extern の引数、`@c_layout` の入力の field）。受け取る向きは `E1008` | 受け取れると、包みでない番地に `FnPtr` の型が付く | 既定案（実装者はこの案に従う） |
| D5 | native だけ。WASM の build は `E1008`（`abi_native_only`）を driver で出す | WASM のコールバックは import の呼び出しの間だけ（W12 D6）で、保持する用途と合わない。`check` は target を知らない | 既定案（実装者はこの案に従う） |
| D6 | `FnPtr` を取る extern は「コールバックを取る extern」ではない（関数値・部分適用にできる） | 引数は普通の Copy の値で、呼び出し側で静的に決める必要がない | 既定案（実装者はこの案に従う） |
| D7 | 整数との変換・比較・`const` は入れない | 今の用途（W14・W21・W26）に要らない。番地の同一性を約束しない | 既定案（実装者はこの案に従う） |
