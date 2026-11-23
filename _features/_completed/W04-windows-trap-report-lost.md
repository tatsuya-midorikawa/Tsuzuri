# W04: Windows でトラップの報告が黙って消える経路を塞ぐ

| 項目 | 内容 |
| --- | --- |
| ID | W04 |
| 優先度 | P1（修正。GUI サブシステムの実行ファイル（W09）や標準エラーを閉じたホストでは、トラップの理由がどこにも残らない） |
| 規模 | S（IR の書き手 2 つの差し替え、ランタイム C の報告 3 か所、Windows の E2E の段と小さな C の補助） |
| 依存 | – |
| 後続 | [W07](../W07-windows-trap-fail-fast.md) |
| 状態 | done |
| 起票 | 2026-11-22（D-63。利用者の依頼「X: D13 を狭く改める方針で決定。実現向けたチケットを @_features に W プリフィックスのチケットを作成して」と「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」から起票。HEAD `fa40eede` で確認） |
| 承認 | 不要（報告の届け先を足すだけで、トラップの意味・終了コード・報告の文・POSIX と WASM の出力は変えない） |
| 手本にする既存実装 | `src/llvm.rs` の `windows_abi`（Windows だけの IR の差し替えと宣言の追加）、`src/llvm_traps.rs` の `TRAP_WRITE`・`with_frame_pointers`、`src/runtime/stack.c` の `tz_stack_fault`（報告を書いてから終わる）、`src/runtime/tzroutine-windows.h`（Windows だけの C を `tzroutine.c` に埋め込む） |
| 主な影響ファイル | `src/llvm.rs`, `src/llvm_traps.rs`, `src/runtime/tzroutine.c`, `src/runtime/tzroutine-windows.h`, `tests/windows.mjs`, `tests/windows_launch.c`（W06 の起動器。W04 が先ならその一部を作る）, `docs/architecture.md`, `_tsuzuri/language-reference/compiler/platforms.md`, `_tsuzuri/language-reference/compiler/native-interop.md` |

## 実装者向け早見

- 範囲: Windows の native の出力で、(a) `--trap-info` のトラップの報告（`tsuzuri run` は既定で付ける）と、(b) tzroutine のランタイム C の報告（`tzr_fatal`、デッドロックの報告、`tsuzuri_tz_report`）を、標準エラーが無い・書けないときに `OutputDebugStringW` へ送る。終わり方（終了コード、fail-fast）は変えない（W07）。
- 最初に読む: 「現状」の 3 節 → `src/llvm_traps.rs` の `TRAP_WRITE` → `src/llvm.rs` の `windows_abi` → `src/runtime/tzroutine.c` の `tzr_fatal`・`tzr_report_deadlock`・`tsuzuri_tz_report`。
- 最初に書くテスト: `src/llvm.rs` の単体テスト `windows_trap_report_falls_back_to_the_debugger`（IR の形）→ `tests/windows.mjs` の段（W06 の起動器 `tests/windows_launch.c` の `--no-handles`・`--debug-strings`）。
- 触ってはいけない: POSIX と WASM の `--emit llvm` と、ランタイム C が POSIX で書くバイト列。終了コード。`Debug.print`・`--overflow-report`・`IO.*`・`tz.console.write` の書き込み先（標準ストリームが無いときの意味は W08 が決める）。
- 完了の見分け方: 次の最後の 2 行の 1 つ目で `ods.txt` に `trap: integer division by zero at ...` がちょうど 1 行入り、2 つ目の出力の最後の行が `exit=0xC000001D` になる（今は `ods.txt` が空）。

```sh
printf 'def divide :: i64 -> i64\nfn divide x = 10 / x\ndivide 0\n' > trap/Main.tz
tsuzuri build trap --trap-info -o trap.exe
windows_launch --no-handles --debug-strings ods.txt -- trap.exe
windows_launch --no-handles -- trap.exe
```

## 目的

