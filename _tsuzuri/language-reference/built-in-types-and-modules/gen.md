# Gen

`Gen` は、プロパティテスト（性質のテスト）のための生成器と実行の標準モジュールです。`Gen.for_all 生成器 性質` は、生成器が作る 100 個の値で性質（`bool` を返す関数）を確かめ、成り立たない値が見つかると、その値を最も単純な反例まで縮小（shrinking）し、seed と何個目の値かと反例を表示してからテストを失敗させます。QuickCheck・FsCheck・proptest・Hypothesis に相当します。

## この記事のポイント

- `test "名前" = Gen.for_all 生成器 (\値 -> 条件)` と書きます。既定は 100 個の値、`Gen.for_all_cases n` で個数を変えられます。
- 値の選び方は決定的です。seed は固定の `11400714819323198485` で、`tsuzuri test --seed N` で変えられます。同じ seed なら、ネイティブでも WASM でも、`-O0` でも `-O3` でも同じ値を試します。
- 縮小は組み込みです。整数は 0 へ、配列と文字列は短く・要素は単純に、`map`・`bind` を通した値も元の選択ごと縮みます。
- 失敗の報告は `Display` で表示します。`Display` のない型は `Gen.check` に表示関数を渡します。
- `Gen` は、プログラムが名前を書いたときだけ読み込まれる標準モジュールです。

## 性質を書く

```tsuzuri project=props file=Props.tz
test "addition commutes" =
    Gen.for_all (Gen.pair (Gen.i64()) (Gen.i64())) (\pair ->
        match pair with
        | (a, b) -> a + b == b + a)

test "sorted arrays keep their length" =
    Gen.for_all (Gen.array (Gen.range (-100) 100)) (\values ->
        let sorted = Array.sort (ref values)
        sorted.length == values.length)
```

性質は値を受け取り（move）、成り立つなら `true` を返します。比較やほかの関数で値を使い切っても構いません。失敗の報告は、選択の記録から値を作り直して表示します。

性質の中のトラップ（`assert` や範囲外の添字など）は、その場でテストを終わらせます。縮小も報告もできないので、確かめたい条件は `bool` で返します。

## 失敗の報告

