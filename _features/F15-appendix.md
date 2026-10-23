# F15 付録: 検証済みの std・fixture・E2E suite

[F15 本体](F15-sequential-equivalent-data-parallel.md)の付録。A〜G のコードは、HEAD の複製に置いて `cargo build --release` したコンパイラで、
E2E suite `monoid`（90 件）・`parallel_equivalence`（48 件）・既存の `parallel`（36 件）が native と WASM の `-O0`／`-O3` で通り、
`tsuzuri test tests/fixtures/monoid` が 11 件通った**検証済みの全文**である。I の `monoid.md` は `check-docs` を通した全文、J の `tests/monoid.rs` は E1039 以外を E1039 のないコンパイラで通した全文、K は文書の変更の全文（`parallel.md`・`array.md`・`index.md` は `check-docs` を通した）。
E1039（Phase 1 の `src/check.rs`・`src/polymorph.rs` の変更）と `--report parallel`（Phase 3）のコードは、
この付録にない（本体の「設計」に手順で書いた。実装して初めて確かめられる）。
実装者は、std・fixture・テスト・文書の全文を**そのまま写し、書き換えない**。

## A. `std/Monoid.tt`（新規）

<!-- file: std/Monoid.tt -->
```text
/// An associative operation with an identity element. For every `a`, `b` and `c` of the type, `combine (combine a b) c` and `combine a (combine b c)`
/// are equal, and so are `combine (empty ()) a`, `combine a (empty ())` and `a`, bit for bit. `Array.fold_monoid` and `Parallel.fold` then return the
/// same value whatever the chunks of `Parallel.fold` are. The compiler cannot check the laws; it rejects an instance for a type that holds a
/// floating-point number (`E1039`), because floating-point addition is not associative.
class Monoid<'m> {
    def empty :: unit -> 'm
    def combine :: 'm -> 'm -> 'm
}
```

## B. `std/Monoids.tz`（新規。opt-in。利用者は `Monoids.Sum` のように修飾して書く）

<!-- file: std/Monoids.tz -->
```text
/// The sum of integers: `empty` is 0 and `combine` is `+`, which wraps around like the operator.
record Sum<'a> { value: 'a }

/// The product of integers: `empty` is 1 and `combine` is `*`, which wraps around like the operator.
record Product<'a> { value: 'a }

/// The largest of the integers seen so far. `empty` has no value (`Maybe.None`); `combine` keeps the left one when the two are equal.
record MaxOf<'a> { value: Maybe<'a> }

/// The smallest of the integers seen so far. `empty` has no value (`Maybe.None`); `combine` keeps the left one when the two are equal.
record MinOf<'a> { value: Maybe<'a> }

/// The bitwise and of integers: `empty` has every bit set.
record BitAnd<'a> { value: 'a }

/// The bitwise or of integers: `empty` is 0.
record BitOr<'a> { value: 'a }

/// The bitwise exclusive or of integers: `empty` is 0.
record BitXor<'a> { value: 'a }

/// Whether every `bool` is `true`: `empty` is `true`.
record All { value: bool }

/// Whether some `bool` is `true`: `empty` is `false`.
record Any { value: bool }

instance (Integer<'a>, Numeric<'a>, Add<'a>, Copy<'a>) => Monoid<Sum<'a>> {
    let empty = \() -> Sum { value: 0i64 as 'a }
    fn combine left right = Sum { value: left.value + right.value }
}

instance (Integer<'a>, Numeric<'a>, Mul<'a>, Copy<'a>) => Monoid<Product<'a>> {
    let empty = \() -> Product { value: 1i64 as 'a }
    fn combine left right = Product { value: left.value * right.value }
}

instance (Integer<'a>, Ord<'a>, Copy<'a>) => Monoid<MaxOf<'a>> {
    let empty = \() -> MaxOf { value: Maybe.None }
    fn combine left right =
        match (left.value, right.value) with
        | (Maybe.None, _) -> right
        | (_, Maybe.None) -> left
        | (Maybe.Some a, Maybe.Some b) -> if Ord.lt (ref a) (ref b) then right else left
}

instance (Integer<'a>, Ord<'a>, Copy<'a>) => Monoid<MinOf<'a>> {
    let empty = \() -> MinOf { value: Maybe.None }
    fn combine left right =
        match (left.value, right.value) with
        | (Maybe.None, _) -> right
        | (_, Maybe.None) -> left
        | (Maybe.Some a, Maybe.Some b) -> if Ord.lt (ref b) (ref a) then right else left
}

instance (Integer<'a>, Numeric<'a>, Bits<'a>, Copy<'a>) => Monoid<BitAnd<'a>> {
    let empty = \() -> BitAnd { value: Bits.bit_not (0i64 as 'a) }
    fn combine left right = BitAnd { value: Bits.bit_and left.value right.value }
}

instance (Integer<'a>, Numeric<'a>, Bits<'a>, Copy<'a>) => Monoid<BitOr<'a>> {
    let empty = \() -> BitOr { value: 0i64 as 'a }
    fn combine left right = BitOr { value: Bits.bit_or left.value right.value }
}

instance (Integer<'a>, Numeric<'a>, Bits<'a>, Copy<'a>) => Monoid<BitXor<'a>> {
    let empty = \() -> BitXor { value: 0i64 as 'a }
    fn combine left right = BitXor { value: Bits.bit_xor left.value right.value }
}

instance Monoid<All> {
    let empty = \() -> All { value: true }
    fn combine left right = All { value: left.value && right.value }
}

instance Monoid<Any> {
    let empty = \() -> Any { value: false }
    fn combine left right = Any { value: left.value || right.value }
}
```

## C. `std/Array.tz` と `std/Parallel.tz` への追加

`std/Array.tz`: 既存の `def fold`（`fn fold folder initial values =` の 4 行）の**直後**に、空行を 1 つ置いて次を足す（インデントは空白 4 つ）。

<!-- file: std/Array.tz (addition) -->
```text
/// Folds `values` from the left with `Monoid.combine`, starting from `Monoid.empty ()`. `Parallel.fold` returns the same value.
def fold_monoid :: (Monoid<'m>, Copy<'m>) => ref ['m] -> 'm
fn fold_monoid values =
    let mut state = Monoid.empty ()
    for value in values do state = Monoid.combine state value
    state
```

`std/Parallel.tz`: 既存の 2 行（`def sum ...` と `fn sum values = ...`。末尾に改行がない）の**後**に、空行を 1 つ置いて次を足す。

<!-- file: std/Parallel.tz (addition) -->
```text
/// Folds `values` with `Monoid.combine` in chunks (see `Parallel.reduce`) and returns the same value as `Array.fold_monoid`, for any chunks, when the
/// instance obeys the laws of `Monoid` and `empty` and `combine` have no side effects.
def fold :: (Monoid<'m>, Copy<'m>, Send<'m>) => ref ['m] -> 'm
fn fold values = Parallel.reduce (Monoid.empty ()) (\left right -> Monoid.combine left right) values
```

## D. `src/stdlib.rs` の変更（4 か所と 1 つのテストの数）

