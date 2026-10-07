# String

`String` は、UTF-16 の `string` を検索・切り出し・連結する標準モジュールです。長さの単位はコード単位で、`.length` と同じです。文字境界や書記素では切りません。

型そのものの所有権、比較、サロゲートは [文字列](../literals-and-strings/strings.md) を参照してください。ここは関数の契約です。

## この記事のポイント

- 引数が `ref string` の関数は共有借用です。入力は消費せず、戻り値は新しい所有文字列です。
- `find` と `split` は、探したい側が第 1 引数です。本文が先ではありません。
- 見つからない、範囲が不正、という失敗は `None` です。範囲外の添字と負の `repeat` はトラップします。
- 検索は素朴な走査で、最悪計算量は本文の長さとパターンの長さの積です。SIMD は保証しません。パターンで探すときは [Regex](regex.md) です。
- `clone_string` だけは `String.clone` ではありません。

> [!WARNING]
> `String.find "zu" title` です。`String.find title "zu"` ではありません。`split`、`starts_with`、`replace` も、パターンが本文より前です。

## 長さと走査

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `length` | `ref string -> i64` | コード単位数。`.length` と同じで O(1)。コピーしない |
| `char_count` | `ref string -> i64` | `length` と同じ値。スカラー数ではない。O(1) |
| `chars` | `ref string -> [char]` | 各コード単位を `char` にした新しい配列。O(n) |
| `decode_at` | `ref string -> i64 -> (char * i64)` | その位置の `char` と、次の位置。常に 1 進む |

`decode_at` はサロゲートペアを 1 文字にまとめません。添字が範囲外だとトラップします。`None` にはなりません。

```tsuzuri run=decoded%3D%5BA%5D1%20chars%3D%5B%27%5CuD83D%27%2C%20%27%5CuDE00%27%5D%20units%3D2
let text = "AB"
let decoded = match String.decode_at (ref text) 0 with
    | (value, next) -> $"[{value}]{next}"
let chars = String.chars "😀"
$"decoded={decoded} chars={chars} units={String.char_count "😀"}"
```

実行結果:

```text
decoded=[A]1 chars=['\uD83D', '\uDE00'] units=2
```

`😀` は 2 コード単位なので、`chars` も 2 要素です。配列の表示は文字を引用符で囲み、サロゲートは `\uXXXX` と出します。

`for unit in text` が返すのも `char` ではなく `i16u` です。文字が欲しいときは `Char.of_u16 unit` にします。

## 検索と比較

すべて共有借用です。空のパターンは、先頭や末尾に一致したとみなします。`find "" text` は `Some 0`、`rfind "" text` は `Some text.length`、`contains` / `starts_with` / `ends_with` の空パターンは真です。

| 関数 | シグネチャ | 失敗 | 計算量 |
| --- | --- | --- | --- |
| `find` | `ref string -> ref string -> Maybe<i64>` | 無ければ `None`。第 1 引数がパターン | 最悪 O(本文 × パターン) |
| `rfind` | `ref string -> ref string -> Maybe<i64>` | 無ければ `None`。最後の一致 | 最悪 O(本文 × パターン) |
| `contains` | `ref string -> ref string -> bool` | トラップしない | `find` と同じ |
| `starts_with` | `ref string -> ref string -> bool` | トラップしない | O(パターン) |
| `ends_with` | `ref string -> ref string -> bool` | トラップしない | O(パターン) |
| `compare` | `ref string -> ref string -> i64` | トラップしない。左が小さければ `-1`、等しければ `0`、大きければ `1` | O(短いほう) |

比較の順序は演算子 `<` と同じ、符号なし 16 bit のコード単位順です。ロケール順ではありません。

## 切り出し

| 関数 | シグネチャ | 失敗 | 計算量 |
| --- | --- | --- | --- |
| `slice` | `ref string -> i64 -> i64 -> Maybe<string>` | 負の位置、`last < first`、範囲外なら `None` | O(切り出し長) |
| `sub` | `ref string -> i64 -> i64 -> Maybe<string>` | 負の位置、加算オーバーフロー、範囲外なら `None` | O(切り出し長) |

`slice text first last` の `last` は結果に含まれません。`sub text first count` は `first` から `count` コード単位を切り出します。どちらも新しい所有文字列を `Some` で返します。

負の位置、`last < first`、範囲外の指定、および加算オーバーフローが発生した場合は安全に `None` を返します（トラップしません）。サロゲートペアの途中インデックスであっても、位置として有効であればそのまま切り出します。

どちらも結果の長さに比例したバッファを確保してコピーします。元の文字列は借用するだけです。

## 組み立て

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `concat` | `ref [string] -> string` | 区切りなしでつなぐ。空配列は `""` |
| `join` | `ref string -> ref [string] -> string` | 区切りは第 1 引数。空配列は `""` |
| `split` | `ref string -> ref string -> [string]` | 区切りは第 1 引数。本文は第 2 引数 |
| `replace` | `ref string -> ref string -> ref string -> string` | パターン、置換、本文の順 |
| `repeat` | `ref string -> i64 -> string` | `count` 回繰り返す。0 回は `""` |

`split` の区切りが空なら、コード単位のあいだで切ります。`"ab"` は `["a", "b"]`、空文字列は空配列です。区切りが空でないときは、一致ごとに切ります。先頭、末尾、連続した一致は空要素になります。空文字列を `","` で割ると `[""]` です。両端に、一致以外の空要素は足しません。

