# Char literal with more than one character

## Why
Single quotes make a `char` literal, which holds exactly one UTF-16 code unit (`E0001`). Text of several characters is a string and uses double quotes.

## Bad
```tsuzuri
def greeting :: unit -> string = \_ -> 'abc'
```

## Good
```tsuzuri
def greeting :: unit -> string = \_ -> "abc"
```
Use `'a'` only for a single character.

## See also
- `type_mismatch`
