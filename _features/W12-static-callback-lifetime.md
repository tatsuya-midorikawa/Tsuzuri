# W12: 静的コールバックの寿命の契約（native ではプロセスの寿命の間呼べる）

| 項目 | 内容 |
| --- | --- |
| ID | W12 |
| 種別 | 機能 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | [W06](W06-windows-gui-test-harness.md)（GUI をヘッドレスに検査する基盤）、[W07](W07-windows-trap-fail-fast.md)（Windows のトラップを確実に終わらせる） |
| 後続 | [W22](W22-fnptr-values.md)、[W23](W23-anchor-state.md)、[W29](W29-callback-latency.md) |
| 状態 | todo |
| 起票 | 2026-11-22（D-63。利用者の依頼「X: D13 を狭く改める方針で決定。実現向けたチケットを @_features に W プリフィックスのチケットを作成して」と「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」から起票。HEAD `fa40eede` で確認） |
| 承認 | 要承認（docs/language.md の置き換え）。D1・D2 は要承認、D3〜D7 は既定案。台帳は [GUIDE D-63](GUIDE.md#d-63-tsuzuri-だけで-win32-のアプリを書くe12-d13-の狭い改訂w01w30) |
| 手本にする既存実装 | `src/llvm.rs` の `with_blocking_externs`（`tz.callback.*` の本体に `tsuzuri_tz_callback_enter`／`_exit` を差し込む）と `with_tzroutine_scopes`・`console_def_main`（境界の `tsuzuri_tz_scope_enter`／`tsuzuri_tz_boundary_leave`）、`src/runtime/tzroutine.c` の `tsuzuri_tz_scope_enter`・`tsuzuri_tz_boundary_leave`・`tzr_pseudo`・`tzr_pseudo_destroy`・`tzr_checkdead`、`tests/tzroutine.mjs` の TZ08 の段（`tests/fixtures/blocking/` の C のホストを `--link` する） |
| 主な影響ファイル | `src/llvm.rs`・`src/llvm_abi.rs`（`wrapper`）・`src/runtime/tzroutine.c`・`src/runtime/tzroutine-jspi.h`（空の実装）・`tests/blocking.rs`・`tests/ffi_extensions.rs`・`tests/tzroutine_runtime.c`・`tests/tzroutine.mjs`・`tests/fixtures/callback_lifetime/`（新規）・`tests/windows.mjs`・`docs/language.md`・`docs/architecture.md`・`_tsuzuri/language-reference/compiler/native-interop.md`・`_tsuzuri/language-reference/async-tasks-and-lazy/tzroutine.md`・`spec/rules/` |

## 実装者向け早見

- 範囲: E12 D8（承認済み）と D10 の「静的な寿命・いつでも呼べる」に合わせ、native の静的コールバックを「保持してよく、受け取った `extern` が返った後も、どのスレッドからも呼べる」と定める。生成コードの形（`ptr @tz.callback.<name>`）は変えない。足すのは 3 つの安全装置だけである: (1) 実行ファイルでは `main` が返った後の呼び出しを理由付きでトラップする、(2) どの境界にもいないスレッドで tzroutine を起動したコールバックを境界にする（D-61 決定 1）、(3) デッドロックの判定が、ホストのスレッドの擬似 G を Tsuzuri のコードの中にいる間だけ数える。
- 最初に読む: ① 「現状」の再現 4 つ ② `src/runtime/tzroutine.c` の `tsuzuri_tz_callback_enter`・`tzr_spawn_scope`・`tzr_checkdead` ③ 仕様 1〜4 ④ E12 の D8〜D10（`_features/_completed/E12-ffi-extensions.md`）。
- 最初に書くテスト: `tests/tzroutine_runtime.c` の `callback_scope_host_thread`（どの境界にもいないスレッドの `tsuzuri_tz_callback_scope_enter` が非 NULL を返し、`_leave` が子を待つ）。
- 触ってはいけない: WASM の出力と JS の糊（呼び出しの間だけ有効のまま）。コールバックを持たないプログラムの IR。`extern` の中の M のコールバックと、`main` の間の main スレッドのコールバックの扱い（今どおり、その G・`main` の境界に数え、待たない）。
- 完了の見分け方: 「現状」の再現 2 が `late tzroutine finished` を `main returns 42` より先に出し、再現 3 が 8 秒待たずに `all tzroutines are asleep - deadlock!` でトラップし、再現 4 が `a callback was called after main returned` でトラップする。再現 1 は今と同じく 42 で終わる。

## 目的

Win32 のウィンドウプロシージャ（`RegisterClassExW` が保持し、`DispatchMessageW`・`SendMessageW`・`CreateWindowExW` の中で呼ばれる）、`SetTimer` の TIMERPROC、フック、スレッドプールのコールバックは、渡した `extern` が返った後に呼ばれる。生成コードはすでにそれに耐える（静的な関数ポインターで、呼ばれる時を区別しない）が、文書は禁止している。契約を E12 の承認済みの決定に合わせ、契約の内側に入った新しい呼ばれ方で、tzroutine の構造化された並行（D-61 決定 1）とデッドロックの判定が崩れないようにする。W22（`FnPtr`）と W23（`Anchor`）はこの契約の上に立つ。

## 着手条件と停止条件

### 着手条件

- W06 と W07 が done（`grep -nE "^\| W0[67] " _features/README.md`）。W07 がないと、user32 が呼んだウィンドウプロシージャの中のトラップがプロセスを終わらせる保証がなく、仕様 2 のトラップを Windows で確かめられない。
- f8fe38cb（2026-10-02、「保持・戻った後の呼び出しの禁止」を足したコミット）の作者に、禁止の意図を確かめる。問い: 「E12 Phase 2 の捕捉のあるコールバック（呼び出しの間だけ貸す）のために余地を残したのか、検証した範囲（呼び出しの間に join するスレッド）に契約を合わせただけか」。答えをこのチケットの末尾に記録する。
- `grep -n "TZR_API int32_t tsuzuri_tz_callback_enter\|npseudo_running++\|static tzr_scope \*tzr_spawn_scope" src/runtime/tzroutine.c` が「現状」と同じ形であること。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- **S1** 作者が、禁止は今も必要な理由（Phase 2 の設計がこの契約と両立しないなど）で入れたと答えた。
- **S2** 仕様 4 の判定の変更で、既存の suite（`tests/tzroutine.mjs`、`tests/tzroutine_runtime.mjs`、`features.mjs` の `tzroutine`・`channel`・`sync_types`・`timers`、`tests/ffi_extensions.mjs` の 8 スレッドの同時呼び出し）がデッドロックと誤って判定される。判定を弱めて通さない。
- **S3** 仕様 3 の境界のために、既存のテストの期待値（`tests/fixtures/blocking` の出力、E12 の `threaded`）を変える必要がある。
- **S4** コールバックを持たないプログラムの IR、WASM の出力、tzroutine を起動しないライブラリ出力の IR が 1 byte でも変わる。
- **S5** W07 の後も、Windows で user32 が呼んだウィンドウプロシージャの中のトラップがプロセスを終わらせない。
- **S6** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

### 契約の文が 2 つに割れている

- `docs/language.md` の「コールバック」節: 490 行目「クロージャ変数の捕捉を持たないため、寿命やコンテキストの複雑な契約オーバーヘッドは生じません」、492 行目「ラッパー関数は名前のない `export def` と同様の規約で呼び出すことができ、再入呼び出し（reentrant call）にも対応しています」、493 行目の末尾「ただし、ホスト側が受け取った関数ポインタを勝手に長期保持したり、呼び出し元の extern 関数がリターンした後に非同期で呼び出したりすることは禁止されています」。同じ節が「寿命の契約はない」と「保持・戻った後の呼び出しは禁止」を同時に言っている。465 行目「例外unwind・非同期保持・回復不能な失敗からの復帰をTsuzuriのフレーム越しに行いません」は借用した入力について書かれた文で、コールバックを区別していない。
- `_tsuzuri/language-reference/compiler/native-interop.md` の「静的コールバック」節（210 行目）「`extern` の呼び出しが返ったあとに、そのポインタを非同期で呼んではいけません」。552 行目「呼び出しが返ったあとのポインタの保持…はしません」。
- E12 の D8（2026-10-02 承認）「捕捉がないので関数 pointer は静的な寿命を持ち、寿命の契約が要らない」、D10（既定案）「コールバックは名前のない `export def` と同じ規則で呼べる（再入可、いつでも呼べる）」。禁止を記録した決定はない。
- 経緯（`git show f8fe38cb`）: 「Keep the trap boundary state off the stack across setjmp」の PR の修正で、`docs/language.md` の「同じスレッドからの同期呼び出しと再入だけを検証しており、ホストの別スレッドから呼ぶときはホストが同期を保ってください」を、ホストのスレッドの検証の文と「ホストが渡された関数pointerを保持したり、呼び出し元のexternが戻った後に呼んだりしてはいけません」に置き換えた。同じコミットの E12 D10 の変更は検証の時期を分けただけで、制限を足していない。`17da9195`（2026-10-07）が native-interop.md の 210 行目の文を足した。

### 生成コードとランタイム

- `src/llvm_imports.rs` の `FunctionEmitter::host_call`: コールバックの実引数は `ptr @tz.callback.<修飾名>` の定数になり、`Globals::callbacks` に id を足す。`src/llvm.rs` の emit が特殊化の後に id 順で `host_abi::wrapper(…, WrapperKind::Callback, …)` を呼び、`src/llvm_abi.rs` の `wrapper` が `define internal … nounwind` の C ABI の包みを出す。登録・文脈・寿命の印はない。
- `src/llvm.rs`: `with_blocking_externs` は `spawning && !wasm` のときだけ働き、`define` の行に ` @tz.callback.` を含む関数と export の本体を `tsuzuri_tz_callback_enter`／`_exit` で挟む。`tzroutine_boundaries` は `tsuzuri_main`・`tsuzuri_test_run`・`tsuzuri_bench_sample`・`tz_<export>` だけで、コールバックは境界ではない。`console_def_main`・`console_main` は tzroutine を起動するプログラムでだけ `@tz.fn.<main>` の呼び出しを `tsuzuri_tz_scope_enter`／`tsuzuri_tz_boundary_leave` で挟み、`main` が終わった印はどこにも残さない。
- `src/runtime/tzroutine.c`: `tsuzuri_tz_callback_enter` は `tzr_this_m()` が NULL か `m->curg` が NULL か `m->syscall_depth == 0` なら何もせず 0 を返す（ホストのスレッドと main スレッドは M ではない）。`tzr_pseudo` は擬似 G を作るときに `tzr_sched.npseudo_running++` し、減らすのはスレッドの終わりの `tzr_pseudo_destroy` だけ。`tzr_checkdead` は `mcount - nmidle - nmlocked + npseudo_running > 0` なら判定しない。`tsuzuri_tz_go` は `tzr_spawn_scope(tzr_current())`（`spawn_scope`、なければ `scope`、なければ `tzr_root_scope`）へ起動し、`tzr_root_scope` を待つ境界はない。
- WASM: `src/runtime/bindings-core.mjs` の `callbackFunction` が `active` の印を持ち、`wrapImports` の `finally` が import の戻りで無効にする。後の呼び出しは `TypeError: the callback of import '<name>' is only valid during that import call`。

### 再現（開発版で確認済み）

Git Bash で `T=/c/Users/tmidorikawa/.vscode/extensions/tmidorikawa.tsuzuri-0.1.6-win32-x64/toolchain`、`B="env TSUZURI_CLANG=$T/bin/tsuzuri-clang.exe target/debug/tsuzuri.exe"`、作業は `target/scratch/w12/` の下。どれも `$B build <dir>/Main.tz --link <dir>/host.c -o <dir>/a.exe` の後に `timeout 8 <dir>/a.exe`。

1. **保持したポインターを別の extern から呼ぶ（今も動く）**: `extern "w_register" def register :: (i64 -> i64) -> unit`・`extern "w_fire" def fire :: i64 -> i64`（どちらも `@trusted`）、`def triple :: i64 -> i64 = \x -> x * 3`、`main` は `register triple` の後 `(fire 14) as i32`。ホストは `w_register` でポインターを静的変数に保存し、`w_fire` で呼ぶ。終了コード 42。IR は `call void @w_register(ptr @tz.callback.Main.triple)` と `define internal i64 @tz.callback.Main.triple(i64 %arg0) nounwind`。
2. **スレッドプールのコールバックが起動した tzroutine が `main` を越える**: `extern "w_pool_submit" def pool_submit :: (i64 -> i64) -> i64 -> unit`・`extern "w_pool_wait" def pool_wait :: unit -> i64`。コールバック `on_pool` は `tzroutine { Time.sleep (300 * Time.millisecond()); do! IO.writeln "late tzroutine finished" }` を起動して `x + 1` を返す。ホストは `TrySubmitThreadpoolCallback` で `on_pool 41` を呼び、`pool_wait` がその戻りを待つ。出力は `main returns 42` だけで終了コード 0（tzroutine は `tzr_root_scope` に入り、`main` は待たない。`Drop` も走らない）。同じコールバックを `main` のスレッドで同期に呼ぶ（`extern "w_call_now"`）と `main returns 42` の後に `late tzroutine finished` が出る（`main` の境界が待つ）。
3. **デッドロックの判定が止まる**: 再現 2 のコールバックの本体を `tzroutine { let _y = x }` にし、`pool_wait` の後で誰も送らない `Channel.make 0` を `Channel.recv` する。`timeout 8` で終了コード 124（止まったまま）。スレッドプールの呼び出しを消すと `Tsuzuri runtime: all tzroutines are asleep - deadlock! (1 waiting: Channel.recv)` でトラップする（終了コード 132）。プールのスレッドの擬似 G が、スレッドが生きている間ずっと走っていると数えられるため。
4. **`main` が返った後に呼ばれる**: 再現 1 のホストの `w_register` が `atexit(late)` を登録し、`late` が保存したポインターを `5` で呼んで表示する。出力 `after main: 15`、終了コード 42。何も報告されない。

### テストの置き場

`tests/ffi_extensions.mjs` を直接実行する workflow は HEAD にない（`tests/compact_strings_modes.mjs` の `full` の一覧にあるが、CI は `quick` だけを使う）。CI が実行するのは `tests/tzroutine.mjs`（`ci.yml` の `suites`・`targets.yml`。POSIX では `tests/fixtures/blocking/host.c` を `--link` する段がある）、`tests/tzroutine_runtime.mjs`、`tests/windows.mjs`（`windows.yml` の W5）である。

## 変えてはいけない意味

- コールバックの ABI と下げ方（E12 D9）: 呼び出し側の定数 `ptr @tz.callback.<name>`、包みの C の署名、ヘッダーの関数ポインターの引数。
- M の上の `extern` の中で呼ばれたコールバックは今どおり P を取り戻し、G をその M に固定し、その G の境界に数える（待たない）。`main` の間に main スレッドで呼ばれたコールバックは `main` の境界に数える（待たない）。ウィンドウプロシージャが tzroutine の終わりを待つことはない。
- export の境界、`--trap-mode return`・`Domain.run` とコールバックの組み合わせの拒否（`src/driver.rs`、`src/llvm.rs` の `domain_without_callbacks`）、コールバックの中のトラップがプロセスを終わらせること。
- WASM のコールバックは import の呼び出しの間だけ有効（JS の糊が無効にする）。

## 仕様

### 1. 契約（native）

| 出力 | 呼べる期間 | 呼べるスレッド |
| --- | --- | --- |
| 実行ファイル（`build` の `--emit exe`・`run`。生成した `@main` を持つ） | プログラムの開始から、`main` の境界（tzroutine の待ちを含む）が終わるまで。その後の呼び出しは仕様 2 のトラップ | どのスレッドからでも |
| ライブラリ（`--emit object`・`llvm`・`bitcode`、W05 の `shared`、ホストが `tsuzuri_main` を呼ぶ形）、`tsuzuri test`・`bench` のランナー | プロセスの寿命の間 | どのスレッドからでも |

- ホストは受け取った関数ポインターを保持してよく、受け取った `extern` が返った後に、別の `extern` の中からでも、どの `extern` の中でもなくても、何度でも、再入して、複数のスレッドから同時に呼んでよい（E12 D8・D10）。
- 「どのスレッド」は、OS とホストが作ったスレッド、スレッドプール、tzroutine の M を含む。ランタイムへのスレッドの登録は要らない（E12 の停止条件のとおり）。
- 対象外（コンパイラは検査できない。文書に禁止として書く）: 非同期の割り込みの文脈からの呼び出し。POSIX のシグナルハンドラー、特別なユーザー APC（`QueueUserAPC2` の `QUEUE_USER_APC_FLAGS_SPECIAL_USER_APC`）、ベクトル化例外ハンドラーと SEH のフィルター、`DllMain`・TLS コールバック、プロセスの終了処理（`exit`・`ExitProcess` の後の `atexit` と `DLL_PROCESS_DETACH`。実行ファイルでは仕様 2 が先に当たる）。ふつうの APC は `extern` の中の alertable な待ち（`SleepEx`・`WaitForSingleObjectEx`・`MsgWaitForMultipleObjectsEx` の `MWMO_ALERTABLE`）でだけ届くので、契約の内側である。
- W22 の `FnPtr` の値も同じ包みへのアドレスなので、同じ契約に従う。

### 2. `main` が返った後の呼び出し（実行ファイル、native）

`main` の境界が終わった後に静的コールバックが呼ばれたら、標準エラーへ `Tsuzuri runtime: a callback was called after main returned; stop the timers, threads and windows that call it before main returns` を書いてトラップする（`TrapKind` は増やさない。D-61 の「ランタイムが見つける誤り」と同じく `llvm.trap`。Windows では W04・W07 の経路で報告と終了が保証される）。ライブラリとランナーには適用しない（`tsuzuri_main` が返った後も export とコールバックは呼べる）。

### 3. 境界のないスレッドのコールバックは境界になる（tzroutine を起動するプログラム、native）

| コールバックが呼ばれた場所 | 起動した tzroutine が数えられる境界 | コールバックは待つか |
| --- | --- | --- |
| tzroutine の M の上の `extern` の中 | その G の境界 | 待たない（今どおり） |
| `main` の間の main スレッド | `main` | 待たない（今どおり） |
| export の呼び出しの中のホストのスレッド | その export | 待たない（今どおり） |
| それ以外（スレッドプール、ホストが作ったスレッド、ライブラリを使うホストの GUI スレッド） | **そのコールバックの 1 回の呼び出し** | **待つ**（export と同じ。子とタイマーを片付けてから戻る） |

判定は「現在の G（擬似 G）が `spawn_scope` も `scope` も持たない」こと。待ちの中のデッドロックは `tsuzuri_tz_boundary_leave` と同じくトラップする。

### 4. デッドロックの判定

擬似 G は、入口（`main`・export・`test`・`bench` の境界、またはコールバック）を通って Tsuzuri のコードに入っている間だけ「走っている」と数える。入口の外にいるホストのスレッドは数えない。入口を通らずに作られた擬似 G（tzroutine を起動しないプログラムが export の中で `Channel` を使うときなど。そうしたプログラムには入口の印がない）は、今どおりスレッドの終わりまで数える。

### 5. WASM

変えない。コールバックは import の呼び出しの間だけ有効で、仕様 2〜4 の生成も出さない（JSPI ではコールバックの中で待てないので、仕様 3 は起きない）。

## 設計

### 生成コード

- **`main` の後の印**（仕様 2）: native の `Entry::Console` で、包みを 1 つでも出すとき（`Globals::callbacks` を空にする前に数を控える）だけ、`@tz.main.done = internal global i32 0` を出す。`console_def_main`・`console_main` は、`main` の関数（IO の入口では `tsuzuri_main`）が戻り、tzroutine を起動するプログラムでは `tsuzuri_tz_boundary_leave` が戻った直後に `store atomic i32 1, ptr @tz.main.done release, align 4` を出す。`wrapper` は `WrapperKind::Callback` の入口で `load atomic i32, ptr @tz.main.done acquire, align 4` と分岐を出し、真なら `@tz.callback.after_main()`（`internal cold noreturn nounwind`。メッセージを `src/llvm_traps.rs` の `@tz.trap.write`（`TRAP_WRITE`。Windows では `windows_abi` の `write` の shim と W04 の経路）で書いて `llvm.trap`。トラップの報告が同じ IR に `@tz.trap.write` を出していればそれを使い、なければ同じ定義を 1 つだけ出す。`declare` を重複させない）を呼ぶ。`wrapper` は `Entry` を知らないので、`Globals` に「印を出す」旗を持たせる。
- **コールバックの境界**（仕様 3）: `with_blocking_externs` が `tz.callback.*` の定義（export でない方）に限り、`tsuzuri_tz_callback_enter` の直後に `%tz.host = call ptr @tsuzuri_tz_callback_scope_enter()`、各 `ret` の前で `tsuzuri_tz_callback_exit` より前に `call void @tsuzuri_tz_callback_scope_leave(ptr %tz.host)` を差し込む。宣言は使ったときだけ 1 回出す。

### ランタイム（`src/runtime/tzroutine.c`）

```c
/* in tzr_g, for a pseudo-G (W12) */
int host_depth;   /* entries (boundaries and callbacks) the thread is in */
int outside;      /* 1 while not counted in npseudo_running: it left its last entry */
```

| 関数（`TZR_API`） | 意味 |
| --- | --- |
| `void *tsuzuri_tz_callback_scope_enter(void)` | 現在の G が擬似 G なら入口に入る（下）。`spawn_scope` も `scope` もなければ `tsuzuri_tz_scope_enter` と同じスコープを作って返す。それ以外は NULL |
| `void tsuzuri_tz_callback_scope_leave(void *scope)` | 非 NULL なら `tsuzuri_tz_boundary_leave(scope)`（待ちとタイマーの片付け）。その後、擬似 G なら入口を出る |

- 入口に入る（`tzr_host_enter`）: `tzr_sched.lock` の下で `outside` なら 0 にして `npseudo_running++`、`host_depth++`。入口を出る（`tzr_host_leave`）: `--host_depth == 0` なら `outside = 1`、`npseudo_running--`、`tzr_checkdead()`。`tsuzuri_tz_scope_enter`（生成コードでは `main`・export・ランナーの境界。`tests/tzroutine_runtime.c` も直接呼ぶ）と、対になる `tsuzuri_tz_boundary_leave`・`tsuzuri_tz_scope_leave` も、擬似 G のときに同じ 2 つを呼ぶ（対になっているので深さは釣り合う）。`tzr_pseudo_destroy` は `outside` でなければ `npseudo_running--` する。擬似 G は作られたときに数えられ（今どおり）、入口を通らない限り `outside` にならない。
- **`outside` の擬似 G がランタイムに戻ったら数え直す**: 入口を出た後も、同じスレッドが入口を通らずにランタイムを呼ぶことがある（`tests/tzroutine_runtime.c` は `main` と pthread からチャンネルの操作を直接呼ぶ。入口の印のないプログラムの擬似 G も同じ形）。`tzr_pseudo()`（`tzr_current()` が M でないスレッドで呼ぶ）は、返す擬似 G が `outside` なら、`tzr_sched.lock` の下で `outside = 0`・`npseudo_running++` してから返す（`outside` の確かめはロックなしの読みで、ロックは数え直すときだけ）。`tzr_host_leave` はこの後に `tzr_current()` を呼ばない（`callback_scope_leave` は `tsuzuri_tz_boundary_leave` を先に呼び、入口を出るのを最後にする）。これで `tzr_park` が数えられていない擬似 G の `npseudo_running` を減らすことはない。
- `tzroutine-jspi.h` には何もしない 2 つの関数（`scope_enter` は NULL）を足す（JSPI には `with_blocking_externs` がかからないが、記号の集合をそろえる）。

### 判定が誤らない理由と、後続への義務

待っている G・擬似 G を起こせるのは、その待ちの対象（チャンネル、ロック、`WaitGroup`、タイマー…）に届く Tsuzuri のコードだけである。入口の外にいるスレッドは Tsuzuri の値に届かない: モジュール共有の可変状態はなく（`docs/language.md` 446 行目）、ABI の型は Tsuzuri の値を運ばない（ハンドルはホストが作る。`Atomic`・`Rc`・`Arc` は `E1008`、`const` に入らない `E1026`）。入口に入れば数えられ、そこで届くのはその入口で作った値だけである。タイマー・ネットワークの待ち・ブロッキング呼び出しの M は今どおり別に数える。**W23 の `Anchor` は別のスレッドからの読み書きを `WrongThread` にするので、この経路を増やさない。** 別のスレッドの値への経路を足す機能（スレッドをまたぐ `Anchor`、W26 の COM のアパートメントをまたぐ呼び出しなど）は、この規則を見直す義務を負う。

### 採らなかった案

| 案 | 採らない理由 |
| --- | --- |
| 文書だけ直す（仕様 2〜4 なし） | 再現 2〜4 の、境界を越える tzroutine（`Drop` が走らない）、止まる判定、`main` の後の黙った実行が契約の内側に入る |
| 境界のないコールバックの tzroutine を、`main` の境界が待つプロセスのスコープに入れる | `main` の後とライブラリでは待つ者がいない |
| コールバックの中の起動を静的に拒む | どのスレッドで呼ばれるかは型から分からない |
| `main` の後に呼んだスレッドを止めたままにする | `atexit` のように main スレッド自身が呼ぶと止まったままになる |
| ホストのスレッドを登録させる | E12 の停止条件（登録を要らないことが前提） |

## 前提とする他チケットのインターフェース

- W06: C の shim `tests/windows_gui_shim.c`（`tz_gui_pump`・`tz_gui_send`・`tz_gui_window`・`tz_gui_wait`・`tz_gui_post`・`tz_gui_create`・`tz_gui_mark`。窓とメッセージは Tsuzuri の側で `i64`）、`tests/windows.mjs` の `buildGui(fixture, flags)`、fixture の置き場 `tests/fixtures/windows_gui/`、5 秒の見張りのタイマー。W12 は shim に関数を足す（W06 の仕様 4 の形で。案: `tz_gui_register_class(proc)`（`RegisterClassExW` に包みのアドレスを入れて窓を作る）、`tz_gui_destroy_on_close`、`SetTimer` の TIMERPROC は W07 が足す `tz_gui_timer_proc` を使い（同じ働きの関数を重ねない。W07 の形が回数の検査に足りなければ引数を足して共有する）、`tz_gui_pool_submit(proc, value)`・`tz_gui_pool_wait()`（`TrySubmitThreadpoolCallback`）、`tz_gui_call_after_main(proc)`（`atexit`））。
- W07: Windows で `tz.trap.report` の後に fail-fast で終わり、VEH が Tsuzuri のコードの範囲の `STATUS_ILLEGAL_INSTRUCTION` を拾うこと（user32 のフィルターに飲まれない）。仕様 2 のトラップも同じ経路を通る。

## 他チケットへ提供するインターフェース

- 仕様 1 の契約（W22 の `FnPtr`、W23、W26、W27 はこれを前提にする）。
- **包みは 1 種類**: `@tz.callback.<修飾名>` に到達する経路（W22 の `FnPtr.of` を含む）は、`Globals::callbacks` に id を足して同じ `wrapper` を使う。仕様 2・3 の印はこの包みにあるので、新しい経路は自動で同じ扱いになる。別の包みを作らない。
- ランタイムの `tsuzuri_tz_callback_scope_enter`／`_leave` と擬似 G の `host_depth`（W29 はこの経路を含めて遅延を測る）。
- 判定の義務（上の節）: W23・W26 は、別のスレッドの値への経路を作らないか、作るならこの規則を改める。

## 実装手順

1. **変更**: なし（ブランチ `feat/w12-static-callback-lifetime`、`target/scratch/w12/` の下書きだけ）。**内容**: `node scripts/repo-kb.mjs brief W12`。着手条件の確認と作者への問い。`tests/fixtures/*`・`examples/*` の `Main.tz` の `--emit llvm`（native と `--target wasm32`）と、再現 1〜4 の出力を保存する。**確認**: 保存した。
2. **変更**: `src/runtime/tzroutine.c`・`tzroutine-jspi.h`。**内容**: 擬似 G の入口の数え方と 2 つの関数。`tests/tzroutine_runtime.c` に「テスト計画」の C のケース。**確認**: `TSUZURI_TSAN=1 TSUZURI_ASAN=1 node tests/tzroutine_runtime.mjs`、`sh scripts/check-runtime-includes.sh`、既存の `tzroutine.mjs` が通る。
3. **変更**: `src/llvm.rs`（`with_blocking_externs`）。**内容**: コールバックの境界の差し込み。**確認**: `cargo test --locked --test blocking`（HEAD の 6 件と新しいケース。`running N tests` を見る）。
4. **変更**: `src/llvm.rs`（`console_def_main`・`console_main`・`Globals`）、`src/llvm_abi.rs`（`wrapper`）。**内容**: `main` の後の印。**確認**: `cargo test --locked --test ffi_extensions`。手順 1 の IR と比べ、変わったのが「コールバックを持つ実行ファイル」と「コールバックを持ち tzroutine を起動するプログラム」だけであること（`--emit llvm` はライブラリの入口なので、コールバックを持つ非 spawn の fixture は一致する）。
5. **変更**: `tests/fixtures/callback_lifetime/`（新規。`Main.tz` と POSIX の `host.c`）、`tests/tzroutine.mjs`。**確認**: `node tests/tzroutine.mjs target/debug/tsuzuri`（`-O0`・`-O3`、P 1・4）。
6. **変更**: `tests/windows_gui_shim.c`（上の関数）、`tests/fixtures/windows_gui/` の fixture、`tests/windows.mjs`（W06 の GUI の段に足す）。**確認**: Windows で `node tests/windows.mjs target/release/tsuzuri.exe`。PR に `ci:full`。
7. **変更**: 文書と規則 ID。**確認**: T2、`node scripts/check-docs.mjs <変更したページ>`。

## テスト計画

| 置き場（CI のジョブ） | ケース | 期待 |
| --- | --- | --- |
| `tests/tzroutine_runtime.c`（`tzroutine_runtime.mjs`。ci・targets・windows） | `callback_scope_host_thread`: pthread（Windows はスレッド）が `callback_scope_enter` → `tsuzuri_tz_go` → `callback_scope_leave` | `enter` が非 NULL、`leave` が子の終わりまで戻らない（子が立てる印で見る） |
| 同上 | `callback_scope_nested`: export の境界（`tsuzuri_tz_scope_enter`）の中と、tzroutine の M の上で呼ぶ | NULL を返し、子はその境界に数えられる |
| 同上 | `pseudo_outside_detector`: ホストのスレッドが入口に入って出た後、main の擬似 G が誰も送らないチャンネルで待つ（子プロセス） | デッドロックの文でトラップ（止まらない） |
| 同上 | `pseudo_without_entry`: 入口を通らずに作った擬似 G が走っている間、別の擬似 G が待つ | 判定しない（今どおり） |
| 同上 | `pseudo_reenters_runtime`: main の擬似 G が境界を出て（`outside`）から、入口を通らずにチャンネルで待ち、別のスレッドが送って起こす。これを 1,000 回 | 誤った判定も止まりもしない。`npseudo_running` が負にならない（デバッグの `assert`）、終わりに 0 と `npseudo_parked` 0 |
| `tests/blocking.rs`（cargo） | spawn するプログラムの `tz.callback.*` に `callback_scope_enter` が `callback_enter` の後、`_leave` が各 `ret` の `callback_exit` の前にあり、export の包みにはない | 文字列の検査 |
| `tests/ffi_extensions.rs`（cargo） | `llvm::emit` を `Entry::Console` と `Entry::Library` で: Console だけ `@tz.main.done` と包みの入口の検査があり、`@tz.callback.after_main` の定義は 1 つ。コールバックのないプログラムと wasm の IR は印を持たない。IR が決定的 | 文字列の検査 |
| `tests/fixtures/callback_lifetime` + `tests/tzroutine.mjs`（ci の suites。POSIX） | `stored`: 再現 1 の形（登録した extern と別の extern から呼ぶ） | 42 |
| 同上 | `after_return_thread`: 登録した extern が返った後、ホストが作ったスレッドから 1,000 回呼ぶ | 合計がホストの C の参照値と一致、`live == 0` |
| 同上 | `pool_spawn_waits`: 境界のないスレッドのコールバックが tzroutine を起動し、その子が `extern "host_mark"` で印を立てる | コールバックが戻った時点でホストが印を見る（時間で判定しない） |
| 同上 | `outside_detector`: 再現 3 の形（POSIX のスレッド） | デッドロックの文でトラップ。suite の `timeout` 内 |
| 同上 | `after_main`: 再現 4 の形 | 仕様 2 の文でトラップ（`SIGILL`・終了コードはトラップのもの） |
| `tests/windows.mjs`（windows.yml W5。W06 の段） | ウィンドウプロシージャを shim の `tz_gui_register_class` で `RegisterClassExW` に登録（Tsuzuri の包みのアドレスを渡し、登録した extern は返る）し、Tsuzuri から `CreateWindowExW`（`WM_CREATE`）・`SendMessageW`・`PostMessageW` と `DispatchMessageW` で呼ぶ | 各メッセージの数がホストの参照値と一致 |
| 同上 | `WM_CLOSE` の中で `DestroyWindow` を呼び、同じプロシージャへ `WM_DESTROY` が再入する | 再入の順序が参照どおり |
| 同上 | `SetTimer` の TIMERPROC と `TrySubmitThreadpoolCallback` | 回数一致。プールのコールバックの tzroutine を待ってから戻る |
| 同上 | 再現 3・4 の Windows 版 | デッドロックの文・仕様 2 の文で終わる（W07 の終了コード） |

`-O0`・`-O3` の両方。時間の上限で合否を決めない（AGENTS.md）。

## ドキュメント

- `docs/language.md` の「コールバック」節: 490 行目の文を「寿命の契約」の説明に、493 行目の禁止を仕様 1 の表と対象外の列挙に置き換え、仕様 2・3 を書く。465 行目の「非同期保持」は「借用した入力と所有を渡していないバッファ」の話だと限定し、静的コールバックを除くと書く。
- `native-interop.md` の「静的コールバック」節（210 行目）と 552 行目。
- `tzroutine.md` の「終了の待ち」の表に「境界のないスレッドでのコールバックの 1 回の呼び出し」の行、「OS を待つ呼び出し」のコールバックの項（301〜302 行目）に仕様 3、デッドロックの説明に仕様 4。
- `webassembly.md` の 174 行目（import の呼び出しの間だけ）は変えない。native との違いへのリンクを足す。
- `docs/architecture.md`（コールバックの包み、擬似 G の数え方）。
- `spec/rules/TSK.md`・`ABI.md`: 着手時の番号の次に「A static callback MAY be called after the extern that received it returned (native).」「A callback on a thread outside every boundary MUST NOT return before the tzroutines it spawned have finished.」など。
- 完了済みの E12 は書き換えない（台帳 D-63 はオーケストレーターが書く）。

## 受け入れ条件

- [ ] 着手条件の作者の答えを記録した。
- [ ] 仕様 1〜5 のとおりに動き、「テスト計画」がすべて通る（`-O0`・`-O3`、P 1・4）。
- [ ] コールバックを持たないプログラムの IR・object・header・診断、WASM の出力、tzroutine を起動しないライブラリ出力の IR が手順 1 と byte 単位で一致する。変わるのは、コールバックを持つ実行ファイル（仕様 2 の印）と、コールバックを持ち tzroutine を起動するプログラム（仕様 3 の差し込み）だけで、その変更を PR の説明に書いた。
- [ ] `TSUZURI_TSAN=1 TSUZURI_ASAN=1 node tests/tzroutine_runtime.mjs` が通る。
- [ ] 文書・規則 ID を更新し、`check-docs` が通る。`docs/language.md` の「コールバック」節の寿命の 2 文（490・493 行目付近の「寿命の契約はない」と「保持・戻った後の非同期の呼び出しは禁止」）と `native-interop.md` の 210 行目・552 行目を、仕様 1 の契約に置き換えた（この文はこのチケットだけが書き換える。W01・W03 は触らない）。
- [ ] PR に `ci:full` を付け、`windows.yml` が緑。

## 落とし穴

- **差し込みの順序**: 入るときは `callback_enter` → `callback_scope_enter`、出るときは `callback_scope_leave` → `callback_exit`。逆にすると、M の上で P を持たないまま待つ。
- **印は境界の後**: `main` の境界の待ちの間も、`main` が起動した tzroutine がコールバックを受けることがある。`@tz.main.done` は `tsuzuri_tz_boundary_leave` が戻ってから立てる。
- **入口の深さ**: 擬似 G が待っている（`TZR_G_WAITING`）間は `tzr_park` が `npseudo_running` を減らしている。`tzr_host_leave` と `tzr_pseudo_destroy` が二重に減らさないこと。`outside` の擬似 G が入口を通らずに `tzr_park` に入る経路は、`tzr_pseudo()` の数え直しが先に塞ぐ（数え直しを `tzr_park` の中だけに置くと、`tzr_current()` を経ない待ちの経路を見落とす）。
- **`with_blocking_externs` は extern の記号がなければ何もしない**: コールバックの包みがあるなら記号もあるが、W22 で「`extern` に渡さない `FnPtr`」ができたときも、包みは呼ばれないので問題はない。
- **`--trap-info`**: `@tz.callback.after_main` は位置を持たない補助関数。`@tz.tzroutine.owned.drop` と同じく計装から外す。
- **codegen units**（`src/llvm_split.rs`）: `@tz.main.done` は可変の大域なので 1 つの単位が持ち、ほかは参照する。定数のように複製されないことを `TSUZURI_CODEGEN_UNITS=split` で確かめる。

## 対象外

- 捕捉のある関数値のコールバック（E12 Phase 2。呼び出しの間だけ貸す設計のまま）。
- WASM で保持できるコールバック（`extern` の印と糊の変種が要る）。
- コールバックごとの回復可能なトラップ、非同期の割り込みからの呼び出し、UI スレッドへ処理を送る仕組み、メッセージを回しながら待つ待ち。
- コールバックの遅延の短縮（W29）。ARM64 Windows での実行の検証（CI の windows-11-arm で `tests/windows.mjs` が動く範囲だけ）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | 仕様 1 の契約: native の静的コールバック（と `FnPtr`）は保持してよく、実行ファイルでは `main` の境界の終わりまで、ライブラリとランナーではプロセスの寿命の間、どのスレッドからでも呼べる。`docs/language.md` 493 行目と `native-interop.md` 210 行目の禁止を置き換える | E12 D8（承認済み）・D10 の「静的な寿命・いつでも呼べる」と生成コードがすでに一致している。禁止を記録した決定はない（f8fe38cb の作者に確かめる） | 要承認（承認前は手順 7 の文書に着手しない） |
| D2 | 実行ファイルで `main` の後に呼ばれたら、生成コードの印で理由を書いてトラップする。コールバックを持つ実行ファイルの IR は変わる | 境界を出た後の呼び出しは、片付けの済んだランタイムの上で `Drop` のない tzroutine を作りうる。黙って動かさない（GUIDE §1 規則 2）。代わりの案は「文書で禁止するだけ」 | 要承認（承認前は手順 4 に着手しない） |
| D3 | 境界のないスレッドのコールバックは、その 1 回の呼び出しが境界（export と同じく待つ） | D-61 決定 1（境界の中の tzroutine は境界より長く生きない）。M・main スレッド・export の中は今の境界のまま | 既定案（実装者はこの案に従う） |
| D4 | デッドロックの判定は、擬似 G を入口の中にいる間だけ数える。入口を通らない擬似 G は今どおり | 入口の外のスレッドは Tsuzuri の値に届かないので、待つ者を起こせない（「判定が誤らない理由」）。W23 の `WrongThread` がこれを保つ | 既定案（実装者はこの案に従う） |
| D5 | 非同期の割り込みの文脈と終了処理からの呼び出しは対象外（文書で禁止） | 実行中の G の途中に割り込み、ランタイムやアロケーターの中で走りうる。コンパイラは検査できない | 既定案（実装者はこの案に従う） |
| D6 | WASM は変えない（import の呼び出しの間だけ） | 糊が無効にする今の挙動を、印のない宣言で変えない。保持できる WASM のコールバックは別チケット | 既定案（実装者はこの案に従う） |
| D7 | ライブラリ出力と `test`・`bench` のランナーは仕様 2 の対象外（プロセスの寿命） | 生成した `@main` がなく、`tsuzuri_main` が返った後も export を呼べる | 既定案（実装者はこの案に従う） |
