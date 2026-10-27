# Mutation through a shared reference

## Why
A `ref` reference only reads its target. Assigning through it (`deref r = ...`) or borrowing its target again with `ref mut` needs an exclusive `ref mut` reference (`E1014`).

## Bad
```tsuzuri
def bump :: ref i64 -> unit = \n -> deref n = deref n + 1
```

## Good
```tsuzuri
def bump :: ref mut i64 -> unit = \n -> deref n = deref n + 1
```
The caller then lends a mutable local with `ref mut`, as in `bump (ref mut count)`.

## See also
- `mutate_immutable_value`
- `borrow_conflict`
- `assign_to_immutable_place`
