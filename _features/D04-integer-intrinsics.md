# D04: 整数 intrinsic（min/max/popcount/rotate/checked など）
| 項目 | 内容 |
|---|---|
| ID | D04 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | E02, B01 |
| 後続 | C04, F04, F05 |
| 状態 | todo |
| 主な影響ファイル | `std/Int.tz`, `src/check.rs`, `src/polymorph.rs`, `src/llvm.rs`, `src/runtime/wasm.ll`, `tests/integers.rs`, `tests/integer_intrinsics.mjs`, `tests/primitives.mjs`, `tests/numeric_casts.mjs`, `docs/language.md`, `docs/architecture.md`, `README.md` |

## 目的

整数の境界処理、bit 操作、overflow 検出、飽和・wrapping 演算を標準 API として提供する。
現行言語は整数 `+ - *` が wrapping、`/ %` がゼロ除算と MIN/-1 をトラップ、shift が count mask という低水準仕様を持つ。
D04 はその仕様を崩さず、意図が明確な `Int.checked_add`、`Int.saturating_add`、`Int.count_ones` などを追加する。
LLVM の型付き intrinsic を使い、演算子と builtin が別の性能経路にならないようにする。
WASM では compiler-rt/libgcc の未同梱 libcall を出さない。
境界テストは JS BigInt の独立参照で全幅・全符号を照合する。

## 現状

`src/check.rs` の `Type::Integer(bits, signed)` は 8/16/32/64/128 bit と signed flag を表す。
`Type::is_integer` は `Type::Integer(..)`。
`Classes::collect` は marker class `Integer` と `SignedInteger`、演算 class `Bits` を持つ。
`Classes::intrinsic` は `Integer`/`Bits` を全整数、`SignedInteger` を signed integer に対して true にする。

`src/llvm.rs` の `FunctionEmitter::binary` は integer add/sub/mul を LLVM `add`/`sub`/`mul` へ下げ、`nsw`/`nuw` を付けない。
除算/剰余は RHS != 0 と signed MIN/-1 を `guard` で検査してから `sdiv`/`udiv`/`srem`/`urem`。
shift は `rhs = rhs & (bits - 1)` で mask してから `shl`/`ashr`/`lshr`。
`ShiftRightUnsigned` は signed でも logical right shift。
比較は signed flag に応じて signed/unsigned predicate を選ぶ。

`src/runtime/wasm.ll` は LLVM が導入しうる i128 乗除算・剰余・可変シフト補助を提供する。
`__multi3` と `tz.runtime.shift` は `noinline optnone` で InstCombine の再 wide 化を防ぐ。
`__udivti3`, `__umodti3`, `__udivmodti4`, `__divti3`, `__modti3`, `__ashlti3`, `__lshrti3`, `__ashrti3` がある。
checked i128 multiply intrinsic は wasm32 で `__muloti4` 等の未同梱 libcall を出すことが確認済みである。

`src/polymorph.rs` の `Specializer::intrinsic_function` は演算 class method を通常関数化する。
`Bits` method は `bit_and`, `bit_or`, `bit_xor`, `shl`, `shr`, `ushr`, `bit_not`。
既存 shift method も演算子と同じ mask semantics になる。

`tests/primitives.mjs` は wide shift と i128 乗除算の境界を一部検査する。
`tests/numeric_casts.mjs` は整数/float 変換の境界を検査するが、checked/saturating/rotate/popcount はない。

## 仕様

### 前提とする他チケットのインターフェース

