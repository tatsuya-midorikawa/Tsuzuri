# PR05: UTF 変換・比較・検証の SIMD 化

| 項目 | 内容 |
| --- | --- |
| ID | PR05 |
| 分類 | 実行速度 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | PX01, (F08) |
| 関連 | PM03, F05, F08 |
| 状態 | in-progress |
| 起票 | 2026-09-29。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | Phase 1・2 とも利用者が全段の実装を明示承認。F08 は完了済み |
| 手本にする既存実装 | 16-bit・8-bit 列の一括比較: `src/runtime/string.ll` の `@tz.string.equal`・`@tz.string.compare` の `wide`／`wide_advance` 節（64-bit 読み出し）。target ごとの runtime 文書の選択: `src/llvm.rs` で `heap-wasm.ll` と `heap-native.ll` を `if wasm` で選ぶ箇所。runtime の単体検査: `tests/strings.mjs` の runtime harness（`--emit llvm` の IR と `tests/strings_runtime.c` を native と wasm32 の `-O0`／`-O3` で結合し、`all_scalars`・`runtime_trap_case` を呼ぶ） |
| 主な影響ファイル | `src/runtime/string.ll`, `src/runtime/utf8string.ll`, `src/runtime/string_scalar.ll`（新規）, `src/runtime/string_v128.ll`（新規）, `src/llvm.rs`, `src/driver.rs`, `tests/strings.mjs`, `tests/strings_runtime.c`, `tests/strings_simd.c`（新規）, `benchmarks/utf/Kernels.tz`（新規）, `benchmarks/utf/host.c`（新規）, `benchmarks/run-utf.mjs`（新規）, `docs/benchmarks.md`, `docs/architecture.md`。Phase 2 だけ: `src/runtime/cpu.c` |
| 計測対象 | `cpp/utf8_roundtrip`・`cpp/utf16_compare`・`cpp/utf16_validate`（改善）、`cpp/utf16_scan`（退行の確認）、`utf/*`（新規。入力 5 種 × 操作 6 種） |

## 目的

UTF-8 と UTF-16 の変換・検証・比較を、128-bit ベクトルで一度に 16 bytes ずつ処理し、simdutf などの SIMD 実装に近づける。
Tsuzuri の既定の `string` は UTF-16 で、ホストやファイルの UTF-8 との変換が頻繁に起こるため、変換の速度がデータ処理全体の速度を左右する。
結果（変換後の列、trap の有無、置換、比較の順序）は HEAD のスカラー実装とビット単位で同一にする。

利用者の明示承認により、Phase 1（target 非依存の 128-bit ベクトル IR）と
Phase 2（非 ASCII の区間のベクトル検証・復号と AVX2 の CPU 選択）をともに実装する。F08 は完了済み。

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
  その `declare` が IR にあると driver が `cpu.c` を native の runtime に加える。
  `CPU_KERNELS` の表は配列の単一引数演算用なので、引数の異なる UTF kernel は表に加えず
  `string_cpu.ll` の宣言と wrapper で接続する。現行 driver の接頭辞判定を再利用する。
- PM03（後続）: PR05 は `%tz.string = { ptr, i64 }`・`%tz.utf8string` の表現と runtime 関数の名前・引数を変えない。PM03 は
  `src/runtime/string_scalar.ll`・`src/runtime/string_v128.ll`（D1）の対に Latin-1 の版を足す前提で設計する。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- ベクトル版とスカラー版（または独立の参照）の結果が一つでも違い、その原因がスカラー版（HEAD）の誤りに見える。スカラー版の意味は変えない。
- Phase 1 のベクトル IR に target 固有の intrinsic（`llvm.aarch64.*`・`llvm.x86.*`・`llvm.wasm.*`）が要る、または
  汎用のベクトル演算が native か wasm32（simd128）で legalize できない。
- 既定の wasm32（`--wasm-feature` なし）の出力に `v128` の命令が現れる、または既定の WASM import が増える。
- driver から emitter へ「wasm32 で simd128 が有効か」を渡すのに、`bool` の欄を一つ足す以上の変更が要る。
- Phase 1 の target 非依存 UTF runtime に native C runtime の compile（`cpu.c` など）が必要になった。Phase 1 は C compile を追加しない（D1）。Phase 2 は利用者承認済み F08 CPU dispatch を使うため、該当する native UTF 呼び出しがある場合の `cpu.c` は明示的な例外とし、PB01 の prebuilt/cache と no-cache の cold build cost を区別する。
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
| M4 成果物の大きさ | bytes（PX01 の成果物サイズ） | `tests/fixtures/strings` の native object（`-O3`）、wasm32 の既定と simd128（`-O3`） | 記録だけ。既定の wasm32 は portable scalar のままで SIMD 命令を含めない。Phase 2 の scalar 経路と module 分離によるサイズ差も記録する |

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

- Phase 1: D1〜D6、V1〜V7、テスト、`utf/*` の計測、文書。
- Phase 2（承認済み）: 非 ASCII の窓のベクトル検証・復号、BMP のベクトル符号化、
  F08 の AVX2（32 bytes）版、`validate_host_utf8` への適用。
  3 表の lookup に限定せず、前 1/2/3 byte の mask と固定 `shufflevector` を使う。
  target 固有の IR は不要で、native の CPU 選択だけ C の kernel を使う。

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
- 全 phase の correctness・計測 gate 後に、親が `_perfs/README.md` の状態欄と完了記録を更新する。

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

### 現行 tree との照合と承認済み実装方針

- 利用者は Phase 2（非 ASCII の検証・復号、AVX2 と安全な CPU 選択）も明示承認した。旧記述の「Phase 1 だけ」は適用しない。
- F08 の現行 interface は `TZ_CPU_PICK`、`tz_cpu_level`、公開名 `tsuzuri_cpu_*`。driver は既にこの接頭辞の宣言を検出する。
  UTF kernel もこの規則を再利用し、既存の能力検査・OS 状態検査・強制 variant の拒否を保つ。独自 dispatcher は追加しない。
- emitter の追加欄は協調作業の約束に従い `wasm_simd128: bool`（既定 false）。PB01 の `runtime_objects` と独立。
  既定 WASM は全 scalar、opt-in と native は汎用 vector IR。CPU helper は native のみ。
- scalar の全対象関数を別文書に保存する。親が追加した append・concat_into・allocate.check は元文書に残す。
  変換は入力全体の検証・長さ計数を完了してから従来どおり一度だけ確保する。
- 非 ASCII は汎用 vector 演算による合法な UTF-8 block の検証と 2/3/4-byte block の復号、BMP block の符号化を実装する。
  target 固有 lookup intrinsic を追加せず、短い／境界をまたぐ／不均一な block は従来の scalar decoder に戻す。
- raw harness に PM06 の容量境界（UTF16 0/128/129、UTF8 0/256/257）と append の空・所有権・overflow を追加する。
- 親の指示により正式な性能計測は CPU 競合解消まで保留。基準 compiler は `target/perf/phase2-baseline/tsuzuri-before`。
  本チケットは移動せず、中央文書・統合検証は親が担当する。

### 実装済みの経路と統合インターフェース

- `string_scalar.ll`: 元の 10 関数を完全に保持。非トラップの計数 2 関数と
  `tz.string.validate_utf8(ptr, i64) -> i1` を追加。後者は UTF-16 変換の長さ制限を host の UTF-8 に持ち込まない。
- `string_v128.ll`: `generate_utf.py` が scalar 参照から決定的に生成。
  等値は 2 × 128 bit、順序比較は 128 bit、残りは元の 64 bit／単位ごとの処理。
  ASCII の拡張・縮小、合法な 2/3/4-byte block の検証・復号、同じ幅の BMP の符号化、
  非サロゲート区間の判定・修復を実装。mixed UTF-8 の検証・計数も vector mask で処理する。
  mixed の出力書き込みは scalar 窓へ戻し、均一な 2/3/4-byte block は vector で復号する。
  最初に失敗した probe または短い tail の後は残り全体を元の scalar decoder で進み、bulk を再検査しない。
  拒否した UTF-8 計数は元の scalar 計数へ戻して
  encoding／size の trap 分類と最初の失敗の順序を保つ。
- `cpu.c`: UTF-8→UTF-16 と UTF-16→UTF-8 の count/write kernel。
  `tsuzuri_cpu_utf8_to_utf16(ptr, i64, ptr) -> i64` と
  `tsuzuri_cpu_utf16_to_utf8(ptr, i64, ptr) -> i64` は出力が NULL なら検証・計数だけを行う。
  `tsuzuri_cpu_utf8_validate(ptr, i64) -> i32` は妥当なら 1、不正なら 0。
  検証だけの場合は変換先 UTF-16 の長さ上限を適用しない。
  baseline/SSE4.2 は 16 bytes、AVX2 は 32 bytes。既存 dispatcher の強制 AVX-512 も 64-byte 版を持つが、自動選択の規則は変えない。
  mixed UTF-8 は前 1/2/3 byte の vector mask を用いて検証し、2/3-byte 区間は vector で復号、
  BMP 区間は vector で計数・符号化する。その他は scalar。3 表 lookup や target 固有 IR は不要だった。
- `string_cpu.ll`: native build の `cpu_dispatch` が真で、変換または UTF-8 検証の
  記号が実際に要求されたときだけ連結。単に heap を使う数値 program には連結しない。
  32 以上の入力は CPU kernel で全体を検証・計数してから従来どおり一度だけ確保する。
  短い入力、raw `--emit llvm`、freestanding は自己完結した portable IR。
  driver の既存 `tsuzuri_cpu_` 接頭辞検出が UTF も扱うので新しい判定は不要。
  Windows は既存 F08 の能力検出に従い portable baseline を使う。Windows の AVX2 dispatch は実装していない。
- `EmitOptions.wasm_simd128` は既定 false。WASM は opt-in だけ 128-bit 文書を選ぶ。
  CPU helper を WASM に連結せず、import は増やさない。PB01 の `runtime_objects` とは独立。
- `.gitignore` に新規 3 文書の例外を追加。append・concat_into・allocate.check は元文書に保持。
- host validation の compiler lowering は親の最終指示により統合済み。`validate_host_utf8` は新しい
  `tz.string.validate_utf8` の i1 結果を既存 `TrapKind::Encoding` guard に渡せばよい。
  scalar decoder と host の範囲・整列・所有権検査を変更しない。
  `src/llvm_abi.rs:690` の旧 scalar loop を次の method body に置き換えた。
  host 境界も scalar／simd128／native CPU の target 選択を使う。最終 dynamic gate は親が source freeze 後に行う。

  ```rust
  let valid = self.value(format!(
      "call i1 @tz.string.validate_utf8(ptr {pointer}, i64 {length})"
  ));
  self.guard(&valid, TrapKind::Encoding);
  ```

### 検証記録と再現

以下は最終 cleanup **前**の先行検証記録。凍結後の最終結果は後述の「凍結後の最終検証」に分けて記録する。

- `cargo check --all-targets --locked --quiet`: 成功。
- `cargo test --locked --test strings --quiet`: 14 件成功（target 別選択、宣言重複、決定性を含む）。
- `cargo test --locked --test formatter corpus_is_idempotent_and_semantically_unchanged --quiet`:
  1 件成功。benchmark の range は `1 .. repeat`（`1 to repeat` ではない）。
- `node scripts/check-docs.mjs`（String・Utf8String の 2 ページ）:
  21 links、5 examples、native O0/O3 の 10 runs が成功。
- `node benchmarks/run-utf.mjs <compiler> --quick`: 新旧とも 30 workload の独立 checksum が成功。
  基準は `target/perf/phase2-baseline/tsuzuri-before`。出力は
  `target/perf/PR05/utf-{baseline-,}quick.jsonl`。quick の時間は性能比較に使わない。
- bounded／exhaustive／guard／storage の 6 構成:

  ```sh
  TSUZURI_CLANG=/opt/homebrew/opt/llvm@21/bin/clang \
  TSUZURI_ASAN_CLANG=/usr/bin/clang \
  TSUZURI_OBJDUMP=/opt/homebrew/opt/llvm@21/bin/llvm-objdump \
  node tests/utf_simd.mjs --quick --asan --exhaustive
  ```

  native・既定 WASM・simd128 WASM の O0/O3 が成功。
  native CPU baseline/auto、全 scalar 4344 batches、strict trap 28 件、
  concat_into の UTF16 0/128/129・UTF8 0/256/257、append の空／所有権と追加 overflow trap 6 件を含む。
  長さ 1〜3 の全 16,843,008 byte 列は O3 native/simd128 で検査。
  `--quick` なしの全 alignment・長さ 0〜257・T2・2048 surrogate × 32 positions の 6 構成も成功。
  その後、T1/T2 の妥当列の出力比較と T3 の全符号なし境界対を追加した全行列、
  JS の TextDecoder／TextEncoder／isWellFormed／toWellFormed による独立参照の全行列も 6 構成で成功。
