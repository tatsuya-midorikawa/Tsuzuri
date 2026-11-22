# W07: Windows のトラップを確実に終わらせる（fail-fast・VEH・`uwtable`）

| 項目 | 内容 |
| --- | --- |
| ID | W07 |
| 優先度 | P1（今でも、ホストの TimerProc から呼んだ Tsuzuri の関数のトラップが user32 に握りつぶされ、プログラムが続く。W12 がコールバックを「いつでも呼べる」と定める前に塞ぐ） |
| 規模 | M（`windows_abi` の後処理 4 つ、IR の補助関数 4 つ、ランタイム C の 1 マクロ、ドライバーの判定 1 つ、W06 の基盤の上の E2E） |
| 依存 | W04, W06 |
| 後続 | W12, W30, W35 |
| 状態 | todo |
| 起票 | 2026-11-22（D-63。利用者の依頼「X: D13 を狭く改める方針で決定。実現向けたチケットを @_features に W プリフィックスのチケットを作成して」と「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」から起票。HEAD `fa40eede` で確認） |
| 承認 | 要承認（終了コード、E14 D9 の拡張）。D1〜D3 が要承認、ほかは既定案 |
| 手本にする既存実装 | `src/runtime/stack.c` の `tz_stack_fault`（E14 Phase 3。報告を書いてから終わる）、`src/llvm_traps.rs` の `with_frame_pointers`（すべての `define` に属性を足す後処理）、`src/llvm.rs` の `windows_abi`（`tz.console.write` の `entry:` に行を差す）、W04 の `TRAP_WRITE_WINDOWS` |
| 主な影響ファイル | `src/llvm.rs`, `src/llvm_traps.rs`, `src/driver.rs`, `src/test_runner.rs`, `src/runtime/tzroutine.c`, `src/runtime/tzroutine-windows.h`, `tests/windows.mjs`, `tests/stack_overflow.mjs`, `tests/windows_gui_shim.c`（W06。関数を 1 つ足す）, `tests/windows_trap_host.c`（新規）, `src/runtime/test-runner.c`, `src/runtime/bench-runner.c`, `docs/architecture.md`, `docs/language.md`, 言語リファレンスの 6 ページ |

## 実装者向け早見

- 範囲: Windows の native の出力だけ。同じ PR で 3 つを入れる。① 報告の関数 `@tz.trap.report` の最後と、exe 以外の出力の素のトラップを fail-fast（`RaiseFailFastException`）にする。② exe に first-chance の VEH を入れ、exe の image の中のトラップ命令と、スタック枯渇を fail-fast にする（枯渇は `trap: stack overflow` を書いてから）。③ Windows の IR のすべての `define` に `uwtable` を付ける（`nounwind` は残す）。ランタイム C のトラップも同じ道を通す。自前のダイアログは出さない。
- 最初に読む: 「現状」の測定の表 → 「仕様」 → W04 の「他チケットへ提供するインターフェース」。
- 最初に書くテスト: W06 の基盤で「TimerProc の中の素のトラップ」（今は exit 0 で続く）。
- 触ってはいけない: トラップの終了コード 0xC000001D と報告の文、`nounwind`、POSIX と WASM の出力、object がホストの例外の設定を変えないこと（E14 の原則）。
- 完了の見分け方: 「現状」の表の握りつぶされる 2 行が 0xC000001D になり、WM_CREATE の行が 0xC000041D から 0xC000001D になる。

## 目的

