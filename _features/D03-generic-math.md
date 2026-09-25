# D03: 数学関数の型汎用化と拡充
| 項目 | 内容 |
|---|---|
| ID | D03 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | E02 |
| 後続 | D05, F04, F05 |
| 状態 | todo |
| 主な影響ファイル | `std/Math.tz`, `src/check.rs`, `src/polymorph.rs`, `src/llvm.rs`, `src/numeric.rs`, `src/runtime/numeric.c`, `src/runtime/generate.py`, `src/runtime/wasm.ll`, `tests/math.rs`, `tests/math.mjs`, `tests/primitives.mjs`, `tests/numeric_casts.mjs`, `docs/language.md`, `docs/architecture.md`, `docs/benchmarks.md`, `README.md` |

## 目的

現行の `sqrt`/`floor`/`ceil`/`abs` は `f64 -> f64` の無修飾 builtin に限られている。
D03 はこれを `Math` モジュールの型汎用 API へ拡張し、f16/f32/f64/f128/decimal を同じ仕様で扱う土台を作る。
既存コードの `sqrt 4.0` は f64 literal defaulting により壊さない。
一方、新しいコードでは `Math.sqrt 4.0f32` のように型を保持して使える。
sin/cos/log/pow などの実用的な数学関数も追加する。
native と WASM で同じ結果を出すため、プラットフォーム libm には依存しない。
fast-math、再結合、暗黙 FMA、double rounding は禁止する。

## 現状

`src/check.rs` の `Builtin` は `Sqrt`, `Floor`, `Ceil`, `Abs` を持つ。
`Builtin::signature` はこれらをすべて `Type::F64 -> Type::F64` として返す。
`docs/language.md` の組み込み関数表も `sqrt`, `floor`, `ceil`, `abs` を `fn(f64) -> f64` と説明している。
`abs` は i64 用ではない。

`src/llvm.rs` の `emit_builtin` は `Sqrt` を `llvm.sqrt.f64`、`Floor` を `llvm.floor.f64`、`Ceil` を `llvm.ceil.f64`、`Abs` を `llvm.fabs.f64` へ下げる。
`intrinsics: BTreeSet<String>` に `declare double @llvm.<name>.f64(double)` を登録する。
`FunctionEmitter::binary` は f32/f64 の `fadd`/`fsub`/`fmul`/`fdiv` を直接出し、f16/f128/decimal は `tz_soft_op` を呼ぶ。
`FunctionEmitter::cast` は f32/f64 と 8〜64-bit integer の直接変換を使い、その他は `tz_soft_cast`。
`numeric_kind` は binary/decimal/integer の runtime kind を決める。
`console_main` は f32/f64 表示に native `printf` を使うが、D01 で formatter 経由へ変える前提。

`src/polymorph.rs` の `Classes::collect` は marker class `Float` を持つ。
`Classes::intrinsic` は `Float` を `ty.is_float()`、つまり `Type::Binary(_) | Type::Decimal(_)` に対して true にする。
型クラス method はない。

`src/runtime/numeric.c` は f16/f128/decimal と i128 を host float なしで扱う。
`tz_soft_op` は add/sub/mul/div だけ。
`tz_soft_cmp` は比較。
`tz_soft_cast` は変換。
`tz_soft_format` は表示。
sqrt や transcendental はない。
`src/runtime/generate.py` は numeric.c から numeric.ll を生成する。

WASM は libc/libm imports なしが原則である。
LLVM intrinsic の一部は WASM 命令へ下がるが、`sin`, `cos`, `pow`, `fma`, `sqrtf128` などは libcall を出す可能性がある。
`src/runtime/wasm.ll` は i128 の乗除算・可変シフト補助だけを持つ。
新しい libcall が出るなら同梱 runtime で提供するか、LLVM が libcall を出さない IR 形にする必要がある。

## 仕様

### 前提とする他チケットのインターフェース