- ASAN は LLVM IR の関数へ `sanitize_address` を明示して guard/storage を検査する。
  Homebrew LLVM 21 の ASAN runtime はこの macOS 27 で main 前の dyld 初期化中に deadlock した
  （`target/perf/PR05/asan-sample.txt`）。Apple Clang 21 の runtime への切り替えで成功。
  Windows の guard は VirtualAlloc/VirtualProtect、POSIX は mmap/mprotect。Windows 実行は CI が確認する。
- 生成コード（`target/perf/PR05/{utf-arm,utf-x86,cpu-x86}.asm`）:
  portable x86 の YMM は 0。UTF AVX2 kernel の YMM 参照は UTF-8→UTF-16 340、UTF-16→UTF-8 67。
  baseline UTF kernel の YMM は 0。NEON と portable SSE の vector 命令も確認。
  WASM の逆アセンブルは 6 構成の runner が検査し、既定は v128 系 0、opt-in は実命令あり、import は空。
  x86 は cross compile の命令確認であり、実機の速度・Windows の AVX2 実行は主張しない。

### 最終 cleanup と計測隔離

- heap-only program へ UTF CPU wrapper が混入する原因を runtime 選択条件で修正。
  `tests/cpu_dispatch.rs` の float／wide-integer の禁止 assertion は弱めていない。
- host の検証だけの長さ領域を保つ dedicated C validator、portable mixed-block 検証・計数を追加。
- UTF-8 count の拒否を元の scalar 失敗経路へ戻し、encoding／size trap の分類と順序を維持。
- native differential runner は CPU wrapper 自体も連結し、portable 計数 helper と
  C kernel の直接検査だけでなく、実際の native count→allocate→write 経路を検査する。
- compare benchmark は同じ長さで最後の単位だけが異なる入力へ変更。
- 親が正式 before 計測を開始したため、最終 Rust／matrix／benchmark smoke の 3 process は停止した。
  その後、host validation と PM06 identity 修正を含む release を親が rebuild し、ソースを凍結した。
  UTF／strings／storage の最終 gate は以下のとおり成功。全 Rust regression と正式な matched 計測は親の統合 gate であり、
  この記録だけでそれらの完了を意味しない。再現コマンド:

  ```sh
  # project-local TMPDIR を用意し、既存 Rust test の作業領域も repository 内へ置く。
  mkdir -p target/pr05-validation
  TMPDIR="$PWD/target/pr05-validation" TSUZURI_CACHE_DIR='' \
    cargo test --locked --test cpu_dispatch --test strings --test prebuilt_runtime --quiet
  TSUZURI_CLANG=<LLVM clang> TSUZURI_OBJDUMP=<llvm-objdump> \
    node tests/utf_simd.mjs --quick
  TSUZURI_CLANG=<LLVM clang> TSUZURI_ASAN_CLANG=<working ASAN clang> \
    TSUZURI_OBJDUMP=<llvm-objdump> node tests/utf_simd.mjs --quick --asan --exhaustive
  TMPDIR="$PWD/target/pr05-validation" TSUZURI_CACHE_DIR='' \
    cargo test --locked --test formatter corpus_is_idempotent_and_semantically_unchanged --quiet
  node benchmarks/run-utf.mjs target/perf/phase2-baseline/tsuzuri-before --quick
  node benchmarks/run-utf.mjs target/release/tsuzuri --quick
  ```

  最後の全-target regression と full matrix（`--quick` を外す）は統合 gate。
  Windows behavior は green Windows CI、x86 AVX2 の実行は対応 x86 runner が確認する。
  今回の macOS ARM 実行から Windows／AVX2 の実機検証済みとは主張しない。

### 凍結後の最終検証

2026-10-14、親が凍結・rebuild 後の結果を報告:

- `frozen-utf`: `tests/utf_simd.mjs --quick --asan --exhaustive` が全 6 構成で成功。
  native CPU baseline/auto、既定 scalar WASM、opt-in simd128 WASM の O0/O3。
  ASAN の native guard/storage と、O3 native／simd128 WASM の長さ 1〜3 の全 16,843,008 byte 列を含む。
  最終 CPU wrapper、dedicated UTF-8 validator、mixed-block 検証・計数、scalar 失敗 fallback 後の結果。
- `frozen-strings`: `tests/strings.mjs` が 3973 cases／75 traps で成功。
  全 Unicode scalar の 4344 batches／28 runtime traps も各 O0/O3 で成功。
  PM06 の `allocation_baseline`／`allocation_move` の allocation parity は元の期待値のまま成功。
- 親の storage gate: ASAN と fake stack を用いた 24 cases が成功。
- 正式な UTF matched timings、Windows の実行検証、x86 AVX2 の実機検証は引き続き未完了。
  性能向上／退行なし、Windows／AVX2 実機での成功はまだ主張しない。

### 親への中央文書・CI の引き渡し

- architecture: scalar/128-bit/CPU wrapper の 3 文書、target 選択、窓の fallback、
  検証完了→一度の確保、F08 の能力・OS 検査を再利用することを説明する。
- language/README/index: String と Utf8String の新しい「UTF 処理の実装」節へ案内する。
  内部 SIMD の変更だけで ABI・trap・置換・コード単位の比較順序・WASM imports は変えない。
- benchmarks: `node benchmarks/run-utf.mjs <compiler> [--quick] [--metrics directory]`。
  5 kinds × 6 operations × sizes 8/64/65536、反復約 2^24 単位、9 samples と独立 checksum。
  PX01 suite `utf` に記録する。M1〜M4 の正式な新旧計測は親の指示で未実施。性能向上／退行なしはまだ主張しない。
- CI bounded command: `node tests/utf_simd.mjs --quick`（Clang・llvm-objdump を toolchain から渡す）。
  full/reference job は `--quick` を外し、対応 platform の ASAN job は `--asan`、長時間 job は `--exhaustive` を付ける。
  数値の速度閾値は置かない。
  Linux/macOS/Windows の bounded job は同じ Node command を使える。
  `--asan` は対応 Linux/macOS に限り、macOS 27 では Apple clang を指定する。
  full／exhaustive は通常 CI から除外して拡張 job へ置く。Windows は `.exe`、VirtualProtect、
  `0xC000001D` trap、`#undef NDEBUG` を runner が扱い、`-lm` は使わない。
- 正式な matched 計測（重い correctness job と重ねない）:
  `node benchmarks/run-utf.mjs target/perf/phase2-baseline/tsuzuri-before --metrics <unique-before-dir>`、
  rebuild 後に `node benchmarks/run-utf.mjs target/release/tsuzuri --metrics <unique-after-dir>`、
  再び基準 compiler で `<unique-before2-dir>`。9 組を交互に実行し、directory を使い回さない。
  `cpp/utf*` は既存 `node benchmarks/run-cpp.mjs <compiler> --metrics <unique-dir>` で別に計測する。
- PB01 は共通 String/Utf8String と新規 scalar/vector/CPU-wrapper の実内容、
  `wasm_simd128` の選択、CPU C source を fingerprint に含める。
- 解決済み PM06 allocator 比較: 先行 `tests/strings.mjs` では baseline=0 calls、move=1 call が観測された。
  fixture は `("a" + "😀").length` と `(to_string ("a" + "😀")).length`。両方 3、live=0。
  baseline は長さだけへ lowering、move は literal の `tz.string.new` と identity を経由する。
  `target/perf/PR05/alloc.ll` と `alloc-probe.c` は修正前の再現。
  親が生成された `$builtin` wrapper 内の `to_string<string>` identity を literal folding、
  frame propagation、length-only lowering で認識するよう PM06 を修正した。
  native／WASM IR regression と最終 `frozen-strings` の allocation parity が成功。
  allocator の期待値は弱めていない。この問題は open blocker ではない。

### M1 の mixed 検証退行と修正

- 既存の `cpp-{before,after,before2}` 生データを読み直すと、4fd compiler の
  `utf16_validate` の mixed 6 標本の中央値は 0.572737269 ms、before2 は
  0.441409160 ms（+29.75%）。ASCII と混ぜた中央値ではこの退行を隠すため、
  入力別の値を採用する。新しい計時は実行していない。12 内部標本であり、
  M1 の 9 独立実行の完了とは扱わない。統合変更の結果で、PR05 単独への帰属も行わない。
- 実際の種目は `String.to_well_formed` と `String.is_well_formed` を呼ぶ。
  host UTF-8 validator や変換の計数段ではない。従来の 8 単位 probe は
  サロゲートが 1 つでもあると失敗するため、正しい対を含む mixed の全窓が
  scalar fallback へ入り、probe と窓管理の費用を上乗せしていた。
- 最小修正として、同じ probe 内で上位／下位サロゲートの隣接関係を vector mask で確認する。
  正しい対を含む窓も通過する。末尾が上位サロゲートなら 7 単位だけ進み、
  その上位単位を次の probe または残り長さを確認する既存 decoder に残す。
  不正な窓の scalar fallback、8 単位の固定窓、UTF-16 の上限は変更しない。
  UTF-8 validator の full-u64 入力契約、変換の検証／確保／trap 順序も変更しない。
- test 専用 decoder counter による機能的な再現:
  mixed 64 単位の `is_well_formed` で旧版は 56 scalar decoder calls。
  新しい回帰 assertion は旧版で failure 41 となった。
  修正版は判定・修復とも 0 calls、末尾に孤立上位単位を加えた場合は 1 call。
  これは制御経路の確認で、時間・処理量の計測ではない。
- 正しさの追加検査: BMP／上位／下位の 9 単位列を全 19,683 通り差分検査し、
  8 単位境界をまたぐ対、連続上位、孤立下位、末尾上位を含める。
  guard page／WASM memory end にある境界の対と末尾上位も検査する。
  判定・計数は 0 alloc、修復の回数・bytes と出力は保持された scalar 参照と同一。
- 修正後の gate:
  `TSUZURI_CLANG=/opt/homebrew/opt/llvm@21/bin/clang TSUZURI_ASAN_CLANG=/usr/bin/clang node tests/utf_simd.mjs --quick --asan --exhaustive`
  が native CPU baseline／auto、WASM scalar／simd128 の各 O0／O3、全 6 構成で成功。
  native ASAN と UTF-8 exhaustive、Unicode 全 scalar 4,344 batches、28 traps も含む。
  `cargo test --locked --test simd --test strings --test cpu_dispatch` は 4／14／5 tests 成功。
  `cargo clippy --locked --all-targets -- -D warnings` 成功。
  String manual の checker は 1 page／10 links／3 examples／6 native O0/O3 runs 成功。
- 生成コードを時間なしで確認: ARM64 は NEON の比較・shuffle・reduction、
  simd128 WASM は `i16x8.eq`・`i8x16.shuffle`・`v128.any_true` で対を確認する。
  `target/perf/PR05/validation-fix/portable-{native,wasm-simd128}-O3.s` に保存。
  生成 IR の再現性、既定 WASM の scalar 経路、CPU capability の選択は維持する。
- 旧 4fd M4 artifacts と M1 の読み取り報告は修正前の snapshot のまま保存する。
  修正後の性能は未計測。親の許可まで全 agent の性能計測を停止し、
  親が focused before／after／before2 を直列化した後でのみ退行の解消を判定する。
  fresh M2 と新しい凍結後の全統合 gate も親が行う。

### 最終 pin 00d5 の計測停止（9 独立実行は未完了）

- 親の exclusive GO を受け、after
  `00d5d6a434fe6a3830741adc9f86e614f065794aaa426de0152adea6160e07c9`、
  baseline `ecbee0901ee678999a612a329018cfafaddc73a75183925a1db3e5770555d0dc`
  を SHA-256 で確認。Node 24.21.0、LLVM 21 clang、generic native O3、
  project 内 TMPDIR で既存 CPP runner を before→after→before2 の順に直列化した。
- run01〜07 の 7 triplet と before-run08 が完了（22 suite 実行、2,112 metric records）。
  mixed 検証の退行が全 7 triplet で再現し、after の全 run 中央値が
  baseline の全 run 中央値の最大値より遅いため、after-run08 の worker と子 process を停止した。
  **9 独立実行の M1 条件は未達。M2・M4 refresh は開始していない。**
- 次の表は各 run の入力別 6 raw 標本の中央値を求め、それを 7 run の
  median [min,max] にしたもの。raw を 42 標本へ pool していない。単位 ms。

| 種目・入力 | before | after | before2 | after/before2 |
| --- | --- | --- | --- | --- |
| validate ASCII | 0.403088 [0.401169,0.412155] | 0.114970 [0.114132,0.116067] | 0.402380 [0.402161,0.408920] | 0.2857 |
| validate mixed | 0.436766 [0.434242,0.445658] | 0.508928 [0.508388,0.509782] | 0.438163 [0.435080,0.449047] | 1.1615 |
| roundtrip ASCII | 0.747672 [0.741992,0.761468] | 0.150197 [0.149532,0.150710] | 0.743753 [0.740735,0.763090] | 0.2019 |
| roundtrip mixed | 1.824232 [1.819279,1.872657] | 1.777182 [1.775353,1.781884] | 1.822787 [1.819198,1.845183] | 0.9750 |

