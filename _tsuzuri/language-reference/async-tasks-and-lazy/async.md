# Async 式

`Async<'a>` は、1 つのスレッドの上で、中断点ごとにほかの計算へ順番を譲りながら進む**コールド**な非同期計算です。標準ライブラリの `Async` モジュールが、型、`Async { ... }` のビルダー、実行器を提供します。

作っただけでは何も起きません。次の 3 つの実行器のどれかに渡して初めて動きます。

- `Async.run`: 仮想時刻で最後まで同期的に実行し、結果を返します。ホストもスレッドも使いません。
- `Async.block_on`: 実時間で待ちながら実行します。待っている間はスレッドを止めます。
- `Async.start`: ホストのイベントループが駆動する実行器へ渡し、すぐ戻ります。

CPU のコアを並列に使う [Task 式](task.md) とは目的が違います。`Task` は同期的なフォーク・ジョインです。`Async` は、待ち時間をほかの計算へ譲る協調的な中断です。

## この記事のポイント

- `Async { ... }` で `Async<T>` を作ります。`let!`、`do!`、`return`、`return!`、`match!`、`if`、`for` を書けます。小文字の `async { ... }` も同じ意味です（`Async.tc` が `@alias async` を宣言しています）。
- `Async.yield ()` は中断点を 1 つ作ります。`Async.sleep n` は時刻が `n` 進むまで中断し、`Async.now ()` は今の時刻を返します。
- `Async.run` の時計は 0 から始まります。すべての計算が眠ったときだけ、いちばん早い起床時刻まで進みます。
- `Async.all` と `Async.all_results` は、子を入力の順に交代で進めます。結果は入力の順です。`all_results` は最初の `Error` で残りを止めます。
- `Async<T>` は不透明で非 Copy です。二重に使うと `E1012`、中断をまたいで借用を持つと `E1013` です。
- `Async.block_on` は単調時計のミリ秒で待ちます。ネイティブは macOS、Linux、Windows です。wasm32 / wasm64 では `--wasm-feature jspi` が要ります。
- `Async.start` と `Async.host` を使うと、ホストのイベントループが計算を進め、ホストの操作が終わったところで再開します。C からは `tsuzuri_async_poll` と `tsuzuri_async_complete` を呼びます。JavaScript では、生成したグルーの `bindings.async` を使います。

## 基本の書き方

```tsuzuri run=42
let work = Async {
    do! Async.sleep 10
    let! time = Async.now ()
    return time + 32
}
Async.run work
```

実行結果:

```text
42
```

`work` を作った時点では、何も実行されません。`Async.run` が最初から最後まで進めます。`Async.run` の時計は 0 から始まるので、`sleep 10` のあとの `now ()` は 10 です。

`Async { ... }` は `async { ... }` とも書けます。小文字の綴りはビルダーの別名で、展開も生成されるコードも同じです。

```tsuzuri run=42
let work = async {
    do! Async.sleep 10
    let! time = Async.now ()
    return time + 32
}
Async.run work
```

実行結果:

```text
42
```

