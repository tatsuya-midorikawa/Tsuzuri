# F08: 256／512-bit SIMD と関数単位の CPU 多版化

| 項目 | 内容 |
| --- | --- |
| ID | F08 |
| 優先度 | P2 |
| 規模 | XL |
| 依存 | F04, F05, (C08) |
| 後続 | C11, C09 Phase 2 |
| 状態 | done（Phase 1・2・3） |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | Phase 1 は不要。要承認: D7（Phase 2: 256-bit SIMD 型名の追加）, D8（Phase 3: 利用者関数の多版化構文）, D9（Phase 3: AVX-512・SVE の採用）。D7・D8・D9 は、2026-10-06 に利用者から「C08、A14、A16、F13、F08 の実装をすべて完遂して。…複数フェーズがある場合には、すべてのフェーズを完了させること」と依頼され、承認として扱った（GUIDE D-39） |
| 改善する劣位 | C/C++ 比: 最適化の自由度（[なぜ Tsuzuri か](../../_docs/learn/why-tsuzuri.md#cc-に対する劣位点)）／追加: SIMD が 128-bit だけで、実行時 ISA 選択が同梱の `Array.sum<i64>` に限られる |
| 手本にする既存実装 | F05 の実行時選択: `src/runtime/cpu.c` の `tz_cpu_decode`・`tsuzuri_cpu_features`・`tz_cpu_resolve`・`tsuzuri_cpu_variant`・`tsuzuri_cpu_sum_i64`。std 本体の置き換え: `src/llvm.rs` の `FunctionEmitter::emit` の `self.globals.cpu_dispatch` 分岐、`emit_native_build` の `trusted_array`。宣言と runtime の同梱: `emit_program` 末尾の `declare i64 @tsuzuri_cpu_sum_i64(ptr, i64)`、`src/driver.rs` の `cpu_runtime` と `include_str!("runtime/cpu.c")`。非公開 std helper: `std/Map.tz` の `private def lower_bound`。テスト: `tests/cpu_dispatch.rs`, `tests/cpu_dispatch.mjs`, `tests/cpu_runtime.c` |
| 主な影響ファイル | Phase 1: `src/runtime/cpu.c`, `src/llvm.rs`, `src/driver.rs`, `std/Array.tz`, `tests/cpu_dispatch.rs`, `tests/cpu_dispatch.mjs`, `tests/cpu_runtime.c`, `tests/fixtures/cpu_dispatch/Main.tz`, `tests/fixtures/cpu_kernels/Main.tz`（新規）, `tests/cpu_kernels.mjs`（新規）, `benchmarks/run-dispatch.mjs`, `docs/language.md`, `docs/architecture.md`, `docs/benchmarks.md`, `_docs/library-reference/arrays-and-lists.md`, `_docs/guides/performance.md`, `README.md`, `_docs/feature-status.md`, `_features/README.md`。Phase 2（承認後）: `src/simd.rs`, `src/llvm_simd.rs`, `src/check.rs`, `tests/simd.rs`, `tests/simd.mjs`, `_docs/library-reference/simd.md` |

## 目的

AVX2 などの幅の広いベクトル命令を、配布用の `--cpu generic` 成果物でも実行機の CPU に応じて使えるようにする。C/C++ の
`target_clones`・`__builtin_cpu_supports`、Rust の `#[target_feature]`・`is_x86_feature_detected!` に相当する仕組みを、F05 の
`Array.sum<i64>` 専用の実装から、同梱 runtime の一括演算 kernel が共有する多版化基盤へ一般化する。どの版もビット単位で同じ
結果を返し、速度だけが異なることを保証する。

- Phase 1（実装対象）: 多版化基盤の一般化と、整数 8 型の `Array.sum`・`Array.min`・`Array.max` の kernel。言語の型・構文・
  意味は変えない（D1）。PR05 が UTF kernel で使う提供インターフェースを確定する。
- Phase 2（要承認 D7。設計方針だけ）: 256-bit の移植可能な SIMD 型（`i32x8` など）を既存 SIMD 型族の lane 数違いとして追加する。
- Phase 3（要承認 D8・D9。設計方針だけ）: 利用者関数の多版化構文、AVX-512、SVE／SVE2。

実装者は Phase 1 だけを実装する。Phase 2・3 は人間が承認して求めた場合だけ着手する。

## 着手条件と停止条件

### 着手条件

- F04（128-bit SIMD 型）と F05（実行時 CPU 選択）が `_features/README.md` の状態欄で done であること。
  確認: `grep -n "F04\|F05\|F08\|C08" _features/README.md`。完了記録は `_features/_completed/F04-portable-simd-types.md` と
  `_features/_completed/F05-runtime-cpu-dispatch.md`。
- C08（排他スライス）は Phase 1 の開始条件にしない。C08 に依存するのは Phase 2 の `Simd.store` だけ（D7）。
- Phase 1 は承認不要。Phase 2 は D7、Phase 3 は D8・D9 の承認前に着手しない。
- GUIDE §2.3 の基準コマンドが成功し、手順 1 のベースラインを保存していること。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- ある kernel の結果が、variant 間で、または独立参照（JS `BigInt`）と 1 件でも一致しない。許容誤差や特例で合わせない。
- 浮動小数点の kernel が必要になった。浮動小数点の集計は D-14 の左から右の順序を変えられない（D4）。
- `cpu.c` を Clang 以外（GCC、MSVC `cl`）で compile する経路が見つかった。または使う Clang で
  `__has_builtin(__builtin_reduce_add)` などが偽になる（D3 は Clang の vector 拡張と builtin に依存する）。
- `std/Array.tz` の変更（D5 の非公開 helper）で、既存テストの期待値（IR の文字列照合・診断・出力）を変える必要がある。
- `honors_the_exact_specialization_limit` か stack-depth の 3 テスト（GUIDE §11.1 の一覧）が失敗する。
- x86 の variant を実行できる機械がないまま、x86 の実行結果や速度を主張する必要が生じた（文書には「cross-compile のみ」と書く）。
- `tsuzuri_cpu_` の呼び出しが `llvm::emit`・`llvm::emit_target`（`--emit llvm` と通常の API）の IR に現れる。dispatch は F05 と同じく
  `emit_native_build` だけで行う（D5 の helper 追加による IR の変化は想定内）。
- `unsafe`、新しい crate、既定の WASM import、GNU ifunc、`__builtin_cpu_supports`（compiler-rt の CPU model）が必要になった。

## 現状（HEAD `f8dc655` で確認）

- SIMD 型: `src/simd.rs` の `SimdType { bits, kind }` と `SimdKind::{Signed, Unsigned, Float, Mask}`。`SimdType::lanes` は
  `128 / self.bits` 固定で、`SimdType::named` は名前の lane 数がそれと一致するときだけ受ける。`src/check.rs` の
  `Type::Simd(crate::simd::SimdType)`。数値 vector 10 型と mask 4 型。docs/language.md「SIMD 値型」は「256-bit vector、
  gather/scatter、integer division、可変slice storeは後続段階」と書く。
- 実行時選択（F05）: `src/runtime/cpu.c`。
  - `tz_cpu_decode(features, extended, xcr0)`: CPUID.1:ECX bit 20 → 結果 bit0（SSE4.2）。ECX bit 27（OSXSAVE）・bit 28（AVX）・
    XCR0 の bit 1–2（XMM・YMM 状態）・CPUID.7:EBX bit 5 がすべて立つと bit1（AVX2）。
  - `tsuzuri_cpu_features`: `static _Atomic uint64_t` の cache（acquire／release）。`TZ_CPU_X86` が 0（x86 以外と Windows）なら 0。
  - kernel: `tz_cpu_sum_baseline`、`tz_cpu_sum_sse42`（`target("sse4.2")`・`_mm_add_epi64`）、`tz_cpu_sum_avx2`（`target("avx2")`・
    `_mm256_add_epi64`）。`tz_cpu_resolve` が `_Atomic(tz_sum_function)` の cache へ関数ポインターを一度だけ cmpxchg で入れる。
  - `TSUZURI_CPU_FORCE`: `baseline`・`sse4.2`・`avx2`。利用できない値と未知の値は `tz_cpu_unavailable` が stderr に
    `Tsuzuri CPU runtime: requested variant is unavailable or unknown` を出して `abort` する。
  - 公開入口（`TZ_CPU_API` = weak／hidden）: `tsuzuri_cpu_features`、`tsuzuri_cpu_variant`（0／1／2）、`tsuzuri_cpu_sum_i64`
    （`length < 0` で `abort`）。
- IR: `src/llvm.rs` の `emit_native_build` は、`sources` に同梱と同一の `std/Array.tz` があるとき（`trusted_array`）だけ
  `Instrumentation::cpu_dispatch` を立てる。`emit_program` は wasm では偽にして `Globals::cpu_dispatch` へ渡す。
  `FunctionEmitter::emit` は std の `Array` の `sum`（`self.function.name.split(".$mono.")` の先頭）で signature が
  `ref [i64] -> i64` の関数だけ、本体の代わりに `extractvalue` 2 個と `call i64 @tsuzuri_cpu_sum_i64(ptr, i64)`・`ret` を出す。
  `emit_program` の末尾は出力が `@tsuzuri_cpu_sum_i64(` を含むときだけ `declare i64 @tsuzuri_cpu_sum_i64(ptr, i64)` を足す。
- driver: `src/driver.rs` の `cpu_runtime` は native で IR が `declare i64 @tsuzuri_cpu_sum_i64(` を含むとき真。task runtime・
  `runtime/io.c` と一つの C ソース（一時ディレクトリの `task.c`）へ連結し、`TSUZURI_CLANG`（既定 `clang`）で
  `-std=c11 -c -O{n}` と `native_compile_args` を付けて compile する。`--cpu native` なら `native_cpu_flag`（`-march=native`／
  `-mcpu=native`）も付く。
- std: `std/Array.tz` の `sum`（`Numeric<'a>`、左から右の `total + value`）、`min`・`max`（`Ord<'a> => ref ['a] -> Maybe<ref 'a>`、
  厳密な `<`／`>` なので同値なら最初の位置）。整数の `+` は幅ごとに折り返す（docs/language.md）。整数型は `i8`〜`i64`、
  `i8u`〜`i64u`、`i128`、`i128u`（`byte`／`ubyte` は `i8`／`i8u` の別名）。
- テスト: `tests/cpu_dispatch.rs::only_native_i64_standard_sum_requests_cpu_dispatch`（native IR に呼び出しと宣言が 1 回ずつ、
  `ref [f64]` は `fadd double` のまま、`emit`・wasm・非同梱 std・未使用では出ない、IR の決定性）。`tests/cpu_dispatch.mjs`
  （`tests/fixtures/cpu_dispatch` を generic／native × `-O0`／`-O3` で object にし、C host から 8 thread で呼ぶ。FORCE の成功と失敗、
  WASM は import なし、`cpu.c` の IR に `cmpxchg`・`load atomic` があり `ifunc|__cpu_model` がない、arm64 macOS では
  `x86_64-apple-macos11` へ cross-compile して `+avx2`・`xgetbv` を確認）。`tests/cpu_runtime.c` は
  `#include "../src/runtime/cpu.c"` で内部関数を直接検査する C テストで、どの harness からも呼ばれていない。
- 計測: `benchmarks/run-dispatch.mjs`（docs/benchmarks.md の CPU dispatch 比較、9 回中央値と feature・variant の記録）。
- 開発機: arm64 macOS（`sw_vers -productVersion` は 27.0、Apple clang 21.0.0）。Rosetta 2 が入っておらず、x86_64 の実行ファイルは
  起動できない。x86 の variant はこの機械では compile と生成コードの確認だけができる。

### 再現（検証済み）

x86_64 へ cross-compile した `cpu.c` はこの機械では実行できない（起票時に確認）。

```sh
mkdir -p /tmp/tz-work-F08 && cd /tmp/tz-work-F08
printf '#include <stdio.h>\n#include <stdint.h>\nuint64_t tsuzuri_cpu_features(void);int tsuzuri_cpu_variant(void);\nint main(void){printf("%%llu %%d\\n",(unsigned long long)tsuzuri_cpu_features(),tsuzuri_cpu_variant());return 0;}\n' > m.c
clang -target x86_64-apple-macos11 -std=c11 -O2 m.c /Users/tmidorikawa/Documents/git/Tsuzuri/src/runtime/cpu.c -o x86probe
arch -x86_64 ./x86probe
# arch: posix_spawnp: ./x86probe: Bad CPU type in executable
```

## 仕様

### 前提とする他チケットのインターフェース

- F05（done）: `src/runtime/cpu.c` の公開入口 `tsuzuri_cpu_features`・`tsuzuri_cpu_variant`・`tsuzuri_cpu_sum_i64` と
  `TSUZURI_CPU_FORCE=baseline|sse4.2|avx2`。F08 は内部を作り直すが、この 3 入口の名前・signature・意味と FORCE の値、
  失敗時の stderr 文言を変えない（`tests/cpu_dispatch.mjs` と `benchmarks/run-dispatch.mjs` がそのまま通る）。
- F04（done）: `SimdType`・`Type::Simd`。Phase 1 は触らない。Phase 2 が拡張する（D7）。
- C08（Phase 2 の `Simd.store` だけ）: 排他スライス `ref mut [T]` を関数引数に取れること。C08 の完了前は `Simd.store` を足さない。

### 他チケットへの提供インターフェース（PR05 など）

PR05（UTF kernel）はこの名前をそのまま使う。F08 の完了後に名前を変えるときは PR05 を同時に直す。

| 種類 | 名前 | 内容 |
| --- | --- | --- |
| C 定数 | `TZ_CPU_BASELINE`・`TZ_CPU_SSE42`・`TZ_CPU_AVX2`（新規） | level 0／1／2。`tsuzuri_cpu_variant` の戻り値と同じ |
| C 関数 | `static int tz_cpu_level(void)`（新規） | 一度だけ決めて `_Atomic int` に cache した level。FORCE を反映し、利用不能・未知なら `tz_cpu_unavailable` |
| C 属性 | `TZ_CPU_TARGET_SSE42`・`TZ_CPU_TARGET_AVX2`（新規） | `TZ_CPU_X86` のときだけ定義。`__attribute__((target("sse4.2"), noinline))` と `target("avx2")` |
| C macro | `TZ_CPU_PICK(name, ...)`（新規） | `name##_avx2`・`name##_sse42`・`name##_baseline` を level で選んで `return` する。x86 以外は `tz_cpu_level()` を呼んでから baseline |
| 公開記号 | `tsuzuri_cpu_<op>_<type>` | `TZ_CPU_API`（weak／hidden）。接頭辞 `tsuzuri_cpu_` の `declare` が IR にあると driver が `cpu.c` を同梱する |
| Rust 定数 | `src/llvm.rs` の `CPU_KERNELS`（新規） | `(記号, LLVM の戻り値型)` の固定順の表。宣言はこの順で出す |
| Rust 関数 | `src/llvm.rs` の `cpu_kernel(function: &CheckedFunction) -> Option<&'static str>`（新規） | std 関数を kernel 記号へ対応させる唯一の場所。PR05 は arm と表の行を足す |

PR05 の kernel は `src/runtime/cpu.c` に置く（別ファイルにしない。`tz_cpu_level` を static のまま共有するため）。
`trusted_array` は `std/Array.tz` だけを見るので、PR05 は自分の std ファイルについて同じ一致検査を `emit_native_build` に足す。

### 構文・型規則

Phase 1 は構文・型・公開 API を変えない（D1）。`std/Array.tz` に非公開の `min_index`・`max_index`（新規、`private def`）を足し、
`min`・`max` をそれを呼ぶ形に書き換える（D5）。公開 signature と結果は変わらない。

### 対象の関数と kernel

`emit_native_build` で `trusted_array` が真、target が native のときだけ、単相化後の次の std 関数本体を kernel 呼び出しに置き換える（D2）。
`T` は `i8`・`i16`・`i32`・`i64`・`i8u`・`i16u`・`i32u`・`i64u`（`byte`／`ubyte` は別名なので同じ）。

| std 関数（単相化後の signature） | 公開記号 | C signature |
| --- | --- | --- |
| `Array.sum :: ref [T] -> T` | `tsuzuri_cpu_sum_i8`・`_i16`・`_i32`・`_i64`（符号なしも同じ記号。D6） | `int8_t tsuzuri_cpu_sum_i8(const int8_t *, int64_t)` など |
| `Array.min_index :: ref [T] -> i64` | `tsuzuri_cpu_min_i8`〜`_i64`、`tsuzuri_cpu_min_i8u`〜`_i64u` | `int64_t tsuzuri_cpu_min_i32u(const uint32_t *, int64_t)` など |
| `Array.max_index :: ref [T] -> i64` | `tsuzuri_cpu_max_i8`〜`_i64`、`tsuzuri_cpu_max_i8u`〜`_i64u` | 同上 |

対象外（std の本体のまま）: 浮動小数点すべて、`i128`・`i128u`、`Array.product`・`fold`・`dot` など、`Parallel.sum`（D4）。

### 評価順序・所有権・借用

- kernel は借用した配列を `[0, length)` の範囲で読むだけで、確保・書き込み・callback 呼び出しをしない。block の読み出しは
  `length - index >= lanes` のときだけ行い、末尾を越えて投機的に読まない。unaligned 読み出しは `memcpy` で行う。
- 引数の評価と呼び出しの順は変わらない（置き換えるのは呼ばれる側の本体だけ）。`length < 0` は Tsuzuri から起こらないが、
  `tsuzuri_cpu_sum_i64` と同じく `abort` する。

### 数値・トラップ・native と WASM の差

- `sum`: 幅ごとの 2 の補数の折り返し和。加算は結合的・可換なので、lane 並列の和と左から右の和はビット単位で一致する（D-14）。
  C では符号なし型で計算し、未定義動作（符号付き overflow）を避ける。
- `min_index`／`max_index`: 厳密な `<`／`>` で最初に現れた最小・最大の位置。std の helper とまったく同じ index を返す。空配列では 0
  （helper と同じ。`min`・`max` は空なら helper を呼ばない）。
- 浮動小数点は対象外。左から右の順序（D-14）、NaN の payload と符号付きゼロ（`min` の同値比較）を変えないため（D4）。
- WASM、`--emit llvm`、`llvm::emit`・`emit_target` は常に std の本体を使う。`--wasm-feature simd128` で LLVM が自動ベクトル化しても
  結果は同じ理由で一致する。
- 各 target の baseline: arm64 は Clang の 128-bit vector が NEON（Advanced SIMD は AArch64 の必須機能）。x86-64 は target の既定 ISA
  （Linux・Windows は SSE2、macOS の x86_64 既定 CPU は SSE4.1 を含む。cross-compile の asm に `pminud` が出ることを起票時に確認）。
  Windows は `TZ_CPU_X86` が 0 なので baseline だけ。SVE は使わない（D9）。

### 診断

コンパイラの診断は増えない。実行時の明示失敗だけがある（F05 の既存文言を保つ）。

| 条件 | 出力 | 終了 |
| --- | --- | --- |
| `TSUZURI_CPU_FORCE` が `baseline`・`sse4.2`・`avx2` 以外 | stderr `Tsuzuri CPU runtime: requested variant is unavailable or unknown` | `abort`（SIGABRT） |
| `TSUZURI_CPU_FORCE` の variant を CPU・OS が提供しない（arm64 で `avx2` など） | 同上 | 同上 |
| 公開入口に `length < 0` | なし | `abort` |

判定は最初の kernel 呼び出し（または `tsuzuri_cpu_variant`）で一度だけ行う。kernel を一度も呼ばないプログラムは FORCE の値で失敗しない。

### 資源上限

- kernel は固定サイズの vector 変数だけを使い、stack を入力長に比例して使わない。
- 同梱される `cpu.c` は 20 入口と 3 level 分の clone（60 関数）を含む。どれか一つの kernel を使う native 成果物は全体を link する。
  大きさは手順 10 で before／after を記録し、閾値は置かない（削減は PM08 の範囲）。

### 例

次のプログラムは HEAD で `515305399` を出力する（`/tmp/tz-work-F08/forms/Main.tz`、起票時に確認）。直接呼び出し、型クラス制約付き
の汎用 wrapper、関数値、部分参照の 4 形はすべて単相化後の同じ `Array.sum`（`ref [i32] -> i32`）に届く。F08 の後は native build の
IR に `call i32 @tsuzuri_cpu_sum_i32(` がちょうど 1 回だけ現れ、出力は変わらない。

```tsuzuri
def next :: i64 -> i64
fn next state = state * 6364136223846793005 + 1442695040888963407

def data :: i64 -> [i32]
fn data length = Array.init length (index -> (next (index + 1)) as i32)

def total :: Numeric<'a> => ref ['a] -> 'a
fn total values = Array.sum values

let values = data 1000
let f = Array.sum
let a = Array.sum (ref values)
let b = total (ref values)
let c = f (ref values)
let d = Array.sum (ref values[3..997])
(a as i64) + (b as i64) + (c as i64) + (d as i64)
```

### Phase 2（設計方針。要承認 D7）

- `SimdType` に `width: u16`（128 か 256）を足し、`lanes()` を `width / bits` にする。256-bit 型は既存の型族の lane 数違いとして
  `i8x32`・`i16x16`・`i32x8`・`i64x4`・`i8ux32`・`i16ux16`・`i32ux8`・`i64ux4`・`f32x8`・`f64x4`・`mask8x32`・`mask16x16`・`mask32x8`・`mask64x4`
  を受ける。別の型族にしないので、`Simd.*` の builtin と演算子は同じ `src/llvm_simd.rs` の lowering を通り、同じ演算の builtin 形と
  演算子形が別の経路を選ばない。i8x32 のために `Simd.of_lanes32`（新規）を足す。
- 意味（lane ごとの折り返し、shift 量のマスク、NaN、`sum_lanes` の lane 0 からの順序）は 128-bit 型と同じ。`--cpu generic` の x86 と
  arm64 では LLVM が 2 × 128-bit に分割する。WASM は simd128 指定時に 2 × v128、それ以外は scalar。
- `storage_layout` の align 上限（16）のため、256-bit の load／store は `align 16` を明示する（GUIDE §6.3 のレイアウト項目）。
- `Simd.store (ref mut dst) index vector` は C08 の完了後。全 lane の境界を書き込み前に検査する。

### Phase 3（設計方針。要承認 D8・D9）

- 利用者関数の多版化: 宣言に対象 ISA を付けた関数を、同じ型付き IR から ISA ごとに `"target-features"` 属性付きで複製し
  （名前は `{symbol}.cpu.avx2` のように決定的）、`tsuzuri_cpu_variant` で選ぶ stub を出す。構文は D8。
- AVX-512（F・BW・VL と XCR0 の bit 5–7）は feature bit2、SVE／SVE2 は F05 が予約した bit16／bit17。採否と kernel は実測後（D9）。

## 設計

### データ構造

`src/llvm.rs` に表と対応関数を足す。`CheckedFunction` の field は `FunctionEmitter::emit` の既存条件と同じものを使う。

```rust
const CPU_KERNELS: [(&str, &str); 20] = [
    ("tsuzuri_cpu_sum_i8", "i8"), ("tsuzuri_cpu_sum_i16", "i16"),
    ("tsuzuri_cpu_sum_i32", "i32"), ("tsuzuri_cpu_sum_i64", "i64"),
    ("tsuzuri_cpu_min_i8", "i64"), /* min: i16, i32, i64, i8u, i16u, i32u, i64u */
    ("tsuzuri_cpu_max_i8", "i64"), /* max: 同じ 8 型 */
];

fn cpu_kernel(function: &CheckedFunction) -> Option<&'static str> {
    if function.origin.module != ModuleOrigin::Std || function.module != "Array" {
        return None;
    }
    let [Type::Reference(array, false)] = function.signature.parameters.as_slice() else { return None };
    let Type::Array(element) = array.as_ref() else { return None };
    let Type::Integer(bits @ (8 | 16 | 32 | 64), signed) = **element else { return None };
    let (operation, result) = match function.name.split(".$mono.").next()? {
        "sum" => ("sum", Type::Integer(bits, signed)),
        "min_index" => ("min", Type::I64),
        "max_index" => ("max", Type::I64),
        _ => return None,
    };
    if function.signature.result != result {
        return None;
    }
    let unsigned = if operation != "sum" && !signed { "u" } else { "" };
    let symbol = format!("tsuzuri_cpu_{operation}_i{bits}{unsigned}");
    CPU_KERNELS.iter().find(|(name, _)| *name == symbol).map(|(name, _)| *name)
}
```

表の順は sum（8, 16, 32, 64）、min（i8, i16, i32, i64, i8u, i16u, i32u, i64u）、max（同じ順）で固定する。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| runtime | `src/runtime/cpu.c` | `tz_cpu_decode`, `tsuzuri_cpu_features` | 変更なし |
| runtime | `src/runtime/cpu.c` | `tz_cpu_select`（新規） | `(features, forced)` から level か -1 を返す純関数。FORCE の解釈をここへ移す |
| runtime | `src/runtime/cpu.c` | `tz_cpu_level`（新規） | `tz_cpu_resolve`・`tz_sum_function`・`tz_cpu_sum_cached` を置き換える。`static _Atomic int` を acquire load、未決定なら `tz_cpu_select` の結果を acq_rel cmpxchg |
| runtime | `src/runtime/cpu.c` | `tz_cpu_sum_baseline`・`tz_cpu_sum_sse42`・`tz_cpu_sum_avx2` | 削除。`TZ_CPU_SUM` の生成物 `tz_cpu_sum_i64_baseline` などに置き換わる |
| runtime | `src/runtime/cpu.c` | `TZ_CPU_SUM`・`TZ_CPU_BEST`・`TZ_CPU_LEVEL_KERNELS`・`TZ_CPU_PICK`・`TZ_CPU_ENTRIES`（新規 macro） | kernel の本体、level ごとの 20 clone の生成、選択、公開入口の生成 |
| runtime | `src/runtime/cpu.c` | `tsuzuri_cpu_variant` | `return tz_cpu_level();` |
| runtime | `src/runtime/cpu.c` | `tsuzuri_cpu_sum_i64` と 19 入口（新規） | `length < 0` で `abort`、`TZ_CPU_PICK` |
| std | `std/Array.tz` | `min`, `max`, `min_index`（新規）, `max_index`（新規） | 走査を helper へ移す（D5） |
| IR | `src/llvm.rs` | `CPU_KERNELS`（新規）, `cpu_kernel`（新規） | 上記 |
| IR | `src/llvm.rs` | `FunctionEmitter::emit` | `cpu_dispatch` 分岐の条件を `cpu_kernel(self.function)` に替え、`call {ret} @{symbol}(ptr, i64)` と `ret` を出す。`{ret}` は `self.ty(&self.function.signature.result)` |
| IR | `src/llvm.rs` | `emit_program` | 単一の `tsuzuri_cpu_sum_i64` 宣言を `CPU_KERNELS` の順の loop に替える（出力が `@{symbol}(` を含む行だけ） |
| IR | `src/llvm.rs` | `emit_native_build`, `Instrumentation`, `Globals` | 変更なし（`trusted_array` と `cpu_dispatch` をそのまま使う） |
| driver | `src/driver.rs` | `cpu_runtime` の条件 | `text.lines().any(\|line\| line.starts_with("declare ") && line.contains(" @tsuzuri_cpu_"))` |
| test | `tests/cpu_runtime.c` | 全体 | 新しい内部名へ更新し、全 kernel・全 level・`tz_cpu_select` を検査する。`tests/cpu_kernels.mjs` から実行する |

### 生成 IR とランタイム

置き換えた関数の IR は次の形になる（値の名前は実際の出力に従う。関数の前後は既存の `emit` のまま）。

```llvm
  %t1 = extractvalue %tz.array %p0, 0
  %t2 = extractvalue %tz.array %p0, 1
  %t3 = call i32 @tsuzuri_cpu_sum_i32(ptr %t1, i64 %t2)
  ret i32 %t3

declare i32 @tsuzuri_cpu_sum_i32(ptr, i64)
```

`i8`・`i16` を返す宣言に `signext` を付けない。呼び出し側は下位ビットだけを使うので C の `int8_t` 戻り値と互換である。

`cpu.c` の kernel の形（arm64 で `-O0`・`-O3` の実行、x86_64 への cross-compile で `ymm` 命令が出ることを起票時に
`/tmp/tz-work-F08/proto/proto.c` で確認）:

```c
#define TZ_CPU_SUM(NAME, T, U, BYTES, ATTR) \
    ATTR static T NAME(const T *values, int64_t length) { \
        typedef U tz_vector __attribute__((vector_size(BYTES))); \
        enum { lanes = BYTES / sizeof(T) }; \
        tz_vector total = {0}; \
        int64_t index = 0; \
        for (; length - index >= lanes; index += lanes) { \
            tz_vector value; \
            memcpy(&value, values + index, BYTES); \
            total += value; \
        } \
        U result = __builtin_reduce_add(total); \
        for (; index < length; ++index) result += (U)values[index]; \
        return (T)result; \
    }
```

`TZ_CPU_BEST(NAME, T, BYTES, ATTR, BETTER, ELEMENTWISE, REDUCE)` は同じ形で、`BETTER` に `<`／`>`、`ELEMENTWISE` に
`__builtin_elementwise_min`／`max`、`REDUCE` に `__builtin_reduce_min`／`max` を渡す。`T` は符号を持つ要素型のまま使う
（builtin が要素型の符号で smin／umin を選ぶ）。block 幅 `BYTES` は baseline と sse4.2 が 16、avx2 が 32（D12）。
`TZ_CPU_LEVEL_KERNELS(LEVEL, BYTES, ATTR)` が 1 level 分の 20 clone（`tz_cpu_<op>_<type>_<LEVEL>`）を出し、baseline は常に、sse42 と
avx2 は `#if TZ_CPU_X86` の中でだけ展開する。

### アルゴリズム

```text
sum(values, length):
  total = 0 vector; 全 block を lane ごとに加算（折り返し）
  result = reduce_add(total); 末尾を scalar で加算; return result

best(values, length, better):             # min は better = <、max は >
  if length <= 0: return 0
  best = values[0]; index = 0
  if length >= lanes: acc = block 0; 残りの block で acc = elementwise(acc, block); best = reduce(acc); index = 完了位置
  末尾を scalar で better なら best を更新
  index = 0; block ごとに any(block == best) なら止まる   # 2 回目の走査
  while values[index] != best: index += 1
  return index                             # 最初に現れた位置
```

最小・最大の値は順序によらず一意なので、2 回目の走査で最初の位置を求めれば scalar helper と同じ index になる。

## 実装手順

Phase 1 だけを行う。各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N を必ず見る（GUIDE §3.1）。`cpu.c` と `std/Array.tz` は `include_str!` で埋め込まれるので、変更後の E2E の前に
必ず `cargo build --release --locked` する。

### 手順 1: ベースラインを取る

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行する。「例」のプログラムを `/tmp/tz-work-F08/forms/Main.tz` に置く。基準のコンパイラを保存する。
- 確認: 次がすべて成功する。`run` は `515305399`、`cpu_dispatch` は `1 passed`、`cpu_runtime` は arm64 で `features=0 variant=0`。
  stack-depth の 3 テストと特殊化上限のテストはそれぞれ `1 passed`。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
mkdir -p target/perf/F08/before target/perf/F08/after && cp target/release/tsuzuri target/perf/F08/baseline-tsuzuri
target/release/tsuzuri run /tmp/tz-work-F08/forms
target/release/tsuzuri build /tmp/tz-work-F08/forms -O3 -o /tmp/tz-work-F08/forms-before && ls -l /tmp/tz-work-F08/forms-before
cargo test --locked --test cpu_dispatch
node tests/cpu_dispatch.mjs target/release/tsuzuri
clang -std=c11 -O2 -pthread tests/cpu_runtime.c -o /tmp/tz-work-F08/cpu_runtime && /tmp/tz-work-F08/cpu_runtime
cargo test --locked --test polymorphism bounds_type_growing_polymorphic_recursion
cargo test --locked --lib bounds_recursive_and_flat_expression_depth
cargo test --locked --test computations bounds_nested_builder_expansion_not_just_source_syntax
cargo test --locked --test polymorphism honors_the_exact_specialization_limit
```

### 手順 2: `min`・`max` の走査を非公開 helper へ移す

- 変更: `std/Array.tz` の `min`・`max`、`min_index`（新規）・`max_index`（新規）。
- 内容: 既存の while loop をそのまま helper へ移し、`min` は次の形にする（`max` は `>` と `max_index`）。同じ形の利用者コードが
  HEAD で動くことを `/tmp/tz-work-F08/minidx` で確認済み（`-294`）。

```tsuzuri
private def min_index :: Ord<'a> => ref ['a] -> i64
fn min_index values =
    let mut best = 0
    let mut index = 1
    while index < values.length do
        if values[index] < values[best] then best = index
        index = index + 1
    best

