# Tensor

`Tensor<'a>` は、N 次元の配列を 1 本の連続したバッファで持つ型です。形（各軸の長さ）と、行優先（C 順）に並んだ要素を持ちます。`Tensor.View<'a>` は、そのバッファや平らな配列を、軸の並べ替え・1 つの軸の固定・範囲の絞り込み・形の読み替えで見直す借用の窓です。どれも $O(1)$ で、要素を複製しません。

2 次元の特別な場合が [`Matrix`](./matrix.md) と [`MatrixView`](./matrix-view.md) です。`Matrix` と `Tensor` は、`Tensor.of_matrix`・`Tensor.to_matrix` で要素を複製せずに行き来できます。

## この記事のポイント

- `Tensor<'a>` は、形と行優先のバッファを所有する非 Copy 型です。`Tensor.View<'a>` は読み取り専用の借用の窓、`Tensor.Mut<'a>` は排他借用の書き込める N 次元の窓です。いずれも内部は不透明で、フィールド参照・パターン分解・リテラルでの構築は `E1022` です。
- 軸は 64 本までです。長さは 0 以上で、長さが 0 の軸があれば要素は 0 個です。軸が 0 本のテンソルは要素を 1 個持ちます。長さが 0 の軸を除いた長さの積は、`i64` に収まる必要があります。
- 窓の要素 `(i0, i1, …)` は `data[offset + i0 * strides[0] + i1 * strides[1] + …]` です。`View` は負のストライド（逆順・反転）と 0 ストライド（ブロードキャスト軸）を安全に扱え、`strided` がオーバーフローしない境界検査で全要素が `data` の中にあることを保証します。`Mut` は単射性（異なる座標が同じスロットを指さないこと）を保つ操作だけを提供します。
- 窓は借用を持つので、持ち主を move・置換すると `E1014`、持ち主より長く使うと `E1013` です。窓自身は非 Copy で、API は `ref` / `ref mut` で受け取ります。
- `Index` / `IndexMut` インスタンスを備え、`t[i, j]`・`t[^0, 0]`・`t[[i, j]]`（確保なしの要素読み出し）、`ref t[i, a..b, ..]`・`ref t[.. -1 .., ..]`・`ref t[[Slice.step 1 4 2, Slice.rev()]]`（要素をコピーしない `View`、形とストライドの 2 配列を確保）、`t[mask]`（ブールマスク抽出）、`m[i, j] = v` / `m[^0, 0] = v` / `m[mask] = v`（`Mut` への書き込み）が使えます。`Eq` と `Display`（要素数 1000 超は `...` 省略表示）も実装しています。
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
| `Tensor.to_array t` | テンソルを消費して、行優先のバッファを複製なしに返す |
| `Tensor.reshape shape t` | テンソルを消費して、複製なしに形を変える。形の積が変わるとトラップ |
| `Tensor.of_matrix m` | 行列を消費して、複製なしに 2 軸のテンソルにする |
| `Tensor.to_matrix t` | テンソルを消費して、複製なしに行列にする。軸が 2 本でないとトラップ |
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

`m` を `[6]` の平らなテンソルにして、`[3, 2]` の窓に読み替えました。`of_matrix` と `reshape` はどちらも要素を複製しません。`window` は同じバッファを 3 行 2 列の行列として見ます。`flipped` は転置した窓で、`Tensor.map` は行優先の新しいテンソルを作ります。`grid` に範囲外の添字 `[3, 0]` を渡した `get` は `None` です。

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

### ブロードキャスト要素ごと演算（ufunc）・比較・インプレース出力・演算子

2 入力・3 入力の要素ごと演算はすべて NumPy 互換の右揃えブロードキャストに対応します。同形状かつ C 連続の窓では 1D スライス直走査の高速経路を取り、非連続・負ストライド・0 ストライドの窓では汎用ストライド経路へ自動分岐します。関数 API は形状非互換を `Result.Error` で返し、同じ型どうしの演算子はトラップします。二項テンソル関数の引数順は常に `left -> right`（`Tensor.sub left right` は `left - right`）であり、スカラーとの関数と `_into`／`_in_place` では対象（`View` または出力 `Mut`）が最後です。