E02 は `std/Math.tz` を同梱し、`Math.sqrt` のような修飾名 builtin を解決できる。
E02 は複数引数 builtin と制約付き builtin signature を提供する。
E02 の builtin interface は `BuiltinType`／`BuiltinScheme` と `FunctionRef::Builtin(BuiltinInstance { builtin, types })` だけを使う。
このチケットで独自の `BuiltinSignature`/`BuiltinSignatureTemplate` や `FunctionRef::Builtin(Builtin, Vec<Type>)` を定義しない。
builtin-backed API 関数（`Math.sqrt`, `Math.sin`, `Math.pi` など）は `Builtin` entry としてだけ存在し、std source に同名 `def` を置かない。
E02 の std 解決順により、ユーザー定義 `Math.tz` は予約名衝突で `E1011`。
B01/Option は D03 の必須依存ではない。
A08/C03/C02 も D03 の必須依存ではない。
ただしテスト補助や将来の vector API は C03/C04/D05 で連携する。

### 他チケットへの提供インターフェース

D05 は `Math.fma` と ordered reductions を追加するとき、D03 の `Float` builtin dispatch と math runtime を再利用する。
F04 は portable SIMD の lane-wise math に D03 の正確な scalar contract を参照する。
F05 は CPU dispatch を導入する場合、D03 の portable path と同じ結果を返す optimized path だけを追加する。
D01 は numeric formatter/parser と math runtime が fast-math を使わないことを前提にしてよい。

### API

既存互換のため、無修飾 `sqrt`, `floor`, `ceil`, `abs` は残す。
ただし新しい推奨 API は `Math.*`。
無修飾 4 関数は引き続き `f64 -> f64` で、内部的には `Math.*` の f64 specialization を呼んでよい。

型汎用関数:

```text
Math.sqrt       :: Float<'a> => 'a -> 'a
Math.floor      :: Float<'a> => 'a -> 'a
Math.ceil       :: Float<'a> => 'a -> 'a
Math.trunc      :: Float<'a> => 'a -> 'a
Math.round      :: Float<'a> => 'a -> 'a
Math.round_even :: Float<'a> => 'a -> 'a
Math.abs        :: Float<'a> => 'a -> 'a
Math.min        :: Float<'a> => 'a -> 'a -> 'a
Math.max        :: Float<'a> => 'a -> 'a -> 'a
Math.clamp      :: Float<'a> => 'a -> 'a -> 'a -> 'a
Math.copysign   :: Float<'a> => 'a -> 'a -> 'a
Math.is_nan     :: Float<'a> => 'a -> bool
Math.is_infinite :: Float<'a> => 'a -> bool
Math.is_finite  :: Float<'a> => 'a -> bool
```

Transcendental（制約は `Float` ではなく組み込みマーカークラス `Elementary`。下の「サポート行列」を参照）:

```text
Math.sin   :: Elementary<'a> => 'a -> 'a
Math.cos   :: Elementary<'a> => 'a -> 'a
Math.tan   :: Elementary<'a> => 'a -> 'a
Math.asin  :: Elementary<'a> => 'a -> 'a
Math.acos  :: Elementary<'a> => 'a -> 'a
Math.atan  :: Elementary<'a> => 'a -> 'a
Math.atan2 :: Elementary<'a> => 'a -> 'a -> 'a
Math.exp   :: Elementary<'a> => 'a -> 'a
Math.exp2  :: Elementary<'a> => 'a -> 'a
Math.log   :: Elementary<'a> => 'a -> 'a
Math.log2  :: Elementary<'a> => 'a -> 'a
Math.log10 :: Elementary<'a> => 'a -> 'a
Math.pow   :: Elementary<'a> => 'a -> 'a -> 'a
Math.cbrt  :: Elementary<'a> => 'a -> 'a
Math.hypot :: Elementary<'a> => 'a -> 'a -> 'a
```

Integer abs:

```text
Math.abs_int :: SignedInteger<'a> => 'a -> 'a
```

`Math.abs` は float 専用とし、既存無修飾 `abs` の f64 挙動と一致する。
整数 abs は D04 の `Int.abs` が本命なので、D03 では `Math.abs_int` を追加しない選択も許す。
既定案: D03 では整数 abs を実装せず、D04 に任せる。

定数:

```text
Math.pi :: Float<'a> => 'a
Math.e  :: Float<'a> => 'a
```

E02 の制約付き 0 引数 builtin が必要である。
Tsuzuri の引数なし関数は `fn() -> T` の値なので、使用時は `Math.pi()`、`Math.e()` と呼ぶ。
bare `Math.pi` は関数値であり、`f64` などの値として使うと型エラーになる。
実装が難しい場合は `Math.pi_f64`, `Math.e_f64` だけをフェーズ 1 にしてはいけない。
型汎用定数がこのチケットの受け入れ条件である。

