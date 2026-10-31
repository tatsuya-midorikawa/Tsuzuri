# Map

`Map<K, V>` は、キーの全順序（`Ord`）で昇順に並べた連想配列です。中身は 1〜512 エントリのチャンクに分けた整列配列で、二分探索木ではありません。検索は $O(\log n)$ 回の比較、挿入と削除は $O(\log n)$ 回の比較と 1 チャンク内の要素の移動（最大 511 エントリ）です。

キー順に走査したいときや、`Hash` がなく `Ord` だけがあるときに使います。平均 $O(1)$ の検索が要るなら [HashMap](./hashmap.md) です。

## この記事のポイント

- 型は `Map<K, V>`。キーと値の型にかかわらず常に非 Copy 型です。
- 内部は不透明です。フィールド参照やパターン分解は `E1022` です。
- キー型には `Ord` 型クラスの実装が必要です。比較の内部で反射性（`k == k`）が検証され、NaN など比較不能なキーは安全にトラップします。
- 要素の検索は二分探索により **$O(\log n)$**、整列状態を維持した挿入・削除は **$O(\log n)$ 回の比較と 1 チャンク内の移動**です（チャンクの分割・併合では、チャンクの個数に比例する移動も加わります）。
- 反復（`fold`, `iter`, `keys`, `values`, `to_array`）は常にキーの昇順で決定論的に実行されます。
- `at` は要素への共有参照 `ref V` を返し（存在しないとトラップ）、`get` は値型に `Copy` 制約を要求して `Maybe<V>` を返します。

## 基本の書き方と生成

### 生成と要素の挿入

`Map.empty()` で空のマップを生成するか、単一のキーと値からなる `Map.singleton key value` を使用します。

`Map.insert` はマップの所有権を消費し、指定されたキーと値が挿入（または更新）された新しい `Map` を返します。通常は `let mut` 変数に再代入してマップを構築します。

```tsuzuri run=len%3D3%20first_key%3D1
let mut map: Map<i64, string> = Map.empty()
map = Map.insert 3 "three" map
map = Map.insert 1 "one" map
map = Map.insert 2 "two" map

let len = Map.length (ref map)
let keys = Map.keys (ref map)
let first_key = keys[0]
$"len={len} first_key={first_key}"
```

実行結果:

```text
len=3 first_key=1
```

バラバラの順序で `insert` しても、内部で常にキーの昇順（`1`, `2`, `3`）にソートされて保持されます。

### 既存キーの更新

すでに存在するキーに対して `Map.insert` を行うと、キーの代表値と順序位置はそのまま維持され、値のみが新しい値に置き換わります（古い値は安全に解放されます）。

```tsuzuri run=val%3Dnew_one
let mut map = Map.singleton 1 "old_one"
map = Map.insert 1 "new_one" map
let val = Map.at 1 (ref map)
$"val={val}"
```

実行結果:

```text
val=new_one
```

### 引数の順序

標準ライブラリ全体の規則は、[標準ライブラリの引数の順序](./argument-order.md)にまとめてあります。

読み取る、または更新するマップは、必ず最後の引数です。`table |> Map.insert key value` のようにパイプで渡せ、`Map.insert 1 "one" >> Map.remove 2` のように、マップだけが足りない関数を合成できます。F# の `Map` の関数と同じ順序です。

| F# | Tsuzuri |
| --- | --- |
| `Map.add key value table` | `Map.insert key value map` |
| `Map.find key table` | `Map.at key (ref map)` |
| `Map.tryFind key table` | `Map.get key (ref map)` |
| `Map.containsKey key table` | `Map.contains_key key (ref map)` |
| `Map.remove key table` | `Map.remove key map` |

```tsuzuri run=len%3D1%20one%3Done%20has_two%3Dfalse
let step = Map.insert 1 "one" >> Map.remove 2
let table: Map<i64, string> = Map.empty() |> Map.insert 2 "two" |> step
let one = table |> Map.at 1
let has_two = table |> Map.contains_key 2
$"len={Map.length (ref table)} one={one} has_two={has_two}"
```

実行結果:

```text
len=1 one=one has_two=false
```

マップを借用する関数（`at`、`get`、`contains_key`）に `table |> Map.at 1` と渡しても、`table` は消費されません。`>>` で合成した関数は、前の関数が返したマップを、次の関数が借りて受け取ります。借りた一時のマップの中を指す参照を返す `at` につなぐと（`Map.insert 1 "one" >> Map.at 1`）、参照が一時のマップより長生きしてしまうので `E1013` です。値を取り出すには、`get` を使うか、マップを `let` に束縛します。

## 検索と要素アクセス

`Map` から値を取得・検索するには以下の関数を使用します：

- `Map.contains_key key (ref map)`: キーが存在するかどうかを `bool` で返します。
- `Map.get key (ref map)`: キーに対応する値を `Maybe<V>` で返します。値型に **`Copy` 制約が必要**です。
- `Map.at key (ref map)`: キーに対応する値への共有借用参照 `ref V` を返します。キーが存在しない場合は安全にトラップします。

