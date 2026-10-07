# Async 式

Tsuzuri 0.1.0 には、F# の `async { }` や C# / JavaScript の `async` / `await` に相当する**非同期式はありません（未実装です）**。

今あるのは、CPU 上の計算を同期的にフォーク・ジョインする [Task 式](task.md) と、副作用を値として遅らせる [IO](../built-in-types-and-modules/io.md) です。スレッドをブロックしない非同期 I/O は、[B08: 非同期計算（Async）とホスト駆動の実行](../../../_features/B08-async.md) として計画されています。状態は todo です。0.1.0 では使えません。

## この記事のポイント

- Tsuzuri 0.1.0 には `async { }` も `Async<T>` もありません。
- 複数コアで計算を分けるなら、`task { ... }` と `Task.run` / `Task.parallel` を使います。
- 入出力の順序を値として組み立てるなら、`IO<T>` と `let!` / `do!` を使います。
- `Task` と `IO` は互いに変換できません。`task` の中の `let!` は `IO` を受け付けません。
- イベントループとの連携は、ホスト側（JavaScript や C）に置きます。
- 言語組み込みの非同期計算は計画中です。計画中の構文は、このページの実行例には出てきません。

## なぜ 0.1.0 には Async 式がないのか

`async` / `await` を言語機能にするには、中断点をまたぐスタックの保存、状態機械の生成、中断中も借用が生きていることの保証が要ります。Tsuzuri は所有権と借用をコンパイル時に検査するので、参照を持ったままスレッドを明け渡す操作を、そのままは置けません。

非同期 I/O には、ブラウザや Node.js のイベントループ、Linux の io_uring / epoll、Windows の IOCP のような待ち受けも要ります。0.1.0 はランタイムを小さく保つため、これらを言語コアには入れていません。

役割は次のように分かれます。

| 目的 | 0.1.0 での手段 | 特徴 |
| --- | --- | --- |
| CPU 上の並列計算 | [Task 式](task.md) | コールドで 1 回実行。スレッドプールで同期フォーク・ジョイン |
| 遅らせた入出力 | [IO](../built-in-types-and-modules/io.md) | 副作用を値として包む。実行するまで OS を呼ばない |
| イベント駆動の非同期 I/O | ホスト側 | JavaScript や C のイベントループから、Tsuzuri の同期関数を呼ぶ |

## 今使える手段

### Task 式で計算を分ける

重い計算を複数のコアに分けるときは、[Task 式](task.md) を使います。`task { ... }` が作る `Task<T>` は、作っただけでは始まりません。`Task.run` や、別のタスクの `let!` で消費して初めて動きます。

```tsuzuri run=42
def compute :: i64 -> Task<i64> = \value -> task { return value * 2 }

let work = task {
    let! first = compute 10
    let! second = compute 11
    return first + second
}
Task.run work
```

実行結果:

```text
42
```

`let!` を並べただけでは逐次です。並べて走らせるときは `Task.parallel` を使います。ネイティブではスレッドプールで並列になります。既定の WebAssembly は逐次です。Workers で並列にするには、wasm32 向けに `--wasm-feature threads` を付けてビルドします。詳しくは [Task 式](task.md) を見てください。

```tsuzuri run=14
let jobs = new [Task<i64>](4, \index -> task { return index * index })
let results = Task.run (Task.parallel jobs)
Array.sum ref results
```

実行結果:

```text
14
```

`task` の `let!` / `do!` が受け取るのは `Task` だけです。`IO` を渡すと `E1003` になります。逆に、`IO` の本体へ `Task` を `let!` することもできません。公開の `IO.run` はなく、`IO` を `Task` に変換する関数もありません。

### IO で副作用を組み立てる

入出力の順序を値として残したいときは、[IO](../built-in-types-and-modules/io.md) を使います。アクションを作っただけでは副作用は起きません。`main` の本体や `do!` で実行したときに起きます。

