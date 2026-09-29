# G16: デバッガー体験（型の表示・PDB）

| 項目 | 内容 |
|---|---|
| ID | G16 |
| 優先度 | P2 |
| 規模 | M |
| 依存 | G08, (G10), (G14) |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善。未レビュー） |
| 改善する劣位 | C/C++ 比: デバッガーの成熟度（[なぜ Tsuzuri か](../_docs/learn/why-tsuzuri.md#cc-に対する劣位点)）、C#/F# 比: 開発体験（[同](../_docs/learn/why-tsuzuri.md#cf-に対する劣位点)） |
| 主な影響ファイル | `src/llvm_debug.rs`, `vsc/`（debug 設定と formatter の同梱）, `scripts/`（toolchain）, `_docs/tools/debugging.md`, `vsc/README.md`, `tests/debug_info.mjs` |

## 目的

Visual Studio（C#）、rust-lldb／natvis（Rust）、GDB の pretty printer（C++）のように、文字列・配列・union・Option・Map を Tsuzuri の型として表示する。
Windows では PDB を出力し、標準のデバッガーを使えるようにする。

## 現状

- `src/llvm_debug.rs` は DWARF version 4、`DW_LANG_C` の compile unit で、関数・行・ローカル・型の情報を出力する。macOS は `.dwarf` sidecar。
- vsc/README: 「現状は DWARF に記録された低水準の型表示で、Tsuzuri 専用 pretty printer や完全な Tsuzuri 式評価はありません」。「テスト単体のソースデバッグは未対応です」。Windows ARM64 はデバッグ不可。
- PDB／CodeView の出力はない（README）。
- `-O3` では変数が最適化で消える場合がある。

## 仕様

### Phase 1: LLDB の data formatter（実装対象）

- LLDB の Python formatter を配布物（G14）と VS Code 拡張に同梱し、launch 設定で自動読み込みする。
- 対象: string（UTF-16 をテキストで表示）、utf8string、配列 `{ptr, len}` の要素、リスト（ノードを上限付きで辿る）、Vec（長さと容量）、
  union（タグから case 名と payload。再帰ノードも）、Option／Result、Map／Set（要素の列）、関数値（コードのシンボル名）、Task（未実行・所有値の概要）。
- 型の識別は DWARF の型名で行う。型名は D-03 の正規マングリング（`Main.Pair[i64,string]` など）に合わせて安定させ、formatter と同じ表を共有する。
- 大きなコレクションは表示件数の上限を設け、デバッガーを止めない。

### Phase 2: テストのデバッグ（設計方針）

- VS Code の Test Explorer から単一テストをデバッグ実行する（G06 の `--index` を使う）。

### Phase 3: Windows（設計方針）

- G10 の完了後、CodeView／PDB（`-gcodeview`、lld-link の `/debug`）と natvis を追加する。

## 設計

- formatter は Python のスクリプトとしてリポジトリで管理し、DWARF の型名の規則を `docs/architecture.md` に明記する。
- DWARF の言語コードは `DW_LANG_C` のまま（未知の言語コードはデバッガーの対応が悪い）とし、表示は formatter で行う。

## 実装手順

1. **型名の安定化**: DWARF の型名と正規マングリングの対応。確認: `tests/debug_info.mjs` に型名の検査を追加。
2. **formatter**: 型ごとの表示。確認: LLDB をバッチモード（`lldb --batch`）で動かし、ブレークポイントでの表示を期待値と比較する。
3. **VS Code 統合**: 確認: `vsc` の installed テスト（LLDB のブレークポイント・変数表示）。
4. **Phase 2／3 の設計レビュー**。

## テスト計画

- `-O0` の fixture で各型の表示を検査する（`-O3` は変数の消失があり得るため、表示できる場合だけ検査する）。
- 表示件数の上限、循環のない再帰ノードの深い走査で停止しないこと。

## ドキュメント

- `_docs/tools/debugging.md`、`vsc/README.md`、`docs/architecture.md`（型名の規則）。

## 受け入れ条件

- [ ] LLDB で主要な型が Tsuzuri の値として表示される。
- [ ] DWARF の型名の規則が文書化され、テストで固定されている。

## 落とし穴

- 型名を変えると既存の DWARF テストと cache が変わる。変更を一回にまとめる。
- 解放済み・未初期化の領域を formatter が読んでもデバッガーが落ちないよう、読み取り失敗を扱う。

## 対象外

- Tsuzuri の式評価（watch 式の完全な評価）、逆実行。

## 未決事項

- **GDB の pretty printer**: 既定案は LLDB の後に需要を見て追加する。
