# DataChannel

> **旧 `Channel`**: TZ03 で `DataChannel` に改名しました。API と意味は変わりません。今の `Channel` は Go と同じ双方向のチャンネル（[Channel](./channel.md)）です。旧の名前で書いたプログラムは `E1040` になり、`tsuzuri fix` で `DataChannel` の名前に直せます。使い分けは [Channel と DataChannel](./channel.md#datachannel-との使い分け) を見てください。

`DataChannel.bounded` は、容量の決まったキューを作り、送る側 `Sender` と受ける側 `Receiver` を返します。tzroutine やスレッドの間で、所有する値を 1 つずつ受け渡すときに使います。共有した 1 つの値を書き換えるなら [Mutex](./mutex.md) や [Atomic](./atomic.md)、結果を集めるだけなら [`Routine.scope`](./routine.md#routinescope) や [Parallel](./parallel.md) の戻り値で足ります。

## この記事のポイント

- `DataChannel.bounded capacity` は `(Sender<T> * Receiver<T>)` を返します。容量は 1 以上で、0 以下はトラップです。要素の型 `T` は、ほかの tzroutine へ渡せる（`Send`）ものに限ります。
- `DataChannel.send (ref sender) item` は、キューに空きがなければ待ち、`Ok ()` を返します。受け手が 1 つも残っていないときは、待たずに `Error item` で要素を返します。
- `DataChannel.recv (ref receiver)` は、要素があれば `Some item` を返し、空なら送り手が来るのを待ちます。すべての `Sender` が drop され、残りの要素も取り尽くしたら `None` です。
- `DataChannel.clone_sender (ref sender)` は、独立した所有者の `Sender` を作ります。**最後の `Sender` が drop されると、チャンネルは閉じます**。最後の `Receiver` が drop されると、以後の `send` は `Error` を返します。
- 要素は FIFO で、1 つの要素は 1 つの受け手だけに届きます。どちらの端も `Sync` で、`ref` を共有してもよく、`Arc` に入れてもかまいません。複数の送り手と複数の受け手（MPMC）が使えます。
- 両端は所有値です。関数値には捕捉できません（`E1005`）。借用 `ref sender` なら捕捉できます。`tzroutine { }` の本体へはムーブで持ち込めます。
- 要素は借用を持てません。関数値は、環境が借用を持たないときだけ送れます（`E1013`、[要素と借用](#要素と借用)）。
- ネイティブでは、待つのは待つ tzroutine だけです（`main` のトップレベルや、`export` を呼んだホストのスレッドでは、そのスレッド）。待つ数に上限はありません。**誰も起こせない待ち**は、無限に待たず、`Tsuzuri runtime: all tzroutines are asleep - deadlock! (1 waiting: DataChannel.recv)` と書いてトラップします（[待ちの規則](#待ちの規則)）。
- チャンネルに残った要素は、最後の端が drop されるとき、ちょうど 1 回ずつ drop されます。

## 基本の書き方

要素は先に入れたものから順に出ます。容量の範囲内なら、送る側と受ける側が同じ 1 本の実行でもかまいません。

```tsuzuri run=first%3D10%20rest%3D90
def take :: ref DataChannel.Receiver<i64> -> i64
fn take receiver =
    match DataChannel.recv receiver with
    | Maybe.Some value -> value
    | Maybe.None -> -1

match DataChannel.bounded 3 with
| (sender, receiver) ->
    let _a = DataChannel.send (ref sender) 10
    let _b = DataChannel.send (ref sender) 20
    let _c = DataChannel.send (ref sender) 30
    let first = take (ref receiver)
    let _d = DataChannel.send (ref sender) 40
    let rest = take (ref receiver) + take (ref receiver) + take (ref receiver)
    $"first={first} rest={rest}"
```

実行結果:

```text
first=10 rest=90
```

3 つの要素は、満杯になった時点で、受け手が 1 つ取るまで次を入れられません。リングバッファなので、`10` を取ったあとに入れた `40` は最後に出ます。

### 閉じる

`Sender` が drop されると、キューに残っている要素は読めて、そのあとは `None` を返し続けます。drop は、スコープの終わりでも、`Owned.drop` でも起きます。

```tsuzuri run=1%202%20-1%20-1
def take :: ref DataChannel.Receiver<i64> -> i64
fn take receiver =
    match DataChannel.recv receiver with
    | Maybe.Some value -> value
    | Maybe.None -> -1

match DataChannel.bounded 4 with
| (sender, receiver) ->
    let _a = DataChannel.send (ref sender) 1
    let _b = DataChannel.send (ref sender) 2
    Owned.drop sender
    let first = take (ref receiver)
    let second = take (ref receiver)
    let ended = take (ref receiver)
    let again = take (ref receiver)
    $"{first} {second} {ended} {again}"
```

実行結果:

```text
1 2 -1 -1
```

受け手がいなくなると、送るほうが気づきます。`send` は待たずに、渡した要素を `Error` に入れて返します。

```tsuzuri run=refused%2041
match DataChannel.bounded 2 with
| (sender, receiver) ->
    Owned.drop receiver
    match DataChannel.send (ref sender) 41 with
    | Result.Ok _unit -> "sent"
    | Result.Error item -> $"refused {item}"
```

実行結果:

```text
refused 41
```

`DataChannel.clone_sender` で作った `Sender` は、元の `Sender` と対等な所有者です。元を drop しても、複製が残っているあいだ、チャンネルは開いています。

```tsuzuri run=first%3D5%20second%3D-1
def take :: ref DataChannel.Receiver<i64> -> i64
fn take receiver =
    match DataChannel.recv receiver with
    | Maybe.Some value -> value
    | Maybe.None -> -1

match DataChannel.bounded 3 with
| (sender, receiver) ->
    let other = DataChannel.clone_sender (ref sender)
    Owned.drop sender
    let _sent = DataChannel.send (ref other) 5
    Owned.drop other
    let first = take (ref receiver)
    let second = take (ref receiver)
    $"first={first} second={second}"
```

実行結果:

```text
first=5 second=-1
```

## tzroutine の間で使う

### 生産者と消費者

`Sender` を `tzroutine { }` の本体へムーブするのが、最も素直な書き方です。本体が終わると、本体が持つ `Sender` が drop され、最後の 1 つなら受け手に終わりが伝わります。

```tsuzuri run=sum%3D5050
def produce :: own DataChannel.Sender<i64> -> i64 -> unit
fn produce sender count =
    let mut index = 1
    while index <= count do
        let _sent = DataChannel.send (ref sender) index
        index = index + 1

def consume :: ref DataChannel.Receiver<i64> -> i64
fn consume receiver =
    let mut sum = 0
    let mut open = true
    while open do
        match DataChannel.recv receiver with
        | Maybe.Some value -> sum = sum + value
        | Maybe.None -> open = false
    sum

match DataChannel.bounded 4 with
| (sender, receiver) ->
    tzroutine {
        produce sender 100
    }
    $"sum={consume (ref receiver)}"
```

実行結果:

```text
sum=5050
```

容量 4 は要素の数より小さいので、生産者は空きを待ち、消費者（`main` のトップレベル）は要素を待ちます。待つのは待つ側だけなので、`TSUZURI_MAXPROCS=1` でも完了します。

### 作業キュー

受け手は `Sync` なので、`Arc` に入れると、複数の tzroutine が 1 つの `Receiver` から取れます。要素は 1 つのワーカーにだけ届きます。ワーカーの結果は `Arc<Mutex<T>>` と `Arc<Atomic<T>>` に集め、全員の終わりを [WaitGroup](./waitgroup.md) で待ちます。

```tsuzuri run=jobs%3D20%20squares%3D2870
def produce :: own DataChannel.Sender<i64> -> i64 -> unit
fn produce sender count =
    let mut index = 1
    while index <= count do
        let _sent = DataChannel.send (ref sender) index
        index = index + 1

def work :: ref DataChannel.Receiver<i64> -> ref Mutex<i64> -> i64
fn work receiver total =
    let mut done = 0
    let mut open = true
    while open do
        match DataChannel.recv receiver with
        | Maybe.Some job ->
            Mutex.with_lock total (\value -> deref value = deref value + job * job)
            done = done + 1
        | Maybe.None -> open = false
    done

match DataChannel.bounded 4 with
| (sender, receiver) ->
    let queue = Arc.new receiver
    let total = Arc.new (Mutex.create 0i64)
    let jobs = Arc.new (Atomic.create 0i64)
    let group = WaitGroup.create ()
    tzroutine {
        produce sender 20
    }
    for _worker in 0 ..< 2 do
        WaitGroup.add (ref group) 1
        let finished = WaitGroup.share (ref group)
        let mine = Arc.share (ref queue)
        let sums = Arc.share (ref total)
        let counts = Arc.share (ref jobs)
        tzroutine {
            let done = work (Arc.get (ref mine)) (Arc.get (ref sums))
            let _before = Atomic.fetch_add (Arc.get (ref counts)) done
            WaitGroup.done (ref finished)
        }
    WaitGroup.wait (ref group)
    let squares = Mutex.with_lock (Arc.get (ref total)) (\value -> deref value)
    $"jobs={Atomic.load (Arc.get (ref jobs))} squares={squares}"
```

実行結果:

```text
jobs=20 squares=2870
```

2 つのワーカーが何件ずつ処理するかは、実行ごとに変わります。合計は変わりません。ワーカーは `recv` をロックの外で行い、受け取った値だけをロックの中で使います。ロックを持ったまま待つこともできますが（[ロックの中](#ロックの中)）、待つあいだ、ほかのワーカーがロックを取れないからです。

### 端を共有する

[`Routine.scope`](./routine.md#routinescope) の `shared` に端を入れると、全員が `ref` で同じ端を使えます。子どもは `Sender` を所有しないので、チャンネルを閉じる drop は、`Routine.scope` を呼んだ側で行います。

```tsuzuri run=total%3D36
match DataChannel.bounded 8 with
| (sender, receiver) ->
    let _sent = Routine.scope (ref sender) 8 (\shared index -> match DataChannel.send shared (index + 1) with | Result.Ok _unit -> 1 | Result.Error _item -> 0)
    Owned.drop sender
    let mut total = 0
    let mut open = true
    while open do
        match DataChannel.recv (ref receiver) with
        | Maybe.Some value -> total = total + value
        | Maybe.None -> open = false
    $"total={total}"
```

実行結果:

```text
total=36
```

この例は、呼んだ側が `Routine.scope` の終わりを待ってから受け取るので、容量が送る件数（8）以上でなければ終わりません（満杯で待つ子を起こす受け手がいないため、デッドロックの判定になります）。容量を小さくするときは、受け取る側も tzroutine にします。`Routine.scope` の子どもが受け取る側で待つときも、閉じるのは `Routine.scope` が返ったあとなので、子どもは閉じるのを待てません。そのときは、受け取る件数を決めて受ける（`recv` を件数だけ呼ぶ）か、`Sender` を所有する tzroutine から送ります。

## 待ちの規則

ネイティブのターゲットでは、`DataChannel` は tzroutine のスケジューラーの上で待ちます（TZ12）。規則は [Channel](./channel.md#待ちとデッドロック) と同じです。

1. **待つ**: `send` が満杯、`recv` が空のとき、待つのは呼んだ tzroutine だけです。その OS スレッドは、ほかの tzroutine を走らせ続けます。`main` のトップレベルや、`export` を呼んだホストのスレッドのような tzroutine でないスレッドは、そのスレッドで待ちます。待つ数は、スレッドの数にも P の数（`TSUZURI_MAXPROCS`）にも縛られません。
2. **起きる**: 要素を入れた側は待っている受け手を 1 つ、要素を取った側は待っている送り手を 1 つ起こします。最後の `Sender` の drop は待っている受け手すべてを、最後の `Receiver` の drop は待っている送り手すべてを起こします。起きた側は、チャンネルをもう一度見ます。
3. **見つける**: すべての tzroutine とスレッドが待っていて、走るものもタイマーもないとき、もうチャンネルを変えられるものはいません。tzroutine のランタイムのデッドロックの判定が、待っている数と理由（`DataChannel.send`・`DataChannel.recv`、ほかの待ちなら `Channel.recv`・`WaitGroup.wait` など）を標準エラーへ書いて、待っていた呼び出しの位置でトラップします（種類は assert）。待ちを止められない `Sender` を drop し忘れた場合も、ここで見つかります。

```text
Tsuzuri runtime: all tzroutines are asleep - deadlock! (1 waiting: DataChannel.recv)
```

`Parallel.*` のコールバックの中で待つのも同じ規則です。ただし、同じチャンクの要素は 1 つの実行で順に呼ばれる（[チャンク境界](./parallel.md#チャンク境界)）ので、ほかの要素のコールバックを待つ形は完了しません。互いを待つ子どもには `Routine.scope` を使います。子ごとに tzroutine なので、子がいくつあっても、`TSUZURI_MAXPROCS=1` でも完了します。

tzroutine を起動しない WebAssembly のプログラムの規則は、これまでと同じです。

| 環境 | 待ちが満たされないとき |
| --- | --- |
| native | 上の規則。誰も起こせなければ判定のトラップ |
| 既定の wasm32 | 1 本の実行なので、満たされない待ちは、すぐトラップする（コンソールがないので、メッセージは出ない） |
| `--wasm-feature jspi` で tzroutine を起動する wasm32 | 待つのは待つ tzroutine だけで、ほかの tzroutine が走る。どの tzroutine も起こせなければ、グルーが export の `Promise` を `RuntimeError("all tzroutines are asleep - deadlock!")` で拒否する |
| `--wasm-feature threads` | 待つスレッドは、先に `Parallel.*` の未着手のチャンクを自分のスタックの上で手伝い（空いているワーカーがいないときだけ。深さは 16 まで）、それでも進めなければ眠る。メインとワーカーのスレッドがすべて待ったときは、判定のトラップ（`RuntimeError`） |

### ロックの中

ネイティブでは、`Mutex.with_lock` のロックは tzroutine のものなので（[Mutex](./mutex.md#ロックの規則)）、ロックを持ったまま `DataChannel.send`・`DataChannel.recv` で待てます。待つあいだ、そのロックを待つほかの tzroutine は止まったままなので、ロックの中で待つのは、そのつもりのときだけにします。次の例は、ロックを持ったまま、ほかの tzroutine が送る値を待ちます。

```tsuzuri run=total%3D42
def add_next :: ref Mutex<i64> -> ref DataChannel.Receiver<i64> -> i64
fn add_next total receiver =
    Mutex.with_lock total (\value -> {
        let item = match DataChannel.recv receiver with
            | Maybe.Some item -> item
            | Maybe.None -> 0;
        deref value = deref value + item;
        deref value
    })

let total = Mutex.create 1i64
match DataChannel.bounded 1 with
| (sender, receiver) ->
    tzroutine {
        let _sent = DataChannel.send (ref sender) 41
    }
    $"total={add_next (ref total) (ref receiver)}"
```

実行結果:

```text
total=42
```

WebAssembly（tzroutine のランタイムを使わないプログラム）では、これまでどおり、`Mutex.with_lock` の中で `DataChannel.send`・`DataChannel.recv` を呼ぶと、待つことになるかどうかにかかわらずトラップします。ロックを持ったまま待つと、ロックを待つスレッドと、ロックの持ち主が待つチャンネルの相手が、互いを待つ形になりうるからです。

### トラップ

- 既定（`--trap-mode abort`）では、判定はプロセスを終わらせます。
- `--trap-mode return` では、判定は、呼び出しの境界へ返るトラップになります。境界は、その呼び出しが確保したもの（チャンネルのブロックを含む）と、呼び出しが取ったロックを解放します。次の呼び出しは、普通に動きます。トラップの境界の中の `Parallel.*` は、すべてのチャンクを呼んだスレッドで添字の順に走らせるので、その呼び出しの中で待つのは、呼んだスレッドだけです。

## 要素と借用

要素は、`DataChannel.send` が戻ったあともチャンネルに残り、あとで、別の tzroutine で、借用元のデータがなくなったあとに受け取られることがあります。そのため、要素は借用を持てません。コンパイラーは、`DataChannel.send` の呼び出しで、要素が借用を持たないことを調べます。整数、文字列、配列などの所有値は、いつも通ります。関数値（と、それを含む `Maybe`・タプル・配列・レコード・`Seq`）は、環境が借用を持たないときだけ通ります。`Sender<T>` は整数 1 つの不透明な値で、型からは要素の型が見えないので、この呼び出しが唯一の検査です。

- 通るもの: 値のコピーを捕捉した関数、何も捕捉しない関数、名前を付けた関数、その場で作った `Owned.function`。
- `E1013` になるもの: `ref` で借りた値（引数で受け取った `ref` も含む）を捕捉した関数と、それを含む値。

```tsuzuri run=42
match DataChannel.bounded 2 with
| (sender, receiver) ->
    let k = 41i64
    let _sent = DataChannel.send (ref sender) (\x -> x + k)
    match DataChannel.recv (ref receiver) with
    | Maybe.Some f -> f 1
    | Maybe.None -> 0
```

実行結果:

```text
42
```

次のように、引数で借りたデータを捕捉した関数を送ると、`E1013` です。この関数は、`work` が戻ったあとの `data` を読むことになります。

```text
def work :: ref DataChannel.Sender<i64 -> i64> -> ref [i64] -> i64
fn work sender data =
    match DataChannel.send sender (x -> x + data[0]) with
    | Result.Ok _unit -> 0
    | Result.Error _item -> 1
```

```text
error[E1013]: a channel item cannot hold a borrow, including a borrowed function environment: it is received later, perhaps after the borrowed data is gone; send an owned value, or a function that captures a copy, or an Owned.Function built where it is sent
```

- 呼び出しは、送り手と要素を一度に渡す直接の形で行います。要素の型が借用を持ちうる（関数値を含む）ときは、`DataChannel.send (ref sender)` のように引数を一部だけ渡すことも、`DataChannel.send` 自体を関数値にすることも、`item |> DataChannel.send (ref sender)` も、`E1013`（`DataChannel.send must be fully applied directly when its item may hold a borrow`）です。要素が整数や文字列などなら、どの形でも書けます。
- 関数値を引数で受け取って送る関数（`def push :: ref DataChannel.Sender<'a> -> 'a -> bool`）は、`'a` が関数型のとき、呼び出し側の関数が借用を持つかどうかを本体からは知れないので、`E1013` です。引数の関数値を `tzroutine { }` に持ち込むのと同じ扱いです。関数は、送る場所で作ります。`'a` が整数や文字列などなら、`push` は書けます。
- 受け手が取り出した要素は、借用を持たず、受け手を借りてもいません。取り出した関数を別のチャンネルへ送ることも、関数から返すこともできます。
- `Mutex.create` に借用を捕捉した関数を入れると、借用はロックの値に残り、そのロックは借用元より長く生きる場所へ出せません（[Mutex](./mutex.md)）。

## 型の規則

- `DataChannel.bounded` は `Send<'a>` を要求します。`Rc` を運ぶチャンネルは作れません（`E1013`、`values that move to another tzroutine or thread must be Send`）。型変数は `Send` かどうか分からないので、`DataChannel.bounded` を包んだジェネリック関数は、その要求を制約として持ち続け、関数を使う型ごとに検査されます。`make :: i64 -> (DataChannel.Sender<'a> * DataChannel.Receiver<'a>)` を `Rc<i64>` で使うのも `E1013` です。型変数に型引数を適用した型（高カインドの型変数の `'f<i64>`）が要素のときも同じで、`'f` に `Rc` を持つ型が決まった使用箇所で `E1013` です（[制約](../types-and-type-inference/constraints.md)）。
- `Sender<T>` と `Receiver<T>` は、どちらも `Sync` で、`Send` です（`T` が `Send` の間だけ作れるので）。共有借用から使う操作は、ランタイムのロックで守られます。
- 両端は非 Copy の所有値です。複製できず、代入や引数渡しはムーブです（`E1012`）。関数値は複製されることがあるので、端そのものの捕捉は `E1005` です。借用 `ref sender` を捕捉するか、`Arc` に入れるか、引数で渡します。
- 両端は中身が不透明です。構築、フィールド参照、パターン分解、更新構文は `E1022`、`export def` と `extern def` の境界は `E1008`、`const` の初期化は `E1026` です。`Eq`・`Ord`・`Hash`・`Display` の instance はありません。
- drop で閉じるのは、std の `DataChannel` モジュールが両端に書いた `Drop` の instance です。利用者が std の型に `Drop` の instance を書くことは `E1016` のままで、`DataChannel.__close_sender` などの内部の関数を呼ぶことは `E1022` です。
- 要素の drop: 閉じたあとにキューに残った要素は、最後の端が drop されるときに、1 つずつ drop されます。受け取った要素は受け取った側のものです。

## 表現と性能

- チャンネルは 1 つのブロックです。先頭の 80 バイトに容量、要素の大きさ、先頭の位置、要素数、送り手・受け手・所有者の数、チャンネルのロック、待っている受け手と送り手の列があり、その後ろにリングバッファ（要素 × 容量）が続きます。確保は `DataChannel.bounded` で 1 回、解放は最後の端の drop で 1 回です。
- 要素は、ランタイムがバイト列としてコピーします。要素の型ごとの drop は、コンパイラーが、ブロックを解放するときに生成します。大きい要素はコピーされるので、小さい値か、所有する配列や文字列のように、小さな表現の値を送ります。
- ネイティブのランタイムは tzroutine のランタイム（`src/runtime/tzroutine.c`）です。ロックはチャンネルごとに 1 つで、ほかのチャンネルの操作とは競合しません。待つ tzroutine は、チャンネルの受け手の列か送り手の列に並んで眠り、要素の出し入れは、反対側の列の先頭の 1 つだけを起こします。
- `--wasm-feature threads` のランタイム（`src/runtime/task-wasm-threads.c`）は、ワーカーのプールの mutex 1 つで、すべてのチャンネルの操作と待ちの数え上げを守り、WebAssembly の atomic 命令で実装します。待つスレッドは、プール共有の `epoch` の語で眠ります。起こすのは、新しい仕事、グループの終わり、チャンネルの変化（待っている者がいるとき）、競合したロックの解放のときだけで、そのとき全員が起きて、条件を見直します。グループのほかのチャンクがまだ走っているあいだに 1 つが終わっても、だれも起こしません。スレッドごとの状態は、インスタンスごとの WebAssembly のグローバルで、TLS は要りません。
- 既定の wasm32 は 1 スレッドで、tzroutine を起動しないプログラムでは、`DataChannel` の操作はモジュール内の IR です。ネイティブの C のランタイムも、threads の C のランタイムも使いません。`--wasm-feature jspi` で tzroutine を起動するプログラムでは、ネイティブと同じ tzroutine のランタイム（`src/runtime/tzroutine.c` を JSPI の 1 本のスレッド向けに作ったもの）で、待つ tzroutine はチャンネルの列に並んで中断します（[tzroutine の WebAssembly](../async-tasks-and-lazy/tzroutine.md#webassemblyjspi)）。
- `DataChannel` は、ソースに名前が現れたプログラムだけに読み込まれます。`DataChannel` を書かないプログラムの出力は変わりません。`--freestanding` で `DataChannel` を使うと `E2000` です（tzroutine のランタイムが要ります）。

## API リファレンス

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `bounded` | `Send<'a> => i64 -> (Sender<'a> * Receiver<'a>)` | 容量 `n`（1 以上）のチャンネルを作る。0 以下はトラップ |
| `send` | `ref Sender<'a> -> 'a -> Result<unit, 'a>` | 要素を入れる。満杯なら待つ。受け手がいなければ `Error 要素` |
| `recv` | `ref Receiver<'a> -> Maybe<'a>` | 要素を取る。空なら待つ。閉じて空なら `None` |
| `clone_sender` | `ref Sender<'a> -> Sender<'a>` | 独立した所有者の `Sender` を作る |

- 型名は `DataChannel.Sender<T>` と `DataChannel.Receiver<T>` で、型引数は 1 つです。モジュール名 `DataChannel` は予約されていて、同じ名前のモジュールは `E1011` です。同じ名前のレコードも宣言できますが、std のものとは別の型です。
- `send` が値を返すのは、受け手が消えたときに、要素を失わないためです。`Result` を無視するときは、`let _sent = ...` と書きます。

## まとめ

- tzroutine やスレッドの間で所有値を受け渡すときは `DataChannel` を使い、`Sender` を送る側の `tzroutine { }` の本体へムーブします。
- 最後の `Sender` の drop が、チャンネルを閉じます。drop し忘れた `Sender` は、受け手を待たせ続けますが、誰も起こせなくなった時点で、判定のトラップになります。
- ネイティブでは待つのは待つ tzroutine だけで、待つ数はスレッドの数に縛られません。ロックを持ったまま待つこともできます。
- tzroutine を起動しない WebAssembly のプログラムの規則は変わりません。既定の wasm32 は 1 本の実行で、満たされない待ちはすぐトラップします。tzroutine のランタイムを使わないプログラムでは、`Mutex.with_lock` の中の `DataChannel` の操作もトラップです。
- tzroutine を起動する wasm32 のプログラム（`--wasm-feature jspi`）では、ネイティブと同じく、待つのは待つ tzroutine だけで、ロックを持ったまま待てます（[tzroutine の WebAssembly](../async-tasks-and-lazy/tzroutine.md#webassemblyjspi)）。
