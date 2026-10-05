# 入力を検証する料金見積もり

[実装例の一覧](README.md) · [ドキュメントのトップ](../README.md)

商品単価と数量を受け取り、小計・送料・合計を表示する小さなアプリケーションです。文字列の解析、業務ルール、表示を別の関数に分け、入力エラーを Result で返します。

## 完成時の動作

単価 1,800 円の商品を 3 個購入する場合の出力です。

```text
Subtotal: 5400 JPY
Shipping: 0 JPY
Total: 5400 JPY
```

この例のルールは「小計 5,000 円以上なら送料無料、それ以外は 500 円」です。税、割引、通貨換算は扱いません。金額は整数円とし、浮動小数点の丸めを持ち込みません。

このページのアプリは Main に用意した入力を処理するコンソール例です。端末からの対話入力、注文の保存、決済は行いません。入力を外から受けるホストの作り方は[WASM ガイド](../guides/webassembly.md)で説明しています。

## 使用する機能

| 機能 | この例での用途 |
| --- | --- |
| record | 小計・送料・合計を一つの Quote にまとめる |
| Parse と Maybe | 入力文字列を i64 として解析する |
| Result の計算式 | 解析・検証の失敗をそのまま返す |
| Int.checked_mul | 単価と数量の積の overflow を失敗値にする |
| 共有借用 | 入力や見積もりを消費せず読み取る |
| test 宣言 | 送料の境界と不正入力を確認する |

## 完成コード

作業ディレクトリを `target/order-quote` とし、その直下の `Main.tz` に置く内容です。次のブロック全体が一つのプログラムです。

```tsuzuri run=Subtotal%3A%205400%20JPY%0AShipping%3A%200%20JPY%0ATotal%3A%205400%20JPY
record Quote { subtotal: i64, shipping: i64, total: i64 }

def parse_integer :: ref string -> Result<i64, string> = \text -> Maybe.to_result "Enter a valid integer." (Parse.parse text)

def quote :: i64 -> i64 -> Result<Quote, string> = \unit_price quantity ->
    if unit_price < 0 then Result.Error "Price must not be negative."
    elif quantity <= 0 then Result.Error "Quantity must be positive."
    else Result {
        let! subtotal = Maybe.to_result "Order total is too large." (Int.checked_mul unit_price quantity)
        let shipping = if subtotal >= 5000 then 0 else 500
        return Quote { subtotal: subtotal, shipping: shipping, total: subtotal + shipping }
    }

def parse_quote :: ref string -> ref string -> Result<Quote, string> = \price_text quantity_text -> Result {
    let! unit_price = parse_integer price_text
    let! quantity = parse_integer quantity_text
    return! quote unit_price quantity
}

def format_quote :: ref Quote -> string = \value ->
    "Subtotal: " + to_string value.subtotal + " JPY\n" +
    "Shipping: " + to_string value.shipping + " JPY\n" +
    "Total: " + to_string value.total + " JPY"

test "adds shipping below the threshold" =
    match quote 4999 1 with
    | Result.Ok value -> assert (value.shipping == 500 && value.total == 5499)
    | Result.Error _ -> assert false

test "offers free shipping at the threshold" =
    match quote 2500 2 with
    | Result.Ok value -> assert (value.shipping == 0 && value.total == 5000)
    | Result.Error _ -> assert false

test "rejects zero quantity" =
    let result = quote 1800 0
    assert (Result.is_error ref result)

test "rejects negative prices" =
    let result = quote (-1) 2
    assert (Result.is_error ref result)

test "rejects overflowing totals" =
    let result = quote 9223372036854775807 2
    assert (Result.is_error ref result)

test "rejects nonnumeric input" =
    let price = "unknown"
    let quantity = "3"
    let result = parse_quote (ref price) (ref quantity)
    assert (Result.is_error ref result)

def main :: string =
    let price = "1800"
    let quantity = "3"
    match parse_quote (ref price) (ref quantity) with
    | Result.Ok value -> format_quote ref value
    | Result.Error message -> "Error: " + message
```

## 実行する

Rust と LLVM を導入したリポジトリのルートで実行します。コンパイラが未ビルドなら[入門](../get-started.md)の準備を先に行います。

```sh
./target/release/tsuzuri check target/order-quote
./target/release/tsuzuri run target/order-quote
./target/release/tsuzuri test target/order-quote
```

単価と数量を変える場合は main の二つの入力値を変更します。たとえば単価 1,200 円・数量 2 個なら、小計 2,400 円、送料 500 円、合計 2,900 円です。

## 処理の流れ

1. parse_integer が文字列を解析し、None を説明付きの Error に変える。
2. quote が負の単価と 0 以下の数量を拒否する。
3. checked_mul が積を検査し、表現できない金額を Error にする。
4. 小計から送料を決め、Quote を返す。
5. main が成功と失敗を分岐し、コンソール向けの文字列を返す。

Result の let! は Error の後の継続を呼びません。return! は別の Result をそのまま計算式の結果にします。例外やトラップを捕捉しているわけではありません。

送料加算が overflow しないのは、このルールでは小計が 5,000 未満のときだけ 500 を加えるためです。送料ルールを変える場合は加算の上限も見直し、必要なら Int.checked_add を使います。

## 入力と所有権の注意

Parse は前後の空白を自動で無視しません。この例では `" 1800 "` は解析エラーです。空白を許可する UI なら String.trim を明示してから解析します。

price_text と quantity_text は共有借用で、解析のために複製しません。format_quote も Quote を借用します。エラーメッセージは所有 string として返し、main が表示文字列に消費します。

アプリが Error の文字列を返しても、コンソールプログラムとしては正常終了です。業務エラーを OS の非ゼロ終了コードに対応させる場合は、ホスト側で成功・失敗を判断する設計にします。

## 発展させるとき

複数商品を扱うなら LineItem の配列を作り、各積だけでなく小計の加算も検査します。消費税や割引を導入する場合は、丸め方と適用順序を業務ルールとして決めてから実装します。

## 関連項目

- [Maybe と Result](../library-reference/maybe-result.md)
- [整数の checked 演算](../library-reference/integers.md)
- [コンピュテーション式](../language-reference/computation-expressions.md)
- [言語内テスト](../tools/testing.md)