Windows の GUI サブシステムの実行ファイルを Explorer から起動すると、標準ハンドルが無い。`tsuzuri run` や `spawnSync` は必ずハンドルを渡すので、既存のテストはこの状態を見ない。今は、この状態でトラップすると理由の文が黙って捨てられ、ランタイム C の報告（デッドロック、`WaitGroup` の負の値など）も同じく消える。開発者は Visual Studio・WinDbg・DebugView の出力で理由を読めるべきである。このチケットは、報告の文を「標準エラーに書く。書けなければデバッガーの出力に送る」に直す。

## 着手条件と停止条件

### 着手条件

- `grep -n "const TRAP_WRITE" src/llvm_traps.rs`、`grep -n "fn windows_abi" src/llvm.rs`、`grep -n "static void tzr_fatal\|static void tzr_report_deadlock\|TZR_API void tsuzuri_tz_report" src/runtime/tzroutine.c` が 1 件ずつ当たり、本文が「現状」と一致すること。
- W06 がすでに `main` にあれば、その起動器 `tests/windows_launch.c` と `tests/windows.mjs` の export（`launchWithoutHandles`）を使う。W04 が先なら、W06 の仕様 4 の起動器のうち `--no-handles`・`--debug-strings <path>`・`--timeout-ms N`・`-- <exe> [args...]` だけを、同じ名前・同じ出力の形で先に作る（W06 が残りを足す）。

### 停止条件

- **S1** `GetStdHandle`・`WriteFile`・`OutputDebugStringW`・`MultiByteToWideChar` のどれかが、MSVC（link.exe）か同梱の MinGW（zig）の既定のリンクで解決しない。起票時は両方で `-lkernel32` なしに解決した（C の試験プログラムで確認）。
- **S2** POSIX か WASM の `--emit llvm`（`tests/fixtures`・`examples` のすべての `Main.tz`、`-O0`・`-O3`）が 1 byte でも変わる。
- **S3** CI の Windows のランナーで、`DEBUG_ONLY_THIS_PROCESS` の子が作れない、またはデバッグイベントが届かない。代わりの案（`DBWIN_BUFFER` を読む）は名前付きの共有オブジェクトを使い並列のテストと衝突するので、即興で切り替えずに報告する。
- **S4** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

### 書き込みの経路

- `src/llvm_traps.rs` の `instrument_with` は、`--trap-info` のビルドでだけ、トラップの地点ごとに `call void @tz.trap.report(i32 site)` を置き、報告の関数が `@tz.trap.write(ptr, i64)`（定数 `TRAP_WRITE`）で文を書く。`TRAP_WRITE` は `@write(i32 2, ...)` を繰り返し、`%progress = icmp sgt i64 %written, 0` が偽なら何もせずに `ret` する。代わりの届け先はない。
- `--trap-info` のない既定の `build` には `@tz.trap.report` がなく、トラップは文を出さない素の `llvm.trap` である（どの OS でも同じ。開発版で、MinGW の実行ファイルは何も書かずに終わった）。`run` は `src/main.rs` の `trap_info || action == Action::Run` で常に付ける。したがって消える文は、`--trap-info`・`run` の報告とランタイム C の報告に限られる。
- `src/llvm.rs` の `windows_abi` は `declare i64 @write(i32, ptr, i64)` を消し、`@write` を「`%mode = call i32 @_setmode(i32 %fd, i32 32768)`（`%mode` は使わない）、2^31−1 で切った `_write`」で定義する。`src/runtime/io.c` の `tz_io_write_now` は `_setmode` が −1 なら失敗にするが、shim は結果を見ない。`@write` は `TRAP_WRITE` のほか `src/runtime/debug.ll`（`Debug.print`）と `src/llvm_overflow.rs`（`--overflow-report`）も呼ぶ。
- ランタイム C: `src/runtime/tzroutine.c` の `tzr_fatal` は `fprintf(stderr, "Tsuzuri runtime: %s\n", ...)` の後 `abort()`、`tzr_report_deadlock` と `tsuzuri_tz_report` も `fprintf(stderr, ...)` だけで、書けたかを見ない。

### 再現（開発版 `target/debug/tsuzuri.exe`、Windows 11 26300、x64）