### 意味

全関数は IEEE 754 の NaN、±inf、±0、subnormal を保持する。
丸めは対象型へ一度だけ最近接・偶数。
ホストの丸めモードや浮動小数点例外フラグは公開しない。
fast-math flag は付けない。
暗黙 FMA contraction は使わない。
`Math.fma` は D05 で別 API として追加する。

`Math.sqrt`:
負の有限値は quiet NaN。
`sqrt(-0)` は `-0` を返す。
`sqrt(+inf)` は `+inf`。
NaN は NaN。
binary f32/f64 は `llvm.sqrt.*` を使ってよい。
f16/f128/decimal は bundled runtime で正確に丸める。

`floor`/`ceil`/`trunc`:
整数値、±0、±inf、NaN はそのまま。
対象型の値として表現できる整数へ丸める。
`trunc` は 0 方向。

`round`:
half away from zero。
`round_even`:
half to even。
どちらも ±0 の符号を保つ。

`abs`:
sign bit を clear する。
NaN payload は可能な限り保持してよいが、payload の保存は公開契約ではない。
NaN は NaN。

`min`/`max`:
IEEE 754-2019 の `minimum`/`maximum` semantics（**NaN 伝播**）。
どちらか一方でも NaN なら NaN を返す。
`min(-0, +0)` は `-0`、`max(-0, +0)` は `+0`。
LLVM では `llvm.minimum.*`/`llvm.maximum.*` を使う。
WASM の `f32.min`/`f64.min` も NaN 伝播かつ `-0 < +0` で同じ意味だが、生成 IR と実行結果（native/WASM × -O0/-O3）で必ず検証する。
一致しない経路が見つかった場合は explicit compare/select + NaN checks で実装する。
NaN を無視して数値を返す `minimumNumber`/`maximumNumber` 系は別名の API が必要になるため、このチケットの対象外とする。

`clamp x lo hi`:
`lo <= hi` でなければ NaN を返す。
`x` が NaN なら NaN。
それ以外は `min(max(x, lo), hi)` の順序で評価する。
lo/hi の評価順序は `x`, `lo`, `hi`。

`copysign magnitude sign`:
magnitude の絶対値に sign の符号 bit を付ける。
NaN でも bit 操作。

`is_nan`:
NaN だけ true。
`is_infinite`:
±inf だけ true。
`is_finite`:
NaN と ±inf 以外 true。

Transcendental:
フェーズ 1 の対象は `Elementary` instance を持つ f32/f64 だけ。
精度契約は f32/f64 で最終結果 ≤ 1 ulp。
正確丸めは要求しない。
ただし native/WASM/optimization level で、非 NaN 結果は同一 bit pattern、NaN 結果は NaN 分類で一致する。
特殊値は IEEE/libm 慣習に揃えるが、仕様に列挙する。
`log(negative finite)` は NaN。
`log(0)` は `-inf`。
`exp(+inf)` は `+inf`、`exp(-inf)` は +0。
`pow(x, y)` は IEEE 754 `pow` の一般的な特殊値規則に従う。
未決になりやすい `pow(-1, inf)`, `pow(1, nan)`, `pow(-0, odd integer)` はテストで固定する。
`hypot(x, y)` は intermediate overflow/underflow を避ける scaling algorithm を使い、`sqrt(x*x + y*y)` へ単純展開しない。

### サポート行列

フェーズ 1 で必須:
f32/f64 の全 API（basic と transcendental）。
f16/f128 の basic API（sqrt/floor/ceil/trunc/round/round_even/abs/min/max/clamp/copysign/is_*、pi/e）。
decimal32/64/128 の basic API。

**決定（GUIDE D-07 の組み込みクラス予約に追加）:**
transcendental は `Float` ではなく、メソッドを持たない組み込みマーカークラス `Elementary` で制約する。
`Classes::collect` の組み込みクラス一覧に `Elementary` を追加し、`Classes::intrinsic` はフェーズ 1 では
`Type::Binary(32 | 64)` だけを true にする。f16/f128/decimal で使うと通常の
`E1005 no instance for Elementary<f16>; ...` になる（実行時フォールバックや近似の代用はしない）。
将来 f16/f128/decimal の transcendental を bundled runtime に実装したら、`intrinsic` の対象型を増やすだけで
API を変えずに対応を広げられる。ユーザーは `Elementary` のインスタンスを定義できない（既存のマーカークラスと同じ扱い）。
`Math.sin_f32` のような型別の名前は作らない。

