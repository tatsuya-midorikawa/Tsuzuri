# Mutation of a binding that is not mutable

## Why
A `let` binding is immutable. Assigning to it, or borrowing it with `ref mut`, needs `let mut` (or an exclusive reference parameter) (`E1014`). In edition 2027 and later, a record field rooted at a `let mut` owner or `ref mut` parameter may also be borrowed with `ref mut b.f`, while array/list elements, union payloads, and tuple components cannot be borrowed exclusively.

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
