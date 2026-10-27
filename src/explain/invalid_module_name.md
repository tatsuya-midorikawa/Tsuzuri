# File name that is not a valid module name

## Why
Each `.tz`, `.tt` or `.tc` file is a module named after its file name, so the name must be an identifier that starts with an uppercase ASCII letter and is not `_`, `Task` or a keyword (`E1011`). The examples are `text` because the condition needs a second file.

## Bad
```text
utils.tz:  def twice :: i64 -> i64 = \n -> n * 2
Main.tz:   utils.twice 21

$ tsuzuri check .
./utils.tz:1:1: error[E1011]: module name 'utils' must start with an uppercase ASCII letter; rename the file, for example to 'Utils'
```

## Good
```text
Utils.tz:  def twice :: i64 -> i64 = \n -> n * 2
Main.tz:   Utils.twice 21
```
Rename the file and use the new name where the module is called.

## See also
- `duplicate_module`
- `reserved_std_namespace`
