# 標準ライブラリの引数の順序

標準ライブラリの関数は、操作する値（**対象**）を**最後の引数**に取ります。対象とは、その関数が読む・変換する・更新する値のことで、コレクション、文字列、`Maybe` や `Result`、行列、アリーナ、`Json` の書き込み器などがそうです。キー、添字、個数、コールバック、区切り文字のようなほかの引数は、その前に並べます。

この規則のおかげで、パイプ `|>` と関数合成 `>>` が、どのモジュールでも同じ形で使えます。F# の `Map`・`Set`・`Option`・`Result` の関数と同じ並びで、F# の `Array` と `List` で対象が最後にない関数だけを、最後に移しています（[Array](./array.md#引数の順序)、[List](./list.md#引数の順序)）。

## この記事のポイント

- 対象は最後の引数です。`Map.insert key value map`、`String.sub first count text`、`Maybe.bind f maybe` のように書きます。
- `xs |> M.f a` は `M.f a xs` と同じ意味です。`M.f a >> M.g b` は、対象を受け取って対象を返す関数になります。
- 対象を持たない関数（`empty`、`init`、`of_array` など）は、この規則の外にあります。
- 対象を 2 つ以上取る関数（`append`、`zip`、`union`、`mul`）は F# と同じ順に並べるので、最後の対象が最後の引数です。一方だけを書き換える関数は、書き換えられる方が最後です（`MatrixView.copy_from source target`）。
- 借用する関数と所有する関数が対になっているとき（`get` と `get_ref`、`fold` と `foldr`）は、どちらも同じ順序です。
- 同じ型の引数が並ぶ関数（`Regex.replace_all re replacement text`、`String.replace needle replacement text`）は、順序を取り違えても型検査を通ります。最後の引数が対象の `text` です。
- 例外は、資源のハンドルを先に取る関数、数値の関数、コンパイラが呼ぶ関数、テストとベンチマークの入口だけです（[例外](#例外)）。

## 対象になる値

| モジュール | 対象 | 例 |
| --- | --- | --- |
| [Array](./array.md)、[List](./list.md)、[Vec](./vec.md) | 配列、リスト、`Vec` | `Array.sub 1 2 xs`、`Vec.add 1 v` |
| [Map](./map.md)、[Set](./set.md)、[HashMap](./hashmap.md)、[HashSet](./hashset.md) | マップ、集合 | `Map.insert key value map`、`Set.contains key set` |
| [Arena](./arena.md) | アリーナ | `Arena.insert value arena`、`Arena.get handle arena` |
| [String](./string.md)、[Utf8String](./utf8string.md)、[Unicode](./unicode.md)、[Regex](./regex.md) | 文字列（`Regex` では調べる文字列で、正規表現は先に取ります） | `String.slice first last text`、`Regex.find_at re start text` |
| [Matrix](./matrix.md)、[MatrixView](./matrix-view.md)、[Tensor](./tensor.md) | 行列、窓、テンソルの view | `Matrix.get row col m`、`MatrixView.write row col value window` |
| [Maybe](./maybe.md)、[Result](./result.md)、[IO](./io.md) | `Maybe`、`Result`、`IO` の値 | `Maybe.bind f maybe`、`IO.bind f action` |
| [Json](./json.md) | 値、書き込み器、読み取り器 | `Json.write_bool flag writer`、`Json.to_utf8string_pretty indent value` |
| [Seq](./seq.md)、[Gen](./gen.md) | シーケンス、生成器 | `Seq.map f seq`、`Gen.map f gen` |

## パイプと合成

対象が最後なので、同じ書き方がモジュールをまたいで使えます。

```tsuzuri run=stock%3D1%20label%3Dababab%20parsed%3D40%20shouted%3Dhihi
let stock =
    HashMap.empty()
    |> HashMap.insert "apple" 3
    |> HashMap.insert "pear" 5
    |> HashMap.remove "apple"
let label = "ab" |> String.repeat 3
let parsed =
    Maybe.Some 20
    |> Maybe.bind (\n -> if n > 10 then Maybe.Some (n * 2) else Maybe.None)
    |> Maybe.default_value 0
let shout = String.trim >> String.repeat 2
let shouted = "  hi " |> shout
$"stock={HashMap.length (ref stock)} label={label} parsed={parsed} shouted={shouted}"
```

実行結果:

```text
stock=1 label=ababab parsed=40 shouted=hihi
```

`String.trim >> String.repeat 2` は、文字列を受け取って文字列を返す関数です。`Result` を連ねるときも、`Result.bind f` を `|>` の右に並べます。

## F# との対応

`Map`・`Set`・`Option`・`Result` と `String.replicate` は、F# と同じ順序です。F# で対象が最後にない関数だけが、Tsuzuri では最後に移っています。

| Tsuzuri | F# |
| --- | --- |
| `Map.insert key value map` | `Map.add key value table` |
| `Map.get key map` | `Map.tryFind key table` |
| `Map.remove key map` | `Map.remove key table` |
| `Set.insert key set` | `Set.add value set` |
| `Set.contains key set` | `Set.contains value set` |
| `Maybe.bind f maybe` | `Option.bind f option` |
| `Maybe.or_else f maybe` | `Option.orElseWith f option` |
| `Result.bind f result` | `Result.bind f result` |
| `String.repeat count text` | `String.replicate count str` |
| `Array.get index array`、`Array.set index value array` | `Array.get array index`、`Array.set array index value`（F# では配列が先です。[Array の引数の順序](./array.md#引数の順序)） |

## 対象を借用で渡すときの評価順序

対象は最後に借りられるので、それより前の引数の式は、対象を読めます。

```tsuzuri run=values%3D%5B20%2C%202%2C%203%5D
let mut values = [1, 2, 3]
Array.write 0 (values[1] * 10) values
$"values={values}"
```

実行結果:

```text
values=[20, 2, 3]
```

パイプで渡すと、対象は先に借りられます。後ろの引数の式で同じ対象を読むと `E1014` です。

```text
let mut values = [1, 2, 3]
values |> Array.write 0 (values[1] * 10)
```

## 例外

次の 4 種類だけが、対象を最後に取りません。

| 種類 | 関数 | 順序 | 理由 |
| --- | --- | --- | --- |
| 資源 | [File](./file.md)、[Net](./net.md) のソケット、[Process](./process.md)、[Atomic](./atomic.md)、[Mutex](./mutex.md)、[Channel](./channel.md)、[Owned](./owned.md)、[Rc と Arc](./rc.md)、`Task` | ハンドルやパスが先で、データ、時間制限、コールバックが後です（`File.write handle bytes`、`Channel.send (ref sender) item`、`Mutex.with_lock (ref lock) callback`） | 状態を持つ資源への操作です。メソッドの受け手が先に来る .NET の呼び出しと同じ形にしています。 |
| 数値 | [Int](./int.md)、[Math](./math.md)、[Simd](./simd.md)、[Char](./char.md)、[Utf8Char](./utf8char.md) | 数式の順です（`Math.pow base exponent`） | コレクションを取らず、被演算子の順が意味を持ちます。 |
| コンパイラが呼ぶ関数 | 計算式ビルダーの大文字で始まる操作（`Bind`、`Return`、`For` など）、`derive` が生成するコードが呼ぶ `Json.decode_field` と `Json.case_index` | プロトコルが決めます | 利用者は直接呼びません。 |
| テストとベンチマークの入口 | [Gen](./gen.md) の `for_all`・`for_all_cases`・`check`、[Test](./test.md)、[Bench](./bench.md) | 生成器や準備が先で、本体のラムダが最後です | 結果を返さないので、パイプでつなぎません。 |

引数が 1 つの関数や、対象を取らない関数は、順序の問題が起こりません。

## 自分の関数を書くとき

利用者の関数も、対象を最後の引数にすると、`|>` と `>>` でそのまま標準ライブラリの関数とつなげられます。

```tsuzuri run=total%3D12
def scale :: i64 -> ref [i64] -> [i64] = \factor values -> Array.map (\x -> x * factor) values
let total = [1, 2, 3] |> scale 2 |> Array.sum
$"total={total}"
```

実行結果:

```text
total=12
```
