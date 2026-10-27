# Input file with an unsupported extension

## Why
Source files must end in `.tz` (code), `.tt` (type classes) or `.tc` (computation builders) (`E2000`). Old `.tzr` code files and files with other extensions are not read. The examples are `text` because the condition is in the file name.

## Bad
```text
$ tsuzuri run Main.tzr
Main.tzr:1:1: error[E2000]: input files must use '.tz' (code), '.tt' (type classes), or '.tc' (computation builder); rename old '.tzr' code files to '.tz'
```

## Good
```text
$ mv Main.tzr Main.tz
$ tsuzuri run Main.tz
```
Rename the file; `tsuzuri fmt` accepts the same three extensions.

## See also
- `missing_input`
