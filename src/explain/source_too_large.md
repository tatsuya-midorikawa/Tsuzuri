# Source file too large

## Why
One source file may hold at most 4,194,304 bytes (4 MiB); a larger file is rejected before it is parsed (`E0003`). The same limit applies to the files of a git dependency and to the program that a REPL session builds. The examples are `text` because a file over the limit does not fit in this page.

## Bad
```text
Main.tz      5,000,000 bytes
```

## Good
```text
Main.tz      1,200,000 bytes
Parser.tz    1,900,000 bytes
Tables.tz    1,900,000 bytes
```
Split the program into several modules, one file each.
