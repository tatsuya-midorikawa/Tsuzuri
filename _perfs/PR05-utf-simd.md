# PR05: UTF 変換・比較・検証の SIMD 化

| 項目 | 内容 |
| --- | --- |
| ID | PR05 |
| 分類 | 実行速度 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | PX01, (F08) |
| 関連 | PM03, F05, F08 |
| 状態 | todo |
| 起票 | 2026-09-29。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 不要（Phase 1）。Phase 2 は F08 の完了と人間の指示の後だけ（D7） |
| 手本にする既存実装 | 16-bit・8-bit 列の一括比較: `src/runtime/string.ll` の `@tz.string.equal`・`@tz.string.compare` の `wide`／`wide_advance` 節（64-bit 読み出し）。target ごとの runtime 文書の選択: `src/llvm.rs` で `heap-wasm.ll` と `heap-native.ll` を `if wasm` で選ぶ箇所。runtime の単体検査: `tests/strings.mjs` の runtime harness（`--emit llvm` の IR と `tests/strings_runtime.c` を native と wasm32 の `-O0`／`-O3` で結合し、`all_scalars`・`runtime_trap_case` を呼ぶ） |
| 主な影響ファイル | `src/runtime/string.ll`, `src/runtime/utf8string.ll`, `src/runtime/string_scalar.ll`（新規）, `src/runtime/string_v128.ll`（新規）, `src/llvm.rs`, `src/driver.rs`, `tests/strings.mjs`, `tests/strings_runtime.c`, `tests/strings_simd.c`（新規）, `benchmarks/utf/Kernels.tz`（新規）, `benchmarks/utf/host.c`（新規）, `benchmarks/run-utf.mjs`（新規）, `docs/benchmarks.md`, `docs/architecture.md`。Phase 2 だけ: `src/runtime/cpu.c` |
| 計測対象 | `cpp/utf8_roundtrip`・`cpp/utf16_compare`・`cpp/utf16_validate`（改善）、`cpp/utf16_scan`（退行の確認）、`utf/*`（新規。入力 5 種 × 操作 6 種） |

## 目的

UTF-8 と UTF-16 の変換・検証・比較を、128-bit ベクトルで一度に 16 bytes ずつ処理し、simdutf などの SIMD 実装に近づける。
Tsuzuri の既定の `string` は UTF-16 で、ホストやファイルの UTF-8 との変換が頻繁に起こるため、変換の速度がデータ処理全体の速度を左右する。
結果（変換後の列、trap の有無、置換、比較の順序）は HEAD のスカラー実装とビット単位で同一にする。

実装者は Phase 1（target 非依存の 128-bit ベクトル IR による ASCII／非サロゲートの区間処理と比較）だけを実装する。
Phase 2（非 ASCII の区間のベクトル検証・復号と AVX2 版）は F08 の完了後、人間が求めた場合だけ着手する。

## 着手条件と停止条件

### 着手条件

- PX01 が `_perfs/README.md` の状態欄で done であること。確認: `grep -n "| PX01 \|| PR05 \|| PM03 " _perfs/README.md`。
- F08 は Phase 1 の着手条件にしない。Phase 1 は F08 の仕組みを使わない（D1・D7）。
- GUIDE §2.3 の基準コマンドと GUIDE §14 の性能チケットの共通手順を実行し、実装手順 1 のベースライン（基準のコンパイラ、IR、計測）を保存していること。

### 前提とする他チケットのインターフェース

- PX01（done が条件）: `node benchmarks/run-cpp.mjs <compiler> --metrics <dir>` が stdout の JSON を今と同じに出したうえで、
  PX01 形式（JSON Lines、`schema: 1`）のレコードを `<dir>/cpp.jsonl` に書く。`benchmarks/metrics.mjs` が
  `captureRun`・`makeRecord`・`writeRecords`・`readRecords`・`compareRuns`・`renderComparison` を export する。
  `writeRecords` は同じ run に同じ suite を二度書けない（`flag: "wx"`）。名前が違ったら PX01 の実装に合わせ、この節を直してから進む。
- F08（Phase 2 だけ）: F08 の「他チケットへの提供インターフェース」を使う。kernel は `src/runtime/cpu.c` に置き（別ファイルにしない）、
  level は `static int tz_cpu_level(void)`（`_Atomic int` に cache、`TSUZURI_CPU_FORCE` を反映）で決まり、variant の選択は
  `TZ_CPU_PICK(name, ...)`（`name##_avx2`・`name##_sse42`・`name##_baseline`）で行う。公開記号は `tsuzuri_cpu_<op>_<type>` で、
  その `declare` が IR にあると driver が `cpu.c` を native の runtime に加える。Rust 側は `src/llvm.rs` の `CPU_KERNELS` の表に
  行を足し、`cpu_kernel(function)` に arm を足す。PR05 Phase 2 は `tsuzuri_cpu_utf8_to_utf16`（新規）などをこの形で登録する。
  F08 の実装で名前が違えば、Phase 2 の着手前にこの節を直す。
- PM03（後続）: PR05 は `%tz.string = { ptr, i64 }`・`%tz.utf8string` の表現と runtime 関数の名前・引数を変えない。PM03 は
  `src/runtime/string_scalar.ll`・`src/runtime/string_v128.ll`（D1）の対に Latin-1 の版を足す前提で設計する。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- ベクトル版とスカラー版（または独立の参照）の結果が一つでも違い、その原因がスカラー版（HEAD）の誤りに見える。スカラー版の意味は変えない。
- Phase 1 のベクトル IR に target 固有の intrinsic（`llvm.aarch64.*`・`llvm.x86.*`・`llvm.wasm.*`）が要る、または
  汎用のベクトル演算が native か wasm32（simd128）で legalize できない。
- 既定の wasm32（`--wasm-feature` なし）の出力に `v128` の命令が現れる、または既定の WASM import が増える。
- driver から emitter へ「wasm32 で simd128 が有効か」を渡すのに、`bool` の欄を一つ足す以上の変更が要る。
- 文字列の処理に native の C runtime の compile（`cpu.c` など）が必要になった（D1 に反する）。
- guard page の検査（テスト計画 T4）が範囲外の読み出しを検出し、確保の大きさを増やす（詰め物を足す）以外に直せない。
- Unicode が混在する入力（`utf/*` の `mixed`・`cjk`・`emoji`）の中央値が、before の最小〜最大の範囲を超えて遅くなり、窓の規則（D3）の範囲で直せない。
  種目名や入力に依存する調整はしない。
- 既存テストの期待値（IR の文字列、trap の件数 `runtime_trap_count` の 28、`all_scalars` の 4344）を変える必要がある。
- `unsafe`、新しい crate、既定の WASM import が必要になった。

