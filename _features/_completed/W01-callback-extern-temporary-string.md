# W01: コールバックを取る `extern` に文字列の一時値を渡すと、`check` は通るのに不正な IR になる

| 項目 | 内容 |
| --- | --- |
| ID | W01 |
| 優先度 | P1（`check` が通るプログラムが `build`・`run`・`test` のすべてで `E2002` になる。Win32 の `CreateWindowExW` などは、コールバックと文字列を同じ呼び出しで渡すことが多い） |
| 規模 | S |
| 依存 | – |
| 後続 | [W27](../W27-win32-package-from-winmd.md)（生成した宣言は文字列のリテラルをそのまま渡す書き方を前提にする） |
| 状態 | done |
| 起票 | 2026-11-22（D-63。利用者の依頼「X: D13 を狭く改める方針で決定。実現向けたチケットを @_features に W プリフィックスのチケットを作成して」と「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」から起票。HEAD `fa40eede` で確認） |
| 承認 | 不要（[E12](E12-ffi-extensions.md) D9「呼び出し側で `host_call` を直接出す」に実装を合わせる修正。言語の意味・診断・ABI は変えない。台帳は [GUIDE D-63](../GUIDE.md#d-63-tsuzuri-だけで-win32-のアプリを書くe12-d13-の狭い改訂w01w30)） |
| 手本にする既存実装 | `src/llvm.rs` の `FunctionEmitter::comparison_call`・`operand_borrow`・`release_operands`（一時値を領域に置いて借用し、呼び出しの後に解放する）、同じファイルの `Binary(BinaryOp::Pipe, ..)` の `BorrowOperand` の枝（`temp \|> f`） |
| 主な影響ファイル | `src/llvm.rs`（`FunctionEmitter::call`）、`src/llvm_imports.rs`（`FunctionEmitter::host_call`）、`tests/ffi_extensions.rs`、`tests/ffi_extensions.mjs`、`tests/ffi_extensions_host.c`、`tests/fixtures/ffi_extensions/Main.tz`、`.github/workflows/ci.yml`（D5）、`_tsuzuri/language-reference/compiler/native-interop.md`、`docs/language.md` |

## 実装者向け早見

- 範囲: コールバックを取る `extern`（以下「コールバック extern」）の呼び出しで、`ref` の引数に一時値（文字列のリテラル、`$"..."`、関数呼び出しの結果、`const`、record のリテラル、配列のリテラル、ハンドルを返す呼び出し）を渡したとき、直接の `host_call` を出す。
- 原因: `FunctionEmitter::call` は、引数に `BorrowOperand`（借用した一時値）が 1 つでもあると、コールバック extern の判定より先に `comparison_call` へ渡す。`comparison_call` は `call @tz.fn.Main.<extern>(%tz.closure ..., ...)` を出すが、コールバック extern の本体は生成しない（E12 D9）ので、その関数は未定義になる。
- 最初に読む: `src/llvm.rs` の `FunctionEmitter::call` の先頭と、`import.callbacks` の枝 → `src/llvm_imports.rs` の `host_call` → `src/llvm.rs` の `comparison_call`・`operand_borrow`・`release_operands`。
- 最初に書くテスト: `tests/ffi_extensions.rs` の `callback_externs_borrow_temporary_arguments`（IR に `@tz.fn.Main.<extern>(` の呼び出しがないこと）。
- 触ってはいけない: コールバックを取らない extern の生成（今も一時値で動く。IR を 1 byte も変えない）。今コンパイルできるプログラムの IR。
- 完了の見分け方: 次のプログラムを MinGW の同梱ツールチェーンで `build` でき、`shim_run` を実装したホストとリンクして実行できる。

```tsuzuri
extern type Hwnd
@trusted
extern "shim_run" def shim_run :: (ref Hwnd -> i32u -> i64u -> i64 -> i64 -> i64) -> ref string -> i32
def on_event :: ref Hwnd -> i32u -> i64u -> i64 -> i64 -> i64 = \_hwnd _msg _wparam _lparam state -> state
def main :: unit -> i32 = \() ->
    shim_run on_event "t"
```

## 目的

E12 は、コールバック extern を「直接・全引数で呼ぶ」ことだけを条件にした（E12 D8・段 D）。一時値を `ref` の引数に渡すことは、ほかの extern と同じく許している。今は `check` がこのプログラムを受け入れるのに、生成コードが未定義の関数を呼ぶので、`build` は Clang の IR の検証で `E2002` になる。利用者は `let t = "t"` と束縛し直す回避策を、診断からは知ることができない。

コールバック extern の呼び出しを、引数の形によらず E12 D9 のとおり直接の `host_call` にする。一時値は、呼び出しの間だけ一時の領域に置いて借用し、呼び出しが戻った後に解放する（ほかの関数の呼び出しと同じ規則）。

## 着手条件と停止条件

### 着手条件

- `node scripts/repo-kb.mjs brief W01`。「現状」の再現（下）を HEAD で繰り返し、同じ IR になることを確かめる。食い違えば、このチケットを直してから始める。
- `grep -n "fn call(&mut self, callee" src/llvm.rs` と `grep -n "import.callbacks" src/llvm.rs src/llvm_imports.rs` で、判定の位置が「現状」と同じであることを確かめる。
- Linux か macOS で、HEAD のまま `node tests/ffi_extensions.mjs target/debug/tsuzuri` が通ることを確かめる（この suite を回すワークフローはなく、起票の時点では Windows の環境しかなかったので、HEAD で通るかは未確認）。

### 停止条件

- **S1** `host_call` の中で `operand_borrow` を使うと、ハンドル・record・配列・文字列のどれかで、既存の `Borrow`（場所の借用）と違う値（ポインターと値の取り違え）が出る。2 回直して直らなければ止めて報告する。
- **S2** 今コンパイルできるプログラム（`tests/fixtures`・`examples` のすべての `Main.tz`）の `--emit llvm` が、native か `--target wasm32` で 1 byte でも変わる。
- **S3** 一時値の解放を足すと `live != 0`（`tests/ffi_extensions.mjs` の `clean()`）か、ASan の二重解放になる。
- **S4** HEAD の `tests/ffi_extensions.mjs` が、このチケットの変更の前から失敗する（着手条件）。失敗を記録して止める。suite の修理は W01 の範囲ではない（D5 もその修理の後にする）。
- **S5** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

### 再現

Git Bash で、開発版のコンパイラ（`target/debug/tsuzuri.exe`）と VS Code の拡張に同梱の MinGW のツールチェーンを使う。

```sh
mkdir -p target/scratch/w01/repro && cd target/scratch/w01
# repro/Main.tz に「実装者向け早見」のプログラムを書く
../../debug/tsuzuri.exe check repro; echo $?                 # 0
../../debug/tsuzuri.exe build repro --emit llvm -o repro.ll
grep -n "shim_run" repro.ll
#   %v7 = call i32 @tz.fn.Main.shim_run(%tz.closure %v3, ptr %v6)
#   （define @tz.fn.Main.shim_run も declare @shim_run もない）
T=$HOME/.vscode/extensions/tmidorikawa.tsuzuri-0.1.6-win32-x64/toolchain
TSUZURI_CLANG=$T/bin/tsuzuri-clang.exe ../../debug/tsuzuri.exe build repro -o repro.exe
#   error[E2002]: '.../tsuzuri-clang.exe' failed (exit code: 1):
#   ...module.ll:266:18: error: use of undefined value '@tz.fn.Main.shim_run'
```

- `"t"` を `let t = "t"` で束縛して `shim_run on_event (ref t)` と書くと、`call i32 @shim_run(ptr @tz.callback.Main.on_event, ptr ..., i64 ...)` になり、通る。
- 配布物 0.1.6 の `tsuzuri.exe`（同じ `toolchain/bin`）でも、`check` が 0 で、同じ `call i32 @tz.fn.Main.shim_run(%tz.closure %v3, ptr %v6)` を出す。
- `--target wasm32` でも同じ IR になり、`build --target wasm32` は Clang の同じエラーで `E2002`。

### 一時値の種類ごとの結果

コールバック `inc :: i64 -> i64` を取る extern に、次の実引数を渡した（`target/scratch/w-w01w02/` で確認し、確認後に消した）。ホストは C で書き、`TSUZURI_CLANG=$T/bin/tsuzuri-clang.exe` で `build --link host.o` した。ホストの C は `$T/bin/tsuzuri-clang.exe -O2 -fno-sanitize=undefined -c host.c` でコンパイルした（同梱の `tsuzuri-clang`（`zig cc`）は、`-O` なし・`-O0`・`-Og`・引数のない `-O` の C に UBSan の参照を入れ、その object は既定の `-O3` の `build --link` で `lld-link: error: undefined symbol: __ubsan_handle_type_mismatch_v1` などになる。`-O2` では参照は入らないので、`-O2` を付けていれば `-fno-sanitize=undefined` は効果がない（[W37](../W37-bundled-clang-ubsan-objects.md) の「現状」A1・A2）。W01 とは別の問題で、W37 が直す）。

| 実引数 | extern の引数の型 | `check` | 生成コード |
| --- | --- | --- | --- |
| `"abc"` | `ref string` | 0 | `call i64 @tz.fn.Main.apply_text(%tz.closure ..., ptr ...)`。未定義で `E2002` |
| `$"n={x}"` | `ref string` | 0 | 同じ |
| `make ()`（`string` を返す） | `ref string` | 0 | 同じ |
| `NAME`（`const NAME: string = "abcd"`） | `ref string` | 0 | 同じ（定数の静的データの借用も `BorrowOperand`） |
| `Point { x: 1, y: 2 }` | `ref Point`（スカラーの record） | 0 | `@tz.fn.Main.apply_point` を呼び、未定義 |
| `[1, 2, 3]` | `ref [i64]` | 0 | `@tz.fn.Main.apply_sum` を呼び、未定義 |
| `counter_new 1`（ハンドルを返す extern） | `ref Counter` | 0 | `@tz.fn.Main.visit` を呼び、未定義 |
| `twice (apply_text inc "abc")`（入れ子） | `ref string` | 0 | 内側が同じ |
| `let t = "abc"` の後に `(ref t)` | `ref string` | 0 | `call i64 @w01_apply_text(ptr @tz.callback.Main.inc, ptr %v6, i64 %v7)`。実行して 4 |
| コールバックを取らない `text_len "abc"` | `ref string` | 0 | 定義された包み `@tz.fn.Main.text_len` を呼ぶ。実行して 3 |
| コールバックを取らない `text_len (make ())` | `ref string` | 0 | 同上。実行して 5 |
| `"abc" \|> apply_text inc` | – | `E1008` | 「extern 'apply_text' takes a callback and must be called directly with all arguments」（`validate_callbacks`） |
| `let f = apply_text inc` の後に `f "abc"` | – | `E1008` | 同上 |

### 原因（コード）

- `src/constants.rs` の `Checker::temporary_borrows`（中の `make_operand`）: 呼び出しが全引数を一度に適用し、結果が借用を持たないとき、場所でない実引数の `Borrow(value, false)` を `TypedExprKind::BorrowOperand(value)` にする。純粋なデータの `const` は、呼び出しの形によらず `BorrowOperand` にする。呼び出しの検査（`src/check.rs` の `temporary_borrows` の呼び出し 2 か所: 関数の適用と `\|>`）と、型が決まった後の `settle_temporary_borrows`（`src/polymorph.rs`）から呼ぶ。
- `src/llvm.rs` の `FunctionEmitter::call`: 先頭で、引数のどれかが `BorrowOperand` なら `return self.comparison_call(callee, arguments)` する。コールバック extern を `host_call` へ送る枝（`if let TypedExprKind::HostCall(import, _) = &function.body.kind && import.callbacks`）は、その後ろ（`DynOf`・`FixedArrayInit`・`wrapping_call` の後）にあるので、一時値があると届かない。
- `src/llvm.rs` の `FunctionEmitter::comparison_call` → `src/llvm_compare.rs` の `comparison_values`: 直接の呼び出し先には `call <結果> @tz.fn.<名前>(<引数>)` を出す。関数型の引数は `self.expression(argument)` で `%tz.closure` の値になる（`@tz.desc.Main.on_event.0` を作る）。
- `src/llvm.rs` の `emit_program_in`: 「A callback extern has no function body」として、`HostCall` の本体で `import.callbacks` が真の関数の定義を飛ばす。そのため `@tz.fn.Main.shim_run` はどこにも定義されない。
- `src/llvm_imports.rs` の `FunctionEmitter::host_call`: 引数を `self.expression(argument)` で評価する。`src/llvm.rs` の `emit_expression_mode` は `TypedExprKind::BorrowOperand(_)` を `unreachable!("polymorphism is resolved before LLVM")` にしているので、判定の順序を入れ替えるだけでは、`E2002` の代わりにコンパイラが panic する。
- `src/check.rs` の `validate_callbacks` は、呼び出しの形（直接・全引数）とコールバックの位置の実引数（トップレベルの利用者関数の名前）だけを見る。一時値の引数は見ないので `check` は通る（これは正しい。直すのは生成）。
- コールバックを取らない extern は、`emit_program_in` が本体（`HostCall`）を `@tz.fn.<名前>` として定義するので、`comparison_call` から呼んでも動く。今の生成は、一時値の有無によらず、この包みを呼ぶ。
- `--backend fast`（`src/backend/`）は、`src/backend/mod.rs` の説明によれば `src/llvm*.rs` の `-O0` の IR を翻訳するので、同じ不正な IR を受け取るはず（未確認。実行して確かめていない）。

## 変えてはいけない意味

- 引数は左から右に一度だけ評価する（E12「評価順序・所有権・借用」）。コールバックの位置は関数名なので、評価も確保もしない（`%tz.closure` を作らない）。
- 借用した一時値は、呼び出しが正常に戻った直後に、作った順に解放する。後の引数の評価中に脱出（`break`・`continue`・`try` が受け止める例外）したときは、作った逆の順に解放する（`docs/language.md` の「制御構文」の段落。`frame_inner` の `remember_temporary` が担う）。
- Latin-1 の文字列は、すべての引数を評価した後で UTF-16 の複写を作り、呼び出しの後に解放する（`host_call` の `widen`・`free_copies`）。
- ハンドルには drop glue がない（E12 D3）。`ref Counter` に渡したハンドルの一時値は、借用の後に何も呼ばずに捨てる（`let c = counter_new 1` を閉じずに置いたときと同じ。ホストの資源の leak はホストの問題）。

## 仕様

- コールバック extern の直接・全引数の呼び出しは、`ref` の引数に一時値があっても受け入れ（今と同じ）、`build`・`run`・`test`・`--emit object`・`--emit llvm`・`--target wasm32`・`wasm64` で正しく動く。
- 生成コードは E12 D9 のとおり、呼び出し側の `call <結果> @<リンク名>(ptr @tz.callback.<関数>, ...)` で、`@tz.fn.<extern>` を参照しない。`%tz.closure` を作らない。
- 一時値は、呼び出しの前にフレームの一時の領域（`operand_borrow` の `spill`）に置き、その借用をホストへ渡す。ホストから見た値（文字列の `const uint16_t *` と長さ、record の正規化した一時の struct、配列の要素へのポインターと長さ、ハンドルの値そのもの）は、束縛してから `ref` で渡したときと同じ。
- 診断は変えない（`E1008` の呼び出しの形の規則も同じ）。C ヘッダーと WASM の import は変えない。

## 設計

### `FunctionEmitter::call`（`src/llvm.rs`）

コールバック extern の判定を関数の先頭に移し、`BorrowOperand` の判定より前に置く。後ろにある同じ枝は消す（判定を 1 か所にする）。

```rust
fn call(&mut self, callee: &TypedExpr, arguments: &[TypedExpr]) -> String {
    // E12 D9: a callback extern has no function body, so every call of it is the host call itself,
    // whatever its arguments are (a borrowed temporary included).
    if let TypedExprKind::Function(FunctionRef::User(id)) = callee.kind
        && let TypedExprKind::HostCall(import, _) = &self.module.functions[id].body.kind
        && import.callbacks
    {
        return self.host_call(import, arguments, &self.module.functions[id].signature.result);
    }
    if arguments.iter().any(|argument| matches!(argument.kind, TypedExprKind::BorrowOperand(_))) {
        return self.comparison_call(callee, arguments);
    }
    // ...（今のまま。下の `import.callbacks` の枝は消す）
}
```

`validate_callbacks` が「直接・全引数」を保証するので、ここで部分適用を考える必要はない（引数の数が違えば `E1008`）。

### `FunctionEmitter::host_call`（`src/llvm_imports.rs`）

- `let mut operands = Vec::new();`（`comparison_call` の `cleanup` と同じ `(Type, String, String, Vec<Frame>)`）を足す。
- 各引数で、`TypedExprKind::BorrowOperand(operand)` なら `self.operand_borrow(argument, operand, &mut operands)`、そうでなければ今の `self.expression(argument)`。`operand_borrow` は、配列の借用（`slice_element` が `Some`）では配列の値（記述子）を、それ以外では値を置いた領域へのポインターを返す。これは `Borrow`（`emit_expression_mode` の `TypedExprKind::Borrow`）が返す形と同じなので、後ろの `Buffer::of`・record・ハンドルの枝はそのまま使える（S1 で確かめる）。
- 解放は `self.release_operands(operands)` を、ホストの呼び出しの後、`free_copies` の後、結果の変換（`read_host_result`、`icmp`、`trunc`）の後に 1 回。`out` の結果と `unit` の結果の早い `return` の前にも同じ順で入れる（3 つの出口すべて）。
- `operand_borrow` と `release_operands` は `src/llvm.rs` の `impl FunctionEmitter` の非公開のメソッドで、`llvm_imports` は `#[path = "llvm_imports.rs"]` の子モジュールなので、そのまま呼べる。

### 採らない案

- **一時値を拒否する**（`E1008` で `let` を求める）: ほかの extern と `f (ref t)` の書き方では受け入れている形を、コールバックの有無で分けることになる。意味は決まっているので、拒否する理由がない（GUIDE §1 規則 2 は未対応の構文の置き換えを禁じるもので、ここは対応済みの構文）。
- **コールバック extern にも本体を出す**: 本体は実行時の `%tz.closure` しか受け取れず、どの関数かを知れない（E12 D9 の理由そのもの）。
- **検査で一時値を `let` に書き換える**: 型付きの木と所有権の検査の入力が変わり、変更が大きい。生成だけで直せる。

## 前提とする他チケットのインターフェース

- なし。

## 他チケットへ提供するインターフェース

- W27 の生成器が出す `extern` は、`ref string`（W20 の後は `ref ZString`）の引数に、文字列のリテラルや `$"..."` をそのまま渡す呼び出しを書いてよい。
- W13（`ref mut`）・W15（`Maybe<ref R>`）・W20（`ref ZString`）が `host_call` に足す引数の種類も、`BorrowOperand` を同じ `operand_borrow` で受ける（このチケットで入れる 1 か所を使う）。

## 実装手順

1. **変更**: ブランチ `fix/w01-callback-extern-temporaries`。基準の記録だけ。
   **内容**: 着手条件。`tests/fixtures`・`examples` の `Main.tz` を、HEAD のコンパイラで `--emit llvm -O1`（native と `--target wasm32`）にして `target/scratch/w01/before/` に保存する。
   **確認**: 保存した数と、単独で build できない fixture の一覧を控える。
2. **変更**: `tests/ffi_extensions.rs` に `callback_externs_borrow_temporary_arguments`（テスト計画）を先に書く。
   **内容**: 今は失敗すること（`@tz.fn.Main.apply_text(` が IR にある）。
   **確認**: `cargo test --locked --test ffi_extensions callback_externs_borrow` が `running 1 test` で失敗する。
3. **変更**: `src/llvm.rs` の `FunctionEmitter::call` と `src/llvm_imports.rs` の `host_call`（設計）。
   **内容**: 判定の移動、`operand_borrow`・`release_operands` の使用、後ろの枝の削除。
   **確認**: 手順 2 のテストが通る。T0。`cargo test --locked --test ffi_extensions` の全件。手順 1 の IR と、手順 3 のコンパイラの IR が `cmp` で一致する（S2）。
4. **変更**: E2E（`tests/fixtures/ffi_extensions/Main.tz`・`tests/ffi_extensions_host.c`・`tests/ffi_extensions.mjs`。テスト計画）。
   **内容**: ネイティブの `-O0`・`-O3`（IR からと `--emit object` から）と WASM の wasm32・wasm64。
   **確認**: `node tests/ffi_extensions.mjs target/debug/tsuzuri` が通る（このチケットの前のコンパイラでは、新しい export のある fixture の build が `E2002` で落ちることも一度確かめる）。`ci.yml` の `suites` の job に同じ段を足す（D5）。
5. **変更**: Windows の再現の確認と文書。
   **内容**: 「現状」の再現のプログラムを、`shim_run` を実装した C のホスト（`on_event` を 1 回呼んで、その結果を返す）と MinGW の同梱ツールチェーン・MSVC の両方で `build --link host.o` し、実行する。文書（下）。
   **確認**: 終了コードが C のホストの計算どおり。T1、T2（`node scripts/check-docs.mjs` に変更したページ）。

## テスト計画

### Rust（`tests/ffi_extensions.rs`）

`callback_externs_borrow_temporary_arguments`: 次の前置きの後に、本体を 1 つずつ足して `analyze` し、native と wasm32 の IR（既存の `ir` 関数。決定的であることも見る）を検査する。

```tsuzuri
record Point { x: i64, y: i64 }
extern type Counter
extern "e12_text" def apply_text :: (i64 -> i64) -> ref string -> i64
extern "e12_point" def apply_point :: (i64 -> i64) -> ref Point -> i64
extern "e12_sum" def apply_sum :: (i64 -> i64) -> ref [i64] -> i64
extern "e12_new" def counter_new :: i64 -> Counter
extern "e12_visit" def visit :: (ref Counter -> i64) -> ref Counter -> i64
private def inc :: i64 -> i64
fn inc value = value + 1
private def peek :: ref Counter -> i64
fn peek counter = 7
private def label :: own i64 -> string
fn label value = $"item-{value}"
const NAME: string = "abcd"
```

| 本体（`export def root :: own i64 -> i64` の中） | 検査 |
| --- | --- |
| `apply_text inc "abc"` | `call i64 @e12_text(ptr @tz.callback.Main.inc, ptr ` がある。`@tz.fn.Main.apply_text(` がない |
| `apply_text inc $"n={value}"` | 同上 |
| `apply_text inc (label value)` | 同上 |
| `apply_text inc NAME` | 同上 |
| `apply_point inc (Point { x: value, y: 2 })` | `call i64 @e12_point(ptr @tz.callback.Main.inc, ptr ` がある。`@tz.fn.Main.apply_point(` がない |
| `apply_sum inc [value, 2, 3]` | `@e12_sum(ptr @tz.callback.Main.inc, ptr `。`@tz.fn.Main.apply_sum(` がない |
| `visit peek (counter_new value)` | `@e12_visit(ptr @tz.callback.Main.peek, ptr `。`@tz.fn.Main.visit(` がない |
| `inc (apply_text inc "abc")` | 内側が `@e12_text` |

- どの本体でも: `declare i64 @e12_text(ptr, ptr, i64)`（など）がちょうど 1 回。適用の補助（`@tz.apply.`）の外に `insertvalue %tz.closure` がない（`callbacks_lower_to_internal_c_wrappers` と同じ検査）。wasm32 では `declare` に `"wasm-import-module"="tsuzuri"`。
- コールバックを取らない extern（`extern "e12_len" def text_len :: ref string -> i64` に `text_len "abc"`）は、今と同じく包みを呼ぶ: IR に `call i64 @tz.fn.Main.text_len(` があり、`define` された `@tz.fn.Main.text_len` の中に `call i64 @e12_len(` がある（期待する文字列は手で書く）。
- 拒否は変わらない: `"abc" |> apply_text inc` と `let f = apply_text inc` の後の `f "abc"` は `E1008`（「must be called directly with all arguments」）。

### E2E（`tests/ffi_extensions.mjs`）

- fixture に足す（`record Pair { x: i64, y: i64 }`、`private def label :: own i64 -> string` = `$"item-{value}"`、`const GREETING: string = "hello"`）:
  - `extern "e12_text_units" def text_units :: (i64 -> i64) -> ref string -> i64 -> i64`（ホストは `callback(len) + extra`）
  - `extern "e12_pair_mix" def pair_mix :: (i64 -> i64) -> ref Pair -> i64`（ホストは `callback(x * 10 + y)`）
  - `extern "e12_sum_with" def sum_with :: (i64 -> i64) -> ref [i64] -> i64`（ホストは `callback(要素の和)`）
  - `export def temporaries :: own i64 -> i64` = `text_units triple "abc" 0 + text_units triple $"n={value}" 0 + text_units triple (label value) 0 + text_units triple GREETING 0 + pair_mix triple (Pair { x: value, y: 2 }) + sum_with triple [value, 2, 3]`
  - `export def temporary_then_raise :: own i64 -> i64`: 後の引数が `OverflowException` を送出したら `-1`、送出しなければ呼び出しの値。形は `match (try text_units triple (label value) (@checked value * value) with | e -> e) with | Ok total -> total | Error _ -> -1`（`try` が `Result` を返す形。構文は `tests/specs.rs` の `try` の例で確かめてから書く）。
- 期待値（手で計算）: `temporaries 4` は `9 + 9 + 18 + 15 + 126 + 27 = 204`（`"abc"` 3 単位、`"n=4"` 3、`"item-4"` 6、`"hello"` 5、`4 * 10 + 2 = 42`、`4 + 2 + 3 = 9`、どれも 3 倍）。`temporary_then_raise 3` は `3 * 6 + 9 = 27`。`temporary_then_raise 3037000500` は `-1`（`3037000500² = 9223372037000250000 > 2⁶³ − 1`、`3037000499² = 9223372030926249001` は収まる）。
- ネイティブ: `tests/ffi_extensions_host.c` の `E12_HOST_MAIN` でない部分に 3 つの関数を足し（`--link`・`-l`・`[native]` の段でも未定義にならないように）、`main` に `assert(tz_temporaries(4) == 204); clean();`、`assert(tz_temporary_then_raise(3) == 27); clean();`、`assert(tz_temporary_then_raise(3037000500) == -1); clean();`。`clean()` が `live == 0`（追跡する allocator）を見るので、`$"..."`・関数の結果・配列のリテラルの一時値の解放と、例外のときの解放が確かめられる。
- WASM の `verify`: `expectedImports`（ソートした完全な一覧）に `tsuzuri.e12_pair_mix:function`・`tsuzuri.e12_sum_with:function`・`tsuzuri.e12_text_units:function` を足す。JS の import は、文字列は長さだけを使い、record と配列は `instance.exports.memory.buffer` から読む（record は先頭から `i64` の `x`・`y`、配列は `BigInt64Array`。wasm64 ではポインターが BigInt なので `Number(...)`）。`tz_temporaries(4n) === 204n` など同じ 3 つ。
- ハンドルの一時値（`visit peek (counter_new value)`）は E2E に入れない（ハンドルを閉じないので `live_counters` の検査に反する）。IR の検査だけにする。

### CI で回す場所

- HEAD では、`tests/ffi_extensions.mjs` を回すワークフローがない（`grep -rn ffi_extensions .github scripts` が 0 件。`tests/compact_strings_modes.mjs` の `full` の一覧にはあるが、`targets.yml` は `quick` だけを回す）。CI で毎回見るのは Rust の IR の検査だけになる。
- そこで `ci.yml` の `suites` の job の「Control flow and ownership cleanup」の隣に、`node tests/ffi_extensions.mjs target/debug/tsuzuri` の段を足す（D5）。この suite は Linux の Clang・`ar`・`-pthread` と Node.js だけを使う。

### Windows

- 手順 5 の再現（MinGW と MSVC）。`tests/windows.mjs` には足さない（`ffi_extensions.mjs` は `-pthread` と `ar` を使う POSIX 向けの suite）。

## ドキュメント

- `_tsuzuri/language-reference/compiler/native-interop.md` の「静的コールバック」: 「ほかの引数には、ほかの `extern` と同じく一時値（文字列のリテラル、`$"..."`、呼び出しの結果）を渡せる。呼び出しの間だけ借用され、戻った後に解放される」を 1 文足す。同じ段落の寿命の文（「`extern` の呼び出しが返ったあとに、そのポインタを非同期で呼んではいけません」）は W12 の担当なので触らない。
- `docs/language.md` の「コールバック」節に同じ 1 文。寿命の 2 文（W12）には触らない。
- README・`docs/architecture.md`: 書いてあることは誤りにならないので変えない（`architecture.md` に `host_call` の引数の説明があれば、`BorrowOperand` を受けることを 1 文足す）。
- `node scripts/check-docs.mjs _tsuzuri/language-reference/compiler/native-interop.md`。

## 受け入れ条件

- [x] 「実装者向け早見」のプログラムと「現状」の表の一時値がすべて、native（MinGW・MSVC）と wasm32 で `build` でき、生成コードが `@tz.fn.<extern>` を参照しない。
- [x] `tests/ffi_extensions.rs` の新しいテストと既存の全件が通る。
- [x] `node tests/ffi_extensions.mjs target/debug/tsuzuri` が native の `-O0`・`-O3`（IR と object）と WASM で通り、`live == 0`。`ci.yml` の `suites` の job がこの suite を回し、PR の CI で通る。
- [x] 今コンパイルできるプログラム（`tests/fixtures`・`examples` のすべての `Main.tz`）の IR・object・header・診断が 1 byte も変わらない。
- [x] コールバックを取らない extern の生成が変わらない。
- [x] T2 が通る。PR に `ci:full` を付ける（Windows の MinGW・MSVC で確かめる W チケットの規則）。

## 落とし穴

- **判定の順序だけを入れ替えない**: `host_call` は `BorrowOperand` を評価できない（`emit_expression_mode` の `unreachable!`）。入れ替えだけでは panic になる。
- **解放の位置**: 3 つの出口（`unit` の結果、`out` の結果、スカラーの結果）すべてで、呼び出しと `free_copies` の後に `release_operands` を入れる。`free_copies` より前に解放すると、Latin-1 の複写の元（借用した一時値の文字列）を先に解放する順序になる（複写は別の確保なので壊れはしないが、`docs/language.md` の順序の説明と合わない）。
- **配列の借用は記述子**: `operand_borrow` は配列（`slice_element` が `Some`）では領域ではなく値を返す。`host_call` の `Buffer::of` の枝は配列のとき `value` をそのまま記述子にするので合う。文字列は領域のポインターを返すので、今の `load %tz.string` の経路に入る。
- **`expectedImports` は完全な一覧**: 足し忘れると WASM の段だけが落ちる。JS の import 関数と `ffi_extensions_host.c` の C の関数も同じ 3 つを足す。
- **W12 との衝突**: `native-interop.md` の「静的コールバック」の段落は W12 も書き換える。文を足す位置を寿命の文と分ける。

## 対象外

- コールバックを取らない extern の生成を、包み（`@tz.fn.<extern>`）を通さない直接の `host_call` に変えること（IR が変わる。性能の差は計測していない。必要なら別のチケット）。
- `comparison_call` が `call` のほかの特殊化（恒等関数、`Add.add` の命令化など）を通らないこと（一時値を `ref` で受ける関数には当たらない。気づいたことの記録だけ）。
- 同梱の `tsuzuri-clang` が `-O` なし・`-O0` の C に UBSan の参照を入れるため、その C のホストが既定の `-O3` のリンクやほかのリンカーでリンクできないこと（[W37](../W37-bundled-clang-ubsan-objects.md) が直す）。
- コールバックの寿命の契約（W12）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | 一時値は拒否せず、一時の領域に置いて借用し、直接の `host_call` を出す | E12 D9（呼び出し側で `host_call` を直接出す）と、ほかの関数の一時値の規則に合わせる。拒否・本体の生成・検査での書き換えは「設計」の「採らない案」のとおり採らない | 確定・実装済み |
| D2 | コールバック extern の判定は `FunctionEmitter::call` の先頭の 1 か所にする | 一時値・特殊化のどの枝より先に見れば、引数の形で経路が分かれない（AGENTS.md の「同じ意味の書き方が別の経路を選ばない」） | 確定・実装済み |
| D3 | コールバックを取らない extern の生成は変えない | 今動いている経路で、IR を変えない規則を守る | 確定・実装済み |
| D4 | ハンドルの一時値は借用の後に何もせず捨てる | E12 D3（drop glue なし。閉じるのは利用者が値で渡したときだけ）のまま | 確定・実装済み |
| D5 | `tests/ffi_extensions.mjs` を `ci.yml` の `suites` の job で回す | 「`check` は通るのに `build` で落ちる」種類の誤りは E2E でしか見えない。今はどのワークフローもこの suite を回さない。ほかの suite と同じく `target/debug/tsuzuri` で回し、速度の合否は入れない | 確定・実装済み |

## 実装と検証（2026-11-23）

- `src/llvm.rs` の `FunctionEmitter::call` の先頭でコールバック extern を判定し、`src/llvm_imports.rs` の `host_call` で `BorrowOperand` を `operand_borrow` で借用し、全出口で `free_copies` の後に `release_operands` で解放するよう修正した。
- `tests/ffi_extensions.rs`（`callback_externs_borrow_temporary_arguments`）、`tests/fixtures/ffi_extensions/Main.tz`・`tests/ffi_extensions_host.c`・`tests/ffi_extensions.mjs`（`temporaries` と `temporary_then_raise`）、`.github/workflows/ci.yml`（`suites` への `ffi_extensions.mjs` 追加）、`docs/language.md`、`_tsuzuri/language-reference/compiler/native-interop.md` を更新した。

