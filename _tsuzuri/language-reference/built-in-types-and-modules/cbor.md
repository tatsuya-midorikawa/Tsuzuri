# Cbor

`Cbor` は、[Json](./json.md) と同じ値 `Json.Value` を CBOR（[RFC 8949](https://www.rfc-editor.org/rfc/rfc8949)）のバイト列として読み書きする標準モジュールです。`Encode` / `Decode` のインスタンスと `deriving (Encode, Decode)` は JSON と共通なので、型を一度導出すれば JSON と CBOR の両方へ変換できます。

## この記事のポイント

- `Cbor.encode` は `Json.Value` を RFC 8949 §4.2.1 の決定的な符号化（core deterministic encoding）で書きます。同じ値は常に同じバイト列です。
- `Cbor.decode` は JSON のデータモデルに入る CBOR を厳密に読み、それ以外は `Result` の `Error` で返します。トラップしません。
- `Cbor.serialize` / `Cbor.deserialize` は `Encode` / `Decode` を通して値とバイト列を一度に変換します。
- エラーは `Json.Error` で、decode の `offset` は、入力が途中で終わった `UnexpectedEnd` では入力の長さ（終わった位置）、ほかのエラーでは問題の項目の先頭のバイト位置です。

## 符号化

```tsuzuri run=a26161016162820203%20f93e00%20fb3ff199999999999a%20c249010000000000000000
def hex :: ref [ubyte] -> string
fn hex bytes =
    let digits = "0123456789abcdef"
    let mut units = Vec.empty()
    for byte in bytes do
        units = Vec.push (Vec.push units digits[(byte >>> 4ubyte) as i64]) digits[(byte &&& 15ubyte) as i64]
    String.from_code_units (Vec.to_array units)

def cbor :: utf8string -> string
fn cbor text =
    let value = Result.get (Json.parse (ref text))
    hex (ref (Result.get (Cbor.encode (ref value))))

$"{cbor u8"{\"b\": [2, 3], \"a\": 1}"} {cbor u8"1.5"} {cbor u8"1.1"} {cbor u8"18446744073709551616"}"
```

実行結果:

```text
a26161016162820203 f93e00 fb3ff199999999999a c249010000000000000000
```

| `Json.Value` | CBOR |
| --- | --- |
| `Null` / `Bool` | 単純値 `null`（0xf6）/ `false`（0xf4）・`true`（0xf5） |
| 小数点も指数もない数値 | 整数（major type 0・1）。64 ビットを超える大きさは bignum（タグ 2・3）。`-0` だけは半精度の -0.0 |
| それ以外の数値 | 字句を `f64` へ一度だけ丸めた値を、値が変わらない最短の浮動小数点（半精度・単精度・倍精度）で。`f64` で無限大になれば `NumberRange` |
| `Text` | テキスト文字列（UTF-8）。孤立サロゲートを含めば `LoneSurrogate` |
| `Items` | 長さを前置した配列 |
| `Object` | 長さを前置したマップ。キーは符号化したバイト列の辞書順に並べ替え、重複キーは `DuplicateKey` |

- 引数（長さ・整数）は常に最短の形で、長さ不定の項目は使いません。これが RFC 8949 §4.2.1 の決定的な符号化です。
- 数値は値で保存するので、JSON の字句の書き方（`1.0` と `1`、`1e2` と `100.0`）は残りません。`1.0` は半精度の 1.0、`1e2` は半精度の 100.0 です。整数の字句は整数のままです。

## 復号

```tsuzuri run=%7B%22a%22%3A1%2C%22b%22%3A%5B2%2C3%5D%7D%20%7C%20json%3A%20unsupported%20byte%20string%20at%20byte%200%20%7C%20json%3A%20unexpected%20end%20of%20input%20at%20byte%201
def decode :: [ubyte] -> string
fn decode bytes =
    match Cbor.decode (ref bytes) with
    | Ok value -> String.from_utf8 (ref (Json.to_utf8string (ref value)))
    | Error error -> Display.display (ref error)

let map = new [162ubyte, 97ubyte, 97ubyte, 1ubyte, 97ubyte, 98ubyte, 130ubyte, 2ubyte, 3ubyte]
$"{decode map} | {decode (new [64ubyte])} | {decode (new [130ubyte])}"
```

実行結果:

```text
{"a":1,"b":[2,3]} | json: unsupported byte string at byte 0 | json: unexpected end of input at byte 1
```

`decode` は JSON のデータモデルに入る項目だけを読みます。

- 整数（major type 0・1）は 10 進の数値、bignum（タグ 2・3）も 10 進の数値です（4096 桁を超えると `NumberRange`）。
- 半精度・単精度・倍精度の浮動小数点は、その値の `f64` の最短表現の数値です。NaN と無限大は `NonFinite` です。
- テキスト文字列は `Text` です。正しい UTF-8 でなければ `InvalidUtf8` です。
- 配列は `Items`、マップは `Object` です。マップのキーはテキスト文字列だけで、キーの順は入力のまま残ります（整列されていない入力も受理します）。重複キーは `DuplicateKey` です。
- バイト文字列、タグ 2・3 以外のタグ、`undefined` などの単純値、長さ不定の項目は `Unsupported` です。予約された追加情報（28〜30）と場所違いの break は `Syntax` です。
- 最短でない引数の形（`0x1800` の 0 など）は受理します。最上位の項目の後に余分なバイトがあれば `Syntax` です。

## 資源上限

- 入力は 64 MiB まで（超えると `TooLarge`、offset 0）。配列とマップの入れ子は 128 段まで（`TooDeep`、129 段目の項目の位置）。
- 宣言された長さが残りのバイト数で足りないとき（配列は要素 1 つに 1 バイト、マップは 2 バイト以上が要る）は、読む前に `UnexpectedEnd` です。巨大な長さを書いた短い入力でも、先に大きな領域を確保しません。
- 再帰は入れ子の深さだけで、128 段で止まります。

## なぜ JSON の値を共有するのか

`Encode` / `Decode` が `Json.Value` を経由するので、同じインスタンスと導出がそのまま CBOR にも使えます。serde のように形式ごとの Serializer を受け取るクラスにすると、`Encode` のメソッドが形式の型について多相になり、導出は形式ごとの呼び出しを生成し、新しい形式はクラスの全メソッドを実装しなければなりません。共通の値を経由すれば、既存のインスタンスと導出を変えずに形式を足せ、形式の側は値の木を 1 つ読み書きするだけで済みます。代わりに値の木を一度作る費用がかかります。木を作らずに JSON を読み書きするには `Json.reader` / `Json.writer` を使います。

## 所有権と借用

`encode` と `decode` は引数を共有借用し、新しい所有値を返します。`Error` の経路でも途中の値はすべて解放します。

## API リファレンス

すべての関数は `std::Cbor` モジュールに属しています。

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `encode` | `ref Json.Value -> Result<[ubyte], Json.Error>` | 決定的な CBOR に符号化します |
| `decode` | `ref [ubyte] -> Result<Json.Value, Json.Error>` | CBOR の項目を 1 つ読みます |
| `serialize` | `Encode<'a> => ref 'a -> Result<[ubyte], Json.Error>` | 値を CBOR に |
| `deserialize` | `Decode<'a> => ref [ubyte] -> Result<'a, Json.Error>` | CBOR を値に |

## まとめ

- `Cbor` は `Json.Value` を RFC 8949 の決定的な符号化で読み書きします。
- `Encode` / `Decode` と導出は JSON と共通です。
- 失敗は `Json.Error` で返り、上限を超える入力もトラップしません。

## 関連項目

- [Json](./json.md) — 値の型、`Encode` / `Decode`、導出
- [型クラス](../types-and-type-inference/type-classes.md) — `deriving`
- [言語リファレンスの目次](../index.md)