## 現状と計測（HEAD `f8dc655`）

### 対象の runtime 関数と呼び出し元

runtime はすべて LLVM IR の文書で、`src/llvm.rs` が出力に `@tz.string.`・`@tz.utf8string.`・`@tz.free`・`@tz.alloc`・`@tz.realloc` の
いずれかを含むとき `include_str!("runtime/string.ll")` と `include_str!("runtime/utf8string.ll")` を連結する。関数は `define internal`
で、使わない物は LLVM が消す。native の C runtime（`src/driver.rs` の `include_str!("runtime/cpu.c")`）は `cpu_runtime` などが
真のときだけ compile され、wasm32 には C runtime の compile がない。

| 関数 | 現在のアルゴリズム | 呼び出し元（grep で確認） | Phase 1 |
| --- | --- | --- | --- |
| `@tz.string.equal` | 長さが違えば false。残り 4 単位以上は `load i64`（align 2）で比較、残りは 1 単位ずつ | `src/llvm.rs` の `call i1 @{runtime}.equal(`（`==`）、真偽値の解析の `call i1 @tz.string.equal(` | 対象 |
| `@tz.utf8string.equal` | 同上（8-bit） | `call i1 @{runtime}.equal(` | 対象 |
| `@tz.string.compare` | 短い方の長さまで `load i64` で一致を探し、不一致の 4 単位の中を 1 単位ずつ。16-bit 符号なしで順序、共通接頭辞なら長さ | `src/llvm.rs` の `call i32 @{runtime}.compare(`、`src/llvm_bulk.rs` の `call i32 @{}.compare(` | 対象 |
| `@tz.utf8string.compare` | 同上（8-bit 符号なし） | 同上 | 対象 |
| `@tz.string.from_utf8` | 計数段: 1 scalar ずつ `@tz.string.decode_utf8`（不正なら trap）、長さが 2^53−1 を超えたら trap。全体が ASCII なら `@tz.string.from_ascii`、そうでなければ書き込み段で 1 scalar ずつ復号（`i1 true`） | `src/llvm.rs`（`call %tz.string @tz.string.from_utf8(`）、`src/llvm_display.rs` | 対象 |
| `@tz.utf8string.from_string` | 計数段: 1 単位ずつ `@tz.string.decode_utf16`、孤立サロゲートで trap。全体が ASCII なら `@tz.utf8string.from_ascii`、そうでなければ 1 scalar ずつ符号化 | `src/llvm.rs` の `call %tz.utf8string @tz.utf8string.from_string(`（2 箇所） | 対象 |
| `@tz.string.is_well_formed` | 1 単位ずつ `@tz.string.decode_utf16`、孤立サロゲートで false | `Builtin::StringIsWellFormed` | 対象 |
| `@tz.string.to_well_formed` | `@tz.string.new` で複製し、孤立サロゲートの位置へ `U+FFFD`（`store i16 -3`） | `Builtin::StringToWellFormed` | 対象 |
| `@tz.string.from_ascii`, `@tz.utf8string.from_ascii` | 1 単位ずつ zext／trunc | 上の 2 変換の全 ASCII 経路 | 対象 |
| `@tz.string.to_ascii` | 4096 単位以下の ASCII を 1 単位ずつ | `src/llvm.rs`（`call i1 @tz.string.to_ascii(ptr %buffer, ptr %data, i64 %length)`） | 対象外 |
| `@tz.string.try_decode_utf8`, `@tz.string.decode_utf8` | 1 scalar の復号 | `src/llvm_abi.rs` の `validate_host_utf8`、`src/llvm_bulk.rs` | 対象外（Phase 2） |

誤りの位置を返す API は HEAD にない。不正な UTF-8 と孤立サロゲートの変換は trap、修復は孤立サロゲート 1 単位を `U+FFFD` 1 単位に置き換える。
HEAD に部分文字列・1 単位の検索の runtime 関数はない（`grep -n "^define" src/runtime/string.ll src/runtime/utf8string.ll` で確認）。

### 計測済みの事実

- 2026-09-26 の相対性能: `cpp/utf8_roundtrip` 0.658（最速は C++）、`cpp/utf16_compare` 0.497（最速は JavaScript）。
  `utf16_scan` と `utf16_validate` は Tsuzuri が最速だった。
- docs/benchmarks.md の `### 数値・UTFの追加改善（2026-09-26）`: native の Tsuzuri 中央値は `utf16_compare` 0.017193 ms、
  `utf8_roundtrip` 0.142395 ms。64-bit 化の前後は UTF-16 比較 0.029318→0.017442 ms、UTF 往復 0.164931→0.144685 ms。
  通常サイズの UTF 変換で、ASCII 経路は 0.9275→0.7125 ms、混在 Unicode は 1.7395→1.7435 ms だった。
- ASCII を各位置で先読みする試作は、Unicode が混在する入力を遅くしたため破棄された（D3 の窓の規則はこの失敗を避ける）。
- 種目の入力（`benchmarks/cpp/Kernels.tz` の `make_text`）: seed が偶数なら `"Az09-_ \n"`（ASCII 8 単位）、奇数なら
  `"A\0\u03A9\uD83D\uDE00\u4E2Dz\n"`（8 単位に 2・3・4 bytes の UTF-8 を含む）を、要求長以上になるまで倍にする。size 26,215 の実際の長さは 32,768 単位。
  `utf16_compare` は末尾 1 単位だけ違う 2 文字列に `==` と `<` を行い、`utf8_roundtrip` は `Utf8String.from_string`・`String.from_utf8`・`==` を行う。
- 実行時の CPU 選択は `Array.sum<i64>` だけ（F05。`src/runtime/cpu.c` の `tsuzuri_cpu_sum_i64`）。

### 再現

計測の再現（時間は計測手順に従う。ここは形式の確認）。`benchmarks/run-cpp.mjs` の usage は
`[compiler] [--quick] [--scale number] [--cpu generic|native] [--artifacts directory]`。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
mkdir -p /tmp/tz-work-PR05
node benchmarks/run-cpp.mjs target/release/tsuzuri --quick > /tmp/tz-work-PR05/cpp-quick.json
grep -o '"name":"utf[^}]*' /tmp/tz-work-PR05/cpp-quick.json
```

runtime の現在の形（手順 1 で保存する）:

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
grep -n "^define" src/runtime/string.ll src/runtime/utf8string.ll
grep -n "include_str!(\"runtime/string.ll\")\|include_str!(\"runtime/cpu.c\")" src/llvm.rs src/driver.rs
```

## 目標と指標

目標は CI の合否条件にしない。専用の計測機で before と after を記録して判断する（GUIDE §14）。

