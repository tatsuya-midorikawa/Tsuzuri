# C11: 多次元配列と数値カーネル

| 項目 | 内容 |
|---|---|
| ID | C11 |
| 優先度 | P3 |
| 規模 | L |
| 依存 | A16, F02, F04, (C08), (F08) |
| 後続 | F09 |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善。未レビュー） |
| 改善する劣位 | C/C++ 比: 最適化済みライブラリの不足（[なぜ Tsuzuri か](../_docs/learn/why-tsuzuri.md#cc-に対する劣位点)） |
| 主な影響ファイル | `std/Matrix.tz`（新規）, `src/llvm_bulk.rs`, `src/stdlib.rs`, `docs/language.md`, `benchmarks/`, `tests/matrix.rs`（新規） |

## 目的

行列・テンソル計算を連続メモリの多次元配列で書けるようにし、GEMM・転置・行単位の集計などの基本カーネルを提供する。
C++ の Eigen／BLAS、Rust の ndarray、C# の `System.Numerics.Tensors` に相当し、数値計算での採用障壁を下げる。

## 現状

- 多次元は `[[T]]`（各行が独立したバッファで長さも可変）だけ。連続した行列型・ストライド付きビューはない。
- Array の集計（`sum`、`sum_pairwise`、`sum_kahan`、`dot`、`dot_fma`、D05）、`Parallel`（F02、入力長だけで決まる固定チャンク）、128-bit `Simd`（F04）がある。
- BLAS 相当のカーネルや行列積はない。

## 仕様

### Phase 1（実装対象）

- std に非 Copy の `Matrix<'a>`（行優先・連続・`rows`／`cols` は `i64`）を追加する。
- API: `Matrix.init rows cols f`、`at`、`row (ref m) i -> ref ['a]`（行のスライス）、`transpose`、`map`、`fold`、`mul`、`add`、`to_array`。
- `Matrix.mul` の浮動小数点の意味: 各出力要素は `k = 0, 1, ...` の順に左から右へ積和する（丸めは積と和を別々に行い、暗黙 FMA なし）。
  出力要素間（列方向・行方向）の SIMD 化・並列化は各要素の演算順序を変えないため許可する。`k` 方向のブロッキングは順序を変えるので使わない。
- 別の演算順序（ブロック化・FMA）は `Matrix.mul_fma` など名前で区別した別 API にする（D-14）。
- 形状不一致は実行時トラップ。サイズ積の overflow もトラップ。

### Phase 2（設計方針）

- ストライド付きの借用ビュー、N 次元 `Tensor<'a>`、`Parallel` の行チャンク版、GPU（F09）との接続。

## 設計

- 表現は `{ rows, cols, data: [T] }` の opaque record（C06 と同じアクセス制限）。
- カーネルは typed builtin として `llvm_bulk.rs` に置き、要素型ごとに特殊化する。i-k-j などのループ順は出力要素ごとの加算順を保つ形に限る。
- 整数は折り返しで結合的なので、順序を変える最適化も許す（D-14）。

## 実装手順

1. **型と基本 API**: init・at・row・transpose。確認: `tests/matrix.rs`。
2. **mul の参照実装**: 定義どおりの逐次版。確認: f32/f64/整数で独立した参照（BigInt・Python の fractions）と一致。
3. **最適化版**: 列方向の SIMD 化・行並列化。確認: 参照実装とビット一致、生成コードの確認。
4. **性能記録**: C の素朴な実装と同順序で比較し、BLAS との差は順序が異なることを明記して記録する。

## テスト計画

- E2E: native/WASM × `-O0`/`-O3`、形状トラップ、確保追跡、NaN・符号付きゼロ・非正規化数を含む入力。
- 性能: `benchmarks/` に行列積を追加し、`docs/benchmarks.md` に条件と生成コードを記録する。

## ドキュメント

- `docs/language.md` の配列 API、`_docs/library-reference/`（新規ページ）、GUIDE D-07 の std 表。

## 受け入れ条件

- [ ] 連続メモリの行列型と基本 API があり、`mul` の演算順序が文書化されている。
- [ ] 最適化版が参照実装とビット一致する。

## 落とし穴

- BLAS との速度比較で順序の違いを無視しない。同順序の C 実装と比較する。
- `[[T]]` からの変換で行の長さが揃っていない場合の扱い（トラップ）。

## 対象外

- 自動微分、スパース行列、外部 BLAS の自動リンク（E12 の extern で利用者が接続する）。

## 未決事項

- **API の配置**: 既定案は std の `Matrix` モジュール。外部パッケージ（E10）にする案もある。
