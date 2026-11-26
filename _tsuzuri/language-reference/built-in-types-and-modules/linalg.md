# Linalg

`Linalg` は、N 次元テンソル [`Tensor`](./tensor.md) の上に構築された線形代数・テンソル縮約の opt-in 標準モジュールです。外部の C ライブラリや BLAS/LAPACK に依存せず、ネイティブ（Windows・Linux・macOS）と WebAssembly のすべてのターゲットで同一の決定論的アルゴリズム（暗黙 FMA・`fast-math` なし）で動作します。

プログラム内で `Linalg` の名前を参照したときだけ読み込まれ（`Tensor` と `Complex` を併用します）、参照しないプログラムの生成 IR には影響しません。また、Tsuzuri の `@attribute` 構文との衝突を避けるため行列積演算子 `@` は設けず、`Linalg.matmul` と `Linalg.einsum` を使用します。

## 引数の順序

[標準ライブラリの引数の順序](./argument-order.md)に従い、設定値・モード・軸番号・添字文字列は先に取り、操作対象の `Tensor.View`（または書き込み先の `Tensor.Mut`）を最後の引数に取ります。2 つのテンソルを取る演算（`matmul`・`solve`・`lstsq` など）は数式の順（左辺 `A`、右辺 `B`）に並べ、`matmul_into` では書き込み先の `out` が最後です。

## エラー型と設定型

数学的・形状的な失敗（特異行列・非正定値・反復未収束・不正な `einsum` 添字など）はトラップではなく、`Result<_, Linalg.Error>` で回復可能に返します。

```text
union Error =
    | ShapeMismatch of ([i64] * [i64])
    | NotSquare of [i64]
    | SingularMatrix
    | NotPositiveDefinite
    | DidNotConverge
    | InvalidEinsum of string
    | InvalidRank of i64

union QrMode = Reduced | Complete
union Triangle = Lower | Upper
union VectorNorm = L1 | L2 | Inf | NegInf | P of f64
union MatrixNorm = Frobenius | One | Infinity | Nuclear | Spectral
record SvdOptions { full_matrices: bool }
record LstsqResult<'a> { solution: Tensor<'a>, residuals: Tensor<'a>, rank: Tensor<i64>, singular_values: Tensor<'a> }
```

## テンソル積・縮約・`einsum`（Phase 1）

| 分類 | 関数一覧 | 説明 |
| --- | --- | --- |
| 行列積・内積 | `matmul`・`matmul_into`・`matmul_complex`、`dot`、`vdot`・`vdot_complex`、`inner` | 1D×1D 内積、2D×2D 行列積、1D/ND ベクトル積、3D 以上の先頭バッチ軸ブロードキャスト積。`A` が C 連続で `B` がバッチ共通のときは 2D 行結合高速経路を使用。`vdot_complex` は第 1 引数を複素共役化 |
| 外積・Kronecker 積・外積ベクトル | `outer`・`outer_complex`、`kron`、`cross` | 2D 外積、任意ランク Kronecker 積、軸に沿った長さ 2/3 ベクトルのクロス積 |
| 対角・トレース | `diagonal_view`・`diagonal`・`trace` | `offset` と 2 軸 `(axis1, axis2)` を指定したゼロコピー対角窓・コピー抽出・対角和 |
| 一般縮約・アインシュタイン縮約 | `tensordot`・`tensordot_complex`、`einsum_path`・`einsum`・`einsum_complex` | 明示出力 `->`・暗黙出力・省略記号 `...`・対角・単項和に対応し、3 入力以上では中間サイズ最小の 2 項縮約木（`einsum_path`）から `matmul` へ帰着 |

```tsuzuri edition=2028 run=%5B19%2C%2022%2C%2043%2C%2050%5D%20%5B15%5D
let a = Tensor.of_array [2, 2] [1, 2, 3, 4]
let b = Tensor.of_array [2, 2] [5, 6, 7, 8]
let sq = Tensor.of_array [3, 3] [1, 2, 3, 4, 5, 6, 7, 8, 9]
let va = Tensor.view (ref a)
let vb = Tensor.view (ref b)
let vsq = Tensor.view (ref sq)
let ab = match Linalg.matmul (ref va) (ref vb) with | Result.Ok c -> Tensor.to_array c | Result.Error _ -> []
let tr = match Linalg.einsum "ii->" [vsq] with | Result.Ok t -> Tensor.to_array t | Result.Error _ -> []
$"{ab} {tr}"
```

## 行列分解・連立方程式求解・逆行列・行列式・ノルム（Phase 2）

