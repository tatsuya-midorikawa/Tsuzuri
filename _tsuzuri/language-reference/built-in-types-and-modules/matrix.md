# Matrix

`Matrix<'a>` は、行と列を持つ行列を 1 本の連続したバッファで持つ型です。要素 `(row, col)` は、行優先で `data[row * cols + col]` に並びます。`[[T]]` のように行ごとに別々の確保をせず、行の長さも揃っています。

構築、要素の参照と置き換え、行の借用、転置、要素ごとの変換と集計、加算、行列積を提供します。行列積は、出力の各要素を足す順序を API の約束として固定しています。どの機械でも、native でも WASM でも、`-O0` でも `-O3` でも、同じビットを返します。FMA を使う別名の積 `mul_fma` と、行を分けて並列に計算する `mul_parallel`・`mul_fma_parallel` もあります。

行列を借りて、転置・部分行列・行・列として見る窓は [MatrixView](./matrix-view.md)、N 次元の配列は [Tensor](./tensor.md) です。

## この記事のポイント

- 型は `Matrix<'a>`。要素型にかかわらず常に非 Copy 型です。コピーは明示的に書きます。
- 内部は不透明です。構築・フィールド参照・パターン分解・`{ m with ... }` は `E1022` です。公開 C ABI への export は `E1008` です。
- `of_array` と `to_array` は所有権を移すだけで、要素を複製しません。`at`・`row`・`as_array` は確保なしの共有借用です。`set` は行列を消費して 1 要素をその場で置き換えます。
- `mul` の出力要素は `+0` から `k = 0, 1, …` の順に `total = total + (left * right)` を行った値です。積と和を別々に丸め、FMA にまとめません。実装は出力の行ごとに更新する形で、ベクトル化できますが、各要素の演算の列は変わりません。
- `mul_fma` は、1 ステップごとに `Math.fma` を使う別名の積です。ビットは `mul` と違うことがあります。`Math.fma` がハードウェアの FMA 命令になるのは、AArch64 向けにビルドした `tsuzuri` が native の `f32`・`f64` を生成するときだけです。それ以外（x86-64 向けなどにビルドした `tsuzuri` の native と、wasm32）では、ソフトウェアのルーチンを積和ごとに呼ぶので、`mul` よりはるかに遅くなります。結果のビットは、どちらでも同じです。
- `mul_parallel`・`mul_fma_parallel` は、行をチャンクに分けて並列に計算します。チャンクの境界は形だけで決まるので、結果は CPU の数によらず `mul`・`mul_fma` と同じビットです。
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

## 要素を置き換える

`Matrix.set m row col value` は、`m` を消費して、`(row, col)` を `value` に置き換えた行列を返します。バッファは複製せず、その場で書き換えます。行と列を別々に検査し、範囲外は `at` と同じくトラップです。

```tsuzuri run=0%2010%2020%203
let m = Matrix.init 2 2 (\row col -> row * 2 + col)
let m = Matrix.set m 0 1 10
let m = Matrix.set m 1 0 20
let flat = Matrix.as_array (ref m)
$"{flat[0]} {flat[1]} {flat[2]} {flat[3]}"
```

実行結果:

```text
0 10 20 3
```

`set` の後で古い `m` を使うと `E1012`、`Matrix.row` などの借用が生きている間に `set` を呼ぶと `E1014` です。1 つずつ置き換える代わりに、行ごとや部分行列ごとにまとめて書き換えるときは、[MatrixView](./matrix-view.md) の書き込める窓を使います。

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

### 実装

`mul` は、出力を 1 行ずつ、`k` と `j` の二重ループで更新します（i-k-j の順）。`out(i, j)` は `out(i, j) + left(i, k) * right(k, j)` だけで更新され、`k` は小さい方から進みます。出力の各要素に起きる演算の列は、出力要素ごとに `k` の和を取る素朴な形（i-j-k）と同じです。変わるのは、異なる出力要素を訪れる順序だけなので、ビットは変わりません。

