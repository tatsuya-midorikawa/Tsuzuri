# W10: Windows のリソース（マニフェスト・アイコン・版情報・`.rc`）を埋め込む

| 項目 | 内容 |
| --- | --- |
| ID | W10 |
| 優先度 | P2 |
| 規模 | M |
| 依存 | [W09](W09-windows-gui-subsystem.md) |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-22（D-63。利用者の依頼「X: D13 を狭く改める方針で決定。実現向けたチケットを @_features に W プリフィックスのチケットを作成して」と「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」から起票。HEAD `fa40eede` で確認） |
| 承認 | 要承認（決定事項 D1〜D3 の 3 つ。D1 は E12 D7 の改訂で、リソースだけはキャッシュの鍵に内容を入れてキャッシュを使う。D2 は D-55 への登録で、`[windows]` のキー `manifest`・`icon`・`version`・`resources` と環境変数 `TSUZURI_RC`。D3 は既定のマニフェストを入れないこと） |
| 手本にする既存実装 | `src/package.rs` の `[native]`（`relative_path`・`string_array`）、`src/driver.rs` の `pdb_link_args`（作業ディレクトリに書いたファイルをリンクに渡す）と `protect_links`、`resolve_tool`・`tool`（環境変数 → 配布物 → PATH）と `TSUZURI_LLVM_LINK` の探し方（Clang の隣も見る）、`src/cache.rs` の `build_key` の `profile`（あるときだけ鍵にフィールドを足す） |
| 主な影響ファイル | `src/winres.rs`（新規）・`src/lib.rs`・`src/package.rs`・`src/driver.rs`・`src/cache.rs`・`src/main.rs`（`HELP` の環境変数）・`src/diagnostic_kinds.rs`・`tests/winres.rs`（新規）・`tests/fixtures/windows_resources/`（新規）・`.gitattributes`（新規）・`tests/windows.mjs`（GUI の段）・`README.md`・`_tsuzuri/language-reference/organizing-tsuzuri/packages.md`・`_tsuzuri/language-reference/compiler/option.md`・`_tsuzuri/language-reference/compiler/platforms.md`・`_tsuzuri/language-reference/compiler/native-interop.md`・`_tsuzuri/language-reference/compiler/diagnostics.md`（生成）・`docs/language.md`・`docs/architecture.md` |

## 実装者向け早見

- 範囲: ルートの `Tsuzuri.toml` の `[windows]` から、Windows の実行ファイルに次の 4 つを埋め込む。
  - アプリケーションのマニフェスト（`manifest`）。
  - アイコン（`icon`）。
  - 版情報（`version`）。
  - 任意の `.rc`・`.res`（`resources`）。
- 作り方: manifest・icon・version は、Rust で決定的な `.res` を書く（新しい crate なし）。`.rc` は資源コンパイラ（`TSUZURI_RC`、同梱の `zig rc`、`llvm-rc`、`rc.exe`）で `.res` にする。全部を Rust で 1 つの `.res` にまとめ、リンクに 1 つの入力として渡す。キャッシュの鍵には、まとめた `.res` のバイト列を入れる（E12 D7 の改訂）。
- 最初に読む: 仕様 1〜5、W09 の「他チケットへ提供するインターフェース」、`src/driver.rs` の `build_complete` のキャッシュの段と実行ファイルのリンクの段、`src/cache.rs` の `build_key`。
- 最初に書くテスト: `tests/winres.rs` の `manifest_icon_and_version_match_the_resource_compiler`。Rust の書き手の出力を、同じ内容の `.rc` を `llvm-rc` でコンパイルした参照の `.res`（fixture）と、バイト単位で比べる。
- 触ってはいけない:
  - `[windows]` のリソースのキーがないビルドの、リンクの引数・キャッシュの鍵・IR・exe。
  - `[native] link` のキャッシュの扱い（今どおり使わない）。
  - `test`・`bench` の runner。
- 完了の見分け方: マニフェストに `activeCodePage` UTF-8 を書いたアプリで、次の 2 つが成り立つ。
  - `extern "GetACP"` が 65001 を返す（OS がそのマニフェストを読んだ）。
  - 2 回目のビルドがキャッシュに当たる。

## 目的

Win32 のアプリには、ほぼ必ず次の 3 つが要る。

- アプリケーションのマニフェスト。用途は Common Controls 6、DPI の扱い（PerMonitorV2）、`activeCodePage` UTF-8、`longPathAware`。
- アイコン。
- 版情報。

今は `[native] link = ["app.res"]` で埋め込める（「現状」）。ただし、その方法には欠点が 3 つある。

