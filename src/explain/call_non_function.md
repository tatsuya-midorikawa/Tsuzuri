# Call of a value that is not a function

## Why
Writing a value before arguments, as in `f x`, calls it, and this value is not a function but a number, a record, or another plain value (`E1005`). The usual cause is a missing operator or comma between two values: Tsuzuri has no implicit multiplication, and `[1 2]` calls `1`.

## Bad
```tsuzuri
def f :: i64 -> i64 = \x -> x (x + 1)
```

## Good
```tsuzuri
def f :: i64 -> i64 = \x -> x * (x + 1)
```

## See also
- `too_many_arguments_applied`
- `type_mismatch`
