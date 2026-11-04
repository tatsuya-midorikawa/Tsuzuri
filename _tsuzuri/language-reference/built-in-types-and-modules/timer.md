# Timer

`Timer` は、一定時間後に時刻をチャンネルへ 1 回送るタイマーです（Go の `time.Timer`）。`Time` のタイマーと同じく、呼んだ時点で働く関数です（`IO` を返しません）。`Timer.create` で作り、`Timer.channel` でチャンネルのハンドルを取り、`Timer.stop` で止め、`Timer.reset` で送り直します。[`Time.after_func`](./time.md#遅らせて呼ぶtimeafter_func) が返すのも `Timer` です。

必要な権限: `time`（`Timer.create` と `Timer.reset`。時計を読みます）。

## この記事のポイント

- `Timer.create d` は、`d` ナノ秒後に時刻（`Time.monotonic_ns` と同じ時計のナノ秒）を容量 1 のチャンネルへ送ります。
- `Timer.stop` は、まだ送っていなければ止めて真を返します。止めた後のチャンネルには、止める前に送った時刻も残りません（Go 1.23 以降と同じ）。 <!-- rule: TSK-024 -->
- `Timer.reset t d` は、今から `d` 後に送り直します。まだ待っていたなら真、止めていたか送った後なら偽です。
- `Timer` は所有値で、drop するとタイマーを止めて解放します。`main`・`export`・`test`・`bench` の終わりも、その中で作ったタイマーを止めます（待ちません）。

## 基本の書き方

```tsuzuri run=fired
let timer = Timer.create (5 * Time.millisecond())
let fired = Timer.channel timer
match Channel.recv fired with
| Maybe.Some _ -> "fired"
| Maybe.None -> "closed"
```

実行結果:

```text
fired
```

`Timer.channel` はチャンネルのハンドルを 1 つ増やします（`Channel.share` と同じ）。ループの中で毎回取らず、前に 1 回取ります。

## 止めて送り直す

```tsuzuri run=10010
def steps :: unit -> i64
fn steps _ =
    let timer = Timer.create Time.hour()
    let fired = Timer.channel timer
    let digit = \flag -> if flag then 1i64 else 0i64
    let first = digit (Timer.stop timer)
    let second = digit (Timer.stop timer)
    let third = digit (Timer.reset timer (2 * Time.millisecond()))
    let fourth = match Channel.recv fired with
        | Maybe.Some _ -> 1i64
        | Maybe.None -> 0i64
    let fifth = digit (Timer.stop timer)
    first * 10000 + second * 1000 + third * 100 + fourth * 10 + fifth

steps ()
```

実行結果:

```text
10010
```

1 回目の `stop` は待っていたタイマーを止め（真）、2 回目は何もしません（偽）。止めたタイマーの `reset` は偽を返して送り直し、チャンネルが時刻を受け取ります。送った後の `stop` は偽です。

`Time.after_func` の `Timer` のチャンネルは、閉じたままの空のチャンネルです。`stop` した関数は `reset` で予定し直せます。一度呼んだ後の `reset` は偽を返し、何も予定しません（所有する関数は 1 回しか呼べないため。Go は同じ関数をもう一度呼びます）。

## 所有権と境界

`Timer` は不透明な所有値です（Copy でなく、`Timer { handle: … }` で作ったり分解したりすると `E1022`）。`Send`・`Sync` なので tzroutine へ移せますが、何度も呼べる関数値には捕捉できません（`E1005`）。借用（`ref Timer`）を渡します。

drop すると、まだ発火していなければ止め、`after_func` の関数がまだ呼ばれていなければ呼ばずに drop し、ランタイムの記録を解放します。チャンネルは、`Timer.channel` で取ったハンドルがなくなった時点で解放されます。

`main`・`export`・`test`・`bench` の終わりは、その中で作ったまだ発火していないタイマーを止めます。`Timer` のハンドルが境界の外へ出ても（`export` の戻り値など）、止められたまま drop を待ちます。

## ターゲット

ネイティブ（macOS・Linux・Windows）と、`--wasm-feature jspi` の wasm32 で動きます。WebAssembly の期限はグルーの `setTimeout` が知らせます。`jspi` のない WebAssembly は tzroutine と同じく `E2000` です（[JSPI の tzroutine](../compiler/webassembly.md#jspi-の-tzroutine)）。

## API リファレンス

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `Timer.create` | `i64 -> Timer` | そのナノ秒後に 1 回だけ時刻を送るタイマー |
| `Timer.channel` | `ref Timer -> Channel<i64>` | タイマーのチャンネルのハンドル。`after_func` のタイマーでは閉じた空のチャンネル |
| `Timer.stop` | `ref Timer -> bool` | 止める。まだ送っていなければ真 |
| `Timer.reset` | `ref Timer -> i64 -> bool` | 今からそのナノ秒後に送り直す。まだ待っていたなら真。0 以下はすぐ送る |

## 引数の順序

`Timer` は資源のハンドルの例外で、ハンドルが先頭の引数です（`Timer.reset timer d`。Go の `t.Reset(d)` と同じ）。[引数の順序](./argument-order.md)を見てください。

## Go との対応

| Go | Tsuzuri |
| --- | --- |
| `t := time.NewTimer(d)` | `let t = Timer.create d` |
| `<-t.C` | `Channel.recv (Timer.channel t)` |
| `t.Stop()` | `Timer.stop t` |
| `t.Reset(d)` | `Timer.reset t d` |
| `time.AfterFunc(d, f)` | `Time.after_func d f` |

## まとめ

- `Timer.create d` は `d` 後に時刻を 1 回送ります。`Timer.channel` でチャンネルを取ります。
- `stop` と `reset` は、まだ待っていたかを返します。止めた後のチャンネルに古い時刻は残りません。
- drop と境界の終わりがタイマーを止めます。境界はタイマーを待ちません。

## 関連項目

- [Time](./time.md)
- [Ticker](./ticker.md)
- [Channel](./channel.md)
- [select](../async-tasks-and-lazy/select.md)
- [言語リファレンスの目次](../index.md)