標準ハンドルなしの子は、次の C で起動した（調査用の試作。デバッガーとして起動するときは `OUTPUT_DEBUG_STRING_EVENT` の文を `ReadProcessMemory` で読み、例外は `EXCEPTION_BREAKPOINT` 以外を `DBG_EXCEPTION_NOT_HANDLED` で返す）。

```c
STARTUPINFOA si = {.cb = sizeof si, .dwFlags = STARTF_USESTDHANDLES}; /* hStdInput/Output/Error = NULL */
SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX);
CreateProcessA(NULL, line, NULL, NULL, TRUE, DETACHED_PROCESS | (debug ? DEBUG_ONLY_THIS_PROCESS : 0), NULL, NULL, &si, &pi);
```

| 試したこと | MinGW（同梱 `tsuzuri-clang`、UCRT の DLL） | MSVC（link.exe 14.44、静的な UCRT） |
| --- | --- | --- |
| `--trap-info` の `10 / 0`、標準ハンドルなし | `exit=0xC000001D`、デバッガーの文 0 件 | 同じ |
| `WaitGroup.done` で負（`Tsuzuri runtime: negative WaitGroup counter`）、標準ハンドルなし | `exit=0xC000001D`、文 0 件 | 同じ |
| C で `_setmode(2, _O_BINARY)` と `_write(2, ...)`、標準ハンドルなし | `_setmode` は 16384（成功）、`_write` は −1・`errno` 9 | 同じ |
| C で `_close(2)` の後に `_setmode(2, _O_BINARY)` | −1 を返す | `_setmode` の中で無効な引数のハンドラーが働き、プロセスが 0xC0000409 で終わる |

したがって、報告を失う原因は `_write` の失敗で `TRAP_WRITE` が抜けることで、`_setmode` の結果を見ないことではない。fd 2 が閉じた静的な UCRT では、`_setmode` を呼んだ時点でトラップの終了コードも文も 0xC0000409 に置き換わる。報告の経路は CRT の fd の表を通らないのがよい。

## 仕様

1. Windows の native の出力では、`@tz.trap.write` は標準エラーの OS のハンドル（`GetStdHandle(STD_ERROR_HANDLE)`）へ `WriteFile` で書く。CRT の fd 2・`_setmode` は通らない。
2. ハンドルが NULL か `INVALID_HANDLE_VALUE`、または `WriteFile` が失敗するか 0 バイトしか書かずに、文の終わりまで書けなかったら、文の全体（先頭から）を `OutputDebugStringW` に送る。UTF-8 を `MultiByteToWideChar(CP_UTF8, 0, ...)` で UTF-16 にし、255 バイト以下の塊（UTF-8 の続きのバイトの前では切らない）ごとに NUL を付けて送る。不正な列は U+FFFD になる（関数の既定）。すでに標準エラーに書けた部分は取り消さない。
3. tzroutine のランタイム C の報告（`tzr_fatal`・`tzr_report_deadlock`・`tsuzuri_tz_report`）も、Windows では 1・2 と同じ規則で書く（改行は CRT のテキストモードの `\r\n` でなく、IR の報告と同じ `\n`）。POSIX は今と同じバイト列を標準エラーの `FILE` に書く。
4. `@write` の shim は、`_setmode` が −1 なら `_write` を呼ばずに −1 を返す（`io.c` と同じ）。測った場合の結果は変わらない（「現状」の表）。
5. 変えないもの: 報告の文、トラップの終わり方と終了コード（`llvm.trap` の 0xC000001D、`tzr_fatal` の `abort()` は両方の CRT で 0xC0000409 と測った。どちらも W07）、POSIX と WASM の IR とランタイム C の出力、`Debug.print`・`--overflow-report`・`IO.*`・結果の表示。デバッガーが付いていても、標準エラーに書けたなら `OutputDebugStringW` には送らない。

## 設計

### IR（`src/llvm_traps.rs`・`src/llvm.rs`）

