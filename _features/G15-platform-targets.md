# G15: 対応ターゲットの拡張とクロスコンパイル

| 項目 | 内容 |
| --- | --- |
| ID | G15 |
| 優先度 | P3 |
| 規模 | XL |
| 依存 | G10, G14, (F13) |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 要承認: D8（CI workflow `.github/workflows/targets.yml` の追加と runner 費用） |
| 改善する劣位 | C/C++ 比: 対応プラットフォームの幅（[なぜ Tsuzuri か](https://github.com/tatsuya-midorikawa/Tsuzuri/blob/c82c13e1e3dd1f02f78694aa1d26d39b3f793504/_docs/learn/why-tsuzuri.md#cc-に対する劣位点)） |
| 手本にする既存実装 | clang 引数の選択と単体テスト: `src/driver.rs` の `native_compile_args`・`native_cpu_flag` と `validates_native_cpu_tuning_and_selects_architecture_flags`。外部 SDK があるときだけ動く cross-link テスト: `tests/windows.rs` の `cross_links_with_microsoft_sdk_when_requested`（`TSUZURI_WINDOWS_SDK`）。target 別の IR 書き換え: `src/llvm.rs` の `windows_abi`。CLI の解析テスト: `src/main.rs` の `selects_target_defaults_and_honors_path_separator`・`rejects_ambiguous_or_unused_arguments`。cache key の環境変数一覧: `src/cache.rs` の `build_key` |
| 主な影響ファイル | `src/driver.rs`, `src/main.rs`, `src/cache.rs`, `src/test_runner.rs`, G14 の同梱 clang shim（HEAD では `vsc/scripts/clang.rs`）, `tests/targets.rs`（新規）, `tests/targets.mjs`（新規）, `tests/windows.rs`（呼び出しの追従だけ）, `.github/workflows/targets.yml`（新規。D8 の承認後）, `README.md`, `docs/architecture.md`, `_docs/tools/build-and-cache.md`, `_docs/tools/command-line.md`, `_docs/feature-status.md`, `_features/README.md` |

## 目的

C/C++ や Rust と同じく、ビルド機と異なる target への出力（クロスコンパイル）と、より多くの OS・CPU・ABI への対応を段階的に広げる。
対応の程度を tier（1 = CI でビルドと実行、2 = CI でビルドだけ、3 = 受け付けるが CI なし）で明示し、文書の tier を CI の実態と一致させる。

実装者は Phase 1 だけを実装する。Phase 1 は「6 つの 64-bit native triple と wasm32 を `--target` で名指しでき、IR・object・（sysroot があれば）実行ファイルを作れる」ことで完結する。
Phase 2（musl・FreeBSD・RISC-V・実行器・bare-metal など）は人間が求めた場合だけ着手する。

## 着手条件と停止条件

### 着手条件

- G14 が `_features/README.md` の状態欄で done であること。G15 は G14 の同梱 clang shim と sysroot（Zig の SDK/libc）を cross-link の既定経路に使う（「前提とする他チケットのインターフェース」）。
  確認: `grep -n "^| G1[045] " _features/README.md`。
- G10 は HEAD で blocked（Windows 上の実行だけが未確認）。G15 は G10 の実装済み部分（`windows_abi`、`native_compile_args` の Windows 分岐、`task_runtime_source`）を前提にし、G10 の done を待たない（D9）。
  G10 が done になるまで Windows の triple は tier 2 より上にしない。
- F13 は着手条件にしない。bare-metal（freestanding）は Phase 2 で、F13 の後に検討する。
- CI の手順（手順 12）は D8 の承認後だけ着手する。それ以外の手順は承認を待たない。
- GUIDE §2.3 の基準コマンドが成功し、手順 1 のベースライン（`--target native` の IR と object）を保存していること。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- `--target native`（`BuildOptions::triple` が `None`）の生成 IR、clang・wasm-ld の引数、出力拡張子のどれかが HEAD から変わる。
- Phase 1 の triple のために、host native と異なる Tsuzuri IR（`windows_abi` の書き換え以外）が必要になった。IR は datalayout と `target triple` を持たず、clang の `--target` で決まる前提である。
- 生成 IR または runtime C の関数が、C 境界で構造体を値渡し・値返しする宣言を持っていた（target ごとに ABI が変わるため、D5 の前提が崩れる）。
- 32-bit pointer の native target（i686、armv7 など）を受け付けたくなった。native IR は 64-bit pointer を前提にする。
- cross-link のために新しい crate、テスト中のネットワーク取得、SDK の同梱、`unsafe` が必要になった。
- 外国の実行ファイルを実行する手段（qemu、Rosetta の導入、Wine）が必要になった。Phase 1 は実行しない。
- 既存テストの期待値を変える必要がある。未知 target のメッセージ（D3）と `task_runtime_source` の引数の追従だけは除く。
- G14 の shim の場所・引数の扱いが「前提とする他チケットのインターフェース」と異なる。

## 現状（HEAD `f8dc655` で確認）

- `src/driver.rs` の `Target` は `Native` と `Wasm32` だけ。`BuildOptions` は `Copy` で、`target`・`emit`・`optimization`・`cpu` などの field を持つ。
- `src/main.rs` の引数解析は `--target` に `native` と `wasm32` だけを受け、ほかは `target must be 'native' or 'wasm32'`（`E2000`、終了コード 2）。
  `run` は `--target` を受けない（`parse(&["run", "Main.tz", "--target", "wasm32"]).is_err()` のテスト）。`test` は `--target native|wasm32` を受ける。
- native の target 性質はすべてビルド機から決まる。`cfg!(windows)`・`cfg!(target_os = ...)`・`env::consts::ARCH` を使う箇所は次のとおり。
  - `BuildOptions::output_path`（`.exe`・`.obj`）、`task_runtime_source`（`task-windows.h` の inline）、`native_compile_args(cfg!(windows), env::consts::ARCH)`
    （Windows は `--target=<arch>-pc-windows-msvc`、それ以外は `-fPIC` だけで `--target` を渡さない）。
  - 生成直後の `llvm::windows_abi` の適用条件、Windows COFF object の `E2002`、`-pthread`・`-lm` の付与、macOS の DWARF sidecar（`dsymutil`）と
    `merge_debug_ir`（`llvm-link`）、`-r -nostdlib` link の `-Wl,-keep_private_externs`（macOS）と `-no-pie`（Linux）、`run_with_diagnostics` の `program.exe`。
  - `native_cpu_flag(env::consts::ARCH)`（`BuildOptions::validate` と runtime・本体の compile）。`src/cache.rs` の `build_key` は `host-os`・`host-arch` と
    `format!("{options:?}")` を key に入れ、`TSUZURI_CLANG`・`SDKROOT` など固定の環境変数一覧を hash する。
- 生成 IR は `target triple` と `target datalayout` を持たない。埋め込み runtime IR（`src/runtime/*.ll` の 15 ファイル）も triple・datalayout・`"target-cpu"`・
  `"target-features"` を持たない。したがって同じ native IR を clang の `--target` だけで別 triple へ下ろせる。
- runtime C の分岐はすべて target の preprocessor 定義による。`src/runtime/task.c` は `_WIN32` で `task-windows.h`、それ以外は `<pthread.h>`・`<unistd.h>` と
  `sysconf(_SC_NPROCESSORS_ONLN)`。`src/runtime/io.c` は `_WIN32` で `_setmode` の binary mode。`src/runtime/cpu.c` は `TZ_CPU_X86`
  （`__x86_64__`/`__i386__` かつ非 `_WIN32`）のときだけ CPUID・XGETBV を使い、ほかは baseline。`src/runtime/numeric.c` は `__BYTE_ORDER__` で分岐する。
  POSIX では `TZ_TASK_API`・`TZ_CPU_API`・`TZ_IO_API` が `__attribute__((weak, visibility("hidden")))`、Windows では空。
- G14 の前身である VSIX の shim `vsc/scripts/clang.rs` は、受け取った `--target=x86_64-pc-windows-msvc`・`--target=aarch64-pc-windows-msvc` を捨て、
  ビルド機の `env::consts::{ARCH, OS}` から Zig の target（`<arch>-macos.12.0`・`<arch>-linux-musl`・`<arch>-windows-gnu`）を作る。つまり同梱ツールでは現状 cross できない。
- CI は `.github/workflows/vscode.yml` だけ。matrix は `macos-15`（darwin-arm64）、`macos-15-intel`（darwin-x64）、`ubuntu-24.04`（linux-x64）、
  `ubuntu-24.04-arm`（linux-arm64）、`windows-2022`（win32-x64）、`windows-11-arm`（win32-arm64）。各 job は fmt・clippy・`npm run test:unit`・
  `npm run toolchain`・`tests/lsp_sessions.mjs`・`npm run test:toolchain`（`vsc/scripts/smoke.mjs`: 同梱ツールで native を build・実行・test、wasm32 を build）・
  VS Code 統合テストを実行する。`cargo test` と `tests/*.mjs` の E2E は CI で実行していない。Actions の実行結果はこの環境から確認できない。
- G10 の「実装と検証」は `windows.yml` の追加を記すが、HEAD に `.github/workflows/windows.yml` はない（`git log --oneline --all -- .github/workflows/windows.yml`
  が `e02cf02`（追加）と `16d4fa9` を示す）。`tests/windows.rs` と `tests/windows.mjs` は残っている。
- F05 の x86 経路（SSE4.2／AVX2）は cross compile 済みだが実機未検証。

### 再現（2026-09-29 に検証済み。arm64 macOS、Rosetta なし）

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
mkdir -p /tmp/tz-g15/a && printf 'let total = 40 + 2\ntotal\n' > /tmp/tz-g15/a/Main.tz
target/release/tsuzuri run /tmp/tz-g15/a                                   # 42
target/release/tsuzuri build /tmp/tz-g15/a --target x86_64-unknown-linux-gnu -o /tmp/tz-g15/x
# <command line>:1:1: error[E2000]: target must be 'native' or 'wasm32'（終了コード 2）
target/release/tsuzuri build /tmp/tz-g15/a --emit llvm -o /tmp/tz-g15/a.ll
grep -c "target triple\|target datalayout" /tmp/tz-g15/a.ll                # 0
for t in x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu x86_64-apple-macos11; do
  /opt/homebrew/opt/llvm@21/bin/clang --target=$t -x ir -Wno-override-module -O3 -c /tmp/tz-g15/a.ll -o /tmp/tz-g15/$t.o
done
file /tmp/tz-g15/*.o   # ELF x86-64、ELF aarch64、Mach-O x86_64 の relocatable
```

同じ機械での事実: `clang --target=x86_64-apple-macos11` で C の実行ファイルを link できるが、実行は `bad CPU type in executable`（Rosetta なし）。
Apple の clang で Linux triple を link すると ld64 が `--hash-style=gnu` などを拒否する（ELF には lld と sysroot が要る）。
`vsc/toolchain/zig/zig cc -target x86_64-linux-gnu` は C の実行ファイルを link できた。LLVM 21 の datalayout は Phase 1 の 6 triple と wasm32 のすべてで
`i128:128`、wasm32 だけ `p:32:32`。

## 仕様

### 前提とする他チケットのインターフェース

- G14（done が着手条件）: ツールの探索順は環境変数（`TSUZURI_CLANG` など）→ 配布物の同梱ツール → `PATH`。同梱 clang は Zig の SDK/libc を使う shim
  （HEAD では `vsc/scripts/clang.rs`。G14 が `scripts/toolchain/` などへ移す）。G15 はこの shim に「受け取った `--target=<clang target>` を Zig の target へ写す」
  変更を加える（手順 10）。`--target=` がないときの挙動（ビルド機から決める）は変えない。
- G10（blocked のまま前提にする。D9）: `llvm::windows_abi(ir, module)` は MSVC CRT 向けの書き換え（公開 wrapper の `dllexport`、`_write`・`_setmode`）をする。
  `task_runtime_source` は `task-windows.h` を inline する。Win32 task runtime は `src/runtime/task-windows.h`。G15 はこれらの適用条件をビルド機から target へ移すだけで、中身を変えない。

### 構文（CLI）

新構文（実装後に有効。未検証）。`build` と `test` の `--target` の値を増やす。`run` は HEAD どおり `--target` を受けない。

```text
--target native | wasm32 | wasm32-unknown-unknown | TRIPLE
TRIPLE  = aarch64-apple-darwin | x86_64-apple-darwin
        | aarch64-unknown-linux-gnu | x86_64-unknown-linux-gnu
        | aarch64-pc-windows-msvc | x86_64-pc-windows-msvc
環境変数 TSUZURI_SYSROOT=DIR   （新規。空でなければ native 系の clang 呼び出しへ --sysroot=DIR を渡す）
```

- `native` は HEAD と同じ（ビルド機の triple。clang へ `--target` を渡さない。Windows ビルド機だけ HEAD どおり渡す）。
- `wasm32-unknown-unknown` は `wasm32` の別名で、`Target::Wasm32` になる。
- TRIPLE は `Target::Native` と `BuildOptions::triple = Some(..)`（新規）になる。ビルド機と同じ triple を明示しても `Some` のまま保ち、clang へ `--target` を渡す。
- 大文字小文字は区別する。別名（`arm64-apple-darwin`、`x86_64-linux-gnu` など）は受けない（D2）。

### 対象 target と tier

tier の定義（D7）: 1 = CI でその triple の実行ファイルを作り、matching runner で `-O0`・`-O3` の E2E を実行する。2 = CI で object を作り形式を検査する（実行しない）。
3 = `--target` で受け付けるが CI がない。文書には、その時点で CI に実在する job から決まる tier だけを書く。

| CLI 名 | clang の `--target` | 出力拡張子 exe / object | この arm64 macOS での扱い | 予定の CI（D8） | 予定の tier |
| --- | --- | --- | --- | --- | --- |
| `aarch64-apple-darwin` | `arm64-apple-macosx12.0.0` | なし / `.o` | build・実行 | `macos-15` | 1 |
| `x86_64-apple-darwin` | `x86_64-apple-macosx12.0.0` | なし / `.o` | build・link だけ（Rosetta なし） | `macos-15-intel` | 1 |
| `aarch64-unknown-linux-gnu` | `aarch64-unknown-linux-gnu` | なし / `.o` | object。link は同梱 shim か `TSUZURI_SYSROOT` | `ubuntu-24.04-arm` | 1 |
| `x86_64-unknown-linux-gnu` | `x86_64-unknown-linux-gnu` | なし / `.o` | object。link は同梱 shim か `TSUZURI_SYSROOT` | `ubuntu-24.04` | 1 |
| `x86_64-pc-windows-msvc` | `x86_64-pc-windows-msvc` | `.exe` / `.obj` | object（runtime を含む object は E2002） | `ubuntu-24.04` で cross object | 2（G10 が done で G10 の job により 1） |
| `aarch64-pc-windows-msvc` | `aarch64-pc-windows-msvc` | `.exe` / `.obj` | object（同上） | `ubuntu-24.04` で cross object | 2 |
| `wasm32`（`wasm32-unknown-unknown`） | `wasm32-unknown-unknown` | – / `.o`・`.wasm` | build・Node で実行 | tier 1 の 4 job | 1 |

Phase 2 に回す target: `*-unknown-linux-musl`（明示 triple として。同梱 shim の `--target native` が Linux で musl を使う HEAD の挙動は変えない）、`x86_64-unknown-freebsd`、
`riscv64gc-unknown-linux-gnu`、Android（NDK）、iOS、MinGW の明示 triple、bare-metal（F13 の後）。32-bit native（i686、armv7）は対象外。

### target ごとの ABI と意味

- 言語の意味（整数の overflow・丸め・NaN・signed zero・評価順序・trap・所有権）は triple で変わらない。IR は host native と同じで、違いは clang の `--target` と
  Windows triple の `windows_abi` だけ（D5）。
- Phase 1 の 6 triple は 64-bit pointer・little-endian で、LLVM 21 の datalayout はすべて `i128:128`（wasm32 も `i128:128`、pointer だけ 32）。
  native の IR と `storage_layout` の前提は変わらない。手順 2 で全 triple の datalayout を検査し、`i128:128` でない triple があれば停止する。
- 公開 ABI: `i128` は export できない（HEAD どおり）。bool は C で `int32_t`、狭い整数は 32-bit に正規化、buffer は 16 bytes の out descriptor（`docs/language.md`）。
  これらは scalar と pointer だけで構成され、構造体の値渡しを使わないので、System V・AAPCS64・Windows x64 の差を受けない。
- symbol: Mach-O の先頭 `_` は LLVM が付けるので IR の名前は変えない。Windows triple は `windows_abi` が `define dllexport` と CRT 名（`_write`・`_setmode`）へ書き換える。
  この書き換えは Windows triple のとき、ビルド機に関係なく適用する（`--emit llvm` を含む。HEAD の Windows ビルド機と同じ条件）。
- runtime C: `task.c`・`io.c`・`cpu.c`・`numeric.c` の分岐は target の preprocessor 定義で決まるので変更しない。Windows の LLP64 で幅が変わる `long` は POSIX の分岐
  （`sysconf` の戻り値）だけで使う。`-pthread`・`-lm`・`-fPIC` は Windows 以外の triple だけに渡す。
- CPU dispatch（F05）: x86_64 の Apple・Linux triple は `TZ_CPU_X86` の CPUID 経路、Windows と aarch64 は baseline。`--cpu native` は cross では拒否する（D6）。
- trap の終了状態は OS ごとに異なる（POSIX は signal、Windows は例外コード）。Phase 1 は cross した実行ファイルを実行しないので、`E2005` の文言は変えない。

### 診断

メッセージの `{triple}` は CLI 名、`{list}` は `aarch64-apple-darwin, x86_64-apple-darwin, aarch64-unknown-linux-gnu, x86_64-unknown-linux-gnu, aarch64-pc-windows-msvc, x86_64-pc-windows-msvc`。
CLI の診断は HEAD と同じく `<command line>:1:1` に出る（位置は `Span::default()`）。

| コード | 条件 | メッセージ | 位置 |
| --- | --- | --- | --- |
| `E2000` | `--target` の値が未知（Phase 2 の triple、別名、32-bit を含む）。HEAD の `target must be 'native' or 'wasm32'` を置き換える | `unknown target '{value}'; use native, wasm32, or one of {list}` | `<command line>` |
| `E2000` | `test --target TRIPLE` で TRIPLE がビルド機の triple と異なる | `cannot run {triple} tests on this machine; run 'tsuzuri test' on a {triple} machine, or use 'tsuzuri build --target {triple}'` | `<command line>` |
| `E2000` | cross（D6）で `--cpu native` | `'--cpu native' requires the host target; use '--cpu generic' when cross-compiling to {triple}` | `<command line>` |
| `E2000` | TRIPLE と `--emit wgsl` | HEAD の `WGSL output does not use target, CPU, debug, or WASM feature options`（条件に `triple.is_some()` を足す） | `<command line>` |
| `E2000` | TRIPLE と `--emit wasm`、`--wasm-feature` | HEAD の既存メッセージ（`Target::Native` なので現行の検査がそのまま効く） | `<command line>` |
| `E2002` | cross の clang・linker が失敗した | HEAD の `'{name}' failed ({status}):\n{output}\n{hint}`。cross のときだけ hint を `cross-compiling to {triple} needs its C headers, C library, and linker; use the bundled toolchain or set TSUZURI_SYSROOT to a {triple} sysroot` にする | なし |
| `E2002` | Windows triple で task・CPU・IO runtime を含む `--emit object` | HEAD の `Windows COFF objects with embedded task, CPU, or IO runtime are not supported; emit LLVM and link the runtime once, or build an executable`（条件をビルド機から target へ） | なし |

### 資源上限

新しい上限はない。triple の一覧は固定の 6 個（`Triple::ALL`（新規））。

### 例

新構文（実装後に有効。未検証）。arm64 macOS で実行したときの期待。

```sh
target/release/tsuzuri build app --target x86_64-apple-darwin -o app-x64          # 成功。file: Mach-O 64-bit executable x86_64
target/release/tsuzuri build app --target x86_64-unknown-linux-gnu --emit object  # 成功。app/Main.o は ELF 64-bit relocatable x86-64
target/release/tsuzuri build app --target x86_64-pc-windows-msvc --emit llvm -o a.ll   # 成功。a.ll に define dllexport（export があるとき）
target/release/tsuzuri build app --target x86_64-unknown-linux-musl               # E2000 unknown target 'x86_64-unknown-linux-musl'; ...
target/release/tsuzuri test app --target x86_64-unknown-linux-gnu                 # E2000 cannot run x86_64-unknown-linux-gnu tests on this machine; ...
target/release/tsuzuri build app --target aarch64-apple-darwin --cpu native       # E2000 '--cpu native' requires the host target; ...
target/release/tsuzuri test app --target aarch64-apple-darwin                     # 成功（ビルド機と同じ triple）
```

## 設計

### データ構造

`src/driver.rs` に置く。新しいファイルは作らない。

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Triple { // （新規）
    Aarch64AppleDarwin, X86_64AppleDarwin, Aarch64LinuxGnu, X86_64LinuxGnu, Aarch64WindowsMsvc, X86_64WindowsMsvc,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Os { MacOs, Linux, Windows, Other } // （新規）。Other は HEAD の「macOS・Linux・Windows 以外」の挙動
#[derive(Clone, Copy, Debug)]
pub(crate) struct Platform { pub os: Os, pub arch: &'static str, pub triple: Option<Triple> } // （新規）

impl Triple {
    pub const ALL: [Triple; 6];                    // CLI 名の一覧と診断の {list} の順
    pub fn parse(text: &str) -> Option<Triple>;     // CLI 名だけ。別名なし
    pub fn name(self) -> &'static str;              // CLI 名
    pub(crate) fn clang_target(self) -> &'static str; // 表の clang の --target
    pub(crate) fn os(self) -> Os;
    pub(crate) fn arch(self) -> &'static str;       // "aarch64" | "x86_64"（env::consts::ARCH と同じ綴り）
    pub fn host() -> Option<Triple>;                // cfg!(target_os, target_arch, target_env = "gnu"/"msvc") から。該当なしは None
}
pub struct BuildOptions { /* 既存の field */ pub triple: Option<Triple> } // triple（新規）。Default は None
pub struct TestOptions { /* 既存の field */ pub triple: Option<Triple> }  // 同上
impl BuildOptions {
    pub(crate) fn platform(self) -> Platform; // None: cfg! と env::consts::ARCH から。Some(t): t.os()・t.arch()
    pub(crate) fn cross(self) -> bool;        // triple.is_some_and(|t| Some(t) != Triple::host())
}
```

`BuildOptions` は `Copy` のまま（`Triple` が `Copy`）。`src/cache.rs` の `build_key` は `format!("{options:?}")` を hash するので、triple は自動で key に入る。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| CLI | `src/main.rs` | `--target` の解析 | `native`・`wasm32`・`wasm32-unknown-unknown`・`Triple::parse`。未知は D3 のメッセージ。`triple` を `Arguments` の options へ入れる |
| CLI | `src/main.rs` | usage 文字列、`BuildOptions` と `driver::TestOptions` の struct literal | `--target` の値（`native`・`wasm32`・TRIPLE）と `TSUZURI_SYSROOT` の説明。`triple` field |
| CLI | `src/main.rs` の `mod tests` | `selects_target_defaults_and_honors_path_separator`, `rejects_ambiguous_or_unused_arguments` | 6 triple の受理と未知値・`run --target` の拒否を足す（既存の assert は変えない） |
| driver | `src/driver.rs` | `Triple`, `Os`, `Platform`, `BuildOptions::platform`, `BuildOptions::cross`（すべて新規） | 上のデータ構造 |
| driver | `src/driver.rs` | `BuildOptions::validate` | `--cpu native` の分岐で `self.cross()` なら D6 の E2000。WGSL の条件に `self.triple.is_some()` |
| driver | `src/driver.rs` | `BuildOptions::output_path` | `cfg!(windows)` を `self.platform().os == Os::Windows` へ |
| driver | `src/driver.rs` | `task_runtime_source` | 引数 `windows: bool`（target）を取り、true なら inline。呼び出し元は `platform.os == Os::Windows` を渡す |
| driver | `src/driver.rs` | `native_compile_args` | `(platform: Platform) -> Vec<OsString>`。`triple == None` は HEAD と同じ引数。`Some(t)` は `--target=<clang_target>`、Windows 以外は `-fPIC`、`TSUZURI_SYSROOT` があれば `--sysroot=DIR` |
| driver | `src/driver.rs` | `cross_link_args`（新規） | `triple` が `Some` で cross かつ target の OS が macOS 以外なら `-fuse-ld=lld`。それ以外は空。link する 3 つの呼び出し（本体の clang が exe を作るとき、DWARF 用 linker、`-r -nostdlib`）へ足す |
| driver | `src/driver.rs` | `build_complete` の `windows_abi` 適用、`E2002` の 2 か所、`task_runtime`・`-pthread`・`-lm`、`dwarf_sidecar`、`merge_debug_ir`、`-Wl,-keep_private_externs`、`-no-pie`、exe の一時名 | `cfg!(windows)`・`cfg!(target_os = ..)` を `platform.os` の比較へ。`!cfg!(any(unix, windows))` の E2002 はビルド機の検査なので変えない |
| driver | `src/driver.rs` | `run_tool` の hint | cross のとき D3 の E2002 hint へ差し替える（呼び出し側で hint を選ぶ） |
| driver | `src/driver.rs` | `native_cpu_flag(env::consts::ARCH)` の 3 か所 | 変更なし（cross では validate が先に拒否する） |
| driver | `src/driver.rs` の `mod tests` | `validates_native_cpu_tuning_and_selects_architecture_flags`, `task_runtime_source_inlines_the_win32_adapter` | 新しい引数への追従だけ。assert の期待値は変えない |
| test | `src/test_runner.rs` | `TestOptions`, `run_with_timeout`, `native_compile_args` と `task_runtime_source` の呼び出し | `triple`。`Target::Native` で `triple` が `Some(t)` かつ `Some(t) != Triple::host()` なら D3 の E2000 |
| cache | `src/cache.rs` | `build_key` | 環境変数一覧に `TSUZURI_SYSROOT`。`cfg!(target_os = "macos") && options.debug_info` の 2 か所は `options.platform().os == Os::MacOs` へ |
| shim | G14 の shim（HEAD では `vsc/scripts/clang.rs` の `main`） | target の決定 | D10 の写像。`--target=` がなければ HEAD の決め方 |
| テスト | `tests/targets.rs`（新規）, `tests/targets.mjs`（新規）, `tests/windows.rs` | – | テスト計画。`tests/windows.rs` は `native_compile_args` を使わないので変更は不要のはず（使っていたら追従だけ） |
| CI | `.github/workflows/targets.yml`（新規） | – | D8 の承認後 |

### 生成 IR とランタイム

- IR: 変更なし。`triple` が Windows のときだけ `llvm::windows_abi` を通す（HEAD の Windows ビルド機と同じ出力）。`target triple` 行は IR に足さない（clang の
  `--target` と `-Wno-override-module` が決める。D5）。
- runtime C: `task.c`・`cpu.c`・`io.c` を HEAD と同じ連結・同じ条件で一時ファイルへ書き、`native_compile_args(platform)` で compile する。Windows triple では
  `task_runtime_source(true)` が `task-windows.h` を inline する（一時ディレクトリに header がないため。macOS から Windows へ cross するときに必須）。
- 連結: 本体と runtime object の link は HEAD の順序どおり。cross では `cross_link_args` を足す。`-r -nostdlib` の object 結合は macOS triple で
  `-Wl,-keep_private_externs`、Linux triple で `-no-pie`。

### アルゴリズム

```text
platform(options):
  if options.triple = Some(t): return { os: t.os(), arch: t.arch(), triple: Some(t) }
  os = Windows if cfg!(windows) else MacOs if cfg!(target_os="macos") else Linux if cfg!(target_os="linux") else Other
  return { os, arch: env::consts::ARCH, triple: None }

