# Borrow that outlives its owner's block

## Why
A `ref` borrow cannot leave the block that owns the borrowed value: the value is dropped at the end of the block, and the borrow would point at nothing (`E1013`).

## Bad
```tsuzuri
def f :: unit -> i64 = \_ ->
    let r = { let x = 1; ref x }
    deref r
```

## Good
```tsuzuri
def f :: unit -> i64 = \_ ->
    let x = 1
    let r = ref x
    deref r
```
Move the owner out of the inner block, or return the value instead of a borrow.

## See also
- `borrow_of_temporary`
- `reference_without_owner`
