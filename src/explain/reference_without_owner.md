# Reborrow of a reference that has no live owner

## Why
`ref (deref r)` and `deref r = ...` use the target of `r` as a place, so the borrow checker must know which live value `r` borrows. When it cannot trace `r` to one, it reports `E1013`. Earlier checks, mainly `borrow_outlives_block`, catch the usual causes first, and no known single-file program reaches this one, so the examples are `text`; the program below is reported as `borrow_outlives_block`.

## Bad
```text
def f :: unit -> i64 = \_ ->
    let r = { let x = 1; ref x }
    let s = ref (deref r)
    deref s
```

## Good
```text
def f :: unit -> i64 = \_ ->
    let x = 1
    let r = ref x
    let s = ref (deref r)
    deref s
```
Keep the owner alive in the scope where the reference is used.

## See also
- `borrow_outlives_block`
- `borrow_of_temporary`
