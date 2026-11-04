# Semaphore

`Semaphore` は、重み付きのセマフォです（`golang.org/x/sync/semaphore.Weighted`、TZ06 Phase 2）。同時に走る仕事の数（や量）を容量で制限します。

## この記事のポイント

- `Semaphore.create n` で容量 `n`（0 以上）のセマフォを作ります。`acquire s w` は重み `w` が空くまで待って取り、`release s w` で返します。`try_acquire` は待たずに取れたかを返します。
- 重みが負か容量より大きいと `Tsuzuri runtime: Semaphore weight is negative or larger than its capacity` を、容量を超えて返すと `Tsuzuri runtime: Semaphore released more than it acquired` を書いてトラップします。容量が負でも前者でトラップします。
- 先着順は保証しません（Go の `Weighted` は先着順。返したときに待つ者を全員起こし、取れる者が取ります）。
- ハンドルは持ち主の数を数え、`Semaphore.share` でもう 1 つ作ります。

## 基本の書き方

```tsuzuri run=101
let slots = Semaphore.create 2
let a = if Semaphore.try_acquire (ref slots) 1 then 1 else 0
let b = if Semaphore.try_acquire (ref slots) 2 then 1 else 0
Semaphore.release (ref slots) 1
let c = if Semaphore.try_acquire (ref slots) 2 then 1 else 0
a * 100 + b * 10 + c
```

実行結果:

```text
101
```

## API リファレンス

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `Semaphore.create` | `i64 -> Semaphore` | 容量のセマフォ |
| `Semaphore.acquire` | `ref Semaphore -> i64 -> unit` | 重みが空くまで待って取る |
| `Semaphore.try_acquire` | `ref Semaphore -> i64 -> bool` | 待たずに取る |
| `Semaphore.release` | `ref Semaphore -> i64 -> unit` | 返す |
| `Semaphore.share` | `ref Semaphore -> Semaphore` | 同じセマフォのハンドルをもう 1 つ |

## 引数の順序

`Semaphore` は資源のハンドルの例外で、ハンドルが先頭の引数です（`Semaphore.acquire s w`。Go の `s.Acquire(ctx, w)` と同じ）。[引数の順序](./argument-order.md)を見てください。

## ターゲット

ネイティブの macOS・Linux・Windows で動きます。既定の WebAssembly では、tzroutine を起動しないプログラムが 1 本の実行として使え、待つべき `acquire` はトラップします。`--wasm-feature jspi` で tzroutine を起動する wasm32 のプログラムでは、ネイティブと同じく、待つのはその tzroutine だけです（[tzroutine の WebAssembly](../async-tasks-and-lazy/tzroutine.md#webassemblyjspi)）。

## 関連項目

- [WaitGroup](./waitgroup.md)
- [Cond](./cond.md)
- [言語リファレンスの目次](../index.md)
