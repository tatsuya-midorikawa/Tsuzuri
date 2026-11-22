# W06: Windows の GUI をヘッドレスに検査するテスト基盤

| 項目 | 内容 |
| --- | --- |
| ID | W06 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | – |
| 後続 | [W07](W07-windows-trap-fail-fast.md)、[W12](W12-static-callback-lifetime.md)、[W28](W28-ui-on-fibers.md) |
| 状態 | todo |
| 起票 | 2026-11-22（D-63。利用者の依頼「X: D13 を狭く改める方針で決定。実現向けたチケットを @_features に W プリフィックスのチケットを作成して」と「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」から起票。HEAD `fa40eede` で確認） |
| 承認 | 不要（テストの補助と CI の手順を足すだけ。言語の意味・生成コード・診断・CI の起動条件は変えない） |
| 改善する劣位 | 検証の欠落: Windows の CI は、標準ハンドルのある、コンソールのプログラムしか見ていない。標準ハンドルなしの起動・メッセージループ・窓のコールバック・PE の `Subsystem`・`.rsrc`・関数ごとの unwind 情報を確かめる手段がない。そのため W07〜W12・W28 の変更が正しいかを、CI で確かめられない |
| 手本にする既存実装 | `tests/windows.mjs`（`execute`・`cli`・`diagnostic`・`nativeFlags`、C のホストを LLVM の MSVC でリンクする段）、`tests/ffi_extensions.mjs` と `tests/ffi_extensions_host.c`（Tsuzuri と C を組み合わせる fixture）、`.github/workflows/vscode.yml` の「Net sockets and WebAssembly on Windows」（同梱の `tsuzuri-clang` で suite を回す段） |
| 主な影響ファイル | `tests/windows.mjs`・`tests/windows_launch.c`（新規）・`tests/windows_gui_shim.c`（新規）・`tests/windows-pe.mjs`（新規）・`tests/fixtures/windows_gui/`（新規）・`.github/workflows/vscode.yml`・`README.md`（変更の種類別の表）・`_tsuzuri/language-reference/compiler/platforms.md`（tier の確認の列） |

## 実装者向け早見

- 範囲: `tests/windows.mjs` に「GUI の段」を足す。中身は次の 4 つで、どれも画面の描画・DWM・対話的なデスクトップを使わない。
  1. 標準ハンドルなしで子を起動する C の補助 `tests/windows_launch.c`。
  2. メッセージ専用の窓（`HWND_MESSAGE`）で、タイマー・別スレッドからの `PostMessageW`・Tsuzuri のコールバックを回す C の shim `tests/windows_gui_shim.c`。
  3. `llvm-readobj` で PE と COFF を読む補助 `tests/windows-pe.mjs`。
  4. それらを使う fixture `tests/fixtures/windows_gui/`。
- 最初に読む: `tests/windows.mjs`（全体）、`.github/instructions/ci.instructions.md`（AGENTS.md の指示。`tests/windows.*` と workflow を触るため）、`windows.yml` の W5、`vscode.yml` の Windows の Net の段。
- 最初に書くテスト: `launch_without_handles`（仕様 2 の表の 1 行目）。補助の起動器が、子に標準ハンドルを渡さないことを確かめる。
- 触ってはいけない:
  - 既存の `tests/windows.mjs` の検査の順序と期待値。
  - CI の起動条件（`on:`）と matrix の host（`tests/platforms.mjs` が `- os:`・`host:` の行を読む）。
  - W07・W08・W12 が変える挙動を、HEAD の値で固定しないこと（仕様 3）。
- 完了の見分け方: 次の 3 つが通る。
  - `node tests/windows.mjs target/debug/tsuzuri.exe --stage gui` が、LLVM の MSVC（`TSUZURI_CLANG=clang.exe`）と同梱の MinGW（`TSUZURI_CLANG=tsuzuri-clang.exe`）の両方で通る。
  - `ci:full` を付けた PR で、`windows.yml` の 2 host の W5 が通る。
  - `ci:full` を付けた PR で、`vscode.yml` の Windows の 2 job の新しい段が通る。

## 目的

