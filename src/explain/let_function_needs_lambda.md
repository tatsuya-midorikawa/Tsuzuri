# Let implementation that is not a lambda

## Why
When a top-level `let` implements a declared `def` signature, its body must be a lambda that names the parameters, such as `\x -> \y -> x + y` (`E0002`). A partial application or the name of another function is not accepted there. The same rule applies to a `let` method implementation in a class or an instance.

## Bad
```tsuzuri
def times :: i64 -> i64 -> i64 = \a -> \b -> a * b
def double :: i64 -> i64
let double = times 2
```

## Good
```tsuzuri
def times :: i64 -> i64 -> i64 = \a -> \b -> a * b
def double :: i64 -> i64
let double = \x -> times 2 x
```

## See also
- `definition_parameter_count_mismatch`
