# NC09: 高速フーリエ変換 `Fft` と多項式 `Polynomial`

| 項目 | 内容 |
| --- | --- |
| ID | NC09 |
| 優先度 | P2（信号処理・スペクトル解析・多項式フィッティング） |
| 規模 | L（2 Phase: Phase 1 `std/Fft.tz`、Phase 2 `std/Polynomial.tz`） |
| 依存 | [NC01](_completed/NC01-complex-and-extended-math.md), [NC02](_completed/NC02-tensor-core-views-and-broadcasting.md), [NC08](_completed/NC08-linalg-core-and-einsum.md), ([NC15](_completed/NC15-parallel-borrowed-chunk-input.md)) |
| 後続 | – |
| 状態 | done（Phase 1・1b・2。2026-11-28） |
| 起票 | 2026-11-24（[GUIDE D-65](GUIDE.md#d-65-数値計算基盤と-numpy-互換機能案-c-ハイブリッドnc01nc14)。HEAD `27069465` で確認） |
| 承認 | 承認済み（委任による決定、2026-11-28。新規 opt-in std モジュール `Fft`・`Polynomial` の `RESERVED_MODULES` 登録（D1）。起票時は要承認。ほかは既定案） |
| 手本にする既存実装 | `std/Complex.tz`（NC01）、`std/Tensor.tz`（NC02）、`std/Linalg.tz`（NC08） |
| 主な影響ファイル | `std/Fft.tz`（新規）、`std/Polynomial.tz`（新規）、`src/stdlib.rs`、`tests/fft.rs`（新規）、`tests/polynomial.rs`（新規）、`tests/std_argument_order.rs`、`tests/std_examples.rs`、`tests/features.mjs`、`tests/fixtures/effects_sweep/`、`tests/fixtures/`、`vsc/src/core.ts`、`_tsuzuri/language-reference/built-in-types-and-modules/fft.md`（新規）、`_tsuzuri/language-reference/built-in-types-and-modules/polynomial.md`（新規）、`_tsuzuri/language-reference/index.md`、`docs/language.md` |

## 実装者向け早見

- 範囲:
  - **Phase 1（`std/Fft.tz`）**: 任意長 $N$ に対する $O(N \log N)$ 高速フーリエ変換（2 のべき乗は反復 Radix-2 Cooley-Tukey、一般合成数・素数長は Bluestein チャープ Z 変換）。`fft`, `ifft`, `rfft`, `irfft`, `hfft`, `ihfft`, `fft2`, `ifft2`, `fftn`, `ifftn`, `rfftn`, `irfftn`, `fftfreq`, `rfftfreq`, `fftshift`, `ifftshift`（正規化 `Backward | Ortho | Forward` 指定、軸指定対応）。1 回のバッチ／ND 呼び出し内では軸ごとに回転因子と Bluestein チャープ FFT を 1 回だけ事前計算して共有し、複数回の呼び出しを跨ぐ反復用途向けに不変計画 `Fft.Plan<'a>`（`plan` / `execute` / `execute_into`）を提供する。Phase 1b（実測ベース）として混合基数 `{2, 3, 5}`（直接変換および Bluestein 内部畳み込みの滑らかな長さ選択）と、[NC15](_completed/NC15-parallel-borrowed-chunk-input.md) 完了後の並列バッチ/ND FFT（`fft_parallel` / `fft2_parallel`）を段階追加する。
  - **Phase 2（`std/Polynomial.tz`）**: 標準べき基底（および Chebyshev・Legendre 基底）の多項式演算 `eval`（Horner 法）, `add`, `sub`, `mul`, `divmod`, `deriv`, `integ`, `fit`（Vandermonde + `Linalg.lstsq`）, `roots`（コンパニオン行列の `Linalg.eigvals`）。
- 最初に読む: ① [NC01](_completed/NC01-complex-and-extended-math.md) の `Complex` 仕様 ② [NC08](_completed/NC08-linalg-core-and-einsum.md) の `lstsq`・`eigvals` ③ 仕様 1〜2。
- 最初に書くテスト: `tests/fft.rs`（素朴 DFT 参照実装との比較: 長さ 1, 2, 7, 8, 15, 16, 100、Parseval の等式、`ifft (fft x) ≈ x`、`irfft (rfft x) ≈ x`、2D/3D）と `tests/polynomial.rs`（`fit`・`roots`・`deriv`/`integ`）。
- 触ってはいけない: `Fft`・`Polynomial` を名指さないプログラムの生成 IR（opt-in 維持）。
- 完了の見分け方: 任意長の 1D/ND 実数・複素数 FFT と多項式フィッティング／求根が native と WASM の `-O0`・`-O3` で参照値と一致する。

## 目的

NumPy の中核サブモジュールである `numpy.fft` と `numpy.polynomial` に相当する機能を、外部 C ライブラリ非依存（WASM 含む全ターゲット対応）の純 Tsuzuri 標準モジュールとして提供する。

## 着手条件と停止条件

### 着手条件

- Phase 1 は [NC01](_completed/NC01-complex-and-extended-math.md) と [NC02](_completed/NC02-tensor-core-views-and-broadcasting.md) が done。
- Phase 2 は [NC08](_completed/NC08-linalg-core-and-einsum.md)（Phase 2 の `lstsq` と Phase 3 の `eigvals`）が done。
- 決定事項 D1 の承認。

### 停止条件

- **S1** Bluestein アルゴリズムのパディング長や回転因子の精度誤差が $O(N \epsilon)$ を大きく超える場合、回転因子計算を `Math.cos`/`Math.sin` の直接角度計算（反復乗算による誤差蓄積を避ける）に統一する。
- **S2** GUIDE-LITE §9 の共通の条件。

## 方針の最新化（2026-11-28）

起票（HEAD `27069465`、2026-11-24）の後に A37・A38・NC16〜NC21・NS07・edition 2028 の範囲・M 系が入った。着手の前に HEAD `b72bccd0` と照らし、下のとおり方針を改めた。下の表が、この文書のほかの節より優先する。利用者は「現在の最新仕様と齟齬がある内容がある場合には、実装前に方針を再検討して最新化して」「判断が必要なものは最高の選択肢で実装することを常に許可」と指示しており、決定はその委任による。

| # | チケットの記述 | HEAD の事実 | 改めた方針 |
| --- | --- | --- | --- |
| U1 | 文書は `_tsuzuri/language-reference/built-in-types-and-modules/fft.md`・`polynomial.md`・`index.md` | 利用者向けの日本語リファレンスは別リポジトリ `Tsuzuri-Reference` の `docs/stdlib/...` へ移った（PR #147）。`_tsuzuri/` はもうない | ページは `Tsuzuri-Reference` の `docs/stdlib/numeric/fft.md`・`polynomial.md`（新規）。`docs/index.md`・`docs/stdlib/conventions.md`・`docs/stdlib/numeric/tensor.md` の関連箇所を直す。`scripts/check-cli-docs.mjs` の `STD_PAGES` に 2 行足す（既定の `stdlib/fft.md` ではないため）。このリポジトリは `docs/language.md` に `### Fft`・`### Polynomial` を足す |
| U2 | `src/stdlib.rs:843` の件数、`uses` の行番号 | `RESERVED_MODULES` は 83 個で、検査は `reserves_the_d07_table`（行番号は変わった） | 現在値から +2（`Fft`・`Polynomial`）。行番号は使わない |
| U3 | `plan :: own i64 -> own Direction -> …` の `Direction` と、`Norm = Backward \| Ortho \| Forward` | 1 つのモジュールの 2 つの共用体は同じケース名を持てない（`E1001: duplicate or reserved name 'Forward'`） | `union Direction = Direct \| Inverse`（順変換・逆変換）。`Norm` は NumPy の名前のまま |
| U4 | `execute_into`（排他出力と作業バッファを渡して確保 0 回） | `Tensor.Mut` は `Tensor` の外から形・ストライドを読めず（`freeze` は確保する）、`pure def` は `ref mut` を取れない（`E1029`） | `execute_into :: ref Plan<'a> -> ref [Complex<'a>] -> ref mut [Complex<'a>..] -> ref mut [Complex<'a>..] -> Result<unit, Error>`（入力・作業・出力の順。書き込む出力が最後: D-59）と、その場で変換する `execute_in_place :: ref Plan<'a> -> ref mut [Complex<'a>..] -> ref mut [Complex<'a>..] -> Result<unit, Error>`（作業・データの順）を、排他スライスの通常の `def` にする。`Parallel.for_each_chunk_with` のチャンクと `Tensor.as_slice_mut` がそのまま渡せる。作業バッファの長さは `plan_work_length`。`plan`・`plan_with`・`execute` と、`ref mut` を取らない変換はすべて `pure def` |
| U5 | 2 のべき乗は反復 Radix-2、Phase 1b で混合基数を「段階追加」 | 利用者は全 Phase の完遂を指示した | 核を 1 つにする: 桁反転の置換と段ごとのバタフライを持つ反復の混合基数 DIT（基数 4・2・3・5）。基数が 2 だけならそのまま反復 Radix-2 Cooley-Tukey なので、Phase 1a の経路は因数分解の方針を変えた同じ核で測る。基数 4 の採否と、Bluestein の畳み込み長（2 のべき乗か `{2, 3, 5}` 滑らか数か）は release ビルドの同条件の計測で決め、数値を「実装と検証」に書く |
| U6 | 回転因子は `'a` の `Math.cos`／`Math.sin`、S1 で直接角度計算へ統一 | `Math.cos`／`Math.sin` の `f64` は native・WASM・`-O0`・`-O3` でビットが同じ。`f64 as 'a` は 1 回の正しい丸め | 最初から直接角度で作る。1 の $n$ 乗根は整数で 8 分円へ畳んでから `f64` で `Math.cos`／`Math.sin` を 1 回ずつ呼び、`'a` へ丸める（`f32` でも位相の誤差は `f64` の 1 ulp と 1 回の丸めだけ。$k = n/4$ などの軸上の値は厳密に 0・±1）。反復乗算は使わない。Bluestein の $k^2 \bmod 2n$ は `i128` で厳密に計算する |
| U7 | `fftshift`・`fftn` の軸指定の書き方が未定 | D-65 の「最頻出の短縮名（`*_axis`）と設定レコード（`*_with`）」。NC07 の `Tensor.AxesOptions` | 1D は `*_axis` と `Fft.Options { axis, n, norm }` の `*_with`。ND は `Fft.NdOptions { axes, shape, norm }` の `*_with`（`fft2` は最後の 2 軸、`fftn` は全軸）。`fftshift_axes`・`ifftshift_axes` は軸の配列を取る。既定値の記録 `Fft.options()`・`Fft.nd_options()` を足す |
| U8 | 誤りは `EmptyAxis axis` だけ | 軸・長さ・形の誤りが要る | `union Error = EmptyAxis of i64 \| InvalidAxis of (i64 * i64) \| RepeatedAxis of i64 \| InvalidLength of i64 \| LengthMismatch of (i64 * i64)`（`deriving (Eq)`、`Display`）。長さ 0 の変換軸は D2 のとおり `EmptyAxis` |
| U9 | `fft_parallel`・`fft2_parallel` だけ | 逆変換も同じ形で要る | `fft_parallel`・`ifft_parallel`（最後の軸のバッチ）と `fft2_parallel`・`ifft2_parallel`（最後の 2 軸）。直列の `fft`／`fft2` と同じ線の計算を同じ順（最後の軸が先）で呼ぶので、結果はビットまで同じ。WASM は NC15 の逐次フォールバック |
| U10 | `Polynomial` の係数の型が未定（`MODULES` の案は `Tensor.View`・`Tensor`・`[]`） | `Linalg.lstsq`・`Linalg.eigvals` は `Tensor.View` を取り、`Elementary<'a>`（`eigvals` は `Neg<'a>` も）を要る | 係数は昇べきの配列 `['a]`（`Polynomial.eval 2.0 [1.0, 2.0, 3.0]`）、標本点とデータは `Tensor.View<'a>`、根は `[Complex<'a>]`。制約はどの関数も `Elementary<'a>`（`f32`／`f64`。`roots` は `Neg<'a>` も）。失敗は `Result<_, Polynomial.Error>`（`ZeroDivisor`・`InvalidDegree`・`InvalidRank`・`ShapeMismatch`・`EmptyData`・`Linalg of Linalg.Error`）。負の階数・負のべきは事前条件の違反としてトラップ |
| U11 | `MODULES` の登録: `Fft` は `subject(&["Tensor.View", "Tensor"])` | `execute_into` は排他スライスが最後、`Plan` を読む関数がある | `("Fft", subject(&["Tensor.View", "Tensor", "Plan", "[]"]))`、`("Polynomial", subject(&["Tensor.View", "Tensor", "[]"]))` |
| U12 | `tests/fft.rs`・`tests/polynomial.rs` の形 | `tests/linalg.rs` は一時ディレクトリの edition 2028 のプロジェクトを `tsuzuri test` で走らせる | 同じ形。期待値は Tsuzuri のテストの中の素朴な $O(N^2)$ DFT と、NumPy 2.5.1 が出した値（生成スクリプトはテストの注釈に書く）。native／WASM の `-O0`／`-O3` は `tests/features.mjs` の suite `fft`・`polynomial` |

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
- [NC08](_completed/NC08-linalg-core-and-einsum.md): `Linalg.lstsq`, `Linalg.eigvals`（Phase 2 のみ）。
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

- **D1（委任による決定、2026-11-28。起票時は要承認）**: opt-in std モジュール `std/Fft.tz`・`std/Polynomial.tz` を追加し、`RESERVED_MODULES` に `"Fft"`・`"Polynomial"` を登録する（`D-55` へ反映）。利用者の指示「判断が必要なものがあれば、あなたが考える最高の選択肢で実装することを常に許可します」に基づき、チケットの案のとおりに決めた。
- **D2（既定案）**: 長さ 0 の変換軸は NumPy と同じくエラーとし、`Result.Error (EmptyAxis axis)` で安全に返す。
- **D3（既定案）**: `Polynomial` の係数順は NumPy 現行標準（`numpy.polynomial`）の昇べき順（`c[0]` が定数項）を採用する。
- **D4（委任による決定、2026-11-28）**: 計画の向きの型は `union Direction = Direct | Inverse` とする。採らない案: `Forward | Inverse`（`Norm.Forward` と同じモジュールで同じケース名は `E1001`）、`bool`（呼び出しの位置で意味が読めない）。
- **D5（委任による決定、2026-11-28）**: 確保なしの実行は、排他スライスを取る `execute_into plan input work output` と `execute_in_place plan work data` の 2 つとし、作業バッファの長さは `plan_work_length` で引く。採らない案: `Tensor.Mut` を取る形（`Tensor` の外から形とストライドを確保なしで読めない）、作業バッファを計画の中に持つ形（計画が可変になり、スレッドで共有できない）。
- **D6（委任による決定、2026-11-28）**: 変換の核は基数 4・2・3・5 の反復の混合基数 1 つとし、Bluestein の畳み込み長は `2n - 1` 以上の最小の `2^a 3^b 5^c` とする。Phase 1b の「実測して採用」は、`docs/benchmarks.md` の計測（基数 4: 1.33〜1.41 倍、混合基数: 5.1〜11 倍、滑らかな畳み込み長: 1.58〜1.83 倍。レビュー後に 9 標本の中央値で測り直した値は 1.33〜1.42 倍、5.1〜11.6 倍、1.58〜1.83 倍）で採用と決めた。Stockham 形・SoA・基数 7 以上は入れない（測っていない）。
- **D7（委任による決定、2026-11-28）**: `NdOptions.axes` に同じ軸を 2 回書くのは誤り（`RepeatedAxis`）とする。NumPy は同じ軸を 2 回変換するが、書き間違いと区別できないので受け付けない。
- **D8（委任による決定、2026-11-28）**: `Polynomial` の係数は配列 `['a]`、データは `Tensor.View<'a>`、要素型は `Elementary<'a>` とし、`add`・`sub`・`mul` は最高次の側の 0 を取り除かない（`trim` で明示する）。採らない案: 係数も `Tensor`（1 次元の小さい列に形とストライドを持たせる利点がない）、NumPy と同じ自動の取り除き（結果の長さが値で変わる）。

## 実装と検証（2026-11-28）

### 実装した Phase

- **Phase 1（`std/Fft.tz`、48 公開関数）**: 複素 1 次元（`fft`・`fft_axis`・`fft_with`・`ifft`・`ifft_axis`・`ifft_with`）、実数 1 次元（`rfft`・`rfft_axis`・`rfft_with`・`irfft`・`irfft_len`・`irfft_with`）、エルミート（`hfft`・`hfft_len`・`hfft_with`・`ihfft`・`ihfft_with`）、2 次元・N 次元（`fft2`・`ifft2`・`rfft2`・`irfft2`・`fftn`・`ifftn`・`rfftn`・`irfftn`・`fftn_with`・`ifftn_with`・`rfftn_with`・`irfftn_with`）、補助（`fftfreq`・`rfftfreq`・`fftshift`・`ifftshift`・`fftshift_axes`・`ifftshift_axes`・`options`・`nd_options`）、計画（`plan`・`plan_with`・`plan_length`・`plan_work_length`・`execute`・`execute_into`・`execute_in_place`）。型は `Norm`・`Direction`・`Options`・`NdOptions`・`Plan<'a>`（不透明）・`Error`。
- **Phase 1b**: 基数 4・2・3・5 の混合基数（核は 1 つ。`load`・`kernel`・`store`）、Bluestein の畳み込み長の `2^a 3^b 5^c` 化、`fft_parallel`・`ifft_parallel`・`fft2_parallel`・`ifft2_parallel`（`Parallel.for_each_chunk_with`、1 チャンク 16384 要素以上）。計測は `docs/benchmarks.md` の「Fft の変換の核と並列バッチ」（`node benchmarks/run-fft.mjs`）。
- **Phase 2（`std/Polynomial.tz`、23 公開関数）**: `trim`・`eval`・`eval_complex`・`eval_tensor`・`add`・`sub`・`mul`・`divmod`・`pow`・`deriv`・`deriv_n`・`integ`・`integ_const`・`from_roots`・`from_roots_complex`・`vander`・`fit`・`fit_weighted`・`roots`・`cheb_eval`・`cheb_fit`・`leg_eval`・`leg_fit`。型は `Error`。
- 登録: `src/stdlib.rs`（`SOURCES`・`RESERVED_MODULES` 83 → 85・`OPT_IN`（`Fft` は `Complex`・`Tensor`、`Polynomial` は `Complex`・`Tensor`・`Linalg`）・`opaque_record` に `Fft.Plan`）、`tests/std_argument_order.rs` の `MODULES`、`tests/std_examples.rs` の `FLOORS`（48 と 23。どの公開関数にも例がある）、`tests/fixtures/effects_sweep/Main.tz`（`spectra`・`polynomials`）、`vsc/src/core.ts`、`scripts/check-cli-docs.mjs` の `STD_PAGES`。

### チケットから外れた判断

「方針の最新化（2026-11-28）」の U1〜U12 と決定事項 D4〜D8 のとおり。ほかに実装の途中で決めたこと:

- `Fft.Options`・`Fft.NdOptions` は `deriving (Eq)` を持たない（`Maybe<i64>`・`Maybe<[i64]>` に `Eq` の instance がない。`E1025`）。
- `irfft`・`hfft` は NumPy と同じく、第 0 項と（偶数長の）第 `n / 2` 項の虚部を読まない。`fftfreq`・`rfftfreq` は `n <= 0` でトラップする（結果の型が `Tensor<f64>` で、`Result` ではない）。
- `fftshift` の 4 関数は `Copy<'a>` を要る（`Tensor.roll` と同じ。`string` のテンソルは渡せない）。
- 実数の ND の逆変換（`irfft2`・`irfftn`・`irfftn_with`）で形を与えないとき、最後の軸の実数の個数は `2 (m - 1)`。軸の一覧が空（階数 0、`axes: Maybe.Some []`）の実数の ND 変換は `LengthMismatch (1, 0)`。
- `Polynomial.integ` は常に係数を 1 個増やす（NumPy は `[0]` の積分を `[0]` のままにする）。`roots` の重根の精度は重複度 m でおよそ ε^(1/m)（テストは 2 重根を 1e-6 で確かめる）。
- 既存のコンパイラの振る舞いへの対処: 1 つの代入だけを持ち終了条件がデータに依る `while` に、コンパイラは `llvm.loop.unroll.enable` を付け、LLVM が展開できないと Clang が `loop not unrolled` の警告を `build`・`run` の標準エラーへ出す（利用者のコードでも起きる。このチケットでは直していない）。`std/Fft.tz` はそうした反復を再帰（`doubled_to`）と回数の決まった `for` で書き、`-O3` の native・wasm32 のビルドで警告が出ないことを確かめた。
- `pack`（窓を密なバッファへ写す）は、形が同じなら `Tensor.to_tensor` の連続の経路（`Array.sub`）を使う。`*_parallel` は軸・長さ・スケーリングの指定を持たない（`Backward` だけ）。
- 性能の再現用に `benchmarks/run-fft.mjs`・`benchmarks/fft/Main.tz` を足した（チケットの影響ファイルにはない）。README の機能一覧・予約名の表・テストの表に 1 行ずつ足した。

### 検証（macOS arm64、Apple M1 Max、dev ビルド。機械はほかの 2 つの作業ツリーのビルドとテストで高負荷だった）

- T0: `cargo fmt --all -- --check`、`cargo clippy --all-targets --locked -- -D warnings` は通過。
- T1: `node scripts/slot.mjs -- cargo nextest run --locked` は 3813 件を実行し、3811 件が通過、2 件が失敗、4 件が skip（基準の 3800 件 + `tests/fft.rs` 9 件 + `tests/polynomial.rs` 4 件）。失敗した 2 件は `llvm::split::tests::splits_the_programs_of_the_repository_1_of_3`・`_2_of_3` で、原因はテストの補助関数（`src/llvm_split.rs` の `executable_ir_with`）がトラップの source map に利用者のソースだけを渡していたこと。`def main` を持つ `tests/fixtures/effects_sweep` と `benchmarks/fft` が std の中の並列区間（`Fft.fft_parallel` の `Parallel.for_each_chunk_with`）に届くと、その位置が std のソースを指し、`E2000 invalid_trap_source_map` になった（CLI の `build --trap-info` は std のソースも渡すので通る）。補助関数が `stdlib::sources_for` の std のソースも渡すように直し、`cargo test --locked --lib splits_the_programs_of_the_repository` の 4 件と `cargo nextest run --locked --lib` の 792 件（skip 1）が通過。直したあとの全体の T1 は流し直していない（変えたのはテストの補助関数だけ）。
- `tests/fft.rs` 9 件（22 個の Tsuzuri のテスト: 41 種類の長さの素朴な DFT との照合、Parseval、`f32` と `f64` の照合（8209 の素数長を含む）、NumPy 2.5.1 の値（スケーリング 3 種・切り詰めと 0 埋め・軸・`rfft`／`irfft`／`hfft`／`ihfft`・2D／ND・`fftfreq`・シフト）、窓（逆順・並べ替え）、誤り、計画（`fft` と同じビット、確保 0 回）、並列と直列の一致）と `tests/polynomial.rs` 4 件（10 個のテスト）は通過。
- E2E: `node tests/features.mjs target/debug/tsuzuri fft`（212 ケース）・`polynomial`（124 ケース）は native と wasm32 の `-O0`・`-O3` で通過（WASM の import なし、`live == 0`、IR に `fmuladd`・fast-math の旗なし）。影響のない確認として `tensor`（42）・`tensor_operators`（4）・`matrix`（529）・`parallel`（39）も通過。
- ビットの一致: 変換のチェックサム 84 個（`fft`・`ifft`・`rfft`・`irfft`・`f32` の `fft`・`fft2` × 14 種類の長さ）が、native `-O0`・`-O3` と wasm32 `-O0`・`-O3` の 4 つで同じビットだった（手元で 1 回確かめた。CI には入れていない）。
- `tsuzuri test --doc std`: `-O0`・`-O3` とも 1421 件が通過（`Fft` 48 件、`Polynomial` 23 件）。`-O3` の全体の実行は `loop not unrolled` の警告を 19 個出すが、`--filter Fft`・`--filter Polynomial` では 0 個で、ほかの std モジュールの既存の警告である。
- `-O3` の警告: `tests/fixtures/fft`・`tests/fixtures/polynomial` の `--emit object` と `--target wasm32` のビルドは警告なし。
- 名指さないプログラムの IR: 基準 `b72bccd0` のコンパイラと、`tests/fixtures/`・`examples/` の `Fft`・`Polynomial` を名指さない全プログラムの `--emit llvm`（native と wasm32、320 個の出力）を比べ、320 個すべてが 1 byte も違わなかった。
- 文書の検査: `TSUZURI_BIN=target/debug/tsuzuri node scripts/gen-docs.mjs --check`（11 ブロック、古いもの 0）、`node scripts/check-cli-docs.mjs`（4 検査、失敗 0）、`node scripts/check-guide-lite.mjs`（87 参照、失敗 0）、`node scripts/check-links.mjs`（69 リンク、切れ 0）、`node scripts/repo-kb.mjs verify`（fresh）。Tsuzuri-Reference は `node scripts/check-docs.mjs` で 6 ページ（262 リンク、28 例、native の実行 56 回）が通過し、`check-cli-docs.mjs --reference-dir` で `Fft`・`Polynomial` の公開関数の書き漏れは 0。
- 性能: `node benchmarks/run-fft.mjs target/release/tsuzuri --numpy`（`docs/benchmarks.md`）。
- **確かめていないこと**: Linux・Windows・x86-64 での実行（CI に任せる）、wasm64、`--allocator host`・`small`、ASan・TSan のビルド、`tests/features.mjs` の全 suite、release ビルドでの T1、静かな機械での性能の絶対値。

### 更新した文書

- このリポジトリ: `docs/language.md`（`### Fft`・`### Polynomial`、予約モジュール名の一覧）、`docs/architecture.md`（`### Fft と Polynomial（NC09）`）、`docs/benchmarks.md`（`## Fft の変換の核と並列バッチ（NC09、2026-11-28）`）、`README.md`（機能一覧・予約名・テストの表）、`_features/GUIDE.md`（D-55 の表の 1 行、D-65 の「委任による決定（2026-11-28、NC09）」）、`_features/README.md`（状態 `done` とリンク）。
- Tsuzuri-Reference（ブランチ `docs/nc09-fft-polynomial`）: `docs/stdlib/numeric/fft.md`・`docs/stdlib/numeric/polynomial.md`（新規。「引数の順序」の節つき）、`docs/index.md`、`docs/stdlib/conventions.md`、`docs/stdlib/numeric/tensor.md`（相互リンク）、`docs/modules/namespaces-and-modules.md`（予約モジュール名の一覧を `RESERVED_MODULES` から作り直した。漏れていた `ExternAddr`・`MainThread` も入った）。

### 変更したファイル

`std/Fft.tz`（新規）、`std/Polynomial.tz`（新規）、`src/stdlib.rs`、`tests/fft.rs`（新規）、`tests/polynomial.rs`（新規）、`tests/fft_polynomial_reference.py`（新規。NumPy の期待値を出す）、`tests/fft-polynomial-cases.mjs`（新規。JavaScript の参照）、`tests/features.mjs`、`tests/fixtures/fft/`（新規）、`tests/fixtures/polynomial/`（新規）、`tests/fixtures/effects_sweep/Main.tz`、`tests/std_argument_order.rs`、`tests/std_examples.rs`、`src/llvm_split.rs`（テストの補助関数だけ）、`vsc/src/core.ts`、`scripts/check-cli-docs.mjs`、`benchmarks/run-fft.mjs`（新規）、`benchmarks/fft/Main.tz`（新規）、`docs/language.md`、`docs/architecture.md`、`docs/benchmarks.md`、`README.md`、`_features/GUIDE.md`、`_features/README.md`、このチケット。

### レビュー後の修正（2026-11-28、PR #153）

- **ベンチマークが最小値しか残さない**（Copilot の 2 回目の指摘）: `benchmarks/run-fft.mjs` は各行の最小値だけを出し、`benchmarks/fft/Main.tz` も 5 回の最小値だけを出していたので、`docs/benchmarks.md` の「性能指標の記録」の読み方（生の標本・中央値・幅）で比べられなかった。`run-fft.mjs` は、数えない 1 回のあとの実行（既定 5 回、`--metrics` では 9 回以上）の標本をすべて持ち、最小・中央値・最大を出し、`--metrics DIR` で `schema: 1` のレコード（suite `fft`、`makeRecord`・`writeRecords`）を書く。`Main.tz` は最初のラウンドを除く 9 ラウンドを 1 行ずつ出す。
- 測り直し（load average 約 6、`target/perf/NC09-fft-20261128/fft.jsonl`。核 36 行 × 9 標本、並列バッチ 20 行 × 81 標本）: 基数 4 は 1.33〜1.42 倍、混合基数は 5.1〜11.6 倍、滑らかな畳み込み長は 1.58〜1.83 倍、並列は 2.7〜6.2 倍（中央値の比）。採否の結論は変わらない。`docs/benchmarks.md` の表を中央値と幅に置き換えた。

