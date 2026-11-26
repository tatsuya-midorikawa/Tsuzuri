# NC07: 軸指定の還元・統計・累積・ソート・探索・集合演算

| 項目 | 内容 |
| --- | --- |
| ID | NC07 |
| 優先度 | P1（データ分析・機械学習・科学計算の基本集計と順序操作） |
| 規模 | L（軸指定リダクション・`nan*` 版・累積/差分/補間/畳み込み・分位点/ヒストグラム/相関・軸ソート/探索/集合演算） |
| 依存 | [NC02](_completed/NC02-tensor-core-views-and-broadcasting.md), [NC05](_completed/NC05-tensor-shape-indexing-and-masking.md), [NC06](NC06-tensor-ufuncs-and-elementwise.md) |
| 後続 | [NC13](NC13-tensor-simd-parallel-and-gpu-kernels.md) |
| 状態 | todo |
| 起票 | 2026-11-24（[GUIDE D-65](GUIDE.md#d-65-数値計算基盤と-numpy-互換機能案-c-ハイブリッドnc01nc14)。HEAD `27069465` で確認） |
| 承認 | 不要（既定案のみ。新予約語・構文・モジュール追加なし。すべて `std/Tensor.tz` または新規 `std/Stats.tz` かは D1 で整理） |
| 手本にする既存実装 | `std/Array.tz`（`sum`/`sum_pairwise`/`sum_kahan`/`average`/`sort`/`binary_search`/`distinct`）、`std/Tensor.tz`（`fold`） |
| 主な影響ファイル | `std/Tensor.tz`、`tests/tensor_stats.rs`（新規）、`tests/std_argument_order.rs`、`tests/std_examples.rs`、`tests/features.mjs`、`tests/fixtures/effects_sweep/`、`tests/fixtures/tensor/`、`_tsuzuri/language-reference/built-in-types-and-modules/tensor.md`、`_tsuzuri/language-reference/built-in-types-and-modules/numpy-migration.md`（新規: NumPy 対応・差分表）、`_tsuzuri/language-reference/index.md` |

## 実装者向け早見

- 範囲: 多次元 `Tensor.View<'a>` に対する軸指定（単一軸・複数軸・全要素、`keepdims` 対応）の還元・統計・`NaN` 無視版・累積・差分/微積分/補間/畳み込み・分位点/ヒストグラム/共分散/相関係数・軸指定ソート/探索/集合演算を純 Tsuzuri で実装する。
- 最初に読む: ① 仕様 1〜5 ② `std/Array.tz` の `sum`/`sum_pairwise`/`sum_kahan`/`sort`/`binary_search` ③ `tests/std_argument_order.rs`。
- 最初に書くテスト: `tests/tensor_stats.rs` で 2D/3D の軸指定 `sum`/`mean`/`var`/`std`（`ddof=0` と `ddof=1`）・`keepdims`、`nanmean`/`nansum`、`min`/`max`/`argmin`/`argmax` の `NaN` 伝播、`percentile`/`histogram`/`cov`/`corrcoef`、`argsort`/`searchsorted`/`unique`。
- 触ってはいけない: 加算・還元順序の決定性（native/WASM・`-O0`/`-O3` でビット一致）。キーワード引数構文や匿名レコードリテラルの追加（最頻出の短縮関数 `*_axis`/`*_axis_keepdims` と、型名付き設定レコード `Tensor.AxisOptions { ... }` を併用する: D2）。
- 完了の見分け方: すべての集計・統計・ソート・探索結果が独立参照（Python/NumPy と同等の数式・定義）と一致し、native/WASM `-O0`/`-O3` で再現すること。

## 目的

現状は `Array` に 1D の `sum`/`average`/`min`/`max`/`sort` があるのみで、N 次元テンソルの軸に沿った集計（`axis`・`keepdims`）、`NaN` 無視統計、分位点、ヒストグラム、共分散、軸指定 `argsort`/`searchsorted`/`unique` が欠けている。これらを網羅的に整備し、データ集計・前処理ワークフローを Tsuzuri 内で完結できるようにする。

## 着手条件と停止条件

### 着手条件

- [NC02](_completed/NC02-tensor-core-views-and-broadcasting.md)・[NC05](_completed/NC05-tensor-shape-indexing-and-masking.md)・[NC06](NC06-tensor-ufuncs-and-elementwise.md) が done。

### 停止条件

- **S1** `std/Tensor.tz` のファイル規模・単相化予算が `honors_the_exact_specialization_limit`（GUIDE-LITE §10）に触れる兆候が出た場合は、統計・信号処理系（`histogram`/`cov`/`corrcoef`/`convolve`/`interp` 等）を別 opt-in モジュールへ分割する方針へ切り替える（D1。変更時は要承認）。
- **S2** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `27069465` で確認）

- `std/Tensor.tz` の集計は全要素を左から畳み込む `Tensor.fold` のみ。
- `std/Array.tz` の `binary_search` は一致検索（`Maybe<i64>`）のみで、挿入位置（`searchsorted` の left/right）や `argsort`・`partition` はない。

## 変えてはいけない意味

- `Array.sum` / `Array.sort` 等の既存 1D 契約。
- 浮動小数点還元の決定的な評価順序（添字昇順・`sum_pairwise` / `sum_kahan` の明示分離）。

## 仕様

### 1. 軸指定還元（短縮 API + 型名付き設定レコード）

- 設定レコード: `record AxisOptions { axis: i64, keepdims: bool }`、`record AxesOptions { axes: [i64], keepdims: bool }`、`record VarianceOptions { axis: Maybe<i64>, keepdims: bool, ddof: i64 }`（Tsuzuri には匿名レコードリテラルがないため、呼び出し時は `Tensor.AxisOptions { axis: 1, keepdims: true }` のように型名を添える）。
- 全要素版（スカラー返却）・単一軸版（`*_axis`、`keepdims` 付き短縮 `*_axis_keepdims`）・詳細設定版（`*_with`）:
  - `sum`, `sum_axis`, `sum_axis_keepdims`, `sum_with`（および高精度版 `sum_kahan`, `sum_kahan_axis`）
  - `prod`, `prod_axis`, `prod_with`
  - `mean`, `mean_axis`, `mean_axis_keepdims`, `mean_with`、`average`（重み付き平均）
  - `var`, `var_axis`, `var_with`、`std`, `std_axis`, `std_with`（Welford / 2 パス法で桁落ちを抑止し、`ddof` 指定対応。自由度 `n - ddof <= 0`（`ddof >= n`）のときは `NaN` を返す — NumPy の `inf` 返却との差分をリファレンスへ明記する）
  - `min`, `min_axis`, `min_with`、`max`, `max_axis`, `max_with`、`ptp`（`max - min`）。浮動小数点の通常版は `Math.min`/`Math.max` および NumPy と同じく `NaN` を伝播する。
  - `argmin`, `argmin_axis`, `argmax`, `argmax_axis`（先頭最小／最大の位置を `i64` で返す。空軸はトラップ。浮動小数点に `NaN` が含まれる場合は NumPy と同じく最初に出現する `NaN` の位置を返す）
  - `all`, `all_axis`, `any`, `any_axis`, `count_nonzero`, `count_nonzero_axis`
- **`NaN` 無視版（`Float<'a>`）**:
  - `nansum`, `nanprod`, `nanmean`, `nanvar`, `nanstd`, `nanmin`, `nanmax`, `nanargmin`, `nanargmax`（全要素版・軸指定版。全要素 `NaN` の `nanmean`/`nanvar`/`nanstd` は `NaN`、`nansum` は `0.0`、`nanprod` は `1.0`、`nanmin`/`nanmax` は `NaN`、`nanargmin`/`nanargmax` はトラップ）

### 2. 累積・差分・数値微積分・補間・1D 畳み込み

- `cumsum`, `cumsum_axis`, `cumprod`, `cumprod_axis`, `nancumsum`, `nancumprod`
- `diff`（`n` 階差分、軸指定）, `ediff1d`
- `gradient`（中心差分＋端点片側差分、等間隔／座標配列指定）
- `trapezoid`（台形則数値積分、軸指定）
- `interp`（1D 単調増加点列上の区分線形補間、左右外挿値指定）
- `convolve`, `correlate`（`Full | Same | Valid` モード）

### 3. 順序統計・ヒストグラム・共分散・相関

- `median`, `median_axis`, `nanmedian`
- `percentile`, `quantile`, `nanpercentile`, `nanquantile`（補間方法 `Linear | Lower | Higher | Midpoint | Nearest`）
- `histogram`（等幅ビン数または境界配列、戻り値は `(Tensor<i64> * Tensor<f64>)`）、`histogram2d`、`histogramdd`
- `bincount`（非負整数 1D 配列の頻度集計、重み付き版あり）、`digitize`（ビン番号割り当て）
- `cov`（共分散行列、`ddof`・`rowvar` 指定）、`corrcoef`（ピアソン相関係数行列）

### 4. ソート・部分ソート・探索・集合演算

- `sort`, `sort_axis`（安定ソート、`NaN` は末尾に配置して NumPy 互換順序を保証）
- `argsort`, `argsort_axis`（安定インデックスソート）
- `lexsort`（複数キー配列の辞書順安定ソート）
- `partition`, `argpartition`（Quickselect / Introselect 相当の $O(n)$ 平均計算量）
- `searchsorted :: Ord<'a> => own SearchSide -> ref View<'a> -> ref View<'a> -> Tensor<i64>`（`Left | Right`）
- `unique`（ソート済み一意値）、`unique_full`（値・初出位置 `index`・逆写像 `inverse`・出現回数 `counts` のレコード返却）
- 1D 集合演算: `isin`, `intersect1d`, `union1d`, `setdiff1d`, `setxor1d`

## 設計

- 軸指定還元は共通の「対象軸を末尾に転置またはストライド抽出し、1 本ずつ還元して出力座標に書き込む」ヘルパで実装し、コード重複と特殊化爆発を抑える。
- `var` / `std` は 1 パス `E[X^2] - E[X]^2` による桁落ちを避け、平均を求めた後の 2 パス法（`(x - mean)^2` の和）を標準実装とする（決定論的で高精度）。
- 浮動小数点ソートでは `NaN` を全有限値・`+Inf` より大きいものとして扱う全順序比較子を使い、`Ord` の部分順序による不定挙動を完全に防ぐ。

## 前提とする他チケットのインターフェース

- [NC02](_completed/NC02-tensor-core-views-and-broadcasting.md): `Tensor.View`, `Tensor.Mut`, `Tensor.Error`。
- [NC05](_completed/NC05-tensor-shape-indexing-and-masking.md): `normalize_axis`, `permute`, `squeeze`, `expand_dims`。
- [NC06](NC06-tensor-ufuncs-and-elementwise.md): `map2`, `is_nan`, 要素ごと演算。

## 他チケットへ提供するインターフェース

- 全集計・統計・ソート・探索 API（[NC13](NC13-tensor-simd-parallel-and-gpu-kernels.md) が SIMD/Parallel/GPU 最適化の基準参照実装として照合する）。

## 実装手順

1. **変更**: `std/Tensor.tz`。**内容**: 軸指定還元フレームワークと `sum`〜`count_nonzero` および全 `nan*` 版を実装する。**確認**: T0、`tests/tensor_stats.rs`（還元系）。
2. **変更**: `std/Tensor.tz`。**内容**: `cumsum`/`cumprod`/`diff`/`gradient`/`trapezoid`/`interp`/`convolve`/`correlate` を実装する。**確認**: T0、数値微積分・畳み込みテスト。
3. **変更**: `std/Tensor.tz`, `tests/fixtures/effects_sweep/`。**内容**: `median`/`percentile`/`quantile`/`histogram`/`bincount`/`digitize`/`cov`/`corrcoef` と、`sort`/`argsort`/`lexsort`/`partition`/`searchsorted`/`unique`/`isin`/集合演算、および全公開関数のコーパス網羅呼び出しを実装する。**確認**: T0、`honors_the_exact_specialization_limit`、`cargo test --locked --test std_argument_order --test std_examples --test effects_alloc_conservative`、`tsuzuri test --doc std`、T1。
4. **変更**: `tests/fixtures/`・`tests/features.mjs`、文書。**内容**: native/WASM `-O0`/`-O3` E2E 検証と文書更新。**確認**: T2、`node scripts/check-docs.mjs`。

## テスト計画

- **Rust テスト（`tests/tensor_stats.rs`）**: 軸 0/1/負軸/`keepdims`、空軸・1 要素軸・`NaN` 混入・全 `NaN`、大きいオフセットを持つデータの分散精度（例: `[1e9 + 1.0, 1e9 + 2.0, 1e9 + 3.0]` で `var == 2.0/3.0`）、`NaN` 混入ソート・`argsort` 安定性、`searchsorted` の `Left`/`Right` 重複境界、`unique_full` の往復再構築（`values[inverse] == input`）。
- **E2E（`tests/features.mjs`）**: native / WASM `-O0`・`-O3`、`live == 0`。

## ドキュメント

- `_tsuzuri/language-reference/built-in-types-and-modules/tensor.md`
- `_tsuzuri/language-reference/built-in-types-and-modules/numpy-migration.md`（新規。`index.md` へ追加）— NumPy との API 対応表および意図的な意味差（`remainder` vs `floor_mod`/`ieee_remainder`、`round` vs `round_even`、`sign(-0.0)`、`ddof >= n` の `NaN` vs `inf`、`==` のスカラー `bool` 契約、乱数列の非互換等）を一元記載する。

## 受け入れ条件

- すべての還元・統計・ソート・探索・集合 API が独立参照値と一致し、native/WASM `-O0`/`-O3` でビット一致すること。
- `honors_the_exact_specialization_limit`・`effects_alloc_conservative`・T2・`node scripts/check-docs.mjs` が通ること。

## 落とし穴

- 浮動小数点の通常 `Ord` 比較では `NaN < x` も `x < NaN` も `false` になるため、そのまま汎用ソートに渡すと順序の推移律が破れて不定になる。必ず全順序の比較ヘルパ（`NaN` を末尾へ寄せる）を使うこと。
- 軸指定還元で空軸（長さ 0 の軸）に対する `min`/`max`/`argmin`/`argmax` はトラップ、`sum` は `0`、`prod` は `1`、`all` は `true`、`any` は `false` という単位元規則を崩さないこと。また通常版 `min`/`max`/`argmin`/`argmax` は `NaN` を伝播・先頭 `NaN` 位置を返す契約とし、通常 `Ord` 比較で `NaN` を黙ってスキップしないこと。
- Tsuzuri には匿名レコードリテラルがなく `f { axis: 1, keepdims: true }` は `E1004` になる。`*_with` の doc-test とドキュメント例は必ず `Tensor.mean_with (Tensor.AxisOptions { axis: 1, keepdims: true }) v` のように型名付きで書くこと。

## 対象外

- 並列・GPU リダクションカーネル（[NC13](NC13-tensor-simd-parallel-and-gpu-kernels.md) が担当）。
- 多変量統計検定・回帰モデル（SciPy 領域）。

## 決定事項

- **D1（既定案）**: 既定では全関数を `std/Tensor.tz` に収める。単相化予算やファイル規模で分離が必要になった場合は、着手前または停止条件 S1 発生時に `std/Stats.tz` 分離の承認を仰ぐ。
- **D2（既定案）**: Tsuzuri には匿名レコードリテラルがないため、最頻出の呼び出しは短い専用関数（`Tensor.mean_axis 1 v`、`Tensor.mean_axis_keepdims 1 v`）を優先し、複数オプションを組み合わせる場面だけ型名付きの設定レコード（`Tensor.mean_with (Tensor.AxisOptions { axis: 1, keepdims: true }) v`）で提供する（案 C の方針）。
- **D3（既定案）**: 浮動小数点の `sort`/`argsort`/`median`/`percentile` では `NaN` を末尾へ寄せる全順序で統一し、通常版 `min`/`max`/`argmin`/`argmax` は `NaN` を伝播／先頭 `NaN` 位置返却とする（NumPy 仕様と整合）。`var`/`std` の `ddof >= n` は `NaN` とし、NumPy（`inf`）との差分を `numpy-migration.md` に明記する。
