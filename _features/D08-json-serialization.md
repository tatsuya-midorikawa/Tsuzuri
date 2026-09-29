# D08: 構造化データの直列化（JSON）と Encode／Decode の導出

| 項目 | 内容 |
|---|---|
| ID | D08 |
| 優先度 | P2 |
| 規模 | L |
| 依存 | A07, D02, (C09) |
| 後続 | E13 |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善。未レビュー） |
| 改善する劣位 | C#/F# 比: 標準ライブラリの不足（[なぜ Tsuzuri か](../_docs/learn/why-tsuzuri.md#cf-に対する劣位点)）／追加: `System.Text.Json`・serde に相当する直列化がない |
| 主な影響ファイル | `std/Json.tz`（新規）, `src/derive.rs`, `src/stdlib.rs`, `src/check.rs`, `docs/language.md`, `tests/json.rs`（新規）, `tests/json.mjs`（新規） |

## 目的

レコード・union・コレクションを JSON と相互変換できるようにする。
ホスト ABI（E05）はスカラーとバッファ中心なので、Web ホストや設定ファイルとの間で複雑な値を受け渡す標準の経路として使う。

## 現状

- JSON・直列化の API はない。deriving は `Eq`／`Ord`／`Display`／`Hash`／`Default`（A07）で、`src/derive.rs` が通常の instance AST を合成する。
- 文字列は string（UTF-16）と utf8string（UTF-8）。D02 の検索・分割・符号化変換がある。
- 公開 ABI で渡せる構造はスカラーだけのレコードまで（docs/language.md 公開 ABI）。

## 仕様

### Phase 1（実装対象）

- std `Json` モジュール:
  - `union Json.Value = Null | Bool of bool | Number of Json.Number | Text of string | Array of [Json.Value] | Object of [(string * Json.Value)]`（Object はキーの出現順を保持）。
  - `Json.Number` は字句をそのまま保持し、`Json.to_i64`／`to_f64` で変換する。変換は整数範囲外で失敗、f64 は一度だけ最近接・偶数丸め。
  - `Json.parse :: ref utf8string -> Result<Json.Value, Json.Error>`: RFC 8259 に厳密。重複キー・先頭 BOM・コメント・末尾カンマは Error。
  - `Json.to_utf8string :: ref Json.Value -> utf8string`: 決定的（空白なし、キー順は保持、エスケープは最小）。
- 導出 `deriving (Encode, Decode)`:
  - `Encode<'a> { def encode :: ref 'a -> Json.Value }`、`Decode<'a> { def decode :: ref Json.Value -> Result<'a, Json.Error> }`。
  - レコードはフィールド名をキーとする Object、union は `{"Case": payload}`（payload なしは `"Case"`）、Option は `null` と値、配列・リスト・Vec は Array。
  - 浮動小数点の NaN・無限大は JSON で表せないため、encode は `Result` を返す別 API（`Json.try_encode`）でだけ扱い、通常の encode 対象から除外するか未決事項で決める。
- 資源上限: 入れ子の深さ 128、入力 64 MiB（WASM ではメモリ上限にも従う）。超過は Error。構文解析は反復で行いスタックを深く使わない。

### Phase 2（設計方針）

- ストリーミング解析、借用した部分文字列によるゼロコピー、他形式（CBOR など）へ一般化した Serializer クラス。

## 設計

- `Json.Value` は再帰 union（A04 の所有ノード、反復 drop）で表す。
- 導出は `derive.rs` の既存の AST 合成に `Encode`／`Decode` を追加し、フィールド名・case 名は宣言の綴りを使う。
- 数値の変換は D01 の解析・最短表示と同じ実装を共有する。

## 実装手順

1. **Value と parse**: RFC 8259 の厳密な解析と上限。確認: JSONTestSuite 相当の受理・拒否ケース（外部データはライセンスを確認して同梱するか、同等のケースを自作）。
2. **出力**: 決定的な直列化。確認: 往復（parse → to_utf8string → parse）で同値。
3. **導出**: Encode/Decode の合成、`E1025`（導出できない型）。確認: `tests/deriving.rs` に追加。
4. **ホスト例**: Web ホストとの JSON 受け渡し例。確認: `tests/examples.mjs`。

## テスト計画

- Rust: 導出の受理・拒否、再帰型、ジェネリックレコード。
- E2E: native/WASM × `-O0`/`-O3`、JavaScript の `JSON.parse` との照合（数値の字句保持を含む）、深い入れ子の上限、確保追跡。
- 性能: serde_json・System.Text.Json と同条件で解析・出力の時間を比較して記録する。

## ドキュメント

- `docs/language.md`（std 一覧・deriving）、`_docs/library-reference/`（新規ページ）、`_docs/language-reference/deriving.md`、GUIDE D-07。

## 受け入れ条件

- [ ] 厳密な JSON の解析・出力と、レコード・union の Encode/Decode 導出がある。
- [ ] 上限を超える入力を Error で拒否し、スタック枯渇を起こさない。

## 落とし穴

- JSON の数値を f64 に固定すると 64-bit 整数が失われる。字句保持を既定にする理由である。
- UTF-16 の string に含まれる孤立サロゲートは JSON の UTF-8 出力で表せない。`\uD800` のエスケープ出力にするか Error にするかを決める。

## 対象外

- スキーマ検証、JSON Pointer／Patch、YAML・TOML。

## 未決事項

- **NaN・無限大**: 既定案は encode を `Result` にして Error を返す。
- **孤立サロゲート**: 既定案は `\uXXXX` エスケープで出力する（ECMA-262 の `JSON.stringify` と同じ）。
