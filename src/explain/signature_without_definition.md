# Signature without an implementation

## Why
A `def name :: Type` line without `= body` declares only the signature. An implementation, `fn name ...` or a top-level `let name = \x -> ...`, must appear in the same file (`E0002`).

## Bad
```tsuzuri
def twice :: i64 -> i64
```

## Good
```tsuzuri
def twice :: i64 -> i64
fn twice x = x * 2
```
Or give the body in the declaration: `def twice :: i64 -> i64 = \x -> x * 2`.

## See also
- `definition_without_signature`