- validate mixed は wall +16.15%、CPU +16.07%。baseline drift は wall +0.32%。
  PX01 の spread は 0.013967 ms、中央値差は 0.070765 ms。
  範囲も交わらず、pooled median や drift では説明できない退行として扱う。
- M3 は全 7 triplet で不変: scan 30 calls／1,572,816 bytes、
  compare 33／3,145,684、validate 32／2,621,396、roundtrip 32／2,359,248。
  CPP tracking は seed 42 の tally であり、mixed 入力の確保計測とは扱わない。
- 生データ: `target/perf/phase2-formal/cpp-final-{before,after,before2}-run01..07/`。
  各 `stdout.json` が seed と checksum、`cpp.jsonl` が 12 内部標本を保持する。
  before-run08 は追加の完了データ、after-run08 は中断データとして残す。
  実行 argv／環境／pin は `pr05-cpp-final-execution.json`、
  CPU・wall、C++・Rust、入力別 run 中央値と M3 の詳細は
  `pr05-final-interim-m1.json`。いずれも同じ phase2-formal directory。
- 実際に計測した kernels object の disassembly を時間なしで読んだ。
  修復・判定とも、混在する各窓で「サロゲート無し」の reduction と分岐を経て
  対の検査へ進み、`<8 x i1>` shuffle が NEON の `xtn`／`uzp1`／`tbl`
  と次の reduction に legalize されている。
  `pr05-00d5-validate-disassembly.txt` に保存。合法対の scalar decode は除けたが、
  この余分な mask／shuffle 経路は残る。各命令の時間への寄与は未分離。
- 親への最小修正候補: 8 個の i16 tag 上で「直前の上位」と「現在の下位」を
  一度の判定にまとめ、先行する BMP-only reduction／分岐と packed i1 shuffle を除く。
  固定 8 単位の窓、末尾上位の保留、失敗時の scalar 順序、no-alloc、UTF-8 full-u64 は維持する。
  新規 CPU intrinsic、入力種目別の条件、確保変更は不要。
  **提案段階。source は変更せず、これ以上の性能計測も親の判断まで停止。**

### 承認済み i16-tag probe の修正・正しさ（性能は未計測）

- 親が 00d5 の停止後に最小修正を承認。`generate_utf.py` と生成した
  `string_v128.ll` の検証／修復 probe だけを変更した。
  i16 tag を 1 lane ずらしてから上位クラスを判定し、現在の下位クラスと照合する。
  BMP-only の先行 probe／分岐と `<8 x i1>` shuffle を削除した。
  8 単位幅、末尾上位を残す 7 単位 advance、失敗時の scalar 順序、
  `string_scalar.ll`、UTF-8 full-u64 validator、変換の確保／trap は変更していない。
- 実際の CPP `tz_utf16_validate` call-site を新しい debug compiler で
  generic O3 object に build し、ARM64 の disassembly を時間なしで確認した。
  修復／判定を含む関数全体で先行 `umaxv.8h` は 2→0、
  最終 `umaxv.8b` は 2→2、`xtn.8b` は 4→2、`uzp1.16b` は 2→0、
  packed predicate の `tbl.8b` は 2→0。i16 tag の `tbl.16b` は残る（0→2）。
  「shuffle が全部なくなった」とは主張しない。
  各 probe の reduction は 2 個から 1 個になった。
- 生成 WASM simd128 も i16 の tag shuffle／比較と単一 `v128.any_true`。
  既定 WASM は変更していない scalar module を選ぶ。
  生成 IR SHA-256 は `d939155b36f7850cdee5f014f2888c859412672929e6cc402193df9b23a731d8`、
  再生成で byte-identical。
- 正しさ gate:
  `TSUZURI_CLANG=/opt/homebrew/opt/llvm@21/bin/clang TSUZURI_ASAN_CLANG=/usr/bin/clang node tests/utf_simd.mjs --quick --asan --exhaustive`
  全 6 構成で成功。19,683 の BMP／上位／下位 boundary 列、
  native／WASM guard end、native ASAN、UTF-8 exhaustive、Unicode 全 scalar、
  trap、allocation parity、mixed block の decoder-count regression を含む。
  `cargo test --locked --test simd --test strings --test cpu_dispatch`
  4／14／5 tests 成功。`cargo clippy --locked --all-targets -- -D warnings` 成功。
- 生成コードと比較結果:
  `target/perf/PR05/tag-probe-fix/actual-cpp-validate.s`、
  `portable-native-O3.s`、`portable-wasm-simd128-O3.s`、`inspection.json`。
  旧 7 triplet の診断証拠はそのまま保持した。
- 新しい shared release は自分では build していない。親が source freeze 後に
  rebuild／pin する必要がある。A17 の correctness／private build 停止と親の
  exclusive GO まで性能計測を開始しない。
  **性能退行の解消は未判定。9 独立 CPP triplet・full M2・M4 refresh は引き続き未完了。**

### pin 2ab8 の fresh triplet: ASCII gate で停止

- 親の次の exclusive GO で content-addressed compiler
  `target/perf/phase2-final/tsuzuri-2ab8e9d944f4ac7b`
  （SHA-256 `2ab8e9d944f4ac7be501e78b19ad954425cedc2a25d86a25b41e313555213a5c`）
  を使用。曖昧な `tsuzuri-after` alias は使っていない。
  00d5 の診断資料を保持し、新しい `cpp-tag-final-{before,after,before2}-run01..03`
  に各 3 回の独立 CPP 実行を記録した（計 9 suite 実行、864 records）。
- 親が許可した 3 triplet の早期停止 gate で ASCII validate が CPU／wall とも
  adverse となり、runner は exit 3 で終了。全 worker は終了した。
  **M1 の 9 独立実行条件は未達。full M2 と final M4 refresh は未着手。**
- 各 run の ASCII／mixed 6 標本の中央値を求め、それを 3 run の
  median [min,max] にした値（ms、raw を pool しない）:

| 種目・入力・clock | before | after | before2 | after/before2 |
| --- | --- | --- | --- | --- |
| validate ASCII wall | 0.400969 [0.400642,0.402474] | 0.430544 [0.429273,0.431227] | 0.403827 [0.401261,0.405071] | 1.0662 |
| validate ASCII CPU | 0.400098 [0.399588,0.401729] | 0.429424 [0.428125,0.430417] | 0.403302 [0.400334,0.403618] | 1.0648 |
| validate mixed wall | 0.437231 [0.433381,0.437801] | 0.428760 [0.428605,0.429929] | 0.438145 [0.433433,0.438997] | 0.9786 |
| roundtrip ASCII wall | 0.745556 [0.741542,0.746657] | 0.150373 [0.149804,0.150762] | 0.741820 [0.741198,0.746240] | 0.2027 |
| roundtrip mixed wall | 1.823621 [1.821555,1.826265] | 1.756158 [1.750417,1.759071] | 1.823549 [1.816782,1.824985] | 0.9630 |

- ASCII validate は +6.62% wall／+6.48% CPU。baseline drift は
  +0.71% wall／+0.80% CPU。after の全 run 中央値が両 baseline の最大より遅い。
  mixed validate は今回の 3 run では −2.14% wall／−2.03% CPU だが、
  この事実を 9-run acceptance の完了や全 UTF の退行解消とは扱わない。
  M3 は前記の scan／compare／validate／roundtrip の calls／bytes と全 9 suite で同一。
- 原因の制御経路: BMP-only fast exit をなくしたので、サロゲートを含まない ASCII の
  各窓も i16 tag shuffle、上下位比較、隣接関係の照合、末尾上位判定を行う。
  命令ごとの時間への寄与は未分離だが、以前の ASCII fast exit は失われている。
- 最小修正候補は、i16 tag を共有する surrogate-free fast exit を戻し、
  サロゲートがある場合だけ現行 i16 adjacency checker を使うこと。
  packed i1 shuffle は復活させない。Unicode 窓に再び reduction を追加する費用は
  要確認で、今回の mixed 改善を維持すると事前には主張しない。
  適用前に親の判断を待ち、窓／末尾／scalar／no-alloc／full-u64 契約は変えない。
- `target/perf/phase2-formal/pr05-cpp-tag-final-execution.json` が argv／環境／pin、
  `pr05-cpp-tag-final-stop.json` が gate、`pr05-tag-final-interim-m1.json` が
  raw の場所・SHA と入力別 per-run 統計・M3 を保持する。
  source は凍結のままで、停止後の compiler build・性能計測・source 修正は行っていない。

### cost-based one-way fallback の実装・正しさ（性能は未判定）

- 親の承認を受け、probe の全 caller を確認した。
  `tz.utf8.block` は UTF-8 validator／UTF-16 長の計数／復号の 3 caller、
  `tz.utf16.block` は UTF-8 長の計数／符号化の 2 caller。
  これらは既に ASCII／同じ幅の BMP 区間の short circuit を持ち、
  検証完了→確保→書き込み、固定窓の scalar fallback を維持する。
  i16 adjacency probe を使うのは `is_well_formed`／`to_well_formed` の 2 caller のみだった。
- その 2 caller には、surrogate-free の 8 単位 SIMD prefix と、
  最初の surrogate window または短い tail からの元の scalar loop を採用した。
  scalar loop から SIMD へ戻る辺はない。probe／window state／adjacency shuffle を
  残りの scalar ごとに繰り返さない。修復も同じ一方向の構造にする方が小さく、
  元のコピー後の buffer に元の位置・順で置換でき、追加の確保や helper は要らない。
  「すべての Unicode を SIMD 化する」ことより入力全体の費用を優先した。
- 生成された関数の scalar remainder を元の `string_scalar.ll` と比較した。
  開始位置の phi の入力だけを除き、元の loop／decoder／reject／replace の本文は同一。
  生成 module で変更された関数も上記 2 つだけ。
  conversion／count／UTF-8 full-u64 validation／CPU kernel は byte-identical のまま。
  LLVM module SHA-256 は
  `5054afd9bcfda2713a16f15c56497fb08c3b28c36e262ffd12d2de1bc86d424c`、
  再生成で一致。
- 回帰 test は one-way transition を test 専用 counter で確認する。
  0／8／16 単位の BMP prefix に mixed 64 単位を続け、valid／末尾孤立上位を検査する。
  判定・修復とも scalar loop 開始は 1 回、scalar decoder は 56／57 calls で元の loop と一致。
  ASCII／Latin／CJK の uniform 64 単位では判定・修復・変換計数／符号化の
  scalar decoder calls が 0。正しさ・alloc 検査は従来の期待を維持する。
- gate:
  `TSUZURI_CLANG=/opt/homebrew/opt/llvm@21/bin/clang TSUZURI_ASAN_CLANG=/usr/bin/clang node tests/utf_simd.mjs --quick --asan --exhaustive`
  が全 6 構成 native／WASM scalar／WASM simd128 の O0／O3 で成功。
  19,683 boundary 列、guard end、native ASAN、UTF-8 exhaustive、
  Unicode 全 scalar、28 traps、alloc parity を含む。
  `cargo test --locked --test simd --test strings --test cpu_dispatch` は 4／14／5 成功、
  `cargo clippy --locked --all-targets -- -D warnings` 成功。
  更新した String manual の checker は 1 page／10 links／3 examples／6 native O0/O3 runs 成功。
- 新しい debug compiler で実際の CPP call-site を O3 object に build し、ARM64 を確認した。
  BMP loop には `cmeq.8h`／`umaxv.8h` がある。surrogate があればそこから scalar loop へ出て、
  scalar backedge は SIMD に戻らない。adjacency 用 `tbl`／`uzp1`／`xtn` はこの関数にない。
  WASM simd128 も同じ二段の構造。code と検証詳細は
  `target/perf/PR05/oneway-fallback/{actual-cpp-validate.s,portable-native-O3.s,portable-wasm-simd128-O3.s,inspection.json}`。
- String manual の現在の fallback 説明を上記に修正。旧 i16 adjacency の測定資料は
  履歴として保持する。shared release の rebuild／content-addressed pin が必要。
  正式な性能計測は一切追加しておらず、ASCII／mixed 退行の解消は主張しない。
  親の exclusive GO まで停止する。M1 の 9 独立実行、full M2、final M4 は依然未完了。

### one-way 版の新規 matched 計測: mixed roundtrip gate で停止

- 親の exclusive GO 後、immutable compiler
  `target/perf/phase2-final/tsuzuri-b1c866562839612e` の SHA-256
  `b1c866562839612e8a15a7d20f0509dd2f103413fdf1d4b06b5ceb5bb310a9ba` と
  baseline の `ecbee0901ee678999a612a329018cfafaddc73a75183925a1db3e5770555d0dc`
  を確認した。production source を変更せず、各実行の前にも hash を確認。
  旧 7／3 triplet は診断資料として保持し、今回の標本には含めない。
