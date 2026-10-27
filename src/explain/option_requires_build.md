# Option that the chosen output does not support

## Why
The option only applies to some targets or output kinds, and the command builds another one (`E2000`). For example `--strip` needs a native executable, `--wasm-opt` needs WASM output, and `run` needs the native target. The message names what the option requires. The examples are `text` because the condition is in the command line.

## Bad
```text
$ tsuzuri build . --emit object --strip
<command line>:1:1: error[E2000]: --strip requires native executable output (--emit exe)
```

## Good
```text
$ tsuzuri build . --strip
```
Drop the option, or change `--target` or `--emit` to an output that supports it.

## See also
- `option_conflict`
- `option_invalid_for_command`
