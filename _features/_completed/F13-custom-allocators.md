# F13: アロケーターの差し替え・確保統計・freestanding 出力

| 項目 | 内容 |
| --- | --- |
| ID | F13 |
| 優先度 | P2 |
| 規模 | L |
| 依存 | (F11), (E12), (E14) |
| 後続 | G15 Phase 3 |
| 状態 | done（Phase 1・2・3） |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | D9 は、2026-10-06 に利用者から「C08、A14、A16、F13、F08 の実装をすべて完遂して。…複数フェーズある場合には、すべてのフェーズを完了させること」と依頼され、承認として扱った（GUIDE D-30・D-39）。Phase 2・3 も同じ依頼で設計して実装した |
| 改善する劣位 | C/C++ 比: allocator を細かく制御できない（[なぜ Tsuzuri か](../../_docs/learn/why-tsuzuri.md#cc-に対する劣位点)）／追加: libc のない環境へ出力できない |
| 手本にする既存実装 | CLI と組み合わせ検査: `src/main.rs` の `--wasm-feature` 分岐と `src/driver.rs` の `BuildOptions::validate` の `wasm_threads` 検査。heap runtime の切り替え: `src/llvm.rs` の `emit_program` 末尾で `instrumentation.wasm_threads` により `heap-wasm.ll`／`heap-wasm-threads.ll` を選ぶ箇所。C ホストの確保追跡: `tests/host_abi.rs` の `host_buffers_and_records_roundtrip_on_native_and_wasm` と `tests/features.mjs` の `run`（`tracked_alloc`） |
| 主な影響ファイル | `src/runtime/heap-host.ll`（新規）, `.gitignore`, `src/llvm.rs`, `src/llvm_abi.rs`, `src/driver.rs`, `src/main.rs`, `tests/allocator.rs`（新規）, `tests/allocator.mjs`（新規）, `tests/allocator_host.c`（新規）, `tests/features.mjs`, `EmitOptions` を構築する既存テスト（`tests/cpu_dispatch.rs`, `tests/debug_info.rs`, `tests/debug_output.rs`, `tests/trap_locations.rs`, `tests/windows.rs`）, `README.md`, `docs/language.md`, `docs/architecture.md`, `_docs/tools/command-line.md`, `_docs/guides/native-interop.md`, `_docs/feature-status.md`, `_features/README.md` |

## 目的

C/C++ のカスタム allocator・メモリプール、Rust の `#[global_allocator]`・`no_std` に相当する制御を提供する。
組み込み機器・ゲーム・リアルタイム処理のホストが確保方式を決められるようにし、libc のない環境への出力の土台を作る。

Phase 1 は「native の object／LLVM IR 出力で、Tsuzuri が管理する全ての確保をホストがリンク時に与える 3 関数へ送る」ことだけを行う。
実装者は Phase 1 だけを実装する。Phase 2（確保統計・WASM の allocator）と Phase 3（freestanding）は人間が D9 を承認して求めた場合だけ着手する。

## 着手条件と停止条件

### 着手条件

- 依存はすべて任意（括弧付き）で、Phase 1 の開始条件にしない。F11 は Phase 2 の WASM allocator、E12 は `--emit exe` との組み合わせ（D7）、
  E14 は失敗の報告（D5）に関係するだけである。状態は `grep -n "F11\|E12\|E14\|F13" _features/README.md` の状態欄で確かめる。
- GUIDE §2.3 の基準コマンドが成功していること。
- 手順 1 のベースライン（既定の IR と object の記号）を保存していること。既定の出力が 1 byte でも変われば停止条件に当たる。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- Tsuzuri が所有する領域を `@tz.alloc`／`@tz.free`／`@tz.realloc` 以外で確保・解放する経路が見つかった（生成 IR の直接の `@malloc`、
  ランタイム C の `malloc`／`free`、`tsuzuri_alloc` を通らないホスト由来の所有バッファなど）。HEAD では見つかっていない（「現状」）。
- `--allocator` を指定しない出力（IR・header・object の記号・WASM）が変わる。`EmitOptions` への field 追加に伴う既存テストのコンパイル修正だけは除く。
- `--trap-info` 付きの出力で、ホスト allocator の失敗が `AllocationFailure` 以外の理由として報告される。
- `@tz.free` の引数を変えたくなった（サイズ付きの解放は PM05 Phase 1 の範囲。D3）。
- native target でポインターが 64 bit でない構成に出会った（D2 の `uint64_t`／`i64` を前提にしている）。
- macOS の `clang -r -nostdlib` 後の object で `tsuzuri_host_alloc` などが未定義の外部記号（`U`）として残らない。
- `unsafe`、新しい crate、既定の WASM import が必要になった。
- 既存テストの期待値（IR、診断コード・メッセージ、`live == 0` の判定）を変える必要がある。

## 現状（HEAD `f8dc655` で確認）

- `src/runtime/heap-native.ll` は `define internal ptr @tz.alloc(i64 %size)`、`define internal void @tz.free(ptr %pointer)`、
  `define internal ptr @tz.realloc(ptr %old, i64 %old_size, i64 %new_size)` を定義し、libc の `@malloc`／`@free`／`@realloc` を呼ぶ。
  null の結果は `@llvm.trap`。`@tz.realloc` は `new_size == 0` なら解放して null を返す。`%old_size` は使っていない。
- `src/runtime/heap-wasm.ll` は 16 bytes のヘッダー付き free-list（`(size + 31) & -16`、上限 16 MiB）。threads では `emit_program` が
  `heap-wasm.ll` の定義を `@tz.heap.alloc.unlocked` などへ置換し、`src/runtime/heap-wasm-threads.ll` の lock wrapper から呼ぶ。
  `tsuzuri_thread_heap_live_bytes` を公開する。
- `src/llvm.rs` の `emit_program` は、生成 IR が `@tz.string.`・`@tz.utf8string.`・`@tz.free`・`@tz.alloc`・`@tz.realloc` を含むときだけ
  `string.ll`・`utf8string.ll` と heap runtime を末尾へ連結する。`uses_host_abi(module)` または `@tsuzuri_io_` を含むときは
  `host_abi::allocator()`（`src/llvm_abi.rs` の `allocator`）が weak な `@tsuzuri_alloc`（0 は 1 byte、負はトラップ）と `@tsuzuri_free` を出す。
- `@malloc` などの libc 確保関数を参照するのは `src/runtime/heap-native.ll` だけである（`src/` の grep）。`src/runtime/*.c` に
  `malloc(`・`free(`・`realloc(`・`calloc(` はない。`io.c` の行バッファは `tsuzuri_alloc`／`tsuzuri_free` を使う。
- トラップ理由: `src/llvm_traps.rs` の `runtime_kind` は、`@tz.alloc`・`@tz.realloc` の中の `@llvm.trap` を
  `TrapKind::AllocationFailure`（`src/trap.rs`、表示 `allocation failed`）に分類する。関数名で判定するので、トラップは両関数の本体に置く必要がある。
- `storage_layout`（`src/llvm.rs`）の align の最大は 16（`Type::Simd` と 128 bit 整数）。native の malloc は 16 byte 境界を返す前提で動いている。
  `docs/architecture.md` は「空配列とサイズ 0 の要素でも確保サイズを 1 バイト以上にし、`malloc(0)` の挙動には依存しません」と定める。
- CLI: `src/driver.rs` の `BuildOptions`（`target`, `emit`, `optimization`, `cpu`, `debug_output`, `trap_info`, `debug_info`, `wasm_simd`,
  `wasm_threads`, `cache`）と `BuildOptions::validate`（不正な組み合わせは `E2000`）。`--allocator` はない。
  `src/cache.rs` の cache key は `format!("{options:?}")` を含むので、`BuildOptions` の field は自動で key に入る。
- `llvm::EmitOptions` は `entry`, `wasm`, `debug_output` の 3 field。構築箇所は `src/llvm.rs` の `emit_target`、`src/driver.rs` の build、
  既存テスト 5 ファイル（`tests/cpu_dispatch.rs`, `tests/debug_info.rs`, `tests/debug_output.rs`, `tests/trap_locations.rs`, `tests/windows.rs`）。
- native の `--emit object` は runtime C（task・CPU・IO）を必要時に `clang -r -nostdlib`（macOS は `-Wl,-keep_private_externs`）で同梱する。
  header（`llvm::header`）は `tsuzuri_alloc`／`tsuzuri_free` の prototype を含み、`extern "C"` の区間を `#ifdef __cplusplus` で閉じて終わる。
- 確保追跡: `tests/features.mjs` の `run`、`tests/primitives.mjs`、`tests/control.mjs`、`tests/computations.mjs`、`tests/strings.mjs`、
  `tests/tasks.mjs`、`tests/io.mjs`、`tests/host_imports.mjs`、`tests/gpu.mjs`、`tests/e2e.mjs`、`tests/math.mjs`、`tests/host_abi.rs` は、
  既定の IR の `@malloc`／`@free`／`@realloc` を文字列置換して C の追跡関数へ差し替える。既定の IR を変えない限りこれらはそのまま動く。
- C10 は「F13 の allocator は Arena のバッファ（`@tz.alloc`）にもそのまま適用される」と定める。PM05 Phase 1 は `@tz.free(ptr, size)` の
  サイズ付き解放を導入し、`tsuzuri_alloc`／`tsuzuri_free` の契約を変えない。

### 再現（検証済み）

`tests/fixtures/host_abi` を `/tmp/tz-work-F13/abi` へ複写して確認した。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
target/release/tsuzuri build /tmp/tz-work-F13/abi --emit object -o /tmp/tz-work-F13/abi.o
nm /tmp/tz-work-F13/abi.o | grep -E "tsuzuri_(alloc|free)|malloc|free"
# U _free / U _malloc / T _tsuzuri_alloc / T _tsuzuri_free
target/release/tsuzuri build /tmp/tz-work-F13/abi --allocator host --emit object -o /tmp/tz-work-F13/x.o
# <command line>:1:1: error[E2000]: unknown option '--allocator'; use --help（終了コード 2）
```

## 仕様

### 前提とする他チケットのインターフェース

Phase 1 は他チケットのインターフェースを前提にしない。F13 が他チケットへ提供するものは次のとおり。

- PM05: `llvm::Allocator`（新規）と `--allocator` の値の追加方法、heap runtime の契約（`define internal` の `@tz.alloc(i64)`・`@tz.free(ptr)`・
  `@tz.realloc(ptr, i64, i64)` を 1 ファイルで定義する）。PM05 Phase 1 が `@tz.free` にサイズを足すときは `heap-host.ll` も同時に更新し、
  ホストへ渡すサイズ（D3）は変えない。PM05 の allocator を既定にするか値を足すかは PM05 が決める。
- C10: Arena のバッファは `@tz.alloc` なので、`--allocator host` でそのままホストへ送られる。C10 側の変更は不要。
- G15 Phase 3: Phase 3（freestanding）の前提。Phase 1 の `tsuzuri_host_*` の 3 関数をそのまま使う。

### CLI

```text
tsuzuri build <input> [--allocator system|host] [その他の build オプション]
```

- `system` は既定で、target 標準の allocator を使う（native は `heap-native.ll` の libc malloc、wasm32 は `heap-wasm.ll`／threads 版）。
  指定しない場合と IR・header・object・WASM が byte 単位で一致する。
- `host` は native target の `--emit object`・`--emit llvm`・`--emit header` だけで使える。生成コードと runtime C の全ての確保・解放を、
  ホストがリンク時に定義する 3 関数（次節）へ送る。header には 3 関数の prototype が加わる。
- `--allocator` は build だけで使える。`run`・`test`・`check` などは既定の allocator を使う。一度だけ指定できる。

### ホストが定義する関数（C ABI）

新 API（実装後に有効。未検証）。header（`--emit header --allocator host`）の `extern "C"` 区間に次の prototype が入る。

```c
void *tsuzuri_host_alloc(uint64_t size, uint64_t align);
void tsuzuri_host_free(void *ptr, uint64_t size, uint64_t align);
void *tsuzuri_host_realloc(void *ptr, uint64_t old_size, uint64_t new_size, uint64_t align);
```

契約（`docs/language.md` と `_docs/guides/native-interop.md` へ同じ内容を書く）:

- `align` は Phase 1 では常に 16。戻り値は `align` の倍数でなければならない。違反は検出してトラップする（`AllocationFailure`）。
- `size`・`new_size` は常に 16 以上で、0 は渡らない。16 の倍数とは限らない。Tsuzuri の要求サイズに 16 bytes のヘッダーを足した値である（D3）。
- `tsuzuri_host_free` の `ptr` は null でない、未解放の、`tsuzuri_host_alloc`／`tsuzuri_host_realloc` が返した値。`size`・`align` はその確保
  （realloc 後は `new_size`）と同じ値。各ブロックで一度だけ呼ばれる。確保と別のスレッドから呼ばれることがある。
- `tsuzuri_host_realloc` の `ptr` は null でなく、`old_size` は現在のサイズと等しい。先頭 `min(old_size, new_size)` bytes を保ち、成功後は旧ブロックを
  使わない。Tsuzuri は null の `ptr` や 0 の `new_size` では呼ばない（`@tz.alloc`／`@tz.free` へ振り分ける）。
- 失敗は null を返す。Tsuzuri はその場でトラップし（`AllocationFailure`）、再試行や errno はない。longjmp・C++ 例外で抜けてはいけない
  （生成関数は `nounwind`）。
- 複数スレッドから同時に呼ばれてよい実装でなければならない（native の `Task.parallel` の worker、ホストの複数スレッドからの export 呼び出し）。
- 3 関数の中から Tsuzuri の export・`tsuzuri_alloc`・`tsuzuri_free` を呼んではいけない。最初の Tsuzuri 呼び出しの前から使える状態にしておく
  （登録関数はない。D1）。返す領域を 0 で埋める必要はない。
- ホストは `tsuzuri_alloc`／`tsuzuri_free`（weak）を上書きしない。Tsuzuri が返した所有バッファは従来どおり `tsuzuri_free` で解放し、
  `tsuzuri_host_free` を直接呼ばない（ヘッダーの 16 bytes がずれる）。
- 同じプロセスに複数の Tsuzuri object をリンクしても 3 関数は一組で、全 object が共有する。

### 評価順序・所有権・借用

言語の意味は変えない。`@tz.alloc`・`@tz.free`・`@tz.realloc` の呼び出し位置・回数・順序は `--allocator` によらず同じで、Tsuzuri の確保 1 回が
ホストの確保（または realloc）1 回、解放 1 回がホストの解放 1 回に対応する。drop の順序・一回性も変わらない。
Tsuzuri が受け取るポインターはホストの戻り値 + 16 である。

### 数値・トラップ・native と WASM の差

- 要求サイズが `2^63 - 17`（9223372036854775791）を超えるとホストを呼ばずにトラップする（`AllocationFailure`）。`size + 16` は `i64` で
  正のまま収まる。
- トラップは `@tz.alloc`／`@tz.realloc` の本体の `@llvm.trap` で起こし、`runtime_kind` の既存の分類をそのまま使う。
- wasm32 では `--allocator host` を受けない（D6）。WASM の出力・import（空）・16 MiB の上限は変わらない。
- native object は、heap runtime を連結したとき（プログラムが確保する、または `tsuzuri_alloc` を公開する）だけ 3 関数を未定義の外部記号として参照する。
  確保しないプログラムでは参照が出ず、ホストは定義しなくてよい（定義しても害はない）。

### 診断

CLI の解析エラーは既存と同じく `<command line>:1:1: error[E2000]: <message>` で終了コード 2。

| コード | 条件 | メッセージ | 位置 |
| --- | --- | --- | --- |
| E2000 | `--allocator` の値が `system`・`host` 以外 | `allocator must be 'system' or 'host'` | コマンドライン |
| E2000 | `--allocator` が 2 回以上 | `allocator specified more than once` | コマンドライン |
| E2000 | build 以外の action で `--allocator` | `--allocator is only valid with build` | コマンドライン |
| E2000 | `host` と `--target wasm32`（`--wasm-feature` を含む） | `--allocator host requires a native target; WebAssembly modules keep their internal allocator` | コマンドライン（`BuildOptions::validate`） |
| E2000 | `host` と `--emit exe`（既定）・`wasm`・`wgsl` | `--allocator host requires object, LLVM IR, or header output; link the object into a host that defines tsuzuri_host_alloc, tsuzuri_host_free, and tsuzuri_host_realloc` | コマンドライン（`BuildOptions::validate`） |
| E2000 | ライブラリ API で `EmitOptions { wasm: true, allocator: Allocator::Host, .. }` | `the host allocator requires a native target` | `Span::default()`（`emit_program`） |

`validate` は target の検査を emit の検査より先に行う（`--target wasm32 --emit object --allocator host` は target のメッセージ）。

### 資源上限

- 確保ごとに 16 bytes のヘッダーを使う（WASM の free-list、テストの追跡関数と同じ量）。PM05 Phase 1 後の削減は PM05 の範囲。
- 要求サイズの上限は `2^63 - 17` bytes。align は 16 固定。

### 例

新 API（実装後に有効。未検証）。受理される使い方:

```sh
tsuzuri build Lib --emit object --allocator host -O3 -o lib.o
tsuzuri build Lib --emit header --allocator host -o lib.h
cc -std=c11 host.c lib.o -lm -o app   # host.c が tsuzuri_host_alloc などを定義する
tsuzuri build Lib --target wasm32 --allocator system -o lib.wasm   # 指定なしと同じ WASM
```

拒否される使い方（結果はすべて `E2000`、終了コード 2）:

```sh
tsuzuri build Lib --allocator host                       # 既定の exe
tsuzuri build Lib --target wasm32 --allocator host
tsuzuri build Lib --emit object --allocator pool
tsuzuri run Main.tz --allocator system
```

最小のホスト実装（新 API（実装後に有効。未検証）。`tests/allocator_host.c` はこれに計数と検査を足す）:

```c
#include <stdlib.h>
#include "lib.h"
void *tsuzuri_host_alloc(uint64_t size, uint64_t align) {
    return aligned_alloc((size_t)align, ((size_t)size + align - 1) / align * align);
}
void tsuzuri_host_free(void *ptr, uint64_t size, uint64_t align) { (void)size; (void)align; free(ptr); }
void *tsuzuri_host_realloc(void *ptr, uint64_t old_size, uint64_t new_size, uint64_t align) {
    (void)old_size; (void)align;
    return realloc(ptr, (size_t)new_size); /* glibc・macOS の realloc は 16 byte 境界を返す */
}
```

### Phase 2（設計方針）

- `--allocator counting`（診断用、native と WASM）: 確保回数・現在量・最大量を公開関数 `tsuzuri_alloc_stats`（新規）から取得する。
  テストの文字列置換による確保追跡を正式な機能に置き換える候補。
- WASM でホストの allocator を使う場合は import が要るので、明示的な opt-in（D-18）とする。
- WASM の size-class allocator は PM05 Phase 2 が担当する（D9 の見直し提案）。

### Phase 3（設計方針）

- `--freestanding`（native object だけ）: libc に依存しない出力。`--allocator host` を必須にし、IO・Task・Debug・trap reporter の `write` を使う
  プログラムは `E2000`。トラップは `llvm.trap` だけにする。G15 の bare-metal target の前提になる。

## 設計

### データ構造

```rust
// src/llvm.rs（新規 enum と field 追加）
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Allocator {
    #[default]
    System,
    Host,
}

pub struct EmitOptions {
    pub entry: Entry,
    pub wasm: bool,
    pub debug_output: bool,
    pub allocator: Allocator, // 追加
}

struct Instrumentation<'a> {
    // 既存の traps, debug, cpu_dispatch, wasm_threads に加えて
    allocator: Allocator, // Default は System
}

