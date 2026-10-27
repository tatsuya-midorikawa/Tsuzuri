# Too many arguments

## Why
A call passes more arguments than the function takes (`E1006`). Arguments are separated by spaces, so `double double 2` passes two arguments, `double` and `2`, to `double`. Put a nested call in parentheses.

## Bad
```tsuzuri
def double :: i64 -> i64 = \x -> x * 2
def f :: unit -> i64 = \_ -> double double 2
```

## Good
```tsuzuri
def double :: i64 -> i64 = \x -> x * 2
def f :: unit -> i64 = \_ -> double (double 2)
```
`2 |> double |> double` is another way to write it.

## See also
- `call_non_function`
- `definition_parameter_count_mismatch`