1. `SOURCES` の `("std/Maybe.tc", ...)` と `("std/Mutex.tz", ...)` の間に 2 行を足す:
   `    ("std/Monoid.tt", include_str!("../std/Monoid.tt")),` と `    ("std/Monoids.tz", include_str!("../std/Monoids.tz")),`
2. `RESERVED_MODULES` の末尾（`"Channel",` の後）に `"Monoid",` と `"Monoids",` を足す。
3. `OPT_IN` の末尾（`module: "Channel"` の要素の後）に、次の要素を足す。
   ```rust
       OptIn {
           module: "Monoids",
           names: &["Monoids"],
           aliases: &[],
           uses: &[],
       },
   ```
4. テスト `reserves_the_d07_table` の `assert_eq!(RESERVED_MODULES.len(), 53);` を `55` にする（他のチケットが先に std のモジュールを足して数が 53 でなければ、その数に 2 を足す）。

`Monoid`（`.tt`）は opt-in にしない（`std/Array.tz`・`std/Parallel.tz` が名前を使う。常時ロードの std が opt-in を使えないという `opt_in_modules_are_reached_only_through_their_names` の規則による）。

## E. `tests/fixtures/monoid/Main.tz`（新規。全文）

<!-- file: tests/fixtures/monoid/Main.tz -->
```text
// F15: Parallel.fold returns the value of Array.fold_monoid, bit for bit, for every Monoid of std and for the monoids of this file.
// The E2E suite `monoid` calls the `export def` functions; `tsuzuri test` runs the laws at the end. Every expected value is worked out by an
// independent loop in tests/features.mjs, not by this program.

// Any value of the E2E functions that is not a total: the parallel and the sequential fold differ.
const Mismatch: i64 = 9223372036854775807

// ---------------------------------------------------------------------------------------------------------------------
// Monoids that std does not have. Composing the functions x -> scale * x + shift is associative but not commutative, so a wrong chunk order shows.

record Affine { scale: i64, shift: i64 }

instance Monoid<Affine> {
    let empty = \() -> Affine { scale: 1, shift: 0 }
    fn combine left right = Affine { scale: right.scale * left.scale, shift: right.scale * left.shift + right.shift }
}

record Stats { count: i64, total: i64, largest: Maybe<i64> }

instance Monoid<Stats> {
    let empty = \() -> Stats { count: 0, total: 0, largest: Maybe.None }
    fn combine left right =
        let largest =
            match (left.largest, right.largest) with
            | (Maybe.None, _) -> right.largest
            | (_, Maybe.None) -> left.largest
            | (Maybe.Some a, Maybe.Some b) -> if a < b then right.largest else left.largest
        Stats { count: left.count + right.count, total: left.total + right.total, largest: largest }
}

// ---------------------------------------------------------------------------------------------------------------------
// The elements: a deterministic pseudo-random number for each index, between -500000 and 500002.

def mix :: i64 -> i64
fn mix index = (index * 2654435761 + 12345) % 1000003 - 500000

// The parallel fold and the sequential fold of `values`.
def both :: (Monoid<'m>, Copy<'m>, Send<'m>) => ref ['m] -> ('m * 'm)
fn both values = (Parallel.fold values, Array.fold_monoid values)

def value_or :: Maybe<i64> -> i64 -> i64
fn value_or maybe fallback = match maybe with | Maybe.Some value -> value | Maybe.None -> fallback

def flag :: bool -> i64
fn flag value = if value then 1 else 0

// ---------------------------------------------------------------------------------------------------------------------
// One function for each Monoid. The result is the folded value as an i64, or Mismatch when the two folds differ.

export def sum_total :: i64 -> i64
fn sum_total count =
    let values = new [Monoids.Sum<i64>](count, \index -> Monoids.Sum { value: mix index })
    match both (ref values) with
    | (parallel, sequential) -> if parallel.value == sequential.value then parallel.value else Mismatch

export def product_total :: i64 -> i64
fn product_total count =
    let values = new [Monoids.Product<i32>](count, \index -> Monoids.Product { value: Bits.bit_or ((mix index) as i32) 1 })
    match both (ref values) with
    | (parallel, sequential) -> if parallel.value == sequential.value then parallel.value as i64 else Mismatch

export def max_total :: i64 -> i64
fn max_total count =
    let values = new [Monoids.MaxOf<i64>](count, \index -> Monoids.MaxOf { value: Maybe.Some (mix index) })
    match both (ref values) with
    | (parallel, sequential) -> if value_or parallel.value (-1) == value_or sequential.value (-1) then value_or parallel.value (-1000000) else Mismatch

export def min_total :: i64 -> i64
fn min_total count =
    let values = new [Monoids.MinOf<i8>](count, \index -> Monoids.MinOf { value: Maybe.Some ((mix index) as i8) })
    match both (ref values) with
    | (parallel, sequential) ->
        let left = match parallel.value with | Maybe.Some value -> value as i64 | Maybe.None -> 1000
        let right = match sequential.value with | Maybe.Some value -> value as i64 | Maybe.None -> 1000
        if left == right then left else Mismatch

export def and_total :: i64 -> i64
fn and_total count =
    let values = new [Monoids.BitAnd<i32u>](count, \index -> Monoids.BitAnd { value: Bits.bit_or ((mix index) as i32u) 4042322160 })
    match both (ref values) with
    | (parallel, sequential) -> if parallel.value == sequential.value then parallel.value as i64 else Mismatch

export def or_total :: i64 -> i64
fn or_total count =
    let values = new [Monoids.BitOr<i64>](count, \index -> Monoids.BitOr { value: Bits.bit_and (mix index) 1048575 })
    match both (ref values) with
    | (parallel, sequential) -> if parallel.value == sequential.value then parallel.value else Mismatch

export def xor_total :: i64 -> i64
fn xor_total count =
    let values = new [Monoids.BitXor<i64>](count, \index -> Monoids.BitXor { value: mix index })
    match both (ref values) with
    | (parallel, sequential) -> if parallel.value == sequential.value then parallel.value else Mismatch

export def all_total :: i64 -> i64
fn all_total count =
    let values = new [Monoids.All](count, \index -> Monoids.All { value: index != 1000002 })
    match both (ref values) with
    | (parallel, sequential) -> if parallel.value == sequential.value then flag parallel.value else Mismatch

export def any_total :: i64 -> i64
fn any_total count =
    let values = new [Monoids.Any](count, \index -> Monoids.Any { value: index % 4099 == 4098 })
    match both (ref values) with
    | (parallel, sequential) -> if parallel.value == sequential.value then flag parallel.value else Mismatch

export def affine_total :: i64 -> i64
fn affine_total count =
    let values = new [Affine](count, \index -> Affine { scale: index % 5 + 1, shift: mix index % 17 })
    match both (ref values) with
    | (parallel, sequential) ->
        if parallel.scale == sequential.scale && parallel.shift == sequential.shift then parallel.scale * 1000003 + parallel.shift else Mismatch

export def stats_total :: i64 -> i64
fn stats_total count =
    let values = new [Stats](count, \index -> Stats { count: 1, total: mix index, largest: Maybe.Some (mix index) })
    match both (ref values) with
    | (parallel, sequential) ->
        let same = parallel.count == sequential.count && parallel.total == sequential.total && value_or parallel.largest (-1000000) == value_or sequential.largest (-1000000)
        if same then parallel.count * 1000003 + parallel.total + value_or parallel.largest 0 else Mismatch

// 4,194,304 elements fill 1,024 chunks of 4,096; one more element makes the chunk count stop at 1,024, with chunks of 4,096 and 4,097 elements.
export def cap_xor :: i64 -> i64
fn cap_xor count =
    let values = new [Monoids.BitXor<i8u>](count, \index -> Monoids.BitXor { value: (index % 251) as i8u })
    match both (ref values) with
    | (parallel, sequential) -> if parallel.value == sequential.value then parallel.value as i64 else Mismatch

// ---------------------------------------------------------------------------------------------------------------------
// The laws, as property tests: associativity, and the identity on both sides. `same` compares two values bit for bit.

def laws :: (Monoid<'m>, Copy<'m>) => ('m -> 'm -> bool) -> ('m * ('m * 'm)) -> bool
fn laws same triple =
    match triple with
    | (a, (b, c)) ->
        let associative = same (Monoid.combine (Monoid.combine a b) c) (Monoid.combine a (Monoid.combine b c))
        let left = same (Monoid.combine (Monoid.empty ()) a) a
        let right = same (Monoid.combine a (Monoid.empty ())) a
        associative && left && right

def numbers :: Gen<(i64 * (i64 * i64))>
fn numbers = Gen.pair (Gen.i64()) (Gen.pair (Gen.i64()) (Gen.i64()))

def bytes :: Gen<(i64 * (i64 * i64))>
fn bytes = Gen.pair (Gen.range 0 255) (Gen.pair (Gen.range 0 255) (Gen.range 0 255))

def flags :: Gen<(bool * (bool * bool))>
fn flags = Gen.pair (Gen.bool()) (Gen.pair (Gen.bool()) (Gen.bool()))

def pair_of_numbers :: Gen<(i64 * i64)>
fn pair_of_numbers = Gen.pair (Gen.i64()) (Gen.i64())

def affines :: Gen<((i64 * i64) * ((i64 * i64) * (i64 * i64)))>
fn affines = Gen.pair (pair_of_numbers()) (Gen.pair (pair_of_numbers()) (pair_of_numbers()))

def maybe_of :: bool -> i64 -> Maybe<i64>
fn maybe_of has value = if has then Maybe.Some value else Maybe.None

def maybe_pair :: Gen<(bool * i64)>
fn maybe_pair = Gen.pair (Gen.bool()) (Gen.range (-5) 5)

def maybe_numbers :: Gen<((bool * i64) * ((bool * i64) * (bool * i64)))>
fn maybe_numbers = Gen.pair (maybe_pair()) (Gen.pair (maybe_pair()) (maybe_pair()))

def same_maybe :: Maybe<i64> -> Maybe<i64> -> bool
fn same_maybe left right = value_or left 100 == value_or right 100 && flag (Maybe.is_some left) == flag (Maybe.is_some right)

test "Sum<i64> obeys the laws" = Gen.for_all_cases 200 (numbers()) (\t -> match t with | (a, (b, c)) -> laws (\x y -> x.value == y.value) (Monoids.Sum { value: a }, (Monoids.Sum { value: b }, Monoids.Sum { value: c })))
test "Sum<i8u> obeys the laws" = Gen.for_all_cases 200 (bytes()) (\t -> match t with | (a, (b, c)) -> laws (\x y -> x.value == y.value) (Monoids.Sum { value: a as i8u }, (Monoids.Sum { value: b as i8u }, Monoids.Sum { value: c as i8u })))
test "Product<i32> obeys the laws" = Gen.for_all_cases 200 (numbers()) (\t -> match t with | (a, (b, c)) -> laws (\x y -> x.value == y.value) (Monoids.Product { value: a as i32 }, (Monoids.Product { value: b as i32 }, Monoids.Product { value: c as i32 })))
test "BitAnd<i8u> obeys the laws" = Gen.for_all_cases 200 (bytes()) (\t -> match t with | (a, (b, c)) -> laws (\x y -> x.value == y.value) (Monoids.BitAnd { value: a as i8u }, (Monoids.BitAnd { value: b as i8u }, Monoids.BitAnd { value: c as i8u })))
test "BitOr<i64> obeys the laws" = Gen.for_all_cases 200 (numbers()) (\t -> match t with | (a, (b, c)) -> laws (\x y -> x.value == y.value) (Monoids.BitOr { value: a }, (Monoids.BitOr { value: b }, Monoids.BitOr { value: c })))
test "BitXor<i64> obeys the laws" = Gen.for_all_cases 200 (numbers()) (\t -> match t with | (a, (b, c)) -> laws (\x y -> x.value == y.value) (Monoids.BitXor { value: a }, (Monoids.BitXor { value: b }, Monoids.BitXor { value: c })))
test "MaxOf<i64> obeys the laws" = Gen.for_all_cases 200 (maybe_numbers()) (\t -> match t with | ((ha, a), ((hb, b), (hc, c))) -> laws (\x y -> same_maybe x.value y.value) (Monoids.MaxOf { value: maybe_of ha a }, (Monoids.MaxOf { value: maybe_of hb b }, Monoids.MaxOf { value: maybe_of hc c })))
test "MinOf<i64> obeys the laws" = Gen.for_all_cases 200 (maybe_numbers()) (\t -> match t with | ((ha, a), ((hb, b), (hc, c))) -> laws (\x y -> same_maybe x.value y.value) (Monoids.MinOf { value: maybe_of ha a }, (Monoids.MinOf { value: maybe_of hb b }, Monoids.MinOf { value: maybe_of hc c })))
test "All obeys the laws" = Gen.for_all_cases 50 (flags()) (\t -> match t with | (a, (b, c)) -> laws (\x y -> x.value == y.value) (Monoids.All { value: a }, (Monoids.All { value: b }, Monoids.All { value: c })))
test "Any obeys the laws" = Gen.for_all_cases 50 (flags()) (\t -> match t with | (a, (b, c)) -> laws (\x y -> x.value == y.value) (Monoids.Any { value: a }, (Monoids.Any { value: b }, Monoids.Any { value: c })))
test "Affine obeys the laws" = Gen.for_all_cases 200 (affines()) (\t -> match t with | ((a, b), ((c, d), (e, f))) -> laws (\x y -> x.scale == y.scale && x.shift == y.shift) (Affine { scale: a, shift: b }, (Affine { scale: c, shift: d }, Affine { scale: e, shift: f })))
```

