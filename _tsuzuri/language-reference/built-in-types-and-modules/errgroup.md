# ErrGroup

`ErrGroup<'e>` は、1 つの仕事を分けた tzroutine のグループです（Go の `golang.org/x/sync/errgroup`、TZ07）。`ErrGroup.go` で関数を新しい tzroutine で呼び、`ErrGroup.wait` で全員の終わりを待って最初の誤りを返します。最初の誤りはグループの `Context` も取り消すので、ほかの関数はそれを見て早く止まれます。

## この記事のポイント

- `ErrGroup.create ()` で作ります。`ErrGroup.with_context parent` は、グループの `Context` を `parent` の子にします。
- 関数は所有する関数 `Owned.Function<unit, Result<unit, 'e>>` です（[Owned](./owned.md)）。`Result.Error e` を返すと、グループの `Context` が取り消されます。
- `ErrGroup.wait` は、`go` に渡したすべての関数が戻ってから戻ります。 <!-- rule: TSK-030 --> 最初に終わった誤り（完了の時刻の順。Go と同じ）か `Ok` を返し、2 つ目以降の誤りは drop されます。`wait` もグループの `Context` を取り消します。
- `ErrGroup.set_limit n` で、同時に走る関数の数を `n` までにします（負は上限なし）。上限に達すると `go` は空くまで待ち、`try_go` は関数を `Result.Error` で返します。関数が走っている間に上限を変えるとトラップします（Go の panic）。
- ハンドルは持ち主の数を数え、`ErrGroup.share` でもう 1 つ作って tzroutine へ移します。

## 基本の書き方

```tsuzuri run=Error%203%2C%20stopped%209
let group: ErrGroup<i64> = ErrGroup.create ()
let stopped = Arc.new (Atomic.create 0i64)
for i in 1 ..= 10 do
    let ctx = ErrGroup.context group
    let mine = Arc.share (ref stopped)
    ErrGroup.go group (Owned.function (\() ->
        if i == 3 then Result.Error i
        else {
            let done = Context.done ctx;
            let _closed = Channel.recv done;
            let _old = Atomic.fetch_add (Arc.get (ref mine)) 1;
            Result.Ok ()
        }))
match ErrGroup.wait group with
| Result.Ok _ -> "Ok"
| Result.Error i -> $"Error {i}, stopped {Atomic.load (Arc.get (ref stopped))}"
```

実行結果:

```text
Error 3, stopped 9
```

3 番目の関数の誤りでグループの `Context` が取り消され、`done` を待っていたほかの 9 個が止まります。

## 同時に走る数の上限

```tsuzuri run=given%20back%3A%207
let group: ErrGroup<i64> = ErrGroup.create ()
ErrGroup.set_limit group 1
let gate: Channel<unit> = Channel.make 0
let held = Channel.share gate
ErrGroup.go group (Owned.function (\() -> {
    let _open = Channel.recv held;
    Result.Ok ()
}))
let refused = match ErrGroup.try_go group (Owned.function (\() -> Result.Error 7)) with
    | Result.Ok _ -> "started"
    | Result.Error f -> match Owned.call (ref f) () with
        | Result.Error n -> $"given back: {n}"
        | Result.Ok _ -> "given back"
Channel.close gate
let _done = ErrGroup.wait group
refused
```

実行結果:

```text
given back: 7
```

## API リファレンス

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `ErrGroup.create` | `Send<'e> => unit -> ErrGroup<'e>` | `Context` が `background` の子のグループ |
| `ErrGroup.with_context` | `Send<'e> => ref Context -> ErrGroup<'e>` | `Context` が `parent` の子のグループ |
| `ErrGroup.context` | `ref ErrGroup<'e> -> Context` | グループの `Context` のハンドル |
| `ErrGroup.go` | `Send<'e> => ref ErrGroup<'e> -> Owned.Function<unit, Result<unit, 'e>> -> unit` | 関数を新しい tzroutine で呼ぶ（上限があれば空くまで待つ） |
| `ErrGroup.try_go` | `Send<'e> => ref ErrGroup<'e> -> Owned.Function<unit, Result<unit, 'e>> -> Result<unit, Owned.Function<unit, Result<unit, 'e>>>` | 空きがなければ関数を返す |
| `ErrGroup.set_limit` | `ref ErrGroup<'e> -> i64 -> unit` | 同時に走る数の上限（負は上限なし。走っている間はトラップ） |
| `ErrGroup.wait` | `ref ErrGroup<'e> -> Result<unit, 'e>` | 全員の終わりを待ち、最初の誤りか `Ok` |
| `ErrGroup.share` | `ref ErrGroup<'e> -> ErrGroup<'e>` | 同じグループのハンドルをもう 1 つ |

## 引数の順序

`ErrGroup` は資源のハンドルの例外で、ハンドルが先頭の引数です（`ErrGroup.go group f`。Go の `g.Go(f)` と同じ）。[引数の順序](./argument-order.md)を見てください。

## 型とターゲット

- `ErrGroup` は std の Tsuzuri で書かれています（`WaitGroup`・容量 1 の `Channel<'e>`・`Arc<Mutex<…>>`・`Cond`・`Context`）。中身は std の外から見えません（`E1022`）。非 Copy で、何度も呼べる関数値には捕捉できません（`E1005`）。
- 「最初の誤り」は完了の時刻の順です。旧 `Task.parallel_results` の「入力の添字が最小の誤り」とは違います。
- ネイティブの macOS・Linux・Windows で動きます。tzroutine を起動するので、WebAssembly では `E2000` です。

## Go との対応

| Go | Tsuzuri |
| --- | --- |
| `var g errgroup.Group` | `let group = ErrGroup.create ()` |
| `g, ctx := errgroup.WithContext(parent)` | `let group = ErrGroup.with_context parent`、`let ctx = ErrGroup.context group` |
| `g.Go(f)`・`g.TryGo(f)`・`g.SetLimit(n)`・`g.Wait()` | `ErrGroup.go group f`・`try_go group f`・`set_limit group n`・`wait group` |

## 関連項目

- [Context](./context.md)
- [WaitGroup](./waitgroup.md)
- [tzroutine](../async-tasks-and-lazy/tzroutine.md)
- [言語リファレンスの目次](../index.md)
