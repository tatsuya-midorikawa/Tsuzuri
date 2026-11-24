# NC02: `Tensor` コア基盤の拡張（`Tensor.Mut`・負/0 ストライド・ブロードキャスト・生成 API・`Eq`/`Display`）

| 項目 | 内容 |
| --- | --- |
| ID | NC02 |
| 優先度 | P0（全数値計算チケットの中核データ構造） |
| 規模 | L（`std/Tensor.tz` の内部表現拡張・`Mut` ビュー・ブロードキャスト・生成・表示） |
| 依存 | – |
| 後続 | ([NC04](NC04-generic-index-class-and-slice.md)), [NC05](NC05-tensor-shape-indexing-and-masking.md), [NC06](NC06-tensor-ufuncs-and-elementwise.md), [NC07](NC07-tensor-reductions-stats-and-sorting.md), [NC08](NC08-linalg-core-and-einsum.md), [NC09](NC09-fft-and-polynomial.md), [NC10](NC10-random-distributions-and-generators.md), [NC11](NC11-tensor-codec-and-io.md), [NC13](NC13-tensor-simd-parallel-and-gpu-kernels.md), [NC14](NC14-python-buffer-protocol-and-dlpack-bindings.md) |
| 状態 | todo |
| 起票 | 2026-11-24（[GUIDE D-65](GUIDE.md#d-65-数値計算基盤と-numpy-互換機能案-c-ハイブリッドnc01nc14)。HEAD `27069465` で確認） |
| 承認 | 要承認（`Tensor.View` の負ストライド・0 ストライド・軸数上限 64・`Tensor.Mut` 不透明型登録。決定事項 D1・D2 が要承認、ほかは既定案） |
| 手本にする既存実装 | `std/Tensor.tz`、`std/MatrixView.tz`（`Mut<'a> { r }`・`of_array_mut`・`strided`・`freeze`・`copy_from`・`last_index` のオーバーフロー安全検証）、`src/stdlib.rs`（`opaque_record`） |
| 主な影響ファイル | `std/Tensor.tz`、`src/stdlib.rs`、`src/check.rs`（`Type::is_noncopy_record` の同期）、`tests/tensor.rs`、`tests/tensor-cases.mjs`、`tests/std_argument_order.rs`、`tests/std_examples.rs`、`tests/features.mjs`、`tests/fixtures/effects_sweep/`、`tests/fixtures/tensor/`、`_tsuzuri/language-reference/built-in-types-and-modules/tensor.md`、`docs/language.md` |

## 実装者向け早見

- 範囲: 現行の `std/Tensor.tz` の機能不足を解消し、NumPy `ndarray` に相当する N 次元配列コアへ引き上げる:
  1. 軸数上限を 16 → 64 に拡張（`std/Tensor.tz` の不変条件コメント、`tests/tensor-cases.mjs:197` の `rank_limit`、`tensor.md` の 10・159・233 行を同時更新）。
  2. `Tensor.View<'a> { r }` で**負のストライド**（逆順・反転ビュー）と **`stride == 0`**（ブロードキャスト軸）を安全に許可する（全要素が `0 <= index < data.length` に収まる不変条件をオーバーフローなしで検証）。
  3. N 次元の書き込み可能ビュー `Tensor.Mut<'a> { r }`（単射性＝エイリアシングなしを保証する配置のみ許可: 重複要素を生む stride=0 や重なりを拒否）を追加する。Tsuzuri ではレコードフィールドへの `ref mut` 借用が `E1014` のため（`src/ownership.rs:7314`）、`MatrixView.of_array_mut` と同じく `let mut` ローカル配列から構築する `borrow_mut` と、所有 `Tensor<'a>` の内部バッファを move out → `Mut` 貸出 → 再ラップする **`Tensor.update`** を提供し、`freeze`・`write`・`fill`・`copy_from`・`sub_mut`・`index_axis_mut`・`permute_mut`・`narrow_mut` を揃える。
  4. ブロードキャスト機構: `broadcast_shape :: ref [i64] -> ref [i64] -> Result<[i64], Error>`、`broadcast_to`（O(rank) ゼロコピー stride=0 ビュー）、`broadcast_arrays`。
  5. C 順・Fortran 順の判定と連続化（`is_c_contiguous`, `is_f_contiguous`, `as_contiguous`, `as_fortran`）、明示的複製 `clone`、型変換 `astype`（`map` ベースの変換補助）。
  6. 基本生成 API: `zeros`, `ones`, `full`, `zeros_like`, `ones_like`, `full_like`, `scalar`, `arange`, `linspace`, `linspace_open`, `logspace`, `geomspace`, `eye`, `identity`, `diag`, `diag_flat`, `tri`, `tril`, `triu`, `meshgrid`。
  7. 型クラスインスタンス: `Eq<Tensor<'a>>`（形状と全要素一致）、`Display<Tensor<'a>>` / `Display<View<'a>>`（インデント・桁揃え・大規模テンソルの `...` 省略表示）。
- 最初に読む: ① `std/Tensor.tz` 全文 ② `std/MatrixView.tz` の `Mut` と `last_index` ③ 仕様 1〜6。
- 最初に書くテスト: `tests/tensor.rs` で負ストライドの境界検査（最小・最大インデックス計算がオーバーフローせず範囲外をトラップ）、`broadcast_to`、`Tensor.Mut` の重複スロット拒否と借用検査（`E1013`/`E1014`）。
- 触ってはいけない: 既存の `Tensor.*` 関数のシグネチャと動作、`Tensor` を使わないプログラムの生成 IR。
- 完了の見分け方: 負ストライド反転・`broadcast_to`・`Tensor.Mut` への書き込み・`linspace`/`eye`/`zeros`・`Display` が native/WASM `-O0`/`-O3` と所有権検査のすべてを通る。

## 目的

現行の [Tensor.tz](../std/Tensor.tz) は読み取り専用ビューと基本操作のみを持ち、`Tensor.Mut`・負ストライド・ブロードキャスト・生成関数群・`Eq`/`Display` を欠いている。これらを `std/Tensor.tz` のコアとして確立することで、後続の要素ごと演算・集計・線形代数・FFT・シリアライズのすべてが統一された N 次元データ構造の上で動作できるようにする。

## 着手条件と停止条件

### 着手条件

- 決定事項 D1・D2 の承認。
- `grep -n "Tensor.View" src/` でコンパイラ側が `Tensor.Tensor`・`Tensor.View` を名指している箇所（`src/stdlib.rs`, `src/check.rs` 等）をすべて控える。

### 停止条件

- **S1** 負ストライドまたは `Tensor.Mut` の境界・単射性検査にオーバーフローや範囲外アクセスを許す抜け道が見つかり、安全な検査式が確定できない。
- **S2** `Tensor.Mut` のライフタイム規則が既存の借用検査（A12/A13/A31）と衝突して `E1013`/`E1014` の健全性が壊れる。
- **S3** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `27069465` で確認）

- `std/Tensor.tz`: `Tensor<'a>` と `View<'a> { r }` の 2 型のみ。`volume` で `shape.length <= 16` に固定。ストライドはすべて非負のみ想定。`Mut` は未実装。`Eq`/`Display` インスタンスなし。
- `src/stdlib.rs`: `opaque_record` に `"Tensor.Tensor"`・`"Tensor.View"` が登録済み。
- `src/check.rs`: `Tensor.Tensor`・`Tensor.View` を非 Copy 判定に含めている（1 箇所）。`"Tensor.Mut"` も同様に登録が必要。

## 変えてはいけない意味

- 既存の `Tensor.of_array`/`init`/`view`/`borrow`/`permute`/`index_axis`/`narrow`/`reshape`/`reshape_view`/`fold`/`map`/`to_tensor`/`as_matrix_view`/`of_matrix_view` の意味と引数順序（D-59）。
- 範囲外アクセス・形状不一致の安全検証（未検査メモリ読み書きは一切許さない）。

## 仕様

### 1. 境界検査と負/0 ストライド（`Tensor.View`）

- 軸数上限を `64` とする（`shape.length <= 64`）。`std/Tensor.tz` のレコードコメント（「at most 16 axes」「strides are never negative」）を新不変条件へ更新する。
- 任意のストライド `strides[d]`（負・0・正）を許す `Tensor.strided { r } :: own i64 -> own [i64] -> own [i64] -> ref { r } ['a] -> View<'a> { r }` を追加する。
- 空でないビュー（`volume > 0`）に対し、各軸 `d` の最小寄与 `min_d = if strides[d] < 0 then (shape[d] - 1) * strides[d] else 0`、最大寄与 `max_d = if strides[d] > 0 then (shape[d] - 1) * strides[d] else 0` を `i64` オーバーフロー検査付きで合算し、`0 <= offset + sum(min_d)` かつ `offset + sum(max_d) < data.length` を満たさない場合はトラップする。
- `stride == 0` は読み取り専用ビュー `View` のみで許可する（ブロードキャストの基盤）。
- 既存の `Tensor.as_matrix_view` は `MatrixView.strided`（`MatrixView.last_index` が `row_stride >= 0 && col_stride >= 0` を assert）へ委譲するため、負ストライドを持つ 2D `Tensor.View` に対して `as_matrix_view` を呼ぶとトラップする契約を維持・明記する（負ストライドを `MatrixView` へ渡す前に `to_tensor` 等で連続化する）。

### 2. N 次元可変ビュー（`Tensor.Mut<'a> { r }`）

- `record Mut<'a> { r } { data: ref mut { r } ['a..], offset: i64, shape: [i64], strides: [i64] }`（不透明レコード。`src/stdlib.rs` と `src/check.rs` へ登録）。
- 単射性（異なる座標が同じ `data` スロットを指さないこと）を保つ操作だけを提供する:
  - 構築・更新入口:
    - `borrow_mut { r } :: own [i64] -> ref mut { r } ['a..] -> Mut<'a> { r }`（`MatrixView.of_array_mut` と同じ流儀で `let mut` ローカル配列から可変ビューを構築する）。
    - `update :: own (ref mut Mut<'a> -> unit) -> own Tensor<'a> -> Tensor<'a>`（所有 `Tensor<'a>` の `shape`・`data` を move out して `Mut` をコールバックへ貸し出し、完了後に再ラップして返す。レコードフィールドへの直接 `ref mut` 借用は `E1014` のため `view_mut :: ref mut Tensor -> Mut` は設けない）。
  - 変換（単射性を保存する操作のみ）: `permute_mut`, `index_axis_mut`, `narrow_mut`, `step_mut`（`step != 0`）, `flip_mut`（負ストライド反転）, `freeze`（`ref { r } Mut<'a> { r } -> View<'a> { r }`）。`broadcast_to` や `stride == 0` は `Mut` には提供しない。
  - 更新: `write :: own [i64] -> own 'a -> ref mut Mut<'a> { r } -> unit`、`fill`、`copy_from`（同形状の `View` から複写）、`map_in_place`、`is_contiguous_mut :: ref Mut<'a> { r } -> bool`、`as_slice_mut { r } :: ref mut { r } Mut<'a> { r } -> ref mut { r } ['a..]`（連続でない場合はトラップ。排他参照は `Maybe` に入れられないため、`MatrixView.row_mut` と同じく事前条件 `is_contiguous_mut` で判定してから呼ぶ契約とする）。

### 3. ブロードキャスティング

- NumPy 互換の右揃えブロードキャスト規則:
  - 2 形状を末尾（最内軸）から比較し、各軸が「等しい」または「一方が `1`」であれば互換。出力長は `max(a, b)`。
  - 互換でない場合、`broadcast_shape` は `Result.Error (IncompatibleBroadcast ...)` を返す。
- `broadcast_to :: own [i64] -> ref View<'a> -> Result<View<'a>, Error>`（ゼロコピー。長さが `1` から拡張された軸および先頭に補われた軸の `stride` を `0` に設定。`regions.rs` の規則により `Result<View<'a> { r }, Error>` のように戻り値の一部へ `{ r }` を付けられないため、`{ r }` なしで唯一の借用引数から寿命を推論する `Array.try_findr` と同じ形を採る）。
- `broadcast_pair :: ref View<'a> -> ref View<'b> -> Result<(View<'a> * View<'b>), Error>`（2 つのビューを共通形状へゼロコピー拡張。両入力の共通借用寿命を持つ。独立した 2 region を保つ必要がある場面では各ビューに `broadcast_to` を個別に呼ぶ）。
- `broadcast_arrays :: ref [View<'a>] -> Result<[View<'a>], Error>`（複数の同型ビューを共通形状へ一括ブロードキャスト拡張したビュー配列を返す）。

### 4. メモリ順序・複製・型変換

- `is_c_contiguous`（既存 `is_contiguous` の別名／維持）、`is_f_contiguous`、`as_contiguous`（連続なら複製、または `to_tensor`）、`as_fortran`（Fortran 順の `Tensor` またはストライド設定）。
- `clone :: Copy<'a> => ref Tensor<'a> -> Tensor<'a>`、`clone_view`（`to_tensor` と同一）。
- `astype :: (Copy<'a>, Copy<'b>) => own ('a -> 'b) -> ref View<'a> -> Tensor<'b>`（`Tensor.map` の明示変換向け別名または `to_f64`/`to_f32`/`to_i64` 等のよく使う具象変換）。

### 5. 基本生成 API

- `zeros :: Default<'a> => own [i64] -> Tensor<'a>`、`ones :: Numeric<'a> => own [i64] -> Tensor<'a>`、`full :: Copy<'a> => own 'a -> own [i64] -> Tensor<'a>`（`Complex<'a>` は組み込み `Numeric` 印を持たないため、複素数の 1 埋めテンソルは `Tensor.full (Complex.one()) shape` を使う）。
- `zeros_like`, `ones_like`, `full_like`（`ref View<'a>` の形状を引継ぎ。対象が最後）
- `scalar :: own 'a -> Tensor<'a>`（ランク 0）
- `arange_i64 :: own i64 -> own i64 -> own i64 -> Tensor<i64>`、`arange_f64 :: own f64 -> own f64 -> own f64 -> Tensor<f64>`、`arange_f32 :: own f32 -> own f32 -> own f32 -> Tensor<f32>`（`start`, `stop`, `step`。`step == 0` はトラップ。整数版は `step > 0 && stop <= start` または `step < 0 && stop >= start` で空テンソルを返し、距離を `i64u` 経由で計算して `stop - start` の符号付きオーバーフローを回避する。浮動小数点版は `Math.ceil ((stop - start) / step)` が有限かつ `0 <= count <= i64::MAX` に収まることを検証してから `start + (i as f64) * step` で生成する）。
- `linspace :: Float<'a> => own 'a -> own 'a -> own i64 -> Tensor<'a>`（両端含む、`count >= 0`）、`linspace_open`（終端除く）
- `logspace`, `geomspace`（`Elementary<'a>`）
- `eye :: Numeric<'a> => own i64 -> own i64 -> own i64 -> Tensor<'a>`（`rows`, `cols`, `k` 対角オフセット）、`identity :: Numeric<'a> => own i64 -> Tensor<'a>`
- `diag`（1D→2D 対角行列、2D→1D 対角抽出は `diagonal` として明確に型分離）、`tri`, `tril`, `triu`
- `meshgrid :: (Copy<'a>, Copy<'b>) => ref View<'a> -> ref View<'b> -> (Tensor<'a> * Tensor<'b>)`（`ij` / `xy` 指定版）

### 6. `Eq`・`Display`・`Error`

- `union Error = ShapeMismatch of ([i64] * [i64]) | IncompatibleBroadcast of ([i64] * [i64]) | InvalidAxis of (i64 * i64) | InvalidShape of [i64] | NonContiguous`（`deriving (Eq)` + `Display`）。
- `instance Eq<'a> => Eq<Tensor<'a>>`: 形状が等しく全要素が等しいとき `true`（`NaN` を含む浮動小数点は IEEE 754 規則で `false`）。
- `instance Display<'a> => Display<Tensor<'a>>`: 階層角括弧・行揃え・要素数 1000 超で各軸先頭 3 要素と末尾 3 要素を `...` 省略表示。

## 設計

- `std/Tensor.tz` はすべて純 Tsuzuri コードで実装し、ランタイム C/LL コードは増やさない（既定 WASM インポートなしを維持）。
- 境界検査のオーバーフロー回避は `Int.checked_mul` / `Int.checked_add` を使う。
- `tests/std_argument_order.rs` の `MODULES` の `Tensor` 行を `("Tensor", subject(&["Tensor", "View", "Mut"]))` へ更新する。
- 追加した全公開関数は `tests/fixtures/effects_sweep/`（または `tests/fixtures/tensor/`）で呼び出し、`tests/effects_alloc_conservative.rs` のコーパス網羅検査を通す。
- 既存の `to_tensor` / `fold` / `map` は負のストライドおよび `stride == 0` のビューをそのまま正しく走査できるよう、`offset + step * strides[axis]`（符号付き加算）と空テンソル（`total == 0` 時に `data[0]` を読まないよう安全構築）を強化する。

## 前提とする他チケットのインターフェース

- なし（独立して着手可能）。

## 他チケットへ提供するインターフェース

- `Tensor.Mut<'a> { r }`、負/0 ストライド `Tensor.View<'a> { r }`、`Tensor.Error`、`Tensor.broadcast_shape`/`broadcast_to`/`broadcast_pair`/`broadcast_arrays`、基本生成 API（[NC05](NC05-tensor-shape-indexing-and-masking.md)〜[NC14](NC14-python-buffer-protocol-and-dlpack-bindings.md) のすべてが利用）。

## 実装手順

1. **変更**: なし。**内容**: `std/Tensor.tz`、`src/stdlib.rs`、`src/check.rs` の参照箇所を確認し、既存テストのベースラインを控える。**確認**: `cargo test --locked --test tensor --test std_argument_order --test std_examples`。
2. **変更**: `std/Tensor.tz`, `src/stdlib.rs`, `src/check.rs`, `tests/std_argument_order.rs`, `tests/tensor-cases.mjs`。**内容**: 64 軸対応（`std/Tensor.tz` のコメントと `tests/tensor-cases.mjs:197` の `rank_limit`（旧 17/40 → 新 65 等）を更新）、負/0 ストライドのオーバーフロー安全な境界検査、`strided { r }`、`Tensor.Mut` 型と不透明登録・操作群（`borrow_mut`・`update`・事前条件検査型 `as_slice_mut`）、`is_f_contiguous`/`as_contiguous` を追加し、`MODULES` に `"Mut"` を足す。**確認**: T0、`tests/tensor.rs` の境界・借用（`E1013`/`E1014`/`E1022`）テスト、`node tests/tensor-cases.mjs` 相当の検証。
3. **変更**: `std/Tensor.tz`, `tests/tensor.rs`, `tests/fixtures/effects_sweep/`。**内容**: `Tensor.Error`、`broadcast_shape`/`broadcast_to`/`broadcast_pair`/`broadcast_arrays`、生成 API 群（`zeros`〜`meshgrid`）、`Eq`/`Display`/`clone` とコーパス網羅呼び出しを追加する。**確認**: T0、`cargo test --locked --test tensor --test std_argument_order --test std_examples --test effects_alloc_conservative`、`tsuzuri test --doc std`、T1。
4. **変更**: `tests/fixtures/tensor/`・`tests/features.mjs`、`_tsuzuri/language-reference/built-in-types-and-modules/tensor.md`（10・159・233 行の「16 本まで」記載を含む全面更新）、`docs/language.md`。**内容**: E2E 検証（native/WASM `-O0`/`-O3`、`live == 0`）と文書更新。**確認**: T2、`node scripts/check-docs.mjs`。

## テスト計画

- **Rust テスト（`tests/tensor.rs`・`tests/tensor-cases.mjs`）**: 64 軸正常系と 65 軸トラップ（`tests/tensor-cases.mjs:197` の `rank_limit` を 17/40 から 65 以上へ更新）、負ストライドの `min`/`max` インデックス計算とオーバーフロー境界、負ストライドビューへの `as_matrix_view` トラップ、0 ストライドの `broadcast_to` / `broadcast_pair` / `broadcast_arrays`、非互換形状の `Result.Error`、`Tensor.Mut`（`borrow_mut` / `update`）のエイリアシング拒否（`E1014`）・寿命超過拒否（`E1013`）・不透明拒否（`E1022`）、`arange_i64`（`i64::MIN` 付近と空範囲）・`arange_f64`・`linspace`・`eye`・`tri`・`meshgrid`、`Eq`/`Display` の省略表示。
- **E2E（`tests/features.mjs`）**: 負ストライド反転・ブロードキャスト・`Mut` 書き込み・生成 API の実行結果とヒープ解放（`live == 0`）を native / WASM の `-O0`・`-O3` で確認。

## ドキュメント

- `_tsuzuri/language-reference/built-in-types-and-modules/tensor.md`（10・159・233 行の 16 軸上限記載および「できないこと」の該当制限を更新し、新 API・`Tensor.Mut`・`update`・`as_matrix_view` の負ストライド制約・ブロードキャストを記述）
- `docs/language.md`

## 受け入れ条件

- 既存 `Tensor` API の後方互換が保たれ、`Tensor` 未使用プログラムの IR が変化しないこと。
- 負ストライド・`stride == 0`・`Tensor.Mut` でメモリ安全性と借用健全性が保たれること。
- `std_argument_order`・`std_examples`・`effects_alloc_conservative`・T2・`node scripts/check-docs.mjs` がすべて通ること。

## 落とし穴

- レコードフィールドへの `ref mut` 借用は `let mut` のレコード変数に対しても `E1014`（`src/ownership.rs:7314`）で拒否されるため、`view_mut :: ref mut Tensor<'a> -> Mut<'a>` は書けない。`let mut` ローカル配列から `borrow_mut` で作るか、所有 `Tensor` に対して `Tensor.update`（フィールドを move out して `Mut` を貸し再ラップ）を使うこと。
- `regions.rs` の制約上、`Result<View<'a> { r }, Error>`・`[View<'a> { r }]`・`Result<(View<'a> { r } * View<'b> { s }), Error>` は `E1013` になるため、`Result` や配列・タプルで包む関数では名前付き region `{ r }` を付けない（単一借用引数なら自動で寿命推論される）。
- 排他参照 `ref mut` は `Maybe`・`Result`・タプル・配列の要素に入れられない（`check.rs`）。`as_slice_mut` は `MatrixView.row_mut` と同じく直接 `ref mut { r } ['a..]` を返す。
- `as_matrix_view` は `MatrixView.last_index` が `row_stride >= 0 && col_stride >= 0` を要求するため負ストライドビューでトラップする。また `tests/tensor-cases.mjs:197` の `rank_limit` は 17/40 軸のトラップを期待しているため、上限を 64 に広げるときに同時更新すること。
- 空テンソル（長さ 0 の軸あり、かつ他の軸が $2^{62}$ のような巨大値）ではループに入らず即座に完了すること（[C11](_completed/C11-multidimensional-arrays.md) の `huge_empty` 回帰を維持）。

## 対象外

- 構造化配列（record dtype）・実行時動的 dtype（`DynTensor`）・マスク配列（`numpy.ma`）。
- 形状操作・マスク抽出・要素ごと演算・軸指定統計（それぞれ [NC05](NC05-tensor-shape-indexing-and-masking.md)・[NC06](NC06-tensor-ufuncs-and-elementwise.md)・[NC07](NC07-tensor-reductions-stats-and-sorting.md) が担当）。

## 決定事項

- **D1（要承認）**: `Tensor.Mut<'a> { r }` を不透明レコードとして `std/Tensor.tz` と `src/stdlib.rs`/`src/check.rs` に追加し、軸数上限を 16 から 64 に引き上げる。
- **D2（要承認）**: `broadcast_to`・`broadcast_shape`・`broadcast_pair`・`broadcast_arrays` は失敗をトラップではなく `Result<_, Tensor.Error>` で返す（形状ミスマッチを実行時に回復可能にするため）。範囲外の単一添字 `at`/`write` および非連続の `as_slice_mut` は既存契約どおりトラップとする。
- **D3（既定案）**: `Tensor.Mut` は単射性（異なる論理座標が同じ物理スロットを指さないこと）を不変条件とし、`stride == 0` は読み取り専用 `View` のみ許可する。
- **D4（既定案）**: `Display` の省略閾値は既定 1000 要素（NumPy の `threshold=1000`, `edgeitems=3` 相当）とし、グローバル可変設定（`set_printoptions`）は置かず、必要なら明示的な `format_with` 関数でオプションレコードを受け取る。
- **D5（既定案）**: `arange` は型ごとの差分オーバーフローと符号なし減算ラップを確実に防ぐため、`arange_i64`・`arange_f64`・`arange_f32` の具象関数で提供する。
