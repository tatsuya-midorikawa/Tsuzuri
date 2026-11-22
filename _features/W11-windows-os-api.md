# W11: Windows の OS API（`File`・`Dir`・`Env`・`Random`・`Process`・`Time` の OS 時計）

| 項目 | 内容 |
| --- | --- |
| ID | W11 |
| 優先度 | P1 |
| 規模 | L（Phase 1: `Process` 以外のすべて。Phase 2: `Process.run`。人間が求めない限り Phase 1 だけを実装する） |
| 依存 | – |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-22（D-63。利用者の依頼「X: D13 を狭く改める方針で決定。実現向けたチケットを @_features に W プリフィックスのチケットを作成して」と「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」から起票。HEAD `fa40eede` で確認） |
| 承認 | 要確認（E08 Phase 2 の Windows の方針の承認の記録がない。E08 の承認の記録は D1・D9・D10・D12 だけで、Windows を外した D11 は既定案）。確認を着手条件にする。決定事項 D4 は要承認 |
| 手本にする既存実装 | `src/runtime/os.c`（POSIX の 10 個の入口・状態値・`tz_os_deliver`・`File.Handle` の世代付きの表・`tz_os_spawn_now` の上限の扱い）、`src/runtime/tzroutine-windows.h` と `src/driver.rs` の `tzroutine_runtime_source_for`（Windows の半分を別ファイルにして、driver が `#include` を中身で置き換える形）、`src/runtime/net.c` の Windows の部分（`WIN32_LEAN_AND_MEAN` を未定義のときだけ定義、`#pragma comment(lib, …)` と driver の `-lws2_32`）、`src/runtime/arguments.c` の `tz_arguments_split`、`tests/windows.rs` の `cross_links_the_net_runtime_with_microsoft_sdk_when_requested` |
| 主な影響ファイル | `src/runtime/os.c`・`src/runtime/os-windows.h`（新規）・`src/runtime/arguments.c`・`src/runtime/arguments-split.h`（新規）・`src/runtime/tzroutine.c`・`src/target.rs`・`src/driver.rs`・`src/test_runner.rs`・`tests/os.mjs`・`tests/windows.mjs`・`tests/windows.rs`・`tests/tzroutine.mjs`・`tests/targets.mjs`・`.github/workflows/windows.yml`・`.github/workflows/vscode.yml`・`scripts/check-runtime-includes.sh`（確認のみ）・文書 |

## 実装者向け早見

- 範囲: E08 の「Phase 2（設計方針）」の Windows の項を、今の `src/runtime/os.c` の 10 個の C の入口（`tsuzuri_os_read`・`args`・`set_args`・`write`・`open`・`handle`・`close`・`spawn`・`random`・`clock`・`sleep`）の Win32 版として実装し、`E2002` のゲートを外す。**生成 IR・std・組み込み・C の入口の型は 1 byte も変えない。** 変わるのはランタイムの C と driver のゲートとリンクだけ。
- Phase 1: `File`・`Dir`・`Env`・`Time`（`monotonic_ns`・`unix_ns`・`sleep_ms`）・`Random`。`Process.run` に到達するビルドは Windows では `E2002`（新しい文）のまま。Phase 2: `Process.run`（`CreateProcessW`、引用、PATH の探索、`.bat`・`.cmd` の拒否、パイプの吸い出し）。
- 最初に読む: ① 現状 ② 仕様 2 の対応表 ③ 設計 1〜3 ④ `src/runtime/os.c` 全体（958 行）と `tzroutine-windows.h`（156 行）。
- 最初に書くテスト: `tests/windows.rs` に `cross_links_the_os_runtime_with_microsoft_sdk_when_requested`（`TSUZURI_WINDOWS_SDK` があるときだけ、`os.c` を `--target=x86_64-pc-windows-msvc -Wall -Wextra -Werror` でコンパイルしてリンクする）→ `tests/os.mjs` の Windows の表（仕様 2）の 1・2 群。
- 触ってはいけない: POSIX と WASI の挙動と `tests/os.mjs` の POSIX の期待値、`Os.ErrorKind` の case（G19 の D8: 足すなら別チケットで承認）、std の関数の型と文書の例、`def main :: Array<string> -> i32` の分割の規則（変えるのは [W33](W33-windows-command-line-splitting.md)）。
- 完了の見分け方: Windows のホストで、MSVC（Developer PowerShell と `TSUZURI_CLANG` に LLVM の clang）と同梱の MinGW の両方で、下が `héllo` と `ok:` で始まる 1 行を出して終了コード 0 になる（`-O0`・`-O3`）。

```tsuzuri
def main :: unit -> i32 = \() ->
    let! _wrote = File.write_text "w11.txt" "h\u00e9llo\n"
    let! text = File.read_text "w11.txt"
    do! IO.write (match text with
        | Result.Ok value -> value
        | Result.Error error -> Os.message (ref error))
    let! names = Dir.list "."
    do! IO.writeln (match names with
        | Result.Ok values -> "ok:" + to_string values.length
        | Result.Error error -> Os.message (ref error))
    0
```

## 目的