- `.res` を別の道具で作る必要がある。
- `.rc` を直接渡せるのは同梱のツールチェーンだけ。
- リンク入力なのでビルドキャッシュがすべて止まる（E12 D7）。

マニフェストのキーで書けるようにし、ツールチェーンによらず同じ `.res` を作り、キャッシュを保つ。

## 着手条件と停止条件

### 着手条件

- D1〜D3 の承認。
- W09 が done（`[windows]` の節・`WindowsSettings`・`Project::windows`・`windows_apply` がある）。
- W06 が入っていれば、その `peResources`・`launchWithoutHandles` を使う。入っていなければ、W06 の仕様 4 と同じ名前・同じ形で、必要な部分だけを足す。

### 停止条件

- **S1** Rust の書き手の出力が、同じ内容の `.rc` の `llvm-rc`・`zig rc`・`rc.exe` の出力と一致しない。しかも、3 つの道具のあいだでも一致しない（どれを参照にするか決められない）。
- **S2** ARM64 の同梱の llvm-mingw の `ld.lld` が、`.res` の入力を受けない。x64 の 3 つのリンカーでしか確かめていない。
- **S3** リソースのキーがないビルドのキャッシュの鍵が変わる。W09 の `BuildOptions` の変更による 1 回だけの変化は除く。
- **S4** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

- **入れる方法**: リソースを入れる設定はない。`[native] link` と `--link` は、ファイルを `-x none` の後ろにそのまま渡す（`src/driver.rs` の `LinkInputs::add_to`）。`check_readable` が確かめるのは、ファイルであることだけである。
- **キャッシュ**: `build_complete` は、`links.is_empty()` でないビルドのキャッシュを開かない。理由は E12 D7（[E12](_completed/E12-ffi-extensions.md)）で、「`build_key` は入力ファイルの内容を知らない。内容を鍵に含めるのは費用に見合わない」。
- **このマシンで確かめたこと**: 開発版と、同梱の 0.1.6（zig 0.16.0）と、MSVC 14.44 の CRT・SDK と clang 22.1.8（既定のリンカーは lld-link）を使った。
  - `.res` の連結: `tsuzuri build try --link app.res` は、同梱と MSVC（lld-link）の両方で `.rsrc` を作った。中身は `llvm-readobj --coff-resources` で ICON・GROUP_ICON・VERSIONINFO・MANIFEST の 4 つ。`clang -fuse-ld=link`（link.exe）でも、C の探りに `.res` を渡して同じ 4 つになった。
  - 既定のマニフェスト: link.exe も lld-link も、既定のマニフェストを足さず、`*.manifest` のファイルも作らなかった。リソースを渡さない exe には `.rsrc` の節がない（同梱・MSVC とも）。
  - `.rc` の連結: `--link app.rc` は、同梱では通った（`zig cc` が `.rc` をコンパイルする）。MSVC では `lld-link: error: res\app.rc: unknown file type` で失敗した。そのときの `E2002` の助言は「install LLVM/Clang 17+ or set TSUZURI_CLANG」だった（W03 の項目 4）。
  - キャッシュ: `[native] link` のあるビルドでは、キャッシュの項目（64 桁の 16 進のディレクトリ）が作られなかった。
  - 資源コンパイラの一致: 同じ `.rc`（マニフェスト・16×16 の 32 bit のアイコン・`VERSIONINFO`）を 3 つでコンパイルした。`zig rc`（同梱の zig 0.16.0）・`llvm-rc`（LLVM 22.1.8）・`rc.exe`（Windows SDK 10.0.26100.0）の出力は、バイト単位で一致した（2044 バイト）。3 つとも `/nologo /fo <out> <in>` を受けた。
  - 一致した `.res` の中身は次のとおり。
    - 先頭: 32 バイトの空の見出し。
    - RT_MANIFEST（24）の ID 1: `MemoryFlags` は 0x0030。
    - RT_ICON（3）の ID 1: 0x1010。
    - RT_GROUP_ICON（14）の ID 1: 0x1030。
    - RT_VERSION（16）の ID 1: 0x0030。
    - 言語はどれも 0x0409。
