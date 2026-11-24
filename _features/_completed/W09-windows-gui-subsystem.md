# W09: GUI サブシステムの実行ファイル（`--subsystem windows` と `[windows] subsystem`）

| 項目 | 内容 |
| --- | --- |
| ID | W09 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | [W08](W08-io-without-std-streams.md) |
| 後続 | [W10](W10-windows-resources.md) |
| 状態 | done |
| 起票 | 2026-11-22（D-63。利用者の依頼「X: D13 を狭く改める方針で決定。実現向けたチケットを @_features に W プリフィックスのチケットを作成して」と「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」から起票。HEAD `fa40eede` で確認） |
| 承認 | 要承認（D-55 への登録: CLI の `--subsystem console\|windows`、マニフェストの節 `[windows]` とキー `subsystem`。決定事項 D1） |
| 手本にする既存実装 | `src/driver.rs` の `BuildOptions::with_manifest_wasm`（マニフェストの設定を、それが効く出力にだけ移す）、`src/main.rs` の `links_apply`（マニフェストの `[native]` が効くコマンド）と `--strip` の解析とコマンドごとの検査、`src/driver.rs` の `pdb_link_args`（`-Xlinker` で MSVC のリンカーに渡す）と `msvc_linker_for`、`src/package.rs` の `parse_manifest` の `[wasm]` 節 |
| 主な影響ファイル | `src/driver.rs`・`src/main.rs`・`src/package.rs`・`src/diagnostic_kinds.rs`・`tests/packages.rs`・`tests/windows.mjs`（GUI の段）・`README.md`・`_tsuzuri/language-reference/compiler/option.md`・`_tsuzuri/language-reference/organizing-tsuzuri/packages.md`・`_tsuzuri/language-reference/compiler/platforms.md`・`_tsuzuri/language-reference/compiler/native-interop.md`・`_tsuzuri/language-reference/compiler/diagnostics.md`（生成）・`docs/language.md` |

## 実装者向け早見

- 範囲: Windows の実行ファイルを、コンソールの窓を作らない GUI サブシステム（PE の `Subsystem` が 2）でリンクする。指定の方法は 2 つある。
  - コマンドライン: `tsuzuri build --subsystem windows`（`run` も可）。
  - ルートの `Tsuzuri.toml` の `[windows] subsystem = "windows"`。
- 変えないもの: 入口は今の C の `main`（`console_def_main`・`console_main`）のままで、`wWinMain` は作らない（D3）。
- 最初に読む: 仕様 1〜3、`src/driver.rs` の `build_complete` のリンクの段（`sdk_args`・`final_link_args` を足す所）、`with_manifest_wasm`、`msvc_linker_for`、`src/main.rs` の `--strip` と `links_apply`、`src/package.rs` の `parse_manifest`。
- 最初に書くテスト: `src/driver.rs` のテスト `subsystem_link_args_follow_the_linker`（仕様 3 の表）。
- 触ってはいけない:
  - `--subsystem` を付けず、`[windows]` もないビルドのリンクの引数・IR・object・診断・exe。
  - `test`・`bench` の runner（常にコンソール）。
- 完了の見分け方: 次の 2 つが成り立つ。
  - `tsuzuri build app --subsystem windows -o app.exe` の exe を、W06 の `peHeaders` で読むと `subsystem` が 2。MSVC と同梱の両方で成り立つ。
  - W06 の起動器でハンドルなしで起動すると、`def main` の返した値で終わる。

## 目的

