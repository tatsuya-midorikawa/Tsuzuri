# NC09: 高速フーリエ変換 `Fft` と多項式 `Polynomial`

| 項目 | 内容 |
| --- | --- |
| ID | NC09 |
| 優先度 | P2（信号処理・スペクトル解析・多項式フィッティング） |
| 規模 | L（2 Phase: Phase 1 `std/Fft.tz`、Phase 2 `std/Polynomial.tz`） |
| 依存 | [NC01](_completed/NC01-complex-and-extended-math.md), [NC02](_completed/NC02-tensor-core-views-and-broadcasting.md), [NC08](NC08-linalg-core-and-einsum.md), ([NC15](_completed/NC15-parallel-borrowed-chunk-input.md)) |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-24（[GUIDE D-65](GUIDE.md#d-65-数値計算基盤と-numpy-互換機能案-c-ハイブリッドnc01nc14)。HEAD `27069465` で確認） |
| 承認 | 要承認（新規 opt-in std モジュール `Fft`・`Polynomial` の `RESERVED_MODULES` 登録。決定事項 D1 が要承認、ほかは既定案） |
| 手本にする既存実装 | `std/Complex.tz`（NC01）、`std/Tensor.tz`（NC02）、`std/Linalg.tz`（NC08） |
| 主な影響ファイル | `std/Fft.tz`（新規）、`std/Polynomial.tz`（新規）、`src/stdlib.rs`、`tests/fft.rs`（新規）、`tests/polynomial.rs`（新規）、`tests/std_argument_order.rs`、`tests/std_examples.rs`、`tests/features.mjs`、`tests/fixtures/effects_sweep/`、`tests/fixtures/`、`vsc/src/core.ts`、`_tsuzuri/language-reference/built-in-types-and-modules/fft.md`（新規）、`_tsuzuri/language-reference/built-in-types-and-modules/polynomial.md`（新規）、`_tsuzuri/language-reference/index.md`、`docs/language.md` |

## 実装者向け早見

- 範囲:
  - **Phase 1（`std/Fft.tz`）**: 任意長 $N$ に対する $O(N \log N)$ 高速フーリエ変換（2 のべき乗は反復 Radix-2 Cooley-Tukey、一般合成数・素数長は Bluestein チャープ Z 変換）。`fft`, `ifft`, `rfft`, `irfft`, `hfft`, `ihfft`, `fft2`, `ifft2`, `fftn`, `ifftn`, `rfftn`, `irfftn`, `fftfreq`, `rfftfreq`, `fftshift`, `ifftshift`（正規化 `Backward | Ortho | Forward` 指定、軸指定対応）。1 回のバッチ／ND 呼び出し内では軸ごとに回転因子と Bluestein チャープ FFT を 1 回だけ事前計算して共有し、複数回の呼び出しを跨ぐ反復用途向けに不変計画 `Fft.Plan<'a>`（`plan` / `execute` / `execute_into`）を提供する。Phase 1b（実測ベース）として混合基数 `{2, 3, 5}`（直接変換および Bluestein 内部畳み込みの滑らかな長さ選択）と、[NC15](_completed/NC15-parallel-borrowed-chunk-input.md) 完了後の並列バッチ/ND FFT（`fft_parallel` / `fft2_parallel`）を段階追加する。
  - **Phase 2（`std/Polynomial.tz`）**: 標準べき基底（および Chebyshev・Legendre 基底）の多項式演算 `eval`（Horner 法）, `add`, `sub`, `mul`, `divmod`, `deriv`, `integ`, `fit`（Vandermonde + `Linalg.lstsq`）, `roots`（コンパニオン行列の `Linalg.eigvals`）。
- 最初に読む: ① [NC01](_completed/NC01-complex-and-extended-math.md) の `Complex` 仕様 ② [NC08](NC08-linalg-core-and-einsum.md) の `lstsq`・`eigvals` ③ 仕様 1〜2。
- 最初に書くテスト: `tests/fft.rs`（素朴 DFT 参照実装との比較: 長さ 1, 2, 7, 8, 15, 16, 100、Parseval の等式、`ifft (fft x) ≈ x`、`irfft (rfft x) ≈ x`、2D/3D）と `tests/polynomial.rs`（`fit`・`roots`・`deriv`/`integ`）。
- 触ってはいけない: `Fft`・`Polynomial` を名指さないプログラムの生成 IR（opt-in 維持）。
- 完了の見分け方: 任意長の 1D/ND 実数・複素数 FFT と多項式フィッティング／求根が native と WASM の `-O0`・`-O3` で参照値と一致する。

## 目的

NumPy の中核サブモジュールである `numpy.fft` と `numpy.polynomial` に相当する機能を、外部 C ライブラリ非依存（WASM 含む全ターゲット対応）の純 Tsuzuri 標準モジュールとして提供する。

## 着手条件と停止条件

### 着手条件

- Phase 1 は [NC01](_completed/NC01-complex-and-extended-math.md) と [NC02](_completed/NC02-tensor-core-views-and-broadcasting.md) が done。
- Phase 2 は [NC08](NC08-linalg-core-and-einsum.md)（Phase 2 の `lstsq` と Phase 3 の `eigvals`）が done。
- 決定事項 D1 の承認。

### 停止条件

- **S1** Bluestein アルゴリズムのパディング長や回転因子の精度誤差が $O(N \epsilon)$ を大きく超える場合、回転因子計算を `Math.cos`/`Math.sin` の直接角度計算（反復乗算による誤差蓄積を避ける）に統一する。
- **S2** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `27069465` で確認）

- FFT・多項式モジュールは未存在、`RESERVED_MODULES` に未登録。

## 変えてはいけない意味

- 決定論的演算（暗黙 FMA・`fast-math` なし）、D-59（対象テンソルは最後の引数）。

## 仕様

### 1. Phase 1: `std/Fft.tz`（opt-in）

- `OPT_IN` 登録: `uses: &["Complex", "Tensor"]`（`std/Fft.tz` 自身が直接名指しする opt-in モジュールのみ。推移閉包は `sources_for` が自動解決する）。`RESERVED_MODULES` に `"Fft"` を追加し `src/stdlib.rs:843` の件数アサーションを更新する。`tests/std_argument_order.rs` の `MODULES` には `("Fft", subject(&["Tensor.View", "Tensor"]))` として登録。
- 正規化モード: `union Norm = Backward | Ortho | Forward`（既定 `Backward` は NumPy と同じく順変換 $1$・逆変換 $1/N$）。
- 1D 変換（最終軸または `*_axis` で軸指定、`*_with` で型名付き設定レコード `Fft.Options { axis: ..., n: ..., norm: ... }` 指定。回転因子に `Math.cos`/`Math.sin` を使うため制約は **`Elementary<'a>`** = `f32`/`f64`）:
  - `fft :: Elementary<'a> => ref Tensor.View<Complex<'a>> -> Result<Tensor<Complex<'a>>, Error>`、`ifft`
  - `rfft :: Elementary<'a> => ref Tensor.View<'a> -> Result<Tensor<Complex<'a>>, Error>`（出力長 `n / 2 + 1`）、`irfft`、`irfft_len`（出力長明示）
  - `hfft`, `ihfft`（エルミート対称入力向け）
- 2D / N 次元変換:
  - `fft2`, `ifft2`, `rfft2`, `irfft2`, `fftn`, `ifftn`, `rfftn`, `irfftn`（指定軸を順に 1D 変換する分離可能実装）
- 補助関数:
  - `fftfreq :: own f64 -> own i64 -> Tensor<f64>`（`d`, `n`）、`rfftfreq`
  - `fftshift`, `ifftshift`（ゼロ周波数成分の中央シフト・逆シフト）
- 呼び出し跨ぎの反復実行（不変計画 `Fft.Plan<'a>`、`pure def`・`Sync`）:
  - Tsuzuri には可変モジュールグローバルがなく（`values.md`）、`SyncMap`/`OnceValue`/`RwMutex` は `Category::State`（`pure def` 不可 `E1029`・`live == 0` 衝突）となるため、隠れた内部キャッシュではなく明示的な不変レコード **`Fft.Plan<'a>`**（回転因子と Bluestein フィルタ FFT を保持）を提供する。
  - `plan :: Elementary<'a> => own i64 -> own Direction -> Result<Plan<'a>, Error>`、`execute :: Elementary<'a> => ref Plan<'a> -> ref Tensor.View<Complex<'a>> -> Result<Tensor<Complex<'a>>, Error>`、`execute_into`（呼び出し側が排他出力と同時実行ごとの作業バッファを渡すことで実行中の追加確保を 0 にし、不変 `Plan` を複数スレッドで安全に共有できる）。
- 長さ 0 の軸は `Result.Error (EmptyAxis axis)` を返す（D2）。
- Phase 1b（実測による段階追加）:
  - 混合基数 `{2, 3, 5}` バタフライを追加し、合成数長（例: `1000`）を直接処理するとともに、素数・大因数長の Bluestein 内部畳み込み長を最小 2 べき乗ではなく最小 `{2, 3, 5}` 滑らか数（例: $N = 1031$ に対し最小 $2N-1 = 2061$ を満たす $2160 = 2^4 \cdot 3^3 \cdot 5$、2 べきの $4096$ の約半分）から選ぶ（要求された変換長 $N$ 自体は変更しない）。
  - [NC15](_completed/NC15-parallel-borrowed-chunk-input.md) 完了後、独立したバッチ／2D/ND スライスを `Parallel.for_each_chunk_with` で並列化する `fft_parallel`・`fft2_parallel`（非 `pure` の `Category::Task`、直列とビット完全一致）を追加する。

### 2. Phase 2: `std/Polynomial.tz`（opt-in）

- `OPT_IN` 登録: `uses: &["Complex", "Tensor", "Linalg"]`（ソース自身が直接参照する opt-in モジュールのみ）。`RESERVED_MODULES` に `"Polynomial"` を追加し `src/stdlib.rs:843` の件数アサーションを更新する。`tests/std_argument_order.rs` の `MODULES` には `("Polynomial", subject(&["Tensor.View", "Tensor", "[]"]))` として登録。
- 係数配列は低次から高次順（`c[0] + c[1]*x + c[2]*x^2 + ...`。現代的な `numpy.polynomial.Polynomial` 規約。旧 `np.polyval` の逆順とは明確に区別）。
- べき基底 API: `eval`, `eval_tensor`, `add`, `sub`, `mul`, `divmod`, `pow`, `deriv`, `integ`, `fit`（重み付き最小二乗対応）, `roots`（先頭 0 除去後にコンパニオン行列の `Linalg.eigvals` で計算）, `from_roots`。
- 直交多項式（Chebyshev・Legendre）: `cheb_eval`, `cheb_fit`, `leg_eval`, `leg_fit`（Clenshaw 漸化式で数値安定に評価）。

## 設計

- `fft_1d` は Phase 1a では 2 のべき乗を反復ビット反転 Radix-2 Cooley-Tukey、それ以外を長さ $M \ge 2N - 1$ の最小 2 べき乗への Bluestein 畳み込みで $O(N \log N)$ を保証する。バッチ／`fft2`/`fftn` の 1 回の呼び出し内では、**軸ごとにスライスループの外で 1 回だけ回転因子と Bluestein フィルタ FFT を計算して共有する**（`pure def` のまま反復準備コストを削減）。
- **Bluestein の `f32` 位相精度対策**: チャープ係数 $e^{\pm i \pi k^2 / N}$ の $k^2$ を `f32` で計算すると $k > 4096$（$k^2 > 2^{24}$）で仮数部欠落により位相が破綻するため、必ず `i64`（大きな $N$ での乗算溢れを避けるため `i128` 経由）で **`(k * k) % (2 * n)`** を厳密計算してから `'a` へ変換し `Math.cos`/`Math.sin` へ渡す。
- `rfft` は実数長 $2K$ を長さ $K$ の複素 FFT へパックして半分の計算量で解く最適化（奇数長は直接計算または複素昇格）を採用する。
- Phase 1b の混合基数 `{2, 3, 5}` は独立に実測して採用する。Stockham 往復バッファや実部・虚部分離（SoA）はコピー／作業領域のトレードオフが大きいため混合基数と混ぜて導入せず、個別の同条件計測（NumPy `out=`・SciPy `workers` 比較）で明確な改善が確認された場合のみ検討する。
- 追加した全公開関数を `tests/fixtures/effects_sweep/` で網羅呼び出しする。
- 将来 [NC12](NC12-ffi-numeric-buffers-and-blas-lapack.md) 経由で FFTW 等を呼ぶ場合も `Fft` の純 Tsuzuri 実装が参照実装・WASM バックエンドとなる。

## 前提とする他チケットのインターフェース

- [NC01](_completed/NC01-complex-and-extended-math.md): `Complex<'a>`, `Math.cos`, `Math.sin`, `Math.pi`。
- [NC02](_completed/NC02-tensor-core-views-and-broadcasting.md): `Tensor`, `Tensor.View`, `Tensor.Mut`。
- [NC08](NC08-linalg-core-and-einsum.md): `Linalg.lstsq`, `Linalg.eigvals`（Phase 2 のみ）。
- 弱い依存 [NC15](_completed/NC15-parallel-borrowed-chunk-input.md): `Parallel.for_each_chunk_with`（Phase 1b の並列バッチ/ND FFT 用）。

## 他チケットへ提供するインターフェース

- `Fft.*`, `Polynomial.*` 公開 API。

## 実装手順

1. **変更**: `std/Fft.tz`, `src/stdlib.rs`, `tests/fft.rs`, `tests/std_argument_order.rs`, `tests/std_examples.rs`, `tests/fixtures/effects_sweep/`, `vsc/src/core.ts`。**内容**: Phase 1（1D/2D/ND FFT・`rfft`・`fftfreq`・`fftshift`）を実装する。**確認**: T0、`tests/fft.rs`（素朴 DFT 照合＋Parseval＋往復）、`cargo test --locked --test std_argument_order --test std_examples --test effects_alloc_conservative`、`tsuzuri test --doc std`、T1。
2. **変更**: `std/Polynomial.tz`, `src/stdlib.rs`, `tests/polynomial.rs`, `tests/std_argument_order.rs`, `tests/std_examples.rs`, `tests/fixtures/effects_sweep/`, `vsc/src/core.ts`。**内容**: Phase 2（多項式演算・`fit`・`roots`・Chebyshev/Legendre）を実装する。**確認**: T0、`tests/polynomial.rs`、`effects_alloc_conservative`、`tsuzuri test --doc std`、T1。
3. **変更**: `tests/fixtures/`・`tests/features.mjs`、文書。**内容**: native/WASM `-O0`/`-O3` E2E とドキュメント更新。**確認**: T2、`node scripts/check-docs.mjs`。

## テスト計画

- **FFT テスト（`tests/fft.rs`）**: 長さ $1, 2, 3, 4, 7, 8, 12, 15, 16, 17, 64, 100$ の素朴 $O(N^2)$ DFT との誤差比較、`Norm` 3 種、`rfft`/`irfft` の偶数長・奇数長往復、`fft2`/`fftn`、`fftfreq`/`rfftfreq`/`fftshift`/`ifftshift`、長さ 0 軸の `Result.Error`。
- **多項式テスト（`tests/polynomial.rs`）**: `from_roots` → `roots` 往復（実根・複素共役根・重根）、`deriv` ↔ `integ`、`fit` の雑音なし完全復元、Chebyshev/Legendre の解析式照合。

## ドキュメント

- `_tsuzuri/language-reference/built-in-types-and-modules/fft.md`（新規）・`polynomial.md`（新規）・`index.md`
- `docs/language.md`

## 受け入れ条件

- 素数長を含む任意長の FFT が $O(N \log N)$ で動作し、素朴 DFT と誤差許容範囲内で一致すること。
- native / WASM `-O0`・`-O3` と `tsuzuri test --doc std`・`effects_alloc_conservative`・T2 が通ること。

## 落とし穴

- `fft`/`ifft` に `Float<'a>` を付けると、内部の回転因子 `Math.cos`/`Math.sin`（`Elementary<'a>` 要求）で `E1005` になる。必ず `Elementary<'a>` にする。
- Bluestein のチャープ位相で `k * k` を `f32` のまま計算すると $k > 4096$ で精度が崩壊する。必ず整数で `(k * k) % (2 * n)` を計算してから浮動小数点へ変換する。
- 呼び出しを跨ぐ計画の再利用に隠れたグローバルキャッシュ（`SyncMap`/`RwMutex` 等）を使うと `Category::State` により `pure def` 不可（`E1029`）かつ `live == 0` 検証と衝突する。必ず不変レコード `Fft.Plan<'a>` として明示的に受け渡すこと。
- `OPT_IN.uses`（`src/stdlib.rs:244, 556, 735`）は「そのモジュール自身のソースが直接参照する opt-in モジュール」との完全一致検査であり、推移閉包は `sources_for` がロード時に自動計算する。ソース内で直接書かない `Matrix`・`MatrixView` 等を `uses` に入れると `opt_in_modules_are_reached_only_through_their_names` が失敗する。また `src/stdlib.rs:843` の `RESERVED_MODULES.len()` アサーション更新も忘れないこと。

## 対象外

- 外部 FFTW / Accelerate vDSP 連携（[NC12](NC12-ffi-numeric-buffers-and-blas-lapack.md) の C FFI バッファ基盤で外部接続可能）。
- 窓関数・デジタルフィルタ設計・STFT（SciPy `scipy.signal` 領域）。

## 決定事項

- **D1（要承認）**: opt-in std モジュール `std/Fft.tz`・`std/Polynomial.tz` を追加し、`RESERVED_MODULES` に `"Fft"`・`"Polynomial"` を登録する（`D-55` へ反映）。
- **D2（既定案）**: 長さ 0 の変換軸は NumPy と同じくエラーとし、`Result.Error (EmptyAxis axis)` で安全に返す。
- **D3（既定案）**: `Polynomial` の係数順は NumPy 現行標準（`numpy.polynomial`）の昇べき順（`c[0]` が定数項）を採用する。