E02 は `std/Int.tz` を同梱し、`Int.count_ones` のような修飾名 builtin を解決できる。
E02 は複数引数 builtin と制約付き builtin signature を提供する。
E02 の builtin interface は `BuiltinType`／`BuiltinScheme` と `FunctionRef::Builtin(BuiltinInstance { builtin, types })` だけを使う。
このチケットで独自の `BuiltinSignatureTemplate` や `FunctionRef::Builtin(Builtin, Vec<Type>)` を定義しない。
builtin-backed API 関数（`Int.count_ones`, `Int.checked_add` など）は `Builtin` entry としてだけ存在し、std source に同名 `def` を置かない。
B01 は `Option 'a` を提供する。
B01 は hard dependency である。
`checked_*` と `checked_pow` がこのチケットの中核なので、B01 が未完了ならこのチケットには着手しない。
A08/C03/C02 は D04 の必須依存ではない。

### 他チケットへの提供インターフェース

C04 の array bulk API は整数 reduction に `Int.saturating_add` や `Int.wrapping_pow` を関数値として渡せる。
F04 の SIMD API は lane-wise popcount/rotate/saturating を D04 と同じ意味にする。
F05 の CPU dispatch は D04 の portable path と bit exact な optimized path だけを追加する。
D05 の ordered reductions は整数について結合的な wrapping 演算を再結合してよい根拠として D04 の wrapping semantics を参照する。

### 型と戻り値

基本制約:
`Integer 'a` は全整数型。
`SignedInteger 'a` は signed integer 型。
`UnsignedInteger` class は現状ないため、このチケットで追加する。

```text
class UnsignedInteger 'a
```

`Classes::collect` に method なし marker として追加し、`Classes::intrinsic` は `Type::Integer(_, false)` に true。
ユーザー instance は禁止する。

戻り値:
bit count/zero count は `i64` を返す。
理由は `.length` と同じ汎用 count 型であり、width に関係なく扱いやすい。
rotate amount は `i64`。
checked/saturating/wrapping の arithmetic result は入力と同じ型。
`unsigned_abs` と `abs_diff` は同じ bit width の unsigned 型を返す。
`widening_mul` は同じ signedness の 2 倍幅を返す。ただし i128/i128u は 2 倍幅がないため対象外。

E02 の `BuiltinType` には `UnsignedOf` と `WidenOf` があるため、それを使う。
正確な前提 interface:

```rust
pub enum BuiltinType {
    Var(&'static str),
    Concrete(Type),
    Std { module: &'static str, name: &'static str, args: Vec<BuiltinType> },
    Array(Box<BuiltinType>),
    List(Box<BuiltinType>),
    Tuple(Vec<BuiltinType>),
    Reference(Box<BuiltinType>, bool),
    Function(Vec<BuiltinType>, Box<BuiltinType>),
    UnsignedOf(Box<BuiltinType>),
    WidenOf(Box<BuiltinType>),
}

pub struct BuiltinScheme {
    pub parameters: Vec<BuiltinType>,
    pub result: BuiltinType,
    pub variables: Vec<&'static str>,
    pub constraints: Vec<BuiltinConstraint>,
}
```

`UnsignedOf(Box::new(Var("a")))` は concrete call site で `Type::Integer(bits, false)` に解決する。
`WidenOf(Box::new(Var("a")))` は concrete call site で bits 8→16、16→32、32→64、64→128 に解決し、128 は不適用で `E1005`。
E02 phase 1 では `UnsignedOf`/`WidenOf` は引数が呼び出し箇所で concrete の場合だけ解決する。
型変数のまま残る generic code では使えず `E1015`。
したがって `def f :: SignedInteger 'a => 'a -> ...; fn f x = Int.unsigned_abs x` のような汎用 wrapper は phase 1 では拒否される。

### API

比較/選択:

```text
Int.min   :: Integer 'a => 'a -> 'a -> 'a
Int.max   :: Integer 'a => 'a -> 'a -> 'a
Int.clamp :: Integer 'a => 'a -> 'a -> 'a -> 'a
```

signed は signed order、unsigned は unsigned order。
`clamp x lo hi` は `lo > hi` の場合トラップ。
評価順序は `x`, `lo`, `hi`。

abs 系:

