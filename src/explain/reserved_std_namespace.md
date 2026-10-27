# Module in the reserved std namespace

## Why
The namespace `std` and the names of the standard modules belong to the standard library (`E1011`). A `namespace std` header, or a file named after a standard module such as `String.tz`, is rejected.

## Bad
```tsuzuri
namespace std

def twice :: i64 -> i64 = \n -> n * 2
```

## Good
```tsuzuri
namespace App

def twice :: i64 -> i64 = \n -> n * 2
```
Choose your own namespace, and rename a file that has a standard module's name.

## See also
- `invalid_module_name`
- `duplicate_module`
