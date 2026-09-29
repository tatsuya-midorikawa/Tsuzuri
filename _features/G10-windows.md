# G10: Windows ネイティブ対応

| 項目 | 内容 |
| --- | --- |
| ID | G10 |
| 優先度 | P3 |
| 規模 | M |
| 依存 | – |
| 後続 | G08（Windows debug 形式の拡張）、F01（ワーカープールの Windows 実装） |
| 状態 | blocked |
| 起票 | P3 計画。2026-09-28 に Phase 1 を実装（実行ゲートだけ未確認）。2026-09-29 残作業（Windows 実行ゲート）を実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 不要（ただし GitHub への push と workflow の実行は人間が行う。D3） |
| 手本にする既存実装 | LLVM 21.1.8 の SHA-256 固定導入: `.github/workflows/vscode.yml` の `LLVM on Windows` step。削除された専用 workflow: `git show e02cf02:.github/workflows/windows.yml`。実行ゲートの記録形式: `_features/README.md` の「P3 実装・総合検証」節の G10 行 |
| 主な影響ファイル | `.github/workflows/windows.yml`（復元）, `tests/windows.mjs`, `tests/windows.rs`, `tests/windows_runtime.c`, `tests/task_runtime.c`, `src/runtime/task.c`, `src/runtime/task-windows.h`, `src/driver.rs`, `src/llvm.rs`（失敗の修正が要る場合だけ。D6）, `README.md`, `docs/architecture.md`, `_docs/tools/build-and-cache.md`, `_docs/get-started.md`, `_docs/README.md`, `_docs/feature-status.md`, `_features/README.md`, `_features/G10-windows.md` |

## 目的

Windows の GitHub Actions runner と開発者環境で、既存の native executable / object / header 出力を安全に生成・実行できるようにする。
Phase 1 の対象は x86_64 Windows（MSVC ABI）で、既存言語機能と `Task.parallel` を含む native tests を通す。

Phase 1 の実装は 2026-09-28 に済んでいる（「実装済みの内容」）。このチケットの残作業は、実 Windows host（x64 実機または
GitHub Actions の Windows runner）で実行ゲートを通し、専用 workflow を復元し、結果を記録して状態を `done` にすることだけである。
新機能（ARM64 Windows の正式対応、PDB、MinGW など）は足さない。

## 着手条件と停止条件

### 着手条件

- 次のどれかを使える。
  - x64 Windows 実機（Windows 10 22H2 以降、Windows 11、Windows Server 2022 以降）で、ソフトウェアを導入できる管理者権限がある。
  - GitHub Actions の `windows-2022` runner（人間が push と workflow の実行を行う。D3）。
  - ARM64 Windows 実機は x64 エミュレーション経路（D5）でだけ使える。ARM64 native の結果はゲートに数えない。
- 作業ツリーが `f8dc655` を含む。確認: `git merge-base --is-ancestor f8dc655 HEAD && echo ok` が `ok`。
- `_features/README.md` の一覧で G10 が `blocked` のまま。確認: `grep -n "^| G10 " _features/README.md`。
- 承認は不要（D1〜D8 はすべて既定案）。

### 停止条件

次の場合は即興で回避せず、作業を止めて、失敗したコマンド・出力全文・環境（「実装手順」手順 2 の版一覧）を報告する（GUIDE §13）。

- 修正に `unsafe`、新しい crate、`Cargo.toml` の依存変更が要る（GUIDE D-29 の前提が崩れる）。
- 既存テストの期待値（出力文字列、診断コード、件数、終了コード）を変えないと通らない。D6 の「harness だけの修正」は除く。
- 修正が macOS / Linux / WASM の挙動や生成 IR を変える（`cfg(windows)`・`cfg!(windows)`・`process.platform === "win32"` の分岐、
  `src/runtime/task-windows.h`、`tests/windows*` の外を変える）。
- 既存出力がある状態で `fs::rename` による置換が失敗する（D2）。古い出力を消してから rename する回避は禁止。
- hard link の出力が `E2003` で拒否されない（出力保護の不変条件）。
- Win32 task runtime が停止しない、結果が違う、failure injection の診断文が違う。
- 生成 IR の compile・link で未解決シンボルや不正 IR が出る。link flag やライブラリを場当たりで足さない。
- LLVM 21.1.8・Visual Studio Build Tools・Windows SDK のどれかを用意できない。別版の LLVM で得た結果はゲートに数えない（参考記録だけ）。
- 同じ失敗が macOS / Linux でも再現する（G10 の範囲外の不具合）。
- テストを飛ばす、`#[ignore]` を付ける、workflow から step を外す、timeout を延ばすことで緑にしたくなった。

## 現状（HEAD `f8dc655` で確認）

- native の clang 引数は `src/driver.rs` の `native_compile_args(windows, architecture)` が決める。Windows では
  `env::consts::ARCH` が `aarch64` なら `--target=aarch64-pc-windows-msvc`、それ以外は `--target=x86_64-pc-windows-msvc`。
  Windows 以外は `-fPIC`。`-pthread`・`-lm` は `!cfg!(windows)` のときだけ渡す。出力拡張子は `Emit::Executable if cfg!(windows) => "exe"`
  と `Emit::Object if cfg!(windows) => "obj"` の分岐で選ぶ。
- `src/llvm.rs` の `windows_abi(ir, module)` は、`exported` な関数の `tz_<name>` と console 入口の `tsuzuri_main` の `define` を
  `define dllexport` に書き換え、`declare i64 @write(i32, ptr, i64)` を `_write`（32-bit count）へ委ねる internal `@write` に置き換え、
  `@tz.console.write` の入口で `_setmode(1, 32768)`（binary mode）を呼ぶ。コードページは変えない。`src/driver.rs` は `cfg!(windows)` かつ
  `Target::Native` かつ emit が `Header`・`Wgsl` 以外のときに呼ぶ。テスト用に `llvm::emit_with_export_style` と
  `ExportStyle::{Default, Dllexport}` があり、wasm と `Dllexport` の組は `E2000`（`dllexport requires a native target`）。
