# Unknown command-line option

## Why
The command does not have an option with this spelling (`E2000`). The cause is usually a misspelling or an option from another tool, such as `--release` from Cargo. The examples are `text` because the condition is in the command line.

## Bad
```text
$ tsuzuri build . --release
<command line>:1:1: error[E2000]: unknown option '--release'; use --help
```

## Good
```text
$ tsuzuri build . -O3
```
`tsuzuri --help` lists the commands and their options.

## See also
- `option_invalid_for_command`
- `duplicate_option`
