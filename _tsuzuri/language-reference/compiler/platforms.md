# 対応プラットフォームとクロスコンパイル

`--target` にビルド機とは別の OS や CPU の名前を渡すと、その向けの LLVM IR、オブジェクト、実行ファイル、共有ライブラリを作れます。名前は Rust と同じ綴りで、64-bit・リトルエンディアンの 22 個に閉じています。このページは、その一覧と、CI が各 target をどこまで確かめているか（tier）、リンクと実行の手段、制限をまとめます。WebAssembly（`wasm32`、`wasm64`）は [WebAssembly への出力](webassembly.md) が扱います。

## この記事のポイント

- `--target native` はビルド機向けで、これまでと同じです。ほかは下の表の名前を `--target` に渡します。別名は `wasm32-unknown-unknown` と `wasm64-unknown-unknown`（`wasm32`、`wasm64` と同じ）だけです。
- 32-bit のネイティブ target（i686、armv7 など）は受け付けません。ネイティブの IR が 64-bit のポインターを前提にするからです。
- 言語の意味は target で変わりません。target で変わるのは、Clang に渡す target、OS ごとのランタイム、Windows の ABI の書き換え、`@cpu` の版、`Math.fma` の実装、出力の拡張子です。
- 実行ファイルのリンクには、その target の C ヘッダー、ライブラリ、リンカーが要ります。配布物の同梱ツールチェーンは 12 個の target をリンクでき、ほかは `TSUZURI_SYSROOT` で SDK か sysroot を渡します。
- ビルド機で動かせない target の `run` と `test` は、`TSUZURI_RUNNER` に実行器（qemu-user、Wine、Rosetta の `arch -x86_64` など）を指定したときだけ動きます。コンパイラは何も取得せず、導入もしません。
- tier は、CI の job が確かめる範囲で決めています。CI が同じ target の機械で実行しているのは、4 つの target だけです。

## target の指定

```sh
# 同梱ツールチェーンで、arm64 Linux 向けの静的な実行ファイルを作る
tsuzuri build app --target aarch64-unknown-linux-musl -o app-linux-arm64
# Intel Mac 向けのオブジェクトを作る
tsuzuri build lib --target x86_64-apple-darwin --emit object -o lib.o
```

