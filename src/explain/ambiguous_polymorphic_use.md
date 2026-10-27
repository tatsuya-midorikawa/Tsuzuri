# Type that nothing determines

## Why
An expression has a type that nothing in the program fixes, such as `Maybe.None` with no annotation, or a generic function whose type variables no argument determines (`E1015`). Generic code is compiled once per concrete type, so every type must be known.

## Bad
```tsuzuri
def f :: unit -> bool = \_ -> Maybe.is_none Maybe.None
```

## Good
```tsuzuri
def f :: unit -> bool = \_ ->
    let nothing: Maybe<i64> = Maybe.None
    Maybe.is_none nothing
```
Annotate the binding, or pass a value that fixes the type.

## See also
- `empty_list_needs_annotation`
- `concrete_type_required`
