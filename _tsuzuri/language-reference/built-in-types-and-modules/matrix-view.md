# MatrixView

`MatrixView<'a>` は、行列のバッファを借りて、行と列の並びを変えて見る窓です。窓は要素を複製しません。転置、部分行列、1 行、1 列の窓はどれも $O(1)$ で、同じバッファをオフセットと行・列のストライドで指し直すだけです。複製が要るときは `MatrixView.to_matrix` で行優先の新しい [`Matrix`](./matrix.md) に取り出します。

書き込める窓 `MatrixView.Mut<'a>` もあります。排他スライス（`ref mut ['a..]`）を持ち、行ごとの書き込みや、その場での変換に使います。

## この記事のポイント

- 窓は借用を持つレコードです。要素 `(row, col)` は `data[offset + row * row_stride + col * col_stride]` で、ストライドは負になりません。窓の全要素は `data` の中にあり、作る関数が検査して、どの操作も保ちます。
- 読み取りの窓 `MatrixView<'a>` は Copy で、共有借用だけを持ちます。持ち主の行列を move・置換すると `E1014`、持ち主より長く使うと `E1013` です。
- 書き込める窓 `MatrixView.Mut<'a>` は非 Copy です。生きている間は元の配列を排他借用するので、読むことも別の窓を作ることもできません（`E1014`）。2 つの要素が同じ場所を指す配置にはなりません。
- `MatrixView.mul` は、連続でない窓を行優先に複製してから `Matrix.mul` を呼びます。結果のビットは、複製した行列の `Matrix.mul` と同じです。
- 内部は不透明です。フィールド参照・パターン分解・`{ view with ... }`・リテラルでの構築は `E1022` です。
- `MatrixView` という名前を書いたプログラムだけが、このモジュールと `Matrix` を読み込みます。書かないプログラムの生成コードは変わりません。ファイル名 `MatrixView.tz` は予約名で `E1011` です。

## 借りる

`MatrixView.of_matrix (ref m)` は、行列全体を窓として借ります。`MatrixView.transpose` は行と列を入れ替えた窓を返しますが、データには触りません。

```tsuzuri run=3x2%2012%203
let m = Matrix.init 2 3 (\row col -> row * 10 + col)
let view = MatrixView.of_matrix (ref m)
let flipped = MatrixView.transpose view
let corner = MatrixView.at flipped 2 1
let copy = MatrixView.to_matrix flipped
$"{MatrixView.rows flipped}x{MatrixView.cols flipped} {deref corner} {Matrix.rows (ref copy)}"
```

実行結果:

```text
3x2 12 3
```

`flipped` は 3 行 2 列で、`(2, 1)` は元の `(1, 2)`、つまり `12` です。`to_matrix` は、この見方どおりに行優先の新しい行列を作ります。

平らな配列も窓にできます。`MatrixView.of_array rows cols (ref values)` は、`values.length` が `rows * cols` に一致しないとトラップします。

## ストライドで指す

`MatrixView.strided offset rows cols row_stride col_stride (ref data)` は、配置を自分で決めた窓を作ります。引数が負のとき、`rows * cols` が `i64` に収まらないとき、窓のどこかの要素が `data` の外に出るときはトラップします。要素のない窓（`rows` か `cols` が 0）には、`offset` の上限がありません。

ストライドが 0 の軸は、同じ要素を繰り返します。行ストライドが 0 なら、全行が同じ行の窓です。

```tsuzuri run=34%2021%20false%20false%2030
let data = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]
let whole = MatrixView.of_array 3 4 (ref data)
let window = MatrixView.sub whole 1 1 2 2
let column = MatrixView.col whole 2
let repeated = MatrixView.strided 0 3 4 0 1 (ref data)
let window_sum = MatrixView.fold (\total x -> total + x) 0 window
let column_sum = MatrixView.fold (\total x -> total + x) 0 column
let repeated_sum = MatrixView.fold (\total x -> total + x) 0 repeated
$"{window_sum} {column_sum} {MatrixView.is_contiguous window} {MatrixView.is_contiguous column} {repeated_sum}"
```

実行結果:

```text
34 21 false false 30
```

`window` は `[[6, 7], [10, 11]]` で和は 34、`column` は `[3, 7, 11]` で和は 21 です。どちらも `data` の連続した一続きではないので、`is_contiguous` は `false` です。`repeated` は `[1, 2, 3, 4]` の行を 3 回見るので、和は 30 です。

派生する窓の配置は次のとおりです。`view` の配置を `(offset, row_stride, col_stride)` とします。

