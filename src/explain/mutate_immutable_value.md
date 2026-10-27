# Mutation of a binding that is not mutable

## Why
A `let` binding is immutable. Assigning to it, or borrowing it with `ref mut`, needs `let mut` (or an exclusive reference parameter) (`E1014`).

## Bad
```tsuzuri
def f :: unit -> i64 = \_ ->
    let x = 1
    x = 2
    x
```

## Good
```tsuzuri
def f :: unit -> i64 = \_ ->
    let mut x = 1
    x = 2
    x
```

## See also
- `borrow_conflict`
- `assign_to_immutable_place`
