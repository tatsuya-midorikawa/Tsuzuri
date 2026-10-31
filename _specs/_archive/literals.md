# リテラル

## 基本型一覧

| 型 | 説明 | プレフィックス/サフィックス | 使用例 |
| --- | --- | --- | --- |
| `i8` / `sbyte` | signed 8-bit integer | `y` | `86y`, `0b00000101y` |
| `i8u` / `byte` | unsigned 8-bit natural number | `uy` | `86uy`, `0b00000101uy` |
| `byte` | ASCII character | `B` | `'a'B` |
| `byte[]` | ASCII string | `B` | `"test"B` |
| `utf8char[]` | Unicode scalar string | `B` | `u8"😊test"B` |
| `i16` | signed 16-bit integer | `s` | `86s` |
| `i16u` | unsigned 16-bit natural number | `us` | `86us` |
| `i32` | signed 32-bit integer | none | `86` |
| `i32u` | unsigned 32-bit natural number | `u` | `86u` |
| `i64` | signed 64-bit integer | `l` | `86l` |
| `i64u` | unsigned 64-bit natural number | `ul` | `86ul` |
| `i128` | signed 128-bit integer | `L` | `86L` |
| `i128u` | unsigned 128-bit natural number | `UL` | `86UL` |
| `bigint` | integer not limited to 64-bit representation | `I` | `9999999999999999999999999999I` |
| `f16` | 16-bit floating point number | `hf` | `4.14hf` |
| `f32` | 32-bit floating point number | `f` | `4.14f` |
| `f64` | 64-bit floating point number | none | `4.14` |
| `f128` | 128-bit floating point number | `F` | `4.14F` |
| `d32` | 32-bit fractional number represented as a fixed point or rational number | `hm` | `0.7833hm` |
| `d64` | 64-bit fractional number represented as a fixed point or rational number | `m` | `0.7833m` |
| `d128` | 128-bit fractional number represented as a fixed point or rational number | `M` | `0.7833M` |
| `char` | UTF-16 character | none | `'a'` |
| `utf8char` | UTF-8 character | `u8` prefix | `u8'😊'` |
| `string` | UTF-16 string | none | `"text"` |
| `utf8string` | UTF-16 string | `u8` prefix | `u8"text"` |
| `bool` | 真偽値 | none | `true` or `false` |
| `unit` | 特定の値がないことを示す型 | none | `()` |


## 定数リテラル

コンパイル時に定数として扱いたい値を `@literal` 属性でマークできる。

```tz
@literal
def PI :: f64 = 3.14

let r: f64 = 2
let area = PI * (r**2)
```