## 設計

### Builtin

`src/check.rs` の `Builtin` を Math namespaced builtin へ拡張する。
既存 `Sqrt`, `Floor`, `Ceil`, `Abs` は互換 alias として残す。
新規 variants 例:

```rust
pub enum Builtin {
    // existing...
    MathSqrt,
    MathFloor,
    MathCeil,
    MathTrunc,
    MathRound,
    MathRoundEven,
    MathAbs,
    MathMin,
    MathMax,
    MathClamp,
    MathCopysign,
    MathIsNan,
    MathIsInfinite,
    MathIsFinite,
    MathSin,
    MathCos,
    MathTan,
    MathAsin,
    MathAcos,
    MathAtan,
    MathAtan2,
    MathExp,
    MathExp2,
    MathLog,
    MathLog2,
    MathLog10,
    MathPow,
    MathCbrt,
    MathHypot,
    MathPi,
    MathE,
}
```

`Builtin::name` は `"Math.sqrt"` 形式を返す。
E02 の修飾 builtin 解決で `Math.sqrt` が見つかる。
無修飾 `sqrt` は既存 `Builtin::Sqrt` のまま。
`Builtin::scheme` 相当は E02 の `BuiltinScheme` を返す。
一引数 Float 関数は `parameters = [Var("a")]`, `result = Var("a")`, `variables = ["a"]`, `constraints = [Float Var("a")]`。
二引数 Float 関数は `parameters = [Var("a"), Var("a")]`。
`clamp` は三引数。
predicate は `result = bool`。
定数は `parameters = []`, `result = 'a`, `constraints = [Float<'a>]`。
transcendental は `constraints = [Elementary Var("a")]`。

`FunctionRef::Builtin` は E02 の `BuiltinInstance { builtin, types }` だけを使う。
Math builtins は対象 float type を type argument 0 に持つ。
IR symbol は `@tz.builtin.Math.sqrt.f32` のように型を含める。

### Runtime

`numeric.c` に f16/f128/decimal の basic op 用 math entry を追加する。

```c
void tz_soft_math_unary(unsigned char *out, const unsigned char *input, int kind, int op);
void tz_soft_math_binary(unsigned char *out, const unsigned char *a, const unsigned char *b, int kind, int op);
int tz_soft_math_predicate(const unsigned char *input, int kind, int op);
void tz_soft_math_constant(unsigned char *out, int kind, int constant);
```

`tz_soft_math_unary`/`tz_soft_math_binary` は f16/f128/decimal の basic op 用であり、f32/f64 transcendental には使わない。
basic op は f16/f128/decimal もここで扱う。
f32/f64 の basic op は LLVM intrinsic/direct instruction を使う（native と WASM で意味が一致することをテストで確認し、一致しない場合だけ runtime に寄せる）。

transcendental の実装場所（性能と決定性の両立のための決定）:

- **f32/f64:** 新しい `src/runtime/math.c` に、IEEE 754 の `float`/`double` 四則演算・`sqrt` だけを使うアルゴリズム
  （範囲縮小 + minimax 多項式、必要に応じて double-double などの誤差なし変換）で実装する。
  多倍長整数による実装は数十〜数百倍遅くなるため、f32/f64 の通常経路には使わない
  （Payne-Hanek の巨大引数の範囲縮小など、まれな経路だけで使ってよい）。
  IEEE の基本演算は正しく丸められ、縮約・再結合をしなければ native/WASM・最適化レベルに関係なく同じ bit 列になる。
  そのため `math.c` は `generate.py` と同じ方式で `math.ll` に変換し、フラグに
  `-ffp-contract=off -fno-fast-math -fno-builtin -ffreestanding` を必ず含める（C の既定の縮約で `llvm.fmuladd` が出るのを防ぐ）。
  生成 IR に `llvm.fmuladd`、`fast`／`contract`／`reassoc` フラグ、libm 呼び出し、`x86_fp80` が無いことを Rust テストで検査する。
  `long double`、ホストの libm、`errno`、丸めモード変更は使わない。
