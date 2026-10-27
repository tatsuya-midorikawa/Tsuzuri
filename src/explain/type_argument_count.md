# Wrong number of type arguments

## Why
A generic type needs as many type arguments as its declaration has type parameters, and a primitive type such as `i64` takes none (`E1004`). `Map` takes a key type and a value type; `Vec` takes one element type. The message says how many the type expects.

## Bad
```tsuzuri
def size :: Map<string> -> i64 = \scores -> Map.length scores
```

## Good
```tsuzuri
def size :: Map<string, i64> -> i64 = \scores -> Map.length scores
```
Write the type arguments separated by commas inside `<...>`.

## See also
- `missing_type_arguments`
- `alias_argument_count`
- `unknown_type_name`
