# Single ':' in a def signature

## Why
A `def` name and its type are separated by `::`, as in `def name :: Type` (`E0002`). A single `:` annotates a `let` binding or a record field, not a function.

## Bad
```tsuzuri
def double : i64 -> i64 = \x -> x * 2
```

## Good
```tsuzuri
def double :: i64 -> i64 = \x -> x * 2
```

## See also
- `signature_uses_def`
