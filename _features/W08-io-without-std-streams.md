# W08: 標準ストリームが無いときの IO の意味を決めて実装する

| 項目 | 内容 |
| --- | --- |
| ID | W08 |
| 優先度 | P1 |
| 規模 | S |
| 依存 | – |
| 後続 | [W09](W09-windows-gui-subsystem.md)、[W32](W32-silent-write-failures.md) |
| 状態 | todo |
| 起票 | 2026-11-22（D-63。利用者の依頼「X: D13 を狭く改める方針で決定。実現向けたチケットを @_features に W プリフィックスのチケットを作成して」と「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」から起票。HEAD `fa40eede` で確認） |
| 承認 | 要承認（意味の選択。決定事項 D1・D2。D2 の `IO.Error` への case の追加は、`docs/stability.md` の「既知の例外」の「std の union への case の追加」に当たり、別の承認が要る） |
| 手本にする既存実装 | `src/runtime/io.c` の `tz_io_write_now`・`tz_io_read_line_now`（状態の数を返す形）、`std/IO.tc` の `try_output`・`try_readln`（状態の数から `Result` を作る形）、`src/llvm.rs` の `windows_abi`（Windows の出力だけ IR を書き換える後処理）、`tests/io.mjs` の閉じた標準出力の検査（読み取り専用の fd を標準出力に渡す） |
| 主な影響ファイル | `src/runtime/io.c`・`std/IO.tc`・`src/llvm.rs`（`windows_abi`）・`src/runtime/debug-windows.ll`（新規）・`.gitignore`・`scripts/check-runtime-includes.sh`・`tests/io.mjs`・`tests/windows.mjs`（GUI の段）・`tests/windows.rs`・`_tsuzuri/language-reference/built-in-types-and-modules/io.md`・`_tsuzuri/language-reference/built-in-types-and-modules/debug.md`・`_tsuzuri/language-reference/compiler/platforms.md`・`docs/language.md`・`docs/stability.md` |

## 実装者向け早見

- 範囲: 標準入力・標準出力・標準エラーの「ストリームそのものが無い」ときの意味を、書き込みの失敗と分けて決める。対象は次の 2 つ。
  - Windows の GUI のプロセスを、Explorer などがハンドルなしで起動したとき。
  - POSIX で fd 0〜2 が閉じているとき。
- 決める意味（既定案。D1）:
  - 非 `try_` の読み書きは、今どおりトラップする。
  - `try_` は新しい `IO.Error` の case `Unavailable` を返す（D2）。
  - `Debug.print`・`Debug.trace` は、Windows で標準エラーが無ければ `OutputDebugStringW` に送り、プログラムを続ける。POSIX は変えない。
- 最初に読む: 仕様 1〜3、`src/runtime/io.c`（123 行）、`std/IO.tc` の `try_readln`・`try_output`、`src/runtime/debug.ll`、`src/llvm.rs` の `windows_abi`。
- 最初に書くテスト: `tests/io.mjs` の POSIX の「閉じた標準出力」。`sh -c 'exec "$0" >&-'` で起動した `IO.try_writeln` が `Unavailable` になり、読み取り専用の fd では今どおり `WriteFailed` になることを確かめる。
- 触ってはいけない:
  - ストリームがあって書き込みが失敗したときの結果（`WriteFailed`・`ReadFailed`・トラップ）。
  - `IO` を使わないプログラムの IR。
  - POSIX と WASM の `Debug.print` の IR。
  - in-repo の WASM のホスト（JS の糊・`os-wasi.c`）の振る舞い。
- 完了の見分け方: W06 の起動器でハンドルなしで起動したとき、次の 3 つが成り立つ。
  - `IO.try_writeln` が `Unavailable` を返す。
  - `IO.writeln` がトラップする。
  - `Debug.print` が `OutputDebugStringW` に届き、プログラムが最後まで走る。

## 目的

GUI サブシステムの実行ファイル（W09）は、Explorer やショートカットから起動されると標準ハンドルを持たない。今の Tsuzuri は、その状態の意味を決めていない。結果は経路ごとにばらばらである（「現状」）。

- `IO.writeln` はトラップする。
- `IO.try_writeln` は「書き込みの失敗」と同じ `WriteFailed` を返す。
- `Debug.print` は、ツールチェーンと最適化の段によって、0xC000001D か 0xC0000005 で終わる。
- トップレベルの式の表示は黙って捨てられる。

GUI のアプリが診断の出力を残してよいか、どこへ出すのかを、言語の規則（GUIDE §1 規則 2: 読み飛ばしをしない）に沿って一つに決め、W09 の前提にする。