- G1: ASCII の区間と非サロゲートの区間を 16 bytes 単位で処理し、ASCII の多い入力の変換・検証・比較を速くする。
- G2: 一致検査と辞書順を 16 bytes 単位のベクトル比較にし、`cpp/utf16_compare` で JavaScript と同等以上にする。
- G3: Unicode が混在する入力と 8 単位の短い文字列を遅くしない。
- G4（Phase 2 を含めた最終目標）: `cpp/utf8_roundtrip` で C++ の最速と同等以上。Phase 1 だけでは混在 Unicode の seed で届かない見込みで、
  届かなければ結果をそのまま記録し、Phase 2 の判断材料にする。

| 指標 | 単位・統計 | 対象 | 期待（計測で確かめる） |
| --- | --- | --- | --- |
| M1 種目の時間 | ms（1 呼び出し。9 回の実行の中央値・最小・最大。各実行の値は runner が出す 12 標本の中央値） | `cpp/utf8_roundtrip`・`cpp/utf16_compare`・`cpp/utf16_validate`・`cpp/utf16_scan`（既定の大きさ） | compare・validate は短縮、roundtrip は ASCII の seed で短縮、scan は変化なし |
| M2 入力別の処理量 | ns／UTF-16 単位（9 回の中央値・最小・最大） | `utf/<op>/<kind>/<size>`（新規。op 6 種、kind 5 種、size 8・64・65,536 単位） | kind `ascii`・`latin` の size 65,536 で短縮。`mixed`・`cjk`・`emoji` と size 8 は変化なし |
| M3 確保 | 回・bytes（PX01 の `alloc_calls`・`alloc_bytes`） | M1 の 4 種目 | before と同一 |
| M4 成果物の大きさ | bytes（PX01 の成果物サイズ） | `tests/fixtures/strings` の native object（`-O3`）、wasm32 の既定と simd128（`-O3`） | 記録だけ。既定の wasm32 は関数の並びの変化以外に差がない |

- op: `to_utf8`（`Utf8String.from_string`）、`from_utf8`（`String.from_utf8`）、`equal`（`==`、同じ内容の 2 文字列）、
  `compare`（`<`、末尾 1 単位だけ違う 2 文字列）、`is_well_formed`、`to_well_formed`。検証の 2 op は末尾に孤立サロゲート `U+D800` を 1 単位足した入力を使う（不正な列を含む入力の計測）。
  不正な UTF-8 の変換は trap するので時間を測らない。
- kind（8 単位の型を size まで倍にする）: `ascii` `"Az09-_ \n"`、`latin` `"A\u00E90\u00FC-\u00F1 \n"`（2 bytes の UTF-8 を 3 個）、
  `cjk` `"\u4E2D\u6587\u5B57\u65E5\u672C\u8A9E\u6F22\u5B57"`（すべて 3 bytes）、`emoji` `"\uD83D\uDE00\uD83D\uDE01\uD83D\uDE02\uD83D\uDE03"`（4 bytes の 4 scalar）、
  `mixed` `"A\0\u03A9\uD83D\uDE00\u4E2Dz\n"`（`benchmarks/cpp/Kernels.tz` の奇数 seed と同じ）。

## 変えてはいけない意味

- 変換結果の列: UTF-8→UTF-16 と UTF-16→UTF-8 の出力の全単位・全 byte、結果の長さ、`%tz.string`・`%tz.utf8string` の表現。
- trap の条件: 不正な UTF-8（先頭 `0x80`〜`0xC1`・`0xF5`〜`0xFF`、途中の非継続 byte、途中で切れた列、冗長な符号化、`U+D800`〜`U+DFFF` の符号化、
  `U+10FFFF` 超）と、UTF-16→UTF-8 の孤立サロゲート。長さの上限（UTF-8 入力 27,021,597,764,222,973 bytes、UTF-16 の結果と入力 2^53−1 単位）。
  trap は `llvm.trap`（native は SIGILL か SIGTRAP、WASM は `unreachable`）で、範囲外アクセスによる trap にしない（`tests/strings.mjs` が `/unreachable/` を検査する）。
- trap の時点: 変換は結果を確保する前に入力全体を検証し終える（HEAD の計数段と同じ）。
- 修復と判定: `String.to_well_formed` は孤立サロゲート 1 単位ごとに `U+FFFD` 1 単位を置き、ほかの単位（正しい対を含む）はそのまま。
  `String.is_well_formed` は孤立サロゲートの有無だけで決まる。
- 比較: `==` は長さと全単位の一致。`<` などの順序は、最初に違う単位の符号なし比較（UTF-16 は 16-bit、UTF-8 は 8-bit）、共通接頭辞なら短い方が小さい。
- メモリ: 読み出しは `[data, data + 長さ)` の中だけ、書き込みは確保した結果の中だけ。確保の回数と大きさは HEAD と同一。全 suite で `live == 0`。
- 決定的な IR: 選ぶ runtime 文書は `(wasm, wasm_simd)` だけで決まる。runtime に intrinsic の `declare` を足さない（重複宣言を作らない）。
- WASM: 既定の wasm32 に `v128` の命令を出さず、既定・simd128 のどちらも import を増やさない（D-18）。
- native の `--cpu generic`: aarch64 の NEON と x86_64 の SSE2 は各 ABI の必須機能で、Phase 1 はそれ以外の命令を出さない。
- 変えない関数: `@tz.string.to_ascii`・`@tz.string.decode_utf8`・`@tz.string.try_decode_utf8`・`@tz.string.decode_utf16` と、その呼び出し元（`validate_host_utf8` など）。

## 設計

### 置き場所と選択（D1・D2）

- 対象の 10 関数（`@tz.string.from_ascii`・`@tz.utf8string.from_ascii`・`@tz.string.equal`・`@tz.utf8string.equal`・`@tz.string.compare`・
  `@tz.utf8string.compare`・`@tz.string.from_utf8`・`@tz.utf8string.from_string`・`@tz.string.is_well_formed`・`@tz.string.to_well_formed`）を
  `src/runtime/string.ll` と `src/runtime/utf8string.ll` から、同じ名前・引数の 2 つの文書へ移す。
  - `src/runtime/string_scalar.ll`（新規）: HEAD の本文をそのまま移す。変えるのは D4 の計数関数の切り出しだけ。
  - `src/runtime/string_v128.ll`（新規）: target 非依存の 128-bit ベクトル IR（`<16 x i8>`・`<8 x i16>`）による版。
- 両文書に計数関数 `@tz.string.utf8_to_utf16_length(ptr, i64) -> i64`（新規）と `@tz.utf8string.utf16_to_utf8_length(ptr, i64) -> i64`（新規）を置く（D4）。
- `src/llvm.rs` の `emit_program` は `string.ll`・`utf8string.ll` を連結する同じ `if` の中で、`!wasm || instrumentation.wasm_simd` なら
  `string_v128.ll`、そうでなければ `string_scalar.ll` を連結する。