| 操作 | 形 | offset | row_stride | col_stride |
| --- | --- | --- | --- | --- |
| `transpose view` | `cols` 行 `rows` 列 | 同じ | `col_stride` | `row_stride` |
| `sub view r c h w` | `h` 行 `w` 列 | `offset + r * row_stride + c * col_stride` | 同じ | 同じ |
| `row view i` | `sub view i 0 1 cols` | 同上 | 同じ | 同じ |
| `col view j` | `sub view 0 j rows 1` | 同上 | 同じ | 同じ |

`sub` は、`0 <= r <= rows`、`0 <= c <= cols`、`h <= rows - r`、`w <= cols - c` でないとトラップします。`h` や `w` が 0 の空の窓も作れます。

`MatrixView.is_contiguous` は、窓の要素が `data` の一続きで、行優先に並んでいるかを返します。要素のない窓は `true` です。大きさ 1 の軸のストライドは見ません。

## 読む・集計する

| API | 返すもの |
| --- | --- |
| `MatrixView.rows view` / `cols` | 行数・列数 |
| `MatrixView.offset view` / `row_stride` / `col_stride` | 配置 |
| `MatrixView.data view` | 窓の外も含む、借りた配列全体 |
| `MatrixView.at view row col` | 要素への共有借用。行と列を別々に検査し、範囲外はトラップ |
| `MatrixView.get view row col` | 要素のコピーの `Maybe<'a>`。範囲外は `None`（`Copy<'a>` が必要） |
| `MatrixView.fold folder initial view` | 窓の行優先で左から右に畳み込む |
| `MatrixView.map transform view` | 窓の行優先で 1 回ずつ変換した新しい `Matrix` |
| `MatrixView.to_matrix view` | 窓の要素を行優先に複製した新しい `Matrix` |

窓は Copy なので、`MatrixView.fold … view` を呼んだ後も同じ窓を使えます。要素が文字列のような非 Copy 型の窓でも、`at`・`rows`・`sub`・`transpose` は使えます。値で読む `get`・`map`・`fold`・`to_matrix` と、`mul` は `Copy<'a>`・`Numeric<'a>` を要求し、満たさない要素型は `E1005` です。

## 積

`MatrixView.mul left right` は、2 つの窓の行列積を新しい `Matrix` で返します。形の条件は `Matrix.mul` と同じで、`left.cols != right.rows` はトラップです。

各窓を `MatrixView.to_matrix` で行優先に複製してから `Matrix.mul` を呼びます。複製は $O(\text{rows} \times \text{cols})$ で、積の $O(\text{rows} \times \text{inner} \times \text{cols})$ に比べて小さいので、転置した窓を渡しても内側のループは連続した行を読みます。出力の各要素は `Matrix.mul` と同じく、`+0` から `k` の昇順に、積と和を別々に丸めた値です。FMA は使いません。

```tsuzuri run=3x3%2036%20true
let a = Matrix.of_array 2 3 [1, 2, 3, 4, 5, 6]
let view = MatrixView.of_matrix (ref a)
let gram = MatrixView.mul (MatrixView.transpose view) view
let plain = Matrix.mul (ref (Matrix.transpose (ref a))) (ref a)
let same = Matrix.fold (\total x -> total * 31 + x) 0 (ref gram) == Matrix.fold (\total x -> total * 31 + x) 0 (ref plain)
$"{Matrix.rows (ref gram)}x{Matrix.cols (ref gram)} {deref (Matrix.at (ref gram) 1 2)} {same}"
```

実行結果:

```text
3x3 36 true
```

`gram` は $A^{\mathsf T} A$ で、`(1, 2)` は `2 * 3 + 5 * 6 = 36` です。転置した行列を先に作って `Matrix.mul` に渡した結果と、すべての要素が一致します。

## 書き込める窓

`MatrixView.of_array_mut rows cols (ref mut data)` は、行優先の配列を排他借用して、書き込める窓にします。`data.length` が `rows * cols` に一致しないとトラップします。窓が生きている間、`data` は読むこともできません。最後に窓を使った後は、`data` を普通に使えます。

| API | 説明 |
| --- | --- |
| `MatrixView.write (ref mut w) row col value` | 1 要素を置き換える。範囲外はトラップ |
| `MatrixView.row_mut (ref mut w) i` | 1 行を `ref mut ['a..]` で借りる。`col_stride` が 1 でない窓（転置した窓）ではトラップ |
| `MatrixView.sub_mut (ref mut w) row col rows cols` | 部分行列の書き込める窓。元の窓は、これが生きている間は使えない |
| `MatrixView.transpose_mut w` | 窓を消費して、行と列を入れ替えた窓を返す |
| `MatrixView.fill (ref mut w) value` | 全要素に `value` を書く |
| `MatrixView.copy_from (ref mut w) source` | 同じ形の読み取りの窓から複製する。形が違うとトラップ |
| `MatrixView.map_in_place transform (ref mut w)` | 全要素を `transform` の結果に置き換える |
| `MatrixView.freeze (ref w)` | 読み取りの窓として借りる。次の書き込みの前に使い終える |

