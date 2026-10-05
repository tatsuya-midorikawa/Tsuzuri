# コンピュテーション式

[ドキュメントのトップ](../README.md)

コンピュテーション式は、束縛や分岐、失敗の伝播、列挙などをビルダーの操作として組み合わせます。`Builder { ... }` の Builder は `.tc` ファイルから決まるモジュール名で、実行時のビルダーオブジェクトではありません。

## ブロックを省略する

関数本体やトップレベルへ直接 `let!`／`do!` を書けます。右辺の型からビルダーを解決するため、IO と Maybe も同じ本体で扱えます。

```tsuzuri
def main :: IO<Maybe<unit>> =
    let! line = IO.read_line ()
    let! value = line
    do! IO.write_line value
```

EOF の None では後続の出力を実行しません。この例の入口の型は `IO<Maybe<unit>>` であり、失敗情報を消さずに結果へ保持します。入口はその IO を一度実行して結果を解放し、追加の出力を行いません。

```tsuzuri run=42
def answer :: Maybe<i64> =
    let! first = Some 20
    let! second = Some 22
    return first + second
Maybe.get (answer())
```

明示的な `Maybe { ... }`／`Result { ... }`／独自の `Builder { ... }` も引き続き使えます。`do expression` は unit の通常式、`do!` は計算値の束縛です。
引数なし・非再帰の main だけは def を省略して入口の型を推論できます。通常の関数には def を付けます。
CE は main 専用ではありません。引数付き・private・別モジュール・ジェネリック・再帰関数、匿名関数、
クラスメソッド、`.tc` の補助関数でも同じ構文を使えます。`\value ->` のラムダ式も改行した暗黙本体に対応します。