## F. `tests/fixtures/parallel_equivalence/Main.tz`（新規。全文）

<!-- file: tests/fixtures/parallel_equivalence/Main.tz -->
```text
// F15: for a pure callback, each Parallel function returns what its sequential counterpart returns, at the sizes around the chunk boundaries.
// The sum of floating-point numbers is the documented exception: it is the same on every machine, and it can differ from Array.sum.
// The E2E suite `parallel_equivalence` calls the `export def` functions; each one returns 1 when the two results are equal and 0 when they are not.

def mix :: i64 -> i64
fn mix index = (index * 2654435761 + 12345) % 1000003 - 500000

def flag :: bool -> i64
fn flag value = if value then 1 else 0

def same :: ref [i64] -> ref [i64] -> bool
fn same left right =
    let mut equal = left.length == right.length
    let mut index = 0
    while equal && index < left.length do
        equal = left[index] == right[index]
        index = index + 1
    equal

def scale :: i64 -> ref mut [i64..] -> unit
fn scale start chunk =
    for index in 0i64 .. (chunk.length - 1) do
        Array.write index ((start + index) * 2 + 1) chunk

export def map_equal :: i64 -> i64
fn map_equal count =
    let values = new [i64](count, \index -> mix index)
    let parallel = Parallel.map (\value -> value * 3 + 1) (ref values)
    let sequential = Array.map (\value -> value * 3 + 1) (ref values)
    flag (same (ref parallel) (ref sequential))

export def map_ref_equal :: i64 -> i64
fn map_ref_equal count =
    let values = new [i64](count, \index -> mix index)
    let parallel = Parallel.map_ref (\value -> deref value * 3 + 1) (ref values)
    let sequential = Array.map_ref (\value -> deref value * 3 + 1) (ref values)
    flag (same (ref parallel) (ref sequential))

export def init_equal :: i64 -> i64
fn init_equal count =
    let parallel = Parallel.init count (\index -> mix index * 5)
    let sequential = new [i64](count, \index -> mix index * 5)
    flag (same (ref parallel) (ref sequential))

// The chunks of 3,000 elements come from `size`, not from the 4,096 of the other functions.
export def for_each_chunk_equal :: i64 -> i64
fn for_each_chunk_equal count =
    let mut parallel = new [i64](count, \_index -> 0)
    Parallel.for_each_chunk 3000 scale (ref mut parallel)
    let sequential = new [i64](count, \index -> index * 2 + 1)
    flag (same (ref parallel) (ref sequential))

export def sum_equal :: i64 -> i64
fn sum_equal count =
    let values = new [i64](count, \index -> mix index)
    flag (Parallel.sum (ref values) == Array.sum (ref values))

def decimals :: i64 -> [f64]
fn decimals count = new [f64](count, \index -> (((index * 7919 + 13) % 1009) as f64) / 10.0 + 0.1)

export def float_sum_equal :: i64 -> i64
fn float_sum_equal count =
    let values = decimals count
    flag (Parallel.sum (ref values) == Array.sum (ref values))
```

