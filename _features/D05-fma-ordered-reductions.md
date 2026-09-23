# D05: 明示 FMA と順序を定めた集計 API
| 項目 | 内容 |
|---|---|
| ID | D05 |
| 優先度 | P2 |
| 規模 | S |
| 依存 | E02, C04 |
| 後続 | F02, F04, F05 |
| 状態 | todo |
| 主な影響ファイル | `std/Math.tz`, `std/Array.tz`, `src/check.rs`, `src/llvm.rs`, `src/runtime/numeric.c`, `src/runtime/generate.py`, `src/runtime/wasm.ll`, `tests/fma_reductions.rs`, `tests/fma_reductions.mjs`, `benchmarks/`, `docs/language.md`, `docs/architecture.md`, `docs/benchmarks.md`, `README.md` |

## 目的

浮動小数点の積和を「暗黙に」融合せず、ユーザーが明示したときだけ単一丸め FMA を使えるようにする。
同時に、配列の合計・内積の集計順序を API 名で固定し、D-14 の決定性を守る。
既存の `a * b + c`、`Array.sum`、通常の fold は左から右の逐次意味を変えない。
SIMD・並列・FMA を使う余地は、`Math.fma`、`Array.sum_pairwise`、`Array.sum_kahan`、`Array.dot_fma` のような明示 API に限定する。

## 現状

`src/llvm.rs` の `FunctionEmitter::binary` は f32/f64 で `fmul` と `fadd` を別命令として出す。
fast-math flag は付けない。
`docs/architecture.md` は fast-math、再結合、暗黙 FMA を禁止している。
`docs/benchmarks.md` の C++ 比較では `-ffp-contract=off` を指定し、Tsuzuri と演算順を揃えている。
D03 で `Math` モジュールと float generic builtin dispatch が入る前提。
C04 で `Array` bulk API と `&[T]` slice 処理が入る前提。
`src/runtime/numeric.c` は f16/f128/decimal の add/mul/div は持つが FMA はない。
WASM には f32/f64 FMA 命令がない。
platform libm の `fma` を呼ぶと WASM import/libcall 問題と結果差が出る。

## 仕様

### 前提とする他チケットのインターフェース

E02 は `Math.fma` と `Array.sum_pairwise` の修飾 builtin/std 関数を提供できる。
C04 は `Array` モジュール、`&[T]` slice、左から右の `Array.sum`/fold の基礎を提供している。
D03 は `Float 'a` class、Math runtime、numeric kind dispatch を提供している前提。
B01/A08/C02 は必須ではない。

### 他チケットへの提供インターフェース

F02 は並列 reduction を実装するとき、D05 の固定 tree shape を分割境界として使う。
F04 は SIMD dot/sum を実装するとき、同じ lane grouping と final fold order を守る。
F05 は CPU FMA 命令の dispatch を実装するとき、`Math.fma` と `Array.dot_fma` だけに適用する。
D03 は `Math.fma` を basic math API とは別扱いにする。

### API

```text
Math.fma :: Float 'a => 'a -> 'a -> 'a -> 'a
```

`Math.fma a b c` は数学的な `a * b + c` を無限精度で計算し、対象型へ一度だけ丸める。
評価順序は `a`, `b`, `c`。
NaN、±inf、±0、subnormal は IEEE 754 fusedMultiplyAdd に従う。
`a * b` と `+ c` を別々に丸めてはいけない。
`a * b + c` の通常式は引き続き別々に丸める。
コンパイラは `a * b + c` を `Math.fma a b c` へ暗黙変換しない。

配列:

```text
Array.sum_pairwise :: Float 'a => &['a] -> 'a
Array.sum_kahan    :: Float 'a => &['a] -> 'a
Array.dot          :: Float 'a => &['a] -> &['a] -> 'a
Array.dot_fma      :: Float 'a => &['a] -> &['a] -> 'a
```

実際の型構文は C03/C04 に合わせて `&[T]` と書く。
空配列の sum は `0`。
`Array.dot`/`dot_fma` は長さ不一致ならトラップ。
`dot` の空配列は `0`。
戻り型は要素型と同じ。
整数版は C04/D04 の範囲で扱い、このチケットでは float に限定する。

### 集計順序

