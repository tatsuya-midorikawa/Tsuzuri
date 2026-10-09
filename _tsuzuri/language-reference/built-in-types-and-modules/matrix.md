# Matrix

`Matrix<'a>` は、行と列を持つ行列を 1 本の連続したバッファで持つ型です。要素 `(row, col)` は、行優先で `data[row * cols + col]` に並びます。`[[T]]` のように行ごとに別々の確保をせず、行の長さも揃っています。

構築、要素の参照、行の借用、転置、要素ごとの変換と集計、加算、行列積を提供します。行列積は、出力の各要素を足す順序を API の約束として固定しています。どの機械でも、native でも WASM でも、`-O0` でも `-O3` でも、同じビットを返します。

## この記事のポイント

- 型は `Matrix<'a>`。要素型にかかわらず常に非 Copy 型です。コピーは明示的に書きます。
- 内部は不透明です。構築・フィールド参照・パターン分解・`{ m with ... }` は `E1022` です。公開 C ABI への export は `E1008` です。
- `of_array` と `to_array` は所有権を移すだけで、要素を複製しません。`at`・`row`・`as_array` は確保なしの共有借用です。
- `mul` の出力要素は `+0` から `k = 0, 1, …` の順に `total = total + (left * right)` を行った値です。積と和を別々に丸め、FMA にまとめません。
- 添字・形・次元の誤りは、確保や callback の前に `assert` でトラップします。
- `Matrix` という名前を書いたプログラムだけが、このモジュールを読み込みます。書かないプログラムの生成コードは変わりません。ファイル名 `Matrix.tz` は予約名で `E1011` です。

## 作る

`Matrix.of_array rows cols values` は、行優先の平らな配列をそのまま所有します。`values.length` が `rows * cols` に一致しないとトラップします。

`Matrix.init rows cols initializer` は、`initializer row col` を行優先の順に、要素ごとにちょうど 1 回呼びます。

```tsuzuri run=0%2C1%2C2%2C10%2C11%2C12
let m = Matrix.init 2 3 (\row col -> row * 10 + col)
let flat = Matrix.as_array (ref m)
$"{flat[0]},{flat[1]},{flat[2]},{flat[3]},{flat[4]},{flat[5]}"
```

実行結果:

```text
0,1,2,10,11,12
```

要素数が 0 の行列（`rows == 0` または `cols == 0`）も作れます。このとき callback は呼ばれません。

## 読む

| API | 返すもの |
| --- | --- |
| `Matrix.rows (ref m)` / `Matrix.cols (ref m)` | 行数・列数 |
| `Matrix.at (ref m) row col` | 要素への共有借用 `ref 'a`。範囲外はトラップ |
| `Matrix.get (ref m) row col` | 要素のコピーの `Maybe<'a>`。範囲外は `None`（`Copy<'a>` が必要） |
| `Matrix.row (ref m) index` | 1 行を `ref ['a]`（長さ `cols`、複製なし）。行番号が範囲外ならトラップ |
| `Matrix.as_array (ref m)` | 全要素を行優先の `ref ['a]` で借用する |
| `Matrix.to_array m` | 行列を消費して、バッファを複製せずに `['a]` で返す |

`at` は行と列を別々に検査します。`data` の添字検査だけに任せると、2 行 3 列の `(0, 3)` が `(1, 0)` を返してしまうためです。

```tsuzuri run=ab%3Acd
let words = Matrix.init 1 2 (\_row col -> if col == 0 then "ab" else "cd")
let first = Matrix.at (ref words) 0 0
let second = Matrix.at (ref words) 0 1
$"{first}:{second}"
```

実行結果:

```text
ab:cd
```

要素が文字列のような非 Copy 型でも、`init`・`at`・`row`・`to_array` は使えます。値で読む `get`・`map`・`fold`・`transpose` と、算術の `add`・`mul` は、`Array` の同名の API と同じく `Copy<'a>`・`Numeric<'a>` を要求します。満たさない要素型は `E1005` です。

## 変換と集計

- `Matrix.map transform (ref m)` は、同じ形の新しい行列を返します。`transform` は行優先の順に 1 回ずつ呼ばれます。関数が先なのは `Array.map` と同じです。
- `Matrix.fold folder initial (ref m)` は、行優先で左から右に畳み込みます。
- `Matrix.transpose (ref m)` は、`cols` 行 `rows` 列の新しい行列を返します。出力 `(j, i)` が入力 `(i, j)` です。
- `Matrix.add (ref a) (ref b)` は、要素ごとの `left + right` です。形が違うとトラップします。

どれも入力を借用し、出力のバッファを 1 つだけ確保します。同じ行列を両方の引数に渡しても構いません。

```tsuzuri run=327
let a = Matrix.of_array 2 3 [1, 2, 3, 4, 5, 6]
let t = Matrix.transpose (ref a)
let p = Matrix.mul (ref a) (ref t)
let r = Matrix.row (ref p) 1
let s = Matrix.add (ref p) (ref p)
let flat = Matrix.to_array s
Array.sum r + deref (Matrix.at (ref p) 0 1) + flat[3] + Maybe.get (Matrix.get (ref p) 1 0)
```

実行結果:

```text
327
```

`p` は `[[14, 32], [32, 77]]` です。1 行目の和 `109`、`p(0, 1)` の `32`、`p + p` の `flat[3]` の `154`、`p(1, 0)` の `32` を足して `327` です。

## 行列積