Windows のネイティブビルドでは、`File`・`Dir`・`Env`・`Time` の OS 時計・`Random`・`Process` に到達すると `E2002` になる。Tsuzuri だけで Win32 のアプリを書く（[GUIDE D-63](GUIDE.md#d-63-tsuzuri-だけで-win32-のアプリを書くe12-d13-の狭い改訂w01w30)）には、設定ファイル・ログ・作業ディレクトリ・子プロセスを、毎回 `extern` を宣言せずに std で扱えなければならない。E08 は Windows を「G10 の後」とし（D11）、G10 は実行ゲート（`windows.yml`）を作ったが、OS API の実装は後続の作業として残した（[G10](_completed/G10-windows.md) の「後続の作業」）。ゲートがある今、E08 の設計方針を Win32 で実装し、POSIX と同じ API・同じエラーの形で動かす。

## 着手条件と停止条件

### 着手条件

- **人間の確認**: E08 Phase 2 の Windows の方針（[E08](_completed/E08-os-api.md) の「Phase 2（設計方針）」）と、それをこのチケットの決定事項で改めた点（D2・D4・D8）の承認を得る。D4 は要承認で、承認前は手順 4 の `Env.args` の段に着手しない（ほかの段は確認の後に進めてよい）。
- `node scripts/repo-kb.mjs brief W11`。`gh run list --branch main --workflow ci.yml --limit 1` と、`windows.yml` の main の最新の結果を基準にする。
- Windows の開発機（MSVC の Developer PowerShell と LLVM 21 以降の clang、または同梱の `tsuzuri-clang`）。macOS・Linux だけの開発機では Phase 1 の C は `tests/windows.rs`（`TSUZURI_WINDOWS_SDK`）で型検査しかできないので、実行の検証は `ci:full` の `windows.yml` に頼る（G10 と同じ）。

### 停止条件

- **S1** 着手条件の確認が得られない。
- **S2** どれかのプログラムの生成 IR が変わる（`tests/fixtures`・`examples` の `--emit llvm`、native・wasm32・`--target x86_64-pc-windows-msvc`）。このチケットは IR を変えない前提で書いてある。
- **S3** `tests/os.mjs` の POSIX の結果、`tests/os-wasi-host.mjs`、`tests/io.mjs` の期待値を変えないと通らない。
- **S4** `Os.ErrorKind` の case、新しい診断コード、新しい crate、`unsafe`、新しい CLI のフラグ・環境変数が要るように見える。
- **S5** Phase 2: `Process.run` の引用が、Node の子（`process.execPath`。MSVC の CRT が分割する）で引数を往復できない種類がある。黙って文字を落とさず、種類と候補を報告する。
- **S6** 同梱の MinGW（x64 の Zig、Arm64 の LLVM-MinGW）に `bcrypt` の import library がなく、`-lbcrypt` でリンクできない。
- **S7** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

- `src/target.rs` の `Platform::os_apis` は `!self.windows()`。テスト `platform_properties_follow_the_target_table` が Windows で偽であることを確かめる。
- `src/driver.rs`: `OS_WINDOWS_MESSAGE`（`the File, Dir, Env, Time, Random, and Process operating-system APIs are not supported on Windows yet (G10); build on macOS or Linux`）と `os_message`（名指しの Windows の target では「build for macOS, Linux, or FreeBSD」に置き換える）。ビルドの関数は `os_runtime = runtime_symbols.prefix("tsuzuri_os_")` を求め、`os_runtime && !platform.os_apis() && options.target == Target::Native && options.emit != Emit::Llvm` で `E2002`（kind `platform_unsupported_api`）を返す。`--emit llvm` は通る。ランタイムの連結では `os.c` を 9 つの部分の先頭に置く（「feature macros of os.c and net.c … must precede every include」）。Windows で足すライブラリは `net_runtime` の `-lws2_32` だけ。
- `src/test_runner.rs`: `tsuzuri test` の実行ファイルを作る 2 つの経路（`os.c` を `sources` に積む所と、`os_runtime` を求めて各部分を別々の C ファイルとしてコンパイルする所）が同じ `E2002` を返す。Windows のリンクで足すのは `-lws2_32` だけ。
- `src/runtime/os.c`: POSIX だけ（`dirent.h`・`poll.h`・`pthread.h`・`spawn.h`・`sys/wait.h`、`getentropy`、`clock_gettime`、`nanosleep`）。`TZ_OS_API` は `__attribute__((weak, visibility("hidden")))`。ブロッキングのフック（TZ08）の定義だけは Windows を考えてあり、COFF には弱いデータがないので `selectany`（「the runner of `tsuzuri test` compiles each part of the runtime alone」）。`tsuzuri_os_sleep_hook` は `weak` のまま。`tsuzuri_os_set_args` は CRT の `argv`（Windows では ANSI）を保存し、`tsuzuri_os_args` はそれを返す（保存がなければ空。E08 D14）。
- `src/runtime/tzroutine.c`: `tsuzuri_os_sleep_hook` の宣言・`tzr_init` での設定・`tzr_sleep_if_routine` の定義が `#if !defined(_WIN32)` の中にあり、注釈は「os.c has no Windows version」。Windows で `os.c` を足しても、このままでは tzroutine の中の `Time.sleep_ms` が M を止める（TZ05 D6 に反する）。
- `src/runtime/arguments.c`（`def main :: Array<string> -> i32`）: Windows では `tz_arguments_split((const uint16_t *)GetCommandLineW(), …)` が空白・タブで分け、二重引用符は状態を反転して消すだけで、**バックスラッシュに特別な意味はない**。`env.md`（「引数と main」）と `docs/language.md`（`def main` の節の「`"C:\dir\"` は文字列 `C:\dir\`」）がこの規則と「`Env.args` と `main` の引数は同じ個数」を約束している。一方 E08 Phase 2 は `Env.args` を `CommandLineToArgvW(GetCommandLineW())` にすると書いており、両者は食い違う（`"a\" b"` は `CommandLineToArgvW` で 1 個、`tz_arguments_split` で 2 個）。
- `Time.sleep`・`Time.after` などは tzroutine のランタイムで動き、Windows でも使える（`Time.sleep (5 * Time.millisecond())` の後に 7 を返すプログラムが、同梱の MinGW で終了コード 7）。時計は `tzroutine-windows.h` の `tzr_nanotime`（`QueryPerformanceCounter` を秒と余りに分けてナノ秒にする）。
- 再現（Git Bash、`T=/c/Users/tmidorikawa/.vscode/extensions/tmidorikawa.tsuzuri-0.1.6-win32-x64/toolchain`、早見のプログラムの `File.write_text`・`File.read_text` の部分を `target/scratch/w11/os1/Main.tz` に置く）: `TSUZURI_CLANG=$T/bin/tsuzuri-clang.exe target/debug/tsuzuri.exe check target/scratch/w11/os1` は 0。`build … -o os1.exe --json` は `{"severity":"error","code":"E2002","message":"the File, Dir, Env, Time, Random, and Process operating-system APIs are not supported on Windows yet (G10); build on macOS or Linux",…,"kind":"platform_unsupported_api"}`。`build --emit llvm` は通り、IR に `declare i64 @tsuzuri_os_read(ptr, i32, ptr, i64)` と `declare i64 @tsuzuri_os_write(i32, ptr, i64, ptr, i64)`。`Time.monotonic_ns ()` だけのプログラムも同じ `E2002`。
- Win32 の呼び出しの可否（起票時に `target/scratch/` の下の小さな C で確認し、削除した）: `BCryptGenRandom(NULL, …, BCRYPT_USE_SYSTEM_PREFERRED_RNG)`・`GetSystemTimePreciseAsFileTime`・`InitializeProcThreadAttributeList`・`FindFirstFileExW(FindExInfoBasic, FIND_FIRST_EX_LARGE_FETCH)`・`CreateNamedPipeW(PIPE_REJECT_REMOTE_CLIENTS | FILE_FLAG_OVERLAPPED)`・`FILE_ATTRIBUTE_TAG_INFO` は、同梱の `tsuzuri-clang`（Zig の MinGW）でも LLVM 22.1.8 の clang（MSVC）でも `-std=c11 -Wall -Wextra -Werror` で通り、実行できた。MinGW は `-lbcrypt` がないと `BCryptGenRandom` が未解決でリンクに失敗する。Arm64 の LLVM-MinGW は未確認。
- テスト: `tests/os.mjs` は POSIX の前提（`os.constants.errno`、`chmod 000`、`/proc/self/fd`・`/dev/fd`、`sort`・`head`・`yes` の子）で、`windows.yml` は走らせない。`tests/tzroutine.mjs` は「Time.sleep_ms in a tzroutine waits without its thread」と「Time.after's value is on Time.monotonic_ns's clock」の検査を `process.platform !== "win32"` で飛ばす（注釈「os.c is POSIX only」）。`tests/targets.mjs` は `x86_64-pc-windows-gnu` の `Env.var` のビルドが `not supported on Windows yet (G10)` で失敗することを確かめる。
- 文書: `os.md`（「WASM と Windows」）・`file.md`・`dir.md`・`env.md`・`time.md`・`random.md`・`process.md`・`path.md`・`test.md`、`compiler/platforms.md`（OS ごとの表の `E2002`、「OS API」の段落、診断の表）・`diagnostics.md`・`native-interop.md`・`usage.md`、`languages/strategy.md`・`why-tsuzuri.md`・`how-about-tsuzuri.md`、`docs/language.md` の「OS API」と「WASM と Windows」が、Windows の OS API は `E2002` と書いている。

## 変えてはいけない意味

- 生成 IR（S2）、std の型、`Os.Error`・`Os.ErrorKind`、状態値 `(kind << 32) | (code & 0xffffffff)`（E08 D2）。
- POSIX と WASI の挙動。`Dir.list` の順序は UTF-8 の byte 順（E08 D7）。symlink の扱い（E08 D13: 読み書きはたどる、`File.remove` は link を消す、`Dir.list` は名前を返す、`Dir.remove` は link に `NotFound`、`Dir.walk` はたどらない）。
- `Env.args` は `set_args` が呼ばれていなければ空（E08 D14。`tsuzuri test` の実行ファイル・ライブラリ）。`Env.args` と `def main :: Array<string> -> i32` は同じ個数（`env.md`）。
- tzroutine の中の `Time.sleep_ms` は G だけを止める（TZ05 D6）。`Time.after` の値は `Time.monotonic_ns` と同じ時計（TZ05 仕様 1）。
- OS 呼び出しは TZ08 のブロッキングのフック（`TZ_BLOCKING_ENTER`／`EXIT`）で囲む（POSIX と同じ）。

## 仕様

### 1. 利用者から見える変更

Windows のネイティブ（x64・Arm64、MSVC と MinGW、`--target native` と名指しの target）で、OS API に到達するビルドが通る。Phase 1 では `Process.run` に到達するビルドだけが `E2002`（kind は既存の `platform_unsupported_api`。文は案として `the Process operating-system API is not supported on Windows yet (W11 Phase 2); File, Dir, Env, Time, and Random are supported, and a child process can be started through an extern of CreateProcessW`）。Phase 2 でそれも外す。wasm と `--emit llvm` の扱いは変わらない。

### 2. API ごとの Windows の意味（POSIX との対応）

`Os.Error.code` は `GetLastError` の値（Phase 2 の `BCryptGenRandom` の失敗は NTSTATUS をビットのまま `i32` に）。Tsuzuri が自分で見つけた失敗は code 0（E08 D2）。

| API（op） | Win32 | Windows での意味（POSIX との違い） |
| --- | --- | --- |
| `File.read_bytes`・`read_text`（read 0） | `CreateFileW(GENERIC_READ, 共有 READ\|WRITE\|DELETE, OPEN_EXISTING)`・`GetFileSizeEx`・`ReadFile` | ディレクトリは `ERROR_ACCESS_DENIED` を `GetFileAttributesW` で見分けて `InvalidInput`（code 0。POSIX は `EISDIR`） |
| `Dir.list`（read 1） | `FindFirstFileExW(<path>\*, FindExInfoBasic, FIND_FIRST_EX_LARGE_FETCH)`・`FindNextFileW` | `.`・`..` を除き、名前を UTF-8 にしてから byte 順（D7）。空のパス `""` は `NotFound`（code 3。`\*` にしない）。ドライブの根の空の一覧（`ERROR_FILE_NOT_FOUND`）は、ディレクトリなら空の配列 |
| `Env.current_dir`（read 2） | `GetCurrentDirectoryW` | `C:\…` の形（区切りは `\`）。1 MiB の上限は同じ |
| `Env.var`（read 3） | `GetEnvironmentVariableW` | 名前の検査（空・`=`・NUL は `InvalidInput`）は同じ。未設定は kind 1・code 0（`Ok None`）。名前の大文字小文字を区別しないのは OS の規則 |
| `File.metadata`・`link_metadata`（read 4・5） | `CreateFileW(FILE_READ_ATTRIBUTES, FILE_FLAG_BACKUP_SEMANTICS [\| FILE_FLAG_OPEN_REPARSE_POINT])`・`GetFileInformationByHandle`・`GetFileInformationByHandleEx(FileAttributeTagInfo)`・`GetFileType` | 種類: ディスクのファイル 0、ディレクトリ 1、名前の代理の reparse point（`IsReparseTagNameSurrogate`: symlink と junction）を `link_metadata` で開いたとき 2、それ以外（`NUL` などの装置）3。ディレクトリの大きさは 0。時刻は `FILETIME` の更新時刻を Unix のナノ秒に（i64 で飽和） |
| `Env.args`（`args`） | `GetCommandLineW` と `arguments.c` と同じ分割 | D4。`set_args` が呼ばれていなければ空。分割した UTF-16 を厳密に UTF-8 へ（孤立サロゲートは `InvalidEncoding`、code 1113）。CRT の `argv` は使わない |
| `File.write_text`・`write_bytes`（write 0） | `CreateFileW(GENERIC_WRITE, CREATE_ALWAYS)`・`WriteFile` | 改行の変換はしない（POSIX と同じ byte）。読み取り専用の属性のファイルは `PermissionDenied`（5） |
| `File.append_text`（write 1） | `CreateFileW(FILE_APPEND_DATA, OPEN_ALWAYS)` | `O_APPEND` と同じく末尾へ |
| `Dir.create`・`Dir.remove`（write 2・3） | `CreateDirectoryW`・`RemoveDirectoryW` | 空でない → `Other`（145）。`Dir.remove` を reparse point のディレクトリに使うと、link を消さずに `NotFound`（code 0。D13 と同じ結果。D7 の決定） |
| `File.remove`（write 4） | `DeleteFileW`、ディレクトリの reparse point なら `RemoveDirectoryW` | link 自体を消す（D13）。読み取り専用のファイルは `PermissionDenied`（POSIX の `unlink` は消せる）。ほかのプロセスが共有なしで開いているファイルは `PermissionDenied`（32） |
| `File.open`・`read`・`write`・`flush`・`close` | `CreateFileW`（Read: `OPEN_EXISTING`、Write: `CREATE_ALWAYS`、Append: `FILE_APPEND_DATA` と `OPEN_ALWAYS`、CreateNew: `CREATE_NEW` → 80 は `AlreadyExists`）・`ReadFile`・`WriteFile`・`CloseHandle` | 表は POSIX と同じ `(generation << 32) \| (slot + 1)`、ロックは `SRWLOCK`。`flush` は検査だけ（POSIX と同じく何も溜めない） |
| `Time.monotonic_ns`（clock 0） | `QueryPerformanceCounter`・`QueryPerformanceFrequency` | `tzr_nanotime` と同じ式（秒と余りに分ける）。起点は未規定 |
| `Time.unix_ns`（clock 1） | `GetSystemTimePreciseAsFileTime` | `(ft − 116444736000000000) × 100`。範囲外は `INT64_MIN`（失敗。POSIX と同じ扱い） |
| `Time.sleep_ms`（sleep） | `Sleep` の繰り返し | tzroutine の中ではフック（`tzr_sleep_if_routine`）で G だけが待つ。それ以外は `monotonic_ns` の期限まで `Sleep`（切り上げたミリ秒）を繰り返す（`Sleep` は時計の分解能より短い値で早く戻りうるので、期限を見る） |
| `Random.bytes`・`next_u64`（random） | `BCryptGenRandom(NULL, buf, n, BCRYPT_USE_SYSTEM_PREFERRED_RNG)` | 上限 2^30 は同じ。失敗は `Other`（code は NTSTATUS のビット） |
| `Process.run`（spawn、Phase 2） | 仕様 4 | 仕様 4 |

### 3. パスと文字

- パスは std がこれまでどおり UTF-8（`Os.encode`）で渡し、ランタイムが `MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS)` で UTF-16 にする（D2）。NUL は `InvalidInput`（code 0、既存の `tz_os_name`）。`/` と `\` はどちらも区切りとして OS が受け取る。
- 長いパス: 変換後の長さが 248 単位（`MAX_PATH − 12`）以上で、`\\?\`・`\\.\` で始まらないものは、`GetFullPathNameW` で絶対パスにしてから `\\?\`（UNC は `\\?\UNC\`）を付ける（D5）。短いパスはそのまま渡す（OS の通常の解釈。D13 の「Tsuzuri は正規化しない」と同じ結果）。
- OS から受け取る名前・値（`Dir.list`、`current_dir`、`Env.var`、`Env.args`）は `WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS)` で厳密に UTF-8 にする。NTFS は孤立サロゲートを含む名前を持てるので、その一覧は全体が `InvalidEncoding`（code 1113。E08 D4 の「置換文字は使わない」）。
- `CON`・`NUL`・`COM1` などの装置名、末尾の `.`・空白の除去、大文字小文字の同一視は OS の規則のまま（D13）。`Path` は `/` だけを区切りと見る純粋な関数のまま（`current_dir` の `\` を分けない）。

### 4. `Process.run`（Phase 2）

- **プログラムの解決**: 名前が `/`・`\`・`:` を含めば、作業ディレクトリからのパスとしてだけ見る。含まなければ、環境変数 `PATH` のディレクトリ（`;` で分け、空の要素を飛ばし、前後の `"` を外す）だけを順に探す。作業ディレクトリと実行ファイルのディレクトリは探さない（`CreateProcessW` の既定の探索をしない。POSIX の `posix_spawnp` と同じ「PATH だけ」）。候補は、最後の要素に `.` がなければ `<name>.exe`・`<name>.com`、あれば `<name>`・`<name>.exe` の順。見つからなければ `NotFound`（code 2）。解決したフルパスを `lpApplicationName` に渡す。
- **`.bat`・`.cmd` の拒否**（D8）: 解決したパスの最後の要素から末尾の `.` と空白を取り除き、`:` 以降（代替データストリーム）を外した名前が、大文字小文字を問わず `.bat`・`.cmd` で終われば、子を作らずに `InvalidInput`（code 0）。バッチファイルは `cmd.exe` が独自の規則でコマンドラインを解釈し、引数を注入できる（BatBadBut。Rust の CVE-2024-24576 とその不完全な修正の CVE-2024-43402）。`Process.run` は「shell を介さない」と約束しているので、置き換えて実行しない（GUIDE §1 規則 2）。バッチを動かしたい利用者は `Process.run "cmd.exe" ["/d", "/c", …]` と書き、`cmd.exe` の解釈を自分で引き受ける（`sh -c` と同じ。文書に書く）。
- **コマンドライン**: 先頭はプログラム名（利用者が渡した文字列。`"` を含めば `InvalidInput`）を `"…"` で囲んだもの。各引数は MSVC の CRT（2008 以降）と `CommandLineToArgvW` が元に戻せる形にする（D9）: 空か空白・タブ・改行・`"` を含めば `"` で囲み、`"` の前のバックスラッシュの並びは 2 倍にして `\"`、囲みの閉じる `"` の前の末尾のバックスラッシュも 2 倍。合計が 32767 単位を超えたら `CreateProcessW` の失敗（206）をそのまま返す。
- **Tsuzuri の子への注意**（D4 との関係）: 子が Tsuzuri のプログラム（`arguments.c` の規則）だと、`"` を含む引数と、空白を含みバックスラッシュで終わる引数は同じ形で届かない。文書に書く。[W33](W33-windows-command-line-splitting.md)（要承認）が分割を UCRT の規則にそろえると、この注意は要らなくなり消す。W33 が先に入っていれば書かない。このチケットが先なら、W33 の PR が消す（W33 の設計 4）。
- **起動**: `CreateProcessW(app, cmdline, NULL, NULL, TRUE, CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT, NULL, NULL, &si.StartupInfo, &pi)`。`STARTUPINFOEXW` の `PROC_THREAD_ATTRIBUTE_HANDLE_LIST` に子の 3 つの端だけを載せる（ほかの tzroutine が同時に作ったパイプの端を継がせない）。環境と作業ディレクトリは継ぐ。コンソールは親のものを継ぐ（`CREATE_NO_WINDOW` は付けない）。
- **入出力**: 親の端を `FILE_FLAG_OVERLAPPED` の名前付きパイプ（`\\.\pipe\tsuzuri-<pid>-<連番>-<乱数>`、`PIPE_REJECT_REMOTE_CLIENTS`、`FILE_FLAG_FIRST_PIPE_INSTANCE`）、子の端を継承可能な普通のハンドルにし、1 本のスレッドで `ReadFile`・`WriteFile` の重なった I/O と `WaitForMultipleObjects` で、標準入力の書き込み（64 KiB ずつ）と 2 つの出力の吸い出しを同時に進める（D10）。上限（合わせて 2^30 byte、読みは上限 + 1 byte で止める）を超えたら `TerminateProcess(child, 1)` して `Other`（code 0）。子が標準入力を閉じた（109・232）ら書き込みだけをやめる（POSIX の `EPIPE` と同じ）。
- **結果**: `GetExitCodeProcess` の値をビットのまま `i32` にして `code`、`signal` は常に 0（D11。`0xC000001D` で終わった Tsuzuri の子の `code` は −1073741795）。

