# Random

## `Pcg`

```tsuzuri
record Pcg {
  state: i64u
  increment: i64u
}
```

A PCG-XSH-RR 64/32 generator. Make one with `pcg`; the stream is odd by construction.

## `bytes`

```tsuzuri
def bytes :: i64 -> IO<Result<[ubyte], Os.Error>>
```

Random bytes from the operating system. A negative count or one above 2^30 is `InvalidInput`.

## `next_u64`

```tsuzuri
def next_u64 :: unit -> IO<Result<i64u, Os.Error>>
```

Eight operating-system random bytes as a little-endian number.

## `pcg`

```tsuzuri
def pcg :: i64u -> i64u -> Pcg
```

A generator from a seed and a stream number (pcg32_srandom_r of pcg-c-basic).

## `pcg_next_u32`

```tsuzuri
def pcg_next_u32 :: Pcg -> (i32u * Pcg)
```

The next 32 bits and the following generator (pcg32_random_r).

## `pcg_next_u64`

```tsuzuri
def pcg_next_u64 :: Pcg -> (i64u * Pcg)
```

64 bits from two outputs, the first as the high half, and the following generator.