native_compile_args(p):
  if p.triple = None: return HEAD の native_compile_args(p.os == Windows, p.arch)
  args = ["--target=" + t.clang_target()]
  if p.os != Windows: args += ["-fPIC"]
  if TSUZURI_SYSROOT が空でない: args += ["--sysroot=" + value]
  return args

cross_link_args(options):
  if options.cross() and options.platform().os != MacOs: return ["-fuse-ld=lld"]
  return []
```

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、`running N tests` の N を必ず見る（GUIDE §3.1）。
コマンドはすべて `cd /Users/tmidorikawa/Documents/git/Tsuzuri` の後で実行する。

### 手順 1: ベースラインを取る

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行し、`--target native` の IR と object を保存する（手順 5 と 14 で byte 比較する）。
- 確認: 次がすべて成功する。`--bin tsuzuri` は `4 passed`。`--test windows` は成功件数を記録する（SDK がなければ cross-link テストは即 return する）。

```sh
cargo build --release --locked
mkdir -p /tmp/tz-g15/before /tmp/tz-g15/lib && printf 'export def answer :: i64\nfn answer = 42\n' > /tmp/tz-g15/lib/Answer.tz
for e in tasks control; do target/release/tsuzuri build examples/$e --emit llvm -o /tmp/tz-g15/before/$e.ll; done
target/release/tsuzuri build /tmp/tz-g15/lib/Answer.tz --emit object --no-cache -o /tmp/tz-g15/before/answer.o
cargo test --locked --bin tsuzuri
cargo test --locked --lib validates_native_cpu_tuning_and_selects_architecture_flags
cargo test --locked --test windows
```

### 手順 2: `Triple`・`Os`・`Platform` と datalayout の確認

- 変更: `src/driver.rs`（データ構造の節の型と `Triple` の関数）、`mod tests` に `parses_phase_one_triples_and_detects_the_host`（新規）。
- 内容: まだどこからも使わない。`Triple::host()` は `cfg!(all(target_os = "macos", target_arch = "aarch64"))` などの 6 条件で決め、Linux は `target_env = "gnu"`、Windows は `target_env = "msvc"` のときだけ `Some`。
- 確認: `cargo test --locked --lib parses_phase_one_triples_and_detects_the_host` が `1 passed`。次のループが 7 行とも `i128:128` を含む（含まなければ停止）。

```sh
for t in arm64-apple-macosx12.0.0 x86_64-apple-macosx12.0.0 aarch64-unknown-linux-gnu x86_64-unknown-linux-gnu \
         aarch64-pc-windows-msvc x86_64-pc-windows-msvc wasm32-unknown-unknown; do
  echo "$t $(echo 'int x;' | ${TSUZURI_CLANG:-clang} --target=$t -x c - -S -emit-llvm -o - | grep -o 'i128:[0-9]*')"