## G. `tests/features.mjs` の suite（2 つ。`arena: {` の直前に足す）

<!-- file: tests/features.mjs (two suites) -->
```text
  monoid: {
    cases: (() => {
      // Independent references: plain loops over BigInt, with the wrapping of each element type.
      const mix = (index) => (index * 2654435761n + 12345n) % 1000003n - 500000n;
      const loop = (count, initial, step) => {
        let state = initial;
        for (let index = 0n; index < count; index++) state = step(state, index);
        return state;
      };
      const references = {
        sum_total: (count) => loop(count, 0n, (total, index) => total + mix(index)),
        product_total: (count) => loop(count, 1n, (total, index) => BigInt.asIntN(32, total * (BigInt.asIntN(32, mix(index)) | 1n))),
        max_total: (count) => count === 0n ? -1000000n : loop(count, mix(0n), (best, index) => mix(index) > best ? mix(index) : best),
        min_total: (count) => count === 0n ? 1000n : loop(count, BigInt.asIntN(8, mix(0n)), (best, index) => BigInt.asIntN(8, mix(index)) < best ? BigInt.asIntN(8, mix(index)) : best),
        and_total: (count) => loop(count, 4294967295n, (total, index) => total & ((BigInt.asUintN(32, mix(index)) | 4042322160n))),
        or_total: (count) => loop(count, 0n, (total, index) => total | (mix(index) & 1048575n)),
        xor_total: (count) => loop(count, 0n, (total, index) => total ^ mix(index)),
        all_total: (count) => count <= 1000002n ? 1n : 0n,
        any_total: (count) => count > 4098n ? 1n : 0n,
        affine_total: (count) => {
          const [scale, shift] = loop(count, [1n, 0n], ([scale, shift], index) => {
            const next = [index % 5n + 1n, mix(index) % 17n];
            return [BigInt.asIntN(64, next[0] * scale), BigInt.asIntN(64, next[0] * shift + next[1])];
          });
          return BigInt.asIntN(64, scale * 1000003n + shift);
        },
        stats_total: (count) => count === 0n ? 0n : count * 1000003n + loop(count, 0n, (total, index) => total + mix(index)) + loop(count, mix(0n), (best, index) => mix(index) > best ? mix(index) : best),
      };
      const sizes = [0n, 1n, 4095n, 4096n, 4097n, 8193n, 12288n, 100000n];
      return [
        ...Object.entries(references).flatMap(([name, reference]) => sizes.map((count) => [name, [count], reference(count)])),
        ...[4194304n, 4194305n].map((count) => ["cap_xor", [count], loop(count, 0n, (total, index) => total ^ (index % 251n))]),
      ];
    })(),
    traps: [],
  },
  parallel_equivalence: {
    cases: (() => {
      const sizes = [0n, 1n, 4095n, 4096n, 4097n, 8193n, 12288n, 100000n];
      // The sum of f64 by the chunk rule of docs/language.md and by one running total, in IEEE double arithmetic.
      const decimals = (count) => Array.from({ length: Number(count) }, (_, index) => ((index * 7919 + 13) % 1009) / 10 + 0.1);
      const sequential = (values) => values.reduce((total, value) => total + value, 0);
      const chunked = (values) => {
        const length = values.length;
        if (length === 0) return 0;
        const chunks = Math.min(1024, Math.ceil(length / 4096));
        const start = (chunk) => Math.floor(length / chunks) * chunk + Math.floor(((length % chunks) * chunk) / chunks);
        let total = 0;
        for (let chunk = 0; chunk < chunks; chunk++) total += values.slice(start(chunk), start(chunk + 1)).reduce((part, value) => part + value, 0);
        return total;
      };
      return [
        ...["map_equal", "map_ref_equal", "init_equal", "for_each_chunk_equal", "sum_equal"].flatMap((name) => sizes.map((count) => [name, [count], 1n])),
        ...sizes.map((count) => ["float_sum_equal", [count], sequential(decimals(count)) === chunked(decimals(count)) ? 1n : 0n]),
      ];
    })(),
    traps: [],
  },
```

## H. 実測と試作の結果

- 試作 1（利用者のプロジェクト。`Monoid.tt`・`Monoids.tz`・`Reduce.tz` に同じ内容を置いた）: `Parallel.reduce (Monoid.empty ()) (\left right -> Monoid.combine left right)` は、`Sum<i64>` と非可換な `Affine` で、要素数 0・1・2・4095・4096・4097・8191・8192・8193・12288・100000・4194304・4194305 のすべてで、逐次の畳み込みと一致した（差 0）。
  `combine` の引数を入れ替える変異（`Monoid.combine right left`）は、`Sum`（可換）では差が出ず、`Affine` で要素数 2 から差が出た（`2 → -8`、`4095 → -9215219183495582855`）。**非可換なモノイドが要る理由**。
