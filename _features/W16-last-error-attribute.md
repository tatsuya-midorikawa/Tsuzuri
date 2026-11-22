# W16: `@last_error`（呼び出し直後の `GetLastError`／`errno`）

| 項目 | 内容 |
| --- | --- |
| ID | W16 |
| 優先度 | P2（`CreateWindowExW`・`RegisterClassExW`・`CloseHandle`・ファイルの API の失敗の理由を、呼び出しと切り離されない形で取れない。Linux・macOS では `errno` を読む手段そのものがない。HRESULT を返す API はすでに `i32` で足りる） |
| 規模 | S（属性の構文と符号化、検査 5 つ、`host_call` の数行と text pass の深さの数え方、テストと文書） |
| 依存 | – |
| 後続 | [W27](W27-win32-package-from-winmd.md)（win32metadata の `SetLastError` の写し先） |
| 状態 | todo |
| 起票 | 2026-11-22（D-63。利用者の依頼「X: D13 を狭く改める方針で決定。実現向けたチケットを @_features に W プリフィックスのチケットを作成して」と「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」から起票。HEAD `fa40eede` で確認） |
| 承認 | 要承認（D-53）。決定事項 D1 が要承認、D2〜D8 は既定案 |
| 手本にする既存実装 | `src/parser.rs` の `attributes`・`trusted_attribute`（宣言の前の属性の読み込み口、D-53）、`src/syntax.rs` の `TrustedAttribute` と `Program::trusted`（脇のリスト）、`src/check.rs` の `check_modules_collect` の `@trusted` の対応付け（名前と位置で extern を探す）、`src/llvm_imports.rs` の `host_call`、`src/llvm.rs` の `with_blocking_externs`、`src/runtime/tzroutine.c` の `tzr_exitsyscall`・`tsuzuri_tz_preempt_check`（`errno` と `GetLastError` の退避） |
| 主な影響ファイル | `src/syntax.rs`, `src/parser.rs`, `src/syntax_codec.rs`, `src/frontend_cache.rs`（`FRONTEND_FORMAT`）, `src/formatter.rs`, `src/api_diff.rs`, `src/check.rs`, `src/llvm_imports.rs`, `src/llvm_abi.rs`, `src/llvm.rs`, `src/target.rs`, `src/diagnostic_kinds.rs`, `tests/last_error.rs`（新規）, `tests/ffi_extensions.mjs`, `tests/windows.mjs`, `docs/language.md`, `_tsuzuri/language-reference/compiler/native-interop.md`, `_tsuzuri/language-reference/values-and-functions/attributes.md` |

## 実装者向け早見

- 範囲: `extern ... def` の前の属性 `@last_error`。宣言の結果は `R * i32u`（R は C の関数の結果）。呼び出しの直前に Windows は `SetLastError(0)`、POSIX は `errno = 0` にし、直後に `GetLastError()`／`errno` を読んで組の 2 番目にする。C の ABI は変えない。native だけ。
- 共通契約の `(R, i32u)` は組の値の書き方で、型は `R * i32u` と書く（Tsuzuri のタプル型の構文。[Tuple](../_tsuzuri/language-reference/built-in-types-and-modules/tuple.md)）。
- 最初に読む: ① 仕様 1〜3 ② `src/llvm_imports.rs` の `host_call` ③ `src/llvm.rs` の `with_blocking_externs` ④ `src/parser.rs` の `attributes` と `signature` の先読み。
- 最初に書くテスト: `tests/last_error.rs` の `last_error_needs_the_pair_result` と `capture_sits_inside_the_blocking_bracket`。
- 触ってはいけない: `@last_error` のない extern の IR（text pass の出力を含む）、extern の `declare` の形、`@trusted` の意味（印は生成コードを変えない）。
- 完了の見分け方: Windows の native で次が終了コード 6（`ERROR_INVALID_HANDLE`）、POSIX で `extern "close" def close_fd :: i32 -> i32 * i32u` に `-1` を渡す版が 9（`EBADF`）になる。

```tsuzuri
@trusted @last_error
extern "CloseHandle" def close_raw :: i64 -> bool * i32u

def main :: unit -> i32 = \() ->
    match close_raw 0 with
    | (ok, code) -> if ok then 0 else code as i32
```

## 目的

