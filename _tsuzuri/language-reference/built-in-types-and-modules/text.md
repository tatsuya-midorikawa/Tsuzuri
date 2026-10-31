# Text

`Text` は、`string`（UTF-16）と `utf8string`（UTF-8）が共通に持つ操作を 1 つにまとめた、標準ライブラリの型クラスです。`Text.find`・`Text.split`・`Text.trim` のように、型を書き分けずに同じ名前で呼べます。メソッドの意味は `String` と `Utf8String` の同名の関数と同じで、呼び出しは instance が持つその関数への呼び出しにそのまま置き換わります。`String` と `Utf8String` は残ります。

型そのものの契約は [文字列](../literals-and-strings/strings.md)、関数ごとの契約は [String](string.md) と [Utf8String](utf8string.md) を参照してください。型クラスの一般的な説明は [型クラス](../types-and-type-inference/type-classes.md) です。

## この記事のポイント

- `Text.f` は、引数が `string` なら `String.f`、`utf8string` なら `Utf8String.f` と同じ結果を返します。引数の並び（パターンや区切りが先で、本文が最後）も同じです。
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
    match Text.slice first last text with
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
let high = "～"
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

`'s` は `string` か `utf8string`（または `Text` の instance を持つ利用者の型）です。

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
| `slice` | `i64 -> i64 -> ref 's -> Maybe<'s>` | `String.slice`／`Utf8String.slice` |
| `sub` | `i64 -> i64 -> ref 's -> Maybe<'s>` | `String.sub`／`Utf8String.sub` |
| `join` | `ref 's -> ref ['s] -> 's` | `String.join`／`Utf8String.join` |
| `concat` | `ref ['s] -> 's` | `String.concat`／`Utf8String.concat` |
| `split` | `ref 's -> ref 's -> ['s]` | `String.split`／`Utf8String.split` |
| `trim` | `ref 's -> 's` | `String.trim`／`Utf8String.trim` |
| `trim_start` | `ref 's -> 's` | `String.trim_start`／`Utf8String.trim_start` |
| `trim_end` | `ref 's -> 's` | `String.trim_end`／`Utf8String.trim_end` |
| `to_ascii_lower` | `ref 's -> 's` | `String.to_ascii_lower`／`Utf8String.to_ascii_lower` |
| `to_ascii_upper` | `ref 's -> 's` | `String.to_ascii_upper`／`Utf8String.to_ascii_upper` |
| `repeat` | `i64 -> ref 's -> 's` | `String.repeat`／`Utf8String.repeat` |
| `replace` | `ref 's -> ref 's -> ref 's -> 's` | `String.replace`／`Utf8String.replace` |