## 設計

### 1. ファイルの配置（D1）

- `src/runtime/os.c` の先頭の共通部分（feature macro、`<stdint.h>` など、`TZ_OS_API`、`tsuzuri_alloc`、`struct tz_os_buffer`、ブロッキングのフック、kind の enum、`tz_os_status`・`tz_os_invalid_input`・`tz_os_clear`・`tz_os_deliver`・`tz_os_name`・`tz_os_compare_names`・`tz_os_put_i64`）は両 OS で共有し、その後を `#if defined(_WIN32)` → `#include "os-windows.h"`、`#else` → 今の POSIX の本体、`#endif` に分ける。POSIX のヘッダーの include も `#else` の側へ移す（POSIX の text の変更は移動だけ）。
- Windows の側の include: `WIN32_LEAN_AND_MEAN`・`_CRT_SECURE_NO_WARNINGS` を未定義のときだけ定義して `<windows.h>`・`<bcrypt.h>`。`os.c` は連結の先頭なので、後に来る `net.c` の `<winsock2.h>` と衝突しないよう `<winsock.h>` を引き込まない形にする。`#if defined(_MSC_VER)` で `#pragma comment(lib, "bcrypt.lib")`。
- `TZ_OS_API` は Windows では空（`TZR_API` と同じ。COFF に弱い定義を使わない）。`tsuzuri_os_sleep_hook` の定義は Windows で `__attribute__((selectany)) … = 0`（ブロッキングのフックと同じ書き方）。
- driver に `os_runtime_source_for(platform) -> String` を足し（`tzroutine_runtime_source_for` と同じく、Windows なら `#include "os-windows.h"` を `include_str!("runtime/os-windows.h")` で置き換える）、`src/driver.rs` の連結と `src/test_runner.rs` の 2 か所の `include_str!("runtime/os.c")` をこの関数に替える。

