# Duplicate or reserved name

## Why
Two parameters of one function, two variables of one pattern, two fields of one record, or two top-level names have the same name, or a top-level function takes a name that a union case or a builtin already uses (`E1001`).

## Bad
```tsuzuri
def add :: i64 -> i64 -> i64 = \x -> \x -> x + x
```

## Good
```tsuzuri
def add :: i64 -> i64 -> i64 = \x -> \y -> x + y
```
Give each binding its own name.

## See also
- `duplicate_function_definition`
- `duplicate_union_case`