`Array.sum_pairwise` は入力長だけで決まる固定 tree。
定義:
1. level 0 は元配列 `x[0..n)`。
2. 各 level で `(0,1)`, `(2,3)`, ... の隣接 pair を左から右に加算し、奇数個なら最後の要素をそのまま次 level へ運ぶ。
3. 1 要素になるまで繰り返す。
4. 各加算は通常の `+` であり、FMA や再結合ではない。

例:
`[a,b,c,d,e]` は level1 `[a+b, c+d, e]`、level2 `[(a+b)+(c+d), e]`、level3 `((a+b)+(c+d))+e`。
この tree はスレッド数、SIMD 幅、CPU に依存しない。
並列化する場合も同じ pair を同じ level で計算する。

`Array.sum_kahan` は Neumaier 補償和を使う。
名前は互換的に `sum_kahan` とするが、仕様では Kahan-Babuska-Neumaier algorithm と明記する。
順序は入力 index 0 から n-1。
各 step:

```text
t = sum + x
if abs(sum) >= abs(x) then c = c + ((sum - t) + x)
else c = c + ((x - t) + sum)
sum = t
```

戻り値は `sum + c` を最後に一回加算したもの。
この加算も通常の `+`。
NaN が出た場合は通常の float 演算規則に従う。

`Array.dot` は index 0 から n-1 の左から右。
各 step は `acc = acc + (xs[i] * ys[i])`。
multiply と add は別丸め。
`Array.dot_fma` は index 0 から n-1 の左から右。
各 step は `acc = Math.fma xs[i] ys[i] acc`。
FMA は明示 API なので単一丸め。
pairwise dot はこのチケットでは提供しない。

## 設計

### Math.fma lowering

`Builtin::MathFma` を追加する。
signature は `Float 'a => 'a -> 'a -> 'a -> 'a`。
f32/f64 native は `llvm.fma.f32/f64` を使ってよい。
ただし clang/LLVM が libm `fma` call へ下げる target では使わず、runtime path にする。
WASM は fma 命令がないので bundled exact implementation を使う。
f16/f128/decimal は `numeric.c` の soft implementation。

`numeric.c` に追加:

```c
void tz_soft_fma(unsigned char *out, const unsigned char *a, const unsigned char *b, const unsigned char *c, int kind);
```

binary/decimal とも、decode した `a*b+c` を多倍長整数で exact に合成し、`pack` で一度だけ丸める。
special value は IEEE fusedMultiplyAdd の表を先に処理する。
`0 * inf + nan` は NaN。
`inf * 0 + finite` は NaN。
符号付きゼロの結果は IEEE 規則に従う。
アルゴリズムをコメントで説明するが、第三者 libm ソースはコピーしない。

### Array reductions

C04 の Array module 実装を拡張する。
小さい配列では std Tsuzuri 実装でもよい。
ただし pairwise tree shape を compiler/runtime 最適化で変えてはいけない。
性能経路:
1. まず正しい Tsuzuri 実装。
2. 必要なら builtin specialization で loops を生成。
3. SIMD/parallel は後続チケットで同じ tree shape を守る。

`sum_pairwise` は再帰ではなく iterative buffer を使う。
C02 Vec がない場合、一時 array を確保して level ごとに長さを縮める。
allocation を避ける optimized path は、長さが compile-time small なら stack slots を使ってよい。
API 契約は tree shape であり、allocation 数ではない。

`sum_kahan` は left-to-right loop。
`dot`/`dot_fma` は length を先に読み、`xs.length == ys.length` を guard してから loop。
length mismatch は trap。

### LLVM flags

FMA intrinsic call に fast-math flags を付けない。
通常 `fmul`/`fadd` に contract flag を付けない。
Clang invocation は既存どおり fast-math なし。
必要なら emitted IR に `contract` がないことをテストする。

## 実装手順

1. E02/C04/D03 の完了を確認する。
   確認: `Math.sqrt` と `Array.sum` が使える。
2. `Builtin::MathFma` と signature を追加する。
   確認: `Math.fma 1.0f32 2.0f32 3.0f32` の型が f32。
3. f32/f64 native lowering を実装する。
   確認: native IR に `llvm.fma.f32/f64`、通常式には出ない。
4. `tz_soft_fma` を `numeric.c` に追加し `generate.py` で再生成する。
   確認: WASM build で imports 空。
5. f16/f128/decimal と WASM f32/f64 を soft path へ接続する。
   確認: `Math.fma` が native/WASM で同一 bit pattern。
