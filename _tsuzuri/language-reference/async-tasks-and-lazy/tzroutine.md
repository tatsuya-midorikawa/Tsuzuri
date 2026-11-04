# tzroutine 式

`tzroutine { ... }` は、ブロックを新しい **tzroutine**（Go の goroutine に相当する、専用のスタックを持つ軽量な実行単位）として起動し、すぐに `()` を返す式です。起動した tzroutine は、少数の OS スレッドの上で、ほかの tzroutine と交互に、または並列に走ります。

`main`・`export` 関数の 1 回の呼び出し・`test` の 1 件・`bench` の 1 回の計測は、その中で起動した tzroutine（孫を含む）が**すべて終わるまで戻りません**。Go の「`main` が戻れば、ほかの goroutine を待たずに終わる」とは違います。

## この記事のポイント

- `tzroutine { 文 }` は捕捉を移して tzroutine を起動し、本体の終わりを待たずに `()` を返します。本体はちょうど 1 回走ります。 <!-- rule: TSK-011 -->
- 本体の型は `unit` です。ほかの型は `E1003` です。 <!-- rule: TSK-014 -->
- 本体は関数の本体と同じ通常のブロックです。`let!`・`do!` は IO をその場で実行します（`task { }` の bind とは違います）。 <!-- rule: TSK-015 -->
- 捕捉は所有値のムーブか Copy の複製で、`Send` でなければなりません。借用（`ref` など）を捕捉すると `E1013`、`Rc` も `E1013` です。 <!-- rule: TSK-013 -->
- 境界（`main`・`export` の呼び出し・`test`・`bench`）は、その中で起動した tzroutine がすべて終わるまで戻りません。 <!-- rule: TSK-012 -->
- ただし、トップレベルの最後の式や `export` の戻り値は、**待つ前に**計算されます。子の結果を読むときは、子の完了を数えて待ってから読みます。
- `Routine` モジュール（`go`・`yield`・`count`・`max_procs`・`cpu_count`）で、関数値の起動、実行の譲渡、数の問い合わせができます（[Routine](../built-in-types-and-modules/routine.md)）。
- tzroutine の間の値の受け渡しには [Channel](../built-in-types-and-modules/channel.md) を使います（Go の `chan T`）。
- 対応するのはネイティブの macOS・Linux・Windows（aarch64・x86-64）です。WebAssembly で tzroutine を起動するプログラムは `E2000` です。

## 基本の書き方

`tzroutine` の直後に、同じ行で `{` を書きます。次の例は、100 個の tzroutine がそれぞれ自分の番号を `Atomic` に足し、[WaitGroup](../built-in-types-and-modules/waitgroup.md) で全員の終わりを待ちます。

```tsuzuri run=sum%3D4950
let group = WaitGroup.create ()
let total = Arc.new (Atomic.create 0i64)
for i in 0 ..< 100 do
    WaitGroup.add (ref group) 1
    let finished = WaitGroup.share (ref group)
    let mine = Arc.share (ref total)
    tzroutine {
        let _sum = Atomic.fetch_add (Arc.get (ref mine)) (i as i64)
        WaitGroup.done (ref finished)
    }
WaitGroup.wait (ref group)
$"sum={Atomic.load (Arc.get (ref total))}"
```

実行結果:

```text
sum=4950
```

- 各 tzroutine は `Arc.share`・`WaitGroup.share` した自分のハンドル `mine`・`finished` を捕捉して持ち込みます（ムーブ）。`i` は Copy なので複製されます。
- 最後の式 `$"sum=..."` は、`main` が子を待つ**前に**計算されます。そこで `WaitGroup.wait` で 100 個の終わりを待ってから合計を読みます。待たずに読むと、結果が実行ごとに変わります。

次の例では、子が書いた行が、最後の式の値の表示より先に出ます。`main` は、最後の式を計算したあと、値を表示する前に、子の終了を待つからです。

```tsuzuri run=child%0A42
tzroutine {
    do! IO.write_line "child"
}
42
```

実行結果:

```text
child
42
```

`tzroutine` は予約語ではなく、直後が同じ行の `{` のときだけの文脈キーワードです。ほかの位置では普通の名前として使えます（`let tzroutine = 1`）。次の行の `{` は、`tzroutine` という名前の後に続く別のブロックです。

## 本体のブロック

本体は関数の本体と同じ通常のブロックです。

