# Command without an input

## Why
The command needs a `.tz`, `.tt` or `.tc` file or a project directory, and none was given (`E2000`). Tsuzuri does not assume the current directory. The examples are `text` because the condition is in the command line.

## Bad
```text
$ tsuzuri run
<command line>:1:1: error[E2000]: missing .tz, .tt, or .tc input or project directory; use --help
```

## Good
```text
$ tsuzuri run .
```
Pass `.` for the project in the current directory, or the path of `Main.tz`.

## See also
- `input_extension_invalid`
- `entry_point_missing`