- `src/llvm_traps.rs` に `pub(crate) const TRAP_WRITE_WINDOWS` と `pub(crate) const DEBUGGER_WRITE` を足し、`TRAP_WRITE` を `pub(crate)` にする。`windows_abi` は IR の中の `TRAP_WRITE` と完全に同じ文字列を `TRAP_WRITE_WINDOWS` に置き換え、置き換えたときだけ `DEBUGGER_WRITE` と宣言を足す。`define internal void @tz.trap.write(` があるのに `TRAP_WRITE` が見つからない（形が変わった）ときと、2 回以上見つかったときは、黙って通さずテストで失敗させる（`windows_abi` の中の `assert!`、または単体テストが報告の 2 つの番号の付け方・`-g`・`-O0` のすべてで置き換えを確かめる）。宣言（`declare ptr @GetStdHandle(i32)`、`declare i32 @WriteFile(ptr, ptr, i32, ptr, ptr)`、`declare void @OutputDebugStringW(ptr)`、`declare i32 @MultiByteToWideChar(i32, i32, ptr, i32, ptr, i32)`）は、同じ行がまだ無いときだけ足す（W07 も同じ宣言を足すので、`intrinsics` と同じく重複させない）。x64・Arm64 の Windows では `WINAPI` は C の呼び出し規約なので、属性は要らない。
- `TRAP_WRITE_WINDOWS` の形（名前と引数は今の `@tz.trap.write` と同じ）:

```llvm
define internal void @tz.trap.write(ptr %data, i64 %length) cold nounwind {
entry:
  %written = alloca i32, align 4
  %handle = call ptr @GetStdHandle(i32 -12)
  ; NULL か INVALID_HANDLE_VALUE なら %debugger。1 回の WriteFile は 2^30 バイトまで
  ; 失敗か 0 バイトなら %debugger、終われば %exit
debugger:
  call void @tz.debugger.write(ptr %data, i64 %length)
  br label %exit
exit:
  ret void
}
```

- `DEBUGGER_WRITE` は `define internal void @tz.debugger.write(ptr %data, i64 %length) cold nounwind`。`alloca [256 x i16]` の上で、残りが 255 を超えるなら、切る位置のバイトが `10xxxxxx` の間は手前へ下がる（0 まで下がったら 255 で切る）。変換した数 `n` の位置に 0 を書いて `OutputDebugStringW` を呼ぶ。スタックは 512 バイトだけ使う（W07 のスタック枯渇の報告から呼べるように）。
- shim の `@write` は、置き換えの後にまだ `call i64 @write(` が残るときだけ定義する（`-O0` で使わない関数を残さない）。`%mode` が −1 なら −1 を返す分岐を足す。

### ランタイム C（`src/runtime/tzroutine.c`・`tzroutine-windows.h`）

- `static void tzr_report(const char *text, size_t length)` を足す。POSIX（`tzroutine.c` の `#if !defined(_WIN32)`）は `fwrite(text, 1, length, stderr); fflush(stderr);`（今の `fprintf` と同じバイト列）。Windows（`tzroutine-windows.h`）は IR と同じ規則（`WriteFile`、だめなら `tzr_debugger_write` が `MultiByteToWideChar` と `OutputDebugStringW`）。`tzroutine-jspi.h`（WASM）は変えない。
- `tzr_fatal`・`tzr_report_deadlock`・`tsuzuri_tz_report` は、文をスタックの配列（`tzr_report_deadlock` は 640 バイト。理由の列は今 400 文字で切っている）に `snprintf` で作ってから `tzr_report` に渡す。`abort()` と `fflush` の順は変えない。新しいファイルは足さない（`scripts/check-runtime-includes.sh` は変わらない）。
- 3 つの関数は `tzroutine.c` の `!TZR_JSPI` の部分にあり、JSPI（WASM）は `tzroutine-jspi.h` の自分の `tzr_fatal`（`__builtin_trap`）と空の `tsuzuri_tz_report` を使う。したがって WASM のランタイムのバイト列は変わらない。
- Windows の改行: 今のランタイムの報告は CRT のテキストモードの `stderr` を通るので、fd 2 がテキストモードなら `\n` が `\r\n` になる（`io.c` が先に fd 2 を二進モードにしていれば `\n` のまま）。`WriteFile` にすると、IR の報告と同じく常に `\n` になる。意図した変更として文書に書く（コンソールは `\n` で改行する）。