## 着手条件と停止条件

### 着手条件

- 決定事項 D1・D2 の承認。D2 が却下されたら、仕様 2 の「D2 が却下されたとき」の形で実装する。
- W06 が入っていれば、その起動器（`tests/windows_launch.c` の `--no-handles`・`--debug-strings`）と `launchWithoutHandles` を使う。入っていなければ、W06 の仕様 4 と同じ名前・同じ形で、必要な部分だけをこのチケットで足す。
- `gh run list --branch main --workflow ci.yml --limit 1` が緑。

### 停止条件

- **S1** 次のどちらかが、ARM64 の llvm-mingw か MSVC の arm64 の CRT で起きる（x64 の 2 つの CRT でしか確かめていない）。判定の方法を変える前に報告する。
  - 標準ハンドルなしの fd の `_get_osfhandle` が `-2` にならない。
  - スレッド局所の不正な引数の処理（`_set_thread_local_invalid_parameter_handler`）を入れても、閉じた fd の `_setmode`・`_get_osfhandle` がプロセスを終わらせる。
- **S2** 判定を足したことで、`tests/io.mjs` の今の期待（読み取り専用の fd の `WriteFailed`・`ReadFailed`、閉じた標準出力でのトラップ）が変わる。
- **S3** `IO` を使わないプログラムの IR が変わる。または POSIX・WASM の `Debug.print` の IR が変わる。`std/IO.tc` は常に読み込む std（`src/stdlib.rs` の `OPT_IN` にない）なので、case と分岐を足すと、すべてのプログラムの内部の名前の番号がずれうる（TZ05 の「着手時の再検討」の 1）。ずれたら、そこで止めて選択肢を報告する。A: IR の変化を承認を得て受け入れる。B: D2 が却下されたときの形（`std/IO.tc` を変えない）にする。
- **S4** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

### コード

- **書き込み**: `src/runtime/io.c` の `tz_io_write_now` は、Windows では `_setmode(destination, _O_BINARY)` の後に `fwrite` と `fflush` を呼ぶ。どれかが失敗すると 1、成功すると 0 を返す。`tsuzuri_io_write` はそれを返す。
- **読み取り**: `tz_io_read_line_now` は、行なら 0、入力の終わりなら 1、`ferror` なら 2 を返す。
- **std での受け方**: `std/IO.tc` の `try_output` は、`IO.__write` が 0 なら `Ok`、それ以外なら `WriteFailed` にする。`try_readln` は `(0, _)` を行、`(1, _)` を `None`、ほかを `ReadFailed` にする。非 `try_` の関数は `map Result.get` で、`Error` ならトラップする。
- **`IO.Error`** は `union Error = ReadFailed | WriteFailed | InvalidEncoding`（`std/IO.tc` の 3 行目）。
- **`Debug.print`** は `Debug.__print_string` を通り、`src/runtime/debug.ll` の `@tz.debug.write` に下がる。`@write(2, ...)` を繰り返し、進まなければ `bad:` で `llvm.trap` する。C のランタイムは連結しない。Windows では、`src/llvm.rs` の `windows_abi` が `@write` を `_setmode` と `_write` の shim に置き換える。
- **トップレベルの式の表示**: `src/llvm.rs` の `console_main` が、`src/runtime/console.ll` の `@tz.console.write` で表示する。`putchar` を 1 byte ずつ呼び、`fflush` はしない。失敗すると終了コードを 1 にする。`def main :: unit -> i32`（`console_def_main`）は何も表示しない。
- **WASM**: 既定の wasm32 では、`tsuzuri_io.read_line` と `tsuzuri_io.write` の import のホストが状態の数を返す（`io.md` の「WASM」: read は 0・1・2、write は成功で 0）。`src/runtime/os-wasi.c` の `tsuzuri_io_write` は 0 か 1、`tsuzuri_io_read_line` は 0・1・2 だけを返す。

### このマシンで確かめた挙動

ツールチェーンは 2 つ使った。1 つは開発版 `target/debug/tsuzuri.exe` と同梱の 0.1.6（MinGW、zig 0.16.0）。もう 1 つは MSVC 14.44 の CRT と SDK を使う clang 22.1.8 で、既定のリンカーは lld-link。起動は W06 の起動器の試作による（`STARTF_USESTDHANDLES`・NULL・`DETACHED_PROCESS`）。

