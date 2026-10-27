# Implementation without a signature

## Why
An implementation `fn name ...` needs a `def name :: Type` signature in the same file (`E0002`). The type of a top-level function is never inferred. For `main`, the message lists the entry-point signatures.

## Bad
```tsuzuri
fn twice x = x * 2
```

## Good
```tsuzuri
def twice :: i64 -> i64
fn twice x = x * 2
```
Or write the whole function in one declaration: `def twice :: i64 -> i64 = \x -> x * 2`.

## See also
- `signature_without_definition`
