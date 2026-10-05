# Env

## `args`

```tsuzuri
def args :: unit -> IO<Result<[string], Os.Error>>
```

The command-line arguments after the program name. Without arguments, or when a host
did not pass them (a library or a test executable), it is empty.

## `var`

```tsuzuri
def var :: string -> IO<Result<Option<string>, Os.Error>>
```

An environment variable; an unset one is `Ok None`. A name that is empty or contains `=` or
a NUL is `InvalidInput`, and a value that is not UTF-8 is `InvalidEncoding`.

## `current_dir`

```tsuzuri
def current_dir :: unit -> IO<Result<string, Os.Error>>
```

The working directory, as the system reports it.