| プログラム（`def main :: unit -> i32`） | 標準ハンドルあり | ハンドルなし（MinGW） | ハンドルなし（MSVC） |
| --- | --- | --- | --- |
| `do! IO.writeln "hello"; 7` | 7 | 0xC000001D | 0xC000001D |
| `IO.try_writeln` の結果（`Ok` 10、`WriteFailed` 11、ほか 12） | 10 | 11 | 11 |
| `IO.try_readln` の結果（`None` 20、`Some` 21、`ReadFailed` 22、ほか 23） | 20（入力なし） | 22 | 22 |
| `Debug.print (ref s); 5` | 5 | `-O0` は 0xC000001D、`-O1`〜`-O3` は 0xC0000005 | 0xC000001D |
| トップレベルの式 `42` | `42` を表示して 0 | 0（表示は消える） | 0 |
| トップレベルの `IO.pure 3` | 3 | 3 | 3 |

- C の探りで見ると、ハンドルなしの子では次のようになった（両方の CRT で同じ）。
  - fd 0〜2 の `_get_osfhandle` は `-2`、`GetStdHandle` は NULL。
  - `_setmode(fd, _O_BINARY)` は成功する（前のモード 16384 を返し、`errno` は 0）。
  - `_write(1, ...)` は -1 を返し、`errno` は 9（`EBADF`）。
  - つまり失敗は `_setmode` ではなく書き込みで起きる。
- **閉じた fd**（`_close(2)` の後）は別の結果になる。静的な UCRT（clang の MSVC の既定の `libcmt`）では、`_setmode(2, _O_BINARY)` の中で CRT の不正な引数の処理が働き、プロセスが 0xC0000409 で終わった（W04 の「現状」の測定と同じ。このマシンでも確かめた）。何もしないスレッド局所の処理を `_set_thread_local_invalid_parameter_handler` で入れると、両方の CRT（静的な UCRT と、`api-ms-win-crt-*` を import する zig の MinGW の UCRT）で、`_setmode` と `_get_osfhandle` が −1・`errno` 9 を返して続いた。zig の MinGW のヘッダーはこの関数を宣言しないが、自分で宣言すればリンクできた。
- `Debug.print` の 0xC0000005 を、デバッグ API の起動器で追った。最初の例外は Tsuzuri のコードの `ud2`（0xC000001D、exe の中の RVA）だった。その後、例外の配送の間に ntdll の中で読み出しの違反（0xC0000005）が続いた。`-O0` と MSVC では、2 回目の配送で 0xC000001D のまま終わる。Tsuzuri のコードに unwind 情報（`.pdata`）がないため、スタックをたどるときに壊れた戻り先を読むと推定する。原因の修正は [W07](W07-windows-trap-fail-fast.md)（`uwtable`）の範囲で、このチケットでは直さない。
- トップレベルの式の表示は `putchar` がバッファに書くだけで、`fflush` を呼ばない。そのため、ストリームが無くても失敗が返らず、表示は黙って消える。

## 変えてはいけない意味

- ストリームがあって読み書きが失敗したときの結果は変えない。対象は `try_` の `ReadFailed`・`WriteFailed`、非 `try_` のトラップ、`Debug.print` のトラップ。`tests/io.mjs` の読み取り専用の fd の検査が、この区別を固定する。
- 非 `try_` の読み書きは、ストリームが無くてもトラップする。失敗を読み飛ばさない（GUIDE §1 規則 2）。
- WASM の in-repo のホストと `--wasm-host wasi` の結果。

## 仕様

### 1. 「ストリームが無い」の判定

読み書きが失敗したときだけ、その fd について次を確かめる。成功の経路には判定を足さない。

| OS | 無いとみなす条件 | 根拠 |
| --- | --- | --- |
| Windows | `_get_osfhandle(fd)` が `-2`、または `-1` で `errno == EBADF` | `-2` は、起動時にハンドルが無効だった fd 0〜2 を CRT が `_NO_CONSOLE_FILENO` にしたもの（両方の CRT で確かめた）。`-1`・`EBADF` は、閉じた fd |
| POSIX | `fcntl(fd, F_GETFD) == -1 && errno == EBADF` | 閉じた fd。読み取り専用の fd への書き込みも `EBADF` になるが、`F_GETFD` は成功するので区別できる |

Windows では、fd 0〜2 に対する CRT の呼び出し（`_setmode`・書き込み・`_get_osfhandle`）を、何もしないスレッド局所の不正な引数の処理の下で行う。対象は `io.c` と、`Debug.print` の Windows 版。閉じた fd で 0xC0000409 で終わらず、失敗として返すためである。

ランタイムの状態の数を次のように増やす。WASM の import の返す値も同じ意味にする（仕様 4）。

