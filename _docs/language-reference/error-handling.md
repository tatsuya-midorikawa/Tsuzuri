# 検査付き算術と例外処理

[ドキュメントのトップ](../README.md)

`@checked` を付けた整数演算は、オーバーフローすると折り返さずに例外を送出します。例外は `try ... with` で捕捉し、`Result` の値として扱います。現在の例外は `OverflowException` だけです。

## @checked

```tsuzuri run=42%20%2F%20Arithmetic%20operation%20resulted%20in%20an%20overflow.
def safe_add :: i32 -> i32 -> Result<i32, 'E> = \x y ->
    try
        @checked x + y
    with
    | e is OverflowException -> e

def show :: i32 -> i32 -> string = \x y ->
    match safe_add x y with
    | Ok value -> to_string value
    | Error e -> e.msg

show 40 2 + " / " + show 2147483647 1
```

`@checked` を式または文の前に置くと、その中の整数の `+`、`-`、`*`、`**` と単項 `-` を検査します。オーバーフローすると `OverflowException` を送出し、メッセージは `Arithmetic operation resulted in an overflow.` です。`@checked` の外は従来どおり型幅で折り返します。

検査するのは `@checked` の中に書いた演算子だけで、そこから呼び出した関数の中の演算は検査しません。オーバーフローを `Option` で受け取る場合は、Int の `checked_add` などを使います。

## try ... with ... finally

```text
try
    本体
with
| パターン -> ハンドラー
| 名前 is 例外の種類 -> ハンドラー
finally
    後処理
```

`try` は `Result<'T, 'E>` 型の式です。本体の値は `Ok`、捕捉した例外に対するハンドラーの結果は `Error` になります。ハンドラーの節にはガードも書けますが、網羅しなくてかまいません。一行で `try @checked x + 1 with | e -> e` とも書けます。

`finally` は省略できます。その本体は unit で、正常終了・捕捉・伝播のすべての経路で実行します。

```tsuzuri run=checked%2020%0Achecked%20100%0A40%20-1
def double :: i8 -> Result<i8, Exception> = \x ->
    try
        @checked x * 2
    with
    | e is OverflowException -> e
    finally
        do! IO.writeln $"checked {x}"

def value_or :: i8 -> Result<i8, Exception> -> i8 = \fallback result ->
    match result with
    | Ok value -> value
    | Error _ -> fallback

let small = value_or (-1) (double 20)
let large = value_or (-1) (double 100)
$"{small} {large}"
```

`try` の本体・ハンドラー・`finally` の中の IO の `let!` / `do!` は、その場で実行します（[IO の直接形式](computation-expressions.md#io-の直接形式)）。上の `finally` は呼び出しごとに一行を出力します。ビルダーの中の `try` も同じく `Result` の式です。

## Exception と Err

std の `Exception` モジュールは次の型とインスタンスを定義します。

```text
union ExceptionKind = OverflowException
record Exception { kind: ExceptionKind, msg: string }
instance Err<Exception>
```

`| e is OverflowException -> ...` は例外の kind で照合し、`e` に `Exception` の値を束縛します。`| e -> ...` はすべての例外に一致します。

ハンドラーの結果の型は、組み込みクラス `Err`（`msg :: ref 'a -> string`）を実装する必要があります。`Exception` 以外の独自の型も返せます。

```tsuzuri run=failure%201
record Failure { code: i32 }

instance Err<Failure> {
    fn msg failure = $"failure {failure.code}"
}

def safe :: i32 -> Result<i32, Failure> = \x ->
    try
        @checked x + 1
    with
    | _ is OverflowException -> Failure { code: 1 }

match safe 2147483647 with
| Ok value -> $"{value}"
| Error failure -> Err.msg failure
```

## エラー型の推論

結果が `Result<'T, 'E>` で、`'E` がそこにしか現れない関数では、`@'E : Err` の制約があるか本体が `try` なら、`'E` は捕捉した `Exception` です。本体が `try` なら制約を省略できます（最初の例の `safe_add`）。

次の例は `Overflow occurred: Arithmetic operation resulted in an overflow.` を表示します。トップレベルの結果の `Result` は `Display` を持たないため表示しません。

```tsuzuri run=Overflow%20occurred%3A%20Arithmetic%20operation%20resulted%20in%20an%20overflow.
def safe_add :: i32 -> i32 -> Result<i32, 'TErr>
    @'TErr : Err = \x y ->
        try
            @checked
            let result = x + y
            do! IO.writeln $"Result: {result}" |> ignore
            result
        with
        | e is OverflowException ->
            do! IO.writeln $"Overflow occurred: {e.msg}" |> ignore
            e
        | e ->
            do! IO.writeln $"Unexpected Error" |> ignore
            e

safe_add 2147483647 1
```

本体が `try` なので、`@'TErr : Err` を省略しても同じです。

```tsuzuri run=Overflow%20occurred%3A%20Arithmetic%20operation%20resulted%20in%20an%20overflow.
def safe_add :: i32 -> i32 -> Result<i32, 'TErr> = \x y ->
    try
        @checked
        let result = x + y
        do! IO.writeln $"Result: {result}" |> ignore
        result
    with
    | e is OverflowException ->
        do! IO.writeln $"Overflow occurred: {e.msg}" |> ignore
        e
    | e ->
        do! IO.writeln $"Unexpected Error" |> ignore
        e

safe_add 2147483647 1
```

## 伝播と字句的な範囲

どの節にも一致しない例外は、`finally` を実行してから外側の `try` へ伝播します。

```tsuzuri run=inner%20finally%0Aouter%3A%20Arithmetic%20operation%20resulted%20in%20an%20overflow.
def square :: i8 -> Result<Result<i8, Exception>, Exception> = \x ->
    try
        try
            @checked x * x
        with
        | e when false -> e
        finally
            do! IO.writeln "inner finally"
    with
    | e is OverflowException ->
        do! IO.writeln $"outer: {e.msg}"
        e

square 12y
```

例外は字句的です。送出は同じ関数本体で最も内側の `try` へ移り、関数呼び出しやラムダを越えません。呼び出し先の関数で送出した例外を、呼び出し元の `try` は捕捉しません。巻き戻し（unwind）もありません。

どの `try` も捕捉しない例外はトラップします。

```tsuzuri
let big = 2147483647
@checked big + 1
```

```text
trap: unhandled OverflowException: arithmetic operation resulted in an overflow at Main.tz:2:10
```

`finally` を持つ `try` の外へ出る `break` / `continue` は `E1023` です。

## 関連項目

- [数値](numbers.md)
- [式と演算子](expressions-and-operators.md)
- [条件とループ](control-flow.md)
- [コンピュテーション式](computation-expressions.md)
- [検査付き算術と例外の正式な契約](../../docs/language.md#検査付き算術と例外)