`replace` は左から、重ならない一致を置換します。置換後の文字列は再検索しません。空のパターンは、コード単位の境界と両端に挿入します。空文字列には 1 回だけ挿入します。

`repeat` の負の回数はトラップします。結果が 2^53 − 1 コード単位を超えるとき、または長さの積が溢れるときもトラップします。`join`、`concat`、`replace` も、結果の長さを先に検査してから 1 回だけ確保します。

```tsuzuri run=index%3D3%20parts%3D%5B%22a%22%2C%20%22%22%2C%20%22b%22%5D%20piece%3DTsu%20lower%3Dtsuzuri
let title = "Tsuzuri"
let found = String.find "zu" title
let parts = String.split "," "a,,b"
let piece = String.slice title 0 3
let index = match found with
    | Some value -> to_string value
    | None -> "none"
let text = match piece with
    | Some value -> value
    | None -> "none"
$"index={index} parts={parts} piece={text} lower={String.to_ascii_lower title}"
```

実行結果:

```text
index=3 parts=["a", "", "b"] piece=Tsu lower=tsuzuri
```

配列の表示は要素の文字列を引用符で囲みます。`parts` の 2 番目は空文字列です。

```tsuzuri run=dashed%3D-a-b-%20once%3D-%20rep%3Dhahaha
let dashed = String.replace "" "-" "ab"
let once = String.replace "" "-" ""
$"dashed={dashed} once={once} rep={String.repeat "ha" 3}"
```

実行結果:

```text
dashed=-a-b- once=- rep=hahaha
```

## 空白と ASCII の大小

| 関数 | シグネチャ | 説明 | 計算量 |
| --- | --- | --- | --- |
| `trim` | `ref string -> string` | 先頭と末尾の ASCII 空白を除去 | O(n) |
| `trim_start` | `ref string -> string` | 先頭の ASCII 空白を除去 | O(n) |
| `trim_end` | `ref string -> string` | 末尾の ASCII 空白を除去 | O(n) |
| `to_ascii_lower` | `ref string -> string` | ASCII 英大文字（`A`〜`Z`）を小文字に変換 | O(n) |
| `to_ascii_upper` | `ref string -> string` | ASCII 英小文字（`a`〜`z`）を大文字に変換 | O(n) |

`trim`、`trim_start`、`trim_end` が取り除くのは ASCII の空白文字だけです。具体的には、コード単位 9〜13（タブ、改行、垂直タブ、フォームフィード、復帰）と 32（空白）が対象です。全角空白は除去しません。新しい所有文字列を返し、失敗することはありません。

`to_ascii_lower` と `to_ascii_upper` は、ASCII の英文字だけを大文字・小文字に変換します。それ以外のコード単位は変更せずそのまま保持します。すべての文字の大文字小文字の変換は [Unicode](unicode.md) の `to_lower`・`to_upper` です。

## 所有権を移す変換

次の関数は入力を消費します。ヒープ上のバッファなら、ポインタを移すだけでコピーしません。定数領域のリテラルは、先にヒープへコピーされます。エンコーディングは変えません。

| 関数 | シグネチャ | 失敗 |
| --- | --- | --- |
| `to_code_units` | `string -> [i16u]` | しない。孤立サロゲートも残る |
| `from_code_units` | `[i16u] -> string` | 長さが 2^53 − 1 を超えるとトラップ |
| `clone_string` | `ref string -> string` | しない。O(n) のコピー。入力は残る |

`clone_string` はモジュール名が付きません。`Utf8String.clone` と対になる組み込み関数です。

## エンコーディング

| 関数 | シグネチャ | 失敗 | 計算量 |
| --- | --- | --- | --- |
| `from_utf8` | `ref utf8string -> string` | 入力は常に妥当な UTF-8。上限超過はトラップ | O(n) |
| `is_well_formed` | `ref string -> bool` | しない。孤立サロゲートが無ければ真 | O(n) |
| `to_well_formed` | `ref string -> string` | しない。孤立サロゲートを U+FFFD にする | O(n) |

`Utf8String.from_string` は逆向きで、孤立サロゲートではトラップします。コンソール出力も UTF-8 なので、整形式でない `string` をそのまま最後の式にするとトラップします。先に `is_well_formed` を見るか、`to_well_formed` を明示してください。

## 注意点

> [!NOTE]
> 関数はカリー化されています。`String.find "zu"` は、残りの `string` を受け取る関数です。`"Tsuzuri" |> String.find "zu"` のようにパイプラインへ渡せます。

> [!WARNING]
> `slice` と `sub` の失敗は `None`、`decode_at` と添字の範囲外はトラップです。同じ「範囲外」でも戻り方が違います。

## まとめ

- 読む関数は `ref` で、戻り値は新しい所有文字列です。
- パターンを取る関数は、パターンが第 1 引数です。
- 切り出しの失敗は `None`、添字と負の繰り返しはトラップです。
- 空の区切りはコード単位の境界、空でない区切りは空要素を残します。
- 孤立サロゲートを UTF-8 へ出す前に、検査か置換を明示します。

## 関連項目

- [Unicode](unicode.md) — 正規化、書記素クラスター、単語境界、完全な大文字小文字の変換
- [Regex](regex.md) — 正規表現による検索・置換・分割
- [文字列](../literals-and-strings/strings.md)
- [Utf8String](utf8string.md)
- [Char](char.md)
- [Format](format.md)
- [言語リファレンスの目次](../index.md)