// src/driver.rs
pub struct BuildOptions {
    // 既存 field に加えて（Default は Allocator::System）
    pub allocator: llvm::Allocator,
}
```

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| CLI | `src/main.rs` | `HELP` | Build options に `--allocator system\|host` の 1 行（既定 system、host は native object/LLVM/header のみ） |
| CLI | `src/main.rs` | 引数解析の loop | `--cpu` 分岐と同形の `Some("--allocator")` 分岐と `allocator` 変数。build 以外なら `--allocator is only valid with build`。`BuildOptions` へ渡す |
| CLI | `src/main.rs` | tests の `rejects_ambiguous_or_unused_arguments` | 拒否ケースを追加。`parses_allocator_selection`（新規）を追加 |
| driver | `src/driver.rs` | `BuildOptions`, `Default`, `BuildOptions::validate` | field と、診断表の 2 つの検査（target → emit の順） |
| driver | `src/driver.rs` | build の header 分岐と `llvm::EmitOptions` の構築 | `llvm::header_with_allocator(module, options.allocator)`、`allocator: options.allocator` |
| cache | `src/cache.rs` | cache key | 変更なし（`format!("{options:?}")` に新 field が入る） |
| emit | `src/llvm.rs` | `Allocator`, `EmitOptions`, `Instrumentation` | データ構造のとおり |
| emit | `src/llvm.rs` | `emit_target` | `allocator: Allocator::System` |
| emit | `src/llvm.rs` | `emit_with_options` | `emit_selected` ではなく `emit_program` を `Instrumentation { allocator: options.allocator, ..Instrumentation::default() }` で呼ぶ |
| emit | `src/llvm.rs` | `emit_with_export_style`, `emit_with_trap_info`, `emit_with_debug_info`, `emit_native_build`, `emit_wasm_threads_build` | 各 `Instrumentation` に `allocator: options.allocator` |
| emit | `src/llvm.rs` | `emit_program` | 先頭で `wasm && allocator == Host` を `E2000`。末尾の heap runtime の選択（次節） |
| header | `src/llvm.rs` | `header`, `header_with_allocator`（新規） | `header(module)` は `header_with_allocator(module, Allocator::System)`。Host なら `#ifdef __cplusplus` で閉じる直前に prototype を足す |
| header | `src/llvm_abi.rs` | `HOST_ALLOCATOR_PROTOTYPES`（新規） | 仕様の 3 行と契約の 1 行コメント |
| runtime | `src/runtime/heap-host.ll`（新規） | `@tz.alloc`, `@tz.free`, `@tz.realloc` | 次節 |
| 追跡 | `.gitignore` | `*.ll` の例外 | `!src/runtime/heap-host.ll`（GUIDE §6.6） |
| trap | `src/llvm_traps.rs` | `runtime_kind`, `instrument` の `external` | 変更なし（関数名が同じ。3 関数は宣言だけで `define` されない） |
| test | 既存 5 ファイル | `llvm::EmitOptions { .. }` | `allocator: llvm::Allocator::System` を足す（期待値は変えない） |

