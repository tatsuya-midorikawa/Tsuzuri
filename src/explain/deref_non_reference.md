# Dereference of a value that is not a reference

## Why
`deref x` reads through a reference, so `x` must have a reference type such as `ref i64` or `ref mut i64` (`E1005`). A parameter declared as `i64` is the value itself, so use it directly.

## Bad
```tsuzuri
def next :: i64 -> i64 = \n -> deref n + 1
```

## Good
```tsuzuri
def next :: i64 -> i64 = \n -> n + 1
```
If the caller passes a borrow, declare the parameter as `ref i64` and keep the `deref`.

## See also
- `type_mismatch`
- `mutate_through_shared_reference`
