# Map

`Map<K, V>` は、キーの全順序（`Ord`）で昇順に並べた連想配列です。中身は整列した連続バッファで、二分探索木ではありません。だから検索は $O(\log n)$、挿入と削除は要素の移動で $O(n)$ です。

キー順に走査したいときや、`Hash` がなく `Ord` だけがあるときに使います。平均 $O(1)$ の検索が要るなら [HashMap](./hashmap.md) です。

## この記事のポイント

- 型は `Map<K, V>`。キーと値の型にかかわらず常に非 Copy 型です。
- 内部は不透明です。フィールド参照やパターン分解は `E1022` です。
- キー型には `Ord` 型クラスの実装が必要です。比較の内部で反射性（`k == k`）が検証され、NaN など比較不能なキーは安全にトラップします。
- 要素の検索は二分探索により **$O(\log n)$**、整列状態を維持した挿入・削除は **$O(n)$** の計算量です。
- 反復（`fold`, `iter`, `keys`, `values`, `to_array`）は常にキーの昇順で決定論的に実行されます。
- `at` は要素への共有参照 `ref V` を返し（存在しないとトラップ）、`get` は値型に `Copy` 制約を要求して `Maybe<V>` を返します。

## 基本の書き方と生成

### 生成と要素の挿入

`Map.empty()` で空のマップを生成するか、単一のキーと値からなる `Map.singleton key value` を使用します。

`Map.insert` はマップの所有権を消費し、指定されたキーと値が挿入（または更新）された新しい `Map` を返します。通常は `let mut` 変数に再代入してマップを構築します。

```tsuzuri run=len%3D3%20first_key%3D1
let mut map: Map<i64, string> = Map.empty()
map = Map.insert map 3 "three"
map = Map.insert map 1 "one"
map = Map.insert map 2 "two"

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
map = Map.insert map 1 "new_one"
let val = Map.at (ref map) 1
$"val={val}"
```

実行結果:

```text
val=new_one
```

## 検索と要素アクセス

`Map` から値を取得・検索するには以下の関数を使用します：

- `Map.contains_key (ref map) key`: キーが存在するかどうかを `bool` で返します。
- `Map.get (ref map) key`: キーに対応する値を `Maybe<V>` で返します。値型に **`Copy` 制約が必要**です。
- `Map.at (ref map) key`: キーに対応する値への共有借用参照 `ref V` を返します。キーが存在しない場合は安全にトラップします。

```tsuzuri run=has_two%3Dtrue%20at_two%3D2%20missing%3Dnone
let mut map: Map<string, i64> = Map.empty()
map = Map.insert map "one" 1
map = Map.insert map "two" 2

let has_two = Map.contains_key (ref map) "two"
let at_two = Map.at (ref map) "two"
let missing = match Map.get (ref map) "three" with
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

`Map.remove map key` は、キーが存在すればそのエントリを安全に解放し、後続要素をシフトして順序を維持した新しい `Map` を返します。キーが存在しない場合はマップを変更せずにそのまま返します。

```tsuzuri run=before%3D2%20after%3D1
let mut map = Map.insert (Map.singleton 1 "one") 2 "two"
let before_len = Map.length (ref map)
map = Map.remove map 1
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
map = Map.insert map 30 "c"
map = Map.insert map 10 "a"
map = Map.insert map 20 "b"

let keys = Map.keys (ref map)

let summary = Map.fold (\acc k v ->
    let item = $"{*k}:{*v}"
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
| `contains_key` | `Ord<'key> => ref Map<'key, 'value> -> 'key -> bool` | 指定キーが存在するか判定します（$O(\log n)$） |
| `get` | `(Ord<'key>, Copy<'value>) => ref Map<'key, 'value> -> 'key -> Maybe<'value>` | キーに対応する値を `Some` で返します。存在しない場合は `None` |
| `at` | `Ord<'key> => ref Map<'key, 'value> -> 'key -> ref 'value` | キーに対応する値の共有参照を返します。存在しない場合はトラップ |

### 更新・削除

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `insert` | `Ord<'key> => Map<'key, 'value> -> 'key -> 'value -> Map<'key, 'value>` | キーと値を挿入・更新した新しいマップを返します（$O(n)$） |
| `remove` | `Ord<'key> => Map<'key, 'value> -> 'key -> Map<'key, 'value>` | 指定キーのエントリを削除した新しいマップを返します（$O(n)$） |

### 変換・反復

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `keys` | `Copy<'key> => ref Map<'key, 'value> -> ['key]` | キーの昇順配列を返します |
| `values` | `Copy<'value> => ref Map<'key, 'value> -> ['value]` | キーの昇順に対応する値の配列を返します |
| `to_array` | `(Copy<'key>, Copy<'value>) => ref Map<'key, 'value> -> [('key * 'value)]` | `(キー * 値)` のタプル配列を返します |
| `fold` | `('state -> ref 'key -> ref 'value -> 'state) -> 'state -> ref Map<'key, 'value> -> 'state` | キー昇順に畳み込みます |
| `iter` | `ref Map<'key, 'value> -> Seq<(ref 'key * ref 'value)>` | キーと値の参照タプルを昇順に列挙する `Seq` を返します |

## 計算量

| 操作 | 計算量 | 備考 |
| --- | --- | --- |
| 検索 `contains_key` / `get` / `at` | $O(\log n)$ | 二分探索によるキー位置の特定 |
| 挿入 `Map.insert` | $O(n)$ | 探索 $O(\log n)$ + 挿入位置への要素シフト |
| 削除 `Map.remove` | $O(n)$ | 探索 $O(\log n)$ + 削除スロットへの要素シフト |
| 要素数の参照 `length` / `is_empty` | $O(1)$ | 内部バッファの長さ参照 |
| 反復・配列化 `fold` / `keys` / `to_array` | $O(n)$ | 全要素を昇順に 1 回走査 |

## 所有権と借用

- `Map` は内部ストレージ（`Vec`）を所有する非 Copy 値です。`insert` や `remove` に渡されたマップ変数はムーブされ、消費後は再利用できません（再利用しようとすると `E1012`）。
- `Map.at` で取得した値の共有参照（`ref V`）が生存している間は、マップ自身を移動したり `remove` で更新したりすることは借用チェッカーによって禁止されます（`E1014`）。
- 借用を関数の外へ返そうとすると `E1013` です。

## まとめ

- `Map` はキー昇順の連続バッファです。二分探索木ではないので、挿入と削除は $O(n)$ です。
- 更新はマップを消費します。同じキーへの `insert` は最初のキー代表値を残し、値だけを置き換えます。
- `at` は不在でトラップする `ref V`、`get` は `Copy` な値の `Maybe<V>` です。NaN キーはトラップします。

## 関連項目

- [Set](./set.md) — 順序付き集合
- [HashMap](./hashmap.md) — 平均 $O(1)$ のハッシュテーブル
- [Seq](./seq.md) — 遅延シーケンス
- [型クラス](../types-and-type-inference/type-classes.md) — `Ord` と `Eq`
- [言語リファレンスの目次](../index.md)

