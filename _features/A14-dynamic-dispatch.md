# A14: 型クラスによる動的ディスパッチ（`dyn` 値）

| 項目 | 内容 |
|---|---|
| ID | A14 |
| 優先度 | P2 |
| 規模 | L |
| 依存 | A06, (B07) |
| 後続 | E13, G17 |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善。未レビュー） |
| 改善する劣位 | 追加（why-tsuzuri 未記載）: 実行時多相がなく、異種コレクションとコードサイズの制御ができない |
| 主な影響ファイル | `src/syntax.rs`, `src/lexer.rs`, `src/parser.rs`, `src/check.rs`, `src/polymorph.rs`, `src/ownership.rs`, `src/llvm.rs`, `docs/language.md`, `tests/polymorphism.rs` |

## 目的

異なる型の値を同じ型クラスの実装として一つのコレクションへ格納し、メソッドを実行時に選べるようにする。
C++ の仮想関数、Rust の `dyn Trait`、C#/F# のインターフェースに相当する。
単相化だけに頼らない経路を用意し、プラグイン的な拡張、コードサイズの抑制、特殊化上限への対処を可能にする。

## 現状

- 型クラスは単相化だけで解決する（`src/polymorph.rs` の `specialize`、`Specializer::request`）。辞書渡し・trait object はない。
- 特殊化数はプログラム全体で上限 1,024。超過すると `Specializer::request` が
  `E1017 "more than 1024 specializations; remove type-growing polymorphic recursion"` を返す。
- 閉じた集合は union で表せるが、別モジュールからケースを追加できない。
- 関数値は `%tz.closure = { code, environment, clone, drop }` で、複数メソッドをまとめる型安全な手段はない。

## 仕様

### Phase 1（実装対象）

- 型 `dyn C` を追加する（`C` は単一の主型変数を持つ型クラス）。`dyn Shape` は「`Shape` のインスタンスを持つ何らかの型の所有値」を表す。
- 構築は期待型が `dyn C` の位置での明示変換 `Dyn.of value` とし、暗黙の変換は行わない。値は所有権ごと移る。
- `dyn C` は自動的に `C` のインスタンスを持つ。既存の制約付きジェネリック関数を `'a = dyn C` で一つだけ具体化でき、全実装型で共有される。
- dyn 互換条件: 各メソッドで主型変数が第 1 引数の `ref 'a`／`ref mut 'a`／`'a` にだけ現れ、結果・他の引数・メソッド固有の型変数・追加制約に現れない。HKT クラスは不可。
  違反は `E1028`（GUIDE D-30 の仮割り当て）で、違反したメソッドと理由を示す。
- 性質: `dyn C` は非 Copy、drop が必要、参照を保持する値は格納できない（Phase 1）。`Send` を満たさない（Phase 2 で `dyn (C, Send)` を検討）。
- `Eq`／`Ord` のように主型変数を二回受けるクラスは dyn 互換ではない。`Display`・`Hash` は互換。

### Phase 2（設計方針）

- `Send` 付きの dyn、借用を格納する `dyn C {r}`、複数クラスの組み合わせ。

## 設計

- `Type::Dyn(class, args)` を追加し、GUIDE §6.3 の全チェックリスト（`display`、`is_copy`、`needs_drop`、`can_send`、`layout_size`、`map_type`、`unify` など）を更新する。
- 表現は `{ ptr data, ptr vtable }`。data はヒープの所有領域、vtable は `{ drop, size, align, methods... }` の定数 global。
  名前は D-03 の正規マングリングで決定的にし、同じ (具体型, クラス) の vtable は一つだけ出力する。
- `polymorph.rs` は `C<dyn C>` のインスタンスを合成し、メソッド本体を vtable 経由の間接呼び出しへ下げる。
- 所有権: `Dyn.of` は値を move する。`ref dyn C` の借用は通常どおり。

## 実装手順

1. **型と構文**: `dyn` 予約語（GUIDE §6.1 の手順）、`Type::Dyn`、表示・レイアウト。確認: `tests/frontend.rs` で旧識別子 `dyn` の拒否。
2. **dyn 互換検査**: クラス宣言時の判定と `E1028`。確認: 互換・非互換の各パターン。
3. **構築と vtable 生成**: `Dyn.of` と vtable global。確認: IR の決定性、同一 vtable の重複なし。
4. **合成インスタンス**: `C<dyn C>` と既存ジェネリック関数の一回具体化。確認: 特殊化数が実装型数に比例しないこと。
5. **drop と所有権**: 所有 data の解放、コレクション格納、match・ループ。確認: 確保追跡 `live == 0`。

## テスト計画

- Rust: 受理・拒否（`E1028`、`E1012`、`E1013`）、ジェネリック関数との組み合わせ、`[dyn Shape]`、`Vec<dyn Shape>`、`Option<dyn Shape>`。
- E2E: native/WASM × `-O0`/`-O3` で異種コレクションの集計、確保追跡、IR の決定性。
- 性能: 単相化版との呼び出しコスト比較とコードサイズ比較を `docs/benchmarks.md` に記録する（主張は実測後）。

## ドキュメント

- `docs/language.md` の多相関数と型クラス、`_docs/language-reference/generics-and-typeclasses.md`、`_docs/guides/from-fsharp.md`（インターフェースとの対応）。

## 受け入れ条件

- [ ] dyn 互換なクラスの値を異種コレクションに格納し、メソッドを呼べる。
- [ ] 非互換なクラスを `E1028` で拒否する。
- [ ] 既存の単相化経路の IR と性能が変わらない。

## 落とし穴

- vtable のメソッド順を宣言順以外で決めると IR が非決定的になる。
- `dyn C` の drop で data の型ごとの drop を呼び忘れると、所有文字列などが漏れる。
- デフォルトメソッド（A06）とスーパークラスの扱いを vtable に含め忘れやすい。

## 対象外

- downcast、実行時型情報、継承、複数ディスパッチ。

## 未決事項

- **構文**: 既定案は予約語 `dyn`（`dyn Shape`）。代替案 `Dyn<Shape>` は D-02 の「型クラス名は制約」の分類に例外が必要になる。
- **構築の表記**: 既定案は `Dyn.of value`。期待型からの暗黙変換は採用しない。
