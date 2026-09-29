# C08: 可変スライスと要素のその場更新

| 項目 | 内容 |
|---|---|
| ID | C08 |
| 優先度 | P2 |
| 規模 | L |
| 依存 | C03, (A13) |
| 後続 | F08, F10, C11 |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善。未レビュー。D-13 の変更を伴うため着手前に承認が必要） |
| 改善する劣位 | Rust 比: 可変スライスがない（[なぜ Tsuzuri か](../_docs/learn/why-tsuzuri.md#rust-に対する劣位点)） |
| 主な影響ファイル | `src/parser.rs`, `src/check.rs`, `src/ownership.rs`, `src/llvm.rs`, `src/llvm_frame.rs`, `std/Array.tz`, `std/Parallel.tz`, `docs/language.md`, `tests/slices.rs` |

## 目的

Rust の `&mut [T]` のように、配列の部分範囲を排他的に貸してその場で要素を更新できるようにする。
in-place sort、分割統治、ブロック単位の書き込み、並列の分割書き込み（Phase 2）を、消費と再構築を繰り返さずに書けるようにする。

## 現状

- 「`let mut`／`ref mut [T]` でできるのは配列全体の別の配列への置換だけで、既存の配列を変更しません」「`ref mut values[0]` や入れ子の要素の書き換えは拒否します」（docs/language.md 配列）。
- `ref values[start..end]` は共有スライス（C03、`{ ptr, i64 }`）。`ref mut values[start..end]` は使えない。
- `[ref mut T]` などは `E1013 "array and list elements cannot contain mutable references; collections are deeply immutable"`（`src/check.rs`）。
- C01 の `Array.set`／`update`／`swap` は所有配列を消費して更新値を返し、一意のヒープ配列はバッファを再利用する。
- GUIDE D-13: 「`&mut [T]` は従来どおり配列全体の置換用」。

## 仕様

### Phase 1（実装対象、D-13 の変更として承認が必要）

- `ref mut values[start..end]`（`&mut values[start..end]`）で排他スライスを作る。型は `ref mut [T]`、範囲検査は共有スライスと同じ。
  元は `let mut` の所有配列または `ref mut [T]`。スライスの生存中は元へのアクセスを `E1014` で拒否する。
- 要素更新は構文を増やさず std API で行う（名前は未決）:
  `Array.write :: ref mut ['a] -> i64 -> 'a -> unit`（旧要素を解放）、`Array.swap_in :: ref mut ['a] -> i64 -> i64 -> unit`、
  `Array.split_at_mut :: ref mut ['a] -> i64 -> (ref mut ['a] * ref mut ['a])`、`Array.sort_in_place :: Ord<'a> => ref mut ['a] -> unit`。
- 既存の `deref r = other`（全体置換）はスライスでない `ref mut [T]` だけに許し、スライスへの全体置換は長さを変え得るため拒否する。
- Copy 配列の値の意味は保つ: `let b = a` の後で `a` を更新しても `b` は独立した複製のまま。
- 評価順序: 引数を左から右に評価してから境界検査し、成功後だけ書き込む（C01 と同じ）。

### Phase 2（設計方針）

- `Parallel.for_each_chunk_mut`: 入力長だけで決まる固定チャンク（D-14）の互いに素な `ref mut` を worker へ渡す。
- SIMD の store（F08）と固定長配列（A16）への同じ API。

## 設計

- 排他スライスは C03 の記述子を再利用し、loan は元の場所の排他 loan の子とする。`split_at_mut` の二つの結果は同じ親の独立した子 loan で、重なりがないことは API が保証する。
- 書き込みは「境界検査 → 旧要素の drop → store」。スタック上の配列はフレーム内のポインターのまま書き込み、ヒープ移送を要求しない。
- `docs/language.md` の「コレクションは深く不変」の記述を「共有されている間は不変、排他借用中だけ要素を置換できる」へ改める。

## 実装手順

1. **承認**: D-13 の変更と API 名を人間が承認する。
2. **排他スライスの型付け**: 構文・範囲検査・loan。確認: `tests/slices.rs` の受理・拒否（`E1014`）。
3. **書き込み API**: `write`／`swap_in`。確認: 旧要素の解放、Copy 配列のスナップショット、トラップ位置。
4. **分割と in-place sort**: `split_at_mut`、安定 merge sort の in-place 版（作業領域の確保量を文書化）。確認: 既存 `Array.sort` との結果一致。
5. **Phase 2 の設計**: 並列の分割書き込み。

## テスト計画

- Rust: 別名の拒否、ループ内の再借用、関数値による捕捉の拒否（`Capture`）、Task 送信の拒否。
- E2E: native/WASM × `-O0`/`-O3`、境界トラップ、非 Copy 要素の解放、`live == 0`。
- 性能: in-place 版と消費的更新版、C の配列操作を比較し、生成コードを確認して記録する。

## ドキュメント

- GUIDE D-13、`docs/language.md` の配列とスライス、`_docs/library-reference/arrays-and-lists.md`、`_docs/language-reference/ownership.md`。

## 受け入れ条件

- [ ] 排他スライスで要素をその場で更新でき、別名・競合を拒否する。
- [ ] Copy の値の意味と既存の消費的更新の結果が変わらない。
- [ ] D-13 の変更が台帳に記録されている。

## 落とし穴

- `Array.set` の一意バッファ再利用と、排他借用中の書き込みを同じバッファに対して混在させない。
- スタック上のリテラル配列の要素を書き換えた後の relocate（`llvm_frame.rs`）で古い内容をコピーしない。
- 文字列のコード単位・リストのノードは対象外のまま。

## 対象外

- `values[i] = x` の代入構文、リスト・文字列の要素更新、共有スライスからの可変化。

## 未決事項

- **D-13 の変更**: 既定案は上記仕様。承認まで Phase 1 に着手しない。
- **API 名**: 既定案は `Array.write`／`swap_in`／`split_at_mut`／`sort_in_place`。
