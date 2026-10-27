# Unknown function in a generated call

## Why
Some calls are written by the compiler, such as the operations of a computation builder (`Builder.Bind`) and the std helpers that `derive` uses. `E1002` with this kind means that such a call names a function that cannot be found. A misspelled name that you write is `unknown_value` or `unknown_module_member`, and a builder without the operation is `builder_missing_operation`, so no single-file program is known to reach this kind; the examples are `text`.

## Bad
```text
error[E1002]: unknown function 'Parser.Bind'
```

## Good
```text
// Parser.tc: every operation that a block uses is a public function
def Bind :: 'a -> ('a -> 'b) -> 'b
fn Bind value next = next value
```
If the error stays, report the program as a compiler issue.

## See also
- `builder_missing_operation`
- `unknown_value`
