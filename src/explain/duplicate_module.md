# Module declared by more than one file

## Why
Two files of the project give the same module name: the same file name under the same namespace (`E1011`). The examples are `text` because the condition needs two files.

## Bad
```text
a/Util.tz:  namespace App
            def one :: i64 = 1
b/Util.tz:  namespace App
            def two :: i64 = 2

$ tsuzuri check .
./b/Util.tz:1:11: error[E1011]: module 'App::Util' is declared by more than one file; rename a file or change its namespace
```

## Good
```text
a/Util.tz:  namespace App
            def one :: i64 = 1
b/Util.tz:  namespace Lib
            def two :: i64 = 2
```
Change one file's namespace, or rename one of the files.

## See also
- `invalid_module_name`
- `reserved_std_namespace`
