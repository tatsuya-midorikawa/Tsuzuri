# BigInt

## `BigInt`

```tsuzuri
record BigInt {
  negative: bool
  limbs: [i64]
}
```

An integer of any size, written with the `I` suffix (`12345678901234567890I`)
or as an integer literal where a `bigint` is expected. It keeps a sign and
base-10^9 digits, least significant first; zero has no digits and no sign.

## `of_i64`

```tsuzuri
def of_i64 :: i64 -> BigInt
```

The bigint of `value`.

## `to_i64`

```tsuzuri
def to_i64 :: ref BigInt -> Option<i64>
```

The `i64` that `value` holds, or `None` when it is out of range.

## `of_string`

```tsuzuri
def of_string :: ref string -> Option<BigInt>
```

The bigint that `text` spells: an optional `-` and decimal digits.

## `compare`

```tsuzuri
def compare :: ref BigInt -> ref BigInt -> i64
```

-1, 0, or 1 as `left` is below, equal to, or above `right`.

