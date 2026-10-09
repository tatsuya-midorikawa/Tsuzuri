# F09: GPU の浮動小数点・64-bit カーネルと実行時接続

| 項目 | 内容 |
| --- | --- |
| ID | F09 |
| 優先度 | P3 |
| 規模 | XL |
| 依存 | F07, (C11) |
| 後続 | – |
| 状態 | doing（Phase 1 done。Phase 2・3 は未完了） |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 要承認: D1（relaxed f32 の言語契約と名前 `Gpu.map_relaxed`・`Gpu.init_relaxed`・`--emit wgsl-relaxed`）, D9（Phase 2: 言語 runtime からの WebGPU 接続と opt-in の WASM import）, D10（Phase 3: Vulkan の strict float・i64・`Gpu.Auto`） |
| 改善する劣位 | C/C++ 比: GPU は実験段階で、成熟した GPU 開発基盤の代替にならない（[なぜ Tsuzuri か](https://github.com/tatsuya-midorikawa/Tsuzuri/blob/c82c13e1e3dd1f02f78694aa1d26d39b3f793504/_docs/learn/why-tsuzuri.md#cc-に対する劣位点)） |
| 手本にする既存実装 | WGSL 生成: `src/gpu.rs` の `GpuKernel::wgsl`・`Wgsl::expression`・`wgsl_type`。std 呼び出しの検査: `src/gpu.rs` の `validate_calls`（`src/check.rs` から呼ばれる）。CPU 参照: `std/Gpu.tz` の `map`・`init`。出力種別: `src/driver.rs` の `Emit::Wgsl` と `build`、`src/main.rs` の `--emit` 解析。ホスト: `src/runtime/webgpu.mjs` の `createWebGpu`（adapter の明示要求、所有 buffer、error scope）。検証: `tests/gpu.mjs`（CPU 参照の bit 比較、WGSL の決定性、`TSUZURI_WEBGPU=1` の実 adapter） |
| 主な影響ファイル | `src/gpu.rs`, `src/driver.rs`, `src/main.rs`, `std/Gpu.tz`, `src/runtime/webgpu.mjs`, `tests/gpu.rs`, `tests/gpu.mjs`, `benchmarks/run-gpu.mjs`（変更なし。JSON の列が増えるだけ）, `docs/language.md`, `docs/architecture.md`, `docs/benchmarks.md`, `_docs/guides/gpu.md`, `README.md`, `_docs/feature-status.md`, `_features/README.md` |

## 目的

CUDA／Metal／Vulkan compute を C/C++ から使う場合に近い GPU 活用を、数値の意味を明示したうえで可能にする。
GPU の浮動小数点は言語の strict な float 契約（再結合・暗黙 FMA なし、subnormal・NaN・符号付きゼロの保持）を満たせない。
そこで緩い意味を持つ別名の API と出力種別を用意し、利用者が名前で選んだときだけ使う。黙って strict から切り替える経路は作らない。

Phase は次のとおり。実装者は Phase 1 だけを実装する。Phase 2・3 は人間が求め、該当する決定事項が承認された場合だけ着手する。

| Phase | 内容 | 状態 |
| --- | --- | --- |
| 1 | relaxed f32 カーネル: `Gpu.map_relaxed`・`Gpu.init_relaxed`（CPU 参照）、`--emit wgsl-relaxed`、`webgpu.mjs` の f32 buffer、GPU なしの検証と `TSUZURI_WEBGPU=1` の実 adapter 検証、転送を含む計測 | 実装対象（D1 の承認後） |
| 2 | 言語 runtime からの WebGPU 接続（`Gpu.request Gpu.WebGpu`）、native の動的読み込み、`shader-f16` による f16 | 設計方針（D9） |
| 3 | Vulkan・SPIR-V の strict f32（float controls）、`shaderInt64` の i64、`Gpu.Auto` | 設計方針（D10） |

## 着手条件と停止条件

### 着手条件

- F07 が `_features/README.md` の状態欄で done であること（HEAD で done）。確認: `grep -n "F07\|F09\|C11" _features/README.md`。
- C11 は開始条件にしない。`Matrix` を GPU へ渡す API は対象外（「対象外」）。
- D1 が承認済みであること。承認前は Phase 1 のどの手順にも着手しない。
- GUIDE §2.3 の基準コマンドが成功し、手順 1 のベースライン（既存 WGSL と `tests/gpu.mjs` の出力）を保存していること。
- 実 adapter の検証をする場合だけ、README の手順で binding を入れる:
  `npm install --prefix target/webgpu-runtime --no-save --package-lock=false webgpu@0.6.1`。repository の依存には加えない。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- 既存の strict 整数カーネルの WGSL が 1 byte でも変わる（手順 1 の保存物との `cmp` が失敗する）。
- `BuildOptions` に field を足したくなった（`tests/debug_info.rs`・`tests/host_abi.rs`・`tests/trap_locations.rs` が struct literal で構築しているため、全部が壊れる）。D3 のとおり `Emit` の variant で表す。
- CPU 参照（`Gpu.map_relaxed` を `Gpu.CpuReference` で実行した結果）を strict 以外の意味にしたくなった（FMA・fast-math・FTZ の導入）。
- 実 adapter の結果が D6 の許容誤差を超えた。許容誤差を広げず、kernel・入力・adapter 情報（`runtime.info`）を報告する。
- 実 adapter が relaxed WGSL を shader 作成時に拒否した（`prepare` が compilation error を投げる）。メッセージをそのまま報告する。
- `Gpu.request Gpu.WebGpu` を成功させる必要が出た、または既定の WASM に import が増えた（Phase 2。D9）。
- 新しい Rust crate、repository の npm 依存、`unsafe` が必要になった。
- 既存テストの期待値（コード・出力・WGSL）を変える必要がある。「既存テストへの影響」に挙げた E1018 のメッセージ文面だけは除く。

## 現状（HEAD `f8dc655` で確認）

- `std/Gpu.tz`: `union Backend = CpuReference | WebGpu | Vulkan | Cuda | Metal | Auto`、`union Error = Unavailable`、
  `record Device { backend: Backend }`、`record Buffer<'a> { values: ['a] }`。`request` は `CpuReference` だけ `Result.Ok`、ほかは
  `Result.Error Unavailable`。`init` は count を `assert` してから `Array.init`、`map` は `Array.map transform (ref values)`（D-34 で関数を先に受け取る順に変更）、
  `from_array`・`to_array`・`backend` がある。GPU へは何も送らない。
- `src/gpu.rs` の `scalar` は `Type::Bool`・`Type::Integer(32 | 64, _)`・`Type::Binary(32 | 64)` を受ける。
- `src/gpu.rs` の `validate_calls`（`src/check.rs` から呼ばれる）は std の `Gpu` の関数のうち名前が `init`・`map`・`from_array` のものを集め、
  関数値としての流出、部分適用、bool や非 scalar の要素、捕捉のある callback を E1018 で拒否し、callback に `extract_kernel` をかける。
- `extract_kernel` は一引数 scalar、再帰なし、深さ 128・関数 1,024・式 65,536（超えると E1017）を検査する。受ける式は
  `Int`・`Float`・`Bool`・`Unit`・`Local`・`Unary`・`Binary`・`Cast`・`If`・`Block`・局所変数への `Assign`・既知関数の完全適用の `Call`。
  それ以外は E1018（`GPU kernels do not support allocation, borrows, host calls, loops, tasks, or assertions`）。CPU 参照では f32/f64/i64 も通る。
- `GpuKernel::wgsl` は root の引数と結果が `Type::Integer(32, _)` でなければ E1018
  （`strict WebGPU kernels require i32 or i32u buffer lanes; 64-bit and strict floating-point lanes are unavailable`）。
  binding 0（入力 storage）、1（出力 storage）、2（`Params { length: u32 }` の uniform）、`@workgroup_size(256)`、`map_main`・`init_main` を出す。
- `Wgsl::expression` に `TypedExprKind::Float` の arm はない（`expression is not supported by strict WGSL generation`）。除算・剰余は拒否、
  cast は i32 と i32u の間の `bitcast` だけ、`UnaryOp::Negate` の非 i32 は `(0u - x)`。`wgsl_type` は i32・u32・bool だけを返す。
- `src/check.rs` の `TypedExprKind::Float(String)` は LLVM の定数文字列を持ち、`src/llvm.rs` はそれをそのまま出す。
- `src/driver.rs`: `Emit::Wgsl`。`BuildOptions::validate` は WGSL と target・CPU・debug・WASM 機能の組み合わせを E2000 で拒否する。
  `build` は export が一つでなければ E2004、あれば `crate::gpu::extract_kernel(module, exports[0])?.wgsl()?`。
  `src/main.rs` は `--emit wgsl` と `--target`・`-O`・`--cpu` の組み合わせを拒否する。
- `src/runtime/webgpu.mjs` の `createWebGpu(gpu)`: gpu がなければ `WebGPU is unavailable; no CPU fallback was selected`、adapter の
  `maxComputeInvocationsPerWorkgroup` が 256 未満なら失敗。`prepare(source)` は compilation error を投げ、`map`・`init` の pipeline を作る。
  `fromArray` は `Int32Array`・`Uint32Array` 以外を `TypeError`（`strict GPU buffers require Int32Array or Uint32Array`）。`toArray` は常に
  `Uint32Array`。buffer は `take` で一度だけ消費され、操作は `checked` の error scope を通る。
- `tests/gpu.rs` は 4 テスト（`extracts_scalar_kernels_with_normal_cpu_lowering`、`rejects_kernel_effects_allocations_captures_and_recursion`、
  `generates_deterministic_strict_integer_wgsl`、`gpu_reference_api_is_explicit_opaque_and_owned`）。
- `tests/gpu.mjs` は整数 3 kernel（`mix`・`cast`・`locals`）× 261 入力 = 783 参照、`float_reference`（`Gpu.map` の f32 CPU 参照を
  native/WASM × `-O0`/`-O3` で bit 比較、`live == 0`、import なし）、f32 kernel の `--emit wgsl` が E1018 で出力を保護することを検査する。
  `TSUZURI_WEBGPU=1` では `target/webgpu-runtime/node_modules/webgpu/index.js`（`TSUZURI_WEBGPU_MODULE` で上書き）の実 adapter を使い、
  `pipeline_creation_ms`・`gpu_transfer_dispatch_sync_readback_ms`・`gpu_resident_dispatch_sync_ms`・`device_startup_ms`・
  `cpu_wasm_scalar_calls_ms` を行に記録する。`--benchmark` は JSON を出し、`benchmarks/run-gpu.mjs` はそれを包む。
- `docs/language.md` の浮動小数点規則は fast-math・再結合・暗黙 FMA を禁じ、subnormal・符号付きゼロ・NaN を保つ。float から整数への cast は
  ゼロ方向の切り捨て・飽和・NaN から 0。GPU Kernel（実験的 Phase 1）節が F07 の現状を書く。GUIDE §9 D-29 が F07 の決定、D-18・D-30 が
  「GPU runtime は明示的な opt-in、既定の WASM に import を足さない」を定める。

### 再現（検証済み）

scratch は `/tmp/tz-work-F09/`。`wgsl/Main.tz` は `export def kernel :: f32 -> f32` と `fn kernel value = value * value + value`。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
target/release/tsuzuri build /tmp/tz-work-F09/wgsl --emit wgsl -o /tmp/tz-work-F09/wgsl/k.wgsl --json
# => E1018 "strict WebGPU kernels require i32 or i32u buffer lanes; ..."、終了コード 1
target/release/tsuzuri check /tmp/tz-work-F09/big --json
# fn kernel value = value * 1.0e39（f32）=> E1009 "floating-point literal is out of range"
```

f32 のリテラルは check の段階で有限に限られる（上の E1009）。したがって WGSL 生成で非有限リテラルを扱う arm は要らない。
「例」の `horner`・`index_kernel`（`(index as f32) * 0.5 + 0.25`）・`Gpu.map` を使う f32 の CPU 参照は HEAD の `check` で成功する。

## 仕様

### 前提とする他チケットのインターフェース

- F07（done）: 「現状」に挙げた `src/gpu.rs`・`std/Gpu.tz`・`src/runtime/webgpu.mjs`・`Emit::Wgsl` をそのまま使う。ほかのチケットの
  インターフェースには依存しない。C11 の `Matrix` と `_features/D10-f16-hardware.md` の f16 は Phase 2 以降でだけ関係する。

### API（Phase 1）

std への追加。新 API（実装後に有効。未検証）。名前は D1 で要承認（`（要割り当て）`）。

```tsuzuri
def init_relaxed :: ref Device -> i64 -> (i32 -> 'a) -> Buffer<'a>
fn init_relaxed device count initializer = init device count initializer

def map_relaxed :: Copy<'a> => ref Device -> ('a -> 'b) -> Buffer<'a> -> Buffer<'b>
fn map_relaxed device transform buffer = map device transform buffer
```

- `Gpu.init_relaxed`（新規・要割り当て）・`Gpu.map_relaxed`（新規・要割り当て）は `Gpu.init`・`Gpu.map` と同じ所有権・借用・count 検査を持つ。
  違いは二つだけ: callback に relaxed kernel の制約（「型規則」）をかけること、GPU backend（Phase 2）で緩い float 意味を許すこと。
- CLI: `tsuzuri build Kernel.tz --emit wgsl-relaxed -o kernel.wgsl`（出力種別 `wgsl-relaxed` は新規・要割り当て）。`--emit wgsl` と同じく、
  export が一つの専用 project を受け、`--target`・`-O`・`--cpu`・debug・WASM 機能と併用できない。
- ホスト（`src/runtime/webgpu.mjs`）: `prepare(source, options)` の `options.float === "relaxed"`（新規）で relaxed WGSL を受ける。
  `fromArray` は `Float32Array` も受ける。`toArray` は f32 出力の kernel の結果を `Float32Array` で返す。

### 型規則

relaxed kernel（`--emit wgsl-relaxed` の root、`Gpu.init_relaxed`・`Gpu.map_relaxed` の callback）には、`extract_kernel` の既存の制約に加えて次をかける。

| 対象 | 許すもの | 拒否（E1018） |
| --- | --- | --- |
| buffer lane（root の引数と結果） | f32, i32, i32u | f64, i64, i64u, bool |
| 局所値・呼び出す関数の引数と結果 | f32, i32, i32u, bool | f64, i64, i64u |
| f32 の演算 | 単項 `-`、`+`、`-`、`*`、`/`、`==`、`!=`、`<`、`<=`、`>`、`>=` | `%` |
| cast | i32・i32u から f32、同じ型への cast、i32 と i32u の間（strict と同じ bitcast） | f32 から整数、f32 と f64 の間 |
| リテラル | f32（有限。E1009 で保証済み）、i32、i32u、bool | f64、64-bit 整数 |
| 整数の演算 | strict と同じ（wrap、shift は下位 5 bit） | 除算・剰余（strict と同じ理由） |

- strict の `--emit wgsl` は変えない。f32 を局所値に持つ i32 kernel も、今と同じく `wgsl_type` の E1018 で拒否する。
- `Gpu.map_relaxed`・`Gpu.init_relaxed` の要素型は上の lane の型に限る。`Gpu.map` と `Gpu.init` は今のまま（CPU 参照で f64・i64 も通る）。

### 評価順序・所有権・借用

- `Gpu.map_relaxed` は buffer を消費し、device を共有借用する。`Gpu.init_relaxed` は count を `init` と同じく `assert` する。どちらも
  `init`・`map` へ委譲するので、CPU 参照の callback は添字順に一回ずつ呼ばれる。
- GPU（Phase 2 と `webgpu.mjs`）は lane を任意の順序・並行に評価してよい。kernel は副作用を持たないので観測できない。
- 直接の完全適用だけを受ける規則、関数値として流出させない規則は `init`・`map` と同じ（`validate_calls`）。

### 数値・トラップ・native と WASM の差

relaxed f32 の言語契約（D1 で要承認）。`docs/language.md` の strict な float 規則の例外として、この API と出力種別の中にだけ適用する。

1. CPU 参照（`Gpu.CpuReference` の device）は relaxed kernel を strict な規則で評価する。結果は `Gpu.map` と bit 単位で同じで、native/WASM ×
   `-O0`/`-O3` で一致する。strict な結果は relaxed で許される結果の一つである。
2. relaxed の実装（`--emit wgsl-relaxed` の shader を GPU で実行したもの）は次をしてよい: `a * b + c` を一回丸めの積和へ縮約する、`+`・`*` の
   結合と順序を変える、subnormal の入力・中間値・結果をどちらかの符号のゼロへ置き換える、`/` を WGSL の精度（2.5 ulp）で計算する、
   i32・i32u から f32 への変換を隣接する二つの f32 のどちらかにする。
3. 入力・中間値・結果のどれかが NaN・無限大、または overflow したとき、その lane の結果は未規定の f32（比較なら未規定の bool）になる。
   trap はせず、ほかの lane には影響しない。float に依存する分岐の選択と、その先の整数結果も変わりうる。
4. 整数の値と演算は strict と同じ意味を保つ。
5. 同じ device・driver でも実行ごとの一致は保証しない。数値の誤差の上限は言語として約束しない。検証用の許容誤差は D6 のテスト規約である。
6. relaxed kernel は trap しない（整数の除算・剰余は拒否済み、float の演算は trap しない）。
7. WGSL の生成は target に依存しない。WASM の既定出力に import は増えない（D-18）。

### 診断

| コード | 条件 | メッセージ | 位置 |
| --- | --- | --- | --- |
| E1018 | `--emit wgsl` の root lane が i32・i32u 以外 | `strict WebGPU kernels require i32 or i32u buffer lanes; 64-bit lanes are unavailable and f32 lanes need --emit wgsl-relaxed` | root 関数（今と同じ） |
| E1018 | relaxed で lane・局所値・引数・結果が f64・i64・i64u、または lane が bool | `relaxed WebGPU kernels support f32, i32, and i32u values; WGSL has no f64 or 64-bit integers` | root 関数、局所値、または関数 |
| E1018 | relaxed で f32 から整数、または f32 と f64 の間の cast | `relaxed WGSL cannot reproduce Tsuzuri float-to-integer or f64 casts; compute them on the CPU` | cast 式 |
| E1018 | relaxed で f32 の `%` | `relaxed WGSL does not support floating-point remainder; compute it on the CPU` | 二項式 |
| E1018 | 整数の除算・剰余（両 mode。既存） | `WGSL kernel division/remainder may trap on the CPU and are not supported` | 二項式 |
| E1018 | `Gpu.init_relaxed`・`Gpu.map_relaxed` の callback が上の relaxed 規則に反する | 上の各行と同じ | 各行と同じ |
| E1018 | GPU 操作の流出（既存の文面を一般化） | `GPU operations cannot escape as function values; use direct full application` | 関数参照 |
| E1018 | GPU 操作の部分適用（同上） | `GPU operations require direct full application` | 呼び出し式 |
| E1018 | callback が既知の関数か捕捉なし lambda でない（同上） | `GPU init/map operations require a known function or capture-free lambda` | callback |
| E2004 | `--emit wgsl-relaxed` の export が一つでない | 既存の `WGSL output requires exactly one exported scalar kernel; use a dedicated source project` | なし |
| E2000 | `--emit wgsl-relaxed` と target・CPU・debug・WASM 機能（`BuildOptions::validate`） | 既存の `WGSL output does not use target, CPU, debug, or WASM feature options` | なし |
| E2000 | `--emit wgsl-relaxed` と `--target`・`-O`・`--cpu`（`src/main.rs` の引数解析。既存経路で E2000 になる） | 既存の `WGSL output does not use target, optimization, or CPU options` | なし |
| E2000 | 不明な emit 種別（同上） | `emit kind must be exe, object, llvm, header, wasm, wgsl, or wgsl-relaxed` | なし |

新しいコードは作らない。ホストの例外（`webgpu.mjs`）は「生成 IR とランタイム」に書く。

### 資源上限

- 抽出の深さ 128・関数 1,024・式 65,536（E1017）、count の 0〜2147483647 は F07 のまま。
- ホストの buffer 長は `bytesFor` の既存の検査（4 bytes × 要素数が `maxStorageBufferBindingSize`・`maxBufferSize` 以下）、workgroup 数は
  `maxComputeWorkgroupsPerDimension` 以下。f32 も 4 bytes なので式は変えない。

### 例

受理される kernel（HEAD の `check` で検証済み。`--emit wgsl-relaxed` は新構文（実装後に有効。未検証））。

```tsuzuri
export def horner :: f32 -> f32
fn horner value = ((value * 0.5 + 0.25) * value + 0.125) * value + 1.0
```

言語内の利用。新 API（実装後に有効。未検証）。`Gpu.map` に置き換えたものは HEAD の `check` で成功する。

```tsuzuri
export def ratio_sum :: f32 -> f32
fn ratio_sum value = {
    let device = Result.get (Gpu.request Gpu.CpuReference);
    let values = [value, value + 1.0];
    let buffer = Gpu.from_array (&device) (&values);
    let mapped = Gpu.map_relaxed (&device) (\item -> (item + 1.0) / (item * item + 2.0)) buffer;
    let result = Gpu.to_array mapped;
    result[0] + result[1]
}
```

拒否される例（`--emit wgsl-relaxed`、または `Gpu.map_relaxed` の callback として）。

| kernel | 結果 |
| --- | --- |
| `export def kernel :: f64 -> f64` と `fn kernel value = value + 1.0` | E1018（f64） |
| `export def kernel :: f32 -> i32` と `fn kernel value = if value * value > 2.0 then 1 else (value as i32)`（HEAD の `check` は成功） | E1018（f32 から整数への cast） |
| `export def kernel :: f32 -> f32` と `fn kernel value = value % 2.0` | E1018（float の剰余） |
| `export def kernel :: i32 -> i32` と `fn kernel value = 100 / value` | E1018（整数の除算。既存） |
| `tsuzuri build <dir> --emit wgsl-relaxed -O3` | CLI エラー（既存の文面） |

`export def kernel :: f32 -> f32`・`fn kernel value = value * value + value` の relaxed WGSL は次になる（`<F>` は `kernel` の関数 id、
`<L>` は引数の局所 id。テストでは module から読む）。1 行目の header 以外の形は strict の生成規則と同じ。

```text
// tsuzuri-gpu float=relaxed input=f32 output=f32
struct Params { length: u32 }
@group(0) @binding(0) var<storage, read> input_values: array<f32>;
@group(0) @binding(1) var<storage, read_write> output_values: array<f32>;
@group(0) @binding(2) var<uniform> params: Params;
fn kernel_<F>(local_<L>: f32) -> f32 {
return ((local_<L> * local_<L>) + local_<L>);
}
@compute @workgroup_size(256)
fn map_main(@builtin(global_invocation_id) invocation: vec3<u32>) {
if (invocation.x < params.length) { output_values[invocation.x] = kernel_<F>(input_values[invocation.x]); }
}
@compute @workgroup_size(256)
fn init_main(@builtin(global_invocation_id) invocation: vec3<u32>) {
if (invocation.x < params.length) { output_values[invocation.x] = kernel_<F>(f32(invocation.x)); }
}
```

### Phase 2・3（設計方針）

- Phase 2（D9）: `Gpu.request Gpu.WebGpu` を言語 runtime で実装する。WASM は opt-in の `--wasm-feature webgpu`（要割り当て）でだけ host import を
  足す。native は WebGPU 実装（Dawn または wgpu-native）を実行時に動的に読み込み、ない場合は `Result.Error Gpu.Unavailable`。link 時の依存は
  作らない。f16 は adapter の `shader-f16` feature を確かめた場合だけ `enable f16;` を出し、ない場合の明示要求はエラーにする。
- Phase 3（D10）: Vulkan・SPIR-V で strict f32（`VK_KHR_shader_float_controls` の RTE・DenormPreserve・SignedZeroInfNanPreserve と
  `NoContraction`）と `shaderInt64` の i64 を出す。能力のない device の明示要求はエラーにし、CPU や relaxed へ黙って切り替えない。
  `Gpu.Auto` は確保・コンパイル・転送・同期を含めて測った閾値でだけ GPU を選び、選択を観測可能にする。

## 設計

### データ構造

```rust
// src/driver.rs
pub enum Emit { Executable, Object, Llvm, Header, Wasm, Wgsl, WgslRelaxed /* 新規 */ }

// src/gpu.rs
impl GpuKernel<'_> {
    pub fn wgsl(&self) -> Result<String, Diagnostic> { self.emit_wgsl(false) }        // 呼び出し元は不変
    pub fn wgsl_relaxed(&self) -> Result<String, Diagnostic> { self.emit_wgsl(true) } // 新規
    fn emit_wgsl(&self, relaxed: bool) -> Result<String, Diagnostic> { /* 今の wgsl の本体 */ } // 新規
}
struct Wgsl { text: String, next: usize, relaxed: bool /* 新規 field */ }
fn wgsl_type(ty: &Type, relaxed: bool, span: Span) -> Result<&'static str, Diagnostic> // 引数 relaxed を追加
fn f32_bits(text: &str) -> u32 // 新規。TypedExprKind::Float の文字列から f32 の bit 列
```

`BuildOptions` には field を足さない（停止条件）。`wgsl` の公開シグネチャも変えない（`tests/gpu.rs` と `src/driver.rs` が呼ぶ）。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| CLI | `src/main.rs` | usage 文字列の `--emit KIND` 行、`--emit` の解析 | `Some("wgsl-relaxed") => Emit::WgslRelaxed`。usage と不明種別のメッセージに `wgsl-relaxed` を足す |
| CLI | `src/main.rs` | `emit == Some(Emit::Wgsl) && (target.is_some() ...)` の検査 | `matches!(emit, Some(Emit::Wgsl \| Emit::WgslRelaxed))` |
| CLI | `src/main.rs` | `parse(&["build", "Kernel.tz", "--emit", "wgsl"])` のある単体テスト | `wgsl-relaxed` の受理と `-O3`・`--target wasm32` の拒否を足す |
| driver | `src/driver.rs` | `Emit` | `WgslRelaxed` |
| driver | `src/driver.rs` | `BuildOptions::validate` | `self.emit == Emit::Wgsl` を二つの variant の `matches!` にする |
| driver | `src/driver.rs` | 出力の拡張子を決める `input.with_extension(match self.emit` | `Emit::WgslRelaxed => "wgsl"` |
| driver | `src/driver.rs` | `build` | `options.emit == Emit::Wgsl` の分岐を両 variant にし、`WgslRelaxed` なら `wgsl_relaxed()`。`!matches!(options.emit, Emit::Header \| Emit::Wgsl)` と `matches!(options.emit, Emit::Llvm \| Emit::Header \| Emit::Wgsl)` に `Emit::WgslRelaxed` を足す |
| 検査 | `src/gpu.rs` | `validate_calls` | 名前の集合に `init_relaxed`・`map_relaxed`。callback の位置は `init`・`init_relaxed` が 2、ほかは 1。relaxed の名前なら `extract_kernel(module, target)?.wgsl_relaxed()?` の結果を捨てて規則だけを検査する。E1018 の 3 文面を「診断」のとおり一般化する |
| 生成 | `src/gpu.rs` | `emit_wgsl` | strict は今の lane 検査。relaxed は lane を `wgsl_type(.., true, ..)` で検査し、bool の lane を拒否し、先頭へ header 行を出す |
| 生成 | `src/gpu.rs` | `wgsl_type` | `Type::Binary(32) if relaxed => Ok("f32")`。relaxed の拒否は relaxed の文面、strict は今の文面 |
| 生成 | `src/gpu.rs` | `Wgsl::expression` の `TypedExprKind::Float` | 新しい arm: `format!("bitcast<f32>({}u)", f32_bits(text))`。strict ではここへ来ない（`wgsl_type` が先に拒否する） |
| 生成 | `src/gpu.rs` | `Wgsl::expression` の `UnaryOp::Negate` | `expression.ty == Type::Binary(32)` なら `(-x)`。`(0u - x)` の fallback より前に置く |
| 生成 | `src/gpu.rs` | `Wgsl::expression` の `TypedExprKind::Binary` | `left.ty == Type::Binary(32)` のとき `BinaryOp::Divide` を `/` で出し、`BinaryOp::Remainder` を relaxed の文面で拒否する。整数の除算・剰余は今の文面 |
| 生成 | `src/gpu.rs` | `Wgsl::expression` の `TypedExprKind::Cast` | `(Type::Integer(32, _), Type::Binary(32))` を `f32(x)`。f32 からの cast と f64 との cast を relaxed の文面で拒否する |
| std | `std/Gpu.tz` | `init_relaxed`, `map_relaxed` | 「API」のとおり `init`・`map` へ委譲 |
| ホスト | `src/runtime/webgpu.mjs` | `prepare`, `fromArray`, `own`, `dispatch`, `toArray` | header の解析、`float: "relaxed"` の確認、buffer の要素種別 `kind`、f32 の読み戻し |
| テスト | `tests/gpu.rs`, `tests/gpu.mjs` | 「テスト計画」 | 追加 |

`Emit` は `src/driver.rs` と `src/main.rs` の外では使われない（確認: `grep -rn "Emit::Wgsl" src tests`）。網羅的な `match` は compile error で
見つかるが、`==` と `matches!` は見つからないので上の表の行をすべて直す。

### 生成 IR とランタイム

- LLVM IR: `Gpu.init_relaxed`・`Gpu.map_relaxed` は std の通常の関数として特殊化され、`init`・`map` を呼ぶ。新しい runtime symbol、`%tz.*` 型、
  WASM import はない。relaxed を使わないプログラムの IR は byte 単位で変わらない。
- WGSL: relaxed では 1 行目に `// tsuzuri-gpu float=relaxed input=<t> output=<t>`（`<t>` は `f32`・`i32`・`u32`）を出す。strict は header を
  出さず、今の出力と byte 単位で同じ。f32 の定数は `bitcast<f32>(<bits>u)` で出し、丸めを WGSL の compiler に任せない。
- `webgpu.mjs` の変更（すべて JavaScript。新しい依存なし）:
  - `prepare(source, options = {})`: 1 行目が `// tsuzuri-gpu float=relaxed` で始まり `options.float !== "relaxed"` なら
    `Error("relaxed floating-point WGSL requires prepare(source, { float: \"relaxed\" })")`。header から `input`・`output` を読み、program に
    `input`・`output` を持たせる。header がなければ両方 `"u32"`（今の整数の扱い）。f32 は `"f32"`、i32・u32 は `"u32"`。
  - `fromArray(values)`: `Float32Array` は kind `"f32"`、`Int32Array`・`Uint32Array` は kind `"u32"`。それ以外は
    `TypeError("GPU buffers require Int32Array, Uint32Array, or Float32Array")`。
  - `map(program, buffer)`: `buffer.kind !== program.input` なら `TypeError("GPU buffer element type does not match the kernel input")`。
    消費（`take`）より前に検査する。出力 handle の kind は `program.output`。`init` の出力も同じ。
  - `toArray(handle)`: kind が `"f32"` なら `Float32Array`、それ以外は今と同じ `Uint32Array`。長さ 0 も同じ型で返す。

### アルゴリズム

`f32_bits` は `crate::numeric::float_literal` が f32 のリテラルを `0x{:016X}`（f32 の値を f64 へ広げた bit 列）で書くことを利用する。

```rust
fn f32_bits(text: &str) -> u32 {
    let bits = u64::from_str_radix(&text[2..], 16).expect("float literal is LLVM hex");
    (f64::from_bits(bits) as f32).to_bits() // f32 で表せる値なので正確
}
```

relaxed の lane 検査（`emit_wgsl`）:

```text
if relaxed:
    input  = wgsl_type(root.parameters[0], true, root.span)?   // f64・i64 はここで E1018
    output = wgsl_type(root.result, true, root.span)?
    if input == "bool" or output == "bool": E1018（relaxed の型の文面）
    text = "// tsuzuri-gpu float=relaxed input={input} output={output}\n"
else:
    今の Type::Integer(32, _) の検査と文面
text += 今の binding・関数・entry point（wgsl_type と Wgsl に relaxed を渡す）
```

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N が期待どおりかを必ず見る（GUIDE §3）。Node の E2E の前には毎回 `cargo build --release --locked` を実行する。

### 手順 1: ベースラインを取る

- 変更: なし。
- 内容: GUIDE §2.3 を実行する。`tests/gpu.rs` の `generates_deterministic_strict_integer_wgsl` と同じ source（`square` と `kernel`）を
  `/tmp/tz-f09/strict/Main.tz` に置き、strict WGSL を保存する。
- 確認: 次がすべて成功する。`cargo test --locked --test gpu` は `4 passed`。`tests/gpu.mjs` は
  `GPU phase 1: 783 integer references, ...` を出す。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked && cargo test --locked
cargo test --locked --test gpu
target/release/tsuzuri build /tmp/tz-f09/strict --emit wgsl -o /tmp/tz-f09/before.wgsl
node tests/gpu.mjs target/release/tsuzuri
```

### 手順 2: relaxed の WGSL 生成

- 変更: `src/gpu.rs` の `GpuKernel::wgsl`（本体を `emit_wgsl` へ移す）、`wgsl_relaxed`（新規）、`wgsl_type`、`Wgsl`、`Wgsl::expression`、
  `f32_bits`（新規）。`tests/gpu.rs`。
- 内容: 「段ごとの変更」の生成の行をすべて入れる。strict の文面を「診断」の新しい文面へ変える。テスト
  `generates_relaxed_f32_wgsl_with_explicit_header`（新規）と `rejects_relaxed_wgsl_outside_f32_i32_lanes`（新規）を足す。
- 確認: `cargo test --locked --test gpu` が `6 passed`。strict の出力が変わらないことは手順 4 で CLI から確かめる。

### 手順 3: std の relaxed API と呼び出し検査

- 変更: `std/Gpu.tz`（`init_relaxed`・`map_relaxed`）、`src/gpu.rs` の `validate_calls`、`tests/gpu.rs`。
- 内容: 「API」の 2 定義を `map` の後に足す。`validate_calls` に relaxed の名前と `wgsl_relaxed` による検査を入れ、3 つの文面を一般化する。
  テスト `relaxed_gpu_api_validates_kernels_on_the_cpu_reference`（新規）を足す。
- 確認: `cargo test --locked --test gpu` が `7 passed`。`cargo test --locked` が成功する（std の追加で特殊化上限のテスト
  `honors_the_exact_specialization_limit` が変わらないこと。relaxed API は利用者が呼ぶまで特殊化されない）。

### 手順 4: 出力種別 `wgsl-relaxed`

- 変更: `src/driver.rs` の `Emit`、`BuildOptions::validate`、拡張子の match、`build`。`src/main.rs` の usage・`--emit` 解析・WGSL の検査・単体テスト。
- 内容: 「段ごとの変更」の CLI と driver の行。`grep -n "Emit::Wgsl" src/driver.rs src/main.rs` の全行を見直し、`Emit::WgslRelaxed` の要否を決める。
- 確認: `cargo test --locked --bin tsuzuri` が成功する。次で strict は不変、relaxed は header 付き、`-O3` は E2000。

```sh
cargo build --release --locked
target/release/tsuzuri build /tmp/tz-f09/strict --emit wgsl -o /tmp/tz-f09/after.wgsl && cmp /tmp/tz-f09/before.wgsl /tmp/tz-f09/after.wgsl
target/release/tsuzuri build /tmp/tz-work-F09/wgsl --emit wgsl-relaxed -o /tmp/tz-f09/poly.wgsl && head -1 /tmp/tz-f09/poly.wgsl
# => // tsuzuri-gpu float=relaxed input=f32 output=f32
target/release/tsuzuri build /tmp/tz-work-F09/wgsl --emit wgsl-relaxed -O3 --json; echo $?
# => E2000、終了コード 1
```

### 手順 5: ホストの f32 buffer

- 変更: `src/runtime/webgpu.mjs`。
- 内容: 「生成 IR とランタイム」の `prepare`・`fromArray`・`map`・`init`・`toArray` の変更。整数の経路（header なし）の挙動は変えない。
- 確認: `node --check src/runtime/webgpu.mjs` が成功する。`node tests/gpu.mjs target/release/tsuzuri` が手順 1 と同じ行を出す。

### 手順 6: GPU なしの E2E

- 変更: `tests/gpu.mjs`。
- 内容: 「E2E」の (a)〜(c) を足す。`TSUZURI_WEBGPU` がなくても必ず実行する部分である。
- 確認: `node tests/gpu.mjs target/release/tsuzuri` が成功し、要約行に relaxed の参照数（5 kernel × 263 入力 = 1,315）が出る。
  `node tests/gpu.mjs target/release/tsuzuri --quick` も成功する。

### 手順 7: 実 adapter の E2E

- 変更: `tests/gpu.mjs`（`if (runtime)` の中）。
- 内容: 「E2E」の (d)。既存の `assert.throws(() => runtime.fromArray(new Float32Array(1)), TypeError)` を `Float64Array` に変える（「既存テストへの影響」）。
- 確認: binding を入れた機械で `TSUZURI_WEBGPU=1 node tests/gpu.mjs target/release/tsuzuri` が成功し、要約行が
  `WebGPU executed and matched` を含む。GPU のない機械では実行しない（要約行が `not requested` を出す）。実行できなかったことは完了報告に書く。

### 手順 8: 計測

- 変更: なし（`benchmarks/run-gpu.mjs` は `--benchmark` を渡すだけなので変えない）。
- 内容: `TSUZURI_WEBGPU=1 node benchmarks/run-gpu.mjs target/release/tsuzuri > /tmp/tz-f09/gpu.json` を 9 回実行し、行ごとの各列の中央値と
  最小・最大を求める。adapter 情報（`adapter` 列）、OS、Node、binding の版と commit を併記する。
- 確認: JSON の relaxed の行に「性能」の列がすべてある。速度の合否条件は作らない。

### 手順 9: 全体の確認

- 変更: なし。
- 内容: GUIDE §3 の形式・lint・全 Rust テスト・E2E を実行する。GPU の数値は CPU 参照に影響しないので、`tests/numeric_casts.mjs` も通す。
- 確認: 次がすべて成功する。

```sh
cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && cargo test --locked
cargo build --release --locked
node tests/gpu.mjs target/release/tsuzuri
node tests/e2e.mjs target/release/tsuzuri && node tests/numeric_casts.mjs target/release/tsuzuri && node tests/examples.mjs target/release/tsuzuri
```

### 手順 10: 文書

- 変更: 「ドキュメント」の全ファイル。
- 内容: relaxed の契約（「数値・トラップ・native と WASM の差」の 1〜7）、API、CLI、ホスト、計測結果（実行した場合だけ）を書く。
- 確認: `node scripts/check-docs.mjs _docs/guides/gpu.md _docs/feature-status.md _docs/library-reference/README.md` が成功し、`git diff --check` が空。

## テスト計画

### Rust テスト

`tests/gpu.rs` に足す。source の組み立ては既存テストに合わせ、関数 id は `kernel_id`、局所 id は
`module.functions[id].parameters[0].id` から読む。

- `generates_relaxed_f32_wgsl_with_explicit_header`（新規）:
  - `fn kernel value = value * value + value`（f32）の `wgsl_relaxed()` が「例」の WGSL と完全一致する（id を埋めた `format!` で比較）。2 回の出力が同じ。
  - 同じ kernel の `wgsl()` が E1018 で、メッセージが `--emit wgsl-relaxed` を含む。
  - `fn kernel value = ((value * 0.5 + 0.25) * value + 0.125) * value + 1.0` の出力が `bitcast<f32>(1056964608u)`（0.5）、
    `bitcast<f32>(1048576000u)`（0.25）、`bitcast<f32>(1040187392u)`（0.125）、`bitcast<f32>(1065353216u)`（1.0）を含む。
  - `fn kernel value = -value / 0.1` が `(-local_` と ` / ` と `bitcast<f32>(1036831949u)` を含む（0.1 の最近接 f32 は `0x3DCCCCCD`。
    Python の `struct.unpack("<I", struct.pack("<f", 0.1))[0]` で確かめた値）。
  - `export def kernel :: i32 -> f32` と `fn kernel index = (index as f32) * 0.5 + 0.25` の出力が header `input=i32 output=f32` と `f32(local_` を含む。
- `rejects_relaxed_wgsl_outside_f32_i32_lanes`（新規）: 次の各 source の `wgsl_relaxed()` が E1018。
  f64 → f64、i64 → i64、f32 → bool（lane が bool）、`if value * value > 2.0 then 1 else (value as i32)`（f32 → i32）、`value % 2.0`、
  `100 / value`（i32）、`{ let wide = (value as f64) + 1.0; wide as f32 }`（f32）。加えて `export def kernel :: i32 -> i32` と
  `fn kernel value = if (value as f32) < 1.5 then 1 else 0` の strict `wgsl()` が E1018（strict は f32 を受けない）。
- `relaxed_gpu_api_validates_kernels_on_the_cpu_reference`（新規）: `gpu_reference_api_is_explicit_opaque_and_owned` と同じ prefix で、
  `Gpu.map_relaxed` の f32 lambda（`\item -> item * item + item`）と `Gpu.init_relaxed` の `\index -> (index as f32) * 0.5 + 0.25` を
  `analyze` が受ける。次は E1018: f64 配列への `Gpu.map_relaxed`、`\item -> item as i32` の callback、`let f = Gpu.map_relaxed`（流出）、
  `Gpu.map_relaxed (&device)` だけの部分適用。同じ callback の `Gpu.map` は受理される（strict API は変えない）。
- `src/main.rs` の既存単体テスト（`parse(&["build", "Kernel.tz", "--emit", "wgsl"])` を含むもの）: `--emit wgsl-relaxed` が
  `Emit::WgslRelaxed` になり、`-O3` と `--target wasm32` との併用が `is_err()`。

### E2E

`tests/gpu.mjs` に足す。期待値は JavaScript の `Math.fround` で各演算を一回ずつ丸めて計算し、コンパイラの出力から作らない。

| kernel | 型 | 本体 | float 演算 | 許容誤差（D6） |
| --- | --- | --- | --- | --- |
| `poly` | f32 → f32 | `value * value + value` | `*` `+` | 4 ulp |
| `horner` | f32 → f32 | `((value * 0.5 + 0.25) * value + 0.125) * value + 1.0` | `*` 3、`+` 3 | 12 ulp |
| `ratio` | f32 → f32 | `(value + 1.0) / (value * value + 2.0)` | `+` 2、`*`、`/` | 9 ulp |
| `index` | i32 → f32 | `(value as f32) * 0.5 + 0.25` | 変換、`*`、`+` | 5 ulp |
| `threshold` | f32 → i32 | `if value * value > 2.0 then 1 else 0` | `*` | 完全一致 |

入力は 263 個: `2 ** -8`、`0.5`、`1`、`1.5`、`2`、`3`、`255`、`256` と、既存の LCG（`Math.imul(seed, 1664525) + 1013904223`）の 248 値を
`Math.fround(2 ** ((seed >>> 8) / 2 ** 24 * 16 - 8))` で `[2^-8, 2^8)` へ写したもの、特殊値 7 個（`0`、`-0`、`2 ** -149`、`NaN`、`Infinity`、
`-Infinity`、`3.4e38`）。`threshold` の LCG 値のうち `[1.4142, 1.4143]` に入るものは除く。i32 入力の `index` は整数 0〜262 を使う。

- (a) WGSL: 各 kernel の専用 project で `--emit wgsl-relaxed` を 2 回実行し byte 一致、1 行目が header。`--emit wgsl` は E1018
  （`index`・`threshold` を含む全 kernel）。f64 kernel の `--emit wgsl-relaxed --json` は `code` が E1018 で、既存の出力ファイルが
  `preserved` のまま。`--emit wgsl-relaxed -O3` は失敗する。
- (b) CPU 参照の scalar: 各 kernel を `--target wasm32 -O0`・`-O3` で build し（import なし）、`tz_kernel` の結果が strict な JavaScript の参照と
  全 263 入力で一致する（NaN は `Number.isNaN` で比較、それ以外は `Object.is`）。
- (c) 言語内の relaxed API: 既存の `api` project に `relaxed_reference`（`Gpu.map_relaxed` と `poly` の lambda）と `relaxed_init_sum`
  （`Gpu.init_relaxed (&device) 8 (\index -> (index as f32) * 0.5 + 0.25)` の `Array.sum`。期待値 16.0。0.25 + 0.75 + ... + 3.75 は f32 で正確）
  を足す。`host.c` と WASM の既存ループに入れ、native/WASM × `-O0`/`-O3` で `relaxed_reference` が `float_reference` と同じ 10 bit パターンで
  bit 一致、`live == 0`、WASM の import が空、memory が 16 MiB 以下。
- (d) `TSUZURI_WEBGPU=1` のときだけ: `prepare(wgsl)` が `/requires prepare/` で reject し、`prepare(wgsl, { float: "relaxed" })` が成功する。
  f32 入力の kernel は `fromArray(new Float32Array(inputs))`、`index` は `init(program, count)`（count は 0、1、255、256、257、263）で実行する。
  `toArray` の結果は `Float32Array`（`threshold` は `Uint32Array`）。有限な 256 入力で `Number.isFinite(actual)` かつ
  `Math.abs(actual - expected) <= tolerance * ulp(expected)`。`threshold` は完全一致。特殊値 7 個は長さだけを確かめる（値は未規定）。
  `map(program, await runtime.fromArray(new Int32Array(1)))` が `TypeError` で、その buffer は消費されない（続けて `toArray` できる）。
  `ratio` を常駐のまま 3 回 `map` し、3 回適用した参照と 54 ulp（D6）以内。

`ulp` は次の独立な計算を使う。

```javascript
const f32 = new Float32Array(1), u32 = new Uint32Array(f32.buffer);
const ulp = x => { f32[0] = Math.abs(x); const low = f32[0]; u32[0] += 1; return f32[0] - low; };
```

### 既存テストへの影響

- `tests/gpu.mjs` の `runtime.fromArray(new Float32Array(1))` の `TypeError` の期待は、f32 buffer を受けるので `Float64Array` に変える
  （`TSUZURI_WEBGPU=1` のときだけ実行される行）。
- E1018 の 4 つの文面（strict lane、流出、部分適用、callback）が変わる。テストは `code` だけを見ており、文面を検査するテストはない
  （確認: `grep -rn "cannot escape\|strict WebGPU kernels require" tests`）。
- それ以外はなし。strict WGSL は byte 単位で不変（手順 4 の `cmp`）。

### 性能

`--benchmark` の JSON の relaxed の行は次の列を持つ（ms は `performance.now()` の差）。合否条件にはしない。

| 列 | 含むもの |
| --- | --- |
| `device_startup_ms` | adapter と device の取得（既存） |
| `pipeline_creation_ms` | shader と 2 pipeline の作成（既存） |
| `gpu_transfer_dispatch_sync_readback_ms` | upload・dispatch・完了待ち・readback の合計。転送を含む主要な値（既存） |
| `gpu_resident_dispatch_sync_ms` | 常駐 3 回の dispatch と各回の完了待ち。転送を含まない（`ratio` だけ） |
| `cpu_wasm_scalar_calls_ms` | WASM の scalar export の呼び出し（既存。bulk の比較ではない） |
| `float`, `tolerance_ulps`, `max_ulp_error` | `"relaxed"`、D6 の許容誤差、観測した最大誤差（ulp） |

## ドキュメント

- `docs/language.md` の `### GPU Kernel（実験的 Phase 1）`: relaxed の契約 1〜7、`Gpu.init_relaxed`・`Gpu.map_relaxed`、`--emit wgsl-relaxed`、
  lane と演算の表、ホストの `float: "relaxed"` と `Float32Array`。浮動小数点の節の「fast-math、許可されていない再結合、暗黙の FMA は使いません」の
  行に「例外は GPU Kernel 節の relaxed API だけ」と書く。
- `docs/architecture.md` の「性能設計の原則」の GPU 行: 「float GPU は未実装」を「relaxed f32 の WGSL とホスト試作。strict float GPU・
  言語 runtime への接続・自動 offload は未実装」にする。
- `docs/benchmarks.md` の GPU の段落: relaxed の行と列、許容誤差の意味、計測した場合だけ結果と環境。速度優位は主張しない。
- `_docs/guides/gpu.md`: 「対応範囲を選ぶ」の表に relaxed f32 の行、新しい節「relaxed f32 カーネル」（契約、CLI、ホスト、f64 と i64 が
  使えない理由）。
- `_docs/library-reference/README.md` の Gpu 行、`README.md` の GPU の説明（冒頭の概要、GPU の節、テストコマンドの注記）。
- `_docs/feature-status.md` と `_features/README.md` の F09 の状態（Phase 1 完了。Phase 2・3 は未着手と明記）。
- 承認された名前（D1）を GUIDE §9 に記録する（D-30 の手順: std 名は D-07、確定後に D-30 の表から除く）。

## 受け入れ条件

- [ ] D1 が承認されている。
- [ ] `--emit wgsl-relaxed` が f32・i32・i32u の kernel から header 付きの決定的な WGSL を出し、f64・i64・f32 から整数への cast・float の剰余を
  E1018 で拒否する。
- [ ] strict の `--emit wgsl` の出力が byte 単位で変わらず、f32 の kernel は今までどおり E1018。
- [ ] `Gpu.init_relaxed`・`Gpu.map_relaxed` が CPU 参照で `Gpu.init`・`Gpu.map` と bit 一致し（native/WASM × `-O0`/`-O3`）、`live == 0`、
  WASM の import が空。
- [ ] `webgpu.mjs` は relaxed WGSL を `float: "relaxed"` なしで拒否し、要素型の不一致を buffer を消費せずに拒否する。
- [ ] `tests/gpu.rs` の 7 テストと `node tests/gpu.mjs target/release/tsuzuri` が成功する。`TSUZURI_WEBGPU=1` の実行結果（成功、または
  実行できなかったこと）を完了報告に書く。
- [ ] 性能の数値は計測したものだけを、転送を含む値と常駐の値を分けて記録する。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- `Emit` への variant 追加は網羅的な `match` では compile error になるが、`options.emit == Emit::Wgsl` と `matches!(.., Emit::Wgsl)` は黙って
  `WgslRelaxed` を外す。「段ごとの変更」の driver の行をすべて直し、`grep -n "Emit::Wgsl" src` で漏れを確かめる。
- `wgsl_type` が strict でも f32 を返すと、f32 の局所値を持つ i32 kernel が strict のまま緩い float の shader になる。`relaxed` 引数で分け、
  `rejects_relaxed_wgsl_outside_f32_i32_lanes` の strict の行で守る。
- `UnaryOp::Negate` の fallback `(0u - x)` は f32 では WGSL の型エラーになり、実 adapter でしか見つからない。f32 の arm を先に置く。
  `0.0 - x` も使わない（`-0.0` の符号が変わる）。
- `TypedExprKind::Float` の文字列は 10 進ではなく LLVM の 16 進（f64 の bit 列）。`text.parse::<f32>()` は失敗し、10 進の元の source から
  読み直すと二重丸めになりうる。`f32_bits` だけを使う。
- `init_main` は f32 入力の kernel に `f32(invocation.x)` を渡す。2^24 を超える index は丸まる。言語内の `Gpu.init_relaxed` の callback は i32 を
  受けるので、この形は `index` のような i32 入力の kernel で使う。
- `map` の要素型の検査を `take` の後に置くと、不一致の buffer が消費され破棄される。検査を先にする。
- `toArray` の長さ 0 の早期 return も kind に合わせた型（`Float32Array`）を返す。
- Metal など多くの adapter は subnormal を flush する。特殊値の結果は検査しない。許容誤差は相殺のない正の入力にだけ適用し、
  超えたら入力や許容誤差を変えず停止条件に従う。
- 転送を除いた時間だけで速度を語らない。`device_startup_ms`・`pipeline_creation_ms` は別の列に残し、1 回目の dispatch の初期化も含めて記録する。
- `tests/gpu.mjs` は `quick` で CPU の繰り返しを減らす。新しい部分も `quick` で短くしてよいが、(a)〜(c) の検査自体は省かない。
- 各 E2E project は既存どおり `mkdtempSync` の root の下に一つずつ作る（E03 の再帰探索で混ざらない）。

## 対象外

- 言語内の `Gpu.request Gpu.WebGpu` などの成功、native の動的読み込み、WASM の GPU import（Phase 2。D9）。
- strict f32 の GPU、i64 の GPU、SPIR-V・Vulkan、`Gpu.Auto`（Phase 3。D10）。f16（Phase 2。`_features/D10-f16-hardware.md` の f16 型が前提）。
- kernel 内の `Math` 関数（`Math.sqrt`・`Math.fma` など）、float の剰余、float から整数への cast、GPU 上の集約（sum など）、複数 buffer の kernel。
- CUDA の PTX 生成、GPU 上のヒープ確保、GPU 上の再帰・トラップの完全な再現、C11 の `Matrix` との接続、browser 本番の glue。
- 速度優位の主張と CI の速度閾値。

## 決定事項

### D1: relaxed f32 の言語契約と名前

- 決定: 緩い float は `Gpu.init_relaxed`・`Gpu.map_relaxed`（要割り当て）と `--emit wgsl-relaxed`（要割り当て）の中だけで許す。契約は
  「数値・トラップ・native と WASM の差」の 1〜7（縮約・再結合・subnormal の flush・WGSL の除算精度・非有限時の未規定値を許し、trap しない）。
  strict の API と `--emit wgsl` は変えず、暗黙に relaxed を選ぶ経路は作らない。
- 理由: WGSL は fusion・再結合・subnormal の差を許し、NaN・無限大を未規定にするので、strict な契約（`docs/language.md`）を満たせない。
  名前で区別すれば strict の意味を一切変えずに GPU の float を使える（`_features/README.md` の「緩い演算は名前で区別した別 API」）。
  `Gpu.Relaxed` のような device 側の mode は、同じ `Gpu.map` の意味が実行時の値で変わるので採らない。
- 状態: 要承認（承認前は Phase 1 のどの手順にも着手しない）

### D2: CPU 参照は strict で評価する

- 決定: `Gpu.init_relaxed`・`Gpu.map_relaxed` は `init`・`map` へ委譲し、CPU 参照では strict と bit 単位で同じ結果を返す。
- 理由: strict な結果は relaxed で許される結果の一つで、native/WASM・`-O0`/`-O3` の決定性と `live == 0` の既存検証をそのまま使える。
  CPU で FMA などを使うと AGENTS.md の「黙って fast-math を有効にしない」に反する。
- 状態: 既定案（実装者はこの案に従う）

### D3: relaxed の選び方は出力種別で表す

- 決定: `Emit::WgslRelaxed`（`--emit wgsl-relaxed`）を足し、`BuildOptions` に field を足さない。公開の `GpuKernel::wgsl` は strict のまま、
  `wgsl_relaxed` を足す。
- 理由: `BuildOptions` は複数のテストが struct literal で作るので field の追加は無関係なテストを壊す。`wgsl` のシグネチャを保てば
  既存の呼び出しとテストが不変。`--gpu-float relaxed` のような別 flag より組み合わせの検査が少ない。
- 状態: 既定案（実装者はこの案に従う。CLI 名は D1 の承認に含める）

### D4: relaxed kernel の制約

- 決定: 「型規則」の表のとおり。lane は f32・i32・i32u、f64・i64 は WGSL にないので拒否、f32 から整数への cast と float の剰余は拒否、
  f32 の定数は `bitcast<f32>(<bits>u)`、否定は `(-x)`。E1018 だけを使い、新しいコードを作らない。
- 理由: Tsuzuri の float→整数 cast（飽和、NaN から 0）と float の剰余は WGSL で同じ意味を保証できない。bit 列の定数は WGSL の
  リテラル解析の丸めに依存しない。F07 の kernel の診断はすべて E1018。
- 状態: 既定案（実装者はこの案に従う）

### D5: WGSL の header とホストでの明示

- 決定: relaxed WGSL の 1 行目に `// tsuzuri-gpu float=relaxed input=<t> output=<t>` を出し、`webgpu.mjs` の `prepare` は
  `{ float: "relaxed" }` がなければ拒否する。buffer は要素種別 `kind` を持ち、kernel の入力と一致しなければ消費せずに拒否する。strict は header なし。
- 理由: ホストの境界でも暗黙の選択をなくし、f32 と整数の buffer の取り違えを bit の再解釈にしない。strict の出力を byte 単位で保つ。
- 状態: 既定案（実装者はこの案に従う）

### D6: 検証の許容誤差

- 決定: relaxed の GPU 結果だけを許容誤差で比べる。kernel ごとの許容誤差は float 演算ごとの重みの和で、`+`・`-`・`*` は 2 ulp、`/` は 3 ulp
  （WGSL の 2.5 ulp を切り上げ）、整数から f32 への変換は 1 ulp。ulp は期待値の位置の f32 の間隔。常駐 3 回の連鎖は
  `3 × 許容誤差 × 2`（`ratio` で 54 ulp）。入力は相殺のない正の正規数 `[2^-8, 2^8]` に限り、特殊値は長さだけを見る。
  float に依存しない整数の出力と strict の整数 kernel は完全一致のまま。
- 理由: 縮約と再結合による差は相殺がなければ演算ごとに数 ulp に収まる。相殺があると ulp の上限は定義できないので、言語の保証ではなく
  テストの規約として入力を限る。
- 状態: 既定案（実装者はこの案に従う）

### D7: GPU のない環境のテスト

- 決定: WGSL の完全一致（Rust の snapshot）、CLI の決定性と診断、CPU 参照の bit 一致（native/WASM × `-O0`/`-O3`）を常に実行する。
  実 adapter の検証は既存どおり `TSUZURI_WEBGPU=1` のときだけで、実行しなかったことを要約行に出す。
- 理由: CI に GPU はない。実行しない部分を skip と混同しないよう、F07 の要約行の形式（`not requested`）を保つ。
- 状態: 既定案（実装者はこの案に従う）

### D8: 計測

- 決定: 転送・dispatch・完了待ち・readback を含む時間を主要な値とし、device の起動、pipeline 作成、常駐の dispatch を別の列にする。
  9 回の中央値・最小・最大を adapter 情報と一緒に記録し、速度の主張と閾値は作らない。
- 理由: AGENTS.md は転送・同期・起動を含む end-to-end の費用で判断することを求める。
- 状態: 既定案（実装者はこの案に従う）

### D9: Phase 2 の言語 runtime への接続

- 決定: `Gpu.request Gpu.WebGpu` を実装する。WASM は `--wasm-feature webgpu`（要割り当て）を指定したときだけ host import を足し、
  native は WebGPU 実装を実行時に動的に読み込む（link 時の依存なし）。読み込めない、または必要な feature（`shader-f16` など）がない場合の
  明示要求は `Result.Error Gpu.Unavailable` で、CPU へ置き換えない。f16 は `_features/D10-f16-hardware.md` の f16 型と `shader-f16` の確認を前提にする。
  strict の `Gpu.map` に float の要素を渡した場合の WebGpu device での扱い（拒否の方法）は Phase 2 の着手前レビューで決める。
- 理由: D-18・D-30 は GPU runtime を明示的な opt-in とし、既定の WASM に import を足さないと定める。動的読み込みと非同期の読み戻しは
  runtime の新しい基盤で、費用が大きい。
- 状態: 要承認（承認前は Phase 2 に着手しない）

### D10: Phase 3 の strict float・i64・自動選択

- 決定: strict f32 は Vulkan の `VK_KHR_shader_float_controls`（RTE・DenormPreserve・SignedZeroInfNanPreserve）と SPIR-V の `NoContraction`
  を確認できる device だけで許す。i64 は `shaderInt64` の device だけ。SPIR-V は LLVM の SPIR-V target をまず評価し、float controls の
  execution mode を出せない場合だけ独自 emitter にする。`Gpu.Auto` は転送を含む計測の閾値でだけ GPU を選び、選択を観測可能にする。
- 理由: 旧版の Phase 1（Vulkan の strict float）は新しい driver 依存と SPIR-V 基盤を要し、WGSL と既存の Dawn 試作で届く relaxed f32 より
  先に着手する理由がない。能力の確認なしに strict と称さない方針は保つ。
- 状態: 要承認（承認前は Phase 3 に着手しない）

## 実装状況（2026-10-10、base `96d7cbf`）

利用者の包括承認（`D1`・`D9`・`D10` を含む `要承認` のすべて）に基づく。実装は worktree `impl/f09-gpu` の Phase 1 と Phase 2。Phase 3 は別の担当者が行う。

### Phase 1（done）

- 実装: `Gpu.init_relaxed`・`Gpu.map_relaxed`（`std/Gpu.tz`）、`Emit::WgslRelaxed`（`--emit wgsl-relaxed`。`BuildOptions` に field なし。D3）、`GpuKernel::wgsl_relaxed`、
  1 行目の宣言 `// tsuzuri-gpu float=relaxed input=<t> output=<t>`、`bitcast<f32>(<bits>u)` のリテラル、`(-x)`・`/`・`f32(x)`、
  `webgpu.mjs` の f32 buffer（`Float32Array`、要素種別 `kind`、`prepare(source, { float: "relaxed" })`）。
- ticket から外れた点: ① `src/cache.rs` の `Emit::Llvm | Header | Wgsl` も直した（ticket の表にない）。② 現行の `--emit` の一覧（`shared`・`bindings-*`）に合わせてメッセージを
  `… wgsl, wgsl-relaxed, shared, …` にした（`src/main.rs` の既存テストの文面。ticket が許す変更）。③ f32 の `%` は言語にない（`Rem` は整数と `BigInt` だけ）ので、ticket の
  「float の剰余」の拒否は到達しない。専用の arm も診断も作っていない。④ bool lane は専用の文面（`relaxed WebGPU buffer lanes must be f32, i32, or i32u; bool lanes are unavailable`）。
  ⑤ ticket の「例」の WGSL は簡略化されていて、実際の出力は全式を `let value_N` に束縛する（strict と同じ生成規則）。テストは実際の出力を完全一致で比べる。
  ⑥ `fromArray` の buffer に `COPY_SRC` を足した（`toArray` が upload 直後の buffer でも検証を通る。以前は未使用の経路）。
- 検証: `cargo test --locked --test gpu` 7 passed、`--emit wgsl` の strict 出力は base と byte 一致（`cmp`）、`node tests/gpu.mjs`（CPU のみ）は 783 の整数参照と 1,315 の緩い f32 参照が成功、
  `TSUZURI_WEBGPU=1 node tests/gpu.mjs`（Apple M1 Max、Dawn 0.6.1 の Metal）は成功。最大誤差は poly 1・horner 1・ratio 2・index 0・threshold 0 ulp（許容 4・12・9・5・0）。
  性能は `docs/benchmarks.md`（9 回の中央値。転送を含む値と常駐の値を分け、速度の優位は主張しない）。
