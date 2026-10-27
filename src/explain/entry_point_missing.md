# Program without an entry point

## Why
An executable starts from top-level code or `def main` in `Main.tz`, and this program has neither, or the input is not `Main.tz` (`E2004`). `tsuzuri check` accepts such a library, so the examples are `text`: the error appears at `build` and `run`.

## Bad
```text
Main.tz:
    def add :: i64 -> i64 -> i64 = \a b -> a + b

$ tsuzuri run .
./Main.tz:1:1: error[E2004]: an executable requires top-level entry-point code or 'def main' in Main.tz; use '--emit object' for a library
```

## Good
```text
Main.tz:
    def add :: i64 -> i64 -> i64 = \a b -> a + b
    add 40 2

$ tsuzuri run .
```
For a library, build an object instead: `tsuzuri build . --emit object`. A shared library and bindings need at least one `export def`.

## See also
- `missing_input`