- **道具のありか**: 同梱の x64 の `toolchain/bin` は `clang.exe`・`tsuzuri-clang.exe`・`wasm-ld.exe`・`tsuzuri.exe`（と DLL）だけで、`llvm-rc`・`llvm-cvtres`・`lld-link` はない。`toolchain/zig/zig.exe rc` は使える。
  - 初回の `zig rc` は、resinator を zig の大域のキャッシュ（`%LOCALAPPDATA%\zig`）にビルドする。この 1 回に約 2.2 秒かかった。
  - ARM64 の配布物は zig の代わりに llvm-mingw を同梱し、`bin` には `scripts/toolchain/bundle.mjs` の `llvmTools`（`clang`・`wasm-ld`・`ld.lld`）だけを写す。そのため、資源コンパイラはない（コードで確認。ARM64 の機械では未確認）。
  - このマシンのシステムの LLVM 22.1.8 には `llvm-rc.exe` と `lld-link.exe` があり、`llvm-cvtres` はない。どのリンカーも `.res` を直接受けるので、`cvtres` は要らない。

## 仕様

### 1. キー

```toml
[windows]
subsystem = "windows"            # W09
manifest = "app.manifest"        # RT_MANIFEST, ID 1
icon = "app.ico"                 # RT_ICON 1..n and RT_GROUP_ICON 1
version = "1.2.3.4"              # RT_VERSION 1
resources = ["dialogs.rc", "prebuilt.res"]
```

| キー | 値 | 埋め込むもの |
| --- | --- | --- |
| `manifest` | ルートのパッケージからの相対パス。UTF-8 の XML（BOM 可）。1 MiB まで | RT_MANIFEST（24）の ID 1、言語 0x0409、`MemoryFlags` 0x0030。中身はファイルのバイトのまま |
| `icon` | 相対パスの `.ico` | 画像ごとに RT_ICON（3）の ID 1..n（0x1010）と、RT_GROUP_ICON（14）の ID 1（0x1030）。Explorer は最初のグループを exe のアイコンにする |
| `version` | `"A.B.C"` か `"A.B.C.D"`。各部分は 0〜65535 の 10 進で、先頭の 0 なし。省略した D は 0 | RT_VERSION（16）の ID 1（0x0030）。中身は仕様 2 |
| `resources` | 相対パスの配列（1 行）。拡張子は `.rc` か `.res` | `.rc` は資源コンパイラ（仕様 3）で、`.res` はそのまま。中の資源を全部入れる |

- 既定のマニフェスト・アイコン・版情報は入れない（D3）。キーがなければ、今と同じく `.rsrc` を作らない。
- `[windows]` は、依存パッケージのものを読まない（W09）。

### 2. 版情報の中身

同じ内容の `.rc` を資源コンパイラにかけたときと、同じバイト列にする（テストの参照）。

```text
1 VERSIONINFO
FILEVERSION A,B,C,D
PRODUCTVERSION A,B,C,D
FILEFLAGSMASK 0x3f
FILEFLAGS 0
FILEOS 0x40004
FILETYPE 1
FILESUBTYPE 0
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904b0"
    BEGIN
      VALUE "FileVersion", "<version の文字列>"
      VALUE "ProductVersion", "<version の文字列>"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
```

- 文字列は、書いたとおりの `version` の値にする（`"1.2.3"` は `"1.2.3"`）。
- `ProductName`・`FileDescription`・`CompanyName`・`LegalCopyright` などは入れない。`[package] name` から補わない（D3）。それらは `resources` の `.rc` に書く。その `.rc` が `VERSIONINFO` も持つなら、`version` のキーとは同時に使えない（仕様 4 の重複）。

### 3. 資源コンパイラ（`.rc`）

- 探す順は次のとおり。最初に見つかったものを使う。
  1. `TSUZURI_RC`: `rc.exe` の引数を受ける実行ファイル（`rc.exe`・`llvm-rc`）。空でも「設定済み」で、配布物や PATH へは進まない（`option.md` の環境変数の規則）。
  2. `TSUZURI_CLANG` が同梱の `tsuzuri-clang`（`bundled_launcher`）で、`TSUZURI_ZIG` か配布物の `zig/zig.exe` があれば、`zig rc`。
  3. `TSUZURI_CLANG` の Clang と同じディレクトリの `llvm-rc`、次に PATH の `llvm-rc`。
  4. PATH の `rc.exe`（MSVC の開発者シェル）。
  5. どれもなければ `E2002`（仕様 6）。
- 呼び方: `<rc> /nologo /fo <作業ディレクトリ>/rc-<n>.res <.rc の絶対パス>`。作業ディレクトリ（cwd）は、その `.rc` のディレクトリにする。`zig rc` は `zig rc` の後ろに同じ引数を付ける。
- 出力の `.res` は、仕様 4 の読み手で読む。資源コンパイラの終了コードが 0 でなければ、`E2002`（`external_tool_failed`）で、道具の出力を付ける。

### 4. まとめ方とリンク

