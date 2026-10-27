# Type that would have to contain itself

## Why
Inference needs a type that contains itself, such as the type of a function that is passed to itself, and no finite type does (`E1015`). The usual cause is a value applied to itself, often through missing parentheses: `f f x` applies `f` to `f`, while `f (f x)` applies `f` twice.

## Bad
```tsuzuri
def twice :: i64 -> i64 = \n ->
    let apply = \f x -> f f x
    apply (\x -> x + 1) n
```

## Good
```tsuzuri
def twice :: i64 -> i64 = \n ->
    let apply = \f x -> f (f x)
    apply (\x -> x + 1) n
```
Add the parentheses that the call needs, or give the lambda's parameters types.

## See also
- `type_mismatch`