| 関数 | 0 | 1 | 2 | 3 |
| --- | --- | --- | --- | --- |
| `tsuzuri_io_write` | 成功 | 失敗 | **ストリームが無い**（新） | – |
| `tsuzuri_io_read_line` | 行 | 入力の終わり | 失敗 | **ストリームが無い**（新） |

### 2. `IO` の意味

- `IO.Error` の末尾に `Unavailable` を足す（`union Error = ReadFailed | WriteFailed | InvalidEncoding | Unavailable`）。意味は「その標準ストリームがプロセスに無い」（D2）。
- `try_write`・`try_writeln`・`try_write_err`・`try_write_errln`: 状態 2 を `Error Unavailable` にする。孤立サロゲートの `InvalidEncoding` が先なのは今と同じ。
- `try_readln`: 状態 3 を `Error Unavailable` にする。入力の終わり（`None`）とは区別する。
- 非 `try_` の 5 つは、`Unavailable` でも今と同じくトラップする。トラップの報告は、標準エラーが無ければ [W04](_completed/W04-windows-trap-report-lost.md) の経路で `OutputDebugStringW` に届く。
- **D2 が却下されたとき**: case を足さない。状態 2・3 を、それぞれ `WriteFailed`・`ReadFailed` にする。このとき `std/IO.tc` は変えず、ランタイムだけが新しい状態を返す。`IO.tc` の受け方が「0 以外は失敗」なので、結果は今と同じになる。仕様 3 の `Debug.print` は D2 によらず入れる。

### 3. `Debug.print`・`Debug.trace`

- Windows の native では、標準エラーへの書き込みが最初の 1 byte も書けずに失敗し、fd 2 が仕様 1 の判定で「無い」とき、同じ文字列と LF を UTF-16 にして `OutputDebugStringW` に送る。変換は `MultiByteToWideChar(CP_UTF8, 0, ...)` で行う。その後プログラムを続ける。
- 途中まで書けてから失敗したとき、またはストリームがあって失敗したときは、今どおりトラップする。
- POSIX の native と WASM は変えない。POSIX には OS の既定のデバッグ出力の経路が無く、閉じた fd 2 は珍しいので、トラップのまま失敗を見せる。

### 4. WASM の import

- `tsuzuri_io.write` の 2 と `tsuzuri_io.read_line` の 3 を、「ホストにそのストリームが無い」として受ける。ホストはこれを返してもよいが、返さなくてもよい。
- in-repo の JS のホストと `os-wasi.c` はこれらを返さないので、結果は変わらない。`write` に 0・2 以外を、`read_line` に 0〜2 だけを返す既存のホストの結果も変わらない。

### 5. 変えないもの

- トップレベルの式の表示（`@tz.console.write`）は変えない（D4）。GUI のアプリは `def main :: unit -> i32` を使う、と W09 の文書に書く。表示の失敗の見落としは、このチケットの D1 の決定に従って [W32](W32-silent-write-failures.md) が直す。
- Windows の `IO.write_err` などの、ストリームが無いときの報告先は `OutputDebugStringW` にしない。`try_` を使えば `Unavailable` が返る。

## 設計

### ランタイム（`src/runtime/io.c`）

```c
/* W08: whether the standard stream `fd` does not exist, asked only after a read or write on it failed. */
static int tz_io_absent(int fd) {
#if defined(_WIN32)
    errno = 0;
    intptr_t handle = _get_osfhandle(fd); /* under tz_io_quiet: a closed descriptor gives -1 and EBADF */
    return handle == -2 || (handle == -1 && errno == EBADF); /* -2 is _NO_CONSOLE_FILENO: no handle at startup */
#else
    int saved = errno;
    int absent = fcntl(fd, F_GETFD) == -1 && errno == EBADF;
    errno = saved;
    return absent;
#endif
}
```

- Windows では、`tz_io_write_now`・`tz_io_read_line_now` の本体を、`_set_thread_local_invalid_parameter_handler(tz_io_quiet)` と、前の処理を戻す呼び出しで挟む（`tz_io_quiet` は何もしない関数）。MinGW のヘッダーは宣言しないので、`io.c` が `_invalid_parameter_handler __cdecl _set_thread_local_invalid_parameter_handler(_invalid_parameter_handler);` を宣言する（MSVC のヘッダーの宣言と同じ型）。
- `tz_io_write_now`: 失敗で 1 を返す 3 か所（`_setmode`・書き込み・`fflush`）を、`tz_io_absent(destination) ? 2 : 1` にする。
- `tz_io_read_line_now`: `ferror` で 2 を返す所を、`tz_io_absent(0) ? 3 : 2` にする。
- POSIX では `<fcntl.h>` と `<unistd.h>` を include する。`errno` は既にある。
- `TZ_BLOCKING_ENTER`・`TZ_BLOCKING_EXIT`（TZ08）の位置は変えない。判定はその内側で行う。

