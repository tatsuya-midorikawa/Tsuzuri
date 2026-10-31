# Monoid

`Monoid` は、「空の値」と「2 つの値を 1 つにする結合的な演算」を持つ型の型クラスです。`Array.fold_monoid` は左から順に、[`Parallel.fold`](parallel.md) はチャンクに分けて畳み込みます。法則が成り立ち、`empty` と `combine` が出力などの作用を持たなければ、**2 つは同じビットの値**を返します。

## この記事のポイント

- クラスは `Monoid<'m>` です。`empty :: unit -> 'm` が空の値、`combine :: 'm -> 'm -> 'm` が演算です。呼び出しは `Monoid.empty ()` と `Monoid.combine left right` です。
- 法則は 3 つです。結合律（`combine (combine a b) c` と `combine a (combine b c)` が同じ値）、左単位元（`combine (empty ()) a` が `a` と同じ値）、右単位元（`combine a (empty ())` が `a` と同じ値）。「同じ値」はビットまで同じです。
- std は整数向けの型を `Monoids` に持ちます（`Sum`・`Product`・`MaxOf`・`MinOf`・`BitAnd`・`BitOr`・`BitXor`・`All`・`Any`）。`Monoids.Sum` のように**修飾して**書きます。`Monoids` を名前に出したプログラムだけが、このモジュールを読み込みます。
- 浮動小数点を含む型には `Monoid` が使えません（`E1039`）。加算が結合的でないので、チャンクの分け方で結果が変わるからです。
- 法則はコンパイラが検査できません。自分の `instance` には、`Gen` の性質テストを書いてください。

## 整数の合計を畳み込む

```tsuzuri run=15
let values = [Monoids.Sum { value: 1 }, Monoids.Sum { value: 2 }, Monoids.Sum { value: 3 }, Monoids.Sum { value: 4 }, Monoids.Sum { value: 5 }]
let total = Parallel.fold (ref values)
total.value
```

実行結果:

```text
15
```

`Monoids.Sum` の `empty` は 0、`combine` は `+` です。`+` は折り返すので、`i32` で `2147483647 + 1` は `-2147483648` になりますが、折り返しも結合的です。だから整数の `Sum`・`Product` は、どんなチャンクでも逐次と同じ値になります。

## Monoids の型

どの型も、中身は `value` というフィールドです。型の引数は整数型で、`Integer<'a>` の文脈が `f32`・`f64` を拒否します。

| 型 | `empty` | `combine left right` |
| --- | --- | --- |
| `Monoids.Sum<'a>` | `0` | `left.value + right.value`（折り返す） |
| `Monoids.Product<'a>` | `1` | `left.value * right.value`（折り返す） |
| `Monoids.MaxOf<'a>`（`value: Maybe<'a>`） | `Maybe.None` | 片方が `None` なら他方、両方 `Some` なら大きいほう（同じ値なら左） |
| `Monoids.MinOf<'a>`（`value: Maybe<'a>`） | `Maybe.None` | 片方が `None` なら他方、両方 `Some` なら小さいほう（同じ値なら左） |
| `Monoids.BitAnd<'a>` | 全ビットが 1 | `Bits.bit_and` |
| `Monoids.BitOr<'a>` | `0` | `Bits.bit_or` |
| `Monoids.BitXor<'a>` | `0` | `Bits.bit_xor` |
| `Monoids.All`（`value: bool`） | `true` | `left.value && right.value` |
| `Monoids.Any`（`value: bool`） | `false` | `left.value \|\| right.value` |

## 自分の型を `Monoid` にする

件数と合計を一緒に持つ型は、足し合わせても結合的です。`empty` は `let empty = \() -> ...` と書きます（`fn empty () = ...` は構文エラーです）。

```tsuzuri run=120
record Totals { count: i64, total: i64 }

instance Monoid<Totals> {
    let empty = \() -> Totals { count: 0, total: 0 }
    fn combine left right = Totals { count: left.count + right.count, total: left.total + right.total }
}

let values = [Totals { count: 1, total: 10 }, Totals { count: 1, total: 20 }, Totals { count: 1, total: 30 }]
let sequential = Array.fold_monoid (ref values)
let parallel = Parallel.fold (ref values)
sequential.total + parallel.total
```

実行結果:

```text
120
```

`Array.fold_monoid` は `empty ()` から添字の昇順に `combine` を重ねます。`Parallel.fold` は同じ値を、配列をチャンクに分けて返します。法則が成り立つので、部分結果を左から結合しても、全体を左から畳み込んでも同じです。

## 法則をテストする

`combine` が結合的であることは、次の形のテストで確かめてください。`laws` は 3 つ組について結合律と左右の単位元を調べ、`same` は 2 つの値がビットまで同じかどうかを返します。`Gen.for_all_cases` に 3 つ組の生成器を渡して、`laws` を性質として走らせます（[Gen](gen.md)）。