def min :: Ord<'a> => ref ['a] -> Maybe<ref 'a>
fn min values =
    if values.length == 0 then Maybe.None else Maybe.Some (ref values[min_index values])
```

- 確認: `cargo build --release --locked && cargo test --locked` が成功する（`tests/array_bulk.rs` の `Array.min` を含む）。
  手順 1 の特殊化上限のテストが `1 passed`。

### 手順 3: level の cache と FORCE の解釈を一か所にする

- 変更: `src/runtime/cpu.c` の `tz_cpu_select`（新規）・`tz_cpu_level`（新規）・`TZ_CPU_BASELINE` など（新規）・`TZ_CPU_PICK`（新規）、
  `tsuzuri_cpu_variant`、`tsuzuri_cpu_sum_i64`。`tests/cpu_runtime.c`。
- 内容: 既存の 3 kernel を `tz_cpu_sum_i64_baseline`・`_sse42`・`_avx2` へ改名し、`tz_cpu_resolve`・`tz_sum_function`・
  `tz_cpu_sum_cached` を消す。`tz_cpu_select(features, forced)` は NULL なら features から最上位の level、`"baseline"` なら 0、
  `"sse4.2"`／`"avx2"` は feature bit があればその level、なければ -1、ほかの文字列は -1 を返す。`tz_cpu_level` は -1 のとき
  `tz_cpu_unavailable` を呼ぶ。`tests/cpu_runtime.c` は新しい名前に直し、`tz_cpu_select` の 7 通り（NULL×features 0／1／3、
  `baseline`、`sse4.2` と features 0、`avx2` と features 3、`sve`）を `assert` する。
- 確認: `cargo build --release --locked && node tests/cpu_dispatch.mjs target/release/tsuzuri` が成功する。
  `clang -std=c11 -O0 -pthread tests/cpu_runtime.c -o /tmp/tz-work-F08/cpu_runtime && /tmp/tz-work-F08/cpu_runtime` と `-O3` が
  `features=0 variant=0` を出す。

### 手順 4: macro で全 kernel と入口を生成する

- 変更: `src/runtime/cpu.c` の `TZ_CPU_SUM`・`TZ_CPU_BEST`・`TZ_CPU_LEVEL_KERNELS`・`TZ_CPU_ENTRIES`（新規）、19 入口（新規）。
  `tests/cpu_runtime.c`。
- 内容: 手順 3 の i64 の 3 kernel を macro の生成物に置き換え、「生成 IR とランタイム」の形で 20 clone × level を出す。ファイル先頭に
  `#if !__has_builtin(__builtin_reduce_add) || !__has_builtin(__builtin_elementwise_min)` なら
  `#error "Tsuzuri CPU runtime requires Clang vector builtins"` を置く。`tests/cpu_runtime.c` は各 kernel の各 level を
  scalar の参照（C の単純 loop。符号なしで加算、厳密比較で最初の位置）と長さ 0〜257・offset 0〜3 で照合する。