ドキュメントは「トラップしたあとに例外として捕まえることはできません」と約束する（`basic-types.md`）。Windows のトラップは `ud2`（Arm64 は `brk #0x1`）が起こす普通の SEH の例外なので、Tsuzuri の外の SEH のフィルターが捕まえうる。捕まえられると、tzroutine の P の状態などが壊れたまま続く（`src/runtime/tzroutine.c` の `tsuzuri_tz_callback_enter` が `syscall_depth` を 0 にし、`tsuzuri_tz_callback_exit` が戻さないまま extern の呼び出しが返ると、`tsuzuri_tz_blocking_exit` が深さを −1 にして `tzr_exitsyscall` をもう一度呼ぶ）。このチケットは、どの OS のフレームの下でもトラップがプロセスを同じ終了コードで終わらせるようにし、デバッガーと WER がスタックを辿れるよう `.pdata` を出す。[E14](_completed/E14-trap-boundary.md) の D9 が「Windows は G10 の完了後に vectored exception handler で同じ報告をする」とし、[G10](_completed/G10-windows.md) が後続の作業に挙げたものを、トラップまで広げて実装する（[GUIDE D-63](GUIDE.md#d-63-tsuzuri-だけで-win32-のアプリを書くe12-d13-の狭い改訂w01w30)）。報告の届け先は [W04](W04-windows-trap-report-lost.md)、検査の基盤は [W06](W06-windows-gui-test-harness.md)。

## 着手条件と停止条件

### 着手条件

- W04 と W06 が `main` にある（`grep -n "TRAP_WRITE_WINDOWS" src/llvm_traps.rs`、W06 の GUI の段が `tests/windows.mjs` にある）。
- D1〜D3 が承認されている。承認前は手順 1（確認だけ）まで。
- `--backend fast` の Windows は、[W03](W03-windows-interop-doc-fixes.md) の D1 の結論（HEAD `fa40eede` で `BuildOptions::validate_fast` が macOS・Linux 以外のホストと Windows の triple を `E2000` にし、x86-64 の COFF の writer は `tests/fast_backend.rs` からしか届かない）のとおり `E2000` なので、このチケットでは何もしない。着手時にこの判定が変わって COFF を作るようになっていれば S3。

### 停止条件

- **S1** CI のランナー（`windows-2022` の x64、`windows-11-arm`）で、`RaiseFailFastException` が 0xC000001D・0xC00000FD を保たない（手順 1）。
- **S2** `uwtable` か VEH で、この表に書いた以外の既存の Windows のテストの結果が変わる（`tsuzuri test` の `trapped or exited with code -1073741795` を含む）。
- **S3** `--backend fast` が Windows の COFF を作る。その出力にも同じ保証を入れるか `E2000` にするかを報告する（`src/backend/target.rs` の `isa` は `unwind_info` を `false` にし、`windows_abi` を通らない）。
- **S4** POSIX か WASM の `--emit llvm` が 1 byte でも変わる。
- **S5** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

- `llvm.trap` は x64 で `ud2`、Arm64 の Windows で `brk #0x1`（`clang --target=...-pc-windows-msvc` の逆アセンブル）。終了コード 0xC000001D は `docs/architecture.md`（Windows MSVC の節）、`basic-types.md`、`test.md` が書く。Arm64 は直接は確かめていない（`windows.yml` の Arm64 の job が `tests/windows.mjs` の 0xc000001d の検査を通している）。
- `src/` に `uwtable` は 0 件。開発版の `--emit object`（`-O0`・`-O3`）は `.pdata`・`.xdata` を持たず、`llvm-readobj --unwind` は空。IR のすべての `define` に手で `uwtable` を足すと、`tz_run` を含む全関数に `RuntimeFunction` が出る。ランタイム C は Clang の既定で `nounwind uwtable` とモジュールの旗 `uwtable` 2 を持つ（`tzroutine.c` の object で 102 個）。
- `src/runtime/` に `AddVectoredExceptionHandler`・`SetUnhandledExceptionFilter`・`RaiseFailFastException` は無い。スタック枯渇の報告は `src/driver.rs` の `stack_runtime`（`platform.unix()`）で POSIX だけ。Windows の深い再帰は `exit=0xC00000FD` で文が無く、`run` の `E2005` は「integer division, indexing, assert, a contract, or allocation may have trapped」と書く（`probable_stack_exhaustion` は Unix 以外で `false`）。`run` は標準エラーの `trap: stack overflow`（`STACK_OVERFLOW_REPORT`）を OS によらず枯渇の文にする。
- ランタイム C のトラップ: `tzroutine.c` の `__builtin_trap()`（`tsuzuri_tz_boundary_leave`、デッドロックの起こし、`Parallel` の待ち）は `ud2`。`tzr_fatal` の `abort()` は、MinGW・MSVC とも 0xC0000409（`__fastfail`）と測った。

### 測定（開発版、Windows 11 26300、x64、MSVC の link.exe。C のホストに Tsuzuri の object（`10 / x` の `tz_on_tick`、`-O3`）を連結）

| トラップの場所 | object | 結果 |
| --- | --- | --- |
| `SetTimer` の TimerProc（`DispatchMessageW` が呼ぶ） | `--trap-info` なし（葉の関数: `test; je; idiv; ret; ud2`） | **握りつぶされる**: exit 0、ループが続いた |
| 同じ | `--trap-info` あり、`uwtable` なし | 0xC000001D |
| 同じ | `--trap-info` あり、手で `uwtable` を足した | **握りつぶされる**: exit 0 |
| 同じ、ホストが first-chance の VEH で元の記録を `RaiseFailFastException` | どちらも | 0xC000001D |
| 同じ、`SetUserObjectInformationW(..., UOI_TIMERPROC_EXCEPTION_SUPPRESSION（7）, FALSE)` が 1 を返した後 | `--trap-info` なし | 0xC000001D |
| `CreateWindowExW` の中の `WM_CREATE`（カーネルからのコールバック） | `uwtable` の有無によらず | 0xC000041D（`STATUS_FATAL_USER_CALLBACK_EXCEPTION`） |
| 同じ、VEH あり | | 0xC000001D |

| 終わらせ方（C） | 終了コード |
| --- | --- |
| `RaiseFailFastException`（コード 0xC000001D の新しい記録、旗 0・`FAIL_FAST_GENERATE_EXCEPTION_ADDRESS`・それに `FAIL_FAST_NO_HARD_ERROR_DLG` を足したもの） | 0xC000001D（旗 3 は MinGW でも同じ）。デバッガーには second-chance の継続できない例外だけが届く |
| VEH の中で元の記録と文脈を `RaiseFailFastException` | 0xC000001D。スタック枯渇なら 0xC00000FD |
| `__fastfail(7)` | 0xC0000409 |
| `TerminateProcess(..., 0xC000001D)` | 0xC000001D（WER を通らない） |

C の VEH でスタック枯渇に `WriteFile` で `trap: stack overflow` を書いて fail-fast すると、文と 0xC00000FD が出た。標準ハンドルなしでは `OutputDebugStringW` の文が 1 つ届いた（`SetThreadStackGuarantee` の有無によらず）。**未確認**: WER のダンプが実際に作られること、Arm64 の VEH の挙動、Fiber（tzroutine）の上の枯渇の報告。

## 変えてはいけない意味

トラップの終了コード 0xC000001D、報告の文と `trap: stack overflow\n`、トラップから戻らないこと、`nounwind`、POSIX と WASM の出力、object とライブラリがプロセス全体の例外の設定を変えないこと。

## 仕様

1. **exe**（`Entry::Console` の `@main` と、`tsuzuri test`・`bench` の実行ファイル）: 入口の最初に `tsuzuri_trap_install` を呼び、それが `AddVectoredExceptionHandler(1, @tz.trap.veh)` と、main スレッドの `SetThreadStackGuarantee(64 KiB)` を呼ぶ。VEH は次の 2 つだけを扱い、ほか（`OutputDebugStringW` の例外、ブレークポイント、C++ の例外、アクセス違反）は `EXCEPTION_CONTINUE_SEARCH` で返す。
   - `STATUS_ILLEGAL_INSTRUCTION` で、例外のアドレスが exe の image（`__ImageBase` から `SizeOfImage` まで）の中にあり、その命令がトラップ命令（x64 は `0F 0B`、Arm64 は `0xD4200020`）のもの → 元の記録と文脈で `RaiseFailFastException(record, context, 0)`。文は書かない（`--trap-info` なら報告がすでに書いた。素のトラップは POSIX と同じく文を持たない）。
   - `STATUS_STACK_OVERFLOW`（アドレスで絞らない。D2） → W04 の `@tz.trap.write` で `trap: stack overflow\n` を書き、`RaiseFailFastException(record, context, 0)`（0xC00000FD）。
2. **報告**: Windows のすべての出力で、`@tz.trap.report` は文を書いた後 `@tz.trap.fail(i32 %site)` を呼び、戻らない。`@tz.trap.fail` は記録（コード 0xC000001D、`EXCEPTION_NONCONTINUABLE`、引数 1 個 = 地点の番号）を作り、`RaiseFailFastException(&record, NULL, FAIL_FAST_GENERATE_EXCEPTION_ADDRESS | FAIL_FAST_NO_HARD_ERROR_DLG)` を呼ぶ。
3. **exe 以外の出力**（`--emit object`・`--emit llvm`、W05 の DLL）: 素のトラップ `call void @llvm.trap()` を `call void @tz.trap.fail(i32 0)` に下げる（後ろの `unreachable` は残す）。exe は `ud2` のまま（VEH が受ける）。object は VEH を入れない。
4. **ランタイム C**: `tzroutine.c` の `__builtin_trap()` を `TZR_TRAP()` にし、Windows では 2 と同じ fail-fast（地点 0）、POSIX と JSPI では今の `__builtin_trap()`。`tzr_fatal` の `abort()` は変えない（D3）。
5. **`uwtable`**: Windows の IR のすべての `define` に `uwtable` を付ける（Clang が C に付けるのと同じ）。`nounwind` は残す。
6. **ドライバー**: Windows の `probable_stack_exhaustion` は終了コード 0xC00000FD を真にする（DLL の中の枯渇など、報告が無いときの `run` の文）。
7. 自前のダイアログは出さない。WER の扱いは OS の設定に任せる。

## 設計

### IR（`src/llvm_traps.rs` に定数、`src/llvm.rs` の `windows_abi` が使う）

- `windows_abi(ir, module, executable: bool)` に引数を足す。呼び出し側は `src/driver.rs`（`options.emit == Emit::Executable`）、`src/test_runner.rs` の 2 か所（`true`）、`emit_with_export_style`（`false`）。
- `TRAP_FAIL_WINDOWS` = `define internal void @tz.trap.fail(i32 %site) cold noreturn nounwind`。`alloca [152 x i8], align 8` に `EXCEPTION_RECORD` を書き（x64・Arm64 で同じ配置: コード 0、旗 4、`ExceptionAddress` 16、`NumberParameters` 24、`ExceptionInformation` 32）、`RaiseFailFastException` を呼んで `unreachable`。素のトラップの置き換えは、この関数の中では行わない。
- `TRAP_VEH_WINDOWS` = `define internal i32 @tz.trap.veh(ptr %info) nounwind` と `define void @tsuzuri_trap_install() nounwind`（C の入口からも呼ぶので internal にしない。名前は E14 D10 のランタイムの `tsuzuri_` 接頭辞）。image の範囲は `@__ImageBase = external dso_local global i8` から `e_lfanew`（+0x3C の `i32`）と `SizeOfImage`（その +0x50）を読む（API を呼ばず、失敗しない）。IR から読んだ `__ImageBase` が image の先頭の "MZ" を指し、+0x50 の値が `SizeOfImage` と一致することを、link.exe と同梱の zig（ld.lld）の両方で確かめた。
- 行の走査で、(a) `define internal void @tz.trap.report(i32 %site)` の中の `  ret void` を `  call void @tz.trap.fail(i32 %site)` と `  unreachable` に、(b) exe なら `define i32 @main(` の `entry:` の次に `  call void @tsuzuri_trap_install()`（`console_main`・`console_def_main` が出す `@main` のどの形でも 1 行）、(c) exe 以外なら `  call void @llvm.trap()` を `  call void @tz.trap.fail(i32 0)` にする。exe には、まだ無ければ W04 の `TRAP_WRITE_WINDOWS` と `DEBUGGER_WRITE` も足す。
- `tsuzuri test`・`bench` の実行ファイルは、`main` が C（`src/runtime/test-runner.c`・`bench-runner.c`）で IR に `@main` が無い。`src/test_runner.rs` の 2 か所（`build_debug_runner` と `compile_native_runner`）は `executable = true` で `windows_abi` を呼び、C の `main` は `#if defined(_WIN32)` の下で最初に `tsuzuri_trap_install()` を呼ぶ。Windows のランナーは `Domain.run` を拒否する（`native_runtime_sources` の `domain_on_windows`）ので、報告が `tsuzuri_trap_raise` に戻る形は Windows に無い。
- 宣言（`RaiseFailFastException`・`AddVectoredExceptionHandler`・`SetThreadStackGuarantee` と W04 の 4 つ）は、同じ行が無いときだけ足す。
- 最後に `with_unwind_tables`（`with_frame_pointers` と同じく `}define` と `!dbg` の位置を扱う）で、shim と `runtime/wasm.ll` を足した後のすべての `define` に ` uwtable` を足す。

### ランタイム C

- `tzroutine-windows.h` に `static void tzr_fail_fast(void)`（2 と同じ記録、地点 0）と `#define TZR_TRAP() tzr_fail_fast()`。`tzroutine.c` の POSIX 側と `tzroutine-jspi.h` は `#define TZR_TRAP() __builtin_trap()`。POSIX の C の出力は変わらない。`numeric.c` の `__builtin_trap` は生成物の `numeric.ll` の `llvm.trap` になり、exe は VEH、object は (c) が受ける。

### ドライバー

- `probable_stack_exhaustion` の `#[cfg(not(unix))]` の版が `status.code() == Some(0xC00000FDu32 as i32)` を返す。`tests/stack_overflow.mjs` は Windows で飛ばさず、実行ファイルの段（main スレッド、tzroutine）を「`trap: stack overflow` を含み、終了コード 3221225725」で検査する（`sigaltstack` などの POSIX の段は Windows で飛ばす）。

## 前提とする他チケットのインターフェース

- W04: `TRAP_WRITE_WINDOWS`（Windows の `@tz.trap.write`）、`DEBUGGER_WRITE`（`@tz.debugger.write`）、ランタイム C の `tzr_report`。
- W06（仕様 4 の名前）: 起動器 `tests/windows_launch.c`（`--no-handles`・`--debug-strings <path>`・`--first-exception`）、`tests/windows.mjs` の `launchWithoutHandles`・`buildGui`、shim `tests/windows_gui_shim.c`（W07 は TimerProc で呼ぶ関数 `tz_gui_timer_proc` を足す。W06 は後続が shim に関数を足す形にしてある）、fixture `trap_timer`・`trap_create`、`tests/windows-pe.mjs` の `unwindFunctions`。W06 の `trap_in_kernel_callback` と `io_trap` の期待（0xC000001D・0xC000041D・0xC0000005 のどれか）を、W07 で 0xC000001D に絞る。
- C のホストに Tsuzuri の object を連結する検査（TimerProc と `WM_CREATE`）は W06 に無いので、W07 が `tests/windows_trap_host.c`（新規）を足す。

## 他チケットへ提供するインターフェース

- 保証: Windows では、Tsuzuri のコードのトラップは、どの OS のフレームの下でも 0xC000001D でプロセスを終わらせる（W12 がコールバックの契約の前提にする）。
- IR の `@tz.trap.fail(i32 %site)` と `@tsuzuri_trap_install()`（Windows だけ）と、`windows_abi` の引数 `executable`。
- Windows の IR の `uwtable`（W28 の Fiber の上のスタックの検証、W30 の検査、[W35](W35-windows-trap-mode-return.md) の `--trap-mode return` の `longjmp` が OS の unwinder で Tsuzuri のフレームを越えるための `.pdata`）。ランタイム C の `TZR_TRAP()`。
- 「まだ無ければ W04 の `TRAP_WRITE_WINDOWS` と `DEBUGGER_WRITE` を足す」段（設計の IR の節）は、1 つの補助（例 `fn ensure_windows_report_writer(output: &mut String)`）にまとめ、[W32](W32-silent-write-failures.md) の Windows の `--overflow-report` の転送と共有する（W32 の D6）。W07 と W32 のうち先に入る方が作る。W04 の `TRAP_WRITE` → `TRAP_WRITE_WINDOWS` の置き換えの後に呼ぶ（先に足すと W04 の形の検査に当たる）。

## 実装手順

1. **変更**: なし（手元と `workflow_dispatch` の `fast`）。**内容**: 「現状」の終わらせ方の C を x64 と Arm64 のランナーで動かす。**確認**: 0xC000001D と 0xC00000FD を記録する（S1）。D1 の承認の材料にする。
2. **変更**: `src/llvm_traps.rs`・`src/llvm.rs`。**内容**: `@tz.trap.fail`、報告の後の fail-fast、exe 以外の素のトラップの置き換え。**確認**: 単体テスト、T1、W06 の基盤で TimerProc と `WM_CREATE` の object の段。
3. **変更**: 同じと `src/runtime/test-runner.c`・`bench-runner.c`。**内容**: VEH と `@tsuzuri_trap_install`、枯渇の報告、ランナーの C の入口からの呼び出し。**確認**: Tsuzuri だけの exe の段（下の表）と `tsuzuri test` の段。
4. **変更**: `tzroutine.c`・`tzroutine-windows.h`。**内容**: `TZR_TRAP()`。**確認**: `node tests/tzroutine.mjs`、`node tests/tzroutine_runtime.mjs`、`sh scripts/check-runtime-includes.sh`。
5. **変更**: `src/llvm.rs`。**内容**: `with_unwind_tables`。2〜4 の後に入れる（`uwtable` だけでは握りつぶしが増える。測定の表）。**確認**: `llvm-readobj --unwind` が `tz_*` を挙げる。`-O3` の `examples` と `benchmarks` の `.text`・`.pdata`・`.xdata` の大きさを測って記録する（CI に合否を入れない）。
6. **変更**: `src/driver.rs`・`tests/stack_overflow.mjs`。**確認**: 枯渇の段が Windows で通る。
7. **変更**: 文書。**内容**: 「ドキュメント」。素のトラップを呼び出しにした object の費用（添字の検査の多いループの `-O3`、置き換えの前後、同じ入力）を測って記録する。**確認**: T2。PR に `ci:full` を付け、`windows.yml` の x64・Arm64 が通る。

## テスト計画

- 単体（`src/llvm.rs`）: exe の IR に `@tsuzuri_trap_install` の呼び出しが `@main` の `entry:` の次に 1 つ（ランナーの IR は定義だけ）、`@tz.trap.veh` があり、`call void @llvm.trap()` が残る。object の IR には `call void @llvm.trap()` が `@tz.trap.fail` の外に無い。すべての `define` が `uwtable` と `nounwind` を持つ。宣言が 1 回ずつ。2 回作って同じ。POSIX の IR（`windows_abi` に通さない）は変わらない。
- E2E（`tests/windows.mjs` と W06 の基盤、`-O0`・`-O3`、x64・Arm64）:

| 段 | 期待 |
| --- | --- |
| `tests/windows_trap_host.c` の TimerProc の素のトラップ（`--trap-info` なしの object） | 0xC000001D、ループが続かない（今は exit 0） |
| 同じ、`--trap-info` の object | 標準エラーの文と 0xC000001D |
| `tests/windows_trap_host.c` の `WM_CREATE` のトラップ（object） | 0xC000001D（今は 0xC000041D） |
| Tsuzuri の exe（`buildGui`）で、shim の `tz_gui_timer_proc` が TimerProc から呼ぶコールバックの中のトラップ | 0xC000001D。トラップの後の印が無い |
| W06 の `trap_in_kernel_callback`・`io_trap` | 0xC000001D だけ（W06 の期待を絞る） |
| exe の深い再帰（`tests/stack_overflow.mjs` の `deep`） | 標準エラーに `trap: stack overflow`、0xC00000FD。`run` の `E2005` が枯渇の文 |
| 同じ、`windows_launch --no-handles --debug-strings <path>` | `<path>` に `trap: stack overflow` が 1 行 |
| tzroutine の中の深い再帰（`routine`） | 結果を記録する。報告が無ければ W28 に渡す（未確認） |
| `--link` した C が `ud1`（`0F B9`）を `__try`/`__except` で捕まえる（MSVC だけ。zig の MinGW は `__try` をコンパイルできないことがある） | 捕まえて続く（VEH はトラップ命令だけを扱う） |
| `tsuzuri test` の `assert false` | 今と同じ `trapped or exited with code -1073741795` |

- 終了コードの検査はデバッガーの下でしない。期待値は測定の表と Windows のヘッダーの定数から書き、コンパイラの出力から写さない。

## ドキュメント

- `types-and-type-inference/basic-types.md`（トラップの段落）と `exception-handling/exception-handling.md`: Windows では、トラップとスタック枯渇は OS の例外の処理の下でも捕まらず、fail-fast で終わること。枯渇は `trap: stack overflow` と 0xC00000FD。
- `compiler/native-interop.md`（ホストは信頼境界の段落）: Windows の object は VEH を入れないが、トラップはホストの SEH の下でも fail-fast で終わること。object の中のスタック枯渇はホストの責任であること。TimerProc を使うホストへの `UOI_TIMERPROC_EXCEPTION_SUPPRESSION` の勧め。
- `compiler/debugging.md`: Windows の出力が `.pdata` を持ち、デバッガーと WER のダンプが Tsuzuri のフレームを辿れること（`--trap-info` のフレームの報告は今どおり macOS と Linux だけ）。
- `compiler/platforms.md`、`built-in-types-and-modules/test.md`（終了コードは変わらない旨の確認）、`docs/architecture.md` の Windows MSVC の節、`docs/language.md` のトラップの節。

## 受け入れ条件

- [ ] 「仕様」の 1〜7 のとおりに動き、E2E の表の段が x64 と Arm64 で通る（tzroutine の段は結果を記録した）。
- [ ] POSIX と WASM の出力が変わらない。Windows でも `--emit header` の出力と診断は変わらない。Windows の IR の変化は「設計」の 4 つの後処理だけ。宣言が重複せず、IR が決定的。
- [ ] `.text`・`.pdata`・`.xdata` の大きさと、object の素のトラップの費用を測って記録した。
- [ ] 文書を直し、`check-docs` が通る。PR に `ci:full` を付けた。

## 落とし穴

- `uwtable` を先に出さない。測定のとおり、`.pdata` があると TimerProc の `__except` まで unwind が届く。fail-fast と VEH を先に入れる。
- VEH は `OutputDebugStringW` の例外（`DBG_PRINTEXCEPTION_C`・`_WIDE_C`）も受ける。コードで絞る。
- exe の image の外（DLL の CPU の探り）の 0xC000001D は扱わない。exe の中でも `ud2` 以外の #UD は扱わない。
- 枯渇の VEH はスタックがほとんど無い所で動く。W04 の書き手（512 バイト、`WriteFile`）だけを呼び、CRT を呼ばない。main 以外のスレッドと Fiber の保証は W28。
- ほかの DLL が先頭に VEH を登録すると、そちらが先に呼ばれる。文書に書く。
- デバッガーの下では fail-fast は second-chance の例外として見える。
- WER が終了を遅らせることがある。トラップするテストの多い suite の時間を前後で測る（合否は入れない）。

## 対象外

- Windows の `--trap-info` のフレームの報告（unwind の情報で辿る walker）と、`--trap-mode return`・`Domain.run` の Windows（`E2002`・`E1038`）。`--trap-mode return` は [W35](W35-windows-trap-mode-return.md) の Phase 1、`Domain.run` は W35 の Phase 2 が、このチケットの `uwtable` の上で扱う。
- object やライブラリが VEH を入れること。tzroutine の Fiber とほかのスレッドの `SetThreadStackGuarantee`（W28）。`--backend fast` の unwind の情報（S3）。CET と CFG（W30）。
- 自前のダイアログ、WER の設定の変更、POSIX のトラップの変更。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | 終わらせ方は `RaiseFailFastException`。報告と素のトラップは新しい記録に旗 `FAIL_FAST_GENERATE_EXCEPTION_ADDRESS \| FAIL_FAST_NO_HARD_ERROR_DLG`、VEH は元の記録と文脈を旗 0 で渡す。終了コード 0xC000001D と 0xC00000FD を保つ | 測定で両方の CRT で保たれた。どのハンドラーも捕まえられず、WER とデバッガーが事後に調べられる。`__fastfail`（命令 `int 0x29` は最も安い）は 0xC0000409 に変わり、`tzr_fatal` とも区別できない。`TerminateProcess` はダンプもデバッガーの停止も無い。CI のヘッドレスの実行で古い形のエラーの窓を出さない | 要承認（承認前は該当する段に着手しない） |
| D2 | E14 D9 の Windows への拡張。すべての exe（再帰の有無によらず）に first-chance の VEH を入れ、exe の image の中のトラップ命令の `STATUS_ILLEGAL_INSTRUCTION` と、アドレスで絞らない `STATUS_STACK_OVERFLOW` を扱う | VEH はフレームのハンドラーより先に呼ばれ、カーネルのコールバック（0xC000041D）と TimerProc の握りつぶしの両方を塞ぐ。トラップはほぼすべての exe にある。枯渇は system DLL や `__chkstk` の中でも起き、OS はスタックの guard でしか上げないので、命令のアドレスでは絞れない（POSIX の `stack.c` もスタックのアドレスで判定する）。E14 の原則どおり object には入れない | 要承認（承認前は該当する段に着手しない） |
| D3 | `tzr_fatal` の `abort()`（0xC0000409）は 0xC000001D にそろえない | POSIX も SIGABRT とトラップの信号を区別する。UCRT の `abort` はすでに fail-fast（両方の CRT で測定）。そろえるなら `tzr_fatal` も `tzr_fail_fast` を呼ぶだけで済む | 要承認（承認前は該当する段に着手しない） |
| D4 | exe 以外の出力の素のトラップは `@tz.trap.fail(i32 0)` の呼び出しに下げる。exe は `ud2` のまま | object には VEH が無く、今も葉の関数のトラップが握りつぶされる（測定）。ホストが VEH を入れる関数を export する案は呼び忘れで元に戻り、「対象外」と書くだけの案は約束を破ったまま残す。exe は VEH が受けるので、命令の大きさと葉の関数のフレームを変えない | 既定案（実装者はこの案に従う） |
| D5 | Windows のすべての `define` に `uwtable`、`nounwind` は残す。fail-fast と同じ PR | Clang が C に付けるのと同じ。`.pdata` が無いと、デバッガー・WER・`RtlVirtualUnwind` が Tsuzuri のフレームを辿れない | 既定案（実装者はこの案に従う） |
| D6 | 記録の引数 1 個目を地点の番号にする（素のトラップは 0） | WER のダンプから `.trap.json` の地点を引ける。終了コードは変わらない | 既定案（実装者はこの案に従う） |
| D7 | main スレッドに `SetThreadStackGuarantee(64 KiB)` | POSIX の `stack.c` の代替スタック 64 KiB に合わせ、枯渇の報告の書き込みと fail-fast の余地を確保する | 既定案（実装者はこの案に従う） |
| D8 | `UOI_TIMERPROC_EXCEPTION_SUPPRESSION` は exe で設定しない（ホスト向けの文書で勧めるだけ） | 塞ぐのはタイマーだけで、`WM_CREATE` の 0xC000041D もほかのフィルターも残る。VEH がすべてを受ける | 既定案（実装者はこの案に従う） |
| D9 | 自前のダイアログは出さない | GUI に出す方法はアプリの判断で、WER の報告と二重になる。壊れた WndProc の中でモーダルのループを回さない | 既定案（実装者はこの案に従う） |
