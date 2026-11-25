# NC06: ブロードキャスト対応の要素ごと演算（ufunc）・比較・インプレース出力・演算子インスタンス

| 項目 | 内容 |
| --- | --- |
| ID | NC06 |
| 優先度 | P1（多次元配列の四則・数学・比較・論理演算の中核） |
| 規模 | L（ブロードキャスト反復エンジン、算術/比較/論理/ビット/数学 ufunc、`_into` ゼロ確保版と累積のインプレース版、同じ型どうしの `Tensor` 演算子インスタンス、[NC19](NC19-operator-extension-instances.md)・[NC20](NC20-compound-assignment-and-mutable-index-views.md) が呼ぶ非公開のトラップ版カーネル） |
| 依存 | [NC01](_completed/NC01-complex-and-extended-math.md), [NC02](_completed/NC02-tensor-core-views-and-broadcasting.md), ([NC03](_completed/NC03-borrowed-arithmetic-operators.md)), ([A37](A37-index-base-place-borrow.md))（演算子インスタンスの段（手順 3）は A37 の手順 5 の後） |
| 後続 | [NC07](NC07-tensor-reductions-stats-and-sorting.md), [NC08](NC08-linalg-core-and-einsum.md), [NC13](NC13-tensor-simd-parallel-and-gpu-kernels.md), [NC19](NC19-operator-extension-instances.md), [NC20](NC20-compound-assignment-and-mutable-index-views.md) |
| 状態 | todo |
| 起票 | 2026-11-24（[GUIDE D-65](GUIDE.md#d-65-数値計算基盤と-numpy-互換機能案-c-ハイブリッドnc01nc14)。HEAD `27069465` で確認）。2026-11-25 更新（[GUIDE D-66](GUIDE.md#d-66-自然な添字範囲演算子d-65-の改訂a37nc16nc20)。HEAD `858a949c` で確認。差分は末尾の「改訂（2026-11-25）」。同日、D-66 の委任による決定を反映） |
| 承認 | 不要（既定案のみ。演算子インスタンスの非消費の契約は完了済みの [NC03](_completed/NC03-borrowed-arithmetic-operators.md) の D1-A に従う。`View`・混在・スカラーとの演算子に要る言語の拡張とその承認は [NC19](NC19-operator-extension-instances.md) が扱う。NC19 の D1 は承認済み（D-66 の委任で決定、2026-11-25）） |
| 手本にする既存実装 | `std/Tensor.tz`（`map` の連続高速経路＋汎用ストライド走査）、`std/Matrix.tz`（`add`）、`std/MatrixView.tz`（`map_in_place`）、`std/Complex.tz`（条件付きの算術インスタンス `instance (Float<'a>, Add<'a>, Copy<'a>) => Add<Complex<'a>>`、354 行付近） |
| 主な影響ファイル | `std/Tensor.tz`、`std/Matrix.tz`、`tests/tensor_ufunc.rs`（新規）、`tests/std_argument_order.rs`、`tests/std_examples.rs`、`tests/features.mjs`、`tests/tensor-cases.mjs`、`tests/fixtures/effects_sweep/`、`tests/fixtures/tensor/`、`_tsuzuri/language-reference/built-in-types-and-modules/tensor.md`、`_tsuzuri/language-reference/built-in-types-and-modules/matrix.md` |

## 実装者向け早見

- 範囲: (1) 2 入力・3 入力のブロードキャスト対応反復関数（`map2`/`map2_into`/`map3`/`map3_into`）を実装し、同形状かつ連続な場合は 1D スライス直走査の高速経路、それ以外はストライド反復経路へ自動分岐する。(2) NumPy ufunc 相当の要素ごと関数群（四則・`floor_divide`/`floor_mod`/`divmod`/`fmod`/`ieee_remainder`・スカラー演算・`neg`/`abs`/`pow`/`rem`/`fma`・ビット演算・論理演算・要素ごと比較 `eq`/`ne`/`lt`/`le`/`gt`/`ge`・`allclose`/`isclose`/`array_equal`・`isnan`/`isinf`/`isfinite`/`nan_to_num`・全 `Math.*` および `Complex.*` のテンソル版）を提供する。(3) ゼロアロケーションのインプレース版（`add_into`/`sub_into`/`mul_into`/`div_into`/`mul_add_into` 等。出力先 `ref mut Mut<'c>` は `Tensor.borrow_mut` または `Tensor.update` から得る）と、出力先そのものに足し込む累積のインプレース版（`add_in_place` 等。D8）と、汎用 `ufunc` 還元（`outer`/`accumulate` 基盤）を揃える。(4) 同じ型どうしの `Tensor<'a>` に `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Pow`/`Neg` インスタンスを提供する（[NC03](_completed/NC03-borrowed-arithmetic-operators.md) は完了済み）。`View` どうし・`Tensor` と `View`・スカラーとの演算子は [NC19](NC19-operator-extension-instances.md)、複合代入 `+=` などは [NC20](NC20-compound-assignment-and-mutable-index-views.md) が担当する（D5）。(5) NC19・NC20 の演算子が呼ぶ非公開のトラップ版カーネル `*_or_trap`（仕様 5・D6）と、スカラーが左にも右にも来られる両側のスカラー版（仕様 2・D7）を用意する。
- 最初に読む: ① [NC02](_completed/NC02-tensor-core-views-and-broadcasting.md) の `broadcast_pair` と `Tensor.Mut` ② 仕様 1〜5 ③ `std/Tensor.tz` の `map` / `gather` / `map_in_place`（確保 0 回にならない書き方の例。設計を参照）。
- 最初に書くテスト: `tests/tensor_ufunc.rs` で同形状連続・非連続（転置/負ストライド）・ブロードキャスト（例: `[10, 100]` と `[10, 1]`、スカラー 0D と 2D）・非互換形状エラー・`add_into` の確保 0 回（`Alloc.snapshot` と `Alloc.since` で検証）。
- 触ってはいけない: 数値の丸め・NaN・符号付きゼロ・整数折り返し規則（暗黙 FMA や `fast-math` 禁止）。`Matrix.mul`（行列積）と要素ごと乗算（`Matrix.hadamard` / `Tensor.mul`）を混同しないこと（D2）。二項関数の引数順は常に `left -> right`（`Tensor.sub left right = left - right`、`Tensor.div left right = left / right`）を守ること（D4）。演算子の型付け（`binary_type` は両辺の型を単一化する。`src/check.rs:17012`）とスカラー・SIMD・`string` の演算子の IR を変えないこと（NC06 は std にインスタンスを足すだけ。言語の拡張は NC19）。
- 完了の見分け方: `(x - mean) / std`（`x`・`mean`・`std` はどれも `Tensor<f64>`。`mean` と `std` は `[1, n]` などブロードキャストできる形）および `Tensor.sub x mean` → `Tensor.div diff std`・`Tensor.add_into`・`Tensor.add_in_place`・`Tensor.sin`・`Tensor.allclose` がすべて native/WASM `-O0`/`-O3` で一致し、演算子 `a - b` と関数 `Tensor.sub` が同じ値を返す。

## 目的

現状の `Tensor` には要素ごとの算術・比較・数学関数が一切なく、`Matrix` にも `add` しかない。ブロードキャスト対応の要素ごと演算とインプレース出力先指定（`_into`）を整備することで、NumPy の ufunc 相当の計算を安全かつ高効率に書けるようにする。演算子は同じ型どうしの `Tensor` だけをこのチケットで足し、`View` やスカラーとの演算子（NC19）と複合代入（NC20）が同じ要素ループを呼べるように、非公開のカーネルの層を用意する。

## 着手条件と停止条件

### 着手条件

- [NC01](_completed/NC01-complex-and-extended-math.md) と [NC02](_completed/NC02-tensor-core-views-and-broadcasting.md) が done（HEAD `858a949c` で done）。
- `Tensor` の `Add`〜`Neg`・`Pow` インスタンス（手順 3）が前提にする [NC03](_completed/NC03-borrowed-arithmetic-operators.md)（D1-A: 非組み込み型の算術インスタンスは引数を共有借用で受ける）も done。
- 手順 3（同じ型どうしの演算子インスタンス）は、[A37](A37-index-base-place-borrow.md) の手順 5（算術クラスの orphan 規則）が done になってから入れる（弱い依存）。手順 1・2 は A37 と無関係に着手できる。手順 4 の演算子の部分は手順 3 の後。理由: この順なら、利用者の `instance Add<Tensor<f64>>` は NC06 との重なりではなく A37 の `orphan_instance` で一度だけ拒否され、移行の案内（help）も 1 つになる（NC19 の D10）。
- [NC16](NC16-index-ref-and-zero-alloc-element-reads.md)（土台を借りる添字に `ref` を必須にする）の前でも後でも着手できる。変わるのはテストで窓を角括弧で作るときの書き方だけ（落とし穴）。

### 停止条件

- **S1** `Matrix` の `*` 演算子を行列積にするか要素ごと積（Hadamard 積）にするかで既存規約と矛盾が生じる場合（D2 に従い、`Tensor` の `*` は要素ごと積、`Matrix` は要素ごと積を `hadamard`・行列積を `Matrix.mul` と明確に分け、`Matrix` への `Mul` インスタンスは誤解を避けて付けない既定案を採る）。
- **S2** GUIDE-LITE §9 の共通の条件。
- **S3** 同じ型どうしの演算子インスタンスを足すだけで、コンパイラ（演算子の型付け、スカラー・SIMD・`string` の演算子の IR、所有権検査）を変える必要が出た場合。NC06 は std だけを変える。コンパイラの変更は NC19 の範囲なので、止めて報告する。

## 現状（HEAD `858a949c` で確認）

- `std/Tensor.tz`（2901 行）: 要素ごとの二項演算・比較・数学関数・`*_into` は未実装。あるのは単項の `map`（460 行付近）・`fold`（416 行付近）・`map_in_place`（803 行付近）、ブロードキャストの `broadcast_shape`/`broadcast_to`/`broadcast_pair`/`broadcast_arrays`（970〜1061 行付近。[NC02](_completed/NC02-tensor-core-views-and-broadcasting.md)）、3 入力の `where_select`（2723 行付近）・`where_scalar`・`clip`（[NC05](_completed/NC05-tensor-shape-indexing-and-masking.md)）。
- 演算子のインスタンスは `Eq<Tensor<'a>>`（1514 行付近）と `Display` だけで、`Add`〜`Neg`・`Pow` はない。`Index`/`IndexMut` のインスタンスは 1660〜1684 行付近とマスクの 2883〜2896 行付近（[NC04](_completed/NC04-generic-index-class-and-slice.md)・NC05）。
- 演算子は同じ型どうしだけ: `binary_type` は両辺の型を単一化してから（`src/check.rs:17012`）クラスを要求し、結果は左辺の型（17022 行付近）。dev バイナリで `t - 1`（`t: Tensor<i64>`）は `E1003`（`expected Tensor<i64>, found i32`）、`let a = t[[Slice.from 1]]`・`let b = t[[Slice.until (-1)]]` の `a - b` は `E1005`（`no instance for Sub<Tensor.View<i64>>`）。どちらも NC19 の範囲。
- NC03 の D1-A により、非組み込み型の算術インスタンスは引数を共有借用で受ける（`src/polymorph.rs:943` の `borrowed_arithmetic_class`）。利用者のモジュールに `instance (Add<'a>, Copy<'a>) => Add<Tensor<'a>>` を書くと、`let c = a + b; let d = a + c` が `a` を消費せずに動く（dev バイナリで確認。NC06 はこれを std に置く）。A37 の手順 5 の後は、このインスタンスは std の型への利用者の算術インスタンスなので `E1016`（kind `orphan_instance`）になる。
- 確保（`--allocator counting` の dev バイナリ、`-O0`/`-O3` で同じ）: `map_in_place` は呼び出しごとに shape と strides を `Array.copy` する（811〜812 行付近。rank 2 で 2 回・32 バイト）。`view` も 2 回（shape の複製と `dense_strides`、208 行）。`Tensor.update` は何もしない writer でも要素を複製する（`[1000]` の `i64` で 7 回・16,040 バイト。646〜647 行付近の `let shape = tensor.shape`・`let mut data = tensor.data`。同じ形の利用者のレコードでも、`own` の引数から `let data = tensor.data` で取り出すだけで 2 回・8,008 バイトを確保した）。文書は既知の制限として直した（このチケット群の PR。HEAD の 636 行付近の「without copying」）。直すのは [PM10](../_perfs/PM10-std-consume-copies.md)。
- `std/Matrix.tz`: `add` と行列積 `mul`/`mul_fma`/`mul_parallel`/`mul_fma_parallel`、`map`・`fold`・`transpose`・`set`。要素ごとの `sub`・`hadamard`・`scale` はない。
- `tsuzuri build --emit llvm` は Clang に渡す前の IR をそのまま書く（`src/driver.rs:4651` 付近）。

## 変えてはいけない意味

- 浮動小数点の丸め・`NaN`・`±0.0`、整数の折り返しとゼロ除算トラップ。
- `Matrix.mul` の行列積仕様とビット一致契約。
- 演算子の型付けと、スカラー・SIMD・`string` の演算子の IR（NC06 はコンパイラを変えない）。
- 非組み込み型の算術インスタンスが引数を共有借用で受ける契約（NC03 D1-A）。`a + b` の後も `a`・`b` を使える。

## 仕様

### 1. ブロードキャスト二項・三項エンジン

- `map2 :: (Copy<'a>, Copy<'b>) => own ('a -> 'b -> 'c) -> ref View<'a> -> ref View<'b> -> Result<Tensor<'c>, Error>`
- `map2_into :: (Copy<'a>, Copy<'b>) => own ('a -> 'b -> 'c) -> ref View<'a> -> ref View<'b> -> ref mut Mut<'c> { r } -> Result<unit, Error>`（出力先形状とブロードキャスト形状が一致しない場合は何も書かずに `Result.Error`）
- `map_into :: Copy<'a> => own ('a -> 'b) -> ref View<'a> -> ref mut Mut<'b> { r } -> Result<unit, Error>`
- `map3` / `map3_into`（`fma`・`where_select` 等の 3 入力 ufunc 用。`where_select` は [NC05](_completed/NC05-tensor-shape-indexing-and-masking.md) が実装済み（`std/Tensor.tz` 2723 行付近）なので、`map3` に載せ替えるときも署名と `Result` の契約を変えない）
- `map2_in_place { r } :: (Copy<'a>, Copy<'b>) => own ('a -> 'b -> 'a) -> ref View<'b> -> ref mut Mut<'a> { r } -> Result<unit, Error>`（`target_i = f target_i rhs_i`。`rhs` だけを `target` の形へブロードキャストし、`target` の形は変えない。ブロードキャストできなければ何も書かずに `Result.Error`。D8）
- 高速経路: 両入力（および出力）が同形状かつ C 連続のとき、多次元ループを省略し 1D スライス直走査で LLVM の自動ベクトル化が効く形にする。両入力の形が同じときは `broadcast_pair` を呼ばない（共通の形と 2 つの窓の確保を省く）。

### 2. 要素ごと算術・スカラー・ビット・論理演算

- 二項（すべてブロードキャスト対応。公開の関数は形状不一致を `Result<Tensor<_>, Error>` の `Result.Error` で返す。トラップするのは演算子だけで、演算子は仕様 5 の非公開のトラップ版カーネルを呼ぶ。公開の `*_exn` やトラップ版の関数は作らない: D1・D6）:
  - `add`, `sub`, `mul`, `div`, `rem`（0 方向丸め剰余）, `floor_divide`（床除算。NumPy `np.floor_divide` 相当）, `floor_mod`（床剰余。NumPy `np.remainder` / `np.mod` 相当）, `divmod`（`(floor_divide, floor_mod)`）, `fmod`（C99 0 方向浮動小数点剰余）, `ieee_remainder`（IEEE 754 最近接偶数商の剰余。NumPy `np.remainder` との混同を避けるため `remainder` 名は使わない）, `pow`, `maximum`, `minimum`, `fmax`, `fmin`, `atan2`, `hypot`, `copysign`, `logaddexp`, `logaddexp2`
  - ビット演算（`Bits<'a>`）: `bit_and`, `bit_or`, `bit_xor`, `bit_not`, `shl`, `shr`
  - 論理演算（`bool`）: `logical_and`, `logical_or`, `logical_xor`, `logical_not`
- スカラー演算（対象ビューが最後: D-59。スカラーが左に来る形と右に来る形を別の関数にし、演算子の左右の順序を要素の式でもそのまま保つ: D7）。署名はどれも `(<Op><'a>, Copy<'a>) => own 'a -> ref View<'a> -> Tensor<'a>`（`<Op>` は使う演算子のクラス）:

  | 関数 | 各要素の式 | NC19 の演算子の形 |
  | --- | --- | --- |
  | `add_scalar s x` | `x_i + s` | `x + s` |
  | `radd_scalar s x` | `s + x_i` | `s + x` |
  | `sub_scalar s x` | `x_i - s` | `x - s` |
  | `rsub_scalar s x` | `s - x_i` | `s - x` |
  | `mul_scalar s x` | `x_i * s` | `x * s` |
  | `scale s x` | `s * x_i` | `s * x` |
  | `div_scalar s x` | `x_i / s` | `x / s` |
  | `rdiv_scalar s x` | `s / x_i` | `s / x` |
  | `rem_scalar s x` | `x_i % s` | `x % s` |
  | `rrem_scalar s x` | `s % x_i` | `s % x` |
  | `pow_scalar s x` | `x_i ** s` | `x ** s` |
  | `rpow_scalar s x` | `s ** x_i` | `s ** x` |

- インプレース出力版（`_into`）: `add_into`, `sub_into`, `mul_into`, `div_into`, `scale_into`, `mul_add_into`（`out = a * b + c`、`fma_into` は明示 `Math.fma`）。出力先 `ref mut Mut<'c>` は [NC02](_completed/NC02-tensor-core-views-and-broadcasting.md) の `Tensor.borrow_mut`（`let mut` ローカル配列から構築）または `Tensor.update`（所有 `Tensor` の move-out 更新。HEAD では要素を複製する。[PM10](../_perfs/PM10-std-consume-copies.md)）で用意する。
- 累積のインプレース版（D8。[NC20](NC20-compound-assignment-and-mutable-index-views.md) の `+=` などが使う）: `add_in_place`, `sub_in_place`, `mul_in_place`, `div_in_place`, `rem_in_place`。署名は `{ r } (<Op><'a>, Copy<'a>) => ref View<'a> -> ref mut Mut<'a> { r } -> Result<unit, Error>` で、`target_i = target_i op rhs_i`（左が `target`、右が `rhs`）。`rhs` だけを `target` の形へブロードキャストし、できなければ何も書かずに `Result.Error`。`target` を広げる形（`[3]` の `target` に `[2, 3]` の `rhs`）も `Result.Error`。`rhs` と `target` が同じバッファを指す呼び出しは借用の規則で `E1014` になるので、重なりの順序は観測されない（[A37](A37-index-base-place-borrow.md) の修正が前提。HEAD では利用者のモジュールが `Index<Tensor.Mut<…>, …>` を定義すると、`Mut` から取った共有の窓が書き込み中も生きる）。`*_into` は出力が入力と重なる `x = x + y` を書けない（同じテンソルの窓と `ref mut` を同時に渡すと `E1014`）ので、このために別の関数にする。
- 単項数学 ufunc（`ref View<'a> -> Tensor<'a>` および `*_into`）:
  - `neg`, `abs`, `sqrt`, `cbrt`, `square`, `reciprocal`, `floor`, `ceil`, `trunc`, `round`（0 から遠い側へ丸め）, `round_even`（最近接偶数丸め。NumPy `np.round` / `np.rint` 相当）, `sign`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `sinh`, `cosh`, `tanh`, `asinh`, `acosh`, `atanh`, `exp`, `exp2`, `expm1`, `log`, `log2`, `log10`, `log1p`, `sinc`, `degrees`, `radians`
  - 複素数ビュー向け: `real`, `imag`, `conj`, `angle`（`Complex<'a>`）
- 特殊値検査・置換: `is_nan`, `is_infinite`, `is_finite`, `signbit`, `nan_to_num :: Float<'a> => own 'a -> own 'a -> own 'a -> ref View<'a> -> Tensor<'a>`

### 3. 要素ごと比較・近傍一致

- `elem_eq`, `elem_ne`, `elem_lt`, `elem_le`, `elem_gt`, `elem_ge`（ブロードキャスト対応で `Result<Tensor<bool>, Error>` を返す。スカラー比較の `gt_scalar` / `lt_scalar` / `ge_scalar` / `le_scalar` / `eq_scalar` も提供し `sub[Tensor.gt_scalar 0.0 sub]` を簡潔に書けるようにする。マスクの添字は複製を返すので `ref` を付けない。`ref` が要るのは結果が土台を借りる添字だけ: D-66d）
- `isclose :: Float<'a> => own 'a -> own 'a -> own bool -> ref View<'a> -> ref View<'a> -> Result<Tensor<bool>, Error>`（`rtol`, `atol`, `equal_nan`）
- `allclose :: Float<'a> => own 'a -> own 'a -> own bool -> ref View<'a> -> ref View<'a> -> bool`
- `array_equal`, `array_equiv`

### 4. `outer` と同じ型どうしの演算子インスタンス

- `outer_with :: (Copy<'a>, Copy<'b>) => own ('a -> 'b -> 'c) -> ref View<'a> -> ref View<'b> -> Tensor<'c>`（1D 入力の外積表。ufunc `outer` 相当）
- `Tensor<'a>` に同じ型どうしの借用ベースの `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Pow`/`Neg` インスタンスを追加（[NC03](_completed/NC03-borrowed-arithmetic-operators.md) D1-A のとおり引数は `ref Tensor<'a>`。ブロードキャスト適用、形状非互換はトラップ。インスタンス文脈には `instance (Add<'a>, Copy<'a>) => Add<Tensor<'a>>` のように使用する演算子クラスと `Copy<'a>` を明示する）。これにより `(x - mean) / std` がそのまま書ける（3 つとも `Tensor<'a>`）。
- 本体は仕様 5 の非公開のトラップ版カーネルを 1 回呼ぶだけにする（例 `fn sub left right = sub_or_trap (ref (view left)) (ref (view right))`。`Neg` は形の不一致が起きないので公開の `neg` を呼ぶ）。関数版と同じ要素ループを通るので、`a - b` の値は `Tensor.sub (ref (Tensor.view a)) (ref (Tensor.view b))` の `Ok` の値とビット単位で同じになる。
- 要素の演算は要素型の演算子そのもので、`l_i op r_i` を左右の順に計算する。融合・FMA・再結合をしない。`a * b + c` は所有の一時値を 2 つ作る（1 走査にしたいときは `mul_add_into` か `map2_into` を明示する）。`@checked` は演算子インスタンスの中の要素の演算を検査しない（「呼び出した関数の中までは検査しません」。`_tsuzuri/language-reference/values-and-functions/op-and-expressions.md` の「整数オーバーフローと @checked」）。
- NC06 が足すのは同じ型どうしの `Tensor<'a>` のインスタンスだけ（D5）。`View` どうし・`Tensor` と `View`・スカラーとの演算子（edition 2028 の範囲で書く `ref a[1..] - ref a[..^1]`、`2.0 * x`、`t + v`）は演算子の拡張インスタンスとして [NC19](NC19-operator-extension-instances.md) が足し、複合代入 `+=`・`-=`・`*=`・`/=`・`%=` は [NC20](NC20-compound-assignment-and-mutable-index-views.md) が足す。どちらも仕様 2・5 のカーネルを呼ぶ。
- orphan 規則との関係（[A37](A37-index-base-place-borrow.md) の D4）: 1 引数の算術インスタンスの振り分けの位置は T で、NC06 のインスタンスでは頭が `Tensor` になる。裸の型変数を持たないので、A37 のテスト `std_instances_with_open_dispatch_positions_are_listed` の一覧には足さない（一覧に載るのは、振り分けの位置に裸の型変数を持つ std のインスタンスだけ）。std のモジュールにはコンパイラの orphan 規則を当てず、置き場所は `src/stdlib.rs` の読み込みのテストが確かめる（`std/Tensor.tz` は `Tensor` を宣言する）。利用者が同じ頭のインスタンス（`instance Add<Tensor<f64>>`）を書くと、`orphan_instance` が `overlapping_instance` より先に検査されるので、A37 の `orphan_instance` になる。

### 5. 演算子から呼ぶ非公開のトラップ版カーネル（NC19・NC20 が使う）

- 名前の規則: `<name>_or_trap` は公開の `<name>` の非公開の双子で、引数は同じ。`<name>` が `Result.Error` を返す場面でトラップし、それ以外は `Ok` の中身を返す（`unit` の関数は `unit`）。トラップは `std/Tensor.tz` のほかの形の検査と同じ `assert` で書き（実行時の種類は `assertion failed`。`tests/features.mjs` の `tensor` スイートの `trapKind`）、出力の確保と書き込みの前に起きる。
- 置く関数（すべて `std/Tensor.tz` の `private def`）:

  ```text
  private def map2_or_trap :: (Copy<'a>, Copy<'b>) => own ('a -> 'b -> 'c) -> ref View<'a> -> ref View<'b> -> Tensor<'c>
  private def add_or_trap :: (Add<'a>, Copy<'a>) => ref View<'a> -> ref View<'a> -> Tensor<'a>
  private def sub_or_trap :: (Sub<'a>, Copy<'a>) => ref View<'a> -> ref View<'a> -> Tensor<'a>
  private def mul_or_trap :: (Mul<'a>, Copy<'a>) => ref View<'a> -> ref View<'a> -> Tensor<'a>
  private def div_or_trap :: (Div<'a>, Copy<'a>) => ref View<'a> -> ref View<'a> -> Tensor<'a>
  private def rem_or_trap :: (Rem<'a>, Copy<'a>) => ref View<'a> -> ref View<'a> -> Tensor<'a>
  private def pow_or_trap :: (Pow<'a>, Copy<'a>) => ref View<'a> -> ref View<'a> -> Tensor<'a>
  private def map2_in_place_or_trap { r } :: (Copy<'a>, Copy<'b>) => own ('a -> 'b -> 'a) -> ref View<'b> -> ref mut Mut<'a> { r } -> unit
  private def add_in_place_or_trap { r } :: (Add<'a>, Copy<'a>) => ref View<'a> -> ref mut Mut<'a> { r } -> unit
  private def sub_in_place_or_trap { r } :: (Sub<'a>, Copy<'a>) => ref View<'a> -> ref mut Mut<'a> { r } -> unit
  private def mul_in_place_or_trap { r } :: (Mul<'a>, Copy<'a>) => ref View<'a> -> ref mut Mut<'a> { r } -> unit
  private def div_in_place_or_trap { r } :: (Div<'a>, Copy<'a>) => ref View<'a> -> ref mut Mut<'a> { r } -> unit
  private def rem_in_place_or_trap { r } :: (Rem<'a>, Copy<'a>) => ref View<'a> -> ref mut Mut<'a> { r } -> unit
  ```

- 使い手: `add_or_trap`〜`pow_or_trap` は仕様 4 の同じ型どうしのインスタンスと、NC19 の拡張インスタンス（`View` どうし、`Tensor` と `View`）。`*_in_place_or_trap` は NC20 の複合代入。`map2_or_trap`・`map2_in_place_or_trap` は NC06 の中の土台。
- 要素ループの共有: 公開の関数とその双子は、同じ非公開の要素ループを呼ぶ。既定の形は、`map2` と `map2_or_trap` が形を確かめ（同じ形なら `broadcast_pair` を省く）、同じ非公開の `map2_kernel`（同じ形の窓だけを受け、連続高速経路とストライド経路を持つ）を呼ぶ。`add` は `map2 (\x y -> x + y)`、`add_or_trap` は `map2_or_trap (\x y -> x + y)` とする。Clang `-O3` の後に間接呼び出しが残る演算だけ、演算ごとの直接ループ（例 `add_kernel`）に置き換え、公開の関数と双子の両方からそれを呼ぶ（設計の「最適化後 IR の確認ルール」）。`map2_in_place`・`*_in_place` と双子も同じ形にする（`map2_in_place_kernel`）。
- 双子を作らないもの: スカラーとの演算（仕様 2 の表）と単項の `neg`・数学 ufunc は形の不一致が起きないので、公開の関数がそのままカーネルになる。NC19 のスカラーとの演算子は表の関数を、`-v` は `neg` を呼ぶ。
- 見える範囲: `private def` なので利用者は呼べない。NC19・NC20 の std のインスタンスは `std/Tensor.tz` に置くので呼べる。`tests/std_argument_order.rs` は `private def` の行を読まない（365 行付近のテスト `pure_definitions_remain_subject_to_argument_order_checks`）。
- 名前は [NC19](NC19-operator-extension-instances.md) の仕様 6 の表と [NC20](NC20-compound-assignment-and-mutable-index-views.md) の §5 と照合済み（2026-11-25 の統合。NC20 が足すスカラーの `*_scalar_in_place` は NC20 の名前）。改名するときは 3 つのチケットを同じ変更で直す。

## 設計

- 1D 連続高速経路は `ref data[offset ..< offset + total]` の単純 `for i in 0i64 ..< total` ループとする。新しく書く std のスライスは素の `..` を使わず `..<` か `..=` で書く（素の `a..b` のスライスは edition 2028 で終端を含む意味に変わり、組み込みの std は `Edition::LATEST` で読まれる（`src/driver.rs` 2784 行付近）ので、[NC17](NC17-edition-2028-inclusive-range-family.md) の Phase 3 で `LATEST` が 2028 になると意味が変わる。std には `W1013` が出ないので警告では気づけない: D-66c。`..<` はどの edition でも半開）。
- 単一パス複合処理（例: `Tensor.map_into (\x -> Math.exp (x - bias)) src out` のような減算＋超越関数の 1 走査化）は、`map_into` / `map2_into` で追加の中間配列を作らずに記述する方針とする（汎用式テンプレート融合は入れない）。
- **最適化後 IR の確認ルール**: `tsuzuri build --emit llvm -O3` は **Clang 実行前の未最適化 IR** を書き出す（`src/driver.rs:4651` 付近）。そのため `map` / `map_into` / `map2` へ渡したクロージャがインライン化・自動ベクトル化されるかの検証は、必ず **Clang `-O3` 通過後の IR（`clang -x ir -O3 -S -emit-llvm`）または生成オブジェクトの逆アセンブル**で確認する。主要二項・単項演算（`add`/`sub`/`mul`/`div`/`scale`/`fma` 等）で Clang `-O3` 後も間接呼び出しが残る場合のみ、クロージャを経由しない直接ループへ書き直す（[Matrix.tz](../std/Matrix.tz) の `multiply_rows` と同じ流儀）。
- 汎用ストライド経路は最内軸（`axis == rank - 1`）をタイトな内側ループにし、外側軸だけ再帰または明示スタックで走査する。
- `Alloc`（F14）の `Alloc.snapshot`・`Alloc.since` を使って `*_into`・`*_in_place` 関数が追加ヒープ確保 0 回であることをテストで構造的に表明する。
- **確保 0 回の書き方**: `*_into`・`*_in_place` のストライド経路は `map_in_place`（803〜815 行付近）を真似ない。`map_in_place` は shape と strides を `Array.copy` してから `view.data` を渡すので、呼び出しごとに 2 回確保する（現状）。`ref view.shape` と `view.data` を同じ呼び出しに渡すと `E1014` になるのがコピーの理由で、代わりに `ref mut Mut` そのものを再帰に渡し、各段で `view.shape[axis]`・`view.strides[axis]` をローカルに読み、`view.offset` は呼び出しの前にローカルへ読む（`Tensor.Mut` と同じ形の利用者のレコードで、この書き方が `-O0`/`-O3` とも確保 0 回になることを dev バイナリで確認した）。2 次元の手本は `MatrixView.map_in_place`（`std/MatrixView.tz` 396 行付近。スカラーのフィールドだけを読む）。ブロードキャストした入力の strides も `broadcast_to`（1006 行付近。strides の配列を確保する）で作らず、各段で `0` か元の stride を選ぶ。形の検査も配列を作らずに軸ごとに比べる。`Result.Error` の値を作る失敗の経路では確保してよい。
- **演算子の層**（仕様 5）: 公開の関数（`Result` を返す）・非公開の双子 `*_or_trap`（トラップする）・共有の要素ループの 3 層にする。演算子インスタンスの本体は双子を 1 回呼ぶだけで、要素ループを複製しない。公開の関数を包む形（`or_trap (sub l r)` のように `Result` を作ってから外す形）は採らない（D6）。
- `Tensor` の被演算子は `view` で窓にしてから双子へ渡す（1 つにつき shape の複製と strides で 2 回確保する）。同じ形の小さい `Tensor` どうしの演算子が遅いと計測で分かったときだけ、窓を作らずに `ref left.data`・`ref right.data` を連続経路のループへ渡す経路を足す。性能の主張は `docs/benchmarks.md` の手順で測ってからにする。

## 前提とする他チケットのインターフェース

- [NC01](_completed/NC01-complex-and-extended-math.md)（done）: `Complex<'a>`、新規 `Math.*` 関数群。
- [NC02](_completed/NC02-tensor-core-views-and-broadcasting.md)（done）: `Tensor.View`、`Tensor.Mut`、`broadcast_pair`、`Tensor.Error`。
- [NC03](_completed/NC03-borrowed-arithmetic-operators.md)（done）: 借用算術演算子クラス（D1-A。非組み込み型のインスタンスは引数を `ref` で受け、スカラー・SIMD・`string` の演算は今の IR のまま）。
- 任意 [NC16](NC16-index-ref-and-zero-alloc-element-reads.md): テストで窓を角括弧で作るときの `ref t[[Slice…]]`（D-66d）。NC16 の前なら関数形 `Tensor.slice` で書く。
- 弱い依存 [A37](A37-index-base-place-borrow.md)（手順 5）: 算術クラスの orphan 規則（kind `orphan_instance`）、`dispatch_positions`、テスト `std_instances_with_open_dispatch_positions_are_listed`、`docs/stability.md` の既知の例外「std への instance の追加」の項。手順 3 はこの後に入れる。

## 他チケットへ提供するインターフェース

- `map2`/`map2_into`/全要素ごと算術・比較・数学 ufunc・`allclose`（[NC07](NC07-tensor-reductions-stats-and-sorting.md)〜[NC13](NC13-tensor-simd-parallel-and-gpu-kernels.md) が利用）。
- [NC19](NC19-operator-extension-instances.md) へ: 非公開のトラップ版カーネル `add_or_trap`・`sub_or_trap`・`mul_or_trap`・`div_or_trap`・`rem_or_trap`・`pow_or_trap`（仕様 5）、両側のスカラー版 `add_scalar`・`radd_scalar`・`sub_scalar`・`rsub_scalar`・`mul_scalar`・`scale`・`div_scalar`・`rdiv_scalar`・`rem_scalar`・`rrem_scalar`・`pow_scalar`・`rpow_scalar`（仕様 2）、単項の `neg`。同じ型どうしの `Tensor<'a>` のインスタンス（NC19 の演算子表に同じ型どうしの項目として入る）。
- [NC20](NC20-compound-assignment-and-mutable-index-views.md) へ: 累積のインプレース版 `add_in_place`・`sub_in_place`・`mul_in_place`・`div_in_place`・`rem_in_place` と、その非公開の双子 `add_in_place_or_trap`〜`rem_in_place_or_trap`（仕様 2・5）。出力先を別に取る `*_into`。

## 実装手順

1. **変更**: `std/Tensor.tz`。**内容**: `map_into`/`map2`/`map2_into`/`map3`/`map3_into`/`map2_in_place` のブロードキャスト反復エンジン（連続高速経路＋汎用ストライド経路、共有の要素ループ `map2_kernel`・`map2_in_place_kernel`）と、非公開の双子 `map2_or_trap`・`map2_in_place_or_trap` を実装する。**確認**: T0、`tests/tensor_ufunc.rs`（`Alloc` で `_into`・`_in_place` の確保 0 回を表明。公開の関数と双子が同じ値を返し、形の不一致では公開の関数が `Result.Error`、双子がトラップする）。
2. **変更**: `std/Tensor.tz`, `std/Matrix.tz`, `tests/fixtures/effects_sweep/`。**内容**: 要素ごと四則（すべて `left -> right` 順）・両側のスカラー版（D7）・累積のインプレース版（D8）・ビット・論理・比較・`isclose`/`allclose`・`nan_to_num`・全数学 ufunc・`outer_with`、二項の算術の非公開の双子 `*_or_trap`・`*_in_place_or_trap`（仕様 5）、および `Matrix` の不足要素ごと演算（`sub`/`hadamard`/`scale`）とコーパス網羅呼び出しを実装する。**確認**: T0、`cargo test --locked --test std_argument_order --test std_examples --test effects_alloc_conservative`、`tsuzuri test --doc std`、T1。
3. **変更**: `std/Tensor.tz`。**内容**: [A37](A37-index-base-place-borrow.md) の手順 5 が done になってから、同じ型どうしの `Tensor<'a>` への借用 `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Pow`/`Neg` インスタンスを追加する（本体は双子を 1 回呼ぶだけ。D5・D6）。**確認**: T0、`(x - mean) / std` テスト、`cargo test --locked --test tensor_ufunc --test borrowed_arithmetic`（演算子と関数版の値と確保回数の一致、被演算子を消費しないこと）、`cargo test --locked --lib opt_in_modules_are_reached_only_through_their_names` と `cargo test --locked --lib std_instances_with_open_dispatch_positions_are_listed`（それぞれ `running 1 test`。一覧は変わらない）。
4. **変更**: `tests/fixtures/`・`tests/features.mjs`・`tests/tensor-cases.mjs`、文書。**内容**: native/WASM `-O0`/`-O3` E2E（形の合わない演算子のトラップを含む）とドキュメント更新。**確認**: T2、`node tests/features.mjs target/debug/tsuzuri tensor`、`node scripts/check-docs.mjs`。

## テスト計画

- **Rust テスト（`tests/tensor_ufunc.rs`）**: 同形状・1D/2D/3D/4D ブロードキャスト・0D スカラー・空テンソル・負ストライドビュー・非互換形状エラー・非可換二項演算（`sub left right == left - right`, `div left right == left / right`）・`isclose`/`allclose` の `NaN`/`Inf`/`equal_nan` 境界・`_into` の確保 0 回。
- **演算子（同じ型どうし）**: `a + b`〜`a ** b`・`-a` がブロードキャストし、関数版の `Ok` の値とビット単位で同じ（`NaN`・`±0.0`・整数の折り返しを含む）。`let c = a + b; let d = a + c` で `a` を消費しない（`live == 0`）。形の不一致はトラップ。`a + b` と `Tensor.add (ref (Tensor.view a)) (ref (Tensor.view b))` の確保回数が同じ（違えば原因を調べる）。std のソースを読み、各演算子インスタンスの本体が `*_or_trap` を 1 回呼ぶだけであることと、公開の関数と双子が同じ要素ループを呼ぶことを確かめる（`tests/std_argument_order.rs` と同じく `std/` のソースを読むテスト）。
- **orphan 規則との関係**（A37 の手順 5 の後）: 利用者のモジュールの `instance Add<Tensor<f64>>` は `E1016`（kind `orphan_instance`）で、NC06 の std の `Add<Tensor<'a>>` との `overlapping_instance` にならない。NC06 の 7 つのインスタンスは振り分けの位置に裸の型変数を持たないので、`std_instances_with_open_dispatch_positions_are_listed` の一覧は変わらずに通る。
- **スカラー版**: 非可換の `Mul` を持つ Copy のレコード（2×2 行列など）を要素にして、`mul_scalar s x` が `x_i * s`、`scale s x` が `s * x_i` を計算する。`radd_scalar`・`rrem_scalar`・`rpow_scalar` も左右の順序を確かめる。
- **累積のインプレース版**: `add_in_place` が `target` だけを書き、`rhs` のブロードキャスト（`[3]` を `[2, 3]` へ）を扱う。`target` を広げる形（`[2, 3]` の `rhs` を `[3]` の `target` へ）は `Result.Error` で何も書かない。双子は同じ場面でトラップし、何も書かない。確保 0 回。
- **窓の作り方**: 非連続の入力（転置・負ストライド）は関数形（`Tensor.transpose`・`Tensor.flip`・`Tensor.slice [...] (ref v)`）で作る。角括弧で書くなら [NC16](NC16-index-ref-and-zero-alloc-element-reads.md) の後の `ref t[[Slice…]]`（D-66d）。HEAD では `ref t[[Slice.range 1 3]]` は `E1013`（`borrow requires a local place`）で、NC16 の後は `ref` のない `t[[Slice…]]` が `E1042` になる。
- **確保の測り方**: 窓（`Alloc.snapshot` から `Alloc.since` まで）の前に入力を一度使っておく。HEAD の dev バイナリでは、窓の前に作ったレコードの配列のフィールド（`shape: [2, 3, 1, 1, 1], strides: [3, 1, 1]`）の確保 2 回・64 バイトが、窓の中の最初の使用で数えられた。`Tensor.update` は要素を複製するので（現状）、`*_into` の確保 0 回は `Tensor.borrow_mut` で作った出力先で測る。
- **E2E（`tests/features.mjs`）**: 独立参照値（Python/NumPy と同等の数値）との照合、`live == 0`、native/WASM `-O0`/`-O3` ビット一致。`tests/tensor-cases.mjs` の `traps` に形の合わない `a + b` と `*_in_place_or_trap` を通る経路を足す（種類は `assertion failed`）。

## ドキュメント

- `_tsuzuri/language-reference/built-in-types-and-modules/tensor.md`・`matrix.md`
- `tensor.md` では、要素ごとの関数・比較・`*_into`・`*_in_place` の API の表と、同じ型どうしの演算子の節を足し、「できないこと」（287 行付近の「要素ごと算術 ufunc … まだ未実装です」）を実際の振る舞いに直す。`View`・スカラーとの演算子と `+=` は計画中として [NC19](NC19-operator-extension-instances.md)・[NC20](NC20-compound-assignment-and-mutable-index-views.md) へリンクする（GUIDE §8 の 4）。NumPy との違いを表にする: 整数の `/` はゼロ方向への切り捨て（NumPy の `/` は真の除算で、`//` に当たるのは `Tensor.floor_divide`）、`%` は被除数の符号（NumPy の `%` に当たるのは `Tensor.floor_mod`）、`@checked` は演算子インスタンスの中の要素の演算を検査しない、NumPy のマスクの `m1 & m2`・`m1 | m2`・`~m` に当たる演算子はなく `Tensor.logical_and`・`logical_or`・`logical_not` を使う（テンソル版の `&&&`・`|||`・`^^^` は D-66 の委任で却下。NC19 の D9 (i)）。
- `docs/stability.md` には書かない。std へのインスタンスの追加は、A37 が書く既知の例外「std への instance の追加」の項が扱う。NC06 のインスタンスは振り分けの位置に裸の型変数を持たないので、その項の一覧の例にも足さない（NC19 の D10）。

## 受け入れ条件

- 全 ufunc がブロードキャスト・負ストライド・空テンソルを正しく扱い、`*_into` と `*_in_place` がヒープ確保 0 回で動作すること。
- 主要二項・単項演算（`add`/`sub`/`mul`/`div`/`scale` 等）および `map_into` 単一パス複合処理の連続高速経路について、**Clang `-O3` 通過後の IR／逆アセンブル**でクロージャ間接呼び出しが残らず自動ベクトル化されることを確認済みであること。
- 暗黙 FMA・`fast-math` を一切含まず、native/WASM `-O0`/`-O3` でビット一致し、`effects_alloc_conservative` と T2 が通ること。
- 同じ型どうしの `Tensor<'a>` の演算子が関数版と同じ値（ビット単位）を返し、形の不一致でトラップし、被演算子を消費しないこと。演算子インスタンスの本体が非公開の双子を 1 回呼ぶだけで、公開の関数と双子が同じ要素ループを通ること（テストで確認）。
- 仕様 2・5 の名前と署名（非公開のトラップ版カーネル、両側のスカラー版、累積のインプレース版）が揃い、NC19・NC20 のチケットの名前と一致していること。
- 新しく書いた std のスライスが `..<` か `..=` で書かれ、素の `..` の終端付きのスライスを足していないこと。

## 落とし穴

- 非可換の二項演算（`sub`/`div`/`rem`/`pow`）は `AGENTS.md`（「複数の引数が対象の関数は F# の順序を保つ」）どおり `left -> right` で統一する。パイプラインで `x |> Tensor.sub y` と書くと `y - x` になるため、数式は同じ型どうしの演算子 `x - y`（仕様 4）または明示の `Tensor.sub x y` を使うこと。
- `Tensor.round` は `Math.round`（0 から遠い側へ丸め）であり、NumPy の `np.round(2.5) == 2.0` に相当するのは `Tensor.round_even` である。また NumPy の `np.remainder` に相当するのは `Tensor.floor_mod` であり、IEEE 剰余は `Tensor.ieee_remainder` として区別する。
- `std/Tensor.tz` から `Complex` の関数（`real`/`imag`/`conj`/`angle`）を直接参照する場合は `OPT_IN` の `Tensor.uses` に `"Complex"` を追加する（`OPT_IN.uses` は直接参照するモジュールのみの一致検査）。
- `tsuzuri build --emit llvm -O3` の出力は Clang 実行前の IR（`src/driver.rs:4651` 付近）のため、未最適化のまま見てクロージャ間接呼び出しと誤認しやすい。自動ベクトル化の有無は必ず `clang -x ir -O3 -S -emit-llvm` または `objdump` で確認すること。
- 素の `..` のスライス（`ref data[a .. b]`）を新しく書かない。edition 2028 で終端を含む意味に変わる（D-66c）。std の既存の素の `..` のスライス（`map` の 464 行付近など）の移行は [NC17](NC17-edition-2028-inclusive-range-family.md) が行うが、NC06 が書き直す行は `..<` にする。
- `map_in_place` の書き方（shape と strides の `Array.copy`）を写すと `*_into` が確保 0 回にならない（設計の「確保 0 回の書き方」）。
- `Tensor.update` は HEAD で要素を複製する（現状）。`*_into` の出力先を `update` で作る例を「確保なし」と書かない。直すのは [PM10](../_perfs/PM10-std-consume-copies.md)。
- 整数のテンソルの `/` と `%` は Tsuzuri の整数の規則のまま（`/` はゼロ方向への切り捨て、`%` は被除数の符号）。NumPy から移した `t / 2` は `[7]` で `3`、`t % 2` は `[-7]` で `-1` になる。床の意味は `floor_divide`・`floor_mod`。
- テストで窓を角括弧で作るときの `ref` の有無は NC16 の前後で変わる（テスト計画の「窓の作り方」）。関数形で作ればどちらでも動く。

## 対象外

- 演算融合（式テンプレート / 遅延計算グラフ）。
- SIMD / 並列 / GPU 専用テンソルカーネル（[NC13](NC13-tensor-simd-parallel-and-gpu-kernels.md) が担当）。
- `View` どうし・`Tensor` と `View`・スカラーとの演算子（演算子の拡張インスタンス。[NC19](NC19-operator-extension-instances.md)）。
- 複合代入 `+=`・`-=`・`*=`・`/=`・`%=` と書き込み窓 `ref mut t[…]`（[NC20](NC20-compound-assignment-and-mutable-index-views.md)）。
- ドット付きの要素ごと比較演算子（`a.<b` など。D3）。
- マスクの演算子 `&&&`・`|||`・`^^^` のテンソル版と `Bits<Tensor<'a>>`（D-66 の委任で却下。NC19 の D9 (i)）。マスクは仕様 2 の `logical_and`・`logical_or`・`logical_xor`・`logical_not` で、整数のテンソルのビット演算は `bit_and`・`bit_or`・`bit_xor`・`bit_not`・`shl`・`shr` で組み合わせる。
- `Tensor.update` の複製をなくす修正（[PM10](../_perfs/PM10-std-consume-copies.md)）。

## 決定事項

- **D1（既定案）**: 関数 API（`Tensor.add` 等）は形状非互換を `Result<Tensor<_>, Tensor.Error>` で返し、演算子インスタンス（`a + b`）は `Add` の署名上 `Tensor<'a>` を直接返すため形状非互換時にトラップする。トラップは非公開の双子 `*_or_trap`（仕様 5）の中で起き、NC19 の拡張インスタンスも同じ双子を呼ぶので、演算子の形によらずトラップの条件は同じになる。公開の `*_exn` は作らない。
- **D2（既定案）**: `Tensor` の `*` 演算子は要素ごと積（NumPy と同じ）。`Matrix` は行列積との取り違えを防ぐため `Mul` インスタンスを付けず、明示的に `Matrix.mul`（行列積）と `Matrix.hadamard`（要素ごと積）を使い分ける。演算子の拡張インスタンス（NC19）もこの決定を前提にする。
- **D3（既定案）**: 要素ごと比較は `Tensor.elem_gt` / `Tensor.gt_scalar` 等の関数 API とし、`Eq`/`Ord` 演算子（`==`/`<`）の「常にスカラー `bool` を返す」契約を壊さない。ドット付きの比較演算子（`a.<b` など）も入れない（`..<`・`<..` と 1 文字違いで読み違えやすい。D-66）。
- **D4（既定案）**: 二項テンソル関数は `AGENTS.md` と `Matrix.mul` に従いすべて `left -> right` 順（`Tensor.sub left right = left - right`、`Tensor.div left right = left / right`）とし、スカラー演算のみ対象テンソルを最後（`sub_scalar s x = x - s`、`rsub_scalar s x = s - x`）とする。スカラーが左に来る形と右に来る形の対応は D7。
- **D5（既定案）**: 演算子の分担。NC06 は同じ型どうしの `Tensor<'a>` の `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Pow`/`Neg` インスタンスだけを足す。`View` どうし・`Tensor` と `View`・スカラーとの演算子は [NC19](NC19-operator-extension-instances.md)、`+=` などは [NC20](NC20-compound-assignment-and-mutable-index-views.md)。理由: 今の `binary_type` は両辺の型を単一化する（`src/check.rs:17012`）ので、型の違う演算子には言語の拡張（NC03 D2 の改訂。NC19 の D1 で、D-66 の委任で承認済み、2026-11-25）が要る。NC06 は std だけで完結させ、NC19 を待たない。`Pow` は NC03 が借用の算術クラスに含め（`src/polymorph.rs:943`）、NC19 も扱うので、同じ型どうしのインスタンスにも入れる。
- **D6（既定案）**: 非公開のトラップ版カーネルは、公開の関数ごとの双子 `<name>_or_trap`（仕様 5）にする。理由: 演算子の本体を 1 回の呼び出しにでき、公開の関数と同じ要素ループを通ることをソースのテストで確かめられる。1 つの汎用の `or_trap`（`Result` を外すだけの関数）で公開の関数を包む形は、本体が 2 回の呼び出しになり、要素ループの共有を名前で確かめられないので採らない。
- **D7（既定案）**: スカラー版は左右の両方を揃える（仕様 2 の表。`radd_scalar`・`mul_scalar`・`rem_scalar`・`rrem_scalar`・`rpow_scalar` を足した）。理由: `x * s` を `scale s x`（`s * x_i`）へ回すと、要素型の `Mul` が非可換（2×2 行列のレコードなど）のときに順序が逆になる。`s ** x` と `s % x` にはカーネルがなかった。
- **D8（既定案）**: 累積のインプレース版 `*_in_place`（`target_i = target_i op rhs_i`、`Result` を返す）とその双子を足す。理由: NC20 の `x += y` は出力が入力と重なるので、出力先を別に取る `*_into` では書けない（同じテンソルの窓と `ref mut` を同時に渡すと `E1014`）。`rhs` だけをブロードキャストし、`target` の形は変えない（NumPy の `a += b` と同じ）。

## 改訂（2026-11-25）

[GUIDE D-66](GUIDE.md#d-66-自然な添字範囲演算子d-65-の改訂a37nc16nc20)（自然な添字・範囲・演算子）で、演算子の拡張インスタンス（NC19）と複合代入（NC20）を別のチケットにしたので、このチケットを次のとおり改めた。GUIDE §0 の 2 に従い、現状も HEAD `858a949c` に合わせた。

- **演算子の分担**（仕様 4・D5・対象外・後続）: `View` どうし・`Tensor` と `View`・スカラーとの演算子を NC19 へ、`+=` を NC20 へ移した。NC06 に残すのは同じ型どうしの `Tensor<'a>` のインスタンスだけで、`Pow` を足した。
- **非公開のトラップ版カーネル**（仕様 5・D6）を足した。NC19・NC20 のインスタンスの本体がこれを 1 回呼ぶ。
- **両側のスカラー版**（仕様 2・D7）: 設計の反証で、`x * s` を `scale` へ回すと非可換の要素型で順序が逆になることと、`s ** x` のカーネルがないことが分かったため。
- **累積のインプレース版**（仕様 1・2・D8）: NC20 の `+=` が `*_into` では書けないため。
- **D-66d（`ref` 必須）**: このチケットには土台を借りる `t[[Slice…]]` の例がなかった。マスクの例 `sub[Tensor.gt_scalar 0.0 sub]` は複製なので `ref` なしのまま。テストで窓を作る書き方をテスト計画と落とし穴に書いた。
- **範囲の構文**（設計・落とし穴）: 1D 連続高速経路の例を `ref data[offset ..< offset + total]` にした。素の `..` のスライスは edition 2028 で意味が変わる（D-66c）。
- **リンク**: 完了済みの NC01・NC02・NC03 を `_completed/` へ直し、NC04・NC05 へのリンクを足した。
- **現状の更新**: NC02・NC05 が足した関数とインスタンス、演算子の型付け（`src/check.rs:17012`）、`src/driver.rs` の行（4620 → 4651 付近）、存在しない `Alloc.count` を `Alloc.snapshot`・`Alloc.since` に、`map_in_place`・`view`・`Tensor.update` の確保の測定（`Tensor.update` は要素を複製する）。設計に確保 0 回の書き方を足した。
- **規模・着手条件**: 「`Tensor`/`Matrix` の演算子インスタンス」を、仕様 4 と手順 3 に合わせて `Tensor` だけにした。`Matrix` に `Add` などを付けるかは元のチケットでも決まっていない（D2 は `Mul` を付けないことだけを決める）。

### D-66 の委任による決定の反映（2026-11-25）

利用者が D-66 の承認の要る項目と問いの決定を委ねたので、その決定に合わせて次のとおり改めた。NC06 には承認を待つ項目がなく、承認の行は「不要」のまま。

- **A37 への弱い依存**（依存・着手条件・前提とするインターフェース・手順 3）: 演算子インスタンスの段（手順 3）を A37 の手順 5（算術クラスの orphan 規則）の後にした。利用者の `instance Add<Tensor<f64>>` が、NC06 との重なりではなく A37 の `orphan_instance` で一度だけ拒否されるようにするため（NC19 の D10）。
- **一覧のテスト**（仕様 4・テスト計画）: NC06 の同じ型どうしのインスタンスは振り分けの位置に裸の型変数を持たないので、A37 の `std_instances_with_open_dispatch_positions_are_listed` に足さないと書いた。`docs/stability.md` には書かない（A37 の既知の例外の項がある）。
- **マスクの演算子**（対象外・ドキュメント）: テンソル版の `&&&`・`|||`・`^^^` は却下された（NC19 の D9 (i)）。マスクは `logical_*`、整数のテンソルのビット演算は `bit_*`・`shl`・`shr` の関数で書く。
- **D5**: NC19 の D1 の状態を、承認済み（D-66 の委任で決定、2026-11-25）に直した。
