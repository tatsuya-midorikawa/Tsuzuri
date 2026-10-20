# 対応プラットフォームとクロスコンパイル

コンパイラ、CLI 配布物、VS Code 拡張、およびネイティブ出力の対応環境は、次の 5 つだけです。WebAssembly（`wasm32`、`wasm64`）への出力は引き続き対応し、[WebAssembly への出力](webassembly.md) で扱います。

| OS | CPU | CLI・拡張の配布名 | 実行を検証する CI runner |
| --- | --- | --- | --- |
| Windows | x64 | `win32-x64` | `windows-2022` |
| Windows | Arm64 | `win32-arm64` | `windows-11-arm` |
| Linux | x64 | `linux-x64` | `ubuntu-24.04` |
| Linux | Arm64 | `linux-arm64` | `ubuntu-24.04-arm` |
| macOS | Arm64（Apple シリコン） | `darwin-arm64` | `macos-15` |

Linux は glibc と musl、Windows の出力は MSVC と MinGW を区別するため、`--target` の native の名前は 9 個です。これは対応環境を増やすものではありません。Linux の配布物と VS Code の拡張ホストは glibc が必要です。

macOS x64（Intel Mac）、32-bit、RISC-V、FreeBSD、Android、iOS、OS を持たない target は対象外です。これらを `--target` に指定すると `E2000` で拒否します。対応環境以外でコンパイラ自体をビルドすると、Rust のコンパイルエラーになります。

## この記事のポイント

- `--target native` はビルド機向けです。別の対応環境向けに作るときは、下の表の名前を渡します。
- 言語の意味は target で変わりません。整数の折り返し、丸め、NaN、符号付きゼロ、評価順序、トラップ、所有権は同じです。
- 実行ファイルのリンクには、その target の C ヘッダー、ライブラリ、リンカーが要ります。同梱ツールチェーンは、MSVC 以外の 7 個の target をクロスリンクできます。
- ビルド機で動かせない target の `run` と `test` は、`TSUZURI_RUNNER` に実行器（qemu-user、Wine など）を指定したときだけ動きます。コンパイラは実行器や SDK を取得も導入もしません。
- CI は、macOS・Linux の 3 つの job と Windows の 2 つの job で、対応するホストの native の `-O0`・`-O3` を検証します。

## target の指定

```sh
# 同梱ツールチェーンで、Arm64 Linux 向けの静的な実行ファイルを作る
tsuzuri build app --target aarch64-unknown-linux-musl -o app-linux-arm64
# Arm64 Windows 向けのオブジェクトを作る
tsuzuri build lib --target aarch64-pc-windows-msvc --emit object -o lib.obj
```