### 2. Windows の部分（`src/runtime/os-windows.h`）

| 関数（すべて `static`、名前は `tz_os_` で始める） | 役目 |
| --- | --- |
| `tz_os_win_error(DWORD)` | 対応表（D3）で状態値にする |
| `tz_os_wide(const char *utf8, const wchar_t *suffix, int64_t *status)` | UTF-8 を UTF-16 に変換し、`suffix`（`Dir.list` の `\*`）を足し、長いパスなら D5 の形にした `malloc` の文字列。失敗は NULL と `*status` |
| `tz_os_narrow(const wchar_t *, size_t, struct tz_os_buffer *)` | UTF-16 を厳密に UTF-8 にして `tz_os_deliver` で渡す |
| `tz_os_open_file(const wchar_t *, DWORD access, DWORD disposition, DWORD flags)` | 共有 `FILE_SHARE_READ \| FILE_SHARE_WRITE \| FILE_SHARE_DELETE`（D6）。継承しない（`lpSecurityAttributes` は NULL。POSIX の `O_CLOEXEC` に当たる） |
| `tz_os_read_now`・`tz_os_write_now`・`tz_os_open_now`・`tz_os_handle_now`・`tz_os_close_now`・`tz_os_random_now` | POSIX と同じ名前と op 番号で Win32 を呼ぶ。入口の `tsuzuri_os_*` はファイルの最後で、POSIX と同じくフックで囲む |
| `tz_os_args_now` | `tz_os_argument_count == 0` なら空。そうでなければ `arguments-split.h` の分割で `GetCommandLineW` を分け、名前の後ろに NUL を付けて渡す（D4） |
| `tz_os_spawn_now`（Phase 2） | 仕様 4。Phase 1 では `ERROR_CALL_NOT_IMPLEMENTED` の `Other` を返す定義だけを置く（driver が先に `E2002` にするので届かない） |

