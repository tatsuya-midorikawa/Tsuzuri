# HashMap と HashSet

[ドキュメントのトップ](../README.md)

HashMap はキーと値、HashSet は重複しないキーを保持するハッシュ表のコレクションです。どちらも非 Copy の不透明な所有型で、キーには `Hash` と `Eq` が必要です。検索・挿入・削除は平均 O(1) で、キーの順序が必要なら [Map と Set](map-set.md) を使います。

反復順序はハッシュ値に依存せず、挿入と削除の列だけで決まります。新しいキーは末尾に付き、削除は末尾の entry（キーと値の組）を空いた位置へ移します。そのため fold、iter、keys、values、to_array の出力は決定的で、native と WASM、最適化段階、seed の有無にかかわらず同じです。

## HashMap の例

```tsuzuri run=154433
let mut map: HashMap<i64, i64> = HashMap.empty()
map = HashMap.insert map 10 1
map = HashMap.insert map 20 2
map = HashMap.insert map 30 3
map = HashMap.insert map 40 4
map = HashMap.insert map 10 5
map = HashMap.remove map 20
HashMap.fold (\total key value -> total * 100 + deref key + deref value) 0 (ref map)
```

キーは `10 20 30 40` の順に入ります。`10` への二度目の insert は値だけを置き換え、位置を変えません。`20` を remove すると末尾の `40` がその位置へ移り、走査順は `10`（値 5）、`40`（値 4）、`30`（値 3）になります。fold は `total * 100 + key + value` をこの順に積むので、結果は `((0 * 100 + 15) * 100 + 44) * 100 + 33 = 154433` です。

insert と remove は所有する map を消費し、更新後の map を返します。上の例のように同じ変数へ代入し直して使います。消費した map をもう一度使うと `E1012` です。

## HashSet の例

```tsuzuri run=241
let mut seen: HashSet<string> = HashSet.empty()
seen = HashSet.insert seen "apple"
seen = HashSet.insert seen "pear"
seen = HashSet.insert seen "apple"
let needle = "pear"
let found = HashSet.contains_ref (ref seen) (ref needle)
HashSet.length (ref seen) * 100 + needle.length * 10 + (if found then 1 else 0)
```

`"apple"` を二度 insert しても要素は一つです。`contains_ref` は検索キーを借用するので、`needle` は呼び出しの後も使えます。値で受け取る `contains` に非 Copy のキーを渡すと、そのキーは呼び出しで消費されます。結果は `2 * 100 + 4 * 10 + 1 = 241` です。

## HashMap API