```text
Int.abs          :: SignedInteger 'a => 'a -> 'a
Int.unsigned_abs :: SignedInteger 'a => 'a -> unsigned_of 'a
Int.abs_diff     :: Integer 'a => 'a -> 'a -> unsigned_of 'a
```

`Int.abs MIN` は wrapping で MIN を返す。
`unsigned_abs MIN` は `2^(bits-1)` を unsigned で返す。
`abs_diff a b` は数学的な絶対差を unsigned same width で返す。
signed の場合も overflow せず unsigned へ計算する。

bit count:

```text
Int.count_ones     :: Integer 'a => 'a -> i64
Int.leading_zeros  :: Integer 'a => 'a -> i64
Int.trailing_zeros :: Integer 'a => 'a -> i64
```

zero input の `leading_zeros`/`trailing_zeros` は bit width を返す。
LLVM `ctlz`/`cttz` は `is_zero_undef = false`。

rotate/byte/bit:

```text
Int.rotate_left  :: Integer 'a => 'a -> i64 -> 'a
Int.rotate_right :: Integer 'a => 'a -> i64 -> 'a
Int.swap_bytes   :: Integer 'a => 'a -> 'a
Int.reverse_bits :: Integer 'a => 'a -> 'a
Int.is_power_of_two :: UnsignedInteger 'a => 'a -> bool
```

rotate amount は `amount & (bits - 1)`。
負 amount も two's complement の下位 bit で mask される。
これは既存 shift の mask semantics と一致する。
`swap_bytes` は 8-bit では入力そのもの。
`reverse_bits` は bit 全体を反転順にする。
`is_power_of_two 0` は false。
signed 型の `is_power_of_two` は提供しない。

checked arithmetic:

```text
Int.checked_add :: Integer 'a => 'a -> 'a -> Option 'a
Int.checked_sub :: Integer 'a => 'a -> 'a -> Option 'a
Int.checked_mul :: Integer 'a => 'a -> 'a -> Option 'a
Int.checked_div :: Integer 'a => 'a -> 'a -> Option 'a
Int.checked_rem :: Integer 'a => 'a -> 'a -> Option 'a
Int.checked_neg :: SignedInteger 'a => 'a -> Option 'a
```

add/sub/mul overflow は `None`。
div/rem は zero divisor と signed MIN/-1 を `None`。
unsigned div/rem zero divisor は `None`。
checked_neg は MIN を `None`。

saturating arithmetic:

```text
Int.saturating_add :: Integer 'a => 'a -> 'a -> 'a
Int.saturating_sub :: Integer 'a => 'a -> 'a -> 'a
Int.saturating_mul :: Integer 'a => 'a -> 'a -> 'a
```

signed は min/max に saturate。
unsigned は 0/max に saturate。
LLVM に直接 intrinsic がない型/演算は overflow intrinsic + select で実装する。

pow:

```text
Int.wrapping_pow :: Integer 'a => 'a -> i64 -> 'a
Int.checked_pow  :: Integer 'a => 'a -> i64 -> Option 'a
```

exponent が負なら `wrapping_pow` はトラップ、`checked_pow` は `None`。
exponent 0 は 1（対象型へ変換した 1）。
binary exponentiation を使う。
wrapping_pow は各 multiply が wrapping。
checked_pow は各 multiply の overflow を検出し、overflow なら `None`。

widening:

```text
Int.widening_mul :: Integer 'a => 'a -> 'a -> (widen_of 'a)
```

8/16/32/64-bit だけ。
signed は signed 2 倍幅、unsigned は unsigned 2 倍幅。
i128/i128u は `E1005`。
戻り値を tuple `(low, high)` にする案もあるが、このチケットでは 2 倍幅 scalar を既定案にする。
既存に i256 がないため i128 は対象外。

### LLVM 対応

