# Options that cannot be used together

## Why
Two options of the command contradict each other, so no build can honor both (`E2000`). For example `--strip` removes the symbols that `--debug-info` asks for. The message names both options and usually the reason. The examples are `text` because the condition is in the command line.

## Bad
```text
$ tsuzuri build . --strip --debug-info
<command line>:1:1: error[E2000]: --strip cannot be combined with --debug-info: a debugger needs the symbols
```

## Good
```text
$ tsuzuri build . --debug-info
```
Keep the option that the build is for and remove the other.

## See also
- `option_requires_build`
- `duplicate_option`
