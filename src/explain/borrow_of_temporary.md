# Borrow of a temporary value

## Why
`ref` borrows a place: a local, a field or element of one, or the target of a reference. The result of a call or an operator is a temporary that no binding owns, so there is nothing to borrow (`E1013`).

## Bad
```tsuzuri
def f :: unit -> i64 = \_ ->
    let r = ref (1 + 2)
    deref r
```

## Good
```tsuzuri
def f :: unit -> i64 = \_ ->
    let sum = 1 + 2
    let r = ref sum
    deref r
```
Bind the value with `let` first, then borrow the binding.

## See also
- `borrow_outlives_block`
- `reference_without_owner`
