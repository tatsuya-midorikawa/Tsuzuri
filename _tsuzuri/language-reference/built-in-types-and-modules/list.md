# List

`List` は、不変な単方向連結リストです。型は `[|T|]` です。`List<T>` という別名はありません。先頭への追加と先頭の分解は $O(1)$、長さ `.length` もノードに保存されているので $O(1)$ です。添字は先頭からたどるので $O(index + 1)$ です。

再帰やパターンマッチに向きます。末尾のノードは複数のリストで共有できます。F# と記号が逆で、Tsuzuri の `[|1, 2|]` がリスト、`[1, 2]` が配列です。

## この記事のポイント

- 型は `[|T|]`。要素もリンクも生成後は不変です。`List<T>` とは書けません。
- リテラル `[|1, 2, 3|]`、空リスト `[||]`、`new [|T|](n, \i -> ...)`、`List.cons` で作ります。
- `::` はパターン専用です。式の `1 :: xs` は `E0002` なので、先頭へ足すときは `List.cons` を使います。
- 添字は $O(index + 1)$ です。ランダムアクセスが中心なら [Array](./array.md) や [Vec](./vec.md) を使います。長さは $O(1)$ です。
- 走査には直接の `for ... in` ループが利用でき、一時メモリを確保せず $O(n)$ で効率的に走査します。
- `map_ref` や `fold_ref` により、非 Copy な所有要素を持つリストも借用したまま安全に処理できます。

## 基本の書き方と生成

### リテラルと空リスト

リストリテラルはパイプ付きの角括弧 `[|...|]` で囲みます。空リストは `[||]` と記述します。

```tsuzuri run=len%3D3%20first%3D10
let numbers = [|10, 20, 30|]
let len = numbers.length
let first = numbers[0]
$"len={len} first={first}"
```

実行結果:

```text
len=3 first=10
```

空リスト `[||]` は要素型を推論できる文脈で記述します。推論できない場合は `let empty: [|i64|] = [||]` のように型注釈を付与します。

### コンスによる要素の追加

先頭に足すには `List.cons` を使います。元のリストは変わらず、新しい先頭ノードだけを確保して残りを共有します。

```tsuzuri run=new_len%3D4%20head%3D5
let original = [|10, 20, 30|]
let extended = List.cons 5 original
let new_len = extended.length
let head = extended[0]
$"new_len={new_len} head={head}"
```

> [!NOTE]
> `::` はパターン専用です。`5 :: xs` のような式は `E0002` になります。先頭へ足すときは `List.cons` を使います。

実行結果:

```text
new_len=4 head=5
```

### 動的初期化 `new [|T|](count, initializer)`

要素数を指定して関数からリストを生成するには `new [|T|](count, initializer)` を使用します。

```tsuzuri run=generated%3D%5B%7C0%2C%202%2C%204%2C%206%7C%5D
let evens = new [|i64|](4, \i -> i * 2)
$"generated={evens}"
```

実行結果:

```text
generated=[|0, 2, 4, 6|]
```

## パターンマッチと分解

パターンの `::` で、先頭と残りに分けられます。式としてのコンスは書けません。

```tsuzuri run=sum%3D60
def rec sum_list :: [|i64|] -> i64
fn rec sum_list xs =
    match xs with
    | head :: tail -> head + sum_list tail
    | [||] -> 0

let values = [|10, 20, 30|]
let total = sum_list values
$"sum={total}"
```

実行結果:

```text
sum=60
```

リストのパターンには以下が使用できます：
- `[||]`: 空リストにマッチ
- `head :: tail`: 先頭要素 `head` と残りのリスト `tail` に分解
- `[|x|]`: ちょうど 1 要素のリストにマッチ
- `[|x, y|]`: ちょうど 2 要素のリストにマッチ

また、関数的に先頭要素以外を取得する `List.tail xs` も用意されています（空リストに適用すると安全にトラップします）。

## 添字アクセスと計算量上の注意

添字 `xs[i]` で要素を読めます。

```tsuzuri run=third%3D30
let values = [|10, 20, 30, 40|]
let third = values[2]
$"third={third}"
```

実行結果:

```text
third=30
```

> [!WARNING]
> **添字は $O(index + 1)$ です**
> インデックス $i$ は先頭から $i$ 回リンクをたどります。ループの中で `xs[i]` を繰り返すと $O(n^2)$ になります。走査は `for ... in` か `List.fold` / `List.map` を使います。長さ `.length` は $O(1)$ です。

## API リファレンス

すべての関数は `std::List` モジュール（または言語組み込み）に属しています。

### 基本操作・情報取得

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `length` | `ref [\|'a\|] -> i64` | 要素数を返します。保存済みの長さなので $O(1)$ です |
| `is_empty` | `ref [\|'a\|] -> bool` | リストが空かどうかを $O(1)$ で返します |
| `cons` | `'a -> [\|'a\|] -> [\|'a\|]` | 先頭に要素を追加した新しいリストを返します（$O(1)$） |
| `tail` | `[\|'a\|] -> [\|'a\|]` | 先頭要素を除いた残りのリストを返します（空リストはトラップ） |

