# Use of 'and' outside a recursive group

## Why
`and` continues a group of mutually recursive definitions that starts with `def rec` (`E1019`). It is not the Boolean operator: Boolean "and" is `&&`, and "or" is `||`.

## Bad
```tsuzuri
def both :: bool -> bool -> bool = \a b -> a and b
```

## Good
```tsuzuri
def both :: bool -> bool -> bool = \a b -> a && b
```
To define functions that call each other, start with `def rec` and join the others with `and`.

## See also
- `missing_rec_marker`