- 整数は折り返す: `Sum<i32>` の `2147483647 + 1 + (-1)` は `2147483647`（トラップしない。`-O0`・`-O3`・WASM で同じ）。折り返しが結合的なので、整数の `Sum`・`Product` は逐次と同一になる。
- `check` の実出力（付録 D の変更を持つコンパイラ）:
  - `[Sum { value: 1 }]`（修飾なし）: `error[E1004]: unknown record, union, or type alias 'Sum'`（opt-in は修飾して書く）。
  - `[Monoids.Sum { value: 1.5 }]` に `Parallel.fold`: `error[E1005]: no instance for Integer<f64>; define an instance or use a supported type`（E1039 を足す前の出力）。
  - `record Mean { total: f64, count: i64 }` に `instance Monoid<Mean>`: **受理される**（E1039 を足す前）。
  - 利用者の `Monoids.tz`: `error[E1011]: module name 'Monoids' is reserved for the standard library; rename the file`。
- E2E: `monoid` が `features: 90 cases passed`、`parallel_equivalence` が `features: 48 cases passed`、既存の `parallel` が `features: 36 cases passed`。`tsuzuri test tests/fixtures/monoid` が `11 passed; 0 failed; 0 ignored`。
- `cargo build --release --offline`（`std` の 4 ファイルと `src/stdlib.rs` の変更を含む複製）: 3 分 6 秒。

## I. `_tsuzuri/language-reference/built-in-types-and-modules/monoid.md`（新規。全文。`check-docs` で `10 links, 2 checked examples, 4 native runs`）

`````markdown
# Monoid

`Monoid` は、「空の値」と「2 つの値を 1 つにする結合的な演算」を持つ型の型クラスです。`Array.fold_monoid` は左から順に、[`Parallel.fold`](parallel.md) はチャンクに分けて畳み込みます。法則が成り立ち、`empty` と `combine` が出力などの作用を持たなければ、**2 つは同じビットの値**を返します。

## この記事のポイント

- クラスは `Monoid<'m>` です。`empty :: unit -> 'm` が空の値、`combine :: 'm -> 'm -> 'm` が演算です。呼び出しは `Monoid.empty ()` と `Monoid.combine left right` です。
- 法則は 3 つです。結合律（`combine (combine a b) c` と `combine a (combine b c)` が同じ値）、左単位元（`combine (empty ()) a` が `a` と同じ値）、右単位元（`combine a (empty ())` が `a` と同じ値）。「同じ値」はビットまで同じです。
- std は整数向けの型を `Monoids` に持ちます（`Sum`・`Product`・`MaxOf`・`MinOf`・`BitAnd`・`BitOr`・`BitXor`・`All`・`Any`）。`Monoids.Sum` のように**修飾して**書きます。`Monoids` を名前に出したプログラムだけが、このモジュールを読み込みます。
- 浮動小数点を含む型には `Monoid` が使えません（`E1039`）。加算が結合的でないので、チャンクの分け方で結果が変わるからです。
- 法則はコンパイラが検査できません。自分の `instance` には、`Gen` の性質テストを書いてください。

## 整数の合計を畳み込む

```tsuzuri run=15
let values = [Monoids.Sum { value: 1 }, Monoids.Sum { value: 2 }, Monoids.Sum { value: 3 }, Monoids.Sum { value: 4 }, Monoids.Sum { value: 5 }]
let total = Parallel.fold (ref values)
total.value
```

実行結果:

```text
15
```

`Monoids.Sum` の `empty` は 0、`combine` は `+` です。`+` は折り返すので、`i32` で `2147483647 + 1` は `-2147483648` になりますが、折り返しも結合的です。だから整数の `Sum`・`Product` は、どんなチャンクでも逐次と同じ値になります。

## 自分の型を `Monoid` にする

件数と合計を一緒に持つ型は、足し合わせても結合的です。`empty` は `let empty = \() -> ...` と書きます（`fn empty () = ...` は構文エラーです）。

```tsuzuri run=120
record Totals { count: i64, total: i64 }

instance Monoid<Totals> {
    let empty = \() -> Totals { count: 0, total: 0 }
    fn combine left right = Totals { count: left.count + right.count, total: left.total + right.total }
}

let values = [Totals { count: 1, total: 10 }, Totals { count: 1, total: 20 }, Totals { count: 1, total: 30 }]
let sequential = Array.fold_monoid (ref values)
let parallel = Parallel.fold (ref values)
sequential.total + parallel.total
```

実行結果:

```text
120
```

`Array.fold_monoid` は `empty ()` から添字の昇順に `combine` を重ねます。`Parallel.fold` は同じ値を、配列をチャンクに分けて返します。法則が成り立つので、部分結果を左から結合しても、全体を左から畳み込んでも同じです。

## 法則をテストする

`combine` が結合的であることは、次の形のテストで確かめてください。非可換な演算（関数の合成）も入れると、結合の順序の誤りが見つかります。

```text
def laws :: (Monoid<'m>, Copy<'m>) => ('m -> 'm -> bool) -> ('m * ('m * 'm)) -> bool
fn laws same triple =
    match triple with
    | (a, (b, c)) ->
        let associative = same (Monoid.combine (Monoid.combine a b) c) (Monoid.combine a (Monoid.combine b c))
        let left = same (Monoid.combine (Monoid.empty ()) a) a
        let right = same (Monoid.combine a (Monoid.empty ())) a
        associative && left && right
```

`same` は 2 つの値がビットまで同じかどうかを返す関数です。`Gen.for_all_cases` に 3 つ組の生成器を渡して、`laws` を性質として走らせます（[Gen](gen.md)）。

## 浮動小数点は使えない

```text
record Mean { total: f64, count: i64 }
instance Monoid<Mean> { ... }
let result = Array.fold_monoid (ref means)
```

この `Array.fold_monoid` は `E1039` で拒否されます（`no Monoid instance for Main.Mean: it holds a floating-point number ...`）。`f64` の加算は結合的でなく、`Parallel.fold` がチャンクごとに足すと、逐次の `Array.fold_monoid` と下位のビットが違うことがあるからです。
浮動小数点の合計が要るなら、[`Parallel.sum`](parallel.md)（チャンクの規則で順序が固定。逐次と違いうる）か、`Array.sum_pairwise`・`Array.sum_kahan`（[Math](math.md)）を使います。固定小数点（整数の `micros`）で持てば、整数の `Monoid` にできます。

## まとめ

- `Monoid` は空の値と結合的な演算です。法則（結合律・単位元）はビットまで同じでなければなりません。
- `Array.fold_monoid` と `Parallel.fold` は、法則が成り立ち、`empty`・`combine` に出力などの作用がなければ同じ値を返します。作用があると、`combine` を呼ぶ回数と順序が違います。
- std の整数向けの型は `Monoids.Sum` のように修飾して書きます。浮動小数点を含む型は `E1039` です。
- 法則は `Gen` で自分でテストします。

