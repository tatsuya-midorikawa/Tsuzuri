# Rc と Arc

`Rc<T>` と `Arc<T>` は、1 つの値を複数の所有者で共有する参照カウントのポインターです。永続データ構造の末尾、木やグラフで共有する部分木、読み取り専用のキャッシュのように、同じ値を複数の場所から所有したいときに使います。`Arc<T>` は計数に atomic 命令を使い、複数のタスクで共有できます。

Tsuzuri に GC はありません。共有はいつも明示的です。`Rc.share` を呼んだときだけ所有者が増え、最後の所有者がスコープを抜けると値がその場で解放されます。

## この記事のポイント

- `Rc<T>` と `Arc<T>` は非 Copy の所有値です。所有者を増やすのは `Rc.share (ref rc)` だけで、代入や引数渡しはムーブです（ムーブ後の使用は `E1012`）。
- 中の値は共有借用 `Rc.get (ref rc)` で読みます。共有した値を書き換えられるのは、中に [Atomic](./atomic.md) か [Mutex](./mutex.md) を入れたときだけです（`Arc<Atomic<i64>>`、`Arc<Mutex<T>>`）。ほかの値は、共有されている間は変わりません。
- 弱参照 `Rc.Weak<T>`／`Arc.Weak<T>` は値の寿命を延ばしません。`Rc.upgrade` は、値が生きていれば新しい `Rc` を `Some` で返し、解放済みなら `None` を返します。
- `Rc` はタスクへ渡せず（`E1013`）、関数値にも捕捉できません（`E1005`）。`Arc<T>` は、`T` が `Send` かつ `Sync` のとき、両方できます（Rust の `Arc<T>: Send` と同じ規則です）。タスクの間で状態を共有する標準の書き方は、`Arc<Atomic<T>>` と `Arc<Mutex<T>>` です。複数のワーカーが 1 つの受け手から取るときは、`Arc<Channel.Receiver<T>>` です（[Channel](./channel.md)）。
- `Rc`／`Arc` だけでは循環を作れません。ただし `Mutex` の中に `Arc` を入れると循環を作れ、サイクルコレクターはないので、そのブロックは解放されません。循環の一方は `Arc.Weak` で持ちます。循環するグラフは [Arena](./arena.md) でも表せます。
- 長い鎖（連結リストなど）の解放は再帰しません。100 万要素の鎖も native のスタックを溢れさせずに解放します。

## 基本の書き方

`Rc.new value` で値をヒープのブロックへ移し、`Rc.share (ref rc)` で所有者を増やします。`Rc.get (ref rc)` は値の共有借用 `ref T` を返し、その借用が生きている間は元の `Rc` をムーブできません。

```tsuzuri run=length%3D11%20owners%3D2%20same%3Dtrue
let first = Rc.new "shared text"
let second = Rc.share (ref first)
let text = Rc.get (ref second)
let length = text.length
let owners = Rc.strong_count (ref first)
let same = Rc.ptr_eq (ref first) (ref second)
$"length={length} owners={owners} same={same}"
```

実行結果:

```text
length=11 owners=2 same=true
```

`Rc.ptr_eq` は 2 つの `Rc` が同じブロックを指すかを返します。値どうしの比較ではありません。`Rc<T>` には `Eq`・`Ord`・`Hash`・`Display` の instance がないので、比較や表示は `Rc.get` で取り出した値に対して行います。

### 末尾を共有するリスト

所有者を 1 つに限る組み込みのリスト `[|T|]` は末尾を共有しません。末尾を共有したい永続リストは、`Rc` を持つ共用体で書きます。

```tsuzuri run=left%3D15%20right%3D25%20tail_owners%3D3
union List = Nil | Cons of (i64 * Rc<List>)

def rec total :: ref Rc<List> -> i64
fn rec total list =
    match Rc.get list with
    | Cons (value, rest) -> value + total rest
    | Nil -> 0

let tail = Rc.new (Cons (2, Rc.new (Cons (3, Rc.new Nil))))
let left = Rc.new (Cons (10, Rc.share (ref tail)))
let right = Rc.new (Cons (20, Rc.share (ref tail)))
$"left={total (ref left)} right={total (ref right)} tail_owners={Rc.strong_count (ref tail)}"
```

