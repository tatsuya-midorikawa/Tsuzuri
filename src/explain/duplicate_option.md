# Option given more than once

## Why
An option that takes one setting appears twice in the command line (`E2000`). Tsuzuri does not pick one of the two for you, even when they agree. The examples are `text` because the condition is in the command line.

## Bad
```text
$ tsuzuri build . -O2 -O3
<command line>:1:1: error[E2000]: optimization specified more than once
```

## Good
```text
$ tsuzuri build . -O3
```
Keep the one setting that you want.

## See also
- `option_conflict`
- `unknown_option`
