# NC08: 線形代数モジュール `Linalg` とテンソル縮約（`matmul`/`tensordot`/`einsum`/分解/求解）

| 項目 | 内容 |
| --- | --- |
| ID | NC08 |
| 優先度 | P1（科学技術計算・機械学習・最適化の中核） |
| 規模 | XL（3 Phase に分割: Phase 1 テンソル積・`einsum`、Phase 2 LU/Cholesky/QR/求解/逆行列/行列式/ノルム、Phase 3 SVD・固有値分解 `eigh`/`eig`） |
| 依存 | [NC01](_completed/NC01-complex-and-extended-math.md), [NC02](_completed/NC02-tensor-core-views-and-broadcasting.md), [NC05](_completed/NC05-tensor-shape-indexing-and-masking.md), [NC06](_completed/NC06-tensor-ufuncs-and-elementwise.md) |
| 後続 | [NC09](NC09-fft-and-polynomial.md), ([NC10](NC10-random-distributions-and-generators.md)), [NC12](NC12-ffi-numeric-buffers-and-blas-lapack.md), [NC13](NC13-tensor-simd-parallel-and-gpu-kernels.md) |
| 状態 | todo |
| 起票 | 2026-11-24（[GUIDE D-65](GUIDE.md#d-65-数値計算基盤と-numpy-互換機能案-c-ハイブリッドnc01nc14)。HEAD `27069465` で確認） |
| 承認 | 要承認（新規 opt-in std モジュール `Linalg` の `RESERVED_MODULES` 登録。決定事項 D1・D2 が要承認、ほかは既定案） |
| 手本にする既存実装 | `std/Matrix.tz`（`mul`/`mul_fma`/`mul_parallel` の決定論契約と i-k-j ループ）、`std/Tensor.tz`、`src/stdlib.rs`（`OPT_IN`） |
| 主な影響ファイル | `std/Linalg.tz`（新規）、`src/stdlib.rs`、`tests/linalg.rs`（新規）、`tests/std_argument_order.rs`、`tests/std_examples.rs`、`tests/features.mjs`、`tests/fixtures/effects_sweep/`、`tests/fixtures/`、`vsc/src/core.ts`、`_tsuzuri/language-reference/built-in-types-and-modules/linalg.md`（新規）、`_tsuzuri/language-reference/index.md`、`docs/language.md` |

## 実装者向け早見

- 範囲: opt-in の std モジュール `Linalg` を新設し、純 Tsuzuri による移植可能・決定論的な線形代数とテンソル縮約を提供する（BLAS/LAPACK 高速経路は [NC12](NC12-ffi-numeric-buffers-and-blas-lapack.md)、SIMD/GEMM 最適化は [NC13](NC13-tensor-simd-parallel-and-gpu-kernels.md) で追加）。
  - **Phase 1（積・縮約）**: バッチ対応 `matmul`（N 次元ブロードキャスト＋C 連続 `A` と共通 `B` の行結合高速経路）、`dot`, `vdot`, `inner`, `outer`, `kron`, `cross`, `trace`, `diagonal`, `tensordot`, `einsum`（文字列添字パーサ＋最適縮約順序計画 `einsum_path`）。
  - **Phase 2（分解・求解・基本指標）**: 部分ピボット付き LU 分解（`lu`）、`cholesky`（正定値対称）、Householder `qr`、`solve`（1D 単一右辺・2D 複数右辺）、分解結果の再利用求解（`lu_solve`, `cholesky_solve`, `solve_triangular`）、`lstsq`、`inv`、`det`、`slogdet`、`matrix_power`、`vector_norm` / `matrix_norm` / `norm`、`cond`。
  - **Phase 3（特異値・固有値）**: Golub-Kahan / Jacobi `svd`・`svdvals`、`pinv`、`matrix_rank`、実対称/エルミート固有値分解 `eigh`・`eigvalsh`、実非対称固有値分解 `eig`・`eigvals`（Schur / QR 反復、結果は `Complex<f64>` 返却）。
- 最初に読む: ① `std/Matrix.tz` の `mul` と精度・決定性契約 ② 仕様 1〜4 ③ `src/stdlib.rs` の `OPT_IN`。
- 最初に書くテスト: `tests/linalg.rs` でバッチ `matmul`・`einsum("bhqd,bhkd->bhqk", ...)`・`solve` の残差 `||Ax - b||` と特異行列の `Result.Error SingularMatrix`・`qr`/`cholesky`/`svd`/`eigh`/`eig` の再構成恒等式（`Q * R ≈ A`, `U * diag(S) * Vt ≈ A`, `A * v ≈ λ * v`）。
- 触ってはいけない: `Matrix.mul` の既存契約、`Linalg` を名指さないプログラムの生成 IR。行列積演算子 `@` は追加しない（D2）。
- 完了の見分け方: 3 Phase の全関数が独立参照（NumPy / SciPy 計算結果および数学恒等式）と一致し、WASM を含む全ターゲットで外部 C ライブラリなしに動作する。

## 目的

現状の Tsuzuri は 2D 行列積（`Matrix.mul`）のみを持ち、バッチ行列積・`tensordot`・`einsum`・連立方程式求解・行列分解（LU/Cholesky/QR/SVD/固有値）を持たない。まずは純 Tsuzuri で決定論的かつ WASM でも動く基準実装（リファレンス経路）を確立することで、科学技術計算の主要機能を提供しつつ、後続の BLAS/LAPACK 連携（[NC12](NC12-ffi-numeric-buffers-and-blas-lapack.md)）やネイティブ高速化（[NC13](NC13-tensor-simd-parallel-and-gpu-kernels.md)）の検証基準とする。

## 着手条件と停止条件

### 着手条件

- [NC01](_completed/NC01-complex-and-extended-math.md)・[NC02](_completed/NC02-tensor-core-views-and-broadcasting.md)・[NC05](_completed/NC05-tensor-shape-indexing-and-masking.md)・[NC06](_completed/NC06-tensor-ufuncs-and-elementwise.md) が done。
- 決定事項 D1・D2 の承認。

### 停止条件

- **S1** Phase 3 の一般非対称 `eig`（実 Schur 分解・Francis ダブルシフト QR）や `svd` で収束しない病的行例が出た場合、最大反復回数上限（例: `30 * n`）で無限ループせず必ず `Result.Error DidNotConverge` を返す設計を守る。無限ループや未収束の黙殺は禁止。
- **S2** `honors_the_exact_specialization_limit` に抵触する場合は、内部アルゴリズムを `f64` / `f32` / `Complex<f64>` の具象ヘルパに落として過剰なジェネリック特殊化を抑える。
- **S3** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `27069465` で確認）

- `std/Matrix.tz` に 2D の `mul`/`mul_fma`/`mul_parallel`/`mul_fma_parallel`、`std/Array.tz` に 1D の `dot`/`dot_fma` があるのみ。
- `Linalg` モジュールは未存在、`RESERVED_MODULES` に未登録。

## 変えてはいけない意味

- `Matrix.mul`・`Array.dot` のビット一致契約。
- 特異行列・非正定値行列・未収束といった数学的失敗をトラップで落とさず、`Result<_, Linalg.Error>` で回復可能に返すこと。

## 仕様

### 1. エラー型とモジュール登録

- `std/Linalg.tz`（opt-in。`OPT_IN` の `uses` は `std/Linalg.tz` 自身のソースが直接参照する opt-in モジュールのみを列挙する — 例: `uses: &["Tensor", "Complex"]`。`Matrix`/`MatrixView` は `std/Linalg.tz` 内で直接名指しする場合のみ含め、推移閉包として書かない。`RESERVED_MODULES` に `"Linalg"` を登録し `src/stdlib.rs:843` の件数アサーションを更新する）。
- `union Error = ShapeMismatch of ([i64] * [i64]) | NotSquare of [i64] | SingularMatrix | NotPositiveDefinite | DidNotConverge | InvalidEinsum of string | InvalidRank of i64`（`deriving (Eq)` + `Display`）。

### 2. Phase 1: テンソル積・縮約

- 実数・整数版（`Numeric<'a>`。`Complex<'a>` は組み込み `Numeric` 印を持たないため、複素数版は `matmul_complex` / `vdot_complex` / `tensordot_complex` / `einsum_complex` として明示提供する: D4）:
  - `matmul :: Numeric<'a> => ref Tensor.View<'a> -> ref Tensor.View<'a> -> Result<Tensor<'a>, Error>`（1D×1D=内積、2D×2D=行列積、1D×2D/2D×1D、3D 以上は先頭バッチ軸をブロードキャストし末尾 2 軸で行列積。`matmul_into` も提供）。
    - **共通 `B` の行結合高速経路**: `A` が C 連続の `[batch..., m, k]` であり、`B` が全バッチで共通（ランク 2 の `[k, n]`、またはブロードキャスト後のバッチ軸ストライドがすべて **`stride == 0`** の 3D 以上ビュー）かつ `k > 0` のときは、`A` をコピーなしで `[total_batch * m, k]` の 2D ビューへ畳んで単一の 2D 行列積として計算し、出力を `[batch..., m, n]` へ戻す（各出力要素の `k = 0 .. K-1` 昇順・別丸めを完全保存する。`k == 0` はゼロ埋め、`A` が非連続または `B` がバッチごとに異なる場合は通常のバッチループへ落とす）。
  - `dot`, `vdot`（実数版。複素数の共役内積は `vdot_complex`）, `inner`, `outer`, `kron`, `cross`（長さ 2/3 ベクトル、軸指定）
  - `trace`（オフセット・2 軸指定）, `diagonal`（ゼロコピービュー版あり）
  - `tensordot :: Numeric<'a> => own ([i64] * [i64]) -> ref Tensor.View<'a> -> ref Tensor.View<'a> -> Result<Tensor<'a>, Error>`
  - `einsum :: Numeric<'a> => ref string -> ref [Tensor.View<'a>] -> Result<Tensor<'a>, Error>`
    - 明示出力（`"ij,jk->ik"`, `"bhqd,bhkd->bhqk"`, `"ii->i"`, `"ij->ji"`）・暗黙出力・省略記号 `...` に対応。
    - 3 入力以上は貪欲法／動的計画法による中間サイズ最小の 2 項縮約木（`einsum_path`）を組み立て、`transpose` + `reshape` + `matmul` へ帰着させる。構文誤りや軸長不一致は `Result.Error (InvalidEinsum ...)`。

### 3. Phase 2: 分解・求解・逆行列・行列式・ノルム（`Elementary<'a>` = `f32`/`f64`、バッチ対応）

- 内部で `Math.hypot`・`Math.log`（`slogdet`）・`Math.pow`（`L_p` ノルム）を使用するため、実数版の制約は **`Elementary<'a>`**（`f32` / `f64`）とする（`Float<'a>` では `f16`/`f128`/`d32`〜`d128` に対して `E1005` になる）。
- `lu :: Elementary<'a> => ref Tensor.View<'a> -> Result<(Tensor<'a> * Tensor<'a> * Tensor<i64>), Error>`（部分ピボット付き `P, L, U`）
- `cholesky :: Elementary<'a> => ref Tensor.View<'a> -> Result<Tensor<'a>, Error>`（下三角 `L`、非正定値は `NotPositiveDefinite`）
- `qr :: Elementary<'a> => own QrMode -> ref Tensor.View<'a> -> Result<(Tensor<'a> * Tensor<'a>), Error>`（`Reduced | Complete`。Householder 反射）
- `solve :: Elementary<'a> => ref Tensor.View<'a> -> ref Tensor.View<'a> -> Result<Tensor<'a>, Error>`（`A X = B`、特異行列は `SingularMatrix`。NumPy 2.0+ 規約に揃え、`B` が 1D `[n]` のときのみ単一列ベクトルとして出力 `[..., n]`、2D 以上 `[..., n, nrhs]` のときは複数右辺行列として出力 `[..., n, nrhs]` とする）
- **分解結果の再利用求解（固定係数行列 `A` に対する反復求解で再分解を避ける薄いラッパー）**:
  - `solve_triangular :: Elementary<'a> => own Triangle -> own bool -> ref Tensor.View<'a> -> ref Tensor.View<'a> -> Result<Tensor<'a>, Error>`（`Upper | Lower`、`unit_diagonal` 指定の前進・後退代入）
  - `lu_solve :: Elementary<'a> => ref Tensor.View<'a> -> ref Tensor.View<'a> -> ref Tensor.View<i64> -> ref Tensor.View<'a> -> Result<Tensor<'a>, Error>`（`lu` が返した `L, U, pivots` を使い回して `B` を解く）
  - `cholesky_solve :: Elementary<'a> => ref Tensor.View<'a> -> ref Tensor.View<'a> -> Result<Tensor<'a>, Error>`（`cholesky` が返した下三角 `L` を使い回して $L L^T X = B$ を解く）
- `lstsq :: Elementary<'a> => ref Tensor.View<'a> -> ref Tensor.View<'a> -> Result<LstsqResult<'a>, Error>`（最小二乗解・残差・ランク・特異値。Phase 2 は QR ベース、Phase 3 で SVD ランク落ち対応へ強化）
- `inv`, `det`, `slogdet`（`(sign, logabsdet)`）, `matrix_power`（正・0・負の整数べき）
- `vector_norm`（`L1 | L2 | Inf | NegInf | P of f64`、軸指定）、`matrix_norm`（`Frobenius | L1 | Inf | Nuclear | Spectral`）、`cond`

### 4. Phase 3: SVD・擬似逆行列・固有値分解

- `svd :: Elementary<'a> => own SvdOptions -> ref Tensor.View<'a> -> Result<(Tensor<'a> * Tensor<'a> * Tensor<'a>), Error>`（`{ full_matrices: bool }`。Golub-Kahan 二重対角化 + 暗黙シフト QR または片側 Jacobi）
- `svdvals`, `pinv`（`rcond` 指定）、`matrix_rank`（`tol` 指定）
- `eigh`, `eigvalsh`（実対称行列: Householder 三重対角化 + 暗黙シフト QR。固有値昇順、固有ベクトル正規直交。エルミート複素行列版は `eigh_complex` / `eigvalsh_complex`）
- `eig`, `eigvals`（実一般正方行列: Hessenberg 化 + Francis ダブルシフト QR。固有値・固有ベクトルは `Tensor<Complex<'a>>` で返す）

## 設計

- 各 Phase を独立した 1 PR としてマージ可能にする（Phase 1 → Phase 2 → Phase 3）。
- `tests/std_argument_order.rs` の `MODULES` には `("Linalg", subject(&["Tensor.View", "Tensor.Mut", "Tensor"]))` として登録し、全公開関数を `tests/fixtures/effects_sweep/` で網羅呼び出しする。
- バッチ軸（先頭 `0 .. rank - 2`）を持つ入力は、先頭バッチを平坦化して 2D スライスごとに分解・求解を実行し元のバッチ形状へ戻す共通ラッパーで統一する。
- 数値演算の順序を固定し（暗黙 FMA・`fast-math` なし）、native と WASM の両方で外部依存ゼロで動くポータブル基準経路とする。

## 前提とする他チケットのインターフェース

- [NC01](_completed/NC01-complex-and-extended-math.md): `Complex<'a>`, `Math.hypot`/`Math.copysign`/`Math.sign`。
- [NC02](_completed/NC02-tensor-core-views-and-broadcasting.md): `Tensor.View`, `Tensor.Mut`, `broadcast_shape`。
- [NC05](_completed/NC05-tensor-shape-indexing-and-masking.md): `permute`, `swapaxes`, `reshape`。
- [NC06](_completed/NC06-tensor-ufuncs-and-elementwise.md): `allclose`, 要素ごと演算。

## 他チケットへ提供するインターフェース

- `Linalg` 全 API と `Linalg.Error`（[NC09](NC09-fft-and-polynomial.md) の多項式フィッティング・コンパニオン行列固有値根、[NC10](NC10-random-distributions-and-generators.md) の多変量正規分布 Cholesky 分解、[NC12](NC12-ffi-numeric-buffers-and-blas-lapack.md) の BLAS/LAPACK 対照検証、[NC13](NC13-tensor-simd-parallel-and-gpu-kernels.md) の高速 GEMM 検証が利用）。

## 実装手順

1. **変更**: `std/Linalg.tz`, `src/stdlib.rs`, `tests/std_argument_order.rs`, `tests/std_examples.rs`, `tests/fixtures/effects_sweep/`, `vsc/src/core.ts`。**内容**: Phase 1（`Linalg.Error`、`matmul`〜`einsum`・複素数版）を実装する。**確認**: T0、`tests/linalg.rs`（Phase 1）、`cargo test --locked --test std_argument_order --test std_examples --test effects_alloc_conservative`、`tsuzuri test --doc std`、T1。
2. **変更**: `std/Linalg.tz`, `tests/linalg.rs`, `tests/fixtures/effects_sweep/`。**内容**: Phase 2（`lu`, `cholesky`, `qr`, `solve`（1D/複数右辺 2D）, `solve_triangular`, `lu_solve`, `cholesky_solve`, `lstsq`, `inv`, `det`, `slogdet`, `matrix_power`, `norm`, `cond`）を実装する。**確認**: T0、各種恒等式・特異行列・非正定値・分解再利用求解テスト、`effects_alloc_conservative`、T1。
3. **変更**: `std/Linalg.tz`, `tests/linalg.rs`, `tests/fixtures/effects_sweep/`。**内容**: Phase 3（`svd`, `svdvals`, `pinv`, `matrix_rank`, `eigh`, `eigvalsh`, `eigh_complex`, `eig`, `eigvals`）を実装する。**確認**: T0、再構成誤差・直交性・複素固有値検証、`effects_alloc_conservative`、T1。
4. **変更**: `tests/fixtures/`・`tests/features.mjs`、文書。**内容**: native/WASM `-O0`/`-O3` E2E 検証とドキュメント更新。**確認**: T2、`node scripts/check-docs.mjs`。

## テスト計画

- **恒等式・精度検証（`tests/linalg.rs`）**:
  - `matmul`/`einsum` の全基本パターン（内積・外積・バッチ積・共通 `B` 行結合経路（ランク 2 `B` および `B[1, k, n]` の `stride == 0` ブロードキャスト・`k == 0`）・トレース・転置・対角抽出・省略記号 `...`・複素 `vdot_complex`/`matmul_complex`）と不正添字エラー。
  - 既知の解析解を持つテスト行列（ヒルベルト行列・直交行列・回転行列・対称行列・エルミート行列・欠陥/特異行列・長方形行列 $m > n$ および $m < n$）に対する再構成誤差（`||A - L L^T||`, `||Q^T Q - I||`, `||A x - b||`, `||U S V^T - A||`, `||A v - λ v||`）が $\epsilon \cdot n \cdot \|A\|$ のオーダーに収まること、`solve` の 1D/2D 複数右辺と `lu_solve`/`cholesky_solve`/`solve_triangular` の一致。
  - `SingularMatrix` / `NotPositiveDefinite` / `DidNotConverge` のエラー返却。
- **E2E（`tests/features.mjs`）**: native / WASM `-O0`・`-O3`、`live == 0`。

## ドキュメント

- `_tsuzuri/language-reference/built-in-types-and-modules/linalg.md`（新規）・`index.md`
- `docs/language.md`

## 受け入れ条件

- 外部 BLAS/LAPACK なしで native と WASM の両方で全 Phase が動作すること。
- 数学的失敗がトラップではなく `Result.Error` で返ること。
- `std_argument_order`・`std_examples`・`effects_alloc_conservative`・T2・`tsuzuri test --doc std`・`node scripts/check-docs.mjs` が通ること。

## 落とし穴

- `Float<'a>` には `f16`/`f128`/`d32`〜`d128` が含まれるため、内部で `Math.hypot` や `Math.log` を呼ぶ関数に `Float<'a>` を書くと `E1005` になる。必ず `Elementary<'a>` を使うこと。
- 組み込み印クラス `Numeric`/`Float`/`Elementary` は `Complex<'a>` に実装できないため、複素数線形代数は明示的に `*_complex` 関数で提供する（D4）。
- `OPT_IN.uses`（`src/stdlib.rs:244, 556, 735`）は「そのモジュール自身のソースが直接名指しする opt-in モジュール」との完全一致検査であり、推移閉包はローダが自動計算する。ソース内で直接参照しない `Matrix`/`MatrixView` 等を `uses` に入れると `opt_in_modules_are_reached_only_through_their_names` が失敗する。また `src/stdlib.rs:843` の `RESERVED_MODULES.len()` アサーション更新も忘れないこと。
- 固有値分解・SVD の QR 反復は収束しない病的入力で無限ループしないよう必ず最大反復上限を設け、`Result.Error DidNotConverge` を返すこと。

## 対象外

- 疎行列（CSR/CSC）および反復線形ソルバー（CG/GMRES 等）。
- 外部 BLAS/LAPACK リンク（[NC12](NC12-ffi-numeric-buffers-and-blas-lapack.md)）とキャッシュタイリング GEMM / GPU 行列積（[NC13](NC13-tensor-simd-parallel-and-gpu-kernels.md)）。
- 行列積演算子 `@`。

## 決定事項

- **D1（要承認）**: opt-in std モジュール `std/Linalg.tz` を新設し、`RESERVED_MODULES` に `"Linalg"` を登録する（`D-55` へ反映）。
- **D2（要承認）**: 行列積演算子 `@` は導入しない（Tsuzuri の `@attribute` 構文との衝突を避ける。案 C の方針）。`Linalg.matmul` と `Linalg.einsum` を使用する。
- **D3（既定案）**: 本チケットでは移植可能かつ決定論的な純 Tsuzuri 実装を基準経路として提供し、OpenBLAS/Accelerate/MKL 連携は [NC12](NC12-ffi-numeric-buffers-and-blas-lapack.md) で別名（`*_blas` 等）として明示分離する。
- **D4（既定案）**: 複素数向け線形代数は封印された印クラス `Numeric`/`Float` に乗せない設計とし、`matmul_complex`・`vdot_complex`・`eigh_complex`・`eigvalsh_complex` 等の明示関数群として提供する。
