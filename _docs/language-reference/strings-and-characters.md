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

## 補間

`$"..."` は、穴 `{式}` に値を埋め込んだ string を作るリテラルです。`u8$"..."` は同じ形で utf8string を作ります。`$` と引用符、`u8` と `$` の間に空白は置けません。通常の `"..."` では `{` と `}` はただの文字で、補間が始まるのは `$` を付けたときだけです。

```tsuzuri run=hello%20Tsuzuri%2C%204%20times%20%7Bok%7D
let name = "Tsuzuri"
let count = 3
let text = $"hello {name}, {count + 1} times {{ok}}"
assert (name.length == 7)
text
```

リテラルの中の `{{` は `{`、`}}` は `}` を表します。穴には、`Display` を持つ型の値になる式を書けます。string、数値、bool、char、unit、配列、タプル、`Display` を実装した利用者型が使えます。穴のない `$"..."` は、同じ内容の通常のリテラルと同じ値で、生成されるコードも同じです。

```tsuzuri run=%5B1%2C%202%2C%203%5D%202%20(1%2C%20%22a%22)%20true%20c%201.5%20()
let values = [1, 2, 3]
let pair = (1, "a")
$"{values} {values[1]} {pair} {true} {'c'} {1.5} {()}"
```

書式のない穴の文字列は `Display.display` の結果と同じで、数値は[最短往復表示](../library-reference/formatting-and-parsing.md#数値の表示形式)です。穴の式の最上位にある `:` から `}` までは[書式指定](../library-reference/formatting-and-parsing.md#書式指定)です。丸括弧、角括弧、波括弧の内側の `:` は書式指定ではありません。

`u8$"..."` の文字とエスケープは、`u8"..."` と同じ UTF-8 の規則に従います。utf8string の穴は直接コピーし、それ以外の穴は表示した UTF-16 を UTF-8 へ変換して入れます。`$"..."` の穴に utf8string を書くと UTF-16 へ変換されます。

```tsuzuri run=5
let word = u8"ü"
let text = u8$"é{word}{1 + 2}"
assert (text == u8"éü3")
text.length
```

### 借用と評価順序

穴の値は消費しません。

- ローカル、フィールド、参照外しは共有借用します。string のような非 Copy の値も、穴の後でそのまま使えます（最初の例の `name`）。
- `ref` の値はそのまま使い、`ref mut` の値は共有として借り直します。
- `count + 1` やリテラルのような一時値は、隠れた領域に置いて借用し、結果へ書き込んだ後に drop します。
- 利用者の `Display` インスタンスは、穴ごとに一回だけ呼ばれます。

穴は左から右へ、それぞれちょうど一回評価します。結果の確保は一回だけで、リテラルと同じ型の string を持つ穴は、確保せずに直接コピーします。

```tsuzuri run=123
def stamp :: ref mut i64 -> i64 -> i64
fn stamp log digit =
    deref log = deref log * 10 + digit
    digit

let mut log = 0
let text = $"{stamp (ref mut log) 1}{stamp (ref mut log) 2}{stamp (ref mut log) 3}"
assert (log == 123)
text
```

借用は補間式の全体の間続きます。ある穴が借りている値を別の穴で排他借用する、たとえば `$"{x}{bump (ref mut x)}"` は `E1014` です。

### 入れ子と上限

穴の式には、文字列リテラルや別の補間リテラルも書けます。

```tsuzuri run=%3C%5B42%5D%3E%20(43)%20literal
let x = 42
let inner = $"[{x}]"
$"<{inner}> {$"({x + 1})"} {"literal"}"
```

- 穴は 1 行に収めます。穴の中の改行とコメントは `E0001` で、値は先に `let` で束縛します。
- 補間リテラルの入れ子は 128 までです。
- 1 リテラルの穴は 1024 個までです。
- 結果の長さが `2^53 - 1` コード単位を超えるとトラップします。`u8$` の穴に孤立サロゲートを含む string を入れてもトラップします。

### 診断

| コード | 条件 |
| --- | --- |
| `E0001` | 穴の外の単独の `}`（`a single '}' in an interpolated string must be written '}}'`） |
| `E0001` | 閉じない穴。穴の中の改行（`an interpolation hole must stay on one line; bind the value with 'let' first`）とコメント |
| `E0001` | 通常のリテラルと同じ不正なエスケープや生の改行。`u8$` では UTF-8 のエスケープ規則に反するもの |
| `E0001` | 書式指定の誤り（[書式指定](../library-reference/formatting-and-parsing.md#書式指定)を参照） |
| `E0002` | 空の穴。空白だけも含む（`an interpolation hole needs an expression; write '{{' for a literal brace`） |
| `E0002` | 1 リテラルに 1025 個以上の穴（`an interpolated string has at most 1024 holes; split it into several strings`） |
| `E0002` | 補間の入れ子が 128 を超える（`syntax nesting exceeds 128`） |
| `E1005` | 穴の型に `Display` がない（`no instance for Display<...>`） |
| `E1012` | 穴の値がすでに move されている |
| `E1014` | ある穴が借りている値を、別の穴で排他借用している |

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
- [書式指定と Format クラス](../library-reference/formatting-and-parsing.md#書式指定)
- [文字列 API の正式な一覧](../../docs/language.md#string-と-utf8string)
