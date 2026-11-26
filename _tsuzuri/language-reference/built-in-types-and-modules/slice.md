# Slice

`Slice` は、[`Tensor`](./tensor.md)・[`Matrix`](./matrix.md)・[`MatrixView`](./matrix-view.md) の角括弧の窓構文（`ref t[i, a..b, ..]`）が使う厳格なセレクタ `Slice.Sel`（および位置 `Slice.Pos`・端 `Slice.Bound`）と、`Tensor.slice`・`MatrixView.slice` や長い形 `ref t[[...]]` に渡す丸めありのスライス記述子 `Slice.Axis` を提供する opt-in モジュールです。

## この記事のポイント

- **`Slice.Sel`（厳格なセレクタ）**: `Item of Pos`・`Span of (Bound * Bound * i64)`・`Whole`・`Rest`・`AddAxis` からなる共用体です。角括弧の窓構文 `ref t[i, 1..<3, ..]` はこの型へ脱糖されます。範囲外を丸めず、負の数を末尾から数えません（末尾からは `Pos.Back k` / `^k`）。
- **`Slice.Axis`（丸めありの旧来の記述子）**: `All`・`At of i64`・`Range of (i64 * i64)`・`From of i64`・`To of i64`・`Step of (Maybe<i64> * Maybe<i64> * i64)`・`NewAxis` からなる共用体です。負の位置を `len + i` と解釈し、範囲端を軸の端に丸めます。新しい角括弧構文に `Slice.Axis` を混ぜることはできません（`E1003`、長い形 `ref t[[...]]` で使います）。
- すべての関数は `pure` です。`Slice.rest()`（`Sel.Rest`、NumPy の `...`）や `Slice.Sel.AddAxis`（長さ 1 の新しい軸）を角括弧の成分として直接書けます。

## 使い方

```tsuzuri run=%5B0%2C%204%2C%208%5D%20%5B11%2C%2010%2C%209%2C%208%2C%207%2C%206%2C%205%2C%204%5D
let t = Tensor.init [3, 4] (\i -> i)
let col0 = ref t[Slice.rest(), 0]
let sub = ref t[[Slice.step 2 0 (-1), Slice.rev()]]
$"{Tensor.to_array (Tensor.to_tensor (ref col0))} {Tensor.to_array (Tensor.to_tensor (ref sub))}"
```

実行結果:

```text
[0, 4, 8] [11, 10, 9, 8, 7, 6, 5, 4]
```

窓を返す添字には `ref` を付け、土台は局所の値（`let t = ...`）または窓からさらに切り出す窓の連鎖（`ref t[1..2, ..][0, ..]`、`ref t[[Slice.range 1 3]][[Slice.at 1]]`）にします。

## 厳格な型（`Pos`・`Bound`・`Sel`）

| 型 | ケース | 意味 |
| --- | --- | --- |
| `Slice.Pos` | `Front of i64` / `Back of i64` | 先頭からの位置 `i`（0 が先頭）、または末尾からの位置 `len - 1 - k`（`^k`、`Back 0` が最後）。範囲外はトラップ |
| `Slice.Bound` | `Unbounded` / `Included of Pos` / `Excluded of Pos` | 刻みの向きの軸端、その位置を含む境界、その位置を含まない境界 |
| `Slice.Sel` | `Item of Pos` | 1 つの位置（Tensor は軸を除き、Matrix は長さ 1 で残す） |
| | `Span of (Bound * Bound * i64)` | 範囲と刻み（`step == 0` や負の刻みと `Excluded` の組み合わせはトラップ） |
| | `Whole` | 軸全体（単独 `..`） |
| | `Rest` | 残りの軸すべてを `Whole` で埋める（キーに 1 つまで。`Slice.rest()`） |
| | `AddAxis` | 長さ 1・ストライド 0 の新しい軸を挿入する（入力の軸を消費しない） |

## API リファレンス

| 関数 | 説明 |
| --- | --- |
| `Slice.rest()` | 必要な数だけの全軸セレクタに展開される `Sel.Rest`（`ref t[Slice.rest(), 0]`） |
| `Slice.resolve_sel len sel` | 長さ `len` の軸に対して `ref Sel` を厳格に検査・解決し、`Slice.Resolved`（`Index` / `Slice (start, count, step)` / `InsertAxis`）を返す（`Rest` を直接渡すとトラップ） |
| `Slice.all()` | 軸全体を選択する（`Axis.All`） |
| `Slice.at index` | 1 位置を選んでその軸を落とす（負の添字対応、`Axis.At index`） |
| `Slice.range start stop` | 半開区間 `[start, stop)`（ステップ 1、端へ丸める） |
| `Slice.from start` | `start` から末尾まで（ステップ 1、端へ丸める） |
| `Slice.until stop` | 先頭から `stop` 未満まで（ステップ 1、端へ丸める） |
| `Slice.step start stop stride` | `start` から `stop` 手前まで `stride` 刻み（負ステップで逆順、端へ丸める） |
| `Slice.every stride` | 軸全体を `stride` 刻み（`Axis`） |
| `Slice.rev()` | 軸全体を逆順（ステップ `-1`、`Axis`） |
| `Slice.new_axis()` | 長さ 1 の新しい軸を挿入する（`Axis.NewAxis`） |
| `Slice.resolve len axis` | 長さ `len` に対して `ref Axis` を丸めありで `Slice.Resolved` へ正規化する |

### 引数の順序

標準ライブラリ全体の規則は、[標準ライブラリの引数の順序](./argument-order.md)にまとめてあります。`Slice.resolve len axis` と `Slice.resolve_sel len sel` は、それぞれ対象の `ref Axis`・`ref Sel` を最後の引数に取ります。

## 関連項目

- [Tensor](./tensor.md) — N 次元配列と `Tensor.slice`
- [MatrixView](./matrix-view.md) — 2 次元窓と `MatrixView.slice`
- [型クラス](../types-and-type-inference/type-classes.md) — 汎用 `Index` / `IndexMut`
- [言語リファレンスの目次](../index.md)