`--target native|wasm32|wasm64|TRIPLE` は、`build`、`run`、`test`、`bench` が受け付けます（`run` と `bench` は native の target だけです）。ほかのサブコマンドの扱いは [コンパイラ オプション](option.md#どのサブコマンドが受け付けるか) の表にあります。

- `native` は、ビルド機向けです。Clang へ `--target` を渡さず、これまでの出力と変わりません（Windows のホストだけは、従来どおり MSVC の target を渡します）。
- `TRIPLE` は、下の表の名前です。大文字と小文字を区別し、`arm64-apple-darwin` や `x86_64-linux-gnu` のような別名は受け付けません。ビルド機と同じ triple を名指しした場合も、Clang へ target を渡します。IR は `--target native` と同じですが、macOS の最低 OS 版が 12.0 に決まるなど、出力が Clang の既定に左右されません。
- 知らない名前は、一覧を添えた `E2000` です。32-bit の名前には、理由も添えます。

```text
<command line>:1:1: error[E2000]: unknown target 'i686-pc-windows-msvc'; 32-bit native targets are not supported because native code generation assumes 64-bit pointers; use native, wasm32, wasm64, or one of aarch64-apple-darwin, x86_64-apple-darwin, aarch64-unknown-linux-gnu, x86_64-unknown-linux-gnu, aarch64-pc-windows-msvc, x86_64-pc-windows-msvc, aarch64-unknown-linux-musl, x86_64-unknown-linux-musl, riscv64gc-unknown-linux-gnu, riscv64gc-unknown-linux-musl, aarch64-pc-windows-gnu, x86_64-pc-windows-gnu, aarch64-unknown-freebsd, x86_64-unknown-freebsd, aarch64-linux-android, x86_64-linux-android, aarch64-apple-ios, aarch64-apple-ios-sim, x86_64-apple-ios, aarch64-unknown-none, x86_64-unknown-none, riscv64gc-unknown-none-elf
```

この終了コードは 2 です。一覧は `tsuzuri --help` にも出ます。

## target の一覧と tier

tier の意味は次のとおりです。

| tier | 意味 |
| --- | --- |
| 1 | CI がその target の実行ファイルを作り、同じ target の機械で `-O0` と `-O3` の `build`、`run`、`test` を実行する |
| 2 | CI がビルドし、出力のヘッダーを読み戻して検査する。実行は、表に書いたエミュレーターのものだけ |
| 3 | `--target` は受け付けるが、CI がない |

次の表が、tier の唯一の記録です。CI は [targets.yml](../../../.github/workflows/targets.yml) の 4 つの job（`macos-15`、`macos-15-intel`、`ubuntu-24.04`、`ubuntu-24.04-arm`）で、表の「確認」は次のことです。

- 実行: `-O0` と `-O3` で `examples/tasks` と `examples/control` をビルドして実行し、`run` と `test` も実行する（`tests/targets.mjs`）。
- リンク: 同梱ツールチェーンか Apple のツールで実行ファイルをリンクし、ヘッダー（形式、CPU、OS の版、ELF のインタプリタ）を読み戻して検査する。
- オブジェクト: オブジェクトを作り、同じ検査をする。
- qemu-user: Linux の 2 つの job が、静的な musl の実行ファイルをエミュレーターで実行する。

| CLI 名 | Clang の `--target` | tier | 確認 |
| --- | --- | --- | --- |
| `aarch64-apple-darwin` | `arm64-apple-macosx12.0.0` | 1 | 実行（`macos-15`）、リンク（ほかの 3 job） |
| `x86_64-apple-darwin` | `x86_64-apple-macosx12.0.0` | 1 | 実行（`macos-15-intel`）、リンク（ほかの 3 job） |
| `aarch64-unknown-linux-gnu` | `aarch64-unknown-linux-gnu` | 1 | 実行（`ubuntu-24.04-arm`）、リンク（ほかの 3 job） |
| `x86_64-unknown-linux-gnu` | `x86_64-unknown-linux-gnu` | 1 | 実行（`ubuntu-24.04`）、リンク（ほかの 3 job） |
| `aarch64-unknown-linux-musl` | `aarch64-unknown-linux-musl` | 2 | リンク（4 job）。静的な実行ファイルを `ubuntu-24.04-arm` はそのまま、`ubuntu-24.04` は qemu-user で実行 |
| `x86_64-unknown-linux-musl` | `x86_64-unknown-linux-musl` | 2 | リンク（4 job）。静的な実行ファイルを `ubuntu-24.04` はそのまま、`ubuntu-24.04-arm` は qemu-user で実行 |
| `riscv64gc-unknown-linux-gnu` | `riscv64-unknown-linux-gnu` | 2 | リンク（4 job） |
| `riscv64gc-unknown-linux-musl` | `riscv64-unknown-linux-musl` | 2 | リンク（4 job）、qemu-user で実行（Linux の 2 job） |
| `aarch64-pc-windows-gnu` | `aarch64-w64-windows-gnu` | 2 | リンク（4 job） |
| `x86_64-pc-windows-gnu` | `x86_64-w64-windows-gnu` | 2 | リンク（4 job） |
| `aarch64-unknown-freebsd` | `aarch64-unknown-freebsd14` | 2 | リンク（4 job） |
| `x86_64-unknown-freebsd` | `x86_64-unknown-freebsd14` | 2 | リンク（4 job） |
| `aarch64-apple-ios` | `arm64-apple-ios13.0` | 2 | リンク（macOS の 2 job。SDK は `TSUZURI_SYSROOT`）、オブジェクト（Linux の 2 job） |
| `aarch64-apple-ios-sim` | `arm64-apple-ios14.0-simulator` | 2 | 同上 |
| `x86_64-apple-ios` | `x86_64-apple-ios13.0-simulator` | 2 | 同上 |
| `aarch64-pc-windows-msvc` | `aarch64-pc-windows-msvc` | 2 | オブジェクト（4 job） |
| `x86_64-pc-windows-msvc` | `x86_64-pc-windows-msvc` | 2 | オブジェクト（4 job） |
| `aarch64-linux-android` | `aarch64-linux-android21` | 2 | オブジェクト（4 job） |
| `x86_64-linux-android` | `x86_64-linux-android21` | 2 | オブジェクト（4 job） |
| `aarch64-unknown-none` | `aarch64-unknown-none-elf` | 2 | `--freestanding` のオブジェクト（4 job） |
| `x86_64-unknown-none` | `x86_64-unknown-none-elf` | 2 | `--freestanding` のオブジェクト（4 job）。全関数が `noredzone` |
| `riscv64gc-unknown-none-elf` | `riscv64-unknown-none-elf` | 2 | `--freestanding` のオブジェクト（4 job）。`-march=rv64gc -mabi=lp64d` |

Windows の target は、この表の job では実行しません。Windows のホストで実行するのは、[vscode.yml](../../../.github/workflows/vscode.yml) の Windows の job と `tests/windows.mjs` で、`--target native` を使います。名前を付けた Windows の target を Windows のホストで実行した結果は、まだありません。MSVC の target は、Windows のホスト以外でリンクした結果もありません。

> [!NOTE]
> tier は CI の job の定義から決めた表で、job が失敗した target は tier を下げます。実際に実行して確かめた範囲は、[確かめた範囲](#確かめた範囲) にあります。

## OS ごとの違い

| OS | target | 実行ファイル | オブジェクト | 共有ライブラリ | OS API | `Net` と `Async.block_on` |
| --- | --- | --- | --- | --- | --- | --- |
| macOS | `*-apple-darwin` | なし | `.o` | `.dylib` | 受け付ける | 受け付ける |
| iOS | `*-apple-ios*` | なし | `.o` | `.dylib` | `E2002` | `E2002` |
| Linux（glibc、musl） | `*-unknown-linux-*` | なし | `.o` | `.so` | 受け付ける | 受け付ける |
| FreeBSD | `*-unknown-freebsd` | なし | `.o` | `.so` | 受け付ける | `E2002` |
| Android | `*-linux-android` | なし | `.o` | `.so` | `E2002` | `E2002` |
| Windows（MinGW、MSVC） | `*-pc-windows-*` | `.exe` | `.obj` | なし（`E2000`） | `E2002` | 受け付ける |
| なし（bare-metal） | `*-unknown-none*` | 作れない | `.o` | なし | なし | なし |

「OS API」は `File`、`Dir`、`Env`、`Time`、`Random`、`Process` です。「受け付ける」はコンパイラが拒否しないという意味で、動くことは tier が示す範囲でしか確かめていません。`E2002` は、そのコードに到達したビルドが、ランタイムの C をコンパイルする前に出すエラーです。理由は次のとおりです。

- Windows の OS API は、G10 の実行確認が済むまで受け付けません。
- iOS の SDK には、`src/runtime/os.c` が使う `sys/random.h` がありません。
- Android の target は API 21 ですが、Bionic が `getentropy` と `posix_spawn` を持つのは API 28 からです。
- FreeBSD、Android、iOS の `Net` と `Async.block_on` は、ランタイムの C を書いたのが macOS、Linux、Windows の向けだけです。FreeBSD は、C がコンパイルできることまでしか確かめていません。

メッセージは [診断](#診断) にあります。

`-g` のデバッグ情報は、Mach-O の実行ファイルと共有ライブラリの dSYM を `dsymutil` が、MSVC の実行ファイルの PDB を MSVC のリンカーが作るので、それぞれ macOS と Windows のホストだけです。ほかのホストで付けると `E2000` です。オブジェクトと LLVM IR の DWARF は、どのホストでも出ます。

## 実行ファイルをリンクする

実行ファイルは、その target の C ヘッダー、C ライブラリ、リンカーが揃って初めて作れます。同梱ツールチェーンがそれを揃えるか、システムの Clang に `TSUZURI_SYSROOT` で渡します。

**同梱ツールチェーン。** 配布物の `tsuzuri-clang` は、`--target` を Zig の target に写し、Zig が持つ C ライブラリ（glibc、musl、MinGW、FreeBSD、macOS の stub）でコンパイルとリンクをします。sysroot は要りません。リンクできるのは、Linux（glibc と musl の x86-64、AArch64、RISC-V）、MinGW の Windows（x86-64、AArch64）、FreeBSD（x86-64、AArch64）、macOS（x86-64、AArch64）の 12 個です。

- MSVC の target は、MinGW のランタイムで置き換えず、`E2002` で `pc-windows-gnu` を勧めます。
- Android、iOS、bare-metal は、C ライブラリが同梱にありません。オブジェクトと LLVM IR は作れますが、実行ファイルは `E2002` です。
- `TSUZURI_SYSROOT` は受け付けず、`E2002` です。同梱ツールチェーンが自分の C ライブラリを持つからです。
- `--target native` は、これまでどおり Linux で musl の静的リンクです。`*-unknown-linux-gnu` を名指しすると glibc の動的リンクで、glibc の版は同梱の Zig が決めます。

**システムの Clang と `TSUZURI_SYSROOT`。** `TSUZURI_CLANG` がシステムの Clang のとき、名前を付けた target のランタイムの C のコンパイルとリンクに、sysroot を渡します。非 Apple の target は `--sysroot=DIR`、Apple の target は `-isysroot DIR` です。`--target native` は読みません。

- その Clang は、target の CPU のコード生成（RISC-V は Apple の Clang にありません）と、target の形式のリンカーを持つ必要があります。ビルド機と違う target には `-fuse-ld=lld` を付けるので、`ld.lld` が PATH に要ります。例外は、macOS のホストで Mach-O の target を作るときで、Apple のリンカーを使います。
- 別の CPU の macOS 向けは、macOS の SDK がそのまま使えるので、sysroot は要りません。iOS は SDK を渡さないと、Apple の Clang が macOS の SDK で黙ってリンクするので、`-Werror=incompatible-sysroot` で止めて `E2002` にします。
- MSVC と bare-metal の target に `TSUZURI_SYSROOT` を付けると `E2000` です。MSVC のリンカーは、Windows SDK と CRT を `INCLUDE` と `LIB` から読みます。

```sh
export TSUZURI_CLANG=/usr/bin/clang
export TSUZURI_SYSROOT="$(xcrun --sdk iphoneos --show-sdk-path)"
tsuzuri build app --target aarch64-apple-ios -o app-ios
```

Clang の実行が失敗すると、`E2002` のメッセージの末尾に、その target が要るものを添えます。

```text
cross-compiling to aarch64-unknown-linux-gnu needs a Clang with the aarch64 backend, LLD, and a sysroot for aarch64-unknown-linux-gnu: use the bundled toolchain, or set TSUZURI_SYSROOT to one; native runtime requires Clang and the platform C/OS SDK headers
```

どの方法でも、ビルドキャッシュは triple と `TSUZURI_SYSROOT` を区別します。target か sysroot を変えると、前のビルドの成果物は返りません。

## ほかの target の実行ファイルを動かす

`run` と `test` は、ビルド機で動かせる target ならそのまま実行します。ビルド機の target とは、`--target native` と、ビルド機と同じ triple です。そのときは `TSUZURI_RUNNER` を見ません。

ほかの target は、実行器を `TSUZURI_RUNNER` に指定したときだけ動きます。値は、空白で区切った語の並びです。先頭がプログラム、残りがその引数で、実行ファイルのパスが最後に付きます。シェルは通らず、引用符も解釈しません。空の値は未設定です。

```sh
# arm64 の静的な musl の実行ファイルを、x86-64 の Linux で qemu-user に動かさせる
TSUZURI_RUNNER=qemu-aarch64 tsuzuri run app --target aarch64-unknown-linux-musl

# Apple シリコンの Mac で、Rosetta が x86-64 の macOS のテストを動かす
TSUZURI_RUNNER="arch -x86_64" tsuzuri test tests --target x86_64-apple-darwin
```

- 実行器がなければ、ビルドの前に `E2000` です。実行器を起動できなければ、実行器の名前を添えた `E2001` です。
- `test --coverage`、`test -g`（デバッグ用のランナー）、`bench` は、ほかの target には使えません。`E2000` です。
- 実行ファイルのない bare-metal の target は、`run`、`test`、`bench` とも `E2000` です。
- コンパイラは、実行器を取得も導入もしません。`TSUZURI_RUNNER` が指すコマンドを、自分で用意してください。

## OS を持たない target

`aarch64-unknown-none`、`x86_64-unknown-none`、`riscv64gc-unknown-none-elf` は、`--freestanding --allocator host` と、`--emit object`、`llvm`、`header` の組み合わせだけを受け付けます。ほかは `E2000` で、実行ファイルは作りません。リンクは、利用者のリンカースクリプトとホストの `tsuzuri_host_alloc`、`tsuzuri_host_free`、`tsuzuri_host_realloc` で行います（[リンクと allocator](option.md#リンクと-allocator)）。

- x86-64 は、割り込みがスタックの下を壊さないよう、すべての関数を `noredzone` にします。Clang は IR の入力に `-mno-red-zone` を効かせないので、IR の `define` の行に付けます。
- RISC-V は `-march=rv64gc -mabi=lp64d` です。Clang の既定は、F と D の拡張を持たないソフト浮動小数点です。
- コードは位置独立（`-fPIC`）です。

## target で決まるもの

生成する IR は、ビルド機ではなく target で決まる部分があります。`--target native` の IR は、これまでと同じです。

- Windows の target は、公開関数の `dllexport` と CRT の名前への書き換え、COFF 用の allocator を、ビルド機を問わず適用します。`--emit llvm` も同じです。
- `@cpu` の版は、target が実行時に選べるものだけを作ります。x86-64（Windows 以外）は `sse4.2`、`avx2`、`avx512`、AArch64 の Linux と Android は `sve`、`sve2`、それ以外は基準の版だけです。
- `Math.fma` は、AArch64 の target ではハードウェアの命令、それ以外では正しく丸めるソフトウェアのルーチンです。結果のビットは同じです。
- `--cpu native` は、ビルド機の命令セットで最適化します。別の target には使えず、`'--cpu native' requires the host target; use '--cpu generic' when cross-compiling to <triple>` の `E2000` です。
- WGSL と SPIR-V の出力は、`--target` と併用できません。

## 診断

メッセージの `<triple>` は、CLI の名前です。引数の誤りは終了コード 2、ビルドや実行の途中で見つかるものは終了コード 1 です。

| コード | 条件 | メッセージの要点 |
| --- | --- | --- |
| `E2000` | `--target` の値が一覧にない | `unknown target '<value>'; use native, wasm32, wasm64, or one of <一覧>`（32-bit の名前は、`32-bit native targets are not supported because native code generation assumes 64-bit pointers; ` が付く） |
| `E2000` | ほかの target の `run` に `TSUZURI_RUNNER` がない | `cannot run <triple> programs on this machine; run the program on a <triple> machine, set TSUZURI_RUNNER to a command that runs <triple> executables (such as qemu-user, Wine, or an emulator wrapper), or use 'tsuzuri build --target <triple>'` |
| `E2000` | ほかの target の `test` に `TSUZURI_RUNNER` がない | `cannot run <triple> tests on this machine; run 'tsuzuri test' on a <triple> machine, set TSUZURI_RUNNER to a command that runs <triple> executables ...` |
| `E2001` | `TSUZURI_RUNNER` のプログラムを起動できない | `cannot start the runner of TSUZURI_RUNNER '<program>': ...` |
| `E2000` | ほかの target の `test --coverage`、`test -g`、`bench` | `test coverage needs a target this machine runs; ...` / `debugging a test needs a target this machine runs; ...` / `tsuzuri bench measures the machine it runs on and cannot run <triple> benchmarks here; ...` |
| `E2000` | bare-metal の target の `run`、`test`、`bench` | `<triple> has no operating system: ...` / `a bare-metal target has no operating system to run a benchmark` |
| `E2000` | bare-metal の target が `--freestanding --allocator host` の object・llvm・header でない | `<triple> has no operating system: build it with --freestanding --allocator host and --emit object, llvm, or header` |
| `E2000` | `--cpu native` と、ほかの target | `'--cpu native' requires the host target; use '--cpu generic' when cross-compiling to <triple>` |
| `E2000` | MSVC と bare-metal の target の `TSUZURI_SYSROOT` | `TSUZURI_SYSROOT does not apply to <triple>: ...` |
| `E2000` | ホストにない dsymutil か MSVC のリンカーが要る `-g` | `--debug-info for <triple> needs a macOS host: its dSYM comes from dsymutil` / `... needs a Windows host: its PDB comes from the MSVC linker` |
| `E2002` | 同梱ツールチェーンに、MSVC の target、sysroot、C ライブラリがない target | `the bundled toolchain links MinGW, not the MSVC runtime of <value>; build <arch>-pc-windows-gnu instead` / `the bundled toolchain brings its own C library and cannot use a sysroot; unset TSUZURI_SYSROOT, or set TSUZURI_CLANG to a Clang that uses it` / `the bundled toolchain has no C library for <clang target>; set TSUZURI_CLANG to a Clang with that target's SDK` |
| `E2002` | iOS で SDK がない | `using sysroot for 'macOS ...' but targeting 'arm64-apple-ios13.0.0'` と、`the SDK for <triple>: set TSUZURI_SYSROOT to it` |
| `E2002` | Windows・iOS・Android の OS API | `the File, Dir, Env, Time, Random, and Process operating-system APIs are not available for <triple>: src/runtime/os.c is written for the headers of macOS, Linux, and FreeBSD`（Windows は `... are not supported on Windows yet (G10); build on macOS or Linux`） |
| `E2002` | FreeBSD・Android・iOS の `Net` | `the Net socket API is only available for macOS, Linux, and Windows targets; <triple> has no implementation of it` |
| `E2002` | FreeBSD・Android・iOS の `Async.block_on` | `Async.block_on is only available for macOS, Linux, and Windows targets; use Async.run for <triple>` |

## 既知の制限

- 32-bit のネイティブ target は、受け付けません。native の IR は 64-bit のポインターを前提にします。
- Android は、オブジェクトと LLVM IR までです。NDK の sysroot を `TSUZURI_SYSROOT` に渡して実行ファイルをリンクできるかは、確かめていません。Android の OS API、`Net`、`Async.block_on` は受け付けません。
- iOS の実行ファイルは、SDK を `TSUZURI_SYSROOT` に渡し、macOS のホストでリンクします。実機とシミュレーターでは動かしていません。
- MSVC の target の実行ファイルは、Windows のホストの MSVC のリンカーが作ります。ほかのホストで `INCLUDE` と `LIB` に Windows SDK を用意してリンクする方法は、確かめていません。Windows の `--emit shared`、OS API は、G10 が終わるまで受け付けません。
- 同梱ツールチェーンは、Mach-O の `--emit object` でランタイムを含むものを作れません。リンカーが `-keep_private_externs` を受け付けないからです（ランタイムを要しないプログラムのオブジェクトと、LLVM IR は作れます）。Windows の COFF の `--emit object` でランタイムを含むものは、ホストを問わず `E2002` です。
- bare-metal の x86-64 は、`noredzone` を IR の関数に付けるだけで、`-mcmodel` や割り込みの規約までは扱いません。RISC-V の bare-metal は `rv64gc`、`lp64d` に固定です。
- クロスコンパイルで C ライブラリや sysroot を自動で取得することはありません。同梱ツールチェーンか、`TSUZURI_SYSROOT` だけです。

## 確かめた範囲

実行まで確かめたのは、開発に使った arm64 の macOS の `aarch64-apple-darwin` だけです（`-O0` と `-O3` の `build`、`run`、`test`、`--target native` との出力の一致）。ほかは次のとおりです。

- この機械でリンクして、ヘッダーを検査した: `x86_64-apple-darwin`（Rosetta がなく実行できない）、Linux の glibc と musl、RISC-V Linux、FreeBSD、MinGW の Windows（同梱の Zig）、iOS の 3 target（Xcode の SDK）。
- この機械でオブジェクトまで作った: MSVC の Windows、Android、bare-metal。
- 実行器のフックは、偽の実行器のスクリプトで確かめた。qemu-user、Rosetta、Wine を使った実行は、この機械では確かめていない。CI の Linux の job が、静的な musl の実行ファイルを qemu-user で動かす。
- Linux と Intel Mac は、CI の job が最初に確かめる。

## まとめ

- `--target` に、`native`、`wasm32`、`wasm64`、または 22 個の triple を渡します。32-bit は受け付けません。
- リンクは、同梱ツールチェーンの 12 個か、`TSUZURI_SYSROOT` で渡す SDK と sysroot です。
- ほかの target の実行は、`TSUZURI_RUNNER` に指定した実行器を通したときだけです。
- 言語の意味は変わりません。IR は、Windows の ABI、`@cpu`、`Math.fma`、bare-metal の `noredzone` だけが target で変わります。
- tier は CI の job が確かめた範囲で、この表だけが記録です。

## 関連項目

- [コンパイラの使い方](usage.md)
- [コンパイラ オプション](option.md)
- [診断メッセージとエラーコード](diagnostics.md)
- [WebAssembly への出力](webassembly.md)
- [ネイティブ連携 (C ABI)](native-interop.md)
- [言語リファレンスの目次](../index.md)