D-63（[GUIDE D-63](GUIDE.md#d-63-tsuzuri-だけで-win32-のアプリを書くe12-d13-の狭い改訂w01w30)）の W チケットは、Windows の GUI のプログラムの挙動を変える。トラップの終わり方（W07）、標準ストリームがないときの IO（W08）、GUI サブシステム（W09）、リソース（W10）、コールバックの寿命（W12）、ファイバーの上の UI（W28）がそれに当たる。今の CI にはそれを確かめる段がない。Node の `spawnSync` は子に必ず有効なハンドルを渡すので、Explorer から起動した GUI のアプリの状態を再現できない。窓を作るテストもない。PE の検査は、`tests/windows.rs` のリンクの引数（`/subsystem:console`）の文字列だけである。このチケットは、共有の runner で人の操作なしに動き、両方のツールチェーンで同じ結果になる検査の部品を足す。合わせて、HEAD で成り立つことを確かめる基準の検査も足す。

## 着手条件と停止条件

### 着手条件

- `.github/instructions/ci.instructions.md` を読む。`zig cc` は `-O1` 以上で `NDEBUG` を定義するので、C の補助は先頭で `#undef NDEBUG` する。Defender は `windows-11-arm` で有効になっている。
- `gh run list --branch main --workflow windows.yml --limit 1` と、同じく `vscode.yml` の最新の run が緑であること。赤なら、その段を手元で再現し、このチケットと無関係であることを記録してから始める。
- 手元の Windows に、LLVM の Clang（MSVC の開発者シェル）と同梱のツールチェーンの両方があること。MSVC は、PowerShell で `Enter-VsDevShell`（Visual Studio の Build Tools、`-arch=x64 -host_arch=x64`）の後に、`TSUZURI_CLANG` を LLVM の `clang.exe` にする。同梱は、`TSUZURI_CLANG` を配布物の `bin/tsuzuri-clang.exe` にする。

### 停止条件

- **S1** GitHub の runner（`windows-2022`・`windows-11-arm`）で、`HWND_MESSAGE` の窓が作れない、またはタイマーとポストしたメッセージが届かない（runner の session に window station がない、など）。段を skip にして逃げない。runner の session の種類と失敗した API と `GetLastError` を記録して報告する。選択肢は 2 つ。A: 窓を使う検査だけを、`windows.yml` の別の job に分ける。B: 窓を使う検査を手元での確認に下げ、CI には起動器と PE の検査だけを残す。
- **S2** `windows-11-arm` の Defender が、テストが作った署名のない exe の起動を止める（起票の前の調査の記録では、このマシンでも、MSVC でリンクした GUI サブシステムの exe が 1 回止められた）。除外の設定を足す前に報告する（ci.instructions.md は workflow で Defender を触らない方針）。
- **S3** GUI の段のどれかの検査が、2 回の修正のあとも不安定に落ちる。時間の上限を緩めたり、再試行を足したりして通さない。
- **S4** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

- `tests/windows.mjs` は、コンソールのプログラム、`tzroutine`、IR と C のホストのリンク、トラップの終了コード 0xC000001D、WASM、名前付きの target を検査する。C のホストは `nativeFlags`（`--target=<arch>-pc-windows-msvc -fuse-ld=lld`）で、LLVM の Clang がリンクする。この suite を回すのは `windows.yml` の W5（`node tests/windows.mjs target/release/tsuzuri.exe`。LLVM 21.1.8 と MSVC 14.44）だけである。`vscode.yml` の Windows の job は、同梱の `tsuzuri-clang` で `tests/net.mjs` を回すが、`tests/windows.mjs` は回さない。
- 標準ハンドルのない起動を作る補助はない。Node の `spawnSync` は `stdio: "ignore"` でも NUL デバイスのハンドルを渡す。このマシンで確かめた: 子の `_get_osfhandle(1)` は有効な値、`GetStdHandle` も非 NULL。
- 窓を作るテスト、`.rsrc` を読む検査、関数ごとの unwind 情報の検査はない。`tests/windows.rs` は、クロスリンクの引数に `/subsystem:console` を書くだけである。
- このマシン（Windows 11 26300 x64、開発版 `target/debug/tsuzuri.exe`）で試作して確かめたこと。ツールチェーンは同梱の 0.1.6（zig 0.16.0 の MinGW）と、MSVC 14.44 の CRT と SDK を使う clang 22.1.8 の 2 つ。clang 22.1.8 の既定のリンカーは lld-link で、`-###` で確かめた。
  - **標準ハンドルなしの起動**: `CreateProcessW` に `STARTF_USESTDHANDLES`、3 つのハンドルに NULL、`bInheritHandles = FALSE`、`DETACHED_PROCESS` を渡す。すると子の fd 0〜2 は `_get_osfhandle` が `-2`、`GetStdHandle` が NULL になった（コンソール・GUI のサブシステム、両方の CRT で同じ）。起動器は `int main(void)` と `GetCommandLineW()` で書けば、同じソースを両方のツールチェーンでコンパイルできた（`-municode` は要らない）。
  - **メッセージ専用の窓**: C の shim が `HWND_MESSAGE` の窓を作る。2 つの `SetTimer` と、C のスレッドからの `PostMessageW(WM_APP)` を、`GetMessageW`・`DispatchMessageW` のループで受け、`WndProc` から Tsuzuri のコールバックを呼ぶ。コールバックを呼ぶのは extern の呼び出しの間なので、今の契約の内である。これが両方のツールチェーンで、標準ハンドルがあってもなくても期待どおりの終了コード 3 で終わった。窓は表示されない。
  - **コールバックの中のトラップ**: `DispatchMessageW` がウィンドウプロシージャに配った `WM_TIMER` の中の整数の 0 除算は、終了コード 0xC000001D で終わった（同梱、`-O3`）。`CreateWindowExW` の中の `WM_CREATE`（カーネルからのコールバック）の中の 0 除算は、0xC000041D（`STATUS_FATAL_USER_CALLBACK_EXCEPTION`）で終わった。どちらも例外は握りつぶされず、トラップの後の Tsuzuri と C のコードは走らなかった（`CreateWindowExW` の後の `return 77` に届かない）。一方、`SetTimer` の TimerProc（`DispatchMessageW` が例外を抑える枠の中で呼ぶ）の中のトラップは、unwind 情報の有無によって握りつぶされ、exit 0 でループが続く（W07 の「現状」の測定）。トラップの後に処理が続くという、意味の破れである。この基盤は TimerProc のトラップの検査を持たず、W07 が直すときに足す。
  - **unwind 情報**: C の `.obj` を `llvm-readobj --unwind` で読むと、関数ごとに `StartAddress: <名前>` の行が出た。同梱（zig）は葉の関数にも出し、MSVC の clang は非葉の関数だけに出した。CRT を連結した exe は、どれも CRT の関数の `.pdata` を持つ。そのため、`.pdata` の節があるかどうかは、Tsuzuri の関数の unwind 情報の有無を表さない。`tsuzuri build --emit object` の `.obj`（`export def rec f` の再帰関数）を `llvm-readobj --unwind` で読むと、`-O0`・`-O3` とも `UnwindInformation [ ]`（空）で、`.pdata`・`.xdata` の節もない。
  - **PE のヘッダー**: `llvm-readobj --file-headers` の `Subsystem:` 行は、`-Wl,--subsystem,windows`（同梱）と `-Xlinker /subsystem:windows -Xlinker /entry:mainCRTStartup`（MSVC）で `IMAGE_SUBSYSTEM_WINDOWS_GUI (0x2)`、既定で `IMAGE_SUBSYSTEM_WINDOWS_CUI (0x3)` を出した。`--coff-resources` は `.res` を連結した exe の資源の型と ID と言語を出した。
- CI の `llvm-readobj`: `windows.yml` と `vscode.yml` の Windows の job は、どちらも LLVM 21.1.8 を `$env:RUNNER_TEMP\llvm21`（`windows.yml`）か `LLVM_PREFIX`（`vscode.yml`）に入れる。公式のインストーラーには `llvm-readobj.exe` が入っている（このマシンの LLVM 22.1.8 にもある）。CI の LLVM 21.1.8 に入っていることは未確認なので、手順 1 で確かめる。同梱のツールチェーンの `bin` には入っていない（`clang`・`tsuzuri-clang`・`wasm-ld`・`tsuzuri` だけ）。

## 仕様

### 1. 段の入口

- `node tests/windows.mjs <tsuzuri.exe>` は、今の検査の後に GUI の段を回す。
- `node tests/windows.mjs <tsuzuri.exe> --stage gui` は GUI の段だけを回す。`vscode.yml` が同梱の `tsuzuri-clang` で使う。
- 今の検査は LLVM の MSVC の Clang（`nativeFlags`）を前提にしているので、`--stage gui` のときは回さない。
- GUI の段のプログラムは、どちらも `TSUZURI_CLANG` のツールチェーンでビルドする。補助の C は、Tsuzuri の `--link` で渡すか、同じ `TSUZURI_CLANG` でコンパイルする。
- 段は標準出力に 1 行の要約を出す（今の suite と同じ書き方。例: `Windows GUI: launcher, message-only windows, callbacks, tzroutine wake and PE checks passed with <toolchain>`）。

### 2. 基準の検査（HEAD で通るもの）

| 名前 | 内容 | 期待 |
| --- | --- | --- |
| `launch_without_handles` | 起動器で、C の探り（`tests/windows_launch.c` の `--probe` で自分を起動し、`_get_osfhandle(0..2)` と `GetStdHandle` をファイルに書く）を起動する | fd 0〜2 がすべて `-2`、ハンドルがすべて NULL |
| `debug_strings` | `--debug-strings` で、`OutputDebugStringW` を 2 回呼ぶ C の探り（`--probe-debug`）を起動する | `<path>` に 2 行。終了コードは 0 |
| `launch_exit_codes` | 起動器が子の終了コードをそのまま返す（`def main` が 0・7・255 を返す Tsuzuri のプログラム） | `exit=0x00000000` など、子の値 |
| `io_without_handles` | fixture `io`: `IO.try_writeln` の結果を終了コードにする（`Ok` → 10、`Error _` → 11）。別の fixture `io_trap` は `IO.writeln` を呼ぶ | ハンドルありは 10 と 0、ハンドルなしは 11 と 0 でない終了コード（仕様 3） |
| `message_window` | fixture `pump`: shim の `tz_gui_pump` を呼ぶ。タイマー 2 つと別スレッドの `PostMessageW` の 3 通を Tsuzuri のコールバックが数える | 終了コード 3。標準ハンドルがあってもなくても同じ |
| `message_window_reentry` | fixture `reentry`: shim は `WM_APP` の `lparam` に窓を入れて渡す。コールバックはその中から shim の `tz_gui_send`（同じスレッドへの `SendMessageW`）を呼び、コールバックが入れ子に呼ばれる | 入れ子の深さ 2 を返す |
| `tzroutine_wakes_pump` | fixture `wake`: `main` の擬似 G が `tz_gui_wait`（`GetMessageW` で待つ）を呼ぶ。`tzroutine { }` が `tz_gui_post`（`PostMessageW`）で起こす。窓は i64 の値で渡す | 終了コード 1。`TSUZURI_MAXPROCS=1` と `4` |
| `trap_in_dispatched_callback` | fixture `trap_timer`: ウィンドウプロシージャに配った `WM_TIMER` の中の 0 除算 | 終了コードが 0xC000001D か 0xC0000005（unwind 情報のない例外の配送。W07 が 0xC000001D に絞る）。トラップの後の印（shim が書くファイル）がない |
| `trap_in_kernel_callback` | fixture `trap_create`: `CreateWindowExW` の `WM_CREATE` の中の 0 除算 | 終了コードが 0xC000001D か 0xC000041D のどちらか。`CreateWindowExW` の後の印がない（W07 が 0xC000001D に絞る） |
| `pe_headers` | `windows-pe.mjs` で、コンソールの Tsuzuri の exe と、C の探りを GUI サブシステムでリンクした exe を読む | `Subsystem` が 3 と 2。Machine が host の arch |
| `pe_resources` | C の探りに、テストが書いた `.res`（手順 3。RT_MANIFEST の ID 1 と RT_VERSION の ID 1）を `--link` したものを読む | 資源の一覧が `{type: 24, name: 1, language: 1033}` と `{type: 16, ...}` |
| `unwind_functions` | C のソースを `TSUZURI_CLANG -c` でコンパイルした `.obj` と、`tsuzuri build --emit object` の `.obj` を `unwindFunctions` で読む | C の非葉の関数の名前が含まれる（補助そのものの検査）。Tsuzuri の `.obj` の結果は出力するだけで、比べない（W07 が全関数を求める） |

- **時間**: どの GUI のプログラムも、自分で終わる。shim は 5 秒の見張りのタイマー（`SetTimer(NULL, ...)`）を張り、それを超えると `DestroyWindow` して -1 を返す。起動器も 60 秒で `TerminateProcess` し、`timeout` を出して失敗する。速さの合否は入れない（AGENTS.md）。
- **後片付け**: 作った exe と一時ファイルは、今の suite の `directory`（`mkdtempSync`）の下に置き、`finally` で消す。

### 3. W07・W08・W12 が変える挙動を固定しない

- 標準ハンドルのない `IO.try_writeln` の `Error` の種類は、W08 が変える。今の段は「`Error` である」ことだけを確かめる。fixture は `Result.Error _ -> 11` で受ける。
- 非 `try_` の書き込みがトラップすることは、W08 でも変わらない。終了コードは W07 が変えうるので、0xC000001D と 0xC000041D と 0xC0000005 のどれかであり、0 でないことだけを確かめる。0xC0000005 は、unwind 情報がないときの例外の配送で観測した値。
- TimerProc（`SetTimer` の第 4 引数）の中のトラップは、HEAD では握りつぶされうるので、この段に入れない（W07 が足す）。
- W12 の検査（登録した extern が返った後の呼び出し、スレッドプール）は、W12 が足す。このチケットは shim に関数を足せる形（仕様 4）だけを用意する。

### 4. 提供する部品の形（後続のチケットがこの名前で使う）

`tests/windows_launch.c`（起動器）。

- 使い方は `windows_launch.exe [--no-handles] [--first-exception] [--debug-strings <path>] [--timeout-ms N] -- <exe> [args...]`。
- 結果は標準出力の最後の行に出す。`exit=0x%08lX` か `timeout`。
- `--first-exception` は `DEBUG_ONLY_THIS_PROCESS` で起動し、最初の初回例外（最初のブレークポイントを除く）を `exception=0x%08lX rva=0x%llx module=<名前>` の 1 行で前に出す。デバッガーが付くと挙動が変わりうるので、終了コードの検査には使わず、失敗の調査（W07）に使う。
- `--debug-strings <path>` も `DEBUG_ONLY_THIS_PROCESS` で起動する。子の `OUTPUT_DEBUG_STRING_EVENT`（`OutputDebugStringW`・`OutputDebugStringA`）を `ReadProcessMemory` で読み、UTF-8 の 1 行ずつで `<path>` に追記する。待つのは `WaitForDebugEventEx` にする。`WaitForDebugEvent` では、`OutputDebugStringW` の文字列が ANSI に変換されて届く（試作で、日本語が `??` になり、`fUnicode` が 0 だった。`Ex` では 1 で UTF-16 のまま届いた）。文字列は NUL まで読む。`nDebugStringLength` は長さの下位 16 bit しか持たないので、長さには使わない（上限は 64 KiB）。W04・W08 が、標準エラーの代わりに届いた報告を確かめるのに使う。例外は `DBG_EXCEPTION_NOT_HANDLED` で子に返すので、終了コードは付けないときと同じになる。ただし、`IsDebuggerPresent` を見るコードの経路は変わりうる。
- `--probe <path>` は、自分の fd 0〜2 の `_get_osfhandle` と `GetStdHandle` を `<path>` に書いて 0 で終わる（`launch_without_handles` の子）。`--probe-debug` は `OutputDebugStringW(L"probe 1")` と `L"probe 2"` を呼んで 0 で終わる（`debug_strings` の子）。

`tests/windows_gui_shim.c`（`--link tests/windows_gui_shim.c -l user32` で Tsuzuri のプログラムに連結する）。

```c
/* Every function creates its own HWND_MESSAGE window of the class "tz-test", runs until done, and destroys it.
 * A 5-second guard timer ends any loop; the functions then return -1. */
int64_t tz_gui_pump(int64_t (*on_message)(uint32_t message, uint64_t wparam, int64_t lparam), int64_t expected);
int64_t tz_gui_send(int64_t window, uint32_t message, uint64_t wparam, int64_t lparam); /* SendMessageW */
int64_t tz_gui_window(void);              /* creates the window and returns it as an integer */
int64_t tz_gui_wait(int64_t window);      /* GetMessageW until WM_APP; returns its wparam */
int64_t tz_gui_post(int64_t window, uint64_t wparam); /* PostMessageW(WM_APP) from any thread */
int64_t tz_gui_create(int64_t (*on_create)(uint32_t, uint64_t, int64_t)); /* WM_CREATE forwarded */
void tz_gui_mark(const uint16_t *path, int64_t length); /* writes a marker file (reached after a call) */
```

- 窓とメッセージは、Tsuzuri の側で i64 の値として扱う（W18 の `extern type copy` の前でも書ける）。W12・W28 は、この shim に関数を足して使う。
- shim は `#undef NDEBUG` で始め、`assert` の中で検査する対象を呼ばない。

`tests/windows-pe.mjs`（ES module）。

```js
export function readobj(); // the llvm-readobj to use: LLVM_PREFIX/bin, then beside TSUZURI_CLANG, then PATH; throws if none
export function peHeaders(file);   // { machine, subsystem, dllCharacteristics: [names] } from --file-headers
export function peSections(file);  // [section names] from --sections
export function peResources(file); // [{ type, name, language }] from --coff-resources (numbers, or strings for named entries)
export function unwindFunctions(object); // Set of function names that have a RuntimeFunction (the StartAddress lines of --unwind) of a COFF object
```

`tests/windows.mjs` は、次の 2 つを export する。どちらも、子のプログラムが自分で終わる前提で使う。

- `launchWithoutHandles(exe, args = [], options = {})`: 起動器を `--no-handles` で回し、子の終了コード（数値）を返す。
- `buildGui(fixture, flags = [])`: `tests/fixtures/windows_gui/<fixture>` を shim と `-l user32` 付きでビルドし、exe のパスを返す。

suite のファイルを import しても検査が走らないように、検査の本体は `import.meta.url` が入口のときだけ走らせる。

## 設計

- **起動器（C）**: `CreateProcessW(NULL, <コマンド行>, NULL, NULL, FALSE, DETACHED_PROCESS | (first_exception || debug_strings ? DEBUG_ONLY_THIS_PROCESS : 0), NULL, NULL, &startup, &process)` を呼ぶ。`startup.dwFlags = STARTF_USESTDHANDLES` で、3 つのハンドルは NULL にする。コマンド行は `GetCommandLineW()` の `--` より後ろをそのまま使い、自分で引用し直さない（W11 の範囲の `CommandLineToArgvW` の規則を持ち込まない）。待つのは `WaitForSingleObject(process, timeout)`、終了コードは `GetExitCodeProcess` で取る。`--no-handles` を付けないときは、ハンドルを継承する通常の起動をする（比べるため）。
- **ビルド**: 起動器は、段の最初に `TSUZURI_CLANG` で 1 回コンパイルする（`-O1`、MSVC は `-D_CRT_SECURE_NO_WARNINGS`）。同梱の `tsuzuri-clang` は `zig cc` に渡すので、MSVC の `--target` は付けない。LLVM の Clang には、今の suite と同じ `nativeFlags` を付ける。
- **`.res` の作り方（`pe_resources`）**: `tests/windows-pe.mjs` に、テスト用の最小の `.res` を書く `writeTestRes(entries)` を置く。RES の形式は、32 バイトの空の見出し、各資源の `DataSize`・`HeaderSize`・型と名前の序数（`0xFFFF` と WORD）・`DataVersion`・`MemoryFlags`・`LanguageId`・`Version`・`Characteristics`・4 バイトの境界の詰め物。rc の道具を要らないようにするためである。W10 の Rust の書き手とは独立に書き、W10 はこれを参照に使わない（W10 は `zig rc`・`llvm-rc` の出力を参照にする）。
- **workflow**: `windows.yml` は変えない。W5 が suite 全体を回すので、GUI の段も入る。`vscode.yml` の Windows の 2 job（`win32-x64`・`win32-arm64`）には、新しい段「Windows GUI stage with the bundled toolchain」を 1 つ足す。中身は `TSUZURI_CLANG` を `toolchain/bin/tsuzuri-clang.exe` にして、`node ../tests/windows.mjs toolchain/bin/tsuzuri.exe --stage gui` を回すこと。`llvm-readobj` は、この job が設定する `LLVM_PREFIX` の `bin` から見つかる（`windows.yml` は LLVM の `bin` を `GITHUB_PATH` に足すので PATH から見つかる）。新しい環境変数は足さない（足すと D-55 への登録になる）。置き場所は Net の段の後ろで、Net と同じく `!cancelled()` にする。`on:` と matrix は変えない。

## 前提とする他チケットのインターフェース

- なし（HEAD の機能だけを使う）。

## 他チケットへ提供するインターフェース

- 仕様 4 の名前と形を、そのまま提供する（`windows_launch.exe` の引数と出力、shim の関数、`windows-pe.mjs` の関数、`launchWithoutHandles`・`buildGui`）。
- 各チケットの使い方:
  - W04: 標準エラーが無いときのトラップの報告を `--debug-strings` で確かめる。
  - W07: 終了コードの検査に `launchWithoutHandles`、失敗の調査に `--first-exception`、全関数の unwind 情報の検査に `unwindFunctions` を使う。
  - W08: 標準ストリームのない IO を、`launchWithoutHandles` で検査する。
  - W09: `peHeaders(...).subsystem` を使う。
  - W10: `peResources` を使う。
  - W12: shim に関数を足す。
  - W28: 固定した tzroutine の中で `tz_gui_pump` を回す。
  - W29: 別のスレッドからの起こしの遅延を `tz_gui_window`・`tz_gui_wait`・`tz_gui_post` で測る（構成 3w）。
  - W30: `readobj`・`peHeaders` を使い、`windows-pe.mjs` に `peExtendedCharacteristics`・`peLoadConfig` を足す。
  - W25・W26・W27: `buildGui` と shim を使い、W25 は別のスレッドからダイアログを閉じる関数、W26 はメッセージ専用でない隠れた窓を作る関数を shim に足す（依存ではない。任意の Windows の段）。
- W04・W08・W09・W10 が先に着手した場合は、そのチケットが同じ名前・同じ形でこの部品の必要な部分を足し、このチケットはそれを引き継ぐ。

## 実装手順

1. **変更**: なし。**内容**: `node scripts/repo-kb.mjs brief W06`。CI の LLVM 21.1.8 に `llvm-readobj.exe` があるかを、`windows.yml` の「Show tools」の出力か、インストーラーの中身で確かめる。なければ止めて報告する（PE の検査の手段を選び直す）。**確認**: 記録だけ。
2. **変更**: `tests/windows_launch.c`。**内容**: 仕様 4 の起動器（`--first-exception`・`--debug-strings`・`--probe` を含む）。**確認**: `launch_without_handles`・`launch_exit_codes`・`debug_strings` を、両方のツールチェーンで手元で回す。
3. **変更**: `tests/windows-pe.mjs`。**内容**: `readobj`・`peHeaders`・`peSections`・`peResources`・`unwindFunctions`・`writeTestRes`。**確認**: `pe_headers`・`pe_resources`・`unwind_functions`。
4. **変更**: `tests/windows_gui_shim.c`、`tests/fixtures/windows_gui/{io,io_trap,pump,reentry,wake,trap_timer,trap_create}/Main.tz`。**内容**: 仕様 2 の fixture。各 `Main.tz` は `@trusted extern` で shim を宣言し、`def main :: unit -> i32` で結果を返す。**確認**: `io_without_handles`・`message_window`・`message_window_reentry`・`tzroutine_wakes_pump`・2 つのトラップ。それぞれ `-O0` と `-O3` で、標準ハンドルのある起動とない起動の両方を回す。
5. **変更**: `tests/windows.mjs`（`--stage gui`、export、`import.meta.url` の判定）。**確認**: `node tests/windows.mjs target/debug/tsuzuri.exe`（全体。MSVC の開発者シェル）と `--stage gui`（同梱）。
6. **変更**: `.github/workflows/vscode.yml`、`README.md`、`platforms.md`。**確認**: `ci:full` を付けて push し、`gh run watch <id> --exit-status` で `windows.yml` と `vscode.yml` の Windows の job を読む。

## テスト計画

- 仕様 2 の表の 12 件。`-O0`・`-O3` × 標準ハンドルあり・なし × MSVC・同梱で回す。トラップの 2 件はハンドルなしだけでよい。
- 補助そのものの検査:
  - `unwindFunctions` は、C の `.obj` の非葉の関数の名前が含まれることで確かめる（葉の関数は、ツールチェーンによって含まれない）。
  - `peResources` は、`writeTestRes` が書いた既知の内容で確かめる。
  - 起動器は、`--probe` の値で確かめる。
- Rust のテストは足さない（コンパイラは変えない）。

## ドキュメント

- `README.md` の「変更の種類別」の表: Windows の挙動を変える変更の行に、`node tests/windows.mjs <tsuzuri> --stage gui`（両方のツールチェーン）を足す。
- `_tsuzuri/language-reference/compiler/platforms.md`: tier の「確認」の説明に、`tests/windows.mjs` の GUI の段の内容を足す。中身は、標準ハンドルなしの起動、メッセージ専用の窓、PE の検査で、`windows.yml` と `vscode.yml` が回すこと。言語の振る舞いは変わらない。
- `.github/instructions/ci.instructions.md` は、S1・S2 に当たったときだけ教訓を足す。
- `node scripts/check-docs.mjs _tsuzuri/language-reference/compiler/platforms.md`。

## 受け入れ条件

- [ ] 仕様 2 の 12 件が、`-O0`・`-O3`、標準ハンドルのあり・なし、MSVC・同梱で通る（手元の記録）。
- [ ] `node tests/windows.mjs` の今の検査の結果と出力が変わらない（GUI の段の行が増えるだけ）。
- [ ] 仕様 4 の名前と形で部品がある。`tests/windows.mjs` を import しても検査が走らない。
- [ ] `ci:full` を付けた PR で、`windows.yml` の 2 host の W5 と、`vscode.yml` の Windows の 2 job の新しい段が通る。
- [ ] `on:` と matrix の host が変わっていない（`node tests/platforms.mjs` が通る）。
- [ ] コンパイラ（`src/`）と std を変えていない。
- [ ] README と platforms.md を直し、`check-docs` が通る。

## 落とし穴

- **Node は標準ハンドルなしで起動できない**。`stdio: "ignore"` も NUL を渡す。PowerShell の `Start-Process` は、起票の前の調査で GUI サブシステムの exe をハンドルなしで起動できた。ただし、コンソールのサブシステムの exe には新しいコンソールを作る（Windows の既定の挙動。ここでは確かめていない）。起動器は C で書く。
- **`DETACHED_PROCESS` を忘れない**: 親がコンソールを持つと、コンソールのサブシステムの子はそれにつながり、ハンドルが復活しうる。
- **`zig cc` の `NDEBUG`**（ci.instructions.md）: shim と起動器の検査を `assert` に入れない。
- **トラップの終了コードを 1 つに固定しない**（仕様 3）。0xC0000005 は、`Debug.print` の失敗のトラップを同梱の MinGW の `-O1` 以上で起こしたときに観測した。このとき最初の例外は、Tsuzuri のコードの `ud2`（0xC000001D）だった。その後の例外の配送中に、ntdll の中で読み出しの違反が続いた（`--first-exception` の試作で確認）。W07 の `uwtable` の範囲。
- **`.pdata` の節の有無で unwind 情報を判定しない**: CRT の関数が持つ。関数の名前の残る `.obj` で `--unwind` を読む。
- **窓のクラスの登録**: 同じプロセスで shim の関数を何度も呼ぶので、`RegisterClassExW` の `ERROR_CLASS_ALREADY_EXISTS` は成功として扱う。
- **見張りのタイマー**: `SetTimer(NULL, ...)` のタイマーは、`hwnd` が NULL の `WM_TIMER` としてスレッドのキューに来る。ループで見分けて、見張りの終わりとして扱う。

## 対象外

- 画面に描いた結果の画素の検査（`PrintWindow` は DWM に依存する。`CreateDIBSection` の描画の検査は、描画の機能を足すチケットで扱う）。
- UI Automation・IME・ドラッグ＆ドロップの検査（W26）。
- 署名（`signtool`）と SmartScreen。
- `windows.yml` の構成の変更と、新しい host。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | GUI の段は `tests/windows.mjs` の中に置き、`--stage gui` で単独でも回せるようにする | `windows.yml` の W5 を変えずに入る。`vscode.yml` は同梱のツールチェーンでその段だけを回せる。別の suite にすると `windows.yml` の段が増える | 既定案（実装者はこの案に従う） |
| D2 | PE と COFF は `llvm-readobj` で読む（`LLVM_PREFIX/bin` → `TSUZURI_CLANG` の隣 → PATH。専用の環境変数は足さない） | 両方の workflow が LLVM 21.1.8 を入れ、`LLVM_PREFIX` か PATH で見つかる。テスト専用の変数でも D-55 の登録になる（`TSUZURI_GUIDE_EXAMPLES` の前例）ので足さない。自前の PE の読み手は、それ自体の正しさを別に確かめる必要がある。却下した案: Node で PE を読む（同梱の bin に `llvm-readobj` がない手元でも動くが、`.rsrc` と unwind の解釈を二重に持つ） | 既定案（実装者はこの案に従う） |
| D3 | 窓は `HWND_MESSAGE` だけを使い、描画・表示・DWM に頼らない | runner の session の種類によらずに動く見込みが高い（S1 で確かめる）。表示の検査は、この基盤の目的（意味の検査）に要らない | 既定案（実装者はこの案に従う） |
| D4 | 標準ハンドルなしの起動は、C の起動器の `STARTF_USESTDHANDLES` と NULL と `DETACHED_PROCESS` で作る | Explorer から起動した GUI のアプリと同じ CRT の状態（fd 0〜2 が `-2`）になることを、両方の CRT で確かめた。PowerShell の `Start-Process` は、コンソールのサブシステムの exe では状態が変わりうる（未確認） | 既定案（実装者はこの案に従う） |
| D5 | CI の起動条件は変えない。W チケットの Windows の挙動を変える PR には `ci:full` を付ける | AGENTS.md の CI の方針。GUI の段は `windows.yml`・`vscode.yml` が merge 後と `ci:full` で回す | 既定案（実装者はこの案に従う） |
| D6 | 後続が変える挙動（トラップの終了コード、`IO.Error` の種類）は、HEAD の値で固定せず、許す値の集合か「0 でない」で確かめる | 後続のチケットが、この段の期待を書き換えずに済む。ただし、トラップの後に処理が続かないこと（意味の不変条件）は、今から固定する | 既定案（実装者はこの案に従う） |
