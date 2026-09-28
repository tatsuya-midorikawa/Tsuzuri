# コンピュテーション式

[ドキュメントのトップ](../README.md)

コンピュテーション式は、束縛や分岐、失敗の伝播、列挙などをビルダーの操作として組み合わせます。`Builder { ... }` の Builder は `.tc` ファイルから決まるモジュール名で、実行時のビルダーオブジェクトではありません。

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

`let` は計算値そのものを束縛し、`let!` はビルダーの `Bind` を通じて内部の成功値を取り出します。Result では Error、Option では None になると後続の継続を呼びません。

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
def Return :: 'value -> 'value
fn Return value = value

def ReturnFrom :: 'value -> 'value
fn ReturnFrom value = value

def Bind :: 'value -> ('value -> 'result) -> 'result
fn Bind value next = next value

def Zero :: unit
fn Zero = ()
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
let answer = Option {
    let! left = Option.Some 20
    and! right = Option.Some 22
    return left + right
}
Option.get answer
```

and! は直前の単純な let! と一つのグループを作ります。同じグループの右辺から、そのグループで束縛する名前は参照できません。すべての右辺を左から右へ一度ずつ評価してから結合します。途中が None / Error でも、残りの右辺の評価は省略しません。

二束縛と末尾 return だけで Bind2 があれば直接使い、それ以外は MergeSources を左結合して Bind へ渡します。and! は計算を自動で並列起動する指示ではありません。

```tsuzuri run=42
let answer = Option {
    match! Option.Some (true, 42) with
    | (true, value) -> return value
    | _ -> return 0
}
Option.get answer
```

match! は Bind の継続内で通常の match を行います。網羅性、ガード、所有権の規則も通常どおりです。

## 所有権と制限

継続は再利用可能な関数値です。排他借用や Task を捕捉することはできず、外側の可変束縛を書き換える共有状態も作れません。`let! mut value` はその継続内のローカル状態だけを可変にします。

Option / Result の For は Copy 要素の所有配列を受け取ります。通常の for が非 Copy 要素を借用して読む経路とは異なります。省略した else と空本体は、それぞれ `Some ()` / `Ok ()` です。

`use`、`use!`、`try ... with`、`try ... finally` は未対応です。自動解放と Option / Result を使います。カスタム演算、暗黙 yield、ビルダーオブジェクトもありません。task は別の一回実行用 lowering を使います。

既知で外へ逃げない継続は直接呼び出しなどへ特殊化できますが、任意のビルダーが無料になる保証ではありません。必要な結果領域、捕捉の複製、ビルダー自身のアルゴリズムを含めて評価します。

## 関連項目

- [タスク](tasks.md)
- [所有権](ownership.md)
- [パターンマッチ](patterns.md)
