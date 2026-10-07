# Lazy 式

Tsuzuri 0.1.0 には、F# の `lazy` 式や `Lazy<T>` 型に相当する**遅延評価の構文はありません**。

`_features/README.md` にも、`lazy` 構文を入れる計画チケットはありません。Tsuzuri は正格評価です。変数束縛や関数の引数は、その場に到達したときに評価されます。ただし、関数の本体、選ばれなかった `if` の枝、`task` の本体、`IO` アクション、`Seq` の次の要素は、定義した瞬間には動きません。

計算の開始を遅らせたいときは、`unit -> T` の関数（サンク）、コールドな [Task 式](task.md)、遅延列 [Seq](../built-in-types-and-modules/seq.md)、遅延アクション [IO](../built-in-types-and-modules/io.md) を使います。

## この記事のポイント

- Tsuzuri 0.1.0 には `lazy` 式も `Lazy<T>` もなく、導入計画もありません。
- 値の束縛と関数引数は、その場で評価されます。
- いちばん軽い遅延は、`unit -> T` の関数です。
- CPU 上の計算を後回しにするなら、コールドな `Task<T>` を使います。
- 要素を必要なときだけ作るなら `Seq`、副作用を遅らせるなら `IO<T>` を使います。
- F# の `lazy` のような自動メモ化はありません。

## 正格評価と、遅らせたくなる場面

束縛は、その行に到達したときに評価されます。

```text
let heavy_value = expensive_calculation () // この時点で計算が走る
```

関数を定義しただけでは本体は走りません。`if` も、選ばれた枝だけを評価します。それでも、次のようなときは「今は評価しない値」が欲しくなります。

1. **条件付きの計算**: 条件が真のときだけ重い計算をしたい。
2. **実行と定義の分離**: タスクを先に組み立て、あとから順や並列度を決めたい。
3. **必要な分だけの列**: 無限列や大きな列を、消費したところまで作りたい。
4. **副作用のタイミング**: 画面表示やファイル書き込みを、呼び出し側が明示的に始めたい。

## 遅延の代わりになるもの

| 手法 | 型 | 動き始めるとき | メモ化 | 主な用途 |
| --- | --- | --- | --- | --- |
| サンク（関数値） | `unit -> 'a` | `thunk ()` の呼び出し | なし（呼ぶたびに再計算） | 軽い遅延、条件付き実行 |
| コールドな Task | `Task<'a>` | `Task.run` や `let!` | なし（1 回実行で消費） | 並列計算、CPU 上の処理の遅延 |
| Seq | `Seq<'a>` | `Seq.next`、`for`、`to_array` | なし（1 回消費） | 必要な要素だけの列、無限列 |
| IO アクション | `IO<'a>` | `main`、`do!`、`let!`、トップレベルの `IO` | なし（実行のたびに副作用） | 入出力の順序 |

公開の `IO.run` はありません。コンピュテーション式の `Delay` を自分で定義すれば、呼び出しまで本体を遅らせるビルダーも作れます。それは言語組み込みの `lazy` ではなく、[コンピュテーション式](../computation-expressions/computation-expressions.md) の操作です。

## 代替手段 1: unit -> T のサンク

いちばん軽いやり方は、`()` を受け取る関数にすることです。

```tsuzuri run=42
def heavy_calc :: unit -> i64 = \() -> 20 + 22

let condition = true
let answer = if condition then heavy_calc () else 0
answer
```

実行結果:

```text
42
```

`heavy_calc` の本体は、`heavy_calc ()` と書くまで走りません。`condition` が `false` なら一度も走りません。

同じ関数を 2 回呼ぶと、中の式も 2 回評価されます。結果を共有したいときは、1 回呼んだ値を変数に束縛します。

## 代替手段 2: コールドな Task 式

[Task 式](task.md) の `Task<T>` は、作った時点ではスレッドを起動しません。重い計算を包んだ遅延値として使えます。

```tsuzuri run=42
let delayed_work = task {
    let a = 20
    let b = 22
    return a + b
}

Task.run delayed_work
```

実行結果:

```text
42
```

`Task<T>` は非 Copy で、1 回しか実行できません。同じ値を 2 回 `Task.run` すると `E1012` になります。何度も走らせたい計算は、呼ぶたびに新しい `Task` を返す関数にします。

