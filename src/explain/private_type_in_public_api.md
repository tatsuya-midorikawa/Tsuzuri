# Private type in a public signature

## Why
A public declaration cannot mention a `private` type: code in other modules could receive a value of a type that it cannot name (`E1022`). The message names the private type and the public declaration that exposes it.

## Bad
```tsuzuri
private record Secret { value: i64 }
def make :: i64 -> Secret = \n -> Secret { value: n }
```

## Good
```tsuzuri
record Secret { value: i64 }
def make :: i64 -> Secret = \n -> Secret { value: n }
```
Make the type public, or make the declaration that uses it `private` as well.

## See also
- `private_name_access`
