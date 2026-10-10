# Atomic

`Atomic<T>` は、整数か `bool` を 1 つ入れるセルです。複数のタスクが共有借用 `ref Atomic<T>` を通して、分割できない操作で読み書きします。ロックを使わずに、カウンター、フラグ、通し番号、最大値のような小さな共有状態を持つときに使います。

## この記事のポイント

- `Atomic.create value` で作ります。`T` は `i8`・`i16`・`i32`・`i64`・`i8u`・`i16u`・`i32u`・`i64u`・`bool` のどれかです（組み込みのクラス `AtomicValue`）。ほかの型は `E1005` で、`string` やレコードの共有は [Mutex](./mutex.md) に入れます。
- `load`・`store`・`swap`・`compare_exchange`・`fetch_add`・`fetch_sub`・`fetch_and`・`fetch_or`・`fetch_xor` は、どれも共有借用 `ref Atomic<T>` で呼べます。順序は常に sequentially consistent で、弱い順序は選べません。
- `fetch_*` は更新前の値を返し、オーバーフローは 2 の補数でラップします（通常の整数演算と同じです）。
- `Atomic<T>` は非 Copy の所有値です。複製できず（`E1005`）、関数値に捕捉できません（`E1005`）。複数のタスクで使うときは、[Task.scope](../async-tasks-and-lazy/task.md#共有状態と-taskscope) の共有借用か、`Arc<Atomic<T>>` を使います。
- 更新は 1 つの atomic 命令に下がります。既定の wasm32 は 1 スレッドで動くので通常の命令になり、WASM の import は増えません。

## 基本の書き方

`Atomic.create` で作り、共有借用 `ref cell` で操作します。`compare_exchange cell expected desired` は、いまの値が `expected` のときだけ `desired` を書き込み、成功なら `Result.Ok` で更新前の値を、失敗なら `Result.Error` でいまの値を返します（見かけ上の失敗はありません）。

```tsuzuri run=first%3D0%20second%3D5%20old%3D10%20swapped%3D20%20refused%3D30%20now%3D30
let counter = Atomic.create 0i64
let first = Atomic.fetch_add (ref counter) 5
let second = Atomic.fetch_add (ref counter) 2
Atomic.store (ref counter) 10
let old = Atomic.swap (ref counter) 20
let swapped = match Atomic.compare_exchange (ref counter) 20 30 with
    | Result.Ok previous -> previous
    | Result.Error current -> current
let refused = match Atomic.compare_exchange (ref counter) 20 40 with
    | Result.Ok previous -> previous
    | Result.Error current -> current
$"first={first} second={second} old={old} swapped={swapped} refused={refused} now={Atomic.load (ref counter)}"
```

実行結果:

```text
first=0 second=5 old=10 swapped=20 refused=30 now=30
```

整数リテラルは注釈がなければ `i32` です。`Atomic<i64>` に使う値は、`0i64` のような接尾辞か、型が決まる文脈で書きます。

## タスクで共有する

`Task.scope` は、外の値を借用したまま、子どもたちに共有させます。各子どもは共有借用と自分の番号を受け取ります。足し算は順序によらないので、スケジュールが違っても合計は同じです。

```tsuzuri run=36
let hits = Atomic.create 0i64
let _seen = Task.scope (ref hits) 8 (shared -> index -> Atomic.fetch_add shared (index + 1))
Atomic.load (ref hits)
```

実行結果:

```text
36
```

`Task.parallel` で作ったタスクのように、借用を持ち込めない場所では、`Arc<Atomic<T>>` を `Arc.share` してタスクごとに渡します。`Arc<Atomic<T>>` は `Send` で、中の `Atomic` に共有借用 `Arc.get` で触れます（[Rc と Arc](./rc.md#送信と捕捉の規則)）。

## compare_exchange で更新を作る

`compare_exchange` を繰り返すと、`fetch_*` にない更新を書けます。次は、複数の子どもが値を持ち寄って最大値を残す例です。失敗は、ほかの子どもが先に書いた印なので、読み直してやり直します。

```tsuzuri run=max%3D70
def rec raise_to :: ref Atomic<i64> -> i64 -> i64
fn rec raise_to cell value =
    let seen = Atomic.load cell
    if seen >= value then seen
    else
        match Atomic.compare_exchange cell seen value with
        | Result.Ok previous -> previous
        | Result.Error _current -> raise_to cell value

let highest = Atomic.create 0i64
let _seen = Task.scope (ref highest) 8 (shared -> index -> raise_to shared (index * 10))
$"max={Atomic.load (ref highest)}"
```

実行結果:

```text
max=70
```

## 型の規則

- `Atomic<T>` は `Send` で、`Sync` です。共有借用から更新できるのは、この型と [Mutex](./mutex.md)、そして [Channel](./channel.md) の端（`Sender`・`Receiver`。ランタイムのロックで守られます）だけです。`Atomic` を持つレコードや共用体も、非 Copy で、関数値に捕捉できません。
- 関数値は複製されることがあり、複製した `Atomic` は別のセルになるので、捕捉は `E1005` です。メッセージは `cannot capture Atomic<i64> in a function value; a function value may be copied, and a copy of an Atomic or Mutex would be a separate cell; capture a borrow of it, share it through an Arc, or pass it as an argument` です。借用 `ref cell` を捕捉するか、`Arc` に入れるか、引数で渡します。
- `task { ... }` は、`Atomic` を持ち込めます。所有値のムーブなので、その 1 つのタスクだけが所有します。
- `Atomic<T>` は中身が不透明です。構築、フィールド参照、パターン分解、更新構文は `E1022`、`export def` と `extern def` の境界は `E1008`、`const` の初期化は `E1026` です。`Eq`・`Ord`・`Hash`・`Display` の instance はなく、比較や表示は `Atomic.load` で取り出した値に対して行います。
- `Atomic<T>` を作る関数をジェネリックにするときは、`AtomicValue<'a>` の制約を書きます。`fetch_*` には、さらに `Integer<'a>` が要ります（`bool` の `fetch_add` は `E1005`）。

## 表現と性能

- セルは、値 1 つ分のバッファを持つレコードです。レコードをムーブしてもセルは動かないので、借用はムーブの後も有効です。ヒープの確保は `Atomic.create` で 1 回です。
- ネイティブでは `atomicrmw`・`cmpxchg`・`load atomic`・`store atomic`（すべて `seq_cst`）に下がり、ランタイムの関数は呼びません。たとえば `fetch_add` は、AArch64 では LSE の `ldaddal` 1 命令、x86-64 では `lock` 付きの 1 命令（更新前の値を使うと `lock xadd`、使わないと `lock add`）です。命令の選択は LLVM のターゲット設定に従います。
- `--wasm-feature threads` では WASM の atomic 命令（`i64.atomic.rmw.add` など）です。既定の wasm32 は 1 スレッドなので、LLVM が通常の命令へ下げます。どちらも import は増えません。
- `bool` は 1 バイトです。セルの確保は 16 バイト境界で、8 バイトの atomic 命令に必要な整列を満たします（`--allocator host` と `counting` も同じ保証です）。
- 同じセルを多数のスレッドが更新すると、キャッシュラインの奪い合いで遅くなります。更新の回数を減らせるなら、タスクごとに集計してから 1 回足す方が速いのが一般的です。性能の優位は主張しません。

## API リファレンス

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `create` | `AtomicValue<'a> => 'a -> Atomic<'a>` | 初期値を持つセルを作ります |
| `load` | `AtomicValue<'a> => ref Atomic<'a> -> 'a` | 値を読みます |
| `store` | `AtomicValue<'a> => ref Atomic<'a> -> 'a -> unit` | 値を書きます |
| `swap` | `AtomicValue<'a> => ref Atomic<'a> -> 'a -> 'a` | 値を書き、更新前の値を返します |
| `compare_exchange` | `AtomicValue<'a> => ref Atomic<'a> -> 'a -> 'a -> Result<'a, 'a>` | いまの値が第 2 引数なら第 3 引数を書き、`Ok` で更新前の値を返します。違えば何も書かず、`Error` でいまの値を返します |
| `fetch_add` | `(AtomicValue<'a>, Integer<'a>) => ref Atomic<'a> -> 'a -> 'a` | 足して、更新前の値を返します |
| `fetch_sub` | `(AtomicValue<'a>, Integer<'a>) => ref Atomic<'a> -> 'a -> 'a` | 引いて、更新前の値を返します |
| `fetch_and` | `(AtomicValue<'a>, Integer<'a>) => ref Atomic<'a> -> 'a -> 'a` | ビット積を書き、更新前の値を返します |
| `fetch_or` | `(AtomicValue<'a>, Integer<'a>) => ref Atomic<'a> -> 'a -> 'a` | ビット和を書き、更新前の値を返します |
| `fetch_xor` | `(AtomicValue<'a>, Integer<'a>) => ref Atomic<'a> -> 'a -> 'a` | 排他的論理和を書き、更新前の値を返します |
| `into_inner` | `AtomicValue<'a> => Atomic<'a> -> 'a` | セルを消費して値を返します |

- 型名は `Atomic<T>` で、`std::Atomic<T>` とも書けます。型引数は 1 つです。モジュール名 `Atomic` は予約されていて、同じ名前のモジュールは `E1011` です。同じ名前のレコードも宣言できますが、std の `Atomic<T>` とは別の型です。
- `new` は予約語なので、作る関数の名前は `create` です。`Atomic.new` は `E1002` です。
- `Atomic` は、ソースに名前が現れたプログラムだけに読み込まれます。`Atomic` を書かないプログラムの出力は変わりません。

## まとめ

- 整数と `bool` の共有状態は `Atomic` に入れ、共有借用から `fetch_*`・`compare_exchange` などで更新します。順序は常に sequentially consistent です。
- 複数のタスクへは、`Task.scope` の共有借用か `Arc<Atomic<T>>` で渡します。関数値には捕捉できません。
- 整数と `bool` 以外の値や、複数の値を 1 つの不変条件で守りたいときは [Mutex](./mutex.md) を使います。

## 関連項目

- [Mutex](./mutex.md) — ロックで守る共有状態
- [Task 式](../async-tasks-and-lazy/task.md#共有状態と-taskscope) — `Task.scope`
- [Rc と Arc](./rc.md) — `Arc` による共有
- [Parallel](./parallel.md) — 配列の並列処理
- [型クラス](../types-and-type-inference/type-classes.md) — `Sync` と `AtomicValue`
- [言語リファレンスの目次](../index.md)
