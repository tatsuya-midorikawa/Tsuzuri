# W30: Windows の実行ファイルのセキュリティの硬化（ASLR・NX・CET・CFG）

| 項目 | 内容 |
| --- | --- |
| ID | W30 |
| 優先度 | P2（改善。Tsuzuri のコードはメモリ安全だが、同じプロセスの CRT・ホストの C・OS の DLL への攻撃を、OS の緩和策で受け止める） |
| 規模 | M（Phase 1: 状態の記録と固定のテスト、CET の既定。Phase 2: CFG の表の既定と検査の opt-in） |
| 依存 | W07（Phase 2: W25） |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-22（D-63。利用者の依頼「X: D13 を狭く改める方針で決定。実現向けたチケットを @_features に W プリフィックスのチケットを作成して」と「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」から起票。HEAD `fa40eede` で確認） |
| 承認 | 要承認（既定値の変更）。D1〜D3 が要承認、ほかは既定案。人間が求めない限り Phase 1 だけを実装する |
| 改善する劣位 | 硬化の状態が記録も検査もされていない。ツールチェーンの更新で既定が外れても気づかない。CET のシャドウスタックと CFG は、Tsuzuri が作る実行ファイルでは一度も有効になっていない |
| 手本にする既存実装 | `src/driver.rs` の `final_link_args_for`（最終リンクだけの引数）・`msvc_linker_for`（MSVC と同梱の MinGW の区別）・`BuildOptions::validate`（使えない組み合わせの `E2000`）、`tests/windows.mjs`（MSVC の E2E と、W06 の `--stage gui`）、W06 の `tests/windows-pe.mjs`（`llvm-readobj` の結果を読む関数） |
| 主な影響ファイル | `src/driver.rs`, `src/main.rs`, `src/llvm.rs`（Phase 2 のモジュールの旗）, `src/llvm_debug.rs`（旗の併合）, `tests/windows.mjs`, `tests/windows-pe.mjs`（W06。関数を 2 つ足す）, `_tsuzuri/language-reference/compiler/platforms.md`, `compiler/option.md`, `compiler/native-interop.md`, `README.md`（CLI の表）, `docs/architecture.md` |

## 実装者向け早見

- 範囲: Windows の native の実行ファイル（W05 の DLL が出れば DLL も）。Phase 1 は、今の ASLR・NX の状態をテストで固定し、MSVC でリンクする x64 の実行ファイルに CET 互換（`/CETCOMPAT`）を既定で付ける。Phase 2 は、CFG の表（`cfguard` の旗 1 と `/guard:cf`）を既定にし、検査（旗 2）を opt-in にする。
- 最初に読む: 「現状」の 2 つの表 → 決定事項 D1〜D3 → `final_link_args_for`。
- 最初に書くテスト: `tests/windows.mjs` の PE の旗の検査（今の値で通る）。
- 触ってはいけない: POSIX と WASM の出力。同梱の MinGW の実行ファイル（ツールチェーンが旗を受け付けない）。既定の IR（Phase 1 は IR を変えない）。
- 完了の見分け方（Phase 1）: MSVC の x64 で `tsuzuri build examples/tzroutines -o t.exe` の `llvm-readobj --coff-debug-directory t.exe` が `IMAGE_DLL_CHARACTERISTICS_EX_CET_COMPAT` を出し、`t.exe` が今と同じ出力を出す。

## 目的

GUI のアプリは、シェルの拡張・COM のサーバー・IME など、他人の DLL を自分のプロセスに読み込む。Tsuzuri のコードがメモリ安全でも、同じプロセスの C のコードの欠陥は戻りアドレスや関数ポインターの書き換えに使われうる。OS が持つ緩和策（ASLR、DEP、ハードウェアのシャドウスタック、Control Flow Guard）は、実行ファイルの印で有効になる。このチケットは、何が有効かを記録して固定し、互換性を確かめたものから既定にする。

## 着手条件と停止条件

### 着手条件

- Phase 1: W07 が `main` にある（トラップの fail-fast と VEH の上で CET を確かめる）。D1・D2 が承認されている（承認前は手順 1・2 まで）。
- Phase 2: W25 が `main` にある（COM の vtable の間接呼び出しを検査の対象に含める）。D2・D3 が承認されている。

### 停止条件