6. `Array.sum_pairwise`, `sum_kahan`, `dot`, `dot_fma` を C04 の Array module に追加する。
   確認: IR/実行結果が固定順序 reference と一致。
7. docs と benchmarks を更新する。
   確認: 暗黙 FMA は未実装/禁止と明記。

## テスト計画

### Rust tests

`tests/fma_reductions.rs`。
受理:
`Math.fma 1.0 2.0 3.0`。
`Math.fma 1.0f32 2.0f32 3.0f32`。
`Array.sum_pairwise (&xs)`。
`Array.dot_fma (&xs) (&ys)`。

拒否:
`Math.fma 1 2 3` は `E1005`。
`Math.fma 1.0 2.0f32 3.0` は `E1003`。
`Array.dot (&xs) (&ys)` で要素型が違えば `E1003`。

IR:
`a * b + c` に `llvm.fma` が出ない。
`Math.fma` だけに `llvm.fma` または `tz_soft_fma`。
fast-math/contract/reassoc flags がない。

### Node E2E

`tests/fma_reductions.mjs`。
native/WASM `-O0`/`-O3`。
WASM imports 空。
fma reference は Python `decimal`/`fractions` で exact product + sum + one rounding。
f16 は exhaustive triplets から代表 subset。
f32/f64 は cancellation cases:
`(1 + 2^-52) * (1 - 2^-52) - 1`。
large * small + cancellation。
signed zero combinations。
NaN/inf invalid combinations。
decimal fma with coefficient/exponent extremes。

Reductions:
small arrays length 0..17 で手書き tree reference。
random arrays length 1..1025。
values include NaN, ±0, ±inf, subnormal, large/small cancellation。
`sum_pairwise` と left-to-right sum が異なる例を入れる。
`sum_kahan` reference は JS/Python で同じ Neumaier steps。
`dot` と `dot_fma` が異なる例を入れる。
length mismatch trap を native child process と WASM RuntimeError で確認。

### Benchmarks

`benchmarks/` に任意で dot/dot_fma/sum_pairwise の matched workload を追加する。
測定する場合:
same input、same order contract、C++ は `std::fma` を明示使用する版とし、通常版は `-ffp-contract=off`。
SIMD/parallel を主張しない。
共有 CI に速度 threshold は置かない。
生成 IR/assembly を保存して FMA 命令の有無を確認する。

## ドキュメント

`docs/language.md` に `Math.fma` と Array reduction API の順序契約を追加する。
`docs/architecture.md` の D-14 関連節に「通常式は contraction しない」「明示 API だけ」を追記する。
`docs/benchmarks.md` は測定を追加した場合だけ更新する。
`README.md` は短い例を追加する。

## 受け入れ条件

- [ ] `Math.fma` が単一丸めである。
- [ ] 通常 `a * b + c` は融合されない。
- [ ] WASM で FMA/libm import がない。
- [ ] f16/f32/f64/f128/decimal の fma が native/WASM で同一。
- [ ] `sum_pairwise` の tree shape が仕様どおり。
- [ ] `sum_kahan` が Neumaier steps どおり。
- [ ] `dot` と `dot_fma` の違いがテストされている。
- [ ] length mismatch は trap。
- [ ] docs に順序契約と非目標が明記されている。

## 落とし穴

`llvm.fmuladd` は contraction 風の意味であり、`llvm.fma` と区別する。
WASM に fma 命令はない。
platform `fma` libcall を呼ぶと import/結果差になる。
pairwise sum を optimizer が任意に再結合してよいわけではない。
Kahan と Neumaier の名前を混同しない。
NaN を含む reduction でも順序を変えると結果が変わりうる。
decimal fma を binary 経由で実装しない。

## 対象外

暗黙 FMA。
fast-math。
自動 reassociation。
parallel reduction runtime。
SIMD API。
GPU dot product。
pairwise dot。
整数 reductions。

## 未決事項

`Array.sum_kahan` という名前で Neumaier を実装するか。
既定案は名前を維持し、docs に Kahan-Babuska-Neumaier と明記する。
decimal FMA の特殊値詳細は IEEE 754 fusedMultiplyAdd に揃える。
ベンチマークをこのチケットで必須にするか。
既定案は正しさテスト必須、性能測定は任意。

台帳の見直し提案: なし。