- driver は `src/driver.rs` の `llvm::EmitOptions { .. }` を作る箇所で `wasm_simd: options.target == Target::Wasm32 && options.wasm_simd` を渡す。
  `BuildOptions::wasm_simd` は HEAD にあり、今は IR の先頭に `; wasm-feature: simd128; compile this IR with -msimd128` を足すだけに使われている。

### データ構造

```rust
// src/llvm.rs
pub struct EmitOptions {
    pub entry: Entry,
    pub wasm: bool,
    pub wasm_simd: bool, // 新規。wasm32 で simd128 が有効なとき true。native では無視する
    pub debug_output: bool,
}

#[derive(Default)]
struct Instrumentation<'a> {
    traps: bool,
    debug: Option<(&'a [TrapSource<'a>], bool)>,
    cpu_dispatch: bool,
    wasm_threads: bool,
    wasm_simd: bool, // 新規。EmitOptions::wasm_simd の写し
}
```

`emit_target` と `emit_selected` の既定は `wasm_simd: false`。`emit_with_options`・`emit_with_trap_info`・`emit_with_debug_info`・
`emit_wasm_threads_build` は `options.wasm_simd` を `Instrumentation` へ写す（`emit_selected` は引数を一つ足す）。`emit_native_build` は `false`。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| runtime | `src/runtime/string.ll` | `@tz.string.from_ascii`, `@tz.utf8string.from_ascii`, `@tz.string.equal`, `@tz.string.compare`, `@tz.string.from_utf8`, `@tz.utf8string.from_string`, `@tz.string.is_well_formed`, `@tz.string.to_well_formed` | 削除して `string_scalar.ll` へ移す。`copy`・`allocate`・`new`・`concat`・`decode_utf8`・`try_decode_utf8`・`decode_utf16`・`to_ascii` は残す |
| runtime | `src/runtime/utf8string.ll` | `@tz.utf8string.equal`, `@tz.utf8string.compare` | 削除して `string_scalar.ll` へ移す。`copy`・`allocate`・`new`・`concat` は残す |
| runtime | `src/runtime/string_scalar.ll`（新規） | 10 関数と計数関数 2 つ | HEAD の本文。`from_utf8`・`from_string` の計数段を計数関数へ移し、`-1` なら `llvm.trap` |
| runtime | `src/runtime/string_v128.ll`（新規） | 同じ 12 関数 | 「アルゴリズム」の V1〜V7 |
| 生成 | `src/llvm.rs` | `EmitOptions`, `Instrumentation`, `emit_target`, `emit_with_options`, `emit_selected`, `emit_with_trap_info`, `emit_with_debug_info`, `emit_wasm_threads_build`, `emit_native_build` | `wasm_simd` の受け渡し |
| 生成 | `src/llvm.rs` | `emit_program` の runtime 連結 | 対の文書の選択（上記） |
| driver | `src/driver.rs` | `llvm::EmitOptions { .. }` を作る箇所 | `wasm_simd` を渡す。IR 先頭の注釈はそのまま |
| テスト | `tests/strings.mjs`, `tests/strings_simd.c`（新規） | runtime harness | 差分検査と境界の検査（テスト計画） |
| 計測 | `benchmarks/utf/Kernels.tz`（新規）, `benchmarks/utf/host.c`（新規）, `benchmarks/run-utf.mjs`（新規） | M2 | 計測手順 |

`EmitOptions` を作る箇所は HEAD で `src/driver.rs` と `src/llvm.rs` の `emit_target` の 2 つ（`grep -rn "EmitOptions {" src tests` で再確認し、tests にあれば `wasm_simd: false` を足す）。

### 生成 IR とランタイム

- 型と演算は汎用のベクトル IR だけを使う: `load <16 x i8>, ptr %p, align 1`、`load <8 x i16>, ptr %p, align 2`、`xor`・`or`・`and`、
  `icmp eq <8 x i16>`、`zext <16 x i8> to <16 x i16>`、`trunc <16 x i16> to <16 x i8>`、`store`。
- 全 lane の判定は intrinsic を使わず `bitcast <16 x i8> to i128`（または `<8 x i16>`）と `icmp eq i128 %x, 0`、mask は
  `bitcast <8 x i1> to i8` と `icmp eq i8 %m, 0` で行う。`declare` を増やさないので、`llvm_simd.rs` や `math.ll` の宣言と衝突しない。
- LLVM はこの形を aarch64 で `ldr q`／`umaxv` など、x86_64 で `movdqu`／`pcmpeqb`／`pmovmskb` など、wasm32 simd128 で `v128.load`／`v128.any_true` などへ下ろす
  （生成コードの確認で確かめる）。
- 読み出しは常に `残り >= 幅` を確かめてから行う。確保の端を越える読み出し・書き込みはしない。
- 既定の wasm32 は `string_scalar.ll` だけを持つので、ベクトル型は出力に現れない。

### アルゴリズム

窓の規則（D3）: ベクトルの判定（probe）が失敗したら、その窓の先頭 + 幅に達するまで HEAD と同じ 1 scalar ずつの処理を続け、
それから次の probe を行う。scalar の処理は窓の端を越えてよい（最大 3 bytes または 1 単位）。次の probe は越えた位置から、揃えずに行う。

- V1 `equal`: 長さが違えば false。残り 32 bytes 以上の間、両側から 16 bytes を 2 本ずつ読み、`(a0 ^ b0) | (a1 ^ b1)` が 0 でなければ false。
  残りは HEAD の 64-bit の段と 1 単位の段。
- V2 `compare`: `n = min(an, bn)`。残り 16 bytes 以上の間、16 bytes を比べ、違えばその位置 `i` から HEAD の `loop`（64-bit の段、次に 1 単位の段）へ入る。
  違いは次の 16 bytes 以内にあるので、HEAD の段が必ず `different` に着く。一致したまま終われば HEAD の `prefix`。
- V3 `@tz.string.utf8_to_utf16_length`: 入力が 27,021,597,764,222,973 bytes を超えれば -1。残り 16 bytes 以上なら 16 bytes を読み、
  `and` の splat `0x80` が 0 なら長さ += 16、位置 += 16。そうでなければ窓の規則で `@tz.string.try_decode_utf8(..., i1 false)` を繰り返し、
  値が負なら -1、`U+10000` 以上なら長さ += 2、それ以外は += 1。各加算の後、長さが 2^53−1 を超えれば -1。