内側の `j` のループは `right` の連続した 1 行を読み、各レーンが別の出力要素です。和の順序を変えずに、オプティマイザがベクトル化できます。

生成コードは確かめてあります（2026-10-10、`-O3`）。

- arm64 の native: `f64` は `fmul.2d`・`fadd.2d`、`f32` は `fmul.4s`・`fadd.4s` のループです。`fmla`・`fmadd` はなく、積と和は別々のままです。`i64` は NEON に 64 ビット整数の積のベクトル命令がないので、スカラーの `mul`・`madd` です。
- wasm32 の既定: `f64.mul`・`f64.add` のスカラーだけです。`--wasm-feature simd128` を付けると、`f64x2.mul`・`f64x2.add` のループが出ます。
- x86-64 と、SVE や AVX-512 のような広いベクトルは確かめていません。

Apple M1 Max の native `-O3` で、`n = 256` と `512` の積は `f64` が約 11〜12 GFLOP/s、`f32` が約 21〜23 GFLOP/s、`i64` が約 5.5〜6 GFLOP/s でした。同じ形の C の i-k-j とほぼ同じで、Tsuzuri の i-j-k のループの約 5〜6 倍（`f64`）です。条件と表は [ベンチマーク](../../../docs/benchmarks.md) にあります。この値は API の約束ではありません。

### 順序の違う積: mul_fma

`Matrix.mul_fma (ref left) (ref right)` は、1 ステップごとに `Math.fma` を使います。出力 `(i, j)` は、`total = +0` から始め、`k = 0, 1, …` の順に

```text
total = fma(left(i, k), right(k, j), total)
```

を行った値です。積と和を 1 回の丸めにまとめるので、`mul` とビットが違うことがあります。順序は `Array.dot_fma` と同じです。要素型は `Float<'a>` で、整数は `E1005` です。`mul` は、どの最適化水準でも暗黙に FMA へまとめません。FMA の結果が欲しいときは、`mul_fma` と名前で選びます。

```tsuzuri run=0%20-8.673617379884035e-19
let left = Matrix.of_array 1 2 [1.0 + 0.000000001862645149230957031250, -(1.0 + 0.000000000931322574615478515625)]
let right = Matrix.of_array 2 1 [1.0, 1.0 + 0.000000000931322574615478515625]
let separate = Matrix.mul (ref left) (ref right)
let fused = Matrix.mul_fma (ref left) (ref right)
$"{deref (Matrix.at (ref separate) 0 0)} {deref (Matrix.at (ref fused) 0 0)}"
```

実行結果:

```text
0 -8.673617379884035e-19
```

1 つ目の積は誤差なく `1 + 2^-29` で、2 つ目の積 `-(1 + 2^-29 + 2^-60)` は `-(1 + 2^-29)` に丸められます。別々に丸める `mul` の和は `0` ですが、`mul_fma` は 2 つ目の積の丸め誤差 `-2^-60` を残します。

`Math.fma` がハードウェアの FMA 命令（`llvm.fma`）になるのは、AArch64 向けにビルドした `tsuzuri` が native の `f32`・`f64` を生成するときだけです（arm64 の `fmadd`・`fmla.2d` を確かめました）。それ以外では、正しく丸めるソフトウェアのルーチン `tz_soft_fma` を積和ごとに呼びます。x86-64 向けなどにビルドした `tsuzuri` の native と、1 命令のスカラー fma を持たない wasm32 がこれに当たります。結果のビットはどちらでも同じですが、速さは大きく違います。

| 条件（Apple M1 Max） | 積の大きさ | 1 回の積 | 1 積和あたり |
| --- | --- | --- | --- |
| native、ハードウェアの FMA | 128 × 128 × 128 | 約 0.34 ms | 約 0.16 ns |
| native、ソフトウェアのルーチン | 64 × 64 × 64 | 約 0.40 s | 約 1.5 µs |
| native、ソフトウェアのルーチン | 128 × 128 × 128 | 約 3.4 s | 約 1.6 µs |
| wasm32（Node） | 64 × 64 × 64 | 約 0.46 s | 約 1.8 µs |