実行結果:

```text
left=15 right=25 tail_owners=3
```

`left` と `right` は同じ `tail` を指します。`tail` の値が解放されるのは、3 つの所有者がすべてスコープを抜けたときです。

### 値を取り出す

`Rc.try_unwrap rc` は、所有者がこの `Rc` だけなら値を `Result.Ok` で取り出し、ほかに所有者がいれば同じ `Rc` を `Result.Error` で返します。

```tsuzuri run=first%3D2%20second%3D10
let shared = Rc.new "only owner"
let other = Rc.share (ref shared)
let first = match Rc.try_unwrap shared with
    | Result.Ok text -> text.length
    | Result.Error kept -> Rc.strong_count (ref kept)
let second = match Rc.try_unwrap other with
    | Result.Ok text -> text.length
    | Result.Error kept -> Rc.strong_count (ref kept)
$"first={first} second={second}"
```

実行結果:

```text
first=2 second=10
```

1 回目は `other` が残っているので `Error` です。`kept` が腕の終わりで解放されると所有者は `other` だけになり、2 回目は値を取り出せます。

## 弱参照

`Rc.downgrade (ref rc)` は弱参照 `Rc.Weak<T>` を作ります。弱参照は値の寿命を延ばしません。最後の強い所有者が抜けると値は解放され、その後の `Rc.upgrade` は `None` です。

```tsuzuri run=before%3D6%20after%3D-1
let cache = Rc.new [1, 2, 3]
let observer = Rc.downgrade (ref cache)
let before = match Rc.upgrade (ref observer) with
    | Maybe.Some alive -> Array.sum (Rc.get (ref alive))
    | Maybe.None -> -1
Owned.drop cache
let after = match Rc.upgrade (ref observer) with
    | Maybe.Some alive -> Array.sum (Rc.get (ref alive))
    | Maybe.None -> -1
$"before={before} after={after}"
```

実行結果:

```text
before=6 after=-1
```

値を解放した後もブロック（2 つの計数）は弱参照が残る間だけ残り、最後の弱参照とともに解放されます。

## Arc とタスク

`Arc<T>` の API は `Rc<T>` と同じ名前・同じ型で、モジュール名が `Arc` に変わるだけです。計数を atomic 命令で更新するので、`T` が `Send` で、後述のとおりタスク間で共有できる値なら `Arc<T>` も `Send` です。タスクごとに `Arc.share` で所有者を作って渡すと、配列を複製せずに複数のタスクで読めます。

```tsuzuri run=even%3D2450%20odd%3D2500%20owners%3D1
def part :: Arc<[i64]> -> i64 -> Task<i64>
fn part values offset = task {
    let numbers = Arc.get (ref values);
    let mut total = 0;
    let mut index = offset;
    while index < numbers.length do
        total = total + numbers[index];
        index = index + 2
    return total
}

def parts :: ref Arc<[i64]> -> [Task<i64>]
fn parts shared = new [Task<i64>](2, \offset -> part (Arc.share shared) offset)

let shared = Arc.new (new [i64](100, \index -> index))
let sums = Task.run (Task.parallel (parts (ref shared)))
$"even={sums[0]} odd={sums[1]} owners={Arc.strong_count (ref shared)}"
```

実行結果:

```text
even=2450 odd=2500 owners=1
```

各タスクの `Arc` はタスクの終わりに解放され、`Task.run` が返った時点で所有者は `shared` だけです。最後の所有者がどのタスクでも、値の解放は他のタスクのすべての読み取りの後に起きます。

## 送信と捕捉の規則

