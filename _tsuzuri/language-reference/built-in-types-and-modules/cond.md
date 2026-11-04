# Cond

`Cond` は、`Mutex` で守る値の条件が成り立つまで待つための値です（Go の `sync.Cond`、TZ06 Phase 2）。`Cond.wait_with` はロックを取り、述語が真になるまで「ロックを外して待ち、起こされたら取り直して述語を見直す」を繰り返し、真になったら本体をロックの下で呼びます。

## この記事のポイント

- `Cond.create ()` で作ります。`Cond.signal` は待つ者を 1 人、`Cond.broadcast` は全員起こします。条件を変えた後（ロックの中で値を変えた後）に呼びます。
- 述語を見直すので、余分な起床は害がありません。待つ間はロックを持ちません。
- Go の `sync.NewCond(&mu)` と違い、`Cond` はどの `Mutex` とも組めます（呼び出しごとに渡します）。
- 本体の結果は `Send` で、ロック中の値への借用を持てません（`Mutex.with_lock` と同じ `E1013`）。
- ハンドルは持ち主の数を数え、`Cond.share` でもう 1 つ作ります。

## 基本の書き方

```tsuzuri run=done
def run :: unit -> string
fn run _ =
    let ready = Arc.new (Mutex.create false)
    let cond = Cond.create ()
    let flag = Arc.share (ref ready)
    let wake = Cond.share (ref cond)
    tzroutine {
        Mutex.with_lock (Arc.get (ref flag)) (\value -> deref value = true)
        Cond.broadcast (ref wake)
    }
    Cond.wait_with (ref cond) (Arc.get (ref ready)) (\value -> deref value) (\_value -> "done")

run ()
```

実行結果:

```text
done
```

## API リファレンス

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `Cond.create` | `unit -> Cond` | 条件 |
| `Cond.wait_with` | `Send<'b> => ref Cond -> ref Mutex<'a> -> (ref 'a -> bool) -> (ref mut 'a -> 'b) -> 'b` | 述語が真になるまで待ち、本体をロックの下で呼ぶ |
| `Cond.signal` | `ref Cond -> unit` | 待つ者を 1 人起こす |
| `Cond.broadcast` | `ref Cond -> unit` | 全員起こす |
| `Cond.share` | `ref Cond -> Cond` | 同じ条件のハンドルをもう 1 つ |

## 引数の順序

`Cond` は資源のハンドルの例外で、ハンドルが先頭の引数です（`Cond.wait_with cond lock ready body`）。[引数の順序](./argument-order.md)を見てください。

## 表現とターゲット

- 語は 64 ビットの通し番号です。待つ側はロックの下で番号を読み、ロックを外して番号が変わるまで待ちます。起こす側は番号を 1 増やしてから起こすので、外してから待つまでの間の起こしも失われません。
- ネイティブの macOS・Linux・Windows で動きます。既定の WebAssembly では、tzroutine を起動しないプログラムが 1 本の実行として使え、述語が偽の `wait_with` は誰も起こせないのでトラップします。`--wasm-feature jspi` で tzroutine を起動する wasm32 のプログラムでは、ネイティブと同じく、待つのはその tzroutine だけです（[tzroutine の WebAssembly](../async-tasks-and-lazy/tzroutine.md#webassemblyjspi)）。

## 関連項目

- [Mutex](./mutex.md)
- [Semaphore](./semaphore.md)
- [言語リファレンスの目次](../index.md)