### テストの補助（W06 の `tests/windows_launch.c`）

- 使い方は W06 の仕様 4 のとおり: `windows_launch [--no-handles] [--debug-strings <path>] [--timeout-ms N] -- <exe> [args...]`。最後の行が `exit=0x%08lX` か `timeout`。`--debug-strings` はデバッガーとして起動し、`WaitForDebugEventEx` で受けた文を UTF-8 の 1 行ずつ `<path>` に追記する（`WaitForDebugEvent` だと `OutputDebugStringW` の文が ANSI に変わる。W06 の試作）。終了コードの検査は `--debug-strings` なしで行う（デバッガーの下では例外の扱いが変わる）。
- `tests/windows.mjs` が `clang` でコンパイルして使う（既存の `host.c` と同じ `nativeFlags`）。

## 前提とする他チケットのインターフェース

- なし。W06 が先に出ていれば、その標準ハンドルなしの起動の補助を使う（着手条件）。

## 他チケットへ提供するインターフェース

- IR `@tz.debugger.write(ptr %data, i64 %length)`（Windows だけ、internal・cold・nounwind、UTF-8 をデバッガーの出力へ）。W07（スタック枯渇の報告）と W08（標準エラーの無い `Debug.print`）が使う。デバッガーへ送る補助はこの 1 つだけにする。W08 が先に入ったときは、W08 がこの名前・型・仕様の `DEBUGGER_WRITE` を足しているので、このチケットはそれを使い、定義を重ねない。
- `TRAP_WRITE_WINDOWS`（Windows の `@tz.trap.write`）。W07 は `--trap-info` のないビルドでも、VEH の報告のためにこれを足す（定義が無いときだけ）。[W32](../W32-silent-write-failures.md) は、Windows の `--overflow-report` の `@tz.overflow.write` をこれへの転送に置き換え、無ければ同じく足す（W07 と共有する 1 つの補助。W32 の D6）。どちらも、このチケットの `TRAP_WRITE` → `TRAP_WRITE_WINDOWS` の置き換えの後に足す。このチケットが先に無ければ、W32 が同じ名前・同じ本文の定数を足し、このチケットはそれを使う。
- ランタイム C の `tzr_report(const char *, size_t)`。W07 のランタイムのトラップが使う。
- W04 が W06 より先に入ったときは、W06 の起動器の一部（上の 4 つの引数）。

## 実装手順

1. **変更**: なし。**内容**: `node scripts/repo-kb.mjs brief W04`、着手条件の `grep`。今の開発版で `tests/fixtures`・`examples` の `Main.tz` を `--emit llvm`（`-O0`・`-O3`、`--target x86_64-unknown-linux-gnu`・`aarch64-apple-darwin`・`wasm32`、Windows ではそのままの native）にして `target/scratch/w04/base/` に保存する。**確認**: 保存した数を控える。
2. **変更**: `src/llvm_traps.rs`・`src/llvm.rs`。**内容**: 「設計」の IR。**確認**: T0、単体テスト（下）、`cargo nextest run --locked`。手順 1 の POSIX・WASM の IR が `cmp` で一致する（S2）。
3. **変更**: `src/runtime/tzroutine.c`・`tzroutine-windows.h`。**内容**: `tzr_report`。**確認**: `node tests/tzroutine.mjs target/debug/tsuzuri`（TZ06 の失敗の文の検査を含む）、`sh scripts/check-runtime-includes.sh`、Windows で `cargo check --all-targets`。
4. **変更**: `tests/windows.mjs`（W06 が無ければ `tests/windows_launch.c` の一部も）。**内容**: 「テスト計画」の E2E。**確認**: `cargo build` の後 `node tests/windows.mjs target/debug/tsuzuri.exe` が MSVC で通る。同梱の `tsuzuri-clang` を `TSUZURI_CLANG` にしても手元で同じ段が通る。
5. **変更**: 文書。**確認**: T2、`node scripts/check-docs.mjs` に変更したページ。PR に `ci:full` を付け、`windows.yml` の x64・Arm64 が通る。

## テスト計画