done
```

### 手順 3: CLI の `--target`

- 変更: `src/main.rs` の `--target` の解析、usage 文字列（`--target native|wasm32|TRIPLE`、`TSUZURI_SYSROOT`）、`BuildOptions {`・`driver::TestOptions {` の構築、
  `mod tests` に `accepts_phase_one_triples_and_rejects_unknown_targets`（新規）。
- 内容: D2・D3 のとおり。`Arguments` に triple を運ぶ（既存の options の流れに `triple` を足すだけ）。
- 確認: `cargo test --locked --bin tsuzuri` が `5 passed`。

### 手順 4: `BuildOptions::triple` と検証

- 変更: `src/driver.rs` の `BuildOptions`・`Default`・`validate`・`platform`・`cross`、`src/test_runner.rs` の `TestOptions`・`Default`、
  全 field を列挙する構築箇所（`src/driver.rs` の `mod tests`、`tests/debug_info.rs`、`tests/host_abi.rs`、`tests/trap_locations.rs` など。compile error が示す）に `triple: None`。
- 内容: `validate` の `Cpu::Native` の分岐の先頭で `self.cross()` を検査し D3 のメッセージ。WGSL の条件に `self.triple.is_some()`。
  `mod tests` に `cross_rejects_cpu_native_and_wgsl`（新規）。
- 確認: `cargo test --locked --lib cross_rejects_cpu_native_and_wgsl` が `1 passed`。`cargo test --locked` が成功する。

### 手順 5: build の target 化（native の出力は不変）

- 変更: `src/driver.rs` の `output_path`、`task_runtime_source`、`native_compile_args`、`build_complete` の各分岐（段ごとの変更の driver 行）、`src/test_runner.rs` の呼び出し。
- 内容: `let platform = options.platform();` を `build_complete` の先頭で一度だけ作り、`cfg!(windows)`・`cfg!(target_os = ..)` を置き換える。まだ `-fuse-ld` と sysroot は足さない。
- 確認: 次の `cmp` が差分なしで終わる。`cargo test --locked --lib task_runtime_source_inlines_the_win32_adapter` と
  `cargo test --locked --lib validates_native_cpu_tuning_and_selects_architecture_flags` がそれぞれ `1 passed`。
  `grep -n 'cfg!(' src/driver.rs` に残るのは `BuildOptions::platform` の host 判定、`!cfg!(any(unix, windows))`、`mod tests` の assert だけ。

```sh
cargo build --release --locked
for e in tasks control; do target/release/tsuzuri build examples/$e --emit llvm -o /tmp/tz-g15/$e.ll; cmp /tmp/tz-g15/before/$e.ll /tmp/tz-g15/$e.ll; done
target/release/tsuzuri build /tmp/tz-g15/lib/Answer.tz --emit object --no-cache -o /tmp/tz-g15/answer.o && cmp /tmp/tz-g15/before/answer.o /tmp/tz-g15/answer.o
```

### 手順 6: cross の link・sysroot・hint

- 変更: `src/driver.rs` の `native_compile_args`（sysroot）、`cross_link_args`（新規）と link する 3 つの呼び出し、native 系 `run_tool` の hint の選択。
- 内容: D11 のとおり。cross のときは native 系の全 `run_tool` 呼び出し（runtime の compile、本体、DWARF 用 linker、`-r -nostdlib`）の hint を D3 の文言にする。
- 確認（arm64 macOS）: 1 行目は `Mach-O 64-bit executable x86_64`、2 行目は `21325334000`、3 行目は `ELF 64-bit LSB relocatable, x86-64`、
  4 行目は JSON の `"code":"E2002"` と `set TSUZURI_SYSROOT` を含む（Apple の clang と sysroot なし）。

```sh
cargo build --release --locked
target/release/tsuzuri build examples/control --target x86_64-apple-darwin --no-cache -o /tmp/tz-g15/control-x64 && file -b /tmp/tz-g15/control-x64
target/release/tsuzuri build examples/tasks --target aarch64-apple-darwin --no-cache -o /tmp/tz-g15/tasks-arm && /tmp/tz-g15/tasks-arm
target/release/tsuzuri build /tmp/tz-g15/lib/Answer.tz --target x86_64-unknown-linux-gnu --emit object -o /tmp/tz-g15/answer-linux.o && file -b /tmp/tz-g15/answer-linux.o
target/release/tsuzuri build examples/tasks --target x86_64-unknown-linux-gnu --no-cache --json -o /tmp/tz-g15/tasks-linux
```

### 手順 7: 言語内テストの target

- 変更: `src/test_runner.rs` の `run_with_timeout`。
- 内容: 最初の検査（最適化レベル）の直後に、foreign triple を D3 の E2000 で拒否する。ビルド機と同じ triple はそのまま実行する。
- 確認: `vsc/scripts/smoke.mjs` と同じ test 宣言（`test "same" = assert true`）を持つ `/tmp/tz-g15/t/Main.tz` に対し、`tsuzuri test /tmp/tz-g15/t --target aarch64-apple-darwin`
  が成功し、`--target x86_64-unknown-linux-gnu` が E2000（D3 の文言）で終わる。

### 手順 8: cache key

- 変更: `src/cache.rs` の `build_key`（環境変数一覧に `TSUZURI_SYSROOT`、macOS の debug 判定を target へ）。
- 確認: `node tests/cache.mjs target/release/tsuzuri` が成功する（release を作り直してから）。

### 手順 9: Rust の統合テスト

- 変更: `tests/targets.rs`（新規）。`tests/host_abi.rs` の `tsuzuri::driver::build` 呼び出しの形（`analyze` → project → `BuildOptions`）を写す。
- 確認: `cargo test --locked --test targets` が `5 passed`（テスト計画の 5 件）。

### 手順 10: 同梱 shim の target 写像（G14）

- 変更: G14 の shim（HEAD では `vsc/scripts/clang.rs` の `main`）。
- 内容: D10 の表で `--target=` を Zig の target と `-x ir` の compile に使う LLVM target へ写し、その引数は Zig へ渡さない。`--target=` がなければ HEAD の決め方。
  ARM64 Windows ビルド機の llvm-mingw 分岐は変えない。
- 確認: 同梱ツールを作った後（`npm --prefix vsc run toolchain`）、次の 2 行が `ELF 64-bit LSB` と `x86-64`、`ELF 64-bit LSB` と `ARM aarch64` を含み、
  `npm --prefix vsc run test:toolchain` が成功する（host の経路が不変）。

```sh
export TSUZURI_CLANG="$PWD/vsc/toolchain/bin/clang" ZIG_GLOBAL_CACHE_DIR=/tmp/tz-g15/zig-cache
for t in x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu; do
  target/release/tsuzuri build examples/tasks --target $t --no-cache -o /tmp/tz-g15/tasks-$t && file -b /tmp/tz-g15/tasks-$t
done
```

### 手順 11: E2E `tests/targets.mjs`

- 変更: `tests/targets.mjs`（新規）。`tests/windows.mjs` の `execute`・`cli` の形を写す。
- 確認: `node tests/targets.mjs target/release/tsuzuri` が成功し、実行した triple・link だけの triple・object だけの triple を 1 行ずつ表示する。
  `TSUZURI_CROSS_LINK=1`（新規。テスト専用）と同梱 shim を付けると Linux 2 triple の link も検査する。

### 手順 12: CI（D8 の承認後だけ）

- 変更: `.github/workflows/targets.yml`（新規）。
- 内容: D8 の matrix と step。LLVM の導入 step は `vscode.yml` の「LLVM on macOS」「LLVM on Linux」を写す（Windows job は作らない）。
- 確認: 人間が `workflow_dispatch` で実行し、4 job が成功した run の URL を完了報告に書く。成功するまで文書の tier を 1 にしない。

### 手順 13: 文書

- 変更: 「ドキュメント」の節のファイル。
- 確認: `node scripts/check-docs.mjs README.md _docs/tools/command-line.md _docs/tools/build-and-cache.md _docs/feature-status.md` が成功する。

### 手順 14: 最終確認

- 確認: 次がすべて成功し、手順 5 の `cmp` がまだ差分なし。

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
node tests/e2e.mjs target/release/tsuzuri
node tests/targets.mjs target/release/tsuzuri
node tests/cache.mjs target/release/tsuzuri
```

## テスト計画

### Rust テスト

- `src/driver.rs` の `mod tests`
  - `parses_phase_one_triples_and_detects_the_host`（新規）: `Triple::ALL` の各 `name()` が `parse` で同じ値に戻る。`parse` は `x86_64-unknown-linux-musl`・
    `arm64-apple-darwin`・`x86_64-linux-gnu`・`i686-pc-windows-msvc`・`AARCH64-apple-darwin`・`wasm32`・空文字列で `None`。`host()` が `Some(t)` なら
    `t.arch() == env::consts::ARCH`。`native_compile_args` が `triple == None` で HEAD の値、各 triple で `["--target=<clang_target>", "-fPIC"]`
    （Windows は `-fPIC` なし）。このテストは `TSUZURI_SYSROOT` が設定されていたら sysroot の assert を飛ばす（環境変数を書き換えない）。
  - `cross_rejects_cpu_native_and_wgsl`（新規）: foreign triple と `Cpu::Native` で D3 の文言の E2000、`Emit::Wgsl` と triple で既存の文言の E2000。
- `src/main.rs` の `mod tests`
  - `accepts_phase_one_triples_and_rejects_unknown_targets`（新規）: 6 triple と `wasm32-unknown-unknown`（`Target::Wasm32`）を `build` と `test` で受理。
    未知値 5 種で `Err` の文字列が D3 と一致。`run ... --target aarch64-apple-darwin` は `Err`。
- `tests/targets.rs`（新規）。独立した参照は ELF gABI、Apple の `<mach-o/loader.h>`・`<mach/machine.h>`、Microsoft PE/COFF 仕様の定数。
  - `non_windows_triples_keep_the_native_ir`: `Emit::Llvm` で `triple: None` と 4 つの非 Windows triple の IR が一致する。
  - `windows_triples_rewrite_exports_on_any_host`: `Answer.tz` の `Emit::Llvm` で Windows 2 triple は `define dllexport i64 @tz_answer(` を含む。
    ビルド機が Windows でなければ `triple: None` は `dllexport` を含まない。
  - `cross_objects_have_the_target_machine`: `Emit::Object` の先頭 bytes を下の表で検査する。
  - `test_runner_rejects_foreign_triples`: `Triple::ALL` のうち `host()` と異なる最初の triple で `run_tests` が D3 の E2000。
  - `cross_link_failure_reports_the_sysroot_hint`: `TSUZURI_SYSROOT` と `TSUZURI_CLANG` が未設定のときだけ、ビルド機と OS が異なる非 Windows triple で `examples/tasks` 相当の
    `Task.parallel` を使う source の `Emit::Executable` が E2002 になり、メッセージが `set TSUZURI_SYSROOT` を含む。

| triple | 検査（little-endian） |
| --- | --- |
| Apple x86_64 / arm64 | 0..4 が `cf fa ed fe`、4..8 の cputype が `0x01000007` / `0x0100000c` |
| Linux x86_64 / aarch64 | 0..4 が `7f 45 4c 46`、byte 4 が 2（64-bit）、16..18 の `e_type` が 1、18..20 の `e_machine` が 62 / 183 |
| Windows x86_64 / aarch64 | 0..2 の Machine が `0x8664` / `0xaa64` |

### E2E

`tests/targets.mjs`（新規）。features.mjs の suite にはしない（外部ツールと host に依存するため。`tests/windows.mjs` と同じ独立 script）。

- host triple を `process.platform`・`process.arch` から決める（darwin/arm64 → `aarch64-apple-darwin` など 6 通り）。
- host triple を明示して `-O0`・`-O3` で `examples/tasks` と `examples/control` を build・実行し、出力が `21325334000\n` と `42\n`（`tests/windows.mjs` と同じ期待値）で、
  `--target native` の実行結果とも一致する。`test --target <host>` が成功する。
- 他の 5 triple: `Answer.tz` の `--emit object` の先頭 bytes を上の表で検査する。macOS ビルド機では別 arch の Apple triple の実行ファイルも作り、cputype を検査する（実行しない）。
- `TSUZURI_CROSS_LINK=1` のとき: 非 Windows の foreign triple で `examples/tasks` の実行ファイルを作り、ELF の `e_type` が 2 か 3、`e_machine` が表の値。
- `--target wasm32-unknown-unknown` と `--target wasm32` の `.wasm`（`--no-cache`）が byte 一致する。
- 拒否: 未知 target、foreign の `test`、cross の `--cpu native` を `--json` で実行し、`code` と D3 の文言の先頭を検査する。
- codegen を変えないので `live == 0` と WASM import の検査は追加しない（IR 不変は手順 5 の `cmp` と `non_windows_triples_keep_the_native_ir` が保証する）。

### 既存テストへの影響

なし。未知 target の旧メッセージを assert するテストは HEAD にない（`grep -rn "target must be" src tests` が `src/main.rs` の 1 件だけ）。
`validates_native_cpu_tuning_and_selects_architecture_flags` と `task_runtime_source_inlines_the_win32_adapter` は呼び出しの形だけ追従し、期待値は変えない。

### 性能

生成コードと実行時間は変わらない。compile 時間の変化は計測しない（引数の組み立てだけ）。

## ドキュメント

- `README.md`: 冒頭の `--target native|wasm32` の文、「ビルド」節の Windows の段落の直後に cross の段落（`TSUZURI_SYSROOT`、tier 表へのリンク）。
- `_docs/tools/command-line.md`: `--target` の値の一覧、`TSUZURI_SYSROOT`、`test` の foreign 拒否。
- `_docs/tools/build-and-cache.md`: 「プラットフォーム」節の表を tier 表へ置き換え、`### クロスコンパイル`（新規）に sysroot と linker の要件、この節の tier は CI の実態だけを書く旨。
- `docs/architecture.md`: `src/driver.rs` の説明に `Triple`・`Platform` による target の決定。
- `_docs/feature-status.md` と `_features/README.md` の G15 の状態。

## 受け入れ条件

- [ ] 6 triple と `wasm32-unknown-unknown` を `build` の `--target` で受け、未知値・foreign の `test`・cross の `--cpu native` を D3 の文言で拒否する。
- [ ] `--target native` の IR と object が HEAD と byte 一致する（手順 5・14）。
- [ ] arm64 macOS で `aarch64-apple-darwin` を build・実行でき、`x86_64-apple-darwin` の実行ファイルと Linux・Windows の object を作れる。
- [ ] 同梱 shim で Linux 2 triple の実行ファイルを link できる（手順 10）。
- [ ] `tests/targets.rs` の 5 件、`src/main.rs` と `src/driver.rs` の新しいテスト、`tests/targets.mjs` が成功する。
- [ ] 文書の tier は CI に実在する job だけから決まり、実行検証していない triple を tier 1 と書いていない。
- [ ] D8 が承認された場合、`targets.yml` の 4 job が成功した run がある。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- Apple の clang で Linux triple を link すると ld64 が `unknown options: --hash-style=gnu ...` を出す。`-fuse-ld=lld` を付け、`ld.lld` が `PATH`
  （Homebrew なら `/opt/homebrew/opt/lld/bin`）にあることを確かめる。
- sysroot がないと Linux triple の runtime C（`pthread.h`）が見つからない。object の検査には runtime を使わない `Answer.tz` を使う。
- macOS から Windows へ cross すると、一時ディレクトリに `task-windows.h` がないため quoted include が失敗する。`task_runtime_source` は target で inline する。
- `cfg!(windows)` の置き換え漏れは、Windows triple で `.exe` が付かない、`-lm` が lld-link に渡る（`could not open 'm.lib'`）形で表に出る。手順 5 の grep で残りを確かめる。
- `-fuse-ld=lld` を `-c` の compile に渡すと `argument unused during compilation` 警告が build の message に混ざる。link の呼び出しだけに渡す。`--sysroot` で同じ警告が出たら
  runtime の compile と link だけに渡す。
- Apple triple を版なし（`x86_64-apple-darwin`）で clang へ渡すと deployment target がビルド機の SDK に依存する。必ず `macosx12.0.0` 付きの clang target を使う（D4）。
- この機械は Rosetta がなく、x86_64 の Mach-O は `bad CPU type in executable` で実行できない。Rosetta・qemu・Wine を導入しない。テストは foreign の実行ファイルを実行しない。
- Zig の初回 link は target ごとに cache を作るので遅い。`ZIG_GLOBAL_CACHE_DIR` を一時ディレクトリに置き、E2E の timeout は `tests/windows.mjs` と同じ 180 秒にする。
  `zig cc` が `-fuse-ld=lld` を拒否した場合は shim がこの引数を落とす（Zig は常に lld を使う）。
- Windows triple の `--emit object` は runtime を含むと E2002（G10 の制約）。
- `TSUZURI_SYSROOT` を cache key に入れ忘れると、sysroot を替えても古い成果物が返る。
- musl の Linux（Alpine）上では `Triple::host()` が `None` なので、明示した gnu triple は cross 扱いになり `test` を拒否する。仕様どおり。

## 対象外

- 32-bit native target（i686、armv7）、big-endian target、GPU 付き target の自動検出。
- 実行器（qemu-user、Wine、Rosetta）による foreign 実行、`run --target`。
- sysroot の自動取得・管理（G14 の同梱 SDK か利用者の `TSUZURI_SYSROOT` だけ）、universal binary（`lipo`）。
- Windows 上の実行 CI（G10）、PDB、MinGW の明示 triple、Android・iOS、bare-metal（F13 の後）。
- cross 先の CPU 機能の指定（`--cpu` の新しい値や `-mcpu=<name>`）。

## 決定事項

### D1: Phase 1 の target

- 決定: `aarch64-apple-darwin`、`x86_64-apple-darwin`、`aarch64-unknown-linux-gnu`、`x86_64-unknown-linux-gnu`、`aarch64-pc-windows-msvc`、`x86_64-pc-windows-msvc` と
  wasm32。musl・FreeBSD・RISC-V・Android・iOS・MinGW は Phase 2。
- 理由: vscode.yml の 6 host と一致し、すべて 64-bit little-endian で同じ native IR を使える。RISC-V と FreeBSD は CI runner がない。
- 状態: 既定案（実装者はこの案に従う）

### D2: CLI の名前

- 決定: Rust と同じ綴りの CLI 名だけを受け、別名は `wasm32-unknown-unknown` だけ。ビルド機と同じ triple を明示しても `triple: Some` とし、clang へ `--target` を渡す。
- 理由: 名前の集合を閉じると診断と文書が固定できる。host と同じ triple を `None` へ畳むと、明示したのに出力（deployment target など）が変わる。
- 状態: 既定案（実装者はこの案に従う）

### D3: 診断

- 決定: 「診断」の表の文言。未知 target は既存の `target must be 'native' or 'wasm32'` を置き換える。新しいコードは使わない（`E2000`・`E2002`）。
- 理由: CLI・ツールの誤りは既存の `E2000`・`E2002` の範囲。旧文言を assert するテストはない。
- 状態: 既定案（実装者はこの案に従う）

### D4: macOS の deployment target

- 決定: Apple triple の clang target は `arm64-apple-macosx12.0.0` と `x86_64-apple-macosx12.0.0`。`triple: None` は HEAD どおり（clang の既定）。
- 理由: G14 の shim が使う Zig の `macos.12.0` と一致させ、同梱ツールとシステムの clang で同じ最低版にする。
- 状態: 既定案（実装者はこの案に従う）

### D5: IR は target に依存させない

- 決定: 生成 IR に `target triple`・`target datalayout` を足さない。triple による IR の違いは Windows triple の `windows_abi` だけ。
- 理由: HEAD の IR と埋め込み runtime IR は target 中立で、Phase 1 の datalayout は pointer 幅以外（wasm32 と native の差は既存）で一致する。IR cache と決定性をそのまま保てる。
- 状態: 既定案（実装者はこの案に従う）

### D6: cross の定義

- 決定: `cross()` は `triple` が `Some` かつ `Triple::host()` と異なること。cross では `--cpu native` と `test` を拒否する。`run` は HEAD どおり `--target` を受けない。
- 理由: ビルド機の ISA とプロセスは foreign target と無関係。実行器は Phase 1 の対象外。
- 状態: 既定案（実装者はこの案に従う）

### D7: tier

- 決定: tier 1 = CI で実行ファイルを作り matching runner で `-O0`・`-O3` の E2E を実行、tier 2 = CI で object を作り形式を検査、tier 3 = 受け付けるが CI なし。
  文書の tier はその時点で実在する CI job だけから決める。Windows triple は G10 が done になるまで 2 を超えない。
- 理由: 旧「最善努力」を具体化し、未検証の target を「対応」と書かないため。
- 状態: 既定案（実装者はこの案に従う）

### D8: CI workflow

- 決定: `.github/workflows/targets.yml`（新規）。trigger は `workflow_dispatch` と、`src/**`・`std/**`・`tests/**`・`Cargo.*`・この workflow を変更する `pull_request`。
  matrix は `macos-15`・`macos-15-intel`・`ubuntu-24.04`・`ubuntu-24.04-arm`。各 job は LLVM 21 と Node 24 を導入し、`cargo test --locked`、
  `cargo build --release --locked`、`node tests/e2e.mjs`、`node tests/targets.mjs` を実行する。Windows triple の object は `tests/targets.mjs` が Linux job で作る。
- 理由: 旧未決事項の既定案（標準 runner で賄える target を tier 1）を具体化した。4 job × pull request ごとの runner 時間がかかる。
- 状態: 要承認（承認前は手順 12 に着手しない）

### D9: G10 の扱い

- 決定: G10 の done を待たずに着手する。G10 の実装済み部分を前提にし、Windows triple は tier 2 に留める。
- 理由: G10 は Windows 上の実行だけが未確認で、G15 が必要とする IR 書き換えと引数はすでに HEAD にある。
- 見直し提案: メタデータの依存 `G10` は「Windows triple の tier 1 昇格の条件」と読み替える。表の変更は人間が判断する。
- 状態: 既定案（実装者はこの案に従う）

### D10: 同梱 shim の写像

- 決定: `arm64-apple-macosx12.0.0` → `aarch64-macos.12.0`、`x86_64-apple-macosx12.0.0` → `x86_64-macos.12.0`、`aarch64-unknown-linux-gnu` → `aarch64-linux-gnu`、
  `x86_64-unknown-linux-gnu` → `x86_64-linux-gnu`、`<arch>-pc-windows-msvc` → `<arch>-windows-gnu`（HEAD の Windows host と同じ）。`-x ir` の compile は受け取った clang target を使う。
- 理由: Zig の libc は MSVC CRT を持たないため、Windows は HEAD の VSIX と同じ MinGW CRT に揃える。
- 状態: 既定案（実装者はこの案に従う）

### D11: sysroot と linker

- 決定: `--sysroot` の CLI option は作らず、環境変数 `TSUZURI_SYSROOT`（新規）を native 系の clang 呼び出しへ `--sysroot=DIR` として渡す。cross で target が macOS 以外なら
  link の呼び出しに `-fuse-ld=lld` を足す。
- 理由: ツールの指定は HEAD でも `TSUZURI_CLANG` などの環境変数で、`BuildOptions` を `Copy` のまま保てる。Apple の ld64 は ELF・COFF を link できない。
- 状態: 既定案（実装者はこの案に従う）

### D12: Linux の libc と Windows の実行ファイル

- 決定: 明示 Linux triple は glibc（`-gnu`）。同梱 shim の `--target native` が Linux で musl を使う HEAD の挙動は変えない。Windows triple の実行ファイルは Windows host か
  同梱 shim（MinGW CRT）で作る。非 Windows host の素の clang では link が E2002（D3 の hint）になる。MSVC SDK による cross-link は `tests/windows.rs` の
  `TSUZURI_WINDOWS_SDK` 付きテストだけで確かめる。MinGW の明示 triple は需要が示されるまで対象外。
- 理由: CI runner と一般の Linux 配布物は glibc。MSVC の SDK/CRT は再配布できず同梱しない。
- 状態: 既定案（実装者はこの案に従う）
