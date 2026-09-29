# G12: LSP の拡張（補完・rename・参照・整形・inlay hints）

| 項目 | 内容 |
|---|---|
| ID | G12 |
| 優先度 | P1 |
| 規模 | L |
| 依存 | G07, G20 |
| 後続 | A15 Phase 2, G13 |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善。未レビュー） |
| 改善する劣位 | Rust 比: 開発ツールの成熟度（[なぜ Tsuzuri か](../_docs/learn/why-tsuzuri.md#rust-に対する劣位点)）、C#/F# 比: IDE 支援が発展途上（[同](../_docs/learn/why-tsuzuri.md#cf-に対する劣位点)） |
| 主な影響ファイル | `src/lsp.rs`, `src/semantic.rs`, `src/check.rs`, `src/formatter.rs`, `vsc/src/`, `tests/lsp.rs`, `tests/lsp_sessions.mjs`, `_docs/tools/editor-tools.md` |

## 目的

rust-analyzer、Roslyn、Ionide（F#）と同じく、補完・名前変更・参照検索・LSP 経由の整形・型の inlay hint・signature help・code action を提供する。

## 現状

- `src/lsp.rs` の capability は positionEncoding（UTF-8／UTF-16）、全量同期、`hoverProvider`、`definitionProvider`、`documentSymbolProvider`。
  扱うメソッドは initialize、didOpen／didChange／didClose、didChangeWatchedFiles、hover、definition、documentSymbol、cancel、shutdown。
- README: 「補完・rename・LSP経由の整形はまだ提供しません」。VS Code 拡張は snippets と `tsuzuri fmt` による整形を持つ。
- `SemanticIndex`（`src/semantic.rs`）は単相化前の型と定義位置を保持し、変更ごとに全体を再検査する。参照位置の一覧はない。
- 型エラーのある関数本体からの回復は限定的（G20）で、編集中のコードでは型情報が欠ける。

## 仕様

1. **Phase 1**: `textDocument/formatting`（G05 の formatter、AST 保存検査に成功した編集だけ）、`textDocument/references`、`documentHighlight`、`workspace/symbol`、
   `prepareRename`／`rename`（ローカル・関数・record・union case・フィールド・型別名・const。std の名前は拒否。`export` 名の変更は ABI 変更として拒否し理由を返す。衝突は拒否）。
2. **Phase 2**: `inlayHint`（`let` の推論型、暗黙の借用、A15 の複製）、`signatureHelp`（カリー化の段階を表示）。
3. **Phase 3**: `completion`（スコープ内のローカル・関数、`Module.` の後のメンバー、レコードのフィールド、union case、型に合う候補の優先）。候補の説明に doc comment（G09）を使う。
4. **Phase 4**: `codeAction`（未使用ローカルの `_` 化、不足する match 節の追加（A03 の不足例から）、未使用 private の削除）、semantic tokens。

- 全機能で UTF-8／UTF-16 の位置を既存の `PositionMapper` で扱う。応答は決定的な順序にする。

## 設計

- `SemanticIndex` に「参照位置 → 定義」の対応表を追加する。std の定義は位置を持たないため、参照・rename の対象外にする。
- rename は全ファイルの `WorkspaceEdit` を作り、適用後のソースを内部で再解析して意味が同じ（名前以外の AST が一致）ことを検査してから返す。
- 補完は G20 の部分的な型情報を使い、壊れた式の周辺でも候補を出す。

## 実装手順

1. **formatting**: 確認: `tests/lsp_sessions.mjs` の UTF-8/UTF-16 両方。
2. **参照表**: references／highlight。確認: ローカル・シャドーイング・モジュール間の参照。
3. **rename**: 確認: 衝突・std・export の拒否、適用後の再解析の一致。
4. **inlay hints／signature help**。
5. **completion**（G20 の完了後）。確認: 編集途中の不完全なコードでの候補。
6. **VS Code 拡張の更新**: 既存の snippets・整形との重複を整理する（`vsc/`）。

## テスト計画

- `tests/lsp.rs`（単体）と `tests/lsp_sessions.mjs`（実セッション、両エンコーディング）。
- 大きなプロジェクトでの応答時間を計測して記録する（閾値は CI に入れない。G17 と連携）。

## ドキュメント

- `_docs/tools/editor-tools.md`、`vsc/README.md`、`README.md` の LSP の説明。

## 受け入れ条件

- [ ] formatting・references・rename が両エンコーディングで動作し、rename が意味を変えない。
- [ ] Phase ごとの capability を正しく広告し、未実装の機能を広告しない。

## 落とし穴

- union case・フィールド・関数の名前空間を混同して rename すると別の名前を変える。
- 全量同期のままでは大きなファイルで遅くなる。増分同期は G17 の解析キャッシュと合わせて検討する。

## 対象外

- デバッグアダプター（G16）、リファクタリングの自動抽出（関数の抽出など）。

## 未決事項

- **export 名の rename**: 既定案は拒否。ABI の変更を伴う操作をエディターから黙って行わない。
