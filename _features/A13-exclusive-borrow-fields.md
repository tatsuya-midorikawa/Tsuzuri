# A13: 排他借用フィールドと参照経由の更新

| 項目 | 内容 |
|---|---|
| ID | A13 |
| 優先度 | P2 |
| 規模 | L |
| 依存 | A12 |
| 後続 | C08, F10 |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善。未レビュー） |
| 改善する劣位 | Rust 比: 排他借用フィールドがない（[なぜ Tsuzuri か](../_docs/learn/why-tsuzuri.md#rust-に対する劣位点)） |
| 主な影響ファイル | `src/check.rs`, `src/regions.rs`, `src/ownership.rs`, `src/ownership_control.rs`, `src/polymorph.rs`, `src/llvm.rs`, `docs/language.md`, `tests/borrowed_records.rs` |

## 目的

`record Cursor {r} { buffer: ref mut {r} Vec<i64>, position: i64 }` のように、排他借用を持つ作業用 view を作れるようにする。
Rust の `struct Parser<'a> { out: &'a mut Vec<u8> }` と同じく、可変な作業領域と状態を一つの値として関数間で受け渡せるようにする。

## 現状

- レコードのフィールドに `ref mut T` を書くと `E1013 "record fields cannot store mutable references; use a shared borrow"`（`src/check.rs`）。
- union の payload は `"union payloads cannot store mutable references; ..."`、配列・リストの要素は
  `"array and list elements cannot contain mutable references; collections are deeply immutable"` で拒否する。
- 組み込みの `Capture` は排他参照を含む型を再利用可能な関数値の捕捉から除外し、`Send` は参照を保持する型を拒否する（`Type::can_capture`／`can_send`）。
- docs/language.md「名前付き Region」: 排他借用フィールドと、参照経由の borrowed aggregate 置換は後続段階。

## 仕様

### Phase 1（実装対象）

- レコードのフィールドに `ref mut {r} T` を許可する。A12 の region 規則に従い、一つの排他 region は一つのフィールドだけが持てる。
- 排他借用フィールドを持つレコードは非 Copy（`ref mut` が非 Copy のため自然に決まる）、`Capture`・`Send` を満たさない。
- `ref mut cursor.buffer` は一段の貸し直しとする。`cursor` は `let mut` 所有値か `ref mut Cursor` である必要がある。
  貸し直しの生存中は `cursor` 全体とそのフィールドへのアクセスを `E1014` で拒否する。
- `deref cursor.buffer = value` は参照先の値全体を置換し、旧値を解放する（既存の参照経由の代入と同じ）。
- レコードの move は排他 loan を移す。部分 move で排他借用フィールドを取り出した後も、残りのフィールドは使える。
- union payload・配列・リストの排他参照は引き続き拒否する。

### Phase 2（設計方針）

- 参照経由の borrowed aggregate 置換（`deref view = other_view`）を、region の包含関係を検査して許可する。

## 設計

- `check.rs` のフィールド検査から `ref mut` 拒否を外し、region 宣言との対応を必須にする。
- `ownership.rs` の loan に「排他・region」を保持し、フィールド経由の貸し直しを既存の `ref mut r` 貸し直しと同じ親子関係で表す。
- `ownership_control.rs` のループ固定点と分岐合流で、排他 loan の親子関係を保存する。
- LLVM ではフィールドはポインターのまま。既存の参照 field（A09 の ptr 表現）を再利用し、表現を追加しない。

## 実装手順

1. **宣言の許可**: region 付き `ref mut` フィールドを受理し、region なし・重複を拒否する。確認: `tests/borrowed_records.rs` の受理／`E1013`。
2. **貸し直し**: `ref mut record.field` の型付けと loan の親子関係。確認: 貸し直し中の `E1014`、終了後の再使用。
3. **置換と部分 move**: `deref record.field = value` と部分 move。確認: 旧値の解放を `tests/fixtures` の確保追跡で `live == 0`。
4. **捕捉・Task の拒否**: `Capture`／`Send` と既存の拒否経路の一致。確認: 関数値・Task・Parallel callback の拒否テスト。

## テスト計画

- Rust: 受理・拒否（`E1012`、`E1013`、`E1014`）、ジェネリックレコード、入れ子レコード、match 分解。
- E2E: native/WASM × `-O0`/`-O3` で `Vec` を排他借用した cursor 操作、確保追跡、IR の決定性。

## ドキュメント

- `docs/language.md` の Ownership／名前付き Region、`_docs/language-reference/lifetimes.md`、`README.md` の未対応一覧。

## 受け入れ条件

- [ ] 排他借用フィールドを持つレコードを安全に作成・貸し直し・置換できる。
- [ ] 別名による同時アクセス・捕捉・Task 送信を拒否する。
- [ ] 既存の共有借用フィールドの挙動と生成 IR が変わらない。

## 落とし穴

- 排他 loan を複製するような Copy 推論の漏れがないか、`is_copy` の全経路を確認する。
- パターンでフィールドを束縛した場合の貸し直しと move の区別を誤りやすい。
- `Capture` の既存の意味（排他参照の捕捉禁止）を緩めない。

## 対象外

- union payload・コレクション要素の排他参照、内部可変性（F10）、複数スレッドからの共有。

## 未決事項

- **union payload への拡張**: 既定案は対象外のまま。必要性が示された時点で別段階とする。
