# NC10: `Random` の数値計算向け拡張（PCG64・`SeedSequence`・確率分布）と opt-in `RandomTensor`

| 項目 | 内容 |
| --- | --- |
| ID | NC10 |
| 優先度 | P2（モンテカルロ法・統計シミュレーション・機械学習の重み初期化とデータシャッフル） |
| 規模 | M（2 Phase: Phase 1 常時ロード `std/Random.tz` のスカラー分布拡張、Phase 2 opt-in `std/RandomTensor.tz` のテンソル生成・軸シャッフル・多変量正規） |
| 依存 | [NC01](NC01-complex-and-extended-math.md), [NC02](NC02-tensor-core-views-and-broadcasting.md), ([NC08](NC08-linalg-core-and-einsum.md)) |
| 後続 | – |
| 状態 | done（Phase 1・2。2026-11-28） |
| 起票 | 2026-11-24（[GUIDE D-65](../GUIDE.md#d-65-数値計算基盤と-numpy-互換機能案-c-ハイブリッドnc01nc14)。HEAD `27069465` で確認） |
| 承認 | 承認済み（D-65 の委任で決定、2026-11-28。D1 の `RandomTensor` の `RESERVED_MODULES` 登録と、「方針の最新化」の D4〜D12） |
| 手本にする既存実装 | `std/Random.tz`（`Pcg`・`pcg_next_u32`・`pcg_next_u64` の純粋関数型設計）、`std/Array.tz`（`random_shuffle_with`/`random_shuffle_in_place_with`/`random_choice_with`）、`tests/capabilities.rs`・`tests/fixtures/capabilities/std-table.txt` |
| 主な影響ファイル | `std/Random.tz`、`std/RandomTensor.tz`（新規・Phase 2）、`src/stdlib.rs`、`src/capabilities.rs`、`tests/capabilities.rs`、`tests/fixtures/capabilities/std-table.txt`、`tests/random_distributions.rs`（新規）、`tests/std_argument_order.rs`、`tests/std_examples.rs`、`tests/features.mjs`、`tests/fixtures/effects_sweep/`、`tests/fixtures/`、`tests/random-cases.mjs`（新規）、`scripts/check-cli-docs.mjs`（`STD_PAGES`）、`vsc/src/core.ts`、`docs/language.md`、別リポジトリ Tsuzuri-Reference の `docs/stdlib/sys/random.md`・`docs/stdlib/numeric/random-tensor.md`（新規）・`docs/index.md`・`docs/stdlib/conventions.md` |

## 実装者向け早見

- 範囲: `std/Random.tz` は**常時ロード**、`Tensor`/`Linalg` は **`OPT_IN`** のため、逆依存（常時ロードから opt-in への参照）を避ける 2 層構成で NumPy `numpy.random.Generator` 相当の乱数基盤を提供する（すべて `pure` 契約と既存 `Random.pcg_next_u32`・`Array.random_*_with` の流儀に揃え、`own Pcg64 -> (T * Pcg64)` の状態引き回し形式とする）:
  1. **Phase 1（`std/Random.tz`・常時ロードのまま、`Tensor`/`Linalg` に依存しない）**: 64 ビット高品質生成器 `Pcg64`（PCG-XSL-RR-128/64）と独立ストリーム分岐 `SeedSequence`（`seed_sequence_spawn`）、`pcg64_from_os`（`random` 権限）、スカラー基本ステップ（既存 `Random.next_u64` との衝突を避け `pcg64_next_u64`/`pcg64_next_f64`/`pcg64_next_i64_range`）、および全主要確率分布のスカラー生成（`pcg64_uniform`, `pcg64_standard_normal`, `pcg64_normal`, `pcg64_exponential`, `pcg64_gamma`, `pcg64_beta`, `pcg64_chisquare`, `pcg64_poisson`, `pcg64_binomial`, `pcg64_bernoulli`, `pcg64_geometric`, `pcg64_laplace`, `pcg64_lognormal`, `pcg64_standard_t`, `pcg64_f_dist`）。
  2. **Phase 2（新規 opt-in `std/RandomTensor.tz`、`uses: &["Tensor", "Linalg"]`）**: 任意形状 `Tensor` の一括分布生成（`uniform`, `normal`, `standard_normal`, `integers`, `exponential`, `gamma`, `beta`, `chisquare`, `poisson`, `binomial`, `bernoulli`, `geometric`, `laplace`, `lognormal`, `standard_t`, `f_dist`, `dirichlet`, `multivariate_normal` — いずれも `own Random.Pcg64 -> (Tensor<_> * Random.Pcg64)`）、`permutation`、`shuffle_axis`（対象 `Tensor.Mut` が最後の引数で更新後の `Random.Pcg64` を返す）、`choice_weighted`（対象 `Tensor.View` が最後の引数で `(Tensor<'a> * Random.Pcg64)` を返す）。
- 最初に読む: ① `std/Random.tz` 全文 ② `src/stdlib.rs` の常時ロードと `OPT_IN` の境界 ③ 仕様 1〜3 ④ `tests/capabilities.rs` の `std_table_file_lists_the_88_functions`・`tests/fixtures/capabilities/std-table.txt`。
- 最初に書くテスト: `tests/random_distributions.rs` でシード固定の決定論的ビット再現性（native/WASM 一致）、各分布の標本平均・標本分散・範囲の統計検定、`seed_sequence_spawn` の独立性。
- 触ってはいけない: `std/Random.tz` から `Tensor` や `Linalg` を参照すること（全プログラムの IR が変化する）。既存の `Random.Pcg`・`Random.pcg`・`Random.pcg_next_u32`・`Random.pcg_next_u64`・`Random.bytes`・`Random.next_u64` の出力と権限契約（シード付き決定論生成器は `random` 権限不要・`pure` を保つ）。
- 完了の見分け方: `let rng = Random.pcg64 42i64u` から `let (samples, rng) = RandomTensor.normal 0.0 1.0 [100, 10] rng` で正規乱数テンソルを生成でき、native と WASM の `-O0`・`-O3` で完全に同一のビット列が得られる。

## 方針の最新化（2026-11-28）

起票（2026-11-24、HEAD `27069465`）のあとに入った変更と、`main` の `b72bccd0` のコードに照らして、着手の前に次を改めました。利用者の委任（「もし現在の最新仕様と齟齬がある内容がある場合には、実装前に方針を再検討して最新化して」「あなたが考える最高の選択肢で実装することを常に許可します」）に基づく決定で、理由は「決定事項」の D4〜D12 にあります。上の表と「仕様」「設計」「実装手順」「落とし穴」の該当する文も、この節に合わせて直してあります。

**HEAD と食い違っていた記述**

| チケットの記述 | HEAD（`b72bccd0`）の事実と、改めた内容 |
| --- | --- |
| 文書は `_tsuzuri/language-reference/built-in-types-and-modules/random.md`・`random-tensor.md`・`index.md` | 利用者向けの言語リファレンスは別リポジトリ Tsuzuri-Reference に移っている（PR #147）。`Random` のページは `docs/stdlib/sys/random.md`（`STD_PAGES` の既存の値）、`RandomTensor` は数値計算の並び（`tensor.md`・`linalg.md`）に合わせて `docs/stdlib/numeric/random-tensor.md` を新設し、`STD_PAGES` には `'stdlib/numeric/random-tensor.md'` を登録する。同じ PR で `docs/language.md` も更新する（`AGENTS.md`） |
| `src/stdlib.rs:843` の件数アサーション、`tests/capabilities.rs:236`、`std-table.txt`（88 関数） | 行番号は動いている。`RESERVED_MODULES` は 83 件（`reserves_the_d07_table`）で、84 にする。`std-table.txt` は 90 行で、`Random.pcg64_from_os: random` を足して 91 行にする。`Random.tz` の `Os.__random` の出現は 1 のまま（`pcg64_from_os` は `bytes` を呼ぶ） |
| 範囲の書き方 | std は最新の edition（2028）で読まれ、`a .. b` は終端を含む（D-66）。実装は `..<`・`..=` を明示する。`pcg64_next_i64_range low high` と `RandomTensor.integers low high` は範囲の式ではなく 2 つの境界を取る関数で、.NET の `Random.Next(min, max)`・NumPy の `integers`・既存の `Array.random_*` と同じ半開区間 `[low, high)` にする（D10） |
| `shuffle_axis :: own i64 -> own Random.Pcg64 -> ref mut Tensor.Mut<'a> -> Random.Pcg64`（制約なし） | `Tensor.Mut` の要素を読む公開 API はすべて `Copy<'a>` を要求し、入れ替えの API はない（`Tensor.Mut` は不透明）。`Copy<'a> =>` を足す（D9） |
| シードの展開は「HashMix ベース」、NumPy と同じ乱数列は対象外 | 展開の算法を NumPy の `SeedSequence`（公開された算法）そのものにする。分布の標本を NumPy と同じにすることは引き続き対象外（D4） |
| `standard_normal` は Marsaglia polar か Ziggurat、`gamma` の `shape < 1` は Johnk/Ahrens-Dieter | Leva の一様乱数の比（D5）、Marsaglia-Tsang の論文の `u ** (1 / shape)`（D6）にする |
| 最初に書くテストは `tests/random_distributions.rs` | Rust のテストは実行できる検査を `tsuzuri test` で走らせる（`tests/linalg.rs` と同じ形）。標本ごとの照合は `tests/features.mjs` の suite `random`・`random_tensor` で行い、期待値は `tests/random-cases.mjs` の BigInt のモデルが作る（モデルは NumPy 2.5.1 が出した数と照合する。D11） |

**変えなかった点**: 2 層の構成（常時ロードの `Random` と opt-in の `RandomTensor`）、`own Pcg64 -> (値 * Pcg64)` の引き回し、`pcg64_*` の名前、対象を最後に置く順序（D-59）、暗黙の RNG を入れないこと（D3）、不正なパラメータのトラップ。

**常時ロードの `Random` に足すことの影響**: `std/Random.tz` は全プログラムが読むので、宣言を足すと内部シンボルの通し番号がずれる（M15・M17 が `std/Env.tz` に足したときと同じ）。使わない関数の本体は出力されない。実測は「実装と検証」にある。

## 目的

現行の [Random.tz](../../std/Random.tz) は 32/64 bit 整数の出力のみで、確率分布・多次元テンソルの一括生成・並列用の独立ストリーム分岐を持たない。常時ロードの `Random`（スカラー分布）と opt-in の `RandomTensor`（テンソル生成・多変量分布・軸シャッフル）を分離することで、数値計算を使わないプログラムの IR を 1 byte も変えずに NumPy 級の乱数機能を提供する。

## 着手条件と停止条件

### 着手条件

- Phase 1 は [NC01](NC01-complex-and-extended-math.md) が done。
- Phase 2 は [NC02](NC02-tensor-core-views-and-broadcasting.md) が done（`multivariate_normal` は [NC08](NC08-linalg-core-and-einsum.md) も done）および決定事項 D1 の承認。

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

- `OPT_IN` 登録: `uses: &["Tensor", "Linalg"]`（ソース自身が直接参照する opt-in モジュールのみ。推移閉包は `sources_for` が自動解決する）。`RESERVED_MODULES` に `"RandomTensor"` を追加し `src/stdlib.rs` の `reserves_the_d07_table` の件数アサーションを更新する（83 → 84）。`scripts/check-cli-docs.mjs` の `STD_PAGES` に `RandomTensor: 'stdlib/numeric/random-tensor.md'` を登録。
- 形状指定テンソル生成（すべて `pure def`、対象 `own Random.Pcg64` が最後で `(Tensor<_> * Random.Pcg64)` を返す）:
  - `uniform`, `standard_normal`, `normal`, `integers`, `exponential`, `gamma`, `beta`, `chisquare`, `laplace`, `lognormal`, `standard_t`, `f_dist`, `bernoulli`, `binomial`, `poisson`, `geometric`（例: `RandomTensor.normal loc scale shape rng -> (Tensor<f64> * Random.Pcg64)`）
  - 多変量分布: `dirichlet :: ref Tensor.View<f64> -> own [i64] -> own Random.Pcg64 -> (Tensor<f64> * Random.Pcg64)`、`multivariate_normal :: ref Tensor.View<f64> -> ref Tensor.View<f64> -> own [i64] -> own Random.Pcg64 -> Result<(Tensor<f64> * Random.Pcg64), Linalg.Error>`（`Linalg.cholesky` 使用）
  - 順列: `permutation :: own i64 -> own Random.Pcg64 -> (Tensor<i64> * Random.Pcg64)`
- テンソル操作（既存の `Array.random_shuffle_in_place_with` / `Array.random_choice_with` と一貫させ、操作・書き換え対象のテンソルを最後の引数にする: D-59）:
  - `shuffle_axis :: Copy<'a> => own i64 -> own Random.Pcg64 -> ref mut Tensor.Mut<'a> -> Random.Pcg64`（`ref mut` 引数を取るため非 `pure` の `def`。`Copy` は「方針の最新化」の D9）
  - `choice_weighted :: Copy<'a> => own i64 -> own bool -> ref Tensor.View<f64> -> own Random.Pcg64 -> ref Tensor.View<'a> -> (Tensor<'a> * Random.Pcg64)`（`pure def`）

### 3. パラメータ検証

- 不正な分布パラメータ（例: `scale < 0.0`、`p < 0.0 || p > 1.0`、`shape <= 0.0`、`low >= high`）は計算前に `assert` でトラップし、サイレントな `NaN` 汚染を防ぐ。NaN と無限大のパラメータも同じくトラップする（D8）。`RandomTensor` の関数は、形状の要素数が 0 でも先に検査する。

## 設計

- `Pcg64` の 128 bit 乗算は Tsuzuri 組み込みの `i128u` 演算をそのまま使えるため、純 Tsuzuri で簡潔かつ高速に実装できる。
- `standard_normal` は Leva の一様乱数の比（1992。表なし・状態なし。D5）、`gamma` は Marsaglia-Tsang 法（`shape < 1` は同じ論文の `gamma (shape + 1) * u ** (1 / shape)`。D6）を採用し、すべての基本演算で組み込み `Math` のビット一致関数を使用する。
- `tests/std_argument_order.rs` の `MODULES` には `Random` を `subject(&["Pcg", "Pcg64", "SeedSequence"])`、`RandomTensor` を `subject(&["Random.Pcg64", "Tensor.Mut", "Tensor.View"])`（生成系は `Pcg64` が最後、操作系は `Tensor.Mut`/`Tensor.View` が最後）として登録する。

## 前提とする他チケットのインターフェース

- [NC01](NC01-complex-and-extended-math.md): `Math.log1p`, `Math.expm1`, `Math.log`, `Math.sqrt`。
- [NC02](NC02-tensor-core-views-and-broadcasting.md): `Tensor`, `Tensor.View`, `Tensor.Mut`（Phase 2）。
- 弱い依存 [NC08](NC08-linalg-core-and-einsum.md): `Linalg.cholesky`（Phase 2 の `multivariate_normal` 用）。

## 他チケットへ提供するインターフェース

- `Random.Pcg64`, `Random.SeedSequence`, 各スカラー分布および `RandomTensor.*` 公開 API。

## 実装手順

1. **変更**: `std/Random.tz`, `src/stdlib.rs`, `tests/capabilities.rs`, `tests/fixtures/capabilities/std-table.txt`, `tests/std_argument_order.rs`, `tests/std_examples.rs`, `tests/fixtures/effects_sweep/`。**内容**: Phase 1（`Pcg64`・`SeedSequence`・`pcg64_from_os`・全スカラー確率分布と権限表・コーパス網羅更新）を実装する。**確認**: T0、`cargo test --locked --test capabilities --test std_argument_order --test std_examples --test effects_alloc_conservative`、`tsuzuri test --doc std`、T1。
2. **変更**: `std/RandomTensor.tz`, `src/stdlib.rs`, `tests/random_distributions.rs`, `tests/std_argument_order.rs`, `tests/std_examples.rs`, `tests/fixtures/effects_sweep/`, `scripts/check-cli-docs.mjs`, `vsc/src/core.ts`。**内容**: Phase 2（opt-in `RandomTensor` のテンソル一括生成・多変量分布・`permutation`/`shuffle_axis`/`choice_weighted`）を実装する。**確認**: T0、`tests/random_distributions.rs`、`effects_alloc_conservative`、`tsuzuri test --doc std`、T1。
3. **変更**: `tests/fixtures/`・`tests/features.mjs`・`tests/random-cases.mjs`、`docs/language.md`、Tsuzuri-Reference の `docs/stdlib/sys/random.md`・`docs/stdlib/numeric/random-tensor.md`（新規）・`docs/index.md`・`docs/stdlib/conventions.md`。**内容**: native/WASM `-O0`/`-O3` ビット一致 E2E と文書更新。**確認**: T2、`node scripts/check-docs.mjs`、`node scripts/check-cli-docs.mjs`。

## テスト計画

- **統計・再現性テスト（`tests/random_distributions.rs`）**: 同一シードでの完全一致、異なる `seed_sequence_spawn` 子間の非相関、各分布（一様・正規・指数・ガンマ・ベータ・ポアソン・二項・カイ二乗・ディリクレ・多変量正規）の標本平均・標本分散・上下限境界（10,000〜50,000 サンプルで理論値と $4\sigma$ 以内の一致）、不正パラメータトラップ。
- **E2E（`tests/features.mjs`）**: native と WASM の `-O0`・`-O3` で同一の乱数テンソルビット列が出ること、WASM インポートなし、`live == 0`。

## ドキュメント

- `docs/language.md`（「環境・時刻・乱数・プロセス」「RandomTensor」ほか）
- 別リポジトリ Tsuzuri-Reference: `docs/stdlib/sys/random.md`、`docs/stdlib/numeric/random-tensor.md`（新規）、`docs/index.md`、`docs/stdlib/conventions.md`、`docs/stdlib/numeric/tensor.md`・`linalg.md` の関連項目

## 受け入れ条件

- シード指定の全分布が native と WASM の `-O0`・`-O3` でビット完全一致し、WASM インポートを増やさないこと。
- `RandomTensor` を使わない既存プログラムに `Tensor`/`Linalg` がロードされず、既存 `Random.Pcg` の互換性が保たれること。
- `capabilities`・`effects_alloc_conservative`・`std_argument_order`・`check-cli-docs`・T2 がすべて通ること。

## 落とし穴

- `pure def` は `mut` / `ref mut` 引数を取れない（`E1029`、`src/pure_rules.rs:134-141`）。また `Random.next_u64` は既存 `IO` 関数として定義済み（`std/Random.tz` の `def next_u64`）のため、新関数は必ず `pcg64_*` 命名と `own Pcg64 -> (T * Pcg64)` の状態引き回し形式にすること。
- `std/Random.tz` は常時ロードモジュールのまま保つため、`Tensor` や `Linalg` の型名を `std/Random.tz` に書いてはならない（全プログラムのコンパイルエラーまたは IR 変化を引き起こす）。また `RandomTensor` の `OPT_IN.uses` はソース内で直接名指しするモジュールのみを列挙し、`src/stdlib.rs` の `reserves_the_d07_table` の `RESERVED_MODULES.len()` アサーションも同時更新すること。
- `pcg64_from_os` は `Random.bytes` 経由で `random` 権限を持つため、`tests/fixtures/capabilities/std-table.txt`（および `tests/capabilities.rs` の行数アサーション）の更新を忘れると T1 が失敗する。
- `RandomTensor` のページは `docs/stdlib/numeric/random-tensor.md` で、既定の `stdlib/randomtensor.md` と違うため、`scripts/check-cli-docs.mjs` の `STD_PAGES` 登録が必須。

## 対象外

- グローバル可変の暗黙 RNG（`np.random.seed()` 相当）。
- NumPy の `numpy.random.PCG64` とビット単位で同一の乱数列を再現すること（シード展開や分布アルゴリズムが異なるため、Tsuzuri 独自のシード固定再現性と native/WASM `-O0`/`-O3` ビット一致を契約とする）。
- 暗号論的疑似乱数生成器（CSPRNG の OS エントロピー取得は既存 `Random.bytes`/`Random.next_u64` を使用）。

## 決定事項

- **D1（承認済み。D-65 の委任で決定、2026-11-28）**: 常時ロードの `Random` から opt-in の `Tensor`/`Linalg` への逆依存を避けるため、テンソル乱数生成・多変量分布・軸シャッフルを新規 opt-in std モジュール **`RandomTensor`**（`std/RandomTensor.tz`）へ分離し、`RESERVED_MODULES` に登録する（`D-55` へ反映）。
- **D2（既定案）**: `pure def` 契約および既存 `Random.pcg_next_u32`・`Array.random_*_with` と完全に揃えるため、生成器は `own Pcg64 -> (値 * Pcg64)` の純粋な状態引き回し形式とし、`Random` 側の新関数名は既存 `Random.next_u64` と衝突しない `pcg64_*` に統一する。生成器のみを操作する関数では `own Pcg64` を最後の引数、既存テンソルを操作する関数（`shuffle_axis`/`choice_weighted`）では対象テンソルを最後の引数とする（D-59）。
- **D3（既定案）**: グローバル可変の暗黙 RNG（旧 `np.random.seed()` 相当）は導入しない。必ず明示的な `Pcg64` 値または `SeedSequence` を渡すことで、純粋性・再現性・並行安全性を保証する。

次の D4〜D12 は、利用者の委任（2026-11-28。「あなたが考える最高の選択肢で実装することを常に許可します」）に基づく決定です。

- **D4（委任による決定、2026-11-28）**: `seed_sequence`・`seed_sequence_spawn` の混ぜ方は NumPy の `SeedSequence`（32 bit 語の hashmix、プール 4 語、`spawn_key`）と同じ算法にし、`pcg64_from_seed` は NumPy の `PCG64` と同じ手順（プールから 64 bit を 4 語取り、前半を状態、後半をストリームにして `pcg_setseq_128_srandom_r`）にする。`pcg64 seed` は `pcg64_from_seed (seed_sequence [seed])` である。この結果、`Random.pcg64 seed`・`pcg64_next_u64`・`pcg64_next_f64` の列は `numpy.random.PCG64(seed).random_raw()`・`Generator.random()` と同じになる。理由: (1) 小さい整数のシードでも状態の全ビットが混ざる、(2) 子ストリームの作り方が実績のある設計である、(3) 独立した参照（NumPy が出した数）でビット単位に検査できる。**分布の標本を NumPy と同じにすることは保証しない**（対象外のまま。NumPy の `Generator` は正規・指数・ガンマに Ziggurat を使い、Tsuzuri は使わない）。採らなかった案: 独自の SplitMix64 による展開（参照がなく、子ストリームの独立性を自分で示す必要がある）。
- **D5（委任による決定）**: `pcg64_standard_normal` は Leva の一様乱数の比（ACM TOMS 18、1992）。表を持たず、状態を持たず、厳密で、対数は標本 80 個に 1 回ほどしか計算しない（モデルの実測で 1 標本あたり対数 0.012 回・一様乱数 2.74 個）。採らなかった案: Marsaglia polar（対で 2 個できるが、`own Pcg64 -> (f64 * Pcg64)` の形では 2 個目を捨てるので、1 標本に対数 1 回・平方根 1 回・一様乱数 2.5 個かかる）、Ziggurat（256 段の表 3 本を常時ロードの `std/Random.tz` に書くことになり、全プログラムの字句解析に載る）。
- **D6（委任による決定）**: `gamma` の `shape < 1` は Marsaglia-Tsang の論文の方法（`shape + 1` の標本に `u ** (1 / shape)` を掛ける）。厳密で、追加は一様乱数 1 個と `Math.pow` 1 回。`beta` は両方のパラメータが 1 以下のとき Jöhnk の方法（2 つのガンマがともに 0 に丸まって `0 / 0` になるのを避ける）、それ以外は 2 つのガンマの比。`binomial` は逆関数法（`n * min p (1 - p) <= 30`）と BTPE、`poisson` は一様乱数の積（`lam < 10`）と PTRS、`geometric` は全域で逆関数法（一様乱数が 1 に近いとき止まらなくなりうる探索のループを持たない）。
- **D7（委任による決定）**: `RandomTensor` の生成関数は、`Random` の同名のスカラー関数を行優先の順に 1 要素 1 回呼んだ結果とビット単位で同じにする。`[100, 10]` の生成は 1000 回のスカラー生成と同じで、`[50, 10]` を 2 回続けても同じになる。対で作る正規乱数などで速くする案は、分割すると列が変わるので採らない。
- **D8（委任による決定）**: パラメータの検査は、チケットの例に加えて NaN と無限大も拒否する（`loc`・`mean` は有限、`scale`・`sigma` は 0 以上の有限、`shape`・`a`・`b`・`df` は正の有限、`p` は `[0, 1]`（`geometric` は `(0, 1]`）、`lam` は `[0, 9.223372006484771e18]`）。`scale == 0` などの退化した値は許す（`normal loc 0.0` は `loc`）。
- **D9（委任による決定）**: `shuffle_axis` は `Copy<'a>` を要求する。`Tensor.freeze` と `Tensor.take` で並べ替えた写しを作り、`Tensor.copy_from` で書き戻す（要素数に比例する作業領域を 1 つ使う）。順序は `permutation` と同じ（`i` 番目は元の `order[i]` 番目）。`Tensor.Mut` に入れ替えの API を足す案は、`std/Tensor.tz` を変えるので採らない。
- **D10（委任による決定）**: 整数の区間は半開 `[low, high)`。`low >= high` はトラップ。幅は 2^64 − 1 まで取れる（`i64` の最小値から最大値の 1 つ手前まで）。
- **D11（委任による決定）**: `choice_weighted count replace weights rng source` は `source` の軸 0 の切片を選ぶ。重みは合計で割って確率にする（合計が 1 である必要はない）。非復元は、重みが正の切片ごとに率 `weight` の指数分布の時計を 1 つ引き、小さい順に `count` 個を取る（逐次の非復元抽出と同じ分布。一様乱数は `count` によらず正の重みの個数だけ使う）。`dirichlet` は、パラメータがすべて 0.1 未満のときベータ分布の連鎖で作る（ガンマがすべて 0 に丸まるのを避ける）。`multivariate_normal` の形の不一致は `Linalg.Error.ShapeMismatch`、対称正定値でない共分散は `Linalg.Error.NotPositiveDefinite` で、エラーのときは乱数を引かない。
- **D12（委任による決定）**: D-55 に、std のモジュール `RandomTensor`（`RESERVED_MODULES` 83 → 84、`stdlib::OPT_IN`）と、std の型 `Random.Pcg64`・`Random.SeedSequence`（不透明）を登録する。新しい診断コード・予約語・組み込みはない。

## 実装と検証（2026-11-28）

Phase 1・2 を実装しました（ブランチ `feat/nc10-random-distributions`、基点は `main` の `b72bccd0`）。

### 実装した内容

- **Phase 1（`std/Random.tz`、常時ロード）**: 不透明な `record Pcg64 { state: i128u, increment: i128u }`（`Copy`）と `record SeedSequence`（種の 32 bit 語、分岐のキー、プール 4 語、分岐した子の数）。公開関数は 23 個: `pcg64`・`pcg64_from_seed`・`pcg64_from_os`・`seed_sequence`・`seed_sequence_spawn`・`pcg64_next_u64`・`pcg64_next_f64`・`pcg64_next_i64_range` と、分布の `pcg64_uniform`・`pcg64_standard_normal`・`pcg64_normal`・`pcg64_exponential`・`pcg64_gamma`・`pcg64_beta`・`pcg64_chisquare`・`pcg64_laplace`・`pcg64_lognormal`・`pcg64_standard_t`・`pcg64_f_dist`・`pcg64_bernoulli`・`pcg64_binomial`・`pcg64_poisson`・`pcg64_geometric`。`pcg64_from_os` を除く 22 個は `pure def`。128 bit の乗算は組み込みの `i128u` をそのまま使う。`std/Random.tz` は `Tensor`・`Linalg` を名指さない（テストで検査）。
- **Phase 2（`std/RandomTensor.tz`、opt-in、`uses: ["Tensor", "Linalg"]`）**: 公開関数は 21 個: `uniform`・`standard_normal`・`normal`・`integers`・`exponential`・`gamma`・`beta`・`chisquare`・`laplace`・`lognormal`・`standard_t`・`f_dist`・`bernoulli`・`binomial`・`poisson`・`geometric`、`dirichlet`・`multivariate_normal`・`permutation`・`shuffle_axis`・`choice_weighted`。`shuffle_axis` を除く 20 個は `pure def`。
- **登録**: `src/stdlib.rs`（`SOURCES`、`RESERVED_MODULES` 83 → 84、`OPT_IN`、`opaque_record` に `Random.Pcg64`・`Random.SeedSequence`）、`tests/std_argument_order.rs`（`Random` を `subject(&["Pcg", "Pcg64", "SeedSequence"])`、`RandomTensor` を `subject(&["Random.Pcg64", "Tensor.Mut", "Tensor.View"])`）、`tests/std_examples.rs`（`Random` 5 → 28、`RandomTensor` 21）、`tests/fixtures/capabilities/std-table.txt`（`Random.pcg64_from_os: random`。90 → 91 行）、`tests/fixtures/effects_sweep/Main.tz`（`Random.pcg64_from_os`）、`scripts/check-cli-docs.mjs`（`STD_PAGES`）、`vsc/src/core.ts`（`libraryModules`）。

### 期待値の出どころ

- **生の列**: NumPy 2.5.1 が出した数（`PCG64(seed).random_raw()`、`SeedSequence(...).generate_state`、`spawn` の子と孫、`Generator.random()`）。`tests/random_distributions.rs` に直接書き、`tests/random-cases.mjs` の BigInt のモデルも同じ数で検査する。
- **分布の標本**: `tests/random-cases.mjs` のモデル（論文の算法を JavaScript の倍精度で書いたもの）。モデルの二項分布（逆関数法・BTPE・`p > 0.5`・`n = 10^12`）とポアソン分布（積・PTRS・`lam = 10`）は、NumPy の `Generator.binomial`・`Generator.poisson` の出力と一致することをモジュールの読み込み時に検査する（NumPy も同じ算法と同じ生の列を使うため）。整数の結果は厳密に、実数の結果は相対誤差 `1e-11` で比べる（モデルの対数は V8、コンパイラは同梱の musl で、最後の桁が違いうる）。
- **分布の形**: 各分布の平均・分散・尖度の公式と、分布関数の値（正規は `erfc`、ガンマは不完全ガンマ関数の級数、二項・ポアソンは厳密な和、ほかは閉じた式。Python で計算）。固定した種の 4 万〜40 万標本が標準誤差の 4 倍以内にあることを検査する。
- **ビット一致**: 期待値を持たず、native の `-O0`・`-O3` と wasm32 の `-O0`・`-O3` の 4 つの結果が互いに等しいことを検査する。

### 実行したコマンドと結果（macOS arm64、dev バイナリ）

| コマンド | 結果 |
| --- | --- |
| `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings` | 差分なし、警告なし |
| `cargo test --locked --test random_distributions` | 13 passed |
| `cargo test --locked --test std_argument_order --test std_examples --test capabilities --test effects_alloc_conservative --test pure_functions --test formatter` | 22・5・18・7・33・24 passed |
| `cargo nextest run --locked`（T1） | 3813 tests run、3813 passed、4 skipped（最終のコードの木）。その前の 1 回は 3812 passed・1 failed（`formatter corpus_is_idempotent_and_semantically_unchanged`。下の「チケットから外れた判断」の書き直しで解消） |
| `tsuzuri test --doc std` | 1394 passed、0 failed（うち `Random` 28、`RandomTensor` 21） |
| `node tests/features.mjs target/debug/tsuzuri random` | 585 cases と 52 traps。native／WASM の `-O0`／`-O3`、WASM の import なし、`live == 0` |
| `node tests/features.mjs target/debug/tsuzuri random_tensor` | 466 cases と 43 traps。同上 |
| `node tests/features.mjs target/debug/tsuzuri`（全 suite） | 途中で止めました（共有機の混雑で 35 分に 34 suite。止めるまでの 34 suite はすべて成功、失敗 0）。残りの suite は実行していません。既存のプログラムの IR が通し番号しか変わらないこと（下の節）が、既存の suite に影響がないことの根拠です |
| `node tests/os.mjs target/debug/tsuzuri` | 成功（native と wasm32 の `Random` を含む OS API、`-O0`／`-O3`）。`e2e.mjs`・`examples.mjs`・`io.mjs` は実行していません |
| `node scripts/gen-docs.mjs --check`、`node scripts/check-cli-docs.mjs`、`node scripts/check-guide-lite.mjs`、`node scripts/check-spec.mjs`、`node scripts/check-links.mjs` | 11 blocks・0 stale、4 checks・0 failed、87 references・0 failed、ok、0 broken |
| Tsuzuri-Reference の `node scripts/check-docs.mjs <変更した 8 ページ>` | 33 examples、66 native runs（`-O0`／`-O3`）、すべて成功 |

`honors_the_exact_specialization_limit`・`reserves_the_d07_table`・`opt_in_modules_are_reached_only_through_their_names` は T1 に含まれ、成功しています。

### 既存のプログラムの IR（停止条件 S2）

`tests/fixtures/*/Main.tz` と `examples/*/Main.tz` の 168 プロジェクトの `build --emit llvm` を、変更の前後で比べました。変更していない 167 プロジェクトのうち 63 はバイト単位で同じ、104 は内部シンボルの通し番号だけが違い（番号を消して比べると同じ）、ほかの差はありません（残りの 1 つは、このチケットが 1 行足した `tests/fixtures/effects_sweep`）。内部シンボルの通し番号は、常時ロードの `std/Random.tz` に宣言が 36 個増えた分だけずれます（`@tz.fn.Shapes.$instance.850.area` が `.886.` になる形）。使わない関数の本体・宣言・文字列は出力されません。`RandomTensor` を名指さないプログラムに `@tz.fn.Tensor.`・`@tz.fn.Linalg.`・`@tz.fn.RandomTensor.` が出ないことは `random_tensor_is_opt_in_and_random_does_not_reach_it` と suite `random` の `inspect` が検査します。

### 計測（参考。性能の主張ではありません）

`-O3`、Apple Silicon、1000 万要素の生成と合計を 1 回ずつ（共有機、1 回だけ）: `RandomTensor.uniform` 約 7 ns／要素、`standard_normal` 約 23 ns、`gamma 2.5 1.0` 約 33 ns、`integers 0 1000` 約 3 ns、`poisson 250.0` 約 35 ns、`binomial 1000 0.4` 約 47 ns。生成コードの確認と、他の実装との同条件の比較はしていません。

### 検証していないこと

- **wasm64**: この機械に Node.js 24 がなく、確かめていません。文書は wasm32 までを述べています。
- **macOS arm64 以外の native**: CI（`ci:full`）に任せます。suite `random`・`random_tensor` の期待値はモデルが実行時に作るので、Node.js の版で `Math.log`・`Math.pow` の結果が変わると、実数の照合（相対 `1e-11`）ではなく、棄却の判定が 1 ulp の差で入れ替わる場合にだけ整数の照合が変わりえます（起きる見込みは非常に小さい）。
- **`pcg64_poisson` の非常に大きい `lam`**: `1e15` まではモデルと一致しますが、判定式の丸め誤差が効く範囲（目安 `1e13` 以上）の分布の精度は測っていません（文書に注意を書きました）。

### チケットから外れた判断

「方針の最新化」と D4〜D12 のとおりです。加えて次の 2 点:

- `binomial_btpe` の 3 つの分岐は、`elif` の本体の最後を `else` のない `if` で終えると `tsuzuri fmt` が意味の変わる整形を検出して拒否する（`tests/formatter.rs` の corpus の検査が落ちる）ので、`check = 条件` の代入に書き直しました。整形器の側は直していません（別の不具合として残ります: `elif` の本体が 2 文以上の本体を持つ `else` なしの `if` で終わり、その後に外側の `elif` が続く形）。
- `tests/capabilities.rs` のテスト名 `std_table_file_lists_the_88_functions` は行数（91）と合っていませんが、起票前からのずれなので変えていません。

### 更新した文書

- `docs/language.md`: 予約モジュール名、純粋関数の一覧の文、OS API の表と「環境・時刻・乱数・プロセス」（`Pcg64`・`SeedSequence`・分布の表・検査・再現性）、WASM の節、新しい節「RandomTensor」、`E1022` の不透明なレコード。
- `README.md`: OS API の文（`Random.Pcg64`）と、テストの表の行。
- `_features/GUIDE.md`（D-55 の登録、D-65 の委任による決定）、`_features/README.md`（一覧と計画の表）。
- Tsuzuri-Reference（ブランチ `docs/nc10-random-distributions`）: `docs/stdlib/sys/random.md`（大幅に追記）、`docs/stdlib/numeric/random-tensor.md`（新規。「引数の順序」あり）、`docs/index.md`、`docs/stdlib/conventions.md`、`docs/modules/imports-and-visibility.md`、`docs/expressions/functions.md`、`docs/modules/packages.md`、`docs/stdlib/numeric/tensor.md`。

### 変更したファイル

`std/Random.tz`、`std/RandomTensor.tz`（新規）、`src/stdlib.rs`、`tests/random_distributions.rs`（新規）、`tests/random-cases.mjs`（新規）、`tests/fixtures/random/Main.tz`（新規）、`tests/fixtures/random_tensor/Main.tz`（新規）、`tests/features.mjs`、`tests/std_argument_order.rs`、`tests/std_examples.rs`、`tests/capabilities.rs`、`tests/fixtures/capabilities/std-table.txt`、`tests/fixtures/effects_sweep/Main.tz`、`scripts/check-cli-docs.mjs`、`vsc/src/core.ts`、`docs/language.md`、`README.md`、`_features/GUIDE.md`、`_features/README.md`、このチケット。