```tsuzuri run=has_two%3Dtrue%20at_two%3D2%20missing%3Dnone
let mut map: Map<string, i64> = Map.empty()
map = Map.insert "one" 1 map
map = Map.insert "two" 2 map

let has_two = Map.contains_key "two" (ref map)
let at_two = Map.at "two" (ref map)
let missing = match Map.get "three" (ref map) with
| Some v -> to_string v
| None -> "none"

$"has_two={has_two} at_two={at_two} missing={missing}"
```

実行結果:

```text
has_two=true at_two=2 missing=none
```

検索キーは値渡しで渡され、比較完了時に安全に解放されます。

> [!WARNING]
> **NaN キーのトラップ**
> 比較の中で `assert (Eq.eq key key)` が反射性を見ます。NaN はこれを満たさないので、空のマップへの検索でもトラップします。`singleton` は制約なしで 1 要素を持てますが、その後の比較で同じ検査が走ります。`-0.0` と `0.0` は等しいキーです。

## 要素の削除

`Map.remove key map` は、キーが存在すればそのエントリを安全に解放し、同じチャンクの後続要素をシフトして順序を維持した新しい `Map` を返します。キーが存在しない場合はマップを変更せずにそのまま返します。

```tsuzuri run=before%3D2%20after%3D1
let mut map = Map.insert 2 "two" (Map.singleton 1 "one")
let before_len = Map.length (ref map)
map = Map.remove 1 map
let after_len = Map.length (ref map)
$"before={before_len} after={after_len}"
```

実行結果:

```text
before=2 after=1
```

## 反復と順序

`Map` の要素走査は、内部の配列が常にソートされているため、**キーの昇順**で決定論的に行われます。

- `Map.keys (ref map)`: キーの配列 `[K]` を昇順で返します（`Copy<K>` が必要）。
- `Map.values (ref map)`: 値の配列 `[V]` をキー昇順で返します（`Copy<V>` が必要）。
- `Map.to_array (ref map)`: `[(K * V)]` のタプル配列をキー昇順で返します（`Copy<K>` と `Copy<V>` が必要）。
- `Map.fold folder initial (ref map)`: `folder: state -> ref K -> ref V -> state` により非 Copy 要素も借用したままキー昇順で畳み込みます。
- `Map.iter (ref map)`: キーと値の参照タプル `(ref K * ref V)` を順次列挙する `Seq` を返します。`for x in map` は `E1005` なので、`for pair in Map.iter (ref map)` と書きます。

```tsuzuri run=keys%3D%5B10%2C%2020%2C%2030%5D%20summary%3D10%3Aa%2C%2020%3Ab%2C%2030%3Ac
let mut map: Map<i64, string> = Map.empty()
map = Map.insert 30 "c" map
map = Map.insert 10 "a" map
map = Map.insert 20 "b" map

let keys = Map.keys (ref map)

let summary = Map.fold (\acc k v ->
    let item = $"{deref k}:{deref v}"
    if acc == "" then item else $"{acc}, {item}"
) "" (ref map)

$"keys={keys} summary={summary}"
```

実行結果:

```text
keys=[10, 20, 30] summary=10:a, 20:b, 30:c
```

## API リファレンス

すべての関数は `std::Map` モジュールに属しています。

### 基本操作・情報取得

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `empty` | `fn() -> Map<'key, 'value>` | 空のマップを作ります。呼び出しは `Map.empty()` です |
| `singleton` | `'key -> 'value -> Map<'key, 'value>` | 1 組のキーと値を持つマップを生成します |
| `length` | `ref Map<'key, 'value> -> i64` | マップ内の要素数を返します |
| `is_empty` | `ref Map<'key, 'value> -> bool` | マップが空かどうかを返します |

### 検索・要素アクセス

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `contains_key` | `Ord<'key> => 'key -> ref Map<'key, 'value> -> bool` | 指定キーが存在するか判定します（$O(\log n)$） |
| `get` | `(Ord<'key>, Copy<'value>) => 'key -> ref Map<'key, 'value> -> Maybe<'value>` | キーに対応する値を `Some` で返します。存在しない場合は `None` |
| `at` | `Ord<'key> => 'key -> ref Map<'key, 'value> -> ref 'value` | キーに対応する値の共有参照を返します。存在しない場合はトラップ |

### 更新・削除

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `insert` | `Ord<'key> => 'key -> 'value -> Map<'key, 'value> -> Map<'key, 'value>` | キーと値を挿入・更新した新しいマップを返します（$O(\log n)$ 回の比較と 1 チャンクの移動） |
| `remove` | `Ord<'key> => 'key -> Map<'key, 'value> -> Map<'key, 'value>` | 指定キーのエントリを削除した新しいマップを返します（$O(\log n)$ 回の比較と 1 チャンクの移動） |

