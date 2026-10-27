# Expected a specific token

## Why
The parser reached a token that cannot continue the construct it is reading. The message names what it needed, such as `'with' after the matched expression` or `the closing delimiter` (`E0002`). Common causes are a missing keyword or bracket, and syntax from another language, such as `=>` in a match arm or `{` after `for`.

## Bad
```tsuzuri
def f :: i64 -> i64 = \x -> match x | 1 -> 2 | _ -> 3
```

## Good
```tsuzuri
def f :: i64 -> i64 = \x -> match x with | 1 -> 2 | _ -> 3
```
Write the token that the message names at the marked position.

## See also
- `expected_expression`
- `expected_identifier`
