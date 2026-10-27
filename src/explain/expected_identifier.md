# Expected a name

## Why
The parser needed a name and found a number, a symbol, or a reserved word such as `new` (`E0002`). After `.` it expects a field name, so `pair.0` is rejected: a tuple element is read by pattern matching.

## Bad
```tsuzuri
def first :: (i64 * i64) -> i64 = \pair -> pair.0
```

## Good
```tsuzuri
def first :: (i64 * i64) -> i64 = \pair ->
    match pair with
    | (left, _) -> left
```
For a reserved word, choose another name, for example `create` instead of `new`.

## See also
- `expected_token`
