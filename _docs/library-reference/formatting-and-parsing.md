# 表示、解析、文字列化

[ドキュメントのトップ](../README.md)

Display は値から UTF-16 string を作り、Parse は string から値を解析します。locale に依存しない共通実装を使い、native と WASM で同じ表現になります。

## Display と to_string

```tsuzuri run=42
let text = to_string 42
let parsed: Option<i64> = Parse.parse ref text
Option.get parsed
```

| API | 所有権 |
| --- | --- |
| `Display.display value` | `Display<T> => ref T -> string`。入力を借用 |
| `to_string value` | `Display<T> => T -> string`。入力を消費 |
| `Parse.parse text` | `Parse<T> => ref string -> Option<T>`。入力を借用 |

string の to_string は元のバッファをそのまま返せます。借用版 Display は独立した複製を返します。utf8string の表示は UTF-16 への変換を行います。単体の文字列の NUL、改行、引用符はそのままで、JSON などのエスケープを追加しません。

## 数値の表示形式

整数は符号と十進数字です。binary 浮動小数点は同じ型へ解析し直したときに元の bit へ戻る最短の有効桁数を使います。同じ桁数なら近い十進数、等距離なら偶数の十進仮数を選びます。

| 値 | 表示 |
| --- | --- |
| `0.1`, `0.1f32` | `0.1` |
| `1.0` | `1` |
| 正負のゼロ | `0`, `-0` |
| 無限大 | `inf`, `-inf` |
| NaN | `nan` |
| `0.10d128` | `0.1` |

decimal の末尾の quantum ゼロ、NaN の符号・payload は表示・解析の往復では保持しません。

正規化した十進指数が -6 未満、または有効桁数 + 6 以上なら指数表記です。指数は小文字 e、必須の符号、先頭ゼロなしで表します。`1000000.0` は `1000000`、`10000000.0` は `1e+7` です。

## 解析文法

| 目標型 | 受理する入力 |
| --- | --- |
| 整数 | 任意の符号、十進数字、小文字 prefix の `0x` / `0b` |
| 浮動小数点 | 任意の符号、十進の小数・指数。`1.`, `.5`, `1e3`, `1E-3` も可 |
| 浮動小数点の特殊値 | 小文字 `inf`, `nan` と任意の符号 |
| bool | 厳密に `true` / `false` |
| char | ちょうど一 UTF-16 コード単位 |
| utf8char | ちょうど一 Unicode スカラー |

数値の `_` は桁と桁の間だけです。前後空白、空文字、途中までの解析、大文字の `0X` / `0B`、`INF` / `NaN`、hex float は認めません。

符号なし整数の負号は `-0` でも失敗です。範囲外整数、有限入力から無限大になる浮動小数点 overflow は None です。subnormal と符号付きゼロへの underflow は成功します。

数値解析は 4,096 UTF-16 コード単位まで、ASCII 数字・記号のみです。文字解析の Unicode 規則とは別です。解析失敗は診断やトラップではなく None になります。

## 独自型

```tsuzuri run=42
record Count { value: i64 }

instance Display<Count> {
    fn display count = to_string count.value
}

instance Parse<Count> {
    fn parse text =
        let number: Option<i64> = Parse.parse text
        match number with
        | Option.Some value -> Option.Some (Count { value: value })
        | Option.None -> Option.None
}

let text = "42"
let parsed: Option<Count> = Parse.parse ref text
to_string (Option.get parsed)
```

インスタンスの選択はコンパイル時です。組み込みのインスタンスは上書きできません。Display の自動導出は使えますが、任意の型の Parse 自動導出はありません。

## コンソールと制限

コンソール用ホストは同じ表示内容を UTF-8 で出力し、末尾に改行を付けます。ただし unit のコンソール結果は無出力です。単体 Display の unit は `()` です。

孤立サロゲートを持つ string / char は内部で保持できますが、UTF-8 出力時にはトラップします。明示的な修復が必要なら String.to_well_formed を使います。

文字列補間、printf 風の書式文字列、桁数指定、locale 書式、Parse の unit / string インスタンスは提供しません。

## 関連項目

- [文字列 API](text.md)
- [Display の自動導出](../language-reference/deriving.md)
- [型クラス](../language-reference/generics-and-typeclasses.md)
