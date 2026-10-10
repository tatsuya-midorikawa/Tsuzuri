# Tensor

`Tensor<'a>` は、N 次元の配列を 1 本の連続したバッファで持つ型です。形（各軸の長さ）と、行優先（C 順）に並んだ要素を持ちます。`Tensor.View<'a>` は、そのバッファや平らな配列を、軸の並べ替え・1 つの軸の固定・範囲の絞り込み・形の読み替えで見直す借用の窓です。どれも $O(1)$ で、要素を複製しません。

2 次元の特別な場合が [`Matrix`](./matrix.md) と [`MatrixView`](./matrix-view.md) です。`Matrix` と `Tensor` は、複製なしで行き来できます。

## この記事のポイント

- `Tensor<'a>` は、形と行優先のバッファを所有する非 Copy 型です。`Tensor.View<'a>` は借用を持つ窓で、オフセットと軸ごとのストライドで要素を指します。どちらも内部は不透明で、フィールド参照・パターン分解・リテラルでの構築は `E1022` です。
- 軸は 16 本までです。長さは 0 以上で、長さが 0 の軸があれば要素は 0 個です。軸が 0 本のテンソルは要素を 1 個持ちます。長さが 0 の軸を除いた長さの積は、`i64` に収まる必要があります。
- 窓の要素 `(i0, i1, …)` は `data[offset + i0 * strides[0] + i1 * strides[1] + …]` です。ストライドは負になりません。窓の全要素は `data` の中にあり、作る関数が検査して、どの操作も保ちます。
- 窓は借用を持つので、持ち主を move・置換すると `E1014`、持ち主より長く使うと `E1013` です。窓自身は非 Copy で、API は `ref` で受け取ります。
- `Tensor` という名前を書いたプログラムだけが、このモジュールと `Matrix`・`MatrixView` を読み込みます。書かないプログラムの生成コードは変わりません。ファイル名 `Tensor.tz` は予約名で `E1011` です。

## 作る

`Tensor.of_array shape values` は、行優先の平らな配列をそのまま所有します。`values.length` が形の積に一致しないとトラップします。`Tensor.init shape initializer` は、`initializer index` を行優先の順に、要素ごとにちょうど 1 回呼びます。引数は、各要素の行優先の通し番号（`i64`）です。

窓は `Tensor.view (ref t)` で借ります。`Tensor.borrow shape (ref values)` は、平らな配列を借りて、その形の窓にします。

```tsuzuri run=4x2x3%2023%2012%2012%20false
let t = Tensor.init [2, 3, 4] (\i -> i)
let v = Tensor.view (ref t)
let rotated = Tensor.permute (ref v) [2, 0, 1]
let plane = Tensor.index_axis (ref v) 0 1
let window = Tensor.narrow (ref v) 2 1 2
let shape = Tensor.shape (ref rotated)
$"{shape[0]}x{shape[1]}x{shape[2]} {deref (Tensor.at (ref rotated) [3, 1, 2])} {Tensor.count (ref plane)} {Tensor.count (ref window)} {Tensor.is_contiguous (ref rotated)}"
```

実行結果:

```text
4x2x3 23 12 12 false
```

`t` は 2×3×4 で、要素 `(a, b, c)` は `a * 12 + b * 4 + c` です。`rotated` は軸を `[2, 0, 1]` に並べ替えた 4×2×3 の窓で、`(3, 1, 2)` は元の `(1, 2, 3)` なので `23` です。`plane` は軸 0 を 1 に固定した 3×4 の窓で、`window` は軸 2 を位置 1 から 2 個に絞った 2×3×2 の窓です。`rotated` の要素は元のバッファの上で飛び飛びなので、連続ではありません。

軸が 0 本のテンソルと、要素のないテンソルも作れます。

```tsuzuri run=0%207%200%202
let scalar = Tensor.of_array [] [7]
let single = Tensor.view (ref scalar)
let nothing = Tensor.init [0, 3] (\i -> i)
let empty = Tensor.view (ref nothing)
$"{Tensor.rank (ref single)} {deref (Tensor.at (ref single) [])} {Tensor.count (ref empty)} {Tensor.rank (ref empty)}"
```

実行結果:

