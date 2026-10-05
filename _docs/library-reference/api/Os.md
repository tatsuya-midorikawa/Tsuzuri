# Os

Namespace: `std`

## `ErrorKind`

```tsuzuri
union ErrorKind =
  | NotFound
  | PermissionDenied
  | AlreadyExists
  | InvalidInput
  | InvalidEncoding
  | Interrupted
  | Other
```

The kind of an operating-system failure. A case is only added with a major edition.

## `Error`

```tsuzuri
record Error {
  kind: ErrorKind
  code: i32
}
```

An operating-system failure: its kind and the system's own code (`errno`), 0 when Tsuzuri found the failure itself.

## `message`

```tsuzuri
def message :: ref Error -> string
```

Describes an error as `not found (os error 2)`; the code is left out when it is 0.

## `encode`

```tsuzuri
def encode :: ref string -> Result<utf8string, Error>
```

Encodes a path, name, or text for the system; a lone surrogate is `InvalidEncoding`.

## `error_of_status`

```tsuzuri
def error_of_status :: i64 -> Error
```

Decodes a status the runtime returned: `(kind <<< 32) | code`, with kind 1 to 7 in declaration order.

## `split_names`

```tsuzuri
def split_names :: ref [ubyte] -> Result<[string], Error>
```

Splits names that each end with a NUL byte, as `Dir.list` and `Env.args` receive them.
Any name that is not UTF-8 makes the whole call `InvalidEncoding`.

## `decode`

```tsuzuri
def decode :: [ubyte] -> Result<string, Error>
```

Decodes the bytes a system call returned as UTF-8 text. Invalid UTF-8 is `InvalidEncoding`.

## `decode_i64`

```tsuzuri
def decode_i64 :: ref [ubyte] -> i64 -> i64
```

Reads eight little-endian bytes at `offset` as a number, as the runtime lays out sizes, times, and codes.