- V4 `@tz.string.from_utf8`: `len = V3`。負なら `llvm.trap`。`len == byte_length` なら `@tz.string.from_ascii`。そうでなければ確保し、
  残り 16 bytes 以上の窓が ASCII なら `zext` して 32 bytes を書く。そうでなければ窓の規則で `@tz.string.decode_utf8(..., i1 true)` と HEAD の書き込み。
- V5 `@tz.utf8string.utf16_to_utf8_length` と `@tz.utf8string.from_string`: 入力が 2^53−1 単位を超えれば -1（呼び出し側で trap）。8 単位の窓で
  `and` の splat `0xFF80` が 0 なら ASCII（長さ += 8、書き込みは `trunc <8 x i16> to <8 x i8>`）。そうでなければ窓の規則で
  `@tz.string.decode_utf16(..., i1 false)`、サロゲートの値なら -1。結果の長さの上限の検査は HEAD と同じく行わない。
- V6 `is_well_formed`・`to_well_formed`: 8 単位の窓で `(v & 0xF800) == 0xD800` の mask が 0 なら位置 += 8。そうでなければ窓の規則で HEAD の判定・置換。
  `to_well_formed` は HEAD と同じく先に `@tz.string.new` で全体を複製する。
- V7 `from_ascii` 2 つ: 16 単位ずつ `zext`／`trunc` し、残りを 1 単位ずつ。

窓の規則の正しさ: probe が成功した窓の単位はすべて ASCII（または非サロゲート）なので、1 scalar ずつ処理した結果と同じである。
probe の開始位置は常に scalar の境界（前の scalar の直後）なので、窓の先頭が継続 byte や対の後半になることはない。

### Phase 分割

- Phase 1（このチケットで実装する）: D1〜D6、V1〜V7、テスト、`utf/*` の計測、文書。
- Phase 2（D7。F08 の完了と人間の指示の後だけ）: 非 ASCII の窓のベクトル検証（Keiser・Lemire の 3 表の lookup。前の窓との連結は `shufflevector`）、
  BMP だけの窓のベクトル符号化、F08 の `cpu.c` の仕組みによる AVX2（32 bytes）版、`validate_host_utf8` への適用。lookup は byte の shuffle
  （NEON `tbl`、SSSE3 `pshufb`、wasm `i8x16.swizzle`）が要り、target 固有の IR か C の kernel になる。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので `running N tests` を必ず見る。

### 手順 1: ベースライン

- 変更: なし。
- 内容: 基準のコンパイラと IR を保存する。
- 確認: 次が成功し、`tests/strings.mjs` が `strings runtime O0: all Unicode scalars in 4344 bounded batches and 28 strict ...` と O3 の同じ行を出す。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
mkdir -p target/perf/PR05 /tmp/tz-work-PR05
cp target/release/tsuzuri target/perf/PR05/baseline-tsuzuri
node tests/strings.mjs target/release/tsuzuri
target/release/tsuzuri build tests/fixtures/strings --emit llvm -o /tmp/tz-work-PR05/before-native.ll
target/release/tsuzuri build tests/fixtures/strings --target wasm32 --emit llvm -o /tmp/tz-work-PR05/before-wasm.ll
```

### 手順 2: 計測 runner（D6）

- 変更: `benchmarks/utf/Kernels.tz`（新規）, `benchmarks/utf/host.c`（新規）, `benchmarks/run-utf.mjs`（新規）。
- 内容: `Kernels.tz` は `benchmarks/cpp/Kernels.tz` の先頭（`clone_string` を含む）を写し、下の `pattern`・`utf_text` と 6 つの export
  （`utf_to_utf8`・`utf_from_utf8`・`utf_equal`・`utf_compare`・`utf_is_well_formed`・`utf_to_well_formed`。どれも `kind size repeat -> i64`）を持つ。
  文字列は export の中で 1 回だけ作り、`repeat` 回の操作の結果の合計を返す。`host.c` は `benchmarks/run-dispatch.mjs` の host の形
  （`clock_gettime(CLOCK_MONOTONIC)`、9 標本、実装順の回転、挿入整列で中央値）で、期待値を C で独立に計算して `assert` する（例: `utf_to_utf8` は
  `repeat × size / 8 × 型の UTF-8 bytes`。型の bytes は ascii 8、latin 11、cjk 24、emoji 16、mixed 13）。`repeat` は `size × repeat` が約 2^24 単位になる値。
  `run-utf.mjs` は `run-dispatch.mjs` を写し、`build benchmarks/utf/Kernels.tz --emit object -O3` と host を結合し、1 workload 1 行の JSON
  （`op`・`kind`・`size`・`repeat`・`ns_per_unit`（中央値）・`min`・`max`）を出す。`--quick`（size 64 だけ、1 標本、正しさの確認）と
  `--metrics <dir>`（PX01 の `makeRecord`・`writeRecords`、suite `utf`）を受ける。
- 下の 2 関数と、同じ形の `utf_to_utf8`・`utf_compare` は、`benchmarks/cpp/Kernels.tz` の先頭と連結して `target/release/tsuzuri check` と `build --emit object`（native）・`--target wasm32` で 2026-09-29 に確認した。

```tsuzuri
def pattern :: i64 -> string
fn pattern kind =
    if kind == 0 then "Az09-_ \n"
    elif kind == 1 then "A\u00E90\u00FC-\u00F1 \n"
    elif kind == 2 then "\u4E2D\u6587\u5B57\u65E5\u672C\u8A9E\u6F22\u5B57"
    elif kind == 3 then "\uD83D\uDE00\uD83D\uDE01\uD83D\uDE02\uD83D\uDE03"
    else "A\0\u03A9\uD83D\uDE00\u4E2Dz\n"

def utf_text :: i64 -> i64 -> string
fn utf_text kind size =
    let mut text = pattern kind
    while text.length < size do
        let copy = clone_string ref text
        text = copy + text
    text
