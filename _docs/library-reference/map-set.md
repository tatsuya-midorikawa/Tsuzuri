# Map と Set

[ドキュメントのトップ](../README.md)

Map はキーと値、Set は重複しないキーを保持する順序付きコレクションです。どちらも非 Copy の不透明な所有型で、キーの比較には Ord を使います。内部はキー昇順の Vec で、ハッシュ表ではありません。平均 O(1) の検索・更新が必要なら、Hash と Eq をキーに使う [HashMap と HashSet](hash-map.md) を選びます。

## Map の例

```tsuzuri run=6
let dictionary = Map.insert (Map.singleton 2 "two") 1 "one"
let first = Map.at (ref dictionary) 1
assert (first.length == 3)
Map.fold (ref dictionary) 0 (\total _key value -> total + value.length)
```

読み取りは Map を共有借用し、更新は所有する Map を消費します。上の値は非 Copy の string ですが、at と fold は借用するので複製を要求しません。

## Map API

| API の引数順 | 契約 |
| --- | --- |
| `empty()`, `singleton key value` | 空または一要素の Map |
| `length map`, `is_empty map` | map を借用して i64 / bool |
| `insert map key value` | 所有更新。同じキーなら値を置換 |
| `remove map key` | 所有更新。不在なら変更なし |
| `contains_key map key` | 借用 Map と所有検索キーから bool |
| `get map key` | 借用 Map から Copy 値の `Option<V>` |
| `at map key` | `ref V`。不在ならトラップ |
| `to_array map` | キー昇順の `(K * V)` 配列。両方の Copy が必要 |
| `keys map`, `values map` | 対象側だけ Copy を要求して配列へ |
| `fold map initial folder` | `State -> ref K -> ref V -> State` |
| `iter map` | `(ref K * ref V)` を返す Seq |

関数名には Map を付けます。検索キーは値引数で、呼び出し後に不要なら解放します。同じキーへ insert すると最初のキー代表値を保持し、新しいキーと旧値を解放して値だけを置換します。

## Set の例

```tsuzuri run=42
let left = Set.insert (Set.singleton 20) 22
let right = Set.singleton 22
let combined = Set.union left right
Set.fold (ref combined) 0 (\total key -> total + deref key)
```

union は両方の所有値を消費します。重複したキーは一つになり、左の代表値を保持します。

## Set API

| API の引数順 | 契約 |
| --- | --- |
| `empty()`, `singleton key` | 空または一要素の Set |
| `length set`, `is_empty set` | 共有借用して情報取得 |
| `insert set key`, `remove set key` | 所有更新 |
| `contains set key` | set を借用し、検索キーを消費 |
| `to_array set` | Copy キーを昇順に複製 |
| `fold set initial folder` | `State -> ref K -> State` |
| `iter set` | キーへの共有参照の Seq |
| `union left right` | 所有する両入力を消費して和集合 |
| `intersect left right` | 両入力を借用し、Copy キーで積集合を作る |
| `difference left right` | 両入力を借用し、Copy キーで左から右を除く |

## 順序と NaN

キーの検索・更新に Copy は必要ありませんが、Ord が一貫した順序を定義する必要があります。比較時には反射的な等値性も検査するため、NaN のようなキーはトラップします。

制約のない singleton が一要素を保持できても、その後の比較が正しく行えるという保証にはなりません。独自の Ord / Eq の契約も利用者が整合させます。

## 計算量

| 操作 | 計算量 |
| --- | --- |
| 検索 | O(log n) |
| 挿入・削除 | O(n) |
| 配列化、fold、全反復 | O(n) |
| 集合演算 | O(n + m) |

内部は整列した Vec を使い、更新は容量が足りれば領域を再利用します。union は出力領域へ移動し、intersect / difference は借用入力から複製します。列挙と snapshot はキー順です。

## 所有権と不透明性

内部フィールドの構築、参照、パターン分解、レコード更新によるアクセスは `E1022` です。API 生成ページに内部の型宣言が表示されても、利用者がそのフィールドを操作できるという意味ではありません。

共有参照を格納すれば元所有者の寿命を保持します。排他参照は格納できず、at の借用中は Map を move・置換できません。可変 iterator と直接の公開 ABI は未対応です。ハッシュ表は別のモジュールの [HashMap と HashSet](hash-map.md) として提供しています。

## API と関連項目

- [Map のソース宣言](api/Map.md)、[Set のソース宣言](api/Set.md)
- [HashMap と HashSet](hash-map.md)
- [Seq と借用反復](sequences.md)
- [比較クラス](../language-reference/generics-and-typeclasses.md)
