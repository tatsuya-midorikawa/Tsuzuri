# Unknown value

## Why
The name is not bound in the current scope: it is misspelled, declared later in a block, private to another module, or never declared (`E1002`). A name that lives in another module is written `Module.name`.

## Bad
```tsuzuri
def f :: i64 -> i64 = \n -> n + m
```

## Good
```tsuzuri
def f :: i64 -> i64 = \n ->
    let m = 1
    n + m
```

## See also
- `unknown_module_member`
- `unknown_function`