- fresh path は `target/perf/phase2-formal/cpp-oneway-final-{before,after,before2}-run01..03`。
  各 triplet は before→after→before2 の直列、3 triplet／9 suite／864 records を完了した。
  各 suite は既存 runner の 12 rotated samples、各入力種は 6 samples。
  各 run 内の ASCII／mixed 中央値を求め、その **3 run 中央値**の median／min／max を比較する。
  9 独立実行／side の M1 は未達。mixed roundtrip の wall・CPU がともに
  全 after run で両 baseline より遅く、run 中央値の範囲も完全に分離したため、
  承認された 3-triplet stop gate で正常停止。full M2／final M4 は実行していない。
- 実行は Node 24.21.0 の
  `/Users/tmidorikawa/.npm/_npx/387698761821791d/node_modules/node/bin/node benchmarks/run-cpp.mjs <immutable compiler> --cpu generic --metrics <fresh directory>`。
  `TMPDIR=target/perf/PR05/oneway-final-work`（絶対パス）、
  `TSUZURI_CLANG=/opt/homebrew/opt/llvm@21/bin/clang`。
  各絶対 argv・環境・compiler SHA・終了値・所要時間は
  `pr05-cpp-oneway-final-execution.json`。9 suite の runner 所要時間合計は 246.17 秒。
- 下表は before2 に対する変化率。ASCII と mixed は一切 pool しない。
  いずれも 3 独立 run の暫定結果で、9-run acceptance の代わりにはしない。

  | 種目 | 入力 | wall | CPU | PX01（wall／CPU） |
  | --- | --- | ---: | ---: | --- |
  | utf16_scan | ASCII | −21.94% | −21.99% | improved／improved |
  | utf16_scan | mixed | −23.81% | −23.60% | improved／improved |
  | utf16_compare | ASCII | −37.82% | −37.74% | improved／improved |
  | utf16_compare | mixed | −39.26% | −39.11% | improved／improved |
  | utf16_validate | ASCII | −79.04% | −79.05% | improved／improved |
  | utf16_validate | mixed | −1.74% | −1.67% | improved／improved |
  | utf8_roundtrip | ASCII | −80.14% | −79.89% | improved／improved |
  | utf8_roundtrip | mixed | **+9.31%** | **+9.30%** | **regressed／regressed** |

- mixed roundtrip wall（ms、3 run median [min,max]）:
  before `1.848672250 [1.842593750,1.854963741]`、
  after `2.005364352 [2.002050247,2.015010742]`、
  before2 `1.834528537 [1.829882212,1.845103389]`。
  CPU は before `1.841704000 [1.836562500,1.842712000]`、
  after `1.995566500 [1.995202500,2.004132500]`、
  before2 `1.825796500 [1.824461500,1.835703500]`。
  baseline drift は −0.77% wall／−0.86% CPU。
  before に対しても +8.48% wall／+8.35% CPU であり、drift だけでは説明できない。
- validation は valid／末尾孤立上位をさらに分離した各 3 samples／run の補助集計も保存。
  ASCII は両 strata とも約 −79%。mixed malformed は −2.29% wall／−2.20% CPU、
  mixed valid は −2.17%／−2.42% だが後者は spread 内の `no difference`。
  従来の boolean／repair 退行は今回の暫定 strata では現れなかったが、
  PR05 全体の acceptance 成功とはしない。
- M3 は全 9 suite で baseline と一致:
  scan `30 calls / 1,572,816 bytes`、
  compare `33 / 3,145,684`、
  validate `32 / 2,621,396`、
  roundtrip `32 / 2,359,248`。これは既存 runner の seed42（ASCII）追跡であり、
  mixed allocation を直接測定した結果ではない。T6 の入力別 allocation parity は別の正しさ gate。
- 全 raw hash・per-run medians・spread・drift・入力別 gate・M3 は
  `target/perf/phase2-formal/pr05-oneway-final-m1.json`、
  stop 理由は `pr05-cpp-oneway-final-stop.json`。
  wrapper は exit3 で終了し、自分の測定 worker は全停止。
- 読み取りのみの call-flow 確認:
  `benchmarks/cpp/Kernels.tz::utf8_roundtrip` は
  `Utf8String.from_string`→`String.from_utf8`→等値／計数であり、
  変更した UTF-16 boolean／repair を呼ばない。
  native object emission の CPU wrapper は conversion count/write を別々に呼ぶ。
  8 UTF-16 単位の mixed pattern は毎窓に surrogate pair を含み、
  CPU UTF-16 count/write の非サロゲート block probe が失敗する。
  UTF-8 write も均一幅の block を受理できない窓では scalar decode に戻り、
  次の固定窓で再び probe する。portable conversion も同じ fixed-window fallback 方針。
  この反復費用は原因候補であり、今回まだ実際の hot assembly／profile で寄与を分離していない。
  最小候補は、利益のない conversion probe の失敗後に元の scalar remainder へ一度だけ移ること。
  mixed UTF-8 count／validation の既存有効な SIMD 経路は切り分けて保持する。
  **原因確定・source 編集・追加計測は親の承認前には行わない。**

### conversion/count/UTF-8 validation の one-way root fix 方針（親承認済み）

- frozen b1c compiler で既存 CPP の `Kernels.tz` を `--emit object -O3 --cpu generic`
  に compile し、ARM relocations を確認した。`utf8_roundtrip` は
  `tsuzuri_cpu_utf16_to_utf8` と `tsuzuri_cpu_utf8_to_utf16` を各 count/write で呼ぶ。
  UTF-16 baseline の scalar loop は `0x11918`→`0x1189c`→`0x118a4` へ戻り、
  次の 8 単位窓で再び load／ASCII reduction／surrogate reduction を行う。
  UTF-8 baseline の scalar 窓も `0x115ac`→`0x11264` へ戻る。
  mixed の毎窓に surrogate pair／異なる UTF-8 幅があるので、
  count/write の複数 pass で失敗 probe を繰り返していた。
  object／relocations／各関数の assembly は
  `target/perf/PR05/conversion-oneway/*before*.{o,s}` に保存。
- 全 caller の変更範囲:
  portable `utf8_to_utf16_length`／`utf16_to_utf8_length`、
  `from_utf8`／`from_string` の write loop、full-u64 `validate_utf8` の 5 loop。
  CPU は UTF-8 macro（count/write と pure validation の共通実装）、
  UTF-16 macro（count/write）の全 baseline／SSE／AVX2／forced AVX-512 版。
  既に one-way にした UTF-16 boolean／repair と、比較／copy は変更しない。
- 選択: 成功する ASCII／Latin／CJK／mixed count の bulk prefix は維持する。
  最初の失敗 probe または短い tail で元の scalar remainder へ一度だけ移り、戻らない。
  uniform 3-byte probe が保証する 12-byte 等の部分 prefix はその正確な長さだけ消費する。
  拒否された窓は成功 prefix が保証されないので、先頭から元の decoder へ渡す。
  scalar index／出力長は prefix の最終値を元の phi に渡し、
  invalid／overflow の guard、count 全体の検証完了後の確保、
  UTF-8 pure validation の full-u64 domain を変えない。
- window state／skip 分岐を削除し、既存 loop／labels を使う。新しい strategy／依存は不要。
  正しさと実際の assembly を検査するが、正式な計測は親の次の GO まで行わない。
  M1 の 9-run 条件と full M2／final M4 の未完了を維持する。

### conversion one-way root fix の正しさ・code inspection 完了（性能は未判定）

- 上記 5 portable loop と CPU 2 macro を修正した。
  successful block の正確な consumed／produced を prefix に加算し、
  failure／tail の後は prefix の index／長さを元の scalar phi に渡す。
  scalar body と各 guard は変更しない。CPU は最初の failed probe で bulk loop を抜け、
  元の scalar body を入力末尾まで処理する。UTF-8 pure validation は共通 macro の
  `validate=1` を維持し、変換 cap の guard を適用せず、結果を確保しない。
  UTF-16 boolean／repair、比較、CPU 能力検査、32-byte dispatch threshold、API は変更しない。
- generator の 5 scalar remainder を完全な `string_scalar.ll` と比較した。
  初期 phi の prefix 値、および既存 UTF-8 trap 分類の legacy-failure 呼び出しを正規化すると、
  本文は byte-identical。scalar から bulk への辺はなく、window state も残らない。
  再生成の SHA-256 は
  `f8717c097c1d4999568e19741fa0f5eb0df3c12eb30e33553f3acf0762c0829d` で一致。
  全 scalar 参照文書は保持し、SHA
  `13863181d4d8099bc26be90f4a6c2602c4f4eee20907f5dacad74fcb2e9d0d5c`。
- 追加 regression は ASCII／Latin／CJK の各 64-unit prefix、
  mixed 64 units、後続 ASCII 64 units の 3 入力を使う。
  test 専用 probe／scalar-entry counter により、portable UTF-16 count は 9 probes／1 entry、
  encode の count/write 合計は 18／2。UTF-8 は実際に消費する 16-byte／12-byte prefix の数
  と最初の failed probe だけで、count/write/validation が後続 ASCII で再び probe しない。
  native CPU の count/write/validation も同じ one-way を検査する。
  larger block で mixed count が成功する場合は、その profitable prefix を禁止しない。
  既存 uniform 64-unit ASCII／Latin／CJK の bulk 検査と境界検査も保持した。
- 新規 mixed 入力で portable decode／encode の出力、1 allocation と実 bytes を
  scalar 参照と比較した。invalid suffix と full-u64 の先頭 invalid UTF-8 では no allocation。
  full-u64 は bool の false だけでなく、実際に probe／scalar entry を通ることも検査し、
  conversion cap による早期 false を成功扱いしない。
- `TSUZURI_CLANG=/opt/homebrew/opt/llvm@21/bin/clang TSUZURI_ASAN_CLANG=/usr/bin/clang node tests/utf_simd.mjs --quick --asan --exhaustive`
  成功。native CPU baseline/auto、WASM scalar、WASM simd128 の O0/O3 全 6 構成、
  19,683 arrangements、guard pages/end、全 Unicode scalars 4344 batches、28 traps、
  allocation parity、UTF-8 exhaustive を含む。
  ASAN は今回 guard-only に加えて differential／19,683／all-scalars／allocation 全体も
  native O0/O3 で実行し、成功した。
- `cargo test --locked --test simd --test strings --test cpu_dispatch` は 4／14／5 成功。
  `cargo clippy --locked --all-targets -- -D warnings` 成功。
  `TSUZURI_BIN=target/debug/tsuzuri TSUZURI_CLANG=/opt/homebrew/opt/llvm@21/bin/clang node scripts/check-docs.mjs _tsuzuri/language-reference/built-in-types-and-modules/string.md _tsuzuri/language-reference/built-in-types-and-modules/utf8string.md`
  は 2 pages／21 links／5 examples／10 native O0/O3 runs 成功。
- debug compiler の private copy
  `target/perf/PR05/conversion-oneway/tsuzuri-debug`（SHA
  `3b70ba241891c82e3c869986ae335c5dfca572706ee80858d476419351dc8b11`）で、
  前記の実 CPP object を O3 compile し、**実行せず** ARM relocations／assembly を検査した。
  CPU count/write の call-flow は同じ。UTF-16 scalar region は `0x11a8c` 以降で閉じ、
  `0x11890`／`0x11a24` の vector probe へ戻る辺がない。
  UTF-8 scalar region は `0x11544` 以降で閉じ、`0x1127c` の vector probe へ戻らない。
  pure validation の wrapper は同じ UTF-8 helper に `output=NULL, validate=1` を渡す。
- portable 全 5 操作を外部 bridge から O3 compile し、native ARM／WASM simd128 assembly と
  optimized LLVM CFG を確認した。各 scalar entry から同じ pass の bulk probe は到達不能。
  count 完了後に別 pass の write prefix を開始することは維持する。
  `target/perf/PR05/conversion-oneway/{after-kernels.s,portable-native-O3.s,portable-wasm-simd128-O3.s,optimized-cfg.json,scalar-equivalence.json}`
  に保存した。assembly だけで性能改善を主張しない。
- String／Utf8String manual を actual prefix→scalar-rest の説明へ更新した。
  parent-owned 中央文書と production compiler/numeric code は変更していない。
  **formal timing は追加していない。b1c の 3 triplet は旧 source の診断資料で、
  +9.3% gate の解消はまだ未判定。shared release rebuild／新 immutable pin／exclusive GO が必要。
  M1 9-run、full M2、final M4 は依然未完了。自分の build/test worker は全終了。**

### all-oneway 版の正式 M1 完了・full M2 の独立 gate で停止

- 親の exclusive GO で immutable
  `target/perf/phase2-final/tsuzuri-2754b46a69a08c1d`
  （SHA-256 `2754b46a69a08c1daa9d0a6e5267a8189b9033fce6c99981542aa1f77ad7e232`）
  と baseline `ecbee0901ee678999a612a329018cfafaddc73a75183925a1db3e5770555d0dc`
  を各 runner の前後に確認した。production source の変更はない。
  旧 version の diagnostic data を混ぜず、suffix は `all-oneway-final`。
