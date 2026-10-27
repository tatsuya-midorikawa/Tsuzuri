# Unknown union case

## Why
`Union.Case` names a case that the union does not declare (`E1002`). Check the spelling against the declaration. The cases of `Maybe` are `Some` and `None`, and those of `Result` are `Ok` and `Error`.

## Bad
```tsuzuri
union Light = Red | Green
def f :: unit -> Light = \_ -> Light.Blue
```

## Good
```tsuzuri
union Light = Red | Green
def f :: unit -> Light = \_ -> Light.Green
```
Add the case to the union if the program needs it.

## See also
- `unknown_module_member`
- `non_exhaustive_match`