```tsuzuri
record Totals { count: i64, total: i64 }

instance Monoid<Totals> {
    let empty = \() -> Totals { count: 0, total: 0 }
    fn combine left right = Totals { count: left.count + right.count, total: left.total + right.total }
}

def laws :: (Monoid<'m>, Copy<'m>) => ('m -> 'm -> bool) -> ('m * ('m * 'm)) -> bool
fn laws same triple =
    match triple with
    | (a, (b, c)) ->
        let associative = same (Monoid.combine (Monoid.combine a b) c) (Monoid.combine a (Monoid.combine b c))
        let left = same (Monoid.combine (Monoid.empty ()) a) a
        let right = same (Monoid.combine a (Monoid.empty ())) a
        associative && left && right

def totals :: i64 -> Totals
fn totals total = Totals { count: 1, total: total }

def same_totals :: Totals -> Totals -> bool
fn same_totals left right = left.count == right.count && left.total == right.total

def triples :: Gen<(i64 * (i64 * i64))>
fn triples = Gen.pair (Gen.range (-1000) 1000) (Gen.pair (Gen.range (-1000) 1000) (Gen.range (-1000) 1000))

test "Totals obeys the laws" = Gen.for_all_cases 100 (triples()) (\t -> match t with | (a, (b, c)) -> laws same_totals (totals a, (totals b, totals c)))
```

可換でない演算（`x -> scale * x + shift` のような関数の合成）を持つ型もテストに入れると、結合の順序の誤りが見つかります。可換な `Sum` だけでは、左右を取り違えても結果が変わりません。

## 浮動小数点は使えない

```text
record Mean { total: f64, count: i64 }
instance Monoid<Mean> { ... }
let result = Array.fold_monoid (ref means)
```

この `Array.fold_monoid` は `E1039` で拒否されます（`no Monoid instance for Main.Mean: it holds a floating-point number ...`）。`f64` の加算は結合的でなく、`Parallel.fold` がチャンクごとに足すと、逐次の `Array.fold_monoid` と下位のビットが違うことがあるからです。
レコードのフィールド・union の payload・タプル・配列・リスト・`Vec`・`Maybe` の中にある浮動小数点（`f16`・`f32`・`f64`・`f128`、`d32`・`d64`・`d128`、浮動小数点のレーンの SIMD）も同じです。`Monoids.Sum { value: 1.5 }` を畳み込むと、`Monoids.Sum<f64>` の `E1039` です。位置は `Array.fold_monoid`・`Parallel.fold`・`Monoid.combine` を呼んだ式で、どの呼び出しにも使われない instance の宣言は拒否されません。
浮動小数点の合計が要るなら、[`Parallel.sum`](parallel.md)（チャンクの規則で順序が固定。逐次と違いうる）か、`Array.sum_pairwise`・`Array.sum_kahan`（[Math](math.md)）を使います。固定小数点（整数の `micros`）で持てば、整数の `Monoid` にできます。

## 作用を持つ instance

`Parallel.fold` は、`combine` を呼ぶ回数・順序・組み合わせが `Array.fold_monoid` と違います（チャンクごとに `empty ()` から畳み込み、最後に部分結果を `empty ()` から結合します）。`empty`・`combine` が出力・共有状態への書き込み・時刻・乱数を使うと、作用の回数や順序が変わり、結果の値も逐次と違いうります。コンパイラはこれを型検査では拒否しません。`tsuzuri check PATH --report parallel` は、`Parallel.fold` の呼び出しごとに、届く `Monoid` の関数が純粋と示せるか（`parallel_safe`）を報告します（[Parallel](parallel.md#逐次と同じ結果になる条件)）。

## 引数の順序

`Monoid.combine left right` は 2 つの値がどちらも対象なので、結合の順（左、右）に取ります。`Array.fold_monoid` と `Parallel.fold` は配列だけを取ります（[標準ライブラリの引数の順序](argument-order.md)）。

## まとめ

- `Monoid` は空の値と結合的な演算です。法則（結合律・単位元）はビットまで同じでなければなりません。
- `Array.fold_monoid` と `Parallel.fold` は、法則が成り立ち、`empty`・`combine` に出力などの作用がなければ同じ値を返します。作用があると、`combine` を呼ぶ回数と順序が違います。
- std の整数向けの型は `Monoids.Sum` のように修飾して書きます。浮動小数点を含む型は `E1039` です。
- 法則は `Gen` で自分でテストします。

## 関連項目

- [Parallel](parallel.md)
- [Array](array.md)
- [Gen](gen.md)
- [Math](math.md)
- [型クラス](../types-and-type-inference/type-classes.md)
- [診断コード](../compiler/diagnostics.md)
- [言語仕様の逐次との同一と Monoid](../../../docs/language.md#逐次との同一と-monoid)
- [言語リファレンスの目次](../index.md)
