# D14 付録 3: 文書の草稿と補助の手順（G・H）

本文は [D14-text-class.md](D14-text-class.md)、付録 1 は [D14-appendix.md](D14-appendix.md)、付録 2 は [D14-appendix-tests.md](D14-appendix-tests.md)。
文書は日本語（`_features/GUIDE.md` §8）。`node scripts/check-docs.mjs` を通した全文を置く（`TSUZURI_BIN` に試作を指して `Docs: 2 pages, 17 links, 10 checked examples, 20 native runs (O0/O3), 0 test projects.`）。実装者は、実際に動くコンパイラで同じコマンドを走らせ、各例の `run=` の値と「実行結果」を確かめる。

## G. 文書

### G.1 `_tsuzuri/language-reference/built-in-types-and-modules/text.md`（新規。Phase 1）

そのまま置く。`run=` の値は、実行結果を URL エンコードした文字列（既存のページと同じ。`check-docs` が `-O0` と `-O3` で `run` して一致を見る）。

````markdown
# Text

`Text` は、`string`（UTF-16）と `utf8string`（UTF-8）が共通に持つ操作を 1 つにまとめた、標準ライブラリの型クラスです。`Text.find`・`Text.split`・`Text.trim` のように、型を書き分けずに同じ名前で呼べます。メソッドの意味は `String` と `Utf8String` の同名の関数と同じで、呼び出しは instance が持つその関数への呼び出しにそのまま置き換わります。`String` と `Utf8String` は残ります。

型そのものの契約は [文字列](../literals-and-strings/strings.md)、関数ごとの契約は [String](string.md) と [Utf8String](utf8string.md) を参照してください。型クラスの一般的な説明は [型クラス](../types-and-type-inference/type-classes.md) です。

## この記事のポイント

