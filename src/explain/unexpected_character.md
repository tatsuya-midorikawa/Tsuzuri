# Unexpected character

## Why
The lexer found a character that does not start any token (`E0001`). Names and operators use ASCII only, so a letter such as `é`, or a symbol such as `?`, is rejected outside string literals, char literals, and comments.

## Bad
```tsuzuri
def café :: unit -> i64 = \_ -> 1
```

## Good
```tsuzuri
def cafe :: unit -> i64 = \_ -> 1
```
Non-ASCII text is fine inside a string literal or a comment.

## See also
- `expected_token`
