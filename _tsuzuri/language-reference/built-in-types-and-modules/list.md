# List

`List` は、不変なリストです。型は `[|T|]` です。`List<T>` という別名はありません。要素は連続した領域（chunk）に並びます。`new`・リテラル・複製・`Array.to_list`・`List.map` のように一度に作ったリストは、要素数ちょうどの 1 つの chunk です。`List.cons` で伸ばしたリストは chunk の連なりです。先頭への追加と先頭の分解は $O(1)$、長さ `.length` もリストに保存されているので $O(1)$ です。添字は、一度に作ったリストなら $O(1)$ で、`List.cons` で伸ばしたリストでは、その要素より前の chunk の数に比例します（100 万要素で約 260 個です）。

`List` モジュールは、F# の `List` モジュールと同じ関数を、同じ名前（snake_case）で提供します。リストはどの関数でも最後の引数なので、`List.map f xs` や `xs |> List.filter p` のように F# の書き方がそのまま使え、`List.sort >> List.rev` のような関数合成もできます。F# でリストが最後にない 4 つの関数だけは、同じ理由でリストを最後に移しました（[引数の順序](#引数の順序)）。所有権のために F# と違う点（失敗の扱い、`Copy`、`r` を付けた関数、リストを共有しないこと）は [F# の List モジュールとの対応](#f-の-list-モジュールとの対応) にまとめました。

再帰やパターンマッチに向きます。リストの所有者は常に 1 つで、リストどうしが要素を共有することはありません。F# と記号が逆で、Tsuzuri の `[|1, 2|]` がリスト、`[1, 2]` が配列です。

## この記事のポイント

