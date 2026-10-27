# Unused local binding

## Why
A local binding, parameter or pattern binding is never used (`W1001`). It is a warning: the program still checks, but the value was often meant to be used. Moves, borrows, guards and closure captures count as uses.

## Bad
```tsuzuri
def area :: i64 -> i64 -> i64 = \width height ->
    let perimeter = 2 * (width + height)
    width * height
```

## Good
```tsuzuri
def area :: i64 -> i64 -> i64 = \width height -> width * height
```
Remove the binding, or start its name with `_` (`_perimeter`) to keep it on purpose.

## See also
- `unused_private_function`
