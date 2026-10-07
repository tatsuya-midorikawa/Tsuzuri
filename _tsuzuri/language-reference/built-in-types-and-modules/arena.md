# Arena

`Arena<T>` は、値を世代付きのハンドル `Arena.Handle<T>` で指す所有コンテナです。グラフ・DAG・キャッシュのように同じ値を複数の場所から指す構造や、循環する構造を、GC や参照カウントを使わずに所有権モデルのまま表せます。Rust の `slotmap::DenseSlotMap` に相当します。

値は arena がまとめて所有し、ほかの値はハンドル（整数 3 つの Copy 値）で値を指します。ハンドルは値の寿命を延ばさないので、循環しても解放漏れは起きません。arena を drop すると、循環を含むすべての値が再帰なしで一度に解放されます。

## この記事のポイント

- `Arena<T>` は非 Copy の所有値です。内部は不透明で、フィールド参照・構築・パターン分解・更新構文は `E1022`、公開 C ABI への export は `E1008` です。ファイル名 `Arena.tz` は予約名で `E1011` です。
- `Arena.Handle<T>` は Copy の小さな値です。要素型は型で照合し（別の要素型の arena に渡すと `E1003`）、どの arena のハンドルかは実行時の arena ID で照合します。
- ハンドルが有効なのは、それを作った arena の中で、`insert` が返してから `remove` されるまでの間だけです。`update` は同じハンドルを有効なまま保ちます。
- 無効なハンドル（削除済み・別の arena）を渡すと、`get` は `None`、`contains` は `false`、`remove` は arena を変えずに `None` を返し、`at` と `update` はトラップします。
- 循環は `update` で後から辺（ハンドル）を足して作ります。
- 検証と読み取りは $O(1)$、`insert` は償却 $O(1)$、`remove` は $O(1)$ です。`iter` は値を詰めて並べた領域を順に走査します。

## 基本の書き方

`Arena.empty()` か `Arena.with_capacity count` で空の arena を作り、`Arena.insert arena value` で値を移します。`insert` は arena を消費し、新しい arena とハンドルの組を返します。ほかの値を指すには、そのハンドルをフィールドに入れます。

次の例は、2 つのノードが互いを指す循環グラフを作り、ハンドルでたどります。`a` を挿入した時点では `b` のハンドルがまだないので、`b` を挿入した後で `Arena.update` を使い `a` に辺を足します。

```tsuzuri run=143
record Node { value: i64, edges: Vec<Arena.Handle<Node>> }

def link :: Arena.Handle<Node> -> Node -> Node
fn link target node =
    match node with
    | Node { value = value, edges = edges } -> Node { value: value, edges: Vec.push edges target }

def cycle :: i64
fn cycle =
    let empty: Arena<Node> = Arena.empty()
    match Arena.insert empty (Node { value: 1, edges: Vec.empty() }) with
    | (with_a, a) ->
        match Arena.insert with_a (Node { value: 20, edges: Vec.push (Vec.empty()) a }) with
        | (with_b, b) ->
            let graph = Arena.update with_b a (link b)
            let first = Arena.at (ref graph) a
            let second = Arena.at (ref graph) first.edges[0]
            let back = Arena.at (ref graph) second.edges[0]
            let mut total = 0
            for (handle, node) in Arena.iter (ref graph) do
                total = total + node.value
                if handle == a then total = total + 100
            first.value + second.value + back.value + total

cycle()
```

実行結果:

```text
143
```

`first`・`second`・`back` は 1・20・1 で、`iter` の合計は 1 + 100 + 20 です。`cycle` の終わりで `graph` が drop され、循環したノードもまとめて解放されます。`record Node { value: i64, edges: Vec<Node> }` のように値を直接入れ子にした record は `E1010` ですが、ハンドルは整数だけの値なので、要素型が `Node` 自身でも受理されます。

引数なしの関数は `Arena.empty()` と書きます。`Arena.empty ()` は unit を渡すので `E1006` です。

## ハンドルの有効性

ハンドルは、arena ID・slot の添字・世代の 3 つの `i64` です。`remove` は slot の世代を 1 増やすので、古いハンドルは同じ slot が再利用された後も無効のままです。