Tsuzuri だけで Win32 のアプリを書くには（[GUIDE D-63](GUIDE.md#d-63-tsuzuri-だけで-win32-のアプリを書くe12-d13-の狭い改訂w01w30)）、Explorer から起動したときにコンソールの窓が開かない実行ファイルが要る。今は、リンカーへの指示を C の補助に埋め込む回避策しかない（「現状」）。そのうえ、その回避策は `[native] link` を使うのでビルドキャッシュも止まる。サブシステムを、ビルドの設定として正面から指定できるようにする。標準ハンドルが無いときの IO の意味は W08 が決める。このチケットは、それを前提に文書を書く。

## 着手条件と停止条件

### 着手条件

- D1 の承認（D-55 への登録）。
- W08 が done であること。W08 の決定（D1・D2）が出ていれば、このチケットの文書はその意味で書く。
- W06 が入っていれば、その `peHeaders`・`launchWithoutHandles`・`buildGui` を使う。入っていなければ、W06 の仕様 4 と同じ名前・同じ形で、必要な部分だけを足す。
- `gh run list --branch main --workflow ci.yml --limit 1` が緑。

### 停止条件

- **S1** ARM64 の Windows（同梱の llvm-mingw の `ld.lld`、MSVC の arm64）で、`-Wl,--subsystem,windows` か `/subsystem:windows /entry:mainCRTStartup` のリンクが失敗する。または、できた exe が `main` を呼ばない。x64 でしか確かめていない。
- **S2** `--subsystem` も `[windows]` もないビルドのリンクの引数・IR・診断が変わる。
- **S3** `tsuzuri test` が、`[windows] subsystem = "windows"` のプロジェクトで失敗する（runner に GUI サブシステムが付いた）。
- **S4** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

- **入口**: ネイティブの実行ファイルは、常にコンパイラの作る C の `main` を入口に持つ。
  - `def main` のときは `src/llvm.rs` の `console_def_main`（`define i32 @main(...)` が `@tz.fn.Main.main` を呼ぶ）。
  - スクリプトの形と `IO` の入口では `console_main`。
  - `WinMain`・`wWinMain` の経路はない。
  - コマンド行は、`def main :: Array<string> -> i32` のとき `src/runtime/arguments.c` が `GetCommandLineW()` から分ける（`tz_arguments_split`）。
- **リンク**: Windows の実行ファイルは、1 回の Clang の実行でリンクする。`src/driver.rs` の `build_complete` は、`options.emit == Emit::Executable && dwarf_sidecar.is_none()` のとき、`sdk_args(platform)?` と `final_link_args(platform)?` を足す。その後にランタイムの object とリンク入力（`links.add_to`）と PDB の引数（`pdb_link_args`）を足す。`--backend fast` の object も同じ Clang の実行でリンクする。サブシステム・入口・リンカーへの任意の指示を足す設定はない。
- **リンカーの種類**: `src/driver.rs` の `msvc_linker_for(platform)` は、MSVC の target を MSVC のリンカー（`link.exe` か `lld-link`）でリンクするときに真。同梱の `tsuzuri-clang`（`bundled_launcher`）は MinGW でリンクするので偽になる。
- **ビルドキャッシュ**: `src/cache.rs` の `build_key` は `BuildOptions` の `Debug` の文字列（`options`）と、プロジェクトのすべてのマニフェストの本文を鍵に入れる。`src/driver.rs` の `build_complete` は、`links.is_empty()` でないビルドのキャッシュを使わない（E12 D7）。
- **マニフェスト**: `src/package.rs` の `parse_manifest` は、`[package]` に続く `[dependencies]`・`[wasm]`・`[native]`・`[registry]`・`[capabilities]` を各 1 回だけ受ける。順序の誤りの文は `expected [package] followed by optional [dependencies], [wasm], [native], [registry] and [capabilities], each once`（`E0002`、kind `manifest_section_order`）。この文は次の 3 か所で固定されている: `src/package.rs` の 737 行（本体）と、テストの 1457・1546 行、`tests/packages.rs` の 1484 行。`[wasm]` はルートのパッケージのものだけを読む。
- **マニフェストの設定が効く範囲の前例**:
  - `src/main.rs` の `links_apply`: `[native]` は、`run`、native の exe か `shared` の `build`、native の `test`・`bench` にだけ効き、ほかの出力では読まない（エラーにしない）。
  - `BuildOptions::with_manifest_wasm`: `[wasm]` を WebAssembly の出力にだけ移す。
- **このマシンで確かめた回避策**（開発版、同梱の 0.1.6 と、MSVC 14.44 の CRT と SDK を使う clang 22.1.8）:
  - 同梱: `[native] link = ["gui_subsystem.c"]` に、`.drectve` の ` -subsystem:windows` を書いた C を渡す。exe の `Subsystem` は `IMAGE_SUBSYSTEM_WINDOWS_GUI (0x2)`（`llvm-readobj --file-headers`）。ハンドルなしで起動すると `def main` の値で終わった。ビルドキャッシュの項目（64 桁の 16 進のディレクトリ）は作られなかった。リンク入力のないビルドでは作られる。
  - MSVC: 同じ C の `#pragma comment(linker, "/SUBSYSTEM:WINDOWS /ENTRY:mainCRTStartup")` でも 0x2 になった。
  - `editbin /SUBSYSTEM:WINDOWS app.exe`（リンクの後の書き換え）でも 0x2 になり、動いた。
- **リンカーの指示（C の探りで確かめた）**:
  - 同梱（zig 0.16.0 の `zig cc`）: `-Wl,--subsystem,windows` で 0x2 になり、`int main` が呼ばれた。起票時の調査の記録では、`-mwindows` は zig が無視し、0x3 のままだった。
  - MSVC: `-Xlinker /subsystem:windows -Xlinker /entry:mainCRTStartup` で 0x2 になった。`lld-link` と `link.exe`（`-fuse-ld=link`）の両方で確かめた。`/entry` がないと、`link.exe` は `LNK1120`、`lld-link` は `libcmt.lib(exe_winmain.obj)` の `WinMain` の未解決でリンクに失敗した。
- **ハンドルなしの IO**: W08 の「現状」のとおり。

## 仕様

### 1. 書き方

コマンドライン:

```sh
tsuzuri build app --subsystem windows -o app.exe
tsuzuri run app --subsystem windows
```

ルートのマニフェスト:

```toml
[windows]
subsystem = "windows"
```

- 値は `console` か `windows`。既定はコンソールで、今と同じ。
- `--subsystem` は `build` と `run` だけが受ける。分離形（`--subsystem windows`）だけで、2 回書くと `E2000`。
- `--subsystem` を書いたときは、`[windows] subsystem` より優先する（`--subsystem console` でマニフェストの `windows` を打ち消せる）。
- `[windows]` は、依存パッケージのものを読まない（`[wasm]` と同じ）。

### 2. 効く範囲

| 指定 | 効く所 | それ以外 |
| --- | --- | --- |
| `--subsystem windows`・`console` | `build` の native の `--emit exe`、`run`。target はどちらも Windows（`*-pc-windows-msvc`・`*-pc-windows-gnu`、または Windows のホストの `native`） | 以下はすべて `E2000`: Windows 以外の target、WASM、exe 以外の `--emit`、`build`・`run` 以外のコマンド |
| `[windows] subsystem` | 同じ | 読まない（エラーにしない）。`test`・`bench` の runner と、Windows 以外の target のビルドと、exe 以外の出力（`links_apply`・`with_manifest_wasm` の前例） |

- `test`・`bench` の runner は、標準出力で結果を返す。そのため、マニフェストに `windows` があってもコンソールでリンクする。
- `run` は子に標準ハンドル（パイプ）を渡す。そのため、`run --subsystem windows` のプログラムの IO は、コンソールのときと同じに働く。

### 3. リンク

| `subsystem` | リンカー（`msvc_linker_for(platform)`） | Clang に足す引数 |
| --- | --- | --- |
| 指定なし・`console` | どちらでも | なし（今と同じ） |
| `windows` | 真（MSVC: `link.exe`・`lld-link`） | `-Xlinker /subsystem:windows -Xlinker /entry:mainCRTStartup` |
| `windows` | 偽（同梱の `tsuzuri-clang`、`*-pc-windows-gnu`） | `-Wl,--subsystem,windows` |

- 入口は今の C の `main` のまま。MSVC の CRT は `/entry:mainCRTStartup` で `main` を呼ぶ。MinGW の CRT は、GUI サブシステムでも、利用者の定義した `main` を呼ぶ（同梱で確かめた）。
- `-mwindows` は使わない。zig が無視する。
- 引数を足すのは、`build_complete` の、実行ファイルをリンクする Clang の実行の 1 か所だけ。`--backend fast` の実行ファイルも同じ所を通る。

### 4. 診断

| 条件 | コード | kind | メッセージ |
| --- | --- | --- | --- |
| `build`・`run` 以外での `--subsystem` | `E2000` | `option_invalid_for_command`（既存） | `--subsystem is only valid with build or run` |
| 値が `console`・`windows` でない | `E2000` | `option_invalid_value`（既存） | `--subsystem must be console or windows` |
| 2 回 | `E2000` | `duplicate_option`（既存） | `subsystem specified more than once` |
| WASM・exe 以外の `--emit` | `E2000` | `option_requires_build`（既存） | `--subsystem requires native executable output (--emit exe)` |
| Windows 以外の target | `E2000` | `option_unsupported_platform`（既存） | `--subsystem applies only to Windows targets; build for *-pc-windows-msvc or *-pc-windows-gnu, or remove it` |
| `[windows]` の未知のキー | `E0002` | `unknown_manifest_key`（既存） | `unknown windows key; expected subsystem` |
| 同じキーを 2 回 | `E0002` | `duplicate_manifest_key`（既存） | `windows keys must occur at most once` |
| 値が `"console"`・`"windows"` でない | `E0002` | `invalid_windows_setting`（新しい kind の案） | `subsystem must be "console" or "windows"` |
| 節の順序 | `E0002` | `manifest_section_order`（既存） | `expected [package] followed by optional [dependencies], [wasm], [native], [registry], [capabilities] and [windows], each once` |

## 設計

### `src/driver.rs`

```rust
/// The PE subsystem of a Windows executable (W09). `None` in `BuildOptions` is the console default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Subsystem {
    Console,
    Windows,
}
```

- `BuildOptions` に `pub subsystem: Option<Subsystem>` を足す。`Default` は `None`。`Copy` は保つ。`Debug` の文字列が変わるので、すべてのビルドキャッシュの鍵が 1 回だけ変わる。キャッシュの形式は安定の対象外（`docs/stability.md`）なので、これは受け入れる。
- `BuildOptions::validate`: `subsystem.is_some()` のとき、次の順に確かめる。
  1. `target == Native && emit == Executable`。違えば `OptionRequiresBuild`。
  2. `self.platform().windows()`。違えば `OptionUnsupportedPlatform`。
  順序は `validate_shared` に合わせる。
- `BuildOptions::with_manifest_windows(self, windows: &crate::package::WindowsSettings) -> Self`: `target == Native && emit == Executable && platform().windows()` のときだけ、`subsystem = self.subsystem.or(windows.subsystem)`。
- `fn subsystem_link_args(subsystem: Option<Subsystem>, msvc: bool) -> Vec<OsString>`: 仕様 3 の表。`build_complete` は、`sdk_args(platform)?`・`final_link_args(platform)?` を足す `if` の中で `subsystem_link_args(options.subsystem, msvc_linker_for(platform))` を足す。純粋な関数にするのは、テストがリンカーを選ばずに表を確かめられるようにするため。
- `Project` に `pub windows: crate::package::WindowsSettings` を足す（ルートのものだけ。`wasm` の隣で、同じ書き方で読む）。

### `src/main.rs`

- `--subsystem` の解析は、`--strip` の書き方で書く（重複・値・コマンド）。`HELP` に `--subsystem console|windows` の行を足す。
- `links_apply` の隣に `windows_apply(action, &options)` を足す。`Action::Build | Action::Run` で真。`with_manifest_wasm` を呼ぶ所（`arguments.options.with_manifest_wasm(project.wasm)`）の隣で、真なら `with_manifest_windows(&project.windows)` を呼ぶ。
- `rejects_ambiguous_or_unused_arguments` などの CLI のテストに、`--subsystem` の行を足す（GUIDE-LITE §7）。

### `src/package.rs`

```rust
/// The `[windows]` section of the root manifest (W09; W10 adds the resource keys).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WindowsSettings {
    pub subsystem: Option<crate::driver::Subsystem>,
}
```

- `Manifest` に `pub windows: WindowsSettings` を足す。`parse_manifest` は `cursor.take("[windows]")` で節を受け、`"windows"` の腕でキーを読む。キーは `subsystem` だけで、値は `cursor.string()?`。
- 順序の誤りの文を仕様 4 のとおりに直す。

### 入口と `wWinMain`

- 何も足さない（D3）。GUI のアプリに要る起動時のデータは、どれも今の仕組みか、ほかの W チケットで手に入る。
  - コマンド行は `def main :: Array<string> -> i32`（`arguments.c`）で受ける。
  - `hInstance` は `GetModuleHandleW` に NULL を渡して得る（引数は文字列（LPCWSTR）なので、正しい型は W15・W20 の後の `Maybe<ref ZString>` の `None`。それまでは W15 の早見のように構築しない `extern type` の `Maybe<H>` の `None` か、`i64` の `0` で宣言する）。
  - `nCmdShow` は、最初の `ShowWindow` に `SW_SHOWDEFAULT` を渡せば、OS が `STARTUPINFO` の値を使う（Win32 の `ShowWindow` の仕様）。

## 前提とする他チケットのインターフェース

- [W08](W08-io-without-std-streams.md): 標準ストリームが無いときの意味（`try_` の `Unavailable`（W08 の D2 が承認されなければ `WriteFailed`・`ReadFailed`）、非 `try_` のトラップ、`Debug.print` の `OutputDebugStringW`）。文書はこれを引き、W08 が出荷した形に合わせる。
- [W06](W06-windows-gui-test-harness.md) の仕様 4: `peHeaders`・`launchWithoutHandles`・`buildGui`。未着手なら同じ形で足す（着手条件）。

## 他チケットへ提供するインターフェース

W10 は、次の 5 つを使う。

- `crate::driver::Subsystem`。
- `BuildOptions::subsystem`。
- `crate::package::WindowsSettings`。W10 はキーを足す。
- `Project::windows`。
- `windows_apply`。W10 は、資源を埋め込む範囲もこれで決める。

`[windows]` の節の解析も W10 が引き継ぐ。W10 は、未知のキーの文の `expected ...` に自分のキーを足す。

## 実装手順

1. **変更**: なし。**内容**: `node scripts/repo-kb.mjs brief W09`。基準（`ci.yml`）を確かめる。`tests/fixtures` と `examples` の Windows の `--emit llvm` を保存する。リンクの引数は、Windows のホストで `--subsystem` なしのビルドを `TSUZURI_CLANG` の包みで記録する。**確認**: 記録。
2. **変更**: `src/package.rs`、`src/diagnostic_kinds.rs`、`tests/packages.rs`。**内容**: `WindowsSettings` と `[windows]` の解析、順序の文、新しい kind `invalid_windows_setting`。kind は G23 の登録簿の手順（GUIDE-LITE §6）で足す。**確認**: `cargo test --locked --test packages`、`src/package.rs` のテスト。
3. **変更**: `src/driver.rs`。**内容**: `Subsystem`、`BuildOptions::subsystem`、`validate`、`with_manifest_windows`、`subsystem_link_args`、`Project::windows`、リンクの 1 か所。**確認**: テスト `subsystem_link_args_follow_the_linker`・`validate_limits_subsystem_to_windows_executables`。T1。
4. **変更**: `src/main.rs`。**内容**: 解析、`windows_apply`、`HELP`、CLI のテスト。**確認**: `cargo test --locked --bin tsuzuri`（`running N tests` の N を確かめる）。
5. **変更**: `tests/windows.mjs`（GUI の段）。**内容**: テスト計画の E2E。**確認**: MSVC の開発者シェルと同梱の両方で `node tests/windows.mjs target/debug/tsuzuri.exe --stage gui`。
6. **変更**: 文書。**確認**: `node scripts/gen-docs.mjs`（kind の表）、`node scripts/check-cli-docs.mjs`、`node scripts/check-docs.mjs <変更したページ>`、T2、`ci:full` を付けた PR の `windows.yml`・`vscode.yml`（ARM64 の 2 job を含む）。

## テスト計画

### Rust

- `src/driver.rs`:
  - `subsystem_link_args_follow_the_linker`: 仕様 3 の表の 4 行を確かめる。`None` と `Some(Console)` は空。
  - `validate_limits_subsystem_to_windows_executables`: 仕様 4 の `E2000` の 2 行と、Windows の名前付き target（`x86_64-pc-windows-gnu`・`aarch64-pc-windows-msvc`）で通ること。どのホストでも走る。
  - `with_manifest_windows` が、exe でない出力と Windows でない target で何もしないこと。
- `src/main.rs`: `subsystem_is_a_build_and_run_option`（`strip_is_a_build_option_for_native_executables` の形）。対象は、受理、重複、不正な値、`test`・`check`・`bench` の拒否、`HELP` の行。
- `src/package.rs`・`tests/packages.rs`: `[windows]` の受理と、仕様 4 の `E0002` の 4 行。既存の 3 か所の順序の文の期待を直す（既存テストへの影響）。

### E2E（`tests/windows.mjs` の GUI の段。MSVC と同梱、`-O0`・`-O3`）

| 検査 | 期待 |
| --- | --- |
| `build --subsystem windows` の `def main`（7 を返す） | `peHeaders(...).subsystem` が 2。`launchWithoutHandles` で 7。ハンドルありの起動でも 7 |
| 既定と `--subsystem console` | 3 |
| マニフェストの `[windows] subsystem = "windows"` | 2。`--subsystem console` を足すと 3 |
| `run --subsystem windows` で `IO.writeln "hi"` | 標準出力が `hi\n`（パイプは渡る） |
| マニフェストに `windows` があるプロジェクトの `tsuzuri test` | 通る（runner はコンソール） |
| ビルドキャッシュ: 同じビルドを 2 回、次に `--subsystem console` で 1 回 | `TSUZURI_CACHE_DIR` の 64 桁の 16 進の項目が、1 回目と 2 回目で 1 個、3 回目の後で 2 個 |
| GUI サブシステムの exe の `IO.try_writeln` をハンドルなしで起動 | W08 の意味（`Unavailable`）。W08 が D2 の代わりの形なら `WriteFailed` |

## ドキュメント

- `compiler/option.md`:
  - 「出力とターゲット」に `--subsystem console|windows` の行を足す。
  - 「組み合わせがエラーになる場合」に仕様 4 の `E2000` を足す。
  - 「どのサブコマンドが受け付けるか」の表に `build`・`run` を足す。
- `organizing-tsuzuri/packages.md`: 節「[windows] セクション」を足す。中身は、`subsystem`、ルートだけ、効く範囲、コマンドラインの優先。W10 がキーを足す。
- `compiler/platforms.md`: 「OS ごとの違い」に、GUI サブシステムの実行ファイルと、標準ハンドルの無い起動の意味（W08 へのリンク）を足す。
- `compiler/native-interop.md`: 「GUI は言語の一部ではありません」の段落を残す。そのうえで、GUI サブシステムの exe は `--subsystem windows` で作り、窓は `extern` で Win32 を呼ぶ、と書き足す。GUI のアプリは `def main` を使うこと（W08 の D4。トップレベルの式の表示はストリームが無いと消える）も書く。
- `compiler/diagnostics.md` の kind の表（`node scripts/gen-docs.mjs`）。
- `README.md` の CLI の表（`check-cli-docs` が `--help` の旗を求める）。
- `docs/language.md` の「アプリケーションのエントリーポイント」: 入口は GUI サブシステムでも同じ `main` であること。

## 受け入れ条件

- [ ] 仕様 1〜4 のとおりに動く（Rust と E2E）。
- [ ] `--subsystem` も `[windows]` もないビルドで、次が変わらない:
  - `tests/fixtures` と `examples` の Windows と POSIX と wasm32 の `--emit llvm`。
  - `--emit object` と `--emit header` の出力。
  - リンクの引数（手順 1 の記録）。
  - 診断。例外は、マニフェストの節の順序の誤りの文だけ（`[windows]` を足すため。`[registry]`・`[capabilities]` を足した E10・E15 と同じ扱い）。
- [ ] `tsuzuri test`・`bench` は、マニフェストの `windows` を読まない。
- [ ] `ci:full` を付けた PR で、`windows.yml` と `vscode.yml` の Windows の job が通る。対象は x64 と ARM64、MSVC と同梱。
- [ ] `check-cli-docs`・`gen-docs --check`・変更したページの `check-docs`・T2 が通る。

## 落とし穴

- **`/entry:mainCRTStartup` を忘れない**: MSVC の CRT は、GUI サブシステムで `WinMain` を探し、リンクに失敗する。
- **`-Wl,` でカンマを含む値を渡さない**: MSVC 側は `-Xlinker` で 1 引数ずつ渡す（`pdb_link_args` と同じ）。
- **リンカーの種類は `msvc_linker_for(platform)` で決める**: Windows 以外のホストで、システムの Clang と `lld-link` で MSVC の target の exe をリンクする構成では、`msvc_linker` が偽なので MinGW の綴りになる。この構成は tier の外（`platforms.md`: MSVC の target はほかのホストでは object まで）なので、検査しない。広げるときは、`codeview_for` と同じく Clang の種類で判定する。
- **runner に付けない**: `test` と `bench` は、runner の標準出力を読む。`windows_apply` を `links_apply` からコピーして `Test`・`Bench` を残さない。
- **キャッシュの鍵**: マニフェストの本文は既に鍵に入っている。ただし、実際に効く値は `BuildOptions` の `Debug` の文字列で鍵に入れる（`--subsystem` のコマンドラインの値はマニフェストにない）。

## 対象外

- `wWinMain`・`WinMain` の生成と、起動時のデータ（`hInstance`・`nCmdShow`）の組み込み（D3）。
- サブシステムの版（`/subsystem:windows,6.02`）と、ほかのサブシステム（`native`・`efi_application`）。
- リソース（W10）と、セキュリティのリンクの旗（W30）。
- `--emit shared`（W05）のサブシステム（DLL には意味がない）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | CLI の `--subsystem console\|windows` と、ルートのマニフェストの `[windows] subsystem` を足す。D-55 に登録する | 1 回のビルドの指定（CLI）と、アプリの性質（マニフェスト）の両方がある。`[native]` の `link` と同じ二本立て。節の名前を `[windows]` にするのは、W10 のリソースのキーをまとめるため。`[native]` に入れると、依存パッケージの `native = true` の規則と混ざる | 要承認（承認前は該当する段に着手しない） |
| D2 | マニフェストの値は、効く出力（Windows の native の `build` の exe と `run`）にだけ移し、ほかでは読まない。CLI の値は、効かない所で `E2000` | `links_apply`・`with_manifest_wasm` の前例。GUI のプロジェクトでも `tsuzuri test` と WASM のビルドが通る | 既定案（実装者はこの案に従う） |
| D3 | `wWinMain` を生成せず、今の C の `main` と `/entry:mainCRTStartup` を使う | コマンド行は `def main :: Array<string> -> i32` で取れる。`hInstance` は `GetModuleHandleW(NULL)`、`nCmdShow` は `SW_SHOWDEFAULT` で足りる。入口を 2 つにすると、`console_def_main` の tzroutine の境界・引数の設定を二重に持つ | 既定案（実装者はこの案に従う） |
| D4 | リンカーへの綴りは、`msvc_linker_for(platform)` で MSVC の `/subsystem:windows /entry:mainCRTStartup` と MinGW の `--subsystem,windows` に分ける。`-mwindows` は使わない | 両方の綴りを試して確かめた。zig は `-mwindows` を無視する | 既定案（実装者はこの案に従う） |
| D5 | `BuildOptions::subsystem` は `Option<Subsystem>` にし、`None` を「指定なし」にする | 明示の `console` がマニフェストの `windows` を打ち消せる。`validate` は、指定のない既定に `E2000` を出さずに済む | 既定案（実装者はこの案に従う） |
