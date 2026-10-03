# Time

## `monotonic_ns`

```tsuzuri
def monotonic_ns :: unit -> IO<Result.Result<i64, Os.Error>>
```

Nanoseconds on a clock that never goes backwards. Its start is unspecified; only differences mean anything.

## `unix_ns`

```tsuzuri
def unix_ns :: unit -> IO<Result.Result<i64, Os.Error>>
```

Nanoseconds since 1970-01-01 UTC. A time that does not fit an `i64` is `Other`.

## `sleep_ms`

```tsuzuri
def sleep_ms :: i64 -> IO<Result.Result<unit, Os.Error>>
```

Waits at least the given milliseconds; a negative count is `InvalidInput`, and 0 returns at once.