```tsuzuri run=stale%3Dfalse%20fresh%3Dtrue%20missing%3Dnone
let empty: Arena<string> = Arena.empty()
match Arena.insert empty "first" with
| (arena, first) ->
    match Arena.remove arena first with
    | (removed, _value) ->
        match Arena.insert removed "second" with
        | (reused, second) ->
            let stale = Arena.contains (ref reused) first
            let fresh = Arena.contains (ref reused) second
            let missing = match Arena.get (ref reused) first with
            | Maybe.Some _text -> "some"
            | Maybe.None -> "none"
            $"stale={stale} fresh={fresh} missing={missing}"
```

実行結果:

```text
stale=false fresh=true missing=none
```

- 無効なハンドルの定義: ハンドルの arena ID が arena と違う、添字が slot の範囲外、または slot の世代がハンドルの世代と違う（削除済み・別の値に再利用済み）。
- 世代は slot ごとの `i64` で、0 から始まり `remove` のたびに 1 増えます。世代が `i64` の最大値の slot を削除すると、その slot は退役して二度と再利用されません（トラップにはしません）。古いハンドルが有効に戻ることはありません。
- 別の arena の判定は「同じ arena から作られたか」です。arena を捕捉した関数値を複製すると、独立した複製ができますが arena ID も複製されるので、同じハンドルが両方で有効です。
- ハンドルは値の寿命を延ばしません。arena を drop した後のハンドルは、どの arena でも無効です。

## 反復と順序

`Arena.iter (ref arena)` は、ハンドルと値の参照の組 `(Arena.Handle<T> * ref T)` を列挙する [Seq](./seq.md) を返します。順序は値を詰めて並べた領域の順で、削除がなければ挿入順です。`remove` は末尾の値を削除位置へ移し（swap-remove）、空いた slot は最後に削除したものから再利用します。

```tsuzuri run=order%3D04235%20reused%3Dtrue
let mut arena: Arena<i64> = Arena.empty()
let mut handles: Vec<Arena.Handle<i64>> = Vec.empty()
for value in 0i64 .. 4 do
    match Arena.insert arena value with
    | (next, handle) ->
        arena = next
        handles = Vec.push handles handle
match Arena.remove arena handles[1] with
| (removed, _one) ->
    match Arena.insert removed 5 with
    | (filled, fifth) ->
        let mut text = ""
        for (_handle, value) in Arena.iter (ref filled) do
            text = text + to_string (deref value)
        let reused = handles[1] < fifth && fifth < handles[2]
        $"order={text} reused={reused}"
```

実行結果:

```text
order=04235 reused=true
```

値 1 を削除すると末尾の 4 が位置 1 へ移り、5 は末尾に入ります。5 は空いた slot 1 を世代 1 で再利用します。

ハンドルの `Eq`・`Ord`・`Hash` は添字と世代だけを使い、arena ID を含みません（`Ord` は添字、次に世代の辞書順）。arena ID の数値は並列の task では実行ごとに変わり得るためで、比較やハッシュの結果、`Map`・`HashMap` のキー順は API の呼び出し列だけで決まります。ハンドルに `Display` と `Debug` の表示はありません。

## 所有権と借用

- `insert`・`remove`・`update` は arena を消費して新しい arena を返します。消費した arena を使うと `E1012` です。
- `get`・`at`・`iter` の結果は arena の共有借用を持ちます。その間に arena を消費すると `E1014` です。必要な整数やハンドル（Copy）を先に取り出し、借用を終えてから更新します。
- `remove` は値の所有権を呼び出し元へ返し、`update` は値を `change` に渡して戻り値を同じ位置へ戻します。`update` に渡す関数は arena を参照できないので、更新中に arena が変わることはありません。
- arena の drop は、値を詰めた領域の先頭から順に各値の drop を呼び、続いて内部のバッファを解放します。循環を含むグラフでも再帰しないので、深さの制限を受けません。
- `Arena<T>` を捕捉した関数値・task の可否は要素型 `T` に従います。`T` が `Send` なら arena を task へ移せます。ハンドルは常に Copy で、task も捕捉できます。`ref Arena<T>` を task が捕捉すると `E1013` です。複数の task で 1 つの arena を共有して読む手段はありません。
- `Arena<ref string>` のように参照を入れた arena は、参照先より長く生きられません（`Map` と同じ規則）。