`min/max`:
LLVM `llvm.smin.*`, `llvm.smax.*`, `llvm.umin.*`, `llvm.umax.*` を使う。
intrinsic 宣言は `declare iN @llvm.smin.iN(iN, iN)` 形式。
LLVM version に intrinsic がない場合は `icmp` + `select`。

`count_ones`:
`llvm.ctpop.iN`。
戻り iN を `zext/trunc` して i64。
N <= 64 は zext。
N = 128 は `trunc` して i64 で十分（結果最大 128）。

`leading_zeros`:
`llvm.ctlz.iN(iN, i1 false)`。
戻り iN を i64 へ変換。

`trailing_zeros`:
`llvm.cttz.iN(iN, i1 false)`。

`rotate_left/right`:
`llvm.fshl.iN` / `llvm.fshr.iN`。
amount は iN へ変換してから intrinsic に渡すか、LLVM signature に合わせる。
既存 shift と同じ mask semantics を保証するため、明示的に `amount & (bits - 1)` を行う。

`swap_bytes`:
`llvm.bswap.iN`。
8-bit はそのまま返す。

`reverse_bits`:
`llvm.bitreverse.iN`。

checked add/sub/mul:
`llvm.sadd.with.overflow.iN`, `llvm.uadd.with.overflow.iN`, `llvm.ssub.with.overflow.iN`, `llvm.usub.with.overflow.iN`, `llvm.smul.with.overflow.iN`, `llvm.umul.with.overflow.iN`。
戻り `{ iN, i1 }` から overflow flag を取り出す。
`Option` は B01/A02 の union lowering で構築する。

saturating add/sub:
`llvm.sadd.sat.iN`, `llvm.uadd.sat.iN`, `llvm.ssub.sat.iN`, `llvm.usub.sat.iN`。
saturating mul:
LLVM に汎用 `smul.sat` がなければ `smul.with.overflow` + sign/max/min select。
unsigned は `umul.with.overflow` + max select。

widening_mul:
N < 128 の場合、operands を 2N に sign/zero extend して `mul`。
result type は 2N。
overflow flag は不要。

checked div/rem:
既存 `binary` の guard と同じ条件を `Option.None` にする。
trap しない。

### WASM libcall 対応

LLVM は i128 overflow multiply や bswap/bitreverse/ctpop を inline できる場合がある。
しかし `llvm.smul.with.overflow.i128` は wasm32 `-O0` と `-O3` の両方で unresolved `__muloti4` を出すことが確認済みである。
実装時は次を必ず確認する。
`--target wasm32 -O0/-O3 --emit llvm` だけではなく、wasm object/link 後に missing symbol がないこと。
既定案は `src/runtime/wasm.ll` に weak hidden `__muloti4(i128 %a, i128 %b, ptr %overflow)` を追加する。
戻り値は低 128 bit の積、`%overflow` が null でなければ signed overflow flag を `i32` または target ABI が要求する幅で store する（実際に LLVM が出す prototype を wasm link failure/IR で確認して合わせる）。
アルゴリズムは 32-bit limb の unsigned 256-bit product を手書きし、上位 128 bit が signed product の sign extension と一致するかで overflow を判定する。
helper 自身が再び wide multiply/overflow libcall に戻らないよう、`__multi3` と同じく `noinline optnone` を付け、内部では i128 `mul` や `llvm.smul.with.overflow.i128` を使わない。
代替案として、`checked_mul`/`saturating_mul` の i128 signed lowering を LLVM overflow intrinsic ではなく compiler-generated limb algorithm にする。
どちらか一方を必ず実装し、未解決 `__muloti4` を許容しない。
unsigned overflow は `umul.with.overflow.i128` が helper を出す場合、同様に limb-based lowering へ切り替える。
Clang/LLVM version により helper 名が違う可能性があるため、テストは IR と wasm link の両方を見る。

## 設計

### Builtin variants

