# ZUtf8String

`ZUtf8String` は、末尾に NUL（`0x00` / `U+0000`）が保証され、途中に NUL を含まない正しい UTF-8 バイト列を表す型です。POSIX や Objective-C ランタイムの C API（`const char *`）のように、長さを別引数で取らず NUL 終端の UTF-8 ポインタを受け渡すネイティブ C API と安全に連携するために使います。

名前を書いたプログラムだけが読み込む標準モジュールです。`ZUtf8String` という利用者のモジュール名は予約され、衝突は `E1011` です。

## この記事のポイント

- `ZUtf8String.literal "..."` は、文字列リテラルからコンパイル時に静的な NUL 終端 UTF-8 バイト配列を作り、ヒープ確保なしで `ref ZUtf8String` を返します。リテラル内に `\0` があると `E1005`（`zutf8string_literal_interior_nul`）、孤立サロゲートがあると `E1005`（`zutf8string_literal_argument`）になります。
- 実行時の `utf8string` からは `ZUtf8String.of text` で `Result<ZUtf8String, ZUtf8String.Error>` を作ります（実行時の `string` からは先に `Utf8String.from_string` を通します）。途中に `U+0000` があれば、最初のバイト位置を `ZUtf8String.InteriorNul index` で返します。
- `ZUtf8String.to_utf8string (ref z)` で末尾の NUL を除いた `utf8string` に戻せます。
- ネイティブの `extern def` の引数には `ref ZUtf8String`（非 `NULL` の `const char *`）と `Maybe<ref ZUtf8String>`（`None` が `NULL`、`Some` が `const char *`）、戻り値には `Maybe<ZUtf8String>`（`NULL` が `None`、非 `NULL` は最初の NUL までを写し UTF-8 を検査して `Some` にする。不正な UTF-8 は `Encoding` トラップ）を書けます。

## API

```text
record ZUtf8String             // 不透明な NUL 終端 UTF-8 文字列
union Error = InteriorNul of i64

ZUtf8String.literal :: string -> ref ZUtf8String
ZUtf8String.of :: ref utf8string -> Result<ZUtf8String, ZUtf8String.Error>
ZUtf8String.to_utf8string :: ref ZUtf8String -> utf8string
```

### 引数の順序

`ZUtf8String.of` と `ZUtf8String.to_utf8string` はどちらも変換元の文字列 1 つだけを引数に取るため、`u8"hello" |> ZUtf8String.of` のようにそのままパイプで渡せます（[標準ライブラリの引数の順序](argument-order.md)）。`ZUtf8String.literal "..."` は文字列リテラルを直接受け取るコンパイル時構文であり、関数値やパイプとしては使えません。

## 基本の書き方

定数の環境変数名・クラス名・セレクタ名には `ZUtf8String.literal` を、動的に組み立てた `utf8string` には `ZUtf8String.of` を使います。

```tsuzuri run=lit%3DTsuzuri%20dyn%3DHello%2C%20%E4%B8%96%E7%95%8C%20err%3DU%2B0000%20at%20byte%203%20cannot%20be%20in%20a%20NUL-terminated%20string
let lit = ZUtf8String.literal "Tsuzuri"
let dynamic_text = u8"Hello, 世界"
let dyn_round =
    match dynamic_text |> ZUtf8String.of with
    | Result.Ok z -> ZUtf8String.to_utf8string (ref z)
    | Result.Error err -> Utf8String.from_string $"{err}"
let bad_nul =
    match u8"abc\0d" |> ZUtf8String.of with
    | Result.Ok z -> ZUtf8String.to_utf8string (ref z)
    | Result.Error err -> Utf8String.from_string $"{err}"
$"lit={ZUtf8String.to_utf8string lit} dyn={dyn_round} err={bad_nul}"
```

実行結果:

```text
lit=Tsuzuri dyn=Hello, 世界 err=U+0000 at byte 3 cannot be in a NUL-terminated string
```

## `extern` での受け渡し

ネイティブの `extern def` では、引数の `ref ZUtf8String` が非 `NULL` の `const char *`、`Maybe<ref ZUtf8String>` が省略可能な `const char *`（`Maybe.None` が `NULL`）になります。戻り値の `Maybe<ZUtf8String>` は、C 関数が返した `const char *` が `NULL` なら `Maybe.None`、非 `NULL` なら最初の NUL までのバイト列を Tsuzuri 側へ写して UTF-8 として検査します（C 側のポインタの所有はホストに残ります）。

```tsuzuri
@trusted
extern "getenv" def getenv :: ref ZUtf8String -> Maybe<ZUtf8String>

def read_home :: unit -> Maybe<utf8string> = \() ->
    match getenv (ZUtf8String.literal "HOME") with
    | Maybe.Some z -> Maybe.Some (ZUtf8String.to_utf8string (ref z))
    | Maybe.None -> Maybe.None
```

`@c_layout` レコードのフィールドにも `ref ZUtf8String` と `Maybe<ref ZUtf8String>` を入力専用フィールドとして書けます。値渡しの `ZUtf8String`、`Maybe` のない戻り値の `ZUtf8String`、`ref mut ZUtf8String`、コールバック、`export def`、および WebAssembly の `extern` は `E1008` です。

## 関連項目

- [Utf8String](utf8string.md)
- [ZString](zstring.md)
- [ネイティブ連携 (C ABI)](../compiler/native-interop.md)
