# Or-pattern whose sides bind different names

## Why
An arm `| A x | B y -> body` runs the same body whichever side matched, so every side of an or-pattern must bind the same names (`E1020`). Here one side binds `r` and the other binds `side`.

## Bad
```tsuzuri
union Shape = Circle of i64 | Square of i64
def size :: Shape -> i64 = \s -> match s with | Circle r | Square side -> 1
```

## Good
```tsuzuri
union Shape = Circle of i64 | Square of i64
def size :: Shape -> i64 = \s -> match s with | Circle n | Square n -> n
```
Use one name on every side, or split the alternatives into separate arms.

## See also
- `case_pattern_missing_payload`
- `non_exhaustive_match`
