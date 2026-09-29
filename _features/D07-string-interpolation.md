# D07: 文字列補間と書式指定

| 項目 | 内容 |
|---|---|
| ID | D07 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | D01 |
| 後続 | D08, G18 |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善。未レビュー） |
| 改善する劣位 | 追加（why-tsuzuri 未記載）: C#／F# の `$"..."`、Rust の `format!` に相当する補間・書式指定がない |
| 主な影響ファイル | `src/lexer.rs`, `src/parser.rs`, `src/syntax.rs`, `src/check.rs`, `src/llvm_display.rs`, `src/runtime/numeric.c`, `src/formatter.rs`, `docs/language.md`, `tests/strings.rs` |

## 目的

値を埋め込んだ文字列を、連結と `to_string` の組み合わせではなく一つの式で書けるようにする。
桁揃え・符号・精度・基数などの書式指定を、ホストの locale や printf に依存せず native と WASM で同一の結果にする。

## 現状

- 表示は `Display.display ref value`（借用）と `to_string value`（消費）（D01、`src/llvm_display.rs`）。浮動小数点は最短往復表示。
- 「`Parse<unit>`／`Parse<string>`、自動 deriving、char、文字列補間、任意の書式指定は未対応です」（docs/language.md 表示と解析）。
- 連結 `+` は両辺の所有文字列を消費する。多数の断片を連結すると中間文字列の確保が増える。
- string（UTF-16）と utf8string（`u8"..."`）の二つの文字列型がある。

## 仕様

### Phase 1（実装対象）

- 補間リテラル `$"x = {x}, name = {record.name}"` は `string`、`u8$"..."` は `utf8string` を作る。
- `{expr}` は値を共有借用して `Display` で表示する。式は左から右へ一度ずつ評価し、所有値を消費しない。`{{`／`}}` は波括弧そのもの。
- 書式指定 `{expr:spec}`。`spec = [[fill]align][sign][width][.precision][type]`:
  - align は `<`（左）`>`（右）`^`（中央）、fill は任意の 1 文字、width と precision は十進整数リテラル。
  - sign `+` は非負にも符号を付ける。
  - type は整数の `x`／`X`／`o`／`b`（2 の補数ではなく値の基数表記。負数は `-` を付ける）、浮動小数点の `e`（指数表記）と `f`（固定小数点）。
  - 浮動小数点の precision は、binary の正確な値を十進へ一度だけ最近接・偶数丸めした結果（二重丸めなし）。decimal は正確な十進値を丸める。
- 型に合わない指定（文字列に `x` など）はコンパイルエラー（既存の演算・型の診断コード）。書式文字列の字句エラーは `E0001`。
- 幅は Unicode スカラー数で数える（string の孤立サロゲートは 1 と数える）。
- locale 非依存。桁区切り・通貨は対象外。

### Phase 2（設計方針）

- `Format` 型クラス（利用者型の書式指定対応）、書式のコンパイル時検証済み値の関数引数化。

## 設計

- 字句解析で補間リテラルを断片と式の列に分け、構文木に専用の `ExprKind::Interpolated` を追加する（GUIDE §6.2 の全段）。
- 生成は全断片の長さを先に求めてから一度だけ確保して書き込む。中間文字列を作らない。
- 精度付きの浮動小数点は `numeric.c` の正確な十進変換（最短表示と同じ多倍長基盤）を拡張し、`tz_soft_format` と同じ実装を共有する。
- formatter（G05）は補間リテラルの字句をそのまま保持する。

## 実装手順

1. **字句と構文**: 補間リテラルの分割、入れ子の波括弧・文字列、深さ上限。確認: `tests/frontend.rs` の受理・拒否。
2. **型付けと評価順序**: Display 制約、借用、評価順序。確認: 副作用のある式の順序テスト。
3. **生成**: 一回確保の書き込み。確認: 確保回数の追跡。
4. **書式指定**: 整数・浮動小数点・幅と揃え。確認: Python の `decimal`／`fractions` による独立した参照との照合（全浮動小数点型、境界値、非正規化数）。

## テスト計画

- Rust: 構文・型エラー・所有権（借用で消費しない）。
- E2E: native/WASM × `-O0`/`-O3`、参照照合、確保追跡、IR の決定性。
- 性能: 連結版と補間版の確保回数・時間を比較して記録する。

## ドキュメント

- `docs/language.md` の表示と解析・文字列、`_docs/library-reference/formatting-and-parsing.md`、`_docs/language-reference/strings-and-characters.md`、`_docs/guides/from-fsharp.md`。

## 受け入れ条件

- [ ] 補間リテラルと書式指定が native/WASM で同一の結果を返す。
- [ ] 浮動小数点の精度指定が二重丸めなしで正確に丸められる。
- [ ] 補間を使わないプログラムの IR が変わらない。

## 落とし穴

- `$` と既存の演算子・識別子の字句衝突（`$` の現在の扱いを確認する）。
- 補間式の中の文字列リテラル・波括弧の入れ子で字句解析の状態を誤りやすい。
- 浮動小数点の precision で f64 経由の丸めをすると f128・decimal が二重丸めになる。

## 対象外

- locale 書式、printf 互換の `%d` 形式、実行時に組み立てた書式文字列。

## 未決事項

- **接頭辞**: 既定案は `$"..."`／`u8$"..."`（C#/F# と同じ）。
- **幅の単位**: 既定案は Unicode スカラー数。書記素クラスター単位は D09 の後で再検討する。