- **f16/f128/decimal:** 既存の `numeric.c` の多倍長整数による正確な演算で実装する（フェーズ 2。フェーズ 1 では
  `Elementary` のインスタンスが無いので呼べない）。

`math.c` の外部に見える stable symbol は型と関数を含めて分ける。
例: `tz_math_sin_f32`, `tz_math_sin_f64`, `tz_math_pow_f32`, `tz_math_pow_f64`, `tz_math_atan2_f64`。
単一の `tz_math_unary(kind, op)` のような dispatch 関数にはしない。
`emit_builtin` は f32/f64 transcendental でこれらの stable symbol を直接呼ぶ。
`emit_target` には `output.contains("@tz_math_")` のような明示的 link predicate を追加し、必要な場合だけ `runtime/math.ll` を連結する。
parse-only/numeric-only プログラムでは `math.ll` を連結しない。
新しい `math.ll` は `.gitignore` の例外（`!src/runtime/math.ll`）に追加して追跡する（GUIDE 6.6、G01）。
runtime 内で host `long double` と libm は使わない。

`generate.py` の計画:
`numeric.c` と `math.c` の 2 つの Clang 出力を単純に文字列連結してはいけない。
LLVM IR の metadata `!N` と `attributes #N` が衝突するためである。
既定案は generator が 2 translation unit を生成した後、`math.ll` 側の metadata id と attribute group id を `numeric.ll` の最大値より後ろへ deterministic に remap してから保存する。
代替案として、`numeric.c` と `math.c` を 1 つの combined C translation unit として Clang に渡し、単一の generated IR を作る。
どちらの方式でも `numeric.ll`/`math.ll` の出力順、attribute番号、metadata番号が決定的で、2 回生成して byte-for-byte 一致することをテストする。
`math.ll` を別ファイルで include する場合、`emit_target` の concatenation order は `numeric.ll` → `math.ll` の固定順にする。

アルゴリズム指針:
sqrt は digit-by-digit integer square root または Newton iteration with directed interval を使い、最後に一度だけ pack する。
floor/ceil/trunc/round は decode した係数と指数に対して整数除算で行う。
min/max/copy sign/is_* は bit/decode 操作。
sin/cos/tan は Payne-Hanek range reduction と minimax polynomial。
asin/acos/atan は argument reduction と minimax/rational approximation。
exp/log は range reduction、table-free または generated deterministic small tables、polynomial/rational approximation。
pow は `exp(y * log(x))` だけでは特殊値と整数指数負底の扱いが不足するため、special cases を先に処理する。
cbrt は scaling + Newton。
hypot は max scaling と sqrt。
係数表を使う場合は `generate.py` か別 generator で生成し、生成条件を docs に書く。
NaN は payload/sign を cross-target bit-exact に要求しない。
実装は canonical quiet NaN を返す方針を推奨するが、テストは NaN 分類で比較する。
非 NaN 結果だけ bit pattern の完全一致を要求する。

### LLVM lowering

f32/f64 basic:
`sqrt` は `llvm.sqrt.f32/f64`。
`floor` は `llvm.floor.*`。
`ceil` は `llvm.ceil.*`。
`trunc` は `llvm.trunc.*` intrinsic。
`round` は `llvm.round.*` を使わない。wasm32 `-O0`/`-O3` で `roundf`/`round` libcall が未解決になることが確認済みのためである。
half-away-from-zero は `trunc`、差分の絶対値比較、符号に応じた `+1`/`-1`、select で明示実装する。
`round_even` も丸め環境依存の `llvm.rint.*`/`llvm.nearbyint.*` を使わない。
`llvm.roundeven.*` を使う場合は wasm32 object/link で未解決 libcall が出ないことを確認した LLVM version に限定し、未確認なら explicit algorithm または runtime にする。
`abs` は `llvm.fabs.*`。
`minimum`/`maximum` は `llvm.minimum.*`/`llvm.maximum.*`。
`copysign` は bit operation（sign mask の clear/or）で実装するのを既定案にする。
`llvm.copysign.*` を使う場合は wasm32 object/link で未解決 libcall がないことを確認した場合だけ。
`floor`/`ceil`/`trunc`/`minimum`/`maximum`/`sqrt` も IR inspection だけでなく wasm32 object/link と imports 空で検証し、未解決 libcall が出る場合は explicit/runtime lowering に切り替える。

