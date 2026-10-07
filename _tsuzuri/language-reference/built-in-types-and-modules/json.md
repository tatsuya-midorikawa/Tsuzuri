# Json

`Json` は、JSON（[RFC 8259](https://www.rfc-editor.org/rfc/rfc8259)）の解析と出力、および Tsuzuri の値と JSON の相互変換をまとめた標準モジュールです。JSON の値は共用体 `Json.Value` で表し、レコードや共用体は `deriving (Encode, Decode)` で JSON と変換できるようになります。

Web ホストや設定ファイルとのあいだで、スカラーより複雑な値を受け渡すための標準の経路です。解析は RFC 8259 に厳密で、上限を超える入力も含めて失敗は `Result` の `Error` で返し、トラップやスタックの枯渇を起こしません。出力は決定的で、ネイティブと WebAssembly、`-O0` と `-O3` で同じバイト列になります。

## この記事のポイント

- `Json.parse` は `ref utf8string` を `Result<Json.Value, Json.Error>` へ解析し、`Json.to_utf8string` は空白なしの JSON テキストを書きます。
- 数値は字句をそのまま保つ `Json.Numeral` です。`2^53` を超える整数や `1.0E+2` の書き方も失われません。
- 組み込み型クラス `Encode` / `Decode` が値と `Json.Value` を変換します。`Json.serialize` / `Json.deserialize` はテキストとのあいだを一度に変換します。
- `deriving (Encode, Decode)` はレコードを object、共用体を `"Case"` / `{"Case": payload}` にします。
- 入力は 64 MiB、入れ子は 128 段まで。エラーは種類とバイト位置を持つ `Json.Error` です。

## 値の型

```text
record Numeral { text: utf8string }
union Value = Null | Bool of bool | Number of Numeral | Text of string | Items of [Value] | Object of [(string * Value)]
union ErrorKind = Syntax | UnexpectedEnd | InvalidEscape | ControlCharacter | DuplicateKey of string | TooDeep | TooLarge
                | NonFinite | NumberRange | ExpectedType of string | MissingField of string | UnknownCase of string | LoneSurrogate
record Error { kind: ErrorKind, offset: i64 }
```

| JSON | `Json.Value` |
| --- | --- |
| `null` | `Null` |
| `true` / `false` | `Bool` |
| 数値 | `Number`。`Numeral.text` は RFC 8259 の number の字句 |
| 文字列 | `Text`。UTF-16 の `string` で、孤立サロゲートも保持する |
| 配列 | `Items`。`Array` は組み込みの型名なので case 名に使えません |
| object | `Object`。メンバーは入力の順に並ぶ `(キー * 値)` の配列 |

`Value`・`Numeral`・`ErrorKind`・`Error` は `Eq` を導出しています。数値の比較は字句の比較なので、`1.0` と `1` は等しくありません。case 名は無修飾でも見えますが、自分のモジュールに同じ名前があればそちらが優先されます。迷うときは `Json.Null` のように修飾します。

## 解析と出力

```tsuzuri run=%5B1%2C2.50%2C%7B%22a%22%3A%5Btrue%2Cnull%5D%7D%2C%22x%F0%9F%98%80%22%5D
let text = u8"[1, 2.50, {\"a\": [true, null]}, \"x\\ud83d\\ude00\"]"
match Json.parse (ref text) with
| Ok value -> Json.to_utf8string (ref value)
| Error error -> Utf8String.from_string (ref (Display.display (ref error)))
```

実行結果:

```text
[1,2.50,{"a":[true,null]},"x😀"]
```

`parse` は RFC 8259 に厳密です。

- 空白は空白・タブ・LF・CR だけです。最上位には任意の値（`1` や `"a"` も）を書けます。
- 先頭 BOM、コメント、末尾のカンマ、`NaN`、`Infinity`、先頭ゼロ（`01`）、`+1`、`.5`、`1.`、`TRUE` は `Syntax` です。値の後に空白以外があっても `Syntax` です。
- 文字列中の 0x00–0x1F の生のバイトは `ControlCharacter`、`\"` `\\` `\/` `\b` `\f` `\n` `\r` `\t` `\uXXXX`（16 進は大文字・小文字とも）以外のエスケープは `InvalidEscape` です。
- `\uD83D\uDE00` の組は 1 文字になり、組にならない `\uD800` はそのまま `Text` に入ります（`JSON.parse` と同じ）。
- 同じ object の中で、エスケープを解いた後のキーが重なると `DuplicateKey` です（`"a"` と `"\u0061"` も重なります）。

`to_utf8string` は空白を出さず、キーを格納順に、数値を `Numeral.text` のまま書きます。文字列は `"` → `\"`、`\` → `\\`、U+0008 → `\b`、U+000C → `\f`、U+000A → `\n`、U+000D → `\r`、U+0009 → `\t`、その他の U+0000–U+001F は `\u00xx`、孤立サロゲートは `\udxxx`（16 進は小文字）にし、それ以外（`/`、U+007F、U+2028 を含む）は UTF-8 のまま書きます。ECMAScript 2019 以降の `JSON.stringify` の文字列と同じバイト列です。利用者が組み立てた `Object` の重複キーは検査せずにそのまま書きます。

## エラー

```tsuzuri run=json%3A%20duplicate%20key%20%22a%22%20at%20byte%207%20%7C%20json%3A%20syntax%20error%20at%20byte%205
def describe :: utf8string -> string
fn describe text =
    match Json.parse (ref text) with
    | Ok _ -> "ok"
    | Error error -> Display.display (ref error)

$"{describe u8"{\"a\":1,\"a\":2}"} | {describe u8"[1,2,]"}"
```

実行結果:

```text
json: duplicate key "a" at byte 7 | json: syntax error at byte 5
```

`offset` は入力の先頭からのバイト位置で、入力を不正にした最初のバイトを指します。`UnexpectedEnd` は入力の長さ、`TooLarge` は 0、`TooDeep` は 129 段目を開く `[` / `{`、`DuplicateKey` は重なったキーの開きの `"` です。一つの入力に複数の誤りがあれば、いちばん前の位置のものを返します。`encode`・`decode`・数値の変換の `Error` の `offset` は -1 です。`Display` は `json: <種類>` に、`offset` が 0 以上なら ` at byte <offset>` を付けます。

| 種類 | 条件 | 表示 |
| --- | --- | --- |
| `Syntax` | 文法の違反、BOM、値の後の余分な文字 | `syntax error` |
| `UnexpectedEnd` | 値の途中で入力が終わる、空の入力 | `unexpected end of input` |
| `InvalidEscape` | 定義されていないエスケープ、`\u` の後が 16 進 4 桁でない | `invalid escape` |
| `ControlCharacter` | 文字列中の 0x00–0x1F | `control character in string` |
| `DuplicateKey k` | 同じ object の重複キー | `duplicate key "k"`（k は `JSON.stringify` と同じ規則で引用） |
| `TooDeep` | 入れ子が 128 段を超える | `nesting deeper than 128` |
| `TooLarge` | 入力が 64 MiB を超える | `input larger than 64 MiB` |
| `NonFinite` | NaN・無限大の encode | `non-finite number` |
| `NumberRange` | 型の範囲外、無限大への overflow、4096 バイトを超える字句 | `number out of range` |
| `ExpectedType t` | JSON の種類が違う | `expected t` |
| `MissingField f` | 必須のフィールドがない | `missing field "f"` |
| `UnknownCase c` | 共用体にない case 名 | `unknown case "c"` |
| `LoneSurrogate` | `utf8string` へ decode する文字列に孤立サロゲートがある | `lone surrogate in string` |

## 数値

`Numeral.text` は常に RFC 8259 の number の字句です。`parse` は入力の字句をそのまま保持し、`Json.numeral` は字句を検証して複製します。`Numeral { text: ... }` を直接書いて不正な字句を作ると、`to_utf8string` がトラップします。

```tsuzuri run=9007199254740993%20NumberRange%201e%2B400%20ok
let big = Maybe.get (Json.numeral (ref u8"9007199254740993"))
let huge = Maybe.get (Json.numeral (ref u8"1e+400"))
let exact = match Json.to_i64 (ref big) with
    | Ok value -> to_string value
    | Error _ -> "error"
let range = match Json.to_f64 (ref huge) with
    | Ok _ -> "finite"
    | Error error -> if error.kind == Json.NumberRange then "NumberRange" else "other"
let invalid = if Maybe.is_none (ref (Json.numeral (ref u8"01"))) then "ok" else "accepted"
$"{exact} {range} {String.from_utf8 (ref huge.text)} {invalid}"
```

実行結果:

```text
9007199254740993 NumberRange 1e+400 ok
```

- 整数型への変換（`to_i64` と整数の `Decode`）は、`.`・`e`・`E` を含む字句を `ExpectedType "integer"`、範囲外と 4096 バイトを超える字句を `NumberRange` にします。`-0` は 0 です。
- 浮動小数点への変換は字句から目的の型へ最近接・偶数丸めで一度だけ丸めます（`f32` も `f64` を経由しません）。無限大になる overflow は `NumberRange`、非正規化数や符号付きゼロへの underflow は成功です。
- 変換は字句の検査の後で `Parse.parse` を一度だけ呼びます。ホストのロケールや C の `strtod` は使いません。

## Encode と Decode

組み込み型クラスです。インスタンスは `Json` モジュールと利用者のソースにだけあります。

```text
Encode<'a> { encode :: ref 'a -> Result<Json.Value, Json.Error> }
Decode<'a> { decode :: ref Json.Value -> Result<'a, Json.Error> }
```

| Tsuzuri の型 | encode | decode が受理するもの |
| --- | --- | --- |
| `bool` | `true` / `false` | JSON の真偽値 |
| `i8`–`i128`、`i8u`–`i128u` | 10 進の整数 | 整数の字句で範囲内 |
| `f32`、`f64` | `to_string` の最短表現。NaN・無限大は `NonFinite` | 任意の数値 |
| `string` | 文字列 | 文字列 |
| `utf8string` | 文字列 | 文字列。孤立サロゲートを含めば `LoneSurrogate` |
| `Maybe<'a>` | `None` は `null`、`Some x` は x | `null` は `None`、それ以外は `Some` |
| `['a]`、`[\|'a\|]`、`Vec<'a>` | 配列 | 配列 |
| 2–4 要素のタプル | 同じ長さの配列 | 同じ長さの配列（違えば `ExpectedType "array of N"`） |
| `Json.Value` | 複製 | 複製 |
| 導出したレコード | object。キーはフィールド名で宣言順 | object。余分なキーは無視。キーが無ければ `null` として decode し、失敗すれば `MissingField` |
| 導出した共用体 | ペイロードなしは `"Case"`、ありは `{"Case": payload}`（複数のペイロードはタプルなので配列） | 同じ形だけ。未知の名前は `UnknownCase`、形が違えば `ExpectedType` |

`Maybe<Maybe<'a>>` の `Some None` は `null` になり、decode では `None` に戻ります（serde と同じく往復しません）。数値は `to_string` の最短表現なので、`-0.0` は `-0`、`10000000.0` は `1e+7` です（`JSON.stringify` は `0` と `10000000`）。インスタンスのない型（`char`、`utf8char`、`unit`、関数、`Task`、5 要素以上のタプルなど）を直接使うと `E1005` です。

```tsuzuri run=%7B%22name%22%3A%22a%22%2C%22points%22%3A%5B%7B%22x%22%3A1%2C%22y%22%3A0.5%7D%5D%2C%22shapes%22%3A%5B%7B%22Circle%22%3A2%7D%2C%7B%22Rect%22%3A%5B1%2C2.5%5D%7D%2C%22Empty%22%5D%2C%22note%22%3Anull%7D%20round%3Dtrue
record Point { x: i64, y: f64 } deriving (Encode, Decode)
union Shape = Circle of f64 | Rect of f64 * f64 | Empty deriving (Encode, Decode)
record Scene { name: string, points: [Point], shapes: Vec<Shape>, note: Maybe<string> } deriving (Encode, Decode)

let shapes = Vec.push (Vec.push (Vec.push (Vec.empty()) (Circle 2.0)) (Rect (1.0, 2.5))) Empty
let scene = Scene { name: "a", points: [Point { x: 1, y: 0.5 }], shapes: shapes, note: None }
let text = Result.get (Json.serialize (ref scene))
let back: Result<Scene, Json.Error> = Json.deserialize (ref text)
let round = match back with
    | Ok copy -> Result.get (Json.serialize (ref copy)) == text
    | Error _ -> false
let shown = String.from_utf8 (ref text)
$"{shown} round={round}"
```

実行結果:

```text
{"name":"a","points":[{"x":1,"y":0.5}],"shapes":[{"Circle":2},{"Rect":[1,2.5]},"Empty"],"note":null} round=true
```

### decode の規則

```tsuzuri run=json%3A%20missing%20field%20%22y%22%20%7C%20ok%20%7C%20json%3A%20expected%20integer%20%7C%20json%3A%20unknown%20case%20%22Square%22%20%7C%20json%3A%20expected%20object
record Point { x: i64, y: i64 } deriving (Decode)
union Shape = Circle of f64 | Empty deriving (Decode)

def point :: utf8string -> string
fn point text =
    let result: Result<Point, Json.Error> = Json.deserialize (ref text)
    match result with
    | Ok _ -> "ok"
    | Error error -> Display.display (ref error)

def shape :: utf8string -> string
fn shape text =
    let result: Result<Shape, Json.Error> = Json.deserialize (ref text)
    match result with
    | Ok _ -> "ok"
    | Error error -> Display.display (ref error)

$"{point u8"{\"x\":1}"} | {point u8"{\"x\":1,\"y\":2,\"z\":3}"} | {point u8"{\"x\":1.5,\"y\":2}"} | {shape u8"{\"Square\":1}"} | {shape u8"\"Circle\""}"
```

実行結果:

```text
json: missing field "y" | ok | json: expected integer | json: unknown case "Square" | json: expected object
```

- 導出したレコードの decode は全フィールドを宣言順に decode し、宣言順で最初の `Error` を返します。成功した他のフィールドは解放します。
- 導出したレコードの encode はフィールドを宣言順に encode し、最初の `Error` を返して残りを encode しません。
- 共用体の decode では、ペイロードのある case を `"Case"` と書くと `ExpectedType "object"`、ペイロードのない case を `{"Case": ...}` と書くと `ExpectedType "string"`、メンバーが 1 つでない object は `ExpectedType "object with one member"`、それ以外の種類は `ExpectedType "string or object"` です。

### 手書きのインスタンス

`instance Encode<型>` / `instance Decode<型>` は普通のインスタンスとして書けます。`Json` に既にある型（`instance Encode<i64>` など）と重なると `E1016` です。

```tsuzuri run=1.5%20%7C%20meters%3D2.5
record Meters { value: f64 }

instance Encode<Meters> {
    fn encode meters = Encode.encode (ref meters.value)
}

instance Decode<Meters> {
    fn decode json = Result.map (\value -> Meters { value: value }) (Decode.decode json)
}

let text = Result.get (Json.serialize (ref (Meters { value: 1.5 })))
let back: Result<Meters, Json.Error> = Json.deserialize (ref u8"2.5")
let meters = Result.get back
$"{String.from_utf8 (ref text)} | meters={meters.value}"
```

実行結果:

```text
1.5 | meters=2.5
```

成分は `Encode.encode` / `Decode.decode` で具体的な型のまま呼びます。`Json.encode`・`Json.decode`・`Json.serialize` のような多相の関数は、どのインスタンスにも届きうる呼び出しとして再帰の検査に数えられるので、それを呼ぶメソッドには `fn rec` が必要です（無いと `E1019`）。導出コードが使う補助関数（`begin_object`、`encode_field`、`end_object`、`encode_case`、`encode_tag`、`expect_object`、`decode_field`、`keep_error`、`case_index`、`decode_payload`）も公開しているので、`fn rec` を付けた手書きのインスタンスから使えます。

## 資源上限と計算量

- 入力は 64 MiB（67,108,864 バイト）まで。超えると字句を読む前に `TooLarge` です。
- 配列と object の入れ子は 128 段まで（`[[1]]` は 2 段）。解析は深さを数える再帰下降で、再帰の前に深さを検査するので、1,000,000 個の `[` でもスタックは 128 段で止まります。
- 数値の変換は 4096 バイトの字句までです（`Parse.parse` の上限）。
- 重複キーの検査は、32 メンバー以下の object では線形、それを超えるとキーを複製して整列し、隣どうしを比べます（$O(n \log n)$）。
- 導出した decode のフィールド探索は線形（フィールド数 × メンバー数）です。
- `to_utf8string`、`encode`、`decode` は値の深さだけ再帰します。解析した値の深さは 128 以下ですが、利用者が組み立てた深い値ではスタックを使います（導出した `Display` や `Hash` と同じ）。

## 所有権と借用

- `parse`・`to_utf8string`・`encode`・`decode`・`serialize`・`deserialize` は引数を共有借用し、新しい所有値を返します。文字列は複製し、入力を変えません。
- `Error` の経路でも途中の値はすべて解放します。トラップするのは不正な `Numeral` の出力と確保の失敗だけです。

## API リファレンス

すべての関数は `std::Json` モジュールに属しています。

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `parse` | `ref utf8string -> Result<Value, Error>` | JSON テキストを解析します |
| `to_utf8string` | `ref Value -> utf8string` | 空白なしの JSON テキストを書きます |
| `numeral` | `ref utf8string -> Maybe<Numeral>` | 字句が number なら複製して `Some` |
| `to_i64` | `ref Numeral -> Result<i64, Error>` | 整数の字句を `i64` に |
| `to_f64` | `ref Numeral -> Result<f64, Error>` | 字句を一度だけ丸めて `f64` に |
| `encode` | `Encode<'a> => ref 'a -> Result<Value, Error>` | 値を `Value` に |
| `decode` | `Decode<'a> => ref Value -> Result<'a, Error>` | `Value` を値に |
| `serialize` | `Encode<'a> => ref 'a -> Result<utf8string, Error>` | 値を JSON テキストに |
| `deserialize` | `Decode<'a> => ref utf8string -> Result<'a, Error>` | JSON テキストを値に |

導出コード用の補助関数:

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `begin_object` | `i64 -> Result<Vec<(string * Value)>, Error>` | 容量付きの空のメンバー列 |
| `encode_field` | `Encode<'a> => Result<Vec<(string * Value)>, Error> -> string -> ref 'a -> Result<Vec<(string * Value)>, Error>` | Error なら何もせず返し、Ok なら値を encode して足す |
| `end_object` | `Result<Vec<(string * Value)>, Error> -> Result<Value, Error>` | メンバー列を `Object` に |
| `encode_case` | `Encode<'a> => string -> ref 'a -> Result<Value, Error>` | `{"Case": payload}` |
| `encode_tag` | `string -> Result<Value, Error>` | `"Case"` |
| `expect_object` | `ref Value -> Result<unit, Error>` | object でなければ `ExpectedType "object"` |
| `decode_field` | `Decode<'a> => ref Value -> string -> Result<'a, Error>` | メンバーを線形に探して decode。無ければ `null` を decode し、失敗なら `MissingField` |
| `keep_error` | `Maybe<Error> -> Result<'a, Error> -> Maybe<Error>` | 最初の Error を残し、残りを解放 |
| `case_index` | `ref Value -> ref [string] -> ref [bool] -> Result<i64, Error>` | case 名とペイロードの有無から添字を求め、形を検査 |
| `decode_payload` | `Decode<'a> => ref Value -> Result<'a, Error>` | `case_index` の成功後に、ただ一つのメンバーの値を decode |

## 他の言語との比較

| 観点 | Tsuzuri `Json` | Rust serde_json | C# System.Text.Json |
| --- | --- | --- | --- |
| 数値 | 字句を保つ `Numeral` | `Number`（既定は f64 / i64 / u64） | `JsonElement` の生テキスト、型付きは double など |
| 重複キー | `DuplicateKey` で拒否 | 後勝ち（既定） | 後勝ち |
| NaN・無限大の出力 | `NonFinite` | エラー | 既定はエラー |
| 共用体の形 | `"Case"` / `{"Case": payload}` | 同じ（外部タグ、既定） | 既定ではなし |
| 導出 | `deriving (Encode, Decode)` | `#[derive(Serialize, Deserialize)]` | ソース生成器・リフレクション |

## まとめ

- `Json.parse` と `Json.to_utf8string` は RFC 8259 に厳密で、決定的なバイト列を書きます。
- 数値は字句を保つ `Numeral`、object はメンバーの順を保つ配列です。
- `Encode` / `Decode` と `deriving (Encode, Decode)` で、レコード・共用体・コレクションを JSON と変換します。
- 失敗はすべて `Json.Error`（種類とバイト位置）で返り、上限（64 MiB、128 段）を超える入力もトラップしません。

## 関連項目

- [型クラス](../types-and-type-inference/type-classes.md) — `deriving` と組み込み型クラス
- [Utf8String](./utf8string.md) — 入力と出力の型
- [Format](./format.md) — `Display` と `Parse`
- [Result](./result.md) — 失敗の扱い
- [言語リファレンスの目次](../index.md)
