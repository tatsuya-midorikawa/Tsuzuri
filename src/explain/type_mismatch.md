# Type mismatch

## Why
Every expression has one type, and the place that uses it expects another. The message names the expected type and the type that was found (`E1003`). Tsuzuri never converts between types implicitly.

## Bad
```tsuzuri
def add :: i64 -> i64 = \n -> n + true
```

## Good
```tsuzuri
def add :: i64 -> i64 = \n -> n + 1
```
Change the expression to the expected type, or convert explicitly (for example with `as`).

## See also
- `call_non_function`
- `integer_literal_out_of_range`
