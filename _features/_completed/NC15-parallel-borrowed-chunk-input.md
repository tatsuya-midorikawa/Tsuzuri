# NC15: 借用入力をゼロコピーで共有できるチャンク並列組み込み（`Parallel.for_each_chunk_with`）

| 項目 | 内容 |
| --- | --- |
| ID | NC15 |
| 優先度 | P1（2 入力 ufunc・`_into`・並列 FFT・`Matrix.mul_parallel` / `Linalg.matmul_parallel` の `Arc` 全コピー税を解消する） |
| 規模 | M（組み込み関数 1 つの追加: 型検査・所有権検査・並列 IR 生成・`Matrix.mul_parallel` 移行） |
| 依存 | – |
| 後続 | ([NC09](NC09-fft-and-polynomial.md)), ([NC13](NC13-tensor-simd-parallel-and-gpu-kernels.md)) |
| 状態 | done |
| 起票 | 2026-11-25（[GUIDE D-65](GUIDE.md#d-65-数値計算基盤と-numpy-互換機能案-c-ハイブリッドnc01nc14)。HEAD `d4afc09b` で確認） |
| 承認 | 要承認（新規組み込み関数 `Parallel.for_each_chunk_with` の追加。決定事項 D1 が要承認、ほかは既定案） |
| 手本にする既存実装 | `src/check.rs:3726`（`Builtin::ParallelForEachChunk`）、`src/check.rs:4646`（`Builtin::RoutineScope`）、`src/ownership.rs:14495-14540`、`src/llvm_parallel.rs:215-270`、`std/Matrix.tz:240-275` |
| 主な影響ファイル | `src/check.rs`、`src/ownership.rs`、`src/pinned_rules.rs`、`src/effects.rs`、`src/pure_rules.rs`、`src/parallel_report.rs`、`src/kg/effects.rs`、`src/llvm_parallel.rs`、`std/Matrix.tz`、`tests/parallel.rs`（または新規 `tests/parallel_chunk_with.rs`）、`tests/features.mjs`、`_tsuzuri/language-reference/built-in-types-and-modules/parallel.md`、`docs/language.md` |

## 実装者向け早見

- 範囲: 同期チャンク並列に共有入力の借用をゼロコピーで渡せる組み込み関数
  ```tsuzuri
  Parallel.for_each_chunk_with :: (Sync<'s>, Send<'a>) => own i64 -> ref 's -> own (ref 's -> i64 -> ref mut ['a..] -> unit) -> ref mut ['a..] -> unit
  ```
  を追加し、既存の `std/Matrix.tz`（`mul_parallel` / `mul_fma_parallel`）で行っている `Arc.new (Array.copy ...)` による入力丸ごと複製を取り除く。
- 最初に読む: ① `src/ownership.rs:14495-14545`・`6552-6560` ② `src/check.rs:1270-1315`（`can_sync`）・`3726`・`4646` ③ `src/llvm_parallel.rs:215-270` ④ `std/Matrix.tz:240-275`。
- 最初に書くテスト: `tests/parallel_chunk_with.rs` で (1) `shared` として `ref [f64]` やタプル `ref (ref [f64] * ref [f64])`・`ref Tensor.View<f64>` をコピーなしで並列読み取りして排他チャンクへ書き込めること、(2) `shared` と出力 `ref mut ['a..]` が同じ変数を指すと `E1014` で拒否されること、(3) `shared` に未証明の借用環境を持つ関数値を入れると `E1013`、コールバックが外の借用を捕捉すると `E1013` で拒否されること。
- 触ってはいけない: 既存の `Parallel.for_each_chunk`・`Parallel.map`・`Routine.scope` の署名・動作・IR、および既定 WASM のインポートなし契約（`tsuzuri_task_parallel` で逐次実行し JSPI を要求しない）。
- 完了の見分け方: `Matrix.mul_parallel` が `Arc.new (Array.copy ...)` なしで動作し、2 入力配列加算の並列 `_into` が追加ヒープ確保 0 回（`Alloc` で確認）で native / WASM `-O0`・`-O3` を通る。

## 目的

現行の `Parallel.for_each_chunk` はコールバックが `Position::Spawned` であり借用環境を捕捉できない（`E1013: parallel callbacks and values cannot retain borrowed environments`、`src/ownership.rs:14523`）。そのため `std/Matrix.tz:250-251` の `mul_parallel` は両入力を `Arc.new (Array.copy ...)` で丸ごと複製して回避している。

$O(N^3)$ の大行列積では複製コストが隠れる場合があるが、$O(N)$ の要素ごと 2 入力 ufunc（`Tensor.add_parallel` / `*_into`）・並列バッチ FFT・中規模行列積では入力の毎回複製がメモリ帯域と確保回数のボトルネックになる。また `Routine.scope` は WASM で `--wasm-feature jspi` を要求し、`shared` に `Tensor.View` のような共有参照を含む値を渡すと `carries_loans` 判定で `E1013` になる。

`Parallel.for_each_chunk` に同期呼び出しの生存期間内だけ共有される読み取り専用引数 `ref 's` を 1 つ足した `Parallel.for_each_chunk_with` を追加することで、既存並列基盤（`tsuzuri_tz_parallel` / `tsuzuri_task_parallel`）のまま全数値カーネルの入力コピー税を一括解消する。

## 着手条件と停止条件

### 着手条件

- 決定事項 D1 の承認（`Parallel` への新規組み込み関数追加）。
- `grep -rn "ParallelForEachChunk" src/` で既存チャンク並列を扱う全箇所（7 ファイル）を控える。

### 停止条件

- **S1** `shared: ref 's` が plain な共有参照（`ref [f64]` や `Tensor.View<f64>`）を許すように緩めた結果、未証明の借用環境を持つ関数値や排他参照とのエイリアス、またはコールバックから返却・保存される借用漏出がすり抜ける穴が見つかった場合は止めて報告する。
- **S2** 既定 WASM ビルドに JSPI や追加ホストインポートが混入する場合。
- **S3** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `d4afc09b` で確認）

- `src/check.rs:1286`: `can_sync` はすでに `Self::Reference(value, false) => value.can_sync(types)` を `true` としており、`ref [f64]` や `Tensor.View<f64>` などの共有参照自体は `Sync` を満たす。
- `src/ownership.rs:14500-14525`: `Routine.scope` は `referent.carries_loans(&self.module.types())` で関数値と plain な共有参照を区別せずに一括判定しているため、`(Tensor.View, Tensor.View)` のようなタプルを渡すと `E1013`（`Routine.scope can share only values with proven owned environments`）になる。
- `src/llvm_parallel.rs:244-262`: `Parallel.for_each_chunk` はネイティブで `tsuzuri_tz_parallel`、WASM で `tsuzuri_task_parallel` を呼び、JSPI 不要・既定 WASM 逐次実行で動作する。
- `src/pure_rules.rs:193-194`: `Parallel.*` は `Category::Task` に分類され、`pure def` からは呼べない（`E1029`）。

## 変えてはいけない意味

- 既存の `Parallel.for_each_chunk`・`Parallel.map`・`Parallel.reduce`・`Routine.scope` の型・診断・IR。
- 並列コールバック内の `extern type local` 呼び出し拒否（`Position::Spawned`、`src/pinned_rules.rs:55`）。
- 並列 API の `Category::Task` 分類（`pure def` の純粋性契約を壊さない）。

## 仕様

### 1. シグネチャと動作

```tsuzuri
Parallel.for_each_chunk_with :: (Sync<'s>, Send<'a>) =>
    own i64 ->
    ref 's ->
    own (ref 's -> i64 -> ref mut ['a..] -> unit) ->
    ref mut ['a..] ->
    unit
```

- 引数順は既存 `Parallel.for_each_chunk`（対象出力スライスが最後: D-59）に揃え、`size`・`shared`・`body`・`values` とする。
- `values` を先頭から `size` 要素ずつの重ならない排他スライスに分割し、各チャンクに対し `body shared start chunk` を同期実行する（すべてのチャンク完了後に戻る）。`size <= 0` はトラップ、`values.length == 0` は `body` を呼ばず戻る。チャンク分割は `size` と `values.length` のみで決まり、スレッド数に依存しない。
- `Parallel.for_each_chunk` と同じく、直接完全適用のみ許可（部分適用・関数値化は `E1013`）。
- ネイティブは `tsuzuri_tz_parallel`、WASM は `tsuzuri_task_parallel`（既定は逐次、`--wasm-feature threads` で Worker 並列、JSPI 不要）。

### 2. 所有権・借用・同期の健全性規則

- `shared` は呼び出し全体にわたる共有借用（`Use::Read`）、`values` は排他借用（`Use::Exclusive`）として評価されるため、同じ配列を `shared` と `values` の両方に渡すと既存規則で `E1014` になる。
- `shared` の型 `'s` は `Sync<'s>` を満たす必要がある（`Rc`・`ref mut`・`extern type local`・非 Copy `dyn` は不可）。
- `shared` の中に **関数値（クロージャ）が含まれる場合**のみ、その環境が借用を持たない所有値であることを要求する（`E1013`）。一方、`ref [f64]`・`Tensor.View<f64>`・`(ref [f64] * ref [f64])` のような**関数値を含まない共有参照**は同期呼び出しの生存期間内に閉じているため安全に許可する。
- `body` コールバック自身は従来どおり借用環境を捕捉できない（`E1013`、`Position::Spawned`）。共有したい入力はすべて `shared` 引数経由で渡す。
- 効果は `Parallel.for_each_chunk` と同じく `Category::Task`（`pure def` 不可、`E1029`）。

### 3. `std/Matrix.tz` のゼロコピー化

- `Matrix.mul_parallel`・`Matrix.mul_fma_parallel` を `Parallel.for_each_chunk_with` へ移行し、`let shared = (ref left.data, ref right.data)` を直接渡すことで `Arc.new (Array.copy ...)` を除去する（出力ビット列は 1 bit も変えない）。

## 設計

- `src/llvm_parallel.rs`: `for_each_chunk` のコンテキスト構造体に `shared` ポインタ 1 フィールドを追加し、チャンクラッパーから `body(shared, start, chunk)` を呼ぶだけなので、新規ランタイム C コードは一切不要（既存の `tsuzuri_tz_parallel` / `tsuzuri_task_parallel` をそのまま流用する）。
- `src/ownership.rs`: 共有参照のみを含む値と借用クロージャ環境を含む値を区別する述語（既存の `carries_loans_beside_owned_functions` 周辺ロジックを参照）を使い、`shared` に `Tensor.View` やスライスタプルを安全に通しつつ、借用環境付き関数値は引き続き `E1013` で弾く。
- `src/effects.rs`・`src/pure_rules.rs`・`src/pinned_rules.rs`・`src/parallel_report.rs`・`src/kg/effects.rs`: `ParallelForEachChunk` の全マッチアームへ `ParallelForEachChunkWith` を並べて追加する。

## 前提とする他チケットのインターフェース

- なし（独立して着手可能）。

## 他チケットへ提供するインターフェース

- `Parallel.for_each_chunk_with`（[NC09](NC09-fft-and-polynomial.md) の並列バッチ/ND FFT および [NC13](NC13-tensor-simd-parallel-and-gpu-kernels.md) Phase 2・Phase 3 のゼロコピー並列 ufunc・`_into`・GEMM が利用）。

## 実装手順

1. **変更**: `src/check.rs`, `src/ownership.rs`, `src/pinned_rules.rs`, `src/effects.rs`, `src/pure_rules.rs`, `src/parallel_report.rs`, `src/kg/effects.rs`, `src/llvm_parallel.rs`。**内容**: `Builtin::ParallelForEachChunkWith` の型・所有権・効果・並列 IR 生成を実装する。**確認**: T0、`tests/parallel_chunk_with.rs`（正常系・エイリアス `E1014`・未証明クロージャ環境 `E1013`・コールバック借用捕捉 `E1013`・`pure def` `E1029`・`extern type local` `E1013`）。
2. **変更**: `std/Matrix.tz`。**内容**: `Matrix.mul_parallel` / `Matrix.mul_fma_parallel` の `Arc.new (Array.copy ...)` を `Parallel.for_each_chunk_with` へ置き換える。**確認**: T0、`cargo test --locked --test matrix`、`Alloc` で入力複製が消えたことを確認、`node tests/features.mjs target/debug/tsuzuri matrix`（native / WASM `-O0`・`-O3`、ビット一致、`live == 0`）。
3. **変更**: `tests/features.mjs`、`docs/language.md`、`_tsuzuri/language-reference/built-in-types-and-modules/parallel.md`、`matrix.md`。**内容**: E2E 検証とドキュメント更新（`matrix.md` の「入力を 1 回複製する」記述も実態に合わせて更新）。**確認**: T2、`node scripts/check-docs.mjs`。

## テスト計画

- **Rust テスト（`tests/parallel_chunk_with.rs`）**: 単一 `ref [f64]`、タプル `ref (ref [f64] * ref [f64])`、レコード `ref Tensor.View<f64>` の共有読み取り＋排他チャンク書き込み、同一配列エイリアス `E1014`、借用クロージャを含む `shared` の `E1013`、`!Sync` 型（`Rc`・`ref mut`）の `E1005`/`E1013`、`extern type local` の `E1013`、`pure def` からの呼び出し `E1029`、部分適用 `E1013`、`Alloc` による追加確保 0 回検証、IR 決定性。
- **E2E（`tests/features.mjs`）**: native / WASM（既定逐次および `--wasm-feature threads`）`-O0`・`-O3`、`Matrix.mul_parallel` ビット一致、`live == 0`。

## ドキュメント

- `_tsuzuri/language-reference/built-in-types-and-modules/parallel.md`・`matrix.md`
- `docs/language.md`

## 受け入れ条件

- 2 入力スライスや `Tensor.View` の組をコピーなしで並列チャンクへ共有でき、エイリアスや不正な借用環境が静的に拒否されること。
- `Matrix.mul_parallel` から入力の `Array.copy` が消え、出力ビットが既存と完全一致すること。
- 既定 WASM で JSPI や追加インポートを必要とせず、T2 と `node scripts/check-docs.mjs` が通ること。

## 落とし穴

- `shared` として渡す一時タプル `ref (ref a, ref b)` を組む際、式の生存期間と `E1013`（`borrow requires a local place`）に注意し、`let inputs = (ref a, ref b); Parallel.for_each_chunk_with ... (ref inputs) ...` の形を標準イディオムとしてテスト・文書化する。
- `src/ownership.rs:14500` の `Routine.scope` の既存挙動を不用意に緩めないよう、借用参照共有の許可範囲とテストを明確に確認する。
- `Parallel.*` は `Category::Task` のため `pure def` には置けない。並列版は必ず非 `pure` の別 API にする。

## 対象外

- `Parallel.for_each_chunk_with` 以外の新しい並列プリミティブ（`Parallel.map2` 等は std 側で `for_each_chunk_with` の薄いラッパーとして書けるため組み込みを増やさない）。
- 非同期（spawn して関数外へ逃がす）tzroutine への借用共有。

## 決定事項

- **D1（要承認）**: 組み込みモジュール `Parallel` に `Parallel.for_each_chunk_with` を追加する（承認時に `GUIDE.md` D-55 へ反映）。`Parallel.map2` 等の専用組み込みを増やさず、汎用 1 組み込みで ufunc・FFT・GEMM のゼロコピー並列をすべて賄う。
- **D2（既定案）**: ランタイム C コードは増やさず既存の `tsuzuri_tz_parallel` / `tsuzuri_task_parallel` を再利用し、既定 WASM のインポートなし・JSPI 不要を維持する。
