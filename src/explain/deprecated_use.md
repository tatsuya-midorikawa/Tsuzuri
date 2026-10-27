# Use of a deprecated declaration

## Why
The code uses a `def`, `record` or `union` whose doc comment starts with `@deprecated` (`W1005`). Only uses from another package, including uses of the standard library, are reported, so the examples are `text`. The message repeats the declaration's advice.

## Bad
```text
legacy-lib/Old.tz:
    /// @deprecated use LegacyLib::Old.next instead
    def old :: i64 -> i64 = \x -> x + 1
    def next :: i64 -> i64 = \x -> x + 1
app/Tsuzuri.toml:
    [dependencies]
    legacy-lib = { path = "../legacy-lib" }
app/Main.tz:
    LegacyLib::Old.old 41

$ tsuzuri check app
.../app/Main.tz:1:16: warning[W1005]: 'LegacyLib::Old.old' is deprecated: use LegacyLib::Old.next instead
```

## Good
```text
app/Main.tz:
    LegacyLib::Old.next 41
```
Move to the replacement that the message names. With `--deny-warnings`, this warning fails the build.