ソフトウェアのルーチンの native の行は、AArch64 向けの `tsuzuri` の判定をテストのために外し、`tz_soft_fma` を使わせて測った値です（x86-64 の機械では測っていません）。`mul` は 128 × 128 × 128 で約 0.35 ms なので、ソフトウェアのルーチンの `mul_fma` は `mul` の約 1 万倍です。1 積和あたりの費用が変わらないとすると、`512 × 512 × 512` の積は約 3.5 分かかります。`mul_fma_parallel` も同じルーチンを呼びます。AArch64 向けの native 以外では、FMA の結果そのものが必要なときだけ、小さい行列に `mul_fma`・`mul_fma_parallel` を使ってください。

### 並列: mul_parallel

`Matrix.mul_parallel` と `Matrix.mul_fma_parallel` は、出力の行を [Parallel.for_each_chunk](./parallel.md) で分けて計算します。要素型には `Send<'a>` も要ります。

- 1 つのチャンクは、およそ $2^{20}$ 回の積和になる行数の行です（`1048576 / (inner * cols)` 行、最低 1 行）。境界は形（`inner` と `cols`）だけで決まり、CPU の数・SIMD 幅・ターゲットで変わりません。
- 各出力要素は `mul`（`mul_fma`）と同じ演算の列なので、結果は `mul`（`mul_fma`）と、スレッドの数によらずビット単位で一致します。
- 積が 1 チャンクで済むとき（約 $2^{20}$ 回以下の積和）は、呼んだスレッドで `mul` と同じ処理をします。2 チャンク以上のときは、スレッドへ借用を渡せないので、両方の入力を 1 回複製して `Arc` で共有します。
- 既定の WASM はスレッドを持たないので、チャンクを順に処理し、速さは `mul` と同じです。`--wasm-feature threads` を付けると、ワーカーで並列に動きます（この場合の結果が `mul` と一致することはテストしましたが、速さは測っていません）。

```tsuzuri run=130x128%20true%208
let left = Matrix.init 130 70 (\i k -> (i * 7 + k * 3) % 11 - 5)
let right = Matrix.init 70 128 (\k j -> (k * 5 + j * 2) % 13 - 6)
let sequential = Matrix.mul (ref left) (ref right)
let parallel = Matrix.mul_parallel (ref left) (ref right)
let digest = Matrix.fold (\total x -> total * 31 + x) 7
$"{Matrix.rows (ref parallel)}x{Matrix.cols (ref parallel)} {digest (ref sequential) == digest (ref parallel)} {deref (Matrix.at (ref parallel) 129 127)}"
```

実行結果:

```text
130x128 true 8
```

この形は 117 行と 13 行の 2 チャンクに分かれます。右端の要素は $\sum_k \text{left}(129, k) \cdot \text{right}(k, 127) = 8$ です。

Apple M1 Max（10 コア）の native `-O3` で、`n = 512` の `f64` は `mul` が約 12 GFLOP/s、`mul_parallel` が約 50〜58 GFLOP/s でした。`n = 64` は 1 チャンクなので `mul` と同じ速さです。同じ行分割の素朴な pthread の C は約 46〜51 GFLOP/s です。負荷のある共有機で測った値で、表は [ベンチマーク](../../../docs/benchmarks.md) にあります。

### 行ごとに並列に書く

`mul_parallel` のような処理を自分で書くときは、`Matrix.to_array` でバッファを取り出し、`Parallel.for_each_chunk` に渡します。チャンクの大きさを 1 行の長さ `cols` にすれば、各チャンクがちょうど 1 行です。チャンクの境界は `cols` だけで決まります。

