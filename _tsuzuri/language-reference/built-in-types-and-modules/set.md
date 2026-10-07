# Set

`Set<K>` は、重複のない要素を `Ord` で昇順に並べた集合です。中身は整列した連続バッファで、二分探索木ではありません。

重複を排除した一意な値の管理や、昇順での要素走査、および集合演算（和集合・積集合・差集合）を行いたい場面に最適です。高速な平均 $O(1)$ の包含判定が求められ順序が不要な場合は [HashSet](./hashset.md) を検討してください。

## この記事のポイント

- 型は `Set<K>`。要素型にかかわらず常に非 Copy 型です。
- 内部は不透明です。フィールド参照やパターン分解は `E1022` です。
- キー型には `Ord` 型クラスの実装が必要です。比較の内部で反射性（`k == k`）が検証され、NaN など比較不能な値は安全にトラップします。
- 要素の検索・包含判定は二分探索により **$O(\log n)$**、整列状態を維持した挿入・削除は **$O(n)$** の計算量です。
- 集合演算として `union`（和集合）、`intersect`（積集合）、`difference`（差集合）を提供します。
- `union` は両方の集合の所有権を消費し、`intersect` と `difference` は両辺を共有借用（`ref`）して新しい集合を構築します（要素型に `Copy` 制約が必要です）。
- 反復（`fold`, `iter`, `to_array`）は常に要素の昇順で実行されます。

## 基本の書き方と生成

### 生成と要素の挿入

`Set.empty()` で空の集合を生成するか、単一の要素を持つ `Set.singleton key` を使用します。

`Set.insert` は集合の所有権を消費し、要素を追加した新しい `Set` を返します。すでに同じ要素が存在する場合は集合を変更せずにそのまま返します。

```tsuzuri run=len%3D3%20elements%3D%5B1%2C%202%2C%203%5D
let mut set: Set<i64> = Set.empty()
set = Set.insert set 3
set = Set.insert set 1
set = Set.insert set 2
set = Set.insert set 1

let len = Set.length (ref set)
let arr = Set.to_array (ref set)
$"len={len} elements={arr}"
```

実行結果:

```text
len=3 elements=[1, 2, 3]
```

重複して挿入された `1` は無視され、要素は常に昇順（`[1, 2, 3]`）に整列されて保持されます。

## 包含判定と削除

- `Set.contains (ref set) key`: 要素が含まれているかを二分探索（$O(\log n)$）で判定します。
- `Set.remove set key`: 指定された要素を削除した新しい集合を返します（$O(n)$）。存在しない要素を指定した場合は何も変更せずそのまま返します。

```tsuzuri run=has_two%3Dtrue%20rem_has_two%3Dfalse
let mut numbers = Set.insert (Set.insert (Set.singleton 1) 2) 3
let has_two = Set.contains (ref numbers) 2
numbers = Set.remove numbers 2
let rem_has_two = Set.contains (ref numbers) 2
$"has_two={has_two} rem_has_two={rem_has_two}"
```

実行結果:

```text
has_two=true rem_has_two=false
```

検索キーは値渡しで渡され、判定完了時に安全に解放されます。

> [!WARNING]
> **NaN キーのトラップ**
> 比較の中で `assert (Eq.eq key key)` が反射性を見ます。NaN は空集合への判定でもトラップします。`singleton` は制約なしで 1 要素を持てますが、その後の比較で同じ検査が走ります。

## 集合演算

`Set` では数学的な集合演算がモジュール関数として提供されています。

```tsuzuri run=union%3D%5B1%2C%202%2C%203%2C%204%5D%20inter%3D%5B2%2C%203%5D%20diff%3D%5B1%5D
let left = Set.insert (Set.insert (Set.singleton 1) 2) 3
let right = Set.insert (Set.insert (Set.singleton 2) 3) 4

let common = Set.intersect (ref left) (ref right)
let different = Set.difference (ref left) (ref right)
let combined = Set.union left right

let union_arr = Set.to_array (ref combined)
let inter_arr = Set.to_array (ref common)
let diff_arr = Set.to_array (ref different)

$"union={union_arr} inter={inter_arr} diff={diff_arr}"
```

実行結果:

```text
union=[1, 2, 3, 4] inter=[2, 3] diff=[1]
```

### 所有権と制約の違い

- `Set.union left right`: 両方の集合の**所有権を消費**してマージします。重複要素がある場合は左辺側のキー代表値が維持されます。非 Copy な要素に対しても適用可能です。
- `Set.intersect (ref left) (ref right)` / `Set.difference (ref left) (ref right)`: 両辺を**共有借用**して新しい集合を構築するため、要素型に **`Copy` 制約**が要求されます。