```tsuzuri run=42
let empty: Arena<i64> = Arena.empty()
match Arena.insert empty 40 with
| (arena, handle) ->
    let updated = Arena.update arena handle (\value -> value + 2)
    Task.run (task { return deref (Arena.at (ref updated) handle) })
```

実行結果:

```text
42
```

## API リファレンス

すべての関数は `std::Arena` モジュールに属しています。`n` は arena の値の数です。

| 関数 | シグネチャ | 計算量 | 無効なハンドル |
| --- | --- | --- | --- |
| `empty` | `fn() -> Arena<'a>` | $O(1)$、確保なし | – |
| `with_capacity` | `i64 -> Arena<'a>` | $O(1)$、3 つのバッファを確保 | –（負の容量はトラップ） |
| `length` | `ref Arena<'a> -> i64` | $O(1)$ | – |
| `contains` | `ref Arena<'a> -> Arena.Handle<'a> -> bool` | $O(1)$ | `false` |
| `get` | `ref Arena<'a> -> Arena.Handle<'a> -> Maybe<ref 'a>` | $O(1)$ | `None` |
| `at` | `ref Arena<'a> -> Arena.Handle<'a> -> ref 'a` | $O(1)$ | トラップ |
| `insert` | `Arena<'a> -> 'a -> (Arena<'a> * Arena.Handle<'a>)` | 償却 $O(1)$ | – |
| `remove` | `Arena<'a> -> Arena.Handle<'a> -> (Arena<'a> * Maybe<'a>)` | $O(1)$ | 変えない arena と `None` |
| `update` | `Arena<'a> -> Arena.Handle<'a> -> ('a -> 'a) -> Arena<'a>` | $O(1)$ ＋ `change` | トラップ（`change` は呼ばない） |
| `iter` | `ref Arena<'a> -> Seq<(Arena.Handle<'a> * ref 'a)>` | 全体で $O(n)$ | – |

`'a` に制約はありません。引数は通常の関数呼び出しと同じく左から一度ずつ評価します。

## 資源と性能

- 内部は、値を詰めて並べた `Vec<'a>`、各位置の slot を記録する `Vec<i64>`、slot の表 `Vec`（位置と世代）、空き slot の列の先頭からなります。値 1 つあたりの追加のメモリは 24 バイトで、ハンドルは 24 バイト、`Arena<T>` の値自体は 88 バイトです。
- 容量と確保は `Vec` と同じ規則です。容量やバイト数の overflow、確保の失敗はトラップします。WASM では 3 つのバッファが既定の 16 MiB のヒープを共有します。
- 検証は比較 2 回と添字の範囲検査だけで、確保しません。
- arena ID は、プロセス全体の 1 つのカウンターから原子的に採番します（native と `--wasm-feature threads` では atomic 命令、既定の wasm32 では通常の加算）。WASM の import は増えません。1 プロセスで作れる arena は $2^{63} - 1$ 個までで、超えるとトラップします。
- 確保方式（`--allocator`）は変えません。Arena は値を通常の `Vec` のバッファに置く型付きのコンテナで、論理的な名前（ハンドル）と世代の検証を加えるものです。
- 性能の優位は主張しません。上の計算量だけを保証します。

## まとめ

- 共有・循環する構造は、値を `Arena<T>` に入れ、互いを `Arena.Handle<T>` で指して表します。
- ハンドルは Copy で、削除済み・別の arena のものは `get`／`contains`／`remove` で検出でき、`at`／`update` ではトラップします。
- arena の drop で、循環を含むすべての値が再帰なしで解放されます。

## 関連項目

- [Vec](./vec.md) — Arena が値を置く伸縮可能な配列
- [Seq](./seq.md) — `iter` が返す遅延シーケンス
- [Union](./union.md) — 所有する木構造（再帰 union）
- [Drop とリソースの解放](../ownership-and-memory/drop.md) — 共有所有の選び方
- [言語リファレンスの目次](../index.md)
