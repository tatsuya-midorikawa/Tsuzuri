# Type class method that does not exist

## Why
`Class.method` names a method that the type class does not declare (`E1002`). A common case is `Display.to_string`: the `Display` class has `display`, which borrows the value, while `to_string` is a separate function that consumes it.

## Bad
```tsuzuri
def show :: i64 -> string = \n -> Display.to_string n
```

## Good
```tsuzuri
def show :: i64 -> string = \n -> to_string n
```
Call the function by its own name, or use a method that the class declares, such as `Display.display`.

## See also
- `unknown_module_member`
- `no_instance`
