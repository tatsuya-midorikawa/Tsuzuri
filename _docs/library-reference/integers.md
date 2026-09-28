# Int

[ドキュメントのトップ](../README.md)

Int は整数幅と符号を保持する組み込み API です。通常の折り返し演算に加え、checked、飽和、ビット操作を明示的に選べます。Int はモジュール名であり、具体的な整数型名ではありません。

## 境界値の扱い

```tsuzuri run=42
let checked = Int.checked_add 127i8 1i8
assert (Option.is_none ref checked)
assert (Int.saturating_add 127i8 1i8 == 127i8)
assert (Int.unsigned_abs (-128i8) == 128i8u)
assert (Int.rotate_left 1i8u 1 == 2i8u)
42
```

通常の `127i8 + 1i8` は -128 に折り返します。checked_add は None、saturating_add は最大値 127 です。必要な契約を呼び出し名で選びます。

## 選択と絶対値

| API | 型・条件 |
| --- | --- |
| `min left right`, `max left right` | 同じ整数型 T から T を選ぶ |
| `clamp value low high` | T 三つ。下限 > 上限ならトラップ |
| `abs value` | 符号付き T。最小値の結果は最小値のまま |
| `unsigned_abs value` | 符号付き T から同幅の符号なし型へ |
| `abs_diff left right` | 同じ整数型二つから、同幅の符号なし絶対差 |

比較は入力型の符号に従います。abs の最小値を正の同幅符号付き整数で表せない場合に、型を勝手に広げません。

## ビット操作

| API | 結果・境界 |
| --- | --- |
| `count_ones value` | 立っている bit の個数を i64 で返す |
| `leading_zeros value`, `trailing_zeros value` | i64。ゼロ入力では型の bit 幅 |
| `rotate_left value amount`, `rotate_right value amount` | 同じ型。amount は i64 で、負数も幅マスクで処理 |
| `swap_bytes value` | バイト順を反転。8-bit では恒等 |
| `reverse_bits value` | 全 bit の順序を反転 |
| `is_power_of_two value` | 符号なし整数専用。ゼロは false |

rotate の量は `amount & (bits - 1)` です。通常のシフト演算子と異なり、量の型は常に i64 です。

## checked と saturating

| API | 失敗・overflow の動作 |
| --- | --- |
| `checked_add`, `checked_sub`, `checked_mul` | 同型二引数。範囲内なら Some、overflow は None |
| `checked_div`, `checked_rem` | ゼロ除算と MIN / -1 も None |
| `checked_neg` | 符号付き一引数。MIN は None |
| `saturating_add`, `saturating_sub`, `saturating_mul` | 数学的結果を型の最小値・最大値へ飽和 |

checked は失敗を値にする API です。同じ入力でも通常の `/` や `%` のトラップ契約は変わりません。saturating_div など、一覧にない飽和演算は提供しません。

## 累乗と拡大乗算

`wrapping_pow base exponent` と `checked_pow base exponent` は i64 の指数で二乗法を使います。指数 0 は 1、負指数は前者がトラップ、後者が None です。overflow はそれぞれ折り返し / None になります。

`widening_mul left right` は同じ符号の倍幅整数を返します。例えば i32 なら i64、i64u なら i128u です。128-bit 入力のさらに倍幅はなく、`E1005` です。

unsigned_abs / abs_diff / widening_mul は返却型が入力幅から決まる型族を使います。呼び出し位置で具体幅を確定させる必要があり、型変数のまま残る汎用ラッパーは `E1015` です。

## 性能と移植性

対応する LLVM 演算・intrinsic を使いますが、CPU の特定命令の有無や実行時間を API 名で保証しません。native / WASM で上記の値と失敗条件を維持し、未定義のゼロ入力 count や未検査の除算を利用しません。

## 関連項目

- [整数のリテラルと as](../language-reference/numbers.md)
- [Option と失敗の扱い](option-result.md)
- [Math](math.md)