### 生成 IR とランタイム

`emit_program` の heap 連結は次の形にする（threads の分岐は変えない）。

```rust
output.push_str(match (wasm, instrumentation.allocator) {
    (true, _) => include_str!("runtime/heap-wasm.ll"),
    (false, Allocator::System) => include_str!("runtime/heap-native.ll"),
    (false, Allocator::Host) => include_str!("runtime/heap-host.ll"),
});
```

`src/runtime/heap-host.ll`（新規。未検証の形で、手順 3 で native `-O0`/`-O3` の compile を確かめる）。`@llvm.trap` は `emit_program` が
宣言するので、このファイルでは宣言しない。

```llvm
declare ptr @tsuzuri_host_alloc(i64, i64)
declare void @tsuzuri_host_free(ptr, i64, i64)
declare ptr @tsuzuri_host_realloc(ptr, i64, i64, i64)

define internal ptr @tz.alloc(i64 %size) nounwind {
entry:
  %large = icmp ugt i64 %size, 9223372036854775791
  br i1 %large, label %fail, label %request
request:
  %total = add i64 %size, 16
  %base = call ptr @tsuzuri_host_alloc(i64 %total, i64 16)
  %address = ptrtoint ptr %base to i64
  %low = and i64 %address, 15
  %null = icmp eq ptr %base, null
  %misaligned = icmp ne i64 %low, 0
  %bad = or i1 %null, %misaligned
  br i1 %bad, label %fail, label %ok
fail:
  call void @llvm.trap()
  unreachable
ok:
  store i64 %size, ptr %base, align 16
  %pointer = getelementptr inbounds i8, ptr %base, i64 16
  ret ptr %pointer
}

define internal void @tz.free(ptr %pointer) nounwind {
entry:
  %null = icmp eq ptr %pointer, null
  br i1 %null, label %done, label %release
release:
  %base = getelementptr inbounds i8, ptr %pointer, i64 -16
  %size = load i64, ptr %base, align 16
  %total = add i64 %size, 16
  call void @tsuzuri_host_free(ptr %base, i64 %total, i64 16)
  br label %done
done:
  ret void
}
```

