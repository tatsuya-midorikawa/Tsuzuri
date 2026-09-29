# G19: 言語版（edition）と互換性・非推奨の管理

| 項目 | 内容 |
|---|---|
| ID | G19 |
| 優先度 | P3 |
| 規模 | M |
| 依存 | E04, G09, (E10) |
| 後続 | E08（エラー種別の追加）, D09（UCD の更新） |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善。未レビュー） |
| 改善する劣位 | C/C++ 比: 長期運用の実績（[なぜ Tsuzuri か](../_docs/learn/why-tsuzuri.md#cc-に対する劣位点)）のうち、仕組みで補える互換性の保証 |
| 主な影響ファイル | `src/package.rs`, `src/lexer.rs`, `src/parser.rs`, `src/warnings.rs`, `src/docgen.rs`, `src/main.rs`, `docs/language.md`, `docs/stability.md`（新規）, `tests/modules.rs` |

## 目的

C/C++ の規格の版、Rust の edition と安定性保証、C# の言語版のように、言語と標準ライブラリの変更で既存のコードが壊れないことを保証する枠組みを作る。
実績そのものは時間でしか得られないが、互換性の方針と検査の仕組みは今から整えられる。

## 現状

- クレートの版は 0.1.0。`Tsuzuri.toml` に言語版のフィールドはない（`src/package.rs`、`[package]` は `name` と `version` だけ）。
- 非推奨の属性・警告、API の差分検査、semver の検査はない。
- 言語の変更は文書とチケットで管理してきた（例: ラムダの `\` 構文への移行、旧 `[T; N]` 構文の拒否、予約語の追加）。予約語の追加は既存の識別子を壊す（GUIDE §6.1）。
- `tsuzuri doc`（G09）は公開 API を宣言の AST から描画する。

## 仕様

- **edition**: manifest の `[package] edition = "2026"`。edition ごとに字句（予約語）・既定・非推奨の削除を切り替える。
  コンパイラは全 edition をサポートし、異なる edition の package を同じグラフで結合できる（package ごとに edition を適用）。manifest なし・指定なしは最新の edition。
  D-30 で計画する新しい予約語（`dyn`、`bench` など）は新しい edition でだけ予約する。
- **非推奨**: doc comment のタグ `@deprecated <message>`（G09 の DocComment を再利用し、新しい構文を追加しない）。非推奨の宣言の使用は警告 `W1005`（GUIDE D-30 の仮割り当て）。
  std の API は「非推奨にする → 次の edition で削除」の順に変更する。
- **API 差分**: `tsuzuri api-diff <old> <new>` は G09 の公開 API モデルを比較し、削除・型の変更・制約の追加・case の追加（網羅的 match を壊す）を breaking として報告する。
  E10 の registry への公開前検査に使う。
- **安定性方針の文書**: 安定するもの（edition ごとの構文と意味、std の公開 API、C ABI の `tz_`／`tsuzuri_` シンボルと header、WASM の import 名）と、安定しないもの（LLVM IR の形、内部シンボル、cache の形式、診断の文言）を明記する。

## 設計

- edition は `SourceFile.package` 経由でソースごとに参照し、lexer の予約語表を edition で切り替える。
- `W1005` は名前解決の結果（`Names`）と doc comment の対応から報告する。std の非推奨も同じ経路にする。
- `api-diff` は docgen の宣言モデルを JSON にして比較する。

## 実装手順

1. **manifest の edition**: 解析と既定値。確認: `tests/modules.rs`。
2. **予約語の切り替え**: 確認: 旧 edition で新しい予約語を識別子として使えること。
3. **`@deprecated` と `W1005`**: 確認: `tests/warnings.rs`、`--deny-warnings` との組み合わせ。
4. **`api-diff`**: 確認: 各 breaking 変更の検出。
5. **安定性方針の文書**。

## テスト計画

- 異なる edition の package の結合、`W1005` の抑制（std 内部・生成コード）、`api-diff` の golden 出力。

## ドキュメント

- `docs/language.md`、`docs/stability.md`（新規）、`_docs/language-reference/modules-and-packages.md`、`_docs/tools/diagnostics.md`、GUIDE D-15・D-30。

## 受け入れ条件

- [ ] package ごとの edition により、新しい予約語の追加が既存 package を壊さない。
- [ ] 非推奨の使用が警告され、公開 API の破壊的変更を検出できる。
- [ ] 安定性方針が文書化されている。

## 落とし穴

- edition で意味（数値・所有権）を変えると、package 間の結合で意味が混在する。edition で変えるのは字句・既定・警告に限る。
- 診断の文言を安定対象に含めると改善できなくなる。コードだけを安定にする。

## 対象外

- 言語の標準化団体・規格書、LTS 版の運用体制。

## 未決事項

- **edition の命名と周期**: 既定案は年号（`2026`）で、周期は定めず必要時に切る。
- **指定なしの既定**: 既定案は最新 edition。互換性を優先して最古にする案もある。