- 対応表（D3）: 2・3 → `NotFound`、5・32 → `PermissionDenied`、80・183 → `AlreadyExists`、87・123・206・267 → `InvalidInput`、1113 → `InvalidEncoding`、それ以外（145・193 を含む）→ `Other`。E08 の表に 206（`ERROR_FILENAME_EXCED_RANGE`、POSIX の `ENAMETOOLONG` に当たる）だけを足した。
- `CREATE_ALWAYS`・`OPEN_ALWAYS` は成功しても `GetLastError` を 183 にするので、成功のときに読まない。`GetEnvironmentVariableW` は空の値と未設定の両方で 0 を返すので、呼ぶ前に `SetLastError(0)` し、0 が返ったら `ERROR_ENVVAR_NOT_FOUND`（203）で見分ける。値の長さは 2 回の呼び出しの間に変わりうるので、足りなければ繰り返す。
- `WriteFile`・`ReadFile` は `DWORD` なので 1 GiB ずつに分ける。

### 3. 引数の分割の共有（D4）

- `arguments.c` の `tz_arguments_blank`・`tz_arguments_token`（[W33](W33-windows-command-line-splitting.md) が先に入っていれば `tz_arguments_program` も。W33 の設計 4）を `src/runtime/arguments-split.h`（新規）へ移し、`#ifndef TZ_ARGUMENTS_SPLIT_DEFINED` で囲む（連結された 1 つの翻訳単位では 1 回だけ定義、`tsuzuri test` の別々のコンパイルではそれぞれに `static` の写し）。`arguments.c` と `os-windows.h` が `#include "arguments-split.h"` し、driver の `arguments.c` と `os.c` の取り込みが中身で置き換える（`tzroutine-windows.h` と同じ）。`tests/windows.rs` のように置き場でそのままコンパイルしても `#include` が効く。
- `os-windows.h` は分割した UTF-16 を UTF-8 にする。孤立サロゲートは `InvalidEncoding`（`main` の引数は U+FFFD に置き換えるが、`Env.args` は厳密。`env.md` の既存の違い）。

### 4. driver・test_runner・target

- `src/target.rs`: `os_apis` はすべての target で真（注釈を「macOS・Linux は POSIX、Windows は Win32」に）。Phase 1 だけ `os_process(self) -> bool { !self.windows() }` を足す（Phase 2 で消す）。テストの期待値を直す。
- `src/driver.rs`: Phase 1 は `os_runtime && runtime_symbols.contains("tsuzuri_os_spawn") && !platform.os_process() && Target::Native && emit != Llvm` で `E2002`（仕様 1 の文。定数 `PROCESS_WINDOWS_MESSAGE`）。このゲートは、今の OS のゲートと同じ位置、つまりランタイム入りの COFF object の汎用の `E2002`（`platform.windows() && native_runtime && options.emit == Emit::Object`）より前に置く。`OS_WINDOWS_MESSAGE` と `os_message` の Windows の腕は消す（使われない定数は clippy の `-D warnings` で落ちる）。`os_runtime && platform.windows()` の実行ファイルのリンクに `-lbcrypt`。
- ランタイム入りの COFF object: `os_apis` が真になると、OS API に到達する Windows の `--emit object` は、OS 専用の `E2002` ではなく汎用の `E2002` に落ちる（E08 のレビュー対応が、OS API の object に専用の文を返すよう判定の順序を入れ替えた経緯がある）。汎用の文 `Windows COFF objects with embedded tzroutine, CPU, IO, or Net runtime are not supported; …` に OS のランタイムを足す（案: `… tzroutine, CPU, IO, Net, or operating-system runtime …`）。この文の扱いは [W03](W03-windows-interop-doc-fixes.md) の (2) も文書で触れるので、先に入った方に合わせる。
- `src/test_runner.rs`: 同じゲートと、2 か所のリンク（`-lws2_32` を足している所の隣）に `-lbcrypt`。
- `src/runtime/tzroutine.c`: `tsuzuri_os_sleep_hook` の宣言・`tzr_init` の設定・`tzr_sleep_if_routine` の `!defined(_WIN32)` を外し（`TZR_JSPI` の除外は残す）、Windows の定義は `selectany`。注釈を直す。

## 前提とする他チケットのインターフェース

- なし（独立）。[W08](W08-io-without-std-streams.md)・[W09](W09-windows-gui-subsystem.md) とは独立で、GUI サブシステムでもこのチケットの関数はコンソールを要らない。

## 他チケットへ提供するインターフェース

- Windows の `File`・`Dir`・`Env`・`Time`・`Random`・`Process.run`（W27 の生成器や Win32 のアプリが設定ファイル・ログ・子プロセスに使う）。
- `src/runtime/arguments-split.h`（Windows のコマンドラインの分割の唯一の実装）。

## 実装手順

### 手順 1: 基準

- 変更: なし（`target/scratch/w11/` の下だけ）。
- 内容: `tests/fixtures`・`examples` のすべての `Main.tz` を、native・`--target wasm32`・`--target x86_64-pc-windows-msvc` の `--emit llvm -O1` で保存する（手順 7 で比べる）。macOS か Linux で `node tests/os.mjs target/debug/tsuzuri` の結果を控える。
- 確認: 保存した数と、ビルドできない fixture の数を記録する。

### 手順 2: `os.c` の分割と driver の取り込み

