# W28: UI を固定した tzroutine（ファイバー）の上で動かすときのスタックと検証

| 項目 | 内容 |
| --- | --- |
| ID | W28 |
| 優先度 | P2 |
| 規模 | M |
| 依存 | W06 |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-22（D-63。利用者の依頼「X: D13 を狭く改める方針で決定。実現向けたチケットを @_features に W プリフィックスのチケットを作成して」と「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」から起票。HEAD `fa40eede` で確認） |
| 承認 | 不要（調査と修正。仕様を変えるなら要承認）。予約の既定値や新しいつまみを変える場合は決定事項 D5（要承認） |
| 改善する劣位 | リスク「ファイバーのスタック」: `Routine.with_locked_thread` の中で UI を動かすと、user32・コモンコントロール・シェル拡張・COM のコードが、その tzroutine の Fiber のスタック（既定の予約 1 MiB）の上で走る。どれだけ使うかを誰も測っておらず、溢れると Windows では理由を出さずに終わる。文書（`routine.md`・`native-interop.md`）は UI のスレッドに `with_locked_thread` を勧めている |
| 手本にする既存実装 | `src/runtime/tzroutine-windows.h` の `tzr_fiber_create`、`src/runtime/tzroutine.c` の `tzr_init`（`TSUZURI_TZROUTINE_STACK_KIB`）・`tzr_galloc`・`tsuzuri_tz_lock_thread`・`tzr_park_locked`、`tests/tzroutine_runtime.c` の `stack_size`・`locked_thread`、W06 の GUI の段（`tests/windows.mjs`） |
| 主な影響ファイル | `tests/windows.mjs`、`tests/windows_ui_fiber.c`（新規。W06 の C の補助の隣）、`tests/fixtures/windows_gui/ui_fiber/Main.tz`（新規。W06 の fixture の置き場）、条件付きで `src/runtime/tzroutine.c`・`src/runtime/tzroutine-windows.h`、`_tsuzuri/language-reference/built-in-types-and-modules/routine.md`・`async-tasks-and-lazy/tzroutine.md`・`compiler/native-interop.md`、`docs/language.md`、`docs/architecture.md` |

## 実装者向け早見

- 範囲: (1) 実際の UI の処理が固定した tzroutine の Fiber のスタックをどれだけ使うかを、`main` のスレッドと比べて測る検証を W06 の基盤に足す。(2) 文書の既定を「UI は `main` のスレッドで動かす」にし、固定した tzroutine で動かすときの条件を書く。(3) 測った値が予約の半分を超えたときだけ、変更の案を報告する（D5 は要承認）。(4) W07 があれば、固定した G の溢れを報告できる余地を `SetThreadStackGuarantee` で確保する。
- 最初に読む: 現状の 1〜6、`tzroutine-windows.h` の `tzr_fiber_create`（151 行付近）、`tzroutine.c` の `tzr_galloc`・`tsuzuri_tz_lock_thread`・`tzr_park_locked`。
- 最初に書くテスト: W06 の `tz_gui_pump` を回す場面 p と、`tests/windows_ui_fiber.c` の場面 a（メッセージ専用の窓と、深さ 64 の `SendMessageW` の入れ子）を、`main` と `with_locked_thread` の 2 つの形で、コミット済みのスタックの最大を出して終わる E2E。
- 触ってはいけない: tzroutine のスタックの既定（1 MiB の予約、`TSUZURI_TZROUTINE_STACK_KIB` の範囲 64〜1048576）、`with_locked_thread` の IR（`tsuzuri_tz_lock_thread`・関数の呼び出し・`unlock_thread`）、POSIX の挙動。
- 完了の見分け方: `node tests/windows.mjs target/release/tsuzuri.exe` の GUI の段が、7 つの場面（p・a〜f）を `main` と固定した tzroutine の両方で成功させ、場面ごとのコミット済みのスタックの最大を出力する。その値が、このチケットの「実装と検証」と `docs/architecture.md` に記録されている。

## 目的

