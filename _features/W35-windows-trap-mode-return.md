# W35: Windows の `--trap-mode return`（`trap.c` の移植と `.pdata`）

| 項目 | 内容 |
| --- | --- |
| ID | W35 |
| 優先度 | P2（今は Windows で `--trap-mode return` の object も DLL も作れない。作れる `--emit llvm` も、MSVC では `trap.c` がコンパイルできず、MinGW では最適化した IR のトラップが 0xC00000FF でプロセスを落とす） |
| 規模 | L（Phase 1・2。人間が求めない限り Phase 1 だけを実装する） |
| 依存 | W07 |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-22（D-63。W01〜W30 の起票中に見つけ、利用者の依頼「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」に当たるので起票。HEAD `fa40eede` で確認） |
| 承認 | 要承認（E14 の Windows の境界の方針）。D1・D3 が要承認、ほかは既定案 |
| 改善する劣位 | C/C++ 比: Windows のホストに埋め込んだ Tsuzuri の関数のトラップを、値として返せない（macOS・Linux は [E14](_completed/E14-trap-boundary.md) Phase 2 で返せる） |
| 手本にする既存実装 | `src/runtime/arguments.c`（`_WIN32` の下で `WIN32_LEAN_AND_MEAN` を守って `windows.h` を読む）、`src/runtime/tzroutine-windows.h`（SRWLOCK）、`src/driver.rs` の `build_complete` の `merge_runtime_ir`（ランタイムを IR で 1 つにする macOS の debug object と bitcode の経路）と `compile_runtime_object`（ランタイムのキャッシュ）、`tests/trap_return.mjs`・`tests/trap_return_host.c`、[W05](W05-windows-shared-library.md) の `link_shared` の Windows の分岐 |
| 主な影響ファイル | `src/runtime/trap.c`, `src/runtime/tzroutine.c`, `src/driver.rs`, `src/test_runner.rs`（Phase 2）, `tests/trap_return.mjs`, `tests/trap_return_host.c`, `tests/trap_boundary_runtime.c`, `tests/trap_return_sync.mjs`, `tests/fixtures/trap_return_plain/Main.tz`（新規）, `tests/windows_trap_return_gui.c`（新規）, `tests/windows.mjs`, `tests/domain.mjs`・`tests/domain_runtime.c`（Phase 2）, `.github/workflows/windows.yml`, `docs/architecture.md`, `docs/language.md`, 言語リファレンスの 5 ページ |

## 実装者向け早見

- 範囲: Phase 1 は Windows の target（x86-64・Arm64、MSVC・MinGW）の `--trap-mode return`（`--emit object`・`llvm`・`header`・`shared`、C#・Python・C++ のバインディング）。①`trap.c` の排他を SRWLOCK にする、②`tzroutine.c` の Windows の「境界の中か」の判定（今は常に偽）と hook を POSIX と同じ意味にする（Windows は定義を持たない弱い参照）、③trap ランタイムだけを埋め込む COFF の object の `E2002` を外す（ランタイムを bitcode にして `-Xclang -mlink-bitcode-file` で連結）、④W05 の DLL の `E2000`（W05 D4）を外す。巻き戻しは [W07](W07-windows-trap-fail-fast.md) の `uwtable`（`.pdata`）の上で CRT の `longjmp` に任せる。Phase 2 は `Domain.run` の Windows（`E1038` を外す）。
- 最初に読む: 「現状」の測定の表 → 仕様 → [E14](_completed/E14-trap-boundary.md) の D6 と「実装と検証（Phase 2・3）」の 9。
- 最初に書くテスト: `tests/trap_return.mjs` の Windows の段（trap だけの fixture の `--emit object -O3`。今は `E2002`）と、`--emit llvm -O3` を C のホストにリンクした段（W07 の前は 0xC00000FF）。
- 触ってはいけない: `tsuzuri_try_<name>` の ABI と状態 0・1・2、`tz_<name>`、トラップした呼び出しの確保を解放する規則、POSIX の `trap.c`・`tzroutine.c` の生成コード、tzroutine・CPU・IO・Net・OS のランタイムを埋め込む Windows の object の `E2002`、コールバックとの併用の `E2000`。
- 完了の見分け方: MSVC と同梱の MinGW の両方で、trap だけの fixture の `--emit object --trap-mode return -O3` を C のホストにリンクして、トラップが状態 1 で返り、`live == 0` のまま 3,000 回続く。C のホストの WndProc の中の `tsuzuri_try_*` のトラップが状態 1 で返り、窓が動き続ける。

## 目的