- 変更: `src/runtime/os.c`（共通部分と `#if`）、`src/runtime/os-windows.h`（新規。`tsuzuri_os_*` は `ERROR_CALL_NOT_IMPLEMENTED` を返すだけの仮の定義）、`src/driver.rs`・`src/test_runner.rs` の `os_runtime_source_for`、`src/runtime/tzroutine.c` のフック。
- 内容: POSIX の text は移動だけにする。
- 確認: T0。`sh scripts/check-runtime-includes.sh`。macOS・Linux の `node tests/os.mjs target/debug/tsuzuri` が手順 1 と同じ。Windows で `clang --target=x86_64-pc-windows-msvc -std=c11 -Wall -Wextra -Werror -fsyntax-only src/runtime/os.c` と、`net.c`・`io.c`・`tzroutine.c` を後ろに連結した 1 ファイルのコンパイルが通る。

### 手順 3: ファイルとディレクトリ

- 変更: `os-windows.h` の read 0・1・4・5、write 0〜4、open・handle・close、`tz_os_wide`・`tz_os_narrow`・`tz_os_win_error`・`tz_os_open_file`。`tests/os.mjs` の Windows の表（テスト計画）の 1〜8・17・19・20 群と `tests/os_windows_reference.c`。
- 内容: 仕様 2・3 の表のとおり。この段の確認のためだけに、手元では `os_apis` を先に真にしてよい（ゲートの正式な変更は手順 5）。
- 確認: Windows で `node tests/os.mjs target/debug/tsuzuri.exe` の該当群（MSVC の Developer PowerShell と、Git Bash の `TSUZURI_CLANG=$T/bin/tsuzuri-clang.exe` の両方）。

### 手順 4: `Env`（D4 の承認後）・`Time`・`Random`

- 変更: `src/runtime/arguments-split.h`（新規）と `arguments.c`、`os-windows.h` の read 2・3、`args`、`clock`、`sleep`、`random`。driver と test_runner の `-lbcrypt` と `arguments-split.h` の置き換え。`tests/tzroutine.mjs` の `win32` の除外を外す。
- 内容: 設計 3。`tz_arguments_split` の振る舞いは変えない（移動だけ）。`sleep` は仕様 2 の期限の繰り返し。
- 確認: 9〜12 群。`tests/tzroutine.mjs` の 2 つの検査（`Time.sleep_ms` の G だけの待ち、`Time.after` の値が `monotonic_ns` の間）を P 1・4、`-O0`・`-O3` で。既存の 14b 群（`def main :: Array<string> -> i32`）と `Env.args` が同じ配列を返す。

### 手順 5: ゲートと既存テストの更新

- 変更: `src/target.rs`（`os_apis`・`os_process` とテスト）、`src/driver.rs`（`Process.run` だけの `E2002`、`OS_WINDOWS_MESSAGE` と `os_message` の Windows の腕の削除）、`src/test_runner.rs`、`tests/targets.mjs`、`tests/windows.rs`、`tests/windows.mjs`。
- 内容: `tests/targets.mjs` は `x86_64-pc-windows-gnu` の `Env.var` がビルドでき、`Process.run` は `W11 Phase 2` の文で `E2002` になることを確かめる形にする。`tests/windows.rs` に `cross_links_the_os_runtime_with_microsoft_sdk_when_requested`。`tests/windows.mjs` に、名指しの MSVC の target でも早見のプログラムが動く小さな検査と、OS API に到達するプログラムの `--emit object` が `E2002` で、文が OS のランタイムを名指すことの検査。
- 確認: T1。`TSUZURI_WINDOWS_SDK=<SDK> cargo test --locked --test windows`（`running N tests` の N を確かめる）。

### 手順 6: CI

- 変更: `.github/workflows/windows.yml` の gate、`.github/workflows/vscode.yml` の Windows の段。
- 内容: windows.yml に「OS API end to end (W11)」の段 `node tests/os.mjs target/release/tsuzuri.exe`（x64 と Arm64。W4 の後で、W4 が失敗したら飛ばす）。vscode.yml の Windows の段に、同梱の MinGW で同じ suite（Net の段と同じく `TSUZURI_CLANG` に `toolchain/bin/tsuzuri-clang.exe`）。
- 確認: PR に `ci:full` を付け、`gh run watch <id> --exit-status`。赤なら `gh run view <id> --log-failed`。

### 手順 7: IR の不変・文書・T2（Phase 1 の完了）

- 変更: 「ドキュメント」の全ページ。
- 内容: 手順 1 の IR を作り直して比べる。Phase 1 の間は `process.md` に `E2002` と Phase 2 の予定を「計画」として分けて書く。
- 確認: 手順 1 の IR と `cmp` で byte 単位に一致する。`node scripts/check-docs.mjs <変更したページ>`。T2（`cargo nextest run --locked`、`node tests/os.mjs`・`tests/tzroutine.mjs`・`tests/windows.mjs` を開発版で、`sh scripts/check-runtime-includes.sh`）。

### 手順 8: Phase 2 の `Process.run`（人間が求めたときだけ）

- 変更: `os-windows.h` の `tz_os_spawn_now` と補助（解決、拒否、引用、`STARTUPINFOEXW`、名前付きパイプ、吸い出し、上限）。`tests/os.mjs` の 21 群の Windows の表。
- 内容: 仕様 4。
- 確認: 21 群（Node の子の往復、バッチの拒否、作業ディレクトリを探さないこと、同時の起動の順序）。

### 手順 9: Phase 2 のゲートの撤去

- 変更: `src/target.rs` の `os_process`、`src/driver.rs`・`src/test_runner.rs` の `PROCESS_WINDOWS_MESSAGE` とそのゲート、`tests/targets.mjs`。
- 内容: 未使用の定数を残さない（clippy）。
- 確認: T1。

### 手順 10: Phase 2 の文書と T2

- 変更: `process.md`・`platforms.md`・`os.md`・`docs/language.md` の Phase 2 の部分。
- 内容: 計画の記述を実際の振る舞い（PATH の探索、バッチの拒否、引用、`signal` は 0）に置き換える。
- 確認: `check-docs`、T2、`ci:full`。

## テスト計画

### Rust

- `src/target.rs` の `platform_properties_follow_the_target_table`: `os_apis` がすべての `Triple::ALL` で真、Phase 1 の `os_process` は Windows で偽。
- `tests/windows.rs` の `cross_links_the_os_runtime_with_microsoft_sdk_when_requested`: `File.read_text`・`Env.var`・`Time.monotonic_ns`・`Random.bytes` を使うプログラムの IR と、`os.c`（`os-windows.h` を置き換えた text）・`io.c` を 1 つにした runtime を、`-Wall -Wextra -Werror` で x86_64（リンクまで、`bcrypt.lib` を足す）と aarch64（object まで）に。
- `tests/os_api.rs` は変えない（IR が変わらないことの検査を含む）。

### E2E（`tests/os.mjs` を Windows でも走らせる）

