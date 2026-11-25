# Tensor

`Tensor<'a>` は、N 次元の配列を 1 本の連続したバッファで持つ型です。形（各軸の長さ）と、行優先（C 順）に並んだ要素を持ちます。`Tensor.View<'a>` は、そのバッファや平らな配列を、軸の並べ替え・1 つの軸の固定・範囲の絞り込み・形の読み替えで見直す借用の窓です。どれも $O(1)$ で、要素を複製しません。

2 次元の特別な場合が [`Matrix`](./matrix.md) と [`MatrixView`](./matrix-view.md) です。`Matrix` と `Tensor` は、`Tensor.of_matrix`・`Tensor.to_matrix` で行き来できます。要素が Copy 型のとき、今のコンパイラはこの変換で要素を複製します（[既知の制限](#所有者の操作と-matrix-との変換)）。

## この記事のポイント

- `Tensor<'a>` は、形と行優先のバッファを所有する非 Copy 型です。`Tensor.View<'a>` は読み取り専用の借用の窓、`Tensor.Mut<'a>` は排他借用の書き込める N 次元の窓です。いずれも内部は不透明で、フィールド参照・パターン分解・リテラルでの構築は `E1022` です。
- 軸は 64 本までです。長さは 0 以上で、長さが 0 の軸があれば要素は 0 個です。軸が 0 本のテンソルは要素を 1 個持ちます。長さが 0 の軸を除いた長さの積は、`i64` に収まる必要があります。
- 窓の要素 `(i0, i1, …)` は `data[offset + i0 * strides[0] + i1 * strides[1] + …]` です。`View` は負のストライド（逆順・反転）と 0 ストライド（ブロードキャスト軸）を安全に扱え、`strided` がオーバーフローしない境界検査で全要素が `data` の中にあることを保証します。`Mut` は単射性（異なる座標が同じスロットを指さないこと）を保つ操作だけを提供します。
- 窓は借用を持つので、持ち主を move・置換すると `E1014`、持ち主より長く使うと `E1013` です。窓自身は非 Copy で、API は `ref` / `ref mut` で受け取ります。
- `Index` / `IndexMut` インスタンスを備え、`t[[i, j]]`・`t[(i, j)]`（確保なしの要素読み出し）・`ref t[[Slice.step 1 4 2, Slice.rev()]]`（ゼロコピー `View`）・`t[mask]`（ブールマスク抽出）・`m[[i, j]] = v` / `m[(i, j)] = v` / `m[mask] = v`（`Mut` への書き込み）が使えます。`Eq` と `Display`（要素数 1000 超は `...` 省略表示）も実装しています。
- `Tensor` という名前を書いたプログラムだけが、このモジュールと `Matrix`・`MatrixView`・`Slice` を読み込みます。書かないプログラムの生成コードは変わりません。ファイル名 `Tensor.tz` は予約名で `E1011` です。

## 作る

`Tensor.of_array shape values` は、行優先の平らな配列をそのまま所有します。`values.length` が形の積に一致しないとトラップします。`Tensor.init shape initializer` は、`initializer index` を行優先の順に、要素ごとにちょうど 1 回呼びます。引数は、各要素の行優先の通し番号（`i64`）です。

窓は `Tensor.view (ref t)` で借ります。`Tensor.borrow shape (ref values)` は、平らな配列を借りて、その形の窓にします。

```tsuzuri run=4x2x3%2023%2012%2012%20false
let t = Tensor.init [2, 3, 4] (\i -> i)
let v = Tensor.view (ref t)
let rotated = Tensor.permute [2, 0, 1] (ref v)
let plane = Tensor.index_axis 0 1 (ref v)
let window = Tensor.narrow 2 1 2 (ref v)
let shape = Tensor.shape (ref rotated)
$"{shape[0]}x{shape[1]}x{shape[2]} {deref (Tensor.at [3, 1, 2] (ref rotated))} {Tensor.count (ref plane)} {Tensor.count (ref window)} {Tensor.is_contiguous (ref rotated)}"
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
$"{Tensor.rank (ref single)} {deref (Tensor.at [] (ref single))} {Tensor.count (ref empty)} {Tensor.rank (ref empty)}"
```

実行結果:

```text
0 7 0 2
```

## 窓の配置

`Tensor.view` と `Tensor.borrow` が作る窓のストライドは、行優先です。`strides[i]` は、`i` より後ろの軸の長さの積です。派生する窓は、`(shape, strides, offset)` を次のように変えます。

| 操作 | shape | strides | offset |
| --- | --- | --- | --- |
| `permute axes v` | `shape[axes[i]]` を並べる | `strides[axes[i]]` を並べる | 同じ |
| `index_axis axis p v` | `axis` の軸を取り除く | `axis` の軸を取り除く | `offset + p * strides[axis]` |
| `narrow axis start length v` | `shape[axis]` を `length` にする | 同じ | `offset + start * strides[axis]` |
| `reshape_view shape v` | 新しい形 | 新しい形の行優先 | 同じ |

- `permute` の `axes` は、`0 .. rank - 1` の並べ替えでないとトラップします。
- `index_axis` は、`0 <= axis < rank`、`0 <= p < shape[axis]` でないとトラップします。
- `narrow` は、`0 <= start <= shape[axis]`、`0 <= length <= shape[axis] - start` でないとトラップします。
- `reshape_view` は、窓が連続していて、形の積が変わらないときだけ成功します。連続でない窓（`permute` した窓など）はトラップです。先に `to_tensor` で複製してください。

```tsuzuri run=3%2C1%201%2C3%203%202%201%204
let t = Tensor.init [2, 3] (\i -> i * 10)
let v = Tensor.view (ref t)
let strides = Tensor.strides (ref v)
let flipped = Tensor.permute [1, 0] (ref v)
let flipped_strides = Tensor.strides (ref flipped)
let column = Tensor.index_axis 1 2 (ref v)
let column_strides = Tensor.strides (ref column)
let tail = Tensor.narrow 1 1 2 (ref v)
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
| `Tensor.at index view` | 要素への共有借用。添字の数が軸の数と違う、または範囲外ならトラップ |
| `Tensor.get index view` | 要素のコピーの `Maybe<'a>`。添字の数が違う、または範囲外なら `None`（`Copy<'a>` が必要） |
| `Tensor.fold folder initial view` | 窓の行優先で左から右に畳み込む |
| `Tensor.map transform view` | 窓の行優先で 1 回ずつ変換した新しい `Tensor` |
| `Tensor.to_tensor view` | 窓の要素を行優先に複製した新しい `Tensor` |

添字関数の `at` と `get` は `[i64]` の配列を所有値で受け取るため、添字の配列を 1 つ確保します。一方、添字構文 `t[[i, j]]`・`v[[i, j]]` と、1〜4 軸のタプルのキー（`t[i]`・`t[(i, j)]`・`t[(i, j, k)]`・`t[(i, j, k, l)]`、および `View`・`Mut` への同じキー）は確保を一切行いません。内側のループで要素を 1 つずつ読み書きするときは、`t[(i, j)]`・`m[(i, j)] = val` の添字構文か、`Tensor.fold`・`Tensor.map` を使ってください。

### 添字構文（`t[k]`・`ref t[[Slice…]]`・`m[k] = val`）

`Tensor<'a>` と `Tensor.View<'a>` は、座標のキー（`[i64]` と 1〜4 軸の `i64` / タプル）で要素 `'a` を確保なしに読み、`[Slice.Axis]` のキーに `ref` を付けた `ref t[[...]]` でゼロコピーの `Tensor.View<'a>` を返します（`ref` を省くと `E1042`、要素読み出しやマスクに `ref` を付けると `E1003` です）。窓から要素を読むときは `(ref t[[Slice.at 2]])[3]` のように窓だけに `ref` を付けます。

```tsuzuri run=8%20120%20110
let t = Tensor.init [3, 4] (\i -> i * 10)
let rows = ref t[[Slice.range 1 3]]
let elem = t[[1, 2]] + t[(1, 2)]
let last = (ref t[[Slice.at 2]])[3]
$"{Tensor.count rows} {elem} {last}"
```

実行結果:

```text
8 120 110
```

値で読む `get`・`fold`・`map`・`to_tensor` は `Copy<'a>` を要求します。要素が文字列のような非 Copy 型の窓でも、`at`・`rank`・`shape`・`count`・`permute`・`index_axis`・`narrow` は使えます。

## 所有者の操作と Matrix との変換

| API | 説明 |
| --- | --- |
| `Tensor.as_array (ref t)` | 全要素を行優先の `ref ['a]` で借りる |
| `Tensor.to_array t` | テンソルを消費して、行優先のバッファを返す。要素が Copy 型なら要素を複製する |
| `Tensor.reshape shape t` | テンソルを消費して、形を変える。形の積が変わるとトラップ。要素が Copy 型なら要素を複製する |
| `Tensor.of_matrix m` | 行列を消費して、2 軸のテンソルにする。要素が Copy 型なら要素を複製する |
| `Tensor.to_matrix t` | テンソルを消費して、行列にする。軸が 2 本でないとトラップ。要素が Copy 型なら要素を複製する |
| `Tensor.as_matrix_view (ref v)` | 2 軸の窓を `MatrixView` として見る。軸が 2 本でないとトラップ |
| `Tensor.of_matrix_view window` | `MatrixView` を 2 軸の窓として見る |

```tsuzuri run=3x2%205%204%208%20-1
let m = Matrix.init 2 3 (\row col -> row * 3 + col)
let t = Tensor.of_matrix m
let flat = Tensor.reshape [6] t
let v = Tensor.view (ref flat)
let grid = Tensor.reshape_view [3, 2] (ref v)
let window = Tensor.as_matrix_view (ref grid)
let flipped = Tensor.permute [1, 0] (ref grid)
let copy = Tensor.map (\x -> x * 2) (ref flipped)
let doubled = Tensor.as_array (ref copy)
$"{MatrixView.rows window}x{MatrixView.cols window} {deref (MatrixView.at 2 1 window)} {doubled[1]} {doubled[2]} {Maybe.default_value (-1) (Tensor.get [3, 0] (ref grid))}"
```

実行結果:

```text
3x2 5 4 8 -1
```

`m` を `[6]` の平らなテンソルにして、`[3, 2]` の窓に読み替えました。要素が `i64` なので、`of_matrix` と `reshape` はそれぞれ要素を 1 回複製します。`window` は同じバッファを 3 行 2 列の行列として見ます。`flipped` は転置した窓で、`Tensor.map` は行優先の新しいテンソルを作ります。`grid` に範囲外の添字 `[3, 0]` を渡した `get` は `None` です。

> 既知の制限: 要素が Copy 型（`i64`・`f64` など）のとき、今のコンパイラは、テンソルを消費する `Tensor.to_array`・`reshape`・`reshape_infer`・`try_reshape`・`of_matrix`・`to_matrix`・`update` で要素を複製します。費用は要素数に比例し、`update` は 2 回複製します。要素が Copy でない型（文字列など）では複製しません。要素が Copy 型の所有するテンソルを、複製せずに書き換える方法は今はありません。`let mut` の配列から `Tensor.borrow_mut` で作った書き込める窓は、複製せずに書き込みます。[NC20](../../../_features/NC20-compound-assignment-and-mutable-index-views.md) の後は、`ref mut t[…]` と `+=` でも書けるようになる予定です。修正は [PM10](../../../_perfs/PM10-std-consume-copies.md) です。

## 借用規則と診断

窓は、持ち主を借りたレコードです。持ち主が生きている間だけ使えます。窓の API は `ref` で窓を受け取るので、窓から作った窓を式の途中で借りるときは、先に `let` で束縛します。

| 書き方 | 診断 |
| --- | --- |
| 局所のテンソルを借りた窓を関数から返す | `E1013` |
| 窓が生きている間に、持ち主のテンソルを move・置換する | `E1014` |
| 窓を `let w = v` で move した後で `v` を使う | `E1012` |
| 一時値を `ref` で借りる（`Tensor.rank (ref (Tensor.permute …))`） | `E1013`（`borrow requires a local place`） |
| 一時値の土台から窓を取る（`ref (mk 100)[[Slice.range 1 3]]`） | `E1013`（`borrow requires a local place`） |
| 窓を返す添字に `ref` を付けない（`t[[Slice.range 1 3]]`） | `E1042`（`index_view_requires_ref`） |
| 窓が生きている間に、窓の土台の `View`・`Mut` を move・置換・排他借用する | `E1014` |
| フィールド参照、パターン分解、リテラルでの構築 | `E1022` |
| 非 Copy の要素に `get`・`fold`・`map`・`to_tensor` | `E1005` |

窓からさらに切り出す窓の連鎖（`ref t[[Slice.range 1 3]][[Slice.at 1]]`）と、一時値からの要素の読み出し（`(mk 100)[2]`・`(mk 100)[[2]]`）はそのまま書けます。

## トラップ

| 条件 | API | 表示 |
| --- | --- | --- |
| 軸が 64 本を超える、長さが負、長さが 0 の軸を除いた積の溢れ | `of_array`・`init`・`reshape`・`reshape_view`・`borrow`・`strided` | `trap: assertion failed` |
| `values.length` が形の積と違う | `of_array`・`borrow`・`borrow_mut` | 同上 |
| 形の積が変わる、または窓が連続でない | `reshape`・`reshape_view`・`as_slice_mut`・`ravel_view` | 同上 |
| 添字の数が軸の数と違う、または範囲外 | `at`・`write` | 同上 |
| `axes` が並べ替えでない、軸や位置が範囲外 | `permute`・`index_axis`・`narrow`・`normalize_axis` | 同上 |
| 軸が 2 本でない、または負のストライドを `MatrixView` へ渡した | `to_matrix`・`as_matrix_view` | 同上 |
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
| `at` | `[i64] -> ref View<'a> { r } -> ref { r } 'a` |
| `get` | `Copy<'a> => [i64] -> ref View<'a> -> Maybe<'a>` |
| `permute` | `[i64] -> ref View<'a> { r } -> View<'a> { r }` |
| `index_axis` | `i64 -> i64 -> ref View<'a> { r } -> View<'a> { r }` |
| `narrow` | `i64 -> i64 -> i64 -> ref View<'a> { r } -> View<'a> { r }` |
| `is_contiguous` | `ref View<'a> -> bool` |
| `reshape_view` | `[i64] -> ref View<'a> { r } -> View<'a> { r }` |
| `fold` | `Copy<'a> => ('state -> 'a -> 'state) -> 'state -> ref View<'a> -> 'state` |
| `to_tensor` | `Copy<'a> => ref View<'a> -> Tensor<'a>` |
| `map` | `Copy<'a> => ('a -> 'b) -> ref View<'a> -> Tensor<'b>` |
| `as_matrix_view` | `ref View<'a> { r } -> MatrixView<'a> { r }`（負のストライドはトラップ） |
| `of_matrix_view` | `MatrixView<'a> { r } -> View<'a> { r }` |
| `strided` | `i64 -> [i64] -> [i64] -> ref { r } ['a] -> View<'a> { r }`（負・0 ストライド対応） |
| `fortran_strides` | `ref [i64] -> [i64]` |
| `is_c_contiguous` / `is_f_contiguous` | `ref View<'a> -> bool` |
| `as_contiguous` / `clone_view` | `Copy<'a> => ref View<'a> -> Tensor<'a>` |
| `as_fortran` | `Copy<'a> => ref View<'a> -> ['a]` |
| `clone` | `Copy<'a> => ref Tensor<'a> -> Tensor<'a>` |
| `astype` | `(Copy<'a>, Copy<'b>) => ('a -> 'b) -> ref View<'a> -> Tensor<'b>` |
| `format_view` | `Display<'a> => ref View<'a> -> string` |
| `borrow_mut` | `[i64] -> ref mut { r } ['a..] -> Mut<'a> { r }` |
| `update` | `(ref mut Mut<'a> -> unit) -> Tensor<'a> -> Tensor<'a>` |
| `freeze` | `ref { r } Mut<'a> { r } -> View<'a> { r }` |
| `is_contiguous_mut` | `ref Mut<'a> { r } -> bool` |
| `as_slice_mut` | `ref mut { r } Mut<'a> { r } -> ref mut { r } ['a..]` |
| `write` / `fill` / `copy_from` / `map_in_place` | `Mut<'a> { r }` への書き込み・一括代入・複写・インプレース変換 |
| `permute_mut` / `index_axis_mut` / `narrow_mut` / `sub_mut` / `step_mut` / `flip_mut` | 単射性を保つ `Mut<'a> { r }` のゼロコピー変形 |
| `broadcast_shape` | `ref [i64] -> ref [i64] -> Result<[i64], Error>` |
| `broadcast_to` / `broadcast_pair` / `broadcast_arrays` | NumPy 互換の右揃えゼロコピー（stride 0）ブロードキャスト |
| `zeros` / `ones` / `full` / `zeros_like` / `ones_like` / `full_like` / `scalar` | 初期値・形状引継ぎ・スカラー生成 |
| `arange_i64` / `arange_f64` / `arange_f32` | 等差数列生成 |
| `linspace` / `linspace_open` / `logspace` / `geomspace` | 線形・対数・等比グリッド生成 |
| `eye` / `identity` / `diag` / `diagonal` / `diag_flat` / `tri` / `tril` / `triu` | 単位・対角・三角行列生成と抽出 |
| `meshgrid` / `meshgrid_ij` | 座標グリッド（`xy`・`ij` 順）生成 |
| `normalize_axis` | `i64 -> i64 -> i64`（負の軸番号の正規化） |
| `reshape_infer` / `try_reshape` | `-1` による 1 軸自動推論付きの変形 |
| `slice` | `[Slice.Axis] -> ref View<'a> { r } -> View<'a> { r }` |
| `transpose` / `swapaxes` / `moveaxis` / `squeeze` / `squeeze_axis` / `expand_dims` | ゼロコピー軸並べ替え・圧縮・拡張 |
| `atleast_1d` / `atleast_2d` / `atleast_3d` | 最小ランク保証のゼロコピービュー |
| `flip` / `flip_axis` / `fliplr` / `flipud` / `rot90` | ゼロコピー反転・90 度回転 |
| `ravel_view` / `flatten` | 1 次元化（連続ビュー／所有コピー） |
| `roll` / `tile` / `repeat` / `pad` | 循環シフト・タイル・反復・パディング（`PadMode`: `Constant`/`Edge`/`Reflect`/`Symmetric`/`Wrap`） |
| `concatenate` / `stack` / `vstack` / `hstack` / `dstack` / `column_stack` | 軸指定・縦横奥行・列結合 |
| `split` / `split_at` / `array_split` / `hsplit` / `vsplit` / `dsplit` | ゼロコピー分割（`[View<'a>]` を返す） |
| `mask` / `compress` / `nonzero` / `argwhere` / `flatnonzero` / `put_mask` | ブールマスク抽出・圧縮・非ゼロ座標・マスク代入 |
| `where_select` / `where_scalar` / `clip` | 条件選択（3 入力ブロードキャスト対応）・クランプ |
| `take` / `take_along_axis` / `put_along_axis` | 整数添字による収集（gather）・散布（scatter） |

```tsuzuri run=[1,%2099,%2099,%204]%20[2,%203]
let g = Tensor.of_array [2, 2] [1, 2, 3, 4]
let bmask = Tensor.of_array [2, 2] [false, true, true, false]
let picked = g[bmask]
let updated = Tensor.update (\m -> m[bmask] = 99) g
$"{Tensor.to_array updated} {Tensor.to_array picked}"
```

実行結果:

```text
[1, 99, 99, 4] [2, 3]
```

### 引数の順序

標準ライブラリ全体の規則は、[標準ライブラリの引数の順序](./argument-order.md)にまとめてあります。

読む窓や、形を変えるテンソルは、必ず最後の引数です。`ref v |> Tensor.permute [1, 0]` のようにパイプで渡せ、`Tensor.index_axis 0 1 >> Tensor.count` のように一部だけ渡した関数を合成できます。添字・軸・位置・個数・形などのほかの引数は、窓の前に置きます（`at index v`、`index_axis axis position v`、`narrow axis start length v`、`reshape_view shape v`）。`fold folder initial v` と `map transform v` も窓が最後で、`reshape shape t` もテンソルが最後です。

```tsuzuri run=23%20-1%2012
let t = Tensor.init [2, 3, 4] (\i -> i)
let v = Tensor.view (ref t)
let rotated = ref v |> Tensor.permute [2, 0, 1]
let corner = ref rotated |> Tensor.at [3, 1, 2]
let missing = ref rotated |> Tensor.get [4, 0, 0] |> Maybe.default_value (-1)
let plane_count = ref v |> (Tensor.index_axis 0 1 >> Tensor.count)
$"{deref corner} {missing} {plane_count}"
```

実行結果:

```text
23 -1 12
```

`>>` で合成した関数は、前の関数が返した窓を、次の関数が借りて受け取ります。借りた一時の窓の中を指す参照を返す関数（`at`、`shape`、`strides`）につなぐと、参照が一時の窓より長生きしてしまうので `E1013` です。窓を `let` に束縛してから、上の例のように `ref rotated |> Tensor.at [3, 1, 2]` と書きます。

## 計算量

| 操作 | 計算量 |
| --- | --- |
| `as_array` | $O(1)$、要素の複製なし |
| `of_array` | $O(\text{rank})$（形の検査だけ）、要素の複製なし |
| `to_array`・`reshape`・`reshape_infer`・`try_reshape`・`of_matrix`・`to_matrix`・`update` | 要素が Copy 型なら $O(\text{count})$（今のコンパイラは要素を複製する。`update` は 2 回）。要素が Copy でない型なら要素を複製せず、形の検査の $O(\text{rank})$ だけ。`update` は、どちらの場合もこれに `writer` の費用が加わる |
| `view`・`borrow`・`permute`・`index_axis`・`narrow`・`reshape_view`・`is_contiguous`・`count`・`at`・`get` | $O(\text{rank})$。窓を返す API は、形とストライドの配列を新しく確保する |
| 添字 `t[[…]]`・`t[(i, j)]`・`m[(i, j)] = v`（要素の読み書き） | $O(\text{rank})$（1〜4 軸タプルは $O(1)$）、確保なし |
| 添字 `ref t[[Slice…]]`・`ref v[[Slice…]]`（窓） | $O(\text{rank})$、結果の形とストライドの 2 配列のみ確保 |
| `init`・`fold`・`map`・`to_tensor` | $O(\text{count})$ |

軸は 64 本までなので、$O(\text{rank})$ は定数です。要素の読み出しは、連続でない窓でも、軸ごとのループで行います。要素のない窓（長さが 0 の軸を持つ窓）では、`fold` は他の軸がどれだけ長くても（`[4611686018427387904, 0]` のような形も作れます）何も走査せずに初期値を返します。最適化水準による違いもありません。

## できないこと

- 要素ごと算術 ufunc、軸指定の統計・ソート、`einsum`・線形代数・FFT の演算はまだ未実装です（[NC06〜NC09](../../../_features/README.md) で追加予定）。
- 窓の並列・SIMD・GPU のカーネルは未実装です。平らなバッファは `Tensor.as_array` で取り出せるので、[Parallel](./parallel.md) や [Gpu](./gpu.md) の API に渡せます。
- 要素が Copy 型の所有するテンソルから、複製せずにバッファを取り出すことと、複製せずに書き換えることは今はできません（[所有者の操作と Matrix との変換](#所有者の操作と-matrix-との変換)の既知の制限）。
- 65 本以上の軸は作れません。`as_matrix_view` は負のストライドを持つ 2D 窓ではトラップするため、先に `to_tensor` 等で連続化してください。

## まとめ

- `Tensor` は形と行優先のバッファを所有し、`Tensor.View`（読み取り専用・負/0 ストライド対応）と `Tensor.Mut`（単射な書き込み専用）が同じバッファを見ます。
- `Matrix` とは `of_matrix`・`to_matrix` で行き来できます（要素が Copy 型なら、今は要素を複製します）。[`Slice`](./slice.md) やブールマスクを使った `t[k]` / `m[k] = v` の汎用添字が使えます。
- 窓は持ち主を動かすと `E1014`、持ち主より長く使うと `E1013` です。

## 関連項目

- [Slice](./slice.md) — スライス記述子 `Slice.Axis`
- [Matrix](./matrix.md) — 2 次元の持ち主と、行列積
- [MatrixView](./matrix-view.md) — 2 次元の窓と、書き込める窓
- [Array](./array.md) — 平らな配列とスライス
- [言語リファレンスの目次](../index.md)
