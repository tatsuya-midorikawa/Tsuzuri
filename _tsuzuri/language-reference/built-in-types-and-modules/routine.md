# Routine

`Routine` は、[tzroutine 式](../async-tasks-and-lazy/tzroutine.md) の実行を扱う組み込みのモジュールです。所有する関数を tzroutine として起動する `go`、借用を共有する子の tzroutine を待つ `scope`、実行の譲渡、スケジューラーへの問い合わせを持ちます。std のファイルはなく、`Routine` は予約されたモジュール名です（`Routine.tz` というファイルは `E1011`）。

## この記事のポイント

- `Routine.go` は `Owned.Function<unit, unit>` を新しい tzroutine で 1 回呼び、そのあと drop します。`tzroutine { }` の関数版です。
- `Routine.scope shared count f` は、子ごとに 1 つの tzroutine で `f shared index` を呼び、全員が終わってから結果を添字の順の配列で返します（`Task.scope` の後継、TZ11）。子は `Channel` や同期の型で待て、待つ子の数に上限はありません。
- `Routine.yield ()` は、実行をほかの tzroutine に譲ります（Go の `runtime.Gosched`）。
- `Routine.count ()` は、生きている tzroutine の数に、呼んだスレッドの 1 を足した数です（Go の `runtime.NumGoroutine` は `main` を数えます）。
- `Routine.max_procs ()` は同時に tzroutine を走らせる OS スレッドの数、`Routine.cpu_count ()` はオンラインの CPU 数です。
- `Routine.scope` 以外は関数値として渡せます（`let f: unit -> unit = Routine.yield`）。`Routine.scope` は `Task.scope` と同じく直接の呼び出しだけです（`E1013`）。`pure def` の中では使えません（`E1029`）。`Routine.yield` は `Mutex.with_lock` の中でも譲れます（ロックは tzroutine のもの。TZ06）。

## 提供 API 一覧

| 名前 | 型シグネチャ | 説明 | Go |
| --- | --- | --- | --- |
| `Routine.go` | `own Owned.Function<unit, unit> -> unit` | 所有する関数を新しい tzroutine で 1 回呼び、そのあと drop する | `go f()` |
| `Routine.scope` | `(Sync<'s>, Send<'a>) => ref 's -> i64 -> (ref 's -> i64 -> 'a) -> ['a]` | 子ごとに 1 つの tzroutine で呼び、全員の終わりを待って結果を添字の順に返す | goroutine と `sync.WaitGroup` |
| `Routine.yield` | `unit -> unit` | 実行を譲る。tzroutine でないスレッドでは OS へ譲る | `runtime.Gosched()` |
| `Routine.count` | `unit -> i64` | 生きている tzroutine の数 + 1（呼んだスレッドの分） | `runtime.NumGoroutine()` |
| `Routine.max_procs` | `unit -> i64` | 同時に tzroutine を走らせる OS スレッドの数（`TSUZURI_MAXPROCS`、既定は CPU 数で 256 まで） | `runtime.GOMAXPROCS(0)` |
| `Routine.cpu_count` | `unit -> i64` | オンラインの CPU 数（1 以上） | `runtime.NumCPU()` |

`Routine.go` で起動した tzroutine も、`tzroutine { }` と同じく、それを起動した境界（`main`・`export` の呼び出し・`test`・`bench`）が終わりを待ちます。

既定の WebAssembly では、tzroutine を起動しないプログラムの `Routine.count ()`・`Routine.max_procs ()`・`Routine.cpu_count ()` は 1 で、`Routine.yield ()` は何もしません（1 本の実行）。`Routine.go` を含むプログラムは `E2000` です。

## 使い方

`Routine.yield` は、待っているほかの tzroutine に実行を譲ります。子の完了を数えて待つときに使います。

```tsuzuri run=done%3D3
let done = Arc.new (Atomic.create 0i64)
for _i in 0 ..< 3 do
    let finished = Arc.share (ref done)
    tzroutine {
        let _count = Atomic.fetch_add (Arc.get (ref finished)) 1i64
    }
while Atomic.load (Arc.get (ref done)) < 3i64 do
    Routine.yield ()
$"done={Atomic.load (Arc.get (ref done))}"
```

`Routine.go` は、関数値として受け取った 1 回の仕事を起動します。

```tsuzuri run=hello%20from%20Routine.go%0A0
def spawn :: own Owned.Function<unit, unit> -> unit = \work -> Routine.go work

spawn (Owned.function (\() -> {
    do! IO.write_line "hello from Routine.go"
}))
0
```

