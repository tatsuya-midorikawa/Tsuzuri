# コードサンプル

[ドキュメントのトップ](../README.md) | [概要](overview.md) | [なぜ Tsuzuri か](why-tsuzuri.md)

小さなプログラムで、Tsuzuri の書き方と実行結果を紹介します。環境の準備は[入門](../get-started.md)を参照してください。

各節の Tsuzuri コードは、特記がなければ独立したディレクトリの Main.tz です。例どうしを同じプロジェクトへ混在させず、実行したい例のディレクトリを指定します。以降のコマンドは `tsuzuri` が PATH にある環境を想定しています。リポジトリからビルドした場合は、`./target/release/tsuzuri` で実行できます。

- [Hello World](#hello-world)
- [関数を渡して配列を変換する](#関数を渡して配列を変換する)
- [ジェネリックなレコードとテスト](#ジェネリックなレコードとテスト)
- [失敗の理由を返す](#失敗の理由を返す)
- [配列を借用して更新する](#配列を借用して更新する)
- [FizzBuzz](#fizzbuzz)
- [タスクをまとめて実行する](#タスクをまとめて実行する)
- [JavaScript から呼び出す](#javascript-から呼び出す)

## Hello World

標準出力へ一行書き込みます。

```tsuzuri run=Hello%2C%20Tsuzuri!
IO {
    do! IO.write_line "Hello, Tsuzuri!"
}
```

この例のディレクトリを hello とした場合のコマンドです。

```sh
tsuzuri check ./hello
tsuzuri run ./hello
```

出力:

```text
Hello, Tsuzuri!
```

`IO` は入出力アクションを組み立てます。入口が返すアクションを実行環境が一度実行し、`do!` が出力を順に進めます。IO の入口には、結果を自動で表示する処理は追加されません。

後の例のように入口が整数などの通常の値を返す場合、native のコンソール用ホストが最後の値を表示します。明示的な出力を行う IO とは区別してください。

詳しくは [IO と標準入出力](../library-reference/io.md)を参照してください。

## 関数を渡して配列を変換する

引数の一部を渡して関数を作り、配列の各要素へ適用します。

```tsuzuri run=32
def multiply :: i64 -> i64 -> i64 = \factor value -> factor * value

let double = multiply 2
let values = [3, 5, 8]
let doubled = Array.map double (ref values)
Array.sum ref doubled
```

結果:

```text
32
```

`multiply 2` は `i64 -> i64` の関数値です。`Array.map` は入力を借用し、変換結果の新しい配列 `[6, 10, 16]` を作ります。最後に `Array.sum` で合計します。

Array の高階関数は F# と同じく変換関数が先、配列が後です。`values` の要素型は `double` へ渡すことから i64 に決まります。`ref values` は読み取りのための借用を示し、元の配列の所有権を渡しません。変換結果の配列には別の記憶域が必要です。`values |> Array.map double |> Array.sum` のようにパイプでつなぐこともでき、途中の配列は呼び出しの間だけ借用されます。

詳しくは[関数と部分適用](../language-reference/functions.md)、[Array の API](../library-reference/arrays-and-lists.md)を参照してください。

## ジェネリックなレコードとテスト

型パラメーターを使うと、同じ構造に異なる型の値を入れられます。次の例は、ラベル付きの値から中身を取り出します。

```tsuzuri run=42
record Named<'value> { label: string, value: 'value }

def take_value :: Named<'value> -> 'value = \entry -> entry.value

test "extracts an integer" =
    let entry = Named { label: "answer", value: 42 }
    assert (take_value entry == 42)

test "extracts an owned string" =
    let entry = Named { label: "language", value: "Tsuzuri" }
    let text = take_value entry
    assert (text == "Tsuzuri")

take_value (Named { label: "answer", value: 42 })
```

結果:

```text
42
```

`'value` は型変数で、呼び出しごとに具体型が決まります。`take_value` はレコードを所有値として受け取り、必要なフィールドを返します。文字列を取り出す場合はその所有権を移し、残ったラベルは解放します。

この例を named ディレクトリとして扱う場合、実行とテストは次のように分けられます。

```sh
tsuzuri run ./named
tsuzuri test ./named
```

通常の実行結果は `42` で、テストコマンドでは 2 件のテストが成功します。`test` 宣言は通常のビルドでは実行コードに含めません。

詳しくは[レコード](../language-reference/records.md)と[テスト](../tools/testing.md)を参照してください。

## 失敗の理由を返す

入力を非負の整数として読み取り、解析失敗と負数を別の理由として返します。二つの入力が成功した場合だけ合計します。

```tsuzuri run=42
def parse_count :: ref string -> Result<i64, string> = \text ->
    let parsed: Option<i64> = Parse.parse text
    match parsed with
    | Option.None -> Result.Error "not an integer"
    | Option.Some count ->
        if count < 0 then Result.Error "must be non-negative"
        else Result.Ok count

test "rejects invalid input" =
    let text = "twenty"
    match parse_count ref text with
    | Result.Error message -> assert (message == "not an integer")
    | Result.Ok _ -> assert false

test "rejects a negative count" =
    let text = "-1"
    match parse_count ref text with
    | Result.Error message -> assert (message == "must be non-negative")
    | Result.Ok _ -> assert false

let first = "20"
let second = "22"
let total: Result<i64, string> = Result {
    let! left = parse_count ref first
    let! right = parse_count ref second
    return left + right
}
match total with
| Result.Ok value -> IO.write_line value
| Result.Error message -> IO.write_error_line message
```

出力:

```text
42
```

`Result` の `let!` は、`Ok` から値を取り出し、`Error` なら残りの計算を行わず同じ失敗を返します。最後の `match` で成功を stdout、失敗の理由を stderr に出力します。

例えば最初の入力が `"twenty"` なら、出力するエラーは `not an integer` です。テストはこの経路と負数の拒否を確認します。合計の `+` 自体は通常の i64 加算で、上限を超えると折り返します。集計のオーバーフローも拒否したい場合は、`Int.checked_add` の結果を処理するか、`@checked` の式を `try` で `Result` に変えます（[例外処理](../language-reference/error-handling.md)）。

詳しくは [Option / Result](../library-reference/option-result.md)、[表示と解析](../library-reference/formatting-and-parsing.md)、[整数 API](../library-reference/integers.md)を参照してください。

## 配列を借用して更新する

配列の一部をコピーせず集計し、その結果を使って更新後の配列を作ります。

```tsuzuri run=42
let original = [10, 20, 22, 100]
let middle = ref original[1..3]
let subtotal = Array.sum middle
let updated = Array.set original 0 subtotal
assert (original[0] == 10)
updated[0]
```

結果:

```text
42
```

`ref original[1..3]` が読むのは位置 1 と 2 の要素です。借用は最後の使用まで生きており、この例では集計が済んだ後に更新できます。

`Array.set` は所有値を受け取り、更新後の配列を返します。この例では元の Copy 配列を後でも使うため、独立したコピーが更新され、`original[0]` は `10` のままです。共有された要素をその場で書き換える操作ではありません。

詳しくは[所有権](../language-reference/ownership.md)と[共有スライス・配列更新](../library-reference/arrays-and-lists.md)を参照してください。

## FizzBuzz

条件分岐で表示する文字列を決め、1 から 15 まで順番に出力します。

```tsuzuri run=1%0A2%0AFizz%0A4%0ABuzz%0AFizz%0A7%0A8%0AFizz%0ABuzz%0A11%0AFizz%0A13%0A14%0AFizzBuzz
def fizz_buzz :: i64 -> string = \value ->
    if value % 15 == 0 then "FizzBuzz"
    elif value % 3 == 0 then "Fizz"
    elif value % 5 == 0 then "Buzz"
    else to_string value

let values = new [i64](15, \index -> index + 1)
IO {
    for value in values do
        do! IO.write_line (fizz_buzz value)
}
```

出力:

```text
1
2
Fizz
4
Buzz
Fizz
7
8
Fizz
Buzz
11
Fizz
13
14
FizzBuzz
```

`if` は値を返す式なので、各分岐の型を `string` にそろえます。`new [i64]` は添字から要素を初期化し、IO 内の `for` は配列を先頭から処理します。ここでの出力は逐次実行です。

詳しくは[条件分岐と反復](../language-reference/control-flow.md)を参照してください。

## タスクをまとめて実行する

それぞれのタスクで整数の二乗を求め、結果をまとめて受け取ります。

```tsuzuri run=14
let work = new [Task<i64>](4, \index -> task { return index * index })
let results = Task.run (Task.parallel work)
assert (results[2] == 4)
Array.sum ref results
```

結果:

```text
14
```

`task` は作成時には実行されません。`Task.parallel` が複数のタスクを一つにまとめ、`Task.run` がそれを一回消費して実行します。結果の配列は完了順ではなく入力順なので、内容は `[0, 1, 4, 9]` です。

native では常駐ワーカープールを使います。WASM は既定で逐次実行し、並列実行には threads の opt-in と Worker ホストが必要です。この小さい例は実行の意味を示すもので、並列化による高速化を示すベンチマークではありません。

詳しくは[タスク](../language-reference/tasks.md)と [Parallel](../library-reference/parallel.md)を参照してください。

## JavaScript から呼び出す

計算用の関数を WASM に公開します。この節だけは入口の Main.tz ではなく、wasm ディレクトリ内の Kernel.tz として扱います。

```tsuzuri project=wasm file=Kernel.tz
export def add :: i64 -> i64 -> i64 = \left right -> left + right
```

WASM の生成には Clang と wasm-ld、ここでのホスト実行には Node.js が必要です。詳細なセットアップは[入門](../get-started.md)を参照してください。

```sh
tsuzuri build ./wasm/Kernel.tz --target wasm32 -o ./wasm/kernel.wasm
```

同じ wasm ディレクトリの run.mjs がホストです。

```javascript
import { readFile } from "node:fs/promises";

const bytes = await readFile(new URL("./kernel.wasm", import.meta.url));
const { instance } = await WebAssembly.instantiate(bytes);
console.log(instance.exports.tz_add(20n, 22n).toString());
```

```sh
node ./wasm/run.mjs
```

出力:

```text
42
```

公開名は `tz_add`、i64 の引数と結果は JavaScript の BigInt です。`20` ではなく `20n` を渡します。この計算用モジュールには IO や extern がなく、import object は不要です。インスタンス化しただけでは関数は実行されず、ホストが明示的に呼び出します。

文字列や配列を渡す場合は、スカラーとは異なるバッファ ABI と所有権の処理が必要です。詳しくは [WASM と JavaScript ホスト](../guides/webassembly.md)を参照してください。native ホストへ組み込む例は [C 連携](../guides/native-interop.md)にあります。

## 次に読む

- [Tsuzuri のツアー](../tour.md): 値、型、借用、計算式を順番に学ぶ。
- [アプリケーションの実装例](../examples/README.md): 料金見積もり、単語カウント、TODO 管理、JSON ファイル集計。
- [言語リファレンス](../language-reference/README.md): 各構文の意味と制限。
- [標準ライブラリ](../library-reference/README.md): 引数順、所有権、失敗条件。

## 参考

構成の参考: [Zig Samples](https://ziglang.org/learn/samples/)。