| API の引数順 | 契約 |
| --- | --- |
| `empty()`, `with_capacity count` | 空の map、または `count` 件を再確保なしで入れられる map |
| `with_seed seed`, `with_capacity_and_seed count seed` | `seed`（`i64u`）で鍵付けした空の map。[HashDoS と seed](#hashdos-と-seed) |
| `try_randomized ()`, `randomized ()` | OS の乱数を seed にする `IO` アクション。前者は失敗を `Result` で返し、後者は失敗でトラップ |
| `length map`, `is_empty map` | map を借用して i64 / bool |
| `insert map key value` | 所有更新。等しいキーがあれば、格納済みのキーと位置を保ったまま値だけを置換 |
| `remove map key`, `remove_ref map key` | 所有更新。不在なら変更なし。末尾の entry が削除位置へ移る |
| `contains_key map key`, `contains_key_ref map key` | 借用 map とキーから bool |
| `get map key`, `get_ref map key` | 借用 map から Copy 値の `Option<V>` |
| `at map key`, `at_ref map key` | `ref V`。不在ならトラップ |
| `to_array map` | 走査順の `(K * V)` 配列。両方の Copy が必要 |
| `keys map`, `values map` | 対象側だけ Copy を要求して走査順の配列へ |
| `fold folder initial map` | `State -> ref K -> ref V -> State` |
| `iter map` | `(ref K * ref V)` を返す Seq |
| `longest_probe map` | 診断。理想の位置から最も遠い entry の距離。空の map は 0 |
| `sip13 key0 key1 word` | 純粋関数。64-bit の語 1 個の SipHash-1-3 |

キーを取る操作は `Hash<K>` と `Eq<K>` を要求します。`get` と `get_ref` は値の `Copy`、`to_array` はキーと値の `Copy`、`keys` と `values` は対象側だけの `Copy` を要求します。`Copy` がない値は `at`、fold、iter で借用して読みます。fold は Map と同じく callback と初期値を先、map を最後に受け取ります。

### 借用したキーで探す

`_ref` なしの `remove`、`contains_key`、`get`、`at` は検索キーを値で受け取り、呼び出しの終わりに解放します。`_ref` 付きは `ref K` を受け取るので、string のような非 Copy のキーを手放さずに探せます。`at_ref` の結果は map だけを借用し、検索キーの借用は呼び出しで終わります。

```tsuzuri run=30
let mut ages: HashMap<string, i64> = HashMap.empty()
ages = HashMap.insert ages "ann" 31
ages = HashMap.insert ages "bob" 27
let name = "bob"
match HashMap.get_ref (ref ages) (ref name) with
| Option.Some age -> age + name.length
| Option.None -> 0
```

`name` を借用して探したので、呼び出しの後も `name.length` を使えます。結果は `27 + 3 = 30` です。

## HashSet API

| API の引数順 | 契約 |
| --- | --- |
| `empty()`, `with_capacity count` | 空の set、または `count` 件を再確保なしで入れられる set |
| `with_seed seed`, `with_capacity_and_seed count seed` | `seed`（`i64u`）で鍵付けした空の set |
| `try_randomized ()`, `randomized ()` | OS の乱数を seed にする `IO` アクション。HashMap と同じ |
| `length set`, `is_empty set` | set を借用して i64 / bool |
| `insert set key` | 所有更新。等しいキーがあれば格納済みのキーを保ち、渡したキーを解放 |
| `remove set key`, `remove_ref set key` | 所有更新。不在なら変更なし。末尾の entry が削除位置へ移る |
| `contains set key`, `contains_ref set key` | set を借用し、キーが入っているかを bool で返す |
| `to_array set` | 走査順に Copy キーを複製した配列 |
| `fold folder initial set` | `State -> ref K -> State` |
| `iter set` | キーへの共有参照の Seq |
| `longest_probe set` | 診断。HashMap と同じ |

HashSet に `union`、`intersect`、`difference`、`singleton`、`pop` はありません。集合演算が必要なら [Set](map-set.md) を使うか、fold と insert で書きます。

## 反復順序

fold、iter、keys、values、to_array は entry の並びの順に走査します。並びは挿入と remove の列だけで決まります。

- 新しいキーは末尾に付きます。
- 既存のキーへの insert は位置を変えず、値だけを置き換えます。
- remove は削除した位置へ末尾の entry を移し、長さを 1 減らします（swap-remove）。削除があると「挿入順」ではなくなります。
- ハッシュ値、添字表の大きさ、target（native と WASM）、最適化段階、seed には依存しません。seed 付きの map も、同じ操作列で同じ順序になります。
- HashSet の順序は、同じキー列を同じ操作順で扱った HashMap のキーの順序と同じです。

キー `a b c d` の順に insert した map の変化は次のとおりです。

| 操作 | 走査順 |
| --- | --- |
| `a`、`b`、`c`、`d` を insert | `a b c d` |
| `b` をもう一度 insert（既存のキー） | `a b c d`（位置は変わらない） |
| `b` を remove | `a d c`（末尾の `d` が移る） |
| `a` を remove | `c d`（末尾の `c` が移る） |
| `d` を remove（末尾の entry） | `c`（移動なし） |

```tsuzuri run=143
let mut map: HashMap<string, i64> = HashMap.empty()
map = HashMap.insert map "a" 1
map = HashMap.insert map "b" 2
map = HashMap.insert map "c" 3
map = HashMap.insert map "d" 4
map = HashMap.remove map "b"
HashMap.fold (\total _key value -> total * 10 + deref value) 0 (ref map)
```

`b` を remove した後の走査順は `a d c` なので、値を `1`、`4`、`3` の順に積んで `143` になります。キーの順に列挙したいときは Map を使います。

## キーの契約

キーの型には `Hash` と `Eq` の instance が必要です。組み込みの型にはあり、record と union は `deriving (Eq, Hash)` で導出できます（[自動導出](../language-reference/deriving.md)）。どちらかがなければ `E1005` です。

```tsuzuri run=230
record Point { x: i64, y: i64 } deriving (Eq, Hash)

let mut visits: HashMap<Point, i64> = HashMap.empty()
visits = HashMap.insert visits (Point { x: 1, y: 2 }) 10
visits = HashMap.insert visits (Point { x: 3, y: 4 }) 20
visits = HashMap.insert visits (Point { x: 1, y: 2 }) 30
HashMap.length (ref visits) * 100 + deref (HashMap.at (ref visits) (Point { x: 1, y: 2 }))
```

同じ座標へ二度 insert したので件数は 2 で、その値は後の `30` です。結果は `2 * 100 + 30 = 230` です。

- `Eq` と `Hash` は整合させます。`Eq` で等しいキーは、等しい `Hash.hash` を返さなければなりません。破ってもメモリ安全で、探索は必ず終わりトラップも増えませんが、同じキーが別の entry になる、検索で見つからないなど、結果は規定されません。`deriving` した instance は整合します。
- 浮動小数点のキーでは、`-0.0` と `0.0` は `Eq` で等しく `Hash` も同じなので、同じ entry です。最初に insert したキーが代表として残ります。
- NaN は `Eq.eq key key` が false なので、insert、remove、検索のどれでもトラップします。空の map への問い合わせも同じです。格納済みのキーは insert のときに検査を通っているため、あらためて検査しません。
- キーを使う操作は、`Eq.eq key key`、`Hash.hash key`、同じ混合ハッシュを持つ entry との `Eq.eq` の順に利用者の instance を呼びます。添字表の再構築は格納済みのハッシュを使うので、instance を呼び直しません。

```tsuzuri run=1
let mut zeros: HashSet<f64> = HashSet.empty()
zeros = HashSet.insert zeros (-0.0)
zeros = HashSet.insert zeros 0.0
HashSet.length (ref zeros)
```

`-0.0` と `0.0` は同じキーなので、件数は 1 です。

## 容量と負荷率

内部は、entry を並べた配列と、その位置を指す開番地法の添字表です。添字表の大きさは 8 以上の 2 の冪で、負荷率は最大 1/2（`2 * 件数 <= 添字表の大きさ`）です。

- 空の map は何も確保しません。最初の insert で添字表を 8 にし、新しいキーの insert が負荷率を超えるときだけ、先に添字表を 2 倍にして作り直します。既存のキーの値を置き換えても伸びません。
- `with_capacity count` は、`count` 件まで再確保なしで insert できる map を作ります。`count` が 0 なら `empty()` と同じで確保しません。負の値と 2^60（`1152921504606846976`）を超える値はトラップします。
- 縮小はありません。remove しても、添字表と entry 配列の容量は残ります。

## 計算量

| 操作 | 期待 | 全キーが衝突する最悪 |
| --- | --- | --- |
| 新しいキーの insert | 償却 O(1)（添字表の倍増を含む） | O(n) |
| 既存キーの insert、remove、contains_key、get、at | O(1) | O(n) |
| `length`、`is_empty`、`empty` | O(1) | O(1) |
| 全走査（fold、iter、keys、values、to_array） | O(n) | O(n) |
| `with_capacity n` | O(n) | O(n) |

期待値は、ハッシュが添字表へ一様に散る場合のものです。`Hash.hash` と `Eq.eq` の費用（例えば文字列の長さに比例する費用）は含みません。検索の内部では確保しません。このページは計算量だけを示し、Map や他言語のコンテナとの速度比較は載せていません。

## Map との使い分け

| 観点 | Map / Set | HashMap / HashSet |
| --- | --- | --- |
| キーの制約 | `Ord` | `Hash` と `Eq` |
| 検索 | O(log n) | 平均 O(1) |
| 挿入・削除 | O(n) | 平均 O(1)（insert は償却） |
| 走査順 | キーの昇順 | entry の並び。挿入順で、remove で末尾の entry が移る |
| 集合演算 | `union`、`intersect`、`difference` | なし |
| 借用したキーでの検索 | なし。検索キーは値で渡す | `_ref` の API |
| 衝突を狙った入力 | キーの値の並びに費用が左右されない | 既定の map は無防備。seed 付きは部分的な保護 |

キーの昇順で列挙したい、集合演算が必要、信頼できない入力をキーにする場合は Map / Set を選びます。検索と更新が中心で、走査順が挿入順（と swap-remove）で足りるなら HashMap / HashSet を選びます。大きさの目安を示す測定は載せていないので、実際の入力で確かめてください。

## HashDoS と seed

HashDoS は、同じ添字表の位置に集まるキーを大量に送り込み、検索と挿入を最悪の O(n) に追い込む攻撃です。

### 既定の map

`HashMap.empty()` と `with_capacity` の map に耐性はありません。添字表の位置は、`Hash.hash`（FNV-1a）の結果を固定の混合関数（MurmurHash3 の fmix64）で拡散して決めます。どちらも公開された固定の関数なので、攻撃者は全キーを同じ位置へ置く入力を作れます。このとき各操作は O(n)、n 件の insert は O(n²) になります。結果の正しさは変わらず、遅くなるだけです。

### seed 付きの map

`with_seed`、`with_capacity_and_seed`、`randomized`、`try_randomized` の map は、`Hash.hash` の結果を SipHash-1-3 で処理して添字表の位置を決めます。鍵は seed から作る 128 bit で、seed を知らなければ位置を選べません。

固定の混合関数で同じ位置になるキーを 600 個選んで insert し、`longest_probe` を測ると、既定の map は 599、seed 付きの map は 200 個の seed（0〜199）で 2 から 13 でした。これは添字表の位置の偏りの測定で、実行時間の測定ではありません。同じ構成を `tests/fixtures/hash_map/Main.tz` の `hash_flood` が検査します。

seed を与えても走査順は変わりません。次の例は 100 個のキーを既定の map と seed 付きの map の両方へ insert し、走査順が同じことを確かめます。

```tsuzuri run=true
let mut plain: HashMap<i64, i64> = HashMap.empty()
let mut seeded: HashMap<i64, i64> = HashMap.with_seed 20260929i64u
let mut key = 0
while key < 100 do
    plain = HashMap.insert plain (key * 8) key
    seeded = HashMap.insert seeded (key * 8) key
    key = key + 1
HashMap.keys (ref plain) == HashMap.keys (ref seeded)
```

### 保護の限界

保護は部分的です。

- `Hash.hash` の 64 bit の結果そのものが衝突するキーは、seed があっても衝突します。FNV-1a は鍵付きではなく、seed は結果に後から掛ける SipHash-1-3 にだけ作用するためです。
- seed は推測できない値でなければなりません。定数や、時刻・連番のように攻撃者が知りうる値では保護になりません。
- 信頼できない入力をキーにするなら、キーの値の並びに費用が左右されない Map を優先します。HashMap を使うなら `randomized` を使い、上の限界を前提にします。

### OS の乱数とホストの seed

`randomized` と `try_randomized` は、[OS API](os.md) の `Random.next_u64` で seed を得る `IO` アクションです。seed を得られないとき、`randomized` はトラップし、`try_randomized` は `Result.Error`（`Os.Error`）で返します。固定の seed へ黙って置き換えることはありません。

```tsuzuri
def main :: IO<unit> =
    let! made = HashMap.randomized ()
    let mut map: HashMap<i64, i64> = made
    map = HashMap.insert map 10 1
    map = HashMap.insert map 20 2
    do! IO.write_line (HashMap.length (ref map))
```

```tsuzuri
def report :: Result.Result<HashMap<i64, i64>, Os.Error> -> IO<unit> = \made ->
    match made with
    | Result.Ok fresh ->
        let map = HashMap.insert (HashMap.insert fresh 10 1) 20 2
        IO.write_line (HashMap.length (ref map))
    | Result.Error error -> IO.write_line (Os.message error)

def main :: IO<unit> =
    let! made = HashMap.try_randomized ()
    do! report made
```

OS の乱数を使えるのは、Windows を除く native と `--wasm-host wasi` のビルドです。既定の wasm32 には OS API がないため、`randomized` と `try_randomized` を含むビルドは `E2000` で拒否され、出力ファイルは作られません。この場合は、ホストが選んだ推測できない seed を引数で受け取り、`with_seed` へ渡します。seed 付きの map を作るだけなら OS API は不要で、既定の wasm32 にも追加の import は出ません。

```tsuzuri
export def distinct_residues :: i64 -> i64 -> i64
fn distinct_residues host_seed count =
    let mut seen: HashSet<i64> = HashSet.with_seed (host_seed as i64u)
    let mut index = 0
    while index < count do
        seen = HashSet.insert seen (index * 7 % 1000)
        index = index + 1
    HashSet.length (ref seen)
```

## 所有権と不透明性

HashMap と HashSet は常に非 Copy で、スコープの終わりに解放されます。内部フィールドの構築、参照、パターン分解、レコード更新によるアクセスは `E1022` です。API 生成ページに内部の型宣言が表示されても、利用者がそのフィールドを操作できるという意味ではありません。

at の結果は map の共有借用で、借用が生きている間に map を更新・move すると `E1014`、関数の外へ返すと `E1013` です。fold の関数と iter の要素は、キーと値の共有参照を受け取ります。キーを変えると添字表が壊れるため、キーの排他借用は提供しません。

公開 ABI の署名には使えず（`E1008`）、`HashMap` と `HashSet` は予約されたモジュール名なので、同名の利用者モジュールは `E1011` です。

未対応の範囲は、縮小、集合演算、`singleton`、`pop`、値の可変 iterator、直接の公開 ABI です。SIMD によるグループ探索は計画中で、現在の実装は SIMD を使いません。

## API と関連項目

- [HashMap のソース宣言](api/HashMap.md)、[HashSet のソース宣言](api/HashSet.md)
- [Map と Set](map-set.md)
- [Seq と借用反復](sequences.md)
- [OS API](os.md)
- [自動導出](../language-reference/deriving.md)
