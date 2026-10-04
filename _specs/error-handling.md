# 例外処理

## try...with(...finally) 式

`try...with(...finally)` は Tsuzuri の例外を処理するために使用します。
`try...with(...finally)` を使う場合、関数の返り値の型は自動で `Result<'T, 'TErr>` となり、`'TErr` は必ず `Err` 型クラスを実装したものになります。
`with` で処理する例外は、`is` を併用することで、特定の例外をキャッチして処理をすることもできます。
また、オプションで `finally` 句を追加することもできます。これは、`try` 句が成功しても失敗しても無関係に必ず呼び出される世s理となります。

### 構文

```tz
try
    expression
with
  | pattern1 -> expression1
  | pattern2 ->
      expression2-1
      expression2-2
  | pattern3 is ExceptionName->
      expression2-1
      expression2-2
...
finally
  finally-expressions
```

### 例

```tz
def safe_add: i32 -> i32 -> Result<i32, 'TErr> 
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

// Output:
// Overflow occurred: Arithmetic operation resulted in an overflow.
```

また、`try...with(...finally)` の場合は、`Result<i32, 'TErr>` の `'TErr` に `Err` 型クラスが実装されているのは仕様上明確であるため、以下のように省略してもよい。

```tz
def safe_add: i32 -> i32 -> Result<i32, 'TErr> = \x y -> 
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

// Output:
// Overflow occurred: Arithmetic operation resulted in an overflow.
```