- 入力の順は `manifest`、`icon`、`version`、`resources` の記述順。各入力の資源を、この順に 1 つの `.res`（作業ディレクトリの `resources.res`）にまとめる。
- RT_ICON（3）と RT_CURSOR（1）の数の ID は、入力ごとに振り直す。前の入力が使った最大の ID の次から振り、同じ入力の RT_GROUP_ICON（14）・RT_GROUP_CURSOR（12）の項目の `nID` も書き換える。rc は ICON を書くたびに RT_ICON の ID を 1 から振るので、振り直さないと、`icon` のキーと `.rc` のアイコンが必ずぶつかる。RT_GROUP_ICON の名前は利用者が付けたものなので、振り直さない。`icon` のキーは名前 1 を使うので、`.rc` のアイコンは 1 以外の名前にする（同じなら次の重複の誤り）。
- そのほかの資源で、型・名前・言語の組が同じものが 2 つあれば `E2000`（仕様 6）。
- まとめた `.res` は、実行ファイルのリンクの Clang の実行に `-x none <resources.res>` で、リンク入力（`links.add_to`）の後に足す。link.exe・lld-link・MinGW の `ld.lld`（`zig cc`）は、どれも `.res` を直接受ける（確かめた）。
- 効く範囲は W09 の `windows_apply`（Windows の native の `build --emit exe` と `run`）。`test`・`bench` の runner には入れない（D5）。ほかの出力と Windows 以外の target では読まない（エラーにしない）。
- 出力のパスがリソースの入力と同じなら `E2003`（`protect_links` と同じ規則）。

### 5. キャッシュ（E12 D7 の改訂。D1）

- リソースはリンク入力（`LinkInputs`）に入れないので、`links.is_empty()` によるキャッシュの停止は起きない。`[native] link` は今どおりキャッシュを止める。
- `build_key` に、まとめた `.res` のバイト列のフィールド `windows-resources` を足す。リソースのないビルドでは足さないので、その鍵は変わらない。
- `.res` はキャッシュを開く前に作る。そのため、`.rc` の資源コンパイラは、キャッシュに当たるビルドでも毎回走る。鍵が出力のバイト列なので、次の 3 つが鍵に入る（正しさの理由）:
  - `.rc` が `#include` したファイルや、参照したアイコンの中身。
  - 資源コンパイラの版の違い。
  - `INCLUDE` の違い（`KEY_ENVIRONMENT` に既にある）。

### 6. 診断

| 条件 | コード | kind | メッセージの例 |
| --- | --- | --- | --- |
| パスが空・絶対、`version` の形、`resources` の拡張子、`[windows]` の値の形 | `E0002` | `invalid_windows_setting`（W09 の新しい kind） | `windows paths must be nonempty relative paths`、`version must be MAJOR.MINOR.PATCH or MAJOR.MINOR.PATCH.BUILD with each part from 0 to 65535`、`windows resources must be .rc or .res files` |
| 未知のキー | `E0002` | `unknown_manifest_key`（既存） | `unknown windows key; expected subsystem, manifest, icon, version, or resources` |
| ファイルを読めない | `E2001` | `io_failure`（既存。`io_error`） | `cannot read windows resource 'app.ico': ...` |
| 中身が不正（マニフェストが UTF-8 でない・大きすぎる、`.ico` の見出し・範囲の誤り、`.res` の見出しの誤り） | `E2001` | `windows_resource_invalid`（新しい kind の案） | `'app.ico' is not an icon file: expected type 1 in the ICONDIR header` |
| 資源の重複 | `E2000` | `windows_resource_conflict`（新しい kind の案） | `resource VERSIONINFO 1 (language 1033) is defined by both [windows] version and 'version.rc'; remove one` |
| 資源コンパイラがない | `E2002` | `resource_compiler_missing`（新しい kind の案） | `compiling 'dialogs.rc' needs a resource compiler; set TSUZURI_RC to llvm-rc or rc.exe, or use the bundled toolchain` |
| 資源コンパイラの失敗 | `E2002` | `external_tool_failed`（既存） | 道具の出力を付ける |

## 設計

### `src/winres.rs`（新規。`#[doc(hidden)]` の公開関数をテストが呼ぶ）

```rust
/// One resource of a `.res` file (W10).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resource {
    pub kind: Name,      // type: RT_MANIFEST = 24, RT_ICON = 3, ...
    pub name: Name,
    pub language: u16,
    pub memory_flags: u16,
    pub data_version: u32,
    pub version: u32,
    pub characteristics: u32,
    pub data: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Name { Id(u16), Text(Vec<u16>) }

pub fn manifest(bytes: &[u8]) -> Result<Resource, String>;
pub fn icon(bytes: &[u8]) -> Result<Vec<Resource>, String>;      // the RT_ICON images, then RT_GROUP_ICON 1
pub fn version_info(text: &str, parts: [u16; 4]) -> Resource;
pub fn read(bytes: &[u8]) -> Result<Vec<Resource>, String>;      // a 32-bit .res file
pub fn merge(inputs: Vec<(String, Vec<Resource>)>) -> Result<Vec<Resource>, String>; // renumbering and conflicts (spec 4)
pub fn write(resources: &[Resource]) -> Vec<u8>;
```

