# Exception

`Exception` は、`try ... with` が受け取る例外の標準の型です。Tsuzuri 0.1.0 で送出される種類は `OverflowException` だけです。モジュールをインポートする必要はありません。

失敗の全体像は [例外処理](../exception-handling/exception-handling.md)、構文は [try 式](../exception-handling/try-with-finally.md) にあります。

## この記事のポイント

- 例外の値は `Exception` レコードです。種類は `kind`、説明は `msg` です。
- `Err` のインスタンスがあるので、`try` のハンドラーからそのまま返せます。
- 利用者のコードが送出できる例外の種類を増やす構文はありません。
- `of_code` は標準ライブラリの内部関数で、ユーザーコードからは呼べません。

## 型

| 名前 | 定義 | 説明 |
| --- | --- | --- |
| `ExceptionKind` | `union ExceptionKind = OverflowException` | `try` が捕捉する例外の種類。ハンドラーの `is` がこれを見ます |
| `Exception` | `record Exception { kind: ExceptionKind, msg: string }` | ハンドラーが受け取る値。`msg` は説明の文字列です |

`OverflowException` は共用体のケースなので、修飾せずに書けます。同名のケースを自分のモジュールで定義すると、そちらが優先されます。そのときは `Exception.OverflowException` と書きます。

メッセージは固定です。`@checked` が桁あふれしたときの `msg` は `Arithmetic operation resulted in an overflow.` です。

## Err

`Err` は組み込みの型クラスです。メソッドは 1 つです。

```text
msg :: ref 'a -> string
```

`try` のハンドラーが返す型は、このクラスを実装している必要があります。標準ライブラリは `Exception` に次のインスタンスを定義しています。`msg` はフィールドを複製して返すので、呼び出し後も例外の値は残ります。

```text
instance Err<Exception> {
    fn msg error = clone_string (ref error.msg)
}
```

`Exception` の `msg` フィールドは `string` 型（非 Copy）です。`let msg = e.msg` のようにフィールドを直接束縛すると所有権がムーブしてしまい、後から `e` 全体を返すことができなくなります。例外の値 `e` を残したままメッセージを取得したい場合は、`Err.msg (ref e)` を呼ぶか、補間文字列 `$"{e.msg}"` のように参照で埋め込みます。

```tsuzuri run=manual
let error = Exception { kind: OverflowException, msg: "manual" }
Err.msg (ref error)
```

実行結果:

```text
manual
```

レコードは公開されているので、上のように自分で値を作れます。作った値を `raise` する構文はありません。送出されるのは、`@checked` の整数演算が桁あふれしたときだけです。

ハンドラーで受け取った `Exception` をそのまま返すと、`try` の結果は `Error` になります。

```tsuzuri run=Arithmetic%20operation%20resulted%20in%20an%20overflow.
def safe_add :: i32 -> i32 -> Result<i32, Exception> = \x y ->
    try
        @checked x + y
    with
    | e is OverflowException -> e

match safe_add 2147483647 1 with
| Ok value -> to_string value
| Error e -> Err.msg (ref e)
```

実行結果:

```text
Arithmetic operation resulted in an overflow.
```

`| e is OverflowException` は、`kind` が `OverflowException` であるレコードと、パターン `e` の両方に一致することを要求します。種類が違う例外は、この節には入りません。いま送出される種類は 1 つだけなので、`| e -> e` でも同じ値を受け取れます。

## 独自のエラー型

`Exception` 以外の型も、`Err` を実装すればハンドラーの結果にできます。これは送出の種類を増やすのではなく、捕捉したあとの `Error` の中身を自分の型に変えるためのものです。

```tsuzuri run=failure%201
record Failure { code: i32 }

instance Err<Failure> {
    fn msg failure = "failure " + to_string failure.code
}

def safe :: i32 -> Result<i32, Failure> = \x ->
    try
        @checked x + 1
    with
    | _ is OverflowException -> Failure { code: 1 }

match safe 2147483647 with
| Ok value -> to_string value
| Error failure -> Err.msg (ref failure)
```

実行結果:

```text
failure 1
```

節の結果の型はすべて同じでなければなりません。ある節が `Exception` を返し、別の節が `Failure` を返すと `E1003` です。

## 注意点

- `Exception` は `string` を含むので Copy ではありません。ハンドラーから返すと、その所有権は `Error` に移ります。
- トップレベルの式が `Result<i32, Exception>` のままだと、`Display` が無くて表示できません。`match` してから表示します。
- ゼロ除算や `assert` は `Exception` になりません。トラップです。
- 内部の `Exception.of_code` は `private` です。ユーザーコードから呼ぶ API ではありません。

## まとめ

- 捕捉できる例外の値は `Exception { kind, msg }` です。種類は `OverflowException` だけです。
- `Err.msg` は説明文字列の複製を返します。フィールドの直接読みはムーブです。
- ハンドラーは `Exception` を返すか、`Err` を実装した別の型へ変換します。
- 利用者定義の送出はできません。回復する失敗は `Result` で返します。

## 関連項目

- [try 式](../exception-handling/try-with-finally.md)
- [例外処理](../exception-handling/exception-handling.md)
- [Result](result.md)
- [言語仕様: 検査付き算術と例外](../../../docs/language.md#検査付き算術と例外)
- [言語リファレンスの目次](../index.md)
