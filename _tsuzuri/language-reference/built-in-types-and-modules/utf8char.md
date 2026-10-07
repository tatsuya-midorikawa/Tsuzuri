# Utf8Char

`Utf8Char` は、Unicode スカラー値 1 個を表す `utf8char` 型の判定や変換を行う標準モジュールです。絵文字などのサロゲートペアを要する文字も 1 つの値として保持できます（`char` 型には収まりません）。

`utf8char` は Copy 型です。判定と ASCII 変換は引数を値渡ししても元の値は維持されます。不正なスカラー値を拒否するのは `of_u32` と `of_u32_unchecked` です。

## この記事のポイント

- 値は U+0000〜U+10FFFF から、サロゲート領域 U+D800〜U+DFFF を除外した妥当な Unicode スカラー値です。
- `of_u32` は不正値に対して `None` を返します。`of_u32_unchecked` は同じ不正値で安全にトラップします。
- ASCII の判定と大小文字変換は [Char](char.md) と同一の範囲です。非 ASCII 文字は変換しません。
- `Display` による文字列化の結果は UTF-16 の `string` です。補助平面の文字はサロゲートペア（2 コード単位）になります。
- `Parse` は、厳密に 1 スカラー値で構成された `string` に対してのみ成功します。

## スカラーとの変換

| 関数 | シグネチャ | 失敗 | 計算量 |
| --- | --- | --- | --- |
| `to_u32` | `utf8char -> i32u` | しない。常に妥当なスカラー | O(1) |
| `of_u32` | `i32u -> Maybe<utf8char>` | サロゲート、または U+10FFFF を超えると `None` | O(1) |
| `of_u32_unchecked` | `i32u -> utf8char` | 同じ不正値でトラップ。黙って通さない | O(1) |

`unchecked` は検査を省く関数ではありません。呼び出し側が妥当だと分かっているときの高速経路で、不正なら安全にトラップします。入力が外部から来るときは `of_u32` を使ってください。

```tsuzuri run=scalar%3D128512%20bad%3Dnone%20good%3D128512
let scalar = Utf8Char.to_u32 u8'😀'
let bad = Utf8Char.of_u32 0xD800i32u
let good = Utf8Char.of_u32 scalar
let bad_text = match bad with
    | Some _ -> "some"
    | None -> "none"
let good_text = match good with
    | Some value -> to_string (Utf8Char.to_u32 value)
    | None -> "none"
$"scalar={scalar} bad={bad_text} good={good_text}"
```

実行結果:

```text
scalar=128512 bad=none good=128512
```

U+1F600 は 128512 です。サロゲート 0xD800 は `None` です。

## ASCII の判定と変換

すべて `utf8char` を引数に取り、失敗せず O(1) で動作します。判定・変換の対象は ASCII 範囲のみです。

| 関数 | シグネチャ | 真になる範囲、または変換 |
| --- | --- | --- |
| `is_ascii_digit` | `utf8char -> bool` | `u8'0'`〜`u8'9'` |
| `is_ascii_lower` | `utf8char -> bool` | `u8'a'`〜`u8'z'` |
| `is_ascii_upper` | `utf8char -> bool` | `u8'A'`〜`u8'Z'` |
| `is_ascii_alphabetic` | `utf8char -> bool` | 上の小文字または大文字 |
| `to_ascii_lower` | `utf8char -> utf8char` | `u8'A'`〜`u8'Z'` を小文字に変換（他はそのまま保持） |
| `to_ascii_upper` | `utf8char -> utf8char` | `u8'a'`〜`u8'z'` を大文字に変換（他はそのまま保持） |

変換は `of_u32_unchecked` を通します。ASCII の英字に 32 を足し引きした結果は常に妥当なスカラーなので、ここではトラップしません。絵文字やひらがなはそのまま返します。

```tsuzuri run=z
Utf8Char.to_ascii_lower u8'Z'
```

実行結果:

```text
z
```

## 比較、表示、解析

Eq と Ord の順序は、スカラー値の数値順です。UTF-8 のバイト順とも一致します。`Numeric` ではないので加減算はできません。整数が欲しいときは `to_u32` です。

`Display` の結果は `string` です。BMP の文字は 1 コード単位、補助平面はサロゲートペア 2 コード単位です。`u8'😀'` を表示すると絵文字 1 つに見えますが、その `string` の `.length` は 2 です。

`Parse.parse` は、入力の `string` がちょうど 1 スカラーのときだけ `Some` です。空、複数スカラー、孤立サロゲートだけの文字列は `None` です。トラップはしません。

```tsuzuri run=128512
let parsed: Maybe<utf8char> = Parse.parse (ref "😀")
match parsed with
| Some value -> Utf8Char.to_u32 value
| None -> 0i32u
```

実行結果:

```text
128512
```

## 注意点

> [!WARNING]
> `utf8string` の添字は `utf8char` ではなく `ubyte` です。1 スカラー取り出すには `Utf8String.decode_at` を使います。途中バイトを渡すとトラップします。

> [!TIP]
> 外部の `i32u` を文字にするときは `of_u32` です。`of_u32_unchecked` は、すでに検査済みの値を戻すときに限ってください。

## まとめ

- `utf8char` はサロゲートを含まないスカラー 1 個です。
- `of_u32` は不正なら `None`、`of_u32_unchecked` はトラップします。
- ASCII 以外の大小は変換しません。
- 表示結果の `string` は、補助平面だと 2 コード単位です。

## 関連項目

- [文字列](../literals-and-strings/strings.md)
- [Char](char.md)
- [Utf8String](utf8string.md)
- [Format](format.md)
- [言語リファレンスの目次](../index.md)