```tsuzuri run=1%2C5%2C0%2C0%2C7%2C7
let mut buffer = new [i64](6, _ -> 0)
let mut grid = MatrixView.of_array_mut 2 3 (ref mut buffer)
let line = MatrixView.row_mut (ref mut grid) 0
Array.write line 1 5
let mut part = MatrixView.sub_mut (ref mut grid) 1 1 1 2
MatrixView.fill (ref mut part) 7
MatrixView.write (ref mut grid) 0 0 1
$"{buffer[0]},{buffer[1]},{buffer[2]},{buffer[3]},{buffer[4]},{buffer[5]}"
```

実行結果:

```text
1,5,0,0,7,7
```

`row_mut` が返す行は、`Array.write` で書ける排他スライスです。`part` を最後に使った後で `grid` に書き、`grid` を最後に使った後で `buffer` を読んでいます。

書き込める窓の配置は、2 つの形に限られます。`of_array_mut` と `sub_mut` が作る行優先の窓（`col_stride` が 1 で `row_stride` が列数以上）と、`transpose_mut` がそれを入れ替えた窓です。どちらも、異なる要素が同じ場所を指すことはありません。ストライドが 0 の窓を書き込める窓にすることはできません。

行列の中身を書き換えるには、`Matrix.to_array` で配列を取り出し、書き込める窓で更新して、`Matrix.of_array` で戻します。

```tsuzuri run=105
let m = Matrix.init 2 3 (\row col -> row * 3 + col)
let mut data = Matrix.to_array m
let mut window = MatrixView.of_array_mut 2 3 (ref mut data)
MatrixView.map_in_place (\x -> x + 100) (ref mut window)
let changed = Matrix.of_array 2 3 data
deref (Matrix.at (ref changed) 1 2)
```

実行結果:

```text
105
```

## 借用規則と診断

窓は、持ち主を借りたレコードです。持ち主が生きている間だけ使えます。

| 書き方 | 診断 |
| --- | --- |
| 局所の行列を借りた窓を関数から返す | `E1013`（`does not live long enough`） |
| 窓が生きている間に、持ち主の行列を move・置換・`Matrix.to_array` する | `E1014` |
| 書き込める窓が生きている間に、元の配列を読む、2 つ目の窓を作る、`row_mut` の行を残して窓に書く | `E1014` |
| 書き込める窓を move した後で使う | `E1012` |
| タスクの本体で窓を使う | `E1013`（`tasks require owned values`） |
| フィールド参照、パターン分解、`{ view with ... }`、リテラルでの構築 | `E1022` |
| 非 Copy の要素に `get`・`map`・`fold`・`to_matrix`・`mul`・`fill`・`copy_from`・`map_in_place` | `E1005` |

局所の行列の窓を返す関数は、次のように拒否されます。

```text
def leak { r } :: ref { r } Matrix<i64> -> MatrixView<i64> { r }
fn leak outer =
    let m = Matrix.init 2 2 (\i j -> i + j)
    MatrixView.of_matrix (ref m)
```

窓が生きている間に持ち主を動かす例です。

```text
let m = Matrix.init 2 2 (\i j -> i + j)
let view = MatrixView.of_matrix (ref m)
let moved = m
MatrixView.rows view
```

最後の `MatrixView.rows view` が借用を生かしているので、`let moved = m` が `E1014` です。`view` を使い終えた後なら、`m` を動かせます。

引数は左から右に評価します。前提条件の検査は、確保や callback より前に済ませます。

## トラップ

| 条件 | API | 表示 |
| --- | --- | --- |
| `values.length != rows * cols`、次元が負、`rows * cols` の溢れ | `of_array`・`of_array_mut`・`strided` | `trap: assertion failed` |
| オフセットやストライドが負、窓の要素が `data` の外、添字の計算の溢れ | `strided` | 同上 |
| 添字が範囲外 | `at`・`write`・`row`・`col`・`row_mut` | 同上 |
| 部分行列が窓の外 | `sub`・`sub_mut` | 同上 |
| `col_stride` が 1 でない窓に `row_mut` | `row_mut` | 同上 |
| 形の不一致 | `copy_from`・`mul` | 同上 |
| 要素の byte 数の溢れ、確保の失敗 | `to_matrix`・`map`・`mul` | `trap: allocation size overflow` / `trap: allocation failed` |

## API リファレンス

すべての関数は `std::MatrixView` モジュールに属しています。