Win32 のアプリを Tsuzuri だけで書くとき（[GUIDE D-63](GUIDE.md#d-63-tsuzuri-だけで-win32-のアプリを書くe12-d13-の狭い改訂w01w30)）、UI のスレッドは `main` のトップレベルか、`Routine.with_locked_thread` で固定した tzroutine のどちらかになる。後者では、Tsuzuri の WndProc から呼ばれる `DefWindowProcW`、コモンコントロール、ファイルを開くダイアログが読み込むシェル拡張、COM の部品が、すべて tzroutine の Fiber のスタックの上で走る。Fiber のスタックは tzroutine を作ったときに予約が決まり、あとから広げられない。溢れても今の Windows は理由を出さない。このチケットは、どれだけ使うかを実際の部品で測り、既定の勧めを文書に書き、測った値に基づいて要るときだけ手を打つ。

## 着手条件と停止条件

### 着手条件

- [W06](W06-windows-gui-test-harness.md) が done（GUI を描かずに検査する段、C の補助の置き場、窓を自分で閉じる書き方、`timeout` の包み）。`node scripts/repo-kb.mjs brief W28`。
- Windows の開発機（MSVC の Developer PowerShell と LLVM の clang、および同梱の MinGW の `tsuzuri-clang`）。

### 停止条件

- **S1** ある場面が `main` では成功し、固定した tzroutine では失敗する（例外・終了コード・ハング・失敗の `HRESULT`）。スタック以外の Fiber の相性の問題（Fiber のローカルな記憶域、CLR を使う拡張、`ConvertThreadToFiber` を自分で呼ぶ部品）かもしれないので、ログと場面を添えて報告する。
- **S2** どれかの場面の固定した tzroutine のコミット済みのスタックの最大が、予約（既定 1 MiB）の半分を超える。D5 の案を添えて報告する（承認前に予約や既定を変えない）。
- **S3** GUI の段がハングするか窓を残す。基盤の直し方が 2 回続けて失敗したら止める。
- **S4** 検証のために生成 IR、std、組み込みを変える必要が出た。
- **S5** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

1. **Fiber の予約**: `src/runtime/tzroutine-windows.h` の `tzr_fiber_create` は `CreateFiberEx(commit, reserve, FIBER_FLAG_FLOAT_SWITCH, …)` で、`commit` は `reserve / 4` と 64 KiB の小さい方。`reserve` は `tzr_init` の `tzr_sched.stack_size`（`TSUZURI_TZROUTINE_STACK_KIB`、64〜1048576 KiB、既定 `TZR_DEFAULT_STACK` = 1024 KiB をページに切り上げ）。
2. **予約は作ったときに決まる**: `tzroutine.c` の `tzr_galloc` は G を作るときに 1 回だけ Fiber を作り、G は `p->gfree`・`tzr_sched.gfree` で使い回す。したがって、`with_locked_thread` に入った時点で、その G のスタックを広げることはできない（起票時の案「固定した G の予約を大きくする」は、既存の G にはそのままでは実現できない）。
3. **固定した G はずっと自分の Fiber の上にいる**: `with_locked_thread` は `src/llvm.rs` の `routine_builtin` で `tsuzuri_tz_lock_thread` → 関数の呼び出し → `tsuzuri_tz_unlock_thread` に下がる。固定した G の待ち（`tzr_park_locked`）と譲渡（`tzr_yield_locked`）は g0 へ切り替えず、その OS スレッドを眠らせる。コールバックの入口（`tsuzuri_tz_callback_enter`）も G を M に固定する。UI の処理はすべて G の Fiber の上で走る。M のスレッド自体は 256 KiB（`tzr_thread_start(tzr_mstart, m, 256 * 1024, …)`）で、g0 だけが使う。
4. **`main` のスレッドのスタック**: `main` のトップレベルは擬似 G で、OS のスレッドの自前のスタックで走る（Fiber ではない。`tsuzuri_tz_callback_enter` は 0 を返して P を扱わない）。予約は実行ファイルの PE ヘッダーで決まり、driver は `/STACK` を渡さない。`Time.sleep` だけのプログラムを `llvm-readobj --file-headers` で見ると、`SizeOfStackReserve` は MSVC の link で 1048576（1 MiB）、同梱の MinGW（Zig）で 16777216（16 MiB）だった。**MSVC では `main` のスレッドも Fiber と同じ 1 MiB** で、「`main` で動かせばスタックに余裕がある」とは言えない。
5. **Fiber で測れること**（起票時に Windows 11 Enterprise 10.0.26300、同梱の MinGW でビルドした小さな C で確認し、削除した）: Fiber の中の `GetCurrentThreadStackLimits` は Fiber の範囲（1024 KiB）を返し、局所変数のアドレスはその中にあった。`NT_TIB` の `StackBase − StackLimit`（コミット済みの大きさ）は最初 64 KiB で、256 KiB の再帰の後は 268 KiB に増えた（減らないので、最大の目安になる）。`SetThreadStackGuarantee` は Fiber ごとで、Fiber で 64 KiB にした後、元のスレッドの Fiber では 0 のまま、Fiber に戻ると 64 KiB だった。MSVC のビルドと Arm64 では未確認。
6. **溢れたとき**: `async-tasks-and-lazy/tzroutine.md` は「Windows では理由なしで終わります」と書く（`src/runtime/stack.c` の報告は POSIX だけ）。Windows の報告は [W07](W07-windows-trap-fail-fast.md) の VEH が扱う予定。
7. **文書と検証**: `routine.md`（`Routine.with_locked_thread` の節）と `native-interop.md`（tzroutine の節）は、GUI の UI スレッドに `with_locked_thread` を勧めるが、Fiber のスタックの上で動くことは書いていない。UI の処理を Fiber の上で動かすテストはない（`tests/windows.mjs` に GUI の段はまだない。W06 が足す）。

## 仕様

- 言語・std・CLI・環境変数は変えない（D5 の承認を得た場合を除く）。
- 文書の勧め（D1）: UI のスレッドは `main` のトップレベルを既定にする。理由はスタックの大きさではなく、(a) Fiber でない普通のスレッドで、Fiber と相性の悪い部品の心配がない、(b) コールバックが P の取り戻しをしない（W29 の遅延がない）、(c) `main` の終わりまで同じスレッドである、の 3 つ。固定した tzroutine で UI を動かしてよいが、そのときは Fiber のスタック（既定 1 MiB の予約）の上で動くこと、`TSUZURI_TZROUTINE_STACK_KIB` で全 tzroutine の予約を増やせること、COM の初期化（`CoInitializeEx`）と終了は `with_locked_thread` の関数の中で対にすること（固定が外れた後、その OS スレッドは別の tzroutine を走らせる）を書く。MSVC と MinGW で `main` のスレッドの予約が違うことも書く。
- 検証の結果（場面ごとの最大）は、このチケットと `docs/architecture.md`（TZ01 の Windows の節）に記録する。CI は場面が成功することだけを確かめ、値の上限は比べない（D3）。

## 設計

### 1. 場面（`tests/windows_ui_fiber.c`）

C の補助は 1 つの関数 `int64_t tz_ui_scenario(int32_t which, int64_t (*wnd_proc)(int64_t, uint32_t, uint64_t, int64_t))` を公開する。補助は 1 回の呼び出しの中で、窓の作成・メッセージの処理・後片付けまでを終え（コールバックは呼ばれている `extern` の中だけで呼ばれる。今の契約の範囲で、W12 を待たない）、終わった時点のコミット済みのスタックの大きさ（`NT_TIB` の `StackBase − StackLimit`）を返す。失敗は負の値（`HRESULT` か `-GetLastError()`）。窓は W06 の書き方（`SetTimer` で自分で閉じる）に従う。

| which | 場面 | 何が Fiber のスタックで走るか |
| --- | --- | --- |
| a | メッセージ専用の窓（`HWND_MESSAGE`）を Tsuzuri の WndProc で作り、WndProc の中から自分へ `SendMessageW` を深さ 64 まで入れ子にする | user32 のカーネルからの呼び戻しと Tsuzuri の frame の入れ子 1 段の費用 |
| b | マニフェストの活性化コンテキスト（`CreateActCtxW`・`ActivateActCtx`、Common-Controls 6.0）の下で `InitCommonControlsEx`、1 万行の ListView（report view）・TreeView・ツールチップを作って壊す | comctl32 v6 |
| c | `TaskDialogIndirect`（`TDF_CALLBACK_TIMER`）を `TDN_TIMER` で `TDM_CLICK_BUTTON` して閉じる | comctl32 v6 のモーダルループ |
| d | `MessageBoxW` を、所有者の窓の `SetTimer` の手続きで見つけて `WM_COMMAND`（`IDOK`）で閉じる | user32 のモーダルループ |
| e | `CoInitializeEx(COINIT_APARTMENTTHREADED)` の後、`IFileOpenDialog` に `IFileDialogEvents` を `Advise` し、`OnFolderChange` で `IFileDialog::Close(S_FALSE)` を呼んで閉じる（最初のフォルダーへの移動で呼ばれる想定。来なければ所有者の窓の `SetTimer` で閉じる。どちらでも最初のフォルダーの列挙までは走る） | シェル、名前空間・アイコン・オーバーレイの拡張、COM |
| f | `SHCreateDefaultContextMenu` でファイルのコンテキストメニューを作り `QueryContextMenu` を呼ぶ | コンテキストメニューのシェル拡張 |

- 基準の場面 p: W06 の shim の `tz_gui_pump`（`tests/windows_gui_shim.c`。タイマー 2 つと別スレッドの `PostMessageW`）を、そのまま固定した tzroutine の中で回す（W06 の「W28: 固定した tzroutine の中で `tz_gui_pump` を回す」）。p だけは shim の関数なので、値はスタックの大きさでなく shim の結果で合否を見て、スタックの大きさは補助の `tz_ui_stack_committed()`（`NT_TIB` の差を返すだけの関数）を続けて呼んで取る。
- 補助は `COBJMACROS` の C で書き、`IFileDialogEvents` の vtable も手で作る（W25 の `extern interface` を待たない）。W06 は shim に関数を足す形を勧めるが、b〜f は comctl32・ole32・shell32 を要るので、shim に足すとすべての GUI の fixture がそれらをリンクすることになる。別のファイル `tests/windows_ui_fiber.c` にし、W06 の `buildGui("ui_fiber", ["--link", "tests/windows_ui_fiber.c", "-l", "comctl32", "-l", "ole32", "-l", "shell32", "-l", "uuid"])` で足す（`buildGui` が shim と `-l user32` を付ける）。
- Tsuzuri の側（`tests/fixtures/windows_gui/ui_fiber/Main.tz`。W06 の fixture の置き場）は `def main :: Array<string> -> i32`。引数は `main` か `locked` と場面の名前。`locked` では `tzroutine { … Routine.with_locked_thread (\() -> ui_scenario which on_message) … }` の結果をチャンネルで受ける。WndProc は `def on_message :: i64 -> i32u -> i64u -> i64 -> i64` で、知らないメッセージは `extern "DefWindowProcW"` に渡す（窓のハンドルは W06 と同じく `i64`。今の ABI で書ける）。

### 2. 実行と記録（`tests/windows.mjs` の GUI の段）

- 場面ごとに新しいプロセスで実行する（コミット済みの大きさは減らないので、場面を混ぜない）。`main` と `locked` の 2 形、`-O0`・`-O3`。`timeout` は W06 の値。
- ツールチェーン: `windows.yml` の gate は LLVM の clang と MSVC だけで、同梱の MinGW（`tsuzuri-clang`）は `vscode.yml` の Windows の job にしかない。場面を W06 の GUI の段に足せば、MSVC は `windows.yml` の W5（`node tests/windows.mjs target/release/tsuzuri.exe`。x64 と Arm64）で、MinGW は W06 が `vscode.yml` に足す「Windows GUI stage with the bundled toolchain」（`node ../tests/windows.mjs toolchain/bin/tsuzuri.exe --stage gui`）で走る。workflow は変えない。W06 の段が `vscode.yml` に無い場合は、MinGW の場面を手元だけで走らせ、その結果を「実装と検証」に記録する。
- CI のホストだけで失敗する場面（対話のデスクトップがない runner でダイアログが出ないなど）は、Fiber の問題（S1）と決める前に、W06 の基盤の前提（デスクトップ、ウィンドウステーション、`timeout`）を確かめる。
- 合否: 終了コード 0、出力した値が正（成功）であること。値は `場面 形 最適化 ツールチェーン: N KiB` の形で出力するだけで、上限と比べない（D3）。
- 手元の記録: 開発機と、`ci:full` の `windows-2022`・`windows-11-arm` の出力を、場面ごとの最大・最小としてこのチケットの「実装と検証」に書く（Windows の版、ツールチェーン、日付）。

### 3. 判断の規則（D4）

- 固定した tzroutine の最大が予約の半分以下なら、予約は変えず、文書（仕様）と記録だけで終える。
- 半分を超えたら S2。報告する案（どれも要承認。D5）: (i) Windows の既定の予約を上げる（予約はアドレス空間だけで、コミットは今と同じ 64 KiB から増える。文書の「既定 1 MiB」が変わる）、(ii) 固定する tzroutine だけの予約のつまみ（D-55 の登録）、(iii) `with_locked_thread` の関数を、より大きな別の Fiber で走らせる（固定した G は待つ間も Fiber を切り替えないので成り立ちそうだが、未確認。`with_locked_thread` の IR とランタイムの入口が変わる）、(iv) 予約は変えず、`TSUZURI_TZROUTINE_STACK_KIB` を文書で案内するだけ。

### 4. 溢れの報告の余地（W07 があるときだけ）

- W07 が done で、W07 の VEH が固定した G の `STATUS_STACK_OVERFLOW` を報告できない（報告の途中で 2 度目の溢れになる）と分かったときだけ、`tsuzuri_tz_lock_thread` の最初の固定（`g->locks` が 0 → 1）で、Windows なら `SetThreadStackGuarantee` を 64 KiB 以上にする（今の値が小さいときだけ）。保証は Fiber ごとで、G を使い回しても残る（現状 5）。費用は固定したことのある G のコミットが増えるだけ。W07 がなければこの段は飛ばす。

## 前提とする他チケットのインターフェース

- [W06](W06-windows-gui-test-harness.md) の仕様 4: `tests/windows.mjs` の GUI の段（`--stage gui` で単独でも回る）、`buildGui(fixture, flags = [])`（`tests/fixtures/windows_gui/<fixture>` を shim と `-l user32` 付きでビルドして exe のパスを返す）、`tests/windows_gui_shim.c` の `tz_gui_pump(int64_t (*on_message)(uint32_t, uint64_t, int64_t), int64_t expected)`（5 秒の見張りのタイマーで必ず終わる）、`vscode.yml` の「Windows GUI stage with the bundled toolchain」。W06 の実装で名前が変わっていたら、W06 に合わせる。
- [W07](W07-windows-trap-fail-fast.md)（設計 4 だけ）: Windows の `STATUS_STACK_OVERFLOW` を報告する first-chance の VEH。

## 他チケットへ提供するインターフェース

- `tests/windows_ui_fiber.c` の場面（W25・W26 の COM の検証と、W29 の `with_locked_thread` の UI の遅延の計測が流用できる）。
- 記録した場面ごとのスタックの最大（W30 の CET のシャドウスタックの検討、W26 の UI Automation の見積もり）。

## 実装手順

### 手順 1: 基準（場面 p）と場面 a

- 変更: `tests/windows_ui_fiber.c`（`tz_ui_stack_committed` と場面 a）、`tests/fixtures/windows_gui/ui_fiber/Main.tz`、`tests/windows.mjs` の GUI の段への追加。
- 内容: 設計 1・2。`main` と `locked` の両方で。
- 確認: Windows で `node tests/windows.mjs target/debug/tsuzuri.exe --stage gui` を、MSVC（Developer PowerShell と `TSUZURI_CLANG` に LLVM の clang）と同梱の MinGW（`TSUZURI_CLANG=$T/bin/tsuzuri-clang.exe`）で。値が出て、2 回の実行で同じ桁になる。

### 手順 2: 場面 b〜f

- 変更: `tests/windows_ui_fiber.c`、マニフェストの書き出し（テストが一時ディレクトリに書く）。
- 内容: 設計 1 の表。場面 e・f は CI のイメージに入っている拡張しか読み込まないので、開発機の値も記録する。
- 確認: 7 場面（p・a〜f）× 2 形 × `-O0`・`-O3` × 2 ツールチェーンが手元で成功する。S1 に当たったら止める。

### 手順 3: 測定と判断

- 変更: なし（記録だけ）。
- 内容: 開発機と `ci:full` の 2 つのホストの値を集め、設計 3 の規則で判断する。
- 確認: 記録を「実装と検証」に書いた。S2 なら報告して止める。

### 手順 4: 溢れの報告の余地（W07 が done のときだけ）

- 変更: `src/runtime/tzroutine.c` の `tsuzuri_tz_lock_thread`（Windows の分岐）、`tests/tzroutine_runtime.c` の新しい場面 `locked_guarantee`（固定した G で `SetThreadStackGuarantee(&0)` が 64 KiB 以上を返し、固定しない G は変わらない）。
- 内容: 設計 4。W07 の VEH の報告が固定した G の溢れで出ることを、深い再帰の場面で確かめてから入れる。
- 確認: `node tests/tzroutine_runtime.mjs`（Windows、MSVC の clang）。

### 手順 5: 文書と T2

- 変更: 「ドキュメント」のページ。
- 内容: 仕様の勧めと条件、記録した値の要約（値は「計測した日・環境・場面」とともに。性能の主張ではない）。
- 確認: `node scripts/check-docs.mjs <変更したページ>`、T2、PR に `ci:full`。

## テスト計画

- E2E: `tests/windows.mjs` の GUI の段（設計 2）。7 場面（p・a〜f）× `main`・`locked` × `-O0`・`-O3`。MSVC は `windows.yml`、MinGW は W06 が `vscode.yml` に足す GUI の段（無ければ手元。設計 2）。合否は成功だけ。
- C: 手順 4 を行ったときだけ `tests/tzroutine_runtime.c` の `locked_guarantee`。
- 既存のテスト: 変わらない（`tests/tzroutine_runtime.mjs` の `stack_size`・`locked_thread`、`tests/tzroutine.mjs`）。

## ドキュメント

- `built-in-types-and-modules/routine.md` の `Routine.with_locked_thread` の節: UI は `main` を既定にする理由、固定した tzroutine の UI は Fiber のスタックで走ること、`TSUZURI_TZROUTINE_STACK_KIB`、COM の初期化を関数の中で対にすること。
- `async-tasks-and-lazy/tzroutine.md`（スタックの段落）、`compiler/native-interop.md`（tzroutine と UI の段落）、`compiler/platforms.md`（MSVC と MinGW の `main` のスレッドの予約の違い）。
- `docs/language.md` の tzroutine の節、`docs/architecture.md` の TZ01 の Windows の節（記録した値）。

## 受け入れ条件

- [ ] 7 つの場面（p・a〜f）が `main` と固定した tzroutine の両方で、`-O0`・`-O3` で成功する。MSVC は `windows.yml` の GUI の段で、MinGW は W06 が `vscode.yml` に足す GUI の段か、無ければ手元で確かめて記録した（設計 2）。
- [ ] 場面ごとのコミット済みのスタックの最大を、開発機と `ci:full` の 2 ホストで記録した。
- [ ] 設計 3 の規則で判断し、半分を超えたなら報告して止めた（予約は承認なしに変えていない）。
- [ ] 文書の勧め（`main` が既定、固定した tzroutine の条件、ツールチェーンごとの `main` の予約）を書き、`check-docs` が通る。
- [ ] 生成 IR・std・CLI・環境変数を変えていない（D5 の承認を得た場合を除く）。T2 が通る。PR に `ci:full` を付けた。

## 落とし穴

- コミット済みの大きさは減らない。場面ごとに新しいプロセスで測る。Fiber の最初のコミット（64 KiB）より小さい使い方は 64 KiB と出る。
- `GetCurrentThreadStackLimits` と `NT_TIB` は呼んだ時点の Fiber の値を返す。補助は Tsuzuri の G の上（`extern` の中）で読む。別のスレッドから読まない。
- `IFileDialog::Close` は `IFileDialogEvents` の中から呼ぶ。`OnFolderChange` が来る前に閉じると、シェル拡張の読み込みを測れない。STA の初期化（`CoInitializeEx`）を忘れると `Show` が失敗する。
- コモンコントロール v6 はマニフェストの活性化コンテキストが要る。ないと v5 の comctl32 が読み込まれ、`TaskDialogIndirect` が見つからない。
- CI のイメージのシェル拡張は開発機より少ない。CI の値が小さくても、開発機の値で判断する。
- `with_locked_thread` の固定が外れた後も、その OS スレッドの COM の apartment は残る。補助は `CoUninitialize` まで終えてから戻る。
- GUI の実験は必ず自分で窓を閉じる（`SetTimer` → `DestroyWindow`）。手元では `timeout 60` で包む。

## 対象外

- tzroutine のスタックの既定の予約の変更と、新しいつまみ（D5 の承認があれば別の PR）。
- 実行ファイルの `main` のスレッドの予約（`/STACK`）の変更とツールチェーンごとの差の解消。
- `extern interface`（W25）での COM の呼び出し、UI Automation（W26）。スレッドに結び付くハンドルの検査（W19）。
- 溢れの報告そのもの（W07）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | 文書の既定は「UI は `main` のスレッドで動かす」。固定した tzroutine の UI は条件付きで使える | `main` は Fiber でなく、コールバックが P を扱わず、終わりまで同じスレッド。スタックの大きさは MSVC では同じなので理由にしない（現状 4） | 既定案（実装者はこの案に従う） |
| D2 | スタックの使用量は C の補助が `NT_TIB` のコミット済みの大きさで測る。ランタイムに計装を足さない | 実際の部品をそのまま走らせて測れ、IR とランタイムを変えない。Fiber で値が正しいことは起票時に確かめた（現状 5） | 既定案（実装者はこの案に従う） |
| D3 | CI は場面の成功だけを確かめ、スタックの値の上限は比べない | 値は Windows の版・更新・入っているシェル拡張で変わり、共有の CI で不安定になる。値は手元と CI の記録として残す | 既定案（実装者はこの案に従う） |
| D4 | 判断の線は「固定した tzroutine の最大が予約の半分」 | 測っていない拡張（利用者の機械のシェル拡張、IME、支援技術のフック）の分の余裕を残す | 既定案（実装者はこの案に従う） |
| D5 | 予約の既定・固定する tzroutine だけの予約のつまみ・別の Fiber で走らせる形のどれかに変えるなら、人間の承認を得る | 文書の既定（1 MiB）、D-55 の登録、`with_locked_thread` の IR とランタイムの入口の変更になる | 要承認（承認前は該当する段に着手しない） |
| D6 | `SetThreadStackGuarantee` は W07 があり、それが要ると確かめたときだけ、固定した G に設定する | 保証はコミットを増やす。すべての G に付けると数万の tzroutine の費用になる。固定する G は少ない | 既定案（実装者はこの案に従う） |
