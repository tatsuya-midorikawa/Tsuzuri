# M20: macOS の framework のリンク入力（`--framework`・`-F`・`--rpath`・`[native] frameworks`・`@link(framework = ...)`）

| 項目 | 内容 |
| --- | --- |
| ID | M20 |
| 優先度 | P1（M07 の公式パッケージと M14 は framework を名前でリンクする口を要る。今は SDK の `.tbd` のパスを `--link` に渡す、文書にない回避策しかない。同梱の zig ではそれもできない） |
| 規模 | M（Phase 1・2。Phase 2 は W17 の出荷の後の 2 つ目の PR） |
| 依存 | ([W17](W17-link-attribute-dllimport.md)), ([M22](M22-attribute-after-bodyless-extern-def.md))（Phase 2 の `@link(framework = …)` は W17 の後） |
| 後続 | [M07](M07-apple-sdk-bindgen-and-package.md), [M14](M14-tls-http-packages.md)（弱い後続: [M01](M01-objc-class-types.md), [M15](M15-macos-app-bundle.md)） |
| 状態 | todo |
| 起票 | 2026-11-26（D-67。利用者の依頼「「どのチケットにもない壁」と「調査中に見つけたコンパイラの不具合（自分で再現済み）」のそれぞれについて @_features/ に開発チケットまたは修正チケットを作成して」から起票。調査で報告した「M07・M15 の前提（-framework・-F・rpath を渡す口がない。W17:247 は対象外で、NC12:70 は存在しない -framework を前提にしていて食い違う。同梱の zig は SDK がないとリンクできない）」に当たる。HEAD `804b7970` で確認） |
| 承認 | 要承認: [GUIDE D-67](GUIDE.md#d-67-tsuzuri-だけで-macos-のデスクトップアプリを書くm01m22) の登録の一覧のうち D-55（CLI の `--framework`・`-F`・`--rpath`、`[native] frameworks`・`framework_search`・`rpath`、Phase 2 の `@link(framework = "...")`）と、W17 の対象外（W17:247-248）の改訂。決定事項 D1・D3・D4・D5・D6 が要承認、ほかは既定案。承認が記録されるまで、該当する Phase に着手しない |
| 改善する劣位 | C・Swift（clang の `-framework`・`-F`・`-Wl,-rpath`）・Rust（`#[link(name = "AppKit", kind = "framework")]`、`cargo:rustc-link-lib=framework=AppKit`）比: macOS の framework を名前でリンクできない。SDK の `.tbd` を `--link` に絶対パスで渡す回避策（文書にない）に頼り、manifest では SDK へのシンボリックリンクが要る。git・registry のパッケージは framework を要求できない。同梱の zig ではどれもリンクに失敗する |
| 手本にする既存実装 | `src/driver.rs` の `LinkInputs`（`check_shape`・`check_readable`・`add_to`・`followed_by`）と `library_name_error`、`src/package.rs` の `parse_manifest` の `[native]`、`src/main.rs` の `-l`・`-L` の解析と `links_apply`、`src/driver.rs` の `BuildOptions::validate` の `--subsystem`（ほかの OS の target を `OptionUnsupportedPlatform` にする形）と `bundled_launcher`、[W17](W17-link-attribute-dllimport.md)（Phase 2 の `@link` の構文・検査・`system_libraries`・`tsuzuri audit` の `link`・パッケージの規則と D2） |
| 主な影響ファイル | `src/driver.rs`, `src/main.rs`, `src/package.rs`, `src/test_runner.rs`, `src/diagnostic_kinds.rs`, `src/syntax.rs`, `src/parser.rs`, `src/syntax_codec.rs`, `src/frontend_cache.rs`, `src/formatter.rs`, `src/check.rs`, `src/llvm.rs`, `src/capabilities.rs`, `tests/macos_link_inputs.mjs`（新規）, `tests/packages.rs`, `tests/targets.rs`, `tests/capabilities.rs`, `tests/link_attribute.rs`（W17 が作る。Phase 2）, `.github/workflows/targets.yml`, `docs/language.md`, `README.md`, `_tsuzuri/language-reference/compiler/option.md`, `_tsuzuri/language-reference/compiler/native-interop.md`, `_tsuzuri/language-reference/organizing-tsuzuri/packages.md`, `_tsuzuri/language-reference/compiler/platforms.md`, `_tsuzuri/language-reference/values-and-functions/attributes.md`, `_features/NC12-ffi-numeric-buffers-and-blas-lapack.md`, `_features/W17-link-attribute-dllimport.md` |

## 実装者向け早見

- **人間が求めない限り Phase 1 だけを実装する**（GUIDE §0）。Phase 2 は W17 の出荷の後の 2 つ目の PR である。
- 範囲:
  - Phase 1: CLI の `--framework NAME`・`-F DIR`・`--rpath PATH`（どれも繰り返せる）と、manifest の `[native] frameworks`・`framework_search`・`rpath`。Tsuzuri がリンクするビルド（`build --emit exe`・`shared`、`run`、native の `test`・`bench`）のリンク行に `-F<dir>`・`-framework <name>`・`-Wl,-rpath,<path>` を足す。macOS の target だけ。同梱の zig では、framework を要るビルドに SDK の `-F`・`-L` を足す（D3）。
  - Phase 2（W17 の後）: W17 の `@link` に `@link(framework = "AppKit")` の形を足す。その宣言の symbol に到達したビルドだけ `-framework` を足す。git・registry のパッケージにも書ける。
- 最初に読む: 「現状」の再現 → 仕様 1〜4 → `src/driver.rs:43-165`（`LinkInputs`）→ `src/package.rs:896-938`（`[native]`）→ `src/main.rs:1281-1308`・`1904-1914`。
- 最初に書くテスト: `src/driver.rs` の単体テスト `framework_names_follow_the_library_rule`・`rpaths_are_anchored_and_comma_free`・`frameworks_follow_libraries_on_the_link_line` → `src/package.rs` の `native_frameworks_parse` → `tests/macos_link_inputs.mjs`。
- 触ってはいけない: `--link`・`-l`・`-L`・`[native] link`・`libraries`・`search` の規則と順序、重複と 256 の上限の誤り、git・registry のパッケージと publish が `[native]` を拒むこと、リンク入力のあるビルドでキャッシュを使わない規則（E12 D7）。新しい入力のないビルドのリンク行・IR・object・キャッシュの鍵・診断は 1 byte も変えない（例外は仕様 6 の 2 文）。
- 完了の見分け方: 「現状」の `fw/Main.tz` を `tsuzuri build fw --framework AppKit -o app` で作ると、Apple の clang と同梱の toolchain の両方でリンクでき、`./app` が 0 を返す。`otool -L app` に `/System/Library/Frameworks/AppKit.framework/Versions/C/AppKit` が出る（今は `unknown option '--framework'` の `E2000`）。

## 目的

macOS のアプリは AppKit・Foundation・Metal などの framework をリンクする。clang は `-framework AppKit` で SDK の framework を探し、`-F DIR` で探す場所を足す。Tsuzuri にはこのどちらもない。今は SDK の `AppKit.tbd` を `--link` に絶対パスで渡すか、manifest に SDK へのシンボリックリンクを置くしかない。どちらも文書にない使い方で、同梱の zig ではリンクに失敗する。

`.app` に dylib を同梱するアプリ（M15 の `embed`）と、`--emit shared` の dylib（install name が `@rpath/<file>`）を使う実行ファイルは、`LC_RPATH` を要る。これを足す口もない。

[W17](W17-link-attribute-dllimport.md) の `@link("name")` は、git・registry のパッケージにもシステムライブラリを要求させるが、W17:247 は framework を対象外にしている。M07 の公式の `apple` パッケージと M14 は git のパッケージなので（D-67 の分岐 5。2026-11-26 に承認済み）、framework を名前で要求する形が要る。NC12:70 は外部 BLAS の選び方に「`-framework Accelerate` 等のビルドフラグ」を挙げるが、その口は存在しない。このチケットがその口を作る。[GUIDE D-67](GUIDE.md#d-67-tsuzuri-だけで-macos-のデスクトップアプリを書くm01m22) の、分岐 1 に依らない土台の 1 つである。

## 着手条件と停止条件

### 着手条件

- **人間が求めない限り Phase 1 だけを実装する。** Phase 2 は、人間が求め、W17 が done のときだけ着手する。
- 承認が記録されるまで、該当する Phase に着手しない。Phase 1 は D1・D3・D4・D5 の承認を要る。Phase 2 は D6 の承認と W17 の D1・D2 の承認を要る。
- `node scripts/repo-kb.mjs brief M20` を読む。P1 だが独立レビューを受けていない起票なので、着手時の `main` で「現状」を確かめ、食い違えばこのチケットを直してから始める（GUIDE §0 の 2）。
- 「現状」の再現を着手時の HEAD で実行する。行番号は `grep -n "pub fn library_name_error\|fn add_to\|links.add_to\|fn links_apply\|fn bundled_launcher" src/driver.rs src/main.rs src/test_runner.rs` と `grep -n '"link" | "libraries" | "search"' src/package.rs` で確かめ直す。
- W17 が先に入っていれば、`add_to` の引数（`system_libraries`）と `system_libraries` の求め方を読み、設計の `LinkExtras` の項に合わせる。
- 基準の保存: `tests/fixtures/*/Main.tz` と `examples/*/Main.tz` の `--emit llvm`（native と `--target x86_64-unknown-linux-gnu`）と、`--link`・`-l`・`-L` を使う既存の E2E（`tests/ffi_extensions.mjs`）のリンク行（偽の clang で記録する）を `target/scratch/m20/base/` に保存する。

### 停止条件

- **S1** Apple の clang か同梱の toolchain のどちらかで、`-framework`・`-F`・`-Wl,-rpath` を足した実行ファイルがリンクできないか、実行の結果が変わる。
- **S2** 新しい入力のないビルドのリンク行・キャッシュの鍵・IR・診断（仕様 6 の 2 文を除く）が、保存した基準と 1 byte でも違う。
- **S3** 同梱の zig で、SDK の `-F`・`-L` を足しても framework がリンクできない SDK の版がある（D3 の前提が崩れる）。SDK の版と zig のエラーを記録して報告する。
- **S4** Phase 2 で、W17 の `@link` の構文・表・リンク行が設計（`LinkExtras` と「Phase 2: 構文・検査・リンク行」）と違う形で出荷されていて、framework の形を足すのに W17 の出荷済みの出力（`tsuzuri audit` の文字・JSON、リンク行の順）を変える必要がある。
- **S5** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `804b7970` で確認）

macOS 27.2.1 arm64、Apple clang 21.0.0（clang-2100.4.1.1）、Apple ld（ld-27106.1）、SDK 27.2（`/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk`）、dev の `target/debug/tsuzuri` で確かめた。

### 再現

`fw/Main.tz`（AppKit がロードされていれば `NSApplication` のクラスが見つかる）:

```tsuzuri
@trusted
@count(1, none)
extern "objc_getClass" def objc_get_class :: ref utf8string -> i64

def main :: unit -> i32 = \() ->
    let cls = objc_get_class (ref u8"NSApplication\0")
    if cls != 0 then 0 else 1
```

`SDK=$(xcrun --show-sdk-path)` の下で:

| コマンド | 結果 |
| --- | --- |
| `tsuzuri build fw -o a` | `E2002`: `Undefined symbols for architecture arm64: "_objc_getClass"`（libobjc もリンクされない） |
| `tsuzuri build fw -l objc -o a && ./a` | ビルドは成功し、終了コード 1（AppKit がロードされない） |
| `tsuzuri build fw --link $SDK/System/Library/Frameworks/AppKit.framework/AppKit.tbd -o a && ./a` | 終了コード 0。`otool -L` に `/System/Library/Frameworks/AppKit.framework/Versions/C/AppKit` と `/usr/lib/libobjc.A.dylib` が出る（`.tbd` を `--link` に渡すのは文書にない使い方） |
| `tsuzuri build fw --framework AppKit -o a` | `<command line>:1:1: error[E2000]: unknown option '--framework'; use --help`（終了コード 2） |
| `tsuzuri build fw -F $SDK/System/Library/Frameworks -o a` | `<command line>:1:1: error[E2000]: unknown option '-F'; use --help` |
| `tsuzuri build fw -l AppKit.framework -o a` | 名前の検査を通り、リンクで `ld: library 'AppKit.framework' not found` の `E2002`。末尾に W03 が直す誤った助言 `install LLVM/Clang 17+ or set TSUZURI_CLANG to its executable` が付く |
| `tsuzuri build fw -l AppKit -l objc -o a` | `ld: library 'AppKit' not found` の `E2002` |

manifest に `frameworks` を書くと拒まれる（`fw/Main.tz` と同じソースのパッケージ）:

```toml
[package]
name = "fwprobe"
version = "0.1.0"

[native]
frameworks = ["AppKit"]
```

```text
.../proj/Tsuzuri.toml:1:1: error[E0002]: expected [native] keys link, libraries or search, each an array of strings on one line
```

`--json` では kind `invalid_native_section`、span の `start` は `frameworks` の行の byte 位置（55）で、`line`・`column` は 1・1 である（manifest の位置の表示はこのチケットの範囲外）。

### 同梱の toolchain（zig 0.16.0）

`target/toolchain`（2026-10-11 にビルドした同梱の toolchain。作り直すには `node scripts/toolchain/bundle.mjs target/toolchain`。リンクの引数を `zig cc` にそのまま渡す部分は HEAD の `scripts/toolchain/clang.rs` と同じ）の `bin/tsuzuri-clang` を `TSUZURI_CLANG` に指定した:

- `-l objc`: `error: unable to find dynamic system library 'objc' using strategy 'paths_first'. searched paths: none`。
- `--link $SDK/.../AppKit.tbd`: `error: unable to resolve dependency` と `note: while resolving /System/Library/Frameworks/ApplicationServices.framework/Versions/A/ApplicationServices`。
- `zig cc -target aarch64-macos.12.0 z.c -framework AppKit`: `error: unable to find framework 'AppKit'. searched paths:  none`。`-F$SDK/System/Library/Frameworks -L$SDK/usr/lib` を足すとリンクでき、終了コード 0。
- `TSUZURI_ZIG` に、`zig cc` ごとに同じ 2 つを足すラッパーを指定すると、`build fw --link .../AppKit.tbd -l objc` がリンクでき、終了コード 0。
- `-Wl,-rpath,@executable_path/../Frameworks` は zig でも Apple の ld でも `LC_RPATH` を出す。存在しない `-F` のディレクトリを、zig は黙って無視し、Apple の ld は `ld: warning: search path '/nonexistent/dir' not found` を出してリンクを続ける（Tsuzuri は仕様 6 のとおり、リンクの前に拒む）。
- `TSUZURI_SYSROOT` は launcher が `-isysroot` を拒み `E2002`（`scripts/toolchain/clang.rs:141-150`、platforms.md:98）。SDK を渡す口は今はない。

### 自作の framework と rpath（新しいフラグの代わりに `TSUZURI_CLANG` のラッパーで確かめた）

`fwdir/M20Probe.framework/M20Probe` を `clang -dynamiclib probe.c -install_name @rpath/M20Probe.framework/M20Probe` で作り（`probe.c` は `long m20_probe_answer(void){return 42;}`）、リンクにだけ `-F$PWD/fwdir -framework M20Probe -Wl,-rpath,@executable_path/fwdir` を足すラッパーで次をビルドすると、0 を返す。`otool -L` に `@rpath/M20Probe.framework/M20Probe`、`otool -l` に `LC_RPATH` の `path @executable_path/fwdir` が出る。

```tsuzuri
@trusted
extern "m20_probe_answer" def probe_answer :: unit -> i64

def main :: unit -> i32 = \() ->
    if probe_answer () == 42 then 0 else 1
```

### コード

- `src/driver.rs:43-56`: `MAX_LINK_INPUTS = 256` と `LinkInputs { paths, libraries, search }`。
- `src/driver.rs:58-75` の `library_name_error`: 最初の文字は英数字か `_`、ほかは英数字と `_+.-`。`lib` の接頭辞と拡張子（`.a`・`.so`・`.dylib`・`.lib`・`.dll`・`.o`・`.obj`・`.tbd`）を拒む。`.framework` は一覧にないので、`-l AppKit.framework` は通ってリンクで失敗する。
- `src/driver.rs:91-127` の `check_shape`（合計 256 の `link_input_limit`、種類ごとの `duplicate_link_input`、`invalid_library_name`）、`129-149` の `check_readable`。
- `src/driver.rs:151-165` の `add_to`: `-L<dir>`、`-x none` とパス、`-l<name>` の順。呼ぶのは `build_complete` の実行ファイルのリンク（5353）と `-g` の別のリンク（5453）、`link_shared`（5806）、`src/test_runner.rs` の `build_debug_runner`（573）と `compile_native_runner`（1094）の 5 か所と、単体テスト（8582）。
- `src/driver.rs:3881-3891`: リンク入力があれば、native の `Executable`・`Shared` 以外を `option_requires_build` にする。`4567-4589`: リンク入力があるビルドはキャッシュを使わない。
- `src/driver.rs:724-736`: `--subsystem` はほかの OS の target で `option_unsupported_platform`（`E2000`）。`1168-1176` の `with_manifest_windows` は、manifest の `[windows]` だけなら Windows でない target で黙って使わない。
- `src/driver.rs:1628-1634` の `link_args`: macOS のホストの Mach-O は Apple の ld でリンクする。`5967-5971` の `bundled_launcher`: clang の stem が `tsuzuri-clang` なら同梱の launcher。
- `src/driver.rs:2806-2845`: `[native]` はルートと `native = true` の path の依存だけ。git・registry は `native_link_setting_not_allowed`（`E2000`）、publish も拒む（`src/fetch.rs:108-116`）。
- `src/package.rs:896-899` のキーの検査（`link`・`libraries`・`search` だけ）、`909-918` の `libraries`（`invalid_native_library_name`）、`919-938` の `link`・`search`（相対パス、`invalid_native_path`）。
- `src/main.rs:1281-1308` の解析、`1904-1914` の `links_apply`（manifest の `[native]` は CLI のリンク入力が効く所でだけ使い、wasm や `--emit object` では黙って使わない）、`3924`・`3966` のスクリプトのオプションの一覧、`272-276` の `--help`。

### 決定と文書

- W17:247 は macOS の framework を、W17:248 は目的ファイルへの要求の埋め込み（Mach-O の `LC_LINKER_OPTION`）を対象外にしている。NC12:70 は「`-framework Accelerate` 等のビルドフラグ」を前提にしている。起票の PR が、この 2 か所に M20 への参照を足す。
- E12 D6 はリンク入力の CLI と `[native]`、D7 はリンク入力があればキャッシュを使わない規則（`_features/_completed/E12-ffi-extensions.md:604-615`）。
- 文書: native-interop.md:302-317、option.md:60・75・430-452、packages.md:308-323、platforms.md:95-99・166、docs/language.md:498-504、README.md:566。

## 変えてはいけない意味

- `--link`・`-l`・`-L`・`[native] link`・`libraries`・`search` の規則、リンク行での順、重複の誤り、256 の上限、パスの読み取りの検査。
- git・registry のパッケージと publish が `[native]` を拒むこと（新しいキーも同じ規則に入る）。`native = true` の規則。
- リンク入力のあるビルドでキャッシュを使わない規則（E12 D7。D4 で改めない）。
- 新しい入力のないビルドのリンク行・キャッシュの鍵・IR・object・診断。例外は仕様 6 の 2 文（`invalid_native_section` の受け付けるキーの一覧と、`script` の `SCRIPT_OPTIONS` の受け付けるオプションの一覧）だけで、それ以外の既存の文は変えない。
- Linux・Windows・WASM の出力。`--target native` で clang に target を渡さないこと。

## 仕様

### Phase の範囲

| Phase | 入るもの | PR |
| --- | --- | --- |
| 1 | CLI の `--framework`・`-F`・`--rpath`、`[native] frameworks`・`framework_search`・`rpath`、リンク行、ほかの OS の扱い、同梱の zig の SDK の探索（D3）、文書 | 1 つ目 |
| 2 | `@link(framework = "...")`（W17 の拡張。D6）、到達した宣言の framework だけを足すこと、`tsuzuri audit` | 2 つ目（W17 の後） |

### 1. CLI（Phase 1）

| オプション | 値 | 意味 |
| --- | --- | --- |
| `--framework NAME` | framework の名前（仕様 3） | リンク行に `-framework NAME` を足す。繰り返せる |
| `-F DIR` | ディレクトリ | framework を探すディレクトリ。リンク行に `-F<DIR>` を足す。繰り返せる |
| `--rpath PATH` | 実行時の探索パス（仕様 4） | リンク行に `-Wl,-rpath,PATH` を足す（`LC_RPATH`）。繰り返せる |

- 分離形だけを受ける（`-l`・`-L` と同じ。`-FDIR`・`--framework=AppKit` は今の未知のオプションの `E2000`）。スクリプトの実行（`src/main.rs:3924`・`3966`）の値を取るオプションにも足し、`SCRIPT_OPTIONS`（`src/main.rs:3903`。script の知らないオプションの文で、受け付けるオプションを並べる）の一覧に 3 つを足す（仕様 6 の例外の 2 つ目）。
- 受けるビルドは `--link`・`-l`・`-L` と同じ（`build` の `--emit exe`・`shared`、`run`、native の `test`・`bench`）。ほかは `option_requires_build` で、新しい 3 つを含むときだけ、文の `remove --link, -l and -L` を `remove --link, -l, -L, --framework, -F and --rpath` にする。この文は `src/main.rs:1835`・`src/driver.rs:3887`・`src/test_runner.rs:138` の 3 か所にあり、どれも同じ規則にする。`test --doc` との併用（`src/main.rs:1420` の `--doc does not use --link, -l, or -L`）も同じく、新しい 3 つを含むときだけ文の一覧を広げる。古いオプションだけの文は変えない。
- 検査の順は `--subsystem`（`src/driver.rs:724-736`）と同じく、`option_requires_build`（wasm の target や `--emit object` など）が先で、macOS でない target の `option_unsupported_platform`（仕様 5）が後である。`--target wasm32 --framework AppKit` は `option_requires_build` になる。
- 合計の上限 256 と種類ごとの重複は、6 つのオプションと manifest の 6 つのキーの全体に効く（今の `link_input_limit`・`duplicate_link_input`）。

### 2. manifest（Phase 1）

```toml
[native]
frameworks = ["AppKit", "Metal"]
framework_search = ["vendor/Frameworks"]
rpath = ["@executable_path/../Frameworks"]
```

- `frameworks` は名前（仕様 3）、`framework_search` は相対パス（`search` と同じ規則）、`rpath` は文字列（仕様 4。パッケージのルートに対して解かない）。
- 1 行の文字列の配列、持てるパッケージ（ルートと `native = true` の path の依存。git・registry と publish は `native_link_setting_not_allowed`）、順序（manifest が先、ルートが先）は今の `[native]` の規則。

### 3. framework の名前（D1）

- 最初の文字は英数字か `_`、ほかは英数字と `_+.-`。末尾の `.framework` とパスの区切りを拒む。`lib` の接頭辞は拒まない。
- SDK 27.2 の 332 個の framework の名前（`_AppIntents_AppKit` のような `_` で始まるものを含む）は、すべてこの規則を満たす（`ls $SDK/System/Library/Frameworks` で確かめた）。

### 4. rpath の値（D2）

- `@executable_path`・`@loader_path`（単独か `/` が続くもの）か、`/` で始まる絶対パス。
- 空、`,`（`-Wl,` が区切る）、制御文字、相対パス（dyld はカレントディレクトリに対して解く）、`@rpath` で始まる値を拒む。

### 5. どのビルドに効くか（Phase 1）

- リンク行（「現状」の 5 か所）の順: `-L<dir>`、`-F<dir>`、（D3 の SDK の `-F`・`-L`）、`-x none` とパス、`-l<name>`、（W17 の `-l`）、`-framework <name>`、（Phase 2 の `-framework`）、`-Wl,-rpath,<path>`（D8）。
- macOS でない target: CLI の 3 つは `E2000`（既存の kind `option_unsupported_platform`）、manifest の 3 つは使わない（D7）。
- ビルドキャッシュ: 今のリンク入力と同じく、あれば使わない（D4）。
- `--emit object`・`llvm`・`bitcode`・`header`: 今のリンク入力と同じく受けない。`LC_LINKER_OPTION` を埋め込まない（対象外）。

### 6. 診断（Phase 1）

| コード | kind | 条件 | メッセージ（案） | 位置 |
| --- | --- | --- | --- | --- |
| E2000 | `invalid_framework_name`（新） | CLI の名前が仕様 3 に当たらない | `invalid framework name '{name}'; pass the name without a path or the '.framework' suffix, for example --framework AppKit` | `<command line>` |
| E0002 | `invalid_native_framework_name`（新） | manifest の名前が仕様 3 に当たらない | 同じ文の `for example frameworks = ["AppKit"]` | 今の `invalid_native_library_name` と同じ（`cursor.error`） |
| E2000 | `invalid_rpath`（新） | CLI の値が仕様 4 に当たらない | `invalid rpath '{value}'; write an absolute path or a path that starts with @executable_path/ or @loader_path/, without commas` | `<command line>` |
| E0002 | `invalid_native_rpath`（新） | manifest の値が仕様 4 に当たらない | 同じ文 | 今の `invalid_native_path` と同じ（`cursor.error`） |
| E2000 | `option_unsupported_platform`（既存） | macOS でない target に CLI の 3 つ | `--framework, -F and --rpath apply only to macOS targets; build for aarch64-apple-darwin or on a Mac, or remove them` | `<command line>` |
| E0002 | `invalid_native_section`（既存） | `[native]` の知らないキー | 文を `expected [native] keys link, libraries, search, frameworks, framework_search or rpath, each an array of strings on one line` に改める | 今と同じ |
| E2002 | `macos_sdk_not_found`（新。D3） | 同梱の toolchain で framework を要るビルドに SDK が見つからない | `linking the macOS framework '{name}' with the bundled toolchain needs the macOS SDK, which 'xcrun --show-sdk-path' did not find; install the Xcode Command Line Tools (xcode-select --install), or set TSUZURI_CLANG to Apple's clang`（`{name}` は名前の順で最初の framework。M01 の `-lobjc` だけのときは `the macOS framework '{name}'` を `libobjc` にする） | リンクの誤り（`external_tool_failed`）と同じ位置 |
| E2000 | `usage_error`（既存。文の改訂） | script の知らないオプション（`src/main.rs:3939-3943` が `SCRIPT_OPTIONS`（`3903`）を出す） | 一覧の `--link, -l, and -L` を `--link, -l, -L, --framework, -F, and --rpath` に改める | 今と同じ |

- `-F` のディレクトリは今の `-L` と同じ `check_readable` で検査する。ないときは `io_failure`、ディレクトリでないときは `link_input_unreadable`（どちらも `E2001`）。
- 古い入力だけのビルドで文が変わるのは、`invalid_native_section` の 1 文と `SCRIPT_OPTIONS` の 1 文だけである。どちらも受け付ける綴りの一覧を正しく保つための変更で、`src/package.rs:1473` と `SCRIPT_OPTIONS` を参照するテスト（`src/main.rs:5071-5074`・`8259`）を同じ PR で改める。

### 7. Phase 2: `@link(framework = "...")`（W17 の後。D6）

```text
link = "@" "link" "(" ( string | "framework" "=" string ) ")"
```

```tsuzuri
@trusted @link(framework = "AppKit")
extern "NSApplicationLoad" def application_load :: unit -> unit
```

- 置ける宣言は W17 の規則（明示の symbol の `extern "sym" def`）と、[M01](M01-objc-class-types.md) の `extern class`（symbol は `OBJC_CLASS_$_<実行時名>`）、[M06](M06-extern-data-symbols-and-zutf8string.md) のデータシンボル。M01・M06 が後なら、その置き場は M01・M06 が足す。
- 名前は仕様 3。誤りは W17 の `link_invalid_name`（`E1008`）、構文の誤りは W17 の `link_attribute_syntax`（`E0002`）を共有し、文に framework の例を足す。
- framework の集合は、最終の IR が参照する symbol の宣言の名前の和（名前の順、CLI と manifest にある名前を除く）。W17 の D5 と同じく、CLI と manifest の `frameworks` と重なる名前は黙って 1 つにし、256 の上限には数える。macOS の target だけで、Mach-O でない native で到達したら `E2000`（kind 案 `link_framework_target`）、wasm は W17 の `link_attribute_wasm`。到達しない宣言は拒まず、`check` は受理する。
- キャッシュを止めない（W17 の D5）。git・registry のパッケージにも書ける（W17 の D2）。同梱の toolchain では D3 の SDK の探索を使う。
- `tsuzuri audit`: 文字は行の末尾に `  link framework AppKit`、JSON は `"framework":"AppKit"`。属性のない行は byte 単位で今のまま。

## 設計

### Phase 1: `LinkInputs`（`src/driver.rs`）

- `LinkInputs` に `frameworks: Vec<String>`、`framework_search: Vec<PathBuf>`、`rpaths: Vec<String>` を足し、`is_empty`・`followed_by`・`check_shape`（6 つの合計、種類ごとの重複、名前と rpath の検査）・`check_readable`（`framework_search` はディレクトリ）を広げる。新しい field が空なら、どの関数も今と同じ値を返す。
- `pub fn framework_name_error` と `pub fn rpath_error`。`src/package.rs` も同じ関数を呼ぶ（`library_name_error` と同じく規則を 1 つにする）。`has_macos_inputs` と `without_macos_inputs`（manifest の分を Mach-O でない target で落とす）。
- `add_to` の順は仕様 5。`-F` はディレクトリを続けた 1 つの引数（`-L` と同じ）、`-framework` と名前は 2 つ、rpath は `-Wl,-rpath,<value>` の 1 つ。
- W17 は `add_to` に `system_libraries` の引数を足し、5 か所の呼び出しをコンパイルの誤りで見つける。M20 はこれを `LinkExtras { system_libraries, system_frameworks, sdk }` にまとめる。W17 より先に入るなら M20 が `LinkExtras { sdk }` を足し、W17 が field を足す。テストの実行ファイルも同じ値を求める。

### Phase 1: CLI と manifest

- `src/main.rs`: 3 つの解析（UTF-8 でない値は `-l` と同じ `option_invalid_value`）。target が決まった所で、CLI の分に macOS の入力があり Mach-O でなければ `option_unsupported_platform`。manifest の分は `followed_by`（`src/main.rs:4294`）の前に、Mach-O でなければ `without_macos_inputs()` にする。`--help` に 3 行を足す。
- `src/package.rs`: `[native]` に 3 つのキー。`framework_search` は今の `relative_path`。`src/driver.rs:2840` の `followed_by` に、`framework_search` をパッケージのルートで解いた値を渡す。

### Phase 1: 同梱の zig の SDK（D3）

- `fn macos_sdk_link_args(platform, clang) -> Result<Vec<OsString>, Diagnostic>`: `platform.mach_o() && bundled_launcher(clang)` のときだけ `xcrun --show-sdk-path` を 1 回実行し（`OnceLock`）、`-F<SDK>/System/Library/Frameworks` と `-L<SDK>/usr/lib` を返す。起動できない、失敗する、空を返す、ディレクトリがないときは `macos_sdk_not_found`。ほかの clang では空（clang が SDK を探す）。
- framework の集合（Phase 2 は `@link` の分も）が空でないビルドだけが呼ぶ。M01 の自動の `-lobjc` も使う。
- 実行する `xcrun` は内側の関数の引数にし、単体テストは偽の `xcrun` のパスを渡す（並行に走るテストのために `PATH` をプロセス全体で変えない）。

### Phase 2: 構文・検査・リンク行

- W17 の `LinkAttribute` に `kind: LinkKind`（`Library`・`Framework`）。符号化に kind を足し `FRONTEND_FORMAT` を 1 上げる。`tsuzuri fmt` は形を保つ。
- パーサーは W17 の `link_attribute` に `framework` `=` 文字列を足す（`src/parser.rs:1500-1553` の `@c_layout(pack = n)` と同じ読み方）。`signature` の先読みは W17 が足した `link` で足りる。
- `CheckedModule::link_frameworks`（symbol → 名前）。driver は W17 の `system_libraries` と同じ形の `system_frameworks`。Mach-O でない native で空でなければ `link_framework_target`、wasm は W17 の拒否の集合に入れる。audit は `ExternReport` に `framework`。

## 前提とする他チケットのインターフェース

- Phase 1: なし（単独で出荷できる）。
- Phase 2: [W17](W17-link-attribute-dllimport.md) の `LinkAttribute`・`Program::links`・`link_attribute`（パーサー）・`HostImport::library`・`CheckedModule::link_libraries`・`system_libraries`・`add_to` の `system_libraries` の引数・wasm の拒否の集合・`ExternReport::link`。W17 の D2（git・registry のパッケージに書けること）の承認。
- [M22](M22-attribute-after-bodyless-extern-def.md)（弱い）: 本体のない `extern def` の直後の属性が制約行と読まれない。M22 が後なら、W17 が `signature` の先読みに足した `link` で足りる。
- [M01](M01-objc-class-types.md)・[M06](M06-extern-data-symbols-and-zutf8string.md)（あれば）: `extern class` とデータシンボルの宣言と、その symbol。

## 他チケットへ提供するインターフェース

- `LinkInputs::{frameworks, framework_search, rpaths}`、`framework_name_error`、`rpath_error`、`LinkExtras`。
- `macos_sdk_link_args`: [M01](M01-objc-class-types.md) は、ObjC を使うプログラムに自動で足す `-lobjc` を、同梱の toolchain でもリンクできるようにこの関数を使う（COMMON の M01 の弱い依存）。
- `[native] rpath` と `--rpath`: [M15](M15-macos-app-bundle.md) の `embed` した dylib は `@executable_path/../Frameworks` の rpath を要る。M15 は `--emit app` のときにこの値を足さず、利用者が `--rpath` か `[native] rpath` で書く（M15 の D12。リンク行を変えず、`--emit exe` とキャッシュの項目を共有するため）。
- Phase 2 の `@link(framework = "...")` と `CheckedModule::link_frameworks`: [M07](M07-apple-sdk-bindgen-and-package.md) の生成器はヘッダーの framework を `@link(framework = "AppKit")` に写し、公式の `apple` パッケージ（git）が framework を要求できる。[M14](M14-tls-http-packages.md) は `Network`・`Security` の framework を同じ形で要求する。NC12 の外部 BLAS は `--framework Accelerate` か `@link(framework = "Accelerate")` を使う。

## 実装手順

### Phase 1

1. **ベースライン**。変更: なし（`target/scratch/m20/` の下書きだけ）。内容: 着手条件の基準を保存し、「現状」の再現を記録する。確認: 保存した件数と、再現の表の各行の結果を記録した。
2. **`LinkInputs` と名前・rpath の検査**。変更: `src/driver.rs`・`src/diagnostic_kinds.rs`。内容: 設計の `LinkInputs`、仕様 3・4・6 の kind。確認: T0。単体テスト `framework_names_follow_the_library_rule`・`rpaths_are_anchored_and_comma_free`・`link_inputs_count_frameworks_toward_the_limit`。
3. **リンク行**。変更: `src/driver.rs`（`add_to`・`LinkExtras`・5 か所）、`src/test_runner.rs`（2 か所）。内容: 仕様 5 の順。確認: `frameworks_follow_libraries_on_the_link_line`（`Command` の引数を比べる）。新しい入力のないビルドのリンク行が基準と一致する（S2）。T1。
4. **CLI と manifest**。変更: `src/main.rs`・`src/package.rs`・`src/driver.rs`（`Project::load_from_root` の `followed_by`）。内容: 仕様 1・2、ほかの OS の扱い（D7）、`--help`。確認: `parses_link_inputs`・`rejects_ambiguous_or_unused_arguments` の拡張、`native_frameworks_parse`・`native_rpath_rejects_relative_paths`、`tests/packages.rs` の `git_packages_reject_native_frameworks`。T1。
5. **同梱の zig の SDK**（D3 の承認の後）。変更: `src/driver.rs`。内容: `macos_sdk_link_args` と `macos_sdk_not_found`。確認: 偽の `xcrun` を渡す単体テスト `bundled_launcher_adds_the_sdk_for_frameworks`（成功と、`xcrun` が失敗するときの `E2002`）。Apple の clang では引数が変わらないこと。
6. **E2E と文書**。変更: `tests/macos_link_inputs.mjs`（新規）、`.github/workflows/targets.yml`、文書、`_features/NC12-ffi-numeric-buffers-and-blas-lapack.md`・`_features/W17-link-attribute-dllimport.md`。内容: 「テスト計画」の E2E、「ドキュメント」。確認: `node tests/macos_link_inputs.mjs target/debug/tsuzuri`（macOS）、T2、`node scripts/check-docs.mjs <変更したページ>`、`node scripts/gen-docs.mjs --check`、`node scripts/check-cli-docs.mjs`。PR に `ci:full` を付ける（macOS のリンクを変える）。

### Phase 2（人間が求め、W17 が done のとき）

7. **構文と検査**。変更: `src/syntax.rs`・`src/parser.rs`・`src/syntax_codec.rs`・`src/frontend_cache.rs`・`src/formatter.rs`・`src/check.rs`・`src/diagnostic_kinds.rs`。内容: 仕様 7 の構文・名前・置き場。確認: `tests/link_attribute.rs` の `framework_link_attribute_parses_and_checks_names`。`tsuzuri fmt` が形を保つ。T1。
8. **リンク行と target**。変更: `src/driver.rs`・`src/test_runner.rs`・`src/llvm.rs`（wasm の拒否の集合）。内容: `system_frameworks`、`link_framework_target`、D3 の SDK。確認: `only_reached_symbols_add_frameworks`・`framework_link_attribute_keeps_the_build_cache`・`framework_link_attribute_is_macos_only`。T1。
9. **audit・パッケージ・E2E・文書**。変更: `src/capabilities.rs`・`tests/capabilities.rs`・`tests/packages.rs`・`tests/macos_link_inputs.mjs`・文書。確認: 「テスト計画」の Phase 2 の項目。T2。`ci:full`。

## テスト計画

### Rust（どのホストでも）

- `src/driver.rs` の単体テスト:
  - `framework_names_follow_the_library_rule`: `AppKit`・`_AppIntents_AppKit`・`libxml2` を受理。`""`・`AppKit.framework`・`../AppKit`・`/System/Library/Frameworks/AppKit.framework`・`-framework`・`App Kit` を拒む。
  - `rpaths_are_anchored_and_comma_free`: `@executable_path`・`@executable_path/../Frameworks`・`@loader_path/lib`・`/opt/lib` を受理。`""`・`lib`・`./lib`・`@rpath/x`・`@executable_pathx`・`/a,b`・`/a\nb` を拒む。
  - `link_inputs_count_frameworks_toward_the_limit`: 6 種類の合計 257 で `link_input_limit`、同じ `--framework` の 2 回で `duplicate_link_input`。
  - `frameworks_follow_libraries_on_the_link_line`: `add_to` の引数が仕様 5 の順。新しい field が空なら今と同じ列。
  - `bundled_launcher_adds_the_sdk_for_frameworks`（unix）: 内側の関数に偽の `xcrun` のパスを渡し、clang が `tsuzuri-clang` のときだけ SDK の `-F`・`-L` が返る。偽の `xcrun` が失敗すると `macos_sdk_not_found`。
- `src/main.rs`: `parses_link_inputs` に 3 つの順と繰り返し、`rejects_ambiguous_or_unused_arguments` に値のない 3 つ（`needs a value`）、`check` での誤り、`test --doc` との併用。
- `src/package.rs`: `native_frameworks_parse`、`native_rpath_rejects_relative_paths`、`invalid_native_section` の新しい文（`src/package.rs:1473` の期待を改める）。
- `tests/packages.rs`: `git_packages_reject_native_frameworks`（今と同じ `native_link_setting_not_allowed`）。`native = true` の path の依存の `frameworks` がルートの後に並ぶ。
- `tests/targets.rs`（既存の `fake_toolchain` の偽の clang でリンク行を記録）: Linux の target の `--framework` は `option_unsupported_platform`、manifest の `frameworks` は Linux のリンク行に出ず、`--target aarch64-apple-darwin` では出る。新しい入力のないビルドのリンク行が基準と一致する。

### E2E（`tests/macos_link_inputs.mjs`、新規。macOS だけで、ほかのホストでは skip を 1 行出す）

- Apple の clang と、あれば同梱の toolchain の両方で、`-O0` と `-O3`:
  - 「現状」の `fw/Main.tz` を `--framework AppKit` で作ると 0 で、`otool -L` に AppKit。`-l objc` だけなら 1。manifest の `frameworks = ["AppKit"]` でも同じ。
  - テストの中で `M20Probe.framework` を作り（「現状」の手順）、`-F <dir> --framework M20Probe --rpath @executable_path/<dir>` の実行ファイルが 0 で、`LC_RPATH` が値と一致する。`--rpath` を外すと dyld が起動を止める。
  - `--emit shared` の dylib にも `-framework` と `LC_RPATH` が入る。
- 同梱の toolchain で、`PATH` の先頭に失敗して終わる偽の `xcrun` を置くと `macos_sdk_not_found`（`/usr/bin` を `PATH` から外すと、ほかの道具も見つからなくなるので外さない）。
- プログラムはウィンドウを作らない（`objc_getClass` だけ）。
- `.github/workflows/targets.yml` の macOS のジョブに手順「macOS framework link inputs (M20)」を足す（同梱の toolchain は同じジョブが作る）。

### Phase 2

- `tests/link_attribute.rs`（W17 のファイル）: `framework_link_attribute_parses_and_checks_names`（`@link(framework)`・`@link(framework = AppKit)`・`@link(frameworks = "AppKit")` は `link_attribute_syntax`、`"AppKit.framework"` は `link_invalid_name`）、`only_reached_symbols_add_frameworks`、`framework_link_attribute_keeps_the_build_cache`、`framework_link_attribute_is_macos_only`（Linux で到達すると `link_framework_target`、wasm32 は `link_attribute_wasm`、到達しないだけなら成功）。
- `tests/capabilities.rs`: audit に `framework` が出て、属性のない行は変わらない。`tests/packages.rs`: git パッケージの `@link(framework = "AppKit")` で `check` が成功する。
- `tests/macos_link_inputs.mjs`: `@link(framework = "AppKit")` を付けた `extern "NSApplicationLoad" def application_load :: unit -> unit` を呼ぶプログラムが、`--framework` なしに両方の toolchain でリンクでき 0。`NSApplicationLoad` は `BOOL` を返すが、結果を `unit` と宣言して捨てても、今の回避策（`--link $SDK/.../AppKit.tbd`）で `-O0`・`-O3` とも 0 を返し、ウィンドウもプロセスも残らないことを確かめた（HEAD `804b7970`、Apple clang）。

### 比較

- 手順 1 の基準と、この機能を使わない全入力の `--emit llvm`・リンク行・キャッシュの鍵が byte 単位で一致する。比べた件数を PR に書く。

## ドキュメント

- `_tsuzuri/language-reference/compiler/option.md`: 「リンクと allocator」の表に 3 行、「どのサブコマンドが受け付けるか」の表（60 行）の `--link` / `-l` / `-L` の行、繰り返せるオプションの一覧と 256 の上限（75 行）、組み合わせの誤りの表に `option_unsupported_platform` の文。
- `_tsuzuri/language-reference/compiler/native-interop.md` の「リンク入力」に小節「macOS の framework と rpath」: `--framework`・`-F`・`--rpath`・manifest のキー、同梱の toolchain が SDK を要ること、`.tbd` を `--link` に渡す今の回避策の扱い（D5）。Phase 2 は「extern ごとの framework（@link(framework = ...)）」。
- `_tsuzuri/language-reference/organizing-tsuzuri/packages.md` の `[native]` の節（308〜323 行）: 3 つのキー。
- `_tsuzuri/language-reference/compiler/platforms.md` の同梱の toolchain の節（95〜99 行）と診断の表: framework を要るビルドは Xcode Command Line Tools の SDK を要ること、`macos_sdk_not_found`。
- Phase 2: `_tsuzuri/language-reference/values-and-functions/attributes.md` の `@link` の節に framework の形。
- `docs/language.md` の「リンク入力」（498〜504 行）、`README.md` の特徴の一覧（566 行）、`compiler/diagnostics.md` の生成された表（`node scripts/gen-docs.mjs`）。
- `_features/NC12-ffi-numeric-buffers-and-blas-lapack.md:70` と `_features/W17-link-attribute-dllimport.md:247` の M20 への参照を、出荷した綴りの説明に改める。
- macOS 専用の例は `run=` を付けない（`check-docs.mjs` はどのホストでも走る）。

## 受け入れ条件

- [ ] 仕様 1〜6 のとおりに動く（Apple の clang と同梱の toolchain、`-O0`・`-O3`）。Phase 2 を実装したなら仕様 7 も。
- [ ] `--framework AppKit` と `[native] frameworks` で AppKit がリンクされ、自作の framework が `-F` と `--rpath` で動く。
- [ ] macOS でない target の CLI の 3 つは `option_unsupported_platform`、manifest の 3 つは使われない。
- [ ] 同梱の toolchain で framework がリンクでき、SDK がなければ `macos_sdk_not_found`。
- [ ] git・registry のパッケージの `[native]` は今どおり拒まれる。リンク入力のあるビルドは今どおりキャッシュを使わない。
- [ ] 新しい入力のないビルドのリンク行・IR・キャッシュの鍵・診断（仕様 6 の 2 文を除く）が変わらない（比べた件数を記録した）。
- [ ] 新しい診断コードを足していない。文書を直し、T2・`check-docs`・`gen-docs --check`・`check-cli-docs` が通る。PR に `ci:full` を付けた。

## 落とし穴

- **`-framework` と名前は 2 つの引数**。`-frameworkAppKit` や、空白を含む 1 つの引数 `"-framework AppKit"` は、clang が別のオプションか入力として読む。
- **`-Wl,` は `,` で区切る**。rpath の値に `,` を許すと、ld に別の引数として渡る。仕様 4 で拒む。
- **`.framework` は今の `library_name_error` を通る**（「現状」）。framework の名前の検査を `library_name_error` の流用で済ませると、末尾の `.framework` を拒めない。
- **manifest の 3 つを Mach-O でない target で拒まない**。拒むと、`frameworks` を持つアプリを Linux 向けにビルドできなくなる。CLI だけを拒む（D7）。
- **SDK の `-L` は利用者の `-L` の後**。前に置くと、利用者のディレクトリの同名のライブラリより SDK の `.tbd` が先に選ばれる。
- **`xcrun` は CLT のない macOS で導入を促すことがある**。テストは偽の `xcrun` で失敗を作る。
- **`add_to` の 5 か所**。`-g` の別のリンクとテストの実行ファイルを落とすと、`-g` と `tsuzuri test` だけがリンクに失敗する。引数を足してコンパイルの誤りで全部を見つける（W17 と同じ）。
- **Phase 2 の framework を `LinkInputs::frameworks` に入れない**。キャッシュが止まり、`--framework` との併用が重複の誤りになる。
- **AppKit の `.tbd` の依存から libobjc が入る**（「現状」の `otool -L`）。E2E は `-l objc` の有無でなく、`-framework` の有無で結果が変わることを見る。

## 対象外

- 任意のリンカーのフラグ（`-Wl,...`）の素通し。`-sectcreate`（Info.plist の埋め込み。M15 の判断）、`-headerpad`。
- `--emit object`・`llvm`・`bitcode` への `LC_LINKER_OPTION` の埋め込み（W17:248 と同じ理由）。
- 独自の framework を作ること（Tsuzuri の出力を `.framework` に包む）。
- 同梱の toolchain で C ランタイムを含む Mach-O の `--emit object`（platforms.md:166 の既知の制限）。
- macOS でないホストから framework をリンクすること（SDK がない。`macos_sdk_not_found`）。
- Linux の rpath（`$ORIGIN`）。`--rpath` は macOS の target だけ（D7）。
- `-l` の system library 一般への SDK の `-L` の追加（framework を要るビルドと M01 の `-lobjc` だけ。D3）。
- iOS など macOS 以外の Apple の target（target がない）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | Phase 1 の綴りは CLI の `--framework NAME`・`-F DIR`・`--rpath PATH` と、manifest の `[native] frameworks`・`framework_search`・`rpath`（D-55 の登録）。W17 に依らない。名前の検査は `library_name_error` に倣い、末尾の `.framework` とパスを拒み、`lib` の接頭辞は拒まない | clang・ld と同じ綴りで、`-l`・`-L` と並ぶ。`[native]` の持てるパッケージの規則（供給網の危険。E12 D6）をそのまま使える。退けた案: `-l` の拡張（`library_name_error` と衝突し、`-L` と `-F` も区別できない）、`[macos] frameworks`（M15 の bundle に同梱するファイルと混ざる。COMMON で `embed` に改めた経緯と同じ） | 要承認（D-55。承認前は手順 2 以降に着手しない） |
| D2 | rpath の値は `@executable_path`・`@loader_path`・絶対パスだけ。空・`,`・制御文字・相対パス・`@rpath` を拒む | rpath は実行時に dyld が解く文字列で、ビルド機のパスではない。相対パスは起動したディレクトリで結果が変わる。`,` は `-Wl,` が区切る | 既定案（実装者はこの案に従う） |
| D3 | 同梱の toolchain で Mach-O の framework を要るビルドは、`xcrun --show-sdk-path` の SDK の `-F<SDK>/System/Library/Frameworks`・`-L<SDK>/usr/lib` を利用者の `-F`・`-L` の後に足す。見つからなければ `E2002`（`macos_sdk_not_found`）で Command Line Tools を入れる指示を付ける。M01 の `-lobjc` も使う | zig は libSystem のスタブしか持たず、SDK を足せばリンクできる（「現状」）。退けた案: 同梱の toolchain に `TSUZURI_SYSROOT` を許す（platforms.md:98 の規則と launcher の `-isysroot` の拒否を改める）、SDK を同梱する（配布物の大きさと SDK の扱いが別の判断になる） | 要承認（承認前は手順 5 に着手しない） |
| D4 | framework・`-F`・rpath は `-l` と同じくリンク入力として扱い、あればビルドキャッシュを使わない。E12 D7 は改めない | 鍵は `-F` のディレクトリの中身を知らない。名前だけの `--framework` を鍵に入れると `-l` と規則が分かれる。Phase 2 の `@link(framework = ...)` は W17 の D5 と同じくキャッシュを止めない | 要承認（E12 D7 の扱い。承認前は手順 3 に着手しない） |
| D5 | `--link` に SDK の `.tbd` を渡す今の挙動は変えず、文書では支える使い方として載せない。framework は `--framework` を案内する | `.tbd` を読むのは Apple の ld だけで、同梱の zig は依存を解けない（「現状」）。文書の `--link` は「ホストのオブジェクトか静的ライブラリ」。退けた案: `.tbd` を禁止する（今動くビルドを壊す） | 要承認（承認前は手順 6 の文書に着手しない） |
| D6 | Phase 2 で W17 の `@link` に `@link(framework = "...")` を足す（W17:247 の対象外の改訂と D-55）。置き場は W17 の規則と M01 の `extern class`、M06 のデータシンボル。構文の誤りは W17 の `link_attribute_syntax` を共有する | 名前だけなので git・registry のパッケージにも書ける（W17 の D2 と同じ理由）。M07 の公式の `apple` パッケージが framework を要求する唯一の形になる。`@c_layout(pack = n)` と同じ key = value の形で、属性の名前を増やさない。退けた案: `@framework("AppKit")`（D-53 の閉じた属性の集合をもう 1 つ広げる）、`@link("framework:AppKit")`（名前の規則と衝突する） | 要承認（W17 の改訂と D-55。承認前は Phase 2 に着手しない） |
| D7 | macOS でない target では、CLI の 3 つを `E2000`（既存の `option_unsupported_platform`）にし、manifest の 3 つは使わない。Phase 2 の到達した `@link(framework = ...)` は `E2000`（kind 案 `link_framework_target`） | CLI は使えない組み合わせを黙って無視しない（GUIDE §6.8）。kind は COMMON の案 `macos_only_option` を足さず、同じ意味の既存の kind（`--subsystem` が使う）にする（GUIDE §6.7）。manifest はアプリの性質で、同じアプリを Linux 向けにもビルドできる（manifest の `[windows]` と `links_apply` と同じ）。`--rpath` を Linux に広げるのは対象外 | 既定案（実装者はこの案に従う） |
| D8 | リンク行の順は `-L`、`-F`、SDK の `-F`・`-L`、パス、`-l`、W17 の `-l`、`-framework`、Phase 2 の `-framework`、`-Wl,-rpath` | 探索のディレクトリを先に、利用者のものを SDK より先に置く。決定的な順にする | 既定案（実装者はこの案に従う） |
| D9 | 新しい診断コードを割り当てない。新しい kind は `E2000` の `invalid_framework_name`・`invalid_rpath`・`link_framework_target`、`E0002` の `invalid_native_framework_name`・`invalid_native_rpath`、`E2002` の `macos_sdk_not_found` | D-51、E12 D12。CLI の値は `E2000`、manifest は `E0002`、ツールの不足は `E2002` という今の分類（`invalid_library_name`・`invalid_native_library_name` と同じ組） | 既定案（実装者はこの案に従う） |
