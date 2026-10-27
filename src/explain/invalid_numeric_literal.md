# Invalid numeric literal or suffix

## Why
A number is followed by letters that are not a type suffix (`E0001`). A suffix is a numeric type name (`i8` to `i128u`, `f16` to `f128`, `d32` to `d128`, `byte`, `sbyte`, `ubyte`), a short form (`y`, `uy`, `s`, `us`, `u`, `l`, `ul`, `L`, `UL`, `hf`, `f`, `F`, `hm`, `m`, `M`), or `I` for a bigint. Unsigned type names end in `u`, so Rust's `u8` is not a suffix.

## Bad
```tsuzuri
def limit :: unit -> i8u = \_ -> 255u8
```

## Good
```tsuzuri
def limit :: unit -> i8u = \_ -> 255i8u
```
`255uy` is the short form of the same literal.

## See also
- `numeric_separator_misplaced`
- `integer_literal_out_of_range`
