# Invalid escape in a string

## Why
Inside a string literal, `\` starts an escape (`E0001`). The escapes are `\"`, `\\`, `\n`, `\r`, `\t`, `\0`, and `\u` followed by four hex digits or by `{...}`; any other character after `\` is rejected. A Windows path is the usual case.

## Bad
```tsuzuri
def folder :: unit -> string = \_ -> "C:\docs"
```

## Good
```tsuzuri
def folder :: unit -> string = \_ -> "C:\\docs"
```
Write a literal backslash as `\\`.

## See also
- `invalid_unicode_scalar_escape`
- `raw_newline_in_string`
