# Channel

`Channel<T>` は、Go の `chan T` に当たる双方向のチャンネルです。[tzroutine](../async-tasks-and-lazy/tzroutine.md) の間で、所有する値を 1 つずつ受け渡します。1 つのハンドルで送りも受けもし、`Channel.share` でハンドルを増やして、ほかの tzroutine へ移します。容量 0 のチャンネルは、送り手と受け手が出会うまで両方を待たせ（受け渡しの同期）、容量のあるチャンネルはキューになります。

> 送り手と受け手の端が分かれ、最後の送り手の drop で閉じるチャンネルは [DataChannel](./datachannel.md)（TZ03 より前の名前は `Channel`）です。[使い分け](#datachannel-との使い分け)を見てください。

## この記事のポイント

- `Channel.make n` は容量 `n` のチャンネルを作ります。`0` はバッファなし（Go の `make(chan T)`）です。負の容量はトラップです。要素の型は、tzroutine へ渡せる（`Send`）ものに限ります。
- `Channel.send ch item` は、受け手が待っていれば直接渡し、空きがあれば入れ、どちらもなければ待ちます。`Channel.recv ch` は、要素があれば `Some item` を返し、空なら待ちます。閉じて空なら `None` です（Go の `v, ok := <-ch`）。 <!-- rule: TSK-016 -->
- **閉じるのは `Channel.close` だけ**です。ハンドルを drop しても閉じません。閉じたチャンネルへの `send` と、2 回目の `close` はトラップです（Go の panic）。 <!-- rule: TSK-017 -->
- `for x in ch do ...` は、閉じて空になるまで受け取り続けます（Go の `for x := range ch`）。`break` と `continue` も使えます。 <!-- rule: TSK-019 -->
- `Channel.try_send` と `Channel.try_recv` は待ちません。入れられなかった要素は `Error` の中に戻ります。
- 待つのは tzroutine だけで、OS スレッドは止まりません。待っている tzroutine の数はスレッド数に縛られません。誰も起こせない待ちは、`Tsuzuri runtime: all tzroutines are asleep - deadlock!` を書いて、その呼び出しの位置でトラップします。
- 要素は FIFO（送りが終わった順）で、1 つの要素は 1 つの受け手だけに届きます。要素は借用を持てません（`E1013`）。 <!-- rule: TSK-018 -->
- 送るだけのハンドル `SendOnly<T>`（Go の `chan<- T`）と、受け取るだけのハンドル `ReceiveOnly<T>`（`<-chan T`）を、`Channel.send_only`・`Channel.receive_only` で作れます（[向きのあるハンドル](#向きのあるハンドル)）。

## 基本の書き方

生産者の tzroutine が 1 から 100 までを送って閉じ、`main` が `for` で受け取って足します。

```tsuzuri run=sum%3D5050
let jobs: Channel<i64> = Channel.make 0
let producer = Channel.share jobs
tzroutine {
    for i in 1 ..= 100 do
        Channel.send producer (i as i64)
    Channel.close producer
}
let mut sum = 0i64
for job in jobs do
    sum = sum + job
$"sum={sum}"
```

実行結果:

```text
sum=5050
```

- 要素の型が決まらない `Channel.make 0` は `E1015` です。`let jobs: Channel<i64> = ...` のように型を書きます。
- `Channel.share jobs` は、同じチャンネルのハンドルをもう 1 つ作ります。tzroutine へは、そのハンドルをムーブします。
- 引数の型が `ref Channel<'a>` なので、呼び出しでは `ref` を省けます（`Channel.send producer 1`）。

## バッファなしとバッファあり

容量 0 では、`send` は受け手が来るまで、`recv` は送り手が来るまで待ちます。値は、出会った瞬間に送り手から受け手へ直接渡ります。

```text
容量 0:   send ──待つ──┐
                      ├─ 出会う: 値が渡り、両方が進む
          recv ──待つ──┘

容量 n:   send → [ 1 | 2 | 3 ] → recv     満杯の send と空の recv だけが待つ
```

容量があれば、満杯になるまで `send` は待たず、2 つ目の tzroutine がなくても使えます。

```tsuzuri run=1234
let values: Channel<i64> = Channel.make 3
Channel.send values 1
Channel.send values 2
Channel.send values 3
let mut code = match Channel.recv values with
    | Maybe.Some value -> value
    | Maybe.None -> 0
Channel.send values 4
for _i in 0 ..< 3 do
    code = code * 10 + (match Channel.recv values with
        | Maybe.Some value -> value
        | Maybe.None -> 0)
code
```

実行結果:

```text
1234
```

## 閉じる

`Channel.close` で閉じると、受け手は残りの要素を受け取り、そのあとは `None` を受け取り続けます。閉じるのは送る側の仕事です。受け手は閉じません。

```tsuzuri run=1-2-none-none
def show :: Maybe<i64> -> string
fn show item =
    match item with
    | Maybe.Some value -> $"{value}"
    | Maybe.None -> "none"

let values: Channel<i64> = Channel.make 4
Channel.send values 1
Channel.send values 2
Channel.close values
let a = show (Channel.recv values)
let b = show (Channel.recv values)
let c = show (Channel.recv values)
let d = show (Channel.recv values)
$"{a}-{b}-{c}-{d}"
```

実行結果:

```text
1-2-none-none
```

- 閉じたチャンネルへの `send` は、`Tsuzuri runtime: send on closed Channel` を書いてトラップします。送っている途中で閉じられた場合も同じです。
- 2 回目の `close` は、`Tsuzuri runtime: close of closed Channel` を書いてトラップします。
- 閉じたときに待っている受け手は `None` で、送り手はトラップで起きます。
- ハンドルを drop しても閉じません。すべてのハンドルが drop されると、チャンネルのブロックは解放され、残っていた要素はちょうど 1 回ずつ drop されます。

## for で受け取る

`for x in ch do body` は、`ch` が閉じて空になるまで受け取ります。`ch` の型は `Channel<T>` か `ref Channel<T>` で、`x` は `T` です。要素は受け取った側のものなので、`string` のような Copy でない要素も受け取れます。`break` で `for` を抜け、`continue` で次の受信へ進みます。`for` のあとも `ch` は使えます（場所にあるチャンネルは借用されるだけです）。

```tsuzuri run=picked%3D16%20rest%3D10
let values: Channel<i64> = Channel.make 0
let out = Channel.share values
tzroutine {
    for i in 1 ..= 10 do
        Channel.send out (i as i64)
    Channel.close out
}
let mut picked = 0i64
for value in values do
    if value % 2 == 0 then
        continue
    if value > 7 then
        break
    picked = picked + value
let mut rest = 0i64
for value in values do
    rest = rest + value
$"picked={picked} rest={rest}"
```

実行結果:

```text
picked=16 rest=10
```

`DataChannel.Receiver` は `for` の対象になりません（`E1005`）。

## 待たない送受信

`Channel.try_send ch item` は、すぐに入れられれば `Ok ()`、満杯で待っている受け手もいなければ `Error (Channel.SendError.Full item)`、閉じていれば `Error (Channel.SendError.Closed item)` です。どちらの `Error` にも要素が戻るので、値は失われません。閉じていてもトラップしません。

`Channel.try_recv ch` は、`Channel.Received.Item value`、開いていて空なら `Channel.Received.Empty`、閉じて空なら `Channel.Received.Disconnected` です。

```tsuzuri run=full%3Dbcd%20empty
let values: Channel<string> = Channel.make 1
let _first = Channel.try_send values "a"
let full = match Channel.try_send values "bcd" with
    | Result.Error (Channel.SendError.Full item) -> item
    | _ -> ""
let _taken = Channel.try_recv values
let state = match Channel.try_recv values with
    | Channel.Received.Item _ -> "item"
    | Channel.Received.Empty -> "empty"
    | Channel.Received.Disconnected -> "disconnected"
$"full={full} {state}"
```

実行結果:

```text
full=bcd empty
```

Go では `select` と `default` で書く操作です。複数のチャンネルから選ぶ `select` は TZ04 です。

## ハンドルと所有権

- `Channel<T>` は非 Copy の所有値です。代入や引数渡しはムーブで、複製は `Channel.share` です。参照の数を数え、最後のハンドルの drop でブロックを解放します。
- `Channel<T>` は `Send` かつ `Sync` です。tzroutine や `Owned.function` へ移せます。
- 関数値には捕捉できません（`Drop` を持つので `Capture` でない。`E1005`）。`ref ch` を捕捉するか、`Channel.share` したハンドルを tzroutine へ移します。
- 中身は不透明です。`Channel { block: ... }` の構築、`ch.block`、パターン分解、更新構文は `E1022`、`export def` と `extern def` の境界は `E1008`、`const` は `E1026` です。`Eq`・`Ord`・`Hash`・`Display` の instance はありません。`Channel.__release` を std の外から呼ぶことも `E1022` です。

## 要素と借用

要素は、`send` が戻ったあともチャンネルに残り、あとで別の tzroutine に受け取られます。そのため、要素は借用を持てません。`Channel.send` と `Channel.try_send` の呼び出しで、要素が借用を持たないことを調べます（[DataChannel](./datachannel.md#要素と借用) と同じ規則）。

- 通るもの: 整数・文字列・配列などの所有値、値のコピーを捕捉した関数、何も捕捉しない関数、その場で作った `Owned.function`。
- `E1013`（`a channel item cannot hold a borrow, ...`）になるもの: `ref` で借りた値を捕捉した関数と、それを含む値。

```text
def push :: ref Channel<i64 -> i64> -> ref [i64] -> unit
fn push ch data =
    Channel.send ch (\x -> x + data[0])
```

```text
error[E1013]: a channel item cannot hold a borrow, including a borrowed function environment: it is received later, perhaps after the borrowed data is gone; send an owned value, or a function that captures a copy, or an Owned.Function built where it is sent
```

要素の型が借用を持ちうる（関数値を含む）ときは、`Channel.send ch` のような部分適用や関数値にすることも `E1013`（`Channel.send must be fully applied directly when its item may hold a borrow, ...`。`try_send` では `Channel.try_send ...`）です。受け取った要素は借用を持たず、チャンネルを借りてもいません。

## 向きのあるハンドル

関数の引数に、送るだけ・受け取るだけの役割を型で書けます。`Channel.send_only ch` は送るだけの `SendOnly<T>`、`Channel.receive_only ch` は受け取るだけの `ReceiveOnly<T>` を作ります。どちらも同じチャンネルのハンドルを 1 つ増やし（`share` と同じ）、drop で手放します。`Channel` へは戻せません。

```tsuzuri run=sum%3D5050
def produce :: own SendOnly<i64> -> i64 -> unit
fn produce out n =
    for i in 1 ..= n do
        SendOnly.send out (i as i64)
    SendOnly.close out

def total :: ref ReceiveOnly<i64> -> i64
fn total input =
    let mut sum = 0i64
    for value in input do
        sum = sum + value
    sum

let numbers: Channel<i64> = Channel.make 0
let out = Channel.send_only numbers
let input = Channel.receive_only numbers
tzroutine {
    produce out 100
}
$"sum={total (ref input)}"
```

実行結果:

```text
sum=5050
```

| ハンドル | 使える操作 |
| --- | --- |
| `Channel<T>` | `Channel.send`・`try_send`・`recv`・`try_recv`・`close`・`length`・`capacity`・`share`・`send_only`・`receive_only`、`for` |
| `SendOnly<T>` | `SendOnly.send`・`try_send`・`close`・`length`・`capacity`・`share` |
| `ReceiveOnly<T>` | `ReceiveOnly.recv`・`try_recv`・`length`・`capacity`・`share`、`for` |

- 操作は、ハンドルのモジュールの関数です（`SendOnly.send out 1`・`ReceiveOnly.recv input`）。意味は `Channel` の同じ名前の関数と同じで、`try_send`・`try_recv` の結果も `Channel.SendError`・`Channel.Received` です。要素の借用の規則も同じです。
- 向きの違う操作はありません。`SendOnly.recv` は `E1002`（`module 'SendOnly' has no function or union case 'recv'`）、`Channel.recv` に `SendOnly<T>` を渡すと `E1003`（`expected Channel<...>, found SendOnly<i64>`）です。
- `SendOnly`・`ReceiveOnly` も予約モジュール名で、不透明な所有値です（`Channel` と同じ規則。関数値への捕捉は `E1005`、構築やフィールド参照は `E1022`）。

## 待ちとデッドロック

- 待つのは tzroutine だけです。待っている間、その OS スレッドはほかの tzroutine を走らせます。
- `main` や `export` を呼んだスレッド、`Task.parallel` のタスクのような tzroutine でないスレッドが待つと、そのスレッドが止まります。
- 誰も起こせない待ち（すべての tzroutine とスレッドが待っていて、走るものもタイマーもない）は、`Tsuzuri runtime: all tzroutines are asleep - deadlock! (1 waiting: Channel.recv)` を書き、待っていた呼び出しの位置でトラップします。`--trap-info` の `tsuzuri run` は、その行を `E2005` の位置にします。

```text
let ch: Channel<i64> = Channel.make 0
match Channel.recv ch with      // ← ここでトラップする
| Maybe.Some value -> value
| Maybe.None -> 0
```

- tzroutine を起動するプログラムでも、`Channel` を使うプログラムでも、`Mutex.with_lock` の中で待つ操作（`send`・`recv`・`for`）を呼ぶとトラップします（`Tsuzuri runtime: a tzroutine cannot yield or wait inside Mutex.with_lock; ...`）。ロックの持ち主は OS スレッドだからです。`try_send`・`try_recv`・`close`・`length`・`capacity` は待たないので使えます。

## ターゲット

| 構成 | `Channel` |
| --- | --- |
| native の macOS・Linux・Windows | 動く |
| 既定の wasm32・wasm64（import なし） | tzroutine を起動しないプログラムは、1 本の実行として使える。待つべき操作（空のバッファなしの `recv` など）はトラップする |
| `--wasm-feature threads` | `E2000`（`Channel cannot be used with --wasm-feature threads; use DataChannel, which runs on the threads backend`） |
| `--freestanding` | `E2000` |

既定の WebAssembly では、モジュールにランタイムのチャンネルの部分（`src/runtime/tzroutine.c` を 1 本の実行向けに作ったもの）が入り、import は増えません。誰も起こせないので、待つべき操作はすぐにトラップします（`DataChannel` の既定の WebAssembly と同じです）。

## 表現と性能

- チャンネルは 1 つのブロックです。128 バイトのヘッダー（ロック、ハンドルの数、要素の大きさ、容量、要素数、送り手と受け手の待ち行列）の後ろに、容量 × 要素の大きさのリングバッファが続きます。確保は `Channel.make` で 1 回、解放は最後のハンドルの drop で 1 回です。
- 要素は、ランタイムがバイト列としてコピーします（バッファなしの受け渡しは、送り手のスタックから受け手のスタックへ 1 回）。要素の型ごとの drop は、コンパイラーが生成します。
- ロックはチャンネルごとに 1 つです。ほかのチャンネルの操作とは競合しません。
- 待ちと起床は tzroutine のスケジューラーが行い、OS のスレッドを起こすのは、待っていた tzroutine を走らせる M が眠っていたときだけです。

## API リファレンス

| 関数 | シグネチャ | 説明 | Go |
| --- | --- | --- | --- |
| `make` | `Send<'a> => i64 -> Channel<'a>` | 容量 `n` のチャンネル。0 はバッファなし。負はトラップ | `make(chan T, n)` |
| `send` | `ref Channel<'a> -> 'a -> unit` | 送る。受け手も空きもなければ待つ。閉じていればトラップ | `ch <- v` |
| `recv` | `ref Channel<'a> -> Maybe<'a>` | 受け取る。空なら待つ。閉じて空なら `None` | `v, ok := <-ch` |
| `try_send` | `ref Channel<'a> -> 'a -> Result<unit, Channel.SendError<'a>>` | 待たずに送る。`Full item`・`Closed item` で要素を返す | `select` + `default` |
| `try_recv` | `ref Channel<'a> -> Channel.Received<'a>` | 待たずに受け取る。`Item v`・`Empty`・`Disconnected` | `select` + `default` |
| `close` | `ref Channel<'a> -> unit` | 閉じる。2 回目はトラップ | `close(ch)` |
| `length` | `ref Channel<'a> -> i64` | 入っている要素の数 | `len(ch)` |
| `capacity` | `ref Channel<'a> -> i64` | 容量 | `cap(ch)` |
| `share` | `ref Channel<'a> -> Channel<'a>` | 同じチャンネルのハンドルをもう 1 つ作る | （ポインタの共有） |
| `send_only` | `ref Channel<'a> -> SendOnly<'a>` | 送るだけのハンドルを作る | `chan<- T` |
| `receive_only` | `ref Channel<'a> -> ReceiveOnly<'a>` | 受け取るだけのハンドルを作る | `<-chan T` |
| `SendOnly.send`・`try_send`・`close`・`length`・`capacity`・`share` | `ref SendOnly<'a> -> …`（`Channel` の同じ名前の関数と同じ型） | `SendOnly` の操作 | |
| `ReceiveOnly.recv`・`try_recv`・`length`・`capacity`・`share` | `ref ReceiveOnly<'a> -> …`（同上） | `ReceiveOnly` の操作 | |

- 型は `Channel<T>`（モジュールと同じ名前の record）、`Channel.SendError<T> = Full of T | Closed of T`、`Channel.Received<T> = Item of T | Empty | Disconnected` です。
- `for x in ch` は Go の `for x := range ch` です。

## 引数の順序

`Channel` は資源のハンドルの例外で、ハンドルが先頭の引数です（`Channel.send ch item`。.NET のメソッドの受け手と同じ）。[引数の順序](./argument-order.md)を見てください。

## DataChannel との使い分け

| | `Channel<T>` | `DataChannel`（旧 `Channel`） |
| --- | --- | --- |
| 形 | 1 つのハンドルで送受信（Go の `chan T`）。`share` で複製 | 送り手 `Sender` と受け手 `Receiver` に分かれる。`clone_sender` |
| 閉じ方 | `close` だけ（drop では閉じない） | 最後の `Sender` の drop で閉じる |
| 閉じた後の `send` | トラップ（Go の panic）。`try_send` は要素を返す | `Error item`（要素を返す） |
| 受け手がいないときの `send` | 待つ（受け手を数えない） | `Error item` |
| バッファなし | 容量 0 | なし（容量は 1 以上） |
| `for` | 使える | 使えない |
| 待ち | tzroutine だけが待つ | OS スレッドを止める（Task のプールの規則） |
| `--wasm-feature threads` | 使えない | 使える |

tzroutine の間の受け渡しには `Channel` を使います。`DataChannel` の待ちは Task のプールの規則のままで、tzroutine を数えません。

## 旧 Channel のプログラム

TZ03 より前の `Channel.bounded`・`Channel.clone_sender`・`Channel.Sender`・`Channel.Receiver` は、`E1040`（kind `renamed_to_data_channel`、`'Channel.bounded' is now 'DataChannel.bounded'`）です。`DataChannel` の送り手や受け手に `Channel.send`・`Channel.recv` を使った呼び出しも `E1040` で、`DataChannel.send`・`DataChannel.recv` を示します。どれも確かな（`certain`）修正案を持ち、`tsuzuri fix` がまとめて `DataChannel` の名前に直します。直したプログラムの意味は、旧と同じです。

## まとめ

- `Channel.make n` で作り、`send`・`recv`・`close`・`for` で使います。容量 0 は受け渡しの同期です。
- 閉じるのは `close` だけで、閉じた後の `send` と 2 回目の `close` はトラップです。
- 待つのは tzroutine だけで、誰も起こせない待ちはその場所でトラップします。

## 関連項目

- [tzroutine 式](../async-tasks-and-lazy/tzroutine.md)
- [Routine](./routine.md)
- [DataChannel](./datachannel.md)
- [Mutex](./mutex.md)
- [for...in](../loops-and-conditionals/for-in.md)
- [言語リファレンスの目次](../index.md)