- RES の形式: 先頭は 32 バイトの空の見出し（`DataSize` 0、`HeaderSize` 32、型と名前は `0xFFFF 0x0000`）。各資源は次の順に並べる。
  - `DataSize`・`HeaderSize`（どちらも u32）。
  - 型と名前（`0xFFFF` と u16 の序数か、NUL 終端の UTF-16）。
  - 4 バイトの境界の詰め物。
  - `DataVersion`（u32）・`MemoryFlags`（u16）・`LanguageId`（u16）・`Version`（u32）・`Characteristics`（u32）。
  - データと、4 バイトの境界の詰め物。
- `read` は見出しの長さ・境界・データの範囲を検査する。最初の見出しが 32 bit の印（空の見出し）でなければ誤り（16 bit の `.res` は受けない）。
- アイコン: ICONDIR は `reserved == 0`・`type == 1`・`count` が 1 以上。各 ICONDIRENTRY（16 バイト）のデータは、ファイルの範囲の中にあること。RT_ICON のデータは画像のバイト（BMP の DIB でも PNG でも、そのまま）。RT_GROUP_ICON のデータは GRPICONDIR（6 バイト）と、GRPICONDIRENTRY（14 バイト。ICONDIRENTRY の先頭 12 バイトと u16 の `nID`）。
- `VS_VERSIONINFO`: 入れ子のブロック（`wLength`・`wValueLength`・`wType`・NUL 終端の UTF-16 の鍵・4 バイトの境界）を書く。`wValueLength` と詰め物の正確な規則は、参照の `.res`（テスト）に一致させる。
- `merge` は `BTreeMap<(Name, Name, u16), 入力の名前>` で重複を調べる。出力の順は入力の順で、決定的。
- `unsafe` は使わない。整数の変換は `u32::try_from` などで検査し、1 つの入力は 64 MiB までにする（超えれば `windows_resource_invalid`）。

### `src/package.rs`

- `WindowsSettings`（W09）に 4 つのフィールドを足す: `manifest: Option<PathBuf>`・`icon: Option<PathBuf>`・`version: Option<(String, [u16; 4])>`・`resources: Vec<PathBuf>`。パスは `relative_path` で検査し、ルートのパッケージに対して解決するのは `Project` を作るとき（`[native]` と同じ。`cache::real_path` を使う）。
- 版の解析は `Version::parse` の規則（10 進、先頭の 0 なし）に、4 つ目の部分と、各部分が u16 に収まることを足す。

### `src/driver.rs`

- `fn windows_resources(project: &Project, directory: &Path) -> Result<Option<PathBuf>, Diagnostic>`: 仕様 1〜4 の手順で `directory/resources.res` を書き、そのパスを返す。キーがなければ `None` を返す。
- 呼び出すのは `build_complete` で、キャッシュの段の前。条件は `windows_apply` が真で、`target == Native`、`emit == Executable`、`platform.windows()`。
- 結果は 2 か所で使う。
  - `build_key` に `Some(bytes)` として渡す。
  - 実行ファイルのリンクの段で、`links.add_to(&mut clang)` の後に `-x none <path>` を足す。
- 資源コンパイラは、`fn resource_compiler() -> Result<(OsString, Vec<OsString>), Diagnostic>` で、仕様 3 の順に探す。返すのはプログラムと、前置きの引数（`zig` のときは `["rc"]`）。

### `src/cache.rs`

- `build_key` の引数に `resources: Option<&[u8]>` を足し、`Some` のときだけ `hash.field("windows-resources", bytes)` を足す。
- `KEY_ENVIRONMENT` に `TSUZURI_RC` を足すかを決める。鍵は出力のバイト列で決まるので、正しさのためには要らない。足すと、変数を変えたときに必ず作り直す。既定案は「足さない」（D4）。

### 文書の例（packages.md に載せる推奨のマニフェスト）

```xml
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency><dependentAssembly>
    <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0"
      processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/>
  </dependentAssembly></dependency>
  <application xmlns="urn:schemas-microsoft-com:asm.v3"><windowsSettings>
    <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
    <activeCodePage xmlns="http://schemas.microsoft.com/SMI/2019/WindowsSettings">UTF-8</activeCodePage>
    <longPathAware xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">true</longPathAware>
  </windowsSettings></application>
</assembly>
```

