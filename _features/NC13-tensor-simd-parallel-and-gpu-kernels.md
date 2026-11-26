# NC13: テンソル向けハードウェア高速化（x86-64 FMA・`f16` 拡充・SIMD/Parallel/タイリング GEMM・GPU カーネル）

| 項目 | 内容 |
| --- | --- |
| ID | NC13 |
| 優先度 | P2（外部 BLAS なし環境や WASM・GPU で実用的な数値計算性能を達成する） |
| 規模 | XL（4 Phase に分割: Phase 1 x86-64 FMA & `f16` SIMD、Phase 2 テンソル SIMD & 軸並列カーネル、Phase 3 キャッシュタイリング GEMM、Phase 4 GPU 2D/リダクション/複数バッファ/行列積） |
| 依存 | [NC02](NC02-tensor-core-views-and-broadcasting.md), [NC06](NC06-tensor-ufuncs-and-elementwise.md), [NC07](NC07-tensor-reductions-stats-and-sorting.md), [NC08](NC08-linalg-core-and-einsum.md), ([NC15](NC15-parallel-borrowed-chunk-input.md)) |
| 後続 | –（弱い後続: [M12](M12-gpu-presentation-to-window.md)。M12 の Phase 2（CAMetalLayer への提示）は、このチケットの D2（Metal）が決まってから着手する） |
| 状態 | todo |
| 起票 | 2026-11-24（[GUIDE D-65](GUIDE.md#d-65-数値計算基盤と-numpy-互換機能案-c-ハイブリッドnc01nc14)。HEAD `27069465` で確認） |
| 承認 | 要承認（Phase 4 の GPU API 拡張・新規 GPU バックエンド検討。決定事項 D1・D2 が要承認、Phase 1〜3 の意味保存最適化は既定案） |
| 手本にする既存実装 | `src/llvm_math.rs`（AArch64 `llvm.fma` 判定）、`src/llvm_simd.rs`、`src/simd.rs`、`std/Matrix.tz`（`mul_parallel`）、`std/Gpu.tz`・`src/gpu.rs`・`src/llvm_gpu.rs`、`benchmarks/` |
| 主な影響ファイル | `src/llvm_math.rs`、`src/llvm_cpu.rs`、`src/target.rs`、`src/runtime/cpu.c`、`src/simd.rs`、`src/llvm_simd.rs`、`src/check.rs`、`src/gpu.rs`、`src/llvm_gpu.rs`、`std/Tensor.tz`、`std/Matrix.tz`、`std/Linalg.tz`、`std/Gpu.tz`、`std/Parallel.tz`、`benchmarks/`、`docs/benchmarks.md`、`docs/architecture.md`、`_tsuzuri/language-reference/built-in-types-and-modules/simd.md`、`_tsuzuri/language-reference/built-in-types-and-modules/gpu.md`、`_tsuzuri/language-reference/built-in-types-and-modules/matrix.md`、`_tsuzuri/language-reference/built-in-types-and-modules/tensor.md` |

## 実装者向け早見

- 範囲: 数値の意味（丸め・NaN・符号付きゼロ・決定性）を壊さずに、Tsuzuri 自身のコード生成と標準ライブラリで実践的な性能を引き上げる 4 Phase の最適化:
  - **Phase 1（CPU 命令・`f16` SIMD 基盤）**:
    - x86-64 で FMA3 が有効なターゲット（`--cpu native` や `@cpu ["avx2"]`・`+fma` 機能付き）の `Math.fma` を `tz_soft_fma` ではなくハードウェア命令 `llvm.fma` へ下げる（現行は AArch64 のみで x86-64 が約 1 万倍遅い問題の解消）。
    - `f16` の `Simd`（`f16x8`, `f16x16`）対応を段階的に拡充（`f16` の `Elementary` 超越関数は [NC01](NC01-complex-and-extended-math.md) の対象外のまま据え置く）。
  - **Phase 2（テンソル SIMD & `Parallel` 軸並列）**:
    - 連続およびブロードキャスト `Tensor` ufunc（`add`/`sub`/`mul`/`div`/`fma` 等）の SIMD 化・`@cpu` 多版化と、大規模テンソル向けの明示的並列 API（`Tensor.map_parallel`, `Tensor.add_parallel`, `Tensor.sum_axis_parallel` 等。チャンク境界は形状のみで決めてビット一致を維持）。
  - **Phase 3（キャッシュタイリング GEMM）**:
    - `Matrix.mul` / `Linalg.matmul` に対し、各出力要素の加算順序（`k = 0, 1, ...` 昇順）を一切変えないまま L1/L2 キャッシュタイリング（パネルパッキング＋レジスタブロッキング）を導入し、同一ビット結果のままキャッシュ効率を大幅改善する。
  - **Phase 4（GPU 数値カーネル拡張）**:
    - 現行 [Gpu.tz](../std/Gpu.tz) の「1D・1 入力・キャプチャなし `init`/`map`」の壁を超え、複数バッファ入力（`Gpu.map2` による要素ごと二項演算）・リダクション（`Gpu.reduce`）・2D 行列積カーネル（`Gpu.matmul` / `Gpu.matmul_relaxed`）を追加し、能力検査と明示的バックエンド選択（`WebGpu`/`Vulkan`/`Auto`、可能なら `Metal` 検討）を整備する。
- 最初に読む: ① `AGENTS.md`（性能ポリシー: 実測・生成コード確認・決定性維持・CI に速度閾値を入れない） ② `src/llvm_math.rs` の `MathFma` ③ `std/Matrix.tz` の `multiply_rows` と `docs/benchmarks.md` ④ `src/gpu.rs`。
- 最初に書くテスト: Phase 1 で x86-64 `+fma` 向け `--emit llvm` に `llvm.fma.f64` が出て `+fma` なしでは `tz_soft_fma` のままであること、かつビット結果が完全一致すること。
- 触ってはいけない: 暗黙の `fast-math`・再結合・暗黙 FMA の禁止（GUIDE §1）。共有 CI への速度 pass/fail 閾値の追加。
- 完了の見分け方: 各 Phase で (1) 生成アセンブリ／LLVM IR・WGSL/SPIR-V を検査、(2) 独立参照および決定論基準経路とビット／精度照合、(3) `benchmarks/`（9 サンプル以上の schema-1 JSONL）で実測し `docs/benchmarks.md` に記録する。

## 目的

案 C の純 Tsuzuri 基盤（[NC02](NC02-tensor-core-views-and-broadcasting.md)〜[NC08](NC08-linalg-core-and-einsum.md)）が機能面で揃った後、外部 BLAS に頼れない環境（単一バイナリ配布・WASM・GPU）でも実用に耐える計算速度を、Tsuzuri の安全・決定論ポリシーを一切妥協せずに達成する。

## 着手条件と停止条件

### 着手条件

- Phase 1 は独立着手可能。Phase 2〜4 は [NC02](NC02-tensor-core-views-and-broadcasting.md)・[NC06](NC06-tensor-ufuncs-and-elementwise.md)・[NC07](NC07-tensor-reductions-stats-and-sorting.md)・[NC08](NC08-linalg-core-and-einsum.md) が done。
- Phase 4 は決定事項 D1・D2 の承認。

### 停止条件

- **S1** 最適化の前後で、決定論契約のある API（`Matrix.mul`, `Linalg.matmul`, `Math.fma`, `Tensor.add` 等）の非 NaN ビット結果が 1 bit でも変化した場合。
- **S2** 実測（matched workload、9 サンプル以上）でベースラインより速くならない、またはコード複雑度に見合う改善が出ない最適化は採用せず見送る（AGENTS.md「ハードウェアを増やすこと自体は性能結果ではない」）。
- **S3** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `27069465` で確認）

- `_tsuzuri/language-reference/built-in-types-and-modules/matrix.md` の記載どおり、`Math.fma` が `llvm.fma` になるのは AArch64 native の `f32`/`f64` のみで、x86-64 native（AVX2/FMA 搭載機でも）と WASM は `tz_soft_fma` になり約 1 万倍遅い。
- `std/Matrix.tz` の `mul` はブロッキングなしの素朴な i-k-j ループ。
- `src/simd.rs` は `f16` ベクトル未対応、ベクトル超越関数なし。
- `std/Gpu.tz` は 1D `init`/`map` のみ、`Cuda`/`Metal` は `Error Unavailable`。

## 変えてはいけない意味

- オーバーフロー・丸め・`NaN`・符号付きゼロ・評価順序・トラップ・所有権・借用。
- 既定 WASM のインポートなし、および `--wasm-feature simd128` なしのポータブル経路。
- 明示指定した利用不可 GPU バックエンドが黙って CPU へフォールバックしない契約。

## 仕様

### 1. Phase 1: x86-64 FMA lowering と `f16` SIMD 拡充

- `src/llvm_math.rs`・`src/llvm_cpu.rs`・`src/target.rs`・`src/runtime/cpu.c`:
  - 現状 `src/llvm_cpu.rs:15` の level 2（`"avx2"`）の target-features は `"+avx2"` のみで `+fma` を含んでおらず、`multiversion`（78 行）はベースライン向けに生成済みのテキスト IR を複製する仕組みになっている。そのため、① `cpu.c` のレベル 2/3 判定に CPUID の FMA3 ビットを含め、`llvm_cpu.rs` の level 2/3 feature に `,+fma` を追加し、② `@cpu ["avx2"]`・`["avx512"]` の多版クローン時（および `--cpu native` かつターゲットが `+fma` を持つとき）に `@tz_soft_fma` 呼び出し（`f32`/`f64`）を `@llvm.fma.f32` / `@llvm.fma.f64` へ差し替える（`+fma` を持たないベースライン x86-64 版は `tz_soft_fma` のまま保ち、精度不明な C `libm` の `fma` へ落ちるのを防ぐ）。`src/target.rs:666` の `hardware_fma()` 単体テストも同期更新する。
  - **Windows x86-64 の扱い**: `src/target.rs:368-375` は現在 `(Arch::X86_64, Os::Windows) => 0` のため `@cpu` 多版化が無効になっている。Windows の `--cpu generic` でも FMA3 を活かすには `cpu.c` の Windows x86-64 CPUID 検出と `target.rs` の `cpu_levels()` 有効化を同時に行うか、Phase 1 の時点では Windows は `--cpu native` 限定であることを文書へ明記する。
- `src/simd.rs` / `src/llvm_simd.rs` へ `f16x8`（128 bit）・`f16x16`（256 bit）および対応マスクを追加検討（ターゲットの `half` ハードウェア対応有無に応じたポータブル低下を含む）。

### 2. Phase 2: テンソル SIMD & 軸並列カーネル（計測先行）

- **1 入力の並列化と単一パス複合処理の先行検証**:
  - 1 入力の新規確保版（例: `Tensor.exp_parallel`・`Tensor.map_parallel`）は、既存の **`Parallel.map`** を使えば入力コピーなしで直ちに並列化でき、既定 WASM でも逐次フォールバックする（`Parallel.for_each_chunk` は借用環境を捕捉できず `E1013` になるため使わない）。まず連続 `f64` の `exp` 1 関数と、`Tensor.map_into (\x -> Math.exp (x - bias))` の単一パス化（Clang `-O3` 通過後の IR／逆アセンブル確認）で、並列化固定費を回収できる閾値と効果を測定する。
- **2 入力・`_into`・軸並列（[NC15](NC15-parallel-borrowed-chunk-input.md) 連携）**:
  - `Tensor.add_parallel`, `Tensor.add_into_parallel`, `Tensor.sum_axis_parallel` 等の複数入力／書き込み先指定の並列 API は、[NC15](NC15-parallel-borrowed-chunk-input.md) の `Parallel.for_each_chunk_with` を使って入力コピー 0 回で実装する（`Arc.new (Array.copy ...)` で回避しない）。チャンク境界は形状のみで決めてスレッド数に依存しない決定論的ビット結果を保証し、`Parallel.*` が `Category::Task` のため非 `pure` の別 API（`*_parallel`）として分離を保つ。
- **明示 SIMD と超越関数 SIMD の契約**:
  - `Simd` 命令は具象ベクトル型でしか書けず（型変数 `'a` のまま不可）、[C11](_completed/C11-multidimensional-arrays.md) の実測では手書き `f64x2` が LLVM の自動ベクトル化より遅かった教訓がある。連続 `Tensor` 演算はまず **Clang `-O3` 通過後の生成アセンブリ**を確認し、明示 SIMD 化は実測で自動ベクトル化を明確に上回る箇所に絞る。
  - 同梱 `musl` の `exp.c`/`log.c`/`sin.c` は 128 要素テーブル引きとデータ依存分岐を含み、NEON・WASM SIMD128 には 64-bit gather 命令がない。したがって**既定の `Tensor.exp`/`sin`/`log` はビット一致契約（`docs/language.md:4616`）を守ってスカラー `Math.*`（および `*_parallel`）のまま維持する**。並列化後も単スレッド超越関数がボトルネックであると実測された場合のみ、`Matrix.mul_fma` や `Gpu.map_relaxed` と同じく**別名の 1-ulp バルク SIMD API**（要承認）として独立検討する。

### 3. Phase 3: キャッシュタイリング GEMM と段階的ブロック化分解

- 出力のブロック $(M_c, N_c)$ に対して内側 $k$ を昇順ブロック $K_c$ ごとに進めるタイリング＋パッキング（各 $(i, j)$ の更新式は厳密に `k = 0, 1, ..., K-1` の昇順 `out[i, j] += a[i, k] * b[k, j]` を保存するため、ビット完全一致を保ったまま L1/L2 キャッシュヒット率を最大化できる）。
- `Matrix.mul` / `Matrix.mul_parallel` / `Linalg.matmul` へ適用し、小行列ではオーバーヘッドを避けて既存ループへフォールバックする閾値を実測で決める。[NC08](NC08-linalg-core-and-einsum.md) の共通 `B` 行結合経路および [NC15](NC15-parallel-borrowed-chunk-input.md) のゼロコピー並列チャンクと組み合わせた上で、アーキテクチャ別の手書きマイクロカーネルが必要かを実測で判断する。
- **数値契約の厳格な分離**: 別丸めの厳密 `mul`/`matmul` と、`mul_fma`/`matmul_fma`（`k` 昇順でも別ビットになる）、さらに細長い行列向けに将来検討しうる固定木 Split-K（自らの固定木では再現可能だが逐次とは別ビット）は必ず別名の API として分離し、既定経路へ混ぜない。
- **分解のブロック化（計測ベースで狭く着手）**: LU/QR/SVD/固有値分解を一括でブロック化せず、まず GEMM 更新が支配的と実測されたら最小対象の `Linalg.cholesky` からブロック化を検証する。SVD・固有値分解は残差・直交性・収束性（`DidNotConverge`）を独立に評価する。

### 4. Phase 4: GPU 数値カーネル拡張（要承認）

- `Gpu.map2 :: (Copy<'a>, Copy<'b>) => ref Device -> own ('a -> 'b -> 'c) -> ref Buffer<'a> -> ref Buffer<'b> -> Buffer<'c>`（複数バッファ入力・ブロードキャストまたは同長二項カーネル）。
- `Gpu.reduce` / `Gpu.matmul` / `Gpu.matmul_relaxed`（2D ワークグループ＋共有メモリタイリングによる行列積。`relaxed` 以外は厳密契約または未対応時エラー）。
- 測定・自動切替は **ホスト往復（初期化＋転送＋ディスパッチ＋同期＋読み戻し）** と **デバイス常駐の反復実行** を必ず分離し、`f32`/`f16` と `f64` を区別したエンドツーエンド実測に基づいて `Gpu.Auto` のしきい値を更新する。

## 設計

- すべての Phase で、ベンチマークは `benchmarks/metrics.mjs` の schema-1 JSONL（9 サンプル以上）で計測し、**Clang `-O3` 通過後の**ネイティブ生成アセンブリ（`objdump`/`llvm-objdump`）と WASM 逆アセンブルを確認してから性能改善を報告する。
- **NumPy / SciPy との公正な同条件比較ルール**: `benchmarks/metrics.mjs:20` の `LANGUAGES` に `"python"` を追加（または専用比較ランナーを整備）し、dtype・形状・スレッド数（`TSUZURI_MAXPROCS` と BLAS/OpenMP/VecLib のスレッド制御）を揃えた **`wall_time`**（全スレッド合算の `cpu_time` ではない）で比較する。相手側も NumPy `out=`・バッチ一括 `matmul`・複数右辺 `solve`・SciPy `workers` 等の適切な API を使い、準備込み単発と反復定常状態の中央値（9 回以上）・ピーク作業メモリ・確保回数を分けて記録する。
- Phase 1〜4 は独立した PR として 1 つずつマージする。

## 前提とする他チケットのインターフェース

- [NC02](NC02-tensor-core-views-and-broadcasting.md)・[NC06](NC06-tensor-ufuncs-and-elementwise.md)・[NC07](NC07-tensor-reductions-stats-and-sorting.md)・[NC08](NC08-linalg-core-and-einsum.md): 最適化対象の `Tensor`/`Linalg` API と決定論基準実装。
- 弱い依存 [NC15](NC15-parallel-borrowed-chunk-input.md): `Parallel.for_each_chunk_with`（Phase 2 の 2 入力/`_into`/軸並列と Phase 3 のゼロコピー並列 GEMM 用）。

## 他チケットへ提供するインターフェース

- 高速化された `Math.fma`・`Tensor`・`Matrix`・`Linalg`・`Gpu` 実行経路。

## 実装手順

1. **変更**: `src/llvm_math.rs`、`src/llvm_cpu.rs`、`src/target.rs`、`src/runtime/cpu.c`、`tests/math.mjs`、`tests/simd.rs`。**内容**: Phase 1（x86-64 FMA3 lowering と `@cpu ["avx2"]`・`["avx512"]` 多版連携、および `f16` SIMD 検討・実装）。**確認**: x86-64 向け IR 生成検査（ベースラインは `@tz_soft_fma`、`.cpu2` は `@llvm.fma`）、`cargo test --locked --lib target`、native/WASM `-O0`/`-O3` ビット照合、T1。
2. **変更**: `std/Tensor.tz`, `benchmarks/`。**内容**: Phase 2（SIMD 連続カーネルと形状決定チャンク並列 API）。**確認**: ビット完全一致テスト、9 サンプル実測と生成コード確認、T1。
3. **変更**: `std/Matrix.tz`, `std/Linalg.tz`, `benchmarks/`。**内容**: Phase 3（加算順序保存型キャッシュタイリング GEMM）。**確認**: `Matrix.mul` 既存テストとのビット完全一致（`1e16, 1.0, -1e16` 等の桁落ちケース含む）、`n = 64, 256, 512, 1024` での実測と生成コード確認、T1。
4. **変更**: `src/gpu.rs`, `src/llvm_gpu.rs`, `std/Gpu.tz`, `tests/gpu*`, 文書。**内容**: Phase 4（`Gpu.map2`・`Gpu.reduce`・`Gpu.matmul` のカーネル抽出・WGSL/SPIR-V 生成・能力検査）。**確認**: T2、`docs/benchmarks.md` 更新、`node scripts/check-docs.mjs`。

## テスト計画

- **ビット一致回帰**: Phase 1〜3 のすべての最適化前後で、桁落ち・`NaN`・`±0.0`・非正規化数・整数折り返しケースのビットが完全一致すること。
- **性能計測**: `benchmarks/` に matched workload を追加し、9 サンプル以上の schema-1 JSONL で結果を記録（CI に速度合否閾値は置かない）。
- **GPU**: `CpuReference` と実デバイス（Vulkan/WebGPU）の結果一致、および未対応バックエンド要求時の `Error Unavailable` 検証。

## ドキュメント

- `docs/benchmarks.md`, `docs/architecture.md`
- `_tsuzuri/language-reference/built-in-types-and-modules/matrix.md`・`simd.md`・`gpu.md`・`tensor.md`・`linalg.md`

## 受け入れ条件

- 各 Phase で決定性・安全性・ビット一致が損なわれないこと。
- 性能主張のすべてに実測データ（9 サンプル以上）と生成コード確認が伴うこと。
- T2 と `node scripts/check-docs.mjs` が通ること。

## 落とし穴

- x86-64 で `+fma` なしに `@llvm.fma.f64` を出すと、LLVM がホスト C `libm` の `fma()` 呼び出しへ下げてしまい、WASM/AArch64 とのビット一致およびフリースタンディング契約が壊れる。必ず `+fma` を確認した経路だけ `@llvm.fma` にすること。また Windows x86-64 は現状 `target.rs:370` で `cpu_levels() == 0` のため見落とさないこと。
- `Parallel.for_each_chunk` コールバックで借用入力を捕捉すると `E1013` になる。1 入力の新規確保版は `Parallel.map`、2 入力・`_into`・軸並列は [NC15](NC15-parallel-borrowed-chunk-input.md) の `Parallel.for_each_chunk_with` を使い、`Arc.new (Array.copy ...)` で逃げないこと。
- 同梱 `musl` と全入力ビット一致する SIMD 超越関数はテーブル引き・分岐のため現実的ではない。既定のビット一致経路へ別近似の SIMD を黙って混ぜないこと。
- 手書き SIMD が LLVM 自動ベクトル化より遅くなるケースがある（C11 参照）。必ず同条件ベンチマーク（`wall_time`・スレッド数固定・9 サンプル以上）で優位性を確認してから採用すること。
- 共有 CI ランナーに実行時間による合否判定を入れないこと（AGENTS.md）。

## 対象外

- 暗黙の `fast-math`・再結合・暗黙 FMA 縮約。
- 512 bit 専用ユーザー可視ベクトル型（AVX-512 は `@cpu ["avx512"]` 多版化による自動ベクトル化を優先）。
- 外部 BLAS/LAPACK リンク（[NC12](NC12-ffi-numeric-buffers-and-blas-lapack.md) が担当）。

## 決定事項

- **D1（要承認）**: Phase 4 の `Gpu` モジュールに `map2`・`reduce`・`matmul` / `matmul_relaxed` を追加する。
- **D2（要承認）**: Phase 4 での Metal ネイティブバックエンド追加は Vulkan/WebGPU 拡張と独立に評価し、端から端までの実測優位性と保守コストを確認したうえで着手を判断する。
- **D3（既定案）**: Phase 3 のタイリング GEMM は、各出力要素 $(i, j)$ に対する `k = 0 .. K-1` の加算順序を厳密に保存する設計とし、`Matrix.mul` のビット一致契約を維持したまま高速化する。