- `process.platform === "win32"` の表を持つ。期待の code は手で書いた Win32 の値（GUIDE §1 規則 10。`os.constants.errno` は libuv の値なので使わない）。文書が値を決めていない場合（`Dir.list` をファイルに使ったときの 3 か 267 など）は、Win32 を直接呼ぶ小さな C の参照（`tests/os_windows_reference.c`、テストがコンパイルする）で求め、Tsuzuri のランタイムの出力を写さない。
- 1 群: 往復・追記・IO と一緒の連結（POSIX と同じ）。2〜6 群: 存在しない → `NotFound` 2、`tests/os_windows_reference.c` の `hold <path>`（`dwShareMode` 0 で開き、`ready` と出して標準入力が閉じるまで持つ）が開いているファイル → `PermissionDenied` 32（`chmod 000` の代わり。Node の `fs.constants` には共有なしで開く指定がなく、`0x10000000` を渡しても共有なしにならないことを起票時に確かめた）、`chmodSync(path, 0o444)` の読み取り専用 → 書き込みと `remove` が `PermissionDenied` 5、不正な UTF-8 → `InvalidEncoding` 0、ディレクトリの `read_bytes` → `InvalidInput` 0、NUL と孤立サロゲートのパス。
- 7・8 群: `Dir.list` の順（`readdirSync` を UTF-8 の `Buffer.compare` で並べた列）、空のパス、260 文字を超える深い階層のパス（D5）、junction（権限なしで作れる）が `link_metadata` で `Symlink`・`Dir.walk` でたどらない・`File.remove` で link だけ消える・`Dir.remove` で `NotFound`。symlink は `symlinkSync` が `EPERM` なら「skip: symlinks need Developer Mode」と表示して飛ばす（黙って成功にしない）。
- 9 群: `windowsVerbatimArguments: true` で生のコマンドラインを渡し、`Env.args` と `main` の引数が `arguments.c` の規則で手で求めた配列と一致（`a "b c" ""`、`"C:\dir\"`、`a"b`。[W33](W33-windows-command-line-splitting.md) が先に入っていれば、その規則で手で求める。このチケットが先なら、W33 の PR が期待値を直す）。`Env.var`（未設定、空の値、`A=B`）。`current_dir` は `realpathSync.native(cwd)`。
- 10・11 群: `sleep_ms 50` の前後の差が 50,000,000 以上（下限だけ。上限は見ない）、`unix_ns` と `Date.now()` の差が 60 秒以内、`Random` の長さと失敗。
- 17・19・20 群: ハンドル（二重 close・古いハンドル）、メタデータは Node の `statSync(path, { bigint: true })` の `mtimeNs`・`size` と比べる、`Dir.walk`。
- 16 群（ASan・UBSan と確保の追跡）は Windows では確保の追跡（`live == 0`）だけを sanitizer なしで走らせる（UBSan の Windows の対応は未確認）。
- 21 群（Phase 2）: 子は Node（`process.execPath -e "process.stdout.write(JSON.stringify(process.argv.slice(1)))"`）で、`["a b", "", "c\"d", "e\\", "f\\\\\"g", "tab\t", "\u00fc"]` が往復する。標準入力と 2 つの出力、終了コード 3、`process.exit(-1)` 相当の大きな値のビット、2^30 + 1 byte の出力 → `Other`。`x.bat`・`X.CMD`・`x.bat.`・`x.bat ` と、PATH に `t.bat` しかない `t` → `InvalidInput`・`NotFound` で、バッチが作るはずの印のファイルが無い。作業ディレクトリにだけある `evil.exe` → `NotFound`。2 つの tzroutine（`TSUZURI_MAXPROCS=2`）が同時に、3 秒待つ子と即座に終わる子を起動し、即座の方の結果が先に届く（順序だけを見る。継承の漏れがあると後になる）。

### 既存テストへの影響

- `src/target.rs` の `platform_properties_follow_the_target_table`（Windows の `os_apis`）と `tests/targets.mjs` の G10 の文の検査は、このチケットの正当な変更として直す。`tests/tzroutine.mjs` の `win32` の除外を外す。ランタイム入りの COFF object の汎用の `E2002` の文に OS のランタイムを足す（`tests/windows.mjs` は今は code だけを見ているので、文の検査を足す）。それ以外の期待値は変えない（S3）。

## ドキュメント

- 探す: `grep -rnE "E2002|OS API|os\.c|G10|Windows" _tsuzuri/language-reference/built-in-types-and-modules _tsuzuri/language-reference/compiler _tsuzuri/language-reference/languages`。
- `os.md`（「WASM と Windows」を「WASM」と「Windows」に分け、code が `GetLastError` の値であることと対応表）、`file.md`・`dir.md`（読み取り専用、共有違反、長いパス、junction）、`env.md`（`Env.args` の Windows の分割は `main` と同じ）、`time.md`、`random.md`、`process.md`（Phase 2: PATH の探索、`.bat`・`.cmd` の拒否と `cmd.exe` の書き方、引用の規則と Tsuzuri の子の制限、`signal` は 0。Phase 1 の間は `E2002`）、`path.md`（`current_dir` の `\`）、`test.md`、`compiler/platforms.md`（表と診断）、`diagnostics.md`、`native-interop.md`、`usage.md`、`languages/strategy.md`・`why-tsuzuri.md`・`how-about-tsuzuri.md`。
- `docs/language.md` の「OS API」と「WASM と Windows」、`docs/architecture.md` の OS のランタイム（`os-windows.h`、`arguments-split.h`、リンクする `bcrypt`）。README の機能の一覧と検証のコマンドが Windows の OS API を `E2002` と書いていれば直す。

## 受け入れ条件

- [ ] Phase 1: Windows（MSVC と MinGW、x64・Arm64）で `File`・`Dir`・`Env`・`Time`・`Random` が仕様 2 のとおりに動き、`tests/os.mjs` の Windows の表が `-O0`・`-O3` で通る。`Process.run` に到達するビルドは仕様 1 の `E2002`。
- [ ] tzroutine の中の `Time.sleep_ms` が G だけを止め、`Time.after` の値が `monotonic_ns` の間にある（`tests/tzroutine.mjs` の Windows）。
- [ ] `Env.args` と `def main :: Array<string> -> i32` が Windows で同じ配列を返す（D4 の承認後）。
- [ ] 手順 1 の IR（native・wasm32・Windows の target）が byte 単位で一致する。POSIX・WASI の `tests/os.mjs`・`tests/os-wasi-host.mjs` の結果が変わらない。
- [ ] `tests/windows.rs` の新しいテストが `TSUZURI_WINDOWS_SDK` で通る。`windows.yml` と `vscode.yml` に段がある。
- [ ] OS API に到達する Windows の `--emit object` は、OS のランタイムを名指す `E2002` のまま（汎用の文に OS を足した）。
- [ ] Phase 2（求められたとき）: 21 群が通り、`.bat`・`.cmd` は子を作らずに `InvalidInput`、作業ディレクトリを探さない。
- [ ] 文書を直し、`check-docs` が通る。T2 が通る。PR に `ci:full` を付けた。

## 落とし穴

- `os.c` は連結の先頭にある。Windows の側で `WIN32_LEAN_AND_MEAN` なしに `<windows.h>` を include すると `<winsock.h>` が入り、後の `net.c` の `<winsock2.h>` と衝突する。マクロは未定義のときだけ定義する（G10 の `-Werror` の再定義と同じ失敗）。
- COFF には弱いデータがない。`tsuzuri test` は各部分を別々にコンパイルするので、`tsuzuri_os_sleep_hook` を `os.c` と `tzroutine.c` の両方で `selectany` にしないと二重定義か未定義になる。
- `CREATE_ALWAYS`・`OPEN_ALWAYS` の成功の後の `GetLastError` は 183。`GetEnvironmentVariableW` の 0 は空の値かもしれない。`FindFirstFileExW` は空のドライブの根で `ERROR_FILE_NOT_FOUND` を返す。
- `Dir.list ""` を `"\*"` にすると、今のドライブの根を一覧してしまう。
- `Sleep` は分解能より短い値で早く戻りうる。期限を `monotonic_ns` で見て繰り返す。`monotonic_ns` を `tzr_nanotime` と違う式にすると、`Time.after` の値の検査が落ちる。
- `FILE_FLAG_BACKUP_SEMANTICS` はメタデータを開くときだけ（ディレクトリを開くのに要る）。読み書きの `CreateFileW` には付けない（バックアップの特権を持つプロセスで ACL を飛ばす）。
- Phase 2: `CreateProcessW` に `lpApplicationName` を NULL で渡すと、作業ディレクトリと実行ファイルのディレクトリを先に探す。継承するハンドルを `PROC_THREAD_ATTRIBUTE_HANDLE_LIST` で絞らないと、同時に走る別の `Process.run` の子がパイプの書き込み端を持ち続け、EOF が来ずに止まる。無名パイプ（`CreatePipe`）は重なった I/O ができない。`.bat` の判定は末尾の `.`・空白を取り除いてから（CVE-2024-43402）。
- Tsuzuri の子（`arguments.c`）は `"` を引数の文字として受け取れない（引用の状態を反転するだけ）。Node の子で引用を確かめ、Tsuzuri の子の制限は文書に書く。
- Windows の CI の stderr の先頭には警告の JSON 行が出ることがある。診断は最後の JSON 行を読む（`tests/windows.mjs` の `diagnostic`）。

