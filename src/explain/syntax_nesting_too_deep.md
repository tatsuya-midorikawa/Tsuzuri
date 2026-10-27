# Syntax nested too deeply

## Why
Expressions, patterns, and types may nest at most 128 levels: brackets, blocks, lambdas, and similar constructs inside each other (`E0002`). The limit keeps the parser's recursion bounded; such depth usually comes from generated code.

## Bad
```tsuzuri
def f :: unit -> i64 = \_ ->
    ((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((
    ((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((
    1
    ))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))
    ))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))
```

## Good
```tsuzuri
def f :: unit -> i64 = \_ -> 1
```
Name intermediate results with `let`, or split the expression into functions.

## See also
- `expression_nesting_too_deep`
- `pattern_nesting_too_deep`