- **M1 の 9 独立実行／side を完了**:
  `target/perf/phase2-formal/cpp-all-oneway-final-{before,after,before2}-run01..09`、
  27 suite／2,592 records。各 suite の 12 rotated samples（seed42..53）、
  各 run 内で ASCII／mixed の 6 samples の median、さらに 9 run medians の median/min/max。
  pooled median は使わない。before→after→before2 の serial triplet。
  runner は従来と同じ Node24 executable と
  `benchmarks/run-cpp.mjs <pin> --cpu generic --metrics <fresh path>`、
  `TMPDIR=target/perf/PR05/all-oneway-final-work`（絶対パス）、
  `TSUZURI_CLANG=/opt/homebrew/opt/llvm@21/bin/clang`。27 runner 所要時間合計 718.91 秒。
- M1 の before2 に対する wall／CPU 変化率:

  | 種目 | ASCII wall／CPU | mixed wall／CPU | PX01 |
  | --- | ---: | ---: | --- |
  | utf16_scan | −24.05%／−24.03% | −23.65%／−23.73% | 両入力・両時計 improved |
  | utf16_compare | −38.91%／−38.81% | −39.47%／−39.42% | 両入力・両時計 improved |
  | utf16_validate | −79.18%／−79.17% | −2.75%／−2.83% | ASCII improved、mixed no difference |
  | utf8_roundtrip | −81.14%／−81.15% | −5.04%／−5.06% | 両入力・両時計 improved |

  9-run の mixed validation は baseline/after の spread 内なので、改善とは読まない。
  旧 +9.3% の mixed roundtrip は、この pin の M1 では adverse gate を通らない。
  ただし後述 M2 の個別変換は別の結果で、M1 の成功で打ち消さない。
  full 9-run median/min/max、raw hash、valid／malformed suffix 補助 strata、
  drift と各判定は `pr05-all-oneway-final-m1.json`。
  M3 は全 27 suite で一致: scan `30/1,572,816`、compare `33/3,145,684`、
  validate `32/2,621,396`、roundtrip `32/2,359,248`（calls/bytes、seed42 のみ）。
- **full M2 triple を完了したが acceptance は失敗**:
  `target/perf/phase2-formal/utf-all-oneway-final-{before,after,before2}`、
  各 90 records／各 record 9 samples、合計 270 records／2,430 samples。
  command は同じ Node24 executable の
  `benchmarks/run-utf.mjs <pin> --metrics <fresh path>`（quick/filter は不使用）。
  runner 所要時間は before 47.04、after 18.90、before2 27.84 秒。
  これは compile/setup を含む所要時間であり、operation の speedup 指標にはしない。
  M2 の既存 runner は wall のみで、CPU samples はない。
- 以下は **全 90 rows** の before2 比。各 cell は独立した 9-sample row の変化率と
  PX01 verdict（I=improved、N=no difference、R=regressed）。
  入力・操作・size は pool せず、全 row の両 baseline median/min/max・drift・samples は
  `target/perf/phase2-formal/pr05-utf-all-oneway-final-m2.json` に保存。

  | 操作 | 入力 | 8 | 64 | 65536 |
  | --- | --- | ---: | ---: | ---: |
  | to_utf8 | ascii | −27.37% I | −65.87% I | −82.91% I |
  | to_utf8 | latin | −1.55% N | −16.82% I | −16.31% I |
  | to_utf8 | cjk | −39.56% I | **+43.64% R** | **+59.47% R** |
  | to_utf8 | emoji | −4.19% I | −11.70% I | −27.41% I |
  | to_utf8 | mixed | −0.89% N | **+29.29% R** | **+17.96% R** |
  | from_utf8 | ascii | **+16.92% R** | −53.42% I | −81.21% I |
  | from_utf8 | latin | **+21.20% R** | −24.77% I | −36.87% I |
  | from_utf8 | cjk | −24.29% I | −73.17% I | −76.98% I |
  | from_utf8 | emoji | −26.74% I | −42.04% I | −50.06% I |
  | from_utf8 | mixed | **+20.12% R** | −1.30% I | −12.01% I |
  | equal | ascii | −2.42% N | −63.72% I | −62.67% I |
  | equal | latin | −0.48% N | −64.22% I | −62.99% I |
  | equal | cjk | +0.76% N | −64.13% I | −62.44% I |
  | equal | emoji | +0.10% N | −64.06% I | −62.51% I |
  | equal | mixed | −0.43% N | −64.13% I | −62.16% I |
  | compare | ascii | **+10.87% R** | −32.47% I | −48.78% I |
  | compare | latin | **+11.00% R** | −33.02% I | −48.80% I |
  | compare | cjk | **+11.18% R** | −32.80% I | −48.24% I |
  | compare | emoji | **+11.08% R** | −32.73% I | −48.28% I |
  | compare | mixed | **+12.65% R** | −33.33% I | −48.03% I |
  | is_well_formed | ascii | −77.50% I | −90.75% I | −90.95% I |
  | is_well_formed | latin | −77.18% I | −90.80% I | −90.94% I |
  | is_well_formed | cjk | −77.45% I | −90.77% I | −90.98% I |
  | is_well_formed | emoji | −0.64% N | +0.43% N | −0.03% N |
  | is_well_formed | mixed | −0.79% N | +0.35% N | +1.39% N |
  | to_well_formed | ascii | −100.00% I | −99.99% I | −99.87% I |
  | to_well_formed | latin | −100.00% I | −100.00% I | −99.87% I |
  | to_well_formed | cjk | −100.00% I | −99.99% I | −99.87% I |
  | to_well_formed | emoji | −100.00% I | −99.99% I | −99.55% I |
  | to_well_formed | mixed | −100.00% I | −99.98% I | −99.56% I |

- before2 比は 65 I／13 N／12 R、before 比は 64 I／14 N／同じ 12 R。
  12 R はすべて after の **最小**が両 baseline の **最大**より遅い。
  adverse row の baseline drift は −1.23%〜+0.52% で、退行を説明できない。
  CJK large encode は before `0.173746094 [0.172886719,0.180074218]`、
  after `0.278000000 [0.277132812,0.279605469]`、
  before2 `0.174332031 [0.173312500,0.182093750]` ms。
  mixed large encode は before `0.152410156 [0.151769531,0.153187500]`、
  after `0.180574219 [0.179652344,0.181449219]`、
  before2 `0.153078125 [0.152296875,0.154468750]` ms。
  short compare の増加は約 0.3 ns/op だが、量が小さいことを理由に stable gate を免除しない。
- `to_well_formed` の約 −100% は SIMD repair の throughput として主張しない。
  この fixture は repaired content を読まず、`length` だけを集計する。
  after の compiler 最適化／work elimination の影響を受け得る。
  M1 validate は repaired text の内容を合計しており、同じ測定ではない。
  benchmark は変更せず、matched source の end-to-end 結果として上表を保存する。
- 読み取りのみの次の原因候補:
  既に保存した actual CPU UTF-16 writer assembly に
  `0x1195c: udiv w12,w9,w8`／`0x11960: msub ...` があり、
  C の dynamic `width` を使う packed-byte loop が homogeneous CJK の byte ごとに division を行う。
  候補は width2/3 の定数 index の packing/store に分け、この software division を消すこと。
  mixed の CPU scalar writer は count 済みの input に pair validation と output-mode 分岐を繰り返すので、
  validated LLVM writer との仕事量・call-flow をさらに比較する必要がある。
  short decode は full vector block 未満なのに prefix/count の固定費を払い、
  short comparison は 128-bit 検査と mismatch scalar path の費用が候補。
  いずれも新たな profile／source 編集を行っていないので、原因の寄与率・修正後性能は未判定。
- 全 exact argv／env／pre/post compiler SHA／終了値は
  `pr05-cpp-all-oneway-final-execution.json` と `pr05-utf-all-oneway-final-execution.json`。
  M2/M4 wrapper は full M2 gate の後で exit3、worker は全終了。
  **final M4 は条件未達で未実行。PR05 は完了しない。
  追加計測・修正は親の承認まで停止する。旧 M4 sizes は final pin の結果とは扱わない。**

### D1: 実装の置き場所

- 決定: Phase 1 は C ではなく、target 非依存の 128-bit ベクトル IR の `src/runtime/string_v128.ll`（新規）に置く。スカラー版は `src/runtime/string_scalar.ll`（新規）。同名・同引数で、どちらか一方を連結する。
- 理由: wasm32 には C runtime の compile がなく、native で C にすると文字列を使う全 program の build に clang の compile が一つ増える。IR なら呼び出し元へ inline され、
  `--emit llvm` と `tests/strings.mjs` の harness にそのまま現れる。起票時の既定案（C カーネル）は、実行時の CPU 選択が要る Phase 2 の AVX2 版で使う。
- 状態: 既定案（実装者はこの案に従う）

## full M2 の 12 退行: baseline/current の帰属と修正方針

pin `2754b46a69a08c1d` と baseline `ecbee0901ee67899` の実際の native object ビルドを
clang wrapper で捕捉した。`--emit llvm` は native CPU wrapper を含まないため、
代用しない。捕捉 IR・clang argv・最適化 IR・ARM disassembly は
`target/perf/PR05/m2-root/` に保存。

- baseline の符号化 builtin は最適化 IR 262 行／ARM 175 行、current は 404／338 行。
  baseline には UTF-16 CPU helper 呼び出しも `udiv` もない。current の C writer は
  動的な `j / width` の packing を行い、既に count で検証した surrogate を再検証する。
  count のみでも外部 C 関数となり、LLVM の呼び出し側への特殊化を失う。
- 短い UTF-8 復号は baseline ではループ内に inline、current は大きくなった
  `.from_utf8.portable` への呼び出しが残る。C 内で閾値を設けても解消しない。
- 8 単位の順序比較は current の 128-bit 比較が末尾の差で失敗し、同じ位置から
  baseline の 64-bit 比較を繰り返す。ARM の `ldr q`／`cmeq`／`umaxv` の後に
  `ldr x` と scalar 比較が続くことを確認した。

最小の operation-specific 方針:

1. 順序比較の vector 開始閾値を 2 窓にする。短い列は元の 64-bit/scalar 経路。
2. 16 bytes 未満の UTF-8 復号は dispatch と大きな portable helper より前に、
   retained scalar 関数と scalar ASCII copy の inlineable なコピーへ直接渡す。
3. UTF-16 符号化は AArch64 では全体を portable typed LLVM に置く。初窓に
   surrogate がある場合は元の検証・count・書き込みへ一度だけ渡す。
   x86 では既存の安全な CPU count 選択を保ち、検証済みの書き込みは同じ LLVM
   writer を使う。CPU raw API の再検証契約は変えない。
4. C の raw UTF-16 writer の packing も幅 2／3 を分けて動的除算をなくす。

CPU 数・SIMD 幅ではなく end-to-end cost を選ぶ。AVX2 UTF-8 検証／復号と
UTF-16 count の実装・安全な選択は保持する。AArch64 UTF-16 符号化では
CPU dispatcher を使わないという実際の fallback 方針を文書化する。
まだ修正後の性能を測っておらず、12 退行の解消は主張しない。新しい immutable pin
で M1 九 triplet・M2 全 90 行・M4 を再実行する必要がある。

### inlineable LLVM 版の correctness freeze

上記 4 方針を実装した。生成器は 4 個の scalar コピー（UTF-8／UTF-16 の
変換と両 ASCII copy）に `alwaysinline` を付けるだけで、参照元の関数本体は
変更しない。属性・記号名を戻した比較で 4 関数とも byte-identical。
`string_scalar.ll` 自体の SHA-256 は引き続き
`13863181d4d8099bc26be90f4a6c2602c4f4eee20907f5dacad74fcb2e9d0d5c`。

ISA／呼び出しの実際の選択:

| 操作 | native | portable LLVM／WASM simd128 |
| --- | --- | --- |
| UTF-8 復号 | 16 bytes 未満は元の inline scalar、16–31 bytes は portable、32 bytes 以上は既存 CPU dispatch | 16 bytes 未満は元の inline scalar、以降は一括 prefix → scalar remainder |
| UTF-8 検証 | 32 bytes 以上の既存 CPU dispatch を維持 | full-u64、無確保、最初の失敗から scalar remainder |
| UTF-16 符号化 | AArch64・他の non-x86 は portable LLVM。x86 の 32 単位以上は既存 CPU count → 確保 → validated LLVM writer | 初窓に surrogate がある場合／8 単位未満は元の全 scalar 変換。それ以外は count と write の各 pass で同じ幅の prefix → scalar remainder |
| 順序比較 | UTF-16 16 単位未満／UTF-8 32 bytes 未満は元の 64-bit/scalar | 同じ閾値。等値比較の閾値は変更なし |

default WASM は参照 scalar のまま。explicit simd128 のみ vector module。
LLVM writer も同じ幅の prefix の検査を行う。count 済みだから検査を一切しない、
あるいは途中で SIMD を再開する、という実装ではない。scalar の書き込みでは元の
`decode_utf16(..., true)` を使い、count で済んだ surrogate 検証を繰り返さない。
count が成功するまで確保しない契約、失敗／trap の分類と順序を維持する。
C の raw UTF API は count／write とも従来通り入力を検証する。

