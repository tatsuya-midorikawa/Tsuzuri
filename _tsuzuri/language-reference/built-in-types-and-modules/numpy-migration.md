# NumPy からの移行ガイド

このページでは、Python の NumPy で書かれた多次元配列・統計・ソート・信号処理のコードを Tsuzuri の [`Tensor`](./tensor.md) へ書き換える際の設計原則と API 対応表をまとめます。

## 基本の違い

| 観点 | NumPy | Tsuzuri (`Tensor`) |
| --- | --- | --- |
| 所有と借用 | すべての `ndarray` が GC 管理の参照共有ビュー | 所有値 `Tensor<'a>`、読み取り専用の窓 `Tensor.View<'a>`（`Tensor.view (ref t)`）、単射な書き込み窓 `Tensor.Mut<'a>` |
| 引数の順序 | 先頭に配列 `np.sum(a, axis=0)` | パイプライン `v \|> Tensor.sum_axis 0` のため **対象の `View` / `Tensor` が最後の引数**（[引数の順序](./argument-order.md)） |
| 戻り値の型（`axis=None` vs `axis=int`） | 実行時にスカラーまたは `ndarray` に変わる | 静的型付けのため、全体縮約（`Tensor.sum v` → `'a`）と軸縮約（`Tensor.sum_axis 0 v` → `Tensor<'a>`）を別の関数に分ける |
| 範囲スライス | `a[1:4]` は半開区間 `[1, 4)` | Edition 2028 では `1..3` が閉区間、`1..<4` が半開区間、末尾相対は `^0`（最後の要素） |
| 出力バッファ再利用 (`out=`) | `np.add(a, b, out=dst)` | `Tensor.add_into va vb (ref mut dst)`（ヒープ確保 0 回、形状不一致は `Result.Error`） |

```tsuzuri edition=2028 run=%5B2%2C%205%5D%20%5B1%2C%202%2C%203%5D
let x = Tensor.of_array [2, 3] [3.0, 1.0, 2.0, 6.0, 4.0, 5.0]
let xv = Tensor.view (ref x)
let row_means = ref xv |> Tensor.mean_axis 1
let sorted = Tensor.sort 1 (ref xv)
let first_sorted = ref sorted[0, 0..<3]
$"{Tensor.to_array row_means} {Tensor.to_array (Tensor.to_tensor (ref first_sorted))}"
```

## 生成・形状変換・結合の対応

| NumPy | Tsuzuri (`Tensor`) |
| --- | --- |
| `np.zeros(shape)` / `np.ones(shape)` / `np.full(shape, v)` | `Tensor.zeros shape` / `Tensor.ones shape` / `Tensor.full v shape` |
| `np.arange(start, stop, step)` / `np.linspace(a, b, n)` | `Tensor.arange start stop step` / `Tensor.linspace a b n` |
| `np.eye(n)` / `np.diag(v, k)` | `Tensor.eye n` / `Tensor.diag k v` |
| `a.reshape(shape)` / `a.T` / `np.transpose(a, axes)` | `Tensor.reshape_infer shape t` / `Tensor.transpose v` / `Tensor.permute axes v` |
| `np.concatenate(xs, axis)` / `np.stack(xs, axis)` | `Tensor.concat axis views` / `Tensor.stack axis views` |
| `np.where(cond, x, y)` / `np.clip(a, lo, hi)` | `Tensor.where_select cond vx vy` / `Tensor.clip lo hi v` |

## 縮約・統計・`NaN` 無視版の対応