### std（`std/IO.tc`）

- `union Error` の末尾に `Unavailable`。
- `try_output`: `IO.__write ...` の結果を `match` で受ける。`0` → `Ok`、`2` → `Error Unavailable`、ほか → `Error WriteFailed`。
- `try_readln`: `(3, _) -> Result.Error Unavailable` を `(1, _)` の後に足す。
- 各関数の `///` に `Unavailable` を書く。G34 の例は今のまま通る（ストリームがある前提）。

### Windows の `Debug.print`（`src/runtime/debug-windows.ll`、`src/llvm.rs` の `windows_abi`）

- `debug-windows.ll` は、`@tz.debug.write` の Windows 版の定義だけを持つ。
  - 本体は `debug.ll` と同じく `@write(2, ...)` で書く。そのため `windows_abi` の `_setmode`・`_write` の shim はそのまま使う（W04 が `_setmode` の −1 を失敗にする）。
  - 書き込みの前後を、`io.c` と同じくスレッド局所の何もしない処理（IR の `@tz.debug.quiet`）で挟む。閉じた fd 2 の `_setmode` で終わらないようにするためである。
  - 失敗の分岐で `offset == 0` かつ fd 2 が無い（`_get_osfhandle(2)` が `-2`、または `-1` で `errno` が `EBADF`。`errno` は `_errno()` で読む）なら、`@tz.debugger.write(ptr %data, i64 %length)` を呼んで戻る。それ以外はトラップする。
  - 送る部分は [W04](_completed/W04-windows-trap-report-lost.md) の `@tz.debugger.write(ptr %data, i64 %length)`（`src/llvm_traps.rs` の `DEBUGGER_WRITE`。スタックの 512 バイトで 255 バイトずつ `MultiByteToWideChar` で変換して `OutputDebugStringW` に送る）の 1 つだけを使い、別名の補助は作らない。改行は足さない: `debug.ll` の `@tz.debug.write` は改行を別の `@write` で書くので、送るのは文の本体だけで、`OutputDebugStringW` の 1 回の呼び出しがデバッガーの 1 項目になる（255 バイトを超える文は W04 と同じく複数の項目に分かれる）。
  - 宣言の型は `declare i64 @_get_osfhandle(i32)`、`declare ptr @_errno()`、`declare ptr @_set_thread_local_invalid_parameter_handler(ptr)`、`declare i32 @MultiByteToWideChar(i32, i32, ptr, i32, ptr, i32)`、`declare void @OutputDebugStringW(ptr)`。kernel32 と CRT の関数なので、リンクの入力は増えない。
  - 長さが `INT32_MAX` を超える文字列は、`MultiByteToWideChar` の `int` に合わせて分けて送る。
- `windows_abi` は、`define internal void @tz.debug.write(%tz.utf8string %text) nounwind {` の行から対応する `}` までを、`debug-windows.ll` の定義に置き換える。宣言は重複させない（`intrinsics` と同じく、既にある `declare` は足さない）。
  - `windows_abi` は Windows の native の出力にだけ掛かるので、POSIX と WASM の IR は 1 byte も変わらない。
  - 却下した案: 出力の生成時に `current_platform()` で `debug.ll` を選び分ける。`tests/windows.rs` のように後処理だけを Windows にするテストの経路と、ずれるため採らない。
- `.gitignore` の `!src/runtime/debug-windows.ll` と、`scripts/check-runtime-includes.sh` を更新する（GUIDE-LITE §7）。

### 診断

- 新しい診断はない。`E1021`（網羅的でない `match`）が、`IO.Error` を網羅的に受ける利用者のコードに出る。これは D2 の影響で、release notes と `docs/stability.md` に書く。

## 前提とする他チケットのインターフェース

- [W06](W06-windows-gui-test-harness.md) の仕様 4: `windows_launch.exe --no-handles`・`--debug-strings <path>`、`launchWithoutHandles`。W06 が未着手なら、同じ形で足す（着手条件）。
- [W04](_completed/W04-windows-trap-report-lost.md): 標準エラーが無いときのトラップの報告の `OutputDebugStringW` への経路。デバッガーへ送る補助は W04 の `@tz.debugger.write(ptr, i64)`（`DEBUGGER_WRITE`）をそのまま使う。