実数分解・求解は初等関数を用いるため `Elementary<'a>`（`f32` / `f64`）を対象とし、すべて先頭バッチ軸 `[..., m, n]` に対応します。

| 分類 | 関数一覧 | 説明 |
| --- | --- | --- |
| 基本分解 | `lu`、`cholesky`、`qr` | 部分ピボット付き `A = P * L * U`（`(P, L, U, pivots)`）、正定値対称下三角 `cholesky`（非正定値は `NotPositiveDefinite`）、Householder `qr`（`Reduced` / `Complete`） |
| 求解と分解再利用 | `solve`、`solve_triangular`、`lu_solve`、`cholesky_solve`、`lstsq` | `solve` は 1D 右辺 `[n]` なら `[..., n]`、2D 以上 `[..., n, nrhs]` なら複数右辺として `[..., n, nrhs]` を返す（特異行列は `SingularMatrix`）。`lstsq` は SVD によるランク落ち対応最小二乗解 |
| 逆行列・行列式・べき | `inv`、`det`、`slogdet`、`matrix_power` | 逆行列、行列式、符号付き対数行列式 `(sign, logabsdet)`、正・0・負の整数べき |
| ノルム・条件数 | `norm`、`vector_norm`、`matrix_norm`、`cond` | オーバーフロー安全なスケーリング付きノルムと条件数 |

```tsuzuri edition=2028 run=%5B1%2C%202%5D%20%5B5%5D
let a = Tensor.of_array [2, 2] [2.0, 1.0, 1.0, 3.0]
let b = Tensor.of_array [2] [4.0, 7.0]
let va = Tensor.view (ref a)
let vb = Tensor.view (ref b)
let x = match Linalg.solve (ref va) (ref vb) with | Result.Ok s -> Tensor.to_array s | Result.Error _ -> []
let d = match Linalg.det (ref va) with | Result.Ok dt -> Tensor.to_array dt | Result.Error _ -> []
$"{x} {d}"
```

## 特異値分解・擬似逆行列・固有値分解（Phase 3）

すべての反復法に確定的な最大反復上限を設け、病的入力で未収束の場合は無限ループせず `Result.Error Linalg.Error.DidNotConverge` を返します。

| 分類 | 関数一覧 | 説明 |
| --- | --- | --- |
| 特異値分解・擬似逆行列・ランク | `svd`・`svdvals`、`pinv`、`matrix_rank` | 片側 Hestenes-Jacobi 法による高精度 SVD（`A = U * diag(S) * Vt`、特異値降順）、Moore-Penrose 擬似逆行列 `pinv rcond view`、数値ランク `matrix_rank tol view` |
| 実対称・複素エルミート固有値 | `eigh`・`eigvalsh`、`eigh_complex`・`eigvalsh_complex` | Householder 三重対角化（複素版は位相正規化付き）＋Wilkinson シフト暗黙 QL 反復。固有値は実数昇順、固有ベクトルは正規直交／ユニタリ |
| 実一般非対称固有値 | `eig`・`eigvals` | 上 Hessenberg 化＋Francis ダブルシフト実 Schur 分解＋複素後退代入。固有値・右固有ベクトルを `Tensor<Complex<'a>>` として返却 |

```tsuzuri edition=2028 run=true%20true
let sym = Tensor.of_array [2, 2] [2.0, 1.0, 1.0, 2.0]
let exp = Tensor.of_array [2] [1.0, 3.0]
let rot = Tensor.of_array [2, 2] [0.0, -1.0, 1.0, 0.0]
let vsym = Tensor.view (ref sym)
let vexp = Tensor.view (ref exp)
let vrot = Tensor.view (ref rot)
let sym_ok =
    match Linalg.eigvalsh (ref vsym) with
    | Result.Ok w ->
        let vw = Tensor.view (ref w)
        Tensor.allclose 1e-12 1e-12 false (ref vw) (ref vexp)
    | Result.Error _ -> false
let rot_ok =
    match Linalg.eigvals (ref vrot) with
    | Result.Ok cw ->
        let cwa = Tensor.as_array (ref cw)
        Math.abs cwa[0].re < 1e-12 && Math.abs (Math.abs cwa[0].im - 1.0) < 1e-12
    | Result.Error _ -> false
$"{sym_ok} {rot_ok}"
```

## 関連項目

- [Tensor](./tensor.md) — N 次元テンソルと軸縮約・統計・ソート
- [Complex](./complex.md) — 複素数型 `Complex<'a>`
- [NumPy からの移行ガイド](./numpy-migration.md) — `numpy.linalg` と `Linalg` の対応表
- [言語リファレンスの目次](../index.md)