### 変換・畳み込み

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `map` | `Copy<'a> => ('a -> 'b) -> ref [\|'a\|] -> [\|'b\|]` | 各要素に関数を適用した新しいリストを返します |
| `map_ref` | `(ref 'a -> 'b) -> ref [\|'a\|] -> [\|'b\|]` | 要素の参照を受け取る関数で変換します（非 Copy 要素に対応） |
| `fold` | `Copy<'a> => ('state -> 'a -> 'state) -> 'state -> ref [\|'a\|] -> 'state` | 先頭から左へ畳み込みます |
| `fold_ref` | `('state -> ref 'a -> 'state) -> 'state -> ref [\|'a\|] -> 'state` | 要素の参照で左へ畳み込みます |
| `reverse` | `Copy<'a> => ref [\|'a\|] -> [\|'a\|]` | 要素の並びを反転した新しいリストを返します |
| `copy` | `Copy<'a> => ref [\|'a\|] -> [\|'a\|]` | リストの全ノードを複製した新しい独立リストを返します |
| `to_array` | `Copy<'a> => ref [\|'a\|] -> ['a]` | リストから連続メモリの所有配列 `[T]` を構築します |
| `iter` | `ref [\|'a\|] -> Seq<ref 'a>` | 各要素への共有参照を順次列挙する `Seq` を返します |

## 計算量

| 操作 | 計算量 | 備考 |
| --- | --- | --- |
| 先頭への追加 `List.cons` | $O(1)$ | 先頭ノードを 1 個確保して既存リストへ接続 |
| 先頭の取得 `match xs with head :: _` | $O(1)$ | 先頭ポインタの参照 |
| 先頭の除去 `List.tail` | $O(1)$ | 先頭要素を解放し、残りを返す。空ならトラップ |
| 空判定 `is_empty` | $O(1)$ | 長さが 0 かどうか |
| 添字アクセス `xs[i]` | $O(index + 1)$ | 先頭からリンクをたどる |
| 長さの取得 `length` / `.length` | $O(1)$ | 保存済みの長さを読む |
| 要素変換 `map` / `map_ref` | $O(n)$ | 全要素を走査して新しいリストを構築 |
| 畳み込み `fold` / `fold_ref` | $O(n)$ | 全要素を 1 回走査 |
| 反転 `reverse` | $O(n)$ | 全要素を逆順に再構築 |
| 配列化 `to_array` | $O(n)$ | 全要素を配列メモリへコピー |

## 所有権と借用

### ノードの受け渡しとメモリ解放

`List.cons x xs` は、所有するリスト `xs` を消費し、その先頭に新しいノードを 1 個つないだリストを返します。既存のノードはコピーされず、所有権ごと新しいリストへ移るので、追加は $O(1)$ です。`List.tail xs` も同じく `xs` を消費し、先頭ノードを解放して残りのノード列を返します。

リスト `[|T|]` の所有者は常に 1 つです。そのため、F# や Haskell のリストのように、1 つの末尾を複数のリストで共有することはありません。末尾を共有する永続リストが要るときは、`union List = Nil | Cons of (i64 * Rc<List>)` のように [Rc](rc.md) を持つ共用体を定義します。

- 要素が非 Copy のリストは、`List.cons` に渡したあとの元の束縛を使うと `E1012` です。
- 要素が Copy のリストは、渡したあとも元の束縛を使えますが、そのときはノード列全体が複製されます。`--warn implicit-copy` を付けると `W1006` で知らせてくれます。
- `match` の `head :: tail` で `tail` を所有値として使うには、要素型が Copy である必要があります（非 Copy なら `E1005`）。このときも残りのノード列が複製されます。長いリストを先頭から順に処理するなら、`tail` を取り出して再帰するより `for ... in` のほうが余分な複製を避けられます。

各ノードは、そのノードを所有するリストが不要になった時点で一度だけ解放されます。

### 非 Copy な要素の処理

要素が `string` や、非 `Copy` のフィールドを持つレコードのとき、値として取り出すと所有権が動きます。リストを借用したまま処理するなら `map_ref` や `fold_ref` を使います。全フィールドが `Copy` のレコードは `Copy` です。

```tsuzuri run=lengths%3D%5B%7C5%2C%206%7C%5D%20total%3D11
let fruits = [|"apple", "banana"|]
let lengths = List.map_ref (\s -> s.length) (ref fruits)
let total = List.fold_ref (\acc s -> acc + s.length) 0 (ref fruits)
$"lengths={lengths} total={total}"
```

実行結果:

```text
lengths=[|5, 6|] total=11
```

## 反復の仕方

### 直接の `for ... in` ループ

リストは `for ... in` で直接走査できます。`List.iter` は要りません。ループ変数は `ref` ではなく、読み取り専用の要素そのものです。非 `Copy` をムーブすると `E1012` です。

```tsuzuri run=total%3D60
let values = [|10, 20, 30|]
let mut total = 0
for x in values do
    total = total + x
$"total={total}"
```

実行結果:

```text
total=60
```

この直接走査は一時的なヒープメモリを一切割り当てず、ノードのリンクを直接たどるため非常に高効率です。

### `List.iter` による反復

[Seq](./seq.md) の遅延イテレータとして扱いたい場合は `List.iter (ref xs)` を呼び出します。

> [!NOTE]
> `List.iter` は、リストのリンクを 1 回走査して各要素への参照ポインタを格納した一時バッファ（Vec）を内部で準備したうえで `Seq` を返します。
> 単純にリストの全要素を 1 度走査するだけであれば、追加メモリを消費しない直接の `for ... in` ループを使用することを推奨します。

## まとめ

- 型は `[|T|]` です。`List<T>` とは書けません。`::` はパターン専用で、先頭追加は `List.cons` です。
- 長さは $O(1)$、添字は $O(index + 1)$ です。走査は直接の `for ... in` が一時メモリを使いません。
- 非 `Copy` の要素は `map_ref` と `fold_ref` で借用します。

## 関連項目

- [Array](./array.md) — 連続メモリ配列
- [Vec](./vec.md) — 伸縮可能な所有バッファ
- [Seq](./seq.md) — 遅延シーケンス
- [パターンマッチ](../pattern-matching/pattern-matching.md)
- [言語リファレンスの目次](../index.md)