## 前提とする他チケットのインターフェース

- [W09](W09-windows-gui-subsystem.md): `crate::package::WindowsSettings`、`Project::windows`、`windows_apply`、`[windows]` の節の解析、E0002 の kind `invalid_windows_setting`。
- [W06](W06-windows-gui-test-harness.md) の仕様 4: `peResources`・`launchWithoutHandles`。未着手なら同じ形で足す（着手条件）。

## 他チケットへ提供するインターフェース

- `crate::winres` の読み書き（`read`・`write`・`merge`）。W05 の DLL の版情報や、W27 の生成器がマニフェストの例を出すときに使える。
- `resource_compiler()` の探し方。

## 実装手順

1. **変更**: `.gitattributes`（`tests/fixtures/windows_resources/** -text`）。**内容**: `node scripts/repo-kb.mjs brief W10`。基準を確かめる。参照の fixture を作る。`tests/fixtures/windows_resources/` に次の 4 つを置き、`llvm-rc /nologo /fo` で `.res` を作ってコミットする。
   - `app.manifest`。
   - `app.ico`（16×16 と 32×32 の 2 枚）。
   - 各キーと同じ内容の `.rc`（`manifest.rc`・`icon.rc`・`version.rc`・`all.rc`）。
   - `dialog.rc`（`DIALOGEX` と、名前 2 の ICON を持ち、振り直しを確かめる）。

   `zig rc` と `rc.exe` の出力が同じバイトであることも確かめて記録する（S1）。**確認**: 記録。
2. **変更**: `src/winres.rs`、`src/lib.rs`、`tests/winres.rs`。**内容**: 書き手と読み手と `merge`。**確認**: `cargo test --locked --test winres`（`running N tests`）。
3. **変更**: `src/package.rs`、`tests/packages.rs`。**内容**: キーの解析。**確認**: 仕様 6 の `E0002` の検査。
4. **変更**: `src/driver.rs`、`src/cache.rs`、`src/main.rs`（`HELP` に `TSUZURI_RC`）、`src/diagnostic_kinds.rs`。**内容**: `windows_resources`・`resource_compiler`・リンク・鍵、新しい kind（GUIDE-LITE §6）。**確認**: T1。`cargo test --locked` の driver のテスト（`resource_compiler` の探す順を、環境変数と一時ディレクトリで確かめる）。
5. **変更**: `tests/windows.mjs`（GUI の段）。**内容**: E2E。**確認**: MSVC と同梱の両方で `node tests/windows.mjs target/debug/tsuzuri.exe --stage gui`。
6. **変更**: 文書。**確認**: `gen-docs`・`check-cli-docs`・`check-docs`・T2・`ci:full` を付けた PR の `windows.yml`・`vscode.yml`（ARM64 を含む）。

## テスト計画

### Rust（`tests/winres.rs`。LLVM 不要）

- `manifest_icon_and_version_match_the_resource_compiler`: `manifest`・`icon`・`version_info` の各出力を `write` した `.res` が、fixture の参照の `.res`（同じ内容の `.rc` を `llvm-rc` でコンパイルしたもの）と同じバイト列であること。各キーの単独と、3 つの組み合わせで確かめる。
- `read_round_trips`: 参照の `.res` を `read` して `write` すると、同じバイト列に戻る。
- `merge_renumbers_icons`: `icon` のキーと `dialog.rc` の参照の `.res` をまとめる。RT_ICON の ID が重ならず、各グループの `nID` が自分の画像を指すこと。
- `merge_rejects_duplicates`: `version` と `version.rc` の組で誤りになる。メッセージに両方の名前がある。
- `invalid_inputs`: 壊れた ICONDIR（`type` 2、範囲外のデータ）、16 bit の `.res`、UTF-8 でないマニフェスト、64 MiB を超える入力は `Err`。
- 決定的であること: 同じ入力から 2 回作って同じバイト列。

### Rust（`src/package.rs`・`tests/packages.rs`・driver のテスト）

- キーの受理と、仕様 6 の `E0002`。
- `resource_compiler` の探す順の 5 段（`TSUZURI_RC`、同梱の zig、Clang の隣の `llvm-rc`、PATH、なし）。ARM64 の配布物の形（zig も `llvm-rc` もない `bin`）で `resource_compiler_missing` になること。
- `build_key`: リソースのないビルドの鍵が、このチケットの前と同じ。資源のバイトが 1 つ違えば鍵が違う。