| 関数 | シグネチャ |
| --- | --- |
| `of_matrix` | `ref { r } Matrix<'a> -> MatrixView<'a> { r }` |
| `of_array` | `i64 -> i64 -> ref { r } ['a] -> MatrixView<'a> { r }` |
| `strided` | `i64 -> i64 -> i64 -> i64 -> i64 -> ref { r } ['a] -> MatrixView<'a> { r }` |
| `rows` / `cols` / `offset` / `row_stride` / `col_stride` | `MatrixView<'a> { r } -> i64` |
| `data` | `MatrixView<'a> { r } -> ref { r } ['a]` |
| `at` | `MatrixView<'a> { r } -> i64 -> i64 -> ref { r } 'a` |
| `get` | `Copy<'a> => MatrixView<'a> { r } -> i64 -> i64 -> Maybe<'a>` |
| `transpose` | `MatrixView<'a> { r } -> MatrixView<'a> { r }` |
| `sub` | `MatrixView<'a> { r } -> i64 -> i64 -> i64 -> i64 -> MatrixView<'a> { r }` |
| `row` / `col` | `MatrixView<'a> { r } -> i64 -> MatrixView<'a> { r }` |
| `is_contiguous` | `MatrixView<'a> { r } -> bool` |
| `to_matrix` | `Copy<'a> => MatrixView<'a> { r } -> Matrix<'a>` |
| `fold` | `Copy<'a> => ('state -> 'a -> 'state) -> 'state -> MatrixView<'a> { r } -> 'state` |
| `map` | `Copy<'a> => ('a -> 'b) -> MatrixView<'a> { r } -> Matrix<'b>` |
| `mul` | `Numeric<'a> => MatrixView<'a> { r } -> MatrixView<'a> { s } -> Matrix<'a>` |
| `of_array_mut` | `i64 -> i64 -> ref mut { r } ['a..] -> Mut<'a> { r }` |
| `freeze` | `ref { r } Mut<'a> { r } -> MatrixView<'a> { r }` |
| `write` | `ref mut Mut<'a> { r } -> i64 -> i64 -> 'a -> unit` |
| `row_mut` | `ref mut { r } Mut<'a> { r } -> i64 -> ref mut { r } ['a..]` |
| `sub_mut` | `ref mut { r } Mut<'a> { r } -> i64 -> i64 -> i64 -> i64 -> Mut<'a> { r }` |
| `transpose_mut` | `Mut<'a> { r } -> Mut<'a> { r }` |
| `fill` | `Copy<'a> => ref mut Mut<'a> { r } -> 'a -> unit` |
| `copy_from` | `Copy<'a> => ref mut Mut<'a> { r } -> MatrixView<'a> { s } -> unit` |
| `map_in_place` | `Copy<'a> => ('a -> 'a) -> ref mut Mut<'a> { r } -> unit` |

## 計算量

| 操作 | 計算量 |
| --- | --- |
| `of_matrix`・`of_array`・`strided`・`transpose`・`sub`・`row`・`col`・`at`・`get`・`is_contiguous`・各アクセサ | $O(1)$、確保なし |
| `of_array_mut`・`sub_mut`・`transpose_mut`・`row_mut`・`freeze`・`write` | $O(1)$、確保なし |
| `to_matrix`・`fold`・`map`・`fill`・`copy_from`・`map_in_place` | $O(\text{rows} \times \text{cols})$ |
| `mul` | $O(\text{rows} \times \text{inner} \times \text{cols})$。窓の複製を含む |

`to_matrix` は、窓が連続していれば 1 回のコピーで、そうでなければ行ごとに要素を読みます。

## できないこと

- 負のストライド（逆順の窓）は作れません。ストライドは 0 以上です。
- ストライドが 0 の窓は読み取り専用です。書き込める窓は、重なりのない 2 つの形に限ります。
- 窓の積は複製を挟みます。複製しない積、順序の違う積、並列の積は `Matrix` の `mul_fma`・`mul_parallel` などです。窓の積では提供しません。
- N 次元の窓は [Tensor](./tensor.md) です。

## まとめ

- `MatrixView` は、行列のバッファを複製せずに、転置・部分行列・行・列として見る借用の窓です。
- 窓は Copy で、持ち主を動かすと `E1014`、持ち主より長く使うと `E1013` です。
- 書き込める窓は排他借用で、行ごとの書き込みやその場での変換に使います。
- 窓の積は `Matrix.mul` と同じビットを返します。

## 関連項目

- [Matrix](./matrix.md) — 窓の持ち主の行列と、行列積
- [Tensor](./tensor.md) — N 次元の窓
- [Array](./array.md) — 排他スライスと `Array.write`
- [Parallel](./parallel.md) — 排他スライスを分ける並列 API
- [言語リファレンスの目次](../index.md)
