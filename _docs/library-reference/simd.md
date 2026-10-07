# SIMD ベクトルと lane mask

[ドキュメントのトップ](../README.md)

明示的な 128-bit・256-bit ベクトル型と lane 単位の mask を使えます。これは自動ベクトル化とは別の言語機能です。型を使っただけで、すべてのターゲットで単一命令や高速化を保証するものではありません。

## 型

| lane | 128-bit | 256-bit | mask（128-bit / 256-bit） |
| --- | --- | --- | --- |
| 8-bit | `i8x16`, `i8ux16` | `i8x32`, `i8ux32` | `mask8x16` / `mask8x32` |
| 16-bit | `i16x8`, `i16ux8` | `i16x16`, `i16ux16` | `mask16x8` / `mask16x16` |
| 32-bit | `i32x4`, `i32ux4`, `f32x4` | `i32x8`, `i32ux8`, `f32x8` | `mask32x4` / `mask32x8` |
| 64-bit | `i64x2`, `i64ux2`, `f64x2` | `i64x4`, `i64ux4`, `f64x4` | `mask64x2` / `mask64x4` |

256-bit の型は lane 数が倍になるだけで、API・演算・意味は 128-bit の型と同じです。AVX2 を使わない x86 や AArch64 では LLVM が 2 つの 128-bit 演算に分けます。

全型が Copy / Send / Capture で、解放処理は不要です。レコード、union、タプル、コレクション、クロージャーへ格納できますが、ホスト ABI へ直接公開できません。

## 読み出しと演算

```tsuzuri run=14
let source = [1i32, 2i32, 3i32, 4i32]
let values: i32x4 = Simd.load (ref source) 0
let shifted = values + Simd.splat 1i32
let increased = Simd.gt shifted values
assert (Simd.all increased)
let selected = Simd.select increased shifted values
Simd.sum_lanes selected as i64
```

Simd.load は全 lane が範囲内かを読み出し前に検査します。連続領域の unaligned load に対応します。型の文脈でベクトル形状を決め、scalar を勝手にベクトルへ変換しません。

## 構築と lane 操作

| API | 引数・結果 |
| --- | --- |
| `Simd.splat scalar` | 全 lane を同じ値にする。結果型を指定 |
| `Simd.of_lanes2`, `of_lanes4`, `of_lanes8`, `of_lanes16`, `of_lanes32` | lane 値を順に指定。型と個数が一致する必要がある |
| `Simd.extract vector index` | i64 の位置から scalar。負数・範囲外はトラップ |
| `Simd.replace vector index scalar` | 一 lane を置換した新しい値。範囲外はトラップ |
| `Simd.load array index` | 共有配列から数値ベクトルを読み出す |
| `Simd.store slice index vector` | 排他スライスへ数値ベクトルを書き込む |
| `Simd.sum_lanes vector` | lane 0 から順番に加算して scalar を返す |

sum_lanes は正のゼロからではなく lane 0 から始めます。浮動小数点の順序をベクトル幅に応じて再結合しません。

## 書き込み

```tsuzuri run=111
let mut values: [i32] = [1i32, 2i32, 3i32, 4i32, 5i32, 6i32, 7i32, 8i32, 9i32]
let lanes: i32x8 = Simd.load (ref values) 1
Simd.store (ref mut values[1..]) 0 (lanes * Simd.splat 10i32)
values[0] + values[1] + values[8]
```

Simd.store は [排他スライス](arrays-and-lists.md#排他スライスとその場の更新) `ref mut [T..]` を受け取り、全 lane が範囲内かを書き込み前に検査します。範囲外ならどの要素も書き換えずにトラップします。境界の揃っていない位置にも書き込めます。

## 比較と mask

`Simd.eq`, `ne`, `lt`, `le`, `gt`, `ge` は二つの数値ベクトルから同形状の mask を返します。通常の Eq / Ord インスタンスはなく、ベクトルの `==` が一つの bool を返す API ではありません。

`Simd.select mask yes no` は lane ごとに選択します。all / any は mask を一つの bool へ還元します。選択は通常の厳格評価の関数なので、yes / no の引数式を分岐のように遅延するわけではありません。

NaN の eq / 大小比較は false、ne は true です。正負のゼロは等しいままで、scalar の比較規則を維持します。

## 対応演算

数値ベクトルは加減乗算、浮動小数点ベクトルは除算、符号付き整数・浮動小数点ベクトルは符号反転を使えます。整数と mask は Bits に対応します。

整数は lane ごとに折り返し、シフト量は各 lane の幅 - 1 でマスクします。整数除算・剰余、暗黙の scalar / vector 変換はありません。

組み込み制約は `SimdVector`、`SimdNumeric`、`SimdMask` です。lane 型・mask 型が結果に現れる API は、呼び出し位置で具体的なベクトル型を決める必要があります。

## ターゲットと制限

native は LLVM の対応する演算へ下げます。命令セットは `--cpu` で決まり、実行時に AVX2 などを選ぶには関数へ [`@cpu`](../guides/performance.md#利用者関数の多版化cpu) を付けます。WASM は既定で scalar fallback、`--wasm-feature simd128` を付けると v128 を利用する成果物になります（256-bit の型は 2 つの v128）。未対応エンジンで opt-in 成果物を実行すると validation に失敗し、既定経路へ自動で差し替わりません。

512-bit 型、gather / scatter、integer division、relaxed-simd は提供しません。fast-math や暗黙 FMA も有効にしません。

## 関連項目

- [数値の意味](../language-reference/numbers.md)
- [Parallel](parallel.md)
- [性能と CPU 選択](../guides/performance.md)
