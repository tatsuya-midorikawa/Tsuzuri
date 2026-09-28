# List

## `length`

```tsuzuri
def length :: ref [|'a|] -> i64
```

## `is_empty`

```tsuzuri
def is_empty :: ref [|'a|] -> bool
```

## `fold`

```tsuzuri
def fold :: Copy<'a> => ref [|'a|] -> 'state -> ('state -> 'a -> 'state) -> 'state
```

## `iter`

```tsuzuri
def iter :: ref [|'a|] -> Seq.Seq<ref 'a>
```

