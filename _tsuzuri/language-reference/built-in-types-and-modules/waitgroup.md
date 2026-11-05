# WaitGroup

`WaitGroup` は、終わりを待つ仕事の数を数えるカウンターです（Go の `sync.WaitGroup`、TZ06）。`WaitGroup.add` で始める前に数え、`WaitGroup.done` で終わりを知らせ、`WaitGroup.wait` で数が 0 になるまで待ちます。`WaitGroup.go` は、数に 1 を足し、関数を新しい tzroutine で呼び、関数が戻ったら 1 を引きます（待つのは呼んだ側の `WaitGroup.wait` です）。

## この記事のポイント

- `WaitGroup.create ()` で数 0 のカウンターを作ります。ハンドルは持ち主の数を数え、`WaitGroup.share` でもう 1 つ作って tzroutine へ移します。最後のハンドルの drop で解放されます。
- `WaitGroup.wait` は、待つ tzroutine だけを止めます（OS スレッドは止めません）。`main` などのスレッドでは、そのスレッドが待ちます。 <!-- rule: TSK-026 -->
- 数が負になると `Tsuzuri runtime: negative WaitGroup counter` を書いてトラップします（Go の panic）。
- Go と同じく、`wait` が戻る前に数を 0 から増やす使い方は誤りです（待ち手が戻らないことがあります）。実行時には検査しません。
- `Routine.scope` の子や `Parallel.*` のコールバックの中の `wait` も、待つのはその tzroutine だけです（[Channel](./channel.md#待ちとデッドロック) の待ちと同じ）。子が互いを待つ形は tzroutine か [`Routine.scope`](./routine.md#routinescope) で書きます（`Parallel.*` の同じチャンクの要素は順に呼ばれるので、互いを待てません）。

## 基本の書き方

```tsuzuri run=sum%3D4950
let group = WaitGroup.create ()
let total = Arc.new (Mutex.create 0i64)
for i in 0 ..< 100 do
    WaitGroup.add (ref group) 1
    let done = WaitGroup.share (ref group)
    let mine = Arc.share (ref total)
    tzroutine {
        Mutex.with_lock (Arc.get (ref mine)) (\value -> deref value = deref value + i)
        WaitGroup.done (ref done)
    }
WaitGroup.wait (ref group)
$"sum={Mutex.with_lock (Arc.get (ref total)) (\value -> deref value)}"
```

実行結果:

```text
sum=4950
```

## WaitGroup.go

`WaitGroup.go group f` は、数に 1 を足してから、所有する関数 `f`（[Owned](./owned.md)）を新しい tzroutine で呼び、終わったら 1 を引きます（Go 1.25 の `wg.Go(f)`）。tzroutine はグループのハンドルを 1 つ持ち、終わりで離します。

```tsuzuri run=500500
let group = WaitGroup.create ()
let total = Arc.new (Atomic.create 0i64)
for i in 1 ..= 1000 do
    let mine = Arc.share (ref total)
    WaitGroup.go (ref group) (Owned.function (\() -> {
        let _old = Atomic.fetch_add (Arc.get (ref mine)) i;
        ()
    }))
WaitGroup.wait (ref group)
Atomic.load (Arc.get (ref total))
```

実行結果:

```text
500500
```

## API リファレンス

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `WaitGroup.create` | `unit -> WaitGroup` | 数 0 のカウンター |
| `WaitGroup.add` | `ref WaitGroup -> i64 -> unit` | 数に足す。負になればトラップ |
| `WaitGroup.done` | `ref WaitGroup -> unit` | `add group (-1)` |
| `WaitGroup.wait` | `ref WaitGroup -> unit` | 数が 0 になるまで待つ |
| `WaitGroup.go` | `ref WaitGroup -> Owned.Function<unit, unit> -> unit` | 1 を足して関数を新しい tzroutine で呼び、終わったら 1 を引く |
| `WaitGroup.share` | `ref WaitGroup -> WaitGroup` | 同じカウンターのハンドルをもう 1 つ |

## 引数の順序

`WaitGroup` は資源のハンドルの例外で、ハンドルが先頭の引数です（`WaitGroup.add group 1`。Go の `wg.Add(1)` と同じ）。[引数の順序](./argument-order.md)を見てください。

## 型とターゲット

- `WaitGroup` は非 Copy の不透明なハンドルです（構築・分解は `E1022`）。`Send`・`Sync` で、何度も呼べる関数値には捕捉できません（`E1005`）。
- ネイティブの macOS・Linux・Windows で動きます。既定の WebAssembly では、tzroutine を起動しないプログラムが 1 本の実行として使えます（待つべき `wait` はトラップ）。`WaitGroup.go` は tzroutine を起動するので、WebAssembly では `E2000` です。

## Go との対応

| Go | Tsuzuri |
| --- | --- |
| `var wg sync.WaitGroup` | `let group = WaitGroup.create ()` |
| `wg.Add(n)`・`wg.Done()`・`wg.Wait()` | `WaitGroup.add group n`・`done group`・`wait group` |
| `wg.Go(f)` | `WaitGroup.go group f` |

## 関連項目

- [tzroutine](../async-tasks-and-lazy/tzroutine.md)
- [Mutex](./mutex.md)
- [Once](./once.md)
- [言語リファレンスの目次](../index.md)
