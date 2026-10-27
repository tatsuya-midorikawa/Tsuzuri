# Function implemented twice

## Why
A function has more than one implementation in a file: two `fn name ...` lines, or a `def name :: Type = body` and a later `fn name ...` (`E1001`). Tsuzuri has no overloading; one function has one body.

## Bad
```tsuzuri
def scale :: i64 -> i64
fn scale x = x * 2
fn scale x = x * 3
```

## Good
```tsuzuri
def scale :: i64 -> i64
fn scale x = x * 2
def triple :: i64 -> i64
fn triple x = x * 3
```
Rename one of them, or merge the cases into one body with `match`.

## See also
- `duplicate_function_signature`
- `duplicate_name`
