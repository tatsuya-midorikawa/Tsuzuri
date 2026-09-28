# Set

## `Set`

```tsuzuri
record Set<'key> {
  entries: Vec<'key>
}
```

## `empty`

```tsuzuri
def empty :: Set<'key>
```

## `singleton`

```tsuzuri
def singleton :: 'key -> Set<'key>
```

## `length`

```tsuzuri
def length :: ref Set<'key> -> i64
```

## `is_empty`

```tsuzuri
def is_empty :: ref Set<'key> -> bool
```

## `contains`

```tsuzuri
def contains :: Ord<'key> => ref Set<'key> -> 'key -> bool
```

## `insert`

```tsuzuri
def insert :: Ord<'key> => Set<'key> -> 'key -> Set<'key>
```

## `remove`

```tsuzuri
def remove :: Ord<'key> => Set<'key> -> 'key -> Set<'key>
```

## `to_array`

```tsuzuri
def to_array :: Copy<'key> => ref Set<'key> -> ['key]
```

## `fold`

```tsuzuri
def fold :: ref Set<'key> -> 'state -> ('state -> ref 'key -> 'state) -> 'state
```

## `union`

```tsuzuri
def union :: Ord<'key> => Set<'key> -> Set<'key> -> Set<'key>
```

## `intersect`

```tsuzuri
def intersect :: (Ord<'key>, Copy<'key>) => ref Set<'key> -> ref Set<'key> -> Set<'key>
```

## `difference`

```tsuzuri
def difference :: (Ord<'key>, Copy<'key>) => ref Set<'key> -> ref Set<'key> -> Set<'key>
```

## `iter`

```tsuzuri
def iter :: ref Set<'key> -> Seq.Seq<ref 'key>
```

