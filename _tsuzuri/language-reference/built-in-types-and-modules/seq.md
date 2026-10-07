# Seq

`Seq<T>` は、同期的かつ **1 回消費限り（single-use）** の遅延シーケンス（ジェネレータ）型です。

全要素を一度に確保せず、要求された分だけをその場で計算します。無限列や、途中の配列を作りたくないパイプラインに向きます。

## この記事のポイント

- 型は `Seq<T>`。要素型にかかわらず常に非 Copy 型です。
- 同じシーケンスを何度も反復できる `IEnumerable` とは異なり、**1 回走査すると消費されて消滅**します。
- `Seq.empty()`、`Seq.once`、`Seq.defer`、`Seq.unfold` で生成します。
- `Seq.next` は現在のシーケンスを消費し、` (次のSeq * Maybe<要素>)` のタプルを返します。
- `Seq.map` や `Seq.filter` は遅延適用され、終端操作（`for` ループや `Seq.to_array`）が呼ばれるまで計算は実行されません。
- `for ... in` ループはシーケンスの所有権を 1 回消費して反復します。`break` で途中脱出した場合も未消費のリソースは安全に解放されます。
- 各コレクションの `iter` 関数（`Array.iter`, `Vec.iter` 等）から要素への共有借用参照を順次列挙する `Seq` を取得できます。

## 基本の書き方と生成

### `Seq.once` と `Seq.empty`

- `Seq.empty()`: 要素を持たない空のシーケンスを返します。
- `Seq.once value`: ちょうど 1 つの所有値を返すシーケンスを生成します。所有文字列や Task インスタンスも保持でき、未消費のまま破棄された場合でもリソースは漏れなく解放されます。

```tsuzuri run=value%3D42
let seq = Seq.once 42
let value = match Seq.next seq with
| (_, Some v) -> v
| (_, None) -> 0
$"value={value}"
```

実行結果:

```text
value=42
```

### 状態遷移による生成 `Seq.unfold`

状態遷移関数 `state -> Maybe<('a * state)>` から有限または無限の列を生成します。状態型には `Capture` 制約が適用されます。

```tsuzuri run=first_five%3D%5B0%2C%201%2C%202%2C%203%2C%204%5D
let numbers = Seq.unfold (\n ->
    if n < 5 then Some (n, n + 1) else None
) 0

let arr = Seq.to_array numbers
$"first_five={arr}"
```

実行結果:

```text
first_five=[0, 1, 2, 3, 4]
```

### 遅延ステップ関数 `Seq.defer`

`Seq.defer step` は、ステップ計算関数 `unit -> (Seq<'a> * Maybe<'a>)` を受け取り、`next` が実際に呼び出されるまでステップの評価を遅延します。

```tsuzuri run=deferred%3D100
let seq = Seq.defer (\() -> (Seq.empty(), Some 100))
let value = match Seq.next seq with
| (_, Some v) -> v
| (_, None) -> 0
$"deferred={value}"
```

実行結果:

```text
deferred=100
```

## 遅延変換とパイプライン

`Seq.map` と `Seq.filter` はコールバック関数を第 1 引数、シーケンスを第 2 引数に取るため、パイプライン演算子 `|>` を用いて自然にチェーンできます。

```tsuzuri run=sum%3D36
let result =
    Seq.unfold (\v -> if v < 10 then Some (v, v + 1) else None) 0
    |> Seq.map (\x -> x * 2)
    |> Seq.filter (\r -> (*r) % 3 == 0)
    |> Seq.to_array

let sum = Array.sum (ref result)
$"sum={sum}"
```

実行結果:

```text
sum=36
```

- `Seq.map (\x -> ...)`: 要素を消費して変換します（要素型に `Capture` 制約が必要）。
- `Seq.filter (\ref_x -> ...)`: 要素の共有借用参照を受け取って判定します。条件に一致する要素のみが遅延選択されます。

各要素は終端操作（上記では `Seq.to_array`）によって要求されたタイミングでのみ順次計算されるため、途中の巨大な中間配列は一切確保されません。

## 反復プロトコルと `for ... in`

言語組み込みの `for ... in` ループに `Seq` を渡すと、反復プロトコルに従ってシーケンスが消費されます。

```tsuzuri run=total%3D15
let seq = Seq.unfold (\n -> if n <= 5 then Some (n, n + 1) else None) 1
let mut total = 0
for x in seq do
    total = total + x
$"total={total}"
```

実行結果:

```text
total=15
```

### ループの中断とメモリ安全性

ループ内で `break` や `continue` を使用した場合も、通常のループ規則に従います。途中で `break` して脱出した場合、残された未走査シーケンスおよび未消費要素はデストラクタにより安全に解放されます。

> [!WARNING]
> **ループ内ローカル参照の持ち出し禁止**
> ループ変数への参照を、その要素より長生きする変数へ入れると `E1013` です。たとえば `for item in Seq.once "inner" do value = ref item` のあとで `value` を使う場合です。

## 各コレクションからの `iter`

すべての標準コレクションは、各要素への共有借用参照を列挙する `iter` 関数を提供しています。

