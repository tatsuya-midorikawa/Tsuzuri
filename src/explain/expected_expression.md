# Expected an expression

## Why
The parser needed a value, for example after an operator, `=`, `->`, or an opening bracket, and found a token that cannot start one (`E0002`). Common causes are a missing operand, unsupported compound assignment such as `**=` (only `+=`, `-=`, `*=`, `/=`, and `%=` exist), and `let ... in`, which Tsuzuri does not have.

## Bad
```tsuzuri
def f :: i64 -> i64 = \n ->
    let mut x = n
    x **= 2
    x
```

## Good
```tsuzuri
def f :: i64 -> i64 = \n ->
    let mut x = n
    x = x ** 2
    x
```
Only `+`, `-`, `*`, `/`, and `%` have a compound assignment form: write `x = x ** 2`.

## See also
- `expected_token`
