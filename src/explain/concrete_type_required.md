# Concrete type required

## Why
When the compiler makes the concrete copy of a generic function, every type in it must be known, and `E1015` with this kind reports one that is still open. The type checker reports such types earlier as `ambiguous_polymorphic_use`, so no single-file program is known to reach this kind; the Bad example is `text`.

## Bad
```text
error[E1015]: a concrete type is required here; add a type annotation
```

## Good
```tsuzuri
def f :: unit -> bool = \_ ->
    let nothing: Maybe<i64> = Maybe.None
    Maybe.is_none nothing
```
Annotate the value that the message marks with a concrete type.

## See also
- `ambiguous_polymorphic_use`