```text
0 7 0 2
```

## 窓の配置

`Tensor.view` と `Tensor.borrow` が作る窓のストライドは、行優先です。`strides[i]` は、`i` より後ろの軸の長さの積です。派生する窓は、`(shape, strides, offset)` を次のように変えます。

| 操作 | shape | strides | offset |
| --- | --- | --- | --- |
| `permute v axes` | `shape[axes[i]]` を並べる | `strides[axes[i]]` を並べる | 同じ |
| `index_axis v axis p` | `axis` の軸を取り除く | `axis` の軸を取り除く | `offset + p * strides[axis]` |
| `narrow v axis start length` | `shape[axis]` を `length` にする | 同じ | `offset + start * strides[axis]` |
| `reshape_view v shape` | 新しい形 | 新しい形の行優先 | 同じ |

- `permute` の `axes` は、`0 .. rank - 1` の並べ替えでないとトラップします。
- `index_axis` は、`0 <= axis < rank`、`0 <= p < shape[axis]` でないとトラップします。
- `narrow` は、`0 <= start <= shape[axis]`、`0 <= length <= shape[axis] - start` でないとトラップします。
- `reshape_view` は、窓が連続していて、形の積が変わらないときだけ成功します。連続でない窓（`permute` した窓など）はトラップです。先に `to_tensor` で複製してください。

```tsuzuri run=3%2C1%201%2C3%203%202%201%204
let t = Tensor.init [2, 3] (\i -> i * 10)
let v = Tensor.view (ref t)
let strides = Tensor.strides (ref v)
let flipped = Tensor.permute (ref v) [1, 0]
let flipped_strides = Tensor.strides (ref flipped)
let column = Tensor.index_axis (ref v) 1 2
let column_strides = Tensor.strides (ref column)
let tail = Tensor.narrow (ref v) 1 1 2
$"{strides[0]},{strides[1]} {flipped_strides[0]},{flipped_strides[1]} {column_strides[0]} {Tensor.offset (ref column)} {Tensor.offset (ref tail)} {Tensor.count (ref tail)}"
```

実行結果:

```text
3,1 1,3 3 2 1 4
```

2×3 のテンソルのストライドは `[3, 1]` です。軸を入れ替えると `[1, 3]`、軸 1 を位置 2 に固定すると `[3]` でオフセットが 2、軸 1 を位置 1 から 2 個に絞るとオフセットが 1 で要素は 4 個です。

`Tensor.is_contiguous` は、窓の要素が `data` の一続きで、行優先に並んでいるかを返します。要素のない窓は `true` です。長さが 1 の軸のストライドは見ません。

## 読む・集計する

| API | 返すもの |
| --- | --- |
| `Tensor.rank view` / `Tensor.shape view` / `Tensor.count view` | 軸の数、各軸の長さ（`ref [i64]`）、要素数 |
| `Tensor.strides view` / `Tensor.offset view` / `Tensor.data view` | 配置と、借りた配列全体 |
| `Tensor.at view index` | 要素への共有借用。添字の数が軸の数と違う、または範囲外ならトラップ |
| `Tensor.get view index` | 要素のコピーの `Maybe<'a>`。添字の数が違う、または範囲外なら `None`（`Copy<'a>` が必要） |
| `Tensor.fold folder initial view` | 窓の行優先で左から右に畳み込む |
| `Tensor.map transform view` | 窓の行優先で 1 回ずつ変換した新しい `Tensor` |
| `Tensor.to_tensor view` | 窓の要素を行優先に複製した新しい `Tensor` |

添字は `[i64]` の配列です。`at` と `get` は、添字の配列を 1 つ確保します。要素ごとに呼ぶ内側のループには、`Tensor.fold`・`Tensor.map` を使うか、`offset`・`strides`・`data` から添字を自分で計算してください。

値で読む `get`・`fold`・`map`・`to_tensor` は `Copy<'a>` を要求します。要素が文字列のような非 Copy 型の窓でも、`at`・`rank`・`shape`・`count`・`permute`・`index_axis`・`narrow` は使えます。

## 所有者の操作と Matrix との変換