`@tz.realloc(ptr %old, i64 %old_size, i64 %new_size)` は同じ形で、次の順に分岐する。`%old_size` は使わず、ヘッダーの値を正とする。

1. `new_size == 0`: `call void @tz.free(ptr %old)`、`ret ptr null`（heap-native と同じ）。
2. `old == null`: `@tz.alloc(new_size)` の結果を返す。
3. `new_size > 9223372036854775791`: `@llvm.trap`。
4. それ以外: `base = old - 16`、`stored = load i64 base`、`tsuzuri_host_realloc(base, stored + 16, new_size + 16, 16)`。
   null または 16 の倍数でなければ `@llvm.trap`。成功なら `store i64 new_size` をヘッダーへ書き、`結果 + 16` を返す。

### アルゴリズム

| Tsuzuri 側の呼び出し | ホストへの呼び出し | Tsuzuri へ返す値 |
| --- | --- | --- |
| `@tz.alloc(n)` | `tsuzuri_host_alloc(n + 16, 16)` → `p`、`*(uint64_t *)p = n` | `p + 16` |
| `@tz.free(null)` | なし | なし |
| `@tz.free(q)` | `n = *(uint64_t *)(q - 16)`、`tsuzuri_host_free(q - 16, n + 16, 16)` | なし |
| `@tz.realloc(q, _, 0)` | `@tz.free(q)` と同じ | null |
| `@tz.realloc(null, _, m)` | `@tz.alloc(m)` と同じ | `p + 16` |
| `@tz.realloc(q, _, m)` | `tsuzuri_host_realloc(q - 16, n + 16, m + 16, 16)` → `p`、ヘッダーへ `m` | `p + 16` |

`tsuzuri_alloc`（weak、`src/llvm_abi.rs`）と `io.c` の行バッファはこの `@tz.alloc`／`@tz.free` を通るので、ホストから見たサイズは全経路で一致する。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N を必ず見る（GUIDE §3.1）。作業用の複写は `/tmp/tz-f13/` に置く（E03: fixture は独立した root へ複写する）。

### 手順 1: ベースラインを取る

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行し、`tests/fixtures/host_abi` を `/tmp/tz-f13/abi` へ複写して既定の出力を保存する。
- 確認: 次がすべて成功する。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
mkdir -p /tmp/tz-f13/before && cp -R tests/fixtures/host_abi /tmp/tz-f13/abi
target/release/tsuzuri build /tmp/tz-f13/abi --emit llvm -o /tmp/tz-f13/before/abi.ll
target/release/tsuzuri build /tmp/tz-f13/abi --emit header -o /tmp/tz-f13/before/abi.h
for o in 0 3; do
  target/release/tsuzuri build /tmp/tz-f13/abi --emit object -O$o --no-cache -o /tmp/tz-f13/before/abi-O$o.o
  nm /tmp/tz-f13/before/abi-O$o.o > /tmp/tz-f13/before/abi-O$o.nm
  target/release/tsuzuri build /tmp/tz-f13/abi --target wasm32 -O$o --no-cache -o /tmp/tz-f13/before/abi-O$o.wasm
done
node tests/features.mjs target/release/tsuzuri
```

### 手順 2: `Allocator` と field の追加（出力は不変）

- 変更: `src/llvm.rs` の `Allocator`（新規）、`EmitOptions`、`Instrumentation`、`emit_target` と `EmitOptions` を受け取る pub 関数、
  `src/driver.rs` の `BuildOptions` と `Default`・`EmitOptions` の構築、既存テスト 5 ファイルの `EmitOptions` 構築。
- 内容: 「段ごとの変更」の emit 行のうち `emit_program` の heap 選択以外。`emit_with_options` を `emit_program` 直呼びへ変える。
- 確認: `cargo test --locked` が成功する。release を作り直し、手順 1 の 5 種の出力を `/tmp/tz-f13/after/` へ出して
  `cmp`（IR・header・wasm）と `diff`（nm）がすべて一致する。

### 手順 3: `heap-host.ll` と IR の切り替え

- 変更: `src/runtime/heap-host.ll`（新規）、`.gitignore`、`src/llvm.rs` の `emit_program`、`tests/allocator.rs`（新規）。
- 内容: 「生成 IR とランタイム」のとおり。`git status --short src/runtime/heap-host.ll` で追跡対象になったことを確かめる。
  テストは「Rust テスト」の 1〜3。
- 確認: `cargo test --locked --test allocator` が `3 passed`。テスト 2 が書き出す host の IR を
  `clang -x ir -c -O0` と `-O3`（`/opt/homebrew/opt/llvm@21/bin/clang` でも可）で compile でき、`nm` に `U _tsuzuri_host_alloc` が出る。

### 手順 4: header の prototype

- 変更: `src/llvm.rs` の `header`・`header_with_allocator`（新規）、`src/llvm_abi.rs` の `HOST_ALLOCATOR_PROTOTYPES`（新規）、`tests/allocator.rs`。
- 内容: prototype は `extern "C"` 区間の中、export の prototype の後、`#ifdef __cplusplus` で閉じる直前に置く。テスト 5 を足す。
- 確認: `cargo test --locked --test allocator` が `4 passed`。`cargo test --locked --test host_abi` が既存件数のまま成功する。