Win32 の多くの関数は失敗を戻り値で知らせ、理由を `GetLastError` に置く。C の関数は `errno` に置く。今の Tsuzuri には、呼び出しと同時にその値を取る手段がない。Windows では `extern "GetLastError"` を別に宣言して呼べるが、2 つの呼び出しの間には Latin-1 の文字列の複写の解放（`host_call` の `free_copies`）、結果の読み出し、drop、確保、利用者のコードが入りうる。成功しても値を変えない API（`SetWindowLongPtrW` の 0 は成功か失敗か分からない）は、呼び出しの直前に 0 にしておく必要もある。POSIX では `errno` を読む関数（`__errno_location`・`__error`）が予約された接頭辞 `__` で始まるので、extern として宣言できない。

このチケットは .NET の `DllImport(SetLastError = true)` と同じ働きの属性 `@last_error` を足す。読み出しは呼び出しの直後で、tzroutine の P の取り戻し（`tsuzuri_tz_blocking_exit`）より前、文字列の複写の解放より前に行う。[GUIDE D-63](GUIDE.md#d-63-tsuzuri-だけで-win32-のアプリを書くe12-d13-の狭い改訂w01w30) の Win32 のアプリを書く道具の 1 つで、W27 の生成器が win32metadata の `SetLastError` をこれに写す。

## 着手条件と停止条件

### 着手条件

- `node scripts/repo-kb.mjs brief W16` を読む。決定事項 D1 の承認があること（承認前は手順 2 以降に着手しない）。
- 「現状」の確かめ方を着手時の `main` で実行し、食い違えばこのチケットを直してから始める。
- W14（同じ symbol の比較）・W15・W17（属性の脇のリスト）が先に入っているかを見る。

### 停止条件

- **S1** 読み出しを `tsuzuri_tz_blocking_exit` より前に置くのに、`@last_error` のない extern の IR か text pass の出力を変える必要がある。
- **S2** `--trap-info`・`--coverage`・`--trap-mode return` などの計装が、呼び出しと読み出しの間に別の呼び出しを入れる。
- **S3** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

- **構文**: `@last_error` はない。`@last_error` の行の後に `extern "GetCurrentProcessId" def pid :: unit -> (i32u, i32u)` を書くと、`1:1: error[E0002]: expected an expression; '@checked' is the only attribute of an expression` と、型の `(i32u, i32u)` に `E0002`（`expected ')' after the type`）が出る（開発版で確認）。組の型は `i32u * i32u` と書く（`unit * i32u` も型検査を通ることを確認）。`@trusted @last_error` を 1 行に書くと、今は `@last_error` を属性と読まないので `E1037`（`trusted_wrong_target`）になる。`@trusted` の置き場の検査は `attributes` が知っている属性をすべて読んだ後に行う（`program_all` の `let Attributes { cpu, effects, trusted } = self.attributes()?` の直後）ので、`attributes` に足せば 1 行の書き方も通る。
- **属性の読み込み口**: `src/parser.rs` の `attributes` が `@cpu`・効果の属性・`@trusted` を任意の順に読む（D-53）。本体のない宣言（`extern ... def`）の後では、`signature` が次の宣言の属性を `name == "cpu" || name == "trusted"` と効果の属性で見分け、それ以外の `@` を制約行として読む。属性は `Program` の脇のリスト（`cpu_attributes`・`effect_attributes`・`trusted`）に持ち、`src/syntax_codec.rs` はこの順に符号化する。
- **呼び出しの生成**（`src/llvm_imports.rs` の `host_call`）: 引数を左から右に評価し、Latin-1 の文字列を UTF-16 の複写に広げてから `call` を出し、その後に `free_copies`（`@tz.free`）、out の結果の `read_host_result` の順。`declare` は `self.intrinsics`（宣言の行の文字列の `BTreeSet`）に入れる。
- **tzroutine の括弧**（`src/llvm.rs` の `with_blocking_externs`）: spawn するネイティブのプログラムでは、生成の後の text pass が extern の symbol を直接呼ぶ行を 1 行ずつ `call void @tsuzuri_tz_blocking_enter()` と `_exit()` で挟み、宣言を末尾に足す。`tzroutine { let _ok = close_raw 0 }` の IR は `call void @tsuzuri_tz_blocking_enter()` / `%v2 = call i32 @CloseHandle(i64 %v1)` / `call void @tsuzuri_tz_blocking_exit()` になる（開発版で確認）。素朴に `host_call` が呼び出しの次の行で読むと、text pass が呼び出しと読み出しの間に `_exit` を入れる。
- **ランタイム**: `tzr_exitsyscall` は sysmon が P を持ち去った遅い経路でだけ `errno` と `GetLastError` を退避して戻す。`tsuzuri_tz_preempt_check` も譲る前後で戻す（コメントは「後の呼び出しが C の呼び出しの値を読みうる」）。G は呼び出しの間その M に留まる（`tzr_acquirep` は同じ M で P を待つ）。
- **今の回避の限界**: Windows で `CloseHandle(0)` の後に別の extern の `GetLastError` を呼ぶと 6、`SetLastError(123)` の後に `GetCurrentProcessId` を呼んでから読むと 123 のまま（成功は値を消さない）。MinGW で終了コード 6123 として確認。POSIX では `extern "__errno_location" def ...` が `E1008`（`extern symbol '__errno_location' is reserved by Tsuzuri; ...`）。
- **記録**: 調査の範囲で、呼び出しと同時に last error を取る決定・チケットはない。docs の `native-interop.md` の「スレッドとホストの責任」は「`errno`（Windows では `GetLastError`）は、P を取り戻す間も呼び出しの値を保ちます」と書く。

確かめ方: `grep -n "fn attributes\|name == \"cpu\" || name == \"trusted\"" src/parser.rs`、`grep -n "free_copies\|intrinsics.insert" src/llvm_imports.rs`、`grep -n "fn with_blocking_externs" -A50 src/llvm.rs`、`grep -n "GetLastError" src/runtime/tzroutine.c`。

## 変えてはいけない意味

- extern の C の ABI と `declare` の形（C の結果は R のまま）。`@last_error` の有無で同じ symbol の `declare` が変わらない。
- `@last_error` のない extern の IR と、text pass の出力（spawn するプログラムを含む）。
- 引数の評価の順序と回数、Latin-1 の複写の作り方と解放、out の結果の読み方。`@trusted` は今どおり生成コードを変えない。

## 仕様

### 1. 構文と型

```text
last_error  = "@" "last_error"
declaration = [ doc ] { attribute } [ "private" ] "extern" [ link_name ] "def" ident "::" signature
```

- `@cpu`・`@trusted` などと任意の順に、`extern ... def`（明示の symbol の有無と WASM のモジュール名を問わない）の直前に 1 つだけ書ける。`@trusted` と同じ行でもよい（`@trusted @last_error`）。edition に依らない（予約語を足さない）。
- 宣言の結果は 2 要素の組 `R * i32u` でなければならない。R は今の extern の結果に許す型（`unit`・スカラー・ハンドル・所有バッファ・スカラー record。W15 の後は `Maybe<H>` も）で、C の関数の結果になる。R が `unit` のとき C は `void` を返す。引数は今の規則のまま。
- 関数値としての使用・部分適用・コールバックを取る extern にも使える（型は `A -> R * i32u`）。
- 共通契約の例 `@trusted @last_error extern "SetWindowTextW" def set_text :: Hwnd -> ref ZString -> bool * i32u` は、W18（`extern type copy Hwnd`）と W20（`ZString`）の後に書ける形である。このチケットの例とテストは、それらに依らない `CloseHandle`・`GetCurrentProcessId`・`close`・`getpid` を使う（今の `ref string` は `(ptr, i64)` で渡り、`LPCWSTR` の契約と合わない）。

### 2. 実行の意味（native）

呼び出しは次の順に行う。

1. 引数を左から右に一度だけ評価し、Latin-1 の文字列の複写を作る（今と同じ）。
2. spawn するプログラムでは `tsuzuri_tz_blocking_enter`。
3. 消去: Windows は `SetLastError(0)`、Linux は `*__errno_location() = 0`、macOS は `*__error() = 0`。
4. C の関数の呼び出し。
5. 読み出し: Windows は `GetLastError()`、POSIX は 3 で得た番地から `errno` を読む。値の 32 bit をそのまま `i32u` にする。
6. spawn するプログラムでは `tsuzuri_tz_blocking_exit`。
7. 複写の解放、out の結果の読み出し、組 `(R の値, 5 の値)` を作る。

- 読み出しは呼び出しをしたスレッドの値で、C の関数（とその中で呼ばれたコールバック）が残した値である。G は 2〜6 の間その OS スレッドから動かない。
- 組の値は普通の値で、後で別のスレッドに渡しても変わらない。
- Windows の C ランタイムの `errno` は読まない（D4）。

### 3. 検査と診断

| コード | kind（案） | 条件 | メッセージ（案） | 位置 |
| --- | --- | --- | --- | --- |
| E0002 | `last_error_arguments` | `@last_error` の後に `(` | `'@last_error' takes no arguments; write '@last_error' before the extern declaration` | `(` |
| E1008 | `last_error_repeated` | 1 つの宣言に 2 つ | `'@last_error' appears twice on one declaration; remove one` | 2 つ目 |
| E0002 | `misplaced_last_error_attribute` | `extern ... def` 以外（`def`・`export def`・`extern type`・`const`）の前 | `'@last_error' applies to an extern function declaration; remove it or declare the function with 'extern'` | 属性 |
| E1008 | `last_error_result_shape` | 結果が `R * i32u` でない、または R が extern の結果に許されない | `'@last_error' externs return the C result paired with the error code; declare the result as 'R * i32u', for example 'bool * i32u'` | extern の名前 |
| E1008 | `last_error_helper_conflict` | `@last_error` を使うプログラムで、明示の symbol `GetLastError` の extern が `i32 ()` に、`SetLastError` が `void (i32)` に下がらない | `extern symbol '{symbol}' is also called by '@last_error' as '{c signature}'; declare it with a 32-bit integer` | その extern の名前 |
| E1037 | `trusted_pure_last_error` | `@trusted` の性質 `pure` と `@last_error` の併用 | `'@last_error' reads the calling thread's error state, so the extern cannot be '@trusted pure'; remove 'pure'` | 性質 `pure` |
| E1008 | `abi_native_only` | wasm の build で `@last_error` の extern の呼び出しを生成した | `'@last_error' reads GetLastError or errno, which only native targets have; build for a native target` | extern の名前 |

- 同じ明示の symbol の宣言の一致（今の `ConflictingExternDeclaration`）は、`@last_error` の extern については結果を R に置き換えた C の署名で比べる。`@last_error` のある宣言とない宣言が同じ symbol を共有してよい（`declare` は 1 つ）。
- `abi_native_only` は W13・W15・W20 などと共有する kind で、先に入ったチケットが登録簿へ足す。wasm でも到達しない extern は拒否しない。`check` は target を知らないので受理する。

## 設計

### 構文と符号化

- `src/syntax.rs`: `LastErrorAttribute { target: Ident, span: Span }` と `Program::last_error: Vec<LastErrorAttribute>`（`trusted` と同じ脇のリスト）。
- `src/parser.rs`: `Attributes` に `last_error: Option<Span>`、`attributes` の繰り返しに `last_error_attribute`。宣言の種類の判定（`TrustedWrongTarget` を出す所）で `extern` と `def` を確かめる。`signature` の次の宣言の属性の先読み（`name == "cpu" || name == "trusted"`）に `last_error` を足す。足さないと、本体のない extern の次の宣言の `@last_error` が制約行として読まれ、`ConstraintLineIndentation` になる。
- `src/syntax_codec.rs`: 符号化の末尾（その時点の最後の脇のリストの後）に足し、`FRONTEND_FORMAT` を着手時の値から 1 上げる。`src/formatter.rs` の正規化（`trusted` と同じく位置を消す）、`src/api_diff.rs` の `Program` の分解（`last_error: _`。組の結果の型が API の差として出るので属性そのものは数えない）。

### 検査（`src/check.rs`）

- `HostImport` に `last_error: bool`。`@trusted` と同じく、名前と宣言の位置で extern を探して立てる。
- `HostImport` に `c_result(&self, result: &Type) -> &Type`（`last_error` なら組の 1 番目、そうでなければ `result`）を足し、extern の結果の ABI を見る所はすべてこれを通す: `check_modules_collect` の `abi::result`、`src/llvm_imports.rs` の `header`（`out_result` と `c_type`）と `host_call`、`src/llvm_abi.rs` の `uses_host_abi`・`c_parameters`（out の引数）・`handle_typedefs`・`record_types`（`header_types` と IR の型の定義の元）。`src/llvm.rs` の `header` の `out_result` は export だけを見るので変えない。
- 明示の symbol の一致の鍵 `(signature.as_type(), wasm_module)` は、結果を `c_result` にした署名にする。
- 補助の symbol の衝突: プログラムに `@last_error` の extern が 1 つでもあれば、明示の symbol `GetLastError`・`SetLastError` の extern の下げた C の型（`abi_type` の列）を比べる（target に依らず両方を見る。POSIX の `__errno_location`・`__error` は `__` の接頭辞で利用者が宣言できない）。`RESERVED_HOST_SYMBOLS` には足さない（今 `extern "GetLastError"` を書いているプログラムを壊すため）。

### 生成

- `src/target.rs`: `Platform::last_error_access()` が `GetLastError`（Windows）・`ErrnoLocation`（Linux。`__errno_location`）・`Error`（それ以外の POSIX。`__error`）を返す。
- `src/llvm.rs`: `blocking_externs(module, wasm) -> bool`（今の `spawning && !wasm` の条件）を 1 つにまとめ、text pass の適用と `host_call` の両方がそれを使う。`Globals` に値を持つ。
- `src/llvm_imports.rs` の `host_call`: `import.last_error` なら、Latin-1 の広げの後、`call` の前後に次を出す。`declare` は `declare void @SetLastError(i32)`・`declare i32 @GetLastError()`・`declare ptr @__errno_location()`（または `@__error`）を `intrinsics` に入れる（メモリの属性は付けない。付けると LLVM が呼び出しを動かしうる）。

```llvm
  call void @tsuzuri_tz_blocking_enter()      ; blocking_externs のときだけ
  call void @SetLastError(i32 0)              ; POSIX: %e = call ptr @__errno_location() / store i32 0, ptr %e
  %v8 = call i32 @CloseHandle(i64 %v7)
  %v9 = call i32 @GetLastError()              ; POSIX: %v9 = load i32, ptr %e
  call void @tsuzuri_tz_blocking_exit()       ; blocking_externs のときだけ
```

- `with_blocking_externs`: 括弧の行（`call void @tsuzuri_tz_blocking_enter()`・`_exit()`）の深さを数え、深さ 0 の行だけを今どおり挟む。括弧の行を見たら `calls` を立て、宣言は今どおり末尾に 1 回足す（`host_call` は括弧の宣言を `intrinsics` に入れない）。`@last_error` のないプログラムでは text pass の前に括弧の行がないので、出力は今と byte 単位で同じ。
- 組は `insertvalue` で 0 番目に R の値、1 番目に読み出した値を入れる（`TypedExprKind::Tuple` の生成と同じ順）。
- wasm: `self.globals.wasm` で `import.last_error` の呼び出しを生成したら、W13・W15 と同じ共有の口（`Globals` の集合に関数の番号を記録し、`emit_program` が関数の生成の後で最も小さい番号の extern の名前の位置に `abi_native_only` を返す）で拒否する。口と kind は W13・W15・W16 のうち先に入ったチケットが作り、後のチケットは条件を足す。

採らなかった案は決定事項 D3 に書く。

## 前提とする他チケットのインターフェース

- なし（単独で出荷できる）。W15 が先なら `abi_native_only` の記録の仕組みを共有し、R に `Maybe<H>` を許す。
- W14 が先なら、W14 の `abi::c_signature` の結果に `c_result`（組の 1 番目）を使う（W14 の記述と同じ）。補助の `GetLastError`・`SetLastError` の `declare` は W14 の `CheckedModule.extern_abis`（symbol ごとに 1 つの正規の `declare`）に乗せ、D7 の検査は W14 の照合（`i32 ()`・`void (i32)` の宣言との分類の比較）で行う。
- W17 が先なら、属性の脇のリスト（`links`）の後に `last_error` を符号化する。

## 他チケットへ提供するインターフェース

- 属性 `@last_error`、`HostImport::last_error`、`HostImport::c_result`（W27 の生成器が win32metadata の `SetLastError = true` の関数に付ける。W14 の比較が使う）。
- `blocking_externs(module, wasm)` と、text pass が既存の括弧の深さを数える規則（呼び出しの前後に別の行を括弧の中に置きたい後のチケットが使う）。

## 実装手順

### 手順 1: ベースライン

- 変更: なし（`target/scratch/w16/` の下書きだけ）。
- 内容: `node scripts/repo-kb.mjs brief W16`。`tests/fixtures/*` と `examples/*` の `Main.tz` の `--emit llvm`（native と `--target wasm32`）を保存する。spawn する fixture（`tests/fixtures/ffi_extensions` に tzroutine を足した写し）の IR も保存する。`FRONTEND_FORMAT` の今の値を控える。
- 確認: 保存した件数を記録する。

### 手順 2: 構文と検査

- 変更: `src/syntax.rs`、`src/parser.rs`、`src/syntax_codec.rs`、`src/frontend_cache.rs`、`src/formatter.rs`、`src/api_diff.rs`、`src/check.rs`、`src/diagnostic_kinds.rs`。
- 内容: 設計の「構文と符号化」と「検査」。仕様 1 と仕様 3 の診断（`abi_native_only` を除く）。
- 確認: T0。`last_error_needs_the_pair_result`・`last_error_attribute_errors`・`helper_declarations_must_lower_to_32_bits`。`tsuzuri fmt` が `@last_error` を保つ。T1。

### 手順 3: 生成

- 変更: `src/target.rs`、`src/llvm.rs`（`blocking_externs`、`with_blocking_externs` の深さ、wasm の拒否）、`src/llvm_imports.rs`（`host_call`・`header`）、`src/llvm_abi.rs`（`c_result` を通す所）。
- 内容: 仕様 2 の消去と読み出しを `host_call` に足し、`with_blocking_externs` に括弧の深さの数え方を足す。wasm の拒否。
- 確認: `capture_follows_the_call`・`capture_sits_inside_the_blocking_bracket`・`wasm_rejects_reached_last_error_externs`。手順 1 の IR と、この機能を使わない入力の IR が `cmp` で一致する。T1。

### 手順 4: E2E と文書

- 変更: `tests/ffi_extensions.mjs`（POSIX）、`tests/windows.mjs`、文書。
- 内容: 「テスト計画」の E2E と「ドキュメント」の節。`node scripts/gen-docs.mjs` で診断の表を作り直す。
- 確認: 「テスト計画」の E2E。T2。PR に `ci:full` を付ける。

## テスト計画

### Rust テスト（`tests/last_error.rs`、新規）

- `last_error_needs_the_pair_result`: `bool * i32u`・`unit * i32u`・`i64 * i32u`・`string * i32u`（所有バッファ）・スカラー record の組を受理。`bool`・`bool * i32`・`i32u * bool`・`bool * i32u * i32u`・`[i64] * i32u` は `E1008` の `last_error_result_shape`。
- `last_error_attribute_errors`: 重複、`def`・`export def`・`extern type` の前、`@last_error(1)`（`E0002`）、`@trusted pure` との併用（`E1037`）。`@trusted @last_error` の 1 行と 2 行の書き方がどちらも受理される。本体のない extern の次の宣言の `@last_error` が制約行と読まれない。
- `same_symbol_with_and_without_last_error`: 同じ symbol を `@last_error` の有無で 2 回宣言して受理され、IR の `declare` が 1 行。
- `helper_declarations_must_lower_to_32_bits`: `extern "GetLastError" def gle :: unit -> i64` と `@last_error` の extern の併用が `last_error_helper_conflict`。`unit -> i32u` なら受理され、`declare i32 @GetLastError()` が 1 行。
- `capture_follows_the_call`: `PlatformScope::enter` で `x86_64-pc-windows-msvc`・`x86_64-unknown-linux-gnu`・`aarch64-apple-darwin` を選び、spawn しないプログラムの IR が「消去 → 呼び出し → 読み出し → 複写の `@tz.free`」の順で、行の間に別の `call` がない。
- `capture_sits_inside_the_blocking_bracket`: `tzroutine { }` を含むプログラムで、`@last_error` の呼び出しの括弧が 1 組だけで消去と読み出しを含み、同じプログラムの `@last_error` のない extern の呼び出しは今どおり 1 行だけを挟まれる。括弧の宣言が末尾に 1 回。
- `wasm_rejects_reached_last_error_externs`: `llvm::emit_target(.., true)` が到達する extern で `abi_native_only`、到達しないだけなら成功。

### E2E

- POSIX（`tests/ffi_extensions.mjs` に段を足す）: `@last_error extern "close" def close_fd :: i32 -> i32 * i32u` に `-1` → `(-1, 9)`（`EBADF` は Linux・macOS とも 9。手で書いた値）。その直後の `@last_error extern "getpid" def pid :: unit -> i32 * i32u` の 2 番目が 0（消去の確認）。同じ検査を `tzroutine { }` の中でも行う。native の `-O0`・`-O3`、`live == 0`。
- Windows（`tests/windows.mjs`）: 早見のプログラムが 6。`extern "SetLastError"` で 123 を置いた後の `@last_error extern "GetCurrentProcessId"` の 2 番目が 0。`tzroutine { }` の中でも同じ。MSVC と同梱の MinGW の両方。
- wasm32・wasm64 の `build` が到達した extern で `E1008`、`check` は成功する。

### 比較

- 手順 1 の基準と、この機能を使わない全入力の `--emit llvm`（native・wasm32）が byte 単位で一致する。

## ドキュメント

- `_tsuzuri/language-reference/values-and-functions/attributes.md` の「属性の一覧と適用対象」の表と、新しい節「@last_error（呼び出し直後のエラー番号）」（書ける位置とエラー）。
- `_tsuzuri/language-reference/compiler/native-interop.md` の「ホスト関数を import する」に例（早見のプログラムと POSIX の `close`）、「スレッドとホストの責任」の `errno` の文に `@last_error` へのリンク。
- `docs/language.md` の「ホスト関数のインポート」（属性、結果の組、順序、wasm）。
- `compiler/diagnostics.md` の生成された表（`node scripts/gen-docs.mjs`）。`node scripts/check-docs.mjs` を変えたページに通す（例は Windows 専用なので、`check-docs` の型検査だけが通る形にし、`run=` は付けない）。

## 受け入れ条件

- [ ] 仕様 1〜3 のとおりに動く（Windows の MSVC・MinGW、Linux、macOS、`-O0`・`-O3`、spawn する・しないプログラム）。
- [ ] spawn するプログラムで、読み出しが `tsuzuri_tz_blocking_exit` より前、消去が `tsuzuri_tz_blocking_enter` より後にある（IR のテスト）。
- [ ] `@last_error` を使わないプログラムの IR（text pass の出力を含む）・object・header・診断が byte 単位で変わらない（比べた件数を記録した）。
- [ ] wasm は到達した extern だけ `E1008`。
- [ ] `tsuzuri fmt` が属性を保ち、`FRONTEND_FORMAT` を上げた。
- [ ] 文書を直し、T2 が成功し、PR に `ci:full` を付けた。

## 落とし穴

- `signature` の先読みに `last_error` を足し忘れると、本体のない extern の直後の宣言に付けた `@last_error` が、前の宣言の制約行として読まれる。
- 消去を引数の評価や Latin-1 の広げ（`tz.string.borrow_utf16` は確保する）より前に置かない。呼び出しの直前に置く。
- 読み出しを `free_copies` や `read_host_result` の後に置かない（`free` と確保は `errno`・last error を変えうる）。
- 消去と読み出しを括弧の外に置かない。`tzr_exitsyscall` は遅い経路で値を戻すが、それに頼ると括弧の順序を変えた後のチケットで黙って壊れる。
- `declare` にメモリの属性（`memory(none)` など）を付けない。LLVM が `GetLastError` を呼び出しの前へ動かしうる。
- `@trusted pure` の extern は `facts::reuse_pure_calls` が呼び出しをまとめうる。組の 2 番目が別の呼び出しの値になるので併用を拒否する。
- `HostImport::c_result` を通し忘れた所は、組の型に `c_type` の `unreachable!` で落ちるか、`out_result` を誤って、`%tz.abi.buffer` の定義やヘッダーの out 引数を落とす。

## 対象外

- Windows の C ランタイムの `errno`（`_wfopen` などの CRT 関数）。要るなら後で `@last_error` に種類の引数を足す（D-53 の改訂）。
- `WSAGetLastError`（Winsock。今の Windows では `GetLastError` と同じ値を返すとされ、そうなら `@last_error` で読める。未確認なので、Winsock の関数を例やテストに使わない）。
- 失敗の判定（戻り値のどの値が失敗か）と `Result` への写し。API ごとに違うので利用者が `match` で書く。
- HRESULT（すでに `i32` の結果で返る）。
- `errno` を戻り値の代わりに読むだけの関数（`@last_error` の extern を呼ぶ）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | 宣言の前の属性 `@last_error` を足す（D-53 の閉じた属性の集合の改訂）。結果は `R * i32u` の組で、C の ABI は変えない | 型の上で「この extern はエラー番号も返す」が見え、別の extern の呼び出しとの間に何も入らない。組は Tsuzuri の側だけの型で、C の結果は R のまま（E12 D11 の「タプルは ABI 型にしない」と矛盾しない）。`Result` にしないのは、失敗の判定が API ごとに違うため | 要承認（承認前は手順 2 以降に着手しない） |
| D2 | 呼び出しの直前に `SetLastError(0)`／`errno = 0` にする | .NET の `SetLastError = true` と同じ。成功しても値を変えない API（`GetCurrentProcessId` の後も 123 が残ることを確認）と、0 が成功か失敗か分からない API（`SetWindowLongPtrW`）で、2 番目が 0 なら「この呼び出しは値を置かなかった」と読める | 既定案（実装者はこの案に従う） |
| D3 | 順序は［括弧の入り］→ 消去 → 呼び出し → 読み出し →［括弧の出］→ 複写の解放。`@last_error` の括弧は `host_call` が自分で出し、text pass は既存の括弧の深さを数えて重ねない | 読み出しを P の取り戻しより前に置く（共通契約）。採らなかった案: (a) 括弧の後に読む（`tzr_exitsyscall` の退避に頼る。正しく動くが、順序の保証がランタイムの実装に移る）、(b) text pass が消去と読み出しの行を印（IR のコメント）で見つけて一緒に挟む（行の形に頼る印が増える）、(c) symbol ごとの内部の関数で包む（W14 の複数宣言と名前が絡み、計装の対象が増える） | 既定案（実装者はこの案に従う） |
| D4 | Windows は `GetLastError`、POSIX は `errno`。Windows の CRT の `errno` は読まない | Win32 の API の契約は `GetLastError`、C の関数の契約は `errno`。Windows で両方を読むと消去と読み出しが倍になり、どちらが意味を持つかは関数ごとに違う | 既定案（実装者はこの案に従う） |
| D5 | native だけ。wasm32・wasm64 は到達した extern だけを `E1008`（`abi_native_only`） | wasm の既定には `errno` も last error もない（GUIDE §1 規則 4。import を足さない）。到達だけで拒否すれば、Windows 向けの宣言を持つパッケージを wasm のプログラムが読み込める | 既定案（実装者はこの案に従う） |
| D6 | 同じ symbol の一致は、結果を C の結果 R にした署名で比べる | `@last_error` の有無は C の関数を変えない。比べ方を Tsuzuri の型のままにすると、同じ関数を両方の形で宣言できない | 既定案（実装者はこの案に従う） |
| D7 | `@last_error` を使うプログラムでは、明示の symbol `GetLastError`・`SetLastError` の extern は `i32 ()`・`void (i32)` に下がる宣言だけを許す（`E1008`） | 違う型の `declare` が 2 つあると、`unique_declarations` が片方を黙って捨て、呼び出しの型と合わなくなる（GUIDE §1 規則 7: 生成で辻褄を合わせない）。`RESERVED_HOST_SYMBOLS` に入れると、今の `extern "GetLastError"` を書いたプログラムが壊れる | 既定案（実装者はこの案に従う） |
| D8 | `@trusted pure` と `@last_error` の併用は `E1037`、ほかの誤りは `E1008`・`E0002` の新しい kind | 新しいコードを割り当てない（E12 D12・D-51）。`@trusted` の性質の誤りは `E1037` の分類。属性の置き場の誤り（`misplaced_last_error_attribute`）は `@cpu` の `misplaced_cpu_attribute` と同じ構文の `E0002`（W13 の `@count`、W17 の `@link`、W21 の `@c_layout` とそろえる） | 既定案（実装者はこの案に従う） |
