# Local that shadows another in the same scope

## Why
A `let` binds a name that an earlier binding of the same scope already has, which hides the earlier value (`W1004`). The compiler can report this warning, but it is off by default and no command-line option turns it on (`--warn shadowing` is an unknown warning), so the examples are `text`.

## Bad
```text
def total :: i64 -> i64 = \n ->
    let value = n * 2
    let value = value + 1
    value
```

## Good
```text
def total :: i64 -> i64 = \n ->
    let doubled = n * 2
    let value = doubled + 1
    value
```
Give each binding its own name. Names that start with `_` and a binding in an inner block are not reported.

## See also
- `unused_local`