`Builtin` に `IntMin`, `IntMax`, ... を追加する。
`Builtin::name` は `"Int.min"` 形式。
E02 の修飾 builtin 解決で std の `Int` module 関数より後に builtin を探す。
std `Int.tz` は builtin-backed API と同名の `def` を置かない。
Tsuzuri source で実装する helper だけを置く。

`FunctionRef::Builtin` は E02 の `BuiltinInstance { builtin, types }` だけを使う。
戻り型が `UnsignedOf`/`WidenOf` の builtin は concrete type arg から `emit_builtin` が symbol を作る。
IR symbol 例:
`@tz.builtin.Int.count_ones.i64`。
`@tz.builtin.Int.unsigned_abs.i32`。
`@tz.builtin.Int.widening_mul.i64u`。

### Option 構築

checked 系は B01 の `Option` union layout を使う。
`Some result` と `None` の IR 構築 helper を E02/B01 から共有する。
helper がない場合、D04 で `FunctionEmitter` に `option_some(ty, value)`/`option_none(ty)` を追加し、B01 の layout 変更に追随する。
Option を `bool + value` の独自 record として再実装してはいけない。

### 評価順序と所有権

整数は Copy なので move 問題はない。
それでも関数引数の評価順序は通常どおり左から右。
`Int.clamp x lo hi` は x、lo、hi の順に式を評価し、その後 lo > hi を検査する。
`checked_div a b` は a、b を評価してから zero/minus-one 判定し、`None` を返す。
trap と Option の違いは API 名で明確にする。

### 診断

型制約不一致は既存 `E1005`。
`Int.widening_mul 1i128 2i128` は「no instance / unsupported width」の `E1005`。
`Int.is_power_of_two (-1i64)` は signed 型なので `E1005`。
`checked_*` を B01 未完了で実装しようとする場合は着手しない。
新しい診断コードは追加しない。

## 実装手順

1. E02 と B01 の状態を確認する。
   確認: B01 が未完了ならこのチケットを開始しない。
2. `UnsignedInteger` marker class を `Classes::collect` と `Classes::intrinsic` に追加する。
   確認: `UnsignedInteger i64u` は通り、`UnsignedInteger i64` は `E1005`。
3. E02 の `BuiltinScheme`/`BuiltinType::UnsignedOf`/`WidenOf` を使って型を実装する。
   確認: `Int.unsigned_abs (-1i8)` の型が `i8u`、`Int.widening_mul 1i32 2i32` の型が `i64`。generic wrapper で `UnsignedOf`/`WidenOf` が未解決なら `E1015`。
4. `Builtin` variants と names を追加する。
   確認: `Int.count_ones` が解決し、ユーザー `fn Int.count_ones` のような不正名は既存 parser で拒否される。
5. bit count/rotate/bswap/bitreverse lowering を実装する。
   確認: IR intrinsic 宣言が BTreeSet で重複しない。
6. min/max/clamp/abs/unsigned_abs/abs_diff を実装する。
   確認: signed/unsigned order と MIN cases。
7. checked add/sub/mul/div/rem/neg を実装する。
   確認: overflow/zero/minus-one で `None`、正常で `Some`。
8. saturating add/sub/mul を実装する。
   確認: min/max saturation。
9. wrapping_pow/checked_pow を binary exponentiation で実装する。
   確認: negative exponent の trap/None。
10. widening_mul を実装する。
    確認: i64*i64 -> i128 の signed/unsigned 参照。
11. WASM link を確認し、`__muloti4` 対策を実装する。
    確認: i128 signed checked/saturating multiply を含む wasm32 `-O0`/`-O3` build/link で `__muloti4` undefined がなく、imports 空。helper に `noinline optnone` があり、helper 内に i128 multiply overflow intrinsic がない。
12. tests/docs を追加する。
    確認: native/WASM `-O0`/`-O3` と BigInt reference。

## テスト計画

### Rust tests

