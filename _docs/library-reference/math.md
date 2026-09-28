# Math と順序付き浮動小数点集計

[ドキュメントのトップ](../README.md)

Math の基本演算は入力の浮動小数点型を保持します。整数を暗黙に浮動小数点へ変換したり、広い型を f64 で代用したりしません。

## 基本関数

以下は `Float<T>`、すなわち f16 / f32 / f64 / f128 / d32 / d64 / d128 に対応します。表の関数名には Math を付けます。

| API | 型・規則 |
| --- | --- |
| `sqrt value` | `T -> T`。正しく最近接・偶数丸め。負数は NaN、-0 は -0 |
| `floor value`, `ceil value` | 負方向、正方向の整数値へ丸める |
| `trunc value` | 0 方向へ丸める |
| `round value` | 最も近い整数値。ちょうど半分は 0 から遠い方 |
| `round_even value` | 最も近い整数値。ちょうど半分は偶数 |
| `abs value` | 符号 bit を消す |
| `copysign value sign` | value の絶対値へ sign の符号 bit を付ける |
| `min left right`, `max left right` | NaN を伝播。異符号のゼロは -0 / +0 |
| `clamp value low high` | low <= high が false なら NaN。ほかは min(max(value, low), high) |
| `is_nan`, `is_infinite`, `is_finite` | `T -> bool` |
| `pi()`, `e()` | 期待型 T に一度だけ丸めた定数 |
| `fma left right addend` | `T -> T -> T -> T`。積和を一度だけ丸める |

丸めた結果がゼロなら元の符号を保ちます。bare な Math.pi は定数値ではなく関数値なので、`Math.pi()` と呼びます。互換用の `Math.zero()` は f64 固定です。

```tsuzuri run=42
let result: f32 = Math.fma 2f32 3f32 36f32
let circle: f64 = Math.pi()
assert (Math.round 2.5 == 3.0)
assert (Math.round_even 2.5 == 2.0)
assert (circle > 3.14 && circle < 3.15)
result as i64
```

## 超越関数

`Elementary<T>` を要求し、f32 / f64 にだけ対応します。

| API | 引数 |
| --- | --- |
| `sin`, `cos`, `tan`, `asin`, `acos`, `atan` | T 一つ |
| `exp`, `exp2`, `log`, `log2`, `log10`, `cbrt` | T 一つ |
| `atan2 vertical horizontal` | y、x の順 |
| `pow base exponent`, `hypot left right` | T 二つ |

精度契約は最終結果 1 ulp 以内で、正確丸めとは異なります。非 NaN の bit 列は native / WASM、O0 / O3 で一致する契約です。同梱の移植可能な数学実装を使い、ホスト libm や locale に依存しません。

f16 / f128 / decimal の超越関数は `E1005` で拒否し、f64 へ暗黙変換しません。

## 特殊値

- 無限大の三角関数、範囲外の asin / acos、負数の log は NaN。
- log の正負のゼロは負の無限大。
- sin / tan / asin / atan / cbrt はゼロの符号を保持。
- atan2 は象限と符号付きゼロを保持。
- `pow(value, 0)`、`pow(1, NaN)`、`pow(-1, inf)` は 1。負の底と非整数指数は NaN。
- hypot は中間の overflow / underflow を避け、無限大があれば NaN より優先して正の無限大。

このリストのカンマ付き表記は説明用です。Tsuzuri の通常の呼び出しは空白で引数を渡します。

## FMA

Math.fma は数学的な積和を対象型へ最近接・偶数で一度だけ丸めます。通常の `left * right + addend` は積と和の二回であり、結果が異なることがあります。decimal も binary 経由では処理しません。

NaN は伝播し、`0 * inf` と無限大の積に異符号の無限大を足す場合は NaN です。正確な取消しは +0、同符号のゼロ同士はその符号になります。subnormal を維持し、NaN payload の一致は保証しません。

## 順序を固定した Array 集計

```tsuzuri run=1
let values = [1e16, 1.0, -1e16]
assert (Array.sum (ref values) == 0.0)
Array.sum_kahan (ref values) as i64
```

| API | 順序 |
| --- | --- |
| `Array.sum values` | 正のゼロから添字昇順の逐次和 |
| `Array.sum_pairwise values` | 隣接二要素を加算し、奇数末尾はそのまま次段へ渡す固定木 |
| `Array.sum_kahan values` | 添字昇順の Kahan-Babuska-Neumaier 補償和 |
| `Array.dot left right` | 正のゼロから添字昇順に積と和を別丸め |
| `Array.dot_fma left right` | 同じ順序で各 step を明示 FMA にする |

sum 以外の上記 API は Float 用です。空の結果は +0、pairwise の一要素はその値をそのまま返します。内積は要素を読む前に長さ一致を検査し、不一致はトラップです。

pairwise の `[a, b, c, d, e]` は `((a+b)+(c+d))+e` の木です。現在は O(n) 作業領域を持つ逐次実装で、スレッド数や SIMD 幅によって木を変えません。

sum_kahan は通常の演算で補償値を更新します。無限大や NaN を特別に取り除かず、補償式が NaN なら結果も NaN です。補償和が任意の入力で数学的に完全な和を返すわけではありません。

Math 演算自身はヒープを確保しませんが、一般の部分適用で捕捉環境を作る費用は別です。暗黙 FMA、fast-math、自動並列化、SIMD の Math 関数は提供しません。

## API と関連項目

- [Math のソース宣言](api/Math.md): zero。型汎用 API はコンパイラ組み込み
- [Array の宣言](api/Array.md)
- [数値の言語仕様](../language-reference/numbers.md)