## 関連項目

- [Parallel](parallel.md)
- [Array](array.md)
- [Gen](gen.md)
- [Math](math.md)
- [診断コード](../compiler/diagnostics.md)
- [言語リファレンスの目次](../index.md)
`````

## J. `tests/monoid.rs`（新規。全文。E1039 以外の 3 件は E1039 のないコンパイラで緑を確かめた。`float_monoids_are_rejected_where_they_are_used` は E1039 を足した後に緑になる）

```rust
//! `Monoid`, `Monoids` and the float-instance rejection E1039 (F15 Phase 1).

use std::process::{Command, Output};

use tsuzuri::{analyze, analyze_modules, llvm};

const MEAN: &str = r#"record Mean { total: f64, count: i64 }
instance Monoid<Mean> {
    let empty = \() -> Mean { total: 0.0, count: 0 }
    fn combine left right = Mean { total: left.total + right.total, count: left.count + right.count }
}
"#;

const PAIR: &str = r#"record Pair { left: i64, right: f32 }
instance Monoid<Pair> {
    let empty = \() -> Pair { left: 0, right: 0.0 }
    fn combine left right = Pair { left: left.left + right.left, right: left.right + right.right }
}
"#;

const OPTIONAL: &str = r#"record Optional { value: Maybe<f64> }
instance Monoid<Optional> {
    let empty = \() -> Optional { value: Maybe.None }
    fn combine _left right = right
}
"#;

const MONEY: &str = r#"record Money { amount: d64 }
instance Monoid<Money> {
    let empty = \() -> Money { amount: 0.0m }
    fn combine left right = Money { amount: left.amount + right.amount }
}
"#;

const LANES: &str = r#"record Lanes { value: f32x4 }
instance Monoid<Lanes> {
    let empty = \() -> Lanes { value: Simd.splat 0.0f32 }
    fn combine left right = Lanes { value: left.value + right.value }
}
"#;

const FIXED: &str = r#"record Fixed { micros: i64 }
instance Monoid<Fixed> {
    let empty = \() -> Fixed { micros: 0 }
    fn combine left right = Fixed { micros: left.micros + right.micros }
}
"#;

fn e1039_message(ty: &str) -> String {
    format!(
        "no Monoid instance for {ty}: it holds a floating-point number, whose operations are not associative, so Parallel.fold could differ from Array.fold_monoid; accumulate in integers (fixed point) or use Parallel.sum for a documented order"
    )
}

fn emits(source: &str) {
    let module = analyze(source).unwrap_or_else(|error| {
        panic!("{source}\n{}: {}", error.code, error.message);
    });
    for wasm in [false, true] {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        assert_eq!(
            ir,
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap(),
            "{source}: IR is deterministic"
        );
    }
}

#[test]
fn float_monoids_are_rejected_where_they_are_used() {
    let cases = [
        (format!("{MEAN}let means = [Mean {{ total: 1.5, count: 1 }}]\nlet total = Array.fold_monoid (ref means)\ntotal.count"), "Main.Mean", "Array.fold_monoid"),
        (format!("{PAIR}let values = [Pair {{ left: 1, right: 1.0 }}]\nlet total = Parallel.fold (ref values)\ntotal.left"), "Main.Pair", "Parallel.fold"),
        (format!("{OPTIONAL}let values = [Optional {{ value: Maybe.Some 1.0 }}]\nlet total = Array.fold_monoid (ref values)\n0"), "Main.Optional", "Array.fold_monoid"),
        (format!("{MONEY}let values = [Money {{ amount: 1.5m }}]\nlet total = Array.fold_monoid (ref values)\n0"), "Main.Money", "Array.fold_monoid"),
        (format!("{LANES}let values = [Lanes {{ value: Simd.splat 1.0f32 }}]\nlet total = Array.fold_monoid (ref values)\n0"), "Main.Lanes", "Array.fold_monoid"),
        ("let values = [Monoids.Sum { value: 1.5 }, Monoids.Sum { value: 2.5 }]\nlet total = Parallel.fold (ref values)\ntotal.value".to_string(), "Monoids.Sum<f64>", "Parallel.fold"),
        ("let left = Monoids.Sum { value: 1.5 }\nlet right = Monoids.Sum { value: 2.5 }\nlet both = Monoid.combine left right\nboth.value".to_string(), "Monoids.Sum<f64>", "Monoid.combine"),
        ("let values = [Monoids.MaxOf { value: Maybe.Some 1.5 }]\nlet total = Array.fold_monoid (ref values)\n0".to_string(), "Monoids.MaxOf<f64>", "Array.fold_monoid"),
    ];
    for (source, ty, call) in cases {
        let error = analyze(&source).expect_err(&source);
        assert_eq!(error.code, "E1039", "{source}\n{}", error.message);
        assert_eq!(error.message, e1039_message(ty), "{source}");
        assert_eq!(error.span.start, source.find(call).unwrap(), "{source}");
    }
}

#[test]
fn integer_monoids_and_unused_float_instances_are_accepted() {
    for source in [
        format!("{FIXED}let values = [Fixed {{ micros: 1 }}, Fixed {{ micros: 2 }}]\nlet total = Array.fold_monoid (ref values)\nlet parallel = Parallel.fold (ref values)\ntotal.micros + parallel.micros"),
        format!("{MEAN}0"),
        "let values = [Monoids.Sum { value: 1 }, Monoids.Sum { value: 2 }]\nlet total = Parallel.fold (ref values)\ntotal.value".to_string(),
        "let values = [Monoids.MaxOf { value: Maybe.Some 1 }, Monoids.MaxOf { value: Maybe.None }]\nlet total = Array.fold_monoid (ref values)\nmatch total.value with\n| Maybe.Some value -> value\n| Maybe.None -> 0".to_string(),
        "let values = [Monoids.All { value: true }, Monoids.All { value: false }]\nlet total = Parallel.fold (ref values)\nif total.value then 1 else 0".to_string(),
    ] {
        emits(&source);
    }
}

#[test]
fn monoids_are_qualified_and_the_module_names_are_reserved() {
    let error = analyze("let total = Sum { value: 1 }\ntotal.value").unwrap_err();
    assert_eq!(error.code, "E1004", "{}", error.message);
    for name in ["Monoids.tz", "Monoid.tt"] {
        let error = analyze_modules(&[("Main.tz", "0"), (name, "")]).unwrap_err();
        assert_eq!(error.code, "E1011", "{name}: {}", error.message);
        assert_eq!(
            error.message,
            format!(
                "module name '{}' is reserved for the standard library; rename the file",
                name.split('.').next().unwrap()
            ),
            "{name}"
        );
    }
}

fn run(level: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args(["test", "tests/fixtures/monoid", level])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap()
}

