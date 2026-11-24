# Complex

`Complex<'a>` は、実部 `re` と虚部 `im` を持つ複素数レコードです。`std/Complex.tz` の opt-in モジュールとして提供され、`Complex` を名指したプログラムだけが読み込みます。

## この記事のポイント

- `record Complex<'a> { re: 'a, im: 'a }` は `Copy` 値型レコードです（`'a` は `f32`・`f64` など `Float<'a>` を満たす型）。
- 除算 `Complex.div`（および `/` 演算子）は入力の大きさを補正した Smith のスケーリング法を用い、`|re|` や `|im|` が極端に大きいときの中間オーバーフローを抑えます（極端な指数差や非有限値では IEEE 754 の結果になります）。
- `Add`・`Sub`・`Mul`・`Div`・`Neg`・`Eq`・`Default`・`Display` インスタンスを備えます。
- `std/Complex.tz` のすべての関数は `pure` であり、ヒープ確保を行いません。

## 構築と演算

```tsuzuri run=4+6i%205%20true
let a = Complex.of 1.0 2.0
let b = Complex.of 3.0 4.0
let sum = a + b
let mag = Complex.abs b
let q = Complex.div (Complex.of 1.0e200 2.0e200) (Complex.of 1.0e200 0.0)
$"{sum} {mag} {q.re == 1.0 && q.im == 2.0}"
```

実行結果:

```text
4+6i 5 true
```

## API リファレンス

| 関数 | 説明 |
| --- | --- |
| `Complex.of re im` | `re + im*i` を構築する |
| `Complex.from_real re` / `Complex.from_imag im` | 実軸上・虚軸上の複素数を構築する |
| `Complex.polar r theta` | 極座標 `(r, theta)` から構築する |
| `Complex.zero ()` / `Complex.one ()` / `Complex.i ()` | `0+0i`・`1+0i`・`0+1i` |
| `Complex.real z` / `Complex.imag z` | 実部・虚部を取り出す |
| `Complex.add` / `Complex.sub` / `Complex.mul` / `Complex.div` / `Complex.neg` | 四則演算・符号反転（`+`・`-`・`*`・`/`・単項 `-` と同等） |
| `Complex.scale factor z` / `Complex.conj z` | 実数倍・複素共役 |
| `Complex.norm_sq z` / `Complex.abs z` / `Complex.arg z` | ノルム二乗・絶対値（`Math.hypot`）・偏角（`Math.atan2`） |
| `Complex.sqrt` / `Complex.exp` / `Complex.log` / `Complex.pow` | 主平方根・指数・主対数・べき乗 |
| `Complex.sin` / `Complex.cos` / `Complex.tan` | 複素三角関数 |
| `Complex.sinh` / `Complex.cosh` / `Complex.tanh` | 複素双曲線関数 |
| `Complex.is_nan` / `Complex.is_infinite` / `Complex.is_finite` | 非数・無限大・有限判定 |

### 引数の順序

標準ライブラリ全体の規則は、[標準ライブラリの引数の順序](./argument-order.md)にまとめてあります。単項演算および `Complex.scale factor z` は対象の複素数を最後の引数に取ります。`add`・`sub`・`mul`・`div`・`pow` は通常の数式順（左被演算子、右被演算子）です。

## 関連項目

- [Math](./math.md) — 浮動小数点の基本・双曲線・初等関数
- [Int](./int.md) — 整数演算（`gcd`・`lcm`・`div_floor`・`rem_floor`）
- [言語リファレンスの目次](../index.md)