`Matrix.mul (ref left) (ref right)` は、`left.cols` と `right.rows` が等しいときの行列積を返します。等しくないとトラップします。出力は `left.rows` 行 `right.cols` 列です。

出力 `(i, j)` は、`total = +0` から始め、`k = 0, 1, …, left.cols - 1` の順に

```text
total = total + (left(i, k) * right(k, j))
```

を行った値です。積と和は別々に丸めます。FMA にまとめることも、順序を入れ替えることも、fast-math を使うこともありません。これは `Array.dot` と同じ約束です。

- 内側の次元 `left.cols` が 0 のとき、出力の全要素は `+0` です。項がすべて `-0` の和も、`+0` から始めるので `+0` になります。
- NaN、無限大、符号付きゼロ、非正規化数は IEEE 754 のとおり伝播します。
- 整数型は幅ごとに折り返します。

```tsuzuri run=0
let left = Matrix.of_array 1 3 [1e16, 1.0, -1e16]
let right = Matrix.of_array 3 1 [1.0, 1.0, 1.0]
let product = Matrix.mul (ref left) (ref right)
deref (Matrix.at (ref product) 0 0)
```

実行結果:

```text
0
```

正確な和は `1` ですが、`1e16 + 1.0` は `1e16` に丸められ、次の `-1e16` で `0` になります。左から右に足すという約束が、そのまま結果に現れます。順序を変えると結果が変わる演算は、別の名前の API にします。

## トラップ

| 条件 | API | 表示 |
| --- | --- | --- |
| 次元が負、または `rows * cols` が `9223372036854775807` を超える | `of_array`・`init`・`mul` | `trap: assertion failed` |
| `values.length != rows * cols` | `of_array` | 同上 |
| 添字が範囲外 | `at`・`row` | 同上 |
| 形の不一致 | `add`・`mul` | 同上 |
| 要素の byte 数の溢れ | 確保する API | `trap: allocation size overflow` |
| 確保の失敗 | 確保する API | `trap: allocation failed` |

どの API も、前提条件の検査をすべて通してから、確保・要素の読み出し・callback の呼び出しをします。引数は左から右に評価します。

## 所有権と借用

`Matrix` は、バッファを所有する非 Copy 値です。要素型が `i64` のように Copy でも同じです。大きなバッファの暗黙の複製が `let` に隠れないようにするためです。複製は明示的に書きます。

```text
Matrix.of_array (Matrix.rows (ref m)) (Matrix.cols (ref m)) (deref (Matrix.as_array (ref m)))
```

- `let n = m` の後で `m` を使うと `E1012` です。`Matrix.to_array m` の後も同じです。
- `at`・`row`・`as_array` の戻り値は、元の行列を共有借用します。借用が生きている間に行列を move・置換すると `E1014` です。
- 借用を関数の外へ返そうとすると `E1013` です。

## API リファレンス

すべての関数は `std::Matrix` モジュールに属しています。

| 関数 | シグネチャ |
| --- | --- |
| `of_array` | `i64 -> i64 -> ['a] -> Matrix<'a>` |
| `init` | `i64 -> i64 -> (i64 -> i64 -> 'a) -> Matrix<'a>` |
| `rows` / `cols` | `ref Matrix<'a> -> i64` |
| `at` | `ref Matrix<'a> -> i64 -> i64 -> ref 'a` |
| `get` | `Copy<'a> => ref Matrix<'a> -> i64 -> i64 -> Maybe<'a>` |
| `row` | `ref Matrix<'a> -> i64 -> ref ['a]` |
| `as_array` | `ref Matrix<'a> -> ref ['a]` |
| `to_array` | `Matrix<'a> -> ['a]` |
| `map` | `Copy<'a> => ('a -> 'b) -> ref Matrix<'a> -> Matrix<'b>` |
| `fold` | `Copy<'a> => ('state -> 'a -> 'state) -> 'state -> ref Matrix<'a> -> 'state` |
| `transpose` | `Copy<'a> => ref Matrix<'a> -> Matrix<'a>` |
| `add` | `Numeric<'a> => ref Matrix<'a> -> ref Matrix<'a> -> Matrix<'a>` |
| `mul` | `Numeric<'a> => ref Matrix<'a> -> ref Matrix<'a> -> Matrix<'a>` |

## 計算量

| 操作 | 計算量 |
| --- | --- |
| `at`・`row`・`as_array`・`rows`・`cols` | $O(1)$、確保なし |
| `of_array`・`to_array` | $O(1)$、複製なし |
| `init`・`map`・`transpose`・`add` | $O(\text{rows} \times \text{cols})$ |
| `mul` | $O(\text{rows} \times \text{inner} \times \text{cols})$ |

`mul` は再帰しないループです。stack の深さは大きさに依存しません。性能は計測した事実だけを [ベンチマーク](../../../docs/benchmarks.md) に書いています。SIMD・並列・BLAS 並みの速度は、この API の約束ではありません。

## まとめ

- `Matrix` は行優先の連続バッファで、常に非 Copy です。
- `mul` の各出力要素は `+0` から `k` の昇順で、積と和を別々に丸めます。
- 前提条件の誤りは、確保や callback の前にトラップします。

## 関連項目

- [Array](./array.md) — 平らな配列と `Array.dot`
- [Map](./map.md) — 同じ不透明・非 Copy の標準 record
- [Math](./math.md) — 明示 FMA と順序付き集計
- [言語リファレンスの目次](../index.md)