f16/f128/decimal:
`alloca` slot に値を store し、`tz_soft_math_*` を呼び、load する。
`numeric_kind` を再利用する。

Transcendental:
f32/f64 は `math.ll` の stable symbols へ runtime call にする。
LLVM `llvm.sin.*`/`llvm.cos.*` は使わない。
理由: native で libm call、WASM で missing import/libcall になる可能性が高く、結果も platform libm 依存になるため。
f16/f128/decimal は `Elementary` instance がないためコンパイル時に `E1005`。

### 既存無修飾 builtin 互換

`sqrt`, `floor`, `ceil`, `abs` は `f64 -> f64` のまま。
`emit_builtin(Builtin::Sqrt)` は `Math.sqrt.f64` と同じ IR を生成する。
同じ intrinsic/runtime path を使い、性能経路が分岐しない。
docs では旧名を互換として残し、新規コードは `Math.sqrt` を推奨する。

### 診断

型が Float でない場合は既存の `E1005 no instance for Float ...`。
`Math.sin 1` は受理しない。
整数 literal は `Integer` 制約を持つため、`Elementary` への暗黙変換はなく `E1005` または型不一致 `E1003` になる。
受理例は `Math.sin 1.0`。
`let n = 1; Math.sin n` は `n` が i64 に決まっているため `E1005`。
未実装型を runtime trap にしてはいけない。
公開した関数が対応できない型ならコンパイル時に拒否する。

## 実装手順

1. E02 の namespaced builtin と constrained signature を確認する。
   確認: `Math.sqrt` が std なしでも builtin として解決できる fixture を作る。
2. `Builtin` に Math variants を追加する。
   確認: `Builtin::ALL` の重複・順序が決定的である。
3. `Builtin::signature` を Float generic にする。
   確認: `Math.sqrt 4.0f32` の型が f32、`Math.sqrt 4.0` が f64。
4. 旧 `sqrt`/`floor`/`ceil`/`abs` を f64 alias として残す。
   確認: 既存 tests が型変更なしで通る。
5. f32/f64 basic lowering を実装する。
   確認: IR に `llvm.sqrt.f32`, `llvm.sqrt.f64` などが重複なく宣言される。さらに wasm32 object/link を `-O0`/`-O3` で行い、floor/ceil/trunc/minimum/maximum/sqrt/copysign/round 系が未解決 libcall を出さないことを確認する。
6. f16/f128/decimal basic runtime entry を `numeric.c` に追加する。
   確認: `python3 src/runtime/generate.py` 後、IR に未解決 libcall がない。
7. predicates/is_* と constants を実装する。
   確認: NaN/inf/subnormal/signed zero を全型で照合する。
8. min/max/clamp/copysign を実装する。
   確認: `min(-0,+0) == -0`, `max(-0,+0) == +0`, NaN cases を native/WASM で照合する。
9. transcendental runtime を実装する（f32/f64 のみ。`src/runtime/math.c` → `math.ll` を生成し、`Elementary` マーカークラスを `Classes::collect`／`Classes::intrinsic` に追加する）。
   確認: f32/f64 の代表点を高精度参照と ≤1 ulp で照合する。f16/f128/decimal の transcendental 使用は `E1005` で拒否されることを確認する。
10. `tests/math.rs` を追加する。
    確認: 受理・拒否・IR invariants。
11. `tests/math.mjs` を追加する。
    確認: native/WASM `-O0`/`-O3`、imports 空、reference values。
12. docs/README を更新する。
    確認: 旧無修飾 builtin と新 Math API の関係が明記されている。

## テスト計画

### Rust tests

受理:
`def f :: f32 -> f32; fn f x = Math.sqrt x`。
`def f :: f128 -> f128; fn f x = Math.floor x`。
`def f :: d128 -> d128; fn f x = Math.ceil x`。
`def f :: f64; fn f = Math.pi()`。
`def f :: bool; fn f = Math.is_nan (0.0 / 0.0)`。
`def f :: f64; fn f = sqrt 4.0` 互換。
`def f :: f32; fn f = Math.min (-0.0f32) 0.0f32`。