#[test]
fn law_tests_pass_at_o0_and_o3() {
    for level in ["-O0", "-O3"] {
        let output = run(level);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "{level}\n{stdout}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(stdout.contains("11 passed; 0 failed; 0 ignored"), "{level}\n{stdout}");
    }
}
```

実測（E1039 のないコンパイラ。`rustc --edition 2024 --test -C lto=fat` で単体にビルドして実行）: `monoids_are_qualified_and_the_module_names_are_reserved`・`integer_monoids_and_unused_float_instances_are_accepted`・`law_tests_pass_at_o0_and_o3` が `ok`、`float_monoids_are_rejected_where_they_are_used` は E1039 がないので失敗する（最初の場合が受理される）。E1039 のない状態での各ソースの `check` の結果: `OPTIONAL`・`MONEY`・`LANES` と `Mean`・`Pair` は受理（警告だけ）、`Monoids.Sum<f64>` と `Monoids.MaxOf<f64>` は `E1005 no instance for Integer<f64>`（位置は呼び出しの先頭）。型の表示は `Main.Mean`・`Main.Money`・`Monoids.Sum<f64>`・`Monoids.MaxOf<f64>`（`Display` の `E1005` の実出力で確かめた）。

## K. 文書の変更の全文（行番号は書かない。アンカーの文字列で探す）

検証: Phase 1・2 の文書を足した複製で `node scripts/check-docs.mjs <ページ>` を通した。HEAD → 変更後の数（links／examples／runs）: `parallel.md` 17／4／8 → 19／6／12、`array.md` 9／11／22 → 10／11／22、`index.md` 114／0／0 → 115／0／0、`monoid.md`（新規）10／2／4。

### K1. `parallel.md`（Phase 1・2。4 か所）

1. 「この記事のポイント」の、`reduce` の結合順がチャンク順で固定、と書いた行の直後に足す:

```text
- `fold` は、`Monoid` の法則（結合律と単位元）が成り立ち、`empty`・`combine` が出力や共有状態への書き込みなどの作用を持たなければ、`Array.fold_monoid` とビットまで同じ値を返します。浮動小数点を含む型の `Monoid` は `E1039` で拒否されます。[Monoid](monoid.md) を見てください。
```

2. 「関数の一覧」の表の `Parallel.sum` の行の直後に足す:

```text
| `Parallel.fold` | `(Monoid<'m>, Copy<'m>, Send<'m>) => ref ['m] -> 'm` | `Monoid.empty ()` と `Monoid.combine` による `reduce` |
```

3. 見出し `## Task.parallel との使い分け` の直前に、次の節を足す（`check-docs` が表の下の 2 つの例を `-O0`・`-O3` で実行し、`true` と `false` を確かめる）:

`````markdown
## 逐次と同じ結果になる条件

| API | 結果 | 条件 |
| --- | --- | --- |
| `fold` | `Array.fold_monoid` と同一（ビットまで） | `Monoid` の法則が成り立つ。`empty`・`combine` に作用がない。浮動小数点を含む型は `E1039` |
| `map`・`map_ref` | `Array.map`・`Array.map_ref` と同一 | コールバックが純粋。純粋でなければ、副作用の回数・順序・実行するスレッドは未規定 |
| `init` | `new [T](n, f)` と同一 | `f` が純粋 |
| `for_each_chunk` | チャンクを先頭から順に処理する逐次のループと同一 | `body` が純粋（自分のチャンクだけを書く） |
| `sum`（整数） | `Array.sum` と同一 | 整数の加算は折り返しても結合的 |
| `sum`（浮動小数点） | 決定的だが `Array.sum` と異なりうる | 同じ順序が要るなら `Array.sum_pairwise`・`Array.sum_kahan` |
| `reduce` | 決定的。結合律と単位元が成り立てば `fold` と同じ | 成り立たなければ `Array.fold` と異なる |

純粋なコールバックとは、外界・時計と乱数・並行・共有可変状態・GPU・動的ディスパッチ・利用者の `Drop` に届かない関数です。どの API も、複数の要素がトラップするとき、どれが報告されるかは未規定です。

整数の和は、チャンクの分け方によらず `Array.sum` と同じです。浮動小数点の和は、チャンクの境界で足す順序が変わるので、同じ列でも結果が違うことがあります。8193 要素は 3 チャンクに分かれます。

```tsuzuri run=true
let values = new [i64](8193, \index -> (index * 7919 + 13) % 1009)
Array.sum (ref values) == Parallel.sum (ref values)
```

実行結果:

```text
true
```

```tsuzuri run=false
let values = new [f64](8193, \index -> (((index * 7919 + 13) % 1009) as f64) / 10.0 + 0.1)
Array.sum (ref values) == Parallel.sum (ref values)
```

実行結果:

```text
false
```

`````

4. 「関連項目」の、`- [Array](array.md)` の行の直後に足す:

```text
- [Monoid](monoid.md) — `Parallel.fold` と `Array.fold_monoid` が返す値の同一の条件
```

### K2. `array.md`・`index.md`

- `array.md`: 表の `fold_ref` の行の直後に足す:

