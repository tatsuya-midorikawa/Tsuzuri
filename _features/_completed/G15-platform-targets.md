# G15: 対応ターゲットの拡張とクロスコンパイル

| 項目 | 内容 |
| --- | --- |
| ID | G15 |
| 優先度 | P3 |
| 規模 | XL |
| 依存 | G10, G14, (F13) |
| 後続 | – |
| 状態 | done（Phase 1・2） |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。旧計画の基準は `f8dc655` |
| 実装の基準 | `053bced8`、2026-10-11 |
| 承認 | 全フェーズと `要承認` の判断への包括承認（旧 D8 の CI workflow `targets.yml` を含む）。G10 は Windows の実行の確認だけが未了で `blocked` のまま、旧 D9 のとおり待たずに、Windows の triple を tier 2 にした |
| 改善する劣位 | C/C++ 比: 対応プラットフォームの幅 |
| 利用者向け仕様 | [対応プラットフォームとクロスコンパイル](../../_tsuzuri/language-reference/compiler/platforms.md)、[コンパイラ オプション](../../_tsuzuri/language-reference/compiler/option.md)、[コンパイラの使い方](../../_tsuzuri/language-reference/compiler/usage.md)、[言語仕様](../../docs/language.md#ネイティブのターゲットとクロスコンパイル)、[コンパイラ構成](../../docs/architecture.md) |

## 目的と実装範囲

`--target TRIPLE` で、ビルド機と別の OS・CPU 向けのオブジェクト、実行ファイル、共有ライブラリを作れるようにした。名前は Rust と同じ綴りの閉じた表（64-bit・リトルエンディアンの 22 個）で、32-bit の native は対象外（native の IR は 64-bit のポインターを前提にする）。対応の程度は tier（1 = CI が同じ target の機械で実行、2 = CI がビルドして形式を検査、3 = 受け付けるが CI なし）で示し、tier の記録は言語リファレンスの表 1 か所だけにした。

| フェーズ | 実装 |
| --- | --- |
| Phase 1 | `aarch64`・`x86_64` の `apple-darwin`・`unknown-linux-gnu`・`pc-windows-msvc` と、`wasm32-unknown-unknown`（`wasm64-unknown-unknown`）の別名。`TSUZURI_SYSROOT`。cross のリンク引数、診断、`test --target` の foreign 拒否、キャッシュキー、同梱 shim の target 写像、`tests/targets.rs`・`tests/targets.mjs`、文書、tier |
| Phase 2 | `*-unknown-linux-musl`、`riscv64gc-unknown-linux-gnu`・`musl`、`*-pc-windows-gnu`（MinGW）、`*-unknown-freebsd`、`*-linux-android`、`aarch64-apple-ios`・`aarch64-apple-ios-sim`・`x86_64-apple-ios`、bare-metal の `aarch64-unknown-none`・`x86_64-unknown-none`・`riscv64gc-unknown-none-elf`。foreign 実行の実行器 `TSUZURI_RUNNER`（`run --target` を含む）。CI workflow `.github/workflows/targets.yml` |

## 確定した仕様

### CLI と target の表

- `--target native|wasm32|wasm64|TRIPLE`。`native` は従来どおり（Clang へ `--target` を渡さない）。`TRIPLE` は `Target::Native` と `BuildOptions::triple = Some(..)` になり、ビルド機と同じ triple を名指ししても `Some` のまま Clang へ target を渡す。大文字小文字を区別し、別名は `wasm32-unknown-unknown`・`wasm64-unknown-unknown` だけ。
- `build`・`run`（native の target だけ）・`test`・`bench`（ビルド機の target だけ）が受ける。`--emit bindings-*` は triple を受けて同じバイト列、`--emit shared` は Windows の target 以外で受ける。
- 表は `src/target.rs` の `Triple::ALL`（22 個）。Clang の target は Apple が `arm64-apple-macosx12.0.0`・`x86_64-apple-macosx12.0.0`・`arm64-apple-ios13.0`・`arm64-apple-ios14.0-simulator`・`x86_64-apple-ios13.0-simulator`、MinGW が `*-w64-windows-gnu`、FreeBSD が `*-freebsd14`、Android が `*-android21`、RISC-V が `riscv64-unknown-…`、bare-metal が `*-unknown-none-elf`（RISC-V は `-march=rv64gc -mabi=lp64d` を足す）、残りは Rust と同じ名前。
- `--help` の TRIPLE の一覧と未知 target の診断は表から生成する。`tests/targets.mjs` は自分の表と突き合わせ、表に target を足すと失敗する。

### リンクと実行

- 実行ファイルは、同梱ツールチェーン（`scripts/toolchain/clang.rs` が `--target=<Clang の target>` を Zig の target に写す。Linux の glibc・musl の x86-64・AArch64・RISC-V、MinGW の Windows、FreeBSD、macOS の 12 個）か、システムの Clang と `TSUZURI_SYSROOT`（非 Apple は `--sysroot=DIR`、Apple は `-isysroot DIR` と `-Werror=incompatible-sysroot`）でリンクする。ビルド機と違う target には `-fuse-ld=lld`（macOS のホストの Mach-O を除く）。MSVC と bare-metal では `TSUZURI_SYSROOT` は `E2000`、同梱ツールチェーンでは `E2002`。
- `TSUZURI_RUNNER`: ビルド機で動かせない target の `run`・`test` が、実行ファイルの前に置く前置コマンド（空白で区切る。引用符は解釈しない。空は未設定。ビルド機の target では読まない）。なければビルドの前に `E2000`、起動できなければ実行器の名前を添えた `E2001`。`test --coverage`・`test -g`・`bench` は foreign target で `E2000`。何も取得せず、導入しない。
- ランタイムの選択は target で決める。OS API は macOS・Linux・FreeBSD の target、`Net` と `Async.block_on` は macOS・Linux・Windows の target だけで、ほかは `E2002`（推測してリンクしない）。bare-metal は `--freestanding --allocator host` の `object`・`llvm`・`header` だけで、x86-64 は全関数に `noredzone` を付ける。

### 診断（新規・変更）

| コード | 条件 | メッセージの要点 |
| --- | --- | --- |
| `E2000` | `--target` の値が未知（32-bit は理由を添える） | `unknown target '<value>'; use native, wasm32, wasm64, or one of <表から生成した一覧>`（旧 `target must be 'native' or 'wasm32'` を置き換え） |
| `E2000` | foreign target の `run`・`test` に `TSUZURI_RUNNER` がない | `cannot run <triple> programs on this machine; … set TSUZURI_RUNNER to a command that runs <triple> executables (such as qemu-user, Wine, or an emulator wrapper), or use 'tsuzuri build --target <triple>'`（`test` は `tests`） |
| `E2000` | foreign target の `test --coverage`・`test -g`・`bench`、bare-metal の `run`・`test`・`bench` と、bare-metal の `--freestanding` なし | `test coverage needs a target this machine runs; …`、`debugging a test needs a target this machine runs; …`、`tsuzuri bench measures the machine it runs on and cannot run <triple> benchmarks here; …`、`<triple> has no operating system: …` |
| `E2000` | cross の `--cpu native`、MSVC・bare-metal の `TSUZURI_SYSROOT`、ホストにない `dsymutil`・MSVC のリンカーが要る `-g` | `'--cpu native' requires the host target; use '--cpu generic' when cross-compiling to <triple>`、`TSUZURI_SYSROOT does not apply to <triple>: …`、`--debug-info for <triple> needs a macOS|Windows host: …` |
| `E2001` | `TSUZURI_RUNNER` のプログラムを起動できない | `cannot start the runner of TSUZURI_RUNNER '<program>': …` |
| `E2002` | cross の Clang・リンカーの失敗 | 従来の `'<tool>' failed (…)` の末尾に、target が要るもの（`cross-compiling to <triple> needs a Clang with the <arch> backend[, LLD,] and …`）。同梱ツールチェーンの拒否（MSVC・sysroot・C ライブラリのない target）もここ |
| `E2002` | Windows・iOS・Android の OS API、FreeBSD・Android・iOS の `Net` と `Async.block_on` | `…operating-system APIs are not available for <triple>: src/runtime/os.c is written for the headers of macOS, Linux, and FreeBSD`、`the Net socket API is only available for macOS, Linux, and Windows targets; <triple> has no implementation of it`、`Async.block_on is only available for macOS, Linux, and Windows targets; use Async.run for <triple>` |

### 環境変数

| 変数 | 意味 |
| --- | --- |
| `TSUZURI_SYSROOT` | 名前を付けた target のランタイムの C のコンパイルとリンクに渡す sysroot（Apple は SDK）。キャッシュのキーに入る |
| `TSUZURI_RUNNER` | foreign target の実行器の前置コマンド |
| `TSUZURI_CROSS_LINK=1` | `tests/targets.mjs` のテスト用。同梱ツールチェーンのリンクを必須にする（`TSUZURI_CROSS_CLANG` に `tsuzuri-clang`） |
| `TSUZURI_CROSS_EXECUTE=1` | `tests/targets.mjs` のテスト用。Linux の qemu-user による静的 musl の実行を必須にする |

### tier

tier の記録は [対応プラットフォームとクロスコンパイル](../../_tsuzuri/language-reference/compiler/platforms.md) の表だけ。`.github/workflows/targets.yml` の 4 つの job（`macos-15`・`macos-15-intel`・`ubuntu-24.04`・`ubuntu-24.04-arm`）が確かめる範囲で決めた。tier 1 はホストの 4 triple、tier 2 は残りのすべて（musl は qemu-user で実行、MSVC・Android・bare-metal はオブジェクトまで、iOS は macOS の job で SDK を渡してリンク）、tier 3 は無し。表は CI の最初の実行の結果で見直す。

## 旧計画から外れた判断

| 旧判断 | 確定した判断と理由 |
| --- | --- |
| IR は target に依存しない（旧 D5）。`windows_abi` だけが triple で変わる | 偽だった。HEAD には host の `cfg!` で決まる IR が 3 か所あった: COFF の allocator（`cfg!(windows)`）、`@cpu` の level（`host_levels`）、`Math.fma`（`cfg!(target_arch = "aarch64")`）。`Platform` に尋ねる形にし、IR を作る間は `PlatformScope`（スレッドローカルの guard）が運ぶ。`EmitOptions` に target を足すと `tests/windows.rs` などの 24 か所のリテラルが壊れるため。`tests/windows.rs` が allocator の IR を書き換えていた回避策は残っている（動作は同じ） |
| 6 つの triple、Phase 2 は人間が求めたときだけ | 22 個。musl・RISC-V・MinGW・FreeBSD・Android・iOS・bare-metal も実装し、各 target を手元の Zig と LLVM 21 で C ランタイムの全翻訳単位までコンパイルして確かめた。落とした候補は 32-bit の native だけ |
| `run` は `--target` を受けない。実行器は対象外 | `run --target` を native の target に許し、foreign は `TSUZURI_RUNNER` があるときだけ動かす（偽の実行器のスクリプトで試験）。明示的な opt-in で、何も導入しない |
| 全 cross で `--sysroot` | Apple の Clang は `--sysroot` を無視するので `-isysroot`。iOS の SDK を渡し忘れると Apple の Clang は macOS の SDK で黙ってリンクするので、`-Werror=incompatible-sysroot` で止める |
| bare-metal は F13 の後に検討 | F13 は done。freestanding の object・llvm・header だけに限る。Clang は IR の入力に `-mno-red-zone` を効かせないので、x86-64 は IR の `define` に `noredzone` を付ける。RISC-V は `-march=rv64gc -mabi=lp64d`（既定はソフト浮動小数点） |
| OS API・`Net`・`Async.block_on` は target に従う | macOS・Linux・Windows 以外の target は `E2002`。Android は target が API 21 で、`os.c` が使う `getentropy`・`posix_spawn` は Bionic の API 28 から。レビューで OS API の誤りを直した |
| G14 の shim は `vsc/scripts/clang.rs` | `scripts/toolchain/clang.rs`。MSVC の target は Windows の launcher だけが受け、MinGW でリンクする（従来と同じ）。それ以外のホストでは `pc-windows-gnu` を勧める `E2002`。`--target=` がないときの挙動は変えていない。wasm64 の IR は従来 host の target でコンパイルされていて、今は `wasm64-unknown-unknown` になる（修正） |
| `-fuse-ld=lld` は非 Apple の target | ビルド機と違う target すべて（macOS のホストの Mach-O を除く）。Zig の launcher はこの引数を落とす |
| テストと E2E は `/tmp` | この環境は `/tmp` を禁じたので、scratch は `target/tz-g15/`（`.gitignore` 済み） |
| 文書は `_docs/tools/*` | `_tsuzuri/language-reference/compiler/` の新ページ `platforms.md` と、`option.md`・`usage.md`・`diagnostics.md` ほか |
| `tests/targets.rs` は 5 件 | 11 件（IR、オブジェクトの形式、launcher の表、偽の Clang と実行器の `run`・`test`、ランタイムの拒否、キャッシュ、CodeView、`noredzone`）。`src/target.rs`・`driver.rs`・`main.rs` に 15 件 |
| `toolchain info` に target の表示 | 変えなかった（`tests/toolchain.rs` が行の位置と数を検査している） |

## 受け入れ条件

- `--target` が 22 個の triple と wasm の別名を受け、未知値・foreign の `test`・cross の `--cpu native` を表の文言で拒否する。
- `--target native` の IR・object・Clang と wasm-ld の引数・出力名が基準（`053bced8`）と一致する（下の 308 回の比較）。
- arm64 macOS で `aarch64-apple-darwin` を `build`・`run`・`test` でき、`x86_64-apple-darwin` の実行ファイルと、Linux・Windows・Android・bare-metal のオブジェクト、iOS の実行ファイル（SDK）を作れる。
- 同梱 shim で、Linux・RISC-V・MinGW・FreeBSD・macOS の 12 個の実行ファイルと共有ライブラリをリンクできる。
- `tests/targets.rs`、`src/` の新しいテスト、`tests/targets.mjs`（既定、`TSUZURI_CROSS_LINK=1`）が成功する。
- tier は CI の job の定義だけから決め、実行を確かめていない target を tier 1 と書かない。CI の最初の実行は、この変更の後に人間が行う。

## 実装と検証（2026-10-11）

### 実装したファイル

- `src/target.rs`（新規）: `Os`・`Arch`・`Triple`（22 個）・`Platform`・`PlatformScope`・`parse_target`・`ForeignRunner`。`src/lib.rs`。
- `src/driver.rs`、`src/test_runner.rs`、`src/main.rs`: `BuildOptions`・`TestOptions`・`BenchOptions` の `triple`、`compile_args`・`sdk_args`・`link_args`・`no_sdk_args`・`cross_hint`、ランタイムの選択、foreign の実行、`--target` の解析と `--help`。`src/llvm.rs`・`src/llvm_cpu.rs`・`src/llvm_math.rs`・`src/cache.rs`: target に従う IR と、キャッシュキー（triple と `TSUZURI_SYSROOT`）。
- `scripts/toolchain/clang.rs`: target の写像（`NATIVE_TARGETS`・`resolve`）。
- `tests/targets.rs`・`tests/targets.mjs`（新規）、`.github/workflows/targets.yml`（新規）。
- `README.md`、`docs/architecture.md`、`docs/language.md`、`_tsuzuri/language-reference/` の `compiler/platforms.md`（新規）・`option.md`・`usage.md`・`diagnostics.md`・`directives.md`・`native-interop.md`・`webassembly.md`、`built-in-types-and-modules/` の `os.md`・`net.md`・`matrix.md`・`test.md`・`bench.md`、`async-tasks-and-lazy/async.md`、`index.md`。

### 検証結果

- `cargo fmt --all -- --check`、`cargo clippy --locked --all-targets -- -D warnings`、`sh scripts/check-runtime-includes.sh`、`git diff --check` が成功。
- `cargo test --locked --no-fail-fast -- --skip bounds_computation_syntax_and_expansion` は 94 バイナリで 1104 件が成功（失敗 0。最後の修正の後に全体を実行し直した）。`bounds_computation_syntax_and_expansion` は `RUST_MIN_STACK=2200000` で成功（既知の基準の失敗で、この変更とは無関係）。GUIDE §3.1 の 4 つの回帰テストは各 1 件で成功。
- `PATH=$HOME/.cargo/bin:$PATH cargo check --all-targets --target x86_64-pc-windows-msvc`・`aarch64-pc-windows-msvc`・`x86_64-unknown-freebsd` が成功。Windows の実行は確かめていない（型検査だけ）。
- `tests/e2e.mjs`・`cache.mjs`・`examples.mjs`・`primitives.mjs`・`tasks.mjs`・`os.mjs`・`control.mjs`・`cpu_dispatch.mjs` が成功。`node tests/targets.mjs` は、既定（Apple の Clang は RISC-V のコード生成を持たず 3 件が未確認）、`CI=true` と LLVM 21（実行 1・リンク 4・オブジェクト 17）、`TSUZURI_CROSS_LINK=1` と `node scripts/toolchain/bundle.mjs` が作った同梱ツールチェーン（実行 1・リンク 14・オブジェクト 7、約 6 分。`targets.yml` の macOS の job と同じ構成）で成功。
- 基準（`053bced8`）のバイナリとの比較: 14 個のプログラム（`examples` の 4 つと `tests/fixtures` の 10 個）× 22 の設定（native と wasm32・wasm64 の `-O0`・`-O3`、`--emit` の llvm・object・header・exe・shared、`-g`、`--trap-info`、`--cpu native`、freestanding）を、Clang・wasm-ld・llvm-link・dsymutil の呼び出しの引数列まで比べて 308 回、差分 0（`-g` の実行ファイルと共有ライブラリは基準でも実行ごとに違い、比較から外した 20 件）。
- 手元（arm64 macOS、Apple の Clang 21 と LLVM 21、同梱の Zig 0.16.0）で、22 個すべてのオブジェクトの形式（Mach-O の platform と最低 OS 版、ELF の machine・OSABI・RISC-V の浮動小数点 ABI、COFF の machine）を検査し、12 個を同梱の launcher でリンクして ELF のインタプリタと COFF のヘッダーを検査した。共有ライブラリ、IO・OS・Net のプログラムも 12 個で作り、iOS の 3 target は Xcode の SDK でリンクした。
- `node scripts/check-docs.mjs` は変更した 14 ページで 340 件のリンク、49 個の例、68 回の native の実行が成功。

### レビュー後の修正

独立したコードレビューを 2 回行い、実際の問題は次のとおりだった。

- 1 回目: Android が OS API を受けていた（API 21 では `getentropy`・`posix_spawn` がない）。壊れた `TSUZURI_RUNNER` が「実行ファイルを起動できない」と報告され、実行器の名前が出なかった（`run` と `test`）。launcher の ARM64 Windows の判定が広すぎた。
- 2 回目: 高い確度の問題はなし。中程度の指摘のうち、MSVC の target の `-g` の CodeView がビルド機の Windows かどうかで決まっていた（target で決めるよう直し、`codeview_for`）、MSVC のオブジェクトが実行ごとに違った（COFF の時刻。`-mno-incremental-linker-compatible` を MSVC の target の Clang 引数に足した。Windows のホストの `--target native` は変えない）、`cross_hint` が同梱ツールチェーンのない target にそれを勧めていた、を直した。wasm64 の launcher の挙動の変更は意図どおり。ランタイムを含む Mach-O のオブジェクトを同梱ツールチェーンで作れないのは基準でも同じ（既知の限界）。

### 文書と計測

- 文書: `_tsuzuri/language-reference/` は、新ページ `compiler/platforms.md`（tier の定義と表、リンク、実行器、bare-metal、診断、制限、確かめた範囲）と、`index.md`・`compiler/option.md`・`usage.md`・`diagnostics.md`・`directives.md`・`native-interop.md`・`webassembly.md`・`built-in-types-and-modules/os.md`・`net.md`・`matrix.md`（`Math.fma` は AArch64 の target）・`test.md`・`bench.md`・`async-tasks-and-lazy/async.md`。`README.md`（CLI の表、クロスコンパイルの節、検証コマンド）、`docs/language.md`（新しい小節）、`docs/architecture.md`（`src/target.rs`、`Platform`、キャッシュ、検証）。tier の数字は `platforms.md` の表だけに書き、README と architecture は表を指す。旧計画の `_docs/tools/*` と `_docs/feature-status.md` は存在しない。
- 計測: 速さの主張はない。生成コードと実行時間は変えていない（ホストの IR は基準と一致）。コンパイル時間は計測していない（増えたのは Clang の引数の組み立てだけ）。ビルドキャッシュのキーは、このコミット以降の最初のビルドで一度だけ全部変わる（triple が `BuildOptions` に入り、`TSUZURI_SYSROOT` が環境変数の一覧に入るため）。

## 既知の限界

- 実行して確かめたのは、手元の arm64 macOS の `aarch64-apple-darwin` だけ。外国の実行ファイル（qemu-user・Rosetta・Wine）は、この機械にないので動かしていない。`TSUZURI_RUNNER` は偽の実行器で確かめ、qemu-user による実行は `targets.yml` の Linux の job が行う。
- `targets.yml` は、まだ GitHub Actions で実行していない。tier の表は、初回の実行の結果で見直す。Linux のホストと Intel Mac の動作は、そこで初めて確かめる。
- Windows のホストの経路は型検査だけ。名前を付けた Windows の target を Windows のホストで実行した結果、MSVC の実行ファイルを Windows 以外のホストでリンクした結果はない。Windows の `--emit shared` と OS API は G10 のまま。
- Android は、オブジェクトと LLVM IR までで、NDK の sysroot で実行ファイルをリンクできるかは確かめていない。iOS は実機でもシミュレーターでも動かしていない。FreeBSD は、ランタイムの C のコンパイルと同梱ツールチェーンでのリンクまで。
- 同梱ツールチェーンは、Mach-O のランタイムを含む `--emit object` を作れない（Zig が `-keep_private_externs` を受け付けない）。Windows の COFF のランタイムを含む `--emit object` は `E2002` のまま。
- 32-bit の native target、sysroot の自動取得、`--cpu` の新しい値は対象外のまま。