| 分類 | API | 概要 |
| --- | --- | --- |
| 反復エンジン | `map_into`, `map2`, `map2_into`, `map3`, `map3_into`, `map2_in_place` | 1〜3 入力のブロードキャスト要素変換・ゼロ確保出力・累積更新 |
| 二項算術・剰余 | `add`, `sub`, `mul`, `div`, `rem`, `pow`, `floor_divide`, `floor_mod`, `divmod`, `floor_divide_float`, `floor_mod_float`, `divmod_float`, `fmod`, `ieee_remainder` | 四則・べき乗・床除算/床剰余（整数と浮動小数点）・C99 `fmod`・IEEE 754 最近接偶数剰余 |
| 極値・二項数学・三項 | `maximum`, `minimum`, `fmax`, `fmin`, `atan2`, `hypot`, `copysign`, `logaddexp`, `logaddexp2`, `fma` | `NaN` 伝播／無視の最大最小・二変数数学関数・明示 FMA |
| ビット・論理 | `bit_and`, `bit_or`, `bit_xor`, `bit_not`, `shl`, `shr`, `logical_and`, `logical_or`, `logical_xor`, `logical_not` | 整数テンソルのビット演算・`bool` テンソルの論理演算 |
| 両側スカラー演算 | `add_scalar`, `radd_scalar`, `sub_scalar`, `rsub_scalar`, `mul_scalar`, `scale`, `div_scalar`, `rdiv_scalar`, `rem_scalar`, `rrem_scalar`, `pow_scalar`, `rpow_scalar` | `x op s` と `s op x`（非可換の要素型でも左右の順序を保持） |
| ゼロ確保出力（`_into`） | `add_into`, `sub_into`, `mul_into`, `div_into`, `scale_into`, `mul_add_into`, `fma_into`, `neg_into`, `abs_into`, `sqrt_into`, `exp_into`, `log_into` | `Tensor.borrow_mut` 等で得た `ref mut Mut` へ追加ヒープ確保 0 回で書き込み |
| 累積インプレース | `add_in_place`, `sub_in_place`, `mul_in_place`, `div_in_place`, `rem_in_place` | `target` の形を保ったまま `rhs` をブロードキャストして `target_i = target_i op rhs_i` をゼロ確保で更新 |
| 単項数学 | `neg`, `abs`, `sqrt`, `cbrt`, `square`, `reciprocal`, `floor`, `ceil`, `trunc`, `round`, `round_even`, `sign`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `sinh`, `cosh`, `tanh`, `asinh`, `acosh`, `atanh`, `exp`, `exp2`, `expm1`, `log`, `log2`, `log10`, `log1p`, `sinc`, `degrees`, `radians` | 符号・丸め・三角・双曲線・指数対数・正規化 `sinc`・角度変換 |
| 複素数・特殊値 | `real`, `imag`, `conj`, `angle`, `is_nan`, `is_infinite`, `is_finite`, `signbit`, `nan_to_num` | `Complex<'a>` ビューの実部/虚部/共役/偏角と、`NaN`/`±Inf`/符号ビット判定・置換 |
| 比較・近傍一致・外積 | `elem_eq`, `elem_ne`, `elem_lt`, `elem_le`, `elem_gt`, `elem_ge`, `eq_scalar`, `ne_scalar`, `lt_scalar`, `le_scalar`, `gt_scalar`, `ge_scalar`, `isclose`, `allclose`, `array_equal`, `array_equiv`, `outer_with`, `accumulate` | 要素ごと比較マスク・許容誤差比較・外積表・軸に沿った累積適用 |

