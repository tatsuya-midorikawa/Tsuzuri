# Expected an expression

## Why
The parser needed a value, for example after an operator, `=`, `->`, or an opening bracket, and found a token that cannot start one (`E0002`). Common causes are a missing operand, compound assignment such as `+=`, and `let ... in`, which Tsuzuri does not have.

## Bad
```tsuzuri
def f :: i64 -> i64 = \n ->
    let mut x = n
    x += 1
    x
```

## Good
```tsuzuri
def f :: i64 -> i64 = \n ->
    let mut x = n
    x = x + 1
    x
```
There is no compound assignment: write `x = x + 1`.

## See also
- `expected_token`
