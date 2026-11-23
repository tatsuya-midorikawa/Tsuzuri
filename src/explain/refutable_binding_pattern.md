# Refutable pattern in a single-pattern binding

## Why
A single-pattern binding (`let <pattern> = ...`, or in edition 2027 a lambda parameter, `for...in`, or computation binding) must cover every value of the bound type (`E1021`). The message lists a missing case; a `when` guard may also fail at runtime.

## Bad
```tsuzuri
let maybe = Maybe.Some 1
let Maybe.Some x = maybe
x
```

## Good
```tsuzuri
let maybe = Maybe.Some 1
match maybe with
| Maybe.Some x -> x
| Maybe.None -> 0
```

## See also
- `non_exhaustive_match`
- `refutable_binding_pattern_warning`
