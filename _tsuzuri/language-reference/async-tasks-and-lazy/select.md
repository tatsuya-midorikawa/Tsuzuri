# select 式

`select` は、複数の [Channel](../built-in-types-and-modules/channel.md) の送受信を同時に待ち、先にできるようになった 1 つの腕を実行する式です。Go の `select` に当たり、見た目は `match` と同じ `|` の腕の列です。

## この記事のポイント

- 腕の頭は `recv x from ch`（受け取る）、`send v to ch`（送る）、`default`（どれもすぐにはできないとき）の 3 つです。
- `recv x` の `x` は `Maybe<T>` です。`Some v` は受け取った値、`None` はチャンネルが閉じて空だったことです。`_` と書くと、受け取った値をすぐに drop します。
- すぐにできる腕が複数あれば、一様乱数で 1 つを選びます。どれもできず `default` があれば `default` へ、なければどれかができるまで待ちます。 <!-- rule: TSK-020 -->
- 腕の `when 条件` が偽の腕は、選ばれず、チャンネルと送る値の式も評価されません（Go の nil チャンネルの代わり）。 <!-- rule: TSK-021 -->
- 送る腕の値は、その腕が選ばれなくても `select` に移され、選ばれなければ 1 回だけ drop されます。 <!-- rule: TSK-022 -->
- `select` は、直後に `|` と `recv`・`send`・`default` が続くときだけの文脈キーワードです。ほかの位置では変数名に使えます。

## 基本の書き方

2 つのチャンネルの値を受け取り、`quit` が閉じたら止めます。各チャンネルの中の順は保たれるので、最後に受け取る値は決まります。

```tsuzuri run=a%3D3%20b%3D2%20quit
let a: Channel<i64> = Channel.make 0
let b: Channel<i64> = Channel.make 0
let quit: Channel<unit> = Channel.make 0
let a2 = Channel.share a
let b2 = Channel.share b
let quit2 = Channel.share quit
tzroutine {
    for i in 1 ..= 3 do
        Channel.send a2 (i as i64)
    for i in 1 ..= 2 do
        Channel.send b2 (i as i64)
    Channel.close quit2
}
let mut last_a = 0i64
let mut last_b = 0i64
let mut running = true
while running do
    select
    | recv x from a -> match x with | Maybe.Some v -> last_a = v | Maybe.None -> ()
    | recv y from b -> match y with | Maybe.Some v -> last_b = v | Maybe.None -> ()
    | recv _ from quit -> running = false
$"a={last_a} b={last_b} quit"
```

実行結果:

```text
a=3 b=2 quit
```

## 腕の形

| 腕 | 意味 | Go |
| --- | --- | --- |
| `\| recv x from ch -> 本体` | `ch` から受け取れたら選ばれる。`x : Maybe<T>` | `case v, ok := <-ch:` |
| `\| recv _ from ch -> 本体` | 同じ。受け取った値は本体の前に drop する | `case <-ch:` |
| `\| send v to ch -> 本体` | `ch` へ送れたら選ばれる | `case ch <- v:` |
| `\| default -> 本体` | どの腕もすぐにはできないとき | `default:` |

- `ch` は `Channel<T>` の場所か参照です（呼び出しの引数と同じく暗黙に借ります）。受け取る腕には `ReceiveOnly<T>`、送る腕には `SendOnly<T>` も書けます。その場で作ったチャンネル（関数の戻り値など）は書けません（`E1013`、`bind the channel to a name first`）。
- 腕は 1 つ以上で、`default` は 0 か 1 つです（2 つ以上は `E0002`、`select_default_repeated`）。`default` の位置は自由です。`default` には `when` を付けられません。
- 腕の頭がほかの形なら `E0002`（`select_arm_expected`）。腕の数の上限は 65,535 です（`E1017`、`select_arm_limit`）。
- すべての腕の本体（`default` を含む）は同じ型で、それが `select` の型です（`match` と同じ）。違えば `E1003` です。
- チャンネルでない式は `E1003`（`expected Channel<…>, found …`）、`when` の条件は `bool` です。

## 待たない試行

`default` があると、`select` は待ちません。

```tsuzuri run=idle%3D42
let jobs: Channel<i64> = Channel.make 1
let got = select
    | recv job from jobs -> match job with | Maybe.Some v -> v | Maybe.None -> 0
    | default -> 42
$"idle={got}"
```

実行結果:

```text
idle=42
```

1 つのチャンネルだけを待たずに試すなら、`Channel.try_send`・`Channel.try_recv` でも書けます。