新しい `src/runtime/string_cpu_utf16.ll` は手書きの x86 wrapper で、
生成物ではない。repository の Apache-2.0（`Cargo.toml`／`LICENSE`）に従い
SPDX header を明記した。外部のコード・依存・別ライセンスは追加していない。
`src/llvm.rs` は native CPU inclusion の局所変更だけ。x86 の場合だけ符号化の
記号を `.portable` に変更してこの新 wrapper を追加する。non-x86 の
符号化だけの module は CPU runtime を要求しない。UTF-8 wrapper の inclusion
と既存の CPU ABI は保持する。PB01 は CPU source hash／emitted IR が既存 cache
key に入ることを確認。次の配布更新で CPU を含む mask 2/3/6/7 × O0–O3 の
16 objects の再生成、残り 36 objects の検証・再利用が必要。

生成物の inventory は `generate_utf.py` → `string_v128.ll`。
入力は retained `string_scalar.ll`、CPU wrapper 2 files は非生成物。
再生成前後の SHA-256 が一致:
`0dbcf1ba2b0744a07bd67f7fb1468cc05f470dd4dd3b29f10d662c91cf0693ad`。

最終 private build の実際の ARM object／最適化 IR を同じ workspace に再保存した。

- 符号化 builtin は IR 622 行／ARM 469 行。CPU 符号化呼び出し、
  `tz.utf16.block` 呼び出し、`udiv` は 0。CJK 書き込みは固定 `tbl`、
  Latin の書き込みは `st2`。長い本体には複数の cold／fallback 経路があるため、
  行数の増加を速度改善として扱わない。
- 短い UTF-8 復号は LLVM の loop unswitch で独立した scalar branch となり、
  その branch 内の CPU／portable／block helper 呼び出しは 0。元の count、
  malloc、ASCII copy／validated Unicode write が inline で残る。
  関数全体には長い入力用の portable／CPU 呼び出しがある。
- 8 単位の順序比較は vector 閾値に届かず元の 64-bit/scalar へ直接進む。
  SIMD での差の reduction を払ってから同じ窓を比較し直す経路ではない。
- raw C UTF-16 writer の ARM `udiv` も 0。幅 2／3 の固定 packing を
  scalar/reference differential、ASAN、guard-page で確認した。

最終の実行済み gate:

```sh
PATH="/opt/homebrew/opt/llvm@21/bin:$PATH" \
  TSUZURI_ASAN_CLANG=/usr/bin/clang \
  node tests/utf_simd.mjs --asan --exhaustive
PATH="/opt/homebrew/opt/llvm@21/bin:$PATH" \
  cargo test --locked --test simd --test strings --test cpu_dispatch --quiet
cargo clippy --locked --all-targets --quiet -- -D warnings
cargo fmt --all -- --check
git diff --check
PATH="/opt/homebrew/opt/llvm@21/bin:$PATH" \
  node scripts/check-docs.mjs \
  _tsuzuri/language-reference/built-in-types-and-modules/string.md \
  _tsuzuri/language-reference/built-in-types-and-modules/utf8string.md
```

- 6 構成（native／WASM scalar／WASM simd128 × O0/O3）成功。
  native baseline／auto、対応 variant と unavailable／unknown rejection、
  ASAN O0/O3 の full differential と guard-only 成功。
- 19,683 surrogate 配置、full-u64 validator の actual probe／無確保、
  4,344 全 scalar batches、28 runtime traps、6 storage traps、allocation parity 成功。
  native O3 の 1–3 bytes 全列の exhaustive も成功。
- 全 offset 0–15／長さ 0–257 の differential を含む。追加の 5 入力種類 ×
  長さ 7–17 の比較、short decode の probe／CPU entry 0 の確認が成功。
  x86 の CPU count → validated LLVM write wrapper は native test-only bridge
  でも全 reference／allocation 差分を検査した。
- Rust は SIMD 4／strings 14／CPU dispatch 5 の計 23 tests、Clippy／format
  成功。manual 2 pages、21 links、5 examples、10 native O0/O3 runs 成功。
- `runTests` tool は Rust files の tests を発見しなかったため、既存 cargo runner
  を使用した。途中の旧属性で開始した full run は停止し、最終属性で上記 full gate
  を完了した。停止した run を成功数には含めない。
- 最後の full-u64 audit で CPU boolean 検証の既存の成功値 0 を確認した。
  test-only C コピーでは validation 時の未使用 count を `2^63` から開始し、
  正しい EOF が signed count の符号で拒否されないことを full gate で確認した。
  production の boolean 処理は変更しない。途中の不要な UTF-16 return 編集は
  clang に拒否され、元に戻した。最終 full gate と private native capture はその後に
  成功。最終 `cpu.c` SHA-256 は
  `26c95b59bfbccda553110d8a55d3d5fdda3e7a6adee4fdbfcd7519baa1d80555`、
  新しい手書き module は
  `f23e0f2f3e9744e8a7f4e15429d56622d14dec272cf3381bcee33929266c6e52`。

String／Utf8String の reference pages は上記の実装だけを記述し、速度の解消を
主張しない。中央 README／architecture／language／benchmarks には親の統合時に、
短い比較・復号の閾値、AArch64 encoding の LLVM 選択、x86 は CPU count と
validated LLVM write の分離、初窓 surrogate の全 scalar 選択、生成／手書き
module の inventory を反映する。その他の変換・計数・検証の prefix 後は
最初の failed probe から remainder を scalar で一度だけ処理する記述は維持できる。

この時点で production source は freeze。正式な新性能データはまだない。
pin `2754` の M1 成功は新 source の acceptance ではなく、12 M2 退行は
新 pin の全 strata を測るまで未解決として扱う。fresh immutable release と
親の exclusive GO 待ち。x86／Windows の実機確認はこの ARM host では行っていない。

## routing-final: fresh full M2 完了、4 independent wall gates で停止

2026-10-15、親の exclusive GO に従って **M2 を先に** before／after／before2
の順で実行した。production source は変更していない。全 90 行 × 9 samples ×
3 passes = **270 records／2,430 wall samples**。各 compiler の SHA-256 を
各実行の前後と終了後に確認し、一致した。

- before／before2: `target/perf/phase2-baseline/tsuzuri-before`、
  `ecbee0901ee678999a612a329018cfafaddc73a75183925a1db3e5770555d0dc`
- after: `target/perf/phase2-final/tsuzuri-ecfc4308c26bfbac`、
  `ecfc4308c26bfbacf08a4c906bc0d9b48c1184f86247829dc03044817252810f`
- wrapper: `python3 target/perf/PR05/routing-final-m2.py`。既存 runner の実際の argv は
  Node 24 の `benchmarks/run-utf.mjs <immutable compiler> --metrics <fresh dir>`。
  `TMPDIR=target/perf/PR05/routing-final-work`、
  `TSUZURI_CLANG=/opt/homebrew/opt/llvm@21/bin/clang`。
- execution log: `target/perf/phase2-formal/pr05-utf-routing-final-execution.json`
- 全 90 行の 9 raw samples、median/min/max、両比較、baseline drift:
  `target/perf/phase2-formal/pr05-utf-routing-final-m2.json`

raw JSONL は次の fresh directories の `utf.jsonl`。各 directory は stdout JSON と
stderr も保持する。以前の pin／dataset には追記していない。

| pass／directory（`target/perf/phase2-formal/` 以下） | `utf.jsonl` SHA-256 |
| --- | --- |
| `utf-routing-final-before` | `7d6b680da51fa089659022b32101861627bc24e07753c2f7973d13ecb61fb0aa` |
| `utf-routing-final-after` | `917b98b3284f8371ec72e5b3534ffe8a8ba310e6ae71b7899aedc1c82ad749d1` |
| `utf-routing-final-before2` | `b2b64c77b4056aeec8fe5449bb07c801d5d7d0ecca56d5f558ee8651a104a567` |

runner の build／setup を含む所要時間は before **45.89s**／after **18.48s**／
before2 **27.54s**。この runner 全体の差を runtime の speedup と扱わない。
M2 は CPU time を収集しないため、CPU gate を通った／落ちたとは主張しない。
各行の wall gate は両 baseline とその spread で独立に判定した。

### 停止した 4 行

時刻は ns/op、median [min,max]。各 pass に 9 samples。

| 操作／入力／size | before | after | before2 | after/before | after/before2 | baseline drift |
| --- | --- | --- | --- | --- | --- | --- |
| from_utf8/ascii/8 | 26.142 [26.044,26.255] | 30.036 [29.898,30.696] | 26.147 [26.098,26.649] | +14.90% | +14.88% | +0.02% |
| to_utf8/latin/8 | 37.177 [37.046,37.255] | 41.150 [41.031,42.152] | 37.138 [37.034,37.231] | +10.69% | +10.80% | −0.10% |
| to_utf8/latin/64 | 175.087 [174.633,175.762] | 181.591 [180.664,186.096] | 175.308 [174.179,176.453] | +3.71% | +3.58% | +0.13% |
| to_utf8/mixed/64 | 170.002 [169.662,176.346] | 180.016 [179.268,182.133] | 169.792 [169.315,173.225] | +5.89% | +6.02% | −0.12% |

4 行とも after の **最小値が両 baseline の最大値を超える**。差は両比較の
max(range) を超え、baseline drift で説明できない。親の条件「全 M2 gates 通過後に
fresh M1 九 triplet／M4」を満たさないので、**CPP／M3／M4 は開始せず停止**した。
全計測 process は exit 済み。wrapper の exit 3 はこの gate 停止を表し、
3 個の runner 自体の exit はすべて 0。

前の 12 adverse rows のうち、比較 size 8 の 5 行、Latin／mixed decode size 8、
CJK encode 64／65536、mixed encode 65536 の **10 行は両比較で改善または neutral**。
ASCII decode 8 と mixed encode 64 は引き続き退行。Latin encode 8／64 の
2 行も新しい独立の退行として扱い、過去の 12 行だけの成功で全体を合格にしない。

`equal/ascii/8` は before 比 +1.93% **neutral**、before2 比 +2.74% **regressed**。
after [1.871,1.922] ns/op は両 baseline と重なる。before/before2 drift −0.79%、
before 比の差は spread 内。この行は両 baseline の stable gate ではないが、
片側だけの adverse verdict を削除せず保存した。

### 全 90 行の独立判定

各 cell は **after/before %／after/before2 %、判定 before/before2**。
I = improved、N = spread 内（no difference）、R = regressed。
after/before は I 62／N 24／R 4、after/before2 は I 63／N 22／R 5。
操作・入力・size の pooling はしない。詳細な各 pass の min/max／samples は上記 JSON。

| 操作／入力 | 8 | 64 | 65536 |
| --- | --- | --- | --- |
| compare/ascii | +1.07/+0.64% N/N | -39.29/-39.56% I/I | -48.64/-48.43% I/I |
| compare/cjk | +2.03/+0.32% N/N | -39.63/-39.51% I/I | -49.49/-48.33% I/I |
| compare/emoji | +0.27/-1.51% N/N | -39.49/-39.34% I/I | -48.33/-48.28% I/I |
| compare/latin | -1.07/-0.96% N/N | -39.21/-39.13% I/I | -48.61/-48.68% I/I |
| compare/mixed | -1.08/+1.15% N/N | -39.57/-41.48% I/I | -47.93/-48.24% I/I |
| equal/ascii | +1.93/+2.74% N/R | -64.35/-64.20% I/I | -62.15/-62.32% I/I |
| equal/cjk | +2.02/+2.04% N/N | -63.99/-64.03% I/I | -62.75/-62.45% I/I |
| equal/emoji | -0.89/-0.43% N/N | -64.32/-64.20% I/I | -62.03/-62.04% I/I |
| equal/latin | +0.08/+0.13% N/N | -63.93/-63.93% I/I | -62.44/-62.28% I/I |
| equal/mixed | -1.66/-0.18% N/N | -64.01/-63.99% I/I | -62.02/-62.95% I/I |
| from_utf8/ascii | +14.90/+14.88% R/R | -51.46/-51.55% I/I | -81.53/-81.52% I/I |
| from_utf8/cjk | -24.54/-24.59% I/I | -72.74/-72.73% I/I | -77.11/-77.13% I/I |
| from_utf8/emoji | -31.12/-31.05% I/I | -42.07/-42.01% I/I | -50.24/-50.03% I/I |
| from_utf8/latin | -7.06/-7.08% I/I | -23.86/-23.91% I/I | -35.91/-35.98% I/I |
| from_utf8/mixed | -8.58/-7.86% I/I | -0.49/-0.41% N/N | -12.14/-12.18% I/I |
| is_well_formed/ascii | -80.21/-80.16% I/I | -90.78/-90.76% I/I | -90.95/-90.98% I/I |
| is_well_formed/cjk | -79.94/-79.86% I/I | -90.77/-90.74% I/I | -90.90/-90.96% I/I |
| is_well_formed/emoji | -0.04/-0.29% N/N | +0.06/+0.04% N/N | -0.12/-0.14% N/N |
| is_well_formed/latin | -80.16/-80.19% I/I | -90.72/-90.72% I/I | -90.94/-90.93% I/I |
| is_well_formed/mixed | -1.33/-0.32% N/N | -0.62/+0.26% N/N | +1.58/+1.84% N/N |
| to_utf8/ascii | -18.51/-18.64% N/I | -70.24/-73.46% I/I | -91.64/-91.59% I/I |
| to_utf8/cjk | -32.33/-38.52% I/I | -58.08/-58.02% I/I | -49.93/-50.01% I/I |
| to_utf8/emoji | +0.31/+0.42% N/N | +0.99/+1.02% N/N | -0.68/-0.60% N/N |
| to_utf8/latin | +10.69/+10.80% R/R | +3.71/+3.58% R/R | +0.03/+0.10% N/N |
| to_utf8/mixed | -1.75/+0.52% N/N | +5.89/+6.02% R/R | -1.02/-1.15% N/N |
| to_well_formed/ascii | -100.00/-100.00% I/I | -99.99/-99.99% I/I | -99.88/-99.88% I/I |
| to_well_formed/cjk | -100.00/-100.00% I/I | -99.99/-99.99% I/I | -99.88/-99.88% I/I |
| to_well_formed/emoji | -100.00/-100.00% I/I | -99.99/-99.99% I/I | -99.55/-99.55% I/I |
| to_well_formed/latin | -100.00/-100.00% I/I | -99.99/-99.99% I/I | -99.87/-99.87% I/I |
| to_well_formed/mixed | -100.00/-100.00% I/I | -99.99/-99.99% I/I | -99.54/-99.54% I/I |

