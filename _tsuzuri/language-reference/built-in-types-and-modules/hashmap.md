# HashMap

`HashMap<K, V>` は、オープンアドレス法に基づく高速なハッシュテーブル連想配列です。平均 **$O(1)$** の計算量で要素の検索・挿入・削除を行えます。

必要な権限: `random`（`HashMap.randomized`・`HashMap.try_randomized` だけ）。権限の宣言と検査は [権限](../organizing-tsuzuri/packages.md#権限capabilities) を見てください。

キーの順序が不要で、大量のデータをキーで頻繁に検索・更新したい場面に最適です。キーの全順序（`Ord`）による整列走査が必要な場合は [Map](./map.md) を使用してください。

## この記事のポイント

- 型は `HashMap<K, V>`。キーと値の型にかかわらず常に非 Copy 型です。
- 内部は不透明です。フィールド参照やパターン分解は `E1022` です。公開 C ABI への export は `E1008`、ファイル名 `HashMap.tz` は予約名で `E1011` です。
- キー型には `Hash` と `Eq` 型クラスの実装が必要です（`deriving (Eq, Hash)` で自動導出可能）。`Ord` は不要です。
- 反射性（`k == k`）が検証されるため、NaN など比較不能なキーは安全にトラップします。
- 走査順は挿入順ですが、`remove` は末尾の要素を削除位置へ移します（swap-remove）。削除すると、そのあとの順序は挿入順ではなくなります。
- 借用版 API（`contains_key_ref`, `get_ref`, `at_ref`, `remove_ref`）により、検索キーを消費せずに借用したまま操作できます。
- HashDoS 攻撃対策として、SipHash-1-3 で暗号論的にハッシュを混合する `with_seed` や、OS 乱数からシードを取得する `randomized ()` が提供されています。

## 基本の書き方と生成

### 生成と要素の挿入

`HashMap.empty()` で空のマップを生成するか、`HashMap.with_capacity count` で容量を事前確保します。

`HashMap.insert` はマップの所有権を消費し、指定されたキーと値が挿入（または更新）された新しい `HashMap` を返します。

```tsuzuri run=len%3D3%20score%3D95
let mut scores: HashMap<string, i64> = HashMap.empty()
scores = HashMap.insert "Alice" 90 scores
scores = HashMap.insert "Bob" 85 scores
scores = HashMap.insert "Charlie" 95 scores

let len = HashMap.length (ref scores)
let score = HashMap.at "Charlie" (ref scores)
$"len={len} score={score}"
```

実行結果:

```text
len=3 score=95
```

### 既存キーの値の置換

すでに存在するキーに対して `HashMap.insert` を行うと、最初のキー代表値と順序位置がそのまま維持され、値のみがインプレースで置換されます（テーブルサイズは拡張されません）。

```tsuzuri run=val%3D100
let mut scores: HashMap<string, i64> = HashMap.empty()
scores = HashMap.insert "Alice" 90 scores
scores = HashMap.insert "Alice" 100 scores
let val = HashMap.at "Alice" (ref scores)
$"val={val}"
```

実行結果:

```text
val=100
```

### 引数の順序

標準ライブラリ全体の規則は、[標準ライブラリの引数の順序](./argument-order.md)にまとめてあります。

読み取る、または更新するマップは、必ず最後の引数です。`table |> HashMap.insert key value` のようにパイプで渡せ、`HashMap.insert 1 10 >> HashMap.remove 2` のように、マップだけが足りない関数を合成できます。キーと値は、関数名が示す順でマップの前に並べます（`HashMap.insert key value table`、`HashMap.remove key table`、`HashMap.get key (ref table)`）。借用版も同じ形です（`HashMap.get_ref (ref key) (ref table)`）。

```tsuzuri run=len%3D1%20ten%3D10%20has_two%3Dfalse
let step = HashMap.insert 1 10 >> HashMap.remove 2
let table: HashMap<i64, i64> = HashMap.empty() |> HashMap.insert 2 20 |> step
let ten = table |> HashMap.at 1
let has_two = table |> HashMap.contains_key 2
$"len={HashMap.length (ref table)} ten={ten} has_two={has_two}"
```

実行結果:

```text
len=1 ten=10 has_two=false
```

マップを借用する関数（`at`、`get`、`contains_key`）に `table |> HashMap.at 1` と渡しても、`table` は消費されません。`>>` で合成した関数は、前の関数が返したマップを、次の関数が借りて受け取ります。借りた一時のマップの中を指す参照を返す `at` につなぐと（`HashMap.insert 1 10 >> HashMap.at 1`）、参照が一時のマップより長生きしてしまうので `E1013` です。値を取り出すには、`get` を使うか、マップを `let` に束縛します。

## 反復順序と swap-remove の仕組み

`HashMap` の反復（`fold`、`iter`、`keys`、`values`、`to_array`）は、内部のエントリ配列のインデックス順です。ハッシュ値、seed、native と WASM、最適化レベルには依存しません。`for x in map` は `E1005` です。走査は `for pair in HashMap.iter (ref map)` と書きます。

- **新規キーの挿入**: 常に入力配列の末尾に追加されます。
- **既存キーの更新**: 順序位置は変化しません。
- **要素の削除（`remove`）**: $O(1)$ で削除を実行するため、**配列末尾のエントリを削除されたスロット位置へ移動（swap-remove）** します。

```tsuzuri run=keys%3D%5B10%2C%2040%2C%2030%5D
let mut ages: HashMap<i64, i64> = HashMap.empty()
ages = HashMap.insert 10 1 ages
ages = HashMap.insert 20 2 ages
ages = HashMap.insert 30 3 ages
ages = HashMap.insert 40 4 ages
ages = HashMap.insert 10 5 ages
ages = HashMap.remove 20 ages

let keys = HashMap.keys (ref ages)
$"keys={keys}"
```

実行結果:

```text
keys=[10, 40, 30]
```

`[10, 20, 30, 40]` から `20` を消すと、末尾の `40` が `20` の位置へ移り、順序は `[10, 40, 30]` です。

## 検索と借用版 API

キーによる検索・アクセスには、キーを値渡しする版と、共有借用参照で渡す `*_ref` 版が用意されています。

### 値渡し版と借用版

| 値渡し版 | 借用版 | 動作 |
| --- | --- | --- |
| `contains_key key (ref m)` | `contains_key_ref (ref key) (ref m)` | キーの存在判定 |
| `get key (ref m)` | `get_ref (ref key) (ref m)` | `Maybe<V>` を返す（`Copy<V>` が必要） |
| `at key (ref m)` | `at_ref (ref key) (ref m)` | 共有参照 `ref V` を返す（存在しないとトラップ） |
| `remove key m` | `remove_ref (ref key) m` | エントリの削除 |

所有文字列など非 Copy なキーを検索する際、値渡し版を使うとキーの所有権が消費されてしまいます。検索後もキーを手元に残したい場合は **`*_ref` 版** を使用します。

```tsuzuri run=has%3Dtrue%20val%3D100%20key%3Dscore
let mut map: HashMap<string, i64> = HashMap.empty()
map = HashMap.insert "score" 100 map

let key = "score"
let has = HashMap.contains_key_ref (ref key) (ref map)
let val = HashMap.at_ref (ref key) (ref map)

$"has={has} val={val} key={key}"
```

実行結果:

```text
has=true val=100 key=score
```

> [!WARNING]
> **NaN キーのトラップ**
> 検索や挿入の前に `assert (Eq.eq key key)` が反射性を見ます。NaN は空のマップへの検索でもトラップします。`-0.0` と `0.0` は同じキーで、最初に入れた側の符号が代表値として残ります。

## seed と HashDoS 対策

デフォルトの `HashMap.empty()` は固定のハッシュ混合関数（fmix64）を使用します。悪意ある第三者が意図的に同一ハッシュスロットへ衝突するキー群を送り込むと、探査距離が長くなり最悪計算量 $O(n)$ へと性能劣化する HashDoS 攻撃のリスクがあります。

Tsuzuri では暗号論的ハッシュ関数を用いた HashDoS 対策機能を提供しています。

### シード付きマップの生成

- `HashMap.with_seed seed: i64u`: 指定した 64 bit シード値から 128 bit の秘密鍵を生成し、SipHash-1-3 でビット混合を行うマップを生成します。
- `HashMap.with_capacity_and_seed count seed`: 指定容量を事前確保したシード付きマップを生成します。
- `HashMap.randomized ()`: OS カーネルの暗号論的乱数源（`Random.next_u64`）から動的シードを取得してマップを構築する `IO` アクションです。
- `HashMap.try_randomized ()`: OS 乱数取得の成否を `Result<..., Os.Error>` で返す `IO` アクションです。
- `HashMap.longest_probe (ref map)`: 理想のハッシュ位置から最も離れたエントリまでの最大探査距離を返します（内部診断用）。

```tsuzuri run=len%3D1
let mut map: HashMap<string, i64> = HashMap.with_seed 42i64u
map = HashMap.insert "safe" 1 map
let len = HashMap.length (ref map)
$"len={len}"
```

実行結果:

```text
len=1
```

seed を付けても走査順は変わりません。同じ操作列なら、seed なしと同じ順序です。

`with_capacity count` の `count` は 0 以上 $2^{60}$ 以下です。範囲外はトラップし、0 は `empty()` と同じです。テーブルは 8 以上の 2 の冪で、負荷率は 1/2 以下（要素数の 2 倍がテーブルサイズ以下）です。空のマップはテーブルを確保しません。既存キーの値の置換では拡張せず、自動では縮小しません。

`randomized ()` と `try_randomized ()` は OS の乱数（`Random.next_u64`）から seed を取る `IO` アクションです。呼び出しは `randomized ()` で、`randomized()` は引数 0 個の適用になり `E1006` です。既定の wasm32 では `E2000` です。Windows 上のコンパイラが、OS API に到達するネイティブビルドを作ると `E2002` です。seed を得られないとき、`randomized` はトラップし、`try_randomized` は `Result.Error`（`Os.Error`）を返します。固定の seed には置き換えません。wasm32 では、ホストが選んだ seed を `with_seed` に渡します。

> [!NOTE]
> `with_seed` の SipHash-1-3 は、スロットの偏りを和らげます。`Hash.hash`（既定は FNV-1a）の 64 bit 全体が衝突するキーは、seed があっても衝突します。信頼できない入力をキーにするなら、順序付きの [Map](./map.md) も候補です。`Map` は二分探索木ではなく、キー昇順に並べたチャンクの列です。

## API リファレンス

すべての関数は `std::HashMap` モジュールに属しています。

### 生成・情報取得

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `empty` | `fn() -> HashMap<'key, 'value>` | 空のマップを作ります。呼び出しは `HashMap.empty()` です |
| `with_capacity` | `i64 -> HashMap<'key, 'value>` | `count` 件まで再確保なしで挿入できる空マップ。0 以上 $2^{60}$ 以下。範囲外はトラップ |
| `with_seed` | `i64u -> HashMap<'key, 'value>` | シード値で鍵付けしたマップを生成します |
| `with_capacity_and_seed` | `i64 -> i64u -> HashMap<'key, 'value>` | 容量事前確保とシード指定を同時に行います |
| `randomized` | `unit -> IO<HashMap<'key, 'value>>` | OS 乱数シードによるマップ生成（IO アクション） |
| `try_randomized` | `unit -> IO<Result<HashMap<'key, 'value>, Os.Error>>` | 乱数取得失敗を Result で返す生成 |
| `length` | `ref HashMap<'key, 'value> -> i64` | 格納されているエントリ数を返します |
| `is_empty` | `ref HashMap<'key, 'value> -> bool` | マップが空かどうかを返します |
| `longest_probe` | `ref HashMap<'key, 'value> -> i64` | 内部テーブルの最大探査距離を返します（診断用） |
| `sip13` | `i64u -> i64u -> i64u -> i64u` | 64 bit ワードに対する SipHash-1-3 計算関数 |

### 検索・要素アクセス

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `contains_key` | `(Hash<'key>, Eq<'key>) => 'key -> ref HashMap<'key, 'value> -> bool` | キーの存在判定（値渡し） |
| `contains_key_ref` | `(Hash<'key>, Eq<'key>) => ref 'key -> ref HashMap<'key, 'value> -> bool` | キーの存在判定（借用） |
| `get` | `(Hash<'key>, Eq<'key>, Copy<'value>) => 'key -> ref HashMap<'key, 'value> -> Maybe<'value>` | 値を `Some` で取得（値渡し） |
| `get_ref` | `(Hash<'key>, Eq<'key>, Copy<'value>) => ref 'key -> ref HashMap<'key, 'value> -> Maybe<'value>` | 値を `Some` で取得（借用） |
| `at` | `(Hash<'key>, Eq<'key>) => 'key -> ref HashMap<'key, 'value> -> ref 'value` | 値への参照を取得（存在しないとトラップ） |
| `at_ref` | `{r s} :: (Hash<'key>, Eq<'key>) => ref {s} 'key -> ref {r} HashMap<'key, 'value> -> ref {r} 'value` | キー借用で値への参照を取得 |

### 更新・削除

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `insert` | `(Hash<'key>, Eq<'key>) => 'key -> 'value -> HashMap<'key, 'value> -> HashMap<'key, 'value>` | キーと値を挿入・更新したマップを返します |
| `remove` | `(Hash<'key>, Eq<'key>) => 'key -> HashMap<'key, 'value> -> HashMap<'key, 'value>` | キーを削除したマップを返します（swap-remove） |
| `remove_ref` | `(Hash<'key>, Eq<'key>) => ref 'key -> HashMap<'key, 'value> -> HashMap<'key, 'value>` | キー借用で削除したマップを返します |

### 変換・反復

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `keys` | `Copy<'key> => ref HashMap<'key, 'value> -> ['key]` | エントリ順序のキー配列を返します |
| `values` | `Copy<'value> => ref HashMap<'key, 'value> -> ['value]` | エントリ順序の値配列を返します |
| `to_array` | `(Copy<'key>, Copy<'value>) => ref HashMap<'key, 'value> -> [('key * 'value)]` | エントリ順序のタプル配列を返します |
| `fold` | `('state -> ref 'key -> ref 'value -> 'state) -> 'state -> ref HashMap<'key, 'value> -> 'state` | エントリ順序で畳み込みを行います |
| `iter` | `ref HashMap<'key, 'value> -> Seq<(ref 'key * ref 'value)>` | エントリ順序で参照タプルを列挙する `Seq` を返します |

## 計算量

計算量は 1 回の呼び出しの最悪の時間で、コールバックの呼び出し・`Eq.eq`・`Ord.lt`・`Hash.hash`・要素 1 つの `Copy` の複製をそれぞれ $O(1)$ と数えます（入れ子のコレクションを持つ要素の複製は、その大きさの分かかります）。`n` は主な入力の要素数、`m` はもう 1 つの入力の要素数、`k` は結果の要素数です（結果が入れ子のコレクションなら入れ子の要素もすべて数え、`with_capacity` などの容量を指定する関数では確保する容量、`longest_probe` では表の大きさです）。`Seq` を返す関数は `Seq` を返すまでの時間で、取り出しは 1 要素ごとに $O(1)$ です。規則は [構造化タグ](../organizing-tsuzuri/documentation-comment.md#構造化タグ) にあります。表は `std/` の `@complexity` から `node scripts/gen-docs.mjs` が作ります。平均・償却の値は、キーのハッシュが一様に分散するときのものです。`Hash.hash` や `Eq.eq` のコスト（例えば長い文字列キーの比較）は含みません。

<!-- generated:complexity:HashMap -->
| 関数 | 計算量 | 関連 |
| --- | --- | --- |
| `HashMap.empty` | $O(1)$ | - |
| `HashMap.length` | $O(1)$ | - |
| `HashMap.is_empty` | $O(1)$ | - |
| `HashMap.sip13` | $O(1)$ | - |
| `HashMap.with_capacity` | $O(k)$ | - |
| `HashMap.with_seed` | $O(1)$ | - |
| `HashMap.with_capacity_and_seed` | $O(k)$ | - |
| `HashMap.try_randomized` | $O(1)$ | - |
| `HashMap.randomized` | $O(1)$ | - |
| `HashMap.contains_key_ref` | 平均 $O(1)$、最悪 $O(n)$ | `HashMap.contains_key` |
| `HashMap.contains_key` | 平均 $O(1)$、最悪 $O(n)$ | `HashMap.contains_key_ref` |
| `HashMap.get_ref` | 平均 $O(1)$、最悪 $O(n)$ | `HashMap.get` |
| `HashMap.get` | 平均 $O(1)$、最悪 $O(n)$ | `HashMap.get_ref` |
| `HashMap.at_ref` | 平均 $O(1)$、最悪 $O(n)$ | `HashMap.at` |
| `HashMap.at` | 平均 $O(1)$、最悪 $O(n)$ | `HashMap.at_ref` |
| `HashMap.insert` | 償却 $O(1)$、最悪 $O(n \cdot n)$ | - |
| `HashMap.remove_ref` | 平均 $O(1)$、最悪 $O(n)$ | `HashMap.remove` |
| `HashMap.remove` | 平均 $O(1)$、最悪 $O(n)$ | `HashMap.remove_ref` |
| `HashMap.longest_probe` | $O(k)$ | - |
| `HashMap.to_array` | $O(n)$ | - |
| `HashMap.keys` | $O(n)$ | - |
| `HashMap.values` | $O(n)$ | - |
| `HashMap.fold` | $O(n)$ | - |
| `HashMap.iter` | $O(1)$ | - |
<!-- /generated:complexity:HashMap -->

`insert` が表を広げるときは、すべてのエントリを入れ直します。すべてのキーのハッシュが衝突すると、その 1 回の呼び出しは $O(n^2)$ になります（`worst` の値）。

消費したマップの再利用は `E1012`、`at` の参照が生きている間の更新や移動は `E1014`、その参照を関数の外へ返すと `E1013` です。`Hash` / `Eq` がないキーや、`get` / `keys` / `values` / `to_array` で `Copy` を満たさない型は `E1005` です。

## まとめ

- 検索・挿入・削除は平均 $O(1)$、全キーが衝突すると $O(n)$ です。
- 走査順は挿入と削除の履歴だけで決まります。`remove` は swap-remove で、末尾の要素が空いた位置へ移ります。
- 既定のマップは HashDoS に耐性がありません。信頼できないキーには `with_seed` か `randomized ()`、または [Map](./map.md) を使います。
- `empty` は `fn() -> HashMap<...>` で、呼び出しは `empty()` です。`randomized` は `unit ->` なので `randomized ()` と書きます。

## 関連項目

- [HashSet](./hashset.md) — 平均 $O(1)$ のハッシュ集合
- [Map](./map.md) — 順序付き連想配列
- [Seq](./seq.md) — 遅延シーケンス
- [型クラス](../types-and-type-inference/type-classes.md) — `Hash` と `Eq`
- [言語リファレンスの目次](../index.md)
