# 関数、部分適用、クロージャー

[ドキュメントのトップ](../README.md)

関数は値として渡せます。型は `def` で明示し、その右辺のラムダ式で実装します。すべての関数はカリー化されます。

## 宣言と実装

```tsuzuri run=42
def add :: i64 -> i64 -> i64 =
    \left -> \right -> left + right

def answer :: i64 = add 20 22

answer()
```

`def name :: 型 = ラムダ式` で宣言と実装を一緒に書けます。`fn` や `let` を重ねて書く必要はありません。`=` は型と実装、`->` はラムダの引数と本体の区切りです。

引数なしの関数は `def answer :: i64 = 式` のように本体を直接書きます。`def` だけで実装がない関数は、ホスト関数を宣言する `extern def` や型クラスのメソッド宣言を除いてエラーです。

引数なしの `answer()` と、`unit` 引数を渡す `accept ()` は異なります。前者の宣言は `def answer :: i64`、後者は `def accept :: unit -> i64` です。引数なしの関数値の型は `fn() -> i64` と表せます。

`export` は型宣言側に付けます。他の Tsuzuri モジュールから呼ぶために `export` を付ける必要はありません。

## カリー化と部分適用

```tsuzuri run=42
def add :: i64 -> i64 -> i64 = \left -> \right -> left + right

let add_twenty = add 20
22 |> add_twenty
```

`add 20 22` は `(add 20) 22` と同じです。`add 20` は残りの引数を待つ `i64 -> i64` の関数値です。パイプ `value |> transform` は、値を関数へ渡します。

型の `->` は右結合なので `i64 -> i64 -> i64` は `i64 -> (i64 -> i64)` と同じです。関数を引数に取る場合は `(i64 -> i64) -> i64` のように括弧が必要です。

部分適用は組み込み関数、型クラスメソッド、union の payload 付き case、ホスト関数にも適用されます（コールバックを受け取る extern は除きます）。ただし、借用や捕捉の制限が消えるわけではありません。

## 匿名関数と高階関数

```tsuzuri run=42
def apply :: (i64 -> i64) -> i64 -> i64 = \transform value -> transform value

let offset = 2
let increment = \value -> value + offset
apply increment 40
```

匿名関数は Haskell と同じ `\引数 -> 本体` の形式で書きます。複数引数は `\left right -> left + right`、型注釈は `\(value: i64) -> value + 1` です。

`\(left, right) -> left + right` のような分解や `\() -> 42` の unit パターンも使えます。関数引数のパターンは `match` と異なり網羅性検査の対象ではなく、適用時に不一致ならトラップします。

ラムダ式はネストできます。

```tsuzuri run=9
let f = \x -> \y -> \z -> x + y + z
f 2 3 4
```

`f 2` は残りの二引数を待つ関数値、`f 2 3` は残りの一引数を待つ関数値です。`\x y z -> x + y + z` と書いても同じカリー化になります。

ローカルな `let` は単相かつ非再帰です。同じ関数値を i64 用と string 用の両方へ一般化することはできません。汎用 API には型変数を含む名前付きの `def` を使います。

```tsuzuri run=42
def twice :: i32 -> i32 = \x -> x * 2

let positive_half = \x when x > 0 -> x / 2
positive_half (twice 42)
```

引数の後に `when` 条件を書くと、条件が false の適用もパターンの不一致と同じくトラップします。`def` の名前と型の間は必ず `::` です。`def twice : i32 -> i32` のような単一の `:` は `E0002` です（`:` は `let`・`const`・フィールドの型注釈に使います）。

### 標準の高階関数

```tsuzuri run=4321
let numbers = [1, 2, 3, 4]
let total = Array.fold (\acc x -> acc + x) 0 numbers
let texts = ["a", "bcd"]
let lengths = texts |> Array.map_ref (\text -> text.length)
assert (total == 10)
assert (lengths[1] == 3)
Array.fold_back (\x digits -> digits * 10 + x) numbers 0
```

標準ライブラリの高階関数は、F# と同じく関数を最初の引数に取ります（`Array.map f xs`、`Array.fold f state xs`、`Array.fold_back f xs state`、`Seq.unfold generator state` など）。対象のコレクションが最後なので、`xs |> Array.map f` とパイプで渡せます。一覧は[配列・リスト API](../../docs/language.md#配列リスト-api)にあります。

呼び出しの引数に書いた匿名関数は、ほかの引数より後に型検査します。`xs |> f` は `xs` を先に検査するので、上の `text` の型は注釈なしで決まります。評価順序は変わりません。

## パイプラインと関数合成

```tsuzuri run=32
def multiply :: i64 -> i64 -> i64 = \factor value -> factor * value

let double = multiply 2
let values = [3, 5, 8]
let piped = values |> Array.map double |> Array.sum
let composed = values |> (Array.map double >> Array.sum)
assert (piped == composed)
piped
```

`f >> g` は `f` の後に `g` を適用する関数（`\x -> g (f x)`）、`f << g` は `g` の後に `f` を適用する関数（`\x -> f (g x)`）です。`double` は i64 を受け取るので、`values` は `[i64]` と推論します。