性質が成り立たない値が見つかると、`Gen` は縮小した反例を標準エラーへ 3 行で書き、トラップします。`tsuzuri test` は失敗したテストの標準エラーを失敗理由の下に表示します（[Test](test.md#隔離と結果の並び)）。

```text
not ok 1 - Props below 100
  failure: trapped or terminated by signal
  property failed at case 1 of 100 (seed 11400714819323198485)
  counterexample: 100
  shrunk 59 times from 9223372036854775807
```

1 行目は何個目の値（1 始まり）で失敗したかと seed、2 行目は縮小した反例、3 行目は縮小の回数と最初に見つかった反例です。上は `Gen.for_all (Gen.i64()) (\x -> x < 100)` の報告で、反例は条件を破る最小の値 `100` まで縮みます。`--json` では、同じ内容が失敗したテストの行の `output` に入ります。

同じ seed で同じテストを動かせば、同じ値を試し、同じ反例になります。別の値で探すときは seed を変えます。

```sh
tsuzuri test . --seed 42
```

## 生成器

`Gen<'a>` は `'a` の値の生成器です。中身は不透明で、構築とフィールドの参照は `E1022` です。引数のない生成器は関数なので、`Gen.i64()` のように呼びます。

| 生成器 | 型 | 値と縮小 |
| --- | --- | --- |
| `Gen.i64()` | `Gen<i64>` | すべての `i64`。小さい桁が多くなるように選び、16 回に 1 回は最小値か最大値（`-9223372036854775808`・`9223372036854775807`）にする。0 へ縮む |
| `Gen.i32()` | `Gen<i32>` | すべての `i32`。同上（境界は `-2147483648`・`2147483647`） |
| `Gen.range low high` | `i64 -> i64 -> Gen<i64>` | `low` 以上 `high` 以下でほぼ一様。範囲の中で 0 に最も近い値へ縮む。`low > high` はトラップ |
| `Gen.bool()` | `Gen<bool>` | `false` へ縮む |
| `Gen.f64()` | `Gen<f64>` | 整数部 52 ビットまでと 20 ビットの小数部を持つ有限の値。NaN・無限大・`-0.0` は作らない。`0.0`、整数、単純な小数へ縮む |
| `Gen.char()` | `Gen<char>` | 印字できる ASCII（`' '`〜`'~'`）。`'a'` へ縮む |
| `Gen.unicode_char()` | `Gen<char>` | サロゲート以外の UTF-16 コード単位。`'a'` へ縮む |
| `Gen.string()` | `Gen<string>` | `Gen.char()` の 32 文字以下の文字列。`""` へ縮む |
| `Gen.string_of g` | `Gen<char> -> Gen<string>` | `g` の文字の 32 文字以下の文字列 |
| `Gen.array g` | `Gen<'a> -> Gen<['a]>` | 32 個以下（平均 7 個程度）の配列。短く、要素も単純に縮む |
| `Gen.array_up_to n g` | `i64 -> Gen<'a> -> Gen<['a]>` | `n` 個以下の配列 |
| `Gen.maybe g` | `Gen<'a> -> Gen<Maybe<'a>>` | `Maybe.None` へ縮む |
| `Gen.result ok error` | `Gen<'a> -> Gen<'e> -> Gen<Result<'a, 'e>>` | `Result.Ok` へ縮む |

## 組み合わせ

| 関数 | 型 | 意味 |
| --- | --- | --- |
| `Gen.constant value` | `Copy<'a> => 'a -> Gen<'a>` | いつも `value` |
| `Gen.map f g` | `('a -> 'b) -> Gen<'a> -> Gen<'b>` | `g` の値に `f` を適用する。元の値ごと縮む |
| `Gen.bind f g` | `('a -> Gen<'b>) -> Gen<'a> -> Gen<'b>` | `g` の値から次の生成器を作る（`and_then`） |
| `Gen.pair g h` | `Gen<'a> -> Gen<'b> -> Gen<('a * 'b)>` | 組 |
| `Gen.one_of gs` | `[Gen<'a>] -> Gen<'a>` | どれか 1 つの生成器の値。先頭の生成器へ縮む。空の配列はトラップ |
| `Gen.element values` | `Copy<'a> => ['a] -> Gen<'a>` | 配列の中のどれか。先頭へ縮む。空の配列はトラップ |

利用者の型の生成器は、組み合わせで作ります。

```tsuzuri project=points file=Points.tz
record Point { x: i64, y: i64 } deriving (Eq, Display)

def points :: Gen<Point>
fn points = Gen.map (\pair -> match pair with | (x, y) -> Point { x: x, y: y }) (Gen.pair (Gen.range (-10) 10) (Gen.range (-10) 10))

test "points equal themselves" = Gen.for_all (points()) (\point -> point == point)
```

## 実行する関数

| 関数 | 型 | 意味 |
| --- | --- | --- |
| `Gen.for_all g p` | `Display<'a> => Gen<'a> -> ('a -> bool) -> unit` | 100 個の値で確かめる |
| `Gen.for_all_cases n g p` | `Display<'a> => i64 -> Gen<'a> -> ('a -> bool) -> unit` | `n` 個の値で確かめる。`n` が 0 以下ならトラップ |
| `Gen.check show n g p` | `(ref 'a -> string) -> i64 -> Gen<'a> -> ('a -> bool) -> unit` | 反例を `show` で表示する。`Display` のない型（`Maybe`、`Result`、`deriving (Display)` のないレコードなど）に使う |

```tsuzuri project=checks file=Checks.tz
def show :: ref Maybe<i64> -> string
fn show value =
    match deref value with
    | Maybe.None -> "None"
    | Maybe.Some inner -> "Some " + to_string inner

test "doubles stay even" =
    Gen.check show 100 (Gen.maybe (Gen.range 0 1000)) (\value ->
        match value with
        | Maybe.None -> true
        | Maybe.Some inner -> (inner * 2) % 2 == 0)
```

`Gen` の実行関数は `test` の中で使うためのものです。テストの外で呼ぶこともでき、そのときは固定の seed を使います。

## 選択と縮小の仕組み

生成器は値を直接ではなく、「選択」の列から作ります。選択は上限のある符号なしの整数で、値の探索中は seed と値の番号から作った `Random.pcg` の乱数が選び、縮小中は記録した選択を再生します（記録が尽きたら 0）。どの生成器も、選択が小さいほど単純な値になるように作ってあります。整数は「0 に近い値からの向き」と「距離」の 2 つの選択、配列は要素の前ごとの「続けるか」の選択と要素の選択です。`Gen.i64()` と `Gen.i32()` は、距離の桁数（0〜64 ビット）を一様に選ぶほか、16 回に 1 回はその向きの最も遠い値（型の最小値か最大値）を選びます。それぞれの境界は平均 32 個に 1 回現れるので、符号の反転や絶対値があふれる境界の値も、100 個の探索でたいてい試されます。

縮小は、性質が失敗したままの、より短いか同じ長さで辞書順に小さい選択の列を探します。8 個から 1 個までの連続した選択を取り除く、各選択を 0 にする、二分探索で小さくする、を変化がなくなるまで繰り返します（性質の実行は 1 つの反例につき最大 10,000 回）。`map` や `bind` を通した値も選択から作り直すので、そのまま縮みます。

## 注意点

- 縮小の結果は、たどり着ける最も単純な反例です。性質によっては、より単純な反例が別にあることもあります。
- 既定の seed は固定なので、どの実行も同じ値を試します。CI でいつも同じ値だけになるのを避けたいなら、`--seed` を実行ごとに変えます。
- 失敗した値の表示は標準エラーです。ネイティブでも WASM でも `tsuzuri test` が失敗理由の下に出します。
- `Gen` は、プログラムのソースが識別子 `Gen` を含むときだけ読み込まれます。名前を書かないプログラムの型検査と生成コードは変わりません。

## まとめ

- `Gen.for_all 生成器 性質` は 100 個の値で性質を確かめ、失敗すると反例を縮小して表示します。
- seed は固定で、`tsuzuri test --seed N` で変えます。同じ seed なら同じ値と同じ反例です。
- 生成器は整数・`bool`・`f64`・文字・文字列・配列・`Maybe`・`Result` と、`map`・`bind`・`pair`・`one_of`・`element`・`constant` の組み合わせです。

## 関連項目

- [Test](test.md)
- [Random](random.md)
- [Bench](bench.md)
- [コンパイラの使い方](../compiler/usage.md)
- [コンパイラ オプション](../compiler/option.md)
- [言語リファレンスの目次](../index.md)