- **S1** CET 互換の実行ファイルで、既存の Windows のテスト（`tests/windows.mjs`、`tests/examples.mjs`、tzroutine の段）のどれかが CET の使える機械で失敗する。
- **S2** MSVC のリンカー（link.exe 14.44、`windows.yml` の版）が `/CETCOMPAT` か `/guard:cf` を受け付けない。
- **S3** CFG の表を既定にして、W06 の WndProc の段、W07 のコールバックの段、W25 の COM の段のどれかが 0xC0000409（CFG の失敗）で終わる。
- **S4** POSIX か WASM の出力が変わる。Phase 1 で Windows の IR が変わる。
- **S5** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

- `src/` に `dynamicbase`・`nxcompat`・`high-entropy`・`guard:cf`・`cetcompat`・`cfguard` は 0 件（`grep -rni`）。リンクの旗はツールチェーンの既定に任せている。ビルド機の Windows の MSVC の実行ファイルは Clang の既定のリンカー（link.exe）でリンクする（`link_args` が `-fuse-ld=lld` を付けるのは別の target のときだけ）。同梱の `tsuzuri-clang` は zig 0.16 の `x86_64-windows-gnu`（UCRT の DLL）でリンクする（`msvc_linker`）。

### 今の実行ファイル（開発版、`-O3`、Windows 11 26300）

| 項目 | MSVC（link.exe 14.44、`hello`・`examples/tzroutines`） | 同梱の MinGW（zig 0.16、`hello`） |
| --- | --- | --- |
| `DYNAMIC_BASE`（ASLR） | あり | あり |
| `HIGH_ENTROPY_VA` | あり | あり |
| `NX_COMPAT`（DEP） | あり | あり |
| `GUARD_CF`（CFG） | なし。load config はあり（`SecurityCookie` あり、`GuardCFFunctionCount` 0、`GuardFlags` 0x100 は CRT の object が持つ計装の印で、`DllCharacteristics` に `GUARD_CF` が無いので CFG は働かない） | なし。load config 自体が無い |
| CET 互換（`EX_CET_COMPAT`） | なし（拡張の DLL の性質の debug directory が無い） | なし |

確かめ方: `llvm-readobj --file-headers`・`--coff-load-config`・`--coff-debug-directory`、`dumpbin /headers /loadconfig`。

### 確かめたこと

| 試したこと | 結果 |
| --- | --- |
| C の試験（Fiber を 10 万回切り替え、`GetProcessMitigationPolicy(ProcessUserShadowStackPolicy)`）を `/CETCOMPAT` あり・なしでリンク（AMD Ryzen AI 7 PRO 350、`IsUserCetAvailableInEnvironment` が真） | あり: `EnableUserShadowStack=1` で完走。なし: 0。link.exe の `/CETCOMPAT` と lld-link の `-cetcompat` の両方 |
| `tests/fixtures/tzroutine` の `--emit llvm --trap-info -O3` と `src/runtime/tzroutine.c` と C のホスト（`tz_sum_atomic` など 4 つを 200 回）を、link.exe `/CETCOMPAT` と lld-link `-cetcompat` でリンク | `CET compatible` の印が付き、どちらも完走 |
| 同梱の `tsuzuri-clang` に `-Wl,--cetcompat`・`-Wl,-cetcompat`・`-Wl,/cetcompat`・`-Wl,--guard-cf`・`-Wl,/guard:cf`（`MSYS_NO_PATHCONV=1`） | どれも zig が `unsupported linker arg` で拒否。`--dynamicbase`・`--high-entropy-va`・`--nxcompat` は受け付ける |
| `/guard:cf` の C のホスト（`-Xclang -cfguard`）が、Tsuzuri の object のコールバック（`ptr @tz.callback.Main.on_event`）を CFG の検査付きの間接呼び出しで呼ぶ。object は表なし・`cfguard` 1 の表ありの 2 通り、link.exe と lld-link | 4 通りとも完走（42）。表なしの object は、両方のリンカーがその関数をすべて有効な呼び先と見なした（`GuardCFFunctionCount` 0x4A、表ありは 0x48） |

- 実行時の CET の互換性: 生成コードは戻りアドレスを書き換えない。Windows の tzroutine は Fiber（`tzroutine-windows.h` の `tzr_fiber_create`・`SwitchToFiber`）で、OS がシャドウスタックを切り替える。`trap.c` の `longjmp` は Windows では連結されない（`--trap-mode return` のランタイム入りの COFF は `E2002`、`Domain.run` は `E1038`）。非同期のプリエンプション（`TZR_ASYNC`）は macOS と Linux だけ。W07 の VEH と `RaiseFailFastException` を CET の下で動かすことは**未確認**。GitHub の Windows のランナーの CPU が CET を持つかも**未確認**。

## 仕様

### Phase 1（W07 の後）

