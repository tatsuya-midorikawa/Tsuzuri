# Expected a pattern

## Why
Each `match` arm starts with `|` and a pattern: a literal, a name, `_`, a tuple, a record, or a union case. The parser found something else, such as a comparison operator (`E0002`). A condition goes in a `when` guard after the pattern.

## Bad
```tsuzuri
def sign :: i64 -> i64 = \x ->
    match x with
    | < 0 -> -1
    | _ -> 1
```

## Good
```tsuzuri
def sign :: i64 -> i64 = \x ->
    match x with
    | n when n < 0 -> -1
    | _ -> 1
```

## See also
- `match_without_arms`
- `non_exhaustive_match`
