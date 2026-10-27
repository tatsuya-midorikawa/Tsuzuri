# Unknown type name

## Why
A type in a signature or an annotation is not a built-in type, and no record, union, type alias, or class of that name is in scope (`E1004`). Tsuzuri spells these types `i64`, `f64`, `string`, `bool`, and `Maybe<i64>`; names from other languages, such as `int`, `str`, or `Option`, do not exist.

## Bad
```tsuzuri
def size :: str -> i64 = \s -> s.length
```

## Good
```tsuzuri
def size :: string -> i64 = \s -> s.length
```
Check the spelling, or declare the type.

## See also
- `ambiguous_name`
- `unknown_type_class`
