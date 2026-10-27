# Move of an element out of an array or list

## Why
Binding `xs[i]` with `let`, or passing it where a value is taken, would move the element out of the collection and leave a hole. That is allowed only for `Copy` elements such as integers; for a `string`, an array or a record that owns one, it is rejected (`E1012`).

## Bad
```tsuzuri
def first_length :: [string] -> i64 = \names ->
    let name = names[0]
    name.length
```

## Good
```tsuzuri
def first_length :: [string] -> i64 = \names ->
    let name = ref names[0]
    name.length
```
Borrow the element with `ref`, or read what you need directly (`names[0].length`).

## See also
- `use_after_move`
- `move_out_of_reference_binding`