`Array.map double` の結果のような一時値も、共有借用の `ref` 引数へ渡せます。借用は呼び出しが戻るまでで、`Array.sum (Array.map double (ref values))` や `String.length "text"` も同じです。結果が借用を保持する呼び出しには渡せません。値を捨てて unit にするには `value |> ignore` と書きます。

## 型の関数を要求する制約

`Foo.tz`:

```tsuzuri project=function-signature-constraint file=Foo.tz
record Foo { num: i32 }
def value :: Foo -> i32 = \x -> x.num
```

`Main.tz`:

```tsuzuri project=function-signature-constraint file=Main.tz run=30
def add :: 'T -> 'T -> 'U
    @'T : (#value: 'T -> 'U) = \x -> \y ->
        ('T.value x) + ('T.value y)

add (Foo { num: 10 }) (Foo { num: 20 })
```

`@'T : (#value: 'T -> 'U)` は、`'T` の定義元モジュールに `'T -> 'U` 型の関数 `value` を要求します。本体の `'T.value x` はこの型で検査し、`Foo` に対しては `Foo.value` を静的に呼びます。型を省いた `#value` の形は[型クラスの記事](generics-and-typeclasses.md#定義元モジュールの関数を要求する)を参照してください。

## 捕捉とメモリ

匿名関数は作成時に外側の値を捕捉し、本体は呼び出し時に評価します。Copy 値はコピー、`string` などの非 Copy 値は関数値へ move します。

関数値自身は Copy です。ただし捕捉環境がある場合、その複製は独立したスナップショットを作る操作です。大きい配列や文字列を捕捉した関数の複製が無料になるわけではありません。関数値が不要になると環境と所有値を解放し、GC や参照カウントは使いません。

捕捉なしの関数値は環境確保を必要とせず、既知の関数の完全適用は直接呼び出しへ最適化できます。一つの小さなスカラー捕捉を関数記述子に直接格納する最適化もあります。これらは値の意味を変えない実装上の最適化です。

共有借用を捕捉した関数は元の所有者より長生きできません。排他的な `ref mut T` と、一回実行の `Task<T>` は再利用可能な捕捉環境に保存できません。排他借用を使う部分適用は、その場で完全適用し、後で呼ぶ関数値として保存しないでください。

## 適用と評価順序

引数は左から右へ評価し、適用は二項演算より強く結合します。式を引数にする場合は `add (base + 1) (step * 2)` と書きます。

`\left -> \right -> body` の `body` は両方の引数がそろってから評価します。一方、`\left -> { ...; \right -> body }` は最初の適用でブロックを評価し、次の関数を返します。後続引数の評価をこの段階の前へ移動してよいという規則はありません。

借用を引数にする場合、`transform ref value` と書けます。さらに引数を続けるときは `transform (ref value) other` と括ると明確です。引数型から分かる借用・再借用・参照外しは省略できますが、所有権と寿命の検査は常に行われます。

## 再帰

```tsuzuri run=5050
def rec sum :: i64 -> i64 -> i64 = \remaining total ->
    if remaining <= 0 then total
    else sum (remaining - 1) (total + remaining)

sum 100 0
```

自己再帰には `def rec`、相互再帰には明示的な再帰グループを使います。直接の自己末尾再帰は `-O0` でもループへ変換しますが、非末尾再帰や関数値を経由する再帰が必ずループになるわけではありません。深い木の処理などではスタック使用量を考慮します。

停止性は保証されません。再帰するデータ型の解放が反復実装でも、利用者の再帰関数がスタック安全になるとは限りません。

相互再帰は `def rec` に続けて、型と実装を持つ `and` を書きます。

```tsuzuri run=42
def rec even :: i64 -> bool = \value -> if value == 0 then true else odd (value - 1)
and odd :: i64 -> bool = \value -> if value == 0 then false else even (value - 1)

if even 42 then 42 else 0
```

`and` は先行グループを引き継ぐため、単独では使えません。関数名は `def rec` が束縛するため、ラムダの先頭に `\even ->` のような自己名の引数を追加しません。別々の rec 関数やモジュール間の再帰も、各関数で明示します。借用を持つ引数や、借用を保持し得る関数値の引数では、参照先フレームの再利用を避けるため直接の末尾ループ変換も行いません。

## 互換構文

宣言と実装を分離する従来の形式も受理します。

```tsuzuri run=42
def add :: i64 -> i64 -> i64
fn add left right = left + right

def identity :: i64 -> i64
let identity = \value -> value

identity (add 20 22)
```

この形式では `def` と対応する `fn` または `let` を同じファイルに一つずつ置きます。隣接や前後関係は必須ではありません。再帰指定も宣言と実装で対応させます。通常のドキュメント例とサンプルは `def ... = ラムダ式` を基本にします。

従来の `value -> expression` は互換構文として受理します。新しいコードでは `\value -> expression` を使います。旧 `fx value -> expression` は廃止され、`fx` は通常の識別子です。

従来の `fn name(value: i64) -> i64 { value }`、`name(value)`、カンマ区切りの呼び出しも受理します。新しいコードでは `def` と空白適用を推奨します。旧 `fn name :: ...` は型宣言として受理せず、`def name :: ...` へ移行します。

## 関連項目

- [ソースとインデント](lexical-and-layout.md)
- [値と定数](values-and-constants.md)
- [関数の正式な契約](../../docs/language.md#関数の宣言と適用)
