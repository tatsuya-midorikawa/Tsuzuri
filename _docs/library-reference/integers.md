# Int

[ドキュメントのトップ](../README.md)

Int は整数幅と符号を保持する組み込み API です。通常の折り返し演算に加え、checked、飽和、ビット操作を明示的に選べます。Int はモジュール名であり、具体的な整数型名ではありません。桁数に上限のない整数は [BigInt](bigint.md) の `bigint` です。

## 境界値の扱い

```tsuzuri run=42
let checked = Int.checked_add 127i8 1i8
assert (Maybe.is_none ref checked)
assert (Int.saturating_add 127i8 1i8 == 127i8)
assert (Int.unsigned_abs (-128i8) == 128i8u)
assert (Int.rotate_left 1i8u 1 == 2i8u)
42
```

通常の `127i8 + 1i8` は -128 に折り返します。checked_add は None、saturating_add は最大値 127 です。必要な契約を呼び出し名で選びます。式に `@checked` を付けると、overflow で [OverflowException](#checked-と-overflowexception) を送出します。

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

rotate の量は `amount &&& (bits - 1)` です。通常のシフト演算子と異なり、量の型は常に i64 です。

## シフトとビット演算子

```tsuzuri run=-1
let value = -16
assert (value >>> 2 == -4)
assert (Bits.ushr value 28 == 15)
assert (240uy >>> 4 == 15uy)
assert (((0b1100 &&& 0b1010) ||| (1 <<< 4)) == 24)
~~~0
```

| 演算子・API | 動作 |
| --- | --- |
| `a &&& b`, `a \|\|\| b`, `a ^^^ b` | ビットごとの論理積、論理和、排他的論理和 |
| `~~~a` | 全 bit の反転 |
| `a <<< n` | 左シフト |
| `a >>> n` | 右シフト。符号付き整数は符号を複製する算術シフト、符号なし整数はゼロを入れる論理シフト |
| `Bits.ushr a n` | 符号付き整数でもゼロを入れる論理右シフト |

旧表記の `&` `|` `^` `~` も同じ意味で使えます。シフト量 n は a と同じ型で、bit 幅でマスクします。例えば i32 の `1 <<< 33` は `1 <<< 1` と同じ 2 です。`>>` と `<<` はシフトではなく関数合成です。

`Bits` はこれらの演算子をまとめた組み込みクラスで、ushr はそのメソッドです。シフトは `+` `-` より弱く比較より強く結合しますが、`&&&` `|||` `^^^` は `==` より弱いので、上の例のようにビット演算の結果を比較するときは括弧で囲みます。演算子の一覧は[式と演算子](../language-reference/expressions-and-operators.md)を参照してください。

## checked と saturating

| API | 失敗・overflow の動作 |
| --- | --- |
| `checked_add`, `checked_sub`, `checked_mul` | 同型二引数。範囲内なら Some、overflow は None |
| `checked_div`, `checked_rem` | ゼロ除算と MIN / -1 も None |
| `checked_neg` | 符号付き一引数。MIN は None |
| `saturating_add`, `saturating_sub`, `saturating_mul` | 数学的結果を型の最小値・最大値へ飽和 |

checked は失敗を値にする API です。同じ入力でも通常の `/` や `%` のトラップ契約は変わりません。saturating_div など、一覧にない飽和演算は提供しません。

## 累乗と拡大乗算

```tsuzuri run=87
let two = 2
assert (2 ** 3 ** 2 == 512)
assert (-two ** 2 == 4)
assert (Int.wrapping_pow 2 10 == 1024)
assert (2.0 ** 0.5 > 1.414)
let base = 7y
base ** 3
```

`**` は `Pow` クラスの演算子です。右結合で `*` より強く結合し、単項 `-` はさらに強いので `-two ** 2` は 4 です。整数では指数も底と同じ型で、負の指数はトラップ、overflow は折り返します。上の `7y ** 3` は 343 を i8 へ折り返した 87 です。f32 / f64 は libm の pow を使い、[bigint](bigint.md) も `**` を持ちます。

`wrapping_pow base exponent` と `checked_pow base exponent` は i64 の指数で二乗法を使います。指数 0 は 1、負指数は前者がトラップ、後者が None です。overflow はそれぞれ折り返し / None になります。

`widening_mul left right` は同じ符号の倍幅整数を返します。例えば i32 なら i64、i64u なら i128u です。128-bit 入力のさらに倍幅はなく、`E1005` です。

unsigned_abs / abs_diff / widening_mul は返却型が入力幅から決まる型族を使います。呼び出し位置で具体幅を確定させる必要があり、型変数のまま残る汎用ラッパーは `E1015` です。

## @checked と OverflowException

```tsuzuri run=-2147483648%3A%20Arithmetic%20operation%20resulted%20in%20an%20overflow.
def add :: i32 -> i32 -> Result<i32, Exception> = \x y ->
    try
        @checked x + y
    with
    | e is OverflowException -> e

let wrapped = 2147483647 + 1
match add 2147483647 1 with
| Result.Ok value -> $"{value}"
| Result.Error e -> $"{wrapped}: {e.msg}"
```

式や文の前に `@checked` を付けると、その中の整数の `+` `-` `*` `**` と単項 `-` が overflow したとき、折り返す代わりに `OverflowException` を送出します。結果の型は変わらず、`/` と `%` は対象外です。`try ... with` はこれを捕捉し、`Result<'T, Exception>` の Error にします。

検査と捕捉は字句的です。`@checked` は書いた式の中の演算だけを検査し、`@checked f x` でも f の本体の演算は検査しません。送出した例外は同じ関数本体で最も内側の `try` へ移り、呼び出しや lambda の境界は越えません。捕捉する `try` がなければ `trap: unhandled OverflowException: arithmetic operation resulted in an overflow` と位置を報告してトラップします。詳しくは[エラー処理](../language-reference/error-handling.md)を参照してください。

## 性能と移植性

対応する LLVM 演算・intrinsic を使いますが、CPU の特定命令の有無や実行時間を API 名で保証しません。native / WASM で上記の値と失敗条件を維持し、未定義のゼロ入力 count や未検査の除算を利用しません。

## 関連項目

- [整数のリテラルと as](../language-reference/numbers.md)
- [式と演算子](../language-reference/expressions-and-operators.md)
- [エラー処理](../language-reference/error-handling.md)
- [BigInt](bigint.md)
- [Maybe と失敗の扱い](maybe-result.md)
- [Math](math.md)
