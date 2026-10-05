# HashSet

## `HashSet`

```tsuzuri
record HashSet<'key> {
  map: HashMap<'key, unit>
}
```

## `empty`

```tsuzuri
def empty :: HashSet<'key>
```

## `with_capacity`

```tsuzuri
def with_capacity :: i64 -> HashSet<'key>
```

## `length`

```tsuzuri
def length :: ref HashSet<'key> -> i64
```

## `is_empty`

```tsuzuri
def is_empty :: ref HashSet<'key> -> bool
```

## `with_seed`

```tsuzuri
def with_seed :: i64u -> HashSet<'key>
```

An empty set whose hashes are keyed by `seed`; see `HashMap.with_seed`.

## `with_capacity_and_seed`

```tsuzuri
def with_capacity_and_seed :: i64 -> i64u -> HashSet<'key>
```

## `try_randomized`

```tsuzuri
def try_randomized :: unit -> IO<Result<HashSet<'key>, Os.Error>>
```

An empty set keyed by a seed from the operating system; see `HashMap.try_randomized`.

## `randomized`

```tsuzuri
def randomized :: unit -> IO<HashSet<'key>>
```

## `insert`

```tsuzuri
def insert :: (Hash<'key>, Eq<'key>) => HashSet<'key> -> 'key -> HashSet<'key>
```

## `remove_ref`

```tsuzuri
def remove_ref :: (Hash<'key>, Eq<'key>) => HashSet<'key> -> ref 'key -> HashSet<'key>
```

## `remove`

```tsuzuri
def remove :: (Hash<'key>, Eq<'key>) => HashSet<'key> -> 'key -> HashSet<'key>
```

## `contains_ref`

```tsuzuri
def contains_ref :: (Hash<'key>, Eq<'key>) => ref HashSet<'key> -> ref 'key -> bool
```

## `contains`

```tsuzuri
def contains :: (Hash<'key>, Eq<'key>) => ref HashSet<'key> -> 'key -> bool
```

## `longest_probe`

```tsuzuri
def longest_probe :: ref HashSet<'key> -> i64
```

The longest probe of the underlying table; see `HashMap.longest_probe`.

## `to_array`

```tsuzuri
def to_array :: Copy<'key> => ref HashSet<'key> -> ['key]
```

## `fold`

```tsuzuri
def fold :: ('state -> ref 'key -> 'state) -> 'state -> ref HashSet<'key> -> 'state
```

## `iter`

```tsuzuri
def iter :: ref HashSet<'key> -> Seq<ref 'key>
```

