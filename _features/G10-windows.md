# G10: Windows ネイティブ対応
| 項目 | 内容 |
|---|---|
| ID | G10 |
| 優先度 | P3 |
| 規模 | M |
| 依存 | – |
| 後続 | G08（Windows debug 形式の拡張）、F01（ワーカープールの Windows 実装） |
| 状態 | todo |
| 主な影響ファイル | `src/driver.rs`, `src/llvm.rs`, `src/runtime/task.c`, `tests/task_runtime.c`, `tests/*.mjs`, `.github/workflows/*`, `README.md`, `docs/architecture.md` |

## 目的

Windows の GitHub Actions runner と開発者環境で、既存の native executable / object / header 出力を安全に生成・実行できるようにする。Phase 1 は x86_64 Windows（MSVC ABI）を対象に、既存言語機能と `Task.parallel` を含む native tests を通すことを目標にする。

## 現状

- README は macOS / Linux を CI 対象とし、「Windows のネイティブ・ツールチェーンは未検証」と書いている。
- `src/driver.rs` は `BuildOptions::output_path` で Windows の `.exe` / `.obj` 拡張子を選ぶが、実際の toolchain flags は POSIX 前提が残る。
- `driver::build` は native で `clang -fPIC`、exe で非 Windows のみ `-lm`、task runtime で non-unix かつ object/exe の場合 `E2002` を返す。
- `protect_source` の hard-link 同一性検査は Unix の `MetadataExt::{dev, ino}` のみ。`#[cfg(not(unix))] same_file` は常に false。
- `src/runtime/task.c` は `<pthread.h>`, `<unistd.h>`, `sysconf(_SC_NPROCESSORS_ONLN)`, `pthread_create`, `pthread_join` を使う。
- `src/llvm.rs` の `export_wrapper` は `define ... @tz_name` を出すが、Windows DLL / object consumer 向けの `dllexport` 属性はない。
- `tests/*.mjs` は一部で `process.platform === "win32"` により `.exe` や `-lm` 省略に対応しているが、`tests/tasks.mjs` と `tests/task_runtime.c` は pthread 前提である。
- `Cargo.toml` は `unsafe_code = "forbid"`。Windows file identity のために Rust 側で Win32 API を直接 unsafe 呼び出しする設計は避ける。

## 仕様

### サポート範囲（Phase 1）

- Host: GitHub Actions `windows-latest`。
- Target: `x86_64-pc-windows-msvc`。
- Toolchain:
  - `clang` または `clang-cl`。既定は既存どおり `TSUZURI_CLANG` / `clang`。
  - link は clang driver 経由で `lld-link` または Visual Studio Build Tools の linker を使う。
- 対応 emit:
  - native exe (`--emit exe`)
  - native object (`--emit object`, `.obj`)
  - LLVM IR (`--emit llvm`)
  - C header (`--emit header`)
- `--target wasm32` は既存どおり OS 非依存。G10 では変更しない。
- `Task.parallel` は Windows native で Win32 threads backend を使う。WASM は既存の逐次 backend。

### CLI / output

- `BuildOptions::output_path` の `.exe` / `.obj` は維持。
- Windows native clang invocation は `--target=x86_64-pc-windows-msvc` を明示する。
- `-fPIC` は Windows では渡さない。
- `-pthread` は Windows では渡さない。
- `-lm` は Windows では渡さない。
- `--cpu native` は Windows x86_64 でも `-march=native` を使える clang の場合だけ許可する。未対応 toolchain では既存 `E2000` と同様に明示エラー。

### 公開 ABI / export

- Windows object / DLL consumer が `tz_<name>` を見つけられるよう、Windows native 出力では exported wrapper に LLVM `dllexport` を付ける。

例:

```llvm
define dllexport i64 @tz_answer() nounwind {
...
}
```

- 内部関数 `@tz.fn.Module.name` は従来どおり internal。
- C header の `tz_name` 宣言は Phase 1 では `__declspec(dllimport)` を付けない。object を直接 link する利用者を主対象にし、DLL import header は後続で検討する。

### task runtime

`src/runtime/task.c` は `_WIN32` で分岐する。

- CPU 数: `GetActiveProcessorCount(ALL_PROCESSOR_GROUPS)`。0 の場合は 1。
- thread: `CreateThread` / `WaitForSingleObject` / `CloseHandle`。
- worker 上限と nested fallback は POSIX 版と同じ:
  - 最大 `TZ_TASK_MAX_THREADS = 32`
  - 呼び出し元も work を実行
  - 追加 worker 枠がなければ呼び出し元で逐次
  - join 後に worker count を返す
- 失敗時:
  - `CreateThread` failure、`WaitForSingleObject` failure、`CloseHandle` failure は stderr に `Tsuzuri task runtime: <operation> failed (<code>)` を出して `abort()`。
  - 成功形の結果を返さない。

### output protection

