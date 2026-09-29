# A15: 暗黙の深いコピーの可視化

| 項目 | 内容 |
|---|---|
| ID | A15 |
| 優先度 | P2 |
| 規模 | M |
| 依存 | G03, (G12) |
| 後続 | C10 |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善。未レビュー） |
| 改善する劣位 | Rust 比: Copy のコストモデルの違い（[なぜ Tsuzuri か](../_docs/learn/why-tsuzuri.md#rust-に対する劣位点)） |
| 主な影響ファイル | `src/warnings.rs`, `src/ownership.rs`, `src/llvm.rs`, `src/main.rs`, `src/lsp.rs`, `std/Array.tz`, `std/List.tz`, `docs/language.md`, `tests/warnings.rs` |

## 目的

Rust の `Copy` はビット複製だが、Tsuzuri では全要素が Copy の配列・リストや捕捉を持つ関数値も Copy になり、複製に長さ比例の確保とコピーを伴う。
言語の意味は変えずに、この複製が起きる位置と費用の種類を利用者が確認できるようにし、意図しない複製を減らせるようにする。

## 現状

- `Type::is_copy`（`src/check.rs`）: 全フィールド・全要素が Copy のレコード・タプル・配列・リストは宣言なしで Copy。関数値も Copy。
- 複製は `FunctionEmitter::clone_value`（`src/llvm.rs`）で生成する。配列は新しいバッファの確保と要素の複製、リストはノードごとの確保。
- 単一使用のローカル全体を取る場合など一部の複製は省略されるが、フィールドなどの場所からの取得は複製する。
- 既定無効の警告の仕組みがある（`src/warnings.rs` の `WarningOptions { shadowing }`、`W1004`）。
- 明示的な複製 API は `Vec.clone` のみ。配列・リストの明示複製 API はない。

## 仕様

### Phase 1（実装対象）

- 既定無効の警告 `W1006`（GUIDE D-30 の仮割り当て）: 所有権検査後の具体型で、長さに比例する暗黙の複製が生成される位置に報告する。
  対象は Copy の配列・リスト、環境を持つ関数値、それらを含むレコード・タプル。複製が省略される位置とスカラーだけの値は報告しない。
- CLI は既存の警告設定に統合する（例: `--warn implicit-copy`）。`--deny-warnings` と併用できる。
- 明示的な複製 API `Array.copy :: ref ['a] -> ['a]`、`List.copy :: ref [|'a|] -> [|'a|]`（要素 Copy）を std に追加する。明示呼び出しは警告しない。
- 警告メッセージは費用の種類（「配列全体の確保とコピー」など）と代替（`ref` で借用、`Array.copy` で明示）を示す。

### Phase 2（設計方針）

- LSP の inlay hint／hover に複製の種類を表示する（G12）。
- 共有の不変バッファ（C10 の `Rc` 相当）で O(1) にする選択肢は C10 で扱う。

## 設計

- 複製の判定は LLVM 生成直前の情報ではなく、所有権検査が確定した「値の取得が clone になる位置」から求める。`ownership.rs` にその位置を記録する出力を追加する。
- `warnings.rs` は位置・型から `W1006` を作る。標準ライブラリ・生成コード由来は既存の抑制規則に従う。
- 生成 IR は警告の有無で変わらない。

## 実装手順

1. **複製位置の収集**: 所有権検査の結果に clone 位置を記録する。確認: 既存テストの IR が不変。
2. **警告と CLI**: `W1006` と有効化オプション。確認: `tests/warnings.rs` に報告・非報告のケース。
3. **明示 API**: `Array.copy`／`List.copy`。確認: `tests/array_bulk.rs` と E2E の確保追跡。
4. **文書**: 費用の説明と使い分け。

## テスト計画

- Rust: `let b = a` 後の `a` の再使用、フィールドからの取得、関数値の捕捉、Task 捕捉、ループ内の複製を報告し、単一使用の move は報告しない。
- E2E: 明示 API の native/WASM × `-O0`/`-O3`、`live == 0`。

## ドキュメント

- `docs/language.md`「Ownership / Borrowing」、`_docs/language-reference/ownership.md`、`_docs/tools/diagnostics.md`、`_docs/guides/performance.md`。

## 受け入れ条件

- [ ] 有効化した場合だけ `W1006` が報告され、既定の出力と生成 IR は変わらない。
- [ ] 明示的な複製 API があり、警告の代替として案内される。

## 落とし穴

- ジェネリック本体では Copy かどうかが具体化まで決まらない。具体化後に報告し、同じソース位置の重複を除く。
- 単一使用の最適化で消える複製を報告すると誤検出になる。実際に `clone_value` が生成される条件と一致させる。

## 対象外

- Copy の意味の変更、宣言による Copy の opt-in、共有バッファ（C10）。

## 未決事項

- **既定の有効化**: 既定案は無効。既存コードへ警告を増やさない。
- **報告の閾値**: 既定案は型だけで判定し、リテラルの長さは見ない。