| API | 説明 |
| --- | --- |
| `Tensor.as_array (ref t)` | 全要素を行優先の `ref ['a]` で借りる |
| `Tensor.to_array t` | テンソルを消費して、バッファを複製せずに返す |
| `Tensor.reshape shape t` | テンソルを消費して、形だけを変える。形の積が変わるとトラップ |
| `Tensor.of_matrix m` | 行列を消費して、2 軸のテンソルにする。複製しない |
| `Tensor.to_matrix t` | テンソルを消費して、行列にする。複製しない。軸が 2 本でないとトラップ |
| `Tensor.as_matrix_view (ref v)` | 2 軸の窓を `MatrixView` として見る。軸が 2 本でないとトラップ |
| `Tensor.of_matrix_view window` | `MatrixView` を 2 軸の窓として見る |

```tsuzuri run=3x2%205%204%208%20-1
let m = Matrix.init 2 3 (\row col -> row * 3 + col)
let t = Tensor.of_matrix m
let flat = Tensor.reshape [6] t
let v = Tensor.view (ref flat)
let grid = Tensor.reshape_view (ref v) [3, 2]
let window = Tensor.as_matrix_view (ref grid)
let flipped = Tensor.permute (ref grid) [1, 0]
let copy = Tensor.map (\x -> x * 2) (ref flipped)
let doubled = Tensor.as_array (ref copy)
$"{MatrixView.rows window}x{MatrixView.cols window} {deref (MatrixView.at window 2 1)} {doubled[1]} {doubled[2]} {Maybe.default_value (-1) (Tensor.get (ref grid) [3, 0])}"
```

実行結果:

```text
3x2 5 4 8 -1
```

`m` を複製せずに `[6]` の平らなテンソルにして、`[3, 2]` の窓に読み替えました。`window` は同じバッファを 3 行 2 列の行列として見ます。`flipped` は転置した窓で、`Tensor.map` は行優先の新しいテンソルを作ります。`grid` に範囲外の添字 `[3, 0]` を渡した `get` は `None` です。

## 借用規則と診断

窓は、持ち主を借りたレコードです。持ち主が生きている間だけ使えます。窓の API は `ref` で窓を受け取るので、窓から作った窓を式の途中で借りるときは、先に `let` で束縛します。

| 書き方 | 診断 |
| --- | --- |
| 局所のテンソルを借りた窓を関数から返す | `E1013` |
| 窓が生きている間に、持ち主のテンソルを move・置換する | `E1014` |
| 窓を `let w = v` で move した後で `v` を使う | `E1012` |
| 一時値を `ref` で借りる（`Tensor.rank (ref (Tensor.permute …))`） | `E1013`（`borrow requires a local place`） |
| フィールド参照、パターン分解、リテラルでの構築 | `E1022` |
| 非 Copy の要素に `get`・`fold`・`map`・`to_tensor` | `E1005` |

## トラップ

| 条件 | API | 表示 |
| --- | --- | --- |
| 軸が 16 本を超える、長さが負、長さが 0 の軸を除いた積の溢れ | `of_array`・`init`・`reshape`・`reshape_view`・`borrow` | `trap: assertion failed` |
| `values.length` が形の積と違う | `of_array`・`borrow` | 同上 |
| 形の積が変わる、または窓が連続でない | `reshape`・`reshape_view` | 同上 |
| 添字の数が軸の数と違う、または範囲外 | `at` | 同上 |
| `axes` が並べ替えでない、軸や位置が範囲外 | `permute`・`index_axis`・`narrow` | 同上 |
| 軸が 2 本でない | `to_matrix`・`as_matrix_view` | 同上 |
| 要素の byte 数の溢れ、確保の失敗 | 確保する API | `trap: allocation size overflow` / `trap: allocation failed` |

どの API も、前提条件の検査をすべて通してから、確保・要素の読み出し・callback の呼び出しをします。

## API リファレンス

すべての関数は `std::Tensor` モジュールに属しています。窓の型は `Tensor.View<'a>` です。