## when で腕を止める

`when` が偽の腕は、ないものとして扱われます。チャンネルの式も送る値の式も評価しません。Go で腕のチャンネルを `nil` にして止める書き方の代わりです。

```tsuzuri run=sent%3D3%20sum%3D6
let input: Channel<i64> = Channel.make 0
let output: Channel<i64> = Channel.make 0
let from_main = Channel.share input
let to_main = Channel.share output
tzroutine {
    for value in from_main do
        Channel.send to_main value
    Channel.close to_main
}
let mut sent = 0i64
let mut got = 0i64
let mut sum = 0i64
while got < 3 do
    select
    | send sent + 1 to input when sent < 3 -> sent = sent + 1
    | recv value from output -> match value with
        | Maybe.Some v ->
            sum = sum + v
            got = got + 1
        | Maybe.None -> got = 3
Channel.close input
$"sent={sent} sum={sum}"
```

実行結果:

```text
sent=3 sum=6
```

有効な腕が 1 つもなく `default` もない `select` は、永久に待ちます（Go の nil チャンネルだけの `select`）。全員が待てば、デッドロックの判定が `all tzroutines are asleep - deadlock! (1 waiting: select (no cases))` を書いてトラップします。

## 評価の順序と所有権

1. 腕を上から順に見ます。`when` があれば評価し、偽ならその腕を無効にします。有効な腕は、チャンネルの式（借用）、送る腕なら値の式（移す）の順に評価します。
2. 有効な腕のうち、すぐにできるもの（受け取る腕: 要素がある・送り手が待っている・閉じている、送る腕: 受け手が待っている・空きがある・閉じている）から 1 つを一様乱数で選びます。どれもなければ `default`、`default` もなければ待ちます。
3. 選ばれなかった送る腕の値を drop します。値を失いたくないときは `Channel.try_send` を使います。
4. 選ばれた腕が閉じたチャンネルへの送信なら、`Tsuzuri runtime: send on closed Channel` を書いてトラップします（`Channel.send` と同じ）。
5. 選ばれた腕の本体を実行します。

- 送る値は `select` に移されるので、`select` のあとでは使えません（`E1012`）。送る値は、`Channel.send` の要素と同じく借用を持てません（`E1013`、`channel_item_holds_borrow`）。
- 本体の中の `break`・`continue` は、外のループのものです（`select` 自体はループではありません）。
- `select` は tzroutine の待ちです。tzroutine を起動するか `Channel` を使うプログラムでは、`Mutex.with_lock` の中の `default` のない `select` はトラップします。

## 公平さ

すぐにできる腕は、同じ確率で選ばれます（再現用の種はありません）。いつも準備のできた 2 つのチャンネルからの `select` は、どちらも半分ほど選びます。

## ターゲット

ネイティブの macOS・Linux・Windows で動きます。既定の WebAssembly では、tzroutine を起動しないプログラムの `select` が 1 本の実行として動きます。待つべき `select`（`default` がなく、どの腕もすぐにはできない）はその場でトラップします。

## 表現と性能

- 腕の情報（チャンネル、要素の場所、送るかどうか。1 つ 72 バイト）は、関数の入口に置いたスタックの領域に並べます（ループの中の `select` でもスタックは伸びません）。65 個以上の腕はヒープに置きます。
- ランタイムは、腕のチャンネルのロックをアドレスの順に 1 回ずつ取り、腕を乱数の順に調べます。費用は腕の数に比例します。

## 他の言語との比較

| 項目 | Tsuzuri | Go | Rust（crossbeam） | Kotlin |
| --- | --- | --- | --- | --- |
| 受け取る | `\| recv v from ch ->` | `case v, ok := <-ch:` | `recv(ch) -> v =>` | `ch.onReceiveCatching { }` |
| 送る | `\| send v to ch ->` | `case ch <- v:` | `send(ch, v) -> r =>` | `ch.onSend(v) { }` |
| 待たない | `\| default ->` | `default:` | `default =>` | `onTimeout(0)` |
| 腕を止める | `when 条件` | チャンネルを `nil` に | — | — |

## まとめ

- `select` は、複数のチャンネルの送受信のうち、先にできる 1 つを実行します。
- `when` で腕を止め、`default` で待たずに試します。
- 選ばれなかった送る値は drop されます。

## 関連項目

- [Channel](../built-in-types-and-modules/channel.md)
- [tzroutine 式](./tzroutine.md)
- [match 式](../pattern-matching/match.md)
- [言語リファレンスの目次](../index.md)
