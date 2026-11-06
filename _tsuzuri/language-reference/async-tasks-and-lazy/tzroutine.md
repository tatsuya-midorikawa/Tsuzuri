# tzroutine 式

`tzroutine { ... }` は、ブロックを新しい **tzroutine**（Go の goroutine に相当する、専用のスタックを持つ軽量な実行単位）として起動し、すぐに `()` を返す式です。起動した tzroutine は、少数の OS スレッドの上で、ほかの tzroutine と交互に、または並列に走ります。

`main`・`export` 関数の 1 回の呼び出し・`test` の 1 件・`bench` の 1 回の計測は、その中で起動した tzroutine（孫を含む）が**すべて終わるまで戻りません**。Go の「`main` が戻れば、ほかの goroutine を待たずに終わる」とは違います。

## この記事のポイント

- `tzroutine { 文 }` は捕捉を移して tzroutine を起動し、本体の終わりを待たずに `()` を返します。本体はちょうど 1 回走ります。 <!-- rule: TSK-011 -->
- 本体の型は `unit` です。ほかの型は `E1003` です。 <!-- rule: TSK-014 -->
- 本体は関数の本体と同じ通常のブロックです。`let!`・`do!` は IO をその場で実行します。 <!-- rule: TSK-015 -->
- 捕捉は所有値のムーブか Copy の複製で、`Send` でなければなりません。借用（`ref` など）を捕捉すると `E1013`、`Rc` も `E1013` です。 <!-- rule: TSK-013 -->
- 境界（`main`・`export` の呼び出し・`test`・`bench`）は、その中で起動した tzroutine がすべて終わるまで戻りません。 <!-- rule: TSK-012 -->
- ただし、トップレベルの最後の式や `export` の戻り値は、**待つ前に**計算されます。子の結果を読むときは、子の完了を数えて待ってから読みます。
- `Routine` モジュール（`go`・`yield`・`count`・`max_procs`・`cpu_count`・`with_locked_thread`）で、関数値の起動、実行の譲渡、数の問い合わせ、OS スレッドへの固定ができます（[Routine](../built-in-types-and-modules/routine.md)）。
- ネイティブでは、待たずに計算し続ける tzroutine も、10 ms ほどで P を譲ります（[横取り](#横取りプリエンプション)）。
- `extern` 関数・ファイル・ソケットなど、OS で待つ呼び出しの間も、ほかの tzroutine は走り続けます（[OS を待つ呼び出し](#os-を待つ呼び出し)）。
- tzroutine の間の値の受け渡しには [Channel](../built-in-types-and-modules/channel.md) を使います（Go の `chan T`）。
- `task { }`・型 `Task<T>`・`Task.run`・`Task.parallel`・`Task.parallel_results`・`Task.scope` は削除しました（`E1040`）。書き換え方は [Task からの移行](#task-からの移行) です。
- `Async { }`・`async { }`・型 `Async<T>`・`Async.*` と、`Net` の `_async` の関数も削除しました（`E1040`）。I/O を待ちながらほかの計算を進める仕事は tzroutine で書きます。書き換え方は [Async からの移行](#async-からの移行) です。
- 対応するのはネイティブの macOS・Linux・Windows（aarch64・x86-64）と、`--wasm-feature jspi` の wasm32 です。wasm32 では JavaScript のグルーが 1 本のスレッドの上で tzroutine を交代に走らせます（並行で、並列ではありません。[WebAssembly](#webassemblyjspi)）。

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

- `let`・`let mut`・`let rec`・`use`・`while`・`for`・`match`・`if` を書けます。文の区切りは改行か `;` です（`let` も改行で終わります）。
- `let!`・`do!` は、その場で IO を実行します。
- `return` は関数の本体と同じ扱いで、本体の型 `unit` を返すものだけが通ります（`return 1` は `E1003`）。
- 本体の中の `break`・`continue` で、外のループへは出られません（`E1023`）。
- `pure def` の中には書けません（`E1029`）。`const` の初期化にも書けません。

## 捕捉と所有権

本体が使う外の値は、次の規則で捕捉されます。

| 規則 | 診断 |
| --- | --- |
| 所有値はムーブされ、外では使えなくなる | 外で続けて使うと `E1012` |
| Copy の値は複製される | — |
| 捕捉は `Send` でなければならない（`Rc`、`Send` の印を持たない `dyn`（`dyn C`・`dyn (C, Copy)`）などは不可。`dyn (C, Send)` は Copy でなくても捕捉できる） | `E1013` |
| 借用（`ref`、借用した関数の環境）は捕捉できない | `E1013` |
| 外の `let mut` には代入できない（捕捉は不変） | `E1014` |

```text
let x = 5
let r = ref x
tzroutine {
    let _y = deref r      // E1013: values that move to another tzroutine or thread must be owned; ref i32 contains a reference
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

`Arc.share (ref shared)` の結果は新しい所有者で、`shared` の借用ではありません。そのため関数の `Arc` も、tzroutine ごとに `Arc.share` して渡せます。関数が借用を捕捉しているときは、その借用を `Arc.share` の結果も持つので、捕捉は `E1013` のままです。また、`shared` が関数の引数やラムダの捕捉（その値を受け取った局所値を含む）のときは、結果が `shared` の借用を持つので、その共有を捕捉する tzroutine も `E1013` です（[所有者を増やした値の借用](../built-in-types-and-modules/rc.md#所有者を増やした値の借用)）。次の例のように、tzroutine を起動する本文で `Arc.new` した値を共有します。

```tsuzuri run=4
let shared = Arc.new (\value -> value + 1)
let total = Arc.new (Atomic.create 0i64)
let mine = Arc.share (ref shared)
let counter = Arc.share (ref total)
tzroutine {
    let _old = Atomic.fetch_add (Arc.get (ref counter)) ((deref (Arc.get (ref mine))) 3)
}
while Atomic.load (Arc.get (ref total)) == 0 do
    Routine.yield ()
Atomic.load (Arc.get (ref total))
```

実行結果:

```text
4
```

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

ネイティブの `Mutex.with_lock` は tzroutine のランタイムのロックで、ロックを持ったまま待てます（[Mutex](../built-in-types-and-modules/mutex.md#ロックの規則)）。ほかの同期の型は [Once](../built-in-types-and-modules/once.md)・[OnceValue](../built-in-types-and-modules/oncevalue.md)・[RwMutex](../built-in-types-and-modules/rwmutex.md)・[Cond](../built-in-types-and-modules/cond.md)・[Semaphore](../built-in-types-and-modules/semaphore.md)・[SyncMap](../built-in-types-and-modules/syncmap.md) です。

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

[`Routine.scope`](../built-in-types-and-modules/routine.md#routinescope) は、子と、子が起動した tzroutine がすべて終わってから戻ります。`Parallel.*` のコールバックが起動した tzroutine は、`Parallel.*` の呼び出しを待たず、呼び出し元の境界（`main` など）が待ちます。`Parallel.*` の呼び出しが戻った時点では、まだ走っていることがあります。

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

`Owned.function` の本体は捕捉を借りて呼ぶので、捕捉をムーブする処理は `tzroutine { }` で書きます。 新しい tzroutine は関数を作った関数より長く生きることがあるので、借用を持つ関数値（`ref` を捕捉した関数値や関数の引数の関数値）からは `Owned.function` を作れません（`E1013`。[Owned](../built-in-types-and-modules/owned.md#捕捉できるのは所有値だけ)）。関数の引数で仕事を受け取るときは、引数の型を `own Owned.Function<unit, unit>` にします。

## スケジューラー

tzroutine は M:N のスケジューラーで走ります。

- 同時に tzroutine を走らせる OS スレッドの数（P の数）は、既定でオンラインの CPU 数（256 まで）です。環境変数 `TSUZURI_MAXPROCS`（1〜256 の 10 進）で、起動時に 1 回だけ決められます。`Routine.max_procs ()` がその値です。
- 待つ tzroutine（`Routine.yield`・`Time.sleep`・`Time.sleep_ms`・境界の待ちなど）は OS スレッドを止めないので、待つ tzroutine の数はスレッド数に縛られません。周期の処理は [Ticker](../built-in-types-and-modules/ticker.md#周期の処理を止める)、遅らせた処理は [`Time.after_func`](../built-in-types-and-modules/time.md#遅らせて呼ぶtimeafter_func) で書きます。
- ネイティブでは、待たずに計算し続ける tzroutine も、10 ms ほどで P をほかの tzroutine に譲ります（[横取り](#横取りプリエンプション)）。OS で待つ呼び出しは [OS を待つ呼び出し](#os-を待つ呼び出し) です。
- `Parallel.*` は、このスケジューラーの上で、呼び出し元と手伝いの tzroutine（呼び出し元が tzroutine なら最大 `min(チャンク数, P) - 1` 個、tzroutine でないスレッドなら最大 `min(チャンク数, P)` 個）がチャンクを分けます（[Parallel](../built-in-types-and-modules/parallel.md)）。`Routine.scope` は子ごとに tzroutine を起動します。どちらも、終わりを待つ tzroutine は P を手放すので、`TSUZURI_MAXPROCS=1` でも、ほかの tzroutine は走ります。
- 各 tzroutine のスタックは、既定で 1 MiB の予約です（触れたページだけが実メモリになります）。下端の 128 KiB はガードで、溢れると `trap: stack overflow` と報告して終わります（Windows では理由なしで終わります）。tzroutine を起動するプログラムは、すべての関数が大きな frame のページを確保の時に順に触れる（stack probe）ので、ガードより大きな frame もガードを飛び越えません。深い再帰には `TSUZURI_TZROUTINE_STACK_KIB`（64〜1048576 の 10 進、KiB）で予約を増やします。範囲外の値は無視して既定値を使います。
- Linux では、1 つのスタックが mapping を 2 つ使うので、同時に生きる tzroutine はおよそ 3 万個（`vm.max_map_count` の既定 65,530 の半分）が上限です。超えると `Tsuzuri runtime: cannot allocate a tzroutine stack (out of memory or address space)` と書いて終わります。

## 横取り（プリエンプション）

待たずに計算し続ける tzroutine も、同じ P を待つほかの tzroutine（チャンネルで起こされた受け手、タイマーで起きた tzroutine など）をいつまでも待たせません。スケジューラーの見張りのスレッド（sysmon）は、同じ tzroutine を 10 ms 以上走らせている P を見つけると、その tzroutine に P を譲るよう求めます。求められた tzroutine は、`Routine.yield ()` を呼んだときと同じく、全体の実行待ちの列の末尾へ回ります（Go の goroutine と同じ考え方です）。止まる位置が増えるだけで、値・評価の順序・所有権は変わりません。

- **検査（ネイティブのすべてのターゲット）**: tzroutine を起動するプログラムでは、生成コードが次の位置で求めを読みます。求めのない間の費用は、1 回の読み込みと予測の当たる分岐です。
  - 再帰する関数の、再帰の呼び出しの前（再帰しない道、たとえば `fib` の `n < 2` の道には入れません）
  - 重い呼び出し（ランタイム、`extern` 関数、標準入出力、再帰する関数、またはそれらを呼ぶ関数）を含むループの、後ろへの分岐
- 呼び出しのないループ、インライン展開で消える小さな関数とラムダだけを呼ぶループ（`Array.map (\x -> x * 2)` など）、確保（`Vec.push` など）だけを呼ぶループには検査を入れません。検査がループに入るとベクトル化と性能が失われるためです（[tzroutine の横取りの費用](../../../docs/benchmarks.md#tzroutine-の横取りの費用tz10)）。
- **信号（macOS と Linux の実行ファイル）**: 求めから 10 ms たっても検査に来ない tzroutine は、sysmon が送る `SIGURG` で止まり、P を譲ります。これで、呼び出しのない密なループも横取りされます。止めるのは生成コードの中だけで（コンパイラーは生成した関数を専用のセクションに置きます）、ランタイム・C ライブラリ・ホストの関数の中では止めません。止めた位置のすべてのレジスタ（浮動小数点とベクトルの状態、フラグを含む）を保存して元に戻すので、計算の結果は変わりません。実行ファイルは `SIGURG` のハンドラーを置き、Tsuzuri のものでない `SIGURG` は、それ以前のハンドラーに渡します（そのハンドラーの `sa_mask` で止める信号と、`SA_RESTART` の有無による割り込まれたシステムコールの扱いは、そのままです。`SA_NODEFER` なら `SIGURG` を止めずに呼び、`SA_RESETHAND` のハンドラーは Linux では 1 回だけ呼び、macOS では `sigaction` がその旗を返さないので毎回呼びます）。ただし、横取りした tzroutine を再開するごく短い間にそのスレッドへ届いたプログラムの `SIGURG` は、ランタイムが再開に使う `SIGURG` と 1 つにまとまり、それ以前のハンドラーへ渡らないことがあります（同じ信号が重なって届くと 1 つにまとまるのと同じです）。sysmon の信号は、ブロッキング呼び出し（`extern` 関数・ファイル・標準入出力）の中の tzroutine には送らないので、その中のシステムコールが `EINTR` で戻ることはありません。
- **横取りしない場所**: tzroutine でないスレッド（`main` のトップレベル、`export` を呼んだホストのスレッド）、`Domain.run` の中（トラップの枠がそのスレッドのものなので、その tzroutine はそのスレッドで続けます）。
- **信号を使わない構成**: Windows、`--emit object`・`--emit shared`・`--emit bitcode`（ホストのシグナルの設定を変えないため）、`--backend fast`、`tsuzuri test`・`tsuzuri bench` の実行器では検査だけです。そこでは呼び出しのない長いループは横取りされないので、長い計算の途中でときどき `Routine.yield ()` を呼びます。Linux の aarch64 では SVE のレジスタも保存して戻します（シグナルの frame に収まるベクトル長で、512 ビットまでは収まります）。収まらない長いベクトル長で SVE の状態が生きているスレッドは信号で止めずに次の求めを待ち、SME を持つ CPU（と、ランタイムの知らない状態の記録をシグナルの frame に持つ CPU）では信号を使わず検査だけです。WebAssembly（JSPI）の tzroutine は横取りしません。

次の例では、1 つの tzroutine が `fib 20` を回し続ける間に、もう 1 つの tzroutine が 10 ms ごとの `Ticker` を 5 回受け取り、終わりの合図を送ります。`TSUZURI_MAXPROCS=1` で P が 1 つだけでも、横取りがあるので同じ出力です。

```tsuzuri run=ticks%3D5
def rec fib :: i64 -> i64
fn rec fib n = if n < 2 then n else fib (n - 1) + fib (n - 2)

let stop = Arc.new (Atomic.create 0i64)
let received = Arc.new (Atomic.create 0i64)
let busy_stop = Arc.share (ref stop)
tzroutine {
    let mut total = 0i64
    while Atomic.load (Arc.get (ref busy_stop)) == 0i64 do
        total = total + fib 20
}
let ticker_stop = Arc.share (ref stop)
let mine = Arc.share (ref received)
tzroutine {
    let ticker = Ticker.create (10 * Time.millisecond())
    let ticks = Ticker.channel ticker
    for _ in 1 ..= 5 do
        match Channel.recv ticks with
        | Maybe.Some _ ->
            let _n = Atomic.fetch_add (Arc.get (ref mine)) 1i64
        | Maybe.None -> ()
    Ticker.stop ticker
    Atomic.store (Arc.get (ref ticker_stop)) 1i64
}
while Atomic.load (Arc.get (ref stop)) == 0i64 do
    Routine.yield ()
$"ticks={Atomic.load (Arc.get (ref received))}"
```

## OS を待つ呼び出し

tzroutine が OS を待つ呼び出しに入っても、ほかの tzroutine は止まりません（Go と同じ仕組みです）。 <!-- rule: TSK-031 -->

- **P の引き継ぎ**: 次の呼び出しの間、その tzroutine の OS スレッドは P を手放せる状態になります。スケジューラーの見張りのスレッド（sysmon）が、同じ呼び出しが 20 µs ほど続いているのを見つけると、P を取り上げて別の OS スレッドに渡し、待っている tzroutine を走らせます。呼び出しから戻った tzroutine は、P を取り戻せればそのまま続け、取り戻せなければ空いた P を待ちます。短い呼び出しの費用は、印の 2 回の atomic な書き込みです。
  - `extern` 関数の呼び出し（すべて。どの関数が待つかを型から知れないため）
  - `File`・`Dir`・`Process.run`・`Random` の OS の呼び出し、標準入出力（`IO.read_line`・`IO.write_line` など）
  - `Net.resolve`（名前解決）、ソケットを閉じる呼び出し
  - GPU の装置の呼び出し（`Gpu.request`・カーネルの実行）
- **ソケットの待ち**: `Net.accept`・`Net.read`・`Net.write`・`Net.connect` などがソケットの準備を待つときは、その tzroutine だけが待ちます（netpoller。macOS は kqueue、Linux は epoll、Windows は `WSAPoll` の 1 本のスレッド）。OS スレッドを止めないので、接続ごとに tzroutine を起動するサーバーを、`TSUZURI_MAXPROCS=1` でも書けます。時間切れ（`Maybe.Some ms`）と、待っているソケットを閉じたときの扱いは、tzroutine でないスレッドと同じです（[Net](../built-in-types-and-modules/net.md#tzroutine-の中の-net)）。
- **デッドロックの判定**: OS を待つ呼び出しの中の tzroutine と、ソケットを待つ tzroutine は、外から起きうるので、ほかが全員眠っていてもデッドロックとは判定しません。
- **コールバック**: `extern` 関数が Tsuzuri の関数を呼び返す（[静的コールバック](../compiler/native-interop.md#静的コールバック)）か、呼び出しの中でホストが `export` 関数を呼ぶと、その間は P を取り戻して普通の tzroutine として走ります。ホストの C の frame がそのスレッドのスタックにあるので、その中で待っても、tzroutine はその OS スレッドで続きます（[`Routine.with_locked_thread`](../built-in-types-and-modules/routine.md#routinewith_locked_thread) と同じ固定）。
- tzroutine でないスレッド（`main` のトップレベル、`export` を呼んだホストのスレッド）は、今までどおり、そのスレッドで待ちます。

次の例では、ホストの C 関数 `host_wait_ms` が 300 ms 待つ間に、ほかの 2 つの tzroutine が 1,000 回往復し、`rounds=1000 slept` と出力します（`TSUZURI_MAXPROCS=1` でも同じです。ホストを `--link` でリンクします）。

```tsuzuri
extern def host_wait_ms :: i64 -> unit

let order: Channel<string> = Channel.make 2
let started: Channel<unit> = Channel.make 1
let sleeper_started = Channel.share started
let sleeper_order = Channel.share order
tzroutine {
    Channel.send sleeper_started ()
    host_wait_ms 300
    Channel.send sleeper_order "slept"
}
let requests: Channel<i64> = Channel.make 0
let replies: Channel<i64> = Channel.make 0
let server_requests = Channel.share requests
let server_replies = Channel.share replies
tzroutine {
    for value in server_requests do
        Channel.send server_replies (value + 1)
}
let pinger_started = Channel.share started
let pinger_order = Channel.share order
tzroutine {
    let _go = Channel.recv pinger_started
    let mut value = 0i64
    for _round in 0 ..< 1000 do
        Channel.send requests value
        value = match Channel.recv replies with
            | Maybe.Some next -> next
            | Maybe.None -> -1
    Channel.close requests
    Channel.send pinger_order $"rounds={value}"
}
let first = match Channel.recv order with
    | Maybe.Some text -> text
    | Maybe.None -> "none"
let second = match Channel.recv order with
    | Maybe.Some text -> text
    | Maybe.None -> "none"
$"{first} {second}"
```

## 失敗

- tzroutine の本体のトラップ（`assert`、範囲外の添字、0 での除算など）は、ほかのトラップと同じく、プロセス全体の失敗です。報告は、トラップした tzroutine の OS スレッドが標準エラーへ書きます（`--trap-info` なら位置を含みます）。
- `--trap-mode return` とは組み合わせられません（`E2000`）。ある tzroutine のトラップを、それを起動した呼び出しから返せないからです。
- ネイティブの `Mutex.with_lock` は tzroutine のランタイムのロックなので、ロックの中で `Routine.yield` や待つ操作を呼べます。同じ `Mutex` をロックの中でもう一度取るとトラップします（`Mutex.with_lock cannot be nested`）。

## ターゲット

| 構成 | 結果 |
| --- | --- |
| native の macOS・Linux・Windows（aarch64・x86-64） | 動く |
| wasm32 + `--wasm-feature jspi`（`wasm`・`object`・`llvm`・`bindings-js`） | 動く。生成したグルーが JSPI で tzroutine を交代に走らせる（[WebAssembly](#webassemblyjspi)） |
| wasm32・wasm64（`jspi` なし） | tzroutine を起動するプログラムは `E2000`（`tzroutine on WebAssembly needs --wasm-feature jspi`）。起動しないプログラムの `Channel` と `Routine` の問い合わせは 1 本の実行として動く（`Routine.count ()`・`max_procs ()`・`cpu_count ()` は 1、`Routine.yield ()` は何もしない） |
| wasm64 + `--wasm-feature jspi` | `E2000`（`tzroutine with --wasm-feature jspi requires --target wasm32: ...`。グルーは wasm32 だけ） |
| `--wasm-feature webgpu` と `jspi` | `E2000`（`tzroutine cannot be combined with --wasm-feature webgpu: ...`） |
| `--wasm-feature threads` | `E2000`（`option_conflict`） |
| `tsuzuri test --target wasm32`・`wasm64` | `E2000`（`the WebAssembly test runner cannot run tzroutines: ...`。テストの実行器は JSPI を回さない） |
| `--freestanding` | `E2000`（`--freestanding cannot use tzroutine: ...`） |
| `--trap-mode return` | `E2000`（`option_conflict`） |
| `--emit llvm`・`--emit header` | 拒否しない（ホストが `src/runtime/tzroutine.c` を連結する） |

## WebAssembly（JSPI）

wasm32 で tzroutine を起動するプログラムは、`.wasm` とグルー（`--emit bindings-js`）の両方に `--wasm-feature jspi` を付けてビルドします。tzroutine は JavaScript の 1 本のスレッドの上で、1 つずつ交代に走ります。tzroutine が待つ（`Channel`・`select`・同期の型・`Time.sleep`・タイマー・`--wasm-feature net` のソケット）と、JSPI（JavaScript Promise Integration）でその tzroutine の WebAssembly のスタックだけを中断し、グルーが次に走れる tzroutine を再開します。

```tsuzuri
export def sum :: i64 -> string
fn sum n =
    let jobs: Channel<i64> = Channel.make 0
    let producer = Channel.share jobs
    tzroutine {
        for i in 1 ..= n do
            Channel.send producer i
        Channel.close producer
    }
    let mut total = 0i64
    for job in jobs do
        total = total + job
    $"sum={total}"
```

```sh
tsuzuri build app --target wasm32 --wasm-feature jspi --emit bindings-js -o app.mjs
tsuzuri build app --target wasm32 --wasm-feature jspi -O3 -o app.wasm
```

```js
import { readFileSync } from "node:fs";
import { load } from "./app.mjs";

const bindings = await load(readFileSync("app.wasm"));
console.log(await bindings.exports.sum(100n)); // sum=5050
```

- export の呼び出しが境界で、その中で起動した tzroutine がすべて終わってから `Promise` が解決します。呼び出しは直列化され、前の呼び出しの tzroutine が終わるまで次の呼び出しは始まりません。
- 走るのはいつも 1 つで、ほかの tzroutine に替わるのは待つ点か `Routine.yield ()` だけです（プリエンプションはありません）。`Routine.max_procs ()` と `Routine.cpu_count ()` は 1 です。`Parallel.*` はその tzroutine の上で添字の順に逐次に走ります。
- tzroutine が待つ間、ホストのイベントループは止まりません。待つ点や `Routine.yield ()` で替わりながら tzroutine が走り続けても、グルーは数ミリ秒ごとにホストのタイマーと I/O に順番を渡します。ただし、グルーに戻るのは tzroutine が止まったときだけです。待たずに計算し続ける tzroutine は、止まるまで JavaScript のスレッドを持ち続けます。長い計算には、ときどき `Routine.yield ()` を入れます。タイマー（`Time.sleep`・`Time.after`・`Timer` など）はグルーの `setTimeout` で期限に発火します。
- tzroutine のスタック（線形メモリ）は 64 KiB の固定です。関数の入口で検査し、溢れるとトラップします（隣の tzroutine のスタックやヒープを壊しません。`--trap-info` なら種類は `stack overflow`）。スタックは tzroutine が最初に走るときに取り、終わった tzroutine のものを使い回すので、メモリを使うのは始まっていて終わっていない tzroutine だけです。既定の 16 MiB のメモリには、同時に約 200 個が入ります。それより多くを同時に待たせるときは `--wasm-max-memory` を増やします。
- `-O0` では、128 bit の浮動小数点数・十進数（`f128`・`d128`）のソフトウェア演算の frame が 60 KiB 近くになり、tzroutine の中ではスタックが足りずにトラップします。tzroutine の中でそれらを計算するときは `-O1` 以上でビルドします。
- すべての tzroutine が待ち、タイマーもソケットの待ちも残っていないと、export の `Promise` が `RuntimeError("all tzroutines are asleep - deadlock!")` で拒否され、インスタンスは捨てられます（次の呼び出しは新しいインスタンスで動きます）。ほかのトラップは、ネイティブの報告の文を出さずに `TsuzuriTrap` で拒否します。
- `extern` 関数のコールバックの中では待てません（JSPI は JavaScript の frame をまたいで中断できず、トラップします）。
- import と export、エンジンの要件は [WebAssembly の JSPI の tzroutine](../compiler/webassembly.md#jspi-の-tzroutine) です。

## Task からの移行

`task { … }`、型 `Task<T>`、`Task.run`・`Task.parallel`・`Task.parallel_results`・`Task.scope` は、言語から削除しました（TZ12）。並行の仕事は tzroutine で、配列のデータ並列は [Parallel](../built-in-types-and-modules/parallel.md) で、借用を共有する子は [Routine.scope](../built-in-types-and-modules/routine.md#routinescope) で、最初の誤りで残りを止める仕事は [ErrGroup](../built-in-types-and-modules/errgroup.md) で書きます。

削除した形を書くと `E1040` です。診断の `help` が書き換え先を示します。

| 書いたもの | kind | メッセージ | help（書き換え先） |
| --- | --- | --- | --- |
| `task { … }` | `removed_task_expression` | `'task { }' was removed` | 仕事をその場で実行するか、`tzroutine { ... }` で起動して `WaitGroup` か `Channel` で待ちます（`run the work directly, or spawn it with 'tzroutine { ... }' and wait with WaitGroup or a Channel`） |
| 型 `Task<T>` | `removed_task_type` | `the type 'Task' was removed` | 値をそのまま返します。並行に走らせるなら `tzroutine { ... }` を使います（`return the value directly; to run work concurrently, use 'tzroutine { ... }'`） |
| `Task.run` | `removed_task_api` | `'Task.run' was removed` | 仕事を直接呼びます（`call the work directly`） |
| `Task.parallel` | `removed_task_api` | `'Task.parallel' was removed` | データ並列は `Parallel.init` か `Parallel.map`、それ以外は tzroutine と `WaitGroup` です（`use Parallel.init or Parallel.map for data parallelism, or tzroutines with a WaitGroup`） |
| `Task.parallel_results` | `removed_task_api` | `'Task.parallel_results' was removed` | `ErrGroup` です。仕事ごとに `ErrGroup.go` を呼び、最初の誤りを `ErrGroup.wait` で受け取ります（`use ErrGroup: ErrGroup.go for each job and ErrGroup.wait for the first error`） |
| `Task.scope` | `removed_task_api` | `'Task.scope' was removed` | 同じ引数をとる `Routine.scope` です（`use Routine.scope, which takes the same arguments`） |

- `tsuzuri fix` が自動で書き換えるのは、`Task.scope` を `Routine.scope` にする 1 つだけです（確かな修正案）。ほかの形は、書き換えると始まる時点、結果の受け取り方、選ばれる誤りが変わるので、次の例を見て手で直します。`tsuzuri fix --dry-run <ファイルかディレクトリ>` で、書き換えの差分だけを表示できます（[fix](../compiler/usage.md#fix)）。
- `task` は予約語ではなくなり、普通の名前です（`let task = 1`）。`.tc` に `@alias task` を書いた利用者のビルダーがあれば、`task { … }` はそのビルダーの式です（[ビルダーの別名](../computation-expressions/computation-expressions.md#ビルダーの別名)）。
- 利用者が定義した `Task` という名前のモジュール・型・名前空間は、そのまま使えます。`E1040` になるのは、その名前が利用者の定義に解決しないときだけです。

### Task.run を外す

`Task.run (task { … })` は、本体をその場で実行して終わりを待つ同期の呼び出しでした。本体をそのまま書くのと同じです。

書き換える前（今は `E1040`）:

```text
let answer = Task.run (task {
    let base = 40
    return base + 2
})
$"answer={answer}"
```

書き換えた後:

```tsuzuri run=answer%3D42
def compute :: i64 -> i64
fn compute base = base + 2

let answer = compute 40
$"answer={answer}"
```

実行結果:

```text
answer=42
```

作っておいて後で走らせるためのタスク（コールドな遅延）は、`unit -> T` の関数（サンク）にします（[Lazy 式](lazy.md)）。別の実行単位で走らせたいときは `tzroutine { }` で起動し、結果を `Channel` で受け取ります（[終了の待ち](#終了の待ち)）。

### Task.parallel を Parallel か tzroutine にする

同じ形の仕事を添字ごとに分けるなら、`Parallel.init n (\i -> …)` です。結果は添字の順で、`Task.parallel` の結果の並びと同じです。

書き換える前（今は `E1040`）:

```text
let jobs = new [Task<i64>](4, \index -> task { return (index + 1) * 10 })
let results = Task.run (Task.parallel jobs)
$"{results[0]},{results[1]},{results[2]},{results[3]}"
```

書き換えた後:

```tsuzuri run=10%2C20%2C30%2C40
let results = Parallel.init 4 (\index -> (index + 1) * 10)
$"{results[0]},{results[1]},{results[2]},{results[3]}"
```

実行結果:

```text
10,20,30,40
```

仕事が待つ操作（`Channel`、`Time.sleep` など）を含むときや、仕事ごとに中身が違うときは、tzroutine を起動して [WaitGroup](../built-in-types-and-modules/waitgroup.md) で終わりを待ちます。結果は `Arc<Mutex<[T]>>`（または `Mutex<Vec<T>>`）に書くか、[Channel](../built-in-types-and-modules/channel.md) で受け取ります。次の例は、各 tzroutine が自分の添字の要素を書くので、結果は添字の順です。

```tsuzuri run=10%2C20%2C30%2C40
def work :: i64 -> i64
fn work index = (index + 1) * 10

let results = Arc.new (Mutex.create (new [i64](4, \_index -> 0)))
let group = WaitGroup.create ()
for index in 0 ..< 4 do
    WaitGroup.add (ref group) 1
    let finished = WaitGroup.share (ref group)
    let mine = Arc.share (ref results)
    tzroutine {
        let value = work index
        Mutex.with_lock (Arc.get (ref mine)) (\values -> Array.write index value values)
        WaitGroup.done (ref finished)
    }
WaitGroup.wait (ref group)
Mutex.with_lock (Arc.get (ref results)) (\values -> $"{values[0]},{values[1]},{values[2]},{values[3]}")
```

実行結果:

```text
10,20,30,40
```

- `Task.parallel` のタスクは `Task.run` まで始まりませんでしたが、tzroutine は `tzroutine { }` を評価した時点で始まります。
- 外の値を借用のまま読む子と、添字の順の結果が欲しいときは、`Routine.scope` も使えます（[Task.scope を Routine.scope にする](#taskscope-を-routinescope-にする)）。

### Task.parallel_results を ErrGroup にする

`Result` を返す仕事を並べ、最初の誤りで残りを止めるなら [ErrGroup](../built-in-types-and-modules/errgroup.md) です。仕事ごとに `ErrGroup.go` で関数を起動し、`ErrGroup.wait` で全員の終わりを待って、最初の誤りか `Ok` を受け取ります。

書き換える前（今は `E1040`）:

```text
def job :: i64 -> Task<Result<i64, string>> = \value -> task {
    if value == 2 { return Result.Error "stopped" }
    else { return Result.Ok (value * 2) }
}

let jobs = new [Task<Result<i64, string>>](4, \index -> job index)
match Task.run (Task.parallel_results jobs) with
| Result.Ok values -> $"sum={Array.sum ref values}"
| Result.Error error -> $"error={error}"
```

書き換えた後:

```tsuzuri run=error%3Dstopped
def job :: i64 -> Result<i64, string>
fn job value =
    if value == 2 then Result.Error "stopped" else Result.Ok (value * 2)

let group: ErrGroup<string> = ErrGroup.create ()
let total = Arc.new (Atomic.create 0i64)
for i in 0 ..< 4 do
    let mine = Arc.share (ref total)
    ErrGroup.go group (Owned.function (\() ->
        match job i with
        | Result.Ok value -> {
            let _old = Atomic.fetch_add (Arc.get (ref mine)) value;
            Result.Ok ()
        }
        | Result.Error error -> Result.Error error))
match ErrGroup.wait group with
| Result.Ok _ -> $"sum={Atomic.load (Arc.get (ref total))}"
| Result.Error error -> $"error={error}"
```

実行結果:

```text
error=stopped
```

- `ErrGroup.wait` が返すのは、**終わった時刻の順で最初**の誤りです。`Task.parallel_results` の「入力の添字が最小の誤り」とは違います。誤りを返す仕事が 2 つ以上あると、どれが選ばれるかは実行ごとに変わりえます。
- `Task.parallel_results` は、誤りのあと未開始のタスクを始めませんでした。`ErrGroup.go` は呼んだ時点で関数を起動します。最初の誤りはグループの `Context`（`ErrGroup.context`）を取り消すので、長い仕事はそれを見て自分で止まります（[ErrGroup](../built-in-types-and-modules/errgroup.md#基本の書き方)）。
- 関数の型は `Owned.Function<unit, Result<unit, 'e>>` で、`Ok` の値は返せません。値は `Atomic`・`Mutex`・`Channel` で受け渡します。
- 添字が最小の誤りが要るときは、`Parallel.init` で `Result` の配列を作り、添字の順に調べます（[並列処理の失敗](../built-in-types-and-modules/result.md#並列処理の失敗)）。この形は、誤りがあっても全件を走らせます。

### Task.scope を Routine.scope にする

`Routine.scope` は、`Task.scope` と同じ引数と型（`(Sync<'s>, Send<'a>) => ref 's -> i64 -> (ref 's -> i64 -> 'a) -> ['a]`）をとります。`tsuzuri fix` が名前を書き換えます。

書き換える前（今は `E1040`。`tsuzuri fix` で直せます）:

```text
let hits = Atomic.create 0i64
let _seen = Task.scope (ref hits) 8 (\shared index -> Atomic.fetch_add shared (index + 1))
Atomic.load (ref hits)
```

書き換えた後:

```tsuzuri run=36
let hits = Atomic.create 0i64
let _seen = Routine.scope (ref hits) 8 (\shared index -> Atomic.fetch_add shared (index + 1))
Atomic.load (ref hits)
```

実行結果:

```text
36
```

- 子はそれぞれ自分の tzroutine で走るので、互いを `Channel` などで待てます。`TSUZURI_MAXPROCS=1` でも終わります（[Routine.scope](../built-in-types-and-modules/routine.md#routinescope)）。呼び出しは、子が起動した tzroutine の終わりも待ちます。
- `shared` は `Sync`、結果は `Send` の所有値、コールバックは借用した環境を持てない、直接の呼び出しだけ、という規則は `Task.scope` と同じです。
- tzroutine を起動するので、`--wasm-feature jspi` のない WebAssembly、`--freestanding`、`--trap-mode return` では `E2000`、`pure def` の中では `E1029` です。これらで `Task.scope` を使っていた配列の計算は、`Parallel.*` に書き換えます。

## Async からの移行

`Async { … }`・`async { … }`、型 `Async<T>`・`Async.Operation`、`Async.yield`・`sleep`・`now`・`all`・`all_results`・`run`・`block_on`・`start`・`host` と、`Net` の `_async` の 12 関数（`connect_async`・`accept_async`・`read_async`・`write_async`・`recv_from_async`・`send_to_async`・`bind_async`・`bind_udp_async`・`close_async`・`close_listener_async`・`close_udp_async`・`shutdown_async`）は、言語から削除しました（TZ13）。I/O を待ちながらほかの計算を進める仕事は、tzroutine で書きます。tzroutine の中の `Net` の操作、`Time.sleep`、`Channel` の待ちは、その tzroutine だけを待たせ、ほかの tzroutine は走り続けます（[OS を待つ呼び出し](#os-を待つ呼び出し)）。

削除した形を書くと `E1040` です。診断の `help` が書き換え先を示します。

| 書いたもの | kind | メッセージ | help（書き換え先） |
| --- | --- | --- | --- |
| `Async { … }`・`async { … }` | `removed_async` | `'Async { }' was removed`・`'async { }' was removed` | 仕事をその場で実行します。ほかの仕事を進めながら I/O を待つなら、tzroutine を起動します（`run the work directly; to wait for I/O while other work runs, spawn tzroutines`） |
| 型 `Async<T>`・`Async.Operation`（`Async.Step`・`Async.Next` も） | `removed_async` | `the type 'Async' was removed`・`the type 'Async.Operation' was removed` | 値をそのまま返すか、あとで届けるなら `Channel` を使います（`return the value directly, or use a Channel to deliver it later`） |
| `Async.run`・`Async.block_on` | `removed_async` | `'Async.run' was removed` など | 仕事を直接呼びます。tzroutine の中なら、I/O の待ちはほかの tzroutine を止めません（`call the work directly (in a tzroutine, I/O waits without stopping other tzroutines)`） |
| `Async.all`・`Async.all_results` | `removed_async` | `'Async.all' was removed` など | 仕事ごとに tzroutine を起動して `WaitGroup` で待つか、最初の誤りを受け取るなら `ErrGroup` で待ちます（`spawn a tzroutine per job and wait with WaitGroup, or with ErrGroup for the first error`） |
| `Async.sleep`・`Async.now`・`Async.yield` | `removed_async` | `'Async.sleep' was removed` など | `Time.sleep`・`Time.monotonic_ns`・`Routine.yield` を使います（`use Time.sleep, Time.monotonic_ns, or Routine.yield`） |
| `Async.start`・`Async.host` | `removed_async` | `'Async.start' was removed` など | ホストが駆動する実行器は削除しました。ネイティブではホストのスレッドから export を呼び、WebAssembly（`--wasm-feature jspi`）では export の `Promise` を待ちます（`the host-driven executor was removed; call exports from host threads (native) or await the export's Promise (WebAssembly with --wasm-feature jspi)`） |
| `Net.connect_async` などの 12 関数 | `removed_net_async` | `'Net.read_async' was removed` など | `_async` の付かない `Net.connect` などを tzroutine の中で呼びます。待つ間もほかの tzroutine は止まりません（`call Net.connect (and the others without '_async') in a tzroutine; it waits without stopping other tzroutines`） |

- ビルダーの操作（`Async.Return`・`Async.Bind` など）を名前で呼んでも、`Async { }` と同じ help の `E1040` です。
- `tsuzuri fix` は、どの形も自動では書き換えません。意味の同じ書き換え先がないからです。次の例を見て手で直します。
- `async` と `Async` は普通の名前です（`let async = 2`）。`.tc` に `@alias async` を書いた利用者のビルダーがあれば、`async { … }` はそのビルダーの式です。利用者の `Async.tc` のビルダー、`Async` という名前のモジュール・型も、そのまま使えます。`E1040` になるのは、その名前が利用者の定義に解決しないときだけです（[ビルダーの別名](../computation-expressions/computation-expressions.md#ビルダーの別名)）。
- `Async.run` の仮想時刻（呼び出しごとに 0 から始まり、すべての計算が眠ったときだけ進む、決定的な時計）には後継がありません（TZ13 の D3）。時刻に依存する処理を決定的に試すときは、時刻を関数の引数で渡すか、時刻を `Channel` で届けます。
- ホストのイベントループに計算を預ける実行器（`Async.start`・`Async.host`、C の `tsuzuri_async_poll`・`tsuzuri_async_complete`・`tsuzuri_async_post`、WebAssembly の `tsuzuri_async_set_epoch` とグルーの `bindings.async`）にも後継がありません（TZ13 の D2。[Async.start とホストの poll](#asyncstart-とホストの-poll)）。
- 時間の単位が変わります。`Async.sleep` と `Async.now` は実行器の単位（`Async.block_on` ではミリ秒）でしたが、`Time.sleep` と `Time.monotonic_ns` はナノ秒です。ミリ秒で書くときは `n * Time.millisecond()` です。
- `Async` の計算はコールド（作っただけでは動かない）で、1 本のスレッドの上で交代に進みました。tzroutine は `tzroutine { }` を評価した時点で始まり、ほかの tzroutine と並列に走ります。共有する値は `Atomic`・`Mutex`・`Channel` で守ります。

### Async.all を tzroutine と WaitGroup にする

`Async.run (Async.all [a, b])` は、2 つの計算を交代に進めて、結果を入力の順の配列で返しました。tzroutine では、仕事ごとに 1 つ起動して [WaitGroup](../built-in-types-and-modules/waitgroup.md) で終わりを待ちます。結果は `Arc<Mutex<[T]>>` の自分の添字に書くか、[Channel](../built-in-types-and-modules/channel.md) で受け取ります。

書き換える前（今は `E1040`）:

```text
def fetch :: i64 -> Async<i64>
fn fetch id = Async {
    do! Async.sleep (id * 10)
    return id * 100
}

let results = Async.run (Async.all [fetch 1, fetch 2])
$"{results[0]},{results[1]}"
```

書き換えた後:

```tsuzuri run=100%2C200
def fetch :: i64 -> i64
fn fetch id =
    Time.sleep (id * 10 * Time.millisecond())
    id * 100

let results = Arc.new (Mutex.create (new [i64](2, \_index -> 0)))
let group = WaitGroup.create ()
WaitGroup.add (ref group) 2
let first_done = WaitGroup.share (ref group)
let first = Arc.share (ref results)
tzroutine {
    let value = fetch 1
    Mutex.with_lock (Arc.get (ref first)) (\values -> Array.write 0 value values)
    WaitGroup.done (ref first_done)
}
let second_done = WaitGroup.share (ref group)
let second = Arc.share (ref results)
tzroutine {
    let value = fetch 2
    Mutex.with_lock (Arc.get (ref second)) (\values -> Array.write 1 value values)
    WaitGroup.done (ref second_done)
}
WaitGroup.wait (ref group)
Mutex.with_lock (Arc.get (ref results)) (\values -> $"{values[0]},{values[1]}")
```

実行結果:

```text
100,200
```

- 2 つの `fetch` は同時に眠るので、全体の待ち時間は長いほうの 20 ms ほどです（`Async.all` と同じく、待ち時間が重なります）。
- 各 tzroutine が自分の添字に書くので、結果は入力の順です。結果を `Channel` で受け取るなら `WaitGroup` は要りません。受け取る数だけ `Channel.recv` を呼びます（[終了の待ち](#終了の待ち)）。その場合の順は、終わった順です。

### Async.sleep を Time.sleep にする

`Async.sleep` は計算を中断して、実行器の時計を待ちました。[`Time.sleep`](../built-in-types-and-modules/time.md#tzroutine-の中で待つ) は、tzroutine の中ではその tzroutine だけを、`main` などのスレッドではそのスレッドを、少なくとも指定のナノ秒だけ待たせます。`Async.now` は `Time.monotonic_ns`、`Async.yield` は `Routine.yield` です。

書き換える前（今は `E1040`）:

```text
def main :: unit -> i32 = \() ->
    let! waited = Async.block_on (Async {
        let! start = Async.now ()
        do! Async.sleep 10
        let! finish = Async.now ()
        return finish - start
    })
    do! IO.write_line (if waited >= 10 then "slept" else "early")
    0
```

書き換えた後:

```tsuzuri run=slept
def main :: unit -> i32 = \() ->
    let! start = Time.monotonic_ns ()
    Time.sleep (10 * Time.millisecond())
    let! finish = Time.monotonic_ns ()
    let waited = Result.get finish - Result.get start
    do! IO.write_line (if waited >= 10 * Time.millisecond() then "slept" else "early")
    0
```

実行結果:

```text
slept
```

`Time.monotonic_ns` は `IO<Result<i64, Os.Error>>` で、差だけに意味があります。`Async.sleep` の 10 は `Async.block_on` ではミリ秒だったので、`10 * Time.millisecond()` にします。

### タイムアウトを select と Time.after にする

`Async` では、期限で計算を止めるのに、期限の時刻に `Error` を返す兄弟を `Async.all_results` に並べました。tzroutine では、待つ相手のチャンネルと [`Time.after`](../built-in-types-and-modules/time.md#タイムアウトtimeafter) のチャンネルを [select](select.md#タイムアウト) で同時に待ち、先に来たほうを選びます。

書き換える前（今は `E1040`）:

```text
def answer :: unit -> Async<Result<string, string>>
fn answer _ = Async {
    do! Async.sleep 200
    return Result.Ok "value"
}

def deadline :: i64 -> Async<Result<string, string>>
fn deadline ms = Async {
    do! Async.sleep ms
    return Result.Error "timeout"
}

let! outcome = Async.block_on (Async.all_results [answer (), deadline 20])
match outcome with
| Result.Ok _ -> "value"
| Result.Error reason -> reason
```

書き換えた後:

```tsuzuri run=timeout
let answers: Channel<string> = Channel.make 1
let out = Channel.share answers
tzroutine {
    Time.sleep (200 * Time.millisecond())
    Channel.send out "value"
}
let deadline = Time.after (20 * Time.millisecond())
select
| recv answer from answers -> Maybe.get answer
| recv _ from deadline -> "timeout"
```

実行結果:

```text
timeout
```

- `select` は待つのをやめるだけで、相手の tzroutine を止めません。境界（ここではトップレベル）は、その tzroutine が終わるまで戻らないので、このプログラムは 200 ms ほどで終わります。`all_results` のように相手を途中で止めるには、相手に [Context](../built-in-types-and-modules/context.md) を渡し、`Context.done` を `select` で見させます（[終わらない tzroutine を止める](#終わらない-tzroutine-を止める)）。期限で取り消す `Context` は `Context.with_timeout` で作ります。
- チャンネルの容量を 1 にしておくと、期限のあとに送る値は受け取られないままチャンネルに入り、相手は待たずに終わります。容量 0 だと、相手は受け取り手を待ち続け、境界の待ちと合わせてデッドロックとして報告されます（`all tzroutines are asleep - deadlock!`）。
- ソケットの操作の期限は、今までどおり `Net` の操作の時間制限（`Maybe.Some ms`）で書けます。

### Async.all_results を ErrGroup にする

`Async.all_results` は、時刻の順で最初の `Error` を返し、残りの計算をその場で止めました。[ErrGroup](../built-in-types-and-modules/errgroup.md) は、最初の誤りを `ErrGroup.wait` で返し、グループの `Context` を取り消します。残りの仕事は、その `Context` を見て自分で止まります。

書き換える前（今は `E1040`）:

```text
def check :: i64 -> i64 -> Async<Result<unit, string>>
fn check delay code = Async {
    do! Async.sleep delay
    if code > 0 then return Result.Error $"code {code}"
    else return Result.Ok ()
}

let! outcome = Async.block_on (Async.all_results [check 1000 0, check 1 2, check 2000 0])
match outcome with
| Result.Ok _ -> "ok"
| Result.Error error -> $"error: {error}"
```

書き換えた後:

```tsuzuri run=error%3A%20code%202
def check :: ref Context -> i64 -> i64 -> Result<unit, string>
fn check ctx delay code =
    let done = Context.done ctx
    let timer = Time.after (delay * Time.millisecond())
    select
    | recv _ from done -> Result.Ok ()
    | recv _ from timer -> if code > 0 then Result.Error $"code {code}" else Result.Ok ()

let group: ErrGroup<string> = ErrGroup.create ()
let delays = [1000, 1, 2000]
let codes = [0, 2, 0]
for index in 0 ..< 3 do
    let delay = delays[index]
    let code = codes[index]
    let ctx = ErrGroup.context group
    ErrGroup.go group (Owned.function (\() -> check (ref ctx) delay code))
match ErrGroup.wait group with
| Result.Ok _ -> "ok"
| Result.Error error -> $"error: {error}"
```

実行結果:

```text
error: code 2
```

- 2 つ目の仕事が 1 ms で失敗するとグループの `Context` が取り消され、`done` を待っていた 1 つ目と 3 つ目がすぐ戻ります。`ErrGroup.wait` は全員が戻ってから戻るので、プログラムは 1 秒を待たずに終わります。
- `ErrGroup.wait` の「最初」は、`Async.all_results` と同じく終わった時刻の順です。関数は `Ok` の値を返せないので、値は `Atomic`・`Mutex`・`Channel` で受け渡します（[Task.parallel_results を ErrGroup にする](#taskparallel_results-を-errgroup-にする)）。

### Net の _async を tzroutine の中の Net にする

`Net.read_async` などは、`Async.block_on` の中でソケットを待つ双子でした。tzroutine の中では、同期版の `Net.read`・`Net.write`・`Net.accept`・`Net.connect` などが、ソケットの準備を待つ間その tzroutine だけを待たせます（[tzroutine の中の Net](../built-in-types-and-modules/net.md#tzroutine-の中の-net)）。型は `Async<…>` から `IO<…>` に戻るので、`let!` で実行します。

書き換える前（今は `E1040`）:

```text
def echo_one :: own Net.TcpStream -> Async<i64>
fn echo_one stream = Async {
    match! Net.read_async stream 100 (Maybe.Some 5000) with
    | Result.Error _ -> return -1
    | Result.Ok bytes ->
        let count = bytes.length
        let! _written = Net.write_async stream bytes (Maybe.Some 5000)
        let! _closed = Net.close_async stream
        return count
}

def serve :: own Net.TcpListener -> Async<i64>
fn serve listener = Async {
    match! Net.accept_async listener (Maybe.Some 5000) with
    | Result.Error _ -> return -2
    | Result.Ok stream -> return! echo_one stream
}

// main: let! counts = Async.block_on (Async.all [serve listener, client target])
```

書き換えた後:

```tsuzuri run=server%205%20client%205
def bytes_of :: ref string -> [ubyte]
fn bytes_of text = Utf8String.to_bytes (Utf8String.from_string text)

def main :: unit -> i32 = \() ->
    let! bound = Net.bind (Maybe.get (Net.parse_address (ref "127.0.0.1:0")))
    let listener = Result.get bound
    let target = Net.local_addr listener
    let server_count: Channel<i64> = Channel.make 1
    let client_count: Channel<i64> = Channel.make 1
    let server_out = Channel.share server_count
    tzroutine {
        let! accepted = Net.accept listener (Maybe.Some 5000)
        let stream = Result.get accepted
        let! received = Net.read stream 100 (Maybe.Some 5000)
        let bytes = Result.get received
        let count = bytes.length
        let! _written = Net.write stream bytes (Maybe.Some 5000)
        let! _closed = Net.close stream
        Channel.send server_out count
    }
    let client_out = Channel.share client_count
    tzroutine {
        let! connected = Net.connect target (Maybe.Some 5000)
        let stream = Result.get connected
        let! _written = Net.write stream (bytes_of (ref "hello")) (Maybe.Some 5000)
        let! received = Net.read stream 100 (Maybe.Some 5000)
        let! _closed = Net.close stream
        Channel.send client_out (Result.get received).length
    }
    let server = Maybe.get (Channel.recv server_count)
    let client = Maybe.get (Channel.recv client_count)
    do! IO.write_line $"server {server} client {client}"
    let! _closed = Net.close_listener listener
    0
```

実行結果:

```text
server 5 client 5
```

- 時間制限（`Maybe.Some ms`）は、同期版でも呼び出し全体の期限で、切れると `TimedOut` です。
- `_async` の操作は、兄弟の失敗で取り消せました。tzroutine で待っている操作を途中でやめるには、ほかの tzroutine がそのソケットを閉じます。待ちは閉じたハンドルの誤り（`InvalidInput`）で終わります。
- wasm32 の `--wasm-feature jspi --wasm-feature net` では、同期版の操作が JSPI で、それを呼んだ tzroutine だけを中断します。1 つのモジュールの中のサーバーとクライアントも、それぞれの tzroutine で同時に待てます（[WebAssembly](#webassemblyjspi)）。

### Async.start とホストの poll

`Async.start` は、計算をホストが駆動する実行器へ預けてすぐ戻り、ホストが `tsuzuri_async_poll` を回して進めました。`Async.host` はホストの操作を始め、ホストが `tsuzuri_async_complete`（ほかのスレッドからは `tsuzuri_async_post`）で完了を渡しました。この形には後継がありません。tzroutine は、それを起動した境界（`export` の 1 回の呼び出しなど）の中で終わるからです。ヘッダーの `tsuzuri_async_*` の宣言も、WebAssembly の `tsuzuri_async_set_epoch` も、グルーの `bindings.async`（`complete`・`settled`）もなくなりました。

書き換える前（今は `E1040`）:

```text
extern def start_download :: i64 -> i64 -> unit
extern def cancel_download :: i64 -> unit
extern def show :: i64 -> unit

def download :: i64 -> Async<i64> = \item -> Async.host (Async.Operation {
    start: \operation -> start_download operation item,
    cancel: \operation -> cancel_download operation
})

export def refresh :: i64 -> unit = \item ->
    do! Async.start (Async {
        let! sizes = Async.all [download item, download (item + 1)]
        show (sizes[0] + sizes[1])
    })
```

ネイティブでは、ホストの操作を普通の（待つ）`extern` 関数にして、export の中の tzroutine から呼びます。`extern` 関数の呼び出しの間も、ほかの tzroutine は走ります。export の呼び出しは、その中の tzroutine がすべて終わるまで戻りません。ホストのイベントループを止めたくないときは、ホストが別のスレッドから export を呼びます。

書き換えた後:

```tsuzuri
extern def download :: i64 -> i64

export def refresh :: i64 -> i64 = \item ->
    let sizes: Channel<i64> = Channel.make 2
    for offset in 0 ..< 2 do
        let out = Channel.share sizes
        tzroutine {
            Channel.send out (download (item + offset))
        }
    Maybe.get (Channel.recv sizes) + Maybe.get (Channel.recv sizes)
```

C のホストは、イベントループのスレッドとは別のスレッドで `tz_refresh` を呼びます。

```c
#include <pthread.h>
#include <stdio.h>
#include <unistd.h>
#include "app.h"

int64_t tsuzuri_host_Main_download(int64_t item) {
    usleep(10000); /* ホストの待つ呼び出し */
    return item * 100;
}

static void *refresh_thread(void *argument) {
    int64_t *item = argument;
    *item = tz_refresh(*item); /* 中の tzroutine が終わると戻る */
    return NULL;
}

int main(void) {
    int64_t item = 1;
    pthread_t thread;
    pthread_create(&thread, NULL, refresh_thread, &item);
    /* ここでホストのイベントループが動き続ける */
    pthread_join(thread, NULL);
    printf("%lld\n", (long long)item);
    return 0;
}
```

```sh
tsuzuri build Main.tz --emit header -o app.h
tsuzuri build Main.tz --emit object -o app.o
clang -O2 host.c app.o -o host -pthread
./host
```

実行結果:

```text
300
```

- 結果を待たずにイベントループへ戻りたいときは、上のように export を呼ぶスレッドを分け、終わったらそのスレッドからイベントループへ知らせます（ホストの仕組みで）。
- WebAssembly では、`--wasm-feature jspi` のグルーの export が `Promise` を返します。JavaScript は `await` で結果を待ち、その間もイベントループは動きます。モジュールの中の tzroutine の待ちとソケットの待ちは、その tzroutine だけを JSPI で中断します（[WebAssembly](#webassemblyjspi)）。

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
- 捕捉は所有値のムーブか Copy で、`Send` でなければなりません。借用は `E1013` です。
- `main`・`export` の呼び出し・`test`・`bench` は、その中で起動した tzroutine がすべて終わるまで戻りません。最後の式の値はその前に計算されます。
- P の数は `TSUZURI_MAXPROCS`、スタックの予約は `TSUZURI_TZROUTINE_STACK_KIB` で変えられます。
- WebAssembly では `--wasm-feature jspi` の wasm32 で動き、1 本のスレッドの上で交代に走ります。tzroutine のスタックは 64 KiB です。`--freestanding`、`--trap-mode return` とは組み合わせられません（`E2000`）。
- 削除した `Task` の書き換えは、[Task からの移行](#task-からの移行) の表と例のとおりです。`Task.scope` だけは `tsuzuri fix` が `Routine.scope` へ書き換えます。
- 削除した `Async` と `Net` の `_async` の書き換えは、[Async からの移行](#async-からの移行) のとおりです。ホストが駆動する実行器と、`Async.run` の仮想時刻には後継がありません。

## 関連項目

- [Routine](../built-in-types-and-modules/routine.md)
- [Parallel](../built-in-types-and-modules/parallel.md)
- [WaitGroup](../built-in-types-and-modules/waitgroup.md)
- [Channel](../built-in-types-and-modules/channel.md)
- [Atomic](../built-in-types-and-modules/atomic.md)
- [Mutex](../built-in-types-and-modules/mutex.md)
- [Rc と Arc](../built-in-types-and-modules/rc.md)
- [Owned](../built-in-types-and-modules/owned.md)
- [Context](../built-in-types-and-modules/context.md)
- [ErrGroup](../built-in-types-and-modules/errgroup.md)
- [Time](../built-in-types-and-modules/time.md)
- [select 式](select.md)
- [Net](../built-in-types-and-modules/net.md)
- [所有権とムーブ](../ownership-and-memory/ownership.md)
- [言語仕様（tzroutine）](../../../docs/language.md#tzroutine)
- [言語リファレンスの目次](../index.md)
