# NC06: ブロードキャスト対応の要素ごと演算（ufunc）・比較・インプレース出力・演算子インスタンス

| 項目 | 内容 |
| --- | --- |
| ID | NC06 |
| 優先度 | P1（多次元配列の四則・数学・比較・論理演算の中核） |
| 規模 | L（ブロードキャスト反復エンジン、算術/比較/論理/ビット/数学 ufunc、`_into` ゼロ確保版、`Tensor`/`Matrix` 演算子インスタンス） |
| 依存 | [NC01](NC01-complex-and-extended-math.md), [NC02](NC02-tensor-core-views-and-broadcasting.md), ([NC03](NC03-borrowed-arithmetic-operators.md)) |
| 後続 | [NC07](NC07-tensor-reductions-stats-and-sorting.md), [NC08](NC08-linalg-core-and-einsum.md), [NC13](NC13-tensor-simd-parallel-and-gpu-kernels.md) |
| 状態 | todo |
| 起票 | 2026-11-24（[GUIDE D-65](GUIDE.md#d-65-数値計算基盤と-numpy-互換機能案-c-ハイブリッドnc01nc14)。HEAD `27069465` で確認） |
| 承認 | 不要（既定案のみ。演算子インスタンスの非消費化は [NC03](NC03-borrowed-arithmetic-operators.md) の承認に委ねる） |
| 手本にする既存実装 | `std/Tensor.tz`（`map` の連続高速経路＋汎用ストライド走査）、`std/Matrix.tz`（`add`）、`std/MatrixView.tz`（`map_in_place`） |
| 主な影響ファイル | `std/Tensor.tz`、`std/Matrix.tz`、`tests/tensor_ufunc.rs`（新規）、`tests/std_argument_order.rs`、`tests/std_examples.rs`、`tests/features.mjs`、`tests/fixtures/effects_sweep/`、`tests/fixtures/tensor/`、`_tsuzuri/language-reference/built-in-types-and-modules/tensor.md`、`_tsuzuri/language-reference/built-in-types-and-modules/matrix.md` |

## 実装者向け早見

- 範囲: (1) 2 入力・3 入力のブロードキャスト対応反復関数（`map2`/`map2_into`/`map3`/`map3_into`）を実装し、同形状かつ連続な場合は 1D スライス直走査の高速経路、それ以外はストライド反復経路へ自動分岐する。(2) NumPy ufunc 相当の要素ごと関数群（四則・`floor_divide`/`floor_mod`/`divmod`/`fmod`/`ieee_remainder`・スカラー演算・`neg`/`abs`/`pow`/`rem`/`fma`・ビット演算・論理演算・要素ごと比較 `eq`/`ne`/`lt`/`le`/`gt`/`ge`・`allclose`/`isclose`/`array_equal`・`isnan`/`isinf`/`isfinite`/`nan_to_num`・全 `Math.*` および `Complex.*` のテンソル版）を提供する。(3) ゼロアロケーションのインプレース版（`add_into`/`sub_into`/`mul_into`/`div_into`/`mul_add_into` 等。出力先 `ref mut Mut<'c>` は `Tensor.borrow_mut` または `Tensor.update` から得る）と、汎用 `ufunc` 還元（`outer`/`accumulate` 基盤）を揃える。(4) [NC03](NC03-borrowed-arithmetic-operators.md) 完了後、`Tensor` に `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Neg` インスタンスを提供する。
- 最初に読む: ① [NC02](NC02-tensor-core-views-and-broadcasting.md) の `broadcast_pair` と `Tensor.Mut` ② 仕様 1〜4 ③ `std/Tensor.tz` の `map` / `gather`。
- 最初に書くテスト: `tests/tensor_ufunc.rs` で同形状連続・非連続（転置/負ストライド）・ブロードキャスト（例: `[10, 100]` と `[10, 1]`、スカラー 0D と 2D）・非互換形状エラー・`add_into` の確保 0 回（`Alloc.count` 検証）。
- 触ってはいけない: 数値の丸め・NaN・符号付きゼロ・整数折り返し規則（暗黙 FMA や `fast-math` 禁止）。`Matrix.mul`（行列積）と要素ごと乗算（`Matrix.hadamard` / `Tensor.mul`）を混同しないこと（D2）。二項関数の引数順は常に `left -> right`（`Tensor.sub left right = left - right`、`Tensor.div left right = left / right`）を守ること（D4）。
- 完了の見分け方: `(x - mean) / std`（`NC03` 後）および `Tensor.sub x mean` → `Tensor.div diff std`・`Tensor.add_into`・`Tensor.sin`・`Tensor.allclose` がすべて native/WASM `-O0`/`-O3` で一致する。

## 目的

現状の `Tensor` には要素ごとの算術・比較・数学関数が一切なく、`Matrix` にも `add` しかない。ブロードキャスト対応の要素ごと演算とインプレース出力先指定（`_into`）を整備することで、NumPy の ufunc 相当の計算を安全かつ高効率に書けるようにする。

## 着手条件と停止条件

### 着手条件

- [NC01](NC01-complex-and-extended-math.md) と [NC02](NC02-tensor-core-views-and-broadcasting.md) が done。
- `Tensor` / `Matrix` の `Add`〜`Neg` インスタンス接続（手順 4）は [NC03](NC03-borrowed-arithmetic-operators.md) が done の後に実施（手順 1〜3 は NC03 と並行可能）。

### 停止条件

- **S1** `Matrix` の `*` 演算子を行列積にするか要素ごと積（Hadamard 積）にするかで既存規約と矛盾が生じる場合（D2 に従い、`Tensor` の `*` は要素ごと積、`Matrix` は要素ごと積を `hadamard`・行列積を `Matrix.mul` と明確に分け、`Matrix` への `Mul` インスタンスは誤解を避けて付けない既定案を採る）。
- **S2** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `27069465` で確認）

- `std/Tensor.tz`: `map` と `fold` のみ。二項演算・ブロードキャスト演算・比較・数学関数・インプレース演算はすべて未実装。
- `std/Matrix.tz`: `add` と行列積 `mul`/`mul_fma`/`mul_parallel`/`mul_fma_parallel` のみ。

## 変えてはいけない意味

- 浮動小数点の丸め・`NaN`・`±0.0`、整数の折り返しとゼロ除算トラップ。
- `Matrix.mul` の行列積仕様とビット一致契約。

## 仕様

### 1. ブロードキャスト二項・三項エンジン

- `map2 :: (Copy<'a>, Copy<'b>) => own ('a -> 'b -> 'c) -> ref View<'a> -> ref View<'b> -> Result<Tensor<'c>, Error>`
- `map2_into :: (Copy<'a>, Copy<'b>) => own ('a -> 'b -> 'c) -> ref View<'a> -> ref View<'b> -> ref mut Mut<'c> { r } -> Result<unit, Error>`（出力先形状とブロードキャスト形状が一致しない場合は何も書かずに `Result.Error`）
- `map_into :: Copy<'a> => own ('a -> 'b) -> ref View<'a> -> ref mut Mut<'b> { r } -> Result<unit, Error>`
- `map3` / `map3_into`（`fma`・`where_select` 等の 3 入力 ufunc 用）
- 高速経路: 両入力（および出力）が同形状かつ C 連続のとき、多次元ループを省略し 1D スライス直走査で LLVM の自動ベクトル化が効く形にする。

### 2. 要素ごと算術・スカラー・ビット・論理演算

- 二項（すべてブロードキャスト対応、形状不一致は `Result<Tensor<_>, Error>`。同形状・ブロードキャスト確定の簡潔版は演算子または `*_exn`/トラップ版ではなく、`Add` インスタンスのみトラップ契約: D1）:
  - `add`, `sub`, `mul`, `div`, `rem`（0 方向丸め剰余）, `floor_divide`（床除算。NumPy `np.floor_divide` 相当）, `floor_mod`（床剰余。NumPy `np.remainder` / `np.mod` 相当）, `divmod`（`(floor_divide, floor_mod)`）, `fmod`（C99 0 方向浮動小数点剰余）, `ieee_remainder`（IEEE 754 最近接偶数商の剰余。NumPy `np.remainder` との混同を避けるため `remainder` 名は使わない）, `pow`, `maximum`, `minimum`, `fmax`, `fmin`, `atan2`, `hypot`, `copysign`, `logaddexp`, `logaddexp2`
  - ビット演算（`Bits<'a>`）: `bit_and`, `bit_or`, `bit_xor`, `bit_not`, `shl`, `shr`
  - 論理演算（`bool`）: `logical_and`, `logical_or`, `logical_xor`, `logical_not`
- スカラー演算（`scale`/`add_scalar`/`sub_scalar`/`rsub_scalar`/`div_scalar`/`rdiv_scalar`/`pow_scalar` — 対象ビューが最後: D-59）
- インプレース出力版（`_into`）: `add_into`, `sub_into`, `mul_into`, `div_into`, `scale_into`, `mul_add_into`（`out = a * b + c`、`fma_into` は明示 `Math.fma`）。出力先 `ref mut Mut<'c>` は [NC02](NC02-tensor-core-views-and-broadcasting.md) の `Tensor.borrow_mut`（`let mut` ローカル配列から構築）または `Tensor.update`（所有 `Tensor` の move-out 更新）で用意する。
- 単項数学 ufunc（`ref View<'a> -> Tensor<'a>` および `*_into`）:
  - `neg`, `abs`, `sqrt`, `cbrt`, `square`, `reciprocal`, `floor`, `ceil`, `trunc`, `round`（0 から遠い側へ丸め）, `round_even`（最近接偶数丸め。NumPy `np.round` / `np.rint` 相当）, `sign`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `sinh`, `cosh`, `tanh`, `asinh`, `acosh`, `atanh`, `exp`, `exp2`, `expm1`, `log`, `log2`, `log10`, `log1p`, `sinc`, `degrees`, `radians`
  - 複素数ビュー向け: `real`, `imag`, `conj`, `angle`（`Complex<'a>`）
- 特殊値検査・置換: `is_nan`, `is_infinite`, `is_finite`, `signbit`, `nan_to_num :: Float<'a> => own 'a -> own 'a -> own 'a -> ref View<'a> -> Tensor<'a>`

### 3. 要素ごと比較・近傍一致

- `elem_eq`, `elem_ne`, `elem_lt`, `elem_le`, `elem_gt`, `elem_ge`（ブロードキャスト対応で `Result<Tensor<bool>, Error>` を返す。スカラー比較の `gt_scalar` / `lt_scalar` / `ge_scalar` / `le_scalar` / `eq_scalar` も提供し `sub[Tensor.gt_scalar 0.0 sub]` を簡潔に書けるようにする）
- `isclose :: Float<'a> => own 'a -> own 'a -> own bool -> ref View<'a> -> ref View<'a> -> Result<Tensor<bool>, Error>`（`rtol`, `atol`, `equal_nan`）
- `allclose :: Float<'a> => own 'a -> own 'a -> own bool -> ref View<'a> -> ref View<'a> -> bool`
- `array_equal`, `array_equiv`

### 4. `outer` と演算子インスタンス（`NC03` 後）

- `outer_with :: (Copy<'a>, Copy<'b>) => own ('a -> 'b -> 'c) -> ref View<'a> -> ref View<'b> -> Tensor<'c>`（1D 入力の外積表。ufunc `outer` 相当）
- [NC03](NC03-borrowed-arithmetic-operators.md) 完了後、`Tensor<'a>` に借用ベースの `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Neg` インスタンスを追加（ブロードキャスト適用、形状非互換はトラップ。インスタンス文脈には `instance (Add<'a>, Copy<'a>) => Add<Tensor<'a>>` のように使用する演算子クラスと `Copy<'a>` を明示する）。これにより `(x - mean) / std` がそのまま書ける。

## 設計

- 1D 連続高速経路は `ref data[offset .. offset + total]` の単純 `for i in 0i64 ..= (total - 1)` ループとする。ただし、汎用高階関数 `map2` 経由ではクロージャ間接呼び出し（`call double %fn(...)`）が残って LLVM が自動ベクトル化できないことがあるため、主要二項・単項演算（`add`/`sub`/`mul`/`div`/`scale`/`fma` 等）の連続高速経路は最適化後 LLVM IR（`--emit llvm -O3`）で間接呼び出しが残らず自動ベクトル化されることを確認し、インライン化されない場合はクロージャを経由しない直接ループで実装する（[Matrix.tz](../std/Matrix.tz) の `multiply_rows` と同じ流儀）。
- 汎用ストライド経路は最内軸（`axis == rank - 1`）をタイトな内側ループにし、外側軸だけ再帰または明示スタックで走査する。
- `Alloc`（F14）を使って `*_into` 関数が追加ヒープ確保 0 回であることをテストで構造的に表明する。

## 前提とする他チケットのインターフェース

- [NC01](NC01-complex-and-extended-math.md): `Complex<'a>`、新規 `Math.*` 関数群。
- [NC02](NC02-tensor-core-views-and-broadcasting.md): `Tensor.View`、`Tensor.Mut`、`broadcast_pair`、`Tensor.Error`。
- 弱い依存 [NC03](NC03-borrowed-arithmetic-operators.md): 借用算術演算子クラス。

## 他チケットへ提供するインターフェース

- `map2`/`map2_into`/全要素ごと算術・比較・数学 ufunc・`allclose`（[NC07](NC07-tensor-reductions-stats-and-sorting.md)〜[NC13](NC13-tensor-simd-parallel-and-gpu-kernels.md) が利用）。

## 実装手順

1. **変更**: `std/Tensor.tz`。**内容**: `map_into`/`map2`/`map2_into`/`map3`/`map3_into` のブロードキャスト反復エンジン（連続高速経路＋汎用ストライド経路）を実装する。**確認**: T0、`tests/tensor_ufunc.rs`（`Alloc` で `_into` の確保 0 回を表明）。
2. **変更**: `std/Tensor.tz`, `std/Matrix.tz`, `tests/fixtures/effects_sweep/`。**内容**: 要素ごと四則（すべて `left -> right` 順）・スカラー・ビット・論理・比較・`isclose`/`allclose`・`nan_to_num`・全数学 ufunc・`outer_with`、および `Matrix` の不足要素ごと演算（`sub`/`hadamard`/`scale`）とコーパス網羅呼び出しを実装する。**確認**: T0、`cargo test --locked --test std_argument_order --test std_examples --test effects_alloc_conservative`、`tsuzuri test --doc std`、T1。
3. **変更**: `std/Tensor.tz`（`NC03` 完了後）。**内容**: `Tensor<'a>` への借用 `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Neg` インスタンスを追加する。**確認**: T0、`(x - mean) / std` テスト。
4. **変更**: `tests/fixtures/`・`tests/features.mjs`、文書。**内容**: native/WASM `-O0`/`-O3` E2E とドキュメント更新。**確認**: T2、`node scripts/check-docs.mjs`。

## テスト計画

- **Rust テスト（`tests/tensor_ufunc.rs`）**: 同形状・1D/2D/3D/4D ブロードキャスト・0D スカラー・空テンソル・負ストライドビュー・非互換形状エラー・非可換二項演算（`sub left right == left - right`, `div left right == left / right`）・`isclose`/`allclose` の `NaN`/`Inf`/`equal_nan` 境界・`_into` の確保 0 回。
- **E2E（`tests/features.mjs`）**: 独立参照値（Python/NumPy と同等の数値）との照合、`live == 0`、native/WASM `-O0`/`-O3` ビット一致。

## ドキュメント

- `_tsuzuri/language-reference/built-in-types-and-modules/tensor.md`・`matrix.md`

## 受け入れ条件

- 全 ufunc がブロードキャスト・負ストライド・空テンソルを正しく扱い、`*_into` がヒープ確保 0 回で動作すること。
- 主要二項・単項演算（`add`/`sub`/`mul`/`div`/`scale` 等）の連続高速経路について、`-O3 --emit llvm` の最適化後コードにクロージャ間接呼び出しが残らず自動ベクトル化可能であることを確認済みであること。
- 暗黙 FMA・`fast-math` を一切含まず、native/WASM `-O0`/`-O3` でビット一致し、`effects_alloc_conservative` と T2 が通ること。

## 落とし穴

- 非可換の二項演算（`sub`/`div`/`rem`/`pow`）は `AGENTS.md`（「複数の引数が対象の関数は F# の順序を保つ」）どおり `left -> right` で統一する。パイプラインで `x |> Tensor.sub y` と書くと `y - x` になるため、数式は `NC03` の演算子 `x - y` または明示の `Tensor.sub x y` を使うこと。
- `Tensor.round` は `Math.round`（0 から遠い側へ丸め）であり、NumPy の `np.round(2.5) == 2.0` に相当するのは `Tensor.round_even` である。また NumPy の `np.remainder` に相当するのは `Tensor.floor_mod` であり、IEEE 剰余は `Tensor.ieee_remainder` として区別する。
- `std/Tensor.tz` から `Complex` の関数（`real`/`imag`/`conj`/`angle`）を直接参照する場合は `OPT_IN` の `Tensor.uses` に `"Complex"` を追加する（`OPT_IN.uses` は直接参照するモジュールのみの一致検査）。
- 高階関数 `map2` へクロージャを渡すと、現在の最適化経路では間接呼び出しが残って自動ベクトル化が止まる場合がある。主要算術 ufunc の連続経路は直接ループまたは実測＋IR 確認を忘れないこと。

## 対象外

- 演算融合（式テンプレート / 遅延計算グラフ）。
- SIMD / 並列 / GPU 専用テンソルカーネル（[NC13](NC13-tensor-simd-parallel-and-gpu-kernels.md) が担当）。

## 決定事項

- **D1（既定案）**: 関数 API（`Tensor.add` 等）は形状非互換を `Result<Tensor<_>, Tensor.Error>` で返し、演算子インスタンス（`a + b`、`NC03` 後）は `Add` の署名上 `Tensor<'a>` を直接返すため形状非互換時にトラップする。
- **D2（既定案）**: `Tensor` の `*` 演算子は要素ごと積（NumPy と同じ）。`Matrix` は行列積との取り違えを防ぐため `Mul` インスタンスを付けず、明示的に `Matrix.mul`（行列積）と `Matrix.hadamard`（要素ごと積）を使い分ける。
- **D3（既定案）**: 要素ごと比較は `Tensor.elem_gt` / `Tensor.gt_scalar` 等の関数 API とし、`Eq`/`Ord` 演算子（`==`/`<`）の「常にスカラー `bool` を返す」契約を壊さない。
- **D4（既定案）**: 二項テンソル関数は `AGENTS.md` と `Matrix.mul` に従いすべて `left -> right` 順（`Tensor.sub left right = left - right`、`Tensor.div left right = left / right`）とし、スカラー演算のみ対象テンソルを最後（`sub_scalar s x = x - s`、`rsub_scalar s x = s - x`）とする。
