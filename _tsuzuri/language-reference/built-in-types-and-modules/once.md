# Once

`Once` は、関数を最大 1 回だけ呼ぶための値です（Go の `sync.Once`、TZ06）。`Once.call` は最初の呼び出しで関数を呼び、それと同時に来たほかの呼び出しは、最初の呼び出しが終わるまで待ちます。

## この記事のポイント

- `Once.create ()` で作り、`Once.call once f` で `f` を最大 1 回呼びます。 <!-- rule: TSK-027 -->
- `Once.call` が戻ったとき、関数はもう終わっています（同時に来た呼び出しは待ちます）。`Once.done` で終わったかを調べられます。
- 関数は普通の関数値で、呼び出しの間だけ使います（借用を捕捉してよい。`Mutex.with_lock` のコールバックと同じ）。
- 関数の中から同じ `Once` を呼ぶと、自分を待ちます（Go と同じく検査しません）。デッドロックの判定が見つけて、トラップします。
- ハンドルは持ち主の数を数え、`Once.share` でもう 1 つ作ります。値を 1 回だけ作るなら [OnceValue](./oncevalue.md) です。

## 基本の書き方

```tsuzuri run=1
let once = Once.create ()
let count = Atomic.create 0i64
let counting = ref count
for _ in 1 ..= 3 do
    Once.call (ref once) (\() -> {
        let _old = Atomic.fetch_add counting 1i64;
        ()
    })
Atomic.load (ref count)
```

実行結果:

```text
1
```

## API リファレンス

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `Once.create` | `unit -> Once` | まだ呼んでいない `Once` |
| `Once.call` | `ref Once -> (unit -> unit) -> unit` | 最初の 1 回だけ関数を呼ぶ。同時に来た呼び出しはその終わりを待つ |
| `Once.done` | `ref Once -> bool` | 関数が終わったか |
| `Once.share` | `ref Once -> Once` | 同じ `Once` のハンドルをもう 1 つ |

## 引数の順序

`Once` は資源のハンドルの例外で、ハンドルが先頭の引数です（`Once.call once f`。Go の `once.Do(f)` と同じ）。[引数の順序](./argument-order.md)を見てください。

## 型とターゲット

- `Once` は非 Copy の不透明なハンドルです（構築・分解は `E1022`）。`Send`・`Sync` で、何度も呼べる関数値には捕捉できません（`E1005`）。
- ネイティブの macOS・Linux・Windows で動きます。既定の WebAssembly では 1 本の実行として使えます（関数の中から同じ `Once` を呼ぶとトラップ）。

## 関連項目

- [OnceValue](./oncevalue.md)
- [WaitGroup](./waitgroup.md)
- [言語リファレンスの目次](../index.md)
