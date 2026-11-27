# M15: `.app` バンドルの生成（`--emit app`）と `[macos]` の manifest の節・`.dSYM`・`Env.executable_path`

| 項目 | 内容 |
| --- | --- |
| ID | M15 |
| 優先度 | P2（配布の土台。今は `.app`・`Info.plist`・`.dSYM` を手で組み、署名をやり直し、実行ファイルのパスは `extern` で引くしかない） |
| 規模 | L |
| 依存 | ([M17](_completed/M17-macos-min-version-and-weak-import.md)), ([M20](_completed/M20-macos-framework-link-inputs.md))（M17 は `LSMinimumSystemVersion`、M20 は `embed` した dylib の rpath） |
| 後続 | [M16](M16-macos-signing-notarization-hardening.md)（弱い後続: [M12](M12-gpu-presentation-to-window.md), [M13](M13-desktop-os-services-package.md)） |
| 状態 | todo |
| 起票 | 2026-11-26（D-67。利用者の依頼「「どのチケットにもない壁」と「調査中に見つけたコンパイラの不具合（自分で再現済み）」のそれぞれについて @_features/ に開発チケットまたは修正チケットを作成して」から起票。調査で報告した「配布 1（.app・Info.plist・アイコン・資源・.dSYM を作る支援がない。実行ファイルのパスを得る std の API もない）」に当たる。HEAD `804b7970` で確認） |
| 承認 | 承認済み（D-67 の委任で決定、2026-11-28。D1・D3・D4 と D-55 の登録）。起票時の記述: 要承認: [GUIDE D-67](GUIDE.md#d-67-tsuzuri-だけで-macos-のデスクトップアプリを書くm01m22) の D-55 の登録（`--emit app`、`[macos]` のキー `bundle_id`・`name`・`version`・`build`・`icon`・`resources`・`embed`・`info_plist`・`category`・`ui_element`、std の `Env.executable_path`）。決定事項 D1（`--emit app`）・D3（`[macos]` のキー）・D4（`Env.executable_path`）が要承認、ほかは既定案 |
| 改善する劣位 | Swift と Xcode は `.app`・`Info.plist`・`.dSYM` を作り、`Bundle.main.executablePath` を持つ。Go の `os.Executable`、Rust の `std::env::current_exe`、.NET の `Environment.ProcessPath` も実行ファイルのパスを返す。Tsuzuri は平らな実行ファイルと `<出力>.dwarf` しか作らず、bundle に入れると署名が壊れ、`-g` の `.dwarf` は bundle の署名を失敗させる。実行ファイルのパスを得る std の関数もない |
| 手本にする既存実装 | [W09](_completed/W09-windows-gui-subsystem.md) の `[windows]`（`src/package.rs` の `WindowsSettings`・`"windows"` の腕・`relative_path`・`Line::string_array`、`src/driver.rs` の `with_manifest_windows`）、[W10](W10-windows-resources.md) の資源の書き手（Rust で決定的なバイト列を書き、参照の道具の出力と照合する）、`src/driver.rs` の `protect_documentation`・`document` の公開（既存の出力の持ち主の検査と、一時ディレクトリからの入れ替えと復元）、`publish_outputs`・`TemporaryDirectory`、`-g` の `dsymutil --flat` の段（`src/driver.rs:5409-5483`）、`std/Env.tz` の `args` と内部の組み込み `Os.__args`（`unit` を取り、結果の置き場だけを渡す。`src/check.rs:3037-3051`、`src/llvm_io.rs:84・121-125`、`src/runtime/os.c:411` の `tsuzuri_os_args`）、`src/runtime/os.c` の `tz_os_current_directory`（長さを倍にして取り直す形）、使うときだけ付ける define（`src/driver.rs:4915-4923` の `-DTZ_TRAP_BOUNDARY`、5089-5094 行の WASI の `-DTZ_WASI_START`） |
| 主な影響ファイル | `src/main.rs`, `src/driver.rs`, `src/package.rs`, `src/cache.rs`, `src/plist.rs`（新規）, `src/app_bundle.rs`（新規）, `src/lib.rs`, `src/diagnostic_kinds.rs`, `src/runtime/os.c`, `src/runtime/os-windows.h`, `src/runtime/os-wasi.c`, `src/check.rs`, `src/llvm_io.rs`, `src/llvm.rs`, `src/polymorph.rs`, `src/pinned_rules.rs`, `src/capabilities.rs`, `src/test_runner.rs`, `std/Env.tz`, `tests/macos_bundle.rs`（新規）, `tests/macos_bundle.mjs`（新規）, `tests/fixtures/macos_bundle/`（新規）, `tests/packages.rs`, `tests/os.mjs`, `tests/os-wasi-host.mjs`, `tests/capabilities.rs`, `tests/fixtures/capabilities/std-table.txt`, `tests/fixtures/effects_sweep/Main.tz`, `.github/workflows/targets.yml`, `_tsuzuri/language-reference/organizing-tsuzuri/packages.md`, `_tsuzuri/language-reference/compiler/option.md`, `_tsuzuri/language-reference/compiler/platforms.md`, `_tsuzuri/language-reference/compiler/debugging.md`, `_tsuzuri/language-reference/compiler/usage.md`, `_tsuzuri/language-reference/built-in-types-and-modules/env.md`, `_tsuzuri/language-reference/compiler/diagnostics.md`（生成）, `README.md`, `docs/language.md`, `docs/architecture.md` |

## 実装者向け早見

- 範囲:
  - `tsuzuri build --emit app -o Name.app`。実行ファイルを `Name.app/Contents/MacOS/Name` に置き、`Contents/Info.plist`（決定的な XML）、`Contents/PkgInfo`、`Contents/Resources`（`icon` と `resources`）、`Contents/Frameworks`（`embed`）を作る。
  - 最後に bundle を内側（`Contents/Frameworks`）から ad-hoc で署名し直す。署名の Identifier は `bundle_id` になる。Developer ID の署名は [M16](M16-macos-signing-notarization-hardening.md) が足す。
  - `-g` では `Name.app.dSYM/Contents/Resources/DWARF/Name` と、その `Info.plist` を作る。今の平らな `.dwarf` を bundle の外へ移す。
  - `[macos]` に `bundle_id`・`name`・`version`・`build`・`icon`・`resources`・`embed`・`info_plist`・`category`・`ui_element` を足す。節は M17 が先に作っていればキーを足し、なければ作る。
  - `Env.executable_path :: own unit -> IO<Result<string, Os.Error>>`。macOS は `_NSGetExecutablePath` と `realpath`、Linux は `/proc/self/exe`、Windows は `GetModuleFileNameW`。
- 最初に読む: 「現状」の再現 3〜6 → 仕様 1〜6 → `src/driver.rs` の `build_complete` のキャッシュの段（4555-4600 行）と `-g` の段（5409-5483 行）と `publish_outputs`（5904 行）→ `protect_documentation`（3486 行）。
- 最初に書くテスト: `tests/macos_bundle.rs` の `info_plist_matches_plutil_canonical_form`（どのホストでも走る golden）と、`tests/packages.rs` の `manifest_macos_bundle_keys_parse`（`parse_manifest` は `pub`。`[windows]` の誤りの文の手本は `src/package.rs:1578` の単体テスト `rejects_malformed_capabilities`）。
- 触ってはいけない:
  - `--emit app` を使わないビルドの IR・object・リンク行・キャッシュの鍵・出力（手順 2〜6）。`[macos]` のキーがあっても、`--emit exe` は今と同じ実行ファイルを作る。手順 7 の `Env.executable_path` のランタイムは、それに到達したプログラムにだけ入る（仕様 6）。到達しないプログラムの object・実行ファイル・`.wasm` は、OS API を使っていても変わらない。
  - 実行ファイルのリンク行。`--emit app` のために rpath も `-sectcreate` も足さない（D12）。
  - `Env.args`・`Env.var`・`Env.current_dir` と、`Os.__read` の操作 0〜5（操作は足さない。D13）。
  - `test`・`bench`・`run` の runner。`[macos]` の bundle のキーを読まない（`Env.executable_path` に到達した runner の define だけは足す。仕様 6）。
- 完了の見分け方: 「仕様 7」の `Probe` を `[macos] bundle_id = "dev.example.probe"`・`ui_element = true` で `tsuzuri build . --emit app -g -o Probe.app` すると、次がすべて成り立つ。
  - `codesign --verify --deep --strict Probe.app` が通り、`codesign -dv Probe.app` の Identifier が `dev.example.probe` になる。
  - `plutil -lint Probe.app/Contents/Info.plist` が通り、その内容が `plutil -convert xml1` の出力とバイト単位で一致する。
  - `open Probe.app` で起動すると、`Env.executable_path` が `…/Probe.app/Contents/MacOS/Probe` を返し、`Env.current_dir` は `/` を返す。
  - `dwarfdump --uuid` の UUID が `Probe.app/Contents/MacOS/Probe` と `Probe.app.dSYM` で一致し、`lldb` が `target symbols add` なしでソースの行を出す。

## 目的

macOS のデスクトップアプリは `.app` の bundle で配る。Finder・Dock・LaunchServices・通知・Gatekeeper は、`Info.plist` の `CFBundleIdentifier` と bundle の署名でアプリを識別する。

今の Tsuzuri は平らな Mach-O の実行ファイルしか作らない。bundle は利用者がシェルで組む。手で組むと、次の 3 つの罠に必ず当たる（「現状」で再現した）。

- リンカーの署名（Identifier は `artifact`）のまま `Info.plist` を足すと、`codesign --verify --strict` が失敗する。署名をやり直す必要がある。
- `-g` の `<出力>.dwarf` が `Contents/MacOS` に出て、bundle の署名そのものが失敗する。
- 資源の写しが Finder の拡張属性を運ぶと、`codesign` が `detritus not allowed` で拒む。Rust の `std::fs::copy` もこの属性を写す。

Finder から起動すると作業ディレクトリは `/` になる。資源を探すには実行ファイルのパスが要るが、std にそれを返す関数がない。

このチケットは、bundle の組み立て・ad-hoc の署名・`.dSYM` をコンパイラの 1 つの出力の種類にし、実行ファイルのパスを std に足す。Developer ID の署名と公証（[M16](M16-macos-signing-notarization-hardening.md)）、bundle の中の GPU ランタイムの探索（[M12](M12-gpu-presentation-to-window.md)）、通知の bundle の識別子（[M13](M13-desktop-os-services-package.md)）が、この bundle の上に立つ。

## 着手条件と停止条件

### 着手条件

- `node scripts/repo-kb.mjs brief M15` を読む。
- 承認が記録されるまで、該当する段に着手しない。手順 3（`[macos]` のキー）は D3、手順 4〜6（`--emit app`）は D1、手順 7（`Env.executable_path`）は D4 の承認が要る。手順 1・2（調査と plist の読み書き）は承認なしで進めてよい。
- [M17](_completed/M17-macos-min-version-and-weak-import.md) が done なら、その `MacosSettings`・`Project::macos`・`invalid_macos_setting` にキーを足す。done でなければ、M17 の仕様 1 と同じ名前で `[macos]` の節を作る（後に入るチケットがキーを足す）。確認: `grep -nE "^\| (M17|M20) " _features/README.md`。
- [M20](_completed/M20-macos-framework-link-inputs.md) が done でなくても着手してよい。`embed` の dylib を rpath で読ませる E2E の段（テスト計画の E7）だけを、M20 の後に足す。
- macOS の挙動を変えるので、PR に `ci:full` を付ける。
- 基準の保存: `tests/fixtures/*/Main.tz` と `examples/*/Main.tz` の `--emit llvm -O1`（native と `--target wasm32`）と診断、`tests/packages.rs` の manifest の誤りの文を `target/scratch/m15/base/` に保存する。D13 の確かめのために、`Env.current_dir` だけを使うプログラム（仕様 7 の `Main.tz` から `executable` を作る行と出す行を除いたもの）の `--emit object` と実行ファイル（native の `-O0`・`-O3`）と、`--target wasm32 --wasm-host wasi` の `.wasm` も同じ所に保存する。

### 停止条件

- **S1** `--emit app` のために、実行ファイルのリンク行か、`--emit exe` のキャッシュの鍵を変える必要が出た。`[macos]` のないビルドは 1 byte も変えない（`Env.executable_path` に到達しないプログラムも同じ。仕様 6）。
- **S2** Rust の書き手の `Info.plist` が、`plutil -convert xml1` の出力とバイト単位で一致しない。しかも、どちらが正しいかを Apple の文書（`plist(5)`）で決められない。
- **S3** ad-hoc で署名した bundle が `codesign --verify --deep --strict` を通らない。しかも、原因が「現状」の 4 つの罠（署名のやり直し、`.dwarf`、`Contents/` の余計なファイル、拡張属性）のどれでもない。
- **S4** CI の macOS の runner（`targets.yml` の `macos-15`）で、`open` による起動が LaunchServices の制約で動かない。その段だけを止め、直接の起動に替えてよいかを報告する。
- **S5** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `804b7970` で確認）

macOS 27.2.1（arm64）、Apple clang 21、Apple ld（ld-27106.1）、`target/debug/tsuzuri`（HEAD で作り直したもの）で確かめた。

### コード

- manifest の節は `[package]`・`[dependencies]`・`[wasm]`・`[registry]`・`[capabilities]`・`[windows]`・`[native]` だけである（`src/package.rs:728-742`）。順序の誤りの文は `expected [package] followed by optional [dependencies], [wasm], [native], [registry], [capabilities] and [windows], each once` で、`src/package.rs:748・1494・1583` と `tests/packages.rs:1484` にある。
- `[windows]` の値は `WindowsSettings { subsystem }`（`src/package.rs:112-116`）で、ルートの manifest の節だけを読む。`Line` に `string`・`string_array`・`take("true")` がある（`src/package.rs:1106・1150・1244`）。パスは `relative_path`（`src/package.rs:1046-1054`）で、空・絶対・NUL を拒む。
- `--emit` の種類は `exe`・`object`・`llvm`・`header`・`wasm`・`wgsl`・`wgsl-relaxed`・`spirv`・`spirv-relaxed`・`shared`・`bitcode`・`bindings-js`・`bindings-cs`・`bindings-py`・`bindings-cpp` である（`src/main.rs:1203-1223`、`src/driver.rs:251-281` の `Emit`）。誤りの文は `src/main.rs:1222` と、そのテストの `src/main.rs:6487` にある。既定の出力のパスは `BuildOptions::output_path`（`src/driver.rs:1177-1195`）の網羅的な `match` で決まる。
- `build_complete`（`src/driver.rs:3851`）は、実行ファイルを一時ディレクトリの `artifact`（Windows は `artifact.exe`）にリンクし（`src/driver.rs:4515-4521`）、`publish_outputs`（`src/driver.rs:5904-5954`）が `fs::rename` で出力へ移す。Apple ld の ad-hoc の署名は、リンクしたときのファイル名を Identifier にする。そのため、どの出力名でも Identifier は `artifact` になる。
- `-g` の Mach-O の実行ファイルは、`<出力>.dwarf` を横に出す（`src/driver.rs:4475-4482` の `dwarf_sidecar`）。作るのは `dsymutil --flat`（`src/driver.rs:5409-5422` は共有ライブラリ、`5423-5483` は実行ファイル）である。
- キャッシュ: リンク入力のあるビルドはキャッシュしない（`src/driver.rs:4567-4568` の注記、`4589` 行の `links.is_empty()`）。`-g` の Mach-O の実行ファイルは、出力の絶対パスを鍵に入れる（`src/cache.rs:459-485` の `hash_output_path`）。鍵には `BuildOptions` の `Debug` の文字列も入る（`src/cache.rs:510`）。
- 既存の出力ディレクトリを置き換える手本は `tsuzuri doc` で、`.tsuzuri-docs` の印で持ち主を確かめ、ソースを含むディレクトリを拒む（`src/driver.rs:3486-3534` の `protect_documentation`）。
- `Env` は `args`・`var`・`current_dir` の 3 つである（`std/Env.tz:37・52・63`）。`current_dir` は `Os.__read 2i32` を呼ぶ（`std/Env.tz:21-25`）。ランタイムの操作は「0 ファイル、1 ディレクトリ、2 作業ディレクトリ（パスを読まない）、3 環境変数、4・5 stat」である（`src/runtime/os.c:363-392`、`src/runtime/os-windows.h:531-557`、`src/runtime/os-wasi.c:510-531`）。`Os.__read` を `Env` から呼ぶと権限 `env` になる（`src/capabilities.rs:1672` のテスト）。`tests/std_argument_order.rs:81` は `("Env", SingleArgument)` である。
- `Env.args` は別の組み込み `Os.__args :: unit -> (i64 * [ubyte])` で、結果の置き場だけを渡して `tsuzuri_os_args(ptr)` を呼ぶ（`src/check.rs:3037-3051`、`src/llvm_io.rs:84・121-125`）。`declare` は到達した組み込みの分だけ出る（`src/llvm_io.rs:95-96`）。`Os.__args` の権限はファイルによらず `env`（`src/capabilities.rs:446`）。ホストに届く私的な組み込みは 31 個で、`every_private_builtin_is_classified`（`src/capabilities.rs:1651-1670`）が数を固定する。OS の組み込みの一覧は、ほかに `src/llvm.rs:8510・8816`、`src/polymorph.rs:4436`（std の私的な組み込みの検査）、`src/pinned_rules.rs:133` にある（`rg -n 'Builtin::OsArgs' src`）。
- ランタイムの C は、到達した部品だけを 1 つの単位にまとめ、使うときだけ define を付けてコンパイルする（`-DTZ_TRAP_BOUNDARY`・`-DTZ_STACK_GUARD`・`-DTZ_ASYNC_PREEMPT` は `src/driver.rs:4915-4923`、WASI の `-DTZ_WASI_START`・`-DTZ_WASI_EXIT_CODE` は 5089-5094 行）。ランタイムのキャッシュの鍵は、単位の文字列とコンパイルの引数を含む（`src/cache.rs:279-306` の `runtime_key`、呼び出しは `src/driver.rs:6851`）。同梱の prebuilt のランタイムは、引数と単位の文字列の完全な一致でだけ使われ（`src/runtime_cache.rs:201-321`）、持つのは `numeric`・`math` と、`cpu.c`・`io.c` だけの単位である（`scripts/toolchain/runtime.mjs:67-72`）。`os.c` を含む単位と WASI の単位は prebuilt にない。
- `src/test_runner.rs` は driver と別に `os.c` をコンパイルする（debug runner の `native_runtime_sources` の 656 行、test・bench の `compile_native_runner` の 1053-1059 行）。どちらも IR に `declare i64 @tsuzuri_os_` があれば `os.c` を足す。
- WASI に手段のない操作の前例は `Process.run` で、`Os.Error { kind: Other, code: 52 }`（`TZ_WASI_NOSYS`）を返す（`src/runtime/os-wasi.c:623-633`、os.md:138）。
- std と `src/runtime` に、実行ファイルのパスを返す関数はない（`rg -n 'executable_path|current_exe|_NSGetExecutablePath|/proc/self/exe' std src/runtime` で 0 件）。
- `os.c` は OS API に到達したプログラムのランタイムとしてコンパイルされ（`src/driver.rs:1449` の `os_runtime_source_for`）、`--emit object` の出力にも入る（`Env.current_dir` だけを使う `Main.tz` の `--emit object` の `nm` に `T _tsuzuri_os_read` がある）。WASI では `os-wasi.c` が `.wasm` に入る。
- `tsuzuri run` は一時ディレクトリの実行ファイルを動かす。再現 1 の `Main.tz` を `tsuzuri run` すると `exe=/private/var/folders/…/T/.tsuzuri-<pid>-0/program` を出した。
- `Info.plist`・`codesign`・`.app` を扱うコードと文書はない（`rg -n 'Info\.plist|codesign|CFBundle' src docs _tsuzuri` で 0 件）。

### 再現 1: bundle の中への出力と署名の Identifier

次の `Main.tz` を使う。`_NSGetExecutablePath` は今の回避策で、仕様 6 の `Env.executable_path` がこれを置き換える。

```tsuzuri
@trusted
@count(1, none)
extern "_NSGetExecutablePath" def ns_get_executable_path :: ref mut [ubyte] -> ref mut i32u -> i32

def executable_path :: unit -> utf8string = \() ->
    let mut buffer: [ubyte] = new [ubyte](4096, \_ -> 0ubyte)
    let mut size = 4096i32u
    let status = ns_get_executable_path (ref mut buffer) (ref mut size)
    let mut n = 0
    while n < 4096 && buffer[n] != 0ubyte do
        n = n + 1
    let bytes: [ubyte] = new [ubyte](n, \i -> buffer[i])
    if status != 0i32 then u8"<too small>" else
        match Utf8String.from_bytes bytes with
        | Maybe.Some text -> text
        | Maybe.None -> u8"<invalid utf8>"

def main :: unit -> i32 = \() ->
    let! directory = Env.current_dir ()
    let cwd = match directory with
        | Result.Ok text -> text
        | Result.Error error -> Os.message (ref error)
    do! IO.writeln $"cwd={cwd}"
    do! IO.writeln $"exe={executable_path ()}"
    0
```

```text
$ tsuzuri build Main.tz -o Probe.app/Contents/MacOS/Probe
$ codesign -dv --verbose=4 Probe.app/Contents/MacOS/Probe
Identifier=artifact
CodeDirectory v=20400 size=897 flags=0x20002(adhoc,linker-signed) hashes=25+0 location=embedded
Info.plist=not bound
```

途中のディレクトリは作られる。

### 再現 2: `Info.plist` を足すと署名をやり直す必要がある

次の `Contents/Info.plist`（11 個のキー）と、`printf 'APPL????' > Probe.app/Contents/PkgInfo` を足す。

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleExecutable</key>
	<string>Probe</string>
	<key>CFBundleIdentifier</key>
	<string>dev.example.m15.probe</string>
	<key>CFBundleInfoDictionaryVersion</key>
	<string>6.0</string>
	<key>CFBundleName</key>
	<string>Probe</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>0.1.0</string>
	<key>CFBundleVersion</key>
	<string>1</string>
	<key>LSApplicationCategoryType</key>
	<string>public.app-category.developer-tools</string>
	<key>LSMinimumSystemVersion</key>
	<string>27.2</string>
	<key>LSUIElement</key>
	<true/>
	<key>NSHighResolutionCapable</key>
	<true/>
</dict>
</plist>
```

```text
$ plutil -lint Probe.app/Contents/Info.plist
Probe.app/Contents/Info.plist: OK
$ codesign --verify --deep --strict --verbose=2 Probe.app
Probe.app: code has no resources but signature indicates they must be present
$ codesign --force --sign - Probe.app
$ codesign -dv --verbose=4 Probe.app
Identifier=dev.example.m15.probe
CodeDirectory v=20400 size=430 flags=0x2(adhoc) hashes=7+3 location=embedded
Info.plist entries=11
Sealed Resources version=2 rules=13 files=0
$ codesign --verify --deep --strict --verbose=2 Probe.app
Probe.app: valid on disk
Probe.app: satisfies its Designated Requirement
```

同じ中身の bundle を 2 回 ad-hoc で署名すると、実行ファイルと `_CodeSignature/CodeResources` はバイト単位で一致した（ad-hoc の署名は決定的）。

署名し直さない bundle（リンカーの署名のまま `Info.plist` を足したもの）も、この機械の `open` では起動した。ただし `Identifier=artifact`・`Info.plist=not bound` のままで、`--verify --strict` を通らない。起動できるかどうかではなく、署名が bundle を表すかどうかが問題である。

### 再現 3: Finder・`open` からの起動

```text
$ open -W --stdout "$PWD/out.txt" --stderr "$PWD/err.txt" Probe.app
Unable to block on application (GetProcessPID() returned 18446744073709551016)
$ cat out.txt
cwd=/
exe=/Users/…/target/scratch/m15/exe/Probe.app/Contents/MacOS/Probe
```

起動は成功し、作業ディレクトリは `/` だった。すぐ終わるアプリでは `open -W` が待てずに上の 1 行を出す（終了コードは 0）。テストは `-W` に頼らず、出力のファイルを待つ。

### 再現 4: `-g` の `.dwarf` が bundle の署名を壊す

```text
$ tsuzuri build -g Main.tz -o G.app/Contents/MacOS/G
$ ls G.app/Contents/MacOS
G  G.dwarf
$ codesign --force --sign - G.app            # Info.plist を足した後
G.app: code object is not signed at all
In subcomponent: …/G.app/Contents/MacOS/G.dwarf
```

`G.dwarf` を `G.app.dSYM/Contents/Resources/DWARF/G` に移し、dSYM の `Info.plist`（`CFBundlePackageType` dSYM、`CFBundleIdentifier` `com.apple.xcode.dsym.<bundle_id>`）を置くと、署名が通り、UUID が一致し、lldb が自動で読んだ。

```text
$ codesign --force --sign - G.app && codesign --verify --deep --strict G.app
G.app: valid on disk
$ dwarfdump --uuid G.app.dSYM G.app/Contents/MacOS/G
UUID: BBAC25FF-052A-3E88-98A9-C5B363A1A06B (arm64) G.app.dSYM/Contents/Resources/DWARF/G
UUID: BBAC25FF-052A-3E88-98A9-C5B363A1A06B (arm64) G.app/Contents/MacOS/G
$ lldb --batch -o 'target create G.app' -o 'breakpoint set -n main'
Breakpoint 1: where = G`main + 32 [inlined] Env.working_directory at Env.tz:24:8, address = 0x00000001000009a4
```

dSYM を消すと、同じ breakpoint は `G`main` だけでソースの行を持たなかった。`codesign` は `LC_UUID` を変えなかった。後から `dsymutil` を走らせると、一時の object が消えているので何も得られない。

```text
$ dsymutil G.app/Contents/MacOS/G -o Later.dSYM
warning: (arm64) …/G.app/Contents/MacOS/.tsuzuri-16628-0/module.o unable to open object file: No such file or directory
warning: no debug symbols in executable (-arch arm64)
```

`-g` の実行ファイルは、同じ出力のパスでも 2 回のビルドでバイト列と UUID が違った（一時ディレクトリの名前がデバッグの対応表に入る）。dSYM は必ず同じビルドの実行ファイルと組にする。

参照として、`clang -g` で作った C の実行ファイルを `C.app/Contents/MacOS/C` に置き（`Info.plist` は再現 2 のもの）、`dsymutil C.app/Contents/MacOS/C -o C.app.dSYM`（`--flat` なし）を走らせた。dSYM の `Info.plist` のキーは仕様 5 の 7 つで、`CFBundleIdentifier` は `com.apple.xcode.dsym.dev.example.m15.probe`、`CFBundleShortVersionString`・`CFBundleVersion` は bundle の `0.1.0`・`1` だった。bundle の外の実行ファイルでは `com.apple.xcode.dsym.<ファイル名>`・`1.0`・`1` だった。

### 再現 5: `Contents/` の余計なファイル・拡張属性・`Frameworks` の中身

| 試したこと | `codesign --force --sign -`（通ったときは続く `--verify --deep --strict`） |
| --- | --- |
| `Contents/.tsuzuri-app`（持ち主の印の候補）を置く | 署名の時点で `code object is not signed at all`・`In subcomponent: …/Contents/.tsuzuri-app`（終了コード 1） |
| `Contents/Resources/note.txt` に `com.apple.FinderInfo` の拡張属性 | 署名の時点で `resource fork, Finder information, or similar detritus not allowed` |
| 同じファイルに `com.apple.quarantine` だけ | 通る |
| `Contents/Frameworks/notes.txt`（コードでないファイル） | 署名の時点で `code object is not signed at all`・`In subcomponent: …/Frameworks/notes.txt` |
| 署名のない dylib を `Contents/Frameworks` に置き、bundle だけ署名 | 署名の時点で `code object is not signed at all`・`In subcomponent: …/libm15.dylib` |
| 先に dylib を `codesign --force --sign -`、次に bundle | 通る |
| リンカーが署名した dylib（`adhoc,linker-signed`）を置き、bundle だけ署名 | 通る |

Rust の `std::fs::copy`（rustc 1.98.1）は、macOS で `com.apple.FinderInfo` を写した（`xattr -l` で確かめた）。資源の写しに使うと、上の 2 行目の誤りになる。

この機械では、新しく作ったファイル（`echo > f`、コンパイラの出力）すべてに OS が `com.apple.provenance` を付けた。この属性があっても署名と `--verify --deep --strict` は通った。写し手が拡張属性を写さなくても、`xattr -l` が空になるとは限らない。

### 再現 6: `embed` した dylib と rpath

`@rpath/libm15.dylib` の install name を持つ C の dylib を `--link` でリンクし、`Contents/Frameworks` に移した。

```tsuzuri
@trusted
extern "m15_value" def m15_value :: unit -> i32

def main :: unit -> i32 = \() -> m15_value ()
```

```text
$ printf 'int m15_value(void){return 7;}\n' > lib.c
$ clang -dynamiclib -install_name @rpath/libm15.dylib lib.c -o libm15.dylib
$ tsuzuri build Main.tz --link libm15.dylib -o App.app/Contents/MacOS/App
$ mkdir -p App.app/Contents/Frameworks && mv libm15.dylib App.app/Contents/Frameworks/
$ otool -l App.app/Contents/MacOS/App | grep LC_RPATH    # 出力なし
$ ./App.app/Contents/MacOS/App
dyld[20646]: Library not loaded: @rpath/libm15.dylib
  Reason: no LC_RPATH's found
$ install_name_tool -add_rpath @executable_path/../Frameworks App.app/Contents/MacOS/App
error: …/install_name_tool: changing install names or rpaths can't be redone for: App.app/Contents/MacOS/App (for architecture arm64) because larger updated load commands do not fit (the program must be relinked, and you may need to use -headerpad or -headerpad_max_install_names)
```

rpath はリンクのときにしか足せない。それは [M20](_completed/M20-macos-framework-link-inputs.md) の `--rpath` の範囲である。

### 再現 7: `plutil` の正準形

手で書いた `Info.plist`（キーを並べ、タブで字下げ）は、`plutil -convert xml1 -o -` の出力とバイト単位で一致した。`plutil` の規則は次のとおりだった。

- キーは UTF-16 の符号単位の順に並ぶ。`1`・`B`・`Z`・`_x`・`a`・`b` の順で、大文字と小文字を区別する。
- 字下げはタブ。`<true/>`・`<false/>` は空要素。
- 文字列の中で置き換えるのは `&`・`<`・`>`（`&amp;`・`&lt;`・`&gt;`）だけ。`"` とタブはそのまま。
- `<real>2.0</real>` は `<real>2</real>` に正規化される。

### 再現 8: LSMinimumSystemVersion の元

```text
$ otool -l hello | grep -A4 LC_BUILD_VERSION | grep minos          # 既定の native
    minos 27.2
$ otool -l hello12 | grep -A4 LC_BUILD_VERSION | grep minos        # --target aarch64-apple-darwin
    minos 12.0
```

既定の版は、D-67 の分岐 6 で 12.0 に固定すると決めた（2026-11-26）。実装は [M17](_completed/M17-macos-min-version-and-weak-import.md) の D1 で、M17 が入るまではビルド機の版のままである。

## 変えてはいけない意味

- `--emit app` を使わないビルド: IR・object・実行ファイル・リンク行・キャッシュの鍵・診断は変えない。例外は、受け付ける綴りを並べる既存の 2 つの誤りの文だけである。manifest の順序の誤りの文に `[macos]` が加わる（M17 と共有。先に入ったほうが変える）。`--emit` の種類の誤りの文（`src/main.rs:1222`、テストは 6487 行）に `app` が加わる（[M11](M11-swift-bindings.md) も同じ文に `bindings-swift` を足す）。
  - 手順 7（`Env.executable_path`）もこの例外にしない。本体は `os.c`・`os-windows.h`・`os-wasi.c` の `#if defined(TZ_OS_EXECUTABLE_PATH)` の中に置き、driver と `src/test_runner.rs` は IR が `tsuzuri_os_executable_path` を参照するときだけ `-DTZ_OS_EXECUTABLE_PATH` を付ける（仕様 6、D13）。`Env.executable_path` に到達しないプログラムのランタイムの部分（実行ファイル、`--emit object`、`--wasm-host wasi` の `.wasm`）は、OS API を使っていても 1 byte も変わらない。ランタイムのキャッシュの鍵は単位の文字列を含むので、`os.c` に節を足すと、OS API に到達するプログラムのランタイムの object が 1 回だけ作り直される。できる object は同じである（落とし穴）。
- `[macos]` の bundle のキーは、`--emit exe`・`run`・`test`・`bench` と Apple でない target では使わない。キーがあること自体は誤りにしない（`[windows]` と同じ）。値の形の検査（仕様 2）は、`[windows]` の `subsystem` と同じく manifest を読むときに常に行う（`subsystem = "bogus"` は macOS の `--emit exe` でも `E0002` になった）。
- `--emit app` の中の実行ファイルは、同じ options の `--emit exe` と同じキャッシュの項目を使い、署名し直す前は同じバイト列である（`-g` はビルドごとに UUID が変わるので、同じ項目に当たったときだけ）。違うのは `codesign` が書き換える署名だけである。
- `Env.args`・`Env.var`・`Env.current_dir` と、`Os.__read` の操作 0〜5 の意味。

## 仕様

### 1. `--emit app` と出力のパス

- `tsuzuri build <入力> --emit app [-o Name.app] [options]`。`build` だけが受け付ける。`run`・`test`・`bench`・`check` は今の `--emit` の扱いのとおり。
- 出力は `.app` で終わり、拡張子を除いた名前が空でないこと。違えば `E2000` `output_extension_invalid`。既定の出力は入力の拡張子を `app` に替えたもの（`Main.tz` は `Main.app`）。
- 拡張子を除いた名前を「実行ファイルの名前」と呼ぶ（`Name.app` の `Name`）。`Contents/MacOS/Name` と `CFBundleExecutable` に使う。
- ホストが macOS でなければ `E2000` `option_unsupported_platform`。target が Apple でなければ同じ `E2000`。`--target aarch64-apple-darwin` は受け付ける。
- 中の実行ファイルは、`--emit exe` と同じ options で作る。`-O`・`--cpu`・`-g`・`--strip`・`--link`・`-l`・`-L`・`--trap-info`・`--backend` などは、`--emit exe` で使える組み合わせだけを受け付け、同じ誤りを出す。
- ルートの `[macos] bundle_id` がなければ `E2000`（kind 案 `macos_bundle_id_missing`）。manifest のない単一ファイルのビルドも同じ。

### 2. `[macos]` のキー

```toml
[macos]
min_version = "13.0"                # M17
bundle_id = "dev.example.probe"     # CFBundleIdentifier（--emit app では必須）
name = "Probe"                      # CFBundleName
version = "1.2.3"                   # CFBundleShortVersionString
build = "42"                        # CFBundleVersion
icon = "assets/Probe.icns"          # Contents/Resources/Probe.icns と CFBundleIconFile
resources = ["assets/help", "assets/en.lproj"]
embed = ["vendor/libfoo.dylib"]     # Contents/Frameworks/libfoo.dylib
info_plist = "Extra.plist"          # 追加のキー
category = "public.app-category.developer-tools"
ui_element = true                   # LSUIElement
```

| キー | 値 | 誤り（`E0002` `invalid_macos_setting`） |
| --- | --- | --- |
| `bundle_id` | 英数字・`-`・`.` だけで、空の成分がなく、`.` で始まらず終わらない | `bundle_id must be a reverse-DNS name of letters, digits, '-' and '.', such as "dev.example.app"` |
| `name` | 空でなく、制御文字と `/` を含まない | `name must be nonempty and must not contain '/' or control characters` |
| `version`・`build` | `.` で区切った 1〜3 個の 10 進の整数。各部分は 0〜2147483647 | `version must be one to three integers separated by '.', such as "1.2.3"`（`build` も同じ形の文） |
| `icon` | 相対パス。拡張子は `.icns` | `icon must be a relative path to an .icns file` |
| `resources`・`embed` | 相対パスの 1 行の配列。ファイルかディレクトリ | `macos paths must be nonempty relative paths` |
| `info_plist` | 相対パス | 同上 |
| `category` | `public.app-category.` の後に英小文字と `-` | `category must start with "public.app-category.", such as "public.app-category.developer-tools"` |
| `ui_element` | `true` か `false` | `ui_element must be true or false` |

- パスは `relative_path` で検査し、ルートのパッケージに対して解決する（`[native]` と同じ）。
- `resources` と `icon` の写し先の名前（最後の成分）が重なれば `E0002` `invalid_macos_setting`（文案 `resources and icon must not repeat the file name 'x'`）。`embed` の中の重なりも同じ。
- 知らないキーは `E0002` `unknown_manifest_key`（文案 `unknown macos key; expected min_version, bundle_id, name, version, build, icon, resources, embed, info_plist, category, or ui_element`。M16 が後ろに 4 つ足す）。重なったキーは `duplicate_manifest_key`。
- 依存パッケージの `[macos]` は読まない（M17 の D2）。
- 省略したキーは `Info.plist` に書かない。`[package]` の `name`・`version` から補わない（GUIDE §1 規則 2。D3）。

### 3. bundle の形

```text
Name.app/
  Contents/
    Info.plist
    PkgInfo                       "APPL????"（8 バイト、改行なし）
    MacOS/Name                    実行ファイル
    Resources/<icon の名前>        icon があるとき
    Resources/<各 resources の名前>
    Frameworks/<各 embed の名前>   embed があるとき
    _CodeSignature/CodeResources  codesign が作る
Name.app.dSYM/                    -g のとき
  Contents/Info.plist
  Contents/Resources/DWARF/Name
```

- `resources` のディレクトリは中身ごと写す（`assets/en.lproj` は `Resources/en.lproj`）。
- `embed` に置けるのは、Mach-O のファイル（先頭の 4 バイトが `cf fa ed fe`（`MH_MAGIC_64` の little endian）か、fat の `ca fe ba be`・`ca fe ba bf`（`FAT_MAGIC`・`FAT_MAGIC_64` の big endian））と、`.framework` で終わるディレクトリだけである。それ以外は `E2001`（kind 案 `macos_bundle_input_invalid`、文案 `'notes.txt' in embed is neither a Mach-O file nor a .framework directory; put it in resources`）。理由は再現 5 の 4 行目。
- 写しは中身のバイトと実行の権限（`0o755` か `0o644`）だけを写す。拡張属性・ACL・時刻は写さない（再現 5）。シンボリックリンクは、相対で、写す木の外へ出ないものだけを同じリンクとして写す。それ以外は `E2001` `macos_bundle_input_invalid`。
- `icon` の中身は先頭 4 バイトが `icns` で、続く big endian の u32 がファイルの長さに等しいこと。違えば `E2001` `macos_bundle_input_invalid`（文案 `'App.icns' is not an icns file: expected the 'icns' magic and its length`）。

### 4. `Info.plist`

コンパイラが書くキー（すべて `plist` の辞書の最上位）:

| キー | 値 | いつ |
| --- | --- | --- |
| `CFBundleExecutable` | 実行ファイルの名前 | 常に |
| `CFBundleIdentifier` | `bundle_id` | 常に |
| `CFBundleInfoDictionaryVersion` | `6.0` | 常に |
| `CFBundlePackageType` | `APPL` | 常に |
| `LSMinimumSystemVersion` | 実行ファイルの `LC_BUILD_VERSION` の `minos`（`12.0`・`13.0`・`27.2` の形。D10） | 常に |
| `NSHighResolutionCapable` | `true` | 常に |
| `CFBundleName` | `name` | `name` があるとき |
| `CFBundleShortVersionString` | `version` | あるとき |
| `CFBundleVersion` | `build` | あるとき |
| `CFBundleIconFile` | `icon` の名前（拡張子つき） | あるとき |
| `LSApplicationCategoryType` | `category` | あるとき |
| `LSUIElement` | `ui_element` の真偽 | あるとき |

- 形は XML の plist で、`plutil -convert xml1` の出力と同じバイト列にする（再現 7）。先頭は `<?xml version="1.0" encoding="UTF-8"?>`、Apple の DOCTYPE、`<plist version="1.0">`。キーは UTF-16 の符号単位の順、字下げはタブ、改行は LF、最後に改行が 1 つ。
- `info_plist` は追加のキーの plist である。最上位は辞書でなければならない。そのキーが上の表のキーと重なれば、生成したかどうかによらず `E2000`（kind 案 `macos_bundle_conflict`、文案 `Info.plist key 'CFBundleIdentifier' comes from both [macos] bundle_id and 'Extra.plist'; remove one`）。値は書き換えずに併合し、全体を同じ規則で並べて書く。
- `info_plist` の読み手は XML の plist の部分集合だけを受ける（D2）。要素は `dict`・`array`・`string`・`integer`・`real`・`true`・`false`・`date`・`data`。実体参照は定義済みの 5 つと数値文字参照だけ。コメントと XML 宣言と Apple の DOCTYPE は読み飛ばす。CDATA・ほかの処理命令・ほかの DOCTYPE は誤り。深さは 128 まで、ファイルは 1 MiB まで。辞書のキーの重なりは誤り。`bplist00` で始まるファイルは、`plutil -convert xml1` を勧める誤りにする。どれも `E2001` `macos_bundle_input_invalid`。
- `real`・`integer`・`date`・`data` の文字列は、形を検査したうえで書かれたまま写す。`plutil` と同じ正規化はしない（「落とし穴」）。

### 5. 署名・公開・`.dSYM`

- 署名は ad-hoc だけ（M16 が identity を足す）。順は内側から: `embed` の各 Mach-O のファイルと各 `.framework` に `codesign --force --sign - <パス>`、最後に `codesign --force --sign - <bundle>`。`--deep` は使わない。`--options runtime` は付けない（M16 の `hardened_runtime`）。Identifier は `Info.plist` から `codesign` が決める（再現 2）。
- `codesign` は PATH の `codesign` を使う（M16 が `TSUZURI_CODESIGN` を足す）。走らせられなければ `E2002` `external_tool_not_runnable`、失敗すれば `E2002` `external_tool_failed`。助言は `macOS application bundles are signed with codesign from /usr/bin or the Xcode Command Line Tools`。
- `-g`: `dsymutil --flat` の出力を `Name.app.dSYM/Contents/Resources/DWARF/Name` に置き、dSYM の `Info.plist` を書く。キーは、`dsymutil`（bundle の形の出力）が bundle の中の実行ファイルに書く 7 つと同じにする: `CFBundleDevelopmentRegion` English、`CFBundleIdentifier` `com.apple.xcode.dsym.<bundle_id>`、`CFBundleInfoDictionaryVersion` 6.0、`CFBundlePackageType` dSYM、`CFBundleSignature` `????`、`CFBundleShortVersionString`（`version`。なければ `1.0`）、`CFBundleVersion`（`build`。なければ `1`）。`dsymutil` は bundle の `Info.plist` からこの 2 つを写し、bundle の外では `1.0`・`1` を書いた（再現 4）。書き方は仕様 4 と同じ正準形で、`dsymutil` の DOCTYPE（`-//Apple Computer//`）と字下げには合わせない。平らな `.dwarf` は残さない。
- `-g` のないビルドは `Name.app.dSYM` を作らず、既存のものにも触れない（UUID が合わないので lldb は読まない）。
- 組み立ては出力の横の一時ディレクトリで行い、最後に入れ替える。既存の `Name.app`（と `-g` の `Name.app.dSYM`）は、退避してから新しいものを移し、失敗したら戻す（`document`（`src/driver.rs:3429-3484`）と同じ手順）。途中で失敗したら、前の bundle が残り、中途半端な bundle は残らない。
- 既存の `Name.app` を置き換えてよいのは、シンボリックリンクでないディレクトリで、その `Contents/Info.plist` を仕様 4 の読み手で読めて、`CFBundleIdentifier` が今の `bundle_id`、`CFBundleExecutable` が今の実行ファイルの名前に等しいときだけである。dSYM は `CFBundlePackageType` が `dSYM` で、識別子が `com.apple.xcode.dsym.<bundle_id>` のときだけ。違えば `E2003`（kind 案 `app_output_not_owned`、文案 `'Name.app' exists and is not a bundle that tsuzuri built for 'dev.example.probe'; remove it or choose another output`）。印のファイルは置かない（再現 5 の 1 行目）。
- 出力の bundle がプロジェクトのソースか manifest かリンク入力を含めば `E2003` `output_overwrites_input`・`output_overwrites_link_input`（既存）。

### 6. `Env.executable_path`

```tsuzuri
def executable_path :: own unit -> IO<Result<string, Os.Error>>
```

- いま動いているプロセスの実行ファイルの絶対パスを、実行したときに OS に聞く。権限は `env`（ほかの `Env` と同じ）。
- macOS: `_NSGetExecutablePath`（長さが足りなければ返された長さで取り直す）の結果を `realpath` で正規化する。
- Linux: `readlink("/proc/self/exe")`（正規化された絶対パス。長さは倍に増やして取り直し、1 MiB で `Other`）。
- Windows: `GetModuleFileNameW(NULL, …)`。切り詰め（`ERROR_INSUFFICIENT_BUFFER` か戻り値がバッファの長さ）なら倍に増やして取り直す。UTF-16 を UTF-8 にして返す。正規化はしない。
- `--wasm-host wasi`: `Os.Error { kind: Other, code: 52 }`（ENOSYS。`Process.run` と同じ）。既定の wasm32 はほかの `Env` と同じく `E2000`。
- 不正な UTF-8（POSIX）・孤立サロゲート（Windows）は `InvalidEncoding`（`code` 0）で、置換しない。OS の失敗は `Os.Error` の `code` に errno か Win32 のコードを入れる。
- `tsuzuri run` と `test` は、コンパイラが作った一時の実行ファイルのパスを返す（文書に書く）。

ランタイムは独立した入口 `tsuzuri_os_executable_path` にし、std の内部の組み込み `Os.__executable_path :: unit -> (i64 * [ubyte])` から呼ぶ。形は `Os.__args` と同じで、結果の置き場だけを渡す（`declare i64 @tsuzuri_os_executable_path(ptr)`）。本体は `os.c`・`os-windows.h`・`os-wasi.c` の `#if defined(TZ_OS_EXECUTABLE_PATH)` の中に置く。IR がこの symbol を参照するとき（`runtime_symbols.contains("tsuzuri_os_executable_path")`）だけ、次の 3 か所で `-DTZ_OS_EXECUTABLE_PATH` を付ける。

- driver の native のランタイムのコンパイル。`-DTZ_TRAP_BOUNDARY`・`-DTZ_STACK_GUARD`・`-DTZ_ASYNC_PREEMPT` と同じ形である（`src/driver.rs:4915-4923`）。
- driver の WASI のランタイム（`os-wasi.c`）のコンパイル。`-DTZ_WASI_START` の隣（`src/driver.rs:5089-5094`）。
- `src/test_runner.rs` の `native_runtime_sources`（656 行）と `compile_native_runner`（1053-1059 行）が `os.c` をコンパイルするとき。付けないと、`Env.executable_path` を呼ぶ `tsuzuri test`・`bench`・debug runner のリンクが `tsuzuri_os_executable_path` の未定義で失敗する。

ランタイムのキャッシュの鍵はコンパイルの引数を含む（`src/driver.rs:6851` の `runtime_key`）ので、define の有無で別の項目になる。prebuilt のランタイムは `os.c` を含む単位を持たない（`scripts/toolchain/runtime.mjs:67-72`）ので、define が prebuilt の照合を外すこともない。`Os.__read` に操作 6 を足す案は採らない。OS API に到達するすべてのプログラムのランタイムの object が変わるからである（D13）。

### 7. 例（受け入れの確認に使う）

```text
Probe/
  Tsuzuri.toml
  Main.tz
  assets/Probe.icns
```

```toml
[package]
name = "probe"
version = "0.1.0"

[macos]
bundle_id = "dev.example.probe"
name = "Probe"
version = "0.1.0"
build = "1"
icon = "assets/Probe.icns"
ui_element = true
```

```tsuzuri
def main :: unit -> i32 = \() ->
    let! directory = Env.current_dir ()
    let! executable = Env.executable_path ()
    do! IO.writeln (match directory with
        | Result.Ok text -> "cwd=" + text
        | Result.Error error -> Os.message (ref error))
    do! IO.writeln (match executable with
        | Result.Ok text -> "exe=" + text
        | Result.Error error -> Os.message (ref error))
    0
```

`open --stdout out.txt Probe.app` の後、`out.txt` は `cwd=/` と `exe=<絶対パス>/Probe.app/Contents/MacOS/Probe` の 2 行になる。

### 8. 診断

| 条件 | コード | kind | 文の例 |
| --- | --- | --- | --- |
| `[macos]` の値の形・重なった名前 | `E0002` | `invalid_macos_setting`（M17 と共有。先に入ったチケットが登録する） | 仕様 2 の表 |
| 知らないキー・重なったキー | `E0002` | `unknown_manifest_key`・`duplicate_manifest_key`（既存） | 仕様 2 |
| macOS でないホスト・Apple でない target | `E2000` | `option_unsupported_platform`（既存） | `'--emit app' needs a macOS host: the bundle is signed with codesign`、`'--emit app' builds a macOS application; build for aarch64-apple-darwin or remove it` |
| 出力が `.app` でない | `E2000` | `output_extension_invalid`（既存） | `app output must end with '.app'; its file name, without the extension, names the executable` |
| `bundle_id` がない | `E2000` | `macos_bundle_id_missing`（新しい kind の案） | `'--emit app' needs [macos] bundle_id in the root Tsuzuri.toml` |
| `Info.plist` のキーの重なり | `E2000` | `macos_bundle_conflict`（新しい kind の案） | 仕様 4 |
| 入力を読めない | `E2001` | `io_failure`（既存） | `cannot read macOS bundle input 'assets/Probe.icns': …` |
| 入力の中身が不正 | `E2001` | `macos_bundle_input_invalid`（新しい kind の案） | 仕様 3・4 |
| `codesign` を走らせられない・失敗 | `E2002` | `external_tool_not_runnable`・`external_tool_failed`（既存） | 仕様 5 |
| 既存の出力の持ち主が違う | `E2003` | `app_output_not_owned`（新しい kind の案） | 仕様 5 |
| 出力がソース・リンク入力を含む | `E2003` | `output_overwrites_input`・`output_overwrites_link_input`（既存） | 既存の文 |

## 設計

### 段ごとの変更

| ファイル | 変更 |
| --- | --- |
| `src/main.rs` | `Some("app") => Emit::App`、誤りの文（1222 行と 6487 行のテスト）に `app` を足す。`HELP` の `--emit` の行 |
| `src/driver.rs` | `Emit::App`。`output_path` の腕（`app`）。`BuildOptions::validate`（`src/driver.rs:509`）にホストと target の検査（同じ関数の中の、`-g` の macOS ホストの検査（`src/driver.rs:1058-1075`）と同じ形）。`build` の入口で `Emit::App` を `app_bundle::build_app` へ分ける。`build_complete` に「鍵に入れる出力のパス」を渡す口を足す（下の「キャッシュ」） |
| `src/package.rs` | `MacosSettings` に 10 個のフィールド（`bundle_id: Option<String>`、`name`、`version`、`build`、`icon: Option<PathBuf>`、`resources: Vec<PathBuf>`、`embed: Vec<PathBuf>`、`info_plist: Option<PathBuf>`、`category`、`ui_element: Option<bool>`）。`"macos"` の腕のキー。M17 が未着手なら節そのもの |
| `src/plist.rs`（新規） | `enum Value { String, Integer(String), Real(String), Bool, Date(String), Data(String), Array(Vec<Value>), Dict(BTreeMap<Utf16Key, Value>) }`、`fn write(&Value) -> Vec<u8>`、`fn read(&[u8]) -> Result<Value, String>`。`Utf16Key` は UTF-16 の符号単位の列で順序を決める |
| `src/app_bundle.rs`（新規） | `build_app`: 入力の検査 → 出力の持ち主の検査 → 一時ディレクトリでの実行ファイルのビルド → `.dSYM` → `Info.plist`・`PkgInfo`・資源・`embed` → 署名 → 入れ替え。`fn sign(plan: &SignPlan, bundle: &Path, nested: &[PathBuf])`（M16 が `SignPlan` に identity などを足す）。`fn mach_o_min_version(&[u8]) -> Option<(u32, u32, u32)>` |
| `src/diagnostic_kinds.rs` | 新しい kind 4 つ（`macos_bundle_id_missing`・`macos_bundle_conflict`・`macos_bundle_input_invalid`・`app_output_not_owned`）。`invalid_macos_setting` は M17 が先なら既にある |
| `src/runtime/os.c`・`os-windows.h`・`os-wasi.c` | `#if defined(TZ_OS_EXECUTABLE_PATH)` の中の `tsuzuri_os_executable_path`（`tsuzuri_os_args` と同じく `struct tz_os_buffer *output` だけを取り、status を返す。`src/runtime/os.c:411`、`os-wasi.c:539`）。`os.c` は `__APPLE__` で `<mach-o/dyld.h>` の `_NSGetExecutablePath`、ほかは `/proc/self/exe`。[M12](M12-gpu-presentation-to-window.md) の Phase 1 の補助（`gpu-vulkan.c`・`gpu.c`）も同じ求め方をするので、`realpath` を通してから使う規則をそろえる（後に入るチケットが合わせる）。失敗したときは、M15 は `Os.Error` を返し、M12 は bundle の候補を飛ばす。`Os.__read` の操作の一覧（0〜5）は変えない |
| `src/check.rs`・`src/llvm_io.rs` | 内部の組み込み `Os.__executable_path`（`Builtin::name` の一覧と scheme は `Os.__args` の腕と同じ `unit -> (i64 * [ubyte])`。下げ方は `src/llvm_io.rs:84` の `("args", "ptr")` と同じ形で、`tsuzuri_os_set_args` は宣言しない）。名前は `__` で始まり、`std/Env.tz` だけが呼ぶ |
| `src/llvm.rs`・`src/polymorph.rs`・`src/pinned_rules.rs`・`src/capabilities.rs` | `Builtin::OsArgs` を並べる一覧に足す（`src/llvm.rs:8510・8816`、`src/polymorph.rs:4436`、`src/pinned_rules.rs:133`。漏れは `rg -n 'Builtin::OsArgs' src` で確かめる）。`primitive` に `"Os.__executable_path" => Env`、`every_private_builtin_is_classified` の 31 を 1 増やす（M17 の `Os.__os_version` も増やす） |
| `src/driver.rs` | `runtime_symbols.contains("tsuzuri_os_executable_path")` のときだけ、native のランタイムのコンパイル（`trap_runtime` の `-DTZ_TRAP_BOUNDARY` の隣、4915-4923 行）と WASI のランタイムのコンパイル（`-DTZ_WASI_START` の隣、5089-5094 行）に `-DTZ_OS_EXECUTABLE_PATH` を足す |
| `src/test_runner.rs` | `native_runtime_sources`（656 行）と `compile_native_runner`（1053-1059 行）でも、IR が `tsuzuri_os_executable_path` を宣言するときだけ `os.c` のコンパイルに `-DTZ_OS_EXECUTABLE_PATH` を足す |
| `tests/capabilities.rs`・`tests/fixtures/capabilities/std-table.txt`・`tests/fixtures/effects_sweep/Main.tz` | ホストに届く名前の一覧に `Os.__executable_path`、`Env.tz` の 3 を 4、合計 44 を 45、std-table に `Env.executable_path: env`（88 を 89）。公開の std の関数はどれもコーパスに実体が要る（`tests/effects_alloc_conservative.rs` の F.4）ので、sweep の fixture に `Env.executable_path ()` を足す。[M17](_completed/M17-macos-min-version-and-weak-import.md) の `Env.os_version` と同じ数を直すので、後に入るほうが足す |
| `std/Env.tz` | `private def executable_file` と `def executable_path`（`current_dir` と同じ形で、`Os.__read 2i32` の代わりに `Os.__executable_path ()` を呼ぶ。`///` の例を付ける） |

### 組み立ての流れ（`build_app`）

1. options を検査し、実行ファイルの名前と `MacosSettings` を得る。`bundle_id` がなければ誤り。
2. 入力（`icon`・`resources`・`embed`・`info_plist`）を読んで検査する。`Info.plist` の値をこの時点で組み、キーの重なりを報告する。実行ファイルをビルドする前に失敗させる。
3. 出力の持ち主を検査する（仕様 5）。
4. `TemporaryDirectory::new(出力の親)` に `Name.app/Contents/MacOS/` を作り、`build_complete` を `emit = Emit::Executable` の options で呼ぶ。出力は一時の `…/Contents/MacOS/Name`、鍵に入れる出力は最終の `<出力>/Contents/MacOS/Name` にする。
5. 実行ファイルの Mach-O の `LC_BUILD_VERSION`（`cmd` 0x32）を読み、`minos`（`xxxx.yy.zz` を nibble で詰めた u32）から `LSMinimumSystemVersion` を作る。読めなければ内部の誤り（仕様 4 の「常に」のキーなので黙って省かない）。
6. `-g` なら `<一時の実行ファイル>.dwarf` を `Name.app.dSYM/Contents/Resources/DWARF/Name` へ `fs::rename` し、dSYM の `Info.plist` を書く。
7. `Info.plist`・`PkgInfo` を書き、`icon`・`resources` を `Resources`、`embed` を `Frameworks` へ写す（拡張属性を写さない写し。`fs::copy` は使わない）。
8. 署名する（仕様 5）。
9. 入れ替える（仕様 5）。`TemporaryDirectory` を閉じる。

### キャッシュ（D5）

- 中の実行ファイルは `emit = Emit::Executable` の options で作るので、鍵の `options` の文字列は `--emit exe` と同じになる。`-g` でなければ出力のパスは鍵に入らないので、同じソースの `--emit exe` とキャッシュの項目を共有する。
- `-g` の鍵は出力の絶対パスを含む（`hash_output_path`）。一時ディレクトリの名前はビルドごとに違うので、そのままでは当たらない。`build_complete` に「鍵に入れる出力のパス」を足し、`--emit app` は最終のパス `<出力>/Contents/MacOS/Name` を渡す。ほかの呼び出しは今の `output` を渡し、鍵は変わらない（S1）。
- bundle の組み立て（手順 5〜9）は、キャッシュに当たっても毎回行う。資源・`Info.plist` の中身は鍵に入れない。費用は写しと ad-hoc の署名だけである。
- リンク入力（`--link`・`[native]`、M20 の framework）があれば、今どおりキャッシュしない。

### `Env.executable_path` の C

- 形は `tz_os_current_directory`（`src/runtime/os.c:315-331`）に合わせる。確保に失敗すれば `Other`（ENOMEM）、上限（`TZ_OS_MAX_CURRENT_DIR`、1 MiB）を超えれば `Other`（ERANGE）、成功すれば `tz_os_deliver` で渡す。入口は `tsuzuri_os_args` と同じく最初に `tz_os_clear(output)` を呼び、`tsuzuri_os_read` と同じく `TZ_BLOCKING_ENTER`・`TZ_BLOCKING_EXIT` で包む（`realpath` はファイルシステムを読む。TZ08）。
- macOS は `#if defined(TZ_OS_EXECUTABLE_PATH) && defined(__APPLE__)` の中で `<mach-o/dyld.h>` を読む。ファイルの先頭の include の並び（`src/runtime/os.c:144-163`）には足さない。define のない単位の前処理の結果を変えないためである。`_NSGetExecutablePath` が -1 を返したら、返された長さで取り直す。`realpath(raw, NULL)` の失敗は errno の `Os.Error`。
- `os-wasi.c` の `tsuzuri_os_executable_path` は `tz_clear(output)` の後に `tz_status(TZ_OS_OTHER, TZ_WASI_NOSYS)` を返す（`tsuzuri_os_spawn` と同じ。`src/runtime/os-wasi.c:623-633`）。
- 3 つの本体とも `#if defined(TZ_OS_EXECUTABLE_PATH)` の中に置く。HEAD の `os.c` の `tsuzuri_os_args` の前に `#if defined(TZ_OS_EXECUTABLE_PATH)` の節（`<mach-o/dyld.h>` の include を含む）を足し、Apple clang 21 の `clang -std=c11 -c -fPIC -O1 -ffunction-sections -fdata-sections` で比べた。define なしの object は節を足す前と `cmp` で一致し、define ありの object だけが `T _tsuzuri_os_executable_path` を持った。define のないときもあるときもコンパイルが通ることは、手順 7 の 2 通りのビルドで確かめる（`sh scripts/check-runtime-includes.sh` が確かめるのは、`include_str!` するファイルが Git に入っていることだけである）。

## 前提とする他チケットのインターフェース

- [M17](_completed/M17-macos-min-version-and-weak-import.md)（弱い依存）: `MacosSettings`・`Project::macos`・`MacosVersion`・`effective_macos_version`、kind `invalid_macos_setting`。M15 は `LSMinimumSystemVersion` を実行ファイルの `minos` から作るので、M17 の関数を呼ばない。M17 の後は、`[macos] min_version` の値が `minos` を通って `Info.plist` に届くことをテストで確かめる。
- [M20](_completed/M20-macos-framework-link-inputs.md)（弱い依存）: `--rpath`・`[native] rpath`。`embed` した `@rpath` の dylib を読ませるには、`--rpath @executable_path/../Frameworks` が要る（再現 6）。M15 はリンク行を変えない。
- [W09](_completed/W09-windows-gui-subsystem.md): ルートの節だけを読む規則と `with_manifest_windows` の形。

## 他チケットへ提供するインターフェース

- `crate::plist`（`Value`・`read`・`write`）。[M16](M16-macos-signing-notarization-hardening.md) が entitlements のファイルの検査に使う。
- `crate::app_bundle::build_app` と `SignPlan`。M16 が identity・entitlements・hardened runtime・timestamp を足し、同じ内側からの順で署名する。
- `MacosSettings` の bundle のキー。M16 は同じ節に `sign_identity`・`entitlements`・`hardened_runtime`・`timestamp` を足す。
- bundle の形（`Contents/Frameworks`・`Contents/Resources`）。[M12](M12-gpu-presentation-to-window.md) の Phase 1 が bundle の中の GPU ランタイムを探す場所、[M13](M13-desktop-os-services-package.md) が通知に使う `bundle_id` と資源の場所。
- `Env.executable_path`。M13 の資源の探索、bundle を使わないアプリの設定ファイルの探索。
- ランタイムに OS の操作を足す形（独立した入口、std だけが呼ぶ内部の組み込み、到達したときだけの `-D`。D13）。[M17](_completed/M17-macos-min-version-and-weak-import.md) の `Env.os_version`（`tsuzuri_os_version`・`Os.__os_version`・`-DTZ_OS_VERSION`）が同じ形を使う。

## 実装手順

1. **変更**: なし（調査）。**内容**: `node scripts/repo-kb.mjs brief M15`。基準を保存する（着手条件）。「現状」の再現 1〜8 を HEAD でやり直し、行番号を確かめる。**確認**: 記録。
2. **変更**: `src/plist.rs`、`src/lib.rs`、`tests/macos_bundle.rs`、`tests/fixtures/macos_bundle/`。**内容**: plist の書き手と読み手。fixture の `Info.plist`・`Extra.plist` は、macOS で `plutil -convert xml1` を通したものをコミットする。**確認**: `cargo test --locked --test macos_bundle`（`running N tests` を見る）。
3. **変更**: `src/package.rs`、`tests/packages.rs`、`src/diagnostic_kinds.rs`。**内容**: `[macos]` のキー（仕様 2）。M17 が未着手なら節と `invalid_macos_setting` も。**確認**: `manifest_macos_bundle_keys_parse`、順序の誤りの文のテスト（`tests/packages.rs:1484`）、T1。
4. **変更**: `src/main.rs`、`src/driver.rs`。**内容**: `Emit::App`、出力のパス、ホストと target の検査、`bundle_id` の検査。**確認**: 仕様 8 の `E2000` の各行（Rust の driver のテスト）、`--emit` の誤りの文のテスト。
5. **変更**: `src/app_bundle.rs`、`src/driver.rs`、`src/cache.rs`。**内容**: 組み立て・`.dSYM`・署名・入れ替え・持ち主の検査・鍵に入れる出力のパス。**確認**: macOS で `node tests/macos_bundle.mjs target/debug/tsuzuri --stage bundle`（E1〜E5）、T1。`--emit app` を使わないビルドの `--emit llvm` と診断が基準と一致する（S1）。
6. **変更**: `tests/macos_bundle.mjs`。**内容**: E6（キャッシュ）と E8（`open` の起動）。**確認**: macOS で全段が通り、残ったプロセスがない（`pgrep -f macos_bundle`）。
7. **変更**: `src/runtime/os.c`・`os-windows.h`・`os-wasi.c`、`src/check.rs`、`src/llvm_io.rs`、`src/llvm.rs`、`src/polymorph.rs`、`src/pinned_rules.rs`、`src/capabilities.rs`、`src/driver.rs`、`src/test_runner.rs`、`std/Env.tz`、`tests/os.mjs`、`tests/os-wasi-host.mjs`、`tests/capabilities.rs`、`tests/fixtures/capabilities/std-table.txt`、`tests/fixtures/effects_sweep/Main.tz`。**内容**: `Env.executable_path`（仕様 6）と、到達したときだけの `-DTZ_OS_EXECUTABLE_PATH`（D13）。**確認**: 着手条件で保存した `Env.current_dir` だけのプログラムの `--emit object` と実行ファイル（native の `-O0`・`-O3`）と `--wasm-host wasi` の `.wasm` が、基準とバイト単位で同じ。`Env.executable_path` を呼ぶプログラムの `tsuzuri build`・`run`・`test` が macOS・Linux でリンクでき、Windows は `ci:full` の `windows.yml` で確かめる。native の `-O0`・`-O3` で `node tests/os.mjs target/debug/tsuzuri` の「9. Env」の段に足した検査、`--wasm-host wasi` で `node tests/os-wasi-host.mjs` の新しい段、`cargo test --locked --test capabilities --test std_argument_order --test effects_alloc_conservative`、`sh scripts/check-runtime-includes.sh`（`src/runtime/` を変えたとき。AGENTS.md の T2）。
8. **変更**: 文書、`.github/workflows/targets.yml`（macOS の段に `node tests/macos_bundle.mjs target/release/tsuzuri`）。**確認**: `node scripts/gen-docs.mjs --check`、`node scripts/check-cli-docs.mjs`、変更したページの `node scripts/check-docs.mjs`、T2、`ci:full` を付けた PR の `targets.yml`。

## テスト計画

### Rust（`tests/macos_bundle.rs`。どのホストでも走る）

- `info_plist_matches_plutil_canonical_form`: 仕様 7 の設定から作った `Info.plist` が fixture（`plutil -convert xml1` を通したもの）とバイト単位で一致する。キーの順（大文字と小文字、`_`、非 ASCII、BMP の外の文字を含むキー）と、`&`・`<`・`>` の置き換えを含める。
- `plist_read_round_trips`: fixture の `Extra.plist`（`dict`・`array`・`string`・`integer`・`real`・`true`・`false`・`date`・`data`・コメント）を読んで書くと、同じバイト列に戻る。
- `plist_read_rejects`: binary plist、CDATA、未知の要素、最上位が辞書でないもの、重なったキー、深さ 129、1 MiB を超えるもの、不正な実体参照は `Err`。
- `info_plist_conflicts`: `Extra.plist` に `CFBundleIdentifier` があると `macos_bundle_conflict`。
- `mach_o_min_version_reads_build_version`: `LC_BUILD_VERSION` を持つ小さな Mach-O の見出し（テストの中で組む）から `12.0`・`13.1.2`・`27.2` を読む。fat と 32 bit は `None`。
- `icns_header_is_checked`: 正しい見出し、短いファイル、長さの不一致、別の magic。
- 決定的であること: 同じ入力から 2 回作って同じバイト列。

### Rust（`tests/packages.rs`・driver のテスト）

- `manifest_macos_bundle_keys_parse`: 仕様 2 の各キーの受理と、表の各誤り、知らないキー、重なり、名前の重なり、依存パッケージの `[macos]` を読まないこと。
- `emit_app_validation`: 仕様 8 の `E2000` の 4 行（Linux・Windows のホストではホストの誤り、macOS ではそれ以外）。
- `emit_exe_ignores_macos_bundle_keys`: `[macos]` に bundle のキーがあっても、`--emit exe` の鍵と出力が、キーのない manifest の場合と（manifest の文字列の違いを除いて）同じ。

### E2E（`tests/macos_bundle.mjs`。macOS だけ。`-O0`・`-O3`）

| 段 | 期待 |
| --- | --- |
| E1 仕様 7 の bundle | 形が仕様 3 のとおり。`plutil -lint` が通る。`codesign --verify --deep --strict` が通る。`codesign -dv` の Identifier が `bundle_id`、flags が `adhoc` で `linker-signed` を含まない。`PkgInfo` が `APPL????` |
| E2 `-g` | `Name.app.dSYM` があり、`.dwarf` がどこにもない。`dwarfdump --uuid` が一致する。`lldb --batch -o 'target create Probe.app' -o 'breakpoint set -n main'` の出力に `Main.tz` か `Env.tz` の行がある |
| E3 `resources`（ディレクトリと `com.apple.FinderInfo` を付けたファイル）と `icon` | 署名が通る。写しに `com.apple.FinderInfo` と `com.apple.quarantine` がない（`xattr -p <名前> <ファイル>` が失敗する）。OS が付ける `com.apple.provenance` は検査しない（再現 5） |
| E4 `embed` に署名のない dylib と `.framework`（テストが clang で作る） | 内側から署名され、`--verify --deep --strict` が通る。`notes.txt` を `embed` に書くと `macos_bundle_input_invalid` |
| E5 置き換え | 2 回目のビルドが前の bundle を置き換える。別の `bundle_id` の bundle や、`Info.plist` のないディレクトリは `app_output_not_owned` で、中身が残る。署名の失敗を `PATH` の偽の `codesign`（終了コード 1）で起こすと、前の bundle が残る |
| E6 キャッシュ（`TSUZURI_CACHE_DIR` を空の一時ディレクトリにする） | 同じビルドを 2 回するとキャッシュの項目は 1 個で、2 回目も bundle が作り直される（`Info.plist` の時刻が新しい）。`--emit exe` の後の `--emit app` も同じ項目に当たる。資源を 1 バイト変えると、項目は増えずに bundle の資源が変わる |
| E7（M20 の後）`embed` の `@rpath` の dylib と `--rpath @executable_path/../Frameworks` | `./Probe.app/Contents/MacOS/Probe` が dylib の関数の値を出す |
| E8 `open --stdout f --stderr g Probe.app`（`ui_element = true`） | 10 秒以内に `f` が `cwd=/` と bundle の中の `exe=` の 2 行になる。`perl -e 'alarm 15; exec @ARGV'` で包み、終わった後に残ったプロセスがない |
| E9 `[macos] min_version = "13.0"`（M17 の後） | `LSMinimumSystemVersion` が `13.0` |
| E10 仕様 8 の各誤り | コードと kind |

### E2E（`tests/os.mjs`・`tests/os-wasi-host.mjs` の `Env.executable_path`）

- native（Linux・macOS・Windows）の `-O0`・`-O3`: ビルドした実行ファイルを絶対パスとシンボリックリンク経由（Windows はなし）で起動し、返る値が実行ファイルの正規化したパスに等しい。
- `--wasm-host wasi`: `Other`（`code` 52）。既定の wasm32: `E2000`。
- `tsuzuri run` と、`Env.executable_path` を呼ぶ `test` を持つ `tsuzuri test`: 空でない絶対パスを返す（中身は一時ファイルなので検査しない）。`test` は `src/test_runner.rs` の経路の define を確かめる。
- define の門（D13）: `TSUZURI_CLANG` に引数を記録するラッパー（下の「変えないことの確認」と同じ形）を置き、`Env.executable_path` を呼ぶプログラムでは native と WASI のランタイムのコンパイルにだけ `-DTZ_OS_EXECUTABLE_PATH` があり、`Env.current_dir` だけのプログラムにはない。

### 変えないことの確認

- 基準の `--emit llvm`（native・wasm32）と診断が一致する。`--emit exe` の実行ファイルのリンク行が変わらない（`TSUZURI_CLANG` に引数を記録するラッパーを置く既存の形。`tests/lto.rs:1182`、`tests/codegen_units.mjs:82`）。`--emit app` の中の実行ファイルのリンク行も、同じ options の `--emit exe` と同じである。
- 着手条件で保存した `Env.current_dir` だけのプログラムの `--emit object`・実行ファイル（native の `-O0`・`-O3`）と `--wasm-host wasi` の `.wasm` が、基準と `cmp` で一致する（D13）。比べる fixture から、この PR が `Env.executable_path` を足す `tests/fixtures/effects_sweep` を除く。

## ドキュメント

- `organizing-tsuzuri/packages.md`: 27 行の節の一覧と「その他のセクション」に `[macos]` の bundle のキー（仕様 2 の表、省略したキーは書かないこと、ルートだけ）。M17 が先なら、その節に足す。
- `compiler/option.md`: 「出力とターゲット」に `--emit app`（形、実行ファイルの名前、署名は ad-hoc、キャッシュ）。「組み合わせがエラーになる場合」に仕様 8 の `E2000`。
- `compiler/platforms.md`: 「OS ごとの違い」の macOS の行と、配布の説明（`.app` を作れること、Developer ID は M16、`embed` には M20 の `--rpath` が要ること）。
- `compiler/debugging.md`: 20〜32 行の macOS の説明に、`--emit app -g` の `.dSYM` は lldb が自動で読むことを足す。
- `compiler/usage.md`: `build` の節に `--emit app`。
- `built-in-types-and-modules/env.md`: 冒頭の「3 つとも」、公開 API の表、例（`run=` では一時ファイルのパスなので、空でないことだけを出す）、「対応環境」。関数を足すモジュールのページには「引数の順序」の節を置く（AGENTS.md の Standard library API conventions。今の env.md にはない）。`Env` の関数はどれも引数が 1 つで（`tests/std_argument_order.rs:81` の `SingleArgument`）、`argument-order.md:111` の「引数が 1 つの関数」に当たると書く。この節は W11 の後も「Windows のネイティブビルドは OS API に到達すると `E2002`」と書いている（`src/target.rs:395-402` の `os_apis` は Windows を含む）。着手時に今の挙動を確かめ、`executable_path` の段を書くときに食い違いを直す。
- `compiler/diagnostics.md`（`node scripts/gen-docs.mjs`）。
- `README.md`: CLI の `--emit` の行（985 行）。`docs/language.md`: `Env` の段落（1443-1445 行）に `executable_path`。`docs/architecture.md`: ドライバーの出力の段に bundle の組み立て（キャッシュの後に毎回行うこと）。OS の組み込みの段（626 行。到達した `tsuzuri_os_<name>` だけを宣言する説明）に、`Os.__executable_path` と、到達したときだけの `-DTZ_OS_EXECUTABLE_PATH`。

## 受け入れ条件

- [ ] 仕様 1〜8 のとおりに動く（Rust と E2E）。
- [ ] Rust の書き手の `Info.plist` が `plutil -convert xml1` の出力とバイト単位で一致する。
- [ ] ad-hoc で署名した bundle が `codesign --verify --deep --strict` を通り、Identifier が `bundle_id` になる。
- [ ] `-g` で `.dSYM` ができ、UUID が一致し、lldb が自動で読む。平らな `.dwarf` が残らない。
- [ ] 失敗したビルドが、前の bundle を壊さず、中途半端な bundle を残さない。
- [ ] `--emit app` を使わないビルドの IR・リンク行・診断が基準と一致する。`Env.executable_path` に到達しないプログラムは、OS API を使っていても object・実行ファイル・`--wasm-host wasi` の `.wasm` が変わらない（D13）。
- [ ] `Env.executable_path` が native（3 つの OS）と WASI で仕様 6 のとおりに動く。
- [ ] `gen-docs --check`・`check-cli-docs`・変更したページの `check-docs`・T2 が通り、`ci:full` を付けた PR の `targets.yml` が通る。

## 落とし穴

- **署名は必ずやり直す**: リンカーの署名のまま `Info.plist` を足すと、`--verify --strict` が `code has no resources but signature indicates they must be present` で落ちる（再現 2）。
- **`Contents/` に何も足さない**: 持ち主の印・ログ・`.dwarf` を `Contents/` か `Contents/MacOS` に置くと、署名が `code object is not signed at all` で落ちる（再現 4・5）。持ち主は `Info.plist` で確かめる。
- **拡張属性**: `std::fs::copy` は macOS で `com.apple.FinderInfo` を写し、`codesign` が `detritus not allowed` で拒む。中身を読んで書く。`com.apple.quarantine` は害がないが、写さない方針で揃える。OS は新しいファイルに `com.apple.provenance` を付けることがある（再現 5）。署名を妨げないので、写し手もテストもこれを消そうとしない。
- **`Frameworks` はコード専用**: コードでないファイルは署名を壊す。`embed` で弾き、`resources` を勧める。
- **`--deep` で署名しない**: Apple は署名での `--deep` を勧めていない。内側から順に署名する。検証の `--verify --deep` は使ってよい。
- **dSYM は同じビルドの組**: `-g` の実行ファイルはビルドごとに UUID が変わる。後から `dsymutil` を走らせても一時の object がない（再現 4）。キャッシュは `dwarf` の項目を実行ファイルと一緒に持つ（`src/driver.rs:4561-4563`）ので、当たったときも組が保たれることをテストで確かめる。
- **`open -W`**: すぐ終わるアプリでは待てずに 1 行の誤りを出す。テストは出力のファイルを時間を切って待つ。CI の runner で LaunchServices が使えなければ S4。
- **App Translocation**: 隔離の属性の付いた bundle を Finder から開くと、OS が読み取り専用の別の場所へ移して起動する。`Env.executable_path` はその場所を返す。資源は実行ファイルからの相対で探し、bundle の外のファイルを当てにしない（文書に書く）。
- **`real` の正規化**: `plutil` は `<real>2.0</real>` を `2` にする。読み手は書かれた文字列を保つので、利用者の plist に `2.0` があると `plutil -convert xml1` とは一致しない。golden は生成するキーと、正規化の要らない値だけで比べる。
- **`LSMinimumSystemVersion` は実行ファイルから読む**: manifest の版を写すと、`MACOSX_DEPLOYMENT_TARGET` や既定の版と食い違いうる。M17 が無くても有効な版は `minos` に入っている。
- **`/proc/self/exe` の削除**: Linux で実行中に実行ファイルを消すと、`readlink` の結果に ` (deleted)` が付く。そのまま返す（文書に書く）。
- **define の門が守るのは object だけ**: `#if` の節を足すと `os.c` の文字列が変わるので、ランタイムのキャッシュの鍵（単位の文字列を含む）は OS API に到達するすべてのプログラムで変わり、1 回だけ作り直しになる。できる object は同じである。object が同じなのは、ランタイムを `-g` なしでコンパイルし、`os.c`・`os-windows.h`・`os-wasi.c` が行番号を値にしない（`grep -n 'assert\|__LINE__\|__FILE__'` が 0 件）からである。この 3 つのファイルに `assert` か `__LINE__` を足すと、節の後の行番号が object に入り、門が崩れる。
- **3 か所の define**: driver の native、driver の WASI、`src/test_runner.rs` のどれかで付け忘れると、その経路のリンクだけが `tsuzuri_os_executable_path` の未定義で失敗する。`build` だけを試すと `test` の経路の漏れに気づかない。

## 対象外

- Developer ID の署名・hardened runtime・entitlements・公証（[M16](M16-macos-signing-notarization-hardening.md)）。
- App Sandbox と Mac App Store の提出。
- `.png` から `.icns` への変換（`iconutil` には `.iconset` の形が要る。D11）。
- NIB・storyboard・asset catalog（`.xcassets`）のコンパイル。
- Universal 2 と Intel Mac（[M18](M18-intel-mac-and-universal2.md)。D-67 の分岐 3 で方針を改めないと決めたので、M18 は blocked）。
- rpath と `-sectcreate __TEXT __info_plist` のリンクの引数（M20。D12）。
- `tsuzuri new` のアプリのひな型。
- `--emit shared` の dylib を `.framework` にすること。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | `--emit app` を emit の種類にする（D-55 の登録）。macOS のホストと Apple の target だけで、ほかは `E2000` | bundle は出力の形の 1 つで、`-O`・`-g`・`--link` などの options をそのまま使える。サブコマンド（`tsuzuri bundle`）は build の options を二重に持つ。ほかのホストには `codesign` がない。署名し直さない bundle は手元では起動するが、署名の Identifier が `artifact` のままで `Info.plist` を束縛せず、`--verify --strict` を通らない（再現 2）。採らない案: 署名し直さない bundle をほかのホストで作る（Mac で署名をやり直す必要があり、手順が分かれる） | 承認済み（D-67 の委任で決定、2026-11-28） |
| D2 | `Info.plist` は Rust で書き、新しい crate を使わない。XML の plist で、`plutil -convert xml1` と同じバイト列にする。`info_plist` は Rust の部分集合の読み手で読み、binary plist は拒む | 決定的で、どのホストでも golden で検査できる。`plutil` の出力は独立した参照になる。採らない案: ビルドのたびに `plutil`・`PlistBuddy` を走らせる（OS の版で出力が変わりうる、テストが macOS に縛られる）、`plist` crate（GUIDE §1 規則 10） | 既定案（実装者はこの案に従う） |
| D3 | `[macos]` に `bundle_id`・`name`・`version`・`build`・`icon`・`resources`・`embed`・`info_plist`・`category`・`ui_element` を足す（D-55）。`--emit app` には `bundle_id` が要る。省略したキーは書かず、`[package]` から補わない | アプリの性質なので manifest に置く（W09・W10 と同じ）。識別子は署名・通知・設定の鍵で、暗黙の値にすると後で変えられない。`embed` は M20 の `[native] frameworks`（リンクするシステムの framework）と意味が違うので別の語にした。採らない案: `[package] name` から識別子を作る（逆 DNS にならない）、`version` を `[package]` から補う（GUIDE §1 規則 2） | 承認済み（D-67 の委任で決定、2026-11-28） |
| D4 | `Env.executable_path :: own unit -> IO<Result<string, Os.Error>>` を足す（D-55）。macOS は `realpath` で正規化し、Windows は正規化しない。WASI は `Other`（ENOSYS） | 対象のない関数なので D-59 の例外は要らない（`Env` は `SingleArgument`）。名前は `current_dir` と同じく何を返すかを書く。Finder から起動すると作業ディレクトリが `/` なので、資源の探索に要る。採らない案: `Env.current_exe`（Rust の名前。`current_dir` と並べると「今の」の意味が紛らわしい）、std に bundle の API（GUI の同梱をしない非目標。M13 の公式パッケージの範囲） | 承認済み（D-67 の委任で決定、2026-11-28） |
| D5 | 中の実行ファイルは `--emit exe` と同じ options と鍵でキャッシュし、bundle は毎回組み立て直す。資源の中身は鍵に入れない。`-g` の鍵には最終のパスを入れる | `--emit exe` とキャッシュを共有でき、実行ファイルのリンク行と鍵を変えない（S1）。組み立ての費用は写しと ad-hoc の署名だけ。採らない案: bundle 全体をキャッシュする（資源の中身を鍵に入れることになる。E12 D7 は、入力ファイルの中身を鍵に入れることを費用に見合わないとして退けた） | 既定案（実装者はこの案に従う） |
| D6 | 資源と `embed` は名前を保って写す。名前の重なりは `E0002`。`embed` は Mach-O と `.framework` だけ。写しは中身と実行の権限だけで、拡張属性を写さない | 署名が受ける形だけを作る（再現 5）。採らない案: 名前を変えて写す（`NSBundle` で探す名前が利用者に分からない）、`embed` に任意のファイルを許す（`Frameworks` のコードでないファイルは署名を壊す） | 既定案（実装者はこの案に従う） |
| D7 | 署名は ad-hoc で、内側から順に行い、`--deep` を使わない。hardened runtime は付けない | arm64 で起動し、`--verify --deep --strict` を通る最小の署名。hardened runtime は ad-hoc の dylib と組むと dyld が拒む（M16 の「現状」）ので、M16 の明示のキーにする。採らない案: `codesign --deep` で 1 回で署名する（Apple は署名での `--deep` を勧めず、内側の順と引数を制御できない）、hardened runtime を既定にする（`embed` した ad-hoc の dylib を読めなくなる） | 既定案（実装者はこの案に従う） |
| D8 | 既存の bundle は、`Info.plist` の `CFBundleIdentifier` と `CFBundleExecutable` が一致するときだけ置き換え、一時ディレクトリから入れ替える | 利用者のディレクトリを消さない（`tsuzuri doc` と同じ考え）。印のファイルは署名を壊すので使えない（再現 5）。採らない案: 常に上書き（`-o` を誤ると別のアプリを消す）、印を `Contents/Resources` に置く（配布物に余計なファイルが入る） | 既定案（実装者はこの案に従う） |
| D9 | `-g` は `Name.app.dSYM` を作り、平らな `.dwarf` を残さない | lldb と Xcode と Spotlight の UUID の探索が、bundle の横の `.dSYM` を読む（再現 4）。`.dwarf` を bundle の中に置くと署名が壊れる。採らない案: 平らな `Name.app.dwarf` を bundle の横に置く（lldb が自動で読まず、`target symbols add` が要る） | 既定案（実装者はこの案に従う） |
| D10 | `LSMinimumSystemVersion` は実行ファイルの `LC_BUILD_VERSION` の `minos` から作る | M17 の有効な版も `MACOSX_DEPLOYMENT_TARGET` も既定の版も、すべて `minos` に入る。M17 の有無によらず実行ファイルと食い違わない。採らない案: manifest の `min_version` を写す（`MACOSX_DEPLOYMENT_TARGET` と既定の版のときに値がなく、実行ファイルと食い違いうる） | 既定案（実装者はこの案に従う） |
| D11 | `icon` は `.icns` だけを受ける | 変換は `iconutil` と `.iconset` の形に頼り、道具なしの経路にならない。`.icns` は `iconutil -c icns` で作れる（文書に書く）。採らない案: `.png` を受けてビルドのたびに `iconutil`・`sips` で変換する（道具に頼り、決定的な出力を確かめにくい） | 既定案（実装者はこの案に従う） |
| D12 | M15 はリンク行に rpath を足さない。`embed` の `@rpath` の dylib には M20 の `--rpath` を使う | リンク行を変えない（S1）。リンクの後には rpath を足せない（再現 6）。M20 の前は、`Env.executable_path` から作ったパスで読む形だけが使える（文書に書く）。採らない案: `--emit app` のときだけリンク行に `-rpath @executable_path/../Frameworks` を足す（`--emit exe` とキャッシュの項目を共有できず、実行ファイルのバイト列も変わる） | 既定案（実装者はこの案に従う） |
| D13 | `Env.executable_path` のランタイムは、独立した入口 `tsuzuri_os_executable_path` と内部の組み込み `Os.__executable_path` にし、到達したプログラムでだけ `-DTZ_OS_EXECUTABLE_PATH` でコンパイルする（driver の native と WASI、`src/test_runner.rs`）。退けた案: `Os.__read` の操作 6（新しい組み込みは要らないが、OS API に到達するすべてのプログラムのランタイムの object（`--emit object` の出力と実行ファイル）が変わり、D-67 の「新しい機能を使わないプログラムは 1 byte も変えない」に例外を足すことになる）、ランタイムを使わず std だけで書く（実行ファイルのパスを得る OS の API は C からしか呼べない） | 前例（`-DTZ_TRAP_BOUNDARY`・`-DTZ_STACK_GUARD`・`-DTZ_ASYNC_PREEMPT` は `src/driver.rs:4915-4923`、WASI の `-DTZ_WASI_START` は 5089-5094 行）と同じ門で、ランタイムのキャッシュの鍵は引数を含む（`src/driver.rs:6851`）。prebuilt のランタイムは `os.c` を含む単位を持たないので、照合に影響しない。守るのは object で、`os.c` の文字列が変わる分のランタイムのキャッシュの作り直しは 1 回だけ起きる（落とし穴）。費用は、組み込みの名前 1 つと、define を付ける分岐 3 つである | 既定案（実装者はこの案に従う。D4 の承認の後） |

## 実装と検証（2026-11-28）

実装手順 1〜8 をすべて実装した。D1・D3・D4 と D-55 の登録は、利用者の委任（GUIDE D-67 の「委任による決定（2026-11-28、M15）」）で、チケットの案のとおりに決めた。macOS 27.2（arm64）、Apple clang、Apple ld、`target/debug/tsuzuri`（dev profile）で確かめた。

### 実装したこと（手順ごと）

1. 調査: 基準（`tests/fixtures/*/Main.tz` と `examples/*/Main.tz` の `--emit llvm -O1` の native・wasm32 と `check --json`、`Env.current_dir` だけのプログラムの object・実行ファイル・`--wasm-host wasi` の `.wasm`）を main（`faa091b2`）のバイナリで保存した。行番号と数は HEAD の値から数え直した（`[macos]` の節・`MacosSettings`・`invalid_macos_setting`・`Env.os_version` の門は M17 が先に入れていたので、それに足した）。
2. `src/plist.rs`: `Value`・`Key`（UTF-16 の符号単位の順）・`write`・`read`・`read_dictionary`。`tests/fixtures/macos_bundle/` の `Info.plist`・`Extra.plist`・`Dsym.plist` は、手で書いた `*.source.plist` を `plutil -convert xml1` に通したもの。
3. `src/package.rs`: `MacosSettings` に 10 個のフィールドと、仕様 2 の検査・誤りの文。`Line::string_array_of`（kind を選べる配列の読み手）。
4. `src/main.rs`・`src/driver.rs`: `Emit::App`、`output_path`、`BuildOptions::validate_app`（ホストと target）・`bundle_executable`、`app_output_error`・`app_executable_name`、`links_apply`、`--help`。
5. `src/app_bundle.rs`: `build_app`（入力の検査 → 持ち主の検査 → 一時ディレクトリでの実行ファイルのビルド → `LSMinimumSystemVersion` → `Info.plist`・`PkgInfo`・資源・`embed` → `.dSYM` → 署名 → 入れ替え）、`SignPlan`・`sign`（M16 の継ぎ目）、`info_plist`・`dsym_info_plist`・`mach_o_min_version`・`valid_icns`・`mach_o_magic`。`driver::build_complete_keyed` が「鍵に入れる出力のパス」を受ける（ほかの呼び出しは `output` をそのまま渡す）。kind は 4 つ（`macos_bundle_id_missing`・`macos_bundle_conflict`・`macos_bundle_input_invalid`・`app_output_not_owned`）。
6. `tests/macos_bundle.mjs`: E1〜E10（`--stage bundle|cache|open`）。
7. `Env.executable_path`: `Builtin::OsExecutablePath`（`Os.__executable_path`）、`tsuzuri_os_executable_path`（`os.c` の macOS と Linux、`os-windows.h`、`os-wasi.c`。どれも `TZ_OS_EXECUTABLE_PATH` の中）、driver の native・WASI と `src/test_runner.rs` の 2 か所の `-DTZ_OS_EXECUTABLE_PATH`、`std/Env.tz` の `executable_file`・`executable_path`。
8. 文書（下）と `.github/workflows/targets.yml` の macOS の段。

### チケットから外した判断

- **実行ファイルは staging の bundle の外で作る**: 設計は一時の `…/Contents/MacOS/Name` へ直接リンクする形だったが、`-g` の `.dwarf` と `--trap-info` の `.trap.json` は出力の横に出るので、`Contents/MacOS` に入ると署名が壊れる（再現 4 と同じ）。`<一時>/build/program` に作ってから `Contents/MacOS/Name` へ移す。鍵に入れるパスは設計どおり最終の `<出力>/Contents/MacOS/Name`。
- **`--trap-info` の表は `Name.app.trap.json`**: チケットに記述がなかった。bundle の中には置けないので、`--emit exe` の `<出力>.trap.json` と同じ規則で bundle の横に置き、bundle・dSYM と同じ入れ替え（退避と復元）に含めた。
- **`data` の値は bytes で持つ**: `Value::Data(Vec<u8>)`。base64 を検査して復号し、書くときは Core Foundation と同じ折り返し（1 行は `76 - 8 × 字下げ` 文字）で書く。値は変わらず、`plutil` の出力とバイト単位で一致する（golden に 3 段の字下げ・空・1 行ちょうどの例がある）。`integer`・`real`・`date` は仕様どおり、形を検査して書かれたまま写す（前後の空白だけ除く）。`integer` は 10 進と `-` だけ（`+5`・`0x10` は拒む）。
- **Apple Computer の DOCTYPE も読み飛ばす**: `dsymutil` や古い道具が書く `-//Apple Computer//DTD PLIST 1.0//EN` を、Apple の DOCTYPE として受ける（内部サブセット `[` を持つものと、ほかの DOCTYPE は誤り）。UTF-8 の BOM も読み飛ばす。
- **生成するキーとの重なりの文を 3 通りにした**: `[macos]` で設定済みなら仕様の文（`comes from both [macos] bundle_id and 'Extra.plist'; remove one`）、未設定なら `belongs to [macos] name; set it there and remove it from …`、`[macos]` のキーのない 5 つは `is written by '--emit app'; remove it from …`。
- **名前の重なりは ASCII の大文字小文字を無視して比べる**: 既定の macOS のボリュームは `help` と `HELP` を同じファイルにするため。
- **`embed`・`resources` の最上位のパスがシンボリックリンクなら、その先を写す**: 仕様は木の中のリンクだけを定めていた。木の深さは 64 段まで（超えると `E2001`）。
- **bundle が `[macos]` の入力を含むときも `output_overwrites_input`**: ソース・manifest と同じ扱いにした（入れ替えで入力が消えるため）。
- **`LC_VERSION_MIN_MACOSX` も読む**: 古いリンカーの出力のため。どちらもなければ `E2002`（`toolchain_missing_feature`）。
- **`tests/os-wasi-host.mjs` に段は足していない**: これは `tests/os.mjs` が呼ぶ WASI のホストで、WASI の検査は `os.mjs` の中にある。Linux の CI で `os.mjs` は走らないので、どのホストの CI でも走る `tests/e2e.mjs` に `Env.executable_path` の 1 段を足した。
- **`emit_exe_ignores_macos_bundle_keys` は object と節の中身で比べる**: 同じプログラムの 2 回のリンクは `LC_UUID` と署名が違う（main のバイナリでも同じ）ので、実行ファイルのバイト列は比べられない。`--emit object` はバイト単位、実行ファイルは `otool -t -d` で比べる。

### 見つけた制限（報告）

- **プロジェクトの中の `.framework`**: モジュール探索（`collect_sources`）はプロジェクトのディレクトリをすべてたどり、ディレクトリへのシンボリックリンクを `E1011` で拒む。`Versions/Current` を持つ `.framework` をプロジェクトの中に置くと、`embed` より前に `check` が落ちる。それを埋め込んだ bundle を既定の出力先（プロジェクトの中の `Main.app`）に作ると、次のビルドが同じ理由で落ちる。M15 は探索の規則を変えず（`--emit app` を使わないビルドの診断を変えないため）、文書に「プロジェクトの外に置き、`embed = ["../vendor/Foo.framework"]` と `-o` で外に出す」と書いた。E4 の fixture もプロジェクトの横に置いた。探索が `*.app`・`*.framework`・`*.dSYM` を飛ばす規則は、別のチケットで決めるのがよい。資源だけの bundle は、既定の出力先でも繰り返しビルドできる（E2E にある）。
- **基準との一致は関数の番号を除いて**: std に関数を足すと、後ろの関数と intrinsic の通し番号（`@tz.fn.$intrinsic.Display.display.882` の `882`）がずれる（M17 の `Env.os_version` と同じ）。`std/Env.tz` の 2 つの関数で 2 ずれるので、基準の 829 個の出力のうち 630 個はバイト単位で一致し、199 個（`.ll`）は番号だけが違い、ほかの違いは 0 個だった。診断は全部一致した。`Env.current_dir` だけのプログラムは、`-O3` の object と `--wasm-host wasi` の `.wasm`（`-O0`・`-O3`）がバイト単位で一致し、`-O0` の object はコードとデータの節が一致して、局所シンボルの番号だけが違う。ランタイムに `executable_path` のシンボルはない。

### 実行したコマンドと結果

- `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`: 通過。
- `cargo test --locked --test macos_bundle`: 12 件。`--test packages`: 24 件（`manifest_macos_bundle_keys_parse` を含む）。`--test explain`: 17 件。`--test capabilities`・`std_argument_order`・`effects_alloc_conservative`・`std_examples`・`macos_min_version`: 通過。
- `node tests/macos_bundle.mjs target/debug/tsuzuri`（`bundle`・`cache`・`open` の全段。`-O0`・`-O3`）: 通過。`open` の段は LaunchServices から起動し、`cwd=/` と bundle の中の `exe=` を確かめ、残ったプロセスがない。
- `node tests/os.mjs target/debug/tsuzuri`: 通過（native の絶対パス・シンボリックリンク経由・相対パスでの起動、WASI の `other (os error 52)`、`tsuzuri run`・`tsuzuri test`、既定の wasm32 の `E2000`、`TSUZURI_CLANG` のラッパーで `-DTZ_OS_EXECUTABLE_PATH` が到達したプログラムの 1 回のコンパイルにだけ付くこと）。
- `node tests/e2e.mjs target/debug/tsuzuri`、`cargo nextest run --locked`（T1）: 下の「T1 と e2e」。
- `node scripts/gen-docs.mjs --check`（11 ブロック、stale 0）、`node scripts/check-cli-docs.mjs`（4 検査、失敗 0）、`sh scripts/check-runtime-includes.sh`（64 ファイル）、`node scripts/check-guide-lite.mjs`: 通過。
- 同梱ツールチェーン（`tsuzuri-clang`）で、`Env.executable_path` を呼ぶプログラムを `x86_64`・`aarch64` の `linux-gnu`・`linux-musl`・`windows-gnu` の 6 target に `-O0`・`-O3` でクロスリンク: 12 個とも成功（実行はしていない）。rustup の stable で `cargo check --all-targets --target x86_64-pc-windows-msvc`: 警告なし。

### この機械で確かめていないこと

- `Env.executable_path` の Linux（`/proc/self/exe`）と Windows（`GetModuleFileNameW`）の実行。コンパイルとリンクまで確かめた。実行は CI の `tests/e2e.mjs`（Linux・macOS）と `windows.yml` の `tests/os.mjs`（Windows の「9. Env」に足した検査）が受け持つ。
- macOS 以外のホストでの `'--emit app' needs a macOS host` の誤り（`validate_app(false)` の単体テストはある）。
- 同梱ツールチェーンでの `tests/macos_bundle.mjs`、CI の runner での `open`（S4。動かなければ `TSUZURI_MACOS_BUNDLE_NO_OPEN=1` でその段だけを飛ばせる）。

### 更新した文書

- このリポジトリ: `docs/language.md`（`Env.executable_path`、WASI、新しい節「macOS のアプリケーションバンドル（`--emit app`）」、生成した kind の表）、`docs/architecture.md`（OS の組み込みの段、ドライバーの出力の段）、`README.md`（機能一覧、`--emit` の行、DWARF、変更の種類別の表、検証コマンド）、`src/explain/codes/E2003.md`、`_features/GUIDE.md`（D-55 の表と D-67 の委任の記録）。
- Tsuzuri-Reference: `docs/compiler/macos-app-bundle.md`（新規）、`docs/stdlib/sys/env.md`、`docs/modules/packages.md`、`docs/compiler/directives.md`、`docs/compiler/options.md`、`docs/compiler/platforms.md`、`docs/compiler/debugging.md`、`docs/compiler/cli.md`、`docs/index.md`。`check-docs.mjs` はこの 9 ページで通過した。`gen-docs.mjs` の生成ブロック（`diagnostics.md` の kind の表）は、3 つのチケットを統合したバイナリで作り直す。

### 変更したファイル

`src/app_bundle.rs`（新規）、`src/plist.rs`（新規）、`src/lib.rs`、`src/main.rs`、`src/driver.rs`、`src/package.rs`、`src/diagnostic_kinds.rs`、`src/check.rs`、`src/llvm.rs`、`src/llvm_io.rs`、`src/polymorph.rs`、`src/pinned_rules.rs`、`src/pure_rules.rs`、`src/effects.rs`、`src/kg/effects.rs`、`src/capabilities.rs`、`src/test_runner.rs`、`src/runtime/os.c`、`src/runtime/os-windows.h`、`src/runtime/os-wasi.c`、`src/explain/codes/E2003.md`、`std/Env.tz`、`tests/macos_bundle.rs`（新規）、`tests/macos_bundle.mjs`（新規）、`tests/fixtures/macos_bundle/`（新規）、`tests/packages.rs`、`tests/capabilities.rs`、`tests/std_examples.rs`、`tests/fixtures/capabilities/std-table.txt`、`tests/fixtures/effects_sweep/Main.tz`、`tests/os.mjs`、`tests/e2e.mjs`、`tests/bindings.mjs`、`.github/workflows/targets.yml`、`README.md`、`docs/language.md`、`docs/architecture.md`、`_features/GUIDE.md`。`src/cache.rs` は変えていない（鍵に入れるパスは呼び出し側で渡す）。

共有の数: `Builtin::ALL` は 384 から 385、効果を持つ組み込みは 173 から 174、ホストに届く私的な組み込みは 32 から 33、`tests/capabilities.rs` の `Env.tz` は 4 から 5・合計 45 から 46・std-table 89 から 90、`tests/std_examples.rs` の `Env` の床は 4 から 5、診断の kind は 4 つ増えた。`RESERVED_MODULES`・`FRONTEND_FORMAT`・`Os.__read` の操作は変えていない。
