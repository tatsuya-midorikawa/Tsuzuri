# Vec

`Vec<T>` は、動的に要素を追加・削除できる所有権付きの可変長バッファです。内部に連続したメモリ領域（ヒープ）を持ち、末尾への要素追加を償却 $O(1)$ で行えます。

`Vec` モジュールは、.NET の [`List<T>`](https://learn.microsoft.com/ja-jp/dotnet/api/system.collections.generic.list-1?view=net-10.0) と同じメンバーを、同じ名前（snake_case）で提供します。引数は .NET と同じ順に並べ、`Vec` は最後に受け取ります。F# の `List` や `Array` と同じ形なので、`values |> Vec.add 1 |> Vec.sort` のように、パイプライン `|>` や関数合成でつなげられます。`Vec.add`・`Vec.insert`・`Vec.remove_at`・`Vec.sort`・`Vec.binary_search` のように、.NET の書き方がほぼそのまま使えます。所有権のために .NET と違う点（更新が `Vec` を消費して返すこと、失敗の扱い、`Copy`、`r` を付けた関数）は [.NET の List<T> との対応](#net-の-listt-との対応) にまとめました。

処理の途中で要素数が動的に変化する場面や、蓄積したデータを最後に [Array](./array.md) へ移送して利用するワークフローに最適です。

## この記事のポイント

- 型は `Vec<T>`。要素型にかかわらず常に非 Copy 型として扱われます。
- .NET の `List<T>` のメンバー（コンストラクター、`Capacity`、`Count`、インデクサーを含む）にはすべて対応する関数があります。名前は snake_case、引数は .NET の順で、**`Vec` は最後**です。`AsReadOnly` だけは、`ref values` で借用すれば読み取り専用になるので、関数がありません。
- 空のバッファ `Vec.empty()`、事前確保 `Vec.with_capacity count`、配列・列・リストからの `Vec.of_array`・`Vec.of_seq`・`Vec.of_list` で生成します。
- 更新操作（`add`、`insert`、`remove_at`、`sort` など）は所有する `Vec` を消費して、更新した `Vec` を返します。`let mut` 変数に再代入して使うか（`values = Vec.add 1 values`）、パイプラインでつなぎます（`values |> Vec.add 1`）。`remove` と `remove_all` は、結果（見つかったか、消した個数）との組を返します。
- .NET が例外を投げる場面（範囲外、負の個数、容量が長さより小さいなど）はトラップです。.NET が `-1` や、二分探索の補数（`~index`）を返す場面は、同じ値を返します。
- コールバックが要素を値で受ける関数（`find_index`、`exists`、`sort_with` など）は、要素が `Copy` であることを要求します。`find_indexr`、`existsr`、`sort_withr` のように `r` を付けた関数は要素を借用するので、文字列のような `Copy` でない要素にも使えます。
- 内部容量（capacity）は .NET と同じ規則（空なら 4、以降は 2 倍、足りなければ必要な数）で増えます。`capacity`、`set_capacity`、`ensure_capacity`、`trim_excess` の値も .NET と一致します。あらかじめ要素数が予測できる場合は `with_capacity` や `reserve` で再確保を防げます。
- `sort` と `sort_range` は安定で、NaN を他のどの数より前に置きます（.NET と同じ）。比較関数を取る `sort_with`・`sort_range_with`・`sort_withr`・`sort_range_withr` も安定です。`sort_with` と `sort_range_with` は `Array.sort_in_place_with` で並べるので、要素が `f32`・`f64` のときは NaN を通常の数値の後ろに置きます（[Array](array.md)）。`sort_withr` と `sort_range_withr` は NaN を特別に扱わず、渡した関数のとおりに並べます。
- `pop` は `(残りのVec, Maybe<末尾要素>)` のタプルを返し、`Copy` でない要素も所有値として安全に取り出せます。
- `Vec.to_array` と `Vec.of_array` は、所有しているバッファを移します。要素の複製はしません（.NET の `ToArray` は複製するので、そこが違います）。`Copy` 配列の引数渡しと、スタック配列のヒープ昇格は通常どおりです。
- 要素そのものへの排他借用参照（`ref mut`）はバッファ破損を防ぐため取得できません。

## 基本の書き方と生成

### 生成と要素の蓄積

`Vec.empty()` で空のバッファを作成し、`Vec.add` で末尾に要素を追加します（.NET の `new List<T>()` と `Add` です）。
`Vec.add` などの消費する更新は、更新後の Vec を返すので可変変数に再束縛します。
2027 の組み込み `Vec.push` は対象を排他借用して更新し、unit を返します。
2026 の `Vec.push` と全版の `Vec.push_owned` は従来の消費する契約です。

```tsuzuri run=len%3D3%20cap%3D4
let mut values: Vec<i64> = Vec.empty()
values = Vec.add 10 values
values = Vec.add 20 values
values = Vec.add 30 values
let len = Vec.length (ref values)
let cap = Vec.capacity (ref values)
$"len={len} cap={cap}"
```

実行結果:

```text
len=3 cap=4
```

### パイプラインと関数合成

すべての関数が `Vec` を最後に受け取るので、F# の `List` や `Array` と同じく、`|>` でつなげられます。引数を一部だけ渡した関数は、`>>` で合成できます。

```tsuzuri run=sorted%3D%5B1%2C%2010%2C%202%2C%203%5D%20top%3D%5B9%2C%205%5D
let sorted =
    Vec.empty()
    |> Vec.add 3
    |> Vec.add 1
    |> Vec.add 2
    |> Vec.sort
    |> Vec.insert 1 10
let top_two = Vec.sort >> Vec.reverse >> Vec.truncate 2
let top = Vec.of_array [5, 1, 9, 3] |> top_two
let sorted_array = Vec.to_array sorted
let top_array = Vec.to_array top
$"sorted={sorted_array} top={top_array}"
```

実行結果:

```text
sorted=[1, 10, 2, 3] top=[9, 5]
```

要素の値や位置を先に渡し、`Vec` を最後に渡す順なので、`Vec.add 1` は「`Vec` を受け取って 1 を足した `Vec` を返す関数」です。`Vec.remove_at 0` や `Vec.sort_with (\a b -> a - b)` のように、一部だけ適用した形をそのまま値として持ち回れます。`Vec.index_of` のように `Vec` を借用する関数にも、`values |> Vec.index_of 2` と渡せます。このときは `Vec` が借用されるだけで、`values` は使い続けられます。引数の順序の全体は [.NET の List<T> との対応](#net-の-listt-との対応) の[引数の順序](#引数の順序)にあります。

### 事前確保 `Vec.with_capacity`

追加する個数が分かっているなら、`Vec.with_capacity count` で先に容量を確保すると、途中の再確保を避けられます。

```tsuzuri run=len%3D0%20cap%3D100
let buffer: Vec<string> = Vec.with_capacity 100
let len = Vec.length (ref buffer)
let cap = Vec.capacity (ref buffer)
$"len={len} cap={cap}"
```

実行結果:

```text
len=0 cap=100
```

`count` に負数を指定した場合や、合計バイト数がオーバーフローする場合は安全にトラップします。

### 配列・列・リストからの生成

`new List<T>(collection)` に当たる生成関数は、元の種類ごとに分かれています。

| 関数 | 元 | できた `Vec` の容量 | 説明 |
| --- | --- | --- | --- |
| `Vec.of_array array` | 所有する配列 `['a]` | 要素数 | 配列のバッファをそのまま `Vec` にします。複製しません |
| `Vec.of_list (ref list)` | リスト `[\|'a\|]` | 要素数 | `Copy` の要素を複製して作ります |
| `Vec.of_seq seq` | `Seq<'a>` | 1 つずつ足した結果（4 から倍々） | 要素をどんな型でも受け取ります |
| `Vec.clone (ref values)` | `Vec<'a>` | 要素数 | `Copy` の要素を持つ独立した `Vec` を複製します。元の容量は引き継ぎません |

.NET も、`ICollection<T>`（配列や `List<T>`）からは要素数ちょうど、それ以外の `IEnumerable<T>` からは 1 つずつ足すので、容量は同じ値になります。

```tsuzuri run=caps%3D3%202%208%20squares%3D%5B1%2C%204%2C%209%2C%2016%2C%2025%5D
let from_array = Vec.of_array [1, 2, 3]
let from_list = Vec.of_list [|4, 5|]
let numbers = [1, 2, 3, 4, 5]
let from_seq = Vec.of_seq (Seq.map (\n -> n * n) (Array.to_seq (ref numbers)))
let cap_array = Vec.capacity (ref from_array)
let cap_list = Vec.capacity (ref from_list)
let cap_seq = Vec.capacity (ref from_seq)
let squares = Vec.to_array from_seq
$"caps={cap_array} {cap_list} {cap_seq} squares={squares}"
```

実行結果:

```text
caps=3 2 8 squares=[1, 4, 9, 16, 25]
```

## 要素の追加・削除・更新

### 追加 `add`・`add_range`・`add_range_seq`・`append`

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `add` | `'a -> Vec<'a> -> Vec<'a>` | 末尾に 1 つ足します（`Add`） |
| `add_range` | `Copy<'a> => ref ['a] -> Vec<'a> -> Vec<'a>` | 配列の要素を複製して末尾に足します（`AddRange`） |
| `add_range_seq` | `Seq<'a> -> Vec<'a> -> Vec<'a>` | `Seq` の要素を順に足します。要素の型は何でも構いません |
| `append` | `Vec<'a> -> Vec<'a> -> Vec<'a>` | 最初の `Vec` の要素を、最後の `Vec` の末尾へ移します。最初の `Vec` は消費され、要素は複製されません |

`add_range` は借用した配列の要素を複製するので、`Copy` でない要素（文字列など）は足せません。そのときは、要素を所有する `add_range_seq` か `append` を使います。`append` は、足される `Vec` が最後の引数なので、`values |> Vec.append more` は `values` の後ろに `more` を付けます。

### 挿入 `insert`・`insert_range`・`insert_range_seq`

`Vec.insert index item values` は `index` に要素を挿入し、そこから後ろの要素を 1 つ後ろへずらします。`index` は 0 から長さまでで、長さなら末尾への追加と同じです。範囲外はトラップです。`insert_range`（配列の要素を複製）と `insert_range_seq`（`Seq` の要素）は、複数の要素を並びの順のまま挿入します。後ろの要素のずらしは、連続した領域をまとめて動かす 1 回の移動です。

### 削除 `remove_at`・`remove_range`・`remove`・`remove_all`

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `remove_at` | `i64 -> Vec<'a> -> Vec<'a>` | 指定位置の要素を消し、後ろの要素を前へ詰めます（`RemoveAt`） |
| `remove_range` | `i64 -> i64 -> Vec<'a> -> Vec<'a>` | 位置と個数で指定した範囲を消します（`RemoveRange`） |
| `remove` | `Eq<'a> => 'a -> Vec<'a> -> (Vec<'a> * bool)` | 最初に等しい要素を消し、消したかどうかを組で返します（`Remove`） |
| `remove_all` | `Copy<'a> => ('a -> bool) -> Vec<'a> -> (Vec<'a> * i64)` | 述語が真の要素をすべて消し、消した個数を組で返します（`RemoveAll`） |

.NET の `Remove` は `bool`、`RemoveAll` は `int` を返しますが、Tsuzuri は更新した `Vec` も返す必要があるので、結果との組にします。`remove_all` は述語を各要素に 1 回、先頭から順に呼びます。残る要素は元の順序を保ち、消える要素は述語を呼び終えたあとに、順序を決めずに解放されます。

```tsuzuri run=values%3D%5B15%2C%2040%2C%2050%5D
let mut values = Vec.of_array [10, 20, 30]
values = Vec.insert 1 15 values
values = Vec.add_range (ref [40, 50]) values
values = Vec.remove_at 0 values
values = Vec.remove_range 1 2 values
let array = Vec.to_array values
$"values={array}"
```

実行結果:

```text
values=[15, 40, 50]
```

```tsuzuri run=found%3Dtrue%20removed%3D2%20left%3D%5B1%5D
let values = Vec.of_array [5, 3, 5, 1]
match Vec.remove 5 values with
| (rest, found) ->
    match Vec.remove_all (\n -> n > 2) rest with
    | (left, removed) ->
        let array = Vec.to_array left
        $"found={found} removed={removed} left={array}"
```

実行結果:

```text
found=true removed=2 left=[1]
```

### 排他借用する `Vec.push`（2027）

```tsuzuri run=42
let mut values: Vec<i64> = Vec.empty()
Vec.push 20 values
Vec.push 22 values
values[0] + values[1]
```

対象は最後です。`Vec.push (values[0] + 1) values` は値を求めてから対象を貸せますが、
`values |> Vec.push (values[0] + 1)` は先に対象を貸すので `E1014` です。
生きた要素の借用と重なる更新も拒否します。借用を持つ要素を格納する push は、
region の格納契約を保つ直接の完全適用だけを使えます。素の値の push は部分適用や関数値でも使えます。
消費して Vec を返す操作が必要なら `Vec.push_owned`、従来の .NET 対応 API なら `Vec.add` を使います。
版を明示的にまたぐ排他更新の入口は `Vec.push_mut` です。全版で
`Vec.push_mut value (ref mut values)` と書け、2027 の `Vec.push` と同じく unit を返します。

2027 では可変な所有者・排他参照を根とする `values[index] = value` も使えます。
添字の範囲検査、古い要素の解放、格納する借用の追跡は省略しません。

### 末尾からの取り出し `Vec.pop`

`Vec.pop` は現在の `Vec` を消費し、更新後の `Vec` と末尾要素 `Maybe<T>` のタプル `(Vec<T>, Maybe<T>)` を返します。

```tsuzuri run=popped%3D30%20rem_len%3D2
let mut values: Vec<i64> = Vec.empty()
values = Vec.push_owned 10 values
values = Vec.push_owned 20 values
values = Vec.push_owned 30 values

let popped_val = match Vec.pop values with
| (remaining, Some value) ->
    values = remaining
    to_string value
| (remaining, None) ->
    values = remaining
    "none"

let rem_len = Vec.length (ref values)
$"popped={popped_val} rem_len={rem_len}"
```

実行結果:

```text
popped=30 rem_len=2
```

`pop` は、文字列や非 `Copy` のレコードも所有値として取り出せます。全フィールドが `Copy` のレコードは `Copy` です。

### 既存要素の置換と交換

- `Vec.set index new_value values`: 指定インデックスの要素を置き換えた新しい `Vec` を返します。
- `Vec.swap i j values`: 指定された 2 つのインデックスの要素を安全に入れ替えた `Vec` を返します。

いずれも範囲外のインデックスが指定された場合は安全にトラップします。

```tsuzuri run=result%3D%5B10%2C%2099%2C%2030%5D
let mut values: Vec<i64> = Vec.empty()
values = Vec.push_owned 10 values
values = Vec.push_owned 20 values
values = Vec.push_owned 30 values
values = Vec.set 1 99 values
let arr = Vec.to_array values
$"result={arr}"
```

実行結果:

```text
result=[10, 99, 30]
```

## 容量管理と再確保

内部バッファの容量（capacity）は、要素の追加によって不足した際に、.NET の `List<T>` と同じ規則で拡張されます。現在の容量が 0 なら 4 に、そうでなければ 2 倍にし、それでも必要な個数に届かないときは必要な個数にします。

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `capacity` | `ref Vec<'a> -> i64` | 現在確保されている最大要素数を返します（`Capacity` の取得） |
| `set_capacity` | `i64 -> Vec<'a> -> Vec<'a>` | 容量をちょうど指定した値にします（`Capacity` の設定）。長さより小さい値はトラップです |
| `ensure_capacity` | `i64 -> Vec<'a> -> Vec<'a>` | 容量が指定した値以上になるようにします。足りなければ上の規則で増やします（`EnsureCapacity`）。負の値はトラップです |
| `trim_excess` | `Vec<'a> -> Vec<'a>` | 長さが容量の 90% 未満なら、容量を長さに縮めます（`TrimExcess`） |
| `reserve` | `i64 -> Vec<'a> -> Vec<'a>` | 追加で `additional` 個の要素を格納できるよう容量を確保します |
| `truncate` | `i64 -> Vec<'a> -> Vec<'a>` | 指定した長さまで縮小し、溢れた要素を先頭側から順に解放します |
| `clear` | `Vec<'a> -> Vec<'a>` | 全要素を解放し、長さ 0 の空の `Vec` にします（容量は維持されます） |

`reserve additional values` は「総容量」ではなく「追加で必要な個数」を指定します。.NET の `EnsureCapacity` は逆に総容量を指定するので、`ensure_capacity n values` は `reserve (n - 長さ) values` とは違う結果になることがあります。

容量の増え方は、足す関数によって 2 通りあります。

| 増え方 | 関数 | 新しい容量 |
| --- | --- | --- |
| 倍々 | `add`、`push`、`insert`、`add_range_seq`、`insert_range_seq`、`of_seq`、`reserve` | 4（空のとき）から、必要な個数に届くまで 2 倍にします。要素を 1 つずつ足す .NET の動きと同じです |
| 1 回で | `add_range`、`append`、`insert_range`、`ensure_capacity` | 現在の 2 倍（空なら 4）と必要な個数の大きい方です。配列や `List<T>` をまとめて足す .NET の動きと同じです |

```tsuzuri run=after_five%3D8%20after_nine%3D16%20ensured%3D40%20exact%3D12%20trimmed%3D9
let mut values: Vec<i64> = Vec.empty()
for n in 1i64 ..= 5 do values = Vec.add n values
let after_five = Vec.capacity (ref values)
for n in 6i64 ..= 9 do values = Vec.add n values
let after_nine = Vec.capacity (ref values)
values = Vec.ensure_capacity 40 values
let ensured = Vec.capacity (ref values)
values = Vec.set_capacity 12 values
let exact = Vec.capacity (ref values)
values = Vec.trim_excess values
let trimmed = Vec.capacity (ref values)
$"after_five={after_five} after_nine={after_nine} ensured={ensured} exact={exact} trimmed={trimmed}"
```

実行結果:

```text
after_five=8 after_nine=16 ensured=40 exact=12 trimmed=9
```

`trim_excess` は、長さが容量の 90% 未満（容量 12 なら 10 未満）のときだけ縮めます。長さが 90% 以上なら、再確保の費用に見合わないので何もしません。

`truncate` は指定した長さまで縮め、溢れた要素を解放します。容量は変わりません。

```tsuzuri run=before%3D3%20after%3D1
let mut values: Vec<string> = Vec.empty()
values = Vec.push_owned "a" values
values = Vec.push_owned "b" values
values = Vec.push_owned "c" values
let before_len = Vec.length (ref values)
values = Vec.truncate 1 values
let after_len = Vec.length (ref values)
$"before={before_len} after={after_len}"
```

実行結果:

```text
before=3 after=1
```

## 並べ替えと二分探索

`Vec.reverse` は並びを逆にし、`Vec.sort` は `Ord` の順に並べ替えます。範囲だけを対象にする `reverse_range`・`sort_range`、比較関数を渡す `sort_with`・`sort_range_with`、要素を借用する比較関数を渡す `sort_withr`・`sort_range_withr` もあります。比較関数は、.NET の `Comparison<T>` と同じく、負の数・0・正の数を返します。

- **安定**: 等しい要素は元の順序を保ちます。.NET の `Sort` は不安定なので、ここが違います。
- **NaN**: `sort` と `sort_range` は、NaN を他のどの数より前に置き、NaN どうしは等しいとみなします（.NET の `double.CompareTo` と同じです）。
- **メモリ**: `sort`、`sort_range`、`sort_withr`、`sort_range_withr` は、その場で並べ替えます（挿入ソートと、回転による併合 SymMerge）。NaN を前へ移す処理も、回転で行うので、追加の領域は要りません。比較は $O(n \log n)$ 回、要素の入れ替えは $O(n \log^2 n)$ 回です。`sort_with` と `sort_range_with` は、対象の長さの一時的な配列を使います。

```tsuzuri run=order%3Dfig%20pear%20kiwi%20banana
let mut words = Vec.of_array ["pear", "fig", "banana", "kiwi"]
words = Vec.sort_withr (\left right -> left.length - right.length) words
let first = Vec.at 0 (ref words)
let second = Vec.at 1 (ref words)
let third = Vec.at 2 (ref words)
let fourth = Vec.at 3 (ref words)
$"order={first} {second} {third} {fourth}"
```

実行結果:

```text
order=fig pear kiwi banana
```

`pear` と `kiwi` は長さが等しいので、元の順序のままです。

`Vec.binary_search item values` は、`Ord` の順に並んだ `Vec` から値を探します。見つかれば、その位置を返します。見つからなければ、値を入れるべき位置 `p`（それより小さい要素の個数）の補数 `-1 - p`（.NET の `~p`）を返します。負の数なので、見つかったかどうかがすぐ分かり、`Vec.insert (-1 - result) item values` で並びを保ったまま挿入できます。等しい要素が複数あるときの位置は、.NET の `BinarySearch` と同じ手順で探した結果です。比較関数を渡す `binary_search_with`・`binary_search_withr`・`binary_search_range` は、比較関数を `comparison 要素 探す値` の順に呼びます。

```tsuzuri run=found%3D3%20missing%3D-3%20sorted%3D%5B1%2C%203%2C%204%2C%205%2C%207%2C%209%5D
let mut numbers = Vec.of_array [5, 3, 9, 1, 7]
numbers = Vec.sort numbers
let found = Vec.binary_search 7 (ref numbers)
let missing = Vec.binary_search 4 (ref numbers)
numbers = Vec.insert (-1 - missing) 4 numbers
let sorted = Vec.to_array numbers
$"found={found} missing={missing} sorted={sorted}"
```

実行結果:

```text
found=3 missing=-3 sorted=[1, 3, 4, 5, 7, 9]
```

## 探索

- `contains`、`index_of`、`last_index_of`、`remove` は `Eq`（`==`）で比べます。位置を返す関数は、見つからないと `-1` を返します。`index_of_from`・`index_of_range` は位置（と個数）から後ろへ、`last_index_of_from`・`last_index_of_range` は位置から前へ向かって探します。
- `find_index` と `find_last_index` は、述語（`'a -> bool`）が真になる最初・最後の位置を返します。`_from`・`_range` を付けた形で範囲を指定できます。
- `find` と `find_last` は、見つけた要素を `Maybe` で返します。.NET が要素の型の既定値を返す「見つからない」場合は `None` です。
- `find_all` は、述語が真の要素の複製を新しい `Vec` に集めます。`exists` は真の要素があるか、`true_for_all` はすべての要素で真か（空の `Vec` では `true`）を返します。

```tsuzuri run=index%3D2%20last%3D3%20missing%3D-1%20first_even%3D8%20evens%3D3%20all_positive%3Dtrue
let values = Vec.of_array [3, 8, 5, 8, 2]
let index = Vec.index_of 5 (ref values)
let last = Vec.last_index_of 8 (ref values)
let missing = Vec.index_of 7 (ref values)
let first_even = match Vec.find (\n -> n % 2 == 0) (ref values) with
| Some n -> n
| None -> -1
let evens = Vec.find_all (\n -> n % 2 == 0) (ref values)
let even_count = Vec.length (ref evens)
let all_positive = Vec.true_for_all (\n -> n > 0) (ref values)
$"index={index} last={last} missing={missing} first_even={first_even} evens={even_count} all_positive={all_positive}"
```

実行結果:

```text
index=2 last=3 missing=-1 first_even=8 evens=3 all_positive=true
```

## 走査・変換・範囲の取り出し

- `for_each` は、要素ごとに関数を呼びます（`ForEach`）。
- `convert_all` は、各要素を変換した新しい `Vec` を返します（`ConvertAll`）。結果の容量は要素数です。
- `get_range` と `slice` は、位置と個数で指定した範囲の要素を複製した、新しい `Vec` を返します。容量は個数です。
- `copy_to` は、要素を配列へ複製します。`copy_to_at` は配列の書き込み位置を、`copy_to_range` は `Vec` の範囲と配列の書き込み位置を指定します。配列は `ref mut` で渡します。

```tsuzuri run=squares%3D%5B1%2C%204%2C%209%2C%2016%2C%2025%5D%20middle%3D%5B2%2C%203%2C%204%5D%20target%3D%5B0%2C%201%2C%202%2C%203%2C%204%2C%205%2C%200%5D
let values = Vec.of_array [1, 2, 3, 4, 5]
let squares = Vec.convert_all (\n -> n * n) (ref values)
let middle = Vec.get_range 1 3 (ref values)
let mut target = [0, 0, 0, 0, 0, 0, 0]
Vec.copy_to_at (ref mut target) 1 (ref values)
let squares_array = Vec.to_array squares
let middle_array = Vec.to_array middle
$"squares={squares_array} middle={middle_array} target={target}"
```

実行結果:

```text
squares=[1, 4, 9, 16, 25] middle=[2, 3, 4] target=[0, 1, 2, 3, 4, 5, 0]
```

## 要素へのアクセスと借用

### 添字アクセスと借用関数

- `values[i]`: 読み取り専用の要素そのものです。`ref` 型ではありません。範囲外はトラップします。非 `Copy` はムーブできず、共有借用は `ref values[i]` で取ります。`i` は [配列の添字](./array.md#要素へのアクセスと境界検査) と同じく、`i8`〜`i128` と `i8u`〜`i128u` のどの整数型でも書けます。`i64` に収まらない値もトラップします。
- `Vec.at index (ref values)`: `ref T` を返します。範囲外はトラップします。こちらは参照なので `deref` できます。
- `Vec.get index (ref values)`: 要素を `Maybe<T>` で返します。**`Copy<T>` 制約が必要**です。範囲外は `None` を返します。

```tsuzuri run=at%3Dhello%20get%3D42%20missing%3Dnone
let mut words: Vec<string> = Vec.empty()
words = Vec.push_owned "hello" words

let mut numbers: Vec<i64> = Vec.empty()
numbers = Vec.push_owned 42 numbers

let word_ref = Vec.at 0 (ref words)
let num_val = match Vec.get 0 (ref numbers) with
| Some v -> to_string v
| None -> "none"
let missing_val = match Vec.get 99 (ref numbers) with
| Some v -> to_string v
| None -> "none"

$"at={word_ref} get={num_val} missing={missing_val}"
```

実行結果:

```text
at=hello get=42 missing=none
```

> [!WARNING]
> **要素への排他借用参照は取得できません**
> `ref mut values[0]` のように要素そのものの排他借用は取れません。コンパイルエラーは `E1014` です。
> 要素の書き換えには `Vec.set` や `Vec.swap` を使用してください。また、`Vec.at` で得た共有借用参照が生存している間は、バッファ自体の更新や移動は禁止されます。

## Array とのゼロコピー移送

`Vec` と固定長配列 `[T]` は、相互に所有バッファをそのまま移送できます。

- `Vec.to_array values`: `Vec` を消費し、内部バッファを所有配列 `[T]` として返します。
- `Vec.of_array array`: 所有配列 `[T]` を消費し、そのバッファを持つ `Vec<T>` を返します。

```tsuzuri run=converted%3D%5B1%2C%202%2C%203%2C%204%5D%20sum%3D10
let mut buffer = Vec.of_array [1, 2, 3]
buffer = Vec.push_owned 4 buffer
let array = Vec.to_array buffer
$"converted={array} sum={Array.sum (ref array)}"
```

実行結果:

```text
converted=[1, 2, 3, 4] sum=10
```

所有しているバッファはポインタを渡すだけで移せます。ただし、`Copy` な配列を後で使うときの引数渡しや、スタック上の配列をヒープへ上げる処理は、通常の所有権規則どおりコピーします。「常に一切コピーしない」わけではありません。空の配列を渡すと、不要なバッファは解放され、空の `Vec` に正規化されます。

.NET の `ToArray` は、要素を新しい配列に複製して `List<T>` を残します。`Vec.to_array` は `Vec` を消費してバッファを移すので、そこが違います。`Vec` を残したいときは、`Vec.clone`（`Copy` の要素）や `Vec.get_range 0 n (ref values)` で複製してから `to_array` を呼ぶか、用意した配列へ `copy_to` で複製します。

## .NET の List<T> との対応

[.NET の `List<T>`](https://learn.microsoft.com/ja-jp/dotnet/api/system.collections.generic.list-1?view=net-10.0) の公開メンバーは、`AsReadOnly` を除いてすべて、同じ名前（PascalCase を snake_case にしたもの。`FindLastIndex` は `find_last_index`）の関数があります。引数は .NET の順で、受け取る `Vec` は最後です。関数の添字・個数・容量はすべて `i64` です（添字式 `values[i]` だけは、どの整数型でも書けます）。

### 引数の順序

.NET のメソッドは `list.Insert(index, item)` のように `List<T>` を受け手として書きます。Tsuzuri の関数は、受け手の `Vec` を最後の引数にして、残りの引数を .NET の順に並べます。標準ライブラリのほかのモジュールも同じ規則です（[標準ライブラリの引数の順序](./argument-order.md)）。

| .NET | Tsuzuri |
| --- | --- |
| `list.Add(item)` | `Vec.add item values` |
| `list.Insert(index, item)` | `Vec.insert index item values` |
| `list.RemoveRange(index, count)` | `Vec.remove_range index count values` |
| `list.IndexOf(item, index, count)` | `Vec.index_of_range item index count values` |
| `list.FindIndex(start, count, match)` | `Vec.find_index_range start count match values` |
| `list.Sort(index, count, comparer)` | `Vec.sort_range_with index count comparer values` |
| `list.BinarySearch(index, count, item, comparer)` | `Vec.binary_search_range index count item comparer values` |
| `list.CopyTo(index, array, arrayIndex, count)` | `Vec.copy_to_range index array arrayIndex count values` |

`Vec` を最後に置くのは、F# の `List` や `Array` と同じです。`values |> Vec.insert 1 10` のようにパイプでつなげられ、`Vec.sort >> Vec.reverse` のように関数合成もできます。もう 1 つの利点は、`Vec` が所有権ごと渡される最後の引数なので、前の引数が同じ `Vec` を借りて計算できることです（`Vec.insert (Vec.length (ref values)) item values`）。`Vec` を最初に渡す順では、`Vec` の移動が先に起きるので、`E1012` になります。

`Vec.push`・`Vec.set`・`Vec.swap`・`Vec.reserve`・`Vec.truncate`・`Vec.get`・`Vec.at` も同じ順です（`Vec.set index item values`）。

### メンバーの対応

| .NET | Tsuzuri |
| --- | --- |
| `new List<T>()` | `Vec.empty()` |
| `new List<T>(int capacity)` | `Vec.with_capacity capacity` |
| `new List<T>(IEnumerable<T>)` | `Vec.of_array`、`Vec.of_list`、`Vec.of_seq`、複製は `Vec.clone` |
| `Capacity`（取得、設定） | `Vec.capacity`、`Vec.set_capacity` |
| `Count` | `Vec.length`（`.length` も同じです） |
| `this[int]`（取得、設定） | `values[i]`、`Vec.at`、`Vec.get`、`Vec.set` |
| `Add` | `add` |
| `AddRange` | `add_range`（配列）、`add_range_seq`（`Seq`）、`append`（`Vec`） |
| `AsReadOnly` | 関数はありません。`ref values` の借用が読み取り専用の見え方です |
| `BinarySearch`（3 つのオーバーロード） | `binary_search`、`binary_search_with`、`binary_search_range` |
| `Clear` | `clear` |
| `Contains` | `contains` |
| `ConvertAll` | `convert_all` |
| `CopyTo`（3 つ） | `copy_to`、`copy_to_at`、`copy_to_range` |
| `EnsureCapacity` | `ensure_capacity` |
| `Exists` | `exists` |
| `Find`、`FindLast` | `find`、`find_last`（`Maybe` を返します） |
| `FindAll` | `find_all` |
| `FindIndex`（3 つ） | `find_index`、`find_index_from`、`find_index_range` |
| `FindLastIndex`（3 つ） | `find_last_index`、`find_last_index_from`、`find_last_index_range` |
| `ForEach` | `for_each` |
| `GetEnumerator` | `Vec.iter`、`for x in values do` |
| `GetRange`、`Slice` | `get_range`、`slice` |
| `IndexOf`（3 つ） | `index_of`、`index_of_from`、`index_of_range` |
| `Insert` | `insert` |
| `InsertRange` | `insert_range`（配列）、`insert_range_seq`（`Seq`） |
| `LastIndexOf`（3 つ） | `last_index_of`、`last_index_of_from`、`last_index_of_range` |
| `Remove`、`RemoveAll` | `remove`、`remove_all` |
| `RemoveAt`、`RemoveRange` | `remove_at`、`remove_range` |
| `Reverse`（2 つ） | `reverse`、`reverse_range` |
| `Sort`（4 つ） | `sort`、`sort_with`（`Comparison<T>` と `IComparer<T>`）、`sort_range`、`sort_range_with` |
| `ToArray` | `to_array`（複製せず、バッファを移します） |
| `TrimExcess` | `trim_excess` |
| `TrueForAll` | `true_for_all` |

### 型と失敗

| .NET | Tsuzuri |
| --- | --- |
| `int`（添字、個数、容量） | `i64`。比較関数の戻り値も `i64` |
| 例外（`ArgumentOutOfRangeException`、`ArgumentException`） | トラップ |
| `default(T)`（`Find`、`FindLast` が見つけられなかったとき） | `Maybe` の `None` |
| `bool` を返す `Remove`、`int` を返す `RemoveAll` | 更新した `Vec` との組 `(Vec<'a> * bool)`、`(Vec<'a> * i64)` |
| `Predicate<T>`、`Action<T>`、`Converter<TInput, TOutput>` | `'a -> bool`、`'a -> unit`、`'a -> 'b` |
| `Comparison<T>`、`IComparer<T>` | 比較関数 `'a -> 'a -> i64`。負の数・0・正の数を返します |
| `IEnumerable<T>`（`AddRange`、`InsertRange`、コンストラクター） | 配列 `ref ['a]`、`Seq<'a>`、`Vec<'a>`（関数ごとに分かれます） |
| `T[]`（`CopyTo`、`ToArray`） | 配列 `['a]` |
| `Equals`（`Contains`、`IndexOf`、`Remove`） | `Eq`（`==`） |
| `IComparable<T>`（`Sort()`、`BinarySearch`） | `Ord` |

トラップになるのは、.NET が例外を投げる次の場面です。

- 添字が範囲外（`values[i]`、`at`、`set`、`swap`、`remove_at`。`insert` と `insert_range` は 0 から長さまで許します）
- 個数が負、または範囲（位置と個数）が `Vec` の外（`remove_range`、`get_range`、`slice`、`reverse_range`、`sort_range`、`index_of_range`、`find_index_range`、`binary_search_range`、`copy_to_range` など）
- 書き込み先の配列が足りない（`copy_to`、`copy_to_at`、`copy_to_range`）
- 容量が負（`with_capacity`、`ensure_capacity`）、または `set_capacity` に長さより小さい値を渡す

空の `Vec` に対する後ろからの探索は、.NET の特別扱いを引き継いでいます。`last_index_of_range` は空なら常に `-1` を返し、`last_index_of_from` は位置が `-1`（長さ未満の値）なら `-1` を返して、0 以上はトラップです。`find_last_index_from` と `find_last_index_range` は、空の `Vec` では位置が `-1` のときだけ許されます。

### 更新は `Vec` を消費する

.NET の `List<T>` はその場で書き換わります。Tsuzuri の `Vec` は所有値なので、更新する関数は `Vec` を受け取って、更新した `Vec` を返します（`Vec.push` と同じです）。`values = Vec.add x values` のように同じ変数へ代入し直すか、`values |> Vec.add x` のようにパイプでつなぎます。そのため、.NET が `void` を返す関数は `Vec<'a>` を返し、結果のある `Remove` と `RemoveAll` は組を返します。

更新した後の古い変数は使えません（`E1012`）。また、`Vec.at` や `iter` で要素を借りている間は、その `Vec` を更新できません（`E1014`）。.NET の `List<T>` は、列挙中の変更を実行時に `InvalidOperationException` で検出しますが、Tsuzuri はコンパイル時に禁じます。

### コールバックと `r` 形

要素を値で受けるコールバックは、要素が `Copy` であることを要求します。文字列などの `Copy` でない要素には、値を取る形は使えません（`E1005`）。そのための借用形が、関数名の末尾に `r` を付けた関数です。コールバックは要素の共有参照 `ref 'a` を受け取り、要素そのものを返す関数は参照を返すので、どんな要素型にも使えます。

| 値で受ける形（.NET に対応） | 借用形 | 借用形のシグネチャ |
| --- | --- | --- |
| `find_index` | `find_indexr` | `(ref 'a -> bool) -> ref Vec<'a> -> i64` |
| `find_last_index` | `find_last_indexr` | `(ref 'a -> bool) -> ref Vec<'a> -> i64` |
| `exists` | `existsr` | `(ref 'a -> bool) -> ref Vec<'a> -> bool` |
| `true_for_all` | `true_for_allr` | `(ref 'a -> bool) -> ref Vec<'a> -> bool` |
| `find` | `findr` | `(ref 'a -> bool) -> ref Vec<'a> -> Maybe<ref 'a>` |
| `find_last` | `find_lastr` | `(ref 'a -> bool) -> ref Vec<'a> -> Maybe<ref 'a>` |
| `for_each` | `for_eachr` | `(ref 'a -> unit) -> ref Vec<'a> -> unit` |
| `convert_all` | `convert_allr` | `(ref 'a -> 'b) -> ref Vec<'a> -> Vec<'b>` |
| `remove_all` | `remove_allr` | `(ref 'a -> bool) -> Vec<'a> -> (Vec<'a> * i64)` |
| `sort_with`、`sort_range_with` | `sort_withr`、`sort_range_withr` | `(ref 'a -> ref 'a -> i64) -> Vec<'a> -> Vec<'a>` |
| `binary_search_with` | `binary_search_withr` | `ref 'a -> (ref 'a -> ref 'a -> i64) -> ref Vec<'a> -> i64` |
| `contains` | `containsr` | `Eq<'a> => ref 'a -> ref Vec<'a> -> bool` |
| `index_of` | `index_ofr` | `Eq<'a> => ref 'a -> ref Vec<'a> -> i64` |
| `last_index_of` | `last_index_ofr` | `Eq<'a> => ref 'a -> ref Vec<'a> -> i64` |
| `remove` | `remover` | `Eq<'a> => ref 'a -> Vec<'a> -> (Vec<'a> * bool)` |
| `binary_search` | `binary_searchr` | `Ord<'a> => ref 'a -> ref Vec<'a> -> i64` |

`_from` と `_range` を付けた形（`find_index_range`、`copy_to_range` など）は、値で受ける形だけです。`contains`、`index_of`、`remove`、`binary_search` は探す値を値で受け取り（消費し）、`Copy` を要求しません。名前付きの文字列などを消費したくないときは、`containsr` や `index_ofr` に参照を渡します。

```tsuzuri run=has_long%3Dtrue%20position%3D1%20found%3D3
let words = Vec.of_array ["pear", "fig", "banana"]
let has_long = Vec.existsr (\word -> word.length > 5) (ref words)
let position = Vec.index_ofr "fig" (ref words)
let found = match Vec.findr (\word -> word.length == 3) (ref words) with
| Some word -> word.length
| None -> 0
$"has_long={has_long} position={position} found={found}"
```

実行結果:

```text
has_long=true position=1 found=3
```

### .NET と違う点

- **`Eq` は IEEE です**。`contains`、`index_of`、`last_index_of`、`remove` は `Eq`（`==`）で比べるので、浮動小数点数の NaN は自分自身とも等しくなく、見つかりません。.NET の `Equals` は NaN を NaN と等しいとみなします。一方、`sort` と `binary_search` は .NET と同じく NaN を最小の値として扱います。
- **`sort` は安定です**。等しい要素の順序が変わりません。.NET の `Sort` は不安定です。
- **`find` と `find_last` は `Maybe` を返します**。.NET は、見つからないと要素の型の既定値を返すので、既定値と見つけた値を区別できません。
- **`to_array` は複製しません**。バッファを移して、`Vec` を消費します。
- **`remove_all` の述語は先頭から順に 1 回ずつ呼ばれます**。消される要素は、述語を呼び終えたあとに、順序を決めずに解放されます。
- **`AsReadOnly` はありません**。`ref values` が、`Vec` を読み取り専用で見せる借用です。

### .NET にない関数

次の関数は Tsuzuri だけのものです。

| 関数 | 説明 |
| --- | --- |
| `push`、`pop` | `push` は `add` と同じ組み込み関数です。`pop` は末尾の要素を取り出し、`(残りの Vec, Maybe)` を返します |
| `reserve` | 追加で `additional` 個の要素が入る容量を確保します |
| `truncate`、`swap`、`is_empty` | 長さを縮める、2 つの位置を入れ替える、空かどうか調べる |
| `at` | 要素への共有参照を返します |
| `of_array`、`of_list`、`of_seq` | 配列・リスト・`Seq` からの生成です |
| `append` | 別の `Vec` の要素を移して連結します。要素は複製されません |
| `add_range_seq`、`insert_range_seq` | `Seq` の要素を足す、挿入する |
| `clone` | `Copy` の要素を複製します |
| `iter` | 要素への共有参照を列挙する `Seq` です |
| `containsr` など `r` を付けた関数 | 上の「コールバックと `r` 形」です |

## API リファレンス

すべての関数は `std::Vec` モジュール（または言語組み込み）に属しています。括弧の中は、対応する .NET のメンバーです。

### 生成・情報取得

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `empty` | `fn() -> Vec<'a>` | 空の `Vec` を作ります。呼び出しは `Vec.empty()` です。`unit ->` ではありません |
| `with_capacity` | `i64 -> Vec<'a>` | 指定容量を事前確保した空の `Vec` を生成します |
| `of_array` | `['a] -> Vec<'a>` | 配列 `['a]` を内部バッファとして取り込みます。容量は要素数です |
| `of_list` | `Copy<'a> => ref [\|'a\|] -> Vec<'a>` | リストの要素を複製した `Vec` を作ります。容量は要素数です |
| `of_seq` | `Seq<'a> -> Vec<'a>` | 列の要素を 1 つずつ足した `Vec` を作ります |
| `clone` | `Copy<'a> => ref Vec<'a> -> Vec<'a>` | `Copy` 要素を持つ独立した `Vec` を複製します。容量は要素数です |
| `length` | `ref Vec<'a> -> i64` | 現在格納されている要素数を返します（`.length` と同じ。`Count`） |
| `capacity` | `ref Vec<'a> -> i64` | 現在確保されている内部容量を返します（`Capacity`） |
| `is_empty` | `ref Vec<'a> -> bool` | 要素数が 0 かどうかを返します |

### 容量

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `set_capacity` | `i64 -> Vec<'a> -> Vec<'a>` | 容量をちょうど指定した値にします。長さより小さい値はトラップです（`Capacity` の設定） |
| `ensure_capacity` | `i64 -> Vec<'a> -> Vec<'a>` | 容量を指定した値以上にします（`EnsureCapacity`） |
| `trim_excess` | `Vec<'a> -> Vec<'a>` | 長さが容量の 90% 未満なら、容量を長さに縮めます（`TrimExcess`） |
| `reserve` | `i64 -> Vec<'a> -> Vec<'a>` | 追加要素数分の容量を確保します |

### 追加・挿入

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `push` | `'a -> Vec<'a> -> Vec<'a>` | 末尾に要素を追加した新しい `Vec` を返します |
| `add` | `'a -> Vec<'a> -> Vec<'a>` | `push` と同じです（`Add`） |
| `add_range` | `Copy<'a> => ref ['a] -> Vec<'a> -> Vec<'a>` | 配列の要素を複製して末尾に足します（`AddRange`） |
| `add_range_seq` | `Seq<'a> -> Vec<'a> -> Vec<'a>` | `Seq` の要素を順に末尾へ足します（`AddRange`） |
| `append` | `Vec<'a> -> Vec<'a> -> Vec<'a>` | 最初の `Vec` の要素を、最後の `Vec` の末尾へ移します（`AddRange`） |
| `insert` | `i64 -> 'a -> Vec<'a> -> Vec<'a>` | 位置に要素を挿入します。位置は 0 から長さまでで、範囲外はトラップです（`Insert`） |
| `insert_range` | `Copy<'a> => i64 -> ref ['a] -> Vec<'a> -> Vec<'a>` | 位置に配列の要素を複製して挿入します（`InsertRange`） |
| `insert_range_seq` | `i64 -> Seq<'a> -> Vec<'a> -> Vec<'a>` | 位置に `Seq` の要素を挿入します（`InsertRange`） |

### 削除・更新

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `pop` | `Vec<'a> -> (Vec<'a> * Maybe<'a>)` | 末尾要素を取り出した結果と残りの `Vec` を返します |
| `set` | `i64 -> 'a -> Vec<'a> -> Vec<'a>` | 指定インデックスの要素を置換します。範囲外はトラップです（`this[int]` の設定） |
| `swap` | `i64 -> i64 -> Vec<'a> -> Vec<'a>` | 2 つの位置の要素を入れ替えます。範囲外はトラップです |
| `remove_at` | `i64 -> Vec<'a> -> Vec<'a>` | 位置の要素を消します（`RemoveAt`） |
| `remove_range` | `i64 -> i64 -> Vec<'a> -> Vec<'a>` | 位置と個数で指定した範囲を消します（`RemoveRange`） |
| `remove` | `Eq<'a> => 'a -> Vec<'a> -> (Vec<'a> * bool)` | 最初に等しい要素を消します（`Remove`） |
| `remover` | `Eq<'a> => ref 'a -> Vec<'a> -> (Vec<'a> * bool)` | `remove` の、探す値を借用する形です |
| `remove_all` | `Copy<'a> => ('a -> bool) -> Vec<'a> -> (Vec<'a> * i64)` | 述語が真の要素をすべて消し、個数を返します（`RemoveAll`） |
| `remove_allr` | `(ref 'a -> bool) -> Vec<'a> -> (Vec<'a> * i64)` | `remove_all` の、要素を借用する形です |
| `truncate` | `i64 -> Vec<'a> -> Vec<'a>` | 指定した長さまで縮小します |
| `clear` | `Vec<'a> -> Vec<'a>` | 全要素を解放して空にします（`Clear`。容量は変わりません） |

### 並べ替え・二分探索

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `reverse` | `Vec<'a> -> Vec<'a>` | 並びを逆にします（`Reverse()`） |
| `reverse_range` | `i64 -> i64 -> Vec<'a> -> Vec<'a>` | 位置と個数で指定した範囲を逆にします（`Reverse(Int32, Int32)`） |
| `sort` | `Ord<'a> => Vec<'a> -> Vec<'a>` | `Ord` の順に安定に並べ替えます。NaN は先頭です（`Sort()`） |
| `sort_range` | `Ord<'a> => i64 -> i64 -> Vec<'a> -> Vec<'a>` | 範囲だけを並べ替えます（`Sort(Int32, Int32, IComparer<T>)`） |
| `sort_with` | `Copy<'a> => ('a -> 'a -> i64) -> Vec<'a> -> Vec<'a>` | 比較関数で並べ替えます（`Sort(Comparison<T>)`、`Sort(IComparer<T>)`） |
| `sort_range_with` | `Copy<'a> => i64 -> i64 -> ('a -> 'a -> i64) -> Vec<'a> -> Vec<'a>` | 範囲だけを比較関数で並べ替えます |
| `sort_withr` | `(ref 'a -> ref 'a -> i64) -> Vec<'a> -> Vec<'a>` | `sort_with` の、要素を借用する形です |
| `sort_range_withr` | `i64 -> i64 -> (ref 'a -> ref 'a -> i64) -> Vec<'a> -> Vec<'a>` | `sort_range_with` の、要素を借用する形です |
| `binary_search` | `Ord<'a> => 'a -> ref Vec<'a> -> i64` | 並んだ `Vec` から探します。なければ位置の補数を返します（`BinarySearch(T)`） |
| `binary_searchr` | `Ord<'a> => ref 'a -> ref Vec<'a> -> i64` | `binary_search` の、探す値を借用する形です |
| `binary_search_with` | `Copy<'a> => 'a -> ('a -> 'a -> i64) -> ref Vec<'a> -> i64` | 比較関数で探します（`BinarySearch(T, IComparer<T>)`） |
| `binary_search_withr` | `ref 'a -> (ref 'a -> ref 'a -> i64) -> ref Vec<'a> -> i64` | `binary_search_with` の、要素を借用する形です |
| `binary_search_range` | `Copy<'a> => i64 -> i64 -> 'a -> ('a -> 'a -> i64) -> ref Vec<'a> -> i64` | 位置と個数で指定した範囲を探します（`BinarySearch(Int32, Int32, T, IComparer<T>)`） |

### 探索・判定

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `contains` | `Eq<'a> => 'a -> ref Vec<'a> -> bool` | 等しい要素があるか調べます（`Contains`） |
| `containsr` | `Eq<'a> => ref 'a -> ref Vec<'a> -> bool` | `contains` の、探す値を借用する形です |
| `index_of` | `Eq<'a> => 'a -> ref Vec<'a> -> i64` | 最初に等しい要素の位置、なければ `-1` です（`IndexOf(T)`） |
| `index_ofr` | `Eq<'a> => ref 'a -> ref Vec<'a> -> i64` | `index_of` の、探す値を借用する形です |
| `index_of_from` | `Eq<'a> => 'a -> i64 -> ref Vec<'a> -> i64` | 位置から後ろを探します（`IndexOf(T, Int32)`） |
| `index_of_range` | `Eq<'a> => 'a -> i64 -> i64 -> ref Vec<'a> -> i64` | 位置と個数で指定した範囲を探します（`IndexOf(T, Int32, Int32)`） |
| `last_index_of` | `Eq<'a> => 'a -> ref Vec<'a> -> i64` | 最後に等しい要素の位置、なければ `-1` です（`LastIndexOf(T)`） |
| `last_index_ofr` | `Eq<'a> => ref 'a -> ref Vec<'a> -> i64` | `last_index_of` の、探す値を借用する形です |
| `last_index_of_from` | `Eq<'a> => 'a -> i64 -> ref Vec<'a> -> i64` | 位置から前へ探します（`LastIndexOf(T, Int32)`） |
| `last_index_of_range` | `Eq<'a> => 'a -> i64 -> i64 -> ref Vec<'a> -> i64` | 位置から前へ、個数の範囲を探します（`LastIndexOf(T, Int32, Int32)`） |
| `find_index` | `Copy<'a> => ('a -> bool) -> ref Vec<'a> -> i64` | 述語が真の最初の位置、なければ `-1` です（`FindIndex`） |
| `find_indexr` | `(ref 'a -> bool) -> ref Vec<'a> -> i64` | `find_index` の、要素を借用する形です |
| `find_index_from` | `Copy<'a> => i64 -> ('a -> bool) -> ref Vec<'a> -> i64` | 位置から後ろを探します |
| `find_index_range` | `Copy<'a> => i64 -> i64 -> ('a -> bool) -> ref Vec<'a> -> i64` | 位置と個数で指定した範囲を探します |
| `find_last_index` | `Copy<'a> => ('a -> bool) -> ref Vec<'a> -> i64` | 述語が真の最後の位置、なければ `-1` です（`FindLastIndex`） |
| `find_last_indexr` | `(ref 'a -> bool) -> ref Vec<'a> -> i64` | `find_last_index` の、要素を借用する形です |
| `find_last_index_from` | `Copy<'a> => i64 -> ('a -> bool) -> ref Vec<'a> -> i64` | 位置から前へ探します |
| `find_last_index_range` | `Copy<'a> => i64 -> i64 -> ('a -> bool) -> ref Vec<'a> -> i64` | 位置から前へ、個数の範囲を探します |
| `find` | `Copy<'a> => ('a -> bool) -> ref Vec<'a> -> Maybe<'a>` | 述語が真の最初の要素です（`Find`）。なければ `None` です |
| `findr` | `(ref 'a -> bool) -> ref Vec<'a> -> Maybe<ref 'a>` | `find` の、要素を借用する形です |
| `find_last` | `Copy<'a> => ('a -> bool) -> ref Vec<'a> -> Maybe<'a>` | 述語が真の最後の要素です（`FindLast`） |
| `find_lastr` | `(ref 'a -> bool) -> ref Vec<'a> -> Maybe<ref 'a>` | `find_last` の、要素を借用する形です |
| `find_all` | `Copy<'a> => ('a -> bool) -> ref Vec<'a> -> Vec<'a>` | 述語が真の要素を複製した `Vec` です（`FindAll`） |
| `exists` | `Copy<'a> => ('a -> bool) -> ref Vec<'a> -> bool` | 述語が真の要素があるか調べます（`Exists`） |
| `existsr` | `(ref 'a -> bool) -> ref Vec<'a> -> bool` | `exists` の、要素を借用する形です |
| `true_for_all` | `Copy<'a> => ('a -> bool) -> ref Vec<'a> -> bool` | すべての要素で述語が真か調べます。空なら `true` です（`TrueForAll`） |
| `true_for_allr` | `(ref 'a -> bool) -> ref Vec<'a> -> bool` | `true_for_all` の、要素を借用する形です |

### 走査・変換・範囲

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `for_each` | `Copy<'a> => ('a -> unit) -> ref Vec<'a> -> unit` | 要素ごとに関数を呼びます（`ForEach`） |
| `for_eachr` | `(ref 'a -> unit) -> ref Vec<'a> -> unit` | `for_each` の、要素を借用する形です |
| `convert_all` | `Copy<'a> => ('a -> 'b) -> ref Vec<'a> -> Vec<'b>` | 各要素を変換した `Vec` です。容量は要素数です（`ConvertAll`） |
| `convert_allr` | `(ref 'a -> 'b) -> ref Vec<'a> -> Vec<'b>` | `convert_all` の、要素を借用する形です |
| `get_range` | `Copy<'a> => i64 -> i64 -> ref Vec<'a> -> Vec<'a>` | 位置と個数で指定した範囲を複製した `Vec` です。容量は個数です（`GetRange`） |
| `slice` | `Copy<'a> => i64 -> i64 -> ref Vec<'a> -> Vec<'a>` | `get_range` と同じです（`Slice`） |
| `copy_to` | `Copy<'a> => ref mut ['a..] -> ref Vec<'a> -> unit` | 要素を配列の先頭から書き込みます（`CopyTo(T[])`） |
| `copy_to_at` | `Copy<'a> => ref mut ['a..] -> i64 -> ref Vec<'a> -> unit` | 要素を配列の位置から書き込みます（`CopyTo(T[], Int32)`） |
| `copy_to_range` | `Copy<'a> => i64 -> ref mut ['a..] -> i64 -> i64 -> ref Vec<'a> -> unit` | `Vec` の位置と個数の範囲を、配列の位置から書き込みます（`CopyTo(Int32, T[], Int32, Int32)`） |
| `to_array` | `Vec<'a> -> ['a]` | 内部バッファをそのまま配列 `['a]` へ移送します（`ToArray`） |
| `iter` | `ref Vec<'a> -> Seq<ref 'a>` | 各要素への共有参照を順次列挙する `Seq` を返します（`GetEnumerator`） |

### 借用

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `at` | `i64 -> ref Vec<'a> -> ref 'a` | 要素への共有参照を返します（範囲外はトラップ） |
| `get` | `Copy<'a> => i64 -> ref Vec<'a> -> Maybe<'a>` | 要素を `Some` で返します（範囲外は `None`） |

## 計算量

計算量は 1 回の呼び出しの最悪の時間で、コールバックの呼び出し・`Eq.eq`・`Ord.lt`・`Hash.hash`・要素 1 つの `Copy` の複製をそれぞれ $O(1)$ と数えます（入れ子のコレクションを持つ要素の複製は、その大きさの分かかります）。`n` は主な入力の要素数、`m` はもう 1 つの入力の要素数、`k` は結果の要素数です（結果が入れ子のコレクションなら入れ子の要素もすべて数え、`longest_probe` では表の大きさです）。`Seq` を返す関数は `Seq` を返すまでの時間で、取り出しは 1 要素ごとに $O(1)$ です。規則は [構造化タグ](../organizing-tsuzuri/documentation-comment.md#構造化タグ) にあります。表は `std/` の `@complexity` から `node scripts/gen-docs.mjs` が作ります。

<!-- generated:complexity:Vec -->
| 関数 | 計算量 | 関連 |
| --- | --- | --- |
| `Vec.iter` | $O(1)$ | - |
| `Vec.of_seq` | $O(n)$ | - |
| `Vec.of_list` | $O(n)$ | - |
| `Vec.set_capacity` | $O(k)$ | - |
| `Vec.ensure_capacity` | $O(k)$ | - |
| `Vec.trim_excess` | $O(n)$ | - |
| `Vec.add` | 償却 $O(1)$、最悪 $O(n)$ | - |
| `Vec.add_range` | 償却 $O(m)$、最悪 $O(n + m)$ | - |
| `Vec.add_range_seq` | 償却 $O(m)$、最悪 $O(n + m)$ | - |
| `Vec.append` | 償却 $O(m)$、最悪 $O(n + m)$ | - |
| `Vec.insert` | $O(n)$ | - |
| `Vec.insert_range` | $O(n + m)$ | - |
| `Vec.insert_range_seq` | $O(n + m)$ | - |
| `Vec.remove_at` | $O(n)$ | - |
| `Vec.remove_range` | $O(n)$ | - |
| `Vec.remover` | $O(n)$ | `Vec.remove` |
| `Vec.remove` | $O(n)$ | `Vec.remover` |
| `Vec.remove_allr` | $O(n)$ | `Vec.remove_all` |
| `Vec.remove_all` | $O(n)$ | `Vec.remove_allr` |
| `Vec.reverse` | $O(n)$ | - |
| `Vec.reverse_range` | $O(n)$ | - |
| `Vec.sort_range` | $O(n \log^2 n)$ | - |
| `Vec.sort` | $O(n \log^2 n)$ | - |
| `Vec.sort_range_with` | $O(n \log n)$ | `Vec.sort_range_withr` |
| `Vec.sort_with` | $O(n \log n)$ | `Vec.sort_withr` |
| `Vec.sort_range_withr` | $O(n \log^2 n)$ | `Vec.sort_range_with` |
| `Vec.sort_withr` | $O(n \log^2 n)$ | `Vec.sort_with` |
| `Vec.index_ofr` | $O(n)$ | `Vec.index_of` |
| `Vec.index_of` | $O(n)$ | `Vec.index_ofr` |
| `Vec.index_of_from` | $O(n)$ | - |
| `Vec.index_of_range` | $O(n)$ | - |
| `Vec.last_index_ofr` | $O(n)$ | `Vec.last_index_of` |
| `Vec.last_index_of` | $O(n)$ | `Vec.last_index_ofr` |
| `Vec.last_index_of_from` | $O(n)$ | - |
| `Vec.last_index_of_range` | $O(n)$ | - |
| `Vec.containsr` | $O(n)$ | `Vec.contains` |
| `Vec.contains` | $O(n)$ | `Vec.containsr` |
| `Vec.find_indexr` | $O(n)$ | `Vec.find_index` |
| `Vec.find_index` | $O(n)$ | `Vec.find_indexr` |
| `Vec.find_index_from` | $O(n)$ | - |
| `Vec.find_index_range` | $O(n)$ | - |
| `Vec.find_last_indexr` | $O(n)$ | `Vec.find_last_index` |
| `Vec.find_last_index` | $O(n)$ | `Vec.find_last_indexr` |
| `Vec.find_last_index_from` | $O(n)$ | - |
| `Vec.find_last_index_range` | $O(n)$ | - |
| `Vec.existsr` | $O(n)$ | `Vec.exists` |
| `Vec.exists` | $O(n)$ | `Vec.existsr` |
| `Vec.true_for_allr` | $O(n)$ | `Vec.true_for_all` |
| `Vec.true_for_all` | $O(n)$ | `Vec.true_for_allr` |
| `Vec.findr` | $O(n)$ | `Vec.find` |
| `Vec.find` | $O(n)$ | `Vec.findr` |
| `Vec.find_lastr` | $O(n)$ | `Vec.find_last` |
| `Vec.find_last` | $O(n)$ | `Vec.find_lastr` |
| `Vec.find_all` | $O(n)$ | - |
| `Vec.binary_searchr` | $O(\log n)$ | `Vec.binary_search` |
| `Vec.binary_search` | $O(\log n)$ | `Vec.binary_searchr` |
| `Vec.binary_search_withr` | $O(\log n)$ | `Vec.binary_search_with` |
| `Vec.binary_search_with` | $O(\log n)$ | `Vec.binary_search_withr` |
| `Vec.binary_search_range` | $O(\log n)$ | - |
| `Vec.for_eachr` | $O(n)$ | `Vec.for_each` |
| `Vec.for_each` | $O(n)$ | `Vec.for_eachr` |
| `Vec.convert_allr` | $O(n)$ | `Vec.convert_all` |
| `Vec.convert_all` | $O(n)$ | `Vec.convert_allr` |
| `Vec.get_range` | $O(k)$ | - |
| `Vec.slice` | $O(k)$ | - |
| `Vec.copy_to_range` | $O(n)$ | - |
| `Vec.copy_to_at` | $O(n)$ | - |
| `Vec.copy_to` | $O(n)$ | - |
<!-- /generated:complexity:Vec -->

### 組み込みの操作

次の操作は `std/Vec.tz` の `def` ではないので、表は手書きです。

| 操作 | 計算量 | 備考 |
| --- | --- | --- |
| 末尾への追加 `Vec.push` | 償却 $O(1)$ | 容量超過時のみメモリ再確保とコピー（倍増） |
| 末尾からの取り出し `Vec.pop` | $O(1)$ | 末尾の要素を `Maybe` の所有値として呼び出し元へ移す。要素の解放処理は実行しない |
| 添字アクセス `xs[i]` / `at` | $O(1)$ | 連続メモリへの直接アクセス |
| 要素の置換 `set` / `swap` | $O(1)$ | インデックス指定の即時更新 |
| 長さ・容量の取得 `length` / `capacity` | $O(1)$ | メタデータ参照 |
| 配列との相互変換 `to_array` / `of_array` | $O(1)$ | 所有バッファの移送。引数渡しのコピーとは別 |
| 全削除 `clear` / 短縮 `truncate` | $O(k)$ | 削除対象の $k$ 要素のデストラクタ実行 |
| 複製 `clone` | $O(n)$ | 要素のディープコピー |

## 反復の仕方

### `for ... in` ループ

`Vec` は `for ... in` で直接走査できます。`Vec.iter` は要りません。ループ変数は `ref` ではなく、読み取り専用の要素そのものです。`.length` のように読めますが、非 `Copy` を関数へ渡してムーブすると `E1012` です。共有借用が要るときは `ref text` と書きます。`Copy` の要素は、値として使うときに複製されます。

```tsuzuri run=total%3D60
let mut values: Vec<i64> = Vec.empty()
values = Vec.push_owned 10 values
values = Vec.push_owned 20 values
values = Vec.push_owned 30 values

let mut total = 0
for x in values do
    total = total + x
$"total={total}"
```

実行結果:

```text
total=60
```

```tsuzuri run=letters%3D3
let mut words: Vec<string> = Vec.empty()
words = Vec.push_owned "ab" words
words = Vec.push_owned "c" words
let mut letters = 0
for text in words do
    letters = letters + text.length
$"letters={letters}"
```

実行結果:

```text
letters=3
```

```text
for text in words do
    consume text
```

この断片は `E1012` です。`text` は `ref string` ではないので、`*text` も書けません。

### `Vec.iter` による遅延走査

遅延列として扱いたいときは `Vec.iter (ref values)` を呼びます。こちらは `Seq<ref 'a>` なので、ループ変数は `ref` です。直接の `for x in values` とは型が違います。

```tsuzuri run=total%3D2
let mut letters: Vec<string> = Vec.empty()
letters = Vec.push_owned "a" letters
letters = Vec.push_owned "b" letters

let mut total = 0
for letter in Vec.iter (ref letters) do
    total = total + letter.length
$"total={total}"
```

実行結果:

```text
total=2
```

## まとめ

- `Vec<T>` は常に非 `Copy` です。更新は `Vec` を消費して新しい `Vec` を返すので、`let mut` へ再代入するか、`|>` でつなぎます。
- .NET の `List<T>` のメンバーには、同じ名前（snake_case）の関数があります。引数は .NET の順で、`Vec` は最後です（`Vec.insert index item values`）。.NET の例外はトラップです。`-1` と二分探索の補数は同じ値を返します。
- コールバックが要素を値で受ける関数は `Copy` を要求します。`find_indexr` や `sort_withr` のように `r` を付けた関数は、要素を借用するので文字列にも使えます。
- 容量は .NET と同じ規則（空なら 4、以降は 2 倍、足りなければ必要な数）で増えます。負の容量やサイズのオーバーフローはトラップします。
- 並べ替えは安定です。`sort` と `sort_range` は NaN を先頭に置きますが、比較関数を渡す形は渡した関数のとおりに並べます。`Eq`（`==`）で比べる `contains` や `index_of` は、NaN を見つけません。
- 要素の排他借用は `E1014` です。書き換えは `Vec.set` と `Vec.swap` です。
- `to_array` / `of_array` は所有バッファを移します。引数渡しのコピーとは別です。

## 関連項目

- [Array](./array.md) — 固定長の連続メモリ配列
- [List](./list.md) — 不変なリスト
- [Seq](./seq.md) — 遅延シーケンス
- [所有権とムーブ](../ownership-and-memory/ownership.md)
- [言語リファレンスの目次](../index.md)