`tests/integers.rs` を追加する。
受理:
`Int.count_ones 0xffi8u` は i64。
`Int.leading_zeros 0i128` は i64。
`Int.rotate_left 1i32 33` は i32。
`Int.swap_bytes 0x1234i16u` は i16u。
`Int.reverse_bits 1i8u` は i8u。
`Int.checked_add 1i64 2i64` は `Option i64`。
`Int.saturating_mul 100i8 100i8` は i8。
`Int.unsigned_abs (-128i8)` は i8u。
`Int.abs_diff (-128i8) 127i8` は i8u。
`Int.widening_mul 2i64 3i64` は i128。

拒否:
`Int.is_power_of_two 1i64` は `E1005`。
`Int.unsigned_abs 1i64u` は `E1005`。
`Int.widening_mul 1i128 2i128` は `E1005`。
`Int.count_ones 1.0` は `E1005`。
`def f :: Integer 'a => 'a -> i64; fn f x = Int.count_ones x` は受理される。
一方、戻り型に「同じ幅の unsigned」を必要とする `Int.unsigned_abs x` の汎用 wrapper は、`UnsignedOf` が concrete でないため `E1015`。
`instance UnsignedInteger i64 {}` は marker/builtin class override の `E1016`。

IR:
ctpop/ctlz/cttz/fshl/fshr/bswap/bitreverse/sadd.with.overflow を含む。
integer add operator は `add` のまま変わらない。
checked_add は trap を含まない。
checked_div は `llvm.trap` を含まない。
normal `/` は引き続き `llvm.trap` を含む。

### Node E2E

`tests/integer_intrinsics.mjs`。
全型: i8/i16/i32/i64/i128/i8u/i16u/i32u/i64u/i128u。
代表値: min、min+1、-2、-1、0、1、2、max-1、max。
疑似乱数 1,000 cases per type。
各 API を JS BigInt reference で照合。
native `-O0`/`-O3`。
WASM `-O0`/`-O3`。
WASM imports 空。
IR determinism。

API 別:
min/max/clamp signed/unsigned。
clamp lo > hi trap。
abs MIN wrapping。
unsigned_abs MIN。
abs_diff all boundaries。
count_ones/leading/trailing zero。
rotate negative/large amounts。
swap_bytes/reverse_bits all 8-bit exhaustive。
is_power_of_two zero/one/powers/non-powers。
checked add/sub/mul overflow boundaries。
checked div/rem zero and MIN/-1。
checked_neg MIN。
saturating add/sub/mul boundaries。
pow exponent 0/1/2/large/negative。
widening_mul random all widths except 128。
i128/i128u や f16 など公開 ABI で直接観測できない値は、Tsuzuri 内部で bool/i64 checksum または 64-bit limb 分割を返す exported helper、または LLVM/C runtime harness で raw bytes を直接検査する。
公開 ABI は広げない。

Trap tests:
`Int.clamp 0 2 1`。
`Int.wrapping_pow 2 (-1)`。
通常 operator `/` zero が既存どおり trap。

Reference snippets:
Use `BigInt.asIntN(bits, value)` and `BigInt.asUintN(bits, value)` for wrapping。
Signed comparisons convert via `BigInt.asIntN`。
Unsigned comparisons via `BigInt.asUintN`。
Overflow detection compares mathematical result against min/max。
Saturating chooses min/max。
Rotate masks amount with `BigInt(bits - 1)` and uses unsigned representation then casts back。

### WASM helper checks

Build a fixture that uses i128 checked_mul, saturating_mul, widening_mul, rotate, bitreverse at `-O0` and `-O3`.
Run `WebAssembly.Module.imports(module)` and assert `[]`。
`llvm.smul.with.overflow.i128` が `__muloti4` を要求するため、`src/runtime/wasm.ll` の helper または limb lowering が必須。
helper を追加した場合は、helper 自身を直接呼ぶのではなく、Tsuzuri exported bool/i64 checksum 関数経由で i128 checked/saturating multiply の結果を検証する。
Inspect generated wasm object or linked module for unexpected imports.
Do not add host imports.

