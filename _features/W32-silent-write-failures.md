# W32: 書き込みの失敗を黙って捨てる経路を塞ぐ（トップレベルの式の表示、`--overflow-report`）

| 項目 | 内容 |
| --- | --- |
| ID | W32 |
| 優先度 | P2（修正。標準出力が無い・閉じている・書けないときに、トップレベルの式の値が消えても終了コードは 0 になる。`--overflow-report` の報告も、標準エラーに書けなければどこにも残らない。GUI サブシステムの実行ファイル（W09）を Explorer から起動すると、必ずこの状態になる） |
| 規模 | S（`console.ll` の書き直し、`windows_abi` の 2 か所、`tests/io.mjs`・`tests/windows.mjs`・`tests/windows.rs`・単体テスト、文書） |
| 依存 | [W08](W08-io-without-std-streams.md) |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-22（D-63。W01〜W30 の起票中に見つけ、利用者の依頼「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」に当たるので起票。HEAD `fa40eede` で確認） |
| 承認 | 不要。W08 の決定（D1: ストリームが無いときも失敗を読み飛ばさない）に従う。表示の失敗を終了コード 1 にするのは、`src/llvm.rs` の `console_main` がもともと持つ失敗の経路（`%failed` → `ret i32 1`）を働かせる修正である。`--overflow-report` は Windows で報告の届け先を足すだけで、値・標準出力・トラップ・終了コードは変えない（`compiler/usage.md` の「整数の折り返しの報告」の契約）。台帳は [GUIDE D-63](GUIDE.md#d-63-tsuzuri-だけで-win32-のアプリを書くe12-d13-の狭い改訂w01w30) |
| 手本にする既存実装 | `src/runtime/debug.ll` の `@tz.debug.write`（`@write` を繰り返し、進まなければ失敗にする。改行は別の `@write` で書く）、`src/llvm.rs` の `windows_abi`（Windows だけの IR の書き換え）、[W04](_completed/W04-windows-trap-report-lost.md) の `TRAP_WRITE_WINDOWS`・`DEBUGGER_WRITE`（標準エラーの OS のハンドルへ書き、書けなければデバッガーへ）、`tests/io.mjs` の閉じた標準出力の検査（読み取り専用の fd を標準出力に渡す） |
| 主な影響ファイル | `src/runtime/console.ll`、`src/llvm.rs`（`windows_abi`・単体テスト）、`src/llvm_overflow.rs`（`Report::runtime` の書き手を定数にする）、`tests/windows.rs`、`tests/io.mjs`、`tests/windows.mjs`（GUI の段）、`docs/architecture.md`、`docs/language.md`、`_tsuzuri/language-reference/built-in-types-and-modules/io.md`、`_tsuzuri/language-reference/compiler/usage.md`、`_tsuzuri/language-reference/compiler/platforms.md`、`_tsuzuri/language-reference/compiler/native-interop.md` |

## 実装者向け早見

- 範囲: native の実行ファイルで、書き込みの失敗を誰にも知らせずに捨てる 2 つの経路。
  - (a) トップレベルの式の値の表示。`src/runtime/console.ll` の `@tz.console.write` で、`src/llvm.rs` の `console_main` が呼ぶ。
  - (b) `--overflow-report` の報告の書き込み。`src/llvm_overflow.rs` の `Report::runtime` が作る `@tz.overflow.write` である。
- 直し方:
  - (a) 表示を、`putchar`（stdio のバッファ）ではなく `write(1, ...)` で直接書く。値と改行を書き終える前に書き込みが失敗したら、終了コードを 1 にする（`console_main` の今の分岐）。標準出力が無いときも同じ。
  - (b) Windows では、W04 の報告の書き手（Windows 版の `@tz.trap.write`）へ転送する。これは標準エラーの OS のハンドルへ `WriteFile` で書き、書けなければ文の全体を `OutputDebugStringW` に送る。POSIX と WASM は変えない。
- 最初に読む: 「現状」→ `src/runtime/console.ll`（25 行）→ `src/llvm.rs` の `console_main` の末尾と `windows_abi` → `src/llvm_overflow.rs` の `Report::runtime` → W04 の「設計」→ W08 の D1・D4 と「対象外」。
- 最初に書くテスト: `tests/io.mjs` の検査。読み取り専用の fd を標準出力にしてトップレベルの `42` を動かすと、終了コードが 1 になることを確かめる。
- 触ってはいけない:
  - 次の IR: `def main`、IO の入口、結果が `unit` のトップレベルのコード、ライブラリ、WASM。POSIX と WASM の `--overflow-report` の IR も。
  - 表示と報告の文。
  - `--overflow-report` の有無で、値・標準出力・トラップ・終了コードが同じであること。
- 完了の見分け方:
  - `printf '42\n' > top/Main.tz` を build した `top.exe` を、Git Bash で 2 通りに動かす。`./top.exe 1<ro.txt`（読み取り専用のファイルを標準出力にする）と `./top.exe >&-` のどちらも、終了コードが 1 になる（今は 0）。
  - W06 の起動器で `windows_launch --no-handles --debug-strings ods.txt -- ovf.exe` とする（`ovf.exe` は「テスト計画」の `def main` の fixture）。`ods.txt` に `warning: integer overflow wrapped at ` で始まる行が 1 行入る（今は空）。`--debug-strings` なしの終了コードは 0 のまま。

## 目的

W08 は、標準ストリームが無いときの `IO` と `Debug` の意味を決める。W04 は、トラップとランタイムの報告の届け先を決める。どちらも残した書き込みの経路が 2 つある。トップレベルの式の値の表示と、`--overflow-report` の報告である。

- 表示: 失敗したら終了コードを 1 にするコードがあるのに、stdio のバッファに書くだけで flush しないので、失敗が見えない。値が消えても終了コードは 0 になる。同じ状況で `IO.writeln` はトラップするので、意味が食い違う。
- 報告: 書けなければ黙ってやめる。

この 2 つを、GUIDE §1 規則 2（読み飛ばしをしない）と W08 の決定に沿って直す。

## 着手条件と停止条件

### 着手条件

- W08 が `main` に入っていること（依存）。W08 の D1 の承認の結果を確かめ、仕様 3 の表のどの行に当たるかを決める。
- W04 が入っていれば、その `TRAP_WRITE_WINDOWS`・`DEBUGGER_WRITE` と `windows_abi` の置き換えを使う。入っていなければ、W04 の仕様 1・2 と「設計」の `TRAP_WRITE_WINDOWS` を、同じ名前・同じ本文で足す。足すのは定数だけで、`--trap-info` の報告の置き換えは W04 に残す。`DEBUGGER_WRITE` は W08 が足している。W08 も `DEBUGGER_WRITE` について同じ方法を取る。
- W06 が入っていれば、その起動器（`tests/windows_launch.c` の `--no-handles`・`--debug-strings`）と `launchWithoutHandles` を使う。入っていなければ、W06 の仕様 4 と同じ名前・同じ形で、要る部分だけを足す（W04・W08 と同じ）。
- `gh run list --branch main --workflow ci.yml --limit 1` が緑。

### 停止条件

- **S1** W08 の D1 が、仕様 3 の表にない形に決まった。表示の意味を即興で決めずに報告する。
- **S2** IR が「触ってはいけない」の範囲で 1 byte でも変わる。または、トップレベルの表示のプログラムの IR が、「予定した差」の外で変わる。予定した差は次のとおり。
  - すべての target: `@tz.console.write`・`@tz.console.put` の定義、`declare i64 @write` の追加、`declare i32 @putchar` の削除。
  - Windows の native だけ（`windows_abi` の後）: `define internal i64 @write(` の shim と `declare i32 @_write(i32, ptr, i32)` の追加、`%binary_mode = call i32 @_setmode(i32 1, i32 32768)` の削除（D5）。`--overflow-report` のプログラムでは、さらに `@tz.overflow.write` の転送と、Windows の報告の書き手（`@tz.trap.write`・`@tz.debugger.write` と 4 つの `declare`）。
- **S3** POSIX の native で、閉じた標準出力か読み取り専用の fd に `write(1, ...)` しても、0 以下が返らない。または `tests/io.mjs` の今の期待（読み取り専用の fd の `WriteFailed`、閉じた標準出力の `writeln` のトラップ）が変わる。
- **S4** W04（または W07）が入れた `@tz.trap.write`・`@tz.debugger.write` の名前・型・意味が、W04 の「他チケットへ提供するインターフェース」と違う。
- **S5** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

### コード

- **表示**: `src/runtime/console.ll` の `@tz.console.write(ptr %data, i64 %length)` は、1 byte ずつ `putchar` を呼ぶ。`putchar` が負なら −1 を返し、最後に `putchar(10)` の結果を返す。`fflush` は呼ばない。
- **入口**: `src/llvm.rs` の `console_main` は、トップレベルの結果が `unit` でないときだけ `console.ll` を入れる。値を `@tz.console.write` で書いた後、`%failed = icmp slt i32 %printed, 0` と `zext` で終了コードを作る（失敗なら 1）。次の 3 つは表示しない: `def main`（`console_def_main`）、IO の入口（`tsuzuri_main` を呼ぶ `@main`）、`unit` の結果（`ret i32 0`）。
- **入口の種類**: `Entry::Console` は `--emit exe` のときだけ使う（`src/driver.rs` の `llvm::EmitOptions` の `entry`）。`tsuzuri run` も exe を作る。したがって `console.ll` を使うのは native の exe だけである。WASM の JS のホストなど、ほかの経路が値を表示するかは確かめておらず（未確認）、このチケットの対象外とする。
- **Windows**: `src/llvm.rs` の `windows_abi` は、`define internal i32 @tz.console.write(` の `entry:` の直後に `%binary_mode = call i32 @_setmode(i32 1, i32 32768)` を入れ、結果は使わない。`declare i32 @_setmode(i32, i32)` も足す。`tests/windows.rs` の `emits_windows_exports_without_changing_default_or_wasm` が、この行を確かめている。
- **バッファ**: `putchar` は CRT の stdout のバッファに書くだけで、普通は失敗を返さない。実際の書き込みはプロセスの終わりに CRT が flush するときで、その失敗は捨てられる。したがって、`console_main` の失敗の分岐は事実上働かない。
- **報告**: `Report::runtime(false)` は 2 つの関数を作る。
  - `@tz.overflow.report`: 最初の 100 地点を書き、101 地点目で止めの行を書く。
  - `@tz.overflow.write`: `@write(2, ...)` を繰り返す。`%progress = icmp sgt i64 %written, 0` が偽なら、何もせずに `exit` へ抜ける。
  - `src/llvm.rs` の `emit_program_in` の末尾は、報告があれば `declare i64 @write(i32, ptr, i64)` を足す。
  - Windows では `windows_abi` が `@write` を `_setmode`・`_write` の shim に置き換える。
  - WASM は書かず、`tsuzuri_overflow_count`・`tsuzuri_overflow_hit` を export する。
- **報告の契約**: `compiler/usage.md` の「整数の折り返しの報告」は、「値・標準出力・トラップ・終了コードは、付けないときとビット単位で同じです」と定める。
- **W08・W04 の扱い**:
  - W08 の D4 は「トップレベルの式の表示は変えない」と決め、表示の失敗の見落としを「ストリームの有無と別の既存の欠陥」とした。
  - W08 の「対象外」は、表示（`fflush` も失敗の検査もしない）と `--overflow-report`（書けなければ黙ってやめる）を別のチケットにした。
  - W04 の「対象外」も、この 2 つを W08 の範囲に挙げている。このチケットがその受け皿である。
- **予約した記号**: `src/abi.rs` の `RESERVED_HOST_SYMBOLS` は、生成コードが宣言する C の名前（`putchar`・`write`・`_setmode`・`_write`・`malloc` など）を持つ。`extern` のリンク名にこれらを書くと `E1008`（`reserved_extern_symbol`）になる。`reserved_symbols_cover_the_runtime_names` は、`src/runtime/*.ll` が宣言する名前がすべてこの表にあることを確かめる。`fflush` は表に無い。

### このマシンで確かめた挙動

ツールチェーンは 2 つ使った。1 つは開発版 `target/debug/tsuzuri.exe` と同梱の 0.1.6（MinGW）。もう 1 つは clang 22 と MSVC 14.44 の CRT（静的な UCRT。既定のリンカーは lld-link）。どちらも結果は同じだった。Git Bash から動かし、確認は `target/scratch/w-w31w32/` で行い、確認後に消した。

| プログラム | 普通に起動 | `>&-` / `2>&-` | `1<ro.txt` / `2<ro.txt`（読み取り専用のファイル） |
| --- | --- | --- | --- |
| トップレベルの `42` | `42` を表示して 0 | `>&-`: 何も出ずに 0 | `1<ro.txt`: 何も出ずに 0 |
| `def main` で `do! IO.writeln "hello"` の後に `7` | `hello` と 7 | `>&-`: トラップ（Illegal instruction） | `1<ro.txt`: トラップ |
| `--overflow-report` で `x * 3i32u`（`x = 4000000000i32u`）を表示 | `warning: integer overflow wrapped at ovf\Main.tz:1:36 (i32u mul)` と値、0 | `2>&-`: 値だけで報告なし、0 | `2<ro.txt`: 値だけで報告なし、0 |

C で探った結果（両方の CRT で同じ）:

- `_setmode(1, _O_BINARY)` の後に `putchar('4')`・`putchar('\n')`・`fflush(NULL)` を呼んだ。
  - `putchar` は、`>&-` と `1<ro.txt` のどちらでも 52 と 10 を返した（成功）。
  - `fflush(NULL)` は、どちらでも −1 を返した。
  - 失敗は flush でしか見えない。
- `_setmode(1, _O_BINARY)` の後に `_write(1, "42\n", 3)` を呼んだ。
  - `_setmode` は 16384 を返した（成功）。
  - `_write` は、どちらでも −1 を返し、`errno` は 9（`EBADF`）だった。
  - 直接の書き込みなら、その場で失敗が分かる。
- W08 の「現状」の表: W06 の起動器の試作で標準ハンドルなしで起動すると、トップレベルの `42` は両方の CRT で終了コード 0 になり、表示は消えた。
- POSIX は、このマシンでは測っていない（未確認）。閉じた fd 1 と読み取り専用の fd への `write` は `EBADF` になるはずで、`tests/io.mjs` の新しい検査で確かめる。

## 変えてはいけない意味

- 書けたときの表示のバイト列（値の UTF-8 と LF）と、終了コード 0。
- `string` の孤立サロゲートの表示は、書く前にトラップする（`@tz.utf8string.from_string`）。
- `def main` の戻り値、IO の入口の `IO<i32>` の終了コード、`unit` の結果の 0。
- `--overflow-report` の契約: どの target でも、値・標準出力・トラップ・終了コードを変えない。
- W08 が決めた `IO.*`・`Debug.*` の意味と、W04 が決めたトラップの報告の届け先。
- POSIX の SIGPIPE: 読み手の無いパイプへの表示は、今もプロセスの終わりの flush で SIGPIPE を受ける。修正後は表示の `write` で受ける。どちらもシグナルで終わる。

## 仕様

### 1. トップレベルの式の表示

- 値の UTF-8 のバイト列と LF を、`write(1, ...)` で順に書く（Windows では `_setmode` と `_write` の shim）。部分的な書き込みは続けて書く。
- 書き終える前に書き込みが 0 以下を返したら、そこで書くのをやめ、プロセスの終了コードを 1 にする。対象は、標準出力が無い（Windows の GUI のプロセス、POSIX の閉じた fd 1）、読み取り専用である、ディスクが一杯である、SIGPIPE を無視したときの `EPIPE`、などである。
  - トラップしない。標準エラーにもデバッガーにも何も書かない。
  - 書けた部分は取り消さない。
- ストリームが無いときと、書き込みに失敗したときを区別しない。どちらも 1 である（W08 の `Unavailable` は `try_` の関数のためのもので、表示には持ち込まない）。
- 表示しない入口（`def main`、IO の入口、`unit` の結果）は変わらない。

### 2. `--overflow-report` の報告

- **Windows の native**: 報告の 1 行（と止めの行）を、標準エラーの OS のハンドルへ `WriteFile` で書く。ハンドルが無い、または文の終わりまで書けなければ、文の全体を `OutputDebugStringW` に送る。これは W04 の仕様 1・2 と同じ規則で、同じ書き手を使う。トラップせず、値・標準出力・終了コードは変わらない。
- **POSIX の native**: 変えない。書けない報告の行は捨てる。報告の契約がトラップと終了コードの変更を禁じ、OS に標準のデバッグ出力の経路が無いためである。このことを文書に書く。
- **WASM**: 変えない（export で読む）。

### 3. W08 の D1 の決定との対応

| W08 の D1 | 表示: ストリームが無い | 表示: 書き込みの失敗 | 報告（Windows） |
| --- | --- | --- | --- |
| 既定案（非 `try_` はトラップ、`try_` は `Unavailable`、`Debug.print` は Windows で `OutputDebugStringW`） | 終了コード 1 | 終了コード 1 | 書けなければ `OutputDebugStringW` |
| 案 A（全 OS で、ストリームが無ければ捨てて `Ok`） | 捨てて 0（今と同じ結果） | 終了コード 1 | ストリームが無ければ捨てる（今と同じ）、書き込みの失敗は `OutputDebugStringW` |
| 案 B（GUI のときだけ `OutputDebugStringW` に送って `Ok`） | GUI サブシステムでは `OutputDebugStringW` に送って 0、ほかは 1 | 終了コード 1 | 書けなければ `OutputDebugStringW` |

既定案以外のときに要る判定（ストリームが無いかどうか）は、W08 の仕様 1 の判定（Windows は `_get_osfhandle` の `-2`、または `-1` と `EBADF`。POSIX は `fcntl(F_GETFD)` の `EBADF`）を、失敗の後にだけ使う。この表のどの行にも当たらなければ S1。

## 設計

### 表示（`src/runtime/console.ll`）

```llvm
declare i64 @write(i32, ptr, i64)

define internal i32 @tz.console.write(ptr %data, i64 %length) {
entry:
  %newline = alloca i8, align 1
  store i8 10, ptr %newline, align 1
  %body = call i32 @tz.console.put(ptr %data, i64 %length)
  %failed = icmp slt i32 %body, 0
  br i1 %failed, label %error, label %end
end:
  %last = call i32 @tz.console.put(ptr %newline, i64 1)
  ret i32 %last
error:
  ret i32 -1
}

; @write(1, ...) を offset が length に届くまで繰り返す。0 以下が返れば -1、書き終えれば 0。
define internal i32 @tz.console.put(ptr %data, i64 %length) { ... }
```

- `@main` の側（`console_main`）は変えない。`%printed` が負なら終了コード 1 である。
- `declare i64 @write` は、`Debug.print`（`debug.ll`）・`--trap-info`・`--overflow-report` も宣言する。重複は `src/llvm.rs` の `unique_declarations` が最初の 1 つに正規化する（今の仕組み）。
- `putchar` は宣言しなくなる。`RESERVED_HOST_SYMBOLS` の `putchar` は残す（消すと受け入れる `extern` が変わる。要るなら別に決める）。`src/driver.rs` の `--freestanding` の印（`declare i32 @putchar(`）も残してよい。表示のある入口は `--freestanding` にならない。
- **Windows**: `windows_abi` の表示のための差し込みを消す。消すのは、`has_console`、`console` の印、`%binary_mode = call i32 @_setmode(i32 1, i32 32768)` である。`console.ll` が `@write` を宣言するので、`has_write` が真になり、`@write` の shim（`_setmode(fd, _O_BINARY)` と `_write`）が定義される。W04 の仕様 4 の後は、shim は `_setmode` の −1 で −1 を返す。テキストモードの変換をしないのは今と同じ。
- `tests/windows.rs` の `emits_windows_exports_without_changing_default_or_wasm` の最後の検査を、次の 3 つに直す。Console の入口の IR に `define internal i64 @write(` と `declare i32 @_write(i32, ptr, i32)` があること。`%binary_mode` が無いこと。

#### 採らない案（表示）

- **`putchar` のまま、改行の後に `fflush(NULL)` を呼ぶ**: いちばん小さい差に見えるが、`console.ll` が `fflush` を宣言することになる。`reserved_symbols_cover_the_runtime_names` により、`fflush` を `RESERVED_HOST_SYMBOLS` に足すことになる。そうすると、今受け入れている `extern "fflush" def flush_all :: i64 -> i32` のような宣言が、表示を使わないプログラムでも `E1008` になる。`docs/stability.md` の「受理していたプログラムを拒否する変更は、新しい edition でだけ」に反する。
- **`fflush(stdout)`**: `stdout` の実体が CRT ごとに違う（glibc の `stdout`、macOS の `__stdoutp`、UCRT の `__acrt_iob_func(1)`）。IR に target ごとの表を持つことになり、壊れやすい。
- **トラップする（W08 の非 `try_` と同じ）**: 表示はプログラムの本体が終わった後に、生成した `@main` が行う。終了コードは、そのときに失敗を知らせる自然な経路で、`console_main` はもともと 1 を返す分岐を持つ（coreutils の `close_stdout` と同じ慣習）。トラップの終了コード（0xC000001D・SIGILL）は、走り終えたプログラムを異常終了に見せる。
- **Windows で `OutputDebugStringW` に送って 0**: 表示はプログラムの出力で、診断ではない。W08 の D1 は、デバッガーへ送るものを診断（`Debug.print`）に限っている。
- **ストリームが無ければ捨てて 0**: W08 が却下した案 A と同じで、書いたはずの出力が黙って消える。
- **失敗したときに標準エラーへ理由を書く**: 標準エラーも無いことが多く、IR が増える。終了コードと文書で足りる（「対象外」）。

### `--overflow-report`（Windows。`src/llvm_overflow.rs`・`src/llvm.rs`）

- `Report::runtime` が書く `@tz.overflow.write` の定義を、`pub(crate) const OVERFLOW_WRITE: &str` に出す。W04 が `TRAP_WRITE` を `pub(crate)` にするのと同じ形である。今の `write!` の書式の中の同じバイト列を、定数の `push_str` に置き換えるだけにする。POSIX の IR は 1 byte も変えない（手順 1 との比較）。
- `windows_abi` は、W04 の `TRAP_WRITE` → `TRAP_WRITE_WINDOWS` の置き換えの**後**に、IR の中の `OVERFLOW_WRITE` と完全に同じ文字列を次の転送に置き換える。

```llvm
define internal void @tz.overflow.write(ptr %data, i64 %length) cold nounwind {
entry:
  call void @tz.trap.write(ptr %data, i64 %length)
  ret void
}
```

- `@tz.trap.write` の定義が無いとき（`--trap-info` なし）は、次を足す。`TRAP_WRITE_WINDOWS`、`DEBUGGER_WRITE`、4 つの `declare`（`GetStdHandle`・`WriteFile`・`OutputDebugStringW`・`MultiByteToWideChar`）。どれも、まだ無いときだけ足す。
  - この「無いときだけ足す」は、W07（VEH の報告）も要る段である。補助は 1 つ（例: `fn ensure_windows_report_writer(output: &mut String)`）にまとめ、W07 と W32 のうち先に入る方が作る。
  - 順序を守る。W04 の `windows_abi` は、`define internal void @tz.trap.write(` があるのに `TRAP_WRITE` が見つからないと、テストで失敗させる。先に足すと、その検査に当たる。
- `define internal void @tz.overflow.write(` があるのに `OVERFLOW_WRITE` が見つからないとき、または 2 回以上見つかったときは、黙って通さずテストで失敗させる（W04 と同じ扱い）。
- 置き換えの後に `call i64 @write(` が残らなければ、W04 の設計どおり `@write` の shim は定義しない。
- W04 の `TRAP_WRITE_WINDOWS` は、報告を CRT の fd 2 でなく OS のハンドルへ書く。そのため、閉じた fd 2 の `_setmode` で静的な UCRT がプロセスを 0xC0000409 で終わらせる経路も通らない。この経路は W04 の「現状」が C の試験プログラムで測ったもので、`--overflow-report` の報告そのものでは測っていない（未確認）。今の報告は `@write` の shim を通るので、ホストが fd 2 を閉じた object では同じことが起きると推定する。W04 の書き手を使えば、その場合も報告の契約（終了コードを変えない）を守れる。

#### 採らない案（報告）

- **今の `@write` の繰り返しのまま、失敗したら `@tz.debugger.write` に送る**: CRT の fd 2 の `_setmode` を通るので、上の 0xC0000409 の経路が残る。W08 は `Debug.print` についてスレッド局所の何もしない処理で挟むが、報告には W04 の書き手がそのまま使える。
- **`Report::runtime` が出力のときに target を見て選ぶ**: W08 の D5 と同じ理由で採らない（`tests/windows.rs` のように後処理だけを Windows にする経路とずれる）。
- **POSIX でも何かに送る（`syslog` など）**: W04 の「対象外」と同じ。設定と権限が要り、報告の契約にも当たらない。

## 前提とする他チケットのインターフェース

- [W08](W08-io-without-std-streams.md):
  - D1 の決定（仕様 3）。
  - 仕様 1 の「ストリームが無い」の判定。既定案以外の行でだけ使う。
  - `DEBUGGER_WRITE` が `windows_abi` で使えること。
  - `io.md` の「終了コードとトップレベルの表示」に W08 が書く「表示はストリームが無いと消える（D4）」の文。このチケットが置き換える。
- [W04](_completed/W04-windows-trap-report-lost.md):
  - `TRAP_WRITE_WINDOWS`（Windows の `@tz.trap.write(ptr, i64)`）と `DEBUGGER_WRITE`（`@tz.debugger.write(ptr, i64)`）。
  - `windows_abi` の置き換えの順序。
  - 仕様 4（shim の `_setmode` の −1）。
  - W04 が未着手なら、着手条件のとおり定数だけを同じ形で足す。
- [W06](W06-windows-gui-test-harness.md): 仕様 4 の `windows_launch.exe --no-handles`・`--debug-strings <path>`・`launchWithoutHandles`。
- [W07](W07-windows-trap-fail-fast.md): 先に入っていれば、その「`@tz.trap.write` が無いときだけ足す」補助を使う。

## 他チケットへ提供するインターフェース

- 意味: トップレベルの表示を書けなければ終了コード 1。Windows の `--overflow-report` は、書けなければデバッガーの出力に送る。
- [W09](W09-windows-gui-subsystem.md) の文書が引く文: 「GUI サブシステムの実行ファイルをハンドルなしで起動すると、トップレベルの式の表示は書けず終了コード 1 になる。GUI のアプリは `def main` を使う」。
- `OVERFLOW_WRITE`（`src/llvm_overflow.rs`）と、Windows の報告の書き手を足す補助（W07 と共有）。

## 実装手順

1. **変更**: なし。
   **内容**: `node scripts/repo-kb.mjs brief W32` と着手条件を確かめる。HEAD の開発版で、`tests/fixtures`・`examples` のすべての `Main.tz` を `--emit llvm` にする。条件は `-O0`・`-O3`、target は次の 4 つ: `--target x86_64-unknown-linux-gnu`・`aarch64-apple-darwin`・`wasm32`・Windows ではそのままの native。`--overflow-report` あり・なしの両方を作り、`target/scratch/w32/base/` に保存する。
   **確認**: 保存した数を控える。
2. **変更**: `tests/io.mjs`（テスト計画の POSIX と読み取り専用の fd の検査）を先に書く。
   **内容**: 今は終了コード 0 で失敗することを確かめる。
   **確認**: `node tests/io.mjs target/debug/tsuzuri` が新しい検査で失敗する。
3. **変更**: `src/runtime/console.ll`、`src/llvm.rs`（`windows_abi` の表示の差し込みを消す）、`tests/windows.rs`。
   **内容**: 「設計」の表示。
   **確認**: T0、`cargo test --locked --test windows --test chars --test display_parse`（`running N tests` の N を確かめる）、手順 2 の検査が通る。手順 1 の IR との差が、S2 の「予定した差」だけであることを確かめる（Windows の native では shim・`_write` の宣言・`%binary_mode` の行も差に入る）。差の行を分類して数える小さなスクリプトを `target/scratch/w32/` に置く。
4. **変更**: `src/llvm_overflow.rs`（`OVERFLOW_WRITE`）、`src/llvm.rs`（`windows_abi` の転送、報告の書き手を足す補助、単体テスト）。
   **内容**: 「設計」の報告。W04 が未着手なら `TRAP_WRITE_WINDOWS` の定数も足す。
   **確認**: 単体テスト、`cargo test --locked --test overflow_report`、`cargo nextest run --locked`。手順 1 の POSIX・WASM の `--overflow-report` の IR が `cmp` で一致する。
5. **変更**: `tests/windows.mjs`（GUI の段）。W06 が無ければ、`tests/windows_launch.c` の一部も足す。
   **内容**: テスト計画の Windows の検査。
   **確認**: `node tests/windows.mjs target/debug/tsuzuri.exe --stage gui` を、MSVC と同梱の `tsuzuri-clang` の両方で回す。
6. **変更**: 文書（下）。
   **確認**: T2、変更したページを渡した `node scripts/check-docs.mjs`、`sh scripts/check-runtime-includes.sh`（`src/runtime/` を変えるため）。PR に `ci:full` を付け、`windows.yml`・`vscode.yml` の Windows の job を通す。

## テスト計画

### `tests/io.mjs`（native の `-O0`・`-O3`）

期待値は手で書く（GUIDE-LITE §2 規則 10）。

| fixture（トップレベル） | 起動 | 期待 |
| --- | --- | --- |
| `42` | 普通に | 標準出力 `42\n`、終了コード 0 |
| `42` | 読み取り専用の fd を標準出力に（既存の `openSync(source, "r")` の形） | 終了コード 1（シグナルでない）、標準出力も標準エラーも空 |
| `"hello"`、`true`、`'a'` | 同上 | 終了コード 1 |
| `42` | POSIX だけ: `sh -c 'exec "$0" >&-' <exe>` | 終了コード 1 |
| `let _ = 1`（結果が `unit`） | 読み取り専用の fd | 終了コード 0（表示しない） |
| `IO.pure 3` | 読み取り専用の fd | 終了コード 3（変わらない） |
| `def main :: unit -> i32 = \() -> 7` | 読み取り専用の fd | 終了コード 7（変わらない） |

- 既存の検査（読み取り専用の fd の `WriteFailed`、閉じた標準出力での `writeln` のトラップ）は、期待を変えずに通ること。

### Windows（`tests/windows.mjs` の GUI の段。`-O0`・`-O3`、MSVC と同梱）

| fixture | 起動 | 期待 |
| --- | --- | --- |
| トップレベルの `42` | `launchWithoutHandles` | 終了コード 1（今は 0） |
| トップレベルの `42` | ハンドルあり | `42`、終了コード 0 |
| `ovf`（`--overflow-report`。`def main :: unit -> i32 = \() -> if grow 4000000000i32u == 3410065408i32u then 0 else 1`、`grow x = x * 3i32u`） | `windows_launch --no-handles --debug-strings <path> -- ovf.exe` | `<path>` の行がちょうど 1 つで、`warning: integer overflow wrapped at ` で始まる |
| `ovf` | `--no-handles`（デバッガーなし） | 終了コード 0（`--overflow-report` なしの build と同じ） |
| `ovf` | ハンドルを継承（標準エラーはパイプ）、`--debug-strings <path>` | 標準エラーに同じ行、`<path>` は空（W04 の D3） |

- `ovf` は表示のない `def main` にする。トップレベルの表示にすると、ハンドルなしでは仕様 1 で終了コード 1 になり、報告の契約の検査にならない。

### Rust

- `tests/windows.rs`: 「設計」のとおり、Console の入口の検査を直す。
- `src/llvm.rs` の単体テスト `windows_overflow_report_falls_back_to_the_debugger`:
  - `--overflow-report` の IR（`--trap-info` あり・なし、`-O0`・`-O3`）を `windows_abi` に通す。
  - `@tz.overflow.write` の本体は `call void @tz.trap.write(` を持つ。
  - `define internal void @tz.trap.write(` と `define internal void @tz.debugger.write(` が 1 回ずつある。
  - 4 つの `declare` が 1 回ずつある。
  - `--trap-info` なしで `Debug.print` もない IR には、`call i64 @write(` も shim も残らない。
  - 2 回作って同じ文字列になる。
- POSIX の IR（`windows_abi` に通さない）は、`OVERFLOW_WRITE` を含み、`GetStdHandle` を含まない。`tests/overflow_report.rs` の `native_ir_reports_through_write_and_wasm_ir_exports_the_hits` は変えずに通る。
- `tests/chars.rs` の `define internal i32 @tz.console.write` の数（1）と、`tests/display_parse.rs` の `@tz.console.write` の存在の検査は、変えずに通る。

## ドキュメント

- `_tsuzuri/language-reference/built-in-types-and-modules/io.md` の「終了コードとトップレベルの表示」:
  - 箇条に「表示を書けなかったとき（標準出力が無い、閉じている、書き込みに失敗した）は、書けた部分を残して終了コード 1」を足す。
  - W08 が書いた「表示はストリームが無いと消える」の文を置き換える。
- `_tsuzuri/language-reference/compiler/usage.md`:
  - `run` の段落（347 行目付近）の表示の文に、同じことを 1 文で足す。
  - 「整数の折り返しの報告」（1002 行目付近）に 2 点を足す。標準エラーに書けないとき、Windows では報告をデバッガーの出力（`OutputDebugStringW`）に送り、POSIX では捨てること。どちらも値と終了コードは変わらないこと。
- `_tsuzuri/language-reference/compiler/platforms.md`: Windows の項に、次の 2 点を書く。GUI のプロセスでトップレベルの表示が終了コード 1 になること。`--overflow-report` の報告がデバッガーに届くこと。W04 の段落の隣に置く。
- `_tsuzuri/language-reference/compiler/native-interop.md`: 次の 2 点を書く。
  - `extern` の C が stdio（`printf` など）で標準出力に書き、flush していない出力は、トップレベルの表示の後に出る。表示は stdio のバッファを通らないためである。C の側で `fflush` する。
  - object を埋め込んだホストでも、Windows の `--overflow-report` の報告は標準エラーの OS のハンドルへ書く。W04 のトラップの報告の文に足す。
- `docs/language.md`: トップレベルの結果の自動表示の段落（1275 行目付近）に、書けなければ終了コード 1 を足す。
- `docs/architecture.md`: 「ホスト境界」の「コンソール出力用の `putchar` は…」（1814 行目付近）を、`write` で直接書くことに直す。
- `docs/stability.md` は変えない（「保証の単位」のバグの修正に当たる）。PR の説明に、POSIX と Windows で閉じた標準出力の表示が 0 から 1 に変わることを書く。

## 受け入れ条件

- [ ] 仕様 1〜3 のとおりに動く（テスト計画の `tests/io.mjs`・Windows・Rust）。
- [ ] `def main`・IO の入口・`unit` の結果・ライブラリ・WASM の IR、POSIX と WASM の `--overflow-report` の IR が、1 byte も変わらない。トップレベルの表示のプログラムの IR の差は、S2 の「予定した差」（Windows の native では shim・`_write` の宣言・`%binary_mode` の削除を含む）だけ（手順 1 との比較）。
- [ ] 既存の `tests/io.mjs`・`tests/overflow_report.rs`・`tests/chars.rs`・`tests/display_parse.rs` が、期待を変えずに通る。
- [ ] 宣言が重複せず、Windows の IR が決定的。
- [ ] 変更したページの `check-docs` と `check-runtime-includes.sh` が通る。
- [ ] `ci:full` を付けた PR で、`windows.yml` と `vscode.yml` の Windows の job が通る。
- [ ] T2 が通る。

## 落とし穴

- **`fflush` を宣言しない**: いちばん小さい修正に見えるが、予約した記号の表を広げることになり、`extern "fflush"` を拒否する破壊的な変更になる（「採らない案」）。
- **C の stdio との順序**: 表示を `write(1)` にすると、`extern` の C が stdio のバッファに残した出力は表示の後に出る（プロセスの終わりの flush）。Tsuzuri のランタイムは標準出力に書くたびに flush する（`io.c` の `tz_io_write_now`）ので、Tsuzuri だけのプログラムの順序は変わらない。文書に書く。
- **表示の fixture で報告の契約を検査しない**: ハンドルなしでは表示が終了コード 1 にするので、報告の検査は `def main` で行う。
- **置き換えの順序**: W04 の `TRAP_WRITE` の置き換えより前に `@tz.trap.write` を足すと、W04 の形の検査に当たる。
- **`OutputDebugStringW` はデバッガーがなければ何もしない**: 届いたかを確かめるのは `--debug-strings` だけ。終了コードの検査はデバッガーなしで行う（W04 の落とし穴）。
- **POSIX の検査は `sh -c 'exec "$0" >&-'`**: Node の `spawn` は閉じた fd を子に渡せない。閉じた標準出力は `sh` で作り、読み取り専用の fd は既存の `openSync(..., "r")` で作る。

## 対象外

- 表示の失敗の理由を標準エラーに書くこと（仕様 1 は終了コードだけ）。
- POSIX の `--overflow-report` の、書けない報告の届け先（`syslog` など。W04 の「対象外」と同じ）。`EINTR` で `write` が −1 を返したときの再試行（`TRAP_WRITE`・`debug.ll` と同じく今は止まる。直すなら 3 つをまとめて直す）。
- `Debug.print`・`IO.*` の意味（W08）、トラップとランタイム C の報告（W04）、トラップの終わり方（W07）。
- `src/runtime/cpu.c` の CPU の振り分けの失敗の文（`fputs` の後に終わる。致命的な報告で、W04 の `tzr_report` の扱いに近い）と、`src/runtime/stack.c` のスタック枯渇の報告（W07）。
- `RESERVED_HOST_SYMBOLS` から `putchar` を外すこと。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | トップレベルの表示を書けなければ、終了コード 1 にする。ストリームが無いときも同じで、トラップせず、ほかの経路にも書かない（W08 の D1 が既定案のとき。ほかの案は仕様 3） | `console_main` がもともと持つ失敗の経路で、表示はプログラムの本体の後に行うので、終了コードが自然な知らせ方である（coreutils の `close_stdout` と同じ）。トラップは走り終えたプログラムを異常終了に見せる。デバッガーへ送るのは、W08 の D1 が診断に限っている | 既定案（実装者はこの案に従う） |
| D2 | 表示は `write(1, ...)` で直接書き、`putchar` と stdio のバッファを使わない | 失敗がその場で分かり、両方の CRT で確かめた（`_write` は −1・`EBADF`）。`fflush` は予約した記号を広げて `extern "fflush"` を拒否することになり、`fflush(stdout)` は CRT ごとに `stdout` の実体が違う。費用は、`extern` の C が flush していない stdio の出力との順序だけ（文書に書く） | 既定案（実装者はこの案に従う） |
| D3 | Windows の `--overflow-report` は、W04 の報告の書き手（`@tz.trap.write` の Windows 版）へ転送し、書けなければデバッガーへ送る。POSIX と WASM は変えない | 報告の契約（値・トラップ・終了コードを変えない）の下では、捨てる代わりに残せる先はデバッガーの出力だけ。W04 の書き手は CRT の fd を通らず、閉じた fd 2 でも終わらない。POSIX には標準の経路が無い | 既定案（実装者はこの案に従う） |
| D4 | 表示の失敗では、ストリームが無いことと書き込みの失敗を区別しない | 表示の結果は終了コードだけで、`try_` のような値の受け手が無い。区別しても利用者が取れる行動は同じ | 既定案（実装者はこの案に従う） |
| D5 | `windows_abi` の表示のための `_setmode(1, _O_BINARY)` の差し込みを消し、`@write` の shim に任せる | shim が書き込みごとに同じ `_setmode` を呼ぶ。差し込みを残すと、同じ処理が 2 か所に残る | 既定案（実装者はこの案に従う） |
| D6 | Windows の報告の書き手を「無いときだけ足す」補助は、W07 と共有する 1 つにする | `intrinsics` と同じく宣言と定義を重複させないため。W04 の形の検査と順序を 1 か所で守れる | 既定案（実装者はこの案に従う） |