## 他チケットへ提供するインターフェース

- 状態の数（write の 2、read の 3）と `IO.Unavailable`（D2 が承認されたとき）。
- W09 の文書が引く意味: 「GUI サブシステムの実行ファイルを標準ハンドルなしで起動したとき、`try_` は `Unavailable`、非 `try_` はトラップ、`Debug.print` は `OutputDebugStringW`」。
- [W32](W32-silent-write-failures.md) が使うもの: D1 の決定（W32 の仕様 3 の表で、トップレベルの表示と Windows の `--overflow-report` の振る舞いが決まる）、仕様 1 の「ストリームが無い」の判定（D1 が既定案でないときだけ）、`windows_abi` の `DEBUGGER_WRITE`。`io.md` の「終了コードとトップレベルの表示」にこのチケットが書く文（表示はストリームが無いと消える）は、W32 が置き換える。

## 実装手順

1. **変更**: なし。**内容**: `node scripts/repo-kb.mjs brief W08`。基準（`ci.yml`）。`grep -rn "WriteFailed\|ReadFailed\|InvalidEncoding" std examples guide tests _tsuzuri docs` で、`IO.Error` を網羅的に受ける所を数え直す。HEAD で見つかったのは、`io.md` の `run=encoding` の例と `tests/io.mjs` の `report` の 2 か所。`tests/fixtures` と `examples` のすべての `Main.tz` の `--emit llvm`（native と `--target wasm32`、`-O1`）を保存する（TZ05 の比べ方）。**確認**: 記録。
2. **変更**: `src/runtime/io.c`。**内容**: 仕様 1。**確認**: `node tests/io.mjs target/debug/tsuzuri` が今のまま通る（std がまだ「0 以外は失敗」なので結果は同じ）。`sh scripts/check-runtime-includes.sh`。
3. **変更**: `std/IO.tc`、`io.md` の例、`tests/io.mjs` の `report`。**内容**: 仕様 2（D2 の承認後）。**確認**: すぐに、手順 1 で保存した IR と比べる。`IO` を使わないプログラムが 1 byte でも変われば S3。続けて `cargo nextest run --locked`、`tsuzuri test --doc std`、`tests/io.mjs` の新しい検査（テスト計画）。
4. **変更**: `src/runtime/debug-windows.ll`、`src/llvm.rs`、`.gitignore`、`scripts/check-runtime-includes.sh`、`tests/windows.rs`。**内容**: 仕様 3。**確認**: `cargo test --locked --test windows`（`running N tests` の N を確かめる）。`tests/fixtures`・`examples` の POSIX と wasm32 の `--emit llvm` が手順 1 と同じ。
5. **変更**: `tests/windows.mjs`（GUI の段）。**内容**: テスト計画の Windows の検査。**確認**: `node tests/windows.mjs target/debug/tsuzuri.exe --stage gui` を MSVC と同梱で回す。
6. **変更**: 文書。**確認**: T2、`node scripts/check-docs.mjs <変更したページ>`、`ci:full` を付けた PR で `windows.yml`・`vscode.yml`。

## テスト計画

### POSIX（`tests/io.mjs`。native の `-O0`・`-O3`）

| 検査 | 起動 | 期待 |
| --- | --- | --- |
| 閉じた標準出力の `try_writeln`・`try_write` | `sh -c 'exec "$0" >&-' <exe>` | `Unavailable`（`report` が `unavailable` を標準エラーに書く） |
| 閉じた標準エラーの `try_write_errln` | `2>&-`（結果は標準出力に書く） | `Unavailable` |
| 閉じた標準入力の `try_readln` | `<&-` | `Unavailable`（`None` ではない） |
| 閉じた標準出力の `writeln` | `>&-` | 0 でない終了（トラップ）。後ろの `write_err` は走らない |
| 読み取り専用の fd の標準出力（既存） | 今のまま | `WriteFailed`（変わらない） |
| 閉じた標準エラーの `Debug.print` | `2>&-` | トラップ（POSIX は変えない） |

### WASM（`tests/io.mjs` の `host`）

- `write` が 2 を返すホストでは `Unavailable`、1 を返すホストでは `WriteFailed`。
- `read_line` が 3 を返すホストでは `Unavailable`、2 を返すホストでは `ReadFailed`。
- 既存の WASM の検査はそのまま通る。

### Windows（`tests/windows.mjs` の GUI の段。`-O0`・`-O3`、MSVC と同梱）