### 手順 5: トラップ理由の保持

- 変更: `tests/allocator.rs`。
- 内容: テスト 4（`--trap-info` 相当の出力で System と Host の `TrapKind` の集合が等しい）を足す。
- 確認: `cargo test --locked --test allocator` が `5 passed`。

### 手順 6: CLI と `validate`

- 変更: `src/main.rs`（`HELP`、引数解析、`parses_allocator_selection`（新規）、`rejects_ambiguous_or_unused_arguments`）、`src/driver.rs`
  （`BuildOptions::validate`、header 分岐、`EmitOptions` の `allocator`）。
- 内容: 診断表の 5 つの CLI 検査。解析段の 3 つは `src/main.rs`、組み合わせの 2 つは `validate`。
- 確認: `cargo test --locked --bin tsuzuri` が成功し、`parses_allocator_selection` を含む。release を作り直し、次を確かめる。
  1 行目は成功し `nm` に `U _tsuzuri_host_alloc`・`U _tsuzuri_host_free` が出て `_malloc` が出ない。2〜4 行目は終了コード 2 と診断表のメッセージ。

```sh
target/release/tsuzuri build /tmp/tz-f13/abi --emit object --allocator host --no-cache -o /tmp/tz-f13/host.o && nm /tmp/tz-f13/host.o | grep -E "tsuzuri_host|malloc"
target/release/tsuzuri build /tmp/tz-f13/abi --allocator host -o /tmp/tz-f13/x
target/release/tsuzuri build /tmp/tz-f13/abi --target wasm32 --allocator host -o /tmp/tz-f13/x.wasm
target/release/tsuzuri run /tmp/tz-f13/abi --allocator system
```

### 手順 7: C ホストの E2E

- 変更: `tests/allocator_host.c`（新規）、`tests/allocator.mjs`（新規）。
- 内容: 「E2E」の A〜G。`tests/features.mjs` の `cli`・`execute` と一時 root（`mkdtempSync`）の使い方を写す。
- 確認: `node tests/allocator.mjs target/release/tsuzuri` が成功し、最後に `allocator: ok` を出す。

### 手順 8: 既存 E2E の host allocator 版

- 変更: `tests/features.mjs`。
- 内容: 環境変数 `TSUZURI_TEST_ALLOCATOR=host`（新規）で `run` の IR と header を `--allocator host` で作り、C の前置きを
  `tsuzuri_host_*` の追跡実装へ替える（「E2E」の H）。既定では何も変えない。
- 確認: `node tests/features.mjs target/release/tsuzuri` と `TSUZURI_TEST_ALLOCATOR=host node tests/features.mjs target/release/tsuzuri` が成功する。

### 手順 9: sanitizer

- 変更: なし。
- 内容: `Task.parallel` の worker からの同時呼び出しと、解放後の使用を確かめる。
- 確認: `TSUZURI_TEST_ALLOCATOR=host TSUZURI_TSAN=1 node tests/features.mjs target/release/tsuzuri parallel` と
  `TSUZURI_TEST_ALLOCATOR=host TSUZURI_ASAN=1 node tests/features.mjs target/release/tsuzuri` が成功する。clang が sanitizer に対応しない環境では
  実行できなかったことを報告する（失敗とみなさない）。

### 手順 10: 文書

- 変更: 「ドキュメント」の全ファイル。
- 確認: `node scripts/check-docs.mjs _docs/tools/command-line.md _docs/guides/native-interop.md _docs/feature-status.md` が成功する。

### 手順 11: 最終確認

- 確認: GUIDE §10 のコマンド（`cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test --locked`）、
  手順 2 の不変性の比較、`node tests/allocator.mjs target/release/tsuzuri`、既定と host の `tests/features.mjs`、
  `node tests/host_imports.mjs target/release/tsuzuri`・`node tests/io.mjs target/release/tsuzuri`（既定の追跡が壊れていないこと）がすべて成功する。

## テスト計画

### Rust テスト

`tests/allocator.rs`（新規）。`tests/host_abi.rs` と同じく `tsuzuri::driver::Project::load` で `tests/fixtures/host_abi/Main.tz` を読む。
runtime の文字列は `include_str!("../src/runtime/heap-native.ll")` と `include_str!("../src/runtime/heap-host.ll")` で得る。

1. `system_allocator_output_is_unchanged`: `emit_with_options`（`Allocator::System`）が `emit_target(&module, Entry::Library, false)` と一致し、
   `wasm: true` でも `emit_target(.., true)` と一致する。
2. `host_allocator_swaps_only_the_heap_runtime`: Host の IR から heap-host の本文を除いたものが、System の IR から heap-native の本文を除いたものと一致する。
   `declare ptr @tsuzuri_host_alloc(i64, i64)` が 1 回だけ現れ、`@malloc`・`@free(`・`@realloc(` を含まない。2 回の出力が一致する。
   IR を `std::env::temp_dir()` 配下へ書き出す（手順 3 の手動確認用）。
3. `host_allocator_rejects_wasm`: `wasm: true` と Host は `E2000`・`the host allocator requires a native target`。
4. `host_allocator_keeps_allocation_failure_traps`: `emit_with_trap_info` を System と Host で呼び、`trap_sites` の `kind` の集合が等しく、
   `TrapKind::AllocationFailure` を含む。trap source は `tests/trap_locations.rs` の先頭のテストと同じ方法で作る。
5. `header_lists_host_allocator_prototypes`: `header(&module)` は `header_with_allocator(&module, Allocator::System)` と一致し `tsuzuri_host_alloc`
   を含まない。Host の header は 3 つの prototype を含み、その位置は最後の `#ifdef __cplusplus` より前。

`src/main.rs` の tests:

- `parses_allocator_selection`（新規）: `build Main.tz --emit object --allocator host` は `Allocator::Host`、`--emit llvm`・`--emit header` も受理。
  指定なしは `Allocator::System`。`build Main.tz --target wasm32 --allocator system` は受理。
- `rejects_ambiguous_or_unused_arguments` へ追加: `--allocator host`（既定 exe）、`--target wasm32 --allocator host`、
  `--target wasm32 --emit object --allocator host`、`--emit wgsl --allocator host`、`--emit object --allocator pool`、`--allocator` の重複、
  値なしの `--allocator`、`run`・`check`・`test` での `--allocator system`。

### E2E

`tests/allocator.mjs`（新規）。fixture は `tests/fixtures/host_abi` を一時 root へ複写して使う。期待値は C ホストの計数と固定の定数だけから決める。

- A. 拒否: 手順 6 の 3 コマンドと `--allocator pool`・`--emit wgsl --allocator host` が終了コード 2、stderr に `error[E2000]` と診断表のメッセージ。
- B. 不変: `--allocator system` の有無で `--emit llvm`・`--emit header` が一致、object の `nm` が一致（`-O0`/`-O3`）、wasm32 の `.wasm` が
  byte 単位で一致し `WebAssembly.Module.imports(module)` が空（`-O0`/`-O3`）。