- 確認: arm64 で `tests/cpu_runtime.c` が `-O0`・`-O3` で成功する。x86 の cross-compile が成功し、avx2 clone に `ymm` がある。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
clang -target x86_64-apple-macos11 -std=c11 -O3 -S src/runtime/cpu.c -o /tmp/tz-work-F08/cpu-x86.s
awk '/^_tz_cpu_min_i32_avx2:/,/retq/' /tmp/tz-work-F08/cpu-x86.s | grep -c ymm   # 1 以上
clang -target x86_64-apple-macos11 -std=c11 -O0 -c src/runtime/cpu.c -o /tmp/tz-work-F08/cpu-x86-O0.o
grep -cE "ifunc|__cpu_model" /tmp/tz-work-F08/cpu-x86.s   # 0
```

### 手順 5: driver の同梱条件を一般化する

- 変更: `src/driver.rs` の `cpu_runtime`。
- 内容: 条件を「native で、`declare ` で始まり ` @tsuzuri_cpu_` を含む行がある」に替える。IR はまだ変えないので挙動は同じ。
- 確認: `cargo build --release --locked && node tests/cpu_dispatch.mjs target/release/tsuzuri` が成功する。

### 手順 6: IR の置き換えと宣言

- 変更: `src/llvm.rs` の `CPU_KERNELS`（新規）・`cpu_kernel`（新規）・`FunctionEmitter::emit`・`emit_program`。
- 内容: 「設計」のとおり。`emit` の分岐条件は `self.globals.cpu_dispatch` と `cpu_kernel(self.function)` の `Some` だけにする。
- 確認: `cargo test --locked --test cpu_dispatch` が `1 passed`（i64 の IR は不変）。`cargo build --release --locked` の後、
  `target/release/tsuzuri run /tmp/tz-work-F08/forms` が `515305399`。

### 手順 7: Rust テスト

- 変更: `tests/cpu_dispatch.rs`。
- 内容: 「テスト計画」の 4 テストを足す。
- 確認: `cargo test --locked --test cpu_dispatch` が `5 passed`。

### 手順 8: E2E suite `cpu_kernels`

- 変更: `tests/fixtures/cpu_kernels/Main.tz`（新規）、`tests/cpu_kernels.mjs`（新規）。
- 内容: 「テスト計画」の E2E。`tests/cpu_dispatch.mjs` の `execute` と C host の生成方法を写す。
- 確認: `node tests/cpu_kernels.mjs target/release/tsuzuri` が成功し、arm64 では最後に
  `cpu_kernels: x86 variants cross-compiled; execution requires an x86 host` を出す。

### 手順 9: 既存の suite

- 確認: `cargo test --locked` が成功する。`node tests/cpu_dispatch.mjs`・`node tests/simd.mjs`・`node tests/features.mjs`（いずれも
  `target/release/tsuzuri`）が成功する。手順 1 の stack-depth と特殊化上限の 4 テストが成功する。

### 手順 10: 生成コード・大きさ・計測の記録

- 内容: `target/release/tsuzuri build /tmp/tz-work-F08/forms -O3 -o /tmp/tz-work-F08/forms-after` の大きさを手順 1 と比べて記録する。
  手順 4 の x86 asm の確認結果を記録する。`node benchmarks/run-dispatch.mjs` を before（`target/perf/F08/baseline-tsuzuri`）と after で
  交互に 9 回ずつ実行し、`target/perf/F08/before/`・`after/` に JSON を保存して i64 和の中央値が悪化していないことを見る（arm64 は baseline
  だけの比較）。速度の優位は主張しない。
- 確認: docs/benchmarks.md に記録する表が埋まる（手順 11）。

### 手順 11: 文書

- 変更: 「ドキュメント」の各ファイル。
- 確認: `node scripts/check-docs.mjs _docs/guides/performance.md _docs/library-reference/arrays-and-lists.md _docs/feature-status.md` が成功する。

### 手順 12: 最終確認

- 確認: 手順 9 のコマンドと `node tests/cpu_kernels.mjs target/release/tsuzuri`、`git diff --check` が成功する。GUIDE §13 の完了報告を書く。

## テスト計画

### Rust テスト

`tests/cpu_dispatch.rs` に足す。source は Rust の loop で 8 型分を生成し、IR は `emit_native_build`（同梱 std を含む `sources`）で得る。

| テスト | 検査すること |
| --- | --- |
| `only_native_i64_standard_sum_requests_cpu_dispatch`（既存） | 変更しない。成功し続ける |
| `integer_sum_min_max_request_their_kernels`（新規） | 8 型の `sum_{t}`・`min_{t}`・`max_{t}` の export から、`call i8 @tsuzuri_cpu_sum_i8(` など 20 記号の呼び出しがある（`sum_i32` の呼び出しは `i32` と `i32u` の 2 回、ほかの sum も同様）。各 `declare` はちょうど 1 回で、出現位置が `CPU_KERNELS` の順に増える。2 回の emit で IR が一致する |
| `floats_and_wide_integers_keep_the_std_body`（新規） | `ref [f32]`・`ref [f64]` の `sum`・`min`、`ref [i128]`・`ref [i128u]` の `sum`・`max` の IR に `tsuzuri_cpu` がない |
| `equivalent_call_forms_share_one_kernel_call`（新規） | 「例」の 4 形の source で `call i32 @tsuzuri_cpu_sum_i32(` がちょうど 1 回 |
| `cpu_kernels_are_native_build_only`（新規） | 同じ 8 型の source で、`llvm::emit`、`llvm::emit_target(.., true)`、std を含まない `sources[..1]` の `emit_native_build` に `tsuzuri_cpu` がない |

### E2E

`tests/cpu_kernels.mjs`（新規）。fixture `tests/fixtures/cpu_kernels/Main.tz` は 24 export（`sum_{t}`・`min_{t}`・`max_{t}`、`t` は 8 型）と
`forms :: i64`（「例」の `main` の本体）を持つ。`min_{t}` は `Array.min` の結果を `match` で値に直し、空なら 0 を返す（`match` の形は
`/tmp/tz-work-F08/minidx` と同じ）。

- データ: 64-bit LCG `state = state * 6364136223846793005 + 1442695040888963407 (mod 2^64)`。要素 k は `state_{k+1} >> (64 - bits)` の
  ビット列。パターンは P0 乱数（seed 1）、P1 全要素が型の最小値、P2 全要素が最大値、P3 `state >> 62`（0〜3、同値が多い）、
  P4 `k` の折り返し。各型 4,100 要素。
- 長さは 0〜70、127〜129、255〜257、4095〜4097、offset は 0〜3。
- C host（mjs が生成）は各組で `tz_sum_{t}`・`tz_min_{t}`・`tz_max_{t}` と、長さ 1 以上なら `tsuzuri_cpu_min_{t}`・`tsuzuri_cpu_max_{t}`
  （index）を呼び、1 行ずつ出す。独立参照は mjs が JS `BigInt` で同じデータを作り、左から右の和を `BigInt.asIntN`／`asUintN` で
  幅に折り返し、最小・最大は厳密比較で最初の index を求める。全行を照合する。
- build は generic／native × `-O0`／`-O3` の object。各実行ファイルを FORCE なし、`baseline`、feature がある `sse4.2`・`avx2` で
  実行し、全 variant の stdout が文字列として一致する。feature がない variant と `sve`・`unknown` は非 0 終了と
  `requested variant is unavailable or unknown`。
- `forms`: native の `tz_forms` と、wasm32 の `-O0`／`-O3` × `--wasm-feature simd128` の有無の `tz_forms` が、JS `BigInt` の参照
  （`Array.init` の i32 列の和 3 回と `[3..997)` の和、各 i32 に折り返してから i64 で加算）と一致する。WASM の import は空。
- `tests/cpu_runtime.c` を `-O0`・`-O3` で compile して実行し、終了 0。
- arm64 macOS では手順 4 の x86 cross-compile の検査（`ymm`、`-O0` object、`ifunc|__cpu_model` なし）を行い、実行はしない。
- kernel は確保しないので `live` の計数は対象外。

### 既存テストへの影響

- `tests/cpu_runtime.c`: 内部名の変更に合わせて直す（公開入口の検査は同じ）。
- `Array.min`・`Array.max` を使うプログラムの IR（全 target）に helper 関数が 1 つ増える。起票時に `tests/` で `Array.min`・
  `Array.max` を使うのは `tests/array_bulk.rs` の値の検査だけで、IR の文字列照合はない。ほかは「なし」。

### 性能

閾値は置かない。手順 10 の記録だけ。x86 の速度は x86 の機械で `benchmarks/run-dispatch.mjs` を実行した場合だけ docs/benchmarks.md に書く。

## ドキュメント

- `docs/language.md`: 冒頭の「native exe/objectの同梱`Array.sum<i64>`は実行時CPU選択の対象です」の段落を、8 整数型の
  `Array.sum`・`Array.min`・`Array.max` へ更新する。「floatの順序や利用者関数・WASMを変えません」は保つ。
- `docs/architecture.md`「性能設計の原則」の CPU dispatch の段落: `tz_cpu_level`、kernel の一覧、Clang vector builtin への依存、
  driver の同梱条件（`tsuzuri_cpu_` の宣言）、「x86 は cross-compile のみ」。
- `_docs/guides/performance.md`「実行時 CPU dispatch」、`_docs/library-reference/arrays-and-lists.md`「集計と List API」（結果は変わらない旨）。
- `docs/benchmarks.md` の CPU dispatch 比較: 手順 10 の記録、生データの保存先、そろえられなかった条件。
- `README.md` のテスト一覧に `node tests/cpu_kernels.mjs target/release/tsuzuri` を足す。
- `_docs/feature-status.md` の F08 行と `_features/README.md` の状態欄は GUIDE §10 に従う（Phase 1 だけ完了したことを明記する）。

## 受け入れ条件

- [ ] 8 整数型の `Array.sum`・`Array.min`・`Array.max` が native の exe／object で kernel を呼び、WASM と `--emit llvm` は std の本体のまま。
- [ ] 全 variant（arm64 では baseline、x86 の機械では sse4.2・avx2 も）の結果が JS `BigInt` の参照とビット単位で一致する。
- [ ] 利用できない・未知の `TSUZURI_CPU_FORCE` は既存の文言で停止する。
- [ ] avx2 clone に 256-bit 命令があることを cross-compile の asm で確認し、x86 の実行が未検証なら文書にそう書く。
- [ ] 「例」の 4 形の呼び出しが一つの kernel 呼び出しを共有する。
- [ ] Rust テスト 5 件、`tests/cpu_kernels.mjs`、既存 suite、stack-depth と特殊化上限のテストが成功する。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- 未使用の `static` kernel は消える。x86 の asm を確かめるときは入口から参照された clone を見る（起票時の試作で、呼び出しのない
  avx2 関数が asm から消えた）。
- vector 型の変数へ配列を pointer の cast で読むと、`vector_size` の自然な align（16／32）を仮定した aligned load になり x86 で落ちる。
  必ず `memcpy` で読む。
- C の符号付き overflow は未定義動作。sum は符号なしの vector と scalar で計算する。
- `value == best`（vector と scalar）は -1／0 の符号付き vector になる。`__builtin_reduce_or` で判定する。
- driver は task runtime・`cpu.c`・`io.c` を一つの C ソースへ連結する。`cpu.c` の static 名は `tz_cpu_` で始め、衝突させない。
- `--cpu native` では `cpu.c` 全体が `-march=native` で compile され、baseline clone も AVX2 を使いうる。variant ごとの速度比較は
  `--cpu generic` で行う。`-O0` の build では kernel も `-O0` になるので速度を判断しない。
- `tests/cpu_dispatch.mjs` は `cpu.c` の IR に `cmpxchg` と `load atomic` があることを検査する。`tz_cpu_level` の cache を atomic で
  書き、`tsuzuri_cpu_features` の cache も残す。
- baseline の x86-64（SSE2）には `pcmpgtq` がなく、i64 の min／max は LLVM が長い命令列へ展開する。正しいが速くない。主張しない。
- この開発機には Rosetta 2 がない。system software の導入はしない。x86 の実行は x86 の機械がある場合だけ行う。
- `TSUZURI_CPU_FORCE` は process で一度だけ読む。test は variant ごとに process を分ける。
- macOS の `clang -r -nostdlib` は weak／hidden 記号を局所化する。relocatable link の `-Wl,-keep_private_externs` を外さない
  （C host が `tsuzuri_cpu_min_i32` を直接呼べなくなる）。

## 対象外

- 浮動小数点の kernel、`i128`・`i128u`、`Array.product`・`fold`・`dot`、`Parallel.sum` の chunk kernel（D4）。
- WASM の実行時選択（WASM は build 時の feature で固定）、利用者が書く ISA 固有 intrinsics（`_mm256_*` の公開）、自動並列化、GNU ifunc。
- 256-bit 型（Phase 2）、利用者関数の多版化・AVX-512・SVE／SVE2（Phase 3）、unroll 幅の調整（D12）。

## 決定事項

### D1: Phase 1 は言語の型を増やさず、幅の広い SIMD は同梱 kernel の内部だけで使う

- 決定: Phase 1 は `Type::Simd` を変えない。256-bit の命令は `cpu.c` の avx2 clone の中だけで使う。起票時の案（Phase 1 = 256-bit 型）
  から順序を入れ替え、256-bit 型は Phase 2（D7）にする。
- 理由: 利用者関数の多版化（Phase 3）がない間、`--cpu generic` の成果物では利用者が書く `i32x8` の演算は 2 × 128-bit に分割され、
  AVX2 を使えない。同じ集計を `Array.sum` で書くと avx2 kernel、`i32x8` で書くと 2 × 128-bit になり、「幅が広い」と明示した形の方が
  遅い経路を選ぶ。AGENTS.md の「同等の builtin と演算子の形が意図せず別の性能経路を選ばない」に反する。内部 kernel だけなら意味と
  言語の表面を変えずに済む。
- 状態: 既定案（実装者はこの案に従う）

### D2: dispatch の単位は単相化後の std 関数本体

- 決定: 呼び出し位置ではなく、`FunctionEmitter::emit` で std 関数の本体を kernel 呼び出しに置き換える（F05 と同じ）。
- 理由: 直接呼び出し、汎用 wrapper、関数値、部分参照はすべて同じ単相化関数に届くので、どの形でも同じ kernel を通る（AGENTS.md の
  同じ規則）。呼び出し位置ごとの置き換えは形ごとの取りこぼしを生む。
- 状態: 既定案（実装者はこの案に従う）

### D3: 多版化の仕組み

- 決定: `cpu.c` の一つの `_Atomic int` level cache（`tz_cpu_level`）、Clang の vector 拡張・`__builtin_reduce_*`・
  `__builtin_elementwise_*` と `target` 属性による clone、`TZ_CPU_PICK` の switch。GNU ifunc と compiler-rt の CPU model は使わない。
- 理由: kernel ごとの関数ポインター cache より単純で、FORCE がすべての kernel に一様に効く。builtin は型付きの LLVM intrinsic
  （`llvm.vector.reduce.*`、`llvm.smin` など）になり、型と ISA ごとの intrinsics を手で書かずに済む。driver は既に Clang で compile する。
- 状態: 既定案（実装者はこの案に従う）

### D4: Phase 1 の kernel の範囲

- 決定: `i8`〜`i64`・`i8u`〜`i64u` の `Array.sum`・`Array.min`・`Array.max` だけ。浮動小数点、`i128`・`i128u`、`product`、`Parallel.sum` は対象外。
- 理由: 整数の折り返し和と最小・最大の値は順序によらず一意で、ビット単位の一致を保証できる（D-14）。浮動小数点の和は左から右の
  順序が契約で、`min` は NaN の payload・符号付きゼロの同値で位置が結果を変える。`i128` は vector の利点がない。`Parallel.sum` は
  F02 の chunk 契約を持つ別 API。
- 状態: 既定案（実装者はこの案に従う）

### D5: `min`・`max` は非公開 helper の本体を置き換える

- 決定: `std/Array.tz` に `private def min_index`・`max_index` を足し、`min`・`max` は `Maybe.Some (ref values[min_index values])` の形にする。
- 理由: kernel は `ref [T] -> i64` の形だけを扱えばよく、`Maybe<ref T>` の IR を手で組み立てずに済む。索引の境界検査も残る。
- 状態: 既定案（実装者はこの案に従う）

### D6: 記号名と ABI

- 決定: `tsuzuri_cpu_{sum|min|max}_{型名}`。型名は Tsuzuri の名前（`i32u` など）。sum は符号によらずビット列が同じなので
  `i{bits}` の記号を共有する。引数は `(ptr, i64)`、sum は要素型、min／max は `i64` の index を返す。`length < 0` は `abort`。
- 理由: 既存の `tsuzuri_cpu_sum_i64` と一貫し、`CPU_KERNELS` と C の入口が 1 対 1 に対応する。
- 状態: 既定案（実装者はこの案に従う）

### D7: Phase 2 の 256-bit 型

- 決定: 「Phase 2」のとおり、同じ型族に `width` を足して lane 数違いの 14 型を受ける。`Simd.store` は C08 の後。
- 理由: 別の型族にすると builtin と演算子の lowering が分かれる。型名の追加は言語の表面を変える。
- 状態: 要承認（承認前は Phase 2 に着手しない）

### D8: 利用者関数の多版化の構文

- 決定: 起票時の既定案（宣言の修飾 `def name :: ... for cpu [avx2]`）と代替案（manifest で関数名と ISA を列挙）を人間が選ぶ。
  仕組みは「Phase 3」の関数属性による複製と stub。
- 理由: 新しい構文と予約語の扱いは GUIDE D-15 の追加に当たる。
- 状態: 要承認（承認前は Phase 3 に着手しない）

### D9: AVX-512 と SVE／SVE2

- 決定: Phase 1・2 では検出も kernel も足さない。`TSUZURI_CPU_FORCE=avx512` は未知の値として停止する。Phase 3 で実機計測の後に判断する。
- 理由: 周波数低下で遅くなる場合があり、この開発機では x86 も SVE も実行できない。
- 状態: 要承認（承認前は Phase 3 に着手しない）

### D10: x86 の検証範囲

- 決定: arm64 の開発機では x86 の clone を compile と asm の確認だけにする。x86 の機械があれば `node tests/cpu_kernels.mjs` をそこで
  実行し、機械名と結果を docs/benchmarks.md に書く。なければ「cross-compile のみ」と書く。
- 理由: Rosetta 2 がなく、導入は system の変更になる。未実行の経路を実行済みと書かない（AGENTS.md）。
- 状態: 既定案（実装者はこの案に従う）

### D11: PR05 への提供インターフェース

- 決定: 「他チケットへの提供インターフェース」の名前で固定し、PR05 の kernel も `src/runtime/cpu.c` に置く。
- 理由: `tz_cpu_level` を static のまま共有でき、driver の同梱条件（`tsuzuri_cpu_` の宣言）も変えずに済む。
- 状態: 既定案（実装者はこの案に従う）

### D12: block 幅と 2 回走査

- 決定: block は baseline・sse4.2 が 16 bytes、avx2 が 32 bytes。min／max は値を求める走査と最初の位置を探す走査の 2 回。unroll と
  1 回走査は計測で必要が示されてから別チケットで行う。
- 理由: F05 の kernel と同じ幅で、結果に影響しない。1 回走査の index 追跡は複雑で、ビット単位の一致の検証が難しくなる。
- 状態: 既定案（実装者はこの案に従う）

## 実装と検証（2026-10-06）

「C08、A14、A16、F13、F08 の実装をすべて完遂して」という依頼を D7・D8・D9 の承認として扱い、Phase 1 と、設計方針だけだった
Phase 2（256-bit 型と `Simd.store`）・Phase 3（`@cpu` と AVX-512・SVE）を設計して実装した。着手時の HEAD は `ff84e4c`
（ブランチ `Phase7-3`）で、C08・A16・A14・F13 と同じ変更に含めた。x86 と SVE は cross-compile だけを確認し、実機の実行と速度は未確認。

### 実装（Phase 1）

- `src/runtime/cpu.c`: 全面的に書き直した。level（0 baseline、1 SSE4.2、2 AVX2、3 AVX-512、4 SVE、5 SVE2）、feature bit
  （bit0 SSE4.2、bit1 AVX2、bit2 AVX-512 F・BW・CD・DQ・VL と XCR0 の `0xE6`、bit16／bit17 は Linux AArch64 の `getauxval` による SVE／SVE2）、
  `tz_cpu_select`・`tz_cpu_level`（一つの `_Atomic int`）・`tz_cpu_within`、kernel の macro `TZ_CPU_SUM`・`TZ_CPU_BEST`（`TZ_CPU_MIN`・`TZ_CPU_MAX`）・
  `TZ_CPU_LEVEL_KERNELS`・`TZ_CPU_PICK`・`TZ_CPU_ENTRY`、20 の入口 `tsuzuri_cpu_{sum|min|max}_{型名}`、`tsuzuri_cpu_variant`、
  Phase 3 の `tsuzuri_cpu_pick`。block は baseline・sse4.2 が 16、avx2 が 32、avx512 が 64 bytes。
- `std/Array.tz`: 非公開 helper `min_index`・`max_index`（D5）。
- `src/llvm.rs`: `CPU_KERNELS`・`cpu_kernel`、`FunctionEmitter::emit_body` の置き換え、宣言の loop。`src/driver.rs`: `cpu_runtime` は
  `@tsuzuri_cpu_` の関数の宣言があるとき。
- テスト: `tests/cpu_dispatch.rs`（5 件）、`tests/cpu_runtime.c`（decode・select・within、全 kernel・全 level の scalar 参照、8 thread、pick）、
  `tests/fixtures/cpu_kernels/Main.tz`・`tests/cpu_kernels.mjs`（新規）。

### 実装（Phase 2）

- `src/simd.rs`: `SimdType::width`（128・256）と `bytes()`。`named` は lane 数 × 幅が 128 か 256 の名前を受ける。
- `src/check.rs`: builtin `Simd.of_lanes32` と `Simd.store`（`ref mut [lane..] -> i64 -> 'a -> unit`、`SimdNumeric`）。
- `src/llvm_simd.rs`: 両 builtin の lowering（store は全 lane の境界を検査してから `align 1` で書く）、`with_vector_alignment`
  （`emit_program` の最後。256-bit ベクトルを含む型の load／store に `align 16`）。`storage_layout`・`llvm_debug::layout` は 256-bit 型を
  (32, 32)、`llvm_frame::stack_size` は 32 にした。
- テスト: `tests/simd.rs`（`f32x8` などを有効な名前へ移し、`wide_vectors_access_storage_at_most_16_byte_aligned` を追加）、
  `tests/fixtures/simd/Main.tz` と `tests/features.mjs` の `simd` suite（256-bit の 8 型 × 7 seed × 4 shift、float、格納、load、store と trap）。
- VS Code の文法: `i8x32` などの型名、属性 `@cpu`。

### 実装（Phase 3）

- `src/syntax.rs`: `CPU_TARGETS`、`CpuAttribute`、`Program::cpu_attributes`。`src/parser.rs`: `cpu_attribute`（doc comment の後、
  `private`・`export` の前）。`src/formatter.rs`: 正規化。
- `src/check.rs`: `FunctionOrigin::cpu`（生成された helper は 0。`src/polymorph.rs` の具体化は引き継ぐ）、`Type::holds_wide_vector`、
  `validate_cpu_functions`。
- `src/llvm.rs`: `Instrumentation`・`Globals` の `multiversion`、define 行の `"tz-cpu"="<levels>:<関数 id>"`、
  `emit_native_build_for(…, levels)`（`emit_native_build` は `cpu::host_levels()` で呼ぶ）、object を作る `emit_trap_return`。
- `src/llvm_cpu.rs`（新規）: `multiversion`。trap 計装後の IR で、印の付いた関数を `.cpu.baseline` に改名し（debug 情報を保つ）、
  level ごとの `.cpu.<名前>` 版（`"target-features"`、debug 情報なし）と、元の名前の stub を作る。stub は `@"<名前>.cpu"` に
  `tsuzuri_cpu_pick` の結果を monotonic で cache し、switch から tail call する。版の本体が 256-bit ベクトルを含む型で直接呼ぶ関数は
  推移的に同じ level の版を作り、呼び出しを付け替える。
- テスト: `tests/multiversion.rs`（4 件。x86-64・AArch64 Linux の level の IR、debug 情報と trap 位置、`clang -target` の asm、
  構文と型の拒否）、`tests/fixtures/cpu_versions/Main.tz` と `tests/cpu_kernels.mjs` の版の実行（`-O0`・`-O3`、各 `TSUZURI_CPU_FORCE`）。
- 文書: `README.md`、`docs/language.md`（冒頭の段落、`### SIMD 値型`、`### CPU ごとの関数の版（@cpu）`）、`docs/architecture.md`、
  `docs/benchmarks.md`、`_docs/library-reference/simd.md`・`arrays-and-lists.md`、`_docs/guides/performance.md`、
  `_docs/language-reference/lexical-and-layout.md`、`_docs/learn/why-tsuzuri.md`、`_docs/feature-status.md`、`_features/README.md`、
  `_features/GUIDE.md`（D-15・D-30・D-39）。

### 決定事項への追記（チケットから外れた判断）

1. **D12 を改めた。** kernel を一つの accumulator で書くと、arm64 の `run-dispatch.mjs` の dispatch が約 66 ms（F08 前は約 21.5 ms）に
   なった（ベクトル加算の待ち時間が律速）。和と min／max の両方の走査を 4 つの accumulator で 4 block ずつ処理する形に改め、
   F08 前と同等に戻した。結果は加算・min・max の結合則で変わらない。
2. **D9（Phase 3）。** AVX-512 と SVE／SVE2 を検出するようにしたが、実機で測れないので、同梱 kernel の自動選択は AVX2 までにし、
   AVX-512 と SVE の kernel は `TSUZURI_CPU_FORCE` の指定時だけ使う。`TSUZURI_CPU_FORCE=avx512` などは未知の値ではなくなった。
   `@cpu` で明示した版は自動選択でも使う（利用者の明示の指定）。
3. **D7（Phase 2）の格納。** `storage_layout` は LLVM と同じ (32, 32) にし、heap などが 16 bytes までしか揃えない差は load／store の
   `align 16` で埋めた（GUIDE §6.3）。256-bit ベクトルを含まない IR は変わらない。
4. **D8（Phase 3）の構文。** 既定案の `def name :: ... for cpu [avx2]` ではなく、既存の `@literal`・`@checked` と同じ属性の形
   `@cpu ["avx2", "sve"]` を `def` の前に置く形にした。名前は `TSUZURI_CPU_FORCE` と同じ文字列で、未知・重複・空・`def` 以外への指定は `E0002`。
   build 先で選べない名前は無視し、一つのソースに x86 と AArch64 の名前を並べられる。
5. **版の境界の ABI。** 版ごとに 256-bit ベクトルの受け渡しレジスタが異なる（x86 の `ymm` と 2 つの `xmm`）。シグネチャ（レコード・union・
   タプル・固定長配列の中を含む）と関数値の呼び出しに 256-bit ベクトルを通すことを `E1005` にし（ジェネリック関数は具体化した型で検査）、
   直接呼ぶ関数は同じ level の版を作る。union は payload が共有の格納でも保守的に数える。
6. **debug 情報。** 二つの関数が同じ subprogram を持てないので、level ごとの版は debug 情報を外し、stub には元の subprogram を複製した
   subprogram と呼び出し位置を付けた（portable 版が stub に inline されても inline 位置が保たれる）。
7. **対象の build 先。** 版を作るのは `cpu.c` が検出できる x86-64（Windows を除く）と AArch64 Linux だけ。macOS の AArch64、Windows、
   WASM、`--emit llvm`、`--freestanding` では属性を外して portable 版だけにする。

### 確認（Apple M1 Max、macOS 27.0.1、Apple clang 21、Homebrew LLVM 21、rustc 1.98.1、Node v20.19.6）

- Phase 1: `node tests/cpu_kernels.mjs target/release/tsuzuri` は generic・native の `-O0`・`-O3`（features=0）で JS `BigInt` の参照と一致し、
  WASM（±simd128）、`tests/cpu_runtime.c`（`-O0`・`-O3`）、x86 の asm（avx2 の kernel に `ymm`、avx512 の kernel に `zmm`、ifunc なし）、
  AArch64 Linux の compile（`getauxval`）が成功し、最後に `cpu_kernels: x86 variants cross-compiled; execution requires an x86 host` を出す。
  `node tests/cpu_dispatch.mjs`・`node benchmarks/run-dispatch.mjs --quick` も成功した。
- 計測（手順 10）: `run-dispatch.mjs` を F08 前（`target/perf/F08/baseline-tsuzuri`）と後で交互に 9 回ずつ実行し、dispatch の中央値は
  21.542 ms → 21.557 ms（scalar 21.526 → 21.606、C 21.709 → 21.687）。生データは `target/perf/F08/before/`・`after/`、表は docs/benchmarks.md。
  例の実行ファイルは 67,432 → 102,600 bytes（`cpu.c` の object の text は arm64 で 605 → 16,457 bytes、x86-64 で 1,920 → 75,649 bytes）。
  参考として、`cpu.c` を取り込んだ C の harness では、i32 の min の baseline kernel が 1,048,577 要素 × 200 回で約 15.5 ms、std の本体と同じ形の
  C のループが約 410 ms だった（再現用のスクリプトは置いていない。性能の主張には使わない）。
- Phase 2: `tests/simd.rs`、`node tests/features.mjs target/release/tsuzuri simd`（467 ケース、native・WASM の `-O0`・`-O3`）、
  `TSUZURI_ASAN=1` の同 suite、`node tests/simd.mjs`（generic・native と simd128 の有無）が成功した。fixture の IR を
  `clang -target x86_64-apple-macos13 -mavx2 -O3` で compile し、32 bytes 揃えの `vmovaps`／`vmovdqa` が定数プール以外の記憶域を読まないことを確認した。
- Phase 3: `cargo test --locked --test multiversion` は 4 passed。x86-64 の AVX2 版に `ymm`、AVX-512 版にベクトル命令、export（stub と
  portable 版を inline）に `ymm` がないこと、AArch64 Linux の SVE 版の生成、debug 情報付きの console のプログラムが LLVM に debug 情報を捨てられずに
  compile されることを確認した。arm64 の macOS では版を作らないので、`tests/cpu_kernels.mjs` は fixture を `-O0`・`-O3` で build し、
  各 `TSUZURI_CPU_FORCE` で同じ行を出すことだけを確認する。
- 既存の不具合の発見（未修正）: export の C 入口 `tz_<name>` は、内部の関数を `!dbg` なしで呼ぶ。`-g` の library build で
  export がある module は LLVM の検証で「invalid debug info」となり、module の debug 情報が捨てられる（F08 の前から）。
- 全体のゲートで見つけて直した不具合: `node tests/tasks.mjs` の object の link が `tsuzuri_cpu_sum_i64` の未定義で失敗した。host ABI の
  allocator の runtime text が改行で終わらず、kernel の宣言が `}declare ...` と同じ行に続いたため、driver の行頭の判定が `cpu.c` を
  同梱しなかった。宣言（と `@cpu` の版・stub の後に足す行）の前に改行を入れ、`tests/cpu_dispatch.rs` と `tests/multiversion.rs` で
  宣言が行頭にあることを検査する。
- 全体のゲート（C08・A16・A14・F13・F08 の実装後）: `cargo fmt --all -- --check`、`cargo clippy --all-targets --locked -- -D warnings`、
  `RUST_MIN_STACK=4194304 cargo test --locked`（735 passed）、GUIDE §3.1 の stack-depth と特殊化上限の 7 テスト（既定の 2 MiB）、
  `tests/*.mjs`（`windows.mjs` と、WASM を引数に取るホストの `os-wasi-host.mjs` を除く。`wasm64.mjs` は Node 24 で実行。
  `debug_info.mjs` は `-O0` が成功し、`-O3` の `llvm-dwarfdump --verify` は F08 の前の compiler でも同じ箇所で失敗する）、
  `TSUZURI_TEST_ALLOCATOR=host` の `tests/features.mjs`、`TSUZURI_ASAN=1` の `simd`・`mutable_slices`・`fixed_arrays`・`dyn_dispatch`、
  `node scripts/check-docs.mjs`（main から壊れているアンカーを持つ `_docs/examples/README.md`・`_docs/get-started.md` を除く 99 ページ、
  927 リンク、231 例、394 回の native 実行。feature の ID の対応は別に確認）、`sh scripts/check-runtime-includes.sh`（29 ファイル）、
  `git diff --check`、VS Code 拡張の `check-types`・`lint`・`test:unit`。
- IR: 全 fixture と例の `--emit llvm`（native・wasm32）を A14 の後の出力と比べ、129 ファイル中 70 が一致、51 が番号の付け替えだけ、
  変わったのは `Array.min`・`Array.max` の helper が増えた `array_bulk` と fixture を足した `simd`（各 native・wasm32）だった。

### レビュー対応（PR #14）

- `tests/cpu_runtime.c` の `check_kernels` は 8 つの thread で動くのに、検査データを関数内の `static` 配列に置いて各 thread が同時に書いていた
  （同じ値を書いてもデータ競合で未定義動作）。`-fsanitize=thread` で 2 件の data race を再現し、配列を thread ごとの自動変数にして 0 件になった。
  ASan・UBSan と `node tests/cpu_kernels.mjs` も成功。
- 同じ検査は `unsigned char` の配列を `T *` で読んでいた（C の実効型の規則に反する）。要素の幅ごとの型の配列へ `memcpy` し、
  各 kernel にはその型（または対応する符号なしの型）で渡す。
- 値のレイアウトの上限（64 KiB）の見積もりが SIMD 値を一律 16 バイト（`Layouts::size`）または 8 バイト（シグネチャと局所変数を検査する
  `Validation::check`）と数えていたため、`[[i8x32; 1024]; 3]`（実際は 96 KiB）が通っていた。どちらも `bytes()`（8 バイト以上）で数え、
  256-bit は 32 バイト、128-bit は 16 バイトになった。`tests/fixed_arrays.rs` にちょうど 64 KiB の受理と超過の `E1010` を追加した。
