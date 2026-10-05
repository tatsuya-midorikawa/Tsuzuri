# String、Utf8String、Char、Utf8Char

[ドキュメントのトップ](../README.md)

文字列 API は原則として入力を共有借用し、独立した所有値を返します。String の位置は UTF-16 コード単位、Utf8String の位置はバイトです。文字数・書記素数とは区別してください。

## 分割と連結

```tsuzuri run=42
let separator = ","
let text = ",one,,two,"
let parts = String.split (ref separator) (ref text)
assert (parts.length == 5)
let joined = String.join (ref separator) (ref parts)
assert (joined == text)
42
```

String と Utf8String に同名の API があります。すべての文字列と配列入力は対応する符号化の共有借用です。

| API の引数順 | 結果と動作 |
| --- | --- |
| `concat parts` | 区切りなしで連結。空配列は空文字列 |
| `join separator parts` | 区切りを挟んで連結。空配列は空文字列 |
| `split separator text` | 所有文字列の配列。非空区切りの連続・先頭・末尾一致で空要素も保持 |
| `replace needle replacement text` | 左から右の非重複一致を置換。replacement をパターンとして解釈しない |
| `repeat text count` | i64 の回数だけ繰り返す。負数は空入力でもトラップ |

空 separator の split は String ではコード単位、Utf8String ではスカラーに分けます。端に空要素を付けず、空 text なら空配列です。

空 needle の replace は要素の間と両端に挿入し、空 text にも一回挿入します。単純に「一致なし」とは扱いません。

concat / join / replace / repeat は結果長を検査し、非空結果のバッファを一回確保します。長さ超過や確保失敗はトラップです。

## 検索と比較

| API | 結果 |
| --- | --- |
| `length text` | O(1) の格納単位数 |
| `find needle text` | 最初の一致の `Maybe<i64>` |
| `rfind needle text` | 最後の一致の `Maybe<i64>` |
| `contains needle text` | 一致が存在するか |
| `starts_with prefix text`, `ends_with suffix text` | 接頭・接尾の一致 |
| `compare left right` | 辞書順の -1 / 0 / 1 |

空 needle の find は 0、rfind は text.length、contains と接頭・接尾判定は true です。検索は追加確保なしの直接走査で、最悪 O(text.length * needle.length) です。正規表現、locale 比較、Unicode 正規化、SIMD 加速はこの契約に含みません。

## 切り出しと復号

```tsuzuri run=42
let text = "\u{1F600}"
let bytes = u8"\u{1F600}"
let half = Maybe.get (String.slice (ref text) 0 1)
assert (!(String.is_well_formed ref half))
let invalid_boundary = Utf8String.slice (ref bytes) 1 1
assert (Maybe.is_none ref invalid_boundary)
42
```

| API | 契約 |
| --- | --- |
| `slice text first last` | 終端を含まない区間の所有文字列を Maybe で返す |
| `sub text first count` | 長さ指定。負数、overflow、範囲外なら None |
| `decode_at text offset` | `(文字, 次の offset)`。範囲・境界違反はトラップ |
| `chars text` | String は `[char]`、Utf8String は `[utf8char]` |
| `char_count text` | String はコード単位数、Utf8String はスカラー数 |

String の slice はサロゲートペアの途中も許し、そのまま保持します。Utf8String は両端がスカラー境界でなければ None です。長さ 0 の区間でも continuation byte の位置は不正です。

## ASCII 操作

trim / trim_start / trim_end は ASCII whitespace のコード 9 から 13、および 32 だけを除きます。to_ascii_lower / to_ascii_upper は ASCII の大小文字だけを変換します。いずれも新しい所有文字列を返し、非 ASCII を一般的な Unicode 規則で変換しません。

## 符号化変換と所有権移送

| API | 所有権・動作 |
| --- | --- |
| `String.from_utf8 text` | UTF-8 を借用し、独立した UTF-16 を生成 |
| `Utf8String.from_string text` | UTF-16 を借用。孤立サロゲートならトラップ |
| `String.is_well_formed text` | 孤立サロゲートがないかを検査 |
| `String.to_well_formed text` | 孤立サロゲートを U+FFFD に置換した所有値 |
| `clone_string text`, `Utf8String.clone text` | 借用から独立した複製 |
| `String.to_code_units text` | string を消費して `[i16u]` へ |
| `String.from_code_units units` | `[i16u]` を消費。サロゲートも保持 |
| `Utf8String.to_bytes text` | utf8string を消費して `[ubyte]` へ |
| `Utf8String.from_bytes bytes` | `[ubyte]` を消費し検証。不正なら解放して None |

移送 API は符号化変換ではありません。既存の所有ヒープバッファは追加コピーなしで移せますが、静的・スタック領域の値のヒープ移送は通常どおり必要です。from_bytes の検証は O(n) です。

## 文字 API

Char の整数変換は `to_u16` / `of_u16`、Utf8Char は `to_u32` / `of_u32` / `of_u32_unchecked` です。分類と ASCII 大小変換は両モジュールにあります。型、失敗条件、Display / Parse の単位は[文字のリファレンス](../language-reference/strings-and-characters.md)を参照してください。

## API と関連項目

- [String の宣言](api/String.md)、[Utf8String の宣言](api/Utf8String.md)
- [Char の宣言](api/Char.md)、[Utf8Char の宣言](api/Utf8Char.md)
- [表示と解析](formatting-and-parsing.md)
