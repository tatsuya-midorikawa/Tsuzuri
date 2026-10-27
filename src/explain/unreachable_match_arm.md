# Match arm that can never be reached

## Why
Earlier arms without a guard already cover every value that this arm matches, so it never runs (`W1003`). It is a warning: the program still checks. A wildcard `_` placed before other arms is the usual cause.

## Bad
```tsuzuri
union Light = Red | Green
def f :: Light -> i64 = \l -> match l with | Red -> 1 | _ -> 2 | Green -> 3
```

## Good
```tsuzuri
union Light = Red | Green
def f :: Light -> i64 = \l -> match l with | Red -> 1 | Green -> 3
```
Delete the arm, or narrow the earlier pattern so that the arm can match.

## See also
- `non_exhaustive_match`