| fixture | 起動 | 期待 |
| --- | --- | --- |
| `io`（`try_writeln` → `Unavailable` 13、`WriteFailed` 11、`Ok` 10） | `launchWithoutHandles` | 13。ハンドルありでは 10 |
| `io_read`（`try_readln` → `Unavailable` 24） | 同上 | 24 |
| `io_trap`（`IO.writeln`） | 同上 | 0 でない（W06 の仕様 3 の集合） |
| `debug`（`Debug.print (ref s); 5`） | `windows_launch.exe --no-handles --debug-strings <file>` | 終了コード 5、`<file>` に `note` の 1 行 |

### Rust（`tests/windows.rs`）

- `Debug.print` を使うモジュールに `windows_abi` を掛けた IR を検査する。`@OutputDebugStringW` の `declare` が 1 つあり、`@tz.debug.write` の定義が 1 つで、`declare i32 @_write(i32, ptr, i32)` が残る。
- 同じモジュールの Windows でない出力は、`OutputDebugStringW` を含まない。
- IR は決定的（2 回の emit が同じ）。

## ドキュメント

- `built-in-types-and-modules/io.md`:
  - 「標準出力、標準エラー、標準入力」に「ストリームが無いとき」の節を足す（Windows の GUI のプロセス、POSIX の閉じた fd）。
  - 「公開 API」の `IO.Error` の表に `IO.Unavailable` を足す。
  - `run=encoding` の例に `| Result.Error IO.Unavailable -> "unavailable"` を足す。
  - 「WASM」に状態の 2・3 を書く。
  - 「終了コードとトップレベルの表示」に、表示はストリームが無いと消えること（D4）を書く。[W32](W32-silent-write-failures.md) がこの文を「書けなければ終了コード 1」に置き換える。W32 が先に入ることはない（W32 はこのチケットに依存する）。
- `built-in-types-and-modules/debug.md`: Windows で標準エラーが無いときは `OutputDebugStringW`（Visual Studio の出力・DebugView で読める）。
- `compiler/platforms.md`: W03 が書いた既知の制限（標準ハンドルのないプロセスで `IO.writeln` がトラップする）を、実際の意味の説明とこのページへのリンクに置き換える。
- `docs/language.md`: 「IO と標準入出力」の `IO.Error` の行と、ストリームが無いときの段落。
- `docs/stability.md`:
  - 「既知の例外」に、`IO.Error` への `Unavailable` の追加（全 edition。std は edition ごとに複製しない）を記録する。
  - POSIX の閉じた fd で `try_` の結果が `WriteFailed` から `Unavailable` に変わったことを記録する。
- `std/IO.tc` の `///`。

## 受け入れ条件

- [ ] 仕様 1〜4 のとおりに動く（テスト計画の POSIX・WASM・Windows・Rust）。
- [ ] 読み取り専用の fd の `WriteFailed`・`ReadFailed` と、閉じた標準出力のトラップの既存の検査が、期待を変えずに通る。
- [ ] `IO` も `Debug` も使わないプログラムの IR が、native と wasm32 で 1 byte も変わらない。POSIX と WASM の `Debug.print` の IR も変わらない。
- [ ] D2 が承認された場合は `IO.Unavailable` を足した。却下された場合は、仕様 2 の代わりの形にした。
- [ ] `docs/stability.md` と release notes に、case の追加と POSIX の変化を書いた。
- [ ] `ci:full` を付けた PR で、`windows.yml` と `vscode.yml` の Windows の job が通る。
- [ ] T2（`cargo nextest run --locked`、`tests/io.mjs`、`tests/windows.mjs --stage gui`、変更したページの `check-docs`、`check-runtime-includes.sh`）が通る。

## 落とし穴

- **判定は失敗の後だけ**: 成功の経路に `_get_osfhandle`・`fcntl` を足すと、書き込みごとに system call が増える。
- **CRT の fd の呼び出しは、何もしない処理の下で**: 閉じた fd の `_setmode`・`_get_osfhandle` は、静的な UCRT では 0xC0000409 でプロセスを終わらせる（確かめた）。`_set_thread_local_invalid_parameter_handler` で挟み、戻すのを忘れない（ホストの処理を壊さない）。ハンドルの無い fd 0〜2 は、処理がなくても安全だった（CRT の表にある）。
- **`errno` を壊さない**: POSIX の判定は、呼ぶ前の `errno` を戻す。
- **0xC0000005 を直したと書かない**: ストリームが無いときの `Debug.print` はトラップしなくなるので、この経路では観測されなくなる。ただし、unwind 情報が無いことによる例外の配送の壊れは、ほかのトラップに残る（W07）。Windows のトラップの検査は、W06 の仕様 3 の集合で受ける。
- **`OutputDebugStringW` はデバッガーがなければ何もしない**: 結果を確かめるのは、デバッグ API の起動器（W06 の `--debug-strings`）だけ。
- **case の追加は利用者のコードを壊す**: `match` で `IO.Error` を網羅的に受け、`_` の腕がないコードは `E1021` になる。
- **常に読み込む std を変えると、すべての IR が変わりうる**: TZ05 は、`std/Time.tz` に関数や `const` を足しただけで、`examples/hello` を含むすべてのプログラムの内部の名前の番号がずれた。`std/IO.tc` も常に読み込む。手順 3 の直後に比べる（S3）。