- 単体（`src/llvm.rs` の `#[cfg(test)]`。`windows_abi` は `pub(crate)` なのでここに置く）:
  - `windows_trap_report_falls_back_to_the_debugger`: `--trap-info` の IR（`instrument_with` の安定した番号と密な番号の両方、`-g` あり・なし、`-O0`・`-O3`）を `windows_abi` に通すと、`@tz.trap.write` が `call ptr @GetStdHandle(i32 -12)` を持ち `call i64 @write(` を持たず、`define internal void @tz.debugger.write(` と 4 つの `declare` がそれぞれ 1 回だけあり、2 回作って同じ文字列になる。
  - `windows_write_shim_reports_a_failed_setmode`: `Debug.print` の IR では shim が残り、`_setmode` の −1 で −1 を返す分岐を持つ。`--trap-info` だけで `Debug.print` のない IR には shim が無い。
  - POSIX の IR（`windows_abi` に通さない）に `GetStdHandle` も `tz.debugger.write` も無い。
- E2E（`tests/windows.mjs`、`-O0`・`-O3`）: (1) `--trap-info` の `10 / 0` を `--no-handles --debug-strings <path>` で動かし、`<path>` の行がちょうど 1 つで `trap: integer division by zero at ` で始まる。`--no-handles` だけで最後の行が `exit=0xC000001D`。(2) `WaitGroup.done` の負の値（`tests/tzroutine.mjs` の TZ06 の例）で `<path>` に `Tsuzuri runtime: negative WaitGroup counter` が 1 行。(3) `--no-handles` なし（ハンドルを継承）の `--debug-strings` では `<path>` が空で、標準エラーの文は今と同じ（既存の段 `trap` の検査を残す）。(4) 日本語のディレクトリ名のプロジェクトで、`<path>` の文のパスがソースのパスと同じ文字になる（変換の検査）。
- 回帰: `tests/windows.rs` の `emits_windows_exports_without_changing_default_or_wasm`（`Debug.print` の shim）がそのまま通る。期待値はコンパイラの出力から写さず、文はソースの位置から手で書く（GUIDE-LITE §2 規則 10）。

## ドキュメント

- `_tsuzuri/language-reference/compiler/platforms.md`: Windows で標準エラーに書けないとき、トラップの報告とランタイムの報告がデバッガーの出力（`OutputDebugStringW`。Visual Studio の出力ウィンドウ、WinDbg、DebugView）に届くこと。`--trap-info` のない既定のビルドは、どの OS でもトラップの文を出さないこと。
- `_tsuzuri/language-reference/compiler/native-interop.md`: object を埋め込んだホストでも、`--trap-info` の報告は標準エラーの OS のハンドルへ直接書き、CRT の fd 2 のリダイレクト（`_dup2`）には従わないこと。
- `docs/architecture.md` の「Windows MSVC」の節の、`_write` の段落と「トラップの位置はプログラムが先に標準エラーへ書きます」の文。
- `docs/language.md` と `test.md` の終了コードの記述は変えない（終わり方を変えないため）。

## 受け入れ条件

- [x] 「仕様」の 1〜4 のとおりに動き、E2E の (1)〜(4) が MSVC で通る。同梱の MinGW でも手元で通した。
- [x] POSIX と WASM の `--emit llvm` と、POSIX のランタイム C の出力のバイト列が変わらない。Windows の IR が変わるのは `--trap-info` の報告と `@write` の shim だけ。
- [x] 宣言が重複せず、Windows の IR が決定的。
- [x] 文書を直し、`check-docs` が通る。PR に `ci:full` を付け、`windows.yml` が通る。

## 落とし穴

