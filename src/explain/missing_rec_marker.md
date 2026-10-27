# Recursive function without rec

## Why
A function that calls itself, directly or through other functions, must be declared with `def rec` (`E1019`). Functions that call each other form one group: the first is `def rec` and the others follow with `and`.

## Bad
```tsuzuri
def count :: i64 -> i64 = \n -> if n == 0 then 0 else count (n - 1)
```

## Good
```tsuzuri
def rec count :: i64 -> i64 = \n -> if n == 0 then 0 else count (n - 1)
```
A local recursive function is written `let rec`.

## See also
- `and_without_recursive_group`
