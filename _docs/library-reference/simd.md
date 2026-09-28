# SIMD ベクトルと lane mask

[ドキュメントのトップ](../README.md)

明示的な 128-bit ベクトル型と lane 単位の mask を使えます。これは自動ベクトル化とは別の言語機能です。型を使っただけで、すべてのターゲットで単一命令や高速化を保証するものではありません。

## 型

| lane | 数値ベクトル | mask |
| --- | --- | --- |
| 8-bit、16 lane | `i8x16`, `i8ux16` | `mask8x16` |
| 16-bit、8 lane | `i16x8`, `i16ux8` | `mask16x8` |
| 32-bit、4 lane | `i32x4`, `i32ux4`, `f32x4` | `mask32x4` |
| 64-bit、2 lane | `i64x2`, `i64ux2`, `f64x2` | `mask64x2` |

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
| `Simd.of_lanes2`, `of_lanes4`, `of_lanes8`, `of_lanes16` | lane 値を順に指定。型と個数が一致する必要がある |
| `Simd.extract vector index` | i64 の位置から scalar。負数・範囲外はトラップ |
| `Simd.replace vector index scalar` | 一 lane を置換した新しい値。範囲外はトラップ |
| `Simd.load array index` | 共有配列から数値ベクトルを読み出す |
| `Simd.sum_lanes vector` | lane 0 から順番に加算して scalar を返す |

sum_lanes は正のゼロからではなく lane 0 から始めます。浮動小数点の順序をベクトル幅に応じて再結合しません。

## 比較と mask

`Simd.eq`, `ne`, `lt`, `le`, `gt`, `ge` は二つの数値ベクトルから同形状の mask を返します。通常の Eq / Ord インスタンスはなく、ベクトルの `==` が一つの bool を返す API ではありません。

`Simd.select mask yes no` は lane ごとに選択します。all / any は mask を一つの bool へ還元します。選択は通常の厳格評価の関数なので、yes / no の引数式を分岐のように遅延するわけではありません。

NaN の eq / 大小比較は false、ne は true です。正負のゼロは等しいままで、scalar の比較規則を維持します。

## 対応演算

数値ベクトルは加減乗算、浮動小数点ベクトルは除算、符号付き整数・浮動小数点ベクトルは符号反転を使えます。整数と mask は Bits に対応します。

整数は lane ごとに折り返し、シフト量は各 lane の幅 - 1 でマスクします。整数除算・剰余、暗黙の scalar / vector 変換はありません。

組み込み制約は `SimdVector`、`SimdNumeric`、`SimdMask` です。lane 型・mask 型が結果に現れる API は、呼び出し位置で具体的なベクトル型を決める必要があります。

## ターゲットと制限

native は LLVM の対応する演算へ下げます。WASM は既定で scalar fallback、`--wasm-feature simd128` を付けると v128 を利用する成果物になります。未対応エンジンで opt-in 成果物を実行すると validation に失敗し、既定経路へ自動で差し替わりません。

256-bit 型、gather / scatter、store、可変スライスへの書き込み、integer division、relaxed-simd は提供しません。fast-math や暗黙 FMA も有効にしません。

## 関連項目

- [数値の意味](../language-reference/numbers.md)
- [Parallel](parallel.md)
- [性能と CPU 選択](../guides/performance.md)