同じ型どうしの所有 `Tensor<'a>` には `Add`・`Sub`・`Mul`・`Div`・`Rem`・`Pow`・`Neg` の借用算術インスタンスが、さらに `View` どうし・`Tensor` と `View`・スカラーとの間には [43 個の演算子の拡張インスタンス](#算術演算子)が定義されており、`a + b`・`ref a[1..] - ref a[..^1]`・`2.0 * x + 1.0`・`-v` を被演算子を消費せずにそのまま書けます（形状非互換時はトラップします）。複合代入 `+=` 等は [NC20](../../../_features/NC20-compound-assignment-and-mutable-index-views.md) で追加予定です。

```tsuzuri run=true
let x = Tensor.of_array [2, 3] [2.0, 4.0, 6.0, 10.0, 20.0, 30.0]
let mean = Tensor.of_array [2, 1] [4.0, 20.0]
let std_dev = Tensor.of_array [2, 1] [2.0, 10.0]
let z = (x - mean) / std_dev
let expected = Tensor.of_array [2, 3] [-1.0, 0.0, 1.0, -1.0, 0.0, 1.0]
Tensor.allclose 1e-12 1e-12 false (Tensor.view (ref z)) (Tensor.view (ref expected))
```

実行結果:

```text
true
```

#### NumPy との違い

| 項目 | NumPy | Tsuzuri `Tensor` |
| --- | --- | --- |
| 整数の `/` と `//` | `/` は浮動小数点の真の除算、`//` は床除算 | `/`（および `Tensor.div`）は Tsuzuri の整数規則どおり 0 方向切り捨て。床除算は `Tensor.floor_divide`（浮動小数点は `Tensor.floor_divide_float`） |
| 整数の `%`（剰余） | `np.remainder` / `%` は除数の符号に揃う床剰余 | `%`（および `Tensor.rem`）は被除数の符号を保つ。床剰余は `Tensor.floor_mod`（浮動小数点は `Tensor.floor_mod_float`）、IEEE 754 最近接偶数剰余は `Tensor.ieee_remainder` |
| 丸め（`round`） | `np.round` は最近接偶数丸め | `Tensor.round` は 0 から遠い側へ丸め（`Math.round`）。最近接偶数丸めは `Tensor.round_even` |
| `sign(-0.0)` | `+0.0` | ゼロの符号を保ち `-0.0`（`Math.sign` と同じ） |
| `@checked` | — | `@checked` は演算子インスタンス内部の要素演算までは検査しません |
| ブールマスクの結合 | `m1 & m2`、`m1 \| m2`、`~m` | 演算子 `&&&` 等は整数ビット演算専用です。マスクの結合には `Tensor.logical_and`・`Tensor.logical_or`・`Tensor.logical_xor`・`Tensor.logical_not` を使います |
| `==` 演算子 | 要素ごとのブール配列 | コレクション全体の等価 `bool`。要素ごとの比較マスクは `Tensor.elem_eq`〜`elem_ge`・`gt_scalar` 等を使います |

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
let rotated = ref v |> Tensor.permute [1, 0, 2]
let _ = rotated
let rotated2 = ref v |> Tensor.permute [2, 0, 1]
let corner = ref rotated2 |> Tensor.at [3, 1, 2]
let missing = ref rotated2 |> Tensor.get [4, 0, 0] |> Maybe.default_value (-1)
let plane_count = ref v |> (Tensor.index_axis 0 1 >> Tensor.count)
$"{deref corner} {missing} {plane_count}"
```

実行結果:

```text
23 -1 12
```

`>>` で合成した関数は、前の関数が返した窓を、次の関数が借りて受け取ります。借りた一時の窓の中を指す参照を返す関数（`at`、`shape`、`strides`）につなぐと、参照が一時の窓より長生きしてしまうので `E1013` です。窓を `let` に束縛してから、上の例のように `ref rotated2 |> Tensor.at [3, 1, 2]` と書きます。

## 角括弧の添字

`Tensor`・`Tensor.View`・`Tensor.Mut` は、カンマ区切りの角括弧で要素の読み書きと厳格な窓の切り出しを行えます（範囲・単独 `..`・`^k` を含むキーと 1 成分の範囲窓 `ref t[a..b]` は edition 2028 から、`t[i, j]` と `Slice.Sel` の値だけのキーは全 edition で使えます）。

- **要素の読み書き（確保 0 回）**: 位置だけ（整数と `^k`）のキー `t[1, 2]`・`t[^0, 0]` は要素を返し、`mt[1, 2] = v`・`mt[^0, 0] = v` は `Tensor.Mut` へその場で書き込みます。1〜4 軸はタプルのインスタンス、5 軸以上はスタック配列で解決され、ヒープ確保は 0 回です。位置の数が軸の数と一致しないとトラップします（2 次元の `t[0]` は行ではなくトラップです。行は `ref t[0, ..]` と書きます）。
- **厳格な窓 `ref t[…]`（要素をコピーせず、shape と strides の 2 配列を確保）**: 範囲・単独 `..`・[`Slice.Sel`](./slice.md) の成分を 1 つでも含むキーは `Tensor.View` を返します。整数の成分はその軸を除き、範囲と `..` は軸を残します。足りない末尾の軸は軸全体（`..`）で補われ、`Slice.rest()` は残りの軸すべてに展開されます（キーに 1 つまで）。`Slice.Sel.AddAxis` は長さ 1・ストライド 0 の新しい軸を挿入します。刻み付きの範囲 `ref t[0 .. 2 ..< 6, ..]` や負の刻みによる反転 `ref t[.. -1 .., ..]` も同じ構文で書けます。
- **窓の連鎖と要素の読み出し**: `View` から `View` を切り出す連鎖 `ref t[1..2, ..][0, ..]` はそのまま書けます。切り出した窓から要素を読むときは `(ref t[1..2, ..])[0, 1]` のように窓の部分だけを括弧で囲みます。

```tsuzuri edition=2028 run=6%208%20%5B4%2C%205%2C%206%2C%207%5D%20%5B1%2C%202%2C%205%2C%206%5D%20%5B8%2C%204%2C%200%5D%20%5B1%2C%203%2C%204%5D
let t = Tensor.init [3, 4] (\i -> i)
let elem = t[1, 2]
let bottom_left = t[^0, 0]
let row1 = Tensor.to_array (Tensor.to_tensor (ref (ref t[1, ..])))
let block = Tensor.to_array (Tensor.to_tensor (ref (ref t[0..1, 1..<3])))
let rev_col0 = Tensor.to_array (Tensor.to_tensor (ref (ref t[.. -1 .., 0])))
let expanded_view = ref t[Slice.Sel.AddAxis, ..]
let expanded = Tensor.shape (ref expanded_view)
$"{elem} {bottom_left} {row1} {block} {rev_col0} {expanded}"
```

実行結果:

```text
6 8 [4, 5, 6, 7] [1, 2, 5, 6] [8, 4, 0] [1, 3, 4]
```

### NumPy との対応と違い

| NumPy | 旧来の `Slice.Axis` 長い形 | 角括弧の構文（edition 2028） |
| --- | --- | --- |
| `a[i, j]` / `a[-1, 0]` | `t[[i, j]]` / – | `t[i, j]` / `t[^0, 0]` |
| `a[1:4]` | `ref t[[Slice.range 1 4]]` | `ref t[1..<4]` または `ref t[1..3]` |
| `a[0]`（2 次元の行） | `ref t[[Slice.at 0]]` | `ref t[0, ..]`（`t[0]` はトラップ） |
| `a[:, 0]` / `a[-1, :]` | `ref t[[Slice.all(), Slice.at 0]]` / `ref t[[Slice.at (-1)]]` | `ref t[.., 0]` / `ref t[^0, ..]` |
| `a[:-1]` / `a[1:-1]` / `a[-3:]` | `Slice.until (-1)` / `Slice.range 1 (-1)` / `Slice.from (-3)` | `ref t[..^1]` / `ref t[1..^1]` / `ref t[^2..]` |
| `a[1:7:2]` / `a[::2]` / `a[::-1]` / `a[4:0:-1]` | `Slice.step 1 7 2` / `Slice.every 2` / `Slice.rev()` / `Slice.step 4 0 (-1)` | `ref t[1 .. 2 ..< 7]` / `ref t[.. 2 ..]` / `ref t[.. -1 ..]` / `ref t[4 .. -1 .. 1]` |
| `a[None, :]` / `a[..., 0]` | `Slice.new_axis()` / – | `ref t[Slice.Sel.AddAxis, ..]` / `ref t[Slice.rest(), 0]` |
| `a[mask]` | `t[mask]` | `t[mask]`（コピー、`ref` なし） |

NumPy との違いは次のとおりです。

- **範囲外は切り詰めずトラップ**: 長さ 0 の軸への `ref t[1..]` や、長さ 1 以下の軸への `ref t[1..^1]`、`n = 9` への `ref t[0 .. 4 ..< 10]` はすべてトラップします。丸めるスライスが要るときは `ref t[[Slice.range …]]` を使います。
- **負の数は末尾から数えない**: 末尾からの位置は `^k`（`^0` が最後の要素）です。範囲の境界や窓の整数の成分に負の整数リテラル（`ref t[..-1, 0]`・`ref t[-1, ..]`）を書くと警告 `W1014` が出ます（要素の読み出し `t[-1, 0]` は `Index` のキーなので警告は出ず、実行時にトラップします）。
- **位置・境界・刻みの型は `i64`**: `i32` などを渡すと `E1003`・`E1005` になるため、`a as i64` で明示変換します。ブールマスクと範囲を同じキーに混ぜることはできません（`E1003`）。

## 算術演算子

`Tensor<'a>` と `Tensor.View<'a>` は、同じ型どうしの `Tensor<'a>` 算術インスタンス（7 個）と、[演算子の拡張インスタンス](../types-and-type-inference/type-classes.md#演算子の拡張インスタンスoei)（6 二項演算子 × 7 形 + 単項 `Neg<View<'a>, Tensor<'a>>` = 43 個）により、所有テンソル・借用の窓・要素スカラーを自由に組み合わせて `+`・`-`・`*`・`/`・`%`・`**`・単項 `-` で計算できます。結果は常に新しい所有 `Tensor<'a>` です。

| 左辺 × 右辺（`op` は `+` `-` `*` `/` `%` `**`） | インスタンスの形 | 呼び出すカーネル（`Sub` の例） |
| --- | --- | --- |
| `Tensor<'a>` × `Tensor<'a>` | `Sub<Tensor<'a>>` | `sub_or_trap (ref (view l)) (ref (view r))` |
| `View<'a>` × `View<'a>` | `Sub<View<'a>, View<'a>, Tensor<'a>>` | `sub_or_trap l r` |
| `Tensor<'a>` × `View<'a>` / `View<'a>` × `Tensor<'a>` | `Sub<Tensor<'a>, View<'a>, Tensor<'a>>` / `Sub<View<'a>, Tensor<'a>, Tensor<'a>>` | `sub_or_trap`（`Tensor` 側は `view`） |
| `'a` × `Tensor<'a>` / `Tensor<'a>` × `'a` | `Sub<'a, Tensor<'a>, Tensor<'a>>` / `Sub<Tensor<'a>, 'a, Tensor<'a>>` | `rsub_scalar` / `sub_scalar`（`*` の左スカラーは `scale`） |
| `'a` × `View<'a>` / `View<'a>` × `'a` | `Sub<'a, View<'a>, Tensor<'a>>` / `Sub<View<'a>, 'a, Tensor<'a>>` | `rsub_scalar` / `sub_scalar` |
| 単項 `-t` / `-v` | `Neg<Tensor<'a>>` / `Neg<View<'a>, Tensor<'a>>` | `Tensor.neg` |

- 被演算子は共有借用で受け取るため、`a - b` の後も `a`・`b` をそのまま再利用できます。接尾辞のない数値リテラル（`2.0 * x + 1.0`、`x - 1`、`x ** 2`）は相手のテンソル要素型 `'a` として読まれます。
- 2 つのテンソル／窓の演算は NumPy 互換のブロードキャストを行い、形が合わなければ出力を確保する前にトラップします（`Result` で受け取りたいときは `Tensor.add`・`Tensor.sub` などの関数版を使います）。整数の `/` と `%` は要素ごとに 0 方向の切り捨てです。

```tsuzuri edition=2028 run=%5B10%2C%2010%2C%2010%2C%2010%5D%20%5B2%2C%204%2C%206%5D%20%5B10%2C%2021%2C%2032%2C%2013%2C%2024%2C%2035%5D
let a = Tensor.of_array [5] [10.0, 20.0, 30.0, 40.0, 50.0]
let d = ref a[1..] - ref a[..^1]
let x = Tensor.of_array [3] [0.5, 1.5, 2.5]
let y = 2.0 * x + 1.0
let t = Tensor.init [2, 3] (\i -> i)
let row = Tensor.init [3] (\i -> (i + 1) * 10)
let s = t + Tensor.view (ref row)
$"{Tensor.to_array d} {Tensor.to_array y} {Tensor.to_array s}"
```

実行結果:

```text
[10, 10, 10, 10] [2, 4, 6] [10, 21, 32, 13, 24, 35]
```

## 軸指定の縮約・統計・`NaN` 無視版

単一軸のオプション `Tensor.AxisOptions { axis, keepdims }`、複数軸の `Tensor.AxesOptions { axes, keepdims }`、分散・標準偏差の `Tensor.VarianceOptions { axis, keepdims, ddof }` を使って、全体縮約・単一軸縮約（`*_axis`）・`keepdims` 付き縮約（`*_with`）・複数軸縮約（`*_axes`）を呼び分けられます。対象となる `View<'a>` は常に最後の引数です。

| 分類 | 関数一覧 | 備考 |
| --- | --- | --- |
| 和・補償付き和・積 | `sum`・`sum_axis`・`sum_with`・`sum_axes`、`sum_kahan`・`sum_kahan_axis`・`sum_kahan_with`、`prod`・`prod_axis`・`prod_with`・`prod_axes` | 空軸では和は `0`、積は `1` |
| 平均・重み付き平均・分散・標準偏差 | `mean`・`mean_axis`・`mean_with`・`mean_axes`、`average`・`average_axis`、`var`・`var_axis`・`var_with`、`std`・`std_axis`・`std_with` | 2 パス法。空軸または `n - ddof <= 0` では `NaN` |
| 極値・位置・範囲 | `min`・`min_axis`・`min_with`・`min_axes`、`max`・`max_axis`・`max_with`・`max_axes`、`argmin`・`argmin_axis`・`argmin_with`、`argmax`・`argmax_axis`・`argmax_with`、`ptp`・`ptp_axis`・`ptp_with` | 空軸ではトラップ。`NaN` は伝播（`argmin` / `argmax` は最初の `NaN` 位置）、`±0.0` の符号を保持 |
| 論理・非ゼロ件数 | `all`・`all_axis`・`all_with`・`all_axes`、`any`・`any_axis`・`any_with`・`any_axes`、`count_nonzero`・`count_nonzero_axis`・`count_nonzero_with` | 空軸では `all` が `true`、`any` が `false` |
| `NaN` 無視版 | `nansum`・`nansum_axis`・`nansum_with`、`nanprod`・`nanprod_axis`・`nanprod_with`、`nanmean`・`nanmean_axis`・`nanmean_with`、`nanvar`・`nanvar_axis`・`nanvar_with`、`nanstd`・`nanstd_axis`・`nanstd_with`、`nanmin`・`nanmin_axis`・`nanmin_with`、`nanmax`・`nanmax_axis`・`nanmax_with`、`nanargmin`・`nanargmin_axis`、`nanargmax`・`nanargmax_axis` | 全要素 `NaN` のとき `nansum` は `0.0`、`nanprod` は `1.0`、`nanmean`/`nanvar`/`nanmin`/`nanmax` は `NaN`、`nanargmin`/`nanargmax` はトラップ |

```tsuzuri edition=2028 run=%5B5%2C%207%2C%209%5D%20%5B2%2C%201%5D%202
let m = Tensor.of_array [2, 3] [1, 2, 3, 4, 5, 6]
let mv = Tensor.view (ref m)
let s0 = Tensor.sum_axis 0 (ref mv)
let sk = Tensor.sum_with (Tensor.AxisOptions { axis: 1, keepdims: true }) (ref mv)
let skv = Tensor.view (ref sk)
let wnan = Tensor.of_array [3] [1.0, 0.0 / 0.0, 3.0]
let wnv = Tensor.view (ref wnan)
$"{Tensor.to_array s0} {Tensor.shape (ref skv)} {Tensor.nanmean (ref wnv)}"
```

## 累積・差分・数値微分・台形積分・1D 畳み込み・線形補間

- 累積: `cumsum`・`cumsum_flat`・`cumprod`・`cumprod_flat`・`nancumsum`・`nancumprod`
- 差分・数値微分・積分: `diff`・`diff_n`・`ediff1d`・`gradient`・`gradient_1d`（内部は 2 次中心差分、端点は 1 次片側差分）・`trapezoid`・`trapezoid_x`
- 1D 信号処理・補間: `convolve`・`correlate`（`ConvolveMode.Full` / `Same` / `Valid`）・`interp`・`interp_extrap`

## 分位点・ヒストグラム・共分散とソート・選択・1D 集合演算

- 分位点 (`QuantileMethod`: `Linear` / `Lower` / `Higher` / `Midpoint` / `Nearest`、`QuantileOptions { method, axis, keepdims }`): `median`・`median_axis`・`median_with`・`nanmedian`・`nanmedian_axis`・`nanmedian_with`・`quantile`・`quantile_with`・`quantiles`・`percentile`・`percentile_with`・`nanquantile`・`nanquantile_with`・`nanpercentile`・`nanpercentile_with`
- ヒストグラム・ビン集計: `histogram`・`histogram_edges`・`histogram_weighted`・`histogram2d`・`histogramdd`・`bincount`・`bincount_weighted`・`digitize`
- 共分散・相関: `cov`・`cov_with`・`corrcoef`・`corrcoef_with`（常に 2D `[p, p]` 行列を返す）
- ソート・選択・探索・集合演算: 安定ソート `sort`・`sort_flat`・`argsort`・`argsort_flat`・多重キー `lexsort`（最終行が主キー、`NaN` は末尾）、期待計算量 $O(n)$（最悪 $O(n \log n)$）の introselect による `partition`・`argpartition`、二分探索 `searchsorted`（`SearchSide.Left` / `Right`）、および 1D 集合演算 `unique`・`unique_full`（`UniqueResult`）・`isin`・`intersect1d`・`union1d`・`setdiff1d`・`setxor1d`

```tsuzuri edition=2028 run=%5B1%2C%202%2C%203%5D%2025%20%5B2%5D
let raw = Tensor.of_array [5] [3, 1, 2, 1, 3]
let q_in = Tensor.of_array [4] [40.0, 10.0, 30.0, 20.0]
let b = Tensor.of_array [3] [2, 4, 5]
let rv = Tensor.view (ref raw)
let qv = Tensor.view (ref q_in)
let bv = Tensor.view (ref b)
$"{Tensor.to_array (Tensor.unique (ref rv))} {Tensor.median (ref qv)} {Tensor.to_array (Tensor.intersect1d (ref rv) (ref bv))}"
```

## 計算量

| 操作 | 計算量 |
| --- | --- |
| `as_array` | $O(1)$、要素の複製なし |
| `of_array` | $O(\text{rank})$（形の検査だけ）、要素の複製なし |
| `to_array`・`reshape`・`reshape_infer`・`try_reshape`・`of_matrix`・`to_matrix`・`update` | 形の検査と窓のメタデータの $O(\text{rank})$ だけで、要素は複製しない（`update` はこれに `writer` の費用が加わる） |
| `view`・`borrow`・`permute`・`index_axis`・`narrow`・`reshape_view`・`is_contiguous`・`count`・`at`・`get` | $O(\text{rank})$。窓を返す API は、形とストライドの配列を新しく確保する |
| 添字 `t[i, j]`・`t[^0, 0]`・`t[[…]]`・`t[(i, j)]`・`m[i, j] = v`（要素の読み書き） | $O(\text{rank})$（1〜4 軸タプルは $O(1)$）、確保 0 回 |
| 添字 `ref t[i, a..b, ..]`・`ref t[[Slice…]]`（窓） | $O(\text{rank})$、要素はコピーせず、結果の形とストライドの 2 配列を確保 |
| 算術演算子 `v - w`・`2.0 * v`・`t + v`・`-v` | $O(\text{count})$、結果の `Tensor`（形とデータ配列）と `Tensor` 被演算子ごとの一時 `view`（2 配列）を確保。確保なしの再利用は `add_into`・`add_in_place` を使う |
| `init`・`fold`・`map`・`to_tensor`・縮約（`sum*`・`mean*`・`var*`・`min*`・`argmax*`・`nan*`）・`cumsum`・`diff`・`gradient`・`trapezoid` | $O(\text{count})$ |
| `partition`・`argpartition`（長さ $n$ の軸スライスごと） | 期待 $O(n)$、最悪 $O(n \log n)$（中央値 3 点選び＋深さ上限ソート切り替えの introselect） |
| `sort`・`argsort`・`median`・`quantile`・`unique*` | スライス長 $n$ に対して $O(n \log n)$ |
| `searchsorted`・`interp`・`digitize` | テーブル長 $m$・問い合わせ要素数 $q$ に対して $O(q \log m)$ |

軸は 64 本までなので、$O(\text{rank})$ は定数です。要素の読み出しは、連続でない窓でも、軸ごとのループで行います。要素のない窓（長さが 0 の軸を持つ窓）では、`fold` は他の軸がどれだけ長くても（`[4611686018427387904, 0]` のような形も作れます）何も走査せずに初期値を返します。最適化水準による違いもありません。

## できないこと

- 線形代数・`einsum`・FFT の演算は `Tensor` 本体には含まれません。また、書き込み窓 `ref mut t[…]` と複合代入 `+=` 等（[NC20](../../../_features/NC20-compound-assignment-and-mutable-index-views.md)）も計画中です。ブールマスクどうしの論理積・論理和に `&`・`|` 演算子は使えないため、`Tensor.logical_and`・`Tensor.logical_or`・`Tensor.logical_not` を使います。マスクと範囲を 1 つの角括弧に混ぜることもできません（`E1003`）。
- 窓の並列・SIMD・GPU のカーネルは未実装です。平らなバッファは `Tensor.as_array` で取り出せるので、[Parallel](./parallel.md) や [Gpu](./gpu.md) の API に渡せます。
- 65 本以上の軸は作れません。`as_matrix_view` は負のストライドを持つ 2D 窓ではトラップするため、先に `to_tensor` 等で連続化してください。

## まとめ

- `Tensor` は形と行優先のバッファを所有し、`Tensor.View`（読み取り専用・負/0 ストライド対応）と `Tensor.Mut`（単射な書き込み専用）が同じバッファを見ます。
- `Matrix` とは `of_matrix`・`to_matrix` で要素を複製せずに行き来できます。`t[i, j]`・`t[^0, 0]`・`ref t[i, a..b, ..]` や [`Slice`](./slice.md)・ブールマスクを使った汎用添字が使えます。
- 窓は持ち主を動かすと `E1014`、持ち主より長く使うと `E1013` です。

## 関連項目

- [NumPy からの移行ガイド](./numpy-migration.md) — NumPy の関数・引数と `Tensor` API の対応表
- [Slice](./slice.md) — 厳格なセレクタ `Slice.Sel` と丸める記述子 `Slice.Axis`
- [Matrix](./matrix.md) — 2 次元の持ち主と、行列積
- [MatrixView](./matrix-view.md) — 2 次元の窓と、書き込める窓
- [Array](./array.md) — 平らな配列とスライス
- [演算子と式](../values-and-functions/op-and-expressions.md) — 角括弧の範囲と評価順序
- [言語リファレンスの目次](../index.md)
