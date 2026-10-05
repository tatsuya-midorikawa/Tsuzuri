# Dir

Namespace: `std`

## `list`

```tsuzuri
def list :: string -> IO<Result<[string], Os.Error>>
```

Lists the names in a directory, without "." and "..", sorted by their UTF-8 bytes.
A name that is not UTF-8 makes the whole call `InvalidEncoding`.

## `create`

```tsuzuri
def create :: string -> IO<Result<unit, Os.Error>>
```

Creates one directory; its parent must exist.

## `remove`

```tsuzuri
def remove :: string -> IO<Result<unit, Os.Error>>
```

Removes an empty directory.

## `walk`

```tsuzuri
def walk :: string -> IO<Result<[string], Os.Error>>
```

Lists everything below a directory, as paths relative to it joined with "/": each entry is followed by
what is inside it, and the entries of one directory come in the order of `list`. A symbolic link is
listed but never followed, so a loop cannot make the walk endless. A failure anywhere gives that error.

