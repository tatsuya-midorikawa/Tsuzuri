# Assignment to something that is not a place

## Why
Assignment replaces the value of a place: a `let mut` local or the target of a `ref mut` reference (`deref r`). Edition 2027 also accepts a field or element of one of those; edition 2026 does not. A character of a `string` and a computed member such as `.length` are never places (`E1012`).

## Bad
```tsuzuri
def greeting :: unit -> string = \_ ->
    let mut s = "abc"
    s[0] = 65
    s
```

## Good
```tsuzuri
def greeting :: unit -> string = \_ ->
    let mut s = "abc"
    s = "Abc"
    s
```
Build the new string (for example with `String.replace`) and assign it to the binding.

## See also
- `mutate_immutable_value`
- `mutate_through_shared_reference`
