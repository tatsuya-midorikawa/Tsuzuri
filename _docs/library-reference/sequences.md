# Seq と反復プロトコル

[ドキュメントのトップ](../README.md)

`Seq<T>` は同期的な、一回消費する遅延列です。同じ列を自由に何度も列挙する IEnumerable ではありません。必要なら新しい列を作る関数を用意します。

## 遅延変換

```tsuzuri run=36
let sequence = Seq.unfold 0 (fx value ->
    if value < 10 then Option.Some (value, value + 1) else Option.None)
let doubled = Seq.map sequence (fx value -> value * 2)
let selected = Seq.filter doubled (fx value -> deref value % 3 == 0)
let result = Seq.to_array selected
Array.sum ref result
```

map は要素を消費して変換し、filter は要素を共有借用して判定します。配列化まで必ず全要素を生成するわけではなく、next や for が列を進めます。

## API

| API の引数順 | 型・意味 |
| --- | --- |
| `empty()` | 空の Seq |
| `once value` | 一つの所有値だけを返す Seq |
| `defer step` | `unit -> (Seq<T> * Option<T>)` を次の要求まで遅延 |
| `unfold state generator` | `State -> Option<(T * State)>` で反復 |
| `next sequence` | sequence を消費し、次の Seq と `Option<T>` を返す |
| `map sequence transform` | `T -> U` を遅延適用 |
| `filter sequence predicate` | `ref T -> bool` に一致する要素を遅延選択 |
| `to_array sequence` | 列を消費して所有配列を作る |

Seq の関数名を使います。map / filter は sequence が先です。unfold の State には Capture、map / filter が保持する入力要素にも Capture が必要です。

once は所有文字列や Task も保持できますが、それが一般の再利用可能な callback で捕捉可能になるわけではありません。未消費の要素は列の破棄時に解放します。

## for と中断

```tsuzuri run=42
let sequence = Seq.once 42
let mut answer = 0
for value in sequence do
    answer = value
answer
```

for は Seq を一度だけ消費します。各 step で次の状態を受け取ってから本体を実行し、break / continue は通常のループ規則に従います。中断したときも残った列や未消費の値を適切に解放します。

内部表現は不透明です。フィールドを直接構築・更新・分解できません。next は所有する closure 環境を複製せずに呼び出します。

## 既存コレクションの借用列

```tsuzuri run=8
let words = ["red", "green"]
let mut total = 0
for word in Array.iter (ref words) do
    total = total + word.length
total
```

Array.iter、List.iter、Vec.iter、Set.iter は `Seq<ref T>` を返します。Map.iter は `Seq<(ref K * ref V)>` です。元のコレクションを複製せず、参照が生きている間は所有者の変更や move を禁止します。

List.iter は最初に O(n) 時間・領域で要素参照の Vec を作り、その後 O(n) で列挙します。要素自体は複製しません。List を一回走査するだけなら直接の for はこの準備領域を必要としません。

## 利用者定義の反復

独自型には `Module.iter ref source` のように Seq を返す関数を定義し、for の列挙式で明示的に呼びます。iter という名前を持つだけで、自動的に言語の反復対象として登録されることはありません。

直接の配列・List・文字列・整数範囲の for は従来の走査経路を使います。ビルダーの For も、Seq の for へ勝手に置換しません。

## 寿命と停止性

借用を返す列は元の所有者より長生きできません。反復本体の一時ローカルへの参照を次の状態や外側へ持ち出すこともできません。

unfold や defer は無限列を作れます。to_array は全体を消費するため、無限列に対して停止を保証せず、メモリ不足になる場合があります。非同期 I/O やバックプレッシャーを提供する API ではありません。

## API と関連項目

- [Seq のソース宣言](api/Seq.md): next はコンパイラ組み込み
- [Array と List](arrays-and-lists.md)
- [for と break](../language-reference/control-flow.md)