| NumPy | Tsuzuri (`Tensor`) |
| --- | --- |
| `np.sum(a)` / `np.sum(a, axis=k)` / `keepdims=True` | `Tensor.sum v` / `Tensor.sum_axis k v` / `Tensor.sum_with (Tensor.AxisOptions { axis: k, keepdims: true }) v` |
| `np.sum(a, axis=(i, j))` | `Tensor.sum_axes (Tensor.AxesOptions { axes: [i, j], keepdims: false }) v` |
| `np.prod` / `np.mean` / `np.all` / `np.any` | `Tensor.prod*` / `Tensor.mean*` / `Tensor.all*` / `Tensor.any*` |
| `np.average(a, weights=w, axis=k)` | `Tensor.average w v` / `Tensor.average_axis k w v` |
| `np.var(a, axis=k, ddof=1)` / `np.std(...)` | `Tensor.var_with (Tensor.VarianceOptions { axis: Maybe.Some k, keepdims: false, ddof: 1 }) v` / `Tensor.std_with ...` |
| `np.min` / `np.max` / `np.argmin` / `np.argmax` / `np.ptp` | `Tensor.min*` / `Tensor.max*` / `Tensor.argmin*` / `Tensor.argmax*` / `Tensor.ptp*` |
| `np.count_nonzero(a, axis=k)` | `Tensor.count_nonzero v` / `Tensor.count_nonzero_axis k v` |
| `np.nansum` / `np.nanmean` / `np.nanvar` / `np.nanstd` / `np.nanmin` / `np.nanmax` / `np.nanargmin` / `np.nanargmax` | `Tensor.nansum*` / `Tensor.nanmean*` / `Tensor.nanvar*` / `Tensor.nanstd*` / `Tensor.nanmin*` / `Tensor.nanmax*` / `Tensor.nanargmin*` / `Tensor.nanargmax*` |

## 累積・差分・数値積分・畳み込み・分位点・ヒストグラム・ソート・集合演算の対応

| NumPy | Tsuzuri (`Tensor`) |
| --- | --- |
| `np.cumsum(a, axis=k)` / `np.cumsum(a)` | `Tensor.cumsum k v` / `Tensor.cumsum_flat v`（`cumprod*`・`nancumsum`・`nancumprod` も同様） |
| `np.diff(a, n=1, axis=k)` / `np.ediff1d(a)` | `Tensor.diff k v` / `Tensor.diff_n n k v` / `Tensor.ediff1d v` |
| `np.gradient(a, h, axis=k)` | `Tensor.gradient h k v` / `Tensor.gradient_1d v` |
| `np.trapezoid(y, dx=h, axis=k)` / `np.trapezoid(y, x=x, axis=k)` | `Tensor.trapezoid h k y` / `Tensor.trapezoid_x k x y` |
| `np.convolve(a, v, mode)` / `np.correlate(a, v, mode)` | `Tensor.convolve mode a v` / `Tensor.correlate mode a v`（`Tensor.ConvolveMode.Full` / `Same` / `Valid`） |
| `np.interp(x, xp, fp, left, right)` | `Tensor.interp xp fp x` / `Tensor.interp_extrap left right xp fp x` |
| `np.median` / `np.quantile` / `np.percentile` / `np.nan*` | `Tensor.median*` / `Tensor.quantile`・`quantile_with`・`quantiles` / `Tensor.percentile*` / `Tensor.nanmedian*`・`nanquantile*`・`nanpercentile*` |
| `np.histogram` / `np.histogram2d` / `np.histogramdd` | `Tensor.histogram bins v`・`histogram_edges edges v`・`histogram_weighted edges w v` / `Tensor.histogram2d bx by x y` / `Tensor.histogramdd bins sample` |
| `np.bincount(x, weights, minlength)` / `np.digitize(x, bins, right)` | `Tensor.bincount minlength v`・`bincount_weighted minlength w v` / `Tensor.digitize right bins v` |
| `np.cov(m, rowvar, ddof)` / `np.corrcoef(m, rowvar)` | `Tensor.cov v`・`cov_with rowvar ddof v` / `Tensor.corrcoef v`・`corrcoef_with rowvar v`（1D 入力でも常に `[1, 1]` の 2D 行列を返す） |
| `np.sort` / `np.argsort` / `np.lexsort` | `Tensor.sort axis v`・`sort_flat v` / `Tensor.argsort axis v`・`argsort_flat v` / `Tensor.lexsort keys_2d`（`NaN` は末尾で安定） |
| `np.partition` / `np.argpartition` / `np.searchsorted` | `Tensor.partition kth axis v` / `Tensor.argpartition kth axis v` / `Tensor.searchsorted side sorted values` |
| `np.unique` / `np.isin` / `np.intersect1d` / `np.union1d` / `np.setdiff1d` / `np.setxor1d` | `Tensor.unique v`・`unique_full v` / `Tensor.isin test_set elems` / `Tensor.intersect1d a b` / `Tensor.union1d a b` / `Tensor.setdiff1d a b` / `Tensor.setxor1d a b` |