### E2E（`tests/windows.mjs` の GUI の段。MSVC と同梱、`-O0`・`-O3`）

| 検査 | 期待 |
| --- | --- |
| `manifest`（`activeCodePage` UTF-8）・`icon`・`version` のアプリ | `peResources` が MANIFEST 1・ICON 1〜2・GROUP_ICON 1・VERSIONINFO 1（言語 1033）を持ち、MANIFEST は 1 つだけ。`@trusted extern "GetACP" def get_acp :: unit -> i32u` の値を `launchWithoutHandles` の終了コードで読むと 65001（0xFDE9）。マニフェストなしでは、このマシンの既定の 932 だった（確かめた。`--link` の `.res` で 65001） |
| リソースのキーのないアプリ | `.rsrc` の節がない（今と同じ） |
| 同じビルドを 2 回、次にアイコンのファイルを 1 バイト変えて 1 回 | キャッシュの項目が 1 個、次に 2 個 |
| `resources = ["dialog.rc"]` と `icon` | 通り、RT_ICON が重ならない。同梱は `zig rc`、MSVC は `llvm-rc`（`LLVM_PREFIX`）か `rc.exe` |
| `version` と `version.rc` | `E2000`（`windows_resource_conflict`） |
| ARM64 の同梱で `TSUZURI_RC` なし・PATH に資源コンパイラなし | `.rc` のときは `E2002`（`resource_compiler_missing`）。`TSUZURI_RC=$LLVM_PREFIX/bin/llvm-rc.exe` で通る。`manifest`・`icon`・`version` だけなら道具なしで通る |
| `[windows]` のキーのあるプロジェクトの `tsuzuri test` と `--target wasm32` のビルド | 通る（読まない） |

## ドキュメント

- `organizing-tsuzuri/packages.md` の「[windows] セクション」（W09 が作る）に、次の 4 つを足す。
  - 4 つのキーと、版情報の中身。
  - 推奨のマニフェストの例。
  - 既定のマニフェストがないこと。マニフェストがないと Common Controls 6・DPI・UTF-8 のコードページが既定のままになる。
  - キャッシュの扱い。
- `compiler/option.md` の「環境変数」の表に `TSUZURI_RC` を足す（既定の名前・同梱の名前（`zig rc`）・用途）。
- `compiler/platforms.md`: Windows の行に、リソースと、ARM64 の配布物には資源コンパイラがないこと。
- `compiler/native-interop.md` の「リンク入力」: `.res` を `[native] link` で渡すとキャッシュが止まるので、`[windows]` を使う、と書く。
- `compiler/diagnostics.md`（`gen-docs`）、`README.md`（環境変数を README で挙げる所があれば）、`docs/language.md`（マニフェストの節）、`docs/architecture.md`（キャッシュの鍵の説明に `windows-resources`）。

## 受け入れ条件

- [ ] 仕様 1〜6 のとおりに動く（Rust と E2E）。
- [ ] Rust の書き手の出力が、資源コンパイラの参照の `.res` とバイト単位で一致する。
- [ ] リソースのキーのないビルドで、次が変わらない（W09 の `BuildOptions` の変化を除く）: リンクの引数・キャッシュの鍵・IR・exe。
- [ ] `[native] link` のあるビルドは、今どおりキャッシュを使わない。
- [ ] リソースのあるビルドはキャッシュに当たり、資源のバイトが変われば作り直す。
- [ ] `test`・`bench` の runner と、Windows 以外の target のビルドは、キーを読まない。
- [ ] `ci:full` を付けた PR で、`windows.yml` と `vscode.yml` の Windows の job（x64 と ARM64）が通る。
- [ ] `gen-docs --check`・`check-cli-docs`・変更したページの `check-docs`・T2 が通る。

## 落とし穴

