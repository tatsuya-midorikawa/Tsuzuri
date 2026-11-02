# Time

`Time` は、単調時計、UNIX 時刻、スリープと、Go の `time` パッケージと同じタイマー（`Time.after`・`Time.tick`・`Time.after_func`）を扱うモジュールです。時計を読む操作は `IO` アクションとして提供され、純粋関数の中から暗黙のうちに現在時刻を読み取ることはできません（`Time.after_func` だけは関数を 1 回しか呼ばないので、`Routine.go` と同じく呼んだ時点で始まります）。

必要な権限: `time`（すべての関数）。権限の宣言と検査は [権限](../organizing-tsuzuri/packages.md#権限capabilities) を見てください。止められるタイマーは [Timer](./timer.md) と [Ticker](./ticker.md) です。

## この記事のポイント

- `monotonic_ns` は単調増加する時計です。起点は規定されていないため、2 点間の差分のみが意味を持ちます。
- `unix_ns` は 1970-01-01 UTC からの経過ナノ秒です。`i64` の表現範囲に収まらない場合は `Other` エラーとなります。
- `sleep_ms 0` は待機せずに `Ok ()` を返します。負の値を渡すと `InvalidInput` エラーとなります。
- 時刻の数値そのものは実行のたびに変化するため、テストや照合では差分や大小関係を検証します。
- 長さはナノ秒の `i64` です（Go の `time.Duration`）。`50 * Time.millisecond` のように定数と掛けて書きます。
- `Time.sleep` は tzroutine の中では、その tzroutine だけを待たせます（ほかの tzroutine はそのスレッドで走り続けます）。`Time.sleep_ms` も tzroutine の中では同じ待ち方です。 <!-- rule: TSK-023 -->
- `Time.after` は一定時間後に時刻を 1 つ受け取るチャンネルで、`select` に書いてタイムアウトにします。`Time.tick` は周期ごとに時刻を送るチャンネルで、ハンドルがなくなれば止まります。
- `main`・`export`・`test`・`bench` の終わりは、その中で作ったまだ発火していないタイマーを待たずに止め、解放します。 <!-- rule: TSK-025 -->

## 経過を測る

```mermaid
flowchart LR
    before["monotonic_ns"] --> work["測りたい処理"]
    work --> after["もう一度 monotonic_ns"]
    after --> diff["後から前を引く"]
```

スリープを挟まなくても、2 回の読みの差は 0 以上です。時計が戻らない、という契約の確認になります。

```tsuzuri run=true%0Aslept-zero%0Atrue%0Ainvalid%20input
def main :: unit -> i32 = \() ->
    let! before = Time.monotonic_ns ()
    let! slept = Time.sleep_ms 0
    let! after = Time.monotonic_ns ()
    let! unix = Time.unix_ns ()
    let! negative = Time.sleep_ms (-1)
    do! IO.write_line (match (before, after) with
        | (Result.Ok first, Result.Ok second) -> to_string (second - first >= 0)
        | _ -> "clock-failed")
    do! IO.write_line (match slept with
        | Result.Ok _ -> "slept-zero"
        | Result.Error error -> Os.message (ref error))
    do! IO.write_line (match unix with
        | Result.Ok value -> to_string (value > 0)
        | Result.Error _ -> "unix-failed")
    do! IO.write_line (match negative with
        | Result.Ok _ -> "unexpected"
        | Result.Error error -> Os.message (ref error))
    0
```

実行結果:

```text
true
slept-zero
true
invalid input
```

`sleep_ms` に正の値を渡すと、少なくともそのミリ秒は現在のスレッドを止めます。ドキュメントの照合を遅らせないため、上の例では `0` だけを実行しています。`50` を渡せば、そのあいだは次の `let!` へ進みません。

`unix_ns` は壁時計です。利用者が時計を戻すと、値も戻ることがあります。経過時間には `monotonic_ns` を使ってください。ファイルの `modified_ns` も UNIX 時刻ですが、こちらは [File](./file.md) のメタデータです。範囲外は丸められ、`unix_ns` の範囲外は `Other` です。同じ「ナノ秒」でも、失敗の仕方が違います。

読み取りに失敗すると、`Os.Error { kind: Other, code: 0 }` になります。`Os.message` は `other` です。

差を取る 2 回の読みは、別々のアクションです。先に `let pending = Time.monotonic_ns ()` と置いただけでは、まだ時計を読んでいません。`let!` した時点の値です。待っている間に他の `IO` を実行すると、その時間も差に入ります。測りたい処理だけを 2 回の読みのあいだに置いてください。

## 長さの定数

長さはナノ秒の `i64` です。定数と掛けて書きます。

| 定数 | 値（ナノ秒） | Go |
| --- | --- | --- |
| `Time.nanosecond` | 1 | `time.Nanosecond` |
| `Time.microsecond` | 1,000 | `time.Microsecond` |
| `Time.millisecond` | 1,000,000 | `time.Millisecond` |
| `Time.second` | 1,000,000,000 | `time.Second` |
| `Time.minute` | 60,000,000,000 | `time.Minute` |
| `Time.hour` | 3,600,000,000,000 | `time.Hour` |

```tsuzuri run=1500
Time.second / Time.millisecond + 500
```

実行結果:

```text
1500
```

## tzroutine の中で待つ

`Time.sleep d` は、少なくとも `d` ナノ秒待ちます（Go の `time.Sleep`）。[tzroutine](../async-tasks-and-lazy/tzroutine.md) の中では、その tzroutine だけが待ち、OS スレッドはほかの tzroutine を走らせます。`main` などのスレッドでは、そのスレッドが止まります。`d` が 0 以下なら、`Routine.yield` と同じく 1 回だけ譲ります。

次の例では、3 つの tzroutine が 30・10・20 ms 眠ってから自分の番号を送ります。受け取る順は眠る時間の順です。

```tsuzuri run=231
def order :: unit -> i64
fn order _ =
    let arrivals: Channel<i64> = Channel.make 3
    for number in 1 ..= 3 do
        let out = Channel.share arrivals
        let nap = match number with
            | 1 -> 30
            | 2 -> 10
            | _ -> 20
        tzroutine {
            do! Time.sleep (nap * Time.millisecond)
            Channel.send out (number as i64)
        }
    let mut result = 0i64
    for _ in 1 ..= 3 do
        result = result * 10 + (match Channel.recv arrivals with
            | Maybe.Some number -> number
            | Maybe.None -> 0)
    result

order ()
```

実行結果:

```text
231
```

普通の関数の本体（ビルダーのない本体）に書いた `let!`・`do!` は、その場でアクションを実行します。上の `order` は `IO` を返さない関数のまま、tzroutine の中で `Time.sleep` を待てます。

既存の `Time.sleep_ms` も、tzroutine の中では `Time.sleep` と同じく、その tzroutine だけを待たせます（スレッドを止めません）。結果の型と、負の数が `InvalidInput` になることは変わりません。

## タイムアウト（Time.after）

`Time.after d` は、`d` ナノ秒後にそのときの時刻（`monotonic_ns` と同じ時計のナノ秒）を 1 つ受け取れる、容量 1 のチャンネルを作ります（Go の `time.After`）。[select](../async-tasks-and-lazy/select.md) の腕に書くと、ほかの腕を待つのをあきらめる時刻になります。

```tsuzuri run=timeout
let! deadline = Time.after (20 * Time.millisecond)
let never: Channel<i64> = Channel.make 0
let result =
    select
    | recv _ from never -> "value"
    | recv _ from deadline -> "timeout"
IO.write_line result
```

実行結果:

```text
timeout
```

`Time.after` は `IO` を返すので、`select` の腕に直接は書けません。`let! deadline = Time.after d` で受けてから腕に書きます。トップレベルでは、`let!` の後ろの文は継続の関数になり、その関数は所有値（`Channel` など）を捕捉できません（`E1005`）。上の例のように、タイマーを先に `let!` するか、関数の中に書きます。

## 周期（Time.tick）

`Time.tick d` は、`d` ナノ秒ごとに時刻を送るチャンネルを作ります（Go の `time.Tick`）。チャンネルは 1 つだけ時刻を持ち、受け手が前の時刻を受け取っていなければ、その回は捨てられます。返したチャンネルのハンドルがすべて drop されると、次の周期で止まり、チャンネルを解放します。`d` が 0 以下ならトラップします。止めたり周期を変えたりするには [Ticker](./ticker.md) を使います。

```tsuzuri run=true
let! ticks = Time.tick (2 * Time.millisecond)
let first = Channel.recv ticks
let second = Channel.recv ticks
match (first, second) with
| (Maybe.Some earlier, Maybe.Some later) -> IO.write_line (later > earlier)
| _ -> IO.write_line "closed"
```

実行結果:

```text
true
```

## 遅らせて呼ぶ（Time.after_func）

`Time.after_func d f` は、`d` ナノ秒後に、所有する関数 `f`（[Owned](./owned.md)）を新しい tzroutine で 1 回呼びます（Go の `time.AfterFunc`）。返した [Timer](./timer.md) で止められます。関数は 1 回しか呼べないので、`IO` を返さずに呼んだ時点で始まります（`Routine.go` と同じ）。

```tsuzuri run=called
let done: Channel<i64> = Channel.make 1
let signal = Channel.share done
let _timer = Time.after_func (5 * Time.millisecond) (Owned.function (\() -> Channel.send signal 1))
match Channel.recv done with
| Maybe.Some _ -> "called"
| Maybe.None -> "not called"
```

実行結果:

```text
called
```

`Timer.stop` で止めた関数は、`Timer.reset` で予定し直せます。関数は、呼ばれるか、境界で止められるか、`Timer` が drop されたときに drop されます。一度呼んだ後の `Timer.reset` は偽を返し、何も予定しません（Go は同じ関数をもう一度呼びます）。

## 境界でのタイマー

`main`・`export`・`test`・`bench`（と `Task` のプールの項目）の終わりは、その中で作られた tzroutine の終わりを待ってから、まだ発火していないタイマーをすべて止め、ランタイムが持つ参照とチャンネルを解放します。タイマーは待ちません。1 時間後のタイマーを作って `main` が終わっても、プログラムはすぐ終わります（Go の `main` と同じ）。`Time.after_func` の関数は呼ばれずに drop されます。`Timer`・`Ticker` のハンドルは境界の外へ出られ、止められたまま drop を待ちます。

```tsuzuri run=done
def leave :: unit -> string
fn leave _ =
    let! _after = Time.after Time.hour
    let! _ticks = Time.tick Time.hour
    let _func = Time.after_func Time.hour (Owned.function (\() -> ()))
    "done"

leave ()
```

実行結果:

```text
done
```

## 公開 API

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `Time.monotonic_ns` | `unit -> IO<Result<i64, Os.Error>>` | 戻らない時計のナノ秒。差だけを使う |
| `Time.unix_ns` | `unit -> IO<Result<i64, Os.Error>>` | 1970-01-01 UTC からのナノ秒。範囲外は `Other` |
| `Time.sleep_ms` | `i64 -> IO<Result<unit, Os.Error>>` | 少なくともそのミリ秒待つ。`0` は即 `Ok`。負は `InvalidInput`。tzroutine の中ではその tzroutine だけが待つ |
| `Time.sleep` | `i64 -> IO<unit>` | 少なくともそのナノ秒待つ。0 以下は 1 回譲る |
| `Time.after` | `i64 -> IO<Channel<i64>>` | そのナノ秒後に時刻を 1 つ受け取れる容量 1 のチャンネル |
| `Time.tick` | `i64 -> IO<Channel<i64>>` | 周期ごとに時刻を送るチャンネル。ハンドルがすべてなくなれば止まる。0 以下はトラップ |
| `Time.after_func` | `i64 -> Owned.Function<unit, unit> -> Timer` | そのナノ秒後に関数を新しい tzroutine で 1 回呼ぶ |
| `Time.nanosecond` ～ `Time.hour` | `i64` | 長さの定数 |

呼び出しは `Time.sleep_ms 0` や `Time.monotonic_ns ()` のように記述し、`unit` 引数には `()` を渡します。

## 引数の順序

`Time.after_func d f` は Go の `AfterFunc(d, f)` と同じく、関数を最後に取ります。ほかの関数は引数が 1 つです。[引数の順序](./argument-order.md)を見てください。

## Go との対応

| Go | Tsuzuri |
| --- | --- |
| `time.Sleep(d)` | `Time.sleep d` |
| `time.After(d)` | `Time.after d` |
| `time.Tick(d)` | `Time.tick d` |
| `time.AfterFunc(d, f)` | `Time.after_func d f` |
| `time.NewTimer(d)` | [`Timer.create d`](./timer.md) |
| `time.NewTicker(d)` | [`Ticker.create d`](./ticker.md) |
| `time.Millisecond` など | `Time.millisecond` など |

Go の `time.Time`（壁時計の時刻・書式・時差）に当たる型はありません。時刻はナノ秒の `i64` です。

## 所有権と対応環境

戻り値の `i64` は `Copy` です。アクション自体は実行するまで時計を読みません。同じ `IO` 値を 2 回実行すると、その都度時計を読み取ります。

既定の wasm32 で `monotonic_ns`・`unix_ns`・`sleep_ms` に到達すると、ビルドは `E2000` で失敗します。`--wasm-host wasi` では WASI の時計とポーリングに下がります。`Time.sleep` は WebAssembly では `sleep_ms` と同じ待ち（ミリ秒に切り上げ、`--wasm-host` が必要）です。`Time.after`・`Time.tick`・`Time.after_func`・`Timer`・`Ticker` は tzroutine のスケジューラーのタイマーを使うので、WebAssembly では tzroutine と同じく `E2000`（`--wasm-feature jspi` が必要。まだ実装されていません）、`--trap-mode return` とも組み合わせられません（`E2000`）。

Windows のネイティブビルドは、`monotonic_ns`・`unix_ns`・`sleep_ms` の OS API に到達すると `E2002` で失敗します。`Time.sleep`・`Time.after`・`Time.tick`・`Time.after_func`・`Timer`・`Ticker` は tzroutine のランタイムで動くので、Windows でも使えます（時計は `QueryPerformanceCounter`）。

並行 `Task` の中から、これらのアクションを直接実行することはできません。測定はメインスレッド（エントリーポイント）で行います。

## 他の言語との比較

| | Tsuzuri | 近いもの |
| --- | --- | --- |
| 経過 | `monotonic_ns` の差。起点は未規定 | Rust の `Instant`、C# の `Stopwatch` |
| 壁時計 | `unix_ns`。戻り得る | `SystemTime`、`DateTimeOffset` の UTC |
| 待機 | `sleep_ms`。負はエラー、`0` は即戻る。`sleep` は tzroutine だけを待たせる | `Thread.Sleep`、Go の `time.Sleep` |
| タイムアウト | `Time.after` を `select` の腕に書く | Go の `time.After`、C# の `Task.Delay` と `WhenAny` |

日時をカレンダーの年月日へ分ける関数は、標準ライブラリにはありません。ナノ秒のまま差を取るか、自分で割り算します。

## まとめ

- 経過時間は `monotonic_ns` の差です。起点には意味がありません。
- 日時を扱うなら `unix_ns` です。調整で戻ることがあります。
- `sleep_ms` の `0` は待たず、負は `InvalidInput` です。
- 数値そのものは毎回変わるので、比較や差で確認します。
- 時計の読み取りは `IO` の実行時だけです。
- 長さはナノ秒の `i64` で、`Time.millisecond` などの定数と掛けて書きます。
- `Time.after` を `select` に書くとタイムアウト、`Time.tick` は周期の処理です。境界はまだ発火していないタイマーを待たずに止めます。

## 関連項目

- [IO](./io.md)
- [Timer](./timer.md)
- [Ticker](./ticker.md)
- [Channel](./channel.md)
- [select](../async-tasks-and-lazy/select.md)
- [tzroutine](../async-tasks-and-lazy/tzroutine.md)
- [Os](./os.md)
- [File](./file.md)
- [言語仕様の環境・時刻・乱数・プロセス](../../../docs/language.md#環境時刻乱数プロセス)
- [言語リファレンスの目次](../index.md)