- C. host の IR: 2 回の出力が一致し、`declare` が重複しない。3 つの `declare` を含み `@malloc` を含まない。
- D. host の header: 3 つの prototype。header を include する 1 行の C と C++ を `clang -fsyntax-only` と `clang -x c++ -fsyntax-only` で確かめる。
- E. 実行（`-O0`/`-O3`）: `--emit object --allocator host` と `tests/allocator_host.c` を `clang -std=c11 ... -lm` でリンクし、引数なしで実行する。
  ホストは `align == 16`、`size >= 16`、解放時のサイズ・realloc の `old_size` が記録と一致、を `assert` し、最後に
  `allocations == frees`、`allocations > 0`、`live == 0` を確かめて `allocations=N frees=N live=0` を出す。本体は `tests/host_abi.rs` の
  `tz_copy_values`・`tz_copy_text`・`tz_make_bytes`（1,024 回、毎回 `tsuzuri_free` 後に `live == 0`）を写す。
- F. 境界: `tsuzuri_alloc(0)` はホストへ `size == 17`（0 → 1 byte、ヘッダー 16）で届く。`tsuzuri_free(NULL)` はホストを呼ばない（`frees` 不変）。
- G. 失敗: 引数 `oom`（`tsuzuri_host_alloc` が null）と `misaligned`（16 の倍数 + 8 を返す）で、プロセスが signal または非 0 で終わる。

`tests/features.mjs` の H（`TSUZURI_TEST_ALLOCATOR=host`）: IR と header を `--allocator host` で作り、`@malloc` などの置換はしない
（IR に `@malloc` がないことを `assert` する）。C の前置きは `tsuzuri_host_alloc`・`tsuzuri_host_free`・`tsuzuri_host_realloc` を
`_Atomic uint64_t live` で追跡する実装にし、各ケースの後の既存の `assert(live == 0)` をそのまま使う。WASM 側は既定のまま実行し、import が空であることも既存どおり検査する。
これで全 suite（`parallel` の worker を含む）が host allocator を通る。

### 既存テストへの影響

- `EmitOptions` を構築する既存テスト 5 ファイルに `allocator: llvm::Allocator::System` を足す。期待値は変えない。
- `HELP` に 1 行増える。`grep -rn "Build options:" tests src/main.rs` で HELP を完全一致で比べるテストがあれば、その行の追加だけを期待値へ反映してよい。
- 既定の IR が変わらないので、`@malloc` を置換する全ての確保追跡（「現状」の一覧）は変更なしで動く。

### 性能

性能の主張はしない。host allocator の確保ごとの 16 bytes と、呼び出し 1 段の追加がある。既定の出力は不変なので既存の計測は影響を受けない。

## ドキュメント

- `docs/language.md`: `## 公開 ABI` に `### ホスト提供の allocator`（新規）を足し、「ホストが定義する関数」の prototype と契約を書く。
  `## 再帰とスタック` の「ネイティブでは malloc/free を使います」に `--allocator host` の場合を足す。
- `docs/architecture.md`: `## 不変条件` の allocator の段落（「allocator は拡張 ABI 使用時だけ…」）に、heap runtime の選択、16 bytes のヘッダー、
  トラップ分類が関数名に依存することを足す。
- `README.md` の `## CLI` と `_docs/tools/command-line.md` の `## build と run の主なオプション`: `--allocator system|host` の行と、使える組み合わせ。
- `_docs/guides/native-interop.md`: `## バッファの責任` の後に `## 確保をホストへ委ねる`（新規）。build・header・リンクのコマンド、最小のホスト実装、
  `tsuzuri_free` と `tsuzuri_host_free` の違い、スレッド安全性。
- `_docs/feature-status.md` の F13 行を「Phase 1 実装済み: native object／LLVM／header のホスト提供 allocator。確保統計・freestanding は計画」にする。
- `_features/README.md` の F13 の状態欄を `Phase 1 done` にする。Phase 2・3 が残るのでチケットは `_completed/` へ移さない。

## 受け入れ条件

- [ ] `--allocator host` の native object で、Tsuzuri が所有する全ての確保・解放・realloc がホストの 3 関数を通り、サイズと align が確保と解放で一致する
  （`tests/allocator.mjs` の E・F と host 版 `tests/features.mjs`）。
- [ ] `--allocator` なしと `--allocator system` の出力（IR・header・object の記号・WASM）が HEAD と一致する。
- [ ] ホストの失敗（null・境界違反）が `AllocationFailure` のトラップになる。
- [ ] 診断表の全ケースが `E2000` とそのメッセージで拒否される。
- [ ] native × `-O0`/`-O3` で `live == 0`、WASM × `-O0`/`-O3` で出力不変・import なし。
- [ ] GUIDE §10 の完了の定義を満たす（チケットの移動は除く。「ドキュメント」）。

## 落とし穴

- `.gitignore` は `*.ll` を無視する。`!src/runtime/heap-host.ll` を足さないと手元では通り CI で `include_str!` が失敗する。
- `heap-host.ll` で `declare void @llvm.trap()` を書くと、`emit_program` の宣言と重なって IR が不正になる。書かない。
- トラップを helper 関数へ移すと `runtime_kind` が `NumericRuntime` に分類する。`@llvm.trap` は `@tz.alloc`／`@tz.realloc` の本体に置く。
- `@tz.free` は null を受ける（libc の `free(NULL)` と同じ前提のコードがある）。ヘッダーを読む前に null を判定する。
- `@tz.realloc` は `old == null` で呼ばれうる（heap-native は libc の `realloc(NULL, n)` に任せていた）。ヘッダーを読まずに `@tz.alloc` へ回す。
- `%old_size` は呼び出し側の値で、ホストへ渡すサイズの根拠にしない。ヘッダーの値を使う。
- header の prototype を `#ifdef __cplusplus` の閉じの後に足すと、C++ のホストで名前が mangle されてリンクできない。E2E の D で C++ の構文検査をする。
- `emit_selected`・`emit_test_runner` は既定の allocator のままでよい（test は `--allocator` を受けない）。`emit_with_options` だけを直す。
- `tsuzuri_host_` は E06 のホスト関数の import 名（`tsuzuri_host_<Module>_<name>`、`src/check.rs`）と接頭辞を共有する。3 関数は接頭辞の後に `_` を含まないので衝突しない。
  名前を変えたくなっても変えない（D2）。
- `tests/features.mjs` の置換は部分文字列の置換である。host 版では置換を行わず、`@tsuzuri_host_free` が `@free` の置換に巻き込まれないようにする。
- cache key は `BuildOptions` の `Debug` 表現を含むので、`allocator` を `Debug` から外したり、cache を無効にして回避したりしない。
- native object の `clang -r -nostdlib` は未定義記号を残す。`nm` で `U` を確かめ、`T` や欠落なら停止条件。

## 対象外

- 値ごと・型ごとの allocator の指定（言語の型パラメーターや `new` の引数）。所有権と drop は解放先の allocator を知る必要があり、所有する全ての型
  （string・配列・Vec・関数値の環境・Task・再帰 union のノード）に allocator の識別子を持たせ、move・clone・Task 境界・`tsuzuri_free` の ABI
  をすべて変えることになる。region 設計（A12 以降）と合わせて別チケットで扱う。
