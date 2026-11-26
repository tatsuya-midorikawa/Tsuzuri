# M19: GUI として起動したときのトラップとランタイムの報告を unified logging（`os_log`）へ送る

| 項目 | 内容 |
| --- | --- |
| ID | M19 |
| 優先度 | P2（回避策はある: `main` の先頭で `extern` の `creat` と `dup2` を呼び、標準エラーをファイルへ向ける（起票前の調査で、Tsuzuri だけで動くことを確かめた）。報告が消えるのは、Finder・Dock・`open`・launchd から起動して標準エラーが `/dev/null` のときだけで、端末からの起動と `tsuzuri run` では消えない） |
| 規模 | M |
| 依存 | – |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-26（D-67。利用者の依頼「「どのチケットにもない壁」と「調査中に見つけたコンパイラの不具合（自分で再現済み）」のそれぞれについて @_features/ に開発チケットまたは修正チケットを作成して」から起票。調査で報告した「配布 4（Finder から起動すると stderr は /dev/null で、トラップ・デッドロック・スタックのあふれの理由が消える。W04・W32 は POSIX の代替の送り先を退けている）」に当たる。HEAD `804b7970` で確認） |
| 承認 | 要承認: D-67 の分岐 7（W04 の対象外（W04:178）と W32 の 207・331 行が退けた POSIX の代替の送り先を、macOS に限って足す。D1）と subsystem の名前（D2。D-55 の外の文字列だが、利用者が `log` で引く名前なので決定事項に書く）。D3〜D9 は既定案。承認が記録されるまで、このチケットに着手しない。台帳は [GUIDE D-67](GUIDE.md#d-67-tsuzuri-だけで-macos-のデスクトップアプリを書くm01m22) |
| 改善する劣位 | Objective-C・Swift（`NSLog`・`os_log` の文は、Finder から起動しても unified logging に残り、Console.app と `log show` で読める）比と、Windows の Tsuzuri（[W04](_completed/W04-windows-trap-report-lost.md) が標準エラーに書けない報告を `OutputDebugStringW` へ送る）比: Finder・Dock・`open` から起動した Tsuzuri のアプリでは、標準エラーが `/dev/null` で、トラップ・デッドロック・スタックのあふれの理由の文が消える。クラッシュレポートには `EXC_BREAKPOINT`（SIGTRAP）しか残らない |
| 手本にする既存実装 | [W04](_completed/W04-windows-trap-report-lost.md)（`src/llvm_traps.rs` の `TRAP_WRITE_WINDOWS`、`src/llvm.rs` の `windows_abi` の置き換えと宣言の重複の防止、`src/runtime/tzroutine-windows.h` の `tzr_report`）、`src/driver.rs` の `stack_runtime`・`backtrace_runtime`（実行ファイルにだけ C のランタイムを足す。`backtrace_runtime` は IR の宣言を見る）と `compile_runtime_object`（ランタイムの object のキャッシュ）、`src/runtime/stack.c` の `tsuzuri_stack_scheduler_thread`（`hidden` の内部の `tsuzuri_` symbol）、`tests/stack_overflow.mjs`（実行ファイルを作って標準エラーを調べる E2E） |
| 主な影響ファイル | `src/runtime/oslog-darwin.c`（新規）、`src/runtime/tzroutine.c`、`src/runtime/stack.c`、`src/runtime/cpu.c`、`src/llvm_traps.rs`、`src/llvm.rs`、`src/driver.rs`、`scripts/toolchain/runtime.mjs`、`tests/macos_report.mjs`（新規）、`.github/workflows/targets.yml`、`docs/architecture.md`、`docs/language.md`、`_tsuzuri/language-reference/compiler/platforms.md`、`_tsuzuri/language-reference/compiler/debugging.md`、`_tsuzuri/language-reference/compiler/native-interop.md` |

## 実装者向け早見

- 範囲: Apple の target の native の実行ファイル（`build --emit exe`・`run`）が書く報告。(a) 生成コードの `@tz.trap.write`（`--trap-info` のトラップの文と、W12 のコールバックが `main` の後に呼ばれたときの報告）、(b) ランタイムの C の報告（tzroutine の `tzr_report` を通る 4 種と `tzr_async_lost`、`stack.c` のスタックのあふれ、`cpu.c` の CPU の版の失敗）。fd 2 が閉じている（`EBADF`）か `/dev/null` のときだけ、同じ文を unified logging（`os_log`）へ送る。
- 原因: どの報告も fd 2 に書くだけである。LaunchServices（Finder・Dock・`open`）が起動したプロセスの fd 2 は `/dev/null` で、文はどこにも残らない。W04 は Windows にだけ代わりの送り先を足した。
- 直し方: 新しいランタイムの C `src/runtime/oslog-darwin.c` に 2 つの `hidden` の関数を置く。`tsuzuri_report_to_log()` は最初の呼び出しで fd 2 を 1 回調べて覚える。`tsuzuri_report_log(category, text, length)` は subsystem `dev.tsuzuri.runtime`、category `trap`・`runtime`、`OS_LOG_TYPE_ERROR`、`%{public}.*s` で送る。Apple の実行ファイルの IR では `@tz.trap.write` の先頭にこの判定を足し、ランタイムの C は `TZ_REPORT_OSLOG` を定義したときだけ同じ判定を通る。どちらも、判定が偽なら今と同じ書き込みをする。
- 最初に読む: 「現状」の「報告の書き手」→ `src/llvm_traps.rs:931` の `TRAP_WRITE` → `src/llvm.rs:237-249` の `windows_abi` の置き換え → `src/driver.rs:4196-4204`（`windows_abi` を当てる所）・`4246-4372`（ランタイムの object を選ぶ所）→ `src/runtime/tzroutine.c:274-289`（`tzr_report`・`tzr_fatal`）→ `src/runtime/stack.c:38-59`。
- 最初に書くテスト: `src/llvm.rs` の単体テスト `darwin_trap_report_asks_the_runtime`（IR の置き換えの形）→ `tests/macos_report.mjs` の `2>/dev/null` と閉じた fd 2 の段。
- 触ってはいけない: 端末・パイプ・ファイルへ書くバイト列とその順序（Apple を含むすべての OS）。Linux・Windows・WASM の IR とランタイムの C の出力。`--emit object`・`shared`・`llvm`・`bitcode` の出力。報告の文、終了コード、トラップの終わり方。`TSUZURI_TRAP_REPORT` のクラッシュレポート。`Debug.print`・`IO.*`・`--overflow-report`。
- 完了の見分け方: 次のプログラムを `--trap-info` で作った実行ファイル `m19trap` を `./m19trap 2>/dev/null & pid=$!; wait $pid` で動かすと、終了コードは今と同じ 133 で、`/usr/bin/log show --predicate "subsystem == \"dev.tsuzuri.runtime\" AND processID == $pid" --last 1m --style compact` に `[dev.tsuzuri.runtime:trap] trap: integer division by zero at …/Main.tz:1:34` の 1 行が出る（今は何も出ない）。zsh では `log` が組み込みのコマンドなので、`/usr/bin/log` と書く。プロセスの名前（`process ==`）では引かない（「落とし穴」）。

```tsuzuri
def divide :: i64 -> i64 = \x -> 10 / x

def main :: Array<string> -> i32 = \args ->
    (divide args.length) as i32
```

## 目的

macOS の GUI のアプリは、Finder・Dock・`open` から起動される。そのとき LaunchServices は標準エラーを `/dev/null` にする。launchd のエージェントも、`StandardErrorPath` を書かなければ同じである。Tsuzuri のトラップ、tzroutine のランタイムの致命的な報告（デッドロック、閉じたチャンネルへの送信、負の `WaitGroup` など）、スタックのあふれは、どれも標準エラーに書くだけなので、この状態では理由の文が消える。OS のクラッシュレポートに残るのは `EXC_BREAKPOINT`（SIGTRAP）などの例外の種類だけで、利用者も開発者も、どの行のどの理由で終わったかを知る手段がない。

macOS の標準の記録先は unified logging（`os_log`）である。Objective-C の `NSLog` と Swift の `os_log`・`Logger` はそこへ書き、Console.app と `log show` で読める。このチケットは、標準エラーに書いても誰も読めないとき（閉じているか `/dev/null`）に限って、同じ報告の文を unified logging へ送る。端末から動かしたとき、パイプやファイルへ向けたときの出力は 1 byte も変えない。W04 が Windows で `OutputDebugStringW` へ送るのと同じ考え方を、macOS の標準の経路で行う。

## 着手条件と停止条件

### 着手条件

- D-67 の分岐 7（D1）と subsystem の名前（D2）の承認が [GUIDE D-67](GUIDE.md#d-67-tsuzuri-だけで-macos-のデスクトップアプリを書くm01m22) の「利用者が決めた点」に記録されている。承認が記録されるまで、このチケットに着手しない。承認で名前が変わったら、このチケットの名前を直してから始める。
- `node scripts/repo-kb.mjs brief M19`。P2 のチケットなので、着手前にその時点のコードと台帳に対してレビューを行い、指摘を反映してから実装する（GUIDE §0 の 2）。
- 「現状」の再現を HEAD で繰り返し、同じ結果になることを確かめる。行番号は `grep -n` で取り直す。
- macOS（arm64）の開発機で、`/usr/bin/log show` が sudo なしで自分のプロセスの記録を読める（起票時の機械では読めた）。
- CI の macOS のランナー（`targets.yml` の `macos-15`）で `log show` が使えるかを、最初の PR の試しの段で確かめる。使えなければ、テスト計画の「差し替えの口」で判定を確かめ、`log show` の段は手元の確認にする（S4）。
- 同梱の zig のツールチェーン（macOS のホストと、Linux のホストからの `--target aarch64-apple-darwin`）が `<os/log.h>` を含めて `src/runtime/oslog-darwin.c` をコンパイルできるかを確かめる。起票時は確かめていない（この機械に同梱のツールチェーンがなかった）。できなければ S3。
- [W32](W32-silent-write-failures.md) の状態を見る。W32 は Windows の `--overflow-report` を W04 の書き手へ送る。このチケットは `src/llvm_overflow.rs` を変えないので、どちらが先でもよい。

### 停止条件

- **S1** 端末・パイプ・ファイルへ書く報告のバイト列か順序が、どの OS でも 1 byte でも変わる。Linux・Windows・WASM の `--emit llvm` とランタイムの C の出力、Apple の `--emit object`・`shared`・`llvm` の出力が変わる（`tests/fixtures`・`examples` の `Main.tz`、`-O0`・`-O3`）。
- **S2** シグナルの中（スタックのあふれ、`tzr_async_lost`）の `os_log` が、テストで止まる（終わらない）か、64 KiB の代替スタックをあふれる。即興でシグナルの中の送り先を外さず、計測の結果を添えて報告する（D8 の代案を人間に諮る）。
- **S3** 同梱の zig のツールチェーンで `<os/log.h>` をコンパイルできず、事前ビルドの object（`scripts/toolchain/runtime.mjs`）でも賄えない。
- **S4** CI の macOS のランナーで `log show` が使えず、差し替えの口でも判定を確かめられない。
- **S5** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `804b7970` で確認）

macOS 27.2.1（arm64）、Apple clang 21、SDK 27.2、開発版 `target/debug/tsuzuri` で確かめた。作業は `target/scratch/m19/` で行い、確認後に消した（実験で作ったクラッシュレポートも消した）。

### 報告の書き手

| 報告 | 書き手（HEAD の行） | 書き方 | 既定の `build` で出るか |
| --- | --- | --- | --- |
| トラップの文（`trap: <理由> at <パス>:<行>:<列>`） | `src/llvm_traps.rs` の `@tz.trap.report` が地点ごとに `@tz.trap.write` を呼ぶ（777・851 行）。`@tz.trap.write` の本体は定数 `TRAP_WRITE`（931 行）で、804-805・856-857 行で足す | `@write(i32 2, …)` を進まなくなるまで繰り返す | 出ない。`--trap-info` か `run` のときだけ（platforms.md:167） |
| コールバックが `main` の後に呼ばれた（W12） | `src/llvm.rs:2238-2259` の `@tz.callback.after_main` が `@tz.trap.write` を呼ぶ。`--trap-info` がなければ 2257-2259 行で `TRAP_WRITE` と `declare i64 @write(i32, ptr, i64)` を足す。条件は、コールバックがあり `guard_callbacks_after_main`（2133 行の `!wasm && entry == Entry::Console`）が真 | 同じ | 出る（native のコンソールの入口でコールバックを持つ実行ファイル） |
| tzroutine の致命的な報告・デッドロック・同期の型の誤り・anchor のあふれ | `src/runtime/tzroutine.c:274-279` の POSIX の `tzr_report`。呼ぶのは `tzr_fatal`（281-289 行）、`tzr_report_deadlock`（1105-1141 行）、`tzr_anchor_overflow`（2236-2240 行）、`tsuzuri_tz_report`（2908-2928 行） | `fwrite(…, stderr)` と `fflush(stderr)` | 出る |
| 先取りした tzroutine を戻せない | `src/runtime/tzroutine.c:2518-2522` の `tzr_async_lost` | `write(2, …)` の後に `abort()` | 出る |
| スタックのあふれ | `src/runtime/stack.c:45・55`（SIGSEGV・SIGBUS のハンドラー `tz_stack_fault`。64 KiB の代替スタックの上） | `write(2, "trap: stack overflow\n", 21)` の後に `abort()` | 出る（再帰か tzroutine のある実行ファイル。`src/driver.rs:4352-4355`） |
| CPU の版が使えない | `src/runtime/cpu.c:105-108` の `tz_cpu_unavailable` | `fputs(…, stderr)` の後に `abort()` | 出る |

- Windows では、`windows_abi`（`src/llvm.rs:237-249`）が `TRAP_WRITE` を W04 の `TRAP_WRITE_WINDOWS` に置き換える（IR にちょうど 1 回あることを `assert_eq!` で確かめる）。tzroutine は `tzroutine-windows.h` の `tzr_report` を使う。
- `src/runtime/backtrace.c` の `tsuzuri_trap_backtrace`（94-115 行）は、`TSUZURI_TRAP_REPORT` がファイルを指すときだけ、そのファイルへ frame のアドレスの 1 行を書く。標準エラーには書かない。
- `grep -rli 'os_log\|syslog' src/runtime` は 0 件である。
- ランタイムの C の object は、IR の名前から要るものだけを連結する（`src/driver.rs:4246` の `RuntimeSymbols::collect`、4250-4251 行の tzroutine、4259 行の cpu、4352-4355 行の stack、4371-4372 行の backtrace）。tzroutine・cpu・io などは 1 つの unit（`native-c`）に連結した C の文である（4856-4897 行）。`-DTZ_STACK_GUARD` などはこの unit に付ける（4918-4920 行）。事前ビルドの object とは、unit の名前・文のハッシュ・引数の列がすべて一致するときだけ照らす（`src/runtime_cache.rs` の `load_prebuilt`）。`scripts/toolchain/runtime.mjs` は cpu と io の組み合わせ（mask 2・4・6）だけを事前ビルドする。
- stack.c と backtrace.c は `native-c` に入らない。`-g` がなければ、最後の Clang の実行に `-x c -std=c11` の入力として渡し、生成コードと一緒にコンパイルする（5336-5351 行）。`-g` のときだけ自分の object（unit `stack-debug`・`backtrace-debug`。`compile_runtime_object` のキャッシュ。5005-5063 行）にし、DWARF の側ファイルを作る macOS ではリンカーに渡す（5423 行からの枝の 5447-5452 行）。
- 起票前の判断: W04 の対象外（W04:178）は「POSIX で標準エラーが閉じているときの届け先（`syslog` など）」を外した。W32 は 207 行（採らない案）と 331 行（対象外）で POSIX の送り先を退け、W32 の D3（342 行）は Windows の `--overflow-report` だけを変えて POSIX と WASM を変えない。W04 の S2 も POSIX と WASM の IR を守った。

### 再現: Finder と同じ起動で報告が消える

1. LaunchServices の起動の fd 2: 最小の C の `.app`（`LSUIElement` の Info.plist と、`fcntl(2, F_GETPATH)`・`getppid()` を絶対パスのファイルへ書く実行ファイル）を `open -W ./Probe.app` で起動すると、`F_GETPATH=0 path=/dev/null ppid=1` を書いた。
2. Tsuzuri の実行ファイル: 早見のプログラムを `tsuzuri build trap --trap-info -o Trap.app/Contents/MacOS/Trap` で作り、同じ Info.plist（名前だけ変えた）で `.app` にした。

```text
$ ./Trap.app/Contents/MacOS/Trap; echo $?
trap: integer division by zero at trap/Main.tz:1:34
133
$ ./Trap.app/Contents/MacOS/Trap 2>/dev/null; echo $?
133
$ open -W ./Trap.app
Unable to block on application (GetProcessPID() returned 18446744073709551016)
$ /usr/bin/log show --predicate 'process == "Trap" AND eventMessage CONTAINS "division"' --last 1m --style compact
Timestamp               Ty Process[PID:TID]
```

   `open -W` の文は、プロセスが起動の直後に終わったので待てなかったことを示す。ReportCrash は 3 つのクラッシュレポートを作り、親が `launchd` のもの（`open` の起動）の `exception` は `"type":"EXC_BREAKPOINT","signal":"SIGTRAP"` で、`division` の文字列はどこにもなかった。
3. 既定の `build`（`--trap-info` なし）でも出る報告: `let group = WaitGroup.create ()` / `WaitGroup.done (ref group)` / `0` の実行ファイルは、端末では `Tsuzuri runtime: negative WaitGroup counter` を書いて 133 で終わり、`2>/dev/null` では何も残さず 133 で終わった。`def rec depth` の深い再帰（`tests/stack_overflow.mjs` の `deep`）は `trap: stack overflow` を書いて 134 で終わった。`extern "atexit"` にコールバックを渡す次のプログラムの既定の実行ファイルは、`strings` で `a callback was called after main returned` を持ち、`_write` を参照していた（W12 の報告の経路が既定の `build` にもある）。レビューで動かすと、`main` の後に `atexit` がコールバックを呼ぶので、端末では `Tsuzuri runtime: a callback was called after main returned; stop the timers, threads and windows that call it before main returns` を書いて 133 で終わり、`2>/dev/null` では何も残さず 133 で終わった。`--emit llvm`（入口が `Entry::Console` でない）の IR には `@tz.trap.write` も `@tz.callback.after_main` もない。

```tsuzuri
@trusted
extern "atexit" def c_atexit :: (unit -> unit) -> i32

def bye :: unit -> unit = \() -> ()

def main :: unit -> i32 = \() ->
    let _r = c_atexit bye
    0
```

4. 回避策（起票前の調査で確かめた。起票時には繰り返していない）: `main` の先頭で `extern` の `creat` と `dup2` を呼んで fd 2 をファイルへ付け替えると、トラップの報告もそのファイルへ向かう。Info.plist の `LSEnvironment` は環境変数を足すだけで、標準エラーの行き先は変えられない。

### macOS の unified logging（起票時の実測）

C の試験プログラム（Apple clang、`-O1`）で、このチケットの設計に要る性質を確かめた。

- `fstat(2)` の判定: fd 2 が端末・パイプ・ファイルなら「使える」、`2>/dev/null` なら `S_ISCHR` で `st_rdev` が `stat("/dev/null")` と同じ、`sh -c 'exec "$0" 2>&-'` で閉じた fd 2 なら `fstat` が `EBADF` だった。上の `.app` の起動でも `/dev/null` と判定した。
- `os_log_with_type(os_log_create("dev.tsuzuri.runtime", "trap"), OS_LOG_TYPE_ERROR, "%{public}s", msg)` を `2>/dev/null` と閉じた fd 2 で呼ぶと、`/usr/bin/log show --predicate 'subsystem == "dev.tsuzuri.runtime"' --last 3m --style compact` に `E  oslog[34955:60947e] [dev.tsuzuri.runtime:trap] m19-probe devnull 34934` の形で出た。`--style json` の `messageType` は `Error`、`subsystem`・`category`・`processImagePath`・`eventMessage` を持つ。sudo は要らなかった。プロセスの終了の直後の `log show` で読めた。
- `%s`（`public` なし）の文字列は `<private>` に伏せられた。
- `%{public}.*s` に長さを渡すと、NUL で終わらない文の一部（改行の手前まで）を送れた。
- 4000 バイトの文は、およそ 1000 バイトで切られ、末尾に `<…>` が付いた。
- 環境変数 `OS_ACTIVITY_MODE=disable` のプロセスの `os_log` は、何も残らなかった。
- `os_log` の直後に `__builtin_trap()`（終了コード 133）か `abort()`（134）で終わっても、文は残った。
- SIGSEGV のハンドラー（`SA_ONSTACK`、64 KiB の代替スタック）の中で、最初の `os_log_create` と `os_log_with_type` を呼び、深い再帰のあふれで動かすと、文が残って 134 で終わった（1 回の実測。`os_log` が非同期シグナル安全だとは SDK の `os/log.h` に書かれていない）。
- clang は `os_log_with_type` を、`os_log_type_enabled` の検査、12 バイトの引数の符号化（`%{public}s` は descriptor 34）、`_os_log_impl(ptr @__dso_handle, ptr %log, i8 16, ptr @"\01LOS_LOG0", ptr %buf, i32 12)` に展開する。書式の文字列は `section "__TEXT,__oslogstring,cstring_literals"` に置く。`OS_LOG_TYPE_ERROR` は `0x10`。`os_log_create` は macOS 10.12 から（`os/log.h` の `API_AVAILABLE`）。`fstat` は IR では `@"\01_fstat"` になる。
- `os_log_create`・`_os_log_impl`・`os_log_type_enabled`・`__dso_handle` は、追加の `-l` なしに libSystem から解決した。`-std=c11` でもコンパイルできた。

## 変えてはいけない意味

- 標準エラーが使えるとき（端末・パイプ・ファイル・ソケットなど、閉じていなくて `/dev/null` でないもの）に書くバイト列と、ほかの出力との順序。`tsuzuri run` はパイプを渡すので、`run` の `E2005` と `trap` の報告は変わらない。
- 報告の文、トラップの終わり方（`llvm.trap` の SIGTRAP、`abort()` の SIGABRT）と終了コード。
- Linux・Windows（W04 の経路）・WASM の出力。
- object・共有ライブラリを埋め込んだホストの標準エラーの扱い（ホストの責任）。
- `TSUZURI_TRAP_REPORT` のクラッシュレポートの行。

## 仕様

1. 対象は Apple の target（ビルド機の macOS の native と `--target aarch64-apple-darwin`）の実行ファイル（`build --emit exe` と `run`）だけである。`--emit object`・`shared`・`llvm`・`bitcode`、`tsuzuri test` と `bench` の実行ファイル、ほかの OS は今と同じ（D5）。
2. 報告を書く直前に、そのプロセスで初めてなら fd 2 を 1 回調べ、結果を覚える。`fstat(2)` が `EBADF` で失敗するか、fd 2 が文字デバイスで `st_rdev` が `/dev/null` と同じなら「記録へ送る」、それ以外（`fstat` のほかの失敗を含む）は「標準エラーへ書く」とする。
3. 「標準エラーへ書く」なら、今と同じ書き方で今と同じバイト列を書く。
4. 「記録へ送る」なら、標準エラーには書かず、`os_log` へ 1 件送る。subsystem は `dev.tsuzuri.runtime`（D2）。category は、生成コードの報告（トラップ、`main` の後のコールバック）とスタックのあふれが `trap`、tzroutine と CPU のランタイムの報告が `runtime`。型は `OS_LOG_TYPE_ERROR`。文は今の報告の文から末尾の改行を 1 つ除いたもので、`%{public}.*s` で送り、伏せない（D6）。
5. 送った後の終わり方は今と同じ（トラップ、`abort()`、報告だけで続くもの（デッドロックの報告の後、パークした擬似 G を起こしてトラップする）を含む）。
6. 文がおよそ 1 KB を超えると OS が切り、`<…>` を付ける。Tsuzuri の報告はこれより短い（いちばん長いデッドロックの報告で 640 バイトの領域）。
7. `OS_ACTIVITY_MODE=disable` などで OS が記録しないときは、何も残らない（今の `/dev/null` と同じ）。
8. 新しい環境変数・CLI・診断・manifest のキーは足さない。切り替えは自動である（D9）。

## 設計

### ランタイムの C（`src/runtime/oslog-darwin.c`、新規）

```c
/* M19: on Apple targets, the reports of an executable whose standard error is closed or /dev/null
   (launched by LaunchServices or launchd) go to unified logging; other destinations are unchanged. */
#if defined(__APPLE__)
#include <errno.h>
#include <os/log.h>
#include <stdatomic.h>
#include <stdint.h>
#include <sys/stat.h>

static _Atomic int32_t tz_report_state; /* 0 unknown, 1 standard error, 2 unified logging */

__attribute__((visibility("hidden"), cold))
int32_t tsuzuri_report_to_log(void) {
    int32_t state = atomic_load_explicit(&tz_report_state, memory_order_relaxed);
    if (state == 0) {
        struct stat fd2, null;
        state = 1;
        if (fstat(2, &fd2) != 0) state = errno == EBADF ? 2 : 1;
        else if (S_ISCHR(fd2.st_mode) && stat("/dev/null", &null) == 0 && fd2.st_rdev == null.st_rdev) state = 2;
        atomic_store_explicit(&tz_report_state, state, memory_order_relaxed);
    }
    return state == 2;
}

__attribute__((visibility("hidden"), cold))
void tsuzuri_report_log(int32_t category, const char *text, int64_t length) {
    if (length > 0 && text[length - 1] == '\n') length--;
    if (length > INT32_MAX) length = INT32_MAX;
    os_log_t log = os_log_create("dev.tsuzuri.runtime", category == 0 ? "trap" : "runtime");
    os_log_with_type(log, OS_LOG_TYPE_ERROR, "%{public}.*s", (int)length, text);
}
#endif
```

- `fstat`・`stat` は非同期シグナル安全な関数（POSIX）なので、判定はシグナルの中でもよい。2 つのスレッドが同時に初めて報告しても、同じ結果を書くだけである。
- `os_log_create` は同じ組に同じ物体を返し、解放しない（`os/log.h` の説明）。報告は一生に数回なので、呼ぶたびに引いてよい。
- 「判定」と「送信」を分けるのは、「標準エラーへ書く」側を呼び出し元の今のコード（`fwrite`・`fflush`、`write` の繰り返し）のまま残し、順序とバイト列を変えないためである。

### 生成コード（`src/llvm_traps.rs`・`src/llvm.rs`）

- `src/llvm_traps.rs` に `pub(crate) const TRAP_WRITE_DARWIN` を足す。`TRAP_WRITE` と同じ名前・引数で、先頭で判定し、偽なら `TRAP_WRITE` と同じ繰り返しに進む。

```llvm
define internal void @tz.trap.write(ptr %data, i64 %length) cold nounwind {
entry:
  %choice = call i32 @tsuzuri_report_to_log()
  %to_log = icmp ne i32 %choice, 0
  br i1 %to_log, label %log, label %loop
log:
  call void @tsuzuri_report_log(i32 0, ptr %data, i64 %length)
  ret void
loop:
  ; TRAP_WRITE の loop・write・advance・exit をそのまま（entry の分岐だけが違う）
}
```

- `src/llvm.rs` に `pub(crate) fn darwin_report(ir: String) -> String` を足す。`define internal void @tz.trap.write(` があれば、`TRAP_WRITE` がちょうど 1 回あることを `windows_abi` と同じく `assert_eq!` で確かめて `TRAP_WRITE_DARWIN` に置き換え、`declare i32 @tsuzuri_report_to_log()` と `declare void @tsuzuri_report_log(i32, ptr, i64)` を、同じ行がないときだけ 1 回足す。なければ何もしない。
- `src/driver.rs` の `windows_abi` を当てる所（4196-4204 行）の並びに、`platform.mach_o() && options.emit == Emit::Executable` のときだけ `darwin_report` を当てる。`RuntimeSymbols::collect`（4246 行）はその後で走るので、`tsuzuri_report_` の名前を見られる。`src/test_runner.rs` の実行ファイルには当てない（D5）。

### 連結（`src/driver.rs`）

- `report_runtime = platform.mach_o() && options.emit == Emit::Executable && (runtime_symbols.prefix("tsuzuri_report_") || tzroutine_runtime || stack_runtime || cpu_runtime)`。真なら `oslog-darwin.c` を 1 つの unit（名前 `report-darwin`）として `compile_runtime_object` でコンパイルし（キャッシュ）、連結する。tzroutine・stack・cpu のどれも連結しないプログラム（W12 の報告だけを持つ既定の `build` や、再帰のない `--trap-info` の実行ファイル）にも足す。
- 真なら、`-DTZ_REPORT_OSLOG` を次の 3 か所に足す。object・共有ライブラリのビルドでは足さないので、その出力は今と同じになる。
  - `native-c` の unit。ただし tzroutine.c か cpu.c を含むとき（`tzroutine_runtime || cpu_runtime`）だけで、`-DTZ_STACK_GUARD` の隣（4918-4920 行）に置く。io だけの unit に足すと、引数の列が事前ビルドの mask 4 の object と合わなくなり、同梱のツールチェーンが毎回コンパイルし直す。
  - `-g` のない stack.c（最後の Clang の実行。5336-5343 行）。
  - `-g` の stack.c の object（unit `stack-debug`。5036-5063 行）。
- `report-darwin` の unit も、`stack-debug` と同じく `--no-cache`（`options.cache` が偽）のときは直接コンパイルする枝を持つ。
- 事前ビルド: `scripts/toolchain/runtime.mjs` の darwin の束に、`report-darwin` と、`-DTZ_REPORT_OSLOG` を付けた cpu の mask（2・6）を足す。照合は引数の列の完全な一致なので、ドライバーが付ける順と同じ列で作る。足さないと、同梱のツールチェーンは実行ファイルのたびにそれらをコンパイルし直す（`<os/log.h>` が要る。着手条件と S3）。
- `oslog-darwin.c` を `include_str!` で埋め込むので、`sh scripts/check-runtime-includes.sh` を走らせる（`src/driver.rs` などの `include_str!("runtime/…")` のファイルが Git に入っていて無視されていないことを確かめる）。

### ランタイムの C の呼び出し側

`TZ_REPORT_OSLOG` を定義したときだけ、次の形を通る。定義しなければプリプロセッサーが消し、生成される object は今と同じ。

```c
#if defined(__APPLE__) && defined(TZ_REPORT_OSLOG)
int32_t tsuzuri_report_to_log(void);
void tsuzuri_report_log(int32_t category, const char *text, int64_t length);
#endif
/* ... */
#if defined(__APPLE__) && defined(TZ_REPORT_OSLOG)
    if (tsuzuri_report_to_log()) { tsuzuri_report_log(1, text, (int64_t)length); return; }
#endif
    /* 今のコード（fwrite と fflush、または write） */
```

- `tzroutine.c` の POSIX の `tzr_report`（category 1 = `runtime`）。`tzr_fatal`・`tzr_report_deadlock`・`tzr_anchor_overflow`・`tsuzuri_tz_report` は変えない。
- `tzroutine.c` の `tzr_async_lost`（category 1）。
- `stack.c` の `tz_stack_fault` の 2 か所（category 0 = `trap`）。
- `cpu.c` の `tz_cpu_unavailable`（category 1）。

### 費用

- 報告のない実行では何も増えない。判定は最初の報告のときだけ走る（プロセスが終わる直前）。constructor は足さず、起動時の syscall も増えない。
- Apple の実行ファイルのうち報告の書き手を持つものに、小さな object（関数 2 つ）が 1 つ増える。新しい dylib は要らない（libSystem）。性能の主張はしない。

### 採らない案

- **IR に直接 `os_log_create` と `_os_log_impl` を書く**（C の object を足さない）: object・ランタイムなしのプログラムにも効くが、clang が持つ引数の符号化（12 バイトの buffer、descriptor 34）と `__oslogstring` の section を、Tsuzuri のコンパイラが写して持つことになる。判定にも `fstat`（IR では `\01_fstat`）と `struct stat` の配置か、可変長の `fcntl(F_GETPATH)` が要る。C の object を足せない事情が出たとき（S3）の代案として残す。
- **`syslog(3)` へ送る**: IR から呼べる普通の関数だが、subsystem と category を持たず、`log show` で Tsuzuri の報告だけを引けない。W04・W32 が退けた POSIX の送り先と同じものになる。伏せ方も確かめていない。
- **いつも記録にも送る**（標準エラーに書けるときも）: 端末から動かしたときにも記録が増え、報告が二重になる。D-67 の分岐 7 の範囲を超える。
- **標準エラーへの書き込みが失敗したら（`EPIPE` など）記録へ送る**: 範囲は「閉じているか `/dev/null`」と決めた（D1）。パイプの相手が先に終わった場合は今の経路のまま。
- **環境変数で切り替える**: 自動の判定で足りる。名前の登録（D-55）も要らない。
- **クラッシュレポートに文を書き込む**（`__crash_info` の注釈）: 非公開の API に頼る。

## 前提とする他チケットのインターフェース

- なし。W04 の `windows_abi`（Windows）とは target が重ならない。M15（`.app` の生成）は要らない。手で組んだ `.app` も launchd のエージェントも、fd 2 が `/dev/null` なら同じく効く。

## 他チケットへ提供するインターフェース

- ランタイムの C の `tsuzuri_report_to_log()` と `tsuzuri_report_log(category, text, length)`（Apple の実行ファイルだけ。`hidden`）。後のランタイムの報告（M09 のメインスレッドの検査のトラップ、M17 の `@weak` の無い symbol の報告など）は、`@tz.trap.write` を通すか、この 2 つを通せば同じく記録へ届く。
- `TRAP_WRITE_DARWIN` と `darwin_report`。後で macOS の `--overflow-report` などに同じ規則を広げるときに使う。

## 実装手順

1. **変更**: なし（基準の記録）。
   **内容**: 着手条件を確かめる。HEAD の開発版で、`tests/fixtures`・`examples` の `Main.tz` を `--emit llvm`（`-O0`・`-O3`、native の macOS、`--target x86_64-unknown-linux-gnu`、`--target wasm32`）と `--emit object`（macOS）にして `target/scratch/m19/base/` に保存する。「現状」の再現の 2・3 の端末の出力も保存する。
   **確認**: 保存した数を控える。
2. **変更**: `src/runtime/oslog-darwin.c`、`src/driver.rs`（unit の連結）。
   **内容**: 「設計」の C と、`report_runtime` の条件。まだ誰も呼ばない。
   **確認**: T0。`sh scripts/check-runtime-includes.sh`。macOS で `-g` の `--trap-info` の実行ファイルを作り、`nm` で `_tsuzuri_report_to_log` が局所の symbol（`t`）として入る。`-g` のない実行ファイルでは、まだ誰も呼ばないので `-Wl,-dead_strip`（`src/driver.rs` の `dead_strip_args`。`-g` のない native の実行ファイルに付く）が消す（レビューで、呼ばれない `hidden` の関数が `-dead_strip` で消えることを C で確かめた）。手順 3 の後は `-g` なしでも残る。
3. **変更**: `src/llvm_traps.rs`・`src/llvm.rs`・`src/driver.rs`（IR の置き換え）。
   **内容**: `TRAP_WRITE_DARWIN`・`darwin_report` と、それを当てる所。
   **確認**: 単体テスト（テスト計画）。`cargo nextest run --locked`。早見のプログラムで `2>/dev/null` の記録の 1 行が出て、端末の出力が手順 1 と `cmp` で一致する。手順 1 の Linux・WASM・object の IR が一致する（S1）。
4. **変更**: `src/runtime/tzroutine.c`・`stack.c`・`cpu.c`、`src/driver.rs`（`-DTZ_REPORT_OSLOG`）、`scripts/toolchain/runtime.mjs`。
   **内容**: 「ランタイムの C の呼び出し側」と事前ビルド。
   **確認**: `node tests/tzroutine.mjs target/debug/tsuzuri`・`node tests/stack_overflow.mjs target/debug/tsuzuri` が今と同じに通る。`sh scripts/check-runtime-includes.sh`。Linux では新しい分岐をプリプロセッサーが消すので、container は要らない。CI の `suites`（Linux）が今と同じに通ることで確かめる。
5. **変更**: `tests/macos_report.mjs`（新規）、`.github/workflows/targets.yml`（macOS の job に 1 行）。
   **内容**: テスト計画の E2E。
   **確認**: `cargo build` の後に `node tests/macos_report.mjs target/debug/tsuzuri`。残ったプロセスがない（`pgrep` で確かめる）。
6. **変更**: 文書（「ドキュメント」）。
   **確認**: 変更したページの `node scripts/check-docs.mjs`。T1、T2。PR に `ci:full` を付け、`targets.yml` の macOS の job が通る。`target/scratch/m19/` を消す。

## テスト計画

### 単体（`src/llvm.rs` の `#[cfg(test)]`）

- `darwin_trap_report_asks_the_runtime`: `--trap-info` の IR（`instrument_with` の安定した番号と密な番号の両方、`-g` あり・なし、`-O0`・`-O3`）と、`--trap-info` なしで W12 の `@tz.callback.after_main` を持つ IR を `darwin_report` に通す。
  - `@tz.trap.write` の本体が `call i32 @tsuzuri_report_to_log()` を持ち、`TRAP_WRITE` の繰り返し（`call i64 @write(i32 2,`）も持つ。
  - 2 つの `declare` がそれぞれ 1 回だけある。2 回通しても同じ文字列（重ねない）で、2 回作った IR も同じ（決定的）。
  - `@tz.trap.write` を持たない IR は 1 byte も変わらない。
  - `darwin_report` を通さない Linux の IR に `tsuzuri_report_` がない。

### Rust（`tests/`）

- 既存の IR のテスト（`tests/windows.rs` の Windows の置き換え、`--emit llvm` を固定するもの）が変わらずに通る。`--emit llvm` は実行ファイルでないので置き換えない（仕様 1）。

### E2E（`tests/macos_report.mjs`、macOS だけ。ほかの OS では理由を出して飛ばす）

記録は子のプロセスの ID で引く。`spawnSync` の結果の `pid`（`sh -c 'exec "$0" 2>&-'` で閉じるときも、`exec` なので同じ ID）を使い、`/usr/bin/log show --predicate 'subsystem == "dev.tsuzuri.runtime" AND processID == <pid>' --last 2m --style json` で読み、見つかるまで 5 秒まで待ち直す。プロセスの名前（`process ==`）では引かない（「落とし穴」）。子の環境から `OS_ACTIVITY_MODE` を外す。期待する文はソースの位置から手で書く（GUIDE-LITE §2 規則 10）。

| 段 | 起動 | 期待 |
| --- | --- | --- |
| trap-pipe | 早見のプログラム（`--trap-info`、`-O0`・`-O3`）、標準エラーはパイプ | 標準エラーが今と同じ 1 行、終了コード 133、記録は 0 件 |
| trap-null | 同じものを `stdio: ["ignore", "pipe", "ignore"]`（fd 2 が `/dev/null`）で | 終了コード 133、記録がちょうど 1 件で、`category` が `trap`、`messageType` が `Error`、`eventMessage` が trap-pipe の行から改行を除いたもの |
| trap-closed | `sh -c 'exec "$0" 2>&-'` で fd 2 を閉じて | trap-null と同じ |
| trap-file | 標準エラーをファイルへ | ファイルの中身が trap-pipe の標準エラーとバイト単位で同じ、記録は 0 件 |
| runtime | `let group = WaitGroup.create ()` / `WaitGroup.done (ref group)` / `0`（既定の `build`）を fd 2 が `/dev/null` で | 終了コード 133、記録の `category` が `runtime`、文が `Tsuzuri runtime: negative WaitGroup counter` |
| overflow | `tests/stack_overflow.mjs` の `deep` と `routine`（tzroutine のスタック）を fd 2 が `/dev/null` で | 終了コード 134、記録の `category` が `trap`、文が `trap: stack overflow`。`deep` はちょうど 1 件、`routine` は 1 件以上（あふれた tzroutine ごとに書く。`tests/stack_overflow.mjs` の 76-77 行と同じ）。30 秒の時間制限の中で終わる（S2） |
| after-main | 「現状」の再現の 3 の `atexit` のプログラム（既定の `build`。C のホストは要らない）を fd 2 が `/dev/null` で | 終了コード 133、記録の文が `Tsuzuri runtime: a callback was called after main returned; stop the timers, threads and windows that call it before main returns`、category が `trap` |
| deadlock | W12 の `tests/fixtures/callback_lifetime`（`Main.tz` と `host.c`。`tests/tzroutine.mjs` の 629-650 行と同じ作り方）を引数 `deadlock` で、fd 2 が `/dev/null` で | 記録の `category` が `runtime`、文が `Tsuzuri runtime: all tzroutines are asleep - deadlock!` で始まる |

- `.app` の起動（`open -W`）の段は CI に入れず、手順 5 で手元で 1 回確かめる（LaunchServices の登録を CI に残さないため）。
- 差し替えの口（S4 のとき）: `oslog-darwin.c` に、`TZ_REPORT_TEST_LOG` を定義したときだけ `os_log` の代わりにそのパスのファイルへ category と文を追記する枝を置く。コンパイラはこの名前を定義しない（環境変数も CLI も足さない）。C の試験プログラム（たとえば `tests/macos_report_host.c`）が `src/runtime/oslog-darwin.c` を `-DTZ_REPORT_TEST_LOG=\"<パス>\"` で `#include` してコンパイルし、fd 2 をパイプ・ファイル・`/dev/null`・閉じた状態にして（判定を覚えるので、状態ごとに別のプロセスで）、`tsuzuri_report_to_log` の答えと、`tsuzuri_report_log` が書く category と文を確かめる。生成コードとの結び付きは単体テストで確かめる。

### 既存のテストへの影響

- `tests/stack_overflow.mjs`・`tests/tzroutine.mjs`・`tests/trap_*.mjs`・`tests/windows.mjs` はそのまま通る（標準エラーはパイプ）。Linux の CI の結果は変わらない。

## ドキュメント

- `_tsuzuri/language-reference/compiler/platforms.md` の「既知の制限」（167 行の Windows の段落の次）: macOS の実行ファイルは、標準エラーが閉じているか `/dev/null`（Finder・Dock・`open`・launchd から起動したとき）のとき、トラップの報告・tzroutine のランタイムの報告・スタックのあふれを unified logging へ送る。`log show --predicate 'subsystem == "dev.tsuzuri.runtime"'` か Console.app で読める。トラップの文は今と同じく `--trap-info` のときだけ出る。object と共有ライブラリは送らない。
- `compiler/debugging.md` の「トラップのバックトレース」（175 行から）に小節「Finder から起動したアプリの報告（macOS）」を足す。`log show` の例と、category（`trap`・`runtime`）、`OS_ACTIVITY_MODE=disable` では残らないこと、zsh では `/usr/bin/log` と書くこと。
- `compiler/native-interop.md` の 106 行（object を埋め込んだホストの Windows の報告の説明）に、macOS の object と共有ライブラリは unified logging へ送らず、ホストが標準エラーを扱うことを 1 文足す。
- `docs/architecture.md` の 440 行（W04 の段落）の後に、macOS の `TRAP_WRITE_DARWIN`・`oslog-darwin.c`・`TZ_REPORT_OSLOG` の段落を足す。
- `docs/language.md` の「トラップ位置」（1512 行から。1514 行の「ネイティブ環境における標準エラー出力レポーター」）に、macOS の実行ファイルで標準エラーが閉じているか `/dev/null` のときの送り先を 1 文足す。
- README は変えない（クイックスタート・特徴の一覧・CLI の概要に当たらない）。

## 受け入れ条件

- [ ] 仕様 1〜8 のとおりに動き、E2E の各段が `-O0`・`-O3` で通る。
- [ ] 端末・パイプ・ファイルへの出力、Linux・Windows・WASM の IR とランタイムの C の出力、Apple の object・`--emit llvm` の出力が手順 1 と同じ。
- [ ] 宣言が重複せず、Apple の実行ファイルの IR が決定的。
- [ ] `sh scripts/check-runtime-includes.sh`、変更したページの `check-docs`、T1、T2 が通る。PR に `ci:full` を付け、`targets.yml` の macOS の job が通る。

## 落とし穴

- **`/dev/null` は書き込みに成功する**: `write` の結果では判定できない。`fstat` で調べる。
- **`%s` は伏せられる**: `public` を付けないと `<private>` になる。報告の文は利用者の秘密を持たない（ソースのパスと理由だけ）ので伏せない（D6）。
- **NUL がない**: IR の報告の文（`@tz.trap.message.N`）は NUL で終わらない。`%{public}s` でなく、長さを渡す `%{public}.*s` を使う。
- **シグナルの中**: スタックのあふれは 64 KiB の代替スタックの上の SIGSEGV のハンドラーで報告する。`os_log` の非同期シグナル安全は文書にない。判定（`fstat`・`stat`）は安全な関数だけで行い、送信は E2E の overflow の段で測る。止まるなら S2。
- **zsh の `log`**: 組み込みのコマンドで、`log show` は `too many arguments` になる。テストと文書は `/usr/bin/log` と書く。
- **プロセスの名前で引かない**: unified logging は、記録の `process`・`processImagePath` を実行ファイルの Mach-O の UUID から引く。Tsuzuri の実行ファイルは決定的で、同じソースを別の名前で作っても UUID が同じになる（レビューで `dwarfdump --uuid` で確かめた）。名前を変えた同じ内容の実行ファイルの記録は、先に記録した名前で出て、`process == "<新しい名前>"` には当たらなかった（レビューで確かめた）。テストは `processID` で引く。
- **`OS_ACTIVITY_MODE=disable`**: Xcode の設定などで付いていると何も残らない。テストの子の環境から外す。
- **事前ビルドの文の一致**: `native-c` の unit は事前ビルドの object と文で照らす。`cpu.c` を変えると、事前ビルドの束を作り直すまで照合が外れる。`-DTZ_REPORT_OSLOG` の付いた版を束に足す。
- **`run` と `test` は効かない**: どちらも子の標準エラーをパイプで受けるので、判定は「標準エラーへ書く」になる。手で `./exe 2>/dev/null` として確かめる。

## 対象外

- Linux・Windows（W04 で済んでいる）。
- `--overflow-report`（警告で、トラップでない。Windows は W32 が扱う）、`Debug.print`・`Debug.trace`・`IO.*` の標準エラーへの出力、GPU のランタイムの診断の文（`gpu.c`・`gpu-vulkan.c`。状態を返す）。
- `TSUZURI_TRAP_REPORT` のクラッシュレポート（frame のアドレス）を記録へ送ること。
- OS のクラッシュレポートへの独自の書き込み（非公開の API）。
- 新しい環境変数・CLI のオプション（切り替えは自動）。
- 標準エラーへの書き込みがほかの理由（`EPIPE` など）で失敗したときの送り先。
- object・共有ライブラリを埋め込んだホストの報告（ホストの責任。D5）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | macOS に限り、fd 2 が閉じている（`EBADF`）か `/dev/null` のときだけ、報告を `os_log` へ送る。端末・パイプ・ファイルへの出力は 1 byte も変えない。退けた案: 送らない（今の W04・W32 の判断のまま、GUI のアプリの報告が消え続ける）と、「設計」の採らない案（いつも送る、`syslog`、書き込みの失敗で送る） | 誰も読めない状態だけを選ぶので、今読める報告を変えない。unified logging は macOS の標準の記録先で、権限も設定も要らない（W04 の D1 が `OutputDebugStringW` を選んだ理由と同じ）。W32 が POSIX の送り先を退けた理由「設定と権限が要り、報告の契約にも当たらない」（W32:207）と「POSIX には標準の経路が無い」（W32:342）は、macOS の unified logging には当たらない。文も終わり方も変えないので、報告の契約にも触れない | 要承認（D-67 の分岐 7。承認前はこのチケットに着手しない） |
| D2 | subsystem は `dev.tsuzuri.runtime`、category は `trap`（生成コードの報告とスタックのあふれ）と `runtime`（tzroutine と CPU のランタイム）。退けた案: `OS_LOG_DEFAULT`（subsystem がなく Tsuzuri の報告だけを引けない）、プログラムごとの subsystem（bundle の識別子を知る手段が要る） | 利用者が `log show --predicate` と Console.app で引く名前で、文書に書く。逆 DNS の形（`os_log_create` の説明）。D-55 の登録の外の文字列だが、変えると利用者の引き方が壊れる。逆 DNS の元のドメイン（`tsuzuri.dev`）の持ち主は起票時に確かめていないので、承認の判断に含める | 要承認（承認前はこのチケットに着手しない。名前が変わったら仕様・テスト・文書を直す） |
| D3 | 判定と送信はランタイムの C の新しいファイル `src/runtime/oslog-darwin.c` に置き、生成した `@tz.trap.write` はそれを呼ぶ。ほかのランタイムの C を連結しないプログラムにも、この小さな object を足す。退けた案: IR に `_os_log_impl` を直接書く（「設計」の採らない案。S3 のときの代案） | `os_log` はマクロで、引数の符号化を clang に任せられる。IR とランタイムの C が 1 つの実装を共有する。連結は stack.c・backtrace.c と同じ「IR が名前を持つ実行ファイルにだけ足す」形 | 既定案（実装者はこの案に従う） |
| D4 | 判定は最初の報告のときに 1 回だけ行い、結果を覚える。constructor は足さない | 報告のない実行の費用を 0 にする。`dup2` で fd 2 を付け替えるプログラムにも、最初の報告の時点の状態で答える | 既定案（実装者はこの案に従う） |
| D5 | 対象は Apple の実行ファイル（`build --emit exe`・`run`）だけ。object・共有ライブラリ・`--emit llvm`・`bitcode`・テストのランナーの実行ファイルは変えない | object を埋め込むホストは自分の標準エラーを持つ（stack.c・backtrace.c が実行ファイルにだけ入るのと同じ理由）。ランナーは子の標準エラーをパイプで受ける。出力が変わる範囲を最小にする | 既定案（実装者はこの案に従う） |
| D6 | 型は `OS_LOG_TYPE_ERROR`、書式は `%{public}.*s`、文は今の報告から末尾の改行を 1 つ除いたもの。退けた案: `OS_LOG_TYPE_FAULT`（「バグの存在を示す予期しない状態」の型だが、デッドロックや CPU の版の失敗も同じ経路を通り、分ける利点がない）、`%s`（`<private>` に伏せられる） | `ERROR` は保存され、`log show` の既定で読める（実測）。文はソースのパスと理由だけで、伏せる必要がない | 既定案（実装者はこの案に従う） |
| D7 | 対象の報告は、`@tz.trap.write`（トラップと W12 の `main` の後のコールバック）、`tzr_report` を通る tzroutine の 4 種、`tzr_async_lost`、スタックのあふれ、CPU の版の失敗。「対象外」の出力は変えない | プロセスを終わらせる報告だけを選ぶ。警告や利用者の出力は端末で読むためのもの | 既定案（実装者はこの案に従う） |
| D8 | シグナルの中の報告（スタックのあふれ、`tzr_async_lost`）も送り、E2E の overflow の段で、止まらないことと代替スタックに収まることを測る。止まるなら S2 で止め、シグナルの中では送らない代案を人間に諮る | 起票時の 1 回の実測では、ハンドラーの中の最初の `os_log_create` と送信が動いた。GUI のアプリでいちばん理由の分からない終わり方なので、外す前に測る | 既定案（実装者はこの案に従う） |
| D9 | 新しい環境変数・CLI・診断・manifest のキーを足さない。`OS_ACTIVITY_MODE=disable` などで OS が記録しないときは何も残らない | 自動の判定で足り、名前の登録（D-55）が要らない。OS の記録の設定に逆らわない | 既定案（実装者はこの案に従う） |