- `let`・`let mut`・`let rec`・`use`・`while`・`for`・`match`・`if` を書けます。文の区切りは改行か `;` です（`task { }` と同じく、`let` も改行で終わります）。
- `let!`・`do!` は、その場で IO を実行します。
- `return` は関数の本体と同じ扱いで、本体の型 `unit` を返すものだけが通ります（`return 1` は `E1003`）。
- 本体の中の `break`・`continue` で、外のループへは出られません（`E1023`）。
- `pure def` の中には書けません（`E1029`）。`const` の初期化にも書けません。

## 捕捉と所有権

本体が使う外の値は、`task { }` と同じ規則で捕捉されます。

| 規則 | 診断 |
| --- | --- |
| 所有値はムーブされ、外では使えなくなる | 外で続けて使うと `E1012` |
| Copy の値は複製される | — |
| 捕捉は `Send` でなければならない（`Rc`、Copy でない `dyn` などは不可） | `E1013` |
| 借用（`ref`、借用した関数の環境）は捕捉できない | `E1013` |
| 外の `let mut` には代入できない（捕捉は不変） | `E1014` |

```text
let x = 5
let r = ref x
tzroutine {
    let _y = deref r      // E1013: tasks require owned values; ref i32 contains a reference
}
```

```text
let s = "abc"
tzroutine {
    let _n = String.length (ref s)
}
String.length (ref s)     // E1012: use of moved or partially moved value 's'
```

複数の tzroutine で値を共有するときは、[Arc](../built-in-types-and-modules/rc.md) を tzroutine ごとに `Arc.share` して渡し、書き換える値は [Atomic](../built-in-types-and-modules/atomic.md) か [Mutex](../built-in-types-and-modules/mutex.md) に入れます。本体の中では、本体の値（持ち込んだ値を含む）を自由に `ref` で借用できます。本体が使わなかった捕捉は、本体の終わりに 1 回だけ drop されます。

## 終了の待ち

次の境界は、その中で起動した tzroutine（孫を含む）がすべて終わるまで戻りません。待つ間、境界のスレッドは眠ります。

| 境界 | いつ待つか |
| --- | --- |
| 実行ファイルの `main`（トップレベルのコード、`def main`、IO の入口） | 入口の関数が戻った直後。トップレベルの最後の式の値を表示する前 |
| `export def` の 1 回の呼び出し | 本体が戻った直後、ホストへ戻る前 |
| `test` の 1 件（`tsuzuri test`）、`bench` の 1 回の計測（`tsuzuri bench`） | その 1 件・1 回の本体が戻った直後 |

Go と違って待つのは、`Drop`（ファイルの flush など）を必ず走らせ、確保した値を残さないためです。

境界の途中で子の終わりを待つには [WaitGroup](../built-in-types-and-modules/waitgroup.md) を使います（Go の `sync.WaitGroup`）。待つのは呼んだ tzroutine だけです。

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

