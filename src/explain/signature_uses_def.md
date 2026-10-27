# Extra '::' after a def signature

## Why
In `def name :: Type`, the signature ends after the type, and a second `::` is rejected (`E0002`). The body follows `=` in the same declaration, or comes later as a separate implementation `fn name ...`.

## Bad
```tsuzuri
def double :: i64 -> i64 :: \x -> x * 2
```

## Good
```tsuzuri
def double :: i64 -> i64 = \x -> x * 2
```

## See also
- `signature_uses_double_colon`
- `signature_without_definition`