| コレクション | 呼び出し | 返却型 | 特徴 |
| --- | --- | --- | --- |
| **Array** | `Array.iter (ref xs)` | `Seq<ref 'a>` | 配列要素の参照をインデックス昇順に列挙 |
| **Vec** | `Vec.iter (ref v)` | `Seq<ref 'a>` | バッファ要素の参照をインデックス昇順に列挙 |
| **List** | `List.iter (ref xs)` | `Seq<ref 'a>` | 一時 Vec にポインタを準備（$O(n)$ 領域）してから列挙 |
| **Set** | `Set.iter (ref s)` | `Seq<ref 'key>` | 要素の参照をキー昇順に列挙 |
| **HashSet** | `HashSet.iter (ref s)` | `Seq<ref 'key>` | 要素の参照を内部エントリ順に列挙 |
| **Map** | `Map.iter (ref m)` | `Seq<(ref 'key * ref 'value)>` | キーと値の参照タプルをキー昇順に列挙 |
| **HashMap** | `HashMap.iter (ref m)` | `Seq<(ref 'key * ref 'value)>` | キーと値の参照タプルを内部エントリ順に列挙 |

```tsuzuri run=total%3D7
let words = ["Tsu", "zuri"]
let mut total = 0
for word in Array.iter (ref words) do
    total = total + word.length
$"total={total}"
```

実行結果:

```text
total=7
```

`iter` で作った `Seq` が生きている間、元のコレクションの移動・更新・排他借用は拒否されます。

直接の `for ... in` が使えるのは、配列、リスト、`Vec`、文字列、`utf8string`、整数範囲、そして `Seq` です。これらに `Module.iter` は要りません。`Map`、`Set`、`HashMap`、`HashSet` とユーザー定義型は対象外で、`for pair in Map.iter (ref map)` のように `Seq` を返す関数を自分で呼びます。省略すると `E1005` です。`iter` という名前だけでは反復対象になりません。コンピュテーション式の `For` も、`Seq` の反復へは書き換わりません。

直接の `for` のループ変数は `ref` ではなく、読み取り専用の要素そのものです。`Array.iter` や `Vec.iter` が返す `Seq<ref T>` を回すときのループ変数は `ref T` です。

> [!NOTE]
> `List.iter` は、リストのリンクを 1 度走査して各要素への参照ポインタを格納した一時 Vec を準備するため、前準備に $O(n)$ の時間と一時メモリを消費します。
> 単純に 1 回リストを走査するだけであれば、前準備不要の直接の `for x in xs` を使用してください。

## 配列への変換と停止性

`Seq.to_array sequence` は、シーケンス全体を消費し、可変長バッファ（Vec）に集約した後に固定長の所有配列 `[T]` へ移送して返します。

> [!WARNING]
> **無限シーケンスに対する注意**
> `Seq.unfold` や再帰的な `Seq.defer` を用いて無限列を生成した場合、`Seq.to_array` を呼び出すとループが終了せずメモリが枯渇するまで停止しません。
> 無限列を扱う際は、`for` ループ内で条件に応じて明示的に `break` してください。

## API リファレンス

すべての関数は `std::Seq` モジュール（または言語組み込み）に属しています。

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `empty` | `fn() -> Seq<'a>` | 空のシーケンスを作ります。呼び出しは `Seq.empty()` です |
| `once` | `'a -> Seq<'a>` | 単一の所有値を持つシーケンスを生成します |
| `defer` | `(unit -> (Seq<'a> * Maybe<'a>)) -> Seq<'a>` | 次のステップ計算を遅延するシーケンスを生成します |
| `unfold` | `Capture<'state> => ('state -> Maybe<('a * 'state)>) -> 'state -> Seq<'a>` | 状態遷移関数からシーケンスを生成します |
| `next` | `Seq<'a> -> (Seq<'a> * Maybe<'a>)` | シーケンスを 1 ステップ進め、次状態と要素を返します（消費） |
| `map` | `Capture<'a> => ('a -> 'b) -> Seq<'a> -> Seq<'b>` | 各要素に関数を遅延適用するシーケンスを返します |
| `filter` | `Capture<'a> => (ref 'a -> bool) -> Seq<'a> -> Seq<'a>` | 述語を満たす要素だけを遅延選択するシーケンスを返します |
| `to_array` | `Seq<'a> -> ['a]` | シーケンス全体を消費して所有配列を構築します |

## 計算量

| 操作 | 計算量 | 備考 |
| --- | --- | --- |
| 生成 `empty` / `once` / `defer` | $O(1)$ | レコード・クロージャの生成 |
| ステップ取得 `Seq.next` | 先頭の値があれば $O(1)$、無ければステップ関数 1 回分 | `map` は上流の 1 ステップと変換 1 回。`filter` は条件を満たす要素か終端まで上流を進めるので、1 回の `next` で任意個の要素を調べることがある |
| 遅延変換 `map` / `filter` | $O(1)$ | ラッパーシーケンスの生成（要素計算は遅延） |
| 全消費走査 `for ... in` | $O(n)$ | 全要素のステップ実行。$n$ は上流から取り出す要素の数で、`filter` で捨てる要素も含む |
| 配列化 `Seq.to_array` | $O(n)$ | 全要素を Vec へ push 後に配列化 |

## 所有権とライフタイム

- `Seq<T>` は 1 回だけ消費する所有値です。`Seq.next` や `for ... in` のあとに同じ変数を使うと `E1012` です。
- 内部は不透明です。フィールド参照やパターン分解は `E1022` です。

## まとめ

- `Seq<T>` は同期的で、1 回消費するとなくなります。同じ列を何度も歩く `IEnumerable` ではありません。
- `map` と `filter` は遅延です。計算が走るのは `next`、`for`、`to_array` のときです。
- 配列やリストの直接の `for` は `Seq` を経由しません。借用を返す `iter` は、元の所有者が生きている間だけ使えます。

## 関連項目

- [Array](./array.md) — 連続メモリ配列
- [Vec](./vec.md) — 伸縮可能な所有バッファ
- [List](./list.md) — 単方向連結リスト
- [for...in 式](../loops-and-conditionals/for-in.md) — ループ構文
- [言語リファレンスの目次](../index.md)

