# NC10: `Random` の数値計算向け拡張（PCG64・`SeedSequence`・確率分布）と opt-in `RandomTensor`

| 項目 | 内容 |
| --- | --- |
| ID | NC10 |
| 優先度 | P2（モンテカルロ法・統計シミュレーション・機械学習の重み初期化とデータシャッフル） |
| 規模 | M（2 Phase: Phase 1 常時ロード `std/Random.tz` のスカラー分布拡張、Phase 2 opt-in `std/RandomTensor.tz` のテンソル生成・軸シャッフル・多変量正規） |
| 依存 | [NC01](_completed/NC01-complex-and-extended-math.md), [NC02](_completed/NC02-tensor-core-views-and-broadcasting.md), ([NC08](_completed/NC08-linalg-core-and-einsum.md)) |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-24（[GUIDE D-65](GUIDE.md#d-65-数値計算基盤と-numpy-互換機能案-c-ハイブリッドnc01nc14)。HEAD `27069465` で確認） |
| 承認 | 要承認（Phase 2 の新規 opt-in std モジュール `RandomTensor` の `RESERVED_MODULES` 登録。決定事項 D1 が要承認、Phase 1 の `std/Random.tz` 拡張は既定案） |
| 手本にする既存実装 | `std/Random.tz`（`Pcg`・`pcg_next_u32`・`pcg_next_u64` の純粋関数型設計）、`std/Array.tz`（`random_shuffle_with`/`random_shuffle_in_place_with`/`random_choice_with`）、`tests/capabilities.rs`・`tests/fixtures/capabilities/std-table.txt` |
| 主な影響ファイル | `std/Random.tz`、`std/RandomTensor.tz`（新規・Phase 2）、`src/stdlib.rs`、`src/capabilities.rs`、`tests/capabilities.rs`、`tests/fixtures/capabilities/std-table.txt`、`tests/random_distributions.rs`（新規）、`tests/std_argument_order.rs`、`tests/std_examples.rs`、`tests/features.mjs`、`tests/fixtures/effects_sweep/`、`tests/fixtures/`、`scripts/check-cli-docs.mjs`（`STD_PAGES`）、`vsc/src/core.ts`、`_tsuzuri/language-reference/built-in-types-and-modules/random.md`、`_tsuzuri/language-reference/built-in-types-and-modules/random-tensor.md`（新規）、`_tsuzuri/language-reference/index.md` |

## 実装者向け早見

- 範囲: `std/Random.tz` は**常時ロード**、`Tensor`/`Linalg` は **`OPT_IN`** のため、逆依存（常時ロードから opt-in への参照）を避ける 2 層構成で NumPy `numpy.random.Generator` 相当の乱数基盤を提供する（すべて `pure` 契約と既存 `Random.pcg_next_u32`・`Array.random_*_with` の流儀に揃え、`own Pcg64 -> (T * Pcg64)` の状態引き回し形式とする）:
  1. **Phase 1（`std/Random.tz`・常時ロードのまま、`Tensor`/`Linalg` に依存しない）**: 64 ビット高品質生成器 `Pcg64`（PCG-XSL-RR-128/64）と独立ストリーム分岐 `SeedSequence`（`seed_sequence_spawn`）、`pcg64_from_os`（`random` 権限）、スカラー基本ステップ（既存 `Random.next_u64` との衝突を避け `pcg64_next_u64`/`pcg64_next_f64`/`pcg64_next_i64_range`）、および全主要確率分布のスカラー生成（`pcg64_uniform`, `pcg64_standard_normal`, `pcg64_normal`, `pcg64_exponential`, `pcg64_gamma`, `pcg64_beta`, `pcg64_chisquare`, `pcg64_poisson`, `pcg64_binomial`, `pcg64_bernoulli`, `pcg64_geometric`, `pcg64_laplace`, `pcg64_lognormal`, `pcg64_standard_t`, `pcg64_f_dist`）。
  2. **Phase 2（新規 opt-in `std/RandomTensor.tz`、`uses: &["Tensor", "Linalg"]`）**: 任意形状 `Tensor` の一括分布生成（`uniform`, `normal`, `standard_normal`, `integers`, `exponential`, `gamma`, `beta`, `chisquare`, `poisson`, `binomial`, `bernoulli`, `geometric`, `laplace`, `lognormal`, `standard_t`, `f_dist`, `dirichlet`, `multivariate_normal` — いずれも `own Random.Pcg64 -> (Tensor<_> * Random.Pcg64)`）、`permutation`、`shuffle_axis`（対象 `Tensor.Mut` が最後の引数で更新後の `Random.Pcg64` を返す）、`choice_weighted`（対象 `Tensor.View` が最後の引数で `(Tensor<'a> * Random.Pcg64)` を返す）。
- 最初に読む: ① `std/Random.tz` 全文 ② `src/stdlib.rs` の常時ロードと `OPT_IN` の境界 ③ 仕様 1〜3 ④ `tests/capabilities.rs:236`・`tests/fixtures/capabilities/std-table.txt`。
- 最初に書くテスト: `tests/random_distributions.rs` でシード固定の決定論的ビット再現性（native/WASM 一致）、各分布の標本平均・標本分散・範囲の統計検定、`seed_sequence_spawn` の独立性。
- 触ってはいけない: `std/Random.tz` から `Tensor` や `Linalg` を参照すること（全プログラムの IR が変化する）。既存の `Random.Pcg`・`Random.pcg`・`Random.pcg_next_u32`・`Random.pcg_next_u64`・`Random.bytes`・`Random.next_u64` の出力と権限契約（シード付き決定論生成器は `random` 権限不要・`pure` を保つ）。
- 完了の見分け方: `let rng = Random.pcg64 42i64u` から `let (samples, rng) = RandomTensor.normal 0.0 1.0 [100, 10] rng` で正規乱数テンソルを生成でき、native と WASM の `-O0`・`-O3` で完全に同一のビット列が得られる。

## 目的

現行の [Random.tz](../std/Random.tz) は 32/64 bit 整数の出力のみで、確率分布・多次元テンソルの一括生成・並列用の独立ストリーム分岐を持たない。常時ロードの `Random`（スカラー分布）と opt-in の `RandomTensor`（テンソル生成・多変量分布・軸シャッフル）を分離することで、数値計算を使わないプログラムの IR を 1 byte も変えずに NumPy 級の乱数機能を提供する。

## 着手条件と停止条件

### 着手条件

- Phase 1 は [NC01](_completed/NC01-complex-and-extended-math.md) が done。
- Phase 2 は [NC02](_completed/NC02-tensor-core-views-and-broadcasting.md) が done（`multivariate_normal` は [NC08](_completed/NC08-linalg-core-and-einsum.md) も done）および決定事項 D1 の承認。

### 停止条件

- **S1** 浮動小数点分布の生成結果が native と WASM、または `-O0` と `-O3` で 1 bit でも異なる場合（`Math.log`/`Math.sqrt`/`Math.cos`/`Math.sin` のビット一致契約を使えば一致するはず）。原因を切り分けて解消する。
- **S2** `RandomTensor` を名指さない既存プログラムの IR が 1 byte でも変わる場合（`Random.tz` への opt-in 参照漏れを疑う）。
- **S3** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `27069465` で確認）

- `std/Random.tz`: 常時ロードの標準モジュール。`Pcg` レコードと 5 関数（`bytes`, `next_u64`（`IO`・`random` 権限）、`pcg`, `pcg_next_u32`, `pcg_next_u64`（`pure`））を持つ。
- `tests/capabilities.rs`: `Random.tz` 内のホスト呼び出し個数と `tests/fixtures/capabilities/std-table.txt`（88 関数）を厳密検査している。
- `std/Array.tz`: `random_shuffle_in_place_with :: own Random.Pcg -> ref mut ['a..] -> Random.Pcg` や `random_choice_with :: Copy<'a> => own Random.Pcg -> ref ['a] -> ('a * Random.Pcg)` のように、生成器を値で受け取り更新後を返し、操作対象コレクションが最後の引数になっている。
- `src/pure_rules.rs:134-141`: `pure def` は `mut` / `ref mut` パラメータを取れない（`E1029`）。そのため純粋な乱数ステップ・分布生成は `own Pcg64 -> (T * Pcg64)` の状態引き回し形式をとる。

## 変えてはいけない意味

- 既存 `Random.Pcg` の出力列（既存の doc-test 期待値 `2707161783`, `2068313097` 等）および既存 `Random.next_u64` の名前・型。
- シード指定の乱数生成が `random` 権限（OS エントロピー）を要求せず、`pure`（インプレース書き換えを行う `shuffle_axis` を除く）かつ WASM インポートなしで動作すること。

## 仕様

### 1. Phase 1: `std/Random.tz`（常時ロード、スカラー専用・`Tensor`/`Linalg` 非依存）

- `record Pcg64`（128 bit 状態・128 bit 増分の PCG-XSL-RR-128/64。不透明レコード、`src/stdlib.rs` の `opaque_record` 登録）。`Random.pcg64 :: own i64u -> Pcg64`、`Random.pcg64_from_seed :: ref SeedSequence -> Pcg64`、`Random.pcg64_from_os :: own unit -> IO<Result<Pcg64, Os.Error>>`（`random` 権限。既存の `Random.bytes` 経由で実装すれば `Os.__random` の直接呼び出し個数を増やさず `std-table.txt` への 1 行追加のみで済む）。
- `record SeedSequence`（HashMix ベースのシード拡張と `seed_sequence :: own [i64u] -> SeedSequence`、`seed_sequence_spawn :: own i64 -> own SeedSequence -> ([SeedSequence] * SeedSequence)` による並列タスク向け独立子ストリーム生成、`pure def`）。
- ステップ関数（すべて `pure def`、既存 `Random.next_u64` との衝突を避け `pcg64_*` 命名・`own Pcg64` が最後の引数: D-59、`MODULES` の `Random` を `subject(&["Pcg", "Pcg64", "SeedSequence"])` へ更新）:
  - `pcg64_next_u64 :: own Pcg64 -> (i64u * Pcg64)`、`pcg64_next_f64 :: own Pcg64 -> (f64 * Pcg64)`（上位 53 bit を $2^{-53}$ 倍して `[0.0, 1.0)` を厳密生成）、`pcg64_next_i64_range :: own i64 -> own i64 -> own Pcg64 -> (i64 * Pcg64)`（Lemire 棄却サンプリングでモジュロバイアスなし）。
- スカラー確率分布（すべて `pure def`、`own Pcg64` が最後の引数で `(値 * Pcg64)` を返す）:
  - 連続: `pcg64_uniform`, `pcg64_standard_normal`, `pcg64_normal`, `pcg64_exponential`, `pcg64_gamma`, `pcg64_beta`, `pcg64_chisquare`, `pcg64_laplace`, `pcg64_lognormal`, `pcg64_standard_t`, `pcg64_f_dist`
  - 離散: `pcg64_bernoulli`, `pcg64_binomial`, `pcg64_poisson`, `pcg64_geometric`

### 2. Phase 2: `std/RandomTensor.tz`（opt-in、テンソル一括生成・多変量分布・軸操作）

- `OPT_IN` 登録: `uses: &["Tensor", "Linalg"]`（ソース自身が直接参照する opt-in モジュールのみ。推移閉包は `sources_for` が自動解決する）。`RESERVED_MODULES` に `"RandomTensor"` を追加し `src/stdlib.rs:843` の件数アサーションを更新する。`scripts/check-cli-docs.mjs` の `STD_PAGES` に `RandomTensor: 'built-in-types-and-modules/random-tensor.md'` を登録。
- 形状指定テンソル生成（すべて `pure def`、対象 `own Random.Pcg64` が最後で `(Tensor<_> * Random.Pcg64)` を返す）:
  - `uniform`, `standard_normal`, `normal`, `integers`, `exponential`, `gamma`, `beta`, `chisquare`, `laplace`, `lognormal`, `standard_t`, `f_dist`, `bernoulli`, `binomial`, `poisson`, `geometric`（例: `RandomTensor.normal loc scale shape rng -> (Tensor<f64> * Random.Pcg64)`）
  - 多変量分布: `dirichlet :: ref Tensor.View<f64> -> own [i64] -> own Random.Pcg64 -> (Tensor<f64> * Random.Pcg64)`、`multivariate_normal :: ref Tensor.View<f64> -> ref Tensor.View<f64> -> own [i64] -> own Random.Pcg64 -> Result<(Tensor<f64> * Random.Pcg64), Linalg.Error>`（`Linalg.cholesky` 使用）
  - 順列: `permutation :: own i64 -> own Random.Pcg64 -> (Tensor<i64> * Random.Pcg64)`
- テンソル操作（既存の `Array.random_shuffle_in_place_with` / `Array.random_choice_with` と一貫させ、操作・書き換え対象のテンソルを最後の引数にする: D-59）:
  - `shuffle_axis :: own i64 -> own Random.Pcg64 -> ref mut Tensor.Mut<'a> -> Random.Pcg64`（`ref mut` 引数を取るため非 `pure` の `def`）
  - `choice_weighted :: Copy<'a> => own i64 -> own bool -> ref Tensor.View<f64> -> own Random.Pcg64 -> ref Tensor.View<'a> -> (Tensor<'a> * Random.Pcg64)`（`pure def`）

### 3. パラメータ検証

- 不正な分布パラメータ（例: `scale < 0.0`、`p < 0.0 || p > 1.0`、`shape <= 0.0`、`low >= high`）は計算前に `assert` でトラップし、サイレントな `NaN` 汚染を防ぐ。

## 設計

- `Pcg64` の 128 bit 乗算は Tsuzuri 組み込みの `i128u` 演算をそのまま使えるため、純 Tsuzuri で簡潔かつ高速に実装できる。
- `standard_normal` は Marsaglia polar 法または Ziggurat テーブル法（テーブルは `const` 静的配列で確保なし）、`gamma` は Marsaglia-Tsang 法（`shape < 1` は Johnk/Ahrens-Dieter 変換併用）を採用し、すべての基本演算で組み込み `Math` のビット一致関数を使用する。
- `tests/std_argument_order.rs` の `MODULES` には `Random` を `subject(&["Pcg", "Pcg64", "SeedSequence"])`、`RandomTensor` を `subject(&["Random.Pcg64", "Tensor.Mut", "Tensor.View"])`（生成系は `Pcg64` が最後、操作系は `Tensor.Mut`/`Tensor.View` が最後）として登録する。

## 前提とする他チケットのインターフェース

- [NC01](_completed/NC01-complex-and-extended-math.md): `Math.log1p`, `Math.expm1`, `Math.log`, `Math.sqrt`。
- [NC02](_completed/NC02-tensor-core-views-and-broadcasting.md): `Tensor`, `Tensor.View`, `Tensor.Mut`（Phase 2）。
- 弱い依存 [NC08](_completed/NC08-linalg-core-and-einsum.md): `Linalg.cholesky`（Phase 2 の `multivariate_normal` 用）。

## 他チケットへ提供するインターフェース

- `Random.Pcg64`, `Random.SeedSequence`, 各スカラー分布および `RandomTensor.*` 公開 API。

## 実装手順

1. **変更**: `std/Random.tz`, `src/stdlib.rs`, `tests/capabilities.rs`, `tests/fixtures/capabilities/std-table.txt`, `tests/std_argument_order.rs`, `tests/std_examples.rs`, `tests/fixtures/effects_sweep/`。**内容**: Phase 1（`Pcg64`・`SeedSequence`・`pcg64_from_os`・全スカラー確率分布と権限表・コーパス網羅更新）を実装する。**確認**: T0、`cargo test --locked --test capabilities --test std_argument_order --test std_examples --test effects_alloc_conservative`、`tsuzuri test --doc std`、T1。
2. **変更**: `std/RandomTensor.tz`, `src/stdlib.rs`, `tests/random_distributions.rs`, `tests/std_argument_order.rs`, `tests/std_examples.rs`, `tests/fixtures/effects_sweep/`, `scripts/check-cli-docs.mjs`, `vsc/src/core.ts`。**内容**: Phase 2（opt-in `RandomTensor` のテンソル一括生成・多変量分布・`permutation`/`shuffle_axis`/`choice_weighted`）を実装する。**確認**: T0、`tests/random_distributions.rs`、`effects_alloc_conservative`、`tsuzuri test --doc std`、T1。
3. **変更**: `tests/fixtures/`・`tests/features.mjs`、`_tsuzuri/language-reference/built-in-types-and-modules/random.md`・`random-tensor.md`（新規）・`index.md`。**内容**: native/WASM `-O0`/`-O3` ビット一致 E2E と文書更新。**確認**: T2、`node scripts/check-docs.mjs`、`node scripts/check-cli-docs.mjs`。

## テスト計画

- **統計・再現性テスト（`tests/random_distributions.rs`）**: 同一シードでの完全一致、異なる `seed_sequence_spawn` 子間の非相関、各分布（一様・正規・指数・ガンマ・ベータ・ポアソン・二項・カイ二乗・ディリクレ・多変量正規）の標本平均・標本分散・上下限境界（10,000〜50,000 サンプルで理論値と $4\sigma$ 以内の一致）、不正パラメータトラップ。
- **E2E（`tests/features.mjs`）**: native と WASM の `-O0`・`-O3` で同一の乱数テンソルビット列が出ること、WASM インポートなし、`live == 0`。

## ドキュメント

- `_tsuzuri/language-reference/built-in-types-and-modules/random.md`、`random-tensor.md`（新規）、`index.md`

## 受け入れ条件

- シード指定の全分布が native と WASM の `-O0`・`-O3` でビット完全一致し、WASM インポートを増やさないこと。
- `RandomTensor` を使わない既存プログラムに `Tensor`/`Linalg` がロードされず、既存 `Random.Pcg` の互換性が保たれること。
- `capabilities`・`effects_alloc_conservative`・`std_argument_order`・`check-cli-docs`・T2 がすべて通ること。

## 落とし穴

- `pure def` は `mut` / `ref mut` 引数を取れない（`E1029`、`src/pure_rules.rs:134-141`）。また `Random.next_u64` は既存 `IO` 関数として定義済み（`std/Random.tz:42`）のため、新関数は必ず `pcg64_*` 命名と `own Pcg64 -> (T * Pcg64)` の状態引き回し形式にすること。
- `std/Random.tz` は常時ロードモジュールのまま保つため、`Tensor` や `Linalg` の型名を `std/Random.tz` に書いてはならない（全プログラムのコンパイルエラーまたは IR 変化を引き起こす）。また `RandomTensor` の `OPT_IN.uses` はソース内で直接名指しするモジュールのみを列挙し、`src/stdlib.rs:843` の `RESERVED_MODULES.len()` アサーションも同時更新すること。
- `pcg64_from_os` は `Random.bytes` 経由で `random` 権限を持つため、`tests/fixtures/capabilities/std-table.txt`（および `tests/capabilities.rs` の行数アサーション）の更新を忘れると T1 が失敗する。
- `RandomTensor` はページ名がハイフン区切り（`random-tensor.md`）になるため、`scripts/check-cli-docs.mjs` の `STD_PAGES` 登録が必須。

## 対象外

- グローバル可変の暗黙 RNG（`np.random.seed()` 相当）。
- NumPy の `numpy.random.PCG64` とビット単位で同一の乱数列を再現すること（シード展開や分布アルゴリズムが異なるため、Tsuzuri 独自のシード固定再現性と native/WASM `-O0`/`-O3` ビット一致を契約とする）。
- 暗号論的疑似乱数生成器（CSPRNG の OS エントロピー取得は既存 `Random.bytes`/`Random.next_u64` を使用）。

## 決定事項

- **D1（要承認）**: 常時ロードの `Random` から opt-in の `Tensor`/`Linalg` への逆依存を避けるため、テンソル乱数生成・多変量分布・軸シャッフルを新規 opt-in std モジュール **`RandomTensor`**（`std/RandomTensor.tz`）へ分離し、`RESERVED_MODULES` に登録する（`D-55` へ反映）。
- **D2（既定案）**: `pure def` 契約および既存 `Random.pcg_next_u32`・`Array.random_*_with` と完全に揃えるため、生成器は `own Pcg64 -> (値 * Pcg64)` の純粋な状態引き回し形式とし、`Random` 側の新関数名は既存 `Random.next_u64` と衝突しない `pcg64_*` に統一する。生成器のみを操作する関数では `own Pcg64` を最後の引数、既存テンソルを操作する関数（`shuffle_axis`/`choice_weighted`）では対象テンソルを最後の引数とする（D-59）。
- **D3（既定案）**: グローバル可変の暗黙 RNG（旧 `np.random.seed()` 相当）は導入しない。必ず明示的な `Pcg64` 値または `SeedSequence` を渡すことで、純粋性・再現性・並行安全性を保証する。