- arena に確保した値の寿命検査（C10・A12）、GC、libc や pthread の内部確保（スレッドの stack など）の差し替え。
- WASM のホスト allocator、確保統計、freestanding（Phase 2・3）。サイズ付きの `@tz.free` と size-class allocator（PM05）。
- `--emit exe` との組み合わせ（E12 のリンク指定の後に D7 で見直す）。

## 決定事項

### D1: 差し替えの方法

- 決定: リンク時に解決される 3 つの外部関数（`tsuzuri_host_alloc`・`tsuzuri_host_free`・`tsuzuri_host_realloc`）で差し替える。
  登録関数（例: `tsuzuri_set_allocator(alloc, free, realloc, context)`）と context 引数は設けない。
- 理由: 直接呼び出しで間接呼び出しと分岐が要らない。登録前の確保、実行中の差し替えで旧 allocator のブロックを新 allocator が解放する誤り、
  登録自体のスレッド安全性という失敗の型が生じない。定義を忘れればリンクエラーになる。context が要るホストは自分の大域状態から得られる。
- 状態: 既定案（実装者はこの案に従う）

### D2: C の型と名前

- 決定: 名前は起票時のまま。サイズと align は `uint64_t`、ポインターは `void *`。align は Phase 1 では常に 16 を渡す。
- 理由: 生成 IR の `i64` と一致し、header が既に include する `<stdint.h>` だけで済む（`size_t` にすると `<stddef.h>` が要り、macOS では
  `unsigned long` と `uint64_t` が別の型になって宣言の不一致を招く）。16 は `storage_layout` の align の最大。
- 状態: 既定案（実装者はこの案に従う）

### D3: 解放時のサイズ

- 決定: `heap-host.ll` が各ブロックの先頭 16 bytes に要求サイズを記録し、ホストへは `要求サイズ + 16` を確保・解放・realloc で一貫して渡す。
  `@tz.free` の引数は変えない。
- 理由: ホストが受け取る所有バッファ（`tsuzuri_free`）や、ホストが `tsuzuri_alloc` で確保して渡す所有結果は、Tsuzuri 側でサイズが分からない。
  起票時の案（外部解放の経路だけヘッダー）は、確保の時点でどの経路で解放されるかが分からないので実現できない。全経路のサイズ付き解放は PM05 Phase 1 の範囲で、
  その後にヘッダーを省く判断は PM05 が行う。16 bytes なら align 16 を保てる。
- 状態: 既定案（実装者はこの案に従う）

### D4: realloc の意味

- 決定: 「生成 IR とランタイム」の 4 分岐。ホストの realloc は null でない `ptr` と 0 でない `new_size` でだけ呼ぶ。
- 理由: heap-native（libc の `realloc`）と同じ観測可能な挙動を保ちつつ、ホストの実装を最小にする。
- 状態: 既定案（実装者はこの案に従う）

### D5: 失敗の扱い

- 決定: null・16 の倍数でない戻り値・要求サイズの超過は、`@tz.alloc`／`@tz.realloc` の中の `@llvm.trap` で `AllocationFailure` にする。回復はしない。
- 理由: 既定の allocator と同じ意味。境界違反を放置すると `i128` や SIMD の読み書きで未定義動作になるので、信頼境界で検査する（2 命令）。
  E14 のトラップ境界はこのトラップにもそのまま適用される。
- 状態: 既定案（実装者はこの案に従う）

### D6: WASM

- 決定: wasm32 は常に module 内の free-list（threads は lock 版）を使い、`--allocator host` は `E2000`。`--allocator system` は受理し出力は不変。
- 理由: ホストの allocator は import を要し、D-18 の既定で import なしを破る。明示 opt-in の設計は Phase 2（D9）。
- 状態: 既定案（実装者はこの案に従う）

### D7: 使える出力形式

- 決定: `host` は native の `object`・`llvm`・`header` だけ。`exe` は `E2000`。
- 理由: `exe` はリンク時に 3 関数の定義が要り、ホストのライブラリを指定する CLI は E12 まで存在しない。E12 の完了後に `exe` の受理を見直す。
- 状態: 既定案（実装者はこの案に従う）

### D8: CLI の名前と値

- 決定: `--allocator system|host`、build 専用、一度だけ。`system` は target 標準（native の libc malloc、wasm32 の free-list）。診断は `E2000`。
- 理由: 起票時の名前を保つ。`--wasm-feature` と同じく build 専用にし、run・test は既定の allocator だけで検証済みの経路を使う。
  PM05 などが allocator を足すときは値を足すだけで済む。
- 状態: 既定案（実装者はこの案に従う）

### D9: Phase 2・Phase 3 の着手

- 決定: Phase 2（`--allocator counting`、WASM のホスト allocator の opt-in）と Phase 3（`--freestanding`）は、人間の承認後に別の詳細化を経て着手する。
- 理由: WASM の import の opt-in（D-18）、trap reporter・IO・Task を外す出力形式の追加は、公開 ABI と台帳に関わる。
- 見直し提案: 起票時の Phase 2 にあった「WASM の size-class allocator」は PM05 Phase 2 と重複するので PM05 へ移す。
- 状態: 要承認（承認前は Phase 2・Phase 3 に着手しない）

### D10: スレッド安全性

- 決定: 3 関数のスレッド安全性はホストの責任とし、`heap-host.ll` に lock を置かない。
- 理由: native の既定（libc malloc）と同じ責任分担。プールや thread-local cache の設計をホストが選べる。二重の lock は性能を損なう。
- 状態: 既定案（実装者はこの案に従う）

### D11: テストの確保追跡

- 決定: 既定の出力を変えず、既存の文字列置換による追跡をそのまま使う。host allocator の検査は `tests/allocator.mjs` と、
  `TSUZURI_TEST_ALLOCATOR=host` の `tests/features.mjs` で行う。
- 理由: 既定の追跡は 12 の harness にまたがり、置き換えは Phase 2 の `counting` の範囲。host 版を env で切り替える方式は `TSUZURI_TSAN` などの既存の慣習に合う。
- 状態: 既定案（実装者はこの案に従う）

## 実装と検証（2026-10-06）

「C08、A14、A16、F13、F08 の実装をすべて完遂して」という依頼を D9 の承認として扱い、Phase 1 と、設計方針だけだった Phase 2（`--allocator counting`、
WASM の host allocator）と Phase 3（`--freestanding`）を設計して実装した。着手時の HEAD は `ff84e4c`（ブランチ `Phase7-3`）で、C08・A16・A14・F08 と同じ変更に含めた。
手順 1 のベースラインは、先に実装した C08・A16・A14 を含む時点の compiler で取った。性能の主張はしない。

### 実装

- `src/llvm.rs`: `Allocator { System, Host, Counting }`（`Default` は `System`）、`EmitOptions::allocator`、`Instrumentation::allocator`。
  `emit_with_options` は `emit_program` を直接呼ぶ（`emit_selected` はなくした）。`emit_program` は threads と host、`--trap-mode return` と system 以外の組み合わせを
  `E2000` にし、末尾で heap runtime を一つ選ぶ。`counted_base` は heap を `@tz.alloc.base`・`@tz.free.base`・`@tz.realloc.base` へ改名し、`heap_host_wasm` は
  3 つの `declare` に `"wasm-import-module"="tsuzuri_heap"` と `alloc`・`free`・`realloc` を付ける。counting の出力は常に `tsuzuri_alloc`・`tsuzuri_free` を定義する。
  header は `header_with_allocator(module, allocator)`（新規）。`header`・`header_with` は従来どおり。