拒否:
`def f :: i64; fn f = Math.sqrt 4i64` は `E1005`。
`def f :: bool; fn f = Math.sin true` は `E1005`。
`def f :: f64; fn f = Math.sin 1` は `E1005` または `E1003`。
`def f :: f16; fn f = Math.sin 1.0f16` は `E1005`。
`def f :: f128; fn f = Math.log 1.0f128` は `E1005`。
`def f :: d128; fn f = Math.exp 1.0d128` は `E1005`。
`fn Math() -> i64 { 1 }` ではなく、ユーザー `Math.tz` は E02 の module collision `E1011`。
`fn sqrt() -> i64 { 1 }` は既存 builtin 名衝突で `E1001`。

IR:
`Math.sqrt 4.0f32` は f32 intrinsic。
`Math.sqrt 4.0f128` は `@tz_soft_math_unary`。
`Math.sin 1.0` は `llvm.sin` も `@sin` も含まず、f64 なら `@tz_math_sin_f64` を呼ぶ。
intrinsic 宣言の重複なし。
WASM target IR に `@pow`, `@log`, `@sqrtf128`, `@__trunctf` 等の未提供 libcall がない。
`@tz_math_` を含む IR だけ `math.ll` が連結され、basic-only program では連結されない。

### Node E2E

`tests/math.mjs`。
native `-O0`/`-O3`。
WASM `-O0`/`-O3`。
WASM imports 空。
IR determinism。
native heap tracking は math 自体に allocation がないことを確認する。

basic cases:
全型で `sqrt(0)`, `sqrt(-0)`, `sqrt(4)`, `sqrt(+inf)`, `sqrt(-1)`, NaN。
floor/ceil/trunc/round/round_even の正負 half cases。
abs NaN/negative/signed zero。
min/max NaN/signed zero。
clamp normal/NaN/lo>hi。
copysign normal/zero/NaN。
is_*。
pi/e の known rounded bit pattern。
pi/e は `Math.pi()`/`Math.e()` として呼び、bare `Math.pi` が値としては型エラーになることを Rust test で確認する。

transcendental cases:
sin/cos/tan 0、±0、π/6、π/2、large argument。
asin/acos domain endpoints and out of domain。
atan/atan2 quadrants and signed zero。
exp/log inverse cases、overflow boundary、underflow boundary。
pow integer exponent、fractional exponent、negative base special cases。
cbrt negative。
hypot scaling extreme values。
NaN 結果は bit pattern ではなく `Number.isNaN`/分類で比較する。
非 NaN 結果だけ native/WASM `-O0`/`-O3` で bit pattern 一致を要求する。

Reference:
Python `mpmath` を使えるなら 200 bits 以上で期待値。
mpmath 不可なら Python `decimal` + `fractions` + Taylor/minimax independent implementation をテスト内に持つ。
transcendental はフェーズ 1 では f32/f64 だけを検査する（f32 は代表境界 + pseudo-random 10,000 cases、f64 は同 10,000 cases）。
basic API は f16 を exhaustive、f128/decimal を 1,000 representative cases で検査する。
`Math.sin 1.0f16` などが `E1005`（`Elementary` のインスタンスなし）になることを Rust テストで確認する。
f16/f128/decimal や i128 など公開 ABI で返せない値は、Tsuzuri 内部で bool/i64 checksum または 64-bit limb 分割を返す test helper、または runtime entry point を C/LLVM harness から raw bytes で直接呼ぶ test-only harness で検査し、公開 ABI は広げない。
期待値は Tsuzuri 出力から生成しない。

### 性能確認

共有 CI に速度 threshold は置かない。
`docs/benchmarks.md` に任意で `Math.sin`/`sqrt` micro benchmark の測定手順を追加する。
測定する場合は C/libm との比較結果を「同じ結果を保証する比較ではない」と明記する。
生成 IR/assembly を確認し、f32/f64 basic が libcall ではなく intrinsic/命令になっていることを書く。
transcendental は portable runtime である。f32/f64 は IEEE 演算によるアルゴリズムなので platform libm と同程度を目標にするが、実測するまで速度を主張しない。

### 実装完了時の検証コマンド

