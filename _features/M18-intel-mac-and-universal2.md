# M18: Intel Mac（`x86_64-apple-darwin`）と Universal 2（d719cf68 の「5 つのプラットフォーム」の方針を改める決定が要る。D-67 の分岐 3 では改めないと決めた）

| 項目 | 内容 |
| --- | --- |
| ID | M18 |
| 優先度 | P3（D-67 の分岐 3 で方針を改めないと決めた（2026-11-26）。方針を改める決定があるまで着手しない。Apple は macOS 26 を Intel 対応の最後の版と発表している（リポジトリの外の事実。着手時に確かめる）） |
| 規模 | XL（Phase 1・2。方針を改める決定があるまで着手しない） |
| 依存 | ([W36](W36-c-layout-by-value.md)), ([M17](M17-macos-min-version-and-weak-import.md)), ([M02](M02-objc-typed-message-send.md)) |
| 後続 | – |
| 状態 | blocked（D-67 の分岐 3 で、d719cf68 の「5 つのプラットフォーム」の方針を改めないと決めた（2026-11-26）。方針を改める決定があるまで着手しない） |
| 起票 | 2026-11-26（D-67。利用者の依頼「「どのチケットにもない壁」と「調査中に見つけたコンパイラの不具合（自分で再現済み）」のそれぞれについて @_features/ に開発チケットまたは修正チケットを作成して」から起票。調査で報告した「配布 3（Intel Mac 向けの出力と Universal 2 が作れない）」に当たる。HEAD `804b7970` で確認） |
| 承認 | [GUIDE D-67](GUIDE.md#d-67-tsuzuri-だけで-macos-のデスクトップアプリを書くm01m22) の分岐 3（d719cf68 と README.md:877・platforms.md:15 の方針の改訂、tier と CI の機械）は承認済み（2026-11-26。改めない）。決定事項 D1（方針を改める案）は却下で、状態は `blocked`。方針を改める決定があるまで着手しない。そのときは、D-55 の登録（target の名前 `x86_64-apple-darwin` を戻すこと、Universal 2 の綴り、`TSUZURI_LIPO`）と決定事項 D2・D3 も要承認。ほかは既定案 |
| 改善する劣位 | C・Swift（Xcode の `ARCHS` と `lipo`）・Go（`GOARCH=amd64`）・Rust（`x86_64-apple-darwin`）比: Intel Mac で動く実行ファイルと、1 つのファイルで Apple シリコンと Intel の両方で動く Universal 2 のバイナリを作れない。Apple シリコン専用として配るしかない |
| 手本にする既存実装 | d719cf68 より前の `x86_64-apple-darwin`（`git show d719cf68^:src/target.rs` の `X86_64AppleDarwin`、`git show d719cf68^:scripts/toolchain/clang.rs` の `NATIVE_TARGETS`、`git show d719cf68^:tests/targets.mjs` の 249〜262 行の Rosetta の実行、`git show d719cf68^:.github/workflows/targets.yml` の `macos-15-intel`）、`src/target.rs` の `Triple::spec`・`cpu_levels`・`ForeignRunner`（`TSUZURI_RUNNER`）、`src/driver.rs` の `tool("TSUZURI_DSYMUTIL", "dsymutil")`（外部のツールの探し方。`resolve_tool`）、[W36](W36-c-layout-by-value.md) の Phase 2（System V x86-64 の分類） |
| 主な影響ファイル | `src/target.rs`, `src/driver.rs`, `src/main.rs`, `src/test_runner.rs`, `src/diagnostic_kinds.rs`, `scripts/toolchain/clang.rs`, `src/runtime/tzroutine.c`, `src/runtime/cpu.c`, `tests/targets.rs`, `tests/targets.mjs`, `tests/platforms.mjs`, `.github/workflows/targets.yml`, `README.md`, `docs/architecture.md`, `docs/language.md`, `_tsuzuri/language-reference/compiler/platforms.md`, `_tsuzuri/language-reference/compiler/option.md`, `_tsuzuri/language-reference/built-in-types-and-modules/net.md`, `_features/W36-c-layout-by-value.md` |

## 実装者向け早見

- **着手しない（blocked）。** D-67 の分岐 3 で、d719cf68 の「5 つのプラットフォーム」の方針を改めないと決めた（2026-11-26）。方針を改める決定があるまで、状態は `blocked` のまま、どの Phase にも着手しない。このチケットは、方針を改める場合に何が要るかの記録として残す（`scripts/repo-kb/tickets.mjs` が読む状態は `todo`・`doing`・`done`・`blocked` で、`closed` は使わない）。
- **人間が求めない限り Phase 1 だけを実装する**（GUIDE §0）。
- 範囲:
  - Phase 1: native の target の表に `x86_64-apple-darwin`（Clang の target `x86_64-apple-macosx12.0.0`）を出力の target として戻す。ホストとしての Intel Mac（コンパイラ自身・配布物・VS Code）は足さない。同梱の launcher が Intel の macOS の target を拒む検査を外す。ランタイムの C は今のまま使う見込み（実験で x86_64 の Mach-O にリンクできた）。x86_64 の Mach-O は macOS のホストで ad-hoc に署名する（D6）。M02 が入っていれば ObjC の x86_64 の差（BOOL と `_stret`）を足す（D4）。CI の範囲は D2。
  - Phase 2: Universal 2（綴りは D3。推奨は `--target universal2-apple-darwin`）。2 つの target でビルドして `lipo -create` で 1 つにする。`--emit exe`・`shared`・`object`。
- 最初に読む: 「現状」の CI の記録と実験 → `git show d719cf68 -- src/target.rs scripts/toolchain/clang.rs tests/targets.mjs .github/workflows/targets.yml` → 仕様 1〜3。
- 最初に書くテスト: `src/target.rs` の `parses_native_wasm_and_unknown_targets` の更新と `x86_64_apple_darwin_is_a_mach_o_x86_64_target` → `tests/targets.rs` の launcher の表 → `tests/targets.mjs` の x86_64 の Mach-O の検査。
- 触ってはいけない: 既存の 9 個の target の出力（IR・object・リンク行）、`--target native`、コンパイラのビルドホストの制限（`src/lib.rs:1-8`）、配布物・VS Code・CI の 5 つのホスト（`tests/platforms.mjs`）、bindgen が Intel の macOS の clang を拒むこと（`src/bindgen.rs:306-315`）、整数・浮動小数点・トラップ・tzroutine・所有の意味。
- 完了の見分け方:
  - Phase 1: Apple シリコンの Mac で `tsuzuri build examples/tzroutines --target x86_64-apple-darwin -o t` が `Mach-O 64-bit executable x86_64` を作り、`codesign --verify --strict t` が通る。Rosetta がある機械で `TSUZURI_RUNNER='arch -x86_64' tsuzuri run examples/tzroutines --target x86_64-apple-darwin` が `21325334000` を出す（`-O0`・`-O3`）。今は `unknown target 'x86_64-apple-darwin'` の `E2000`。
  - Phase 2: `--target universal2-apple-darwin` の出力の `lipo -info` が `x86_64 arm64` で、`lipo -thin arm64` で取り出したものが `--target aarch64-apple-darwin` の出力と byte 単位で一致し、`codesign --verify --strict` が通る。

## 目的

Intel の CPU の Mac で Tsuzuri のアプリを動かすには、x86_64 の出力が要る。Mac のアプリを Intel と Apple シリコンの両方に配る標準の形は、2 つのスライスを 1 つのファイルに持つ Universal 2 のバイナリである。Tsuzuri は 2026-10-21 の d719cf68（Restrict native support and CI to five platforms）で、`x86_64-apple-darwin` の target、Intel Mac の配布物、`macos-15-intel` の CI を外した。今の Tsuzuri で作ったアプリは Apple シリコンの Mac でしか動かない。

このチケットは、方針を改める場合に何が要るかを記録し、判断を明示する。Apple は macOS 26 を Intel 対応の最後の版と発表している（リポジトリの外の事実）。対象の利用者は今後減るので、[GUIDE D-67](GUIDE.md#d-67-tsuzuri-だけで-macos-のデスクトップアプリを書くm01m22) の分岐 3 は推奨の案の「改めない」に決まった（2026-11-26）。後に方針を改める場合も、足すのは出力の target だけで、コンパイラを Intel Mac で動かすことは足さない。

## 着手条件と停止条件

### 着手条件

- **方針を改める決定（D1）。** D-67 の分岐 3 で、d719cf68 の「5 つのプラットフォーム」の方針を改めないと決め、D1 は却下した（2026-11-26）。状態は `blocked` で、方針を改める決定が記録されるまで、どの Phase にも着手しない。そのときは D2（tier と CI）と D3（Universal 2 の綴りと `TSUZURI_LIPO`。D-55）の承認も要る。D3 の承認前は Phase 2 に着手しない。
- **人間が求めない限り Phase 1 だけを実装する。**
- P3 で独立レビューを受けていないので、着手前にその時点のコードと台帳に対してこのチケットをレビューし、指摘を反映してから実装する（GUIDE §0 の 2）。`node scripts/repo-kb.mjs brief M18` を読む。
- リポジトリの外の事実を確かめ、チケットに日付とともに記録する:
  1. Apple の Intel の Mac への対応の終わり（macOS 26 が最後とされる）と、Rosetta 2 を提供する予定。
  2. GitHub Actions の Intel の macOS の runner（`macos-15-intel` など）が使えるか、Apple シリコンの runner（`macos-15`）に Rosetta 2 があるか（runner の image の README と、試しの workflow の `arch -x86_64 /usr/bin/true`）。
  3. その時点の Xcode・Command Line Tools の SDK が x86_64 を出せること（`clang -target x86_64-apple-macosx12.0 hello.c` と `lipo -create`。SDK の `.tbd` の `targets` に `x86_64-macos` があること）。
- CI で x86_64 の実行を確かめる機械（D2）を確保できなければ着手しない。
- 基準の保存: `tests/fixtures/*/Main.tz` と `examples/*/Main.tz` の `--emit llvm` と `--emit object`（9 個の target）を `target/scratch/m18/base/` に保存する。
- 「現状」の行番号を `grep -n "pub enum Triple\|pub const ALL\|pub const fn cpu_levels" src/target.rs`、`grep -n "macOS toolchain" scripts/toolchain/clang.rs`、`grep -n "#define TZR_UC_PC" src/runtime/tzroutine.c` で確かめ直す。

### 停止条件

- **S1** tzroutine の x86-64 の macOS の文脈の切り替えか SIGURG の横取り（TZ10 の Phase 2）が、今の x86-64 System V の版（`src/runtime/tzroutine.c:579-618`）と Apple の x86_64 の分岐（`2437-2439`・`2461-2467`）で動かず、新しい実装が要る。
- **S2** 既存の 9 個の target のどれかの `--emit llvm`・object・リンク行が、保存した基準と 1 byte でも違う。
- **S3** CI で x86_64 を実行する手段（Rosetta 2 か Intel の runner）がない、またはなくなる予定が確かめられた。tier の主張ができないので、止めて D2 を決め直す。
- **S4** その時点の SDK が x86_64 を外した（`.tbd` の `targets` に `x86_64-macos` がない）。
- **S5** Phase 2 で、Universal 2 から取り出した arm64 のスライスが、`--target aarch64-apple-darwin` の出力と一致しない（出力が決定的でない）。
- **S6** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `804b7970` で確認）

macOS 27.2.1 arm64、Apple clang 21.0.0（clang-2100.4.1.1）、Apple ld（ld-27106.1）、SDK 27.2（`/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk`）、dev の `target/debug/tsuzuri` で確かめた。この機械には Rosetta 2 がない。

### 方針と記録

- README.md:877: 「対応 OS / CPU: Windows x64・Arm64、Linux x64・Arm64、macOS Arm64 のみ（Intel Mac、32-bit、その他の OS / CPU は対象外）」。
- platforms.md:15: macOS x64（Intel Mac）を `--target` に指定すると `E2000`。platforms.md:17: 同梱の `tsuzuri-clang` は Intel macOS のホストを拒み、どのホストからも Intel macOS 向けの生成を受け付けない。platforms.md:49-68 の tier の表に x86_64 の macOS はない。
- d719cf68（2026-10-21、Restrict native support and CI to five platforms）: `src/target.rs` の `X86_64AppleDarwin`（`x86_64-apple-darwin`、Clang の target `x86_64-apple-macosx12.0.0`）、launcher の `("x86_64-apple-macosx12.0.0", "x86_64-macos.12.0")`、`targets.yml` の `macos-15-intel` の job、Intel Mac の配布物を外し、`src/lib.rs` に 5 つのホストの `compile_error!` を足した。
- `_features/_completed/G14-toolchain-distribution.md:593`: 配布物（ツールチェーン）の universal binary は対象外。プログラムの出力の Universal 2 を扱うチケットは、未完了にも完了にもない。
- docs/architecture.md:443-450、docs/language.md:4951-4954、net.md:381 も Intel の macOS を対象外と書く。
- W36:19・164 は、Phase 2 の System V x86-64 の対象を「Linux・Intel の macOS」と書く。今は target がないので、起票の PR が W36:19・164 に注記を足した。D-67 の分岐 3 で方針を改めないと決めたので、注記は「Intel の macOS の target は入れない」である（2026-11-26）。

### 再現

`app` は `examples/hello` の写し:

```text
$ tsuzuri build app --target x86_64-apple-darwin -o app-x64
<command line>:1:1: error[E2000]: unknown target 'x86_64-apple-darwin'; use native, wasm32, wasm64, or one of aarch64-apple-darwin, aarch64-unknown-linux-gnu, x86_64-unknown-linux-gnu, aarch64-pc-windows-msvc, x86_64-pc-windows-msvc, aarch64-unknown-linux-musl, x86_64-unknown-linux-musl, aarch64-pc-windows-gnu, x86_64-pc-windows-gnu
$ echo $?
2
$ tsuzuri build app --target x86_64-apple-darwin --json 2>&1 | grep -o '"kind":"[a-z_]*"'
"kind":"unknown_target"
$ arch -x86_64 /usr/bin/true
arch: posix_spawnp: /usr/bin/true: Bad CPU type in executable
```

### CI の記録（d719cf68 の前）

`gh run view` で読んだ、2026-10-20 の `main` の `targets.yml` の run 39428404372（head `95875f12`）:

- `targets (darwin-x64)`（`macos-15-intel`、image `macos-15` 20260824.0482）: `x86_64-apple-darwin: executed (-O0 and -O3: build, run, test; --target native gives the same output)`。
- `targets (darwin-arm64)`（`macos-15`）: `x86_64-apple-darwin: executed (through Rosetta (TSUZURI_RUNNER='arch -x86_64'))`。GitHub の Apple シリコンの runner に、この日は Rosetta 2 があった。
- `targets (linux-x64)`: `x86_64-apple-darwin: linked only (macho linked with tsuzuri-clang)`（同梱の zig で `x86_64-macos.12.0` をリンクした）。

d719cf68 が消した `targets.yml` の注記によると、`macos-15-intel` は 1 回に約 220 分（Rust のテストだけで 75 分。dev の profile を `opt-level = 1` にする前の測定）かかり、ほかの 2 ホストは 80〜110 分だった。f16 の検査は `generic=soft,native=conversions`（F16C で変換し、演算はソフトウェア）で、fast backend の wasm32 の段は Intel の runner で外していた。Rosetta の実行は、消した `tests/targets.mjs:249-262` が `arch -x86_64 /usr/bin/true` の成否で判断し、`TSUZURI_RUNNER='arch -x86_64'` で `examples/tasks` の `run` を 1 回（既定の最適化だけ）通していた。`-O0`・`-O3` の `build`・`run`・`test` を x86_64 で実行していたのは `macos-15-intel` の job だけである。

### 実験: 今のランタイムで x86_64 の Mach-O を作る

`--target aarch64-apple-darwin` のビルドに、Clang の target だけを書き換える `TSUZURI_CLANG` のラッパーを付けた（IR は aarch64 向けに作るので、`@cpu` の版と `Math.fma` は x86_64 の target の IR と違う。確かめたのはランタイムの C のコンパイルとリンクだけである）:

```sh
#!/bin/sh
# Rewrites the Apple arm64 target to x86_64.
out=""
for a in "$@"; do
  case "$a" in
    --target=arm64-apple-macosx12.0.0) a="--target=x86_64-apple-macosx12.0.0" ;;
  esac
  out="$out '$a'"
done
eval exec /usr/bin/clang $out
```

- `Main.tz` を持つ `examples` の 9 個（`computations`・`control`・`currying`・`functional`・`hello`・`io`・`point`・`polymorphism`・`tzroutines`）が、`--no-cache` の `-O3` で `Mach-O 64-bit executable x86_64` にリンクできた。`tzroutines` は `-O0` でも。`runtime.c` は `-DTZ_ASYNC_PREEMPT`、`stack.c` は `-DTZ_STACK_GUARD` でコンパイルされ、`__TEXT,__tz_text` の節があり、`LC_BUILD_VERSION` は `minos 12.0`・`sdk 27.2`。実行はしていない（Rosetta がない）。
- `lipo -create tzr-x64 tzr-arm64 -output tzr-universal` で `x86_64 arm64` の fat のファイルができ、この機械では arm64 のスライスが動いて `21325334000` を出した。
- 署名: arm64 のスライスは Apple の ld の `adhoc,linker-signed`。x86_64 のスライスは `code object is not signed at all`（Apple の ld は x86_64 を既定で署名しない）で、`codesign --verify --strict tzr-universal` は `code object is not signed at all` で失敗した。Apple の ld に `-Wl,-adhoc_codesign` を渡すと x86_64 も `adhoc,linker-signed` になり、Universal 2 の検査が通った。fat のファイルに `codesign -s - -f` をかけても通った。x86_64 の thin のファイルだけを `codesign -s -` で署名してから `lipo -create` すると、fat のファイルが `codesign --verify --strict` を通り、`lipo -thin arm64` で取り出したスライスは arm64 の入力と byte 単位で一致した（D6 の手順。C の小さな実行ファイルで確かめた）。
- `--target aarch64-apple-darwin --no-cache` で `examples/tzroutines` を 2 回、出力の場所を変えてビルドすると、実行ファイルは byte 単位で一致した（S5 の検査の前提）。
- 同梱の zig 0.16.0（`target/toolchain/zig/zig`）は `zig cc -target x86_64-macos.12.0` で x86_64 の Mach-O をリンクできた（署名なし）。`-Wl,-adhoc_codesign` は `error: unsupported linker arg: -adhoc_codesign` で拒んだ。zig の arm64 の出力は `adhoc,linker-signed`。
- C の目的ファイル 2 つを `lipo -create` した fat の `.o` は、`clang -target arm64-apple-macosx12.0` と `-target x86_64-apple-macosx12.0` のどちらのリンクにも渡せ、arm64 の実行ファイルが動いた。
- `/usr/bin/lipo` は xcselect の shim（`libxcselect.dylib` をリンクし、実体は `/Library/Developer/CommandLineTools/usr/bin/lipo`）で、Command Line Tools が要る。`/usr/bin/codesign` は OS の一部（`libxcselect` をリンクしない）。
- SDK 27.2 の `AppKit.tbd` の `targets` は `x86_64-macos` を含む。`libobjc.tbd` は `_objc_msgSend_stret`・`_objc_msgSendSuper2_stret`（と `_objc_msgSendSuper_stret`）を、x86_64 の target の節にだけ export する。

### ObjC の差（clang の参照）

`clang -target <t> -fobjc-arc -fno-objc-msgsend-selector-stubs -O1 -S -emit-llvm` で、次の関数を 2 つの target で下げた:

```objc
#import <AppKit/AppKit.h>
BOOL visible(NSWindow *w) { return [w isVisible]; }
void set_visible(NSView *v, BOOL b) { [v setHidden:b]; }
NSRect frame_of(NSWindow *w) { return [w frame]; }
NSPoint origin_of(NSView *v) { return [v frame].origin; }
NSRange range_of(NSString *s) { return [s rangeOfString:@"a"]; }
double alpha_of(NSWindow *w) { return [w alphaValue]; }
```

| 送信 | `x86_64-apple-macosx12.0` | `arm64-apple-macosx12.0` |
| --- | --- | --- |
| BOOL の結果 | `call signext i8 @objc_msgSend(ptr, ptr)` | `call zeroext i1 @objc_msgSend(ptr, ptr)` |
| BOOL の引数 | `i8 noundef signext` | `i1 noundef zeroext` |
| NSRect（32 バイト）の結果 | `call void @objc_msgSend_stret(ptr sret(%struct.CGRect) align 8, ptr, ptr)` | `call %struct.CGRect @objc_msgSend(ptr, ptr)` |
| NSPoint（16 バイト、`double` 2 つ）の結果 | `{ double, double }`（`origin_of` の定義の結果） | `%struct.CGPoint` |
| NSRange（16 バイト）の結果 | `call { i64, i64 } @objc_msgSend(...)` | `call [2 x i64] @objc_msgSend(...)` |
| `double` の結果 | `call double @objc_msgSend(ptr, ptr)` | 同じ |
| `[super frame]` | `call void @objc_msgSendSuper2_stret(ptr sret(%struct.CGRect) ...)` | `call %struct.CGRect @objc_msgSendSuper2(...)` |
| `[super isFlipped]` | `call signext i8 @objc_msgSendSuper2(...)` | `call zeroext i1 @objc_msgSendSuper2(...)` |

- x86_64 の `frame_of` は、受け手が nil なら `objc_msgSend_stret` を呼ばずに結果の 32 バイトを `llvm.memset` で 0 にする分岐を置く。
- ObjC のモジュールのフラグ 6 つ（`Objective-C Version`=2 から `Enforce ClassRO Pointer Signing`=0 まで）は 2 つの target で同じ。
- `clang -dM -E` の `__OBJC_BOOL_IS_BOOL` は x86_64 で 0、arm64 で 1。SDK の `objc/objc.h:68-88` が、0 なら `typedef signed char BOOL`、1 なら `typedef bool BOOL` にする。`objc/message.h:119-148` は `objc_msgSend_stret`・`objc_msgSendSuper_stret` を `OBJC_ARM64_UNAVAILABLE` で宣言し、`158-164` は x86-64 の `objc_msgSend_fpret` を `long double` の結果だけに使うと書く。

### コード

- `src/target.rs:31-42` の `Triple`（9 個）、`91-101` の `ALL`（診断の順）、`106-112` の `aarch64-apple-darwin` の spec（`arm64-apple-macosx12.0.0`）、`217-235` の `host()`（Intel の macOS のホストは `None`）、`368-386` の `cpu_levels`（`(Arch::X86_64, _) => 0b1110` なので x86_64 の macOS にも `sse4.2`・`avx2`・`avx512` の版が効く）、`391-393` の `hardware_fma`（aarch64 だけ）。
- `src/lib.rs:1-8`: Windows x64/Arm64・Linux x64/Arm64・macOS Arm64 以外のホストでコンパイラをビルドすると `compile_error!`。
- `scripts/toolchain/clang.rs:20-30` の `NATIVE_TARGETS`（9 個）、`38-40` は Intel の macOS のホストを `the macOS toolchain requires ARM64` で拒み、`47-53` は `x86_64-apple-darwin*`・`x86_64-apple-macos*` の target を `the macOS toolchain supports only ARM64 targets` で拒む。`tests/targets.rs:465-497` の `the_bundled_launcher_rejects_intel_macos_hosts_and_targets` がこの 2 つを固定する（ホストの拒否は 467-478、target の拒否は 479-496）。`NATIVE_TARGETS` にない target の値は、launcher が `the bundled toolchain has no C library for ...` で止める（`219-228`）。
- `src/runtime/tzroutine.c`: `238-240` は aarch64 と x86_64 以外を `#error`。`579-618` の x86-64 System V の `tzr_switch` と `tzr_initial_frame` は OS に依らない naked の C の関数。`2264-2275` は Apple の `section$start$__TEXT$__tz_text`。`2380-2393` の `tzr_async_measure` は Apple で `uc_mcsize` を使う。`2434-2445` の `TZR_UC_PC`・`TZR_UC_SP` と `2461-2467` の `tzr_async_redirect` は、Apple の x86_64 の分岐（`__ss.__rip`・`__rsp`・`__rdi`・`__rflags`）を今も持つ。
- `src/runtime/cpu.c:19-23`: Windows でない x86 は `cpuid` で検出する。`54-63` の `tz_cpu_decode` は AVX-512 を `(xcr0 & 0xE6) == 0xE6` で判定する。
- `src/runtime/stack.c:92-96`: Apple の分岐は `pthread_get_stackaddr_np`・`pthread_get_stacksize_np` だけで、CPU に依らない。
- `src/runtime/gpu.c:149-150`・`gpu-vulkan.c:935-939`: macOS の既定の候補に `/usr/local/lib`（Intel の Homebrew の場所）を含む。
- `src/backend/target.rs:79` と `src/backend/mod.rs:452-455`: fast backend は x86_64 の Mach-O の triple と最低版（10.13）を今も持つ。名前付きの `--target` の fast backend は `E2000` なので、Intel のホストでしか届かない。
- `src/bindgen.rs:306-315` の `lp64_target`: bindgen は Intel の macOS の clang を拒む（ホストの clang の問題で、このチケットでは変えない）。
- `src/driver.rs:1628-1634` の `link_args`: macOS のホストの Mach-O は Apple の ld、ほかのホストは `-fuse-ld=lld`。`1586-1624` の `sdk_args`: Mach-O に `-Werror=incompatible-sysroot`、`TSUZURI_SYSROOT` があれば `-isysroot`。`4361-4367`: tzroutine を起動する実行ファイルは、生成コードを Mach-O なら `__TEXT,__tz_text` に置く。
- docs/architecture.md:446: IR が target で変わるのは COFF の allocator と `windows_abi`、`@cpu` の版、`Math.fma` の 3 か所だけ。したがって `x86_64-apple-darwin` の `--emit llvm` は `x86_64-unknown-linux-gnu` のものと同じになるはずである（`--emit llvm` には `__tz_text` の書き換えが入らない）。同じ CPU で OS だけが違う組では、今も `aarch64-apple-darwin` と `aarch64-unknown-linux-gnu` の `--emit llvm` が `examples/hello`・`tzroutines`・`control` で byte 単位で一致する（確かめた）。
- テスト: `src/target.rs:575` の `parses_native_wasm_and_unknown_targets`（610 行で `x86_64-apple-darwin` を拒む）、`src/main.rs:6899` の `accepts_named_targets_and_rejects_unknown_ones`（6945 行）、`tests/targets.mjs:41-51` の triple の表と `297` の拒否、`tests/platforms.mjs:8`・`57-73`（配布物と 3 つの workflow の matrix が 5 つのホストだけ）。

## 変えてはいけない意味

- 既存の 9 個の target の IR・object・リンク行・キャッシュの鍵。`--target native` で clang に target を渡さないこと。
- ホストの制限: コンパイラのビルドホスト（`src/lib.rs:1-8`）、配布物・VS Code・CI の 5 つのホスト、launcher の Intel の macOS のホストの拒否、bindgen の Intel の macOS の clang の拒否。
- 整数・浮動小数点（丸め・NaN・符号付きゼロ）・トラップ・評価順序・tzroutine・所有の意味。x86_64 の macOS でも、ほかの target と同じ結果を出す（`Math.fma` は正しく丸めるソフトウェアのルーチンで、ビットは同じ）。
- 診断で変わるのは、知らない target の `E2000` の文の一覧に `x86_64-apple-darwin`（Phase 2 では `universal2-apple-darwin` も）が加わることだけである。

## 仕様

### Phase の範囲

| Phase | 入るもの | PR |
| --- | --- | --- |
| 1 | `x86_64-apple-darwin` の target、launcher、x86_64 の Mach-O の署名（D6）、M02 があれば ObjC の差（D4）、CI（D2）、文書 | 1 つ目 |
| 2 | Universal 2（D3）: 2 回のビルドと `lipo -create`、`exe`・`shared`・`object` | 2 つ目 |

### 1. Phase 1: target `x86_64-apple-darwin`

- `--target x86_64-apple-darwin` を受ける。Clang の target は `x86_64-apple-macosx12.0.0`、OS は macOS、CPU は x86_64、追加の Clang の引数はない（d719cf68 の前と同じ）。`Triple::ALL` の末尾に置き、診断の一覧は末尾に 1 つ増える（D9）。
- 受けるコマンドと出力は、ほかの名前付きの target と同じ: `build` のすべての native の `--emit`、`run`・`test`（ビルド機で動かせないので `TSUZURI_RUNNER` が要る。Rosetta 2 があれば `arch -x86_64`。D7）。`bench`・`test --coverage`・`test -g` は今の規則で `E2000`。
- IR: `x86_64-unknown-linux-gnu` と同じ（`@cpu` は `sse4.2`・`avx2`・`avx512` の版、`Math.fma` はソフトウェア）。実行ファイルの生成コードは Mach-O の `__TEXT,__tz_text` に置く（今の `platform.mach_o()` の分岐）。
- リンク: macOS のホストは Apple の ld（今の `link_args`）。ほかのホストは `-fuse-ld=lld`（`ld64.lld` と `TSUZURI_SYSROOT` の SDK が要る）か、同梱の toolchain（launcher が `x86_64-macos.12.0` に写す）。
- 最低版は 12.0（`aarch64-apple-darwin` と同じ）。[M17](M17-macos-min-version-and-weak-import.md) が入っていれば、その有効な版が x86_64 にも効く（`x86_64-apple-macosx<版>`、launcher は `x86_64-macos.<版>`）。
- 署名: macOS のホストでは、x86_64 の Mach-O の実行ファイルと共有ライブラリを、リンクの直後（出力の場所へ移す前）に `codesign -s -` で ad-hoc に署名する。macOS でないホストでは署名しない（D6）。
- `-g`: macOS のホストの `dsymutil` が今の arm64 と同じく `.dwarf` を作る。
- ランタイム: C は変えない（D5）。

### 2. Phase 1: ObjC の x86_64 の差（[M02](M02-objc-typed-message-send.md) が入っているとき。D4）

- メンバーの `bool` は x86_64 で `i8 signext`（arm64 は `i1 zeroext`）。結果は `!= 0` で `bool` に正規化する（`signed char` の BOOL は 0・1 以外を持ちうる）。引数は 0 か 1 を渡す。`ref mut bool` は 1 バイトの `BOOL *` で、arm64 と同じ規則（非ゼロを true）。
- 構造体の結果: [W36](W36-c-layout-by-value.md) の Phase 2 の System V の分類がメモリー（`sret`）にするときだけ `objc_msgSend_stret`（[M03](M03-objc-classes-defined-in-tsuzuri.md) の super は `objc_msgSendSuper2_stret`）を呼び、`sret` の領域を最初の引数にする。レジスターで返る構造体（NSPoint の `{ double, double }`、NSRange の `{ i64, i64 }`）は `objc_msgSend` を分類の型で呼ぶ。W36 の Phase 2 がなければ、x86_64 の構造体の値渡しと値返しは `E1008`（`abi_record_by_value_target`）のまま。
- 受け手は型で非 nil なので、clang が nil の受け手に置く結果の 0 埋め（「現状」）は要らない（M02 の arm64 と同じ）。`@weak` のクラス（M17）は classref の NULL を検査してトラップする。
- `objc_msgSend_fpret`・`fp2ret` は使わない（x86-64 では `long double` の結果だけで、Tsuzuri に `long double` はない）。
- selref・classref・superref・CFSTR・Block の literal・モジュールのフラグは arm64 と同じ形で出す（フラグの 6 つが同じことを確かめた）。

### 3. Phase 1: CI と tier（D2）

推奨（D2 の案 a）:

| CLI 名 | Clang の `--target` | tier | 確認 |
| --- | --- | --- | --- |
| `x86_64-apple-darwin` | `x86_64-apple-macosx12.0.0` | 2 | リンク（3 job。macOS は Apple の ld、Linux は同梱の zig）。macOS Arm64 の job で Rosetta 2 を使って実行（`-O0`・`-O3` の `build`・`run`・`test`、aarch64 との出力の一致） |

- `tests/platforms.mjs` の 5 つのホストは変えない（Intel の runner を足さない）。
- Rosetta 2 が runner にないときは、`tests/targets.mjs` が「executed」の代わりに「linked only」と報告する（d719cf68 の前と同じ）。tier 2 の主張は、その run の記録の範囲だけである（platforms.md の注記）。

### 4. Phase 2: Universal 2（D3・D8）

- `--target universal2-apple-darwin` は、`aarch64-apple-darwin` と `x86_64-apple-darwin` の 2 回のビルドの出力を `lipo -create` で 1 つにする。`lipo` への入力の順は x86_64、arm64。
- 受ける出力は `build` の `--emit exe`（fat の実行ファイル）・`shared`（fat の dylib）・`object`（fat の `.o`）だけ。`run`・`test`・`bench`、ほかの `--emit`、`-g`、`--cpu native` は `E2000`（kind 案 `universal_requires_build`）。
- 各スライスは今の `build_complete` で作り、それぞれ今のキャッシュの鍵（triple を含む）で成果物のキャッシュを使う。`lipo` は毎回走らせる。
- 署名: x86_64 のスライスは Phase 1 の規則で、`lipo` の前に署名済み（macOS のホスト）。arm64 のスライスはリンカーの署名のまま。fat のファイルを署名し直さない（arm64 のスライスを thin の出力と同じに保つ）。
- `lipo` は `TSUZURI_LIPO`、配布物の `bin/`、`PATH` の `lipo` の順に探す（`tool` の規則）。起動できないか失敗したら `E2002`（kind 案 `universal_lipo_failed`。文案 `Universal 2 output needs lipo: install the Xcode Command Line Tools, or set TSUZURI_LIPO to lipo or llvm-lipo`）。macOS でないホストでも、同梱の toolchain で 2 つをリンクでき、`TSUZURI_LIPO` が `llvm-lipo` を指せば作れる見込みだが、確かめていない（Phase 2 の手順で確かめる。x86_64 のスライスは署名されない）。

### 5. 診断

| コード | kind | 条件 | メッセージ（案） |
| --- | --- | --- | --- |
| E2000 | `unknown_target`（既存の kind） | 知らない target | 今の文。一覧の末尾に `x86_64-apple-darwin`（Phase 2 は `universal2-apple-darwin` も） |
| E2000 | `universal_requires_build`（新。Phase 2） | 仕様 4 の受けない組み合わせ | `--target universal2-apple-darwin builds an executable, a shared library or an object with --emit exe, shared or object; build one architecture with --target aarch64-apple-darwin or x86_64-apple-darwin to run, test or debug it` |
| E2002 | `universal_lipo_failed`（新。Phase 2） | `lipo` を起動できないか失敗した | 仕様 4 の文 |

- 知らない target の kind は今の `unknown_target`（`src/diagnostic_kinds.rs:818`、`src/main.rs:1122`。「現状」の再現の `--json` で確かめた）。
- launcher の `the macOS toolchain supports only ARM64 targets` は届かなくなる（x86_64 の target を受ける）。`the macOS toolchain requires ARM64`（ホストの拒否）は残る。

## 設計

### Phase 1

- `src/target.rs`: `Triple::X86_64AppleDarwin` と spec、`ALL` を 10 個にする（末尾）。`host()` は変えない。`cpu_levels`・`hardware_fma`・`mach_o`・`shared_extension` は今の `match` のまま正しい値を返す（単体テストで固定する）。
- `scripts/toolchain/clang.rs`: `NATIVE_TARGETS` に `("x86_64-apple-macosx12.0.0", "x86_64-macos.12.0")` を足し、`47-53` の target の拒否を外す。`38-40` のホストの拒否は残す。M17 が入っていれば、M17 の版の写し方に x86_64 を足す。
- `src/driver.rs`: x86_64 の Mach-O の `Executable`・`Shared` で、macOS のホストなら、リンクの後に `codesign -s - <staged artifact>` を走らせる（M16 が入っていれば M16 の `TSUZURI_CODESIGN` の解決を使い、入っていなければ `PATH` の `codesign`。後から入る M16 が揃える）。失敗は `E2002`（今の `run_tool` の形）。`-g` の別のリンク（`build_complete` の 2 つ目のリンク）の出力にも同じく。
- M02 があれば（D4）: M02 の下げ方（M02 が作る `src/llvm_objc.rs` の送信の型を作る所）に `current_platform()` の x86_64 の分岐を足す。`bool` の型の文字列、`_stret` の選択（W36 の `llvm_byvalue::classify` の結果が `sret` か）、super の `_stret`。arm64 の IR は変えない。
- ランタイム（D5）: 変えない。手順 2 で x86_64 の macOS 向けにコンパイルし、警告が増えないことと、Rosetta で tzroutine の E2E が通ることを確かめる。
- `tests/targets.mjs`: 表に `["x86_64-apple-darwin", { os: "macos", arch: "x86_64", format: "macho", platform: 1, minos: 12 }]`、`machine.macho.x86_64 = 0x01000007`、拒否の一覧から外す。macOS のホストでは d719cf68 の前の 249〜262 行と同じく、system Clang でリンクし、Rosetta があれば `TSUZURI_RUNNER='arch -x86_64'` で実行して出力を比べる。`codesign --verify --strict` も検査する。

### Phase 2

- `src/target.rs` の `parse_target`: `universal2-apple-darwin` を `(Target::Native, Some(Triple::Aarch64AppleDarwin))` と「Universal 2」の印に読む（`BuildOptions::universal: bool`）。今の返り値は `(Target, Option<Triple>)` なので、印を返す形に広げるか、`src/main.rs` の `--target` の解析（1106-1126 行）が `parse_target` の前にこの名前を読む。`Triple` は増やさない（IR を作る target ではない）。知らない target の文の一覧（`unknown_target_message`）には、`Triple::names()` の後にこの名前を足す。
- `src/driver.rs`: `build_universal`。`universal` なら、`BuildOptions` の triple を x86_64、aarch64 の順に替えて `build_complete` を 2 回呼び（出力は一時のディレクトリ）、`lipo -create <x86_64> <arm64> -output <出力>` を走らせる。`build_complete` に渡す `BuildOptions` では `universal` を `false` に戻す。成果物のキャッシュの鍵は `BuildOptions` の `Debug` の文字を含む（`src/cache.rs:510`）ので、戻さないと各スライスの鍵が thin のビルドと分かれ、仕様 4 の「今のキャッシュを使う」と S5 の比較が成り立たない。仕様 4 の組み合わせは `BuildOptions::validate` で拒む。
- `src/main.rs`: `--help` と `--target` の一覧に `universal2-apple-darwin`。`run`・`test`・`bench` の解析で `universal` を拒む。

## 前提とする他チケットのインターフェース

- [W36](W36-c-layout-by-value.md)（弱い）: Phase 2 の System V x86-64 の分類（`llvm_byvalue::classify`）。W36:164 の「Intel の macOS」は、この target がある場合に効く。M18 が先なら、W36 の Phase 2 は x86_64-apple-darwin を Linux と同じ分類で扱う（macOS の x86-64 は System V に従う）。
- [M17](M17-macos-min-version-and-weak-import.md)（弱い）: `[macos] min_version` と、版を Clang の target と launcher に渡す口。x86_64 にも同じ版を使う。
- [M02](M02-objc-typed-message-send.md)（弱い）: ObjC の送信の下げ方。M02 が後なら、M02 が仕様 2 の差を足す（このチケットの参照の表を使う）。
- [M16](M16-macos-signing-notarization-hardening.md)（あれば）: `TSUZURI_CODESIGN` と署名の手順。M16 の Developer ID の署名は bundle の中の fat のファイルの全スライスを署名し直すので、仕様 1 の ad-hoc の署名と矛盾しない。

## 他チケットへ提供するインターフェース

- `Triple::X86_64AppleDarwin`。[M15](M15-macos-app-bundle.md) の `--emit app` は、Universal 2 の実行ファイルを `Contents/MacOS` に置ける（M15 がそれを足すかは M15 か後のチケットで決める。このチケットの対象外）。
- Phase 2 の `BuildOptions::universal` と `build_universal`。
- ObjC の x86_64 の差の参照の表（「現状」）: M02・M03・M05 が x86_64 の分岐を足すときに使う。

## 実装手順

### Phase 1

1. **事実の確認とベースライン**。変更: なし（`target/scratch/m18/` の下書きだけ）。内容: 着手条件の外の事実を確かめ、このチケットに記録する。基準を保存する。「現状」の実験（ラッパーでのリンク）を着手時の HEAD でやり直す。確認: 記録した。9 個の target の基準の件数を記録した。
2. **target と launcher**。変更: `src/target.rs`・`scripts/toolchain/clang.rs`・`src/main.rs`（テスト）・`tests/targets.rs`。内容: 設計の Phase 1 の最初の 2 項。確認: T0。`x86_64_apple_darwin_is_a_mach_o_x86_64_target`、`parses_native_wasm_and_unknown_targets`、`accepts_named_targets_and_rejects_unknown_ones`、launcher の表のテスト。`x86_64-apple-darwin` の `--emit llvm` が `x86_64-unknown-linux-gnu` と一致する（fixture と examples の全件）。既存の 9 個の target の出力が基準と一致する（S2）。T1。
3. **リンクと署名**。変更: `src/driver.rs`・`src/test_runner.rs`。内容: x86_64 の Mach-O の ad-hoc の署名（D6）。確認: macOS のホストで `build examples/tzroutines --target x86_64-apple-darwin` が `Mach-O 64-bit executable x86_64` で、`codesign --verify --strict` が通る。`--emit shared` と `-g` も。
4. **ObjC の差**（M02 が done のときだけ）。変更: M02 の下げ方のファイル。内容: 仕様 2。確認: `--target x86_64-apple-darwin --emit llvm` の送信の型が「現状」の参照の表と一致する（`signext i8`、`objc_msgSend_stret` と `sret`、`{ double, double }`、`{ i64, i64 }`）。arm64 の IR は基準と一致する。
5. **CI と E2E**。変更: `tests/targets.mjs`・`.github/workflows/targets.yml`。内容: 設計の `tests/targets.mjs`、D2 の範囲。確認: Rosetta のある Mac（runner か手元）で `node tests/targets.mjs target/debug/tsuzuri` が `x86_64-apple-darwin: executed` を報告する。PR に `ci:full` を付け、`targets.yml` の 3 つの job の結果を確かめる。
6. **文書**。確認: T2、`node scripts/check-docs.mjs <変更したページ>`、`node scripts/check-cli-docs.mjs`。

### Phase 2（人間が求め、D3 が承認されたとき）

7. **解析と検査**。変更: `src/target.rs`・`src/driver.rs`・`src/main.rs`・`src/diagnostic_kinds.rs`。内容: 設計の Phase 2、仕様 4・5。確認: `universal_target_parses_and_rejects_runs`。T1。
8. **ビルドと lipo**。変更: `src/driver.rs`。内容: `build_universal`。確認: 呼び出しを記録する偽の clang と `TSUZURI_LIPO` の偽の `lipo` で、2 回のビルドの triple と `lipo` の引数の順を検査する `universal_build_links_both_slices_then_lipo`。macOS で 3 つの `--emit` の E2E（「テスト計画」）。macOS でないホストで `TSUZURI_LIPO=llvm-lipo` を試し、結果をこのチケットに記録する。T2。`ci:full`。

## テスト計画

### Rust（どのホストでも）

- `src/target.rs`:
  - `x86_64_apple_darwin_is_a_mach_o_x86_64_target`: `parse("x86_64-apple-darwin")`、`clang_target() == "x86_64-apple-macosx12.0.0"`、`os() == MacOs`、`arch() == X86_64`、`Platform::new(Some(..))` の `mach_o()`・`unix()` が真、`windows()` が偽、`cpu_levels() == 0b1110`、`hardware_fma()` が偽、`shared_extension() == "dylib"`、`executable_extension() == ""`。
  - `parses_native_wasm_and_unknown_targets`: 拒否の一覧から `x86_64-apple-darwin` を外し、`Triple::names()` の末尾が `x86_64-apple-darwin`。
- `src/main.rs`: `accepts_named_targets_and_rejects_unknown_ones` の更新。
- `tests/targets.rs`:
  - launcher の表: `resolve(Some("x86_64-apple-macosx12.0.0"), "macos", "aarch64")` の zig が `x86_64-macos.12.0`、`linux`・`windows` のホストからも同じ。`resolve(_, "macos", "x86_64")` は今どおり `the macOS toolchain requires ARM64`。今の `the_bundled_launcher_rejects_intel_macos_hosts_and_targets` は、ホストの拒否（467-478 行）を残し、target の拒否（479-496 行）を「表にある `x86_64-apple-macosx12.0.0` は写し、表にない `x86_64-apple-darwin27.0.0` などは `zig` が `None`」の検査に改める（名前も実際の検査に合わせて改める）。
  - `x86_64_apple_darwin_ir_matches_linux_x64`: fixture と examples の `--emit llvm` が `--target x86_64-unknown-linux-gnu` と byte 単位で一致する。
  - 偽の clang の記録で、`--target=x86_64-apple-macosx12.0.0` が渡り、macOS のホストでは `-fuse-ld=lld` が付かず、Linux のホストでは付く。
  - キャッシュ: `x86_64-apple-darwin` と `aarch64-apple-darwin` の成果物を取り違えない（今の `a_different_triple_or_sysroot_never_reuses_a_cached_build` に triple を足す）。
- Phase 2: `universal_target_parses_and_rejects_runs`（`build --emit exe|shared|object` を受け、`run`・`test`・`bench`・`--emit llvm`・`-g` は `universal_requires_build`）、`universal_build_links_both_slices_then_lipo`（偽の clang と偽の `lipo`）、`lipo` がないときの `universal_lipo_failed`。

### E2E（`tests/targets.mjs`）

- macOS のホスト（Apple の clang）: `examples/tzroutines`・`examples/control` を `--target x86_64-apple-darwin` で `-O0`・`-O3` にビルドし、Mach-O の CPU の型が x86_64（`0x01000007`）、`LC_BUILD_VERSION` の `minos` が 12、`codesign --verify --strict` が通る。Rosetta があれば `TSUZURI_RUNNER='arch -x86_64'` で `run` と `test` を実行し、出力が aarch64 の出力（`21325334000`・`42`）と一致する。
- 数値・トラップ・tzroutine の代表: `tests/fixtures/*/Main.tz` と `examples/*/Main.tz` のうち `main` を持ち OS の API を使わないものを、Rosetta の x86_64 と native の aarch64 の両方で走らせ、標準出力と終了コードを比べる（`-O0`・`-O3`。期待値は aarch64 の結果で、fixture に期待値のファイルはない）。全 suite を x86_64 で回す口（suite に target を渡す仕組み）は今ないので、Phase 1 はこの範囲にする。全 suite は D2 の案 b でだけできる。
- Linux のホスト: `TSUZURI_CROSS_LINK=1` で同梱の toolchain が x86_64 の Mach-O をリンクし、形式を読み返す。
- Phase 2（macOS）: `examples/tzroutines` の `--target universal2-apple-darwin` の出力の `lipo -info` が `x86_64 arm64`、そのまま実行して `21325334000`、`lipo -thin arm64` の結果が `--target aarch64-apple-darwin` の出力と一致、`codesign --verify --strict` が通る。Rosetta があれば `arch -x86_64` で x86_64 のスライスも実行する。`--emit shared` の fat の dylib を C のホストが両方の CPU でリンクでき、`--emit object` の fat の `.o` を `clang -arch arm64`・`-arch x86_64` のリンクに渡せる。

### 比較

- 既存の 9 個の target の `--emit llvm`・object と、`--target native` の出力が、手順 1 の基準と byte 単位で一致する。比べた件数を PR に書く。

## ドキュメント

- `_tsuzuri/language-reference/compiler/platforms.md`: 冒頭の対応環境の段落（「ネイティブ出力の対応環境は 5 つ」に、出力の target としての x86_64 の macOS を足し、ホストは 5 つのままと書く）、13 行の「9 個」、15・17 行、23・96 行の同梱ツールチェーンがクロスリンクできる「7 個」、43-45 行の誤りの例の名前の一覧、tier の表（D2 の行）、OS ごとの違いの表、診断の表、確かめた範囲。Phase 2 は「Universal 2」の節。
- `README.md:877`（対応 OS / CPU）。`docs/architecture.md:443-450`（Target と Platform）、`docs/language.md:4951-4954`。`_tsuzuri/language-reference/built-in-types-and-modules/net.md:381` の対象外の表。
- `_tsuzuri/language-reference/compiler/option.md` の 14・91 行の「9 個の native の名前」（Phase 2 は `universal2-apple-darwin` と、`TSUZURI_LIPO` を環境変数の表（`TSUZURI_DSYMUTIL` の行の近く）にも）。
- `_features/W36-c-layout-by-value.md:19` の M18 の注記を、出荷した形に改める。
- 文書の主張は CI の run の記録の範囲だけにする（Rosetta で実行したか、リンクだけか）。

## 受け入れ条件

- [ ] 方針を改める決定（D-67 の分岐 3 は「改めない」に決まったので、それを改める新しい決定）と D2・D3 の承認が記録されている。
- [ ] 仕様 1 のとおりに `x86_64-apple-darwin` をビルドでき、Rosetta か Intel の機械で `-O0`・`-O3` の `run`・`test` が aarch64 と同じ出力を出す（実行した手段を記録した）。
- [ ] x86_64 の `--emit llvm` が `x86_64-unknown-linux-gnu` と一致し、既存の 9 個の target の出力が変わらない（比べた件数を記録した）。
- [ ] macOS のホストの x86_64 の Mach-O が `codesign --verify --strict` を通る。
- [ ] M02 が done なら、x86_64 の送信の型が「現状」の参照の表と一致する。
- [ ] Phase 2 を実装したなら、仕様 4 のとおりに Universal 2 ができ、arm64 のスライスが thin の出力と一致する。
- [ ] ホストの 5 環境（`tests/platforms.mjs`）は変わらない。新しい診断コードを足していない。文書を直し、T2 が通り、PR に `ci:full` を付けた。

## 落とし穴

- **ホストを足さない**。`Triple::host()`、`src/lib.rs` の `compile_error!`、launcher のホストの拒否、配布物と VS Code のホストは変えない。Intel のホストでコンパイラを動かす経路を開くと、`tests/platforms.mjs` と 3 つの workflow の matrix の検査、fast backend の x86_64 の Mach-O（`src/backend/target.rs:79`）が届くようになる。
- **x86_64 は Apple の ld が署名しない**（「現状」）。署名しないまま Universal 2 にすると、`codesign --verify --strict` が fat のファイル全体で失敗する。`-Wl,-adhoc_codesign` は Apple の ld では効くが、同梱の zig が拒むので使わない（D6）。
- **fat のファイルを署名し直さない**。`codesign -s - -f` を fat にかけると arm64 のスライスの署名も替わり、thin の出力と一致しなくなる（S5 の検査ができない）。
- **AVX-512 の検出**。`src/runtime/cpu.c` は XCR0 で AVX-512 の状態を判定する。macOS は AVX-512 の状態を最初の使用で有効にするので、XCR0 に出ず、`avx512` の版が選ばれない可能性がある（LLVM の compiler-rt の `cpu_model` は Darwin を別に扱っている。着手時に確かめる）。誤った版を選ぶのではなく、遅い版に落ちる方向の問題である。性能の主張は Intel の実機で測ってからにする。
- **Rosetta の CPU の機能**。Rosetta 2 が報告する機能（AVX・AVX2・F16C の有無）は実機と違う。Rosetta の実行で確かめられるのは意味の一致で、`@cpu` の版の選択や性能ではない。
- **`objc_msgSend_stret` は x86_64 だけ**。SDK のヘッダーは `OBJC_ARM64_UNAVAILABLE` で宣言し、SDK の `libobjc.tbd` は `_stret` の 3 つを x86_64 の target の節にだけ export する（確かめた）。arm64 のビルドに出すとリンクで未定義になる。分岐を `current_platform()` の x86_64 に限る。
- **BOOL の正規化**。x86_64 の BOOL は `signed char` で、メソッドが 2 や -1 を返すことがある。`i8` を `trunc` して `i1` にすると 2 が false になる。`icmp ne 0` にする。
- **Universal 2 の 2 回のビルドは IR が違う**。`@cpu` の版と `Math.fma` が CPU で違うので、1 つの IR を 2 回コンパイルしない。`PlatformScope` を target ごとに張る今の `build_complete` を 2 回呼ぶ。
- **`/usr/bin/lipo` は CLT の shim**。CLT のない Mac では、起動すると導入を促すことがある。テストは偽の `lipo` を `TSUZURI_LIPO` で渡す。

## 対象外

- 32-bit（i386）と PowerPC。
- macOS 12 より前（`aarch64-apple-darwin` と同じ下限）。
- Rosetta の上や Intel Mac の上でコンパイラ自身を動かすこと。Intel Mac の配布物（`darwin-x64`）。
- Intel Mac の VS Code の拡張。
- bindgen の Intel の macOS の clang（`src/bindgen.rs:306-315`）。
- `--emit app`（M15）の Universal 2。M15 か後のチケットで決める。
- iOS・Mac Catalyst など macOS 以外の Apple の target。
- fast backend の x86_64 の Mach-O（名前付きの `--target` の fast backend は今どおり `E2000`）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | D-67 の分岐 3: d719cf68 の方針を改め、`x86_64-apple-darwin` を出力の target として戻す。ホスト（コンパイラ・配布物・VS Code・CI の 5 環境）は変えない。分岐 3 で改めないと決まったので、このチケットは方針の記録として残す | Intel Mac の利用者に配るには、出力の target と Universal 2 が要る。ホストまで戻すと、CI の時間（d719cf68 の前は 1 回約 220 分）と配布物の維持が戻る。分岐 3 は推奨の案の「改めない」に決まった: Apple の Intel 対応は macOS 26 で終わるとされ（リポジトリの外の事実）、対象の利用者は減る | 却下（2026-11-26。D-67 の分岐 3） |
| D2 | tier 2 にする（案 a）。CI は 3 つの job でリンクと形式を検査し、`macos-15` の job で Rosetta 2 を使って `-O0`・`-O3` の `build`・`run`・`test` を実行する。Intel の runner を足さない | d719cf68 の前に `macos-15` の job で Rosetta の実行が動いていた（「現状」。そのときは `examples/tasks` の `run` 1 回だけで、`-O0`・`-O3` の `build`・`run`・`test` への拡張はこのチケットで足す）。退けた案: (b) tier 1（`macos-15-intel` の runner）。その runner の上でコンパイラを動かすには Intel のホストを戻すか（D1 と矛盾）、arm64 でビルドした出力を別の job で走らせる仕組みが要り、時間も約 220 分かかる。(c) リンクだけ（実行しない）。BOOL・`_stret`・System V の差は実行しないと確かめられない | 要承認（承認前は手順 5 に着手しない） |
| D3 | Universal 2 の綴りは `--target universal2-apple-darwin`（D-55）。`lipo` は `TSUZURI_LIPO`（D-55）、配布物の `bin/`、`PATH` の順に探す | 「どこで動く出力か」を選ぶのは `--target` の役目で、1 つの値で表せる。`universal2` は Python の wheel の `macosx_11_0_universal2` と同じ語で、古い universal（PowerPC・i386）と区別できる。`TSUZURI_DSYMUTIL` と同じ探し方。退けた案: 専用のフラグ `--universal`（`--target` との組み合わせの規則が要る）、`--target` の繰り返し（今は同じオプションの 2 回目が `E2000`）、`universal-apple-darwin`（古い universal と紛れる） | 要承認（D-55。承認前は Phase 2 に着手しない） |
| D4 | M02 の ObjC の x86_64 の差（BOOL の `i8 signext` と正規化、メモリーで返す構造体の `objc_msgSend_stret`・`objc_msgSendSuper2_stret`）は、M02 の下げ方の target ごとの分岐として足す。`fpret` は使わない | 差は clang の参照で確かめた 2 点だけで、セレクターやモジュールのフラグは同じ。M02 の型付きの送信の上に分岐を置けば、arm64 の IR を変えない | 既定案（実装者はこの案に従う） |
| D5 | ランタイムの C は変えない。tzroutine は今の x86-64 System V の文脈の切り替えと、Apple の x86_64 の mcontext の分岐を使う。動かなければ S1 で止める | 実験で x86_64 の Mach-O にコンパイル・リンクでき、d719cf68 の前の CI はこの経路を `macos-15-intel` と Rosetta で実行していた | 既定案（実装者はこの案に従う） |
| D6 | x86_64 の Mach-O の実行ファイルと共有ライブラリは、macOS のホストでリンクの直後に `codesign -s -` で ad-hoc に署名する。macOS でないホストでは署名しない | Apple の ld は x86_64 を署名せず（実測）、Universal 2 の検査が失敗する。arm64 の出力が linker-signed であることとそろえる。`codesign` は OS の一部で、CLT を要らない。Intel と Rosetta の実行に署名は要らないので、macOS でないホストの未署名は実行を妨げない（配布の署名は M16）。退けた案: `-Wl,-adhoc_codesign`（同梱の zig が拒む）、fat のファイルを `lipo` の後に署名する（arm64 のスライスが thin の出力と変わる） | 既定案（実装者はこの案に従う） |
| D7 | x86_64 の実行は `TSUZURI_RUNNER` で明示する（例 `arch -x86_64`）。コンパイラは Rosetta を自動で選ばない | 「コンパイラは実行器を取得も導入もしない」という今の規則（platforms.md）と同じ。Rosetta の有無で `run` の挙動が黙って変わらない | 既定案（実装者はこの案に従う） |
| D8 | Universal 2 は `build` の `exe`・`shared`・`object` だけ。各スライスは今のキャッシュを使い、`lipo` は毎回走らせる。`-g` は Phase 2 では `E2000` | fat の実行ファイル・dylib・`.o` は Apple の ld が読む（実測で fat の `.o` を確かめた）。`run`・`test` は 1 つの CPU で走るので thin の target で足りる。`-g` の `.dwarf` を fat にまとめる手順は、M15 の `.dSYM` と合わせて決める | 既定案（実装者はこの案に従う） |
| D9 | `Triple::ALL` の末尾に足し、Clang の target は `x86_64-apple-macosx12.0.0`、最低版は 12.0（M17 があればその版） | 末尾なら、既存の診断の一覧の順を保ち、増えるのは末尾の 1 つだけ。Clang の target と最低版は d719cf68 の前と同じで、`aarch64-apple-darwin` とそろう | 既定案（実装者はこの案に従う） |
| D10 | 新しい診断コードを割り当てない。新しい kind は Phase 2 の `E2000` の `universal_requires_build` と `E2002` の `universal_lipo_failed` だけ | D-51、E12 D12 | 既定案（実装者はこの案に従う） |
