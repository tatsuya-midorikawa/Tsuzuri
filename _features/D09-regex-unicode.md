# D09: 正規表現と Unicode テキスト処理

| 項目 | 内容 |
|---|---|
| ID | D09 |
| 優先度 | P2 |
| 規模 | L |
| 依存 | D02, A08 |
| 後続 | D07 Phase 2 |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善。未レビュー） |
| 改善する劣位 | C#/F# 比: 標準ライブラリの不足（[なぜ Tsuzuri か](../_docs/learn/why-tsuzuri.md#cf-に対する劣位点)）／追加: 正規表現・Unicode 正規化・書記素処理がない |
| 主な影響ファイル | `std/Regex.tz`（新規）, `std/Unicode.tz`（新規）, `src/runtime/`（新規テーブル生成）, `src/stdlib.rs`, `docs/language.md`, `tests/regex.mjs`（新規） |

## 目的

.NET の `Regex`、Rust の `regex`、C++ の `<regex>` に相当する検索・置換・分割と、Unicode 正規化・書記素クラスター・大文字小文字変換を標準で提供する。
入力の検証・抽出・整形という Tsuzuri の主要用途で、ホスト実装に頼らずに済むようにする。

## 現状

- `String`／`Utf8String`（`std/String.tz`、`std/Utf8String.tz`）は検索・分割・trim・ASCII の大文字小文字変換を提供する（D02）。
- 正規表現はない。`_docs/feature-status.md` の未対応範囲に「一般的な Unicode 正規化」がある。
- char は UTF-16 コード単位、utf8char は Unicode スカラー（A08、D-12）。

## 仕様

### Phase 1: 正規表現（実装対象）

- std `Regex`: `Regex.compile :: ref string -> Result<Regex, Regex.Error>`、`is_match`、`find`（`Option<(i64 * i64)>` の範囲）、`find_all`（Seq）、`captures`、`replace_all`、`split`。
  string は UTF-16 コード単位の添字、utf8string は byte 添字を返す（既存の索引規則に合わせる）。
- **線形時間を保証する**エンジン（Thompson NFA／Pike VM と遅延 DFA）。後方参照・先読み・後読みは提供しない。ReDoS を構造的に防ぐ。
- 上限: パターン長 64 KiB、NFA 状態数、DFA キャッシュの大きさ。超過は `Regex.Error`（実行中の上限超過は NFA へフォールバック）。
- `.` は Unicode スカラー 1 個（string の孤立サロゲートも 1 個）。`\d`／`\w`／`\s` の既定は Unicode の定義とし、`(?-u)` で ASCII に限定できる。
- Unicode Character Database の版を固定して文書化する。

### Phase 2: Unicode アルゴリズム（設計方針）

- `Unicode.normalize` の NFC/NFD/NFKC/NFKD、UAX #29 の書記素クラスター・単語境界の反復、locale 非依存の完全な大文字小文字変換。
- テーブルは使用時だけ連結する（到達可能性による除去）。サイズを測定して記録する。

## 設計

- エンジンは std の Tsuzuri コードと、必要な部分だけ C ランタイム（`src/runtime/`）で実装する。どちらにするかは生成コードと速度を比較して決める。
- Unicode テーブルは固定版の UCD から生成スクリプト（`src/runtime/generate.py` と同じ方式）で生成し、生成物を追跡する（GUIDE §2.2、`.gitignore` の例外）。
- `Regex` は非 Copy の不透明型。コンパイル済みプログラムは共有借用で複数回使う。

## 実装手順

1. **構文解析と NFA**: パターンの解析・上限・エラー位置。確認: 受理・拒否のテスト。
2. **Pike VM**: 捕捉付きの線形時間照合。確認: 独立した参照（Rust の regex crate を使う外部スクリプト、または RE2 の検証データ）と照合。
3. **遅延 DFA**: 捕捉なしの高速経路。確認: 結果が Pike VM と一致。
4. **Unicode クラス**: 固定版テーブル。確認: UCD の全スカラーで分類を照合。
5. **Phase 2 の設計レビュー**: 正規化・書記素のテーブル容量と API。

## テスト計画

- E2E: native/WASM × `-O0`/`-O3`、UTF-16 と UTF-8 の添字、孤立サロゲート、病的パターン（`(a*)*b` など）の線形時間の確認（時間の閾値は CI に入れず、ステップ数で検査する）。
- 性能: Rust regex・.NET Regex と同条件で比較して記録する。

## ドキュメント

- `docs/language.md` の文字列、`_docs/library-reference/text.md`、GUIDE D-07 の std 表。

## 受け入れ条件

- [ ] 線形時間の正規表現で検索・置換・分割ができ、上限で安全に失敗する。
- [ ] UCD の版と Unicode クラスの定義が文書化されている。

## 落とし穴

- バックトラッキング実装は ReDoS を招くため採用しない。
- string と utf8string で添字の単位が異なる。結果の範囲を混同しない。
- テーブルを常に連結するとバイナリサイズが増える。

## 対象外

- 後方参照・lookaround、locale 依存の照合、正規表現リテラルの専用構文。

## 未決事項

- **実装言語**: 既定案は std の Tsuzuri 実装を先に作り、速度が不足する部分だけ C ランタイムへ移す。
- **UCD の版**: 既定案は実装時点の最新安定版を固定し、更新は edition（G19）に合わせる。