[E14](_completed/E14-trap-boundary.md) Phase 2 は、ホストに埋め込んだ Tsuzuri の関数のトラップを `tsuzuri_try_<name>` の状態 1 として返す境界（`src/runtime/trap.c` の `setjmp`・`longjmp`）を macOS と Linux に入れ、Windows を「残作業」に残した（`--trap-mode return` の COFF object は `E2002`、実行は未検証）。Windows の GUI のホスト（C・C++ の WndProc から Tsuzuri の関数を呼ぶ形）では、1 回のトラップでプロセス全体が落ちるのを避けたい。このチケットは、`trap.c` を Windows でコンパイルできるようにし、W07 が出す `.pdata` の上で `longjmp` が Tsuzuri のフレームを正しく越えることを保証し、object と DLL の出力の拒否を外す。[GUIDE D-63](GUIDE.md#d-63-tsuzuri-だけで-win32-のアプリを書くe12-d13-の狭い改訂w01w30) の Win32 のアプリの土台の一部である。

## 着手条件と停止条件

### 着手条件

- W07 が `main` にある。確認: `grep -n "with_unwind_tables" src/llvm.rs` が当たり、Windows の `--emit llvm` のすべての `define` が ` uwtable` を持つ（W07 のテスト）。
- D1・D3 が承認されている。承認前は手順 1（確認だけ）まで。
- W05 が `main` にあるかを確かめる（`grep -n "fn module_definition" src/driver.rs`）。無ければ手順 5（DLL）を飛ばし、W05 の実装者が D4 の `E2000` を入れずに済むよう、このチケットの PR の説明に書く（W05 D4 の理由は、このチケットで両方とも解消する）。
- `node scripts/repo-kb.mjs brief W35`。手元に MSVC 14.44、LLVM 21 以降の Clang（`C:\Program Files\LLVM\bin`）、同梱ツールチェーン（`T=/c/Users/<user>/.vscode/extensions/tmidorikawa.tsuzuri-0.1.6-win32-x64/toolchain`）。

### 停止条件

- **S1** CI の LLVM（`windows.yml` の 21.1.8）か同梱の Arm64 の LLVM-MinGW の Clang が、IR の入力に `-Xclang -mlink-bitcode-file` を受けない、または連結した object のリンクで記号が解決しない。選択肢: A Windows の trap の object は `E2002` のまま、`llvm` と DLL だけを進める。B 止めて報告する。起票時に確かめたのは同梱の 21.1.8（x86-64 MinGW）と LLVM 22.1.8（x86-64 MSVC）だけ。
- **S2** W07 の `uwtable` の上でも、`-O3` のトラップが状態 1 にならない（0xC00000FF など）。`.pdata` の無い関数が残っている（ランタイムの `.ll` の一部、W07 の後処理の漏れ）と考えられる。直し方を即興で決めずに、`llvm-readobj --unwind` の結果と一緒に報告する。
- **S3** POSIX（`x86_64-unknown-linux-gnu`・`aarch64-apple-darwin`）の `trap.c`・`tzroutine.c` を `-S -emit-llvm` にした結果が、変更の前後で違う（`source_filename` を除く）。
- **S4** `tzroutine.c` の hook を Windows で有効にしたことで、既存の Windows の検査（`tests/windows.mjs`、`tests/tzroutine.mjs`、`tests/tzroutine_runtime.mjs`）の結果が変わる。
- **S5** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

### ランタイムの C

- `src/runtime/trap.c` は 8 行目で `pthread.h` を無条件に読み、`struct tz_boundary` の `pthread_mutex_t lock` を `pthread_mutex_init`・`lock`・`unlock`・`destroy` で使う。`TZ_TRAP_API` は `_WIN32` では空（強い定義）、それ以外は `weak` かつ `hidden`。`tsuzuri_trap_hooks`・`tsuzuri_sync_hooks` は `__attribute__((weak, visibility("hidden")))`、`tz_frame` は `_Thread_local`、`tz_boundary_install` は `__attribute__((constructor))`。`TZ_SETJMP` は Apple 以外で `setjmp`。
- 再現（Git Bash、リポジトリの根）: `"/c/Program Files/LLVM/bin/clang.exe" -std=c11 -c -O1 src/runtime/trap.c` は `fatal error: 'pthread.h' file not found`。Developer PowerShell の `cl /nologo /c /std:c11 src\runtime\trap.c`（`pthread.h` を SRWLOCK に写す仮のヘッダーを `/I` で与えても）は 72 行目（`__attribute__((weak, visibility("hidden")))`）で `error C2143`。同梱の `$T/bin/tsuzuri-clang.exe -std=c11 -c src/runtime/trap.c` は通り、リンクした実行ファイルの import は `KERNEL32.dll`・`ntdll.dll`・UCRT だけだった（zig が winpthreads を静的に入れる）。
- `src/runtime/tzroutine.c` の `tzr_trap_boundary` は `#if defined(_WIN32)` で常に `0` を返す（TZ11 の 19c29b06 で、Windows に境界が無かったときに入った）。`tsuzuri_trap_hooks`・`tsuzuri_sync_hooks` の宣言（316〜340 行）と `tzr_init` の `tsuzuri_sync_hooks.abandon = tzr_abandon`（1395〜1397 行）は `#if !defined(_WIN32)` の中にある。`tsuzuri_tz_parallel` は境界の中なら呼び出し元ですべての添字を走らせるが、Windows では判定が偽なので手伝いの tzroutine に配る。`tsuzuri_tz_preempt_check` と `tzr_async_entry` も同じ判定で、境界の中の G に譲らせない。

### ドライバーの判定

- `build_complete` の `if platform.windows() && native_runtime && options.emit == Emit::Object` が `E2002`（`platform_unsupported_api`）を返す。文は `Windows COFF objects with embedded tzroutine, CPU, IO, or Net runtime are not supported; emit LLVM and link the runtime once, or build an executable` で、trap ランタイムは `native_runtime` に入るのに文に出てこない。再現: `export def div :: own i64 -> own i64 -> i64` を持つ `Main.tz` を `target/debug/tsuzuri.exe build <dir> --emit object --trap-mode return -o lib.obj` → この `E2002`。
- この拒否の理由: ランタイム入りの object は、`build_complete` の最後で `clang -r -nostdlib`（再配置可能なリンク）で 1 つにする。COFF ではこれができない。再現: `clang --target=x86_64-pc-windows-msvc -fuse-ld=lld -r -nostdlib a.obj b.obj -o c.obj` は `lld-link: warning: ignoring unknown argument '-r'` と `subsystem must be defined`、同梱の `tsuzuri-clang -r -nostdlib a.o b.o` は `error: coff does not support linking multiple objects into one`。macOS の debug object と bitcode が使う `merge_runtime_ir`（`llvm-link`）は、LLVM の Windows のインストーラーの `bin` にも同梱ツールチェーンにも `llvm-link.exe` が無い（`native-interop.md:468` も「配布物の `bin/` に `llvm-link` があるのは macOS だけ」と書く）。[G10](_completed/G10-windows.md) D8 は「ランタイムを含む COFF の object は `E2002` のまま。複数 object の weak 統合は対象外」。
- `--emit llvm --trap-mode return` は Windows でも通る（ホストが `trap.c` を自分でリンクする）。`--emit shared` は Windows で `E2000`（`BuildOptions::validate_shared`）。W05 はそれを外すが、`--trap-mode return` の Windows の DLL は W05 D4 で `E2000` のまま残す（理由は「`trap.c` は `pthread.h` を要し、`longjmp` は `.pdata` のない Windows のフレームを越えられない」）。
- コールバックを使うプログラムとの併用は `E2000`（`build_complete` の `--trap-mode return cannot be combined with extern callbacks`）。したがって `longjmp` が越えるのは Tsuzuri のフレームとランタイムの C のフレームだけで、ホストや OS のフレームは越えない（E14 の「決定事項への追記」の 9）。
- `Domain.run` は Windows で `E1038`。`build_complete` の `domain` の判定（`platform.windows()` → `Windows is not supported`）と、`src/test_runner.rs` の `native_runtime_sources`・`compile_native_runner` の `domain_on_windows`。再現: `tests/fixtures/domain/Main.tz` を写したディレクトリで `target/debug/tsuzuri.exe build <dir>` と `target/debug/tsuzuri.exe test <dir>` → どちらも `error[E1038]: 'Domain.run' needs a native POSIX build with the system allocator: Windows is not supported`。

### Windows の `setjmp`・`longjmp` は巻き戻す

- 同梱の MinGW の `setjmp.h`（`zig/lib/libc/include/any-windows-any/setjmp.h`）は、x86-64 の SEH では `setjmp(BUF)` を `_setjmp((BUF), __builtin_frame_address (0))`、UCRT では `_setjmp` を `__intrinsic_setjmpex` にする（`trap.c` の object は `__intrinsic_setjmpex` と `__imp_longjmp` を参照した）。MSVC の target の Clang の object は `_setjmp` と `longjmp` を参照した。どちらも frame を記録した `longjmp` で、CRT が OS の unwinder にフレームを辿らせる。
- Tsuzuri の IR には `uwtable` が無く（W07 の「現状」）、`--emit llvm --trap-mode return` の IR を `clang -c` した object の `.pdata` は `write` の 1 つだけだった（`llvm-readobj --unwind`）。

### 測定（開発版、Windows 11 26300、x64）

`inner`（`a / b`）→ `middle` → `export def div` の 3 段の Tsuzuri の関数を `--emit llvm --trap-mode return` にし、`tsuzuri_try_div(&info, &r, 10, 2)`・`(10, 0)`・`(7, 1)` を順に呼ぶ C のホストと `src/runtime/trap.c` を一緒にリンクした。`uwtable` は IR の `define` の行に `sed -E '/^define /s/ \{$/ uwtable {/'` で足した。MSVC の列は、`pthread.h` を SRWLOCK に写す仮のヘッダーで `trap.c` をコンパイルした。

| 構成 | `-O0` の IR | `-O3` の IR |
| --- | --- | --- |
| 同梱 MinGW（clang 21.1.8）、`uwtable` なし | 状態 1、続きも動く | **0xC00000FF**（`STATUS_BAD_FUNCTION_TABLE`）。2 回目の呼び出しの途中で終わる |
| 同じ、`uwtable` あり | 状態 1 | 状態 1、3 回目も正しい |
| MSVC の target（clang 22.1.8、リンカーは既定の lld-link） | 状態 1 | **0xC00000FF** |
| 同じ、`uwtable` あり | 状態 1 | 状態 1 |
| 同じ、`-fuse-ld=link`（link.exe） | – | `uwtable` なし 0xC00000FF、あり 状態 1 |
| C の GUI のホスト（MinGW）。WndProc の `WM_TIMER` で `tsuzuri_try_div` を呼び、2 回目の tick で 0 で割り、3 回目で `DestroyWindow` | – | `uwtable` あり: 状態 1 の後も tick が続き、`PostQuitMessage(7)` の終了コード 7。なし: 2 回目の tick（0 で割る呼び出し）で 0xC00000FF |
| `trap.c` を `-D__USE_MINGW_SETJMP_NON_SEH`（巻き戻さない `_setjmp(buf, NULL)`）でコンパイル、`uwtable` なし | – | 状態 1 |
| 境界の外（`tz_div(10, 0)` を直接） | – | `trap: integer division by zero at ...Main.tz:2:16` を書いて 0xC000001D |

- `-O0` で動くのは偶然と考えられる（推定。`RUNTIME_FUNCTION` の無い関数を unwinder は葉の関数と見なしてスタックの先頭を戻り先として読むので、たまたま正しい戻り先に当たるまで進んだ）。`-O0` の結果から正しさを判断しない。
- **未確認**: Arm64（この機械は x64）。zig の MinGW のヘッダーでは、Arm64 の SEH の `setjmp` も `__builtin_sponentry()` を渡して巻き戻す形である。Windows Arm64 の配布物が使う LLVM-MinGW（platforms.md）のヘッダーは見ていない。

### ランタイムを bitcode にして 1 つの object にする

- `clang -std=c11 -c -emit-llvm -O3 trap.c -o trap.bc` の後、`clang -c -O3 lib.ll -Xclang -mlink-bitcode-file -Xclang trap.bc -o merged.obj` で、同じ clang の実行の中で連結した 1 つの COFF の object ができた（MSVC の target の LLVM 22.1.8 と、同梱の 21.1.8）。`llvm-nm` は `T tsuzuri_trap_raise`・`T tsuzuri_boundary_run`・`T tsuzuri_try_div`・`W tsuzuri_trap_hooks` を示し、`llvm-readobj --unwind` はランタイムの関数の `RuntimeFunction` を示した。C のホストとリンクして状態 1（MSVC は lld-link と link.exe の両方）。
- `-mno-incremental-linker-compatible`（`src/target.rs` が MSVC の triple に付ける）を付けると、2 回の出力が同じバイト列になった。付けないと COFF の時刻で違う。
- 同じ方法で作った 2 つの trap の object（`div` と `rem`）を 1 つのホストにリンクすると `lld-link: error: duplicate symbol: tsuzuri_boundary_run`（Windows の `TZ_TRAP_API` は強い定義）。
- trap mode と関係なく、Windows では Tsuzuri の object を 2 つ 1 つのホストにリンクできない。どの object も弱い定義（128 bit の補助 `__multi3` など、所有バッファを返すなら `tsuzuri_alloc`・`tsuzuri_free`）を持ち、COFF の弱い定義は既定の名前（`.weak.__multi3.default.<object の最初の公開記号>`）が object ごとに違う。スカラーだけの普通の object と trap の object の組でも、lld-link は `duplicate symbol: __multi3`、link.exe は `LNK1227: conflicting weak extern definition for '__multi3'` で止まった（所有バッファを返す 2 つの object では `tsuzuri_alloc` で同じ誤り）。G10 D8 の「複数 object の weak 統合は対象外」の具体的な姿である。この弱い定義の衝突は [W39](W39-windows-multiple-objects-weak-defaults.md) が直す（弱い定義を `comdat any` に入れる）。W39 の後も、上の trap ランタイムの強い定義の重複（`tsuzuri_boundary_run`）は残る。
- 一方、片方の翻訳単位が定義を持たない弱い参照（`extern __attribute__((weak)) struct H hooks;` と、読む前の `&hooks != 0`）で、もう片方が 1 つだけ定義を持つ形は、lld-link・link.exe・同梱の MinGW のどれでもリンクでき、定義が無ければアドレスが 0、あれば定義を読んだ（小さな C の試作）。
- `trap.c` と `tzroutine.c` を 1 つの翻訳単位にした bitcode を連結した object も `.CRT$XLB`（`tzr_tls_callback_entry`）と `.tls$` を保った（`llvm-objdump -h`）。

### tzroutine と一緒に使うと壊れる

- `tests/fixtures/trap_return/Main.tz` は `Parallel.init` を使い、Windows の `--emit llvm --trap-mode return` の IR は `tsuzuri_tz_parallel`・`tsuzuri_tz_scope_enter` などを宣言する（tzroutine のランタイムが要る）。この IR に `uwtable` を足し、上の方法で `trap.c`+`tzroutine.c` を連結した試作（MinGW、`-O1`）では、`tests/trap_return_host.c` の `scalar_cases`・`buffer_cases`・`parallel_cases` は通ったが、`alternating` で落ちた。切り分けると、`tsuzuri_try_parallel_div(…, 16, 0)` を繰り返すと 3 回目でセグメンテーション違反、`tsuzuri_try_parallel_mixed(…, 3, 9)` を繰り返すと 5 回目に `trap: integer division by zero at …Main.tz:47:44` を書いて 0xC000001D で終わった（境界の外の手伝いの tzroutine でトラップした）。結果は実行ごとに変わる（どの添字を呼び出し元と手伝いのどちらが取るかの競争）。`parallel_div` の繰り返しを 8 回実行すると、1 つに連結した試作は 6 回セグメンテーション違反、`--emit llvm` の経路と同じく IR・`trap.c`・`tzroutine.c` を別々の object にしてリンクした形（今も受け付ける経路。MinGW）は 4 回セグメンテーション違反・1 回トラップの報告で終わった。原因は上の `tzr_trap_boundary` の Windows の分岐と読める（呼び出し元でトラップすると、手伝いが死んだスタックの上の `job` を触り、手伝いでトラップすると境界の外になる）。`Mutex.with_lock` の中のトラップの後にロックが解放されないこと（`abandon` が無い）は、コードから読んだだけで再現していない（未確認）。

### テスト

- `tests/trap_return.mjs`・`tests/trap_return_sync.mjs`・`tests/domain.mjs` は `process.platform === "win32"` で飛ばす（前の 2 つの文は `a Windows object with an embedded runtime is E2002`）。CI では `targets.yml` の「Trap boundaries and failure domains (E14, B10)」の step と、`tests/compact_strings_modes.mjs` の一覧が動かす。`tests/trap_boundary_runtime.c`・`tests/domain_runtime.c` はホストのスレッドを `pthread_create` で作る。`tests/windows.mjs` は tzroutine の fixture の `--emit object` の `E2002` を検査する（このチケットの後も残る）。
- `src/llvm.rs` の `windows_abi` が `dllexport` を付けるのは `tz_*` だけで、`tsuzuri_try_*` には付かない（`define i32 @tsuzuri_try_div(`）。W05 の DLL は `.def` で export するので影響しない。

## 変えてはいけない意味

- `tsuzuri_try_<name>` の ABI、状態 0（成功）・1（トラップ、`site`・`kind`）・2（同じスレッドで入れ子）、トラップした呼び出しが確保したものの解放、成功した呼び出しの結果はホストの物であること（E14 D1・D4・D6・D7）。
- 境界の外のトラップは W07 のとおり報告を書いて fail-fast（`@tz.trap.fail`）で終わる。
- POSIX の生成コードと C ランタイムの出力。tzroutine・CPU・IO・Net・OS のランタイムを埋め込む Windows の object の `E2002`。コールバックとの併用の `E2000`。

## 仕様

### Phase 1

1. Windows の target（`x86_64-pc-windows-msvc`・`-gnu`、`aarch64-pc-windows-msvc`・`-gnu`、Windows のホストの `--target native`）の `--trap-mode return` は、`--emit object`・`llvm`・`header`・`shared`（W05 の後）とバインディングで、POSIX と同じ意味で動く。
2. `--emit object`: 埋め込む C ランタイムが trap ランタイムだけ（`trap_runtime` が真で、tzroutine・CPU・IO・OS・Net・GPU・arguments のどれも要らない）なら作る。ほかのランタイムが要れば今の `E2002`（文は変えない。今の文の「tzroutine, CPU, IO, or Net」がこれで正確になる）。
3. Windows の 1 つのホストにリンクできる Tsuzuri の object は、trap mode によらず今と同じく 1 つ（G10 D8。「現状」の測定）。trap の object の 2 つ目や普通の object との組は、リンカーの記号の重複の誤り（link.exe の `LNK2005`・`LNK1227`、lld-link の `duplicate symbol`）になると文書に書く（D4）。今の文書がこの制限を書いていなければ、同じ PR で書く。[W39](W39-windows-multiple-objects-weak-defaults.md) が先に `main` に入っていれば、着手時にこの項を見直す: 弱い定義の衝突はなくなり、残るのは trap ランタイムの強い定義の重複（trap の object の 2 つ目）だけなので、「1 つのホストに 1 つ」の文は書かず、その重複だけを書く（W39 の「他チケットへ提供するインターフェース」）。
4. `--emit shared`（W05 の後）: `--trap-mode return` を受け、tzroutine・CPU・IO・Net のランタイムも今の DLL と同じく連結する。export の一覧は W05 の `shared_exports`（`tsuzuri_try_<name>` を含む）。
5. `--emit llvm`: ホストは `src/runtime/trap.c`（tzroutine を使うなら `tzroutine.c` も）を Clang でコンパイルしてリンクする。`cl.exe` ではコンパイルできない（`__attribute__`）と文書に書く。
6. tzroutine のランタイムは、Windows でも境界の中の `Parallel.*` をすべて呼び出し元で添字の順に走らせ、境界の中の G を横取りで譲らせず、トラップの後に呼び出しのスレッドが持つロックを解放する（POSIX と同じ）。
7. 新しい診断コード・kind・オプションは足さない。

### Phase 2

8. `Domain.run` を Windows の native の実行ファイル・object（仕様 2 と同じ条件）・DLL・`tsuzuri test`・`run` で使える。`E1038` の `Windows is not supported` を外す（ほかの理由の `E1038` は今のまま）。
9. tzroutine の中の `Domain.run` は、POSIX と同じく、領域の間その G を横取りで別の M へ移さない（仕様 6 の判定に依る）。

## 設計

### `trap.c`（Phase 1）

```c
#if defined(_WIN32)
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>
typedef SRWLOCK tz_lock;
#define TZ_LOCK_INIT(lock) InitializeSRWLock(lock)
#define TZ_LOCK_DESTROY(lock) ((void)(lock))
#define TZ_LOCK(lock) AcquireSRWLockExclusive(lock)
#define TZ_UNLOCK(lock) ReleaseSRWLockExclusive(lock)
#else
#include <pthread.h>
typedef pthread_mutex_t tz_lock;
#define TZ_LOCK_INIT(lock) pthread_mutex_init(lock, NULL)
#define TZ_LOCK_DESTROY(lock) pthread_mutex_destroy(lock)
#define TZ_LOCK(lock) pthread_mutex_lock(lock)
#define TZ_UNLOCK(lock) pthread_mutex_unlock(lock)
#endif
```

- `struct tz_boundary` の `lock` を `tz_lock` にし、15 か所の `pthread_mutex_*` の呼び出しをマクロにする。POSIX の展開は今の呼び出しと同じ文字列なので、生成コードは変わらない（S3 で確かめる）。
- `setjmp`・`longjmp` は変えない（D1）。冒頭の注釈の「Windows has no sigsetjmp」の後に、Windows の `longjmp` は OS の unwinder が `.pdata` を辿ること、その `.pdata` は W07 の `uwtable` が出すことを書く。
- ドライバーはランタイムの部品を 1 つの `runtime.c` に連結する（`trap.c` は 5 番目、`tzroutine.c` は最後）。`WIN32_LEAN_AND_MEAN` の守りは `arguments.c`・`tzroutine-windows.h` と同じ形なので、どの順でも再定義の警告は出ない。新しいファイルは足さない（`scripts/check-runtime-includes.sh` は変わらない）。

### `tzroutine.c`（Phase 1）

- 316〜340 行の Windows の扱いを変える。POSIX は今のまま（`weak`・`hidden` の定義。ELF と Mach-O は複数の弱い定義を 1 つにまとめる）。Windows では、`TSUZURI_TRAP_HOOKS_DEFINED`・`TSUZURI_SYNC_HOOKS_DEFINED` が未定義のとき、型を宣言して、定義を持たない弱い参照 `extern __attribute__((weak)) struct tz_trap_hooks tsuzuri_trap_hooks;`（`tsuzuri_sync_hooks` も同じ）だけを置く。定義は `trap.c` の 1 つだけになる。COFF では、別々の翻訳単位の弱い定義は既定の名前が違って衝突するが、弱い参照と 1 つの定義の組はどのリンカーでも通る（「現状」の測定）。ドライバーの 1 つの `runtime.c` では `trap.c` が先にあるので、守りのマクロでこの宣言は飛ばされる。
- `tzr_init` の `tsuzuri_sync_hooks.abandon = tzr_abandon` は、Windows では `if (&tsuzuri_sync_hooks != NULL)` の中で行う（trap ランタイムが無ければアドレスが 0）。
- `tzr_trap_boundary` の `_WIN32` の分岐を、`&tsuzuri_trap_hooks != NULL && tsuzuri_trap_hooks.owner != NULL && tsuzuri_trap_hooks.owner() != NULL` にする（POSIX は今の式のまま）。trap ランタイムの無いプログラムでは今と同じ偽になる。
- 1 つの翻訳単位で弱い定義を見たときの `&x != NULL` に Clang が警告（`-Wtautological-pointer-compare` など）を出すなら、Windows の分岐の比較を守りのマクロの外側でだけ使う。警告が出るかは確かめていない（未確認）。

### ドライバー（Phase 1）

- `build_complete` の COFF の判定を 2 つに分ける。`windows_trap_object = platform.windows() && options.emit == Emit::Object && trap_runtime && !(tzroutine_runtime || cpu_runtime || gpu_runtime || io_runtime || os_runtime || net_runtime || arguments_runtime)`。`windows_trap_object` でない `platform.windows() && native_runtime && Emit::Object` は今の `E2002`。
- `windows_trap_object` のとき: ランタイムの C を `-c -emit-llvm`（`-S` なし。bitcode）で `runtime.bc` にコンパイルし（`compile_runtime_object` でキャッシュする。引数に `-emit-llvm` が入るので鍵は別になる）、プログラムの IR を object にする clang の実行に `-Xclang -mlink-bitcode-file -Xclang <runtime.bc>` を足して、その出力を成果物にする。再配置可能なリンク（`clang -r`）は行わない。
- `src/llvm.rs` に定数を 1 つ置く（名前は案）: `pub(crate) const LINK_BITCODE: [&str; 3] = ["-Xclang", "-mlink-bitcode-file", "-Xclang"];`。cc1 の旗なので、使う所はここだけにし、テストで旗の文字列を固定する。
- `-mno-incremental-linker-compatible` は今の規則のまま（MSVC の triple だけ。Windows のホストの `--target native` には付けない。platforms.md の「MSVC の COFF の時刻」）。
- `BuildOptions::validate_shared` の W05 の判定（`self.platform().windows() && self.trap_return` の `E2000`）を外す。W05 が無ければ何もしない（着手条件）。

### Phase 2（`Domain.run`）

- `build_complete` の `domain` の `else if platform.windows()` の腕と、`src/test_runner.rs` の `native_runtime_sources`・`compile_native_runner` の `domain_on_windows` の呼び出しを外す。`domain_on_windows` の関数も消す。実行ファイルの `trap.c` は普通のリンクで入る。object は Phase 1 の `windows_trap_object` と同じ条件。
- `tests/domain.mjs` の Windows の段、`tests/domain_runtime.c` のスレッドの差し替え（下）。

### テストの補助

- `tests/trap_boundary_runtime.c`・`tests/domain_runtime.c` は `#if defined(_WIN32)` で `CreateThread`・`WaitForSingleObject`・`CloseHandle` を使う小さな関数（`tz_test_thread_start`・`tz_test_thread_join`、名前は案）を足し、`pthread_create`・`pthread_join` の呼び出しをそれに替える。POSIX の展開は今の呼び出しのまま。
- `tests/fixtures/trap_return_plain/Main.tz`（新規）: `tests/fixtures/trap_return/Main.tz` から `parallel_div`・`parallel_mixed`・`nested_div` など tzroutine・CPU のランタイムを要る関数を除いたもの（`div`・`noop`・`fill_div`・`imported_div`・`make`・`label`・`label_div`・`call_host`）。Windows の object の経路はこれを使い、全部の fixture は `--emit llvm` と DLL の経路で使う。
- `tests/windows_trap_return_gui.c`（新規）: 「現状」の GUI のホストの形（窓を作り、`SetTimer` の `WM_TIMER` で `tsuzuri_try_div` を呼び、2 回目の tick でトラップ、3 回目で `DestroyWindow`）。`tests/windows.mjs` が `timeout` 60 秒で動かす。W06 の `tests/windows_launch.c` があれば `--timeout-ms` で包む。

## 前提とする他チケットのインターフェース

- [W07](W07-windows-trap-fail-fast.md): Windows の IR のすべての `define` の `uwtable`（`with_unwind_tables`）、境界の外のトラップの `@tz.trap.fail`、ランタイム C の `TZR_TRAP()`。`trap.c` 自身はトラップしない（メモリ不足は `abort()`）。
- [W05](W05-windows-shared-library.md)（あれば）: `link_shared` の Windows の分岐、`module_definition`、`validate_shared` の D4 の判定。
- [W06](W06-windows-gui-test-harness.md)（W07 の依存なので入っている）: `tests/windows_launch.c` の `--timeout-ms`。

## 他チケットへ提供するインターフェース

- 保証: Windows のホストから `tsuzuri_try_<name>` を呼ぶと、トラップは状態 1 で返る（POSIX と同じ）。[W26](W26-com-objects-in-tsuzuri.md) と [W27](W27-win32-package-from-winmd.md) が C++ のホストへ Tsuzuri の部品を配るときの土台。
- ドライバーの `windows_trap_object` の経路（ランタイムを bitcode にして `-mlink-bitcode-file` で連結する）。ほかのランタイムを埋め込む Windows の object の `E2002`（G10 D8）を将来外すときの材料（`.CRT$XLB` が残ることは確かめた）。
- `tzroutine.c` の Windows の `tzr_trap_boundary` と `abandon`（Phase 2 の `Domain.run`、W28 の固定した tzroutine の検証）。

## 実装手順

1. **変更**: なし（下書きは `target/scratch/w35/`）。**内容**: 「現状」の測定を W07 の入った `main` で繰り返す（`uwtable` は W07 が付けるので `sed` は要らない）。CI の LLVM 21.1.8 と、手に入れば Arm64 の機械で `-mlink-bitcode-file` を試す。POSIX の `trap.c`・`tzroutine.c` の `-S -emit-llvm`（`--target=x86_64-unknown-linux-gnu`・`aarch64-apple-darwin`、`-O0`・`-O3`）を保存する。`tests/fixtures`・`examples` の `Main.tz` の `--emit llvm`（native・`--target x86_64-pc-windows-msvc`・`wasm32`、`-O0`・`-O3`）・`--emit header`・`check --json` を `target/scratch/w35/base/` に保存する（受け入れ条件の比較）。**確認**: `-O3` で状態 1。S1・S2 に当たらない。
2. **変更**: `src/runtime/trap.c`。**内容**: 設計の `tz_lock`。**確認**: MSVC の Clang・同梱の MinGW で `-Wall -Wextra -Werror -c` が通る。手順 1 の POSIX の IR と一致（S3）。`sh scripts/check-runtime-includes.sh`。
3. **変更**: `src/runtime/tzroutine.c`。**内容**: hook と `tzr_trap_boundary`。**確認**: `node tests/tzroutine.mjs target/debug/tsuzuri.exe`、`node tests/tzroutine_runtime.mjs target/debug/tsuzuri.exe`、`node tests/windows.mjs target/debug/tsuzuri.exe` が今と同じ（S4）。`trap_return` の全部の fixture の `--emit llvm` を、ホストが `trap.c` と `tzroutine.c` を別々にコンパイルしてリンクした形で 3,000 回（設計の弱い参照がリンクできること）。
4. **変更**: `src/driver.rs`・`src/llvm.rs`。**内容**: `windows_trap_object` と bitcode の連結。**確認**: 単体テスト（下）、T1。手元で trap だけの fixture の object が MSVC と同梱の MinGW で動く。2 回の `build --no-cache` の出力が同じ（MSVC の triple を名前で渡したとき）。
5. **変更**: `src/driver.rs`（`validate_shared`）。**内容**: W05 の D4 の判定を外す（W05 があるとき）。**確認**: W05 の `tests/windows.mjs` の DLL の段に trap mode の段を足して通す。`tests/host_bindings.mjs` の Windows の段の `--trap-mode return` の段を、`E2000` の確認から実行に替える。
6. **変更**: `tests/trap_return.mjs`・`tests/trap_return_host.c`・`tests/trap_boundary_runtime.c`・`tests/trap_return_sync.mjs`・`tests/fixtures/trap_return_plain/Main.tz`・`tests/windows_trap_return_gui.c`・`tests/windows.mjs`・`.github/workflows/windows.yml`。**内容**: テスト計画の段。`windows.yml` に `node tests/trap_return.mjs target/release/tsuzuri.exe` の step を足す（`.github/instructions/ci.instructions.md` を先に読む）。**確認**: Developer PowerShell と Git Bash（同梱 MinGW）で各 suite。
7. **変更**: 文書。**確認**: T2、`node scripts/check-docs.mjs <変更したページ>`、`node scripts/check-cli-docs.mjs`。PR に `ci:full` を付け、`windows.yml` の x64・Arm64 と `targets.yml` が通る。
8. （Phase 2、人間が求めたとき）**変更**: `src/driver.rs`・`src/test_runner.rs`・`tests/domain.mjs`・`tests/domain_runtime.c`・文書。**内容**: 設計の Phase 2。**確認**: `node tests/domain.mjs target/debug/tsuzuri.exe` が Windows で通る。`tests/fixtures/domain` の `run`・`build`・`test`。tzroutine の中で `Domain.run` を呼び、領域の中で長く計算するプログラム（横取りの要求が来る長さ）が状態を壊さない。

## テスト計画

- 単体（`src/driver.rs`）: Windows の triple の `Emit::Object` で、trap だけなら `E2002` にならず、`tzroutine`・`io` などがあれば今の `E2002` の文（判定を関数にして入力の組で検査する）。`LINK_BITCODE` の文字列。`validate_shared` が Windows の trap mode を受ける（W05 があるとき）。Phase 2: `domain_unsupported` が Windows で出ない。
- E2E（`tests/trap_return.mjs` の Windows の段、`-O0`・`-O3`、MSVC と同梱 MinGW）:

| 段 | 期待 |
| --- | --- |
| trap だけの fixture の `--emit object --trap-mode return` と `tests/trap_return_host.c`（`COUNTING` なし） | POSIX と同じ状態・`site`・`kind`。`.trap.json` の位置と一致。3,000 回の交互 |
| `-DMIXED_OBJECTS`（普通の object と trap の object を 1 つのホストに） | Windows では飛ばす（仕様 3。`__multi3` の弱い定義の衝突を測定済み）。[W39](W39-windows-multiple-objects-weak-defaults.md) が先に入っていれば、この理由は成り立たないので、Windows でも動かせるかを着手時に確かめる |
| trap ランタイムを `--emit llvm` の IR とは別の object（`trap.c` と `tzroutine.c` を別々にコンパイル）でリンク | リンクでき（設計の弱い参照）、`parallel_*` の段が 3,000 回通る（今の Windows の分岐では 8 回中 5 回落ちる） |
| `-g` の trap だけの object | リンクでき、状態 1（ランタイムの bitcode はデバッグ情報を持たない。未確認） |
| 全部の fixture の `--emit llvm` と `trap_return_host.c -DCOUNTING`（ホストが `trap.c` を数える allocator で持つ）と `tzroutine.c` | 3,000 回、`live == 0`、`parallel_*` の段を含む |
| 全部の fixture の `--emit shared`（W05 があるとき） | 同じ（`COUNTING` なし） |
| `tests/trap_return_sync.mjs` の `Mutex.with_lock` のトラップ（`--emit llvm` か DLL） | トラップの後の次の呼び出しがロックを取れる |
| `tests/windows_trap_return_gui.c`（trap だけの object） | 状態 1 の後も tick が続き、終了コード 7 |
| 境界の外で `tz_div(1, 0)` を直接 | W07 のとおり 0xC000001D |
| `tests/trap_boundary_runtime.c`（`-O0`・`-O3`） | `trap boundary runtime: N checks passed` |
| `tests/windows.mjs` の tzroutine の fixture の `--emit object` | 今と同じ `E2002` |

- 期待値は `.trap.json` とソースの位置から書き、出力を写さない（GUIDE-LITE §2 規則 10）。終了コードの検査はデバッガーの下でしない。
- 変えないことの確認: POSIX の `trap.c`・`tzroutine.c` の IR（手順 1・2）、POSIX の `tests/trap_return.mjs`・`tests/domain.mjs`（`targets.yml`）。

## ドキュメント

- `_tsuzuri/language-reference/compiler/native-interop.md`: `--trap-mode return` の節（371 行付近）に Windows の object・DLL・`--emit llvm` の例と、1 つのホストに trap の object は 1 つ、`trap.c` は Clang でコンパイルすること。
- `_tsuzuri/language-reference/compiler/option.md`: 383・387 行（`--trap-mode return` の出力の表と文）。
- `_tsuzuri/language-reference/compiler/platforms.md`: 166 行の既知の制限（trap だけの object は作れる、ほかのランタイムの object は `E2002`）。
- `docs/language.md`: 1571 行付近の「Windows の組み込みランタイムオブジェクトでは `E2002`」。`docs/architecture.md`: 240 行と 437 行の COFF の段落（trap の object の bitcode の連結と、G10 D8 のほかのランタイムの `E2002`）。
- Phase 2: `built-in-types-and-modules/domain.md`（13 行と 132 行の表の「Windows のネイティブ」）、`docs/language.md` の 1548 行付近の「Windows のネイティブ」。
- `grep -rnE "trap-mode return|E1038|Windows COFF" _tsuzuri/language-reference docs README.md` で残りを探す。[W03](W03-windows-interop-doc-fixes.md) が `E2002` の文の誤り（同梱ツールチェーンだけの制限と読める）を直していれば、その文に合わせる。

## 受け入れ条件

- [ ] 仕様 1〜7 のとおりに動き、テスト計画の段が MSVC と同梱 MinGW の `-O0`・`-O3` で通る。CI の `windows.yml` の x64・Arm64 で `tests/trap_return.mjs` が通る。
- [ ] POSIX の `trap.c`・`tzroutine.c` の生成コードと、POSIX の suite の結果が変わらない。trap ランタイム以外を埋め込む Windows の object は今の `E2002`。
- [ ] `--trap-mode return` を使わないプログラムの IR・header・診断が、どの target でも 1 byte も変わらない（`tests/fixtures`・`examples` の `--emit llvm`・`--emit header`・`check --json` を手順 1 で保存して比べる）。Windows の `tzroutine.c` の変更は C ランタイムの出力だけを変え、trap ランタイムの無いプログラムでは `tzr_trap_boundary` が今と同じく偽で、`abandon` も登録しない（設計の `tzroutine.c`）。
- [ ] 新しい診断コード・kind・オプションを足していない。Windows の trap の object が決定的（MSVC の triple）。
- [ ] 文書を直し、`check-docs`・`check-cli-docs` が通る。PR に `ci:full` を付けた。
- [ ] （Phase 2 を実装したとき）`Domain.run` が Windows で `run`・`build`・`test` に通り、`tests/domain.mjs` が Windows で通る。

## 落とし穴

- `-O0` だけで確かめない。`.pdata` が無くても `-O0` は偶然通る（測定）。必ず `-O3` を入れる。
- W07 の `uwtable` が無い IR では、Windows の境界は `-O3` でプロセスを落とす。`--emit llvm` の IR を古いコンパイラで作ったホストも同じ。
- `-mlink-bitcode-file` は cc1 の旗で、clang の driver の公開の旗ではない。使う所を 1 か所にし、版ごとの確認（S1）を CI に任せる。Clang 17〜20 で IR の入力に効くかは確かめていない（未確認）。
- `-mlink-bitcode-file` は連結した記号を内部にしない。Windows の trap ランタイムの関数は強い定義で、2 つの trap の object は重複の誤りになる（測定）。`weak` にしても直らない: COFF の弱い定義は object ごとに既定の名前が違い、2 つあれば `LNK1227`・`duplicate symbol` になる（`__multi3` で測定）。W05 の測定のとおり、DLL の export にも使えない。[W39](W39-windows-multiple-objects-weak-defaults.md) は `windows_abi` が返す IR の弱い定義を `comdat any` に入れてこの衝突をなくすが、bitcode で連結する trap ランタイムの関数に同じ形が使えるかは確かめていない（D4）。
- SRWLOCK は再帰しない（`pthread_mutex_t` の既定も同じ）。`tz_boundary` の lock を同じスレッドで 2 回取る経路が無いことは今も同じ。SRWLOCK は destroy が要らないが、`TZ_LOCK_DESTROY` は対称のために残す。
- `__attribute__((constructor))` は COFF では `.CRT$XCU` で走る。DLL では loader lock の下なので、`tz_boundary_install` にポインターの代入以外を足さない（W05 の落とし穴と同じ）。
- `tests/trap_return_host.c` の `-DCOUNTING` の版はホストが `trap.c` を強い定義で持つ。Windows の trap の object も強い定義を持つので、`COUNTING` の版を object と一緒にリンクすると重複の誤りになる（2 つの trap の object の測定と同じ形。この組そのものは試していない）。Windows では `COUNTING` の版を `--emit llvm` の経路だけで使う（テスト計画の表）。
- `tzr_trap_boundary` を Windows で真にすると、境界の中の `Parallel.*` は並列に走らない（POSIX と同じ。文書の既存の記述どおり）。

## 対象外

- trap ランタイム以外（tzroutine・CPU・IO・Net・OS・GPU）を埋め込む Windows の object（G10 D8。仕組みは提供インターフェースに残す）。複数の Tsuzuri の object を 1 つの Windows のホストに入れること（trap mode によらない今の制限。弱い定義の統合は G10 D8 の対象外で、[W39](W39-windows-multiple-objects-weak-defaults.md) が扱う）。
- コールバックとの併用（E14 の決定のまま `E2000`。ホストと OS のフレームを越える）。
- `--backend fast` の Windows（`E2000` のまま）。`--emit bitcode` の Windows（`E2000` のまま）。
- `cl.exe` で `trap.c` をコンパイルすること。CET のシャドウスタックとの関係（[W30](W30-windows-hardening.md) が確かめる）。
- Phase 2 は人間が求めたときだけ。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | E14 の Windows の境界の方針: Windows でも E14 D6 の `setjmp`・`longjmp` の境界を使い、`longjmp` は CRT の巻き戻し（OS の unwinder が `.pdata` を辿る）に任せる。W07 の `uwtable` を前提にする | 測定で `uwtable` があれば MSVC・MinGW の `-O0`・`-O3`、link.exe・lld-link、WndProc の中で状態 1 になった。Clang が C のコードに出すのと同じ `nounwind uwtable` の形で、Tsuzuri のフレームは unwinder から見て C のフレームと同じになる。退けた案: (a) 巻き戻さない `longjmp`（MinGW の `_setjmp(buf, NULL)`。測定では `.pdata` なしでも状態 1）は、MSVC の target の Clang が `setjmp` を常に frame 付きの `_setjmp` に下げるので toolchain ごとに別の仕組み（`__builtin_setjmp` の Arm64 の Windows の対応は未確認）が要り、OS の unwinder を通らないので CET のシャドウスタックとの整合も保証できない（未確認、W30）。`.pdata` はデバッガーと WER のために W07 がどのみち出す。(b) E14 D6 の代案（すべての呼び出しの後で旗を検査して戻る）は IR のすべての呼び出しと数値ランタイムを変える。(c) `invoke`・SEH の `__try` は D-10（例外を入れない）に反する | 要承認（承認前は該当する段に着手しない） |
| D2 | `trap.c` の排他は Windows で SRWLOCK（`tz_lock` のマクロ）、POSIX は今の `pthread_mutex_t` | MSVC の Clang でコンパイルでき（今は `pthread.h` が無い）、MinGW の winpthreads への依存も消える。`tzroutine-windows.h` と同じ。初期化は失敗しない。C11 の `<threads.h>` は UCRT と同梱の MinGW での可否を確かめていない | 既定案（実装者はこの案に従う） |
| D3 | Windows の object は、埋め込む C ランタイムが trap ランタイムだけのときに限り `E2002` を外す。ランタイムを bitcode にして、IR を object にする clang の実行に `-Xclang -mlink-bitcode-file` で連結する（G10 D8 の部分的な改訂） | COFF には再配置可能なリンクが無く（lld-link は `-r` を無視、zig は拒否）、`llvm-link` は Windows のどの配布物にも無い。この方法は 1 回の clang の実行で、ツールを増やさず、同梱 21.1.8 と LLVM 22.1.8 で動き、決定的だった。退けた案: ランタイムの object を別のファイルで出す（出力の契約が変わり、ホストが 2 つをリンクする）、静的ライブラリで出す（拡張子と中身が食い違う）、すべてのランタイムを同じ方法で埋め込む（tzroutine の TLS callback と複数 object の重複の扱いを含む G10 D8 全体の改訂で、このチケットの範囲を越える。`.CRT$XLB` が残ることは確かめたので材料として残す） | 要承認（承認前は手順 4 に着手しない） |
| D4 | Windows の trap ランタイムの公開関数は強い定義のまま（`TZ_TRAP_API` は空）。Windows の 1 つのホストに Tsuzuri の object は 1 つ（trap mode によらない今の制限）と文書に書く | G10 D8 と同じ扱い。trap mode が無くても、2 つの object の `__multi3` などの弱い定義が COFF で衝突する（測定）ので、trap ランタイムだけを弱くしても 2 つ目の object は入らない。`-mlink-builtin-bitcode` で object ごとに内部化する案は、入れ子の検出（状態 2）が object ごとになって POSIX と意味が変わる。COFF の `weak` は W05 の測定で DLL の export に使えない。[W39](W39-windows-multiple-objects-weak-defaults.md) が先に `main` に入っていれば、理由の前半（弱い定義の衝突）は成り立たないので、着手時にこの決定と仕様 3 を見直す（W39 の「他チケットへ提供するインターフェース」） | 既定案（実装者はこの案に従う） |
| D5 | `tzroutine.c` の Windows の `tzr_trap_boundary` と `abandon` の登録を POSIX と同じ意味にする。Windows の `tsuzuri_trap_hooks`・`tsuzuri_sync_hooks` は `tzroutine.c` では定義を持たない弱い参照にし、定義は `trap.c` の 1 つだけにする | 今の Windows の分岐は境界が無かったときの仮のもので、境界の中の `Parallel` が手伝いに配られて落ちる（測定）。`--emit llvm` と DLL の経路は tzroutine のランタイムを含む。POSIX と同じ弱い定義を両方に置くと、ホストが 2 つを別々にコンパイルする `--emit llvm` の経路で COFF の弱い定義が衝突する（同じ形を `__multi3` で測定）。弱い参照と 1 つの定義の組は 3 つのリンカーで通った | 既定案（実装者はこの案に従う） |
| D6 | W05 D4（`--trap-mode return` の Windows の DLL の `E2000`）を外す | W05 D4 の理由（`pthread.h` と `.pdata`）は D2 と W07 で両方とも消える。DLL は普通のリンクなので D3 の制限も無い | 既定案（実装者はこの案に従う） |
| D7 | `--emit llvm` の経路の `trap.c` は Clang でコンパイルすると文書に書き、`cl.exe` 向けの書き換えはしない | `cl.exe` は 72 行目の `__attribute__` で止まる（測定）。IR を object にするのにどのみち Clang が要る | 既定案（実装者はこの案に従う） |
| D8 | `Domain.run` の Windows は Phase 2 に分ける | Phase 1 の境界（D1・D5）の上に立ち、`tsuzuri test` のランナーと tzroutine の横取りの検証が増える。Phase 1 だけでホストの用途は満たせる | 既定案（実装者はこの案に従う） |
| D9 | 新しい診断コード・kind を足さない。Windows の `E2002` の文は変えない | D-51。今の文は trap ランタイムを挙げておらず、このチケットの後にちょうど正しくなる | 既定案（実装者はこの案に従う） |