```tsuzuri run=24
def scale_row :: i64 -> i64 -> ref mut [i64..] -> unit
fn scale_row cols start chunk =
    let row = start / cols
    for j in 0i64 .. (chunk.length - 1) do
        let value = chunk[j]
        Array.write chunk j (value * (row + 1))

let m = Matrix.init 3 4 (\_row _col -> 1)
let mut data = Matrix.to_array m
Parallel.for_each_chunk 4 (scale_row 4) (ref mut data)
let scaled = Matrix.of_array 3 4 data
Matrix.fold (\total x -> total + x) 0 (ref scaled)
```

実行結果:

```text
24
```

各行を `行番号 + 1` 倍して、`4 * (1 + 2 + 3)` で `24` です。コールバックが借用を捕捉することはできないので、`cols` は部分適用 `scale_row 4` で渡します。

## GPU との連携

`Gpu` の CPU 参照バッファには、`Matrix.as_array` で行列の全要素を渡し、`Matrix.of_array` で行列に戻せます。`Gpu.map` は要素ごとの変換なので、行列の形を意識する必要はありません。

```tsuzuri run=24
let device = Result.get (Gpu.request Gpu.CpuReference)
let m = Matrix.init 2 3 (\row col -> row * 10 + col)
let buffer = Gpu.from_array (ref device) (Matrix.as_array (ref m))
let doubled = Gpu.map (ref device) (\x -> x * 2) buffer
let result = Matrix.of_array (Matrix.rows (ref m)) (Matrix.cols (ref m)) (Gpu.to_array doubled)
deref (Matrix.at (ref result) 1 2)
```

実行結果:

```text
24
```

これは今の `Gpu` が提供する範囲です。実行は CPU 参照だけで、GPU メモリには載りません（[Gpu](./gpu.md)）。**行列積の GPU カーネルは提供していません。** 今の GPU カーネルは `export` された `i32 -> i32` か `i32u -> i32u` の 1 つの要素ごとの関数で、除算と浮動小数点を受け付けません。2 次元の添字と `k` の総和を書けないので、`Matrix.mul` の置き換えにはなりません。実機の GPU での実行は計画中です（[F09](../../../_features/F09-gpu-float-runtime.md)）。

## トラップ

| 条件 | API | 表示 |
| --- | --- | --- |
| 次元が負、または `rows * cols` が `9223372036854775807` を超える | `of_array`・`init`・`mul`・`mul_fma`・`mul_parallel`・`mul_fma_parallel` | `trap: assertion failed` |
| `values.length != rows * cols` | `of_array` | 同上 |
| 添字が範囲外 | `at`・`row`・`set` | 同上 |
| 形の不一致 | `add`・`mul`・`mul_fma`・`mul_parallel`・`mul_fma_parallel` | 同上 |
| 要素の byte 数の溢れ | 確保する API | `trap: allocation size overflow` |
| 確保の失敗 | 確保する API | `trap: allocation failed` |

どの API も、前提条件の検査をすべて通してから、確保・要素の読み出し・callback の呼び出しをします。引数は左から右に評価します。

## 所有権と借用

`Matrix` は、バッファを所有する非 Copy 値です。要素型が `i64` のように Copy でも同じです。大きなバッファの暗黙の複製が `let` に隠れないようにするためです。複製は明示的に書きます。

```text
Matrix.of_array (Matrix.rows (ref m)) (Matrix.cols (ref m)) (deref (Matrix.as_array (ref m)))
```