- task・CPU・IO runtime を含む Windows の `--emit object` は `E2002`（`Windows COFF objects with embedded task, CPU, or IO runtime are not supported; emit LLVM and link the runtime once, or build an executable`）。
- `task_runtime_source()` は Windows で `src/runtime/task.c` の `#include "task-windows.h"` を `src/runtime/task-windows.h` の本文で
  置き換える（verbatim path `\\?\` の隣では Clang が引用 include を解決できないため）。`task-windows.h` は既存 scheduler の
  pthread 呼び出しを SRWLOCK・CONDITION_VARIABLE・INIT_ONCE・`CreateThread`・`WaitForSingleObject`・`CloseHandle` へ写し、失敗時は
  `tz_windows_fail` が `Tsuzuri task runtime: <operation> failed (<GetLastError>)` を stderr に出して abort する。差し替え用の
  `TZ_TASK_CREATE_THREAD`・`TZ_TASK_WAIT_THREAD`・`TZ_TASK_CLOSE_THREAD` がある。
- 出力保護: `protect_source` は symlink・directory・特殊ファイルと、`fs::canonicalize` の一致または `same_file` を `E2003` にする。
  `#[cfg(windows)] same_file` は `same_file::is_same_file`（`Cargo.toml` の `[target.'cfg(windows)'.dependencies]` の
  `same-file = "1.0.6"`）。生成物は一時ディレクトリに作り、`publish_outputs` が `fs::rename(artifact, output)` で既存出力を置き換える。
- Windows 専用テスト:
  - `tests/windows.rs`: `emits_windows_exports_without_changing_default_or_wasm`（全 OS）、`cross_links_with_microsoft_sdk_when_requested`
    （`TSUZURI_WINDOWS_SDK` がなければ何もしない）、`#[cfg(windows)] windows_outputs_protect_hardlinks_and_replace_existing_files`。
  - `tests/windows.mjs`: win32 以外では最初に throw する。O0/O3 で UTF-8 出力、同じ exe への再 build、clang 不在の `E2002` と旧出力の保持、
    `examples/tasks`（`21325334000`）・`examples/control`（`42`）、`tests/task_runtime.c` の topology `-1`・`1`・`4`・`1000`、
    `tests/windows_runtime.c` の `create`・`wait`・`close` 注入、`dllexport` の IR と C host の link・実行、runtime 入り object の `E2002`、
    hard link の `E2003`、`--emit object` の `tz_answer` と C host、wasm32 の import なしを確かめ、最後に
    `Windows MSVC: native O0/O3, ... and WASM passed` を出す。
  - `tests/lsp.rs` の `#[cfg(windows)] windows_file_uris_round_trip_drive_letters_and_canonical_paths`。
- `process.platform === "win32"` の分岐を持つ harness: `tests/e2e.mjs`, `tests/examples.mjs`, `tests/cache.mjs`,
  `tests/numeric_casts.mjs`, `tests/gpu.mjs`, `tests/lsp_sessions.mjs`。`-pthread` を無条件に渡す POSIX 専用 harness:
  `tests/tasks.mjs`, `tests/features.mjs`, `tests/strings.mjs`, `tests/host_imports.mjs`, `tests/cpu_dispatch.mjs`, `tests/debug_info.mjs`。
- CI: HEAD の `.github/workflows/` は `vscode.yml` だけ。`vscode.yml` は `windows-2022`・`windows-11-arm` で fmt・clippy・拡張機能の
  テスト・`tests/lsp_sessions.mjs` を実行するが、`cargo test` と `tests/windows.mjs` は実行しない。専用の `windows.yml` は `e02cf02`
  （2026-09-28）で追加され、`16d4fa9`（2026-09-28、`Refactor implicit computation handling and enhance error diagnostics`）で削除された。
  削除理由の記録はない。G15 も HEAD に `windows.yml` がないことを記している。
- 文書はすべて「Windows 上の実行は未確認」と書く: `README.md` の「ビルド」節、`_docs/tools/build-and-cache.md` の「プラットフォーム」節、
  `_docs/get-started.md`、`_docs/README.md`、`docs/architecture.md` の `**Windows MSVC:**` 段落、`_docs/feature-status.md` の G10 行、
  `_features/README.md` の「P3 実装・総合検証」節と一覧の G10 行。

### 再現（macOS で確認）

