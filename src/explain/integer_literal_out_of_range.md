# Integer literal out of range

## Why
An integer literal does not fit the type it gets from its suffix or from the place that uses it (`E1009`). For example, `i8` holds -128 to 127, and an unsigned type holds no negative value. The compiler never wraps a literal.

## Bad
```tsuzuri
def f :: unit -> i8 = \_ -> 200
```

## Good
```tsuzuri
def f :: unit -> i16 = \_ -> 200
```
Use a wider type, or a value that fits.

## See also
- `type_mismatch`
- `invalid_numeric_literal`
