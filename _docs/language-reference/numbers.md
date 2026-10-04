# 数値、リテラル、変換

[ドキュメントのトップ](../README.md)

数値の意味は native と WASM、および最適化レベルで変えません。整数の折り返しと浮動小数点の丸めを明示し、LLVM の未定義動作に依存する演算は検査します。

## リテラルと型

| 表記 | 意味 |
| --- | --- |
| `42`, `1_000` | 十進整数 |
| `0x2A`, `0b101010` | 十六進・二進整数 |
| `127i8`, `42i32u` | 型名の接尾辞付き整数 |
| `100y`, `0xFFuy`, `3000000000l` | 短い接尾辞付き整数 |
| `9999999999999999999999999999I`, `0xFFI` | 任意精度の [bigint](#bigint) |
| `1.5`, `1e3`, `1.2e-3` | 十進で記述する浮動小数点 |
| `1f16`, `0.1f32`, `0.1f128` | binary 浮動小数点の型指定 |
| `1.5hf`, `0.25f`, `0.1F` | 短い接尾辞付き binary 浮動小数点 |
| `0.1d32`, `0.1d64`, `0.1d128` | decimal 浮動小数点 |
| `0.1hm`, `0.1m`, `0.1M` | 短い接尾辞付き decimal 浮動小数点 |
| `'a'B`, `"text"B` | ASCII の [byte と byte 配列](strings-and-characters.md#バイト列リテラル) |

| 短い接尾辞 | 型 |
| --- | --- |
| `y`／`uy` | `i8`／`i8u` |
| `s`／`us` | `i16`／`i16u` |
| `u` | `i32u` |
| `l`／`ul` | `i64`／`i64u` |
| `L`／`UL` | `i128`／`i128u` |
| `I` | `bigint` |
| `hf`／`f`／`F` | `f16`／`f32`／`f128` |
| `hm`／`m`／`M` | `d32`／`d64`／`d128` |

桁区切りの `_` は数字の間に置きます。`86i64` や `1.5f32` のような型名の接尾辞も使えます。十六進・二進のリテラルに浮動小数点の接尾辞は付けられません（`0x10hf` はエラー）。

```tsuzuri run=42
let small = 100y
let mask = 0xFFuy
let count = 3000000000l
let wide = 86UL
let half = 1.5hf
let ratio = 0.25f
let quad = 0.1F
let tiny = 0.1hm
let price = 0.1m
assert (small == 100i8)
assert (mask == 255i8u)
assert (count == 3000000000i64)
assert (wide == 86i128u)
assert (half == 1.5f16)
assert (ratio == 0.25f32)
assert (quad == 0.1f128)
assert (tiny == 0.1d32)
assert (price == 0.1d64)
42
```

接尾辞のない整数リテラルの型は、Rust と同じく後の使用からも推論します。何も型を決めなければ `i32` です。`i32` に収まらず型も決まらないリテラルはエラーなので、`3000000000l` のように接尾辞を付けます。

```tsuzuri run=-2147483648
def total :: [i64] -> i64 = \values -> Array.sum values

let values = [3, 5, 8]
assert (total values == 16)
let ratio: f64 = 2
assert (ratio == 2.0)
let large = 2147483647
large + 1
```

`values` は後で `[i64]` を受け取る関数へ渡すので `[i64]` です。`large` は型を決める使用がないので `i32` になり、`large + 1` は i32 で折り返します。

浮動小数点・decimal が期待される位置の整数リテラルは、その型の値です（`let ratio: f64 = 2`）。`bigint` が期待される位置では `bigint` です。小数点・指数付きの値は、型が決まらなければ f64 です。接尾辞で固定した型を別の型へ暗黙変換しません。

型の範囲外の整数リテラルと、無限大へ丸められる浮動小数点リテラルはコンパイルエラーです。符号付き整数の最小値も負のリテラルとして記述できます。binary のリテラルは目的の幅へ直接丸め、f128 を f64 経由で作ることはありません。

## 整数演算

```tsuzuri run=42
let wrapped: i8 = 127i8 + 1i8
let narrowed = 255i64 as i8
let saturated = 1e30 as i8
assert (wrapped == -128i8)
assert (narrowed == -1i8)
assert (saturated == 127i8)
42
```

通常の `+` / `-` / `*` / `**`、符号反転、ビット演算は型幅で折り返します。符号なし整数に単項 `-` は使えません。符号付き最小値の反転は、その最小値になります。`@checked` を付けた式では、`+` / `-` / `*` / `**` と符号反転のオーバーフローが `OverflowException` になります（[例外処理](error-handling.md)）。

`/` はゼロ方向への切り捨て、`%` は被除数と同符号の剰余です。ゼロ除算と、符号付き最小値を `-1` で割る除算・剰余はトラップします。overflow を値として扱う場合は Int の checked API を使います。

```tsuzuri run=42
assert (2 ** 10 == 1024)
assert (2 ** 3 ** 2 == 512)
assert (-2 ** 2 == 4)
assert (2.0 ** 0.5 > 1.414)
let negative = -16
assert ((negative >>> 2) == -4)
assert ((Bits.ushr negative 28) == 15)
assert ((0xF0uy >>> 4) == 0x0Fuy)
assert ((1 <<< 33) == 2)
42
```

`**` はべき乗で右結合です。単項 `-` は `**` より強く結合するので、`-2 ** 2` は 4 です。整数の指数は底と同じ型で、負の指数はトラップします。f32 / f64 の `**` は同梱の数学ランタイムの pow で計算します。

シフト量は `amount & (bit_width - 1)` にマスクします（上の `1 <<< 33` は i32 なので `1 <<< 1` と同じ）。`<<<` は左シフト、`>>>` は符号付き整数では算術右シフト、符号なし整数では論理右シフトです。符号付きの値を論理右シフトするには `Bits.ushr value amount` を使います。シフトの右辺は左辺と同じ整数型です。`>>` / `<<` は関数合成で、シフトではありません。

## bigint

```tsuzuri run=1267650600228229401496703205376
let large = 9999999999999999999999999999I
let next = large + 1I
let two: bigint = 2
assert (-7I / 2I == -3I)
assert (-7I % 2I == -1I)
assert (BigInt.compare next large > 0)
let small = match BigInt.to_i64 (BigInt.of_i64 42l) with
| Some value -> value
| None -> 0l
assert (small == 42l)
two ** 100
```

`bigint` は任意精度の整数で、std のレコード `BigInt.BigInt` です。`I` 接尾辞のリテラルか、`bigint` が期待される位置の接尾辞のない整数で作ります。`+`、`-`、`*`、`/`、`%`、`**`、単項 `-`、比較、`Display`、`Parse`、`Hash`、`Default` を使えます。`/` はゼロ方向への切り捨て、`%` は被除数と同符号の剰余で、ゼロ除算はトラップします。

| API | 型と結果 |
| --- | --- |
| `BigInt.of_i64` | `i64 -> bigint` |
| `BigInt.to_i64` | `ref bigint -> Option<i64>`。i64 に収まらなければ None |
| `BigInt.of_string` | `ref string -> Option<bigint>` |
| `BigInt.compare` | `ref bigint -> ref bigint -> i64` |

## 浮動小数点

```tsuzuri run=42
let invalid = 0.0 / 0.0
let infinite = 1.0 / 0.0
assert (Math.is_nan invalid)
assert (Math.is_infinite infinite)
assert (!(invalid == invalid))
assert ((invalid as i64) == 0)
42
```

f16 / f32 / f64 / f128 は IEEE 754 binary、d32 / d64 / d128 は decimal です。最近接・偶数丸めを使い、NaN、無限大、符号付きゼロ、subnormal を保持します。NaN の payload、例外フラグ、変更可能な丸めモードは公開しません。

NaN の `==` と大小比較は false、`!=` は true です。浮動小数点のゼロ除算は整数と異なり IEEE 754 の結果になります。

通常の積和は積と和で別々に丸めます。単一丸めが必要なときだけ `Math.fma` を指定します。fast-math、暗黙の再結合、暗黙の FMA は有効にしません。

## decimal

```tsuzuri run=42
let sum = 0.1d128 + 0.2d128
assert (sum == 0.3d128)
42
```

| 型 | 有効十進桁 | 調整後の指数範囲 | 最小の非ゼロ値 |
| --- | --- | --- | --- |
| d32 | 7 | -95 から 96 | `1e-101` |
| d64 | 16 | -383 から 384 | `1e-398` |
| d128 | 34 | -6143 から 6144 | `1e-6176` |

decimal は無制限精度でも、任意の丸め規則を持つ金額型でもありません。f16、f128、decimal の演算は同梱の整数ベース実装を使い、binary64 の近似で代用しません。

## as による変換

| 変換 | 契約 |
| --- | --- |
| 整数から狭い整数 | 下位ビットを保持 |
| 整数から広い整数 | 変換元の符号に従って拡張 |
| 同幅の符号の変更 | ビット列を保持 |
| 数値から浮動小数点 | 目的の形式へ最近接・偶数丸め |
| 浮動小数点から整数 | ゼロ方向に切り捨て、範囲外は飽和、NaN は 0 |
| binary と decimal の間 | 中間の別形式を介さず対応する直接変換 |

整数同士の as は飽和変換ではありません。上の例の `255i64 as i8` は `127` ではなく `-1` です。文字型は数値型ではないので、Char / Utf8Char の専用変換を使います。

`to_float :: i64 -> f64` と `to_int :: f64 -> i64` は互換用の固定型です。同じ変換であれば as と同じ意味・lowering を使います。他の型には as を使います。

## 関連項目

- [型と推論](types.md)
- [演算子と評価順序](expressions-and-operators.md)
- [数学と整数 API の正式な契約](../../docs/language.md#数学-api)