メソッドの失敗と計算量は、呼び出す関数と同じです（[String の計算量](string.md#計算量)）。`is_empty` だけは `String` と `Utf8String` にない新しい操作で、`length` が 0 かどうかを返します。`compare`・`clone` は、組み込み関数 `String.compare`・`clone_string`（`Utf8String.compare`・`Utf8String.clone`）の名前をそろえた入口です。

## 引数の順序

標準ライブラリ全体の規則は、[標準ライブラリの引数の順序](./argument-order.md)にまとめてあります。

`Text` のメソッドも、読み取る文字列が最後の引数です。パターン・区切り・接頭辞・接尾辞・置換文字列・位置・個数は、その前に `String` と同じ順で並べます（`Text.find needle text`、`Text.replace needle replacement text`、`Text.slice first last text`、`Text.repeat count text`、`Text.join separator parts`）。パイプと合成も、`String` と同じ形で使えます。

```tsuzuri run=word%3Dsuz%20loud%3DSUZ%20at%3D2
let middle = Text.sub 1 3 >> Maybe.default_value u8""
let word = u8"Tsuzuri" |> middle
let loud = word |> Text.to_ascii_upper
let z = u8"z"
let at = word |> Text.find z |> Maybe.default_value (-1)
$"word={word} loud={loud} at={at}"
```

実行結果:

```text
word=suz loud=SUZ at=2
```

`Text.find z` のように引数の一部だけを渡すときは、`String.find` と同じく、借用する引数を先に `let` で束縛します（リテラルを渡すと `E1013`）。

## 入っていないもの

| 関数 | 理由 |
| --- | --- |
| `char_count` | 同じ名前で意味が違います。`String.char_count` は `length` と同じ値（O(1)）、`Utf8String.char_count` はスカラー数（O(n)）です |
| `chars`、`decode_at` | 要素の型が違います（`char` と `utf8char`）。`decode_at` は進む幅も違います |
| `to_code_units`・`to_bytes`、`from_code_units`・`from_bytes` | 要素の型が違い、`from_bytes` は検査して `Maybe` を返します |
| `String.from_utf8`、`Utf8String.from_string` | 2 つの型のあいだの変換は、暗黙にせず、明示した呼び出しだけにします |
| `String.is_well_formed`、`String.to_well_formed` | `string` だけの操作です（孤立サロゲートがあるのは `string` だけ） |

`Char` と `Utf8Char` の 6 つずつの関数にも、型クラスはありません。`Text` から文字を取り出す手段がないので、文字の型クラスだけでは使い道がないからです。正規表現は [Regex](regex.md) の `Regex.find_text` などが、同じように `string` と `utf8string` の両方を受けます。

## 費用

- `Text.f` は、`string` や `utf8string` に使うと、instance のメソッド（`String.f` を呼ぶだけの関数）への直接の呼び出しになります。型クラスの辞書も vtable も間接呼び出しも作りません。
- 最適化（`-O1` 以上）では、メソッドが呼び出し元に組み込まれます。小さいプログラムでは、直接呼んだ場合とオブジェクトコードが同じです。呼び出しの多い大きなプログラムでは、LLVM の判断で呼び出し元の命令数が数 % 違うことがあります（[測定](../../../docs/benchmarks.md#text-クラスの呼び出しd14)）。`-O0` では、呼び出しが 1 段増えます。
- 特殊化の予算（65,536）は、使ったメソッドと型の組ごとに 1 つ多く使います。`String.length` を直接呼ぶと 1 つ、`Text.length` を `string` に使うと instance のメソッドと `String.length` の 2 つです。使わない `Text` は予算を使わず、IR にも出ません。

## 注意点

> [!WARNING]
> ユーザーのモジュールの名前に `Text` は使えません。`Text.tz`・`Text.tt`・`Text.tc` と `namespace Text` は `E1011` です。同じ名前の record・union・型エイリアス・型クラスは宣言できます。ユーザーが `class Text` か `union Text` を宣言すると、無修飾の `Text.length` はユーザーの宣言を指すので、標準の `Text` は `Text.Text` と書きます（メソッドは `Text.Text.length`）。

> [!WARNING]
> 結果の型が `'s` のメソッド（`Text.trim`・`Text.clone` など）の呼び出しを、借用を取る引数へそのまま渡すと、`'s` が参照の型に決まってしまい `E1003` になります（`Text.length (Text.trim text)`、`String.length (Text.clone text)`）。`Maybe.get` などほかのジェネリックな関数の結果と同じ制限です。結果を `let` で束縛するか、`Text.length (ref (Text.trim text))` と `ref` を明示します。

- 引数を全部そろえない部分適用で一時値を借用すると、直接の関数と同じく `E1013` です。`let f = Text.find "a"` は、先に `let` で束縛してください。
- `Text<'s> =>` のジェネリックな関数の中では、どの instance が呼ばれるかがまだ決まらないので、引数が 2 つ以上のメソッドへ一時値を借用で渡すと `E1013` です（`Text.contains (ref (Text.trim needle)) text`）。`let trimmed = Text.trim needle` と束縛してから渡します。`string` と `utf8string` に直接使う呼び出しは、この制限を受けません。
- ユーザーの型にも instance を書けます。`Ord` の instance（`deriving (Eq, Ord)` で作れます）と、`is_empty` 以外のすべてのメソッドを実装します。足りないメソッドは `E1016`（`missing method 'Text.Text.trim_end'`）、`Ord`・`Eq` の instance がなければ `E1027` です。
- instance のない型に使うと `E1005`（`no instance for Text.Text<i32>`）、`string` と `utf8string` を混ぜると `E1003` です。
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
- [標準ライブラリの引数の順序](argument-order.md)
- [言語リファレンスの目次](../index.md)
