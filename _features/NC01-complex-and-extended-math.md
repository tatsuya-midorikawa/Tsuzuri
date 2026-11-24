# NC01: 複素数型 `Complex<'a>` と組み込み `Math` の不足関数群

| 項目 | 内容 |
| --- | --- |
| ID | NC01 |
| 優先度 | P1（`Linalg` の一般固有値分解・`Fft`・数値安定な統計・科学技術計算の前提） |
| 規模 | M（`std/Complex.tz` と組み込み `Math` 関数の追加、native / WASM `-O0`・`-O3` 参照検証） |
| 依存 | – |
| 後続 | [NC06](NC06-tensor-ufuncs-and-elementwise.md), [NC08](NC08-linalg-core-and-einsum.md), [NC09](NC09-fft-and-polynomial.md), [NC10](NC10-random-distributions-and-generators.md), [NC11](NC11-tensor-codec-and-io.md), [NC12](NC12-ffi-numeric-buffers-and-blas-lapack.md) |
| 状態 | todo |
| 起票 | 2026-11-24（[GUIDE D-65](GUIDE.md#d-65-数値計算基盤と-numpy-互換機能案-c-ハイブリッドnc01nc14)。HEAD `27069465` で確認） |
| 承認 | 要承認（新規 opt-in std モジュール `Complex` の `RESERVED_MODULES` 登録と新規 `Math.*` 組み込み関数群。決定事項 D1・D2 が要承認、ほかは既定案） |
| 手本にする既存実装 | `src/llvm_math.rs`、`src/check.rs` の `Builtin::Math*`、`src/runtime/math.ll`、`std/Math.tz`、`std/Tensor.tz`、`tests/math.mjs` |
| 主な影響ファイル | `std/Complex.tz`（新規）、`src/stdlib.rs`、`src/check.rs`、`src/llvm_math.rs`、`src/abi.rs`（`RESERVED_HOST_SYMBOLS`）、`src/effects.rs`、`src/pure_rules.rs`、`src/capabilities.rs`、`src/runtime/math.c`、`src/runtime/musl/`（新規 `.c` ソース追加）、`src/runtime/generate_math.py`、`src/runtime/math.ll`（生成物）、`tests/complex.rs`（新規）、`tests/math.mjs`、`tests/std_argument_order.rs`、`tests/std_examples.rs`、`tests/fixtures/effects_sweep/`、`vsc/src/core.ts`、`_tsuzuri/language-reference/built-in-types-and-modules/math.md`、`_tsuzuri/language-reference/built-in-types-and-modules/int.md`、`_tsuzuri/language-reference/built-in-types-and-modules/complex.md`（新規）、`_tsuzuri/language-reference/index.md`、`docs/language.md` |

## 実装者向け早見

- 範囲: (1) opt-in の std モジュール `Complex` に `record Complex<'a> { re: 'a, im: 'a }`（`Float<'a>` 向け。`Copy`/`Send`/`Sync`/`Eq`/`Display`、四則・`conj`・`abs`・`arg`/`angle`・`polar`・`exp`/`log`/`sqrt`/`sin`/`cos` 等）を追加する。(2) 組み込み `Math` に NumPy / C99 互換の不足関数群（`Elementary`: 双曲線・逆双曲線 `sinh`/`cosh`/`tanh`/`asinh`/`acosh`/`atanh`、精度重視 `expm1`/`log1p`/`logaddexp`/`logaddexp2`/`sinc`、角度・剰余・分解 `degrees`/`radians`/`fmod`/`floor_div`/`floor_mod`/`ieee_remainder`/`modf`/`frexp`/`ldexp`/`nextafter`、`Float`: 符号 `sign`/`signbit`、整数 `Int.gcd`/`Int.lcm`/`Int.checked_gcd`/`Int.checked_lcm`/`Int.div_floor`/`Int.rem_floor`）を追加する。
- 最初に読む: ① 仕様 1〜3 ② `src/llvm_math.rs` ③ `src/check.rs` の `Builtin::MathSin` 周辺 ④ `tests/math.mjs`。
- 最初に書くテスト: `tests/complex.rs`（`Complex<f64>` の四則・除算のオーバーフロー耐性・`conj`/`abs`/`polar`）と `tests/math.mjs` への新関数参照ケース（Python / mpmath 等の独立参照値）。
- 触ってはいけない: 既存の `Math.*` 関数の精度・ビット契約・名前。`Complex` を名指さないプログラムの生成 IR（opt-in を保つ）。
- 完了の見分け方: `Complex.of 3.0 4.0 |> Complex.abs` が `5.0`、`Math.expm1 1e-12` が `1e-12` 相当を 1 ulp 以内で返し、native と WASM の `-O0`・`-O3` でビット一致する。

## 目的

現状の Tsuzuri は [numeric.rs](../src/numeric.rs) に実数・整数・10 進型を備えているが複素数型を持たず、組み込み `Math` も双曲線関数や `expm1`/`log1p` 等の数値安定関数を欠いている。FFT、非対称行列の固有値分解、確率分布やソフトマックス等の安定計算（`log1p`/`logaddexp`）を構築する土台としてこれらを整備する。

## 着手条件と停止条件

### 着手条件

- 決定事項 D1・D2 の承認。
- `grep -n "RESERVED_MODULES" src/stdlib.rs` と `Builtin::ALL` の件数を控える（GUIDE-LITE §11）。

### 停止条件

- **S1** 追加した `Math.*` 超越関数で、native と WASM、または `-O0` と `-O3` の間で非 NaN ビット列が一致しない。
- **S2** `Complex` を名指さないプログラムの IR が 1 byte でも変わる。
- **S3** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `27069465` で確認）

- `std/` に複素数モジュールは存在しない。`RESERVED_MODULES` は 73 個（`src/stdlib.rs`）。
- 組み込み `Math` は `sqrt`/`floor`/`ceil`/`trunc`/`round`/`round_even`/`abs`/`min`/`max`/`clamp`/`fma`/`copysign`/`is_nan`/`is_infinite`/`is_finite`/`pi`/`e`（`Float`）と `sin`/`cos`/`tan`/`asin`/`acos`/`atan`/`atan2`/`exp`/`exp2`/`log`/`log2`/`log10`/`pow`/`cbrt`/`hypot`（`Elementary`: `f32`/`f64` のみ）を持つ。
- `Math` の超越関数はホスト `libm` に依存せず、同梱の数学実装（`src/runtime/math.ll`）で 1 ulp 以内かつ native/WASM・`-O0`/`-O3` ビット一致の契約を持つ（`docs/language.md`「数学 API」）。

## 変えてはいけない意味

- 既存の `Math.*` の 1 ulp・ビット一致・`NaN`・符号付きゼロ契約（GUIDE §1）。
- 暗黙の `f32` ↔ `f64` 変換や `fast-math` を入れない。

## 仕様

### 1. `std/Complex.tz`（opt-in）

- 型: `record Complex<'a> { re: 'a, im: 'a }`（公開フィールドまたは不透明かは D1 で `re`/`im` 読み取り公開レコードとし、`Copy` を維持するためスカラー `Float` 要素で値型になる）。
- 生成・分解: `Complex.of :: 'a -> 'a -> Complex<'a>`、`Complex.from_real`、`Complex.from_imag`、`Complex.polar :: Elementary<'a> => 'a -> 'a -> Complex<'a>`、`Complex.zero`、`Complex.one`、`Complex.i`、`Complex.real`、`Complex.imag`。
- 基本演算（すべて `pure def`、D-59 に従い単項は対象を最後、二項は `Add`/`Sub`/`Mul`/`Div`/`Neg` インスタンスも提供）:
  - `add`, `sub`, `mul`, `div`（スミス法等のスケーリング除算で中間オーバーフローを抑止）, `neg`, `scale`, `conj`
  - `abs`（`Math.hypot re im`）, `norm_sq`, `arg`（`Math.atan2 im re`）
  - `sqrt`, `exp`, `log`, `pow`, `sin`, `cos`, `tan`, `sinh`, `cosh`, `tanh`（`Elementary<'a>`）
  - `is_nan`, `is_infinite`, `is_finite`
- 型クラスインスタンス: `Eq`, `Display`（例: `3+4i` / `3-4i`）, `Default`, `Add`, `Sub`, `Mul`, `Div`, `Neg`。なお、Tsuzuri のインスタンス文脈では `Float<'a>` が演算子クラスや `Copy` を自動含意しないため（`E1027`、`std/Monoids.tz:28` と同じ規則）、`Add` は `(Float<'a>, Add<'a>) =>`、`Mul` は `(Float<'a>, Add<'a>, Sub<'a>, Mul<'a>, Copy<'a>) =>` のようにメソッド本体が使用する演算子クラスおよび `Copy<'a>` を明示列挙する。

### 2. 組み込み `Math` / `Int` の拡張

- `Elementary<'a>`（`f32` / `f64` のみ、1 ulp 以内・native/WASM・`-O0`/`-O3` ビット一致・`libm` 非依存。`f16`/`f128`/`d32`〜`d128` には 2 進仮数/指数分解や 10 進変換を強要しないため `Elementary` に限定する）:
  - 双曲線・逆双曲線: `Math.sinh`, `Math.cosh`, `Math.tanh`, `Math.asinh`, `Math.acosh`, `Math.atanh`
  - 数値安定・特殊関数: `Math.expm1`, `Math.log1p`, `Math.logaddexp`, `Math.logaddexp2`, `Math.sinc`（NumPy と同じ正規化版 $\sin(\pi x)/(\pi x)$、`x == 0` で `1.0`）
  - 角度・剰余・分解:
    - `Math.degrees`, `Math.radians`
    - `Math.fmod`（C99 0 方向丸め剰余）、`Math.floor_div`・`Math.floor_mod`（床除算・床剰余。NumPy の `np.floor_divide`・`np.remainder`/`np.mod` に相当し `floor_mod (-7.0) 3.0 == 2.0`）、`Math.ieee_remainder`（IEEE 754 最近接偶数商の剰余。`ieee_remainder (-7.0) 3.0 == -1.0` で、NumPy `np.remainder` との同名混同を防ぐため `ieee_` 接頭辞を付ける）
    - `Math.modf :: Elementary<'a> => 'a -> ('a * 'a)`（小数部・整数部）
    - `Math.frexp :: Elementary<'a> => 'a -> ('a * i32)`、`Math.ldexp :: Elementary<'a> => 'a -> i32 -> 'a`
    - `Math.nextafter :: Elementary<'a> => 'a -> 'a -> 'a`
- `Float<'a>`（`f16`〜`f128`, `d32`〜`d128` の全 7 型）:
  - `Math.sign`（正は `+1`、負は `-1`、`NaN` は `NaN`、`+0.0` は `+0.0`、`-0.0` は Tsuzuri の丸め契約「結果がゼロのとき元の符号を保つ」に従い `-0.0` とする。NumPy の `np.sign(-0.0) == +0.0` との差分としてドキュメントへ明記する）
  - `Math.signbit :: Float<'a> => 'a -> bool`
- `Int`（組み込み `Integer` 向け）:
  - `Int.gcd :: Integer<'a> => 'a -> 'a -> 'a`、`Int.lcm :: Integer<'a> => 'a -> 'a -> 'a`: 内部では同幅符号なし絶対値でユークリッドの互除法を計算し、`gcd(0, 0) = 0`、一方が `0` の `lcm` は `0`。符号付き最小値（例: `Int.gcd (-128i8) 0i8` や `Int.gcd (-128i8) (-128i8)` で数学上の値 `+128` が `i8` に収まらない場合）および `lcm` の型幅超過は既存の `Int.abs` / 通常演算子と同じく同幅折り返しとする。
  - `Int.checked_gcd :: Integer<'a> => 'a -> 'a -> Maybe<'a>`、`Int.checked_lcm :: Integer<'a> => 'a -> 'a -> Maybe<'a>`: 符号付き最小値表現不可または乗算オーバーフロー時に `Maybe.None` を返す安全版（既存の `Int.checked_*` 群と一貫）。
  - `Int.div_floor :: Integer<'a> => 'a -> 'a -> 'a`、`Int.rem_floor :: Integer<'a> => 'a -> 'a -> 'a`（負の無限大方向へ丸める床除算・床剰余。NumPy の `np.floor_divide` / `np.remainder` と一致し、ゼロ除算および符号付き最小値 `div_floor (-1)` は既存 `/` と同じくトラップ）。

### 3. 効果と権限

- 追加するすべての `Math.*`・`Int.gcd`/`lcm`/`checked_gcd`/`checked_lcm`・`Complex.*` 関数は純粋（`pure`、`@no_alloc`、`@no_trap`）、権限不要（`Int.div_floor`/`Int.rem_floor` は `pure`・`@no_alloc` かつゼロ除算/最小値除算のみ既存 `/` と同じトラップ契約）。

## 設計

- `src/runtime/musl/` には現在 `sinh`〜`nextafter` 用の `.c` ファイルが存在しないため、musl 1.2.5 から該当 `.c` ソース（および依存ヘルパ）を追加し、`src/runtime/generate_math.py` で `src/runtime/math.ll` を再生成する（`math.ll` を手で編集しない。GUIDE-LITE §7）。`math.ll` に追加された `tz_` 以外の内部シンボルがある場合は `src/abi.rs` の `RESERVED_HOST_SYMBOLS`（昇順）へ登録し、`reserved_symbols_cover_the_runtime_names` を通す。`fmod`/`frexp`/`ldexp`/`nextafter`/`signbit` は LLVM 組み込み命令または純ビット操作／musl ソースによる決定論的実装で native と WASM 同一ビットを保証する。
- `std/Complex.tz` は `OPT_IN` に `"Complex"`（`uses: &[]`）として登録し、`RESERVED_MODULES` に追加する（`src/stdlib.rs:843` の件数アサーションも同時更新する）。`tests/std_argument_order.rs` の `MODULES` には免除（`Exempt`）を使わず **`("Complex", subject(&["Complex"]))`** として登録する（`scale factor z` のように対象 `Complex` を最後の引数に統一すれば AGENTS.md の規則に完全適合する）。
- 追加した全公開関数は `tests/fixtures/effects_sweep/`（または専用 fixture）で 1 回以上呼び出し、`tests/effects_alloc_conservative.rs` のコーパス網羅検査を通す。
- `Complex` は 2 つの `Float` フィールドを持つ平たい値型レコードなので、ヒープ確保なしで `Array<Complex<f64>>` や `Tensor<Complex<f64>>` の連続メモリにそのまま格納できる（std レコードは `llvm_layout::record_slots` で並べ替え対象外のため、C の `double _Complex` と同一の `[re, im]` 2 要素配置）。

## 前提とする他チケットのインターフェース

- なし（独立して着手可能）。

## 他チケットへ提供するインターフェース

- `Complex<'a>` 型と全演算 API（[NC06](NC06-tensor-ufuncs-and-elementwise.md) の複素数 ufunc、[NC08](NC08-linalg-core-and-einsum.md) の `eig`・複素線形代数、[NC09](NC09-fft-and-polynomial.md) の `Fft`、[NC11](NC11-tensor-codec-and-io.md) の `complex64`/`complex128` シリアライズ、[NC12](NC12-ffi-numeric-buffers-and-blas-lapack.md) の BLAS/LAPACK 複素バッファ）。
- 新規 `Math.*` 関数群（[NC06](NC06-tensor-ufuncs-and-elementwise.md) のテンソル ufunc、[NC10](NC10-random-distributions-and-generators.md) の分布計算）。

## 実装手順

1. **変更**: なし。**内容**: `Builtin::ALL`・`RESERVED_MODULES`・`tests/math.mjs`・`src/runtime/musl/` の現状ベースラインを控える。**確認**: `cargo test --locked --test std_argument_order`。
2. **変更**: `src/check.rs`, `src/llvm_math.rs`, `src/abi.rs`, `src/runtime/math.c`, `src/runtime/musl/`（新規 `.c`）, `src/runtime/generate_math.py`, `src/runtime/math.ll`（生成）, `src/effects.rs`, `src/pure_rules.rs`, `src/capabilities.rs`。**内容**: 新規 `Math.*` と `Int.gcd`/`Int.lcm`/`checked_gcd`/`checked_lcm`/`div_floor`/`rem_floor` の型・効果・lowering を実装する（`math.ll` は `generate_math.py` で生成し、`RESERVED_HOST_SYMBOLS` を同期）。**確認**: T0、`cargo test --locked --lib abi::tests::reserved_symbols_cover_the_runtime_names`、T1、`node tests/math.mjs target/debug/tsuzuri`（独立参照値で native / WASM `-O0`・`-O3` 照合）。
3. **変更**: `std/Complex.tz`, `src/stdlib.rs`（`RESERVED_MODULES` と 843 行の件数アサーション更新）, `tests/complex.rs`, `tests/std_argument_order.rs`, `tests/std_examples.rs`, `tests/fixtures/effects_sweep/`, `vsc/src/core.ts`。**内容**: `Complex<'a>` 型・全関数・型クラスインスタンス（メソッド本体が使う演算子クラスと `Copy` を文脈へ明示）・doc-test 例・コーパス網羅呼び出しを追加する。**確認**: T0、`cargo test --locked --test complex --test std_argument_order --test std_examples --test effects_alloc_conservative`、`tsuzuri test --doc std`、T1。
4. **変更**: `tests/fixtures/`・`tests/features.mjs`、`docs/language.md`、`_tsuzuri/language-reference/built-in-types-and-modules/math.md`・`int.md`・`complex.md`（新規）・`index.md`。**内容**: E2E 検証とドキュメント更新。**確認**: T2、`node scripts/check-docs.mjs`、`sh scripts/check-runtime-includes.sh`。

## テスト計画

- **単体（`tests/complex.rs`）**: `Complex<f32>`・`Complex<f64>` の四則・`div` の極大/極小スケーリング（`1e200` 等）・`conj`/`abs`/`arg`/`polar`/`exp`/`log`/`sqrt`・`NaN`/`Inf` 伝播・`Int.gcd`/`Int.lcm`/`checked_gcd`/`checked_lcm` の最小値境界（`-128i8`, `-2147483648i32`, `0`）・IR 決定性・`Complex` 未使用時の IR 非変更。
- **参照照合（`tests/math.mjs`）**: `sinh`〜`atanh`、`expm1`/`log1p`/`logaddexp`/`logaddexp2`/`sinc`（正規化版 $\sin(\pi x)/(\pi x)$）、`fmod`/`floor_div`/`floor_mod`/`ieee_remainder`/`modf`/`frexp`/`ldexp`/`nextafter`/`sign`/`signbit`、`Int.div_floor`/`Int.rem_floor` の通常値・境界値（`±0`, `±Inf`, `NaN`, 非正規化数, `1e-15` 付近の桁落ち域, 負の割られる数/割る数）を多倍長参照値と 1 ulp 以内かつ native/WASM `-O0`/`-O3` ビット一致で検証。

## ドキュメント

- `_tsuzuri/language-reference/built-in-types-and-modules/complex.md`（新規）
- `_tsuzuri/language-reference/built-in-types-and-modules/math.md`、`int.md`、`index.md`
- `docs/language.md`（数学 API・標準モジュール一覧）

## 受け入れ条件

- すべての新規 `Math.*` が native / WASM の `-O0`・`-O3` で 1 ulp 以内かつビット一致すること。
- `Complex<'a>` が `Copy`・ヒープ非確保で、`tsuzuri test --doc std`・`std_argument_order`・`std_examples`・`effects_alloc_conservative`・`reserved_symbols_cover_the_runtime_names`・T2 がすべて通ること。
- `Complex` を使わない既存 fixture の IR が変化しないこと。

## 落とし穴

- `Numeric` / `Float` / `Elementary` は組み込み印クラスのため、`Complex<'a>` に対して `instance Float<Complex<'a>>` は書けない（`E1016`）。複素数向け演算は `Complex` モジュール関数および `Add`〜`Div`/`Neg`/`Eq`/`Display`/`Default` インスタンスで提供する。
- インスタンス宣言では、ジェネリック関数と異なり `Float<'a>` を書いても `Add<'a>` や `Copy<'a>` は含意されない（`E1027`、`src/polymorph.rs:5132`）。メソッド本体で使う演算子クラスと `Copy<'a>` を必ず文脈へ並べること（前例 `std/Monoids.tz:28`）。また [NC03](NC03-borrowed-arithmetic-operators.md) の結果によって `Add`〜`Neg` インスタンスの仮引数規約が変わるため、NC01 が先行した場合は NC03 で `std/Complex.tz` も同時更新する。
- `generate_math.py` で生成される IR に `fast`/`reassoc`/`contract`/`llvm.fmuladd` が入らないこと、および musl ソースのライセンス通知（`src/runtime/musl/COPYRIGHT`）を保つこと。
- `RESERVED_HOST_SYMBOLS` は昇順配列であり、追加後にソート順が崩れるとテストが失敗する。また `src/stdlib.rs:843` の `RESERVED_MODULES.len()` アサーション更新を忘れないこと。

## 対象外

- 虚数リテラル構文（`3.0i` / `3.0j`）や複素数専用の組み込みプリミティブ型。
- `f16` / `f128` に対する `Elementary` 超越関数の拡張（`f16` SIMD 拡充は [NC13](NC13-tensor-simd-parallel-and-gpu-kernels.md) で扱う）。
- `bfloat16`・`datetime64`・`timedelta64` 型。

## 決定事項

- **D1（要承認）**: `Complex<'a>` は opt-in std モジュール `std/Complex.tz` として追加し、`RESERVED_MODULES` に `"Complex"` を登録する（`D-55` へ反映）。フィールド `re`・`im` は公開レコードとする（値型 `Copy` かつ連続メモリ 2 要素配置を保証するため）。
- **D2（要承認）**: `Math.sinh`〜`Math.nextafter`・`Math.floor_div`/`Math.floor_mod`/`Math.ieee_remainder`・`Int.gcd`/`Int.lcm`/`Int.checked_gcd`/`Int.checked_lcm`/`Int.div_floor`/`Int.rem_floor` を既存の組み込みモジュール `Math`/`Int` に追加する（新モジュールは増やさない）。`fmod`/`floor_div`/`floor_mod`/`ieee_remainder`/`modf`/`frexp`/`ldexp`/`nextafter`/`degrees`/`radians` は `Elementary<'a>`（`f32`/`f64`）に限定し、`Float<'a>` へ 10 進変換や 2 進固有演算を持ち込まない。
- **D3（既定案）**: 虚数リテラル構文（`3.0i` や `3.0j`）は追加しない。`Complex.of 3.0 4.0` または `Complex.i()` を使う（新リテラル接尾辞を避け、構文を簡潔に保つ）。
- **D4（既定案）**: 複素数除算はオーバーフロー・アンダーフロー耐性のあるスミス法（Smith's algorithm）を既定実装とし、native/WASM で同一の演算手順・ビット結果を保つ。
- **D5（既定案）**: `Int.gcd`/`Int.lcm` は `Int.abs`・通常整数演算子と同じく同幅折り返し契約とし、符号付き最小値や乗算溢れを値で検知する用途には `Int.checked_gcd`/`Int.checked_lcm` を提供する。
- **D6（既定案）**: NumPy との名称・意味衝突を防ぐため、IEEE 754 剰余は `Math.ieee_remainder`、床剰余は `Math.floor_mod` と明示命名する。`Math.sign (-0.0)` は既存の丸め契約（ゼロの符号保存）に合わせ `-0.0` とし、NumPy（`+0.0`）との差分をリファレンスへ明記する。
