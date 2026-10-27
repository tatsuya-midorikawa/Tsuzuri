# String literal not closed before the end of the file

## Why
The file ends inside a string literal: its closing `"` is missing (`E0001`). The Bad example is `text` because this kind needs a file that ends right after the string, without a line break, and a code block always ends with one; an unclosed string followed by a line break is reported as `raw_newline_in_string`.

## Bad
```text
def greeting :: unit -> string = \_ -> "hello
```

## Good
```tsuzuri
def greeting :: unit -> string = \_ -> "hello"
```
Close the string with `"`. Write a line break inside a string as `\n`.

## See also
- `raw_newline_in_string`
