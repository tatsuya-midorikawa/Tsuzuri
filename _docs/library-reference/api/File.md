# File

## `read_bytes`

```tsuzuri
def read_bytes :: string -> IO<Result.Result<[ubyte], Os.Error>>
```

Reads a whole file. The path is the system's, so a relative one starts at the working directory.

## `read_text`

```tsuzuri
def read_text :: string -> IO<Result.Result<string, Os.Error>>
```

Reads a whole file as UTF-8. A byte order mark is kept; invalid UTF-8 is `InvalidEncoding`.

## `write_bytes`

```tsuzuri
def write_bytes :: string -> [ubyte] -> IO<Result.Result<unit, Os.Error>>
```

Creates or truncates a file and writes the bytes. A failure part way leaves a partial file.

## `write_text`

```tsuzuri
def write_text :: string -> string -> IO<Result.Result<unit, Os.Error>>
```

Creates or truncates a file and writes the text as UTF-8.

## `append_text`

```tsuzuri
def append_text :: string -> string -> IO<Result.Result<unit, Os.Error>>
```

Appends the text as UTF-8, creating the file when it is missing.

## `remove`

```tsuzuri
def remove :: string -> IO<Result.Result<unit, Os.Error>>
```

Removes a file or a symbolic link, not what a link points to.

## `Mode`

```tsuzuri
union Mode =
  | Read
  | Write
  | Append
  | CreateNew
```

How `open` treats the file: `Read` reads an existing one, `Write` creates or truncates, `Append` creates
or adds to the end, and `CreateNew` creates a file that must not exist yet.

## `Handle`

```tsuzuri
record Handle {
  id: i64
}
```

An open file. It is a plain number inside, so an action can hold it, but it must be closed with `close`
(or by `with_open`); nothing closes it for you, and the process end closes what is left. A closed
handle is never mistaken for another file: using it is `InvalidInput`.

## `open`

```tsuzuri
def open :: string -> Mode -> IO<Result.Result<Handle, Os.Error>>
```

Opens a file. A file that is already open can be opened again; each handle is separate.

## `read`

```tsuzuri
def read :: Handle -> i64 -> IO<Result.Result<[ubyte], Os.Error>>
```

Reads up to `count` bytes. An empty result means the end of the file; fewer bytes than `count`
does not. A negative count or one above 2^30 is `InvalidInput`, and so is reading a handle that was
not opened with `Read`.

## `write`

```tsuzuri
def write :: Handle -> [ubyte] -> IO<Result.Result<unit, Os.Error>>
```

Writes all the bytes. A handle opened with `Read` is `InvalidInput`.

## `flush`

```tsuzuri
def flush :: Handle -> IO<Result.Result<unit, Os.Error>>
```

Checks the handle. Tsuzuri buffers nothing, so every `write` has already reached the system.

## `close`

```tsuzuri
def close :: Handle -> IO<Result.Result<unit, Os.Error>>
```

Closes a file. The handle is dead afterwards even when the system reports a failure, so closing twice
is `InvalidInput`.

## `with_open`

```tsuzuri
def with_open :: Capture<'a> => string -> Mode -> (Handle -> IO<'a>) -> IO<Result.Result<'a, Os.Error>>
```

Opens a file, runs the action with its handle, and closes it before returning, whether or not the
action's own result is an error. The action's value is returned unless closing fails.

## `EntryKind`

```tsuzuri
union EntryKind =
  | Regular
  | Directory
  | Symlink
  | Other
```

What a path is. `Symlink` only comes from `link_metadata`; `Other` is a device, a socket, a pipe, and the like.

## `Metadata`

```tsuzuri
record Metadata {
  kind: EntryKind
  size: i64
  modified_ns: i64
}
```

What the system knows about a path: its kind, its size in bytes, and when it was last modified, in
nanoseconds since 1970-01-01 UTC (a time an `i64` cannot hold is clamped to its limit).

## `metadata`

```tsuzuri
def metadata :: string -> IO<Result.Result<Metadata, Os.Error>>
```

Describes a path, following a symbolic link to what it points at.

## `link_metadata`

```tsuzuri
def link_metadata :: string -> IO<Result.Result<Metadata, Os.Error>>
```

Describes a path itself: a symbolic link is described, not followed.