`Owned.function` の本体は捕捉を借りて呼ばれるので、捕捉をムーブする処理は `tzroutine { }` で書きます。

## Routine.scope

`Routine.scope shared count f` は、`count` 個の子を、それぞれ自分の tzroutine で `f shared index` として呼びます。子は同じ共有借用 `shared` を読み、結果は添字の順の配列になります。呼び出しは、子と、子が起動した tzroutine がすべて終わってから戻ります。型・借用の規則・診断は [`Task.scope`](../async-tasks-and-lazy/task.md) と同じです（`shared` は `Sync`、結果は `Send` の所有値、コールバックは借用した環境を持てない、直接の呼び出しだけ）。

```tsuzuri run=partial%3D1225%2C1250%2C1275%2C1300%20total%3D5050
def stride_sum :: ref [i64] -> i64 -> i64
fn stride_sum numbers start =
    let mut total = 0;
    let mut index = start;
    while index < numbers.length do
        total = total + numbers[index];
        index = index + 4
    total

let numbers = new [i64](100, \index -> index + 1)
let partial = Routine.scope (ref numbers) 4 (\shared part -> stride_sum shared part)
$"partial={partial[0]},{partial[1]},{partial[2]},{partial[3]} total={Array.sum (ref partial)}"
```

実行結果:

```text
partial=1225,1250,1275,1300 total=5050
```

`Task.scope` の子はスレッドプールの仕事なので、待つ子がプールのスレッドの数を超えると終わりません。`Routine.scope` の子は tzroutine なので、互いを待っても、`TSUZURI_MAXPROCS=1` でも終わります。次の 8 個の子は、輪になったチャンネルで値を 1 周回します。

```tsuzuri run=8
def received :: ref Channel<i64> -> i64
fn received channel =
    match Channel.recv channel with
    | Maybe.Some value -> value
    | Maybe.None -> 0

def pass_on :: ref [Channel<i64>] -> i64 -> i64
fn pass_on channels index =
    if index == 0 then {
        Channel.send (ref channels[1]) 1;
        received (ref channels[0])
    } else {
        let value = received (ref channels[index]) + 1;
        Channel.send (ref channels[(index + 1) % channels.length]) value;
        value
    }

let ring = new [Channel<i64>](8, \_index -> Channel.make 0)
let results = Routine.scope (ref ring) 8 (\channels index -> pass_on channels index)
results[0]
```

実行結果:

```text
8
```

- `count` が 0 なら空の配列です。負なら結果の確保でトラップします（`Task.scope` と同じ）。
- 子が作ったタイマーは、呼び出し元の境界のものです（`Routine.scope` の終わりでは止まりません）。
- tzroutine を起動するので、`pure def` の中では使えず（`E1029`）、WebAssembly では `E2000` です。

## 引数の順序

`Routine` の関数は対象（操作する値）を取らないか、引数が 1 つだけなので、[引数の順序](./argument-order.md) の規則の影響を受けません。

## 他の言語との比較

| 項目 | Tsuzuri | Go | Kotlin |
| --- | --- | --- | --- |
| 起動 | `tzroutine { f x }`、`Routine.go f` | `go f(x)` | `launch { f(x) }` |
| 子を待つ | `Routine.scope shared n f` | goroutine と `sync.WaitGroup` | `coroutineScope { … }` |
| 譲る | `Routine.yield ()` | `runtime.Gosched()` | `yield()` |
| 数 | `Routine.count ()` | `runtime.NumGoroutine()` | — |
| 並列度 | `Routine.max_procs ()`（`TSUZURI_MAXPROCS`） | `runtime.GOMAXPROCS(0)`（`GOMAXPROCS`） | ディスパッチャーのスレッド数 |

## まとめ

- `Routine.go` は所有する関数を tzroutine で 1 回呼びます。
- `Routine.scope` は借用を共有する子を tzroutine で呼び、全員を待ちます（`Task.scope` の後継）。
- `Routine.yield`・`count`・`max_procs`・`cpu_count` は、譲渡とスケジューラーへの問い合わせです。
- 予約されたモジュール名で、std のファイルはありません。

## 関連項目

- [tzroutine 式](../async-tasks-and-lazy/tzroutine.md)
- [Owned](./owned.md)
- [Atomic](./atomic.md)
- [言語仕様（tzroutine）](../../../docs/language.md#tzroutine)
- [言語リファレンスの目次](../index.md)
