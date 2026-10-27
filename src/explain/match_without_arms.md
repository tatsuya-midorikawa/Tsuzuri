# Match without arms

## Why
After `match value with`, at least one arm `| pattern -> result` must follow, and every arm starts with `|` (`E0002`). An arm without the leading `|` is not read as an arm, so the match has none.

## Bad
```tsuzuri
def f :: i64 -> i64 = \x ->
    match x with
    1 -> 2
    _ -> 3
```

## Good
```tsuzuri
def f :: i64 -> i64 = \x ->
    match x with
    | 1 -> 2
    | _ -> 3
```

## See also
- `expected_pattern`
- `non_exhaustive_match`