```text
| `fold_monoid` | `(Monoid<'m>, Copy<'m>) => ref ['m] -> 'm` | `Monoid.empty ()` から `Monoid.combine` で左畳み込みを行います。空なら `Monoid.empty ()` です。[Monoid](monoid.md) |
```

- `index.md`: `- [Parallel](./built-in-types-and-modules/parallel.md)` の行の直後に足す:

```text
- [Monoid](./built-in-types-and-modules/monoid.md)
```

### K3. `docs/language.md`（Phase 1・2）

「### データ並列 API」の表の `sum` の行の直後に、次の行を足す:

```text
| `fold` | `(Monoid<'m>, Copy<'m>, Send<'m>) => ref ['m] -> 'm` |
```

同じ節の最後の段落（`map` は各要素を値として複製してワーカーへ…で始まる段落）の直後、`### 配列・リスト API` の直前に、次を足す:

```text
#### 逐次との同一と `Monoid`

標準ライブラリの型クラス `Monoid<'m>`（`empty :: unit -> 'm` と `combine :: 'm -> 'm -> 'm`）の instance が、結合律と単位元の法則（ビットまで同じ）を満たすとき、`Parallel.fold values`（`Parallel.reduce (Monoid.empty ()) (\left right -> Monoid.combine left right) values` と同じ）は、チャンクの分け方によらず `Array.fold_monoid values`（`Monoid.empty ()` から添字の昇順に `combine` を重ねる）とビット単位で同じ値を返します。コンパイラは法則を検査しません。浮動小数点の数（`f32`・`f64`・`d32`・`d64`・`d128`、浮動小数点のレーンの SIMD。record のフィールド・union の payload・タプル・配列・リスト・`Vec`・`Maybe` の中身を辿って届くもの）を含む型への `Monoid` の制約は `E1039` で拒否されます。整数向けの型 `Sum`・`Product`・`MaxOf`・`MinOf`・`BitAnd`・`BitOr`・`BitXor`・`All`・`Any` は opt-in の `Monoids` モジュールにあり、`Monoids.Sum` のように修飾して書きます。

他の API の保証は表のとおりです（`parallel.md` の同じ表）。コールバックが純粋でなければ、副作用の回数・順序・実行するスレッドは未規定で、複数の要素がトラップするとき、どれが報告されるかも未規定です。

| API | 結果 | 条件 |
|---|---|---|
| `fold` | `Array.fold_monoid` と同一 | `Monoid` の法則と、`empty`・`combine` に作用がないこと |
| `map`・`map_ref`・`init` | `Array.map`・`Array.map_ref`・`new [T](n, f)` と同一 | コールバックが純粋 |
| `for_each_chunk` | チャンクを先頭から順に処理する逐次のループと同一 | `body` が純粋 |
| `sum`（整数） | `Array.sum` と同一 | 整数の加算は折り返しても結合的 |
| `sum`（浮動小数点） | 決定的だが `Array.sum` と異なりうる | `Array.sum_pairwise`・`Array.sum_kahan` が順序を固定する |
| `reduce` | 決定的 | 結合律と単位元が成り立てば `fold` と同じ |
```

### K4. `docs/architecture.md`（Phase 1）

1. `docs/architecture.md` の、`stdlib::OPT_IN` の opt-in std モジュールの列挙の文（「例外は `stdlib::OPT_IN` の opt-in std モジュール（…）です。」）の括弧の中で、`Channel` の後ろ（`。D-40` の前）に `Monoids` を足す:

```text
（`Arena`、`Regex`、`Unicode`、`Json`、`Cbor`、`Bench`、`Gen`、`Async`、`Matrix`、`MatrixView`、`Tensor`、`Net`、`Atomic`、`Mutex`、`Channel`、`Monoids`。D-40・D-41・D-42・D-44）
```
2. 「`RESERVED_MODULES` は 53 個」を「`RESERVED_MODULES` は 55 個」に直す（`Channel` の段落。`grep -n "RESERVED_MODULES" docs/architecture.md` で見つかる行のうち「53 個」と書いてある方。他のチケットが先に足していれば、その数に 2 を足す）。同じ段落の数の文言は、`src/stdlib.rs` のテストの数と一致させる。
3. 段落 `**行列と N 次元配列（C11）:**` の直前に、次の段落を足す:

```text
**Monoid（F15）:** `Monoid<'m>` は `std/Monoid.tt` のクラスで、常時ロードされます（`Array.tz`・`Parallel.tz` が使うので opt-in にしません）。整数向けの `Monoids.Sum`・`Monoids.MaxOf` は opt-in の `std/Monoids.tz` で、`OPT_IN` の名前は `["Monoids"]`、`RESERVED_MODULES` は 55 個です。`Array.fold_monoid` と `Parallel.fold` は通常の Tsuzuri 関数で、専用の `Builtin`・ランタイム関数・WASM の import を持ちません。`E1039` は `polymorph::ClassEnv::normalize` が、具象な `Monoid<T>` の制約で `T` が浮動小数点の数を含む（`Type::holds_float`）ときに出します。`validate_instance` ではなく `normalize` に置くのは、instance が見つかった制約が `validate_instance` を通らず、文脈の制約へ進むためです。
```

### K5. `_tsuzuri/language-reference/compiler/diagnostics.md`（Phase 1）

- 表の `E1028` の行の直後（他チケットが `E1029` 以降の行を足していれば、コードの昇順の位置）に足す:

```text
| `E1039` | `Monoid` の制約が、浮動小数点の数を含む型に対して解決された。結合的でないため、`Parallel.fold` が `Array.fold_monoid` と違う値を返しうる（kind `monoid_float_instance`） |
```
- 134 行目の文の `E1`（`E1001`〜`E1028`）の上限を、そのときの表の最大の E1 のコードに直す（`E1039` を含める）。

### K6. Phase 3 の文書

- `_tsuzuri/language-reference/compiler/usage.md` の `### check` の、`--deny-warnings` の段落の直後に足す:

```text
`--report parallel` を付けると、検査が通ったあと、標準出力に報告を出します（診断は従来どおり標準エラー出力）。報告は、`Array.map`・`Array.map_ref`・`Array.fold_monoid`・整数の `Array.sum` のうち `Parallel` の関数に置き換えられる呼び出し（提案。`--json` では `fixes` の `maybe`）と、既存の `Parallel.*` の呼び出しのコールバックが純粋かどうか（`parallel_safe`）の一覧です。提案は自動では適用されません。`--report` は `check` だけが受け付けます。`--deny-warnings` で終了コード 1 になるときは報告を出しません。
```

- `docs/language.md`: `--warn implicit-copy` の段落（「なお `--json` 指定時における警告の重要度は…」で終わる塊）の直後に、上の `usage.md` の文章と同じ文章（仕様 §7 の要約）を足す。
- `docs/architecture.md` の「ファイル | 責務」の表の `src/copies.rs` の行の直後に足す:

```text
| `src/parallel_report.rs` | `check --report parallel`（F15）の報告: `Array.map`・`Array.map_ref`・`Array.fold_monoid`・整数の `Array.sum` を `Parallel` に置き換える提案と、`Parallel.*` の呼び出しのコールバックの純粋性（`parallel_safe`）。`build`・`render_text`・`render_json`。提案は `maybe` の `fixes` で、自動では適用しない |
```

- `parallel.md` の K1 の 3 の節の末尾（`false` の例の後、`## Task.parallel との使い分け` の前）に足す:

```text
`tsuzuri check PATH --report parallel` は、`Array.map`・`Array.map_ref`・`Array.fold_monoid`・整数の `Array.sum` のうち `Parallel` に置き換えられる呼び出しを提案として一覧にします。提案だけで、書き換えは自動では行いません。
```

- 検証: `node scripts/check-docs.mjs` を、変更した `_tsuzuri` のページ（`parallel.md`・`usage.md`）に実行する。`usage.md` は変更前の数を手順で控える。

### K7. `README.md`（Phase 1 と Phase 3。`docs/language.md` と同じ変更で揃える）

「データ並列 API」の項（`- **データ並列 API**:` で始まる 1 行）の `` `Parallel.reduce`、`Parallel.sum` を使用します。`` を、Phase 1 で次に直す:

```text
`Parallel.reduce`、`Parallel.sum`、`Parallel.fold` を使用します。
```

同じ行の `…最小限の同期コストで高速に処理します。` の直後に、Phase 1 で次を足す:

```text
結合的な還元を `Monoid`（`Monoids.Sum`・`Monoids.MaxOf`）で書くと、`Parallel.fold` は逐次の `Array.fold_monoid` とビットまで同じ値を返します（浮動小数点を含む型は `E1039` で拒否されます）。
```

Phase 3 では、さらにその直後に足す:

```text
`tsuzuri check PATH --report parallel` は、`Parallel` に置き換えられる `Array` の呼び出しを提案として一覧にします（自動では書き換えません）。
```

`README.md` の標準ライブラリの表（`` `Parallel`, `Simd`, `Gpu` `` の行）は変えない。`README.md` は `_tsuzuri` のページではないので `check-docs` の対象外。`grep -n "Parallel.fold" README.md docs/language.md` で、2 つのファイルに同じ名前があることを確かめる。
