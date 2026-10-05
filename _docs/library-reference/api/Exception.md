# Exception

Namespace: `std`

## `ExceptionKind`

```tsuzuri
union ExceptionKind =
  | OverflowException
```

The kind of an exception that `try ... with` catches. `e is OverflowException`
in a handler arm tests it.

## `Exception`

```tsuzuri
record Exception {
  kind: ExceptionKind
  msg: string
}
```

An exception that a `try ... with` handler receives: its kind and message.

