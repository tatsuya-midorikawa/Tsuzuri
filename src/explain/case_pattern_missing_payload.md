# Union case pattern without its payload

## Why
A union case declared with `of` carries a value, and a pattern for it must match that value too: `Circle r` or `Circle _`, not `Circle` alone (`E1020`).

## Bad
```tsuzuri
union Shape = Circle of i64 | Empty
def size :: Shape -> i64 = \s -> match s with | Circle -> 1 | Empty -> 0
```

## Good
```tsuzuri
union Shape = Circle of i64 | Empty
def size :: Shape -> i64 = \s -> match s with | Circle _ -> 1 | Empty -> 0
```
Write `_` when the arm does not use the payload, or a name when it does.

## See also
- `or_pattern_binding_mismatch`
- `non_exhaustive_match`
