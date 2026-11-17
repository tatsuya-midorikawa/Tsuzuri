# Call of a value that is not a function

## Why
Writing a value before arguments, as in `f x`, calls it, and this value is not a function but a number, a record, another plain value, or a reference to a function (`E1005`). The usual cause is a missing operator or comma between two values: Tsuzuri has no implicit multiplication, and `[1 2]` calls `1`. A reference to a function, such as a parameter declared `ref (i64 -> i64)` or a lambda parameter annotated `\(g: ref (i64 -> i64)) -> …`, cannot be called either: dereference it first, as in `(deref g) x`.

## Bad
```tsuzuri
def f :: i64 -> i64 = \x -> x (x + 1)
def apply :: ref (i64 -> i64) -> i64 -> i64 = \g x -> g x
```

## Good
```tsuzuri
def f :: i64 -> i64 = \x -> x * (x + 1)
def apply :: ref (i64 -> i64) -> i64 -> i64 = \g x -> (deref g) x
```

## See also
- `too_many_arguments_applied`
- `type_mismatch`
