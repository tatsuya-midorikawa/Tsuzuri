# Implicit deep copy of an array, list or dyn value

## Why
Binding or passing an array, list or `dyn` value (or a value that contains one) and then using the original again makes a deep copy of all of it (`W1006`). The warning is off by default; `tsuzuri check --warn implicit-copy` turns it on, so the examples are `text`. The generated code is the same either way.

## Bad
```text
let a = [1, 2, 3]
let b = a
a.length + b.length

$ tsuzuri check . --warn implicit-copy
./Main.tz:2:9: warning[W1006]: implicit copy of an array allocates and copies every element; borrow it with 'ref', or call 'Array.copy' to make the copy explicit
```

## Good
```text
let a = [1, 2, 3]
let b = ref a
a.length + b.length
```
Borrow with `ref`, or write `Array.copy a` (`List.copy` for a list) when you need a copy.

## See also
- `use_after_move`