## 代替手段 3: Seq による遅延列

要素を一度に確保せず、消費したところまで作りたいときは [Seq](../built-in-types-and-modules/seq.md) を使います。`Seq.unfold` と `Seq.defer` で、有限列も無限列も作れます。

```tsuzuri run=30
let evens = Seq.unfold (\n -> if n <= 10 then Maybe.Some (n, n + 2) else Maybe.None) 0
let values = Seq.to_array evens
Array.sum ref values
```

実行結果:

```text
30
```

要素ができるのは、`Seq.next`、`for`、`to_array` が列を進めたときです。`Seq` は非 Copy の 1 回消費です。同じ列をもう一度進めると `E1012` になります。もう一度欲しいときは、列を作る関数を再度呼びます。

> [!WARNING]
> `Seq.unfold` や `Seq.defer` は無限列を作れます。`Seq.to_array` は列を最後まで消費するので、無限列では止まりません。メモリ不足になることもあります。非同期 I/O やバックプレッシャーの API でもありません。

## 代替手段 4: IO による副作用の遅延

画面出力やファイルアクセスを遅らせるときは、[IO](../built-in-types-and-modules/io.md) を使います。

```tsuzuri run=done
def main :: unit -> i32 = \() ->
    let action = IO.write_line "done"
    do! action
    0
```

実行結果:

```text
done
```

`IO.write_line` を呼んでアクションを作っただけでは、画面には出ません。`do!` や `main` の本体で実行したときに副作用が起きます。同じアクションを 2 回実行すると、副作用も 2 回起きます。

## メモ化はない

F# の `lazy` は、最初の評価結果を覚えて、2 回目以降はそれを返します。

**Tsuzuri には、言語組み込みの自動メモ化はありません。**

- サンク（`unit -> T`）は、呼ぶたびに本体を再評価します。
- `Task<T>` は 1 回実行です。同じタスクの二重実行は `E1012` になります。
- `Seq` を進めると、その列は消費されます。結果の配列を使い回したいときは、`Seq.to_array` した値を束縛します。

同じ結果を何度も使うなら、評価した値を通常の変数に入れます。

```tsuzuri run=42
def compute :: unit -> i64 = \() -> 20 + 22

let cached_result = compute ()
cached_result
```

実行結果:

```text
42
```

## 他の言語との比較

| 言語 | 構文 / 型 | 評価 | 自動メモ化 | 特徴 |
| --- | --- | --- | --- | --- |
| Tsuzuri | なし（サンク / `Task` / `Seq` / `IO`） | 正格。遅延は明示 | なし | 関数、タスク、列、IO で遅延を書き分ける |
| F# | `lazy expr` / `Lazy<'T>` | 正格。`lazy` で遅延を選ぶ | あり | 初回の結果をキャッシュする |
| Haskell | 言語の既定 | 遅延（非正格） | あり | 式は既定で必要になるまで評価され、結果は共有される |
| Rust | なし（クロージャ / `LazyLock` など） | 正格 | 標準ライブラリで明示 | `OnceCell` や `LazyLock` で初期化を遅らせる |
| C# | `Lazy<T>` | 正格 | あり | ファクトリを渡し、`Value` で取得する |

## まとめ

- Tsuzuri 0.1.0 には `lazy` 式も `Lazy<T>` もなく、導入計画もありません。
- 束縛と引数はその場で評価されます。関数本体や選ばれない枝は、その時点では走りません。
- 単純な遅延には `unit -> T` の関数を使います。
- 並列計算の遅延にはコールドな `Task<T>`、列の遅延には `Seq`、副作用の遅延には `IO<T>` を使います。
- 自動メモ化はないので、共有したい結果は評価後の値を変数に入れます。

## 関連項目

- [Async 式](async.md)
- [Task 式](task.md)
- [Seq](../built-in-types-and-modules/seq.md)
- [IO](../built-in-types-and-modules/io.md)
- [コンピュテーション式](../computation-expressions/computation-expressions.md)
- [所有権とムーブ](../ownership-and-memory/ownership.md)
- [言語リファレンスの目次](../index.md)