| 型 | 所有者を増やす | タスクへ移す（`Send`） | 関数値に捕捉する |
| --- | --- | --- | --- |
| `Rc<T>`・`Rc.Weak<T>` | `Rc.share`・`Rc.downgrade` | できない（`E1013`） | できない（`E1005`） |
| `Arc<T>`・`Arc.Weak<T>` | `Arc.share`・`Arc.downgrade` | `T` が `Send` かつ `Sync` のとき | `T` が `Sync` のとき |

- `Rc` の計数は atomic ではないので、`Rc` はそれを作ったタスクから出ません。`Rc` を持つレコード・共用体・配列、`Arc<Rc<T>>` も同じです。
- 関数値の型は捕捉した値を表さず、どの関数値もタスクへ渡せます。そのため `Rc` は関数値（ラムダ、部分適用、`Owned.function`）に捕捉できません。`Rc` は引数として渡します。
- `Arc` を捕捉した関数値を複製すると、`Arc.share` と同じく所有者が 1 増えます。値は複製しません。
- `Arc` を持つタスクはどれも、`Arc.get` で同時に値を借用できます。そのため `Arc<T>` をタスクへ渡せるのは、`T` が `Send` であり、かつ `Sync`（複数のタスクが共有借用で同時に使ってよい値）のときだけです。`Sync` でないのは、`Rc`、extern ハンドル（`extern type`）、Copy でない `dyn` 値、`Owned.Function`、`Task`、排他参照、`Seq`、`Async`、GPU のハンドルを、入れ子の中にも含む値です。ホストのライブラリーのハンドルは複数のスレッドから同時に使えるとは限らないためです。`Sync` でない値の `Arc` は、それを持つレコードや共用体も含めて、タスクへ渡せず（`E1013`）、関数値にも捕捉できません（`E1005`）。同じタスクの中で `Arc.share` するのは自由です。ハンドルを別のタスクで使うときは、`Arc` に入れずに値そのものを 1 つのタスクへ移します。`Atomic<T>` と `Mutex<T>` は `Sync` で、`Mutex<T>` は `T` が `Send` なら作れるので、`Arc<Atomic<T>>` と `Arc<Mutex<T>>` はタスクへ渡せます。`Sync` は組み込みの型クラスです（[組み込みの印](../types-and-type-inference/constraints.md#組み込みの印)）。
- `Rc` と `Arc` には排他参照 `ref mut` を入れられません（`E1005`）。`Rc<ref string>` のように共有参照を入れた値は、参照先より長く生きられません（`E1013`）。
- `Rc` と `Arc` は公開 ABI（`export def`・`extern def`）に使えず（`E1008`）、const にもできません（`E1026`）。

## 循環と解放

`Rc` と `Arc` の値は、共有されている間は変更できないので、値ができる前にその値を指す `Rc`・`Arc` は作れません。`Rc` と `Arc` だけでは循環は作れず、参照カウントの循環による解放漏れも起きません。循環するグラフは [Arena](./arena.md) とハンドルでも表せます。

ただし、[Mutex](./mutex.md) は共有された値を書き換えられます。`Arc` は `Mutex` の中に入れられる（`Rc` は `Send` ではないので入れられません）ので、`Arc<Node>` の中の `Mutex` が別の `Arc<Node>` を指すと、循環を作れます。Tsuzuri にサイクルコレクターはなく、循環の中のブロックは、所有者がすべていなくなっても解放されません。次の例は、2 つのノードが互いを `Arc` で指して、`Owned.drop` の後も `Arc.upgrade` が成功する（ブロックが残っている）ようすです。

```tsuzuri run=alive%3Dtrue
record Node { id: i64, peer: Mutex<Maybe<Arc<Node>>> }

def link :: ref Arc<Node> -> Arc<Node> -> i64
fn link node target =
    Mutex.with_lock (ref (Arc.get node).peer) (peer -> {
        deref peer = Maybe.Some target;
        0
    })

let first = Arc.new (Node { id: 1, peer: Mutex.create Maybe.None })
let second = Arc.new (Node { id: 2, peer: Mutex.create Maybe.None })
let watcher = Arc.downgrade (ref first)
let _a = link (ref first) (Arc.share (ref second))
let _b = link (ref second) (Arc.share (ref first))
Owned.drop first
Owned.drop second
let alive = match Arc.upgrade (ref watcher) with
    | Maybe.Some _node -> true
    | Maybe.None -> false
$"alive={alive}"
```

実行結果:

```text
alive=true
```

循環の一方を `Arc.Weak` で持つと、強い所有者の循環ができないので、所有者がいなくなれば値は解放されます。親から子へは `Arc`、子から親へは `Arc.Weak` のように、所有の向きを 1 つに決めます。

```tsuzuri run=alive%3Dfalse
record Node { id: i64, peer: Mutex<Maybe<Arc.Weak<Node>>> }

def link :: ref Arc<Node> -> Arc.Weak<Node> -> i64
fn link node target =
    Mutex.with_lock (ref (Arc.get node).peer) (peer -> {
        deref peer = Maybe.Some target;
        0
    })

let first = Arc.new (Node { id: 1, peer: Mutex.create Maybe.None })
let second = Arc.new (Node { id: 2, peer: Mutex.create Maybe.None })
let watcher = Arc.downgrade (ref first)
let _a = link (ref first) (Arc.downgrade (ref second))
let _b = link (ref second) (Arc.downgrade (ref first))
Owned.drop first
Owned.drop second
let alive = match Arc.upgrade (ref watcher) with
    | Maybe.Some _node -> true
    | Maybe.None -> false
$"alive={alive}"
```

実行結果:

```text
alive=false
```

型の定義は `Rc` を通って自分自身を含められます。`record Node { value: i64, children: Vec<Rc<Node>> }` のように、空のコレクションや共用体の case で有限の値を作れれば受理されます。`record Loop { next: Rc<Loop> }` は有限の値がないので `E1010` です。

親を弱参照で指す木も書けます。`Mutex` を使わない木では値は共有の後に変わらないので、親を先に作り、子が `Rc.downgrade` で親を指します。親を解放した後の `Rc.upgrade` は `None` です。

```tsuzuri run=parent%3D7%20after%3D-1
record TreeNode { value: i64, parent: Maybe<Rc.Weak<TreeNode>>, children: Vec<Rc<TreeNode>> }

def parent_value :: ref Rc<TreeNode> -> i64
fn parent_value node =
    match (Rc.get node).parent with
    | Maybe.Some weak ->
        match Rc.upgrade weak with
        | Maybe.Some parent -> (Rc.get (ref parent)).value
        | Maybe.None -> -1
    | Maybe.None -> 0

let root = Rc.new (TreeNode { value: 7, parent: Maybe.None, children: Vec.empty() })
let child = Rc.new (TreeNode { value: 2, parent: Maybe.Some (Rc.downgrade (ref root)), children: Vec.empty() })
let before = parent_value (ref child)
Owned.drop root
$"parent={before} after={parent_value (ref child)}"
```

実行結果:

```text
parent=7 after=-1
```

自分自身を含む型の値を持つブロックの解放は、[Union](./union.md) の再帰型と同じ待ちリストで反復的に行います。100 万要素の連結リストも native のスタックを使い尽くさずに解放でき、`Arc` の鎖も同じです。

## 表現と性能

- `Rc<T>` の値は 1 つのポインターです。`Rc.new` はブロック `{ 強い参照の数, 弱い参照の数, T }` を 1 回確保し、強い所有者全体で弱い参照を 1 つ持ちます（Rust と同じ）。`Rc.weak_count` が返すのは `Rc.Weak` の数で、この 1 つを含みません。
- 自分自身を含みうる型（再帰型を含む `T`）のブロックは、解放の待ちリストのために先頭に 16 バイトの見出しを持ちます。共用体が `Rc` を通って自分自身を含むとき、その共用体の値も再帰型のヒープノードになるので、`Cons` 1 つにつき確保は 2 回です。
- `Arc` は、所有者の追加に `monotonic`、解放に `release` の atomic 加減算を使い、計数が 0 になったタスクは `fence acquire` の後で値を解放します。`Arc.upgrade` は計数が 0 でないときだけ増やす比較交換（`cmpxchg`）を繰り返します。`Rc` は通常のロード・ストアだけです。
- 既定の wasm32 では atomic 命令を LLVM が通常の命令へ下げ、`--wasm-feature threads` では WASM の atomic 命令になります。どちらも WASM の import は増えません。
- 計数が `i64` の最大値を超えるとトラップします。
- WASM では既定の 16 MiB のヒープに収まる数だけ確保できます。`union List = Nil | Cons of (i64 * Rc<List>)` の鎖はおよそ 16 万要素までで、それを超えると確保の失敗でトラップします。
- 性能の優位は主張しません。

## API リファレンス

`Rc` モジュールの関数です。`Arc` モジュールには、`Rc` を `Arc` に置き換えた同じ関数があります。

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `new` | `'a -> Rc<'a>` | 値を新しいブロックへ移します |
| `share` | `ref Rc<'a> -> Rc<'a>` | 同じ値の所有者を 1 つ増やします |
| `get` | `ref Rc<'a> -> ref 'a` | 値の共有借用を返します |
| `strong_count` | `ref Rc<'a> -> i64` | 強い所有者の数 |
| `weak_count` | `ref Rc<'a> -> i64` | 弱参照の数 |
| `ptr_eq` | `ref Rc<'a> -> ref Rc<'a> -> bool` | 同じブロックを指すか |
| `try_unwrap` | `Rc<'a> -> Result<'a, Rc<'a>>` | 唯一の所有者なら値を `Ok` で取り出し、そうでなければ `Error` で返します |
| `downgrade` | `ref Rc<'a> -> Rc.Weak<'a>` | 弱参照を作ります |
| `upgrade` | `ref Rc.Weak<'a> -> Maybe<Rc<'a>>` | 値が生きていれば新しい所有者を `Some` で返します |

- 型名は `Rc<T>`・`Rc.Weak<T>`・`Arc<T>`・`Arc.Weak<T>` で、`std::Rc<T>` とも書けます。型引数は 1 つです（`Rc` だけを書くと `E1004`）。型名 `Rc`・`Arc` は予約されていて、同じ名前のレコード・共用体・型エイリアスは `E1001`、モジュール名は `E1011` です。
- `new` は予約語ですが、ドットの後ろではメンバー名として `Rc.new` と書けます。
- `'a` に制約はありません。引数は通常の関数呼び出しと同じく左から一度ずつ評価します。

## まとめ

- 同じ値を複数の所有者で読むときは `Rc`、複数のタスクで読むときは `Arc` を使います。共有は `share` で明示します。
- 中の値は `get` の共有借用で読みます。共有した値を書き換えるには `Atomic` か `Mutex` を入れ、`Arc<Atomic<T>>`・`Arc<Mutex<T>>` をタスクへ渡します。`Rc`／`Arc` だけでは循環できませんが、`Arc` と `Mutex` では循環でき、解放されません。循環の一方は `Arc.Weak` で持ちます。
- 弱参照は値の寿命を延ばさず、`upgrade` で生きているかを確かめます。

## 関連項目

- [Atomic](./atomic.md) — 共有できる整数と `bool`
- [Mutex](./mutex.md) — ロックで守る共有状態
- [Arena](./arena.md) — 循環するグラフ
- [Drop とリソースの解放](../ownership-and-memory/drop.md) — 共有所有の選び方
- [Task 式](../async-tasks-and-lazy/task.md) — タスクへ渡せる値
- [Union](./union.md) — 再帰型の表現
- [言語リファレンスの目次](../index.md)
