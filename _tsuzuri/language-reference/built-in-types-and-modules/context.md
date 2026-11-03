# Context

`Context` は、取り消しと期限の木です（Go の `context`、TZ07）。`Context.background ()` で根を作り、`Context.with_cancel`・`with_timeout`・`with_deadline` で子を作ります。`Context.cancel` は、その `Context` と子孫をすべて取り消します。取り消された `Context` の `done` のチャンネルは閉じ、`Context.err` が理由を返します。

## この記事のポイント

- 取り消しは協調的です。取り消された `Context` を見ている tzroutine が自分で止まります。ランタイムが tzroutine を強制的に止めることはありません。
- 境界（`main`・`export`・`test`・`bench` の終わり）は、そこで起動した tzroutine の終わりを待ちます。終わらない tzroutine（サーバーのループ、周期の処理）は、`Context` の `done` を `select` で見て止めます。
- 取り消すと、自分と子孫の `done` が閉じます。親は取り消されません。 <!-- rule: TSK-029 -->
- 一度取り消された `Context` は取り消しのままで、`err` は最初の理由（`Context.Canceled` か `Context.DeadlineExceeded`）のまま変わりません。
- 期限は TZ05 のタイマーです。期限の時刻に `DeadlineExceeded` で取り消されます。子は親の早い期限を受け継ぎます。
- ハンドルは持ち主の数を数え、`Context.share` でもう 1 つ作って tzroutine へ移します。どのハンドルからでも取り消せます。取り消す権利を渡したくないときは、`with_cancel` で子を作って渡します。

## 基本の書き方

次の tzroutine は、受け手がいる間は数を送り続け、`done` が閉じたら止まります。

```tsuzuri run=stopped%20after%203
let root = Context.background ()
let ctx = Context.with_cancel root
let ticks: Channel<i64> = Channel.make 0
let worker_ctx = Context.share ctx
let out = Channel.share ticks
tzroutine {
    let done = Context.done worker_ctx
    let mut n = 0i64
    let mut running = true
    while running do
        select
        | recv _ from done -> running = false
        | send n to out -> n = n + 1
}
let mut seen = 0i64
for _ in 1 ..= 3 do
    match Channel.recv ticks with
    | Maybe.Some _ -> seen = seen + 1
    | Maybe.None -> ()
Context.cancel ctx
$"stopped after {seen}"
```

実行結果:

```text
stopped after 3
```

`Context.done` は呼ぶたびにチャンネルのハンドルを 1 つ作ります（drop で離します）。ループの中で呼ばず、ループの前に 1 回取ります。

## 期限

`Context.with_timeout parent d` は、今から `d` ナノ秒の期限を持つ子です。`Context.with_deadline parent t` の `t` は `Time.monotonic_ns` の時計の時刻です。親の期限のほうが早ければ、子の期限は親の期限です。過ぎた時刻なら、子はすぐに取り消されています。

```tsuzuri run=DeadlineExceeded
let root = Context.background ()
let ctx = Context.with_timeout root (30 * Time.millisecond())
let done = Context.done ctx
let deadline = Time.after Time.second()
select
| recv _ from done -> ()
| recv _ from deadline -> ()
match Context.err ctx with
| Maybe.Some why -> $"{why}"
| Maybe.None -> "running"
```

実行結果:

```text
DeadlineExceeded
```

## Context.after_func

`Context.after_func ctx f` は、`ctx` が取り消されたら所有する関数 `f`（[Owned](./owned.md)）を新しい tzroutine で 1 回呼びます（すでに取り消されていれば、すぐ）。`Context.stop_after` は、まだ呼んでいなければ止めて `true` を返します。戻り値の `Context.AfterFunc` を drop すると止まります。呼ばれないまま境界の終わりに来た関数は、そこで drop されます。

```tsuzuri run=called
let root = Context.background ()
let ctx = Context.with_cancel root
let called: Channel<string> = Channel.make 1
let out = Channel.share called
let _after = Context.after_func ctx (Owned.function (\() -> Channel.send out "called"))
Context.cancel ctx
match Channel.recv called with
| Maybe.Some text -> text
| Maybe.None -> "closed"
```

