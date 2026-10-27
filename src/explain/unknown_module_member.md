# Unknown member of a module

## Why
`Module.name` names a function or union case that the module does not have (`E1002`). The usual causes are a misspelling and a name from another language's library (`String.len` instead of `String.length`).

## Bad
```tsuzuri
def f :: string -> i64 = \s -> String.len s
```

## Good
```tsuzuri
def f :: string -> i64 = \s -> String.length s
```
Check the module's functions with `tsuzuri doc`, or read `std/<Module>.tz`.

## See also
- `unknown_value`