## 対象外

- `def main :: Array<string> -> i32`（`arguments.c`）の分割の規則を CRT の規則に揃えること（`env.md` と `docs/language.md` の約束の変更。[W33](W33-windows-command-line-splitting.md) が要承認で扱う）。
- Windows を考えた `Path`（`\` やドライブ名の分解）、byte 列・UTF-16 のパスの API、ファイルの監視・ロック・ACL、環境変数の設定。
- `Process.run` の `cmd.exe` 用の自動のエスケープ、子のウィンドウを隠す指定、標準入出力を継がせる起動。
- `Os.ErrorKind` の case の追加、WASI preview2。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | Windows の実装は `src/runtime/os-windows.h` に置き、`os.c` の共通部分の後で `#if defined(_WIN32)` から取り込む。driver は `#include` を中身で置き換える | `tzroutine-windows.h` の前例。状態値・`tz_os_deliver`・整列・フックを 1 か所に保ち、POSIX の text を移動だけにする。別の `os-windows.c` は共通部分を二重に持つ | 既定案（実装者はこの案に従う） |
| D2 | 組み込みの ABI（UTF-8 の byte と長さ）を変えず、ランタイムが UTF-16 に変換する（E08 Phase 2 の「UTF-16 のまま」を改める） | IR と std を target ごとに分けずに済み、S2 を守れる。WASI の経路も同じ。変換の費用は OS 呼び出しに比べて小さい（計測はしない） | 既定案（実装者はこの案に従う） |
| D3 | `GetLastError` の対応は E08 の表に 206 を足したもの | `ENAMETOOLONG` と同じ扱い。kind は増やさない（G19 の D8） | 既定案（実装者はこの案に従う） |
| D4 | Windows の `Env.args` は `arguments.c` と同じ分割（`arguments-split.h`）で `GetCommandLineW` を分ける。E08 Phase 2 の `CommandLineToArgvW` は採らない | `env.md` と `docs/language.md` が「`main` の引数と同じ個数」と「バックスラッシュは特別でない」を約束している。`CommandLineToArgvW` はその約束を破り、`shell32` も要る。CRT の規則への統一は利用者に見える変更なので対象外で、[W33](W33-windows-command-line-splitting.md)（要承認）が扱う。W33 の後は、この理由は「UCRT の規則を 1 つの C の実装で持つ。`CommandLineToArgvW` は 3 つの場合で UCRT と違い、`shell32` が要る」に読み替わり、決定（`CommandLineToArgvW` を採らない、`arguments.c` と同じ関数）は変わらない（W33 の設計 3）。承認前に着手しないのは手順 4 の `Env.args` の段だけ（着手条件） | 要承認（承認前は該当する段に着手しない） |
| D5 | 248 単位以上のパスは `GetFullPathNameW` の後に `\\?\` を付ける | 長いパスを使えるようにする設定（マニフェストの `longPathAware` とレジストリ）は既定で無効。短いパスは OS の解釈のまま渡すので、D13 の結果は変わらない | 既定案（実装者はこの案に従う） |
| D6 | 開くときの共有は `READ \| WRITE \| DELETE` | POSIX の開き方に最も近く、開いている間も別のプロセスが読み書き・削除できる | 既定案（実装者はこの案に従う） |
| D7 | junction と symlink は `link_metadata` で `Symlink`、`File.remove` は link だけを消し、`Dir.remove` は `NotFound`（code 0） | E08 D13 の結果を Windows でも同じにする。`Dir.walk` が junction の環（`Application Data` など）をたどらない | 既定案（実装者はこの案に従う） |
| D8 | `Process.run` は PATH だけを探し、`.bat`・`.cmd` は `InvalidInput` で拒否する | 「shell を介さない」約束を Windows でも守る。`cmd.exe` を暗黙に通さない（GUIDE §1 規則 2）。Node.js も同じ問題（CVE-2024-27980）で、shell の指定なしのバッチの起動を拒否にした。Rust のように cmd 用にエスケープする案は、規則が複雑で不完全な修正の前例（CVE-2024-43402）があるので採らない | 既定案（実装者はこの案に従う） |
| D9 | 子のコマンドラインは MSVC の CRT（2008 以降）と `CommandLineToArgvW` の規則で引用する | Windows のほとんどのプログラムがこの規則で分ける。Node の子で往復を確かめられる | 既定案（実装者はこの案に従う） |
| D10 | パイプは親の端を重なった I/O の名前付きパイプにし、1 本のスレッドで `WaitForMultipleObjects` | 無名パイプは重なった I/O ができない。補助のスレッド 2 本の案は、上限での打ち切りと後片付けが複雑になる。Rust の標準ライブラリと同じ方式 | 既定案（実装者はこの案に従う） |
| D11 | `Output.code` は終了コードのビット、`signal` は 0 | Windows に signal はない。NTSTATUS の終了（トラップの `0xC000001D` など）もそのまま見える | 既定案（実装者はこの案に従う） |
| D12 | 乱数は `BCryptGenRandom`（`BCRYPT_USE_SYSTEM_PREFERRED_RNG`）、`bcrypt` をリンクする | E08 の設計方針どおりで、文書化された安定した API。`ProcessPrng` は失敗しないが、別の DLL への依存になり、E08 の方針を改める理由がない | 既定案（実装者はこの案に従う） |
| D13 | Phase 1 と Phase 2 に分け、Phase 1 の間は `Process.run` だけを `E2002` にする | `Process.run` が最も危険（引用・バッチ・継承・吸い出し）で、ほかを待たせない。実行時の `Other` で隠さず、ビルドで拒否する | 既定案（実装者はこの案に従う） |