Windows 以外では専用 E2E が最初に止まる。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
node tests/windows.mjs target/release/tsuzuri
# Error: Windows E2E requires a Windows host; use TSUZURI_WINDOWS_SDK cargo test --test windows for cross-link validation
git log --oneline --name-status -- .github/workflows/windows.yml
# 16d4fa9 ... D .github/workflows/windows.yml
# e02cf02 ... A .github/workflows/windows.yml
```

### 実装済みの内容（2026-09-28 の記録）

- 実装: native flags、公開 wrapper に限った `dllexport`、CRT への UTF-8 bytes 出力、safe な hard link 同一性、Win32 常駐 pool と
  Result の cancellation、言語内 test runner（`src/test_runner.rs` の Windows 分岐）、専用 E2E（`tests/windows.mjs`）と CI（`windows.yml`。
  後に削除）。
- runtime を同梱する COFF object はチケットの既定案どおり `E2002`。LLVM IR と runtime を明示的に一度だけ link する経路は使える。
  Windows の CPU dispatch は portable baseline。
- hard link 判定は、stable Rust の `MetadataExt::file_index`・`volume_serial_number` が不安定 API（`windows_by_handle`）だったため、
  Windows 限定の `same-file` 1.0.6 を追加した（利用者の判断委任による。GUIDE D-29）。
- macOS 上で実 Windows SDK/CRT を使い、runtime と failure harness を `-Wall -Wextra -Werror` で COFF にし、生成 IR と Win32 runtime を
  O0/O3 で PE に link した。全 Rust target の Windows cfg 検査と fmt・clippy が成功した。
- macOS の既存テスト（tasks 41 件・trap 4 件、examples 32,600 steps、test runner、native/WASM の O0/O3）は成功した。
- 未確認: Windows runner 上の `cargo test`、native 実行、Win32 failure injection の実行、hard link 拒否と既存出力置換の実行。
  cross の型検査・COFF/PE link の成功は Windows での実行成功として扱わない。この未確認分がこのチケットの残作業である。

## 仕様

Phase 1 の仕様は実装済みで、このチケットでは変えない。ここでは実行ゲートで確かめる契約と、ゲートの合格条件を定める。

### 前提とする他チケットのインターフェース

- 依存はない。後続の G15 は G10 の実装済み部分（`windows_abi`、`native_compile_args` の Windows 分岐、`task_runtime_source`）を前提にし、
  「G10 の job」で Windows の tier を上げる。したがって workflow の名前 `Windows MSVC` と job id `windows` は D3 のとおり固定する。
- G14 は依存に `(G10)` を持つ。G10 は G14 の同梱 toolchain（`vsc/` の VSIX）に依存しない。ゲートは開発者の toolchain（D1）で取る。

### サポート範囲（Phase 1）

| 項目 | 内容 |
| --- | --- |
| host | x64 Windows。CI は GitHub Actions `windows-2022` |
| target | `x86_64-pc-windows-msvc`（`native_compile_args` が `--target=` で渡す） |
| toolchain | LLVM 21.1.8 の `clang`（driver 形式。`clang-cl` 専用の引数形式は対象外）、Visual Studio Build Tools 2022 の MSVC と CRT、Windows SDK |
| link | コンパイラの exe は clang driver の既定 linker（MSVC `link.exe`）。`tests/windows.mjs` の C host は `-fuse-ld=lld` |
| emit | `exe`（`.exe`）、`object`（`.obj`。runtime を含むものは `E2002`）、`llvm`、`header`。`--target wasm32` は OS 非依存で変更なし |
| `Task.parallel` | Win32 threads（既存 scheduler に `task-windows.h` を当てる）。WASM の backend は変更なし |
| CPU dispatch | portable baseline |
| ARM64 Windows | driver は `aarch64-pc-windows-msvc` を選ぶが未検証。ゲートに含めない（D5） |

### 公開 ABI・console・出力保護（実装済みの契約）

- `exported` な関数の `tz_<name>` と console 入口の `tsuzuri_main` だけが `dllexport` を持つ。内部関数 `@tz.fn.*` は internal のまま。
  C header は `__declspec(dllimport)` を付けない（object を直接 link する利用者が主対象）。
- stdout・stderr へは UTF-8 bytes をそのまま書く。fd を binary mode にし、コードページは変えない。言語の文字列出力の意味は変えない。
- source と同じファイル（hard link を含む）への出力は `E2003`。symlink・directory・特殊ファイルへの出力も `E2003`。
- 既存出力の置換は一時ファイルからの `fs::rename` だけで行う。Rust std の Windows 実装は既存ファイルを置き換える rename を使う。
  置換に失敗したら出力を消して再試行する fallback は使わない（D2）。
- Win32 task runtime の失敗（`CreateThread`・`WaitForSingleObject`・`CloseHandle`・`InitOnceExecuteOnce` など）は
  `Tsuzuri task runtime: <operation> failed (<code>)` を stderr に出して abort し、成功形の結果を返さない。worker 上限
  `TZ_TASK_MAX_THREADS = 32`、呼び出し元も work を実行すること、入れ子の逐次 fallback は POSIX 版と共通の `src/runtime/task.c` による。

### 実行ゲート

次の W1〜W8 がすべて、D1 の環境の Windows x64 上で成功したときにゲート合格とする。コマンドは repository root で、
「実装手順」手順 3 の環境変数を設定した Developer PowerShell（CI では workflow）から実行する。

| ID | コマンド | 合格の条件 |
| --- | --- | --- |
| W1 | `cargo fmt --all -- --check` | 終了コード 0、出力なし |
| W2 | `cargo clippy --locked --all-targets -- -D warnings` | 終了コード 0 |
| W3 | `cargo test --locked` | 全 binary が `test result: ok`、`0 failed`。`tests/windows.rs` は `3 passed`、`tests/lsp.rs` に `windows_file_uris_round_trip_drive_letters_and_canonical_paths ... ok` が出る |
| W4 | `cargo build --release --locked` | `target\release\tsuzuri.exe` ができる |
| W5 | `node tests/windows.mjs target/release/tsuzuri.exe` | 最後の行が `Windows MSVC: native O0/O3, Win32 tasks/results/failure injection, UTF-8, hardlinks, atomic replacement, object ABI and WASM passed` |
| W6 | `node tests/cache.mjs target/release/tsuzuri.exe` | 最後の行が `Cache: SHA-256, byte-identical hits, source/options/tools invalidation, corruption repair, concurrent writes, no-cache, output protection, executable permissions and sidecars passed` |
| W7 | `node tests/examples.mjs target/release/tsuzuri.exe` | 最後の行が `Examples: console, higher-order functions, native C host, desktop ABI, HTTP/WASM loading, 32,600 game steps`（Windows では desktop の Python 部分を harness が飛ばす） |
| W8 | `node tests/wasm_threads.mjs target/release/tsuzuri.exe` | 終了コード 0。`WASM threads O0:`・`WASM tasks O0:`・`WASM threads O3:`・`WASM tasks O3:` で始まる行が出る |

参考記録（ゲートに含めない。D4）: `node tests/e2e.mjs target/release/tsuzuri.exe`、`node tests/numeric_casts.mjs target/release/tsuzuri.exe`。

### 診断

新しい診断はない。実行ゲートで出ることを確かめる既存の診断は次のとおり（位置はすべて `Span::default()`）。

| コード | 条件 | メッセージ |
| --- | --- | --- |
| `E2002` | Windows で task・CPU・IO runtime を含む `--emit object` | `Windows COFF objects with embedded task, CPU, or IO runtime are not supported; emit LLVM and link the runtime once, or build an executable` |
| `E2002` | `TSUZURI_CLANG` の実行ファイルがない | `install LLVM/Clang 17+ or set TSUZURI_CLANG to its executable` を含む |
| `E2003` | 出力が source と同じファイル（hard link を含む） | `refusing to overwrite the input source` |
| `E2003` | 出力が symlink・directory・特殊ファイル | `the output must not be a symlink, directory, or special file` |
| `E2001` | Windows のファイル同一性を調べられない | `cannot compare file identity '<path>': <error>` |
| `E2000` | `llvm::emit_with_export_style` に wasm と `ExportStyle::Dllexport` を渡した（API だけ） | `dllexport requires a native target` |

### 数値・トラップ・native と WASM の差

- Windows native の IR は、共通の生成 IR に `windows_abi` の書き換え（`dllexport`、`_write`・`_setmode`、`src/runtime/wasm.ll` の 128-bit helper の
  同梱）を足したものだけが異なる。整数・浮動小数点・trap の意味は変わらない。
- `--target wasm32` の出力は host OS に依存しない。W5 は wasm32 の module が import を持たないこと（D-18）も確かめる。

### 資源上限

- `TZ_TASK_MAX_THREADS = 32`（`src/runtime/task.c`。変更しない）。
- `tests/windows.mjs` の子プロセスは 1 回 180,000 ms で timeout する。workflow の job は `timeout-minutes: 90`（D3）。

### 例

`tests/windows.mjs` が確かめる動作。Windows では未実行なので、期待値は harness の assert から写した。

```powershell
target\release\tsuzuri.exe run examples\control -O3      # stdout: 42
target\release\tsuzuri.exe run examples\tasks -O0        # stdout: 21325334000
target\release\tsuzuri.exe build tests\fixtures\tasks\Main.tz --emit object -o rejected.obj --json
# stderr の JSON の code: E2002（runtime を含む COFF object）
```

## 設計

### データ構造

変更しない。使う既存の名前: `llvm::ExportStyle`、`llvm::EmitOptions`、`llvm::emit_with_export_style`、`llvm::windows_abi`、
`src/driver.rs` の `native_compile_args`・`task_runtime_source`・`protect_source`・`same_file`・`publish_outputs`・`tool`。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| CI | `.github/workflows/windows.yml` | workflow `Windows MSVC`、job `windows` | 下の「CI workflow」の内容で復元する（D3・D7・D8） |
| 修正（条件付き） | `tests/windows.mjs`, `tests/cache.mjs`, `tests/examples.mjs`, `tests/wasm_threads.mjs`, `tests/*.rs` | 失敗した harness の該当箇所 | D6 の範囲（パス区切り、`.exe`、Windows で渡せない flag）だけ直す |
| 修正（条件付き） | `src/driver.rs`, `src/cache.rs`, `src/lsp.rs`, `src/test_runner.rs` | `cfg(windows)`・`cfg!(windows)` の分岐 | Windows だけで起きる不具合を、Windows 分岐の中で直す（D6）。それ以外は停止条件 |
| 修正（条件付き） | `src/runtime/task-windows.h`, `src/llvm.rs` | Win32 adapter、`windows_abi` | 同上。`src/runtime/task.c` の共通部分は変えない |
| 記録 | `_features/G10-windows.md` | `### Windows 実行記録（<日付>）`（新規） | 「現状」の末尾に環境と W1〜W8 の結果を書く（手順 11） |
| 文書 | `README.md` ほか「ドキュメント」の一覧 | 「未確認」の記述 | 実行した環境と結果へ書き換える |
| 状態 | `_features/README.md`, `_docs/feature-status.md` | G10 の行 | `blocked` → `done`。チケットを `_features/_completed/` へ移す |

### 生成 IR とランタイム

- exe の build: 生成 IR に `windows_abi` を当てて `module.ll` に書き、`TSUZURI_CLANG` に `native_compile_args(true, "x86_64")` の
  `--target=x86_64-pc-windows-msvc` を渡して compile・link する。`Task.parallel` を使うときは `task_runtime_source()` の C を同じ clang で
  compile して一度だけ link する。`-fPIC`・`-pthread`・`-lm` は渡さない。
- clang の MSVC target は `link.exe`・CRT・SDK を Visual Studio の環境（Developer PowerShell、CI では `ilammy/msvc-dev-cmd`）から見つける。
  この環境がないと link が `LNK1104` などで失敗する（「落とし穴」）。

### CI workflow

削除版（`git show e02cf02:.github/workflows/windows.yml`）を土台に、D3・D7・D8 の差分を入れた形。`vscode.yml` の `LLVM on Windows` と
同じ installer・SHA-256 を使う。

```yaml
name: Windows MSVC

on:
  push:
  pull_request:
  workflow_dispatch:

permissions:
  contents: read

jobs:
  windows:
    runs-on: windows-2022
    timeout-minutes: 90
    steps:
      - name: Keep LF line endings
        run: git config --global core.autocrlf false
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: actions/setup-node@v4
        with:
          node-version: 24
      - uses: ilammy/msvc-dev-cmd@v1
        with:
          arch: x64
      - name: LLVM 21.1.8
        shell: pwsh
        run: |
          $asset = 'LLVM-21.1.8-win64.exe'
          $expected = '7a5386c26497db1691f320121e5b113364dd0274b98e55f15f4dbc00c0450113'
          $installer = Join-Path $env:RUNNER_TEMP $asset
          Invoke-WebRequest "https://github.com/llvm/llvm-project/releases/download/llvmorg-21.1.8/$asset" -OutFile $installer
          if ((Get-FileHash $installer -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expected) { throw 'LLVM checksum mismatch' }
          $prefix = Join-Path $env:RUNNER_TEMP 'llvm21'
          $process = Start-Process $installer -ArgumentList '/S', "/D=$prefix" -Wait -PassThru
          if ($process.ExitCode -ne 0) { throw "LLVM installation failed: $($process.ExitCode)" }
          "$prefix\bin" | Out-File -FilePath $env:GITHUB_PATH -Append
          "TSUZURI_CLANG=$prefix\bin\clang.exe" | Out-File -FilePath $env:GITHUB_ENV -Append
          "TSUZURI_WASM_LD=$prefix\bin\wasm-ld.exe" | Out-File -FilePath $env:GITHUB_ENV -Append
      - name: Show tools
        shell: pwsh
        run: |
          & $env:TSUZURI_CLANG --version
          & $env:TSUZURI_WASM_LD --version
          node --version
          rustc --version
      - run: cargo fmt --all -- --check
      - run: cargo clippy --locked --all-targets -- -D warnings
      - run: cargo test --locked
      - run: cargo build --release --locked
      - run: node tests/windows.mjs target/release/tsuzuri.exe
      - run: node tests/cache.mjs target/release/tsuzuri.exe
      - run: node tests/examples.mjs target/release/tsuzuri.exe
      - run: node tests/wasm_threads.mjs target/release/tsuzuri.exe
```

削除版との差: `runs-on` を `windows-latest` から `windows-2022` に固定、runner 付属の `C:\Program Files\LLVM`（版が image ごとに変わる）を
使わず LLVM 21.1.8 を SHA-256 付きで導入、`TSUZURI_CLANG`・`TSUZURI_WASM_LD` を明示、`core.autocrlf` を無効化、`workflow_dispatch` と
`timeout-minutes` を追加、clippy に `--locked` を追加。

## 実装手順

コードの実装は済んでいる。手順は Windows での実行、workflow の復元、記録だけである。コマンドは PowerShell 7（`pwsh`）で書く。
`cargo test --locked <filter>` は 0 件でも成功するので、`running N tests` の N を必ず見る。ログは `target\g10\`（Git 管理外）に保存する。

### 手順 1: 作業ツリーと履歴の確認

- 変更: なし。
- 内容: 基点と、削除された workflow を確かめる（どの OS でもよい）。
- 確認: 次の出力になる。

```sh
git merge-base --is-ancestor f8dc655 HEAD && echo ok                 # ok
git log --oneline --name-status -- .github/workflows/windows.yml      # 16d4fa9 の D と e02cf02 の A の 2 件
ls .github/workflows                                                  # vscode.yml だけ（復元前）
git show e02cf02:.github/workflows/windows.yml | head -3              # name: Windows MSVC
```

### 手順 2: Windows の道具を入れる（実機。runner では不要）

- 変更: なし（host の設定だけ）。
- 内容: 管理者 PowerShell で入れる。LLVM は `vscode.yml` と同じ installer と SHA-256 を使う（D1）。ARM64 実機は D5 の x64 経路。

```powershell
winget install --id Microsoft.VisualStudio.2022.BuildTools -e --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
winget install --id Rustlang.Rustup -e
rustup default stable-x86_64-pc-windows-msvc
rustup component add rustfmt clippy
winget install --id OpenJS.NodeJS -e --version 24.x   # 失敗したら nodejs.org の v24 x64 MSI
$asset = 'LLVM-21.1.8-win64.exe'; $installer = "$env:TEMP\$asset"
Invoke-WebRequest "https://github.com/llvm/llvm-project/releases/download/llvmorg-21.1.8/$asset" -OutFile $installer
(Get-FileHash $installer -Algorithm SHA256).Hash.ToLowerInvariant()   # 7a5386c26497db1691f320121e5b113364dd0274b98e55f15f4dbc00c0450113
Start-Process $installer -ArgumentList '/S', '/D=C:\llvm21' -Wait
git clone -c core.autocrlf=false <利用者の remote> C:\src\Tsuzuri
```

- 確認: `rustc -vV` の `host:` が `x86_64-pc-windows-msvc` で `release:` が 1.88 以上（`src/driver.rs` が `if ... && let` を使う）。
  `node --version` が `v24.`、`C:\llvm21\bin\clang.exe --version` が `clang version 21.1.8`、
  `git -C C:\src\Tsuzuri config core.autocrlf` が `false`。hash が一致しなければ停止する。

### 手順 3: シェルと環境変数

- 変更: なし。
- 内容: 毎回この順で開く。Git Bash は使わない（「落とし穴」）。

```powershell
& "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\Launch-VsDevShell.ps1" -Arch amd64 -HostArch amd64
$env:TSUZURI_CLANG = 'C:\llvm21\bin\clang.exe'
$env:TSUZURI_WASM_LD = 'C:\llvm21\bin\wasm-ld.exe'
$env:PATH = "C:\llvm21\bin;$env:PATH"
Set-Location C:\src\Tsuzuri; New-Item -ItemType Directory -Force target\g10 | Out-Null
```

- 確認: `(Get-Command clang).Source` が `C:\llvm21\bin\clang.exe`、`(where.exe link)[0]` が `...\VC\Tools\MSVC\<版>\bin\Hostx64\x64\link.exe`、
  `& $env:TSUZURI_WASM_LD --version` が `LLD 21.1.8`、`git ls-files --eol src/runtime/string.ll` が `i/lf w/lf`。

### 手順 4: W1〜W4（Rust）

- 変更: なし。
- 内容: 仕様の W1〜W4 を順に実行し、ログを残す。

```powershell
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings 2>&1 | Tee-Object target\g10\clippy.log
cargo test --locked 2>&1 | Tee-Object target\g10\cargo-test.log
cargo test --locked --test windows
cargo test --locked --test lsp windows_file_uris
cargo build --release --locked
```

- 確認: W1〜W4 の条件。`--test windows` は `3 passed`、`--test lsp windows_file_uris` は `1 passed`。
  `Select-String -Path target\g10\cargo-test.log -Pattern 'FAILED|panicked'` が何も出さない。失敗したら手順 8。

### 手順 5: W5（Windows 専用 E2E）

- 変更: なし。
- 内容: `node tests/windows.mjs target/release/tsuzuri.exe 2>&1 | Tee-Object target\g10\windows-mjs.log`。
- 確認: 最後の行が W5 の文。途中の assert は `AssertionError` とコマンド行を出すので、それを手順 8 で分類する。

### 手順 6: W6〜W8（共通 E2E）

- 変更: なし。
- 内容: `tests/cache.mjs`、`tests/examples.mjs`、`tests/wasm_threads.mjs` を `target/release/tsuzuri.exe` で実行し、`target\g10\` に保存する。
- 確認: W6〜W8 の条件。`examples.mjs` は引数を省くと `target/debug/tsuzuri` を探すので必ず渡す。

### 手順 7: 参考記録

- 変更: なし。
- 内容: `node tests/e2e.mjs target/release/tsuzuri.exe` と `node tests/numeric_casts.mjs target/release/tsuzuri.exe` を実行する（D4）。
- 確認: 結果（成功、または最初の失敗の assert と原因の分類）を手順 11 の記録に書く。失敗してもゲートは止めない。
  `tests/tasks.mjs` など「現状」の POSIX 専用 harness は実行しない。

### 手順 8: 失敗を分類して対応する

- 変更: D6 で許す範囲だけ。
- 内容: 失敗ごとに次の表で分類し、「修正」なら直して、失敗した W から再実行する。「停止」なら停止条件どおり報告する。

| 分類 | 見分け方 | 対応 |
| --- | --- | --- |
| 道具 | `E2002` と `install LLVM/Clang 17+ or set TSUZURI_CLANG`、`clang version` が 21.1.8 でない、`'clang' is not recognized` | 修正（手順 2・3 をやり直す。コードは変えない） |
| MSVC 環境 | `LNK1104: cannot open file 'libcmt.lib'`・`'kernel32.lib'`、`'windows.h' file not found`、`link: extra operand` | 修正（Developer PowerShell、VCTools workload と Windows SDK を確認） |
| 改行 | `git ls-files --eol` に `w/crlf`、期待値との差が `\r` だけ | 修正（`core.autocrlf=false` で clone し直す。D7） |
| 長いパス | `os error 206`、`The filename or extension is too long` | 修正（`C:\src\Tsuzuri` のような短い場所へ移す） |
| ファイルのロック | rename・削除で `os error 5`・`os error 32`、実機だけで起きる | 修正（残ったプロセスを止め、Defender の除外に作業ディレクトリを足して 1 回だけ再実行）。runner でも起きたら停止（D2） |
| harness | パス区切り、`.exe` の欠落、`which`、Windows で渡せない flag（`-pthread`・`-lm`・`-fPIC`） | 修正（D6。期待値は変えない。同じ suite を macOS か Linux でも通す） |
| Windows 分岐の不具合 | `cfg(windows)`・`cfg!(windows)` の分岐内だけが原因（拡張子、ドライブ文字、`\\?\` パス、cache の場所） | 修正（D6）。回帰テストを `tests/windows.rs` に `#[cfg(windows)]` で足す |
| Win32 runtime | `ETIMEDOUT`、scheduler の abort、`<name> failed` の文の不一致、結果の不一致 | 停止 |
| 生成コード・link | `undefined symbol`、`LNK2019`・`LNK2001`、不正 IR | 停止（`--emit llvm` の IR を添える） |
| 出力保護 | hard link が `E2003` にならない、失敗した build の後に旧出力が変わる | 停止 |
| 共通の不具合 | 同じ失敗が macOS か Linux でも起きる | 停止（G10 の範囲外） |

- 確認: 修正したら、失敗した W 以降を再実行して成功する。コードを変えた場合は macOS か Linux で `cargo test --locked` が成功し、
  `rustup target add x86_64-pc-windows-msvc` の後の `cargo clippy --locked --all-targets --target x86_64-pc-windows-msvc -- -D warnings` が成功する。

### 手順 9: workflow を復元する

- 変更: `.github/workflows/windows.yml`。
- 内容: 設計の「CI workflow」の YAML をそのまま書く。push はしない（D3）。
- 確認: `git status --short` が `?? .github/workflows/windows.yml` を含み、差分が設計の「削除版との差」だけであること:
  `git show e02cf02:.github/workflows/windows.yml > target/g10/old.yml; git diff --no-index target/g10/old.yml .github/workflows/windows.yml`。

### 手順 10: GitHub 上の実行（人間が push した後）

- 変更: なし。
- 内容: 人間に push と `Windows MSVC` の実行（`workflow_dispatch` か pull request）を依頼する。
- 確認: run の全 step が緑で、`Show tools` が `clang version 21.1.8` を出す。`vscode.yml` の Windows job も緑のまま。
  runner だけで失敗したら手順 8 で分類する（多いのは改行と PATH の順序）。

### 手順 11: 結果を記録する

- 変更: `_features/G10-windows.md`。
- 内容: 「現状」の末尾に `### Windows 実行記録（<YYYY-MM-DD>）`（新規）を足す。「実装済みの内容」は書き換えない。

```text
- 環境: <実機 x64 | GitHub windows-2022 run <URL>>、OS <winver の版>、CPU <名前・論理コア数>
- 版: rustc <版>、clang 21.1.8、node <版>、MSVC <link.exe の版>、Windows SDK <$env:WindowsSDKVersion>、commit <git rev-parse HEAD>
- W1〜W8: | ID | 結果 | 所要時間 | の表（全件）
- 参考: e2e.mjs <結果>、numeric_casts.mjs <結果>
- 修正: <ファイルと関数、手順 8 の分類、理由>（なければ「なし」）
```

- 確認: W1〜W8 の 8 行がすべて埋まっている。

### 手順 12: 文書と状態を更新する

- 変更: 「ドキュメント」の一覧。
- 内容: D10 の条件を満たしたときだけ状態を `done` にし、`git mv _features/G10-windows.md _features/_completed/G10-windows.md` で移して
  リンクを直す。満たさないときは `blocked` のまま、`_features/README.md` の G10 の説明を残りの作業に書き換える。
- 確認: `node scripts/check-docs.mjs _docs README.md` が成功する。`done` にした場合、
  `grep -rn "G10-windows.md" _docs _features README.md docs` のリンクが `_completed/` を指し、
  `grep -rn "実行ゲート.*未確認\|実行結果は未確認\|実行検証は未完了" README.md docs _docs _features/README.md` が何も出さない。

### 手順 13: 最終確認

- 変更: なし。
- 内容: `git status --short` と `git diff --stat` で、変更が「段ごとの変更」の行だけであることを確かめる。
- 確認: 予定外のファイルがない。`cargo fmt --all -- --check` が成功する。

## テスト計画

### Rust テスト

- 既存の `cargo test --locked` 全体（W3）。Windows だけで走るもの: `tests/windows.rs` の
  `windows_outputs_protect_hardlinks_and_replace_existing_files`（hard link の `E2003`、既存出力の置換 2 回、source が `42` のまま）と
  `tests/lsp.rs` の `windows_file_uris_round_trip_drive_letters_and_canonical_paths`。
- `emits_windows_exports_without_changing_default_or_wasm` は全 OS で走る。`cross_links_with_microsoft_sdk_when_requested` は
  `TSUZURI_WINDOWS_SDK` がなければ即 return する（ゲートでは設定しない）。
- 新しいテストは手順 8 で Windows 分岐を直したときの回帰テストだけ。`#[cfg(windows)]` を付けて `tests/windows.rs` に足す。

### E2E

- `tests/windows.mjs`（W5）: O0/O3 それぞれで console、再 build、clang 不在の `E2002` と旧出力の保持、`examples/tasks`・`examples/control`、
  scheduler の 4 topology、failure injection の 3 mode、`dllexport` の IR、C host からの
  `tz_parallel_results_lowest() == 2`・`tz_parallel_results_owned(2048) == 6`・`tz_parallel_results_nested() == 2016`・`tz_wide_numbers()`、
  runtime 入り object の `E2002`。O に依らず hard link の `E2003`、`tz_answer` の object と C host、wasm32 の import なしと `42n`。
  期待値は C の assert と POSIX の suite と同じ定数で、Windows の実行結果から作らない。
- `tests/cache.mjs`・`tests/examples.mjs`・`tests/wasm_threads.mjs`（W6〜W8）: 既存の assert のまま。

### 既存テストへの影響

なし。D6 の harness 修正は期待値を変えない。

### 性能

対象外。所要時間は記録するが、合否の条件にしない。

## ドキュメント

ゲート合格（D10）後に、「未確認」の記述を実行した環境と範囲（x64、W1〜W8）へ書き換える。ARM64・PDB・MinGW の未対応は残す。

- `README.md` の「ビルド」節: 「Windows runnerでの実行結果は未確認です」の文。
- `_docs/tools/build-and-cache.md` の「プラットフォーム」節: `x86_64 Windows MSVC` の行と、「Windows での全実行検証が完了した保証とはしません」の文。
- `_docs/get-started.md` の Windows の段落、`_docs/README.md` の「Windows 上の実行検証は未完了です」の文。
- `docs/architecture.md` の `**Windows MSVC:**` 段落の「Windows runnerでの実行ゲートは未確認です」の文。
- `_docs/feature-status.md` の G10 行と、未対応の一覧の「Windows の実機実行検証」。
- `_features/README.md` の配置の説明（直下の `blocked`）、「P3 実装・総合検証」節の G10 の記述、一覧の G10 行。
- このチケット: 手順 11 の記録と、状態の `done`。

## 受け入れ条件

- [ ] x64 Windows（実機か runner）で W1〜W8 がすべて成功し、手順 11 の形式で記録されている。
- [ ] `.github/workflows/windows.yml` が設計どおり復元され、GitHub で緑の run がある（URL を記録）。
- [ ] `Task.parallel` が Windows native で動き、failure injection の 3 mode が明示診断で abort する（W5）。
- [ ] Windows で source の hard link への出力が `E2003` になり、既存出力の置換が成功する（W3・W5）。
- [ ] 公開 wrapper が `dllexport` を持つ（W3・W5）。
- [ ] コードを変えた場合、macOS か Linux の `cargo test --locked` と Windows target の clippy が成功する（native/WASM の挙動は不変）。
- [ ] 「ドキュメント」の記述と状態が更新され、`node scripts/check-docs.mjs _docs README.md` が成功する。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- GitHub の Windows runner は `core.autocrlf=true` が既定で、checkout が fixture と `src/runtime/*.ll` を CRLF にする。workflow の最初の step で
  無効にする（D7）。実機でも clone 時に `-c core.autocrlf=false` を付ける。
- Git Bash では Git 付属の `/usr/bin/link` が MSVC の `link.exe` より先に見つかり、`link: extra operand` で失敗する。PowerShell を使う。
- runner と Visual Studio には別版の clang（`C:\Program Files\LLVM`、VS の Clang tools）がある。`TSUZURI_CLANG` を絶対パスで設定し、
  `Show tools` step と手順 3 の確認で 21.1.8 であることを見る。
- `fs::canonicalize` は `\\?\` 付きのパスを返し、その隣の引用 include を Clang が解決できない。`task_runtime_source()` が回避している。
  Windows 分岐を直すときに canonical パスを clang へ渡さない。
- Defender や実行中の exe が出力をロックすると rename が `os error 5`・`32` で失敗する。コードに retry を足して隠さない（D2）。
- `tests/windows.mjs` は `--target=x86_64-pc-windows-msvc` を固定で渡す。ARM64 native の compiler とは組み合わせられない（D5）。
- `tests/tasks.mjs`・`tests/features.mjs` などは `-pthread` を無条件に渡す。Windows での失敗は G10 の不具合ではないので実行しない。
- LLVM installer の `/D=` は最後の引数で、引用符を付けない。
- Rust が古いと `if ... && let` で compile が失敗する。`rustup update stable` で直す。
- 対話 console（cmd.exe）の文字化けは不具合ではない。W5 は pipe で bytes を比べる。コードページを変えるコードは足さない。

## 対象外

- ARM64 Windows のゲート（D5）、MinGW ABI、PDB / CodeView、DLL import library の自動生成、`clang-cl` の引数形式。
- 複数の生成 object に含まれる task runtime の COFF での統合（D8）。
- Windows GUI / console subsystem の選択。
- POSIX 専用 harness（`tests/tasks.mjs` など）の Windows 対応。
- VSIX と同梱 toolchain（`vscode.yml`、G14）。

## 決定事項

### D1: ゲートの環境

- 決定: x64 Windows、LLVM 21.1.8（`LLVM-21.1.8-win64.exe`、SHA-256 `7a5386c26497db1691f320121e5b113364dd0274b98e55f15f4dbc00c0450113`）、
  Visual Studio Build Tools 2022 の VCTools と Windows SDK、Rust stable の MSVC toolchain（1.88 以上）、Node.js 24。
- 理由: `vscode.yml` と同じ版で、runner image の LLVM の版の揺れを除ける。Node 20 は重い BigInt suite で V8 が異常終了した記録がある。
- 状態: 既定案（実装者はこの案に従う）

### D2: 既存出力の置換

- 決定: `publish_outputs` の `fs::rename` による置換だけを使う。runner で置換が失敗したら停止し、古い出力を消してから rename する
  fallback は入れない。
- 理由: fallback は atomicity と、失敗時に旧出力を残す保証（W5 が検査する）を壊す。safe Rust だけで別の置換 API は呼べず、依存追加か
  `unsafe` の許可は人間の判断になる。
- 状態: 既定案（実装者はこの案に従う）

### D3: workflow の復元

- 決定: `.github/workflows/windows.yml` を設計の YAML で復元する。workflow 名 `Windows MSVC`、job id `windows`、`windows-2022`、
  trigger は `push`・`pull_request`・`workflow_dispatch`。実装者は push しない。
- 理由: 専用 CI は GUIDE D-29 で承認済みで、削除は無関係な題名のコミットに含まれ理由の記録がない。G15 が「G10 の job」を前提にする。
  push は共有環境への操作なので人間が行う。
- 状態: 既定案（実装者はこの案に従う）

### D4: ゲートに含める suite

- 決定: W1〜W8（削除版 workflow と同じ集合）。`tests/e2e.mjs`・`tests/numeric_casts.mjs` は参考記録。POSIX 専用 harness は実行しない。
- 理由: W5〜W8 は Windows 対応が確認済みの harness である。参考 suite は win32 分岐を持つが Windows で一度も実行されておらず、
  失敗が harness か本体かを切り分ける費用がゲートの目的に見合わない。
- 状態: 既定案（実装者はこの案に従う）

### D5: ARM64 Windows

- 決定: ゲートに含めない。ARM64 実機しかない場合は、x64 の Rust toolchain・Node・LLVM（win64）と Build Tools の x64 tools で x64 として
  動かす（未検証の経路。失敗したら GitHub runner を使う）。ARM64 native の結果は参考として記録するだけで、`tests/windows.mjs` は変えない。
- 理由: Phase 1 の対象は x86_64 で、ARM64 は対象外と決めてある。target の追加は G15 の範囲である。
- 状態: 既定案（実装者はこの案に従う）

### D6: 失敗時に許す修正の範囲

- 決定: 期待値を変えない harness の修正（パス区切り、`.exe`、Windows で渡せない flag）と、`cfg(windows)`・`cfg!(windows)` の分岐、
  `src/runtime/task-windows.h`、`windows_abi` の中だけの修正を許す。それ以外は停止条件に従う。
- 理由: macOS / Linux / WASM の挙動と生成 IR を変えずにゲートを閉じるため。
- 状態: 既定案（実装者はこの案に従う）

### D7: 改行

- 決定: CI と clone で `core.autocrlf=false` にする。`.gitattributes` は G10 では足さない（HEAD にはない）。
- 理由: 改行変換は fixture と runtime IR の bytes を変える。`.gitattributes` は全 host の checkout に効く repository 全体の変更である。
- 状態: 既定案（実装者はこの案に従う）

### D8: COFF の task runtime

- 決定: runtime を含む COFF object は `E2002` のまま。exe と、LLVM IR と runtime を一度だけ link する経路を提供する。複数 object の
  weak 統合は対象外。
- 理由: COFF の weak / `linkonce_odr` は ELF / Mach-O と同じでなく、検証なしに POSIX と同等とは言えない（GUIDE D-29）。
- 状態: 既定案（実装者はこの案に従う）

### D9: hard link の同一性

- 決定: Windows 限定の `same-file` 1.0.6 の `is_same_file` を使う（実装済み）。
- 理由: stable Rust の `MetadataExt::file_index`・`volume_serial_number` は不安定 API で、`unsafe_code = "forbid"` と hard link 保護を
  両立できる safe API がこれだった。2026-09-28 に利用者の判断委任で採用した（GUIDE D-29）。
- 状態: 既定案（実装者はこの案に従う）

### D10: 完了の判定

- 決定: `done` は、x64 で W1〜W8 がすべて成功し、復元した workflow に GitHub の緑の run があるときだけ。前者だけなら `blocked` のまま、
  理由を「workflow の GitHub 実行待ち」に書き換える。cross の型検査・COFF/PE link は数えない。
- 理由: G15 が Windows の tier を G10 の job に結び付けている。実行していない検証を完了と混同しない（GUIDE D-29）。
- 状態: 既定案（実装者はこの案に従う）
