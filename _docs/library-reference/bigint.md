# BigInt

[ドキュメントのトップ](../README.md)

`bigint` は桁数に上限のない符号付き整数です。std の record `BigInt` の型名で、`I` 接尾辞のリテラル（`123I`、`0xFFI`）か、`bigint` を期待する位置に書いた接尾辞なしの整数リテラルで作ります。固定幅の整数の API は [Int](integers.md) です。

## 例

```tsuzuri run=1267650600228229401496703205376
let large = 2I ** 100
assert (Maybe.is_none (BigInt.to_i64 (ref large)))
assert (large > 9999999999999999999999999999I)
large
```

指数の `100` は bigint を期待する位置にあるので bigint になります。結果は i64 に収まらないため、to_i64 は None です。トップレベルの結果は Display で十進表記になります。

## 演算

| 演算 | 規則 |
| --- | --- |
| `+`, `-`, `*`, 単項 `-` | 桁が増えるだけで overflow しない |
| `/` | 0 方向へ切り捨てる。ゼロ除算はトラップ |
| `%` | 符号は被除数と同じ。ゼロ除算はトラップ |
| `**` | 指数も bigint。負の指数と 10^18 以上の指数はトラップ |
| `==`, `!=`, `<`, `<=`, `>`, `>=` | 数値として比較し、両辺を借用 |

```tsuzuri run=-3%20-2%20123456789012345678901234567891
let quotient = -17I / 5I
let remainder = -17I % 5I
let parsed = Maybe.get (BigInt.of_string "123456789012345678901234567890")
$"{quotient} {remainder} {parsed + 1I}"
```

## API

| API | 型・動作 |
| --- | --- |
| `BigInt.of_i64 value` | `i64 -> bigint` |
| `BigInt.to_i64 value` | `ref bigint -> Maybe<i64>`。範囲外は None |
| `BigInt.of_string text` | `ref string -> Maybe<bigint>`。先頭の省略可能な `-` と十進数字だけを受け付け、空文字列、`+`、空白、その他の文字は None |
| `BigInt.compare left right` | `ref bigint -> ref bigint -> i64`。小さい・等しい・大きいを -1・0・1 で返す |

instance は Add、Sub、Mul、Div、Rem、Pow、Neg、Eq、Ord、Display、Parse、Hash、Default です。Parse は of_string と同じ規則で、`Parse.parse` からも使えます。Default は 0 です。Hash と Eq を持つので HashMap のキーにもなります。

## 表現と計算量

内部は符号と、下位から並べた 10^9 進の桁の配列です。ゼロは桁を持たず、符号も負になりません。この表現は不透明で、`BigInt` モジュールの外からは構築・field の読み書き・パターンでの分解ができません（E1022）。値はリテラル、`BigInt.of_i64`・`BigInt.of_string`、演算で作ります。乗算は桁数の積に比例する筆算、除算は商の各桁を二分探索する筆算、`**` は二乗法です。演算ごとに新しい桁配列を確保するので、固定幅整数の一命令の演算とは費用が異なります。速度の測定値は載せていません。

## 関連項目

- [Int](integers.md)
- [整数のリテラルと as](../language-reference/numbers.md)
- [表示と解析](formatting-and-parsing.md)
- [BigInt のソース宣言](api/BigInt.md)