別名はビルダーを呼ぶ位置の `{` の前だけで使えます。`Async.run` や `Async<T>` の `Async` は、これまでどおりモジュール名と型名です（`async.run` とは書けません）。別名の規則は [ビルダーの別名](../computation-expressions/computation-expressions.md#ビルダーの別名) を参照してください。

| 構文 | 意味 |
| --- | --- |
| `let! x = computation` | `computation: Async<T>` を実行し、結果を `x` に束縛する |
| `do! computation` | `Async<unit>` を実行して、続きへ進む |
| `return value` | そのブロックの結果を作る |
| `return! computation` | 末尾で別の計算を実行し、その結果をこのブロックの結果にする。入れ子を深くしない |
| `match! computation with ...` | 計算の結果で分岐する |
| `if` / `for value in values do` | 条件と反復。`else` のない `if` も書ける |

`Async { ... }` は [コンピュテーション式](../computation-expressions/computation-expressions.md) です。ビルダー `Async` が定義する操作は `Return`、`ReturnFrom`、`Bind`、`Delay`、`Zero`、`Combine`、`For` です。`while`、`and!`、文の `yield` は `E1018`（`computation builder 'Async' does not define 'While'` など）です。繰り返しは `for` か、末尾の `return!` による再帰で書きます。

関数の結果型が `Async<T>` なら、本体の `Async { }` を省略できます。

```tsuzuri run=7
def worker :: i64 -> Async<i64> = \n ->
    do! Async.yield ()
    return n + 1

Async.run (worker 6)
```

実行結果:

```text
7
```

`yield` はキーワードですが、`Async.yield` のようにモジュールの後ろではメンバー名として書けます。`for` の要素は Copy の値に限ります。`string` の配列を回すと `E1005`（`no instance for Copy<string>; define an instance or use a supported type`）です。

## API

| 関数 | シグネチャ | 意味 |
| --- | --- | --- |
| `Async.yield` | `unit -> Async<unit>` | 中断点を 1 つ作る。時刻は進まない |
| `Async.sleep` | `i64 -> Async<unit>` | 時刻が `n` 以上進むまで中断する。`n <= 0` なら `yield` と同じ |
| `Async.now` | `unit -> Async<i64>` | 実行器の今の時刻。中断しない |
| `Async.all` | `Capture<'a> => [Async<'a>] -> Async<['a]>` | 並行に進め、結果を入力の順に返す |
| `Async.all_results` | `(Capture<'a>, Capture<'e>) => [Async<Result<'a, 'e>>] -> Async<Result<['a], 'e>>` | 最初の `Error` で止まる `all` |
| `Async.run` | `Async<'a> -> 'a` | 仮想時刻で最後まで実行する |
| `Async.block_on` | `Async<'a> -> IO<'a>` | 実時間で最後まで実行する |
| `Async.start` | `Async<unit> -> IO<unit>` | ホストが駆動する実行器へ渡し、すぐ戻る |
| `Async.host` | `Async.Operation -> Async<i64>` | ホストの操作を始め、完了の値を待つ |

`Async.Operation` は `record Operation { start: i64 -> unit, cancel: i64 -> unit }` です。ビルダーの操作 `Async.Return`、`Async.Bind` なども公開していますが、普段は `Async { }` から使います。`Async` は、`Async` か別名の `async` の語を書いたプログラムだけが読み込む標準モジュールです。どちらも使わないプログラムの生成コードは変わりません。

## 時刻と順序

### Async.run の仮想時刻

`Async.run` は、計算を最後まで同期的に実行して結果を返します。時計は呼び出しごとに 0 から始まり、次の規則で進みます。

- `Async.yield ()` は中断点を 1 つ作ります。時刻は進みません。
- `Async.sleep n` は、時刻が `now + n` 以上になるまで中断します。起床時刻の足し算は `i64` の最大値で止まります（飽和）。
- `Async.now ()` は今の時刻を返します。中断点ではありません。
- すべての計算が `sleep` 中のときだけ、時計はいちばん早い起床時刻まで一度に進みます。`yield` を続ける計算がある間、眠っている計算は起きません。

計算の中で別の `Async.run` を呼ぶと、内側の計算は独立した時計で最後まで実行されます。外側から見れば普通の関数呼び出しです。同じプログラムは、ネイティブでも WebAssembly でも、`-O0` でも `-O3` でも、同じ順序で進んで同じ結果になります。

### all と all_results

`Async.all` は、子を入力の順に始めます。その後は 1 巡ごとに、続けられる子を入力の順に 1 回ずつ再開します。結果の配列は入力の順です。空の配列は、中断せずに `[]` を返します。結果が Copy でない値（`string` など）でも使えます。

```tsuzuri run=123024
def rec worker :: i64 -> i64 -> i64 -> Async<i64> = \delay count trace -> Async {
    do! Async.sleep delay
    let! tick = Async.now ()
    if count == 1 then return trace * 10 + tick
    else return! worker delay (count - 1) (trace * 10 + tick)
}

let results = Async.run (Async.all [worker 1 3 0, worker 2 2 0])
results[0] * 1000 + results[1]
```

実行結果:

```text
123024
```

時刻は 0、1、2、3、4 と進みます。1 つ目の子は時刻 1、2、3 に、2 つ目は 2、4 に起きるので、記録は `123` と `24` です。

`Async.all_results` は、すべて `Ok` なら値の配列を `Ok` で返します。どれかが `Error` で終わったら、その `Error` をすぐ返します。時刻の早い `Error` が勝ち、同じ巡の中では入力の順です。残りの子は中断した場所で捨て、捕捉していた値を解放します。

```tsuzuri run=-2
def check :: i64 -> i64 -> Async<Result<i64, i64>> = \delay code -> Async {
    do! Async.sleep delay
    if code > 0 then return Result.Error code
    else return Result.Ok delay
}

let outcome = Async.run (Async.all_results [check 3 1, check 1 2, check 5 0])
match outcome with
| Result.Ok values -> values.length
| Result.Error code -> -code
```

実行結果:

```text
-2
```

2 つ目の子が時刻 1 で `Error 2` を返すので、1 つ目と 3 つ目はそこで止まります。最小の入力インデックスの `Error` を返す [Task.parallel_results](task.md#失敗の伝播) と違い、時刻の順で最初の `Error` を返します。中断している子は、その場で止められるからです。

子がホストの操作（[後述](#ホストが駆動する実行asyncstart)）を待っている間に止まったときは、実行器がその操作の `cancel` を呼びます。取り消しトークンはありません。

## 所有権と借用

`Async<T>` は不透明で非 Copy です。同じ値を 2 回使うと `E1012` です。標準ライブラリの外で `start` フィールドを読んだり、`Async { start: ... }` と書いたりすると `E1022` です。

```text
let work = Async.yield ()
let copy = work
Async.run work
```

```text
error[E1012]: use of moved or partially moved value 'work'
```

中断をまたいで借用を持つことはできません。型が `Async<T>` の式の値が借用を持っていると、`E1013` です。

```text
let values = [1, 2, 3]
Async.run (Async {
    let view = ref values
    do! Async.yield ()
    return Array.length view
})
```

```text
error[E1013]: async computations cannot keep borrowed values across 'let!', 'do!', or the start of the computation; move or clone the value into the async block instead
```

借用の引数を `do!` のあとで使う関数（`def total :: ref [i64] -> Async<i64>`）や、`Async.Return (ref values)` も同じエラーです。`Async.start` に渡した計算は呼び出し元より長く生きるので、同じ規則をすべての実行器に当てはめています。

中断より前に終わる借用と、ブロックへ移した所有値は使えます。

```tsuzuri run=36
def total :: [i64] -> Async<i64>
fn total values = Async {
    let count = Array.length (ref values)
    do! Async.yield ()
    return count * 10 + Array.sum (ref values)
}

Async.run (total [1, 2, 3])
```

実行結果:

```text
36
```

`Async { }` のブロックは、ラムダと同じように外の値を捕捉します。Copy の値はコピーし、そうでない値はブロックへ移します。ブロックの中の `ref` は、そのコピーか移した値を借用します。たとえば `let text = "coffee"` を `Async { return String.length (ref text) }` で使うと、`text` はブロックへ移り、そのあと外で使うと `E1012` です。

`Async` の値は借用を持たないので、`Task` へ移せます。`Task` の本体で `Async.run` を呼べます。`Async` 自身はスレッドを使わず、複数のスレッドで共有もしません。

```tsuzuri run=9
let computation = Async {
    do! Async.sleep 2
    let! time = Async.now ()
    return time + 7
}
Task.run (task { return Async.run computation })
```

実行結果:

```text
9
```

## 実時間で待つ（Async.block_on）

`Async.block_on` は、計算を実時間で最後まで実行し、その値を返す `IO` です。時計は単調時計のミリ秒です。どの計算も続けられない間は、次のタイマーか、ホストの操作の完了までスレッドを止めます。`Async.start` で渡した計算も、その間に同じ実行器で進みます。

```tsuzuri run=ok
def span :: i64 -> Async<i64>
fn span delay = Async {
    let! start = Async.now ()
    do! Async.sleep delay
    let! finish = Async.now ()
    return finish - start
}

let! spans = Async.block_on (Async.all [span 5, span 10])
do! IO.write_line (if spans[0] >= 5 && spans[1] >= 10 then "ok" else "early")
```

実行結果:

```text
ok
```

`Async.now ()` は 0 からではなく、単調時計の値を返します。差だけに意味があります。

- ネイティブは macOS、Linux、Windows です。コンパイラが `src/runtime/async.c`（条件変数による待ち）を自動でリンクします。ほかのホスト（FreeBSD などの Unix）は、通常のビルドと test / debug-test / bench で `E2002`（`Async.block_on is only available on macOS, Linux, and Windows; use Async.run on this platform`）です。ほかの実装を推測してリンクしません。
- Windows の待ちは、Win32 の SRWLOCK、条件変数、`QueryPerformanceCounter` を使います。Windows の既定のタイマー分解能は約 15.6 ms（環境で変わります）で、短い `sleep` はその分だけ遅れることがあります。待ちは起きるたびに単調時計を読み直すので、早く起きることはありません。runtime を埋め込む COFF の `--emit object` は非対応で、LLVM IR を出して runtime を 1 回だけリンクするか、実行ファイルを作ります。
- ほかのスレッドからは `tsuzuri_async_post(operation, value)` で操作を完了します。受理したときは 1、未知・二重・完了済み・取り消し済みの操作なら 0 を返し、失敗した post は確保を残しません。どのスレッドから呼んでもよく、待っているスレッドはすぐ起きます。同じスレッドのコールバックの中からは、`tsuzuri_async_complete` も使えます。
- wasm32 / wasm64 では `--wasm-feature jspi` が要ります。モジュールは `tsuzuri_async.clock` と `tsuzuri_async.wait` を import し、待つ間は JavaScript Promise Integration（JSPI）で WebAssembly のスタックを中断します。付けないと `E2000`（`Async.block_on on WebAssembly needs --wasm-feature jspi: ...`）です。生成グルーは wasm32 だけです。詳しくは [WebAssembly への出力](../compiler/webassembly.md#非同期計算と-jspi) を見てください。

## ホストが駆動する実行（Async.start）

`Async.start` は、計算をホストが駆動する実行器へ渡してすぐ戻ります。計算が動くのは、ホストが次に `tsuzuri_async_poll` を呼んだときです。`Async.host` は、ホストの操作を始めて、その完了を待ちます。

```tsuzuri
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

この断片は型検査できます。実行にはホストが要るので、`run=` は付けていません。

計算が `Async.host operation` に達すると、実行器は 1 以上の新しい操作 ID を割り当てて `operation.start` に渡し、計算を中断します。ホストが `tsuzuri_async_complete(id, value)` を呼ぶと、次の poll で `value` が `Async.host` の結果になります。その前に計算が待つのをやめたとき（`all_results` のほかの子が失敗したとき）は、`operation.cancel` に同じ ID を渡します。

ホスト側の関数は、プログラムが実行器に到達したときだけ公開されます。`--emit header` のヘッダーにも宣言が入ります。

| C の関数 | 意味 |
| --- | --- |
| `int64_t tsuzuri_async_poll(int64_t now)` | 0 以上の `now` まで時計を進め（戻しはしない）、完了した操作を届け、poll の開始時に続けられた計算を 1 回ずつ再開する。負の `now` はトラップ。戻り値は次に poll する時刻 |
| `void tsuzuri_async_complete(int64_t operation, int64_t value)` | 操作の完了を記録する。値を届けるのは次の poll。コールバックの中からも呼べる |
| `int32_t tsuzuri_async_post(int64_t operation, int64_t value)` | `Async.block_on` を使うネイティブのプログラムだけ。どのスレッドからでも呼べる。受理で 1、既に終わった操作などの拒否で 0 |

`tsuzuri_async_poll` の戻り値は、計算がなければ `-1`、すぐ続けられる計算があれば実行器の今の時刻、そうでなければいちばん早いタイマーの時刻です。ホストの操作の完了だけを待っているときは `INT64_MAX` です。時刻の単位はホストが決めます。実行器は `now` に渡された数だけを見ます。`Async.block_on` と一緒に使うときは、同じ単調時計のミリ秒を渡します。

待っていない ID、完了済みの ID、取り消した ID を `tsuzuri_async_complete` に渡すとトラップします。`tsuzuri_async_poll` の中から（コールバック経由で）`tsuzuri_async_poll` を呼んでもトラップします。`Async.run` が `Async.host` に達してもトラップします。`Async.run` には、操作を完了させるホストがいないからです。

C のホストの骨組みです。`now_ms` とイベント待ちは、ホストのイベントループのものを使います。

```c
#include "app.h"

void tsuzuri_host_Main_start_download(int64_t operation, int64_t item) {
    /* I/O を始め、終わったらイベントループのスレッドで tsuzuri_async_complete(operation, size) を呼ぶ */
}

void tsuzuri_host_Main_cancel_download(int64_t operation) { /* I/O を止める */ }

void tsuzuri_host_Main_show(int64_t total) { /* 表示する */ }

int main(void) {
    tz_refresh(7);
    for (;;) {
        int64_t next = tsuzuri_async_poll(now_ms());
        if (next < 0) break;
        wait_for_events_until(next); /* 完了した I/O の tsuzuri_async_complete を呼ぶ */
    }
    return 0;
}
```

JavaScript では、`--emit bindings-js` のグルーが実行器を駆動します。グルーは export の呼び出しと `complete` のあと、またいちばん早いタイマーの時刻に、`performance.now()` のミリ秒で poll します。`settled()` は、計算が残っていない状態になると解決します。

ホストの start コールバックから同期的に `complete` を呼ぶこともできます。poll の途中でも、再入的に poll はせず、完了を記録して次に再開します。そのときの駆動タイマーは重複させません。

native の実行器はスレッドごとに独立し、`start` で投入したスレッドから poll します。ホストは、そのスレッドを終える前に計算を最後まで駆動します。ほかのスレッドの poll やスレッドの終了で自動的に続きが動くことはありません。WASM のグルーは、同じ生成モジュール内でインスタンスごとに異なる世代を操作 ID に使います。別のインスタンスや破棄したインスタンスの完了は `TypeError` で拒否し、新しい計算へ渡しません。別々に生成したグルー同士や生の WASM では、操作 ID と実行器をホストが対応付けます。`complete` の操作 ID と値は `i64` の範囲を検査します。実行器の失敗は `settled()` を拒否させ、`ready()` や次の export による再作成まで残ります。

```javascript
import { load } from "./app.mjs";

let api;
api = await load(bytes, { imports: {
  "Main.start_download": (operation, item) => {
    fetch(`/items/${item}`)
      .then((response) => response.arrayBuffer())
      .then((body) => api.async.complete(operation, BigInt(body.byteLength)));
  },
  "Main.cancel_download": (operation) => { /* AbortController などで止める */ },
  "Main.show": (total) => console.log(total),
} });
api.exports.refresh(7n);
await api.async.settled();
```

実行器を使うプログラムには、次の制限があります。

- `--trap-mode return` とは組み合わせられません（`E2000`）。トラップした呼び出しが確保したブロックを境界が解放しても、実行器の状態から届いてしまうからです。
- `--wasm-feature threads` は、実行器を使うプログラムを拒否します（`E2000`）。現在の WASM worker は実行器の TLS を設定しないためです。
- 同じスレッドで `Async.block_on` を重ねて実行することはできません。JSPI のグルーは export を呼び出し順に直列化し、待機をまたぐ入力はコピーします。`withBorrowed` はありません。生の `WebAssembly.promising` を使う場合は、前の呼び出しを await してから次を呼びます。
- native の `test`、デバッグ用 `test`、`bench` でも `Async.block_on` を使えます。WASM のテストランナーはホスト駆動と JSPI を提供しないので、`Async.start` / `Async.block_on` は `E2000` で、`Async.run` を使います。
- トラップは、ほかの呼び出しと同じくプログラム全体の失敗です。グルーはインスタンスを作り直すので、待っていた計算は失われます。

## 制限と性能

- 中断のたびに、継続のクロージャを確保します。非末尾の `let!` が入れ子になっていると、1 回の再開で入れ子の段数分の継続を作り直します。深さ n の非末尾の再帰は、全体で O(n²) です。末尾の `return!` と `for` は入れ子を深くしません。
- 非末尾の `let!` の入れ子は、1 段ごとにスタックを使います。ネイティブでは、`-O0` と `-O3` の両方で 4000 段まで確かめました。WebAssembly の関数のフレームは JavaScript エンジンのスタックに積まれます。Node.js 20 の既定では、`-O0` でおよそ 500 段、`-O3` でおよそ 2000 段で `RangeError: Maximum call stack size exceeded` になりました。エンジンの最適化の段階によって前後します。長い繰り返しは、末尾の `return!` か `for` で書きます。
- 計測（Apple M1 Max、macOS、`-O3`、9 回の中央値）: 末尾の `return!` で 100 万回 `yield` するループは 0.18 秒、最大常駐メモリ約 1.7 MB でした。非末尾の入れ子は、1000 段で 0.03 秒、2000 段で 0.11 秒、4000 段で 0.40 秒でした。条件と生データの保存先は [性能測定](../../../docs/benchmarks.md#async-の中断と継続のコストb08) にあります。
- 計算の中のトラップは、プログラム全体を止めます。計算ごとの隔離や巻き戻しはありません。
- `Async` はスレッドを使いません。CPU の並列は [Task 式](task.md) を使います。

## 他の言語との比較

| 言語 | 構文 / 型 | いつ始まるか | 実行 | Tsuzuri との違い |
| --- | --- | --- | --- | --- |
| Tsuzuri | `Async { ... }` / `Async<T>` | コールド。`run`、`block_on`、`start` | 1 スレッドの協調的な中断。仮想時刻、実時間、ホスト駆動 | 中断をまたぐ借用は `E1013` |
| F# | `async { ... }` / `Async<T>` | コールド | スレッドプール | 形は近い。`Async.Parallel` はスレッドプールで並列に走る |
| Rust | `async` / `await` / `Future` | コールド。poll されて進む | 実行器は外部クレート | `tsuzuri_async_poll` は、ホストが回す実行器に近い。借用を持つ計算は作れない |
| C# | `async` / `await` / `Task` | ホット | スレッドプールと同期コンテキスト | 作った時点で走る |
| JavaScript | `async` / `await` / `Promise` | ホット | イベントループ | JSPI のグルーでは、`block_on` を含む export が `Promise` を返す |

## まとめ

- `Async<T>` はコールドで非 Copy の非同期計算です。`Async { ... }` で作ります。
- `Async.run` は仮想時刻、`Async.block_on` は実時間、`Async.start` はホストの駆動で実行します。
- `yield` は時刻を進めず、`sleep` は全員が眠ったときに時計を進めます。`all` の結果は入力の順で、`all_results` は時刻の順で最初の `Error` を返します。
- 中断をまたぐ借用は `E1013` です。値はブロックへ移すか、中断の前に借用を終えます。
- `Async.host` の操作は、`tsuzuri_async_complete`、`tsuzuri_async_post`、グルーの `bindings.async.complete` で完了させます。

## 関連項目

- [Task 式](task.md)
- [Lazy 式](lazy.md)
- [IO](../built-in-types-and-modules/io.md)
- [コンピュテーション式](../computation-expressions/computation-expressions.md)
- [借用と参照](../ownership-and-memory/borrowing.md)
- [WebAssembly への出力](../compiler/webassembly.md)
- [ネイティブ連携 (C ABI)](../compiler/native-interop.md)
- [言語仕様（非同期計算）](../../../docs/language.md#非同期計算async)
- [言語リファレンスの目次](../index.md)
