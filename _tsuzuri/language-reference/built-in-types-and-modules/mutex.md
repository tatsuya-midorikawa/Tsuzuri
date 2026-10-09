# Mutex

`Mutex<T>` は、値 `T` を 1 つ入れ、一度に 1 つのタスクだけが `Mutex.with_lock` の中で読み書きできるようにするロックです。複数の値を 1 つの不変条件で守るときや、`string`・配列・レコードのように [Atomic](./atomic.md) に入らない値を共有するときに使います。

## この記事のポイント

- `Mutex.create value` で作り、共有借用 `ref Mutex<T>` に対する `Mutex.with_lock mutex callback` の中でだけ値に触れます。`callback` は排他借用 `ref mut T` を受け取り、その結果を返します。ロックは `callback` が終わると解放されます。
- 結果は所有値で、`Send` でなければなりません。ロック中の値への参照は外へ持ち出せません（`E1013`）。`Mutex.create` も、タスクへ渡せる `T`（`Send`）だけを受け取ります。
- ロックを持ったまま別の `Mutex.with_lock` を呼ぶこと（入れ子）と、ロック中に並列処理（`Task.parallel`・`Task.scope`・`Parallel.*`）を始めることは、トラップです。デッドロックはしません。
- `Mutex<T>` は非 Copy の所有値で、`Send` かつ `Sync` です。関数値には捕捉できません（`E1005`）。複数のタスクへは、[Task.scope](../async-tasks-and-lazy/task.md#共有状態と-taskscope) の共有借用か `Arc<Mutex<T>>` で渡します。
- `Mutex` のポイズンはありません。トラップが起きたときの扱いは[トラップとロック](#トラップとロック)に書きます。
- `Arc` と `Mutex` を組み合わせると循環を作れます。サイクルコレクターはないので、循環の一方は `Arc.Weak` で持ちます（[Rc と Arc](./rc.md#循環と解放)）。

## 基本の書き方

`Mutex.with_lock` の `callback` は `ref mut T` を受け取るので、`deref value = ...` で書き換えます。取り出すときは `Mutex.into_inner` で、ロックなしに所有値を返します（所有者が 1 つなので、待つ相手はいません）。

```tsuzuri run=total%3D36%20indices%3D28
let total = Mutex.create 0i64
let seen = Task.scope (ref total) 8 (shared -> index -> Mutex.with_lock shared (value -> {
    deref value = deref value + index + 1;
    index
}))
$"total={Mutex.into_inner total} indices={Array.sum (ref seen)}"
```

実行結果:

```text
total=36 indices=28
```

`Task.scope` の 8 つの子どもが、順序を決めずに 1 から 8 までを足します。合計は常に 36 で、各子どもが返す `index` は、結果の配列に並びます（[Task 式](../async-tasks-and-lazy/task.md#共有状態と-taskscope)）。

### 複数の値を 1 つのロックで守る

`count` と `sum` を同時に更新するように、2 つの値が 1 つの不変条件を持つときは、レコードにまとめて 1 つの `Mutex` に入れます。別々の `Atomic` にすると、途中の状態が見えてしまいます。

```tsuzuri run=count%3D100%20sum%3D4950
record Stats { count: i64, sum: i64 }

let stats = Mutex.create (Stats { count: 0, sum: 0 })
let _seen = Task.scope (ref stats) 100 (shared -> index -> Mutex.with_lock shared (value -> {
    let current = deref value;
    deref value = Stats { count: current.count + 1, sum: current.sum + index };
    0
}))
let final = Mutex.into_inner stats
$"count={final.count} sum={final.sum}"
```

実行結果:

```text
count=100 sum=4950
```

配列を入れて、子どもごとに別の要素を書くこともできます。`Mutex<[i64]>` の中の配列は `Array.write values index x` で書き換えます。`Vec` や `string` のように、書き換えが所有権の移動になる型は、新しい値を `deref value = ...` で書き戻します。

## タスクへ持ち込む

`Task.parallel` で作ったタスクのように、借用を持ち込めない場所では、`Arc<Mutex<T>>` を `Arc.share` してタスクごとに渡します。`T` が `Send` なら、`Mutex<T>` は `Sync` なので、`Arc<Mutex<T>>` は `Send` です（`Arc<T>` は `T` が `Send` かつ `Sync` のとき `Send` です）。

```tsuzuri run=total%3D36%20owners%3D1
def bump :: Arc<Mutex<i64>> -> i64 -> Task<i64>
fn bump shared amount = task {
    return Mutex.with_lock (Arc.get (ref shared)) (value -> { deref value = deref value + amount; amount })
}

def bumps :: ref Arc<Mutex<i64>> -> i64 -> [Task<i64>]
fn bumps shared n = new [Task<i64>](n, \index -> bump (Arc.share shared) (index + 1))

let shared = Arc.new (Mutex.create 0i64)
let _seen = Task.run (Task.parallel (bumps (ref shared) 8))
let total = Mutex.with_lock (Arc.get (ref shared)) (value -> deref value)
$"total={total} owners={Arc.strong_count (ref shared)}"
```

実行結果:

```text
total=36 owners=1
```

各タスクの `Arc` はタスクの終わりに解放されるので、`Task.parallel` が返った後の所有者は `shared` だけです。

## ロックの規則

| 操作 | 結果 |
| --- | --- |
| ロックを持ったまま、同じスレッドで別の（または同じ）`Mutex` に `with_lock` する | トラップ（assert） |
| `with_lock` の中で `Task.parallel`・`Task.parallel_results`・`Task.scope`・`Parallel.*` を始める | トラップ（assert） |
| `with_lock` の中で `Task.run` する | できる。そのタスクはこのスレッドで逐次に実行します |
| 別のスレッドがロックを持っている間に `with_lock` する | 待つ。持ち主が終えたら取れる |

入れ子と並列開始を禁じるのは、ロックを持ったまま待つ場所をなくすためです。持ち主は、ロックの中で別のロックも子どもの完了も待たないので、待つ側は必ずロックを取れます。デッドロックのかわりにトラップになり、native ではトラップの前に標準エラーへ `Mutex.with_lock cannot be nested; release the outer mutex first` または `parallel work cannot start inside Mutex.with_lock; move it outside the critical section` と出します。WASM にはコンソールがないので、トラップだけです。

`with_lock` の `callback` はロックを取った後で走るので、長い処理を入れると、待つスレッドの時間も延びます。`Mutex.with_lock` の中では、値の読み書きだけをして、重い計算はロックの外で行います。ロックを待つスレッドは眠り、順序（公平性）は保証しません。

## トラップとロック

`Mutex.with_lock` の中のトラップは、ポイズン（以後の `with_lock` が失敗する状態）を残しません。

- 既定（`--trap-mode abort`）では、トラップはプロセスを終わらせるので、ロックの状態は残りません。
- `--trap-mode return` では、トラップは、そのスレッドの最も外側の `tsuzuri_try_*` の境界で止まります。境界は、トラップしたスレッドが持っているロックを解放し、スレッドの状態を消してから、ステータスを返します。同じ呼び出しの別の子どもがそのロックを待っていても、待ちは終わり、グループは完了してトラップが境界へ伝わります。次の呼び出しは、通常どおりロックを取れます。
- ロックの中の値は、トラップした呼び出しが確保したほかのものと同じく、境界が解放します。`Mutex` は export の境界を越えられず（`E1008`）、1 回の呼び出しの外へは残らないので、壊れかけの値を後から読む経路がありません。ポイズンを持たないのは、そのためです。

## 型の規則

- `Mutex.create` は `Send<'a>` を要求します。`Mutex<Rc<T>>` は作れません（`E1013`、`tasks require Send values`）。
- `Mutex<T>` は、`T` の中を見ずに `Sync` です。ロックの外から `T` に触れる道がなく、`T` はタスクの間を移れるものに限られるからです。
- `Mutex<T>` は中身が不透明です。構築、フィールド参照、パターン分解、更新構文は `E1022`、`export def` と `extern def` の境界は `E1008`、`const` の初期化は `E1026` です。`Eq`・`Ord`・`Hash`・`Display` の instance はありません。
- 関数値は複製されることがあり、複製した `Mutex` は別のロックになるので、捕捉は `E1005` です。借用 `ref mutex` を捕捉するか、`Arc` に入れるか、引数で渡します。
- `callback` の結果は所有値です。`value` を返す `Mutex.with_lock m (v -> v)` は `E1013`（`tasks require owned values; ref mut i64 contains a reference`）です。

## 表現と性能

- セルは、ロックの語（32 ビット）と値を持つ 1 要素の配列です。レコードをムーブしてもセルは動きません。確保は `Mutex.create` で 1 回です。
- ネイティブでは、ロックの語を compare-exchange（acquire）で取り、解放は exchange（release）です。競合がなければ、ほかの同期は使いません。待つスレッドは、全 `Mutex` で共有する 1 組の pthread の mutex と条件変数で眠ります。
- 既定の wasm32 は 1 スレッドなので、ロックは `with_lock` が開いているかを示す 1 つのフラグです。入れ子の検査は同じです。
- `--wasm-feature threads` と `Mutex` の組み合わせは、まだ使えません（`E2000`）。`Atomic` と `Task.scope` は使えます（[WebAssembly への出力](../compiler/webassembly.md)）。
- `Mutex` は、ソースに名前が現れたプログラムだけに読み込まれます。`Mutex` を書かないプログラムの出力は変わりません。

## API リファレンス

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `create` | `Send<'a> => 'a -> Mutex<'a>` | 値を入れたロックを作ります |
| `with_lock` | `Send<'b> => ref Mutex<'a> -> (ref mut 'a -> 'b) -> 'b` | ロックを取り、`callback` に排他借用を渡して、その結果を返します。`callback` の後にロックを解放します |
| `into_inner` | `Mutex<'a> -> 'a` | ロックを消費して値を返します |

- 型名は `Mutex<T>` で、`std::Mutex<T>` とも書けます。型引数は 1 つです。モジュール名 `Mutex` は予約されていて、同じ名前のモジュールは `E1011` です。同じ名前のレコードも宣言できますが、std の `Mutex<T>` とは別の型です。
- `with` は予約語で、ドットの後ろにも書けないので、関数名は `with_lock` です（`Bench.with_input` と同じ規則です）。作る関数の名前も、`new` が予約語なので `create` です。

## まとめ

- 整数や `bool` 以外の共有状態、または 2 つ以上の値を 1 つの不変条件で守る共有状態は `Mutex` に入れ、`Mutex.with_lock` の中だけで触れます。
- ロックの入れ子とロック中の並列開始はトラップです。結果は所有値で、ロックの外へ参照を出せません。
- 複数のタスクへは、`Task.scope` の共有借用か `Arc<Mutex<T>>` で渡します。`Arc` と `Mutex` で循環を作ると解放されないので、`Arc.Weak` を使います。

## 関連項目

- [Atomic](./atomic.md) — ロックなしの整数と `bool`
- [Task 式](../async-tasks-and-lazy/task.md#共有状態と-taskscope) — `Task.scope`
- [Rc と Arc](./rc.md) — `Arc` による共有と循環
- [WebAssembly への出力](../compiler/webassembly.md) — WASM でのロック
- [言語リファレンスの目次](../index.md)