tzroutine を使うプログラムの `Mutex.with_lock` は tzroutine のロックで、ロックを持ったまま待てます（[Mutex](../built-in-types-and-modules/mutex.md#tzroutine-のプログラムのロック)）。ほかの同期の型は [Once](../built-in-types-and-modules/once.md)・[OnceValue](../built-in-types-and-modules/oncevalue.md)・[RwMutex](../built-in-types-and-modules/rwmutex.md)・[Cond](../built-in-types-and-modules/cond.md)・[Semaphore](../built-in-types-and-modules/semaphore.md)・[SyncMap](../built-in-types-and-modules/syncmap.md) です。

子の結果は、[Channel](../built-in-types-and-modules/channel.md) で受け取ると、数えて待つ必要がありません。受け取ること自体が待ち合わせです。

```tsuzuri run=total%3D30
let results: Channel<i64> = Channel.make 0
for i in 1 ..= 4 do
    let out = Channel.share results
    tzroutine {
        Channel.send out ((i as i64) * 3)
    }
let mut total = 0i64
for _i in 0 ..< 4 do
    total = total + (match Channel.recv results with
        | Maybe.Some value -> value
        | Maybe.None -> 0)
$"total={total}"
```

`Task.parallel` のタスク、`Task.scope` の子ども、`Parallel.*` のコールバックはスレッドプールのワーカーで走り、どの境界の中でもありません。そこで起動した tzroutine は、そのタスク（子ども、コールバックの 1 回）が終わる前に待たれます。したがって、それらの呼び出しが戻るのは、中で起動した tzroutine が終わった後です。

> **Go から来た人への注意**: 終わらない tzroutine（無限ループ、来ない値を待つもの）を残すと、境界が待ち続けます（止め方は次の節）。全員が待ちに入って誰も起こせなくなったときは、デッドロックの判定が次の文を標準エラーへ書いて、プロセスを終えます。
>
> ```text
> Tsuzuri runtime: all tzroutines are asleep - deadlock! (1 waiting: end of scope (waiting for tzroutines))
> ```

## 終わらない tzroutine を止める

境界は待つので、止まらない処理（サーバーのループ、周期の処理、遅い呼び出しの待ち）は、外から止める合図を受け取れるように書きます。標準の手段は [Context](../built-in-types-and-modules/context.md) です（Go の `context`）。tzroutine は `Context.done` のチャンネルを `select` で見て、閉じたら自分で戻ります。ランタイムが tzroutine を強制的に止めることはありません。

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

期限で止めるときは `Context.with_timeout` を使います。いくつもの tzroutine のうち 1 つが失敗したら残りを止めて最初の誤りを返す処理は [ErrGroup](../built-in-types-and-modules/errgroup.md) です。

## Routine.go

`Routine.go` は、所有する関数 `Owned.Function<unit, unit>` を新しい tzroutine で 1 回呼び、そのあと drop します。`tzroutine { }` を関数として受け渡したいときに使います。

```tsuzuri run=go%0A7
Routine.go (Owned.function (\() -> {
    do! IO.write_line "go"
}))
7
```

`Owned.function` の本体は捕捉を借りて呼ぶので、捕捉をムーブする処理は `tzroutine { }` で書きます。

## スケジューラー

tzroutine は M:N のスケジューラーで走ります。

- 同時に tzroutine を走らせる OS スレッドの数（P の数）は、既定でオンラインの CPU 数（256 まで）です。環境変数 `TSUZURI_MAXPROCS`（1〜256 の 10 進）で、起動時に 1 回だけ決められます。`Routine.max_procs ()` がその値です。
- 待つ tzroutine（`Routine.yield`・`Time.sleep`・`Time.sleep_ms`・境界の待ちなど）は OS スレッドを止めないので、待つ tzroutine の数はスレッド数に縛られません。周期の処理は [Ticker](../built-in-types-and-modules/ticker.md#周期の処理を止める)、遅らせた処理は [`Time.after_func`](../built-in-types-and-modules/time.md#遅らせて呼ぶtimeafter_func) で書きます。
- プリエンプションはまだありません。待たずに計算し続ける tzroutine は、その OS スレッドを離しません。長い計算では、ときどき `Routine.yield ()` を呼びます。`extern` 関数やファイルの I/O など、OS で待つ呼び出しも、その間 OS スレッドを止めます。
- tzroutine の中の `Task.run (Task.parallel …)`・`Task.scope`・`Parallel.*` は、終わりを待つ間、P を手放します。`TSUZURI_MAXPROCS=1` でも、ほかの tzroutine と、そのタスクの中で起動した tzroutine は走ります。待っている間に呼び出したスレッドで走るタスクは、tzroutine でないスレッドと同じく、そのスレッドを止めて待ちます。
- 各 tzroutine のスタックは、既定で 1 MiB の予約です（触れたページだけが実メモリになります）。下端の 128 KiB はガードで、溢れると `trap: stack overflow` と報告して終わります（Windows では理由なしで終わります）。tzroutine を起動するプログラムは、すべての関数が大きな frame のページを確保の時に順に触れる（stack probe）ので、ガードより大きな frame もガードを飛び越えません。深い再帰には `TSUZURI_TZROUTINE_STACK_KIB`（64〜1048576 の 10 進、KiB）で予約を増やします。範囲外の値は無視して既定値を使います。
- Linux では、1 つのスタックが mapping を 2 つ使うので、同時に生きる tzroutine はおよそ 3 万個（`vm.max_map_count` の既定 65,530 の半分）が上限です。超えると `Tsuzuri runtime: cannot allocate a tzroutine stack (out of memory or address space)` と書いて終わります。

## 失敗

- tzroutine の本体のトラップ（`assert`、範囲外の添字、0 での除算など）は、ほかのトラップと同じく、プロセス全体の失敗です。報告は、トラップした tzroutine の OS スレッドが標準エラーへ書きます（`--trap-info` なら位置を含みます）。
- `--trap-mode return` とは組み合わせられません（`E2000`）。ある tzroutine のトラップを、それを起動した呼び出しから返せないからです。
- tzroutine を起動するプログラムの `Mutex.with_lock` は tzroutine のロックなので、ロックの中で `Routine.yield` や待つ操作を呼べます（TZ06。TZ05 まではトラップでした）。

## ターゲット

| 構成 | 結果 |
| --- | --- |
| native の macOS・Linux・Windows（aarch64・x86-64） | 動く |
| wasm32・wasm64 | tzroutine を起動するプログラムは `E2000`（`tzroutine on WebAssembly needs --wasm-feature jspi`）。起動しないプログラムの `Channel` と `Routine` の問い合わせは 1 本の実行として動く（`Routine.count ()`・`max_procs ()`・`cpu_count ()` は 1、`Routine.yield ()` は何もしない） |
| wasm32 + `--wasm-feature jspi` | `E2000`（`tzroutine with --wasm-feature jspi is not implemented yet`） |
| `--wasm-feature threads` | `E2000`（`option_conflict`） |
| `--freestanding` | `E2000`（`--freestanding cannot use tzroutine: ...`） |
| `--trap-mode return` | `E2000`（`option_conflict`） |
| `--emit llvm`・`--emit header` | 拒否しない（ホストが `src/runtime/tzroutine.c` を連結する） |

## Task との違い

| | `tzroutine { }` | `task { }` |
| --- | --- | --- |
| 起動 | 評価した時点で起動する（ホット） | `Task.run` か `let!` まで始まらない（コールド） |
| 結果 | `unit`。値は `Atomic` などで受け渡す | `Task<T>` の結果 `T` |
| 待ち | 境界がまとめて待つ | 実行した呼び出しが待つ |
| 待つ数とスレッド | 待つ tzroutine は OS スレッドを止めない | 待つタスクはスレッドプールのスレッドを止める |
| 本体の `let!`・`do!` | IO をその場で実行する | 別のタスクを実行する |
| WebAssembly | `E2000` | 既定は逐次、threads はワーカー |

## 他の言語との比較

| 言語 | 書き方 | 終わりを待つか |
| --- | --- | --- |
| Tsuzuri | `tzroutine { f x }` | 境界が待つ |
| Go | `go f(x)` | `main` は待たない（`sync.WaitGroup` で待つ） |
| Rust | `std::thread::spawn(move \|\| f(x))` | `JoinHandle::join` で待つ（OS スレッド） |
| Kotlin | `scope.launch { f(x) }` | 構造化並行性: スコープが待つ |
| C# | `Task.Run(() => f(x))` | `await` か `Wait` で待つ |

Go の `go f(x)` は引数 `x` を呼び出し側で評価しますが、Tsuzuri の `tzroutine { f x }` では `x` の値は捕捉として起動時に移され、`f x` の式は子の中で評価されます。

## まとめ

- `tzroutine { ... }` は捕捉を移して tzroutine を起動し、`()` を返します。本体の型は `unit` です。
- 捕捉は `task { }` と同じく、所有値のムーブか Copy で、`Send` でなければなりません。借用は `E1013` です。
- `main`・`export` の呼び出し・`test`・`bench` は、その中で起動した tzroutine がすべて終わるまで戻りません。最後の式の値はその前に計算されます。
- P の数は `TSUZURI_MAXPROCS`、スタックの予約は `TSUZURI_TZROUTINE_STACK_KIB` で変えられます。
- WebAssembly、`--freestanding`、`--trap-mode return` とは組み合わせられません（`E2000`）。

## 関連項目

- [Routine](../built-in-types-and-modules/routine.md)
- [Task 式](task.md)
- [Atomic](../built-in-types-and-modules/atomic.md)
- [Mutex](../built-in-types-and-modules/mutex.md)
- [Rc と Arc](../built-in-types-and-modules/rc.md)
- [Owned](../built-in-types-and-modules/owned.md)
- [Context](../built-in-types-and-modules/context.md)
- [ErrGroup](../built-in-types-and-modules/errgroup.md)
- [所有権とムーブ](../ownership-and-memory/ownership.md)
- [言語仕様（tzroutine）](../../../docs/language.md#tzroutine)
- [言語リファレンスの目次](../index.md)
