# Slice

`Slice` は、[`Tensor`](./tensor.md)・[`Matrix`](./matrix.md)・[`MatrixView`](./matrix-view.md) の汎用添字（[`Index`](../types-and-type-inference/type-classes.md)）や `Tensor.slice`・`MatrixView.slice` に渡す 1 軸分のスライス記述子 `Slice.Axis` を提供する opt-in モジュールです。

## この記事のポイント

- `Slice.Axis` は `All`・`At of i64`・`Range of (i64 * i64)`・`From of i64`・`To of i64`・`Step of (Maybe<i64> * Maybe<i64> * i64)`・`NewAxis` からなる値型共用体です。
- 負の位置は軸の長さ `len` に対して `len + i` と解釈します。範囲指定は軸の端に丸められ、範囲外の `At` と `step == 0` は `Slice.resolve` でトラップします。
- すべての関数は `pure` であり、新しい予約語や専用構文を増やさずに `t[[Slice.step 1 4 2, Slice.rev()]]` のように書けます。

## 使い方

```tsuzuri run=[11,%2010,%209,%208,%207,%206,%205,%204]
let t = Tensor.init [3, 4] (\i -> i)
let sub = t[[Slice.step 2 0 (-1), Slice.rev()]]
Tensor.to_array (Tensor.to_tensor (ref sub))
```

実行結果:

```text
[11, 10, 9, 8, 7, 6, 5, 4]
```

> 既知の不具合: 結果が土台を借りる添字（`t[[Slice.range 1 3]]`、`m[(Slice.range 0 2, Slice.all())]`、参照・関数・region を持つレコードなど借用を運ぶ型を返す利用者の `Index`）の土台に一時値を書くと、検査を通り、窓は解放した後のメモリを読みます。`(mk 100)[[Slice.range 1 3]]` ではなく、`let base = mk 100` と束縛してから `base[[Slice.range 1 3]]` と書いてください。また、`Tensor.Mut` など借用を持つ型に利用者が定義した `Index` の窓は、生きている間も土台への書き込みを止めません。修正は [A37](../../../_features/A37-index-base-place-borrow.md) です。

## API リファレンス

| 関数 | 説明 |
| --- | --- |
| `Slice.all()` | 軸全体を選択する（`Axis.All`） |
| `Slice.at index` | 1 位置を選んでその軸を落とす（負の添字対応、`Axis.At index`） |
| `Slice.range start stop` | 半開区間 `[start, stop)`（ステップ 1） |
| `Slice.from start` | `start` から末尾まで（ステップ 1） |
| `Slice.until stop` | 先頭から `stop` 未満まで（ステップ 1） |
| `Slice.step start stop stride` | `start` から `stop` 手前まで `stride` 刻み（負ステップで逆順） |
| `Slice.every stride` | 軸全体を `stride` 刻み |
| `Slice.rev()` | 軸全体を逆順（ステップ `-1`） |
| `Slice.new_axis()` | 長さ 1 の新しい軸を挿入する（入力軸を消費しない） |
| `Slice.resolve len axis` | 長さ `len` に対して `Axis` を `Slice.Resolved`（`Index` / `Slice (start, count, step)` / `InsertAxis`）へ正規化する |

### 引数の順序

標準ライブラリ全体の規則は、[標準ライブラリの引数の順序](./argument-order.md)にまとめてあります。`Slice.resolve len axis` は対象の `ref Axis` を最後の引数に取ります。

## 関連項目

- [Tensor](./tensor.md) — N 次元配列と `Tensor.slice`
- [MatrixView](./matrix-view.md) — 2 次元窓と `MatrixView.slice`
- [型クラス](../types-and-type-inference/type-classes.md) — 汎用 `Index` / `IndexMut`
- [言語リファレンスの目次](../index.md)
