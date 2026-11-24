# NC05: `Tensor` の形状操作・結合/分割・ファンシーインデキシング・ブールマスク

| 項目 | 内容 |
| --- | --- |
| ID | NC05 |
| 優先度 | P1（多次元配列の実用的な前処理・変形・抽出に必須） |
| 規模 | M（純 Tsuzuri ライブラリ実装、`std/Tensor.tz` 拡張とテスト） |
| 依存 | [NC02](NC02-tensor-core-views-and-broadcasting.md), ([NC04](NC04-generic-index-class-and-slice.md)) |
| 後続 | [NC07](NC07-tensor-reductions-stats-and-sorting.md), [NC08](NC08-linalg-core-and-einsum.md) |
| 状態 | done |
| 起票 | 2026-11-24（[GUIDE D-65](GUIDE.md#d-65-数値計算基盤と-numpy-互換機能案-c-ハイブリッドnc01nc14)。HEAD `27069465` で確認） |
| 承認 | 不要（既定案のみ。新予約語・構文・モジュール追加なし） |
| 手本にする既存実装 | `std/Tensor.tz`、`std/Array.tz`（`filter`/`partition`/`concat`/`take`/`skip` の引数順序とエラー処理） |
| 主な影響ファイル | `std/Tensor.tz`、`src/stdlib.rs`（`NC04` 連携時の `Tensor.uses`）、`tests/tensor_ops.rs`（新規）、`tests/std_argument_order.rs`、`tests/std_examples.rs`、`tests/features.mjs`、`tests/fixtures/effects_sweep/`、`tests/fixtures/tensor/`、`_tsuzuri/language-reference/built-in-types-and-modules/tensor.md` |

## 実装者向け早見

- 範囲: [NC02](NC02-tensor-core-views-and-broadcasting.md) の負/0 ストライドと `Tensor.Mut` の上に、NumPy の形状操作・結合/分割・ファンシーインデキシング・ブールマスク API をすべて純 Tsuzuri で実装する。
- 最初に読む: ① [NC02](NC02-tensor-core-views-and-broadcasting.md) の仕様 ② 仕様 1〜4 ③ `tests/std_argument_order.rs`（D-59: 対象は最後の引数）。
- 最初に書くテスト: `tests/tensor_ops.rs` で `reshape` の `-1` 推論、`squeeze`/`expand_dims`、`concatenate`/`stack`/`split`、`where`/`take`/`compress`/マスク抽出、非互換形状のエラー処理。
- 触ってはいけない: 新予約語や新構文の追加、既存 `Tensor` 関数のシグネチャ。
- 完了の見分け方: ゼロコピー形状変換（`squeeze`/`expand_dims`/`swapaxes`/`moveaxis`/`transpose`/`flip`）とコピー系（`concatenate`/`stack`/`pad`/`tile`/`repeat`/`where`/`take`/`mask`）がすべて独立参照（NumPy と同等の仕様）と一致する。

## 目的

[NC02](NC02-tensor-core-views-and-broadcasting.md) で整えたストライドビュー基盤を、データ整形・結合・分割・条件抽出といった日常的な配列操作まで広げる。

## 着手条件と停止条件

### 着手条件

- [NC02](NC02-tensor-core-views-and-broadcasting.md) が done。

### 停止条件

- **S1** すべての新規関数が D-59（対象が最後）を満たせないケースが見つかった場合（特に `where` や `concatenate` の引数順）。決定事項 D1 に従って統一し、矛盾があれば止めて報告する。
- **S2** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `27069465` で確認）

- `std/Tensor.tz` には `reshape`（`-1` 非対応）・`permute`・`index_axis`・`narrow` のみがあり、`squeeze`・`expand_dims`・`concatenate`・`stack`・`split`・`flip`・`pad`・`tile`・`repeat`・`where`・`take`・`nonzero`・ブールマスク抽出はすべて未実装。

## 変えてはいけない意味

- 既存の `reshape`（正の形状指定）・`permute`・`index_axis`・`narrow` の挙動。
- D-59（対象は最後の引数）。

## 仕様

### 1. 形状操作

- **`reshape_infer`（または `reshape` の `-1` 対応）**: 形状配列に `-1` を最大 1 つ含められ、他軸の積で割り切れる長さを自動推論する（複数 `-1` や割り切れない場合はトラップ／`try_reshape` は `Result`）。
- **ゼロコピービュー操作（`ref View<'a> { r } -> View<'a> { r }`）**:
  - `transpose`（引数なし全軸反転。軸指定版は既存 `permute`）
  - `swapaxes :: own i64 -> own i64 -> ref View<'a> { r } -> View<'a> { r }`（負の軸番号対応）
  - `moveaxis :: own i64 -> own i64 -> ref View<'a> { r } -> View<'a> { r }`
  - `squeeze`（長さ 1 の全軸を除去）、`squeeze_axis :: own i64 -> ...`
  - `expand_dims :: own i64 -> ...`（位置 `axis` に長さ 1・stride 0 の軸を挿入）
  - `atleast_1d`, `atleast_2d`, `atleast_3d`
  - `flip`（全軸反転）、`flip_axis :: own i64 -> ...`、`fliplr`, `flipud`、`rot90 :: own i64 -> ...`
  - `slice :: own [Slice.Axis] -> ref View<'a> { r } -> View<'a> { r }`（[NC04](NC04-generic-index-class-and-slice.md) が done のときに接続し、`t[axes]` でも呼べる。`std/Tensor.tz` から `Slice` を参照する場合は `src/stdlib.rs` の `OPT_IN` で `Tensor.uses` に `"Slice"` を追加する）
- **新しい `Tensor` を返す操作**:
  - `ravel`（連続なら `reshape_view [count]` のビュー版 `ravel_view` と、所有コピー版 `flatten`）
  - `roll :: own i64 -> own i64 -> ref View<'a> -> Tensor<'a>`（軸とシフト量）
  - `tile :: own [i64] -> ref View<'a> -> Tensor<'a>`、`repeat :: own i64 -> own i64 -> ref View<'a> -> Tensor<'a>`
  - `pad :: own [(i64 * i64)] -> own PadMode<'a> -> ref View<'a> -> Tensor<'a>`（`Constant of 'a | Edge | Reflect | Symmetric | Wrap`）

### 2. 結合・分割

- `concatenate :: Copy<'a> => own i64 -> ref [View<'a>] -> Result<Tensor<'a>, Error>`（軸指定。他軸の形状不一致は `Result.Error`）
- `stack :: Copy<'a> => own i64 -> ref [View<'a>] -> Result<Tensor<'a>, Error>`（新しい軸を作成して積み重ね）
- `vstack`, `hstack`, `dstack`, `column_stack`
- `split :: own i64 -> own i64 -> ref View<'a> -> [View<'a>]`（均等分割、割り切れない場合トラップ）、`split_at :: own i64 -> own [i64] -> ref View<'a> -> [View<'a>]`、`array_split`（不均等分割許容）、`hsplit`, `vsplit`, `dsplit`（すべてゼロコピービューの配列を返す。`regions.rs` の規則により戻り値 `[View<'a>]` には `{ r }` を付けず、唯一の借用引数から寿命を推論する）。

### 3. ファンシーインデキシング・ブールマスク・条件選択

- **ブールマスク**:
  - `mask :: Copy<'a> => ref View<bool> -> ref View<'a> -> Tensor<'a>`（マスクが `true` の要素を 1D テンソルとして抽出。`NC04` の `t[mask]` に対応）
  - `compress :: Copy<'a> => own i64 -> ref [bool] -> ref View<'a> -> Tensor<'a>`
  - `nonzero :: (Eq<'a>, Default<'a>) => ref View<'a> -> [[i64]]`（軸ごとの座標配列）、`argwhere`（`[N, rank]` 形状の `Tensor<i64>`）、`flatnonzero :: ... -> [i64]`
  - `put_mask :: Copy<'a> => ref View<bool> -> own 'a -> ref mut Mut<'a> { r } -> unit`（マスク位置へのインプレース代入）
- **条件選択**:
  - `where_select :: Copy<'a> => ref View<bool> -> ref View<'a> -> ref View<'a> -> Result<Tensor<'a>, Error>`（3 入力のブロードキャスト対応。`where` は Tsuzuri/Array 慣例との衝突を避け `where_select` または `select_where` とする: D1）
  - `where_scalar`（真側・偽側スカラー版）、`clip :: (Ord<'a>, Copy<'a>) => own 'a -> own 'a -> ref View<'a> -> Tensor<'a>`
- **整数配列インデキシング（gather / scatter）**:
  - `take :: Copy<'a> => own i64 -> ref View<i64> -> ref View<'a> -> Tensor<'a>`（負インデックス対応）
  - `take_along_axis :: Copy<'a> => own i64 -> ref View<i64> -> ref View<'a> -> Tensor<'a>`
  - `put_along_axis :: Copy<'a> => own i64 -> ref View<i64> -> ref View<'a> -> ref mut Mut<'a> { r } -> unit`

## 設計

- ゼロコピーで表現できる操作（`transpose`/`swapaxes`/`moveaxis`/`squeeze`/`expand_dims`/`flip`/`rot90`/`split`/`slice`）は必ず `View<'a> { r }`（および単射なものは `Mut` 版）を返し、要素をコピーしない。
- 負の軸番号（`-1` = 最終軸）は共通ヘルパ `normalize_axis(axis, rank)` で一元検証する。
- [NC04](NC04-generic-index-class-and-slice.md) が完了していれば `Tensor` に `Index`（`[Slice.Axis]`・`Tensor<bool>`）インスタンスを接続する。

## 前提とする他チケットのインターフェース

- [NC02](NC02-tensor-core-views-and-broadcasting.md): `Tensor.View`（負/0 ストライド）、`Tensor.Mut`、`Tensor.Error`、`broadcast_pair`。
- 弱い依存 [NC04](NC04-generic-index-class-and-slice.md): `Slice.Axis`、`Index` インスタンス接続。

## 他チケットへ提供するインターフェース

- `normalize_axis`、`squeeze`/`expand_dims`/`swapaxes`/`moveaxis`/`concatenate`/`stack`/`take`/`where_select`/`clip`（[NC07](NC07-tensor-reductions-stats-and-sorting.md)・[NC08](NC08-linalg-core-and-einsum.md) が利用）。

## 実装手順

1. **変更**: `std/Tensor.tz`。**内容**: `normalize_axis`・ゼロコピー形状操作群・`-1` 推論 `reshape`・`roll`/`tile`/`repeat`/`pad` を実装する。**確認**: T0、`tests/tensor_ops.rs` の形状テスト。
2. **変更**: `std/Tensor.tz`。**内容**: `concatenate`/`stack`/`vstack`/`hstack`/`dstack`/`split`/`array_split` を実装する。**確認**: T0、結合・分割テスト（0 要素軸・空入力・形状不一致エラー）。
3. **変更**: `std/Tensor.tz`, `tests/fixtures/effects_sweep/`。**内容**: `mask`/`compress`/`nonzero`/`argwhere`/`flatnonzero`/`put_mask`/`where_select`/`clip`/`take`/`take_along_axis`/`put_along_axis` を実装し、全公開関数に doc-test 例とコーパス網羅呼び出しを付ける。**確認**: T0、`cargo test --locked --test std_argument_order --test std_examples --test effects_alloc_conservative`、`tsuzuri test --doc std`、T1。
4. **変更**: `tests/fixtures/`・`tests/features.mjs`、`_tsuzuri/language-reference/built-in-types-and-modules/tensor.md`。**内容**: E2E 検証と文書更新。**確認**: T2、`node scripts/check-docs.mjs`。

## テスト計画

- **Rust テスト（`tests/tensor_ops.rs`）**: `-1` 推論の正常・複数 `-1`・割り切れない場合のトラップ、負軸番号、ゼロコピー操作のストライド検証、`pad` の全 5 モード、`concatenate`/`stack`/`split` の境界、`where_select` のブロードキャスト、`take`/`take_along_axis`/`put_along_axis` の参照値一致。
- **E2E（`tests/features.mjs`）**: native / WASM `-O0`・`-O3`、`live == 0`。

## ドキュメント

- `_tsuzuri/language-reference/built-in-types-and-modules/tensor.md`

## 受け入れ条件

- 全 API が D-59（対象が最後）を満たし、`tests/std_argument_order.rs`・`tests/effects_alloc_conservative.rs`・`tsuzuri test --doc std` を通ること。
- ゼロコピー操作がバッファを複製せず、コピー操作も `live == 0` を満たすこと。

## 落とし穴

- `split` 系の戻り値 `[View<'a>]` に `{ r }` を付けると `regions.rs` の `ResultRegionMissing`（`E1013`）になる。単一借用引数関数として `{ r }` なしで書くこと。
- `NC04` の `Slice` を `std/Tensor.tz` から直接参照する場合は `src/stdlib.rs` の `OPT_IN` の `Tensor.uses` に `"Slice"` を必ず追加する（漏れると `Tensor` だけを使うプログラムが `Slice` 未ロードで失敗する）。

## 対象外

- 要素ごと算術・数学 ufunc（[NC06](NC06-tensor-ufuncs-and-elementwise.md)）。
- 軸指定の還元・統計・ソート（[NC07](NC07-tensor-reductions-stats-and-sorting.md)）。

## 決定事項

- **D1（既定案）**: NumPy の `np.where` は `Array.where`（フィルタ）と名前・意味が紛らわしいため、`Tensor.where_select condition on_true on_false`（条件選択）と `Tensor.nonzero`（座標取得）に役割を明確に分離する。
- **D2（既定案）**: `split` / `array_split` は要素を複製せず `View<'a>` の配列を返す（`regions.rs` により戻り値要素へ `{ r }` は付けず、単一借用引数から寿命を推論する）。所有が必要な呼び出し側は `Tensor.to_tensor` を明示する。