| 関数 | シグネチャ |
| --- | --- |
| `of_array` | `[i64] -> ['a] -> Tensor<'a>` |
| `init` | `[i64] -> (i64 -> 'a) -> Tensor<'a>` |
| `of_matrix` | `Matrix<'a> -> Tensor<'a>` |
| `to_matrix` | `Tensor<'a> -> Matrix<'a>` |
| `as_array` | `ref Tensor<'a> -> ref ['a]` |
| `to_array` | `Tensor<'a> -> ['a]` |
| `reshape` | `[i64] -> Tensor<'a> -> Tensor<'a>` |
| `view` | `ref { r } Tensor<'a> -> View<'a> { r }` |
| `borrow` | `[i64] -> ref { r } ['a] -> View<'a> { r }` |
| `rank` / `count` / `offset` | `ref View<'a> -> i64` |
| `shape` / `strides` | `ref View<'a> -> ref [i64]` |
| `data` | `ref View<'a> { r } -> ref { r } ['a]` |
| `at` | `ref View<'a> { r } -> [i64] -> ref { r } 'a` |
| `get` | `Copy<'a> => ref View<'a> -> [i64] -> Maybe<'a>` |
| `permute` | `ref View<'a> { r } -> [i64] -> View<'a> { r }` |
| `index_axis` | `ref View<'a> { r } -> i64 -> i64 -> View<'a> { r }` |
| `narrow` | `ref View<'a> { r } -> i64 -> i64 -> i64 -> View<'a> { r }` |
| `is_contiguous` | `ref View<'a> -> bool` |
| `reshape_view` | `ref View<'a> { r } -> [i64] -> View<'a> { r }` |
| `fold` | `Copy<'a> => ('state -> 'a -> 'state) -> 'state -> ref View<'a> -> 'state` |
| `to_tensor` | `Copy<'a> => ref View<'a> -> Tensor<'a>` |
| `map` | `Copy<'a> => ('a -> 'b) -> ref View<'a> -> Tensor<'b>` |
| `as_matrix_view` | `ref View<'a> { r } -> MatrixView<'a> { r }` |
| `of_matrix_view` | `MatrixView<'a> { r } -> View<'a> { r }` |

## 計算量

| 操作 | 計算量 |
| --- | --- |
| `as_array`・`to_array`・`of_matrix` | $O(1)$、要素の複製なし |
| `of_array`・`reshape`・`to_matrix` | $O(\text{rank})$（形の検査だけ）、要素の複製なし |
| `view`・`borrow`・`permute`・`index_axis`・`narrow`・`reshape_view`・`is_contiguous`・`count`・`at`・`get` | $O(\text{rank})$。窓を返す API は、形とストライドの配列を新しく確保する |
| `init`・`fold`・`map`・`to_tensor` | $O(\text{count})$ |

軸は 16 本までなので、$O(\text{rank})$ は定数です。要素の読み出しは、連続でない窓でも、軸ごとのループで行います。要素のない窓（長さが 0 の軸を持つ窓）では、`fold` は他の軸がどれだけ長くても（`[4611686018427387904, 0]` のような形も作れます）何も走査せずに初期値を返します。最適化水準による違いもありません。

## できないこと

- 書き込める窓は、`Matrix` の [`MatrixView.Mut`](./matrix-view.md) だけです。N 次元の書き込める窓はありません。
- ブロードキャスト、軸に沿った集計、`einsum` のような演算はありません。`fold` と `map` を使います。
- 窓の並列・SIMD・GPU のカーネルはありません。平らなバッファは `Tensor.as_array` で取り出せるので、[Parallel](./parallel.md) や [Gpu](./gpu.md) の API に渡せます。
- 負のストライド（逆順の窓）と、17 本以上の軸は作れません。

## まとめ

- `Tensor` は形と行優先のバッファを所有し、`Tensor.View` は同じバッファを軸の並べ替え・固定・絞り込み・読み替えで見る借用の窓です。
- `Matrix` とは複製なしで行き来でき、2 軸の窓は `MatrixView` として見られます。
- 窓は持ち主を動かすと `E1014`、持ち主より長く使うと `E1013` です。

## 関連項目

- [Matrix](./matrix.md) — 2 次元の持ち主と、行列積
- [MatrixView](./matrix-view.md) — 2 次元の窓と、書き込める窓
- [Array](./array.md) — 平らな配列とスライス
- [言語リファレンスの目次](../index.md)
