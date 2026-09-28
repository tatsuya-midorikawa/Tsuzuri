# 単語の出現回数を集計する

[実装例の一覧](README.md) · [ドキュメントのトップ](../README.md)

短いテキストから単語を取り出し、出現回数を名前順に表示するアプリケーションです。String の走査、所有する部分文字列、Map の更新、借用 fold を組み合わせます。

## 完成時の動作

入力が `Rust rust; Tsuzuri, TSUZURI! LLVM 21.` の場合:

```text
21: 1
llvm: 1
rust: 2
tsuzuri: 2
```

この例では ASCII の英字と数字が連続した部分を一つの単語とします。大小文字を区別せず、句読点、空白、改行、非 ASCII 文字は区切りです。たとえば `can't` は can と t に分かれます。

日本語の形態素解析、Unicode の単語境界、語幹への正規化、頻度順のランキングは扱いません。入力は main 内の固定文字列で、ファイルを読む機能は[ファイル集計アプリ](file-statistics.md)で紹介します。

## 使用する機能

| 機能 | この例での用途 |
| --- | --- |
| String.to_ascii_lower | 英字を小文字へそろえる |
| while と短絡条件 | 境界を越えずに単語の終端を探す |
| String.slice | 一単語の所有 string を作る |
| Map<string, i64> | 単語と回数を保持する |
| clone_string | 検索で消費するキーと挿入するキーを分ける |
| Map.fold | キー順に借用してレポートを作る |

## 完成コード

作業ディレクトリ `target/word-count` の `Main.tz` に置く、一つのプログラムです。

```tsuzuri run=21%3A%201%0Allvm%3A%201%0Arust%3A%202%0Atsuzuri%3A%202
def is_word_unit :: i16u -> bool
fn is_word_unit value =
    (value >= 97i16u && value <= 122i16u) ||
    (value >= 48i16u && value <= 57i16u)

def count_words :: ref string -> Map<string, i64>
fn count_words text =
    let normalized = String.to_ascii_lower text
    let mut counts: Map<string, i64> = Map.empty()
    let mut index = 0
    while index < normalized.length do
        if is_word_unit normalized[index] then
            let start = index
            while index < normalized.length && is_word_unit normalized[index] do
                index = index + 1
            let word = Option.get (String.slice (ref normalized) start index)
            let previous = Map.get (ref counts) (clone_string (ref word))
            let count = Option.default_value 0 previous
            counts = Map.insert counts word (count + 1)
        else
            index = index + 1
    counts

def format_counts :: ref Map<string, i64> -> string
fn format_counts counts =
    if Map.is_empty counts then "(no words)"
    else Map.fold counts "" (\report word count ->
        let separator = if report.length == 0 then "" else "\n"
        report + separator + clone_string word + ": " + to_string (deref count))

test "merges ASCII letter case" =
    let text = "Rust RUST rust"
    let counts = count_words ref text
    assert (Option.get (Map.get (ref counts) "rust") == 3)
    assert (Map.length (ref counts) == 1)

test "handles empty input" =
    let text = ""
    let counts = count_words ref text
    assert (Map.is_empty ref counts)
    assert (format_counts (ref counts) == "(no words)")

test "ignores repeated separators" =
    let text = ",; \n!"
    let counts = count_words ref text
    assert (Map.is_empty ref counts)

test "finishes a word at the end of input" =
    let text = "llvm21"
    let counts = count_words ref text
    assert (Option.get (Map.get (ref counts) "llvm21") == 1)

test "treats non-ASCII text as separators" =
    let text = "one\u{65E5}\u{672C}\u{8A9E}two"
    let counts = count_words ref text
    assert (Map.length (ref counts) == 2)
    assert (Map.contains_key (ref counts) "one")
    assert (Map.contains_key (ref counts) "two")

def main :: string
fn main =
    let text = "Rust rust; Tsuzuri, TSUZURI! LLVM 21."
    let counts = count_words ref text
    format_counts ref counts
```

## 実行する

リポジトリルートから実行します。

```sh
./target/release/tsuzuri check target/word-count
./target/release/tsuzuri run target/word-count
./target/release/tsuzuri test target/word-count
```

main の入力を `"One fish, two fish."` にすると、fish が 2、one と two が 1 になります。キー順の列挙なので、最も多い単語を先頭にする処理ではありません。

## 走査の仕組み

1. 小文字化した独立の string を作る。
2. index が英数字を指していなければ一コード単位進める。
3. 単語の先頭を記録し、英数字が続く間だけ index を進める。
4. 半開区間 start から index を切り出し、回数を更新する。
5. Map をキー順に走査して表示する。

内側の while は、まず index < length を検査します。&& の短絡によって、文字列末尾の外へアクセスしません。String.slice に渡す両端はこの走査で検証済みなので、Option.get が None を取り出すことはありません。

## 所有権の流れ

count_words の入力は共有借用です。小文字化した文字列と各単語の切り出しは独立した所有値です。

Map.get の検索キーは値引数で消費されるため、ここでは word の複製を検索へ渡します。元の word は続く Map.insert へ移動します。キーを借用する別 API があると推測して `ref word` に置き換えないでください。

format_counts は Map のキーと回数を借用します。表示へ連結するキーだけを複製し、Map 自体は変更しません。

## 規模と制限

文字列の走査は前へ進むだけですが、Map は整列した Vec を使います。検索は O(log n)、新しい単語の挿入は O(n) で、語彙数が増えると挿入費用も増えます。

レポートの文字列連結も成長した出力を繰り返しコピーします。この実装は短い文章と小さい語彙を対象にした例です。大きいログ集計へ拡張するなら、表示用の行を Vec に集めて一度だけ String.join する方法や、ホスト側の入出力を検討します。

UTF-16 の索引は文字・書記素ではなくコード単位です。この例では対象を ASCII に限定しているため、非 ASCII の単語解析を暗黙に行うことはありません。

## 関連項目

- [文字列と Unicode](../language-reference/strings-and-characters.md)
- [String API](../library-reference/text.md)
- [Map と Set](../library-reference/map-set.md)
- [while と短絡](../language-reference/control-flow.md)
