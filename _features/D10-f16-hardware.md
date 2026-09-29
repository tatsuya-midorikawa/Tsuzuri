# D10: f16 のハードウェア演算経路

| 項目 | 内容 |
|---|---|
| ID | D10 |
| 優先度 | P3 |
| 規模 | M |
| 依存 | D03 |
| 後続 | F08, F09 |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善。未レビュー） |
| 改善する劣位 | 追加（why-tsuzuri 未記載）: f16 が常にソフトウェア演算で、C/C++ の `_Float16`（AArch64 FP16 など）より遅い |
| 主な影響ファイル | `src/llvm.rs`, `src/llvm_math.rs`, `src/runtime/numeric.c`, `src/driver.rs`, `tests/numeric_casts.mjs`, `tests/math.mjs`, `docs/language.md`, `docs/benchmarks.md` |

## 目的

binary16 の意味（最近接・偶数丸め、NaN、符号付きゼロ、非正規化数）を保ったまま、ハードウェアの FP16 命令や binary32 演算を使って f16 を高速化する。
AGENTS.md の「意味を保つ場合は型付き LLVM 命令を優先する」方針を f16 に適用する。

## 現状

- 「f16／f128 と decimal は同梱の整数ベース演算で処理し、binary64 で代用しません」（docs/language.md 数値仕様）。
- 演算・比較・変換は `src/runtime/numeric.c` の `tz_soft_op`／`tz_soft_cmp`／`tz_soft_cast`（`numeric.ll` として埋め込み）。
- f32/f64 は LLVM の直接命令と飽和 intrinsic を使う。

## 仕様

- 観測できる結果はビット単位で現在のソフトウェア実装と同一にする（NaN は既存の canonical 規則に従う）。
- 経路 1（hardware FP16）: 対象 CPU が FP16 算術を保証する場合（例: native AArch64 で fullfp16 を含む CPU 指定）、`fadd`／`fsub`／`fmul`／`fdiv` と `llvm.sqrt` を LLVM の `half` 型で直接生成する。
- 経路 2（binary32 経由）: 加減乗除と sqrt は、binary16 を binary32 へ正確に拡張して演算し、一度だけ binary16 へ最近接・偶数丸めしても正しく丸められる
  （binary32 の精度 24 bit ≥ 2 × 11 + 2 のため二重丸めが無害）。WASM を含む FP16 命令のない環境で使う。
- 経路に含めないもの: FMA（binary32 では正確でない）、f64 から f16 への変換を binary32 経由で行うこと（二重丸め）。これらは従来のソフトウェア経路を維持する。
- 変換: f16 → f32/f64 は正確。f32 → f16 はハードウェア（AArch64 FCVT、x86 F16C の RNE 指定）またはビット演算で直接丸める。
- 経路の選択はコンパイル時の target 能力で決め、`--cpu generic` の配布物が未対応命令を無条件に使うことはない。

## 設計

- `FunctionEmitter::binary`／`cast` の f16 分岐で、target 能力に応じて IR を選ぶ。LLVM の暗黙の legalization に頼らず、経路 2 は `fpext` → 演算 → `fptrunc` を明示的に出力する。
- 演算子と組み込み関数（`Math.sqrt` など）で同じ経路を選ぶ（GUIDE §1.3）。
- target 能力の判定は driver の CPU 設定と共有し、IR にコメントで記録する。

## 実装手順

1. **参照ハーネス**: C で全 f16 ペアの加減乗除をソフトウェア参照と比較するハーネス（専用環境で全数、CI では境界値と乱択）。
2. **経路 2**: binary32 経由の生成。確認: ハーネスでビット一致、native/WASM × `-O0`/`-O3`。
3. **経路 1**: AArch64 FP16。確認: 生成アセンブリで `fadd h` 等を確認し、ハーネスでビット一致。
4. **変換**: f32 → f16 のハードウェア経路。確認: 全 f32 入力（2^32 件は専用環境）と境界値。
5. **計測**: `docs/benchmarks.md` に C の `_Float16` と比較した結果と生成コードを記録する。

## テスト計画

- 既存の `tests/math.mjs`、`tests/numeric_casts.mjs` の f16 参照が全経路で一致する。
- NaN・符号付きゼロ・非正規化数・overflow・underflow の境界値を明示的に含める。

## ドキュメント

- `docs/language.md` 数値仕様の実装説明、`docs/architecture.md` 性能設計の原則、`_docs/language-reference/numbers.md`。

## 受け入れ条件

- [ ] f16 の結果が全経路でソフトウェア実装とビット一致する。
- [ ] 生成コードで経路を確認し、速度を実測して記録している。

## 落とし穴

- LLVM の `half` legalization は target により中間精度を保持する場合がある。明示的な `fptrunc` で毎回丸める。
- x86 F16C には f64 → f16 の直接変換がない。f32 経由は二重丸めになる。

## 対象外

- f128 のハードウェア経路（主要 target にない）、decimal のハードウェア経路、bfloat16 型の追加。

## 未決事項

- **AArch64 の既定 CPU**: 既定案は target triple が FP16 を保証する場合（Apple arm64 など）だけ経路 1 を既定で使う。
