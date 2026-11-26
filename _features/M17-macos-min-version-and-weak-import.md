# M17: 最低 OS 版の指定（`[macos] min_version`）と弱い import（`@weak`）

| 項目 | 内容 |
| --- | --- |
| ID | M17 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | ([M22](M22-attribute-after-bodyless-extern-def.md)), ([M01](M01-objc-class-types.md)), ([M06](M06-extern-data-symbols-and-zutf8string.md))（M22 が後なら `weak` を `signature` の先読みに足す。M01・M06 は `@weak` をクラスとデータシンボルに付ける段） |
| 後続 | –（弱い後続: [M07](M07-apple-sdk-bindgen-and-package.md), [M15](M15-macos-app-bundle.md), [M18](M18-intel-mac-and-universal2.md)） |
| 状態 | todo |
| 起票 | 2026-11-26（D-67。利用者の依頼「「どのチケットにもない壁」と「調査中に見つけたコンパイラの不具合（自分で再現済み）」のそれぞれについて @_features/ に開発チケットまたは修正チケットを作成して」から起票。調査で報告した「配布 2（既定の native ビルドの minos がビルド機の版 27.2 になり、古い macOS で起動しない。版を選ぶ正式な手段も弱い import もない）」に当たる。HEAD `804b7970` で確認） |
| 承認 | [GUIDE D-67](GUIDE.md#d-67-tsuzuri-だけで-macos-のデスクトップアプリを書くm01m22) の分岐 6（既定の native ビルドの最低 OS 版。12.0 に固定）は承認済み（2026-11-26）。決定事項 D1（分岐 6）は承認済み。要承認: D-55（`[macos]` の節とキー `min_version`、属性 `@weak`）。決定事項 D2（`[macos]`・`min_version`）・D3（`@weak` と検査の形、版の問い合わせの API）が要承認、ほかは既定案 |
| 改善する劣位 | C（clang の `-mmacosx-version-min` と `__attribute__((weak_import))`）・Swift（Deployment Target と `#available`）比: 最低 OS 版を manifest で選べず、既定の native ビルドがビルド機の版（実測 27.2）になって古い macOS で起動しない。新しい OS にだけある関数を参照すると、呼ばない経路でも古い OS で dyld が起動を止める |
| 手本にする既存実装 | [W09](_completed/W09-windows-gui-subsystem.md) の `[windows]`（`src/package.rs` の `WindowsSettings`・`parse_manifest`、`src/driver.rs` の `with_manifest_windows`）、`src/target.rs` の `Triple::clang_target`、`src/driver.rs` の `compile_args`・`target_args`、`scripts/toolchain/clang.rs` の `resolve`、`src/cache.rs` の `runtime_key`、[W17](W17-link-attribute-dllimport.md) の `@link`（属性、`signature` の先読み、symbol ごとの `declare` の表）、`src/llvm.rs` の `@tz.callback.after_main`（報告してからトラップする） |
| 主な影響ファイル | `src/package.rs`, `src/driver.rs`, `src/target.rs`, `src/main.rs`, `src/test_runner.rs`, `scripts/toolchain/clang.rs`, `scripts/toolchain/runtime.mjs`, `src/syntax.rs`, `src/parser.rs`, `src/syntax_codec.rs`, `src/frontend_cache.rs`, `src/formatter.rs`, `src/check.rs`, `src/llvm.rs`, `src/llvm_imports.rs`, `src/effects.rs`, `src/capabilities.rs`, `src/diagnostic_kinds.rs`, `std/Env.tz`（D3）, `src/runtime/os.c`（D3）, `src/runtime/os-windows.h`（D3）, `src/runtime/os-wasi.c`（D3）, `src/llvm_io.rs`（D3）, `src/polymorph.rs`（D3）, `src/pinned_rules.rs`（D3）, `tests/capabilities.rs`（D3）, `tests/fixtures/capabilities/std-table.txt`（D3）, `tests/fixtures/effects_sweep/Main.tz`（D3）, `tests/os.mjs`（D3）, `tests/os-wasi-host.mjs`（D3）, `_tsuzuri/language-reference/built-in-types-and-modules/env.md`（D3）, `tests/macos_min_version.rs`（新規）, `tests/weak_import.rs`（新規）, `tests/weak_import.mjs`（新規）, `tests/targets.rs`, `tests/targets.mjs`, `tests/packages.rs`, `docs/stability.md`, `docs/language.md`, `_tsuzuri/language-reference/compiler/platforms.md`, `_tsuzuri/language-reference/organizing-tsuzuri/packages.md`, `_tsuzuri/language-reference/values-and-functions/attributes.md`, `_tsuzuri/language-reference/compiler/option.md` |

## 実装者向け早見

- 範囲:
  - ルートの `Tsuzuri.toml` の `[macos] min_version = "13.0"`。有効な版は manifest、`MACOSX_DEPLOYMENT_TARGET`、既定（D1）の順に決め、macOS の native・`--target aarch64-apple-darwin`・同梱の zig・ランタイムの object とその鍵・`--emit object`・`shared` のすべてに効かせる。
  - `@weak` を `extern "sym" def`・`extern class`（[M01](M01-objc-class-types.md)）・データシンボル（[M06](M06-extern-data-symbols-and-zutf8string.md)）の前に置ける。`extern_weak` に下がり、無い関数を呼ぶと `Tsuzuri runtime: the weak symbol '<sym>' is not available on this system` と報告して `TrapKind::Assert` でトラップする。Apple の target だけ。
- 最初に読む: 「現状」の表 → 仕様 2・3 → `src/driver.rs:1499-1529` → `scripts/toolchain/clang.rs:17-99` → 仕様 5・6。
- 最初に書くテスト: `tests/macos_min_version.rs` の `manifest_min_version_parses`・`effective_version_reaches_clang` → `tests/weak_import.rs` の `weak_extern_lowers_to_extern_weak`。
- 触ってはいけない: Linux・Windows・WASM の出力、すべての `--emit llvm`・`header`、Apple clang で `MACOSX_DEPLOYMENT_TARGET` を設定した native のビルド、`[windows]`・`[native]` の規則、`@weak` のないプログラムの IR、`Env.os_version` に到達しないプログラムのランタイムの object（D3）。D1 が変えるのは macOS の `LC_BUILD_VERSION` だけ。
- 完了の見分け方:
  - `[macos]` に `min_version = "13.0"` を書いたパッケージを、native・`--target aarch64-apple-darwin`・同梱の zig・`--emit object` でビルドすると、`otool -l` の `minos` がすべて 13.0 で、ld の `was built for newer 'macOS' version` の警告が出ない。
  - 次を `m17_new_api` を持つ dylib とリンクすると `./app x` は 7。dylib を同じ install name で `m17_new_api` のないものに差し替えると、`./app` は 3（今は dyld が `Symbol not found` で起動を止め、終了コード 134）、`./app x` は報告の 1 行を出して終了コード 133（SIGTRAP）。`args` はプログラム名を含まない（`./app` で 0、`./app x` で 1）。

```tsuzuri
@trusted
@weak
extern "m17_new_api" def new_api :: unit -> i32

def main :: Array<string> -> i32 = \args ->
    if args.length > 0 then new_api () else 3
```

## 目的

macOS の実行ファイルは `LC_BUILD_VERSION` の `minos` より古い OS では起動しない。今の既定の native ビルドはビルド機の版を `minos` に書くので、作ったアプリを古い Mac に渡すと起動しない。版を選ぶ正式な手段は、名前付きの target（12.0 に固定）か、Apple clang が読む `MACOSX_DEPLOYMENT_TARGET`（名前付きの target と同梱の zig では効かない）しかない。

新しい OS の API を使いながら古い OS でも動くアプリは、その symbol を弱い参照にし、無ければ使わない。Tsuzuri には弱い参照を宣言する口がない。

このチケットは、最低 OS 版を manifest に書けるようにしてすべての経路に同じ版を効かせ、既定の版を D-67 の分岐 6 で決めた 12.0 にし（2026-11-26）、`@weak` と呼び出しの地点の検査を足す。[M15](M15-macos-app-bundle.md) の `LSMinimumSystemVersion` と [M07](M07-apple-sdk-bindgen-and-package.md) の availability の写し方がこれを使う。

## 着手条件と停止条件

### 着手条件

- `node scripts/repo-kb.mjs brief M17` を読む。
- 承認が記録されるまで、該当する手順に着手しない（Phase はない）: 手順 2・3 は D2、手順 4〜7 は D3。手順 3 の既定の変更（D1。D-67 の分岐 6）は承認済み（2026-11-26）。
- [M22](M22-attribute-after-bodyless-extern-def.md) が done でなければ、`signature` の先読み（`src/parser.rs:2397` の `"cpu" | "trusted" | "count"`）に `weak` を足す（M22 が後なら M22 が消す）。[M06](M06-extern-data-symbols-and-zutf8string.md) の Phase 1・[M01](M01-objc-class-types.md) が done ならデータシンボル・クラスの段も実装し、後なら後のチケットが足す。確認: `grep -nE "^\| (M01|M06|M22) " _features/README.md`。
- `scripts/toolchain/` を変えるので `.github/instructions/ci.instructions.md` を読み、PR に `ci:full` を付ける。
- 基準の保存: `tests/fixtures/*/Main.tz` と `examples/*/Main.tz` の `--emit llvm -O1`（native と `wasm32`）と診断、macOS の native・`--target aarch64-apple-darwin` の実行ファイルの `LC_BUILD_VERSION` を `target/scratch/m17/base/` に保存する。D3 の `Env.os_version` を実装するときは、`Env.current_dir` だけを使うプログラムの `--emit object`（native の `-O0`・`-O3`）と `--target wasm32 --wasm-host wasi` の `.wasm` も保存する。macOS では D1 の既定の版の変化を除くため、前後とも `MACOSX_DEPLOYMENT_TARGET=13.0` を設定して Apple clang でビルドする（仕様 2・3 の 2 段目で、clang の引数は今と同じ）。

### 停止条件

- **S1** `[macos]`・`@weak`・`Env.os_version` のどれも使わないプログラムで、IR・診断、Linux・Windows・WASM の出力、または仕様 4 の表の外の macOS の出力が変わる（`Env.os_version` の define の門で、OS API に到達するプログラムのランタイムの object が変わる場合を含む）。
- **S2** 同梱の zig が `aarch64-macos.<版>`（12.0 以外）でリンクできない、または prebuilt のランタイムと版が合わず ld が警告する。
- **S3** コンパイラが生成した IR で、Apple の ld か同梱の zig（lld）が `declare extern_weak` を weak import の束縛（`dyld_info -fixups` の `[weak-import]`）にしない（手で書いた IR では両方ともなった。「現状」の「弱い import」）。
- **S4** 新しい `TrapKind` か診断コードが要るようになった。
- **S5** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `804b7970` で確認）

### 最低 OS 版の実測

macOS 27.2.1（arm64）、Apple clang 21、SDK 27.2。`IO.writeln "hello"` の `main` を `target/debug/tsuzuri build hello -o <名前>` でビルドし、`otool -l <名前> | grep -A4 LC_BUILD_VERSION` を読んだ。

| ビルド | minos | sdk |
| --- | --- | --- |
| native（実行ファイル、`--emit object`、`--emit shared`） | 27.2 | 27.2 |
| `--target aarch64-apple-darwin` | 12.0 | 27.2 |
| native、`MACOSX_DEPLOYMENT_TARGET=13.0` | 13.0 | 27.2 |
| `--target aarch64-apple-darwin`、`MACOSX_DEPLOYMENT_TARGET=13.0` | 12.0（効かない） | 27.2 |
| native、`TSUZURI_CLANG=<VS Code 拡張 0.1.6 の同梱の tsuzuri-clang>`（環境変数の有無によらない） | 12.0 | 26.4 |
| native、`MACOSX_DEPLOYMENT_TARGET=10.15` | 11.0（arm64 の最小） | 27.2 |

- native と `--target aarch64-apple-darwin` の `--emit llvm` は `cmp` で同じ。IR は `target triple` を持たず、版は Clang の引数だけで決まる。
- `MACOSX_DEPLOYMENT_TARGET=banana` の native は `error[E2002]: 'clang' failed (exit status: 1): clang: error: invalid version number in 'MACOSX_DEPLOYMENT_TARGET=0'`。
- native の `--emit object` を `clang -mmacosx-version-min=13.0 host.c lib.o` でリンクすると `ld: warning: object file (…/lib.o) was built for newer 'macOS' version (27.2) than being linked (13.0)`。
- Apple clang の `-mmacosx-version-min=13.0` は `MACOSX_DEPLOYMENT_TARGET=14.0` より優先し（minos 13.0）、`-x ir` の compile でも効いた。
- 同梱の zig（0.1.6 の `toolchain/zig/zig`）を直接 `zig cc -target aarch64-macos.13.0 m.c` で使うと minos 13.0（sdk 26.4）になった。zig 自体は 12.0 以外の版を受ける。
- 0.1.6 の prebuilt のランタイム（`share/runtime/manifest.json`）の native の引数は `-x ir -Wno-override-module -O<n> -fPIC -ffunction-sections -fdata-sections` で、`environment` の `MACOSX_DEPLOYMENT_TARGET` は `null`。環境変数を設定したビルドは prebuilt を使わない。

### コード

- `src/target.rs:106-112`: `aarch64-apple-darwin` の Clang の target は `arm64-apple-macosx12.0.0`（`clang_target`、186 行）。platforms.md:39・60。
- `src/driver.rs:1499-1509` の `native_compile_args` は Windows でない native に `-fPIC` だけを渡す。`compile_args`（1515-1529 行）は名前付きの target に `--target=<clang_target>` を足す。実行ファイルは IR の compile とリンクを 1 回の Clang でするので `compile_args` が両方に効く（5255 行）。リンクだけの run（`-g` の 5426・5490 行、`--emit shared` の 5762 行）は `target_args`（1570-1576 行）だけを使い、native では空である。単体テスト（8747-8753 行）は native の `compile_args` が `native_compile_args` と同じで、`target_args` が空であることを固定する。`src/test_runner.rs` は driver の子のモジュールで、`compile_args` を 4 か所（513・530・543・1039 行。リンクを含む）で使う。
- `scripts/toolchain/clang.rs`: `NATIVE_TARGETS`（20-30 行）は `("arm64-apple-macosx12.0.0", "aarch64-macos.12.0")` を文字列の一致で引き（21 行）、target のない run は macOS で `("macos.12.0", "apple-macosx12.0.0")`（82 行）。表にない target は `zig: None` でリンクが失敗する。IR の compile は `-m` で始まる引数を LLVM の clang に渡す。`tests/targets.rs:416-456` が対応を固定する。
- `scripts/toolchain/runtime.mjs:43`: prebuilt のランタイムの native の引数は `['-fPIC']`。`driver::compile_runtime_object` の引数と一致しなければ使われない（`src/runtime_cache.rs:201-275`）。
- 鍵: `MACOSX_DEPLOYMENT_TARGET` は `src/cache.rs:31`（`KEY_ENVIRONMENT`）と `src/runtime_cache.rs:12`。`runtime_key`（`src/cache.rs:279-306`）と `unit_key_prefix` は Clang の引数を鍵に入れ、`build_key`（494 行）は manifest の本文を入れる。`tsuzuri serve` は `KEY_ENVIRONMENT` を比べる（`src/server.rs:373-382`）。option.md:724。
- `tests/targets.rs:280-290`・`336-337` と `tests/targets.mjs:42`・`104-134` は名前付きの target の minos 12 を検査する。native の minos のテストはない。

### manifest

- `src/package.rs:728-750` の `parse_manifest` は `[package]` の後の 6 つの節を各 1 回だけ受ける。順序の誤りの文 `expected [package] followed by optional [dependencies], [wasm], [native], [registry], [capabilities] and [windows], each once` は `src/package.rs:748`・`1494`・`1583` と `tests/packages.rs:1484` が固定する。
- W09 の `[windows]`: `WindowsSettings`（`src/package.rs:112-116`）、`"windows"` の腕（940-965 行。`invalid_windows_setting`・`unknown_manifest_key`・`duplicate_manifest_key`）、`with_manifest_windows`（`src/driver.rs:1165-1175`、`src/main.rs:4206`）。packages.md:327「依存パッケージの `[windows]` は読みません」。

### 弱い import

- 構文がない。`@trusted` の次の行の `@weak` は `1:1: error[E1037]: '@trusted' applies to an extern declaration; remove it or declare the function with 'extern'`、`@weak` が先なら `1:1: error[E0002]: expected an expression; '@checked' is the only attribute of an expression`。
- `extern_weak` は IR を読む側にだけ現れる。PB07 の `linkage_words` は global の `extern_weak` を `Unsupported` にし（`src/llvm_split.rs:589`）、-O0 のビルドは黙って分割なしに戻る（`src/driver/units.rs:281-284`）。`declare` の行は名前だけを読み（`src/llvm_split.rs:789-800`）、使う unit へ本文を書き換えて写す（1715-1723 行）ので、関数の `declare extern_weak` は分割を外さず、linkage の語も残るはずである（コードを読んだだけで未確認。手順 5 で確かめる）。`--backend fast` は `extern_weak` を `bail!` し（`src/backend/module.rs:198`）、`E2002` `fast_backend_unsupported_construct`（`src/backend/error.rs:39`）になる。
- dylib の差し替えの実験: `new.c`（`m17_old_api` と、7 を返す `m17_new_api`）と `old.c`（`m17_old_api` だけ）を、同じ絶対パスの install name で `clang -dynamiclib` にした。
  - 早見のプログラムから `@weak` を除き、条件を `args.length > 5` にして `-L . -l m17w` でビルドすると、`new.c` の dylib では 3。`old.c` に差し替えると、起動の時点で `dyld[…]: Symbol not found: _m17_new_api` を出して終了コード 134。
  - C の `__attribute__((weak_import))` では `dyld_info -fixups` が `bind libm17w/_m17_new_api [weak-import]` で、差し替えた後は関数のアドレスが 0 になり、起動できた。
  - Apple の ld は、どのライブラリにもない symbol への弱い参照を拒む（C の `weak_import` と、`declare extern_weak i32 @m17_typo_api()` を呼ぶ IR の両方で `Undefined symbols for architecture arm64: "_m17_typo_api"`）。
  - 仕様 6 の形を手で書いた IR で確かめた。次を `clang -O3 w.ll -L . -l m17w` と、VS Code 拡張 0.1.6 の同梱の `tsuzuri-clang`（IR の compile の後に zig の lld でリンク）でリンクした。どちらも `dyld_info -fixups` が `bind libm17w/_m17_new_api [weak-import]` を示し、`new.c` の dylib で 7、`old.c` に差し替えると 3 を返した（`-O3` でも NULL の検査は残る）。

```llvm
declare extern_weak i32 @m17_new_api()
define i32 @main(i32 %argc, ptr %argv) {
  %p = icmp ne ptr @m17_new_api, null
  br i1 %p, label %call, label %none
call:
  %r = call i32 @m17_new_api()
  ret i32 %r
none:
  ret i32 3
}
```
- 報告の前例: `src/llvm.rs:2240-2259` の `@tz.callback.after_main` は `@tz.trap.write`（`src/llvm_traps.rs:931`）で `Tsuzuri runtime: …` を書いてから `llvm.trap` を呼ぶ。トラップは既定では文を出さず終了コード 133、`--trap-info` では `trap: null extern handle at tr/Main.tz:4:23` の形（`NullHandle` で確かめた）。`Assert` の文は `assertion failed`（`src/trap.rs:56`）。
- CI の macOS は `macos-15` だけ（platforms.md:11・56・60）。

## 変えてはいけない意味

- Linux・Windows・WASM の出力、すべての `--emit llvm`・`header`、`[windows]`・`[native]` の規則。
- Apple clang で `MACOSX_DEPLOYMENT_TARGET` を設定した native のビルドの版。名前付きの target の、環境変数も manifest もないときの `arm64-apple-macosx12.0.0`（prebuilt とキャッシュの一致）。
- `@weak` のない extern の `declare` と呼び出し。トラップの終了コード（W07 の範囲）。
- `Env.os_version`（D3）に到達しないプログラムのランタイムの部分（実行ファイル、`--emit object`、`--wasm-host wasi` の `.wasm`）。OS API を使っていても変えない。`Os.__read` の操作の一覧（0〜5）も変えない。
- 例外: 仕様 4 の表の `LC_BUILD_VERSION`（D1。D-67 の分岐 6 で承認済み（2026-11-26））と、manifest の節の順序の誤りの文（`[macos]` を足す。W09 の `[windows]` と同じ）。

## 仕様

### 1. `[macos]` の節と `min_version`

```toml
[macos]
min_version = "13.0"
```

- ルートの `[macos]` だけを読み、依存パッケージの `[macos]` は読まない（`[windows]`・`[wasm]` と同じ。D2）。節は 1 回まで。[M15](M15-macos-app-bundle.md)・[M16](M16-macos-signing-notarization-hardening.md) がキーを足す（先に入ったチケットが節を作る）。
- 値は `"<major>[.<minor>[.<patch>]]"`（10 進の数字）で 12.0 以上。違えば `E0002`（kind 案 `invalid_macos_setting`、文案 `min_version must be a macOS version 12.0 or later, such as "13.0"`）。知らないキーは `unknown_manifest_key`（文案 `unknown macos key; expected min_version`）、重複は `duplicate_manifest_key`。
- macOS でない target のビルドでは使わない（誤りにもしない）。

### 2. 有効な版

macOS の target（Apple のホストの native と `aarch64-apple-darwin`）のビルドごとに 1 つ決める。

| 優先 | 出どころ | 検査 |
| --- | --- | --- |
| 1 | ルートの `[macos] min_version` | 仕様 1 |
| 2 | 空でない `MACOSX_DEPLOYMENT_TARGET` | Apple clang の native は clang に任せる（今と同じ）。名前付きの target ではコンパイラが読み、形の誤りと 12.0 より前は `E2000`（kind 案 `invalid_macos_deployment_target`、文案 `MACOSX_DEPLOYMENT_TARGET='{value}' must be a macOS version 12.0 or later for --target aarch64-apple-darwin`）。同梱の zig の native は launcher が読み、誤りは launcher の文の `E2002` |
| 3 | 既定（D1） | – |

### 3. 版を渡す先

| 経路 | 渡し方 |
| --- | --- |
| Apple clang の native | 1 か 3 で決まったとき `-mmacosx-version-min=<版>` を `compile_args`（IR とランタイムの C の compile、実行ファイルのリンク）と `target_args`（リンクだけの run）の両方に足す。2 のときは足さない（3 の既定は D1 の 12.0。ビルド機の版を既定に残す案は、D-67 の分岐 6 で選ばれなかった） |
| `--target aarch64-apple-darwin` | Clang の target を `arm64-apple-macosx<major>.<minor>.<patch>` にする（compile とリンク）。12.0 では今の文字列のまま |
| 同梱の zig（`scripts/toolchain/clang.rs`） | `arm64-apple-macosx<版>` を zig の `aarch64-macos.<版>` に写す（文字列の一致をやめる）。target のない run は `-mmacosx-version-min=` があればその版、なければ `MACOSX_DEPLOYMENT_TARGET`、なければ 12.0。LLVM の clang には `-target aarch64-apple-macosx<版>` だけを渡す |
| ランタイムの object | 上の引数で compile するので `runtime_key` が版を区別する。`scripts/toolchain/runtime.mjs:43` の native の引数をコンパイラと同じにする |
| `run`・`test`・`bench`、`--emit object`・`shared` | 同じ `compile_args` を通る |

ビルドの鍵にフィールドを足さない（manifest の本文と環境変数は入っていて、既定はコンパイラの版で決まる）。

### 4. 既定の版（D1。分岐 6）

D-67 の分岐 6 で、既定の版を 12.0 に固定すると決めた（2026-11-26。D1）。`[macos]` を書かないプログラムの変わる出力:

| ビルド | 今 | 後 |
| --- | --- | --- |
| Apple clang の native、環境変数なし | ビルド機の版（27.2） | 12.0 |
| `--target aarch64-apple-darwin`、環境変数あり | 12.0 | 環境変数の版 |
| 同梱の zig、環境変数あり | 12.0 | 環境変数の版 |

ほかの組み合わせと IR は変わらない。実装の PR が `docs/stability.md` の「既知の例外」に記録する。ビルド機の版のままにする案（1 行目を変えず、文書に配布の注意を書く）は、分岐 6 で選ばれなかった。

### 5. `@weak` の構文と位置

- `@weak` は引数を取らず、`@trusted`・`@link`（W17）・`@last_error`（W16）と任意の順に、同じ行か前の行に書ける。edition に依らない。
- 書ける位置: 明示の symbol を持つ `extern "sym" def`、`extern class`（M01）、データシンボル（M06 の `view def … :: unit -> T`・`address def`）。

| コード | kind（案） | 条件 |
| --- | --- | --- |
| `E0002` | `weak_attribute_syntax` | `@weak(…)`（文案 `'@weak' takes no arguments`） |
| `E0002` | `misplaced_weak_attribute` | `def`・`export def`・`extern type`・アドレスを取る extern view の前、1 つの宣言に 2 つ（文案 `'@weak' applies to an extern function with a link name, an extern class, or a data symbol`） |
| `E1008` | `weak_needs_symbol` | 明示の symbol のない `extern def`（文案 `'@weak' needs an explicit symbol; write extern "symbol" def, because the generated tsuzuri_host_ symbols belong to the host`） |
| `E1008` | `weak_data_symbol_result` | `@weak` のデータシンボルの結果が `Maybe<…>` でない |
| `E1037` | `trusted_no_trap_weak` | `@trusted no_trap` と `@weak`（文案 `'@trusted no_trap' contradicts '@weak': a call of a missing weak symbol traps`） |
| `E1008` | `abi_apple_only`（共通の kind 案） | Apple でない native の build で到達（文案 `'@weak' needs a Mach-O weak import, which only Apple targets have`） |
| `E1008` | `abi_native_only` | wasm の build で到達 |

到達しない宣言は拒まない。`check` は受理する。

### 6. `@weak` の意味（D3）

- symbol ごと: 同じ symbol の宣言のどれか 1 つに `@weak` があれば、その symbol の `declare`（W14 の 1 行）は `extern_weak` になり、すべての呼び出しが検査する。
- 関数: 呼び出しの地点で `icmp ne ptr @sym, null` を検査し、NULL なら `Tsuzuri runtime: the weak symbol 'sym' is not available on this system` を標準エラーに書いてから `TrapKind::Assert` でトラップする（`--trap-info` ではその後に `trap: assertion failed at <位置>`）。呼び出しは効果 `trap` を持つ。
- データシンボル（M06）: `@sym = extern_weak global [0 x i8]`。結果は `Maybe<T>`・`Maybe<H>` で、symbol がなければ `None`。トラップしない。
- クラス（M01）: `OBJC_CLASS_$_<実行時名>` を `extern_weak` にし、classref を読むたびに NULL を検査して同じ報告とトラップをする（M02 の送信は受け手を非 nil と仮定するので、classref で止める）。
- 版の問い合わせ: `Env.os_version :: own unit -> IO<Result<string, Os.Error>>`。権限は `env`（ほかの `Env` と同じ）。macOS は `sysctlbyname("kern.osproductversion")`（例 `27.2.1`）、Linux はカーネルの release（`uname` の `release`）、Windows は `RtlGetVersion` の `major.minor.build`。`--wasm-host wasi` は `Os.Error { kind: Other, code: 52 }`（ENOSYS。`Process.run` と M15 の `Env.executable_path` と同じ）、既定の wasm32 はほかの `Env` と同じく build の `E2000`。対象のない関数で D-59 の例外は要らない（`Env` は `SingleArgument`）。

## 設計

- **manifest**（`src/package.rs`）: `MacosSettings { min_version: Option<(MacosVersion, Span)> }` と `Manifest::macos`、節の判定と順序の誤りの文（3 か所と `tests/packages.rs:1484`）、`"macos"` の腕。`MacosVersion { major, minor, patch }` の `parse` と `Display` は `src/target.rs`。
- **版の決定**（`src/driver.rs`）: `Project::macos`（ルートだけ）、`BuildOptions::with_manifest_macos`（`with_manifest_windows` と同じ所で呼ぶ。`src/main.rs:4206`。単体テストは `src/driver.rs:8220`・`8229` の隣）、`effective_macos_version(platform, options) -> Result<MacosChoice, Diagnostic>`（`Explicit`・`FromEnvironment`・`Default`）を 1 か所で求める。`compile_args`・`target_args` は版を引数に取り、driver の呼び出し（4682・4772・4907・5010・5041・5255・5426・5490・5762・6803 行）と `src/test_runner.rs` の 4 か所がそれを渡す。8747-8753 行の単体テストは macOS で版の引数を足した形に直す。`Triple::clang_target` の固定の文字列の代わりに、版を取る関数で triple を作る。
- **launcher**（`scripts/toolchain/clang.rs`）: `resolve` が版を読み、zig と LLVM の target を作る（仕様 3）。`runtime.mjs:43` をコンパイラの既定の引数と同じにする。
- **構文と検査**: W17 の `LinkAttribute` と同じ脇のリスト `Program::weaks`、`FRONTEND_FORMAT` を 1 上げる、`src/formatter.rs` は保つ。`HostImport::weak`、`CheckedModule::weak_symbols: BTreeSet<String>`。`src/effects.rs` は `@weak` の呼び出しに `trap`。`src/capabilities.rs` の audit は行の末尾に `  weak`（JSON は `"weak":true`。ほかの行は今の文字列）。
- **`Env.os_version`**（D3 で採ったとき）: `Os.__read` に種類を足さず、独立した入口 `tsuzuri_os_version` と内部の組み込み `Os.__os_version :: unit -> (i64 * [ubyte])` にする（[M15](M15-macos-app-bundle.md) の D13 の `Env.executable_path` と同じ形）。scheme と下げ方は `Os.__args` と同じで、結果の置き場だけを渡す（`src/check.rs:3037-3051`、`src/llvm_io.rs:84` の `("args", "ptr")`。`declare i64 @tsuzuri_os_version(ptr)`）。`Builtin::OsArgs` を並べる一覧（`src/llvm.rs:8510・8816`、`src/polymorph.rs:4436`、`src/pinned_rules.rs:133`。`rg -n 'Builtin::OsArgs' src`）にも足す。
  - 本体は `src/runtime/os.c`（macOS は `sysctlbyname("kern.osproductversion")`、Linux は `uname` の `release`）、`src/runtime/os-windows.h`（`RtlGetVersion`）、`src/runtime/os-wasi.c`（`tz_status(TZ_OS_OTHER, TZ_WASI_NOSYS)`。`tsuzuri_os_spawn` と同じ）の `#if defined(TZ_OS_VERSION)` の中に置く。`<sys/sysctl.h>`・`<sys/utsname.h>` の include も同じ `#if` の中に置き、ファイルの先頭の include の並びを変えない。`RtlGetVersion` は ntdll.dll の関数で、今の Windows の OS のランタイムのリンクは `-lbcrypt` だけである（`src/driver.rs:5333`、`src/test_runner.rs:570・1092`）。リンク行を変えないよう、`GetModuleHandleW(L"ntdll.dll")` と `GetProcAddress` で引く（`src/runtime/gpu.c:122` の `GetProcAddress` が前例）。
  - IR がこの symbol を参照するとき（`runtime_symbols.contains("tsuzuri_os_version")`）だけ、3 か所で `-DTZ_OS_VERSION` を付ける。driver の native のランタイム（`-DTZ_TRAP_BOUNDARY` と同じ門。`src/driver.rs:4915-4923`）、driver の WASI のランタイム（`-DTZ_WASI_START` の隣。5089-5094 行）、`src/test_runner.rs` の `native_runtime_sources`（656 行）と `compile_native_runner`（1053-1059 行）である。test runner で付け忘れると、`tsuzuri test` のリンクが `tsuzuri_os_version` の未定義で失敗する。prebuilt のランタイムは `os.c` を含む単位を持たない（`scripts/toolchain/runtime.mjs:67-72`）ので、define は prebuilt の照合に影響しない。`#if` の節を足すと `os.c` の文字列が変わり、ランタイムのキャッシュの鍵（単位の文字列を含む。`src/cache.rs:279-306`）は 1 回だけ変わるが、到達しないプログラムの object は変わらない（M15 の落とし穴「define の門が守るのは object だけ」）。
  - `Os.__read` に種類を足すと、OS API に到達するすべてのプログラムのランタイムの object が変わるので採らない。権限は `env`: `src/capabilities.rs` の `primitive` に `"Os.__os_version" => Env` を足し、`every_private_builtin_is_classified` の 31 を 1 増やす。`tests/fixtures/capabilities/std-table.txt` に `Env.os_version: env` を足し、`tests/capabilities.rs` のホストに届く名前の一覧に `Os.__os_version` を足して数（`std_table_file_lists_the_88_functions` の 88、`Env.tz` の 3 と合計 44）を直す。公開の std の関数はどれもコーパスに実体が要る（`tests/effects_alloc_conservative.rs` の F.4）ので、`tests/fixtures/effects_sweep/Main.tz` に `Env.os_version ()` を足す。M15 の `Env.executable_path` と同じ数を直すので、後に入るほうが足す。
- **生成**（`src/llvm_imports.rs`・`src/llvm.rs`）: `declare` の行を作る所で `weak_symbols` にあれば `declare extern_weak`。`host_call` は呼び出しの前に検査を置き、失敗の枝で `call void @tz.weak.missing(ptr @tz.weak.name.<n>, i64 <len>)` と `emit_trap(TrapKind::Assert)`。`@tz.weak.missing` は `cold nounwind` の内部関数で、`@tz.trap.write` で前置き・名前・後置きを書く（`TRAP_WRITE` と `declare i64 @write(i32, ptr, i64)` は after_main と同じく、まだなければ足す）。名前の定数は symbol の名前の順。Apple でない native と wasm は `validate_wasm_externs` と同じ所で拒む。

## 前提とする他チケットのインターフェース

- [M22](M22-attribute-after-bodyless-extern-def.md)（弱い）: 本体のない `extern def` の直後の属性の一般の修正。
- [M06](M06-extern-data-symbols-and-zutf8string.md)・[M01](M01-objc-class-types.md)（弱い）: `HostImport.data` と `@sym = external global [0 x i8]` の行、classref の生成。
- [W14](_completed/W14-extern-multiple-declarations.md)（done）: symbol ごとに 1 行の `declare`。[W17](W17-link-attribute-dllimport.md)（todo）: 先に入っていれば `dllimport` と同じ「`declare` の行を作る 1 つの関数」に足す。

## 他チケットへ提供するインターフェース

- `MacosSettings`・`Project::macos`・`MacosVersion`・`effective_macos_version`。[M15](M15-macos-app-bundle.md) は `LSMinimumSystemVersion` をリンクした実行ファイルの `LC_BUILD_VERSION` の minos から作る（M15 の D10。M17 の後はこの関数の有効な版と一致し、M15 のテストが一致を確かめる）。M15・[M16](M16-macos-signing-notarization-hardening.md) が `[macos]` にキーを足す。[M18](M18-intel-mac-and-universal2.md) は、方針を改めて着手するときに x86_64 にも同じ版を渡す（D-67 の分岐 3 で改めないと決めたので、今は blocked）。
- `@weak`・`CheckedModule::weak_symbols`（[M07](M07-apple-sdk-bindgen-and-package.md) が availability から付ける）、`Env.os_version`（D3 で採ったとき）。

## 実装手順

1. **変更**: なし（下書きは `target/scratch/m17/`）。**内容**: 着手条件の基準の保存と「現状」の表の再測定。**確認**: 表と一致する。
2. **変更**: `src/package.rs`・`src/target.rs`・`src/driver.rs`・`src/main.rs`・`src/test_runner.rs`・`src/diagnostic_kinds.rs`・`tests/packages.rs`。**内容**: 仕様 1〜3 の Apple clang の経路。**確認**: T0、`manifest_min_version_parses`・`dependency_sections_are_not_read`・`effective_version_reaches_clang`、T1。
3. **変更**: `scripts/toolchain/clang.rs`・`scripts/toolchain/runtime.mjs`・`tests/targets.rs`・`tests/targets.mjs`。**内容**: launcher と prebuilt、仕様 4（D1 の既定）。**確認**: launcher のテストに `arm64-apple-macosx13.0.0` → `aarch64-macos.13.0` と target のない run の 3 通り。`node tests/targets.mjs target/debug/tsuzuri` と、`TSUZURI_CROSS_CLANG` に同梱の launcher を指した run。
4. **変更**: `src/syntax.rs`・`src/parser.rs`・`src/syntax_codec.rs`・`src/frontend_cache.rs`・`src/formatter.rs`・`src/check.rs`・`src/effects.rs`・`src/capabilities.rs`・`src/diagnostic_kinds.rs`。**内容**: 仕様 5、効果、audit。**確認**: `weak_attribute_positions`・`weak_effects_and_trust`、`tsuzuri fmt` が保つ。
5. **変更**: `src/llvm_imports.rs`・`src/llvm.rs`。**内容**: 仕様 6 の関数。**確認**: `weak_extern_lowers_to_extern_weak`・`weak_is_per_symbol`・`non_apple_targets_reject`、`TSUZURI_CODEGEN_UNITS=split` の -O0 で分割が外れるかの記録、`--backend fast` の `E2002`、基準との `cmp`（S1）。
6. **変更**: 同上（M06・M01 が done のとき）。**内容**: データシンボルとクラス。**確認**: `weak_data_symbol_is_maybe`・`weak_class_checks_each_read`。
7. **変更**: `std/Env.tz`・`src/runtime/os.c`・`src/runtime/os-windows.h`・`src/runtime/os-wasi.c`・`src/check.rs`・`src/llvm_io.rs`・`src/llvm.rs`・`src/polymorph.rs`・`src/pinned_rules.rs`・`src/capabilities.rs`・`src/driver.rs`・`src/test_runner.rs`・`tests/fixtures/capabilities/std-table.txt`・`tests/capabilities.rs`・`tests/fixtures/effects_sweep/Main.tz`・`tests/os.mjs`・`tests/os-wasi-host.mjs`（D3 で採ったとき）、`tests/weak_import.mjs`。**確認**: `node tests/weak_import.mjs target/debug/tsuzuri`（macOS、`-O0`・`-O3`、Apple の ld と同梱の zig）、`Env.os_version` の E2E（テスト計画）、手順 1 の基準の OS のプログラムの object と `.wasm` との `cmp`（S1）、`cargo test --locked --test capabilities --test std_argument_order --test effects_alloc_conservative`、`sh scripts/check-runtime-includes.sh`、`tsuzuri test --doc std`。
8. **変更**: 文書。**確認**: T2、`node scripts/check-docs.mjs <変更したページ>`、`node scripts/gen-docs.mjs --check`、`node scripts/check-cli-docs.mjs`。PR に `ci:full`。

## テスト計画

- `tests/macos_min_version.rs`（新規）: `manifest_min_version_parses`（`"12"`・`"13.0"`・`"14.1.2"` を受理、`"11.0"`・`"13.x"`・`""`・`"13.0.0.1"` は `invalid_macos_setting`、知らないキー・重複・2 つ目の節、順序の誤りの文）、`dependency_sections_are_not_read`（path の依存の `min_version = "99.0"` が効かない）、`effective_version_resolution`（仕様 2 の 3 段と `invalid_macos_deployment_target`）、`effective_version_reaches_clang`（`tests/targets.rs` の `fake_toolchain`（500 行。偽の clang が引数を `FAKE_LOG` に書く）の形で、compile とリンクの行に版。manifest の版を変えるとビルドとランタイムの鍵が変わる。Linux・Windows・wasm の引数は変わらない）。
- `tests/weak_import.rs`（新規）: `weak_attribute_positions`（仕様 5 の各行、本体のない extern の次の `@weak` が制約行と読まれない）、`weak_extern_lowers_to_extern_weak`（`declare extern_weak i32 @m17_new_api()`、呼び出しの前の `icmp`、`@tz.weak.missing` と名前の定数が 1 つずつ、2 回の生成が同じ）、`weak_is_per_symbol`、`non_apple_targets_reject`（`abi_apple_only`・`abi_native_only`、到達しなければ成功、`check` は 0）、`weak_effects_and_trust`（`@no_trap` の関数から呼ぶと `E1031`、`trusted_no_trap_weak`、audit の `weak`）、`weak_data_symbol_is_maybe`・`weak_class_checks_each_read`（M06・M01 が done のとき）、`unchanged_without_weak`。
- macOS の E2E: native・`--target aarch64-apple-darwin`・同梱の zig × manifest の 13.0・環境変数の 13.0・どちらもなし で、実行ファイルと `--emit object`・`shared` の minos を `mach_o_build_version` で読む。期待は仕様 2・4 の表から手で書く。版の違うビルドで ld の `built for newer` の警告がない。
- `tests/weak_import.mjs`（新規、macOS）: テストの中で `new.c`・`old.c` の dylib を作り、早見のとおり 7・3・報告と終了コード 133・`--trap-info` の `trap: assertion failed`。`dyld_info -fixups` の `[weak-import]`。どのライブラリにもない symbol の `@weak` はリンクの `E2002`。M06 が done ならデータシンボルの `None` と値。
- `Env.os_version`（D3 で採ったとき）: `tests/os.mjs` の「9. Env」の段に、native の `-O0`・`-O3` で、macOS は `sw_vers -productVersion` と一致、Linux は `uname -r` と一致、Windows は `<数>.<数>.<数>` の形。`tests/os-wasi-host.mjs` で `--wasm-host wasi` は `Other`（`code` 52）、既定の wasm32 は `E2000`。`Env.os_version` を呼ぶ `test` を持つ `tsuzuri test` が通る（`src/test_runner.rs` の define）。`TSUZURI_CLANG` に引数を記録するラッパーで、`Env.os_version` を呼ぶプログラムのランタイムのコンパイルにだけ `-DTZ_OS_VERSION` があり、`Env.current_dir` だけのプログラムの object と `.wasm` が基準と `cmp` で一致する（S1）。
- 古い macOS での実行は CI にない（`macos-15` だけ）。手で確かめたら版と結果を PR に記録する。

## ドキュメント

- `compiler/platforms.md` の 39 行を有効な版の決め方と既定に直し、配布の節に `[macos] min_version`。
- `organizing-tsuzuri/packages.md` の 27 行の節の一覧と「その他のセクション」に `[macos]`（ルートだけ、値の形、優先順）。
- `values-and-functions/attributes.md` の一覧の表と新しい節「@weak（弱い import）」（位置、検査とトラップ、Apple だけ、SDK にない symbol はリンクの誤り）。`compiler/native-interop.md` から案内。
- `compiler/option.md` の環境変数の節と 724 行。D3 で採れば `built-in-types-and-modules/env.md` に `Env.os_version`（公開 API の表、例、対応環境、WASI の `Other`）。今の env.md には「引数の順序」の節がない（AGENTS.md の Standard library API conventions）。M15 が先でなければ、この PR が節を置く（`Env` の関数はどれも引数が 1 つ。`argument-order.md` の「引数が 1 つの関数」）。`docs/language.md` の `Env` の段落と、`docs/architecture.md` の OS の組み込みの段（626 行）に `Os.__os_version` と到達したときだけの `-DTZ_OS_VERSION`。
- `compiler/diagnostics.md` の生成された表、`docs/language.md` の属性とリンク名の段落、`docs/stability.md` の「既知の例外」（D1）。

## 受け入れ条件

- [ ] 仕様 1〜4 のとおり、版が native・名前付きの target・同梱の zig・ランタイム・`--emit object`・`shared` に効く（`otool -l` で確かめた）。
- [ ] 仕様 5・6 のとおり `@weak` が動く（macOS の `-O0`・`-O3`、Apple の ld と同梱の zig）。
- [ ] `[macos]` も `@weak` も使わないプログラムの IR・診断と Linux・Windows・WASM の出力が基準と一致し、macOS の変化は仕様 4 の表の範囲だけ。IR が決定的。
- [ ] （D3 で採ったとき）`Env.os_version` が仕様 6 のとおりに native（3 つの OS）と WASI で動き、`tsuzuri test` でもリンクできる。到達しないプログラムは、OS API を使っていても object・実行ファイル・`--wasm-host wasi` の `.wasm` が変わらない。
- [ ] 新しい診断コードと `TrapKind` がなく、新しい kind は D10 のものだけ。`docs/stability.md` に D1 を記録した。
- [ ] 文書を直し、T2 が通り、PR に `ci:full` を付けた。

## 落とし穴

- **prebuilt のランタイム**。native の引数を変えたら `scripts/toolchain/runtime.mjs:43` も同じにする。違うと同梱の toolchain が prebuilt を使わず毎回 compile する（壊れないので気づきにくい）。
- **launcher の二重の版**。`-target …12.0.0` と `-mmacosx-version-min=13.0` を両方 LLVM の clang に渡さない。版を 1 つにしてから `-target` を作る。
- **名前付きの target の既定の文字列**は 1 文字も変えない（キャッシュ、prebuilt、`tests/targets.rs` の期待）。
- **ld64 は SDK にない symbol への弱い参照を拒む**。古い SDK でのリンクの誤りは正しい挙動で、文書に書く。ELF は黙ってリンクする（D5）。
- **検査と TZ08 の包み**。包みは `call … @sym(` の命令を囲むので、`icmp` と分岐は包みの前に置ける。
- **`signature` の先読み**。M22 がなければ、本体のない extern の直後の `@weak` が制約行と読まれる。

## 対象外

- Swift の `#available` に当たる、流れに依存する型の検査。
- 12.0 より前の macOS、Intel の macOS（[M18](M18-intel-mac-and-universal2.md)。D-67 の分岐 3 で入れないと決めた（2026-11-26））、CI での古い macOS の実行。
- Linux の ELF の弱い参照、Windows の遅延読み込み、`dlsym`・`GetProcAddress` の糖衣。
- 依存パッケージの最低 OS 版の要求とルートの版の照合（D2 の退けた案）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | 既定の native ビルドの最低 OS 版を 12.0 に固定する（D-67 の分岐 6）。上げるときは `[macos] min_version` と `MACOSX_DEPLOYMENT_TARGET` を使い、名前付きの target と同梱の zig も `MACOSX_DEPLOYMENT_TARGET` を読む。変わる出力は仕様 4 の表で、IR は変わらない。実装の PR が `docs/stability.md` に記録する | ビルド機の版を既定にすると、配布先がビルド機の OS の更新で黙って狭くなる。名前付きの target の 12.0 と揃う。退けた案（分岐 6 で選ばれなかった）: ビルド機の版のまま（文書に注意を書くだけ。ビルドの警告はノイズなので出さない）。名前付きの target で環境変数を無視し続ける案は、優先順が経路ごとに違うまま | 承認済み（2026-11-26。D-67 の分岐 6） |
| D2 | `[macos]` の節とキー `min_version` を足す（D-55）。ルートの節だけを読み、依存の `[macos]` は読まない | W09 の `[windows]` と同じくアプリの性質を manifest に置く。依存の節を読まないのは `[windows]`・`[wasm]`・`[capabilities]` と同じ規則で、M15 のキーを持つアプリのパッケージも依存にできる。退けた案: 依存の `[macos]` を `E0002` にする（アプリのパッケージを依存にできない）、依存の要求とルートの版の照合（範囲が広がる） | 要承認（D-55。承認前は手順 2 に着手しない） |
| D3 | `@weak` を足す（D-55）。関数は呼び出しの地点で NULL を検査し、報告の 1 行の後に `TrapKind::Assert` でトラップ、クラスは classref を読むたびに検査、データシンボルは `Maybe`。問い合わせに `Env.os_version` を足す。そのランタイムは、独立した入口 `tsuzuri_os_version` と std だけが呼ぶ内部の組み込み `Os.__os_version` にし、到達したプログラムでだけ `-DTZ_OS_VERSION` でコンパイルする（driver の native と WASI、`src/test_runner.rs`。M15 の D13 と同じ形） | 新しい `TrapKind` を足さずに、無い symbol の名前を見せる。`NullHandle` は handle の値の誤りの意味なので使わない。問い合わせがないと、呼ぶ前に分岐できない。define の門は、`Env.os_version` に到達しないプログラムのランタイムの object を変えない（D-67 の「1 byte も変えない」の例外にしない）。退けた案: 関数の結果を `Maybe` にする（C の型とずれ、W14 の照合と食い違う）、symbol の有無を問う組み込み（新しいモジュール名が要り、extern を関数値にする規則と衝突する）、`Os.__read` に操作を足す（OS API に到達するすべてのプログラムのランタイムの object が変わる） | 要承認（D-55 と検査の形。承認前は手順 4 以降に着手しない） |
| D4 | 版は manifest、`MACOSX_DEPLOYMENT_TARGET`、既定の順。manifest の誤りは `E0002`、名前付きの target の環境変数の誤りは `E2000`、Apple clang の native の環境変数は clang に任せる | manifest はアプリの性質、環境変数は 1 回のビルドの上書き。Apple clang の native を検査しないのは、今動く `10.15`（minos 11.0）のビルドを壊さないため | 既定案（実装者はこの案に従う） |
| D5 | `@weak` は Apple の target だけ（ほかの native は `abi_apple_only`、wasm は `abi_native_only`） | Mach-O の weak import はリンクで symbol の存在を検査し、実行時に無ければ 0 にする。ELF は無い symbol を黙ってリンクし（綴りの誤りが実行時まで見えない）、Windows の DLL の import は弱くできない | 既定案（実装者はこの案に従う） |
| D6 | `@weak` は明示の symbol を要り、symbol ごとに効く | `tsuzuri_host_` の symbol はホストの契約（W17 と同じ）。LLVM は 1 つの symbol に 1 行の `declare` で、検査を増やす向きの和なら安全側 | 既定案（実装者はこの案に従う） |
| D7 | Apple clang の native には `-mmacosx-version-min`、名前付きの target には版を持つ triple を渡し、launcher は版を 1 つにして target を作る | native は「target を渡さない」という今の約束（platforms.md）を保ち、名前付きの target は今の triple の形を保つ | 既定案（実装者はこの案に従う） |
| D8 | `@weak` の呼び出しは効果 `trap` を持ち、`@trusted no_trap` との併用は `E1037` | コンパイラが足す検査は宣言の性質に依らずトラップしうる | 既定案（実装者はこの案に従う） |
| D9 | `--backend fast` は `extern_weak` を今どおり `E2002` で拒み、PB07 は `extern_weak` の global で分割を外す（今のまま） | どちらも誤った出力を作らない既存の経路 | 既定案（実装者はこの案に従う） |
| D10 | 新しい診断コードを割り当てない。新しい kind は `E0002` の `invalid_macos_setting`・`weak_attribute_syntax`・`misplaced_weak_attribute`、`E1008` の `weak_needs_symbol`・`weak_data_symbol_result`・`abi_apple_only`（共通の kind 案。最初に出荷したチケットが登録する）、`E1037` の `trusted_no_trap_weak`、`E2000` の `invalid_macos_deployment_target` | D-51、E12 D12 の前例 | 既定案（実装者はこの案に従う） |
