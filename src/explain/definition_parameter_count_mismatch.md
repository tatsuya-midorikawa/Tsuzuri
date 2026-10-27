# Implementation and signature disagree on parameters

## Why
An implementation names more parameters than its `def` signature declares, or none for a signature that takes some (`E1006`). Each `->` in the signature adds one parameter: `i64 -> i64 -> i64` takes two.

## Bad
```tsuzuri
def add :: i64 -> i64
fn add a b = a + b
```

## Good
```tsuzuri
def add :: i64 -> i64 -> i64
fn add a b = a + b
```
Fix the signature, or the parameter list of the implementation.

## See also
- `too_many_arguments_applied`
- `let_function_needs_lambda`