実行結果:

```text
called
```

## API リファレンス

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `Context.background` | `unit -> Context` | 取り消されない根（期限なし。`done` は閉じない） |
| `Context.with_cancel` | `ref Context -> Context` | 子。親か自分が取り消されると取り消される |
| `Context.with_timeout` | `ref Context -> i64 -> Context` | 今から `d` ナノ秒の期限を持つ子 |
| `Context.with_deadline` | `ref Context -> i64 -> Context` | 単調時計の時刻 `t` の期限を持つ子 |
| `Context.cancel` | `ref Context -> unit` | 自分と子孫を `Canceled` で取り消す（終わっていれば何もしない） |
| `Context.done` | `ref Context -> Channel<unit>` | 取り消されると閉じるチャンネル（何も送られない） |
| `Context.err` | `ref Context -> Maybe<Context.Error>` | 取り消されていれば理由 |
| `Context.is_done` | `ref Context -> bool` | `err` が `Some` か |
| `Context.deadline` | `ref Context -> Maybe<i64>` | 期限（単調時計の時刻） |
| `Context.share` | `ref Context -> Context` | 同じ `Context` のハンドルをもう 1 つ |
| `Context.after_func` | `ref Context -> Owned.Function<unit, unit> -> Context.AfterFunc` | 取り消されたら関数を新しい tzroutine で呼ぶ |
| `Context.stop_after` | `ref Context.AfterFunc -> bool` | まだ呼んでいなければ止めて `true` |

`Context.Error` は `Canceled | DeadlineExceeded` です（`Eq`・`Display`）。

## 引数の順序

`Context` は資源のハンドルの例外で、ハンドルが先頭の引数です（`Context.with_timeout ctx d`。Go の `context.WithTimeout(ctx, d)` と同じ）。[引数の順序](./argument-order.md)を見てください。

## 型とターゲット

- `Context` と `Context.AfterFunc` は非 Copy の不透明なハンドルです（構築・分解は `E1022`）。`Send`・`Sync` で、何度も呼べる関数値には捕捉できません（`E1005`）。
- `Context.with_timeout` と `Context.with_deadline` は時計を読むので、権限 `time` が要ります。ほかの関数は権限が要りません。
- `Context` の木は 1 つのロックで守られ、取り消しは再帰しません（10 万段の木も取り消せます）。期限のタイマーは、境界の終わりで止まります（[タイマー](./timer.md)と同じ規則）。止まった後も、ハンドルの `cancel`・`err`・`done` は使えます。
- ネイティブの macOS・Linux・Windows で動きます。`Context` は tzroutine のスケジューラーのタイマーを使うので、WebAssembly では `E2000` です。

## Go との対応

| Go | Tsuzuri |
| --- | --- |
| `context.Background()` | `Context.background ()` |
| `ctx, cancel := context.WithCancel(parent)` | `let ctx = Context.with_cancel parent`（`Context.cancel ctx` が `cancel()`） |
| `context.WithTimeout(parent, d)`・`WithDeadline(parent, t)` | `Context.with_timeout parent d`・`with_deadline parent t` |
| `ctx.Done()`・`ctx.Err()`・`ctx.Deadline()` | `Context.done ctx`・`err ctx`・`deadline ctx` |
| `context.AfterFunc(ctx, f)` | `Context.after_func ctx f` |
| `context.WithValue`・`WithoutCancel`・`WithCancelCause` | ありません（値は引数で渡します。独自の理由は [ErrGroup](./errgroup.md) の誤りの型で返します） |

## 関連項目

- [ErrGroup](./errgroup.md)
- [tzroutine](../async-tasks-and-lazy/tzroutine.md)
- [select](../async-tasks-and-lazy/select.md)
- [Timer](./timer.md)
- [言語リファレンスの目次](../index.md)