- `let n = m` の後で `m` を使うと `E1012` です。`Matrix.to_array m` と `Matrix.set m …` の後も同じです。
- `at`・`row`・`as_array` の戻り値は、元の行列を共有借用します。借用が生きている間に行列を move・置換・`set` すると `E1014` です。
- 借用を関数の外へ返そうとすると `E1013` です。
- [MatrixView](./matrix-view.md) や [Tensor](./tensor.md) の窓も、持ち主の行列を借ります。同じ規則です。

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
| `set` | `Matrix<'a> -> i64 -> i64 -> 'a -> Matrix<'a>` |
| `map` | `Copy<'a> => ('a -> 'b) -> ref Matrix<'a> -> Matrix<'b>` |
| `fold` | `Copy<'a> => ('state -> 'a -> 'state) -> 'state -> ref Matrix<'a> -> 'state` |
| `transpose` | `Copy<'a> => ref Matrix<'a> -> Matrix<'a>` |
| `add` | `Numeric<'a> => ref Matrix<'a> -> ref Matrix<'a> -> Matrix<'a>` |
| `mul` | `Numeric<'a> => ref Matrix<'a> -> ref Matrix<'a> -> Matrix<'a>` |
| `mul_fma` | `Float<'a> => ref Matrix<'a> -> ref Matrix<'a> -> Matrix<'a>` |
| `mul_parallel` | `(Numeric<'a>, Send<'a>) => ref Matrix<'a> -> ref Matrix<'a> -> Matrix<'a>` |
| `mul_fma_parallel` | `(Float<'a>, Send<'a>) => ref Matrix<'a> -> ref Matrix<'a> -> Matrix<'a>` |

## 計算量

| 操作 | 計算量 |
| --- | --- |
| `at`・`row`・`as_array`・`rows`・`cols` | $O(1)$、確保なし |
| `of_array`・`to_array`・`set` | $O(1)$、複製なし |
| `init`・`map`・`transpose`・`add` | $O(\text{rows} \times \text{cols})$ |
| `mul`・`mul_fma` | $O(\text{rows} \times \text{inner} \times \text{cols})$ |
| `mul_parallel`・`mul_fma_parallel` | 同じ仕事を複数のスレッドで分ける。2 チャンク以上のときは入力の複製 $O(\text{rows} \times \text{inner} + \text{inner} \times \text{cols})$ を足す |

`mul` は再帰しないループです。stack の深さは大きさに依存しません。`mul_fma`・`mul_fma_parallel` の 1 積和あたりの費用は、`Math.fma` がハードウェアの命令かソフトウェアのルーチンかで約 1 万倍違います（[順序の違う積: mul_fma](#順序の違う積-mul_fma)）。性能は計測した事実だけを [ベンチマーク](../../../docs/benchmarks.md) に書いています。速度は API の約束ではありません。ブロッキングや pairwise の積、手書きの SIMD、BLAS のような最適化済みの積は提供しません。

## まとめ

- `Matrix` は行優先の連続バッファで、常に非 Copy です。
- `mul` の各出力要素は `+0` から `k` の昇順で、積と和を別々に丸めます。実装は行ごとの更新ですが、ビットは変わりません。
- 順序の違う積は別名です。FMA は `mul_fma`（AArch64 向けの native 以外ではソフトウェアの fma で遅い）、並列は `mul_parallel`・`mul_fma_parallel` で、後者はチャンクの境界が形だけで決まります。
- 窓は [MatrixView](./matrix-view.md)、N 次元は [Tensor](./tensor.md) です。GPU の行列積カーネルはありません。
- 前提条件の誤りは、確保や callback の前にトラップします。

## 関連項目

- [MatrixView](./matrix-view.md) — 転置・部分行列・行・列の窓と、書き込める窓
- [Tensor](./tensor.md) — N 次元の配列
- [Array](./array.md) — 平らな配列と `Array.dot`
- [Parallel](./parallel.md) — 排他スライスを分ける並列 API
- [Simd](./simd.md) — 明示的な SIMD。`Matrix.mul` はこれを使わず、オプティマイザのベクトル化に任せています
- [Gpu](./gpu.md) — CPU 参照バッファ
- [Map](./map.md) — 同じ不透明・非 Copy の標準 record
- [Math](./math.md) — 明示 FMA と順序付き集計
- [言語リファレンスの目次](../index.md)
