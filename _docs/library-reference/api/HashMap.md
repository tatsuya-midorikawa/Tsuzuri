# HashMap

## `HashMap`

```tsuzuri
record HashMap<'key, 'value> {
  entries: Vec<Entry<'key, 'value>>
  slots: Vec<i64>
  key0: i64u
  key1: i64u
  keyed: bool
}
```

## `empty`

```tsuzuri
def empty :: HashMap<'key, 'value>
```

## `length`

```tsuzuri
def length :: ref HashMap<'key, 'value> -> i64
```

## `is_empty`

```tsuzuri
def is_empty :: ref HashMap<'key, 'value> -> bool
```

## `sip13`

```tsuzuri
def sip13 :: i64u -> i64u -> i64u -> i64u
```

SipHash-1-3 of one 64-bit word (its eight little-endian bytes) under the 128-bit key `key0`, `key1`.
A map made with `with_seed` finalizes `Hash.hash` with it, so a table position cannot be chosen
without knowing the seed.

## `with_capacity`

```tsuzuri
def with_capacity :: i64 -> HashMap<'key, 'value>
```

## `with_seed`

```tsuzuri
def with_seed :: i64u -> HashMap<'key, 'value>
```

An empty map whose hashes are keyed by `seed`, so keys chosen to share table positions under the
fixed mixing of `empty` spread out. The 128-bit key is two SplitMix64 outputs of the seed, and the
iteration order does not depend on it. Keys that collide in the whole `Hash.hash` digest still collide.

## `with_capacity_and_seed`

```tsuzuri
def with_capacity_and_seed :: i64 -> i64u -> HashMap<'key, 'value>
```

`with_seed` with room for `count` entries that needs no growth.

## `try_randomized`

```tsuzuri
def try_randomized :: unit -> IO<Result.Result<HashMap<'key, 'value>, Os.Error>>
```

An empty map keyed by a seed from the operating system (`Random.next_u64`). A failure to get one is
reported, never replaced by a fixed seed. Not available on the default wasm32 target, which has no
operating-system APIs; give `with_seed` a seed the host chose instead.

## `randomized`

```tsuzuri
def randomized :: unit -> IO<HashMap<'key, 'value>>
```

`try_randomized` that traps when the operating system gives no seed.

## `contains_key_ref`

```tsuzuri
def contains_key_ref :: (Hash<'key>, Eq<'key>) => ref HashMap<'key, 'value> -> ref 'key -> bool
```

## `contains_key`

```tsuzuri
def contains_key :: (Hash<'key>, Eq<'key>) => ref HashMap<'key, 'value> -> 'key -> bool
```

## `get_ref`

```tsuzuri
def get_ref :: (Hash<'key>, Eq<'key>, Copy<'value>) => ref HashMap<'key, 'value> -> ref 'key -> Option.Option<'value>
```

## `get`

```tsuzuri
def get :: (Hash<'key>, Eq<'key>, Copy<'value>) => ref HashMap<'key, 'value> -> 'key -> Option.Option<'value>
```

## `at_ref`

```tsuzuri
def at_ref {r s} :: (Hash<'key>, Eq<'key>) => ref {r} HashMap<'key, 'value> -> ref {s} 'key -> ref {r} 'value
```

## `at`

```tsuzuri
def at :: (Hash<'key>, Eq<'key>) => ref HashMap<'key, 'value> -> 'key -> ref 'value
```

## `insert`

```tsuzuri
def insert :: (Hash<'key>, Eq<'key>) => HashMap<'key, 'value> -> 'key -> 'value -> HashMap<'key, 'value>
```

## `remove_ref`

```tsuzuri
def remove_ref :: (Hash<'key>, Eq<'key>) => HashMap<'key, 'value> -> ref 'key -> HashMap<'key, 'value>
```

## `remove`

```tsuzuri
def remove :: (Hash<'key>, Eq<'key>) => HashMap<'key, 'value> -> 'key -> HashMap<'key, 'value>
```

## `longest_probe`

```tsuzuri
def longest_probe :: ref HashMap<'key, 'value> -> i64
```

How far the entry that sits farthest from its ideal table position is from it: 0 for an empty map.
A diagnostic for tuning and tests; a map that is flooded with colliding keys has a large value.

## `to_array`

```tsuzuri
def to_array :: (Copy<'key>, Copy<'value>) => ref HashMap<'key, 'value> -> [('key * 'value)]
```

## `keys`

```tsuzuri
def keys :: Copy<'key> => ref HashMap<'key, 'value> -> ['key]
```

## `values`

```tsuzuri
def values :: Copy<'value> => ref HashMap<'key, 'value> -> ['value]
```

## `fold`

```tsuzuri
def fold :: ('state -> ref 'key -> ref 'value -> 'state) -> 'state -> ref HashMap<'key, 'value> -> 'state
```

## `iter`

```tsuzuri
def iter :: ref HashMap<'key, 'value> -> Seq.Seq<(ref 'key * ref 'value)>
```

