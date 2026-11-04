# SyncMap

`SyncMap<K, V>` は、多くの tzroutine が同時に読み書きするマップです（Go の `sync.Map`、TZ06 Phase 3）。中身は [RwMutex](./rwmutex.md) で守る [HashMap](./hashmap.md) で、std の Tsuzuri で書かれています。

## この記事のポイント

- `SyncMap.create ()` で空のマップを作ります。読み（`load`・`with_value`・`length`・`for_each`）は読みのロックを分け合い、書き（`store`・`load_or_store`・`delete`）は書きのロックを 1 人で取ります。
- `load` は値のコピー（`V` は `Copy`）、`with_value` はロックの下で値を借りて関数の結果を返します。
- `with_value` の関数の結果は、関数値のように借用を持ちうる型にはできません（`E1013`）。std の中の `RwMutex.with_read` が、呼び出し元の関数の結果がロック中の値を借りていないことを示せないためです。値を取り出して返すか、`load` を使います。
- `for_each` の関数は読みのロックの中で呼ばれます。その中から同じマップへ書くと、自分を待つのでトラップします（`RwMutex` の規則）。
- 複数の tzroutine で共有するときは `Arc<SyncMap<K, V>>` にします。

## 基本の書き方

```tsuzuri run=20
let map: SyncMap<i64, i64> = SyncMap.create ()
SyncMap.store (ref map) 1 10
SyncMap.store (ref map) 2 20
let _removed = SyncMap.delete (ref map) (ref 1)
match SyncMap.load (ref map) (ref 2) with
| Maybe.Some value -> value
| Maybe.None -> 0
```

実行結果:

```text
20
```

## API リファレンス

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `SyncMap.create` | `(Send<'k>, Send<'v>) => unit -> SyncMap<'k, 'v>` | 空のマップ |
| `SyncMap.load` | `(Hash<'k>, Eq<'k>, Copy<'v>, Send<'v>) => ref SyncMap<'k, 'v> -> ref 'k -> Maybe<'v>` | 値のコピー（Go の `Load`） |
| `SyncMap.with_value` | `(Hash<'k>, Eq<'k>, Send<'b>) => ref SyncMap<'k, 'v> -> ref 'k -> (ref 'v -> 'b) -> Maybe<'b>` | 値を借りた関数の結果 |
| `SyncMap.store` | `(Hash<'k>, Eq<'k>, Send<'k>, Send<'v>) => ref SyncMap<'k, 'v> -> 'k -> 'v -> unit` | 値を置く（Go の `Store`）。関数値に捕捉できない値（`Mutex` など）も置ける |
| `SyncMap.load_or_store` | `(Hash<'k>, Eq<'k>, Copy<'v>, Send<'k>, Send<'v>) => ref SyncMap<'k, 'v> -> 'k -> 'v -> ('v * bool)` | あれば今の値と `true`、なければ置いて渡した値と `false`（Go の `LoadOrStore`） |
| `SyncMap.delete` | `(Hash<'k>, Eq<'k>) => ref SyncMap<'k, 'v> -> ref 'k -> bool` | 取り除く。あったなら真（Go の `Delete`） |
| `SyncMap.length` | `ref SyncMap<'k, 'v> -> i64` | キーの数 |
| `SyncMap.for_each` | `ref SyncMap<'k, 'v> -> (ref 'k -> ref 'v -> unit) -> unit` | すべてのキーと値（Go の `Range`。順序は決まらない） |

## 引数の順序

`SyncMap` は `Mutex` と同じく tzroutine が分け合う資源なので、マップが先頭の引数です（`SyncMap.store map key value`。Go の `m.Store(key, value)` と同じ）。[引数の順序](./argument-order.md)を見てください。

## 表現と性能

- 書きは、ロックの下で `HashMap` を作り直します（`HashMap.insert`・`remove_ref` は所有する表を受け取って返すので、私的な組み込み `SyncMap.__replace` で場所の表を空の表と入れ替えてから渡します。トラップしても場所には完全な表が残ります）。`HashMap` と同じ償却の費用で、書きの間は読みも待ちます。
- 値をコピーせずに読むには `with_value` を使います。

## 関連項目

- [RwMutex](./rwmutex.md)
- [HashMap](./hashmap.md)
- [言語リファレンスの目次](../index.md)