### 変換・反復

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `keys` | `Copy<'key> => ref Map<'key, 'value> -> ['key]` | キーの昇順配列を返します |
| `values` | `Copy<'value> => ref Map<'key, 'value> -> ['value]` | キーの昇順に対応する値の配列を返します |
| `to_array` | `(Copy<'key>, Copy<'value>) => ref Map<'key, 'value> -> [('key * 'value)]` | `(キー * 値)` のタプル配列を返します |
| `fold` | `('state -> ref 'key -> ref 'value -> 'state) -> 'state -> ref Map<'key, 'value> -> 'state` | キー昇順に畳み込みます |
| `iter` | `ref Map<'key, 'value> -> Seq<(ref 'key * ref 'value)>` | キーと値の参照タプルを昇順に列挙する `Seq` を返します |

### 診断用

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `chunk_count` | `ref Map<'key, 'value> -> i64` | エントリを持つチャンクの個数を返します（空のマップは 0）。個数は実装の詳細で、将来の版で変わりえます。テストで使います |
| `invariants_hold` | `Ord<'key> => ref Map<'key, 'value> -> bool` | チャンクの規則（各チャンク 1〜512 エントリ、隣り合う 2 つの合計が 256 を超える、キーが厳密に昇順、`length` が合計と一致する）を満たすかを $O(n)$ で調べます。テストで使います |

## 計算量

計算量は 1 回の呼び出しの最悪の時間で、コールバックの呼び出し・`Eq.eq`・`Ord.lt`・`Hash.hash`・要素 1 つの `Copy` の複製をそれぞれ $O(1)$ と数えます（入れ子のコレクションを持つ要素の複製は、その大きさの分かかります）。`n` は主な入力の要素数、`m` はもう 1 つの入力の要素数、`k` は結果の要素数です（結果が入れ子のコレクションなら入れ子の要素もすべて数え、`with_capacity` などの容量を指定する関数では確保する容量、`longest_probe` では表の大きさです）。`Seq` を返す関数は `Seq` を返すまでの時間で、取り出しは 1 要素ごとに $O(1)$ です。規則は [構造化タグ](../organizing-tsuzuri/documentation-comment.md#構造化タグ) にあります。表は `std/` の `@complexity` から `node scripts/gen-docs.mjs` が作ります。

<!-- generated:complexity:Map -->
| 関数 | 計算量 |
| --- | --- |
| `Map.empty` | $O(1)$ |
| `Map.singleton` | $O(1)$ |
| `Map.length` | $O(1)$ |
| `Map.is_empty` | $O(1)$ |
| `Map.chunk_count` | $O(1)$ |
| `Map.contains_key` | $O(\log n)$ |
| `Map.get` | $O(\log n)$ |
| `Map.at` | $O(\log n)$ |
| `Map.insert` | $O(n)$ |
| `Map.remove` | $O(n)$ |
| `Map.to_array` | $O(n)$ |
| `Map.keys` | $O(n)$ |
| `Map.values` | $O(n)$ |
| `Map.fold` | $O(n)$ |
| `Map.iter` | $O(1)$ |
| `Map.invariants_hold` | $O(n)$ |
<!-- /generated:complexity:Map -->

検索は、チャンクの最後のキーへの二分探索と、チャンク内の二分探索です（確保なし）。`insert` と `remove` の要素の移動は 1 つのチャンクの中（最大 511 エントリ）で済みますが、満ちたチャンクの分割、隣り合うチャンクの併合、空になったチャンクの削除は、後ろのチャンクをずらすので最悪 $O(n)$ です（チャンクの個数 $c \le 2n/257 + 1$）。既存のキーの値の置換は $O(\log n)$ で、最大のキーより大きいキーの追加は最後のチャンクへの追加です。

## 所有権と借用

- `Map` は内部ストレージ（`Vec`）を所有する非 Copy 値です。`insert` や `remove` に渡されたマップ変数はムーブされ、消費後は再利用できません（再利用しようとすると `E1012`）。
- `Map.at` で取得した値の共有参照（`ref V`）が生存している間は、マップ自身を移動したり `remove` で更新したりすることは借用チェッカーによって禁止されます（`E1014`）。
- 借用を関数の外へ返そうとすると `E1013` です。

## まとめ

- `Map` はキー昇順に並べた 1〜512 エントリのチャンクの列です。二分探索木ではありませんが、検索は $O(\log n)$ 回の比較、挿入と削除は $O(\log n)$ 回の比較と 1 チャンク内の移動です。
- 更新はマップを消費します。同じキーへの `insert` は最初のキー代表値を残し、値だけを置き換えます。
- `at` は不在でトラップする `ref V`、`get` は `Copy` な値の `Maybe<V>` です。NaN キーはトラップします。

## 関連項目

- [Set](./set.md) — 順序付き集合
- [HashMap](./hashmap.md) — 平均 $O(1)$ のハッシュテーブル
- [Seq](./seq.md) — 遅延シーケンス
- [型クラス](../types-and-type-inference/type-classes.md) — `Ord` と `Eq`
- [言語リファレンスの目次](../index.md)