- Windows でも source と output が同一 file / hard link の場合は `E2003`。
- `std::os::windows::fs::MetadataExt` を使い、volume serial + file index で比較する。unsafe は使わない。
- symlink / directory / special file 拒否は既存 `symlink_metadata` 分岐を維持。
- 既存出力の atomic replacement は `std::fs::rename` の Windows 挙動を実測テストする。既存 file を置換できない toolchain / std であれば、古い出力を削除してから rename する fallback は採用しない（atomicity と output preservation を破るため）。その場合は未決事項に従い human decision を要求する。

### console / UTF-8

- Phase 1 では compiler diagnostics / `tsuzuri run` の stderr/stdout は UTF-8 bytes を出す。Windows console code page の変更はしない。
- 文字化け対策として README に Windows Terminal / UTF-8 code page の注意を書く。
- 言語の string console output の semantics は変えない。

## 設計

### driver

`Target` は既存 `Native` / `Wasm32` のまま。OS ABI 判定 helper を追加する。

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeAbi {
    Posix,
    WindowsMsvc,
}

fn native_abi() -> NativeAbi;
fn clang_target_args(abi: NativeAbi) -> &'static [&'static str];
```

Windows native build:

- IR → object/exe:
  - `clang -x ir -Wno-override-module -O<n> --target=x86_64-pc-windows-msvc`
  - object: `-c module.ll -o artifact.obj`
  - exe: `module.ll [task.obj] -o artifact.exe`
- task runtime C:
  - `clang -std=c11 --target=x86_64-pc-windows-msvc -c task.c -o task.obj`
  - no `-pthread`
- object with task runtime:
  - POSIX は既存 `clang -r -nostdlib`。
  - Windows は `lld-link /lib` では relocatable object merge の意味が違うため、Phase 1 は `--emit object` with task runtime を `E2002` で拒否するか、COFF `lld-link -r` 相当が確認できる場合だけ対応する。既定案はまず拒否し、exe は対応する。

### llvm

`llvm::emit_target` は target ABI を知らないため、export style を渡す API に分ける。

```rust
pub enum ExportStyle {
    Default,
    Dllexport,
}

pub fn emit_target_with_options(
    module: &CheckedModule,
    entry: Entry,
    wasm: bool,
    export_style: ExportStyle,
) -> Result<String, Diagnostic>;
```

`export_wrapper(function, module, export_style)` で `define dllexport ...` を出す。

G08 が先に `emit_target_with_options` を追加している場合は options struct に統合する。

### Windows hard-link protection

実装案:

```rust
#[cfg(windows)]
fn same_file(input: &Path, output: &fs::Metadata) -> Result<bool, Diagnostic> {
    use std::os::windows::fs::MetadataExt;
    let input_metadata =
        fs::metadata(input).map_err(|error| io_error("inspect source", input, error))?;
    Ok(input_metadata.volume_serial_number() == output.volume_serial_number()
        && input_metadata.file_index() == output.file_index()
        && input_metadata.volume_serial_number().is_some()
        && output.volume_serial_number().is_some()
        && input_metadata.file_index().is_some()
        && output.file_index().is_some())
}
```

Rust 1.85 の `MetadataExt` に `file_index()` がない場合は `file_index_high()` / `file_index_low()` を組み合わせる。どちらも safe API のみ使う。

### task.c

構造は既存 POSIX 版を保つ。

```c
#if defined(_WIN32)
#include <windows.h>
static unsigned tz_task_parallelism(void) { ... GetActiveProcessorCount(...); }
static DWORD WINAPI tz_task_work_win32(LPVOID pointer) { tz_task_work(pointer); return 0; }
#else
#include <pthread.h>
#include <unistd.h>
#endif
```

`tz_task_work` の core loop は OS 非依存 helper にし、POSIX / Win32 wrapper から呼ぶ。

関数 export / weak:

- POSIX は既存 `__attribute__((weak, visibility("hidden")))`。
- Windows はまず通常 external symbol とし、generated exe では duplicate が起きないように driver が task runtime を一度だけ link する。
- 複数 generated object を同時に link したときの runtime 重複は Phase 1 では対象外にし、README に object 利用時は task runtime object を 1 つだけ link する注意を書く。既存 POSIX の weak 統合と同等にするのは Phase 2。

### テスト harness

- `tests/task_runtime.c` は `_WIN32` で Win32 synchronization に分ける。
  - `pthread_mutex_t` / `pthread_cond_t` の代わりに `CRITICAL_SECTION` + `CONDITION_VARIABLE`。
  - host_group は `CreateThread`。
  - failure injection は `#define CreateThread test_CreateThread` などで差し替える。
- `tests/tasks.mjs`:
  - Windows では `-pthread` / `-lm` を外す。
  - `task_runtime.c` を Windows でも compile / run。
- 既存で win32 分岐済みの `tests/e2e.mjs`, `tests/examples.mjs`, `tests/numeric_casts.mjs` はその方針を他の tests に展開する。

### CI

`.github/workflows` に Windows job を追加する。

最小:

```yaml
runs-on: windows-latest
steps:
  - uses: actions/checkout@v4
  - uses: dtolnay/rust-toolchain@stable
  - uses: actions/setup-node@v4
    with: { node-version: 20 }
  - run: cargo test --locked
  - run: cargo build --release --locked
  - run: node tests/e2e.mjs target/release/tsuzuri.exe
  - run: node tests/tasks.mjs target/release/tsuzuri.exe
```