- **アイコンの ID の衝突**: rc は `.rc` ごとに RT_ICON を 1 から振る。振り直さずに連結するとリンクに失敗する。このマシンで、`1 ICON` を持つ 2 つの `.res` を連結して確かめた。link.exe は `CVT1100: duplicate resource. type:ICON, name:1, language:0x0409`（`LNK1123`）で失敗した。lld-link は `duplicate resource: type ICON (ID 3)/name ID 1/language 1033` と、GROUP_ICON の同じ文で失敗した。
- **`zig rc` の初回**: resinator をビルドするので、初回だけ約 2 秒かかる。書き込み先は zig の大域のキャッシュで、ビルドのディレクトリの外になる。読み取り専用の環境では、`ZIG_GLOBAL_CACHE_DIR` が要りうる（未確認）。
- **`.rc` の相対パス**: `#include` と `ICON "x.ico"` は `.rc` のディレクトリから解決させたい。cwd をそこにし、入力は絶対パスで渡す。
- **`rc.exe` の `INCLUDE`**: `#include <windows.h>` は `INCLUDE` で決まる。`llvm-rc` は前処理に Clang を探す。どちらも出力のバイトが鍵なので、キャッシュは正しい。
- **既定のマニフェストを足さない**: link.exe の `/MANIFEST` を足すと、マニフェストが 2 つになる。リンクの引数は変えない。
- **fixture の改行**: リポジトリに `.gitattributes` がなく、`vscode.yml` には `windows.yml` の「Keep LF line endings」の段がない。runner の既定の `core.autocrlf=true` では、`.manifest`・`.rc` が CRLF で checkout される（ci.instructions.md）。そうなると、マニフェストのバイトを埋めた出力が、LF のまま作った参照の `.res` と一致しない。`.gitattributes` に `tests/fixtures/windows_resources/** -text` を書く。
- **E12 D7 は狭く改める**: `LinkInputs` に `.res` を入れると、`links.is_empty()` でキャッシュが止まり、改訂の意味がなくなる。

## 対象外

- ARM64 の配布物への `llvm-rc` の同梱（配布物の大きさを測ってから、別に決める）。
- 署名（`signtool`）とタイムスタンプ。
- MUI・多言語の資源（言語は 0x0409 に固定）、16 bit の `.res`。
- `--emit shared`（W05）と `test`・`bench` の runner へのリソースの埋め込み。
- マニフェストの XML の妥当性の検査。OS が起動時に検査し、誤りがあれば起動に失敗する（Windows の side-by-side の仕様。終了の仕方はここでは確かめていない）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | E12 D7 を、リソースについてだけ改める。まとめた `.res` のバイト列を鍵に入れて、キャッシュを使う。`[native] link` は今どおり使わない | D7 の理由「入力の内容を知らない」は、コンパイラ自身が作る `.res` には当たらない。出力のバイト列を鍵にすると、`.rc` の `#include` も道具の版も漏れずに入る。費用は資源コンパイラを毎回動かすことだけ | 要承認（承認前は該当する段に着手しない） |
| D2 | `[windows]` に `manifest`・`icon`・`version`・`resources` を足し、環境変数 `TSUZURI_RC` を足す。D-55 に登録する | アプリの性質なのでマニフェストに書く。`TSUZURI_RC` は、`TSUZURI_CLANG` などと同じ道具の選び方 | 要承認（承認前は該当する段に着手しない） |
| D3 | 既定のマニフェスト・アイコン・版情報は入れず、明示したキーからだけ作る。版情報の文字列も `[package]` から補わない | 既定のマニフェストは実行時の挙動を変える（Common Controls の版・DPI・コードページ・UAC）。既存のプログラムの exe と挙動を変えない。GUIDE §1 規則 2（暗黙の既定値を置かない） | 要承認（承認前は該当する段に着手しない） |
| D4 | manifest・icon・version は Rust で `.res` を書く。`.rc` は資源コンパイラにかける。全部を Rust で 1 つの `.res` にまとめ、1 つのリンク入力にする。`TSUZURI_RC` は鍵に入れない | 3 つのキーは道具なしで、どのツールチェーンでも同じバイトになる。まとめれば、リンカーが複数の `.res` をどう扱うかに頼らず、重複を道具によらない文で報告でき、鍵も 1 つのバイト列になる。却下した案: すべてを `.rc` に生成して道具にかける（ARM64 の配布物で manifest すら作れなくなる） | 既定案（実装者はこの案に従う） |
| D5 | 埋め込む範囲は `build --emit exe` と `run`（W09 の `windows_apply`）。`test`・`bench` の runner には入れない | runner はコンパイラが作る内部のプログラムである。マニフェストの `requireAdministrator` などが CI の runner を止めうる。`run` は、アプリと同じ実行時の挙動にするため入れる | 既定案（実装者はこの案に従う） |
| D6 | 資源の言語は 0x0409、`MemoryFlags` は資源コンパイラの既定（manifest・version 0x0030、icon 0x1010、group 0x1030）に合わせる | 3 つの資源コンパイラの出力と同じバイトにすると、独立した参照でテストできる | 既定案（実装者はこの案に従う） |
| D7 | ARM64 の同梱では、`.rc` は `TSUZURI_RC` がなければ `E2002` にする。manifest・icon・version は道具なしで動く | 配布物に資源コンパイラがない。黙って飛ばさない（GUIDE §1 規則 2） | 既定案（実装者はこの案に従う） |
