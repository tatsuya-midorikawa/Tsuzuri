# Match that does not cover every case

## Why
A `match` must handle every value of the matched type. The message lists the cases that no arm covers (`E1021`). A wildcard arm `| _ -> ...` covers the rest, but it also hides cases that are added later.

## Bad
```tsuzuri
union Light = Red | Green
def f :: Light -> i64 = \l -> match l with | Red -> 1
```

## Good
```tsuzuri
union Light = Red | Green
def f :: Light -> i64 = \l -> match l with | Red -> 1 | Green -> 2
```

## See also
- `unreachable_match_arm`
