# Expected a line break after a header

## Why
A `namespace` or `using` path, and the name after `@alias` in a builder file, must be the last thing on its line. Something else followed on the same line (`E0002`).

## Bad
```tsuzuri
namespace Demo def answer :: unit -> i64 = \_ -> 42
```

## Good
```tsuzuri
namespace Demo

def answer :: unit -> i64 = \_ -> 42
```
Start the next declaration on a line of its own.

## See also
- `header_path_on_same_line`
- `header_path_spacing`
