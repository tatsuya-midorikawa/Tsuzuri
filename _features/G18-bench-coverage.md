# G18: ベンチマーク・カバレッジ・プロパティテスト

| 項目 | 内容 |
|---|---|
| ID | G18 |
| 優先度 | P2 |
| 規模 | M |
| 依存 | G06, (D07), (E08) |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善。未レビュー） |
| 改善する劣位 | Rust 比: 開発ツールの成熟度（[なぜ Tsuzuri か](../_docs/learn/why-tsuzuri.md#rust-に対する劣位点)）／追加: 利用者コードの性能測定・カバレッジ・プロパティテストを言語のツールで行えない |
| 主な影響ファイル | `src/test_runner.rs`, `src/main.rs`, `src/parser.rs`, `src/lexer.rs`, `src/llvm.rs`, `std/Test.tz`, `std/Bench.tz`（新規）, `docs/language.md`, `tests/test_runner.rs` |

## 目的

`cargo bench`／criterion、BenchmarkDotNet、`cargo llvm-cov`／coverlet、QuickCheck／FsCheck／proptest に相当する機能を提供する。
AGENTS.md の「実測してから主張する」方針を、利用者のコードにも適用しやすくする。

## 現状

- `test "name" = ...` 宣言と `tsuzuri test`（`--filter`、`--list`、`--index`、`--json`、`-O0`～`-O3`、`--target`、別プロセスでの隔離、30 秒のタイムアウト）（`src/test_runner.rs`、G06）。
- ベンチマークは `benchmarks/*.mjs` の外部スクリプトで、利用者のプロジェクト向けではない。
- カバレッジ・プロパティテスト・パラメーター化テストはない。

## 仕様

### Phase 1: `bench` 宣言と `tsuzuri bench`（実装対象）

- `bench "sum 1e6" = Bench.with (\_ -> new [i64](1000000, \i -> i)) (\values -> Array.sum (ref values))`。
  準備（setup）と計測対象を分け、準備の時間を計測に含めない。予約語 `bench` は GUIDE D-30 の仮割り当て。
- `Bench.consume :: 'a -> unit` は結果を最適化で消させないための builtin（値を消費し、観測可能な副作用なしに保持したことにする）。
- `tsuzuri bench [--filter] [-O0..3] [--target] [--json]`: 予熱、反復回数の自動調整、中央値と MAD、実行環境（CPU・OS・コンパイラの版・target）の記録。
- 合否の閾値は持たない（AGENTS.md）。前回の JSON との比較表示は任意。
- `bench` は通常ビルド・`test` には含めない（`test` 宣言と同じ到達性の扱い、D-22 の origin を拡張）。

### Phase 2: カバレッジ（設計方針）

- `tsuzuri test --coverage`: Tsuzuri が source span ごとのカウンターを IR に挿入し、lcov 形式で出力する。LLVM の coverage mapping 形式は使わず、決定的な独自計装にする。

### Phase 3: プロパティテスト（設計方針）

- `Test.property` と生成器 `Gen<'a>`、縮小（shrinking）、失敗時の seed の表示。乱数は E08 の決定的 PRNG を使う。

## 設計

- `bench` は `test` 宣言の構文・到達性・隔離実行の仕組みを共有し、新しい宣言種別として GUIDE §6.4 の手順に従う。
- `Bench.consume` は LLVM で最適化の障壁になる形（volatile store など）に下げ、計測対象の意味を変えない。
- 計時は native では単調時計、WASM では Node ホストの `performance.now()` を使う（ベンチ実行だけの import）。

## 実装手順

1. **構文と到達性**: `bench` 宣言。確認: `tests/test_runner.rs`、通常ビルドの IR が不変。
2. **実行器**: 反復の調整・統計・JSON。確認: 固定の遅延を持つ extern で統計の妥当性を検査する（時間の閾値ではなく反復回数と出力形式を検査）。
3. **WASM**: Node ホストでの計時。確認: WASM 実行時だけ import が増える。
4. **Phase 2／3 の設計レビュー**。

## テスト計画

- 出力形式・統計量の計算・フィルター・隔離を検査する。速度そのものは検査しない。

## ドキュメント

- `docs/language.md` の言語内テスト、`_docs/tools/testing.md`、`_docs/guides/performance.md`、GUIDE D-15・D-30。

## 受け入れ条件

- [ ] `bench` 宣言を `tsuzuri bench` で計測し、環境付きの統計を出力できる。
- [ ] 通常ビルドとテストの出力が変わらない。

## 落とし穴

- 計測対象の結果を使わないと LLVM が計算を消す。`Bench.consume` の必要性を文書化する。
- `bench` の予約語化で既存の識別子 `bench` を壊す（GUIDE §6.1 の手順）。

## 対象外

- 分散・継続的なベンチマークの保存サービス、CI での速度の合否判定。

## 未決事項

- **構文**: 既定案は予約語 `bench`。代替案は `test` 宣言の修飾子。
- **カバレッジの形式**: 既定案は独自計装と lcov 出力。