独自 `.tc` も Bind の入力型で解決します。同じ型に複数の候補があれば宣言順で選ばず `E1018` にし、型注釈や明示ビルダーを要求します。
異種ビルダーは `Maybe<Result<T, E>>` などの入れ子を保持し、失敗を相互変換しません。内側が遅延値なら、その遅延も残ります。
IO は任意のビルダーの Bind／Return／Delay／Run を `IO.Using` で合成し、継続が呼ばれたときだけ後続の IO を実行します。
独自の遅延ビルダーも公開 Using を実装できます。契約と推論規則は[言語仕様](../../docs/language.md#ビルダー名を省略した本体)を参照してください。
任意の異種モナドを追加の契約なしで同じ型へ平坦化する機能ではありません。

## IO の直接形式

結果型が分かっていて、その型にビルダーがない本体（`def main :: i32` など）では、IO の `let!` / `do!` をその場で順に実行します。

```tsuzuri run=hello%0A42
def main :: i32 =
    do! IO.writeln "hello"
    42
```

`IO.writeln` は `IO.write_line` の別名です。`do! a |> f` は `a` を束縛してから結果を `f` へ渡す文で、`do! IO.writeln "x" |> ignore` のように使います。

Main.tz のトップレベルも、IO の束縛の後を通常の結果式で終えると、束縛をその場で順に実行し、その式の値を結果にします。

```tsuzuri run=start%0A42
do! IO.writeln "start" |> ignore
40 + 2
```

`try` の本体・ハンドラー・`finally` の中も同じです（[例外処理](error-handling.md)）。直接実行するのは IO の束縛だけです。

## 標準 Result ビルダー

```tsuzuri run=42
let answer: Result<i64, string> = Result {
    let! first = Result.Ok 20
    let! second = Result.Ok 22
    return first + second
}
match answer with
| Result.Ok value -> value
| Result.Error _ -> 0
```

`let` は計算値そのものを束縛し、`let!` はビルダーの `Bind` を通じて内部の成功値を取り出します。Result では Error、Maybe では None になると後続の継続を呼びません。

`return` は成功値を生成する操作で、関数からどこでも早期脱出する構文ではありません。各本体・分岐の末尾に置きます。通常の式で起きるトラップも Result の Error に変換しません。

## 操作一覧

| 構文・役割 | 必要な操作 |
| --- | --- |
| `let!` / `do!` | `Bind`。do! の成功値は unit |
| `return` / `return!` | `Return` / `ReturnFrom` |
| `yield` / `yield!` | `Yield` / `YieldFrom` |
| 空本体、省略された else | 引数なしの `Zero()` |
| 二つの計算の連結 | `Combine` |
| 本体を遅延値として包む | 任意の `Delay` |
| 最後に計算を実行・変換する | 任意の `Run` |
| ビルダー内の for | `For` |
| ビルダー内の while | `While` と `Delay` |
| `match!` | `Bind` と通常の match |
| `and!` | `MergeSources`、条件を満たす場合は `Bind2` |
| 単純な let! と末尾 return | 存在すれば `BindReturn` |

必要な操作がなければ `E1018` です。`ReturnFrom` などを暗黙の恒等関数として補いません。操作があって型が合わない場合も、別の展開へ黙って戻すことはありません。

## ユーザー定義ビルダー

`Identity.tc`:

```tsuzuri project=identity file=Identity.tc
def Return :: 'value -> 'value = \value -> value

def ReturnFrom :: 'value -> 'value = \value -> value

def Bind :: 'value -> ('value -> 'result) -> 'result = \value next -> next value

def Zero :: unit = ()
```

`Main.tz`:

```tsuzuri project=identity file=Main.tz run=42
Identity {
    let! first = 20
    let! second = 22
    do! Identity {}
    return first + second
}
```

これらは通常の多相関数です。`Bind` が継続を呼ぶ回数や条件は実装で決まり、コンパイラはモナド則などを仮定しません。操作は通常の関数としても呼べます。

`Zero` は引数なしであり、`unit -> unit` の一引数関数とは異なります。操作名は大文字小文字を区別し、private にはできません。補助関数は private にできます。

## Delay と評価時点

`Delay` があると本体を unit 引数の関数として渡します。その結果を `Run`、`Combine` の第二引数、`While` の本体として利用します。Delay が受け取った関数をすぐ実行する実装なら、実際には遅延しません。

Delay がない Combine では第二引数も厳格評価されます。短絡を実現するなら、後続本体を実行するかどうかをビルダーが選べる形にします。

## and! と match! による合成

```tsuzuri run=42
let answer = Maybe {
    let! left = Maybe.Some 20
    and! right = Maybe.Some 22
    return left + right
}
Maybe.get answer
```

and! は直前の単純な let! と一つのグループを作ります。同じグループの右辺から、そのグループで束縛する名前は参照できません。すべての右辺を左から右へ一度ずつ評価してから結合します。途中が None / Error でも、残りの右辺の評価は省略しません。

二束縛と末尾 return だけで Bind2 があれば直接使い、それ以外は MergeSources を左結合して Bind へ渡します。and! は計算を自動で並列起動する指示ではありません。

```tsuzuri run=42
let answer = Maybe {
    match! Maybe.Some (true, 42) with
    | (true, value) -> return value
    | _ -> return 0
}
Maybe.get answer
```

match! は Bind の継続内で通常の match を行います。網羅性、ガード、所有権の規則も通常どおりです。

## 所有権と制限

継続は再利用可能な関数値です。排他借用、Task、`Drop` を持つ値を捕捉することはできず、外側の可変束縛を書き換える共有状態も作れません。`let! mut value` はその継続内のローカル状態だけを可変にします。

Maybe / Result の For は Copy 要素の所有配列を受け取ります。通常の for が非 Copy 要素を借用して読む経路とは異なります。省略した else と空本体は、それぞれ `Some ()` / `Ok ()` です。

`use name = value` は `let` と同じ束縛で、値の型が `Drop` を持つことを要求します。`use! name = source` は `let!` と同じく値を取り出し、その値を `use` で束縛します（`and!` とは組み合わせられません）。解放は通常の束縛と同じ scope の終わりで、`Using` 操作は呼びません。`let!`・`use!` より前に束縛した Drop 型の値は、継続の関数値に捕捉できないので後ろでは使えません（[所有権](ownership.md#use-束縛と早期解放)）。

`try ... with ... finally` はビルダーの中でも `Result` を返す通常の式で、ビルダーの操作には展開しません（[例外処理](error-handling.md)）。カスタム演算、暗黙 yield、ビルダーオブジェクトはありません。task は別の一回実行用 lowering を使います。

```tsuzuri run=42
let answer = Maybe {
    let! base = Some 40
    let total = try @checked base + 2 with | e -> e
    return total
}
match answer with
| Some (Ok value) -> value
| _ -> 0
```

既知で外へ逃げない継続は直接呼び出しなどへ特殊化できますが、任意のビルダーが無料になる保証ではありません。必要な結果領域、捕捉の複製、ビルダー自身のアルゴリズムを含めて評価します。

## 関連項目

- [タスク](tasks.md)
- [所有権](ownership.md)
- [パターンマッチ](patterns.md)
