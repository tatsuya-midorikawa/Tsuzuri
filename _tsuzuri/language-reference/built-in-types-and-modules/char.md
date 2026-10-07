# Char

`Char` は、UTF-16 の 1 コード単位を表す `char` 型の判定や変換を行う標準モジュールです。`char` は Copy 型のため、関数に値を渡しても元の変数は失われません。このモジュールの関数はいずれも失敗せず、$O(1)$ で高速に動作します。

文字リテラルの記法や、Unicode スカラー値を表す `utf8char` との違いは [文字列](../literals-and-strings/strings.md) を参照してください。

## この記事のポイント

- `char` は 0〜65,535 の範囲の UTF-16 コード単位です。サロゲートコード単位もそのまま値として保持できます。
- 判定と大小文字変換は ASCII 範囲のみを対象とします。ひらがなやアクセント付きラテン文字などの変換は行いません。
- `to_u16` と `of_u16` は、65,536 通りすべての値で完全な可逆変換を保証します。
- 算術演算や、`as` による整数への直接キャストはできません。数値として扱いたいときは `to_u16` を使用します。
- 孤立サロゲートをコンソール標準出力へ書き出すと、UTF-8 エンコーディング変換でトラップします。

## コード単位との変換

| 関数 | シグネチャ | 所有権 | 失敗 | 計算量 |
| --- | --- | --- | --- | --- |
| `to_u16` | `char -> i16u` | Copy（元の値は維持） | しない | O(1) |
| `of_u16` | `i16u -> char` | Copy | しない（全 65,536 通りで有効） | O(1) |

サロゲートのコード単位も、この 2 つの関数で安全に往復できます。サロゲートをそのまま標準出力へ書き出すとトラップするため、サロゲート判定や比較は数値のまま行うか、`string` に変換して行います。

```tsuzuri run=code%3D65%20back%3DA%20surrogate%3D55296
let code = Char.to_u16 'A'
let back = Char.of_u16 code
let surrogate = Char.to_u16 (Char.of_u16 55296i16u)
$"code={code} back={back} surrogate={surrogate}"
```

実行結果:

```text
code=65 back=A surrogate=55296
```

`back` が `A` なのは、`char` の `Display` がその文字そのものだからです。配列の要素として表示するときは、引用符付きの `'A'` になります。

## ASCII の判定と変換

すべて `char` を引数に取り、失敗せず O(1) で動作します。

| 関数 | シグネチャ | 真になる範囲、または変換 |
| --- | --- | --- |
| `is_ascii_digit` | `char -> bool` | `'0'`〜`'9'` |
| `is_ascii_lower` | `char -> bool` | `'a'`〜`'z'` |
| `is_ascii_upper` | `char -> bool` | `'A'`〜`'Z'` |
| `is_ascii_alphabetic` | `char -> bool` | 上の小文字または大文字 |
| `to_ascii_lower` | `char -> char` | `'A'`〜`'Z'` を小文字に変換（他はそのまま保持） |
| `to_ascii_upper` | `char -> char` | `'a'`〜`'z'` を大文字に変換（他はそのまま保持） |

Unicode の文字カテゴリは参照しません。全角数字、アクセント付きラテン文字、ひらがなは、英字判定が `false` となり、大小文字変換も行われません。

```tsuzuri run=digit%3Dtrue%20lower%3Dz%20kana%3D%E3%81%82
let kana = Char.of_u16 0x3042i16u
$"digit={Char.is_ascii_digit '7'} lower={Char.to_ascii_lower 'Z'} kana={Char.to_ascii_lower kana}"
```

実行結果:

```text
digit=true lower=z kana=あ
```

## 比較と表示

`char` は Eq と Ord 型クラスを実装しています。ソート順序は符号なし 16 bit の数値順です。`Numeric` は実装していないため、`+` や `-` による算術演算はできません。

`Display` による文字列化は常に 1 コード単位の `string` です。`Parse.parse` は、入力が厳密に 1 コード単位の `string` の場合のみ `Some` を返します。空文字列や 2 コード単位以上の文字列は安全に `None` となり、解析失敗でトラップすることはありません。型推論のために型注釈が必要です。

```tsuzuri run=A
let parsed: Maybe<char> = Parse.parse (ref "A")
match parsed with
| Some value -> value
| None -> '?'
```

実行結果:

```text
A
```

2 コード単位以上の文字列や空文字列に対しては `None` を返します。

サロゲートコード単位を `Display` で文字列化した場合、UTF-16 コード単位自体は保持されますが、その文字列をコンソール標準出力へ書き出すと UTF-8 変換エラーによってトラップします。数値として検査・比較したいときは `to_u16` を使用してください。

## 注意点

> [!WARNING]
> `string` の添字 `text[index]` は `char` ではなく `i16u` です。`Char.of_u16 text[index]` で文字にします。範囲外の添字は、このモジュールではなく添字の側でトラップします。

> [!NOTE]
> 補助平面の絵文字を 1 つの文字として持ちたいときは `char` には入りません。[Utf8Char](utf8char.md) の `utf8char` を使います。

## まとめ

- `of_u16` と `to_u16` は全コード単位で往復できます。
- ASCII の数字と英字だけを判定し、英字だけ大小を変えます。
- 値は Copy で、これらの関数は失敗しません。
- サロゲートは値として持てますが、UTF-8 の出力ではトラップします。

## 関連項目

- [文字列](../literals-and-strings/strings.md)
- [Utf8Char](utf8char.md)
- [String](string.md)
- [Format](format.md)
- [言語リファレンスの目次](../index.md)
