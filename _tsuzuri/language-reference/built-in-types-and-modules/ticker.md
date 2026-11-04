# Ticker

`Ticker` は、周期ごとに時刻をチャンネルへ送るティッカーです（Go の `time.Ticker`）。`Ticker.create` で作り、`Ticker.channel` でチャンネルのハンドルを取り、`Ticker.stop` で止め、`Ticker.reset` で周期を変えます。

必要な権限: `time`（`Ticker.create` と `Ticker.reset`。時計を読みます）。

## この記事のポイント

- `Ticker.create d` は、`d` ナノ秒ごとに時刻（`Time.monotonic_ns` と同じ時計のナノ秒）を送ります。最初は今から `d` 後です。`d` が 0 以下ならトラップします。
- チャンネルは 1 つだけ時刻を持ちます。受け手が前の時刻を受け取っていなければ、その回は捨てられます（Go と同じ）。処理が遅れて次の期限を過ぎていたら、次は「今 + d」です。
- `Ticker.stop` で止まり、止める前に送った時刻も残りません。`Ticker.reset t d` は周期を `d` にして今から数え直します。
- `Ticker` は stop されるか drop されるまで止まりません（[`Time.tick`](./time.md#周期timetick) はチャンネルのハンドルがなくなれば止まります）。

## 基本の書き方

```tsuzuri run=true
let ticker = Ticker.create (2 * Time.millisecond())
let ticks = Ticker.channel ticker
let first = Channel.recv ticks
let second = Channel.recv ticks
Ticker.stop ticker
match (first, second) with
| (Maybe.Some earlier, Maybe.Some later) -> later > earlier
| _ -> false
```

実行結果:

```text
true
```

## 周期の処理を止める

ティッカーと終わりの合図を 1 つの [select](../async-tasks-and-lazy/select.md) で待つと、周期の処理を外から止められます。

```tsuzuri run=stopped
def worker :: ref Channel<unit> -> ref Channel<i64> -> unit
fn worker quit report =
    let ticker = Ticker.create (2 * Time.millisecond())
    let ticks = Ticker.channel ticker
    let mut count = 0i64
    let mut running = true
    while running do
        select
        | recv _ from ticks -> count = count + 1
        | recv _ from quit -> running = false
    Channel.send report count

def run :: unit -> string
fn run _ =
    let quit: Channel<unit> = Channel.make 0
    let reports: Channel<i64> = Channel.make 1
    let worker_quit = Channel.share quit
    let worker_report = Channel.share reports
    tzroutine {
        worker (ref worker_quit) (ref worker_report)
    }
    Time.sleep (10 * Time.millisecond())
    Channel.send quit ()
    match Channel.recv reports with
    | Maybe.Some _ -> "stopped"
    | Maybe.None -> "closed"

run ()
```

実行結果:

```text
stopped
```

`worker` が返ると `ticker` は drop され、止まります。

## 所有権と境界

`Ticker` は不透明な所有値です（Copy でなく、作ったり分解したりすると `E1022`）。`Send`・`Sync` で tzroutine へ移せますが、何度も呼べる関数値には捕捉できません（`E1005`）。drop するとティッカーを止め、ランタイムの記録を解放します。チャンネルは、`Ticker.channel` で取ったハンドルがなくなった時点で解放されます。`main`・`export`・`test`・`bench` の終わりも、その中で作ったティッカーを止めます。

## ターゲット

ネイティブ（macOS・Linux・Windows）と、`--wasm-feature jspi` の wasm32 で動きます。`jspi` のない WebAssembly は tzroutine と同じく `E2000` です。

## API リファレンス

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `Ticker.create` | `i64 -> Ticker` | そのナノ秒ごとに時刻を送るティッカー。0 以下はトラップ |
| `Ticker.channel` | `ref Ticker -> Channel<i64>` | ティッカーのチャンネルのハンドル |
| `Ticker.stop` | `ref Ticker -> unit` | 止める。送った時刻も捨てる |
| `Ticker.reset` | `ref Ticker -> i64 -> unit` | 周期を変えて今から数え直す。0 以下はトラップ |

## 引数の順序

`Ticker` は資源のハンドルの例外で、ハンドルが先頭の引数です（`Ticker.reset ticker d`）。[引数の順序](./argument-order.md)を見てください。

## Go との対応

| Go | Tsuzuri |
| --- | --- |
| `t := time.NewTicker(d)` | `let t = Ticker.create d` |
| `<-t.C` | `Channel.recv (Ticker.channel t)` |
| `t.Stop()` | `Ticker.stop t` |
| `t.Reset(d)` | `Ticker.reset t d` |
| `time.Tick(d)` | `Time.tick d` |

## まとめ

- `Ticker.create d` は `d` ごとに時刻を送ります。受け手が遅れた回は捨てられます。
- `stop`・`reset`・drop・境界の終わりで止まります。
- 止めずに使い捨てるなら `Time.tick` です。

## 関連項目

- [Time](./time.md)
- [Timer](./timer.md)
- [Channel](./channel.md)
- [select](../async-tasks-and-lazy/select.md)
- [言語リファレンスの目次](../index.md)
