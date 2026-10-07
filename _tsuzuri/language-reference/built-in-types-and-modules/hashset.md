# HashSet

`HashSet<K>` は、オープンアドレス法ハッシュテーブルに基づく高速な集合コンテナです。平均 **$O(1)$** の計算量で要素の包含判定・追加・削除を行えます。

要素の重複を排除したい場合や、高速な存在判定（`contains`）が主たる目的であり順序が不要な場合に最適です。要素の整列順序や集合演算（`union`, `intersect`, `difference`）が必要な場合は [Set](./set.md) を使用してください。

## この記事のポイント

- 型は `HashSet<K>`。要素型にかかわらず常に非 Copy 型です。
- 内部は不透明です。フィールド参照やパターン分解は `E1022` です。ファイル名 `HashSet.tz` は予約名で `E1011` です。
- キー型には `Hash` と `Eq` 型クラスの実装が必要です（`deriving (Eq, Hash)` で自動導出可能）。`Ord` は不要です。
- 反射性（`k == k`）が検証されるため、NaN など比較不能な要素は安全にトラップします。
- 走査順は挿入順ですが、`remove` は末尾の要素を削除位置へ移します（swap-remove）。
- 借用版 API（`contains_ref`, `remove_ref`）により、検索キーを手放さずに判定・削除できます。
- HashDoS 対策として `with_seed`、`with_capacity_and_seed`、および OS 乱数から安全にシードを取得する `randomized ()` が提供されています。

## 基本の書き方と生成

### 生成と要素の追加

`HashSet.empty()` で空の集合を作り、`HashSet.insert` で要素を足します。等しい要素がすでにあるときは、最初の代表値を残して新しいキーを解放します。集合の中身は変わりません。

```tsuzuri run=len%3D3%20has_apple%3Dtrue
let mut fruits: HashSet<string> = HashSet.empty()
fruits = HashSet.insert fruits "apple"
fruits = HashSet.insert fruits "banana"
fruits = HashSet.insert fruits "cherry"
fruits = HashSet.insert fruits "apple"

let len = HashSet.length (ref fruits)
let has_apple = HashSet.contains (ref fruits) "apple"
$"len={len} has_apple={has_apple}"
```

実行結果:

```text
len=3 has_apple=true
```

重複して追加された `"apple"` は安全に無視され、一意な要素のみが保持されます。

### 事前確保 `HashSet.with_capacity`

追加する要素数が事前に分かっている場合は、`HashSet.with_capacity count` で容量を事前確保しておくことで、途中のテーブル拡張を回避できます。

```tsuzuri run=len%3D0
let empty_set: HashSet<i64> = HashSet.with_capacity 100
let len = HashSet.length (ref empty_set)
$"len={len}"
```

実行結果:

```text
len=0
```

`count` が負、または $2^{60}$ を超えるとトラップします。負荷率と OS 乱数の使える環境は [HashMap](./hashmap.md) と同じです。

## 包含判定・削除と借用版 API

要素の判定や削除には、要素を値渡しする版と、共有借用参照で渡す `*_ref` 版が用意されています。

- `HashSet.contains (ref set) key`: 要素が含まれているかを判定（値渡し）
- `HashSet.contains_ref (ref set) (ref key)`: 要素が含まれているかを判定（借用）
- `HashSet.remove set key`: 要素を削除した集合を返却（値渡し、swap-remove）
- `HashSet.remove_ref set (ref key)`: 要素を削除した集合を返却（借用、swap-remove）

非 Copy なキー（文字列など）を判定する際、手元にキーの所有権を残したい場合は **`contains_ref`** や **`remove_ref`** を使用します。

```tsuzuri run=has%3Dtrue%20rem_has%3Dfalse%20key%3Dtarget
let mut set: HashSet<string> = HashSet.empty()
set = HashSet.insert set "target"

let key = "target"
let has = HashSet.contains_ref (ref set) (ref key)
set = HashSet.remove_ref set (ref key)
let rem_has = HashSet.contains_ref (ref set) (ref key)

$"has={has} rem_has={rem_has} key={key}"
```

実行結果:

```text
has=true rem_has=false key=target
```

> [!WARNING]
> **NaN 要素のトラップ**
> 比較処理の内部では `assert (Eq.eq key key)` により反射性が検査されます。浮動小数点数の NaN を要素として渡すと、空集合への判定であっても安全にトラップします。

## 反復順序と swap-remove

`HashSet` の走査順は、内部の `HashMap<'key, unit>` のエントリ順と同じです。`for x in set` は `E1005` です。走査は `for key in HashSet.iter (ref set)` と書きます。

- **新規追加**: 配列末尾に追加されます。
- **削除（`remove`）**: 計算量 $O(1)$ を達成するため、**末尾要素が削除位置へ移動（swap-remove）** します。

```tsuzuri run=elements%3D%5B%27a%27%2C%20%27d%27%2C%20%27c%27%5D
let mut chars: HashSet<char> = HashSet.empty()
chars = HashSet.insert chars 'a'
chars = HashSet.insert chars 'b'
chars = HashSet.insert chars 'c'
chars = HashSet.insert chars 'd'
chars = HashSet.remove chars 'b'

let arr = HashSet.to_array (ref chars)
$"elements={arr}"
```

実行結果:

```text
elements=['a', 'd', 'c']
```

`'b'` を消すと、末尾の `'d'` が `'b'` の位置へ移り、順序は `'a'`, `'d'`, `'c'` です。