- 型は `[|T|]`。要素も構造も生成後は不変です。`List<T>` とは書けません。
- F# の `List` モジュールの関数（117 個。廃止予定の `nth` を除く）がすべてあります。名前は snake_case です。リストは常に最後の引数で、`|>` でも `>>` でもつなげられます。F# でリストが最後にない関数（`fold_back`、`fold_back2`、`scan_back`、`map_fold_back`）だけ、リストを最後に移しています。
- F# が例外を投げる場面（空リスト、範囲外、長さの不一致、見つからない `find`）はトラップです。`try_item` や `try_find` のように `try_` が付く関数は `Maybe` を返します。
- コールバックが要素を値で受ける関数（`filter`、`exists`、`fold` など）は、要素が `Copy` であることを要求します。`mapr`、`foldr`、`existsr` のように `r` を付けた関数は要素を借用するので、文字列やリストのリストにも使えます。
- リテラル `[|1, 2, 3|]`、空リスト `[||]`、`new [|T|](n, \i -> ...)`、`List.cons` で作ります。`List.length [|1, 2, 3|]` のように、リテラルをそのまま引数に渡せます。
- `::` はパターン専用です。式の `1 :: xs` は `E0002` なので、先頭へ足すときは `List.cons` を使います。
- 添字は一度に作ったリストなら $O(1)$、`List.cons` で伸ばしたリストなら先行する chunk の数に比例します。長さは $O(1)$ です。`List` の関数はリストを添字で回しません。`for` で走査するか、いったん配列へ複写して処理します。ランダムアクセスが中心なら [Array](./array.md) や [Vec](./vec.md) も使えます。
- 走査には直接の `for ... in` ループが利用でき、一時メモリを確保せず $O(n)$ で効率的に走査します。
- 要素数が定数の小さなリストを 1 つの関数の中で作って使って捨てる処理は、以前の連結ノードの表現（LLVM が確保を全部消して 1 回約 1 ns）より遅く、25〜80 ns かかることがあります。要素数が実行時に決まるリストは速くなりました。詳しくは[小さな定数サイズのリストの性能](#小さな定数サイズのリストの性能既知の制限)です。

## 基本の書き方と生成

### リテラルと空リスト

リストリテラルはパイプ付きの角括弧 `[|...|]` で囲みます。空リストは `[||]` と記述します。

```tsuzuri run=len%3D3%20first%3D10
let numbers = [|10, 20, 30|]
let len = numbers.length
let first = numbers[0]
$"len={len} first={first}"
```

実行結果:

```text
len=3 first=10
```

空リスト `[||]` は要素型を推論できる文脈で記述します。推論できない場合は `let empty: [|i64|] = [||]` のように型注釈を付与します。

### コンスによる要素の追加

先頭に足すには `List.cons` を使います。`List.cons x xs` は所有するリスト `xs` を消費し、先頭の chunk に空きがあればその slot に `x` を書き、なければ新しい chunk を足して、先頭が `x` のリストを返します。要素が Copy の `xs` をあとで使うときは、`xs` が複製されます。

```tsuzuri run=new_len%3D4%20head%3D5
let original = [|10, 20, 30|]
let extended = List.cons 5 original
let new_len = extended.length
let head = extended[0]
$"new_len={new_len} head={head}"
```

> [!NOTE]
> `::` はパターン専用です。`5 :: xs` のような式は `E0002` になります。先頭へ足すときは `List.cons` を使います。

実行結果:

```text
new_len=4 head=5
```

### 動的初期化 `new [|T|](count, initializer)`

要素数を指定して関数からリストを生成するには `new [|T|](count, initializer)` を使用します。

```tsuzuri run=generated%3D%5B%7C0%2C%202%2C%204%2C%206%7C%5D
let evens = new [|i64|](4, \i -> i * 2)
$"generated={evens}"
```

実行結果:

```text
generated=[|0, 2, 4, 6|]
```

### 生成関数

`List.init` は `new [|T|](count, initializer)` と同じです。`replicate` は同じ値を並べ、`unfold` は `None` を返すまで要素を作り、`singleton` は要素が 1 つのリストです。空のリストは、関数 `List.empty()` でも作れます。配列からは `List.of_array`、`Seq` からは `List.of_seq` で作ります。

```tsuzuri run=squares%3D%5B%7C0%2C%201%2C%204%2C%209%2C%2016%7C%5D%20sevens%3D%5B%7C7%2C%207%2C%207%7C%5D%20powers%3D%5B%7C1%2C%202%2C%204%2C%208%2C%2016%2C%2032%2C%2064%7C%5D%20from_array%3D%5B%7C1%2C%202%2C%203%7C%5D%20single%3D%5B%7C9%7C%5D
let squares = List.init 5 (\i -> i * i)
let sevens = List.replicate 3 7
let powers = List.unfold (\n -> if n > 100 then None else Some (n, n * 2)) 1
let from_array = List.of_array [1, 2, 3]
$"squares={squares} sevens={sevens} powers={powers} from_array={from_array} single={List.singleton 9}"
```

実行結果:

```text
squares=[|0, 1, 4, 9, 16|] sevens=[|7, 7, 7|] powers=[|1, 2, 4, 8, 16, 32, 64|] from_array=[|1, 2, 3|] single=[|9|]
```

## パターンマッチと分解

パターンの `::` で、先頭と残りに分けられます。式としてのコンスは書けません。

```tsuzuri run=sum%3D60
def rec sum_list :: [|i64|] -> i64
fn rec sum_list xs =
    match xs with
    | head :: tail -> head + sum_list tail
    | [||] -> 0

let values = [|10, 20, 30|]
let total = sum_list values
$"sum={total}"
```

実行結果:

```text
sum=60
```

リストのパターンには以下が使用できます：
- `[||]`: 空リストにマッチ
- `head :: tail`: 先頭要素 `head` と残りのリスト `tail` に分解
- `[|x|]`: ちょうど 1 要素のリストにマッチ
- `[|x, y|]`: ちょうど 2 要素のリストにマッチ

関数で分けたいときは、先頭を `List.head xs`（空リストはトラップ）か `List.try_head xs`（空リストは `None`）で読み、先頭以外を `List.tail xs` で取ります。`List.tail` は `xs` を消費し、空リストに適用すると安全にトラップします。

## 添字アクセスと計算量上の注意

添字 `xs[i]` や `List.item i xs` で要素を読めます。

```tsuzuri run=third%3D30
let values = [|10, 20, 30, 40|]
let third = values[2]
$"third={third}"
```

実行結果:

```text
third=30
```

> [!WARNING]
> **添字は先行する chunk の数に比例します**
> 一度に作ったリストなら $O(1)$ ですが、`List.cons` で伸ばしたリストでは、その要素より前の chunk を先頭からたどります。ループの中で `xs[i]` を繰り返すと、`List.cons` で伸ばしたリストでは遅くなります。走査は `for ... in` か `List.fold` / `List.map` を使います。長さ `.length` は $O(1)$ です。

## 変換・畳み込み・探索

F# の `List` と同じ道具が揃っています。関数を先に、リストを最後に渡すので、`|>` でつなげます。コールバックは要素を値で受け取ります。

```tsuzuri run=squares%3D%5B%7C1%2C%209%2C%2025%7C%5D%20sum%3D35
let squares =
    [|1, 2, 3, 4, 5|]
    |> List.filter (\x -> x % 2 == 1)
    |> List.map (\x -> x * x)
$"squares={squares} sum={List.sum squares}"
```

実行結果:

```text
squares=[|1, 9, 25|] sum=35
```

`find` は見つからないとトラップします。`try_find` は `Maybe` を返し、`find_index` と `try_find_index` は位置を返します。

```tsuzuri run=found%3D15%20missing%3Dnone%20first_23%3D23%20at%3D4
let values = [|4, 8, 15, 16, 23, 42|]
let found = match List.try_find (\x -> x > 10) values with
| Some v -> to_string v
| None -> "none"
let missing = match List.try_find (\x -> x > 100) values with
| Some v -> to_string v
| None -> "none"
let index = List.find_index (\x -> x == 23) values
$"found={found} missing={missing} first_23={List.find (\x -> x > 20) values} at={index}"
```

実行結果:

```text
found=15 missing=none first_23=23 at=4
```

分割、グループ化、重複の除去は次のとおりです。`group_by` と `count_by` のキーは、最初に現れた順に並びます。各グループの中は入力の順です。

```tsuzuri run=groups%3D%5B%7C(0%2C%20%5B%7C3%2C%209%2C%206%7C%5D)%2C%20(1%2C%20%5B%7C1%2C%204%2C%201%7C%5D)%2C%20(2%2C%20%5B%7C5%2C%202%7C%5D)%7C%5D%20counts%3D%5B%7C(0%2C%203)%2C%20(1%2C%203)%2C%20(2%2C%202)%7C%5D%20big%3D%5B%7C4%2C%205%2C%209%2C%206%7C%5D%20small%3D%5B%7C3%2C%201%2C%201%2C%202%7C%5D%20distinct%3D%5B%7C3%2C%201%2C%204%2C%205%2C%209%2C%202%2C%206%7C%5D
let words = [|3, 1, 4, 1, 5, 9, 2, 6|]
let groups = List.group_by (\x -> x % 3) words
let counts = List.count_by (\x -> x % 3) words
match List.partition (\x -> x > 3) words with
| (big, small) -> $"groups={groups} counts={counts} big={big} small={small} distinct={List.distinct words}"
```

実行結果:

```text
groups=[|(0, [|3, 9, 6|]), (1, [|1, 4, 1|]), (2, [|5, 2|])|] counts=[|(0, 3), (1, 3), (2, 2)|] big=[|4, 5, 9, 6|] small=[|3, 1, 1, 2|] distinct=[|3, 1, 4, 5, 9, 2, 6|]
```

`distinct`、`distinct_by`、`except`、`count_by`、`group_by` は、F# と同じく NaN を 1 つの値として扱います。`HashMap` は、自分自身と等しくないキー（NaN、または NaN を含むタプルやレコード）でトラップしますが、これらの関数はそのようなキーをハッシュ表に入れません。代わりに `Hash.hash` が同じキーを同じものとして扱います。`Hash.hash` はどの NaN にも同じ値を返すので、NaN はすべて 1 つのキー（1 つのグループ）になり、`(0, nan)` と `(1, nan)` は別のキーのままです。最初に現れた位置に置かれ、NaN が 1 つだけのリストでもトラップしません。ハッシュ値の一致だけで同じキーとみなすので、64 bit の値が衝突する別のキーは同じキーになります（実際には起きません）。

`scan`、`unfold`、`map_fold` は、途中の状態や生成を扱います。

```tsuzuri run=running%3D%5B%7C0%2C%203%2C%204%2C%208%2C%209%2C%2014%7C%5D%20doubled%3D%5B%7C5%2C%2010%2C%2020%2C%2040%7C%5D%20tens%3D%5B%7C30%2C%2010%2C%2040%2C%2010%2C%2050%7C%5D%20total%3D14
let readings = [|3, 1, 4, 1, 5|]
let running = List.scan (\total x -> total + x) 0 readings
let doubled = List.unfold (\n -> if n > 40 then None else Some (n, n * 2)) 5
match List.map_fold (\total x -> (x * 10, total + x)) 0 readings with
| (tens, total) -> $"running={running} doubled={doubled} tens={tens} total={total}"
```

実行結果:

```text
running=[|0, 3, 4, 8, 9, 14|] doubled=[|5, 10, 20, 40|] tens=[|30, 10, 40, 10, 50|] total=14
```

`map`、`mapi`、`map2`、`choose`、`map_fold`、`unfold`、`partition_with`、`init`、`of_seq` が返すリストの要素は、`Copy` でなくてかまいません。文字列のリストも作れます。

```tsuzuri run=labels%3D%5B%7C%220%3A1%22%2C%20%221%3A2%22%2C%20%222%3A3%22%7C%5D%20big%3D%5B%7C%22big2%22%2C%20%22big3%22%7C%5D
let numbers = [|1, 2, 3|]
let labels = List.mapi (\i x -> $"{i}:{x}") numbers
let big = List.choose (\x -> if x > 1 then Some $"big{x}" else None) numbers
$"labels={labels} big={big}"
```

実行結果:

```text
labels=[|"0:1", "1:2", "2:3"|] big=[|"big2", "big3"|]
```

2 つのリストを組み合わせる関数（`map2`、`fold2`、`zip`、`unzip`、`all_pairs` など）は、長さが違うとトラップします。トラップの前にコールバックを何回呼ぶかは関数ごとに決まっています。詳しくは [2 つのリストを取る関数](#2-つのリストを取る関数) です。

```tsuzuri run=sums%3D%5B%7C11%2C%2022%2C%2033%7C%5D%20dot%3D140%20pairs%3D%5B%7C(1%2C%2010)%2C%20(2%2C%2020)%2C%20(3%2C%2030)%7C%5D%20left%3D%5B%7C1%2C%202%2C%203%7C%5D%20right%3D%5B%7C10%2C%2020%2C%2030%7C%5D%20all%3D%5B%7C(1%2C%203)%2C%20(1%2C%204)%2C%20(2%2C%203)%2C%20(2%2C%204)%7C%5D
let a = [|1, 2, 3|]
let b = [|10, 20, 30|]
let sums = List.map2 (\x y -> x + y) a b
let dot = List.fold2 (\acc x y -> acc + x * y) 0 a b
let pairs = List.zip a b
match List.unzip pairs with
| (left, right) -> $"sums={sums} dot={dot} pairs={pairs} left={left} right={right} all={List.all_pairs [|1, 2|] [|3, 4|]}"
```

実行結果:

```text
sums=[|11, 22, 33|] dot=140 pairs=[|(1, 10), (2, 20), (3, 30)|] left=[|1, 2, 3|] right=[|10, 20, 30|] all=[|(1, 3), (1, 4), (2, 3), (2, 4)|]
```

窓、塊、隣り合う組は、新しいリストのリストを返します。

```tsuzuri run=windows%3D%5B%7C%5B%7C1%2C%202%2C%203%7C%5D%2C%20%5B%7C2%2C%203%2C%204%7C%5D%2C%20%5B%7C3%2C%204%2C%205%7C%5D%7C%5D%20chunks%3D%5B%7C%5B%7C1%2C%202%7C%5D%2C%20%5B%7C3%2C%204%7C%5D%2C%20%5B%7C5%7C%5D%7C%5D%20pairs%3D%5B%7C(1%2C%202)%2C%20(2%2C%203)%2C%20(3%2C%204)%2C%20(4%2C%205)%7C%5D%20parts%3D%5B%7C%5B%7C1%2C%202%7C%5D%2C%20%5B%7C3%2C%204%7C%5D%2C%20%5B%7C5%7C%5D%7C%5D
let xs = [|1, 2, 3, 4, 5|]
let windows = List.windowed 3 xs
let chunks = List.chunk_by_size 2 xs
let pairs = List.pairwise xs
let parts = List.split_into 3 xs
$"windows={windows} chunks={chunks} pairs={pairs} parts={parts}"
```

実行結果:

```text
windows=[|[|1, 2, 3|], [|2, 3, 4|], [|3, 4, 5|]|] chunks=[|[|1, 2|], [|3, 4|], [|5|]|] pairs=[|(1, 2), (2, 3), (3, 4), (4, 5)|] parts=[|[|1, 2|], [|3, 4|], [|5|]|]
```

先頭からの取り出し、連結、反転、更新は、どれも新しいリストを返します。元のリストは変わりません。

```tsuzuri run=taken%3D%5B%7C0%2C%201%7C%5D%20skipped%3D%5B%7C4%2C%205%7C%5D%20appended%3D%5B%7C0%2C%201%2C%204%2C%205%7C%5D%20reversed%3D%5B%7C5%2C%204%2C%201%2C%200%7C%5D%20inserted%3D%5B%7C0%2C%209%2C%201%2C%202%2C%203%2C%204%2C%205%7C%5D%20removed%3D%5B%7C0%2C%201%2C%203%2C%204%2C%205%7C%5D%20replaced%3D%5B%7C7%2C%201%2C%202%2C%203%2C%204%2C%205%7C%5D%20spliced%3D%5B%7C0%2C%201%2C%208%2C%208%2C%202%2C%203%2C%204%2C%205%7C%5D
let xs = [|0, 1, 2, 3, 4, 5|]
let taken = List.take 2 xs
let skipped = List.skip 4 xs
let appended = List.append taken skipped
let reversed = List.rev appended
let inserted = List.insert_at 1 9 xs
let removed = List.remove_at 2 xs
let replaced = List.update_at 0 7 xs
let spliced = List.insert_many_at 2 [|8, 8|] xs
$"taken={taken} skipped={skipped} appended={appended} reversed={reversed} inserted={inserted} removed={removed} replaced={replaced} spliced={spliced}"
```

実行結果:

```text
taken=[|0, 1|] skipped=[|4, 5|] appended=[|0, 1, 4, 5|] reversed=[|5, 4, 1, 0|] inserted=[|0, 9, 1, 2, 3, 4, 5|] removed=[|0, 1, 3, 4, 5|] replaced=[|7, 1, 2, 3, 4, 5|] spliced=[|0, 1, 8, 8, 2, 3, 4, 5|]
```

## ソートと集計

並べ替えは安定です。`sort` と `sort_descending` は `Ord` で、`sort_by` と `sort_by_descending` は要素から取ったキーで、`sort_with` は比較関数（負・ゼロ・正の `i64` を返す）で並べます。`sort_by` はキーを要素ごとに 1 度だけ計算します。

NaN の位置は、関数によって決まる範囲が違います。

| 関数 | NaN の扱い |
| --- | --- |
| `sort`、`sort_descending`、`sort_with` | 要素の型が `f32` と `f64` のときだけ、昇順でも降順でも NaN を通常の数値の後ろに置き、NaN どうしは入力の順のままにします。`f16`、`f128`、`decimal` の NaN には、この特別扱いがなく、`Ord` の比較だけで決まるので、位置は決まっていません |
| `sort_by`、`sort_by_descending` | キーの型を問わず、自分自身と等しくないキー（NaN）を持つ要素を、昇順でも降順でも最後に、入力の順のまま置きます |

キーがタプルやレコードのときは、`Ord` で順序が決まる 2 つの要素はその順序のままです（`(0, nan)` と `(1, 0.0)` は 0 < 1 で、`sort` と同じです）。等しくも、どちらが小さいとも言えない 2 つのうち、自分自身と等しくないキー（NaN を含むもの）を持つほうが後ろになります。

```tsuzuri run=ascending%3D%5B%7C1%2C%201%2C%202%2C%203%2C%204%2C%205%2C%206%2C%209%7C%5D%20descending%3D%5B%7C9%2C%206%2C%205%2C%204%2C%203%2C%202%2C%201%2C%201%7C%5D%20by_distance%3D%5B%7C5%2C%204%2C%206%2C%203%2C%202%2C%201%2C%201%2C%209%7C%5D%20ranked%3D%5B%7C9%2C%206%2C%205%2C%204%2C%203%2C%202%2C%201%2C%201%7C%5D
let xs = [|3, 1, 4, 1, 5, 9, 2, 6|]
let ascending = List.sort xs
let descending = List.sort_descending xs
let by_distance = List.sort_by (\x -> (x - 5) * (x - 5)) xs
let ranked = List.sort_with (\a b -> b - a) xs
$"ascending={ascending} descending={descending} by_distance={by_distance} ranked={ranked}"
```

実行結果:

```text
ascending=[|1, 1, 2, 3, 4, 5, 6, 9|] descending=[|9, 6, 5, 4, 3, 2, 1, 1|] by_distance=[|5, 4, 6, 3, 2, 1, 1, 9|] ranked=[|9, 6, 5, 4, 3, 2, 1, 1|]
```

`sum`、`average`、`min`、`max` と、キーを取る `sum_by`、`average_by`、`min_by`、`max_by` があります。`min` と `max` は、同じ値のうち最初の要素を返します。空のリストでは、`sum` は 0、`average`、`min`、`max`、`reduce` はトラップします。

```tsuzuri run=total%3D14%20mean%3D3%20smallest%3D1%20largest_square%3D-7
let xs = [|3, 1, 4, 1, 5|]
let total = List.sum xs
let mean = List.average [|1.0, 2.0, 6.0|]
let smallest = List.min xs
let largest_square = List.max_by (\x -> x * x) [|-7, 3, 5|]
$"total={total} mean={mean} smallest={smallest} largest_square={largest_square}"
```

実行結果:

```text
total=14 mean=3 smallest=1 largest_square=-7
```

整数の `sum` と `sum_by` は、桁があふれると折り返します（F# は `OverflowException`）。言語の整数の `+` と同じです。

## 乱数

F# の `random*` の 12 関数は、乱数の取り方で 3 つに分かれます。

| 形 | 乱数の源 | 例 |
| --- | --- | --- |
| `_by` | `unit -> f64` の関数。`[0, 1)` の数を返す。それ以外はトラップ | `List.random_choice_by randomizer xs` |
| `_with` | `Random.Pcg` の生成器。進んだ生成器を結果と一緒に返す | `List.random_shuffle_with generator xs` は `(リスト * 生成器)` |
| 接尾辞なし | OS の乱数で種を作る。実行するまで何も起きない `IO<Result<_, Os.Error>>` | `List.random_shuffle xs` |

シャッフルは Fisher-Yates、標本は先頭から `count` 個を交換して選ぶ方式です。選択は、乱数が `r` なら添字 `floor(r * 長さ)` です。同じ生成器からは同じ結果が出ます。結果は [Array](./array.md#乱数) の同名の関数と同じです。

```tsuzuri run=first%3D10%20last%3D50%20shuffled%3D%5B%7C50%2C%2010%2C%2020%2C%2030%2C%2040%7C%5D%20sample_size%3D3
let xs = [|10, 20, 30, 40, 50|]
let first = List.random_choice_by (\() -> 0.0) xs
let last = List.random_choice_by (\() -> 0.99) xs
let shuffled = List.random_shuffle_by (\() -> 0.99) xs
let generator = Random.pcg 42i64u 54i64u
match List.random_sample_with generator 3 xs with
| (sample, _) ->
    let size = sample.length
    $"first={first} last={last} shuffled={shuffled} sample_size={size}"
```

実行結果:

```text
first=10 last=50 shuffled=[|50, 10, 20, 30, 40|] sample_size=3
```

接尾辞のない形は、リストを値で受け取ります。既定の WASM ターゲットには OS の API がないので、接尾辞のない形を使うと `E2000` です。`--wasm-host wasi` を付けるか、`_by` や `_with` の形を使います。

```tsuzuri run=length%3D5%20sum%3D15
def main :: unit -> i32 = \() ->
    let! shuffled = List.random_shuffle [|1, 2, 3, 4, 5|]
    match shuffled with
    | Ok values ->
        let size = values.length
        let total = List.sum values
        do! IO.write_line $"length={size} sum={total}"
    | Error _ -> do! IO.write_line "no entropy"
    0
```

実行結果:

```text
length=5 sum=15
```

## F# の List モジュールとの対応

[F# の `List` モジュール](https://fsharp.github.io/fsharp-core-docs/reference/fsharp-collections-listmodule.html)の関数は、廃止予定の `nth`（`item` を使います）を除いてすべてあります。名前は camelCase を snake_case にしたもの（`tryFind` は `try_find`、`foldBack2` は `fold_back2`、`splitInto` は `split_into`）です。リストを最後に渡す点も F# と同じで、F# でリストが最後にない 4 つの関数だけ、リストを最後に移しています（[引数の順序](#引数の順序)）。コンピュテーション式やパイプラインで、F# と同じ形に書けます。

### 型と失敗

| F# | Tsuzuri |
| --- | --- |
| `int`（添字、個数、位置） | `i64`。比較関数（`sort_with`、`compare_with`）の戻り値も `i64` |
| 例外（`ArgumentException`、`InvalidOperationException`） | トラップ。`try_` 付きの関数は `Maybe` を返します |
| `option` | `Maybe` |
| `Choice<'T1, 'T2>`（`partition_with` の分け先） | `Result<'b, 'c>`。`Ok` が前のリスト、`Error` が後ろのリストになります |
| `seq<'T>` の引数（`concat`、`except`、`insert_many_at`、`transpose`） | リスト（`concat` と `transpose` はリストのリスト） |
| `seq<'T>` を作る `to_seq`、受け取る `of_seq` | `Seq<'a>` |
| 配列（`of_array`、`to_array`） | 配列 `[T]` |
| `Random`（`random_*_with`） | `Random.Pcg`。生成器は値なので、進んだものを結果と一緒に返します |
| `Random.Shared`（`random_choice` など接尾辞なし） | `IO<Result<_, Os.Error>>`。OS の乱数で種を作ります |
| `float`（`average`、`average_by`） | `f64` など浮動小数点数（`Float`）。整数は F# と同じく使えません |
| 整数の `sum`、`sum_by`（`Checked.(+)`。桁あふれで `OverflowException`） | 桁あふれで折り返します。言語の整数の `+` と同じで、トラップにはなりません |

トラップになる F# の例外は、空リスト（`head`、`last`、`reduce`、`min`、`max`、`average`、`tail` など）、負の個数（`init`、`replicate`、`take`、`chunk_by_size`、`windowed`、`split_into` など）、範囲外（`item`、`update_at`、`insert_at`、`remove_at`、`split_at`、`take`、`skip`）、長さの不一致（`zip`、`map2`、`fold2`、`exists2`、`transpose` など）、見つからない検索（`find`、`find_back`、`find_index`、`pick`）、順列でない `permute` です。

`remove_many_at` に負の個数を渡すと、F# のリストは何もせずにリストを返しますが、Tsuzuri は `Array.remove_many_at` と同じくトラップします。

### 引数の順序

操作するリストは、必ず最後の引数です。`xs |> List.f a` のようにパイプで渡せ、`List.f a >> List.g b` のように一部だけ渡した関数を合成できます。F# の `List` も多くの関数がそうですが、次の 4 つはリストが最後にありません。Tsuzuri では、ほかの引数の順を保ったまま、リストを最後に移しました。標準ライブラリのほかのモジュールも同じ規則です（[標準ライブラリの引数の順序](./argument-order.md)）。

| 関数 | F# | Tsuzuri |
| --- | --- | --- |
| `fold_back` | `List.foldBack folder list state` | `List.fold_back folder state list` |
| `fold_back2` | `List.foldBack2 folder list1 list2 state` | `List.fold_back2 folder state list1 list2` |
| `scan_back` | `List.scanBack folder list state` | `List.scan_back folder state list` |
| `map_fold_back` | `List.mapFoldBack mapping list state` | `List.map_fold_back mapping state list` |

`folder` と `mapping` の引数の順は F# のままです（`fold_back` の `folder` は `'a -> 'state -> 'state`）。`fold` と同じく、初期値は関数の直後に置きます。

```tsuzuri run=digits%3D321%20ordered%3D%5B%7C9%2C%204%7C%5D
let digits = [|1, 2, 3|] |> List.fold_back (\x acc -> acc * 10 + x) 0
let sort_down = List.sort >> List.rev
let ordered = [|4, 9, 1|] |> sort_down |> List.take 2
$"digits={digits} ordered={ordered}"
```

実行結果:

```text
digits=321 ordered=[|9, 4|]
```

`>>` で合成した関数は、前の関数が返したリストを、次の関数が借りて受け取ります。借りた一時のリストの中を指す参照を返す関数（`minr`、`findr` など）につなぐと、参照が一時のリストより長生きしてしまうので `E1013` です。

### 2 つのリストを取る関数

F# のリストの関数は、2 つのリストを先頭から組にして順に見ていき、長さの違いには、短い方が尽きたときに気づきます。Tsuzuri の `List` も同じ順序で動きます。

| 関数 | 長さが違うとき |
| --- | --- |
| `iter2`、`iteri2`、`map2`、`mapi2`、`map3`、`fold2` | 2 つのリストが共有する組すべてにコールバックを呼んでから、トラップします |
| `exists2`、`forall2` | 答えが決まる組（`exists2` は述語が真、`forall2` は偽）があれば、そこで結果を返します。決まらないまま短い方が尽きたらトラップします |
| `fold_back2` | コールバックを呼ぶ前に、長さを調べてトラップします |
| `zip`、`zip3`、`unzip` など、コールバックのない関数 | トラップします |

```tsuzuri run=early%3Dtrue%20late%3Dfalse
let early = List.exists2 (\a b -> a == b) [|1, 2, 3|] [|1|]
let late = List.forall2 (\a b -> a == b) [|1, 2, 3|] [|9|]
$"early={early} late={late}"
```

実行結果:

```text
early=true late=false
```

[Array](./array.md) の `exists2` や `map2` は、コールバックを呼ぶ前に長さを調べます。

### リストを共有しない

F# のリストは不変で、いくつものリストが末尾を共有します。Tsuzuri のリストは所有者が 1 つなので、共有できません。そのため、F# では末尾を共有して $O(1)$ や $O(n)$ で済む関数が、Tsuzuri では要素を複製します。

| 関数 | F# | Tsuzuri |
| --- | --- | --- |
| `append xs ys` | $O(n)$（`ys` は共有） | $O(n + m)$（両方を複製） |
| `skip n xs` | $O(n)$（`n` は飛ばす個数） | $O(\text{length})$（残りを複製） |
| `tail xs` | $O(1)$ | $O(1)$（`xs` を消費して、先頭の要素を取り除く） |
| `length xs` | $O(n)$ | $O(1)$（長さは保存されています） |
| `item i xs` | $O(i)$ | 先行する chunk の数。一度に作ったリストは $O(1)$ |

要素が `Copy` でないリスト（文字列のリストなど）は複製できないので、要素を複製する関数（`append`、`skip`、`filter`、`rev` など）には渡せません（`E1005`）。

### コールバックと `r` 形

要素を値で受けるコールバックは、要素が `Copy` であることを要求します。文字列やリストのリストは `Copy` ではないので、値を取る形は使えません（`E1005`）。そのための借用形が、関数名の語幹（最初の語）の直後に `r` を付けた関数です。コールバックは要素の共有参照 `ref 'a` を受け取り、要素そのものを返す関数は参照を返すので、どんな要素型にも使えます。`fold_back` の借用形が `foldr_back` になるように、`_back` などの後ろの語は動きません。

| 値で受ける形（F#） | 借用形 | 借用形のシグネチャ |
| --- | --- | --- |
| `map` | `mapr` | `(ref 'a -> 'b) -> ref [\|'a\|] -> [\|'b\|]` |
| `mapi` | `mapir` | `(i64 -> ref 'a -> 'b) -> ref [\|'a\|] -> [\|'b\|]` |
| `fold` | `foldr` | `('state -> ref 'a -> 'state) -> 'state -> ref [\|'a\|] -> 'state` |
| `fold_back` | `foldr_back` | `(ref 'a -> 'state -> 'state) -> 'state -> ref [\|'a\|] -> 'state` |
| `iter` | `iterr` | `(ref 'a -> unit) -> ref [\|'a\|] -> unit` |
| `iteri` | `iterir` | `(i64 -> ref 'a -> unit) -> ref [\|'a\|] -> unit` |
| `exists` | `existsr` | `(ref 'a -> bool) -> ref [\|'a\|] -> bool` |
| `forall` | `forallr` | `(ref 'a -> bool) -> ref [\|'a\|] -> bool` |
| `find` | `findr` | `(ref 'a -> bool) -> ref [\|'a\|] -> ref 'a` |
| `try_find` | `try_findr` | `(ref 'a -> bool) -> ref [\|'a\|] -> Maybe<ref 'a>` |
| `min`、`max` | `minr`、`maxr` | `Ord<'a> => ref [\|'a\|] -> ref 'a` |
| `contains` | `containsr` | `Eq<'a> => ref 'a -> ref [\|'a\|] -> bool` |
| `to_seq` | `to_seqr` | `ref [\|'a\|] -> Seq<ref 'a>` |

`mapr` と `foldr` はコンパイラ組み込みです（以前の `map_ref` と `fold_ref`）。`contains` は値を受け取ります。名前付きの文字列などを渡して消費したくないときは `containsr` に参照を渡します。

```tsuzuri run=lengths%3D%5B%7C5%2C%203%2C%206%7C%5D%20total%3D14%20has_long%3Dtrue%20has_fig%3Dtrue
let names = [|"apple", "fig", "banana"|]
let lengths = List.mapr (\s -> s.length) names
let total = List.foldr (\acc s -> acc + s.length) 0 names
let has_long = List.existsr (\s -> s.length > 5) names
let has_fig = List.containsr "fig" names
$"lengths={lengths} total={total} has_long={has_long} has_fig={has_fig}"
```

実行結果:

```text
lengths=[|5, 3, 6|] total=14 has_long=true has_fig=true
```

`ref` を取る関数には、リテラルや関数の戻り値のような一時的なリストをそのまま渡せます（`List.length [|1, 2, 3|]`、`xs |> List.rev`）。参照を返す `minr`、`maxr`、`findr` だけは、参照先が生きている必要があるので、リストに名前を付けてから渡します。名前がないと `E1013` です。

### F# にない関数

次の関数は Tsuzuri だけのものです。

| 関数 | 説明 |
| --- | --- |
| `cons` | リストの先頭に要素を足します。F# の式 `x :: xs` に当たります。`::` は Tsuzuri ではパターン専用です |
| `copy` | リストの全要素を複製した、1 つの chunk の新しいリストを返します |
| `mapr`、`mapir`、`foldr`、`foldr_back`、`iterr`、`iterir`、`existsr`、`forallr`、`findr`、`try_findr`、`minr`、`maxr`、`containsr`、`to_seqr` | 要素を借用する形（[コールバックと `r` 形](#コールバックと-r-形)） |

## API リファレンス

すべての関数は `std::List` モジュール（または言語組み込み）に属しています。`'a` を値で読む関数は `Copy<'a>` を要求します。表では、F# に同名の関数がある行の名前をそのまま載せています。

### 基本操作・情報取得

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `length` | `ref [\|'a\|] -> i64` | 要素数を返します。保存済みの長さなので $O(1)$ です |
| `is_empty` | `ref [\|'a\|] -> bool` | リストが空かどうかを $O(1)$ で返します |
| `cons` | `'a -> [\|'a\|] -> [\|'a\|]` | 先頭に要素を追加した新しいリストを返します（$O(1)$。F# にはありません） |
| `tail` | `[\|'a\|] -> [\|'a\|]` | 先頭要素を除いた残りのリストを返します（空リストはトラップ） |
| `head`、`last` | `Copy<'a> => ref [\|'a\|] -> 'a` | 先頭、末尾の要素。空リストはトラップします |
| `try_head`、`try_last` | `Copy<'a> => ref [\|'a\|] -> Maybe<'a>` | 空リストは `None` です |
| `item` | `Copy<'a> => i64 -> ref [\|'a\|] -> 'a` | 指定位置の要素。範囲外はトラップします。`xs[i]` と同じです |
| `try_item` | `Copy<'a> => i64 -> ref [\|'a\|] -> Maybe<'a>` | 範囲外は `None` です |
| `exactly_one` | `Copy<'a> => ref [\|'a\|] -> 'a` | 要素が 1 つだけのときのその要素。それ以外はトラップします |
| `try_exactly_one` | `Copy<'a> => ref [\|'a\|] -> Maybe<'a>` | 要素が 1 つだけでなければ `None` です |

### 生成・変換

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `empty` | `[\|'a\|]` | 空のリスト。`List.empty()` と書きます |
| `singleton` | `'a -> [\|'a\|]` | 要素が 1 つのリスト |
| `replicate` | `Copy<'a> => i64 -> 'a -> [\|'a\|]` | 同じ値を並べたリスト。負の個数はトラップします |
| `init` | `i64 -> (i64 -> 'a) -> [\|'a\|]` | 長さと初期化関数からリストを生成します。`new [\|T\|](n, f)` と同じです |
| `unfold` | `('state -> Maybe<('a * 'state)>) -> 'state -> [\|'a\|]` | `None` を返すまで要素を生成します |
| `copy` | `Copy<'a> => ref [\|'a\|] -> [\|'a\|]` | リストの全要素を複製した、1 つの chunk の新しい独立リストを返します。要素が所有するメモリを持たない型なら、chunk ごとのバイト複写（`llvm.memcpy`）です（F# にはありません） |
| `rev` | `Copy<'a> => ref [\|'a\|] -> [\|'a\|]` | 要素の並びを反転した新しいリストを返します（組み込み） |
| `permute` | `Copy<'a> => (i64 -> i64) -> ref [\|'a\|] -> [\|'a\|]` | 元の添字 `i` の要素を `f i` へ移します。順列でなければトラップします |
| `of_array` | `Copy<'a> => ref ['a] -> [\|'a\|]` | 配列から、要素数ちょうどの 1 つの chunk のリストを作ります。要素が所有するメモリを持たない型なら、1 回のバイト複写です |
| `to_array` | `Copy<'a> => ref [\|'a\|] -> ['a]` | リストから連続メモリの所有配列 `[T]` を構築します。要素が所有するメモリを持たない型なら、chunk ごとのバイト複写です（組み込み） |
| `of_seq` | `Seq<'a> -> [\|'a\|]` | `Seq` からリストを作ります。要素は `Copy` でなくてかまいません |
| `to_seq` | `Copy<'a> => ref [\|'a\|] -> Seq<'a>` | 要素を値で順次列挙する `Seq` を返します |
| `to_seqr` | `ref [\|'a\|] -> Seq<ref 'a>` | 各要素への共有参照を順次列挙する `Seq` を返します |

### 部分リスト・連結・組み合わせ

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `take`、`skip`、`truncate` | `Copy<'a> => i64 -> ref [\|'a\|] -> [\|'a\|]` | 先頭 n 個、先頭 n 個を除いたもの、最大 n 個。`take` と `skip` は長さを超えるとトラップします |
| `split_at` | `Copy<'a> => i64 -> ref [\|'a\|] -> ([\|'a\|] * [\|'a\|])` | 位置で 2 つに分けます |
| `take_while`、`skip_while` | `Copy<'a> => ('a -> bool) -> ref [\|'a\|] -> [\|'a\|]` | 述語が真の間だけ取る、真の間は飛ばす |
| `chunk_by_size` | `Copy<'a> => i64 -> ref [\|'a\|] -> [\|[\|'a\|]\|]` | 最大 n 個ずつの塊に分けます。n が正でなければトラップします |
| `split_into` | `Copy<'a> => i64 -> ref [\|'a\|] -> [\|[\|'a\|]\|]` | 高々 n 個の塊に、大きさの差が 1 以下になるよう分けます |
| `windowed` | `Copy<'a> => i64 -> ref [\|'a\|] -> [\|[\|'a\|]\|]` | 長さ n の窓を 1 つずつずらして返します |
| `pairwise` | `Copy<'a> => ref [\|'a\|] -> [\|('a * 'a)\|]` | 隣り合う要素の組 |
| `append` | `Copy<'a> => ref [\|'a\|] -> ref [\|'a\|] -> [\|'a\|]` | 2 つのリストを連結した新しいリストを返します |
| `concat` | `Copy<'a> => ref [\|[\|'a\|]\|] -> [\|'a\|]` | リストのリストを 1 つに平坦化して連結します |
| `zip`、`zip3` | `(Copy<'a>, Copy<'b>) => ref [\|'a\|] -> ref [\|'b\|] -> [\|('a * 'b)\|]` | 要素を複製して組にします。長さが違うとトラップします |
| `unzip`、`unzip3` | `(Copy<'a>, Copy<'b>) => ref [\|('a * 'b)\|] -> ([\|'a\|] * [\|'b\|])` | 組のリストを、成分ごとのリストに分けます |
| `all_pairs` | `(Copy<'a>, Copy<'b>) => ref [\|'a\|] -> ref [\|'b\|] -> [\|('a * 'b)\|]` | すべての組み合わせ。最初のリストが外側のループです |
| `indexed` | `Copy<'a> => ref [\|'a\|] -> [\|(i64 * 'a)\|]` | 位置と要素の組 |
| `transpose` | `Copy<'a> => ref [\|[\|'a\|]\|] -> [\|[\|'a\|]\|]` | リストのリストを転置します。長さが揃っていないとトラップします |

### 走査・変換

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `iter`、`iteri` | `Copy<'a> => ('a -> unit) -> ref [\|'a\|] -> unit` | 各要素に関数を適用します。`iteri` は位置も渡します |
| `iter2`、`iteri2` | `(Copy<'a>, Copy<'b>) => ('a -> 'b -> unit) -> ref [\|'a\|] -> ref [\|'b\|] -> unit` | 2 つのリストの対応する要素に適用します |
| `map` | `Copy<'a> => ('a -> 'b) -> ref [\|'a\|] -> [\|'b\|]` | 各要素に関数を適用した新しいリストを返します（組み込み） |
| `mapi` | `Copy<'a> => (i64 -> 'a -> 'b) -> ref [\|'a\|] -> [\|'b\|]` | 位置も渡す `map` です |
| `map2`、`mapi2`、`map3` | `(Copy<'a>, Copy<'b>) => ('a -> 'b -> 'c) -> ref [\|'a\|] -> ref [\|'b\|] -> [\|'c\|]` | 2 つ、3 つのリストを同時に変換します |
| `map_fold`、`map_fold_back` | `Copy<'a> => ('state -> 'a -> ('b * 'state)) -> 'state -> ref [\|'a\|] -> ([\|'b\|] * 'state)` | 変換と畳み込みを同時に行います。`map_fold_back` は末尾から（結果は元の順）で、`('a -> 'state -> ('b * 'state)) -> 'state -> ref [\|'a\|] -> ([\|'b\|] * 'state)` |
| `collect` | `(Copy<'a>, Copy<'b>) => ('a -> [\|'b\|]) -> ref [\|'a\|] -> [\|'b\|]` | 各要素が返すリストを連結します |
| `choose` | `Copy<'a> => ('a -> Maybe<'b>) -> ref [\|'a\|] -> [\|'b\|]` | `Some` の中身だけを集めます |
| `filter`、`where` | `Copy<'a> => ('a -> bool) -> ref [\|'a\|] -> [\|'a\|]` | 述語を満たす要素を、順序を保って集めます（`where` は同じ関数です） |
| `partition` | `Copy<'a> => ('a -> bool) -> ref [\|'a\|] -> ([\|'a\|] * [\|'a\|])` | 述語が真の要素と偽の要素に分けます |
| `partition_with` | `Copy<'a> => ('a -> Result<'b, 'c>) -> ref [\|'a\|] -> ([\|'b\|] * [\|'c\|])` | `Ok` の中身と `Error` の中身に分けます |
| `mapr`、`mapir` | `(ref 'a -> 'b) -> ref [\|'a\|] -> [\|'b\|]` | 要素の参照を受け取る `map`、`mapi` です（非 Copy 要素に対応。`mapr` は組み込み） |
| `iterr`、`iterir` | `(ref 'a -> unit) -> ref [\|'a\|] -> unit` | 要素の参照を受け取る `iter`、`iteri` です |

### 畳み込み

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `fold` | `Copy<'a> => ('state -> 'a -> 'state) -> 'state -> ref [\|'a\|] -> 'state` | 先頭から左畳み込みを行います |
| `fold_back` | `Copy<'a> => ('a -> 'state -> 'state) -> 'state -> ref [\|'a\|] -> 'state` | 末尾から右畳み込みを行います |
| `fold2`、`fold_back2` | `(Copy<'a>, Copy<'b>) => ('state -> 'a -> 'b -> 'state) -> 'state -> ref [\|'a\|] -> ref [\|'b\|] -> 'state` | 2 つのリストを同時に畳み込みます。`fold_back2` は `('a -> 'b -> 'state -> 'state) -> 'state -> ref [\|'a\|] -> ref [\|'b\|] -> 'state` |
| `foldr`、`foldr_back` | `('state -> ref 'a -> 'state) -> 'state -> ref [\|'a\|] -> 'state` | 要素の参照を受け取る `fold`、`fold_back` です（`foldr_back` は `(ref 'a -> 'state -> 'state) -> 'state -> ref [\|'a\|] -> 'state`。`foldr` は組み込み） |
| `reduce`、`reduce_back` | `Copy<'a> => ('a -> 'a -> 'a) -> ref [\|'a\|] -> 'a` | 先頭の要素を初期値にして畳み込みます。空リストはトラップします |
| `scan`、`scan_back` | `(Copy<'a>, Copy<'state>) => ('state -> 'a -> 'state) -> 'state -> ref [\|'a\|] -> [\|'state\|]` | 途中の状態も含めて返します。長さは `n + 1` です。`scan_back` は `('a -> 'state -> 'state) -> 'state -> ref [\|'a\|] -> [\|'state\|]` で、末尾から畳み込みます |

### 探索・判定

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `exists`、`forall` | `Copy<'a> => ('a -> bool) -> ref [\|'a\|] -> bool` | いずれかの要素が、すべての要素が述語を満たすか判定します（短絡評価） |
| `exists2`、`forall2` | `(Copy<'a>, Copy<'b>) => ('a -> 'b -> bool) -> ref [\|'a\|] -> ref [\|'b\|] -> bool` | 対応する要素の組で判定します。答えが決まれば長さの違いは問われません |
| `existsr`、`forallr` | `(ref 'a -> bool) -> ref [\|'a\|] -> bool` | 要素の参照を受け取る `exists`、`forall` です |
| `contains` | `Eq<'a> => 'a -> ref [\|'a\|] -> bool` | 指定された値が含まれているかを判定します |
| `containsr` | `Eq<'a> => ref 'a -> ref [\|'a\|] -> bool` | 値を借用して判定します |
| `find`、`find_back` | `Copy<'a> => ('a -> bool) -> ref [\|'a\|] -> 'a` | 述語を満たす最初、最後の要素。なければトラップします |
| `try_find`、`try_find_back` | `Copy<'a> => ('a -> bool) -> ref [\|'a\|] -> Maybe<'a>` | なければ `None` です |
| `find_index`、`find_index_back` | `Copy<'a> => ('a -> bool) -> ref [\|'a\|] -> i64` | 述語を満たす最初、最後の要素の位置。なければトラップします |
| `try_find_index`、`try_find_index_back` | `Copy<'a> => ('a -> bool) -> ref [\|'a\|] -> Maybe<i64>` | なければ `None` です |
| `findr`、`try_findr` | `(ref 'a -> bool) -> ref [\|'a\|] -> ref 'a` | 要素の参照を返す `find`、`try_find` です（`try_findr` は `Maybe<ref 'a>`） |
| `pick`、`try_pick` | `Copy<'a> => ('a -> Maybe<'b>) -> ref [\|'a\|] -> 'b` | 最初に `Some` を返した結果。`try_pick` は `Maybe<'b>` を返します |
| `compare_with` | `Copy<'a> => ('a -> 'a -> i64) -> ref [\|'a\|] -> ref [\|'a\|] -> i64` | 要素ごとに比べ、違いがなければ短い方を小さいとします |

### 集計・グループ化

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `sum` | `Numeric<'a> => ref [\|'a\|] -> 'a` | 全要素の総和を計算します。空リストは 0 |
| `sum_by` | `(Copy<'a>, Numeric<'b>) => ('a -> 'b) -> ref [\|'a\|] -> 'b` | 関数の結果の総和 |
| `average`、`average_by` | `Float<'a> => ref [\|'a\|] -> 'a` | 平均。空リストはトラップします。`average_by` は `(Copy<'a>, Float<'b>) => ('a -> 'b) -> ref [\|'a\|] -> 'b` |
| `min`、`max` | `(Ord<'a>, Copy<'a>) => ref [\|'a\|] -> 'a` | 最小、最大の要素。同値は最初の要素です。空リストはトラップします |
| `minr`、`maxr` | `Ord<'a> => ref [\|'a\|] -> ref 'a` | 要素の参照を返す `min`、`max` です |
| `min_by`、`max_by` | `(Copy<'a>, Ord<'key>) => ('a -> 'key) -> ref [\|'a\|] -> 'a` | キーが最小、最大の要素。同じキーは最初の要素です。キーは要素ごとに 1 度だけ計算します。空リストはトラップします |
| `distinct` | `(Hash<'a>, Eq<'a>, Copy<'a>) => ref [\|'a\|] -> [\|'a\|]` | 重複を除きます。最初の要素が残ります |
| `distinct_by` | `(Copy<'a>, Hash<'key>, Eq<'key>) => ('a -> 'key) -> ref [\|'a\|] -> [\|'a\|]` | キーが重複する要素を除きます |
| `except` | `(Hash<'a>, Eq<'a>, Copy<'a>) => ref [\|'a\|] -> ref [\|'a\|] -> [\|'a\|]` | `except excluded xs`：`excluded` にない要素を、重複を除いて返します |
| `count_by` | `(Copy<'a>, Copy<'key>, Hash<'key>, Eq<'key>) => ('a -> 'key) -> ref [\|'a\|] -> [\|('key * i64)\|]` | キーごとの個数。キーは最初に現れた順です |
| `group_by` | `(Copy<'a>, Copy<'key>, Hash<'key>, Eq<'key>) => ('a -> 'key) -> ref [\|'a\|] -> [\|('key * [\|'a\|])\|]` | キーごとの要素のリスト。キーは最初に現れた順です |

### ソート

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `sort`、`sort_descending` | `(Ord<'a>, Copy<'a>) => ref [\|'a\|] -> [\|'a\|]` | `Ord` に基づく安定ソートを行い新しいリストを返します |
| `sort_by`、`sort_by_descending` | `(Copy<'a>, Ord<'key>) => ('a -> 'key) -> ref [\|'a\|] -> [\|'a\|]` | キーに基づく安定ソート |
| `sort_with` | `Copy<'a> => ('a -> 'a -> i64) -> ref [\|'a\|] -> [\|'a\|]` | 比較関数に基づく安定ソート。負・ゼロ・正の `i64` を返します |

### 更新

リストは不変なので、更新は新しいリストを返します。

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `update_at` | `Copy<'a> => i64 -> 'a -> ref [\|'a\|] -> [\|'a\|]` | 指定位置を置換した新しいリスト。範囲外はトラップします |
| `insert_at` | `Copy<'a> => i64 -> 'a -> ref [\|'a\|] -> [\|'a\|]` | 位置の前へ挿入した新しいリスト。位置が `0 .. length` を外れるとトラップします |
| `insert_many_at` | `Copy<'a> => i64 -> ref [\|'a\|] -> ref [\|'a\|] -> [\|'a\|]` | 位置の前へ複数の要素を挿入した新しいリスト |
| `remove_at` | `Copy<'a> => i64 -> ref [\|'a\|] -> [\|'a\|]` | 指定位置を除いた新しいリスト |
| `remove_many_at` | `Copy<'a> => i64 -> i64 -> ref [\|'a\|] -> [\|'a\|]` | 指定位置から個数ぶんを除いた新しいリスト |

### 乱数

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `random_choice_by` | `Copy<'a> => (unit -> f64) -> ref [\|'a\|] -> 'a` | 無作為な 1 要素。空リストはトラップします |
| `random_choices_by` | `Copy<'a> => (unit -> f64) -> i64 -> ref [\|'a\|] -> [\|'a\|]` | 重複を許して n 個選びます |
| `random_sample_by` | `Copy<'a> => (unit -> f64) -> i64 -> ref [\|'a\|] -> [\|'a\|]` | 重複なしに n 個選びます。n が長さを超えるとトラップします |
| `random_shuffle_by` | `Copy<'a> => (unit -> f64) -> ref [\|'a\|] -> [\|'a\|]` | シャッフルした新しいリスト |
| `random_choice_with`、`random_choices_with`、`random_sample_with`、`random_shuffle_with` | `Copy<'a> => Random.Pcg -> ref [\|'a\|] -> ([\|'a\|] * Random.Pcg)` など | `_by` と同じ結果に、進んだ生成器を添えて返します |
| `random_choice`、`random_choices`、`random_sample`、`random_shuffle` | `Copy<'a> => [\|'a\|] -> IO<Result<[\|'a\|], Os.Error>>` など | OS の乱数で種を作る `IO` アクションです |

## 計算量

| 操作 | 計算量 | 備考 |
| --- | --- | --- |
| 先頭への追加 `List.cons` | 償却 $O(1)$ | 先頭の chunk の空き slot に書く。満杯なら容量 1.5 倍（上限 32 KiB）の chunk を 1 つ確保して足す |
| 先頭の取得 `match xs with head :: _` / `head` | $O(1)$ | 先頭の chunk の slot の読み出し |
| 先頭の除去 `List.tail` | $O(1)$ | 先頭要素を drop し、残りを返す（chunk は最後の要素と一緒に解放）。空ならトラップ |
| 空判定 `is_empty` | $O(1)$ | 長さが 0 かどうか |
| 添字アクセス `xs[i]` / `item` / `last` | 一度に作ったリストは $O(1)$、`List.cons` で伸ばしたリストは先行する chunk の数 | その要素より前の chunk の `next` をたどる |
| 長さの取得 `length` / `.length` | $O(1)$ | 保存済みの長さを読む |
| 要素変換 `map` / `mapr` | $O(n)$ | 全要素を走査して 1 つの chunk の新しいリストを構築 |
| 要素変換 `mapi` / `mapir` / `map2` / `mapi2` / `map3` / `choose` / `map_fold` / `unfold` | $O(n)$ | 結果を `Vec` に集めてから `List.cons` で並べるので、chunk の連なりのリスト |
| 畳み込み `fold` / `foldr` / `sum` | $O(n)$ | 全要素を 1 回走査 |
| 反転 `rev` | $O(n)$ | 全要素を逆順に並べた 1 つの chunk のリストを構築 |
| 先頭 n 個 `take` / `truncate` / `take_while` | $O(n)$ | n 個目で走査を止める |
| 取り除き `skip` / `skip_while` / `split_at` | $O(\text{length})$ | 残りの要素を複製する |
| 連結 `append` / `concat` | $O(n + m)$ | 両方の要素を複製する |
| 探索 `exists` / `forall` / `find` / `try_find` / `contains` | $O(n)$ | 答えが決まった時点で短絡終了。要素は添字を使わず走査する |
| 末尾からの探索 `find_back` / `fold_back` / `reduce_back` | $O(n)$ | いったん配列へ複写する |
| ソート `sort` / `sort_by` / `sort_with` | $O(n \log n)$ | 安定マージソート。`sort_by` はキーを $n$ 回だけ計算 |
| 重複除去・グループ化 `distinct` / `count_by` / `group_by` | 平均 $O(n)$ | ハッシュ表を使う |
| 組み合わせ `all_pairs` / `transpose` | $O(n \cdot m)$ | 結果の要素数に比例 |
| 窓 `windowed` | $O(n \cdot w)$ | 窓の数 × 窓の幅。各窓は新しいリスト |
| 挿入・削除・更新 `insert_at` / `remove_at` / `update_at` | $O(n)$ | 新しいリストを作る |
| 配列化 `to_array`、配列から `of_array` | $O(n)$ | 全要素を連続メモリへコピー |

要素を複製する関数は、いったん配列へ複写して処理する関数と、リストを 1 回走査して集める関数のどちらかです。どちらもリストを添字で回しません。`List.cons` で伸ばしたリストでも、長さに比例する時間で終わります。結果の要素が `Copy` でなくてよい関数（`mapi`、`map2`、`choose`、`partition_with`、`unfold`、`of_seq` など）は、結果が配列を通れないので、`Vec` に集めてから `List.cons` で並べたリスト、つまり chunk の連なりを返します。`List.cons` で伸ばしたリストと同じで、走査の速さは変わらず、添字だけが先行する chunk の数に比例します。コールバックは、閉包に包まず、ループから直接呼びます。呼び出しごとに環境が複製されて、表のような大きな値を所有するコールバックが遅くなるのを避けるためです。

### 小さな定数サイズのリストの性能（既知の制限）

> [!WARNING]
> **要素数が定数の小さなリストを、1 つの関数の中で作って使って捨てる処理は、以前より遅くなりました**
> 以前の連結ノードの表現では、要素数がコンパイル時に決まる小さなリスト（3 要素など）を同じ関数の中で作って走査して捨てると、LLVM が確保を全部消して 1 回あたり約 1 ns で済みました。いまの chunk の表現でも、`new` のリテラルを走査するだけなら同じく確保が消えて約 1 ns です。しかし `List.map`・`List.rev`・`List.copy`・`List.cons` を重ねたリスト、要素が `string` のリストでは、1〜3 個の chunk の確保と解放が残り、3 要素で 1 回あたり約 25〜80 ns かかります（Apple M1 Max、`-O3`。`List.map`・`List.rev` が約 25 ns、`List.copy` が約 45 ns、`List.cons` を 3 回重ねると約 55〜65 ns、`string` 2 個の `new` が約 75〜80 ns）。chunk の中身の読み書きや、header の次の chunk・容量を、LLVM が定数に畳みきれないためです。
>
> 要素数が実行時に決まるリストでは逆に速くなります（100 万要素の構築が約 36 倍、`for` での走査が約 14 倍、`control/list_sum` が約 40 倍）。この回帰が効くのは、小さな定数サイズのリストを同じ関数の中で何度も作って捨てる内側のループです。条件と生の値は [PM04 の測定](../../../docs/benchmarks.md#pm04-連結リストの連続表現)にあります。

## 所有権と借用

### リストの受け渡しとメモリ解放

`List.cons x xs` は、所有するリスト `xs` を消費し、先頭に `x` を足したリストを返します。`xs` の chunk はコピーされず、所有権ごと新しいリストへ移ります。`List.tail xs` も同じく `xs` を消費し、先頭要素を drop して残りを返します。先頭の chunk は、その最後の要素を取り除いたときに解放されます。

リスト `[|T|]` の所有者は常に 1 つです。そのため、F# や Haskell のリストのように、1 つの末尾を複数のリストで共有することはありません。末尾を共有する永続リストが要るときは、`union List = Nil | Cons of (i64 * Rc<List>)` のように [Rc](rc.md) を持つ共用体を定義します。

- 要素が非 Copy のリストは、`List.cons` に渡したあとの元の束縛を使うと `E1012` です。
- 要素が Copy のリストは、渡したあとも元の束縛を使えますが、そのときは全要素が複製されます。`--warn implicit-copy` を付けると `W1006` で知らせてくれます。
- `match` の `head :: tail` で `tail` を所有値として使うには、要素型が Copy である必要があります（非 Copy なら `E1005`）。このときも残りの要素が複製されます。長いリストを先頭から順に処理するなら、`tail` を取り出して再帰するより `for ... in` のほうが余分な複製を避けられます。

`List.cons` と `List.tail` 以外の関数は、リストを借用（`ref`）で受け取ります。渡したリストは消費されず、要素は必要なぶんだけ複製されて、新しいリストに入ります。各 chunk は、それを所有するリストが不要になった時点で一度だけ解放されます。

### 非 Copy な要素の処理

要素が `string` や、非 `Copy` のフィールドを持つレコードのとき、値として取り出すと所有権が動きます。リストを借用したまま処理するなら、`r` を付けた関数（`mapr`、`foldr`、`existsr` など）を使います。全フィールドが `Copy` のレコードは `Copy` です。

```tsuzuri run=lengths%3D%5B%7C5%2C%206%7C%5D%20total%3D11
let fruits = [|"apple", "banana"|]
let lengths = List.mapr (\s -> s.length) fruits
let total = List.foldr (\acc s -> acc + s.length) 0 fruits
$"lengths={lengths} total={total}"
```

実行結果:

```text
lengths=[|5, 6|] total=11
```

`find` に当たる `findr` や、`min`、`max` に当たる `minr`、`maxr` は、リスト内の要素への参照を返します。参照先のリストが生きている間だけ使えます。

```tsuzuri run=greatest%3Dpear%20short%3Dfig
let names = [|"pear", "fig", "banana", "apple"|]
let greatest = List.maxr names
let short = List.findr (\s -> s.length < 4) names
$"greatest={greatest} short={short}"
```

実行結果:

```text
greatest=pear short=fig
```

## 反復の仕方

### 直接の `for ... in` ループ

リストは `for ... in` で直接走査できます。ループ変数は `ref` ではなく、読み取り専用の要素そのものです。非 `Copy` をムーブすると `E1012` です。`break` と `continue` も使えます。

```tsuzuri run=total%3D60
let values = [|10, 20, 30|]
let mut total = 0
for x in values do
    total = total + x
$"total={total}"
```

実行結果:

```text
total=60
```

この直接走査は一時的なヒープメモリを一切割り当てず、連続して並んだ要素を chunk ごとに直接読むため非常に高効率です。chunk の中は配列の `for` と同じ形の loop なので、総和のように本体が単純なら LLVM がベクトル化します。`List.foldr` も同じ loop で走査します。

F# と同じ `List.iter f xs` は、各要素に `f` を適用して `unit` を返します。値を集める反復には `for` を使い、副作用だけの呼び出し（`Debug.print` など）には `List.iter` を使います。要素が `Copy` でなければ `List.iterr` です。

### `List.to_seq` と `List.to_seqr` による遅延走査

[Seq](./seq.md) の遅延イテレータとして扱いたい場合は `List.to_seq xs` か `List.to_seqr xs` を呼び出します。`to_seq` は要素の値の `Seq<'a>`（要素は `Copy`）、`to_seqr` は各要素への共有借用参照を順次生成する `Seq<ref 'a>` を返します。

```tsuzuri run=total%3D11
let words = [|"Tsuzuri", "Lang"|]
let mut total = 0
for word in List.to_seqr words do
    total = total + word.length
$"total={total}"
```

実行結果:

```text
total=11
```

> [!NOTE]
> `List.to_seq` と `List.to_seqr` は、リストを 1 回走査して各要素への参照ポインタを格納した一時バッファ（Vec）を内部で準備したうえで `Seq` を返します。以前の `List.iter (ref xs)` は、この `List.to_seqr` になりました。
> 単純にリストの全要素を 1 度走査するだけであれば、追加メモリを消費しない直接の `for ... in` ループを使用することを推奨します。

## まとめ

- 型は `[|T|]` です。`List<T>` とは書けません。`::` はパターン専用で、先頭追加は `List.cons` です。
- F# の `List` モジュールの関数がすべてあります。名前は snake_case、リストは常に最後の引数（F# で最後にない 4 つの関数も移しました）、F# の例外はトラップ、`try_` は `Maybe` です。
- 要素を値で受ける関数は `Copy` の要素向けで、非 `Copy` の要素には `mapr`、`foldr`、`existsr` のような `r` 形を使います。
- 2 つのリストを取る関数は、F# と同じく共有する組を先に見ます。長さの違いは、答えが決まらなかったときにトラップします。
- 長さは $O(1)$、添字は一度に作ったリストなら $O(1)$、`List.cons` で伸ばしたリストなら先行する chunk の数に比例します。`List` の関数は添字で回さないので、どちらのリストでも $O(n)$ 前後で終わります。走査は直接の `for ... in` が一時メモリを使いません。
- 要素数が定数の小さなリストを同じ関数の中で作って捨てる処理は、以前の連結ノードの表現より遅くなることがあります（約 1 ns → 25〜80 ns）。要素数が実行時に決まるリストは速くなりました。

## 関連項目

- [Array](./array.md) — 連続メモリ配列。F# の `Array` モジュールと同じ関数があります
- [Vec](./vec.md) — 伸縮可能な所有バッファ
- [Seq](./seq.md) — 遅延シーケンス
- [Random](./random.md) — `Random.Pcg` と OS の乱数
- [パターンマッチ](../pattern-matching/pattern-matching.md)
- [言語リファレンスの目次](../index.md)