## 反復と畳み込み

`Set` の反復処理は、内部の整列順序に従って常に**要素の昇順**で行われます。

- `Set.to_array (ref set)`: 全要素を昇順に格納した所有配列 `[K]` を返します（`Copy<K>` が必要）。
- `Set.fold folder initial (ref set)`: `folder: state -> ref K -> state` により非 Copy 要素も借用したまま昇順に畳み込みます。
- `Set.iter (ref set)`: 要素への共有参照 `ref K` を昇順に列挙する `Seq` を返します。`for x in set` は `E1005` なので、`for key in Set.iter (ref set)` と書きます。

```tsuzuri run=sum%3D60
let mut numbers: Set<i64> = Set.empty()
numbers = Set.insert numbers 20
numbers = Set.insert numbers 10
numbers = Set.insert numbers 30

let sum = Set.fold (\acc x -> acc + *x) 0 (ref numbers)
$"sum={sum}"
```

実行結果:

```text
sum=60
```

## API リファレンス

すべての関数は `std::Set` モジュールに属しています。

### 基本操作・情報取得

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `empty` | `fn() -> Set<'key>` | 空の集合を作ります。呼び出しは `Set.empty()` です |
| `singleton` | `'key -> Set<'key>` | 1 つの要素を持つ集合を生成します |
| `length` | `ref Set<'key> -> i64` | 集合内の要素数を返します |
| `is_empty` | `ref Set<'key> -> bool` | 集合が空かどうかを返します |
| `contains` | `Ord<'key> => ref Set<'key> -> 'key -> bool` | 要素が含まれているかを判定します（$O(\log n)$） |

### 更新・削除

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `insert` | `Ord<'key> => Set<'key> -> 'key -> Set<'key>` | 要素を追加した新しい集合を返します（$O(n)$） |
| `remove` | `Ord<'key> => Set<'key> -> 'key -> Set<'key>` | 指定要素を削除した新しい集合を返します（$O(n)$） |

### 集合演算

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `union` | `Ord<'key> => Set<'key> -> Set<'key> -> Set<'key>` | 2 つの集合の和集合を返します（両辺を消費） |
| `intersect` | `(Ord<'key>, Copy<'key>) => ref Set<'key> -> ref Set<'key> -> Set<'key>` | 2 つの集合の積集合を返します（両辺を借用） |
| `difference` | `(Ord<'key>, Copy<'key>) => ref Set<'key> -> ref Set<'key> -> Set<'key>` | 差集合（左辺にあり右辺にない要素）を返します（両辺を借用） |

### 変換・反復

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `to_array` | `Copy<'key> => ref Set<'key> -> ['key]` | 全要素を昇順に並べた配列を返します |
| `fold` | `('state -> ref 'key -> 'state) -> 'state -> ref Set<'key> -> 'state` | 要素を昇順に畳み込みます |
| `iter` | `ref Set<'key> -> Seq<ref 'key>` | 要素の共有参照を昇順に列挙する `Seq` を返します |

## 計算量

| 操作 | 計算量 | 備考 |
| --- | --- | --- |
| 包含判定 `Set.contains` | $O(\log n)$ | 二分探索による要素特定 |
| 挿入 `Set.insert` | $O(n)$ | 探索 $O(\log n)$ + 挿入位置への要素シフト |
| 削除 `Set.remove` | $O(n)$ | 探索 $O(\log n)$ + 削除スロットへの要素シフト |
| 和集合 `Set.union` | $O(n + m)$ | 整列済み配列同士のマージ |
| 積集合 / 差集合 `intersect` / `difference` | $O(n + m)$ | 整列済み配列同士の走査マージ |
| 長さの参照 `length` / `is_empty` | $O(1)$ | 内部バッファの長さ参照 |
| 反復・配列化 `fold` / `to_array` | $O(n)$ | 全要素を昇順に 1 回走査 |

## まとめ

- `Set` はキー昇順の連続バッファです。検索は $O(\log n)$、挿入と削除は $O(n)$ です。
- すでにある要素への `insert` は、その集合をそのまま返します。
- `union` は両辺を消費し、重複キーは左の代表値を残します。`intersect` と `difference` は両辺を借用し、キーに `Copy` が必要です。

## 関連項目

- [Map](./map.md) — 順序付き連想配列
- [HashSet](./hashset.md) — 平均 $O(1)$ のハッシュ集合
- [Seq](./seq.md) — 遅延シーケンス
- [型クラス](../types-and-type-inference/type-classes.md) — `Ord` と `Eq`
- [言語リファレンスの目次](../index.md)