### 実装完了時の検証コマンド

GUIDE §3 に従い、小さい範囲から次を実行する。

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --locked --test integers
cargo test --locked
cargo build --release --locked
node tests/integer_intrinsics.mjs target/release/tsuzuri
node tests/primitives.mjs target/release/tsuzuri
node tests/numeric_casts.mjs target/release/tsuzuri
```

`tests/integer_intrinsics.mjs` は native/WASM × `-O0`/`-O3`、WASM imports 空、i128 checked/saturating multiply の `__muloti4` 未解決不在、BigInt reference 照合を必ず含める。

## ドキュメント

`docs/language.md` に `Int` モジュール表を追加する。
shift/rotate amount mask、checked vs trapping operator、saturating semantics、count result i64 を明記する。
`UnsignedInteger` marker class を型クラス表へ追加する。
`docs/architecture.md` に LLVM intrinsic mapping と WASM helper 方針を追加する。
`README.md` には小例だけ追加する。
`docs/benchmarks.md` は性能主張をしない限り不要。
もし popcount/rotate の命令利用を主張するなら生成 code inspection と測定条件を記録する。

## 受け入れ条件

- [ ] `Int.*` API が仕様の型で使える。
- [ ] checked 系は `Option` を使い、trap しない。
- [ ] trapping operator の既存意味は変わらない。
- [ ] shift/rotate amount mask が既存 shift と一致する。
- [ ] count result は i64。
- [ ] unsigned_abs/abs_diff/widening_mul の型が width から正しく計算される。
- [ ] `UnsignedOf`/`WidenOf` は concrete call site だけで解決し、generic code では `E1015`。
- [ ] LLVM intrinsic 宣言は重複せず決定的。
- [ ] i128 checked/saturating multiply を含む WASM build で `__muloti4` を含む undefined libcall/import がない。
- [ ] native/WASM `-O0`/`-O3` で BigInt reference と一致。
- [ ] docs が checked/saturating/wrapping/trapping の差を明記している。

## 落とし穴

`llvm.ctlz/cttz` の second argument を true にすると zero input が undef になる。
必ず false。
signed saturating_mul は overflow の向き判定が難しい。
`a` と `b` の符号と overflow flag から min/max を選ぶ。
`abs MIN` を trap や saturate にしてはいけない。
`unsigned_abs MIN` は表現可能。
`abs_diff` を signed subtraction で計算すると overflow する。
rotate amount をそのまま intrinsic に渡して LLVM semantics に任せず、既存 shift と同じ mask を明示する。
i128 checked_mul が WASM で helper を出す可能性を見落とさない。
`__muloti4` は wasm32 `-O0`/`-O3` で確認済みなので、helper または limb lowering を必ず実装する。
Option layout を独自に仮定しない。
`widening_mul` の i128 対応を無理に i256 なしで実装しない。

## 対象外

i256 型。
arbitrary precision integer。
carry chain API。
ADC/SBB。
bit deposit/extract。
SIMD integer intrinsic。
CPU feature dispatch。
constant-time crypto guarantee。
checked shift。
division with Euclidean remainder。

## 未決事項

`UnsignedInteger` marker class を D04 で追加するか、A06 型クラス拡張まで待つか。
既定案は D04 で method なし builtin marker として追加する。
`widening_mul` の戻りを 2 倍幅 scalar にするか `(low, high)` tuple にするか。
既定案は 2 倍幅 scalar。
`Int.abs` を wrapping MIN にするか saturating にするか。
既定案は Rust の `wrapping_abs` に近い wrapping MIN。
saturating abs は D04 では追加しない。

台帳の見直し提案: なし。`UnsignedOf`/`WidenOf` は E02 の `BuiltinType` に含まれる前提であり、phase 1 では concrete call site 専用とする。
