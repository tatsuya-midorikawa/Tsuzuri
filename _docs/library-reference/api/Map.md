# Map

## `Map`

```tsuzuri
record Map<'key, 'value> {
  entries: Vec<Entry<'key, 'value>>
}
```

## `empty`

```tsuzuri
def empty :: Map<'key, 'value>
```

## `singleton`

```tsuzuri
def singleton :: 'key -> 'value -> Map<'key, 'value>
```

## `length`

```tsuzuri
def length :: ref Map<'key, 'value> -> i64
```

## `is_empty`

```tsuzuri
def is_empty :: ref Map<'key, 'value> -> bool
```

## `contains_key`

```tsuzuri
def contains_key :: Ord<'key> => ref Map<'key, 'value> -> 'key -> bool
```

## `get`

```tsuzuri
def get :: (Ord<'key>, Copy<'value>) => ref Map<'key, 'value> -> 'key -> Option.Option<'value>
```

## `at`

```tsuzuri
def at :: Ord<'key> => ref Map<'key, 'value> -> 'key -> ref 'value
```

## `insert`

```tsuzuri
def insert :: Ord<'key> => Map<'key, 'value> -> 'key -> 'value -> Map<'key, 'value>
```

## `remove`

```tsuzuri
def remove :: Ord<'key> => Map<'key, 'value> -> 'key -> Map<'key, 'value>
```

## `to_array`

```tsuzuri
def to_array :: (Copy<'key>, Copy<'value>) => ref Map<'key, 'value> -> [('key * 'value)]
```

## `keys`

```tsuzuri
def keys :: Copy<'key> => ref Map<'key, 'value> -> ['key]
```

## `values`

```tsuzuri
def values :: Copy<'value> => ref Map<'key, 'value> -> ['value]
```

## `fold`

```tsuzuri
def fold :: ('state -> ref 'key -> ref 'value -> 'state) -> 'state -> ref Map<'key, 'value> -> 'state
```

## `iter`

```tsuzuri
def iter :: ref Map<'key, 'value> -> Seq.Seq<(ref 'key * ref 'value)>
```