- `Text.f` は、引数が `string` なら `String.f`、`utf8string` なら `Utf8String.f` と同じ結果を返します。引数の並び（パターンや区切りが先）も同じです。
- 長さ・位置・範囲の単位は型で決まります。`string` は UTF-16 のコード単位、`utf8string` はバイトです。違いは [単位は型で決まる](#単位は型で決まる) の表にまとめました。
- ジェネリックな関数には `Text<'s> =>` と書きます。`Text` は `Ord` を含むので、関数の中で `<` と `==` も使えます。
- 実行時にメソッドを選ぶ処理はありません。呼び出しは instance のメソッドへの直接の呼び出しで、最適化すると呼び出し元へ組み込まれます。
- `char_count`・`chars`・`decode_at` と、型の変換（`String.from_utf8` と `Utf8String.from_string`）は入っていません。型によって意味や戻り値の型が違うからです。

## 同じ呼び出しが両方の型で動く

```tsuzuri run=7%207%203%203%20true%20TSUZURI
let title = "Tsuzuri"
let bytes = u8"Tsuzuri"
let at = Maybe.default_value (-1) (Text.find "zu" title)
let at8 = Maybe.default_value (-1) (Text.find u8"zu" bytes)
let starts = Text.starts_with "Tsu" title
let upper = Text.to_ascii_upper bytes
$"{Text.length title} {Text.length bytes} {at} {at8} {starts} {upper}"
```

実行結果:

```text
7 7 3 3 true TSUZURI
```

`Text.find "zu" title` は `String.find "zu" title`、`Text.find u8"zu" bytes` は `Utf8String.find u8"zu" bytes` です。共有借用の引数には、`ref` を付けても付けなくても、リテラルや関数呼び出しの結果をそのまま渡せます。引数がそろった呼び出しなら、`String.find` と同じ規則で一時値が借用されます。

## 単位は型で決まる

`Text` のメソッドは、型ごとの関数をそのまま呼びます。型の違いは `Text` が隠さず、そのまま残ります。

| 操作 | `string` | `utf8string` |
| --- | --- | --- |
| `length`、`find`、`rfind` の位置 | UTF-16 のコード単位 | バイト |
| `slice`、`sub` | サロゲートペアの途中でも切れる | 範囲がスカラーの境界でなければ、空の範囲でも `None` |
| `split`、`replace` の空のパターン | コード単位のあいだ | スカラーのあいだ |
| `compare`、`<` の順序 | UTF-16 のコード単位の順 | バイトの順（スカラーの順と同じ） |
| `join`、`concat`、`repeat` の結果の上限 | 2^53 − 1 コード単位 | `i64` の範囲 |

```tsuzuri run=3%207%202%20-1%203
def cut_length :: Text<'s> => ref 's -> i64 -> i64 -> i64
fn cut_length text first last =
    match Text.slice text first last with
    | Some part -> Text.length (ref part)
    | None -> -1

let word = "あ😀"
let bytes = u8"あ😀"
$"{Text.length word} {Text.length bytes} {cut_length word 1 3} {cut_length bytes 1 3} {cut_length bytes 0 3}"
```

実行結果:

```text
3 7 2 -1 3
```

`あ😀` は、`string` では 3 コード単位（`あ` が 1、`😀` が 2）、`utf8string` では 7 バイト（`あ` が 3、`😀` が 4）です。コード単位 1 から 3 は、`string` ではサロゲートペア `😀` の 2 単位ですが、`utf8string` のバイト 1 から 3 は `あ` の途中なので `None` です。

```tsuzuri run=1%20-1%202%201%205%206
let high = "\uFF5E"
let low = "😀"
let order = Text.compare high low
let high8 = u8"\u{FF5E}"
let low8 = u8"😀"
let order8 = Text.compare high8 low8
let pieces = (Text.split "" low).length
let pieces8 = (Text.split u8"" low8).length
let filled = (Text.replace "" "-" low).length
let filled8 = (Text.replace u8"" u8"-" low8).length
$"{order} {order8} {pieces} {pieces8} {filled} {filled8}"
```

実行結果:

```text
1 -1 2 1 5 6
```

U+FF5E は U+1F600 より小さいスカラーです。`utf8string` のバイトの順はスカラーの順と同じなので `-1` ですが、`string` ではサロゲートの `0xD83D` より `0xFF5E` が大きいので `1` です。空のパターンの `split` と `replace` は、`string` では 2 つのコード単位のあいだ、`utf8string` では 1 つのスカラーの前後です。

## ジェネリックな関数

```tsuzuri run=4%204%200%20true%20false
def count_words :: Text<'s> => ref 's -> ref 's -> i64
fn count_words separator text =
    let trimmed = Text.trim text
    if Text.is_empty (ref trimmed) then 0 else (Text.split separator (ref trimmed)).length

def earlier :: Text<'s> => ref 's -> ref 's -> bool
fn earlier left right = deref left < deref right

let a = count_words " " "  to be or not  "
let b = count_words u8" " u8"  to be or not  "
let c = count_words " " ""
let d = earlier "apple" "banana"
let e = earlier u8"banana" u8"apple"
$"{a} {b} {c} {d} {e}"
```

実行結果:

```text
4 4 0 true false
```

`Text<'s> =>` の制約を持つ関数は、使う型ごとに 1 つずつ特殊化されます。実行時にメソッドを選ぶことはありません。ジェネリックな関数の中の `Text.trim text` は、`'s` が決まった時点で `String.trim` か `Utf8String.trim` への直接の呼び出しです。`Text` は `Ord` を含むので、`deref left < deref right` のような比較と `==` が `'s` に使えます。

型クラス名を修飾して `Text.Text<'s>`、メソッドを `Text.Text.length` と書くこともできます。

## メソッドの一覧

引数の型は、`'s` が `string` か `utf8string` のどちらかです。パターン・区切り・接頭辞・接尾辞は第 1 引数です。

| メソッド | シグネチャ | 呼び出す関数（`string`／`utf8string`） |
| --- | --- | --- |
| `length` | `ref 's -> i64` | `String.length`／`Utf8String.length` |
| `is_empty` | `ref 's -> bool` | 既定の実装。`length` が 0 のとき真 |
| `compare` | `ref 's -> ref 's -> i64` | `String.compare`／`Utf8String.compare` |
| `clone` | `ref 's -> 's` | `clone_string`／`Utf8String.clone` |
| `find` | `ref 's -> ref 's -> Maybe<i64>` | `String.find`／`Utf8String.find` |
| `rfind` | `ref 's -> ref 's -> Maybe<i64>` | `String.rfind`／`Utf8String.rfind` |
| `contains` | `ref 's -> ref 's -> bool` | `String.contains`／`Utf8String.contains` |
| `starts_with` | `ref 's -> ref 's -> bool` | `String.starts_with`／`Utf8String.starts_with` |
| `ends_with` | `ref 's -> ref 's -> bool` | `String.ends_with`／`Utf8String.ends_with` |
| `slice` | `ref 's -> i64 -> i64 -> Maybe<'s>` | `String.slice`／`Utf8String.slice` |
| `sub` | `ref 's -> i64 -> i64 -> Maybe<'s>` | `String.sub`／`Utf8String.sub` |
| `join` | `ref 's -> ref ['s] -> 's` | `String.join`／`Utf8String.join` |
| `concat` | `ref ['s] -> 's` | `String.concat`／`Utf8String.concat` |
| `split` | `ref 's -> ref 's -> ['s]` | `String.split`／`Utf8String.split` |
| `trim` | `ref 's -> 's` | `String.trim`／`Utf8String.trim` |
| `trim_start` | `ref 's -> 's` | `String.trim_start`／`Utf8String.trim_start` |
| `trim_end` | `ref 's -> 's` | `String.trim_end`／`Utf8String.trim_end` |
| `to_ascii_lower` | `ref 's -> 's` | `String.to_ascii_lower`／`Utf8String.to_ascii_lower` |
| `to_ascii_upper` | `ref 's -> 's` | `String.to_ascii_upper`／`Utf8String.to_ascii_upper` |
| `repeat` | `ref 's -> i64 -> 's` | `String.repeat`／`Utf8String.repeat` |
| `replace` | `ref 's -> ref 's -> ref 's -> 's` | `String.replace`／`Utf8String.replace` |

メソッドの失敗と計算量は、呼び出す関数と同じです。`is_empty` だけは `String` と `Utf8String` にない新しい操作で、`length` が 0 かどうかを返します。`compare`・`clone` は、組み込み関数 `String.compare`・`clone_string`（`Utf8String.compare`・`Utf8String.clone`）の名前をそろえた入口です。

## 入っていないもの

| 関数 | 理由 |
| --- | --- |
| `char_count` | 同じ名前で意味が違います。`String.char_count` は `length` と同じ値（O(1)）、`Utf8String.char_count` はスカラー数（O(n)）です |
| `chars`、`decode_at` | 要素の型が違います（`char` と `utf8char`）。`decode_at` は進む幅も違います |
| `to_code_units`・`to_bytes`、`from_code_units`・`from_bytes` | 要素の型が違い、`from_bytes` は検査して `Maybe` を返します |
| `String.from_utf8`、`Utf8String.from_string` | 2 つの型のあいだの変換は、暗黙にせず、明示した呼び出しだけにします |
| `String.is_well_formed`、`String.to_well_formed` | `string` だけの操作です（孤立サロゲートがあるのは `string` だけ） |

`Char` と `Utf8Char` の 6 つずつの関数にも、型クラスはありません。`Text` から文字を取り出す手段がないので、文字の型クラスだけでは使い道がないからです。

## 費用

- `Text.f` は、`string` や `utf8string` に使うと、instance のメソッド（`String.f` を呼ぶだけの関数）への直接の呼び出しになります。型クラスの辞書も vtable も間接呼び出しも作りません。
- 最適化（`-O1` 以上）では、メソッドが呼び出し元に組み込まれます。小さいプログラムでは、直接呼んだ場合とオブジェクトコードが同じです。呼び出しの多い大きなプログラムでは、LLVM の判断で呼び出し元の命令数が数 % 違うことがあります。`-O0` では、呼び出しが 1 段増えます。
- 特殊化の予算（65,536）は、使ったメソッドと型の組ごとに 1 つ使います。`String.length` を直接呼ぶより、`Text.length` を `string` に使うほうが 1 つ多い計算です。

## 注意点

> [!WARNING]
> ユーザーのモジュールの名前に `Text` は使えません。`Text.tz`・`Text.tt`・`Text.tc` と `namespace Text` は `E1011` です。同じ名前の record・union・型エイリアス・型クラスは宣言できます。ユーザーが `class Text` か `union Text` を宣言すると、無修飾の `Text.length` はユーザーの宣言を指すので、標準の `Text` は `Text.Text` と書きます（メソッドは `Text.Text.length`）。

- 引数を全部そろえない部分適用で一時値を借用すると、直接の関数と同じく `E1013` です。`let f = Text.find "a"` は、先に `let` で束縛してください。
- ユーザーの型にも instance を書けます。`Ord` の instance（`deriving (Eq, Ord)` で作れます）と、`is_empty` 以外のすべてのメソッドを実装します。
- `Text` に `dyn` は使えません（`E1028`）。ジェネリックな関数を使ってください。
- `String` と `Utf8String` は非推奨ではありません。型が決まっている場所では、今までどおり `String.find` と書けます。

## まとめ

- 同じ操作は `Text.f`、型が決まっていて型に固有の操作は `String.f`／`Utf8String.f` と使い分けます。
- 単位（コード単位とバイト）と、境界・空のパターン・順序の違いは、型にそのまま残ります。
- ジェネリックな関数は `Text<'s> =>` で書き、`Ord` の比較も使えます。
- 呼び出しは instance のメソッドへの直接の呼び出しで、最適化すると組み込まれます。

## 関連項目

- [String](string.md)
- [Utf8String](utf8string.md)
- [Regex](regex.md) — 正規表現（`string` と `utf8string`）
- [文字列](../literals-and-strings/strings.md)
- [型クラス](../types-and-type-inference/type-classes.md)
- [言語リファレンスの目次](../index.md)
````

Phase 2 のあと、`text.md` の「入っていないもの」の表の直前の文（または `## 関連項目` の `[Regex]` の行）は変えない。`regex.md` が `RegexText` を説明する（G.2）。

### G.2 `_tsuzuri/language-reference/built-in-types-and-modules/regex.md`（Phase 2。`diff -u` の結果）

`### 型を選ばずに使う` を、既存の「`bytes=(4, 5) units=(2, 3)` を出す例」の `text` のブロックの次、`## 構文` の前に足す。API の表の `split` / `split_utf8` の行の次に 7 行足す。`regex.md` の `## 関連項目` に `[Text]` の行は足さない（`Regex.*_text` の説明はこの節に書く）。

````diff
@@ -98,8 +98,39 @@
 
 ```text
 bytes=(4, 5) units=(2, 3)
+```
+
+### 型を選ばずに使う
+
+`Regex.is_match_text`・`find_text`・`find_at_text`・`find_all_text`・`captures_text`・`replace_all_text`・`split_text` は、テキストの型が `string` なら `_text` を除いた名前の関数（`is_match`・`find`・…）、`utf8string` なら `_utf8` のついた名前の関数（`is_match_utf8`・`find_utf8`・…）を呼びます。引数の順番、意味、範囲の単位（`string` はコード単位、`utf8string` は byte）は、呼ばれる関数と同じです。`_utf8` のないパターンは、どちらの型にも同じ `Regex` を使います。
+
+```tsuzuri run=%281%2C%202%29%20%285%2C%206%29%20a-b-%20%F0%9F%98%80a-b-%202%202%20true
+def match_count :: RegexText.RegexText<'s> => ref Regex -> ref 's -> i64
+fn match_count re text = (Regex.find_all_text re text).length
+
+let re = Result.get (Regex.compile (ref "\\d+"))
+let text = "a1b22"
+let bytes = u8"😀a1b22"
+let first = match Regex.find_text (ref re) text with
+    | Maybe.Some (start, stop) -> $"({start}, {stop})"
+    | Maybe.None -> "none"
+let first8 = match Regex.find_text (ref re) bytes with
+    | Maybe.Some (start, stop) -> $"({start}, {stop})"
+    | Maybe.None -> "none"
+let dashed = Regex.replace_all_text (ref re) text "-"
+let dashed8 = Regex.replace_all_text (ref re) bytes u8"-"
+let counts = $"{match_count (ref re) text} {match_count (ref re) bytes}"
+$"{first} {first8} {dashed} {dashed8} {counts} {Regex.is_match_text (ref re) bytes}"
 ```
 
+実行結果:
+
+```text
+(1, 2) (5, 6) a-b- 😀a-b- 2 2 true
+```
+
+`-O1` 以上では、`Regex.*_text` と instance のメソッドは呼び出しに組み込まれ、関数としては残りません。オブジェクトコードの関数の集合は、`_utf8` を直接呼んだ場合と同じです（命令の並びは、LLVM の判断で数命令ずつ変わることがあります）。ジェネリックな関数に制約を書くときの型クラスは `RegexText` です。`RegexText` は opt-in の標準モジュールなので、ユーザーのコードでは `RegexText.RegexText<'s>` と修飾します。メソッドを直接呼ぶときは `RegexText.RegexText.find_all` のように 3 つに分けて書きますが、普通は `Regex.find_all_text` を使います。
+
 ## 構文
@@ -224,6 +255,13 @@
 | `captures` / `captures_utf8` | `ref Regex -> ref string -> Maybe<[(i64 * i64)]>` | 最初の一致の全体と各 group の範囲 |
 | `replace_all` / `replace_all_utf8` | `ref Regex -> ref string -> ref string -> string` | すべての一致を置換文字列で置き換えます |
 | `split` / `split_utf8` | `ref Regex -> ref string -> [string]` | 一致の間の部分文字列 |
+| `is_match_text` | `RegexText<'s> => ref Regex -> ref 's -> bool` | `is_match` か `is_match_utf8`（`'s` が `string` か `utf8string`） |
+| `find_text` | `RegexText<'s> => ref Regex -> ref 's -> Maybe<(i64 * i64)>` | `find` か `find_utf8` |
+| `find_at_text` | `RegexText<'s> => ref Regex -> ref 's -> i64 -> Maybe<(i64 * i64)>` | `find_at` か `find_at_utf8` |
+| `find_all_text` | `RegexText<'s> => ref Regex -> ref 's -> [(i64 * i64)]` | `find_all` か `find_all_utf8` |
+| `captures_text` | `RegexText<'s> => ref Regex -> ref 's -> Maybe<[(i64 * i64)]>` | `captures` か `captures_utf8` |
+| `replace_all_text` | `RegexText<'s> => ref Regex -> ref 's -> ref 's -> 's` | `replace_all` か `replace_all_utf8`。置換文字列は本文と同じ型 |
+| `split_text` | `RegexText<'s> => ref Regex -> ref 's -> ['s]` | `split` か `split_utf8` |
 | `search_steps` | `ref Regex -> ref string -> i64` | `find` と同じ探索の VM の手数。線形時間の試験のための関数です |
````

（`regex.md` の既存の `## この記事のポイント` に、`_text` の 1 行を足してよい。足すなら 1 行だけ: 「`string` と `utf8string` を型で選ぶ呼び分けが要らないときは、`is_match_text` などを使います。」）

### G.3 `_tsuzuri/language-reference/` のほかのページ

`Text` の分は Phase 1、`RegexText` の分は Phase 2。「場所」は見出しと、その節の中の目印の行で指す（行番号は動く）。

| ページ | 場所 | 追加・変更するもの（そのまま） | Phase |
| --- | --- | --- | --- |
| `index.md` | `#### 組み込み型 / 組み込みモジュール` の一覧の `- [Utf8Char](./built-in-types-and-modules/utf8char.md)` の次の行 | `- [Text](./built-in-types-and-modules/text.md)` | 1 |
| `built-in-types-and-modules/string.md` | `## この記事のポイント` の最後の箇条書きの次 | `- `string` と `utf8string` を書き分けずに `find` や `split` を呼びたいときは、[Text](text.md) の `Text.find` を使います。同じ結果で、呼び出しは `String.find` への直接の呼び出しになります。` | 1 |
| 同上 | `## 関連項目` の `- [Regex](regex.md) — 正規表現による検索・置換・分割` の次 | `- [Text](text.md) — `string` と `utf8string` に同じ名前で呼べる共通の操作（型クラス）` | 1 |
| `built-in-types-and-modules/utf8string.md` | `## この記事のポイント` の最後の箇条書きの次 | `string.md` と同じ 1 行 | 1 |
| 同上 | `## 関連項目` の `- [Regex](regex.md) — 正規表現による検索・置換・分割（`_utf8` の関数）` の次 | `string.md` の関連項目と同じ 1 行 | 1 |
| `built-in-types-and-modules/char.md` | `## 注意点` の最後の項目の次 | `> [!NOTE]` で始まる引用: `> `Char` には `Text` のような型クラスはありません。`Text` から文字を取り出す手段がなく、文字の型クラスだけでは使い道がないためです（[Text](text.md#入っていないもの)）。` | 1 |
| 同上 | `## 関連項目` の `- [Utf8Char](utf8char.md)` の次 | `- [Text](text.md)` | 1 |
| `built-in-types-and-modules/utf8char.md` | `## 注意点` の最後の項目の次、`## 関連項目` の `- [Char](char.md)` の次 | `char.md` と同じ引用と 1 行 | 1 |
| `built-in-types-and-modules/json.md` | `## API リファレンス` の表の次 | 段落: `` `Json` に `Text` を受ける版はありません。文書は `utf8string`、キーと文字列の値は `string` に固定されていて、同じ操作の 2 つの版がないためです（[Text](text.md)）。`Json.Value` の case の `Text` は型クラスの `Text` と関係なく、`Json.Text "a"` と修飾して書きます。`` | 1 |
| `built-in-types-and-modules/unicode.md` | `## 関連項目` の `- [String](./string.md) と [Utf8String](./utf8string.md) — …` の次 | `- [Text](./text.md) — `string` と `utf8string` に共通の検索・切り出し。`Unicode` の `_utf8` の対は、まだ型クラスにしていません` | 1 |
| `built-in-types-and-modules/regex.md` | G.2 | | 2 |
| `literals-and-strings/strings.md` | `## どちらを選ぶか` の最後の段落（`…と覚えると迷いません。`）の次 | 段落: `型を選ばずに同じ名前で検索・切り出し・分割をしたいときは、[Text](../built-in-types-and-modules/text.md) を使います。ジェネリックな関数は `Text<'s> =>` と書くと、`string` と `utf8string` の両方を受けられます。` | 1 |
| 同上 | `## 関連項目` の `- [Utf8Char](../built-in-types-and-modules/utf8char.md)` の次 | `- [Text](../built-in-types-and-modules/text.md)` | 1 |
| `types-and-type-inference/type-classes.md` | `## クラス名の解決` の mermaid の図の次、`## 組み込み型クラス` の前 | 段落: `標準ライブラリが `.tt` で宣言するクラスもあります。`Text`（[Text](../built-in-types-and-modules/text.md)）は `string` と `utf8string` に共通の操作で、無修飾の `Text` は段 5 で標準ライブラリのクラスに解決します。利用者が同じ名前のクラスを宣言すると段 2 か段 4 でそちらが選ばれるので、標準のクラスは `Text.Text` と修飾します。opt-in の標準モジュールが宣言するクラス（`RegexText`）は無修飾では候補にならず、`RegexText.RegexText<'s>` と修飾します。` | 1（`RegexText` の文は 2） |
| 同上 | `## 関連項目` の先頭 | `- [Text](../built-in-types-and-modules/text.md) — 標準ライブラリの型クラスの例（`string` と `utf8string`）` | 1 |
| `organizing-tsuzuri/modules.md` | 13 行目・`## 予約モジュール名` の本文・218 行目の `53 個` | `54 個`（Phase 2 は `55 個`） | 1, 2 |
| 同上 | `## 予約モジュール名` の `text` ブロックの最後の行 `MatrixView  Tensor      Atomic      Mutex       Channel` | `MatrixView  Tensor      Atomic      Mutex       Channel     Text`（Phase 2 は、次の行に `RegexText` を足す） | 1, 2 |
| `compiler/diagnostics.md` | 変更なし | E1011・E1013 の行は一般的な説明で、`Text` の追加で誤りになる記述はない（`grep -c "Text" compiler/diagnostics.md` は 0） | — |

### G.4 `docs/language.md`・`docs/architecture.md`・`README.md`

**`docs/language.md`**

1. （Phase 1・2）予約名の一覧（`Maybe`、`Result`、… と並んで `Channel` で終わる 1 行）と、「…は後から予約語として追加されたモジュール名です。」の直前の一覧（`HashMap`、`HashSet`、… `Channel` で終わる 1 行）の、両方の末尾に `Text` を足す。Phase 2 は続けて `RegexText` を足す。
2. （Phase 2）「ただし、後発の std モジュール …（opt-in std モジュール）は、…」で始まる段落の一覧の末尾に `RegexText` を足す。次の段落の「`Regex` は `Unicode` を、`Cbor` は `Json` を伴います。」を、「`Regex` は `Unicode` と `RegexText` を、`RegexText` は `Regex` を、`Cbor` は `Json` を伴います。」に変える。
3. （Phase 1）一時値の借用の段落（「共有参照 `ref T` を要求する仮引数に対しては、…」で始まる段落と、その次の「このような一時値は、…」の段落）の次に、1 つの段落を足す。

   > 型クラスのメソッド（`Text.find "a" text` の形）にも同じ規則が働きます。引数をすべてそろえた呼び出しだけが対象で、部分適用は `E1013` のままです。

4. （Phase 1）文字列の関数の表（最後の行が `compare left right` の表）の次、「`slice` および `sub` は、…」で始まる段落の前に、1 つの段落を足す。

   > `Text<'s>`（`string` と `utf8string`）は、この表の操作のうち `decode_at`・`chars`・`char_count` を除いたものと `length`・`clone`・`is_empty` を、同じ名前のメソッド（21 個）にした標準の型クラスです（`std/Text.tt`）。メソッドの意味は `String.f`・`Utf8String.f` と同じで、呼び出しは instance のメソッドへの直接の呼び出し（その本体は `String.f`・`Utf8String.f` の 1 回の呼び出し）です。単位・境界・空のパターン・順序の違いは型ごとにそのまま残ります。

5. （Phase 2）`### Regex` の箇条書きのうち、「`Regex` は不透明な非 Copy の record …」で始まる箇条書きの前に、1 つの箇条書きを足す。

   > - `is_match_text`・`find_text`・`find_at_text`・`find_all_text`・`captures_text`・`replace_all_text`・`split_text` は、テキストの型（`string` か `utf8string`）に応じて `is_match`・… か `is_match_utf8`・… を呼ぶ薄い関数です（型クラス `RegexText`。opt-in なので、ユーザーのコードでは `RegexText.RegexText<'s>` と修飾します）。

**`docs/architecture.md`**

1. （Phase 2）「例外は `stdlib::OPT_IN` の opt-in std モジュール（`Arena`、`Regex`、… `Channel`。D-40・D-41・D-42・D-44）です。」の一覧の末尾に `RegexText` を足す。
2. （Phase 1・2）「**Channel（F10 Phase 2）:**」の段落の「`RESERVED_MODULES` は 53 個」を「`RESERVED_MODULES` は 54 個」に変える。Phase 2 は「55 個」にする。
3. （Phase 1・2）「**行列と N 次元配列（C11）:**」の段落の次に、下の段落を足す（Phase 2 は同じ段落の末尾に、`RegexText` の文を足す）。

**`README.md`**

1. （Phase 1）「**一時値の借用**」の箇条書きの末尾に、「型クラスのメソッド（`Text.find "a" text`）も同じです。」を足す。
2. （Phase 2）「**正規表現 (`Regex`)**」の箇条書きの「（UTF-8 は `_utf8` 版）」のあとに、「、型を選ばない `is_match_text`・`find_text`・`find_at_text`・`find_all_text`・`captures_text`・`replace_all_text`・`split_text` もあります」を足す。
3. （Phase 1・2）予約名の表の `String`, `Utf8String`, `Char` の行の次に、次の行を足す。Phase 2 は、この行の説明の末尾（最後の `|` の前）に「。opt-in の `RegexText` は `Regex.*_text` が使う」を足す。

   ```text
   | `Text` | `string` と `utf8string` に共通の操作の型クラス（`Text.find`・`Text.split`・`Text.trim` の形。`String`・`Utf8String` の同名の関数への直接の呼び出し） |
   ```

`docs/architecture.md` に足す段落（Phase 1 の分。Phase 2 の `RegexText` の文は、Phase 2 で同じ段落に足す）:

> **`Text` クラス（D14）:** `Text` は `std/Text.tt` の型クラス（`class Ord<'s> => Text<'s>`。21 メソッド）で、常に読み込む（opt-in にすると、ユーザーのコードがクラスを `Text.Text` と修飾することになる）。instance は `std/String.tz` の `instance Text<string>` と `std/Utf8String.tz` の `instance Text<utf8string>` で、各メソッドの本体は `String.f`・`Utf8String.f`・組み込みの `clone_string`・`Utf8String.clone`・`String.compare`・`Utf8String.compare` を 1 回呼ぶだけ。`is_empty` は既定の実装（`Text.length text == 0`）。単相化の後は、呼び出しが instance のメソッドへの直接の呼び出しになり、辞書も間接呼び出しもない（`tests/text_class.mjs` が IR の構造で検査する）。`Text` は `RESERVED_MODULES` に足した予約名で、`Text.tz`・`Text.tt`・`Text.tc` と `namespace Text` は `E1011`。コンパイラ側の変更は `Checker::temporary_borrows`（`src/constants.rs`）の 1 腕だけで、クラスのメソッド（`TypedExprKind::Method`）の呼び出しを、`callee.ty` の仮引数の数が実引数の数と同じときに「引数を一度に適用する呼び出し」と数える。それまでは引数が 2 個以上のメソッドへ一時値の借用を渡すと `E1013` だった。特殊化の予算は、(メソッド, 型) の組ごとに 1 つ、直接の呼び出しより多く使う。

### G.5 `docs/benchmarks.md`（Phase 3）

`## 静的データ（D11 Phase 2）` の節の次、`## 性能指標の記録` の前に、次の節を足す。表は実装者が `node tests/text_class.mjs target/release/tsuzuri --objects` を走らせた結果に置き換える（下は試作での測定）。

````markdown
## Text クラスの呼び出し（D14）

`Text.f`（`std/Text.tt` の型クラス）は、`string` と `utf8string` に使うと、instance のメソッド（`String.f` か `Utf8String.f` を 1 回呼ぶだけの関数）への直接の呼び出しになります。辞書・vtable・間接呼び出しはありません。ここで測るのは速度ではなく、生成物が直接呼び出しと同じかどうかです。速度の合否の閾値は置きません。

再現: `cargo build --release --locked` のあとに `node tests/text_class.mjs target/release/tsuzuri --objects`。20 メソッドを `string` と `utf8string` に使う `tests/fixtures/text_class_calls` と、直接の関数を呼ぶ `tests/fixtures/text_class_direct` を同じ条件でビルドして、オブジェクトコードを比べます。表明するのは IR の構造（instance のメソッドが 1 回の呼び出しで、呼び先が同じ名前の関数であること）だけで、下の一致は測定です。

| 構成 | 結果 |
| --- | --- |
| native `-O2`・`-O3`・`-Os`・`-Oz` | バイト一致 |
| native `-O1` | 一致しない（同じ大きさで、数命令の違い） |
| wasm32 `-O1`・`-O2` | バイト一致 |
| wasm32 `-O3` | 一致しない（直接 21,020 バイト、`Text` 21,264 バイト。約 1.2%） |
| `-O0` | 一致しない（instance のメソッドが関数として残り、呼び出しが 1 つ増える） |

4 メソッドだけの小さいプログラム（付録 1 の B.3 の `score`・`score8`）では、native の `-O1`〜`-Oz` と wasm32 の `-O1`〜`-O3` のすべてがバイト一致でした。大きいプログラムの差は、LLVM の組み込みの判断で呼び出し元の命令が数個動くことによるものです。`Text` を使わない関数のコードは変わりません。
````

## H. IR を比べる手順（Phase 1 の手順 7、Phase 2 の手順 6）

「`String.*`・`Utf8String.*` を使う既存のプログラムの IR は、`Text` を足しても変わらない」ことの確認。**一時のスクリプトで、コミットしない**。変更前の HEAD のバイナリを `target/tsuzuri-before` に保存してから変更し、変更後に `cargo build --release --locked` する。

```sh
cp target/release/tsuzuri target/tsuzuri-before     # 変更前（手順 1 のベースラインのあと）
# ……変更して、cargo build --release --locked ……
export PATH="$(brew --prefix llvm)/bin:$(brew --prefix lld)/bin:$PATH"
python3 target/ir_compare.py target/tsuzuri-before target/release/tsuzuri target/ir-compare $(ls -d examples/*/ tests/fixtures/*/)
```

期待する出力（試作での結果。HEAD で作れる 102 個）: `byte-identical: 50` / `identical after renumbering: 51` / `DIFFERENT` が 1 件（`tests/fixtures/computations`。`Text.tc` の改名の前だけ。改名のあとは `DIFFERENT: 0`）。作れないディレクトリ（ライブラリや引数が要るもの）は `both failed to build` に入る。`DIFFERENT` が 0 でなければ、本文の停止条件 S2。

`target/ir_compare.py`:

```python
"""Compare the LLVM IR of programs built by two compilers (D14 step 7).

Usage: python3 ir_compare.py HEAD_COMPILER NEW_COMPILER WORKDIR PROJECT_DIR...

Each project is built with `build --emit llvm -O0` by both compilers into WORKDIR. The IR must be byte-identical or,
after the numbers of generated symbols (`$intrinsic.Display.display.428`, `$instance.12`) are renumbered in the order of
first appearance (the rule of `renumbered` in tests/bench.rs), identical. Anything else is a difference.
"""
import os
import subprocess
import sys


def renumbered(ir):
    numbers = {}
    output = []
    rest = ir
    while True:
        dollar = rest.find("$")
        if dollar < 0:
            break
        output.append(rest[: dollar + 1])
        rest = rest[dollar + 1 :]
        end = 0
        digits = 0
        while True:
            word = 0
            while end + word < len(rest) and rest[end + word].isascii() and (rest[end + word].isalpha() or rest[end + word] == "_"):
                word += 1
            if word == 0:
                digits = 0
                break
            end += word
            if not rest[end:].startswith("."):
                digits = 0
                break
            after = rest[end + 1 :]
            count = 0
            while count < len(after) and after[count].isdigit():
                count += 1
            if count > 0:
                digits = count
                break
            end += 1
        if digits == 0:
            continue
        kind = rest[:end]
        number = rest[end + 1 : end + 1 + digits]
        canonical = numbers.setdefault((kind, number), len(numbers))
        output.append("%s.#%d" % (kind, canonical))
        rest = rest[end + 1 + digits :]
    output.append(rest)
    return "".join(output)


def build(compiler, project, output):
    result = subprocess.run([compiler, "build", project, "--emit", "llvm", "-O0", "-o", output], capture_output=True, text=True)
    return result.returncode, (open(output).read() if result.returncode == 0 and os.path.exists(output) else ""), result.stderr.strip().splitlines()[:1]


head, new, work = sys.argv[1:4]
os.makedirs(work, exist_ok=True)
identical, renumbered_same, both_failed, different = [], [], [], []
for project in sys.argv[4:]:
    label = project.strip("/").replace("/", "_")
    a = build(head, project, os.path.join(work, label + ".head.ll"))
    b = build(new, project, os.path.join(work, label + ".new.ll"))
    if a[0] != 0 and b[0] != 0:
        both_failed.append(project)
    elif a[0] != b[0]:
        different.append((project, "build status", a[2], b[2]))
    elif a[1] == b[1]:
        identical.append(project)
    elif renumbered(a[1]) == renumbered(b[1]):
        renumbered_same.append(project)
    else:
        different.append((project, "IR differs", [], []))
print("byte-identical:", len(identical))
print("identical after renumbering:", len(renumbered_same))
print("both failed to build:", len(both_failed))
print("DIFFERENT:", len(different))
for item in different:
    print("  ", item)
sys.exit(1 if different else 0)
```

