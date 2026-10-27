# Empty array or list without an element type

## Why
An empty literal, `[]` for an array or `[||]` for a list, has no element to infer the element type from, so the type must be known where the literal appears; a later use does not fix it (`E1004`). `[i64]` is an array of `i64`, and `[|i64|]` is a list of `i64`.

## Bad
```tsuzuri
def count :: unit -> i64 = \_ ->
    let xs = []
    xs.length
```

## Good
```tsuzuri
def count :: unit -> i64 = \_ ->
    let xs: [i64] = []
    xs.length
```
A literal passed straight to a typed parameter, or returned from a function, takes its type from there and needs no annotation.

## See also
- `type_mismatch`