GUIDE §3 に従い、小さい範囲から次を実行する。

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --locked --test math
cargo test --locked
cargo build --release --locked
node tests/math.mjs target/release/tsuzuri
node tests/primitives.mjs target/release/tsuzuri
node tests/numeric_casts.mjs target/release/tsuzuri
```

`tests/math.mjs` は native/WASM × `-O0`/`-O3`、WASM imports 空、wasm32 object/link の未解決 libcall 不在、`math.ll`/`numeric.ll` の metadata/attribute 衝突不在、非 NaN bit 一致と NaN 分類比較を必ず含める。

## ドキュメント

`docs/language.md` の組み込み関数表を更新する。
旧 `sqrt`/`floor`/`ceil`/`abs` は互換 f64 builtin と明記。
新しい `Math` モジュール表を追加する。
精度契約（basic は exact/correctly rounded、transcendental は ≤1 ulp）を書く。
NaN/signed zero/minimum/maximum semantics を書く。
`docs/architecture.md` に platform libm 非依存、WASM import なし、`math.c`/`math.ll` の生成・link predicate・metadata/attribute remap 方針を書く。
`README.md` の例で `sqrt` を `Math.sqrt` へ移行するか、互換説明を加える。
`docs/benchmarks.md` は性能測定を行った場合だけ更新する。

## 受け入れ条件

- [ ] `Math.*` API が仕様の型で使える。
- [ ] 旧 f64 builtin が壊れない。
- [ ] f32/f64 basic は typed LLVM intrinsic/direct lowering。
- [ ] f16/f128/decimal basic は bundled runtime で正確。
- [ ] transcendental は platform libm に依存しない。f32/f64 は `math.c`（縮約なしの IEEE 演算）で実装し、`math.ll` に `llvm.fmuladd`・fast-math フラグ・libm 呼び出しが無い。
- [ ] `Elementary` は f32/f64 だけを受理し、f16/f128/decimal は `E1005` で拒否される。
- [ ] native/WASM `-O0`/`-O3` で非 NaN 結果は同一 bit pattern、NaN は分類で一致する。
- [ ] WASM imports は空。
- [ ] wasm32 object/link 検証で round/floor/ceil/trunc/copysign/minimum/maximum/sqrt/transcendental が未解決 libcall を出さない。
- [ ] `math.ll` と `numeric.ll` の metadata/attribute ID が衝突しない。
- [ ] fast-math、reassociation、implicit FMA がない。
- [ ] NaN、signed zero、subnormal、infinity のテストがある。
- [ ] docs に精度と未対応範囲が正直に書かれている。

## 落とし穴

LLVM `llvm.sin.*` は portable な結果保証ではない。
WASM に `sin` 命令はない。
`llvm.sqrt.f64` は WASM f64.sqrt へ下がるが、f128 sqrt は libcall になりうる。
`llvm.minimum`/`maximum` と WASM min/max の NaN semantics を混同しない。
`round` と `round_even` を取り違えない。
`llvm.round.*` は WASM で `roundf`/`round` libcall になりうるため使わない。
`pow` の負 base と signed zero は特殊値地獄なので参照テストを先に作る。
decimal を binary に変換して計算すると仕様違反。
f16 を f32 で計算して最後に f16 へ丸めるだけでは ≤1 ulp は満たせても「同一結果」検証が必要であり、double rounding に注意する。
定数 pi/e は型ごとに一度だけ丸める。

## 対象外

Complex number。
vectorized Math API。
random number。
statistical functions。
BigFloat。
変更可能な丸めモード。
浮動小数点例外フラグ。
暗黙 FMA。
性能最適化された platform-specific libm dispatch。

## 未決事項

transcendental の対応型は組み込みマーカークラス `Elementary` で表す（決定済み。「サポート行列」参照）。
フェーズ 1 は f32/f64 だけ。f16/f128/decimal の transcendental は後続チケットで `numeric.c` に実装し、`Elementary` の対象型を増やす。
`Math.abs_int` は D04 の `Int.abs` と重複する。
既定案は D03 では実装しない。
`round` の NaN payload 保存は未決。
既定案は payload 非保証。
pi/e の値表生成方法は未決。
既定案は Python `decimal` で十分な桁を生成し、`numeric.rs`/runtime pack で対象型に一度だけ丸める。

台帳への反映: 組み込みマーカークラス `Elementary` を GUIDE D-07 の組み込みクラス予約に追加済み。