## seed と HashDoS 対策

外部からの入力データを集合に格納する場合、悪意あるハッシュ衝突攻撃（HashDoS）を防ぐため、シード付きのコンストラクタが用意されています。

- `HashSet.with_seed seed: i64u`: 指定シード値を用いて SipHash-1-3 で暗号論的混合を行う集合を生成します。
- `HashSet.with_capacity_and_seed count seed`: 容量事前確保とシード指定を同時に行います。
- `HashSet.randomized ()`: OS の暗号論的乱数源から安全にシードを取得する `IO` アクションです。
- `HashSet.try_randomized ()`: 乱数取得失敗時に `Result<..., Os.Error>` を返す `IO` アクションです。
- `HashSet.longest_probe (ref set)`: 内部テーブルの最大探査距離を返します（内部診断用）。

```tsuzuri run=len%3D1
let mut set: HashSet<string> = HashSet.with_seed 100i64u
set = HashSet.insert set "item"
let len = HashSet.length (ref set)
$"len={len}"
```

実行結果:

```text
len=1
```

seed を付けても走査順は変わりません。同じ操作列なら、seed なしと同じ順序です。`randomized ()` は `unit` を取るので、`randomized()` ではなく `randomized ()` と書きます。既定の wasm32 では `E2000`、Windows 上のコンパイラが OS API に到達するネイティブビルドを作ると `E2002` です。

## API リファレンス

すべての関数は `std::HashSet` モジュールに属しています。

### 生成・情報取得

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `empty` | `fn() -> HashSet<'key>` | 空の集合を作ります。呼び出しは `HashSet.empty()` です |
| `with_capacity` | `i64 -> HashSet<'key>` | 指定容量を事前確保した集合を生成します |
| `with_seed` | `i64u -> HashSet<'key>` | シード値で鍵付けした集合を生成します |
| `with_capacity_and_seed` | `i64 -> i64u -> HashSet<'key>` | 容量事前確保とシード指定を同時に行います |
| `randomized` | `unit -> IO<HashSet<'key>>` | OS 乱数シードによる集合生成（IO アクション） |
| `try_randomized` | `unit -> IO<Result<HashSet<'key>, Os.Error>>` | 乱数取得成否を Result で返す生成 |
| `length` | `ref HashSet<'key> -> i64` | 格納されている要素数を返します |
| `is_empty` | `ref HashSet<'key> -> bool` | 集合が空かどうかを返します |
| `longest_probe` | `ref HashSet<'key> -> i64` | 内部テーブルの最大探査距離を返します（診断用） |

### 判定・要素アクセス

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `contains` | `(Hash<'key>, Eq<'key>) => ref HashSet<'key> -> 'key -> bool` | 要素が含まれているか判定（値渡し） |
| `contains_ref` | `(Hash<'key>, Eq<'key>) => ref HashSet<'key> -> ref 'key -> bool` | 要素が含まれているか判定（借用） |

### 更新・削除

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `insert` | `(Hash<'key>, Eq<'key>) => HashSet<'key> -> 'key -> HashSet<'key>` | 要素を追加した新しい集合を返します |
| `remove` | `(Hash<'key>, Eq<'key>) => HashSet<'key> -> 'key -> HashSet<'key>` | 要素を削除した新しい集合を返します（swap-remove） |
| `remove_ref` | `(Hash<'key>, Eq<'key>) => HashSet<'key> -> ref 'key -> HashSet<'key>` | 要素借用で削除した新しい集合を返します |

### 変換・反復

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `to_array` | `Copy<'key> => ref HashSet<'key> -> ['key]` | 内部順序の要素配列を返します |
| `fold` | `('state -> ref 'key -> 'state) -> 'state -> ref HashSet<'key> -> 'state` | 内部順序で畳み込みます |
| `iter` | `ref HashSet<'key> -> Seq<ref 'key>` | 要素の共有参照を順次列挙する `Seq` を返します |

## 計算量

| 操作 | 期待計算量（平均） | 最悪計算量（全キー衝突時） |
| --- | --- | --- |
| 包含判定 `HashSet.contains` | $O(1)$ | $O(n)$ |
| 要素追加 `HashSet.insert`（新規） | 償却 $O(1)$ | $O(n)$ |
| 要素削除 `HashSet.remove`（swap-remove） | $O(1)$ | $O(n)$ |
| 要素数の参照 `length` / `is_empty` | $O(1)$ | $O(1)$ |
| 全走査・配列化 `fold` / `to_array` | $O(n)$ | $O(n)$ |

※ ハッシュ値が一様に分散している場合の計算量です。キー型のハッシュ計算や等値比較の計算量は除きます。

## まとめ

- 包含判定・追加・削除は平均 $O(1)$ です。集合演算はなく、それが要るなら [Set](./set.md) です。
- 走査順は内部の `HashMap` と同じで、削除は swap-remove です。
- `empty` は `fn() -> HashSet<...>`、`randomized` は `unit -> IO<...>` です。呼び出しの括弧の書き方が違います。

## 関連項目

- [HashMap](./hashmap.md) — 平均 $O(1)$ のハッシュテーブル
- [Set](./set.md) — 順序付き集合
- [Seq](./seq.md) — 遅延シーケンス
- [型クラス](../types-and-type-inference/type-classes.md) — `Hash` と `Eq`
- [言語リファレンスの目次](../index.md)

