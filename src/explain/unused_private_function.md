# Unused private function

## Why
A `private` function that no public declaration of its module reaches can never run (`W1002`). It is a warning: the program still checks.

## Bad
```tsuzuri
private def square :: i64 -> i64 = \n -> n * n
def area :: i64 -> i64 = \side -> side * side
```

## Good
```tsuzuri
private def square :: i64 -> i64 = \n -> n * n
def area :: i64 -> i64 = \side -> square side
```
Call the function, delete it, or remove `private` if other modules should use it.

## See also
- `unused_private_type`
- `unused_local`
