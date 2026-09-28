# 文字列、文字、Unicode

[ドキュメントのトップ](../README.md)

Tsuzuri は UTF-16 と UTF-8 を別の型として扱います。長さや索引の単位を混同しないことが、ホスト連携やテキスト処理の前提です。

## 四つの型

| 型 | リテラル | 一つの値・要素の意味 |
| --- | --- | --- |
| `string` | `"text"` | UTF-16 コード単位列 |
| `utf8string` | `u8"text"` | 妥当な UTF-8 バイト列 |
| `char` | `'A'` | 一つの UTF-16 コード単位 |
| `utf8char` | `u8'A'` | 一つの Unicode スカラー |

string は ECMA-262 の String 値モデルに従い、孤立サロゲートも保持します。JavaScript の String オブジェクトや暗黙の型強制を提供するものではありません。utf8string は不正な UTF-8 を保持しません。

## 長さと索引の単位

```tsuzuri run=42
let text = "\u{1F600}"
let bytes = u8"\u{1F600}"
assert (text.length == 2)
assert (bytes.length == 4)
assert ((String.chars ref text).length == 2)
assert ((Utf8String.chars ref bytes).length == 1)
42
```

string の length、索引、通常の for は UTF-16 コード単位です。索引や for の要素型は char ではなく `i16u` です。utf8string ではバイト単位で、要素型は `ubyte` です。

`String.chars` は char 配列、`Utf8String.chars` は utf8char 配列を返します。後者はスカラー単位ですが、結合文字を含む書記素クラスタの単位ではありません。

length は O(1) で所有権を消費せず、`String.length ref text` も同じです。索引は i64、負数・範囲外はトラップです。

## エスケープ

文字列は `\"`, `\\`, `\n`, `\r`, `\t`, `\0`, `\u{...}` を受理します。string はさらに `\uXXXX` を受理し、`\uD800` のような単独サロゲートも指定できます。

UTF-8 型の Unicode エスケープは波括弧付きの妥当なスカラーだけです。サロゲートと U+10FFFF を超える値は拒否します。文字リテラルには `\'` も使えます。

char に補助平面の文字一つを格納することはできません。utf8char なら格納できます。空・複数文字の文字リテラルは `E0001` です。閉じる引用符のない `'value` は型変数として扱います。

## 文字の変換

```tsuzuri run=65
let character = Char.of_u16 65i16u
assert (character == 'A')
Char.to_u16 character as i64
```

char と utf8char は Copy / Eq / Ord ですが Numeric ではありません。算術や整数との as ではなく、専用 API を使います。

| API | 契約 |
| --- | --- |
| `Char.to_u16`, `Char.of_u16` | 全 65,536 コード単位を往復 |
| `Utf8Char.to_u32` | スカラー値を i32u へ |
| `Utf8Char.of_u32` | 不正なスカラーなら None |
| `Utf8Char.of_u32_unchecked` | 不正なスカラーならトラップ。検査を省く意味ではない |
| `is_ascii_digit`, `is_ascii_alphabetic` | ASCII の分類 |
| `is_ascii_lower`, `is_ascii_upper` | ASCII の大文字・小文字の分類 |
| `to_ascii_lower`, `to_ascii_upper` | ASCII だけを変換。非 ASCII は維持 |

分類と大小変換は両文字モジュールにあります。文字型同士の暗黙変換はありません。Display は UTF-16 string を返し、Parse は char ならちょうど一コード単位、utf8char ならちょうど一スカラーの入力に成功します。

文字型もコンソールの最終結果として返せます。次は `A` と改行を表示します。

```tsuzuri run=A
Char.to_ascii_upper 'a'
```

## 所有権と比較

文字列は非 Copy です。`+` は左右を消費して連結し、比較、length、索引、反復は読み取り借用します。必要なら `clone_string` / `Utf8String.clone` で独立した所有値を作ります。

比較は locale に依存せず、string は符号なし UTF-16 コード単位順、utf8string は符号なし UTF-8 バイト順です。正規化や大文字小文字の無視を暗黙には行いません。見た目の同じテキストでもコード列が異なれば等しくありません。

NUL 終端はなく、途中の NUL も通常のデータです。ホストへ渡すときも長さを必ず扱います。

## 符号化変換とサロゲート

```tsuzuri run=3
let invalid = "\uD800"
assert (!(String.is_well_formed ref invalid))
let repaired = String.to_well_formed ref invalid
let encoded = Utf8String.from_string ref repaired
encoded.length
```

`String.from_utf8` は UTF-8 を UTF-16 へ、`Utf8String.from_string` は逆へ変換します。どちらも借用した入力とは独立した所有値を返します。後者は孤立サロゲートでトラップします。

置換したい場合だけ `String.to_well_formed` を使い、孤立サロゲートを U+FFFD にします。コンソールの UTF-8 出力も暗黙置換せず、不正な UTF-16 ならトラップします。

string の理論上限は `2^53 - 1` コード単位ですが、実際にはアドレス空間・ヒープ上限・確保可能量に制限されます。検索や正規化などの機能範囲は標準ライブラリの各 API 契約に従います。

## 関連項目

- [基本型](types.md)
- [所有権](ownership.md)
- [文字列 API の正式な一覧](../../docs/language.md#string-と-utf8string)
