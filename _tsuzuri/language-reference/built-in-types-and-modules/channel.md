# Channel

`Channel.bounded` は、容量の決まったキューを作り、送る側 `Sender` と受ける側 `Receiver` を返します。タスクの間で、所有する値を 1 つずつ受け渡すときに使います。共有した 1 つの値を書き換えるなら [Mutex](./mutex.md) や [Atomic](./atomic.md)、結果を集めるだけなら `Task.parallel` の戻り値で足ります。

## この記事のポイント

- `Channel.bounded capacity` は `(Sender<T> * Receiver<T>)` を返します。容量は 1 以上で、0 以下はトラップです。要素の型 `T` は、タスクへ渡せる（`Send`）ものに限ります。
- `Channel.send (ref sender) item` は、キューに空きがなければ待ち、`Ok ()` を返します。受け手が 1 つも残っていないときは、待たずに `Error item` で要素を返します。
- `Channel.recv (ref receiver)` は、要素があれば `Some item` を返し、空なら送り手が来るのを待ちます。すべての `Sender` が drop され、残りの要素も取り尽くしたら `None` です。
- `Channel.clone_sender (ref sender)` は、独立した所有者の `Sender` を作ります。**最後の `Sender` が drop されると、チャンネルは閉じます**。最後の `Receiver` が drop されると、以後の `send` は `Error` を返します。
- 要素は FIFO で、1 つの要素は 1 つの受け手だけに届きます。どちらの端も `Sync` で、`ref` を共有してもよく、`Arc` に入れてもかまいません。複数の送り手と複数の受け手（MPMC）が使えます。
- 両端は所有値です。関数値には捕捉できません（`E1005`）。借用 `ref sender` なら捕捉できます。
- 待つスレッドは、先にプールの未着手の仕事を手伝い、それでも進めなければ眠ります。**どのスレッドも待っていて、動かせる仕事もないとき**は、無限に待たず、`deadlock: every task is waiting on a channel` というトラップになります。完了するかどうかは、スレッド数に左右されます（[待ちの規則](#待ちの規則)）。
- チャンネルに残った要素は、最後の端が drop されるとき、ちょうど 1 回ずつ drop されます。

## 基本の書き方

要素は先に入れたものから順に出ます。容量の範囲内なら、2 つ目のスレッドは要りません。

```tsuzuri run=first%3D10%20rest%3D90
def take :: ref Channel.Receiver<i64> -> i64
fn take receiver =
    match Channel.recv receiver with
    | Maybe.Some value -> value
    | Maybe.None -> -1

match Channel.bounded 3 with
| (sender, receiver) ->
    let _a = Channel.send (ref sender) 10
    let _b = Channel.send (ref sender) 20
    let _c = Channel.send (ref sender) 30
    let first = take (ref receiver)
    let _d = Channel.send (ref sender) 40
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
def take :: ref Channel.Receiver<i64> -> i64
fn take receiver =
    match Channel.recv receiver with
    | Maybe.Some value -> value
    | Maybe.None -> -1

match Channel.bounded 4 with
| (sender, receiver) ->
    let _a = Channel.send (ref sender) 1
    let _b = Channel.send (ref sender) 2
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
match Channel.bounded 2 with
| (sender, receiver) ->
    Owned.drop receiver
    match Channel.send (ref sender) 41 with
    | Result.Ok _unit -> "sent"
    | Result.Error item -> $"refused {item}"
```

実行結果:

```text
refused 41
```

`Channel.clone_sender` で作った `Sender` は、元の `Sender` と対等な所有者です。元を drop しても、複製が残っているあいだ、チャンネルは開いています。

```tsuzuri run=first%3D5%20second%3D-1
def take :: ref Channel.Receiver<i64> -> i64
fn take receiver =
    match Channel.recv receiver with
    | Maybe.Some value -> value
    | Maybe.None -> -1

match Channel.bounded 3 with
| (sender, receiver) ->
    let other = Channel.clone_sender (ref sender)
    Owned.drop sender
    let _sent = Channel.send (ref other) 5
    Owned.drop other
    let first = take (ref receiver)
    let second = take (ref receiver)
    $"first={first} second={second}"
```

実行結果:

```text
first=5 second=-1
```

## タスクの間で使う

### 生産者と消費者

端を所有するタスクを `Task.parallel` に渡すのが、最も素直な書き方です。タスクが終わると、タスクが持つ `Sender` が drop され、最後の 1 つなら受け手に終わりが伝わります。

```tsuzuri run=sent%3D100%20sum%3D5050
def produce :: Channel.Sender<i64> -> i64 -> Task<i64>
fn produce sender count = task {
    let mut index = 1
    while index <= count do
        let _sent = Channel.send (ref sender) index
        index = index + 1
    return count
}

def consume :: Channel.Receiver<i64> -> Task<i64>
fn consume receiver = task {
    let mut sum = 0
    let mut open = true
    while open do
        match Channel.recv (ref receiver) with
        | Maybe.Some value -> sum = sum + value
        | Maybe.None -> open = false
    return sum
}

match Channel.bounded 100 with
| (sender, receiver) ->
    let results = Task.run (Task.parallel [produce sender 100, consume receiver])
    $"sent={results[0]} sum={results[1]}"
```

実行結果:

```text
sent=100 sum=5050
```

容量が要素の数以上なので、生産者は待たずに終わります。スレッドが 1 つでも、先頭のタスクから順に走るので完了します。容量が要素の数より小さいと、生産者は空きを待ち、消費者は要素を待ちます。2 つのタスクが同時に走る必要があり、スレッドが 2 つ以上ないと完了しません（[待ちの規則](#待ちの規則)）。

### 作業キュー

受け手は `Sync` なので、`Arc` に入れると、複数のタスクが 1 つの `Receiver` から取れます。要素は 1 つのワーカーにだけ届きます。ワーカーの結果は、`Arc<Mutex<T>>` に集めます。

```tsuzuri run=jobs%3D20%20squares%3D2870
def produce :: Channel.Sender<i64> -> i64 -> Task<i64>
fn produce sender count = task {
    let mut index = 1
    while index <= count do
        let _sent = Channel.send (ref sender) index
        index = index + 1
    return count
}

def work :: Arc<Channel.Receiver<i64>> -> Arc<Mutex<i64>> -> Task<i64>
fn work receiver total = task {
    let mut done = 0
    let mut open = true
    while open do
        match Channel.recv (Arc.get (ref receiver)) with
        | Maybe.Some job ->
            let _seen = Mutex.with_lock (Arc.get (ref total)) (value -> {
                deref value = deref value + job * job;
                0
            })
            done = done + 1
        | Maybe.None -> open = false
    return done
}

match Channel.bounded 64 with
| (sender, receiver) ->
    let shared = Arc.new receiver
    let total = Arc.new (Mutex.create 0i64)
    let results = Task.run (Task.parallel [produce sender 20, work (Arc.share (ref shared)) (Arc.share (ref total)), work (Arc.share (ref shared)) (Arc.share (ref total))])
    let squares = Mutex.with_lock (Arc.get (ref total)) (value -> deref value)
    $"jobs={results[1] + results[2]} squares={squares}"
```

実行結果:

```text
jobs=20 squares=2870
```

2 つのワーカーが何件ずつ処理するかは、実行ごとに変わります。合計は変わりません。`Mutex.with_lock` の中では `Channel` の操作はできないので（[ロックの中](#ロックの中)）、`recv` はロックの外で行い、受け取った値だけをロックの中で使います。

### 端を共有する

`Task.scope` の `shared` に端を入れると、全員が `ref` で同じ端を使えます。1 つの端を複数の子どもが使うときは、子どもが `Sender` を所有できないので、チャンネルを閉じる drop は、`Task.scope` を呼んだ側で行います。

```tsuzuri run=total%3D36
match Channel.bounded 8 with
| (sender, receiver) ->
    let _sent = Task.scope (ref sender) 8 (shared -> index -> match Channel.send shared (index + 1) with | Result.Ok _unit -> 1 | Result.Error _item -> 0)
    Owned.drop sender
    let mut total = 0
    let mut open = true
    while open do
        match Channel.recv (ref receiver) with
        | Maybe.Some value -> total = total + value
        | Maybe.None -> open = false
    $"total={total}"
```

実行結果:

```text
total=36
```

`Task.scope` の子どもが受け取る側で待つとき、閉じるのは `Task.scope` が返ったあとなので、子どもは閉じるのを待てません。そのときは、受け取る件数を決めて受ける（`recv` を件数だけ呼ぶ）か、端を所有するタスクを `Task.parallel` で使います。

## 待ちの規則

`send` が満杯、`recv` が空のときに、スレッドは次の順で動きます。

1. **手伝う**: プールに、どのスレッドも始めていない仕事（`Task.parallel` や `Task.scope` の子ども）があれば、その 1 つを自分のスタックの上で走らせます。仕事が終わったら、チャンネルをもう一度見ます。ただし、空いているワーカーがいるときは手伝いません。空きワーカーが引き受けるからです。手伝いを重ねる深さは 16 までです。
2. **眠る**: 手伝う仕事がなければ、チャンネルが変わるまで眠ります。要素が入る、空きができる、端が drop される、のどれかで起こされます。
3. **見つける**: スレッドがすべて待っていて（チャンネルの待ち、`Task.parallel` や `Task.scope` の結合の待ち、仕事を待つワーカー）、動かせる仕事もないとき、もうチャンネルを変えられるスレッドはいません。待っているスレッドすべてを起こし、標準エラーへ `Tsuzuri runtime: deadlock: every task is waiting on a channel` と出して、トラップします（種類は assert）。待ちを止められない `Sender` を drop し忘れた場合も、ここで見つかります。

| 環境 | 待ちが満たされないとき |
| --- | --- |
| native（CPU が複数） | 上の規則どおり。最後の待ちスレッドが、判定を出す |
| native（CPU が 1 つ） | 子どもは `index` の順に 1 つのスレッドで走り、待ちはすぐ判定になる（トラップ） |
| 既定の wasm32 | 子どもは `index` の順に 1 つずつ走り、満たされない待ちは、すぐトラップする（コンソールがないので、メッセージは出ない） |
| `--wasm-feature threads` | native と同じ規則で、メインとワーカーのスレッドを数える。トラップは `RuntimeError` |

**完了するかどうかは、スレッド数に左右されます。** プールのスレッド数は固定で、子どもごとの専用スレッドは保証されません。待っている仕事の数がスレッド数を超えると、足りない分の仕事は、待っているスレッドのスタックの上で走るか、走れないままになります。

- 生産者と消費者の 2 つのタスクで、容量が足りないとき: スレッドが 2 つ以上必要です。
- 生産者、変換、消費の 3 段のパイプラインで、どの段も隣を待つとき: スレッドが 3 つ以上必要です。
- 1 つのスレッドでは、容量が足りない生産者は、消費者が動く前に満杯で待つので、判定になります。

スレッドが足りないとトラップになるだけで、ハングもデータの破損もありません。ただし、別の理由でトラップするより見つけにくいので、パイプラインの段数と、各段が待ちうるチャンネルの数は、スレッド数に合わせて設計します。たとえば、`Task.parallel` に渡すタスクの数を CPU 数以下にします。

```text
Tsuzuri runtime: deadlock: every task is waiting on a channel
```

### ロックの中

`Mutex.with_lock` の中で `Channel.send`・`Channel.recv` を呼ぶと、待つことになるかどうかにかかわらず、トラップします。ロックを持ったまま待つと、ロックを待つスレッドと、ロックの持ち主が待つチャンネルの相手が、互いを待つ形になりうるからです。native では、トラップの前に `a Channel operation may wait; move it outside Mutex.with_lock` と出します。

### トラップ

- 既定（`--trap-mode abort`）では、判定はプロセスを終わらせます。
- `--trap-mode return` では、判定は、呼び出しの境界へ返るトラップになります。待っていたスレッドは全員起き、グループは終わり、境界は、その呼び出しが確保したもの（チャンネルのブロックを含む）を解放します。次の呼び出しは、普通に動きます。端を持つタスクがトラップしたときは、その端の drop が走らないので、待っている受け手は、判定で終わります。呼び出しに返るのは、`index` が最小のトラップです。

## 型の規則

- `Channel.bounded` は `Send<'a>` を要求します。`Channel.bounded` で `Rc` を運ぶ型は作れません（`E1013`、`tasks require Send values`）。
- `Sender<T>` と `Receiver<T>` は、どちらも `Sync` で、`Send` です（`T` が `Send` の間だけ作れるので）。共有借用から使う操作は、ランタイムのロックで守られます。
- 両端は非 Copy の所有値です。複製できず、代入や引数渡しはムーブです（`E1012`）。関数値は複製されることがあるので、端そのものの捕捉は `E1005` です。借用 `ref sender` を捕捉するか、`Arc` に入れるか、引数で渡します。
- 両端は中身が不透明です。構築、フィールド参照、パターン分解、更新構文は `E1022`、`export def` と `extern def` の境界は `E1008`、`const` の初期化は `E1026` です。`Eq`・`Ord`・`Hash`・`Display` の instance はありません。
- drop で閉じるのは、std の `Channel` モジュールが両端に書いた `Drop` の instance です。利用者が std の型に `Drop` の instance を書くことは `E1016` のままで、`Channel.__close_sender` などの内部の関数を呼ぶことは `E1022` です。
- 要素の drop: 閉じたあとにキューに残った要素は、最後の端が drop されるときに、1 つずつ drop されます。受け取った要素は受け取った側のものです。

## 表現と性能

- チャンネルは 1 つのブロックです。先頭の 80 バイトに容量、要素の大きさ、先頭の位置、要素数、送り手・受け手・所有者の数があり、その後ろにリングバッファ（要素 × 容量）が続きます。確保は `Channel.bounded` で 1 回、解放は最後の端の drop で 1 回です。
- 要素は、ランタイムがバイト列としてコピーします。要素の型ごとの drop は、コンパイラーが、ブロックを解放するときに生成します。大きい要素はコピーされるので、小さい値か、所有する配列や文字列のように、小さな表現の値を送ります。
- ネイティブのランタイム（`task.c`）は、スレッドプールの mutex 1 つで、すべてのチャンネルの操作と、待ちの数え上げを守ります。待ちは、スレッドごとの条件変数で、チャンネルの変化を知る 1 つのスレッドだけを起こします。チャンネルの数が多く、操作が非常に頻繁なプログラムでは、この 1 つの mutex が競合します。
- `--wasm-feature threads` のランタイムは、同じ規則を WebAssembly の atomic 命令で実装します。待つスレッドは、プール共有の `epoch` の語で眠り、何かが変わるたびに全員が起きて、条件を見直します。スレッドごとの状態は、インスタンスごとの WebAssembly のグローバルで、TLS は要りません。
- 既定の wasm32 は 1 スレッドで、`Channel` の操作はモジュール内の IR です。ネイティブの C のランタイムも、threads の C のランタイムも使いません。
- `Channel` は、ソースに名前が現れたプログラムだけに読み込まれます。`Channel` を書かないプログラムの出力は変わりません。`--freestanding` で `Channel` を使うと `E2000` です（タスクのランタイムが要ります）。

## API リファレンス

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `bounded` | `Send<'a> => i64 -> (Sender<'a> * Receiver<'a>)` | 容量 `n`（1 以上）のチャンネルを作る。0 以下はトラップ |
| `send` | `ref Sender<'a> -> 'a -> Result<unit, 'a>` | 要素を入れる。満杯なら待つ。受け手がいなければ `Error 要素` |
| `recv` | `ref Receiver<'a> -> Maybe<'a>` | 要素を取る。空なら待つ。閉じて空なら `None` |
| `clone_sender` | `ref Sender<'a> -> Sender<'a>` | 独立した所有者の `Sender` を作る |

- 型名は `Channel.Sender<T>` と `Channel.Receiver<T>` で、型引数は 1 つです。モジュール名 `Channel` は予約されていて、同じ名前のモジュールは `E1011` です。同じ名前のレコードも宣言できますが、std のものとは別の型です。
- `send` が値を返すのは、受け手が消えたときに、要素を失わないためです。`Result` を無視するときは、`let _sent = ...` と書きます。

## まとめ

- タスクの間で所有値を受け渡すときは `Channel` を使い、端を所有するタスクを `Task.parallel` に渡します。
- 最後の `Sender` の drop が、チャンネルを閉じます。drop し忘れた `Sender` は、受け手を待たせ続けますが、すべてのスレッドが待った時点で、判定のトラップになります。
- 待つタスクの数がスレッド数を超えるパイプラインは、完了しないことがあります。判定のトラップは、そのときの合図です。
- `Mutex.with_lock` の中では `Channel` を使えません。