## 対象外

- トップレベルの式の表示（`@tz.console.write`）が、`fflush` をせず、失敗を見落とすこと。ストリームがある場合の書き込みの失敗も見落としうる、既存の欠陥である。修正は [W32](W32-silent-write-failures.md)（このチケットの D1 の決定に従う）。
- Windows のトラップの報告先（W04）と終わり方（W07）。
- `--wasm-host wasi` の閉じた fd の扱い（`os-wasi.c` は変えない）。
- `--overflow-report` の報告の書き込み（`src/llvm_overflow.rs` の `@tz.overflow.write`。書けなければ黙ってやめる今の形）。W04 はこれも W08 の範囲に挙げたが、このチケットは変えない。開発用の計装で、`run`・`test` は常にパイプを渡す。[W32](W32-silent-write-failures.md) が直す（Windows では W04 の報告の書き手 `@tz.trap.write` へ転送し、書けなければ `@tz.debugger.write` に送る。POSIX と WASM は変えない）。
- 標準ストリームを後から作る API（`AllocConsole` の包みなど）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | ストリームが無いときの意味は 3 つにする。非 `try_` はトラップ、`try_` は `Error`、`Debug.print` は Windows で `OutputDebugStringW` に送って続ける。却下した案は 2 つ。A: 全 OS で捨てて `Ok`（Rust の std の `handle_ebadf`）。B: GUI のときだけ `OutputDebugStringW` に送って `Ok` | GUIDE §1 規則 2（読み飛ばしをしない）。A は書いたはずの出力が黙って消える。B はサブシステムで意味が変わる。また、ハンドルを渡された GUI の exe とハンドルのないコンソールの exe で結果が逆になる。`Debug.print` だけを `OutputDebugStringW` に送るのは、診断の出力で、Windows に標準の受け手があるため | 要承認（承認前は該当する段に着手しない） |
| D2 | `IO.Error` に `Unavailable` を足す（末尾。全 edition） | 「出せなかった」と「出す先が無い」を分ければ、GUI のアプリは後者を正常として扱える。`docs/stability.md` の「std の union への case の追加」に当たり、網羅的な `match` を壊すので別の承認が要る。std は edition ごとに複製しない（同ページ）ので、edition で分けない。却下されたら仕様 2 の代わりの形にする | 要承認（承認前は該当する段に着手しない） |
| D3 | 判定は失敗の後にだけ行う。Windows は `_get_osfhandle(fd)` の `-2` か `-1`・`EBADF` で、CRT の呼び出しはスレッド局所の何もしない不正な引数の処理の下で行う。POSIX は `fcntl(F_GETFD)` の `EBADF` | 両方の CRT で、ハンドルの無い fd が -2 になり、処理の下では閉じた fd が −1・`EBADF` になることを確かめた。読み取り専用の fd の `EBADF` と区別できる。成功の経路の費用は、処理の付け外しの 2 回の呼び出しだけ | 既定案（実装者はこの案に従う） |
| D4 | トップレベルの式の表示は変えない | すべてのスクリプトの形のプログラムの IR を変える。表示の失敗の見落としは、ストリームの有無と別の既存の欠陥で、[W32](W32-silent-write-failures.md) が D1 の決定に従って直す。GUI のアプリは `def main` を使う | 既定案（実装者はこの案に従う） |
| D5 | Windows の `Debug.print` の差し替えは、`windows_abi` の後処理で行う | POSIX と WASM の IR を 1 byte も変えずに済む。`windows_abi` は既に Windows の出力だけの IR の書き換えを持つ | 既定案（実装者はこの案に従う） |
| D6 | WASM の import の状態の 2（write）と 3（read）を、ストリームが無い意味として受ける | 意味を target でそろえる。in-repo のホストはこれらを返さないので、結果は変わらない | 既定案（実装者はこの案に従う） |
