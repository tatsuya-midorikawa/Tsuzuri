# C10: 共有所有と循環構造（Arena・Handle・Rc／Arc）

| 項目 | 内容 |
|---|---|
| ID | C10 |
| 優先度 | P2 |
| 規模 | XL |
| 依存 | C02, (B07), (F10) |
| 後続 | A15 Phase 2 |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善。未レビュー。Phase 2 は参照カウント導入の承認が必要） |
| 改善する劣位 | C#/F# 比: GC に任せられる共有データ・循環構造を所有権に沿って設計し直す必要がある（[なぜ Tsuzuri か](../_docs/learn/why-tsuzuri.md#cf-に対する劣位点)） |
| 主な影響ファイル | `std/Arena.tz`（新規）, `std/Vec.tz`, `src/stdlib.rs`, `src/check.rs`, `src/llvm.rs`, `docs/language.md`, `tests/arena.rs`（新規） |

## 目的

グラフ・DAG・キャッシュの共有・循環参照を、所有権モデルのまま表す標準の手段を用意する。
Rust の arena／slotmap と `Rc`／`Arc`／`Weak`、GC 言語の共有参照に相当する。

## 現状

- 所有値は常に一意所有。「GC、参照カウント、手動の解放操作は使わず、所有権と借用によりメモリを管理します」（docs/language.md 型とメモリ）。
- 再帰型は union を通る有限値の所有ノードだけ（A04、D-24）。「union を通らないレコードの循環は未対応です」。
- std に Rc/Arc/Weak・arena・handle・slotmap はない。

## 仕様

### Phase 1: Arena と Handle（言語の意味は変更しない）

- std に `Arena<'a>`（非 Copy の所有コンテナ）と `Handle<'a>`（Copy・Eq・Ord・Hash の小さな値）を追加する。
- Handle は「arena ID・slot 添字・世代」を持つ。値の生存を延ばさず、削除後・別 arena の Handle は検出できる。
- API（所有 Vec と同じ消費的更新の形）: `Arena.empty`、`insert :: Arena<'a> -> 'a -> (Arena<'a> * Handle<'a>)`、
  `get :: ref Arena<'a> -> Handle<'a> -> Option<ref 'a>`、`remove :: Arena<'a> -> Handle<'a> -> (Arena<'a> * Option<'a>)`、`length`、`iter`。
- グラフは `Arena<Node>` と `[Handle<Node>]` の辺で表す。循環しても解放は arena 全体の drop で完結する。
- 世代の overflow は該当 slot を再利用不可にする（トラップではなく容量で吸収）。

### Phase 2: 参照カウント（要承認、設計方針）

- `Rc<'a>`（単一スレッド、`Send` なし）と `Arc<'a>`（atomic、`'a` が Send なら Send）、`Weak<'a>`。
- 共有の増加は明示的な `Rc.share (ref rc)` とし、暗黙の Copy にしない（A15 の方針）。内容は共有借用だけで読み、内部可変性は F10 に委ねる。
- 循環は `Weak` を使わなければ解放されないことを文書化する。

## 設計

- Phase 1 は Vec の上の std コードと、必要なら小さな builtin（arena ID の採番）だけで実装する。コンパイラ登録の opaque record とアクセス制限は C06 の方式を再利用する。
- Phase 2 は `Type` への追加、`drop_value` の減算と解放、atomic 操作の生成を伴う。GUIDE §6.3 の全項目を確認する。

## 実装手順

1. **Arena の std 実装**: slot（Occupied/Free）と世代。確認: `tests/arena.rs` でモデル照合。
2. **Handle の検証**: 削除後・別 arena・世代 overflow。確認: `get` が None を返す全経路。
3. **グラフ例**: 循環するグラフの構築・探索・解放。確認: E2E の `live == 0`。
4. **Phase 2 の設計レビュー**: 参照カウント導入の是非を人間が判断する。

## テスト計画

- Rust: 型制約、`E1022`（内部表現へのアクセス）、借用規則。
- E2E: native/WASM × `-O0`/`-O3`、大規模グラフ、確保追跡。
- 性能: Rust の slotmap、C# の参照グラフと同条件で比較してから主張する。

## ドキュメント

- `docs/language.md` の型とメモリ、`_docs/guides/style-and-design.md`（グラフの設計指針）、`_docs/guides/from-fsharp.md`。

## 受け入れ条件

- [ ] 循環を含むグラフを Arena と Handle で安全に構築・探索・解放できる。
- [ ] Phase 2 の判断が未決事項に記録されている。

## 落とし穴

- Handle を Copy にするため、Handle が値の寿命を保つと誤解されやすい。文書で明示する。
- arena ID が同じ値になる別 arena の検出漏れ（採番を単調にする）。

## 対象外

- GC、サイクルコレクター、内部可変性（F10）。

## 未決事項

- **Phase 2 の参照カウント**: 既定案は Phase 1 の利用実績を見てから判断する。
- **Handle の型付け**: 既定案は要素型と arena ID の実行時照合。コンパイル時の arena 識別（branded type）は採用しない。