```

- 確認: `node benchmarks/run-utf.mjs target/release/tsuzuri --quick` が 30 行を出して終了コード 0。`target/perf/PR05/baseline-tsuzuri` でも同じ。

### 手順 3: 文書の分割（IR の意味は変えない）

- 変更: `src/runtime/string.ll`, `src/runtime/utf8string.ll`, `src/runtime/string_scalar.ll`（新規）, `src/llvm.rs`（`EmitOptions`・`Instrumentation`・各 `emit_*`・`emit_program`）, `src/driver.rs`。
- 内容: 設計「段ごとの変更」の runtime と生成の行。`string_v128.ll` はまだ作らず、全 target で `string_scalar.ll` を連結する。`wasm_simd` の受け渡しだけ入れる。
- 確認: `cargo test --locked --test strings` が `13 passed`。`node tests/strings.mjs target/release/tsuzuri` が成功。行の多重集合が変わらない（`diff` の出力なし）:

```sh
target/release/tsuzuri build tests/fixtures/strings --emit llvm -o /tmp/tz-work-PR05/step3.ll
diff <(sort /tmp/tz-work-PR05/before-native.ll) <(sort /tmp/tz-work-PR05/step3.ll)
```

### 手順 4: 計数関数の切り出し（D4）

- 変更: `src/runtime/string_scalar.ll`。
- 内容: `@tz.string.utf8_to_utf16_length`・`@tz.utf8string.utf16_to_utf8_length` を作り、`from_utf8`・`from_string` は結果が負なら `llvm.trap`。
  計数関数は `try_decode_utf8`・`decode_utf16` の `i1 false` を使い、trap しない。
- 確認: `node tests/strings.mjs target/release/tsuzuri` が成功（trap 28 件は変わらない）。`cargo test --locked --test strings` が `13 passed`。

### 手順 5: 差分 harness（ベクトル版の前に作る）

- 変更: `tests/strings_simd.c`（新規）, `tests/strings.mjs`。
- 内容: テスト計画の「共通の源」と T1〜T7。この時点では両側がスカラー版なので全件一致する。harness 自体の誤りをここで除く。
- 確認: `node tests/strings.mjs target/release/tsuzuri` が 6 構成分の新しい行（例: `strings simd native O0: ...`）を出して成功。

### 手順 6: `string_v128.ll` と比較（V1・V2）

- 変更: `src/runtime/string_v128.ll`（新規。最初は `string_scalar.ll` の写し）, `src/llvm.rs` の選択（D2）, `tests/strings.rs`。
- 内容: `equal`・`compare` の 4 関数をベクトル版にし、Rust テストを足す。
- 確認: `node tests/strings.mjs target/release/tsuzuri` が成功し、simd128 構成の逆アセンブルに `v128.` がある（harness が検査）。`cargo test --locked --test strings` が `14 passed`。

### 手順 7: 変換と検証（V3〜V7）

- 変更: `src/runtime/string_v128.ll`。
- 内容: V7、V3・V4、V5、V6 の順に一つずつ入れ、その都度確認する。
- 確認: 毎回 `node tests/strings.mjs target/release/tsuzuri` が成功。最後に emit した native と simd128 の IR を
  `/opt/homebrew/opt/llvm@21/bin/clang -O0 -c`（simd128 は `--target=wasm32-unknown-unknown -msimd128`）で compile でき、verifier の誤りがない。

### 手順 8: 全体の確認

- 確認: `cargo test --locked` が成功。`cargo build --release --locked` の後、`node tests/strings.mjs`・`node tests/simd.mjs`・`node tests/cpu_dispatch.mjs`・
  `node tests/features.mjs` を `target/release/tsuzuri` で実行して成功（V8 の異常終了は `npx --yes --package=node@24 node ...`）。

### 手順 9: 生成コードの確認・計測・文書

- 内容: 「生成コードの確認」「計測手順」「ドキュメント」を行う。混在 Unicode の退行が出たら停止条件に従う。
- 確認: `docs/benchmarks.md` に PR05 の節があり、`git diff --check` が空。

## 計測手順

GUIDE §14 に従う。生データは `target/perf/`（コミットしない）。before は `target/perf/PR05/baseline-tsuzuri`、after は `target/release/tsuzuri`。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
{ git rev-parse HEAD; git diff --stat; sysctl -n machdep.cpu.brand_string; sw_vers; clang --version | head -n 1; rustc --version; node --version; } > target/perf/PR05/env.txt
node benchmarks/run-cpp.mjs target/release/tsuzuri --quick > /dev/null && node benchmarks/run-utf.mjs target/release/tsuzuri --quick > /dev/null
for trial in 1 2 3 4 5 6 7 8 9; do
  node benchmarks/run-cpp.mjs target/perf/PR05/baseline-tsuzuri --metrics target/perf/PR05-before-$trial > target/perf/PR05/cpp-before-$trial.json || break
  node benchmarks/run-cpp.mjs target/release/tsuzuri --metrics target/perf/PR05-after-$trial > target/perf/PR05/cpp-after-$trial.json || break
  node benchmarks/run-utf.mjs target/perf/PR05/baseline-tsuzuri --metrics target/perf/PR05-before-$trial > target/perf/PR05/utf-before-$trial.json || break
  node benchmarks/run-utf.mjs target/release/tsuzuri --metrics target/perf/PR05-after-$trial > target/perf/PR05/utf-after-$trial.json || break
done
```

- ウォームアップは上の `--quick` の 1 回（時間は捨てる）。計測中はほかの重い処理を止め、電源に接続する。
- M1・M2: 9 回の中央値・最小・最大を before と after で表にする（PX01 の `readRecords`・`compareRuns`・`renderComparison`）。
- M3: PX01 の確保の計数が `run-cpp.mjs` にあれば 4 種目を 1 回ずつ記録する。なければテスト T6（確保の一致）の結果で代え、その旨を書く。
- M4: `tests/fixtures/strings` を before と after で native object、wasm32、wasm32 simd128（すべて `-O3`）に build し、`wc -c` を記録する。
- 記録: docs/benchmarks.md に日付、計測機、before と after のコミット、Clang・Node の版、M1〜M4 の表、生データの場所、そろえられなかった条件を書く。
  x86_64 は cross compile の命令確認だけで、速度は主張しない。

## 生成コードの確認

`--emit llvm` は最適化前の IR なので、命令は object を逆アセンブルして見る。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
D=/tmp/tz-work-PR05/asm; mkdir -p $D; OBJDUMP=/opt/homebrew/opt/llvm@21/bin/llvm-objdump
for c in target/perf/PR05/baseline-tsuzuri target/release/tsuzuri; do n=$(basename $c)
  $c build benchmarks/utf/Kernels.tz --emit object -O3 -o $D/$n.o
  $c build benchmarks/utf/Kernels.tz --target wasm32 -O3 -o $D/$n.wasm
  $c build benchmarks/utf/Kernels.tz --target wasm32 --wasm-feature simd128 -O3 -o $D/$n-simd.wasm
