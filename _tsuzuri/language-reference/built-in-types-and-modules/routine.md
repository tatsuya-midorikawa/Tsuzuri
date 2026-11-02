# Routine

`Routine` は、[tzroutine 式](../async-tasks-and-lazy/tzroutine.md) の実行を扱う組み込みのモジュールです。所有する関数を tzroutine として起動する `go` と、実行の譲渡、スケジューラーへの問い合わせを持ちます。std のファイルはなく、`Routine` は予約されたモジュール名です（`Routine.tz` というファイルは `E1011`）。

## この記事のポイント

- `Routine.go` は `Owned.Function<unit, unit>` を新しい tzroutine で 1 回呼び、そのあと drop します。`tzroutine { }` の関数版です。
- `Routine.yield ()` は、実行をほかの tzroutine に譲ります（Go の `runtime.Gosched`）。
- `Routine.count ()` は、生きている tzroutine の数に、呼んだスレッドの 1 を足した数です（Go の `runtime.NumGoroutine` は `main` を数えます）。
- `Routine.max_procs ()` は同時に tzroutine を走らせる OS スレッドの数、`Routine.cpu_count ()` はオンラインの CPU 数です。
- どれも関数値として渡せます（`let f: unit -> unit = Routine.yield`）。`pure def` の中では使えません（`E1029`）。

## 提供 API 一覧

| 名前 | 型シグネチャ | 説明 | Go |
| --- | --- | --- | --- |
| `Routine.go` | `own Owned.Function<unit, unit> -> unit` | 所有する関数を新しい tzroutine で 1 回呼び、そのあと drop する | `go f()` |
| `Routine.yield` | `unit -> unit` | 実行を譲る。tzroutine でないスレッドでは OS へ譲る | `runtime.Gosched()` |
| `Routine.count` | `unit -> i64` | 生きている tzroutine の数 + 1（呼んだスレッドの分） | `runtime.NumGoroutine()` |
| `Routine.max_procs` | `unit -> i64` | 同時に tzroutine を走らせる OS スレッドの数（`TSUZURI_MAXPROCS`、既定は CPU 数で 256 まで） | `runtime.GOMAXPROCS(0)` |
| `Routine.cpu_count` | `unit -> i64` | オンラインの CPU 数（1 以上） | `runtime.NumCPU()` |

`Routine.go` で起動した tzroutine も、`tzroutine { }` と同じく、それを起動した境界（`main`・`export` の呼び出し・`test`・`bench`）が終わりを待ちます。

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

## 引数の順序

`Routine` の関数は対象（操作する値）を取らないか、引数が 1 つだけなので、[引数の順序](./argument-order.md) の規則の影響を受けません。

## 他の言語との比較

| 項目 | Tsuzuri | Go | Kotlin |
| --- | --- | --- | --- |
| 起動 | `tzroutine { f x }`、`Routine.go f` | `go f(x)` | `launch { f(x) }` |
| 譲る | `Routine.yield ()` | `runtime.Gosched()` | `yield()` |
| 数 | `Routine.count ()` | `runtime.NumGoroutine()` | — |
| 並列度 | `Routine.max_procs ()`（`TSUZURI_MAXPROCS`） | `runtime.GOMAXPROCS(0)`（`GOMAXPROCS`） | ディスパッチャーのスレッド数 |

## まとめ

- `Routine.go` は所有する関数を tzroutine で 1 回呼びます。
- `Routine.yield`・`count`・`max_procs`・`cpu_count` は、譲渡とスケジューラーへの問い合わせです。
- 予約されたモジュール名で、std のファイルはありません。

## 関連項目

- [tzroutine 式](../async-tasks-and-lazy/tzroutine.md)
- [Owned](./owned.md)
- [Atomic](./atomic.md)
- [言語仕様（tzroutine）](../../../docs/language.md#tzroutine)
- [言語リファレンスの目次](../index.md)
