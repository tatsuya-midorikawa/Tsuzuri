# ZString

`ZString` は、末尾に NUL（`U+0000`）が保証され、途中に NUL を含まない UTF-16 コード単位列を表す型です。Win32 の `*W` 系 API（`LPCWSTR` / `const wchar_t *`）のように、長さを別引数で取らず NUL 終端の UTF-16 ポインタを要求するネイティブ C API へ、追加コピーなしで文字列を渡すために使います。

名前を書いたプログラムだけが読み込む標準モジュールです。`ZString` という利用者のモジュール名は予約され、衝突は `E1011` です。

## この記事のポイント

- `ZString.literal "..."` は、文字列リテラルからコンパイル時に静的な NUL 終端 UTF-16 配列を作り、ヒープ確保なしで `ref ZString` を返します。リテラル内に `\0`（`\u{0}`）があるとコンパイル時に `E1005`（`zstring_literal_interior_nul`）になります。
- 実行時の `string` からは `ZString.of text` で `Result<ZString, ZString.Error>` を作ります。途中に `U+0000` があれば、最初の UTF-16 位置を `ZString.InteriorNul index` で返します。
- `ZString.to_string (ref z)` で末尾の NUL を除いた通常の `string` に戻せます。
- ネイティブの `extern def` の引数には `ref ZString`（非 `NULL` の `const uint16_t *`）と `Maybe<ref ZString>`（`None` が `NULL`、`Some` が `const uint16_t *`）を書けます。所有の `ZString` の値渡し・戻り値・`export def`・WebAssembly の `extern` は `E1008` です。
- `TSUZURI_STRINGS=compact`（Latin-1 圧縮文字列モード）でも、`ZString` の内部バッファは常に NUL 終端の 16-bit コード単位列なので、C への呼び出しで一時変換は起きません。

## API

```text
record ZString                 // 不透明な NUL 終端 UTF-16 文字列
union Error = InteriorNul of i64

ZString.literal :: string -> ref ZString
ZString.of :: ref string -> Result<ZString, ZString.Error>
ZString.to_string :: ref ZString -> string
```

### 引数の順序

`ZString.of`・`ZString.to_string`・`ZString.literal` はいずれも変換元の文字列 1 つだけを引数に取るため、`"hello" |> ZString.of` のようにそのままパイプで渡せます（[標準ライブラリの引数の順序](argument-order.md)）。

## 基本の書き方

定数のクラス名やウィンドウタイトルには `ZString.literal` を、動的に組み立てた文字列には `ZString.of` を使います。

```tsuzuri run=lit%3DTsuzuri%20dyn%3DHello%2C%20%E4%B8%96%E7%95%8C%20err%3DU%2B0000%20at%20UTF-16%20index%202%20cannot%20be%20in%20a%20NUL-terminated%20string
let lit = ZString.literal "Tsuzuri"
let dynamic_text = "Hello, 世界"
let dyn_round =
    match dynamic_text |> ZString.of with
    | Result.Ok z -> ZString.to_string (ref z)
    | Result.Error err -> $"{err}"
let bad_nul =
    match "ab\0cd" |> ZString.of with
    | Result.Ok z -> ZString.to_string (ref z)
    | Result.Error err -> $"{err}"
$"lit={ZString.to_string lit} dyn={dyn_round} err={bad_nul}"
```

実行結果:

```text
lit=Tsuzuri dyn=Hello, 世界 err=U+0000 at UTF-16 index 2 cannot be in a NUL-terminated string
```

サロゲートペアで表される非 BMP 文字（絵文字など）を含む場合、`InteriorNul` の添字は UTF-16 コード単位の位置（0 始まり）です。

## `extern` での受け渡し

ネイティブの `extern def` では、`ref ZString` が非 `NULL` の `const uint16_t *`（Windows では `LPCWSTR` 相当）、`Maybe<ref ZString>` が省略可能な `const uint16_t *`（`Maybe.None` が `NULL`）になります。

```tsuzuri
extern type copy Hwnd

@trusted
extern "SetWindowTextW" def set_window_text :: Hwnd -> Maybe<ref ZString> -> i32

def clear_title :: Hwnd -> i32 = \hwnd ->
    set_window_text hwnd Maybe.None

def set_default_title :: Hwnd -> i32 = \hwnd ->
    set_window_text hwnd (Maybe.Some (ZString.literal "Untitled"))
```

`ZString.literal "..."` が返す `ref ZString` は静的データを指すため、関数からそのまま返すことも、`Maybe.Some` に包んで渡すこともできます。`ZString.of` で作った所有値 `z` を渡すときは `ref z`（または `Maybe.Some (ref z)`）を渡し、呼び出しの間 `z` が生存します。

## 関連項目

- [String](string.md)
- [ネイティブ連携 (C ABI)](../compiler/native-interop.md)