done
target/release/tsuzuri build benchmarks/utf/Kernels.tz --emit llvm -O3 -o $D/utf.ll
/opt/homebrew/opt/llvm@21/bin/clang --target=x86_64-apple-macos11 -O3 -c $D/utf.ll -o $D/utf-x86.o
$OBJDUMP -d --no-show-raw-insn --disassemble-symbols=_tz_utf_equal,_tz_utf_compare,_tz_utf_from_utf8,_tz_utf_to_utf8 $D/tsuzuri.o
```

| 対象 | after で見る物 | 見てはいけない物 |
| --- | --- | --- |
| native aarch64（`$D/tsuzuri.o`） | 4 記号に `ldr q` か `ldp q`。`_tz_utf_equal`・`_tz_utf_compare` に 16 bytes の `eor`、`_tz_utf_from_utf8` に `uxtl` か `ushll`、`_tz_utf_to_utf8` に `xtn` か `uzp1` | なし（before の同じ記号の数と並べて記録する） |
| x86_64（`$D/utf-x86.o`） | `movdqu`、`pcmpeqb` か `pcmpeqw`、`pmovmskb` か `ptest` | `ymm`（AVX）。`grep -c ymm` が 0 |
| wasm32 既定（`$D/tsuzuri.wasm`） | なし | `tests/simd.mjs` と同じ正規表現 `\b(?:v128\|i8x16\|i16x8\|i32x4\|i64x2\|f32x4\|f64x2)\.` に一致する行（0 件。HEAD の同じ形の module でも 0 件を 2026-09-29 に確認） |
| wasm32 simd128（`$D/tsuzuri-simd.wasm`） | `v128.load` と、`v128.any_true` か `i8x16.bitmask`・`i16x8.bitmask` | import（`WebAssembly.Module.imports` が空でない） |
| IR（`$D/utf.ll`） | `<16 x i8>`・`<8 x i16>` がある。`^declare` の行の集合が before と同じ | `splat (` の短縮構文、`llvm.aarch64.`・`llvm.x86.`・`llvm.wasm.` |

## テスト計画

### 共通の源

- 1 つの module に「選ばれた runtime」と「参照」を同居させる。参照は `src/runtime/string_scalar.ll` の文書で、定義する 12 関数の名前だけ
  `@tz.` を `@ref.` に置き換える（`@<名前>(` の完全一致だけ。共通の `@tz.string.allocate` などは置き換えない）。`tests/strings.mjs` が
  `--emit llvm` の出力に参照と bridge（`@test_v_*`・`@test_r_*`（新規））を連結し、`tests/strings_simd.c` と結合する（既存の `runtimeBridges` と同じ方法）。
- 構成は 6 つ: native `-O0`／`-O3`（ベクトル版）、wasm32 既定 `-O0`／`-O3`（スカラー版同士。Node の参照が主）、wasm32 simd128 `-O0`／`-O3`
  （`--wasm-feature simd128` の IR を clang の `-msimd128` で compile。ベクトル版）。
- 独立の参照（wasm の 4 構成で JS から呼ぶ）: `new TextDecoder("utf-8", { fatal: true, ignoreBOM: true })`、`TextEncoder`（`isWellFormed()` が真のときだけ）、
  `String.prototype.isWellFormed`・`toWellFormed`、UTF-16 の順序は JS の `<`・`===`、UTF-8 の順序は `Buffer.compare`。

### 境界と網羅

- T1（native `-O3` と simd128 `-O3` だけ）: 長さ 1〜3 の全 byte 列（16,843,008 通り）を、ASCII の枠 32 bytes の位置 0 と 13 に置き、計数関数の値（-1 を含む）と、正しい列なら変換結果の全単位を参照と比べる。
- T2（全構成）: 先頭 byte 0x00〜0xFF と、後続の各位置に {0x00, 0x7F, 0x80, 0x8F, 0x90, 0x9F, 0xA0, 0xBF, 0xC0, 0xFF} を置いた長さ 1〜4 の列（284,416 通り）を、
  48 bytes の枠の位置 0〜16 に置いて参照と比べる。wasm では位置 0・13・15・16 を `TextDecoder` とも比べる。長さ 1〜3 の列は長さ 4 の列の途中切れを兼ねる。
- T3（全構成）: data の開始位置（UTF-8 は 0〜15 bytes、UTF-16 は 0〜14 の偶数 bytes）と長さ 0〜257 の全組で、全 ASCII、位置 p（全 p）に非 ASCII 1 個、
  位置 p に孤立サロゲート、(p, p+1) に対。`equal`・`compare` は位置 p だけ違う 2 列（UTF-16 の値の組 (0x41, 0x42)・(0x7FFF, 0x8000)・(0xFFFF, 0x0000)・(0xD800, 0xE000)、
  UTF-8 の (0x7F, 0x80)・(0xFF, 0x00)）と、長さが 1 違う接頭辞の組。
- T4（範囲外の読み出し）: native は `mmap` した 2 page の後ろを `mprotect(PROT_NONE)` にし、入力の末尾を page の端に合わせて長さ 0〜257 で 12 関数を呼ぶ（SIGSEGV で失敗）。
  wasm は `__builtin_wasm_memory_grow` で足した最後の page の末尾に入力を置き、確保しない関数（`equal`・`compare`・計数関数・`is_well_formed`）を呼ぶ。
- T5（全構成）: 2048 個のサロゲート値それぞれを 40 単位の ASCII の枠の位置 0〜31 に置き、`is_well_formed`・`to_well_formed`・`utf16_to_utf8_length` を参照と JS と比べる。
  窓の境界（位置 7・15・23）をまたぐ対も入れる。
- T6（全構成）: T3 の入力で、ベクトル版と参照の確保の回数・bytes（`@malloc` を `@tracked_alloc` に置き換える既存の方法）が一致し、最後に `live == 0`。
- T7: 既存の `all_scalars`（4344）と trap 28 件を simd128 の 2 構成にも広げる。

### Rust テスト

- `tests/strings.rs` に `string_runtime_uses_vectors_only_when_the_target_has_them`（新規）: `String.from_utf8` を使う program を `llvm::emit_with_options` で
  native・wasm32・wasm32 simd128 の 3 通りに emit し、`<16 x i8>` の有無（あり・なし・あり）と `^declare` の行に重複がないこと、2 回の emit が同一であることを確かめる。

### 既存テストへの影響

なし。runtime 関数の並びは変わるが、IR の文字列を比べる既存の検査（`src/llvm.rs` のテストの `call %tz.string @tz.string.from_utf8(` など）は呼び出しだけを見る。

### 性能

合否の閾値は置かない。M1〜M4 は計測手順で記録する。

## ドキュメント

- docs/benchmarks.md: `### 数値・UTFの追加改善（2026-09-26）` の後に `### UTF の SIMD 化（PR05）`（新規）。計測手順の記録と `run-utf.mjs` の再現コマンド。
- docs/architecture.md の `## 性能設計の原則`: 文字列 runtime の 2 文書と選択規則（native は常に 128-bit 版、wasm32 は simd128 のときだけ）、窓の規則、Phase 1 は実行時の CPU 選択を使わないこと。
- `_perfs/README.md` の PR05 の状態欄を done にし、Phase 2 が未着手であることを書く。

## 受け入れ条件

- [ ] T1〜T7 が全構成で成功し、`all_scalars` 4344 と trap 28 件が変わらない。
- [ ] 既定の wasm32 に `v128` 系の命令がなく、simd128 と native にベクトル命令がある（生成コードの確認の表）。import は増えない。
- [ ] `cargo test --locked`、`tests/strings.mjs`・`tests/simd.mjs`・`tests/cpu_dispatch.mjs`・`tests/features.mjs` が成功する。
- [ ] M1〜M4 を 9 回の中央値・最小・最大で記録し、混在 Unicode と size 8 の退行がない（退行があれば停止条件で報告済み）。
- [ ] runtime に intrinsic の `declare` と target 固有の intrinsic がなく、IR が決定的である。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- ASan は `sanitize_address` 属性のない `.ll` の関数を計装しないので、runtime IR の範囲外読み出しを見逃す。T4 の guard page で検査する。
- vector の `load` に `align 16` を書くと、x86 で `movdqa` になり未整列の data で落ちる。UTF-8 は `align 1`、UTF-16 は `align 2` だけを使う。
- wasm の harness を `-msimd128` なしで compile すると、LLVM はベクトル IR を黙ってスカラーに下ろし、テストは通るが SIMD を検査しない。harness は逆アセンブルの `v128.` を確かめる。
- probe を 1 scalar ごとに行うと、混在 Unicode が遅くなる（破棄された試作と同じ）。窓の規則（D3）を守る。
- HEAD の `compare` の `scalar_loop` などは phi の入口が複数ある。ブロックを足したら phi の対応を直し、手順 7 の `clang -O0 -c` で verifier を通す。
- `splat (i8 -128)` の短縮定数は古い Clang で読めない。定数ベクトルは `<i8 -128, i8 -128, ...>` と全 lane を書く。
- `TextDecoder` の既定は先頭の BOM を消すので `ignoreBOM: true` が要る。`TextEncoder` は孤立サロゲートを `U+FFFD` にするので、trap の期待値は `isWellFormed()` で決める。
- 参照の名前の置き換えで、共通関数や別の関数名の一部を巻き込まない（完全一致の `@名前(` だけ）。
- `bitcast <8 x i1> to i8` の bit 順に依存しない（0 との比較だけに使う）。最初の不一致の位置は HEAD のスカラー段で求める。

## 対象外

- 文字列の内部表現の変更（PM03）、正規化・書記素の処理（D09）。
- `@tz.string.to_ascii`、`validate_host_utf8`・`try_decode_utf8`・`decode_utf8` の呼び出し元（Phase 2）。
- 部分文字列・1 単位の検索（HEAD に runtime 関数がない。D8）。
- AVX-512、SVE、GPU、実行時の CPU 選択（Phase 1）。

## 決定事項

### D1: 実装の置き場所

- 決定: Phase 1 は C ではなく、target 非依存の 128-bit ベクトル IR の `src/runtime/string_v128.ll`（新規）に置く。スカラー版は `src/runtime/string_scalar.ll`（新規）。同名・同引数で、どちらか一方を連結する。
- 理由: wasm32 には C runtime の compile がなく、native で C にすると文字列を使う全 program の build に clang の compile が一つ増える。IR なら呼び出し元へ inline され、
  `--emit llvm` と `tests/strings.mjs` の harness にそのまま現れる。起票時の既定案（C カーネル）は、実行時の CPU 選択が要る Phase 2 の AVX2 版で使う。
- 状態: 既定案（実装者はこの案に従う）

### D2: 選択規則

- 決定: `!wasm || wasm_simd` なら `string_v128.ll`。native は常にベクトル版（NEON と SSE2 は ABI の必須機能）で、Phase 1 に実行時の CPU 選択はない。
- 理由: D-18（WASM の SIMD は opt-in）を守り、native の `--cpu generic` の配布物にも必須機能の命令だけを出す。
- 状態: 既定案（実装者はこの案に従う）

### D3: 窓の規則と幅

- 決定: probe の幅は 16 bytes（UTF-8 16 bytes、UTF-16 8 単位）で固定。probe の失敗後は窓の終わりまで scalar で進む。長さの閾値、適応的な窓の拡大、種目別の調整は入れない。
- 理由: probe の費用は 16 bytes あたり最大 1 回に抑えられ、短い文字列は `残り >= 幅` の比較 1 回で HEAD の経路に入る。閾値は計測への過適合を招く。
- 状態: 既定案（実装者はこの案に従う）

### D4: 計数関数の切り出し

- 決定: `@tz.string.utf8_to_utf16_length` と `@tz.utf8string.utf16_to_utf8_length` を両文書に置き、不正と上限超過で -1 を返す。変換はこれを呼び、負なら trap する。
- 理由: 不正な入力の網羅検査（T1・T2・T5）を、1 件ごとに process や instance を作らずに行える。trap の条件と時点は HEAD と同じ。
- 状態: 既定案（実装者はこの案に従う）

### D5: 検証の参照

- 決定: スカラー版を名前を変えて同じ module に同居させる差分検査と、Node の `TextDecoder`・`isWellFormed`・`toWellFormed`・JS の比較を参照にする。ASan の代わりに guard page を使う。
- 理由: 現在の出力ではなく独立の参照で期待値を決める。同じ target・同じ最適化で比べるので、差の原因を絞れる。
- 状態: 既定案（実装者はこの案に従う）

### D6: 入力別の計測 runner

- 決定: `benchmarks/utf/` と `benchmarks/run-utf.mjs`（suite `utf`）を足す。入力 5 種 × 操作 6 種 × 大きさ 3 種。`benchmarks/run-cpp.mjs` の種目と比較相手は変えない。
- 理由: 既存の 4 種目は 2 種類の入力しか持たず、混在 Unicode の退行と短い文字列の費用を分けて見られない。
- 状態: 既定案（実装者はこの案に従う）

### D7: Phase 2

- 決定: 非 ASCII の窓の lookup 検証、BMP の窓のベクトル符号化、F08 の仕組みによる AVX2 版、`validate_host_utf8` への適用は Phase 2 とする。
- 理由: target 固有の shuffle（または C の kernel と実行時の選択）が要り、F08 の interface と費用の判断が先に要る。
- 状態: 要承認（承認前は Phase 2 に着手しない）

### D8: 検索

- 決定: 1 byte・1 単位の検索の SIMD 版は作らない。
- 理由: HEAD に検索の runtime 関数がなく、速くする対象がない。検索の API を足すチケットが、その時点で窓の規則と `string_v128.ll` を使う。
- 状態: 既定案（実装者はこの案に従う）
