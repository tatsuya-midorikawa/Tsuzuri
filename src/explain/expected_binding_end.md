# Expected the end of a binding

## Why
A `let` binding ends at a line break or at `;`. Here more tokens followed the bound expression on the same line (`E0002`). Common causes are two bindings on one line without `;`, and `Module::name`: a module member is reached with `.`, and `::` only joins the parts of a namespace path.

## Bad
```tsuzuri
def f :: unit -> i64 = \_ ->
    let n = String::length "abc"
    n
```

## Good
```tsuzuri
def f :: unit -> i64 = \_ ->
    let n = String.length "abc"
    n
```

## See also
- `expected_line_break`