`to_well_formed` の fixture は結果の `.length` のみを読む。ほぼ −100% の値を
SIMD repair throughput と扱わない。コンパイラの仕事の除去を含む可能性がある。
この fixture の制約は以前の記録と同じで、純粋な repair の速度を主張しない。

**PR05 acceptance は未完了**。新しい M1 九 triplet と M4 は未実行であり、
pin `2754` の成功・古い M4 サイズで代用しない。中央 docs の実装 routing は親の
統合対象だが、この実装の全性能 gates が通過したという記述はできない。

## remaining-4: actual hot pipeline の再帰属と correctness freeze

2026-10-15、親の承認で残る 4 行を修正した。性能計測はまだ再実行していない。
baseline と frozen `ecfc` の **実際の native object ビルド**を同じ clang wrapper で
捕捉し、最適化 IR／ARM assembly を比較した。保存先は
`target/perf/PR05/remaining-root/`、baseline は前の `m2-root/before-*`。
scalar helper のソースが同じという理由だけでは全 hot path の同一性を認めない。

### 帰属

- ASCII decode/8: baseline の ASCII copy は runtime overlap check 後に 8 bytes を
  `ldr d` → `ushll.8h` → `str q` で拡幅する。`ecfc` は短い caller に inline した
  上限 16 未満の情報の影響で、同じ scalar copy ソースでも 8 回の scalar
  load/store loop になっていた。count／malloc／copy の実際のパイプラインは
  baseline と異なる。CPU call や確保の追加ではない。
- Latin encode/8: 初窓の surrogate-free 判定後、均一幅でない BMP 入力が
  count／write の両 block probe で失敗し、各 pass の全 scalar と二重に仕事をする。
  短入力にはこの前処理を置かない。
- Latin encode/64: C の count は混在幅 BMP を一括計数できたが、LLVM count は
  同じ幅に限定していた。AArch64 を LLVM に戻した結果、従来の scalar 計数に
  failed probe の固定費用を足していた。書き込みの制約を count にも適用する必要はない。
- mixed encode/64: `ecfc` の builtin は最適化 IR 619 行／ARM 469 行、
  frame 64 bytes。baseline は 262／175、frame 48 bytes。大きな vector／cold
  本体と scalar fallback が同じ caller 内で register allocation され、allocation
  周辺の register move／frame layout も異なる。helper 本体だけの等価比較では
  この差が見えなかった。

### 実装選択

1. 短い UTF-8／UTF-16 変換の検証と Unicode の count/write は元の scalar 本体。
   検証後の ASCII copy だけ、baseline が実際に行っていた **8 bytes／8 単位の
   typed vector copy** を明示する。Scalar reference と元の 4 scalar copies は
   変更しない。新しい `.short` の count/write 本体も、ASCII copy の呼び先以外は元と同じ。
2. LLVM UTF-16 変換は **16 単位未満**で上記の短入力経路へ直接入る。
   それ以上の初窓に surrogate があれば元の full scalar count/write へ一度だけ渡す。
   他の入力は private `.from_string.bulk`（`noinline`）へ渡し、大きな bulk/cold
   本体を scalar hot caller から分離する。種目名、入力名、計測時間では選ばない。
3. portable UTF-16 count の block は **surrogate-free BMP の混在幅**も受理する。
   `1 + (unit >= 128) + (unit >= 2048)` の幅を typed vector で加算し、
   `llvm.vector.reduce.add.v8i16` で計数する。1 block の追加幅は 0–16 なので
   i16 overflow はない。surrogate を含む block は消費せず scalar count へ移る。
   書き込みは従来の同じ幅の block と validated scalar remainder のまま。
   count／write が別の条件で一括処理することを記述する。

CPU ISA routing／32 threshold、C ABI／C source、default WASM scalar は変更しない。
native AArch64 と opt-in WASM の LLVM count が混在幅 BMP を処理する。
x86 の long conversion は既存 CPU count → validated LLVM writer。
成功した prefix 後の最初の失敗から、各 pass は scalar remainder を一度だけ処理する。
全体の検証・計数が成功してから確保する契約、invalid/trap 順序、無確保の pure
validator と full-u64 domain は維持する。新しい strategy framework／依存は追加しない。

### 最終 actual code と検証

- private final object の encoder builtin は **IR 481／ARM 317 行、frame 48 bytes**。
  bulk helper は 389／316 行で別の `noinline` function。短入力は bulk helper と
  初窓の vector 判定を通らない。scalar mixed path の **33 count instructions と
  51 write instructions** は、baseline と **address/branch destination だけを正規化**
  すると一致する（register は変更せず比較）。
  証拠: `remaining-root/scalar-arm-equivalence.json`。caller の入口選択の固定費用は残り、
  同一性や速度を誇張しない。
- short decoder の actual hot caller は 437 IR／349 ARM 行、frame 128 bytes。
  ASCII copy の `ldr d`／`ushll.8h`／`str q` が復活し、short branch の CPU／
  portable helper 呼び出しはない。関数全体には long conversion 用の別経路がある。
- 新 intrinsic は actual emitted module に declaration 1 個だけ。
  generator 再実行で SHA が一致:
  `669b6e72e588eb2761f26378db8d04436894c8b21bfa1e1045d5c164a2fdaf9d`。
  新しい short helpers／private bulk helper／intrinsic は生成物 `string_v128.ll` に入り、
  generator は `generate_utf.py`。新しい手書き module／外部ライセンスは追加していない。
- scalar reference SHA は `13863181d4d8099bc26be90f4a6c2602c4f4eee20907f5dacad74fcb2e9d0d5c`、
  C source は `26c95b59bfbccda553110d8a55d3d5fdda3e7a6adee4fdbfcd7519baa1d80555` のまま。
  CPU-containing runtime objects の fingerprint は今回の LLVM-only 変更では変わらない。

最終 source で前記と同じ full correctness commands を実行:

- `node tests/utf_simd.mjs --asan --exhaustive`（LLVM 21 PATH、ASAN は Apple clang）:
  6 full configurations、native baseline／auto と unavailable guards、ASAN O0/O3、
  exhaustive UTF8、19,683 arrangements、4,344 scalar batches、28 runtime traps／
  6 storage traps、全 offset 0–15／長さ 0–257、allocation parity／full-u64 が成功。
- 新たに mixed-BMP 長さ 8／16／…／64 の count が reference と等しく、
  actual block probe が長さ/8、scalar decode calls と allocation が 0 であることを確認。
  短い比較／変換の 7–17 境界と guard-page を保持する。
- Rust SIMD 4／strings 14／CPU dispatch 5 の計 23 tests、all-target Clippy
  `-D warnings`、format／diff check 成功。
- String／Utf8String manual 2 pages、21 links、5 examples、10 native O0/O3 runs 成功。
  計数の混在幅サポート、16 単位の short encode routing、8 単位の ASCII copy を更新。

source はここで freeze、全 worker は停止。`benchmarks/run-utf.mjs` は workload filter を
持たず、`--quick` は計測定義を変えるため、4 行だけの診断計測は実行しない。
親の新 immutable release／GO で **fresh 全 90 行 × 9 samples の M2**、合格後に
**M1 九 independent CPP triplets／M4** が必要。`ecfc` の M2 は上の 4 adverse
gates の記録として保持し、新 source の全 gate 成功とはまだ扱わない。

## remaining-root-final: fresh M2 完了、6 independent adverse gates で停止

2026-10-15、親の exclusive GO で全 90 行 × 9 samples の before／after／before2 を
完了。270 records／2,430 samples。immutable compiler は実行前後・各 pass 前後で
SHA-256 を照合して一致した。計測中の production source 変更はない。

- baseline: `target/perf/phase2-baseline/tsuzuri-before`,
  `ecbee0901ee678999a612a329018cfafaddc73a75183925a1db3e5770555d0dc`
- after: `target/perf/phase2-final/tsuzuri-517622b8ccd3f59b`,
  `517622b8ccd3f59b2634064d246af440f4e4f702c5706c731cde258b1c89985f`
- 実行: `python3 target/perf/PR05/remaining-root-final-m2.py`。
  既存 runner／sample 数／計測定義は変更せず、新しい immutable path と dataset suffix
  だけを更新した。Node 24、LLVM 21、TMPDIR は repository 内。
  before 28.41 s／after 17.71 s／before2 27.19 s（runner/build 時間であり、性能 gate
  は各 workload の samples）。
- raw: `target/perf/phase2-formal/utf-remaining-root-final-{before,after,before2}/utf.jsonl`
- 全 90 行の samples／中央値／range／drift／verdict:
  `target/perf/phase2-formal/pr05-utf-remaining-root-final-m2.json`
- argv／環境／前後 compiler identity／時間:
  `target/perf/phase2-formal/pr05-utf-remaining-root-final-execution.json`
- raw SHA（before／after／before2）:
  `e8a5726fe08cd3836d0fb670a23660b02f36a00787af6b5113e4bd6dcc3fc3be` /
  `cf777427b9cdf62697b7e5651859cb717de82d7fcc3d02bdcdcac362a1bec814` /
  `1441375256617289efae416fa61f8915590b69f324b6c6791108e031511d2a59`

### Adverse gates と以前の 4 行

単位は **ns／complete operation**。各欄は 9 wall samples の median[min,max]。
既存 M2 runner に CPU metric はなく、wall+CPU gate の結果とは呼ばない。

| workload / size | before | after | before2 | after vs before / before2 | baseline drift |
|---|---:|---:|---:|---:|---:|
| compare/emoji 8 | 2.822[2.792,2.880] | 3.391[3.343,3.491] | 2.800[2.789,2.909] | +20.14% / +21.08% | −0.78% |
| compare/latin 8 | 2.901[2.793,2.975] | 3.491[3.424,3.550] | 2.814[2.790,2.928] | +20.33% / +24.04% | −2.99% |
| compare/mixed 8 | 2.840[2.790,2.861] | 3.387[3.352,3.458] | 2.799[2.791,2.860] | +19.29% / +21.04% | −1.44% |
| to_utf8/emoji 8 | 32.378[32.311,33.222] | 36.373[36.239,37.097] | 32.575[32.314,34.823] | +12.34% / +11.66% | +0.61% |
| to_utf8/latin 8 | 34.863[34.124,35.095] | 37.854[37.598,38.288] | 34.173[34.122,34.556] | +8.58% / +10.77% | −1.98% |
| to_utf8/mixed 8 | 35.228[35.121,36.150] | 38.107[38.035,38.158] | 35.207[35.079,35.668] | +8.17% / +8.24% | −0.06% |
| from_utf8/ascii 8（旧 adverse、今回は N/N） | 26.925[26.186,27.303] | 26.303[25.598,29.397] | 26.128[26.056,26.604] | −2.31% / +0.67% | −2.96% |
| to_utf8/latin 64（旧 adverse、今回は I/I） | 182.541[178.493,191.471] | 99.434[99.045,101.299] | 174.763[173.950,176.144] | −45.53% / −43.10% | −4.26% |
| to_utf8/mixed 64（旧 adverse、今回は N/I） | 169.636[169.350,173.244] | 165.752[165.432,168.087] | 169.926[169.636,170.361] | −2.29% / −2.46% | +0.17% |