LLVM/Clang の PATH は runner image の Visual Studio / LLVM を確認し、不足なら setup step を追加する。外部 tool 不足は skip せず fail させる。

## 実装手順

1. **driver flags**
   - `native_abi` / `clang_target_args` を追加し、Windows で `--target=x86_64-pc-windows-msvc`。
   - `-fPIC`, `-pthread`, `-lm` を ABI で分岐。
   - 確認: Windows unit test で command line を組み立てる helper を検査できるよう、Command 構築を小関数へ分割。
2. **dllexport**
   - `llvm::ExportStyle` を追加し、Windows native で `export_wrapper` に `dllexport`。
   - 確認: `--emit llvm` 相当 unit test で `define dllexport ... @tz_`。
3. **Windows same_file**
   - `#[cfg(windows)] same_file` を実装。
   - 確認: Windows CI で source hard link を作り `E2003`。
4. **task.c Win32 backend**
   - `_WIN32` 分岐を追加。
   - POSIX 版の behavior を変えない。
   - 確認: macOS/Linux で既存 `tests/task_runtime.c` が通る。
5. **Windows task tests**
   - `tests/task_runtime.c` に Win32 harness。
   - `tests/tasks.mjs` から Windows compile flags を分岐。
   - 確認: Windows CI で failure injection 含め通す。
6. **Node harness 横展開**
   - `-lm`, `.exe`, dynamic library skip/extension を全 tests で統一。
   - 確認: macOS/Linux の既存 tests が変わらず通る。
7. **CI / docs**
   - Windows job 追加。
   - README の build section 更新。

## テスト計画

- Rust:
  - `BuildOptions::output_path` が `.exe` / `.obj`。
  - Windows cfg test で `same_file` hard link 拒否（CI 上）。
  - `llvm` export style が `dllexport` を出す。
- C:
  - `tests/task_runtime.c` を Windows / POSIX で compile and run。
  - create / join failure diagnostics を検査。
- Node:
  - `tests/e2e.mjs`, `tests/primitives.mjs`, `tests/tasks.mjs`, `tests/computations.mjs`, `tests/control.mjs`, `tests/numeric_casts.mjs`, `tests/examples.mjs` のうち Windows で GUI / dynamic library を除いた native 部分を通す。
  - wasm tests は Windows でも同じ。
- CI:
  - `cargo fmt --all -- --check`
  - `cargo clippy --all-targets -- -D warnings`
  - `cargo test --locked`
  - `cargo build --release --locked`
  - representative Node E2E。

## ドキュメント

- `README.md`:
  - Windows prerequisites（LLVM/Clang, Visual Studio Build Tools / lld-link, Node）。
  - `.exe` / `.obj` examples。
  - `Task.parallel` が Windows native で Win32 threads を使うこと。
- `docs/architecture.md`:
  - host/toolchain 表に Windows MSVC ABI。
  - output protection の Windows file identity。
  - task runtime backend の POSIX / Win32 分岐。

## 受け入れ条件

- [ ] Windows CI で `cargo test --locked` が通る。
- [ ] Windows CI で native exe を build/run できる。
- [ ] `Task.parallel` が Windows native で動き、failure は明示診断で abort する。
- [ ] Windows で source hard link output を `E2003` で拒否する。
- [ ] Windows native exported wrapper が `dllexport` を持つ。
- [ ] macOS/Linux の既存 native/WASM behavior が変わらない。

## 落とし穴

- Windows で `-pthread`, `-lm`, `-fPIC` を渡すと toolchain により失敗する。
- `std::fs::rename` の既存 destination 挙動を確認せずに古い出力を削除すると、出力保護の不変条件を壊す。
- Rust 側で Win32 API を直接呼ぶと `unsafe_code = "forbid"` に抵触する。file identity は safe `MetadataExt` を使う。
- COFF object の weak symbol は ELF/Mach-O と同じではない。task runtime の重複統合を POSIX と同じと仮定しない。
- Windows console の文字表示は shell 設定に依存する。言語の UTF-8 bytes semantics を変えない。

## 対象外

- ARM64 Windows。
- MinGW ABI。
- PDB / CodeView debug info。
- DLL import library 自動生成。
- POSIX と同等の複数 generated object 内 task runtime weak 統合。
- Windows GUI / console subsystem selection。

## 未決事項

- **atomic replacement**: 既定案は `std::fs::rename` で既存 file 置換が安全にできるか Windows CI で確認する。できない場合、safe Rust だけで `MoveFileEx(MOVEFILE_REPLACE_EXISTING)` 相当を呼ぶ手段がないため、既存 output がある Windows build を error にするか、依存追加 / unsafe 許可を人間が判断する。
- **COFF task runtime symbol**: Phase 1 は exe を優先し、object の task runtime 統合は保守的に制限する。複数 object link を公式サポートするなら COFF weak / linkonce_odr の検証が必要。