`--target native|wasm32|wasm64|TRIPLE` は、`build`、`run`、`test`、`bench` が受け付けます（`run` と `bench` は native の target だけです）。ほかのサブコマンドの扱いは [コンパイラ オプション](option.md#どのサブコマンドが受け付けるか) の表にあります。

- `native` はビルド機向けです。Clang へ `--target` を渡しません（Windows のホストだけは、ホストの CPU に合う MSVC の target を渡します）。
- `TRIPLE` は下の表の名前です。大文字と小文字を区別し、`arm64-apple-darwin` や `x86_64-linux-gnu` のような別名は受け付けません。ビルド機と同じ triple を名指しした場合も、Clang へ target を渡します。IR は `--target native` と同じですが、macOS の最低 OS 版が 12.0 に決まるなど、出力が Clang の既定に左右されません。
- `wasm32-unknown-unknown` と `wasm64-unknown-unknown` は、`wasm32` と `wasm64` の別名です。
- 知らない名前と対象外の target は、対応する名前の一覧を添えた `E2000` です。32-bit の名前には、native の IR が 64-bit のポインターを前提にする理由も添えます。

```text
<command line>:1:1: error[E2000]: unknown target 'i686-pc-windows-msvc'; 32-bit native targets are not supported because native code generation assumes 64-bit pointers; use native, wasm32, wasm64, or one of aarch64-apple-darwin, aarch64-unknown-linux-gnu, x86_64-unknown-linux-gnu, aarch64-pc-windows-msvc, x86_64-pc-windows-msvc, aarch64-unknown-linux-musl, x86_64-unknown-linux-musl, aarch64-pc-windows-gnu, x86_64-pc-windows-gnu
```

この終了コードは 2 です。一覧は `tsuzuri --help` にも出ます。

## target の一覧と tier

| tier | 意味 |
| --- | --- |
| 1 | CI がその target と同じ環境で `-O0` と `-O3` の `build`、`run`、`test` を実行する |
| 2 | CI がビルドして出力のヘッダーを検査する。実行は表に書いた方法だけ |

次の表が tier の記録です。[targets.yml](../../../.github/workflows/targets.yml) は `macos-15`、`ubuntu-24.04`、`ubuntu-24.04-arm` の 3 job、[windows.yml](../../../.github/workflows/windows.yml) は `windows-2022`、`windows-11-arm` の 2 job です。macOS・Linux は `tests/targets.mjs`、Windows は `tests/windows.mjs` で、`examples/tasks` と `examples/control` の実行、`run` と `test`、`--target native` との出力の一致を検査します。

| CLI 名 | Clang の `--target` | tier | 確認 |
| --- | --- | --- | --- |
| `aarch64-apple-darwin` | `arm64-apple-macosx12.0.0` | 1 | 実行（`macos-15`）、リンク（Linux の 2 job） |
| `aarch64-unknown-linux-gnu` | `aarch64-unknown-linux-gnu` | 1 | 実行（`ubuntu-24.04-arm`）、リンク（ほかの 2 job） |
| `x86_64-unknown-linux-gnu` | `x86_64-unknown-linux-gnu` | 1 | 実行（`ubuntu-24.04`）、リンク（ほかの 2 job） |
| `aarch64-pc-windows-msvc` | `aarch64-pc-windows-msvc` | 1 | 実行（`windows-11-arm`）、オブジェクト（macOS・Linux の 3 job） |
| `x86_64-pc-windows-msvc` | `x86_64-pc-windows-msvc` | 1 | 実行（`windows-2022`）、オブジェクト（macOS・Linux の 3 job） |
| `aarch64-unknown-linux-musl` | `aarch64-unknown-linux-musl` | 2 | リンク（3 job）。Linux Arm64 は直接、Linux x64 は qemu-user で実行 |
| `x86_64-unknown-linux-musl` | `x86_64-unknown-linux-musl` | 2 | リンク（3 job）。Linux x64 は直接、Linux Arm64 は qemu-user で実行 |
| `aarch64-pc-windows-gnu` | `aarch64-w64-windows-gnu` | 2 | リンク（macOS・Linux の 3 job） |
| `x86_64-pc-windows-gnu` | `x86_64-w64-windows-gnu` | 2 | リンク（macOS・Linux の 3 job） |

[vscode.yml](../../../.github/workflows/vscode.yml) は、5 つのホストで CLI archive と VSIX を作成し、展開・インストールした配布物、LSP、Async、Net を検証します。Windows の配布物は MinGW を使い、x64 は LLVM の MSVC でも Async・Net と PDB を検査します。Windows Arm64 の拡張は編集・診断・ビルド・実行・テストに対応しますが、同梱 CodeLLDB がないためソースデバッグは未対応です。

> [!NOTE]
> tier は CI の job が検査する範囲です。job の成功は、その run の結果で確認してください。別の target や未実装の機能まで保証するものではありません。

## OS ごとの違い

| OS | target | 実行ファイル | オブジェクト | 共有ライブラリ | OS API | `Net` と `Async.block_on` |
| --- | --- | --- | --- | --- | --- | --- |
| macOS Arm64 | `aarch64-apple-darwin` | なし | `.o` | `.dylib` | 受け付ける | 受け付ける |
| Linux x64・Arm64（glibc、musl） | `*-unknown-linux-*` | なし | `.o` | `.so` | 受け付ける | 受け付ける |
| Windows x64・Arm64（MinGW、MSVC） | `*-pc-windows-*` | `.exe` | `.obj` | なし（`E2000`） | `E2002` | 受け付ける |

「OS API」は `File`、`Dir`、`Env`、`Time`、`Random`、`Process` です。Windows 向けの実装がないため、これらに到達するビルドは、ランタイムの C をコンパイルする前に `E2002` です。対応するホストでも、すべての API が使えるという意味ではありません。

`-g` のデバッグ情報は、Mach-O の実行ファイルと共有ライブラリの dSYM を `dsymutil` が、MSVC の実行ファイルの PDB を MSVC のリンカーが作るので、それぞれ macOS と Windows のホストだけです。ほかのホストで付けると `E2000` です。オブジェクトと LLVM IR の DWARF は、どの対応ホストでも出ます。MSVC の target は CodeView も付けます（同梱の `tsuzuri-clang` は MinGW でリンクするので DWARF だけです）。

## 実行ファイルをリンクする

実行ファイルは、その target の C ヘッダー、C ライブラリ、リンカーが揃って初めて作れます。同梱ツールチェーンを使うか、システムの Clang に `TSUZURI_SYSROOT` で渡します。

**同梱ツールチェーン。** 配布物の `tsuzuri-clang` は、`--target` を Zig の target に写します。クロスリンクできるのは、Linux の glibc と musl（各 x64・Arm64）、MinGW の Windows（x64・Arm64）、macOS Arm64 の 7 個です。

- Windows 以外のホストで MSVC の target をリンクしようとすると、`E2002` で `pc-windows-gnu` を勧めます。Windows の launcher は従来どおり MSVC の指定を MinGW でリンクします。Windows Arm64 の配布物は、Zig ではなく同梱 LLVM-MinGW を使います。
- `TSUZURI_SYSROOT` は受け付けず、`E2002` です。同梱ツールチェーンが自分の C ライブラリを持つからです。
- `--target native` は Linux で musl の静的リンクです。`*-unknown-linux-gnu` を名指しすると glibc の動的リンクで、glibc の版は同梱の Zig が決めます。

**システムの Clang と `TSUZURI_SYSROOT`。** 名前を付けた target のランタイムの C のコンパイルとリンクへ渡します。Linux と MinGW は `--sysroot=DIR`、macOS は `-isysroot DIR` です。`--target native` は読みません。

- Clang は、target の CPU のコード生成と、target の形式のリンカーを持つ必要があります。ビルド機と違う target には `-fuse-ld=lld` を付けるので、LLD が必要です。例外は、macOS のホストで Mach-O を作るときで、Apple のリンカーを使います。ビルド機の target でも `TSUZURI_LINKER=lld` で選べます（[環境変数](option.md#環境変数)）。
- MSVC の target に `TSUZURI_SYSROOT` を付けると `E2000` です。Windows SDK と CRT は、Developer PowerShell の `INCLUDE` と `LIB` から読みます。
- Clang がコンパイラの出す IR を読めなければ `E2002` です。CI は LLVM 21 を使います。

```sh
export TSUZURI_CLANG=/usr/bin/clang
export TSUZURI_SYSROOT=/opt/sysroots/linux-arm64
tsuzuri build app --target aarch64-unknown-linux-gnu -o app-linux-arm64
```

Clang の実行が失敗すると、`E2002` の末尾に、その target が要るものを添えます。

```text
cross-compiling to aarch64-unknown-linux-gnu needs a Clang with the aarch64 backend, LLD, and a sysroot for aarch64-unknown-linux-gnu: use the bundled toolchain, or set TSUZURI_SYSROOT to one; native runtime requires Clang and the platform C/OS SDK headers
```

ビルドキャッシュは triple と `TSUZURI_SYSROOT` を区別します。target か sysroot を変えると、前のビルドの成果物は返りません。

## ほかの target の実行ファイルを動かす

ビルド機で動かせない target の `run` と `test` は、`TSUZURI_RUNNER` のコマンドへ実行ファイルのパスを渡します。値は空白で区切ったプログラム名と引数で、シェルの展開も引用の解釈もしません。パスに空白がある場合は、空白のない場所にラッパーを置いてください。

```sh
# Arm64 の静的な musl の実行ファイルを、x64 Linux で動かす
TSUZURI_RUNNER=qemu-aarch64 tsuzuri run app --target aarch64-unknown-linux-musl
```

- 実行器がなければ、ビルドの前に `E2000` です。起動できなければ、実行器の名前を添えた `E2001` です。
- `test --coverage`、`test -g`、`bench` は、ビルド機で動かせない target には使えず、`E2000` です。
- コンパイラは実行器を取得も導入もしません。CI の Linux の 2 job は、qemu-user で静的な musl の別 CPU のプログラムを実行します。Wine による実行は検証していません。

## target で決まるもの

`--target native` の IR は、従来と同じです。名前を付けた target では、次の部分がビルド機ではなく target で決まります。

- Windows の公開関数の `dllexport` と CRT の名前への書き換え、COFF 用の allocator。
- `@cpu` の版。Linux x64 は `sse4.2`、`avx2`、`avx512`、Linux Arm64 は `sve`、`sve2`、ほかは基準の版だけです。
- `Math.fma`。Arm64 はハードウェアの命令、x64 は正しく丸めるソフトウェアのルーチンで、結果のビットは同じです。
- MSVC の COFF の時刻。`-mno-incremental-linker-compatible` で、同じ入力から同じオブジェクトにします（Windows のホストの `--target native` は従来どおり渡しません）。

`--cpu native` はビルド機の命令セットを使うので、別の target では `E2000` です。WGSL と SPIR-V の出力は、`--target` と併用できません。

## 診断

| コード | 条件 | メッセージの要点 |
| --- | --- | --- |
| `E2000` | 対象外・不明の `--target`（macOS x64、Android、iOS、FreeBSD、RISC-V、bare-metal を含む） | `unknown target '<value>'; use native, wasm32, wasm64, or one of <一覧>` |
| `E2000` | 32-bit の `--target` | 同じ一覧と `32-bit native targets are not supported because native code generation assumes 64-bit pointers` |
| `E2000` | 別 target の `run`・`test` に実行器がない | `cannot run <triple> programs on this machine; ...`（`test` は `tests`） |
| `E2001` | 実行器を起動できない | `cannot start the runner of TSUZURI_RUNNER '<program>': ...` |
| `E2000` | 別 target の `test --coverage`、`test -g`、`bench` | `test coverage needs a target this machine runs; ...` / `debugging a test needs a target this machine runs; ...` / `tsuzuri bench measures the machine it runs on ...` |
| `E2000` | `--cpu native` と別 target | `'--cpu native' requires the host target; use '--cpu generic' when cross-compiling to <triple>` |
| `E2000` | MSVC の `TSUZURI_SYSROOT` | `TSUZURI_SYSROOT does not apply to <triple>: ...` |
| `E2000` | ホストにない dsymutil か MSVC のリンカーが要る `-g` | `--debug-info for <triple> needs a macOS host: ...` / `... needs a Windows host: ...` |
| `E2002` | Windows 以外で同梱ツールチェーンに MSVC を指定 | `the bundled toolchain links MinGW, not the MSVC runtime of <value>; build <arch>-pc-windows-gnu instead` |
| `E2002` | 同梱ツールチェーンに sysroot を指定 | `the bundled toolchain brings its own C library and cannot use a sysroot; ...` |
| `E2002` | Windows の OS API | `the File, Dir, Env, Time, Random, and Process operating-system APIs are not supported on Windows yet (G10); build on macOS or Linux` |

引数の誤りは終了コード 2、ビルドや実行中の失敗は 1 です。実行ファイルの OS API の検査は、入口・`export def`・`Drop` から到達する関数だけが対象です。未使用の公開関数だけが Windows の OS API を使っても、実行ファイルには出さないので拒否しません。`--emit object` と `--emit llvm` は公開関数をすべて出すので拒否します（[出力とターゲット](option.md#出力とターゲット)）。

## 既知の制限

- Windows の `--emit shared` と OS API は未対応です。MSVC の実行ファイルを Windows 以外のホストでリンクする方法は検証していません。
- 同梱ツールチェーンは、Mach-O の `--emit object` で C ランタイムを含むものを作れません。Windows の COFF のランタイム埋め込み object も `E2002` です。ランタイムを要しない object と LLVM IR は作れます。
- Windows x64 の `i128`・`i128u` の `/` と `%` は正しい値になりません。LLVM が `__udivti3` の系列を、オペランドをポインターで渡して XMM0 で返す規約で呼ぶのに対し、連結する `runtime/wasm.ll` は `i128` を 2 レジスタで受けるためです（assembly の読解による結論）。整数の表示と `Parse` は `i128` の除算を使わず、影響しません。
- プロファイル誘導最適化は、ビルド機の native の `exe` と `object`、wasm32 の module だけです。Windows と別の `--target TRIPLE` は `E2000`、同梱の launcher は `E2002` です。LLVM の Clang を指定してください（[プロファイル誘導最適化](option.md#プロファイル誘導最適化)）。
- `--backend fast` は、feature 付きのビルドで macOS Arm64・Linux x64/Arm64 の native と wasm32 だけです。Windows、名前を付けた `--target TRIPLE`、wasm64 は `E2000` です（[コード生成器](option.md#コード生成器)）。

## 確かめた範囲

- `ci.yml` は、Linux x64 の Clippy と Rust テストに加え、Windows x64・Arm64、Linux Arm64、macOS Arm64 を型検査します。`tests/platforms.mjs` が配布物と CI マトリクスの対応環境を照合します。
- `targets.yml` の 3 job は、ホストの実行、9 triple の object の形式と CPU、同梱ツールチェーンの 7 triple のリンク、Linux での qemu-user を検査します。
- `windows.yml` の 2 job は、LLVM 21.1.8、Visual Studio 2022 の MSVC 14.44、Windows SDK 10.0.26100 で、native と名前を付けた MSVC の target の `-O0`・`-O3`、Win32 のタスク、UTF-8、ファイル保護、object の ABI、トラップ、WASM、Rust テストを検査します。
- `vscode.yml` の 5 job は、ホストに対応する CLI archive と VSIX、そのインストール、LSP、Async、Net を検査します。Windows Arm64 のソースデバッグは対象外です。

## まとめ

- 対応環境は Windows x64/Arm64、Linux x64/Arm64、macOS Arm64 の 5 つだけです。
- `--target` は、`native`、9 個の native triple、`wasm32`、`wasm64` を受け付けます。
- リンクは同梱ツールチェーンか利用者の SDK、別 CPU・OS の実行は明示した `TSUZURI_RUNNER` を使います。
- 言語の意味は target によらず、対応環境ごとの機能の制限と CI の検証範囲は別に扱います。

## 関連項目

- [コンパイラの使い方](usage.md)
- [コンパイラ オプション](option.md)
- [診断メッセージとエラーコード](diagnostics.md)
- [WebAssembly への出力](webassembly.md)
- [ネイティブ連携 (C ABI)](native-interop.md)
- [言語リファレンスの目次](../index.md)
