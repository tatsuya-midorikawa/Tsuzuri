# 関数

## 関数宣言と利用

関数は `式` であるため、値として渡せます。関数は以下のような構文となります。
`constraint-list` には、型に対して実装しておくべき型クラスのリストを指定します。
また、関数名はスネークケース (snake_case) で命名することを推奨します。

```tz
def function_name :: parameter-type-list 
  constraint-list = lambda-expression
```

ラムダ式は以下のような構文となります。
複数行の場合、は改行後、インデントします。

```tz
// one line
\parameter-list [when constant-list] -> expression

// multi lines
\parameter-list ->
  expressions
```

例えば、以下のような宣言となります。

```tz
def add :: i32 -> i32 -> i32 = \x -> \y ->
    x + y
```

また、関数は自動的にカリー化されるため、上の例は、以下のように引数をスペース区切りで記述しても同じ意味となります。

```tz
def add :: i32 -> i32 -> i32 = \x y ->
    x + y
```

関数内の一番最後の値が、その関数の戻り値となります。
もし、強制的に戻り値を `unit` にしたい場合、パイプラインで `ignore` に繋げます。

```tz
def main :: i32 =
  do! IO.writeln "Hello, Tsuzuri!!"
  |> ignore
  0
```


### ジェネリック関数
ジェネリック関数は、型のプレースホルダーを `'` 始まりにして指定します。
例えば、以下のようになります。

```tz
def add :: 'T -> 'T -> 'T 
    @'T : Add = \x -> \y -> 
        x + y
```

ジェネリック関数は、型のプレースホルダーの制約に型クラスではなく、そのモジュールで実装している関数名を指定することもできます。関数名を指定する場合には、関数名に `#` プレフィクスを付与します。この場合、型のプレースホルダーを実際のモジュール名の代わりに利用して、対象の関数を呼び出せるようになります。

```Foo.tz
record Foo { num: i32 }
def value :: Foo -> i32 = \x -> x.num
```

```Main.tz
def add :: 'T -> 'T -> 'U
    @'T : (#value: 'T -> 'U) = \x -> \y -> 
        ('T.value x) + ('T.value y)

add (Foo { num: 10 }) (Foo { num: 20 })
```

## カリー化と部分適用

Tsuzuri の関数はすべてカリー化されるため、指定した数より少ない数の引数を指定した場合は、残りの引数を受け取る新しい関数を作成します。これを関数の部分適用といいます。

例えば以下のようになります。

```tz
def add :: i32 -> i32 -> i32 = \x y ->
    x + y

let add5 = add 5
let result = add5 10

do! IO.writeln result |> ignore
```

## パイプラインと関数合成

### パイプライン

パイプ演算子 `|>` は、Tsuzuri でデータを処理するときに広く使用されます。 この演算子を使用すると、柔軟な方法で関数の "パイプライン" を確立できます。 パイプライン処理を使用すると、関数呼び出しを連続する操作として連結できます。

```tz
let result = 100 |> function1 |> function2
```

次のサンプルでは、これらの演算子を使用して単純な機能パイプラインを構築する方法について説明します。

```tz
def multiply :: i64 -> i64 -> i64 = \factor value -> factor * value

let double = multiply 2
let values = [3, 5, 8]
let result = values |> Array.map double |> Array.sum
```

### 関数合成

Tsuzuri の関数は、複数の関数を合成し、新しい関数を構成できます。 2 つの関数 function1 と function2 の構成は、function1 の適用とその後の function2 の適用を表す別の関数です。

```tz
def function1 :: i32 -> i32 = \x -> x + 1
def function2 :: i32 -> i32 = \x -> x * 2
let h = function1 >> function2
let result5 = h 100
```

パイプラインとの組み合わせもできます。

```tz
def multiply :: i64 -> i64 -> i64 = \factor value -> factor * value

let double = multiply 2
let values = [3, 5, 8]
let result = values |> (Array.map double >> Array.sum)
```


## ラムダ式と高階関数

ラムダ式は Haskell と同じ `\引数 -> 本体` の形式で書きます。引数が複数の場合は、`\left right -> left + right` のように半角スペース区切りで指定するか、`\left -> \right -> left + right` のように明示的にカリー化した関数とします。半角スペース区切りで指定した場合も、明示的にカリー化したものと同じものとして判断されます。

また、`\(left, right) -> left + right` のようにも指定できますが、`(,)` はタプルを表しており、パターンマッチで分解しているだけであるため、これで 1 変数となり、カリー化はされません。

`\() -> 43` などとすることで、unit パターンを使えます。

型パラメータリストの `->` は右結合であるため、例えば `i32 -> i32 -> i32` は `i32 -> (i32 -> i32)` と等価になります。関数を引数にとる場合には、明示的に `(i32 -> i32) -> i32` とする必要があります。関数を引数に取る関数を高階関数と言います。

例えば、以下のように宣言します。

```tz
def apply :: (i64 -> i64) -> i64 -> i64 = \transform value -> transform value
let offset = 2
let increment = \value -> value + offset
apply increment 40
```

## キャプチャとメモリ

匿名関数は作成時に外側の値をキャプチャし、本体は呼び出し時に評価します。Copy 値はコピー、string などの非 Copy 値は関数値へ move します。

関数値自身は Copy です。ただしキャプチャ環境がある場合、その複製は独立したスナップショットを作る操作です。大きい配列や文字列を捕捉した関数の複製が無料になるわけではありません。関数値が不要になると環境と所有値を解放し、GC や参照カウントは使いません。

捕捉なしの関数値は環境確保を必要とせず、既知の関数の完全適用は直接呼び出しへ最適化できます。一つの小さなスカラー捕捉を関数記述子に直接格納する最適化もあります。これらは値の意味を変えない実装上の最適化です。

共有借用を捕捉した関数は元の所有者より長生きできません。排他的な ref mut T と、一回実行の Task<T> は再利用可能な捕捉環境に保存できません。排他借用を使う部分適用は、その場で完全適用し、後で呼ぶ関数値として保存しないでください。

## 再帰関数と相互再帰関数

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