1. ASLR・`HIGH_ENTROPY_VA`・DEP は、両方のツールチェーンの既定のまま有効であることを、テストで固定する（旗は渡さない。D5）。
2. MSVC でリンクする（`msvc_linker_for(platform)` が真）x86-64 の実行ファイルは、既定で CET 互換の印を持つ（link.exe と lld-link に `/CETCOMPAT`）。W05 の DLL も同じ。
3. 新しいオプション `--cet-compat <on|off>`（`build`・`run`・`test`・`bench`。D2）。`off` は印を付けない。`on` を明示したのに付けられない target（同梱の MinGW、aarch64、Windows 以外、実行ファイルでない出力）は `E2000`（kind 案 `windows_hardening_unavailable`）。指定しないときは、付けられる所でだけ付け、付けられない所では黙って付けない（文書に書く。AGENTS の「自動の選択には能力の確認と文書にした代替」）。

### Phase 2（W25 の後）

4. MSVC でリンクする実行ファイルは、既定で CFG を有効にする: IR にモジュールの旗 `!{i32 2, !"cfguard", i32 1}`（表だけ、生成コードに検査なし）、リンクに `/guard:cf`。Windows の `--emit object`・`--emit llvm` も表を持つ（ホストが `/guard:cf` でリンクしなければ働かない）。
5. 新しいオプション `--cf-guard <off|tables|checks>`。`checks` は旗 2 で、LLVM が生成コードのすべての間接呼び出し（関数値、W25 の vtable のスロット）の前に `__guard_check_icall_fptr` の検査を入れる。明示したのに使えない target は 3 と同じ `E2000`。
6. ランタイム C の object は CFG なしのまま（リンカーが保守的に全関数を有効にする。現状の表）。

## 設計

- `BuildOptions` に `cet_compat: Option<bool>`（Phase 2 で `cf_guard: Option<CfGuard>`）。`Debug` の文がビルドキャッシュの鍵に入るので、鍵は自動で変わる。`src/main.rs` の引数解析と `rejects_ambiguous_or_unused_arguments`、`BuildOptions::validate`（`E2000`）。
- 印は `final_link_args_for` の後に、Windows・MSVC・x86-64・実行ファイル（W05 の後は DLL も）のときだけ `-Wl,/CETCOMPAT` を足す。`-g` の別リンクがあれば同じ引数を通す。同梱の launcher には渡さない（zig が拒否する）。
- Phase 2: モジュールの旗は、今 `src/llvm_debug.rs` が `-g` で書く `!llvm.module.flags` と 1 つの列に併合する（同じ番号を二重に定義しない）。旗は Windows の後処理（`windows_abi` の後）で足し、POSIX の IR を変えない。リンクに `-Wl,/guard:cf`。
- PE の検査: W06 の `tests/windows-pe.mjs` に 2 つの関数を足す。`peExtendedCharacteristics(file)`（`llvm-readobj --coff-debug-directory` の `ExtendedDLLCharacteristics` の名前の配列）と `peLoadConfig(file)`（`--coff-load-config` の `GuardFlags` と `GuardCFFunctionCount`、load config が無ければ `null`）。旗の検査は `tests/windows.mjs` の GUI の段に置く。W06 は `vscode.yml` でもこの段を同梱の `tsuzuri-clang` と `TSUZURI_READOBJ` で回すので、同梱の MinGW の実行ファイルも同じ関数で検査できる（別の PE の読み手を作らない）。

## 前提とする他チケットのインターフェース

- [W07](_completed/W07-windows-trap-fail-fast.md): Windows のトラップの fail-fast（0xC000001D）と VEH（0xC00000FD の報告）。CET の下でも同じ終了コードであることを Phase 1 で確かめる。
- [W06](_completed/W06-windows-gui-test-harness.md): `tests/windows-pe.mjs`（`readobj`・`peHeaders`）、`tests/windows.mjs` の `--stage gui`（`vscode.yml` が同梱の `tsuzuri-clang` で回す）、GUI の段（WndProc・TimerProc）。
- [W05](_completed/W05-windows-shared-library.md)（出ていれば）: Windows の `--emit shared` の最終リンク。印を同じ所で足す。
- [W25](W25-extern-interface-com.md)（Phase 2）: `extern interface` の vtable のスロットの呼び出し（間接呼び出し）。W22 の `FnPtr` と W26 の Tsuzuri の COM オブジェクトは、アドレスを取られた関数として表に入る。

## 他チケットへ提供するインターフェース

