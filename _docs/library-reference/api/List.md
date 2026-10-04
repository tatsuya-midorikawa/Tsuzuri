# List

## `length`

```tsuzuri
def length :: ref [|'a|] -> i64
```

## `is_empty`

```tsuzuri
def is_empty :: ref [|'a|] -> bool
```

## `copy`

```tsuzuri
def copy :: Copy<'a> => ref [|'a|] -> [|'a|]
```

Returns a new list with a copy of every element, the explicit form of an implicit list copy.

## `fold`

```tsuzuri
def fold :: Copy<'a> => ('state -> 'a -> 'state) -> 'state -> ref [|'a|] -> 'state
```

## `iter`

```tsuzuri
def iter :: ref [|'a|] -> Seq.Seq<ref 'a>
```