## 線形代数 (`numpy.linalg` / `einsum`) の対応

行列積演算子 `@` は存在しないため、[`Linalg`](./linalg.md) モジュールを使用します。数学的失敗（特異行列・非正定値・未収束）は例外ではなく `Result<_, Linalg.Error>` で返ります。

| NumPy (`np` / `np.linalg`) | Tsuzuri (`Linalg`) |
| --- | --- |
| `a @ b` / `np.matmul(a, b, out=dst)` | `Linalg.matmul va vb` / `Linalg.matmul_into va vb (ref mut dst)` / `Linalg.matmul_complex va vb` |
| `np.dot` / `np.vdot` / `np.inner` / `np.outer` / `np.kron` / `np.cross` | `Linalg.dot` / `Linalg.vdot`・`vdot_complex` / `Linalg.inner` / `Linalg.outer`・`outer_complex` / `Linalg.kron` / `Linalg.cross axis va vb` |
| `np.diagonal(a, offset, ax1, ax2)` / `np.trace` | `Linalg.diagonal_view offset ax1 ax2 v`・`Linalg.diagonal` / `Linalg.trace offset ax1 ax2 v` |
| `np.tensordot(a, b, axes)` / `np.einsum(subscripts, *ops)` | `Linalg.tensordot (ax_a, ax_b) va vb` / `Linalg.einsum spec [v1, v2, ...]`・`Linalg.einsum_path` |
| `scipy.linalg.lu` / `np.linalg.cholesky` / `np.linalg.qr` | `Linalg.lu v` / `Linalg.cholesky v` / `Linalg.qr mode v` |
| `np.linalg.solve(a, b)` / `np.linalg.lstsq(a, b)` | `Linalg.solve va vb`（`lu_solve`・`cholesky_solve`・`solve_triangular` も提供） / `Linalg.lstsq va vb` |
| `np.linalg.inv` / `np.linalg.det` / `np.linalg.slogdet` / `np.linalg.matrix_power(a, n)` | `Linalg.inv v` / `Linalg.det v` / `Linalg.slogdet v` / `Linalg.matrix_power n v` |
| `np.linalg.norm` / `np.linalg.vector_norm` / `np.linalg.matrix_norm` / `np.linalg.cond` | `Linalg.norm v` / `Linalg.vector_norm ord axis keepdims v` / `Linalg.matrix_norm ord v` / `Linalg.cond ord v` |
| `np.linalg.svd` / `np.linalg.svdvals` / `np.linalg.pinv` / `np.linalg.matrix_rank` | `Linalg.svd opts v` / `Linalg.svdvals v` / `Linalg.pinv rcond v` / `Linalg.matrix_rank tol v` |
| `np.linalg.eigh` / `np.linalg.eigvalsh` / `np.linalg.eig` / `np.linalg.eigvals` | `Linalg.eigh v`・`eigvalsh v`・`eigh_complex v`・`eigvalsh_complex v` / `Linalg.eig v`・`eigvals v` |

## 関連項目

- [Tensor](./tensor.md) — N 次元テンソル・窓・ブロードキャスト・統計・ソート API
- [Linalg](./linalg.md) — バッチ行列積・`einsum`・行列分解・連立方程式求解・固有値分解 API
- [Slice](./slice.md) — スライス記述子
- [引数の順序](./argument-order.md) — パイプライン向けの引数配置規則
- [言語リファレンスの目次](../index.md)