- オプション `--cet-compat`・`--cf-guard` と `BuildOptions` の 2 つの欄（W05 の DLL、W09 の GUI の実行ファイルも同じ規則に従う）。
- `platforms.md` の硬化の状態の表（W27 の配布の文書が引く）。

## 実装手順

1. **変更**: `tests/windows.mjs`。**内容**: MSVC の `-O0`・`-O3` の `hello` と `examples/tzroutines` で、`DYNAMIC_BASE`・`HIGH_ENTROPY_VA`・`NX_COMPAT` があり、`GUARD_CF` と CET 互換が無いことを検査する。**確認**: 今の HEAD で通る（ベースライン）。
2. **変更**: `tests/windows-pe.mjs`・`tests/windows.mjs`。**内容**: `peExtendedCharacteristics`・`peLoadConfig` と、GUI の段の旗の検査（手順 1 と同じ期待）。**確認**: `node tests/windows.mjs target/debug/tsuzuri.exe --stage gui` が、同梱の `tsuzuri-clang`（`TSUZURI_READOBJ` を付ける）でも通る。
3. **変更**: `src/main.rs`・`src/driver.rs`。**内容**: `--cet-compat` と既定の印。**確認**: T0・T1、`tests/windows.mjs` の検査を「x64 は CET 互換あり、`--cet-compat off` はなし、Arm64 はなし、`--cet-compat on` は Arm64 と同梱の launcher で `E2000`」に変える。
4. **変更**: なし（実行）。**内容**: CET の使える機械で、`tests/windows.mjs`・`tests/examples.mjs`・`node tests/tzroutine.mjs`・W07 のトラップと枯渇の段を CET 互換の実行ファイルで動かし、C の試験で `EnableUserShadowStack=1` を確かめる。ランナーが CET を持つかも C の試験で記録する。**確認**: すべて通る（S1）。結果をこのチケットと `platforms.md` に記録する。
5. **変更**: 文書（Phase 1）。**確認**: T2、`check-docs`、`check-cli-docs`。PR に `ci:full`。
6. **変更**（Phase 2）: `src/llvm.rs`・`src/llvm_debug.rs`・`src/driver.rs`・`src/main.rs`。**内容**: 旗 1 の既定、`/guard:cf`、`--cf-guard`。**確認**: PE に `GUARD_CF` と表、W06・W07・W25 の段が通る（S3）。`checks` で同じ段を通し、間接呼び出しの多いベンチマーク（関数値の `map` など）の `-O3` の時間を前後で測って記録する（合否は入れない）。

## テスト計画

| 段 | 期待 |
| --- | --- |
| MSVC x64・Arm64、同梱の MinGW の実行ファイルの ASLR・`HIGH_ENTROPY_VA`・DEP | すべてあり |
| MSVC x64 の既定（Phase 1） | CET 互換あり。`--cet-compat off` でなし。出力は今と同じ |
| MSVC Arm64、同梱の MinGW の既定 | CET 互換なし。`--cet-compat on` は `E2000` |
| `--emit object` と `--cet-compat on` | `E2000` |
| CET の使える機械で、CET 互換の tzroutine・トラップ（0xC000001D）・枯渇（0xC00000FD）の実行 | 今と同じ結果 |
| MSVC の既定（Phase 2） | `GUARD_CF` あり、`GuardCFFunctionCount` が 0 より大きい。W06 の WndProc、W07 の TimerProc のコールバック（`tz_gui_timer_proc`）、W25 の COM の段が通る |
| `--cf-guard checks` | IR の間接呼び出しの前に `__guard_check_icall_fptr` の読み込みがある。同じ段が通る |
| Windows の `--emit object`（Phase 2） | `.gfids$y` の節がある。`/guard:cf` の C のホスト（「現状」の試験）が完走する |
| POSIX・WASM の `--emit llvm` | 変わらない |

- 期待値は Windows のヘッダーの定数（`IMAGE_DLLCHARACTERISTICS_*`、`IMAGE_DLLCHARACTERISTICS_EX_CET_COMPAT` = 1、`IMAGE_DEBUG_TYPE_EX_DLLCHARACTERISTICS` = 20）から書く。

## ドキュメント

- `compiler/platforms.md`: 新しい節「実行ファイルのセキュリティ機能」に、ツールチェーンと CPU ごとの ASLR・DEP・CET・CFG の表、同梱の MinGW で付けられない理由（zig が旗を拒否、load config が無い）、確かめた範囲（CET の使える機械の名前）。
- `compiler/option.md` と `README.md` の CLI の表: `--cet-compat`（Phase 2 で `--cf-guard`）。
- `compiler/native-interop.md`: Windows の object は CFG の表を持つこと（Phase 2）。`--link` する C が独自のスタック切り替え（アセンブリのコルーチンなど）をするなら `--cet-compat off` が要ること。
- `docs/architecture.md` の Windows MSVC の節。