- `GetStdHandle` は、ハンドルが無いとき NULL を返すことも `INVALID_HANDLE_VALUE`（−1）を返すこともある。両方を見る。
- `WriteFile` の長さは `DWORD`。塊を 2^30 バイトにすれば、符号の扱いで迷わない。
- `OutputDebugStringW` は内部で例外（`DBG_PRINTEXCEPTION_WIDE_C`）を投げる。W07 の VEH はこの例外を対象にしない（コードで絞る）。デバッガーの補助は、この例外を `DBG_EXCEPTION_NOT_HANDLED` で返してよい（文は `OUTPUT_DEBUG_STRING_EVENT` で届く）。
- 文を 255 バイトで切るとき、続きのバイトの途中で切ると U+FFFD が 2 つ入る。続きのバイトの前で切る。
- コンソールへの `WriteFile` は今の二進モードの `_write` と同じくバイト列をそのまま書く。コンソールのコードページの扱いは変わらない。
- 終了コードの検査をデバッガーの下でしない（デバッガーが最初の機会の例外を受けるため）。

## 対象外

- トラップの終わり方（fail-fast、VEH、`uwtable`）と、`tzr_fatal` の `abort()` の 0xC0000409 を 0xC000001D にそろえるかどうか（[W07](../W07-windows-trap-fail-fast.md)）。
- `Debug.print`・`Debug.trace`・`IO.*` の、標準ストリームが無いとき（閉じた fd で静的な UCRT が `_setmode` の中で終わる件を含む）の意味（[W08](../W08-io-without-std-streams.md)）。結果の表示と `--overflow-report` の、書けないときの扱い（今は黙って捨てる）は、W08 の決定に従って [W32](../W32-silent-write-failures.md) が直す。shim の `_setmode` の変更は仕様 4 だけ。
- 自前のダイアログ（`MessageBoxW`）。GPU のランタイムの診断の文（`gpu.c`・`gpu-vulkan.c`。致命的な報告ではなく状態を返す）。
- POSIX で標準エラーが閉じているときの届け先（`syslog` など）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | 書けないときの届け先は `OutputDebugStringW` だけ | どの Windows の開発環境でも読め、権限も設定も要らない。イベントログはソースの登録が要り、ファイルは場所の取り決めが要る | 確定・実装済み |
| D2 | Windows の報告は CRT を通らず `GetStdHandle`・`WriteFile` で書く | `_setmode` の結果を見ても、静的な UCRT は閉じた fd で `_setmode` の中で終わる（測定）。CRT のロックと無効な引数のハンドラーを避け、W07 の VEH からも呼べる | 確定・実装済み |
| D3 | 標準エラーに書けたときはデバッガーへ送らない | 報告を二重にしない。コンソールの実行の出力を変えない | 確定・実装済み |
| D4 | 送るのは文の全体（先頭から） | 途中まで書けた文の残りだけでは位置が読めない | 確定・実装済み |
| D5 | UTF-16 への変換は実行時に `MultiByteToWideChar` で行う（コンパイル時に UTF-16 の複製を持たない） | 地点ごとの文の定数とその IR を変えずに済み、データが倍にならない。報告は一度しか走らない | 確定・実装済み |
| D6 | shim の `_setmode` は `io.c` と同じく −1 を失敗にする | 2 つの書き込みの経路の規則をそろえる。測った場合の結果は変わらない | 確定・実装済み |

## 実装と検証（2026-11-23）

- `src/llvm_traps.rs` に `TRAP_WRITE_WINDOWS` と `DEBUGGER_WRITE` を追加し、`src/llvm.rs` の `windows_abi` で `@tz.trap.write` を OS ハンドル書き込み + `OutputDebugStringW` フォールバックへ差し替え、`@write` shim は `call i64 @write(` が残るときだけ出力して `_setmode == -1` で `-1` を返すようにした。
- `src/runtime/tzroutine-windows.h` と `src/runtime/tzroutine.c` に `tzr_report`（Windows では `WriteFile` + `tzr_debugger_write`、POSIX では `fwrite` + `fflush`）を追加し、`tzr_fatal`・`tzr_report_deadlock`・`tsuzuri_tz_report` を通した。
- `tests/windows_launch.c`、`src/llvm.rs` の単体テスト、`tests/windows.mjs` の E2E 検査（ハンドルなしトラップ・`WaitGroup` 負値・ハンドル継承時・日本語パス）、`docs/architecture.md`、`_tsuzuri/language-reference/compiler/platforms.md`、`_tsuzuri/language-reference/compiler/native-interop.md` を更新した。