- `src/runtime/heap-host.ll`（新規）: 仕様どおり。`src/runtime/heap-counting.ll`（新規）: 16 バイトのヘッダーで基底の heap を包み、`@tz.heap.counts`
  （確保・解放・現在・最大）を `atomicrmw`（monotonic）で更新し、`tsuzuri_alloc_stats` が原子的に読んで書き出す。`.gitignore` の例外に 2 ファイルを足した。
- `src/llvm_abi.rs`: `HOST_ALLOCATOR_PROTOTYPES`、`ALLOCATION_STATS_PROTOTYPE`（`tsuzuri_allocation_stats` と `TSUZURI_ALLOCATION_STATS_DEFINED`）。
- `src/llvm_traps.rs`: `runtime_kind` が `@tz.alloc.base`・`@tz.realloc.base` のトラップも `AllocationFailure` にする（既存の名前の分類は変えない）。
- `src/driver.rs`: `BuildOptions::{allocator, freestanding}` と `validate_allocator`。header の分岐、`EmitOptions::allocator`、`--freestanding` は
  `emit_native_build`（CPU ディスパッチ）を通らない。生成後に、freestanding で C ライブラリを要する runtime（タスク、標準 IO、OS API、引数、`write`／`putchar`）を
  `E2000` にする。WASM のリンクは counting で `tsuzuri_alloc_stats`・`tsuzuri_alloc`・`tsuzuri_free`・memory を、host で `__heap_base`・memory を export する。
- `src/main.rs`: `--allocator system|host|counting`・`--freestanding`（build だけ、一度だけ）、HELP、`parses_allocator_selection`。
- テスト: `tests/allocator.rs`（7 件）、`tests/allocator.mjs`・`tests/allocator_host.c`（新規）、`tests/features.mjs` の `TSUZURI_TEST_ALLOCATOR=host`、
  `EmitOptions` を作る既存テスト 6 ファイル（チケットの 5 つと `tests/bounds_checks.rs`）。
- 文書: `README.md`、`docs/language.md`（`### ホスト提供の allocator`）、`docs/architecture.md`、`_docs/tools/command-line.md`、`_docs/guides/native-interop.md`
  （`## 確保をホストへ委ねる`）、`_docs/learn/why-tsuzuri.md`、`_docs/feature-status.md`、`_features/README.md`、`_perfs/README.md`、`_features/GUIDE.md`（D-30・D-39）。

### 決定事項への追記（チケットから外れた判断）

1. **D6 を Phase 2 で改めた。** WASM（wasm32・wasm64）でも `--allocator host` を受け、3 関数を import する（D-18 の明示の opt-in）。import は既存の
   `tsuzuri_io`・`tsuzuri_debug` に合わせてモジュール `tsuzuri_heap`、名前 `alloc`・`free`・`realloc` にした。ホストが管理する範囲を示すため
   `__heap_base` と memory を export する。`--wasm-feature threads` との併用は `E2000`（worker ごとの JS の import が同じ共有メモリを管理する必要がある）。
   チケットの診断「`--allocator host requires a native target`」はなく、`emit_program` の検査は threads との組み合わせになった。
2. **D7 の出力形式。** host と counting は object・LLVM IR・header に WASM を加えた。`--emit exe`・`wgsl` は `E2000` で、文言に WebAssembly を含めた。
3. **Phase 2 の counting。** `--allocator counting` は system の allocator を host と同じ 16 バイトのヘッダーで包む（基底の名前は `.base`）。確保 1 回・解放 1 回を数え、
   resize は現在量だけを動かす。最大値は `atomicrmw umax`。ホストの読み出しは `void tsuzuri_alloc_stats(tsuzuri_allocation_stats *stats)` の一つで、
   WASM ではホストが `tsuzuri_alloc(32)` の領域を渡す（この確保も数に入る）。exe は数を読む相手がいないので `E2000`。
4. **`--trap-mode return` との併用。** 追跡 heap（`heap_native_tracked`）を使うので、system 以外の allocator との併用は `E2000` にした。
5. **Phase 3 の `--freestanding`。** native の object・LLVM IR・header だけで、`--allocator host` が必須。`--trap-info`・`--debug-output` は `validate` で、
   C ライブラリを要する runtime を到達させたプログラムは生成後に `E2000`（終了コード 1、`--freestanding cannot use ...`）にする。
   CPU ディスパッチ（`cpu.c`）は使わない。参照してよい外部記号は `tsuzuri_host_*`、extern のホスト関数、`memcpy` などの freestanding な C の関数、
   compiler-rt／libgcc の組み込み（128-bit 除算の `__divti3` など）とした。
6. **既存の追跡ハーネスの一覧。** 既定の IR が変わらないので、`@malloc` を置換する harness は変更していない。

### 確認（Apple M1 Max、macOS 27.0.1、Apple clang 21、Homebrew LLVM 21、rustc 1.98.1、Node v20.19.6）

- 手順 2 の不変性: `tests/fixtures/host_abi` の `--emit llvm`・`--emit header`、object の `nm`（`-O0`・`-O3`）、wasm32（`-O0`・`-O3`）が、
  F13 の前の compiler の出力と byte 一致した。`--allocator system` の有無でも一致した。全 fixture と例の IR の比較は F08 の記録を参照。
- `cargo test --locked --test allocator` は 7 passed、`cargo test --locked --bin tsuzuri` は 11 passed（`parses_allocator_selection` を含む）。
- `node tests/allocator.mjs target/release/tsuzuri` は `allocator: ok`。拒否 8 件、不変性、host の IR・header（C と C++ の構文検査）、
  host の object と `tests/allocator_host.c`（`-O0`・`-O3`。確保 1,027 回と同数の解放、`live=0`、`tsuzuri_alloc(0)` が 17 バイト、null の解放はホストを呼ばない、
  `oom`・`misaligned` はトラップ）、counting の native（`tsuzuri_alloc_stats`）と WASM、WASM の host allocator（JS の bump allocator。realloc を含む）、
  freestanding の object の `nm -u` が `tsuzuri_host_alloc`・`tsuzuri_host_free`（`-O0` は `tsuzuri_host_realloc` も）だけであること、IO・タスク・Debug の拒否。
- `TSUZURI_TEST_ALLOCATOR=host node tests/features.mjs target/release/tsuzuri` は全 suite で成功（5,481 ケース、`live == 0`）。
  同じ設定で `TSUZURI_TSAN=1` の `parallel`（36 ケース）と `TSUZURI_ASAN=1` の全 suite も成功した。
- 全体のゲート（fmt・clippy・`cargo test`・既定の features の全 suite・`check-docs`・`sh scripts/check-runtime-includes.sh`）は 5 チケットの実装の後に
  まとめて実行した（F08 の記録を参照）。

### レビュー対応（PR #14）

- `--freestanding` の検査は生成した出力の文字列を調べていたので、`--emit header` では C の header だけを調べて IO・タスク・OS API・引数・Debug 出力を見逃していた。
  header の build では、同じ build の object が持つ library の IR（`Entry::Library`）を別に生成して検査する。
  `tests/allocator.mjs` は IO・タスク・Debug の拒否を `--emit object`・`llvm`・`header` の 3 つで確かめ、受理するプログラムの freestanding の header が
  `--allocator host` の header と byte 一致することも確かめる。