## 受け入れ条件

- [ ] Phase 1: 仕様 1〜3 のとおりで、テスト計画の Phase 1 の段が通る。CET の使える機械での実行を記録した。IR は変わらない。
- [ ] Phase 2: 仕様 4〜6 のとおりで、Phase 2 の段が通る。`checks` の費用を測って記録した。
- [ ] POSIX と WASM の出力が変わらない。文書を直し、`check-docs`・`check-cli-docs` が通る。PR に `ci:full` を付けた。

## 落とし穴

- Git Bash は `-Wl,/CETCOMPAT` の `/CETCOMPAT` をパスに書き換える。手で試すときは `MSYS_NO_PATHCONV=1` か PowerShell を使う。`editbin /CETCOMPAT` は MSVC 14.44 で `LNK4044` を出して何もしない（確認に使わない）。
- `GuardFlags` 0x100 だけでは CFG は働かない。`DllCharacteristics` の `GUARD_CF`（0x4000）を見る。
- CET の印は、CPU と OS が対応しなければ何もしない。ランナーで通っても実行の互換性の証拠にならない（手順 4）。
- CFG を有効にした実行ファイルでは、表に無いアドレスへの OS からの間接呼び出しは 0xC0000409 で終わる。アドレスを取る関数（コールバック、VEH、Fiber の開始関数）は IR か C で取られていることを確かめる。
- `!llvm.module.flags` を 2 つ定義すると IR が壊れる。`-g` と併合する。

## 対象外

- 同梱の MinGW の CET・CFG（zig が対応したら別のチケット）。`/guard:ehcont`・CET の厳格モード・`SetProcessMitigationPolicy` での強制。
- コード署名（`signtool`）と SmartScreen。調査では、署名のない小さな GUI の実行ファイルの 1 つを Defender が止めた（1 件だけの観察）。
- POSIX の硬化（PIE・RELRO・CET の ELF の印）。ランタイム C を CFG 付きでコンパイルし直すこと。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | MSVC でリンクする x86-64 の実行ファイル（と W05 の DLL）に CET 互換を既定で付ける | 生成コード・Fiber の tzroutine・CRT は互換で、CET の使える機械で Fiber と tzroutine の実行を確かめた。シャドウスタックは戻りアドレスの書き換えを、同じプロセスの C のコードも含めて止める。費用は印だけ（シャドウスタックの書き込みは CPU が行う） | 要承認（承認前は該当する段に着手しない） |
| D2 | オプション `--cet-compat <on\|off>` と `--cf-guard <off\|tables\|checks>` を足す（D-55 の登録）。明示したのに使えない target は `E2000`、指定なしは使える所でだけ有効 | `--link` する C が独自のスタック切り替えをするとき、利用者が外せる必要がある。明示の要求を黙って捨てない（AGENTS）。manifest のキーは足さない（ビルドの性質で、W09 の `[windows]` の表に足すかは後で決める） | 要承認（承認前は該当する段に着手しない） |
| D3 | Phase 2 で、MSVC の実行ファイルの CFG の表（旗 1 と `/guard:cf`）を既定にし、Windows の object も表を持つ | 実行ファイルが CFG を有効にすると、OS と CRT の間接呼び出しは表の関数にしか飛べない。表だけなら生成コードの費用はない。ホストへの埋め込みは表がなくても動く（リンカーが保守的に扱う。現状の表）ので、object の表の利点は有効な呼び先を絞ることにある | 要承認（承認前は該当する段に着手しない） |
| D4 | CFG の検査（旗 2）は opt-in | 間接呼び出しごとに検査の呼び出しが入る。Tsuzuri の関数値はメモリ安全な値から作られ、書き換えの経路が無いので、利益は W25 の外部の vtable に偏る | 既定案（実装者はこの案に従う） |
| D5 | ASLR・`HIGH_ENTROPY_VA`・DEP の旗は渡さず、テストで固定する | 両方のツールチェーンの既定で有効。旗を渡すとリンクの命令とキャッシュが変わるだけで、既定が外れればテストが落ちる | 既定案（実装者はこの案に従う） |
| D6 | 同梱の MinGW は CET・CFG を付けない（文書に書く） | zig 0.16 が旗を拒否し、CRT に load config が無い。ツールチェーンの外で PE を書き換えるのは検証の範囲が広すぎる | 既定案（実装者はこの案に従う） |