先頭 6 行は after の **minimum が両 baseline の maximum より大きい** disjoint
adverse。短い encode/Latin の問題は残り、short compare 3 kinds と
encode/emoji・mixed が新しく stable adverse となった。既存の短入力選択だけでは
実際の whole hot pipeline のコストを baseline に戻せていない。長い mixed scalar
count/write の ARM 同一性証明を、短い whole pipeline や比較の性能証明として使わない。
新しい 6 行の原因はまだ帰属していないため、症状から code alignment／dispatch 等を断定しない。

single-side adverse も除外せず保持: compare/cjk/8 は +19.23% / +20.68%、R/N。
before2 の最大 4.800 ns が after の range と重なり、両 baseline の stable gate には
ならない。compare/ascii/8 も +17.34% / +21.90% だが range による N/N であり、
「全 short comparison が無増加」とは記述しない。

### 全 90 行: pooled summary ではない verdict matrix

I=improved、N=no difference、R=regressed。セルは **before / before2**
（% delta も同じ順序）。判定は各行の median delta と最大 sample spread。
before に対し 59 I／24 N／7 R、before2 に対し 63 I／21 N／6 R。
kind／size／operation は統合しない。repair は結果の length だけを読む fixture
なので、−100% 近傍を実メモリー修復の SIMD 処理量と解釈しない。

| workload | size 8 | size 64 | size 65,536 |
|---|---:|---:|---:|
| compare/ascii | N/N (+17.34/+21.90%) | N/N (−38.38/−38.05%) | I/I (−46.94/−47.30%) |
| compare/latin | R/R (+20.33/+24.04%) | I/I (−39.09/−37.22%) | I/I (−49.17/−46.78%) |
| compare/cjk | R/N (+19.23/+20.68%) | I/I (−41.51/−39.73%) | N/I (−49.21/−47.27%) |
| compare/emoji | R/R (+20.14/+21.08%) | I/I (−41.07/−39.91%) | I/I (−49.73/−48.26%) |
| compare/mixed | R/R (+19.29/+21.04%) | I/I (−39.42/−39.45%) | I/I (−48.50/−48.10%) |
| equal/ascii | N/N (−1.56/+2.73%) | N/N (−63.28/−62.60%) | I/I (−61.09/−61.18%) |
| equal/latin | N/N (−0.22/+2.87%) | I/I (−64.82/−63.55%) | I/I (−62.35/−61.10%) |
| equal/cjk | N/N (−1.87/+0.20%) | I/I (−65.22/−64.19%) | I/I (−62.58/−60.81%) |
| equal/emoji | N/N (−0.86/−2.74%) | I/I (−65.08/−64.52%) | I/I (−63.40/−62.23%) |
| equal/mixed | N/N (−0.10/−0.23%) | I/I (−63.98/−64.09%) | I/I (−62.80/−62.23%) |
| from_utf8/ascii | N/N (−2.31/+0.67%) | I/I (−51.65/−51.68%) | I/I (−80.95/−80.97%) |
| from_utf8/latin | N/I (−7.08/−5.84%) | I/I (−22.80/−21.67%) | I/I (−37.47/−35.36%) |
| from_utf8/cjk | I/I (−25.21/−24.38%) | I/I (−71.86/−71.46%) | I/I (−77.45/−76.79%) |
| from_utf8/emoji | I/I (−27.14/−27.31%) | I/I (−41.39/−40.70%) | I/I (−50.96/−49.86%) |
| from_utf8/mixed | I/I (−7.76/−7.74%) | N/N (+0.18/−0.33%) | I/I (−12.20/−11.94%) |
| is_well_formed/ascii | I/I (−80.49/−79.61%) | I/I (−90.47/−90.46%) | I/I (−90.69/−90.66%) |
| is_well_formed/latin | I/I (−80.05/−79.70%) | I/I (−90.55/−90.43%) | I/I (−90.93/−90.65%) |
| is_well_formed/cjk | I/I (−80.30/−80.16%) | I/I (−90.83/−90.76%) | I/I (−90.86/−90.81%) |
| is_well_formed/emoji | N/N (+0.18/−0.33%) | N/N (−1.96/−0.41%) | N/N (−2.07/−0.30%) |
| is_well_formed/mixed | N/N (+0.15/+0.21%) | N/N (+0.22/+0.40%) | N/N (+2.09/+1.07%) |
| to_utf8/ascii | N/N (−3.53/+0.40%) | I/I (−72.64/−69.16%) | I/I (−91.56/−91.53%) |
| to_utf8/latin | R/R (+8.58/+10.77%) | I/I (−45.53/−43.10%) | I/I (−50.17/−48.76%) |
| to_utf8/cjk | N/N (−1.33/−0.34%) | I/I (−77.80/−77.55%) | I/I (−85.37/−84.95%) |
| to_utf8/emoji | R/R (+12.34/+11.66%) | N/N (+0.09/−1.70%) | N/N (−2.92/−0.69%) |
| to_utf8/mixed | R/R (+8.17/+8.24%) | N/I (−2.29/−2.46%) | N/I (−1.03/−1.92%) |
| to_well_formed/ascii | I/I (−100.00/−100.00%) | I/I (−99.99/−99.99%) | I/I (−99.86/−99.86%) |
| to_well_formed/latin | I/I (−100.00/−100.00%) | I/I (−100.00/−99.99%) | I/I (−99.84/−99.84%) |
| to_well_formed/cjk | I/I (−100.00/−100.00%) | I/I (−100.00/−99.99%) | I/I (−99.86/−99.86%) |
| to_well_formed/emoji | I/I (−100.00/−100.00%) | I/I (−99.99/−99.99%) | I/I (−99.54/−99.53%) |
| to_well_formed/mixed | I/I (−100.00/−100.00%) | I/I (−99.99/−99.99%) | I/I (−99.56/−99.56%) |

**停止条件に従い fresh M1／M3／M4 は開始しない。全 owned workers 停止済み。**
production source は freeze のまま。PR05 acceptance は未完了で、過去の `2754` M1 や
`ecfc` M2 をこの pin の成功として再利用しない。中央 docs の implementation routing は
そのまま実装を表すが、性能 gates 通過／short overhead 解消の主張はできない。

### D2: 選択規則

- 決定: `!wasm || wasm_simd` なら `string_v128.ll`。native は常にベクトル版（NEON と SSE2 は ABI の必須機能）で、Phase 1 に実行時の CPU 選択はない。
- 理由: D-18（WASM の SIMD は opt-in）を守り、native の `--cpu generic` の配布物にも必須機能の命令だけを出す。
- 状態: 既定案（実装者はこの案に従う）

### D3: 窓の規則と幅

- 決定: probe の幅は 16 bytes（UTF-8 16 bytes、UTF-16 8 単位）で固定。
  変換・計数・UTF-8 validation は成功する bulk prefix を処理し、
  probe の最初の失敗後は残り全体を元の scalar loop へ一度だけ渡す。
  UTF-16 の判定・修復は、surrogate-free SIMD prefix の後に元の scalar loop へ一度だけ移り、
  残り全体をそこで処理する。復号の 16 bytes 未満は dispatch／大きな helper の前に
  inlineable な元の scalar へ直接渡す。順序比較の SIMD 開始は 2 窓分
  （UTF-16 16 単位、UTF-8 32 bytes）とし、それ未満は元の 64-bit/scalar を使う。
  CPU wrapper の既存の閾値 32（復号・検証では bytes、x86 符号化の count では単位）は
  この短入力の LLVM 選択とは別。LLVM 符号化の 16 単位未満も元の scalar count/write
  へ直接入り、短い ASCII copy は 8 単位／8 bytes の typed copy とする。
  適応的な窓の拡大、種目名／入力名による調整はしない。
- 理由: 窓の幅に基づく固定閾値で、inlineability を失った helper／外部 dispatch の
  短入力での固定費用と、順序比較で失敗した窓を元の経路で比較し直す費用を避ける。
  baseline/current の実際の IR／ARM call flow に基づく選択で、特定の workload 名に
  分岐しない。最初の失敗後は probe を繰り返さず scalar remainder を処理する。
  性能の合否はこの規則とは別に全入力・操作・サイズで検査する。
- 状態: 親承認の固定 routing／one-way fallback を実装済み。pin `ecfc` の fresh M2
  で残る 4 independent wall gates は上記記録の通り。remaining-4 版は correctness freeze
  であり、閾値導入を全 gate 成功とは扱わない。

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

- 決定: 非 ASCII の窓の検証・復号、BMP のベクトル符号化、F08 の AVX2 版、
  `validate_host_utf8` への適用は承認済み Phase 2 とする。固定 shuffle と mask を使い、3 表 lookup に限定しない。
- 理由: 共通 vector IR と F08 の安全な CPU 選択で実装でき、独立の dispatcher や target 固有 IR を足す必要がない。
- 状態: 全 phase を実装済み。最終 acceptance は下記 M1〜M4 に記録。

### D8: 検索

- 決定: 1 byte・1 単位の検索の SIMD 版は作らない。
- 理由: HEAD に検索の runtime 関数がなく、速くする対象がない。検索の API を足すチケットが、その時点で窓の規則と `string_v128.ll` を使う。
- 状態: 既定案（実装者はこの案に従う）

## 最終 acceptance: pin `de42dba363e1fa87`（2026-10-15）

全測定は Apple M1 Max、Homebrew Clang 21.1.8、Node 24.21.0 で実施。baseline compiler SHA-256 は `ecbee0901ee678999a612a329018cfafaddc73a75183925a1db3e5770555d0dc`、after compiler SHA-256 は `de42dba363e1fa877c0dab5bd51cbad17b60b774bed5b91399d9d996a99d9d08`。旧 pin `517622` の6件の短入力 adverse は歴史的な失敗記録として残し、この最終 pin では全計測を再実行した。

短入力 M2 regressions への修正は、compare wrapper を `noinline` にして呼び出し側の過剰な code expansion を抑え、UTF-16→UTF-8 converter の短い scalar block を先頭に置いて `<16` の短入力が fall-through するようにしたもの。演算内容、scalar fallback、trap 条件は変えていない。

### M1: 既存 UTF workloads

`target/perf/phase2-formal/cpp-shortfix-final-{before,after,before2}-run01..09/cpp.jsonl` に9独立 triplets（各96 records、計2,592 records）。表は Tsuzuri の native CPU 時間、ms／呼び出しの9 run medians:

| workload | before | after | before2 | after vs. before / before2 |
|---|---:|---:|---:|---:|
| `utf8_roundtrip` | 0.786423 | 0.118418 | 0.771444 | −84.94% / −84.65% |
| `utf16_compare` | 0.138647 | 0.083552 | 0.135256 | −39.74% / −38.23% |
| `utf16_validate` | 0.410970 | 0.087010 | 0.412500 | −78.83% / −78.91% |
| `utf16_scan` | 0.050981 | 0.040664 | 0.051600 | −20.24% / −21.19% |

全4 workload で stable adverse wall+CPU gate は0。

### M2: 入力別 90 workloads

各passは6 operations × 5 input kinds × 3 sizes の90行、各9 wall samples（270 records、2,430 samples）。before・after・before2 の raw `utf.jsonl` SHA-256 は `759a8a11cbe600fc033708567c171dfac9b3134bd3d0b0d82520df441545b27a`、`d4a95aacfd4e307e55589a41aeed7a27602f6092b53072dd172cd1134d3d09be`、`899f29c09896cf35f2015fd8451d3628037261f19bbfbd891408c2fa7d52634f`。stable adverse は **0**。35行は両baselineに対して improved、34行は両方と no difference、21行は一方に improved・もう一方に no difference。regressed 判定はない。比較は各operation／kind／sizeを別々に保ち、wall-time のみ（runnerにCPU metricなし）。

`to_well_formed` のfixtureは結果のlengthしか観測せず、repair内容がdead-code eliminationされ得るため、処理速度の根拠にはしない。全90行のsamples、中央値、range、drift、verdictは `target/perf/phase2-formal/pr05-utf-shortfix-final-m2.json`。

### M3／M4 と正しさ

| workload | before = after = before2 allocations |
|---|---:|
| `utf8_roundtrip` | 32 calls / 2,359,248 bytes |
| `utf16_compare` | 33 / 3,145,684 |
| `utf16_validate` | 32 / 2,621,396 |
| `utf16_scan` | 30 / 1,572,816 |

M4 `-O3` object sizes（before → after）: native 79,424 → 134,368 bytes、WASM scalar 69,230 → 78,041 bytes、WASM simd128 71,070 → 87,571 bytes。runtime gains とともに成果物サイズ増も報告し、size-neutralとは扱わない。

`node tests/strings.mjs target/release/tsuzuri` は native/WASM O0/O3 で3,973 ECMAScript/reference・ownership cases、75 traps、runtime 4,344 Unicode scalars と28 strict conversion/bounds trapsを通過。`PATH=/opt/homebrew/opt/llvm@21/bin:$PATH node tests/utf_simd.mjs --quick` は native CPU baseline/auto と WASM scalar/simd128 のO0/O3を通過した。全Rust suite と最終統合測定は親作業の validation record に記録する。
