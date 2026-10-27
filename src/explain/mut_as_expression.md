# Keyword 'mut' used as an expression

## Why
`mut` is not a value or an operator on its own (`E0002`). To lend a variable for mutation, write `ref mut x`; to make a binding mutable, write `let mut x = ...`.

## Bad
```tsuzuri
def bump :: ref mut i64 -> unit = \n -> deref n = deref n + 1
def f :: unit -> i64 = \_ ->
    let mut x = 1
    bump (mut x)
    x
```

## Good
```tsuzuri
def bump :: ref mut i64 -> unit = \n -> deref n = deref n + 1
def f :: unit -> i64 = \_ ->
    let mut x = 1
    bump (ref mut x)
    x
```

## See also
- `mutate_immutable_value`