```tsuzuri run=hello
def main :: unit -> i32 = \() ->
    do! IO.write_line "hello"
    0
```

実行結果:

```text
hello
```

これは非同期 I/O ではありません。`IO.write_line` は、実行したスレッドを書き込みが終わるまで止めます。

### ホストのイベントループに渡す

`fetch`、タイマー、UI イベント、ソケットの待ち受けは、ホスト側のイベントループに置きます。Tsuzuri 側は、データが揃ってから呼ぶ同期関数にします。

次の JavaScript は構成のスケッチです。`process_user` というエクスポートが標準で用意されているわけではありません。実際の export 名と引数の渡し方は、[WebAssembly への出力](../compiler/webassembly.md) と [ネイティブ連携 (C ABI)](../compiler/native-interop.md) に従ってください。

```javascript
async function loadUserData(userId) {
    const response = await fetch(`/api/users/${userId}`);
    const data = await response.json();
    return wasmInstance.exports.process_user(data.id, data.score);
}
```

ネイティブでは、`export def` で C ABI の関数を出し、libuv や Tokio などのホストから同期的に呼びます。`Task<T>` 自体は export できません（`E1008`）。

## 将来の計画

I/O 待ちでスレッドを止めない非同期計算は、[B08](../../../_features/B08-async.md) で計画されています。**計画中であり、0.1.0 では使えません。** 承認前の Phase 1 も未着手です。

計画メモに書かれている方向は、おおよそ次のとおりです。構文を試す例は載せていません。

- Phase 1（土台）: 標準の `Async<'a>` とビルダー、協調的な中断点、中断をまたぐ借用の禁止。
- Phase 2 以降（設計のみ。別途承認が要る）: ブラウザの `Promise` や OS のイベントから計算を再開するホスト連携。

名前や API はチケットの承認で変わり得ます。使えるようになるまでは、上の `Task` と `IO` とホスト委譲を使ってください。

## 他の言語との比較

| 言語 | 構文 | 実行モデル | Tsuzuri 0.1.0 との違い |
| --- | --- | --- | --- |
| Tsuzuri | なし（未実装） | 同期フォーク・ジョイン（`Task`）と遅延アクション（`IO`） | `async` はない。CPU 並列は `Task`、副作用は `IO` |
| F# | `async { }` / `task { }` | スレッドプールとタスクスケジューラ | F# の `async` は実行するまで始まらない。F# の `task` は生成時に始まる。Tsuzuri の `Task` は前者に近い |
| Rust | `async` / `await` | ポーリング型の `Future`。実行器は外部クレート | 言語とランタイムを分けている。Tsuzuri は同期タスクだけを標準で持つ |
| C# | `async` / `await` | `Task` とスレッドプール | ランタイムがスレッドプールとタイマーを持つ |
| JavaScript | `async` / `await` | シングルスレッドのイベントループ | I/O 待ちが中心。Tsuzuri の `Task` はその代わりにはならない |

## まとめ

- Tsuzuri 0.1.0 には `async { }` も `Async<T>` もありません。
- CPU の並列計算には、コールドな `Task<T>` を使います。
- 入出力は `IO<T>` で遅らせます。非同期 I/O ではなく、実行中はスレッドを止めます。
- `Task` と `IO` は混ぜられません。イベント待ちはホストに委譲します。
- 非同期計算そのものは B08 として計画中です。

## 関連項目

- [Task 式](task.md)
- [Lazy 式](lazy.md)
- [IO](../built-in-types-and-modules/io.md)
- [Parallel](../built-in-types-and-modules/parallel.md)
- [コンピュテーション式](../computation-expressions/computation-expressions.md)
- [WebAssembly への出力](../compiler/webassembly.md)
- [ネイティブ連携 (C ABI)](../compiler/native-interop.md)
- [言語仕様（タスク）](../../../docs/language.md#タスク)
- [B08 機能計画チケット](../../../_features/B08-async.md)
- [言語リファレンスの目次](../index.md)
